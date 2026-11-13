//! The optional item in the Explorer context menu (the classic menu; in Windows 11 under "Show more options").
//! Only in the registry of the user (HKCU\Software\Classes), so no administrator rights are needed; off by default.

use crate::i18n::tr;
use crate::procs::wide;
use std::ptr::{null, null_mut};
use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};
use windows_sys::Win32::System::Registry::*;

/// The keys of the item: for files and for folders.
const ITEMS: [&str; 2] = [r"Software\Classes\*\shell\HeMiUnlocker", r"Software\Classes\Directory\shell\HeMiUnlocker"];

/// The text of the item in the menu (the same in every language).
const TEXT: &str = "HeMiUnlock";

fn command() -> Option<(String, String)> {
    let exe = std::env::current_exe().ok()?.display().to_string();
    Some((format!("\"{exe}\" \"%1\""), format!("\"{exe}\",0")))
}

fn read(key: &str) -> Option<String> {
    let k = wide(key);
    let mut buf = vec![0u16; 2048];
    let mut size = (buf.len() * 2) as u32;
    let r = unsafe { RegGetValueW(HKEY_CURRENT_USER, k.as_ptr(), null(), RRF_RT_REG_SZ, null_mut(), buf.as_mut_ptr() as _, &mut size) };
    (r == ERROR_SUCCESS).then(|| crate::locks::from_wide(&buf))
}

fn write(key: &str, name: Option<&str>, value: &str) -> Result<(), String> {
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

pub fn is_on() -> bool {
    read(&format!(r"{}\command", ITEMS[0])).is_some()
}

pub fn set(on: bool) -> Result<(), String> {
    for key in ITEMS {
        if on {
            let (cmd, icon) = command().ok_or(tr("Could not find out where HeMiUnlocker is.", "Nepodařilo se zjistit, kde je HeMiUnlocker."))?;
            write(key, None, TEXT)?;
            write(key, Some("Icon"), &icon)?;
            write(&format!(r"{key}\command"), None, &cmd)?;
        } else {
            let k = wide(key);
            let r = unsafe { RegDeleteTreeW(HKEY_CURRENT_USER, k.as_ptr()) };
            if r != ERROR_SUCCESS && r != ERROR_FILE_NOT_FOUND {
                return Err(format!("{} ({r}).", tr("The item could not be removed", "Položku se nepodařilo odebrat – chyba")));
            }
            let _ = unsafe { RegDeleteKeyW(HKEY_CURRENT_USER, k.as_ptr()) };
        }
    }
    Ok(())
}

/// The item is on but points to another place (HeMiUnlocker was moved) or has an older text: write it again.
pub fn repair() {
    if let (true, Some((cmd, _))) = (is_on(), command()) {
        let stale = ITEMS.iter().any(|key| {
            read(&format!(r"{key}\command")).as_deref() != Some(cmd.as_str()) || read(key).as_deref() != Some(TEXT)
        });
        if stale {
            let _ = set(true);
        }
    }
}
