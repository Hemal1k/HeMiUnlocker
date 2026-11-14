//! "Unlock" from the File Explorer menu, without the window: the programs that hold the file are asked to close
//! (so they can still save), what does not close within a few seconds is ended; File Explorer is restarted.
//! What cannot be ended without administrator rights: HeMiUnlocker starts again as administrator (Windows asks)
//! and tries once more. At the end a notification of Windows says whether the file is free.

use crate::i18n::tr;
use crate::locks::{self, AppKind};
use crate::procs::{self, wide};
use std::path::{Path, PathBuf};
use std::ptr::{null, null_mut};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Shell::{
    NIF_ICON, NIF_INFO, NIF_TIP, NIIF_ERROR, NIIF_INFO, NIIF_LARGE_ICON, NIIF_WARNING, NIM_ADD, NIM_DELETE, NOTIFYICONDATAW,
    SEE_MASK_NOASYNC, SHELLEXECUTEINFOW, ShellExecuteExW, Shell_NotifyIconW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DestroyWindow, HWND_MESSAGE, IDI_APPLICATION, LoadIconW, SW_SHOWNORMAL,
};

/// The switch of the File Explorer item; `--elevated` marks the second try as administrator.
pub const SWITCH: &str = "--unlock";
pub const ELEVATED: &str = "--elevated";

enum Outcome {
    Free,
    /// Nothing held it.
    WasFree,
    /// Still held: the names of the programs.
    Held(Vec<String>),
    /// Held by something Windows does not tell.
    Unknown,
    Failed(String),
}

pub fn run(targets: Vec<PathBuf>, elevated: bool) {
    let outcome = unlock(&targets);
    // still held and not yet administrator: once more as administrator (it notifies itself)
    if matches!(outcome, Outcome::Held(_) | Outcome::Unknown) && !elevated && !procs::self_elevated() && relaunch_elevated(&targets) {
        return;
    }
    notify(&targets, outcome);
}

fn unlock(targets: &[PathBuf]) -> Outcome {
    let scan = locks::scan(targets);
    if let Some(e) = scan.error {
        return Outcome::Failed(e);
    }
    if scan.lockers.is_empty() {
        return if scan.locked_unknown { Outcome::Unknown } else { Outcome::WasFree };
    }
    // first all are asked to close (at the same time), then the rest is ended
    let mut explorer = Vec::new();
    for l in &scan.lockers {
        match l.kind {
            AppKind::Critical => {}
            AppKind::Explorer => explorer.push(l.clone()),
            _ => {
                procs::post_close(l.pid);
            }
        }
    }
    for l in &scan.lockers {
        if matches!(l.kind, AppKind::Critical | AppKind::Explorer) {
            continue;
        }
        let exe = procs::exe_path(l.pid);
        if !procs::wait_exit(l.pid, 5000) && !procs::is_protected(l.pid, exe.as_deref()) {
            let _ = procs::terminate(l.pid, l.start_time);
        }
    }
    for l in explorer {
        let _ = procs::restart_explorer(l.pid, l.start_time);
    }
    std::thread::sleep(std::time::Duration::from_millis(400));
    let again = locks::scan(targets);
    if !again.lockers.is_empty() {
        let mut names: Vec<String> = again.lockers.iter().map(|l| if l.name.trim().is_empty() { format!("PID {}", l.pid) } else { l.name.clone() }).collect();
        names.dedup();
        Outcome::Held(names)
    } else if again.locked_unknown {
        Outcome::Unknown
    } else {
        Outcome::Free
    }
}

/// HeMiUnlocker again as administrator with the same files; false when Windows (or the user) said no.
fn relaunch_elevated(targets: &[PathBuf]) -> bool {
    let Ok(exe) = std::env::current_exe() else { return false };
    let mut params = format!("{SWITCH} {ELEVATED}");
    for t in targets {
        params.push_str(&format!(" \"{}\"", t.display()));
    }
    let (verb, exe, params) = (wide("runas"), wide(&exe.to_string_lossy()), wide(&params));
    let mut info: SHELLEXECUTEINFOW = unsafe { std::mem::zeroed() };
    info.cbSize = size_of::<SHELLEXECUTEINFOW>() as u32;
    info.fMask = SEE_MASK_NOASYNC;
    info.lpVerb = verb.as_ptr();
    info.lpFile = exe.as_ptr();
    info.lpParameters = params.as_ptr();
    info.nShow = SW_SHOWNORMAL;
    unsafe { ShellExecuteExW(&mut info) != 0 }
}

fn name_of(targets: &[PathBuf]) -> String {
    match targets {
        [one] => one.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| one.display().to_string()),
        many => tr("{n} items", "{n} položek").replace("{n}", &many.len().to_string()),
    }
}

/// The name HeMiUnlocker shows its notifications under (registered for the user with its name and icon).
const APP_ID: &str = "HeMi.HeMiUnlocker";

/// A notification of Windows (a toast, as other apps have); false when Windows did not take it.
fn toast(title: &str, text: &str) -> bool {
    use windows::Data::Xml::Dom::XmlDocument;
    use windows::UI::Notifications::{ToastNotification, ToastNotificationManager};
    use windows::Win32::System::WinRT::{RO_INIT_MULTITHREADED, RoInitialize};
    use windows::core::HSTRING;
    // the app id with the name and the icon of HeMiUnlocker (the icon as a file next to the settings of the user)
    let icon = std::env::var_os("LOCALAPPDATA").map(|d| PathBuf::from(d).join("HeMiUnlocker").join("HeMiUnlocker.png"));
    if let Some(icon) = &icon
        && !icon.exists()
    {
        let _ = std::fs::create_dir_all(icon.parent().unwrap());
        let _ = std::fs::write(icon, include_bytes!("../assets/HeMiUnlocker.png"));
    }
    let key = format!(r"Software\Classes\AppUserModelId\{APP_ID}");
    let _ = crate::shell_menu::write(&key, Some("DisplayName"), "HeMiUnlocker");
    if let Some(icon) = &icon {
        let _ = crate::shell_menu::write(&key, Some("IconUri"), &icon.display().to_string());
    }
    let esc = |s: &str| s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;");
    let xml = format!(
        r#"<toast><visual><binding template="ToastGeneric"><text>{}</text><text>{}</text></binding></visual></toast>"#,
        esc(title),
        esc(text)
    );
    let shown = (|| -> windows::core::Result<()> {
        unsafe {
            let _ = RoInitialize(RO_INIT_MULTITHREADED);
        }
        let doc = XmlDocument::new()?;
        doc.LoadXml(&HSTRING::from(xml))?;
        let note = ToastNotification::CreateToastNotification(&doc)?;
        ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from(APP_ID))?.Show(&note)
    })();
    if shown.is_ok() {
        // a moment for Windows to take it before the process ends
        std::thread::sleep(std::time::Duration::from_millis(800));
    }
    shown.is_ok()
}

/// The result as a notification of Windows; the balloon of a short-lived notification-area icon when toasts fail.
fn notify(targets: &[PathBuf], outcome: Outcome) {
    let name = name_of(targets);
    let (title, text, flag) = match outcome {
        Outcome::Free => (tr("Unlocked", "Odemčeno"), tr("{name} is free now.", "{name} je teď volný.").replace("{name}", &name), NIIF_INFO),
        Outcome::WasFree => (
            tr("Nothing to unlock", "Není co odemykat"),
            tr("No program holds {name}.", "{name} nedrží žádný program.").replace("{name}", &name),
            NIIF_INFO,
        ),
        Outcome::Held(names) => (
            tr("Could not unlock", "Nepodařilo se odemknout"),
            tr("{name} is still held by: {who}", "{name} pořád drží: {who}").replace("{name}", &name).replace("{who}", &names.join(", ")),
            NIIF_ERROR,
        ),
        Outcome::Unknown => (
            tr("Could not unlock", "Nepodařilo se odemknout"),
            tr("{name} is held by something Windows does not show (a driver or a system service).", "{name} drží něco, co Windows neukazuje (ovladač nebo systémová služba).")
                .replace("{name}", &name),
            NIIF_WARNING,
        ),
        Outcome::Failed(e) => (tr("Could not unlock", "Nepodařilo se odemknout"), e, NIIF_ERROR),
    };
    if toast(title, &text) {
        return;
    }
    unsafe {
        // an invisible window that owns the icon for the time of the notification
        let class = wide("STATIC");
        let hwnd = CreateWindowExW(0, class.as_ptr(), null(), 0, 0, 0, 0, 0, HWND_MESSAGE, null_mut(), GetModuleHandleW(null()), null());
        let mut nid: NOTIFYICONDATAW = std::mem::zeroed();
        nid.cbSize = size_of::<NOTIFYICONDATAW>() as u32;
        nid.hWnd = hwnd;
        nid.uID = 1;
        nid.uFlags = NIF_ICON | NIF_TIP | NIF_INFO;
        // the icon of HeMiUnlocker.exe (resource 1), else the one of Windows
        let icon = LoadIconW(GetModuleHandleW(null()), 1 as _);
        nid.hIcon = if icon.is_null() { LoadIconW(null_mut(), IDI_APPLICATION) } else { icon };
        copy(&mut nid.szTip, "HeMiUnlocker");
        copy(&mut nid.szInfoTitle, title);
        copy(&mut nid.szInfo, &text);
        nid.dwInfoFlags = flag | NIIF_LARGE_ICON;
        if Shell_NotifyIconW(NIM_ADD, &nid) != 0 {
            // shown as a notification of Windows; the icon may go after a moment, the notification stays in the centre
            std::thread::sleep(std::time::Duration::from_secs(6));
            Shell_NotifyIconW(NIM_DELETE, &nid);
        }
        DestroyWindow(hwnd);
    }
}

fn copy<const N: usize>(to: &mut [u16; N], text: &str) {
    let w: Vec<u16> = text.encode_utf16().take(N - 1).collect();
    to[..w.len()].copy_from_slice(&w);
    to[w.len()] = 0;
}

/// The files after `--unlock [--elevated]`.
pub fn args(rest: &[String]) -> (Vec<PathBuf>, bool) {
    let elevated = rest.iter().any(|a| a == ELEVATED);
    let paths = rest
        .iter()
        .filter(|a| a.as_str() != ELEVATED)
        .map(PathBuf::from)
        .filter(|p| Path::new(p).exists())
        .map(|p| std::path::absolute(&p).unwrap_or(p))
        .collect();
    (paths, elevated)
}
