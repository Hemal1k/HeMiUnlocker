//! The optional log (the switch "Logging" in the settings, off by default): what HeMiUnlocker did – unlocking, ended
//! programs, taking ownership, errors – one line each, in log.txt next to the settings. Never the contents of files.
//! Also the processes run as administrator write it, to the settings folder of the account they run as (with UAC
//! confirmed by another account, the log of that account). At most about 1 MB: the older half goes first.
//! The same in all HeMi apps: %AppData%\HeMi\<app>\log.txt, the local time, at most 1 MB (then the older half goes), the
//! folders of the user written as %USERPROFILE%, %AppData% and %LocalAppData%.

use std::io::{Read, Seek, SeekFrom, Write};
use std::os::windows::fs::OpenOptionsExt;
use std::os::windows::io::AsRawHandle;
use std::path::{Path, PathBuf};
use windows_sys::Win32::Storage::FileSystem::{
    BY_HANDLE_FILE_INFORMATION, FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_OPEN_REPARSE_POINT, GetFileInformationByHandle,
};

/// The size the log is kept under.
const MAX: usize = 1024 * 1024;

pub fn file() -> Option<PathBuf> {
    Some(crate::look::dir()?.join("log.txt"))
}

/// One line in the log when logging is on (read from the settings each time: the process may be another one than the
/// window that switched it). A failure to write is ignored – the log never stops the work.
pub fn write(text: &str) {
    if crate::look::Look::load().logging {
        let _ = append(text);
    }
}

fn is_link(p: &Path) -> bool {
    use std::os::windows::fs::MetadataExt;
    std::fs::symlink_metadata(p).is_ok_and(|m| m.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0)
}

fn append(text: &str) -> std::io::Result<()> {
    let Some(path) = file() else { return Ok(()) };
    let dir = path.parent().unwrap_or(Path::new("."));
    std::fs::create_dir_all(dir)?;
    // any program of the user may put a link there: never written through one (an administrator would write elsewhere)
    if is_link(dir) || dir.parent().is_some_and(is_link) {
        return Ok(());
    }
    let mut f = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(&path)?;
    // SAFETY: plain data, all zero is valid
    let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
    // SAFETY: a valid handle of the open file and an out structure of plain data
    let ok = unsafe { GetFileInformationByHandle(f.as_raw_handle() as _, &mut info) } != 0;
    // a link or a second name of another file (a hard link): left alone
    if !ok || info.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0 || info.nNumberOfLinks != 1 {
        return Ok(());
    }
    let line = format!("{}  {}{}\r\n", now(), if crate::procs::self_elevated() { "[admin] " } else { "" }, scrub(text));
    let len = f.metadata()?.len() as usize;
    if len + line.len() > MAX {
        let mut old = Vec::with_capacity(len);
        f.read_to_end(&mut old)?;
        let keep = newer_half(&old).to_vec();
        f.set_len(0)?;
        f.seek(SeekFrom::Start(0))?;
        f.write_all(&keep)?;
    } else {
        f.seek(SeekFrom::End(0))?;
    }
    f.write_all(line.as_bytes())
}

/// The newer half of the log, from the start of a line.
fn newer_half(old: &[u8]) -> &[u8] {
    let from = old.len().saturating_sub(MAX / 2);
    let rest = &old[from..];
    match rest.iter().position(|&b| b == b'\n') {
        Some(i) if from > 0 => &rest[i + 1..],
        _ => rest,
    }
}

/// The local date and time, "YYYY-MM-DD HH:MM:SS".
fn now() -> String {
    // SAFETY: plain data, all zero is valid
    let mut t: windows_sys::Win32::Foundation::SYSTEMTIME = unsafe { std::mem::zeroed() };
    // SAFETY: an out structure of plain data
    unsafe { windows_sys::Win32::System::SystemInformation::GetLocalTime(&mut t) };
    format!("{:04}-{:02}-{:02} {:02}:{:02}:{:02}", t.wYear, t.wMonth, t.wDay, t.wHour, t.wMinute, t.wSecond)
}

/// One line without the folders and the name of the user.
fn scrub(text: &str) -> String {
    let mut s = text.replace(['\r', '\n'], " ");
    // the longer folders first (both AppData folders are inside the profile)
    for (var, name) in [("LOCALAPPDATA", "%LocalAppData%"), ("APPDATA", "%AppData%"), ("USERPROFILE", "%USERPROFILE%")] {
        if let Some(v) = std::env::var_os(var).map(|v| v.to_string_lossy().trim_end_matches('\\').to_string())
            && v.len() > 3
        {
            s = replace_ci(&s, &v, name);
        }
    }
    // the name only as a folder of Users (a name like "HeMi" is also a part of the names of the apps)
    if let Ok(user) = std::env::var("USERNAME")
        && !user.is_empty()
    {
        s = replace_ci(&s, &format!(r"\Users\{user}"), r"\Users\<user>");
    }
    s
}

/// `needle` replaced in any case of its ASCII letters (paths of Windows).
fn replace_ci(hay: &str, needle: &str, with: &str) -> String {
    let mut out = String::with_capacity(hay.len());
    let mut i = 0;
    while i < hay.len() {
        if let Some(part) = hay.get(i..i + needle.len())
            && part.eq_ignore_ascii_case(needle)
        {
            out.push_str(with);
            i += needle.len();
            continue;
        }
        let Some(c) = hay[i..].chars().next() else { break };
        out.push(c);
        i += c.len_utf8();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{replace_ci, scrub};

    #[test]
    fn the_user_folder_is_left_out() {
        assert_eq!(replace_ci(r"C:\USERS\Jan\a.txt – č", r"c:\users\jan", "%USERPROFILE%"), r"%USERPROFILE%\a.txt – č");
        assert_eq!(scrub("a\r\nb"), "a  b");
    }
    use super::*;

    #[test]
    fn the_newer_half_starts_a_line() {
        let line = "2026-11-26 10:00:00  something happened\r\n";
        let old = line.repeat(MAX / line.len() + 10);
        let kept = newer_half(old.as_bytes());
        assert!(kept.len() <= MAX / 2 && kept.len() > MAX / 2 - line.len());
        assert!(kept.starts_with(b"2026-"));
        assert_eq!(newer_half(b"short\n"), b"short\n");
    }

    #[test]
    fn time_has_its_form() {
        let t = now();
        assert_eq!(t.len(), 19, "{t}");
        assert_eq!(&t[4..5], "-");
    }
}
