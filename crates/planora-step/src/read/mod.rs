//! STEP-Import: eigener Leser für STEP-Dateien als Text (ISO 10303-21).
//!
//! P1 – Lesen in zwei Stufen: Ein Indexlauf zerlegt die Datei in Einträge (Nummer →
//! Byte-Bereich und Zeile; dazu je Typname die Nummern) und prüft Kopf, Struktur, Syntax,
//! Zahlen, Klammertiefe, doppelte Nummern und Grenzen. Die Parameter eines Eintrags werden erst
//! beim Gebrauch zerlegt (`StepFile::entity`); fehlende Verweise fallen dort auf und treffen nur
//! die Körper, die sie brauchen. Speicher: Dateibytes plus etwa 32 Bytes je Eintrag.

pub mod error;
pub mod geom;
pub mod index;
pub mod lex;
pub mod topo;
pub mod units;
pub mod value;
