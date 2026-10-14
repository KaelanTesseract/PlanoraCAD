//! Ecken der Rundung (Teilprojekt 8, Task 11): Gehrung und Kugelstück sind dicht vernetzbar und
//! haben das Volumen nach geschlossenen Formeln. Liegt hier, weil der Kernel nicht vernetzen kann.
//!
//! Formeln (Querschnitt `A`, Moment `M = ∫ y dA` des Querschnitts um eine der beiden Kantenflächen,
//! Rundung: `A = (1 − π/4) r²`, `M = (5/6 − π/4) r³`; Fase gleich d: `A = d²/2`, `M = d³/6`):
//! - Gehrung an einer Ecke mit Innenwinkel θ (Wände senkrecht zur gemeinsamen Fläche): Das Prisma
//!   einer Kante endet an der Winkelhalbierenden; im Abstand `y` von der Wand ist es um
//!   `y · cot(θ/2)` kürzer, zusammen `A · L − M · cot(θ/2)` je Kantenende.
//! - Kugelecke (rechtwinklig): jede Kante bis zur Ebene durch den Kugelmittelpunkt (`L − r` je
//!   Ecke), dazu der Eckwürfel r³ minus Kugelachtel π r³ / 6.

use planora_kernel::*;
use planora_math::{DMat4, DVec2, DVec3, Frame, PI};
use planora_mesh::{BLEND_REL, Discretizer, is_watertight, mass_properties, tessellate_with};

/// Sehnentoleranz der Volumenprobe.
const CHORD: f64 = 1e-4;
/// Erlaubte Abweichung der Volumenänderung (mm³): Sehnenfehler der gekrümmten Flächen (wie in
/// `blend_chain_volume.rs`).
const VOLUME_SLACK: f64 = 0.02;
/// Schranke der Dichtheitsprüfung des Netzes.
const WATERTIGHT_TOL: f64 = 1e-9;

fn k() -> AnalyticKernel {
    AnalyticKernel::default()
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

fn blend(s: &Solid, edges: &[Id<Edge>], kind: BlendKind) -> Blended {
    k().blend(
        s,
        &BlendRequest {
            edges: edges
                .iter()
                .map(|&edge| BlendEdge {
                    edge,
                    first_face: None,
                })
                .collect(),
            kind,
            tangent_chain: false,
        },
    )
    .unwrap()
}

/// Volumenänderung (Ergebnis − Eingang) bei gemeinsamer Teilung; das Ergebnis ist gültig und
/// sein Netz dicht.
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

/// Vernetzte Volumenänderung gegen die Formel; die Schätzung des Kernels innerhalb der
/// Plausibilitätsschranke des Features (P2).
fn check(s: &Solid, b: &Blended, want: f64, what: &str) {
    let got = delta_volume(s, &b.solid);
    assert!(
        (got - want).abs() <= VOLUME_SLACK,
        "{what}: {got} statt {want}"
    );
    assert!(
        (b.volume_change - want).abs() <= BLEND_REL * want.abs(),
        "{what}: Schätzung {} statt {want}",
        b.volume_change
    );
}

/// (A, M) der Rundung r.
fn fillet(r: f64) -> (f64, f64) {
    ((1.0 - PI / 4.0) * r * r, (5.0 / 6.0 - PI / 4.0) * r * r * r)
}

/// (A, M) der gleichen Fase d.
fn chamfer(d: f64) -> (f64, f64) {
    (d * d / 2.0, d * d * d / 6.0)
}

/// Eckwürfel minus Kugelachtel.
fn ball_corner(r: f64) -> f64 {
    (1.0 - PI / 6.0) * r * r * r
}

#[test]
fn two_edges_at_a_box_corner_meet_in_a_miter() {
    let cube = k().make_box(DVec3::splat(10.0)).unwrap();
    let v = DVec3::new(10.0, 0.0, 10.0);
    let pair = [
        edge_at(&cube, DVec3::new(0.0, 0.0, 10.0), v),
        edge_at(&cube, v, DVec3::new(10.0, 10.0, 10.0)),
    ];
    for r in [1.0, 2.5] {
        let (a, m) = fillet(r);
        let b = blend(&cube, &pair, BlendKind::Fillet { radius: r });
        check(&cube, &b, -2.0 * (a * 10.0 - m), &format!("Gehrung r {r}"));
    }
    let (a, m) = chamfer(1.5);
    let b = blend(&cube, &pair, BlendKind::ChamferEqual { distance: 1.5 });
    check(&cube, &b, -2.0 * (a * 10.0 - m), "Gehrung Fase 1,5");
    // Fase 1 × 2 mit Fläche 1 = Deckel an beiden Kanten (spiegelgleich): Dreieck mit dem Schenkel
    // t auf dem Deckel und w an der Wand, A = t·w/2, M = t²·w/6 (Abstand von der Wand).
    let top = cube
        .faces
        .iter()
        .map(|(id, _)| id)
        .find(|&id| planora_kernel::blend::topo::face_name(&cube, id) == "Deckel")
        .unwrap();
    let (t, w) = (1.0, 2.0);
    let b = k()
        .blend(
            &cube,
            &BlendRequest {
                edges: pair
                    .iter()
                    .map(|&edge| BlendEdge {
                        edge,
                        first_face: Some(top),
                    })
                    .collect(),
                kind: BlendKind::ChamferTwo {
                    distance1: t,
                    distance2: w,
                },
                tangent_chain: false,
            },
        )
        .unwrap();
    check(
        &cube,
        &b,
        -2.0 * (t * w / 2.0 * 10.0 - t * t * w / 6.0),
        "Gehrung Fase 1 × 2",
    );
}

#[test]
fn three_edges_at_a_box_corner_get_a_sphere_patch() {
    let cube = k().make_box(DVec3::splat(10.0)).unwrap();
    let v = DVec3::new(0.0, 0.0, 10.0);
    let three = [
        edge_at(&cube, v, DVec3::new(10.0, 0.0, 10.0)),
        edge_at(&cube, v, DVec3::new(0.0, 10.0, 10.0)),
        edge_at(&cube, v, DVec3::ZERO),
    ];
    for r in [1.0, 3.0] {
        let (a, _) = fillet(r);
        let b = blend(&cube, &three, BlendKind::Fillet { radius: r });
        let want = -(3.0 * a * (10.0 - r) + ball_corner(r));
        check(&cube, &b, want, &format!("Kugelecke r {r}"));
    }
}

#[test]
fn all_twelve_edges_of_a_box_round_with_eight_sphere_patches() {
    let s = k().make_box(DVec3::new(10.0, 8.0, 6.0)).unwrap();
    let all: Vec<Id<Edge>> = s.edges.iter().map(|(id, _)| id).collect();
    let r = 1.0;
    let (a, _) = fillet(r);
    let b = blend(&s, &all, BlendKind::Fillet { radius: r });
    let lengths = 4.0 * ((10.0 - 2.0 * r) + (8.0 - 2.0 * r) + (6.0 - 2.0 * r));
    check(
        &s,
        &b,
        -(a * lengths + 8.0 * ball_corner(r)),
        "zwölf Kanten",
    );
}

#[test]
fn oblique_corners_of_a_triangle_prism_miter() {
    let pts = [
        DVec2::new(0.0, 0.0),
        DVec2::new(10.0, 0.0),
        DVec2::new(5.0, 8.0),
    ];
    let s = k()
        .extrude(
            &Profile {
                outer: ProfileLoop::polygon(&pts).unwrap(),
                holes: vec![],
            },
            &Frame::WORLD,
            5.0,
        )
        .unwrap();
    let top = |p: DVec2| DVec3::new(p.x, p.y, 5.0);
    let edges: Vec<Id<Edge>> = (0..3)
        .map(|i| edge_at(&s, top(pts[i]), top(pts[(i + 1) % 3])))
        .collect();
    let perimeter: f64 = (0..3).map(|i| pts[i].distance(pts[(i + 1) % 3])).sum();
    // Σ cot(θ/2) über die Innenwinkel des Dreiecks.
    let cot_sum: f64 = (0..3)
        .map(|i| {
            let (p, a, b) = (pts[i], pts[(i + 1) % 3], pts[(i + 2) % 3]);
            let theta = (a - p).angle_to(b - p).abs();
            1.0 / (theta / 2.0).tan()
        })
        .sum();
    for (kind, (a, m)) in [
        (BlendKind::Fillet { radius: 1.0 }, fillet(1.0)),
        (BlendKind::ChamferEqual { distance: 0.8 }, chamfer(0.8)),
    ] {
        let b = blend(&s, &edges, kind.clone());
        check(
            &s,
            &b,
            -(a * perimeter - 2.0 * m * cot_sum),
            &format!("{kind:?}"),
        );
    }
}

#[test]
fn concave_edges_inside_a_pocket_miter_and_add_material() {
    let cube = k().make_box(DVec3::splat(10.0)).unwrap();
    let tool = k()
        .transform(
            &k().make_box(DVec3::splat(6.0)).unwrap(),
            &DMat4::from_translation(DVec3::new(2.0, 2.0, 5.0)),
        )
        .unwrap();
    let s = k().boolean(&cube, &tool, BoolOp::Difference).unwrap();
    let v = DVec3::new(8.0, 2.0, 5.0);
    let pair = [
        edge_at(&s, DVec3::new(2.0, 2.0, 5.0), v),
        edge_at(&s, v, DVec3::new(8.0, 8.0, 5.0)),
    ];
    for (kind, (a, m)) in [
        (BlendKind::Fillet { radius: 1.0 }, fillet(1.0)),
        (BlendKind::ChamferEqual { distance: 1.0 }, chamfer(1.0)),
    ] {
        // Bodenkanten der Länge 6: an (2, 2) bzw. (8, 8) Abschluss an der Taschenwand, an (8, 2)
        // die Gehrung.
        let b = blend(&s, &pair, kind.clone());
        check(&s, &b, 2.0 * (a * 6.0 - m), &format!("Tasche {kind:?}"));
    }
}

/// Zufallszahlen mit festem Startwert (wie in `planora-kernel/tests/nurbs_geometry.rs`).
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

/// Zufällige Quader mit zufälligen Teilmengen ihrer Kanten. Formel für jede Teilmenge: jede
/// gewählte Kante `A · L`; je Ecke mit zwei gewählten Kanten (Gehrung) `− 2 M`, mit dreien
/// (Kugelecke) `− 3 A r + (1 − π/6) r³`. Rundungen müssen immer gelingen (r unter der halben
/// kürzesten Kante), Fasen genau dann nicht, wenn eine Ecke drei gewählte Kanten hat.
#[test]
fn random_edge_subsets_of_boxes_have_the_formula_volume() {
    // Debug-Laufzeit etwa 15 s.
    random_edge_subsets(10, 0x5EED_0011);
}

#[test]
#[ignore = "lange Reihe; nur mit --release --ignored (etwa 25 s)"]
fn many_random_edge_subsets_of_boxes_have_the_formula_volume() {
    random_edge_subsets(300, 0x5EED_0012);
}

fn random_edge_subsets(cases: usize, seed: u64) {
    let mut rng = Lcg(seed);
    let mut counts = [0usize; 3]; // Gehrungen, Kugelecken, abgelehnte Fasen
    for case in 0..cases {
        let size = DVec3::new(
            rng.range(3.0, 12.0),
            rng.range(3.0, 12.0),
            rng.range(3.0, 12.0),
        );
        let s = k().make_box(size).unwrap();
        let all: Vec<Id<Edge>> = s.edges.iter().map(|(id, _)| id).collect();
        let mask = 1 + (rng.next() * 4095.0) as u32;
        let picked: Vec<Id<Edge>> = all
            .iter()
            .enumerate()
            .filter(|(i, _)| mask & (1 << i) != 0)
            .map(|(_, &e)| e)
            .collect();
        let d = rng.range(0.2, size.min_element() / 2.2);
        let is_fillet = case % 2 == 0;
        let ((a, m), kind) = if is_fillet {
            (fillet(d), BlendKind::Fillet { radius: d })
        } else {
            (chamfer(d), BlendKind::ChamferEqual { distance: d })
        };
        let mut per_vertex: std::collections::BTreeMap<Id<Vertex>, usize> = Default::default();
        let mut want = 0.0;
        for &e in &picked {
            let edge = &s.edges[e];
            let len = s.vertices[edge.start]
                .point
                .distance(s.vertices[edge.end].point);
            want -= a * len;
            *per_vertex.entry(edge.start).or_default() += 1;
            *per_vertex.entry(edge.end).or_default() += 1;
        }
        let (mut miters, mut balls) = (0, 0);
        for &n in per_vertex.values() {
            match n {
                2 => {
                    want += 2.0 * m;
                    miters += 1;
                }
                3 => {
                    want += 3.0 * a * d - ball_corner(d);
                    balls += 1;
                }
                _ => {}
            }
        }
        let what = format!("Fall {case}: Quader {size}, Maske {mask:#05x}, {kind:?}");
        let result = k().blend(
            &s,
            &BlendRequest {
                edges: picked
                    .iter()
                    .map(|&edge| BlendEdge {
                        edge,
                        first_face: None,
                    })
                    .collect(),
                kind: kind.clone(),
                tangent_chain: false,
            },
        );
        match result {
            Err(BlendError::Corner(msg)) if !is_fillet && balls > 0 => {
                assert!(msg.contains("Fase"), "{what}: {msg}");
                counts[2] += 1;
            }
            Ok(b) if is_fillet || balls == 0 => {
                assert_eq!(
                    b.solid.faces.len(),
                    6 + picked.len() + if is_fillet { balls } else { 0 },
                    "{what}"
                );
                check(&s, &b, want, &what);
                counts[0] += miters;
                counts[1] += balls;
            }
            other => panic!("{what}: {:?}", other.map(|b| b.solid.faces.len())),
        }
    }
    // Abdeckung: alle drei Arten kommen vor.
    assert!(counts.iter().all(|&c| c > 0), "{counts:?}");
}

#[test]
fn a_pocket_beside_the_corner_ball_keeps_the_volume() {
    // Tasche von unten (x, y 1,3…1,7) bis z = 8 liegt ganz im Material, das nach der Kugelecke
    // r = 3 um (3, 3, 7) bleibt: dieselbe Volumenänderung wie am vollen Würfel.
    let cube = k().make_box(DVec3::splat(10.0)).unwrap();
    let tool = k()
        .transform(
            &k().make_box(DVec3::new(0.4, 0.4, 9.0)).unwrap(),
            &DMat4::from_translation(DVec3::new(1.3, 1.3, -1.0)),
        )
        .unwrap();
    let s = k().boolean(&cube, &tool, BoolOp::Difference).unwrap();
    let v = DVec3::new(0.0, 0.0, 10.0);
    let three = [
        edge_at(&s, v, DVec3::new(10.0, 0.0, 10.0)),
        edge_at(&s, v, DVec3::new(0.0, 10.0, 10.0)),
        edge_at(&s, v, DVec3::ZERO),
    ];
    let r = 3.0;
    let (a, _) = fillet(r);
    let b = blend(&s, &three, BlendKind::Fillet { radius: r });
    check(&s, &b, -(3.0 * a * (10.0 - r) + ball_corner(r)), "Tasche");
}
