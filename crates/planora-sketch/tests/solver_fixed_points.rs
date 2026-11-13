//! Punkte mit `Fixed` (etwa jeder Punkt nach dem Auflösen eines Textes): Eine Gruppe aus genau
//! einem erfüllten `Fixed` erkennt der Löser ohne Rechnung. Bericht, Freiheitsgrade, Überzähliges,
//! Widersprüche, Entartung und Ziehen bleiben genau wie beim vollen Lauf.

use planora_math::{DVec2, Frame, Tolerance};
use planora_sketch::constraint::ConstraintKind as K;
use planora_sketch::model::{PointId, Sketch};
use planora_sketch::solver::{Drag, SolveStatus};

fn sketch() -> Sketch {
    Sketch::new(Frame::WORLD, "S", Tolerance::default())
}

fn bits(v: &[f64]) -> Vec<u64> {
    v.iter().map(|x| x.to_bits()).collect()
}

fn at(s: &Sketch, p: PointId) -> DVec2 {
    s.point_xy(p).unwrap()
}

#[test]
fn satisfied_fixed_points_report_like_a_full_run() {
    let mut s = sketch();
    let lone = s.add_point(1.0, 2.0).unwrap();
    s.add_constraint(K::Fixed(lone, 1.0, 2.0)).unwrap();
    // Linie mit festem Anfang: Das freie Ende trägt die zwei Freiheitsgrade.
    let a = s.add_point(0.0, 0.0).unwrap();
    let b = s.add_point(10.0, 0.0).unwrap();
    let line = s.add_line(a, b).unwrap();
    s.add_constraint(K::Fixed(a, 0.0, 0.0)).unwrap();
    // Zwei feste Punkte, die zusätzlich zusammenfallen: Das ist überzählig.
    let q = s.add_point(5.0, 5.0).unwrap();
    let r = s.add_point(5.0, 5.0).unwrap();
    s.add_constraint(K::Fixed(q, 5.0, 5.0)).unwrap();
    s.add_constraint(K::Fixed(r, 5.0, 5.0)).unwrap();
    let both = s.add_constraint(K::Coincident(q, r)).unwrap();
    let before = s.variables().to_vec();

    let report = s.solve();
    assert_eq!(report.status, SolveStatus::Solved, "{report:?}");
    assert_eq!(report.dof, 2, "nur das freie Ende: {report:?}");
    assert_eq!(report.free_points, vec![b]);
    assert_eq!(report.free_curves, vec![line]);
    assert_eq!(report.redundant, vec![both], "{report:?}");
    assert!(report.conflicting.is_empty());
    assert_eq!(report.iterations, 0);
    assert_eq!(
        report.group_solves, 3,
        "zwei einzelne Fixed und die Gruppe q, r"
    );
    assert!(!report.drag_failed);
    assert_eq!(
        bits(s.variables()),
        bits(&before),
        "nichts bewegt sich, auch nicht im letzten Bit"
    );

    // Der zweite Lauf kommt ganz aus dem Zwischenspeicher, mit demselben Bericht.
    let again = s.solve();
    assert_eq!(again.group_solves, 0, "{again:?}");
    assert_eq!(again.status, report.status);
    assert_eq!(again.dof, report.dof);
    assert_eq!(again.free_points, report.free_points);
    assert_eq!(again.free_curves, report.free_curves);
    assert_eq!(again.redundant, report.redundant);
    assert_eq!(bits(s.variables()), bits(&before));
}

#[test]
fn a_fixed_point_off_its_target_is_still_pulled_onto_it() {
    let mut s = sketch();
    let p = s.add_point(0.0, 0.0).unwrap();
    s.add_constraint(K::Fixed(p, 3.0, 4.0)).unwrap();
    let r = s.solve();
    assert_eq!(r.status, SolveStatus::Solved, "{r:?}");
    assert_eq!(r.dof, 0);
    assert!(r.free_points.is_empty());
    assert!(r.iterations >= 1, "gerechnet: {r:?}");
    assert_eq!(r.group_solves, 1);
    assert!(
        at(&s, p).distance(DVec2::new(3.0, 4.0)) < 1e-9,
        "{:?}",
        at(&s, p)
    );
}

#[test]
fn two_fixed_targets_for_one_point_are_still_a_conflict() {
    let mut s = sketch();
    let p = s.add_point(0.0, 0.0).unwrap();
    let here = s.add_constraint(K::Fixed(p, 0.0, 0.0)).unwrap();
    let there = s.add_constraint(K::Fixed(p, 1.0, 0.0)).unwrap();
    let r = s.solve();
    assert_eq!(r.status, SolveStatus::Conflicting, "{r:?}");
    assert_eq!(r.conflicting, vec![here, there], "{r:?}");
    assert_eq!(at(&s, p), DVec2::ZERO, "die Lage bleibt");
}

#[test]
fn a_zero_length_line_between_fixed_points_still_fails_as_degenerate() {
    let mut s = sketch();
    let a = s.add_point(0.0, 0.0).unwrap();
    let b = s.add_point(10.0, 0.0).unwrap();
    s.add_line(a, b).unwrap();
    s.set_point_xy(b, 0.0, 0.0).unwrap();
    s.add_constraint(K::Fixed(a, 0.0, 0.0)).unwrap();
    s.add_constraint(K::Fixed(b, 0.0, 0.0)).unwrap();
    let before = s.variables().to_vec();
    let r = s.solve();
    assert_eq!(r.status, SolveStatus::NotConverged, "{r:?}");
    assert!(r.conflicting.is_empty(), "{r:?}");
    assert_eq!(r.group_solves, 2);
    assert_eq!(bits(s.variables()), bits(&before));
    // Auch aus dem Zwischenspeicher gescheitert.
    let again = s.solve();
    assert_eq!(again.status, SolveStatus::NotConverged, "{again:?}");
    assert_eq!(again.group_solves, 0);
}

#[test]
fn a_fixed_point_does_not_follow_a_drag() {
    let mut s = sketch();
    let p = s.add_point(2.0, 1.0).unwrap();
    let q = s.add_point(12.0, 1.0).unwrap();
    s.add_line(p, q).unwrap();
    s.add_constraint(K::Fixed(p, 2.0, 1.0)).unwrap();
    assert_eq!(s.solve().status, SolveStatus::Solved);
    let r = s
        .drag(&[Drag {
            point: p,
            target: DVec2::new(20.0, 20.0).extend(0.0),
            plane: None,
        }])
        .unwrap();
    assert_eq!(r.status, SolveStatus::Solved, "{r:?}");
    assert!(
        at(&s, p).distance(DVec2::new(2.0, 1.0)) < 1e-9,
        "der feste Punkt bleibt: {:?}",
        at(&s, p)
    );
    assert_eq!(at(&s, q), DVec2::new(12.0, 1.0), "das freie Ende bleibt");
}
