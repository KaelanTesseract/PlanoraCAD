//! Update-Felder der Konfiguration (`update_channel`, `update_check`, `last_update_check`,
//! `skipped_version`, `update_pending`): fehlen sie, gelten die Voreinstellungen; ein falscher
//! Typ macht die Datei nicht unlesbar und kostet keine anderen Felder; geschrieben wird nur, was
//! von der Voreinstellung abweicht; andere Einstellungen und die Update-Felder überschreiben
//! einander beim Lesen-Ändern-Schreiben nicht. Nur mit eigenen Dateien im Temp-Ordner.

use planora_project::config::{Config, DEFAULT_UPDATE_CHANNEL, PendingUpdate, WindowState};
use std::collections::BTreeMap;
use std::path::PathBuf;

fn temp_file(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("planora-cfg-update-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d.join("config.json")
}

fn cleanup(p: &std::path::Path) {
    let _ = std::fs::remove_dir_all(p.parent().unwrap());
}

/// Eine Änderung an der Konfiguration (für die Tabelle der Abweichungen).
type Change = fn(&mut Config);

const UPDATE_KEYS: [&str; 5] = [
    "update_channel",
    "update_check",
    "last_update_check",
    "skipped_version",
    "update_pending",
];

#[test]
fn defaults_for_old_files() {
    assert_eq!(DEFAULT_UPDATE_CHANNEL, "dev");
    let d = Config::default();
    assert_eq!(d.update_channel, "dev");
    assert!(d.update_check);
    assert_eq!(d.last_update_check, None);
    assert_eq!(d.skipped_version, None);
    assert_eq!(d.update_pending, None);

    let p = temp_file("alt");
    std::fs::write(
        &p,
        r#"{ "root": "D:/Projekte", "favorites": ["D:/A"], "panel_transparency": 30 }"#,
    )
    .unwrap();
    let c = Config::read_from(&p).unwrap();
    assert_eq!(c.update_channel, "dev");
    assert!(c.update_check);
    assert_eq!(c.last_update_check, None);
    assert_eq!(c.skipped_version, None);
    assert_eq!(c.update_pending, None);
    assert_eq!(c.root, Some(PathBuf::from("D:/Projekte")));
    assert_eq!(c.panel_transparency, 30);
    // Fehlende Datei: ebenfalls die Voreinstellungen.
    let missing = Config::load_from(&p.with_file_name("fehlt.json"));
    assert_eq!(missing.update_channel, "dev");
    assert!(missing.update_check);
    cleanup(&p);
}

#[test]
fn unknown_channel_reads_as_dev() {
    let p = temp_file("kanal");
    for (text, want) in [
        ("\"dev\"", "dev"),
        ("\"beta\"", "beta"),
        ("\"stable\"", "stable"),
        ("\"nightly\"", "dev"),
        ("\"\"", "dev"),
        ("\" dev\"", "dev"),
        ("\"Dev\"", "dev"),
    ] {
        std::fs::write(
            &p,
            format!(r#"{{ "root": "D:/Projekte", "update_channel": {text} }}"#),
        )
        .unwrap();
        let c = Config::read_from(&p).unwrap_or_else(|e| panic!("{text}: {e}"));
        assert_eq!(c.update_channel, want, "{text}");
        assert_eq!(c.root, Some(PathBuf::from("D:/Projekte")), "{text}");
    }
    cleanup(&p);
}

#[test]
fn broken_types_read_as_defaults_and_keep_other_fields() {
    let p = temp_file("kaputt");
    let broken = [
        ("update_channel", "5"),
        ("update_channel", "null"),
        ("update_channel", "[\"dev\"]"),
        ("update_check", "\"ja\""),
        ("update_check", "1"),
        ("update_check", "null"),
        ("last_update_check", "\"gestern\""),
        ("last_update_check", "-5"),
        ("last_update_check", "1.5"),
        ("last_update_check", "null"),
        ("skipped_version", "5"),
        ("skipped_version", "\"\""),
        ("skipped_version", "{}"),
        ("update_pending", "\"0.2.0\""),
        ("update_pending", "{ \"from\": \"0.1.0\" }"),
        ("update_pending", "{ \"from\": 1, \"to\": \"0.2.0\" }"),
        ("update_pending", "[]"),
        ("update_pending", "null"),
    ];
    for (key, text) in broken {
        std::fs::write(
            &p,
            format!(
                r#"{{ "root": "D:/Projekte", "favorites": ["D:/A"], "panel_transparency": 25,
                     "keys": {{ "datei.neu": ["Strg+N"] }}, "{key}": {text} }}"#
            ),
        )
        .unwrap();
        let c = Config::read_from(&p).unwrap_or_else(|e| panic!("{key} = {text}: {e}"));
        let what = format!("{key} = {text}");
        assert_eq!(c.update_channel, "dev", "{what}");
        assert!(c.update_check, "{what}");
        assert_eq!(c.last_update_check, None, "{what}");
        assert_eq!(c.skipped_version, None, "{what}");
        assert_eq!(c.update_pending, None, "{what}");
        assert_eq!(c.root, Some(PathBuf::from("D:/Projekte")), "{what}");
        assert_eq!(c.favorites, [PathBuf::from("D:/A")], "{what}");
        assert_eq!(c.panel_transparency, 25, "{what}");
        assert_eq!(c.keys["datei.neu"], ["Strg+N"], "{what}");
    }
    // Ein kaputtes Feld kostet die anderen Update-Felder nicht.
    std::fs::write(
        &p,
        r#"{ "update_channel": 7, "update_check": false, "last_update_check": 1700000000,
             "skipped_version": "0.2.0-dev.3", "update_pending": { "from": "0.2.0-dev.1", "to": "0.2.0-dev.2" } }"#,
    )
    .unwrap();
    let c = Config::read_from(&p).unwrap();
    assert_eq!(c.update_channel, "dev");
    assert!(!c.update_check);
    assert_eq!(c.last_update_check, Some(1_700_000_000));
    assert_eq!(c.skipped_version.as_deref(), Some("0.2.0-dev.3"));
    assert_eq!(
        c.update_pending,
        Some(PendingUpdate {
            from: "0.2.0-dev.1".into(),
            to: "0.2.0-dev.2".into(),
            window: None,
        })
    );
    cleanup(&p);
}

#[test]
fn only_changed_fields_are_written() {
    let p = temp_file("abweichung");
    Config::default().save_to(&p).unwrap();
    let text = std::fs::read_to_string(&p).unwrap();
    for key in UPDATE_KEYS {
        assert!(!text.contains(&format!("\"{key}\"")), "{key}: {text}");
    }
    // Jede Abweichung erscheint einzeln, die anderen Felder bleiben weg.
    let changes: [(&str, Change); 5] = [
        ("update_channel", |c| c.update_channel = "beta".into()),
        ("update_check", |c| c.update_check = false),
        ("last_update_check", |c| c.last_update_check = Some(1)),
        ("skipped_version", |c| {
            c.skipped_version = Some("0.2.0".into())
        }),
        ("update_pending", |c| {
            c.update_pending = Some(PendingUpdate {
                from: "0.1.0".into(),
                to: "0.2.0".into(),
                window: None,
            })
        }),
    ];
    for (key, change) in changes {
        let mut c = Config::default();
        change(&mut c);
        c.save_to(&p).unwrap();
        let text = std::fs::read_to_string(&p).unwrap();
        for other in UPDATE_KEYS {
            assert_eq!(
                text.contains(&format!("\"{other}\"")),
                other == key,
                "{key} geändert, {other}: {text}"
            );
        }
        assert_eq!(Config::read_from(&p).unwrap(), c, "{key}");
    }
    // Der Kanal „dev“ ist die Voreinstellung und wird nicht geschrieben.
    let c = Config {
        update_channel: "dev".into(),
        ..Config::default()
    };
    c.save_to(&p).unwrap();
    assert!(
        !std::fs::read_to_string(&p)
            .unwrap()
            .contains("update_channel")
    );
    cleanup(&p);
}

#[test]
fn update_fields_survive_other_config_updates_and_vice_versa() {
    let p = temp_file("andere");
    let pending = PendingUpdate {
        from: "0.2.0-dev.1".into(),
        to: "0.2.0-dev.2".into(),
        window: Some(WindowState {
            x: -8.0,
            y: 12.5,
            width: 1920.0,
            height: 1050.0,
            maximized: true,
        }),
    };
    Config {
        update_channel: "beta".into(),
        update_check: false,
        last_update_check: Some(1_761_400_000),
        skipped_version: Some("0.2.0-dev.5".into()),
        update_pending: Some(pending.clone()),
        ..Config::default()
    }
    .save_to(&p)
    .unwrap();
    let k: BTreeMap<String, Vec<String>> = [("datei.neu".to_string(), vec!["Alt+N".to_string()])]
        .into_iter()
        .collect();
    Config::update_at(&p, |c| c.root = Some(PathBuf::from("D:/Neu"))).unwrap();
    Config::update_at(&p, |c| c.favorites = vec![PathBuf::from("D:/Fav")]).unwrap();
    Config::update_at(&p, |c| c.panel_transparency = 12).unwrap();
    Config::update_at(&p, |c| c.keys = k.clone()).unwrap();
    let disk = Config::read_from(&p).unwrap();
    assert_eq!(disk.update_channel, "beta");
    assert!(!disk.update_check);
    assert_eq!(disk.last_update_check, Some(1_761_400_000));
    assert_eq!(disk.skipped_version.as_deref(), Some("0.2.0-dev.5"));
    assert_eq!(disk.update_pending, Some(pending));
    assert_eq!(disk.root, Some(PathBuf::from("D:/Neu")));
    assert_eq!(disk.favorites, [PathBuf::from("D:/Fav")]);
    assert_eq!(disk.panel_transparency, 12);
    assert_eq!(disk.keys, k);

    // Umgekehrt: Update-Felder ändern lässt die anderen Einstellungen stehen.
    Config::update_at(&p, |c| {
        c.update_channel = "dev".into();
        c.update_check = true;
        c.last_update_check = Some(1_761_500_000);
        c.skipped_version = None;
        c.update_pending = None;
    })
    .unwrap();
    let disk = Config::read_from(&p).unwrap();
    assert_eq!(disk.root, Some(PathBuf::from("D:/Neu")));
    assert_eq!(disk.favorites, [PathBuf::from("D:/Fav")]);
    assert_eq!(disk.panel_transparency, 12);
    assert_eq!(disk.keys, k);
    assert_eq!(disk.last_update_check, Some(1_761_500_000));
    let text = std::fs::read_to_string(&p).unwrap();
    for key in [
        "update_channel",
        "update_check",
        "skipped_version",
        "update_pending",
    ] {
        assert!(!text.contains(&format!("\"{key}\"")), "{key}: {text}");
    }
    cleanup(&p);
}

#[test]
fn pending_update_roundtrips() {
    let p = temp_file("vormerkung");
    let c = Config {
        update_pending: Some(PendingUpdate {
            from: "0.2.0-dev.1".into(),
            to: "0.2.0-dev.2".into(),
            window: None,
        }),
        ..Config::default()
    };
    c.save_to(&p).unwrap();
    let text = std::fs::read_to_string(&p).unwrap();
    assert!(text.contains("\"from\": \"0.2.0-dev.1\""), "{text}");
    assert!(text.contains("\"to\": \"0.2.0-dev.2\""), "{text}");
    assert_eq!(Config::read_from(&p).unwrap(), c);
    // Zusätzliche Felder (neuerer Programmstand) stören nicht.
    std::fs::write(
        &p,
        r#"{ "update_pending": { "from": "0.2.0-dev.1", "to": "0.2.0-dev.2", "neu": 1 } }"#,
    )
    .unwrap();
    assert_eq!(
        Config::read_from(&p).unwrap().update_pending,
        c.update_pending
    );
    cleanup(&p);
}

/// Fenster der alten Version beim Neustart für ein Update (`update_pending.window`): wird mit dem
/// Eintrag geschrieben und gelesen; fehlt es oder ist es kaputt, gibt es keines, der Eintrag selbst
/// bleibt aber lesbar. Ältere Dateien ohne das Feld lesen sich wie bisher.
#[test]
fn pending_update_carries_the_window_of_the_old_version() {
    let p = temp_file("fenster");
    let window = WindowState {
        x: 100.0,
        y: 50.0,
        width: 1600.0,
        height: 900.0,
        maximized: false,
    };
    let c = Config {
        update_pending: Some(PendingUpdate {
            from: "0.2.0-dev.1".into(),
            to: "0.2.0-dev.2".into(),
            window: Some(window),
        }),
        ..Config::default()
    };
    c.save_to(&p).unwrap();
    assert_eq!(Config::read_from(&p).unwrap(), c);
    // Ohne Fenster: das Feld wird nicht geschrieben.
    let mut without = c.clone();
    without.update_pending.as_mut().unwrap().window = None;
    without.save_to(&p).unwrap();
    assert!(!std::fs::read_to_string(&p).unwrap().contains("window"));
    // Kaputte Angaben: kein Fenster, der Eintrag bleibt.
    for broken in [
        r#""gross""#,
        r#"{"x": 1, "y": 2, "width": "breit", "height": 900}"#,
        r#"{"x": 1, "y": 2, "width": 0, "height": 900}"#,
        r#"{"x": 1, "y": 2, "width": -5, "height": 900}"#,
        r#"{"x": 1, "y": 2}"#,
    ] {
        std::fs::write(
            &p,
            format!(r#"{{"root": null, "update_pending": {{"from": "0.1.0", "to": "0.2.0", "window": {broken}}}}}"#),
        )
        .unwrap();
        let pending = Config::read_from(&p).unwrap().update_pending.unwrap();
        assert_eq!(pending.to, "0.2.0", "{broken}");
        assert_eq!(pending.window, None, "{broken}");
    }
    // „maximized“ fehlt: nicht maximiert.
    std::fs::write(
        &p,
        r#"{"root": null, "update_pending": {"from": "0.1.0", "to": "0.2.0", "window": {"x": 1, "y": 2, "width": 1000, "height": 700}}}"#,
    )
    .unwrap();
    let w = Config::read_from(&p)
        .unwrap()
        .update_pending
        .unwrap()
        .window
        .unwrap();
    assert!(!w.maximized);
    assert_eq!((w.x, w.y, w.width, w.height), (1.0, 2.0, 1000.0, 700.0));
    cleanup(&p);
}
