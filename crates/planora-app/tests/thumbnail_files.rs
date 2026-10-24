//! Die Zusatzdateien `vorschau.png`/`vorschau.json` im Dokumentordner sind für Bibliothek,
//! Versionsspeicher und Dateiformat unsichtbar (Spec §6, §9): Baum und Lesen bleiben gleich,
//! Umbenennen, Verschieben, Löschen und Kopieren nehmen sie mit, alte Dateien laden und
//! speichern wie bisher.

mod common;

use planora_app::project_tiles::scan_library;
use planora_app::state::AppState;
use planora_app::thumbnail_cache::*;
use planora_exchange::Progress;
use planora_math::DVec3;
use planora_project::docstore::{
    DocStore, LockMode, MAIN_BRANCH, VERSIONS_DIR, peek, read_version,
};
use planora_project::fsutil::TRASH;
use planora_project::library::{Library, NodeKind, NodePath, TreeNode};
use planora_project::{decode_version, encode_version};
use std::fs;
use std::path::{Path, PathBuf};

fn boxed() -> AppState {
    let mut s = AppState::new();
    s.create_box(DVec3::new(10.0, 20.0, 30.0)).unwrap();
    s
}

/// Bibliothek mit „P/F/Teil.pcad“ und „P/Zwei.pcad“.
fn library() -> (Library, PathBuf, PathBuf) {
    let root = common::temp_root("planora-thumbfiles");
    let lib = Library::open(&root).unwrap();
    let p = lib.create_project("P").unwrap();
    lib.create_folder(&p, "F").unwrap();
    let base = lib.root().path().join("P");
    let a = base.join("F").join("Teil.pcad");
    let b = base.join("Zwei.pcad");
    drop(DocStore::create(&a, &boxed().snapshot(), "").unwrap());
    drop(DocStore::create(&b, &AppState::new().snapshot(), "").unwrap());
    (lib, a, b)
}

fn key1() -> ThumbKey {
    ThumbKey::new(1, MAIN_BRANCH, true)
}

/// Miniatur bauen und speichern; beide Zusatzdateien müssen danach da sein (bzw. die JSON).
fn add_preview(dir: &Path) {
    let out = thumbnail_for(dir, &key1(), &BuildLimits::default());
    assert!(out.stored, "{:?}", out.thumb);
    assert!(dir.join(PREVIEW_JSON).is_file());
}

/// Baum als Liste (Pfad, Art, Name), alle Ebenen.
fn flat(tree: &[TreeNode]) -> Vec<(NodePath, NodeKind, String)> {
    let mut out = Vec::new();
    fn walk(n: &TreeNode, out: &mut Vec<(NodePath, NodeKind, String)>) {
        out.push((n.path.clone(), n.kind, n.name.clone()));
        n.children.iter().for_each(|c| walk(c, out));
    }
    tree.iter().for_each(|n| walk(n, &mut out));
    out
}

fn copy_dir(from: &Path, to: &Path) {
    fs::create_dir(to).unwrap();
    for e in fs::read_dir(from).unwrap().flatten() {
        let target = to.join(e.file_name());
        if e.path().is_dir() {
            copy_dir(&e.path(), &target);
        } else {
            fs::copy(e.path(), target).unwrap();
        }
    }
}

/// Sucht unter `dir` (rekursiv) einen Ordner, der `vorschau.json` enthält.
fn find_preview(dir: &Path) -> Option<PathBuf> {
    if dir.join(PREVIEW_JSON).is_file() {
        return Some(dir.to_path_buf());
    }
    fs::read_dir(dir)
        .ok()?
        .flatten()
        .filter(|e| e.path().is_dir())
        .find_map(|e| find_preview(&e.path()))
}

#[test]
fn library_tree_ignores_preview_files() {
    let (lib, a, b) = library();
    let root = lib.root().path().to_path_buf();
    let tree = flat(&lib.tree().unwrap());
    let scan = scan_library(&root, &Progress::new()).unwrap();
    assert_eq!(tree.len(), 4, "{tree:?}");
    add_preview(&a);
    add_preview(&b);
    // Dazu ein Ordner namens vorschau.png.tmp und eine Zwischendatei.
    fs::write(a.join("vorschau.png.77-1.tmp"), "x").unwrap();
    assert_eq!(flat(&lib.tree().unwrap()), tree);
    let again = scan_library(&root, &Progress::new()).unwrap();
    assert_eq!(again.docs, scan.docs);
    assert_eq!(flat(&again.tree), tree);
}

#[test]
fn peek_and_open_report_no_orphans_and_save_still_works() {
    let (_lib, a, _) = library();
    add_preview(&a);
    let png = fs::read(a.join(PREVIEW_PNG)).unwrap();
    let json = fs::read(a.join(PREVIEW_JSON)).unwrap();
    let (info, report) = peek(&a).unwrap();
    assert!(
        report.orphans.is_empty() && report.missing.is_empty(),
        "{report:?}"
    );
    assert_eq!(info.versions.len(), 1);
    let (mut st, report) = DocStore::open(&a, LockMode::Acquire).unwrap();
    assert!(
        report.orphans.is_empty() && report.missing.is_empty(),
        "{report:?}"
    );
    let mut s = boxed();
    s.create_sphere(5.0).unwrap();
    let v = st.save(None, &s.snapshot(), "Kugel", None).unwrap();
    assert_eq!(v, 2);
    assert!(st.load_version(2).unwrap().same_content(&s.snapshot()));
    assert!(st.load_version(1).is_ok());
    drop(st);
    assert_eq!(
        fs::read(a.join(PREVIEW_PNG)).unwrap(),
        png,
        "Bild unberührt"
    );
    assert_eq!(fs::read(a.join(PREVIEW_JSON)).unwrap(), json);
    // Die alte Miniatur gilt für Version 1 weiter, für die neue Spitze nicht.
    assert!(load_cached(&a, &key1()).is_some());
    assert!(load_cached(&a, &ThumbKey::new(2, MAIN_BRANCH, true)).is_none());
    let (_, report) = peek(&a).unwrap();
    assert!(report.orphans.is_empty() && report.missing.is_empty());
}

#[test]
fn rename_move_delete_carry_the_preview_along() {
    let (lib, a, _) = library();
    add_preview(&a);
    let node: NodePath = vec!["P".into(), "F".into(), "Teil.pcad".into()];

    let renamed = lib.rename(&node, "Neu").unwrap();
    let dir = lib.document_dir(&renamed).unwrap();
    assert!(!a.exists());
    assert!(dir.join(PREVIEW_PNG).is_file() && dir.join(PREVIEW_JSON).is_file());
    assert!(
        load_cached(&dir, &key1()).is_some(),
        "nach Umbenennen gültig"
    );

    let moved = lib.move_node(&renamed, &vec!["P".into()]).unwrap();
    let dir2 = lib.document_dir(&moved).unwrap();
    assert!(!dir.exists());
    assert!(
        load_cached(&dir2, &key1()).is_some(),
        "nach Verschieben gültig"
    );

    // Ordner samt Dokument verschieben und löschen.
    let folder = lib.create_folder(&vec!["P".into()], "G").unwrap();
    let in_folder = lib.move_node(&moved, &folder).unwrap();
    assert!(load_cached(&lib.document_dir(&in_folder).unwrap(), &key1()).is_some());
    lib.delete(&folder).unwrap();
    assert!(!lib.root().path().join("P").join("G").exists());
    let trash = lib.root().path().join(TRASH);
    let found = find_preview(&trash).expect("Miniatur liegt im Papierkorb");
    assert!(found.join(PREVIEW_PNG).is_file());
    assert!(found.join("dokument.json").is_file());

    // Dokument direkt löschen.
    let (lib, b, _) = library();
    add_preview(&b);
    lib.delete(&vec!["P".into(), "F".into(), "Teil.pcad".into()])
        .unwrap();
    assert!(!b.exists());
    let found = find_preview(&lib.root().path().join(TRASH)).unwrap();
    assert!(found.join(PREVIEW_PNG).is_file());
}

#[test]
fn a_copied_document_folder_keeps_a_valid_preview() {
    let (lib, a, _) = library();
    add_preview(&a);
    let copy = a.with_file_name("Kopie.pcad");
    copy_dir(&a, &copy);
    let (_, report) = peek(&copy).unwrap();
    assert!(report.orphans.is_empty() && report.missing.is_empty());
    let cached = load_cached(&copy, &key1()).expect("gültig in der Kopie");
    let original = load_cached(&a, &key1()).unwrap();
    match (cached, original) {
        (Thumb::Image(x), Thumb::Image(y)) => assert_eq!(x.rgba, y.rgba),
        other => panic!("{other:?}"),
    }
    let names: Vec<String> = flat(&lib.tree().unwrap())
        .into_iter()
        .map(|n| n.2)
        .collect();
    assert!(names.contains(&"Kopie".to_string()), "{names:?}");
    assert!(DocStore::open(&copy, LockMode::Acquire).is_ok());
}

#[test]
fn leftover_temporary_preview_files_are_ignored() {
    let (lib, a, _) = library();
    let tree = flat(&lib.tree().unwrap());
    let report_before = peek(&a).unwrap().1;
    add_preview(&a);
    for n in ["vorschau.png.123-4.tmp", "vorschau.json.123-5.tmp"] {
        fs::write(a.join(n), "halb").unwrap();
    }
    assert_eq!(flat(&lib.tree().unwrap()), tree);
    assert_eq!(peek(&a).unwrap().1, report_before);
    let (st, report) = DocStore::open(&a, LockMode::Acquire).unwrap();
    assert_eq!(report, report_before);
    drop(st);
    assert!(load_cached(&a, &key1()).is_some(), "Miniatur gilt weiter");
}

#[test]
fn old_golden_documents_still_load_and_save_byte_identical() {
    let golden = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("planora-project")
        .join("tests")
        .join("data");
    let (lib, _, _) = library();
    let mut n = 0;
    for entry in fs::read_dir(&golden).unwrap().flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let original = fs::read(&path).unwrap();
        let text = String::from_utf8(original.clone()).unwrap();
        let name = path.file_stem().unwrap().to_string_lossy().into_owned();
        let dir = lib.root().path().join("P").join(format!("{name}.pcad"));
        drop(DocStore::create(&dir, &AppState::new().snapshot(), "").unwrap());
        fs::write(dir.join(VERSIONS_DIR).join("0001.json"), &original).unwrap();
        // Miniatur aus dem alten Format bauen und daneben legen.
        let out = thumbnail_for(&dir, &key1(), &BuildLimits::default());
        assert!(
            !matches!(out.thumb, Thumb::Failed(_)),
            "{name}: {:?}",
            out.thumb
        );
        assert!(out.stored, "{name}");
        let s = read_version(&dir, 1).unwrap();
        let again = encode_version(&s).unwrap();
        assert_eq!(
            again.replace("\r\n", "\n"),
            text.replace("\r\n", "\n"),
            "{name}"
        );
        let direct = encode_version(&decode_version(&text, &name).unwrap()).unwrap();
        assert_eq!(again, direct, "{name}");
        assert_eq!(
            fs::read(dir.join(VERSIONS_DIR).join("0001.json")).unwrap(),
            original,
            "{name}: Versionsdatei unberührt"
        );
        assert_eq!(
            fs::read(&path).unwrap(),
            original,
            "{name}: Vorlage unberührt"
        );
        n += 1;
    }
    assert!(n >= 4, "{n} Vorlagen");
}
