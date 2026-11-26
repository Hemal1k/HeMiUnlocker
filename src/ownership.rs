//! "Take ownership" (from the File Explorer submenu or the window): the user becomes the owner of a file or folder
//! (with everything in it) and the Administrators get full control – what the well-known "Take Ownership" registry
//! tweak does with takeown and icacls, here by the security functions of Windows itself (no hidden command lines: the
//! machine learning of Defender took hidden takeown / icacls for malware). It needs administrator rights: HeMiUnlocker
//! starts again as administrator (Windows asks). A notification tells the result.
//!
//! Since the audit of 2026-11-24: it asks first, refuses the places whose owner Windows needs (a drive, the Windows
//! folder, WindowsApps, the top folders of Program Files, ProgramData and the user folders), and writes the owner and
//! the permissions of every item down before changing them – "Undo" in the window puts them back.

use crate::i18n::{count, os_error, tr};
use crate::procs::wide;
use crate::unlock::{name_of, relaunch_elevated};
use std::path::{Path, PathBuf};
use std::ptr::{null, null_mut};
use windows_sys::Win32::Foundation::{CloseHandle, ERROR_NOT_ALL_ASSIGNED, ERROR_SUCCESS, GetLastError, HANDLE, LUID, LocalFree};
use windows_sys::Win32::Security::Authorization::{
    ConvertSecurityDescriptorToStringSecurityDescriptorW, ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
    ConvertStringSidToSidW, EXPLICIT_ACCESS_W, GRANT_ACCESS, GetNamedSecurityInfoW, NO_MULTIPLE_TRUSTEE, SE_FILE_OBJECT, SetEntriesInAclW,
    SetNamedSecurityInfoW, TRUSTEE_IS_GROUP, TRUSTEE_IS_SID, TRUSTEE_W,
};
use windows_sys::Win32::Security::{
    ACL, AdjustTokenPrivileges, DACL_SECURITY_INFORMATION, GetSecurityDescriptorControl, GetSecurityDescriptorDacl,
    GetSecurityDescriptorOwner, PROTECTED_DACL_SECURITY_INFORMATION, SE_DACL_PROTECTED, UNPROTECTED_DACL_SECURITY_INFORMATION,
    GetTokenInformation, LUID_AND_ATTRIBUTES, LookupAccountSidW, LookupPrivilegeValueW, NO_INHERITANCE,
    OWNER_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR, PSID, SE_PRIVILEGE_ENABLED, TOKEN_ADJUST_PRIVILEGES, TOKEN_PRIVILEGES, TOKEN_QUERY,
    TOKEN_USER, TokenUser,
};
use windows_sys::Win32::Storage::FileSystem::FILE_ALL_ACCESS;
use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

pub const SWITCH: &str = "--take-ownership";
/// Puts back the owners and permissions written down by a take ownership (the file of that list follows).
pub const RESTORE_SWITCH: &str = "--restore-ownership";

/// The notification of Windows, also written to the log (when logging is on).
fn toast(title: &str, text: &str) -> bool {
    crate::log::write(&format!("Ownership: {title} – {text}"));
    crate::unlock::toast(title, text)
}

/// The text of the last error of Windows on this thread.
fn last_error() -> String {
    std::io::Error::last_os_error().to_string()
}

/// The user of this process and the Administrators group, and the privileges to set the owner of anything.
struct Who {
    /// The TOKEN_USER, in u64 so that it is aligned as the struct (it holds a pointer) needs.
    user: Vec<u64>,
    admins: PSID,
}

impl Who {
    fn new() -> Result<Who, String> {
        // SAFETY: the token is closed on every path once opened; the buffers passed with their sizes in bytes; the
        // SID of the Administrators is freed by `Drop`
        unsafe {
            let mut token: HANDLE = null_mut();
            if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY | TOKEN_ADJUST_PRIVILEGES, &mut token) == 0 {
                return Err(last_error());
            }
            // the owner may be set to anyone (restore) and taken from anyone (take ownership) – administrators have both
            let mut missing = None;
            for name in ["SeTakeOwnershipPrivilege", "SeRestorePrivilege"] {
                let mut luid: LUID = std::mem::zeroed();
                let enabled = LookupPrivilegeValueW(null(), wide(name).as_ptr(), &mut luid) != 0 && {
                    let tp = TOKEN_PRIVILEGES { PrivilegeCount: 1, Privileges: [LUID_AND_ATTRIBUTES { Luid: luid, Attributes: SE_PRIVILEGE_ENABLED }] };
                    // success with ERROR_NOT_ALL_ASSIGNED: the token does not have it
                    AdjustTokenPrivileges(token, 0, &tp, 0, null_mut(), null_mut()) != 0 && GetLastError() != ERROR_NOT_ALL_ASSIGNED
                };
                if !enabled {
                    missing.get_or_insert(name);
                }
            }
            if missing.is_some() {
                CloseHandle(token);
                return Err(tr(
                    "Windows did not give the rights to change owners – HeMiUnlocker has to run as administrator.",
                    "Windows nedal oprávnění měnit vlastníky – HeMiUnlocker musí běžet jako správce.",
                )
                .into());
            }
            let mut size = 0u32;
            GetTokenInformation(token, TokenUser, null_mut(), 0, &mut size);
            let mut user = vec![0u64; (size.max(64) as usize).div_ceil(8)];
            let ok = GetTokenInformation(token, TokenUser, user.as_mut_ptr().cast(), (user.len() * 8) as u32, &mut size) != 0;
            CloseHandle(token);
            if !ok {
                return Err(last_error());
            }
            // the Administrators group by its SID: the same on every language of Windows
            let mut admins: PSID = null_mut();
            if ConvertStringSidToSidW(wide("S-1-5-32-544").as_ptr(), &mut admins) == 0 {
                return Err(last_error());
            }
            Ok(Who { user, admins })
        }
    }

    fn user_sid(&self) -> PSID {
        // SAFETY: `user` holds the TOKEN_USER written by GetTokenInformation, aligned for it (u64); the SID it points to
        // is inside the same buffer, which lives as long as `self`
        unsafe { (*self.user.as_ptr().cast::<TOKEN_USER>()).User.Sid }
    }

    /// The account that becomes the owner ("DOMAIN\name"), for the notification: when the administrator who confirmed
    /// UAC is another account, the files are its now, not of the one signed in.
    fn user_name(&self) -> Option<String> {
        let (mut name, mut domain) = ([0u16; 256], [0u16; 256]);
        let (mut n, mut d, mut kind) = (name.len() as u32, domain.len() as u32, 0);
        // SAFETY: a valid SID; both buffers with their lengths in characters
        let ok = unsafe { LookupAccountSidW(null(), self.user_sid(), name.as_mut_ptr(), &mut n, domain.as_mut_ptr(), &mut d, &mut kind) } != 0;
        if !ok {
            return None;
        }
        let (name, domain) = (crate::locks::from_wide(&name), crate::locks::from_wide(&domain));
        Some(if domain.is_empty() { name } else { format!("{domain}\\{name}") })
    }

    /// One file or folder: the user its owner, the Administrators full control added to its permissions.
    fn take_one(&self, path: &Path) -> Result<(), String> {
        let name = wide(path);
        // SAFETY: the path ends with a NUL; the SIDs are valid while `self` lives; the descriptor and the new DACL are
        // freed with LocalFree after their last use
        unsafe {
            let r = SetNamedSecurityInfoW(name.as_ptr(), SE_FILE_OBJECT, OWNER_SECURITY_INFORMATION, self.user_sid(), null_mut(), null(), null());
            if r != ERROR_SUCCESS {
                return Err(os_error(r));
            }
            let mut dacl: *mut ACL = null_mut();
            let mut sd: PSECURITY_DESCRIPTOR = null_mut();
            let r = GetNamedSecurityInfoW(name.as_ptr(), SE_FILE_OBJECT, DACL_SECURITY_INFORMATION, null_mut(), null_mut(), &mut dacl, null_mut(), &mut sd);
            if r != ERROR_SUCCESS {
                return Err(os_error(r));
            }
            let entry = EXPLICIT_ACCESS_W {
                grfAccessPermissions: FILE_ALL_ACCESS,
                grfAccessMode: GRANT_ACCESS,
                grfInheritance: NO_INHERITANCE,
                Trustee: TRUSTEE_W {
                    pMultipleTrustee: null_mut(),
                    MultipleTrusteeOperation: NO_MULTIPLE_TRUSTEE,
                    TrusteeForm: TRUSTEE_IS_SID,
                    TrusteeType: TRUSTEE_IS_GROUP,
                    ptstrName: self.admins.cast(),
                },
            };
            let mut new_dacl: *mut ACL = null_mut();
            let r = SetEntriesInAclW(1, &entry, dacl, &mut new_dacl);
            LocalFree(sd);
            if r != ERROR_SUCCESS {
                return Err(os_error(r));
            }
            let r = SetNamedSecurityInfoW(name.as_ptr(), SE_FILE_OBJECT, DACL_SECURITY_INFORMATION, null_mut(), null_mut(), new_dacl, null());
            LocalFree(new_dacl.cast());
            if r != ERROR_SUCCESS {
                return Err(os_error(r));
            }
        }
        Ok(())
    }

    /// A folder with everything in it (links are left alone, see [`walk`]); goes on past an item that fails.
    fn take_tree(&self, path: &Path, tally: &mut Tally, saved: &mut String) {
        walk(path, &mut |item| {
            // the owner and the permissions as they were, to put them back
            if let Some(sddl) = sddl_of(item) {
                saved.push_str(&format!("{sddl}\t{}\n", item.display()));
            }
            match self.take_one(item) {
                Ok(()) => tally.done += 1,
                Err(e) => {
                    tally.failed += 1;
                    tally.first_error.get_or_insert_with(|| {
                        format!("{}: {e}", item.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| item.display().to_string()))
                    });
                }
            }
        });
    }
}

/// How a take ownership went: the items done, the ones that failed and the first failure.
#[derive(Default)]
struct Tally {
    done: usize,
    failed: usize,
    first_error: Option<String>,
}

impl Tally {
    /// The failure told: alone for one item, with the counts for a folder ("9 999 of 10 000 items, 1 failed: …").
    fn message(&self) -> Option<String> {
        let first = self.first_error.as_ref()?;
        if self.done == 0 && self.failed <= 1 {
            return Some(first.clone());
        }
        // Czech in the genitive after "z" / "u": 1 položky, 2 položek, 5 položek
        let (items, polozky) = (["{n} item", "{n} items"], ["{n} položky", "{n} položek", "{n} položek"]);
        Some(
            tr("Taken for {done} of {all}, {failed} failed: {first}", "Převzato u {done} z {all}, nepodařilo se u {failed}: {first}")
                .replace("{done}", &self.done.to_string())
                .replace("{all}", &count(self.done + self.failed, items, polozky))
                .replace("{failed}", &count(self.failed, items, polozky))
                .replace("{first}", first),
        )
    }
}

/// A symbolic link or a junction (not a file of OneDrive, which is a reparse point too): the security functions of
/// Windows set the owner of what it points to, somewhere outside the chosen folder.
fn is_link(path: &Path) -> bool {
    // `is_symlink` on Windows: a reparse point whose tag is a name surrogate (symbolic link, junction)
    std::fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink())
}

/// Every item of a folder with the folder itself, or the file; a link is neither visited nor followed.
fn walk(path: &Path, visit: &mut dyn FnMut(&Path)) {
    if is_link(path) {
        return;
    }
    visit(path);
    if std::fs::symlink_metadata(path).is_ok_and(|m| m.is_dir())
        && let Ok(entries) = std::fs::read_dir(path)
    {
        for entry in entries.flatten() {
            walk(&entry.path(), visit);
        }
    }
}

impl Drop for Who {
    fn drop(&mut self) {
        // SAFETY: made by ConvertStringSidToSidW, freed once here
        unsafe { LocalFree(self.admins) };
    }
}

pub fn run(targets: Vec<PathBuf>, elevated: bool) {
    if let Some(why) = targets.iter().find_map(|t| forbidden(t)) {
        toast(tr("Ownership not taken", "Vlastnictví nepřevzato"), &why);
        return;
    }
    // asked once, by the first instance (the elevated one it starts was asked for already)
    if !elevated && !confirm(&targets) {
        return;
    }
    if !elevated && !crate::procs::self_elevated() {
        if !relaunch_elevated(SWITCH, &targets) {
            toast(
                tr("Ownership not taken", "Vlastnictví nepřevzato"),
                tr("Taking ownership needs administrator rights.", "Převzetí vlastnictví potřebuje oprávnění správce."),
            );
        }
        return;
    }
    let paths: Vec<String> = targets.iter().map(|t| t.display().to_string()).collect();
    crate::log::write(&format!("Take ownership: {}", paths.join(", ")));
    let failed: Vec<String> = targets.iter().filter_map(|t| take(t).err()).collect();
    let name = name_of(&targets);
    if failed.is_empty() {
        // the account named: with UAC confirmed by another administrator, the owner is that account
        let owner = Who::new().ok().and_then(|w| w.user_name());
        let text = match owner {
            Some(who) => tr(
                "{name}: the owner is {who} now and the administrators have full control.",
                "{name}: vlastníkem je teď {who} a správci mají plný přístup.",
            )
            .replace("{who}", &who),
            None => tr("You own {name} now and the administrators have full control.", "{name}: vlastníkem jsi teď ty a správci mají plný přístup.").into(),
        };
        toast(tr("Ownership taken", "Vlastnictví převzato"), &text.replace("{name}", &name));
    } else {
        toast(tr("Ownership not taken", "Vlastnictví nepřevzato"), &failed.join("\n"));
    }
}

/// The user becomes the owner, the Administrators get full control (for a folder: everything in it); what was there
/// before is written to a new file of the undo folder.
fn take(path: &Path) -> Result<(), String> {
    if is_link(path) {
        return Err(tr(
            "{name} is a link – the owner of what it points to is not changed (choose the folder or file itself).",
            "{name} je odkaz – vlastník toho, kam vede, se nemění (vyber samotnou složku nebo soubor).",
        )
        .replace("{name}", &name_of(&[path.to_path_buf()])));
    }
    let who = Who::new()?;
    let mut tally = Tally::default();
    let mut saved = String::from("# HeMiUnlocker: the owners and permissions before taking ownership (SDDL, tab, path)\n");
    who.take_tree(path, &mut tally, &mut saved);
    let mut undo_error = None;
    if let Some(dir) = undo_dir() {
        let _ = std::fs::create_dir_all(&dir);
        // only administrators may write there: "Undo" runs as administrator and must not take a list another program
        // of the user wrote
        let protected = restore_one(&dir, UNDO_DIR_SDDL);
        let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
        let written = protected.and_then(|()| std::fs::write(dir.join(format!("ownership-{stamp}.txt")), saved).map_err(|e| e.to_string()));
        if let Err(e) = written {
            undo_error = Some(format!("{}: {e}", tr("The list to undo it could not be saved", "Seznam pro vrácení nešel uložit")));
        }
    }
    match (tally.message(), undo_error) {
        (None, None) => Ok(()),
        (Some(a), Some(b)) => Err(format!("{a}\n{b}")),
        (Some(e), None) | (None, Some(e)) => Err(e),
    }
}

/// Started from the window: the same as the File Explorer item (a process of its own, as administrator). False only
/// when the start as administrator failed.
pub fn start(targets: &[PathBuf]) -> bool {
    if let Some(why) = targets.iter().find_map(|t| forbidden(t)) {
        toast(tr("Ownership not taken", "Vlastnictví nepřevzato"), &why);
        return true;
    }
    if !confirm(targets) {
        return true;
    }
    relaunch_elevated(SWITCH, targets)
}

/// %ProgramData%\HeMi\HeMiUnlocker\ownership-undo: one file per take ownership.
fn undo_dir() -> Option<PathBuf> {
    Some(PathBuf::from(std::env::var_os("ProgramData")?).join("HeMi").join("HeMiUnlocker").join("ownership-undo"))
}

/// The undo folder: owned by the Administrators, full control for them and SYSTEM, read only for the users (the window
/// shows "Undo" when a list is there), nothing inherited from ProgramData (where users may create files).
const UNDO_DIR_SDDL: &str = "O:BAD:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)(A;OICI;FR;;;BU)";

/// The newest list to undo, if any.
pub fn last_undo() -> Option<PathBuf> {
    let rd = std::fs::read_dir(undo_dir()?).ok()?;
    rd.flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "txt"))
        .max_by_key(|p| std::fs::metadata(p).and_then(|m| m.modified()).ok())
}

/// The places whose owner Windows itself needs: refused, with the reason.
fn forbidden(path: &Path) -> Option<String> {
    let norm = |p: &Path| p.display().to_string().trim_end_matches('\\').to_lowercase();
    let me = norm(path);
    let env = |n: &str| std::env::var_os(n).map(|v| norm(Path::new(&v)));
    let refuse = || {
        Some(
            tr(
                "Windows needs this place as it is – its owner is not changed (choose a file or folder inside it).",
                "Windows toto místo potřebuje tak, jak je – jeho vlastník se nemění (vyber soubor nebo složku uvnitř).",
            )
            .to_string(),
        )
    };
    if path.parent().is_none() || me.len() <= 2 {
        return refuse();
    }
    // the Windows folder and the apps of the Store: nothing inside either
    for whole in [env("SystemRoot"), env("ProgramFiles").map(|p| format!("{p}\\windowsapps"))].into_iter().flatten() {
        if me == whole || me.starts_with(&format!("{whole}\\")) {
            return refuse();
        }
    }
    // these folders themselves (what is inside them is allowed)
    let drive = env("SystemDrive").unwrap_or_else(|| "c:".into());
    let tops = [env("ProgramFiles"), env("ProgramFiles(x86)"), env("ProgramW6432"), env("ProgramData"), env("USERPROFILE"), Some(format!("{drive}\\users"))];
    if tops.into_iter().flatten().any(|t| t == me) {
        return refuse();
    }
    None
}

/// Asks before anything changes (Yes / No, No by default).
fn confirm(targets: &[PathBuf]) -> bool {
    use windows_sys::Win32::UI::WindowsAndMessaging::{IDYES, MB_DEFBUTTON2, MB_ICONWARNING, MB_SETFOREGROUND, MB_TOPMOST, MB_YESNO, MessageBoxW};
    let folder = targets.iter().any(|t| t.is_dir());
    let text = if folder {
        tr(
            "Take ownership of {name} and everything in it?\n\nYou become the owner and the administrators get full control. The owners and permissions as they are now are saved – “Undo” in the window of HeMiUnlocker puts them back.",
            "Převzít vlastnictví: {name} a vše uvnitř?\n\nStaneš se vlastníkem a správci dostanou plný přístup. Dosavadní vlastníci a oprávnění se uloží – „Vrátit“ v okně HeMiUnlockeru je obnoví.",
        )
    } else {
        tr(
            "Take ownership of {name}?\n\nYou become the owner and the administrators get full control. The owner and permissions as they are now are saved – “Undo” in the window of HeMiUnlocker puts them back.",
            "Převzít vlastnictví: {name}?\n\nStaneš se vlastníkem a správci dostanou plný přístup. Dosavadní vlastník a oprávnění se uloží – „Vrátit“ v okně HeMiUnlockeru je obnoví.",
        )
    }
    .replace("{name}", &name_of(targets));
    let (text, title) = (wide(&text), wide("HeMiUnlocker"));
    // SAFETY: both strings end with a NUL and live through the call
    unsafe { MessageBoxW(null_mut(), text.as_ptr(), title.as_ptr(), MB_YESNO | MB_ICONWARNING | MB_DEFBUTTON2 | MB_TOPMOST | MB_SETFOREGROUND) == IDYES }
}

/// The owner and the permissions of one item as SDDL.
fn sddl_of(path: &Path) -> Option<String> {
    let name = wide(path);
    let info = OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION;
    // SAFETY: the out pointers are valid; the descriptor and the string are freed with LocalFree; the string ends with
    // a NUL and `len` is its length in characters
    unsafe {
        let mut sd: PSECURITY_DESCRIPTOR = null_mut();
        if GetNamedSecurityInfoW(name.as_ptr(), SE_FILE_OBJECT, info, null_mut(), null_mut(), null_mut(), null_mut(), &mut sd) != ERROR_SUCCESS {
            return None;
        }
        let mut text: *mut u16 = null_mut();
        let mut len = 0u32;
        let ok = ConvertSecurityDescriptorToStringSecurityDescriptorW(sd, SDDL_REVISION_1, info, &mut text, &mut len) != 0;
        LocalFree(sd);
        if !ok || text.is_null() {
            return None;
        }
        let mut n = 0;
        while *text.add(n) != 0 {
            n += 1;
        }
        let s = String::from_utf16_lossy(std::slice::from_raw_parts(text, n));
        LocalFree(text.cast());
        Some(s)
    }
}

/// Puts one SDDL back on one item: the owner and the permissions, their protection from inheritance as it was.
fn restore_one(path: &Path, sddl: &str) -> Result<(), String> {
    let name = wide(path);
    let text = wide(sddl);
    // SAFETY: the descriptor made from the string is freed with LocalFree; owner and DACL point into it and are used
    // before that
    unsafe {
        let mut sd: PSECURITY_DESCRIPTOR = null_mut();
        if ConvertStringSecurityDescriptorToSecurityDescriptorW(text.as_ptr(), SDDL_REVISION_1, &mut sd, null_mut()) == 0 {
            return Err(last_error());
        }
        let (mut owner, mut defaulted): (PSID, i32) = (null_mut(), 0);
        GetSecurityDescriptorOwner(sd, &mut owner, &mut defaulted);
        let (mut present, mut dacl): (i32, *mut ACL) = (0, null_mut());
        GetSecurityDescriptorDacl(sd, &mut present, &mut dacl, &mut defaulted);
        let (mut control, mut revision) = (0u16, 0u32);
        GetSecurityDescriptorControl(sd, &mut control, &mut revision);
        let protection = if control & SE_DACL_PROTECTED != 0 { PROTECTED_DACL_SECURITY_INFORMATION } else { UNPROTECTED_DACL_SECURITY_INFORMATION };
        let r = SetNamedSecurityInfoW(name.as_ptr(), SE_FILE_OBJECT, OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION | protection, owner, null_mut(), dacl, null());
        LocalFree(sd);
        if r == ERROR_SUCCESS { Ok(()) } else { Err(os_error(r)) }
    }
}

/// "Undo" in the window: the last list put back, as administrator (a process of its own).
pub fn start_restore() -> bool {
    match last_undo() {
        Some(file) => relaunch_elevated(RESTORE_SWITCH, &[file]),
        None => false,
    }
}

/// The elevated process of "Undo": every item of the list as it was; the list goes when all went well.
pub fn restore(file: &Path) {
    // only a list of the protected undo folder
    let inside = undo_dir().is_some_and(|d| file.parent().is_some_and(|p| p.eq(&d)));
    if !inside {
        return;
    }
    let Ok(text) = std::fs::read_to_string(file) else { return };
    crate::log::write(&format!("Undo ownership: {}", file.display()));
    // the privileges to set any owner again
    let _who = match Who::new() {
        Ok(w) => w,
        Err(e) => {
            toast(tr("Ownership not put back", "Vlastnictví nevráceno"), &e);
            return;
        }
    };
    let mut failed = Vec::new();
    let mut done = 0;
    for line in text.lines().filter(|l| !l.starts_with('#')) {
        let Some((sddl, path)) = line.split_once('\t') else { continue };
        // the list has no links; one made there since would carry the old permissions to its target
        if is_link(Path::new(path)) {
            failed.push(format!("{path}: {}", tr("now a link, left alone", "teď odkaz, ponecháno")));
            continue;
        }
        match restore_one(Path::new(path), sddl) {
            Ok(()) => done += 1,
            Err(e) => failed.push(format!("{path}: {e}")),
        }
    }
    if failed.is_empty() {
        let _ = std::fs::remove_file(file);
        toast(
            tr("Ownership put back", "Vlastnictví vráceno"),
            &tr("Owners and permissions are as they were ({n}).", "Vlastníci a oprávnění jsou jako předtím ({n}).")
                .replace("{n}", &count(done, ["{n} item", "{n} items"], ["{n} položka", "{n} položky", "{n} položek"])),
        );
    } else {
        toast(tr("Ownership not fully put back", "Vlastnictví nevráceno úplně"), &failed.into_iter().take(3).collect::<Vec<_>>().join("\n"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refuses_the_places_of_windows() {
        let windir = std::env::var("SystemRoot").unwrap();
        for p in [r"C:\".to_string(), windir.clone(), format!(r"{windir}\System32\drivers"), std::env::var("ProgramFiles").unwrap(), format!(r"{}\WindowsApps\x", std::env::var("ProgramFiles").unwrap()), std::env::var("USERPROFILE").unwrap(), r"C:\Users".into()] {
            assert!(forbidden(Path::new(&p)).is_some(), "{p}");
        }
        for p in [format!(r"{}\SomeApp", std::env::var("ProgramFiles").unwrap()), format!(r"{}\Downloads", std::env::var("USERPROFILE").unwrap()), r"D:\Data".into()] {
            assert!(forbidden(Path::new(&p)).is_none(), "{p}");
        }
    }

    /// A junction in the chosen folder (made without administrator rights by mklink /J, only in the temp folder): neither
    /// it nor what it points to is visited.
    #[test]
    fn links_are_left_alone() {
        let base = std::env::temp_dir().join("hemiunlocker-link-test");
        let _ = std::fs::remove_dir_all(&base);
        let (tree, outside) = (base.join("tree"), base.join("outside"));
        std::fs::create_dir_all(&tree).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(tree.join("a.txt"), "a").unwrap();
        std::fs::write(outside.join("secret.txt"), "s").unwrap();
        let link = tree.join("link");
        let made = std::os::windows::fs::symlink_dir(&outside, &link).is_ok()
            || std::process::Command::new(std::env::var("ComSpec").unwrap_or_else(|_| r"C:\Windows\System32\cmd.exe".into()))
                .args(["/c", "mklink", "/J"])
                .arg(&link)
                .arg(&outside)
                .output()
                .is_ok_and(|o| o.status.success());
        if made {
            assert!(is_link(&link));
            let mut seen = Vec::new();
            walk(&tree, &mut |p| seen.push(p.to_path_buf()));
            assert_eq!(seen, vec![tree.clone(), tree.join("a.txt")]);
            // the link chosen itself: refused
            assert!(take(&link).is_err());
        }
        assert!(!is_link(&tree) && !is_link(&tree.join("a.txt")));
        // a junction goes with remove_dir (not what it points to)
        let _ = std::fs::remove_dir(&link);
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn owner_and_permissions_go_back() {
        let dir = std::env::temp_dir().join("hemiunlocker-sddl-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let before = sddl_of(&dir).unwrap();
        restore_one(&dir, &before).unwrap();
        // Windows marks a DACL written anew as auto-inherited ("AI"); the entries themselves are the same
        let plain = |s: String| s.replace("D:AI", "D:");
        assert_eq!(plain(sddl_of(&dir).unwrap()), plain(before));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
