//! Ellipsen und Splines der Skizze werden zu Freiformsegmenten und damit zu Körpern.

use planora_kernel::{AnalyticKernel, Kernel, Profile, ProfileLoop, Segment, Solid, SurfaceKind};
use planora_math::nurbs::NurbsCurve;
use planora_math::{DVec2, DVec3, Frame, PI, Tolerance};
use planora_mesh::{TessellationOptions, mass_properties, tessellate};
use planora_sketch::SketchError;
use planora_sketch::constraint::ConstraintKind as K;
use planora_sketch::model::{CurveId, Sketch};
use planora_sketch::solid::{extrude_profile, revolve_profile};
use std::sync::Arc;

fn k() -> AnalyticKernel {
    AnalyticKernel::default()
}

fn sketch() -> Sketch {
    Sketch::new(Frame::WORLD, "S", Tolerance::default())
}

/// Sehnenfehler für Volumenmessungen (wie in `e2e.rs`).
const VOLUME_CHORD_TOLERANCE: f64 = 1e-3;
/// Erlaubter relativer Volumenfehler des Netzes.
const VOLUME_REL: f64 = 2e-3;
/// Abtastschritte je Spline-Abschnitt für die Referenzintegration.
const REFERENCE_STEPS_PER_SPAN: usize = 4000;

fn volume(solid: &Solid) -> f64 {
    let options = TessellationOptions {
        chord_tolerance: VOLUME_CHORD_TOLERANCE,
    };
    mass_properties(&tessellate(solid, &options).unwrap()).volume
}

fn assert_rel(got: f64, want: f64, what: &str) {
    assert!(
        ((got - want) / want).abs() < VOLUME_REL,
        "{what}: {got} statt {want}"
    );
}

fn nurbs_faces(solid: &Solid) -> usize {
    solid
        .faces
        .iter()
        .filter(|(_, f)| f.surface.kind() == SurfaceKind::Nurbs)
        .count()
}

/// Punkte des Splines in Richtung der Kurve, fein abgetastet.
fn spline_samples(s: &Sketch, c: CurveId, spans: usize) -> Vec<DVec2> {
    let n = spans * REFERENCE_STEPS_PER_SPAN;
    (0..=n)
        .map(|i| {
            s.spline_point(c, spans as f64 * i as f64 / n as f64)
                .unwrap()
        })
        .collect()
}

#[test]
fn an_ellipse_is_one_profile_and_extrudes_to_pi_a_b_h() {
    let mut s = sketch();
    let c = s.add_point(1.0, 2.0).unwrap();
    // große Halbachse schräg: |(3, 4)| = 5, kleine Halbachse 2
    let e = s.add_ellipse(c, DVec2::new(3.0, 4.0), 2.0).unwrap();
    let ex = s.profiles();
    assert!(ex.rejected.is_empty(), "{:?}", ex.rejected);
    assert_eq!(ex.profiles.len(), 1);
    assert_eq!(ex.curve_loops, vec![vec![e]]);
    let lp = &ex.profiles[0].outer;
    assert_eq!(lp.segments.len(), 1);
    assert!(matches!(lp.segments[0], Segment::Nurbs { .. }));
    // Die Freiformkurve ist die Ellipse der Skizze.
    assert!(
        lp.start.distance(s.ellipse_point(e, 0.0).unwrap()) < 1e-9,
        "Naht am Parameter 0 der Skizze"
    );
    for p in lp.sample(64) {
        let d = p - DVec2::new(1.0, 2.0);
        let (ua, ub) = (DVec2::new(3.0, 4.0) / 5.0, DVec2::new(-4.0, 3.0) / 5.0);
        let (x, y) = (d.dot(ua) / 5.0, d.dot(ub) / 2.0);
        assert!(
            (x * x + y * y - 1.0).abs() < 1e-9,
            "Punkt {p} liegt nicht auf der Ellipse"
        );
    }
    let h = 3.0;
    let solid = extrude_profile(&k(), &s, &ex.profiles[0], h).unwrap();
    k().validate(&solid).unwrap();
    assert!(nurbs_faces(&solid) >= 1, "Mantel ist eine Freiformfläche");
    assert_rel(volume(&solid), PI * 5.0 * 2.0 * h, "Ellipsenzylinder");
}

#[test]
fn an_ellipse_with_a_rectangle_hole_and_as_a_hole() {
    // Ellipse als Loch in einem Rechteck
    let mut s = sketch();
    s.add_rectangle(DVec2::new(-10.0, -10.0), DVec2::new(10.0, 10.0))
        .unwrap();
    let c = s.add_point(0.0, 0.0).unwrap();
    s.add_ellipse(c, DVec2::new(4.0, 0.0), 2.0).unwrap();
    let ex = s.profiles();
    assert!(ex.rejected.is_empty(), "{:?}", ex.rejected);
    assert_eq!(ex.profiles.len(), 1);
    assert_eq!(ex.profiles[0].holes.len(), 1);
    let solid = extrude_profile(&k(), &s, &ex.profiles[0], 2.0).unwrap();
    k().validate(&solid).unwrap();
    assert_rel(
        volume(&solid),
        (400.0 - PI * 4.0 * 2.0) * 2.0,
        "Platte mit Ellipsenloch",
    );
}

/// Rechteck 10 × 5 mit einem Spline als Oberkante.
fn spline_top_sketch() -> (Sketch, CurveId) {
    let mut s = sketch();
    let p0 = s.add_point(0.0, 0.0).unwrap();
    let p1 = s.add_point(10.0, 0.0).unwrap();
    let p2 = s.add_point(10.0, 5.0).unwrap();
    let m1 = s.add_point(7.0, 6.5).unwrap();
    let m2 = s.add_point(3.0, 4.0).unwrap();
    let p3 = s.add_point(0.0, 5.0).unwrap();
    s.add_line(p0, p1).unwrap();
    s.add_line(p1, p2).unwrap();
    let sp = s.add_spline(&[p2, m1, m2, p3]).unwrap();
    s.add_line(p3, p0).unwrap();
    (s, sp)
}

#[test]
fn a_rectangle_with_a_spline_top_extrudes_to_the_integrated_volume() {
    let (s, sp) = spline_top_sketch();
    let ex = s.profiles();
    assert!(ex.rejected.is_empty(), "{:?}", ex.rejected);
    assert_eq!(ex.profiles.len(), 1);
    let lp = &ex.profiles[0].outer;
    assert_eq!(lp.segments.len(), 4);
    assert_eq!(
        lp.segments
            .iter()
            .filter(|g| matches!(g, Segment::Nurbs { .. }))
            .count(),
        1
    );
    // Referenzfläche: Trapezformel über den fein abgetasteten Spline (von x = 10 nach x = 0)
    // plus die drei Geraden (Gaußsche Trapezformel ∮ x dy … hier als Fläche unter der Kurve).
    let pts = spline_samples(&s, sp, 3);
    let area: f64 = pts
        .windows(2)
        .map(|w| (w[0].x - w[1].x) * 0.5 * (w[0].y + w[1].y))
        .sum();
    let h = 4.0;
    let solid = extrude_profile(&k(), &s, &ex.profiles[0], h).unwrap();
    k().validate(&solid).unwrap();
    assert!(nurbs_faces(&solid) >= 1);
    assert_rel(volume(&solid), area * h, "Spline-Extrusion");
}

#[test]
fn the_spline_curve_matches_the_sketch_parametrization() {
    // vorwärts und rückwärts durchlaufen: einmal ist der Spline die erste Kurve der Schleife,
    // einmal folgt er auf eine Linie, die ihn gegen seine Richtung erreicht
    for spline_first in [true, false] {
        let mut s = sketch();
        let a = s.add_point(0.0, 0.0).unwrap();
        let m1 = s.add_point(2.0, 3.0).unwrap();
        let m2 = s.add_point(5.0, 2.5).unwrap();
        let m3 = s.add_point(7.0, 4.0).unwrap();
        let b = s.add_point(9.0, 0.0).unwrap();
        let sp = if spline_first {
            let sp = s.add_spline(&[a, m1, m2, m3, b]).unwrap();
            s.add_line(b, a).unwrap();
            sp
        } else {
            s.add_line(a, b).unwrap();
            s.add_spline(&[a, m1, m2, m3, b]).unwrap()
        };
        let ex = s.profiles();
        assert!(ex.rejected.is_empty(), "{:?}", ex.rejected);
        let curve = ex.profiles[0]
            .outer
            .segments
            .iter()
            .find_map(|g| match g {
                Segment::Nurbs { curve } => Some(curve.clone()),
                _ => None,
            })
            .unwrap();
        assert_eq!(curve.domain(), (0.0, 4.0));
        let forward = curve.point(0.0).distance(DVec3::ZERO) < 1e-9;
        assert_eq!(forward, spline_first, "Richtung im Profil");
        for i in 0..=400 {
            let t = 4.0 * i as f64 / 400.0;
            let q = curve.point(t);
            let want = s
                .spline_point(sp, if forward { t } else { 4.0 - t })
                .unwrap();
            assert!(
                DVec2::new(q.x, q.y).distance(want) < 1e-9 && q.z.abs() < 1e-12,
                "t = {t}: {q} statt {want}"
            );
        }
    }
}

#[test]
fn a_two_point_spline_is_a_straight_freeform_segment() {
    let mut s = sketch();
    let a = s.add_point(0.0, 0.0).unwrap();
    let b = s.add_point(4.0, 0.0).unwrap();
    let c = s.add_point(0.0, 3.0).unwrap();
    s.add_line(a, b).unwrap();
    s.add_spline(&[b, c]).unwrap();
    s.add_line(c, a).unwrap();
    let ex = s.profiles();
    assert!(ex.rejected.is_empty(), "{:?}", ex.rejected);
    let solid = extrude_profile(&k(), &s, &ex.profiles[0], 2.0).unwrap();
    k().validate(&solid).unwrap();
    assert_rel(volume(&solid), 6.0 * 2.0, "Dreieck");
}

#[test]
fn a_spline_closed_on_itself_is_a_loop() {
    let mut s = sketch();
    let p: Vec<_> = [(0.0, 0.0), (4.0, -1.0), (6.0, 3.0), (2.0, 5.0), (0.0, 0.0)]
        .iter()
        .map(|&(x, y)| s.add_point(x, y).unwrap())
        .collect();
    let sp = s.add_spline(&p).unwrap();
    let ex = s.profiles();
    assert!(ex.rejected.is_empty(), "{:?}", ex.rejected);
    assert_eq!(ex.profiles.len(), 1);
    let pts = spline_samples(&s, sp, 4);
    let area: f64 = pts.windows(2).map(|w| w[0].perp_dot(w[1])).sum::<f64>() / 2.0;
    let solid = extrude_profile(&k(), &s, &ex.profiles[0], 1.5).unwrap();
    k().validate(&solid).unwrap();
    assert_rel(volume(&solid), area.abs() * 1.5, "geschlossener Spline");
}

#[test]
fn a_spline_crossing_the_profile_is_rejected_as_self_intersecting() {
    // Der Spline taucht unter die Grundlinie und schneidet sie.
    let mut s = sketch();
    let p0 = s.add_point(0.0, 0.0).unwrap();
    let p1 = s.add_point(10.0, 0.0).unwrap();
    let p2 = s.add_point(10.0, 5.0).unwrap();
    let m = s.add_point(5.0, -3.0).unwrap();
    let p3 = s.add_point(0.0, 5.0).unwrap();
    s.add_line(p0, p1).unwrap();
    s.add_line(p1, p2).unwrap();
    s.add_spline(&[p2, m, p3]).unwrap();
    s.add_line(p3, p0).unwrap();
    let ex = s.profiles();
    assert!(ex.profiles.is_empty());
    assert_eq!(ex.rejected.len(), 1);
    assert_eq!(ex.rejected[0].reason, SketchError::SelfIntersecting);

    // Ein Spline mit Schlaufe schneidet sich selbst.
    let mut s = sketch();
    let p: Vec<_> = [(0.0, 0.0), (6.0, 4.0), (6.0, 1.0), (2.0, 4.0), (8.0, 0.0)]
        .iter()
        .map(|&(x, y)| s.add_point(x, y).unwrap())
        .collect();
    s.add_spline(&p).unwrap();
    s.add_line(p[4], p[0]).unwrap();
    let ex = s.profiles();
    assert!(ex.profiles.is_empty());
    assert_eq!(ex.rejected[0].reason, SketchError::SelfIntersecting);

    // Ein Spline, der eine Ellipse daneben schneidet: beide abgelehnt.
    let (mut s, _) = spline_top_sketch();
    let c = s.add_point(5.0, 6.0).unwrap();
    s.add_ellipse(c, DVec2::new(3.0, 0.0), 1.0).unwrap();
    let ex = s.profiles();
    assert!(ex.profiles.is_empty());
    assert_eq!(ex.rejected.len(), 2);
    assert!(
        ex.rejected
            .iter()
            .all(|r| r.reason == SketchError::SelfIntersecting)
    );
}

#[test]
fn spline_points_merged_by_the_solver_give_a_german_error() {
    let mut s = sketch();
    let p0 = s.add_point(0.0, 0.0).unwrap();
    let p1 = s.add_point(10.0, 0.0).unwrap();
    let p2 = s.add_point(10.0, 5.0).unwrap();
    let m = s.add_point(5.0, 6.0).unwrap();
    let p3 = s.add_point(0.0, 5.0).unwrap();
    s.add_line(p0, p1).unwrap();
    s.add_line(p1, p2).unwrap();
    let sp = s.add_spline(&[p2, m, p3]).unwrap();
    s.add_line(p3, p0).unwrap();
    s.add_constraint(K::Coincident(m, p3)).unwrap();
    let _ = s.solve();
    let ex = s.profiles();
    assert!(ex.profiles.is_empty());
    let r = ex.rejected.iter().find(|r| r.curves.contains(&sp)).unwrap();
    assert!(
        matches!(&r.reason, SketchError::Degenerate(m) if m.contains("Splines")),
        "{:?}",
        r.reason
    );
}

/// Halbe Ellipse von `(0, −b)` über `(±a, 0)` nach `(0, b)` und zurück auf der Achse `x = 0`.
fn half_ellipse_profile(a: f64, b: f64, side: f64, ccw: bool) -> Profile {
    let arc = NurbsCurve::elliptic_arc(
        DVec3::ZERO,
        DVec3::new(0.0, -b, 0.0),
        DVec3::new(side * a, 0.0, 0.0),
        0.0,
        PI,
    )
    .unwrap();
    let lp = ProfileLoop {
        start: DVec2::new(0.0, -b),
        segments: vec![
            Segment::Nurbs {
                curve: Arc::new(arc),
            },
            Segment::Line {
                to: DVec2::new(0.0, -b),
            },
        ],
    };
    Profile {
        outer: if ccw == (side > 0.0) {
            lp
        } else {
            lp.try_reversed().unwrap()
        },
        holes: vec![],
    }
}

#[test]
fn a_half_ellipse_revolves_to_an_ellipsoid_on_both_sides_of_the_axis() {
    let (a, b) = (3.0, 5.0);
    let want = 4.0 / 3.0 * PI * a * a * b;
    for side in [1.0, -1.0] {
        for ccw in [true, false] {
            let mut s = sketch();
            let p = s.add_point(0.0, -8.0).unwrap();
            let q = s.add_point(0.0, 8.0).unwrap();
            let axis = s.add_line(p, q).unwrap();
            s.set_construction(axis, true).unwrap();
            let profile = half_ellipse_profile(a, b, side, ccw);
            let solid = revolve_profile(&k(), &s, &profile, axis).unwrap();
            k().validate(&solid).unwrap();
            assert!(nurbs_faces(&solid) >= 1);
            assert_rel(
                volume(&solid),
                want,
                &format!("Ellipsoid (Seite {side}, ccw {ccw})"),
            );
        }
    }
}

#[test]
fn a_spline_profile_revolves_to_the_integrated_volume() {
    // Achse x = 0; Profil: Achsstück, Deckel, Spline als Außenkontur, Boden.
    let mut s = sketch();
    let ap = s.add_point(0.0, -2.0).unwrap();
    let aq = s.add_point(0.0, 8.0).unwrap();
    let axis = s.add_line(ap, aq).unwrap();
    s.set_construction(axis, true).unwrap();
    let o = s.add_point(0.0, 0.0).unwrap();
    let t = s.add_point(0.0, 5.0).unwrap();
    let r1 = s.add_point(2.0, 5.0).unwrap();
    let m1 = s.add_point(3.0, 3.0).unwrap();
    let m2 = s.add_point(2.5, 1.0).unwrap();
    let r0 = s.add_point(2.0, 0.0).unwrap();
    s.add_line(o, t).unwrap();
    s.add_line(t, r1).unwrap();
    let sp = s.add_spline(&[r1, m1, m2, r0]).unwrap();
    s.add_line(r0, o).unwrap();
    let ex = s.profiles();
    assert!(ex.rejected.is_empty(), "{:?}", ex.rejected);
    // V = π ∫ x² dy längs des Splines (Linien auf der Achse oder waagerecht tragen nichts bei).
    let pts = spline_samples(&s, sp, 3);
    let want: f64 = PI
        * pts
            .windows(2)
            .map(|w| {
                let (x0, x1) = (w[0].x, w[1].x);
                (x0 * x0 + x0 * x1 + x1 * x1) / 3.0 * (w[0].y - w[1].y)
            })
            .sum::<f64>();
    let solid = revolve_profile(&k(), &s, &ex.profiles[0], axis).unwrap();
    k().validate(&solid).unwrap();
    assert!(nurbs_faces(&solid) >= 1);
    assert_rel(volume(&solid), want, "Spline-Drehkörper");

    // gespiegelt auf die andere Seite der Achse: gleiches Volumen
    let mut m = sketch();
    let ap = m.add_point(0.0, -2.0).unwrap();
    let aq = m.add_point(0.0, 8.0).unwrap();
    let axis = m.add_line(ap, aq).unwrap();
    m.set_construction(axis, true).unwrap();
    let pts: Vec<_> = [
        (0.0, 0.0),
        (0.0, 5.0),
        (-2.0, 5.0),
        (-3.0, 3.0),
        (-2.5, 1.0),
        (-2.0, 0.0),
    ]
    .iter()
    .map(|&(x, y)| m.add_point(x, y).unwrap())
    .collect();
    m.add_line(pts[0], pts[1]).unwrap();
    m.add_line(pts[1], pts[2]).unwrap();
    m.add_spline(&pts[2..6]).unwrap();
    m.add_line(pts[5], pts[0]).unwrap();
    let solid = revolve_profile(&k(), &m, &m.profiles().profiles[0], axis).unwrap();
    k().validate(&solid).unwrap();
    assert_rel(volume(&solid), want, "gespiegelter Spline-Drehkörper");
}
