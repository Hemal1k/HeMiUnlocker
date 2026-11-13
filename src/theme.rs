//! The look of HeMiDock (WinUI 3 / Fluent) in egui: the colours of the Windows 11 theme, the accent colour,
//! Segoe UI Variable and Segoe Fluent Icons, the title bar in the colour of the window.
//!
//! Light / dark and the accent follow the settings of HeMiDock (%AppData%\HeMiDock\settings.json:
//! ThemeMode 0 = Windows, 1 = light, 2 = dark; AccentMode 0 = Windows, 1 = AccentColor), else Windows.

use eframe::egui::{self, Color32, FontData, FontDefinitions, FontFamily, FontId, Stroke, TextStyle};
use eframe::epaint::text::VariationCoords;
use std::ptr::null_mut;
use std::sync::Arc;
use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::Graphics::Dwm::{
    DWMWA_CAPTION_COLOR, DWMWA_TEXT_COLOR, DWMWA_USE_IMMERSIVE_DARK_MODE, DwmSetWindowAttribute,
};
use windows_sys::Win32::System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_BINARY, RRF_RT_REG_DWORD, RegGetValueW};

pub const SEMIBOLD: &str = "semibold";
pub const DISPLAY: &str = "display";
pub const ICONS: &str = "icons";

pub fn body(size: f32) -> FontId {
    FontId::new(size, FontFamily::Proportional)
}
pub fn semibold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(SEMIBOLD.into()))
}
pub fn display(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(DISPLAY.into()))
}
pub fn icon(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(ICONS.into()))
}

/// Segoe Fluent Icons glyphs.
pub mod glyph {
    pub const DOCUMENT: char = '\u{E8A5}';
    pub const FOLDER: char = '\u{E8B7}';
    pub const OPEN_FOLDER: char = '\u{E838}';
    pub const OPEN_FILE: char = '\u{E8E5}';
    pub const REFRESH: char = '\u{E72C}';
    pub const CLOSE: char = '\u{E711}';
    pub const LIGHTNING: char = '\u{E945}';
    pub const SHIELD: char = '\u{EA18}';
    pub const COMPLETED: char = '\u{E930}';
    pub const INFO: char = '\u{E946}';
    pub const BACK: char = '\u{E72B}';
    pub const WARNING: char = '\u{E7BA}';
    pub const ERROR: char = '\u{EA39}';
    pub const APP: char = '\u{ECAA}';
    pub const GEAR: char = '\u{E713}';
    pub const CONSOLE: char = '\u{E756}';
    pub const UNLOCK: char = '\u{E785}';
    pub const RESTART: char = '\u{E777}';
}

/// Everything the colours depend on; read again every few seconds and compared.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Source {
    pub light: bool,
    /// Light3, Light2, Light1, accent, Dark1, Dark2, Dark3
    pub accent: [Color32; 7],
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Palette {
    pub light: bool,
    pub bg: Color32,
    pub card: Color32,
    pub card_stroke: Color32,
    pub dialog: Color32,
    pub dialog_foot: Color32,
    pub text: Color32,
    pub text2: Color32,
    pub text3: Color32,
    pub control: Color32,
    pub control_hover: Color32,
    pub control_pressed: Color32,
    pub control_stroke: Color32,
    pub control_stroke_bottom: Color32,
    pub control_disabled: Color32,
    pub subtle_hover: Color32,
    pub subtle_pressed: Color32,
    pub accent: Color32,
    pub accent_hover: Color32,
    pub accent_pressed: Color32,
    pub on_accent: Color32,
    pub accent_text: Color32,
    pub critical: Color32,
    pub critical_bg: Color32,
    pub success: Color32,
    pub success_bg: Color32,
    pub caution: Color32,
    pub caution_bg: Color32,
    pub info_bg: Color32,
    pub smoke: Color32,
}

const fn rgb(r: u8, g: u8, b: u8) -> Color32 {
    Color32::from_rgb(r, g, b)
}

pub fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    rgb(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()))
}

pub fn palette(s: &Source) -> Palette {
    let a = s.accent;
    if s.light {
        let bg = rgb(243, 243, 243);
        let accent = a[4];
        Palette {
            light: true,
            bg,
            card: rgb(251, 251, 251),
            card_stroke: rgb(229, 229, 229),
            dialog: rgb(255, 255, 255),
            dialog_foot: rgb(243, 243, 243),
            text: rgb(27, 27, 27),
            text2: rgb(96, 96, 96),
            text3: rgb(142, 142, 142),
            control: rgb(253, 253, 253),
            control_hover: rgb(248, 248, 248),
            control_pressed: rgb(245, 245, 245),
            control_stroke: rgb(229, 229, 229),
            control_stroke_bottom: rgb(204, 204, 204),
            control_disabled: rgb(245, 245, 245),
            subtle_hover: Color32::from_black_alpha(10),
            subtle_pressed: Color32::from_black_alpha(6),
            accent,
            accent_hover: mix(accent, bg, 0.10),
            accent_pressed: mix(accent, bg, 0.20),
            on_accent: Color32::WHITE,
            accent_text: a[5],
            critical: rgb(196, 43, 28),
            critical_bg: rgb(253, 231, 233),
            success: rgb(15, 123, 15),
            success_bg: rgb(223, 246, 221),
            caution: rgb(157, 93, 0),
            caution_bg: rgb(255, 244, 206),
            info_bg: rgb(246, 246, 246),
            smoke: Color32::from_black_alpha(77),
        }
    } else {
        let bg = rgb(32, 32, 32);
        let accent = a[1];
        Palette {
            light: false,
            bg,
            card: rgb(43, 43, 43),
            card_stroke: rgb(29, 29, 29),
            dialog: rgb(43, 43, 43),
            dialog_foot: rgb(32, 32, 32),
            text: rgb(255, 255, 255),
            text2: rgb(200, 200, 200),
            text3: rgb(139, 139, 139),
            control: rgb(50, 50, 50),
            control_hover: rgb(56, 56, 56),
            control_pressed: rgb(45, 45, 45),
            control_stroke: rgb(58, 58, 58),
            control_stroke_bottom: rgb(48, 48, 48),
            control_disabled: rgb(46, 46, 46),
            subtle_hover: Color32::from_white_alpha(15),
            subtle_pressed: Color32::from_white_alpha(10),
            accent,
            accent_hover: mix(accent, bg, 0.10),
            accent_pressed: mix(accent, bg, 0.20),
            on_accent: Color32::BLACK,
            accent_text: a[0],
            critical: rgb(255, 153, 164),
            critical_bg: rgb(68, 39, 38),
            success: rgb(108, 203, 95),
            success_bg: rgb(57, 61, 27),
            caution: rgb(252, 225, 0),
            caution_bg: rgb(67, 53, 25),
            info_bg: rgb(39, 39, 39),
            smoke: Color32::from_black_alpha(77),
        }
    }
}

// ---------------------------------------------------------------- where the colours come from

fn reg_dword(key: &str, value: &str) -> Option<u32> {
    let (k, v) = (crate::procs::wide(key), crate::procs::wide(value));
    let mut data = 0u32;
    let mut size = 4u32;
    let r = unsafe {
        RegGetValueW(HKEY_CURRENT_USER, k.as_ptr(), v.as_ptr(), RRF_RT_REG_DWORD, null_mut(), &mut data as *mut u32 as _, &mut size)
    };
    (r == 0).then_some(data)
}

fn reg_binary(key: &str, value: &str) -> Option<Vec<u8>> {
    let (k, v) = (crate::procs::wide(key), crate::procs::wide(value));
    let mut data = vec![0u8; 64];
    let mut size = data.len() as u32;
    let r = unsafe {
        RegGetValueW(HKEY_CURRENT_USER, k.as_ptr(), v.as_ptr(), RRF_RT_REG_BINARY, null_mut(), data.as_mut_ptr() as _, &mut size)
    };
    (r == 0).then(|| {
        data.truncate(size as usize);
        data
    })
}

/// A value from HeMiDock's settings.json (a flat object; no JSON library needed).
fn setting<'a>(json: &'a str, key: &str) -> Option<&'a str> {
    let k = format!("\"{key}\"");
    let rest = &json[json.find(&k)? + k.len()..];
    let rest = rest.trim_start().strip_prefix(':')?.trim_start();
    let end = rest.find([',', '\n', '}'])?;
    Some(rest[..end].trim().trim_matches('"'))
}

fn shades(base: Color32) -> [Color32; 7] {
    let (w, b) = (Color32::WHITE, Color32::BLACK);
    [mix(base, w, 0.72), mix(base, w, 0.5), mix(base, w, 0.25), base, mix(base, b, 0.2), mix(base, b, 0.4), mix(base, b, 0.6)]
}

pub fn read_source() -> Source {
    let hemidock = std::env::var("APPDATA")
        .ok()
        .and_then(|d| std::fs::read_to_string(format!(r"{d}\HeMiDock\settings.json")).ok())
        .unwrap_or_default();
    let theme_mode = setting(&hemidock, "ThemeMode").and_then(|v| v.parse::<i32>().ok()).unwrap_or(0);
    let light = match theme_mode {
        1 => true,
        2 => false,
        _ => reg_dword(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize", "AppsUseLightTheme").unwrap_or(1) != 0,
    };

    let own = (setting(&hemidock, "AccentMode") == Some("1"))
        .then(|| setting(&hemidock, "AccentColor"))
        .flatten()
        .and_then(|c| u32::from_str_radix(c.trim_start_matches('#'), 16).ok())
        .map(|c| rgb((c >> 16) as u8, (c >> 8) as u8, c as u8));

    let accent = match own {
        Some(c) => shades(c),
        None => match reg_binary(r"Software\Microsoft\Windows\CurrentVersion\Explorer\Accent", "AccentPalette") {
            Some(p) if p.len() >= 28 => std::array::from_fn(|i| rgb(p[i * 4], p[i * 4 + 1], p[i * 4 + 2])),
            _ => shades(rgb(0x00, 0x78, 0xD4)),
        },
    };
    Source { light, accent }
}

// ---------------------------------------------------------------- egui

/// Segoe UI Variable (with its weight axis for the semibold texts) and Segoe Fluent Icons from Windows.
pub fn install_fonts(ctx: &egui::Context) {
    let mut defs = FontDefinitions::default();
    let fonts = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into()) + r"\Fonts\";
    let read = |name: &str| std::fs::read(format!("{fonts}{name}")).ok();
    let fallback: Vec<String> = defs.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();

    let mut body = Vec::new();
    let mut semi = Vec::new();
    let mut disp = Vec::new();
    if let Some(var) = read("SegUIVar.ttf") {
        let blob: Arc<dyn AsRef<[u8]> + Send + Sync> = Arc::new(var);
        let mut add = |name: &str, wght: f32, opsz: f32| {
            let mut data = FontData::from_blob(blob.clone(), 0);
            data.tweak.coords = VariationCoords::new([(b"wght", wght), (b"opsz", opsz)]);
            defs.font_data.insert(name.into(), Arc::new(data));
            name.to_string()
        };
        body.push(add("segoe", 400.0, 10.5));
        semi.push(add("segoe-semibold", 600.0, 10.5));
        disp.push(add("segoe-display", 600.0, 28.0));
    } else if let Some(ui) = read("segoeui.ttf") {
        defs.font_data.insert("segoe".into(), Arc::new(FontData::from_owned(ui)));
        body.push("segoe".into());
        let sb = read("seguisb.ttf").map(|b| {
            defs.font_data.insert("segoe-semibold".into(), Arc::new(FontData::from_owned(b)));
            "segoe-semibold".to_string()
        });
        semi.extend(sb.clone().or(Some("segoe".into())));
        disp.extend(sb.or(Some("segoe".into())));
    }
    let icons = read("SegoeIcons.ttf").or_else(|| read("segmdl2.ttf"));
    let mut icon_family = Vec::new();
    if let Some(i) = icons {
        defs.font_data.insert("fluent-icons".into(), Arc::new(FontData::from_owned(i)));
        icon_family.push("fluent-icons".to_string());
    }

    let with_fallback = |mut v: Vec<String>| {
        v.extend(fallback.iter().cloned());
        v
    };
    defs.families.insert(FontFamily::Proportional, with_fallback(body));
    defs.families.insert(FontFamily::Name(SEMIBOLD.into()), with_fallback(semi));
    defs.families.insert(FontFamily::Name(DISPLAY.into()), with_fallback(disp));
    defs.families.insert(FontFamily::Name(ICONS.into()), with_fallback(icon_family));
    ctx.set_fonts(defs);
}

pub fn apply(ctx: &egui::Context, p: &Palette) {
    ctx.set_theme(if p.light { egui::Theme::Light } else { egui::Theme::Dark });
    ctx.all_styles_mut(|s| {
        s.text_styles = [
            (TextStyle::Small, body(12.0)),
            (TextStyle::Body, body(14.0)),
            (TextStyle::Button, body(14.0)),
            (TextStyle::Heading, display(28.0)),
            (TextStyle::Monospace, FontId::monospace(13.0)),
        ]
        .into();
        s.spacing.item_spacing = egui::vec2(8.0, 6.0);
        s.spacing.button_padding = egui::vec2(12.0, 6.0);
        s.spacing.interact_size.y = 32.0;
        s.interaction.selectable_labels = false;
        s.interaction.tooltip_delay = 0.4;

        let v = &mut s.visuals;
        v.dark_mode = !p.light;
        v.panel_fill = p.bg;
        v.window_fill = p.dialog;
        v.window_stroke = Stroke::new(1.0, p.card_stroke);
        v.window_corner_radius = 8.into();
        v.menu_corner_radius = 8.into();
        v.extreme_bg_color = p.card;
        v.faint_bg_color = p.card;
        v.hyperlink_color = p.accent_text;
        v.override_text_color = Some(p.text);
        v.selection.bg_fill = p.accent.gamma_multiply(0.45);
        v.selection.stroke = Stroke::new(1.0, p.accent_text);
        let shadow = egui::Shadow { offset: [0, 8], blur: 28, spread: 0, color: Color32::from_black_alpha(if p.light { 38 } else { 110 }) };
        v.window_shadow = shadow;
        v.popup_shadow = egui::Shadow { offset: [0, 4], blur: 14, spread: 0, color: shadow.color };

        let w = &mut v.widgets;
        for (state, fill) in [
            (&mut w.noninteractive, p.card),
            (&mut w.inactive, p.control),
            (&mut w.hovered, p.control_hover),
            (&mut w.active, p.control_pressed),
            (&mut w.open, p.control_pressed),
        ] {
            state.bg_fill = fill;
            state.weak_bg_fill = fill;
            state.bg_stroke = Stroke::new(1.0, p.control_stroke);
            state.fg_stroke = Stroke::new(1.0, p.text);
            state.corner_radius = 4.into();
            state.expansion = 0.0;
        }
        w.noninteractive.bg_stroke = Stroke::new(1.0, p.card_stroke);
        w.hovered.fg_stroke = Stroke::new(1.0, p.text2);
    });
}

fn colorref(c: Color32) -> u32 {
    c.r() as u32 | (c.g() as u32) << 8 | (c.b() as u32) << 16
}

/// The title bar of Windows in the colours of the window (as WinUI with its own title bar).
pub fn style_window(hwnd: isize, p: &Palette) {
    if hwnd == 0 {
        return;
    }
    let hwnd = hwnd as HWND;
    let dark: i32 = (!p.light) as i32;
    let caption = colorref(p.bg);
    let text = colorref(p.text);
    unsafe {
        DwmSetWindowAttribute(hwnd, DWMWA_USE_IMMERSIVE_DARK_MODE as _, &dark as *const i32 as _, 4);
        DwmSetWindowAttribute(hwnd, DWMWA_CAPTION_COLOR as _, &caption as *const u32 as _, 4);
        DwmSetWindowAttribute(hwnd, DWMWA_TEXT_COLOR as _, &text as *const u32 as _, 4);
    }
}
