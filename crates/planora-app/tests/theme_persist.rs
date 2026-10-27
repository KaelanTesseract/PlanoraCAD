//! Das gewählte Design (Ansicht → Helles/Dunkles Design) wird in der Konfiguration gespeichert,
//! beim Start angewandt und kommt bei misslungenem Schreiben über jeden Schreibweg mit.
//! Konfiguration nur im Temp-Ordner.

mod common;

use planora_app::keymap::Keymap;
use planora_app::workspace::Workspace;
use planora_project::config::Config;
use std::fs;
use std::path::PathBuf;

struct Env {
    ws: Workspace,
    tmp: PathBuf,
    cfg: PathBuf,
}

fn env(prefix: &str) -> Env {
    let tmp = common::temp_root(prefix);
    fs::create_dir_all(&tmp).unwrap();
    let mut ws = Workspace::new(None, Config::default());
    let cfg = tmp.join("config.json");
    ws.config_path = cfg.clone();
    assert_ne!(cfg, Config::config_path(), "nie die echte Datei");
    Env { ws, tmp, cfg }
}

#[test]
fn switching_the_theme_writes_it_to_the_config() {
    let mut e = env("planora-design-schreiben");
    assert!(e.ws.active_tab().state.dark, "ohne Wahl dunkel");
    assert!(e.ws.set_theme(false));
    assert!(!e.ws.active_tab().state.dark);
    let disk = Config::read_from(&e.cfg).unwrap();
    assert_eq!(disk.theme.as_deref(), Some("hell"));
    assert!(e.ws.set_theme(true));
    assert!(Config::read_from(&e.cfg).unwrap().dark());
}

#[test]
fn the_saved_theme_is_applied_at_start() {
    let light = Config {
        theme: Some("hell".into()),
        ..Config::default()
    };
    let mut ws = Workspace::new(None, light);
    assert!(!ws.active_tab().state.dark);
    // auch neue Tabs übernehmen es
    ws.new_tab();
    assert!(!ws.active_tab().state.dark);
    let dark = Workspace::new(None, Config::default());
    assert!(dark.active_tab().state.dark);
}

#[test]
fn a_failed_write_rides_along_on_every_write_path() {
    for path in ["verzeichnis", "durchsicht", "tasten", "favoriten", "update"] {
        let mut e = env(&format!("planora-design-mit-{path}"));
        let blocker = e.tmp.join("blockiert");
        fs::write(&blocker, b"x").unwrap();
        e.ws.config_path = blocker.join("config.json");
        assert!(!e.ws.set_theme(false), "{path}");
        assert!(!e.ws.active_tab().state.dark, "gilt für die Sitzung");
        assert_eq!(fs::read(&blocker).unwrap(), b"x");
        e.ws.config_path = e.cfg.clone();
        let written = match path {
            "verzeichnis" => e.ws.try_persist_config(),
            "durchsicht" => e.ws.set_panel_transparency(20),
            "tasten" => e.ws.set_keymap(Keymap::default()),
            "update" => e.ws.set_update_check(false),
            _ => {
                let mut places = e.ws.places();
                places.add_favorite(&e.tmp.join("Teile")).unwrap();
                e.ws.save_places(places)
            }
        };
        assert!(written, "{path}");
        assert_eq!(
            Config::read_from(&e.cfg).unwrap().theme.as_deref(),
            Some("hell"),
            "{path}"
        );
        // danach nicht mehr vorgemerkt: eine Änderung der Datei von außen bleibt
        Config::update_at(&e.cfg, |c| c.theme = Some("dunkel".into())).unwrap();
        assert!(e.ws.set_panel_transparency(30), "{path}");
        assert_eq!(
            Config::read_from(&e.cfg).unwrap().theme.as_deref(),
            Some("dunkel"),
            "{path}"
        );
    }
}

#[test]
fn other_writes_keep_the_theme_and_the_theme_keeps_other_fields() {
    let mut e = env("planora-design-erhalten");
    assert!(e.ws.set_panel_transparency(25));
    assert!(e.ws.set_theme(false));
    assert!(e.ws.set_panel_transparency(35));
    let disk = Config::read_from(&e.cfg).unwrap();
    assert_eq!(disk.theme.as_deref(), Some("hell"));
    assert_eq!(disk.panel_transparency, 35);
}
