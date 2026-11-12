//! Schriftliste für Skizzentext (Skizzen 8, Spec A1, G4): einmal je Programmlauf im Hintergrund
//! ab dem Programmstart aufgebaut, damit das Durchsuchen der Schriftordner den Start nie
//! aufhält. Die App fragt das Ergebnis in jedem Bild ab ([`crate::state::FontState::poll`]).
//!
//! Panik-Sicherheit: `planora-text` fängt Fehler im Schriftleser mit `catch_unwind` ab. Rust ruft
//! den Absturz-Hook aber schon vor dem Abwickeln; ohne [`crate::crash::expected_panics`] gälte
//! jede abgefangene Panik einer kaputten Schriftdatei als Absturz mit Bericht. Darum laufen das
//! Durchsuchen und jedes Setzen ([`Guarded`]) in diesem Merker.

use crate::state::FontState;
use planora_sketch::text_shape::{TextShape, TextShapeError, TextShaper, TextStyle};
use planora_text::{FontLibrary, system_font_dirs};
use std::sync::mpsc;

/// Startet das Durchsuchen der Schriftordner des Systems im Hintergrund und kehrt sofort zurück.
/// Übersprungene Dateien kommen je einmal ins Protokoll; sobald die Liste fertig ist, zeichnet
/// `ctx` ein neues Bild.
pub fn start(ctx: &egui::Context) -> FontState {
    spawn_scan(Some(ctx.clone()), || FontLibrary::scan(&system_font_dirs()))
}

/// Führt `scan` (das Durchsuchen selbst) in einem eigenen Faden aus und liefert sofort den
/// Laden-Zustand. Nur `scan` läuft im Merker [`crate::crash::expected_panics`]: Eine Panik darin
/// schreibt keinen Absturzbericht, dann gelten die mitgelieferten Schriften (mit Zeile im
/// Protokoll). Das Protokoll der gelesenen und übersprungenen Dateien steht außerhalb, ein Fehler
/// dort wäre ein echter Absturz. `ctx`: neues Bild, sobald die Liste da ist (Tests ohne Fenster:
/// `None`).
#[doc(hidden)]
pub fn spawn_scan(
    ctx: Option<egui::Context>,
    scan: impl FnOnce() -> FontLibrary + Send + 'static,
) -> FontState {
    let (tx, rx) = mpsc::channel();
    let spawned = std::thread::Builder::new()
        .name("planora-fonts".into())
        .spawn(move || {
            let started = std::time::Instant::now();
            let scanned = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                crate::crash::expected_panics(scan)
            }));
            let lib = match scanned {
                Ok(lib) => {
                    log_scan(&lib, started.elapsed());
                    lib
                }
                Err(_) => {
                    crate::diag::warn("Font scan failed; using bundled fonts");
                    FontLibrary::bundled_only()
                }
            };
            // Ist die App schon beendet, fragt niemand mehr.
            let _ = tx.send(lib);
            if let Some(ctx) = ctx {
                ctx.request_repaint();
            }
        });
    match spawned {
        Ok(_) => FontState::Loading(rx),
        Err(e) => {
            // Kein Faden möglich: lieber gleich die mitgelieferten Schriften als gar keine.
            crate::diag::warn(&format!("Cannot start font scan: {e}; using bundled fonts"));
            FontState::Ready(std::sync::Arc::new(FontLibrary::bundled_only()))
        }
    }
}

/// Schreibt jede übersprungene Datei und die Zahlen des Durchsuchens ins Protokoll.
fn log_scan(lib: &FontLibrary, took: std::time::Duration) {
    for (path, reason) in lib.skipped() {
        crate::diag::info(&format!("Font skipped: {} ({reason})", path.display()));
    }
    crate::diag::info(&format!(
        "Fonts read: {} faces, {} files skipped, {} ms",
        lib.face_count(),
        lib.skipped().len(),
        took.as_millis()
    ));
}

/// Setzt mit der Schriftliste; eine abgefangene Panik im Schriftleser (siehe Modulkopf) ist dabei
/// kein Absturz. Jeder Aufruf, der im Programm Text setzt, nimmt diesen Weg.
pub struct Guarded<'a>(pub &'a FontLibrary);

impl TextShaper for Guarded<'_> {
    fn shape(&self, content: &str, style: &TextStyle) -> Result<TextShape, TextShapeError> {
        crate::crash::expected_panics(|| self.0.shape(content, style))
    }
}
