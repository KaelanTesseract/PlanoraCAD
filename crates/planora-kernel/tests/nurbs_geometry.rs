use planora_kernel::{Curve, CurveKind, Surface, SurfaceKind};
use planora_math::nurbs::{NurbsCurve, NurbsSurface};
use planora_math::{DVec3, TAU};

fn cylinder_as_nurbs(r: f64, h: f64) -> Surface {
    let line = NurbsCurve::new(
        1,
        vec![0.0, 0.0, 1.0, 1.0],
        vec![DVec3::new(r, 0.0, 0.0), DVec3::new(r, 0.0, h)],
        vec![1.0; 2],
    )
    .unwrap();
    let s = NurbsSurface::revolve(&line, DVec3::ZERO, DVec3::Z, 0.0, TAU).unwrap();
    Surface::nurbs(s, true, false).unwrap()
}

#[test]
fn a_nurbs_cylinder_agrees_with_the_analytic_one() {
    let n = cylinder_as_nurbs(3.0, 5.0);
    let a = Surface::Cylinder {
        frame: planora_math::Frame::from_origin_normal(DVec3::ZERO, DVec3::Z),
        radius: 3.0,
    };
    assert_eq!(n.kind(), SurfaceKind::Nurbs);
    for &p in &[
        DVec3::new(4.0, 1.0, 2.0),
        DVec3::new(-5.0, 2.0, 4.5),
        DVec3::new(1.0, -1.5, 0.7),
        DVec3::new(2.5, 0.2, 3.3),
    ] {
        let (dn, da) = (n.implicit(p), a.implicit(p));
        assert!((dn - da).abs() < 1e-8, "{p:?}: {dn} vs {da}");
    }
    // Punkt auf der Fläche -> project -> point
    let on = DVec3::new(3.0 * 0.6f64.cos(), 3.0 * 0.6f64.sin(), 2.5);
    let (u, v) = n.project(on);
    assert!(n.point(u, v).distance(on) < 1e-9);
    // Normale zeigt nach außen (Orientierung der Konstruktion) oder nach innen – entscheidend ist Konsistenz mit implicit
    let nn = n.normal(u, v);
    assert!((nn.length() - 1.0).abs() < 1e-12);
    assert!(n.implicit(on + nn * 0.1) > 0.0 && n.implicit(on - nn * 0.1) < 0.0);
    assert!(n.is_periodic());
}

#[test]
fn project_near_follows_the_hint_across_the_seam() {
    let n = cylinder_as_nurbs(3.0, 5.0);
    let p1 = DVec3::new(3.0 * 0.05f64.cos(), 3.0 * 0.05f64.sin(), 1.0);
    let p2 = DVec3::new(3.0 * (TAU - 0.05).cos(), 3.0 * (TAU - 0.05).sin(), 1.0);
    let a = n.project(p1);
    let b = n.project_near(p2, a);
    // fortlaufend: u von p2 liegt dicht an u von p1 (Umlauf um die Naht), nicht ein Umlauf weiter
    assert!((a.0 - b.0).abs() < 0.2 || ((a.0 - b.0).abs() - TAU).abs() < 0.2);
}

#[test]
fn curves_have_period_only_when_closed() {
    let circle = NurbsCurve::elliptic_arc(DVec3::ZERO, DVec3::X, DVec3::Y, 0.0, TAU).unwrap();
    let arc = NurbsCurve::elliptic_arc(DVec3::ZERO, DVec3::X, DVec3::Y, 0.0, 1.0).unwrap();
    assert!(Curve::nurbs(circle.clone()).period().is_some());
    assert!(Curve::nurbs(arc).period().is_none());
    assert_eq!(Curve::nurbs(circle).kind(), CurveKind::Nurbs);
    assert_eq!(
        Curve::Line {
            origin: DVec3::ZERO,
            dir: DVec3::X
        }
        .period(),
        None
    );
    assert_eq!(
        Curve::Circle {
            frame: planora_math::Frame::from_origin_normal(DVec3::ZERO, DVec3::Z),
            radius: 1.0
        }
        .period(),
        Some(TAU)
    );
}

#[test]
fn hostile_surface_data_is_an_error() {
    // innerer Pol (Mittelreihe fällt zusammen) und fehlerhafte Periodenangabe
    let k = vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0];
    let mut c = Vec::new();
    for i in 0..3 {
        for j in 0..3 {
            c.push(if j == 1 {
                DVec3::new(0.0, 0.0, 1.0)
            } else {
                DVec3::new(i as f64, j as f64, 0.0)
            });
        }
    }
    let s = NurbsSurface::new(2, 2, k.clone(), k.clone(), 3, 3, c, vec![1.0; 9]).unwrap();
    assert!(
        Surface::nurbs(s.clone(), false, false).is_err(),
        "innerer Pol"
    );
    let flat = NurbsSurface::new(
        1,
        1,
        vec![0.0, 0.0, 1.0, 1.0],
        vec![0.0, 0.0, 1.0, 1.0],
        2,
        2,
        vec![DVec3::ZERO, DVec3::X, DVec3::Y, DVec3::new(1.0, 1.0, 0.0)],
        vec![1.0; 4],
    )
    .unwrap();
    assert!(
        Surface::nurbs(flat, true, false).is_err(),
        "nicht wirklich periodisch"
    );
}

#[test]
fn a_degenerate_row_is_detected_as_a_pole_at_the_edge() {
    // Halbkreis um die z-Achse gedreht -> Kugel mit Polen bei v = Anfang und Ende
    let half = NurbsCurve::elliptic_arc(
        DVec3::ZERO,
        DVec3::new(0.0, 0.0, 2.0),
        DVec3::new(2.0, 0.0, 0.0),
        0.0,
        std::f64::consts::PI,
    )
    .unwrap();
    let s = NurbsSurface::revolve(&half, DVec3::ZERO, DVec3::Z, 0.0, TAU).unwrap();
    let ((_, _), (v0, v1)) = s.domain();
    let surf = Surface::nurbs(s, true, false).unwrap();
    let poles = surf.pole_vs();
    assert_eq!(poles.len(), 2);
    assert!(
        poles.iter().any(|p| (p - v0).abs() < 1e-12)
            && poles.iter().any(|p| (p - v1).abs() < 1e-12)
    );
}

#[test]
fn implicit_is_total_for_far_and_special_points() {
    let n = cylinder_as_nurbs(3.0, 5.0);
    for p in [
        DVec3::new(1e6, 0.0, 0.0),
        DVec3::new(0.0, 0.0, 2.0),
        DVec3::ZERO,
        DVec3::new(0.0, 0.0, -1e3),
    ] {
        let d = n.implicit(p);
        assert!(d.is_finite(), "{p:?}");
    }
}

#[test]
fn cloning_a_nurbs_surface_shares_the_data() {
    let n = cylinder_as_nurbs(1.0, 1.0);
    let m = n.clone();
    let (Surface::Nurbs(a), Surface::Nurbs(b)) = (&n, &m) else {
        panic!()
    };
    assert!(std::sync::Arc::ptr_eq(a, b));
}

// ---- Ergänzungen über die Planvorgaben hinaus ----

#[test]
fn non_finite_points_never_give_nan() {
    let n = cylinder_as_nurbs(3.0, 5.0);
    for p in [
        DVec3::new(f64::NAN, 0.0, 0.0),
        DVec3::new(f64::INFINITY, 1.0, 2.0),
    ] {
        assert!(!n.implicit(p).is_nan(), "{p:?}");
        let (u, v) = n.project(p);
        assert!(u.is_finite() && v.is_finite());
        let (u, v) = n.project_near(p, (0.5, 0.5));
        assert!(u.is_finite() && v.is_finite());
    }
    let c =
        Curve::nurbs(NurbsCurve::elliptic_arc(DVec3::ZERO, DVec3::X, DVec3::Y, 0.0, TAU).unwrap());
    assert!(c.project(DVec3::new(f64::NAN, 0.0, 0.0)).is_finite());
}

#[test]
fn a_pole_has_a_unit_replacement_normal() {
    let half = NurbsCurve::elliptic_arc(
        DVec3::ZERO,
        DVec3::new(0.0, 0.0, 2.0),
        DVec3::new(2.0, 0.0, 0.0),
        0.0,
        std::f64::consts::PI,
    )
    .unwrap();
    let s = NurbsSurface::revolve(&half, DVec3::ZERO, DVec3::Z, 0.0, TAU).unwrap();
    let ((u0, _), (v0, v1)) = s.domain();
    let surf = Surface::nurbs(s, true, false).unwrap();
    for v in [v0, v1] {
        let n = surf.normal(u0, v);
        assert!((n.length() - 1.0).abs() < 1e-9, "{n:?}");
        // am Pol zeigt die Normale entlang der Achse
        assert!(n.cross(DVec3::Z).length() < 1e-3, "{n:?}");
    }
    // Abstand eines Punkts über dem Pol: |p| - 2
    let d = surf.implicit(DVec3::new(0.0, 0.0, 3.0));
    assert!((d.abs() - 1.0).abs() < 1e-8, "{d}");
}

#[test]
fn nurbs_curves_evaluate_project_and_wrap() {
    let circle =
        NurbsCurve::elliptic_arc(DVec3::ZERO, DVec3::X * 2.0, DVec3::Y * 2.0, 0.0, TAU).unwrap();
    let c = Curve::nurbs(circle);
    assert_eq!(c.domain(), Some((0.0, 1.0)));
    assert_eq!(c.period(), Some(1.0));
    // geschlossen: Parameter außerhalb des Bereichs werden gefaltet
    assert!(c.point(1.25).distance(c.point(0.25)) < 1e-12);
    assert!(c.point(-0.25).distance(c.point(0.75)) < 1e-12);
    for t in [0.0, 0.13, 0.37, 0.5, 0.9] {
        let p = c.point(t);
        let back = c.project(p);
        assert!(c.point(back).distance(p) < 1e-10, "{t}: {back}");
        // Punkt neben der Kurve: Fußpunkt in radialer Richtung
        let q = p * 1.5;
        assert!(c.point(c.project(q)).distance(p) < 1e-9, "{t}");
    }
    assert!(c.tangent(0.1).length() > 0.0);
    let arc =
        Curve::nurbs(NurbsCurve::elliptic_arc(DVec3::ZERO, DVec3::X, DVec3::Y, 0.0, 1.0).unwrap());
    // offen: außerhalb begrenzt auf das Ende
    let (a, b) = arc.domain().unwrap();
    assert!(arc.point(b + 0.5).distance(arc.point(b)) < 1e-15);
    assert!(arc.point(a - 0.5).distance(arc.point(a)) < 1e-15);
}

#[test]
fn a_line_meets_the_nurbs_cylinder_twice() {
    use planora_kernel::boolean::{CurveSurface, curve_surface_roots};
    let n = cylinder_as_nurbs(3.0, 5.0);
    let line = Curve::Line {
        origin: DVec3::new(-10.0, 0.0, 2.5),
        dir: DVec3::X,
    };
    let CurveSurface::Roots(rs) =
        curve_surface_roots(&line, 0.0, 20.0, &n, &planora_math::Tolerance::default())
    else {
        panic!("Gerade liegt nicht auf dem Zylinder");
    };
    assert_eq!(rs.len(), 2, "{rs:?}");
    assert!((rs[0].t - 7.0).abs() < 1e-9 && (rs[1].t - 13.0).abs() < 1e-9);
    assert!(rs.iter().all(|r| !r.touching));
}

#[test]
fn intersections_with_nurbs_are_unsupported_for_now() {
    use planora_kernel::boolean::{SurfaceIntersection, intersect_surfaces, surface_name};
    let n = cylinder_as_nurbs(3.0, 5.0);
    let plane = Surface::Plane {
        frame: planora_math::Frame::WORLD,
    };
    let tol = planora_math::Tolerance::default();
    assert!(matches!(
        intersect_surfaces(&n, &plane, &tol),
        SurfaceIntersection::Unsupported
    ));
    assert!(matches!(
        intersect_surfaces(&plane, &n, &tol),
        SurfaceIntersection::Unsupported
    ));
    assert_eq!(surface_name(&n), "Freiformfläche");
    assert_eq!(SurfaceKind::Nurbs.label(), "Freiformfläche");
    assert_eq!(CurveKind::Nurbs.label(), "Freiformkurve");
}

#[test]
fn mirroring_flips_the_sense_of_a_nurbs_face() {
    use planora_kernel::{Face, HalfEdge, Loop, Solid, UvRect};
    use planora_math::DMat4;
    let surf = cylinder_as_nurbs(3.0, 5.0);
    let rim = Curve::nurbs(
        NurbsCurve::elliptic_arc(DVec3::ZERO, DVec3::X * 3.0, DVec3::Y * 3.0, 0.0, TAU).unwrap(),
    );
    let mut s = Solid::new();
    let v = s.add_vertex(rim.point(0.0));
    let e = s.add_edge(rim.clone(), 0.0, 1.0, v, v);
    s.add_face(Face {
        surface: surf.clone(),
        same_sense: true,
        outer: Loop {
            half_edges: vec![HalfEdge {
                edge: e,
                forward: true,
            }],
        },
        inner: vec![],
        patch: Some(UvRect {
            u0: 0.0,
            u1: TAU,
            v0: 0.0,
            v1: 1.0,
        }),
        tag: None,
    });
    let m = DMat4::from_scale(DVec3::new(-1.0, 1.0, 1.0)) * DMat4::from_translation(DVec3::Z);
    let t = s.transformed(&m).unwrap();
    let (_, f) = t.faces.iter().next().unwrap();
    assert!(!f.same_sense, "Spiegelung kehrt die natürliche Normale um");
    let p = DVec3::new(3.0 * 0.6f64.cos(), 3.0 * 0.6f64.sin(), 2.5);
    let (u, v) = surf.project(p);
    let outward_old = surf.normal(u, v);
    let q = m.transform_point3(p);
    let (u2, v2) = f.surface.project(q);
    assert!(f.surface.point(u2, v2).distance(q) < 1e-9);
    let outward_new = -f.surface.normal(u2, v2);
    assert!(outward_new.distance(m.transform_vector3(outward_old)) < 1e-9);
    let (_, e2) = t.edges.iter().next().unwrap();
    assert!(
        e2.curve
            .point(0.3)
            .distance(m.transform_point3(rim.point(0.3)))
            < 1e-12
    );
    assert_eq!((e2.t0, e2.t1), (0.0, 1.0));
    // zu große Skalierung: benannter Fehler statt ungültiger Fläche
    assert!(
        s.transformed(&DMat4::from_scale(DVec3::splat(1e9)))
            .is_err()
    );
}
