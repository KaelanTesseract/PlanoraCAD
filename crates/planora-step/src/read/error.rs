//! Fehler und Grenzen des STEP-Lesers. Die Meldungen sind deutsch und für die Oberfläche
//! bestimmt; Zeilen zählen ab 1.

use thiserror::Error;

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
#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum StepReadError {
    #[error("Die Datei ist keine STEP-Datei (am Anfang fehlt „ISO-10303-21;“).")]
    NotStep,
    #[error("Die Datei ist unvollständig: Sie endet in Zeile {line}, bevor alles gelesen ist.")]
    Truncated { line: usize },
    #[error("Die Datei ist zu groß ({} MB); erlaubt sind höchstens 200 MB.", megabytes(*.bytes))]
    TooLarge { bytes: u64 },
    #[error("Die Datei hat mehr als 5 Millionen Einträge.")]
    TooManyEntities,
    #[error("Zeile {line}: Die Nummer #{id} kommt doppelt vor.")]
    Duplicate { line: usize, id: u64 },
    #[error("Zeile {line}: #{id} verweist auf #{target}, den es nicht gibt.")]
    MissingRef { line: usize, id: u64, target: u64 },
    #[error("Zeile {line}: Die Einträge verweisen im Kreis (#{id}).")]
    Cycle { line: usize, id: u64 },
    #[error("Zeile {line}: Die Zahl „{}“ ist zu groß oder keine gültige Zahl.", shorten(.text))]
    BadNumber { line: usize, text: String },
    #[error("Zeile {line}: Die Klammern sind zu tief verschachtelt (höchstens 64 Ebenen).")]
    TooDeep { line: usize },
    #[error("Zeile {line}: Unerwartetes Zeichen „{c}“.")]
    UnexpectedChar { line: usize, c: char },
    #[error("Zeile {line}: Eine Zeichenkette ist nicht geschlossen.")]
    OpenString { line: usize },
    #[error("Zeile {line}: Ein Kommentar ist nicht geschlossen.")]
    OpenComment { line: usize },
    #[error("Zeile {line}: {msg}")]
    Syntax { line: usize, msg: String },
    #[error("Die Datei lässt sich nicht lesen: {msg}")]
    Io { msg: String },
    #[error("Die Datei enthält keine Volumenkörper.")]
    NoBodies,
    #[error("Die Datei enthält mehr als 50 Körper.")]
    TooManyBodies,
    #[error("Abgebrochen")]
    Cancelled,
}

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

/// Bytes in MB (1024·1024) mit einer Nachkommastelle und Komma.
fn megabytes(bytes: u64) -> String {
    format!("{:.1}", bytes as f64 / (1024.0 * 1024.0)).replace('.', ",")
}

fn shorten(text: &str) -> String {
    text.chars().take(MAX_BAD_NUMBER_CHARS).collect()
}
