//! The window of HeMiUnlocker: the chosen file, the programs that hold it and what can be done with them.
//! Laid out like the settings of HeMiDock: a large title, small semibold section headers, cards with a frame.

use crate::i18n::{self, tr};
use crate::locks::{self, AppKind, Locker, Scan};
use crate::procs::{self, Elevation};
use crate::theme::{self, Palette, glyph};
use eframe::egui::{
    self, Align, Align2, Color32, CornerRadius, Id, LayerId, Layout, Margin, Order, Rect, RichText, Sense, Stroke,
    StrokeKind, TextureHandle, TextureId, pos2, vec2,
};
use raw_window_handle::{
    DisplayHandle, HandleError, HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle, WindowHandle,
    WindowsDisplayHandle,
};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::{Duration, Instant};

/// Problems and ideas: the Issues of the project on GitHub (no e-mail address in the public code).
const CONTACT: &str = "github.com/Hemal1k/HeMiUnlocker/issues";
const CONTROL_RADIUS: u8 = 8;
const CARD_RADIUS: u8 = 10;

#[derive(Clone)]
struct Row {
    l: Locker,
    exe: Option<String>,
    elevation: Elevation,
    protected: bool,
}

impl Row {
    fn of(l: &Locker) -> Row {
        let exe = procs::exe_path(l.pid);
        Row {
            protected: l.kind == AppKind::Critical || procs::is_protected(l.pid, exe.as_deref()),
            elevation: procs::elevation(l.pid),
            exe,
            l: l.clone(),
        }
    }

    fn exe_name(&self) -> Option<&str> {
        self.exe.as_deref().map(|e| e.rsplit('\\').next().unwrap_or(e))
    }

    fn title(&self) -> String {
        if !self.l.name.trim().is_empty() {
            self.l.name.clone()
        } else {
            self.exe_name().unwrap_or(tr("Unknown process", "Neznámý proces")).to_string()
        }
    }
}

enum Outcome {
    Closed,
    Killed,
    ExplorerRestarted,
    NoWindow,
    Timeout,
    Failed(String),
}

enum Event {
    Scanned(u64, Scan, Vec<Row>),
    Done { pid: u32, name: String, outcome: Outcome },
}

#[derive(Clone, Copy, PartialEq)]
enum Busy {
    Closing,
    Killing,
}

#[derive(Clone, Copy, PartialEq)]
enum Tone {
    Success,
    Caution,
    Critical,
}

struct Toast {
    text: String,
    tone: Tone,
    until: Instant,
}

enum Confirm {
    Force(Row),
    Explorer(Row),
    CloseAll(Vec<Row>),
}

enum Action {
    PickFiles,
    PickFolder,
    Rescan,
    Close(Row),
    AskForce(Row),
    AskExplorer(Row),
    AskCloseAll(Vec<Row>),
    ShowInExplorer(String),
    RelaunchAsAdmin,
    DismissToast,
    ToggleMenu,
    About(bool),
    Open(String),
    OpenLicences,
}

pub struct HeMiUnlockerApp {
    targets: Vec<PathBuf>,
    result: Option<(Scan, Vec<Row>)>,
    scanning: bool,
    scan_id: u64,
    tx: Sender<Event>,
    rx: Receiver<Event>,
    p: Palette,
    source: theme::Source,
    theme_checked: Instant,
    icons: HashMap<String, Option<TextureHandle>>,
    busy: HashMap<u32, Busy>,
    notes: HashMap<u32, String>,
    confirm: Option<Confirm>,
    toast: Option<Toast>,
    admin: bool,
    hwnd: isize,
    focused: bool,
    menu_on: bool,
    about: bool,
}

impl HeMiUnlockerApp {
    pub fn new(cc: &eframe::CreationContext<'_>, targets: Vec<PathBuf>) -> Self {
        let source = theme::read_source();
        let p = theme::palette(&source);
        theme::install_fonts(&cc.egui_ctx);
        theme::apply(&cc.egui_ctx, &p);
        let hwnd = match cc.window_handle().map(|h| h.as_raw()) {
            Ok(RawWindowHandle::Win32(w)) => w.hwnd.get(),
            _ => 0,
        };
        theme::style_window(hwnd, &p);
        let (tx, rx) = channel();
        let mut app = HeMiUnlockerApp {
            targets: Vec::new(),
            result: None,
            scanning: false,
            scan_id: 0,
            tx,
            rx,
            p,
            source,
            theme_checked: Instant::now(),
            icons: HashMap::new(),
            busy: HashMap::new(),
            notes: HashMap::new(),
            confirm: None,
            toast: None,
            admin: procs::self_elevated(),
            hwnd,
            focused: true,
            menu_on: crate::shell_menu::is_on(),
            about: false,
        };
        if !targets.is_empty() {
            app.set_targets(&cc.egui_ctx, targets);
        }
        app
    }

    fn set_targets(&mut self, ctx: &egui::Context, targets: Vec<PathBuf>) {
        self.targets = targets;
        self.result = None;
        self.busy.clear();
        self.notes.clear();
        self.rescan(ctx);
    }

    fn rescan(&mut self, ctx: &egui::Context) {
        if self.targets.is_empty() {
            return;
        }
        self.scan_id += 1;
        self.scanning = true;
        let (id, targets, tx, ctx) = (self.scan_id, self.targets.clone(), self.tx.clone(), ctx.clone());
        std::thread::spawn(move || {
            let scan = locks::scan(&targets);
            let rows = scan.lockers.iter().map(Row::of).collect();
            let _ = tx.send(Event::Scanned(id, scan, rows));
            ctx.request_repaint();
        });
    }

    fn work(&mut self, ctx: &egui::Context, row: &Row, busy: Busy, job: impl FnOnce(u32, u64) -> Outcome + Send + 'static) {
        let (pid, start, name) = (row.l.pid, row.l.start_time, row.title());
        self.busy.insert(pid, busy);
        self.notes.remove(&pid);
        let (tx, ctx) = (self.tx.clone(), ctx.clone());
        std::thread::spawn(move || {
            let outcome = job(pid, start);
            let _ = tx.send(Event::Done { pid, name, outcome });
            ctx.request_repaint();
        });
    }

    fn close(&mut self, ctx: &egui::Context, row: &Row) {
        self.work(ctx, row, Busy::Closing, |pid, _| {
            if procs::post_close(pid) == 0 {
                Outcome::NoWindow
            } else if procs::wait_exit(pid, 8000) {
                Outcome::Closed
            } else {
                Outcome::Timeout
            }
        });
    }

    fn kill(&mut self, ctx: &egui::Context, row: &Row) {
        self.work(ctx, row, Busy::Killing, |pid, start| match procs::terminate(pid, start) {
            Ok(()) => Outcome::Killed,
            Err(e) => Outcome::Failed(e),
        });
    }

    fn restart_explorer(&mut self, ctx: &egui::Context, row: &Row) {
        self.work(ctx, row, Busy::Killing, |pid, start| match procs::restart_explorer(pid, start) {
            Ok(()) => Outcome::ExplorerRestarted,
            Err(e) => Outcome::Failed(e),
        });
    }

    fn toast(&mut self, text: impl Into<String>, tone: Tone) {
        self.toast = Some(Toast { text: text.into(), tone, until: Instant::now() + Duration::from_secs(6) });
    }

    fn poll(&mut self, ctx: &egui::Context) {
        while let Ok(ev) = self.rx.try_recv() {
            match ev {
                Event::Scanned(id, scan, rows) => {
                    if id == self.scan_id {
                        self.scanning = false;
                        self.notes.retain(|pid, _| rows.iter().any(|r| r.l.pid == *pid));
                        self.result = Some((scan, rows));
                    }
                }
                Event::Done { pid, name, outcome } => {
                    self.busy.remove(&pid);
                    match outcome {
                        Outcome::Closed => {
                            self.toast(tr("{name} closed.", "{name} se zavřel.").replace("{name}", &name), Tone::Success);
                            self.rescan(ctx);
                        }
                        Outcome::Killed => {
                            self.toast(tr("{name} was ended.", "{name} byl ukončen.").replace("{name}", &name), Tone::Success);
                            self.rescan(ctx);
                        }
                        Outcome::ExplorerRestarted => {
                            self.toast(tr("File Explorer was restarted.", "Průzkumník byl restartován."), Tone::Success);
                            self.rescan(ctx);
                        }
                        Outcome::NoWindow => {
                            let text = tr(
                                "The program has no window that could be closed. Use Force quit.",
                                "Program nemá okno, které by šlo zavřít. Použij Vynutit ukončení.",
                            );
                            self.notes.insert(pid, text.into());
                        }
                        Outcome::Timeout => {
                            let text = tr(
                                "The program has not closed yet – it may be asking to save. Look at its window, or use Force quit.",
                                "Program se zatím nezavřel – možná se ptá na uložení. Podívej se do jeho okna, nebo použij Vynutit ukončení.",
                            );
                            self.notes.insert(pid, text.into());
                        }
                        Outcome::Failed(e) => self.toast(e, Tone::Critical),
                    }
                }
            }
        }
        if self.toast.as_ref().is_some_and(|t| Instant::now() > t.until) {
            self.toast = None;
        }
    }

    /// Light / dark or the accent colour changed (in Windows or in HeMiDock): the new colours at once.
    fn follow_theme(&mut self, ctx: &egui::Context) {
        if self.theme_checked.elapsed() < Duration::from_secs(2) {
            return;
        }
        self.theme_checked = Instant::now();
        let source = theme::read_source();
        if source != self.source {
            self.source = source;
            self.p = theme::palette(&source);
            theme::apply(ctx, &self.p);
            theme::style_window(self.hwnd, &self.p);
        }
    }

    fn icon(&mut self, ctx: &egui::Context, exe: Option<&str>) -> Option<TextureId> {
        let exe = exe?;
        let px = (32.0 * ctx.pixels_per_point()).round() as i32;
        let key = format!("{exe}|{px}");
        self.icons
            .entry(key.clone())
            .or_insert_with(|| {
                procs::exe_icon(exe, px).map(|(rgba, w, h)| {
                    ctx.load_texture(key, egui::ColorImage::from_rgba_unmultiplied([w, h], &rgba), egui::TextureOptions::LINEAR)
                })
            })
            .as_ref()
            .map(|t| t.id())
    }

    fn pick(&self, folder: bool) -> Option<Vec<PathBuf>> {
        let d = rfd::FileDialog::new().set_parent(&Parent(self.hwnd));
        if folder {
            d.set_title(tr("Choose a folder", "Vyber složku")).pick_folder().map(|f| vec![f])
        } else {
            d.set_title(tr("Choose a file", "Vyber soubor")).pick_files()
        }
    }

    // ------------------------------------------------------------------ the window

    fn header(&self, ui: &mut egui::Ui, acts: &mut Vec<Action>) {
        let p = self.p;
        ui.horizontal(|ui| {
            ui.set_min_height(40.0);
            if self.about {
                if Btn::new("").icon(glyph::BACK).subtle().tip(tr("Back", "Zpět")).show(ui, &p).clicked() {
                    acts.push(Action::About(false));
                }
                ui.add_space(4.0);
            }
            let title = if self.about { tr("About", "O aplikaci") } else { "HeMiUnlocker" };
            ui.label(RichText::new(title).font(theme::display(28.0)).color(p.text));
            if !self.about {
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if Btn::new("").icon(glyph::INFO).subtle().tip(tr("About", "O aplikaci")).show(ui, &p).clicked() {
                        acts.push(Action::About(true));
                    }
                });
            }
        });
        ui.add_space(18.0);
    }

    /// The page "O aplikaci", as in HeMiDock: author, version, build date, contact, licences.
    fn about_page(&self, ui: &mut egui::Ui, acts: &mut Vec<Action>) {
        let p = self.p;
        section(ui, &p, "HeMiUnlocker");
        ui.spacing_mut().item_spacing.y = 4.0;
        let value = |ui: &mut egui::Ui, label: &str, value: &str| {
            card(ui, &p, |ui| {
                row_layout(ui, 44.0, |ui| {
                    ui.label(RichText::new(label).size(14.0).color(p.text));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(RichText::new(value).size(14.0).color(p.text2));
                    });
                });
            });
        };
        value(ui, tr("Author", "Autor"), "Richard Hemala");
        value(ui, tr("Version", "Verze"), &format!("Beta {}", env!("CARGO_PKG_VERSION")));
        value(ui, tr("Build date", "Datum sestavení"), &i18n::build_date());
        card(ui, &p, |ui| {
            row_layout(ui, 44.0, |ui| {
                ui.label(RichText::new(tr("Contact", "Kontakt")).size(14.0).color(p.text));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if Btn::new(tr("Report a problem", "Nahlásit problém")).tip(CONTACT).show(ui, &p).clicked() {
                        acts.push(Action::Open(format!("https://{CONTACT}")));
                    }
                });
            });
        });
        card(ui, &p, |ui| {
            row_layout(ui, 44.0, |ui| {
                two_lines(ui, &p, tr("Licences", "Licence"), tr("Licences of the components used", "Licence použitých součástí"));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if Btn::new(tr("Open", "Otevřít")).show(ui, &p).clicked() {
                        acts.push(Action::OpenLicences);
                    }
                });
            });
        });
    }

    fn target_card(&self, ui: &mut egui::Ui, acts: &mut Vec<Action>) {
        let p = self.p;
        section(ui, &p, if self.targets.iter().any(|t| t.is_dir()) { tr("Folder", "Složka") } else { tr("File", "Soubor") });
        card(ui, &p, |ui| {
            row_layout(ui, 44.0, |ui| {
                let (g, color) = match self.targets.as_slice() {
                    [] => (glyph::OPEN_FILE, p.accent_text),
                    [one] if one.is_dir() => (glyph::FOLDER, p.accent_text),
                    _ => (glyph::DOCUMENT, p.accent_text),
                };
                glyph_box(ui, g, 22.0, color);
                ui.add_space(6.0);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let folder = tr("Folder…", "Složka…");
                    if self.targets.is_empty() {
                        if Btn::new(folder).icon(glyph::FOLDER).show(ui, &p).clicked() {
                            acts.push(Action::PickFolder);
                        }
                        if Btn::new(tr("Choose file", "Vybrat soubor")).icon(glyph::OPEN_FILE).accent().show(ui, &p).clicked() {
                            acts.push(Action::PickFiles);
                        }
                    } else {
                        if Btn::new(folder).icon(glyph::FOLDER).show(ui, &p).clicked() {
                            acts.push(Action::PickFolder);
                        }
                        if Btn::new(tr("Other file…", "Jiný soubor…")).icon(glyph::OPEN_FILE).show(ui, &p).clicked() {
                            acts.push(Action::PickFiles);
                        }
                        if let [one] = self.targets.as_slice() {
                            let tip = tr("Show in File Explorer", "Ukázat v Průzkumníkovi");
                            if Btn::new("").icon(glyph::OPEN_FOLDER).subtle().tip(tip).show(ui, &p).clicked() {
                                acts.push(Action::ShowInExplorer(one.display().to_string()));
                            }
                        }
                    }
                    ui.add_space(8.0);
                    ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                        ui.vertical(|ui| {
                            ui.spacing_mut().item_spacing.y = 1.0;
                            let (title, detail) = match self.targets.as_slice() {
                                [] => (
                                    tr("Choose a file or folder", "Vyber soubor nebo složku").to_string(),
                                    tr("Or drag it here from File Explorer.", "Nebo ho sem přetáhni z Průzkumníka.").to_string(),
                                ),
                                [one] => (
                                    one.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| one.display().to_string()),
                                    one.parent().map(|d| d.display().to_string()).unwrap_or_default(),
                                ),
                                many => (tr("{n} items", "{n} položek").replace("{n}", &many.len().to_string()), many.iter().filter_map(|t| t.file_name()).map(|n| n.to_string_lossy()).collect::<Vec<_>>().join(", ")),
                            };
                            ui.add(egui::Label::new(RichText::new(title).size(14.0).color(p.text)).truncate());
                            ui.add(egui::Label::new(RichText::new(detail).size(12.0).color(p.text2)).truncate());
                        });
                    });
                });
            });
        });
    }

    fn results(&mut self, ui: &mut egui::Ui, acts: &mut Vec<Action>) {
        let p = self.p;
        let rows: Vec<Row> = self.result.as_ref().map(|r| r.1.clone()).unwrap_or_default();
        let closable: Vec<Row> = rows.iter().filter(|r| !r.protected && r.l.kind != AppKind::Explorer).cloned().collect();

        ui.add_space(22.0);
        ui.horizontal(|ui| {
            ui.set_min_height(32.0);
            ui.label(RichText::new(tr("Status:", "Stav:")).font(theme::semibold(14.0)).color(p.text));
            if self.scanning && self.result.is_some() {
                ui.add(egui::Spinner::new().size(14.0).color(p.accent_text));
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if closable.len() > 1 && Btn::new(tr("Close all", "Ukončit vše")).icon(glyph::CLOSE).show(ui, &p).clicked() {
                    acts.push(Action::AskCloseAll(closable.clone()));
                }
                if Btn::new(tr("Refresh", "Obnovit")).icon(glyph::REFRESH).subtle().enabled(!self.scanning).show(ui, &p).clicked() {
                    acts.push(Action::Rescan);
                }
            });
        });
        ui.add_space(4.0);

        let Some((scan, _)) = &self.result else {
            card(ui, &p, |ui| {
                row_layout(ui, 44.0, |ui| {
                    ui.add_space(6.0);
                    ui.add(egui::Spinner::new().size(20.0).color(p.accent_text));
                    ui.add_space(10.0);
                    ui.label(RichText::new(tr("Looking for programs…", "Hledám programy…")).color(p.text2));
                });
            });
            return;
        };
        let (truncated, locked_unknown, error) = (scan.truncated, scan.locked_unknown, scan.error.clone());

        if let Some(e) = error {
            info_bar(ui, &p, Tone::Critical, &e, None);
            return;
        }
        if rows.is_empty() {
            card(ui, &p, |ui| {
                row_layout(ui, 52.0, |ui| {
                    if locked_unknown {
                        glyph_box(ui, glyph::WARNING, 22.0, p.caution);
                        ui.add_space(6.0);
                        let hint = if self.admin {
                            tr(
                                "Probably the system, a service or a driver that Windows does not reveal.",
                                "Drží ho nejspíš systém, služba nebo ovladač, které Windows neprozradí.",
                            )
                        } else {
                            tr(
                                "A program running as administrator may hold it. Try running HeMiUnlocker as administrator.",
                                "Může ho držet program spuštěný jako správce. Zkus HeMiUnlocker spustit jako správce.",
                            )
                        };
                        let head = tr("The file is open, but it is not clear what holds it", "Soubor je otevřený, ale nepodařilo se zjistit čím");
                        two_lines(ui, &p, head, hint);
                    } else {
                        glyph_box(ui, glyph::COMPLETED, 22.0, p.success);
                        ui.add_space(6.0);
                        let text = if self.targets.iter().any(|t| t.is_dir()) {
                            tr("No process has the folder open", "Složka nemá otevřený žádný proces")
                        } else {
                            tr("No process has the file open", "Soubor nemá otevřený žádný proces")
                        };
                        ui.label(RichText::new(text).size(14.0).color(p.text));
                    }
                });
            });
        } else {
            egui::ScrollArea::vertical().auto_shrink([false, true]).show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 4.0;
                for row in &rows {
                    self.row(ui, row, acts);
                }
            });
        }
        if truncated {
            ui.add_space(6.0);
            ui.label(
                RichText::new(
                    tr(
                        "The folder has more than {n} files – only the first {n} were checked.",
                        "Složka má víc než {n} souborů – zkontrolováno jen prvních {n}.",
                    )
                    .replace("{n}", &locks::MAX_FILES.to_string()),
                )
                    .size(12.0)
                    .color(p.text2),
            );
        }
    }

    fn row(&mut self, ui: &mut egui::Ui, row: &Row, acts: &mut Vec<Action>) {
        let p = self.p;
        let icon = self.icon(ui.ctx(), row.exe.as_deref());
        let busy = self.busy.get(&row.l.pid).copied();
        let note = self.notes.get(&row.l.pid).cloned();
        card(ui, &p, |ui| {
            row_layout(ui, 44.0, |ui| {
                let (r, _) = ui.allocate_exact_size(vec2(32.0, 32.0), Sense::hover());
                match icon {
                    Some(t) => {
                        ui.painter().image(t, r, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
                    }
                    None => {
                        let g = match row.l.kind {
                            AppKind::Service => glyph::GEAR,
                            AppKind::Console => glyph::CONSOLE,
                            AppKind::Explorer => glyph::FOLDER,
                            AppKind::Critical => glyph::SHIELD,
                            _ => glyph::APP,
                        };
                        ui.painter().text(r.center(), Align2::CENTER_CENTER, g, theme::icon(24.0), p.text2);
                    }
                }
                ui.add_space(8.0);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if let Some(b) = busy {
                        let text = if b == Busy::Closing { tr("Closing…", "Zavírám…") } else { tr("Ending…", "Ukončuji…") };
                        ui.label(RichText::new(text).color(p.text2));
                        ui.add(egui::Spinner::new().size(16.0).color(p.accent_text));
                    } else if row.protected {
                        ui.label(RichText::new(tr("Protected system process", "Chráněný systémový proces")).size(12.0).color(p.text3));
                        ui.label(RichText::new(glyph::SHIELD.to_string()).font(theme::icon(14.0)).color(p.text3));
                    } else if row.l.kind == AppKind::Explorer {
                        let tip = tr("Ends File Explorer and starts it again right away.", "Ukončí Průzkumník a hned ho spustí znovu.");
                        if Btn::new(tr("Restart", "Restartovat")).icon(glyph::RESTART).tip(tip).show(ui, &p).clicked() {
                            acts.push(Action::AskExplorer(row.clone()));
                        }
                    } else {
                        if Btn::new(tr("Close", "Ukončit"))
                            .icon(glyph::CLOSE)
                            .accent()
                            .tip(tr(
                                "Asks the program to close – it may still ask to save.",
                                "Požádá program o zavření – může se ještě zeptat na uložení.",
                            ))
                            .show(ui, &p)
                            .clicked()
                        {
                            acts.push(Action::Close(row.clone()));
                        }
                        if Btn::new(tr("Force quit", "Vynutit ukončení"))
                            .icon(glyph::LIGHTNING)
                            .tint(p.critical)
                            .tip(tr("Ends the program at once. Unsaved work is lost.", "Ukončí program okamžitě. Neuložená práce se ztratí."))
                            .show(ui, &p)
                            .clicked()
                        {
                            acts.push(Action::AskForce(row.clone()));
                        }
                    }
                    if let Some(exe) = &row.exe {
                        let tip = tr("Show the program in File Explorer", "Ukázat program v Průzkumníkovi");
                        if Btn::new("").icon(glyph::OPEN_FOLDER).subtle().tip(tip).show(ui, &p).clicked() {
                            acts.push(Action::ShowInExplorer(exe.clone()));
                        }
                    }
                    ui.add_space(8.0);
                    ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                        ui.vertical(|ui| {
                            ui.spacing_mut().item_spacing.y = 1.0;
                            ui.add(egui::Label::new(RichText::new(row.title()).size(14.0).color(p.text)).truncate());
                            let mut detail = vec![row.l.kind.label().to_string()];
                            if let Some(n) = row.exe_name() {
                                detail.push(n.to_string());
                            }
                            detail.push(format!("PID {}", row.l.pid));
                            if !row.l.service.is_empty() {
                                detail.push(format!("{} {}", tr("service", "služba"), row.l.service));
                            }
                            match row.elevation {
                                Elevation::Elevated => detail.push(tr("as administrator", "jako správce").into()),
                                Elevation::Unknown if !self.admin => detail.push(tr("higher rights", "vyšší práva").into()),
                                _ => {}
                            }
                            ui.add(egui::Label::new(RichText::new(detail.join("  ·  ")).size(12.0).color(p.text2)).truncate());
                        });
                    });
                });
            });
            if let Some(n) = note {
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.add_space(40.0);
                    ui.label(RichText::new(glyph::WARNING.to_string()).font(theme::icon(12.0)).color(p.caution));
                    ui.add(egui::Label::new(RichText::new(n).size(12.0).color(p.text2)).wrap());
                });
            }
        });
    }

    fn footer(&self, ui: &mut egui::Ui, acts: &mut Vec<Action>) {
        let p = self.p;
        if let Some(t) = &self.toast {
            if info_bar(ui, &p, t.tone, &t.text, Some(())) {
                acts.push(Action::DismissToast);
            }
            ui.add_space(8.0);
        }
        ui.horizontal(|ui| {
            ui.set_min_height(32.0);
            let tip = tr(
                "Adds “HeMiUnlock” to the right-click menu of files and folders. \
                 In Windows 11 it is under “Show more options”.",
                "Přidá položku „HeMiUnlock“ do nabídky po kliknutí pravým tlačítkem na soubor nebo složku. \
                 Ve Windows 11 je pod „Zobrazit další možnosti“.",
            );
            if toggle(ui, &p, self.menu_on).on_hover_text(tip).clicked() {
                acts.push(Action::ToggleMenu);
            }
            ui.add_space(4.0);
            ui.label(RichText::new(tr("File Explorer context menu", "Kontextová nabídka Průzkumníka")).size(13.0).color(p.text)).on_hover_text(tip);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if !self.admin
                    && Btn::new(tr("Run as administrator", "Spustit jako správce"))
                        .icon(glyph::SHIELD)
                        .tip(tr(
                            "Without administrator rights, programs running as administrator cannot be ended.",
                            "Bez práv správce nepůjde ukončit programy spuštěné jako správce.",
                        ))
                        .show(ui, &p)
                        .clicked()
                {
                    acts.push(Action::RelaunchAsAdmin);
                }
            });
        });
    }

    fn drop_overlay(&self, ctx: &egui::Context) {
        if !ctx.input(|i| !i.raw.hovered_files.is_empty()) {
            return;
        }
        let p = self.p;
        let painter = ctx.layer_painter(LayerId::new(Order::Foreground, Id::unique("drop")));
        let r = ctx.content_rect().shrink(14.0);
        painter.rect_filled(r, CARD_RADIUS, p.bg.gamma_multiply(0.94));
        painter.rect_stroke(r, CARD_RADIUS, Stroke::new(2.0, p.accent), StrokeKind::Inside);
        painter.text(r.center() - vec2(0.0, 22.0), Align2::CENTER_CENTER, glyph::OPEN_FILE, theme::icon(36.0), p.accent_text);
        painter.text(r.center() + vec2(0.0, 22.0), Align2::CENTER_CENTER, tr("Drop it here", "Pusť ho sem"), theme::semibold(20.0), p.text);
    }

    fn dialog(&mut self, ctx: &egui::Context) {
        let Some(c) = &self.confirm else { return };
        let p = self.p;
        let (title, text, primary) = match c {
            Confirm::Force(r) => (
                tr("Force the program to end?", "Vynutit ukončení?"),
                tr(
                    "{name} will end at once, with no chance to save. Unsaved work is lost.",
                    "{name} se ukončí okamžitě, bez možnosti uložit. Neuložená práce se ztratí.",
                )
                .replace("{name}", &r.title()),
                tr("Force quit", "Vynutit ukončení"),
            ),
            Confirm::Explorer(_) => (
                tr("Restart File Explorer?", "Restartovat Průzkumník?"),
                tr(
                    "Folder windows close and the taskbar disappears for a moment. File Explorer starts again right away.",
                    "Okna složek se zavřou a hlavní panel na chvíli zmizí. Průzkumník se hned spustí znovu.",
                )
                .to_string(),
                tr("Restart", "Restartovat"),
            ),
            Confirm::CloseAll(rows) => (
                tr("Close all programs?", "Ukončit všechny programy?"),
                tr(
                    "HeMiUnlocker will ask {n} programs to close. Each may still ask to save.",
                    "HeMiUnlocker požádá {n} programy o zavření. Každý se ještě může zeptat na uložení.",
                )
                .replace("{n}", &rows.len().to_string()),
                tr("Close all", "Ukončit vše"),
            ),
        };
        let mut choice: Option<bool> = None;
        let frame = egui::Frame::new()
            .fill(p.dialog)
            .stroke(Stroke::new(1.0, p.card_stroke))
            .corner_radius(8)
            .shadow(ctx.global_style().visuals.window_shadow);
        let modal = egui::Modal::new(Id::unique("confirm")).backdrop_color(p.smoke).frame(frame).show(ctx, |ui| {
            let width = 440.0;
            ui.set_width(width);
            ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
            egui::Frame::new().inner_margin(Margin { left: 24, right: 24, top: 22, bottom: 24 }).show(ui, |ui| {
                ui.set_width(width - 48.0);
                ui.label(RichText::new(title).font(theme::semibold(20.0)).color(p.text));
                ui.add_space(12.0);
                ui.add(egui::Label::new(RichText::new(text).size(14.0).color(p.text)).wrap());
            });
            egui::Frame::new()
                .fill(p.dialog_foot)
                .corner_radius(CornerRadius { nw: 0, ne: 0, sw: 8, se: 8 })
                .inner_margin(Margin::same(24))
                .show(ui, |ui| {
                    ui.set_width(width - 48.0);
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 8.0;
                        let half = (width - 48.0 - 8.0) / 2.0;
                        if Btn::new(primary).accent().width(half).show(ui, &p).clicked() {
                            choice = Some(true);
                        }
                        if Btn::new(tr("Cancel", "Zrušit")).width(half).show(ui, &p).clicked() {
                            choice = Some(false);
                        }
                    });
                });
        });
        if modal.should_close() {
            choice = choice.or(Some(false));
        }
        match choice {
            Some(true) => match self.confirm.take() {
                Some(Confirm::Force(r)) => self.kill(ctx, &r),
                Some(Confirm::Explorer(r)) => self.restart_explorer(ctx, &r),
                Some(Confirm::CloseAll(rows)) => {
                    for r in rows {
                        self.close(ctx, &r);
                    }
                }
                None => {}
            },
            Some(false) => self.confirm = None,
            None => {}
        }
    }

    fn act(&mut self, ctx: &egui::Context, acts: Vec<Action>) {
        for a in acts {
            match a {
                Action::PickFiles | Action::PickFolder => {
                    if let Some(t) = self.pick(matches!(a, Action::PickFolder)) {
                        if !t.is_empty() {
                            self.set_targets(ctx, t);
                        }
                    }
                }
                Action::Rescan => self.rescan(ctx),
                Action::Close(r) => self.close(ctx, &r),
                Action::AskForce(r) => self.confirm = Some(Confirm::Force(r)),
                Action::AskExplorer(r) => self.confirm = Some(Confirm::Explorer(r)),
                Action::AskCloseAll(rows) => self.confirm = Some(Confirm::CloseAll(rows)),
                Action::ShowInExplorer(path) => procs::show_in_explorer(&path),
                Action::RelaunchAsAdmin => {
                    if procs::relaunch_as_admin(&self.targets) {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    } else {
                        self.toast(tr("Running as administrator was cancelled.", "Spuštění jako správce bylo zrušeno."), Tone::Caution);
                    }
                }
                Action::DismissToast => self.toast = None,
                Action::About(on) => self.about = on,
                Action::Open(target) => procs::shell_open(&target),
                Action::OpenLicences => {
                    let file = std::env::temp_dir().join("HeMiUnlocker-ThirdPartyNotices.txt");
                    match std::fs::write(&file, include_str!("../assets/ThirdPartyNotices.txt")) {
                        Ok(()) => procs::shell_open(&file.display().to_string()),
                        Err(e) => self.toast(format!("{}: {e}", tr("The licences cannot be opened", "Licence nejde otevřít")), Tone::Critical),
                    }
                }
                Action::ToggleMenu => match crate::shell_menu::set(!self.menu_on) {
                    Ok(()) => {
                        self.menu_on = !self.menu_on;
                        let text = if self.menu_on {
                            tr(
                                "The item is in the File Explorer menu (in Windows 11 under “Show more options”).",
                                "Položka je v nabídce Průzkumníka (ve Windows 11 pod „Zobrazit další možnosti“).",
                            )
                        } else {
                            tr("The item was removed from the File Explorer menu.", "Položka byla z nabídky Průzkumníka odebrána.")
                        };
                        self.toast(text, Tone::Success);
                    }
                    Err(e) => self.toast(e, Tone::Critical),
                },
            }
        }
    }
}

impl eframe::App for HeMiUnlockerApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.poll(&ctx);
        self.follow_theme(&ctx);
        ctx.request_repaint_after(Duration::from_secs(2));

        // files dropped from Explorer
        let dropped: Vec<PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).collect());
        if !dropped.is_empty() {
            self.about = false;
            self.set_targets(&ctx, dropped);
        }
        // back in the window: the user may have closed a program by hand meanwhile
        let focused = ctx.input(|i| i.viewport().focused).unwrap_or(true);
        if focused && !self.focused && !self.scanning && self.busy.is_empty() {
            self.rescan(&ctx);
        }
        self.focused = focused;

        let p = self.p;
        let mut acts = Vec::new();
        if !self.about {
            egui::Panel::bottom("footer")
                .frame(egui::Frame::new().fill(p.bg).inner_margin(Margin { left: 28, right: 28, top: 8, bottom: 16 }))
                .show_separator_line(false)
                .show(ui, |ui| self.footer(ui, &mut acts));
        }
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(p.bg).inner_margin(Margin { left: 28, right: 28, top: 14, bottom: 4 }))
            .show(ui, |ui| {
                self.header(ui, &mut acts);
                if self.about {
                    self.about_page(ui, &mut acts);
                    return;
                }
                self.target_card(ui, &mut acts);
                if self.targets.is_empty() {
                    empty_hint(ui, &p);
                } else {
                    self.results(ui, &mut acts);
                }
            });
        self.drop_overlay(&ctx);
        self.dialog(&ctx);
        self.act(&ctx, acts);
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        self.p.bg.to_normalized_gamma_f32()
    }
}

// ---------------------------------------------------------------------- parts

/// The window as the owner of the file dialog.
struct Parent(isize);

impl HasWindowHandle for Parent {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        let hwnd = std::num::NonZeroIsize::new(self.0).ok_or(HandleError::Unavailable)?;
        let raw = RawWindowHandle::Win32(raw_window_handle::Win32WindowHandle::new(hwnd));
        Ok(unsafe { WindowHandle::borrow_raw(raw) })
    }
}

impl HasDisplayHandle for Parent {
    fn display_handle(&self) -> Result<DisplayHandle<'_>, HandleError> {
        Ok(unsafe { DisplayHandle::borrow_raw(RawDisplayHandle::Windows(WindowsDisplayHandle::new())) })
    }
}

fn section(ui: &mut egui::Ui, p: &Palette, text: &str) {
    ui.label(RichText::new(text).font(theme::semibold(14.0)).color(p.text));
    ui.add_space(4.0);
}

/// A card of the settings of HeMiDock: the card colour, a thin frame, rounded corners.
fn card<R>(ui: &mut egui::Ui, p: &Palette, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    egui::Frame::new()
        .fill(p.card)
        .stroke(Stroke::new(1.0, p.card_stroke))
        .corner_radius(CARD_RADIUS)
        .inner_margin(Margin { left: 16, right: 14, top: 10, bottom: 10 })
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui)
        })
        .inner
}

/// A row of fixed height with the items centred vertically.
fn row_layout(ui: &mut egui::Ui, height: f32, add: impl FnOnce(&mut egui::Ui)) {
    let w = ui.available_width();
    ui.allocate_ui_with_layout(vec2(w, height), Layout::left_to_right(Align::Center), |ui| {
        ui.set_min_size(vec2(w, height));
        add(ui)
    });
}

fn glyph_box(ui: &mut egui::Ui, g: char, size: f32, color: Color32) {
    let (r, _) = ui.allocate_exact_size(vec2(32.0, 32.0), Sense::hover());
    ui.painter().text(r.center(), Align2::CENTER_CENTER, g, theme::icon(size), color);
}

fn two_lines(ui: &mut egui::Ui, p: &Palette, title: &str, detail: &str) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 1.0;
        ui.add(egui::Label::new(RichText::new(title).size(14.0).color(p.text)).truncate());
        ui.add(egui::Label::new(RichText::new(detail).size(12.0).color(p.text2)).wrap());
    });
}

fn empty_hint(ui: &mut egui::Ui, p: &Palette) {
    ui.add_space(ui.available_height() * 0.22);
    ui.vertical_centered(|ui| {
        ui.label(RichText::new(glyph::UNLOCK.to_string()).font(theme::icon(44.0)).color(p.text3));
        ui.add_space(10.0);
        let head = tr("Which program is holding my file?", "Který program drží můj soubor?");
        ui.label(RichText::new(head).font(theme::semibold(16.0)).color(p.text));
        ui.add_space(2.0);
        let text = tr(
            "Choose a file or folder and HeMiUnlocker shows what has it open.",
            "Vyber soubor nebo složku a HeMiUnlocker ukáže, co je má otevřené.",
        );
        ui.label(RichText::new(text).size(13.0).color(p.text2));
    });
}

/// An InfoBar of WinUI: a coloured strip with an icon. Returns true when its × was clicked.
fn info_bar(ui: &mut egui::Ui, p: &Palette, tone: Tone, text: &str, closable: Option<()>) -> bool {
    let (bg, fg, g) = match tone {
        Tone::Success => (p.success_bg, p.success, glyph::COMPLETED),
        Tone::Caution => (p.caution_bg, p.caution, glyph::WARNING),
        Tone::Critical => (p.critical_bg, p.critical, glyph::ERROR),
    };
    let mut closed = false;
    egui::Frame::new()
        .fill(bg)
        .stroke(Stroke::new(1.0, p.card_stroke))
        .corner_radius(CONTROL_RADIUS)
        .inner_margin(Margin { left: 14, right: 6, top: 6, bottom: 6 })
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            row_layout(ui, 32.0, |ui| {
                ui.label(RichText::new(g.to_string()).font(theme::icon(16.0)).color(fg));
                ui.add_space(4.0);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if closable.is_some() && Btn::new("").icon(glyph::CLOSE).subtle().show(ui, p).clicked() {
                        closed = true;
                    }
                    ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                        ui.add(egui::Label::new(RichText::new(text).size(13.0).color(p.text)).truncate());
                    });
                });
            });
        });
    closed
}

/// A ToggleSwitch of WinUI: a pill 40 × 20 with a knob, the accent colour when on.
fn toggle(ui: &mut egui::Ui, p: &Palette, on: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(40.0, 20.0), Sense::click());
    let t = ui.ctx().animate_bool_with_time(resp.id, on, 0.12);
    let hover = resp.hovered();
    let painter = ui.painter();
    if on {
        painter.rect_filled(rect, 10, if hover { p.accent_hover } else { p.accent });
    } else {
        painter.rect_filled(rect, 10, if hover { p.subtle_hover } else { Color32::TRANSPARENT });
        painter.rect_stroke(rect, 10, Stroke::new(1.0, p.text2), StrokeKind::Inside);
    }
    let r = if hover { 7.0 } else { 6.0 };
    let x = egui::lerp((rect.left() + 10.0)..=(rect.right() - 10.0), t);
    painter.circle_filled(pos2(x, rect.center().y), r, if on { p.on_accent } else { p.text2 });
    resp
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Standard,
    Accent,
    Subtle,
}

/// A button of WinUI 3: standard (light fill, a frame with a darker bottom edge), accent or subtle.
struct Btn<'a> {
    text: &'a str,
    icon: Option<char>,
    kind: Kind,
    enabled: bool,
    tint: Option<Color32>,
    tip: Option<&'a str>,
    width: Option<f32>,
}

impl<'a> Btn<'a> {
    fn new(text: &'a str) -> Self {
        Btn { text, icon: None, kind: Kind::Standard, enabled: true, tint: None, tip: None, width: None }
    }
    fn icon(mut self, g: char) -> Self {
        self.icon = Some(g);
        self
    }
    fn accent(mut self) -> Self {
        self.kind = Kind::Accent;
        self
    }
    fn subtle(mut self) -> Self {
        self.kind = Kind::Subtle;
        self
    }
    fn enabled(mut self, on: bool) -> Self {
        self.enabled = on;
        self
    }
    fn tint(mut self, c: Color32) -> Self {
        self.tint = Some(c);
        self
    }
    fn tip(mut self, t: &'a str) -> Self {
        self.tip = Some(t);
        self
    }
    fn width(mut self, w: f32) -> Self {
        self.width = Some(w);
        self
    }

    fn show(self, ui: &mut egui::Ui, p: &Palette) -> egui::Response {
        let text = (!self.text.is_empty()).then(|| ui.painter().layout_no_wrap(self.text.to_string(), theme::body(14.0), Color32::PLACEHOLDER));
        let icon = self.icon.map(|g| ui.painter().layout_no_wrap(g.to_string(), theme::icon(14.0), Color32::PLACEHOLDER));
        let gap = if text.is_some() && icon.is_some() { 8.0 } else { 0.0 };
        let content = text.as_ref().map_or(0.0, |g| g.size().x) + icon.as_ref().map_or(0.0, |g| g.size().x) + gap;
        let pad = if text.is_none() { 9.0 } else { 12.0 };
        let w = self.width.unwrap_or(content + pad * 2.0).max(32.0);
        let sense = if self.enabled { Sense::click() } else { Sense::hover() };
        let (rect, mut resp) = ui.allocate_exact_size(vec2(w, 32.0), sense);
        if ui.is_rect_visible(rect) {
            let (down, hover) = (resp.is_pointer_button_down_on(), resp.hovered());
            let (fill, stroke, bottom, fg) = match (self.kind, self.enabled) {
                (_, false) => (
                    if self.kind == Kind::Subtle { Color32::TRANSPARENT } else { p.control_disabled },
                    if self.kind == Kind::Subtle { Color32::TRANSPARENT } else { p.control_stroke },
                    None,
                    p.text3,
                ),
                (Kind::Accent, _) => (
                    if down { p.accent_pressed } else if hover { p.accent_hover } else { p.accent },
                    Color32::TRANSPARENT,
                    (!down).then(|| Color32::from_black_alpha(if p.light { 60 } else { 40 })),
                    if down { p.on_accent.gamma_multiply(0.8) } else { p.on_accent },
                ),
                (Kind::Standard, _) => (
                    if down { p.control_pressed } else if hover { p.control_hover } else { p.control },
                    p.control_stroke,
                    (!down).then_some(p.control_stroke_bottom),
                    if down { p.text2 } else { p.text },
                ),
                (Kind::Subtle, _) => (
                    if down { p.subtle_pressed } else if hover { p.subtle_hover } else { Color32::TRANSPARENT },
                    Color32::TRANSPARENT,
                    None,
                    if down { p.text2 } else { p.text },
                ),
            };
            let painter = ui.painter();
            painter.rect_filled(rect, CONTROL_RADIUS, fill);
            if stroke != Color32::TRANSPARENT {
                painter.rect_stroke(rect, CONTROL_RADIUS, Stroke::new(1.0, stroke), StrokeKind::Inside);
            }
            if let Some(b) = bottom {
                let r = CONTROL_RADIUS as f32;
                let y = rect.bottom() - 0.5;
                painter.hline(rect.left() + r - 1.0..=rect.right() - r + 1.0, y, Stroke::new(1.0, b));
            }
            let mut x = rect.center().x - content / 2.0;
            if let Some(g) = icon {
                let c = if self.enabled { self.tint.unwrap_or(fg) } else { fg };
                let y = rect.center().y - g.size().y / 2.0;
                let gw = g.size().x;
                painter.galley(pos2(x, y), g, c);
                x += gw + gap;
            }
            if let Some(g) = text {
                let y = rect.center().y - g.size().y / 2.0;
                painter.galley(pos2(x, y), g, fg);
            }
        }
        if let Some(t) = self.tip {
            resp = resp.on_hover_text(t);
        }
        if self.enabled && resp.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Default);
        }
        resp
    }
}
