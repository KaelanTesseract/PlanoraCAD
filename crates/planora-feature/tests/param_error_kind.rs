//! Zuordnung und Vorsilbe von Tabellenfehlern hängen an der Art des Fehlers, nie am Wortlaut der
//! Meldung: Eine Übersetzerdatei mit ganz anderen Texten (Kreis-Meldung ohne `$list`, eine
//! Formelmeldung, die wie die Kreis-Meldung beginnt) ändert nichts daran. Ruft nie `set_global`.

use planora_feature::expr::{ParamDef, TableErrorKind, evaluate_table};
use planora_feature::{Document, FeatureError, ParamEdit, ParamUnit};
use planora_i18n::{find, load_dir, with_lang};

/// Testsprache „xq“ aus einem eigenen Temp-Ordner.
fn test_language() -> planora_i18n::Lang {
    let dir =
        std::env::temp_dir().join(format!("planora-feature-param-kind-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("xq.ftl"),
        "language-name = Testsprache\n\
         feature-param-cycle = Problem\n\
         feature-param-self = Problem\n\
         feature-expr-no-name = Problem: { $name } is unknown\n\
         feature-param-prefix = [{ $name }] { $detail }\n",
    )
    .unwrap();
    let problems = load_dir(&dir);
    let _ = std::fs::remove_dir_all(&dir);
    // Die fehlenden Platzhalter werden gemeldet, die Datei wird trotzdem geladen.
    assert!(problems.iter().all(|p| !p.skipped), "{problems:?}");
    assert!(
        problems
            .iter()
            .any(|p| p.message.contains("\"feature-param-cycle\"")),
        "{problems:?}"
    );
    find("xq").expect("Testsprache geladen")
}

fn edit(name: &str, formula: &str) -> ParamEdit {
    ParamEdit {
        name: name.into(),
        formula: formula.into(),
        unit: ParamUnit::Length,
        comment: String::new(),
    }
}

fn formula_error(r: Result<impl Sized, FeatureError>) -> String {
    match r {
        Err(FeatureError::Formula(m)) => m,
        Err(e) => panic!("anderer Fehler: {e:?}"),
        Ok(_) => panic!("kein Fehler"),
    }
}

#[test]
fn table_errors_do_not_depend_on_the_wording() {
    let lang = test_language();
    with_lang(&lang, || {
        let def = |name: &'static str, formula: &'static str| ParamDef {
            name,
            formula,
            unit: ParamUnit::Length,
        };
        let e = evaluate_table(&[def("a", "b"), def("b", "a")]).unwrap_err();
        assert_eq!(
            (e.index, e.kind, e.message.as_str()),
            (0, TableErrorKind::Cycle, "Problem")
        );
        let e = evaluate_table(&[def("a", "a")]).unwrap_err();
        assert_eq!(e.kind, TableErrorKind::SelfRef);
        let e = evaluate_table(&[def("a", "zzz")]).unwrap_err();
        assert_eq!(e.kind, TableErrorKind::Other);
        assert!(!e.stands_alone());

        let mut d = Document::new();
        let a = d.add_parameter("a", "1", ParamUnit::Length, "").unwrap();
        let b = d.add_parameter("b", "a", ParamUnit::Length, "").unwrap();
        // Kreis und Selbstbezug: ohne Vorsilbe, obwohl die Meldung den Namen nicht nennt.
        assert_eq!(formula_error(d.set_parameter(a, edit("a", "b"))), "Problem");
        assert_eq!(formula_error(d.set_parameter(b, edit("b", "b"))), "Problem");
        // Unbekannter Name: mit Vorsilbe, obwohl die Meldung wie die Kreis-Meldung beginnt.
        assert_eq!(
            formula_error(d.set_parameter(b, edit("b", "zzz"))),
            "[b] Problem: zzz is unknown"
        );
    });
}
