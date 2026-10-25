//! Meldung „Neue Version verfügbar“: Versionsnummer mit Kanal, die laufende Version, die
//! Patchnotes aller Versionen zwischen der laufenden und der neuen (neueste oben, je mit
//! Überschrift „Version X“) und die Knöpfe „Jetzt aktualisieren“, „Später“ und „Diese Version
//! überspringen“. Die Patchnotes kommen schon zerlegt aus [`parse_notes`]; beim Anzeigen wird
//! nichts aus dem Netz geholt.
//!
//! **Schließen.** ✕ und Esc wirken wie „Später“ (nie wie „Überspringen“): die Meldung kommt in
//! dieser Sitzung frühestens nach [`SNOOZE_SECS`] wieder von allein. Gemerkt wird das nur im
//! Speicher; beim nächsten Programmstart kommt sie wieder, wenn die Prüfung wieder ein Update
//! findet. „Diese Version überspringen“ speichert die Version
//! ([`Workspace::set_skipped_version`]); die Prüfung bietet sie dann nie wieder an, eine neuere
//! aber schon.
//!
//! Wann die Meldung von allein erscheint, welche Fenster sie schließen und welche sie schließt,
//! regelt die App (`app::show_update_notice`, `app::open_update_notice`,
//! `app::enforce_update_rules`).

use crate::dialog::{Dialog, DialogSize};
use crate::workspace::Workspace;
use crate::{fonts, running_version, theme};
use planora_update::{NoteBlock, Span, Update, Version, parse_notes};

/// Knopf: das Update herunterladen und einspielen.
pub const INSTALL_NOW: &str = "Jetzt aktualisieren";
/// Knopf: schließen, frühestens nach [`SNOOZE_SECS`] wieder von allein zeigen.
pub const LATER: &str = "Später";
/// Knopf: diese Version nie wieder anbieten.
pub const SKIP: &str = "Diese Version überspringen";
/// Unter den Patchnotes, wenn sie wegen der Obergrenzen gekürzt sind.
pub const MORE_ON_GITHUB: &str = "… und weitere Änderungen auf GitHub.";
/// Statt der Notizen einer Version, die keine Beschreibung hat.
pub const NO_NOTES: &str = "Keine Beschreibung.";
/// Statuszeile nach „Jetzt aktualisieren“, bis das Update läuft.
pub const PREPARING: &str = "Update wird vorbereitet …";
/// So lange zeigt sich die Meldung nach „Später“ nicht von allein (24 Stunden).
pub const SNOOZE_SECS: u64 = 24 * 60 * 60;
/// Größte Höhe der Patchnotes; darüber rollen sie (Kopf und Knöpfe bleiben stehen).
pub const NOTES_MAX_H: f32 = 520.0;
/// So viel Fensterhöhe bleibt für Kopfzeile, Zeile „Du hast …“, Hinweis, Fußzeile und Rand.
const NOTES_RESERVE_H: f32 = 320.0;
/// Kennung des Fensters (unabhängig vom Titel, der die Version nennt).
const WINDOW_ID: &str = "planora_update_notice";

/// Titel: „Neue Version verfügbar: 0.2.0-dev.2 (Dev)“.
pub fn title(v: &Version) -> String {
    format!("Neue Version verfügbar: {v} ({})", v.channel().label())
}

/// „Du hast Version 0.2.0-dev.1“.
pub fn running_line(v: &Version) -> String {
    format!("Du hast Version {v}")
}

/// Überschrift der Notizen einer Version: „Version 0.2.0-dev.2“.
pub fn version_heading(v: &Version) -> String {
    format!("Version {v}")
}

/// Hinweis in der Statuszeile, solange ein Update vorgemerkt und die Meldung zu ist (ein Klick
/// öffnet sie): „Update verfügbar: 0.2.0-dev.2“.
pub fn status_hint(v: &Version) -> String {
    format!("Update verfügbar: {v}")
}

/// Was in der Meldung gewählt wurde.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UpdateAction {
    /// „Jetzt aktualisieren“.
    InstallNow,
    /// „Später“, ✕ oder Esc.
    Later,
    /// „Diese Version überspringen“.
    Skip,
}

/// Zustand der Meldung zwischen den Bildern (gehört der App).
#[derive(Clone, Debug, Default)]
pub struct UpdateDialog {
    pub open: bool,
    /// Das angezeigte (nach „Jetzt aktualisieren“: das einzuspielende) Update.
    pub update: Option<Update>,
    /// Nach „Später“: vor diesem Zeitpunkt (Unix-Sekunden) erscheint die Meldung nicht von allein.
    /// Nur für die Sitzung, nicht gespeichert.
    pub snooze_until: Option<u64>,
    /// „Jetzt aktualisieren“ wurde gewählt; die Meldung kommt nicht mehr von allein.
    pub installing: bool,
}

impl UpdateDialog {
    /// Zeigt die Meldung für `update`.
    pub fn open_with(&mut self, update: Update) {
        self.update = Some(update);
        self.open = true;
    }

    /// Schließt die Meldung ohne Wahl (ein anderes Fenster oder eine Rückfrage geht vor); sie
    /// erscheint wieder, sobald nichts anderes mehr offen ist.
    pub fn close(&mut self) {
        self.open = false;
    }

    /// Ist „Später“ zum Zeitpunkt `now` noch in Kraft?
    pub fn snoozed(&self, now: u64) -> bool {
        self.snooze_until.is_some_and(|until| now < until)
    }

    /// Darf die Meldung zum Zeitpunkt `now` von allein erscheinen (nicht schon offen, nicht nach
    /// „Später“ und nicht nach „Jetzt aktualisieren“)?
    pub fn may_auto_open(&self, now: u64) -> bool {
        !self.open && !self.installing && !self.snoozed(now)
    }

    /// Führt `action` aus: die Meldung schließt; „Später“ merkt `now` + [`SNOOZE_SECS`],
    /// „Überspringen“ speichert die Version, „Jetzt aktualisieren“ setzt `installing`.
    pub fn apply(&mut self, ws: &mut Workspace, action: UpdateAction, now: u64) {
        self.open = false;
        match action {
            UpdateAction::Later => self.snooze_until = Some(now.saturating_add(SNOOZE_SECS)),
            UpdateAction::Skip => {
                if let Some(update) = self.update.take() {
                    // Misslingt das Speichern, gilt das Überspringen bis zum Programmende.
                    let _ = ws.set_skipped_version(Some(update.latest.version));
                }
            }
            UpdateAction::InstallNow => self.installing = true,
        }
    }
}

/// Esc schließt die offene Meldung wie „Später“ und wird verbraucht (vor den Ansichten
/// aufrufen). Nicht bei offener Rückfrage und nicht, solange ein Textfeld die Tastatur hat.
pub fn take_escape(ctx: &egui::Context, state: &UpdateDialog, prompt_open: bool) -> bool {
    state.open
        && !prompt_open
        && !crate::viewport::typing(ctx)
        && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
}

/// Höhe der Patchnotes: was das Fenster hergibt, höchstens [`NOTES_MAX_H`].
fn notes_height(ctx: &egui::Context) -> f32 {
    (ctx.content_rect().height() - NOTES_RESERVE_H).clamp(120.0, NOTES_MAX_H)
}

/// Zeichnet die Meldung, solange `state.open` gilt, und liefert die Wahl (✕ zählt als
/// [`UpdateAction::Later`]). Ausgeführt wird sie hier schon ([`UpdateDialog::apply`]).
pub fn show(
    ctx: &egui::Context,
    ws: &mut Workspace,
    state: &mut UpdateDialog,
) -> Option<UpdateAction> {
    if !state.open {
        return None;
    }
    let Some(update) = state.update.as_ref() else {
        state.open = false;
        return None;
    };
    let dark = ctx.global_style().visuals.dark_mode;
    let mut open = true;
    let mut action = None;
    Dialog::new(title(&update.latest.version), DialogSize::Wide)
        .id(egui::Id::new(WINDOW_ID))
        .center_on_open()
        .open(&mut open)
        .show_with_footer(
            ctx,
            |ui| {
                ui.label(running_line(&running_version()));
                ui.add_space(theme::SPACE_2);
                theme::scroll_area(
                    ui,
                    egui::ScrollArea::vertical()
                        .id_salt("update_patchnotes")
                        .auto_shrink([false, true])
                        .max_height(notes_height(ctx)),
                    |ui| notes(ui, update, dark),
                );
                if update.truncated {
                    ui.add_space(theme::SPACE_2);
                    ui.label(
                        egui::RichText::new(MORE_ON_GITHUB)
                            .color(theme::palette(dark).text_secondary),
                    );
                }
            },
            |ui| {
                ui.horizontal(|ui| {
                    if ui.add(theme::primary_button(INSTALL_NOW, dark)).clicked() {
                        action = Some(UpdateAction::InstallNow);
                    }
                    if ui.button(LATER).clicked() {
                        action = Some(UpdateAction::Later);
                    }
                    if ui.button(SKIP).clicked() {
                        action = Some(UpdateAction::Skip);
                    }
                });
            },
        );
    if !open && action.is_none() {
        action = Some(UpdateAction::Later);
    }
    if let Some(a) = action {
        state.apply(ws, a, crate::update_check::now_unix());
    }
    action
}

/// Die Notizen aller Versionen, neueste oben, je mit Überschrift „Version X“.
fn notes(ui: &mut egui::Ui, update: &Update, dark: bool) {
    for (i, (version, text)) in update.notes_between.iter().enumerate() {
        if i > 0 {
            ui.add_space(theme::SPACE_3);
        }
        theme::section_header(ui, &version_heading(version), theme::SUB_HEADER_H);
        ui.add_space(theme::SPACE_1);
        let blocks = parse_notes(text);
        if blocks.is_empty() {
            ui.label(egui::RichText::new(NO_NOTES).color(theme::palette(dark).text_secondary));
        }
        ui.push_id(i, |ui| {
            for block in &blocks {
                note_block(ui, block, dark);
            }
        });
    }
}

/// Ein Block: Überschriften größer und halbfett, Aufzählungspunkte mit Spiegelstrich und
/// Einzug, Absätze; fetter Text halbfett in der Hauptfarbe.
fn note_block(ui: &mut egui::Ui, block: &NoteBlock, dark: bool) {
    let body = egui::TextStyle::Body.resolve(ui.style());
    match block {
        NoteBlock::Heading(level, spans) => {
            ui.add_space(theme::SPACE_1);
            let size = body.size + heading_extra(*level);
            ui.add(egui::Label::new(job(spans, size, true, dark)).wrap());
        }
        NoteBlock::Bullet(spans) => {
            ui.horizontal_top(|ui| {
                ui.label("–");
                ui.add(egui::Label::new(job(spans, body.size, false, dark)).wrap());
            });
        }
        NoteBlock::Paragraph(spans) => {
            ui.add(egui::Label::new(job(spans, body.size, false, dark)).wrap());
            ui.add_space(theme::SPACE_1);
        }
    }
}

/// So viel größer als der Fließtext ist eine Überschrift der Ebene `level` (1 bis 3).
pub fn heading_extra(level: u8) -> f32 {
    match level {
        1 => 5.0,
        2 => 3.0,
        _ => 1.0,
    }
}

/// Text aus `spans` in Schriftgröße `size`; fette Stücke (und alles bei `all_bold`) halbfett
/// in der Hauptfarbe, die übrigen in der normalen Textfarbe.
fn job(spans: &[Span], size: f32, all_bold: bool, dark: bool) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::default();
    let strong = theme::palette(dark).text_primary;
    for span in spans {
        let bold = all_bold || span.bold;
        let font = if bold {
            egui::FontId::new(size, fonts::semibold())
        } else {
            egui::FontId::proportional(size)
        };
        let color = if bold {
            strong
        } else {
            egui::Color32::PLACEHOLDER
        };
        job.append(&span.text, 0.0, egui::TextFormat::simple(font, color));
    }
    job
}
