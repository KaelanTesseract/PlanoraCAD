//! Parametertabelle im Dokument (Aufgabe 2): Anlegen, Ändern, Umbenennen, Löschen, Ersetzen
//! durch den Wert, Ablehnung ohne Folgen, Zustandsbild, Zurückladen und Laden.

use planora_feature::*;
use planora_math::DVec3;

use ParamUnit::{Angle, Length};

fn breite_tiefe() -> (Document, ParamId, ParamId) {
    let mut d = Document::new();
    let b = d.add_parameter("Breite", "40", Length, "Außenmaß").unwrap();
    let t = d.add_parameter("Tiefe", "Breite/2", Length, "").unwrap();
    (d, b, t)
}

fn edit_of(p: &Parameter) -> ParamEdit {
    ParamEdit {
        name: p.name.clone(),
        formula: p.formula.clone(),
        unit: p.unit,
        comment: p.comment.clone(),
    }
}

fn names(d: &Document) -> Vec<String> {
    d.parameters().iter().map(|p| p.name.clone()).collect()
}

fn formula_err(r: Result<impl std::fmt::Debug, FeatureError>) -> String {
    match r {
        Err(FeatureError::Formula(m)) => m,
        other => panic!("Formula-Fehler erwartet, bekam {other:?}"),
    }
}

fn param(id: u64, name: &str, formula: &str) -> Parameter {
    Parameter {
        id: ParamId(id),
        name: name.into(),
        formula: formula.into(),
        unit: Length,
        comment: String::new(),
    }
}

#[test]
fn add_and_read() {
    let mut d = Document::new();
    let e0 = d.edit_count();
    let b = d.add_parameter("Breite", "40", Length, "Außenmaß");
    assert_eq!(b, Ok(ParamId(1)));
    assert_eq!(d.edit_count(), e0 + 1);
    assert_eq!(d.model_edit_count(), e0 + 1);
    assert_eq!(d.parameter_value(ParamId(1)), Some(40.0));
    let t = d.add_parameter("Tiefe", "Breite/2", Length, "").unwrap();
    assert_eq!(t, ParamId(2));
    assert_eq!(d.edit_count(), e0 + 2);
    assert_eq!(d.parameter_value(t), Some(20.0));
    assert_eq!(
        d.parameter(ParamId(1)),
        Some(&Parameter {
            id: ParamId(1),
            name: "Breite".into(),
            formula: "40".into(),
            unit: Length,
            comment: "Außenmaß".into(),
        })
    );
    assert_eq!(d.parameter(ParamId(3)), None);
    assert_eq!(d.parameter_value(ParamId(3)), None);
    let scope = d.param_scope();
    assert_eq!(scope.lookup("TIEFE"), Some((20.0, Length)));
    assert_eq!(scope.lookup("breite"), Some((40.0, Length)));
    assert_eq!(scope.lookup("Weite"), None);
    assert_eq!(
        scope.names(),
        vec!["Breite".to_string(), "Tiefe".to_string()]
    );
    assert_eq!(scope.eval("tiefe*2+1", Length), Ok(41.0));
    assert_eq!(
        scope.eval("Breite", Angle),
        Err("Hier wird ein Winkel erwartet, die Formel ergibt eine Länge.".to_string())
    );
    assert_eq!(ParamScope::default().names(), Vec::<String>::new());
    assert_eq!(d.check_invariants(), Ok(()));
}

#[test]
fn rejected_edits_change_nothing() {
    let (mut d, b, _t) = breite_tiefe();
    let before = d.snapshot();
    let e0 = d.edit_count();
    let m0 = d.model_edit_count();
    let unchanged = |d: &Document, what: &str| {
        assert!(d.snapshot().same_content(&before), "{what}: Bild geändert");
        assert_eq!(d.edit_count(), e0, "{what}: Zähler");
        assert_eq!(d.model_edit_count(), m0, "{what}: Modellzähler");
        assert_eq!(d.parameter_value(b), Some(40.0), "{what}: Wert");
        assert_eq!(d.parameter_value(ParamId(2)), Some(20.0), "{what}: Wert");
    };

    assert_eq!(
        d.add_parameter("breite", "1", Length, ""),
        Err(FeatureError::Formula(
            "Den Namen „breite“ gibt es schon.".into()
        ))
    );
    unchanged(&d, "doppelter Name");

    let mut e = edit_of(d.parameter(b).unwrap());
    e.formula = "Tiefe+1".into();
    let m = formula_err(d.set_parameter(b, e));
    assert!(m.contains("im Kreis"), "{m}");
    unchanged(&d, "Kreis");

    let mut e = edit_of(d.parameter(b).unwrap());
    e.unit = Angle;
    assert_eq!(
        d.set_parameter(b, e).unwrap_err(),
        FeatureError::Formula(
            "Parameter „Tiefe“: Hier wird eine Länge erwartet, die Formel ergibt einen Winkel."
                .into()
        )
    );
    unchanged(&d, "Einheit");

    let mut e = edit_of(d.parameter(b).unwrap());
    e.name = "1x".into();
    assert_eq!(
        formula_err(d.set_parameter(b, e)),
        "Ein Name beginnt mit einem Buchstaben oder „_“ und enthält nur Buchstaben, Ziffern und „_“."
    );
    unchanged(&d, "Name 1x");

    let long = format!("{}1", "1+".repeat(100));
    assert_eq!(long.chars().count(), 201);
    assert_eq!(
        formula_err(d.add_parameter("Lang", &long, Length, "")),
        "Parameter „Lang“: Die Formel ist zu lang (höchstens 200 Zeichen)."
    );
    unchanged(&d, "Formel 201");

    let mut e = edit_of(d.parameter(b).unwrap());
    e.comment = "ä".repeat(201);
    assert_eq!(
        formula_err(d.set_parameter(b, e)),
        "Der Kommentar ist zu lang (höchstens 200 Zeichen)."
    );
    unchanged(&d, "Kommentar 201");
    assert_eq!(
        formula_err(d.add_parameter("Neu", "1", Length, &"x".repeat(201))),
        "Der Kommentar ist zu lang (höchstens 200 Zeichen)."
    );
    unchanged(&d, "Kommentar 201 beim Anlegen");

    assert_eq!(
        d.set_parameter(ParamId(99), edit_of(d.parameter(b).unwrap()))
            .unwrap_err(),
        FeatureError::InvalidInput("Den Parameter gibt es in diesem Dokument nicht".into())
    );
    assert!(d.remove_parameter(ParamId(99)).is_err());
    assert!(d.replace_parameter_by_value(ParamId(99)).is_err());
    unchanged(&d, "unbekannte Kennung");

    // Grenzfälle, die gelingen: 200 Zeichen Kommentar, 200 Zeichen Formel.
    let mut e = edit_of(d.parameter(b).unwrap());
    e.comment = "ä".repeat(200);
    d.set_parameter(b, e).unwrap();
    assert_eq!(d.edit_count(), e0 + 1);
    let ok200 = format!("{}1", "1+".repeat(99));
    assert_eq!(ok200.chars().count(), 199);
    d.add_parameter("Lang", &format!("{ok200} "), Length, "")
        .unwrap();

    // 501. Parameter
    let mut d = Document::new();
    for i in 0..500 {
        d.add_parameter(&format!("p{i}"), "1", Length, "").unwrap();
    }
    let before = d.snapshot();
    let e0 = d.edit_count();
    assert_eq!(
        formula_err(d.add_parameter("p500", "1", Length, "")),
        "Höchstens 500 Parameter."
    );
    assert_eq!(
        formula_err(d.add_parameter_default()),
        "Höchstens 500 Parameter."
    );
    assert!(d.snapshot().same_content(&before));
    assert_eq!(d.edit_count(), e0);
}

#[test]
fn rename_rewrites_parameter_formulas() {
    let (mut d, b, t) = breite_tiefe();
    let x = d.add_parameter("Breitex", "3", Length, "").unwrap();
    let y = d
        .add_parameter("Summe", "breite + Breitex + BREITE*2", Length, "")
        .unwrap();
    let e0 = d.edit_count();
    let mut e = edit_of(d.parameter(b).unwrap());
    e.name = "Weite".into();
    d.set_parameter(b, e).unwrap();
    assert_eq!(d.edit_count(), e0 + 1);
    assert_eq!(d.parameter(b).unwrap().name, "Weite");
    assert_eq!(d.parameter(t).unwrap().formula, "Weite/2");
    assert_eq!(d.parameter(x).unwrap().formula, "3");
    assert_eq!(d.parameter(y).unwrap().formula, "Weite + Breitex + Weite*2");
    assert_eq!(d.parameter_value(b), Some(40.0));
    assert_eq!(d.parameter_value(t), Some(20.0));
    assert_eq!(d.parameter_value(y), Some(123.0));
    assert_eq!(d.param_scope().lookup("breite"), None);

    // Umbenennen und neue Formel in einem Schritt.
    let e0 = d.edit_count();
    let mut e = edit_of(d.parameter(b).unwrap());
    e.name = "B".into();
    e.formula = "50".into();
    d.set_parameter(b, e).unwrap();
    assert_eq!(d.edit_count(), e0 + 1);
    assert_eq!(d.parameter(t).unwrap().formula, "B/2");
    assert_eq!(d.parameter_value(t), Some(25.0));

    // Umbenennen auf einen vorhandenen Namen: abgelehnt, nichts umgeschrieben.
    let before = d.snapshot();
    let mut e = edit_of(d.parameter(b).unwrap());
    e.name = "tiefe".into();
    assert_eq!(
        formula_err(d.set_parameter(b, e)),
        "Den Namen „tiefe“ gibt es schon."
    );
    assert!(d.snapshot().same_content(&before));
    assert_eq!(d.parameter(t).unwrap().formula, "B/2");
    assert_eq!(d.edit_count(), e0 + 1);
}

#[test]
fn remove_blocked_and_replace() {
    let (mut d, b, t) = breite_tiefe();
    assert_eq!(d.parameter_users(b), vec!["Parameter Tiefe".to_string()]);
    assert_eq!(d.parameter_users(t), Vec::<String>::new());
    assert_eq!(d.parameter_users(ParamId(77)), Vec::<String>::new());
    let before = d.snapshot();
    let e0 = d.edit_count();
    let err = d.remove_parameter(b).unwrap_err();
    assert_eq!(
        err,
        FeatureError::ParameterInUse {
            name: "Breite".into(),
            users: vec!["Parameter Tiefe".into()],
        }
    );
    assert_eq!(err.to_string(), "Breite wird in Parameter Tiefe benutzt.");
    assert!(d.snapshot().same_content(&before));
    assert_eq!(d.edit_count(), e0);

    // Zwei Benutzer, in Tabellenreihenfolge.
    let h = d.add_parameter("Hoehe", "breite*2", Length, "").unwrap();
    assert_eq!(
        d.remove_parameter(b).unwrap_err().to_string(),
        "Breite wird in Parameter Tiefe, Parameter Hoehe benutzt."
    );

    // Ersetzen: ein Schritt, Werte gleich, Breite weg.
    let u = d.add_parameter("Drittel", "Breite/3", Length, "").unwrap();
    let third = d.parameter_value(u).unwrap();
    let e0 = d.edit_count();
    let m0 = d.model_edit_count();
    d.replace_parameter_by_value(b).unwrap();
    assert_eq!(d.edit_count(), e0 + 1);
    assert_eq!(d.model_edit_count(), m0 + 1);
    assert_eq!(d.parameter(b), None);
    assert_eq!(d.parameter_value(b), None);
    assert_eq!(d.parameter(t).unwrap().formula, "20");
    assert_eq!(d.parameter_value(t), Some(20.0));
    assert_eq!(d.parameter(h).unwrap().formula, "80");
    assert_eq!(d.parameter_value(u).unwrap().to_bits(), third.to_bits());
    assert_eq!(
        d.parameter(u).unwrap().formula,
        planora_feature::expr::number_text(40.0 / 3.0)
    );
    assert_eq!(names(&d), vec!["Tiefe", "Hoehe", "Drittel"]);

    // Unbenutzten Parameter löschen.
    let e0 = d.edit_count();
    d.remove_parameter(h).unwrap();
    assert_eq!(d.edit_count(), e0 + 1);
    assert_eq!(names(&d), vec!["Tiefe", "Drittel"]);
    assert_eq!(d.parameter(h), None);

    // Ersetzen eines unbenutzten Parameters = Löschen, ein Schritt.
    let e0 = d.edit_count();
    d.replace_parameter_by_value(u).unwrap();
    assert_eq!(d.edit_count(), e0 + 1);
    assert_eq!(names(&d), vec!["Tiefe"]);

    // Kennungen werden nicht wiederverwendet.
    let n = d.add_parameter("Neu", "1", Length, "").unwrap();
    assert_eq!(n, ParamId(5));
    assert_eq!(d.check_invariants(), Ok(()));
}

#[test]
fn replace_refuses_value_too_long() {
    let mut d = Document::new();
    let g = d
        .add_parameter("g", "10^250", ParamUnit::Number, "")
        .unwrap();
    let h = d.add_parameter("h", "g*1", ParamUnit::Number, "").unwrap();
    let before = d.snapshot();
    let e0 = d.edit_count();
    assert_eq!(
        formula_err(d.replace_parameter_by_value(g)),
        "Parameter „h“: Der Wert lässt sich nicht als Zahl mit höchstens 200 Zeichen schreiben."
    );
    assert!(d.snapshot().same_content(&before));
    assert_eq!(d.edit_count(), e0);
    assert_eq!(d.parameter(h).unwrap().formula, "g*1");
}

#[test]
fn snapshot_roundtrip_and_restore() {
    let (mut d, b, t) = breite_tiefe();
    d.add_feature(
        "Quader 1",
        FeatureKind::Box {
            size: DVec3::splat(10.0),
            origin: DVec3::ZERO,
        },
    )
    .unwrap();
    let a = d.snapshot();
    assert_eq!(a.parameters, d.parameters().to_vec());
    assert_eq!(a.next_param, 3);

    let loaded = Document::from_snapshot(a.clone(), 0.05).unwrap();
    assert_eq!(loaded.parameters(), d.parameters());
    assert_eq!(loaded.parameter_value(b), Some(40.0));
    assert_eq!(loaded.parameter_value(t), Some(20.0));
    assert!(loaded.snapshot().same_content(&a));
    assert_eq!(loaded.edit_count(), 0);
    assert_eq!(loaded.check_invariants(), Ok(()));
    // Der Kennungszähler kommt mit: keine wiederverwendete Kennung.
    let mut loaded = loaded;
    assert_eq!(loaded.add_parameter("Neu", "1", Length, ""), Ok(ParamId(3)));

    let mut e = edit_of(d.parameter(b).unwrap());
    e.formula = "60".into();
    d.set_parameter(b, e).unwrap();
    assert_eq!(d.parameter_value(t), Some(30.0));
    assert!(!d.snapshot().same_model(&a));
    assert!(!d.snapshot().same_content(&a));
    let e0 = d.edit_count();
    let m0 = d.model_edit_count();
    assert_eq!(d.restore(a.clone()), Ok(RestoreKind::Full));
    assert_eq!(d.parameter(b).unwrap().formula, "40");
    assert_eq!(d.parameter_value(b), Some(40.0));
    assert_eq!(d.parameter_value(t), Some(20.0));
    assert!(d.snapshot().same_content(&a));
    assert!(d.edit_count() > e0);
    assert!(d.model_edit_count() > m0);
    assert_eq!(d.check_invariants(), Ok(()));

    // Nur der Kommentar anders: trotzdem voller Weg (Tabelle gehört zum Modell).
    let mut e = edit_of(d.parameter(b).unwrap());
    e.comment = "anders".into();
    d.set_parameter(b, e).unwrap();
    assert!(!d.snapshot().same_model(&a));
    let m0 = d.model_edit_count();
    assert_eq!(d.restore(a.clone()), Ok(RestoreKind::Full));
    assert!(d.model_edit_count() > m0);
    assert_eq!(d.parameter(b).unwrap().comment, "Außenmaß");

    // Nur der Kennungszähler anders: ebenfalls ungleich.
    let mut c = a.clone();
    c.next_param = 7;
    assert!(!c.same_model(&a));
    assert_eq!(d.restore(c), Ok(RestoreKind::Full));
    assert_eq!(d.add_parameter("Neu", "1", Length, ""), Ok(ParamId(7)));

    // Gleiches Bild: Zeichnungsweg, Tabelle bleibt.
    let same = d.snapshot();
    assert_eq!(d.restore(same), Ok(RestoreKind::Annotations));
    assert_eq!(names(&d), vec!["Breite", "Tiefe", "Neu"]);
}

#[test]
fn restore_rejects_bad_table_and_keeps_document() {
    let (mut d, _b, _t) = breite_tiefe();
    let before = d.snapshot();
    let e0 = d.edit_count();
    let mut bad = before.clone();
    bad.parameters[1].formula = "Tiefe+1".into();
    assert!(d.restore(bad).is_err());
    assert!(d.snapshot().same_content(&before));
    assert_eq!(d.edit_count(), e0);
}

#[test]
fn build_rejects_invalid_table() {
    let snap = |params: Vec<Parameter>, next: u64| {
        let mut s = Document::new().snapshot();
        s.parameters = params;
        s.next_param = next;
        s
    };
    let err = |s: Snapshot| match Document::from_snapshot(s, 0.05) {
        Ok(_) => panic!("Fehler erwartet"),
        Err(e) => e.to_string(),
    };

    let m = err(snap(vec![param(1, "a", "1"), param(2, "A", "2")], 3));
    assert!(
        m.contains("Parametertabelle") && m.contains("gibt es schon"),
        "{m}"
    );
    assert_eq!(
        m,
        "Ungültige Eingabe: Parametertabelle: Den Namen „A“ gibt es schon."
    );

    let m = err(snap(vec![param(1, "a", "b+1"), param(2, "b", "a")], 3));
    assert!(
        m.contains("Parametertabelle") && m.contains("im Kreis"),
        "{m}"
    );

    let m = err(snap(vec![param(1, "a", "1+")], 2));
    assert_eq!(
        m,
        "Ungültige Eingabe: Parametertabelle: Parameter „a“: Die Formel endet unerwartet."
    );

    let mut long = param(1, "a", "1");
    long.comment = "x".repeat(201);
    let m = err(snap(vec![long], 2));
    assert!(
        m.contains("Parametertabelle") && m.contains("Kommentar"),
        "{m}"
    );

    for (params, next) in [
        (vec![param(0, "a", "1")], 3),
        (vec![param(3, "a", "1")], 3),
        (vec![param(4, "a", "1")], 3),
    ] {
        assert_eq!(
            err(snap(params, next)),
            "Ungültige Eingabe: Ein Parameter hat eine ungültige Kennung"
        );
    }
    assert_eq!(
        err(snap(vec![param(1, "a", "1"), param(1, "b", "2")], 3)),
        "Ungültige Eingabe: Eine Parameter-Kennung kommt doppelt vor"
    );
    assert_eq!(
        err(snap(vec![], 0)),
        "Ungültige Eingabe: Der Parameterzähler ist ungültig"
    );
    assert_eq!(
        err(snap(vec![], u64::MAX)),
        "Ungültige Eingabe: Der Parameterzähler ist ungültig"
    );

    // Gültig: Werte werden beim Laden berechnet.
    let d = Document::from_snapshot(
        snap(vec![param(2, "b", "a*2"), param(5, "a", "3")], 6),
        0.05,
    )
    .unwrap();
    assert_eq!(d.parameter_value(ParamId(2)), Some(6.0));
    assert_eq!(d.parameter_value(ParamId(5)), Some(3.0));
}

#[test]
fn no_op_edit_counts_nothing() {
    let (mut d, b, t) = breite_tiefe();
    d.add_feature(
        "Quader 1",
        FeatureKind::Box {
            size: DVec3::splat(10.0),
            origin: DVec3::ZERO,
        },
    )
    .unwrap();
    let e0 = d.edit_count();
    let m0 = d.model_edit_count();
    let before = d.snapshot();
    for id in [b, t] {
        let report = d
            .set_parameter(id, edit_of(d.parameter(id).unwrap()))
            .unwrap();
        assert!(report.computed.is_empty() && report.reused.is_empty());
    }
    assert_eq!(d.edit_count(), e0);
    assert_eq!(d.model_edit_count(), m0);
    assert!(d.snapshot().same_content(&before));

    // Eine echte Änderung zählt genau einmal.
    let mut e = edit_of(d.parameter(t).unwrap());
    e.comment = "neu".into();
    d.set_parameter(t, e).unwrap();
    assert_eq!(d.edit_count(), e0 + 1);
    assert_eq!(d.model_edit_count(), m0 + 1);
    // Nur Groß-/Kleinschreibung des Namens: auch eine Änderung.
    let mut e = edit_of(d.parameter(b).unwrap());
    e.name = "BREITE".into();
    d.set_parameter(b, e).unwrap();
    assert_eq!(d.edit_count(), e0 + 2);
    assert_eq!(d.parameter(t).unwrap().formula, "BREITE/2");
}

#[test]
fn default_rows() {
    let mut d = Document::new();
    let p1 = d.add_parameter_default().unwrap();
    let p2 = d.add_parameter_default().unwrap();
    assert_eq!(
        d.parameter(p1),
        Some(&Parameter {
            id: p1,
            name: "Parameter1".into(),
            formula: "10".into(),
            unit: Length,
            comment: String::new(),
        })
    );
    assert_eq!(d.parameter(p2).unwrap().name, "Parameter2");
    assert_eq!(d.parameter_value(p2), Some(10.0));
    assert_eq!(d.edit_count(), 2);
    d.remove_parameter(p1).unwrap();
    let again = d.add_parameter_default().unwrap();
    assert_eq!(d.parameter(again).unwrap().name, "Parameter1");
    assert_eq!(again, ParamId(3));

    let mut d = Document::new();
    d.add_parameter("parameter3", "1", Length, "").unwrap();
    let names_after: Vec<String> = (0..3)
        .map(|_| {
            let id = d.add_parameter_default().unwrap();
            d.parameter(id).unwrap().name.clone()
        })
        .collect();
    assert_eq!(names_after, vec!["Parameter1", "Parameter2", "Parameter4"]);
    d.add_parameter("PARAMETER5", "1", Length, "").unwrap();
    let id = d.add_parameter_default().unwrap();
    assert_eq!(d.parameter(id).unwrap().name, "Parameter6");
}
