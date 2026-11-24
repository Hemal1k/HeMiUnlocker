//! Languages: English by default, Czech when the display language of Windows is Czech.

use std::sync::OnceLock;
use windows_sys::Win32::Globalization::GetUserDefaultUILanguage;

/// The language chosen in the settings: 0 = as Windows, 1 = English, 2 = Czech.
static CHOICE: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);

pub fn set(choice: u8) {
    CHOICE.store(choice.min(2), std::sync::atomic::Ordering::Relaxed);
}

/// True when the app speaks Czech: chosen in the settings, else as the display language of Windows (primary language 0x05).
pub fn cz() -> bool {
    match CHOICE.load(std::sync::atomic::Ordering::Relaxed) {
        1 => return false,
        2 => return true,
        _ => {}
    }
    static CZ: OnceLock<bool> = OnceLock::new();
    *CZ.get_or_init(|| {
        // Debug builds only: HEMIUNLOCKER_LANG=en / cs to test the other language on this PC
        if cfg!(debug_assertions) {
            match std::env::var("HEMIUNLOCKER_LANG").as_deref() {
                Ok("en") => return false,
                Ok("cs") => return true,
                _ => {}
            }
        }
        (unsafe { GetUserDefaultUILanguage() } & 0x3FF) == 0x05
    })
}

/// The text in the language of the app.
pub fn tr<'a>(en: &'a str, cs: &'a str) -> &'a str {
    if cz() { cs } else { en }
}


/// The text of Windows for an error code (in the language of Windows), with the code.
pub fn os_error(code: u32) -> String {
    std::io::Error::from_raw_os_error(code as i32).to_string()
}

/// The form for a count in Czech: 1 → `one`, 2–4 → `few`, otherwise `many` (English: `few` = `many`).
pub fn plural<'a>(n: usize, one: &'a str, few: &'a str, many: &'a str) -> &'a str {
    match n {
        1 => one,
        2..=4 => few,
        _ => many,
    }
}

/// "{n} item(s)" in the language of the app, with the right form for the count.
pub fn count(n: usize, en: [&str; 2], cs: [&str; 3]) -> String {
    tr(plural(n, en[0], en[1], en[1]), plural(n, cs[0], cs[1], cs[2])).replace("{n}", &n.to_string())
}

#[cfg(test)]
mod tests {
    use super::plural;

    #[test]
    fn czech_plural() {
        let f = |n| plural(n, "položka", "položky", "položek");
        assert_eq!([f(0), f(1), f(2), f(4), f(5), f(22)], ["položek", "položka", "položky", "položky", "položek", "položek"]);
    }
}
