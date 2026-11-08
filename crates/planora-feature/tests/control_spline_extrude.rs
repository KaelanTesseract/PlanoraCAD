//! Spline mit Kontrollpunkten als Profil im Dokument (Skizzen 7, Task 3): Das Volumen entspricht
//! der numerischen Fläche unter der Kurve mal Höhe.

use planora_feature::*;
use planora_kernel::Solid;
use planora_math::{DVec2, Frame, Tolerance};
use planora_mesh::{TessellationOptions, mass_properties, tessellate};
use planora_sketch::model::{CurveId, Sketch};

fn volume(s: &Solid) -> f64 {
    let options = TessellationOptions {
        chord_tolerance: 1e-3,
    };
    mass_properties(&tessellate(s, &options).unwrap()).volume
}

fn feature_of(sk: Sketch) -> FeatureKind {
    FeatureKind::Sketch(SketchFeature {
        sketch: sk,
        plane: SketchPlane::Standard(StandardPlane::XY),
        copies: vec![],
        formulas: Default::default(),
    })
}

/// Fläche zwischen der Kurve und ihrer Sehne auf der x-Achse (Gauß-Formel über dichte Abtastung).
fn area_under(s: &Sketch, c: CurveId) -> f64 {
    let (lo, hi) = s.curve_domain(c).unwrap();
    let n = 200_000;
    let at = |i: usize| {
        s.curve_eval(c, lo + (hi - lo) * i as f64 / n as f64)
            .unwrap()
    };
    let mut sum = 0.0;
    let mut prev: DVec2 = at(0);
    for i in 1..=n {
        let q = at(i);
        sum += prev.perp_dot(q);
        prev = q;
    }
    // Schließend zurück zum Anfang.
    sum += prev.perp_dot(at(0));
    (0.5 * sum).abs()
}

fn arch(degree: u8) -> (Sketch, CurveId) {
    let mut sk = Sketch::new(Frame::WORLD, "Skizze", Tolerance::default());
    let p: Vec<_> = [
        (0.0, 0.0),
        (2.0, 5.0),
        (5.0, 7.0),
        (9.0, 6.0),
        (12.0, 3.0),
        (14.0, 0.0),
    ]
    .iter()
    .map(|&(x, y)| sk.add_point(x, y).unwrap())
    .collect();
    let c = sk.add_control_spline(&p, degree).unwrap();
    sk.add_line(p[5], p[0]).unwrap();
    (sk, c)
}

#[test]
fn extrude_control_spline_profile_has_the_right_volume() {
    for degree in [3, 5] {
        let (sk, c) = arch(degree);
        let area = area_under(&sk, c);
        let refs = sk.profiles().refs;
        assert_eq!(refs.len(), 1);
        let mut d = Document::new();
        let s = d.add_feature("Skizze 1", feature_of(sk)).unwrap();
        let e = d
            .add_feature(
                "Extrusion 1",
                FeatureKind::Extrude {
                    sketch: s,
                    profiles: ProfileSel::Refs(refs),
                    height: 3.0,
                },
            )
            .unwrap();
        assert_eq!(d.feature(e).unwrap().status, FeatureStatus::Ok);
        let bodies = d.visible_bodies();
        assert_eq!(bodies.len(), 1);
        let got = volume(bodies[0].solid);
        let want = area * 3.0;
        assert!(
            ((got - want) / want).abs() < 2e-3,
            "Grad {degree}: {got} statt {want}"
        );
        d.check_invariants().unwrap();
    }
}

#[test]
fn a_trimmed_control_spline_profile_extrudes_too() {
    let (mut sk, c) = arch(3);
    let a = sk.add_point(7.0, -2.0).unwrap();
    let b = sk.add_point(7.0, 20.0).unwrap();
    let cut = sk.add_line(a, b).unwrap();
    let click = sk.curve_eval(c, 2.8).unwrap();
    sk.trim(c, click).unwrap();
    let parts = sk.profiles();
    assert!(parts.rejected.is_empty(), "{:?}", parts.rejected);
    assert!(!parts.profiles.is_empty());
    let refs = parts.refs;
    let _ = cut;
    let mut d = Document::new();
    let s = d.add_feature("Skizze 1", feature_of(sk)).unwrap();
    let e = d
        .add_feature(
            "Extrusion 1",
            FeatureKind::Extrude {
                sketch: s,
                profiles: ProfileSel::Refs(vec![refs[0].clone()]),
                height: 2.0,
            },
        )
        .unwrap();
    assert_eq!(d.feature(e).unwrap().status, FeatureStatus::Ok);
    d.check_invariants().unwrap();
}
