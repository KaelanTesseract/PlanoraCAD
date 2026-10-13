//! Nachbesserungen aus dem Review von Stufe 1 (Teilprojekt 8): Volumen der weiterhin erlaubten
//! Fälle neben den neuen Ablehnungen (C1: Fase an quer stehender, gekrümmter Abschlussfläche und
//! neben einer Rundung; I1: Fasenwinkel an gekrümmter Fläche 1).

mod blend_common;

use blend_common::*;
use planora_kernel::*;
use planora_math::{DVec2, DVec3, Frame, PI};

const CHORD: f64 = 1e-4;
/// Erlaubter relativer Fehler der Volumenänderung gegen die Formel.
const REL: f64 = 2e-3;
/// Stützstellen der Simpson-Regel für das Vergleichsintegral (gerade).
const SIMPSON_STEPS: usize = 2000;

/// Viertelkreisscheibe (Höhe 4): (0,0) → (10,0) → (10,10) → Bogen um (10,0) zurück nach (0,0).
fn quarter_disc() -> Solid {
    let profile = Profile {
        outer: ProfileLoop {
            start: DVec2::ZERO,
            segments: vec![
                Segment::Line {
                    to: DVec2::new(10.0, 0.0),
                },
                Segment::Line {
                    to: DVec2::new(10.0, 10.0),
                },
                Segment::Arc {
                    center: DVec2::new(10.0, 0.0),
                    sweep: PI / 2.0,
                },
            ],
        },
        holes: vec![],
    };
    k().extrude(&profile, &Frame::WORLD, 4.0).unwrap()
}

fn edge_at(s: &Solid, p0: DVec3, p1: DVec3) -> Id<Edge> {
    let near = |v: Id<Vertex>, p: DVec3| s.vertices[v].point.distance(p) < 1e-9;
    s.edges
        .iter()
        .find(|(_, e)| {
            (near(e.start, p0) && near(e.end, p1)) || (near(e.start, p1) && near(e.end, p0))
        })
        .map(|(id, _)| id)
        .unwrap_or_else(|| panic!("keine Kante {p0} – {p1}"))
}

fn simpson(f: impl Fn(f64) -> f64, a: f64, b: f64) -> f64 {
    let h = (b - a) / SIMPSON_STEPS as f64;
    let mut sum = f(a) + f(b);
    for i in 1..SIMPSON_STEPS {
        let w = if i % 2 == 1 { 4.0 } else { 2.0 };
        sum += w * f(a + h * i as f64);
    }
    sum * h / 3.0
}

#[test]
fn a_chamfer_ending_at_a_transversal_cylinder_has_the_right_volume() {
    // Fase 1 × 1 an Deckel/gerader Wand (y = 0, Höhe 4); am Ende x = 0 schneidet der Zylinder um
    // (10, 0) mit r = 10 quer (90°). Abgetragen: Tiefe y, Dicke 1 − y, Länge √(100 − y²).
    let s = quarter_disc();
    let e = edge_at(&s, DVec3::new(0.0, 0.0, 4.0), DVec3::new(10.0, 0.0, 4.0));
    let b = blend_edges(&s, &[e], BlendKind::ChamferEqual { distance: 1.0 }, true).unwrap();
    let want = -simpson(|y| (1.0 - y) * (100.0 - y * y).sqrt(), 0.0, 1.0);
    assert_rel(
        delta_volume(&s, &b.solid, CHORD),
        want,
        REL,
        "Fase am Zylinder",
    );
    assert!(plausible(&s, &b, CHORD));
}

#[test]
fn chamfers_next_to_a_fillet_with_transversal_ends_have_the_right_volume() {
    // Würfel mit Rundung r = 2 an Deckel/Wand 1; danach Fasen 1 × 1 an Kanten, deren Enden an
    // quer stehenden Ebenen liegen (die Rundung grenzt an, ist aber nicht Abschlussfläche).
    let cube = k().make_box(DVec3::splat(10.0)).unwrap();
    let e = edge_between(&cube, "Deckel", "Wand 1");
    let rounded = blend_edges(&cube, &[e], BlendKind::Fillet { radius: 2.0 }, true)
        .unwrap()
        .solid;
    for (a, b) in [
        ("Boden", "Wand 1"),
        ("Wand 2", "Wand 3"),
        ("Deckel", "Wand 3"),
    ] {
        let e = edge_between(&rounded, a, b);
        let r = blend_edges(
            &rounded,
            &[e],
            BlendKind::ChamferEqual { distance: 1.0 },
            true,
        )
        .unwrap_or_else(|err| panic!("{a}/{b}: {err}"));
        assert_rel(
            delta_volume(&rounded, &r.solid, CHORD),
            -5.0,
            REL,
            &format!("{a}/{b}"),
        );
    }
}

#[test]
fn an_angle_chamfer_on_a_cylinder_face_has_the_right_volume() {
    // Senkrechte Kante bei (10, 10) zwischen Ebene x = 10 und Zylinder (um (10, 0), r = 10);
    // Fläche 1 ist der Zylinder, Abstand 1, Winkel 30° zur Zylindertangente im Berührpunkt.
    let s = quarter_disc();
    let e = edge_at(&s, DVec3::new(10.0, 10.0, 0.0), DVec3::new(10.0, 10.0, 4.0));
    let cyl = s
        .faces
        .iter()
        .find(|(_, f)| matches!(f.surface, Surface::Cylinder { .. }))
        .map(|(id, _)| id)
        .unwrap();
    let alpha = 30f64.to_radians();
    let b = k()
        .blend(
            &s,
            &BlendRequest {
                edges: vec![BlendEdge {
                    edge: e,
                    first_face: Some(cyl),
                }],
                kind: BlendKind::ChamferAngle {
                    distance: 1.0,
                    angle: alpha,
                },
                tangent_chain: true,
            },
        )
        .unwrap();
    // Unabhängige Formel im Querschnitt: Berührpunkt im Sehnenabstand 1 auf dem Kreis, Linie
    // unter 30° zur Tangente nach innen bis zur Ebene x = 10; Fläche = Dreieck + Kreisabschnitt.
    let (r, o) = (10.0, DVec2::new(10.0, 10.0));
    let theta = 2.0 * (0.5f64 / r).asin();
    let phi = PI / 2.0 + theta;
    let pa = DVec2::new(10.0 + r * phi.cos(), r * phi.sin());
    let tau = DVec2::new(phi.sin(), -phi.cos());
    let n = DVec2::new(phi.cos(), phi.sin());
    let dir = tau * alpha.cos() - n * alpha.sin();
    let pb = pa + dir * ((10.0 - pa.x) / dir.x);
    let triangle = 0.5 * ((pa - o).perp_dot(pb - o)).abs();
    let segment = 0.5 * r * r * (theta - theta.sin());
    let want = -(triangle + segment) * 4.0;
    assert_rel(
        delta_volume(&s, &b.solid, CHORD),
        want,
        REL,
        "Winkelfase am Zylinder",
    );
}
