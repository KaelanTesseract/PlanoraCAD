//! Gemeinsame Hilfen der Tests: eigener STEP-Leser (Spec §5.1), Modell mit eigenen Formeln
//! (§5.2) und Ausgabe über `write_step`.
#![allow(dead_code)]

pub mod model;
pub mod reader;

use planora_exchange::Progress;
use planora_kernel::Solid;
use planora_step::{StepBody, StepOptions, StepReport, write_step};
use std::time::{Duration, UNIX_EPOCH};

/// Feste Einstellungen (gleiche Datei bei gleichem Inhalt): Zeitpunkt 2026-10-14T12:34:56.
pub fn options() -> StepOptions {
    StepOptions {
        time: UNIX_EPOCH + Duration::from_secs(1_791_981_296),
        ..StepOptions::new("probe.step")
    }
}

/// Schreibt die Körper mit Namen und liefert Text und Bericht.
pub fn export(bodies: &[(&str, &Solid)]) -> (String, StepReport) {
    let list: Vec<StepBody> = bodies
        .iter()
        .map(|(name, solid)| StepBody {
            name: (*name).to_string(),
            solid,
        })
        .collect();
    let mut out = Vec::new();
    let report = write_step(&list, &options(), &mut out, &Progress::new())
        .unwrap_or_else(|e| panic!("write_step: {e}"));
    (String::from_utf8(out).expect("nur ASCII"), report)
}

/// Kleiner Zufallsgenerator mit festem Startwert (keine Abhängigkeit).
pub struct Lcg(pub u64);

impl Lcg {
    pub fn uniform(&mut self) -> f64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (self.0 >> 11) as f64 / (1u64 << 53) as f64
    }

    pub fn range(&mut self, a: f64, b: f64) -> f64 {
        a + (b - a) * self.uniform()
    }
}
