//! Fenster „Projekte“ (Spec §4): Zustand und Navigation. Das Fenster liest das
//! Projektverzeichnis im Hintergrund ([`scan_library`] in einem [`Job`]); bis ein neues Ergebnis
//! da ist, bleiben die bisherigen Kacheln stehen. Die Kacheln einer Ebene werden nur berechnet,
//! wenn ein neues Ergebnis kommt oder der Ort wechselt, nie in jedem Bild. Aktionen und ihre
//! Fenster kommen aus [`crate::project_actions`].

use crate::jobs::{Job, Poll};
use crate::project_actions::{self as actions, ProjectsUi};
use crate::project_tiles::{Scan, Tile, TileKind, scan_library_with, tiles_at};
use crate::workspace::Workspace;
use planora_project::library::{NodeKind, NodePath, TreeNode};
use std::path::PathBuf;
use std::time::{Duration, Instant};

pub const TITLE: &str = "Projekte";
pub const RENAME: &str = "Umbenennen …";
pub const MOVE: &str = "Verschieben …";
pub const DELETE: &str = "Löschen …";
/// Meldung, wenn das Lesen ohne Ergebnis endete.
pub const SCAN_LOST: &str = "Das Lesen der Projekte wurde unerwartet beendet.";
/// Längstes Warten in [`ProjectsWindow::wait_for_scan`].
const SCAN_WAIT_LIMIT: Duration = Duration::from_secs(60);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectsEvent {
    None,
    OpenSettings,
    Closed,
}

/// Einträge des Rechtsklick-Menüs einer Kachel; Projekte lassen sich nicht verschieben.
pub fn tile_menu(kind: TileKind) -> &'static [&'static str] {
    match kind {
        TileKind::Project => &[RENAME, DELETE],
        TileKind::Folder | TileKind::Document => &[RENAME, MOVE, DELETE],
    }
}

/// Laufendes Lesen mit dem Stand, zu dem es begann.
struct ScanJob {
    job: Job<Result<Scan, String>>,
    revision: u64,
    generation: u64,
}

#[derive(Default)]
pub struct ProjectsWindow {
    /// Auswahl, Baum und Fenster der Aktionen.
    pub ui: ProjectsUi,
    open: bool,
    /// Aktuelle Ebene (leer = Projekte).
    location: NodePath,
    job: Option<ScanJob>,
    /// Letztes Ergebnis des Lesens.
    data: Option<Result<Scan, String>>,
    /// Kacheln der aktuellen Ebene aus `data`.
    tiles: Vec<Tile>,
    /// Zuletzt gesehenes Projektverzeichnis (`None` = noch nie hingesehen).
    root_seen: Option<Option<PathBuf>>,
    /// Pause nach jedem gelesenen Dokument (Tests: langsames Lesen).
    scan_delay: Duration,
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

impl ProjectsWindow {
    /// Öffnet das Fenster; der Stand wird neu gelesen.
    pub fn open(&mut self) {
        self.open = true;
        self.ui.invalidate();
    }

    /// Schließt das Fenster; ein laufendes Lesen wird abgebrochen.
    pub fn close(&mut self) {
        self.open = false;
        self.job = None;
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
        let Some(Ok(scan)) = &self.data else {
            self.tiles.clear();
            return;
        };
        loop {
            if let Some(tiles) = tiles_at(scan, &self.location) {
                self.tiles = tiles;
                return;
            }
            self.location.pop();
        }
    }

    /// Pause nach jedem gelesenen Dokument (nur für Tests).
    #[doc(hidden)]
    pub fn set_scan_delay(&mut self, per_document: Duration) {
        self.scan_delay = per_document;
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
            self.tiles.clear();
        }
        self.root_seen = Some(root.clone());
        self.ui.observe(ws);
        let Some(root) = root else {
            self.job = None;
            self.data = None;
            self.tiles.clear();
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
            // ein überholtes Lesen bricht beim Fallenlassen ab
            self.job = Some(ScanJob {
                job: Job::spawn(move |progress| {
                    scan_library_with(&root, progress, |_| {
                        if !delay.is_zero() {
                            std::thread::sleep(delay);
                        }
                    })
                }),
                revision: current.0,
                generation: current.1,
            });
        }
        let Some(j) = &self.job else { return };
        let result = match j.job.poll() {
            Poll::Pending => return,
            Poll::Ready(r) => r,
            Poll::Lost => Err(SCAN_LOST.to_string()),
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
    pub fn apply_menu(&mut self, tile: &Tile, entry: &str) {
        let path = tile.path.clone();
        match entry {
            RENAME => {
                self.ui.close_dialogs();
                actions::request_rename(&mut self.ui, path, &tile.name);
            }
            MOVE if tile.kind != TileKind::Project => {
                self.ui.close_dialogs();
                actions::request_move(&mut self.ui, path);
            }
            DELETE => {
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
}
