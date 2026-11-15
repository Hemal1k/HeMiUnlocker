//! The look all HeMi apps in Rust share: the theme (automatic / light / dark) and the language (automatic / English /
//! Czech), chosen at the top of the settings and kept in %AppData%\HeMi\HeMiUnlocker\appearance.ini.

use std::path::PathBuf;

#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct Look {
    /// 0 automatic (HeMiDock, else Windows), 1 light, 2 dark.
    pub theme: u8,
    /// 0 as Windows, 1 English, 2 Czech.
    pub language: u8,
}

fn file() -> Option<PathBuf> {
    Some(PathBuf::from(std::env::var_os("APPDATA")?).join("HeMi").join("HeMiUnlocker").join("appearance.ini"))
}

impl Look {
    pub fn load() -> Look {
        let mut l = Look::default();
        let Some(text) = file().and_then(|f| std::fs::read_to_string(f).ok()) else { return l };
        for line in text.lines() {
            match line.split_once('=') {
                Some(("Theme", v)) => l.theme = v.trim().parse::<u8>().unwrap_or(0).min(2),
                Some(("Language", v)) => l.language = v.trim().parse::<u8>().unwrap_or(0).min(2),
                _ => {}
            }
        }
        l
    }

    pub fn save(&self) {
        if let Some(f) = file() {
            if let Some(dir) = f.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            let _ = std::fs::write(f, format!("Theme={}\nLanguage={}\n", self.theme, self.language));
        }
    }
}