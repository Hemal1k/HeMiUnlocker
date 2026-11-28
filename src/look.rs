//! The look all HeMi apps in Rust share: the theme (automatic / light / dark) and the language (automatic / English /
//! Czech), chosen at the top of the settings and kept in %AppData%\HeMi\HeMiUnlocker\appearance.ini.

use std::path::PathBuf;

#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct Look {
    /// 0 automatic (as Windows), 1 light, 2 dark.
    pub theme: u8,
    /// 0 as Windows, 1 English, 2 Czech.
    pub language: u8,
    /// What HeMiUnlocker does written to Logs\log-<day>.txt in the settings folder (off by default).
    pub logging: bool,
}

/// %AppData%\HeMi\HeMiUnlocker: the settings and the log (of the account the process runs as).
pub fn dir() -> Option<PathBuf> {
    Some(PathBuf::from(std::env::var_os("APPDATA")?).join("HeMi").join("HeMiUnlocker"))
}

fn file() -> Option<PathBuf> {
    Some(dir()?.join("appearance.ini"))
}

impl Look {
    pub fn load() -> Look {
        file().and_then(|f| std::fs::read_to_string(f).ok()).map(|t| Look::parse(&t)).unwrap_or_default()
    }

    fn parse(text: &str) -> Look {
        let mut l = Look::default();
        for line in text.lines() {
            match line.split_once('=') {
                Some(("Theme", v)) => l.theme = v.trim().parse::<u8>().unwrap_or(0).min(2),
                Some(("Language", v)) => l.language = v.trim().parse::<u8>().unwrap_or(0).min(2),
                Some(("Logging", v)) => l.logging = v.trim() == "1",
                _ => {}
            }
        }
        l
    }

    /// Written to a new file first and then moved over the old one: a crash halfway leaves the old settings whole.
    pub fn save(&self) -> std::io::Result<()> {
        let Some(f) = file() else { return Ok(()) };
        if let Some(dir) = f.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = f.with_extension("ini.tmp");
        std::fs::write(&tmp, format!("Theme={}\nLanguage={}\nLogging={}\n", self.theme, self.language, u8::from(self.logging)))?;
        std::fs::rename(&tmp, &f)
    }
}

#[cfg(test)]
mod tests {
    use super::Look;

    #[test]
    fn logging_is_off_unless_written_on() {
        assert!(!Look::parse("Theme=2\nLanguage=1\n").logging);
        assert!(!Look::parse("Logging=0\n").logging);
        let l = Look::parse("Theme=2\nLanguage=1\nLogging=1\n");
        assert_eq!((l.theme, l.language, l.logging), (2, 1, true));
    }
}