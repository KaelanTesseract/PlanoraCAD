//! Keine verschachtelte Lesesperre auf dem egui-Kontext.
//!
//! egui schützt den Kontext mit einem fairen `parking_lot::RwLock`: Wartet ein Schreiber (z. B.
//! `request_repaint` aus dem Thread der Update-Prüfung), bekommt auch ein Thread, der schon
//! liest, keine zweite Lesesperre mehr. Liest Code innerhalb von `ctx.data(|…| …)` noch einmal
//! den Kontext (etwa `ctx.viewport_id()`), warten Leser und Schreiber aufeinander, bis egui nach
//! 10 s mit „Failed to acquire RwLock read/write after 10s. Deadlock?“ abbricht. So fielen
//! zufällig Tests aus, sobald der Rechner ausgelastet war.
//!
//! Der Test lässt einen zweiten Thread ununterbrochen `request_repaint` rufen und fragt dabei
//! die Tipp-Abfragen der Ansicht sehr oft ab. Mit verschachtelter Sperre hängt das fast sofort.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use planora_app::viewport::{typing, typing_for_history};

#[test]
fn typing_queries_survive_a_concurrent_repaint_request() {
    let ctx = egui::Context::default();
    let stop = Arc::new(AtomicBool::new(false));

    let writer = {
        let ctx = ctx.clone();
        let stop = Arc::clone(&stop);
        std::thread::spawn(move || {
            while !stop.load(Ordering::Relaxed) {
                ctx.request_repaint();
            }
        })
    };

    let (done_tx, done_rx) = mpsc::channel();
    let reader = {
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let until = Instant::now() + Duration::from_secs(2);
            let mut rounds = 0u64;
            while Instant::now() < until {
                let _ = typing(&ctx);
                let _ = typing_for_history(&ctx);
                rounds += 1;
            }
            let _ = done_tx.send(rounds);
        })
    };

    // Weit unter den 10 s, nach denen egui selbst abbricht.
    let result = done_rx.recv_timeout(Duration::from_secs(8));
    stop.store(true, Ordering::Relaxed);
    let rounds = result.expect("Tipp-Abfragen hängen: verschachtelte Lesesperre auf dem Kontext");
    assert!(rounds > 0);
    reader.join().expect("Leser ist abgestürzt");
    writer.join().expect("Schreiber ist abgestürzt");
}
