//! Formeln in Skizzenmaßen (Aufgabe 6): Nebentabelle `SketchFeature.formulas` nach
//! Abhängigkeit, Winkel in Grad, Wertprüfung der Skizze, Solverstatus bei Skizzen mit Formeln
//! (Plan P12), Aufräumen, Gleichheit, Benutzer/Umbenennen/Ersetzen und Rückgängig.

use planora_feature::*;
use planora_kernel::Solid;
use planora_math::{DVec2, DVec3, Frame, Tolerance};
use planora_sketch::{ConstraintId, ConstraintKind, LineOrPoints, PointId, Sketch};

use ParamUnit::{Angle, Length};

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

fn error_text(d: &Document, id: FeatureId) -> String {
    match status(d, id) {
        FeatureStatus::Error(m) => m,
        s => panic!("Fehlerstatus erwartet, bekam {s:?}"),
    }
}

fn formula_err<T: std::fmt::Debug>(r: Result<T, FeatureError>) -> String {
    match r {
        Err(FeatureError::Formula(m)) => m,
        other => panic!("Formula-Fehler erwartet, bekam {other:?}"),
    }
}

fn sketch_feature(d: &Document, id: FeatureId) -> &SketchFeature {
    match &d.feature(id).unwrap().kind {
        FeatureKind::Sketch(sf) => sf,
        k => panic!("keine Skizze: {k:?}"),
    }
}

/// Gespeicherter Wert des Maßes in der Definition der Skizze.
fn stored(d: &Document, id: FeatureId, c: ConstraintId) -> f64 {
    let kind = &sketch_feature(d, id).sketch.constraint(c).unwrap().kind;
    match kind {
        ConstraintKind::Distance(_, _, v) | ConstraintKind::Angle(_, _, v) => *v,
        k => panic!("kein Maß: {k:?}"),
    }
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

fn extrude(sketch: FeatureId) -> FeatureKind {
    FeatureKind::Extrude {
        sketch,
        profile: 0,
        height: 5.0,
    }
}

fn on_xy(sketch: Sketch, formulas: SketchFormulas) -> SketchFeature {
    SketchFeature {
        sketch,
        plane: SketchPlane::Standard(StandardPlane::XY),
        copies: vec![],
        formulas,
    }
}

/// Voll bestimmtes Rechteck: Ecke 0 fest im Ursprung, Unterseite `w` (Abstand Ecke 0–1),
/// rechte Seite 20 (Abstand Ecke 1–2).
struct Rect {
    sketch: Sketch,
    points: [PointId; 4],
    bottom: ConstraintId,
    right: ConstraintId,
    horizontal: ConstraintId,
}

fn rect(w: f64) -> Rect {
    let mut sk = Sketch::new(Frame::WORLD, "Skizze", Tolerance::default());
    let ids = sk.add_rectangle(DVec2::ZERO, DVec2::new(w, 20.0)).unwrap();
    let p = ids.points;
    sk.add_constraint(ConstraintKind::Fixed(p[0], 0.0, 0.0))
        .unwrap();
    let bottom = sk
        .add_constraint(ConstraintKind::Distance(p[0], p[1], w))
        .unwrap();
    let right = sk
        .add_constraint(ConstraintKind::Distance(p[1], p[2], 20.0))
        .unwrap();
    let horizontal = sk
        .constraints()
        .find(|(_, c)| matches!(c.kind, ConstraintKind::Horizontal(LineOrPoints::Line(_))))
        .map(|(id, _)| id)
        .unwrap();
    Rect {
        sketch: sk,
        points: p,
        bottom,
        right,
        horizontal,
    }
}

fn one(c: ConstraintId, text: &str) -> SketchFormulas {
    [(c, text.to_string())].into_iter().collect()
}

/// Parameter Breite = 30; Skizze 1 (Rechteck, Unterseite „Breite“) und Extrusion 1.
fn rect_doc() -> (Document, ParamId, FeatureId, FeatureId, Rect) {
    let mut d = Document::new();
    let b = d.add_parameter("Breite", "30", Length, "").unwrap();
    let r = rect(30.0);
    let s = d
        .add_feature(
            "Skizze 1",
            FeatureKind::Sketch(on_xy(r.sketch.clone(), one(r.bottom, "Breite"))),
        )
        .unwrap();
    let e = d.add_feature("Extrusion 1", extrude(s)).unwrap();
    d.check_invariants().unwrap();
    (d, b, s, e, r)
}

/// Breite der Unterseite in der berechneten Skizze.
fn measured_width(d: &Document, s: FeatureId, r: &Rect) -> f64 {
    let sk = d.sketch_of(s).expect("berechnete Skizze");
    let a = sk.point_xy(r.points[0]).unwrap();
    let b = sk.point_xy(r.points[1]).unwrap();
    (b - a).length()
}

#[test]
fn dimension_follows_parameter() {
    let (mut d, b, s, e, r) = rect_doc();
    assert_eq!(status(&d, s), FeatureStatus::Ok);
    assert_eq!(stored(&d, s, r.bottom), 30.0);
    let (lo, hi) = hull_of(&d, e);
    assert!(((hi - lo).x - 30.0).abs() <= 1e-6, "{lo:?} {hi:?}");
    let report = d.set_parameter(b, edit(&d, b, "40")).unwrap();
    assert!(report.computed.contains(&s), "{report:?}");
    assert!(report.computed.contains(&e), "{report:?}");
    assert_eq!(stored(&d, s, r.bottom), 40.0, "gespeicherter Maßwert");
    assert_eq!(status(&d, s), FeatureStatus::Ok);
    assert_eq!(status(&d, e), FeatureStatus::Ok);
    assert!((measured_width(&d, s, &r) - 40.0).abs() <= 1e-9);
    let (lo, hi) = hull_of(&d, e);
    assert!(((hi - lo).x - 40.0).abs() <= 1e-6, "{lo:?} {hi:?}");
    assert!(((hi - lo).y - 20.0).abs() <= 1e-6, "{lo:?} {hi:?}");
    // Die Formel bleibt stehen, die übrigen Maße bleiben unverändert.
    assert_eq!(sketch_feature(&d, s).formulas, one(r.bottom, "Breite"));
    assert_eq!(stored_any(&d, s, r.right), 20.0);
    d.check_invariants().unwrap();
}

fn stored_any(d: &Document, id: FeatureId, c: ConstraintId) -> f64 {
    stored(d, id, c)
}

#[test]
fn no_value_change_keeps_version_and_revision() {
    let (mut d, _b, s, e, r) = rect_doc();
    let other = d.add_parameter("Andere", "5", Length, "").unwrap();
    let version = d.feature(s).unwrap().version();
    let revision = sketch_feature(&d, s).sketch.revision();
    let report = d.set_parameter(other, edit(&d, other, "6")).unwrap();
    assert!(report.computed.is_empty(), "{report:?}");
    assert_eq!(d.feature(s).unwrap().version(), version);
    assert_eq!(sketch_feature(&d, s).sketch.revision(), revision);
    assert_eq!(status(&d, e), FeatureStatus::Ok);
    // Eine Formel, die denselben Wert ergibt, ändert den Stand der Skizze nicht.
    d.update(s, |k| {
        if let FeatureKind::Sketch(sf) = k {
            sf.formulas.insert(r.right, "20".into());
        }
    })
    .unwrap();
    assert_eq!(sketch_feature(&d, s).sketch.revision(), revision);
    assert_eq!(sketch_feature(&d, s).formulas.len(), 2);
}

#[test]
fn ellipse_axes_change_together() {
    // Beide Halbachsen mit Formeln: Beim Verkleinern lehnt die Skizze die große Halbachse erst
    // ab (die kleine ist noch länger); nach der kleinen gelingt sie.
    let mut d = Document::new();
    let k = d.add_parameter("k", "10", Length, "").unwrap();
    let mut sk = Sketch::new(Frame::WORLD, "Skizze", Tolerance::default());
    let m = sk.add_point(0.0, 0.0).unwrap();
    let el = sk.add_ellipse(m, DVec2::new(20.0, 0.0), 10.0).unwrap();
    sk.add_constraint(ConstraintKind::Fixed(m, 0.0, 0.0))
        .unwrap();
    let major = sk
        .add_constraint(ConstraintKind::EllipseMajor(el, 20.0))
        .unwrap();
    let minor = sk
        .add_constraint(ConstraintKind::EllipseMinor(el, 10.0))
        .unwrap();
    assert!(major < minor, "die große Halbachse kommt zuerst dran");
    let formulas: SketchFormulas = [(major, "2*k"), (minor, "k")]
        .into_iter()
        .map(|(k, t)| (k, t.to_string()))
        .collect();
    let s = d
        .add_feature("Skizze 1", FeatureKind::Sketch(on_xy(sk, formulas)))
        .unwrap();
    assert_eq!(status(&d, s), FeatureStatus::Ok);
    let value = |d: &Document, c: ConstraintId| match sketch_feature(d, s)
        .sketch
        .constraint(c)
        .unwrap()
        .kind
    {
        ConstraintKind::EllipseMajor(_, v) | ConstraintKind::EllipseMinor(_, v) => v,
        ref k => panic!("{k:?}"),
    };
    assert_eq!((value(&d, major), value(&d, minor)), (20.0, 10.0));
    // k = 4: große Halbachse 8 erst nach der kleinen (4) zulässig.
    d.set_parameter(k, edit(&d, k, "4")).unwrap();
    assert_eq!(status(&d, s), FeatureStatus::Ok);
    assert_eq!((value(&d, major), value(&d, minor)), (8.0, 4.0));
    d.check_invariants().unwrap();
    // k = 30: wachsen geht ohnehin.
    d.set_parameter(k, edit(&d, k, "30")).unwrap();
    assert_eq!(status(&d, s), FeatureStatus::Ok);
    assert_eq!((value(&d, major), value(&d, minor)), (60.0, 30.0));
    // Kleine Halbachse länger als die große: Wertfehler mit der Meldung der Skizze, Maße bleiben.
    d.update(s, |kind| {
        if let FeatureKind::Sketch(sf) = kind {
            sf.formulas.insert(minor, "k*3".into());
        }
    })
    .unwrap();
    let msg = error_text(&d, s);
    assert_eq!(
        msg,
        "Ein Maß ergibt 90 mm: Die kleine Halbachse darf nicht länger als die große sein; \
         zuerst die große Halbachse vergrößern"
    );
    assert_eq!((value(&d, major), value(&d, minor)), (60.0, 30.0));
    d.check_invariants().unwrap();
}

#[test]
fn angle_formula_in_degrees() {
    let mut d = Document::new();
    let w = d.add_parameter("w", "60", Angle, "").unwrap();
    let mut sk = Sketch::new(Frame::WORLD, "Skizze", Tolerance::default());
    let o = sk.add_point(0.0, 0.0).unwrap();
    let a = sk.add_point(10.0, 0.0).unwrap();
    let b = sk.add_point(5.0, 8.0).unwrap();
    let la = sk.add_line(o, a).unwrap();
    let lb = sk.add_line(o, b).unwrap();
    let ang = sk
        .add_constraint(ConstraintKind::Angle(la, lb, 1.0))
        .unwrap();
    let s = d
        .add_feature("Skizze 1", FeatureKind::Sketch(on_xy(sk, one(ang, "w"))))
        .unwrap();
    assert_eq!(status(&d, s), FeatureStatus::Ok);
    assert!((stored(&d, s, ang) - 60f64.to_radians()).abs() <= 1e-12);
    // gemessen in der berechneten Skizze
    let sk = d.sketch_of(s).unwrap();
    let measured = sk
        .measure(&sk.constraint(ang).unwrap().kind)
        .expect("Winkel messbar");
    assert!((measured - 60f64.to_radians()).abs() <= 1e-9, "{measured}");
    d.set_parameter(w, edit(&d, w, "45")).unwrap();
    assert!((stored(&d, s, ang) - 45f64.to_radians()).abs() <= 1e-12);
    // Eine Länge passt nicht in ein Winkelmaß: abgelehnt, nichts ändert sich.
    let before = d.snapshot();
    let count = d.edit_count();
    let mut e = edit(&d, w, "45");
    e.unit = Length;
    assert_eq!(
        formula_err(d.set_parameter(w, e)),
        "Skizze 1: Hier wird ein Winkel erwartet, die Formel ergibt eine Länge."
    );
    assert!(d.snapshot().same_content(&before));
    assert_eq!(d.edit_count(), count);
    d.check_invariants().unwrap();
    // Eine reine Zahl gilt als Grad.
    let r = d.update(s, |k| {
        if let FeatureKind::Sketch(sf) = k {
            sf.formulas.insert(ang, "30".into());
        }
    });
    r.unwrap();
    assert!((stored(&d, s, ang) - 30f64.to_radians()).abs() <= 1e-12);
}

#[test]
fn constraint_unit_by_kind() {
    use planora_sketch::{CurveId, PointId};
    let p = PointId::from_index(0);
    let q = PointId::from_index(1);
    let c = CurveId::from_index(0);
    let k = CurveId::from_index(1);
    assert_eq!(
        constraint_unit(&ConstraintKind::Angle(c, k, 1.0)),
        ParamUnit::Angle
    );
    assert_eq!(
        constraint_unit(&ConstraintKind::EllipseAngle(c, 1.0)),
        ParamUnit::Angle
    );
    for kind in [
        ConstraintKind::Distance(p, q, 1.0),
        ConstraintKind::DistancePointLine(p, c, 1.0),
        ConstraintKind::Radius(c, 1.0),
        ConstraintKind::Diameter(c, 1.0),
        ConstraintKind::EllipseMajor(c, 1.0),
        ConstraintKind::EllipseMinor(c, 1.0),
    ] {
        assert_eq!(constraint_unit(&kind), ParamUnit::Length, "{kind:?}");
    }
}

/// Dreieck mit den Seiten „a“, „b“, „c“ (30, 30, 40), Ecke 0 fest, Seite a waagerecht.
fn triangle_doc() -> (Document, ParamId, FeatureId, FeatureId) {
    let mut d = Document::new();
    d.add_parameter("a", "30", Length, "").unwrap();
    d.add_parameter("b", "30", Length, "").unwrap();
    let c = d.add_parameter("c", "40", Length, "").unwrap();
    let mut sk = Sketch::new(Frame::WORLD, "Skizze", Tolerance::default());
    let p0 = sk.add_point(0.0, 0.0).unwrap();
    let p1 = sk.add_point(30.0, 0.0).unwrap();
    let p2 = sk.add_point(20.0, 20.0).unwrap();
    let l0 = sk.add_line(p0, p1).unwrap();
    sk.add_line(p1, p2).unwrap();
    sk.add_line(p2, p0).unwrap();
    sk.add_constraint(ConstraintKind::Fixed(p0, 0.0, 0.0))
        .unwrap();
    sk.add_constraint(ConstraintKind::Horizontal(LineOrPoints::Line(l0)))
        .unwrap();
    let da = sk
        .add_constraint(ConstraintKind::Distance(p0, p1, 30.0))
        .unwrap();
    let db = sk
        .add_constraint(ConstraintKind::Distance(p1, p2, 30.0))
        .unwrap();
    let dc = sk
        .add_constraint(ConstraintKind::Distance(p2, p0, 40.0))
        .unwrap();
    let formulas: SketchFormulas = [(da, "a"), (db, "b"), (dc, "c")]
        .into_iter()
        .map(|(k, t)| (k, t.to_string()))
        .collect();
    let s = d
        .add_feature("Skizze 1", FeatureKind::Sketch(on_xy(sk, formulas)))
        .unwrap();
    let e = d.add_feature("Extrusion 1", extrude(s)).unwrap();
    (d, c, s, e)
}

#[test]
fn unsolvable_is_error_status() {
    let (mut d, c, s, e) = triangle_doc();
    assert_eq!(status(&d, s), FeatureStatus::Ok);
    assert_eq!(status(&d, e), FeatureStatus::Ok);
    let before = d.snapshot();
    d.set_parameter(c, edit(&d, c, "100")).unwrap();
    let msg = error_text(&d, s);
    assert!(
        msg.starts_with("Fehler in der Skizze: Mit den Werten der Formeln"),
        "{msg}"
    );
    // Der Solver findet die Seite, ohne die es ginge: ein Widerspruch.
    assert_eq!(
        msg,
        "Fehler in der Skizze: Mit den Werten der Formeln widersprechen sich Abhängigkeiten"
    );
    assert!(d.sketch_of(s).is_none(), "keine berechnete Skizze");
    assert!(
        matches!(status(&d, e), FeatureStatus::Skipped(_)),
        "{:?}",
        status(&d, e)
    );
    d.check_invariants().unwrap();
    // Zurück: beide wieder Ok, Hüllbox wie vorher.
    d.set_parameter(c, edit(&d, c, "40")).unwrap();
    assert_eq!(status(&d, s), FeatureStatus::Ok);
    assert_eq!(status(&d, e), FeatureStatus::Ok);
    d.check_invariants().unwrap();
    // Rückgängig über das Zustandsbild stellt Tabelle, Maß und Körper her.
    d.set_parameter(c, edit(&d, c, "100")).unwrap();
    assert!(matches!(status(&d, s), FeatureStatus::Error(_)));
    assert_eq!(d.restore(before.clone()).unwrap(), RestoreKind::Full);
    assert_eq!(status(&d, s), FeatureStatus::Ok);
    assert_eq!(status(&d, e), FeatureStatus::Ok);
    assert!(d.snapshot().same_model(&before));
    d.check_invariants().unwrap();
}

#[test]
fn never_silently_wrong() {
    let (mut d, b, s, e, r) = rect_doc();
    d.set_parameter(b, edit(&d, b, "400")).unwrap();
    match status(&d, s) {
        FeatureStatus::Error(m) => {
            assert!(m.starts_with("Fehler in der Skizze: Mit den Werten der Formeln"));
            assert!(matches!(status(&d, e), FeatureStatus::Skipped(_)));
        }
        FeatureStatus::Ok => {
            assert!((measured_width(&d, s, &r) - 400.0).abs() <= 1e-6);
            let (lo, hi) = hull_of(&d, e);
            assert!(((hi - lo).x - 400.0).abs() <= 1e-6, "{lo:?} {hi:?}");
        }
        st => panic!("{st:?}"),
    }
    d.set_parameter(b, edit(&d, b, "30")).unwrap();
    assert_eq!(status(&d, s), FeatureStatus::Ok);
    assert!((measured_width(&d, s, &r) - 30.0).abs() <= 1e-6);
    let (lo, hi) = hull_of(&d, e);
    assert!(((hi - lo).x - 30.0).abs() <= 1e-6, "{lo:?} {hi:?}");
    d.check_invariants().unwrap();
}

#[test]
fn load_drops_formulas_without_dimension() {
    // Ein Bild mit Formeln an einer gelöschten und an einer Nicht-Maß-Abhängigkeit lädt; die
    // Formeln entfallen still, die gültige bleibt.
    let (d, _b, s, _e, r) = rect_doc();
    let mut snap = d.snapshot();
    let idx = snap.features.iter().position(|f| f.id == s).unwrap();
    let FeatureKind::Sketch(sf) = &mut snap.features[idx].kind else {
        panic!("keine Skizze")
    };
    sf.formulas.insert(r.horizontal, "Breite".into());
    sf.sketch.remove_constraint(r.right).unwrap();
    sf.formulas.insert(r.right, "Breite".into());
    let loaded = Document::from_snapshot(snap, 0.05).unwrap();
    assert_eq!(sketch_feature(&loaded, s).formulas, one(r.bottom, "Breite"));
    loaded.check_invariants().unwrap();
}

#[test]
fn prune_and_equality() {
    let r = rect(30.0);
    // Abhängigkeit gelöscht → Formel entfällt
    let mut sf = on_xy(r.sketch.clone(), one(r.bottom, "Breite"));
    assert!(!sf.prune_formulas(), "nichts zu entfernen");
    sf.sketch.remove_constraint(r.bottom).unwrap();
    assert!(sf.prune_formulas());
    assert!(sf.formulas.is_empty());
    assert!(!sf.prune_formulas());
    // anzeigendes Maß → Formel entfällt
    let mut sf = on_xy(
        r.sketch.clone(),
        [
            (r.bottom, "Breite".to_string()),
            (r.right, "20".to_string()),
        ]
        .into_iter()
        .collect(),
    );
    sf.sketch.set_driven(r.bottom, true).unwrap();
    assert!(sf.prune_formulas());
    assert_eq!(sf.formulas, one(r.right, "20"));
    // keine Maß-Abhängigkeit → Formel entfällt
    let mut sf = on_xy(r.sketch.clone(), one(r.horizontal, "Breite"));
    assert!(sf.prune_formulas());
    assert!(sf.formulas.is_empty());
    // Gleichheit: nur der Formeltext verschieden, gleiche Revision → ungleich
    let a = on_xy(r.sketch.clone(), one(r.bottom, "Breite"));
    let mut b = a.clone();
    assert_eq!(a, b, "Klon gleich");
    b.formulas.insert(r.bottom, "breite".into());
    assert_eq!(a.sketch.revision(), b.sketch.revision());
    assert_ne!(a, b);
    let mut c = a.clone();
    c.formulas.clear();
    assert_ne!(a, c);

    // `update` mit einer Formel an einem anzeigenden Maß: die gespeicherte Tabelle ist ohne sie.
    let mut d = Document::new();
    d.add_parameter("Breite", "30", Length, "").unwrap();
    let mut sk = r.sketch.clone();
    let shown = sk
        .add_constraint(ConstraintKind::Distance(r.points[2], r.points[3], 30.0))
        .unwrap();
    sk.set_driven(shown, true).unwrap();
    let s = d
        .add_feature(
            "Skizze 1",
            FeatureKind::Sketch(on_xy(sk.clone(), one(shown, "Breite"))),
        )
        .unwrap();
    assert!(sketch_feature(&d, s).formulas.is_empty(), "beim Anlegen");
    let count = d.edit_count();
    d.update(s, |k| {
        if let FeatureKind::Sketch(sf) = k {
            sf.formulas.insert(shown, "Breite".into());
            sf.formulas.insert(r.bottom, "Breite".into());
        }
    })
    .unwrap();
    assert_eq!(sketch_feature(&d, s).formulas, one(r.bottom, "Breite"));
    assert_eq!(d.edit_count(), count + 1);
    d.check_invariants().unwrap();
    // Eine Abhängigkeit mit Formel löschen: die Formel entfällt mit ihr.
    d.update(s, |k| {
        if let FeatureKind::Sketch(sf) = k {
            sf.sketch.remove_constraint(r.bottom).unwrap();
        }
    })
    .unwrap();
    assert!(sketch_feature(&d, s).formulas.is_empty());
    d.check_invariants().unwrap();
}

#[test]
fn typed_number_replaces_the_formula() {
    // P2: Ändert `update` den Wert eines Maßes mit Formel, entfällt nur diese Formel.
    let (mut d, b, s, _e, r) = rect_doc();
    d.update(s, |k| {
        if let FeatureKind::Sketch(sf) = k {
            sf.formulas.insert(r.right, "Breite/2".into());
        }
    })
    .unwrap();
    assert_eq!(stored(&d, s, r.right), 15.0);
    d.update(s, |k| {
        if let FeatureKind::Sketch(sf) = k {
            sf.sketch.set_constraint_value(r.bottom, 7.0).unwrap();
        }
    })
    .unwrap();
    assert_eq!(sketch_feature(&d, s).formulas, one(r.right, "Breite/2"));
    assert_eq!(stored(&d, s, r.bottom), 7.0);
    d.set_parameter(b, edit(&d, b, "50")).unwrap();
    assert_eq!(stored(&d, s, r.bottom), 7.0, "getippte Zahl bleibt");
    assert_eq!(stored(&d, s, r.right), 25.0);
    // Ein gleichzeitig neu gesetzter Formeltext bleibt (das Fenster schickt ihn mit).
    d.update(s, |k| {
        if let FeatureKind::Sketch(sf) = k {
            sf.sketch.set_constraint_value(r.bottom, 9.0).unwrap();
            sf.formulas.insert(r.bottom, "Breite".into());
        }
    })
    .unwrap();
    assert_eq!(stored(&d, s, r.bottom), 50.0);
    d.check_invariants().unwrap();
}

#[test]
fn invalid_sketch_formula_rejected() {
    let (mut d, _b, s, _e, r) = rect_doc();
    let before = d.snapshot();
    let count = d.edit_count();
    let version = d.feature(s).unwrap().version();
    let res = d.update(s, |k| {
        if let FeatureKind::Sketch(sf) = k {
            sf.formulas.insert(r.right, "Breit".into());
        }
    });
    assert_eq!(
        formula_err(res),
        "Skizze 1: Den Namen „Breit“ gibt es nicht."
    );
    assert!(d.snapshot().same_content(&before));
    assert_eq!(d.edit_count(), count);
    assert_eq!(d.feature(s).unwrap().version(), version);
    // auch beim Anlegen
    let r2 = rect(10.0);
    let res = d.add_feature(
        "Skizze 2",
        FeatureKind::Sketch(on_xy(r2.sketch.clone(), one(r2.bottom, "1+"))),
    );
    assert!(formula_err(res).starts_with("Skizze 2: "));
    assert!(d.snapshot().same_content(&before));
}

#[test]
fn value_check_gives_error_status() {
    let (mut d, b, s, e, r) = rect_doc();
    d.update(s, |k| {
        if let FeatureKind::Sketch(sf) = k {
            sf.formulas.insert(r.bottom, "Breite-20".into());
        }
    })
    .unwrap();
    assert_eq!(stored(&d, s, r.bottom), 10.0);
    d.set_parameter(b, edit(&d, b, "20")).unwrap();
    let msg = error_text(&d, s);
    assert!(msg.contains("Ein Maß ergibt 0 mm: "), "{msg}");
    assert_eq!(
        msg,
        "Ein Maß ergibt 0 mm: Das Maß muss größer als null sein"
    );
    assert!(matches!(status(&d, e), FeatureStatus::Skipped(_)));
    // Das Maß behält seinen letzten gültigen Wert.
    assert_eq!(stored(&d, s, r.bottom), 10.0);
    d.check_invariants().unwrap();
    d.set_parameter(b, edit(&d, b, "35")).unwrap();
    assert_eq!(status(&d, s), FeatureStatus::Ok);
    assert_eq!(status(&d, e), FeatureStatus::Ok);
    assert_eq!(stored(&d, s, r.bottom), 15.0);
    d.check_invariants().unwrap();
}

#[test]
fn users_rename_replace_sketch() {
    let (mut d, b, s, _e, r) = rect_doc();
    d.set_parameter(b, edit(&d, b, "40")).unwrap();
    assert_eq!(d.parameter_users(b), vec!["Skizze 1".to_string()]);
    let err = d.remove_parameter(b).unwrap_err();
    assert_eq!(err.to_string(), "Breite wird in Skizze 1 benutzt.");
    let before = sketch_feature(&d, s).clone();
    let snap = d.snapshot();
    let mut e = edit(&d, b, "40");
    e.name = "Weite".into();
    d.set_parameter(b, e).unwrap();
    assert_eq!(sketch_feature(&d, s).formulas, one(r.bottom, "Weite"));
    assert_ne!(*sketch_feature(&d, s), before);
    assert!(!d.snapshot().same_model(&snap));
    assert_eq!(d.parameter_users(b), vec!["Skizze 1".to_string()]);
    d.check_invariants().unwrap();
    // Rückgängig bringt Name und Formel zurück.
    let renamed = d.snapshot();
    d.restore(snap.clone()).unwrap();
    assert_eq!(sketch_feature(&d, s).formulas, one(r.bottom, "Breite"));
    d.restore(renamed).unwrap();
    // Durch den Wert ersetzen: Formel weg, Wert gleich, ein Schritt.
    let count = d.edit_count();
    d.replace_parameter_by_value(b).unwrap();
    assert!(sketch_feature(&d, s).formulas.is_empty());
    assert_eq!(stored(&d, s, r.bottom), 40.0);
    assert_eq!(d.edit_count(), count + 1);
    assert!(d.parameters().is_empty());
    assert_eq!(status(&d, s), FeatureStatus::Ok);
    d.check_invariants().unwrap();
}

/// Rechteck mit zwei sich widersprechenden Abständen der Unterseite (30 und 40).
fn conflicting_sketch() -> (Sketch, ConstraintId) {
    let r = rect(30.0);
    let mut sk = r.sketch;
    sk.add_constraint(ConstraintKind::Distance(r.points[0], r.points[1], 40.0))
        .unwrap();
    (sk, r.right)
}

#[test]
fn sketch_without_formulas_unchanged() {
    let mut d = Document::new();
    d.add_parameter("h", "20", Length, "").unwrap();
    let (sk, right) = conflicting_sketch();
    let mut probe = sk.clone();
    assert_ne!(probe.solve().status, planora_sketch::SolveStatus::Solved);
    let s = d
        .add_feature(
            "Skizze 1",
            FeatureKind::Sketch(on_xy(sk.clone(), SketchFormulas::new())),
        )
        .unwrap();
    assert_eq!(status(&d, s), FeatureStatus::Ok, "wie bisher");
    assert!(d.sketch_of(s).is_some());
    // P12: Mit einer Formel (auch an einem unbeteiligten Maß) wird dieselbe Skizze ein Fehler.
    d.update(s, |k| {
        if let FeatureKind::Sketch(sf) = k {
            sf.formulas.insert(right, "h".into());
        }
    })
    .unwrap();
    assert_eq!(
        error_text(&d, s),
        "Fehler in der Skizze: Mit den Werten der Formeln widersprechen sich Abhängigkeiten"
    );
    d.check_invariants().unwrap();
    // Ohne Formel wieder Ok (die Formel ist weg, die Skizze rechnet wie bisher).
    d.update(s, |k| {
        if let FeatureKind::Sketch(sf) = k {
            sf.formulas.clear();
        }
    })
    .unwrap();
    assert_eq!(status(&d, s), FeatureStatus::Ok);
}

/// Eine Linie, deren Enden zweimal auf denselben Punkt festgelegt sind: Der Solver findet keine
/// einzelne schuldige Abhängigkeit („nicht gelöst“ statt „Widerspruch“). Dazu ein Kreis mit
/// Radiusmaß.
fn not_converging_sketch() -> (Sketch, ConstraintId) {
    let mut sk = Sketch::new(Frame::WORLD, "Skizze", Tolerance::default());
    let p0 = sk.add_point(0.0, 0.0).unwrap();
    let p1 = sk.add_point(10.0, 0.0).unwrap();
    sk.add_line(p0, p1).unwrap();
    sk.add_constraint(ConstraintKind::Fixed(p0, 0.0, 0.0))
        .unwrap();
    sk.add_constraint(ConstraintKind::Fixed(p1, 0.0, 0.0))
        .unwrap();
    sk.add_constraint(ConstraintKind::Fixed(p1, 0.0, 0.0))
        .unwrap();
    let m = sk.add_point(50.0, 0.0).unwrap();
    let c = sk.add_circle(m, 5.0).unwrap();
    let r = sk.add_constraint(ConstraintKind::Radius(c, 5.0)).unwrap();
    (sk, r)
}

#[test]
fn not_converged_has_its_own_message() {
    let (sk, radius) = not_converging_sketch();
    let mut probe = sk.clone();
    assert_eq!(
        probe.solve().status,
        planora_sketch::SolveStatus::NotConverged
    );
    let mut d = Document::new();
    d.add_parameter("h", "7", Length, "").unwrap();
    let s = d
        .add_feature(
            "Skizze 1",
            FeatureKind::Sketch(on_xy(sk.clone(), SketchFormulas::new())),
        )
        .unwrap();
    assert_eq!(status(&d, s), FeatureStatus::Ok, "ohne Formeln wie bisher");
    d.update(s, |k| {
        if let FeatureKind::Sketch(sf) = k {
            sf.formulas.insert(radius, "h".into());
        }
    })
    .unwrap();
    assert_eq!(
        error_text(&d, s),
        "Fehler in der Skizze: Mit den Werten der Formeln lässt sich die Skizze nicht lösen"
    );
    d.check_invariants().unwrap();
}

// ---- Nachbesserung (Review F1, F2, F3, F6) ----------------------------------------------------

/// Status aller Features gleich wie nach dem Laden desselben Bildes (pfadunabhängig).
fn assert_same_as_loaded(d: &Document) {
    let loaded = Document::from_snapshot(d.snapshot(), 0.05).unwrap();
    for f in d.features() {
        assert_eq!(
            f.status,
            loaded.feature(f.id).unwrap().status,
            "Feature „{}“ nach dem Laden anders",
            f.name
        );
    }
}

fn replace_refused(d: &mut Document, p: ParamId, feature: &str) {
    let before = d.snapshot();
    let count = d.edit_count();
    let name = d.parameter(p).unwrap().name.clone();
    assert_eq!(
        formula_err(d.replace_parameter_by_value(p)),
        format!(
            "Parameter „{name}“ lässt sich nicht durch den Wert ersetzen: „{feature}“ hat \
             gerade einen Fehler. Beheben Sie zuerst den Fehler."
        )
    );
    assert!(d.snapshot().same_content(&before), "nichts geändert");
    assert_eq!(d.edit_count(), count, "kein Schritt");
}

#[test]
fn replace_in_unsolved_sketch_is_refused_and_path_independent() {
    // Probe aus dem Review (F1): Dreieck a/b/c, c = 100 → Widerspruch. Die Formeln dürfen nicht
    // so verschwinden, dass ein alter Fehlschlag stehen bleibt; das Ersetzen wird abgelehnt.
    let (mut d, c, s, e) = triangle_doc();
    d.set_parameter(c, edit(&d, c, "100")).unwrap();
    assert!(matches!(status(&d, s), FeatureStatus::Error(_)));
    let ids: Vec<ParamId> = d.parameters().iter().map(|p| p.id).collect();
    for p in ids {
        replace_refused(&mut d, p, "Skizze 1");
    }
    assert_eq!(sketch_feature(&d, s).formulas.len(), 3);
    assert!(matches!(status(&d, e), FeatureStatus::Skipped(_)));
    assert_same_as_loaded(&d);
    d.check_invariants().unwrap();
    // Ungelöste Skizze mit einer Formel an einem unbeteiligten Maß: ebenso.
    let mut d = Document::new();
    let h = d.add_parameter("h", "20", Length, "").unwrap();
    let (sk, right) = conflicting_sketch();
    let s = d
        .add_feature("Skizze 1", FeatureKind::Sketch(on_xy(sk, one(right, "h"))))
        .unwrap();
    assert!(matches!(status(&d, s), FeatureStatus::Error(_)));
    replace_refused(&mut d, h, "Skizze 1");
    assert_same_as_loaded(&d);
}

#[test]
fn changed_sketch_formulas_get_a_new_version() {
    // F1: Ändern Umbenennen oder Ersetzen die Formeln einer Skizze, bekommt sie eine neue
    // Version (kein gespeicherter Fehlschlag überdauert); gesunde Skizzen bleiben Ok.
    let (mut d, b, s, e, r) = rect_doc();
    let v0 = d.feature(s).unwrap().version();
    let mut ed = edit(&d, b, "30");
    ed.name = "Weite".into();
    d.set_parameter(b, ed).unwrap();
    let v1 = d.feature(s).unwrap().version();
    assert!(v1 > v0, "Umbenennen: neue Version");
    assert_same_as_loaded(&d);
    let count = d.edit_count();
    d.replace_parameter_by_value(b).unwrap();
    assert!(
        d.feature(s).unwrap().version() > v1,
        "Ersetzen: neue Version"
    );
    assert_eq!(d.edit_count(), count + 1, "ein Schritt");
    assert_eq!(stored(&d, s, r.bottom), 30.0);
    assert_eq!(status(&d, s), FeatureStatus::Ok);
    assert_eq!(status(&d, e), FeatureStatus::Ok);
    assert_same_as_loaded(&d);
    d.check_invariants().unwrap();
}

#[test]
fn replace_refused_while_sketch_has_a_value_error() {
    // F2: „Breite-30“ mit Breite 30 ergibt 0 → Wertfehler, das Maß steht noch auf 10. Ersetzen
    // ergäbe still ein gültiges 10-mm-Modell; es wird abgelehnt.
    let (mut d, b, s, e, r) = rect_doc();
    d.update(s, |k| {
        if let FeatureKind::Sketch(sf) = k {
            sf.formulas.insert(r.bottom, "Breite-30".into());
        }
    })
    .unwrap();
    d.set_parameter(b, edit(&d, b, "40")).unwrap();
    d.set_parameter(b, edit(&d, b, "30")).unwrap();
    assert!(error_text(&d, s).starts_with("Ein Maß ergibt 0 mm: "));
    replace_refused(&mut d, b, "Skizze 1");
    assert!(matches!(status(&d, e), FeatureStatus::Skipped(_)));
    assert_same_as_loaded(&d);
    // Nach dem Beheben geht es wie gewohnt, als ein Schritt.
    d.set_parameter(b, edit(&d, b, "40")).unwrap();
    let count = d.edit_count();
    d.replace_parameter_by_value(b).unwrap();
    assert_eq!(d.edit_count(), count + 1);
    assert_eq!(stored(&d, s, r.bottom), 10.0);
    assert_eq!(status(&d, e), FeatureStatus::Ok);
    d.check_invariants().unwrap();
}

#[test]
fn replace_refused_while_body_feature_fails() {
    // F2 auch für Körperfelder: Fehler (Wertfehler) und übersprungen.
    let mut d = Document::new();
    let b = d.add_parameter("Breite", "40", Length, "").unwrap();
    let q = d
        .add_feature_with_formulas(
            "Quader 1",
            FeatureKind::Box {
                size: DVec3::ONE,
                origin: DVec3::ZERO,
            },
            [(FieldKey::Breite, "Breite-40".to_string())]
                .into_iter()
                .collect(),
        )
        .unwrap();
    assert!(matches!(status(&d, q), FeatureStatus::Error(_)));
    replace_refused(&mut d, b, "Quader 1");
    // Verschieben mit Formel hinter einem Quader im Fehler: übersprungen → ebenso abgelehnt.
    let mut d2 = Document::new();
    let k2 = d2.add_parameter("k", "1", Length, "").unwrap();
    let dx = d2.add_parameter("dx", "5", Length, "").unwrap();
    let q2 = d2
        .add_feature_with_formulas(
            "Quader 1",
            FeatureKind::Box {
                size: DVec3::ONE,
                origin: DVec3::ZERO,
            },
            [(FieldKey::Breite, "k-1".to_string())]
                .into_iter()
                .collect(),
        )
        .unwrap();
    let m = d2
        .add_feature_with_formulas(
            "Verschieben 1",
            FeatureKind::Move {
                body: BodyRef {
                    feature: q2,
                    index: 0,
                },
                delta: DVec3::ZERO,
            },
            [(FieldKey::X, "dx".to_string())].into_iter().collect(),
        )
        .unwrap();
    assert!(matches!(status(&d2, m), FeatureStatus::Skipped(_)));
    replace_refused(&mut d2, dx, "Verschieben 1");
    // Behoben (k = 2): Ersetzen von dx ist ein Schritt, Δx bleibt 5.
    d2.set_parameter(k2, edit(&d2, k2, "2")).unwrap();
    let count = d2.edit_count();
    d2.replace_parameter_by_value(dx).unwrap();
    assert_eq!(d2.edit_count(), count + 1);
    assert!(d2.feature(m).unwrap().formulas().is_empty());
    assert_eq!(status(&d2, m), FeatureStatus::Ok);
    d2.check_invariants().unwrap();
}

#[test]
fn load_with_broken_sketch_formula() {
    // F3: Ein Bild mit einer kaputten Skizzenformel lädt (P3); die Skizze zeigt den Fehler, das
    // Maß behält seinen Wert, andere Parameteränderungen werden geduldet.
    let (d, b, s, e, r) = rect_doc();
    let mut snap = d.snapshot();
    let idx = snap.features.iter().position(|f| f.id == s).unwrap();
    let FeatureKind::Sketch(sf) = &mut snap.features[idx].kind else {
        panic!("keine Skizze")
    };
    sf.formulas.insert(r.right, "Gibtsnicht".into());
    let mut d = Document::from_snapshot(snap, 0.05).unwrap();
    assert_eq!(
        status(&d, s),
        FeatureStatus::Error("Den Namen „Gibtsnicht“ gibt es nicht.".into())
    );
    assert!(matches!(status(&d, e), FeatureStatus::Skipped(_)));
    assert_eq!(stored(&d, s, r.right), 20.0, "Maß behält seinen Wert");
    assert_eq!(stored(&d, s, r.bottom), 30.0);
    d.check_invariants().unwrap();
    // andere Parameteränderung geduldet (der alte Fehler blockiert nicht)
    let other = d.add_parameter("Andere", "1", Length, "").unwrap();
    d.set_parameter(other, edit(&d, other, "2")).unwrap();
    d.set_parameter(b, edit(&d, b, "35")).unwrap();
    assert_eq!(stored(&d, s, r.bottom), 35.0);
    assert!(matches!(status(&d, s), FeatureStatus::Error(_)));
    // gültige Formel → Ok
    d.update(s, |k| {
        if let FeatureKind::Sketch(sf) = k {
            sf.formulas.insert(r.right, "Breite/7*4".into());
        }
    })
    .unwrap();
    assert_eq!(status(&d, s), FeatureStatus::Ok);
    assert_eq!(status(&d, e), FeatureStatus::Ok);
    assert!((stored(&d, s, r.right) - 20.0).abs() <= 1e-12);
    d.check_invariants().unwrap();
}

#[test]
fn ellipse_angle_formula_in_degrees() {
    // F6: Winkel einer Ellipse in Grad durch das Feature (Bogenmaß in der Skizze).
    let mut d = Document::new();
    let w = d.add_parameter("w", "30", Angle, "").unwrap();
    let mut sk = Sketch::new(Frame::WORLD, "Skizze", Tolerance::default());
    let m = sk.add_point(0.0, 0.0).unwrap();
    let el = sk.add_ellipse(m, DVec2::new(20.0, 0.0), 10.0).unwrap();
    sk.add_constraint(ConstraintKind::Fixed(m, 0.0, 0.0))
        .unwrap();
    let ang = sk
        .add_constraint(ConstraintKind::EllipseAngle(el, 0.0))
        .unwrap();
    let s = d
        .add_feature("Skizze 1", FeatureKind::Sketch(on_xy(sk, one(ang, "w"))))
        .unwrap();
    assert_eq!(status(&d, s), FeatureStatus::Ok);
    let value = |d: &Document| match sketch_feature(d, s).sketch.constraint(ang).unwrap().kind {
        ConstraintKind::EllipseAngle(_, v) => v,
        ref k => panic!("{k:?}"),
    };
    assert_eq!(value(&d), 30f64.to_radians());
    let measure = |d: &Document| {
        let sk = d.sketch_of(s).unwrap();
        sk.measure(&sk.constraint(ang).unwrap().kind).unwrap()
    };
    assert!((measure(&d) - 30f64.to_radians()).abs() <= 1e-9);
    d.set_parameter(w, edit(&d, w, "60")).unwrap();
    assert_eq!(value(&d), 60f64.to_radians());
    assert!((measure(&d) - 60f64.to_radians()).abs() <= 1e-9);
    d.check_invariants().unwrap();
}
