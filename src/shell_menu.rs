//! The optional submenu "HeMiUnlocker" in the Explorer context menu of files and folders (the classic menu; in
//! Windows 11 under "Show more options"): "Open in HeMiUnlocker" and "Unlock" (which frees the file without the window
//! and tells the result in a notification). Only in the registry of the user (HKCU\Software\Classes), so no
//! administrator rights are needed; off by default.

use crate::i18n::tr;
use crate::procs::wide;
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

/// The two items of the submenu: key, text, command.
fn items(exe: &str) -> [(&'static str, &'static str, String); 2] {
    [
        ("open", tr("Open in HeMiUnlocker", "Otevřít v HeMiUnlockeru"), format!("\"{exe}\" \"%1\"")),
        ("unlock", tr("Unlock", "Odemknout"), format!("\"{exe}\" {} \"%1\"", crate::unlock::SWITCH)),
    ]
}
fn read(key: &str, name: Option<&str>) -> Option<String> {
    let k = wide(key);
    let n = name.map(wide);
    let mut buf = vec![0u16; 2048];
    let mut size = (buf.len() * 2) as u32;
    let r = unsafe {
        RegGetValueW(HKEY_CURRENT_USER, k.as_ptr(), n.as_ref().map_or(null(), |n| n.as_ptr()), RRF_RT_REG_SZ, null_mut(), buf.as_mut_ptr() as _, &mut size)
    };
    (r == ERROR_SUCCESS).then(|| crate::locks::from_wide(&buf))
}

pub(crate) fn write(key: &str, name: Option<&str>, value: &str) -> Result<(), String> {
    let k = wide(key);
    let n = name.map(wide);
    let v = wide(value);
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
    format!("{} ({r}).", tr("Writing to the registry failed", "Zápis do registru se nepodařil – chyba"))
}

/// On: the submenu, or the single item of older versions (it is rewritten by `repair`).
pub fn is_on() -> bool {
    read(&format!(r"{}\shell\open\command", ITEMS[0]), None).is_some() || read(&format!(r"{}\command", ITEMS[0]), None).is_some()
}

fn remove(key: &str) -> Result<(), String> {
    let k = wide(key);
    let r = unsafe { RegDeleteTreeW(HKEY_CURRENT_USER, k.as_ptr()) };
    if r != ERROR_SUCCESS && r != ERROR_FILE_NOT_FOUND {
        return Err(format!("{} ({r}).", tr("The item could not be removed", "Položku se nepodařilo odebrat – chyba")));
    }
    let _ = unsafe { RegDeleteKeyW(HKEY_CURRENT_USER, k.as_ptr()) };
    Ok(())
}

pub fn set(on: bool) -> Result<(), String> {
    for key in ITEMS {
        remove(key)?;
        if on {
            let exe = exe().ok_or(tr("Could not find out where HeMiUnlocker is.", "Nepodařilo se zjistit, kde je HeMiUnlocker."))?;
            let icon = format!("\"{exe}\",0");
            // a submenu: MUIVerb is its text, SubCommands (empty) says the items are in its own "shell" key
            write(key, Some("MUIVerb"), TEXT)?;
            write(key, Some("Icon"), &icon)?;
            write(key, Some("SubCommands"), "")?;
            for (name, text, cmd) in items(&exe) {
                let sub = format!(r"{key}\shell\{name}");
                write(&sub, Some("MUIVerb"), text)?;
                write(&sub, Some("Icon"), &icon)?;
                write(&format!(r"{sub}\command"), None, &cmd)?;
            }
        }
    }
    Ok(())
}

/// The submenu is on but points to another place (HeMiUnlocker was moved), has the texts of the other language or is the
/// single item of an older version: written again.
pub fn repair() {
    let Some(exe) = exe() else { return };
    if !is_on() {
        return;
    }
    let stale = ITEMS.iter().any(|key| {
        read(key, Some("SubCommands")).is_none()
            || items(&exe).iter().any(|(name, text, cmd)| {
                let sub = format!(r"{key}\shell\{name}");
                read(&format!(r"{sub}\command"), None).as_deref() != Some(cmd.as_str()) || read(&sub, Some("MUIVerb")).as_deref() != Some(*text)
            })
    });
    if stale {
        let _ = set(true);
    }
}