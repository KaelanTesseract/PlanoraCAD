//! Kachelmodell des Fensters „Projekte“ (ohne Oberfläche): liest den Baum der Bibliothek und je
//! Dokument nur `dokument.json` (`docstore::peek`), zählt Dokumente rekursiv und sucht das
//! zuletzt geänderte lesbare Dokument je Kachel. Das Lesen läuft im Hintergrund und lässt sich
//! über `Progress` abbrechen; beschädigte Dokumente werden gezählt, aber nie für Datum oder
//! Version benutzt.

use crate::local_time;
use planora_exchange::Progress;
use planora_i18n::tr;
use planora_project::docstore::{DocumentInfo, peek};
use planora_project::library::{Library, NodeKind, NodePath, TreeNode};
use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Höchstens so viele Kacheln je Ebene.
pub const MAX_TILES: usize = 500;
pub fn too_many() -> String {
    tr!("projects-too-many")
}
pub fn cancelled() -> String {
    ScanError::Cancelled.text()
}

/// Warum das Lesen der Bibliothek nichts ergab. Gespeichert wird der Grund, nicht der Text:
/// gezeichnet wird in der Sprache des Bildes ([`ScanError::text`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ScanError {
    /// Das Lesen wurde abgebrochen.
    Cancelled,
    /// Das Lesen endete unerwartet ohne Ergebnis.
    Lost,
    /// Das Projektverzeichnis (der Pfad) gibt es nicht.
    RootMissing(String),
    /// Fertiger Text der Bibliothek (nicht übersetzt).
    Raw(String),
}

impl ScanError {
    /// Der Text in der aktuellen Sprache.
    pub fn text(&self) -> String {
        match self {
            ScanError::Cancelled => tr!("projects-cancelled"),
            ScanError::Lost => tr!("projects-scan-lost"),
            ScanError::RootMissing(path) => tr!("projects-root-missing", path = path.as_str()),
            ScanError::Raw(text) => text.clone(),
        }
    }
}

impl std::fmt::Display for ScanError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.text())
    }
}

/// Angaben eines lesbaren Dokuments.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DocFacts {
    /// Spitze des aktiven Zweigs.
    pub version: u32,
    /// Name des aktiven Zweigs.
    pub branch: String,
    /// Zeit der jüngsten Version (ISO, UTC).
    pub changed: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DocState {
    Read(DocFacts),
    Unreadable(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DocEntry {
    pub dir: PathBuf,
    pub state: DocState,
}

/// Ergebnis des Lesens: Baum und Angaben je Dokument.
#[derive(Clone, Debug)]
pub struct Scan {
    pub root: PathBuf,
    pub tree: Vec<TreeNode>,
    pub docs: BTreeMap<NodePath, DocEntry>,
    /// Kennung (`dokument.json`) je lesbarem Dokument, für den Schlüssel der Miniatur (leer bei
    /// [`scan_tree`]).
    pub ids: BTreeMap<NodePath, String>,
}

/// Baum + `peek` je Dokument; fehlt `root`, Fehler (nie anlegen); Abbruch → Err(CANCELLED).
pub fn scan_library(root: &Path, progress: &Progress) -> Result<Scan, ScanError> {
    scan_library_with(root, progress, |_| {})
}

/// Wie [`scan_library`]; `after_document` läuft nach jedem gelesenen Dokument (nachdem der
/// Fortschritt weitergezählt wurde). Prüfhaken für Tests, etwa für einen Abbruch mittendrin.
pub fn scan_library_with(
    root: &Path,
    progress: &Progress,
    mut after_document: impl FnMut(&NodePath),
) -> Result<Scan, ScanError> {
    // `Library::open` legt einen fehlenden Ordner an: vorher prüfen.
    if !root.is_dir() {
        return Err(ScanError::RootMissing(root.display().to_string()));
    }
    if stop(progress) {
        return Err(ScanError::Cancelled);
    }
    let lib = Library::open(root).map_err(|e| ScanError::Raw(e.to_string()))?;
    let tree = lib.tree().map_err(|e| ScanError::Raw(e.to_string()))?;
    let mut count: u64 = 0;
    walk_documents(&tree, &mut |_| count += 1);
    progress.set_total(count);
    let mut cancelled = false;
    let mut docs = BTreeMap::new();
    let mut ids = BTreeMap::new();
    walk_documents(&tree, &mut |node| {
        if cancelled || stop(progress) {
            cancelled = true;
            return;
        }
        let entry = match lib.document_dir(&node.path) {
            Ok(dir) => {
                let (entry, id) = read_entry(&dir);
                if let Some(id) = id {
                    ids.insert(node.path.clone(), id);
                }
                entry
            }
            Err(e) => DocEntry {
                dir: node.path.iter().fold(root.to_path_buf(), |d, p| d.join(p)),
                state: DocState::Unreadable(e.to_string()),
            },
        };
        docs.insert(node.path.clone(), entry);
        progress.advance(1);
        after_document(&node.path);
    });
    if cancelled {
        return Err(ScanError::Cancelled);
    }
    Ok(Scan {
        root: root.to_path_buf(),
        tree,
        docs,
        ids,
    })
}

/// Soll das Lesen abbrechen?
fn stop(progress: &Progress) -> bool {
    progress.is_cancelled()
}

/// Wie `scan_library` mit vorgegebenem Baum und Lesefunktion (Tests).
pub fn scan_tree(
    root: PathBuf,
    tree: Vec<TreeNode>,
    mut read: impl FnMut(&NodePath) -> DocEntry,
) -> Scan {
    let mut docs = BTreeMap::new();
    walk_documents(&tree, &mut |node| {
        docs.insert(node.path.clone(), read(&node.path));
    });
    Scan {
        root,
        tree,
        docs,
        ids: BTreeMap::new(),
    }
}

/// Ruft `f` für jedes Dokument im Baum (in Baumreihenfolge).
fn walk_documents<'a>(nodes: &'a [TreeNode], f: &mut impl FnMut(&'a TreeNode)) {
    for n in nodes {
        if n.kind == NodeKind::Document {
            f(n);
        } else {
            walk_documents(&n.children, f);
        }
    }
}

/// Spitze des aktiven Zweigs, Zweigname und Zeit **dieser** Version (Datum und Version
/// beschreiben immer dieselbe Version); ohne Spitze oder mit unmöglicher Zeit → None.
pub fn facts(info: &DocumentInfo) -> Option<DocFacts> {
    tip_facts(info).ok()
}

/// Wie [`facts`], mit dem Grund, warum es keine Angaben gibt.
fn tip_facts(info: &DocumentInfo) -> Result<DocFacts, String> {
    let no_tip = || "Der aktive Zweig hat keine Version.".to_string();
    let version = info.tip(&info.active_branch).ok_or_else(no_tip)?;
    let time = &info.version(version).ok_or_else(no_tip)?.time;
    // `peek` prüft nur die Form der Zeit; „9999-99-99T99:99:99Z“ darf weder datieren noch
    // beim Vergleich „zuletzt geändert“ gewinnen.
    if local_time::format_offset(time, 0).is_none() {
        return Err(format!("Die Zeitangabe „{time}“ ist ungültig."));
    }
    Ok(DocFacts {
        version,
        branch: info.active_branch.clone(),
        changed: time.clone(),
    })
}

/// Liest `dokument.json` eines Dokumentordners; peek-Fehler → `Unreadable(e.to_string())`,
/// ebenso ohne Spitze oder mit unmöglicher Zeit der Spitze.
pub fn doc_entry(dir: &Path) -> DocEntry {
    read_entry(dir).0
}

/// Wie [`doc_entry`], dazu die Kennung des Dokuments, wenn es lesbar ist.
fn read_entry(dir: &Path) -> (DocEntry, Option<String>) {
    let (state, id) = match peek(dir) {
        Ok((info, _)) => match tip_facts(&info) {
            Ok(f) => (DocState::Read(f), Some(info.id)),
            Err(e) => (DocState::Unreadable(e), None),
        },
        Err(e) => (DocState::Unreadable(e.to_string()), None),
    };
    let entry = DocEntry {
        dir: dir.to_path_buf(),
        state,
    };
    (entry, id)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TileKind {
    Project,
    Folder,
    Document,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Tile {
    pub kind: TileKind,
    pub name: String,
    pub path: NodePath,
    pub documents: usize,
    pub unreadable: usize,
    /// Zuletzt geändertes lesbares Dokument darunter (bei Dokumentkacheln es selbst).
    pub newest: Option<(NodePath, DocFacts)>,
}

/// Kacheln der Ebene `at` (leer = Projekte); `None`, wenn es `at` nicht mehr gibt. Reihenfolge
/// wie der Baum (Ordner und Dokumente gemischt nach Namen, wie `Library::tree`).
pub fn tiles_at(scan: &Scan, at: &NodePath) -> Option<Vec<Tile>> {
    let mut level = &scan.tree;
    for depth in 0..at.len() {
        let node = level.iter().find(|n| n.path[..] == at[..=depth])?;
        if node.kind == NodeKind::Document {
            return None;
        }
        level = &node.children;
    }
    Some(
        level
            .iter()
            .take(MAX_TILES)
            .map(|n| tile_of(scan, n))
            .collect(),
    )
}

/// Ein lesbares Dokument als Kandidat für „zuletzt geändert“.
struct Candidate<'a> {
    name: String,
    path: &'a NodePath,
    facts: &'a DocFacts,
}

/// Ist `a` neuer als `b`? Größere Zeit; bei Gleichstand kleinerer Name ohne
/// Groß-/Kleinschreibung, dann kleinerer Pfad (P3).
fn newer(a: &Candidate, b: &Candidate) -> bool {
    let order = a
        .facts
        .changed
        .cmp(&b.facts.changed)
        .then_with(|| b.name.cmp(&a.name))
        .then_with(|| b.path.cmp(a.path));
    order == Ordering::Greater
}

fn tile_of<'a>(scan: &'a Scan, node: &'a TreeNode) -> Tile {
    let mut documents = 0;
    let mut unreadable = 0;
    let mut best: Option<Candidate<'a>> = None;
    let mut visit = |n: &'a TreeNode| {
        documents += 1;
        let facts = match scan.docs.get(&n.path).map(|e| &e.state) {
            Some(DocState::Read(f)) => Some(f),
            _ => None,
        };
        let Some(facts) = facts else {
            unreadable += 1;
            return;
        };
        let c = Candidate {
            name: n.name.to_lowercase(),
            path: &n.path,
            facts,
        };
        if best.as_ref().is_none_or(|b| newer(&c, b)) {
            best = Some(c);
        }
    };
    if node.kind == NodeKind::Document {
        visit(node);
    } else {
        walk_documents(&node.children, &mut visit);
    }
    Tile {
        kind: match node.kind {
            NodeKind::Project => TileKind::Project,
            NodeKind::Folder => TileKind::Folder,
            NodeKind::Document => TileKind::Document,
        },
        name: node.name.clone(),
        path: node.path.clone(),
        documents,
        unreadable,
        newest: best.map(|c| (c.path.clone(), c.facts.clone())),
    }
}

/// „23.10.2026, 18:42“ aus „2026-10-23T16:42:07Z“ (P1: Ortszeit über `local_time`); unlesbar → None.
pub fn format_changed(iso: &str) -> Option<String> {
    local_time::format_offset(iso, 0)?;
    Some(local_time::local_time_text(iso))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    Normal,
    Warning,
}

/// „Geändert 23.10.2026, 16:42“ | „Datum unbekannt“ | "" (kein lesbares Dokument).
pub fn changed_line(tile: &Tile) -> String {
    match &tile.newest {
        None => String::new(),
        Some((_, f)) => match format_changed(&f.changed) {
            Some(t) => tr!("projects-changed", time = t),
            None => tr!("projects-date-unknown"),
        },
    }
}

/// Zweite Textzeile einer Kachel (Version und Anzahl) mit ihrem Ton.
pub fn meta_line(tile: &Tile, no_bodies: bool) -> (String, Tone) {
    let normal = |s: String| (s, Tone::Normal);
    let version = tile.newest.as_ref().map(|(_, f)| f.version);
    match tile.kind {
        TileKind::Document => match version {
            Some(v) if tile.unreadable == 0 => normal(tr!("projects-version", v = v)),
            _ => (tr!("projects-document-unreadable"), Tone::Warning),
        },
        TileKind::Project | TileKind::Folder => {
            if tile.documents == 0 {
                return normal(tr!("projects-no-document"));
            }
            if tile.unreadable >= tile.documents {
                return (tr!("projects-documents-unreadable"), Tone::Warning);
            }
            let count = documents_text(tile.documents);
            match (tile.kind, version) {
                (TileKind::Project, _) if no_bodies => {
                    normal(tr!("projects-meta-no-bodies", count = count))
                }
                (TileKind::Project, Some(v)) => {
                    normal(tr!("projects-meta-version", v = v, count = count))
                }
                _ => normal(count),
            }
        }
    }
}

/// „1 Dokument“ / „{n} Dokumente“.
pub fn documents_text(n: usize) -> String {
    tr!("projects-documents", n = n)
}

/// Wie [`footer_text`]; zeigt die Ebene nur einen Teil ihrer `total` Einträge (über
/// [`MAX_TILES`]), heißt es „500 von 600 Projekten“ bzw. „500 von 600 Einträgen“.
pub fn footer_text_of(tiles: &[Tile], top_level: bool, total: usize) -> String {
    if total <= tiles.len() {
        return footer_text(tiles, top_level);
    }
    // `total` ist hier größer als eins: immer Mehrzahl
    if top_level {
        tr!(
            "projects-footer-partial-top",
            shown = tiles.len(),
            total = total
        )
    } else {
        tr!(
            "projects-footer-partial-entries",
            shown = tiles.len(),
            total = total
        )
    }
}

/// Fußzeile: oben die Zahl der Projekte, darunter Ordner und Dokumente der Ebene.
pub fn footer_text(tiles: &[Tile], top_level: bool) -> String {
    if top_level {
        return tr!("projects-count", n = tiles.len());
    }
    let folders = tiles.iter().filter(|t| t.kind == TileKind::Folder).count();
    let docs = tiles
        .iter()
        .filter(|t| t.kind == TileKind::Document)
        .count();
    let mut parts = Vec::new();
    if folders > 0 {
        parts.push(tr!("projects-folders", n = folders));
    }
    if docs > 0 {
        parts.push(documents_text(docs));
    }
    if parts.is_empty() {
        tr!("projects-empty-level")
    } else {
        parts.join(" · ")
    }
}
