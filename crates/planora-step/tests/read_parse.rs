//! STEP-Leser, Stufe 1: Lexer, Index und Einträge bei Bedarf (STEP-Import, Aufgabe 1).
//! Unabhängig vom Testleser `tests/common/reader.rs`; `common` liefert nur den Schreiber.

mod common;

use common::{Lcg, export};
use planora_exchange::Progress;
use planora_kernel::{AnalyticKernel, Kernel};
use planora_math::DVec3;
use planora_step::read::value::{Entity, Value};
use planora_step::{
    MAX_STEP_BODIES, MAX_STEP_ENTITIES, MAX_STEP_FACES, MAX_STEP_FILE_BYTES, ReadLimits, StepFile,
    StepReadError, read_step_file,
};

/// Kopf (7 Zeilen) + `DATA;` (Zeile 7) + `data` (ab Zeile 8) + Ende.
fn file(data: &str) -> String {
    let head = "ISO-10303-21;\nHEADER;\nFILE_DESCRIPTION(('x'),'2;1');\n\
                FILE_NAME('a.step','2026-10-14T00:00:00',(''),(''),'P','P','');\n\
                FILE_SCHEMA(('AUTOMOTIVE_DESIGN { 1 0 10303 214 1 1 1 1 }'));\nENDSEC;\nDATA;\n";
    format!("{head}{data}ENDSEC;\nEND-ISO-10303-21;\n")
}

fn parse(text: &str) -> Result<StepFile, StepReadError> {
    StepFile::parse(
        text.as_bytes().to_vec(),
        &ReadLimits::DEFAULT,
        &Progress::new(),
    )
}

fn err(text: &str) -> String {
    match parse(text) {
        Ok(_) => panic!("sollte scheitern: {text:?}"),
        Err(e) => e.to_string(),
    }
}

/// Probe aus (a): Kommentar vor dem Kopf, Eintrag über drei Zeilen, Vorwärtsverweis,
/// zusammengesetzter Eintrag, alle Maskierungen.
fn probe() -> String {
    let head = "/* Kommentar vor dem Kopf */ ISO-10303-21;\nHEADER;\nFILE_DESCRIPTION(('x'),'2;1');\n\
                FILE_NAME('a.step','2026-10-14T00:00:00',(''),(''),'P','P','');\n\
                FILE_SCHEMA(('AUTOMOTIVE_DESIGN { 1 0 10303 214 1 1 1 1 }'));\nENDSEC;\nDATA;\n";
    let data = concat!(
        "#3=FOO(#1,$,*,.T.,\n",
        ".UNSPECIFIED.,(1,2,3),/* mitten */LENGTH_MEASURE(1.E-6),\n",
        r"'O''Brien \\ \X2\00FC\X0\ \X4\0001F600\X0\ \X\E4 \S\d');",
        "\n",
        "#1=CARTESIAN_POINT('',(0.,-1.5,2.E-3));\n",
        "#4=(BOUNDED_CURVE()B_SPLINE_CURVE(2,(#1,#1,#1),.UNSPECIFIED.,.F.,.F.)\n",
        "B_SPLINE_CURVE_WITH_KNOTS((3,3),(0.,1.),.UNSPECIFIED.)CURVE()",
        "GEOMETRIC_REPRESENTATION_ITEM()\n",
        "RATIONAL_B_SPLINE_CURVE((1.,0.5,1.))REPRESENTATION_ITEM(''));\n",
    );
    format!("{head}{data}ENDSEC;\nEND-ISO-10303-21;\n")
}

fn nested(levels: usize) -> String {
    // `A(` zählt als erste Ebene.
    let inner = levels - 1;
    file(&format!(
        "#1=A({}1{});\n",
        "(".repeat(inner),
        ")".repeat(inner)
    ))
}

#[test]
fn all_value_kinds_and_complex_entities() {
    let text = probe();
    let f = parse(&text).unwrap();
    assert_eq!(f.len(), 3);
    assert!(!f.is_empty());
    let e = f.entity(3).unwrap();
    let Entity::Simple(r) = &e else {
        panic!("einfach erwartet: {e:?}")
    };
    assert_eq!(r.name, "FOO");
    assert_eq!(
        r.args,
        vec![
            Value::Ref(1),
            Value::Unset,
            Value::Derived,
            Value::Enum("T".into()),
            Value::Enum("UNSPECIFIED".into()),
            Value::List(vec![Value::Int(1), Value::Int(2), Value::Int(3)]),
            Value::Typed("LENGTH_MEASURE".into(), vec![Value::Real(1e-6)]),
            Value::Str("O'Brien \\ ü \u{1F600} ä ä".into()),
        ]
    );
    assert_eq!(r.args[3].logical(), Ok(Some(true)));
    assert!(r.args[1].is_unset() && r.args[2].is_unset() && !r.args[0].is_unset());
    let p = f.entity(1).unwrap();
    assert_eq!(
        p.part("CARTESIAN_POINT").unwrap()[1].reals(),
        Ok(vec![0.0, -1.5, 2e-3])
    );
    let c = f.entity(4).unwrap();
    assert!(matches!(&c, Entity::Complex(parts) if parts.len() == 7));
    assert!(c.is("B_SPLINE_CURVE") && c.is("CURVE") && !c.is("FOO"));
    assert_eq!(
        c.part("RATIONAL_B_SPLINE_CURVE").unwrap(),
        &[Value::List(vec![
            Value::Real(1.0),
            Value::Real(0.5),
            Value::Real(1.0)
        ])][..]
    );
    assert_eq!(
        c.part("B_SPLINE_CURVE").unwrap()[1].refs(),
        Ok(vec![1, 1, 1])
    );
    assert_eq!(c.names()[0], "BOUNDED_CURVE");
    assert_eq!(c.names().len(), 7);
    assert_eq!(f.ids_of("B_SPLINE_CURVE"), vec![4]);
    assert_eq!(f.ids_of("CARTESIAN_POINT"), vec![1]);
    assert_eq!(f.ids_of("FOO"), vec![3]);
    assert_eq!(
        f.ids_of("LENGTH_MEASURE"),
        Vec::<u64>::new(),
        "getypte Werte zählen nicht"
    );
    assert_eq!(f.line(3), Some(8));
    assert_eq!(f.line(1), Some(11));
    assert_eq!(f.line(4), Some(12));
    assert_eq!(f.line(2), None);
    // Gleiche Zeilen mit Windows-Zeilenenden.
    let crlf = parse(&text.replace('\n', "\r\n")).unwrap();
    assert_eq!(crlf.line(4), Some(12));
    assert_eq!(crlf.entity(3).unwrap(), e);
}

#[test]
fn lenient_numbers_and_keywords() {
    let f = parse(&file(
        "#1=a(1E5, 1.e-3, -0.5, +2, foo_bar(), -.5, 7., 0);\n",
    ))
    .unwrap();
    let e = f.entity(1).unwrap();
    assert!(e.is("A"));
    assert_eq!(
        e.part("A").unwrap(),
        &[
            Value::Real(1e5),
            Value::Real(1e-3),
            Value::Real(-0.5),
            Value::Int(2),
            Value::Typed("FOO_BAR".into(), vec![]),
            Value::Real(-0.5),
            Value::Real(7.0),
            Value::Int(0),
        ][..]
    );
    assert_eq!(Value::Int(2).real(), Ok(2.0));
    assert!(Value::Real(2.0).int().is_err(), "Real ist keine ganze Zahl");
    assert!(Value::Str("x".into()).real().is_err());
    assert_eq!(Value::Enum("F".into()).logical(), Ok(Some(false)));
    assert_eq!(Value::Enum("U".into()).logical(), Ok(None));
    assert!(Value::Enum("X".into()).logical().is_err());
    assert_eq!(
        f.ids_of("a"),
        vec![1],
        "Typnamen ohne Rücksicht auf Groß/klein"
    );
    let f = parse(&file("#9=A();\n#2=B();\n#5=(A()B()A());\n#3=A();\n")).unwrap();
    assert_eq!(
        f.ids_of("A"),
        vec![3, 5, 9],
        "aufsteigend, jede Nummer einmal"
    );
    assert_eq!(f.ids_of("B"), vec![2, 5]);
}

#[test]
fn header_is_read() {
    let two = file("#1=A();\nENDSEC;\nDATA;\n#2=B(#1);\n");
    let f = parse(&two).unwrap();
    let h = f.header();
    assert_eq!(
        h.schemas,
        vec!["AUTOMOTIVE_DESIGN { 1 0 10303 214 1 1 1 1 }"]
    );
    assert_eq!(h.file_name, "a.step");
    assert_eq!(h.system, "P");
    assert_eq!(f.len(), 2, "beide DATA-Abschnitte");
    assert_eq!(f.entity(2).unwrap().part("B").unwrap()[0], Value::Ref(1));
    // Unbekannte Kopfeinträge werden übergangen; DATA mit Parametern (AP242) geht auch.
    let other = file("#1=A();\n")
        .replace(
            "FILE_DESCRIPTION",
            "FILE_POPULATION(('x'),$);\nFILE_DESCRIPTION",
        )
        .replace("DATA;", "DATA('teil',('S'));")
        .replace("'P','P'", "'P','Mein System'");
    let f = parse(&other).unwrap();
    assert_eq!(f.header().system, "Mein System");
    assert_eq!(f.len(), 1);
}

#[test]
fn hostile_files_give_exact_messages() {
    let deep = nested(65);
    let crlf = file("#1=A();\n#1=B();\n").replace('\n', "\r\n");
    let table: Vec<(String, &str)> = vec![
        (
            "hallo".into(),
            "Die Datei ist keine STEP-Datei (am Anfang fehlt „ISO-10303-21;“).",
        ),
        (
            "".into(),
            "Die Datei ist keine STEP-Datei (am Anfang fehlt „ISO-10303-21;“).",
        ),
        (
            "\u{0}\u{1}PK".into(),
            "Die Datei ist keine STEP-Datei (am Anfang fehlt „ISO-10303-21;“).",
        ),
        (
            file("#1=A(1E400);\n"),
            "Zeile 8: Die Zahl „1E400“ ist zu groß oder keine gültige Zahl.",
        ),
        (
            file("#1=A(99999999999999999999);\n"),
            "Zeile 8: Die Zahl „99999999999999999999“ ist zu groß oder keine gültige Zahl.",
        ),
        (
            file(&format!("#1=A({});\n", "9".repeat(40))),
            "Zeile 8: Die Zahl „99999999999999999999999999999999“ ist zu groß oder keine gültige Zahl.",
        ),
        (
            file("#1=A(1.E);\n"),
            "Zeile 8: Die Zahl „1.E“ ist zu groß oder keine gültige Zahl.",
        ),
        (
            file("#1=A();\n#99999999999999999999=A();\n"),
            "Zeile 9: Die Zahl „#99999999999999999999“ ist zu groß oder keine gültige Zahl.",
        ),
        (
            deep.clone(),
            "Zeile 8: Die Klammern sind zu tief verschachtelt (höchstens 64 Ebenen).",
        ),
        (
            file("#1=A();\n#1=B();\n"),
            "Zeile 9: Die Nummer #1 kommt doppelt vor.",
        ),
        (crlf, "Zeile 9: Die Nummer #1 kommt doppelt vor."),
        (
            file("#5=A();\n#1=A();\n#2=A();\n#1=A();\n#5=B();\n"),
            "Zeile 11: Die Nummer #1 kommt doppelt vor.",
        ),
        (
            file("#1=A('offen);\n"),
            "Zeile 8: Eine Zeichenkette ist nicht geschlossen.",
        ),
        (
            file("#1=A();\n/* offen"),
            "Zeile 9: Ein Kommentar ist nicht geschlossen.",
        ),
        (file("#1=A(ä);\n"), "Zeile 8: Unerwartetes Zeichen „ä“."),
        (file("#1=A(1)\n"), "Zeile 8: Es fehlt ein „;“."),
        (
            file("#1=A(1;\n"),
            "Zeile 8: Es fehlt eine schließende Klammer.",
        ),
        (file("#1 A(1);\n"), "Zeile 8: Nach „#1“ fehlt „=“."),
        (
            file(r"#1=A('\X2\00F\X0\');"),
            "Zeile 8: Eine Maskierung in einer Zeichenkette ist ungültig.",
        ),
        (
            "ISO-10303-21;\nHEADER;\nENDSEC;\nDATA;\n#1=A(".into(),
            "Die Datei ist unvollständig: Sie endet in Zeile 5, bevor alles gelesen ist.",
        ),
        (
            "ISO-10303-21;\nHEADER;\nENDSEC;\nDATA;\n#1=A();\n".into(),
            "Die Datei ist unvollständig: Sie endet in Zeile 5, bevor alles gelesen ist.",
        ),
    ];
    for (text, want) in &table {
        assert_eq!(&err(text), want, "{text:?}");
    }
    assert!(matches!(
        parse(&deep),
        Err(StepReadError::TooDeep { line: 8 })
    ));
    assert!(parse(&nested(64)).is_ok(), "64 Ebenen sind erlaubt");
    assert!(matches!(
        parse(&file("#1=A(1E400);\n")),
        Err(StepReadError::BadNumber { line: 8, .. })
    ));
    // Sehr tiefe Verschachtelung ohne Stapelüberlauf.
    let very = file(&format!("#1=A({});\n", "(".repeat(1_000_000)));
    assert!(matches!(
        parse(&very),
        Err(StepReadError::TooDeep { line: 8 })
    ));
    // Abgeschnitten bei jedem Byte, das kein Leerraum ist: nie Panik, immer eine Meldung.
    let text = probe();
    let bytes = text.as_bytes();
    for i in 1..bytes.len() {
        if bytes[i].is_ascii_whitespace() {
            continue;
        }
        let r = StepFile::parse(bytes[..i].to_vec(), &ReadLimits::DEFAULT, &Progress::new());
        let msg = match r {
            Ok(_) => panic!("abgeschnitten bei {i} geladen"),
            Err(e) => e.to_string(),
        };
        assert!(
            msg.starts_with("Die Datei ist unvollständig") || msg.starts_with("Zeile"),
            "bei {i}: {msg}"
        );
    }
}

#[test]
fn missing_refs_hit_only_the_entity() {
    let f = parse(&file(
        "#1=A(#2);\n#3=B(1);\n#4=C((1,(#3,#9)));\n#5=D(#5);\n",
    ))
    .unwrap();
    let e = f.entity(1).unwrap_err();
    assert_eq!(
        e,
        StepReadError::MissingRef {
            line: 8,
            id: 1,
            target: 2
        }
    );
    assert_eq!(
        e.to_string(),
        "Zeile 8: #1 verweist auf #2, den es nicht gibt."
    );
    assert!(f.entity(3).is_ok());
    assert_eq!(
        f.entity(4),
        Err(StepReadError::MissingRef {
            line: 10,
            id: 4,
            target: 9
        }),
        "auch tief in Listen"
    );
    assert_eq!(f.entity(5), Err(StepReadError::Cycle { line: 11, id: 5 }));
    assert_eq!(
        f.entity(5).unwrap_err().to_string(),
        "Zeile 11: Die Einträge verweisen im Kreis (#5)."
    );
    assert!(f.entity(7).is_err(), "unbekannte Nummer");
}

#[test]
fn limits_are_named_errors() {
    assert_eq!(MAX_STEP_FILE_BYTES, 200 * 1024 * 1024);
    assert_eq!(MAX_STEP_ENTITIES, 5_000_000);
    assert_eq!(MAX_STEP_BODIES, 50);
    assert_eq!(MAX_STEP_FACES, 20_000);
    assert_eq!(
        ReadLimits::DEFAULT,
        ReadLimits {
            max_bytes: MAX_STEP_FILE_BYTES,
            max_entities: MAX_STEP_ENTITIES,
            max_bodies: MAX_STEP_BODIES,
            max_faces: MAX_STEP_FACES,
        }
    );
    let four = file("#1=A();\n#2=A();\n#3=A();\n#4=A();\n");
    let small = ReadLimits {
        max_entities: 3,
        ..ReadLimits::DEFAULT
    };
    let r = StepFile::parse(four.clone().into_bytes(), &small, &Progress::new());
    assert!(matches!(r, Err(StepReadError::TooManyEntities)));
    assert_eq!(
        StepReadError::TooManyEntities.to_string(),
        "Die Datei hat mehr als 5 Millionen Einträge."
    );
    let exact = ReadLimits {
        max_entities: 4,
        ..ReadLimits::DEFAULT
    };
    assert!(StepFile::parse(four.clone().into_bytes(), &exact, &Progress::new()).is_ok());
    // Größe im Speicher.
    let tiny = ReadLimits {
        max_bytes: 100,
        ..ReadLimits::DEFAULT
    };
    let n = four.len() as u64;
    assert_eq!(
        StepFile::parse(four.into_bytes(), &tiny, &Progress::new()).err(),
        Some(StepReadError::TooLarge { bytes: n })
    );
    // Größe aus den Metadaten, bevor gelesen wird.
    let dir = std::env::temp_dir().join(format!("planora-step-read-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let limit = ReadLimits {
        max_bytes: 1000,
        ..ReadLimits::DEFAULT
    };
    let path = dir.join("gross.step");
    std::fs::write(&path, vec![0u8; 1001]).unwrap();
    assert_eq!(
        read_step_file(&path, &limit, &Progress::new()).err(),
        Some(StepReadError::TooLarge { bytes: 1001 })
    );
    let big = dir.join("riesig.step");
    std::fs::write(&big, vec![0u8; 1_000_000]).unwrap();
    assert_eq!(
        read_step_file(&big, &limit, &Progress::new()).err(),
        Some(StepReadError::TooLarge { bytes: 1_000_000 }),
        "Größe aus den Metadaten, nicht aus dem Gelesenen"
    );
    #[cfg(windows)]
    {
        // Eine Datei, die gerade niemand sonst öffnen darf: die Größe kommt trotzdem an.
        use std::os::windows::fs::OpenOptionsExt;
        let _lock = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(&path)
            .unwrap();
        assert_eq!(
            read_step_file(&path, &limit, &Progress::new()).err(),
            Some(StepReadError::TooLarge { bytes: 1001 }),
            "Fehler kommt vor dem Öffnen"
        );
    }
    let ok = dir.join("gut.step");
    std::fs::write(&ok, file("#1=A();\n")).unwrap();
    assert_eq!(
        read_step_file(&ok, &limit, &Progress::new()).unwrap().len(),
        1
    );
    let missing = read_step_file(&dir.join("fehlt.step"), &limit, &Progress::new());
    assert!(
        matches!(&missing, Err(StepReadError::Io { .. })),
        "{missing:?}"
    );
    assert!(
        missing
            .unwrap_err()
            .to_string()
            .starts_with("Die Datei lässt sich nicht lesen: ")
    );
    std::fs::remove_dir_all(&dir).unwrap();
    assert_eq!(
        StepReadError::TooLarge { bytes: 210_000_000 }.to_string(),
        "Die Datei ist zu groß (200,3 MB); erlaubt sind höchstens 200 MB."
    );
    let f = parse(&file("#1=A();\n")).unwrap();
    assert_eq!(f.kib(), 1);
    assert_eq!(f.limits(), &ReadLimits::DEFAULT);
}

#[test]
fn messages_of_the_other_cases() {
    for (e, want) in [
        (
            StepReadError::NoBodies,
            "Die Datei enthält keine Volumenkörper.",
        ),
        (
            StepReadError::TooManyBodies,
            "Die Datei enthält mehr als 50 Körper.",
        ),
        (StepReadError::Cancelled, "Abgebrochen"),
        (
            StepReadError::Io {
                msg: "kaputt".into(),
            },
            "Die Datei lässt sich nicht lesen: kaputt",
        ),
        (
            StepReadError::Syntax {
                line: 3,
                msg: "Es fehlt ein „;“.".into(),
            },
            "Zeile 3: Es fehlt ein „;“.",
        ),
        (
            StepReadError::Truncated { line: 4 },
            "Die Datei ist unvollständig: Sie endet in Zeile 4, bevor alles gelesen ist.",
        ),
        (
            StepReadError::BadNumber {
                line: 2,
                text: "1".repeat(40),
            },
            "Zeile 2: Die Zahl „11111111111111111111111111111111“ ist zu groß oder keine gültige Zahl.",
        ),
    ] {
        assert_eq!(e.to_string(), want);
    }
}

#[test]
fn cancel_and_progress() {
    let data: String = (1..=10_000).map(|i| format!("#{i}=A({i});\n")).collect();
    let text = file(&data);
    let cancelled = Progress::new();
    cancelled.cancel();
    assert!(matches!(
        StepFile::parse(text.clone().into_bytes(), &ReadLimits::DEFAULT, &cancelled),
        Err(StepReadError::Cancelled)
    ));
    let progress = Progress::new();
    let f = StepFile::parse(text.into_bytes(), &ReadLimits::DEFAULT, &progress).unwrap();
    assert_eq!(progress.fraction(), 1.0);
    assert_eq!(f.len(), 10_000);
    assert!(f.kib() > 100);
    assert_eq!(f.ids_of("A").len(), 10_000);
    assert_eq!(
        f.entity(9_999).unwrap().part("A").unwrap()[0],
        Value::Int(9_999)
    );
}

/// Die zerlegte Datei wandert später als `Arc<StepFile>` in Hintergrundaufträge.
const _: () = {
    const fn send_sync<T: Send + Sync>() {}
    send_sync::<StepFile>();
};

fn quader() -> planora_kernel::Solid {
    AnalyticKernel::default()
        .make_box(DVec3::new(10.0, 20.0, 30.0))
        .unwrap()
}

#[test]
fn our_writer_output_is_read() {
    let b = quader();
    let (text, _) = export(&[("Quader", &b)]);
    let f = parse(&text).unwrap();
    assert_eq!(f.ids_of("ADVANCED_FACE").len(), 6);
    assert_eq!(f.header().file_name, "probe.step");
    for id in 1..=f.len() as u64 * 2 {
        if f.line(id).is_some() {
            f.entity(id).unwrap_or_else(|e| panic!("#{id}: {e}"));
        }
    }
    let ids: usize = (1..=f.len() as u64 * 2)
        .filter(|&id| f.line(id).is_some())
        .count();
    assert_eq!(ids, f.len());
}

#[test]
fn random_mutations_never_panic() {
    let b = quader();
    let (writer, _) = export(&[("Quader", &b)]);
    let samples = [probe(), writer];
    let pieces: &[&[u8]] = &[
        b"(",
        b")",
        b",",
        b";",
        b"'",
        b"#",
        b"#1",
        b"$",
        b"*",
        b".",
        b".T.",
        b"/*",
        b"*/",
        b"\\",
        b"\\X2\\",
        b"\\X0\\",
        b"\\S\\",
        b"=",
        b"1E999",
        b"-",
        b"\r",
        b"\n",
        b"\xC3\xA4",
        b"\xFF",
        b"ENDSEC;",
        b"DATA;",
        b"A(",
        b"9999999999999999999999",
    ];
    let mut rng = Lcg(7);
    let mut loaded = 0;
    for round in 0..3000 {
        let mut bytes = samples[round % 2].as_bytes().to_vec();
        for _ in 0..1 + (rng.uniform() * 4.0) as usize {
            let at = (rng.uniform() * bytes.len() as f64) as usize;
            match (rng.uniform() * 4.0) as usize {
                0 => {
                    bytes.remove(at.min(bytes.len() - 1));
                }
                1 => {
                    let p = pieces[(rng.uniform() * pieces.len() as f64) as usize];
                    bytes.splice(at..at, p.iter().copied());
                }
                2 => {
                    let i = at.min(bytes.len() - 1);
                    bytes[i] = (rng.uniform() * 256.0) as u8;
                }
                _ => {
                    let end = (at + 1 + (rng.uniform() * 40.0) as usize).min(bytes.len());
                    let chunk = bytes[at..end].to_vec();
                    bytes.splice(at..at, chunk);
                }
            }
        }
        if let Ok(f) = StepFile::parse(bytes, &ReadLimits::DEFAULT, &Progress::new()) {
            loaded += 1;
            for id in 0..200 {
                let _ = f.entity(id).map(|e| e.names().len());
                let _ = f.line(id);
            }
            let _ = f.ids_of("ADVANCED_FACE");
        }
    }
    assert!(loaded > 100, "nur {loaded} Proben geladen");
}
