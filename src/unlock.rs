//! "Unlock" from the File Explorer menu, without the window: the programs that hold the file are asked to close
//! (so they can still save), what does not close within a few seconds is ended; File Explorer is restarted.
//! Services, the processes of the system and the critical ones are never ended from here (the notification names them).
//! What cannot be ended without administrator rights: HeMiUnlocker starts again as administrator (Windows asks)
//! and tries once more. At the end a notification of Windows says whether the file is free.

use crate::i18n::{count, tr};
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
    let (outcome, spared) = unlock(&targets);
    // still held (not only by what is spared) and not yet administrator: once more as administrator (it notifies itself)
    let only_spared = matches!(&outcome, Outcome::Held(names) if names.iter().all(|n| spared.contains(n)));
    if matches!(outcome, Outcome::Held(_) | Outcome::Unknown) && !only_spared && !elevated && !procs::self_elevated() && relaunch_elevated(SWITCH, &targets) {
        return;
    }
    notify(&targets, outcome, &spared);
}

/// The name shown for a holder.
fn shown_name(l: &locks::Locker) -> String {
    if l.name.trim().is_empty() { format!("PID {}", l.pid) } else { l.name.clone() }
}

/// What the menu item does not end without asking: services, the processes of the system and the critical ones.
fn spared(l: &locks::Locker, exe: Option<&str>) -> bool {
    matches!(l.kind, AppKind::Critical | AppKind::Service) || procs::is_service(l.pid, exe) || procs::is_protected(l.pid, exe)
}

/// The outcome and the names of the holders left running because they are services or of the system.
fn unlock(targets: &[PathBuf]) -> (Outcome, Vec<String>) {
    let scan = locks::scan(targets);
    if let Some(e) = scan.error {
        return (Outcome::Failed(e), Vec::new());
    }
    if scan.lockers.is_empty() {
        return (if scan.locked_unknown { Outcome::Unknown } else { Outcome::WasFree }, Vec::new());
    }
    // first all are asked to close (at the same time), then the rest is ended; services and the system are left alone
    // (ending them may stop Windows or lose data) – the window can end them after a question
    let mut explorer = Vec::new();
    let mut ordinary = Vec::new();
    let mut spared_names = Vec::new();
    for l in &scan.lockers {
        let exe = procs::exe_path(l.pid);
        if l.kind == AppKind::Explorer {
            explorer.push(l.clone());
        } else if spared(l, exe.as_deref()) {
            spared_names.push(shown_name(l));
        } else {
            procs::post_close(l.pid);
            ordinary.push(l.clone());
        }
    }
    for l in &ordinary {
        if !procs::wait_exit(l.pid, 5000) {
            let _ = procs::terminate(l.pid, l.start_time);
        }
    }
    for l in explorer {
        let _ = procs::restart_explorer(l.pid, l.start_time);
    }
    std::thread::sleep(std::time::Duration::from_millis(400));
    let again = locks::scan(targets);
    spared_names.dedup();
    let outcome = if !again.lockers.is_empty() {
        let mut names: Vec<String> = again.lockers.iter().map(shown_name).collect();
        names.dedup();
        Outcome::Held(names)
    } else if again.locked_unknown {
        Outcome::Unknown
    } else {
        Outcome::Free
    };
    (outcome, spared_names)
}

/// HeMiUnlocker again as administrator with the same files; false when Windows (or the user) said no.
pub fn relaunch_elevated(switch: &str, targets: &[PathBuf]) -> bool {
    let Ok(exe) = std::env::current_exe() else { return false };
    let mut params = format!("{switch} {ELEVATED}");
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

pub fn name_of(targets: &[PathBuf]) -> String {
    match targets {
        // one item in quotes, more as a count
        [one] => {
            let n = one.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| one.display().to_string());
            tr("“{n}”", "„{n}“").replace("{n}", &n)
        }
        many => count(many.len(), ["{n} item", "{n} items"], ["{n} položka", "{n} položky", "{n} položek"]),
    }
}

/// The name HeMiUnlocker shows its notifications under (registered for the user with its name and icon).
const APP_ID: &str = "HeMi.HeMiUnlocker";

/// A notification of Windows (a toast, as other apps have); false when Windows did not take it.
pub fn toast(title: &str, text: &str) -> bool {
    use windows::Data::Xml::Dom::XmlDocument;
    use windows::UI::Notifications::{ToastNotification, ToastNotificationManager};
    use windows::Win32::System::WinRT::{RO_INIT_MULTITHREADED, RoInitialize};
    use windows::core::HSTRING;
    // the app id with the name and the icon of HeMiUnlocker (the icon as a file next to the settings of the user)
    let icon = std::env::var_os("LOCALAPPDATA").map(|d| PathBuf::from(d).join("HeMi").join("HeMiUnlocker").join("HeMiUnlocker.png"));
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
fn notify(targets: &[PathBuf], outcome: Outcome, spared: &[String]) {
    let name = name_of(targets);
    let (title, mut text, flag) = match outcome {
        Outcome::Free => (tr("Unlocked", "Odemčeno"), tr("Nothing holds {name} now.", "{name} už nic nedrží.").replace("{name}", &name), NIIF_INFO),
        Outcome::WasFree => (
            tr("Nothing to unlock", "Není co odemykat"),
            tr("No program holds {name}.", "{name} nedrží žádný program.").replace("{name}", &name),
            NIIF_INFO,
        ),
        Outcome::Held(names) => (
            tr("Could not unlock", "Nepodařilo se odemknout"),
            tr("Still holding {name}: {who}", "{name} pořád drží: {who}").replace("{name}", &name).replace("{who}", &names.join(", ")),
            NIIF_ERROR,
        ),
        Outcome::Unknown => (
            tr("Could not unlock", "Nepodařilo se odemknout"),
            tr("Something Windows does not show holds {name} (a driver or a system service).", "{name} drží něco, co Windows neukazuje (ovladač nebo systémová služba).")
                .replace("{name}", &name),
            NIIF_WARNING,
        ),
        Outcome::Failed(e) => (tr("Could not unlock", "Nepodařilo se odemknout"), e, NIIF_ERROR),
    };
    if !spared.is_empty() {
        text.push('\n');
        text.push_str(
            &tr(
                "Not ended (a service or part of Windows): {who}. The window of HeMiUnlocker shows what can be done.",
                "Neukončeno (služba nebo součást Windows): {who}. Co s tím jde dělat, ukáže okno HeMiUnlockeru.",
            )
            .replace("{who}", &spared.join(", ")),
        );
    }
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
