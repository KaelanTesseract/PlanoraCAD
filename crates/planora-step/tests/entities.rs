//! Eintragsverwaltung: Nummern, Zeilen, Umbruch, Grenze und Abbrechen (Teilprojekt 10, Task 2).

use planora_exchange::{CHECK_EVERY, Progress};
use planora_step::*;

#[test]
fn entities_are_numbered_from_one_and_end_with_a_semicolon() {
    let mut out = Vec::new();
    let p = Progress::new();
    let mut e = Entities::new(&mut out, &p);
    e.line("DATA;").unwrap();
    assert_eq!(e.push("CARTESIAN_POINT('',(0.,0.,0.))").unwrap(), 1);
    assert_eq!(e.push("VERTEX_POINT('',#1)").unwrap(), 2);
    assert_eq!(e.count(), 2);
    e.flush().unwrap();
    drop(e);
    assert_eq!(
        String::from_utf8(out).unwrap(),
        "DATA;\n#1=CARTESIAN_POINT('',(0.,0.,0.));\n#2=VERTEX_POINT('',#1);\n"
    );
    p.set_total(4);
    assert!((p.fraction() - 0.5).abs() < 1e-6, "jeder Eintrag zählt");
}

#[test]
fn long_entities_wrap_after_commas_but_never_inside_strings() {
    let mut out = Vec::new();
    let p = Progress::new();
    let mut e = Entities::new(&mut out, &p);
    let ids: Vec<u64> = (1..=60).collect();
    let name = format!("'{}'", "a,b''c,".repeat(30));
    let text = format!("CLOSED_SHELL({name},{})", refs(&ids));
    e.push(&text).unwrap();
    drop(e);
    let s = String::from_utf8(out).unwrap();
    assert!(s.lines().count() > 2, "{s}");
    for line in s.lines() {
        // Jede Zeile außer der letzten endet mit einem Komma, und die Zeichenkette steht ganz
        // in einer Zeile.
        assert!(line.ends_with(',') || line.ends_with(");"), "{line}");
    }
    assert!(s.lines().next().unwrap().contains(&name));
    assert_eq!(s.replace('\n', ""), format!("#1={text};"));
}

#[test]
fn refs_and_logicals_have_the_step_form() {
    assert_eq!(refs(&[]), "()");
    assert_eq!(refs(&[7]), "(#7)");
    assert_eq!(refs(&[1, 22, 333]), "(#1,#22,#333)");
    assert_eq!(logical(true), ".T.");
    assert_eq!(logical(false), ".F.");
    assert_eq!(WRAP_COLUMN, 72);
}

#[test]
fn the_entity_limit_is_a_named_error() {
    assert_eq!(MAX_ENTITIES, 500_000_000);
    let mut out = Vec::new();
    let p = Progress::new();
    let mut e = Entities::new(&mut out, &p);
    e.set_next(MAX_ENTITIES);
    assert_eq!(e.push("A()").unwrap(), MAX_ENTITIES);
    assert_eq!(e.push("B()"), Err(StepError::TooManyEntities));
    drop(e);
    assert_eq!(
        String::from_utf8(out).unwrap(),
        format!("#{MAX_ENTITIES}=A();\n")
    );
}

#[test]
fn cancelling_stops_at_the_next_check_and_the_probe_does_not_count() {
    let mut sink = std::io::sink();
    let p = Progress::new();
    p.cancel();
    let mut e = Entities::probe(&mut sink, &p);
    for i in 1..CHECK_EVERY {
        assert_eq!(e.push("A()").unwrap(), i);
    }
    assert_eq!(e.push("A()"), Err(StepError::Cancelled));
    let q = Progress::new();
    q.set_total(10);
    let mut e = Entities::probe(&mut sink, &q);
    e.push("A()").unwrap();
    assert_eq!(q.fraction(), 0.0, "der Probelauf zählt nicht");
}

#[test]
fn write_errors_become_io_errors() {
    struct Full;
    impl std::io::Write for Full {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            Err(std::io::Error::other("Datenträger voll"))
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut full = Full;
    let p = Progress::new();
    let mut e = Entities::new(&mut full, &p);
    assert!(matches!(e.push("A()"), Err(StepError::Io { .. })));
}
