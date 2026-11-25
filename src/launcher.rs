//! HeMi Launcher (installs and updates the HeMi apps): when it is on this PC, About offers it for the updates. It writes
//! where it is to HKCU\Software\HeMi\HeMiLauncher (value Path) at each start.

use crate::procs::wide;
use std::path::PathBuf;
use windows_sys::Win32::System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_SZ, RegGetValueW};

/// The exe of the launcher, when it is still there. Only a file named HeMiLauncher.exe, and never for a window run as
/// administrator: any program of the user may write that value, and it would start elevated.
pub fn exe() -> Option<PathBuf> {
    if crate::procs::self_elevated() {
        return None;
    }
    let (key, name) = (wide(r"Software\HeMi\HeMiLauncher"), wide("Path"));
    let mut buf = vec![0u16; 1024];
    let mut size = (buf.len() * 2) as u32;
    // SAFETY: the key and value names end with a NUL; `size` is the size of the buffer in bytes
    let r = unsafe {
        RegGetValueW(HKEY_CURRENT_USER, key.as_ptr(), name.as_ptr(), RRF_RT_REG_SZ, std::ptr::null_mut(), buf.as_mut_ptr().cast(), &mut size)
    };
    if r != 0 {
        return None;
    }
    let path = PathBuf::from(crate::locks::from_wide(&buf));
    let named = path.file_name().is_some_and(|n| n.eq_ignore_ascii_case("HeMiLauncher.exe"));
    (named && path.is_file()).then_some(path)
}

/// Opens the launcher (the running one comes to the front) and lets it look for updates.
pub fn open() {
    if let Some(exe) = exe() {
        let _ = std::process::Command::new(exe).arg("--check").spawn();
    }
}
