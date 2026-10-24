//! Hintergrund-Arbeiter für Miniaturen (Plan P9): genau ein Thread („planora-miniaturen“) je
//! Programmlauf, höchstens ein Aufbau gleichzeitig.
//!
//! Der Besitzer (das Fenster „Projekte“) gibt mit [`ThumbWorker::set_queue`] die ganze
//! Warteschlange vor, sichtbare Kacheln zuerst; jeder Aufruf ersetzt die alte. Ergebnisse holt
//! er ohne Warten mit [`ThumbWorker::poll`] ab. Schließen des Fensters
//! ([`ThumbWorker::cancel_pending`]) leert nur die Warteschlange: ein laufender Aufbau läuft zu
//! Ende und speichert selbst (die Arbeit ist [`thumbnail_for`]). Eine Panik in einem Dokument
//! ergibt für dieses `Thumb::Failed`; der Thread arbeitet weiter. Fallenlassen des Arbeiters
//! kehrt sofort zurück; der Thread endet nach dem laufenden Auftrag.
//!
//! Hell/Dunkel gehört zum Schlüssel: nach einem Wechsel reiht der Besitzer die Dokumente mit dem
//! neuen Schlüssel ein (auch das gerade laufende, es ist dann neue Arbeit).
//!
//! `Thumb::Busy` (eine andere Instanz baut gerade): der Arbeiter baut nichts und meldet das
//! Ergebnis wie jedes andere. Der Besitzer reiht das Dokument frühestens nach [`BUSY_RETRY`]
//! wieder ein.

use crate::thumbnail_cache::{BuildLimits, Thumb, ThumbKey, ThumbOutcome, thumbnail_for};
use std::collections::{HashSet, VecDeque};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;
use std::sync::{Arc, Condvar, Mutex, MutexGuard, mpsc};
use std::time::{Duration, Instant};

/// Name des Arbeitsthreads.
pub const THREAD_NAME: &str = "planora-miniaturen";
/// Frühestens so lange nach einem `Thumb::Busy` fragt der Besitzer dasselbe Dokument erneut an.
pub const BUSY_RETRY: Duration = Duration::from_secs(5);
/// P9: genau ein Arbeitsthread.
const THREADS: usize = 1;

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
    /// Ersetzt die Warteschlange: erst `visible`, dann `rest`; je Ordner nur der erste Auftrag;
    /// ein Auftrag gleich dem laufenden entfällt.
    fn replace(
        &mut self,
        visible: Vec<ThumbRequest>,
        rest: Vec<ThumbRequest>,
        running: Option<&ThumbRequest>,
    ) {
        self.clear();
        let mut seen: HashSet<PathBuf> = HashSet::new();
        for r in visible.into_iter().chain(rest) {
            if running == Some(&r) {
                continue;
            }
            if !seen.insert(r.dir.clone()) {
                continue;
            }
            self.items.push_back(r);
        }
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
    /// Der Besitzer ist fort: nach dem laufenden Auftrag endet der Thread.
    stop: bool,
}

#[derive(Default)]
struct Shared {
    state: Mutex<State>,
    /// Weckt den Thread (neue Arbeit, Ende) und Wartende in `wait_idle` (Auftrag fertig).
    cv: Condvar,
}

impl Shared {
    fn lock(&self) -> MutexGuard<'_, State> {
        // Die Arbeit läuft nie unter der Sperre; eine Vergiftung ist daher harmlos.
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }
}

type Work = dyn Fn(&ThumbRequest) -> ThumbOutcome + Send + Sync + 'static;

/// Der Arbeiter. Fallenlassen beendet den Thread nach dem laufenden Auftrag (ohne zu warten).
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

    /// Mit eigener Arbeit (Tests).
    pub fn start_with(
        work: impl Fn(&ThumbRequest) -> ThumbOutcome + Send + 'static,
    ) -> ThumbWorker {
        let shared = Arc::new(Shared::default());
        let (tx, rx) = mpsc::channel();
        // Die Arbeit muss nur `Send` sein; die Sperre macht sie teilbar (es gibt genau einen
        // Thread, sie kostet je Auftrag nichts Merkliches).
        let work = Mutex::new(work);
        let work: Arc<Work> = Arc::new(move |r: &ThumbRequest| {
            let w = work.lock().unwrap_or_else(|e| e.into_inner());
            w(r)
        });
        for _ in 0..THREADS {
            let (shared, tx, work) = (Arc::clone(&shared), tx.clone(), Arc::clone(&work));
            // Lässt sich kein Thread starten, bleibt die Warteschlange liegen; die Kacheln zeigen
            // dann weiter den Platzhalter (die Oberfläche wartet nie).
            let _ = std::thread::Builder::new()
                .name(THREAD_NAME.into())
                .spawn(move || run(&shared, &tx, &*work));
        }
        ThumbWorker { shared, rx }
    }

    /// Ersetzt die Warteschlange: erst `visible`, dann `rest`, ohne Doppelte (nach `dir`), ohne
    /// den gerade laufenden Auftrag.
    pub fn set_queue(&self, visible: Vec<ThumbRequest>, rest: Vec<ThumbRequest>) {
        let mut st = self.shared.lock();
        let running = st.running.clone();
        st.queue.replace(visible, rest, running.as_ref());
        drop(st);
        self.shared.cv.notify_all(); // neue Arbeit
    }

    /// Fenster zu: nichts Neues beginnt; ein laufender Auftrag läuft zu Ende und speichert.
    pub fn cancel_pending(&self) {
        let mut st = self.shared.lock();
        st.queue.clear(); // nur die Wartenden
    }

    /// Fertige Ergebnisse seit dem letzten Aufruf (wartet nie).
    pub fn poll(&self) -> Vec<ThumbDone> {
        self.rx.try_iter().collect()
    }

    /// Ordner des Dokuments, dessen Miniatur gerade entsteht.
    pub fn running(&self) -> Option<PathBuf> {
        self.shared.lock().running.as_ref().map(|r| r.dir.clone())
    }

    /// Wartet höchstens `timeout`, bis nichts mehr wartet und nichts läuft (für Tests).
    pub fn wait_idle(&self, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        let mut st = self.shared.lock();
        loop {
            if st.queue.is_empty() && st.running.is_none() {
                return true;
            }
            let now = Instant::now();
            if now >= deadline {
                return false;
            }
            st = match self.shared.cv.wait_timeout(st, deadline - now) {
                Ok((g, _)) => g,
                Err(e) => e.into_inner().0,
            };
        }
    }
}

impl Drop for ThumbWorker {
    fn drop(&mut self) {
        let mut st = self.shared.lock();
        st.queue.clear();
        st.stop = true; // Ende nach dem laufenden Auftrag
        drop(st);
        self.shared.cv.notify_all();
    }
}

/// Schleife des Arbeitsthreads: wartet ohne Rechenlast auf Arbeit, baut eins nach dem anderen.
fn run(shared: &Shared, tx: &mpsc::Sender<ThumbDone>, work: &Work) {
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
                st = match shared.cv.wait(st) {
                    Ok(g) => g,
                    Err(e) => e.into_inner(),
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
fn guarded(work: &Work, req: &ThumbRequest) -> ThumbOutcome {
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

    fn order(q: &mut PendingQueue) -> Vec<String> {
        std::iter::from_fn(|| q.pop())
            .map(|r| r.dir.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn visible_first_then_rest_without_duplicates() {
        let mut q = PendingQueue::default();
        q.replace(
            vec![r("c", true), r("a", true)],
            vec![r("a", true), r("b", true), r("c", false)],
            None,
        );
        assert_eq!(order(&mut q), ["c", "a", "b"]);
    }

    #[test]
    fn replacing_drops_the_old_requests() {
        let mut q = PendingQueue::default();
        q.replace(vec![r("a", true), r("b", true)], Vec::new(), None);
        q.replace(Vec::new(), vec![r("c", true)], None);
        assert_eq!(order(&mut q), ["c"]);
    }

    #[test]
    fn the_running_request_is_left_out_but_another_key_for_it_is_not() {
        let mut q = PendingQueue::default();
        let x = r("x", true);
        q.replace(vec![r("x", true), r("a", true)], Vec::new(), Some(&x));
        assert_eq!(order(&mut q), ["a"]);
        q.replace(vec![r("x", false)], Vec::new(), Some(&x));
        assert_eq!(order(&mut q), ["x"]);
    }

    #[test]
    fn clear_empties_the_queue() {
        let mut q = PendingQueue::default();
        q.replace(vec![r("a", true)], Vec::new(), None);
        assert!(!q.is_empty());
        q.clear();
        assert!(q.is_empty());
        assert!(q.pop().is_none());
    }
}
