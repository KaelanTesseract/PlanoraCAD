//! Rundungen und Fasen, Stufe 2: Ketten, Tangentenkette, Ecken, Zufallsreihe gegen geschlossene
//! Formeln (Teilprojekt 8, Task 12).

mod blend_common;

use blend_common::*;
use planora_kernel::blend::topo::vertex_edges;
use planora_kernel::*;
use planora_math::{DMat4, DVec2, DVec3, Frame, PI};
use planora_mesh::{Discretizer, mass_properties, tessellate_with};

const CHORD: f64 = 1e-3;
const REL: f64 = 5e-3;
/// Feinere Sehne für die Zufallsreihe: bei kleinen Radien (Fall 8: r = 0,56) ist der Netzfehler
/// eines Zylinders mit Sehne 1e-3 größer als 0,5 % der Volumenänderung (gemessen: Netz
/// konvergiert gegen die Formel, -2,0729 / -2,0622 / -2,0613 bei 1e-3 / 1e-4 / 3e-5; Formel -2,0609).
const SERIES_CHORD: f64 = 1e-4;
const A: f64 = 1.0 - PI / 4.0;

fn stadium(h: f64) -> Solid {
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
    k().extrude(&profile, &Frame::WORLD, h).unwrap()
}

#[test]
fn a_tangent_chain_around_a_stadium_follows_steiner() {
    let s = stadium(3.0);
    let e = edge_between(&s, "Deckel", "Wand 1");
    let r = 0.5;
    let b = blend_edges(&s, &[e], BlendKind::Fillet { radius: r }, true).unwrap();
    let offset = r * (5.0 / 6.0 - PI / 4.0) / A;
    let want = -A * r * r * (20.0 + 4.0 * PI - 2.0 * PI * offset);
    assert_rel(delta_volume(&s, &b.solid, CHORD), want, REL, "Stadion");
    assert!(plausible(&s, &b, 0.05));
}

#[test]
fn box_corners_follow_their_closed_forms() {
    let b = k().make_box(DVec3::splat(10.0)).unwrap();
    let (e1, e2, e3) = (
        edge_between(&b, "Deckel", "Wand 1"),
        edge_between(&b, "Deckel", "Wand 2"),
        edge_between(&b, "Wand 1", "Wand 2"),
    );
    let r = 1.5;
    let three = blend_edges(&b, &[e1, e2, e3], BlendKind::Fillet { radius: r }, false).unwrap();
    let want = -(3.0 * A * r * r * (10.0 - r) + (1.0 - PI / 6.0) * r * r * r);
    assert_rel(
        delta_volume(&b, &three.solid, CHORD),
        want,
        REL,
        "Kugelecke",
    );
    let miter = blend_edges(&b, &[e1, e2], BlendKind::Fillet { radius: r }, false).unwrap();
    let want = -(A * r * r * 20.0 - (5.0 / 3.0 - PI / 2.0) * r * r * r);
    assert_rel(delta_volume(&b, &miter.solid, CHORD), want, REL, "Gehrung");
    let d = 1.5;
    let chamfer = blend_edges(
        &b,
        &[e1, e2],
        BlendKind::ChamferEqual { distance: d },
        false,
    )
    .unwrap();
    let want = -(0.5 * d * d * 20.0 - d * d * d / 3.0);
    assert_rel(
        delta_volume(&b, &chamfer.solid, CHORD),
        want,
        1e-9,
        "Fasengehrung",
    );
}

#[test]
fn all_twelve_edges_follow_steiner() {
    let (a, b, c, r) = (10.0, 8.0, 6.0, 1.0);
    let s = k().make_box(DVec3::new(a, b, c)).unwrap();
    let edges: Vec<Id<Edge>> = s.edges.iter().map(|(id, _)| id).collect();
    let res = blend_edges(&s, &edges, BlendKind::Fillet { radius: r }, false).unwrap();
    let (a1, b1, c1) = (a - 2.0 * r, b - 2.0 * r, c - 2.0 * r);
    let v = a1 * b1 * c1
        + 2.0 * (a1 * b1 + b1 * c1 + c1 * a1) * r
        + PI * r * r * (a1 + b1 + c1)
        + 4.0 / 3.0 * PI * r * r * r;
    assert_rel(
        delta_volume(&s, &res.solid, CHORD),
        v - a * b * c,
        REL,
        "zwölf Kanten",
    );
}

/// Netzvolumen eines Körpers (Sehne `CHORD`).
fn mesh_volume(s: &Solid) -> f64 {
    let disc = Discretizer::for_solids(&[s], CHORD);
    mass_properties(&tessellate_with(s, &disc).unwrap()).volume
}

#[test]
fn corner_results_survive_follow_ups() {
    let b = k().make_box(DVec3::splat(10.0)).unwrap();
    let edges = [
        edge_between(&b, "Deckel", "Wand 1"),
        edge_between(&b, "Deckel", "Wand 2"),
        edge_between(&b, "Wand 1", "Wand 2"),
    ];
    let one = blend_edges(&b, &edges, BlendKind::Fillet { radius: 1.0 }, false)
        .unwrap()
        .solid;
    // R-7: Der Würfel (2 mm) schneidet das Kugelstück und beide angrenzenden Rundungen an der
    // Ecke (10, 0, 10).
    let cutter = k()
        .transform(
            &k().make_box(DVec3::splat(2.0)).unwrap(),
            &DMat4::from_translation(DVec3::new(9.5, -1.0, 9.5)),
        )
        .unwrap();
    let cut = k().boolean(&one, &cutter, BoolOp::Difference).unwrap();
    validate(&cut).unwrap();
    let (v_one, v_cut) = (mesh_volume(&one), mesh_volume(&cut));
    assert!(
        v_cut < v_one,
        "Schnitt durch das Eckstück trägt nichts ab: {v_cut} statt weniger als {v_one}"
    );
    let e = edge_between(&one, "Boden", "Wand 3");
    let two = blend_edges(&one, &[e], BlendKind::Fillet { radius: 0.5 }, false).unwrap();
    assert_rel(
        delta_volume(&one, &two.solid, CHORD),
        -A * 0.25 * 10.0,
        REL,
        "weitere Rundung",
    );
}

/// Erwartete Volumenänderung einer Kantenauswahl an einem Quader (Radius r ≤ 0,45 · kleinste
/// Kante): Kanten bis zu Kugelecken gekürzt, Gehrungen mit ihrer Überlappung, Kugelecken.
fn box_selection_change(s: &Solid, chosen: &[Id<Edge>], r: f64) -> f64 {
    let mut removed = 0.0;
    for &e in chosen {
        let edge = &s.edges[e];
        let len = (edge.t1 - edge.t0).abs();
        let at_corner = |v| {
            vertex_edges(s, v)
                .iter()
                .filter(|x| chosen.contains(x))
                .count()
        };
        let short = [edge.start, edge.end]
            .iter()
            .filter(|v| at_corner(**v) == 3)
            .count() as f64;
        removed += A * r * r * (len - r * short);
    }
    for (v, _) in s.vertices.iter() {
        match vertex_edges(s, v)
            .iter()
            .filter(|x| chosen.contains(x))
            .count()
        {
            2 => removed -= (5.0 / 3.0 - PI / 2.0) * r * r * r,
            3 => removed += (1.0 - PI / 6.0) * r * r * r,
            _ => {}
        }
    }
    -removed
}

fn random_box_selections(cases: usize, seed: u64) {
    let mut rng = Lcg(seed);
    for case in 0..cases {
        let size = DVec3::new(
            rng.range(4.0, 12.0),
            rng.range(4.0, 12.0),
            rng.range(4.0, 12.0),
        );
        let s = k().make_box(size).unwrap();
        let all: Vec<Id<Edge>> = s.edges.iter().map(|(id, _)| id).collect();
        let mut chosen: Vec<Id<Edge>> = all.iter().copied().filter(|_| rng.next() < 0.4).collect();
        if chosen.is_empty() {
            chosen.push(all[case % all.len()]);
        }
        let r = rng.range(0.1, 0.45) * size.min_element();
        let b = blend_edges(&s, &chosen, BlendKind::Fillet { radius: r }, false)
            .unwrap_or_else(|e| panic!("Fall {case}: {e}"));
        let want = box_selection_change(&s, &chosen, r);
        assert_rel(
            delta_volume(&s, &b.solid, SERIES_CHORD),
            want,
            REL,
            &format!("Fall {case}"),
        );
        assert!(plausible(&s, &b, 0.05), "Fall {case}: Volumenprobe");
    }
}

#[test]
fn seeded_box_selections_small() {
    random_box_selections(6, 0x5eed_0001);
}

#[test]
#[ignore = "200 zufällige Kantenauswahlen, nur mit --release --ignored (etwa 40 s)"]
fn seeded_box_selections_200() {
    random_box_selections(200, 0x5eed_0002);
}

#[test]
fn concave_corners_and_mixed_corners_are_named_not_wrong() {
    let l = extrude_polygon(
        &[
            DVec2::new(0.0, 0.0),
            DVec2::new(10.0, 0.0),
            DVec2::new(10.0, 4.0),
            DVec2::new(4.0, 4.0),
            DVec2::new(4.0, 10.0),
            DVec2::new(0.0, 10.0),
        ],
        5.0,
    );
    let mixed = [
        edge_between(&l, "Deckel", "Wand 3"),
        edge_between(&l, "Wand 3", "Wand 4"),
    ];
    assert!(matches!(
        blend_edges(&l, &mixed, BlendKind::Fillet { radius: 1.0 }, false),
        Err(BlendError::Corner(_))
    ));
}
