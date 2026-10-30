//! Formelbaustein für Parameter: Rangfolge, Funktionen, Komma, Einheiten, Fehlermeldungen,
//! Grenzen, Namensregeln, Umbenennen und Auswertung der Tabelle.

use planora_feature::expr::{
    ExprError, ParamDef, ParamUnit, TableError, TableErrorKind, check_name, eval_text,
    evaluate_table, number_text, parse, plain_number, references, rename_in, same_name,
    short_number, value_text,
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
    assert_eq!(ev("cos(30)"), 3f64.sqrt() / 2.0);
    assert_eq!(ev("sin(60)"), 3f64.sqrt() / 2.0);
    assert_eq!(ev("cos(45)"), std::f64::consts::FRAC_1_SQRT_2);
    // Kosinus in allen Vierteln, auch für negative Winkel (Vorzeichen!).
    assert_eq!(ev("cos(-30)"), 3f64.sqrt() / 2.0);
    assert!((ev("cos(200)") - 200f64.to_radians().cos()).abs() < 1e-15);
    assert!(ev("cos(200)") < -0.93);
    assert!((ev("cos(250)") - 250f64.to_radians().cos()).abs() < 1e-15);
    assert_eq!(ev("cos(300)"), 0.5);
    assert!((ev("cos(-80)") - 80f64.to_radians().cos()).abs() < 1e-15);
    assert!((ev("cos(340)") - 340f64.to_radians().cos()).abs() < 1e-15);
    assert_eq!(ev("cos(240)"), -0.5);
    assert_eq!(ev("tan(-45)"), -1.0);
    assert!((ev("tan(300)") + 3f64.sqrt()).abs() < 1e-15);
    assert!((ev("tan(200)") - 200f64.to_radians().tan()).abs() < 1e-15);
    // Winzige negative Winkel landen nicht im Viertel „360°“.
    assert_eq!(ev("sin(-0,00000000000000000001)"), 0.0);
    assert_eq!(ev("cos(-0,00000000000000000001)"), 1.0);
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
    // Ein Komma zwischen Ziffern, das zu keiner Zahl mehr gehört, trennt nicht still.
    assert_eq!(
        ev_err("min(1,5,3)"),
        "Unerwartetes Zeichen „,“ an Stelle 8."
    );
    assert_eq!(
        eval_text("min(k1,5)", ParamUnit::Number, &|_| Some((
            1.0,
            ParamUnit::Number
        )))
        .unwrap_err()
        .0,
        "Unerwartetes Zeichen „,“ an Stelle 7."
    );
    // Tabulator und geschütztes Leerzeichen sind Leerraum.
    assert_eq!(ev("2\t*\u{a0}3"), 6.0);
    assert_eq!(ev("min(1,\u{a0}5)"), 1.0);
    assert_eq!(ev("\t\n 4 \u{a0}"), 4.0);
    assert_eq!(ev_err("\t\u{a0}"), "Die Formel ist leer.");
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
    // Nie „-0“: weder als Text noch als Ergebnis (bitgenauer Vergleich mit 0).
    assert_eq!(number_text(-0.0), "0");
    for t in [
        "-0",
        "round(-0,4)",
        "sqrt(-0)",
        "0*-1",
        "-(1-1)",
        "abs(0)*-2",
    ] {
        assert_eq!(ev(t).to_bits(), 0f64.to_bits(), "{t}");
    }
    assert_eq!(
        eval_text("-0", ParamUnit::Length, &|_| None)
            .unwrap()
            .to_bits(),
        0f64.to_bits()
    );
    assert_eq!(
        planora_feature::expr::fit_unit(-0.0, planora_feature::expr::Dim::NUMBER, ParamUnit::Angle)
            .unwrap()
            .to_bits(),
        0f64.to_bits()
    );
    let raw = planora_feature::expr::evaluate(&parse("-0*Breite").unwrap(), &sc).unwrap();
    assert_eq!(raw.0.to_bits(), 0f64.to_bits());
    // value_text: höchstens 200 Zeichen, als Formel bitgenau derselbe Wert.
    for v in [
        0.0,
        -0.0,
        2.5,
        -12.5,
        1.0 / 3.0,
        1e-7,
        1e199,
        -1e198,
        1e-180,
        -2.2250738585072014e-180,
        123456.789,
    ] {
        let t = value_text(v).unwrap();
        assert!(t.chars().count() <= 200, "{v}");
        assert_eq!(ev(&t).to_bits(), (v + 0.0).to_bits(), "{v}");
        assert_eq!(
            plain_number(&t).unwrap().to_bits(),
            (v + 0.0).to_bits(),
            "{v}"
        );
    }
    assert_eq!(value_text(1e199).unwrap().chars().count(), 200);
    const VALUE_LONG: &str =
        "Der Wert lässt sich nicht als Zahl mit höchstens 200 Zeichen schreiben.";
    for v in [
        1e200,
        -1e199,
        1e300,
        f64::MAX,
        5e-324,
        -1e-250,
        f64::INFINITY,
        f64::NAN,
    ] {
        assert_eq!(value_text(v).unwrap_err().0, VALUE_LONG, "{v}");
    }
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
    assert_eq!(sc_err("w-Breite", Length), MIX);
    assert_eq!(
        sc_err("Breite + Breite^2", Length),
        "Die Einheiten passen nicht zusammen: Länge und Fläche lassen sich nicht zusammenrechnen."
    );
    assert_eq!(
        sc_err("Breite^3 - Breite^2", Length),
        "Die Einheiten passen nicht zusammen: Rauminhalt und Fläche lassen sich nicht zusammenrechnen."
    );
    assert_eq!(
        sc_err("min(w; w^2)", Angle),
        "Die Einheiten passen nicht zusammen: Winkel und eine andere Einheit lassen sich nicht zusammenrechnen."
    );
    assert_eq!(
        sc_err("Breite^4 + Breite*w", Length),
        "Die Einheiten passen nicht zusammen."
    );
    assert_eq!(sc_ok("sqrt(Breite^2*Breite^2)/Breite", Length), 40.0);
    assert_eq!(
        sc_err("sqrt(w)", Number),
        "„sqrt“ passt hier nicht zur Einheit."
    );
    assert_eq!(
        sc_err("sqrt(w*Breite^2)", Number),
        "„sqrt“ passt hier nicht zur Einheit."
    );
    assert_eq!(sc_ok("sqrt(w^2)", Angle), 30.0);
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
        "Der Exponent darf keine Einheit haben."
    );
    assert_eq!(
        sc_err("Breite^w", Length),
        "Der Exponent darf keine Einheit haben."
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
    // Funktionsaufrufe zählen als Ebene.
    let neg_abs = |n: usize| format!("{}1{}", "-abs(".repeat(n), ")".repeat(n));
    assert_eq!(neg_abs(33).chars().count(), 199);
    assert_eq!(ev_err(&neg_abs(33)), DEEP);
    assert_eq!(ev(&neg_abs(32)), -1.0);
    // Die 65. Ebene ist selbst ein Funktionsaufruf.
    let call_last = format!("{}abs(1){}", "-abs(".repeat(32), ")".repeat(32));
    assert_eq!(call_last.chars().count(), 198);
    assert_eq!(ev_err(&call_last), DEEP);

    // Jedes Zwischenergebnis muss endlich sein, auch wenn das Endergebnis es wäre.
    const BIG: &str = "Das Ergebnis ist zu groß oder keine Zahl.";
    assert_eq!(ev_err("min(10^400; 1)"), BIG);
    assert_eq!(ev_err("1/10^400"), BIG);
    assert_eq!(ev_err("max(-(10^400); 1)"), BIG);
    assert_eq!(ev_err("10^308*10 - 10^308*10"), BIG);
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
            kind: TableErrorKind::Cycle,
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
            kind: TableErrorKind::Cycle,
            message: "Die Parameter „d“, „c“, „e“ hängen im Kreis voneinander ab.".into()
        })
    );
    assert_eq!(
        evaluate_table(&[def("a", "a+1", Length)]),
        Err(TableError {
            index: 0,
            kind: TableErrorKind::SelfRef,
            message: "Der Parameter „a“ hängt von sich selbst ab.".into()
        })
    );
    assert_eq!(
        evaluate_table(&[def("z", "1", Number), def("Abc", "ABC*2", Length)]),
        Err(TableError {
            index: 1,
            kind: TableErrorKind::SelfRef,
            message: "Der Parameter „Abc“ hängt von sich selbst ab.".into()
        })
    );
    assert_eq!(
        evaluate_table(&[def("Breite", "1", Length), def("breite", "2", Length)]),
        Err(TableError {
            index: 1,
            kind: TableErrorKind::Name,
            message: "Den Namen „breite“ gibt es schon.".into()
        })
    );
    assert_eq!(
        evaluate_table(&[def("a", "c", Length)]),
        Err(TableError {
            index: 0,
            kind: TableErrorKind::Other,
            message: "Den Namen „c“ gibt es nicht.".into()
        })
    );
    assert_eq!(
        evaluate_table(&[def("w", "30", Angle), def("l", "w", Length)]),
        Err(TableError {
            index: 1,
            kind: TableErrorKind::Other,
            message: "Hier wird eine Länge erwartet, die Formel ergibt einen Winkel.".into()
        })
    );
    assert_eq!(
        evaluate_table(&[def("a", "1", Length), def("sin", "2", Length)]),
        Err(TableError {
            index: 1,
            kind: TableErrorKind::Name,
            message: "„sin“ ist schon eine Funktion oder Konstante.".into()
        })
    );
    assert_eq!(
        evaluate_table(&[def("a", "1", Length), def("b", "2+", Length)]),
        Err(TableError {
            index: 1,
            kind: TableErrorKind::Other,
            message: "Die Formel endet unerwartet.".into()
        })
    );
    assert_eq!(
        evaluate_table(&[def("n", "0", Number), def("q", "1/n", Number)]),
        Err(TableError {
            index: 1,
            kind: TableErrorKind::Other,
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
            kind: TableErrorKind::TooMany,
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
    // Kreis, der nicht in Tabellenreihenfolge gefunden wird: Glieder nach Tabelle sortiert,
    // Zeile ist das erste Kreisglied, nicht der Abhängige „y“.
    assert_eq!(
        evaluate_table(&[
            def("y", "d", Length),
            def("e", "d", Length),
            def("d", "c", Length),
            def("c", "e", Length),
        ]),
        Err(TableError {
            index: 1,
            kind: TableErrorKind::Cycle,
            message: "Die Parameter „e“, „d“, „c“ hängen im Kreis voneinander ab.".into()
        })
    );
    // Ein Kreis über 500 Parameter wird ohne Stapelüberlauf erkannt.
    let mut ring = chain.clone();
    ring[499].1 = "p500+1".into();
    let defs: Vec<ParamDef<'_>> = ring.iter().map(|(n, f)| def(n, f, Length)).collect();
    let e = evaluate_table(&defs).unwrap_err();
    assert_eq!((e.index, e.kind), (0, TableErrorKind::Cycle));
    assert!(e.message.starts_with("Die Parameter „p500“, „p499“, "));
    assert!(e.message.ends_with("„p1“ hängen im Kreis voneinander ab."));
}

/// Die ungünstigsten 200-Zeichen-Formeln lassen sich auf einem kleinen Stapel (512 KiB,
/// Debug-Build) zerlegen und auswerten, wie später tief in der Oberfläche („= Wert“).
#[test]
fn worst_formulas_on_small_stack() {
    let run = || {
        let chain = format!("{}1", "1-".repeat(99));
        let parens = format!("{}1{}", "(".repeat(64), ")".repeat(64));
        let parens_chain = format!("{}{}1{}", "(".repeat(64), "1-".repeat(35), ")".repeat(64));
        let minus = format!("{}1", "-".repeat(64));
        let pow = format!("{}1", "1^".repeat(64));
        let calls = format!("{}1{}", "-abs(".repeat(32), ")".repeat(32));
        let mins = format!("{}1{}", "min(1;".repeat(28), ")".repeat(28));
        let mut out = Vec::new();
        for f in [chain, parens, parens_chain, minus, pow, calls, mins] {
            assert!(f.chars().count() <= 200, "{f}");
            out.push(eval_text(&f, ParamUnit::Number, &|_| None).unwrap());
        }
        out
    };
    let values = std::thread::Builder::new()
        .stack_size(512 * 1024)
        .spawn(run)
        .unwrap()
        .join()
        .unwrap();
    assert_eq!(values, vec![-98.0, 1.0, -34.0, 1.0, 1.0, -1.0, 1.0]);
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
