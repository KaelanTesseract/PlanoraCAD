//! Hintergrund-Arbeiter für Miniaturen (Aufgabe 6, Plan P9): ein Thread je Programmlauf,
//! höchstens ein Aufbau gleichzeitig, sichtbare Kacheln zuerst, Schließen leert nur die
//! Warteschlange, eine Panik trifft nur ihr Dokument, Fallenlassen hängt nie.
//!
//! Die meisten Tests geben dem Arbeiter eine eigene Arbeit (`start_with`), die ihren Beginn über
//! einen Kanal meldet und auf Freigabe wartet: so laufen die Schritte fest nacheinander, ohne
//! Schlafen. Zeitgrenzen sind nur Sicherungen gegen Hängen und großzügig gewählt.

mod common;

use planora_app::state::AppState;
use planora_app::thumbnail_cache::{
    PREVIEW_JSON, PREVIEW_PNG, Thumb, ThumbKey, ThumbOutcome, marker_json,
};
use planora_app::thumbnail_worker::*;
use planora_math::DVec3;
use planora_project::docstore::{DocStore, MAIN_BRANCH};
use planora_project::fsutil::now_iso;
use planora_project::library::Library;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Sicherung gegen Hängen (nie die erwartete Dauer eines Schritts).
const T: Duration = Duration::from_secs(10);

// ---- Hilfen ---------------------------------------------------------------------------------

fn req(name: &str) -> ThumbRequest {
    ThumbRequest {
        dir: PathBuf::from(name),
        key: ThumbKey::new("", 1, MAIN_BRANCH, true),
    }
}

fn outcome(stored: bool) -> ThumbOutcome {
    ThumbOutcome {
        thumb: Thumb::NoBodies,
        stored,
        from_cache: false,
    }
}

fn name(dir: &std::path::Path) -> String {
    dir.to_string_lossy().into_owned()
}

fn names(done: &[ThumbDone]) -> Vec<String> {
    done.iter().map(|d| name(&d.dir)).collect()
}

/// Gesteuerte Arbeit: jeder Auftrag trägt seinen Namen in `order` ein und meldet ihn über
/// `started`; der Auftrag `hold` wartet danach, bis `release` ein Zeichen schickt. Die Namen in
/// `panics` lösen eine Panik aus.
struct Script {
    order: Arc<Mutex<Vec<String>>>,
    started: Receiver<String>,
    release: Sender<()>,
}

fn scripted(hold: &'static str, panics: &'static [&'static str]) -> (ThumbWorker, Script) {
    let order = Arc::new(Mutex::new(Vec::new()));
    let (started_tx, started) = mpsc::channel();
    let (release, release_rx) = mpsc::channel::<()>();
    let release_rx = Mutex::new(release_rx);
    let log = Arc::clone(&order);
    let w = ThumbWorker::start_with(move |r: &ThumbRequest| {
        let n = name(&r.dir);
        log.lock().unwrap().push(n.clone());
        let _ = started_tx.send(n.clone());
        if n == hold {
            let _ = release_rx.lock().unwrap().recv_timeout(T);
        }
        if panics.contains(&n.as_str()) {
            panic!("Prüfpanik in {n}");
        }
        outcome(true)
    });
    (
        w,
        Script {
            order,
            started,
            release,
        },
    )
}

impl Script {
    fn wait_started(&self, n: &str) {
        assert_eq!(self.started.recv_timeout(T).as_deref(), Ok(n));
    }

    fn order(&self) -> Vec<String> {
        self.order.lock().unwrap().clone()
    }
}

fn strings(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

/// Neue Bibliothek in Temp mit dem Projekt „P“ und einem gespeicherten Quader „P/<name>.pcad“.
fn box_document(name: &str) -> PathBuf {
    let root = common::temp_root("planora-worker");
    let lib = Library::open(&root).unwrap();
    lib.create_project("P").unwrap();
    let mut s = AppState::new();
    s.create_box(DVec3::new(10.0, 20.0, 30.0)).unwrap();
    let dir = lib.root().path().join("P").join(format!("{name}.pcad"));
    drop(DocStore::create(&dir, &s.snapshot(), "").unwrap());
    dir
}

// ---- Reihenfolge und Warteschlange -----------------------------------------------------------

#[test]
fn at_most_one_build_runs_at_a_time() {
    let now = Arc::new(AtomicUsize::new(0));
    let peak = Arc::new(AtomicUsize::new(0));
    let threads = Arc::new(Mutex::new(Vec::new()));
    let (n2, p2, t2) = (Arc::clone(&now), Arc::clone(&peak), Arc::clone(&threads));
    let w = ThumbWorker::start_with(move |_r: &ThumbRequest| {
        let mut ids = t2.lock().unwrap();
        let id = std::thread::current().id();
        if !ids.contains(&id) {
            ids.push(id);
        }
        drop(ids);
        let c = n2.fetch_add(1, Ordering::SeqCst) + 1;
        p2.fetch_max(c, Ordering::SeqCst);
        std::thread::sleep(Duration::from_millis(3));
        n2.fetch_sub(1, Ordering::SeqCst);
        outcome(true)
    });
    let all: Vec<ThumbRequest> = (0..20).map(|i| req(&format!("d{i}"))).collect();
    w.set_queue(all, Vec::new());
    assert!(w.wait_idle(T));
    assert_eq!(
        peak.load(Ordering::SeqCst),
        1,
        "nie zwei Aufbauten gleichzeitig"
    );
    assert_eq!(
        threads.lock().unwrap().len(),
        1,
        "alle Aufträge auf einem Thread"
    );
    let done = w.poll();
    assert_eq!(done.len(), 20, "alle 20 fertig");
    let expected: Vec<String> = (0..20).map(|i| format!("d{i}")).collect();
    assert_eq!(names(&done), expected);
}

#[test]
fn visible_tiles_come_first() {
    let (w, s) = scripted("x", &[]);
    w.set_queue(vec![req("x")], Vec::new());
    s.wait_started("x");
    w.set_queue(vec![req("c")], vec![req("a"), req("b")]);
    s.release.send(()).unwrap();
    assert!(w.wait_idle(T));
    assert_eq!(s.order(), strings(&["x", "c", "a", "b"]));
    assert_eq!(names(&w.poll()), strings(&["x", "c", "a", "b"]));
}

#[test]
fn a_new_queue_replaces_the_old_one() {
    let (w, s) = scripted("x", &[]);
    w.set_queue(vec![req("x")], Vec::new());
    s.wait_started("x");
    w.set_queue(vec![req("a"), req("b")], Vec::new());
    w.set_queue(vec![req("c")], Vec::new());
    s.release.send(()).unwrap();
    assert!(w.wait_idle(T));
    assert_eq!(s.order(), strings(&["x", "c"]));
    assert_eq!(names(&w.poll()), strings(&["x", "c"]));
}

#[test]
fn duplicates_and_the_running_request_are_not_queued_twice() {
    let (w, s) = scripted("x", &[]);
    w.set_queue(vec![req("x")], Vec::new());
    s.wait_started("x");
    w.set_queue(vec![req("x"), req("a"), req("a")], vec![req("a"), req("x")]);
    s.release.send(()).unwrap();
    assert!(w.wait_idle(T));
    assert_eq!(s.order(), strings(&["x", "a"]));
    assert_eq!(names(&w.poll()), strings(&["x", "a"]));
}

#[test]
fn a_theme_change_queues_the_running_document_again() {
    // Gleicher Ordner, anderer Schlüssel (Hell statt Dunkel): das ist neue Arbeit.
    let (w, s) = scripted("x", &[]);
    w.set_queue(vec![req("x")], Vec::new());
    s.wait_started("x");
    let mut light = req("x");
    light.key.dark = false;
    w.set_queue(vec![light.clone()], Vec::new());
    // Beide Läufe von „x“ warten auf Freigabe.
    s.release.send(()).unwrap();
    s.release.send(()).unwrap();
    assert!(w.wait_idle(T));
    let done = w.poll();
    assert_eq!(names(&done), strings(&["x", "x"]));
    assert!(done[0].key.dark);
    assert_eq!(done[1].key, light.key);
}

#[test]
fn closing_starts_nothing_new_but_finishes_and_keeps_the_running_one() {
    let (w, s) = scripted("x", &[]);
    w.set_queue(vec![req("x")], Vec::new());
    s.wait_started("x");
    w.set_queue(vec![req("a")], vec![req("b")]);
    w.cancel_pending();
    s.release.send(()).unwrap();
    assert!(w.wait_idle(T));
    let done = w.poll();
    assert_eq!(names(&done), strings(&["x"]), "nur der laufende Auftrag");
    assert!(
        done[0].outcome.stored,
        "der laufende Auftrag hat gespeichert"
    );
    assert_eq!(s.order(), strings(&["x"]));
    // Das Fenster öffnet wieder: der Arbeiter nimmt neue Arbeit an.
    w.set_queue(vec![req("c")], Vec::new());
    assert!(w.wait_idle(T));
    assert_eq!(names(&w.poll()), strings(&["c"]));
}

#[test]
fn a_panicking_build_reports_failed_and_the_worker_goes_on() {
    let (w, s) = scripted("", &["a"]);
    w.set_queue(vec![req("a"), req("b")], Vec::new());
    assert!(w.wait_idle(T), "der Arbeiter lebt nach der Panik weiter");
    let done = w.poll();
    assert_eq!(names(&done), strings(&["a", "b"]));
    assert!(
        matches!(&done[0].outcome.thumb, Thumb::Failed(m) if !m.is_empty()),
        "Panik → Failed, war {:?}",
        done[0].outcome.thumb
    );
    assert!(!done[0].outcome.stored);
    assert!(matches!(done[1].outcome.thumb, Thumb::NoBodies));
    assert_eq!(s.order(), strings(&["a", "b"]));
    assert_eq!(w.running(), None);
}

#[test]
fn running_names_the_document_in_progress() {
    let (w, s) = scripted("x", &[]);
    assert_eq!(w.running(), None);
    w.set_queue(vec![req("x")], Vec::new());
    s.wait_started("x");
    assert_eq!(w.running(), Some(PathBuf::from("x")));
    assert!(
        !w.wait_idle(Duration::from_millis(50)),
        "nicht im Leerlauf, solange ein Auftrag läuft"
    );
    assert!(w.poll().is_empty(), "noch kein Ergebnis");
    s.release.send(()).unwrap();
    assert!(w.wait_idle(T));
    assert_eq!(w.running(), None);
    assert_eq!(names(&w.poll()), strings(&["x"]));
}

#[test]
fn results_carry_the_request() {
    let w = ThumbWorker::start_with(|_r: &ThumbRequest| outcome(false));
    let mut r = req("d");
    r.key = ThumbKey::new("kennung", 7, "Zweig", false);
    w.set_queue(Vec::new(), vec![r.clone()]);
    assert!(w.wait_idle(T));
    let done = w.poll();
    assert_eq!(done.len(), 1);
    assert_eq!(done[0].dir, r.dir);
    assert_eq!(done[0].key, r.key);
    assert!(
        !done[0].outcome.stored,
        "auch ungespeicherte Ergebnisse kommen an"
    );
    assert!(w.poll().is_empty(), "jedes Ergebnis nur einmal");
}

#[test]
fn the_work_runs_on_the_named_thread() {
    let seen = Arc::new(Mutex::new(None));
    let s2 = Arc::clone(&seen);
    let w = ThumbWorker::start_with(move |_r: &ThumbRequest| {
        *s2.lock().unwrap() = std::thread::current().name().map(str::to_string);
        outcome(true)
    });
    w.set_queue(vec![req("a")], Vec::new());
    assert!(w.wait_idle(T));
    assert_eq!(seen.lock().unwrap().as_deref(), Some("planora-miniaturen"));
}

// ---- Echte Miniaturen ----------------------------------------------------------------------

#[test]
fn the_real_worker_stores_a_preview() {
    let dir = box_document("Quader");
    let key = ThumbKey::current(&dir, true).unwrap();
    let w = ThumbWorker::start();
    w.set_queue(
        vec![ThumbRequest {
            dir: dir.clone(),
            key: key.clone(),
        }],
        Vec::new(),
    );
    assert!(w.wait_idle(T));
    let done = w.poll();
    assert_eq!(done.len(), 1);
    assert_eq!(done[0].dir, dir);
    assert_eq!(done[0].key, key);
    assert!(
        matches!(done[0].outcome.thumb, Thumb::Image(_)),
        "{:?}",
        done[0].outcome.thumb
    );
    assert!(done[0].outcome.stored);
    assert!(dir.join(PREVIEW_JSON).is_file());
    assert!(dir.join(PREVIEW_PNG).is_file());
}

#[test]
fn a_theme_change_rebuilds_for_the_new_theme() {
    let dir = box_document("Quader");
    let dark = ThumbKey::current(&dir, true).unwrap();
    let light = ThumbKey::current(&dir, false).unwrap();
    let w = ThumbWorker::start();
    w.set_queue(
        vec![ThumbRequest {
            dir: dir.clone(),
            key: dark,
        }],
        Vec::new(),
    );
    assert!(w.wait_idle(T));
    w.set_queue(
        vec![ThumbRequest {
            dir: dir.clone(),
            key: light,
        }],
        Vec::new(),
    );
    assert!(w.wait_idle(T));
    let done = w.poll();
    assert_eq!(done.len(), 2);
    assert!(
        done.iter()
            .all(|d| d.outcome.stored && !d.outcome.from_cache)
    );
    assert!(!done[1].key.dark);
    let json = fs::read_to_string(dir.join(PREVIEW_JSON)).unwrap();
    assert!(json.contains("\"dark\":false"), "{json}");
}

#[test]
fn another_instance_building_is_reported_busy_and_nothing_is_built() {
    let dir = box_document("Quader");
    let key = ThumbKey::current(&dir, true).unwrap();
    let marker = marker_json(&key, std::process::id().wrapping_add(1), &now_iso());
    fs::write(dir.join(PREVIEW_JSON), &marker).unwrap();
    let w = ThumbWorker::start();
    w.set_queue(
        vec![ThumbRequest {
            dir: dir.clone(),
            key: key.clone(),
        }],
        Vec::new(),
    );
    assert!(w.wait_idle(T));
    let done = w.poll();
    assert_eq!(done.len(), 1);
    assert!(
        matches!(done[0].outcome.thumb, Thumb::Busy),
        "{:?}",
        done[0].outcome.thumb
    );
    assert!(!done[0].outcome.stored);
    assert!(!dir.join(PREVIEW_PNG).exists(), "nichts gebaut");
    assert_eq!(fs::read_to_string(dir.join(PREVIEW_JSON)).unwrap(), marker);
    assert!(BUSY_RETRY >= Duration::from_secs(1), "nicht sofort erneut");
}

// ---- Ende ----------------------------------------------------------------------------------

#[test]
fn dropping_the_worker_does_not_hang() {
    // Die Arbeit hält einen Sender: endet der Thread, ist der Kanal getrennt.
    let (alive_tx, alive_rx) = mpsc::channel::<()>();
    let alive_tx = Mutex::new(alive_tx);
    let w = ThumbWorker::start_with(move |_r: &ThumbRequest| {
        let _ = alive_tx.lock().unwrap().send(());
        outcome(true)
    });
    w.set_queue(vec![req("a")], Vec::new());
    assert!(w.wait_idle(T));
    let t0 = Instant::now();
    drop(w);
    assert!(
        t0.elapsed() < Duration::from_secs(2),
        "drop kehrt sofort zurück"
    );
    assert_eq!(alive_rx.recv_timeout(T), Ok(()), "Meldung von „a“");
    assert_eq!(
        alive_rx.recv_timeout(T),
        Err(RecvTimeoutError::Disconnected),
        "der Thread hat geendet"
    );
}

#[test]
fn dropping_during_a_build_finishes_it_and_then_ends_the_thread() {
    let finished = Arc::new(AtomicUsize::new(0));
    let f2 = Arc::clone(&finished);
    let (started_tx, started) = mpsc::channel::<String>();
    let (release, release_rx) = mpsc::channel::<()>();
    let release_rx = Mutex::new(release_rx);
    let (alive_tx, alive_rx) = mpsc::channel::<()>();
    let alive_tx = Mutex::new(alive_tx);
    let w = ThumbWorker::start_with(move |r: &ThumbRequest| {
        let _ = started_tx.send(name(&r.dir));
        let _ = release_rx.lock().unwrap().recv_timeout(T);
        f2.fetch_add(1, Ordering::SeqCst);
        let _ = alive_tx.lock().unwrap().send(());
        outcome(true)
    });
    w.set_queue(vec![req("x"), req("a")], Vec::new());
    assert_eq!(started.recv_timeout(T).as_deref(), Ok("x"));
    let t0 = Instant::now();
    drop(w);
    assert!(
        t0.elapsed() < Duration::from_secs(2),
        "drop wartet nicht auf den Aufbau"
    );
    release.send(()).unwrap();
    assert_eq!(
        alive_rx.recv_timeout(T),
        Ok(()),
        "der laufende Aufbau läuft zu Ende"
    );
    assert_eq!(
        alive_rx.recv_timeout(T),
        Err(RecvTimeoutError::Disconnected),
        "danach endet der Thread, „a“ beginnt nicht"
    );
    assert_eq!(finished.load(Ordering::SeqCst), 1);
}
