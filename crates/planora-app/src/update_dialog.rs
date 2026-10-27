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
//! ([`Workspace::set_skipped_version`]); die Prüfung beim Start bietet sie dann nicht wieder an,
//! eine neuere aber schon. Die Suche von Hand übergeht das Überspringen und hebt „Später“ auf;
//! in den Einstellungen nimmt „Doch wieder anzeigen“ das Überspringen zurück.
//!
//! **Aktualisieren.** Nach „Jetzt aktualisieren“ bleibt die Meldung offen und zeigt statt der
//! Knöpfe den Stand ([`InstallView`]): Fortschritt mit „Abbrechen“, „Update ist bereit …“
//! oder einen Fehler. Ein Fehler steht deutlich da: Titel [`FAILED_TITLE`], die Meldung mit
//! Warnsymbol in Warnfarbe über den Patchnotes (die weiter zu lesen sind), Knöpfe „Erneut
//! versuchen“ (wo es helfen kann), „Download-Seite öffnen“ und „Schließen“. Während des
//! Neustarts bleibt die Fußzeile leer; dann sagt es der Kasten über allem. ✕ und Esc blenden
//! sie während des Updates nur aus (der
//! Download läuft weiter, die Statuszeile zeigt den Stand); nach einem Fehler schließen sie wie
//! „Schließen“. Ist diese Fassung nicht installiert (Entwicklung), steht statt „Jetzt
//! aktualisieren“ ein Satz dazu ([`update_install::NOT_INSTALLED_HINT`]) und „Download-Seite
//! öffnen“ ist der Hauptknopf. Den Ablauf selbst führt [`crate::update_install`] mit der App aus. Nach dem
//! Neustart zeigt [`show_after_update`] einmal „Aktualisiert auf Version X“.
//!
//! **Lage.** Die Meldung steht fest unten rechts über der Verlaufsleiste ([`notice_area`]), so
//! schmal, dass mittige Fenster wie „Quader erzeugen“ neben ihr Platz haben und sie nicht
//! verdecken. Der Hinweis nach einem Update steht mittig.
//!
//! Wann die Meldung von allein erscheint, welche Fenster sie schließen und welche sie schließt,
//! regelt die App (`app::show_update_notice`, `app::open_update_notice`,
//! `app::enforce_update_rules`).

use crate::dialog::{Dialog, DialogSize};
use crate::icons::{self, Icon};
use crate::update_install::{self, AfterUpdate};
use crate::workspace::Workspace;
use crate::{fonts, running_version, theme};
use planora_update::{NoteBlock, Span, Update, Version, parse_notes};

/// Knopf: das Update herunterladen und einspielen.
pub const INSTALL_NOW: &str = "Jetzt aktualisieren";
/// Knopf: schließen, frühestens nach [`SNOOZE_SECS`] wieder von allein zeigen.
pub const LATER: &str = "Später";
/// Knopf: diese Version beim Start nicht wieder anbieten (die Suche von Hand zeigt sie weiter).
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
/// Kennung des Hinweises nach einem Update.
const DONE_WINDOW_ID: &str = "planora_update_done";
/// Breite des Fortschrittsbalkens.
const PROGRESS_W: f32 = 360.0;
/// Knopf während des Downloads: bricht das Update ab.
pub const CANCEL: &str = "Abbrechen";
/// Knopf nach einem Fehler: schließt die Meldung.
pub const CLOSE: &str = "Schließen";
/// Im Hinweis nach einem Update, wenn keine Patchnotes hinterlegt sind.
pub const NOTES_ON_GITHUB: &str = "Was neu ist, steht auf GitHub.";
/// Knopf des Hinweises nach einem Update.
pub const OK: &str = "OK";
/// Titel der Meldung nach einem Fehler.
pub const FAILED_TITLE: &str = "Update hat nicht geklappt";
/// Knopf nach einem Fehler: das Update noch einmal versuchen.
pub const RETRY: &str = "Erneut versuchen";
/// Größe des Warnsymbols vor einer Fehlermeldung.
const WARNING_ICON_SIZE: f32 = 20.0;

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
    /// „Abbrechen“ während des Downloads.
    CancelInstall,
    /// ✕ oder Esc während des Downloads: die Meldung verschwindet, der Download läuft weiter
    /// (ein Klick auf die Statuszeile holt sie zurück).
    Hide,
    /// „Schließen“, ✕ oder Esc nach einem Fehler.
    CloseFailed,
    /// „Download-Seite öffnen“ nach einem Fehler.
    OpenDownloadPage,
    /// „Erneut versuchen“ nach einem Fehler.
    Retry,
}

/// Was die Meldung nach „Jetzt aktualisieren“ statt der Knöpfe zeigt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InstallView<'a> {
    /// Noch nichts gestartet: [`PREPARING`].
    Preparing,
    /// Fortschrittsbalken und „Abbrechen“.
    Downloading { done: u64, total: u64 },
    /// Fertig geladen; der Neustart wartet, bis nichts mehr offen ist.
    Waiting,
    /// Die neue Version wird gestartet (die Fußzeile bleibt leer, der Kasten über allem sagt es).
    Restarting,
    /// Fehlermeldung mit „Download-Seite öffnen“ und „Schließen“, bei `retry` auch „Erneut
    /// versuchen“.
    Failed { message: &'a str, retry: bool },
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
    /// „Jetzt aktualisieren“ wurde gewählt: die Meldung zeigt den Fortschritt statt der Knöpfe
    /// und „Später“ gilt nicht mehr.
    pub installing: bool,
    /// Während des Updates mit ✕ oder Esc ausgeblendet: kommt nicht von allein wieder.
    pub install_hidden: bool,
}

impl UpdateDialog {
    /// Zeigt die Meldung für `update`.
    pub fn open_with(&mut self, update: Update) {
        self.update = Some(update);
        self.open = true;
        self.install_hidden = false;
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

    /// Darf die Meldung zum Zeitpunkt `now` von allein erscheinen? Nicht, wenn sie schon offen
    /// ist; während eines Updates, solange sie nicht ausgeblendet ist (Fortschritt, Fehler),
    /// sonst nicht nach „Später“.
    pub fn may_auto_open(&self, now: u64) -> bool {
        !self.open
            && if self.installing {
                !self.install_hidden
            } else {
                !self.snoozed(now)
            }
    }

    /// Was ✕ und Esc bedeuten: sonst „Später“, während des Updates ausblenden, nach einem
    /// Fehler (`failed`) schließen.
    pub fn close_action(&self, failed: bool) -> UpdateAction {
        match (self.installing, failed) {
            (false, _) => UpdateAction::Later,
            (true, false) => UpdateAction::Hide,
            (true, true) => UpdateAction::CloseFailed,
        }
    }

    /// Führt `action` aus, soweit sie die Meldung betrifft (Download, Abbruch und Browser
    /// übernimmt die App): „Später“ merkt `now` + [`SNOOZE_SECS`], „Überspringen“ speichert die
    /// Version, „Jetzt aktualisieren“ setzt `installing` (die Meldung bleibt für den Fortschritt
    /// offen). Abbrechen und Schließen nach einem Fehler beenden das Update; die Meldung kommt
    /// dann wie nach „Später“ frühestens nach [`SNOOZE_SECS`] von allein wieder.
    pub fn apply(&mut self, ws: &mut Workspace, action: UpdateAction, now: u64) {
        match action {
            UpdateAction::Later => {
                self.open = false;
                self.snooze_until = Some(now.saturating_add(SNOOZE_SECS));
            }
            UpdateAction::Skip => {
                self.open = false;
                if let Some(update) = self.update.take() {
                    // Misslingt das Speichern, gilt das Überspringen bis zum Programmende.
                    let _ = ws.set_skipped_version(Some(update.latest.version));
                }
            }
            UpdateAction::InstallNow | UpdateAction::Retry => {
                self.installing = true;
                self.install_hidden = false;
            }
            UpdateAction::CancelInstall | UpdateAction::CloseFailed => {
                self.open = false;
                self.installing = false;
                self.install_hidden = false;
                self.snooze_until = Some(now.saturating_add(SNOOZE_SECS));
            }
            UpdateAction::Hide => {
                self.open = false;
                self.install_hidden = true;
            }
            UpdateAction::OpenDownloadPage => {}
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

/// Höhe der Patchnotes: was ein Bereich der Höhe `height` hergibt, höchstens [`NOTES_MAX_H`].
fn notes_height(height: f32) -> f32 {
    (height - NOTES_RESERVE_H).clamp(120.0, NOTES_MAX_H)
}

/// Bereich der Meldung: rechts im Fenster, unten bis über die Verlaufsleiste, oben bis unter die
/// Menüleiste (je mit Rand). Die Meldung steht unten rechts darin.
pub fn notice_area(content: egui::Rect) -> egui::Rect {
    use crate::shell::{BOTTOM_H, GAP, HISTORY_H, SHELL_INSET, TOP_H};
    egui::Rect::from_min_max(
        egui::pos2(
            content.left() + SHELL_INSET,
            content.top() + SHELL_INSET + TOP_H + GAP,
        ),
        egui::pos2(
            content.right() - SHELL_INSET,
            content.bottom() - (SHELL_INSET + BOTTOM_H + GAP + HISTORY_H + GAP),
        ),
    )
}

/// Zeichnet die Meldung, solange `state.open` gilt, und liefert die Wahl (✕ wie
/// [`UpdateDialog::close_action`]). Ausgeführt wird sie hier schon, soweit sie die Meldung
/// betrifft ([`UpdateDialog::apply`]). Nach „Jetzt aktualisieren“ stehen statt der Knöpfe
/// Fortschritt oder Fehler (`install`). Ohne `installable` (nicht installiert) bietet die
/// Fußzeile nur „Download-Seite öffnen“ (Hauptknopf), „Später“ und „Überspringen“.
pub fn show(
    ctx: &egui::Context,
    ws: &mut Workspace,
    state: &mut UpdateDialog,
    install: InstallView<'_>,
    installable: bool,
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
    let failure = match install {
        InstallView::Failed { message, .. } if state.installing => Some(message),
        _ => None,
    };
    let heading = match failure {
        Some(_) => FAILED_TITLE.to_string(),
        None => title(&update.latest.version),
    };
    let area = notice_area(ctx.content_rect());
    Dialog::new(heading, DialogSize::Normal)
        .id(egui::Id::new(WINDOW_ID))
        .bottom_right_in(area)
        .open(&mut open)
        .show_with_footer(
            ctx,
            |ui| {
                if let Some(message) = failure {
                    failure_box(ui, message, dark);
                    ui.add_space(theme::SPACE_3);
                }
                ui.label(running_line(&running_version()));
                ui.add_space(theme::SPACE_2);
                // Mit Fehlermeldung bleibt für die Patchnotes etwas weniger Höhe.
                let reserve = if failure.is_some() { 80.0 } else { 0.0 };
                theme::scroll_area(
                    ui,
                    egui::ScrollArea::vertical()
                        .id_salt("update_patchnotes")
                        .auto_shrink([false, true])
                        .max_height(notes_height(area.height() - reserve)),
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
                if state.installing {
                    action = install_footer(ui, install, dark);
                    return;
                }
                if !installable {
                    ui.add(egui::Label::new(update_install::NOT_INSTALLED_HINT).wrap());
                    ui.add_space(theme::SPACE_2);
                }
                ui.horizontal(|ui| {
                    let main = if installable {
                        (INSTALL_NOW, UpdateAction::InstallNow)
                    } else {
                        (
                            update_install::OPEN_DOWNLOAD_PAGE,
                            UpdateAction::OpenDownloadPage,
                        )
                    };
                    if ui.add(theme::primary_button(main.0, dark)).clicked() {
                        action = Some(main.1);
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
        action = Some(state.close_action(matches!(install, InstallView::Failed { .. })));
    }
    if let Some(a) = action {
        state.apply(ws, a, crate::update_check::now_unix());
    }
    action
}

/// Fußzeile während des Updates: Fortschritt mit „Abbrechen“, Neustart oder Fehler mit
/// „Download-Seite öffnen“ und „Schließen“.
fn install_footer(ui: &mut egui::Ui, install: InstallView<'_>, dark: bool) -> Option<UpdateAction> {
    let mut action = None;
    match install {
        InstallView::Preparing => {
            ui.label(PREPARING);
        }
        // Der Kasten über allem sagt es schon ([`update_install::RESTART_BOX`]).
        InstallView::Restarting => {}
        InstallView::Waiting => {
            ui.label(update_install::WAITING);
        }
        InstallView::Downloading { done, total } => {
            ui.horizontal(|ui| {
                ui.add(
                    egui::ProgressBar::new(update_install::fraction(done, total))
                        .desired_width(PROGRESS_W)
                        .text(update_install::download_text(done, total)),
                );
                if ui.button(CANCEL).clicked() {
                    action = Some(UpdateAction::CancelInstall);
                }
            });
        }
        // Die Meldung selbst steht oben im Inhalt ([`failure_box`]).
        InstallView::Failed { retry, .. } => {
            ui.horizontal(|ui| {
                if retry && ui.add(theme::primary_button(RETRY, dark)).clicked() {
                    action = Some(UpdateAction::Retry);
                }
                let page = if retry {
                    ui.button(update_install::OPEN_DOWNLOAD_PAGE)
                } else {
                    ui.add(theme::primary_button(
                        update_install::OPEN_DOWNLOAD_PAGE,
                        dark,
                    ))
                };
                if page.clicked() {
                    action = Some(UpdateAction::OpenDownloadPage);
                }
                if ui.button(CLOSE).clicked() {
                    action = Some(UpdateAction::CloseFailed);
                }
            });
        }
    }
    action
}

/// Fehlermeldung oben in der Meldung: Warnsymbol und Text in Warnfarbe, in einem Kasten mit
/// Rand in Warnfarbe, damit sie nicht zu übersehen ist. Auch im Fenster „Rückmeldung“.
pub(crate) fn failure_box(ui: &mut egui::Ui, message: &str, dark: bool) {
    let warning = theme::palette(dark).warning;
    egui::Frame::new()
        .stroke(egui::Stroke::new(1.0, warning))
        .corner_radius(4.0)
        .inner_margin(theme::SPACE_2)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_top(|ui| {
                ui.label(icons::text(Icon::Warning, WARNING_ICON_SIZE, warning));
                ui.add(egui::Label::new(egui::RichText::new(message).color(warning)).wrap());
            });
        });
}

/// Satz im Hinweis nach einem Update.
pub const AFTER_UPDATE_FEEDBACK: &str = "Etwas stimmt nicht?";
/// Knopf im Hinweis nach einem Update: öffnet das Fenster „Rückmeldung“.
pub const FEEDBACK_BUTTON: &str = "Rückmeldung geben";

/// Was der Hinweis nach einem Update dem Programm aufträgt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AfterUpdateAction {
    /// Das Fenster „Rückmeldung“ öffnen (der Hinweis ist dann schon zu).
    Feedback,
}

/// Hinweis „Aktualisiert auf Version X“ nach einem Update, einmal: mit den Patchnotes dieser
/// Version, sonst nur dem Verweis auf GitHub, dazu der Knopf „Rückmeldung geben“ (die Version
/// steht im Titel). „OK“, ✕ und Esc schließen ihn (`after` wird leer); „Rückmeldung geben“
/// liefert [`AfterUpdateAction::Feedback`] und lässt ihn offen: der Aufrufer schließt ihn erst,
/// wenn sich das Fenster „Rückmeldung“ wirklich geöffnet hat.
pub fn show_after_update(
    ctx: &egui::Context,
    after: &mut Option<AfterUpdate>,
) -> Option<AfterUpdateAction> {
    let info = after.as_ref()?;
    let dark = ctx.global_style().visuals.dark_mode;
    let mut open = true;
    let mut ok = false;
    let mut feedback = false;
    Dialog::new(
        update_install::updated_title(&info.version),
        DialogSize::Wide,
    )
    .id(egui::Id::new(DONE_WINDOW_ID))
    .center_on_open()
    .open(&mut open)
    .show_with_footer(
        ctx,
        |ui| {
            let blocks = info.notes.as_deref().map(parse_notes).unwrap_or_default();
            if blocks.is_empty() {
                ui.label(NOTES_ON_GITHUB);
            } else {
                theme::scroll_area(
                    ui,
                    egui::ScrollArea::vertical()
                        .id_salt("update_done_notes")
                        .auto_shrink([false, true])
                        .max_height(notes_height(ctx.content_rect().height())),
                    |ui| {
                        for block in &blocks {
                            note_block(ui, block, dark);
                        }
                    },
                );
            }
            ui.add_space(theme::SPACE_3);
            ui.label(
                egui::RichText::new(AFTER_UPDATE_FEEDBACK)
                    .color(theme::palette(dark).text_secondary),
            );
        },
        |ui| {
            ui.horizontal(|ui| {
                if ui.add(theme::primary_button(OK, dark)).clicked() {
                    ok = true;
                }
                if ui.button(FEEDBACK_BUTTON).clicked() {
                    feedback = true;
                }
            });
        },
    );
    let esc = !crate::viewport::typing(ctx)
        && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
    if ok || !open || esc {
        *after = None;
    }
    feedback.then_some(AfterUpdateAction::Feedback)
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
