//! Processes: the path of the program, administrator rights, its icon, closing and ending it.

use crate::i18n::tr;
use std::ffi::c_void;
use std::mem::{size_of, zeroed};
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::Command;
use std::ptr::{null, null_mut};
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::Security::{GetTokenInformation, TOKEN_ELEVATION, TOKEN_QUERY, TokenElevation};
use windows_sys::Win32::System::Diagnostics::ToolHelp::*;
use windows_sys::Win32::System::RemoteDesktop::ProcessIdToSessionId;
use windows_sys::Win32::System::SystemInformation::GetWindowsDirectoryW;
use windows_sys::Win32::System::Threading::*;
use windows_sys::Win32::UI::Shell::ShellExecuteW;
use windows_sys::Win32::UI::WindowsAndMessaging::*;
use windows_sys::core::BOOL;

pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

struct Handle(HANDLE);
impl Drop for Handle {
    fn drop(&mut self) {
        unsafe { CloseHandle(self.0) };
    }
}

fn open(pid: u32, access: u32) -> Option<Handle> {
    let h = unsafe { OpenProcess(access, 0, pid) };
    if h.is_null() { None } else { Some(Handle(h)) }
}

pub fn exe_path(pid: u32) -> Option<String> {
    let h = open(pid, PROCESS_QUERY_LIMITED_INFORMATION)?;
    let mut buf = [0u16; 2048];
    let mut len = buf.len() as u32;
    let ok = unsafe { QueryFullProcessImageNameW(h.0, PROCESS_NAME_WIN32, buf.as_mut_ptr(), &mut len) };
    (ok != 0).then(|| String::from_utf16_lossy(&buf[..len as usize]))
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Elevation {
    Normal,
    Elevated,
    /// We may not look: an elevated process or one of another user.
    Unknown,
}

pub fn elevation(pid: u32) -> Elevation {
    match open(pid, PROCESS_QUERY_LIMITED_INFORMATION) {
        Some(h) => token_elevation(h.0),
        None => Elevation::Unknown,
    }
}

fn token_elevation(process: HANDLE) -> Elevation {
    unsafe {
        let mut token: HANDLE = null_mut();
        if OpenProcessToken(process, TOKEN_QUERY, &mut token) == 0 {
            return Elevation::Unknown;
        }
        let token = Handle(token);
        let mut e = TOKEN_ELEVATION { TokenIsElevated: 0 };
        let mut len = 0u32;
        let ok = GetTokenInformation(
            token.0,
            TokenElevation,
            &mut e as *mut _ as *mut c_void,
            size_of::<TOKEN_ELEVATION>() as u32,
            &mut len,
        );
        if ok == 0 {
            Elevation::Unknown
        } else if e.TokenIsElevated != 0 {
            Elevation::Elevated
        } else {
            Elevation::Normal
        }
    }
}

pub fn self_elevated() -> bool {
    token_elevation(unsafe { GetCurrentProcess() }) == Elevation::Elevated
}

fn creation_time(h: HANDLE) -> Option<u64> {
    unsafe {
        let (mut c, mut e, mut k, mut u): (FILETIME, FILETIME, FILETIME, FILETIME) = (zeroed(), zeroed(), zeroed(), zeroed());
        (GetProcessTimes(h, &mut c, &mut e, &mut k, &mut u) != 0)
            .then(|| ((c.dwHighDateTime as u64) << 32) | c.dwLowDateTime as u64)
    }
}

/// Processes that must never be ended (Windows would stop or restart).
pub fn is_protected(pid: u32, exe: Option<&str>) -> bool {
    const SYSTEM: [&str; 9] = [
        "csrss.exe", "wininit.exe", "winlogon.exe", "smss.exe", "services.exe", "lsass.exe", "lsaiso.exe", "dwm.exe",
        "fontdrvhost.exe",
    ];
    if pid <= 4 || pid == std::process::id() {
        return true;
    }
    // marked critical by Windows: ending it stops Windows (a blue screen), whatever its name
    if is_critical(pid) {
        return true;
    }
    let Some(exe) = exe else { return false };
    SYSTEM.contains(&exe_name(exe).as_str())
}

/// The file name of a program path in lower case.
fn exe_name(exe: &str) -> String {
    exe.rsplit(['\\', '/']).next().unwrap_or(exe).to_ascii_lowercase()
}

/// True when Windows marks the process as critical (IsProcessCritical).
fn is_critical(pid: u32) -> bool {
    let Some(h) = open(pid, PROCESS_QUERY_LIMITED_INFORMATION) else { return false };
    let mut critical: BOOL = 0;
    // SAFETY: a valid process handle and a valid out pointer
    unsafe { IsProcessCritical(h.0, &mut critical) != 0 && critical != 0 }
}

/// A service or a process of the system (the session of the services, svchost): "Unlock" from the menu does not end
/// it without asking – the window can, after a question.
pub fn is_service(pid: u32, exe: Option<&str>) -> bool {
    if exe.is_some_and(|e| exe_name(e) == "svchost.exe") {
        return true;
    }
    let mut session = u32::MAX;
    // SAFETY: a valid out pointer; session 0 holds the services and the system, never a program of the user
    unsafe { ProcessIdToSessionId(pid, &mut session) != 0 && session == 0 }
}

/// %SystemRoot%\explorer.exe by the Windows folder itself – never a program named so in the current folder or PATH.
fn explorer() -> PathBuf {
    let mut buf = [0u16; 260];
    // SAFETY: the buffer and its length in characters
    let n = unsafe { GetWindowsDirectoryW(buf.as_mut_ptr(), buf.len() as u32) } as usize;
    let dir = if n > 0 && n < buf.len() { String::from_utf16_lossy(&buf[..n]) } else { r"C:\Windows".into() };
    PathBuf::from(dir).join("explorer.exe")
}

fn open_error() -> String {
    match unsafe { GetLastError() } {
        // already administrator: Windows protects the program (an antivirus, for example)
        ERROR_ACCESS_DENIED if self_elevated() => {
            tr("The program is protected by Windows and cannot be ended.", "Program je chráněný Windows a nejde ukončit.").into()
        }
        ERROR_ACCESS_DENIED => tr(
            "Access denied – the program runs with higher rights. Run HeMiUnlocker as administrator.",
            "Přístup odepřen – program běží s vyššími právy. Spusť HeMiUnlocker jako správce.",
        )
        .into(),
        ERROR_INVALID_PARAMETER => tr("The program is no longer running.", "Program už neběží.").into(),
        e => format!("{}: {}", tr("The program cannot be opened", "Program nejde otevřít"), crate::i18n::os_error(e)),
    }
}

/// Asks the program to close its windows (as the × button does): it can still save its work.
/// Returns how many windows were asked.
pub fn post_close(pid: u32) -> usize {
    struct Found {
        pid: u32,
        windows: Vec<HWND>,
    }
    unsafe extern "system" fn each(hwnd: HWND, lp: LPARAM) -> BOOL {
        let found = unsafe { &mut *(lp as *mut Found) };
        let mut p = 0u32;
        unsafe { GetWindowThreadProcessId(hwnd, &mut p) };
        if p == found.pid && unsafe { IsWindowVisible(hwnd) } != 0 && unsafe { GetWindow(hwnd, GW_OWNER) }.is_null() {
            found.windows.push(hwnd);
        }
        1
    }
    let mut found = Found { pid, windows: Vec::new() };
    unsafe {
        EnumWindows(Some(each), &mut found as *mut Found as LPARAM);
        // its "Save changes?" dialog may come to the front
        AllowSetForegroundWindow(pid);
        for w in &found.windows {
            PostMessageW(*w, WM_CLOSE, 0, 0);
        }
    }
    found.windows.len()
}

/// Waits until the process ends; true when it is gone.
pub fn wait_exit(pid: u32, ms: u32) -> bool {
    match open(pid, PROCESS_SYNCHRONIZE) {
        Some(h) => unsafe { WaitForSingleObject(h.0, ms) == WAIT_OBJECT_0 },
        None => (unsafe { GetLastError() }) == ERROR_INVALID_PARAMETER,
    }
}

/// Ends the process at once (unsaved work is lost).
pub fn terminate(pid: u32, start_time: u64) -> Result<(), String> {
    let h = open(pid, PROCESS_TERMINATE | PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE).ok_or_else(open_error)?;
    if start_time != 0 && creation_time(h.0).is_some_and(|t| t != start_time) {
        return Err(tr(
            "The program is no longer running (its PID now belongs to another process).",
            "Program už neběží (jeho PID teď patří jinému procesu).",
        )
        .into());
    }
    unsafe {
        if TerminateProcess(h.0, 1) == 0 {
            return Err(open_error());
        }
        WaitForSingleObject(h.0, 5000);
    }
    Ok(())
}

pub fn running(exe_name: &str) -> bool {
    unsafe {
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snap == INVALID_HANDLE_VALUE {
            return false;
        }
        let snap = Handle(snap);
        let mut e: PROCESSENTRY32W = zeroed();
        e.dwSize = size_of::<PROCESSENTRY32W>() as u32;
        let mut ok = Process32FirstW(snap.0, &mut e);
        while ok != 0 {
            if crate::locks::from_wide(&e.szExeFile).eq_ignore_ascii_case(exe_name) {
                return true;
            }
            ok = Process32NextW(snap.0, &mut e);
        }
        false
    }
}

/// Ends this Explorer and starts it again when Windows does not do it by itself.
pub fn restart_explorer(pid: u32, start_time: u64) -> Result<(), String> {
    terminate(pid, start_time)?;
    for _ in 0..20 {
        std::thread::sleep(std::time::Duration::from_millis(200));
        if running("explorer.exe") {
            return Ok(());
        }
    }
    Command::new(explorer()).spawn().map(|_| ()).map_err(|e| e.to_string())
}

pub fn show_in_explorer(path: &str) {
    let _ = Command::new(explorer()).raw_arg(format!("/select,\"{path}\"")).spawn();
}

/// Opens a file or a link (mailto:, a text file …) in its default app.
pub fn shell_open(target: &str) {
    let (verb, t) = (wide("open"), wide(target));
    unsafe { ShellExecuteW(null_mut(), verb.as_ptr(), t.as_ptr(), null(), null(), SW_SHOWNORMAL) };
}

/// Starts HeMiUnlocker again as administrator (Windows asks) with the same files; true when it started.
pub fn relaunch_as_admin(targets: &[PathBuf]) -> bool {
    let Ok(exe) = std::env::current_exe() else { return false };
    let params: Vec<String> = targets.iter().map(|t| format!("\"{}\"", t.display())).collect();
    let (verb, exe, params) = (wide("runas"), wide(&exe.to_string_lossy()), wide(&params.join(" ")));
    let r = unsafe { ShellExecuteW(null_mut(), verb.as_ptr(), exe.as_ptr(), params.as_ptr(), null(), SW_SHOWNORMAL) };
    r as isize > 32
}

/// The icon of a program as RGBA pixels.
pub fn exe_icon(path: &str, size: i32) -> Option<(Vec<u8>, usize, usize)> {
    let w = wide(path);
    unsafe {
        let mut icon: HICON = null_mut();
        let mut id = 0u32;
        let n = PrivateExtractIconsW(w.as_ptr(), 0, size, size, &mut icon, &mut id, 1, 0);
        if n == 0 || n == u32::MAX || icon.is_null() {
            return None;
        }
        let pixels = icon_pixels(icon);
        DestroyIcon(icon);
        pixels
    }
}

unsafe fn icon_pixels(icon: HICON) -> Option<(Vec<u8>, usize, usize)> {
    unsafe {
        let mut info: ICONINFO = zeroed();
        if GetIconInfo(icon, &mut info) == 0 {
            return None;
        }
        let free = |info: &ICONINFO| {
            if !info.hbmColor.is_null() {
                DeleteObject(info.hbmColor);
            }
            if !info.hbmMask.is_null() {
                DeleteObject(info.hbmMask);
            }
        };
        if info.hbmColor.is_null() {
            free(&info);
            return None;
        }
        let mut bm: BITMAP = zeroed();
        GetObjectW(info.hbmColor, size_of::<BITMAP>() as i32, &mut bm as *mut _ as *mut c_void);
        let (w, h) = (bm.bmWidth, bm.bmHeight);
        if w <= 0 || h <= 0 {
            free(&info);
            return None;
        }
        let mut bi: BITMAPINFO = zeroed();
        bi.bmiHeader.biSize = size_of::<BITMAPINFOHEADER>() as u32;
        bi.bmiHeader.biWidth = w;
        bi.bmiHeader.biHeight = -h; // top-down
        bi.bmiHeader.biPlanes = 1;
        bi.bmiHeader.biBitCount = 32;
        bi.bmiHeader.biCompression = BI_RGB;
        let mut px = vec![0u8; (w * h * 4) as usize];
        let dc = CreateCompatibleDC(null_mut());
        let lines = GetDIBits(dc, info.hbmColor, 0, h as u32, px.as_mut_ptr() as *mut c_void, &mut bi, DIB_RGB_COLORS);
        DeleteDC(dc);
        free(&info);
        if lines == 0 {
            return None;
        }
        let has_alpha = px.chunks_exact(4).any(|c| c[3] != 0);
        for c in px.chunks_exact_mut(4) {
            c.swap(0, 2); // BGRA -> RGBA
            if !has_alpha {
                c[3] = 255;
            }
        }
        Some((px, w as usize, h as usize))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explorer_by_its_full_path() {
        let e = explorer();
        assert!(e.is_absolute() && e.ends_with("explorer.exe") && e.exists(), "{}", e.display());
    }

    #[test]
    fn services_and_the_system_are_told() {
        // this test: a program of the user, not critical
        let me = std::process::id();
        let exe = std::env::current_exe().unwrap().display().to_string();
        assert!(!is_service(me, Some(&exe)));
        assert!(!is_service(me + 1_000_000_000, Some(&exe)));
        assert!(is_service(me, Some(r"C:\Windows\System32\svchost.exe")));
        assert!(!is_critical(me));
        // the System process (its session may be read only as administrator)
        assert!(is_protected(4, None));
        assert_eq!(exe_name(r"C:\Windows\System32\CSRSS.EXE"), "csrss.exe");
    }
}
