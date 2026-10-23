//! Zeichenstrom und Parameter einer STEP-Datei (ISO 10303-21), ohne Rekursion.
//!
//! Nachsichtig, wie fremde Schreiber es brauchen: Leerraum (`' ' \t \r \n`) und Kommentare
//! `/* … */` überall außerhalb von Zeichenketten; Schlüsselwörter `[A-Za-z_!][A-Za-z0-9_-]*`
//! in Großbuchstaben; Zahlen `[+-]?Ziffern[.Ziffern*][(E|e)[+-]?Ziffern]` und zusätzlich
//! `[+-]?.Ziffern…`; Zeilenumbrüche innerhalb von Zeichenketten werden übergangen, rohe Bytes
//! ab 0x80 in Zeichenketten als UTF-8 gelesen, wenn gültig, sonst als ISO 8859-1. Derselbe
//! Zerleger prüft im Indexlauf (`build = false`, ohne Werte anzulegen) und baut bei Bedarf die
//! Werte eines Eintrags (`build = true`).

use super::error::{MAX_STEP_NESTING, StepReadError};
use super::value::{Entity, Record, Value};
use planora_exchange::Progress;

/// Abstand der Abbruchprüfungen im Zeichenstrom (1 MiB), damit auch ein riesiger einzelner
/// Eintrag abgebrochen werden kann.
pub const CHECK_BYTES: usize = 1024 * 1024;

type Result<T> = std::result::Result<T, StepReadError>;

/// Ein Wort des Zeichenstroms; Bereiche sind Byte-Stellen im Text des Lexers.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Tok {
    Keyword {
        start: usize,
        end: usize,
    },
    Int(i64),
    Real(f64),
    /// Inhalt zwischen den Hochkommas, noch maskiert.
    Str {
        start: usize,
        end: usize,
    },
    /// Name zwischen den Punkten.
    Enum {
        start: usize,
        end: usize,
    },
    Ref(u64),
    Unset,
    Derived,
    Open,
    Close,
    Comma,
    Semicolon,
    Equals,
    Eof,
}

/// Lexer über einen Byte-Bereich mit Zeilenzählung (`\r\n` zählt einmal, `\r` allein auch).
pub struct Lexer<'a> {
    src: &'a [u8],
    pos: usize,
    line: usize,
    tok_start: usize,
    tok_line: usize,
    end_line: usize,
    watch: Option<&'a Progress>,
    kib_total: u64,
    reported: u64,
    next_check: usize,
}

impl<'a> Lexer<'a> {
    /// Lexer ab dem Anfang von `src`, das in Zeile `line` beginnt.
    pub fn new(src: &'a [u8], line: usize) -> Lexer<'a> {
        Lexer {
            src,
            pos: 0,
            line,
            tok_start: 0,
            tok_line: line,
            end_line: line,
            watch: None,
            kib_total: 0,
            reported: 0,
            next_check: 0,
        }
    }

    /// Meldet Fortschritt (in KiB von `kib_total`) und prüft den Abbruch: beim ersten Wort
    /// und danach je [`CHECK_BYTES`] gelesener Bytes.
    pub fn watch(mut self, progress: &'a Progress, kib_total: u64) -> Lexer<'a> {
        self.watch = Some(progress);
        self.kib_total = kib_total;
        self
    }

    /// Abbruchprüfung und Fortschritt jetzt.
    pub fn checkpoint(&mut self) -> Result<()> {
        let Some(progress) = self.watch else {
            return Ok(());
        };
        if progress.is_cancelled() {
            return Err(StepReadError::Cancelled);
        }
        let done = (self.pos as u64 / 1024).min(self.kib_total);
        progress.advance(done.saturating_sub(self.reported));
        self.reported = self.reported.max(done);
        self.next_check = self.pos.saturating_add(CHECK_BYTES);
        Ok(())
    }

    /// Bisher gemeldeter Fortschritt in KiB.
    pub fn reported(&self) -> u64 {
        self.reported
    }

    /// Übergeht eine UTF-8-Kennung (BOM) am Anfang; die Zeile bleibt 1.
    pub fn skip_bom(&mut self) {
        if self.pos == 0 && self.src.starts_with(&[0xEF, 0xBB, 0xBF]) {
            self.pos = 3;
        }
    }

    /// Jetzige Byte-Stelle (hinter dem zuletzt gelesenen Wort).
    pub fn pos(&self) -> usize {
        self.pos
    }

    /// Byte-Stelle, an der das zuletzt gelesene Wort beginnt.
    pub fn tok_start(&self) -> usize {
        self.tok_start
    }

    /// Zeile, in der das zuletzt gelesene Wort beginnt.
    pub fn tok_line(&self) -> usize {
        self.tok_line
    }

    /// Zeile, in der das Wort davor endet (für „Es fehlt …“).
    pub fn end_line(&self) -> usize {
        self.end_line
    }

    pub fn bytes(&self, start: usize, end: usize) -> &'a [u8] {
        &self.src[start..end]
    }

    /// Fehler „unvollständig“ mit der Zeile des letzten Bytes.
    pub fn truncated(&self) -> StepReadError {
        let mut line = self.line;
        if self.pos >= self.src.len() && matches!(self.src.last(), Some(b'\n' | b'\r')) && line > 1
        {
            line -= 1;
        }
        StepReadError::Truncated { line }
    }

    fn peek(&self) -> Option<u8> {
        self.src.get(self.pos).copied()
    }

    fn peek_at(&self, n: usize) -> Option<u8> {
        self.src.get(self.pos + n).copied()
    }

    /// Geht über ein Zeilenende (`\r\n`, `\n` oder `\r`) an der jetzigen Stelle.
    fn newline(&mut self) {
        if self.peek() == Some(b'\r') && self.peek_at(1) == Some(b'\n') {
            self.pos += 2;
        } else {
            self.pos += 1;
        }
        self.line += 1;
    }

    /// Übergeht Leerraum und Kommentare.
    pub fn skip_space(&mut self) -> Result<()> {
        loop {
            match self.peek() {
                Some(b' ' | b'\t') => self.pos += 1,
                Some(b'\n' | b'\r') => self.newline(),
                Some(b'/') if self.peek_at(1) == Some(b'*') => {
                    let start = self.line;
                    self.pos += 2;
                    loop {
                        match self.peek() {
                            None => return Err(StepReadError::OpenComment { line: start }),
                            Some(b'*') if self.peek_at(1) == Some(b'/') => {
                                self.pos += 2;
                                break;
                            }
                            Some(b'\n' | b'\r') => self.newline(),
                            Some(_) => self.pos += 1,
                        }
                    }
                }
                _ => return Ok(()),
            }
        }
    }

    /// Nächstes Byte nach Leerraum und Kommentaren, ohne es zu verbrauchen.
    pub fn peek_byte(&mut self) -> Result<Option<u8>> {
        self.skip_space()?;
        Ok(self.peek())
    }

    /// Das Zeichen an der Stelle `at` für die Meldung (UTF-8, sonst ISO 8859-1).
    fn char_at(&self, at: usize) -> char {
        let b = self.src[at];
        if b >= 0x80 {
            for w in 2..=4 {
                if let Some(s) = self.src.get(at..at + w)
                    && let Ok(s) = std::str::from_utf8(s)
                    && let Some(c) = s.chars().next()
                {
                    return c;
                }
            }
        }
        char::from(b)
    }

    fn unexpected(&self, at: usize) -> StepReadError {
        StepReadError::UnexpectedChar {
            line: self.line,
            c: self.char_at(at),
        }
    }

    fn digits(&mut self) -> usize {
        let start = self.pos;
        while matches!(self.peek(), Some(b'0'..=b'9')) {
            self.pos += 1;
        }
        self.pos - start
    }

    /// Nächstes Wort.
    pub fn token(&mut self) -> Result<Tok> {
        if self.watch.is_some() && self.pos >= self.next_check {
            self.checkpoint()?;
        }
        self.end_line = self.line;
        self.skip_space()?;
        self.tok_start = self.pos;
        self.tok_line = self.line;
        let Some(b) = self.peek() else {
            return Ok(Tok::Eof);
        };
        let single = match b {
            b'(' => Some(Tok::Open),
            b')' => Some(Tok::Close),
            b',' => Some(Tok::Comma),
            b';' => Some(Tok::Semicolon),
            b'=' => Some(Tok::Equals),
            b'$' => Some(Tok::Unset),
            b'*' => Some(Tok::Derived),
            _ => None,
        };
        if let Some(t) = single {
            self.pos += 1;
            return Ok(t);
        }
        match b {
            b'\'' => self.string(),
            b'#' => self.reference(),
            b'0'..=b'9' => self.number(),
            b'+' | b'-' => match (self.peek_at(1), self.peek_at(2)) {
                (Some(b'0'..=b'9'), _) | (Some(b'.'), Some(b'0'..=b'9')) => self.number(),
                (None, _) | (Some(b'.'), None) => Err(self.at_end()),
                _ => Err(self.unexpected(self.pos)),
            },
            b'.' => match self.peek_at(1) {
                Some(b'0'..=b'9') => self.number(),
                Some(c) if c.is_ascii_alphabetic() || c == b'_' => self.enumeration(),
                None => Err(self.at_end()),
                Some(_) => Err(self.unexpected(self.pos)),
            },
            b'/' if self.peek_at(1).is_none() => Err(self.at_end()),
            b'!' | b'_' | b'A'..=b'Z' | b'a'..=b'z' => self.keyword(),
            _ => Err(self.unexpected(self.pos)),
        }
    }

    /// Das Ende der Daten mitten in einem Wort.
    fn at_end(&mut self) -> StepReadError {
        self.pos = self.src.len();
        self.truncated()
    }

    fn keyword(&mut self) -> Result<Tok> {
        let start = self.pos;
        if self.peek() == Some(b'!') {
            self.pos += 1;
            match self.peek() {
                Some(c) if c.is_ascii_alphabetic() || c == b'_' => {}
                None => return Err(self.at_end()),
                Some(_) => return Err(self.unexpected(start)),
            }
        }
        while matches!(self.peek(), Some(c) if c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
        {
            self.pos += 1;
        }
        Ok(Tok::Keyword {
            start,
            end: self.pos,
        })
    }

    fn enumeration(&mut self) -> Result<Tok> {
        self.pos += 1;
        let start = self.pos;
        while matches!(self.peek(), Some(c) if c.is_ascii_alphanumeric() || c == b'_') {
            self.pos += 1;
        }
        let end = self.pos;
        match self.peek() {
            Some(b'.') => {
                self.pos += 1;
                Ok(Tok::Enum { start, end })
            }
            None => Err(self.at_end()),
            Some(_) => Err(StepReadError::syntax(
                self.line,
                "Eine Aufzählung ist nicht mit „.“ abgeschlossen.",
            )),
        }
    }

    fn reference(&mut self) -> Result<Tok> {
        let start = self.pos;
        self.pos += 1;
        if self.digits() == 0 {
            return match self.peek() {
                None => Err(self.at_end()),
                Some(_) => Err(StepReadError::syntax(
                    self.line,
                    "Nach „#“ fehlt die Nummer.",
                )),
            };
        }
        let text = ascii(&self.src[start..self.pos]);
        text[1..]
            .parse::<u64>()
            .map(Tok::Ref)
            .map_err(|_| StepReadError::bad_number(self.line, text))
    }

    fn number(&mut self) -> Result<Tok> {
        let start = self.pos;
        if matches!(self.peek(), Some(b'+' | b'-')) {
            self.pos += 1;
        }
        self.digits();
        let mut real = false;
        if self.peek() == Some(b'.') {
            real = true;
            self.pos += 1;
            self.digits();
        }
        if matches!(self.peek(), Some(b'E' | b'e')) {
            real = true;
            self.pos += 1;
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.pos += 1;
            }
            if self.digits() == 0 {
                if self.peek().is_none() {
                    return Err(self.at_end());
                }
                return Err(StepReadError::bad_number(
                    self.line,
                    ascii(&self.src[start..self.pos]),
                ));
            }
        }
        let text = ascii(&self.src[start..self.pos]);
        let bad = || StepReadError::bad_number(self.line, text);
        if real {
            match text.parse::<f64>() {
                Ok(x) if x.is_finite() => Ok(Tok::Real(x)),
                _ => Err(bad()),
            }
        } else {
            text.parse::<i64>().map(Tok::Int).map_err(|_| bad())
        }
    }

    /// Zeichenkette bis zum schließenden Hochkomma (`''` ist ein Hochkomma); der Inhalt wird
    /// gleich geprüft.
    fn string(&mut self) -> Result<Tok> {
        let line = self.line;
        self.pos += 1;
        let start = self.pos;
        loop {
            match self.peek() {
                None => return Err(StepReadError::OpenString { line }),
                Some(b'\'') if self.peek_at(1) == Some(b'\'') => self.pos += 2,
                Some(b'\'') => break,
                Some(b'\n' | b'\r') => self.newline(),
                Some(_) => self.pos += 1,
            }
        }
        let end = self.pos;
        self.pos += 1;
        decode(&self.src[start..end], line, None)?;
        Ok(Tok::Str { start, end })
    }

    /// Entmaskierter Text eines `Tok::Str`.
    pub fn text(&self, start: usize, end: usize) -> Result<String> {
        let mut out = String::with_capacity(end - start);
        decode(&self.src[start..end], self.tok_line, Some(&mut out))?;
        Ok(out)
    }

    /// Schlüsselwort in Großbuchstaben.
    pub fn upper(&self, start: usize, end: usize) -> String {
        ascii(&self.src[start..end]).to_ascii_uppercase()
    }

    /// Ist das Wort das Schlüsselwort `kw` (ohne Rücksicht auf Groß/klein)?
    pub fn is_keyword(&self, tok: Tok, kw: &str) -> bool {
        matches!(tok, Tok::Keyword { start, end } if self.src[start..end].eq_ignore_ascii_case(kw.as_bytes()))
    }
}

/// Die Bytes sind nach der Prüfung des Lexers ASCII.
fn ascii(bytes: &[u8]) -> &str {
    std::str::from_utf8(bytes).unwrap_or("")
}

fn hex(bytes: &[u8]) -> Option<u32> {
    if bytes.is_empty() || !bytes.iter().all(u8::is_ascii_hexdigit) {
        return None;
    }
    u32::from_str_radix(ascii(bytes), 16).ok()
}

/// Entmaskiert den Inhalt einer Zeichenkette (ohne die äußeren Hochkommas); mit `out = None`
/// wird nur geprüft. `''` → `'`, `\\` → `\`, `\X\hh` (ein Byte ISO 8859-1), `\X2\…\X0\`
/// (UTF-16, auch Ersatzpaare), `\X4\…\X0\`, `\S\c` (`c` + 128), `\Px\` wird übergangen; ein
/// anderer einzelner Rückstrich bleibt stehen. Rohe Bytes ab 0x80: gültiges UTF-8 als UTF-8,
/// sonst ISO 8859-1. Zeilenumbrüche fallen weg (ISO 10303-21, 3. Ausgabe).
pub fn decode(s: &[u8], line: usize, mut out: Option<&mut String>) -> Result<()> {
    let bad = || StepReadError::syntax(line, "Eine Maskierung in einer Zeichenkette ist ungültig.");
    let mut push = |c: char| {
        if let Some(o) = out.as_deref_mut() {
            o.push(c);
        }
    };
    let at = |i: usize, b: u8| s.get(i).is_some_and(|c| c.eq_ignore_ascii_case(&b));
    let mut i = 0;
    while i < s.len() {
        let b = s[i];
        match b {
            b'\'' => {
                // Nur als Paar `''` möglich.
                push('\'');
                i += 2;
            }
            b'\n' | b'\r' => i += 1,
            b'\\' if at(i + 1, b'\\') => {
                push('\\');
                i += 2;
            }
            b'\\' if at(i + 1, b'X') && at(i + 2, b'\\') => {
                let c = s.get(i + 3..i + 5).and_then(hex).ok_or_else(bad)?;
                push(char::from(c as u8));
                i += 5;
            }
            b'\\'
                if at(i + 1, b'X') && (at(i + 2, b'2') || at(i + 2, b'4')) && at(i + 3, b'\\') =>
            {
                let wide = at(i + 2, b'4');
                let width = if wide { 8 } else { 4 };
                i += 4;
                let mut high: Option<u32> = None;
                loop {
                    if at(i, b'\\') && at(i + 1, b'X') && at(i + 2, b'0') && at(i + 3, b'\\') {
                        i += 4;
                        break;
                    }
                    let unit = s.get(i..i + width).and_then(hex).ok_or_else(bad)?;
                    i += width;
                    let code = match (wide, high.take(), unit) {
                        (true, _, u) => u,
                        (false, None, 0xD800..=0xDBFF) => {
                            high = Some(unit);
                            continue;
                        }
                        (false, Some(h), 0xDC00..=0xDFFF) => {
                            0x10000 + ((h - 0xD800) << 10) + (unit - 0xDC00)
                        }
                        (false, Some(_), _) | (false, None, 0xDC00..=0xDFFF) => {
                            return Err(bad());
                        }
                        (false, None, u) => u,
                    };
                    push(char::from_u32(code).ok_or_else(bad)?);
                }
                if high.is_some() {
                    return Err(bad());
                }
            }
            b'\\' if at(i + 1, b'S') && at(i + 2, b'\\') => {
                let c = *s.get(i + 3).ok_or_else(bad)?;
                if c >= 0x80 {
                    return Err(bad());
                }
                push(char::from(c + 0x80));
                // Ein Hochkomma steht als `''`.
                i += if c == b'\'' { 5 } else { 4 };
            }
            b'\\' if at(i + 1, b'P') && s.get(i + 2).is_some() && at(i + 3, b'\\') => i += 4,
            _ => {
                // Rohe Bytes: gültiges UTF-8 als UTF-8, sonst ISO 8859-1.
                let width = match b {
                    0xC2..=0xDF => 2,
                    0xE0..=0xEF => 3,
                    0xF0..=0xF4 => 4,
                    _ => 1,
                };
                let utf8 = (width > 1)
                    .then(|| s.get(i..i + width))
                    .flatten()
                    .and_then(|w| std::str::from_utf8(w).ok())
                    .and_then(|w| w.chars().next());
                match utf8 {
                    Some(c) => {
                        push(c);
                        i += width;
                    }
                    None => {
                        push(char::from(b));
                        i += 1;
                    }
                }
            }
        }
    }
    Ok(())
}

/// Ergebnis eines Eintragskörpers (Teil zwischen „=“ und „;“).
pub struct Body {
    pub complex: bool,
    /// Nur mit `build = true`.
    pub entity: Option<Entity>,
}

/// Zerlegt den Eintragskörper `TYP(…)` oder `(A(…) B(…) …)`; die Lage der Typnamen kommt nach
/// `names` (vorher geleert). Mit `build = false` wird nur geprüft.
pub fn parse_body(lex: &mut Lexer, build: bool, names: &mut Vec<(usize, usize)>) -> Result<Body> {
    names.clear();
    match lex.token()? {
        Tok::Keyword { start, end } => {
            names.push((start, end));
            let args = parse_args(lex, 0, build, (start, end))?;
            Ok(Body {
                complex: false,
                entity: build.then(|| {
                    Entity::Simple(Record {
                        name: lex.upper(start, end),
                        args,
                    })
                }),
            })
        }
        Tok::Open => {
            let mut parts = Vec::new();
            loop {
                match lex.token()? {
                    Tok::Keyword { start, end } => {
                        names.push((start, end));
                        let args = parse_args(lex, 1, build, (start, end))?;
                        if build {
                            parts.push(Record {
                                name: lex.upper(start, end),
                                args,
                            });
                        }
                    }
                    Tok::Close if !names.is_empty() => break,
                    Tok::Close => {
                        return Err(StepReadError::syntax(
                            lex.tok_line(),
                            "Ein zusammengesetzter Eintrag ist leer.",
                        ));
                    }
                    Tok::Eof => return Err(lex.truncated()),
                    _ => {
                        return Err(StepReadError::syntax(
                            lex.tok_line(),
                            "In einem zusammengesetzten Eintrag wird ein Typname erwartet.",
                        ));
                    }
                }
            }
            Ok(Body {
                complex: true,
                entity: build.then_some(Entity::Complex(parts)),
            })
        }
        Tok::Eof => Err(lex.truncated()),
        _ => Err(StepReadError::syntax(
            lex.tok_line(),
            "Nach „=“ fehlt der Typname.",
        )),
    }
}

struct Frame {
    typed: Option<String>,
    items: Vec<Value>,
}

/// Zerlegt `( … )` nach dem Typnamen `name` (Bereich im Text) ohne Rekursion; `base` ist die
/// Tiefe außerhalb (0 für einfache Einträge, 1 für Teile zusammengesetzter).
pub fn parse_args(
    lex: &mut Lexer,
    base: usize,
    build: bool,
    name: (usize, usize),
) -> Result<Vec<Value>> {
    match lex.token()? {
        Tok::Open => {}
        Tok::Eof => return Err(lex.truncated()),
        _ => return Err(missing_open(lex, name)),
    }
    let mut stack = vec![Frame {
        typed: None,
        items: Vec::new(),
    }];
    if base + 1 > MAX_STEP_NESTING {
        return Err(StepReadError::TooDeep {
            line: lex.tok_line(),
        });
    }
    // `want_value`: nach „(“ oder „,“; `may_close`: „)“ ist erlaubt.
    let mut want_value = true;
    let mut may_close = true;
    loop {
        let tok = lex.token()?;
        let line = lex.tok_line();
        let value = match tok {
            Tok::Close => {
                if !may_close {
                    return Err(StepReadError::syntax(line, "Nach „,“ fehlt ein Wert."));
                }
                // Der Stapel ist hier nie leer: die äußerste „)“ beendet die Schleife unten;
                // eine weitere „)“ meldet der Aufrufer („Zu viele „)“.“).
                let Some(frame) = stack.pop() else {
                    return Err(StepReadError::syntax(line, TOO_MANY_CLOSE));
                };
                let Some(parent) = stack.last_mut() else {
                    return Ok(frame.items);
                };
                if build {
                    parent.items.push(match frame.typed {
                        Some(n) => Value::Typed(n, frame.items),
                        None => Value::List(frame.items),
                    });
                }
                want_value = false;
                may_close = true;
                continue;
            }
            Tok::Comma => {
                if want_value {
                    return Err(StepReadError::syntax(line, "Hier fehlt ein Wert."));
                }
                want_value = true;
                may_close = false;
                continue;
            }
            Tok::Semicolon => {
                return Err(StepReadError::syntax(
                    line,
                    "Es fehlt eine schließende Klammer.",
                ));
            }
            Tok::Eof => return Err(lex.truncated()),
            Tok::Equals => return Err(StepReadError::syntax(line, "Unerwartetes „=“.")),
            _ if !want_value => {
                return Err(StepReadError::syntax(
                    line,
                    "Zwischen zwei Werten fehlt ein „,“.",
                ));
            }
            Tok::Open | Tok::Keyword { .. } => {
                let typed = if let Tok::Keyword { start, end } = tok {
                    match lex.token()? {
                        Tok::Open => {}
                        Tok::Eof => return Err(lex.truncated()),
                        _ => return Err(missing_open(lex, (start, end))),
                    }
                    build.then(|| lex.upper(start, end))
                } else {
                    None
                };
                stack.push(Frame {
                    typed,
                    items: Vec::new(),
                });
                if base + stack.len() > MAX_STEP_NESTING {
                    return Err(StepReadError::TooDeep { line });
                }
                want_value = true;
                may_close = true;
                continue;
            }
            Tok::Int(i) => Value::Int(i),
            Tok::Real(x) => Value::Real(x),
            Tok::Ref(r) => Value::Ref(r),
            Tok::Unset => Value::Unset,
            Tok::Derived => Value::Derived,
            Tok::Str { start, end } => {
                if build {
                    Value::Str(lex.text(start, end)?)
                } else {
                    Value::Unset
                }
            }
            Tok::Enum { start, end } => {
                if build {
                    Value::Enum(lex.upper(start, end))
                } else {
                    Value::Unset
                }
            }
        };
        if build && let Some(top) = stack.last_mut() {
            top.items.push(value);
        }
        want_value = false;
        may_close = true;
    }
}

/// Meldung für eine „)“ zu viel.
pub const TOO_MANY_CLOSE: &str = "Zu viele „)“.";

fn missing_open(lex: &Lexer, (start, end): (usize, usize)) -> StepReadError {
    StepReadError::syntax(
        lex.end_line(),
        format!("Nach „{}“ fehlt „(“.", lex.upper(start, end)),
    )
}
