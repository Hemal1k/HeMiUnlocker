//! Which processes hold a file: the Restart Manager of Windows (the same thing Windows asks when it says
//! "the file is open in another program"). It needs no administrator rights.

use crate::i18n::tr;
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::ptr::{null, null_mut};
use windows_sys::Win32::Foundation::{
    CloseHandle, ERROR_LOCK_VIOLATION, ERROR_MORE_DATA, ERROR_SHARING_VIOLATION, ERROR_SUCCESS,
    GENERIC_READ, GetLastError, INVALID_HANDLE_VALUE,
};
use windows_sys::Win32::Storage::FileSystem::{CreateFileW, FILE_ATTRIBUTE_NORMAL, OPEN_EXISTING};
use windows_sys::Win32::System::RestartManager::*;

/// What kind of program the Restart Manager says it is.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AppKind {
    Window,
    OtherWindow,
    Service,
    Explorer,
    Console,
    Critical,
    Unknown,
}

impl AppKind {
    pub fn label(self) -> &'static str {
        match self {
            AppKind::Window | AppKind::OtherWindow => tr("App", "Aplikace"),
            AppKind::Service => tr("Service", "Služba"),
            AppKind::Explorer => tr("File Explorer", "Průzkumník"),
            AppKind::Console => tr("Console", "Konzole"),
            AppKind::Critical => tr("System", "Systém"),
            AppKind::Unknown => tr("Process", "Proces"),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Locker {
    pub pid: u32,
    /// The start of the process (FILETIME), so that a PID used again by another process is never ended.
    pub start_time: u64,
    pub name: String,
    pub service: String,
    pub kind: AppKind,
}

#[derive(Default)]
pub struct Scan {
    pub lockers: Vec<Locker>,
    /// The folder had more files than [`MAX_FILES`].
    pub truncated: bool,
    /// Nothing found, but the file cannot be opened: something the Restart Manager does not see holds it.
    pub locked_unknown: bool,
    pub error: Option<String>,
}

pub const MAX_FILES: usize = 20_000;

pub fn scan(targets: &[PathBuf]) -> Scan {
    let (files, truncated) = collect_files(targets);
    let mut scan = Scan { truncated, ..Default::default() };
    if files.is_empty() {
        return scan;
    }
    match query(&files) {
        Ok(lockers) => scan.lockers = lockers,
        Err(e) => scan.error = Some(e),
    }
    if scan.lockers.is_empty() && scan.error.is_none() {
        scan.locked_unknown = targets.iter().filter(|t| t.is_file()).any(|t| is_in_use(t));
    }
    scan
}

/// The files to check: the files themselves and all files in the folders (at most MAX_FILES).
fn collect_files(targets: &[PathBuf]) -> (Vec<PathBuf>, bool) {
    let mut out = Vec::new();
    let mut dirs = Vec::new();
    for t in targets {
        if t.is_dir() {
            dirs.push(t.clone());
        } else {
            out.push(t.clone());
        }
    }
    while let Some(dir) = dirs.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for e in entries.flatten() {
            let Ok(ft) = e.file_type() else { continue };
            if ft.is_symlink() {
                continue;
            }
            if ft.is_dir() {
                dirs.push(e.path());
            } else {
                if out.len() >= MAX_FILES {
                    return (out, true);
                }
                out.push(e.path());
            }
        }
    }
    (out, false)
}

fn wide(p: &Path) -> Vec<u16> {
    p.as_os_str().encode_wide().chain(Some(0)).collect()
}

pub fn from_wide(b: &[u16]) -> String {
    let n = b.iter().position(|&c| c == 0).unwrap_or(b.len());
    String::from_utf16_lossy(&b[..n])
}

#[allow(non_upper_case_globals)]
fn query(files: &[PathBuf]) -> Result<Vec<Locker>, String> {
    unsafe {
        let mut session = 0u32;
        let mut key = [0u16; CCH_RM_SESSION_KEY as usize + 1];
        let r = RmStartSession(&mut session, 0, key.as_mut_ptr());
        if r != ERROR_SUCCESS {
            return Err(format!("{} ({r}).", tr("The Windows Restart Manager did not start", "Správce restartů Windows se nespustil – chyba")));
        }
        let infos = list(session, files);
        RmEndSession(session);
        let mut out: Vec<Locker> = Vec::new();
        for i in infos? {
            let pid = i.Process.dwProcessId;
            if out.iter().any(|l| l.pid == pid) {
                continue;
            }
            let ft = i.Process.ProcessStartTime;
            out.push(Locker {
                pid,
                start_time: ((ft.dwHighDateTime as u64) << 32) | ft.dwLowDateTime as u64,
                name: from_wide(&i.strAppName),
                service: from_wide(&i.strServiceShortName),
                kind: match i.ApplicationType {
                    RmMainWindow => AppKind::Window,
                    RmOtherWindow => AppKind::OtherWindow,
                    RmService => AppKind::Service,
                    RmExplorer => AppKind::Explorer,
                    RmConsole => AppKind::Console,
                    RmCritical => AppKind::Critical,
                    _ => AppKind::Unknown,
                },
            });
        }
        Ok(out)
    }
}

unsafe fn list(session: u32, files: &[PathBuf]) -> Result<Vec<RM_PROCESS_INFO>, String> {
    let wides: Vec<Vec<u16>> = files.iter().map(|f| wide(f)).collect();
    for chunk in wides.chunks(1000) {
        let ptrs: Vec<*const u16> = chunk.iter().map(|w| w.as_ptr()).collect();
        let r = unsafe { RmRegisterResources(session, ptrs.len() as u32, ptrs.as_ptr(), 0, null(), 0, null()) };
        if r != ERROR_SUCCESS {
            return Err(format!("{} ({r}).", tr("Windows could not check the file", "Soubor se nepodařilo předat Windows ke kontrole – chyba")));
        }
    }
    let mut infos: Vec<RM_PROCESS_INFO> = Vec::new();
    // the list can grow between the two calls: a few tries
    for _ in 0..5 {
        let mut needed = 0u32;
        let mut count = infos.len() as u32;
        let mut reasons = 0u32;
        let buf = if infos.is_empty() { null_mut() } else { infos.as_mut_ptr() };
        let r = unsafe { RmGetList(session, &mut needed, &mut count, buf, &mut reasons) };
        if r == ERROR_SUCCESS {
            infos.truncate(count as usize);
            return Ok(infos);
        }
        if r != ERROR_MORE_DATA {
            return Err(format!("{} ({r}).", tr("The list of programs could not be read", "Seznam programů se nepodařilo získat – chyba")));
        }
        infos = vec![unsafe { std::mem::zeroed() }; needed as usize + 4];
    }
    Err(tr("The list of programs kept changing – try Refresh.", "Seznam programů se stále měnil – zkus Obnovit.").into())
}

/// The file cannot be opened only for us: someone has it open.
pub fn is_in_use(p: &Path) -> bool {
    let w = wide(p);
    unsafe {
        let h = CreateFileW(w.as_ptr(), GENERIC_READ, 0, null(), OPEN_EXISTING, FILE_ATTRIBUTE_NORMAL, null_mut());
        if h == INVALID_HANDLE_VALUE {
            let e = GetLastError();
            return e == ERROR_SHARING_VIOLATION || e == ERROR_LOCK_VIOLATION;
        }
        CloseHandle(h);
        false
    }
}
