//! Eigene Dateiauswahl im Fenster (Plan P14: ohne neue Abhängigkeit statt des Dialogs des
//! Betriebssystems): Ordner wechseln oder eintippen, Datei wählen oder Namen eingeben, Ersetzen
//! einer vorhandenen Datei erst nach einem zweiten Klick.

use crate::browser::error_color;
use crate::mesh_body::warning_color;
use crate::theme;
use std::path::{Path, PathBuf};

/// Höchstzahl der gezeigten Einträge eines Ordners (große Ordner bleiben bedienbar).
pub const MAX_ENTRIES: usize = 2000;
/// Höhe der Liste in Punkten.
const LIST_HEIGHT: f32 = 220.0;
const REPLACE_HINT: &str = "Diese Datei gibt es schon. Noch einmal klicken, um sie zu ersetzen.";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChooserMode {
    Save,
    Open,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub is_dir: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChooserAction {
    None,
    Chosen(PathBuf),
    Cancel,
}

pub struct FileChooser {
    pub mode: ChooserMode,
    dir: PathBuf,
    /// Eingabefeld des Ordners (Enter wechselt dorthin).
    pub dir_text: String,
    pub file_name: String,
    /// Endungen klein und ohne Punkt; beim Speichern genau eine.
    extensions: Vec<&'static str>,
    entries: Vec<Entry>,
    /// Der Ordner hat mehr als [`MAX_ENTRIES`] Einträge.
    pub truncated: bool,
    pub error: Option<String>,
    /// Ziel, dessen Ersetzen noch bestätigt werden muss.
    confirm: Option<PathBuf>,
}

/// Startordner: „Documents“ im Benutzerordner, sonst der Benutzerordner, sonst der aktuelle
/// Ordner.
pub fn default_dir() -> PathBuf {
    let home = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from);
    if let Some(h) = home {
        let docs = h.join("Documents");
        if docs.is_dir() {
            return docs;
        }
        if h.is_dir() {
            return h;
        }
    }
    std::env::current_dir().unwrap_or_default()
}

/// Einfache Meldung zu einem Lesefehler, ohne Text des Betriebssystems.
fn io_message(kind: std::io::ErrorKind) -> &'static str {
    match kind {
        std::io::ErrorKind::NotFound => "Der Ordner ist nicht vorhanden.",
        std::io::ErrorKind::PermissionDenied => "Auf diesen Ordner darf nicht zugegriffen werden.",
        _ => "Der Ordner lässt sich nicht öffnen.",
    }
}

/// Zeichen, die in Dateinamen nicht erlaubt sind (Windows).
const FORBIDDEN_CHARS: [char; 9] = [':', '<', '>', '"', '|', '?', '*', '/', '\\'];

impl FileChooser {
    /// Auswahl zum Speichern mit vorgeschlagenem Namen; die Endung wird ergänzt.
    pub fn save(dir: PathBuf, file_name: String, extension: &'static str) -> FileChooser {
        let mut c = FileChooser::new(ChooserMode::Save, dir, vec![extension]);
        c.file_name = file_name;
        c
    }

    /// Auswahl zum Öffnen einer vorhandenen Datei mit einer der Endungen.
    pub fn open(dir: PathBuf, extensions: Vec<&'static str>) -> FileChooser {
        FileChooser::new(ChooserMode::Open, dir, extensions)
    }

    fn new(mode: ChooserMode, dir: PathBuf, extensions: Vec<&'static str>) -> FileChooser {
        let mut c = FileChooser {
            mode,
            dir_text: dir.display().to_string(),
            dir,
            file_name: String::new(),
            extensions,
            entries: Vec::new(),
            truncated: false,
            error: None,
            confirm: None,
        };
        c.reload();
        c
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    /// Liest den Ordner neu: Unterordner zuerst, dann passende Dateien, je nach Namen ohne
    /// Groß- und Kleinschreibung sortiert.
    pub fn reload(&mut self) {
        self.entries.clear();
        self.truncated = false;
        self.error = None;
        let read = match std::fs::read_dir(&self.dir) {
            Ok(r) => r,
            Err(e) => {
                self.error = Some(io_message(e.kind()).into());
                return;
            }
        };
        let (mut dirs, mut files) = (Vec::new(), Vec::new());
        for item in read.flatten() {
            let name = item.file_name().to_string_lossy().into_owned();
            if item.path().is_dir() {
                dirs.push(name);
            } else if self.matches(&name) {
                files.push(name);
            }
        }
        dirs.sort_by_cached_key(|s| s.to_lowercase());
        files.sort_by_cached_key(|s| s.to_lowercase());
        let all: Vec<Entry> = dirs
            .into_iter()
            .map(|name| Entry { name, is_dir: true })
            .chain(files.into_iter().map(|name| Entry {
                name,
                is_dir: false,
            }))
            .collect();
        self.truncated = all.len() > MAX_ENTRIES;
        self.entries = all.into_iter().take(MAX_ENTRIES).collect();
    }

    fn matches(&self, name: &str) -> bool {
        let lower = name.to_lowercase();
        self.extensions
            .iter()
            .any(|e| lower.ends_with(&format!(".{e}")))
    }

    fn set_dir(&mut self, dir: PathBuf) {
        let dir = std::path::absolute(&dir).unwrap_or(dir);
        self.dir_text = dir.display().to_string();
        self.dir = dir;
        self.confirm = None;
        self.reload();
    }

    pub fn enter(&mut self, name: &str) {
        let next = self.dir.join(name);
        self.set_dir(next);
    }

    pub fn up(&mut self) {
        if let Some(parent) = self.dir.parent() {
            let parent = parent.to_path_buf();
            self.set_dir(parent);
        }
    }

    /// Wechselt in den eingetippten Ordner; `false` (mit Meldung), wenn es ihn nicht gibt.
    pub fn go_to_text(&mut self) -> bool {
        let p = PathBuf::from(self.dir_text.trim());
        if p.is_dir() {
            self.set_dir(p);
            true
        } else {
            self.error = Some("Diesen Ordner gibt es nicht.".into());
            false
        }
    }

    pub fn select(&mut self, name: &str) {
        self.file_name = name.to_string();
        self.confirm = None;
    }

    /// Ziel: beim Speichern im aktuellen Ordner, Endung ergänzt; beim Öffnen nur eine
    /// vorhandene Datei. `None` bei leerem Namen oder Namen mit Ordnerteilen.
    pub fn target(&self) -> Option<PathBuf> {
        let name = self.file_name.trim().trim_end_matches(['.', ' ']);
        if name.is_empty() || name.contains(FORBIDDEN_CHARS) || name.chars().any(char::is_control) {
            return None;
        }
        match self.mode {
            ChooserMode::Save => {
                let name = if self.matches(name) {
                    name.to_string()
                } else {
                    format!("{name}.{}", self.extensions[0])
                };
                Some(self.dir.join(name))
            }
            ChooserMode::Open => {
                let p = self.dir.join(name);
                p.is_file().then_some(p)
            }
        }
    }

    /// „Speichern“ oder „Öffnen“: liefert das Ziel. Gibt es die Datei beim Speichern schon,
    /// erst beim zweiten Aufruf mit demselben Ziel.
    pub fn confirm(&mut self) -> ChooserAction {
        let Some(target) = self.target() else {
            self.error = Some(
                match self.mode {
                    ChooserMode::Save => {
                        "Bitte einen gültigen Dateinamen eingeben (ohne Sonderzeichen wie : ? * oder Schrägstriche)."
                    }
                    ChooserMode::Open => "Bitte eine vorhandene Datei wählen.",
                }
                .into(),
            );
            return ChooserAction::None;
        };
        self.error = None;
        if self.mode == ChooserMode::Save
            && target.exists()
            && self.confirm.as_ref() != Some(&target)
        {
            self.confirm = Some(target);
            return ChooserAction::None;
        }
        ChooserAction::Chosen(target)
    }

    /// Wartet die Auswahl auf die Bestätigung zum Ersetzen?
    pub fn asks_to_replace(&self) -> bool {
        self.confirm.is_some()
    }

    /// Zeichnet die Auswahl über die volle Breite.
    pub fn show(&mut self, ui: &mut egui::Ui) -> ChooserAction {
        let mut action = ChooserAction::None;
        ui.horizontal(|ui| {
            if ui.button("Ordner höher").clicked() {
                self.up();
            }
            let edit = theme::text_field(
                ui,
                egui::TextEdit::singleline(&mut self.dir_text).desired_width(f32::INFINITY),
            );
            if edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                self.go_to_text();
            }
        });
        let (mut enter, mut pick, mut double) = (None, None, None);
        egui::ScrollArea::vertical()
            .max_height(LIST_HEIGHT)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                for e in &self.entries {
                    let text = if e.is_dir {
                        format!("{}  (Ordner, Doppelklick zum Öffnen)", e.name)
                    } else {
                        e.name.clone()
                    };
                    let selected = !e.is_dir && e.name == self.file_name;
                    let r = ui.selectable_label(selected, text.as_str());
                    // Ordner öffnen nur per Doppelklick: ein einfacher Klick würde sofort den
                    // Ordner wechseln, und der zweite Klick träfe dann einen Eintrag darin.
                    if e.is_dir {
                        if r.double_clicked() {
                            enter = Some(e.name.clone());
                        }
                    } else if r.double_clicked() {
                        double = Some(e.name.clone());
                    } else if r.clicked() {
                        pick = Some(e.name.clone());
                    }
                }
                if self.entries.is_empty() {
                    ui.weak("Hier gibt es keine passenden Dateien.");
                }
                if self.truncated {
                    ui.weak(format!(
                        "Es werden nur die ersten {MAX_ENTRIES} Einträge gezeigt."
                    ));
                }
            });
        if let Some(n) = enter {
            self.enter(&n);
        }
        if let Some(n) = pick {
            self.select(&n);
        }
        ui.horizontal(|ui| {
            ui.label("Dateiname");
            let edit = theme::text_field(
                ui,
                egui::TextEdit::singleline(&mut self.file_name).desired_width(f32::INFINITY),
            );
            if edit.changed() {
                self.confirm = None;
            }
        });
        if let Some(e) = &self.error {
            ui.colored_label(error_color(ui.visuals().dark_mode), e);
        }
        if self.asks_to_replace() {
            ui.colored_label(warning_color(ui.visuals().dark_mode), REPLACE_HINT);
        }
        ui.horizontal(|ui| {
            let label = match self.mode {
                ChooserMode::Save if self.asks_to_replace() => "Ersetzen",
                ChooserMode::Save => "Speichern",
                ChooserMode::Open => "Öffnen",
            };
            if ui.button(label).clicked() {
                action = self.confirm();
            }
            if ui.button("Abbrechen").clicked() {
                action = ChooserAction::Cancel;
            }
        });
        if let Some(n) = double {
            self.select(&n);
            action = self.confirm();
        }
        action
    }
}
