//! Fortschritt und Abbrechen zwischen Oberfläche und Arbeitsthread.

use crate::error::ExchangeError;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

/// Gemeinsamer Zustand: Abbruchwunsch und erledigter Anteil (`done` von `total`, etwa gelesene
/// Bytes oder fertige Körper).
#[derive(Debug, Default)]
pub struct Progress {
    cancelled: AtomicBool,
    done: AtomicU64,
    total: AtomicU64,
}

impl Progress {
    pub fn new() -> Progress {
        Progress::default()
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }

    /// `Err(Cancelled)`, wenn abgebrochen werden soll.
    pub fn check(&self) -> Result<(), ExchangeError> {
        if self.is_cancelled() {
            Err(ExchangeError::Cancelled)
        } else {
            Ok(())
        }
    }

    pub fn set_total(&self, total: u64) {
        self.total.store(total, Ordering::Relaxed);
    }

    pub fn advance(&self, n: u64) {
        self.done.fetch_add(n, Ordering::Relaxed);
    }

    /// Erledigter Anteil zwischen 0 und 1 (0, solange die Gesamtmenge unbekannt ist).
    pub fn fraction(&self) -> f32 {
        let total = self.total.load(Ordering::Relaxed);
        if total == 0 {
            return 0.0;
        }
        (self.done.load(Ordering::Relaxed) as f64 / total as f64).clamp(0.0, 1.0) as f32
    }
}
