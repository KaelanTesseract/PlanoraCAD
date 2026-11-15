//! Analyse beim Ziehen (Skizzen 11a, Task 3, A10): Auf dem dünnen Weg übernimmt ein Zug Rang,
//! überzählige Abhängigkeiten und freie Teile vom Zugbeginn, statt sie in jedem Schritt neu zu
//! rechnen. Das Lösen beim Loslassen rechnet sie an der neuen Lage neu; jede Änderung der
//! Struktur verwirft die übernommene Analyse. Kleine Gruppen (dichter Weg) bleiben, wie sie sind.
//! Die Tests setzen den Weg ausdrücklich (Ruling F12).

use planora_math::{DVec3, Frame, Tolerance};
use planora_sketch::constraint::{ConstraintKind as K, LineOrPoints};
use planora_sketch::model::{CurveKind, PointId, Sketch};
use planora_sketch::solver::{Drag, SolveReport, SolveStatus};
use planora_sketch::sparse::SolverPath;
use std::time::Duration;

mod common;
use common::{chain_2d, rect_with_dims};

/// `chain_2d(40)` auf dem dünnen Weg (400 Variablen in einer Gruppe) und ihr Endpunkt.
fn chain() -> (Sketch, PointId) {
    let mut s = Sketch::new(Frame::WORLD, "S", Tolerance::default());
    let (last, _) = chain_2d(&mut s, 40);
    s.set_solver_path(SolverPath::Sparse);
    (s, last)
}

/// Ein Zugschritt am Punkt `p` um `(dx, dy)`, mit der Aufteilung der Zeit.
fn step(s: &mut Sketch, p: PointId, dx: f64, dy: f64) -> (SolveReport, Duration) {
    let at = s.point_xyz(p).unwrap();
    let (r, t) = s
        .drag_timed(&[Drag {
            point: p,
            target: at + DVec3::new(dx, dy, 0.0),
            plane: None,
        }])
        .unwrap();
    assert_eq!(r.status, SolveStatus::Solved, "{r:?}");
    (r, t.rank)
}

/// Freiheitsgrade, überzählige Abhängigkeiten und freie Teile zweier Berichte gleich.
fn same_analysis(a: &SolveReport, b: &SolveReport) {
    assert_eq!(a.dof, b.dof);
    assert_eq!(a.redundant, b.redundant);
    assert_eq!(a.free_points, b.free_points);
    assert_eq!(a.free_curves, b.free_curves);
}

#[test]
fn drag_reuses_analysis_of_drag_start() {
    let (mut s, last) = chain();
    let before = s.solve();
    assert_eq!(before.status, SolveStatus::Solved, "{before:?}");
    assert!(before.dof > 0, "{before:?}");
    for (dx, dy) in [(0.2, -0.1), (0.1, 0.1), (-0.3, 0.05)] {
        let (r, rank) = step(&mut s, last, dx, dy);
        same_analysis(&r, &before);
        // Ruling F7: Auf dem dünnen Weg rechnet der Zug keine Rangprüfung.
        assert_eq!(rank, Duration::ZERO, "Rangprüfung im Zug");
    }
}

#[test]
fn release_solve_recomputes_analysis() {
    let (mut s, last) = chain();
    s.solve();
    for (dx, dy) in [(0.2, -0.1), (0.1, 0.1), (-0.3, 0.05)] {
        step(&mut s, last, dx, dy);
    }
    let (release, t) = s.solve_timed();
    assert_eq!(release.status, SolveStatus::Solved, "{release:?}");
    // Die Analyse vom Zugbeginn gilt nicht für das Loslassen: neu gerechnet.
    assert!(t.rank > Duration::ZERO, "keine Rangprüfung beim Loslassen");
    // Ein frischer Sketch an denselben Werten (der Wegwechsel leert den Zwischenspeicher).
    let mut fresh = s.clone();
    fresh.set_solver_path(SolverPath::Sparse);
    let r = fresh.solve();
    assert_eq!(r.status, release.status);
    same_analysis(&release, &r);
}

#[test]
fn structure_change_drops_carried() {
    let (mut s, last) = chain();
    let before = s.solve();
    let (r, _) = step(&mut s, last, 0.2, -0.1);
    assert_eq!(r.dof, before.dof);
    // Während des Zugs: die erste Linie waagerecht.
    let line = s
        .curves()
        .find(|(_, c)| matches!(c.kind, CurveKind::Line { .. }))
        .map(|(id, _)| id)
        .unwrap();
    let h = s
        .add_constraint(K::Horizontal(LineOrPoints::Line(line)))
        .unwrap();
    let (r, rank) = step(&mut s, last, 0.1, 0.1);
    assert!(rank > Duration::ZERO, "Analyse nicht neu gerechnet");
    // Neu gerechnet an der Lage dieses Zugs: wie ein frischer Sketch an denselben Werten, und
    // die neue Abhängigkeit nimmt einen Freiheitsgrad. (Gegen den Bericht vor dem Zug lässt sich
    // nicht vergleichen: Die Kette wird beim Ziehen an vier Stellen fast abhängig, die Analyse an
    // der gezogenen Lage hat darum ohnehin mehr Freiheitsgrade.)
    let mut fresh = s.clone();
    fresh.set_solver_path(SolverPath::Sparse);
    same_analysis(&r, &fresh.solve());
    let mut without = s.clone();
    without.remove_constraint(h).unwrap();
    without.set_solver_path(SolverPath::Sparse);
    let w = without.solve();
    assert_eq!(r.dof + 1, w.dof, "{r:?}");
}

/// Erwartete Lage nach dem Zug im Rechteck: Bitmuster von x und y aller vier Ecken, gemessen
/// auf dem Stand vor dieser Aufgabe (Commit 8ef6b85).
const RECT_AFTER_DRAG: [(u64, u64); 4] = [
    (0x3fe8_0000_0012_f6e7, 0x3ff0_cccc_cd02_07a8),
    (0x4044_6000_0000_4bdc, 0x3ff0_cccc_cd02_07a8),
    (0x4044_6000_0000_4bdc, 0x403f_0ccc_ccd0_207a),
    (0x3fe8_0000_0012_f6e7, 0x403f_0ccc_ccd0_207a),
];

#[test]
fn small_group_drag_unchanged() {
    for path in [SolverPath::Auto, SolverPath::Dense] {
        let mut s = rect_with_dims();
        s.set_solver_path(path);
        let r = s.solve();
        assert_eq!(r.status, SolveStatus::Solved, "{r:?}");
        let corner = s.points().nth(2).map(|(id, _)| id).unwrap();
        let at = s.point_xyz(corner).unwrap();
        let r = s
            .drag(&[Drag {
                point: corner,
                target: at + DVec3::new(1.5, -0.7, 0.0),
                plane: None,
            }])
            .unwrap();
        assert_eq!(r.status, SolveStatus::Solved, "{r:?}");
        // Vier Linien mit gemeinsamen Ecken: 8 Variablen, 4 Richtungen und 2 Maße, frei bleibt
        // die Verschiebung.
        assert_eq!(r.dof, 2, "{r:?}");
        assert!(r.redundant.is_empty(), "{r:?}");
        let bits = common::values_bits(&s);
        assert_eq!(bits, RECT_AFTER_DRAG, "{path:?}: {bits:x?}");
    }
}
