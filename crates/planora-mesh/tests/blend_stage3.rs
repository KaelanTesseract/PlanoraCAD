//! Rundungen und Fasen, Stufe 3: variabler Radius gegen numerisches Integral, Freiformkanten
//! gegen Zwillinge und Steiner-Formel, Zufallsreihe (Teilprojekt 8, Task 16).

mod blend_common;

use blend_common::*;
use planora_kernel::boolean::{Location, solid_contains, solid_faces};
use planora_kernel::*;
use planora_math::nurbs::NurbsCurve;
use planora_math::{DVec3, Frame, PI, RadiusLaw, TAU};

const CHORD: f64 = 1e-3;
/// Sehne für Körper mit Freiformflächen: Bei 1e-3 hebt sich der Netzfehler der Freiform-Vernetzung
/// zwischen Eingang und Ergebnis nicht auf (gemessen 0,8 bis 4,2 % relativ zu ΔV, siehe
/// `blend_freeform_volume.rs`); bei 1e-4 liegt er unter [`REL`].
const FREEFORM_CHORD: f64 = 1e-4;
/// Relative Schranke bei Sehne [`CHORD`] für Freiformkörper (wie `FREEFORM_MESH_REL` in
/// `blend_freeform_volume.rs`); die feinen Reihen prüfen mit [`REL`].
const FREEFORM_MESH_REL: f64 = 1e-2;
const REL: f64 = 5e-3;
const A: f64 = 1.0 - PI / 4.0;

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

/// ∫₀ᴸ (1 − π/4)·r(s/L)² ds nach Simpson.
fn spandrel_integral(law: &RadiusLaw, length: f64) -> f64 {
    let n = 4000;
    let h = length / n as f64;
    let f = |s: f64| A * law.radius(s / length).powi(2);
    let mut sum = f(0.0) + f(length);
    for i in 1..n {
        sum += f(i as f64 * h) * if i % 2 == 1 { 4.0 } else { 2.0 };
    }
    sum * h / 3.0
}

#[test]
fn a_variable_radius_matches_its_integral() {
    let s = k().make_box(DVec3::splat(10.0)).unwrap();
    let e = edge_between(&s, "Deckel", "Wand 1");
    let points = vec![(0.0, 1.0), (0.4, 2.0), (0.7, 1.2), (1.0, 1.6)];
    let law = RadiusLaw::new(&points).unwrap();
    let b = blend_edges(&s, &[e], BlendKind::VariableFillet { points }, false).unwrap();
    assert_rel(
        delta_volume(&s, &b.solid, CHORD),
        -spandrel_integral(&law, 10.0),
        REL,
        "variabel",
    );
    assert!(plausible(&s, &b, 0.05));
}

fn freeform_twin(chord: f64, rel: f64) {
    let s = ellipse_prism(5.0, 5.0, 8.0);
    let twin = k().make_cylinder(5.0, 8.0).unwrap();
    for kind in [
        BlendKind::Fillet { radius: 1.0 },
        BlendKind::ChamferEqual { distance: 1.0 },
    ] {
        let b = blend_edges(
            &s,
            &[edge_between(&s, "Wand 1", "Deckel")],
            kind.clone(),
            false,
        )
        .unwrap();
        let bt = blend_edges(
            &twin,
            &[edge_between(&twin, "Wand 1", "Deckel")],
            kind.clone(),
            false,
        )
        .unwrap();
        let (d, dt) = (
            delta_volume(&s, &b.solid, chord),
            delta_volume(&twin, &bt.solid, chord),
        );
        assert_rel(d, dt, rel, &format!("Zwilling {kind:?}"));
    }
}

#[test]
fn a_freeform_cylinder_rounds_like_its_twin() {
    freeform_twin(CHORD, FREEFORM_MESH_REL);
}

#[test]
#[ignore = "feine Sehne 1e-4, nur mit --release --ignored (etwa 50 s)"]
fn a_freeform_cylinder_rounds_like_its_twin_fine() {
    freeform_twin(FREEFORM_CHORD, 2e-3);
}

fn ellipse_steiner(chord: f64, rel: f64) {
    let (a, b, r) = (5.0, 3.0, 0.5);
    let s = ellipse_prism(a, b, 4.0);
    let res = blend_edges(
        &s,
        &[edge_between(&s, "Wand 1", "Deckel")],
        BlendKind::Fillet { radius: r },
        false,
    )
    .unwrap();
    let n = 100_000;
    let perimeter: f64 = (0..n)
        .map(|i| {
            let (t0, t1) = (TAU * i as f64 / n as f64, TAU * (i + 1) as f64 / n as f64);
            let p = |t: f64| DVec3::new(a * t.cos(), b * t.sin(), 0.0);
            p(t0).distance(p(t1))
        })
        .sum();
    let offset = r * (5.0 / 6.0 - PI / 4.0) / A;
    let want = -A * r * r * (perimeter - TAU * offset);
    assert_rel(delta_volume(&s, &res.solid, chord), want, rel, "Ellipse");
    let faces = solid_faces(&res.solid);
    assert_eq!(
        solid_contains(&faces, DVec3::new(0.0, 0.0, 2.0), &tol()).unwrap(),
        Location::Inside
    );
    assert_eq!(
        solid_contains(&faces, DVec3::new(a - 0.05, 0.0, 3.95), &tol()).unwrap(),
        Location::Outside
    );
}

#[test]
fn an_ellipse_top_fillet_follows_steiner() {
    ellipse_steiner(CHORD, FREEFORM_MESH_REL);
}

#[test]
#[ignore = "feine Sehne 1e-4, nur mit --release --ignored (etwa 30 s)"]
fn an_ellipse_top_fillet_follows_steiner_fine() {
    ellipse_steiner(FREEFORM_CHORD, REL);
}

/// Zufällige Zylinder (Deckel- und Bodenkante, Rundung und Fase) gegen Pappus und gegen den
/// Freiform-Zwilling.
///
/// Der Freiform-Zwilling wird immer gerundet, aber nur in jedem `mesh_every`-ten Fall (0 = nie)
/// vernetzt und mit Sehne [`FREEFORM_CHORD`] gegen die Formel geprüft (siehe dort).
fn random_cylinders(cases: usize, seed: u64, mesh_every: usize) {
    let mut rng = Lcg(seed);
    for case in 0..cases {
        let (big, h) = (rng.range(2.0, 8.0), rng.range(2.0, 8.0));
        let r = rng.range(0.05, 0.45) * big.min(h);
        let top = rng.next() < 0.5;
        let fillet = rng.next() < 0.5;
        let kind = if fillet {
            BlendKind::Fillet { radius: r }
        } else {
            BlendKind::ChamferEqual { distance: r }
        };
        let side = if top { "Deckel" } else { "Boden" };
        let s = k().make_cylinder(big, h).unwrap();
        let b = blend_edges(&s, &[edge_between(&s, "Wand 1", side)], kind.clone(), false)
            .unwrap_or_else(|e| panic!("Fall {case}: {e}"));
        let want = if fillet {
            let offset = r * (5.0 / 6.0 - PI / 4.0) / A;
            -A * r * r * TAU * (big - offset)
        } else {
            -0.5 * r * r * TAU * (big - r / 3.0)
        };
        assert_rel(
            delta_volume(&s, &b.solid, CHORD),
            want,
            REL,
            &format!("Fall {case}"),
        );
        let n = ellipse_prism(big, big, h);
        let bn = blend_edges(&n, &[edge_between(&n, "Wand 1", side)], kind, false)
            .unwrap_or_else(|e| panic!("Fall {case} (Freiform): {e}"));
        if mesh_every > 0 && case % mesh_every == 0 {
            assert_rel(
                delta_volume(&n, &bn.solid, FREEFORM_CHORD),
                want,
                REL,
                &format!("Fall {case} Freiform"),
            );
        }
    }
}

#[test]
fn seeded_cylinders_small() {
    random_cylinders(3, 0x5eed_0101, 0);
}

#[test]
#[ignore = "100 zufällige Zylinder, jeder zehnte mit Freiform-Zwilling bei Sehne 1e-4, nur mit --release --ignored"]
fn seeded_cylinders_100() {
    random_cylinders(100, 0x5eed_0102, 10);
}
