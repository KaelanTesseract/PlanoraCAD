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
    places::install(&p, &paket("test-faelle")).unwrap();
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
    let user = places::install(&p, &pkg).unwrap();
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
    let first = places::install(&p, &pkg).unwrap();
    std::fs::write(first.join("alt.txt"), "vom ersten Mal").unwrap();
    let second = places::install(&p, &pkg).unwrap();
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

    let user = places::install(&p, &paket("test-faelle")).unwrap();
    places::uninstall(&p, "planora.test-faelle").unwrap();
    assert!(!user.exists());
    // Gibt es beides, entfernt `uninstall` nur das installierte.
    let user = places::install(&p, &pkg).unwrap();
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
