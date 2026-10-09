use planora_kernel::{AnalyticKernel, Kernel};
use planora_math::{DVec2, DVec3, Frame, PI, Tolerance};
use planora_sketch::SketchError;
use planora_sketch::constraint::{ConstraintKind as K, LineOrPoints};
use planora_sketch::model::{CurveId, Sketch};
use planora_sketch::solver::SolveStatus;

fn sketch() -> Sketch {
    Sketch::new(Frame::WORLD, "S", Tolerance::default())
}

fn max_over_curve(
    s: &Sketch,
    c: CurveId,
    ts: std::ops::Range<f64>,
    f: impl Fn(DVec2) -> f64,
) -> f64 {
    let n = 4000;
    (0..=n)
        .map(|i| {
            let t = ts.start + (ts.end - ts.start) * i as f64 / n as f64;
            f(s.ellipse_point(c, t)
                .or_else(|| s.spline_point(c, t))
                .unwrap())
        })
        .fold(f64::NEG_INFINITY, f64::max)
}

#[test]
fn ellipse_points_follow_the_axes() {
    let mut s = sketch();
    let c = s.add_point(1.0, 2.0).unwrap();
    let e = s.add_ellipse(c, DVec2::new(0.0, 5.0), 3.0).unwrap();
    // große Halbachse zeigt nach +y; die kleine liegt links davon (b̂ = (−ay, ax)/|a|)
    assert!((s.ellipse_point(e, 0.0).unwrap() - DVec2::new(1.0, 7.0)).length() < 1e-12);
    assert!((s.ellipse_point(e, PI / 2.0).unwrap() - DVec2::new(-2.0, 2.0)).length() < 1e-12);
    assert!(s.add_ellipse(c, DVec2::ZERO, 1.0).is_err());
    assert!(
        s.add_ellipse(c, DVec2::new(1.0, 0.0), 2.0).is_err(),
        "klein > groß"
    );
    assert!(s.add_ellipse(c, DVec2::new(f64::NAN, 0.0), 1.0).is_err());
}

#[test]
fn splines_interpolate_their_support_points() {
    let mut s = sketch();
    let pts: Vec<_> = [(0.0, 0.0), (3.0, 4.0), (7.0, 3.0), (10.0, 0.0)]
        .iter()
        .map(|&(x, y)| s.add_point(x, y).unwrap())
        .collect();
    let sp = s.add_spline(&pts).unwrap();
    for (i, p) in pts.iter().enumerate() {
        let got = s.spline_point(sp, i as f64).unwrap();
        assert!(
            got.distance(s.point_xy(*p).unwrap()) < 1e-9,
            "Stützpunkt {i}"
        );
    }
    // natürlicher Spline: stetig zwischen den Segmenten
    let a = s.spline_point(sp, 1.0 - 1e-9).unwrap();
    let b = s.spline_point(sp, 1.0 + 1e-9).unwrap();
    assert!(a.distance(b) < 1e-6);
    assert!(s.add_spline(&pts[..1]).is_err());
    assert!(s.add_spline(&[pts[0], pts[0]]).is_err());
}

#[test]
fn ellipse_dimensions_and_point_on_ellipse() {
    let mut s = sketch();
    let c = s.add_point(0.0, 0.0).unwrap();
    let e = s.add_ellipse(c, DVec2::new(4.0, 1.0), 2.0).unwrap();
    let p = s.add_point(3.0, 2.5).unwrap();
    s.add_constraint(K::Fixed(c, 0.0, 0.0)).unwrap();
    s.add_constraint(K::EllipseMajor(e, 5.0)).unwrap();
    s.add_constraint(K::EllipseMinor(e, 3.0)).unwrap();
    s.add_constraint(K::EllipseAngle(e, 0.0)).unwrap();
    s.add_constraint(K::PointOnCurve(p, e)).unwrap();
    let rep = s.solve();
    assert_eq!(rep.status, SolveStatus::Solved, "{rep:?}");
    let pe = s.point_xy(p).unwrap();
    // Ellipse x²/25 + y²/9 = 1
    assert!(
        ((pe.x / 5.0).powi(2) + (pe.y / 3.0).powi(2) - 1.0).abs() < 1e-7,
        "{pe:?}"
    );
    assert!((s.measure(&K::EllipseMajor(e, 0.0)).unwrap() - 5.0).abs() < 1e-7);
    assert!(s.measure(&K::EllipseAngle(e, 0.0)).unwrap().abs() < 1e-7);
}

#[test]
fn a_line_can_be_tangent_to_an_ellipse() {
    let mut s = sketch();
    let c = s.add_point(0.0, 0.0).unwrap();
    let e = s.add_ellipse(c, DVec2::new(5.0, 0.5), 2.0).unwrap();
    let l0 = s.add_point(-10.0, 4.0).unwrap();
    let l1 = s.add_point(10.0, 4.0).unwrap();
    let line = s.add_line(l0, l1).unwrap();
    s.add_constraint(K::Fixed(c, 0.0, 0.0)).unwrap();
    s.add_constraint(K::Fixed(l0, -10.0, 4.0)).unwrap();
    s.add_constraint(K::Fixed(l1, 10.0, 4.0)).unwrap();
    s.add_constraint(K::EllipseMajor(e, 5.0)).unwrap();
    s.add_constraint(K::EllipseMinor(e, 2.0)).unwrap();
    s.add_constraint(K::Tangent(line, e)).unwrap();
    let rep = s.solve();
    assert_eq!(rep.status, SolveStatus::Solved, "{rep:?}");
    // die Ellipse berührt y = 4 von unten: ihr höchster Punkt liegt bei y = 4
    let top = max_over_curve(&s, e, 0.0..2.0 * PI, |p| p.y);
    assert!((top - 4.0).abs() < 1e-4, "höchster Punkt {top}");
}

#[test]
fn a_point_can_lie_on_a_spline_and_a_line_can_touch_it() {
    let mut s = sketch();
    let a = s.add_point(0.0, 0.0).unwrap();
    let m = s.add_point(5.0, 4.0).unwrap();
    let b = s.add_point(10.0, 0.0).unwrap();
    let sp = s.add_spline(&[a, m, b]).unwrap();
    let q = s.add_point(2.4, 2.6).unwrap();
    s.add_constraint(K::Fixed(a, 0.0, 0.0)).unwrap();
    s.add_constraint(K::Fixed(b, 10.0, 0.0)).unwrap();
    s.add_constraint(K::Fixed(m, 5.0, 4.0)).unwrap();
    s.add_constraint(K::PointOnCurve(q, sp)).unwrap();
    let rep = s.solve();
    assert_eq!(rep.status, SolveStatus::Solved, "{rep:?}");
    let pq = s.point_xy(q).unwrap();
    // 200 000 Schritte: Bei |S′| ≈ 7 liegen Abtastpunkte höchstens ≈ 3,5e-5 vom Kurvenpunkt.
    let dist = (0..=200_000)
        .map(|i| {
            s.spline_point(sp, 2.0 * i as f64 / 200_000.0)
                .unwrap()
                .distance(pq)
        })
        .fold(f64::INFINITY, f64::min);
    assert!(dist < 1e-4, "Abstand zur Kurve {dist}");

    // Tangente: Stützpunkt m darf wandern; die waagerechte Linie y = 4.5 berührt den Spline
    let mut s = sketch();
    let a = s.add_point(0.0, 0.0).unwrap();
    let m = s.add_point(5.0, 4.0).unwrap();
    let b = s.add_point(10.0, 0.0).unwrap();
    let sp = s.add_spline(&[a, m, b]).unwrap();
    let l0 = s.add_point(-1.0, 4.5).unwrap();
    let l1 = s.add_point(11.0, 4.5).unwrap();
    let line = s.add_line(l0, l1).unwrap();
    for (p, x, y) in [
        (a, 0.0, 0.0),
        (b, 10.0, 0.0),
        (l0, -1.0, 4.5),
        (l1, 11.0, 4.5),
    ] {
        s.add_constraint(K::Fixed(p, x, y)).unwrap();
    }
    s.add_constraint(K::Tangent(line, sp)).unwrap();
    let rep = s.solve();
    assert_eq!(rep.status, SolveStatus::Solved, "{rep:?}");
    let top = max_over_curve(&s, sp, 0.0..2.0, |p| p.y);
    assert!((top - 4.5).abs() < 1e-3, "höchster Punkt {top}");
    let _ = LineOrPoints::Line(line);
}

#[test]
fn ellipses_and_splines_never_become_solids() {
    let mut s = sketch();
    let c = s.add_point(0.0, 0.0).unwrap();
    s.add_ellipse(c, DVec2::new(3.0, 0.0), 2.0).unwrap();
    // ein Rechteck daneben bleibt ein gültiges Profil
    s.add_rectangle(DVec2::new(10.0, 0.0), DVec2::new(14.0, 3.0))
        .unwrap();
    let e = s.profiles();
    assert_eq!(e.profiles.len(), 1);
    assert_eq!(e.rejected.len(), 1);
    assert!(matches!(&e.rejected[0].reason, SketchError::UnsupportedForSolid(w) if w == "Ellipse"));

    let mut s = sketch();
    let a = s.add_point(0.0, 0.0).unwrap();
    let m = s.add_point(2.0, 2.0).unwrap();
    let b = s.add_point(4.0, 0.0).unwrap();
    s.add_spline(&[a, m, b]).unwrap();
    s.add_line(b, a).unwrap();
    let e = s.profiles();
    assert!(e.profiles.is_empty());
    assert!(matches!(&e.rejected[0].reason, SketchError::UnsupportedForSolid(w) if w == "Spline"));
}

#[test]
fn a_tilted_circle_projects_to_an_ellipse() {
    let k = AnalyticKernel::default();
    let cyl = k.make_cylinder(3.0, 8.0).unwrap();
    // Skizzenebene um 30° gegen die Kreisebene geneigt
    let normal = DVec3::new(0.0, 0.5, 3.0_f64.sqrt() / 2.0);
    let mut s = Sketch::new(
        Frame::from_origin_normal(DVec3::ZERO, normal),
        "S",
        Tolerance::default(),
    );
    let mut ellipses = 0;
    for (id, _) in cyl.edges.iter() {
        if let Ok(c) = s.add_projected_edge(&cyl, id)
            && let Some(planora_sketch::model::CurveKind::Ellipse { ax, ay, minor, .. }) =
                s.curve(c).map(|d| d.kind.clone())
        {
            ellipses += 1;
            let vars = s.variables();
            let (a, b) = (DVec2::new(vars[ax], vars[ay]).length(), vars[minor]);
            assert!((a - 3.0).abs() < 1e-9, "große Halbachse {a}");
            assert!(
                (b - 3.0 * (PI / 6.0).cos()).abs() < 1e-9,
                "kleine Halbachse {b}"
            );
        }
    }
    assert_eq!(ellipses, 2, "unterer und oberer Kreis");
    assert_eq!(s.solve().status, SolveStatus::Solved);
}

// ---- Zusatztests (über den Plan hinaus) ---------------------------------------------------

#[test]
fn parametric_constraints_get_an_extra_variable_only_on_success() {
    let mut s = sketch();
    let a = s.add_point(0.0, 0.0).unwrap();
    let m = s.add_point(5.0, 4.0).unwrap();
    let b = s.add_point(10.0, 0.0).unwrap();
    let sp = s.add_spline(&[a, m, b]).unwrap();
    let c = s.add_point(20.0, 0.0).unwrap();
    let e = s.add_ellipse(c, DVec2::new(3.0, 0.0), 1.0).unwrap();
    let q = s.add_point(2.0, 3.0).unwrap();
    let n = s.variable_count();

    // falsche Typen: nichts wird angelegt, auch keine Zusatzvariable
    assert!(matches!(
        s.add_constraint(K::Tangent(sp, e)),
        Err(SketchError::WrongType(_))
    ));
    assert!(matches!(
        s.add_constraint(K::EllipseMajor(sp, 2.0)),
        Err(SketchError::WrongType(_))
    ));
    assert!(matches!(
        s.add_constraint(K::Radius(e, 2.0)),
        Err(SketchError::WrongType(_))
    ));
    assert!(s.add_constraint(K::EllipseMinor(e, 0.0)).is_err());
    assert!(s.add_constraint(K::EllipseAngle(e, f64::NAN)).is_err());
    assert_eq!(s.variable_count(), n);
    assert_eq!(s.constraints().count(), 0);

    // implizit auf der Ellipse: keine Zusatzvariable
    let on_e = s.add_constraint(K::PointOnCurve(q, e)).unwrap();
    assert_eq!(s.constraint(on_e).unwrap().extra, None);
    assert_eq!(s.variable_count(), n);

    // parametrisch auf dem Spline: eine Zusatzvariable, die in `locals` steht
    let on_sp = s.add_constraint(K::PointOnCurve(q, sp)).unwrap();
    let t = s.constraint(on_sp).unwrap().extra.expect("Zusatzvariable");
    assert_eq!(s.variable_count(), n + 1);
    assert_eq!(t, n);
    let ev = s.evaluate(on_sp, s.variables()).unwrap();
    assert!(ev.locals.contains(&t));
    assert_eq!(ev.residuals.len(), 2);
    // Startwert: nächstgelegener Kurvenparameter
    let start = s.spline_point(sp, s.variables()[t]).unwrap();
    let best = (0..=2000)
        .map(|i| {
            s.spline_point(sp, 2.0 * i as f64 / 2000.0)
                .unwrap()
                .distance(DVec2::new(2.0, 3.0))
        })
        .fold(f64::INFINITY, f64::min);
    assert!(start.distance(DVec2::new(2.0, 3.0)) <= best + 1e-9);

    // Entfernen: die Abhängigkeit entfällt, ihre Zusatzvariable wird nicht mehr benutzt
    s.remove_constraint(on_sp).unwrap();
    s.remove_constraint(on_e).unwrap();
    let rep = s.solve();
    assert_eq!(rep.status, SolveStatus::Solved);
    // freie Variablen: 5 Punkte (10) + Ellipse (3); die tote Zusatzvariable zählt nicht
    assert_eq!(rep.dof, 13, "{rep:?}");
}

#[test]
fn removing_a_support_point_removes_the_spline_and_its_constraints() {
    let mut s = sketch();
    let a = s.add_point(0.0, 0.0).unwrap();
    let m = s.add_point(5.0, 4.0).unwrap();
    let b = s.add_point(10.0, 0.0).unwrap();
    let sp = s.add_spline(&[a, m, b]).unwrap();
    let q = s.add_point(2.0, 3.0).unwrap();
    let k = s.add_constraint(K::PointOnCurve(q, sp)).unwrap();
    s.remove_point(m).unwrap();
    assert!(s.curve(sp).is_none());
    assert!(s.constraint(k).is_none());
    assert_eq!(s.solve().status, SolveStatus::Solved);
}

#[test]
fn spline_input_is_validated() {
    let mut s = sketch();
    let a = s.add_point(0.0, 0.0).unwrap();
    let b = s.add_point(1.0, 0.0).unwrap();
    let near = s.add_point(1.0, 1e-9).unwrap();
    let bogus = planora_kernel::Id::from_index(999);
    assert_eq!(s.add_spline(&[a, bogus]), Err(SketchError::UnknownObject));
    assert!(matches!(
        s.add_spline(&[a, b, near]),
        Err(SketchError::Degenerate(_))
    ));
    assert!(s.add_spline(&[a, b, a]).is_err(), "Punkt doppelt");
    let many: Vec<_> = (0..65)
        .map(|i| s.add_point(i as f64, 0.0).unwrap())
        .collect();
    assert!(s.add_spline(&many).is_err());
    assert!(s.add_spline(&many[..64]).is_ok());
    assert_eq!(s.curves().count(), 1);
    // Parameter außerhalb wird geklemmt
    let sp = s.add_spline(&[a, b]).unwrap();
    assert!((s.spline_point(sp, -3.0).unwrap() - DVec2::ZERO).length() < 1e-12);
    assert!((s.spline_point(sp, 7.0).unwrap() - DVec2::X).length() < 1e-12);
    assert_eq!(s.spline_point(sp, f64::NAN), None);
    assert_eq!(s.ellipse_point(sp, 0.0), None);
}

#[test]
fn whole_ellipse_edges_project_and_edge_on_circles_are_rejected() {
    use planora_kernel::{Curve, Solid};
    let mut solid = Solid::new();
    let v = solid.add_vertex(DVec3::new(2.0, 0.0, 0.0));
    let frame = Frame {
        origin: DVec3::new(1.0, 1.0, 5.0),
        x: DVec3::Y,
        y: -DVec3::X,
        z: DVec3::Z,
    };
    let ellipse = solid.add_edge(
        Curve::Ellipse {
            frame,
            major: 2.0,
            minor: 1.0,
        },
        0.0,
        2.0 * PI,
        v,
        v,
    );
    let edge_on = solid.add_edge(
        Curve::Circle {
            frame: Frame::from_origin_normal(DVec3::ZERO, DVec3::X),
            radius: 1.0,
        },
        0.0,
        2.0 * PI,
        v,
        v,
    );
    let mut s = sketch();
    let e = s.add_projected_edge(&solid, ellipse).unwrap();
    assert!(s.curve(e).unwrap().construction);
    assert!((s.measure(&K::EllipseMajor(e, 0.0)).unwrap() - 2.0).abs() < 1e-12);
    assert!((s.measure(&K::EllipseMinor(e, 0.0)).unwrap() - 1.0).abs() < 1e-12);
    // große Halbachse entlang ±y
    assert!((s.measure(&K::EllipseAngle(e, 0.0)).unwrap().abs() - PI / 2.0).abs() < 1e-12);
    for t in [0.0, 1.0, 2.5] {
        let p = s.ellipse_point(e, t).unwrap();
        assert!((((p.x - 1.0) / 1.0).powi(2) + ((p.y - 1.0) / 2.0).powi(2) - 1.0).abs() < 1e-12);
    }
    let before = (s.points().count(), s.variable_count());
    assert!(matches!(
        s.add_projected_edge(&solid, edge_on),
        Err(SketchError::UnsupportedProjection(_))
    ));
    assert_eq!((s.points().count(), s.variable_count()), before);
    let rep = s.solve();
    assert_eq!(rep.status, SolveStatus::Solved);
    assert!(rep.redundant.is_empty(), "{rep:?}");
    assert_eq!(rep.dof, 0);
}
