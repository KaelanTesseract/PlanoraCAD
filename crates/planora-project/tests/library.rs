use planora_project::ProjectError;
use planora_project::fsutil::new_id;
use planora_project::library::*;
use std::path::PathBuf;

fn lib() -> (Library, PathBuf) {
    let p = std::env::temp_dir().join(format!("planora-lib-{}", new_id()));
    (Library::open(&p).unwrap(), p)
}

fn names(nodes: &[TreeNode]) -> Vec<String> {
    nodes.iter().map(|n| n.name.clone()).collect()
}

/// Legt von Hand ein Dokument an (die echte Anlage macht der Versionsspeicher, Task 7).
fn fake_doc(l: &Library, parent: &NodePath, name: &str) -> NodePath {
    let mut path = parent.clone();
    path.push(format!("{name}.pcad"));
    let dir = l.root().resolve(&path).unwrap();
    std::fs::create_dir_all(dir.join("versionen")).unwrap();
    std::fs::write(dir.join("dokument.json"), "{}").unwrap();
    path
}

#[test]
fn projects_and_folders_form_a_sorted_tree() {
    let (l, _) = lib();
    let b = l.create_project("B-Projekt").unwrap();
    let a = l.create_project("a-Projekt").unwrap();
    let f = l.create_folder(&a, "Deckel").unwrap();
    let g = l.create_folder(&f, "Innen").unwrap();
    fake_doc(&l, &g, "Teil");
    fake_doc(&l, &b, "Roh");
    let t = l.tree().unwrap();
    assert_eq!(names(&t), ["a-Projekt", "B-Projekt"]);
    assert_eq!(t[0].kind, NodeKind::Project);
    assert_eq!(t[0].children[0].name, "Deckel");
    assert_eq!(
        t[0].children[0].children[0].children[0].kind,
        NodeKind::Document
    );
    assert_eq!(t[0].children[0].children[0].children[0].name, "Teil");
    assert_eq!(t[1].children[0].name, "Roh");
}

#[test]
fn names_and_places_are_checked() {
    let (l, _) = lib();
    let p = l.create_project("P").unwrap();
    assert!(matches!(
        l.create_project("P"),
        Err(ProjectError::Exists(_))
    ));
    assert!(matches!(
        l.create_project(".."),
        Err(ProjectError::InvalidName(_))
    ));
    assert!(
        l.create_folder(&vec![], "X").is_err(),
        "Ordner nur in Projekten"
    );
    let f = l.create_folder(&p, "F").unwrap();
    assert!(matches!(
        l.create_folder(&p, "F"),
        Err(ProjectError::Exists(_))
    ));
    let d = fake_doc(&l, &f, "D");
    assert!(
        l.create_folder(&d, "In-Dokument").is_err(),
        "nicht in einem Dokument"
    );
    assert!(l.create_project("Neu").is_ok());
    assert!(
        l.is_container(&p) && l.is_container(&f) && !l.is_container(&d) && !l.is_container(&vec![])
    );
}

#[test]
fn rename_and_move_keep_content_and_reject_collisions_and_loops() {
    let (l, _) = lib();
    let p = l.create_project("P").unwrap();
    let a = l.create_folder(&p, "A").unwrap();
    let b = l.create_folder(&p, "B").unwrap();
    let d = fake_doc(&l, &a, "Teil");
    let d2 = l.rename(&d, "Teil neu").unwrap();
    assert_eq!(d2.last().unwrap(), "Teil neu.pcad");
    let d3 = l.move_node(&d2, &b).unwrap();
    assert_eq!(
        d3,
        vec![
            "P".to_string(),
            "B".to_string(),
            "Teil neu.pcad".to_string()
        ]
    );
    assert!(l.document_dir(&d3).unwrap().join("dokument.json").exists());
    // Kollision
    fake_doc(&l, &a, "Teil neu");
    assert!(matches!(
        l.move_node(&vec!["P".into(), "A".into(), "Teil neu.pcad".into()], &b),
        Err(ProjectError::Exists(_))
    ));
    // Ordner in sich selbst oder in einen Nachfahren
    let c = l.create_folder(&a, "C").unwrap();
    assert!(l.move_node(&a, &c).is_err());
    assert!(l.move_node(&a, &a).is_err());
    // Projekte lassen sich nicht in Projekte verschieben
    let q = l.create_project("Q").unwrap();
    assert!(l.move_node(&q, &p).is_err());
    // Umbenennen mit schlechtem Namen
    assert!(l.rename(&a, "a/b").is_err());
}

#[test]
fn deleting_moves_to_the_trash_and_hides_the_node() {
    let (l, root) = lib();
    let p = l.create_project("P").unwrap();
    let d = fake_doc(&l, &p, "D");
    l.delete(&d).unwrap();
    assert!(l.tree().unwrap()[0].children.is_empty());
    assert!(root.canonicalize().unwrap().join(".papierkorb").exists());
    l.delete(&p).unwrap();
    assert!(l.tree().unwrap().is_empty());
    assert!(l.delete(&vec![]).is_err());
    assert!(l.delete(&vec!["Gibt es nicht".into()]).is_err());
}

#[test]
fn the_tree_ignores_foreign_files_and_the_trash() {
    let (l, root) = lib();
    std::fs::write(root.join("notiz.txt"), "x").unwrap();
    std::fs::create_dir_all(root.join("Kein Projekt")).unwrap();
    std::fs::create_dir_all(root.join(".papierkorb")).unwrap();
    l.create_project("Echt").unwrap();
    assert_eq!(names(&l.tree().unwrap()), ["Echt"]);
}

#[test]
fn config_round_trips_and_survives_garbage() {
    use planora_project::config::Config;
    let p = std::env::temp_dir().join(format!("planora-cfg-{}.json", new_id()));
    assert!(Config::load_from(&p).root.is_none());
    let c = Config {
        root: Some(PathBuf::from("D:/Projekte")),
        ..Config::default()
    };
    c.save_to(&p).unwrap();
    assert_eq!(Config::load_from(&p).root, c.root);
    std::fs::write(&p, "kaputt{").unwrap();
    assert!(Config::load_from(&p).root.is_none());
}

// Zusätzliche Tests (nicht aus dem Plan): Namens- und Pfadsicherheit.

#[test]
fn hostile_names_and_paths_are_rejected_and_nothing_is_written_outside() {
    let (l, root) = lib();
    let esc = format!("Escape-{}", new_id());
    let p = l.create_project("P").unwrap();
    for bad in ["", "  ", "CON", "a/b", "a\\b", "..", ".versteckt", "x.pcad"] {
        assert!(l.create_project(bad).is_err(), "Projekt {bad:?}");
        assert!(l.create_folder(&p, bad).is_err(), "Ordner {bad:?}");
    }
    let f = l.create_folder(&p, "F").unwrap();
    assert!(l.rename(&f, "NUL").is_err());
    assert!(l.rename(&f, "y.pcad").is_err());
    // Pfade mit .. oder absoluten Teilen
    assert!(l.create_folder(&vec!["..".into()], &esc).is_err());
    assert!(l.move_node(&f, &vec!["P".into(), "..".into()]).is_err());
    assert!(l.delete(&vec![".papierkorb".into()]).is_err());
    assert!(
        l.delete(&vec!["P".into(), "..".into(), "P".into()])
            .is_err()
    );
    assert!(!root.parent().unwrap().join(&esc).exists());
}

#[test]
fn an_unrelated_leaf_folder_is_not_a_document_or_container_target() {
    let (l, _) = lib();
    let p = l.create_project("P").unwrap();
    let f = l.create_folder(&p, "F").unwrap();
    // Ordner ohne dokument.json mit .pcad-Endung (von Hand angelegt) ist weder Ordner noch Dokument
    let broken = l.root().resolve(&f).unwrap().join("kaputt.pcad");
    std::fs::create_dir_all(&broken).unwrap();
    let bp: NodePath = vec!["P".into(), "F".into(), "kaputt.pcad".into()];
    assert!(!l.is_container(&bp));
    assert!(l.create_folder(&bp, "X").is_err());
    assert!(l.document_dir(&bp).is_err());
    assert!(l.tree().unwrap()[0].children[0].children.is_empty());
}
