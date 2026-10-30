//! Gemeinsames für das Lesen: Zeilen mit allen Zeilenenden (P4), Byte-Zähler für den
//! Fortschritt, Zahlen mit Zeilennummer.

use crate::error::ExchangeError;
use crate::limits::MAX_LINE_BYTES;
use crate::progress::Progress;
use planora_i18n::tr;
use std::io::{BufRead, Read};

/// Zählt gelesene Bytes in `progress`.
pub(crate) struct CountingReader<'a, R> {
    inner: R,
    progress: &'a Progress,
}

impl<'a, R> CountingReader<'a, R> {
    pub(crate) fn new(inner: R, progress: &'a Progress) -> Self {
        Self { inner, progress }
    }
}

impl<R: Read> Read for CountingReader<'_, R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.progress.advance(n as u64);
        Ok(n)
    }
}

/// Liest Zeilen mit `\n`, `\r\n` oder `\r` als Ende; ein UTF-8-BOM am Dateianfang entfällt.
/// Zeilen über [`MAX_LINE_BYTES`] sind ein Fehler.
pub(crate) struct LineReader<R> {
    inner: R,
    buf: Vec<u8>,
    line: usize,
    /// Das letzte Zeilenende war `\r`; ein unmittelbar folgendes `\n` gehört noch dazu.
    after_cr: bool,
    done: bool,
}

impl<R: BufRead> LineReader<R> {
    pub(crate) fn new(inner: R) -> Self {
        Self {
            inner,
            buf: Vec::new(),
            line: 0,
            after_cr: false,
            done: false,
        }
    }

    /// Nächste Zeile: Nummer (ab 1) und Inhalt ohne Zeilenende; `None` am Dateiende.
    pub(crate) fn next_line(&mut self) -> Result<Option<(usize, &[u8])>, ExchangeError> {
        if self.done {
            return Ok(None);
        }
        self.buf.clear();
        loop {
            let chunk = match self.inner.fill_buf() {
                Ok(c) => c,
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(e.into()),
            };
            if chunk.is_empty() {
                self.done = true;
                if self.buf.is_empty() {
                    return Ok(None);
                }
                return Ok(Some(self.finish_line()));
            }
            let start = usize::from(std::mem::take(&mut self.after_cr) && chunk[0] == b'\n');
            let rest = &chunk[start..];
            let end = rest.iter().position(|&b| b == b'\n' || b == b'\r');
            let take = end.unwrap_or(rest.len());
            if self.buf.len() + take > MAX_LINE_BYTES {
                return Err(ExchangeError::Syntax {
                    line: self.line + 1,
                    msg: tr!("exchange-line-too-long"),
                });
            }
            self.buf.extend_from_slice(&rest[..take]);
            match end {
                Some(i) => {
                    self.after_cr = rest[i] == b'\r';
                    self.inner.consume(start + i + 1);
                    return Ok(Some(self.finish_line()));
                }
                None => {
                    let used = chunk.len();
                    self.inner.consume(used);
                }
            }
        }
    }

    fn finish_line(&mut self) -> (usize, &[u8]) {
        self.line += 1;
        if self.line == 1 && self.buf.starts_with(&[0xEF, 0xBB, 0xBF]) {
            self.buf.drain(..3);
        }
        (self.line, &self.buf)
    }
}

/// Liest drei Zahlen: fehlende → `Syntax`, unlesbare → `BadNumber`, unendliche oder NaN →
/// `NotFinite`, jeweils mit Zeile.
pub(crate) fn three_numbers<'a>(
    tokens: &mut impl Iterator<Item = &'a str>,
    line: usize,
) -> Result<[f32; 3], ExchangeError> {
    let mut out = [0.0f32; 3];
    for v in &mut out {
        let token = tokens.next().ok_or_else(|| ExchangeError::Syntax {
            line,
            msg: tr!("exchange-number-missing"),
        })?;
        let x: f32 = token
            .parse()
            .map_err(|_| ExchangeError::BadNumber { line })?;
        if !x.is_finite() {
            return Err(ExchangeError::NotFinite { line: Some(line) });
        }
        *v = x;
    }
    Ok(out)
}
