//! Arbeitsgrenze der Booleschen Operationen mit Freiformflächen.
//!
//! Gemessen wird nicht die Zeit, sondern die Zahl der NURBS-Auswertungen
//! ([`planora_math::nurbs::evaluations`]): Das Ergebnis (Körper oder Meldung) hängt dann nur von
//! den Eingaben ab und ist bei jedem Lauf gleich. Geprüft wird an festen Stellen der Pipeline
//! (je Flächenpaar beim Schnitt, je Fläche beim Teilen, je Flächenstück beim Einordnen); eine
//! einzelne Stufe kann die Grenze daher um ihren eigenen Aufwand überschreiten.

use super::BooleanError;
use planora_math::nurbs::evaluations;
use std::cell::Cell;

/// Höchstzahl der NURBS-Auswertungen einer Booleschen Operation mit Freiformflächen.
/// Festgelegt nach Messung (Release): Die aufwendigste Operation aller Tests (auch der nur mit
/// `--ignored` laufenden Reihen) braucht gut 6 Millionen, ein Drehkörper aus einem Spline mit
/// 39 Punkten gegen einen Zylinder 3 bis 7 Millionen (3 bis 7 s); im Mittel schafft der
/// Release-Build gut 1 Million Auswertungen je Sekunde. 15 Millionen lassen den Tests den
/// 2,5-fachen Spielraum und begrenzen eine Operation auf etwa 10 bis 15 Sekunden. Nicht gezählt
/// werden Auswertungen von Bézier-Flicken (Strahlschnitt, Unterteilung im Schnittrechner); der
/// Schnittrechner hat dafür eigene Grenzen (`MAX_PROBES` u. a.).
pub const MAX_FREEFORM_EVALUATIONS: u64 = 15_000_000;

/// Meldung bei überschrittener Arbeitsgrenze.
pub const TOO_EXPENSIVE: &str = "Die Berechnung ist zu aufwendig (Arbeitsgrenze für Freiformflächen erreicht); bitte weniger Spline-Punkte oder eine einfachere Form verwenden";

thread_local! {
    /// Stand des Zählers, ab dem die laufende Operation abbricht (`None`: keine Grenze).
    static STOP_AT: Cell<Option<u64>> = const { Cell::new(None) };
}

/// Setzt die Grenze für die Dauer des Werts; danach gilt wieder die vorige.
pub(crate) struct Scope {
    previous: Option<u64>,
}

impl Scope {
    /// Grenze von `limit` weiteren Auswertungen ab jetzt.
    pub(crate) fn start(limit: u64) -> Scope {
        let previous = STOP_AT.with(|c| c.replace(Some(evaluations().saturating_add(limit))));
        Scope { previous }
    }
}

impl Drop for Scope {
    fn drop(&mut self) {
        STOP_AT.with(|c| c.set(self.previous));
    }
}

/// Fehler, wenn die laufende Operation ihre Grenze überschritten hat.
pub(crate) fn check() -> Result<(), BooleanError> {
    match STOP_AT.with(Cell::get) {
        Some(stop) if evaluations() > stop => Err(BooleanError::Unsupported(TOO_EXPENSIVE.into())),
        _ => Ok(()),
    }
}
