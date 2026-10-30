//! Fehler und Grenzen des STEP-Lesers. Die Meldungen sind deutsch und für die Oberfläche
//! bestimmt; Zeilen zählen ab 1.

use planora_i18n::{fmt_num, tr};
use std::fmt;

/// Größte Datei, die gelesen wird (200 MB).
pub const MAX_STEP_FILE_BYTES: u64 = 200 * 1024 * 1024;
/// Höchstens so viele Einträge (`#n = …;`) je Datei.
pub const MAX_STEP_ENTITIES: usize = 5_000_000;
/// Höchstens so viele Körper (Vorkommen) je Datei.
pub const MAX_STEP_BODIES: usize = 50;
/// Höchstens so viele Flächen je Körper.
pub const MAX_STEP_FACES: usize = 20_000;
/// Höchste Klammertiefe eines Eintrags; die Parameterliste des Eintrags selbst ist Ebene 1.
pub const MAX_STEP_NESTING: usize = 64;

/// Höchste Länge einer Verweiskette, der [`crate::read::index::RefPath`] folgt.
pub const MAX_REF_DEPTH: usize = 256;

/// Höchstens so viele Zeichen einer fehlerhaften Zahl stehen in der Meldung.
pub const MAX_BAD_NUMBER_CHARS: usize = 32;

/// Grenzen eines Lesevorgangs (Vorgabe: die Konstanten oben; Tests setzen kleinere).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReadLimits {
    pub max_bytes: u64,
    pub max_entities: usize,
    pub max_bodies: usize,
    pub max_faces: usize,
}

impl ReadLimits {
    pub const DEFAULT: ReadLimits = ReadLimits {
        max_bytes: MAX_STEP_FILE_BYTES,
        max_entities: MAX_STEP_ENTITIES,
        max_bodies: MAX_STEP_BODIES,
        max_faces: MAX_STEP_FACES,
    };
}

impl Default for ReadLimits {
    fn default() -> Self {
        ReadLimits::DEFAULT
    }
}

/// Fehler beim Lesen einer STEP-Datei.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StepReadError {
    NotStep,
    Truncated {
        line: usize,
    },
    TooLarge {
        bytes: u64,
    },
    TooManyEntities,
    Duplicate {
        line: usize,
        id: u64,
        first: usize,
    },
    MissingRef {
        line: usize,
        id: u64,
        target: u64,
    },
    Cycle {
        line: usize,
        id: u64,
    },
    BadNumber {
        line: usize,
        text: String,
    },
    TooDeep {
        line: usize,
    },
    UnexpectedChar {
        line: usize,
        c: char,
    },
    OpenString {
        line: usize,
    },
    OpenComment {
        line: usize,
    },
    Syntax {
        line: usize,
        msg: String,
    },
    Io {
        msg: String,
    },
    NoBodies,
    TooManyBodies,
    Cancelled,
    /// Ein Eintrag ohne eigenen Kontext (freier Volumen-Eintrag), während die Kontexte der Datei
    /// verschiedene Einheiten nennen (Aufgabe 9).
    UnclearUnits,
    /// Programmfehler des Aufrufers: nach einer Nummer gefragt, die es nicht gibt.
    NoEntity {
        id: u64,
    },
}

impl fmt::Display for StepReadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = match self {
            StepReadError::NotStep => tr!("step-read-not-step"),
            StepReadError::Truncated { line } => tr!("step-read-truncated", line = *line),
            StepReadError::TooLarge { bytes } => {
                tr!("step-read-too-large", mb = megabytes(*bytes))
            }
            StepReadError::TooManyEntities => tr!("step-read-too-many-entities"),
            StepReadError::Duplicate { line, id, first } => {
                tr!(
                    "step-read-duplicate",
                    line = *line,
                    id = *id,
                    first = *first
                )
            }
            StepReadError::MissingRef { line, id, target } => {
                tr!(
                    "step-read-missing-ref",
                    line = *line,
                    id = *id,
                    target = *target
                )
            }
            StepReadError::Cycle { line, id } => tr!("step-read-cycle", line = *line, id = *id),
            StepReadError::BadNumber { line, text } => {
                tr!("step-read-bad-number", line = *line, text = shorten(text))
            }
            StepReadError::TooDeep { line } => tr!("step-read-too-deep", line = *line),
            StepReadError::UnexpectedChar { line, c } => {
                tr!("step-read-unexpected-char", line = *line, c = show_char(*c))
            }
            StepReadError::OpenString { line } => tr!("step-read-open-string", line = *line),
            StepReadError::OpenComment { line } => tr!("step-read-open-comment", line = *line),
            StepReadError::Syntax { line, msg } => {
                tr!("step-read-syntax", line = *line, msg = msg)
            }
            StepReadError::Io { msg } => tr!("step-read-io", msg = msg),
            StepReadError::NoBodies => tr!("step-read-no-bodies"),
            StepReadError::TooManyBodies => tr!("step-read-too-many-bodies"),
            StepReadError::Cancelled => tr!("step-cancelled"),
            StepReadError::UnclearUnits => tr!("step-read-unclear-units"),
            StepReadError::NoEntity { id } => tr!("step-read-no-entity", id = *id),
        };
        f.write_str(&text)
    }
}

impl std::error::Error for StepReadError {}

impl StepReadError {
    /// `BadNumber` mit höchstens [`MAX_BAD_NUMBER_CHARS`] Zeichen des Textes.
    pub fn bad_number(line: usize, text: &str) -> StepReadError {
        StepReadError::BadNumber {
            line,
            text: shorten(text),
        }
    }

    /// `Syntax` mit Zeile und Grund.
    pub fn syntax(line: usize, msg: impl Into<String>) -> StepReadError {
        StepReadError::Syntax {
            line,
            msg: msg.into(),
        }
    }
}

/// Bytes in MB (1024·1024) mit einer Nachkommastelle und dem Dezimaltrenner der Sprache.
fn megabytes(bytes: u64) -> String {
    fmt_num(bytes as f64 / (1024.0 * 1024.0), 1)
}

/// Steuer- und unsichtbare Zeichen erscheinen als `U+XXXX`, nie roh.
fn show_char(c: char) -> String {
    if c.is_control() || c.is_whitespace() || c == '\u{FEFF}' {
        format!("U+{:04X}", u32::from(c))
    } else {
        c.to_string()
    }
}

fn shorten(text: &str) -> String {
    text.chars().take(MAX_BAD_NUMBER_CHARS).collect()
}
