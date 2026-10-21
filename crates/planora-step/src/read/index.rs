//! Indexlauf über eine STEP-Datei (P1): prüft Kopf, Abschnitte, Syntax aller Einträge, Tiefe,
//! Zahlen, doppelte Nummern und Grenzen, merkt sich je Eintrag nur Byte-Bereich und Zeile.
//! Die Parameter eines Eintrags werden erst in [`StepFile::entity`] zerlegt; dort fallen
//! fehlende Verweise auf.
//!
//! Für die Folgeaufgaben: `entity` zerlegt bei **jedem** Aufruf neu und legt Werte an; wer
//! Einträge mehrfach braucht (Punkte, Richtungen), speichert die Ergebnisse selbst zwischen
//! (Nummer → Ergebnis). `entity` erkennt nur Selbstverweise; wer Verweisen folgt, muss längere
//! Kreise selbst erkennen, mit Besuchsliste und Tiefengrenze: dafür gibt es [`RefPath`].

use super::error::{MAX_REF_DEPTH, ReadLimits, StepReadError};
use super::lex::{Lexer, TOO_MANY_CLOSE, Tok, parse_args, parse_body};
use super::value::{Entity, Value};
use planora_exchange::{CHECK_EVERY, Progress};
use std::collections::HashMap;
use std::io::Read;
use std::path::Path;

type Result<T> = std::result::Result<T, StepReadError>;

/// Kopfeinträge, die größer sind, werden nur geprüft, nicht gelesen (Schutz vor riesigen
/// Köpfen).
const MAX_HEADER_ENTRY_BYTES: usize = 64 * 1024;

const MAGIC: &str = "ISO-10303-21";
const END_MAGIC: &str = "END-ISO-10303-21";
const BOM: &[u8] = &[0xEF, 0xBB, 0xBF];

/// Was der Kopf über die Datei sagt.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StepHeader {
    /// Schemanamen aus `FILE_SCHEMA`.
    pub schemas: Vec<String>,
    /// Erstes Feld von `FILE_NAME`.
    pub file_name: String,
    /// Sechstes Feld von `FILE_NAME` (`originating_system`).
    pub system: String,
}

/// Ein Eintrag im Index: Bereich zwischen „=“ und „;“ und Zeile von „#n“ (24 Bytes).
#[derive(Clone, Copy, Debug)]
struct Slot {
    id: u64,
    start: u32,
    len: u32,
    line: u32,
}

/// Eine gelesene STEP-Datei: Bytes, Index (nach Nummer sortiert) und Kopf.
#[derive(Debug)]
pub struct StepFile {
    bytes: Vec<u8>,
    slots: Vec<Slot>,
    by_name: HashMap<String, Vec<u64>>,
    header: StepHeader,
    limits: ReadLimits,
}

/// Liest die Datei, prüft die Größe aber vorher aus den Metadaten.
pub fn read_step_file(path: &Path, limits: &ReadLimits, progress: &Progress) -> Result<StepFile> {
    let io = |e: std::io::Error| StepReadError::Io { msg: e.to_string() };
    let bytes = std::fs::metadata(path).map_err(io)?.len();
    if bytes > limits.max_bytes {
        return Err(StepReadError::TooLarge { bytes });
    }
    let file = std::fs::File::open(path).map_err(io)?;
    let mut data = Vec::with_capacity(usize::try_from(bytes).unwrap_or(0));
    // Wächst die Datei inzwischen, wird nicht mehr als die Grenze gelesen.
    file.take(limits.max_bytes.saturating_add(1))
        .read_to_end(&mut data)
        .map_err(io)?;
    StepFile::parse(data, limits, progress)
}

impl StepFile {
    /// Indexlauf über den ganzen Text.
    pub fn parse(bytes: Vec<u8>, limits: &ReadLimits, progress: &Progress) -> Result<StepFile> {
        let n = bytes.len() as u64;
        if n > limits.max_bytes || n > u64::from(u32::MAX) {
            return Err(StepReadError::TooLarge { bytes: n });
        }
        let kib = kib_of(n);
        progress.set_total(kib);
        let (slots, by_name, header) = Indexer::run(&bytes, limits, progress, kib)?;
        Ok(StepFile {
            bytes,
            slots,
            by_name,
            header,
            limits: *limits,
        })
    }

    pub fn header(&self) -> &StepHeader {
        &self.header
    }

    /// Zahl der Einträge.
    pub fn len(&self) -> usize {
        self.slots.len()
    }

    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }

    /// Dateigröße in KiB, mindestens 1.
    pub fn kib(&self) -> u64 {
        kib_of(self.bytes.len() as u64)
    }

    pub fn limits(&self) -> &ReadLimits {
        &self.limits
    }

    fn slot(&self, id: u64) -> Option<&Slot> {
        self.slots
            .binary_search_by_key(&id, |s| s.id)
            .ok()
            .map(|i| &self.slots[i])
    }

    /// Zeile, in der `#id` steht.
    pub fn line(&self, id: u64) -> Option<usize> {
        self.slot(id).map(|s| s.line as usize)
    }

    /// Zerlegt den Eintrag `#id` jetzt und prüft, dass alle seine Verweise existieren und er
    /// nicht auf sich selbst verweist. Längere Kreise erkennt erst, wer den Verweisen folgt.
    pub fn entity(&self, id: u64) -> Result<Entity> {
        let Some(slot) = self.slot(id) else {
            return Err(StepReadError::NoEntity { id });
        };
        let line = slot.line as usize;
        let start = slot.start as usize;
        let end = start + slot.len as usize;
        let src = self.bytes.get(start..end).unwrap_or_default();
        let mut lex = Lexer::new(src, line);
        let mut names = Vec::new();
        let body = parse_body(&mut lex, true, &mut names)?;
        match lex.token()? {
            Tok::Eof => {}
            Tok::Close => return Err(StepReadError::syntax(line, TOO_MANY_CLOSE)),
            _ => return Err(StepReadError::syntax(line, "Es fehlt ein „;“.")),
        }
        let Some(entity) = body.entity else {
            return Err(StepReadError::syntax(line, "Der Eintrag ist leer."));
        };
        let records: Vec<&[Value]> = match &entity {
            Entity::Simple(r) => vec![&r.args],
            Entity::Complex(parts) => parts.iter().map(|r| r.args.as_slice()).collect(),
        };
        for args in records {
            self.check_refs(args, id, line)?;
        }
        Ok(entity)
    }

    /// Prüft Verweise ohne Rekursion (die Tiefe ist ohnehin auf 64 begrenzt).
    fn check_refs(&self, values: &[Value], id: u64, line: usize) -> Result<()> {
        let mut todo: Vec<&[Value]> = vec![values];
        while let Some(list) = todo.pop() {
            for v in list {
                match v {
                    Value::Ref(target) if *target == id => {
                        return Err(StepReadError::Cycle { line, id });
                    }
                    Value::Ref(target) if self.slot(*target).is_none() => {
                        return Err(StepReadError::MissingRef {
                            line,
                            id,
                            target: *target,
                        });
                    }
                    Value::List(items) | Value::Typed(_, items) => todo.push(items),
                    _ => {}
                }
            }
        }
        Ok(())
    }

    /// Einfache Einträge dieses Typs und zusammengesetzte mit diesem Teil, aufsteigend und
    /// ohne Doppelte.
    pub fn ids_of(&self, name: &str) -> &[u64] {
        self.by_name
            .get(&name.to_ascii_uppercase())
            .map_or(&[], Vec::as_slice)
    }

    /// Fehler mit der Zeile von `#id` und dem Grund `msg` (etwa aus [`Value`]-Zugriffen); eine
    /// unbekannte Nummer ergibt `NoEntity`.
    pub fn fail(&self, id: u64, msg: impl Into<String>) -> StepReadError {
        match self.line(id) {
            Some(line) => StepReadError::syntax(line, msg),
            None => StepReadError::NoEntity { id },
        }
    }
}

/// Weg der gerade verfolgten Verweise (für Folger wie die Geometrie-Auflösung): `enter` vor dem
/// Auflösen von `#id`, `leave` danach. Ein Eintrag, der schon auf dem Weg liegt, ergibt
/// `Cycle`; mehr als `limit` Ebenen (Vorgabe [`MAX_REF_DEPTH`]) ergeben einen Fehler statt
/// eines tiefen Stapels.
#[derive(Clone, Debug)]
pub struct RefPath {
    ids: Vec<u64>,
    limit: usize,
}

impl Default for RefPath {
    fn default() -> Self {
        RefPath::with_limit(MAX_REF_DEPTH)
    }
}

impl RefPath {
    pub fn new() -> RefPath {
        RefPath::default()
    }

    /// Weg mit eigener Tiefengrenze (die Geometrie-Auflösung nimmt 64, weil ihre Rahmen groß
    /// sind).
    pub fn with_limit(limit: usize) -> RefPath {
        RefPath {
            ids: Vec::new(),
            limit,
        }
    }

    pub fn limit(&self) -> usize {
        self.limit
    }

    pub fn enter(&mut self, file: &StepFile, id: u64) -> Result<()> {
        let line = file.line(id).ok_or(StepReadError::NoEntity { id })?;
        if self.ids.contains(&id) {
            return Err(StepReadError::Cycle { line, id });
        }
        if self.ids.len() >= self.limit {
            return Err(StepReadError::syntax(
                line,
                format!(
                    "Die Verweise sind zu tief verschachtelt (höchstens {} Ebenen).",
                    self.limit
                ),
            ));
        }
        self.ids.push(id);
        Ok(())
    }

    pub fn leave(&mut self) {
        self.ids.pop();
    }

    pub fn depth(&self) -> usize {
        self.ids.len()
    }
}

fn kib_of(bytes: u64) -> u64 {
    bytes.div_ceil(1024).max(1)
}

type Index = (Vec<Slot>, HashMap<String, Vec<u64>>, StepHeader);

struct Indexer<'a> {
    lex: Lexer<'a>,
    src: &'a [u8],
    limits: &'a ReadLimits,
    progress: &'a Progress,
    kib: u64,
    slots: Vec<Slot>,
    symbols: HashMap<String, usize>,
    ids: Vec<Vec<u64>>,
    names: Vec<(usize, usize)>,
    upper: String,
}

impl<'a> Indexer<'a> {
    fn run(
        src: &'a [u8],
        limits: &'a ReadLimits,
        progress: &'a Progress,
        kib: u64,
    ) -> Result<Index> {
        let mut ix = Indexer {
            lex: Lexer::new(src, 1).watch(progress, kib),
            src,
            limits,
            progress,
            kib,
            slots: Vec::new(),
            symbols: HashMap::new(),
            ids: Vec::new(),
            names: Vec::new(),
            upper: String::new(),
        };
        ix.lex.skip_bom();
        ix.magic()?;
        let header = ix.header()?;
        let mut sections = 0usize;
        loop {
            let tok = ix.lex.token()?;
            if ix.lex.is_keyword(tok, "DATA") {
                if ix.lex.peek_byte()? == Some(b'(') {
                    parse_args(&mut ix.lex, 0, false, (0, 0))?;
                }
                ix.semicolon()?;
                ix.data()?;
                sections += 1;
            } else if ix.lex.is_keyword(tok, END_MAGIC) {
                if sections == 0 {
                    return Err(StepReadError::syntax(
                        ix.lex.tok_line(),
                        "Es fehlt der Abschnitt „DATA;“.",
                    ));
                }
                ix.semicolon()?;
                break;
            } else if tok == Tok::Eof {
                return Err(ix.lex.truncated());
            } else {
                return Err(StepReadError::syntax(
                    ix.lex.tok_line(),
                    "Hier wird „DATA;“ oder „END-ISO-10303-21;“ erwartet.",
                ));
            }
        }
        ix.finish(header)
    }

    /// `ISO-10303-21;` am Anfang; ein abgeschnittener Anfang ist unvollständig, alles andere
    /// keine STEP-Datei.
    fn magic(&mut self) -> Result<()> {
        let tok = match self.lex.token() {
            Ok(t) => t,
            Err(
                e @ (StepReadError::OpenComment { .. }
                | StepReadError::Truncated { .. }
                | StepReadError::Cancelled),
            ) => {
                return Err(e);
            }
            Err(_) => return Err(StepReadError::NotStep),
        };
        if self.lex.is_keyword(tok, MAGIC) {
            return self.semicolon();
        }
        let at_end = |lex: &mut Lexer| matches!(lex.peek_byte(), Ok(None));
        match tok {
            Tok::Eof
                if self
                    .src
                    .strip_prefix(BOM)
                    .unwrap_or(self.src)
                    .iter()
                    .any(|b| !b.is_ascii_whitespace()) =>
            {
                Err(self.lex.truncated())
            }
            Tok::Keyword { start, end }
                if MAGIC.as_bytes()[..]
                    .get(..end - start)
                    .is_some_and(|p| p.eq_ignore_ascii_case(&self.src[start..end]))
                    && at_end(&mut self.lex) =>
            {
                Err(self.lex.truncated())
            }
            _ => Err(StepReadError::NotStep),
        }
    }

    fn semicolon(&mut self) -> Result<()> {
        match self.lex.token()? {
            Tok::Semicolon => Ok(()),
            Tok::Eof => Err(self.lex.truncated()),
            _ => Err(StepReadError::syntax(
                self.lex.end_line(),
                "Es fehlt ein „;“.",
            )),
        }
    }

    fn header(&mut self) -> Result<StepHeader> {
        let tok = self.lex.token()?;
        if tok == Tok::Eof {
            return Err(self.lex.truncated());
        }
        if !self.lex.is_keyword(tok, "HEADER") {
            return Err(StepReadError::syntax(
                self.lex.tok_line(),
                "Es fehlt „HEADER;“.",
            ));
        }
        self.semicolon()?;
        let mut header = StepHeader::default();
        loop {
            let tok = self.lex.token()?;
            let Tok::Keyword { start, end } = tok else {
                if tok == Tok::Eof {
                    return Err(self.lex.truncated());
                }
                return Err(StepReadError::syntax(
                    self.lex.tok_line(),
                    "Im Kopf wird ein Eintrag wie „FILE_NAME(…);“ erwartet.",
                ));
            };
            if self.lex.is_keyword(tok, "ENDSEC") {
                self.semicolon()?;
                return Ok(header);
            }
            let line = self.lex.tok_line();
            let from = self.lex.pos();
            parse_args(&mut self.lex, 0, false, (start, end))?;
            let to = self.lex.pos();
            self.semicolon()?;
            let name = self.lex.upper(start, end);
            if (name == "FILE_NAME" || name == "FILE_SCHEMA") && to - from <= MAX_HEADER_ENTRY_BYTES
            {
                let mut sub = Lexer::new(&self.src[from..to], line);
                let args = parse_args(&mut sub, 0, true, (0, 0))?;
                let text = |i: usize| {
                    args.get(i)
                        .and_then(|v| v.text().ok())
                        .unwrap_or_default()
                        .to_string()
                };
                if name == "FILE_NAME" {
                    header.file_name = text(0);
                    header.system = text(5);
                } else if let Some(Ok(list)) = args.first().map(Value::list) {
                    header.schemas = list
                        .iter()
                        .filter_map(|v| v.text().ok().map(str::to_string))
                        .collect();
                }
            }
        }
    }

    /// Einträge bis `ENDSEC;`.
    fn data(&mut self) -> Result<()> {
        loop {
            let tok = self.lex.token()?;
            let id = match tok {
                Tok::Ref(id) => id,
                Tok::Eof => return Err(self.lex.truncated()),
                _ if self.lex.is_keyword(tok, "ENDSEC") => return self.semicolon(),
                _ => {
                    return Err(StepReadError::syntax(
                        self.lex.tok_line(),
                        "Hier wird ein Eintrag „#Nummer = …;“ erwartet.",
                    ));
                }
            };
            let line = self.lex.tok_line();
            match self.lex.token()? {
                Tok::Equals => {}
                Tok::Eof => return Err(self.lex.truncated()),
                _ => {
                    return Err(StepReadError::syntax(
                        self.lex.end_line(),
                        format!("Nach „#{id}“ fehlt „=“."),
                    ));
                }
            }
            let start = self.lex.pos();
            parse_body(&mut self.lex, false, &mut self.names)?;
            match self.lex.token()? {
                Tok::Semicolon => {}
                Tok::Eof => return Err(self.lex.truncated()),
                Tok::Close => {
                    return Err(StepReadError::syntax(self.lex.tok_line(), TOO_MANY_CLOSE));
                }
                _ => {
                    return Err(StepReadError::syntax(
                        self.lex.end_line(),
                        "Es fehlt ein „;“.",
                    ));
                }
            }
            let end = self.lex.tok_start();
            if self.slots.len() >= self.limits.max_entities {
                return Err(StepReadError::TooManyEntities);
            }
            // Die Datei ist höchstens u32::MAX Bytes groß (geprüft in `parse`).
            self.slots.push(Slot {
                id,
                start: start as u32,
                len: (end - start) as u32,
                line: u32::try_from(line).unwrap_or(u32::MAX),
            });
            for i in 0..self.names.len() {
                let (s, e) = self.names[i];
                self.upper.clear();
                self.upper
                    .push_str(std::str::from_utf8(&self.src[s..e]).unwrap_or_default());
                self.upper.make_ascii_uppercase();
                let sym = match self.symbols.get(self.upper.as_str()) {
                    Some(&sym) => sym,
                    None => {
                        let sym = self.ids.len();
                        self.symbols.insert(self.upper.clone(), sym);
                        self.ids.push(Vec::new());
                        sym
                    }
                };
                self.ids[sym].push(id);
            }
            if (self.slots.len() as u64).is_multiple_of(CHECK_EVERY) {
                self.lex.checkpoint()?;
            }
        }
    }

    /// Doppelte Nummern (gemeldet wird die früheste Zeile einer Wiederholung), Index sortieren.
    fn finish(mut self, header: StepHeader) -> Result<Index> {
        self.slots.sort_by_key(|s| s.id);
        let dup = self
            .slots
            .windows(2)
            .filter(|w| w[0].id == w[1].id)
            .map(|w| (w[1].line, w[0].line, w[0].id))
            .min();
        if let Some((line, first, id)) = dup {
            return Err(StepReadError::Duplicate {
                line: line as usize,
                id,
                first: first as usize,
            });
        }
        // Keine Wachstumsreserve behalten: 24 Bytes je Eintrag im Index.
        self.slots.shrink_to_fit();
        let mut by_name = HashMap::with_capacity(self.symbols.len());
        for (name, sym) in self.symbols {
            let mut ids = std::mem::take(&mut self.ids[sym]);
            ids.sort_unstable();
            ids.dedup();
            ids.shrink_to_fit();
            by_name.insert(name, ids);
        }
        self.progress
            .advance(self.kib.saturating_sub(self.lex.reported()));
        Ok((self.slots, by_name, header))
    }
}
