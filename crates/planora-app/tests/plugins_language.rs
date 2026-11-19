//! Plug-ins Etappe 1, Task 5: Der Sprachwechsel in den Einstellungen erreicht die Plug-ins
//! (Beschriftungen im Menü). Eigene Datei, weil der Wechsel die globale Sprache des Prozesses
//! ändert und andere Tests sonst englische Texte sähen.

mod common;

use planora_app::plugins::Plugins;
use planora_app::workspace::Workspace;
use planora_plugin_host::EntryState;
use planora_plugin_host::package::Package;
use planora_plugin_host::places::{self as plugin_places, Places};
use planora_project::config::Config;
use std::path::PathBuf;
use std::time::{Duration, Instant};

#[test]
fn language_switch_reaches_plugins() {
    let root = common::temp_root("planora-plugins-sprache");
    let places = Places {
        bundled: vec![root.join("mitgeliefert")],
        user: root.join("benutzer"),
        cache: root.join("cache"),
    };
    let file = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../planora-plugin-host/tests/data/beispiel-hallo.pcplugin");
    let pkg = Package::read(&std::fs::read(file).unwrap()).unwrap();
    plugin_places::install(&places, &pkg).unwrap();

    let mut ws = Workspace::new(None, Config::default());
    let cfg = common::temp_root("planora-plugins-sprache-cfg");
    std::fs::create_dir_all(&cfg).unwrap();
    ws.config_path = cfg.join("config.json");
    ws.set_language(&planora_i18n::german());
    ws.plugins = Plugins::new(places, &ws.config);
    let start = Instant::now();
    while ws.plugins.host.entries()[0].state == EntryState::Loading {
        ws.plugins.update(&mut ws.config);
        assert!(start.elapsed() < Duration::from_secs(60), "lädt zu lange");
        std::thread::sleep(Duration::from_millis(10));
    }
    let label = |ws: &Workspace| ws.plugins.host.commands()[0].label.clone();
    assert_eq!(label(&ws), "Hallo sagen");
    ws.set_language(&planora_i18n::english());
    assert_eq!(label(&ws), "Say hello");
    ws.set_language(&planora_i18n::german());
    assert_eq!(label(&ws), "Hallo sagen");
}
