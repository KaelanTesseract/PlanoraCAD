//! Formeln in Rundung (Radius) und Fasen (Abstand, Abstand 1/2, Winkel in Grad); die variable
//! Rundung bleibt ohne Formeln — Aufgabe 5.

use planora_feature::*;
use planora_kernel::blend::topo::{edge_uses, face_name};
use planora_kernel::{Edge, Id, Solid};
use planora_math::{DVec3, Tolerance};

use FieldKey::{Abstand, Abstand1, Abstand2, Radius, Winkel};
use ParamUnit::{Angle, Length};

fn formulas(pairs: &[(FieldKey, &str)]) -> Formulas {
    pairs.iter().map(|(k, t)| (*k, t.to_string())).collect()
}

fn edge_between(s: &Solid, a: &str, b: &str) -> Id<Edge> {
    s.edges
        .iter()
        .map(|(id, _)| id)
        .find(|&id| {
            let n: Vec<String> = edge_uses(s, id)
                .iter()
                .map(|u| face_name(s, u.face))
                .collect();
            n.len() == 2 && ((n[0] == a && n[1] == b) || (n[0] == b && n[1] == a))
        })
        .unwrap_or_else(|| panic!("keine Kante zwischen {a} und {b}"))
}

/// Quader 20³ und die Kante Deckel/Wand 1.
fn with_box() -> (Document, BodyRef, EdgeRef) {
    let mut d = Document::new();
    let f = d
        .add_feature(
            "Quader 1",
            FeatureKind::Box {
                size: DVec3::splat(20.0),
                origin: DVec3::ZERO,
            },
        )
        .unwrap();
    let b = BodyRef {
        feature: f,
        index: 0,
    };
    let s = d.body(b).unwrap();
    let e = EdgeRef::from_edge(
        s,
        edge_between(s, "Deckel", "Wand 1"),
        &Tolerance::default(),
    )
    .unwrap();
    (d, b, e)
}

fn blend(body: BodyRef, edge: &EdgeRef, art: BlendArt) -> FeatureKind {
    FeatureKind::Verrunden {
        body,
        edges: vec![edge.clone()],
        art,
        tangentenkette: true,
    }
}

fn art_of(d: &Document, id: FeatureId) -> BlendArt {
    match &d.feature(id).unwrap().kind {
        FeatureKind::Verrunden { art, .. } => art.clone(),
        k => panic!("keine Verrundung: {k:?}"),
    }
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

fn has_body(d: &Document, id: FeatureId) -> bool {
    d.body(BodyRef {
        feature: id,
        index: 0,
    })
    .is_some()
}

/// Eckpunkte des Körpers bitgenau, sortiert (der Kernel-Körper hat keinen Vergleich).
fn corners(d: &Document, id: FeatureId) -> Vec<[u64; 3]> {
    let s = d
        .body(BodyRef {
            feature: id,
            index: 0,
        })
        .expect("Körper vorhanden");
    let mut v: Vec<[u64; 3]> = s
        .vertices
        .iter()
        .map(|(_, v)| v.point.to_array().map(f64::to_bits))
        .collect();
    v.sort_unstable();
    v
}

fn formula_err<T: std::fmt::Debug>(r: Result<T, FeatureError>) -> String {
    match r {
        Err(FeatureError::Formula(m)) => m,
        other => panic!("Formula-Fehler erwartet, bekam {other:?}"),
    }
}

#[test]
fn chamfer_angle_limits_are_the_dialog_limits() {
    assert_eq!(MIN_CHAMFER_ANGLE_DEG, 1.0);
    assert_eq!(MAX_CHAMFER_ANGLE_DEG, 179.0);
}

#[test]
fn fillet_radius_follows() {
    let (mut d, b, e) = with_box();
    let r = d.add_parameter("r", "2", Length, "").unwrap();
    let f = d
        .add_feature_with_formulas(
            "Rundung 1",
            blend(b, &e, BlendArt::Rundung { radius: 9.0 }),
            formulas(&[(Radius, "r")]),
        )
        .unwrap();
    assert_eq!(status(&d, f), FeatureStatus::Ok);
    assert_eq!(art_of(&d, f), BlendArt::Rundung { radius: 2.0 });
    assert!(has_body(&d, f));
    d.check_invariants().unwrap();

    let report = d.set_parameter(r, edit(&d, r, "3")).unwrap();
    assert_eq!(status(&d, f), FeatureStatus::Ok);
    assert_eq!(art_of(&d, f), BlendArt::Rundung { radius: 3.0 });
    assert!(report.computed.contains(&f), "{:?}", report.computed);
    assert!(!report.computed.contains(&b.feature), "Quader bleibt");
    d.check_invariants().unwrap();

    d.set_parameter(r, edit(&d, r, "0")).unwrap();
    assert_eq!(
        status(&d, f),
        FeatureStatus::Error("Radius ergibt 0 mm, es muss größer als 0 sein".into())
    );
    assert!(!has_body(&d, f));
    d.check_invariants().unwrap();

    // Ein Fasenabstand ≤ 0 ebenso.
    let (mut d, b, e) = with_box();
    let a = d.add_parameter("a", "1", Length, "").unwrap();
    let f = d
        .add_feature_with_formulas(
            "Fase 1",
            blend(b, &e, BlendArt::FaseGleich { abstand: 5.0 }),
            formulas(&[(Abstand, "a")]),
        )
        .unwrap();
    assert_eq!(art_of(&d, f), BlendArt::FaseGleich { abstand: 1.0 });
    assert_eq!(status(&d, f), FeatureStatus::Ok);
    d.set_parameter(a, edit(&d, a, "-1")).unwrap();
    assert_eq!(
        status(&d, f),
        FeatureStatus::Error("Abstand ergibt -1 mm, es muss größer als 0 sein".into())
    );
}

#[test]
fn chamfer_kinds() {
    let (mut d, b, e) = with_box();
    d.add_parameter("a", "1", Length, "").unwrap();
    let f2 = d
        .add_feature_with_formulas(
            "Fase 1",
            blend(
                b,
                &e,
                BlendArt::FaseZwei {
                    abstand1: 7.0,
                    abstand2: 8.0,
                },
            ),
            formulas(&[(Abstand1, "a"), (Abstand2, "a*2")]),
        )
        .unwrap();
    assert_eq!(
        art_of(&d, f2),
        BlendArt::FaseZwei {
            abstand1: 1.0,
            abstand2: 2.0
        }
    );
    assert_eq!(status(&d, f2), FeatureStatus::Ok);
    assert_eq!(
        field_label(&d.feature(f2).unwrap().kind, Abstand1),
        "Abstand 1"
    );
    assert_eq!(
        field_label(&d.feature(f2).unwrap().kind, Abstand2),
        "Abstand 2"
    );

    // Ein zu kleiner zweiter Abstand nennt „Abstand 2“.
    d.update_with_formulas(f2, |_| {}, formulas(&[(Abstand1, "a"), (Abstand2, "a-1")]))
        .unwrap();
    assert_eq!(
        status(&d, f2),
        FeatureStatus::Error("Abstand 2 ergibt 0 mm, es muss größer als 0 sein".into())
    );

    let (mut d, b, e) = with_box();
    d.add_parameter("a", "1", Length, "").unwrap();
    let w = d.add_parameter("w", "45", Angle, "").unwrap();
    let fw = d
        .add_feature_with_formulas(
            "Fase 1",
            blend(
                b,
                &e,
                BlendArt::FaseWinkel {
                    abstand: 3.0,
                    winkel: 30.0,
                },
            ),
            formulas(&[(Abstand, "a"), (Winkel, "w")]),
        )
        .unwrap();
    assert_eq!(status(&d, fw), FeatureStatus::Ok);
    assert_eq!(
        art_of(&d, fw),
        BlendArt::FaseWinkel {
            abstand: 1.0,
            winkel: 45.0
        }
    );
    assert_eq!(field_unit(&d.feature(fw).unwrap().kind, Winkel), Angle);
    d.check_invariants().unwrap();

    // Grad bitgenau ohne Umrechnung: 1/3 Grad bleibt 1/3 (nicht über Bogenmaß gerundet).
    d.set_parameter(w, edit(&d, w, "100/3")).unwrap();
    match art_of(&d, fw) {
        BlendArt::FaseWinkel { winkel, .. } => {
            assert_eq!(winkel.to_bits(), (100.0f64 / 3.0).to_bits())
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(status(&d, fw), FeatureStatus::Ok);
    d.set_parameter(w, edit(&d, w, "45")).unwrap();

    // Eine Länge im Winkelfeld wird abgelehnt, nichts ändert sich.
    let before = d.snapshot();
    let edits = d.edit_count();
    assert_eq!(
        formula_err(d.update_with_formulas(fw, |_| {}, formulas(&[(Abstand, "a"), (Winkel, "a")]))),
        "Fase 1, Winkel: Hier wird ein Winkel erwartet, die Formel ergibt eine Länge."
    );
    assert!(d.snapshot().same_content(&before));
    assert_eq!(d.edit_count(), edits);
    // Ein Winkel im Abstandsfeld ebenso.
    assert!(matches!(
        d.update_with_formulas(fw, |_| {}, formulas(&[(Abstand, "w"), (Winkel, "w")])),
        Err(FeatureError::Formula(_))
    ));
    assert!(d.snapshot().same_content(&before));

    // Grenzen eingeschlossen: kein Wertfehler, sondern dasselbe Ergebnis wie eine fest
    // eingetragene Fase (bei 179° an dieser rechtwinkligen Kante lehnt der Kernel ab).
    for (ok, deg) in [("1", 1.0), ("179", 179.0)] {
        d.set_parameter(w, edit(&d, w, ok)).unwrap();
        let (mut plain, pb, pe) = with_box();
        let pf = plain
            .add_feature(
                "Fase 1",
                blend(
                    pb,
                    &pe,
                    BlendArt::FaseWinkel {
                        abstand: 1.0,
                        winkel: deg,
                    },
                ),
            )
            .unwrap();
        assert_eq!(status(&d, fw), status(&plain, pf), "w = {ok}");
        if let FeatureStatus::Error(m) = status(&d, fw) {
            assert!(!m.starts_with("Winkel ergibt"), "{m}");
        }
    }
    d.set_parameter(w, edit(&d, w, "1")).unwrap();
    assert_eq!(status(&d, fw), FeatureStatus::Ok);
    for (bad, text) in [
        ("0", "0"),
        ("200", "200"),
        ("180", "180"),
        ("0.999", "0,999"),
        ("179.004", "179,004"),
        ("-45", "-45"),
    ] {
        d.set_parameter(w, edit(&d, w, bad)).unwrap();
        assert_eq!(
            status(&d, fw),
            FeatureStatus::Error(format!(
                "Winkel ergibt {text}°, er muss zwischen 1° und 179° liegen"
            )),
            "w = {bad}"
        );
        assert!(!has_body(&d, fw));
        d.check_invariants().unwrap();
    }
    d.set_parameter(w, edit(&d, w, "45")).unwrap();
    assert_eq!(status(&d, fw), FeatureStatus::Ok);
}

#[test]
fn art_switch_drops_missing_keys() {
    let (mut d, b, e) = with_box();
    d.add_parameter("a", "1", Length, "").unwrap();
    d.add_parameter("w", "45", Angle, "").unwrap();
    let f2 = d
        .add_feature_with_formulas(
            "Fase 1",
            blend(
                b,
                &e,
                BlendArt::FaseZwei {
                    abstand1: 1.0,
                    abstand2: 1.0,
                },
            ),
            formulas(&[(Abstand1, "a"), (Abstand2, "a*2")]),
        )
        .unwrap();
    d.update(f2, |k| {
        if let FeatureKind::Verrunden { art, .. } = k {
            *art = BlendArt::FaseGleich { abstand: 1.0 };
        }
    })
    .unwrap();
    assert!(d.feature(f2).unwrap().formulas().is_empty());
    assert_eq!(art_of(&d, f2), BlendArt::FaseGleich { abstand: 1.0 });
    d.check_invariants().unwrap();
    d.delete(f2).unwrap();

    let fw = d
        .add_feature_with_formulas(
            "Fase 2",
            blend(
                b,
                &e,
                BlendArt::FaseWinkel {
                    abstand: 1.0,
                    winkel: 45.0,
                },
            ),
            formulas(&[(Abstand, "a"), (Winkel, "w")]),
        )
        .unwrap();
    d.update(fw, |k| {
        if let FeatureKind::Verrunden { art, .. } = k {
            *art = BlendArt::FaseGleich { abstand: 1.0 };
        }
    })
    .unwrap();
    assert_eq!(
        d.feature(fw).unwrap().formulas(),
        &formulas(&[(Abstand, "a")])
    );
    d.check_invariants().unwrap();

    // Rundung → Fase gleich: „Radius“ gibt es nicht mehr, „Abstand“ hatte keine Formel.
    let (mut d, b, e) = with_box();
    d.add_parameter("r", "2", Length, "").unwrap();
    let f = d
        .add_feature_with_formulas(
            "Rundung 1",
            blend(b, &e, BlendArt::Rundung { radius: 1.0 }),
            formulas(&[(Radius, "r")]),
        )
        .unwrap();
    d.update(f, |k| {
        if let FeatureKind::Verrunden { art, .. } = k {
            *art = BlendArt::FaseGleich { abstand: 2.0 };
        }
    })
    .unwrap();
    assert!(d.feature(f).unwrap().formulas().is_empty());
    assert_eq!(art_of(&d, f), BlendArt::FaseGleich { abstand: 2.0 });
    assert_eq!(status(&d, f), FeatureStatus::Ok);
}

#[test]
fn variable_fillet_has_no_fields() {
    let var = BlendArt::RundungVariabel {
        punkte: vec![(0.0, 1.0), (100.0, 2.0)],
    };
    let (mut d, b, e) = with_box();
    let kind = blend(b, &e, var.clone());
    assert_eq!(field_keys(&kind), Vec::<FieldKey>::new());
    for key in [Radius, Abstand, Abstand1, Abstand2, Winkel] {
        assert_eq!(field_value(&kind, key), None, "{key:?}");
    }
    d.add_parameter("r", "2", Length, "").unwrap();
    assert!(matches!(
        d.add_feature_with_formulas("Rundung 1", kind.clone(), formulas(&[(Radius, "r")])),
        Err(FeatureError::InvalidInput(_))
    ));
    let f = d.add_feature("Rundung 1", kind).unwrap();
    let before = d.snapshot();
    assert!(matches!(
        d.update_with_formulas(f, |_| {}, formulas(&[(Radius, "r")])),
        Err(FeatureError::InvalidInput(_))
    ));
    assert!(d.snapshot().same_content(&before));

    // Die übrigen Arten haben genau ihre Felder.
    let keys = |art: BlendArt| field_keys(&blend(b, &e, art));
    assert_eq!(keys(BlendArt::Rundung { radius: 1.0 }), vec![Radius]);
    assert_eq!(keys(BlendArt::FaseGleich { abstand: 1.0 }), vec![Abstand]);
    assert_eq!(
        keys(BlendArt::FaseZwei {
            abstand1: 1.0,
            abstand2: 2.0
        }),
        vec![Abstand1, Abstand2]
    );
    assert_eq!(
        keys(BlendArt::FaseWinkel {
            abstand: 1.0,
            winkel: 45.0
        }),
        vec![Abstand, Winkel]
    );
    // Lesen und Schreiben treffen das richtige Feld.
    let mut k = blend(
        b,
        &e,
        BlendArt::FaseZwei {
            abstand1: 1.0,
            abstand2: 2.0,
        },
    );
    assert_eq!(field_value(&k, Abstand1), Some(1.0));
    assert_eq!(field_value(&k, Abstand2), Some(2.0));
    assert_eq!(field_value(&k, Abstand), None);
    assert!(set_field_value(&mut k, Abstand2, 5.0));
    assert_eq!(field_value(&k, Abstand1), Some(1.0));
    assert_eq!(field_value(&k, Abstand2), Some(5.0));
    assert!(!set_field_value(&mut k, Radius, 5.0));
    let mut k = blend(
        b,
        &e,
        BlendArt::FaseWinkel {
            abstand: 1.0,
            winkel: 45.0,
        },
    );
    assert_eq!(field_value(&k, Winkel), Some(45.0));
    assert!(set_field_value(&mut k, Winkel, 60.0));
    assert_eq!(field_value(&k, Abstand), Some(1.0));
    assert_eq!(field_value(&k, Winkel), Some(60.0));
    assert_eq!(field_label(&k, Winkel), "Winkel");
    assert_eq!(field_unit(&k, Abstand), Length);
}

#[test]
fn parameter_breaks_blend_then_back() {
    let (mut d, b, e) = with_box();
    let r = d.add_parameter("r", "2", Length, "").unwrap();
    let f = d
        .add_feature_with_formulas(
            "Rundung 1",
            blend(b, &e, BlendArt::Rundung { radius: 2.0 }),
            formulas(&[(Radius, "r")]),
        )
        .unwrap();
    assert_eq!(status(&d, f), FeatureStatus::Ok);
    let ok_body = corners(&d, f);
    let bild = d.snapshot();

    d.set_parameter(r, edit(&d, r, "50")).unwrap();
    // Dieselbe Meldung wie eine fest eingetragene Rundung mit 50 mm (die des Kernels).
    let (mut plain, pb, pe) = with_box();
    let pf = plain
        .add_feature(
            "Rundung 1",
            blend(pb, &pe, BlendArt::Rundung { radius: 50.0 }),
        )
        .unwrap();
    let kernel_status = status(&plain, pf);
    assert!(
        matches!(kernel_status, FeatureStatus::Error(_)),
        "{kernel_status:?}"
    );
    assert_eq!(status(&d, f), kernel_status);
    assert_eq!(art_of(&d, f), BlendArt::Rundung { radius: 50.0 });
    assert!(!has_body(&d, f), "kein veralteter Körper");
    assert_eq!(status(&d, b.feature), FeatureStatus::Ok);
    d.check_invariants().unwrap();

    assert_eq!(d.restore(bild.clone()), Ok(RestoreKind::Full));
    assert_eq!(status(&d, f), FeatureStatus::Ok);
    assert_eq!(art_of(&d, f), BlendArt::Rundung { radius: 2.0 });
    assert_eq!(corners(&d, f), ok_body, "derselbe Körper wie vorher");
    assert!(d.snapshot().same_content(&bild));
    d.check_invariants().unwrap();

    // Ein Bild mit anderer Parameterformel (Feld noch auf dem alten Wert) wird beim
    // Zurückladen neu ausgewertet.
    let mut drei = bild.clone();
    drei.parameters[0].formula = "3".into();
    assert_eq!(d.restore(drei), Ok(RestoreKind::Full));
    assert_eq!(art_of(&d, f), BlendArt::Rundung { radius: 3.0 });
    assert_eq!(status(&d, f), FeatureStatus::Ok);
    assert_ne!(corners(&d, f), ok_body);
    d.check_invariants().unwrap();
}
