//! Kachelmodell des Fensters „Projekte“: Zählung, neuestes Dokument, Version, Texte, Lesen
//! echter Bibliotheken (nur in Temp-Ordnern).

mod common;

use chrono::{Datelike, NaiveDate, TimeZone, Timelike};
use planora_app::project_tiles::*;
use planora_exchange::Progress;
use planora_feature::Document;
use planora_project::docstore::{DocStore, DocumentInfo, LockMode, VersionInfo, peek};
use planora_project::library::{Library, NodeKind, NodePath, TreeNode};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

// ---------- Hilfen ----------

fn p(parts: &[&str]) -> NodePath {
    parts.iter().map(|s| s.to_string()).collect()
}

fn node(path: &[&str], kind: NodeKind, children: Vec<TreeNode>) -> TreeNode {
    let last = path.last().copied().unwrap_or_default();
    TreeNode {
        name: last.strip_suffix(".pcad").unwrap_or(last).to_string(),
        kind,
        path: p(path),
        children,
    }
}

fn doc(path: &[&str]) -> TreeNode {
    node(path, NodeKind::Document, Vec::new())
}

fn folder(path: &[&str], children: Vec<TreeNode>) -> TreeNode {
    node(path, NodeKind::Folder, children)
}

fn project(name: &str, children: Vec<TreeNode>) -> TreeNode {
    node(&[name], NodeKind::Project, children)
}

fn read_ok(version: u32, changed: &str) -> DocState {
    DocState::Read(DocFacts {
        version,
        branch: "Haupt".into(),
        changed: changed.into(),
    })
}

fn bad() -> DocState {
    DocState::Unreadable("kaputt".into())
}

/// Scan über einen von Hand gebauten Baum; Dokumente ohne Angabe sind unlesbar.
fn scan_of(tree: Vec<TreeNode>, states: Vec<(NodePath, DocState)>) -> Scan {
    let states: HashMap<NodePath, DocState> = states.into_iter().collect();
    scan_tree(PathBuf::from("bibliothek"), tree, |path| DocEntry {
        dir: PathBuf::from(path.join("/")),
        state: states.get(path).cloned().unwrap_or_else(bad),
    })
}

fn tile_named<'a>(tiles: &'a [Tile], name: &str) -> &'a Tile {
    tiles.iter().find(|t| t.name == name).expect("Kachel fehlt")
}

fn facts_of(version: u32) -> DocFacts {
    DocFacts {
        version,
        branch: "Haupt".into(),
        changed: "2026-10-23T12:00:00Z".into(),
    }
}

fn tile(kind: TileKind, documents: usize, unreadable: usize, version: Option<u32>) -> Tile {
    Tile {
        kind,
        name: "x".into(),
        path: p(&["x"]),
        documents,
        unreadable,
        newest: version.map(|v| (p(&["x", "d.pcad"]), facts_of(v))),
    }
}

/// Erwartete Ortszeit, unabhängig von `local_time` über die Zeitbibliothek gerechnet.
fn expected_local(y: i32, mo: u32, d: u32, h: u32, mi: u32, s: u32) -> String {
    let utc = NaiveDate::from_ymd_opt(y, mo, d)
        .unwrap()
        .and_hms_opt(h, mi, s)
        .unwrap()
        .and_utc();
    let local = chrono::Local
        .timestamp_opt(utc.timestamp(), 0)
        .single()
        .unwrap();
    format!(
        "{:02}.{:02}.{:04}, {:02}:{:02}",
        local.day(),
        local.month(),
        local.year(),
        local.hour(),
        local.minute()
    )
}

/// Neue Bibliothek in Temp mit einem Projekt; liefert Wurzel und Bibliothek.
fn temp_library(prefix: &str) -> (PathBuf, Library) {
    let root = common::temp_root(prefix);
    let lib = Library::open(&root).unwrap();
    (root, lib)
}

// ---------- Datum ----------

#[test]
fn format_changed_is_day_month_year_and_minutes() {
    // P1: Ortszeit des Rechners zum angegebenen Zeitpunkt.
    assert_eq!(
        format_changed("2026-10-23T16:42:07Z"),
        Some(expected_local(2026, 10, 23, 16, 42, 7))
    );
    // Tag ≠ Monat, Minuten ≠ Sekunden: Vertauschungen fallen auf.
    assert_eq!(
        format_changed("2026-03-05T10:42:07Z"),
        Some(expected_local(2026, 3, 5, 10, 42, 7))
    );
    assert_eq!(format_changed("2026-1-3T1:2Z"), None);
    assert_eq!(format_changed(""), None);
    assert_eq!(format_changed("x".repeat(30).as_str()), None);
    assert_eq!(format_changed("2026-10-23 16:42:07"), None);
}

#[test]
fn newest_document_dates_the_project() {
    let tree = vec![project(
        "P",
        vec![doc(&["P", "früh.pcad"]), doc(&["P", "spät.pcad"])],
    )];
    let scan = scan_of(
        tree,
        vec![
            (p(&["P", "früh.pcad"]), read_ok(3, "2026-10-23T10:00:00Z")),
            (p(&["P", "spät.pcad"]), read_ok(7, "2026-10-23T12:00:00Z")),
        ],
    );
    let tiles = tiles_at(&scan, &vec![]).unwrap();
    let t = tile_named(&tiles, "P");
    let (path, f) = t.newest.clone().unwrap();
    assert_eq!(path, p(&["P", "spät.pcad"]));
    assert_eq!(f.version, 7);
    assert_eq!(
        changed_line(t),
        format!("Geändert {}", expected_local(2026, 10, 23, 12, 0, 0))
    );
    assert_eq!(
        meta_line(t, false),
        ("Version 7 · 2 Dokumente".into(), Tone::Normal)
    );
}

#[test]
fn changed_line_without_a_readable_date() {
    let mut t = tile(TileKind::Document, 1, 0, Some(4));
    t.newest.as_mut().unwrap().1.changed = "kein Datum".into();
    assert_eq!(changed_line(&t), "Datum unbekannt");
    let none = tile(TileKind::Project, 1, 1, None);
    assert_eq!(changed_line(&none), "");
}

// ---------- Version ----------

#[test]
fn version_is_the_tip_of_the_active_branch_not_the_newest() {
    let (root, lib) = temp_library("planora-tiles-version");
    lib.create_project("P").unwrap();
    let dir = root.join("P").join("d.pcad");
    let mut store = DocStore::create(&dir, &Document::new().snapshot(), "").unwrap();
    let v2 = store
        .save(None, &Document::new().snapshot(), "", Some("B"))
        .unwrap();
    assert_eq!(v2, 2);
    store.set_active_branch("Haupt").unwrap();
    drop(store);
    let (info, _) = peek(&dir).unwrap();
    assert_eq!(info.active_branch, "Haupt");
    let f = facts(&info).unwrap();
    assert_eq!(f.version, 1);
    assert_eq!(f.branch, "Haupt");
    // Datum und Version beschreiben dieselbe Version: die Spitze des aktiven Zweigs.
    assert_eq!(f.changed, info.version(1).unwrap().time);
    assert_eq!(doc_entry(&dir).state, DocState::Read(f));
    assert_eq!(doc_entry(&dir).dir, dir);
}

#[test]
fn a_document_without_tip_has_no_facts() {
    let info = DocumentInfo {
        id: "x".into(),
        versions: Vec::new(),
        branches: BTreeMap::new(),
        active_branch: "Haupt".into(),
        max_number: 3,
    };
    assert_eq!(facts(&info), None);
}

#[test]
fn a_damaged_document_is_unreadable() {
    let (root, lib) = temp_library("planora-tiles-damaged");
    lib.create_project("P").unwrap();
    let dir = root.join("P").join("d.pcad");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("dokument.json"), b"{ abgeschnitten").unwrap();
    let e = doc_entry(&dir);
    assert_eq!(e.dir, dir);
    assert!(matches!(e.state, DocState::Unreadable(ref m) if !m.is_empty()));
    // Auch über die ganze Bibliothek: gezählt, aber unlesbar.
    let scan = scan_library(&root, &Progress::new()).unwrap();
    let tiles = tiles_at(&scan, &vec![]).unwrap();
    assert_eq!(tiles[0].documents, 1);
    assert_eq!(tiles[0].unreadable, 1);
    assert_eq!(tiles[0].newest, None);
}

// ---------- Gleichstand ----------

#[test]
fn ties_pick_the_smaller_name() {
    let t = "2026-10-23T12:00:00Z";
    let tree = vec![project(
        "P",
        vec![doc(&["P", "A.pcad"]), doc(&["P", "b.pcad"])],
    )];
    let scan = scan_of(
        tree,
        vec![
            (p(&["P", "b.pcad"]), read_ok(2, t)),
            (p(&["P", "A.pcad"]), read_ok(1, t)),
        ],
    );
    let tiles = tiles_at(&scan, &vec![]).unwrap();
    assert_eq!(tiles[0].newest.as_ref().unwrap().0, p(&["P", "A.pcad"]));
    // ohne Groß-/Kleinschreibung: „a“ vor „B“ (mit Schreibung käme „B“ zuerst)
    let tree = vec![project(
        "P",
        vec![doc(&["P", "a.pcad"]), doc(&["P", "B.pcad"])],
    )];
    let scan = scan_of(
        tree,
        vec![
            (p(&["P", "B.pcad"]), read_ok(2, t)),
            (p(&["P", "a.pcad"]), read_ok(1, t)),
        ],
    );
    let tiles = tiles_at(&scan, &vec![]).unwrap();
    assert_eq!(tiles[0].newest.as_ref().unwrap().0, p(&["P", "a.pcad"]));
}

#[test]
fn equal_names_pick_the_smaller_path() {
    let t = "2026-10-23T12:00:00Z";
    let tree = vec![project(
        "P",
        vec![
            folder(&["P", "Y"], vec![doc(&["P", "Y", "d.pcad"])]),
            folder(&["P", "X"], vec![doc(&["P", "X", "D.pcad"])]),
        ],
    )];
    let scan = scan_of(
        tree,
        vec![
            (p(&["P", "Y", "d.pcad"]), read_ok(2, t)),
            (p(&["P", "X", "D.pcad"]), read_ok(1, t)),
        ],
    );
    let tiles = tiles_at(&scan, &vec![]).unwrap();
    assert_eq!(
        tiles[0].newest.as_ref().unwrap().0,
        p(&["P", "X", "D.pcad"])
    );
}

// ---------- Unlesbare ----------

#[test]
fn unreadable_documents_count_but_do_not_date() {
    let tree = vec![project(
        "P",
        vec![doc(&["P", "alt.pcad"]), doc(&["P", "kaputt.pcad"])],
    )];
    let scan = scan_of(
        tree,
        vec![(p(&["P", "alt.pcad"]), read_ok(5, "2020-01-01T00:00:00Z"))],
    );
    let tiles = tiles_at(&scan, &vec![]).unwrap();
    let t = &tiles[0];
    assert_eq!(t.documents, 2);
    assert_eq!(t.unreadable, 1);
    assert_eq!(t.newest.as_ref().unwrap().0, p(&["P", "alt.pcad"]));
    assert_eq!(
        meta_line(t, false),
        ("Version 5 · 2 Dokumente".into(), Tone::Normal)
    );
    // Dokumentkacheln der Ebene darunter
    let inner = tiles_at(&scan, &p(&["P"])).unwrap();
    let k = tile_named(&inner, "kaputt");
    assert_eq!((k.documents, k.unreadable, k.newest.clone()), (1, 1, None));
    assert_eq!(changed_line(k), "");
    assert_eq!(
        meta_line(k, false),
        ("Dokument nicht lesbar".into(), Tone::Warning)
    );
}

#[test]
fn all_unreadable_warns() {
    let tree = vec![project(
        "P",
        vec![
            doc(&["P", "a.pcad"]),
            folder(&["P", "F"], vec![doc(&["P", "F", "b.pcad"])]),
        ],
    )];
    let scan = scan_of(tree, Vec::new());
    let t = tiles_at(&scan, &vec![]).unwrap().remove(0);
    assert_eq!((t.documents, t.unreadable, t.newest.clone()), (2, 2, None));
    assert_eq!(
        meta_line(&t, false),
        ("Dokumente nicht lesbar".into(), Tone::Warning)
    );
    assert_eq!(
        meta_line(&t, true),
        ("Dokumente nicht lesbar".into(), Tone::Warning)
    );
    assert_eq!(changed_line(&t), "");
    let f = tile_named(&tiles_at(&scan, &p(&["P"])).unwrap(), "F").clone();
    assert_eq!(
        meta_line(&f, false),
        ("Dokumente nicht lesbar".into(), Tone::Warning)
    );
}

// ---------- Rekursiv ----------

#[test]
fn nested_documents_count_for_project_and_folder() {
    let tree = vec![project(
        "P",
        vec![
            doc(&["P", "d2.pcad"]),
            folder(
                &["P", "F"],
                vec![folder(
                    &["P", "F", "G"],
                    vec![doc(&["P", "F", "G", "d1.pcad"])],
                )],
            ),
        ],
    )];
    let scan = scan_of(
        tree,
        vec![
            (
                p(&["P", "F", "G", "d1.pcad"]),
                read_ok(9, "2026-10-23T12:00:00Z"),
            ),
            (p(&["P", "d2.pcad"]), read_ok(4, "2026-10-23T10:00:00Z")),
        ],
    );
    let top = tiles_at(&scan, &vec![]).unwrap();
    assert_eq!(top[0].documents, 2);
    assert_eq!(
        top[0].newest.as_ref().unwrap().0,
        p(&["P", "F", "G", "d1.pcad"])
    );
    let level = tiles_at(&scan, &p(&["P"])).unwrap();
    // Reihenfolge wie im Baum
    assert_eq!(
        level.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(),
        ["d2", "F"]
    );
    let f = tile_named(&level, "F");
    assert_eq!(f.kind, TileKind::Folder);
    assert_eq!(f.documents, 1);
    assert_eq!(f.newest.as_ref().unwrap().1.version, 9);
    assert_eq!(meta_line(f, false), ("1 Dokument".into(), Tone::Normal));
    let d = tile_named(&level, "d2");
    assert_eq!(d.kind, TileKind::Document);
    assert_eq!(d.path, p(&["P", "d2.pcad"]));
    assert_eq!(d.newest.as_ref().unwrap().0, p(&["P", "d2.pcad"]));
    assert_eq!(meta_line(d, false), ("Version 4".into(), Tone::Normal));
    let g = tiles_at(&scan, &p(&["P", "F"])).unwrap();
    assert_eq!(g[0].documents, 1);
    assert_eq!(g[0].kind, TileKind::Folder);
    assert_eq!(
        tiles_at(&scan, &p(&["P", "F", "G"])).unwrap()[0].kind,
        TileKind::Document
    );
}

// ---------- Texte ----------

#[test]
fn meta_lines_have_the_spec_texts() {
    let normal = |s: &str| (s.to_string(), Tone::Normal);
    assert_eq!(
        meta_line(&tile(TileKind::Project, 3, 0, Some(12)), false),
        normal("Version 12 · 3 Dokumente")
    );
    assert_eq!(
        meta_line(&tile(TileKind::Project, 1, 0, Some(12)), true),
        normal("Noch keine Körper · 1 Dokument")
    );
    assert_eq!(
        meta_line(&tile(TileKind::Project, 0, 0, None), false),
        normal("Noch kein Dokument")
    );
    assert_eq!(
        meta_line(&tile(TileKind::Folder, 4, 1, Some(2)), false),
        normal("4 Dokumente")
    );
    assert_eq!(
        meta_line(&tile(TileKind::Folder, 0, 0, None), false),
        normal("Noch kein Dokument")
    );
    assert_eq!(
        meta_line(&tile(TileKind::Document, 1, 0, Some(12)), false),
        normal("Version 12")
    );
    assert_eq!(documents_text(0), "0 Dokumente");
    assert_eq!(documents_text(1), "1 Dokument");
    assert_eq!(documents_text(2), "2 Dokumente");
}

#[test]
fn footer_counts() {
    let many = |kind, n| {
        (0..n)
            .map(|_| tile(kind, 1, 0, Some(1)))
            .collect::<Vec<_>>()
    };
    assert_eq!(footer_text(&many(TileKind::Project, 6), true), "6 Projekte");
    assert_eq!(footer_text(&many(TileKind::Project, 1), true), "1 Projekt");
    assert_eq!(footer_text(&[], true), "Keine Projekte");
    let mut mixed = many(TileKind::Folder, 1);
    mixed.extend(many(TileKind::Document, 3));
    assert_eq!(footer_text(&mixed, false), "1 Ordner · 3 Dokumente");
    assert_eq!(footer_text(&many(TileKind::Folder, 2), false), "2 Ordner");
    assert_eq!(
        footer_text(&many(TileKind::Document, 1), false),
        "1 Dokument"
    );
    assert_eq!(footer_text(&[], false), "Leer");
}

// ---------- Ebenen ----------

#[test]
fn tiles_at_a_vanished_location_is_none() {
    let scan = scan_of(vec![project("P", vec![doc(&["P", "d.pcad"])])], Vec::new());
    assert!(tiles_at(&scan, &vec!["weg".into()]).is_none());
    assert!(tiles_at(&scan, &p(&["P", "weg"])).is_none());
    // Ein Dokument ist keine Ebene.
    assert!(tiles_at(&scan, &p(&["P", "d.pcad"])).is_none());
    assert_eq!(tiles_at(&scan, &p(&["P"])).unwrap().len(), 1);
}

#[test]
fn more_than_500_tiles_are_cut() {
    let tree: Vec<TreeNode> = (0..501)
        .map(|i| project(&format!("P{i:03}"), Vec::new()))
        .collect();
    let scan = scan_of(tree, Vec::new());
    let tiles = tiles_at(&scan, &vec![]).unwrap();
    assert_eq!(tiles.len(), MAX_TILES);
    assert_eq!(tiles[0].name, "P000");
    assert_eq!(tiles[MAX_TILES - 1].name, "P499");
    assert_eq!(TOO_MANY, "Es werden nur die ersten 500 gezeigt.");
}

// ---------- Echte Bibliotheken ----------

#[test]
fn scan_library_reads_a_real_library_and_ignores_extra_files() {
    let (root, lib) = temp_library("planora-tiles-real");
    let prj = lib.create_project("Haus").unwrap();
    let f = lib.create_folder(&prj, "Keller").unwrap();
    let dir = root.join("Haus").join("Keller").join("Tür.pcad");
    DocStore::create(&dir, &Document::new().snapshot(), "").unwrap();
    std::fs::write(root.join("Haus").join("notiz.txt"), b"fremd").unwrap();
    let scan = scan_library(&root, &Progress::new()).unwrap();
    assert_eq!(scan.root, root);
    assert_eq!(scan.docs.len(), 1);
    let key = p(&["Haus", "Keller", "Tür.pcad"]);
    let entry = &scan.docs[&key];
    // Ordner so, wie die Bibliothek ihn liefert (unter Windows mit Präfix `\\?\`).
    assert_eq!(entry.dir, lib.document_dir(&key).unwrap());
    assert!(entry.dir.ends_with(dir.strip_prefix(&root).unwrap()));
    assert!(matches!(entry.state, DocState::Read(ref f) if f.version == 1 && f.branch == "Haupt"));
    let top = tiles_at(&scan, &vec![]).unwrap();
    assert_eq!(top.len(), 1);
    assert_eq!(top[0].kind, TileKind::Project);
    assert_eq!(top[0].documents, 1);
    let level = tiles_at(&scan, &prj).unwrap();
    assert_eq!(level.len(), 1, "notiz.txt ist keine Kachel");
    assert_eq!(level[0].path, f);
    assert_eq!(footer_text(&level, false), "1 Ordner");
    let docs = tiles_at(&scan, &f).unwrap();
    assert_eq!(docs[0].name, "Tür");
    assert_eq!(
        meta_line(&docs[0], false),
        ("Version 1".into(), Tone::Normal)
    );
}

#[test]
fn scan_library_never_creates_a_missing_root() {
    let root = common::temp_root("planora-tiles-missing");
    assert!(!root.exists());
    assert!(scan_library(&root, &Progress::new()).is_err());
    assert!(
        !root.exists(),
        "der fehlende Ordner darf nicht angelegt werden"
    );
}

#[test]
fn a_cancelled_scan_stops() {
    let (root, lib) = temp_library("planora-tiles-cancel");
    lib.create_project("P").unwrap();
    DocStore::create(
        &root.join("P").join("d.pcad"),
        &Document::new().snapshot(),
        "",
    )
    .unwrap();
    let progress = Progress::new();
    progress.cancel();
    assert_eq!(scan_library(&root, &progress).unwrap_err(), CANCELLED);
    assert_eq!(CANCELLED, "Abgebrochen");
}

// ---------- Fix-Runde 1: Datum der Spitze, unmögliche Zeiten, Abbruch, Fortschritt ----------

/// Schreibt die Zeiten der genannten Versionen in `dokument.json` um (ohne Warten). Die Datei
/// wird zeilenweise geändert: in jedem Versionseintrag steht `"nummer"` vor `"zeit"`.
fn set_times(dir: &Path, times: &[(u32, &str)]) {
    let file = dir.join("dokument.json");
    let text = std::fs::read_to_string(&file).unwrap();
    let mut current: Option<u32> = None;
    let mut changed = 0;
    let out: Vec<String> = text
        .lines()
        .map(|line| {
            let t = line.trim_start();
            if let Some(rest) = t.strip_prefix("\"nummer\":") {
                current = rest.trim().trim_end_matches(',').parse().ok();
            } else if t.starts_with("\"zeit\":")
                && let Some((_, time)) = times.iter().find(|(n, _)| Some(*n) == current)
            {
                changed += 1;
                let indent = &line[..line.len() - t.len()];
                return format!("{indent}\"zeit\": \"{time}\",");
            }
            line.to_string()
        })
        .collect();
    assert_eq!(
        changed,
        times.len(),
        "jede Zeit muss genau einmal ersetzt werden"
    );
    std::fs::write(&file, out.join("\n")).unwrap();
}

fn new_doc(root: &Path, project: &str, name: &str) -> PathBuf {
    let dir = root.join(project).join(format!("{name}.pcad"));
    DocStore::create(&dir, &Document::new().snapshot(), "").unwrap();
    dir
}

fn read_facts(dir: &Path) -> DocFacts {
    match doc_entry(dir).state {
        DocState::Read(f) => f,
        DocState::Unreadable(e) => panic!("unlesbar: {e}"),
    }
}

#[test]
fn the_date_is_the_time_of_the_active_tip_after_branch_restore_and_delete() {
    let (root, lib) = temp_library("planora-tiles-tipdate");
    lib.create_project("P").unwrap();
    let dir = new_doc(&root, "P", "d");
    let mut store = DocStore::open(&dir, LockMode::Acquire).unwrap().0;
    store
        .save(None, &Document::new().snapshot(), "", Some("B"))
        .unwrap(); // v2 auf B
    store
        .save(None, &Document::new().snapshot(), "", None)
        .unwrap(); // v3 auf B
    store.set_active_branch("Haupt").unwrap();
    drop(store);
    set_times(
        &dir,
        &[
            (1, "2026-10-23T10:00:00Z"),
            (2, "2026-10-23T11:00:00Z"),
            (3, "2026-10-23T12:00:00Z"),
        ],
    );
    let f = read_facts(&dir);
    assert_eq!((f.version, f.branch.as_str()), (1, "Haupt"));
    assert_eq!(
        f.changed, "2026-10-23T10:00:00Z",
        "Datum von v1, nicht von v3 auf B"
    );
    let scan = scan_library(&root, &Progress::new()).unwrap();
    let t = tiles_at(&scan, &p(&["P"])).unwrap().remove(0);
    assert_eq!(meta_line(&t, false), ("Version 1".into(), Tone::Normal));
    assert_eq!(
        changed_line(&t),
        format!("Geändert {}", expected_local(2026, 10, 23, 10, 0, 0))
    );

    // Wiederherstellen auf den aktiven Zweig: neue Spitze v4 mit eigenem Datum.
    let mut store = DocStore::open(&dir, LockMode::Acquire).unwrap().0;
    assert_eq!(store.restore(3, "Haupt").unwrap(), 4);
    drop(store);
    set_times(&dir, &[(4, "2026-10-23T13:00:00Z")]);
    let f = read_facts(&dir);
    assert_eq!((f.version, f.changed.as_str()), (4, "2026-10-23T13:00:00Z"));

    // Die Spitze löschen: wieder v1 mit dem Datum von v1 (v3 auf B bleibt jünger).
    let mut store = DocStore::open(&dir, LockMode::Acquire).unwrap().0;
    store.delete_version(4).unwrap();
    drop(store);
    let f = read_facts(&dir);
    assert_eq!((f.version, f.changed.as_str()), (1, "2026-10-23T10:00:00Z"));
}

#[test]
fn the_newest_document_is_judged_by_its_active_tip() {
    let (root, lib) = temp_library("planora-tiles-tiporder");
    lib.create_project("P").unwrap();
    // X: aktiver Zweig alt (10:00), Nebenzweig sehr jung (15:00)
    let x = new_doc(&root, "P", "X");
    let mut store = DocStore::open(&x, LockMode::Acquire).unwrap().0;
    store
        .save(None, &Document::new().snapshot(), "", Some("B"))
        .unwrap();
    store.set_active_branch("Haupt").unwrap();
    drop(store);
    set_times(
        &x,
        &[(1, "2026-10-23T10:00:00Z"), (2, "2026-10-23T15:00:00Z")],
    );
    // Y: Spitze 12:00
    let y = new_doc(&root, "P", "Y");
    set_times(&y, &[(1, "2026-10-23T12:00:00Z")]);
    let scan = scan_library(&root, &Progress::new()).unwrap();
    let t = tiles_at(&scan, &vec![]).unwrap().remove(0);
    let (path, f) = t.newest.clone().unwrap();
    assert_eq!(path, p(&["P", "Y.pcad"]));
    assert_eq!(f.changed, "2026-10-23T12:00:00Z");
}

#[test]
fn an_impossible_time_does_not_date_and_does_not_win() {
    for bad_time in ["9999-99-99T99:99:99Z", "2026-13-40T25:61:61Z"] {
        let (root, lib) = temp_library("planora-tiles-badtime");
        lib.create_project("P").unwrap();
        let healthy = new_doc(&root, "P", "gesund");
        set_times(&healthy, &[(1, "2026-10-23T12:00:00Z")]);
        let odd = new_doc(&root, "P", "seltsam");
        set_times(&odd, &[(1, bad_time)]);
        assert!(
            matches!(doc_entry(&odd).state, DocState::Unreadable(ref m) if !m.is_empty()),
            "{bad_time}"
        );
        let scan = scan_library(&root, &Progress::new()).unwrap();
        let t = tiles_at(&scan, &vec![]).unwrap().remove(0);
        assert_eq!((t.documents, t.unreadable), (2, 1), "{bad_time}");
        assert_eq!(t.newest.as_ref().unwrap().0, p(&["P", "gesund.pcad"]));
        assert_eq!(
            changed_line(&t),
            format!("Geändert {}", expected_local(2026, 10, 23, 12, 0, 0))
        );
    }
}

fn info_with_time(time: &str) -> DocumentInfo {
    DocumentInfo {
        id: "x".into(),
        versions: vec![VersionInfo {
            number: 1,
            parent: None,
            branch: "Haupt".into(),
            time: time.into(),
            comment: String::new(),
            program: String::new(),
        }],
        branches: BTreeMap::from([("Haupt".to_string(), 1)]),
        active_branch: "Haupt".into(),
        max_number: 1,
    }
}

#[test]
fn facts_reject_an_impossible_tip_time() {
    assert_eq!(facts(&info_with_time("9999-99-99T99:99:99Z")), None);
    assert_eq!(facts(&info_with_time("2026-13-40T25:61:61Z")), None);
    let f = facts(&info_with_time("2026-10-23T12:00:00Z")).unwrap();
    assert_eq!((f.version, f.changed.as_str()), (1, "2026-10-23T12:00:00Z"));
}

/// Bibliothek mit drei Dokumenten.
fn three_docs(prefix: &str) -> PathBuf {
    let (root, lib) = temp_library(prefix);
    lib.create_project("P").unwrap();
    for n in ["a", "b", "c"] {
        new_doc(&root, "P", n);
    }
    root
}

#[test]
fn cancelling_during_the_scan_stops_before_the_rest() {
    let root = three_docs("planora-tiles-midcancel");
    let progress = Progress::new();
    let mut read = 0;
    let result = scan_library_with(&root, &progress, |_| {
        read += 1;
        progress.cancel(); // nach dem ersten Dokument
    });
    assert_eq!(result.unwrap_err(), CANCELLED);
    assert_eq!(
        read, 1,
        "nach dem Abbruch wird kein weiteres Dokument gelesen"
    );
}

#[test]
fn progress_counts_every_document_once() {
    let root = three_docs("planora-tiles-progress");
    let progress = Progress::new();
    assert_eq!(progress.fraction(), 0.0);
    let mut seen = Vec::new();
    let mut paths = Vec::new();
    let scan = scan_library_with(&root, &progress, |path| {
        seen.push(progress.fraction());
        paths.push(path.clone());
    })
    .unwrap();
    // Gesamtzahl 3, je Dokument genau ein Schritt, am Ende alles erledigt.
    assert_eq!(seen, [1.0 / 3.0, 2.0 / 3.0, 1.0]);
    assert_eq!(progress.fraction(), 1.0);
    assert_eq!(paths, scan.docs.keys().cloned().collect::<Vec<_>>());
    // Ohne Haken: gleiches Ergebnis.
    let plain = scan_library(&root, &Progress::new()).unwrap();
    assert_eq!(plain.docs, scan.docs);
}
