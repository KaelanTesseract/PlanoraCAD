//! Fehler beim Lesen und Schreiben von Netzen (Meldungen deutsch, für die Oberfläche bestimmt).

use planora_i18n::{fmt_num, tr};
use std::fmt;

/// Bytes je Gigabyte (dezimal, für die Meldung).
const BYTES_PER_GB: f64 = 1e9;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExchangeError {
    Io { msg: String },
    FileTooLarge { bytes: u64 },
    TooManyTriangles,
    TooManyVertices,
    Truncated,
    CountTooLarge { claimed: u64, possible: u64 },
    NotFinite { line: Option<usize> },
    BadNumber { line: usize },
    IndexOutOfRange { line: usize },
    Syntax { line: usize, msg: String },
    Polygon { line: usize },
    Empty,
    TooLargeForFormat { format: &'static str },
    Corrupt(String),
    UnknownFormat(String),
    Cancelled,
}

impl fmt::Display for ExchangeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = match self {
            ExchangeError::Io { msg } => tr!("exchange-io", msg = msg),
            ExchangeError::FileTooLarge { bytes } => {
                tr!("exchange-file-too-large", size = gigabytes(bytes))
            }
            ExchangeError::TooManyTriangles => tr!("exchange-too-many-triangles"),
            ExchangeError::TooManyVertices => tr!("exchange-too-many-vertices"),
            ExchangeError::Truncated => tr!("exchange-truncated"),
            ExchangeError::CountTooLarge { claimed, possible } => {
                tr!(
                    "exchange-count-too-large",
                    claimed = *claimed,
                    possible = *possible
                )
            }
            ExchangeError::NotFinite { line } => not_finite(line),
            ExchangeError::BadNumber { line } => tr!("exchange-bad-number", line = *line),
            ExchangeError::IndexOutOfRange { line } => {
                tr!("exchange-index-out-of-range", line = *line)
            }
            ExchangeError::Syntax { line, msg } => {
                tr!("exchange-syntax", line = *line, msg = msg)
            }
            ExchangeError::Polygon { line } => tr!("exchange-polygon", line = *line),
            ExchangeError::Empty => tr!("exchange-empty"),
            ExchangeError::TooLargeForFormat { format } => {
                tr!("exchange-too-large-for-format", format = *format)
            }
            ExchangeError::Corrupt(m) => tr!("exchange-corrupt", msg = m),
            ExchangeError::UnknownFormat(ext) => tr!("exchange-unknown-format", ext = ext),
            ExchangeError::Cancelled => tr!("exchange-cancelled"),
        };
        f.write_str(&text)
    }
}

impl std::error::Error for ExchangeError {}

/// Größe in Gigabyte mit dem Dezimaltrenner der Sprache („9,1 GB“).
fn gigabytes(bytes: &u64) -> String {
    format!("{} GB", fmt_num(*bytes as f64 / BYTES_PER_GB, 1))
}

fn not_finite(line: &Option<usize>) -> String {
    match line {
        Some(n) => tr!("exchange-not-finite-line", line = *n),
        None => tr!("exchange-not-finite"),
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
