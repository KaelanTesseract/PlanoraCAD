//! Hintergrund-Arbeiter für Miniaturen (Plan P9): genau ein Thread („planora-miniaturen“) je
//! Programmlauf, höchstens ein Aufbau gleichzeitig.
//!
//! Der Besitzer (das Fenster „Projekte“) gibt mit [`ThumbWorker::set_queue`] die ganze
//! Warteschlange vor, sichtbare Kacheln zuerst; jeder Aufruf ersetzt die alte. Entdoppelt wird
//! vor der Sperre; unter der Sperre wird nur der laufende Auftrag herausgenommen und die neue
//! Liste eingesetzt. Trotzdem nur aufrufen, wenn sich etwas geändert hat (Kacheln, sichtbarer
//! Ausschnitt, Thema), nicht in jedem Bild. Ergebnisse holt der Besitzer ohne Warten mit
//! [`ThumbWorker::poll`] ab; Zustand ([`ThumbWorker::is_idle`], [`ThumbWorker::running`]) liest er
//! **vor** `poll`, sonst kann „Leerlauf“ ein noch ungelesenes Ergebnis verbergen.
//!
//! Schließen des Fensters ([`ThumbWorker::cancel_pending`]) leert nur die Warteschlange: ein
//! laufender Aufbau läuft zu Ende und speichert selbst (die Arbeit ist [`thumbnail_for`]). Eine
//! Panik in einem Dokument ergibt für dieses `Thumb::Failed`; der Thread arbeitet weiter.
//!
//! Ende des Programms: [`ThumbWorker::shutdown`] (und `Drop`, mit höchstens [`DROP_WAIT`])
//! nimmt keine Arbeit mehr an, wartet begrenzt auf einen laufenden Aufbau und entfernt, wenn er
//! danach noch läuft, die eigene Marke „baut“ dieses Auftrags. Sonst fände der nächste Start eine
//! frische fremde Marke (bis zu 15 min `Busy`). Läuft der Aufbau danach doch noch fertig,
//! speichert er wie sonst (atomar); endet der Prozess vorher, bleibt nichts zurück oder (seltenes
//! Rennen) eine Marke, die der Versuchszähler im Zwischenspeicher abdeckt.
//!
//! Hell/Dunkel gehört zum Schlüssel: nach einem Wechsel reiht der Besitzer die Dokumente mit dem
//! neuen Schlüssel ein (auch das gerade laufende, es ist dann neue Arbeit).
//!
//! `Thumb::Busy` (eine andere Instanz baut gerade): der Arbeiter baut nichts und meldet das
//! Ergebnis wie jedes andere. Der Besitzer reiht das Dokument frühestens nach [`BUSY_RETRY`]
//! wieder ein.
//!
//! [`ThumbWorker::alive`] ist false, wenn der Thread nicht gestartet werden konnte oder beendet
//! ist; `set_queue` ist dann wirkungslos (nichts wartet, der Besitzer zeigt keinen Fortschritt).

use crate::thumbnail_cache::{
    BuildLimits, Thumb, ThumbKey, ThumbOutcome, clear_own_marker, thumbnail_for,
};
use std::collections::{HashSet, VecDeque};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;
use std::sync::{Arc, Condvar, Mutex, MutexGuard, mpsc};
use std::time::{Duration, Instant};

/// Name des Arbeitsthreads.
pub const THREAD_NAME: &str = "planora-miniaturen";
/// Frühestens so lange nach einem `Thumb::Busy` fragt der Besitzer dasselbe Dokument erneut an.
pub const BUSY_RETRY: Duration = Duration::from_secs(5);
/// So lange wartet `Drop` höchstens auf einen laufenden Aufbau (dann wird nur die Marke entfernt).
pub const DROP_WAIT: Duration = Duration::from_millis(50);

/// Auftrag: Miniatur des Dokuments in `dir` für `key`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThumbRequest {
    pub dir: PathBuf,
    pub key: ThumbKey,
}

/// Ergebnis eines Auftrags.
#[derive(Clone, Debug)]
pub struct ThumbDone {
    pub dir: PathBuf,
    pub key: ThumbKey,
    pub outcome: ThumbOutcome,
}

/// Wartende Aufträge (reine Logik, ohne Thread).
#[derive(Debug, Default)]
struct PendingQueue {
    items: VecDeque<ThumbRequest>,
}

impl PendingQueue {
    /// Neue Reihenfolge (ohne Sperre): erst `visible`, dann `rest`; je Ordner nur der erste.
    fn plan(visible: Vec<ThumbRequest>, rest: Vec<ThumbRequest>) -> VecDeque<ThumbRequest> {
        let mut seen: HashSet<PathBuf> = HashSet::new();
        let mut out = VecDeque::with_capacity(visible.len() + rest.len());
        for r in visible.into_iter().chain(rest) {
            if !seen.insert(r.dir.clone()) {
                continue;
            }
            out.push_back(r);
        }
        out
    }

    /// Ersetzt die Warteschlange durch `planned`; ein Auftrag gleich dem laufenden entfällt.
    fn set(&mut self, planned: VecDeque<ThumbRequest>, running: Option<&ThumbRequest>) {
        self.items = planned;
        self.items.retain(|r| Some(r) != running);
    }

    fn pop(&mut self) -> Option<ThumbRequest> {
        self.items.pop_front()
    }

    fn clear(&mut self) {
        self.items.clear();
    }

    fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

#[derive(Default)]
struct State {
    queue: PendingQueue,
    running: Option<ThumbRequest>,
    /// Nach dem laufenden Auftrag endet der Thread; nichts Neues wird angenommen.
    stop: bool,
    /// Der Thread läuft (gestartet und noch nicht beendet).
    alive: bool,
}

impl State {
    fn idle(&self) -> bool {
        self.queue.is_empty() && self.running.is_none()
    }
}

#[derive(Default)]
struct Shared {
    state: Mutex<State>,
    /// Weckt den Thread (neue Arbeit, Ende) und Wartende (Auftrag fertig, Thread beendet).
    cv: Condvar,
}

impl Shared {
    fn lock(&self) -> MutexGuard<'_, State> {
        // Die Arbeit läuft nie unter der Sperre; eine Vergiftung ist daher harmlos.
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Wartet auf eine Meldung, höchstens bis `deadline` (None = ohne Grenze). false, wenn die
    /// Frist schon vorbei ist.
    fn wait<'a>(
        &self,
        st: MutexGuard<'a, State>,
        deadline: Option<Instant>,
    ) -> Result<MutexGuard<'a, State>, MutexGuard<'a, State>> {
        match deadline {
            None => Ok(self.cv.wait(st).unwrap_or_else(|e| e.into_inner())),
            Some(d) => {
                let now = Instant::now();
                if now >= d {
                    return Err(st);
                }
                Ok(match self.cv.wait_timeout(st, d - now) {
                    Ok((g, _)) => g,
                    Err(e) => e.into_inner().0,
                })
            }
        }
    }
}

/// Meldet das Ende des Threads (auch nach einer Panik außerhalb der Arbeit).
struct AliveGuard<'a>(&'a Shared);

impl Drop for AliveGuard<'_> {
    fn drop(&mut self) {
        self.0.lock().alive = false;
        self.0.cv.notify_all();
    }
}

type Spawn = Box<dyn FnOnce() + Send + 'static>;

/// Der Arbeiter. Fallenlassen = [`ThumbWorker::shutdown`] mit [`DROP_WAIT`].
pub struct ThumbWorker {
    shared: Arc<Shared>,
    rx: mpsc::Receiver<ThumbDone>,
}

impl ThumbWorker {
    /// Arbeit = [`thumbnail_for`] mit [`BuildLimits::default`].
    pub fn start() -> ThumbWorker {
        ThumbWorker::start_with(|r: &ThumbRequest| {
            thumbnail_for(&r.dir, &r.key, &BuildLimits::default())
        })
    }

    /// Mit eigener Arbeit (Tests). Die Arbeit wandert in den einen Thread.
    pub fn start_with(
        work: impl Fn(&ThumbRequest) -> ThumbOutcome + Send + 'static,
    ) -> ThumbWorker {
        ThumbWorker::start_on(work, |f: Spawn| {
            std::thread::Builder::new()
                .name(THREAD_NAME.into())
                .spawn(f)
                .map(drop)
        })
    }

    /// Wie [`ThumbWorker::start_with`] mit eigenem Threadstart (Tests: Start scheitert).
    fn start_on(
        work: impl Fn(&ThumbRequest) -> ThumbOutcome + Send + 'static,
        spawn: impl FnOnce(Spawn) -> std::io::Result<()>,
    ) -> ThumbWorker {
        let shared = Arc::new(Shared::default());
        let (tx, rx) = mpsc::channel();
        shared.lock().alive = true;
        let inner = Arc::clone(&shared);
        let body: Spawn = Box::new(move || {
            let _alive = AliveGuard(&inner);
            run(&inner, &tx, &work);
        });
        if spawn(body).is_err() {
            // Kein Thread: der Arbeiter bleibt tot, die Oberfläche wartet nie.
            shared.lock().alive = false;
        }
        ThumbWorker { shared, rx }
    }

    /// Ersetzt die Warteschlange: erst `visible`, dann `rest`, ohne Doppelte (nach `dir`), ohne
    /// den gerade laufenden Auftrag. Wirkungslos, wenn der Arbeiter nicht (mehr) läuft.
    pub fn set_queue(&self, visible: Vec<ThumbRequest>, rest: Vec<ThumbRequest>) {
        let planned = PendingQueue::plan(visible, rest);
        let mut st = self.shared.lock();
        if !st.alive || st.stop {
            return;
        }
        let running = st.running.clone();
        st.queue.set(planned, running.as_ref());
        drop(st);
        self.shared.cv.notify_all(); // neue Arbeit
    }

    /// Fenster zu: nichts Neues beginnt; ein laufender Auftrag läuft zu Ende und speichert.
    pub fn cancel_pending(&self) {
        let mut st = self.shared.lock();
        st.queue.clear(); // nur die Wartenden
        drop(st);
        self.shared.cv.notify_all(); // Wartende in wait_idle
    }

    /// Fertige Ergebnisse seit dem letzten Aufruf (wartet nie).
    pub fn poll(&self) -> Vec<ThumbDone> {
        self.rx.try_iter().collect()
    }

    /// Ordner des Dokuments, dessen Miniatur gerade entsteht.
    pub fn running(&self) -> Option<PathBuf> {
        self.shared.lock().running.as_ref().map(|r| r.dir.clone())
    }

    /// Nichts wartet und nichts läuft.
    pub fn is_idle(&self) -> bool {
        self.shared.lock().idle()
    }

    /// Der Thread läuft (false: Start gescheitert oder beendet).
    pub fn alive(&self) -> bool {
        self.shared.lock().alive
    }

    /// Wartet höchstens `timeout` (auch `Duration::MAX`), bis nichts mehr wartet und nichts läuft
    /// (für Tests).
    pub fn wait_idle(&self, timeout: Duration) -> bool {
        let deadline = Instant::now().checked_add(timeout);
        let mut st = self.shared.lock();
        loop {
            if st.idle() {
                return true;
            }
            st = match self.shared.wait(st, deadline) {
                Ok(g) => g,
                Err(_) => return false,
            };
        }
    }

    /// Geordnetes Ende: nichts Neues mehr, höchstens `wait` auf das Ende des Threads (ein
    /// laufender Aufbau speichert dann noch); läuft er danach noch, wird die eigene Marke „baut“
    /// seines Auftrags entfernt. Ergebnisse kommen danach keine mehr. Mehrfach aufrufbar.
    pub fn shutdown(&self, wait: Duration) {
        let deadline = Instant::now().checked_add(wait);
        let mut st = self.shared.lock();
        st.queue.clear();
        st.stop = true; // Ende nach dem laufenden Auftrag
        self.shared.cv.notify_all();
        while st.alive {
            st = match self.shared.wait(st, deadline) {
                Ok(g) => g,
                Err(g) => {
                    let running = g.running.clone();
                    drop(g);
                    if let Some(r) = running {
                        clear_own_marker(&r.dir, &r.key);
                    }
                    return;
                }
            };
        }
    }
}

impl Drop for ThumbWorker {
    fn drop(&mut self) {
        self.shutdown(DROP_WAIT);
    }
}

/// Schleife des Arbeitsthreads: wartet ohne Rechenlast auf Arbeit, baut eins nach dem anderen.
fn run(
    shared: &Shared,
    tx: &mpsc::Sender<ThumbDone>,
    work: &dyn Fn(&ThumbRequest) -> ThumbOutcome,
) {
    loop {
        let req = {
            let mut st = shared.lock();
            loop {
                if st.stop {
                    return;
                }
                if let Some(r) = st.queue.pop() {
                    st.running = Some(r.clone());
                    break r;
                }
                st = match shared.wait(st, None) {
                    Ok(g) | Err(g) => g,
                };
            }
        };
        let outcome = guarded(work, &req);
        let done = ThumbDone {
            dir: req.dir,
            key: req.key,
            outcome,
        };
        // Unter der Sperre: wer „nichts läuft“ sieht, findet das Ergebnis schon im Kanal.
        let mut st = shared.lock();
        let stop = st.stop;
        if !stop {
            let _ = tx.send(done);
        }
        st.running = None;
        drop(st);
        shared.cv.notify_all(); // für wait_idle
        if stop {
            return; // niemand holt das Ergebnis mehr ab; gespeichert hat die Arbeit selbst
        }
    }
}

/// Führt die Arbeit aus; eine Panik ergibt `Thumb::Failed` (nur im Speicher).
fn guarded(work: &dyn Fn(&ThumbRequest) -> ThumbOutcome, req: &ThumbRequest) -> ThumbOutcome {
    match catch_unwind(AssertUnwindSafe(|| work(req))) {
        Ok(o) => o,
        Err(_) => ThumbOutcome {
            thumb: Thumb::Failed("Beim Erstellen der Vorschau ist ein Fehler aufgetreten.".into()),
            stored: false,
            from_cache: false,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use planora_project::docstore::MAIN_BRANCH;

    fn r(name: &str, dark: bool) -> ThumbRequest {
        ThumbRequest {
            dir: PathBuf::from(name),
            key: ThumbKey::new("", 1, MAIN_BRANCH, dark),
        }
    }

    fn replace(
        q: &mut PendingQueue,
        visible: Vec<ThumbRequest>,
        rest: Vec<ThumbRequest>,
        running: Option<&ThumbRequest>,
    ) {
        q.set(PendingQueue::plan(visible, rest), running);
    }

    fn order(q: &mut PendingQueue) -> Vec<String> {
        std::iter::from_fn(|| q.pop())
            .map(|r| r.dir.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn visible_first_then_rest_without_duplicates() {
        let mut q = PendingQueue::default();
        replace(
            &mut q,
            vec![r("c", true), r("a", true)],
            vec![r("a", true), r("b", true), r("c", false)],
            None,
        );
        assert_eq!(order(&mut q), ["c", "a", "b"]);
    }

    #[test]
    fn replacing_drops_the_old_requests() {
        let mut q = PendingQueue::default();
        replace(&mut q, vec![r("a", true), r("b", true)], Vec::new(), None);
        replace(&mut q, Vec::new(), vec![r("c", true)], None);
        assert_eq!(order(&mut q), ["c"]);
    }

    #[test]
    fn the_running_request_is_left_out_but_another_key_for_it_is_not() {
        let mut q = PendingQueue::default();
        let x = r("x", true);
        replace(
            &mut q,
            vec![r("x", true), r("a", true)],
            Vec::new(),
            Some(&x),
        );
        assert_eq!(order(&mut q), ["a"]);
        replace(&mut q, vec![r("x", false)], Vec::new(), Some(&x));
        assert_eq!(order(&mut q), ["x"]);
    }

    #[test]
    fn clear_empties_the_queue() {
        let mut q = PendingQueue::default();
        replace(&mut q, vec![r("a", true)], Vec::new(), None);
        assert!(!q.is_empty());
        q.clear();
        assert!(q.is_empty());
        assert!(q.pop().is_none());
    }

    #[test]
    fn a_worker_whose_thread_cannot_start_is_dead_and_harmless() {
        let w = ThumbWorker::start_on(
            |_r: &ThumbRequest| panic!("nie aufgerufen"),
            |_f: Spawn| Err(std::io::Error::other("kein Thread")),
        );
        assert!(!w.alive());
        w.set_queue(vec![r("a", true)], vec![r("b", true)]);
        assert!(w.is_idle(), "nichts wartet");
        assert!(w.wait_idle(Duration::from_millis(10)));
        assert_eq!(w.running(), None);
        assert!(w.poll().is_empty());
        let t0 = Instant::now();
        drop(w);
        assert!(t0.elapsed() < Duration::from_secs(1));
    }
}
