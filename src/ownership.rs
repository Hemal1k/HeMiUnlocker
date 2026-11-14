//! "Take ownership" (from the File Explorer submenu or the window): the user becomes the owner of a file or folder
//! (with everything in it) and the Administrators get full control – what the well-known "Take Ownership" registry
//! tweak does with takeown and icacls. It needs administrator rights: HeMiUnlocker starts again as administrator
//! (Windows asks). A notification tells the result.

use crate::i18n::tr;
use crate::unlock::{ELEVATED, name_of, relaunch_elevated, toast};
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;

pub const SWITCH: &str = "--take-ownership";

/// No console window for takeown / icacls.
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// The Administrators group by its SID: the same on every language of Windows.
const ADMINISTRATORS: &str = "*S-1-5-32-544";

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

/// takeown makes the user the owner, icacls grants the Administrators full control (for a folder: everything in it).
fn take(path: &Path) -> Result<(), String> {
    let p = path.display().to_string();
    let dir = path.is_dir();
    let mut takeown = Command::new("takeown.exe");
    takeown.arg("/F").arg(&p);
    if dir {
        takeown.args(["/R", "/D", "Y"]);
    }
    let a = takeown.creation_flags(CREATE_NO_WINDOW).output().map_err(|e| e.to_string())?;
    let mut icacls = Command::new("icacls.exe");
    icacls.arg(&p).args(["/grant", &format!("{ADMINISTRATORS}:F")]);
    if dir {
        icacls.args(["/T", "/C"]);
    }
    let b = icacls.creation_flags(CREATE_NO_WINDOW).output().map_err(|e| e.to_string())?;
    if a.status.success() && b.status.success() {
        Ok(())
    } else {
        let out = |o: &std::process::Output| String::from_utf8_lossy(&o.stderr).trim().to_string() + &String::from_utf8_lossy(&o.stdout).trim().to_string();
        let detail = if !a.status.success() { out(&a) } else { out(&b) };
        Err(format!("{}: {}", path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or(p), detail.lines().last().unwrap_or("").trim()))
    }
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
