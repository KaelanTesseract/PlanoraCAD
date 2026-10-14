//! Fehler beim Lesen und Schreiben von Netzen (Meldungen deutsch, für die Oberfläche bestimmt).

use thiserror::Error;

/// Bytes je Gigabyte (dezimal, für die Meldung).
const BYTES_PER_GB: f64 = 1e9;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ExchangeError {
    #[error("Die Datei lässt sich nicht lesen oder schreiben: {msg}")]
    Io { msg: String },
    #[error(
        "Die Datei ist zu groß ({}); erlaubt sind höchstens 8 GB",
        gigabytes(.bytes)
    )]
    FileTooLarge { bytes: u64 },
    #[error("Das Netz hat mehr als 50 Millionen Dreiecke")]
    TooManyTriangles,
    #[error("Das Netz hat mehr als 150 Millionen Punkte")]
    TooManyVertices,
    #[error("Die Datei ist unvollständig: Sie endet, bevor alles gelesen ist")]
    Truncated,
    #[error("Die Datei gibt {claimed} Dreiecke an, ist aber nur groß genug für {possible}")]
    CountTooLarge { claimed: u64, possible: u64 },
    #[error("{}", not_finite(.line))]
    NotFinite { line: Option<usize> },
    #[error("Zeile {line}: Die Zahl lässt sich nicht lesen")]
    BadNumber { line: usize },
    #[error("Zeile {line}: Die Fläche verweist auf einen Punkt, den es nicht gibt")]
    IndexOutOfRange { line: usize },
    #[error("Zeile {line}: {msg}")]
    Syntax { line: usize, msg: &'static str },
    #[error("Zeile {line}: Das Vieleck lässt sich nicht in Dreiecke zerlegen")]
    Polygon { line: usize },
    #[error("Das Netz enthält keine Dreiecke")]
    Empty,
    #[error(
        "Das Netz ist für das Format {format} zu groß; bitte eine gröbere Qualität oder STL wählen"
    )]
    TooLargeForFormat { format: &'static str },
    #[error("Die Netzdaten sind beschädigt: {0}")]
    Corrupt(String),
    #[error("Die Dateiendung „{0}“ wird nicht unterstützt; möglich sind .stl und .obj")]
    UnknownFormat(String),
    #[error("Abgebrochen")]
    Cancelled,
}

/// Größe in Gigabyte mit Komma („9,1 GB“).
fn gigabytes(bytes: &u64) -> String {
    format!("{:.1} GB", *bytes as f64 / BYTES_PER_GB).replace('.', ",")
}

fn not_finite(line: &Option<usize>) -> String {
    match line {
        Some(n) => format!("Zeile {n}: Eine Zahl ist zu groß oder keine gültige Zahl"),
        None => "Die Datei enthält eine Zahl, die zu groß oder keine gültige Zahl ist".into(),
    }
}

impl From<std::io::Error> for ExchangeError {
    fn from(e: std::io::Error) -> Self {
        if e.kind() == std::io::ErrorKind::UnexpectedEof {
            ExchangeError::Truncated
        } else {
            ExchangeError::Io { msg: e.to_string() }
        }
    }
}
