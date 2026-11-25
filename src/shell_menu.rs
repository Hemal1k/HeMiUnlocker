//! The optional submenu "HeMiUnlocker" in the Explorer context menu of files and folders (the classic menu; in
//! Windows 11 under "Show more options"): "Open in HeMiUnlocker" and "Unlock" (which frees the file without the window
//! and tells the result in a notification). Only in the registry of the user (HKCU\Software\Classes), so no
//! administrator rights are needed; off by default.

use crate::i18n::tr;
use crate::procs::wide;
use std::path::{Path, PathBuf};
use std::ptr::{null, null_mut};
use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};
use windows_sys::Win32::System::Registry::*;

/// The keys of the item: for files and for folders.
const ITEMS: [&str; 2] = [r"Software\Classes\*\shell\HeMiUnlocker", r"Software\Classes\Directory\shell\HeMiUnlocker"];

/// The name of the submenu (the same in every language).
const TEXT: &str = "HeMiUnlocker";

fn exe() -> Option<String> {
    Some(std::env::current_exe().ok()?.display().to_string())
}

/// The items of the submenu: key, text, command, and whether it needs administrator rights (the shield of UAC).
fn items(exe: &str) -> [(&'static str, &'static str, String, bool); 3] {
    [
        ("1open", tr("Open in HeMiUnlocker", "Otevřít v HeMiUnlockeru"), format!("\"{exe}\" \"%1\""), false),
        ("2unlock", tr("Unlock", "Odemknout"), format!("\"{exe}\" {} \"%1\"", crate::unlock::SWITCH), false),
        ("3ownership", tr("Take ownership", "Převzít vlastnictví"), format!("\"{exe}\" {} \"%1\"", crate::ownership::SWITCH), true),
    ]
}
fn read(key: &str, name: Option<&str>) -> Option<String> {
    read_in(HKEY_CURRENT_USER, key, name)
}

fn read_in(root: HKEY, key: &str, name: Option<&str>) -> Option<String> {
    let k = wide(key);
    let n = name.map(wide);
    let mut buf = vec![0u16; 2048];
    let mut size = (buf.len() * 2) as u32;
    // SAFETY: the names end with a NUL (or the value name is null for the default value); `size` is the size of the
    // buffer in bytes
    let r = unsafe {
        RegGetValueW(root, k.as_ptr(), n.as_ref().map_or(null(), |n| n.as_ptr()), RRF_RT_REG_SZ, null_mut(), buf.as_mut_ptr() as _, &mut size)
    };
    (r == ERROR_SUCCESS).then(|| crate::locks::from_wide(&buf))
}

pub(crate) fn write(key: &str, name: Option<&str>, value: &str) -> Result<(), String> {
    let k = wide(key);
    let n = name.map(wide);
    let v = wide(value);
    // SAFETY: the names and the value end with a NUL and the size of the value is given in bytes (with the NUL); the key
    // is closed after the write
    unsafe {
        let mut hkey: HKEY = null_mut();
        let r = RegCreateKeyExW(HKEY_CURRENT_USER, k.as_ptr(), 0, null(), 0, KEY_WRITE, null(), &mut hkey, null_mut());
        if r != ERROR_SUCCESS {
            return Err(write_error(r));
        }
        let r = RegSetValueExW(
            hkey,
            n.as_ref().map_or(null(), |n| n.as_ptr()),
            0,
            REG_SZ,
            v.as_ptr() as *const u8,
            (v.len() * 2) as u32,
        );
        RegCloseKey(hkey);
        if r != ERROR_SUCCESS {
            return Err(write_error(r));
        }
    }
    Ok(())
}

fn write_error(r: u32) -> String {
    format!("{}: {}", tr("Writing to the registry failed", "Zápis do registru se nepodařil"), crate::i18n::os_error(r))
}

/// On: the submenu, or the single item of older versions (it is rewritten by `repair`).
pub fn is_on() -> bool {
    read(&format!(r"{}\shell\1open\command", ITEMS[0]), None).is_some() || read(&format!(r"{}\shell\open\command", ITEMS[0]), None).is_some() || read(&format!(r"{}\command", ITEMS[0]), None).is_some()
}

fn remove(key: &str) -> Result<(), String> {
    let k = wide(key);
    // SAFETY: the key name ends with a NUL
    let r = unsafe { RegDeleteTreeW(HKEY_CURRENT_USER, k.as_ptr()) };
    if r != ERROR_SUCCESS && r != ERROR_FILE_NOT_FOUND {
        return Err(format!("{}: {}", tr("The item could not be removed", "Položku se nepodařilo odebrat"), crate::i18n::os_error(r)));
    }
    // the now empty key itself (RegDeleteTreeW leaves it); already gone is fine
    // SAFETY: the key name ends with a NUL
    let _ = unsafe { RegDeleteKeyW(HKEY_CURRENT_USER, k.as_ptr()) };
    Ok(())
}

/// The items on (to this exe) or off.
pub fn set(on: bool) -> Result<(), String> {
    let exe = if on { Some(exe().ok_or(tr("Could not find out where HeMiUnlocker is.", "Nepodařilo se zjistit, kde je HeMiUnlocker."))?) } else { None };
    set_for(exe.as_deref())
}

/// The items to `exe`, or none.
fn set_for(exe: Option<&str>) -> Result<(), String> {
    for key in ITEMS {
        remove(key)?;
        if let Some(exe) = exe {
            let icon = format!("\"{exe}\",0");
            // a submenu: MUIVerb is its text, SubCommands (empty) says the items are in its own "shell" key
            write(key, Some("MUIVerb"), TEXT)?;
            write(key, Some("Icon"), &icon)?;
            write(key, Some("SubCommands"), "")?;
            for (name, text, cmd, admin) in items(exe) {
                let sub = format!(r"{key}\shell\{name}");
                write(&sub, Some("MUIVerb"), text)?;
                if admin {
                    // the shield of UAC instead of the icon: it asks for administrator rights
                    write(&sub, Some("HasLUAShield"), "")?;
                } else {
                    write(&sub, Some("Icon"), &icon)?;
                }
                write(&format!(r"{sub}\command"), None, &cmd)?;
            }
        }
    }
    Ok(())
}

/// The exe the submenu starts now (from its "Open" command, also of the older versions), if any.
fn configured_exe() -> Option<PathBuf> {
    let cmd = [format!(r"{}\shell\1open\command", ITEMS[0]), format!(r"{}\shell\open\command", ITEMS[0]), format!(r"{}\command", ITEMS[0])]
        .iter()
        .find_map(|k| read(k, None))?;
    let rest = cmd.strip_prefix('"')?;
    Some(PathBuf::from(&rest[..rest.find('"')?]))
}

/// This exe is the one the installer put there (for this user or for all users).
fn is_installed(me: &Path) -> bool {
    let key = r"Software\Microsoft\Windows\CurrentVersion\Uninstall\HeMiUnlocker_is1";
    let same = |a: &Path, b: &Path| match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => a.display().to_string().eq_ignore_ascii_case(&b.display().to_string()),
    };
    [HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE]
        .into_iter()
        .filter_map(|root| read_in(root, key, Some("InstallLocation")))
        .any(|dir| same(&Path::new(dir.trim_end_matches('\\')).join("HeMiUnlocker.exe"), me))
}

/// At each start of the window. The submenu is on but points to a place that is gone (HeMiUnlocker was moved), has the
/// texts of the other language or is the single item of an older version: written again. Another copy (a portable
/// one in Downloads, an old one from a backup) does not take the submenu over while the exe it starts is still there,
/// unless it is the installed one.
pub fn repair() {
    let Some(me) = exe() else { return };
    if !is_on() {
        return;
    }
    let exe = match configured_exe() {
        Some(c) if c.is_file() && !is_installed(Path::new(&me)) => c.display().to_string(),
        _ => me,
    };
    repair_to(&exe);
}

/// `--repair` from the installer (after an installation or a move to another folder): the submenu to this exe.
pub fn repair_installed() {
    let Some(me) = exe() else { return };
    if is_on() {
        repair_to(&me);
    }
}

fn repair_to(exe: &str) {
    let stale = ITEMS.iter().any(|key| {
        read(key, Some("SubCommands")).is_none()
            || items(exe).iter().any(|(name, text, cmd, _)| {
                let sub = format!(r"{key}\shell\{name}");
                read(&format!(r"{sub}\command"), None).as_deref() != Some(cmd.as_str()) || read(&sub, Some("MUIVerb")).as_deref() != Some(*text)
            })
    });
    // best effort at a start: the switch in the settings tells a failure when the user turns the menu on
    if stale {
        let _ = set_for(Some(exe));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn this_test_is_not_the_installed_copy() {
        assert!(!is_installed(&std::env::current_exe().unwrap()));
    }
}