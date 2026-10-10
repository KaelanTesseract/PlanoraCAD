use planora_math::{DVec2, DVec3, Frame, Tolerance};
use planora_sketch::*;

fn frame() -> Frame {
    Frame {
        origin: DVec3::ZERO,
        x: DVec3::X,
        y: DVec3::Y,
        z: DVec3::Z,
    }
}

/// Skizze mit allen Kurvenarten und mehreren Abhängigkeiten, ein Objekt gelöscht.
fn rich_sketch() -> Sketch {
    let mut s = Sketch::new(frame(), "Reich", Tolerance::default());
    let r = s
        .add_rectangle(DVec2::new(0.0, 0.0), DVec2::new(20.0, 10.0))
        .unwrap();
    let c = s.add_point(30.0, 5.0).unwrap();
    let circle = s.add_circle(c, 3.0).unwrap();
    let a = s.add_point(33.0, 5.0).unwrap();
    let b = s.add_point(30.0, 8.0).unwrap();
    let arc = s.add_arc(c, a, b).unwrap();
    let ec = s.add_point(50.0, 5.0).unwrap();
    let ellipse = s.add_ellipse(ec, DVec2::new(6.0, 0.0), 3.0).unwrap();
    let sp: Vec<_> = [(0.0, 20.0), (5.0, 24.0), (10.0, 20.0)]
        .iter()
        .map(|&(x, y)| s.add_point(x, y).unwrap())
        .collect();
    let spline = s.add_spline(&sp).unwrap();
    s.add_constraint(ConstraintKind::Radius(circle, 3.0))
        .unwrap();
    s.add_constraint(ConstraintKind::Fixed(c, 30.0, 5.0))
        .unwrap();
    s.add_constraint(ConstraintKind::Distance(r.points[0], r.points[1], 20.0))
        .unwrap();
    let doomed = s
        .add_constraint(ConstraintKind::Equal(circle, arc))
        .unwrap();
    s.remove_constraint(doomed).unwrap();
    s.set_construction(ellipse, true).unwrap();
    let _ = spline;
    s.solve();
    s
}

#[test]
fn a_sketch_survives_the_round_trip_with_identical_data() {
    let s = rich_sketch();
    let d = s.to_data();
    let t = Sketch::from_data(d.clone()).unwrap();
    assert_eq!(t.to_data(), d);
    assert_eq!(t.profiles().profiles.len(), s.profiles().profiles.len());
    // gelöschte Einträge behalten ihren Platz, damit Ids gültig bleiben
    assert!(d.constraints.iter().any(|c| !c.alive));
}

#[test]
fn a_restored_sketch_still_solves_and_edits() {
    let s = rich_sketch();
    let mut t = Sketch::from_data(s.to_data()).unwrap();
    assert!(matches!(t.solve().status, SolveStatus::Solved));
    let p = t.points().next().unwrap().0;
    t.set_point_xy(p, 1.0, 1.0).unwrap();
}

fn broken(f: impl FnOnce(&mut SketchData)) -> SketchError {
    let mut d = rich_sketch().to_data();
    f(&mut d);
    Sketch::from_data(d).unwrap_err()
}

#[test]
fn invalid_data_is_rejected_without_panic() {
    let _ = broken(|d| d.vars[0] = f64::NAN);
    let _ = broken(|d| d.vars[0] = 1e300);
    let _ = broken(|d| d.points[0].x = 10_000);
    let _ = broken(|d| {
        d.curves[0].kind = CurveKind::Line {
            a: PointId::from_index(9_999),
            b: PointId::from_index(0),
        }
    });
    let _ = broken(|d| d.constraints[0].extra = Some(10_000));
    let _ = broken(|d| d.constraints[0].branch = 5);
    let _ = broken(|d| d.tolerance.linear = -1.0);
    let _ = broken(|d| d.frame.x = DVec3::ZERO);
    // lebende Kurve verweist auf gelöschten Punkt
    let _ = broken(|d| {
        let idx = match &d.curves[0].kind {
            CurveKind::Line { a, .. } => a.index(),
            _ => 0,
        };
        d.points[idx].alive = false;
    });
    let _ = broken(|d| {
        d.constraints[0].kind = ConstraintKind::Radius(CurveId::from_index(0), f64::INFINITY)
    });
}

type MismatchCase = Box<dyn Fn(CurveId, CurveId, CurveId) -> ConstraintKind>;

fn first_line_and_circle(d: &SketchData) -> (CurveId, CurveId, CurveId) {
    let idx = |f: &dyn Fn(&CurveKind) -> bool| {
        CurveId::from_index(d.curves.iter().position(|c| c.alive && f(&c.kind)).unwrap())
    };
    (
        idx(&|k| matches!(k, CurveKind::Line { .. })),
        idx(&|k| matches!(k, CurveKind::Circle { .. })),
        idx(&|k| matches!(k, CurveKind::Ellipse { .. })),
    )
}

fn push_constraint(d: &mut SketchData, kind: ConstraintKind, alive: bool) {
    d.constraints.push(ConstraintRecord {
        kind,
        branch: 0,
        driven: false,
        enabled: true,
        internal: false,
        extra: None,
        alive,
    });
}

#[test]
fn type_mismatched_constraints_are_rejected_alive_or_dead() {
    for alive in [true, false] {
        let mut cases: Vec<MismatchCase> = vec![
            Box::new(|l, _, _| ConstraintKind::Radius(l, 3.0)),
            Box::new(|l, _, _| ConstraintKind::Diameter(l, 3.0)),
            Box::new(|_, c, _| ConstraintKind::Horizontal(LineOrPoints::Line(c))),
            Box::new(|_, c, _| ConstraintKind::ArcRadius(c)),
            Box::new(|_, c, _| ConstraintKind::Midpoint(PointId::from_index(0), c)),
            Box::new(|_, c, _| ConstraintKind::EllipseMajor(c, 5.0)),
            Box::new(|l, _, _| ConstraintKind::EllipseAngle(l, 0.5)),
            Box::new(|l, c, _| ConstraintKind::Equal(l, c)),
        ];
        for case in cases.drain(..) {
            let err = broken(|d| {
                let (l, c, e) = first_line_and_circle(d);
                let kind = case(l, c, e);
                push_constraint(d, kind, alive);
            });
            assert!(
                matches!(err, SketchError::WrongType(_)),
                "alive={alive}: {err:?}"
            );
        }
    }
}

#[test]
fn bad_numbers_and_splines_and_extra_are_rejected() {
    let err = broken(|d| {
        let (_, c, _) = first_line_and_circle(d);
        push_constraint(d, ConstraintKind::Radius(c, -1.0), true);
    });
    assert!(matches!(err, SketchError::Degenerate(_)));
    let err = broken(|d| {
        let (_, c, _) = first_line_and_circle(d);
        push_constraint(d, ConstraintKind::Radius(c, 0.0), true);
    });
    assert!(matches!(err, SketchError::Degenerate(_)));
    let spline = |d: &mut SketchData, n: usize| {
        let i = d
            .curves
            .iter()
            .position(|c| matches!(c.kind, CurveKind::Spline { .. }))
            .unwrap();
        d.curves[i].kind = CurveKind::Spline {
            points: vec![PointId::from_index(0); n],
        };
    };
    for n in [0, 1, 65, 10_000] {
        let err = broken(|d| spline(d, n));
        assert!(matches!(err, SketchError::InvalidInput(_)), "n={n}");
    }
    let err = broken(|d| d.constraints[0].extra = Some(d.vars.len()));
    assert!(matches!(err, SketchError::InvalidInput(_)));
}

#[test]
fn a_round_tripped_sketch_survives_all_follow_up_calls() {
    let mut t = Sketch::from_data(rich_sketch().to_data()).unwrap();
    assert!(matches!(t.solve().status, SolveStatus::Solved));
    assert!(!t.profiles().profiles.is_empty());
    assert!(t.extent().is_some());
    let d = t.to_data();
    assert_eq!(Sketch::from_data(d.clone()).unwrap().to_data(), d);
}
