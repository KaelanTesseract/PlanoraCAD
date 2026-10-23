//! Favoriten der Dateiauswahl in der Konfiguration: alte Dateien ohne das Feld laden samt
//! Wurzel, neue Dateien behalten die Favoriten. Nur mit eigenen Dateien im Temp-Ordner, nie mit
//! der echten Konfiguration.

use planora_project::config::Config;
use std::path::PathBuf;

fn temp_file(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("planora-cfg-fav-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d.join("config.json")
}

#[test]
fn an_old_config_without_favorites_keeps_its_root() {
    let p = temp_file("alt");
    std::fs::write(&p, r#"{ "root": "D:/Projekte" }"#).unwrap();
    let c = Config::load_from(&p);
    assert_eq!(c.root, Some(PathBuf::from("D:/Projekte")));
    assert!(c.favorites.is_empty());
    let _ = std::fs::remove_dir_all(p.parent().unwrap());
}

#[test]
fn favorites_round_trip_in_order() {
    let p = temp_file("rund");
    let c = Config {
        root: Some(PathBuf::from("D:/Projekte")),
        favorites: vec![PathBuf::from("D:/Teile"), PathBuf::from("C:/Ablage")],
    };
    c.save_to(&p).unwrap();
    let text = std::fs::read_to_string(&p).unwrap();
    assert!(text.contains("\"favorites\""), "{text}");
    assert_eq!(Config::load_from(&p), c);
    let _ = std::fs::remove_dir_all(p.parent().unwrap());
}

#[test]
fn unknown_fields_of_newer_versions_are_ignored() {
    let p = temp_file("neu");
    std::fs::write(
        &p,
        r#"{ "root": "D:/Projekte", "favorites": ["D:/Teile"], "spaeter": 3 }"#,
    )
    .unwrap();
    let c = Config::load_from(&p);
    assert_eq!(c.root, Some(PathBuf::from("D:/Projekte")));
    assert_eq!(c.favorites, [PathBuf::from("D:/Teile")]);
    let _ = std::fs::remove_dir_all(p.parent().unwrap());
}

#[test]
fn an_unreadable_config_is_not_overwritten_by_favorites() {
    let p = temp_file("kaputt");
    assert!(Config::safe_to_overwrite(&p), "fehlende Datei");
    Config::default().save_to(&p).unwrap();
    assert!(Config::safe_to_overwrite(&p), "lesbare Datei");
    std::fs::write(&p, r#"{ "root": "D:/Projekte", "#).unwrap();
    assert!(!Config::safe_to_overwrite(&p), "unlesbare Datei bleibt");
    let _ = std::fs::remove_dir_all(p.parent().unwrap());
}

#[test]
fn other_read_errors_also_protect_the_file() {
    // Ein Ordner an der Stelle der Datei: Lesefehler, aber nicht „fehlt“.
    let p = temp_file("ordner");
    std::fs::create_dir_all(&p).unwrap();
    assert!(Config::read_from(&p).is_err());
    assert!(!Config::safe_to_overwrite(&p));
    assert!(Config::update_at(&p, |c| c.root = None).is_err());
    assert!(p.is_dir(), "bleibt unangetastet");
    let _ = std::fs::remove_dir_all(p.parent().unwrap());
}

#[test]
fn update_at_changes_only_what_it_is_asked_to() {
    let p = temp_file("update");
    // Fehlende Datei: von der Vorgabe aus.
    let c = Config::update_at(&p, |c| c.favorites.push(PathBuf::from("D:/A"))).unwrap();
    assert_eq!(c.root, None);
    assert_eq!(Config::load_from(&p).favorites, [PathBuf::from("D:/A")]);
    std::fs::write(&p, r#"{ "root": "D:/Projekte", "favorites": ["D:/A"] }"#).unwrap();
    Config::update_at(&p, |c| c.favorites.push(PathBuf::from("D:/B"))).unwrap();
    let back = Config::load_from(&p);
    assert_eq!(back.root, Some(PathBuf::from("D:/Projekte")));
    assert_eq!(
        back.favorites,
        [PathBuf::from("D:/A"), PathBuf::from("D:/B")]
    );
    Config::update_at(&p, |c| c.root = Some(PathBuf::from("E:/Neu"))).unwrap();
    let back = Config::load_from(&p);
    assert_eq!(back.root, Some(PathBuf::from("E:/Neu")));
    assert_eq!(back.favorites.len(), 2, "Favoriten bleiben");
    // Keine Zwischendateien bleiben liegen.
    let names: Vec<String> = std::fs::read_dir(p.parent().unwrap())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, ["config.json"]);
    let _ = std::fs::remove_dir_all(p.parent().unwrap());
}

#[test]
fn a_bad_favorites_field_never_costs_the_root() {
    let p = temp_file("tolerant");
    for favs in [
        "null",
        "3",
        "\"D:/Einzeln\"",
        "{ \"a\": 1 }",
        "[1, null, \"\", \"  \", \"D:/Gut\"]",
    ] {
        std::fs::write(
            &p,
            format!(r#"{{ "root": "D:/Projekte", "favorites": {favs} }}"#),
        )
        .unwrap();
        let c = Config::load_from(&p);
        assert_eq!(c.root, Some(PathBuf::from("D:/Projekte")), "{favs}");
        let expected: Vec<PathBuf> = if favs.contains("D:/Gut") {
            vec![PathBuf::from("D:/Gut")]
        } else {
            Vec::new()
        };
        assert_eq!(c.favorites, expected, "{favs}");
        assert!(Config::safe_to_overwrite(&p), "{favs}");
    }
    let _ = std::fs::remove_dir_all(p.parent().unwrap());
}

#[test]
fn duplicate_and_surplus_favorites_are_cleaned_on_load() {
    let p = temp_file("aufraeumen");
    let mut list: Vec<String> = vec!["\"D:/Teile\"".into(), "\"D:/Teile/\"".into()];
    if cfg!(windows) {
        list.push("\"d:\\\\teile\"".into());
    }
    for i in 0..30 {
        list.push(format!("\"D:/F{i}\""));
    }
    std::fs::write(
        &p,
        format!(
            r#"{{ "root": "D:/Projekte", "favorites": [{}] }}"#,
            list.join(", ")
        ),
    )
    .unwrap();
    let c = Config::load_from(&p);
    assert_eq!(c.favorites.len(), planora_project::config::MAX_FAVORITES);
    assert_eq!(c.favorites[0], PathBuf::from("D:/Teile"));
    assert_eq!(c.favorites[1], PathBuf::from("D:/F0"), "Doppelte entfernt");
    assert_eq!(
        c.favorites[23],
        PathBuf::from("D:/F22"),
        "Reihenfolge bleibt"
    );
    let _ = std::fs::remove_dir_all(p.parent().unwrap());
}
