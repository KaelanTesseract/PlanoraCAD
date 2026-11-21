//! Fundorte, Installieren und Zwischenspeicher (Plug-ins Etappe 1, Task 4). Alles in eigenen
//! Temp-Ordnern, nie in den echten Programm- oder Benutzerordnern.

use planora_plugin_host::PluginHost;
use planora_plugin_host::package::Package;
use planora_plugin_host::places::{self, Origin, Places};
use planora_plugin_host::runtime::{Dialog, Level, Right};
use planora_plugin_host::runtime::{Engine, HostCalls, HostInfo};
use std::ffi::OsString;
use std::path::{Path, PathBuf};

fn data(file: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/data")
        .join(file)
}

fn paket(name: &str) -> Package {
    let bytes = std::fs::read(data(&format!("{name}.pcplugin"))).expect("Paket fehlt");
    Package::read(&bytes).expect("Paket nicht lesbar")
}

/// Eigene Fundorte unter dem Temp-Ordner (vorher geleert).
fn orte(name: &str) -> Places {
    let root = std::env::temp_dir().join(format!(
        "planora-plugin-places-{}-{name}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    Places {
        bundled: vec![root.join("mitgeliefert")],
        user: root.join("benutzer"),
        cache: root.join("cache"),
    }
}

fn aufraeumen(p: &Places) {
    let _ = std::fs::remove_dir_all(p.user.parent().unwrap());
}

/// Packt `pkg` als mitgeliefertes Plug-in aus (wie der Installer).
fn mitliefern(p: &Places, pkg: &Package) -> PathBuf {
    let dir = p.bundled[0].join(&pkg.manifest.id);
    pkg.unpack_to(&dir).unwrap();
    dir
}

/// Setzt die Version in `plugin.toml` eines ausgepackten Plug-ins.
fn version_setzen(dir: &Path, version: &str) {
    let path = dir.join("plugin.toml");
    let text = std::fs::read_to_string(&path).unwrap();
    let neu = text.replace("version = \"0.1.0\"", &format!("version = \"{version}\""));
    assert_ne!(text, neu, "Version nicht gefunden");
    std::fs::write(path, neu).unwrap();
}

#[test]
fn scan_both_origins() {
    let p = orte("beide");
    mitliefern(&p, &paket("beispiel-hallo"));
    places::install(&p, &paket("test-faelle"), None).unwrap();
    let found = places::scan(&p);
    assert_eq!(found.len(), 2, "{found:?}");
    let hallo = found
        .iter()
        .find(|f| f.manifest.id == "planora.beispiel-hallo")
        .unwrap();
    assert_eq!(hallo.origin, Origin::Bundled);
    assert_eq!(hallo.problem, None);
    let faelle = found
        .iter()
        .find(|f| f.manifest.id == "planora.test-faelle")
        .unwrap();
    assert_eq!(faelle.origin, Origin::User);
    assert_eq!(faelle.problem, None);
    assert_eq!(faelle.dir, p.user.join("planora.test-faelle"));
    aufraeumen(&p);
}

#[test]
fn same_id_higher_version_wins() {
    let p = orte("version");
    let pkg = paket("beispiel-hallo");
    let bundled = mitliefern(&p, &pkg);
    let user = places::install(&p, &pkg, None).unwrap();
    version_setzen(&user, "0.2.0");

    let host = PluginHost::new(p.clone());
    let e = &host.entries()[0];
    assert_eq!(host.entries().len(), 1);
    assert_eq!(e.found.len(), 2, "die Verwaltung zeigt beide");
    let active = &e.found[e.active.unwrap()];
    assert_eq!(active.origin, Origin::User);
    assert_eq!(active.manifest.version.to_string(), "0.2.0");

    version_setzen(&bundled, "0.3.0");
    let host = PluginHost::new(p.clone());
    let e = &host.entries()[0];
    let active = &e.found[e.active.unwrap()];
    assert_eq!(active.origin, Origin::Bundled);
    assert_eq!(active.manifest.version.to_string(), "0.3.0");
    aufraeumen(&p);
}

#[test]
fn broken_plugin_listed_with_problem() {
    let p = orte("kaputt");
    mitliefern(&p, &paket("beispiel-hallo"));
    let broken = p.bundled[0].join("planora.kaputt");
    std::fs::create_dir_all(&broken).unwrap();
    std::fs::write(broken.join("plugin.toml"), "das ist kein toml = = =").unwrap();
    let found = places::scan(&p);
    assert_eq!(found.len(), 2, "{found:?}");
    let k = found.iter().find(|f| f.dir == broken).unwrap();
    assert!(k.problem.is_some());
    let ok = found
        .iter()
        .find(|f| f.manifest.id == "planora.beispiel-hallo")
        .unwrap();
    assert_eq!(ok.problem, None);

    let host = PluginHost::new(p.clone());
    let kaputt = host
        .entries()
        .iter()
        .find(|e| e.found.iter().any(|f| f.dir == broken))
        .unwrap();
    assert_eq!(kaputt.active, None);
    aufraeumen(&p);
}

/// Namen im Benutzerordner, die mit `prefix` beginnen.
fn reste(dir: &Path, prefix: &str) -> Vec<String> {
    std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with(prefix))
        .collect()
}

#[test]
fn install_replaces_atomically() {
    let p = orte("ersetzen");
    let pkg = paket("beispiel-hallo");
    let first = places::install(&p, &pkg, None).unwrap();
    std::fs::write(first.join("alt.txt"), "vom ersten Mal").unwrap();
    let second = places::install(&p, &pkg, None).unwrap();
    assert_eq!(first, second);
    assert_eq!(second, p.user.join("planora.beispiel-hallo"));
    assert!(second.join("plugin.toml").is_file());
    assert!(second.join("plugin.wasm").is_file());
    assert!(
        !second.join("alt.txt").exists(),
        "der alte Ordner wurde ersetzt"
    );
    assert!(reste(&p.user, ".tmp-").is_empty());
    assert!(reste(&p.user, ".alt-").is_empty());
    aufraeumen(&p);
}

#[test]
fn uninstall_bundled_refused() {
    let p = orte("entfernen");
    let pkg = paket("beispiel-hallo");
    let bundled = mitliefern(&p, &pkg);
    assert!(places::uninstall(&p, "planora.beispiel-hallo").is_err());
    assert!(
        bundled.join("plugin.toml").is_file(),
        "Mitgeliefertes bleibt"
    );

    let user = places::install(&p, &paket("test-faelle"), None).unwrap();
    places::uninstall(&p, "planora.test-faelle").unwrap();
    assert!(!user.exists());
    // Gibt es beides, entfernt `uninstall` nur das installierte.
    let user = places::install(&p, &pkg, None).unwrap();
    places::uninstall(&p, "planora.beispiel-hallo").unwrap();
    assert!(!user.exists());
    assert!(bundled.join("plugin.toml").is_file());
    aufraeumen(&p);
}

/// Ein Host, der nichts tut (für den Start nach dem Laden aus dem Zwischenspeicher).
struct Still;

impl HostCalls for Still {
    fn log(&mut self, _level: Level, _text: &str) {}
    fn message(&mut self, _level: Level, _text: &str) {}
    fn open_dialog(&mut self, _d: Dialog) -> Result<(), String> {
        Ok(())
    }
    fn close_dialog(&mut self, _id: &str) {}
    fn request_right(&mut self, _r: Right) -> u64 {
        1
    }
    fn right_granted(&self, _r: Right) -> bool {
        false
    }
    fn net_get(&mut self, _url: &str) -> Result<u64, String> {
        Err("nein".into())
    }
}

fn cwasm_dateien(cache: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(cache)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "cwasm"))
        .collect()
}

#[test]
fn cache_reused_and_repaired() {
    let p = orte("cache");
    let pkg = paket("beispiel-hallo");
    let engine = Engine::new().unwrap();
    places::load_component(&engine, &p.cache, &pkg.wasm).unwrap();
    let files = cwasm_dateien(&p.cache);
    assert_eq!(files.len(), 1, "{files:?}");
    let file = &files[0];
    let len = std::fs::metadata(file).unwrap().len();
    let written = std::fs::metadata(file).unwrap().modified().unwrap();

    // Zweites Laden liest die Datei, statt neu zu übersetzen und zu schreiben.
    std::thread::sleep(std::time::Duration::from_millis(50));
    let c = places::load_component(&engine, &p.cache, &pkg.wasm).unwrap();
    assert_eq!(
        std::fs::metadata(file).unwrap().modified().unwrap(),
        written
    );
    let mut inst = engine
        .instantiate_component(&pkg.manifest, &c, &pkg.languages, Box::new(Still))
        .unwrap();
    let info = HostInfo {
        api_version: "0.1".into(),
        program_version: "0.0.0".into(),
        language: "de".into(),
    };
    assert_eq!(inst.init(info.clone()).unwrap().commands.len(), 1);

    // Abgeschnittene Datei: neu übersetzen, kein Fehler, Datei wieder heil.
    let bytes = std::fs::read(file).unwrap();
    std::fs::write(file, &bytes[..bytes.len() / 2]).unwrap();
    let c = places::load_component(&engine, &p.cache, &pkg.wasm).unwrap();
    assert_eq!(std::fs::metadata(file).unwrap().len(), len);
    let mut inst = engine
        .instantiate_component(&pkg.manifest, &c, &pkg.languages, Box::new(Still))
        .unwrap();
    assert_eq!(inst.init(info).unwrap().commands.len(), 1);
    // Keine Zwischendateien bleiben liegen.
    assert_eq!(std::fs::read_dir(&p.cache).unwrap().count(), 1);
    aufraeumen(&p);
}

#[test]
fn places_linux_and_windows() {
    let env = |vars: &'static [(&'static str, &'static str)]| {
        move |k: &str| {
            vars.iter()
                .find(|(n, _)| *n == k)
                .map(|(_, v)| OsString::from(*v))
        }
    };

    let win = env(&[
        ("APPDATA", "C:/Users/a/AppData/Roaming"),
        ("LOCALAPPDATA", "C:/Users/a/AppData/Local"),
        ("XDG_DATA_HOME", "/falsch"),
    ]);
    let p = Places::from_env(
        Path::new("C:/Programme/PlanoraCAD/planora-cad.exe"),
        &win,
        true,
    );
    assert_eq!(
        p.bundled,
        vec![PathBuf::from("C:/Programme/PlanoraCAD").join("plugins")]
    );
    assert_eq!(
        p.user,
        PathBuf::from("C:/Users/a/AppData/Roaming")
            .join("PlanoraCAD")
            .join("plugins")
    );
    assert_eq!(
        p.cache,
        PathBuf::from("C:/Users/a/AppData/Local")
            .join("PlanoraCAD")
            .join("plugin-cache")
    );

    let linux = env(&[
        ("HOME", "/home/a"),
        ("XDG_DATA_HOME", "/home/a/daten"),
        ("XDG_CACHE_HOME", "/home/a/zwischen"),
        ("APPDATA", "/falsch"),
    ]);
    let exe = Path::new("/usr/bin/planora-cad");
    let p = Places::from_env(exe, &linux, false);
    assert_eq!(
        p.bundled,
        vec![
            PathBuf::from("/usr/bin").join("plugins"),
            PathBuf::from("/usr/bin")
                .join("..")
                .join("share")
                .join("planora-cad")
                .join("plugins"),
        ]
    );
    assert_eq!(
        p.user,
        PathBuf::from("/home/a/daten")
            .join("planora-cad")
            .join("plugins")
    );
    assert_eq!(
        p.cache,
        PathBuf::from("/home/a/zwischen")
            .join("planora-cad")
            .join("plugin-cache")
    );

    // Ohne XDG-Variablen: die üblichen Ordner unter HOME.
    let nur_home = env(&[("HOME", "/home/a")]);
    let p = Places::from_env(exe, &nur_home, false);
    assert_eq!(
        p.user,
        PathBuf::from("/home/a")
            .join(".local")
            .join("share")
            .join("planora-cad")
            .join("plugins")
    );
    assert_eq!(
        p.cache,
        PathBuf::from("/home/a")
            .join(".cache")
            .join("planora-cad")
            .join("plugin-cache")
    );
}

fn herkunft() -> places::Provenance {
    places::Provenance {
        link: "https://github.com/a/b".to_string(),
        tag: "v0.1.0".to_string(),
        catalog: false,
    }
}

#[test]
fn provenance_roundtrip() {
    let p = orte("herkunft-hin-zurueck");
    let dir = p.user.join("x");
    std::fs::create_dir_all(&dir).unwrap();
    assert_eq!(places::read_provenance(&dir), None);
    places::write_provenance(&dir, &herkunft()).unwrap();
    assert!(dir.join("herkunft.json").is_file());
    assert_eq!(places::read_provenance(&dir), Some(herkunft()));
    aufraeumen(&p);
}

#[test]
fn provenance_lenient() {
    let p = orte("herkunft-nachsichtig");
    let dir = places::install(&p, &paket("beispiel-hallo"), None).unwrap();
    for kaputt in [
        "{kaputt",
        "[]",
        r#"{"link":"https://github.com/a/b"}"#,
        r#"{"link":5,"tag":"v1"}"#,
        r#"{"link":"","tag":"v1"}"#,
    ] {
        std::fs::write(dir.join("herkunft.json"), kaputt).unwrap();
        assert_eq!(places::read_provenance(&dir), None, "{kaputt}");
        // Das Plug-in bleibt startbar.
        let found = places::scan(&p);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].problem, None, "{kaputt}");
    }
    // Weitere Felder stören nicht.
    std::fs::write(
        dir.join("herkunft.json"),
        r#"{"link":"https://github.com/a/b","tag":"v0.1.0","neu":1}"#,
    )
    .unwrap();
    assert_eq!(places::read_provenance(&dir), Some(herkunft()));
    aufraeumen(&p);
}

#[test]
fn install_writes_provenance() {
    let p = orte("herkunft-installieren");
    let pkg = paket("beispiel-hallo");
    let dir = places::install(&p, &pkg, Some(&herkunft())).unwrap();
    assert_eq!(places::read_provenance(&dir), Some(herkunft()));
    // Aus einer Datei neu installiert: der Ordner ist neu, ohne Link.
    let dir = places::install(&p, &pkg, None).unwrap();
    assert_eq!(places::read_provenance(&dir), None);
    aufraeumen(&p);
}

/// `catalog` kommt in die Datei nur, wenn es wahr ist; ältere Dateien ohne das Feld lesen als falsch.
#[test]
fn provenance_catalog_flag() {
    let p = orte("herkunft-katalog");
    let dir = p.user.join("x");
    std::fs::create_dir_all(&dir).unwrap();
    places::write_provenance(&dir, &herkunft()).unwrap();
    let text = std::fs::read_to_string(dir.join("herkunft.json")).unwrap();
    assert!(!text.contains("catalog"), "{text}");
    let aus_katalog = places::Provenance {
        catalog: true,
        ..herkunft()
    };
    places::write_provenance(&dir, &aus_katalog).unwrap();
    let text = std::fs::read_to_string(dir.join("herkunft.json")).unwrap();
    assert!(text.contains("\"catalog\": true"), "{text}");
    assert_eq!(places::read_provenance(&dir), Some(aus_katalog));
    // Nachsichtig: ein kaputter Wert gilt als falsch, die Herkunft bleibt.
    std::fs::write(
        dir.join("herkunft.json"),
        r#"{"link":"https://github.com/a/b","tag":"v0.1.0","catalog":"ja"}"#,
    )
    .unwrap();
    assert_eq!(places::read_provenance(&dir), Some(herkunft()));
}
