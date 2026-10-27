//! Fenster „Einstellungen“ (Spec §5): Abschnitte untereinander mit Überschrift, damit später
//! weitere dazukommen. Oben „Darstellung“ mit dem Regler „Durchsichtigkeit der Panels“ (wirkt
//! sofort, wird gespeichert wie das Projektverzeichnis), darunter „Updates“ (laufende Version,
//! Update-Kanal, Suchen beim Start, letzte Prüfung; gespeichert sofort; Knopf „Jetzt nach Updates
//! suchen“ mit Ergebnis und bei einem Fund „Meldung anzeigen“, die Suche selbst macht die App;
//! eine übersprungene Version mit „Doch wieder anzeigen“), dann „Projekte“ mit dem
//! Projektverzeichnis und zuletzt „Tastenkürzel“ ([`crate::keymap::settings_ui`]; zuletzt, weil
//! die lange Tabelle sonst „Projekte“ nach unten schöbe). Gewählt wird über die
//! Dateiauswahl im Ordner-Modus; der Wechsel wirkt wie [`Workspace::choose_root`] und
//! [`Workspace::persist_config`] (nur `root` wird geschrieben, Favoriten bleiben). Nichts wird
//! verschoben, kopiert oder gelöscht.

use crate::dialog::{CLOSE_LABEL, Dialog, DialogSize};
use crate::file_chooser::{self, ChooserAction, FileChooser, display_path};
use crate::keymap::settings_ui::{self, CaptureState};
use crate::places::{self, Places};
use crate::theme;
use crate::workspace::{ROOT_LOCKED, Workspace};
use crate::{local_time, running_version};
use planora_project::config::{Config, MAX_PANEL_TRANSPARENCY, clamp_panel_transparency};
use planora_project::library::{DOCUMENT_FILE, DOCUMENT_SUFFIX, PROJECT_FILE};
use planora_update::{Channel, Version};
use std::cell::RefCell;
use std::path::{Path, PathBuf};

pub const TITLE: &str = "Einstellungen";
pub const SECTION_APPEARANCE: &str = "Darstellung";
pub const TRANSPARENCY_LABEL: &str = "Durchsichtigkeit der Panels";
/// Warnung, wenn die Durchsichtigkeit gilt, aber nicht gespeichert werden konnte.
pub const TRANSPARENCY_NOT_SAVED: &str = "Die Durchsichtigkeit gilt nur bis zum Programmende: die Einstellung konnte nicht gespeichert werden.";
/// Breite der Schieberspur des Reglers.
pub const SLIDER_WIDTH: f32 = 200.0;
pub const SECTION_UPDATES: &str = "Updates";
pub const CHANNEL_LABEL: &str = "Update-Kanal";
/// Kanäle in der Auswahl „Update-Kanal“ (heute nur Dev; später kommen Beta und Stabil dazu).
pub const AVAILABLE_CHANNELS: &[Channel] = &[Channel::Dev];
pub const CHECK_ON_START_LABEL: &str = "Beim Start nach Updates suchen";
/// Hinweis zum Datenschutz unter dem Schalter.
pub const PRIVACY_NOTE: &str =
    "Beim Suchen fragt das Programm bei GitHub nach; dabei sieht GitHub deine IP-Adresse.";
pub const CHECK_NOW: &str = "Jetzt nach Updates suchen";
/// Neben dem Knopf [`CHECK_NOW`], solange die Suche läuft (der Knopf ist dann aus).
pub const CHECK_RUNNING: &str = "Suche läuft …";
/// Ergebnis der Suche über den Knopf: keine neuere Version.
pub const UP_TO_DATE: &str = "Du hast die neueste Version.";
/// Ergebnis der Suche über den Knopf: Abruf oder Antwort gescheitert.
pub const CHECK_FAILED: &str = "Die Suche hat nicht geklappt (kein Netz?).";
/// Ergebnis der Suche über den Knopf auf einem System ohne Aktualisierung über das Programm
/// (alles außer Windows).
pub const NOT_ON_THIS_SYSTEM: &str =
    "Auf diesem System gibt es keine Aktualisierung über das Programm.";
/// Knopf neben „Neue Version gefunden“: öffnet die Meldung mit Jetzt, Später, Überspringen.
pub const SHOW_NOTICE: &str = "Meldung anzeigen";
/// Knopf neben der übersprungenen Version: hebt das Überspringen auf und sucht gleich nach.
pub const UNSKIP: &str = "Doch wieder anzeigen";
/// Statt eines Zeitpunkts, wenn noch nie geprüft wurde.
pub const NEVER_CHECKED: &str = "noch nie";
/// Warnung, wenn eine Update-Einstellung gilt, aber nicht gespeichert werden konnte.
pub const UPDATES_NOT_SAVED: &str =
    "Die Update-Einstellung gilt nur bis zum Programmende: sie konnte nicht gespeichert werden.";
pub const SECTION_HELP: &str = "Hilfe";
pub const VERBOSE_LABEL: &str = "Ausführliches Protokoll";
/// Hinweis unter dem Schalter [`VERBOSE_LABEL`].
pub const VERBOSE_NOTE: &str = "Schreibt mehr in die Protokolldatei auf deinem Rechner, zum Beispiel jeden Befehl und wie lange er dauert. Gesendet wird davon nichts.";
/// Warnung, wenn der Schalter gilt, aber nicht gespeichert werden konnte.
pub const VERBOSE_NOT_SAVED: &str = "Das ausführliche Protokoll gilt nur bis zum Programmende: die Einstellung konnte nicht gespeichert werden.";
pub const SECTION_PROJECTS: &str = "Projekte";
pub const DIRECTORY_LABEL: &str = "Projektverzeichnis";
pub const CHANGE_DIRECTORY: &str = "Projektverzeichnis ändern …";
pub const CHOOSE_HEADING: &str = "Projektverzeichnis wählen";
pub const KEEP_HINT: &str = "Deine Projekte bleiben, wo sie sind. Im neuen Verzeichnis erscheinen nur die Projekte, die dort liegen.";
/// Anfang der Meldung, wenn sich das gewählte Verzeichnis nicht lesen lässt (danach der Grund).
pub const UNREADABLE: &str = "Das Verzeichnis lässt sich nicht lesen.";
/// Meldung, wenn das gewählte Verzeichnis selbst ein Projekt oder Dokument ist.
pub const NESTED: &str = "Das ist selbst schon ein Projekt oder Dokument. Bitte den Ordner wählen, der deine Projekte enthält.";
/// Warnung, wenn der Wechsel gilt, die Einstellung aber nicht gespeichert werden konnte.
pub const NOT_SAVED: &str = "Das neue Projektverzeichnis gilt nur bis zum Programmende: die Einstellung konnte nicht gespeichert werden.";

/// Kennung des Fensters (unabhängig vom Titel).
const WINDOW_ID: &str = "planora_settings_window";

/// „Laufende Version: 0.2.0-dev.1 (Dev)“ für die laufende Version.
pub fn running_version_line() -> String {
    running_version_line_for(&running_version())
}

/// „Laufende Version: <Version> (<Kanal>)“.
pub fn running_version_line_for(v: &Version) -> String {
    format!("Laufende Version: {v} ({})", v.channel().label())
}

/// „Letzte Prüfung: 25.10.2026, 18:42“ (Ortszeit) oder „Letzte Prüfung: noch nie“.
pub fn last_check_line(unix_seconds: Option<u64>) -> String {
    let when = unix_seconds.map_or_else(
        || NEVER_CHECKED.to_string(),
        |t| local_time::unix_time_text(i64::try_from(t).unwrap_or(i64::MAX)),
    );
    format!("Letzte Prüfung: {when}")
}

/// „Neue Version gefunden: 0.2.0-dev.2“ (Ergebnis der Suche über den Knopf).
pub fn update_found_line(v: &Version) -> String {
    format!("Neue Version gefunden: {v}")
}

/// „Übersprungen: Version 0.2.0-dev.2“ (Einstellungen → Updates).
pub fn skipped_line(v: &Version) -> String {
    format!("Übersprungen: Version {v}")
}

/// Zustand der Suche über den Knopf [`CHECK_NOW`], neben dem Knopf angezeigt. Die App setzt ihn
/// ([`SettingsWindow::set_check_hint`]); eine Suche beim Start zeigt hier nichts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CheckHint {
    /// Die Suche läuft ([`CHECK_RUNNING`]).
    Running,
    /// [`UP_TO_DATE`].
    UpToDate,
    /// Neue Version gefunden ([`update_found_line`]).
    Found(Version),
    /// [`CHECK_FAILED`] (Warnfarbe).
    Failed,
    /// [`NOT_ON_THIS_SYSTEM`].
    Unsupported,
}

impl CheckHint {
    pub fn text(&self) -> String {
        match self {
            CheckHint::Running => CHECK_RUNNING.into(),
            CheckHint::UpToDate => UP_TO_DATE.into(),
            CheckHint::Found(v) => update_found_line(v),
            CheckHint::Failed => CHECK_FAILED.into(),
            CheckHint::Unsupported => NOT_ON_THIS_SYSTEM.into(),
        }
    }
}

/// Meldung nach einem Wechsel: Warnung (Warnfarbe) oder Fehler (Fehlerfarbe).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RootMessage {
    Warning(String),
    Error(String),
}

impl RootMessage {
    pub fn text(&self) -> &str {
        match self {
            RootMessage::Warning(t) | RootMessage::Error(t) => t,
        }
    }

    pub fn color(&self, dark: bool) -> egui::Color32 {
        match self {
            RootMessage::Warning(_) => theme::palette(dark).warning,
            RootMessage::Error(_) => theme::palette(dark).error,
        }
    }
}

/// Angezeigter Pfad: gewählte Bibliothek (ohne Windows-Präfix für lange Pfade), sonst
/// `config.root`, sonst [`Config::default_root`].
pub fn current_directory(ws: &Workspace) -> PathBuf {
    match (&ws.library, &ws.config.root) {
        (Some(lib), _) => PathBuf::from(display_path(lib.root().path())),
        (None, Some(root)) => root.clone(),
        (None, None) => Config::default_root(),
    }
}

/// Ist `dir` der Ordner der geöffneten Bibliothek (auch anders geschrieben)?
fn is_open_library(ws: &Workspace, dir: &Path) -> bool {
    let Some(lib) = &ws.library else {
        return false;
    };
    let root = lib.root().path();
    places::same_dir(dir, &current_directory(ws))
        || std::fs::canonicalize(dir).is_ok_and(|c| places::same_dir(&c, root))
}

/// Prüft ein vorhandenes Ziel vor dem Wechsel: lesbar und nicht selbst ein Projekt- oder
/// Dokumentordner. Ein fehlendes Ziel legt `choose_root` an.
fn check_target(dir: &Path) -> Result<(), RootMessage> {
    if !dir.exists() {
        return Ok(());
    }
    if let Err(e) = std::fs::read_dir(dir) {
        return Err(RootMessage::Error(format!(
            "{UNREADABLE} {}",
            file_chooser::io_message(e.kind())
        )));
    }
    let document_name = dir.file_name().is_some_and(|n| {
        n.to_string_lossy()
            .to_lowercase()
            .ends_with(DOCUMENT_SUFFIX)
    });
    if document_name || dir.join(PROJECT_FILE).exists() || dir.join(DOCUMENT_FILE).exists() {
        return Err(RootMessage::Error(NESTED.into()));
    }
    Ok(())
}

/// Wechselt das Projektverzeichnis wie `Workspace::choose_root` + `persist_config`.
/// Gleicher Ordner (nach `places::same_dir`) → Ok ohne Schreiben. Offene gespeicherte Dokumente
/// → Warning mit dem Wortlaut aus `choose_root`; ein nicht lesbares Ziel, ein Projekt- oder
/// Dokumentordner und `Library::open`-Fehler → Error (altes Verzeichnis und Datei bleiben).
/// Lässt sich die Einstellung nicht speichern, gilt der Wechsel bis Programmende (Warning
/// [`NOT_SAVED`]).
pub fn change_root(ws: &mut Workspace, dir: &Path) -> Result<(), RootMessage> {
    if is_open_library(ws, dir) {
        return Ok(());
    }
    if ws.has_saved_documents() {
        return Err(RootMessage::Warning(ROOT_LOCKED.into()));
    }
    check_target(dir)?;
    ws.choose_root(dir).map_err(RootMessage::Error)?;
    if !ws.try_persist_config() {
        return Err(RootMessage::Warning(NOT_SAVED.into()));
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettingsEvent {
    None,
    Closed { return_to_projects: bool },
}

#[derive(Default)]
pub struct SettingsWindow {
    open: bool,
    /// Beim Schließen wieder das Fenster „Projekte“ öffnen.
    return_to_projects: bool,
    /// Dateiauswahl im Ordner-Modus, solange gewählt wird.
    chooser: Option<FileChooser>,
    message: Option<RootMessage>,
    /// Hinweis unter dem Regler (nur [`TRANSPARENCY_NOT_SAVED`]).
    appearance_message: Option<RootMessage>,
    /// Hinweis im Abschnitt „Updates“ (nur [`UPDATES_NOT_SAVED`]).
    updates_message: Option<RootMessage>,
    /// Hinweis im Abschnitt „Hilfe“ (nur [`VERBOSE_NOT_SAVED`]).
    help_message: Option<RootMessage>,
    /// Zustand der Suche über den Knopf [`CHECK_NOW`].
    check_hint: Option<CheckHint>,
    /// Das Ergebnis in `check_hint` war schon einmal im offenen Fenster zu sehen (erst dann
    /// leert es ein erneutes Öffnen).
    check_hint_shown: bool,
    /// Der Knopf [`CHECK_NOW`] wurde gedrückt; die App holt das ab
    /// ([`SettingsWindow::take_check_request`]).
    check_requested: bool,
    /// Der Knopf [`SHOW_NOTICE`] wurde gedrückt; die App holt das ab
    /// ([`SettingsWindow::take_notice_request`]) und öffnet die Meldung.
    notice_requested: bool,
    /// Der Regler wurde bewegt, der Wert ist aber noch nicht gespeichert (gespeichert wird beim
    /// Loslassen, nicht in jedem Bild des Ziehens).
    transparency_unsaved: bool,
    /// Abschnitt „Tastenkürzel“: Suche, Erfassen, Doppelbelegung, Hinweise.
    keys: CaptureState,
    /// Rechteck des Fensters im letzten Bild (Layouttests).
    pub last_rect: Option<egui::Rect>,
}

impl SettingsWindow {
    /// Öffnet das Fenster; leert Meldung und Auswahl.
    pub fn open(&mut self, return_to_projects: bool) {
        self.open = true;
        self.return_to_projects = return_to_projects;
        self.chooser = None;
        self.message = None;
        self.appearance_message = None;
        self.updates_message = None;
        self.help_message = None;
        // Eine noch laufende Suche bleibt sichtbar, ebenso ein Ergebnis, das noch nie zu sehen war
        // (das Fenster ging zu, während die Suche lief); ein schon gezeigtes Ergebnis nicht.
        if self.check_hint_shown && self.check_hint != Some(CheckHint::Running) {
            self.check_hint = None;
        }
        self.keys = CaptureState::default();
    }

    /// Schließt das Fenster; ein laufendes Erfassen einer Taste endet (die Sperre der Kürzel
    /// fällt im nächsten Bild, siehe [`settings_ui::is_capturing`]).
    pub fn close(&mut self) {
        self.open = false;
        self.chooser = None;
        self.keys.stop();
    }

    /// Zustand des Abschnitts „Tastenkürzel“ (Suchtext, Erfassen; für Tests).
    #[doc(hidden)]
    pub fn keys_mut(&mut self) -> &mut CaptureState {
        &mut self.keys
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    /// Öffnet die Auswahl im Ordner-Modus im aktuellen Verzeichnis, fehlt es, in dessen
    /// Elternordner, sonst in [`file_chooser::default_dir`].
    pub fn start_choosing(&mut self, ws: &Workspace) {
        let current = current_directory(ws);
        let dir = if current.is_dir() {
            current
        } else {
            match current.parent().filter(|p| p.is_dir()) {
                Some(parent) => parent.to_path_buf(),
                None => file_chooser::default_dir(),
            }
        };
        self.chooser = Some(FileChooser::folder(dir));
    }

    pub fn chooser_mut(&mut self) -> Option<&mut FileChooser> {
        self.chooser.as_mut()
    }

    pub fn message(&self) -> Option<&RootMessage> {
        self.message.as_ref()
    }

    /// Hinweis des Abschnitts „Darstellung“ (Speichern fehlgeschlagen).
    pub fn appearance_message(&self) -> Option<&RootMessage> {
        self.appearance_message.as_ref()
    }

    /// Hinweis des Abschnitts „Updates“ (Speichern fehlgeschlagen).
    pub fn updates_message(&self) -> Option<&RootMessage> {
        self.updates_message.as_ref()
    }

    /// Hinweis des Abschnitts „Hilfe“ (Speichern fehlgeschlagen).
    pub fn help_message(&self) -> Option<&RootMessage> {
        self.help_message.as_ref()
    }

    /// Zustand der Suche über den Knopf [`CHECK_NOW`].
    pub fn check_hint(&self) -> Option<&CheckHint> {
        self.check_hint.as_ref()
    }

    /// Setzt den Zustand der Suche (die App, wenn sie startet und wenn das Ergebnis da ist).
    pub fn set_check_hint(&mut self, hint: Option<CheckHint>) {
        self.check_hint = hint;
        self.check_hint_shown = false;
    }

    /// Wurde der Knopf [`CHECK_NOW`] seit dem letzten Abholen gedrückt? Setzt zurück.
    pub fn take_check_request(&mut self) -> bool {
        std::mem::take(&mut self.check_requested)
    }

    /// Wurde der Knopf [`SHOW_NOTICE`] seit dem letzten Abholen gedrückt? Setzt zurück.
    pub fn take_notice_request(&mut self) -> bool {
        std::mem::take(&mut self.notice_requested)
    }

    /// Hinweis nach dem Speichern einer Update-Einstellung: [`UPDATES_NOT_SAVED`], wenn es
    /// misslang (der Wert gilt dann für die Sitzung), sonst keiner.
    fn note_updates_saved(&mut self, written: bool) {
        self.updates_message = (!written).then(|| RootMessage::Warning(UPDATES_NOT_SAVED.into()));
    }

    /// Was der Regler beim Ziehen tut: der Wert gilt sofort, ist aber noch nicht gespeichert. Er
    /// wird im Arbeitsbereich vorgemerkt (`panel_transparency_pending`), damit er auch dann in
    /// die Datei kommt, wenn das Fenster von außen ([`SettingsWindow::close`]) zugeht: beim
    /// nächsten gelungenen Schreiben der Konfiguration.
    pub fn preview_transparency(&mut self, ws: &mut Workspace, percent: u8) {
        ws.config.panel_transparency = clamp_panel_transparency(percent);
        ws.panel_transparency_pending = true;
        self.transparency_unsaved = true;
    }

    /// Speichert die Durchsichtigkeit (sie gilt schon); misslingt das, bleibt sie für die
    /// Sitzung und der Hinweis [`TRANSPARENCY_NOT_SAVED`] erscheint.
    fn save_transparency(&mut self, ws: &mut Workspace) {
        self.transparency_unsaved = false;
        self.appearance_message = (!ws.set_panel_transparency(ws.panel_transparency()))
            .then(|| RootMessage::Warning(TRANSPARENCY_NOT_SAVED.into()));
    }

    /// Wie in der App: [`SettingsWindow::show`] mit den Schnellzugriffen des Arbeitsbereichs;
    /// geänderte Favoriten werden danach gespeichert. Das Ereignis (auch der Rückweg zu
    /// „Projekte“) geht an den Aufrufer.
    pub fn show_in(&mut self, ctx: &egui::Context, ws: &mut Workspace) -> SettingsEvent {
        let mut places = ws.places();
        let event = self.show(ctx, ws, &mut places);
        ws.save_places(places);
        event
    }

    /// Zeichnet das Fenster (Id `"planora_settings_window"`, `center_on_open`, ✕, Fußzeile
    /// „Schließen“ bzw. beim Wählen die Fußzeile der Auswahl).
    ///
    /// Geänderte Favoriten der Auswahl stehen danach in `places` (der Aufrufer speichert sie mit
    /// [`Workspace::save_places`]).
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        ws: &mut Workspace,
        places: &mut Places,
    ) -> SettingsEvent {
        if !self.open {
            self.last_rect = None;
            settings_ui::set_capturing(ctx, false);
            return SettingsEvent::None;
        }
        let (mut open, mut close, mut start) = (true, false, false);
        // Bei offener Rückfrage (etwa „Dokument gesperrt“, deren Pfad zum aktuellen Verzeichnis
        // gehört) ist der Inhalt gesperrt: kein Wechsel, keine Auswahl, bis sie beantwortet ist.
        // „Schließen“ und ✕ bleiben.
        let locked = ws.prompt.is_some();
        // Erfasst der Abschnitt „Tastenkürzel“ gerade eine Taste, gehört sie ihm: vor allen
        // Knöpfen und Feldern des Fensters.
        settings_ui::capture_input(ctx, ws, &mut self.keys, locked);
        let keys = &mut self.keys;
        let path = display_path(&current_directory(ws));
        let message = self.message.clone();
        let appearance_message = self.appearance_message.clone();
        let updates_message = self.updates_message.clone();
        let help_message = self.help_message.clone();
        let mut verbose = ws.verbose_log();
        let check_hint = self.check_hint.clone();
        self.check_hint_shown = true;
        let mut check_now = false;
        let mut show_notice = false;
        let skipped = ws.skipped_version();
        let mut unskip = false;
        let channel = ws.update_channel();
        let (mut chosen_channel, mut check_on_start) = (channel, ws.update_check_enabled());
        let last_check = last_check_line(ws.last_update_check());
        let mut transparency = ws.panel_transparency();
        let (mut slid, mut released) = (false, false);
        let choosing = self.chooser.is_some();
        // Die Auswahl zeichnet oben (rollt) und in der Fußzeile (wählen, abbrechen).
        let chooser = RefCell::new(self.chooser.as_mut());
        let (mut body_action, mut footer_action) = (ChooserAction::None, ChooserAction::None);
        let response = Dialog::new(TITLE, DialogSize::File)
            .id(egui::Id::new(WINDOW_ID))
            .center_on_open()
            .open(&mut open)
            .show_with_footer(
                ctx,
                |ui| {
                    ui.add_enabled_ui(!locked, |ui| {
                        let dark = ui.visuals().dark_mode;
                        theme::section_header(ui, SECTION_APPEARANCE, theme::SUB_HEADER_H);
                        ui.add_space(theme::SPACE_2);
                        ui.strong(TRANSPARENCY_LABEL);
                        ui.spacing_mut().slider_width = SLIDER_WIDTH;
                        let r = ui.add(
                            egui::Slider::new(&mut transparency, 0..=MAX_PANEL_TRANSPARENCY)
                                .integer()
                                .suffix(" %"),
                        );
                        theme::focus_ring(ui, &r);
                        slid = r.changed();
                        released = !r.is_pointer_button_down_on() && !r.dragged();
                        if let Some(m) = &appearance_message {
                            ui.colored_label(m.color(dark), m.text());
                        }
                        ui.add_space(theme::SPACE_4);
                        show_updates(
                            ui,
                            &mut chosen_channel,
                            &mut check_on_start,
                            &last_check,
                            UpdateButtons {
                                hint: check_hint.as_ref(),
                                check_now: &mut check_now,
                                show_notice: &mut show_notice,
                                skipped,
                                unskip: &mut unskip,
                            },
                            updates_message.as_ref(),
                        );
                        ui.add_space(theme::SPACE_4);
                        show_help(ui, &mut verbose, help_message.as_ref());
                        ui.add_space(theme::SPACE_4);
                        theme::section_header(ui, SECTION_PROJECTS, theme::SUB_HEADER_H);
                        ui.add_space(theme::SPACE_2);
                        ui.strong(DIRECTORY_LABEL);
                        ui.add(egui::Label::new(path).wrap());
                        if !choosing && ui.button(CHANGE_DIRECTORY).clicked() {
                            start = true;
                        }
                        ui.label(KEEP_HINT);
                        if let Some(m) = &message {
                            ui.colored_label(m.color(dark), m.text());
                        }
                        if let Some(c) = chooser.borrow_mut().as_mut() {
                            ui.add_space(theme::SPACE_2);
                            ui.strong(CHOOSE_HEADING);
                            body_action = c.show_body(ui, places);
                        }
                        ui.add_space(theme::SPACE_4);
                        settings_ui::show(ui, ws, keys);
                    });
                },
                |ui| match chooser.borrow_mut().as_mut() {
                    Some(c) => {
                        footer_action = ui.add_enabled_ui(!locked, |ui| c.show_footer(ui)).inner
                    }
                    None => {
                        if ui.button(CLOSE_LABEL).clicked() {
                            close = true;
                        }
                    }
                },
            );
        self.last_rect = response.map(|r| r.response.rect);
        // Wirkt sofort; gespeichert wird, sobald der Regler losgelassen ist (Tastatur sofort).
        if slid && !locked {
            self.preview_transparency(ws, transparency);
        }
        if self.transparency_unsaved && released {
            self.save_transparency(ws);
        }
        if check_now && !locked {
            self.check_requested = true;
        }
        if show_notice && !locked {
            self.notice_requested = true;
        }
        if unskip && !locked {
            // Wieder anbieten: das Überspringen aufheben und gleich nachsehen.
            let written = ws.set_skipped_version(None);
            self.note_updates_saved(written);
            self.check_requested = true;
        }
        // Update-Einstellungen gelten und werden sofort gespeichert.
        if !locked && chosen_channel != channel {
            let written = ws.set_update_channel(chosen_channel);
            self.note_updates_saved(written);
        }
        if !locked && check_on_start != ws.update_check_enabled() {
            let written = ws.set_update_check(check_on_start);
            self.note_updates_saved(written);
        }
        if !locked && verbose != ws.verbose_log() {
            let written = ws.set_verbose_log(verbose);
            self.help_message = (!written).then(|| RootMessage::Warning(VERBOSE_NOT_SAVED.into()));
        }
        for action in [body_action, footer_action] {
            let target = match action {
                ChooserAction::Chosen(p) => Some(p),
                ChooserAction::Cancel => None,
                ChooserAction::None => continue,
            };
            // Erfolg wie Fehler schließen die Auswahl; ein Fehler bleibt als Meldung stehen.
            self.chooser = None;
            if let Some(dir) = target {
                self.message = change_root(ws, &dir).err();
            }
        }
        if start {
            self.start_choosing(ws);
        }
        if !open || close {
            // Ein noch nicht gespeicherter Reglerwert geht beim Schließen nicht verloren.
            if self.transparency_unsaved {
                self.save_transparency(ws);
            }
            let return_to_projects = self.return_to_projects;
            self.close();
            return SettingsEvent::Closed { return_to_projects };
        }
        SettingsEvent::None
    }
}

/// Knöpfe und Anzeigen zur Suche im Abschnitt „Updates“ (gedrückt: das jeweilige Feld `true`).
struct UpdateButtons<'a> {
    /// Zustand der Suche neben [`CHECK_NOW`].
    hint: Option<&'a CheckHint>,
    check_now: &'a mut bool,
    /// [`SHOW_NOTICE`] neben „Neue Version gefunden“.
    show_notice: &'a mut bool,
    /// Übersprungene Version (dann [`skipped_line`] mit [`UNSKIP`]).
    skipped: Option<Version>,
    unskip: &'a mut bool,
}

/// Abschnitt „Updates“: laufende Version und letzte Prüfung, Auswahl des Kanals (nur [`AVAILABLE_CHANNELS`]; ein
/// anderer gespeicherter Kanal steht nur als Anzeige da), Schalter mit Hinweis zum Datenschutz,
/// der Knopf [`CHECK_NOW`] mit dem Zustand der Suche daneben (aus, solange sie läuft; nach einem
/// Fund mit [`SHOW_NOTICE`]) und eine übersprungene Version mit [`UNSKIP`].
fn show_updates(
    ui: &mut egui::Ui,
    channel: &mut Channel,
    check_on_start: &mut bool,
    last_check: &str,
    buttons: UpdateButtons<'_>,
    message: Option<&RootMessage>,
) {
    let UpdateButtons {
        hint,
        check_now,
        show_notice,
        skipped,
        unskip,
    } = buttons;
    let dark = ui.visuals().dark_mode;
    // Knapp gehalten (je Zeile zwei Angaben), damit das Fenster bei 1920 × 1080 nicht bis über
    // die Menüleiste wächst.
    theme::section_header(ui, SECTION_UPDATES, theme::SUB_HEADER_H);
    ui.add_space(theme::SPACE_2);
    ui.horizontal(|ui| {
        ui.label(running_version_line());
        ui.add_space(theme::SPACE_6);
        ui.label(last_check);
    });
    ui.add_space(theme::SPACE_1);
    ui.horizontal(|ui| {
        ui.strong(CHANNEL_LABEL);
        egui::ComboBox::from_id_salt("planora_settings_update_channel")
            .selected_text(channel.label())
            .show_ui(ui, |ui| {
                for c in AVAILABLE_CHANNELS {
                    ui.selectable_value(channel, *c, c.label());
                }
            });
        ui.add_space(theme::SPACE_6);
        let r = ui.checkbox(check_on_start, CHECK_ON_START_LABEL);
        theme::focus_ring(ui, &r);
    });
    ui.add(egui::Label::new(egui::RichText::new(PRIVACY_NOTE).weak()).wrap());
    ui.add_space(theme::SPACE_1);
    // Das Ergebnis steht neben dem Knopf, damit der Abschnitt nicht höher wird.
    ui.horizontal(|ui| {
        let running = hint == Some(&CheckHint::Running);
        let r = ui.add_enabled(!running, egui::Button::new(CHECK_NOW));
        theme::focus_ring(ui, &r);
        *check_now = r.clicked();
        match hint {
            Some(h @ CheckHint::Failed) => {
                ui.colored_label(theme::palette(dark).warning, h.text());
            }
            Some(h @ CheckHint::Found(_)) => {
                ui.label(h.text());
                let r = ui.button(SHOW_NOTICE);
                theme::focus_ring(ui, &r);
                *show_notice = r.clicked();
            }
            Some(h) => {
                ui.label(h.text());
            }
            None => {}
        }
    });
    if let Some(v) = skipped {
        ui.add_space(theme::SPACE_1);
        ui.horizontal(|ui| {
            ui.label(skipped_line(&v));
            let r = ui.button(UNSKIP);
            theme::focus_ring(ui, &r);
            *unskip = r.clicked();
        });
    }
    if let Some(m) = message {
        ui.colored_label(m.color(dark), m.text());
    }
}

/// Abschnitt „Hilfe“: Schalter [`VERBOSE_LABEL`], darunter der Hinweis [`VERBOSE_NOTE`] und bei
/// einem Schreibfehler [`VERBOSE_NOT_SAVED`].
fn show_help(ui: &mut egui::Ui, verbose: &mut bool, message: Option<&RootMessage>) {
    let dark = ui.visuals().dark_mode;
    theme::section_header(ui, SECTION_HELP, theme::SUB_HEADER_H);
    ui.add_space(theme::SPACE_2);
    let r = ui.checkbox(verbose, VERBOSE_LABEL);
    theme::focus_ring(ui, &r);
    ui.add(egui::Label::new(egui::RichText::new(VERBOSE_NOTE).weak()).wrap());
    if let Some(m) = message {
        ui.colored_label(m.color(dark), m.text());
    }
}
