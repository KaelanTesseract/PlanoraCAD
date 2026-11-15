//! Zeitzähler des Lösers (Skizzen 11a, Task 1): Die Aufteilung stimmt, und der Zähler verändert
//! das Ergebnis nicht.

use planora_sketch::solver::{Drag, SolveStatus};
use std::time::Duration;

mod common;
use common::{rect_with_dims, values_bits};

#[test]
fn solve_timings_add_up() {
    let mut s = rect_with_dims();
    let (r, t) = s.solve_timed();
    assert_eq!(r.status, SolveStatus::Solved);
    assert!(t.total >= t.assemble + t.normal + t.cholesky + t.rank);
    assert!(t.assemble > Duration::ZERO);
}

#[test]
fn timed_solve_equals_plain_solve() {
    let (mut a, mut b) = (rect_with_dims(), rect_with_dims());
    let ra = a.solve();
    let (rb, _) = b.solve_timed();
    assert_eq!(ra.dof, rb.dof);
    assert_eq!(values_bits(&a), values_bits(&b));
}

#[test]
fn timed_drag_equals_plain_drag() {
    let (mut a, mut b) = (rect_with_dims(), rect_with_dims());
    a.solve();
    b.solve();
    let p = a.points().next().unwrap().0;
    let at = a.point_xyz(p).unwrap();
    let drag = [Drag {
        point: p,
        target: at + planora_math::DVec3::new(0.5, 0.25, 0.0),
        plane: None,
    }];
    let ra = a.drag(&drag).unwrap();
    let (rb, t) = b.drag_timed(&drag).unwrap();
    assert_eq!(ra.status, rb.status);
    assert_eq!(values_bits(&a), values_bits(&b));
    assert!(t.total > Duration::ZERO);
}

#[test]
fn each_timed_call_starts_from_zero() {
    // Der Zähler darf nicht über Aufrufe hinweg aufsummieren: Die Teile des zweiten Aufrufs
    // passen in dessen eigene Gesamtzeit.
    let mut s = rect_with_dims();
    s.solve_timed();
    let (_, t) = s.solve_timed();
    assert!(t.total >= t.assemble + t.normal + t.cholesky + t.rank);
}
