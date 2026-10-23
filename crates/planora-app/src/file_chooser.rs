//! Eigene Dateiauswahl im Fenster (Plan P14: ohne neue Abhängigkeit statt des Dialogs des
//! Betriebssystems): Ordner wechseln oder eintippen, Datei wählen oder Namen eingeben, Ersetzen
//! einer vorhandenen Datei erst nach einem zweiten Klick.
//!
//! Über der Liste stehen die Schnellzugriffe ([`crate::places`]); Ordner und Dateien der Liste
//! haben ein Symbol, Ordner lassen sich per Rechtsklick zu den Favoriten hinzufügen.
//!
//! Im Ordner-Modus ([`ChooserMode::Folder`]) stehen nur Ordner in der Liste: Klick markiert,
//! Doppelklick geht hinein, gewählt wird der markierte oder sonst der aktuelle Ordner.

use crate::browser::error_color;
use crate::icons::{self, Icon};
use crate::mesh_body::warning_color;
use crate::places::{self, Place, PlaceKind, Places};
use crate::theme;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Höchstzahl der gezeigten Einträge eines Ordners (große Ordner bleiben bedienbar).
pub const MAX_ENTRIES: usize = 2000;
/// Höhe der Liste in Punkten.
const LIST_HEIGHT: f32 = 220.0;
const REPLACE_HINT: &str = "Diese Datei gibt es schon. Noch einmal klicken, um sie zu ersetzen.";

/// Knopf der Fußzeile im Ordner-Modus.
pub const CHOOSE_FOLDER: &str = "Diesen Ordner wählen";
/// Leere Liste im Ordner-Modus.
pub const NO_SUBFOLDERS: &str = "Hier gibt es keine Unterordner.";
/// Anfang der Zeile mit dem Ordner, der gewählt würde (Ordner-Modus, statt „Dateiname“).
pub const CHOSEN_PREFIX: &str = "Gewählt:";
/// Meldung, wenn der Ordner nicht (mehr) vorhanden ist.
pub const NO_FOLDER: &str = "Bitte einen vorhandenen Ordner wählen.";
const NO_MATCHING_FILES: &str = "Hier gibt es keine passenden Dateien.";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChooserMode {
    Save,
    Open,
    /// Einen Ordner wählen: nur Ordner gelistet, kein Dateiname.
    Folder,
}

/// Pfad für die Anzeige ohne das Windows-Präfix für lange Pfade (`\\?\`; bei Netzpfaden wird
/// `\\?\UNC\` zu `\\`). Für Dateizugriffe gilt weiter der echte Pfad.
pub fn display_path(path: &Path) -> String {
    let s = path.display().to_string();
    if let Some(rest) = s.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{rest}")
    } else if let Some(rest) = s.strip_prefix(r"\\?\") {
        rest.to_string()
    } else {
        s
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub is_dir: bool,
}

/// Hinweis beim Überfahren eines Ordners.
pub const OPEN_HINT: &str = "Doppelklick zum Öffnen";

/// Symbol eines Eintrags der Liste: Ordner (Akzentfarbe) oder Datei (ruhiges Grau, eines für
/// alle Dateiarten, damit die Namen bündig stehen).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntryIcon {
    Folder,
    File,
}

impl EntryIcon {
    pub fn of(entry: &Entry) -> EntryIcon {
        if entry.is_dir {
            EntryIcon::Folder
        } else {
            EntryIcon::File
        }
    }

    pub fn icon(self) -> Icon {
        match self {
            EntryIcon::Folder => Icon::Folder,
            EntryIcon::File => Icon::File,
        }
    }

    pub fn color(self, dark: bool) -> egui::Color32 {
        match self {
            EntryIcon::Folder => theme::folder_icon_color(dark),
            EntryIcon::File => theme::file_icon_color(dark),
        }
    }
}

/// Text eines Eintrags: nur der Name (der Hinweis zum Öffnen steht im Tooltip).
pub fn entry_text(entry: &Entry) -> &str {
    &entry.name
}

/// Tooltip eines Eintrags: bei Ordnern [`OPEN_HINT`].
pub fn entry_hover(entry: &Entry) -> Option<&'static str> {
    entry.is_dir.then_some(OPEN_HINT)
}

/// Eintrag des Kontextmenüs einer Zeile oder eines Favoriten-Knopfs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RowMenu {
    AddFavorite(PathBuf),
    RemoveFavorite(PathBuf),
}

impl RowMenu {
    pub fn label(&self) -> &'static str {
        match self {
            RowMenu::AddFavorite(_) => places::ADD_FAVORITE,
            RowMenu::RemoveFavorite(_) => places::REMOVE_FAVORITE,
        }
    }
}

/// Kontextmenü einer Zeile: nur Ordner haben eins (hinzufügen oder, wenn schon Favorit,
/// entfernen).
pub fn row_menu(entry: &Entry, dir: &Path, favorites: &[PathBuf]) -> Option<RowMenu> {
    if !entry.is_dir {
        return None;
    }
    let path = dir.join(&entry.name);
    Some(if places::is_favorite(favorites, &path) {
        RowMenu::RemoveFavorite(path)
    } else {
        RowMenu::AddFavorite(path)
    })
}

/// Kontextmenü eines Schnellzugriffs: nur Favoriten lassen sich entfernen.
pub fn place_menu(place: &Place) -> Option<RowMenu> {
    (place.kind == PlaceKind::Favorite).then(|| RowMenu::RemoveFavorite(place.path.clone()))
}

/// Wie oft das Bild neu gezeichnet wird, solange Laufwerke noch gesucht werden (höchstens
/// [`places::DRIVE_WAIT`] lang).
const DRIVE_REPAINT: Duration = Duration::from_millis(250);
/// Höhe der Leiste der Schnellzugriffe, ab der sie rollt (zwei Zeilen Knöpfe).
pub const PLACES_HEIGHT: f32 = 56.0;
/// Mindesthöhe der Liste, auch wenn die Leiste zwei Zeilen braucht.
const MIN_LIST_HEIGHT: f32 = 160.0;
/// Größte Höhe der Liste in sehr hohen Fenstern.
const MAX_LIST_HEIGHT: f32 = 700.0;
/// Anteil der zusätzlichen Fensterhöhe (über 600 px), um den die Liste wächst.
const LIST_GROWTH: f32 = 0.6;
/// Längste Beschriftung eines Knopfs der Leiste (Zeichen), länger wird mit „…“ gekürzt.
const MAX_PLACE_LABEL: usize = 40;
/// Strichbreite des Rahmens eines Knopfs der Leiste, in jedem Zustand gleich.
const PLACE_STROKE: f32 = 1.0;

/// Höhe der Liste im Fenster der Höhe `window_h`: bei 600 px [`LIST_HEIGHT`], darüber wächst
/// sie mit (höchstens [`MAX_LIST_HEIGHT`]).
pub fn list_height_for(window_h: f32) -> f32 {
    (LIST_HEIGHT + (window_h - 600.0).max(0.0) * LIST_GROWTH).min(MAX_LIST_HEIGHT)
}

/// Beschriftung eines Knopfs der Leiste, höchstens [`MAX_PLACE_LABEL`] Zeichen.
pub fn short_label(label: &str) -> String {
    if label.chars().count() <= MAX_PLACE_LABEL {
        label.to_string()
    } else {
        let head: String = label.chars().take(MAX_PLACE_LABEL - 1).collect();
        format!("{head}…")
    }
}

/// Knopf der Leiste mit immer gleich großem Rahmen: Strich und Innenabstand sind in jedem
/// Zustand gleich (in Ruhe unsichtbar), damit Überfahren, Drücken und Hervorhebung weder den
/// Knopf noch die Leiste verschieben.
fn place_button(ui: &mut egui::Ui, current: bool, icon: Icon, label: &str) -> egui::Response {
    let p = theme::palette(ui.visuals().dark_mode);
    let before = ui.ctx().read_response(ui.next_auto_id());
    let hovered = before.as_ref().is_some_and(egui::Response::hovered);
    let pressed = before
        .as_ref()
        .is_some_and(egui::Response::is_pointer_button_down_on);
    let fill = if current || pressed {
        p.bg_active
    } else if hovered {
        p.bg_field
    } else {
        egui::Color32::TRANSPARENT
    };
    let stroke = if current || hovered || pressed {
        p.border_subtle
    } else {
        egui::Color32::TRANSPARENT
    };
    // egui rechnet den Innenabstand aus Strichbreite und Ausdehnung des jeweiligen Zustands:
    // für diesen Knopf in allen Zuständen gleich setzen (ohne eigenen Unterbereich, der das
    // Umbrechen der Leiste verhindern würde), danach den Stil zurück.
    let saved = ui.style().clone();
    {
        let w = &mut ui.style_mut().visuals.widgets;
        for s in [
            &mut w.noninteractive,
            &mut w.inactive,
            &mut w.hovered,
            &mut w.active,
            &mut w.open,
        ] {
            s.bg_stroke.width = PLACE_STROKE;
            s.expansion = 0.0;
        }
    }
    let response = ui.add(
        egui::Button::new((icons::atom(icon), short_label(label)))
            .selected(current)
            .frame(true)
            .frame_when_inactive(true)
            .fill(fill)
            .stroke(egui::Stroke::new(PLACE_STROKE, stroke)),
    );
    ui.set_style(saved);
    response
}

/// Ein Knopf der Schnellzugriffe, wie er gezeichnet wird.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlaceButton {
    pub place: Place,
    /// Hervorgehoben: der aktuelle Ordner.
    pub current: bool,
    /// Kontextmenü (nur Favoriten).
    pub menu: Option<RowMenu>,
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
    /// Per Klick markierter Unterordner (nur Ordner-Modus); jeder Ordnerwechsel löscht ihn.
    marked: Option<String>,
    /// Vorhandene bekannte Ordner (einmal beim Öffnen der Auswahl ermittelt).
    known: Vec<Place>,
    /// Zuletzt geprüfter Projektordner und ob es ihn gibt (nicht in jedem Bild prüfen).
    project: Option<(PathBuf, bool)>,
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

    /// Auswahl eines Ordners: nur Ordner werden gelistet.
    pub fn folder(dir: PathBuf) -> FileChooser {
        FileChooser::new(ChooserMode::Folder, dir, Vec::new())
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
            marked: None,
            known: places::known_places(places::home_dir().as_deref(), &|p| p.is_dir()),
            project: None,
        };
        c.reload();
        c
    }

    /// Die Schnellzugriffe mit den bisher bekannten Laufwerken (wartet nie auf die Suche).
    pub fn places(&mut self, shared: &Places) -> Vec<Place> {
        let drives = places::system_drives().known();
        self.places_with(shared, &drives)
    }

    /// Wie [`FileChooser::places`] mit vorgegebenen Laufwerken.
    pub fn places_with(&mut self, shared: &Places, drives: &[PathBuf]) -> Vec<Place> {
        let project = match &shared.project_root {
            Some(root) => {
                let fresh = !matches!(&self.project, Some((p, _)) if p == root);
                if fresh {
                    self.project = Some((root.clone(), root.is_dir()));
                }
                self.project
                    .as_ref()
                    .filter(|(_, exists)| *exists)
                    .map(|(p, _)| p.as_path())
            }
            None => None,
        };
        places::build_places(&self.known, project, drives, &shared.favorites)
    }

    /// Wechselt in den Ordner eines Schnellzugriffs (wie eingetippt); fehlt er, die übliche
    /// Meldung, bei einem Favoriten mit dem Hinweis zum Entfernen.
    pub fn go_to_place(&mut self, place: &Place) -> bool {
        self.dir_text = place.path.display().to_string();
        let ok = self.go_to_text();
        if !ok && place.kind == PlaceKind::Favorite {
            self.error = Some(places::FAVORITE_MISSING.into());
        }
        ok
    }

    /// Führt einen Eintrag eines Kontextmenüs aus; eine volle Liste ergibt eine Meldung.
    pub fn apply_menu(&mut self, menu: &RowMenu, shared: &mut Places) {
        match menu {
            RowMenu::AddFavorite(p) => {
                if let Err(e) = shared.add_favorite(p) {
                    self.error = Some(e.into());
                }
            }
            RowMenu::RemoveFavorite(p) => shared.remove_favorite(p),
        }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    /// Liest den Ordner neu: Unterordner zuerst, dann passende Dateien (im Ordner-Modus nie
    /// Dateien), je nach Namen ohne Groß- und Kleinschreibung sortiert.
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
            } else if self.mode != ChooserMode::Folder && self.matches(&name) {
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
        self.marked = None;
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

    /// Per Klick markierter Unterordner (nur im Ordner-Modus).
    pub fn marked(&self) -> Option<&str> {
        self.marked.as_deref()
    }

    /// Markiert einen Unterordner der Liste; außerhalb des Ordner-Modus ohne Wirkung.
    pub fn mark(&mut self, name: &str) {
        if self.mode == ChooserMode::Folder {
            self.marked = Some(name.to_string());
        }
    }

    /// Ordner, der im Ordner-Modus gewählt würde (ohne Prüfung im Dateisystem, für die
    /// Anzeige in jedem Bild).
    fn folder_choice(&self) -> PathBuf {
        match &self.marked {
            Some(m) => self.dir.join(m),
            None => self.dir.clone(),
        }
    }

    pub fn select(&mut self, name: &str) {
        self.file_name = name.to_string();
        self.confirm = None;
    }

    /// Ziel: beim Speichern im aktuellen Ordner, Endung ergänzt; beim Öffnen nur eine
    /// vorhandene Datei. `None` bei leerem Namen oder Namen mit Ordnerteilen. Im Ordner-Modus
    /// der markierte Ordner (wenn es ihn gibt), sonst der aktuelle; `None`, wenn es den nicht
    /// (mehr) gibt.
    pub fn target(&self) -> Option<PathBuf> {
        if self.mode == ChooserMode::Folder {
            let marked = self.marked.as_ref().map(|m| self.dir.join(m));
            return match marked.filter(|p| p.is_dir()) {
                Some(p) => Some(p),
                None => self.dir.is_dir().then(|| self.dir.clone()),
            };
        }
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
            ChooserMode::Folder => unreachable!("oben behandelt"),
        }
    }

    /// „Speichern“, „Öffnen“ oder [`CHOOSE_FOLDER`]: liefert das Ziel. Gibt es die Datei beim
    /// Speichern schon, erst beim zweiten Aufruf mit demselben Ziel (ein Ordner wird nie
    /// ersetzt).
    pub fn confirm(&mut self) -> ChooserAction {
        let Some(target) = self.target() else {
            self.error = Some(
                match self.mode {
                    ChooserMode::Save => {
                        "Bitte einen gültigen Dateinamen eingeben (ohne Sonderzeichen wie : ? * oder Schrägstriche)."
                    }
                    ChooserMode::Open => "Bitte eine vorhandene Datei wählen.",
                    ChooserMode::Folder => NO_FOLDER,
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

    /// Zeichnet die ganze Auswahl über die volle Breite: [`FileChooser::show_body`] und darunter
    /// [`FileChooser::show_footer`]. Fenster mit fester Fußzeile zeichnen beide Teile getrennt,
    /// damit „Speichern“/„Öffnen“ ohne Rollen zu sehen bleibt.
    pub fn show_with(&mut self, ui: &mut egui::Ui, shared: &mut Places) -> ChooserAction {
        match self.show_body(ui, shared) {
            ChooserAction::None => self.show_footer(ui),
            other => {
                self.show_footer(ui);
                other
            }
        }
    }

    /// Oberer Teil: Ordnerzeile, Schnellzugriffe, Liste und Dateiname, im Ordner-Modus statt
    /// des Dateinamens die Zeile [`CHOSEN_PREFIX`] (alles, was rollen darf).
    /// Geänderte Favoriten stehen danach in `shared` (`changes`); eine Meldung aus dem letzten
    /// Speichern (`shared.notice`) wird übernommen und angezeigt.
    pub fn show_body(&mut self, ui: &mut egui::Ui, shared: &mut Places) -> ChooserAction {
        if let Some(notice) = shared.notice.take() {
            self.error = Some(notice.into());
        }
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
        let mut menu = None;
        let before = ui.cursor().top();
        self.show_places(ui, shared, &mut menu);
        let bar = ui.cursor().top() - before;
        // Eine zweite Zeile der Leiste geht von der Liste ab: zusammen bleiben sie so hoch wie
        // Leiste (eine Zeile) und Liste, das Fenster rollt dadurch nicht früher.
        let row = ui.spacing().interact_size.y + ui.spacing().item_spacing.y;
        let base = list_height_for(ui.ctx().content_rect().height());
        let list_height = (base - (bar - row).max(0.0)).max(MIN_LIST_HEIGHT);
        let (mut enter, mut pick, mut double, mut mark) = (None, None, None, None);
        let folder_mode = self.mode == ChooserMode::Folder;
        let dark = ui.visuals().dark_mode;
        theme::scroll_area(
            ui,
            egui::ScrollArea::vertical()
                .max_height(list_height)
                .auto_shrink([false, true]),
            |ui| {
                // Jede Zeile über die ganze Breite; zu lange Namen werden gekürzt (sonst würde
                // das Fenster breiter als vorgesehen).
                ui.with_layout(egui::Layout::top_down_justified(egui::Align::LEFT), |ui| {
                    for e in &self.entries {
                        let kind = EntryIcon::of(e);
                        // Symbol und Name als getrennte Bausteine: der Name bleibt ein eigener Text
                        // in der üblichen Schrift, alle Namen stehen bündig hinter dem Symbol.
                        let symbol =
                            icons::text(kind.icon(), icons::LABEL_ICON_SIZE, kind.color(dark));
                        let selected = if folder_mode {
                            self.marked.as_deref() == Some(e.name.as_str())
                        } else {
                            !e.is_dir && e.name == self.file_name
                        };
                        let mut r = ui.add(
                            egui::Button::selectable(
                                selected,
                                (symbol, egui::AtomExt::atom_shrink(entry_text(e), true)),
                            )
                            .truncate(),
                        );
                        if let Some(hint) = entry_hover(e) {
                            r = r.on_hover_text(hint);
                        }
                        if let Some(m) = row_menu(e, &self.dir, &shared.favorites) {
                            r.context_menu(|ui| {
                                if ui.button(m.label()).clicked() {
                                    menu = Some(m.clone());
                                    ui.close();
                                }
                            });
                        }
                        // Ordner öffnen nur per Doppelklick: ein einfacher Klick würde sofort den
                        // Ordner wechseln, und der zweite Klick träfe dann einen Eintrag darin.
                        // Im Ordner-Modus markiert der einfache Klick den Ordner.
                        if e.is_dir {
                            if r.double_clicked() {
                                enter = Some(e.name.clone());
                            } else if folder_mode && r.clicked() {
                                mark = Some(e.name.clone());
                            }
                        } else if r.double_clicked() {
                            double = Some(e.name.clone());
                        } else if r.clicked() {
                            pick = Some(e.name.clone());
                        }
                    }
                });
                if self.entries.is_empty() {
                    ui.weak(if folder_mode {
                        NO_SUBFOLDERS
                    } else {
                        NO_MATCHING_FILES
                    });
                }
                if self.truncated {
                    ui.weak(format!(
                        "Es werden nur die ersten {MAX_ENTRIES} Einträge gezeigt."
                    ));
                }
            },
        );
        if let Some(m) = menu {
            self.apply_menu(&m, shared);
        }
        if let Some(n) = enter {
            self.enter(&n);
        }
        if let Some(n) = pick {
            self.select(&n);
        }
        if let Some(n) = mark {
            self.mark(&n);
        }
        if folder_mode {
            // Statt des Dateinamens: der Ordner, der gewählt würde (eine Zeile, gekürzt).
            let chosen = format!("{CHOSEN_PREFIX} {}", display_path(&self.folder_choice()));
            ui.add(egui::Label::new(chosen).truncate());
            return action;
        }
        let mut enter = false;
        ui.horizontal(|ui| {
            ui.label("Dateiname");
            let edit = theme::text_field(
                ui,
                egui::TextEdit::singleline(&mut self.file_name).desired_width(f32::INFINITY),
            );
            if edit.changed() {
                self.confirm = None;
            }
            // Beim Öffnen wählt Enter im Namensfeld die Datei (wie „Öffnen“).
            enter = self.mode == ChooserMode::Open
                && edit.lost_focus()
                && ui.input(|i| i.key_pressed(egui::Key::Enter));
        });
        if enter {
            action = self.confirm();
        }
        if let Some(n) = double {
            self.select(&n);
            action = self.confirm();
        }
        action
    }

    /// Unterer Teil: Meldungen und die Knöpfe „Speichern“ bzw. „Ersetzen“, „Öffnen“ oder
    /// [`CHOOSE_FOLDER`] und „Abbrechen“ (in Fenstern mit Fußzeile dort, damit sie nie unter den Rand rollen).
    pub fn show_footer(&mut self, ui: &mut egui::Ui) -> ChooserAction {
        let mut action = ChooserAction::None;
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
                ChooserMode::Folder => CHOOSE_FOLDER,
            };
            if ui.button(label).clicked() {
                action = self.confirm();
            }
            if ui.button("Abbrechen").clicked() {
                action = ChooserAction::Cancel;
            }
        });
        action
    }

    /// Knöpfe der Schnellzugriffe mit Hervorhebung und Kontextmenü (ohne Oberfläche, für
    /// [`FileChooser::show_with`] und Tests).
    pub fn place_buttons(&mut self, shared: &Places, drives: &[PathBuf]) -> Vec<PlaceButton> {
        self.places_with(shared, drives)
            .into_iter()
            .map(|place| PlaceButton {
                current: places::is_current(&place, &self.dir),
                menu: place_menu(&place),
                place,
            })
            .collect()
    }

    /// Leiste der Schnellzugriffe: bricht in weitere Zeilen um und rollt ab
    /// [`PLACES_HEIGHT`]. Der Knopf des aktuellen Ordners ist hervorgehoben; Favoriten haben
    /// ein Kontextmenü zum Entfernen.
    fn show_places(&mut self, ui: &mut egui::Ui, shared: &Places, menu: &mut Option<RowMenu>) {
        let buttons = match &shared.drives {
            Some(drives) => self.place_buttons(shared, drives),
            None => {
                let scan = places::system_drives();
                if scan.wants_repaint() {
                    ui.ctx().request_repaint_after(DRIVE_REPAINT);
                }
                self.place_buttons(shared, &scan.known())
            }
        };
        if buttons.is_empty() {
            return;
        }
        let mut go = None;
        theme::scroll_area(
            ui,
            egui::ScrollArea::vertical()
                .id_salt("planora-places")
                .max_height(PLACES_HEIGHT)
                .auto_shrink([false, true]),
            |ui| {
                // Ohne eigene Unterbereiche je Knopf: nur so bricht die Zeile um.
                ui.horizontal_wrapped(|ui| {
                    for (i, b) in buttons.iter().enumerate() {
                        let p = &b.place;
                        let r = place_button(ui, b.current, p.kind.icon(), &p.label)
                            .on_hover_text(p.path.display().to_string());
                        if r.clicked() {
                            go = Some(i);
                        }
                        if let Some(m) = &b.menu {
                            r.context_menu(|ui| {
                                if ui.button(m.label()).clicked() {
                                    *menu = Some(m.clone());
                                    ui.close();
                                }
                            });
                        }
                    }
                });
            },
        );
        if let Some(i) = go {
            self.go_to_place(&buttons[i].place);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_verbatim_prefix_is_hidden_in_the_display_only() {
        assert_eq!(display_path(Path::new(r"\\?\C:\Users\x")), r"C:\Users\x");
        assert_eq!(display_path(Path::new(r"\\?\UNC\srv\frei")), r"\\srv\frei");
        assert_eq!(display_path(Path::new(r"C:\a")), r"C:\a");
        assert_eq!(display_path(Path::new("/home/x")), "/home/x");
    }
}
