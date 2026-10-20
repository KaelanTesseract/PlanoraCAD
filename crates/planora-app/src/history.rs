//! Verlauf für Rückgängig und Wiederholen: rein, ohne Oberfläche, ohne egui-Typen.
//!
//! * **R3 – Schritt = Bild vorher:** `History::current` ist das Zustandsbild nach dem letzten
//!   Schritt. Jeder Eintrag in der Rückgängig-Liste hält das Bild *vor* einem Schritt samt dessen
//!   Nummer und dem Namen des Schritts; der Eintrag in der Wiederholen-Liste hält das Bild
//!   *nach* dem Schritt. Wer einen Schritt anlegt, vergleicht vorher mit nachher (gleicher
//!   Inhalt ergibt keinen Schritt) und gibt das neue Bild mit `record` ab.
//! * **R4 – Geste:** Der `Tracker` vergleicht je Bild den Änderungszähler des Dokuments mit dem
//!   zuletzt gesehenen. Solange eine Geste läuft (Maustaste gedrückt oder Eingabefeld aktiv),
//!   bleibt die Änderung offen; erst im ersten Bild ohne Geste entsteht ein einziger Schritt.
//! * **R7 – Gespeichert-Marke:** `History` merkt die Nummer des gespeicherten Stands. Jeder
//!   Schritt bekommt eine neue, nie wiederverwendete Nummer; Rückgängig und Wiederholen
//!   stellen die Nummern wieder her. „Ungespeichert“ heißt: aktuelle Nummer ≠ Marke.
//!
//! Grenzen: höchstens [`MAX_STEPS`] Schritte und [`MAX_BYTES`] geschätzte Bytes (Summe aus
//! Rückgängig-Liste, Wiederholen-Liste und aktuellem Bild); die ältesten fallen still heraus.

use planora_drawing::Drawing;
use planora_drawing::model::{Balloon, ViewSpec};
use planora_feature::{FeatureKind, SketchFeature, Snapshot, SnapshotFeature};
use std::collections::VecDeque;
use std::mem::size_of;

/// Höchstzahl der Schritte je Verlauf.
pub const MAX_STEPS: usize = 100;
/// Höchstgröße (geschätzt) je Verlauf in Bytes.
pub const MAX_BYTES: usize = 200 * 1024 * 1024;

/// Nummer eines Zustands im Verlauf.
pub type StepId = u64;

struct Entry<T> {
    state: T,
    bytes: usize,
    id: StepId,
    label: &'static str,
}

/// Verlauf von Zustandsbildern mit Rückgängig- und Wiederholen-Liste.
pub struct History<T> {
    current: T,
    current_id: StepId,
    current_bytes: usize,
    undo: VecDeque<Entry<T>>,
    redo: Vec<Entry<T>>,
    next_id: StepId,
    saved: Option<StepId>,
    max_steps: usize,
    max_bytes: usize,
}

impl<T> History<T> {
    /// Neuer Verlauf mit den üblichen Grenzen; der Anfangsstand hat die Nummer 0 und gilt als
    /// gespeichert.
    pub fn new(initial: T, bytes: usize) -> History<T> {
        History::with_limits(initial, bytes, MAX_STEPS, MAX_BYTES)
    }

    pub fn with_limits(initial: T, bytes: usize, max_steps: usize, max_bytes: usize) -> History<T> {
        History {
            current: initial,
            current_id: 0,
            current_bytes: bytes,
            undo: VecDeque::new(),
            redo: Vec::new(),
            next_id: 1,
            saved: Some(0),
            max_steps,
            max_bytes,
        }
    }

    pub fn current(&self) -> &T {
        &self.current
    }

    pub fn current_id(&self) -> StepId {
        self.current_id
    }

    /// Legt einen Schritt an: das bisherige Bild wandert in die Rückgängig-Liste, `after` wird
    /// aktuell und bekommt eine neue Nummer; die Wiederholen-Liste wird geleert.
    pub fn record(&mut self, after: T, bytes: usize, label: &'static str) {
        let before = std::mem::replace(&mut self.current, after);
        self.undo.push_back(Entry {
            state: before,
            bytes: self.current_bytes,
            id: self.current_id,
            label,
        });
        self.current_bytes = bytes;
        self.current_id = self.next_id;
        self.next_id += 1;
        self.redo.clear();
        self.enforce_limits();
    }

    fn enforce_limits(&mut self) {
        while self.undo.len() > self.max_steps || self.bytes() > self.max_bytes {
            if self.undo.pop_front().is_none() {
                break;
            }
        }
    }

    pub fn undo_label(&self) -> Option<&'static str> {
        self.undo.back().map(|e| e.label)
    }

    pub fn redo_label(&self) -> Option<&'static str> {
        self.redo.last().map(|e| e.label)
    }

    /// Das Bild, das Rückgängig herstellen würde.
    pub fn peek_undo(&self) -> Option<&T> {
        self.undo.back().map(|e| &e.state)
    }

    /// Das Bild, das Wiederholen herstellen würde.
    pub fn peek_redo(&self) -> Option<&T> {
        self.redo.last().map(|e| &e.state)
    }

    /// Nach gelungenem Zurückladen: aktuelles Bild in die Wiederholen-Liste, oberstes Bild der
    /// Rückgängig-Liste wird aktuell (mit seiner Nummer). Liefert den Namen des Schritts.
    pub fn commit_undo(&mut self) -> Option<&'static str> {
        let e = self.undo.pop_back()?;
        let label = e.label;
        let old = std::mem::replace(&mut self.current, e.state);
        self.redo.push(Entry {
            state: old,
            bytes: self.current_bytes,
            id: self.current_id,
            label,
        });
        self.current_bytes = e.bytes;
        self.current_id = e.id;
        Some(label)
    }

    /// Gegenstück zu [`History::commit_undo`].
    pub fn commit_redo(&mut self) -> Option<&'static str> {
        let e = self.redo.pop()?;
        let label = e.label;
        let old = std::mem::replace(&mut self.current, e.state);
        self.undo.push_back(Entry {
            state: old,
            bytes: self.current_bytes,
            id: self.current_id,
            label,
        });
        self.current_bytes = e.bytes;
        self.current_id = e.id;
        Some(label)
    }

    /// Verwirft nur den obersten Rückgängig-Schritt (Zurückladen fehlgeschlagen).
    pub fn discard_undo(&mut self) {
        self.undo.pop_back();
    }

    /// Verwirft nur den obersten Wiederholen-Schritt.
    pub fn discard_redo(&mut self) {
        self.redo.pop();
    }

    pub fn undo_len(&self) -> usize {
        self.undo.len()
    }

    pub fn redo_len(&self) -> usize {
        self.redo.len()
    }

    /// Geschätzte Größe: Rückgängig-Liste + Wiederholen-Liste + aktuelles Bild.
    pub fn bytes(&self) -> usize {
        self.undo.iter().map(|e| e.bytes).sum::<usize>()
            + self.redo.iter().map(|e| e.bytes).sum::<usize>()
            + self.current_bytes
    }

    pub fn mark_saved(&mut self) {
        self.saved = Some(self.current_id);
    }

    pub fn forget_saved(&mut self) {
        self.saved = None;
    }

    pub fn is_saved(&self) -> bool {
        self.saved == Some(self.current_id)
    }
}

/// Fasst Änderungen zu Schritten zusammen (R4): vergleicht je Bild den Änderungszähler mit dem
/// zuletzt gesehenen und hält Änderungen während einer Geste offen.
#[derive(Clone, Debug)]
pub struct Tracker {
    seen: u64,
    pending: bool,
}

impl Tracker {
    pub fn new(count: u64) -> Tracker {
        Tracker {
            seen: count,
            pending: false,
        }
    }

    /// `true`, wenn jetzt ein Schritt zu bilden ist: der Zähler hat sich verändert (in beide
    /// Richtungen) oder eine Änderung war offen, und es läuft keine Geste mehr.
    pub fn observe(&mut self, count: u64, gesture: bool) -> bool {
        if count != self.seen {
            self.seen = count;
            self.pending = true;
        }
        if self.pending && !gesture {
            self.pending = false;
            return true;
        }
        false
    }

    /// Nach dem Zurückladen: neuer Zählerstand ohne Schritt, nichts bleibt offen.
    pub fn reset(&mut self, count: u64) {
        self.seen = count;
        self.pending = false;
    }

    /// Gibt es Ungesehenes oder Offenes?
    pub fn is_pending(&self, count: u64) -> bool {
        self.pending || count != self.seen
    }
}

/// Geschätzte Größe eines Zustandsbildes. Netze zählen nicht (geteilte `Arc`).
pub fn snapshot_bytes(s: &Snapshot) -> usize {
    let mut total = size_of::<Snapshot>();
    for f in &s.features {
        total += size_of::<SnapshotFeature>() + f.name.len();
        if let FeatureKind::Sketch(sf) = &f.kind {
            total += sketch_data_bytes(sf);
        }
    }
    for d in &s.drawings {
        total += drawing_bytes(d);
    }
    total + s.parts_overrides.len() * 256
}

/// Geschätzte Größe eines Skizzenentwurfs (Eintrag im Skizzenverlauf, R10).
pub fn sketch_bytes(sf: &SketchFeature) -> usize {
    size_of::<SketchFeature>() + sf.sketch.name.len() + sketch_data_bytes(sf)
}

fn sketch_data_bytes(sf: &SketchFeature) -> usize {
    sf.sketch.variable_count() * 8
        + (sf.sketch.points().count() + sf.sketch.curves().count()) * 64
        + sf.sketch.constraints().count() * 64
        + sf.copies.len() * 64
}

fn drawing_bytes(d: &Drawing) -> usize {
    size_of::<Drawing>()
        + d.name.len()
        + d.views.len() * size_of::<ViewSpec>()
        + d.dimensions.len() * 256
        + d.balloons.len() * size_of::<Balloon>()
        + 1024
}
