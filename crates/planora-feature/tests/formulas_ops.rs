//! Formeln in Extrusion (Höhe), Verschieben (Δx, Δy, Δz) und Netz-Körper (Position x/y/z,
//! Maßstab; die Drehung bleibt ohne Formel, Plan P10) — Aufgabe 4.

use planora_exchange::{ExchangeMesh, MeshTransform};
use planora_feature::*;
use planora_kernel::Solid;
use planora_math::{DQuat, DVec2, DVec3, Frame, Tolerance};
use planora_sketch::model::Sketch;
use std::sync::Arc;

use FieldKey::{Hoehe, Massstab, X, Y, Z};
use ParamUnit::{Length, Number};

fn formulas(pairs: &[(FieldKey, &str)]) -> Formulas {
    pairs.iter().map(|(k, t)| (*k, t.to_string())).collect()
}

fn rect_sketch(w: f64, h: f64) -> SketchFeature {
    let mut sk = Sketch::new(Frame::WORLD, "Skizze", Tolerance::default());
    sk.add_rectangle(DVec2::ZERO, DVec2::new(w, h)).unwrap();
    SketchFeature {
        sketch: sk,
        plane: SketchPlane::Standard(StandardPlane::XY),
        copies: vec![],
        formulas: Default::default(),
    }
}

/// Tetraeder mit Kantenlänge 10 an den Achsen (wie `tests/mesh_bodies.rs`).
fn tetra() -> Arc<ExchangeMesh> {
    Arc::new(ExchangeMesh {
        positions: vec![
            [0.0, 0.0, 0.0],
            [10.0, 0.0, 0.0],
            [0.0, 10.0, 0.0],
            [0.0, 0.0, 10.0],
        ],
        indices: vec![0, 2, 1, 0, 1, 3, 0, 3, 2, 1, 2, 3],
    })
}

fn edit(d: &Document, id: ParamId, formula: &str) -> ParamEdit {
    let p = d.parameter(id).unwrap();
    ParamEdit {
        name: p.name.clone(),
        formula: formula.into(),
        unit: p.unit,
        comment: p.comment.clone(),
    }
}

fn status(d: &Document, id: FeatureId) -> FeatureStatus {
    d.feature(id).unwrap().status.clone()
}

fn hull(s: &Solid) -> (DVec3, DVec3) {
    let mut lo = DVec3::splat(f64::INFINITY);
    let mut hi = DVec3::splat(f64::NEG_INFINITY);
    for (_, v) in s.vertices.iter() {
        lo = lo.min(v.point);
        hi = hi.max(v.point);
    }
    (lo, hi)
}

fn hull_of(d: &Document, id: FeatureId) -> (DVec3, DVec3) {
    hull(
        d.body(BodyRef {
            feature: id,
            index: 0,
        })
        .expect("Körper vorhanden"),
    )
}

fn near(a: DVec3, b: DVec3) -> bool {
    (a - b).abs().max_element() <= 1e-9
}

fn formula_err<T: std::fmt::Debug>(r: Result<T, FeatureError>) -> String {
    match r {
        Err(FeatureError::Formula(m)) => m,
        other => panic!("Formula-Fehler erwartet, bekam {other:?}"),
    }
}

fn delta_of(d: &Document, id: FeatureId) -> DVec3 {
    match &d.feature(id).unwrap().kind {
        FeatureKind::Move { delta, .. } => *delta,
        k => panic!("kein Verschieben: {k:?}"),
    }
}

fn mesh_t(d: &Document, id: FeatureId) -> MeshTransform {
    d.feature(id).unwrap().kind.mesh_transform().unwrap()
}

/// Skizze 1 (Rechteck 10 × 10 auf XY) und Extrusion 1 mit Höhe „h*2“.
fn extrude_doc(h: &str) -> (Document, ParamId, FeatureId) {
    let mut d = Document::new();
    let p = d.add_parameter("h", h, Length, "").unwrap();
    let s = d
        .add_feature("Skizze 1", FeatureKind::Sketch(rect_sketch(10.0, 10.0)))
        .unwrap();
    let e = d
        .add_feature_with_formulas(
            "Extrusion 1",
            FeatureKind::Extrude {
                sketch: s,
                profile: 0,
                height: 1.0,
            },
            formulas(&[(Hoehe, "h*2")]),
        )
        .unwrap();
    (d, p, e)
}

#[test]
fn extrude_height_formula() {
    let (mut d, h, e) = extrude_doc("5");
    assert_eq!(status(&d, e), FeatureStatus::Ok);
    let (lo, hi) = hull_of(&d, e);
    assert!(near(lo, DVec3::ZERO), "{lo}");
    assert!(near(hi, DVec3::new(10.0, 10.0, 10.0)), "{hi}");
    d.check_invariants().unwrap();

    d.set_parameter(h, edit(&d, h, "0")).unwrap();
    assert_eq!(
        status(&d, e),
        FeatureStatus::Error("Höhe ergibt 0 mm, es muss größer als 0 sein".into())
    );
    assert!(
        d.body(BodyRef {
            feature: e,
            index: 0
        })
        .is_none()
    );
    d.check_invariants().unwrap();

    // Eine negative Höhe kennt die Extrusion auch ohne Formel nicht (Kernel: > Toleranz).
    d.set_parameter(h, edit(&d, h, "-2")).unwrap();
    assert_eq!(
        status(&d, e),
        FeatureStatus::Error("Höhe ergibt -4 mm, es muss größer als 0 sein".into())
    );

    // Grenze an der Toleranz (1e-6): genau die Toleranz ist ein Wertfehler, das Doppelte nicht.
    d.set_parameter(h, edit(&d, h, "0,0000005")).unwrap();
    assert_eq!(
        status(&d, e),
        FeatureStatus::Error("Höhe ergibt 0 mm, es muss größer als 0 sein".into())
    );
    d.set_parameter(h, edit(&d, h, "0,000001")).unwrap();
    assert_eq!(status(&d, e), FeatureStatus::Ok, "Höhe 2e-6");
    let (_, hi) = hull_of(&d, e);
    assert!((hi.z - 2e-6).abs() <= 1e-12, "{hi}");
    d.check_invariants().unwrap();

    d.set_parameter(h, edit(&d, h, "7")).unwrap();
    assert_eq!(status(&d, e), FeatureStatus::Ok);
    let (_, hi) = hull_of(&d, e);
    assert!((hi.z - 14.0).abs() <= 1e-9, "{hi}");
    d.check_invariants().unwrap();
}

#[test]
fn extrude_without_formula_keeps_old_validation() {
    // Formeln ändern nicht, was gültig ist: Höhe 0 ohne Formel scheitert ebenfalls.
    let mut d = Document::new();
    let s = d
        .add_feature("Skizze 1", FeatureKind::Sketch(rect_sketch(10.0, 10.0)))
        .unwrap();
    let e = d
        .add_feature(
            "Extrusion 1",
            FeatureKind::Extrude {
                sketch: s,
                profile: 0,
                height: 0.0,
            },
        )
        .unwrap();
    assert!(matches!(status(&d, e), FeatureStatus::Error(_)));
}

/// Quader 10³ im Ursprung und Verschieben 1 mit `{X: "a", Y: "-a", Z: "0"}`.
fn move_doc(a: &str) -> (Document, ParamId, FeatureId, FeatureId) {
    let mut d = Document::new();
    let p = d.add_parameter("a", a, Length, "").unwrap();
    let q = d
        .add_feature(
            "Quader 1",
            FeatureKind::Box {
                size: DVec3::splat(10.0),
                origin: DVec3::ZERO,
            },
        )
        .unwrap();
    let m = d
        .add_feature_with_formulas(
            "Verschieben 1",
            FeatureKind::Move {
                body: BodyRef {
                    feature: q,
                    index: 0,
                },
                delta: DVec3::ZERO,
            },
            formulas(&[(X, "a"), (Y, "-a"), (Z, "0")]),
        )
        .unwrap();
    (d, p, q, m)
}

#[test]
fn move_delta_formulas() {
    let (mut d, a, _q, m) = move_doc("15");
    assert_eq!(status(&d, m), FeatureStatus::Ok);
    assert_eq!(delta_of(&d, m), DVec3::new(15.0, -15.0, 0.0));
    let (lo, _) = hull_of(&d, m);
    assert!(near(lo, DVec3::new(15.0, -15.0, 0.0)), "{lo}");
    d.check_invariants().unwrap();

    d.set_parameter(a, edit(&d, a, "-5")).unwrap();
    assert_eq!(status(&d, m), FeatureStatus::Ok, "negatives Δ ist erlaubt");
    let (lo, _) = hull_of(&d, m);
    assert!(near(lo, DVec3::new(-5.0, 5.0, 0.0)), "{lo}");

    d.set_parameter(a, edit(&d, a, "0")).unwrap();
    assert_eq!(status(&d, m), FeatureStatus::Ok, "Δ = 0 ist erlaubt");
    assert_eq!(delta_of(&d, m), DVec3::ZERO);
    d.check_invariants().unwrap();

    assert_eq!(
        field_label(&d.feature(m).unwrap().kind, X),
        "Δx",
        "Bezeichnung im Verschieben"
    );
}

#[test]
fn move_plain_update_drops_only_the_changed_formula() {
    // Grundlage für P11: Eine einfache Änderung (wie das Erweitern beim Ziehen) löscht die
    // Formel des geänderten Feldes; die App legt darum bei Formeln ein neues Verschieben an.
    let (mut d, a, _q, m) = move_doc("15");
    d.update(m, |k| {
        if let FeatureKind::Move { delta, .. } = k {
            delta.x += 5.0;
        }
    })
    .unwrap();
    assert_eq!(
        d.feature(m).unwrap().formulas(),
        &formulas(&[(Y, "-a"), (Z, "0")])
    );
    assert_eq!(delta_of(&d, m), DVec3::new(20.0, -15.0, 0.0));
    d.set_parameter(a, edit(&d, a, "1")).unwrap();
    assert_eq!(delta_of(&d, m), DVec3::new(20.0, -1.0, 0.0));
    d.check_invariants().unwrap();
}

/// Netz-Körper 1 mit Drehung `rot` und `{X: "a", Massstab: "k"}`; a = 3 (Länge), k = 2 (Zahl).
fn mesh_doc(rot: DQuat) -> (Document, ParamId, ParamId, FeatureId) {
    let mut d = Document::new();
    let a = d.add_parameter("a", "3", Length, "").unwrap();
    let k = d.add_parameter("k", "2", Number, "").unwrap();
    let n = d
        .add_mesh_body(
            "Netz-Körper 1",
            tetra(),
            MeshTransform {
                offset: DVec3::new(0.0, 4.0, 5.0),
                rotation: rot,
                scale: 1.0,
            },
        )
        .unwrap();
    d.update_with_formulas(n, |_| {}, formulas(&[(X, "a"), (Massstab, "k")]))
        .unwrap();
    (d, a, k, n)
}

#[test]
fn mesh_offset_and_scale() {
    let rot = DQuat::from_rotation_z(0.5);
    let (mut d, a, k, n) = mesh_doc(rot);
    let t = mesh_t(&d, n);
    assert_eq!(t.offset, DVec3::new(3.0, 4.0, 5.0));
    assert_eq!(t.scale, 2.0);
    assert_eq!(t.rotation, rot, "Drehung unberührt");
    assert_eq!(status(&d, n), FeatureStatus::Ok);
    d.check_invariants().unwrap();

    // Negative Lage ist erlaubt.
    d.set_parameter(a, edit(&d, a, "-3")).unwrap();
    assert_eq!(mesh_t(&d, n).offset.x, -3.0);
    assert_eq!(status(&d, n), FeatureStatus::Ok);

    // k als Länge: abgelehnt, nichts geändert.
    let before = d.snapshot();
    let e0 = d.edit_count();
    let mut ek = edit(&d, k, "2");
    ek.unit = Length;
    assert_eq!(
        formula_err(d.set_parameter(k, ek)),
        "Netz-Körper 1, Maßstab: Hier wird eine Zahl erwartet, die Formel ergibt eine Länge."
    );
    assert!(d.snapshot().same_content(&before));
    assert_eq!(d.edit_count(), e0);
    // ebenso eine Länge direkt im Feld
    assert_eq!(
        formula_err(d.update_with_formulas(n, |_| {}, formulas(&[(Massstab, "a")]))),
        "Netz-Körper 1, Maßstab: Hier wird eine Zahl erwartet, die Formel ergibt eine Länge."
    );
    assert!(d.snapshot().same_content(&before));
    assert_eq!(d.edit_count(), e0);

    // k = 0: Wertfehler, ohne „mm“.
    d.set_parameter(k, edit(&d, k, "0")).unwrap();
    assert_eq!(
        status(&d, n),
        FeatureStatus::Error(
            "Maßstab ergibt 0, er muss zwischen 0,000001 und 1 000 000 liegen".into()
        )
    );
    assert_eq!(mesh_t(&d, n).scale, 0.0, "Wert steht trotzdem im Feld");
    d.check_invariants().unwrap();
    d.set_parameter(k, edit(&d, k, "-1")).unwrap();
    assert_eq!(
        status(&d, n),
        FeatureStatus::Error(
            "Maßstab ergibt -1, er muss zwischen 0,000001 und 1 000 000 liegen".into()
        )
    );
    d.set_parameter(k, edit(&d, k, "1000001")).unwrap();
    assert_eq!(
        status(&d, n),
        FeatureStatus::Error(
            "Maßstab ergibt 1000001, er muss zwischen 0,000001 und 1 000 000 liegen".into()
        )
    );

    // Der Wert in der Meldung ist nie gerundet (kein „0“ für 5e-7, kein „1000000“ über der
    // Grenze), sehr kleine und sehr große Beträge in Exponentschreibweise.
    for (text, shown) in [
        ("0,0000005", "5e-7"),
        ("0,0000001", "1e-7"),
        ("0,0000009999999", "9,999999e-7"),
        ("-0,0000001", "-1e-7"),
        ("1000000,0000001", "1000000,0000001"),
        ("1000000,5", "1000000,5"),
        ("1000000*1000000*1000000*100", "1e20"),
    ] {
        d.set_parameter(k, edit(&d, k, text)).unwrap();
        assert_eq!(
            status(&d, n),
            FeatureStatus::Error(format!(
                "Maßstab ergibt {shown}, er muss zwischen 0,000001 und 1 000 000 liegen"
            )),
            "{text}"
        );
    }

    // Die Grenzen selbst sind erlaubt (wie beim Eintippen).
    for (text, v) in [("0,000001", MIN_MESH_SCALE), ("1000000", MAX_MESH_SCALE)] {
        d.set_parameter(k, edit(&d, k, text)).unwrap();
        assert_eq!(status(&d, n), FeatureStatus::Ok, "{text}");
        assert_eq!(mesh_t(&d, n).scale, v);
    }
    assert_eq!(mesh_t(&d, n).rotation, rot, "Drehung bleibt");
    d.check_invariants().unwrap();
}

#[test]
fn mesh_position_y_z_follow() {
    let mut d = Document::new();
    let a = d.add_parameter("a", "7", Length, "").unwrap();
    let n = d
        .add_mesh_body("Netz-Körper 1", tetra(), MeshTransform::IDENTITY)
        .unwrap();
    d.update_with_formulas(n, |_| {}, formulas(&[(Y, "a"), (Z, "a*2")]))
        .unwrap();
    assert_eq!(mesh_t(&d, n).offset, DVec3::new(0.0, 7.0, 14.0));
    assert_eq!(mesh_t(&d, n).scale, 1.0);
    d.set_parameter(a, edit(&d, a, "-1")).unwrap();
    assert_eq!(mesh_t(&d, n).offset, DVec3::new(0.0, -1.0, -2.0));
    assert_eq!(d.visible_meshes()[0].transform, mesh_t(&d, n));
    d.check_invariants().unwrap();
}

#[test]
fn restore_brings_back_table_and_values() {
    let (mut d, h, e) = extrude_doc("5");
    let a = d.add_parameter("a", "15", Length, "").unwrap();
    let q = d
        .add_feature(
            "Quader 1",
            FeatureKind::Box {
                size: DVec3::splat(10.0),
                origin: DVec3::ZERO,
            },
        )
        .unwrap();
    let m = d
        .add_feature_with_formulas(
            "Verschieben 1",
            FeatureKind::Move {
                body: BodyRef {
                    feature: q,
                    index: 0,
                },
                delta: DVec3::ZERO,
            },
            formulas(&[(X, "a"), (Y, "-a")]),
        )
        .unwrap();
    let k = d.add_parameter("k", "2", Number, "").unwrap();
    let n = d
        .add_mesh_body("Netz-Körper 1", tetra(), MeshTransform::IDENTITY)
        .unwrap();
    d.update_with_formulas(n, |_| {}, formulas(&[(Z, "a"), (Massstab, "k")]))
        .unwrap();

    let bild = d.snapshot();
    let hull_e = hull_of(&d, e);
    let hull_m = hull_of(&d, m);
    let t = mesh_t(&d, n);
    assert_eq!(t.offset.z, 15.0);

    for (p, text) in [(a, "-8"), (h, "9"), (k, "3")] {
        d.set_parameter(p, edit(&d, p, text)).unwrap();
    }
    assert_eq!(delta_of(&d, m), DVec3::new(-8.0, 8.0, 0.0));
    assert_eq!(mesh_t(&d, n).scale, 3.0);

    assert_eq!(d.restore(bild.clone()), Ok(RestoreKind::Full));
    assert!(d.snapshot().same_content(&bild));
    assert_eq!(d.parameter_value(a), Some(15.0));
    assert_eq!(d.parameter_value(h), Some(5.0));
    assert_eq!(d.parameter_value(k), Some(2.0));
    assert_eq!(delta_of(&d, m), DVec3::new(15.0, -15.0, 0.0));
    assert_eq!(mesh_t(&d, n), t);
    let (lo, hi) = hull_of(&d, e);
    assert!(near(lo, hull_e.0) && near(hi, hull_e.1), "{lo} {hi}");
    let (lo, hi) = hull_of(&d, m);
    assert!(near(lo, hull_m.0) && near(hi, hull_m.1), "{lo} {hi}");
    assert_eq!(
        d.feature(m).unwrap().formulas(),
        &formulas(&[(X, "a"), (Y, "-a")])
    );
    d.check_invariants().unwrap();
}

#[test]
fn rename_remove_replace_cover_new_fields() {
    let (mut d, h, e) = extrude_doc("5");
    let q = d
        .add_feature(
            "Quader 1",
            FeatureKind::Box {
                size: DVec3::splat(10.0),
                origin: DVec3::ZERO,
            },
        )
        .unwrap();
    let m = d
        .add_feature_with_formulas(
            "Verschieben 1",
            FeatureKind::Move {
                body: BodyRef {
                    feature: q,
                    index: 0,
                },
                delta: DVec3::ZERO,
            },
            formulas(&[(Z, "h+1")]),
        )
        .unwrap();
    let n = d
        .add_mesh_body("Netz-Körper 1", tetra(), MeshTransform::IDENTITY)
        .unwrap();
    d.update_with_formulas(n, |_| {}, formulas(&[(X, "h"), (Z, "-h")]))
        .unwrap();
    assert_eq!(mesh_t(&d, n).offset, DVec3::new(5.0, 0.0, -5.0));

    // Löschen gesperrt, alle drei Arten genannt (Zeitleistenreihenfolge).
    match d.remove_parameter(h) {
        Err(FeatureError::ParameterInUse { name, users }) => {
            assert_eq!(name, "h");
            assert_eq!(users, ["Extrusion 1", "Verschieben 1", "Netz-Körper 1"]);
        }
        other => panic!("{other:?}"),
    }

    // Umbenennen schreibt alle Formeln um.
    let mut ed = edit(&d, h, "5");
    ed.name = "Hoch".into();
    d.set_parameter(h, ed).unwrap();
    assert_eq!(d.feature(e).unwrap().formulas()[&Hoehe], "Hoch*2");
    assert_eq!(d.feature(m).unwrap().formulas()[&Z], "Hoch+1");
    assert_eq!(
        d.feature(n).unwrap().formulas(),
        &formulas(&[(X, "Hoch"), (Z, "-Hoch")])
    );

    // Durch den Wert ersetzen: Formeln entfallen, Werte bleiben.
    let e0 = d.edit_count();
    d.replace_parameter_by_value(h).unwrap();
    assert_eq!(d.edit_count(), e0 + 1);
    assert!(d.parameters().is_empty());
    for id in [e, m, n] {
        assert!(d.feature(id).unwrap().formulas().is_empty(), "{id:?}");
        assert_eq!(status(&d, id), FeatureStatus::Ok);
    }
    assert_eq!(delta_of(&d, m).z, 6.0);
    assert_eq!(mesh_t(&d, n).offset, DVec3::new(5.0, 0.0, -5.0));
    let (_, hi) = hull_of(&d, e);
    assert!((hi.z - 10.0).abs() <= 1e-9);
    d.check_invariants().unwrap();
}

#[test]
fn keys_and_labels() {
    let ext = FeatureKind::Extrude {
        sketch: FeatureId(1),
        profile: 0,
        height: 1.0,
    };
    let mv = FeatureKind::Move {
        body: BodyRef {
            feature: FeatureId(1),
            index: 0,
        },
        delta: DVec3::ZERO,
    };
    let mesh = FeatureKind::MeshBody {
        mesh: MeshId(0),
        offset: DVec3::ZERO,
        rotation: DQuat::IDENTITY,
        scale: 1.0,
    };
    assert_eq!(field_keys(&ext), [Hoehe]);
    assert_eq!(field_keys(&mv), [X, Y, Z]);
    assert_eq!(field_keys(&mesh), [X, Y, Z, Massstab]);
    let labels = |k: &FeatureKind| -> Vec<String> {
        field_keys(k)
            .into_iter()
            .map(|f| field_label(k, f))
            .collect()
    };
    assert_eq!(labels(&ext), ["Höhe"]);
    assert_eq!(labels(&mv), ["Δx", "Δy", "Δz"]);
    assert_eq!(
        labels(&mesh),
        ["Position x", "Position y", "Position z", "Maßstab"]
    );
    for k in [&ext, &mv] {
        for f in field_keys(k) {
            assert_eq!(field_unit(k, f), Length);
        }
    }
    for f in [X, Y, Z] {
        assert_eq!(field_unit(&mesh, f), Length);
    }
    assert_eq!(field_unit(&mesh, Massstab), Number);
}

#[test]
fn every_new_field_maps_to_its_own_field() {
    let mut ext = FeatureKind::Extrude {
        sketch: FeatureId(1),
        profile: 0,
        height: 1.0,
    };
    assert!(set_field_value(&mut ext, Hoehe, 4.0));
    assert!(!set_field_value(&mut ext, X, 4.0));
    assert_eq!(field_value(&ext, Hoehe), Some(4.0));
    assert!(matches!(ext, FeatureKind::Extrude { height, .. } if height == 4.0));

    let mut mv = FeatureKind::Move {
        body: BodyRef {
            feature: FeatureId(1),
            index: 0,
        },
        delta: DVec3::ZERO,
    };
    for (k, v) in [(X, 1.0), (Y, 2.0), (Z, 3.0)] {
        assert!(set_field_value(&mut mv, k, v));
    }
    assert!(!set_field_value(&mut mv, Hoehe, 9.0));
    assert!(matches!(mv, FeatureKind::Move { delta, .. } if delta == DVec3::new(1.0, 2.0, 3.0)));

    let rot = DQuat::from_rotation_x(0.25);
    let mut mesh = FeatureKind::MeshBody {
        mesh: MeshId(0),
        offset: DVec3::ZERO,
        rotation: rot,
        scale: 1.0,
    };
    for (k, v) in [(X, 1.0), (Y, 2.0), (Z, 3.0), (Massstab, 4.0)] {
        assert!(set_field_value(&mut mesh, k, v));
        assert_eq!(field_value(&mesh, k), Some(v));
    }
    assert!(!set_field_value(&mut mesh, Hoehe, 9.0));
    assert_eq!(
        mesh.mesh_transform(),
        Some(MeshTransform {
            offset: DVec3::new(1.0, 2.0, 3.0),
            rotation: rot,
            scale: 4.0,
        })
    );
}
