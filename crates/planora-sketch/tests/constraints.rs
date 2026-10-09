use planora_math::{DVec2, Frame, Tolerance};
use planora_sketch::SketchError;
use planora_sketch::constraint::{ConstraintId, ConstraintKind as K, LineOrPoints};
use planora_sketch::model::{CurveId, PointId, Sketch};

struct Fixture {
    s: Sketch,
    p: Vec<PointId>,
    l: Vec<CurveId>,
    c: Vec<CurveId>,
    arc: CurveId,
}

fn fixture() -> Fixture {
    let mut s = Sketch::new(Frame::WORLD, "S", Tolerance::default());
    let xy = [
        (0.0, 0.0),
        (7.0, 1.0),
        (3.0, 5.0),
        (8.0, 6.0),
        (-2.0, 3.0),
        (4.0, -1.0),
    ];
    let p: Vec<PointId> = xy
        .iter()
        .map(|&(x, y)| s.add_point(x, y).unwrap())
        .collect();
    let l = vec![
        s.add_line(p[0], p[1]).unwrap(),
        s.add_line(p[2], p[3]).unwrap(),
        s.add_line(p[4], p[5]).unwrap(),
    ];
    let c = vec![
        s.add_circle(p[2], 2.0).unwrap(),
        s.add_circle(p[5], 3.5).unwrap(),
    ];
    let arc = s.add_arc(p[0], p[1], p[3]).unwrap();
    Fixture { s, p, l, c, arc }
}

fn all_kinds(f: &Fixture) -> Vec<K> {
    let (p, l, c) = (&f.p, &f.l, &f.c);
    vec![
        K::Coincident(p[0], p[3]),
        K::PointOnCurve(p[4], l[0]),
        K::PointOnCurve(p[4], c[0]),
        K::PointOnCurve(p[4], f.arc),
        K::Horizontal(LineOrPoints::Line(l[0])),
        K::Horizontal(LineOrPoints::Points(p[0], p[3])),
        K::Vertical(LineOrPoints::Line(l[1])),
        K::Parallel(l[0], l[1]),
        K::Perpendicular(l[0], l[2]),
        K::Tangent(l[0], c[0]),
        K::Tangent(c[0], l[0]),
        K::Tangent(c[0], c[1]),
        K::Tangent(f.arc, c[1]),
        K::Equal(l[0], l[1]),
        K::Equal(c[0], c[1]),
        K::Fixed(p[1], 7.5, 1.5),
        K::Symmetric(p[0], p[3], l[2]),
        K::Midpoint(p[4], l[0]),
        K::Distance(p[0], p[3], 9.0),
        K::DistancePointLine(p[4], l[0], 3.0),
        K::Radius(c[0], 2.0),
        K::Radius(f.arc, 7.0),
        K::Diameter(c[1], 7.0),
        K::Angle(l[0], l[2], 1.0),
    ]
}

/// Legt `kind` an und vergleicht die Ableitungen aller Residuen nach allen lokalen Variablen
/// (einschließlich einer Zusatzvariablen) mit zentralen Differenzenquotienten.
fn check_jacobian(s: &mut Sketch, kind: K) {
    let id = s.add_constraint(kind.clone()).unwrap();
    let x0 = s.variables().to_vec();
    let ev = s.evaluate(id, &x0).unwrap();
    assert!(!ev.residuals.is_empty(), "{kind:?}");
    if let Some(extra) = s.constraint(id).unwrap().extra {
        assert!(ev.locals.contains(&extra), "{kind:?}: Zusatzvariable fehlt");
    }
    for (ri, r) in ev.residuals.iter().enumerate() {
        assert!(r.v.is_finite(), "{kind:?}");
        for (k, &var) in ev.locals.iter().enumerate() {
            let h = 1e-6;
            let mut xp = x0.clone();
            xp[var] += h;
            let mut xm = x0.clone();
            xm[var] -= h;
            let fp = s.evaluate(id, &xp).unwrap().residuals[ri].v;
            let fm = s.evaluate(id, &xm).unwrap().residuals[ri].v;
            let fd = (fp - fm) / (2.0 * h);
            let g = r.grad(k);
            assert!(
                (g - fd).abs() < 1e-5 * (1.0 + fd.abs()),
                "{kind:?} Residuum {ri} Variable {var}: {g} vs {fd}"
            );
        }
    }
}

#[test]
fn jacobians_match_finite_differences_for_every_kind() {
    let mut f = fixture();
    for kind in all_kinds(&f) {
        check_jacobian(&mut f.s, kind);
    }
}

#[test]
fn jacobians_match_finite_differences_for_ellipses_and_splines() {
    let mut s = Sketch::new(Frame::WORLD, "S", Tolerance::default());
    let c = s.add_point(1.0, -0.5).unwrap();
    let e = s.add_ellipse(c, DVec2::new(4.0, 1.5), 2.0).unwrap();
    let sp_pts: Vec<PointId> = [(0.0, 0.0), (3.0, 4.0), (7.0, 3.0), (10.0, -1.0)]
        .iter()
        .map(|&(x, y)| s.add_point(x, y).unwrap())
        .collect();
    let sp = s.add_spline(&sp_pts).unwrap();
    let q = s.add_point(3.3, 2.1).unwrap();
    let r = s.add_point(5.2, 5.5).unwrap();
    let l0 = s.add_point(-6.0, 4.0).unwrap();
    let l1 = s.add_point(6.0, 5.0).unwrap();
    let line = s.add_line(l0, l1).unwrap();
    let m0 = s.add_point(-2.0, 7.0).unwrap();
    let m1 = s.add_point(12.0, 6.0).unwrap();
    let line2 = s.add_line(m0, m1).unwrap();
    for kind in [
        K::EllipseMajor(e, 5.0),
        K::EllipseMinor(e, 1.5),
        K::EllipseAngle(e, 0.3),
        K::PointOnCurve(q, e),
        K::PointOnCurve(r, sp),
        K::Tangent(line, e),
        K::Tangent(e, line),
        K::Tangent(line2, sp),
        K::Tangent(sp, line2),
    ] {
        check_jacobian(&mut s, kind);
    }
}

fn residual_norm(s: &Sketch, id: ConstraintId) -> f64 {
    let ev = s.evaluate(id, s.variables()).unwrap();
    ev.residuals.iter().map(|r| r.v.abs()).fold(0.0, f64::max)
}

#[test]
fn dimensions_taken_from_the_current_geometry_start_satisfied() {
    let mut f = fixture();
    let (p, l, c) = (f.p.clone(), f.l.clone(), f.c.clone());
    for kind in [
        K::Distance(p[0], p[3], 0.0),
        K::DistancePointLine(p[4], l[0], 0.0),
        K::DistancePointLine(p[2], l[0], 0.0),
        K::Radius(c[0], 1.0),
        K::Diameter(c[1], 1.0),
        K::Angle(l[0], l[2], 0.0),
    ] {
        let v = f.s.measure(&kind).unwrap();
        let kind = set_value(kind, v);
        let id = f.s.add_constraint(kind.clone()).unwrap();
        assert!(residual_norm(&f.s, id) < 1e-9, "{kind:?}");
    }
    assert_eq!(f.s.measure(&K::Parallel(l[0], l[1])), None, "kein Maß");
}

fn set_value(kind: K, v: f64) -> K {
    match kind {
        K::Distance(a, b, _) => K::Distance(a, b, v),
        K::DistancePointLine(a, l, _) => K::DistancePointLine(a, l, v),
        K::Radius(c, _) => K::Radius(c, v),
        K::Diameter(c, _) => K::Diameter(c, v),
        K::Angle(a, b, _) => K::Angle(a, b, v),
        other => other,
    }
}

#[test]
fn satisfied_geometry_gives_zero_residuals() {
    let mut s = Sketch::new(Frame::WORLD, "S", Tolerance::default());
    let a = s.add_point(0.0, 0.0).unwrap();
    let b = s.add_point(4.0, 0.0).unwrap();
    let c = s.add_point(4.0, 3.0).unwrap();
    let d = s.add_point(0.0, 5.0).unwrap();
    let e = s.add_point(4.0, 5.0).unwrap();
    let ab = s.add_line(a, b).unwrap();
    let bc = s.add_line(b, c).unwrap();
    let de = s.add_line(d, e).unwrap();
    let circle = s.add_circle(c, 1.0).unwrap();
    // y = 4 berührt den Kreis um (4, 3) mit r = 1 von oben
    let t0 = s.add_point(0.0, 4.0).unwrap();
    let t1 = s.add_point(9.0, 4.0).unwrap();
    let tangent_line = s.add_line(t0, t1).unwrap();
    let mid = s.add_point(2.0, 0.0).unwrap();
    for (kind, name) in [
        (K::Horizontal(LineOrPoints::Line(ab)), "horizontal"),
        (K::Vertical(LineOrPoints::Line(bc)), "vertikal"),
        (K::Parallel(ab, de), "parallel"),
        (K::Perpendicular(ab, bc), "senkrecht"),
        (K::Tangent(tangent_line, circle), "tangential"),
        (K::Equal(ab, de), "gleich"),
        (K::Midpoint(mid, ab), "Mitte"),
        (K::PointOnCurve(mid, ab), "Punkt auf Linie"),
        (K::Fixed(mid, 2.0, 0.0), "fest"),
    ] {
        let id = s.add_constraint(kind).unwrap();
        assert!(residual_norm(&s, id) < 1e-9, "{name}");
    }
}

#[test]
fn unsatisfied_constraints_have_nonzero_residuals() {
    let mut s = Sketch::new(Frame::WORLD, "S", Tolerance::default());
    let a = s.add_point(0.0, 0.0).unwrap();
    let b = s.add_point(4.0, 1.0).unwrap();
    let c = s.add_point(4.0, 3.0).unwrap();
    let ab = s.add_line(a, b).unwrap();
    let bc = s.add_line(b, c).unwrap();
    // a und b liegen nicht symmetrisch zur Achse bc (x = 4)
    let sym = s.add_constraint(K::Symmetric(a, b, bc)).unwrap();
    assert!(residual_norm(&s, sym) > 1e-3);
    let h = s
        .add_constraint(K::Horizontal(LineOrPoints::Line(ab)))
        .unwrap();
    assert!(residual_norm(&s, h) > 1e-3);
}

#[test]
fn wrong_types_and_bad_values_are_rejected() {
    let mut f = fixture();
    let (p, l, c) = (f.p.clone(), f.l.clone(), f.c.clone());
    assert!(matches!(
        f.s.add_constraint(K::Parallel(l[0], c[0])),
        Err(SketchError::WrongType(_))
    ));
    assert!(matches!(
        f.s.add_constraint(K::Radius(l[0], 2.0)),
        Err(SketchError::WrongType(_))
    ));
    assert!(matches!(
        f.s.add_constraint(K::Horizontal(LineOrPoints::Line(c[0]))),
        Err(SketchError::WrongType(_))
    ));
    assert!(f.s.add_constraint(K::Tangent(l[0], l[1])).is_err());
    assert!(
        f.s.add_constraint(K::Distance(p[0], p[1], f64::NAN))
            .is_err()
    );
    assert!(f.s.add_constraint(K::Distance(p[0], p[0], 1.0)).is_err());
    assert!(f.s.add_constraint(K::Radius(c[0], 0.0)).is_err());
    assert!(
        f.s.add_constraint(K::Fixed(p[0], f64::INFINITY, 0.0))
            .is_err()
    );
    let bogus_point = planora_kernel::Id::from_index(500);
    assert_eq!(
        f.s.add_constraint(K::Coincident(p[0], bogus_point)),
        Err(SketchError::UnknownObject)
    );
    assert_eq!(f.s.constraints().count(), 0, "nichts wurde angelegt");
}

#[test]
fn arcs_carry_a_hidden_internal_constraint() {
    let f = fixture();
    assert_eq!(
        f.s.constraints().count(),
        0,
        "interne Abhängigkeiten sind unsichtbar"
    );
    let arc_internal =
        f.s.internal_constraints()
            .filter(|(_, c)| c.kind == K::ArcRadius(f.arc))
            .count();
    assert_eq!(arc_internal, 1);
}

#[test]
fn dimension_values_can_be_changed_and_flags_set() {
    let mut f = fixture();
    let id = f.s.add_constraint(K::Radius(f.c[0], 2.0)).unwrap();
    f.s.set_constraint_value(id, 3.0).unwrap();
    assert_eq!(f.s.constraint(id).unwrap().kind, K::Radius(f.c[0], 3.0));
    assert!(f.s.set_constraint_value(id, f64::NAN).is_err());
    let h =
        f.s.add_constraint(K::Horizontal(LineOrPoints::Line(f.l[0])))
            .unwrap();
    assert!(f.s.set_constraint_value(h, 1.0).is_err(), "kein Maß");
    f.s.set_driven(id, true).unwrap();
    f.s.set_enabled(h, false).unwrap();
    assert!(f.s.constraint(id).unwrap().driven);
    assert!(!f.s.constraint(h).unwrap().enabled);
}

#[test]
fn removing_objects_cascades() {
    let mut f = fixture();
    let (p, l, c) = (f.p.clone(), f.l.clone(), f.c.clone());
    let k1 = f.s.add_constraint(K::Parallel(l[0], l[1])).unwrap();
    let k2 = f.s.add_constraint(K::Radius(c[0], 2.0)).unwrap();
    let k3 = f.s.add_constraint(K::Coincident(p[0], p[1])).unwrap();
    f.s.remove_curve(l[0]).unwrap();
    assert!(f.s.curve(l[0]).is_none());
    assert!(
        f.s.constraint(k1).is_none(),
        "abhängige Abhängigkeit entfällt"
    );
    assert!(f.s.constraint(k2).is_some());
    assert_eq!(f.s.remove_curve(l[0]), Err(SketchError::UnknownObject));
    // Punkt p[1] wird von der Linie l[0] (entfernt), dem Bogen und k3 benutzt
    f.s.remove_point(p[1]).unwrap();
    assert!(f.s.curve(f.arc).is_none(), "Bogen hing am Punkt");
    assert!(f.s.constraint(k3).is_none());
    assert!(
        f.s.internal_constraints()
            .all(|(_, c)| c.kind != K::ArcRadius(f.arc)),
        "die interne Abhängigkeit des Bogens entfällt mit ihm"
    );
    let before = f.s.points().count();
    let pruned = f.s.prune_unused_points();
    assert_eq!(f.s.points().count(), before - pruned);
    assert!(pruned >= 1, "p[0] hing nur noch an entfernten Objekten");
    assert!(f.s.remove_constraint(k2).is_ok());
    assert!(f.s.remove_constraint(k2).is_err());
}

#[test]
fn internal_arc_radius_jacobian_matches_finite_differences() {
    let f = fixture();
    let (id, _) =
        f.s.internal_constraints()
            .find(|(_, c)| c.kind == K::ArcRadius(f.arc))
            .unwrap();
    let x0 = f.s.variables().to_vec();
    let ev = f.s.evaluate(id, &x0).unwrap();
    assert_eq!(ev.residuals.len(), 1);
    assert_eq!(ev.locals.len(), 6, "Mitte, Start und Ende");
    let r = &ev.residuals[0];
    for (k, &var) in ev.locals.iter().enumerate() {
        let h = 1e-6;
        let mut xp = x0.clone();
        xp[var] += h;
        let mut xm = x0.clone();
        xm[var] -= h;
        let fd = (f.s.evaluate(id, &xp).unwrap().residuals[0].v
            - f.s.evaluate(id, &xm).unwrap().residuals[0].v)
            / (2.0 * h);
        assert!(
            (r.grad(k) - fd).abs() < 1e-5 * (1.0 + fd.abs()),
            "Variable {var}"
        );
    }
}
