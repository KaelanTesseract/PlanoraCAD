//! Rundungen und Fasen an Freiformflächen (Teilprojekt 8, Task 15).

use planora_kernel::blend::topo::{edge_uses, face_name};
use planora_kernel::*;
use planora_math::nurbs::NurbsCurve;
use planora_math::{DMat4, DVec2, DVec3, Frame, PI, TAU, Tolerance};
use std::sync::Arc;

fn k() -> AnalyticKernel {
    AnalyticKernel::default()
}

fn edge_between(s: &Solid, a: &str, b: &str) -> Id<Edge> {
    s.edges
        .iter()
        .map(|(id, _)| id)
        .find(|&id| {
            let n: Vec<String> = edge_uses(s, id)
                .iter()
                .map(|u| face_name(s, u.face))
                .collect();
            n.len() == 2 && ((n[0] == a && n[1] == b) || (n[0] == b && n[1] == a))
        })
        .unwrap()
}

fn req(e: Id<Edge>, kind: BlendKind) -> BlendRequest {
    BlendRequest {
        edges: vec![BlendEdge {
            edge: e,
            first_face: None,
        }],
        kind,
        tangent_chain: false,
    }
}

/// Extrudierte Ellipse (a = b: Freiform-Zylinder).
fn ellipse_prism(a: f64, b: f64, h: f64) -> Solid {
    let c = NurbsCurve::elliptic_arc(
        DVec3::ZERO,
        DVec3::new(a, 0.0, 0.0),
        DVec3::new(0.0, b, 0.0),
        0.0,
        TAU,
    )
    .unwrap();
    k().extrude(
        &Profile {
            outer: ProfileLoop::nurbs_loop(c).unwrap(),
            holes: vec![],
        },
        &Frame::WORLD,
        h,
    )
    .unwrap()
}

#[test]
fn a_freeform_cylinder_top_edge_matches_its_analytic_twin() {
    let s = ellipse_prism(5.0, 5.0, 8.0);
    let e = edge_between(&s, "Wand 1", "Deckel");
    let b = k()
        .blend(&s, &req(e, BlendKind::Fillet { radius: 1.0 }))
        .unwrap();
    validate(&b.solid).unwrap();
    let twin = k().make_cylinder(5.0, 8.0).unwrap();
    let et = edge_between(&twin, "Wand 1", "Deckel");
    let bt = k()
        .blend(&twin, &req(et, BlendKind::Fillet { radius: 1.0 }))
        .unwrap();
    assert!(
        (b.volume_change - bt.volume_change).abs() < 2e-3 * bt.volume_change.abs(),
        "{} / {}",
        b.volume_change,
        bt.volume_change
    );
    // Die Berührkurve auf dem Deckel liegt auf dem Kreis r = 4.
    let deckel = b
        .solid
        .faces
        .iter()
        .find(|(id, _)| face_name(&b.solid, *id) == "Deckel")
        .unwrap()
        .1;
    for he in &deckel.outer.half_edges {
        let edge = &b.solid.edges[he.edge];
        for i in 0..=16 {
            let p = edge
                .curve
                .point(edge.t0 + (edge.t1 - edge.t0) * i as f64 / 16.0);
            assert!(((p.x * p.x + p.y * p.y).sqrt() - 4.0).abs() < 1e-5, "{p:?}");
        }
    }
}

#[test]
fn ellipse_fillets_and_chamfers_give_valid_bodies() {
    let s = ellipse_prism(5.0, 3.0, 4.0);
    let e = edge_between(&s, "Wand 1", "Deckel");
    for kind in [
        BlendKind::Fillet { radius: 0.5 },
        BlendKind::ChamferEqual { distance: 0.5 },
    ] {
        let b = k()
            .blend(&s, &req(e, kind.clone()))
            .unwrap_or_else(|err| panic!("{kind:?}: {err}"));
        validate(&b.solid).unwrap();
        assert!(b.volume_change < 0.0);
    }
    // Determinismus: zweimal gleich.
    let x = k()
        .blend(&s, &req(e, BlendKind::Fillet { radius: 0.5 }))
        .unwrap();
    let y = k()
        .blend(&s, &req(e, BlendKind::Fillet { radius: 0.5 }))
        .unwrap();
    assert_eq!(x.volume_change.to_bits(), y.volume_change.to_bits());
}

/// Profil mit einer Spline-Wand (Wand 2) zwischen zwei ebenen Wänden.
fn spline_prism() -> Solid {
    let spline = NurbsCurve::natural_cubic(
        &[
            DVec3::new(10.0, 0.0, 0.0),
            DVec3::new(12.0, 5.0, 0.0),
            DVec3::new(10.0, 10.0, 0.0),
        ],
        &[0.0, 1.0, 2.0],
    )
    .unwrap();
    let profile = Profile {
        outer: ProfileLoop {
            start: DVec2::ZERO,
            segments: vec![
                Segment::Line {
                    to: DVec2::new(10.0, 0.0),
                },
                Segment::Nurbs {
                    curve: Arc::new(spline),
                },
                Segment::Line {
                    to: DVec2::new(0.0, 10.0),
                },
                Segment::Line { to: DVec2::ZERO },
            ],
        },
        holes: vec![],
    };
    k().extrude(&profile, &Frame::WORLD, 5.0).unwrap()
}

#[test]
fn the_edge_between_a_spline_wall_and_a_plane_wall_rounds() {
    let s = spline_prism();
    let e = edge_between(&s, "Wand 1", "Wand 2");
    let b = k()
        .blend(&s, &req(e, BlendKind::Fillet { radius: 1.0 }))
        .unwrap();
    validate(&b.solid).unwrap();
    assert!(
        b.volume_change < 0.0 && b.volume_change > -5.0 * 1.0,
        "{}",
        b.volume_change
    );
}

#[test]
fn a_freeform_edge_with_an_oblique_end_is_named() {
    // Obere Kante der Spline-Wand: ihre Enden liegen an schrägen ebenen Wänden.
    let s = spline_prism();
    let e = edge_between(&s, "Wand 2", "Deckel");
    assert!(matches!(
        k().blend(&s, &req(e, BlendKind::Fillet { radius: 0.5 })),
        Err(BlendError::Unsupported(m)) if m == planora_kernel::blend::march::OBLIQUE_END
    ));
}

// ---- Eigene Tests (über den Brief hinaus): Lage, Krümmung, Ecken, Arbeitsgrenze ----

/// Erlaubter Abstand der Berührkurven von ihren Flächen (`NURBS_EDGE_TOL / 2`, wie die Bahn).
const CONTACT_TOL: f64 = 0.5 * planora_kernel::validate::NURBS_EDGE_TOL;
/// Erlaubte Abweichung der Rundungsfläche von der rollenden Kugel (unabhängig gerechnet).
const SKIN_CHECK: f64 = 1e-5;

/// Die neue Freiform-Rundungsfläche (Rolle `role`) und ihr Parameterbereich.
fn skin<'a>(s: &'a Solid, role: &str) -> (&'a Face, UvRect) {
    let (_, f) = s
        .faces
        .iter()
        .find(|(id, f)| matches!(f.surface, Surface::Nurbs(_)) && face_name(s, *id) == role)
        .unwrap_or_else(|| panic!("keine Freiform-Fläche {role}"));
    (f, f.patch.unwrap())
}

/// Größte Abweichung `dist(p)` über ein Raster 41 × 11 der Fläche.
fn worst_on(face: &Face, patch: UvRect, dist: impl Fn(DVec3) -> f64) -> f64 {
    let mut worst: f64 = 0.0;
    for i in 0..=40 {
        for j in 0..=10 {
            let u = patch.u0 + (patch.u1 - patch.u0) * i as f64 / 40.0;
            let v = patch.v0 + (patch.v1 - patch.v0) * j as f64 / 10.0;
            worst = worst.max(dist(face.surface.point(u, v)));
        }
    }
    worst
}

/// Jede Kante der Rundungsfläche `role` liegt an 33 Stellen auf beiden angrenzenden Flächen
/// (höchstens [`CONTACT_TOL`]); Ergebnis: die Freiform-Kanten (Berührkurven) dieser Fläche.
fn contacts_on_both_faces(s: &Solid, role: &str) -> Vec<Id<Edge>> {
    let mut out = Vec::new();
    for (id, e) in s.edges.iter() {
        let uses = edge_uses(s, id);
        if !uses.iter().any(|u| face_name(s, u.face) == role) {
            continue;
        }
        for u in &uses {
            let surface = &s.faces[u.face].surface;
            for i in 0..=32 {
                let p = e.curve.point(e.t0 + (e.t1 - e.t0) * i as f64 / 32.0);
                let d = surface.implicit(p).abs();
                assert!(d <= CONTACT_TOL, "{}: {d} bei {p}", face_name(s, u.face));
            }
        }
        if matches!(e.curve, Curve::Nurbs(_)) {
            out.push(id);
        }
    }
    out
}

/// Abstand von `p` zur ebenen Kurve `c(t)` (t periodisch in [0, 2π)) in der Höhe `z`: dichte
/// Abtastung und Goldener Schnitt um die beste Stelle.
fn distance_to_curve(p: DVec3, z: f64, c: impl Fn(f64) -> DVec2) -> f64 {
    const N: usize = 4000;
    let d = |t: f64| (c(t) - p.truncate()).length().hypot(p.z - z);
    let h = TAU / N as f64;
    let best = (0..N)
        .map(|i| i as f64 * h)
        .min_by(|a, b| d(*a).total_cmp(&d(*b)))
        .unwrap();
    let (mut lo, mut hi) = (best - h, best + h);
    let g = 0.5 * (5f64.sqrt() - 1.0);
    for _ in 0..100 {
        let (x1, x2) = (hi - g * (hi - lo), lo + g * (hi - lo));
        if d(x1) < d(x2) {
            hi = x2;
        } else {
            lo = x1;
        }
    }
    d(0.5 * (lo + hi))
}

#[test]
fn an_ellipse_fillet_lies_on_the_rolling_ball_and_its_contacts_on_both_faces() {
    // Ellipse 5 × 3, Höhe 4, Rundung r = 1,5 (kleinster Krümmungsradius 1,8): Die Kugel rollt
    // innen; ihr Mittelpunkt läuft auf der Parallelkurve im Abstand r in der Höhe h − r.
    let (a, b, h, r) = (5.0, 3.0, 4.0, 1.5);
    let s = ellipse_prism(a, b, h);
    let e = edge_between(&s, "Wand 1", "Deckel");
    let bl = k()
        .blend(&s, &req(e, BlendKind::Fillet { radius: r }))
        .unwrap();
    validate(&bl.solid).unwrap();
    let ell = |t: f64| DVec2::new(a * t.cos(), b * t.sin());
    let normal = |t: f64| DVec2::new(b * t.cos(), a * t.sin()).normalize();
    let center = |t: f64| ell(t) - normal(t) * r;
    let (face, patch) = skin(&bl.solid, "Rundung 1");
    let worst = worst_on(face, patch, |p| {
        (distance_to_curve(p, h - r, center) - r).abs()
    });
    assert!(worst < SKIN_CHECK, "Rundung: größte Abweichung {worst}");
    // Berührkurven: auf dem Deckel die Parallelkurve (z = h), auf der Wand die Ellipse bei h − r.
    let contacts = contacts_on_both_faces(&bl.solid, "Rundung 1");
    assert_eq!(contacts.len(), 2);
    for id in contacts {
        let ed = &bl.solid.edges[id];
        for i in 0..=32 {
            let p = ed.curve.point(ed.t0 + (ed.t1 - ed.t0) * i as f64 / 32.0);
            let top = (p.z - h).abs() < SKIN_CHECK
                && (distance_to_curve(p, h, ell) - r).abs() < SKIN_CHECK;
            let wall =
                (p.z - (h - r)).abs() < SKIN_CHECK && distance_to_curve(p, h - r, ell) < SKIN_CHECK;
            let inside = (p.x / a).powi(2) + (p.y / b).powi(2) <= 1.0 + 1e-9;
            assert!((top || wall) && inside, "{p}");
        }
    }
    // Fase d = 1,2: Regelfläche zwischen Wandpunkt (Ellipse bei h − d) und Deckelpunkt
    // (Parallelkurve im Abstand d bei h); in der Höhe z ist der waagerechte Abstand zur Ellipse
    // also z − (h − d).
    let d = 1.2;
    let bl = k()
        .blend(&s, &req(e, BlendKind::ChamferEqual { distance: d }))
        .unwrap();
    validate(&bl.solid).unwrap();
    let (face, patch) = skin(&bl.solid, "Fase 1");
    let worst = worst_on(face, patch, |p| {
        (distance_to_curve(p, p.z, ell) - (p.z - (h - d))).abs()
    });
    assert!(worst < SKIN_CHECK, "Fase: größte Abweichung {worst}");
    assert_eq!(contacts_on_both_faces(&bl.solid, "Fase 1").len(), 2);
}

/// NURBS-Bogen in der Skizzenebene (Mittelpunkt `c`, Halbachsen `x`, `y`, Winkel 0 … `sweep`).
fn nurbs_arc(c: DVec2, x: DVec2, y: DVec2, sweep: f64) -> Segment {
    Segment::Nurbs {
        curve: Arc::new(
            NurbsCurve::elliptic_arc(c.extend(0.0), x.extend(0.0), y.extend(0.0), 0.0, sweep)
                .unwrap(),
        ),
    }
}

#[test]
fn sphere_and_torus_fillets_lie_on_the_rolling_ball() {
    // Drehkörper: Die Skizze (x, y) dreht sich um die y-Achse in die Lage über der xy-Ebene
    // (ρ = √(x² + y²), Höhe z = Skizzen-y). Der Kugelmittelpunkt läuft auf dem Kreis
    // (ρ_c, z = r).
    let on_ring = |p: DVec3, rho_c: f64, r: f64| {
        let rho = (p.x * p.x + p.y * p.y).sqrt();
        ((rho - rho_c).hypot(p.z - r) - r).abs()
    };
    // Halbkugel R = 5, Rundung r = 1: ρ_c = √((R − r)² − r²).
    let (big, r): (f64, f64) = (5.0, 1.0);
    let sphere = k()
        .revolve(
            &ProfileLoop {
                start: DVec2::ZERO,
                segments: vec![
                    Segment::Line {
                        to: DVec2::new(big, 0.0),
                    },
                    nurbs_arc(
                        DVec2::ZERO,
                        DVec2::new(big, 0.0),
                        DVec2::new(0.0, big),
                        PI / 2.0,
                    ),
                ],
            },
            &Frame::WORLD,
        )
        .unwrap();
    let e = edge_between(&sphere, "Fläche 1", "Fläche 2");
    let b = k()
        .blend(&sphere, &req(e, BlendKind::Fillet { radius: r }))
        .unwrap();
    validate(&b.solid).unwrap();
    let rho_c = ((big - r) * (big - r) - r * r).sqrt();
    let (face, patch) = skin(&b.solid, "Rundung 1");
    let worst = worst_on(face, patch, |p| on_ring(p, rho_c, r));
    assert!(worst < SKIN_CHECK, "Kugel: {worst}");
    assert_eq!(contacts_on_both_faces(&b.solid, "Rundung 1").len(), 2);
    // Torusstück (Ring um (7, 0), Radius 2, Material außen), Rundung r = 0,5:
    // ρ_c = 7 − √((2 + r)² − r²).
    let r: f64 = 0.5;
    let torus = k()
        .revolve(
            &ProfileLoop {
                start: DVec2::ZERO,
                segments: vec![
                    Segment::Line {
                        to: DVec2::new(5.0, 0.0),
                    },
                    nurbs_arc(
                        DVec2::new(7.0, 0.0),
                        DVec2::new(-2.0, 0.0),
                        DVec2::new(0.0, 2.0),
                        PI / 2.0,
                    ),
                    Segment::Line {
                        to: DVec2::new(0.0, 2.0),
                    },
                ],
            },
            &Frame::WORLD,
        )
        .unwrap();
    let e = edge_between(&torus, "Fläche 1", "Fläche 2");
    let b = k()
        .blend(&torus, &req(e, BlendKind::Fillet { radius: r }))
        .unwrap();
    validate(&b.solid).unwrap();
    let rho_c = 7.0 - ((2.0 + r) * (2.0 + r) - r * r).sqrt();
    let (face, patch) = skin(&b.solid, "Rundung 1");
    let worst = worst_on(face, patch, |p| on_ring(p, rho_c, r));
    assert!(worst < SKIN_CHECK, "Torus: {worst}");
    assert_eq!(contacts_on_both_faces(&b.solid, "Rundung 1").len(), 2);
}

#[test]
fn an_oblique_ellipse_edge_between_analytic_faces_rounds_on_the_rolling_ball() {
    // Zylinder R = 3 schräg (30° um y) bei z = 6 abgeschnitten: Die Kante zwischen Wand und
    // schräger Ebene ist eine Ellipse ohne analytische Klasse (übrige Flächenpaare → Bahn). An
    // der Naht der Wand (x = R) läuft die Ellipse waagerecht, der Endquerschnitt liegt also auf
    // der Nahtkante. Kugelmittelpunkte: Abstand R − r von der Achse und r unter der Ebene
    // n·x = 6 cos θ mit n = (sin θ, 0, cos θ): z(φ) = (6 cos θ − r − sin θ (R − r) cos φ) / cos θ.
    let (big, r, theta) = (3.0, 0.5, PI / 6.0);
    let cut_at = |rotation: DMat4| {
        let cyl = k().make_cylinder(big, 10.0).unwrap();
        let cutter = k()
            .transform(
                &k().make_box(DVec3::splat(20.0)).unwrap(),
                &(DMat4::from_translation(DVec3::new(0.0, 0.0, 6.0))
                    * rotation
                    * DMat4::from_translation(DVec3::new(-10.0, -10.0, 0.0))),
            )
            .unwrap();
        let cut = k().boolean(&cyl, &cutter, BoolOp::Difference).unwrap();
        let e = cut
            .edges
            .iter()
            .map(|(id, _)| id)
            .find(|&id| {
                let ed = &cut.edges[id];
                !matches!(ed.curve, Curve::Circle { .. } | Curve::Line { .. })
                    && ed.curve.point(0.5 * (ed.t0 + ed.t1)).z > 3.0
            })
            .expect("schräge Randkante");
        (cut, e)
    };
    // Um x gekippt: An der Naht steigt die Ellipse, der Endquerschnitt trifft die Nahtkante nicht
    // (P5: schräger Abschluss) – benannt.
    let (cut, e) = cut_at(DMat4::from_rotation_x(theta));
    let r1 = k().blend(&cut, &req(e, BlendKind::Fillet { radius: r }));
    assert!(
        matches!(&r1, Err(BlendError::Unsupported(_))),
        "{:?}",
        r1.map(|_| ())
    );
    let (cut, e) = cut_at(DMat4::from_rotation_y(theta));
    let b = k()
        .blend(&cut, &req(e, BlendKind::Fillet { radius: r }))
        .unwrap();
    validate(&b.solid).unwrap();
    assert!(b.volume_change < 0.0);
    let center = |phi: f64| {
        let rho = big - r;
        DVec3::new(
            rho * phi.cos(),
            rho * phi.sin(),
            (6.0 * theta.cos() - r - theta.sin() * rho * phi.cos()) / theta.cos(),
        )
    };
    let (face, patch) = skin(&b.solid, "Rundung 1");
    let worst = worst_on(face, patch, |p| {
        // Abstand zur Mittelpunktskurve (dichte Abtastung, Goldener Schnitt).
        const N: usize = 4000;
        let d = |phi: f64| center(phi).distance(p);
        let h = TAU / N as f64;
        let best = (0..N)
            .map(|i| i as f64 * h)
            .min_by(|a, b| d(*a).total_cmp(&d(*b)))
            .unwrap();
        let (mut lo, mut hi) = (best - h, best + h);
        let g = 0.5 * (5f64.sqrt() - 1.0);
        for _ in 0..100 {
            let (x1, x2) = (hi - g * (hi - lo), lo + g * (hi - lo));
            if d(x1) < d(x2) {
                hi = x2;
            } else {
                lo = x1;
            }
        }
        (d(0.5 * (lo + hi)) - r).abs()
    });
    assert!(worst < SKIN_CHECK, "schräge Kante: {worst}");
    assert_eq!(contacts_on_both_faces(&b.solid, "Rundung 1").len(), 2);
    // Hautflächen verlangen das Mindestmaß für Freiformkörper, auch am analytischen Körper.
    match k().blend(&cut, &req(e, BlendKind::Fillet { radius: 1e-3 })) {
        Err(BlendError::InvalidParameter(m)) => {
            assert!(m.contains("zu klein") && m.contains("0.002"), "{m}")
        }
        other => panic!("{:?}", other.map(|_| ())),
    }
}

#[test]
fn freeform_curvature_limits_are_named() {
    // Konvexe Randkante einer Ellipse 5 × 2 (kleinster Krümmungsradius b²/a = 0,8): Die Kugel
    // liegt im Material auf der hohlen Seite der Wand; r = 1 rollt nicht, r = 0,7 schon.
    let s = ellipse_prism(5.0, 2.0, 4.0);
    let e = edge_between(&s, "Wand 1", "Deckel");
    match k().blend(&s, &req(e, BlendKind::Fillet { radius: 1.0 })) {
        Err(BlendError::CurvatureTooSmall { face }) => assert_eq!(face, "Wand 1"),
        other => panic!("{:?}", other.map(|_| ())),
    }
    let ok = k()
        .blend(&s, &req(e, BlendKind::Fillet { radius: 0.7 }))
        .unwrap();
    validate(&ok.solid).unwrap();
    // Gerade senkrechte Kante zwischen Wand 1 (y = 0) und einer hohlen Freiform-Kreiswand
    // (Mittelpunkt (8, 3), R = √13 ≈ 3,606, Material innen): Die Kante selbst ist gerade, nur die
    // Fläche ist gekrümmt. r = 3,7 > R rollt nicht (Freiform-Krümmungsprüfung); r = 1 rollt, der
    // Kugelmittelpunkt liegt auf der Senkrechten durch (8 + √((R − 1)² − 2²), 1).
    let big = 13f64.sqrt();
    let profile = Profile {
        outer: ProfileLoop {
            start: DVec2::ZERO,
            segments: vec![
                Segment::Line {
                    to: DVec2::new(10.0, 0.0),
                },
                Segment::Nurbs {
                    curve: Arc::new(
                        NurbsCurve::elliptic_arc(
                            DVec3::new(8.0, 3.0, 0.0),
                            DVec3::new(2.0, -3.0, 0.0),
                            DVec3::new(3.0, 2.0, 0.0),
                            0.0,
                            PI / 2.0,
                        )
                        .unwrap(),
                    ),
                },
                Segment::Line {
                    to: DVec2::new(0.0, 5.0),
                },
                Segment::Line { to: DVec2::ZERO },
            ],
        },
        holes: vec![],
    };
    let bowl = k().extrude(&profile, &Frame::WORLD, 3.0).unwrap();
    let e = edge_between(&bowl, "Wand 1", "Wand 2");
    match k().blend(&bowl, &req(e, BlendKind::Fillet { radius: 3.7 })) {
        Err(BlendError::CurvatureTooSmall { face }) => assert_eq!(face, "Wand 2"),
        other => panic!("{:?}", other.map(|_| ())),
    }
    let r = 1.0;
    let ok = k()
        .blend(&bowl, &req(e, BlendKind::Fillet { radius: r }))
        .unwrap();
    validate(&ok.solid).unwrap();
    let axis = DVec2::new(8.0 + ((big - r).powi(2) - (3.0 - r).powi(2)).sqrt(), r);
    let (face, patch) = skin(&ok.solid, "Rundung 1");
    let worst = worst_on(face, patch, |p| (p.truncate().distance(axis) - r).abs());
    assert!(worst < SKIN_CHECK, "Kreiswand: {worst}");
    // Konkave Bodenkante einer elliptischen Tasche 2 × 1 (Krümmungsradius 0,5): Die Kugel liegt
    // außerhalb des Materials in der Tasche, die Wand wölbt sich um sie.
    let c = NurbsCurve::elliptic_arc(
        DVec3::new(5.0, 5.0, 0.0),
        DVec3::new(2.0, 0.0, 0.0),
        DVec3::new(0.0, 1.0, 0.0),
        0.0,
        TAU,
    )
    .unwrap();
    let tool = k()
        .extrude(
            &Profile {
                outer: ProfileLoop::nurbs_loop(c).unwrap(),
                holes: vec![],
            },
            &Frame::WORLD,
            6.0,
        )
        .unwrap();
    let tool = k()
        .transform(&tool, &DMat4::from_translation(DVec3::new(0.0, 0.0, 5.0)))
        .unwrap();
    let block = k().make_box(DVec3::splat(10.0)).unwrap();
    let pocket = k().boolean(&block, &tool, BoolOp::Difference).unwrap();
    let floor = pocket
        .edges
        .iter()
        .map(|(id, _)| id)
        .find(|&id| {
            let e = &pocket.edges[id];
            matches!(e.curve, Curve::Nurbs(_)) && (e.curve.point(e.t0).z - 5.0).abs() < 1e-6
        })
        .unwrap();
    match k().blend(&pocket, &req(floor, BlendKind::Fillet { radius: 0.6 })) {
        Err(BlendError::CurvatureTooSmall { .. }) => {}
        other => panic!("{:?}", other.map(|_| ())),
    }
    let ok = k()
        .blend(&pocket, &req(floor, BlendKind::Fillet { radius: 0.3 }))
        .unwrap();
    validate(&ok.solid).unwrap();
    assert!(ok.volume_change > 0.0, "{}", ok.volume_change);
}

#[test]
fn corners_cavities_and_patch_limits_on_freeform_bodies_are_named() {
    // Ecke: Spline-Kante (Bahn) und Deckelkante (analytisch) treffen sich bei (10, 0, 5).
    let s = spline_prism();
    let r = BlendRequest {
        edges: [("Wand 1", "Wand 2"), ("Wand 1", "Deckel")]
            .iter()
            .map(|(a, b)| BlendEdge {
                edge: edge_between(&s, a, b),
                first_face: None,
            })
            .collect(),
        kind: BlendKind::Fillet { radius: 0.5 },
        tangent_chain: false,
    };
    match k().blend(&s, &r) {
        Err(BlendError::Corner(m)) => {
            assert_eq!(m, planora_kernel::blend::march::VARIABLE_CORNER)
        }
        other => panic!("{:?}", other.map(|_| ())),
    }
    // Hohlraum: Ellipsenkörper mit eingeschlossener Kugel.
    let ell = ellipse_prism(5.0, 3.0, 4.0);
    let ball = k()
        .transform(
            &k().make_sphere(0.5).unwrap(),
            &DMat4::from_translation(DVec3::new(0.0, 0.0, 2.0)),
        )
        .unwrap();
    let hollow = k().boolean(&ell, &ball, BoolOp::Difference).unwrap();
    let e = edge_between(&hollow, "Wand 1", "Deckel");
    assert!(matches!(
        k().blend(&hollow, &req(e, BlendKind::Fillet { radius: 0.5 })),
        Err(BlendError::Unsupported(m)) if m == planora_kernel::blend::build::CAVITY
    ));
    // Zu kurze Spline-Wand (2 mm) für r = 1,5: Die Kugel fände nur jenseits des Flächenstücks
    // Halt; benannt „zu groß“.
    let short = NurbsCurve::natural_cubic(
        &[
            DVec3::new(10.0, 0.0, 0.0),
            DVec3::new(10.5, 1.0, 0.0),
            DVec3::new(10.0, 2.0, 0.0),
        ],
        &[0.0, 1.0, 2.0],
    )
    .unwrap();
    let profile = Profile {
        outer: ProfileLoop {
            start: DVec2::ZERO,
            segments: vec![
                Segment::Line {
                    to: DVec2::new(10.0, 0.0),
                },
                Segment::Nurbs {
                    curve: Arc::new(short),
                },
                Segment::Line {
                    to: DVec2::new(0.0, 2.0),
                },
                Segment::Line { to: DVec2::ZERO },
            ],
        },
        holes: vec![],
    };
    let thin = k().extrude(&profile, &Frame::WORLD, 3.0).unwrap();
    let e = edge_between(&thin, "Wand 1", "Wand 2");
    let r = k().blend(&thin, &req(e, BlendKind::Fillet { radius: 1.5 }));
    assert!(
        matches!(&r, Err(BlendError::TooLarge { .. })),
        "{:?}",
        r.map(|_| ())
    );
}

#[test]
fn freeform_blends_stop_at_the_work_limit_and_report_their_cost() {
    use planora_kernel::blend::{blend_limited, march::MAX_BLEND_REQUEST_EVALUATIONS};
    use planora_math::nurbs::evaluations;
    let tol = Tolerance::default();
    let s = ellipse_prism(5.0, 3.0, 4.0);
    let e = edge_between(&s, "Wand 1", "Deckel");
    let request = req(e, BlendKind::Fillet { radius: 0.5 });
    assert!(matches!(
        blend_limited(&s, &request, &tol, 1000),
        Err(BlendError::TooExpensive)
    ));
    // Kosten typischer Fälle (Zähler je Thread, deterministisch): weit unter der Grenze.
    let spline = spline_prism();
    let cases = [
        (&s, request.clone(), "Ellipse Rundung"),
        (
            &s,
            req(e, BlendKind::ChamferEqual { distance: 0.5 }),
            "Ellipse Fase",
        ),
        (
            &spline,
            req(
                edge_between(&spline, "Wand 1", "Wand 2"),
                BlendKind::Fillet { radius: 1.0 },
            ),
            "Spline-Wand",
        ),
    ];
    for (body, r, what) in cases {
        let before = evaluations();
        let b = blend_limited(body, &r, &tol, MAX_BLEND_REQUEST_EVALUATIONS).unwrap();
        let cost = evaluations() - before;
        eprintln!("{what}: {cost} NURBS-Auswertungen");
        validate(&b.solid).unwrap();
        assert!(cost < MAX_BLEND_REQUEST_EVALUATIONS / 4, "{what}: {cost}");
        // Mit den gemessenen Kosten als Grenze dasselbe Ergebnis (die Grenze ändert nichts,
        // solange sie nicht überschritten wird).
        let again = blend_limited(body, &r, &tol, cost).unwrap();
        assert_eq!(again.volume_change.to_bits(), b.volume_change.to_bits());
    }
}
