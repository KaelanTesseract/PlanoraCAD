//! Das Fenster der Hülle im Graphit-Stil (Entwurf C): links das Feld „Installation“ mit den fünf
//! Schritten, rechts der Inhalt der Seite. Hier laufen auch die Wirkungen nach außen: Motor
//! auspacken und in eigenem Faden starten, Fortschrittsdatei lesen, Ordnerdialog, App starten.

mod assets;
mod folder_dialog;
mod pages;
mod steps;
mod style;

use crate::args::{EngineChoice, engine_args};
use crate::engine::{self, RunError, Unpacked};
use crate::progress::{self, Tracker};
use crate::system;
use crate::wizard::{Page, Wizard};
use egui::{Margin, Rect, StrokeKind, UiBuilder, pos2, vec2};
use pages::{Action, View};
use planora_i18n::tr;
use std::path::{Path, PathBuf};
use std::thread::JoinHandle;
use std::time::Duration;

/// Fenstergröße in logischen Punkten (fest).
const WINDOW_SIZE: [f32; 2] = [960.0, 640.0];
/// Anteil des linken Felds an der Fensterbreite (wie in der Vorschau).
const STEPS_FRACTION: f32 = 0.26;
/// Abstand der Felder zum Fensterrand und zueinander.
const GAP: f32 = style::SPACE_3;
/// Innenabstand des linken Felds (wie `padding: 14px 10px` der Vorschau).
const STEPS_MARGIN: Margin = Margin::symmetric(10, 14);
/// Innenabstand des rechten Felds (wie `padding: 20px 22px` der Vorschau).
const PAGE_MARGIN: Margin = Margin::symmetric(22, 20);
/// Abfrage von Fortschritt und Motor während der Installation.
const POLL: Duration = Duration::from_millis(100);
/// Dateiname der App im Installationsordner.
const APP_EXE: &str = "PlanoraCAD.exe";

/// Öffnet das Fenster und läuft, bis es geschlossen wird.
pub fn run(wizard: Wizard) -> eframe::Result<()> {
    let mut viewport = egui::ViewportBuilder::default()
        .with_title(tr!("setup-window-title"))
        .with_inner_size(WINDOW_SIZE)
        .with_resizable(false)
        .with_maximize_button(false);
    // Ohne lesbares Symbol bleibt das Standardsymbol des Systems.
    if let Some(icon) = assets::window_icon() {
        viewport = viewport.with_icon(icon);
    }
    let options = eframe::NativeOptions {
        viewport,
        renderer: eframe::Renderer::Wgpu,
        centered: true,
        ..Default::default()
    };
    eframe::run_native(
        "planoracad-setup",
        options,
        Box::new(move |cc| Ok(Box::new(SetupApp::new(&cc.egui_ctx, wizard)))),
    )
}

/// Linkes und rechtes Feld im Fenster `screen`: Abstand `GAP` zum Rand und zueinander, links
/// `STEPS_FRACTION` der Breite.
pub fn panel_rects(screen: Rect) -> (Rect, Rect) {
    let inner = screen.shrink(GAP);
    let left_w = (screen.width() * STEPS_FRACTION).round();
    let left = Rect::from_min_size(inner.min, vec2(left_w, inner.height()));
    let right = Rect::from_min_max(pos2(left.right() + GAP, inner.top()), inner.max);
    (left, right)
}

/// Ein laufender Motor.
struct EngineRun {
    unpacked: Unpacked,
    progress_file: PathBuf,
    log_file: PathBuf,
    /// Start und Warten im eigenen Faden, damit das Fenster während der Admin-Abfrage und der
    /// Installation weiterzeichnet.
    thread: JoinHandle<Result<u32, RunError>>,
}

struct SetupApp {
    wizard: Wizard,
    assets: assets::Assets,
    /// Zuletzt gewähltes Thema; `None` vor dem ersten Bild.
    dark: Option<bool>,
    tracker: Tracker,
    run: Option<EngineRun>,
    /// Nach einem Fehler bleibt der ausgepackte Ordner mit dem Protokoll bis „Schließen“.
    kept: Option<Unpacked>,
    lang: &'static str,
}

impl SetupApp {
    fn new(ctx: &egui::Context, wizard: Wizard) -> SetupApp {
        style::install(ctx);
        SetupApp {
            wizard,
            assets: assets::Assets::load(ctx),
            dark: None,
            tracker: Tracker::new(),
            run: None,
            kept: None,
            lang: system::ui_lang(),
        }
    }

    /// Thema nach Windows (fehlt die Angabe: dunkel); nur bei einer Änderung neu setzen.
    fn follow_system_theme(&mut self, ctx: &egui::Context) -> bool {
        let dark = style::is_dark(ctx.system_theme());
        if self.dark != Some(dark) {
            style::set_theme(ctx, dark);
            self.dark = Some(dark);
        }
        dark
    }

    /// „Installieren“: prüfen, auspacken, Motor im eigenen Faden starten.
    fn start_install(&mut self, ctx: &egui::Context) {
        let running = system::running_in(Path::new(&self.wizard.dir));
        if !self.wizard.start_install(running) {
            return;
        }
        self.tracker = Tracker::new();
        let unpacked = match Unpacked::unpack() {
            Ok(u) => u,
            Err(err) => {
                // Ohne ausgepackten Motor gibt es kein Protokoll.
                let log = std::env::temp_dir().join("planoracad-setup.log");
                self.wizard.engine_finished(Err(RunError::Io(err)), log);
                return;
            }
        };
        let progress_file = unpacked.dir.join("progress.txt");
        let log_file = unpacked.dir.join("setup.log");
        let choice = EngineChoice {
            all_users: self.wizard.all_users,
            dir: PathBuf::from(&self.wizard.dir),
            desktop_icon: self.wizard.desktop_icon,
            lang: self.lang,
            progress_file: progress_file.clone(),
            log_file: log_file.clone(),
        };
        let args = engine_args(&choice);
        let exe = unpacked.exe.clone();
        let elevate = self.wizard.all_users;
        let repaint = ctx.clone();
        let thread = std::thread::spawn(move || {
            let result = engine::spawn(&exe, &args, elevate).map(|process| process.wait());
            repaint.request_repaint();
            result
        });
        self.run = Some(EngineRun {
            unpacked,
            progress_file,
            log_file,
            thread,
        });
    }

    /// Während der Installation: Fortschritt lesen; ist der Motor fertig, Ergebnis übernehmen.
    fn poll_engine(&mut self, ctx: &egui::Context) {
        let Some(run) = &self.run else {
            return;
        };
        if let Some(p) = std::fs::read_to_string(&run.progress_file)
            .ok()
            .as_deref()
            .and_then(progress::parse)
        {
            self.tracker.update(p);
        }
        if !run.thread.is_finished() {
            ctx.request_repaint_after(POLL);
            return;
        }
        let Some(run) = self.run.take() else {
            return;
        };
        let result = run.thread.join().unwrap_or_else(|_| {
            Err(RunError::Io(std::io::Error::other(
                "engine thread panicked",
            )))
        });
        self.wizard.engine_finished(result, run.log_file);
        if matches!(self.wizard.page, Page::Failed { .. }) {
            self.kept = Some(run.unpacked);
        } else {
            run.unpacked.cleanup();
        }
    }

    fn apply(&mut self, action: Action, ctx: &egui::Context) {
        match action {
            Action::Cancel => {
                if self.wizard.page != Page::Installing {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
            Action::Back => self.wizard.back(),
            Action::Next => self.wizard.next(),
            Action::Install => self.start_install(ctx),
            Action::Recheck => {
                let running = system::running_in(Path::new(&self.wizard.dir));
                self.wizard.recheck_running(running);
            }
            Action::Browse => {
                if let Some(dir) = folder_dialog::pick(Path::new(&self.wizard.dir)) {
                    self.wizard.set_dir(dir.to_string_lossy().into_owned());
                }
            }
            Action::OpenLog => {
                if let Page::Failed { log } = &self.wizard.page {
                    open_log(log);
                }
            }
            Action::Finish => {
                if self.wizard.start_app {
                    start_app(Path::new(&self.wizard.dir));
                }
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            Action::Close => {
                if let Some(kept) = self.kept.take() {
                    kept.cleanup();
                }
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }
}

impl eframe::App for SetupApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Während der Installation lässt sich das Fenster nicht schließen (Abbrechen ist dort
        // ebenfalls gesperrt); der Motor liefe sonst ohne Anzeige weiter.
        if self.run.is_some() && ctx.input(|i| i.viewport().close_requested()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        }
        self.poll_engine(ctx);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let dark = self.follow_system_theme(&ctx);
        let p = planora_style::palette(dark);
        let screen = ui.max_rect();
        ui.painter().rect_filled(screen, 0.0, p.bg_viewport);
        let (left, right) = panel_rects(screen);

        let frame = style::panel_frame(p);
        for rect in [left, right] {
            ui.painter().rect(
                rect,
                frame.corner_radius,
                frame.fill,
                frame.stroke,
                StrokeKind::Inside,
            );
        }

        let steps_rect = left - STEPS_MARGIN;
        let mut steps_ui = ui.new_child(
            UiBuilder::new()
                .max_rect(steps_rect)
                .layout(egui::Layout::top_down(egui::Align::Min)),
        );
        steps::show(&mut steps_ui, &self.wizard, p);

        let log_exists = match &self.wizard.page {
            Page::Failed { log } => log.is_file(),
            _ => false,
        };
        let view = View {
            p,
            assets: &self.assets,
            progress: self.tracker.current(),
            version: env!("CARGO_PKG_VERSION"),
            log_exists,
        };
        let action = pages::show(ui, right - PAGE_MARGIN, &mut self.wizard, &view);
        if let Some(action) = action {
            self.apply(action, &ctx);
        }
    }

    fn on_exit(&mut self) {
        if let Some(kept) = self.kept.take() {
            kept.cleanup();
        }
    }
}

/// Öffnet das Protokoll im Editor (Fehler werden ignoriert, der Knopf bleibt einfach ohne
/// Wirkung).
fn open_log(log: &Path) {
    #[cfg(windows)]
    {
        let _ = std::process::Command::new("notepad.exe").arg(log).spawn();
    }
    #[cfg(not(windows))]
    {
        let _ = log;
    }
}

/// Startet die installierte App mit den Rechten der Hülle (ohne Admin).
fn start_app(dir: &Path) {
    let _ = std::process::Command::new(dir.join(APP_EXE))
        .current_dir(dir)
        .spawn();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn panels_float_with_gap_and_steps_take_a_quarter() {
        let screen = Rect::from_min_size(pos2(0.0, 0.0), vec2(960.0, 640.0));
        let (left, right) = panel_rects(screen);
        assert_eq!(left.min, pos2(12.0, 12.0));
        assert_eq!(left.width(), 250.0);
        assert_eq!(right.left() - left.right(), 12.0);
        assert_eq!(right.max, pos2(948.0, 628.0));
        assert_eq!(left.height(), right.height());
    }
}
