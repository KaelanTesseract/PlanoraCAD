//! Selbsttest des eigenen STEP-Lesers: gültige Dateien, zusammengesetzte Einträge, Maskierung
//! und benannte Fehler (Teilprojekt 10, Task 4).

mod common;

use common::reader::{Entity, Value, parse};

/// Kleine gültige Datei um `data` (Einträge ohne `DATA;`/`ENDSEC;`).
fn file(data: &str) -> String {
    let head = "ISO-10303-21;\nHEADER;\nFILE_DESCRIPTION(('x'),'2;1');\n\
                FILE_NAME('a.step','2026-10-14T00:00:00',(''),(''),'P','P','');\n\
                FILE_SCHEMA(('AUTOMOTIVE_DESIGN { 1 0 10303 214 1 1 1 1 }'));\nENDSEC;\nDATA;\n";
    format!("{head}{data}ENDSEC;\nEND-ISO-10303-21;\n")
}

#[test]
fn a_small_file_with_all_value_kinds_is_read() {
    let text = file(concat!(
        "#1=CARTESIAN_POINT('',(0.,-1.5,2.E-3));\n",
        "#2=DIRECTION('d',\n(0.,0.,1.));\n",
        r"#3=FOO(#1,$,*,.T.,.UNSPECIFIED.,(1,2,3),LENGTH_MEASURE(1.E-6),",
        r"'O''Brien \\ \X2\00FC\X0\ \X4\0001F600\X0\');",
        "\n",
        "#4=(BOUNDED_CURVE()B_SPLINE_CURVE(2,(#1,#1,#1),.UNSPECIFIED.,.F.,.F.)\n",
        "B_SPLINE_CURVE_WITH_KNOTS((3,3),(0.,1.),.UNSPECIFIED.)CURVE()",
        "GEOMETRIC_REPRESENTATION_ITEM()\n",
        "RATIONAL_B_SPLINE_CURVE((1.,0.5,1.))REPRESENTATION_ITEM(''));\n",
    ));
    let f = parse(&text).unwrap();
    assert_eq!(f.header.len(), 3);
    assert_eq!(f.entities.len(), 4);
    let p = f.simple(1, "CARTESIAN_POINT").unwrap();
    assert_eq!(p[1].reals().unwrap(), vec![0.0, -1.5, 2e-3]);
    assert_eq!(f.simple(2, "DIRECTION").unwrap()[0], Value::Str("d".into()));
    let foo = f.simple(3, "FOO").unwrap();
    assert_eq!(foo[0], Value::Ref(1));
    assert_eq!(foo[1], Value::Unset);
    assert_eq!(foo[2], Value::Derived);
    assert!(foo[3].logical().unwrap());
    assert_eq!(foo[4].enumeration().unwrap(), "UNSPECIFIED");
    assert_eq!(foo[5].ints().unwrap(), vec![1, 2, 3]);
    assert_eq!(
        foo[6],
        Value::Typed("LENGTH_MEASURE".into(), vec![Value::Real(1e-6)])
    );
    assert_eq!(foo[7].text().unwrap(), "O'Brien \\ ü \u{1F600}");
    let e = f.get(4).unwrap();
    assert!(matches!(e, Entity::Complex(parts) if parts.len() == 7));
    assert_eq!(
        e.part("RATIONAL_B_SPLINE_CURVE").unwrap()[0]
            .reals()
            .unwrap(),
        vec![1.0, 0.5, 1.0]
    );
    assert_eq!(e.part("B_SPLINE_CURVE").unwrap()[0].int().unwrap(), 2);
    assert_eq!(f.ids_of("DIRECTION"), vec![2]);
    assert_eq!(f.count("CARTESIAN_POINT"), 1);
}

#[test]
fn integers_are_not_reals_and_reals_are_not_integers() {
    let f = parse(&file("#1=A(1,1.,-0.5);\n")).unwrap();
    let a = f.simple(1, "A").unwrap();
    assert!(a[0].real().is_err(), "1 ohne Punkt ist keine REAL-Zahl");
    assert_eq!(a[0].int().unwrap(), 1);
    assert!(a[1].int().is_err());
    assert_eq!(a[2].real().unwrap(), -0.5);
}

#[test]
fn broken_files_get_named_errors() {
    for (data, want) in [
        ("#1=A(1.e5);\n", "kleines e"),
        ("#1=A(1E5);\n", "Exponent ohne Punkt"),
        ("#1=A(.5);\n", "Aufzählung"),
        ("#1=A(#2);\n", "#2"),
        ("#1=A();\n#1=B();\n", "doppelt"),
        ("#1=(B()A());\n", "aufsteigend"),
        ("#1=(A());\n", "weniger als zwei"),
        ("#1=A('offen);\n", "Zeichenkette"),
        ("#1=A('a\nb');\n", "Steuerzeichen"),
        (r"#1=A('\X2\00F\X0\');", "Länge"),
        (r"#1=A('\S\a');", "Maskierung"),
        ("#1=a();\n", "unerwartetes Zeichen"),
        ("#1=A(1,);\n", "Wert"),
        ("#1=A(1)\n", "Semicolon"),
        ("#1=A('ä');\n", "ASCII"),
    ] {
        let err = parse(&file(data)).unwrap_err();
        assert!(err.contains(want), "{data:?}: {err}");
    }
    let good = file("#1=A();\n");
    assert!(
        parse(&format!("{good}#2=A('x"))
            .unwrap_err()
            .contains("nicht geschlossen")
    );
    assert!(parse(&good.replace("ENDSEC;\nEND", "END")).is_err());
    assert!(
        parse(&format!("{good}#2=B();"))
            .unwrap_err()
            .contains("nach dem Dateiende")
    );
    assert!(
        parse(&good.replace("FILE_NAME", "FILE_X"))
            .unwrap_err()
            .contains("FILE_NAME")
    );
}

#[test]
fn simple_checks_the_entity_name() {
    let f = parse(&file("#1=A();\n#2=(B()C());\n")).unwrap();
    assert!(f.simple(1, "B").is_err());
    assert!(f.simple(2, "B").is_err());
    assert!(f.simple(3, "A").unwrap_err().contains("#3 fehlt"));
    assert_eq!(f.name_of(1).unwrap(), Some("A"));
    assert_eq!(f.name_of(2).unwrap(), None);
}
