//! Eintragsverwaltung (Spec §2, §4): fortlaufende Nummern `#1, #2, …`, ein Eintrag je Zeile
//! (lange Einträge nach einem Komma umbrochen), höchstens [`MAX_ENTITIES`] Einträge, Abbrechen
//! über [`Progress`]. Geschrieben wird sofort in den Ausgabestrom, nichts sammelt sich an.

use crate::error::StepError;
use planora_exchange::{CHECK_EVERY, Progress};
use std::fmt::Write as _;
use std::io::Write;

/// Höchstzahl der Einträge einer Datei (Spec §4).
pub const MAX_ENTITIES: u64 = 500_000_000;
/// Ab dieser Spalte wird ein langer Eintrag nach dem nächsten Komma außerhalb einer
/// Zeichenkette umbrochen (die Zeilen bleiben für Menschen und strenge Leser lesbar).
pub const WRAP_COLUMN: usize = 72;

/// Schreibt Einträge mit fortlaufender Nummer.
pub struct Entities<'a> {
    out: &'a mut dyn Write,
    progress: &'a Progress,
    next: u64,
    /// Jeder Eintrag zählt im Fortschritt (beim Probelauf nicht).
    count_progress: bool,
    line: String,
}

impl<'a> Entities<'a> {
    /// Schreibt nach `out`; jeder Eintrag zählt einen Schritt in `progress`.
    pub fn new(out: &'a mut dyn Write, progress: &'a Progress) -> Entities<'a> {
        Entities {
            out,
            progress,
            next: 1,
            count_progress: true,
            line: String::new(),
        }
    }

    /// Probelauf (etwa nach `std::io::sink()`): zählt nicht im Fortschritt, bricht aber ab.
    pub fn probe(out: &'a mut dyn Write, progress: &'a Progress) -> Entities<'a> {
        Entities {
            count_progress: false,
            ..Entities::new(out, progress)
        }
    }

    /// Zahl der bisher geschriebenen Einträge.
    pub fn count(&self) -> u64 {
        self.next - 1
    }

    /// Setzt die nächste Nummer (nur für Tests der Grenze [`MAX_ENTITIES`]).
    pub fn set_next(&mut self, next: u64) {
        self.next = next;
    }

    /// Schreibt `#n=text;` und liefert `n`. Alle [`CHECK_EVERY`] Einträge wird gefragt, ob
    /// abgebrochen werden soll.
    pub fn push(&mut self, text: &str) -> Result<u64, StepError> {
        if self.next > MAX_ENTITIES {
            return Err(StepError::TooManyEntities);
        }
        if self.next.is_multiple_of(CHECK_EVERY) && self.progress.is_cancelled() {
            return Err(StepError::Cancelled);
        }
        let id = self.next;
        self.line.clear();
        let _ = write!(self.line, "#{id}=");
        push_wrapped(&mut self.line, text);
        self.line.push_str(";\n");
        self.out.write_all(self.line.as_bytes())?;
        self.next += 1;
        if self.count_progress {
            self.progress.advance(1);
        }
        Ok(id)
    }

    /// Schreibt eine Zeile ohne Nummer (Kopf und Abschnittsgrenzen).
    pub fn line(&mut self, text: &str) -> Result<(), StepError> {
        self.out.write_all(text.as_bytes())?;
        self.out.write_all(b"\n")?;
        Ok(())
    }

    pub fn flush(&mut self) -> Result<(), StepError> {
        self.out.flush()?;
        Ok(())
    }
}

/// Hängt `text` an `line` an und bricht nach einem Komma außerhalb einer Zeichenkette um, sobald
/// die Zeile [`WRAP_COLUMN`] Zeichen erreicht hat. `''` in einer Zeichenkette schaltet zweimal
/// um und ändert daher nichts.
fn push_wrapped(line: &mut String, text: &str) {
    let mut column = line.len();
    let mut in_string = false;
    for c in text.chars() {
        line.push(c);
        column += 1;
        if c == '\'' {
            in_string = !in_string;
        } else if c == ',' && !in_string && column >= WRAP_COLUMN {
            line.push('\n');
            column = 0;
        }
    }
}

/// Liste von Verweisen: `(#1,#2,#3)`.
pub fn refs(ids: &[u64]) -> String {
    let mut s = String::with_capacity(2 + ids.len() * 8);
    s.push('(');
    for (i, id) in ids.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        let _ = write!(s, "#{id}");
    }
    s.push(')');
    s
}

/// STEP-Wahrheitswert: `.T.` oder `.F.`.
pub fn logical(b: bool) -> &'static str {
    if b { ".T." } else { ".F." }
}
