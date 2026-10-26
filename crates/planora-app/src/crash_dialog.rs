//! Rückfrage beim Start nach einem Absturz („PlanoraCAD ist beim letzten Mal abgestürzt“) oder
//! einem unerwarteten Ende ohne Bericht.
//!
//! Das Fenster rechnet nichts selbst: Den Befund liefert [`crate::crash::start_run`], wann es
//! erscheint, regelt die App (`app::PlanoraApp::crash_notice_tick`). „Bericht ansehen“ zeigt den
//! neuesten Bericht bereinigt ([`crate::feedback::sanitize`]); was nach außen geht (Ordner
//! öffnen, Rückmeldung), gibt es als [`CrashAction`] an die App.

use crate::crash::{StartFinding, report_field};
use crate::dialog::{Dialog, DialogSize};
use crate::feedback::{self, Personal};
use crate::theme::{self, SPACE_1, SPACE_2, SPACE_3};
use std::path::{Path, PathBuf};

/// Titel nach einem Absturz mit Bericht.
pub const TITLE: &str = "PlanoraCAD ist beim letzten Mal abgestürzt";
/// Frage unter dem Titel.
pub const QUESTION: &str = "Möchtest du uns helfen?";
/// Erklärung unter der Frage.
pub const EXPLAIN: &str = "Dabei ist ein Bericht entstanden. Er liegt nur auf deinem Rechner. \
     Schickst du ihn uns als Rückmeldung, können wir den Fehler leichter finden.";
/// Zeigt den bereinigten Bericht.
pub const VIEW: &str = "Bericht ansehen";
/// Öffnet das Rückmeldungsfenster mit dem Bericht.
pub const AS_FEEDBACK: &str = "Als Rückmeldung öffnen";
/// Öffnet den Ordner der Berichte.
pub const OPEN_FOLDER: &str = "Ordner öffnen";
/// Schließt; der Bericht bleibt und kommt beim nächsten Start wieder.
pub const NOT_NOW: &str = "Nicht jetzt";
/// Löscht die angebotenen Berichte.
pub const DELETE: &str = "Löschen";
/// Titel nach einem Ende ohne Bericht.
pub const UNEXPECTED_TITLE: &str = "PlanoraCAD wurde beim letzten Mal unerwartet beendet";
/// Text nach einem Ende ohne Bericht.
pub const UNEXPECTED_TEXT: &str = "Wenn du magst, schreib uns über „Als Rückmeldung öffnen“ kurz, \
     was du zuletzt gemacht hast.";

/// Kennung des Fensters.
const WINDOW_ID: &str = "planora_absturz_rueckfrage";
/// Größte Höhe des angezeigten Berichts; darüber rollt er.
const VIEW_MAX_H: f32 = 260.0;

/// Kennung des Fensters (für Lage und Größe in Tests).
pub fn window_id() -> egui::Id {
    egui::Id::new(WINDOW_ID)
}

/// Wahl in der Rückfrage.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CrashAction {
    View,
    AsFeedback,
    OpenFolder,
    NotNow,
    Delete,
}

/// Zustand der Rückfrage (gehört der App, solange sie ansteht).
#[derive(Clone, Debug)]
pub struct CrashNotice {
    /// Angebotene Berichte, neueste zuerst; leer nach einem Ende ohne Bericht.
    pub reports: Vec<PathBuf>,
    /// Ordner der Berichte.
    pub dir: PathBuf,
    /// Wird gezeigt (vorher wartet sie, bis nichts anderes ansteht).
    pub open: bool,
    /// „Bericht ansehen“ wurde geklickt.
    pub viewing: bool,
    /// Bereinigter Text des neuesten Berichts.
    text: String,
    /// Zeile mit Zeit und Version des neuesten Berichts.
    summary: String,
}

impl CrashNotice {
    /// Rückfrage zum Befund; `None` bei [`StartFinding::Clean`]. `dir`: Ordner der Berichte.
    pub fn from_finding(finding: &StartFinding, dir: PathBuf) -> Option<CrashNotice> {
        let reports = match finding {
            StartFinding::Clean => return None,
            StartFinding::Crashed(r) => r.clone(),
            StartFinding::Unexpected => Vec::new(),
        };
        let raw = reports.first().and_then(|p| read(p)).unwrap_or_default();
        let text = feedback::sanitize(&raw, &Personal::from_env(Vec::new()));
        let summary = summary(&text, reports.len());
        Some(CrashNotice {
            reports,
            dir,
            open: false,
            viewing: false,
            text,
            summary,
        })
    }

    /// Ein Absturz mit Bericht (sonst ein Ende ohne Bericht).
    pub fn crashed(&self) -> bool {
        !self.reports.is_empty()
    }

    /// Der neueste Bericht.
    pub fn newest(&self) -> Option<&Path> {
        self.reports.first().map(PathBuf::as_path)
    }

    /// Löscht die angebotenen Berichte (Fehler kommen nur als Art ins Protokoll).
    pub fn delete_reports(&self) {
        let mut failed = 0usize;
        for p in &self.reports {
            if let Err(e) = std::fs::remove_file(p)
                && e.kind() != std::io::ErrorKind::NotFound
            {
                failed += 1;
                crate::diag::warn(&format!("Absturzbericht nicht gelöscht ({:?})", e.kind()));
            }
        }
        crate::diag::info(&format!(
            "Absturzberichte gelöscht ({} von {})",
            self.reports.len() - failed,
            self.reports.len()
        ));
    }
}

fn read(path: &Path) -> Option<String> {
    std::fs::read(path)
        .ok()
        .map(|b| String::from_utf8_lossy(&b).into_owned())
}

/// „Bericht vom …, Version …“, bei mehreren Berichten mit Anzahl.
fn summary(text: &str, count: usize) -> String {
    let mut s = String::from("Bericht");
    if let Some(t) = report_field(text, "Zeit") {
        s.push_str(&format!(" vom {t}"));
    }
    if let Some(v) = report_field(text, "Version") {
        s.push_str(&format!(", Version {v}"));
    }
    if count > 1 {
        s.push_str(&format!(" (dazu {} ältere)", count - 1));
    }
    s
}

/// Zeichnet die Rückfrage, solange `state.open` gilt. „Bericht ansehen“ und „Nicht jetzt“
/// erledigt sie selbst (zeigen bzw. schließen), alles andere führt die App aus.
pub fn show(ctx: &egui::Context, state: &mut CrashNotice, dark: bool) -> Option<CrashAction> {
    if !state.open {
        return None;
    }
    let crashed = state.crashed();
    let title = if crashed { TITLE } else { UNEXPECTED_TITLE };
    let mut open = true;
    let mut action = None;
    let viewing = state.viewing;
    Dialog::new(title, DialogSize::Wide)
        .id(window_id())
        .center_on_open()
        .open(&mut open)
        .show_with_footer(
            ctx,
            |ui| {
                if !crashed {
                    ui.add(egui::Label::new(UNEXPECTED_TEXT).wrap());
                    return;
                }
                ui.strong(QUESTION);
                ui.add_space(SPACE_1);
                ui.add(egui::Label::new(EXPLAIN).wrap());
                ui.add_space(SPACE_2);
                ui.label(
                    egui::RichText::new(&state.summary).color(theme::palette(dark).text_secondary),
                );
                if viewing {
                    ui.add_space(SPACE_3);
                    theme::scroll_area(
                        ui,
                        egui::ScrollArea::vertical()
                            .id_salt("planora_absturz_bericht")
                            .max_height(VIEW_MAX_H)
                            .auto_shrink([false, true]),
                        |ui| {
                            ui.add(
                                egui::Label::new(egui::RichText::new(&state.text).monospace())
                                    .wrap(),
                            );
                        },
                    );
                }
            },
            |ui| {
                ui.add_space(SPACE_2);
                ui.horizontal_wrapped(|ui| {
                    let mut button = |ui: &mut egui::Ui, w: egui::Button<'_>, a: CrashAction| {
                        let r = ui.add(w);
                        theme::focus_ring(ui, &r);
                        if r.clicked() {
                            action = Some(a);
                        }
                    };
                    button(
                        ui,
                        theme::primary_button(AS_FEEDBACK, dark),
                        CrashAction::AsFeedback,
                    );
                    if crashed {
                        button(ui, egui::Button::new(VIEW), CrashAction::View);
                        button(ui, egui::Button::new(OPEN_FOLDER), CrashAction::OpenFolder);
                    }
                    button(ui, egui::Button::new(NOT_NOW), CrashAction::NotNow);
                    if crashed {
                        button(ui, egui::Button::new(DELETE), CrashAction::Delete);
                    }
                });
            },
        );
    if !open {
        action = Some(CrashAction::NotNow);
    }
    match action {
        Some(CrashAction::View) => state.viewing = true,
        Some(CrashAction::NotNow) => state.open = false,
        _ => {}
    }
    action
}
