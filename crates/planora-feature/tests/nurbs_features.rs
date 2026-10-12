//! Features aus Skizzen mit Ellipse und Spline: Extrusion, Rotation, Neuberechnung.

use planora_feature::*;
use planora_kernel::{BoolOp, Kernel, Solid, SurfaceKind};
use planora_math::{DVec2, DVec3, Frame, PI, Tolerance};
use planora_mesh::{TessellationOptions, mass_properties, tessellate};
use planora_sketch::ConstraintKind as K;
use planora_sketch::model::Sketch;
use planora_sketch::{ConstraintId, CurveId};

/// Erlaubter relativer Volumenfehler des Netzes.
const VOLUME_REL: f64 = 2e-3;

fn volume(s: &Solid) -> f64 {
    mass_properties(
        &tessellate(
            s,
            &TessellationOptions {
                chord_tolerance: 1e-3,
            },
        )
        .unwrap(),
    )
    .volume
}

fn assert_rel(got: f64, want: f64, what: &str) {
    assert!(
        ((got - want) / want).abs() < VOLUME_REL,
        "{what}: {got} statt {want}"
    );
}

fn feature_sketch(sk: Sketch) -> FeatureKind {
    FeatureKind::Sketch(SketchFeature {
        sketch: sk,
        plane: SketchPlane::Standard(StandardPlane::XY),
        copies: vec![],
    })
}

/// Skizze mit einer Ellipse (große Halbachse 5 bemaßt, kleine 2 bemaßt).
fn ellipse_sketch() -> (Sketch, ConstraintId) {
    let mut sk = Sketch::new(Frame::WORLD, "Skizze", Tolerance::default());
    let c = sk.add_point(0.0, 0.0).unwrap();
    let e = sk.add_ellipse(c, DVec2::new(5.0, 0.0), 2.0).unwrap();
    sk.add_constraint(K::Fixed(c, 0.0, 0.0)).unwrap();
    sk.add_constraint(K::EllipseAngle(e, 0.0)).unwrap();
    let major = sk.add_constraint(K::EllipseMajor(e, 5.0)).unwrap();
    sk.add_constraint(K::EllipseMinor(e, 2.0)).unwrap();
    (sk, major)
}

#[test]
fn an_ellipse_sketch_extrudes_and_follows_its_dimension() {
    let mut d = Document::new();
    let (sk, major) = ellipse_sketch();
    let s = d.add_feature("Skizze 1", feature_sketch(sk)).unwrap();
    let e = d
        .add_feature(
            "Extrusion 1",
            FeatureKind::Extrude {
                sketch: s,
                profile: 0,
                height: 3.0,
            },
        )
        .unwrap();
    for id in [s, e] {
        assert_eq!(
            d.feature(id).unwrap().status,
            FeatureStatus::Ok,
            "{:?}",
            d.feature(id).unwrap().status
        );
    }
    let vis = d.visible_bodies();
    assert_eq!(vis.len(), 1);
    let solid = vis[0].solid;
    d.kernel().validate(solid).unwrap();
    assert!(
        solid
            .faces
            .iter()
            .any(|(_, f)| f.surface.kind() == SurfaceKind::Nurbs),
        "der Mantel ist eine Freiformfläche"
    );
    assert_rel(volume(solid), PI * 5.0 * 2.0 * 3.0, "vorher");
    d.check_invariants().unwrap();

    // Maß der großen Halbachse ändern: Skizze und Extrusion werden neu berechnet.
    let report = d
        .update_with_report(s, |k| {
            if let FeatureKind::Sketch(sf) = k {
                sf.sketch.set_constraint_value(major, 7.0).unwrap();
            }
        })
        .unwrap();
    assert!(report.computed.contains(&s) && report.computed.contains(&e));
    assert_eq!(d.feature(e).unwrap().status, FeatureStatus::Ok);
    assert_rel(
        volume(d.visible_bodies()[0].solid),
        PI * 7.0 * 2.0 * 3.0,
        "nachher",
    );

    // Ein zweiter Lauf ohne Änderung rechnet nichts neu.
    let report = d.recompute();
    assert!(report.computed.is_empty(), "{:?}", report.computed);
    assert_eq!(report.reused.len(), 2);

    // Umbenennen ändert nichts am Körper; Löschen entfernt ihn.
    d.rename(e, "Ellipsenzylinder").unwrap();
    assert_eq!(d.visible_bodies()[0].name, "Ellipsenzylinder");
    assert!(d.recompute().computed.is_empty());
    d.delete(e).unwrap();
    assert!(d.visible_bodies().is_empty());
    d.check_invariants().unwrap();
}

/// Achse `x = 0` (Konstruktionslinie) und ein Profil mit Spline als Außenkontur.
fn spline_revolve_sketch() -> (Sketch, CurveId, CurveId) {
    let mut sk = Sketch::new(Frame::WORLD, "Skizze", Tolerance::default());
    let ap = sk.add_point(0.0, -2.0).unwrap();
    let aq = sk.add_point(0.0, 8.0).unwrap();
    let axis = sk.add_line(ap, aq).unwrap();
    sk.set_construction(axis, true).unwrap();
    let p: Vec<_> = [
        (0.0, 0.0),
        (0.0, 5.0),
        (2.0, 5.0),
        (3.0, 3.0),
        (2.5, 1.0),
        (2.0, 0.0),
    ]
    .iter()
    .map(|&(x, y)| sk.add_point(x, y).unwrap())
    .collect();
    sk.add_line(p[0], p[1]).unwrap();
    sk.add_line(p[1], p[2]).unwrap();
    let sp = sk.add_spline(&p[2..6]).unwrap();
    sk.add_line(p[5], p[0]).unwrap();
    (sk, axis, sp)
}

#[test]
fn a_spline_sketch_revolves() {
    let (sk, axis, sp) = spline_revolve_sketch();
    // Referenz: V = π ∫ x² dy längs des Splines (die übrigen Kanten tragen nichts bei).
    let n = 12000;
    let pts: Vec<DVec2> = (0..=n)
        .map(|i| sk.spline_point(sp, 3.0 * i as f64 / n as f64).unwrap())
        .collect();
    let want = PI
        * pts
            .windows(2)
            .map(|w| {
                let (x0, x1) = (w[0].x, w[1].x);
                (x0 * x0 + x0 * x1 + x1 * x1) / 3.0 * (w[0].y - w[1].y)
            })
            .sum::<f64>();

    let mut d = Document::new();
    let s = d.add_feature("Skizze 1", feature_sketch(sk)).unwrap();
    let r = d
        .add_feature(
            "Rotation 1",
            FeatureKind::Revolve {
                sketch: s,
                profile: 0,
                axis,
            },
        )
        .unwrap();
    assert_eq!(
        d.feature(r).unwrap().status,
        FeatureStatus::Ok,
        "{:?}",
        d.feature(r).unwrap().status
    );
    let solid = d.visible_bodies()[0].solid;
    assert!(
        solid
            .faces
            .iter()
            .any(|(_, f)| f.surface.kind() == SurfaceKind::Nurbs)
    );
    assert_rel(volume(solid), want, "Spline-Drehkörper");
    assert!(d.recompute().computed.is_empty(), "Zwischenspeicher");
    d.check_invariants().unwrap();
}

#[test]
fn a_spline_sketch_extrudes_and_a_bad_spline_reports_an_error() {
    let mut sk = Sketch::new(Frame::WORLD, "Skizze", Tolerance::default());
    let p: Vec<_> = [(0.0, 0.0), (10.0, 0.0), (10.0, 5.0), (5.0, 7.0), (0.0, 5.0)]
        .iter()
        .map(|&(x, y)| sk.add_point(x, y).unwrap())
        .collect();
    sk.add_line(p[0], p[1]).unwrap();
    sk.add_line(p[1], p[2]).unwrap();
    sk.add_spline(&p[2..5]).unwrap();
    sk.add_line(p[4], p[0]).unwrap();
    let mid = p[3];
    let mut d = Document::new();
    let s = d.add_feature("Skizze 1", feature_sketch(sk)).unwrap();
    let e = d
        .add_feature(
            "Extrusion 1",
            FeatureKind::Extrude {
                sketch: s,
                profile: 0,
                height: 2.0,
            },
        )
        .unwrap();
    assert_eq!(d.feature(e).unwrap().status, FeatureStatus::Ok);

    // Der mittlere Punkt wandert unter die Grundlinie: Das Profil schneidet sich, es gibt
    // kein Profil 0 mehr, die Extrusion trägt einen Fehler statt eines Absturzes.
    d.update(s, |k| {
        if let FeatureKind::Sketch(sf) = k {
            sf.sketch.add_constraint(K::Fixed(mid, 5.0, -3.0)).unwrap();
        }
    })
    .unwrap();
    assert!(matches!(
        d.feature(e).unwrap().status,
        FeatureStatus::Error(_)
    ));
    assert!(d.visible_bodies().is_empty());
    d.check_invariants().unwrap();
}

/// Bewusst hingenommene Folge (Spec §13 „Alte Dokumente“, `offene-punkte.md`): Extrusionen
/// verweisen über die Profilnummer, die Profile sind nach der kleinsten Kurven-Id sortiert. Vor
/// Task 7 bekam eine Ellipse keine Nummer; ein altes Dokument mit einer Ellipse, die vor dem
/// Rechteck gezeichnet wurde, extrudierte als „Profil 0“ das Rechteck. Jetzt ist Profil 0 die
/// Ellipse. Wer dieses Verhalten ändert (etwa mit dauerhaften Profilverweisen), passt diesen
/// Test bewusst an.
#[test]
fn old_documents_may_extrude_a_different_profile() {
    let mut sk = Sketch::new(Frame::WORLD, "Skizze", Tolerance::default());
    let c = sk.add_point(-20.0, 0.0).unwrap();
    let ellipse = sk.add_ellipse(c, DVec2::new(4.0, 0.0), 2.0).unwrap();
    let rect = sk
        .add_rectangle(DVec2::new(0.0, 0.0), DVec2::new(10.0, 6.0))
        .unwrap();
    let ex = sk.profiles();
    assert_eq!(ex.profiles.len(), 2);
    assert_eq!(ex.curve_loops[0], vec![ellipse], "Profil 0 ist die Ellipse");
    assert!(rect.lines.iter().all(|l| ex.curve_loops[1].contains(l)));

    let mut d = Document::new();
    let s = d.add_feature("Skizze 1", feature_sketch(sk)).unwrap();
    let e = d
        .add_feature(
            "Extrusion 1",
            FeatureKind::Extrude {
                sketch: s,
                profile: 0,
                height: 3.0,
            },
        )
        .unwrap();
    assert_eq!(d.feature(e).unwrap().status, FeatureStatus::Ok);
    let solid = d.visible_bodies()[0].solid;
    assert!(
        solid
            .faces
            .iter()
            .any(|(_, f)| f.surface.kind() == SurfaceKind::Nurbs),
        "extrudiert wird die Ellipse, nicht mehr das Rechteck"
    );
    assert_rel(volume(solid), PI * 4.0 * 2.0 * 3.0, "Ellipsenzylinder");
}

/// Ellipsen-Skizze mit beliebigen Halbachsen (beide bemaßt).
fn ellipse_sketch_ab(a: f64, b: f64) -> (Sketch, ConstraintId) {
    let mut sk = Sketch::new(Frame::WORLD, "Skizze", Tolerance::default());
    let c = sk.add_point(0.0, 0.0).unwrap();
    let e = sk.add_ellipse(c, DVec2::new(a, 0.0), b).unwrap();
    sk.add_constraint(K::Fixed(c, 0.0, 0.0)).unwrap();
    sk.add_constraint(K::EllipseAngle(e, 0.0)).unwrap();
    let major = sk.add_constraint(K::EllipseMajor(e, a)).unwrap();
    sk.add_constraint(K::EllipseMinor(e, b)).unwrap();
    (sk, major)
}

fn body(feature: FeatureId) -> BodyRef {
    BodyRef { feature, index: 0 }
}

/// Ellipsenzylinder (a = 5, b = 3, Höhe 4) und ein Quader, dessen Fläche bei `x = 5 - depth`
/// liegt und sich nach +x erstreckt (Tiefe, mit der er in den Zylinder ragt).
fn ellipse_and_box(depth: f64) -> (Document, FeatureId, FeatureId, FeatureId, ConstraintId) {
    let mut d = Document::new();
    let (sk, major) = ellipse_sketch_ab(5.0, 3.0);
    let s = d.add_feature("Skizze 1", feature_sketch(sk)).unwrap();
    let e = d
        .add_feature(
            "Extrusion 1",
            FeatureKind::Extrude {
                sketch: s,
                profile: 0,
                height: 4.0,
            },
        )
        .unwrap();
    let b = d
        .add_feature(
            "Quader 1",
            FeatureKind::Box {
                size: DVec3::new(10.0, 10.0, 10.0),
                origin: DVec3::new(5.0 - depth, -5.0, -3.0),
            },
        )
        .unwrap();
    (d, s, e, b, major)
}

#[test]
fn a_boolean_with_a_freeform_body_follows_the_sketch_dimension() {
    let (mut d, s, e, b, major) = ellipse_and_box(1.0);
    let bo = d
        .add_feature(
            "Abzug 1",
            FeatureKind::Boolean {
                target: body(e),
                tool: body(b),
                op: BoolOp::Difference,
            },
        )
        .unwrap();
    let mv = d
        .add_feature(
            "Verschieben 1",
            FeatureKind::Move {
                body: body(bo),
                delta: DVec3::new(2.0, 0.0, 0.0),
            },
        )
        .unwrap();
    for id in [s, e, b, bo, mv] {
        assert_eq!(
            d.feature(id).unwrap().status,
            FeatureStatus::Ok,
            "{:?}",
            d.feature(id).unwrap().status
        );
    }
    d.check_invariants().unwrap();
    let vis = d.visible_bodies();
    assert_eq!(vis.len(), 1);
    d.kernel().validate(vis[0].solid).unwrap();
    // Ellipse (a = 5, b = 3) bis x = 4 abgeschnitten: Segmentfläche der Ellipse.
    let seg = |a: f64, b: f64, x: f64| {
        let t = (x / a).acos();
        a * b * (t - t.sin() * t.cos())
    };
    let ellipse_area = PI * 5.0 * 3.0;
    let cut_area = seg(5.0, 3.0, 4.0);
    let before = volume(vis[0].solid);
    assert_rel(before, (ellipse_area - cut_area) * 4.0, "Abzug");

    // Das Skizzen-Maß ändern (a = 6): Skizze, Extrusion, Abzug und Verschieben rechnen neu,
    // der Quader bleibt im Zwischenspeicher.
    let report = d
        .update_with_report(s, |k| {
            if let FeatureKind::Sketch(sf) = k {
                sf.sketch.set_constraint_value(major, 6.0).unwrap();
            }
        })
        .unwrap();
    for id in [s, e, bo, mv] {
        assert!(report.computed.contains(&id), "{id:?} neu berechnet");
    }
    assert!(!report.computed.contains(&b), "Quader nicht neu berechnet");
    assert!(report.reused.contains(&b));
    for id in [s, e, bo, mv] {
        assert_eq!(d.feature(id).unwrap().status, FeatureStatus::Ok);
    }
    let after = volume(d.visible_bodies()[0].solid);
    assert_rel(
        after,
        (PI * 6.0 * 3.0 - seg(6.0, 3.0, 4.0)) * 4.0,
        "Abzug nach der Maßänderung",
    );
    assert!(after > before * 1.1);
    d.check_invariants().unwrap();
}

/// Eine Lage, die der Rechner benannt ablehnt (Spec §13): Die Quaderfläche liegt 1e-3 mm
/// innerhalb der Mantelspitze der Ellipse und schneidet den Mantel fast tangential.
#[test]
fn an_unsupported_freeform_boolean_reports_an_error_and_skips_dependents() {
    let (mut d, _s, e, b, _) = ellipse_and_box(1e-3);
    let bo = d
        .add_feature(
            "Abzug 1",
            FeatureKind::Boolean {
                target: body(e),
                tool: body(b),
                op: BoolOp::Difference,
            },
        )
        .unwrap();
    let mv = d
        .add_feature(
            "Verschieben 1",
            FeatureKind::Move {
                body: body(bo),
                delta: DVec3::X,
            },
        )
        .unwrap();
    match &d.feature(bo).unwrap().status {
        FeatureStatus::Error(m) => {
            assert!(m.contains("Freiformfläche"), "{m}");
        }
        s => panic!("Fehler erwartet: {s:?}"),
    }
    assert!(
        matches!(d.feature(mv).unwrap().status, FeatureStatus::Skipped(_)),
        "{:?}",
        d.feature(mv).unwrap().status
    );
    // Die Eingangskörper bleiben sichtbar (Ellipsenzylinder und Quader).
    assert_eq!(d.visible_bodies().len(), 2);
    d.check_invariants().unwrap();

    // Behoben: Die Lage wird eindeutig, alles rechnet wieder.
    d.update(b, |k| {
        if let FeatureKind::Box { origin, .. } = k {
            origin.x = 4.0;
        }
    })
    .unwrap();
    assert_eq!(d.feature(bo).unwrap().status, FeatureStatus::Ok);
    assert_eq!(d.feature(mv).unwrap().status, FeatureStatus::Ok);
    d.check_invariants().unwrap();
}

/// Ein leeres Ergebnis ist keine Grenze des Rechners und trägt den Freiform-Hinweis nicht.
#[test]
fn an_empty_freeform_result_keeps_its_plain_message() {
    let (mut d, _s, e, b, _) = ellipse_and_box(0.0);
    let bo = d
        .add_feature(
            "Schnitt 1",
            FeatureKind::Boolean {
                target: body(e),
                tool: body(b),
                op: BoolOp::Intersection,
            },
        )
        .unwrap();
    match &d.feature(bo).unwrap().status {
        FeatureStatus::Error(m) => {
            assert!(m.contains("leer"), "{m}");
            assert!(!m.contains("Freiformfläche"), "{m}");
        }
        s => panic!("{s:?}"),
    }
}
