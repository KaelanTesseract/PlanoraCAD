//! Extrusion und Rotation mit Profil-Verweisen (Skizzen 1, Task 6): mehrere Flächen in einem
//! Feature, nebeneinanderliegende werden ein Körper, getrennte je einer (wie Fusion); verlorene
//! Profile werden gemeldet, alte Profilnummern ergeben denselben Körper wie früher.

use planora_exchange::Progress;
use planora_feature::*;
use planora_i18n::{english, german, tr, with_lang};
use planora_kernel::{Kernel, Solid};
use planora_math::{DVec2, Frame, Tolerance};
use planora_mesh::{TessellationOptions, mass_properties, tessellate};
use planora_sketch::{Sketch, extrude_profile};
use planora_step::{StepBody, StepOptions, write_step};
use std::f64::consts::PI;

fn volume(s: &Solid) -> f64 {
    let options = TessellationOptions {
        chord_tolerance: 1e-3,
    };
    mass_properties(&tessellate(s, &options).unwrap()).volume
}

fn sketch_feature(sk: Sketch) -> FeatureKind {
    FeatureKind::Sketch(SketchFeature {
        sketch: sk,
        plane: SketchPlane::Standard(StandardPlane::XY),
        copies: vec![],
        formulas: Default::default(),
    })
}

fn new_sketch() -> Sketch {
    Sketch::new(Frame::WORLD, "S", Tolerance::default())
}

/// Verweise auf alle Flächen der Skizze.
fn all_refs(sk: &Sketch) -> Vec<planora_sketch::ProfileRef> {
    sk.profiles().refs
}

/// Körper des Features `f` (sichtbar am Ende).
fn bodies_of(d: &Document, f: FeatureId) -> Vec<&Solid> {
    d.visible_bodies()
        .into_iter()
        .filter(|v| v.body.feature == f)
        .map(|v| v.solid)
        .collect()
}

fn assert_ok(d: &Document, f: FeatureId) {
    let st = &d.feature(f).unwrap().status;
    assert_eq!(*st, FeatureStatus::Ok, "{st:?}");
}

#[test]
fn two_separate_rectangles_give_two_bodies_in_one_feature() {
    let mut sk = new_sketch();
    sk.add_rectangle(DVec2::ZERO, DVec2::new(10.0, 10.0))
        .unwrap();
    sk.add_rectangle(DVec2::new(20.0, 0.0), DVec2::new(30.0, 5.0))
        .unwrap();
    let refs = all_refs(&sk);
    assert_eq!(refs.len(), 2);
    let mut d = Document::new();
    let s = d.add_feature("S", sketch_feature(sk)).unwrap();
    let e = d
        .add_feature(
            "E",
            FeatureKind::Extrude {
                sketch: s,
                profiles: ProfileSel::Refs(refs),
                height: 2.0,
            },
        )
        .unwrap();
    assert_ok(&d, e);
    let bodies = bodies_of(&d, e);
    assert_eq!(bodies.len(), 2);
    assert!(
        (volume(bodies[0]) - 200.0).abs() < 1e-6,
        "kleinste Nummer zuerst"
    );
    assert!((volume(bodies[1]) - 100.0).abs() < 1e-6);
    for b in &bodies {
        assert!(
            b.faces
                .iter()
                .all(|(_, f)| f.tag.as_ref().unwrap().origin == e.0),
            "Herkunft je Körper"
        );
    }
}

#[test]
fn both_halves_of_a_rectangle_with_a_diagonal_give_one_body() {
    let mut sk = new_sketch();
    let r = sk
        .add_rectangle(DVec2::ZERO, DVec2::new(20.0, 10.0))
        .unwrap();
    sk.add_line(r.points[0], r.points[2]).unwrap();
    let refs = all_refs(&sk);
    assert_eq!(refs.len(), 2, "zwei Dreiecke");
    let mut d = Document::new();
    let s = d.add_feature("S", sketch_feature(sk)).unwrap();
    let e = d
        .add_feature(
            "E",
            FeatureKind::Extrude {
                sketch: s,
                profiles: ProfileSel::Refs(refs),
                height: 3.0,
            },
        )
        .unwrap();
    assert_ok(&d, e);
    let bodies = bodies_of(&d, e);
    assert_eq!(bodies.len(), 1);
    assert!((volume(bodies[0]) - 600.0).abs() < 1e-6);
    assert_eq!(bodies[0].faces.len(), 6, "ganzer Quader ohne Naht");
}

/// Rechteck als Verweis gewählt, danach teilt eine Linie das Profil: Fehler mit Hinweis.
fn split_after_selection() -> String {
    let mut sk = new_sketch();
    sk.add_rectangle(DVec2::ZERO, DVec2::new(20.0, 10.0))
        .unwrap();
    let refs = all_refs(&sk);
    let mut d = Document::new();
    let s = d.add_feature("S", sketch_feature(sk)).unwrap();
    let e = d
        .add_feature(
            "E",
            FeatureKind::Extrude {
                sketch: s,
                profiles: ProfileSel::Refs(refs),
                height: 3.0,
            },
        )
        .unwrap();
    assert_ok(&d, e);
    d.update(s, |k| {
        let FeatureKind::Sketch(sf) = k else {
            unreachable!()
        };
        let a = sf.sketch.add_point(10.0, -5.0).unwrap();
        let b = sf.sketch.add_point(10.0, 15.0).unwrap();
        sf.sketch.add_line(a, b).unwrap();
    })
    .unwrap();
    assert_ok(&d, s);
    match &d.feature(e).unwrap().status {
        FeatureStatus::Error(m) => {
            assert!(bodies_of(&d, e).is_empty());
            m.clone()
        }
        other => panic!("Fehler erwartet, war {other:?}"),
    }
}

#[test]
fn a_profile_split_by_a_sketch_change_is_reported_as_lost() {
    let de = with_lang(&german(), || {
        let m = split_after_selection();
        assert!(m.contains(&tr!("feature-profile-lost")), "{m}");
        m
    });
    assert!(de.contains("nicht mehr vorhanden"), "{de}");
    let en = with_lang(&english(), split_after_selection);
    assert!(en.contains("no longer exists"), "{en}");
}

#[test]
fn old_index_of_a_plate_with_a_hole_gives_the_old_body() {
    let mut sk = new_sketch();
    sk.add_rectangle(DVec2::ZERO, DVec2::new(20.0, 10.0))
        .unwrap();
    let c = sk.add_point(10.0, 5.0).unwrap();
    sk.add_circle(c, 3.0).unwrap();
    assert!(sk.migrate_profile_index(0).is_some());
    let old = sk.legacy_profiles().profiles[0].clone();
    assert_eq!(old.holes.len(), 1, "altes Profil 0 ist die Platte mit Loch");
    let h = 4.0;
    let kernel = planora_kernel::AnalyticKernel::default();
    let expected = extrude_profile(&kernel, &sk, &old, h).unwrap();

    let mut d = Document::new();
    let s = d.add_feature("S", sketch_feature(sk)).unwrap();
    let e = d
        .add_feature(
            "E",
            FeatureKind::Extrude {
                sketch: s,
                profiles: ProfileSel::Index(0),
                height: h,
            },
        )
        .unwrap();
    assert_ok(&d, e);
    let bodies = bodies_of(&d, e);
    assert_eq!(bodies.len(), 1);
    let v = volume(bodies[0]);
    assert!((v - volume(&expected)).abs() < 1e-6, "derselbe Körper: {v}");
    assert!(((v - (200.0 - 9.0 * PI) * h) / v).abs() < 1e-3, "{v}");
    assert_eq!(bodies[0].faces.len(), expected.faces.len());
    assert_eq!(bodies[0].edges.len(), expected.edges.len());
}

#[test]
fn old_index_of_a_plate_with_a_touched_hole_gives_the_old_body() {
    let mut sk = new_sketch();
    sk.add_rectangle(DVec2::ZERO, DVec2::new(20.0, 10.0))
        .unwrap();
    let c = sk.add_point(10.0, 5.0).unwrap();
    sk.add_circle(c, 3.0).unwrap();
    let a = sk.add_point(-5.0, 8.0).unwrap();
    let b = sk.add_point(25.0, 8.0).unwrap();
    sk.add_line(a, b).unwrap();
    assert!(
        sk.migrate_profile_index(0).is_none(),
        "die Umrechnung geht hier nicht"
    );
    let old = sk.legacy_profiles().profiles[0].clone();
    let h = 4.0;
    let kernel = planora_kernel::AnalyticKernel::default();
    let expected = extrude_profile(&kernel, &sk, &old, h).unwrap();
    kernel.validate(&expected).unwrap();

    let mut d = Document::new();
    let s = d.add_feature("S", sketch_feature(sk)).unwrap();
    let e = d
        .add_feature(
            "E",
            FeatureKind::Extrude {
                sketch: s,
                profiles: ProfileSel::Index(0),
                height: h,
            },
        )
        .unwrap();
    assert_ok(&d, e);
    let bodies = bodies_of(&d, e);
    assert_eq!(bodies.len(), 1);
    let v = volume(bodies[0]);
    assert!((v - volume(&expected)).abs() < 1e-9, "derselbe Körper");
    assert!(
        ((v - (200.0 - 9.0 * PI) * h) / v).abs() < 1e-3,
        "Platte mit Loch: {v}"
    );
    assert_eq!(bodies[0].faces.len(), expected.faces.len());
    assert_eq!(bodies[0].edges.len(), expected.edges.len());
}

#[test]
fn revolving_two_profiles_fails_if_one_crosses_the_axis() {
    let mut sk = new_sketch();
    let (p, q) = (
        sk.add_point(0.0, -1.0).unwrap(),
        sk.add_point(0.0, 20.0).unwrap(),
    );
    let axis = sk.add_line(p, q).unwrap();
    sk.set_construction(axis, true).unwrap();
    sk.add_rectangle(DVec2::ZERO, DVec2::new(3.0, 5.0)).unwrap();
    sk.add_rectangle(DVec2::new(-2.0, 10.0), DVec2::new(2.0, 12.0))
        .unwrap();
    let refs = all_refs(&sk);
    assert_eq!(refs.len(), 2);
    let mut d = Document::new();
    let s = d.add_feature("S", sketch_feature(sk)).unwrap();
    let one = d
        .add_feature(
            "R1",
            FeatureKind::Revolve {
                sketch: s,
                profiles: ProfileSel::Refs(vec![refs[0].clone()]),
                axis,
            },
        )
        .unwrap();
    assert_ok(&d, one);
    let both = d
        .add_feature(
            "R2",
            FeatureKind::Revolve {
                sketch: s,
                profiles: ProfileSel::Refs(refs),
                axis,
            },
        )
        .unwrap();
    assert!(
        matches!(d.feature(both).unwrap().status, FeatureStatus::Error(_)),
        "{:?}",
        d.feature(both).unwrap().status
    );
    assert!(bodies_of(&d, both).is_empty());
}

#[test]
fn a_profile_of_an_elliptic_arc_and_a_line_extrudes_and_writes_step() {
    let mut sk = new_sketch();
    let c = sk.add_point(0.0, 0.0).unwrap();
    let a = sk.add_point(10.0, 0.0).unwrap();
    let b = sk.add_point(-10.0, 0.0).unwrap();
    sk.add_elliptic_arc(c, DVec2::new(10.0, 0.0), 5.0, a, b)
        .unwrap();
    sk.add_line(b, a).unwrap();
    let refs = all_refs(&sk);
    assert_eq!(refs.len(), 1, "{:?}", sk.profiles().rejected);
    let mut d = Document::new();
    let s = d.add_feature("S", sketch_feature(sk)).unwrap();
    let e = d
        .add_feature(
            "E",
            FeatureKind::Extrude {
                sketch: s,
                profiles: ProfileSel::Refs(refs),
                height: 2.0,
            },
        )
        .unwrap();
    assert_ok(&d, e);
    let bodies = bodies_of(&d, e);
    assert_eq!(bodies.len(), 1);
    d.kernel().validate(bodies[0]).unwrap();
    let v = volume(bodies[0]);
    assert!(((v - PI * 10.0 * 5.0 / 2.0 * 2.0) / v).abs() < 1e-3, "{v}");

    let step = [StepBody {
        name: "E".into(),
        solid: bodies[0],
    }];
    let mut out = Vec::new();
    let report = write_step(
        &step,
        &StepOptions::new("e.step"),
        &mut out,
        &Progress::new(),
    )
    .unwrap();
    assert_eq!(report.bodies, 1);
    assert!(report.rejected.is_empty(), "{:?}", report.rejected);
}
