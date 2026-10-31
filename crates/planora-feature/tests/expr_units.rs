//! Formelleser mit Einheiten: Zeichen hinter Zahlen, reine Zahl in der Anzeigeeinheit,
//! Umschreiben vor dem Speichern und Anzeige gespeicherter Formeln in Zoll.

use planora_feature::expr::{
    ParamUnit, display_formula, eval_text, eval_text_unit, rewrite_bare_lengths,
};
use planora_i18n::DisplayUnit::{Millimeter, Zoll};

/// `breite` ist eine Länge (10 mm), `anzahl` eine Zahl (3), `m` eine Länge (5 mm).
fn len(name: &str) -> Option<(f64, ParamUnit)> {
    match name.to_lowercase().as_str() {
        "breite" => Some((10.0, ParamUnit::Length)),
        "anzahl" => Some((3.0, ParamUnit::Number)),
        "m" => Some((5.0, ParamUnit::Length)),
        "w" => Some((30.0, ParamUnit::Angle)),
        _ => None,
    }
}

/// Wie [`len`], aber `breite` und `anzahl` sind reine Zahlen.
fn num(name: &str) -> Option<(f64, ParamUnit)> {
    match name.to_lowercase().as_str() {
        "breite" => Some((10.0, ParamUnit::Number)),
        "anzahl" => Some((3.0, ParamUnit::Number)),
        _ => None,
    }
}

fn close(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-9, "{a} != {b}");
}

#[test]
fn units_after_numbers() {
    close(eval_text("2 in", ParamUnit::Length, &len).unwrap(), 50.8);
    close(eval_text("50mm", ParamUnit::Length, &len).unwrap(), 50.0);
    close(
        eval_text("1 cm + 1 mm", ParamUnit::Length, &len).unwrap(),
        11.0,
    );
    close(eval_text("1 M", ParamUnit::Length, &len).unwrap(), 1000.0);
    close(eval_text("1,5IN", ParamUnit::Length, &len).unwrap(), 38.1);
    // Mit Zeichen zählt die Zahl nicht mehr in der Anzeigeeinheit.
    close(
        eval_text_unit("50 mm", ParamUnit::Length, &len, Zoll).unwrap(),
        50.0,
    );
    // Ein Parameter `m` bleibt ein Name.
    close(eval_text("m + 1", ParamUnit::Length, &len).unwrap(), 6.0);
    close(eval_text("2 * m", ParamUnit::Length, &len).unwrap(), 10.0);
    assert!(eval_text("2 inch", ParamUnit::Length, &len).is_err());
    assert!(eval_text("2 in in", ParamUnit::Length, &len).is_err());
}

#[test]
fn bare_numbers_count_in_display_unit() {
    let z = |t: &str, want| eval_text_unit(t, want, &len, Zoll).unwrap();
    close(z("2", ParamUnit::Length), 50.8);
    close(z("breite + 1", ParamUnit::Length), 35.4);
    close(z("breite * 2", ParamUnit::Length), 20.0);
    close(z("2 * 3", ParamUnit::Length), 152.4);
    close(z("30", ParamUnit::Angle), 30.0);
    close(z("min(breite; 1)", ParamUnit::Length), 10.0);
    close(z("max(breite; 1)", ParamUnit::Length), 25.4);
    close(z("-1 + breite", ParamUnit::Length), -15.4);
    close(z("sin(30) * breite", ParamUnit::Length), 5.0);
    close(z("anzahl * 10", ParamUnit::Length), 762.0);
    close(
        eval_text_unit("breite + 1", ParamUnit::Length, &num, Zoll).unwrap(),
        11.0 * 25.4,
    );
    // Millimeter: wie bisher.
    close(
        eval_text_unit("breite + 1", ParamUnit::Length, &len, Millimeter).unwrap(),
        11.0,
    );
}

#[test]
fn rewrite_marks_bare_lengths() {
    let rw = |t: &str, look: &dyn Fn(&str) -> Option<(f64, ParamUnit)>| {
        rewrite_bare_lengths(t, ParamUnit::Length, look, Zoll).unwrap()
    };
    assert_eq!(rw("breite + 1", &len), "breite + 1 in");
    assert_eq!(rw("2^2 + breite", &len), "(2^2) * 1 in + breite");
    assert_eq!(rw("2", &len), "2 in");
    assert_eq!(rw("-5", &len), "-5 in");
    assert_eq!(rw("breite * 2", &len), "breite * 2");
    assert_eq!(rw("min(breite;  1)", &len), "min(breite;  1 in)");
    assert_eq!(rw("breite + 1", &num), "(breite + 1) * 1 in");
    assert_eq!(rw("anzahl * 10", &len), "(anzahl * 10) * 1 in");
    assert_eq!(rw("breite*10", &len), "breite*10");
    assert_eq!(
        rewrite_bare_lengths("breite + 1", ParamUnit::Length, &len, Millimeter).unwrap(),
        "breite + 1"
    );
    assert_eq!(
        rewrite_bare_lengths("30", ParamUnit::Angle, &len, Zoll).unwrap(),
        "30"
    );
    assert!(rewrite_bare_lengths("2 +", ParamUnit::Length, &len, Zoll).is_err());
    assert!(rewrite_bare_lengths("unbekannt", ParamUnit::Length, &len, Zoll).is_err());
}

#[test]
fn rewrite_keeps_value() {
    let cases: [(&str, ParamUnit); 14] = [
        ("2", ParamUnit::Length),
        ("breite + 1", ParamUnit::Length),
        ("2^2 + breite", ParamUnit::Length),
        ("breite * 2 - 3", ParamUnit::Length),
        ("-(2) + breite", ParamUnit::Length),
        ("(1 + 2) + breite", ParamUnit::Length),
        ("max(1; breite; 2 * 3)", ParamUnit::Length),
        ("(breite + 1) / breite + 2", ParamUnit::Length),
        ("anzahl * 10", ParamUnit::Length),
        ("breite / (breite + 1)", ParamUnit::Number),
        ("abs(-3) + 1 mm", ParamUnit::Length),
        ("pi + breite", ParamUnit::Length),
        ("w + 5", ParamUnit::Angle),
        ("1,5 + breite", ParamUnit::Length),
    ];
    for look in [&len as &dyn Fn(&str) -> Option<(f64, ParamUnit)>, &num] {
        for (t, want) in cases {
            let Ok(expected) = eval_text_unit(t, want, look, Zoll) else {
                continue;
            };
            let stored = rewrite_bare_lengths(t, want, look, Zoll).unwrap();
            let got = eval_text(&stored, want, look).unwrap();
            assert!(
                (got - expected).abs() < 1e-9,
                "{t} -> {stored}: {got} != {expected}"
            );
        }
    }
}

#[test]
fn display_in_inch() {
    assert_eq!(
        display_formula("50", ParamUnit::Length, &len, Zoll),
        "1,968504"
    );
    assert_eq!(display_formula("25,4", ParamUnit::Length, &len, Zoll), "1");
    assert_eq!(
        display_formula("breite + 5", ParamUnit::Length, &len, Zoll),
        "breite + 5 mm"
    );
    assert_eq!(
        display_formula("breite + 5", ParamUnit::Length, &len, Millimeter),
        "breite + 5"
    );
    assert_eq!(display_formula("30", ParamUnit::Angle, &len, Zoll), "30");
    assert_eq!(display_formula("2 +", ParamUnit::Length, &len, Zoll), "2 +");
    // Angezeigter Text bedeutet wieder denselben Wert.
    let shown = display_formula("2^2 + breite", ParamUnit::Length, &len, Zoll);
    assert_eq!(shown, "(2^2) * 1 mm + breite");
    close(
        eval_text_unit(&shown, ParamUnit::Length, &len, Zoll).unwrap(),
        14.0,
    );
}
