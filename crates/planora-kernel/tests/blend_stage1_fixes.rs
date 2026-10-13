//! Nachbesserungen aus dem Review von Stufe 1 (Teilprojekt 8): glatt anschließende
//! Abschlussflächen (C1), Fasenwinkel an gekrümmter Fläche 1 (I1), Tangentenkette (I2) und
//! Mindestmaß bei Körpern mit Freiformflächen (I3).

use planora_kernel::blend::section::{ChamferSpec, Side, chamfer_section};
use planora_kernel::blend::topo::{edge_uses, face_name};
use planora_kernel::blend::{min_blend_size, min_blend_size_for};
use planora_kernel::*;
use planora_math::nurbs::NurbsCurve;
use planora_math::{DVec2, DVec3, Frame, PI, TAU, Tolerance};

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
        .unwrap_or_else(|| panic!("keine Kante zwischen {a} und {b}"))
}

/// Kante mit den Endpunkten `p0` und `p1` (in beliebiger Richtung).
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

fn face_named(s: &Solid, name: &str) -> Id<Face> {
    s.faces
        .iter()
        .map(|(id, _)| id)
        .find(|&id| face_name(s, id) == name)
        .unwrap_or_else(|| panic!("keine Fläche {name}"))
}

fn request(
    edge: Id<Edge>,
    first_face: Option<Id<Face>>,
    kind: BlendKind,
    chain: bool,
) -> BlendRequest {
    BlendRequest {
        edges: vec![BlendEdge { edge, first_face }],
        kind,
        tangent_chain: chain,
    }
}

fn blend(s: &Solid, e: Id<Edge>, kind: BlendKind, chain: bool) -> Result<Blended, BlendError> {
    k().blend(s, &request(e, None, kind, chain))
}

fn assert_smooth_end_refused(r: Result<Blended, BlendError>, what: &str) {
    match r {
        Err(BlendError::Unsupported(m)) => assert!(
            m.contains("glatt fort") && m.contains("nicht abschließen") && m.contains("„"),
            "{what}: {m}"
        ),
        other => panic!("{what}: {:?}", other.map(|b| b.solid.faces.len())),
    }
}

/// Langloch („Stadion“): gerade Seiten der Länge 5 bei y = ±2, Halbkreise r = 2, Höhe 3.
fn stadium() -> Solid {
    let profile = Profile {
        outer: ProfileLoop {
            start: DVec2::new(-2.5, -2.0),
            segments: vec![
                Segment::Line {
                    to: DVec2::new(2.5, -2.0),
                },
                Segment::Arc {
                    center: DVec2::new(2.5, 0.0),
                    sweep: PI,
                },
                Segment::Line {
                    to: DVec2::new(-2.5, 2.0),
                },
                Segment::Arc {
                    center: DVec2::new(-2.5, 0.0),
                    sweep: PI,
                },
            ],
        },
        holes: vec![],
    };
    k().extrude(&profile, &Frame::WORLD, 3.0).unwrap()
}

#[test]
fn chamfers_ending_at_a_smoothly_continuing_face_are_refused_by_name() {
    // C1 (a): Würfel mit Rundung r = 2 an Deckel/Wand 1; die Rundungsfläche setzt Wand 1 und den
    // Deckel glatt fort. Fasen an Wand 1/Wand 2 und Deckel/Wand 2 enden an ihr.
    let cube = k().make_box(DVec3::splat(10.0)).unwrap();
    let e = edge_between(&cube, "Deckel", "Wand 1");
    let rounded = blend(&cube, e, BlendKind::Fillet { radius: 2.0 }, false)
        .unwrap()
        .solid;
    let equal = BlendKind::ChamferEqual { distance: 1.0 };
    let angled = BlendKind::ChamferAngle {
        distance: 1.0,
        angle: 30f64.to_radians(),
    };
    for (a, b) in [("Wand 1", "Wand 2"), ("Deckel", "Wand 2")] {
        let e = edge_between(&rounded, a, b);
        for kind in [&equal, &angled] {
            let what = format!("{a}/{b} {kind:?}");
            assert_smooth_end_refused(blend(&rounded, e, kind.clone(), false), &what);
            // Mit Kette: die Kante läuft über den Abschlussbogen glatt weiter (Kette) – auch das
            // ist eine benannte Ablehnung, nie ein Körper.
            match blend(&rounded, e, kind.clone(), true) {
                Err(BlendError::Unsupported(m)) => assert!(
                    m.contains("Tangentenkette") || m.contains("glatt fort"),
                    "{what}: {m}"
                ),
                other => panic!("{what}: {:?}", other.map(|b| b.solid.faces.len())),
            }
        }
    }
    // C1 (b): Langloch, Kante Boden/gerade Wand: an beiden Enden setzt ein Halbzylinder die
    // gerade Wand glatt fort.
    let s = stadium();
    let e = edge_at(&s, DVec3::new(-2.5, -2.0, 0.0), DVec3::new(2.5, -2.0, 0.0));
    for kind in [
        equal.clone(),
        BlendKind::ChamferTwo {
            distance1: 2.5,
            distance2: 1.0,
        },
        BlendKind::Fillet { radius: 1.0 },
    ] {
        assert_smooth_end_refused(blend(&s, e, kind.clone(), false), &format!("{kind:?}"));
    }
}

#[test]
fn a_chamfer_at_a_transversal_curved_end_face_still_works() {
    // Viertelkreisscheibe: Bogen um (10, 0) mit r = 10 von (10, 10) nach (0, 0); er trifft die
    // Gerade y = 0 bei (0, 0) unter 90°. Fase an Deckel/gerader Wand endet dort am Zylinder.
    let s = quarter_disc();
    let e = edge_at(&s, DVec3::new(0.0, 0.0, 4.0), DVec3::new(10.0, 0.0, 4.0));
    // (Eine Rundung dort wäre Zylinder gegen Zylinder mit gekreuzten Achsen; das schneidet der
    // analytische Rechner nicht und lehnt benannt ab.)
    for kind in [
        BlendKind::ChamferEqual { distance: 1.0 },
        BlendKind::ChamferTwo {
            distance1: 1.0,
            distance2: 0.5,
        },
    ] {
        let b = blend(&s, e, kind, true).unwrap();
        validate(&b.solid).unwrap();
    }
}

/// Viertelkreisscheibe (Höhe 4): (0,0) → (10,0) → (10,10) → Bogen um (10,0) zurück nach (0,0).
pub fn quarter_disc() -> Solid {
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

/// Winkel zwischen der Fasenlinie `pa → pb` und der zur Kante `o` hin zeigenden Tangente von
/// Fläche A bei `pa` im Querschnitt (senkrecht zu `t`); dazu, ob die Linie im Querschnitt liegt
/// und ins Material (gegen die Außennormale) läuft.
fn measured_angle(a: Side, o: DVec3, t: DVec3, pa: DVec3, pb: DVec3) -> (f64, f64, f64) {
    let v = (pb - pa).normalize();
    let n = a.normal_at(pa);
    let mut tau = t.cross(n).normalize();
    if tau.dot(o - pa) < 0.0 {
        tau = -tau;
    }
    (v.dot(tau).clamp(-1.0, 1.0).acos(), v.dot(t), v.dot(n))
}

#[test]
fn the_chamfer_angle_is_measured_against_the_tangent_of_face_one() {
    // Kugel (r = 5) über der Ebene z = 0 und Torus (R = 4,24, r = 1) unter der Ebene z = 0, je mit
    // der Kante am Äquator; Fläche 1 ist die gekrümmte Fläche.
    let plane_up = Surface::Plane {
        frame: Frame::WORLD,
    };
    let sphere = Surface::Sphere {
        frame: Frame::WORLD,
        radius: 5.0,
    };
    let torus = Surface::Torus {
        frame: Frame::WORLD,
        major: 4.24,
        minor: 1.0,
    };
    let cases = [
        // (Fläche A, Fläche B mit Außenrichtung, Kantenpunkt, Richtung in A, Richtung in B)
        (
            &sphere,
            false,
            DVec3::new(5.0, 0.0, 0.0),
            DVec3::Z,
            -DVec3::X,
        ),
        (
            &torus,
            true,
            DVec3::new(5.24, 0.0, 0.0),
            -DVec3::Z,
            -DVec3::X,
        ),
    ];
    for (surface, plane_same_sense, o, wa, wb) in cases {
        let a = Side {
            surface,
            same_sense: true,
        };
        let b = Side {
            surface: &plane_up,
            same_sense: plane_same_sense,
        };
        let t = DVec3::Y;
        for (d, deg) in [(1.0, 25.0), (0.5, 45.0), (1.5, 60.0)] {
            let alpha = f64::to_radians(deg);
            let spec = ChamferSpec::DistanceAngle {
                distance: d,
                angle: alpha,
            };
            let sec = chamfer_section(a, b, o, t, true, wa, wb, spec).unwrap();
            assert!((sec.pa.distance(o) - d).abs() < 1e-9);
            assert!(b.outside(sec.pb).abs() < 1e-9);
            let (angle, along, into) = measured_angle(a, o, t, sec.pa, sec.pb);
            assert!(
                (angle - alpha).abs() < 1e-9,
                "{deg}°: {}°",
                angle.to_degrees()
            );
            assert!(along.abs() < 1e-12 && into < 0.0, "{along} {into}");
        }
    }
}

#[test]
fn the_chamfer_angle_on_a_cylinder_face_through_the_kernel() {
    // Senkrechte Kante der Viertelkreisscheibe bei (10, 10, ·) zwischen Ebene x = 10 und dem
    // Zylinder um (10, 0) mit r = 10; Fläche 1 ist der Zylinder (im Querschnitt ein Kreis).
    let s = quarter_disc();
    let e = edge_at(&s, DVec3::new(10.0, 10.0, 0.0), DVec3::new(10.0, 10.0, 4.0));
    let cyl = s
        .faces
        .iter()
        .find(|(_, f)| matches!(f.surface, Surface::Cylinder { .. }))
        .map(|(id, _)| id)
        .unwrap();
    let alpha = 30f64.to_radians();
    let kind = BlendKind::ChamferAngle {
        distance: 1.0,
        angle: alpha,
    };
    let b = k().blend(&s, &request(e, Some(cyl), kind, false)).unwrap();
    validate(&b.solid).unwrap();
    // Die neue Ebene schneidet den Deckel in der Fasenlinie: Winkel dort messen.
    let fase = face_named(&b.solid, "Fase 1");
    let Surface::Plane { frame } = b.solid.faces[fase].surface else {
        panic!("Fase ist keine Ebene");
    };
    // Berührpunkt auf dem Zylinder im Sehnenabstand 1 von der Kante (Höhe 4).
    let theta = 2.0 * (0.5f64 / 10.0).asin();
    let phi = PI / 2.0 + theta;
    let pa = DVec3::new(10.0 + 10.0 * phi.cos(), 10.0 * phi.sin(), 4.0);
    let tau = DVec3::new(phi.sin(), -phi.cos(), 0.0);
    assert!(
        frame.z.dot(pa - frame.origin).abs() < 1e-9,
        "Berührpunkt auf der Fase"
    );
    // Richtung der Fasenlinie im Querschnitt z = 4: senkrecht zu Ebenennormale und z.
    let mut v = frame.z.cross(DVec3::Z).normalize();
    if v.dot(DVec3::new(10.0, 10.0, 4.0) - pa) < 0.0 {
        v = -v;
    }
    let angle = v.dot(tau).clamp(-1.0, 1.0).acos();
    assert!((angle - alpha).abs() < 1e-9, "{}°", angle.to_degrees());
}

#[test]
fn the_tangent_chain_switch_is_not_ignored() {
    // Langloch: die Deckelkante der geraden Wand läuft glatt in die Bögen weiter.
    let s = stadium();
    let e = edge_at(&s, DVec3::new(-2.5, -2.0, 3.0), DVec3::new(2.5, -2.0, 3.0));
    for kind in [
        BlendKind::Fillet { radius: 0.5 },
        BlendKind::ChamferEqual { distance: 0.5 },
    ] {
        match blend(&s, e, kind.clone(), true) {
            Err(BlendError::Unsupported(m)) => assert!(
                m.contains("Tangentenkette") && m.contains("Deckel") && m.contains("Stufe 2"),
                "{m}"
            ),
            other => panic!("{kind:?}: {:?}", other.map(|b| b.solid.faces.len())),
        }
        // Ohne Kette: die einzelne Kante endet an glatt anschließenden Flächen (C1).
        assert_smooth_end_refused(blend(&s, e, kind.clone(), false), &format!("{kind:?}"));
    }
    // Ohne glatten Anschluss ändert der Schalter nichts.
    let cube = k().make_box(DVec3::splat(10.0)).unwrap();
    let e = edge_between(&cube, "Deckel", "Wand 1");
    for chain in [false, true] {
        let b = blend(&cube, e, BlendKind::Fillet { radius: 1.0 }, chain).unwrap();
        validate(&b.solid).unwrap();
    }
    // Ein Vollkreis (Zylinderdeckel) läuft nur in sich selbst weiter: keine Kette.
    let cyl = k().make_cylinder(5.0, 8.0).unwrap();
    let e = edge_between(&cyl, "Wand 1", "Deckel");
    blend(&cyl, e, BlendKind::Fillet { radius: 1.0 }, true).unwrap();
}

/// Würfel 10 mit einem elliptischen Durchbruch (Freiformwand) in der Mitte.
fn box_with_ellipse() -> Solid {
    let ellipse = NurbsCurve::elliptic_arc(
        DVec3::new(5.0, 5.0, 0.0),
        DVec3::new(2.0, 0.0, 0.0),
        DVec3::new(0.0, 1.0, 0.0),
        0.0,
        TAU,
    )
    .unwrap();
    let profile = Profile {
        outer: ProfileLoop::rectangle(DVec2::ZERO, DVec2::splat(10.0)),
        holes: vec![ProfileLoop::nurbs_loop(ellipse).unwrap()],
    };
    k().extrude(&profile, &Frame::WORLD, 10.0).unwrap()
}

#[test]
fn the_minimum_size_follows_the_weld_tolerance_of_the_body() {
    let tol = Tolerance::default();
    let s = box_with_ellipse();
    let min = min_blend_size_for(&s, &tol);
    assert!(
        min > min_blend_size(&tol),
        "Freiformkörper: größeres Mindestmaß"
    );
    let cube = k().make_box(DVec3::splat(10.0)).unwrap();
    assert_eq!(min_blend_size_for(&cube, &tol), min_blend_size(&tol));
    let e = edge_at(&s, DVec3::new(0.0, 0.0, 10.0), DVec3::new(10.0, 0.0, 10.0));
    for kind in [
        BlendKind::Fillet { radius: 0.9 * min },
        BlendKind::ChamferEqual {
            distance: 0.9 * min,
        },
        BlendKind::ChamferTwo {
            distance1: 1.0,
            distance2: 0.75 * min,
        },
    ] {
        match blend(&s, e, kind.clone(), false) {
            Err(BlendError::InvalidParameter(m)) => assert!(
                m.contains("Das Maß ist zu klein") && m.contains("mindestens 0.002 mm"),
                "{kind:?}: {m}"
            ),
            other => panic!("{kind:?}: {:?}", other.map(|b| b.solid.faces.len())),
        }
    }
    for kind in [
        BlendKind::Fillet { radius: 1.1 * min },
        BlendKind::ChamferEqual {
            distance: 1.1 * min,
        },
        BlendKind::ChamferTwo {
            distance1: 1.1 * min,
            distance2: 2.0 * min,
        },
        BlendKind::Fillet { radius: 1.0 },
    ] {
        let b = blend(&s, e, kind.clone(), false).unwrap_or_else(|e| panic!("{kind:?}: {e}"));
        validate(&b.solid).unwrap();
    }
    // Nicht positive Maße: verständliche Meldung ohne „war -1“.
    match blend(&s, e, BlendKind::Fillet { radius: -1.0 }, false) {
        Err(BlendError::InvalidParameter(m)) => {
            assert!(m.contains("größer als 0") && !m.contains("war"), "{m}")
        }
        other => panic!("{:?}", other.map(|_| ())),
    }
}
