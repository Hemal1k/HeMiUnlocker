//! Languages: English by default, Czech when the display language of Windows is Czech.

use std::sync::OnceLock;
use windows_sys::Win32::Globalization::GetUserDefaultUILanguage;

/// True when Windows shows its texts in Czech (primary language 0x05).
pub fn cz() -> bool {
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

/// The build date (YYYY-MM-DD from build.rs) as Czech or English write it.
pub fn build_date() -> String {
    let iso = env!("HEMIUNLOCKER_BUILD_DATE");
    let mut parts = iso.split('-');
    let (Some(y), Some(m), Some(d)) = (parts.next(), parts.next(), parts.next()) else { return iso.into() };
    if cz() {
        format!("{d}.{m}.{y}")
    } else {
        const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
        let month = m.parse::<usize>().ok().and_then(|m| MONTHS.get(m.wrapping_sub(1))).copied().unwrap_or(m);
        format!("{} {month} {y}", d.trim_start_matches('0'))
    }
}
