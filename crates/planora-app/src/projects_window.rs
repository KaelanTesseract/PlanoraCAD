//! Fenster „Projekte“ (Spec §4): Zustand, Navigation, Kacheln und Miniaturen. Das Fenster liest
//! das Projektverzeichnis im Hintergrund ([`scan_library`] in einem [`Job`]); bis ein neues
//! Ergebnis da ist, bleiben die bisherigen Kacheln stehen. Die Kacheln einer Ebene werden nur
//! berechnet, wenn ein neues Ergebnis kommt oder der Ort wechselt, nie in jedem Bild. Aktionen und
//! ihre Fenster kommen aus [`crate::project_actions`].
//!
//! Gezeichnet wird wie bei den Fenstern der Dateiauswahl: [`DialogSize::File`], mittig beim
//! Öffnen, feste Fußzeile. Der Kachelbereich hat eine feste Höhe ([`grid_height_for`]) und rollt
//! selbst; Kacheln haben feste Maße ([`TILE_MIN_W`], [`TILE_H`], [`TILE_GAP`]); ihr Rand wird
//! innen gezeichnet ([`TILE_BORDER`], gewählt [`SELECTED_BORDER`] in Akzentfarbe auf getönter
//! Fläche). So verschiebt sich nichts, wenn Kacheln oder Miniaturen eintreffen, beim Überfahren
//! oder beim Wählen. Ein neuer Stand eines Dokuments über den Arbeitsbereich
//! (`Workspace::content_revision`) wird wie „Aktualisieren“ neu gelesen.
//!
//! Miniaturen entstehen im [`ThumbWorker`] (einer je Programmlauf, gehört diesem Fenster). Die
//! Warteschlange wird nur neu gesetzt, wenn sich Kacheln, der sichtbare Ausschnitt oder Hell/Dunkel
//! ändern, sichtbare Kacheln zuerst. Ergebnisse werden je Bild ohne Warten abgeholt und gelten nur,
//! wenn ihr Schlüssel noch der aktuelle ist. Gemerkt wird je Dokumentordner (nicht je Pfad in der
//! Bibliothek): ein anderes Projektverzeichnis mit gleichnamigen Projekten zeigt nie alte Bilder.
//! Was sich nicht speichern ließ oder fehlschlug, bleibt bis zur nächsten Version im Speicher und
//! wird in dieser Sitzung nicht neu gebaut.

use crate::dialog::{Dialog, DialogSize};
use crate::file_chooser::display_path;
use crate::fonts;
use crate::icons::{self, Icon};
use crate::jobs::{Job, Poll};
use crate::project_actions::{self as actions, ProjectsUi, no_library, read_error_prefix};
use crate::project_tiles::{
    MAX_TILES, Scan, Tile, TileKind, Tone, changed_line, footer_text_of, meta_line,
    scan_library_with, tiles_at,
};
use crate::theme;
use crate::thumbnail::{Image, THUMB_H, THUMB_W};
use crate::thumbnail_cache::{Thumb, ThumbKey, impossible_text};
use crate::thumbnail_worker::{BUSY_RETRY, ThumbRequest, ThumbWorker};
use crate::workspace::Workspace;
use egui::text::{LayoutJob, TextWrapping};
use egui::{Align, Align2, Color32, FontId, Layout, Rect, Sense, Stroke, StrokeKind, pos2, vec2};
use planora_i18n::tr;
use planora_project::library::{NodeKind, NodePath, TreeNode};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

pub fn title() -> String {
    tr!("projects-title")
}
pub fn rename() -> String {
    tr!("projects-rename")
}
pub fn move_item() -> String {
    tr!("projects-move")
}
pub fn delete() -> String {
    tr!("projects-delete")
}
/// Meldung, wenn das Lesen ohne Ergebnis endete.
pub fn scan_lost() -> String {
    tr!("projects-scan-lost")
}
/// Längstes Warten in [`ProjectsWindow::wait_for_scan`].
const SCAN_WAIT_LIMIT: Duration = Duration::from_secs(60);

/// Kleinste Breite einer Kachel.
pub const TILE_MIN_W: f32 = 240.0;
/// Höhe jeder Kachel.
pub const TILE_H: f32 = 220.0;
/// Abstand zwischen Kacheln (waagrecht und senkrecht).
pub const TILE_GAP: f32 = theme::SPACE_3;
/// Fensterhöhe, die nicht zum Kachelbereich gehört (Rand, Kopf, Knöpfe, Fußzeile).
pub const PROJECTS_CHROME_H: f32 = 260.0;
pub fn footer_hint() -> String {
    tr!("projects-footer-hint")
}
pub fn directory_button() -> String {
    tr!("projects-directory-button")
}
pub fn no_bodies() -> String {
    tr!("projects-no-bodies")
}
pub fn too_complex() -> String {
    tr!("projects-too-complex")
}
pub fn new_project() -> String {
    tr!("projects-new-project")
}
pub fn new_document() -> String {
    tr!("projects-new-document")
}
pub fn new_folder() -> String {
    tr!("projects-new-folder")
}
pub fn refresh() -> String {
    tr!("projects-refresh")
}
/// Leere Bibliothek.
pub fn empty() -> String {
    tr!("projects-empty")
}
/// Solange das erste Lesen läuft.
pub fn reading() -> String {
    tr!("projects-reading")
}
/// Steht im Pfad für zusammengefallene vordere Teile.
pub const CRUMB_ELLIPSIS: &str = "…";
/// Hinweis am Pfeil „eine Ebene hinauf“ im Pfad.
pub fn back_hint() -> String {
    tr!("projects-back-hint")
}

/// Kennung des Fensters (unabhängig vom Titel).
const WINDOW_ID: &str = "planora_projects_window";
/// Höhe der Bildfläche einer Kachel.
const IMAGE_H: f32 = 140.0;
/// Innenabstand einer Kachel.
const TILE_PAD: f32 = theme::SPACE_2;
/// Rand einer Kachel in Ruhe und beim Überfahren (innen gezeichnet).
pub const TILE_BORDER: f32 = 1.0;
/// Rand der gewählten Kachel (Akzent), ebenfalls innen gezeichnet: die Kachel bleibt gleich
/// groß, nichts verschiebt sich.
pub const SELECTED_BORDER: f32 = 2.0;
/// Leeres Projekt (Ebene 2).
pub fn empty_project() -> String {
    tr!("projects-empty-project")
}
/// Leerer Ordner.
pub fn empty_folder() -> String {
    tr!("projects-empty-folder")
}
/// Tastaturfokus: Ring außen um die Kachel (Mitte des Strichs so weit außerhalb), deutlich
/// anders als der Rand der Auswahl.
pub const FOCUS_OFFSET: f32 = 2.0;
pub const FOCUS_WIDTH: f32 = 2.0;
/// Platz über und unter den Kacheln, damit der Fokusring oben und unten nicht abgeschnitten wird.
const FOCUS_ROOM: f32 = FOCUS_OFFSET + FOCUS_WIDTH / 2.0;
/// Zeilenhöhen der Texte unter der Bildfläche.
const NAME_LINE: f32 = 20.0;
const SMALL_LINE: f32 = 16.0;
const NAME_SIZE: f32 = 14.0;
const SMALL_SIZE: f32 = 12.0;
const CAPTION_SIZE: f32 = 11.0;
const FOLDER_ICON_SIZE: f32 = 64.0;
const CUBE_ICON_SIZE: f32 = 40.0;
/// Ruhiger Hinweis, solange eine Miniatur entsteht (hier oder in einer anderen Instanz).
pub fn building_text() -> String {
    tr!("projects-building")
}
/// So oft wird neu gezeichnet, solange eigene Miniaturen entstehen.
pub const THUMB_POLL: Duration = Duration::from_millis(100);
/// Höchstens so viele Texturen gleichzeitig (sichtbare werden nie verdrängt).
pub const MAX_TEXTURES: usize = 64;
/// Texturen bleiben für Kacheln bis so viele Höhen des Kachelbereichs über und unter dem
/// sichtbaren Ausschnitt.
const KEEP_MARGIN: f32 = 2.0;

/// Gemeinsames leeres Bild: eine Miniatur, deren Pixel schon als Textur hochgeladen und danach
/// verworfen wurden.
static UPLOADED: std::sync::LazyLock<Arc<Image>> = std::sync::LazyLock::new(|| {
    Arc::new(Image {
        width: 0,
        height: 0,
        rgba: Vec::new(),
    })
});

/// Hat die Miniatur noch ihre Pixel?
fn has_pixels(thumb: &Thumb) -> bool {
    matches!(thumb, Thumb::Image(img) if !img.rgba.is_empty())
}

/// Eine hochgeladene Miniatur.
struct Tex {
    key: ThumbKey,
    handle: egui::TextureHandle,
    /// Bild, in dem sie zuletzt gezeichnet wurde (Verdrängen der am längsten ungenutzten).
    used: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectsEvent {
    None,
    OpenSettings,
    Closed,
}

/// Einträge des Rechtsklick-Menüs einer Kachel; Projekte lassen sich nicht verschieben.
pub fn tile_menu(kind: TileKind) -> Vec<String> {
    match kind {
        TileKind::Project => vec![rename(), delete()],
        TileKind::Folder | TileKind::Document => vec![rename(), move_item(), delete()],
    }
}

/// Spalten im Kachelbereich der Breite `inner_width`: so viele Kacheln der Mindestbreite mit
/// Abstand, wie hineinpassen, mindestens eine. Die Kacheln teilen die Zeile gleichmäßig.
pub fn columns_for(inner_width: f32) -> usize {
    (((inner_width + TILE_GAP) / (TILE_MIN_W + TILE_GAP)).floor() as usize).max(1)
}

/// Höhe des Kachelbereichs im Fenster der Höhe `window_height` (fest, rollt selbst).
pub fn grid_height_for(window_height: f32) -> f32 {
    (window_height - PROJECTS_CHROME_H).max(0.0)
}

/// Laufendes Lesen mit dem Stand, zu dem es begann.
struct ScanJob {
    job: Job<Result<Scan, String>>,
    revision: u64,
    generation: u64,
}

/// Gemerkte Miniatur eines Dokumentordners.
struct Slot {
    key: ThumbKey,
    /// `None` = angefragt.
    thumb: Option<Thumb>,
    /// Liegt als Zusatzdatei vor (sonst nur hier im Speicher).
    stored: bool,
    /// Zeit des letzten Ergebnisses (für den erneuten Versuch nach `Thumb::Busy`).
    since: Instant,
}

impl Slot {
    fn requested(key: &ThumbKey) -> Slot {
        Slot {
            key: key.clone(),
            thumb: None,
            stored: false,
            since: Instant::now(),
        }
    }
}

/// Was in der Bildfläche einer Kachel steht.
enum Picture {
    Folder,
    Texture(egui::TextureId),
    /// Würfel, darunter ein Hinweis (ruhig, ohne Bewegung).
    Placeholder {
        caption: Option<String>,
    },
}

/// Eingabe auf eine Kachel oder einen Knopf (nach dem Zeichnen ausgeführt).
enum Action {
    Select(NodePath),
    Activate(NodePath),
    Menu(Tile, String),
    Back,
    GoTo(NodePath),
    NewProject,
    NewDocument,
    NewFolder,
    Settings,
}

/// Angaben für ein Bild des Fensters.
struct Frame {
    locked: bool,
    has_library: bool,
    /// Das Projektverzeichnis ließ sich nicht lesen.
    read_error: bool,
    root_text: String,
    grid_h: f32,
    alive: bool,
}

#[derive(Default)]
pub struct ProjectsWindow {
    /// Auswahl, Baum und Fenster der Aktionen.
    pub ui: ProjectsUi,
    /// Rechteck des Fensters im letzten Bild (Layouttests).
    pub last_rect: Option<Rect>,
    open: bool,
    /// Aktuelle Ebene (leer = Projekte).
    location: NodePath,
    job: Option<ScanJob>,
    /// Letztes Ergebnis des Lesens.
    data: Option<Result<Scan, String>>,
    /// Kacheln der aktuellen Ebene aus `data`.
    tiles: Vec<Tile>,
    /// Die Ebene hat mehr als [`MAX_TILES`] Einträge.
    too_many: bool,
    /// Zahl der Einträge der Ebene (auch über [`MAX_TILES`]).
    total: usize,
    /// Zählt die Berechnungen der Kacheln (Tests).
    refreshes: u64,
    /// Zuletzt gesehenes Projektverzeichnis (`None` = noch nie hingesehen).
    root_seen: Option<Option<PathBuf>>,
    /// Zuletzt gesehenes `Workspace::content_revision`.
    content_seen: Option<u64>,
    /// Pause nach jedem gelesenen Dokument (Tests: langsames Lesen).
    scan_delay: Duration,
    /// Weckt die Oberfläche, wenn das Lesen fertig ist (gesetzt in `show`).
    repaint: Option<egui::Context>,
    /// Miniatur-Arbeiter (beim ersten Bedarf gestartet).
    worker: Option<ThumbWorker>,
    /// Hell/Dunkel der Miniaturen (aktiver Tab).
    dark: bool,
    /// Je Kachel: Dokumentordner und Schlüssel ihrer Miniatur (Ordner und unlesbare: keine).
    sources: Vec<Option<(PathBuf, ThumbKey)>>,
    /// Miniaturen je Dokumentordner.
    slots: HashMap<PathBuf, Slot>,
    /// Texturen je Dokumentordner: nur für Kacheln nahe am sichtbaren Ausschnitt, höchstens
    /// [`MAX_TEXTURES`]. Die Pixel einer gespeicherten Miniatur werden nach dem Hochladen
    /// verworfen; wird ihre Textur verdrängt und die Kachel wieder sichtbar, wird sie über die
    /// Warteschlange neu geholt (aus der Zusatzdatei). Nicht gespeicherte behalten ihre Pixel.
    textures: HashMap<PathBuf, Tex>,
    textures_made: u64,
    /// Zählt die Bilder (für `Tex::used`).
    frame_no: u64,
    /// Dokumentordner der Kacheln nahe am sichtbaren Ausschnitt (Texturen bleiben).
    keep: HashSet<PathBuf>,
    /// Die Warteschlange muss neu gesetzt werden.
    queue_dirty: bool,
    /// Dokumentordner der sichtbaren Kacheln im letzten Bild.
    visible: HashSet<PathBuf>,
    /// Zuletzt gesetzte Warteschlange (sichtbare, übrige).
    last_queue: (Vec<ThumbRequest>, Vec<ThumbRequest>),
    /// Kachelrechtecke und Kachelbereich im letzten Bild, Kachel mit Tastaturfokus.
    tile_rects: Vec<Rect>,
    grid_rect: Option<Rect>,
    focused: Option<usize>,
    /// Zählt die Ortswechsel (eigener Rollzustand je Ebene).
    view: u64,
    view_location: NodePath,
}

/// Knoten `path` im Baum.
fn find_node<'a>(nodes: &'a [TreeNode], path: &NodePath) -> Option<&'a TreeNode> {
    let mut level = nodes;
    let mut found = None;
    for depth in 0..path.len() {
        let n = level.iter().find(|n| n.path[..] == path[..=depth])?;
        level = &n.children;
        found = Some(n);
    }
    found
}

/// Dokumentordner und Schlüssel der Miniatur einer Kachel: das Dokument selbst bzw. das zuletzt
/// geänderte lesbare darunter; Ordner zeigen ihr Symbol.
fn thumb_source(scan: &Scan, tile: &Tile, dark: bool) -> Option<(PathBuf, ThumbKey)> {
    if tile.kind == TileKind::Folder {
        return None;
    }
    let (path, facts) = tile.newest.as_ref()?;
    let dir = scan.docs.get(path)?.dir.clone();
    let id = scan.ids.get(path)?;
    Some((dir, ThumbKey::new(id, facts.version, &facts.branch, dark)))
}

impl ProjectsWindow {
    /// Öffnet das Fenster; der Stand wird neu gelesen.
    pub fn open(&mut self) {
        self.open = true;
        self.ui.invalidate();
        self.queue_dirty = true;
        self.last_queue = Default::default();
    }

    /// Schließt das Fenster; ein laufendes Lesen wird abgebrochen, wartende Miniaturen entfallen
    /// (eine gerade entstehende wird fertig und gespeichert).
    pub fn close(&mut self) {
        self.open = false;
        self.job = None;
        if let Some(w) = self.worker.as_ref() {
            w.cancel_pending();
        }
        self.last_queue = Default::default();
        self.textures.clear();
        self.keep.clear();
        self.tile_rects.clear();
        self.grid_rect = None;
        self.focused = None;
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    /// Aktuelle Ebene (leer = Projekte).
    pub fn location(&self) -> &NodePath {
        &self.location
    }

    /// Geht zur Ebene `at` (auch für die Pfadteile); gibt es sie nicht, zum nächsten
    /// vorhandenen Elternteil.
    pub fn go_to(&mut self, at: NodePath) {
        self.location = at;
        self.refresh_tiles();
    }

    /// Eine Ebene hinauf.
    pub fn back(&mut self) {
        self.location.pop();
        self.refresh_tiles();
    }

    /// Kacheln der aktuellen Ebene neu berechnen; verschwundene Orte fallen auf den nächsten
    /// vorhandenen Elternteil zurück (sonst oben).
    fn refresh_tiles(&mut self) {
        self.refreshes += 1;
        if let Some(Ok(scan)) = &self.data {
            loop {
                if let Some(tiles) = tiles_at(scan, &self.location) {
                    let total = if self.location.is_empty() {
                        scan.tree.len()
                    } else {
                        find_node(&scan.tree, &self.location).map_or(0, |n| n.children.len())
                    };
                    self.too_many = total > MAX_TILES;
                    self.total = total;
                    self.tiles = tiles;
                    break;
                }
                self.location.pop();
            }
        } else {
            self.tiles.clear();
            self.too_many = false;
            self.total = 0;
        }
        self.rebuild_sources();
    }

    /// Ordner und Schlüssel der Miniaturen neu bestimmen; Miniaturen und Texturen von Kacheln,
    /// die nicht mehr da sind, freigeben (nicht gespeicherte Ergebnisse bleiben gemerkt).
    fn rebuild_sources(&mut self) {
        let dark = self.dark;
        self.sources = match &self.data {
            Some(Ok(scan)) => self
                .tiles
                .iter()
                .map(|t| thumb_source(scan, t, dark))
                .collect(),
            _ => Vec::new(),
        };
        let wanted: HashSet<&PathBuf> = self.sources.iter().flatten().map(|(d, _)| d).collect();
        self.slots
            .retain(|dir, s| wanted.contains(dir) || (s.thumb.is_some() && !s.stored));
        self.textures.retain(|dir, _| wanted.contains(dir));
        self.queue_dirty = true;
    }

    /// Pause nach jedem gelesenen Dokument (nur für Tests).
    #[doc(hidden)]
    pub fn set_scan_delay(&mut self, per_document: Duration) {
        self.scan_delay = per_document;
    }

    /// Eigener Miniatur-Arbeiter (nur für Tests).
    #[doc(hidden)]
    pub fn set_worker(&mut self, w: ThumbWorker) {
        self.worker = Some(w);
        self.queue_dirty = true;
        self.last_queue = Default::default();
    }

    /// Der Miniatur-Arbeiter, falls schon gestartet (Tests).
    #[doc(hidden)]
    pub fn worker(&self) -> Option<&ThumbWorker> {
        self.worker.as_ref()
    }

    /// Ende des Programms: der Miniatur-Arbeiter (falls gestartet) nimmt nichts mehr an und
    /// bekommt höchstens `wait`, um einen laufenden Aufbau zu beenden und zu speichern
    /// ([`ThumbWorker::shutdown`]); ein laufendes Lesen wird abgebrochen.
    pub fn shutdown(&mut self, wait: Duration) {
        self.job = None;
        if let Some(w) = &self.worker {
            w.shutdown(wait);
        }
    }

    /// Läuft gerade ein Lesen?
    pub fn scanning(&self) -> bool {
        self.job.is_some()
    }

    /// Startet bei Bedarf das Lesen (`is_stale`, anderes Verzeichnis) und holt fertige Ergebnisse;
    /// hält bis zum neuen Ergebnis die alten Kacheln. Ein anderes Verzeichnis setzt Ort und Auswahl
    /// zurück (und schließt die Fenster der Aktionen; die alten Kacheln verschwinden sofort).
    /// Wartet nie auf das Lesen.
    pub fn update(&mut self, ws: &Workspace) {
        let root = ws.library.as_ref().map(|l| l.root().path().to_path_buf());
        if self.root_seen.as_ref().is_some_and(|seen| *seen != root) {
            self.location.clear();
            self.ui.select(None);
            self.ui.close_dialogs();
            self.ui.invalidate();
            self.job = None;
            self.data = None;
            self.drop_tiles();
        }
        self.root_seen = Some(root.clone());
        self.ui.observe(ws);
        // Ein neuer Stand eines Dokuments (Speichern, Wiederherstellen, Zweigwechsel): neu
        // lesen wie „Aktualisieren“; Ort, Auswahl und Fenster der Aktionen bleiben. Die Miniatur
        // folgt, weil ihr Schlüssel die Version enthält.
        if self
            .content_seen
            .is_some_and(|seen| seen != ws.content_revision)
        {
            self.ui.invalidate();
        }
        self.content_seen = Some(ws.content_revision);
        let Some(root) = root else {
            self.job = None;
            if self.data.is_some() || !self.tiles.is_empty() {
                self.data = None;
                self.drop_tiles();
            }
            return;
        };
        if !self.open {
            return;
        }
        let current = (ws.library_revision, self.ui.generation());
        let running = self
            .job
            .as_ref()
            .is_some_and(|j| (j.revision, j.generation) == current);
        if self.ui.is_stale(ws) && !running {
            let delay = self.scan_delay;
            let wake = self.repaint.clone();
            // ein überholtes Lesen bricht beim Fallenlassen ab
            self.job = Some(ScanJob {
                job: Job::spawn_notify(
                    Default::default(),
                    // erst nach dem Senden des Ergebnisses; egui zeichnet sonst bis zur nächsten
                    // Eingabe nicht neu und das „Wird gelesen“ bliebe stehen
                    move || {
                        if let Some(ctx) = wake {
                            ctx.request_repaint();
                        }
                    },
                    move |progress| {
                        scan_library_with(&root, progress, |_| {
                            if !delay.is_zero() {
                                std::thread::sleep(delay);
                            }
                        })
                    },
                ),
                revision: current.0,
                generation: current.1,
            });
        }
        let Some(j) = &self.job else { return };
        let result = match j.job.poll() {
            Poll::Pending => return,
            Poll::Ready(r) => r,
            Poll::Lost => Err(scan_lost()),
        };
        let revision = j.revision;
        self.job = None;
        self.ui.set_tree(
            result
                .as_ref()
                .map(|s| s.tree.clone())
                .map_err(Clone::clone),
            revision,
        );
        self.data = Some(result);
        self.refresh_tiles();
    }

    /// Alte Kacheln sofort weg (anderes oder kein Verzeichnis); wartende Miniaturen entfallen.
    fn drop_tiles(&mut self) {
        self.tiles.clear();
        self.too_many = false;
        self.total = 0;
        if let Some(w) = &self.worker {
            w.cancel_pending();
        }
        self.last_queue = Default::default();
        self.rebuild_sources();
    }

    /// Wie [`ProjectsWindow::update`], wartet aber (höchstens eine Minute), bis das Lesen
    /// fertig ist (Tests und Abläufe ohne Fenster).
    pub fn wait_for_scan(&mut self, ws: &Workspace) {
        let deadline = Instant::now() + SCAN_WAIT_LIMIT;
        self.update(ws);
        while self.job.is_some() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
            self.update(ws);
        }
    }

    /// Letztes Ergebnis des Lesens (`None`, solange nichts gelesen ist oder ohne
    /// Projektverzeichnis).
    pub fn scan_state(&self) -> Option<&Result<Scan, String>> {
        self.data.as_ref()
    }

    /// Kacheln der aktuellen Ebene (leer, solange nichts gelesen ist).
    pub fn tiles(&self) -> &[Tile] {
        &self.tiles
    }

    /// Wie oft die Kacheln berechnet wurden (Tests: nicht in jedem Bild).
    #[doc(hidden)]
    pub fn tile_refreshes(&self) -> u64 {
        self.refreshes
    }

    /// Rechtecke der Kacheln im letzten Bild, in der Reihenfolge von [`ProjectsWindow::tiles`].
    #[doc(hidden)]
    pub fn tile_rects(&self) -> &[Rect] {
        &self.tile_rects
    }

    /// Sichtbarer Kachelbereich im letzten Bild.
    #[doc(hidden)]
    pub fn grid_rect(&self) -> Option<Rect> {
        self.grid_rect
    }

    /// Kachel mit dem Tastaturfokus im letzten Bild.
    #[doc(hidden)]
    pub fn focused_tile(&self) -> Option<usize> {
        self.focused
    }

    /// Zahl der hochgeladenen Texturen (Tests).
    #[doc(hidden)]
    pub fn texture_count(&self) -> usize {
        self.textures.len()
    }

    /// Zahl der Miniaturen, deren Pixel noch im Speicher liegen (Tests).
    #[doc(hidden)]
    pub fn pixel_count(&self) -> usize {
        self.slots
            .values()
            .filter(|s| s.thumb.as_ref().is_some_and(has_pixels))
            .count()
    }

    /// Gemerkte Miniatur des Dokuments im Ordner `doc_dir` (`None` = keine oder angefragt). Ein
    /// Bild, dessen Pixel schon hochgeladen und verworfen sind, ist ein leeres `Thumb::Image`.
    pub fn thumb_state(&self, doc_dir: &Path) -> Option<&Thumb> {
        self.slots.get(doc_dir)?.thumb.as_ref()
    }

    /// Wartet höchstens `timeout`, bis alle Miniaturen der Kacheln da sind (Tests). `false` bei
    /// Ablauf oder ohne lebenden Arbeiter.
    pub fn wait_for_thumbnails(&mut self, timeout: Duration) -> bool {
        let deadline = Instant::now().checked_add(timeout);
        loop {
            self.sync_queue();
            self.collect();
            if !self.outstanding() {
                return true;
            }
            let Some(w) = &self.worker else { return false };
            if !w.alive() {
                return false;
            }
            let step = Duration::from_millis(20);
            let step = match deadline {
                Some(d) => {
                    let now = Instant::now();
                    if now >= d {
                        return false;
                    }
                    (d - now).min(step)
                }
                None => step,
            };
            w.wait_idle(step);
        }
    }

    /// Fehlt für eine Kachel noch die Miniatur zum aktuellen Schlüssel?
    fn outstanding(&self) -> bool {
        self.sources.iter().flatten().any(|(dir, key)| {
            self.slots
                .get(dir)
                .is_none_or(|s| s.key != *key || s.thumb.is_none())
        })
    }

    /// Fertige Miniaturen abholen (wartet nie). Ergebnisse zu einem alten Schlüssel entfallen.
    /// Ob weiter neu gezeichnet wird, entscheidet danach [`ProjectsWindow::outstanding`] (fehlt
    /// einer Kachel noch ihr Ergebnis?): das ist strenger als „der Arbeiter ist beschäftigt“ und
    /// verpasst kein ungelesenes Ergebnis.
    fn collect(&mut self) {
        let Some(w) = &self.worker else { return };
        let now = Instant::now();
        let results = w.poll();
        if !results.is_empty() {
            // Erledigte Aufträge sind nicht mehr in der Warteschlange des Arbeiters: dieselbe
            // Liste später (etwa nach `Thumb::Busy`) muss wieder gesendet werden.
            self.last_queue = Default::default();
        }
        for done in results {
            if let Some(s) = self.slots.get_mut(&done.dir)
                && s.key == done.key
            {
                s.thumb = Some(done.outcome.thumb);
                s.stored = done.outcome.stored;
                s.since = now;
            }
        }
    }

    /// Setzt die Warteschlange neu, wenn sich Kacheln, sichtbarer Ausschnitt oder Schlüssel
    /// geändert haben (sichtbare zuerst); `Thumb::Busy` wird nach [`BUSY_RETRY`] neu angefragt.
    /// Liefert, wann frühestens ein solcher neuer Versuch fällig ist.
    fn sync_queue(&mut self) -> Option<Duration> {
        let now = Instant::now();
        let mut retry_in: Option<Duration> = None;
        for (dir, key) in self.sources.iter().flatten() {
            let Some(s) = self.slots.get_mut(dir) else {
                continue;
            };
            if s.key != *key || !matches!(s.thumb, Some(Thumb::Busy)) {
                continue;
            }
            let waited = now.saturating_duration_since(s.since);
            if waited >= BUSY_RETRY {
                s.thumb = None;
                self.queue_dirty = true;
            } else {
                let left = BUSY_RETRY - waited;
                retry_in = Some(retry_in.map_or(left, |r| r.min(left)));
            }
        }
        if !self.queue_dirty {
            return retry_in;
        }
        self.queue_dirty = false;
        let (mut visible, mut rest) = (Vec::new(), Vec::new());
        for (dir, key) in self.sources.iter().flatten() {
            match self.slots.get_mut(dir) {
                Some(s) if s.key == *key && s.thumb.is_some() => {
                    // verdrängt (ohne Pixel und ohne Textur): nur sichtbare neu holen, sonst
                    // holten sich Kacheln am Rand immer wieder gegenseitig heraus
                    let evicted = matches!(&s.thumb, Some(Thumb::Image(img)) if img.rgba.is_empty())
                        && !self.textures.get(dir).is_some_and(|t| t.key == *key);
                    if !(evicted && self.visible.contains(dir)) {
                        continue;
                    }
                    *s = Slot::requested(key);
                }
                Some(s) if s.key == *key => {}
                Some(s) => *s = Slot::requested(key),
                None => {
                    self.slots.insert(dir.clone(), Slot::requested(key));
                }
            }
            let req = ThumbRequest {
                dir: dir.clone(),
                key: key.clone(),
            };
            if self.visible.contains(dir) {
                visible.push(req);
            } else {
                rest.push(req);
            }
        }
        let planned = (visible, rest);
        if planned != self.last_queue {
            if !(planned.0.is_empty() && planned.1.is_empty()) || self.worker.is_some() {
                let w = self.worker.get_or_insert_with(ThumbWorker::start);
                w.set_queue(planned.0.clone(), planned.1.clone());
            }
            self.last_queue = planned;
        }
        retry_in
    }

    /// Lädt Miniaturen von Kacheln nahe am sichtbaren Ausschnitt als Textur hoch und verwirft
    /// danach die Pixel gespeicherter Miniaturen (auch die von weiter entfernten Kacheln: die
    /// werden, wenn sie sichtbar werden, aus der Zusatzdatei neu geholt). Gibt Texturen außerhalb
    /// des nahen Bereichs frei und hält höchstens [`MAX_TEXTURES`] (die am längsten nicht
    /// gezeichneten zuerst; sichtbare nie). `true`, wenn etwas hochgeladen wurde.
    fn manage_textures(&mut self, ctx: &egui::Context) -> bool {
        let mut uploaded = false;
        for (dir, key) in self.sources.iter().flatten() {
            let Some(slot) = self.slots.get_mut(dir) else {
                continue;
            };
            if slot.key != *key {
                continue;
            }
            let Some(Thumb::Image(img)) = &slot.thumb else {
                continue;
            };
            if img.rgba.is_empty() {
                continue;
            }
            if self.keep.contains(dir) && !self.textures.get(dir).is_some_and(|t| t.key == *key) {
                let color = egui::ColorImage::from_rgba_unmultiplied(
                    [img.width as usize, img.height as usize],
                    &img.rgba,
                );
                self.textures_made += 1;
                let handle = ctx.load_texture(
                    format!("planora-miniatur-{}", self.textures_made),
                    color,
                    egui::TextureOptions::LINEAR,
                );
                let tex = Tex {
                    key: key.clone(),
                    handle,
                    used: self.frame_no,
                };
                self.textures.insert(dir.clone(), tex);
                uploaded = true;
            }
            if slot.stored {
                slot.thumb = Some(Thumb::Image(UPLOADED.clone()));
            }
        }
        self.textures.retain(|dir, _| self.keep.contains(dir));
        while self.textures.len() > MAX_TEXTURES {
            let oldest = self
                .textures
                .iter()
                .filter(|(dir, _)| !self.visible.contains(*dir))
                .min_by_key(|(_, t)| t.used)
                .map(|(dir, _)| dir.clone());
            match oldest {
                Some(dir) => {
                    self.textures.remove(&dir);
                }
                None => break,
            }
        }
        uploaded
    }

    /// Doppelklick/Enter: Projekt oder Ordner → hinein; Dokument → wählen und öffnen
    /// (`open_node`: ein offenes wird nur nach vorn geholt). Das Fenster bleibt offen.
    pub fn activate(&mut self, ws: &mut Workspace, path: &NodePath) {
        let Some(Ok(scan)) = &self.data else { return };
        let Some(node) = find_node(&scan.tree, path) else {
            return;
        };
        match node.kind {
            NodeKind::Project | NodeKind::Folder => self.go_to(path.clone()),
            NodeKind::Document => {
                self.ui.select(Some(path.clone()));
                actions::open_node(ws, path);
            }
        }
    }

    /// Eintrag `entry` aus [`tile_menu`] für `tile`: öffnet das passende Fenster der Aktionen
    /// (andere offene Fenster der Aktionen schließen dabei).
    pub fn apply_menu(&mut self, tile: &Tile, entry: impl AsRef<str>) {
        let path = tile.path.clone();
        let entry = entry.as_ref();
        match entry {
            e if e == rename() => {
                self.ui.close_dialogs();
                actions::request_rename(&mut self.ui, path, &tile.name);
            }
            e if e == move_item() && tile.kind != TileKind::Project => {
                self.ui.close_dialogs();
                actions::request_move(&mut self.ui, path);
            }
            e if e == delete() => {
                self.ui.close_dialogs();
                actions::request_delete(&mut self.ui, path);
            }
            _ => {}
        }
    }

    /// Knopf „Neues Projekt“.
    pub fn new_project(&mut self) {
        self.ui.close_dialogs();
        actions::request_new_project(&mut self.ui, "");
    }

    /// Knopf „Neues Dokument“ in der aktuellen Ebene (nur in einem Projekt oder Ordner).
    pub fn new_document(&mut self) {
        if self.location.is_empty() {
            return;
        }
        self.ui.close_dialogs();
        actions::request_new_document(&mut self.ui, self.location.clone(), "");
    }

    /// Knopf „Neuer Ordner“ in der aktuellen Ebene (nur in einem Projekt oder Ordner).
    pub fn new_folder(&mut self) {
        if self.location.is_empty() {
            return;
        }
        self.ui.close_dialogs();
        actions::request_new_folder(&mut self.ui, self.location.clone(), "");
    }

    // ---- Zeichnen ----------------------------------------------------------------------------

    /// Zeichnet das Fenster (nur wenn offen); bei offener Rückfrage ganz gesperrt (Kacheln,
    /// Knöpfe, Pfad, „Aktualisieren“; nur ✕ schließt). Liest
    /// vorher den Stand ([`ProjectsWindow::update`]), damit Klicks dieses Bilds zu den
    /// gezeichneten Kacheln gehören, und holt fertige Miniaturen ab. Wartet nie. Danach die
    /// Fenster der Aktionen ([`actions::show_dialogs`]).
    pub fn show(&mut self, ctx: &egui::Context, ws: &mut Workspace) -> ProjectsEvent {
        if !self.open {
            self.last_rect = None;
            return ProjectsEvent::None;
        }
        let dark = ws.active_tab().state.dark;
        if dark != self.dark {
            self.dark = dark;
            self.rebuild_sources();
        }
        self.repaint = Some(ctx.clone());
        // vor dem Zeichnen: Klicks dieses Bilds gehören zu den Kacheln dieses Stands
        Self::update(self, ws);
        self.collect();
        self.frame_no += 1;
        if self.location != self.view_location {
            self.view += 1;
            self.view_location = self.location.clone();
        }
        let frame = Frame {
            locked: ws.prompt.is_some(),
            has_library: ws.library.is_some(),
            read_error: matches!(self.data, Some(Err(_))),
            root_text: ws
                .library
                .as_ref()
                .map(|l| display_path(l.root().path()))
                .unwrap_or_default(),
            grid_h: grid_height_for(ctx.content_rect().height()),
            alive: self.worker.as_ref().is_none_or(ThumbWorker::alive),
        };
        let footer = match &self.data {
            Some(Ok(_)) => footer_text_of(&self.tiles, self.location.is_empty(), self.total),
            _ => String::new(),
        };
        let mut acts = Vec::new();
        let (mut open, mut refresh) = (true, false);
        let response = Dialog::new(title(), DialogSize::File)
            .id(egui::Id::new(WINDOW_ID))
            .center_on_open()
            .open(&mut open)
            .show_with_footer(
                ctx,
                |ui| self.body(ui, &frame, &mut acts),
                // Bei offener Rückfrage ist das ganze Fenster gesperrt (wie früher das Feld):
                // gesperrte Knöpfe nehmen auch keinen Tastaturfokus und kein Enter an.
                |ui| {
                    ui.add_enabled_ui(!frame.locked, |ui| footer_row(ui, &footer, &mut refresh));
                },
            );
        self.last_rect = response.map(|r| r.response.rect);
        if self.manage_textures(ctx) {
            // neue Texturen erscheinen im nächsten Bild
            ctx.request_repaint();
        }
        let retry = self.sync_queue();
        let alive = self.worker.as_ref().is_some_and(ThumbWorker::alive);
        // eigene Aufträge: in Abständen nachsehen (nie in jedem Bild)
        if alive && self.outstanding() {
            ctx.request_repaint_after(THUMB_POLL);
        }
        // Thumb::Busy: erst zum erneuten Versuch wieder zeichnen
        if let Some(r) = retry {
            ctx.request_repaint_after(r);
        }
        let mut event = ProjectsEvent::None;
        for a in acts {
            match a {
                Action::Select(p) => self.ui.select(Some(p)),
                Action::Activate(p) => self.activate(ws, &p),
                Action::Menu(t, entry) => self.apply_menu(&t, entry),
                Action::Back => self.back(),
                Action::GoTo(p) => self.go_to(p),
                Action::NewProject => self.new_project(),
                Action::NewDocument => self.new_document(),
                Action::NewFolder => self.new_folder(),
                Action::Settings => event = ProjectsEvent::OpenSettings,
            }
        }
        if refresh {
            self.ui.invalidate();
        }
        if !open {
            self.ui.close_dialogs();
            self.close();
            return ProjectsEvent::Closed;
        }
        actions::show_dialogs(ctx, ws, &mut self.ui);
        event
    }

    /// Inhalt über der Fußzeile: Knopfzeile bzw. Pfad, dann der Kachelbereich fester Höhe.
    fn body(&mut self, ui: &mut egui::Ui, frame: &Frame, out: &mut Vec<Action>) {
        ui.add_enabled_ui(!frame.locked, |ui| self.top_row(ui, frame, out));
        let dark = ui.visuals().dark_mode;
        let p = theme::palette(dark);
        let area = egui::ScrollArea::vertical()
            .id_salt(("planora_projects_grid", self.view))
            .auto_shrink([false, false])
            .min_scrolled_height(frame.grid_h)
            .max_height(frame.grid_h);
        let mut drawn: Vec<(Rect, Option<PathBuf>)> = Vec::with_capacity(self.tiles.len());
        let mut focused = None;
        let tiles = &self.tiles;
        let sources = &self.sources;
        let slots = &self.slots;
        let textures = &mut self.textures;
        let frame_no = self.frame_no;
        let selected = self.ui.selected();
        let location_empty = self.location.is_empty();
        // Art der aktuellen Ebene (Projekt oder Ordner) für den Hinweis „noch leer“.
        let level_kind = match &self.data {
            Some(Ok(scan)) if !location_empty => {
                find_node(&scan.tree, &self.location).map(|n| n.kind)
            }
            _ => None,
        };
        let message = match (&self.data, frame.has_library) {
            (_, false) => Some((no_library(), p.text_primary)),
            (Some(Err(e)), true) => Some((format!("{}{e}", read_error_prefix()), p.error)),
            (None, true) => Some((reading(), p.text_tertiary)),
            (Some(Ok(_)), true) if tiles.is_empty() && location_empty => {
                Some((empty(), p.text_primary))
            }
            (Some(Ok(_)), true) if tiles.is_empty() => match level_kind {
                Some(NodeKind::Project) => Some((empty_project(), p.text_tertiary)),
                Some(_) => Some((empty_folder(), p.text_tertiary)),
                None => None,
            },
            _ => None,
        };
        let too_many = self.too_many;
        let output = theme::scroll_area(ui, area, |ui| {
            if let Some((text, color)) = message {
                ui.add(egui::Label::new(egui::RichText::new(text).color(color)).wrap());
            }
            if too_many {
                ui.label(
                    egui::RichText::new(crate::project_tiles::too_many()).color(p.text_secondary),
                );
            }
            ui.add_space(FOCUS_ROOM);
            ui.spacing_mut().item_spacing = vec2(TILE_GAP, TILE_GAP);
            let width = ui.available_width();
            let cols = columns_for(width);
            let tile_w = ((width - TILE_GAP * (cols - 1) as f32) / cols as f32).floor();
            for (r, row) in tiles.chunks(cols).enumerate() {
                ui.horizontal(|ui| {
                    for (c, tile) in row.iter().enumerate() {
                        let i = r * cols + c;
                        let sense = if frame.locked {
                            Sense::hover()
                        } else {
                            Sense::click()
                        };
                        let (rect, resp) = ui.allocate_exact_size(vec2(tile_w, TILE_H), sense);
                        let is_selected = selected == Some(&tile.path);
                        resp.widget_info(|| {
                            egui::WidgetInfo::selected(
                                egui::WidgetType::Button,
                                !frame.locked,
                                is_selected,
                                &tile.name,
                            )
                        });
                        let source = sources.get(i).and_then(Option::as_ref);
                        let slot =
                            source.and_then(|(dir, key)| slots.get(dir).filter(|s| s.key == *key));
                        let thumb = slot.and_then(|s| s.thumb.as_ref());
                        let no_bodies = matches!(thumb, Some(Thumb::NoBodies));
                        if ui.is_rect_visible(rect) {
                            let pic = picture(tile, source, thumb, textures, frame_no, frame.alive);
                            paint_tile(ui, rect, &resp, tile, &pic, is_selected, no_bodies);
                        }
                        if resp.gained_focus() {
                            // per Tab erreicht: in den sichtbaren Ausschnitt rollen
                            resp.scroll_to_me(None);
                        }
                        if resp.has_focus() {
                            focused = Some(i);
                        }
                        if !frame.locked {
                            if resp.clicked() {
                                out.push(Action::Select(tile.path.clone()));
                            }
                            let enter = resp.has_focus()
                                && ui.input(|inp| inp.key_pressed(egui::Key::Enter));
                            if resp.double_clicked() || enter {
                                out.push(Action::Activate(tile.path.clone()));
                            }
                            resp.context_menu(|ui| {
                                for entry in tile_menu(tile.kind) {
                                    if ui.button(&entry).clicked() {
                                        out.push(Action::Menu(tile.clone(), entry));
                                        ui.close();
                                    }
                                }
                            });
                        }
                        drawn.push((rect, source.map(|(d, _)| d.clone())));
                    }
                });
            }
            if !tiles.is_empty() {
                ui.add_space(FOCUS_ROOM);
            }
        });
        let viewport = output.inner_rect;
        let near = viewport.expand2(vec2(0.0, KEEP_MARGIN * viewport.height()));
        let dirs_in = |area: Rect| -> HashSet<PathBuf> {
            drawn
                .iter()
                .filter(|(r, _)| r.intersects(area))
                .filter_map(|(_, d)| d.clone())
                .collect()
        };
        let visible = dirs_in(viewport);
        self.keep = dirs_in(near);
        if visible != self.visible {
            self.visible = visible;
            self.queue_dirty = true;
        }
        self.tile_rects = drawn.into_iter().map(|(r, _)| r).collect();
        self.grid_rect = Some(viewport);
        self.focused = focused;
    }

    /// Zwei Zeilen über dem Kachelbereich, auf jeder Ebene gleich hoch (das Fenster springt beim
    /// Wechsel nicht). Erste Zeile: auf Ebene 1 der Pfad des Projektverzeichnisses, tiefer der
    /// Pfad „‹ Projekte › P › F“; rechts „Projektverzeichnis …“ (Ebene 1 und bei einem
    /// Lesefehler). Zweite Zeile: „Neues Projekt“ bzw. „Neues Dokument“ und „Neuer Ordner“.
    fn top_row(&self, ui: &mut egui::Ui, frame: &Frame, out: &mut Vec<Action>) {
        let dark = ui.visuals().dark_mode;
        let p = theme::palette(dark);
        let row_h = ui.spacing().interact_size.y;
        let level_one = self.location.is_empty();
        ui.horizontal(|ui| {
            ui.set_min_height(row_h);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if level_one || frame.read_error {
                    let r = ui.button(directory_button());
                    theme::focus_ring(ui, &r);
                    if r.clicked() {
                        out.push(Action::Settings);
                    }
                }
                ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                    if level_one {
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(&frame.root_text).color(p.text_secondary),
                            )
                            .truncate(),
                        );
                    } else {
                        self.breadcrumb(ui, out);
                    }
                });
            });
        });
        ui.horizontal(|ui| {
            ui.set_min_height(row_h);
            if level_one {
                let r = ui.add_enabled(
                    frame.has_library,
                    theme::primary_button(&new_project(), dark),
                );
                theme::focus_ring(ui, &r);
                if r.clicked() {
                    out.push(Action::NewProject);
                }
            } else {
                let r = ui.add(theme::primary_button(&new_document(), dark));
                theme::focus_ring(ui, &r);
                if r.clicked() {
                    out.push(Action::NewDocument);
                }
                let r = ui.button(new_folder());
                theme::focus_ring(ui, &r);
                if r.clicked() {
                    out.push(Action::NewFolder);
                }
            }
        });
    }

    /// Pfad „‹ Projekte › P › F“: der Pfeil geht eine Ebene hinauf, jeder Teil zu seiner Ebene.
    fn breadcrumb(&self, ui: &mut egui::Ui, out: &mut Vec<Action>) {
        let p = theme::palette(ui.visuals().dark_mode);
        theme::frameless(ui.style_mut());
        ui.spacing_mut().item_spacing.x = theme::SPACE_1;
        let r = ui
            .button(icons::text(
                Icon::Back,
                icons::LABEL_ICON_SIZE,
                Color32::PLACEHOLDER,
            ))
            .on_hover_text(back_hint());
        // Der Knopf trägt nur ein Symbol; für Bildschirmleser heißt er wie sein Hinweis.
        r.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, back_hint()));
        theme::focus_ring(ui, &r);
        if r.clicked() {
            out.push(Action::Back);
        }
        let r = ui.button(title());
        theme::focus_ring(ui, &r);
        if r.clicked() {
            out.push(Action::GoTo(NodePath::new()));
        }
        let separator = |ui: &mut egui::Ui| {
            ui.label(icons::text(
                Icon::PathSeparator,
                icons::LABEL_ICON_SIZE,
                p.text_tertiary,
            ));
        };
        let hidden = self.crumbs_hidden(ui);
        if hidden > 0 {
            // die vorderen Teile als „…“, der ganze Pfad als Hinweis
            separator(ui);
            let first = title();
            let full = std::iter::once(first.as_str())
                .chain(self.location.iter().map(String::as_str))
                .collect::<Vec<_>>()
                .join(" / ");
            ui.label(egui::RichText::new(CRUMB_ELLIPSIS).color(p.text_secondary))
                .on_hover_text(full);
        }
        for i in hidden..self.location.len() {
            separator(ui);
            // der letzte Teil steht immer da, notfalls mit „…“ gekürzt
            let r = ui.add(egui::Button::new(self.location[i].as_str()).truncate());
            theme::focus_ring(ui, &r);
            if r.clicked() {
                out.push(Action::GoTo(self.location[..=i].to_vec()));
            }
        }
    }

    /// Wie viele vordere Pfadteile als „…“ zusammenfallen, damit der Rest in die Zeile passt
    /// (höchstens alle bis auf den letzten). Gemessen mit Schrift und Abständen der Knöpfe.
    fn crumbs_hidden(&self, ui: &egui::Ui) -> usize {
        let n = self.location.len();
        let font = egui::TextStyle::Button.resolve(ui.style());
        let pad = 2.0 * ui.spacing().button_padding.x;
        let gap = ui.spacing().item_spacing.x;
        let text_w = |t: &str| {
            ui.painter()
                .layout_no_wrap(t.to_owned(), font.clone(), Color32::PLACEHOLDER)
                .size()
                .x
        };
        let separator = icons::LABEL_ICON_SIZE + gap;
        let part = |t: &str| separator + text_w(t) + pad + gap;
        let ellipsis = separator + text_w(CRUMB_ELLIPSIS) + gap;
        let avail = ui.available_width();
        let mut hidden = 0;
        loop {
            let shown: f32 = self.location[hidden..].iter().map(|t| part(t)).sum();
            let wanted = shown + if hidden > 0 { ellipsis } else { 0.0 };
            if wanted <= avail || hidden + 1 >= n {
                return hidden;
            }
            hidden += 1;
        }
    }
}

/// Fußzeile: links die Anzahl, rechts der Hinweis und „Aktualisieren“.
fn footer_row(ui: &mut egui::Ui, footer: &str, refresh: &mut bool) {
    let p = theme::palette(ui.visuals().dark_mode);
    ui.horizontal(|ui| {
        ui.set_min_height(ui.spacing().interact_size.y);
        ui.label(egui::RichText::new(footer).color(p.text_secondary));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let r = ui.button(self::refresh());
            theme::focus_ring(ui, &r);
            if r.clicked() {
                *refresh = true;
            }
            ui.add(
                egui::Label::new(egui::RichText::new(footer_hint()).color(p.text_tertiary))
                    .truncate(),
            );
        });
    });
}

/// Inhalt der Bildfläche. Die Textur einer Miniatur legt [`ProjectsWindow::manage_textures`]
/// an; bis dahin (und solange sie entsteht) steht der ruhige Hinweis [`building_text`].
fn picture(
    tile: &Tile,
    source: Option<&(PathBuf, ThumbKey)>,
    thumb: Option<&Thumb>,
    textures: &mut HashMap<PathBuf, Tex>,
    frame_no: u64,
    alive: bool,
) -> Picture {
    let placeholder = |caption| Picture::Placeholder { caption };
    if tile.kind == TileKind::Folder {
        return Picture::Folder;
    }
    let Some((dir, key)) = source else {
        // ohne lesbares Dokument: leeres Projekt oder nicht lesbar
        return placeholder((tile.documents == 0).then(no_bodies));
    };
    let building = alive.then(building_text);
    match thumb {
        Some(Thumb::Image(_)) => match textures.get_mut(dir).filter(|t| t.key == *key) {
            Some(t) => {
                t.used = frame_no;
                Picture::Texture(t.handle.id())
            }
            None => placeholder(building),
        },
        Some(Thumb::NoBodies) => placeholder(Some(no_bodies())),
        Some(Thumb::TooComplex) => placeholder(Some(too_complex())),
        Some(Thumb::Impossible) => placeholder(Some(impossible_text())),
        Some(Thumb::Failed(_)) => placeholder(None),
        Some(Thumb::Busy) | None => placeholder(building),
    }
}

/// Eine Zeile Text, zu lang mit „…“ gekürzt.
fn line(
    ui: &egui::Ui,
    text: &str,
    font: FontId,
    color: Color32,
    width: f32,
) -> std::sync::Arc<egui::Galley> {
    let mut job = LayoutJob::simple_singleline(text.to_owned(), font, color);
    job.wrap = TextWrapping::truncate_at_width(width);
    ui.painter().layout_job(job)
}

/// Größe eines Bilds der Größe `image`, eingepasst in `area` (Seitenverhältnis bleibt).
fn fit(image: egui::Vec2, area: egui::Vec2) -> egui::Vec2 {
    image * (area.x / image.x).min(area.y / image.y)
}

/// Malt eine Kachel: Fläche (gewählt `accent_subtle`, sonst `bg_section`), Rand innen (gewählt
/// 2 px Akzent, sonst 1 px; überfahren `text_tertiary`, in Ruhe `border_subtle`), Bildfläche,
/// Name, „Geändert …“ und Version/Anzahl (Warnfarbe bei `Tone::Warning`).
fn paint_tile(
    ui: &egui::Ui,
    rect: Rect,
    resp: &egui::Response,
    tile: &Tile,
    pic: &Picture,
    selected: bool,
    no_bodies: bool,
) {
    let dark = ui.visuals().dark_mode;
    let p = theme::palette(dark);
    let painter = ui.painter();
    let border = if selected {
        p.accent
    } else if resp.hovered() {
        p.text_tertiary
    } else {
        p.border_subtle
    };
    let radius = egui::CornerRadius::same(theme::RADIUS_MD);
    painter.rect(
        rect,
        radius,
        theme::card_fill(dark, selected),
        Stroke::new(
            if selected {
                SELECTED_BORDER
            } else {
                TILE_BORDER
            },
            border,
        ),
        StrokeKind::Inside,
    );
    if resp.has_focus() {
        // Fokus: eigener Ring außen, mit Abstand zum Rand (die Auswahl ist der Rand selbst)
        painter.rect_stroke(
            rect.expand(FOCUS_OFFSET),
            egui::CornerRadius::same(theme::RADIUS_MD + FOCUS_OFFSET as u8),
            Stroke::new(FOCUS_WIDTH, p.accent),
            StrokeKind::Middle,
        );
    }
    let inner = rect.shrink(TILE_PAD);
    let image = Rect::from_min_size(inner.min, vec2(inner.width(), IMAGE_H));
    painter.rect_filled(
        image,
        egui::CornerRadius::same(theme::RADIUS_SM),
        p.bg_inset,
    );
    match pic {
        Picture::Folder => {
            painter.text(
                image.center(),
                Align2::CENTER_CENTER,
                icons::glyph(Icon::Folder).to_string(),
                FontId::new(FOLDER_ICON_SIZE, fonts::icons()),
                theme::folder_icon_color(dark),
            );
        }
        Picture::Texture(id) => {
            let size = fit(vec2(THUMB_W as f32, THUMB_H as f32), image.size());
            egui::Image::new((*id, size))
                .paint_at(ui, Rect::from_center_size(image.center(), size));
        }
        Picture::Placeholder { caption } => {
            let lifted = caption.is_some();
            let center = image.center() - vec2(0.0, if lifted { theme::SPACE_3 } else { 0.0 });
            painter.text(
                center,
                Align2::CENTER_CENTER,
                icons::glyph(Icon::Box).to_string(),
                FontId::new(CUBE_ICON_SIZE, fonts::icons()),
                p.text_muted,
            );
            let below = center.y + CUBE_ICON_SIZE / 2.0 + theme::SPACE_1;
            if let Some(c) = caption {
                painter.text(
                    pos2(image.center().x, below),
                    Align2::CENTER_TOP,
                    c.as_str(),
                    FontId::new(CAPTION_SIZE, fonts::medium()),
                    p.text_tertiary,
                );
            }
        }
    }
    let width = inner.width();
    let mut y = image.bottom() + theme::SPACE_2;
    let name = line(
        ui,
        &tile.name,
        FontId::new(NAME_SIZE, fonts::semibold()),
        p.text_primary,
        width,
    );
    painter.galley(pos2(inner.left(), y), name, p.text_primary);
    y += NAME_LINE;
    let small = FontId::new(SMALL_SIZE, egui::FontFamily::Proportional);
    let changed = changed_line(tile);
    if !changed.is_empty() {
        let g = line(ui, &changed, small.clone(), p.text_secondary, width);
        painter.galley(pos2(inner.left(), y), g, p.text_secondary);
    }
    y += SMALL_LINE;
    let (meta, tone) = meta_line(tile, no_bodies);
    let color = match tone {
        Tone::Warning => p.warning,
        Tone::Normal => p.text_secondary,
    };
    let g = line(ui, &meta, small, color, width);
    painter.galley(pos2(inner.left(), y), g, color);
}
