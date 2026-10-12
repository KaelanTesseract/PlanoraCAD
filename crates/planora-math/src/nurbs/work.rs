//! Zähler der NURBS-Auswertungen je Thread: ein deterministisches Maß für den Rechenaufwand
//! (anders als die Uhrzeit bei jedem Lauf gleich). Aufrufer wie die Booleschen Operationen
//! begrenzen damit ihre Arbeit.

use std::cell::Cell;

thread_local! {
    static EVALUATIONS: Cell<u64> = const { Cell::new(0) };
}

/// Zahl der Auswertungen von NURBS-Kurven und -Flächen (Punkt oder Ableitungen) in diesem Thread
/// seit dem Start; wächst nur (bei Überlauf wieder ab 0, praktisch nie).
pub fn evaluations() -> u64 {
    EVALUATIONS.with(Cell::get)
}

/// Führt `f` aus, ohne dessen Auswertungen zu zählen. Für Zwischenspeicher, die beim ersten
/// Gebrauch aufgebaut werden und danach bleiben: Sonst hinge die Zählung davon ab, ob der Speicher
/// schon gefüllt ist, und eine Arbeitsgrenze wäre nicht mehr bei jedem Lauf gleich.
pub fn uncounted<T>(f: impl FnOnce() -> T) -> T {
    let before = evaluations();
    let r = f();
    EVALUATIONS.with(|c| c.set(before));
    r
}

/// Zählt eine Auswertung.
#[inline]
pub(crate) fn count() {
    EVALUATIONS.with(|c| c.set(c.get().wrapping_add(1)));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_evaluation_is_counted_once() {
        use crate::DVec3;
        use crate::nurbs::NurbsCurve;
        let c = NurbsCurve::new(
            1,
            vec![0.0, 0.0, 1.0, 1.0],
            vec![DVec3::ZERO, DVec3::X],
            vec![1.0, 1.0],
        )
        .unwrap();
        let before = evaluations();
        c.point(0.5);
        c.tangent(0.25);
        assert_eq!(evaluations() - before, 2);
        let mid = evaluations();
        uncounted(|| c.point(0.75));
        assert_eq!(evaluations(), mid);
    }
}
