//! Nachbesserung Task 15 (Review I-1, I-2): Rand eines Booleschen Freiform-Durchbruchs (angepasste
//! kubische Schnittkante mit vielen Knoten), lang gestreckte Ellipse und Spline-Wand mit inneren
//! Knoten – jedes `Ok` gültig, dicht vernetzbar, ΔV nach geschlossener Formel, Volumenprobe des
//! Features. Formeln:
//! - Durchbruch (konvexer Querschnitt E, Rundung r am oberen Rand): Das Loch wächst in der Höhe
//!   um `r − ρ` (ρ = √(r² − t²)), Fläche `(r − ρ) P + π (r − ρ)²`, integriert
//!   `ΔV = −(P r² (1 − π/4) + π r³ (5/3 − π/2))`.
//! - Außenrand (Steiner): `ΔV = −((P − 2πr) r² (1 − π/4) + π r³ / 3)`.
//! - Spline-Wand mit waagerechten Enden (∫κ ds = 0): `ΔV = −L r² (1 − π/4)`.
//!
//! Netzschranke wie `blend_freeform_volume.rs` (`FREEFORM_MESH_REL`, Sehne 1e-3).

mod blend_common;

use blend_common::*;
use planora_kernel::*;
use planora_math::nurbs::NurbsCurve;
use planora_math::{DMat4, DVec2, DVec3, Frame, PI, TAU};
use std::sync::Arc;

/// Sehnentoleranz.
const CHORD: f64 = 1e-3;
/// Relative Netzschranke bei Freiformflächen und Sehne 1e-3. Hier sind die Volumenänderungen klein
/// gegen die neu geteilten Freiformflächen (ΔV 0,15 mm³ bei der Ellipse 4 × 1): gemessen bis
/// 2,4e-2, linear fallend mit der Sehne (8,6e-3 bei 5e-4, 3,8e-3 bei 2e-4). Die exakte Prüfung ist
/// die Schätzung des Kernels gegen die Formel ([`ESTIMATE_REL`], Querschnitte von geraden
/// Flächenstücken begrenzt) und die punktweise Lage in `planora-kernel/tests/blend_freeform_adaptive.rs`.
const FREEFORM_MESH_REL: f64 = 5e-2;
/// Sehne und relative Schranke der Release-Reihe.
const FINE: (f64, f64) = (2e-4, 1e-2);
/// Relative Schranke der Kernel-Schätzung gegen die Formel.
const ESTIMATE_REL: f64 = 1e-4;
/// Querschnittsfläche der Rundung mit Radius 1 an einer rechtwinkligen Kante.
const A: f64 = 1.0 - PI / 4.0;

fn simpson(a: f64, b: f64, n: usize, f: impl Fn(f64) -> f64) -> f64 {
    let h = (b - a) / n as f64;
    let mut sum = f(a) + f(b);
    for i in 1..n {
        sum += f(a + h * i as f64) * if i % 2 == 1 { 4.0 } else { 2.0 };
    }
    sum * h / 3.0
}

fn ellipse_perimeter(a: f64, b: f64) -> f64 {
    simpson(0.0, TAU, 40_000, |t| {
        (a * a * t.sin().powi(2) + b * b * t.cos().powi(2)).sqrt()
    })
}

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

/// Prüft ein `Ok`: gültig, dichtes Netz (über `delta_volume`), ΔV nach Formel, Volumenprobe.
fn check(before: &Solid, b: &Blended, want: f64, what: &str) -> f64 {
    check_at(before, b, want, what, (CHORD, FREEFORM_MESH_REL), true)
}

fn check_at(
    before: &Solid,
    b: &Blended,
    want: f64,
    what: &str,
    (chord, rel): (f64, f64),
    estimate: bool,
) -> f64 {
    // Die analytische Schätzung nähert den Bogen durch ein Vieleck; exakt ist die der Bahn.
    assert!(
        !estimate || (b.volume_change - want).abs() <= ESTIMATE_REL * want.abs(),
        "{what}: Schätzung {} statt {want}",
        b.volume_change
    );
    let got = delta_volume(before, &b.solid, chord);
    assert!(
        (got - want).abs() <= rel * want.abs(),
        "{what}: ΔV {got} statt {want} (relativ {:.2e})",
        ((got - want) / want).abs()
    );
    assert!(plausible(before, b, chord), "{what}: Volumenprobe");
    got
}

/// Quader 10 × 10 × 5 minus Durchbruch (Mitte (5, 5)).
fn holed(tool: &Solid) -> Solid {
    let block = k().make_box(DVec3::new(10.0, 10.0, 5.0)).unwrap();
    let tool = k()
        .transform(tool, &DMat4::from_translation(DVec3::new(5.0, 5.0, -1.0)))
        .unwrap();
    k().boolean(&block, &tool, BoolOp::Difference).unwrap()
}

/// Obere Randkante des Durchbruchs (bei z = 5, nicht gerade).
fn hole_rim(s: &Solid) -> Id<Edge> {
    s.edges
        .iter()
        .map(|(id, _)| id)
        .find(|&id| {
            let e = &s.edges[id];
            !matches!(e.curve, Curve::Line { .. }) && (e.curve.point(e.t0).z - 5.0).abs() < 1e-6
        })
        .unwrap()
}

fn hole_fillet(p: f64, r: f64) -> f64 {
    p * r * r * A + PI * r * r * r * (5.0 / 3.0 - PI / 2.0)
}

#[test]
fn boolean_freeform_hole_rims_match_the_formula_and_the_analytic_twin() {
    let r = 0.5;
    // Freiform-Kreisloch R = 2 (Ellipse mit gleichen Halbachsen) und analytischer Zwilling.
    let free = holed(&ellipse_prism(2.0, 2.0, 7.0));
    let twin = holed(&k().make_cylinder(2.0, 7.0).unwrap());
    let want = -hole_fillet(TAU * 2.0, r);
    let mut got = Vec::new();
    for (s, what) in [(&free, "Freiform-Loch"), (&twin, "Zylinderloch")] {
        let e = hole_rim(s);
        let b = blend_edges(s, &[e], BlendKind::Fillet { radius: r }, false)
            .unwrap_or_else(|x| panic!("{what}: {x}"));
        let estimate = what == "Freiform-Loch";
        got.push(check_at(
            s,
            &b,
            want,
            what,
            (CHORD, FREEFORM_MESH_REL),
            estimate,
        ));
    }
    assert!(
        (got[0] - got[1]).abs() <= FREEFORM_MESH_REL * want.abs(),
        "{got:?}"
    );
    // Elliptisches Loch 2 × 1,5, r = 0,2.
    let s = holed(&ellipse_prism(2.0, 1.5, 7.0));
    let e = hole_rim(&s);
    let b = blend_edges(&s, &[e], BlendKind::Fillet { radius: 0.2 }, false).unwrap();
    check(
        &s,
        &b,
        -hole_fillet(ellipse_perimeter(2.0, 1.5), 0.2),
        "Ellipsenloch",
    );
}

#[test]
fn an_elongated_ellipse_and_a_wavy_spline_rim_match_their_formulas() {
    // Ellipse 4 × 1 (ρ_min = 0,25), r = 0,2.
    let s = ellipse_prism(4.0, 1.0, 3.0);
    let e = edge_between(&s, "Wand 1", "Deckel");
    let b = blend_edges(&s, &[e], BlendKind::Fillet { radius: 0.2 }, false).unwrap();
    let p = ellipse_perimeter(4.0, 1.0);
    let r: f64 = 0.2;
    check(
        &s,
        &b,
        -((p - TAU * r) * r * r * A + PI * r * r * r / 3.0),
        "Ellipse 4×1",
    );
    // Kubische Spline-Wand mit inneren Knoten und waagerechten Enden.
    let ys = [0.0, 0.0, 0.8, -0.8, 0.8, -0.4, 0.8, 0.0, 0.0];
    let n = ys.len();
    let control: Vec<DVec3> = ys
        .iter()
        .enumerate()
        .map(|(i, y)| DVec3::new(10.0 * i as f64 / (n - 1) as f64, *y, 0.0))
        .collect();
    let mut knots = vec![0.0; 4];
    knots.extend([0.15, 0.3, 0.42, 0.6, 0.85]);
    knots.extend([1.0; 4]);
    let wall = NurbsCurve::new(3, knots, control, vec![1.0; n]).unwrap();
    let (t0, t1) = wall.domain();
    let len = simpson(t0, t1, 40_000, |t| wall.tangent(t).length());
    let profile = Profile {
        outer: ProfileLoop {
            start: DVec2::ZERO,
            segments: vec![
                Segment::Nurbs {
                    curve: Arc::new(wall),
                },
                Segment::Line {
                    to: DVec2::new(10.0, 6.0),
                },
                Segment::Line {
                    to: DVec2::new(0.0, 6.0),
                },
                Segment::Line { to: DVec2::ZERO },
            ],
        },
        holes: vec![],
    };
    let s = k().extrude(&profile, &Frame::WORLD, 3.0).unwrap();
    let e = edge_between(&s, "Wand 1", "Deckel");
    let r = 0.15;
    let b = blend_edges(&s, &[e], BlendKind::Fillet { radius: r }, false).unwrap();
    check(&s, &b, -len * r * r * A, "Spline-Welle");
}

#[test]
#[ignore = "feinere Sehne 2e-4, nur mit --release --ignored (etwa 15 s)"]
fn the_elongated_ellipse_at_a_fine_chord() {
    let s = ellipse_prism(4.0, 1.0, 3.0);
    let e = edge_between(&s, "Wand 1", "Deckel");
    let r: f64 = 0.2;
    let b = blend_edges(&s, &[e], BlendKind::Fillet { radius: r }, false).unwrap();
    let p = ellipse_perimeter(4.0, 1.0);
    check_at(
        &s,
        &b,
        -((p - TAU * r) * r * r * A + PI * r * r * r / 3.0),
        "Ellipse 4×1 fein",
        FINE,
        true,
    );
}
