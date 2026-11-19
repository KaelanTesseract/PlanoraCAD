//! Die Laufzeit der Plug-ins (Plug-ins Etappe 1, Task 2): Vertrag, Fristen, Speichergrenze, Abschottung.

#[path = "support/source_hash.rs"]
mod source_hash;

use planora_plugin_host::manifest::Manifest;
use planora_plugin_host::package::Package;
use planora_plugin_host::runtime::{CallKind, Engine, HostCalls, Instance, PluginError};
use planora_plugin_host::runtime::{
    Dialog, DialogEvent, FieldValue, HostInfo, Level, RequestResult, Right,
};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Was der Fake-Host mitgeschrieben hat.
#[derive(Default)]
struct Protokoll {
    meldungen: Vec<(Level, String)>,
    dialoge: Vec<Dialog>,
    rechte: Vec<Right>,
    netz: Vec<String>,
}

#[derive(Clone, Default)]
struct FakeHost(Arc<Mutex<Protokoll>>);

impl HostCalls for FakeHost {
    fn log(&mut self, _level: Level, _text: &str) {}
    fn message(&mut self, level: Level, text: &str) {
        self.0
            .lock()
            .unwrap()
            .meldungen
            .push((level, text.to_string()));
    }
    fn open_dialog(&mut self, d: Dialog) -> Result<(), String> {
        self.0.lock().unwrap().dialoge.push(d);
        Ok(())
    }
    fn close_dialog(&mut self, _id: &str) {}
    fn request_right(&mut self, r: Right) -> u64 {
        self.0.lock().unwrap().rechte.push(r);
        7
    }
    fn right_granted(&self, _r: Right) -> bool {
        false
    }
    fn net_get(&mut self, url: &str) -> Result<u64, String> {
        self.0.lock().unwrap().netz.push(url.to_string());
        Ok(9)
    }
}

fn data(file: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/data")
        .join(file)
}

fn laden(name: &str) -> (Manifest, Package) {
    let bytes = std::fs::read(data(&format!("{name}.pcplugin"))).expect("Paket fehlt");
    let p = Package::read(&bytes).expect("Paket nicht lesbar");
    (p.manifest.clone(), p)
}

fn info(sprache: &str) -> HostInfo {
    HostInfo {
        api_version: "0.1".into(),
        program_version: "0.0.0".into(),
        language: sprache.into(),
    }
}

fn instanz(engine: &Engine, m: &Manifest, p: &Package) -> (Instance, FakeHost) {
    let host = FakeHost::default();
    let inst = engine
        .instantiate(m, &p.wasm, &p.languages, Box::new(host.clone()))
        .expect("instantiate");
    (inst, host)
}

fn starten(name: &str, engine: &Engine) -> (Instance, FakeHost) {
    let (m, p) = laden(name);
    let (mut inst, host) = instanz(engine, &m, &p);
    inst.init(info("de")).expect("init");
    (inst, host)
}

#[test]
fn hallo_registers_command() {
    let engine = Engine::new().unwrap();
    let (m, p) = laden("beispiel-hallo");
    let (mut inst, _h) = instanz(&engine, &m, &p);
    let reg = inst.init(info("de")).unwrap();
    assert_eq!(reg.commands[0].id, "hallo");
    assert_eq!(reg.commands[0].label_key, "hallo-befehl");
}

#[test]
fn hallo_dialog_roundtrip() {
    let engine = Engine::new().unwrap();
    let (mut inst, host) = starten("beispiel-hallo", &engine);
    inst.run_command("hallo").unwrap();
    {
        let p = host.0.lock().unwrap();
        assert_eq!(p.dialoge.len(), 1);
        assert_eq!(p.dialoge[0].id, "gruss");
        assert_eq!(p.dialoge[0].fields.len(), 2);
    }
    inst.handle_dialog(
        "gruss",
        DialogEvent::Ok(vec![
            ("name".into(), FieldValue::Text("Dennis".into())),
            ("laut".into(), FieldValue::Toggle(true)),
        ]),
    )
    .unwrap();
    let p = host.0.lock().unwrap();
    assert_eq!(
        p.meldungen,
        vec![(Level::Info, "HALLO, DENNIS!".to_string())]
    );
}

#[test]
fn hallo_falls_back_to_english() {
    let engine = Engine::new().unwrap();
    let (m, p) = laden("beispiel-hallo");
    let (mut inst, host) = instanz(&engine, &m, &p);
    inst.init(info("fr")).unwrap();
    inst.handle_dialog(
        "gruss",
        DialogEvent::Ok(vec![("name".into(), FieldValue::Text("Ann".into()))]),
    )
    .unwrap();
    assert_eq!(
        host.0.lock().unwrap().meldungen,
        vec![(Level::Info, "Hello, Ann!".to_string())]
    );
}

#[test]
fn endless_loop_times_out() {
    let engine = Engine::new().unwrap();
    let (mut inst, _h) = starten("test-faelle", &engine);
    let t = Instant::now();
    assert_eq!(inst.run_command("endlos"), Err(PluginError::Timeout));
    assert!(t.elapsed() < Duration::from_secs(6), "{:?}", t.elapsed());
}

#[test]
fn ui_call_deadline() {
    let engine = Engine::new().unwrap();
    let (mut inst, _h) = starten("test-faelle", &engine);
    inst.run_command("langsam-dialog").unwrap();
    let t = Instant::now();
    assert_eq!(
        inst.handle_dialog("langsam", DialogEvent::Cancel),
        Err(PluginError::Timeout)
    );
    assert!(t.elapsed() < Duration::from_secs(1), "{:?}", t.elapsed());
}

#[test]
fn memory_limit() {
    let engine = Engine::new().unwrap();
    let (mut m, p) = laden("test-faelle");
    m.memory_mb = 64;
    let (mut inst, _h) = instanz(&engine, &m, &p);
    inst.init(info("de")).unwrap();
    assert_eq!(inst.run_command("speicher"), Err(PluginError::OutOfMemory));
    // Das Programm lebt weiter: eine zweite Instanz lässt sich anlegen und benutzen.
    let (mut zweite, _h) = starten("test-faelle", &engine);
    assert_eq!(
        zweite.run_command("fehler"),
        Err(PluginError::Plugin("absichtlich".into()))
    );
}

#[test]
fn panic_is_trap() {
    let engine = Engine::new().unwrap();
    let (mut inst, _h) = starten("test-faelle", &engine);
    assert!(matches!(
        inst.run_command("panik"),
        Err(PluginError::Trap(_))
    ));
}

#[test]
fn plugin_error_is_not_failure() {
    let engine = Engine::new().unwrap();
    let (mut inst, _h) = starten("test-faelle", &engine);
    assert_eq!(
        inst.run_command("fehler"),
        Err(PluginError::Plugin("absichtlich".into()))
    );
    // Der Fehlertext des Plug-ins ist kein Ausfall: die Instanz arbeitet weiter.
    assert_eq!(
        inst.run_command("fehler"),
        Err(PluginError::Plugin("absichtlich".into()))
    );
}

#[test]
fn instance_unusable_after_trap() {
    let engine = Engine::new().unwrap();
    let (mut inst, host) = starten("test-faelle", &engine);
    assert!(matches!(
        inst.run_command("panik"),
        Err(PluginError::Trap(_))
    ));
    assert!(matches!(
        inst.run_command("umgebung"),
        Err(PluginError::Load(_))
    ));
    assert!(matches!(
        inst.handle_dialog("x", DialogEvent::Cancel),
        Err(PluginError::Load(_))
    ));
    assert!(matches!(
        inst.on_result(1, RequestResult::Granted),
        Err(PluginError::Load(_))
    ));
    // Es wurde nichts mehr ausgeführt: keine Meldung von `umgebung`.
    assert!(host.0.lock().unwrap().meldungen.is_empty());
}

#[test]
fn wasi_is_empty() {
    let engine = Engine::new().unwrap();
    let (mut inst, host) = starten("test-faelle", &engine);
    inst.run_command("umgebung").unwrap();
    assert_eq!(
        host.0.lock().unwrap().meldungen,
        vec![(
            Level::Info,
            "0 Variablen, Ordner: Fehler, Zeit: 0".to_string()
        )]
    );
}

#[test]
fn net_and_rights_reach_host() {
    let engine = Engine::new().unwrap();
    let (mut inst, host) = starten("test-faelle", &engine);
    inst.run_command("netz").unwrap();
    inst.run_command("recht").unwrap();
    let p = host.0.lock().unwrap();
    assert_eq!(p.netz, vec!["https://example.org/".to_string()]);
    assert_eq!(p.rechte, vec![Right::Net]);
}

#[test]
fn call_kinds_have_deadlines() {
    assert_eq!(CallKind::Ui.ticks(), 20);
    assert_eq!(CallKind::Command.ticks(), 500);
}

#[test]
fn sources_match_packages() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../plugins");
    for name in ["beispiel-hallo", "test-faelle"] {
        let jetzt = source_hash::source_hash(&root.join(name)).unwrap();
        let gespeichert = std::fs::read_to_string(data(&format!("{name}.quelle-sha256"))).unwrap();
        assert_eq!(
            jetzt,
            gespeichert.trim(),
            "Die Quellen von plugins/{name} passen nicht mehr zum Paket in tests/data. \
             Bitte tools/build-plugins.ps1 (oder .sh) ausführen und die Pakete mit einchecken."
        );
    }
}

#[test]
fn api_version_mismatch() {
    let engine = Engine::new().unwrap();
    let (mut m, p) = laden("beispiel-hallo");
    m.api = "0.2".into();
    let Err(PluginError::Load(text)) =
        engine.instantiate(&m, &p.wasm, &p.languages, Box::new(FakeHost::default()))
    else {
        panic!("Load erwartet");
    };
    assert!(
        text.contains("braucht PlanoraCAD-Schnittstelle 0.2"),
        "{text}"
    );
}

#[test]
fn host_sleep_cannot_outlast_deadline() {
    // `std::thread::sleep` wird zu monotonic-clock.subscribe-duration + poll im Host; die Epochen-
    // Unterbrechung greift dort nicht, deshalb lehnt die Laufzeit zu lange Wartezeiten selbst ab.
    let engine = Engine::new().unwrap();
    let (mut inst, _h) = starten("test-faelle", &engine);
    let t = Instant::now();
    assert_eq!(inst.run_command("schlafen"), Err(PluginError::Timeout));
    assert!(t.elapsed() < Duration::from_secs(6), "{:?}", t.elapsed());
    assert!(matches!(
        inst.run_command("fehler"),
        Err(PluginError::Load(_))
    ));
}
