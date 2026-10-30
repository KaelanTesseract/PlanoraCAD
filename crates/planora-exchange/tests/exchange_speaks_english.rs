//! Task 12: Die Meldungen des Netz-Austauschs (STL, OBJ, 3MF, glTF) erscheinen unter
//! `with_lang(&english())` auf Englisch; unter Deutsch bleiben sie wie bisher.

use planora_exchange::{ExchangeError, Progress, read_obj, read_stl};
use planora_i18n::{english, german, with_lang};

fn assert_english(what: &str, text: &str) {
    assert!(!text.is_empty(), "{what}: leer");
    assert!(
        !text.chars().any(|c| "äöüßÄÖÜ„“".contains(c)),
        "{what}: {text}"
    );
    let looks_like_key = !text.contains(' ')
        && text.contains('-')
        && text
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
    assert!(!looks_like_key, "{what}: Schlüssel statt Text: {text}");
}

fn all_errors() -> Vec<ExchangeError> {
    vec![
        ExchangeError::Io { msg: "x".into() },
        ExchangeError::FileTooLarge {
            bytes: 9_100_000_000,
        },
        ExchangeError::TooManyTriangles,
        ExchangeError::TooManyVertices,
        ExchangeError::Truncated,
        ExchangeError::CountTooLarge {
            claimed: 5,
            possible: 3,
        },
        ExchangeError::NotFinite { line: Some(4) },
        ExchangeError::NotFinite { line: None },
        ExchangeError::BadNumber { line: 4 },
        ExchangeError::IndexOutOfRange { line: 4 },
        ExchangeError::Syntax {
            line: 4,
            msg: "x".into(),
        },
        ExchangeError::Polygon { line: 4 },
        ExchangeError::Empty,
        ExchangeError::TooLargeForFormat { format: "STL" },
        ExchangeError::Corrupt("x".into()),
        ExchangeError::UnknownFormat("xyz".into()),
        ExchangeError::Cancelled,
    ]
}

fn read_text(kind: &str, text: &str) -> Result<(), ExchangeError> {
    let bytes = text.as_bytes();
    let len = bytes.len() as u64;
    let p = Progress::new();
    match kind {
        "stl" => read_stl(bytes, len, &p).map(|_| ()),
        _ => read_obj(bytes, len, &p).map(|_| ()),
    }
}

#[test]
fn every_exchange_error_speaks_english() {
    with_lang(&english(), || {
        for e in all_errors() {
            assert_english("ExchangeError", &e.to_string());
        }
    });
}

#[test]
fn german_texts_stay_as_they_were() {
    with_lang(&german(), || {
        assert_eq!(
            ExchangeError::FileTooLarge {
                bytes: 9_100_000_000
            }
            .to_string(),
            "Die Datei ist zu groß (9,1 GB); erlaubt sind höchstens 8 GB"
        );
        assert_eq!(
            ExchangeError::Syntax {
                line: 4,
                msg: "x".into()
            }
            .to_string(),
            "Zeile 4: x"
        );
    });
    with_lang(&english(), || {
        assert_eq!(
            ExchangeError::FileTooLarge {
                bytes: 9_100_000_000
            }
            .to_string(),
            "The file is too large (9.1 GB); at most 8 GB are allowed"
        );
    });
}

#[test]
fn broken_files_are_reported_in_english() {
    with_lang(&english(), || {
        let cases = [
            ("stl", "solid x\n facet normal 0 0 0\n  vertex 1 2\n"),
            ("stl", "solid x\n facet normal 0 0 0\n facet normal 0 0 0\n"),
            ("stl", "solid x\n bogus\n"),
            (
                "stl",
                "solid x\n facet normal 0 0 0\n vertex 0 0 0\n endfacet\n",
            ),
            ("stl", "solid x\n facet normal 0 0 0\n vertex 0 0 nan\n"),
            ("stl", "solid x\n facet normal 0 0 0\n vertex 0 0 abc\n"),
            ("obj", "v 0 0 0\nv 1 0 0\nf 1 2\n"),
            ("obj", "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 x\n"),
            ("obj", "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 9\n"),
            ("obj", "v 0 0\n"),
            ("obj", "v 0 0 0\n"),
        ];
        let mut seen = 0;
        for (kind, text) in cases {
            let e = read_text(kind, text).unwrap_err();
            assert_english(&format!("{kind}: {text:?}"), &e.to_string());
            seen += 1;
        }
        assert_eq!(seen, cases.len());
    });
}
