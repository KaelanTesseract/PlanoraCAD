//! Fenster „Einstellungen“ (Spec §5): Abschnitte untereinander mit Überschrift, damit später
//! weitere dazukommen. Oben „Darstellung“ mit dem Regler „Durchsichtigkeit der Panels“ (wirkt
//! sofort, wird gespeichert wie das Projektverzeichnis), darunter „Projekte“ mit dem
//! Projektverzeichnis. Gewählt wird über die
//! Dateiauswahl im Ordner-Modus; der Wechsel wirkt wie [`Workspace::choose_root`] und
//! [`Workspace::persist_config`] (nur `root` wird geschrieben, Favoriten bleiben). Nichts wird
//! verschoben, kopiert oder gelöscht.

use crate::dialog::{CLOSE_LABEL, Dialog, DialogSize};
use crate::file_chooser::{self, ChooserAction, FileChooser, display_path};
use crate::places::{self, Places};
use crate::theme;
use crate::workspace::{ROOT_LOCKED, Workspace};
use planora_project::config::{Config, MAX_PANEL_TRANSPARENCY};
use planora_project::library::{DOCUMENT_FILE, DOCUMENT_SUFFIX, PROJECT_FILE};
use std::cell::RefCell;
use std::path::{Path, PathBuf};

pub const TITLE: &str = "Einstellungen";
pub const SECTION_APPEARANCE: &str = "Darstellung";
pub const TRANSPARENCY_LABEL: &str = "Durchsichtigkeit der Panels";
/// Warnung, wenn die Durchsichtigkeit gilt, aber nicht gespeichert werden konnte.
pub const TRANSPARENCY_NOT_SAVED: &str = "Die Durchsichtigkeit gilt nur bis zum Programmende: die Einstellung konnte nicht gespeichert werden.";
/// Breite der Schieberspur des Reglers.
pub const SLIDER_WIDTH: f32 = 200.0;
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
    /// Der Regler wurde bewegt, der Wert ist aber noch nicht gespeichert (gespeichert wird beim
    /// Loslassen, nicht in jedem Bild des Ziehens).
    transparency_unsaved: bool,
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
    }

    pub fn close(&mut self) {
        self.open = false;
        self.chooser = None;
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
            return SettingsEvent::None;
        }
        let (mut open, mut close, mut start) = (true, false, false);
        // Bei offener Rückfrage (etwa „Dokument gesperrt“, deren Pfad zum aktuellen Verzeichnis
        // gehört) ist der Inhalt gesperrt: kein Wechsel, keine Auswahl, bis sie beantwortet ist.
        // „Schließen“ und ✕ bleiben.
        let locked = ws.prompt.is_some();
        let path = display_path(&current_directory(ws));
        let message = self.message.clone();
        let appearance_message = self.appearance_message.clone();
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
            ws.config.panel_transparency = transparency;
            self.transparency_unsaved = true;
        }
        if self.transparency_unsaved && released {
            self.save_transparency(ws);
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
