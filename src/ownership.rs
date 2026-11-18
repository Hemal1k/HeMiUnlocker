//! "Take ownership" (from the File Explorer submenu or the window): the user becomes the owner of a file or folder
//! (with everything in it) and the Administrators get full control – what the well-known "Take Ownership" registry
//! tweak does with takeown and icacls, here by the security functions of Windows itself (no hidden command lines: the
//! machine learning of Defender took hidden takeown / icacls for malware). It needs administrator rights: HeMiUnlocker
//! starts again as administrator (Windows asks). A notification tells the result.

use crate::i18n::tr;
use crate::unlock::{ELEVATED, name_of, relaunch_elevated, toast};
use std::path::{Path, PathBuf};
use std::ptr::{null, null_mut};
use windows_sys::Win32::Foundation::{CloseHandle, ERROR_SUCCESS, HANDLE, LUID, LocalFree};
use windows_sys::Win32::Security::Authorization::{
    ConvertStringSidToSidW, EXPLICIT_ACCESS_W, GRANT_ACCESS, GetNamedSecurityInfoW, NO_MULTIPLE_TRUSTEE, SE_FILE_OBJECT, SetEntriesInAclW,
    SetNamedSecurityInfoW, TRUSTEE_IS_GROUP, TRUSTEE_IS_SID, TRUSTEE_W,
};
use windows_sys::Win32::Security::{
    ACL, AdjustTokenPrivileges, DACL_SECURITY_INFORMATION, GetTokenInformation, LUID_AND_ATTRIBUTES, LookupPrivilegeValueW, NO_INHERITANCE,
    OWNER_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR, PSID, SE_PRIVILEGE_ENABLED, TOKEN_ADJUST_PRIVILEGES, TOKEN_PRIVILEGES, TOKEN_QUERY,
    TOKEN_USER, TokenUser,
};
use windows_sys::Win32::Storage::FileSystem::FILE_ALL_ACCESS;
use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

pub const SWITCH: &str = "--take-ownership";

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn os_error(code: u32) -> String {
    std::io::Error::from_raw_os_error(code as i32).to_string()
}

/// The user of this process and the Administrators group, and the privileges to set the owner of anything.
struct Who {
    user: Vec<u8>,
    admins: PSID,
}

impl Who {
    fn new() -> Result<Who, String> {
        unsafe {
            let mut token: HANDLE = null_mut();
            if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY | TOKEN_ADJUST_PRIVILEGES, &mut token) == 0 {
                return Err(os_error(std::io::Error::last_os_error().raw_os_error().unwrap_or(0) as u32));
            }
            // the owner may be set to anyone (restore) and taken from anyone (take ownership) – administrators have both
            for name in ["SeTakeOwnershipPrivilege", "SeRestorePrivilege"] {
                let mut luid: LUID = std::mem::zeroed();
                if LookupPrivilegeValueW(null(), wide(name).as_ptr(), &mut luid) != 0 {
                    let tp = TOKEN_PRIVILEGES { PrivilegeCount: 1, Privileges: [LUID_AND_ATTRIBUTES { Luid: luid, Attributes: SE_PRIVILEGE_ENABLED }] };
                    AdjustTokenPrivileges(token, 0, &tp, 0, null_mut(), null_mut());
                }
            }
            let mut size = 0u32;
            GetTokenInformation(token, TokenUser, null_mut(), 0, &mut size);
            let mut user = vec![0u8; size.max(64) as usize];
            let ok = GetTokenInformation(token, TokenUser, user.as_mut_ptr().cast(), user.len() as u32, &mut size) != 0;
            CloseHandle(token);
            if !ok {
                return Err(os_error(std::io::Error::last_os_error().raw_os_error().unwrap_or(0) as u32));
            }
            // the Administrators group by its SID: the same on every language of Windows
            let mut admins: PSID = null_mut();
            if ConvertStringSidToSidW(wide("S-1-5-32-544").as_ptr(), &mut admins) == 0 {
                return Err(os_error(std::io::Error::last_os_error().raw_os_error().unwrap_or(0) as u32));
            }
            Ok(Who { user, admins })
        }
    }

    fn user_sid(&self) -> PSID {
        unsafe { (*(self.user.as_ptr() as *const TOKEN_USER)).User.Sid }
    }

    /// One file or folder: the user its owner, the Administrators full control added to its permissions.
    fn take_one(&self, path: &Path) -> Result<(), String> {
        let name = wide(&path.display().to_string());
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

    /// A folder with everything in it (links into other places are not followed); goes on past an item that fails, the
    /// first failure is told.
    fn take_tree(&self, path: &Path, first_error: &mut Option<String>) {
        if let Err(e) = self.take_one(path) {
            first_error.get_or_insert_with(|| format!("{}: {e}", path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| path.display().to_string())));
        }
        let is_dir = std::fs::symlink_metadata(path).map(|m| m.is_dir()).unwrap_or(false);
        if is_dir {
            if let Ok(entries) = std::fs::read_dir(path) {
                for entry in entries.flatten() {
                    self.take_tree(&entry.path(), first_error);
                }
            }
        }
    }
}

impl Drop for Who {
    fn drop(&mut self) {
        unsafe { LocalFree(self.admins) };
    }
}

pub fn run(targets: Vec<PathBuf>, elevated: bool) {
    if !elevated && !crate::procs::self_elevated() {
        if !relaunch_elevated(SWITCH, &targets) {
            toast(
                tr("Ownership not taken", "Vlastnictví nepřevzato"),
                tr("It needs administrator rights.", "Potřebuje oprávnění správce."),
            );
        }
        return;
    }
    let failed: Vec<String> = targets.iter().filter_map(|t| take(t).err()).collect();
    let name = name_of(&targets);
    if failed.is_empty() {
        toast(
            tr("Ownership taken", "Vlastnictví převzato"),
            &tr("You own {name} now and the administrators have full control.", "{name} teď patří tobě a správci k němu mají plný přístup.").replace("{name}", &name),
        );
    } else {
        toast(tr("Ownership not taken", "Vlastnictví nepřevzato"), &failed.join("\n"));
    }
}

/// The user becomes the owner, the Administrators get full control (for a folder: everything in it).
fn take(path: &Path) -> Result<(), String> {
    let who = Who::new()?;
    let mut first_error = None;
    who.take_tree(path, &mut first_error);
    first_error.map_or(Ok(()), Err)
}

/// Started from the window: the same as the File Explorer item (a process of its own, as administrator).
pub fn start(targets: &[PathBuf]) -> bool {
    relaunch_elevated(SWITCH, targets)
}

pub fn args(rest: &[String]) -> (Vec<PathBuf>, bool) {
    let elevated = rest.iter().any(|a| a == ELEVATED);
    let paths = rest
        .iter()
        .filter(|a| a.as_str() != ELEVATED)
        .map(PathBuf::from)
        .filter(|p| p.exists())
        .map(|p| std::path::absolute(&p).unwrap_or(p))
        .collect();
    (paths, elevated)
}
