//! Volle Boolesche Operationen mit Freiformflächen (Task 10, Brief-Tests 1–7, Torusstück).
//! Hilfsfunktionen in `nurbs_common`.

mod nurbs_common;

use nurbs_common::*;
use planora_kernel::boolean::{BoolOp, BooleanError, boolean};
use planora_kernel::{Kernel, Profile, ProfileLoop, Segment, Solid, Surface, UvRect, validate};
use planora_math::nurbs::{NurbsCurve, NurbsSurface};
use planora_math::{DMat4, DVec2, DVec3, Frame, PI, TAU};
use planora_mesh::boolean_volume_plausible;

#[test]
fn box_minus_nurbs_cylinder_through_hole() {
    let r = boolean(
        &centered_box(),
        &placed_cylinder(0.0, 0.0),
        BoolOp::Difference,
        &tol(),
    )
    .unwrap();
    let rep = validate(&r).unwrap();
    // sechs Quaderflächen (Deckel und Boden mit Loch) und der NURBS-Mantel
    assert_eq!(rep.faces, 7);
    assert_eq!(count_nurbs_faces(&r), 1);
    let holed = r.faces.iter().filter(|(_, f)| f.inner.len() == 1).count();
    assert_eq!(holed, 2);
    assert_eq!(rep.genus, 1);
    let v = checked_volume(&r);
    let expect = 4000.0 - PI * 9.0 * 10.0;
    assert!(rel(v, expect) < 3e-3, "{v} statt {expect}");
}

#[test]
fn nurbs_cylinder_minus_box_slab_removes_the_middle() {
    // Der Quader umschließt z ∈ [3, 6] des Zylinders vollständig: zwei getrennte Stücke bleiben.
    // Erwartet wird dasselbe Verhalten wie beim analytischen Zwilling.
    let slab = box_solid(DVec3::new(10.0, 10.0, 3.0), DVec3::new(-5.0, -5.0, 3.0));
    let nurbs = nurbs_cylinder_solid(3.0, 10.0);
    let analytic = k().make_cylinder(3.0, 10.0).unwrap();
    let ra = boolean(&analytic, &slab, BoolOp::Difference, &tol());
    let rn = boolean(&nurbs, &slab, BoolOp::Difference, &tol());
    let expect = PI * 9.0 * (3.0 + 4.0);
    match (ra, rn) {
        (Ok(a), Ok(n)) => {
            assert_eq!(a.shells.len(), n.shells.len(), "Schalen");
            assert_eq!(n.shells.len(), 2, "zwei getrennte Stücke");
            let (va, vn) = (checked_volume(&a), checked_volume(&n));
            assert!(rel(va, expect) < 3e-3, "analytisch {va}");
            assert!(rel(vn, expect) < 3e-3, "NURBS {vn}");
            assert_eq!(validate(&a).unwrap().faces, validate(&n).unwrap().faces);
        }
        (Err(a), Err(n)) => assert_eq!(
            std::mem::discriminant(&a),
            std::mem::discriminant(&n),
            "{a:?} / {n:?}"
        ),
        (a, n) => panic!(
            "verschieden: analytisch {:?}, NURBS {:?}",
            a.map(|_| ()),
            n.map(|_| ())
        ),
    }
}

#[test]
fn union_and_intersection_follow_inclusion_exclusion() {
    // A = Quader [0, 10]³, B = NURBS-Zylinder r = 4, Achse z durch (10, 5), z ∈ [0, 10]: Deckel
    // und Boden liegen koplanar in den Ebenen des Quaders (Ebene–Ebene, schon unterstützt).
    let a = k().make_box(DVec3::splat(10.0)).unwrap();
    let b = k()
        .transform(
            &nurbs_cylinder_solid(4.0, 10.0),
            &DMat4::from_translation(DVec3::new(10.0, 5.0, 0.0)),
        )
        .unwrap();
    let u = boolean(&a, &b, BoolOp::Union, &tol()).unwrap();
    let i = boolean(&a, &b, BoolOp::Intersection, &tol()).unwrap();
    let (vu, vi) = (checked_volume(&u), checked_volume(&i));
    let (va, vb) = (1000.0, PI * 16.0 * 10.0);
    assert!(rel(vu + vi, va + vb) < 3e-3, "{vu} + {vi} vs {}", va + vb);
    // Halber Zylinder liegt im Quader
    assert!(rel(vi, vb / 2.0) < 3e-3, "{vi}");
    assert!(boolean_volume_plausible(BoolOp::Union, va, vb, vu, 3e-3));
    assert!(boolean_volume_plausible(
        BoolOp::Intersection,
        va,
        vb,
        vi,
        3e-3
    ));
    assert!(count_nurbs_faces(&u) >= 1 && count_nurbs_faces(&i) >= 1);
}

/// Profil der Spline-Extrusion (wie in `nurbs_extrude.rs`): unten eine Gerade, rechts hoch,
/// oben ein Spline zurück, links herunter.
fn spline_profile() -> (Profile, NurbsCurve) {
    let spline = NurbsCurve::natural_cubic(
        &[
            DVec3::new(10.0, 6.0, 0.0),
            DVec3::new(7.0, 8.0, 0.0),
            DVec3::new(3.0, 5.0, 0.0),
            DVec3::new(0.0, 7.0, 0.0),
        ],
        &[0.0, 1.0, 2.0, 3.0],
    )
    .unwrap();
    let lp = ProfileLoop {
        start: DVec2::new(0.0, 0.0),
        segments: vec![
            Segment::Line {
                to: DVec2::new(10.0, 0.0),
            },
            Segment::Line {
                to: DVec2::new(10.0, 6.0),
            },
            Segment::Nurbs {
                curve: std::sync::Arc::new(spline.clone()),
            },
            Segment::Line {
                to: DVec2::new(0.0, 0.0),
            },
        ],
    };
    (
        Profile {
            outer: lp,
            holes: vec![],
        },
        spline,
    )
}

/// Fläche eines Polygons links von `x = cut` (Sutherland–Hodgman an der Halbebene `x ≤ cut`).
fn area_left_of(poly: &[DVec2], cut: f64) -> f64 {
    let mut out: Vec<DVec2> = Vec::new();
    for i in 0..poly.len() {
        let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
        let (ina, inb) = (a.x <= cut, b.x <= cut);
        if ina {
            out.push(a);
        }
        if ina != inb {
            let t = (cut - a.x) / (b.x - a.x);
            out.push(a + (b - a) * t);
        }
    }
    let n = out.len();
    ((0..n)
        .map(|i| out[i].perp_dot(out[(i + 1) % n]))
        .sum::<f64>()
        / 2.0)
        .abs()
}

#[test]
fn spline_extrusion_cut_by_a_box() {
    let (profile, spline) = spline_profile();
    let h = 3.0;
    // Weltachsen: Profil-x = Welt-x (`from_origin_normal` wählt die x-Achse selbst)
    let body = k().extrude(&profile, &Frame::WORLD, h).unwrap();
    assert!(
        body.vertices
            .iter()
            .any(|(_, v)| v.point.distance(DVec3::new(10.0, 6.0, 0.0)) < 1e-9),
        "Profil in Weltlage"
    );
    // Halbraum x < 5, groß genug
    let half = box_solid(
        DVec3::new(55.0, 100.0, 100.0),
        DVec3::new(-50.0, -50.0, -50.0),
    );
    let r = boolean(&body, &half, BoolOp::Intersection, &tol()).unwrap();
    let v = checked_volume(&r);
    // Profil fein abgetastet (4000 Punkte auf dem Spline)
    let (t0, t1) = spline.domain();
    let mut poly = vec![
        DVec2::new(0.0, 0.0),
        DVec2::new(10.0, 0.0),
        DVec2::new(10.0, 6.0),
    ];
    let n = 4000;
    for i in 1..n {
        let p = spline.point(t0 + (t1 - t0) * i as f64 / n as f64);
        poly.push(DVec2::new(p.x, p.y));
    }
    poly.push(DVec2::new(0.0, 7.0));
    let expect = area_left_of(&poly, 5.0) * h;
    assert!(rel(v, expect) < 3e-3, "{v} statt {expect}");
    assert_eq!(count_nurbs_faces(&r), 1, "Spline-Wand bleibt (geschnitten)");
}

#[test]
fn ellipse_extrusion_minus_circle_extrusion() {
    let ellipse = NurbsCurve::elliptic_arc(
        DVec3::ZERO,
        DVec3::new(5.0, 0.0, 0.0),
        DVec3::new(0.0, 3.0, 0.0),
        0.0,
        TAU,
    )
    .unwrap();
    let profile = Profile {
        outer: ProfileLoop::nurbs_loop(ellipse).unwrap(),
        holes: vec![],
    };
    let body = k()
        .extrude(
            &profile,
            &Frame::from_origin_normal(DVec3::ZERO, DVec3::Z),
            4.0,
        )
        .unwrap();
    // durchgehender analytischer Zylinder r = 1,5 (berührt den Ellipsenmantel nicht)
    let drill = k()
        .transform(
            &k().make_cylinder(1.5, 8.0).unwrap(),
            &DMat4::from_translation(DVec3::new(0.0, 0.0, -2.0)),
        )
        .unwrap();
    let r = boolean(&body, &drill, BoolOp::Difference, &tol()).unwrap();
    let rep = validate(&r).unwrap();
    assert_eq!(rep.genus, 1);
    assert_eq!(count_nurbs_faces(&r), 1);
    let v = checked_volume(&r);
    let expect = PI * 15.0 * 4.0 - PI * 2.25 * 4.0;
    assert!(rel(v, expect) < 3e-3, "{v} statt {expect}");
}

/// Punkt im Zylinder um die z-Achse (r) bzw. um die x-Achse.
fn in_z_cyl(p: DVec3, r: f64) -> bool {
    p.x * p.x + p.y * p.y <= r * r
}

fn in_x_cyl(p: DVec3, r: f64) -> bool {
    p.y * p.y + p.z * p.z <= r * r
}

#[test]
fn nurbs_cylinder_minus_crossing_nurbs_cylinder_of_other_radius() {
    // A: r = 2, Achse z, z ∈ [−10, 10]; B: r = 1, Achse x, x ∈ [−10, 10] (durchbohrt A).
    let a = k()
        .transform(
            &nurbs_cylinder_solid(2.0, 20.0),
            &DMat4::from_translation(DVec3::new(0.0, 0.0, -10.0)),
        )
        .unwrap();
    let b = k()
        .transform(
            &nurbs_cylinder_solid(1.0, 20.0),
            &(DMat4::from_rotation_y(PI / 2.0)
                * DMat4::from_translation(DVec3::new(0.0, 0.0, -10.0))),
        )
        .unwrap();
    let r = boolean(&a, &b, BoolOp::Difference, &tol()).unwrap_or_else(|e| panic!("{e}"));
    let v = checked_volume(&r);
    // Voxelzählung 150³ über [−2, 2]³: entferntes Volumen = A ∩ B (B ragt nur dort in A)
    let n = 150;
    let h = 4.0 / n as f64;
    let mut removed = 0usize;
    for i in 0..n {
        for j in 0..n {
            for l in 0..n {
                let p = DVec3::new(
                    -2.0 + (i as f64 + 0.5) * h,
                    -2.0 + (j as f64 + 0.5) * h,
                    -2.0 + (l as f64 + 0.5) * h,
                );
                if in_z_cyl(p, 2.0) && in_x_cyl(p, 1.0) {
                    removed += 1;
                }
            }
        }
    }
    let removed = removed as f64 * h * h * h;
    let expect = PI * 4.0 * 20.0 - removed;
    assert!(rel(v, expect) < 0.01, "{v} statt {expect}");
    assert!(
        rel(PI * 4.0 * 20.0 - v, removed) < 0.03,
        "entfernt {} statt {removed}",
        PI * 4.0 * 20.0 - v
    );
}

#[test]
fn equal_radius_crossing_reports_a_named_error_and_keeps_the_inputs() {
    let a = k()
        .transform(
            &nurbs_cylinder_solid(2.0, 10.0),
            &DMat4::from_translation(DVec3::new(0.0, 0.0, -5.0)),
        )
        .unwrap();
    let b = k()
        .transform(
            &nurbs_cylinder_solid(2.0, 10.0),
            &(DMat4::from_rotation_y(PI / 2.0)
                * DMat4::from_translation(DVec3::new(0.0, 0.0, -5.0))),
        )
        .unwrap();
    let before = |s: &Solid| (s.faces.len(), s.edges.len(), volume(s));
    let (ba, bb) = (before(&a), before(&b));
    for op in [BoolOp::Union, BoolOp::Difference, BoolOp::Intersection] {
        match boolean(&a, &b, op, &tol()) {
            Err(e) => assert!(e.to_string().contains("Freiformfläche"), "{op:?}: {e}"),
            Ok(_) => panic!("{op:?}: Ergebnis statt Meldung"),
        }
    }
    assert_eq!(before(&a), ba);
    assert_eq!(before(&b), bb);
}

/// Scheibe mit gerundetem Rand (Kette in der (r, z)-Halbebene um die z-Achse gedreht, wie in
/// `revolve.rs`); mit `nurbs` wird das Torusstück zu einer rationalen Drehfläche eines
/// NURBS-Viertelkreises (rational in beiden Richtungen, in u geschlossen).
fn rounded_puck(nurbs: bool) -> Solid {
    let chain = ProfileLoop {
        start: DVec2::new(0.0, 0.0),
        segments: vec![
            Segment::Line {
                to: DVec2::new(3.0, 0.0),
            },
            Segment::Arc {
                center: DVec2::new(3.0, 1.0),
                sweep: PI / 2.0,
            },
            Segment::Line {
                to: DVec2::new(0.0, 1.0),
            },
        ],
    };
    let mut s = k().revolve(&chain, &Frame::WORLD).unwrap();
    if !nurbs {
        return s;
    }
    let id = s
        .faces
        .iter()
        .find(|(_, f)| matches!(f.surface, Surface::Torus { .. }))
        .map(|(id, _)| id)
        .unwrap();
    // Viertelkreis von (3, 0, 0) nach (4, 0, 1) um (3, 0, 1)
    let arc = NurbsCurve::elliptic_arc(
        DVec3::new(3.0, 0.0, 1.0),
        -DVec3::Z,
        DVec3::X,
        0.0,
        PI / 2.0,
    )
    .unwrap();
    let ns = NurbsSurface::revolve(&arc, DVec3::ZERO, DVec3::Z, 0.0, TAU).unwrap();
    let ((u0, u1), (v0, v1)) = ns.domain();
    let surface = Surface::nurbs(ns, true, false).unwrap();
    // Außenrichtung wie beim analytischen Torusstück
    let old = &s.faces[id];
    let p = DVec3::new(3.0 + 0.5_f64.sqrt(), 0.0, 1.0 - 0.5_f64.sqrt());
    let (ou, ov) = old.surface.project(p);
    let outward = old.surface.normal(ou, ov) * if old.same_sense { 1.0 } else { -1.0 };
    let (nu, nv) = surface.project(p);
    let same_sense = surface.normal(nu, nv).dot(outward) > 0.0;
    let f = &mut s.faces[id];
    f.surface = surface;
    f.same_sense = same_sense;
    f.patch = Some(UvRect { u0, u1, v0, v1 });
    s
}

/// Außenradius der gerundeten Scheibe in der Höhe `z ∈ [0, 1]`.
fn puck_radius(z: f64) -> f64 {
    3.0 + (1.0 - (1.0 - z).powi(2)).max(0.0).sqrt()
}

/// Mittelpunktregel mit `n` Stützstellen auf `[a, b]`.
fn integrate(a: f64, b: f64, n: usize, f: impl Fn(f64) -> f64) -> f64 {
    let h = (b - a) / n as f64;
    (0..n).map(|i| f(a + (i as f64 + 0.5) * h)).sum::<f64>() * h
}

#[test]
fn a_rounded_puck_with_a_nurbs_torus_patch_is_cut_by_a_box() {
    // Das analytische Torusstück kann gegen Ebenen nicht geschnitten werden
    // („Der Schnitt von Torus mit Ebene wird noch nicht unterstützt“); als Freiformfläche geht
    // es. Bezugsvolumen aus der Profilgleichung (numerische Integration).
    let (n, a) = (rounded_puck(true), rounded_puck(false));
    validate(&n).unwrap();
    let v_puck = integrate(0.0, 1.0, 4000, |z| PI * puck_radius(z).powi(2));
    let (vn, va) = (checked_volume(&n), checked_volume(&a));
    assert!(
        rel(vn, v_puck) < 3e-3 && rel(va, v_puck) < 3e-3,
        "{vn} / {va} / {v_puck}"
    );
    // Quader x ∈ [2,5; 10,5], y ∈ [−1, 1], z ∈ [−0,8; 0,6] durch den gerundeten Rand
    let boxx = box_solid(DVec3::new(8.0, 2.0, 1.4), DVec3::new(2.5, -1.0, -0.8));
    assert!(matches!(
        boolean(&a, &boxx, BoolOp::Difference, &tol()),
        Err(BooleanError::UnsupportedIntersection { .. })
    ));
    let common = integrate(0.0, 0.6, 600, |z| {
        integrate(-1.0, 1.0, 600, |y| {
            let x = (puck_radius(z).powi(2) - y * y).max(0.0).sqrt();
            (x - 2.5).max(0.0)
        })
    });
    let v_box = 8.0 * 2.0 * 1.4;
    for (op, expect) in [
        (BoolOp::Difference, v_puck - common),
        (BoolOp::Union, v_puck + v_box - common),
        (BoolOp::Intersection, common),
    ] {
        let r = boolean(&n, &boxx, op, &tol()).unwrap_or_else(|e| panic!("{op:?}: {e}"));
        let v = checked_volume(&r);
        assert!(rel(v, expect) < 3e-3, "{op:?}: {v} statt {expect}");
        assert!(count_nurbs_faces(&r) >= 1, "{op:?}");
    }
}

/// Platte `size × size × 10` minus NURBS-Bohrung r = 2, dann minus Quader 5 × 5 × 30 über
/// `[−1, 4]²`; prüft das entfernte Volumen.
fn plate_with_hole_and_cut(size: f64) {
    // Nachbesserung Task 10 (I3): Das Band um Randkanten einer Freiformfläche richtete sich nur
    // nach der Strahllänge, also nach dem ganzen Körper; bei einer Platte von 1000 mm lag jeder
    // Treffer auf der Bohrung r = 2 im Band („Streifschuss“). Jetzt zählt auch die Größe der
    // Freiformfläche.
    let hole = k()
        .transform(
            &nurbs_cylinder_solid(2.0, 30.0),
            &DMat4::from_translation(DVec3::new(0.0, 0.0, -10.0)),
        )
        .unwrap();
    let cutter = box_solid(DVec3::new(5.0, 5.0, 30.0), DVec3::new(-1.0, -1.0, -10.0));
    // Kreisscheibe r = 2 ∩ Quadrat [−1, 4]²: Fläche numerisch
    let overlap = integrate(-1.0, 2.0, 4000, |x| {
        let h = (4.0 - x * x).max(0.0).sqrt();
        (h.min(4.0) - (-h).max(-1.0)).max(0.0)
    }) * 10.0;
    let removed = PI * 4.0 * 10.0 + 250.0 - overlap;
    {
        let plate = box_solid(
            DVec3::new(size, size, 10.0),
            DVec3::new(-size / 2.0, -size / 2.0, 0.0),
        );
        let drilled = boolean(&plate, &hole, BoolOp::Difference, &tol())
            .unwrap_or_else(|e| panic!("{size}: {e}"));
        let r = boolean(&drilled, &cutter, BoolOp::Difference, &tol())
            .unwrap_or_else(|e| panic!("{size}: {e}"));
        let v = checked_volume(&r);
        let got = size * size * 10.0 - v;
        assert!(
            (got - removed).abs() <= 3e-3 * removed,
            "{size}: entfernt {got} statt {removed}"
        );
    }
}

#[test]
fn a_large_plate_with_a_small_nurbs_hole_is_cut_again() {
    plate_with_hole_and_cut(1000.0);
}

#[test]
#[ignore = "Platte 3000 mm, nur mit --release --ignored (wenige Sekunden)"]
fn a_very_large_plate_with_a_small_nurbs_hole_is_cut_again() {
    plate_with_hole_and_cut(3000.0);
}
