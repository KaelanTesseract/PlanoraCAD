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

// ---- Nachbesserung 1: Ränder beschränkter Flächen, Randminima, Laufzeit ----

fn unit_square() -> Surface {
    let flat = NurbsSurface::new(
        1,
        1,
        vec![0.0, 0.0, 1.0, 1.0],
        vec![0.0, 0.0, 1.0, 1.0],
        2,
        2,
        vec![DVec3::ZERO, DVec3::Y, DVec3::X, DVec3::new(1.0, 1.0, 0.0)],
        vec![1.0; 4],
    )
    .unwrap();
    Surface::nurbs(flat, false, false).unwrap()
}

fn roots_of(line: &Curve, t1: f64, s: &Surface) -> Vec<f64> {
    use planora_kernel::boolean::{CurveSurface, curve_surface_roots};
    match curve_surface_roots(line, 0.0, t1, s, &planora_math::Tolerance::default()) {
        CurveSurface::Roots(rs) => rs.iter().map(|r| r.t).collect(),
        CurveSurface::Lies => panic!("liegt nicht auf der Fläche"),
    }
}

#[test]
fn no_false_roots_beside_a_bounded_patch() {
    // Gerade neben dem Einheitsquadrat: der Fußpunkt springt auf den Rand, das Vorzeichen
    // wechselt, aber die Gerade trifft die Fläche nicht.
    let sq = unit_square();
    let beside = Curve::Line {
        origin: DVec3::new(2.0, 0.5, -1.0),
        dir: DVec3::Z,
    };
    assert_eq!(roots_of(&beside, 2.0, &sq), Vec::<f64>::new());
    // Strahl aus dem Zylinder, der die Mantelfläche erst oberhalb ihres Endes (z = 5) träfe.
    let cyl = cylinder_as_nurbs(3.0, 5.0);
    let ray = Curve::Line {
        origin: DVec3::new(0.0, 0.0, 2.5),
        dir: DVec3::new(0.3, 0.0, 1.0).normalize(),
    };
    assert_eq!(roots_of(&ray, 40.0, &cyl), Vec::<f64>::new());
}

#[test]
fn genuine_roots_on_bounded_patches_remain() {
    let sq = unit_square();
    let through = Curve::Line {
        origin: DVec3::new(0.5, 0.25, -1.0),
        dir: DVec3::Z,
    };
    let rs = roots_of(&through, 2.0, &sq);
    assert_eq!(rs.len(), 1, "{rs:?}");
    assert!((rs[0] - 1.0).abs() < 1e-9);
    // schräg durch den Zylindermantel innerhalb der Höhe
    let cyl = cylinder_as_nurbs(3.0, 5.0);
    let dir = DVec3::new(1.0, 0.0, 0.2).normalize();
    let ray = Curve::Line {
        origin: DVec3::new(0.0, 0.0, 2.0),
        dir,
    };
    let rs = roots_of(&ray, 40.0, &cyl);
    assert_eq!(rs.len(), 1, "{rs:?}");
    assert!((ray.point(rs[0]).truncate().length() - 3.0).abs() < 1e-9);
}

/// Kleiner Zufallsgenerator mit festem Startwert (keine Abhängigkeit).
struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> f64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (self.0 >> 11) as f64 / (1u64 << 53) as f64
    }
    fn range(&mut self, a: f64, b: f64) -> f64 {
        a + (b - a) * self.next()
    }
}

fn random_patch(rng: &mut Lcg) -> NurbsSurface {
    let n = 6;
    let k = vec![0.0, 0.0, 0.0, 0.0, 1.0 / 3.0, 2.0 / 3.0, 1.0, 1.0, 1.0, 1.0];
    let mut c = Vec::new();
    let mut w = Vec::new();
    for i in 0..n {
        for j in 0..n {
            c.push(DVec3::new(
                2.0 * i as f64 + rng.range(-0.7, 0.7),
                2.0 * j as f64 + rng.range(-0.7, 0.7),
                rng.range(-3.0, 3.0),
            ));
            w.push(rng.range(0.3, 3.0));
        }
    }
    NurbsSurface::new(3, 3, k.clone(), k, n, n, c, w).unwrap()
}

#[test]
fn closest_never_misses_the_brute_force_minimum_on_random_rational_patches() {
    let mut rng = Lcg(0x5eed_1234);
    let brute_n = 80;
    let mut checked = 0;
    let mut misses: Vec<String> = Vec::new();
    for _ in 0..10 {
        let s = random_patch(&mut rng);
        let data = Surface::nurbs(s.clone(), false, false).unwrap();
        let Surface::Nurbs(d) = &data else {
            unreachable!()
        };
        let samples: Vec<DVec3> = (0..=brute_n)
            .flat_map(|i| (0..=brute_n).map(move |j| (i, j)))
            .map(|(i, j)| s.point(i as f64 / brute_n as f64, j as f64 / brute_n as f64))
            .collect();
        for _ in 0..20 {
            // Punkte im Hüllquader und bis zum Doppelten darüber hinaus
            let p = DVec3::new(
                rng.range(-6.0, 16.0),
                rng.range(-6.0, 16.0),
                rng.range(-8.0, 8.0),
            );
            let brute = samples
                .iter()
                .map(|q| q.distance(p))
                .fold(f64::INFINITY, f64::min);
            checked += 1;
            let Some((u, v, dist)) = d.closest(p, None) else {
                misses.push(format!("{p:?}: keine Konvergenz"));
                continue;
            };
            assert!((s.point(u, v).distance(p) - dist).abs() < 1e-9);
            if dist > brute + 1e-9 {
                misses.push(format!(
                    "{p:?}: gefunden {dist} bei ({u}, {v}), Probe {brute}"
                ));
                continue;
            }
            assert!((data.implicit(p).abs() - dist).abs() < 1e-9, "{p:?}");
        }
    }
    assert_eq!(checked, 200);
    assert!(
        misses.is_empty(),
        "{} Fehlgriffe:
{}",
        misses.len(),
        misses.join(
            "
"
        )
    );
}

#[test]
fn roots_on_a_large_patch_finish_quickly() {
    // 125 × 125 Kontrollpunkte (Raster am Höchstwert): vorher etwa 1,7 s je Aufruf im
    // Release-Build; die Grenze ist großzügig, auch für den Debug-Build.
    let n = 125;
    let mut k = vec![0.0; 4];
    for i in 1..n - 3 {
        k.push(i as f64 / (n - 3) as f64);
    }
    k.extend([1.0; 4]);
    let mut c = Vec::new();
    for i in 0..n {
        for j in 0..n {
            let (x, y) = (i as f64 * 10.0 / n as f64, j as f64 * 10.0 / n as f64);
            c.push(DVec3::new(x, y, (x * 1.3).sin() * (y * 0.7).cos()));
        }
    }
    let s = Surface::nurbs(
        NurbsSurface::new(3, 3, k.clone(), k, n, n, c, vec![1.0; n * n]).unwrap(),
        false,
        false,
    )
    .unwrap();
    let line = Curve::Line {
        origin: DVec3::new(-1.0, 3.3, 0.2),
        dir: DVec3::new(1.0, 0.31, 0.0).normalize(),
    };
    let start = std::time::Instant::now();
    let rs = roots_of(&line, 12.0, &s);
    let took = start.elapsed();
    assert!(took.as_secs_f64() < 20.0, "{took:?}");
    for t in rs {
        assert!(s.implicit(line.point(t)).abs() <= 1e-5, "{t}");
    }
}

/// Zylinder (r = 3, h = 5) aus dem Profil; der Mantel wird durch `mantle` ersetzt.
fn extruded_cylinder_with(
    mantle: Surface,
) -> (
    planora_kernel::Solid,
    planora_kernel::Id<planora_kernel::Face>,
) {
    use planora_kernel::{AnalyticKernel, Kernel, Profile, ProfileLoop};
    let kernel = AnalyticKernel::default();
    let profile = Profile {
        outer: ProfileLoop::circle(planora_math::DVec2::ZERO, 3.0),
        holes: vec![],
    };
    let frame = planora_math::Frame::from_origin_normal(DVec3::ZERO, DVec3::Z);
    let mut solid = kernel.extrude(&profile, &frame, 5.0).unwrap();
    let id = solid
        .faces
        .iter()
        .find(|(_, f)| matches!(f.surface, Surface::Cylinder { .. }))
        .map(|(id, _)| id)
        .unwrap();
    solid.faces[id].surface = mantle;
    (solid, id)
}

#[test]
fn surface_periods_follow_the_kind_and_the_nurbs_domain() {
    let frame = planora_math::Frame::WORLD;
    assert_eq!(Surface::Plane { frame }.u_period(), None);
    assert_eq!(Surface::Plane { frame }.v_period(), None);
    let cyl = Surface::Cylinder { frame, radius: 1.0 };
    assert_eq!((cyl.u_period(), cyl.v_period()), (Some(TAU), None));
    let torus = Surface::Torus {
        frame,
        major: 3.0,
        minor: 1.0,
    };
    assert_eq!((torus.u_period(), torus.v_period()), (Some(TAU), Some(TAU)));
    // Drehfläche: u-Bereich ist der Winkel (Breite 2π).
    let n = cylinder_as_nurbs(3.0, 5.0);
    assert_eq!(n.v_period(), None);
    assert!((n.u_period().unwrap() - TAU).abs() < 1e-12);
    // Extrudierter NURBS-Kreis: u-Bereich 0..1, also Periode 1.
    let circle =
        NurbsCurve::elliptic_arc(DVec3::ZERO, DVec3::X * 3.0, DVec3::Y * 3.0, 0.0, TAU).unwrap();
    let ext = Surface::nurbs(
        NurbsSurface::extrude(&circle, DVec3::Z * 5.0).unwrap(),
        true,
        false,
    )
    .unwrap();
    assert!((ext.u_period().unwrap() - 1.0).abs() < 1e-12);
    let open = Surface::nurbs(
        NurbsSurface::extrude(&circle, DVec3::Z * 5.0).unwrap(),
        false,
        false,
    )
    .unwrap();
    assert_eq!((open.u_period(), open.v_period()), (None, None));
}

#[test]
fn loops_on_a_closed_nurbs_surface_unwrap_with_its_own_period() {
    use planora_kernel::boolean::points_to_uv_indexed;
    // Periode 1 in u, Naht der Fläche bei (3, 0, z); die Schleife beginnt bei 80° und läuft
    // einmal herum, kreuzt also die Naht mitten in der Schleife.
    let circle =
        NurbsCurve::elliptic_arc(DVec3::ZERO, DVec3::X * 3.0, DVec3::Y * 3.0, 0.0, TAU).unwrap();
    let s = Surface::nurbs(
        NurbsSurface::extrude(&circle, DVec3::Z * 5.0).unwrap(),
        true,
        false,
    )
    .unwrap();
    let n = 90;
    let pts: Vec<DVec3> = (0..n)
        .map(|i| {
            let a = 80f64.to_radians() + TAU * i as f64 / n as f64;
            DVec3::new(3.0 * a.cos(), 3.0 * a.sin(), 2.0)
        })
        .collect();
    let uv = points_to_uv_indexed(&s, true, &pts);
    assert_eq!(uv.len(), n);
    for w in uv.windows(2) {
        let du = w[1].0.x - w[0].0.x;
        assert!(du > 0.0 && du < 0.05, "u springt: {du}");
        assert!((w[1].0.y - 0.4).abs() < 1e-9);
    }
    let span = uv[n - 1].0.x - uv[0].0.x;
    assert!(span > 0.9 && span < 1.0, "fast eine Periode: {span}");
    for (p, (q, src)) in pts.iter().zip(&uv) {
        assert_eq!(*src, Some(pts.iter().position(|x| x == p).unwrap()));
        assert!(s.point(q.x, q.y).distance(*p) < 1e-9);
    }
}

#[test]
fn validate_checks_that_edges_lie_on_their_nurbs_faces() {
    use planora_kernel::{ValidationError, validate};
    // Richtige Fläche: besteht.
    let (good, _) = extruded_cylinder_with(cylinder_as_nurbs(3.0, 5.0));
    validate(&good).unwrap();
    // Mantel mit Radius 3,1: Die Kreiskanten liegen 0,1 mm daneben.
    let (bad, id) = extruded_cylinder_with(cylinder_as_nurbs(3.1, 5.0));
    match validate(&bad) {
        Err(ValidationError::EdgeOffSurface(_, f)) => assert_eq!(f, id.index()),
        other => panic!("erwartet EdgeOffSurface, erhalten {other:?}"),
    }
    // Abweichung knapp unter NURBS_EDGE_TOL wird angenommen, deutlich darüber nicht.
    let (near, _) = extruded_cylinder_with(cylinder_as_nurbs(
        3.0 + 0.5 * planora_kernel::NURBS_EDGE_TOL,
        5.0,
    ));
    validate(&near).unwrap();
    let (off, _) = extruded_cylinder_with(cylinder_as_nurbs(
        3.0 + 3.0 * planora_kernel::NURBS_EDGE_TOL,
        5.0,
    ));
    assert!(matches!(
        validate(&off),
        Err(ValidationError::EdgeOffSurface(..))
    ));
}

#[test]
fn validate_checks_nurbs_edges_against_analytic_faces() {
    use planora_kernel::{ValidationError, validate};
    let (mut solid, _) = extruded_cylinder_with(cylinder_as_nurbs(3.0, 5.0));
    // Untere Kreiskante als NURBS-Kreis (gleiche Lage, Start bei (0, 3, 0) wie die Kante).
    let bottom = solid
        .edges
        .iter()
        .find(|(_, e)| {
            matches!(e.curve, Curve::Circle { .. }) && solid.vertices[e.start].point.z.abs() < 1e-12
        })
        .map(|(id, _)| id)
        .unwrap();
    let circle = |z: f64| {
        NurbsCurve::elliptic_arc(
            DVec3::new(0.0, 0.0, z),
            DVec3::Y * 3.0,
            DVec3::X * -3.0,
            0.0,
            TAU,
        )
        .unwrap()
    };
    let mut ok = solid.clone();
    ok.edges[bottom].curve = Curve::nurbs(circle(0.0));
    (ok.edges[bottom].t0, ok.edges[bottom].t1) = (0.0, 1.0);
    validate(&ok).unwrap();
    // Dieselbe Kurve 0,1 mm über dem Boden: liegt weder auf der Bodenebene noch auf dem Mantel.
    solid.edges[bottom].curve = Curve::nurbs(circle(0.1));
    (solid.edges[bottom].t0, solid.edges[bottom].t1) = (0.0, 1.0);
    assert!(matches!(
        validate(&solid),
        Err(ValidationError::EdgeOffSurface(..))
    ));
}
