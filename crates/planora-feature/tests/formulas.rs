//! Formeln in Körper-Features (Aufgabe 3): Nebentabelle, Auswertungsdurchgang vor der
//! Neuberechnung, strenge Prüfung vor dem Übernehmen, Fehlerstatus bei scheiterndem Modell,
//! Umbenennen/Löschen/Ersetzen von Parametern samt Feature-Formeln, Zustandsbild.

use planora_feature::*;
use planora_kernel::{Solid, Surface};
use planora_math::DVec3;
use planora_mesh::{TessellationOptions, mass_properties, tessellate};

use FieldKey::{Breite, Hoehe, Radius, Tiefe, Winkel, X, Y, Z};
use ParamUnit::{Angle, Length};

fn formulas(pairs: &[(FieldKey, &str)]) -> Formulas {
    pairs.iter().map(|(k, t)| (*k, t.to_string())).collect()
}

fn unit_box() -> FeatureKind {
    FeatureKind::Box {
        size: DVec3::ONE,
        origin: DVec3::ZERO,
    }
}

/// Parameter Breite = 40, Tiefe = „Breite/2“.
fn doc_with_params() -> (Document, ParamId, ParamId) {
    let mut d = Document::new();
    let b = d.add_parameter("Breite", "40", Length, "").unwrap();
    let t = d.add_parameter("Tiefe", "Breite/2", Length, "").unwrap();
    (d, b, t)
}

/// Quader 1 mit den Formeln aus Test (a).
fn box_doc() -> (Document, ParamId, ParamId, FeatureId) {
    let (mut d, b, t) = doc_with_params();
    let q = d
        .add_feature_with_formulas(
            "Quader 1",
            unit_box(),
            formulas(&[
                (Breite, "Breite"),
                (Tiefe, "Tiefe"),
                (Hoehe, "Breite/4"),
                (X, "-Breite/2"),
            ]),
        )
        .unwrap();
    (d, b, t, q)
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

fn box_of(d: &Document, id: FeatureId) -> (DVec3, DVec3) {
    match &d.feature(id).unwrap().kind {
        FeatureKind::Box { size, origin } => (*size, *origin),
        k => panic!("kein Quader: {k:?}"),
    }
}

/// Hüllbox aus Ecken und Kugelflächen (genau, ohne Tessellierung).
fn hull(s: &Solid) -> (DVec3, DVec3) {
    let mut lo = DVec3::splat(f64::INFINITY);
    let mut hi = DVec3::splat(f64::NEG_INFINITY);
    for (_, v) in s.vertices.iter() {
        lo = lo.min(v.point);
        hi = hi.max(v.point);
    }
    for (_, f) in s.faces.iter() {
        if let Surface::Sphere { frame, radius } = &f.surface {
            lo = lo.min(frame.origin - DVec3::splat(*radius));
            hi = hi.max(frame.origin + DVec3::splat(*radius));
        }
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

fn near(a: DVec3, b: DVec3, eps: f64) -> bool {
    (a - b).abs().max_element() <= eps
}

fn formula_err<T: std::fmt::Debug>(r: Result<T, FeatureError>) -> String {
    match r {
        Err(FeatureError::Formula(m)) => m,
        other => panic!("Formula-Fehler erwartet, bekam {other:?}"),
    }
}

#[test]
fn box_follows_parameter() {
    let (mut d, b, _t, q) = box_doc();
    d.check_invariants().unwrap();
    let (size, origin) = box_of(&d, q);
    assert_eq!(size, DVec3::new(40.0, 20.0, 10.0));
    assert_eq!(origin, DVec3::new(-20.0, 0.0, 0.0));
    assert_eq!(d.feature(q).unwrap().status, FeatureStatus::Ok);
    let (lo, hi) = hull_of(&d, q);
    assert!(
        near(hi - lo, DVec3::new(40.0, 20.0, 10.0), 1e-9),
        "{lo} {hi}"
    );
    assert!(near(lo, DVec3::new(-20.0, 0.0, 0.0), 1e-9), "{lo}");
    assert!(
        (volume(d.visible_bodies()[0].solid) - 8000.0).abs() < 1e-6,
        "Rauminhalt"
    );
    assert_eq!(
        d.feature(q).unwrap().formulas(),
        &formulas(&[
            (Breite, "Breite"),
            (Tiefe, "Tiefe"),
            (Hoehe, "Breite/4"),
            (X, "-Breite/2"),
        ])
    );

    let (e0, v0) = (d.edit_count(), d.feature(q).unwrap().version());
    let report = d.set_parameter(b, edit(&d, b, "60")).unwrap();
    assert_eq!(d.edit_count(), e0 + 1);
    assert_eq!(report.computed, vec![q], "Quader neu berechnet");
    assert!(d.feature(q).unwrap().version() > v0, "neue Version");
    let (size, origin) = box_of(&d, q);
    assert_eq!(size, DVec3::new(60.0, 30.0, 15.0));
    assert_eq!(origin, DVec3::new(-30.0, 0.0, 0.0));
    let (lo, hi) = hull_of(&d, q);
    assert!(
        near(hi - lo, DVec3::new(60.0, 30.0, 15.0), 1e-9),
        "{lo} {hi}"
    );
    assert!(near(lo, DVec3::new(-30.0, 0.0, 0.0), 1e-9), "{lo}");
    assert!(
        (volume(d.visible_bodies()[0].solid) - 27000.0).abs() < 1e-6,
        "Rauminhalt folgt dem Parameter"
    );
    d.check_invariants().unwrap();

    // Ein Formelbezug über einen anderen Parameter (Tiefe = Breite/2) folgt ebenfalls, und der
    // Zwischenspeicher liefert danach nichts Altes.
    d.set_parameter(b, edit(&d, b, "40")).unwrap();
    assert_eq!(box_of(&d, q).0, DVec3::new(40.0, 20.0, 10.0));
    let (lo, hi) = hull_of(&d, q);
    assert!(
        near(hi - lo, DVec3::new(40.0, 20.0, 10.0), 1e-9),
        "{lo} {hi}"
    );
    d.check_invariants().unwrap();
}

#[test]
fn cylinder_cone_sphere_follow() {
    let mut d = Document::new();
    let p = d.add_parameter("d", "10", Length, "").unwrap();
    let zyl = d
        .add_feature_with_formulas(
            "Zylinder 1",
            FeatureKind::Cylinder {
                radius: 1.0,
                height: 1.0,
                origin: DVec3::ZERO,
            },
            formulas(&[(Radius, "d/2"), (Hoehe, "d")]),
        )
        .unwrap();
    let kegel = d
        .add_feature_with_formulas(
            "Kegel 1",
            FeatureKind::Cone {
                radius: 3.0,
                height: 1.0,
                origin: DVec3::new(50.0, 0.0, 0.0),
            },
            formulas(&[(Hoehe, "2*d")]),
        )
        .unwrap();
    let kugel = d
        .add_feature_with_formulas(
            "Kugel 1",
            FeatureKind::Sphere {
                radius: 1.0,
                origin: DVec3::new(0.0, 100.0, 0.0),
            },
            formulas(&[(Radius, "d"), (Z, "d*3")]),
        )
        .unwrap();
    d.check_invariants().unwrap();
    match d.feature(zyl).unwrap().kind {
        FeatureKind::Cylinder { radius, height, .. } => assert_eq!((radius, height), (5.0, 10.0)),
        ref k => panic!("{k:?}"),
    }
    match d.feature(kegel).unwrap().kind {
        FeatureKind::Cone { radius, height, .. } => assert_eq!((radius, height), (3.0, 20.0)),
        ref k => panic!("{k:?}"),
    }
    let (lo, hi) = hull_of(&d, kugel);
    assert!(near(hi - lo, DVec3::splat(20.0), 1e-6), "{lo} {hi}");
    assert!(near(lo, DVec3::new(-10.0, 90.0, 20.0), 1e-6), "{lo}");
    let (lo, hi) = hull_of(&d, kegel);
    assert!((hi.z - lo.z - 20.0).abs() < 1e-9, "Kegelhöhe {lo} {hi}");

    // Parameter ändern: alle drei folgen.
    d.set_parameter(p, edit(&d, p, "4")).unwrap();
    match d.feature(zyl).unwrap().kind {
        FeatureKind::Cylinder { radius, height, .. } => assert_eq!((radius, height), (2.0, 4.0)),
        ref k => panic!("{k:?}"),
    }
    let (lo, hi) = hull_of(&d, zyl);
    assert!((hi.z - lo.z - 4.0).abs() < 1e-9, "Zylinderhöhe {lo} {hi}");
    let (lo, hi) = hull_of(&d, kugel);
    assert!(near(hi - lo, DVec3::splat(8.0), 1e-6), "{lo} {hi}");
    assert!(near(lo, DVec3::new(-4.0, 96.0, 8.0), 1e-6), "{lo}");
    d.check_invariants().unwrap();

    let sphere = FeatureKind::Sphere {
        radius: 1.0,
        origin: DVec3::ZERO,
    };
    let cyl = FeatureKind::Cylinder {
        radius: 1.0,
        height: 1.0,
        origin: DVec3::ZERO,
    };
    let cone = FeatureKind::Cone {
        radius: 1.0,
        height: 1.0,
        origin: DVec3::ZERO,
    };
    assert_eq!(field_keys(&unit_box()), vec![Breite, Tiefe, Hoehe, X, Y, Z]);
    assert_eq!(field_keys(&cyl), vec![Radius, Hoehe, X, Y, Z]);
    assert_eq!(field_keys(&cone), vec![Radius, Hoehe, X, Y, Z]);
    assert_eq!(field_keys(&sphere), vec![Radius, X, Y, Z]);
    let labels: Vec<_> = field_keys(&unit_box())
        .into_iter()
        .map(|k| field_label(&unit_box(), k))
        .collect();
    assert_eq!(
        labels,
        [
            "Breite",
            "Tiefe",
            "Höhe",
            "Position x",
            "Position y",
            "Position z"
        ]
    );
    assert_eq!(field_label(&sphere, Radius), "Radius");
    for k in field_keys(&cyl) {
        assert_eq!(field_unit(&cyl, k), Length);
    }
    // Feldzugriff hin und zurück
    let mut k = unit_box();
    assert!(set_field_value(&mut k, Tiefe, 7.5));
    assert!(set_field_value(&mut k, Z, -2.0));
    assert_eq!(field_value(&k, Tiefe), Some(7.5));
    assert_eq!(field_value(&k, Z), Some(-2.0));
    assert_eq!(field_value(&k, Radius), None);
    assert!(!set_field_value(&mut k, Radius, 3.0));
    assert_eq!(field_value(&sphere, Hoehe), None);
    // Dateischlüssel
    for key in [
        Breite,
        Tiefe,
        Hoehe,
        Radius,
        X,
        Y,
        Z,
        FieldKey::Abstand,
        FieldKey::Abstand1,
        FieldKey::Abstand2,
        Winkel,
        FieldKey::Massstab,
    ] {
        assert_eq!(FieldKey::from_file_key(key.file_key()), Some(key));
    }
    assert_eq!(Hoehe.file_key(), "hoehe");
    assert_eq!(FieldKey::Massstab.file_key(), "massstab");
    assert_eq!(FieldKey::from_file_key("Breite"), None);
    // Arten ohne Formelfelder in dieser Aufgabe
    assert!(
        field_keys(&FeatureKind::Extrude {
            sketch: FeatureId(1),
            profile: 0,
            height: 1.0
        })
        .is_empty()
    );
}

#[test]
fn invalid_formula_rejected() {
    let (mut d, _b, _t, q) = box_doc();
    let w = d.add_parameter("w", "30", Angle, "").unwrap();
    let _ = w;
    let before = d.snapshot();
    let (e0, v0) = (d.edit_count(), d.feature(q).unwrap().version());
    let unchanged = |d: &Document| {
        assert!(d.snapshot().same_content(&before), "Dokument unverändert");
        assert_eq!(d.edit_count(), e0);
        assert_eq!(d.feature(q).unwrap().version(), v0);
        assert_eq!(box_of(d, q).0, DVec3::new(40.0, 20.0, 10.0));
        d.check_invariants().unwrap();
    };

    let err = formula_err(d.update_with_formulas(q, |_| {}, formulas(&[(Breite, "Breit")])));
    assert_eq!(err, "Quader 1, Breite: Den Namen „Breit“ gibt es nicht.");
    unchanged(&d);
    let err = formula_err(d.update_with_formulas(q, |_| {}, formulas(&[(Breite, "w")])));
    assert_eq!(
        err,
        "Quader 1, Breite: Hier wird eine Länge erwartet, die Formel ergibt einen Winkel."
    );
    unchanged(&d);
    // Höhe kaputt, Breite gültig: das kaputte Feld wird genannt
    let err = formula_err(d.update_with_formulas(
        q,
        |_| {},
        formulas(&[(Breite, "Breite"), (Hoehe, "1/0")]),
    ));
    assert_eq!(err, "Quader 1, Höhe: Division durch null.");
    unchanged(&d);
    // leere Formel
    let err = formula_err(d.update_with_formulas(q, |_| {}, formulas(&[(Y, "  ")])));
    assert_eq!(err, "Quader 1, Position y: Die Formel ist leer.");
    unchanged(&d);
    let r = d.update_with_formulas(q, |_| {}, formulas(&[(Winkel, "1")]));
    assert_eq!(
        r.clone().unwrap_err(),
        FeatureError::InvalidInput("Dieses Feld kann keine Formel haben".into())
    );
    assert_eq!(
        r.unwrap_err().to_string(),
        "Ungültige Eingabe: Dieses Feld kann keine Formel haben"
    );
    unchanged(&d);
    // Kugel hat keine Breite
    let r = d.add_feature_with_formulas(
        "Kugel 1",
        FeatureKind::Sphere {
            radius: 1.0,
            origin: DVec3::ZERO,
        },
        formulas(&[(Breite, "Breite")]),
    );
    assert_eq!(
        r.unwrap_err(),
        FeatureError::InvalidInput("Dieses Feld kann keine Formel haben".into())
    );
    unchanged(&d);
    // Anlegen mit kaputter Formel: kein neues Feature, keine Kennung verbraucht
    let r = d.add_feature_with_formulas("Quader 2", unit_box(), formulas(&[(Breite, "Breit")]));
    assert_eq!(
        formula_err(r),
        "Quader 2, Breite: Den Namen „Breit“ gibt es nicht."
    );
    unchanged(&d);
    // Wertfehler beim Anlegen ist keine Ablehnung (Fehlerstatus wie heute)
    let q2 = d
        .add_feature_with_formulas("Quader 2", unit_box(), formulas(&[(Tiefe, "Breite-40")]))
        .unwrap();
    assert_eq!(
        d.feature(q2).unwrap().status,
        FeatureStatus::Error("Tiefe ergibt 0 mm, es muss größer als 0 sein".into())
    );
    d.check_invariants().unwrap();
}

#[test]
fn bad_value_is_error_status() {
    let mut d = Document::new();
    let b = d.add_parameter("Breite", "50", Length, "").unwrap();
    let q = d
        .add_feature_with_formulas(
            "Quader 1",
            FeatureKind::Box {
                size: DVec3::new(1.0, 5.0, 5.0),
                origin: DVec3::ZERO,
            },
            formulas(&[(Breite, "Breite-40")]),
        )
        .unwrap();
    let m = d
        .add_feature(
            "Verschieben 1",
            FeatureKind::Move {
                body: BodyRef {
                    feature: q,
                    index: 0,
                },
                delta: DVec3::new(0.0, 0.0, 3.0),
            },
        )
        .unwrap();
    assert_eq!(box_of(&d, q).0.x, 10.0);
    assert_eq!(d.feature(q).unwrap().status, FeatureStatus::Ok);
    assert_eq!(d.feature(m).unwrap().status, FeatureStatus::Ok);
    d.check_invariants().unwrap();

    let e0 = d.edit_count();
    let v0 = d.feature(q).unwrap().version();
    d.set_parameter(b, edit(&d, b, "40")).unwrap();
    assert_eq!(d.edit_count(), e0 + 1);
    assert!(d.feature(q).unwrap().version() > v0);
    assert_eq!(
        d.feature(q).unwrap().status,
        FeatureStatus::Error("Breite ergibt 0 mm, es muss größer als 0 sein".into())
    );
    assert!(
        d.body(BodyRef {
            feature: q,
            index: 0
        })
        .is_none(),
        "kein Körper"
    );
    assert_eq!(
        d.feature(m).unwrap().status,
        FeatureStatus::Skipped("abhängig von Feature „Quader 1“ (Fehler)".into())
    );
    assert!(d.visible_bodies().is_empty());
    d.check_invariants().unwrap();
    // Unverändert neu rechnen: Fehler bleibt, nichts wird gerechnet
    let again = d.recompute();
    assert!(again.computed.is_empty(), "{:?}", again.computed);
    assert!(matches!(
        d.feature(q).unwrap().status,
        FeatureStatus::Error(_)
    ));

    d.set_parameter(b, edit(&d, b, "50")).unwrap();
    assert_eq!(d.feature(q).unwrap().status, FeatureStatus::Ok);
    assert_eq!(d.feature(m).unwrap().status, FeatureStatus::Ok);
    assert_eq!(box_of(&d, q).0.x, 10.0);
    let (lo, hi) = hull(d.visible_bodies()[0].solid);
    assert!(near(hi - lo, DVec3::new(10.0, 5.0, 5.0), 1e-9), "{lo} {hi}");
    d.check_invariants().unwrap();

    // Negatives Ergebnis ist ebenfalls ein Wertfehler (Meldung mit gerundetem Wert)
    d.update_with_formulas(q, |_| {}, formulas(&[(Breite, "Breite-52,5")]))
        .unwrap();
    assert_eq!(
        d.feature(q).unwrap().status,
        FeatureStatus::Error("Breite ergibt -2,5 mm, es muss größer als 0 sein".into())
    );
    d.check_invariants().unwrap();

    // Division durch null erst nach der Parameteränderung: abgelehnt, nichts ändert sich.
    // („100/(Breite-40)“ aus dem Plan ergibt Zahl durch Länge und passt nach P4 nicht in ein
    // Längenfeld; deshalb mit Länge im Zähler.)
    assert_eq!(
        formula_err(d.update_with_formulas(q, |_| {}, formulas(&[(Breite, "100/(Breite-40)")]))),
        "Quader 1, Breite: Hier wird eine Länge erwartet, die Formel ergibt eine andere Einheit."
    );
    d.update_with_formulas(q, |_| {}, formulas(&[(Breite, "Breite*10/(Breite-40)")]))
        .unwrap();
    assert_eq!(d.feature(q).unwrap().status, FeatureStatus::Ok);
    assert_eq!(box_of(&d, q).0.x, 50.0);
    let before = d.snapshot();
    let e1 = d.edit_count();
    let err = formula_err(d.set_parameter(b, edit(&d, b, "40")));
    assert_eq!(err, "Quader 1, Breite: Division durch null.");
    assert!(d.snapshot().same_content(&before));
    assert_eq!(d.edit_count(), e1);
    assert_eq!(d.parameter_value(b), Some(50.0));
    assert_eq!(d.parameter(b).unwrap().formula, "50");
    d.check_invariants().unwrap();
}

#[test]
fn plain_update_clears_only_changed_field() {
    let (mut d, b, _t, q) = box_doc();
    let e0 = d.edit_count();
    d.update(q, |k| {
        if let FeatureKind::Box { size, .. } = k {
            size.x = 7.0;
        }
    })
    .unwrap();
    assert_eq!(d.edit_count(), e0 + 1);
    // Breite fiel weg (geändert); Höhe und X blieben (Wert gleich), Tiefe auch.
    assert_eq!(
        d.feature(q).unwrap().formulas(),
        &formulas(&[(Tiefe, "Tiefe"), (Hoehe, "Breite/4"), (X, "-Breite/2")])
    );
    d.set_parameter(b, edit(&d, b, "80")).unwrap();
    let (size, origin) = box_of(&d, q);
    assert_eq!(size.x, 7.0);
    assert_eq!(size.y, 40.0);
    assert_eq!(size.z, 20.0);
    assert_eq!(origin.x, -40.0);
    d.check_invariants().unwrap();

    // Mehrere Felder auf einmal ändern: genau diese Formeln fallen weg.
    d.update(q, |k| {
        if let FeatureKind::Box { size, origin } = k {
            size.y = 3.0;
            origin.x = 1.0;
        }
    })
    .unwrap();
    assert_eq!(
        d.feature(q).unwrap().formulas(),
        &formulas(&[(Hoehe, "Breite/4")])
    );
    assert_eq!(box_of(&d, q).0, DVec3::new(7.0, 3.0, 20.0));
    d.check_invariants().unwrap();
}

#[test]
fn rename_users_replace_with_features() {
    let (mut d, b, t, q) = box_doc();
    let e0 = d.edit_count();
    let mut e = edit(&d, b, "40");
    e.name = "Weite".into();
    let report = d.set_parameter(b, e).unwrap();
    assert_eq!(d.edit_count(), e0 + 1);
    assert!(report.computed.is_empty(), "Umbenennen ändert keine Werte");
    assert_eq!(
        d.feature(q).unwrap().formulas(),
        &formulas(&[
            (Breite, "Weite"),
            (Tiefe, "Tiefe"),
            (Hoehe, "Weite/4"),
            (X, "-Weite/2"),
        ])
    );
    assert_eq!(d.parameter(t).unwrap().formula, "Weite/2");
    assert_eq!(
        d.parameter_users(b),
        vec!["Quader 1".to_string(), "Parameter Tiefe".to_string()]
    );
    assert_eq!(d.parameter_users(t), vec!["Quader 1".to_string()]);
    d.check_invariants().unwrap();

    let before = d.snapshot();
    let e1 = d.edit_count();
    let r = d.remove_parameter(b);
    assert_eq!(
        r.clone().unwrap_err().to_string(),
        "Weite wird in Quader 1, Parameter Tiefe benutzt."
    );
    assert_eq!(
        r.unwrap_err(),
        FeatureError::ParameterInUse {
            name: "Weite".into(),
            users: vec!["Quader 1".into(), "Parameter Tiefe".into()],
        }
    );
    assert!(d.snapshot().same_content(&before));
    assert_eq!(d.edit_count(), e1);
    // Tiefe ist nur im Quader benutzt
    assert_eq!(
        d.remove_parameter(t).unwrap_err().to_string(),
        "Tiefe wird in Quader 1 benutzt."
    );

    let (size, origin) = box_of(&d, q);
    let v0 = d.feature(q).unwrap().version();
    d.replace_parameter_by_value(b).unwrap();
    assert_eq!(d.edit_count(), e1 + 1, "ein Schritt");
    assert!(d.parameter(b).is_none());
    assert_eq!(
        d.feature(q).unwrap().formulas(),
        &formulas(&[(Tiefe, "Tiefe")])
    );
    assert_eq!(box_of(&d, q), (size, origin), "alle Werte gleich");
    assert_eq!(d.feature(q).unwrap().version(), v0, "nichts neu gerechnet");
    assert_eq!(d.parameter(t).unwrap().formula, "20");
    assert_eq!(d.parameter_value(t), Some(20.0));
    assert_eq!(d.feature(q).unwrap().status, FeatureStatus::Ok);
    d.check_invariants().unwrap();
}

#[test]
fn users_in_timeline_order_and_case_insensitive() {
    let (mut d, b, _t) = doc_with_params();
    let a = d
        .add_feature_with_formulas("Zweiter", unit_box(), formulas(&[(Z, "BREITE")]))
        .unwrap();
    // vor dem ersten einfügen: Marker zurück, anlegen, Marker wieder ans Ende
    d.set_marker(0).unwrap();
    d.add_feature_with_formulas("Erster", unit_box(), formulas(&[(Y, "breite*2")]))
        .unwrap();
    d.set_marker(2).unwrap();
    // ein Feature, das den Namen nur als Teil eines längeren Namens trägt, zählt nicht
    d.add_parameter("Breite2", "1", Length, "").unwrap();
    d.add_feature_with_formulas("Dritter", unit_box(), formulas(&[(X, "Breite2")]))
        .unwrap();
    assert_eq!(
        d.parameter_users(b),
        vec![
            "Erster".to_string(),
            "Zweiter".to_string(),
            "Parameter Tiefe".to_string()
        ]
    );
    assert_eq!(field_value(&d.feature(a).unwrap().kind, Z), Some(40.0));
    d.check_invariants().unwrap();
}

#[test]
fn no_value_change_no_recompute() {
    let (mut d, b, t, q) = box_doc();
    let frei = d.add_parameter("Frei", "3", Length, "").unwrap();
    let v0 = d.feature(q).unwrap().version();
    let mut e = edit(&d, b, "40");
    e.comment = "Außenmaß".into();
    let report = d.set_parameter(b, e).unwrap();
    assert!(report.computed.is_empty(), "{:?}", report.computed);
    assert_eq!(d.feature(q).unwrap().version(), v0);
    let report = d.set_parameter(frei, edit(&d, frei, "4")).unwrap();
    assert!(report.computed.is_empty(), "{:?}", report.computed);
    assert_eq!(d.feature(q).unwrap().version(), v0);
    // Gleicher Wert über eine andere Formel: auch nichts
    let report = d.set_parameter(t, edit(&d, t, "20")).unwrap();
    assert!(report.computed.is_empty(), "{:?}", report.computed);
    assert_eq!(d.feature(q).unwrap().version(), v0);
    assert_eq!(report.reused, vec![q]);
    d.check_invariants().unwrap();
}

#[test]
fn load_with_broken_formula() {
    let mut src = Document::new();
    src.add_parameter("Breite", "40", Length, "").unwrap();
    let other = src.add_parameter("Anders", "1", Length, "").unwrap();
    src.add_feature(
        "Quader 1",
        FeatureKind::Box {
            size: DVec3::new(12.0, 3.0, 4.0),
            origin: DVec3::ZERO,
        },
    )
    .unwrap();
    let mut s = src.snapshot();
    s.features[0].formulas = formulas(&[(Breite, "Gibtsnicht")]);
    let mut d = Document::from_snapshot(s, model::DEFAULT_CHORD_TOLERANCE).unwrap();
    let q = d.features()[0].id;
    assert_eq!(
        d.feature(q).unwrap().status,
        FeatureStatus::Error("Den Namen „Gibtsnicht“ gibt es nicht.".into())
    );
    assert_eq!(box_of(&d, q).0.x, 12.0);
    assert!(d.visible_bodies().is_empty());
    d.check_invariants().unwrap();

    // alter Fehler geduldet (P3)
    let e0 = d.edit_count();
    d.set_parameter(other, edit(&d, other, "2")).unwrap();
    assert_eq!(d.edit_count(), e0 + 1);
    // auch ein anderes Feld des Features lässt sich ändern, wenn die kaputte Formel bleibt
    d.update_with_formulas(
        q,
        |_| {},
        formulas(&[(Breite, "Gibtsnicht"), (Tiefe, "Breite/10")]),
    )
    .unwrap();
    assert_eq!(box_of(&d, q).0, DVec3::new(12.0, 4.0, 4.0));
    assert!(matches!(
        d.feature(q).unwrap().status,
        FeatureStatus::Error(_)
    ));
    d.check_invariants().unwrap();
    // eine andere kaputte Formel im selben Feld ist neu und wird abgelehnt
    let err =
        formula_err(d.update_with_formulas(q, |_| {}, formulas(&[(Breite, "Gibtsauchnicht")])));
    assert_eq!(
        err,
        "Quader 1, Breite: Den Namen „Gibtsauchnicht“ gibt es nicht."
    );

    d.update_with_formulas(q, |_| {}, formulas(&[(Breite, "Breite")]))
        .unwrap();
    assert_eq!(d.feature(q).unwrap().status, FeatureStatus::Ok);
    assert_eq!(box_of(&d, q).0.x, 40.0);
    d.check_invariants().unwrap();

    // Ungültiger Feldschlüssel im Bild: Laden abgelehnt
    let mut s = src.snapshot();
    s.features[0].formulas = formulas(&[(Radius, "1")]);
    let err = Document::from_snapshot(s, model::DEFAULT_CHORD_TOLERANCE).unwrap_err();
    assert!(err.to_string().contains("kann keine Formel haben"), "{err}");
}

#[test]
fn snapshot_carries_formulas() {
    let (mut d, _b, _t, q) = box_doc();
    let a = d.snapshot();
    assert_eq!(&a.features[0].formulas, d.feature(q).unwrap().formulas());
    let mut bild = a.clone();
    bild.features[0]
        .formulas
        .insert(Breite, "breite".to_string());
    assert!(!a.same_model(&bild));
    assert!(a.same_model(&a.clone()));
    let me0 = d.model_edit_count();
    assert_eq!(d.restore(bild.clone()), Ok(RestoreKind::Full));
    assert!(d.model_edit_count() > me0);
    assert!(d.snapshot().same_content(&bild));
    assert_eq!(d.feature(q).unwrap().formulas()[&Breite], "breite");
    d.check_invariants().unwrap();
    // und zurück
    assert_eq!(d.restore(a.clone()), Ok(RestoreKind::Full));
    assert!(d.snapshot().same_content(&a));
    // Bild nach Rückgängig einer Parameteränderung: Felder und Körper wieder wie vorher
    let b = d.parameters()[0].id;
    let before = d.snapshot();
    d.set_parameter(b, edit(&d, b, "60")).unwrap();
    assert_eq!(box_of(&d, q).0.x, 60.0);
    assert_eq!(d.restore(before.clone()), Ok(RestoreKind::Full));
    assert_eq!(box_of(&d, q).0, DVec3::new(40.0, 20.0, 10.0));
    let (lo, hi) = hull_of(&d, q);
    assert!(
        near(hi - lo, DVec3::new(40.0, 20.0, 10.0), 1e-9),
        "{lo} {hi}"
    );
    d.check_invariants().unwrap();
}

#[test]
fn update_with_formulas_replaces_table_and_bumps_once() {
    let (mut d, b, _t, q) = box_doc();
    let e0 = d.edit_count();
    let report = d
        .update_with_formulas(
            q,
            |k| {
                if let FeatureKind::Box { size, .. } = k {
                    size.z = 3.0;
                }
            },
            formulas(&[(Tiefe, "Breite/8")]),
        )
        .unwrap();
    assert_eq!(d.edit_count(), e0 + 1);
    assert_eq!(report.computed, vec![q]);
    assert_eq!(
        d.feature(q).unwrap().formulas(),
        &formulas(&[(Tiefe, "Breite/8")])
    );
    // Breite und X behalten ihren letzten Wert, Höhe die getippte Zahl
    assert_eq!(
        box_of(&d, q),
        (DVec3::new(40.0, 5.0, 3.0), DVec3::new(-20.0, 0.0, 0.0))
    );
    d.set_parameter(b, edit(&d, b, "80")).unwrap();
    assert_eq!(box_of(&d, q).0, DVec3::new(40.0, 10.0, 3.0));
    // Unbekanntes Feature
    assert_eq!(
        d.update_with_formulas(FeatureId(999), |_| {}, Formulas::new())
            .unwrap_err(),
        FeatureError::UnknownFeature
    );
    d.check_invariants().unwrap();
}

#[test]
fn failed_compute_follows_parameter() {
    // Ein Fehler aus der Berechnung (nicht aus der Formel) hängt am Schlüssel der Eingaben; ein
    // neuer Formelwert muss ihn ablösen, ein unveränderter ihn übernehmen.
    let mut d = Document::new();
    let b = d.add_parameter("Breite", "40", Length, "").unwrap();
    let frei = d.add_parameter("Frei", "1", Length, "").unwrap();
    let q = d
        .add_feature_with_formulas("Quader 1", unit_box(), formulas(&[(Breite, "Breite")]))
        .unwrap();
    d.set_parameter(b, edit(&d, b, "10^10")).unwrap();
    let FeatureStatus::Error(m) = d.feature(q).unwrap().status.clone() else {
        panic!("Fehler erwartet: {:?}", d.feature(q).unwrap().status)
    };
    assert!(m.contains("außerhalb"), "{m}");
    assert_eq!(box_of(&d, q).0.x, 1e10);
    d.check_invariants().unwrap();
    let report = d.set_parameter(frei, edit(&d, frei, "2")).unwrap();
    assert!(report.computed.is_empty(), "{:?}", report.computed);
    assert_eq!(report.reused, vec![q], "Fehler übernommen");
    d.set_parameter(b, edit(&d, b, "40")).unwrap();
    assert_eq!(d.feature(q).unwrap().status, FeatureStatus::Ok);
    let (lo, hi) = hull_of(&d, q);
    assert!(near(hi - lo, DVec3::new(40.0, 1.0, 1.0), 1e-9), "{lo} {hi}");
    d.set_parameter(b, edit(&d, b, "10^10")).unwrap();
    assert!(matches!(
        d.feature(q).unwrap().status,
        FeatureStatus::Error(_)
    ));
    d.check_invariants().unwrap();
}

#[test]
fn rename_tolerates_old_broken_formula() {
    let mut src = Document::new();
    let b = src.add_parameter("Breite", "40", Length, "").unwrap();
    src.add_feature("Quader 1", unit_box()).unwrap();
    let mut s = src.snapshot();
    s.features[0].formulas = formulas(&[(Breite, "Breite/0"), (Tiefe, "Breite/8")]);
    let mut d = Document::from_snapshot(s, model::DEFAULT_CHORD_TOLERANCE).unwrap();
    let q = d.features()[0].id;
    assert_eq!(
        d.feature(q).unwrap().status,
        FeatureStatus::Error("Division durch null.".into())
    );
    assert_eq!(box_of(&d, q).0, DVec3::new(1.0, 5.0, 1.0));
    d.check_invariants().unwrap();
    // Umbenennen schreibt auch die kaputte Formel um und wird nicht abgelehnt
    let mut e = edit(&d, b, "40");
    e.name = "Weite".into();
    d.set_parameter(b, e).unwrap();
    assert_eq!(
        d.feature(q).unwrap().formulas(),
        &formulas(&[(Breite, "Weite/0"), (Tiefe, "Weite/8")])
    );
    // Einen Wert ändern: der alte Fehler bleibt geduldet, die gültige Formel folgt
    d.set_parameter(b, edit(&d, b, "80")).unwrap();
    assert_eq!(box_of(&d, q).0, DVec3::new(1.0, 10.0, 1.0));
    assert!(matches!(
        d.feature(q).unwrap().status,
        FeatureStatus::Error(_)
    ));
    // Durch den Wert ersetzen entfernt beide Formeln; der Fehler verschwindet, das Feld behält
    // seinen letzten Wert
    d.replace_parameter_by_value(b).unwrap();
    assert!(d.feature(q).unwrap().formulas().is_empty());
    assert_eq!(box_of(&d, q).0, DVec3::new(1.0, 10.0, 1.0));
    assert_eq!(d.feature(q).unwrap().status, FeatureStatus::Ok);
    d.check_invariants().unwrap();
}
