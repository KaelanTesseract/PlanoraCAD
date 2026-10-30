//! Task 12: Die Meldungen des STEP-Schreibers und -Lesers erscheinen unter
//! `with_lang(&english())` auf Englisch; unter Deutsch bleiben sie wie bisher.

use planora_exchange::Progress;
use planora_i18n::{english, german, with_lang};
use planora_step::read::assembly::{guessed_note, placement_note};
use planora_step::read::lex::too_many_close;
use planora_step::read::weld::{coarse_warning, unknown_accuracy_warning};
use planora_step::{
    ImportOptions, LengthUnit, ReadLimits, StepError, StepFile, StepReadError, import_bodies,
};
use std::path::PathBuf;

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

fn write_errors() -> Vec<StepError> {
    vec![
        StepError::Io { msg: "x".into() },
        StepError::NotFinite,
        StepError::InvalidBody {
            name: "Body".into(),
            reason: "x".into(),
        },
        StepError::NothingToExport,
        StepError::TooManyEntities,
        StepError::Cancelled,
    ]
}

fn read_errors() -> Vec<StepReadError> {
    vec![
        StepReadError::NotStep,
        StepReadError::Truncated { line: 12_345 },
        StepReadError::TooLarge {
            bytes: 300 * 1024 * 1024,
        },
        StepReadError::TooManyEntities,
        StepReadError::Duplicate {
            line: 3,
            id: 15,
            first: 2,
        },
        StepReadError::MissingRef {
            line: 3,
            id: 15,
            target: 16,
        },
        StepReadError::Cycle { line: 3, id: 15 },
        StepReadError::bad_number(4, "1E99999"),
        StepReadError::TooDeep { line: 5 },
        StepReadError::UnexpectedChar {
            line: 6,
            c: '\u{1}',
        },
        StepReadError::OpenString { line: 7 },
        StepReadError::OpenComment { line: 8 },
        StepReadError::syntax(9, "x"),
        StepReadError::Io { msg: "x".into() },
        StepReadError::NoBodies,
        StepReadError::TooManyBodies,
        StepReadError::Cancelled,
        StepReadError::UnclearUnits,
        StepReadError::NoEntity { id: 99 },
    ]
}

#[test]
fn every_step_error_speaks_english() {
    with_lang(&english(), || {
        for e in write_errors() {
            assert_english("StepError", &e.to_string());
        }
        for e in read_errors() {
            assert_english("StepReadError", &e.to_string());
        }
        for text in [
            placement_note("Part", "mirroring"),
            guessed_note("Part"),
            unknown_accuracy_warning(0.001),
            coarse_warning(0.5),
            too_many_close(),
        ] {
            assert_english("Meldung", &text);
        }
        for unit in [
            LengthUnit::Millimeter,
            LengthUnit::Zentimeter,
            LengthUnit::Meter,
            LengthUnit::Zoll,
            LengthUnit::Other { mm: 3.5 },
        ] {
            assert_english("Einheit", &unit.label());
        }
    });
}

#[test]
fn german_texts_stay_as_they_were() {
    with_lang(&german(), || {
        assert_eq!(
            StepReadError::Truncated { line: 7 }.to_string(),
            "Die Datei ist unvollständig: Sie endet in Zeile 7, bevor alles gelesen ist."
        );
        assert_eq!(
            StepReadError::TooLarge {
                bytes: 300 * 1024 * 1024
            }
            .to_string(),
            "Die Datei ist zu groß (300,0 MB); erlaubt sind höchstens 200 MB."
        );
        assert_eq!(
            StepError::InvalidBody {
                name: "A".into(),
                reason: "b".into()
            }
            .to_string(),
            "„A“ wird nicht ausgegeben: b"
        );
        assert_eq!(LengthUnit::Zoll.label(), "Zoll");
    });
    with_lang(&english(), || {
        assert_eq!(
            StepReadError::TooLarge {
                bytes: 300 * 1024 * 1024
            }
            .to_string(),
            "The file is too large (300.0 MB); at most 200 MB are allowed."
        );
        assert_eq!(
            StepReadError::Truncated { line: 12_345 }.to_string(),
            "The file is incomplete: it ends in line 12345 before everything is read."
        );
    });
}

/// Alle Testdateien, abgeschnitten und leicht verändert: Jede Meldung, die dabei entsteht
/// (Fehler der Datei, Meldungen zur Datei, Warnungen und Gründe der Körper), ist englisch.
#[test]
fn damaged_files_are_reported_in_english() {
    let dir: PathBuf = [env!("CARGO_MANIFEST_DIR"), "tests", "data"]
        .iter()
        .collect();
    let mut files: Vec<Vec<u8>> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "step"))
        .map(|p| std::fs::read(p).unwrap())
        .collect();
    files.sort();
    assert!(!files.is_empty());
    let mut seen = 0usize;
    with_lang(&english(), || {
        let mut check = |bytes: Vec<u8>| {
            let mut texts: Vec<String> = Vec::new();
            match StepFile::parse(bytes, &ReadLimits::DEFAULT, &Progress::new()) {
                Err(e) => texts.push(e.to_string()),
                Ok(file) => {
                    match import_bodies(&file, &ImportOptions::default(), &Progress::new()) {
                        Err(e) => texts.push(e.to_string()),
                        Ok(import) => {
                            texts.extend(import.messages.iter().cloned());
                            for b in &import.bodies {
                                texts.extend(b.warnings.iter().cloned());
                                if let Err(why) = &b.outcome {
                                    texts.push(why.clone());
                                }
                            }
                        }
                    }
                }
            }
            for t in texts {
                // Die Marke einer Fläche („STEP-Fläche n“) ist ein Bezeichner, der in
                // Dokumenten gespeichert wird; sie bleibt in jeder Sprache gleich.
                let t = t.replace("STEP-Fläche", "STEP-face");
                assert_english("STEP-Meldung", &t);
                seen += 1;
            }
        };
        let mut state = 20_261_027u64;
        let mut next = move || {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            (state >> 33) as usize
        };
        for bytes in &files {
            // Abgeschnitten an vielen Stellen.
            let step = (bytes.len() / 25).max(1);
            for cut in (0..bytes.len()).step_by(step) {
                check(bytes[..cut].to_vec());
            }
            // Einzelne Bytes durch Zeichen der STEP-Sprache ersetzt.
            let symbols = b"#();,'.$*=0123456789E-";
            for _ in 0..15 {
                let mut copy = bytes.clone();
                let i = next() % copy.len();
                copy[i] = symbols[next() % symbols.len()];
                check(copy);
            }
        }
    });
    assert!(seen > 20, "zu wenige Meldungen geprüft: {seen}");
}
