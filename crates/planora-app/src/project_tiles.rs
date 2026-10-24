//! Kachelmodell des Fensters „Projekte“ (ohne Oberfläche): liest den Baum der Bibliothek und je
//! Dokument nur `dokument.json` (`docstore::peek`), zählt Dokumente rekursiv und sucht das
//! zuletzt geänderte lesbare Dokument je Kachel. Das Lesen läuft im Hintergrund und lässt sich
//! über `Progress` abbrechen; beschädigte Dokumente werden gezählt, aber nie für Datum oder
//! Version benutzt.

use crate::local_time;
use planora_exchange::Progress;
use planora_project::docstore::{DocumentInfo, peek};
use planora_project::library::{Library, NodeKind, NodePath, TreeNode};
use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Höchstens so viele Kacheln je Ebene.
pub const MAX_TILES: usize = 500;
pub const TOO_MANY: &str = "Es werden nur die ersten 500 gezeigt.";
pub const CANCELLED: &str = "Abgebrochen";

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
}

/// Baum + `peek` je Dokument; fehlt `root`, Fehler (nie anlegen); Abbruch → Err(CANCELLED).
pub fn scan_library(root: &Path, progress: &Progress) -> Result<Scan, String> {
    // `Library::open` legt einen fehlenden Ordner an: vorher prüfen.
    if !root.is_dir() {
        return Err(format!("„{}“ wurde nicht gefunden.", root.display()));
    }
    if stop(progress) {
        return Err(CANCELLED.into());
    }
    let lib = Library::open(root).map_err(|e| e.to_string())?;
    let tree = lib.tree().map_err(|e| e.to_string())?;
    let mut count: u64 = 0;
    walk_documents(&tree, &mut |_| count += 1);
    progress.set_total(count);
    let mut cancelled = false;
    let mut docs = BTreeMap::new();
    walk_documents(&tree, &mut |node| {
        if cancelled || stop(progress) {
            cancelled = true;
            return;
        }
        let entry = match lib.document_dir(&node.path) {
            Ok(dir) => doc_entry(&dir),
            Err(e) => DocEntry {
                dir: node.path.iter().fold(root.to_path_buf(), |d, p| d.join(p)),
                state: DocState::Unreadable(e.to_string()),
            },
        };
        docs.insert(node.path.clone(), entry);
        progress.advance(1);
    });
    if cancelled {
        return Err(CANCELLED.into());
    }
    Ok(Scan {
        root: root.to_path_buf(),
        tree,
        docs,
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
    Scan { root, tree, docs }
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

/// Spitze des aktiven Zweigs, Zweigname, `latest_time()`; ohne Spitze → None.
pub fn facts(info: &DocumentInfo) -> Option<DocFacts> {
    let version = info.tip(&info.active_branch)?;
    Some(DocFacts {
        version,
        branch: info.active_branch.clone(),
        changed: info.latest_time().to_string(),
    })
}

/// Liest `dokument.json` eines Dokumentordners; peek-Fehler → `Unreadable(e.to_string())`.
pub fn doc_entry(dir: &Path) -> DocEntry {
    let state = match peek(dir) {
        Ok((info, _)) => match facts(&info) {
            Some(f) => DocState::Read(f),
            None => DocState::Unreadable("Der aktive Zweig hat keine Version.".into()),
        },
        Err(e) => DocState::Unreadable(e.to_string()),
    };
    DocEntry {
        dir: dir.to_path_buf(),
        state,
    }
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
            Some(t) => format!("Geändert {t}"),
            None => "Datum unbekannt".into(),
        },
    }
}

/// Zweite Textzeile einer Kachel (Version und Anzahl) mit ihrem Ton.
pub fn meta_line(tile: &Tile, no_bodies: bool) -> (String, Tone) {
    let normal = |s: String| (s, Tone::Normal);
    let version = tile.newest.as_ref().map(|(_, f)| f.version);
    match tile.kind {
        TileKind::Document => match version {
            Some(v) if tile.unreadable == 0 => normal(format!("Version {v}")),
            _ => ("Dokument nicht lesbar".into(), Tone::Warning),
        },
        TileKind::Project | TileKind::Folder => {
            if tile.documents == 0 {
                return normal("Noch kein Dokument".into());
            }
            if tile.unreadable >= tile.documents {
                return ("Dokumente nicht lesbar".into(), Tone::Warning);
            }
            let count = documents_text(tile.documents);
            match (tile.kind, version) {
                (TileKind::Project, _) if no_bodies => {
                    normal(format!("Noch keine Körper · {count}"))
                }
                (TileKind::Project, Some(v)) => normal(format!("Version {v} · {count}")),
                _ => normal(count),
            }
        }
    }
}

/// „1 Dokument“ / „{n} Dokumente“.
pub fn documents_text(n: usize) -> String {
    if n == 1 {
        "1 Dokument".into()
    } else {
        format!("{n} Dokumente")
    }
}

/// Fußzeile: oben die Zahl der Projekte, darunter Ordner und Dokumente der Ebene.
pub fn footer_text(tiles: &[Tile], top_level: bool) -> String {
    if top_level {
        return match tiles.len() {
            0 => "Keine Projekte".into(),
            1 => "1 Projekt".into(),
            n => format!("{n} Projekte"),
        };
    }
    let folders = tiles.iter().filter(|t| t.kind == TileKind::Folder).count();
    let docs = tiles
        .iter()
        .filter(|t| t.kind == TileKind::Document)
        .count();
    let mut parts = Vec::new();
    if folders > 0 {
        parts.push(format!("{folders} Ordner"));
    }
    if docs > 0 {
        parts.push(documents_text(docs));
    }
    if parts.is_empty() {
        "Leer".into()
    } else {
        parts.join(" · ")
    }
}
