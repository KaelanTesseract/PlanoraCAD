//! Rückfrage beim Start nach einem Absturz („PlanoraCAD ist beim letzten Mal abgestürzt“) oder
//! einem unerwarteten Ende ohne Bericht.
//!
//! Das Fenster rechnet nichts selbst: Den Befund liefert [`crate::crash::start_run`], wann es
//! erscheint, regelt die App (`app::PlanoraApp::crash_notice_tick`). „Bericht ansehen“ zeigt den
//! neuesten Bericht bereinigt (und wird dann zu „Bericht ausblenden“) ([`crate::feedback::sanitize`]); was nach außen geht (Ordner
//! öffnen, Rückmeldung), gibt es als [`CrashAction`] an die App.

use crate::crash::{StartFinding, TIME_KEY, report_field, report_field_either};
use crate::dialog::{Dialog, DialogSize};
use crate::feedback::{self, Personal};
use crate::theme::{self, SPACE_1, SPACE_2, SPACE_3};
use planora_i18n::tr;
use std::path::{Path, PathBuf};

/// Titel nach einem Absturz mit Bericht.
pub fn title() -> String {
    tr!("crash-title")
}
/// Frage unter dem Titel.
pub fn question() -> String {
    tr!("crash-question")
}
/// Erklärung unter der Frage.
pub fn explain() -> String {
    tr!("crash-explain")
}
/// Zeigt den bereinigten Bericht.
pub fn view() -> String {
    tr!("crash-view")
}
/// Statt [`view`], solange der Bericht gezeigt wird: blendet ihn wieder aus.
pub fn hide() -> String {
    tr!("crash-hide")
}
/// Öffnet das Rückmeldungsfenster mit dem Bericht.
pub fn as_feedback() -> String {
    tr!("crash-as-feedback")
}
/// Öffnet den Ordner der Berichte.
pub fn open_folder() -> String {
    tr!("crash-open-folder")
}
/// Schließt; der Bericht bleibt und kommt beim nächsten Start wieder.
pub fn not_now() -> String {
    tr!("crash-not-now")
}
/// Löscht die angebotenen Berichte.
pub fn delete() -> String {
    tr!("crash-delete")
}
/// Titel nach einem Ende ohne Bericht.
pub fn unexpected_title() -> String {
    tr!("crash-unexpected-title")
}
/// Text nach einem Ende ohne Bericht.
pub fn unexpected_text() -> String {
    tr!("crash-unexpected-text")
}

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
    /// Der Bericht wird gezeigt („Bericht ansehen“; „Bericht ausblenden“ schaltet zurück).
    pub viewing: bool,
    /// Bereinigter Text des neuesten Berichts.
    text: String,
    /// Angaben für die Zeile mit Zeit und Version des neuesten Berichts; der Text entsteht
    /// erst beim Zeichnen, in der Sprache des Fensters.
    summary: Summary,
}

/// Zeit und Version des neuesten Berichts und die Zahl der älteren.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Summary {
    time: Option<String>,
    version: Option<String>,
    older: usize,
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

    /// Legt die angebotenen Berichte nach `gesendet/` ([`crate::crash::mark_sent`]): Nach
    /// „Als Rückmeldung öffnen“ gelten sie als erledigt und kommen nicht wieder. Gibt den neuen
    /// Ort des neuesten zurück (bleibt er liegen, den alten).
    pub fn mark_sent(&self) -> Option<PathBuf> {
        let mut newest = None;
        for (i, p) in self.reports.iter().enumerate() {
            let to = match crate::crash::mark_sent(p) {
                Ok(to) => to,
                Err(e) => {
                    crate::diag::warn(&format!("Crash report not moved ({:?})", e.kind()));
                    p.clone()
                }
            };
            if i == 0 {
                newest = Some(to);
            }
        }
        newest
    }

    /// Löscht die angebotenen Berichte samt Begleitdatei (Fehler kommen nur als Art ins
    /// Protokoll).
    pub fn delete_reports(&self) {
        let mut failed = 0usize;
        for p in &self.reports {
            if let Err(e) = crate::crash::remove_report(p)
                && e.kind() != std::io::ErrorKind::NotFound
            {
                failed += 1;
                crate::diag::warn(&format!("Crash report not deleted ({:?})", e.kind()));
            }
        }
        crate::diag::info(&format!(
            "Crash reports deleted ({} of {})",
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

/// Zeit und Version aus dem Bericht (auch aus älteren, deutschen Berichten) und die Zahl der
/// älteren Berichte.
fn summary(text: &str, count: usize) -> Summary {
    Summary {
        time: report_field_either(text, TIME_KEY, "Zeit").map(str::to_string),
        version: report_field(text, "Version").map(str::to_string),
        older: count.saturating_sub(1),
    }
}

/// Die Zeit des Berichts in der Schreibweise der Sprache; eine Zeit, die sich nicht lesen lässt,
/// steht, wie sie ist.
fn shown_time(t: &str) -> String {
    let parsed = chrono::NaiveDateTime::parse_from_str(t, crate::crash::REPORT_TIME_FORMAT)
        .or_else(|_| chrono::NaiveDateTime::parse_from_str(t, "%d.%m.%Y, %H:%M:%S"));
    match parsed {
        Ok(d) => tr!(
            "crash-summary-date",
            day = d.format("%d").to_string(),
            month = d.format("%m").to_string(),
            year = d.format("%Y").to_string(),
            time = d.format("%H:%M:%S").to_string()
        ),
        Err(_) => t.to_string(),
    }
}

/// „Bericht vom …, Version …“, bei mehreren Berichten mit Anzahl (in der aktuellen Sprache).
fn summary_text(s: &Summary) -> String {
    let mut out = tr!("crash-summary-report");
    if let Some(t) = &s.time {
        out.push_str(&tr!("crash-summary-time", time = shown_time(t)));
    }
    if let Some(v) = &s.version {
        out.push_str(&tr!("crash-summary-version", version = v.as_str()));
    }
    if s.older > 0 {
        out.push_str(&tr!("crash-summary-older", count = s.older as i64));
    }
    out
}

/// Zeichnet die Rückfrage, solange `state.open` gilt. „Bericht ansehen“ und „Nicht jetzt“
/// erledigt sie selbst (zeigen bzw. schließen), alles andere führt die App aus.
pub fn show(ctx: &egui::Context, state: &mut CrashNotice, dark: bool) -> Option<CrashAction> {
    if !state.open {
        return None;
    }
    let crashed = state.crashed();
    let title = if crashed { title() } else { unexpected_title() };
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
                    ui.add(egui::Label::new(unexpected_text()).wrap());
                    return;
                }
                ui.strong(question());
                ui.add_space(SPACE_1);
                ui.add(egui::Label::new(explain()).wrap());
                ui.add_space(SPACE_2);
                ui.label(
                    egui::RichText::new(summary_text(&state.summary))
                        .color(theme::palette(dark).text_secondary),
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
                        theme::primary_button(&as_feedback(), dark),
                        CrashAction::AsFeedback,
                    );
                    if crashed {
                        let label = if viewing { hide() } else { view() };
                        button(ui, egui::Button::new(label), CrashAction::View);
                        button(
                            ui,
                            egui::Button::new(open_folder()),
                            CrashAction::OpenFolder,
                        );
                    }
                    button(ui, egui::Button::new(not_now()), CrashAction::NotNow);
                    if crashed {
                        button(ui, egui::Button::new(delete()), CrashAction::Delete);
                    }
                });
            },
        );
    if !open {
        action = Some(CrashAction::NotNow);
    }
    match action {
        Some(CrashAction::View) => state.viewing = !state.viewing,
        Some(CrashAction::NotNow) => state.open = false,
        _ => {}
    }
    action
}
