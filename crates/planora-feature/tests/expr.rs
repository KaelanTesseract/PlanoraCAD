//! Formelbaustein für Parameter: Rangfolge, Funktionen, Komma, Einheiten, Fehlermeldungen,
//! Grenzen, Namensregeln, Umbenennen und Auswertung der Tabelle.

use planora_feature::expr::{
    ExprError, ParamDef, ParamUnit, TableError, check_name, eval_text, evaluate_table, number_text,
    parse, plain_number, references, rename_in, same_name, short_number,
};
use std::f64::consts::PI;

fn ev(t: &str) -> f64 {
    eval_text(t, ParamUnit::Number, &|_| None).unwrap()
}

fn ev_err(t: &str) -> String {
    eval_text(t, ParamUnit::Number, &|_| None).unwrap_err().0
}

fn sc(name: &str) -> Option<(f64, ParamUnit)> {
    match name.to_lowercase().as_str() {
        "breite" => Some((40.0, ParamUnit::Length)),
        "w" => Some((30.0, ParamUnit::Angle)),
        "k" => Some((2.0, ParamUnit::Number)),
        _ => None,
    }
}

fn sc_ok(t: &str, want: ParamUnit) -> f64 {
    eval_text(t, want, &sc).unwrap()
}

fn sc_err(t: &str, want: ParamUnit) -> String {
    eval_text(t, want, &sc).unwrap_err().0
}

const MIX: &str = "Länge und Winkel lassen sich nicht zusammenrechnen.";
const TWO_VALUES_MIN: &str =
    "„min“ braucht mindestens zwei Werte (Werte mit „;“ oder „, “ trennen).";

#[test]
fn precedence_and_associativity() {
    assert_eq!(ev("2+3*4"), 14.0);
    assert_eq!(ev("(2+3)*4"), 20.0);
    assert_eq!(ev("10-4-3"), 3.0);
    assert_eq!(ev("8/4/2"), 1.0);
    assert_eq!(ev("2^3^2"), 512.0);
    assert_eq!(ev("-2^2"), -4.0);
    assert_eq!(ev("(-2)^2"), 4.0);
    assert_eq!(ev("2^-1"), 0.5);
    assert_eq!(ev("--3"), 3.0);
    assert_eq!(ev("2*-3"), -6.0);
    assert_eq!(ev(" 2 * ( 3 + 4 ) "), 14.0);
    // Zusätzlich: Punkt vor Strich auch bei Division, Potenz vor Punkt.
    assert_eq!(ev("1+6/3"), 3.0);
    assert_eq!(ev("2*3^2"), 18.0);
    assert_eq!(ev("2^-2^2"), 0.0625);
    assert_eq!(ev("10-2+3"), 11.0);
    assert_eq!(ev("12/3*2"), 8.0);
}

#[test]
fn functions_pi_and_commas() {
    assert_eq!(ev("sqrt(16)"), 4.0);
    assert_eq!(ev("abs(-2,5)"), 2.5);
    assert_eq!(ev("min(3; 1)"), 1.0);
    assert_eq!(ev("max(3, 1)"), 3.0);
    assert_eq!(ev("min(4; 2; 3)"), 2.0);
    assert_eq!(ev("max(4; 7; 3)"), 7.0);
    assert_eq!(ev("round(2,5)"), 3.0);
    assert_eq!(ev("round(-2,5)"), -3.0);
    assert_eq!(ev("round(2,4)"), 2.0);
    assert!((ev("sin(30)") - 0.5).abs() < 1e-12);
    assert!((ev("cos(60)") - 0.5).abs() < 1e-12);
    assert!((ev("tan(45)") - 1.0).abs() < 1e-12);
    assert_eq!(ev("pi"), PI);
    assert_eq!(ev("PI"), PI);
    assert_eq!(ev("SQRT(9)"), 3.0);
    assert_eq!(ev_err("min(1,5)"), TWO_VALUES_MIN);
    assert_eq!(ev_err("sin(1; 2)"), "„sin“ braucht genau einen Wert.");
    assert_eq!(ev_err("sqrt()"), "„sqrt“ braucht genau einen Wert.");
    assert_eq!(
        ev_err("max(2)"),
        "„max“ braucht mindestens zwei Werte (Werte mit „;“ oder „, “ trennen)."
    );
    // Grad, nicht Bogenmaß, auch für beliebige Winkel und über eine Umdrehung hinaus.
    assert!((ev("sin(10)") - 10f64.to_radians().sin()).abs() < 1e-15);
    assert!((ev("cos(10)") - 10f64.to_radians().cos()).abs() < 1e-15);
    assert!((ev("tan(10)") - 10f64.to_radians().tan()).abs() < 1e-15);
    assert!((ev("sin(100)") - 100f64.to_radians().sin()).abs() < 1e-15);
    // Glatte Winkel ergeben genaue Werte.
    assert_eq!(ev("sin(30)"), 0.5);
    assert_eq!(ev("cos(60)"), 0.5);
    assert_eq!(ev("tan(45)"), 1.0);
    assert_eq!(ev("cos(90)"), 0.0);
    assert_eq!(ev("sin(180)"), 0.0);
    assert!(ev("sin(180)").is_sign_positive());
    assert_eq!(ev("cos(120)"), -0.5);
    assert_eq!(ev("sin(-30)"), -0.5);
    assert_eq!(ev("sin(390)"), 0.5);
    assert_eq!(ev("cos(270)"), 0.0);
    assert_eq!(ev("sin(270)"), -1.0);
    assert_eq!(
        ev_err("tan(90)"),
        "Das Ergebnis ist zu groß oder keine Zahl."
    );
    // Komma zwischen Ziffern ist Dezimalzeichen, sonst trennt es Werte.
    assert_eq!(ev("min(1, 5)"), 1.0);
    assert_eq!(ev("min(1 ,5)"), 1.0);
    assert_eq!(ev("max(1,5; 2)"), 2.0);
    assert_eq!(ev("max(1,5;1)"), 1.5);
    assert_eq!(ev_err("1,5,3"), "Unerwartetes Zeichen „,“ an Stelle 4.");
    assert_eq!(ev_err("1;2"), "Unerwartetes Zeichen „;“ an Stelle 2.");
}

#[test]
fn numbers_round_trip() {
    assert_eq!(ev("1,5+1.5"), 3.0);
    assert_eq!(ev(".5*4"), 2.0);
    assert_eq!(ev("0,25*4"), 1.0);
    assert_eq!(plain_number("2,5"), Some(2.5));
    assert_eq!(plain_number(" -4 "), Some(-4.0));
    assert_eq!(plain_number("1e3"), Some(1000.0));
    assert_eq!(plain_number("2*3"), None);
    assert_eq!(plain_number("inf"), None);
    assert_eq!(plain_number("NaN"), None);
    assert_eq!(plain_number(""), None);
    for v in [0.1, 1.0 / 3.0, -12.5, 1e-7, 123456.789, 0.0] {
        let back = plain_number(&number_text(v)).unwrap();
        assert_eq!(back.to_bits(), v.to_bits(), "{v}");
        // Der Zahltext ist auch eine gültige Formel mit demselben Wert.
        assert_eq!(ev(&number_text(v)).to_bits(), v.to_bits(), "{v}");
    }
    assert_eq!(number_text(2.5), "2,5");
    assert_eq!(number_text(40.0), "40");
    assert_eq!(short_number(0.0), "0");
    assert_eq!(short_number(-0.0), "0");
    assert_eq!(short_number(12.5), "12,5");
    assert_eq!(short_number(1.0 / 3.0), "0,33");
    assert_eq!(short_number(-0.001), "0");
    assert_eq!(short_number(40.0), "40");
    assert_eq!(short_number(-2.25), "-2,25");
    assert_eq!(short_number(2.999), "3");
    assert_eq!(short_number(100.0), "100");
}

#[test]
fn units() {
    use ParamUnit::{Angle, Length, Number};
    assert_eq!(sc_ok("Breite/4", Length), 10.0);
    assert_eq!(sc_ok("breite*k", Length), 80.0);
    assert_eq!(sc_ok("Breite+2", Length), 42.0);
    assert_eq!(sc_ok("2+Breite", Length), 42.0);
    assert_eq!(sc_ok("Breite-k", Length), 38.0);
    assert_eq!(sc_ok("sqrt(Breite^2 + 30^2)", Length), 50.0);
    assert!((sc_ok("sin(w)*Breite", Length) - 20.0).abs() < 1e-12);
    assert_eq!(sc_ok("w*2", Angle), 60.0);
    assert_eq!(sc_ok("Breite/Breite", Number), 1.0);
    assert_eq!(sc_ok("k", Length), 2.0);
    assert_eq!(sc_ok("k", Angle), 2.0);
    assert_eq!(sc_ok("Breite*Breite/Breite", Length), 40.0);
    assert_eq!(sc_ok("min(Breite; 5)", Length), 5.0);
    assert_eq!(sc_ok("max(3; w)", Angle), 30.0);
    assert_eq!(sc_ok("abs(-Breite)", Length), 40.0);
    assert_eq!(sc_ok("round(Breite/3)", Length), 13.0);
    assert_eq!(sc_ok("Breite^-1*Breite", Number), 1.0);
    assert_eq!(sc_ok("cos(w*2)", Number), 0.5);
    assert_eq!(
        sc_err("w", Length),
        "Hier wird eine Länge erwartet, die Formel ergibt einen Winkel."
    );
    assert_eq!(
        sc_err("Breite", Angle),
        "Hier wird ein Winkel erwartet, die Formel ergibt eine Länge."
    );
    assert_eq!(
        sc_err("Breite", Number),
        "Hier wird eine Zahl erwartet, die Formel ergibt eine Länge."
    );
    assert_eq!(
        sc_err("w", Number),
        "Hier wird eine Zahl erwartet, die Formel ergibt einen Winkel."
    );
    assert_eq!(
        sc_err("Breite*Breite", Length),
        "Hier wird eine Länge erwartet, die Formel ergibt eine andere Einheit."
    );
    assert_eq!(
        sc_err("k/Breite", Length),
        "Hier wird eine Länge erwartet, die Formel ergibt eine andere Einheit."
    );
    assert_eq!(sc_err("Breite+w", Length), MIX);
    assert_eq!(sc_err("Breite-w", Length), MIX);
    assert_eq!(sc_err("min(Breite; w)", Length), MIX);
    assert_eq!(sc_err("max(w; 1; Breite)", Length), MIX);
    assert_eq!(sc_err("Breite + Breite^2", Length), MIX);
    assert_eq!(
        sc_err("sin(Breite)", Number),
        "„sin“ braucht einen Winkel oder eine Zahl."
    );
    assert_eq!(
        sc_err("tan(w*w)", Number),
        "„tan“ braucht einen Winkel oder eine Zahl."
    );
    assert_eq!(
        sc_err("sqrt(Breite)", Length),
        "„sqrt“ passt hier nicht zur Einheit."
    );
    assert_eq!(
        sc_err("Breite^1,5", Length),
        "Eine Länge oder ein Winkel lässt sich nur mit einer ganzen Zahl potenzieren."
    );
    assert_eq!(
        sc_err("2^Breite", Number),
        "Eine Länge oder ein Winkel lässt sich nur mit einer ganzen Zahl potenzieren."
    );
}

#[test]
fn errors_and_limits() {
    assert_eq!(ev_err("1/0"), "Division durch null.");
    assert_eq!(ev_err("1/(2-2)"), "Division durch null.");
    assert_eq!(
        ev_err("sqrt(-1)"),
        "Die Wurzel einer negativen Zahl gibt es nicht."
    );
    assert_eq!(
        ev_err("10^400"),
        "Das Ergebnis ist zu groß oder keine Zahl."
    );
    assert_eq!(ev_err("0^-1"), "Das Ergebnis ist zu groß oder keine Zahl.");
    assert_eq!(
        ev_err("(-8)^0,5"),
        "Das Ergebnis ist zu groß oder keine Zahl."
    );
    assert_eq!(ev_err(""), "Die Formel ist leer.");
    assert_eq!(ev_err("   "), "Die Formel ist leer.");
    assert_eq!(ev_err("2+"), "Die Formel endet unerwartet.");
    assert_eq!(ev_err("-"), "Die Formel endet unerwartet.");
    assert_eq!(ev_err("(2+3"), "Es fehlt eine schließende Klammer.");
    assert_eq!(ev_err("min(2; 3"), "Es fehlt eine schließende Klammer.");
    assert_eq!(ev_err("2+3)"), "Unerwartetes Zeichen „)“ an Stelle 4.");
    assert_eq!(ev_err("2 # 3"), "Unerwartetes Zeichen „#“ an Stelle 3.");
    assert_eq!(ev_err("2 3"), "Unerwartetes Zeichen „3“ an Stelle 3.");
    assert_eq!(ev_err("2*)"), "Unerwartetes Zeichen „)“ an Stelle 3.");
    assert_eq!(ev_err("1."), "Unerwartetes Zeichen „.“ an Stelle 2.");
    // Stelle zählt Zeichen, nicht Bytes („ß“ und „ö“ sind zwei Bytes lang).
    assert_eq!(
        eval_text("Maß + )", ParamUnit::Number, &sc).unwrap_err().0,
        "Unerwartetes Zeichen „)“ an Stelle 7."
    );
    assert_eq!(
        eval_text("Höhe # 2", ParamUnit::Number, &sc).unwrap_err().0,
        "Unerwartetes Zeichen „#“ an Stelle 6."
    );
    assert_eq!(
        eval_text("Breit", ParamUnit::Length, &sc).unwrap_err().0,
        "Den Namen „Breit“ gibt es nicht."
    );
    assert_eq!(ev_err("foo(2)"), "Die Funktion „foo“ gibt es nicht.");
    assert_eq!(ev_err("pi(2)"), "Die Funktion „pi“ gibt es nicht.");

    let ok200 = format!("{}1 ", "1+".repeat(99));
    assert_eq!(ok200.chars().count(), 200);
    assert_eq!(ev(&ok200), 100.0);
    let long201 = format!("{}1  ", "1+".repeat(99));
    assert_eq!(
        ev_err(&long201),
        "Die Formel ist zu lang (höchstens 200 Zeichen)."
    );
    // Zeichen, nicht Bytes.
    assert!(parse(&"ä".repeat(200)).is_ok());
    assert_eq!(
        parse(&"ä".repeat(201)).unwrap_err().0,
        "Die Formel ist zu lang (höchstens 200 Zeichen)."
    );

    const DEEP: &str = "Die Formel ist zu tief verschachtelt (höchstens 64 Ebenen).";
    let nest = |n: usize| format!("{}1{}", "(".repeat(n), ")".repeat(n));
    assert_eq!(ev(&nest(64)), 1.0);
    assert_eq!(ev_err(&nest(65)), DEEP);
    assert_eq!(ev(&format!("{}1", "-".repeat(64))), 1.0);
    assert_eq!(ev_err(&format!("{}1", "-".repeat(65))), DEEP);
    let calls = |n: usize| format!("{}1{}", "abs(".repeat(n), ")".repeat(n));
    assert_eq!(ev(&calls(39)), 1.0);
    assert_eq!(ev_err(&format!("{}1", "1^".repeat(65))), DEEP);
    assert_eq!(ev(&format!("{}1", "1^".repeat(64))), 1.0);
    assert_eq!(
        ev_err(&format!("{}1{}", "-(".repeat(33), ")".repeat(33))),
        DEEP
    );
    assert_eq!(ev(&format!("{}1{}", "-(".repeat(32), ")".repeat(32))), 1.0);
}

#[test]
fn names() {
    for good in ["Breite", "_a1", "Höhe", "Maß", "Ärger", "a"] {
        assert_eq!(check_name(good), Ok(()), "{good}");
    }
    let err = |n: &str| check_name(n).unwrap_err().0;
    assert_eq!(err(""), "Der Name darf nicht leer sein.");
    const FORM: &str = "Ein Name beginnt mit einem Buchstaben oder „_“ und enthält nur Buchstaben, Ziffern und „_“.";
    for bad in ["1a", "a-b", "a b", " a", "a.b", "a,b"] {
        assert_eq!(err(bad), FORM, "{bad}");
    }
    assert_eq!(err("sin"), "„sin“ ist schon eine Funktion oder Konstante.");
    assert_eq!(err("PI"), "„PI“ ist schon eine Funktion oder Konstante.");
    assert_eq!(
        err("Sqrt"),
        "„Sqrt“ ist schon eine Funktion oder Konstante."
    );
    for r in ["cos", "tan", "abs", "min", "max", "round", "pi", "sqrt"] {
        assert_eq!(
            err(r),
            format!("„{r}“ ist schon eine Funktion oder Konstante.")
        );
    }
    assert_eq!(
        err(&"a".repeat(65)),
        "Der Name ist zu lang (höchstens 64 Zeichen)."
    );
    assert_eq!(check_name(&"a".repeat(64)), Ok(()));
    assert_eq!(check_name(&"ä".repeat(64)), Ok(()));
    assert!(same_name("Breite", "BREITE"));
    assert!(same_name("Höhe", "HÖHE"));
    assert!(!same_name("Breite", "Breite2"));
    assert_eq!(sc_ok("breite", ParamUnit::Length), 40.0);
    assert_eq!(sc_ok("BREITE", ParamUnit::Length), 40.0);

    let e = parse("Breite + breite*sin(w) - pi + Höhe / BREITE + min(k; Höhe)").unwrap();
    assert_eq!(e.names(), vec!["Breite", "w", "Höhe", "k"]);
}

#[test]
fn rename_tokens() {
    assert_eq!(
        rename_in("Breite/4 + breite*2 - Breite2 + Breitex", "Breite", "B"),
        "B/4 + B*2 - Breite2 + Breitex"
    );
    assert_eq!(rename_in("Breite +* 2", "Breite", "B"), "B +* 2");
    assert_eq!(
        rename_in("min(1,5; Breite)", "Breite", "Weite"),
        "min(1,5; Weite)"
    );
    assert_eq!(rename_in("xBreite_Breite", "Breite", "B"), "xBreite_Breite");
    assert_eq!(rename_in("Höhe*2+höhe", "HÖHE", "Tiefe"), "Tiefe*2+Tiefe");
    assert_eq!(rename_in("2Breite", "Breite", "B"), "2B");
    assert_eq!(rename_in("", "Breite", "B"), "");
    assert!(references("min(Breite, 2)", "breite"));
    assert!(references("Breite # (", "Breite"));
    assert!(!references("Breite2", "Breite"));
    assert!(!references("2*3", "Breite"));
    assert!(!references("_Breite", "Breite"));
}

fn def<'a>(name: &'a str, formula: &'a str, unit: ParamUnit) -> ParamDef<'a> {
    ParamDef {
        name,
        formula,
        unit,
    }
}

#[test]
fn table() {
    use ParamUnit::{Angle, Length, Number};
    assert_eq!(
        evaluate_table(&[def("a", "b+1", Length), def("b", "2", Length)]),
        Ok(vec![3.0, 2.0])
    );
    assert_eq!(evaluate_table(&[]), Ok(vec![]));
    assert_eq!(
        evaluate_table(&[def("a", "b+1", Length), def("b", "a", Length)]),
        Err(TableError {
            index: 0,
            message: "Die Parameter „a“, „b“ hängen im Kreis voneinander ab.".into()
        })
    );
    // Nur die Glieder des Kreises, in der Reihenfolge der Tabelle, nicht die Abhängigen.
    assert_eq!(
        evaluate_table(&[
            def("x", "1", Length),
            def("d", "c+1", Length),
            def("c", "e", Length),
            def("y", "d", Length),
            def("e", "d*2", Length),
        ]),
        Err(TableError {
            index: 1,
            message: "Die Parameter „d“, „c“, „e“ hängen im Kreis voneinander ab.".into()
        })
    );
    assert_eq!(
        evaluate_table(&[def("a", "a+1", Length)]),
        Err(TableError {
            index: 0,
            message: "Der Parameter „a“ hängt von sich selbst ab.".into()
        })
    );
    assert_eq!(
        evaluate_table(&[def("z", "1", Number), def("Abc", "ABC*2", Length)]),
        Err(TableError {
            index: 1,
            message: "Der Parameter „Abc“ hängt von sich selbst ab.".into()
        })
    );
    assert_eq!(
        evaluate_table(&[def("Breite", "1", Length), def("breite", "2", Length)]),
        Err(TableError {
            index: 1,
            message: "Den Namen „breite“ gibt es schon.".into()
        })
    );
    assert_eq!(
        evaluate_table(&[def("a", "c", Length)]),
        Err(TableError {
            index: 0,
            message: "Den Namen „c“ gibt es nicht.".into()
        })
    );
    assert_eq!(
        evaluate_table(&[def("w", "30", Angle), def("l", "w", Length)]),
        Err(TableError {
            index: 1,
            message: "Hier wird eine Länge erwartet, die Formel ergibt einen Winkel.".into()
        })
    );
    assert_eq!(
        evaluate_table(&[def("a", "1", Length), def("sin", "2", Length)]),
        Err(TableError {
            index: 1,
            message: "„sin“ ist schon eine Funktion oder Konstante.".into()
        })
    );
    assert_eq!(
        evaluate_table(&[def("a", "1", Length), def("b", "2+", Length)]),
        Err(TableError {
            index: 1,
            message: "Die Formel endet unerwartet.".into()
        })
    );
    assert_eq!(
        evaluate_table(&[def("n", "0", Number), def("q", "1/n", Number)]),
        Err(TableError {
            index: 1,
            message: "Division durch null.".into()
        })
    );
    // Gemischte Arten: Zahl passt überall, Länge*Zahl bleibt Länge.
    assert_eq!(
        evaluate_table(&[
            def("Wand", "2", Length),
            def("f", "1,5", Number),
            def("Breite", "Wand*f*10", Length),
            def("Winkel", "atan_ersatz", Angle),
            def("atan_ersatz", "45", Angle),
        ]),
        Ok(vec![2.0, 1.5, 30.0, 45.0, 45.0])
    );

    let names: Vec<String> = (0..=500).map(|i| format!("p{i}")).collect();
    let many: Vec<ParamDef<'_>> = names.iter().map(|n| def(n, "1", Length)).collect();
    assert_eq!(many.len(), 501);
    assert_eq!(
        evaluate_table(&many),
        Err(TableError {
            index: 500,
            message: "Höchstens 500 Parameter.".into()
        })
    );
    assert_eq!(evaluate_table(&many[..500]).map(|v| v.len()), Ok(500));

    // Kette p1 = 1, p_i = p_{i-1} + 1 bis p500, in umgekehrter Reihenfolge.
    let chain: Vec<(String, String)> = (1..=500)
        .rev()
        .map(|i| {
            let f = if i == 1 {
                "1".to_string()
            } else {
                format!("p{}+1", i - 1)
            };
            (format!("p{i}"), f)
        })
        .collect();
    let defs: Vec<ParamDef<'_>> = chain.iter().map(|(n, f)| def(n, f, Length)).collect();
    let values = evaluate_table(&defs).unwrap();
    assert_eq!(values[0], 500.0);
    assert_eq!(values[499], 1.0);
    // Ein Kreis über 500 Parameter wird ohne Stapelüberlauf erkannt.
    let mut ring = chain.clone();
    ring[499].1 = "p500+1".into();
    let defs: Vec<ParamDef<'_>> = ring.iter().map(|(n, f)| def(n, f, Length)).collect();
    let e = evaluate_table(&defs).unwrap_err();
    assert_eq!(e.index, 0);
    assert!(e.message.starts_with("Die Parameter „p500“, „p499“, "));
    assert!(e.message.ends_with("„p1“ hängen im Kreis voneinander ab."));
}

#[test]
fn unit_labels_and_error_type() {
    assert_eq!(
        ParamUnit::ALL,
        [ParamUnit::Length, ParamUnit::Angle, ParamUnit::Number]
    );
    let labels: Vec<_> = ParamUnit::ALL.iter().map(|u| u.label()).collect();
    assert_eq!(labels, ["Länge", "Winkel", "Zahl"]);
    let suffixes: Vec<_> = ParamUnit::ALL.iter().map(|u| u.suffix()).collect();
    assert_eq!(suffixes, ["mm", "°", ""]);
    assert_eq!(
        ExprError("Division durch null.".into()).to_string(),
        "Division durch null."
    );
    assert_eq!(planora_feature::MAX_PARAMETERS, 500);
    assert_eq!(planora_feature::MAX_FORMULA_CHARS, 200);
    assert_eq!(planora_feature::MAX_FORMULA_DEPTH, 64);
    assert_eq!(planora_feature::MAX_PARAM_NAME_CHARS, 64);
    assert_eq!(planora_feature::MAX_COMMENT_CHARS, 200);
}
