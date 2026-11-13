// HeMiUnlocker – shows which processes hold a locked file and offers to close them.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod i18n;
mod locks;
mod procs;
mod shell_menu;
mod theme;

use eframe::egui;
use std::path::PathBuf;

fn main() -> eframe::Result {
    // files / folders given on the command line (also the way a context-menu item will start it)
    let paths: Vec<PathBuf> = std::env::args_os()
        .skip(1)
        .map(PathBuf::from)
        .filter(|p| p.exists())
        .map(|p| std::path::absolute(&p).unwrap_or(p))
        .collect();

    shell_menu::repair();

    let icon = egui::IconData {
        rgba: include_bytes!("../assets/HeMiUnlocker-64.rgba").to_vec(),
        width: 64,
        height: 64,
    };
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("HeMiUnlocker")
            .with_app_id("HeMiUnlocker")
            .with_inner_size([780.0, 600.0])
            .with_min_inner_size([560.0, 440.0])
            .with_icon(icon)
            .with_drag_and_drop(true),
        centered: true,
        ..Default::default()
    };
    eframe::run_native(
        "HeMiUnlocker",
        options,
        Box::new(move |cc| Ok(Box::new(app::HeMiUnlockerApp::new(cc, paths)))),
    )
}
