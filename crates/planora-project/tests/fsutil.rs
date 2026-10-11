use planora_project::ProjectError;
use planora_project::fsutil::*;
use std::path::{Path, PathBuf};

fn tmp(name: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("planora-test-{name}-{}", new_id()));
    std::fs::create_dir_all(&p).unwrap();
    p
}

/// Legt eine Verknüpfung (Symlink bzw. Junction) `link` -> `target` an; `false`, wenn das auf
/// diesem System nicht erlaubt ist.
fn make_link(link: &Path, target: &Path) -> bool {
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(target, link).is_ok()
    }
    #[cfg(windows)]
    {
        std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(link)
            .arg(target)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }
}

#[test]
fn names_are_trimmed_and_checked() {
    assert_eq!(validate_name("  Gehäuse 2  ").unwrap(), "Gehäuse 2");
    for bad in [
        "",
        "   ",
        ".",
        "..",
        "a/b",
        "a\\b",
        "a:b",
        "a*b",
        "a?b",
        "a\"b",
        "a<b",
        "a>b",
        "a|b",
        "CON",
        "con",
        "NUL",
        "COM1",
        "lpt9",
        "COM0",
        "lpt0",
        "CONIN$",
        "conout$",
        "CONOUT$.txt",
        "AUX.txt",
        "name.",
        ".versteckt",
        "x\u{0}y",
        &"a".repeat(MAX_NAME_CHARS + 1),
        "x\ny",
    ] {
        assert!(
            matches!(validate_name(bad), Err(ProjectError::InvalidName(_))),
            "{bad:?} muss abgelehnt werden"
        );
    }
    assert!(validate_name(&"ä".repeat(MAX_NAME_CHARS)).is_ok());
}

#[test]
fn atomic_write_replaces_the_file_and_leaves_no_temp_file() {
    let d = tmp("aw");
    let f = d.join("a.json");
    atomic_write(&f, b"eins").unwrap();
    atomic_write(&f, b"zwei").unwrap();
    assert_eq!(std::fs::read(&f).unwrap(), b"zwei");
    let names: Vec<_> = std::fs::read_dir(&d)
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(names.len(), 1, "{names:?}");
}

#[test]
fn ids_are_unique_and_times_look_like_iso() {
    let ids: std::collections::HashSet<_> = (0..1000).map(|_| new_id()).collect();
    assert_eq!(ids.len(), 1000);
    assert!(
        ids.iter()
            .all(|i| i.len() == 32 && i.chars().all(|c| c.is_ascii_hexdigit()))
    );
    let t = now_iso();
    assert_eq!(t.len(), 20, "{t}");
    assert!(
        t.ends_with('Z') && &t[4..5] == "-" && &t[10..11] == "T",
        "{t}"
    );
}

#[test]
fn resolve_stays_inside_the_root() {
    let d = tmp("root");
    let r = Root::open(&d).unwrap();
    let ok = r.resolve(&["Projekt".into(), "Ordner".into()]).unwrap();
    assert!(ok.starts_with(r.path()));
    for parts in [
        vec!["..".to_string()],
        vec!["a".to_string(), "..".to_string(), "..".to_string()],
        vec!["C:\\Windows".to_string()],
        vec!["/etc".to_string()],
        vec!["CON".to_string()],
        vec![String::new()],
    ] {
        assert!(r.resolve(&parts).is_err(), "{parts:?}");
    }
}

#[test]
fn a_link_leading_out_of_the_root_is_refused() {
    let d = tmp("link");
    let outside = tmp("outside");
    let r = Root::open(&d).unwrap();
    // Symlink bzw. Junction nach außen anlegen; ist das auf diesem System nicht erlaubt
    // (Windows ohne Rechte), wird der Test übersprungen.
    let link = d.join("Aussen");
    if !make_link(&link, &outside) {
        eprintln!("Verknüpfung nicht anlegbar – Test übersprungen");
        return;
    }
    assert!(r.resolve(&["Aussen".into(), "x".into()]).is_err());
}

#[test]
fn a_link_to_a_lookalike_sibling_directory_is_refused() {
    // Wurzel „…/Wurzel“, Nachbar „…/Wurzel-boese“: ein reiner Zeichenketten-Präfixvergleich
    // würde den Nachbarn fälschlich als innerhalb ansehen.
    let base = tmp("lookalike");
    let root_dir = base.join("Wurzel");
    let evil = base.join("Wurzel-boese");
    std::fs::create_dir_all(&evil).unwrap();
    let r = Root::open(&root_dir).unwrap();
    let link = root_dir.join("Verweis");
    if !make_link(&link, &evil) {
        eprintln!("Verknüpfung nicht anlegbar – Test übersprungen");
        return;
    }
    assert!(r.resolve(&["Verweis".into()]).is_err());
    assert!(r.resolve(&["Verweis".into(), "neu".into()]).is_err());
    assert!(r.to_trash(&["Verweis".into()]).is_err());
    assert!(evil.exists());
}

#[test]
fn dot_dot_and_lookalike_parts_never_leave_the_root() {
    // Zweites Mittel ohne Verknüpfungen: Teile mit „..“, Laufwerks- oder UNC-Anteilen,
    // abschließendem Punkt oder Leerzeichen und Alternativ-Datenströmen werden abgelehnt.
    let base = tmp("dotdot");
    let root_dir = base.join("Wurzel");
    std::fs::create_dir_all(base.join("Wurzel-boese")).unwrap();
    let r = Root::open(&root_dir).unwrap();
    for part in [
        "..",
        "../Wurzel-boese",
        "..\\Wurzel-boese",
        "\\\\server\\share",
        "x.",
        "x:strom",
        "Wurzel-boese/..",
        "a/b",
    ] {
        assert!(r.resolve(&[part.to_string()]).is_err(), "{part:?}");
    }
    // Ein Teil mit abschließendem Leerzeichen wird getrimmt abgelehnt oder bleibt innerhalb.
    if let Ok(p) = r.resolve(&["x ".to_string()]) {
        assert!(p.starts_with(r.path()));
    }
    let ok = r.resolve(&["Projekt.pcad".into()]).unwrap();
    assert!(ok.starts_with(r.path()));
}

#[test]
fn trash_moves_instead_of_deleting() {
    let d = tmp("trash");
    let r = Root::open(&d).unwrap();
    let p = r.resolve(&["Projekt".into()]).unwrap();
    std::fs::create_dir_all(&p).unwrap();
    std::fs::write(p.join("datei.txt"), "x").unwrap();
    r.to_trash(&["Projekt".into()]).unwrap();
    assert!(!p.exists());
    let trash = r.path().join(".papierkorb");
    let entry = std::fs::read_dir(&trash).unwrap().next().unwrap().unwrap();
    assert!(entry.file_name().to_string_lossy().contains("Projekt"));
    assert!(entry.path().join("datei.txt").exists());
    // zweimal dasselbe löschen kollidiert nicht
    std::fs::create_dir_all(&p).unwrap();
    r.to_trash(&["Projekt".into()]).unwrap();
    assert_eq!(std::fs::read_dir(&trash).unwrap().count(), 2);
    assert!(r.to_trash(&[".papierkorb".into()]).is_err());
}

#[test]
fn a_lock_blocks_a_second_holder_until_it_is_dropped_or_taken_over() {
    let d = tmp("lock");
    let a = DocLock::acquire(&d).unwrap();
    assert!(matches!(
        DocLock::acquire(&d),
        Err(ProjectError::Locked { .. })
    ));
    assert!(lock_age(&d).is_some());
    let b = DocLock::take_over(&d).unwrap();
    // `a` löscht beim Drop nur, was ihm gehört: die übernommene Sperre `b` bleibt bestehen.
    drop(a);
    assert!(lock_age(&d).is_some());
    drop(b);
    assert!(lock_age(&d).is_none());
    let _c = DocLock::acquire(&d).unwrap();
}
