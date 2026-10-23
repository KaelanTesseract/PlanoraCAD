//! Fenster „Einstellungen“ (Spec §5): Abschnitte untereinander mit Überschrift, damit später
//! weitere dazukommen. Jetzt einer: „Projekte“ mit dem Projektverzeichnis. Gewählt wird über die
//! Dateiauswahl im Ordner-Modus; der Wechsel wirkt wie [`Workspace::choose_root`] und
//! [`Workspace::persist_config`] (nur `root` wird geschrieben, Favoriten bleiben). Nichts wird
//! verschoben, kopiert oder gelöscht.

use crate::dialog::{CLOSE_LABEL, Dialog, DialogSize};
use crate::file_chooser::{self, ChooserAction, FileChooser, display_path};
use crate::places::{self, Places};
use crate::theme;
use crate::workspace::{ROOT_LOCKED, Workspace};
use planora_project::config::Config;
use std::cell::RefCell;
use std::path::{Path, PathBuf};

pub const TITLE: &str = "Einstellungen";
pub const SECTION_PROJECTS: &str = "Projekte";
pub const DIRECTORY_LABEL: &str = "Projektverzeichnis";
pub const CHANGE_DIRECTORY: &str = "Projektverzeichnis ändern …";
pub const CHOOSE_HEADING: &str = "Projektverzeichnis wählen";
pub const KEEP_HINT: &str = "Ihre Projekte bleiben, wo sie sind. Im neuen Verzeichnis erscheinen nur die Projekte, die dort liegen.";

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

/// Wechselt das Projektverzeichnis wie `Workspace::choose_root` + `persist_config`.
/// Gleicher Ordner (nach `places::same_dir`) → Ok ohne Schreiben. Offene gespeicherte Dokumente
/// → Warning mit dem Wortlaut aus `choose_root`; `Library::open`-Fehler → Error.
pub fn change_root(ws: &mut Workspace, dir: &Path) -> Result<(), RootMessage> {
    if is_open_library(ws, dir) {
        return Ok(());
    }
    if ws.has_saved_documents() {
        return Err(RootMessage::Warning(ROOT_LOCKED.into()));
    }
    ws.choose_root(dir).map_err(RootMessage::Error)?;
    ws.persist_config();
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
        let path = display_path(&current_directory(ws));
        let message = self.message.clone();
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
                    let dark = ui.visuals().dark_mode;
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
                },
                |ui| match chooser.borrow_mut().as_mut() {
                    Some(c) => footer_action = c.show_footer(ui),
                    None => {
                        if ui.button(CLOSE_LABEL).clicked() {
                            close = true;
                        }
                    }
                },
            );
        self.last_rect = response.map(|r| r.response.rect);
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
            let return_to_projects = self.return_to_projects;
            self.close();
            return SettingsEvent::Closed { return_to_projects };
        }
        SettingsEvent::None
    }
}
