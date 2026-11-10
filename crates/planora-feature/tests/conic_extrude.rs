//! Kegelschnitt als Profil im Dokument (Skizzen 7, Task 5): Das Volumen über einer Parabel ist
//! zwei Drittel des Dreiecks aus Start, Scheitel und Ende mal Höhe.

use planora_feature::*;
use planora_kernel::Solid;
use planora_math::{Frame, Tolerance};
use planora_mesh::{TessellationOptions, mass_properties, tessellate};
use planora_sketch::model::Sketch;

fn volume(s: &Solid) -> f64 {
    let options = TessellationOptions {
        chord_tolerance: 1e-3,
    };
    mass_properties(&tessellate(s, &options).unwrap()).volume
}

#[test]
fn extrude_a_parabola_profile_has_the_exact_volume() {
    let mut sk = Sketch::new(Frame::WORLD, "Skizze", Tolerance::default());
    let a = sk.add_point(0.0, 0.0).unwrap();
    let m = sk.add_point(5.0, 8.0).unwrap();
    let b = sk.add_point(10.0, 0.0).unwrap();
    sk.add_conic(a, b, m, 0.5).unwrap();
    sk.add_line(b, a).unwrap();
    let refs = sk.profiles().refs;
    assert_eq!(refs.len(), 1);
    let mut d = Document::new();
    let s = d
        .add_feature(
            "Skizze 1",
            FeatureKind::Sketch(SketchFeature {
                sketch: sk,
                plane: SketchPlane::Standard(StandardPlane::XY),
                copies: vec![],
                formulas: Default::default(),
            }),
        )
        .unwrap();
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
    let want = 2.0 / 3.0 * 40.0 * 3.0;
    let got = volume(bodies[0].solid);
    assert!(((got - want) / want).abs() < 2e-3, "{got} statt {want}");
    d.check_invariants().unwrap();
}
