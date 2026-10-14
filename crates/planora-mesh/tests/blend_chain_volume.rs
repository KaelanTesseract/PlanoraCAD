//! Kantenketten und glatte Übergänge (Teilprojekt 8, Task 9): Die vernähten Ergebnisse sind dicht
//! vernetzbar und haben das Volumen nach geschlossenen Formeln (Steiner/Pappus). Liegt hier, weil
//! der Kernel nicht vernetzen kann.

use planora_kernel::blend::topo::{edge_uses, face_name};
use planora_kernel::*;
use planora_math::{DVec2, DVec3, Frame, PI, TAU};
use planora_mesh::{Discretizer, is_watertight, mass_properties, tessellate_with};

/// Sehnentoleranz der Volumenprobe.
const CHORD: f64 = 1e-4;
/// Erlaubte Abweichung der Volumenänderung (mm³): Sehnenfehler der gekrümmten Flächen (wie in
/// `blend_build_volume.rs`).
const VOLUME_SLACK: f64 = 0.02;
/// Schranke der Dichtheitsprüfung des Netzes.
const WATERTIGHT_TOL: f64 = 1e-9;

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

fn face_named(s: &Solid, name: &str) -> Id<Face> {
    s.faces
        .iter()
        .map(|(id, _)| id)
        .find(|&id| face_name(s, id) == name)
        .unwrap_or_else(|| panic!("keine Fläche {name}"))
}

/// Stadion: zwei Geraden (Länge 10) und zwei Halbkreise (r = 2), Höhe 3.
fn stadium() -> Solid {
    let profile = Profile {
        outer: ProfileLoop {
            start: DVec2::ZERO,
            segments: vec![
                Segment::Line {
                    to: DVec2::new(10.0, 0.0),
                },
                Segment::Arc {
                    center: DVec2::new(10.0, 2.0),
                    sweep: PI,
                },
                Segment::Line {
                    to: DVec2::new(0.0, 4.0),
                },
                Segment::Arc {
                    center: DVec2::new(0.0, 2.0),
                    sweep: PI,
                },
            ],
        },
        holes: vec![],
    };
    k().extrude(&profile, &Frame::WORLD, 3.0).unwrap()
}

/// Rundes Vieleck: regelmäßiges `n`-Eck (Umkreis `r0`) um `rho` nach außen versetzt, Höhe `h`;
/// dazu der Umfang des Vielecks.
fn rounded_polygon(n: usize, r0: f64, rho: f64, h: f64) -> (Solid, f64) {
    let p = |k: usize| {
        let a = TAU * k as f64 / n as f64;
        DVec2::new(a.cos(), a.sin()) * r0
    };
    let normal = |k: usize| {
        let a = TAU * (k as f64 + 0.5) / n as f64;
        DVec2::new(a.cos(), a.sin())
    };
    let mut segments = Vec::new();
    for k in 0..n {
        segments.push(Segment::Line {
            to: p(k + 1) + normal(k) * rho,
        });
        segments.push(Segment::Arc {
            center: p(k + 1),
            sweep: TAU / n as f64,
        });
    }
    let profile = Profile {
        outer: ProfileLoop {
            start: p(0) + normal(0) * rho,
            segments,
        },
        holes: vec![],
    };
    let side = 2.0 * r0 * (PI / n as f64).sin();
    (
        k().extrude(&profile, &Frame::WORLD, h).unwrap(),
        n as f64 * side,
    )
}

fn request(edges: &[BlendEdge], kind: BlendKind, chain: bool) -> BlendRequest {
    BlendRequest {
        edges: edges.to_vec(),
        kind,
        tangent_chain: chain,
    }
}

fn plain(e: Id<Edge>) -> BlendEdge {
    BlendEdge {
        edge: e,
        first_face: None,
    }
}

/// Volumenänderung (Ergebnis − Eingang) bei gemeinsamer Teilung; das Ergebnis ist dicht.
fn delta_volume(s: &Solid, r: &Solid) -> f64 {
    validate(r).unwrap();
    let d = Discretizer::for_solids(&[s, r], CHORD);
    let (m0, m1) = (
        tessellate_with(s, &d).unwrap(),
        tessellate_with(r, &d).unwrap(),
    );
    assert!(is_watertight(&m1, WATERTIGHT_TOL), "Netz nicht dicht");
    mass_properties(&m1).volume - mass_properties(&m0).volume
}

fn assert_close(got: f64, want: f64, what: &str) {
    assert!(
        (got - want).abs() <= VOLUME_SLACK,
        "{what}: {got} statt {want}"
    );
}

/// Rundung r an einer Deckelschleife (Steiner): Querschnitt (1 − π/4) r², Schwerpunkt im
/// Abstand r (5/6 − π/4)/(1 − π/4) von der Wand.
fn top_fillet_volume(r: f64, polygon: f64, rho: f64) -> f64 {
    let area = (1.0 - PI / 4.0) * r * r;
    let offset = r * (5.0 / 6.0 - PI / 4.0) / (1.0 - PI / 4.0);
    -area * (polygon + TAU * (rho - offset))
}

#[test]
fn chains_around_a_stadium_have_the_expected_volume() {
    let s = stadium();
    let e = edge_between(&s, "Deckel", "Wand 1");
    let b = k()
        .blend(
            &s,
            &request(&[plain(e)], BlendKind::Fillet { radius: 0.5 }, true),
        )
        .unwrap();
    assert_close(
        delta_volume(&s, &b.solid),
        top_fillet_volume(0.5, 20.0, 2.0),
        "Rundung r 0,5",
    );
    // Fase 0,3 × 0,6, Fläche 1 = gerade Wand bzw. Deckel; Schwerpunkt im Abstand
    // (Maß auf dem Deckel)/3 von der Wand.
    for (first, on_top) in [("Wand 1", 0.6), ("Deckel", 0.3)] {
        let be = BlendEdge {
            edge: e,
            first_face: Some(face_named(&s, first)),
        };
        let kind = BlendKind::ChamferTwo {
            distance1: 0.3,
            distance2: 0.6,
        };
        let b = k().blend(&s, &request(&[be], kind, true)).unwrap();
        let want = -0.09 * (20.0 + TAU * (2.0 - on_top / 3.0));
        assert_close(delta_volume(&s, &b.solid), want, first);
        assert!((b.volume_change - want).abs() < 1e-9);
    }
}

#[test]
fn a_chamfer_continues_over_a_fillet_with_the_expected_volume() {
    // Würfel mit Rundung r = 2 an Deckel/Wand 1; die Fase 1 × 1 an Wand 1/Wand 2 läuft mit der
    // Tangentenkette über den Abschlussbogen der Rundung auf die Deckelkante: Geraden 8 + 8,
    // Viertelbogen mit dem Schwerpunkt im Abstand 5/3 von seiner Achse.
    let cube = k().make_box(DVec3::splat(10.0)).unwrap();
    let e = edge_between(&cube, "Deckel", "Wand 1");
    let rounded = k()
        .blend(
            &cube,
            &request(&[plain(e)], BlendKind::Fillet { radius: 2.0 }, false),
        )
        .unwrap()
        .solid;
    let e = edge_between(&rounded, "Wand 1", "Wand 2");
    let b = k()
        .blend(
            &rounded,
            &request(&[plain(e)], BlendKind::ChamferEqual { distance: 1.0 }, true),
        )
        .unwrap();
    assert_eq!(b.edges.len(), 3);
    assert_close(
        delta_volume(&rounded, &b.solid),
        -0.5 * (16.0 + 5.0 / 3.0 * PI / 2.0),
        "Fase über die Rundung",
    );
}

#[test]
fn long_chains_and_independent_edges_have_the_expected_volume() {
    let (s, polygon) = rounded_polygon(16, 10.0, 2.0, 3.0);
    let e = s
        .edges
        .iter()
        .map(|(id, _)| id)
        .find(|&id| matches!(s.edges[id].curve, Curve::Circle { .. }))
        .unwrap();
    let b = k()
        .blend(
            &s,
            &request(&[plain(e)], BlendKind::Fillet { radius: 0.5 }, true),
        )
        .unwrap();
    assert_eq!(b.edges.len(), 32);
    assert_close(
        delta_volume(&s, &b.solid),
        top_fillet_volume(0.5, polygon, 2.0),
        "rundes Vieleck",
    );
    // Vier senkrechte Würfelkanten in einem Feature: je (4 − π) · 10.
    let cube = k().make_box(DVec3::splat(10.0)).unwrap();
    let edges: Vec<BlendEdge> = [
        ("Wand 1", "Wand 2"),
        ("Wand 2", "Wand 3"),
        ("Wand 3", "Wand 4"),
        ("Wand 4", "Wand 1"),
    ]
    .iter()
    .map(|(a, c)| plain(edge_between(&cube, a, c)))
    .collect();
    let b = k()
        .blend(
            &cube,
            &request(&edges, BlendKind::Fillet { radius: 2.0 }, true),
        )
        .unwrap();
    assert_close(
        delta_volume(&cube, &b.solid),
        -4.0 * (4.0 - PI) * 10.0,
        "vier Kanten",
    );
}
