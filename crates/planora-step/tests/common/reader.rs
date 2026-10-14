//! Eigener STEP-Leser für die Tests (Spec §5.1), unabhängig vom Schreiber: zerlegt eine Datei
//! nach ISO 10303-21 in Kopf und Einträge (auch zusammengesetzte) und prüft die Schreibweise
//! streng: Zahlen, Zeichenketten, Aufzählungen, Verweise, Reihenfolge der Teile.

use std::collections::BTreeMap;

/// Ein Wert in der Parameterliste eines Eintrags.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Int(i64),
    Real(f64),
    /// Zeichenkette, schon entmaskiert.
    Str(String),
    /// Aufzählung ohne Punkte, etwa `T`, `F`, `UNSPECIFIED`, `MILLI`.
    Enum(String),
    Ref(u64),
    List(Vec<Value>),
    /// Getypter Wert, etwa `LENGTH_MEASURE(1.E-6)`.
    Typed(String, Vec<Value>),
    /// `$`
    Unset,
    /// `*`
    Derived,
}

impl Value {
    pub fn real(&self) -> Result<f64, String> {
        match self {
            Value::Real(x) => Ok(*x),
            other => Err(format!("Zahl mit Punkt erwartet, gefunden {other:?}")),
        }
    }

    pub fn int(&self) -> Result<i64, String> {
        match self {
            Value::Int(x) => Ok(*x),
            other => Err(format!("ganze Zahl erwartet, gefunden {other:?}")),
        }
    }

    pub fn reference(&self) -> Result<u64, String> {
        match self {
            Value::Ref(x) => Ok(*x),
            other => Err(format!("Verweis erwartet, gefunden {other:?}")),
        }
    }

    pub fn list(&self) -> Result<&[Value], String> {
        match self {
            Value::List(x) => Ok(x),
            other => Err(format!("Liste erwartet, gefunden {other:?}")),
        }
    }

    pub fn text(&self) -> Result<&str, String> {
        match self {
            Value::Str(x) => Ok(x),
            other => Err(format!("Zeichenkette erwartet, gefunden {other:?}")),
        }
    }

    pub fn enumeration(&self) -> Result<&str, String> {
        match self {
            Value::Enum(x) => Ok(x),
            other => Err(format!("Aufzählung erwartet, gefunden {other:?}")),
        }
    }

    /// `.T.` oder `.F.`.
    pub fn logical(&self) -> Result<bool, String> {
        match self.enumeration()? {
            "T" => Ok(true),
            "F" => Ok(false),
            other => Err(format!(".T. oder .F. erwartet, gefunden .{other}.")),
        }
    }

    pub fn refs(&self) -> Result<Vec<u64>, String> {
        self.list()?.iter().map(Value::reference).collect()
    }

    pub fn reals(&self) -> Result<Vec<f64>, String> {
        self.list()?.iter().map(Value::real).collect()
    }

    pub fn ints(&self) -> Result<Vec<i64>, String> {
        self.list()?.iter().map(Value::int).collect()
    }
}

/// Name und Parameter (ein einfacher Eintrag oder ein Teil eines zusammengesetzten).
#[derive(Clone, Debug, PartialEq)]
pub struct Record {
    pub name: String,
    pub args: Vec<Value>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Entity {
    Simple(Record),
    /// Zusammengesetzter Eintrag `( A(..) B(..) … )`, Teile in aufsteigender Reihenfolge.
    Complex(Vec<Record>),
}

impl Entity {
    /// Teil `name` eines zusammengesetzten Eintrags.
    pub fn part(&self, name: &str) -> Option<&[Value]> {
        match self {
            Entity::Complex(parts) => parts
                .iter()
                .find(|r| r.name == name)
                .map(|r| r.args.as_slice()),
            Entity::Simple(_) => None,
        }
    }

    pub fn names(&self) -> Vec<&str> {
        match self {
            Entity::Simple(r) => vec![r.name.as_str()],
            Entity::Complex(parts) => parts.iter().map(|r| r.name.as_str()).collect(),
        }
    }
}

/// Gelesene Datei.
#[derive(Clone, Debug)]
pub struct StepFile {
    /// `FILE_DESCRIPTION`, `FILE_NAME`, `FILE_SCHEMA` in dieser Reihenfolge.
    pub header: Vec<Record>,
    pub entities: BTreeMap<u64, Entity>,
}

impl StepFile {
    pub fn get(&self, id: u64) -> Result<&Entity, String> {
        self.entities.get(&id).ok_or_else(|| format!("#{id} fehlt"))
    }

    /// Parameter des einfachen Eintrags `#id`, der `name` heißen muss.
    pub fn simple(&self, id: u64, name: &str) -> Result<&[Value], String> {
        match self.get(id)? {
            Entity::Simple(r) if r.name == name => Ok(&r.args),
            other => Err(format!(
                "#{id}: {name} erwartet, gefunden {:?}",
                other.names()
            )),
        }
    }

    /// Name des einfachen Eintrags `#id` (zusammengesetzte: `None`).
    pub fn name_of(&self, id: u64) -> Result<Option<&str>, String> {
        Ok(match self.get(id)? {
            Entity::Simple(r) => Some(r.name.as_str()),
            Entity::Complex(_) => None,
        })
    }

    /// Nummern aller einfachen Einträge mit diesem Namen, aufsteigend.
    pub fn ids_of(&self, name: &str) -> Vec<u64> {
        self.entities
            .iter()
            .filter(|(_, e)| matches!(e, Entity::Simple(r) if r.name == name))
            .map(|(id, _)| *id)
            .collect()
    }

    pub fn count(&self, name: &str) -> usize {
        self.ids_of(name).len()
    }
}

#[derive(Clone, Debug, PartialEq)]
enum Token {
    Keyword(String),
    Id(u64),
    Equals,
    Open,
    Close,
    Comma,
    Semicolon,
    Str(String),
    Enum(String),
    Int(i64),
    Real(f64),
    Dollar,
    Star,
}

fn is_name_start(b: u8) -> bool {
    b.is_ascii_uppercase() || b == b'_'
}

fn is_name_char(b: u8) -> bool {
    b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_' || b == b'-'
}

/// Zerlegt den Text in Zeichen der Austauschdatei. Nur ASCII ist erlaubt; Leerraum (Leerzeichen,
/// Zeilenende, Tabulator) nur zwischen Zeichen.
fn tokens(text: &str) -> Result<Vec<Token>, String> {
    let b = text.as_bytes();
    if let Some(i) = b.iter().position(|c| !c.is_ascii()) {
        return Err(format!("Zeichen außerhalb von ASCII an Stelle {i}"));
    }
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        match c {
            b' ' | b'\n' | b'\r' | b'\t' => i += 1,
            b'=' => {
                out.push(Token::Equals);
                i += 1;
            }
            b'(' => {
                out.push(Token::Open);
                i += 1;
            }
            b')' => {
                out.push(Token::Close);
                i += 1;
            }
            b',' => {
                out.push(Token::Comma);
                i += 1;
            }
            b';' => {
                out.push(Token::Semicolon);
                i += 1;
            }
            b'$' => {
                out.push(Token::Dollar);
                i += 1;
            }
            b'*' => {
                out.push(Token::Star);
                i += 1;
            }
            b'#' => {
                let start = i + 1;
                let mut j = start;
                while j < b.len() && b[j].is_ascii_digit() {
                    j += 1;
                }
                let n: u64 = text[start..j]
                    .parse()
                    .map_err(|_| format!("Nummer nach # an Stelle {i}"))?;
                out.push(Token::Id(n));
                i = j;
            }
            b'\'' => {
                let (s, next) = string(b, i)?;
                out.push(Token::Str(s));
                i = next;
            }
            b'.' => {
                let start = i + 1;
                let mut j = start;
                while j < b.len()
                    && (b[j].is_ascii_uppercase() || b[j].is_ascii_digit() || b[j] == b'_')
                {
                    j += 1;
                }
                if j == start || j >= b.len() || b[j] != b'.' || !is_name_start(b[start]) {
                    return Err(format!("Aufzählung an Stelle {i} ungültig"));
                }
                out.push(Token::Enum(text[start..j].to_string()));
                i = j + 1;
            }
            b'+' | b'-' | b'0'..=b'9' => {
                let (t, next) = number(text, i)?;
                out.push(t);
                i = next;
            }
            _ if is_name_start(c) => {
                let start = i;
                while i < b.len() && is_name_char(b[i]) {
                    i += 1;
                }
                out.push(Token::Keyword(text[start..i].to_string()));
            }
            _ => {
                return Err(format!(
                    "unerwartetes Zeichen {:?} an Stelle {i}",
                    char::from(c)
                ));
            }
        }
    }
    Ok(out)
}

/// Zahl nach ISO 10303-21: `[Vorzeichen] Ziffern [. Ziffern*] [E [Vorzeichen] Ziffern]`; mit
/// Punkt REAL, ohne Punkt INTEGER; ein Exponent ohne Punkt, ein kleines `e` oder ein Punkt ohne
/// Ziffern davor sind Fehler.
fn number(text: &str, start: usize) -> Result<(Token, usize), String> {
    let b = text.as_bytes();
    let mut i = start;
    if b[i] == b'+' || b[i] == b'-' {
        i += 1;
    }
    let digits_start = i;
    while i < b.len() && b[i].is_ascii_digit() {
        i += 1;
    }
    if i == digits_start {
        return Err(format!("Zahl ohne Ziffern an Stelle {start}"));
    }
    let mut is_real = false;
    if i < b.len() && b[i] == b'.' {
        is_real = true;
        i += 1;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
    }
    if i < b.len() && b[i] == b'e' {
        return Err(format!("kleines e in der Zahl an Stelle {start}"));
    }
    if i < b.len() && b[i] == b'E' {
        if !is_real {
            return Err(format!("Exponent ohne Punkt an Stelle {start}"));
        }
        i += 1;
        if i < b.len() && (b[i] == b'+' || b[i] == b'-') {
            i += 1;
        }
        let e0 = i;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
        if i == e0 {
            return Err(format!("Exponent ohne Ziffern an Stelle {start}"));
        }
    }
    let s = &text[start..i];
    let t = if is_real {
        let x: f64 = s
            .replace('E', "e")
            .parse()
            .map_err(|_| format!("Zahl {s} unlesbar"))?;
        if !x.is_finite() {
            return Err(format!("Zahl {s} nicht endlich"));
        }
        Token::Real(x)
    } else {
        Token::Int(s.parse().map_err(|_| format!("Zahl {s} unlesbar"))?)
    };
    Ok((t, i))
}

/// Hexziffern als Zeichen (je `width` Ziffern eines).
fn hex_chars(hex: &[u8], width: usize) -> Result<String, String> {
    if hex.is_empty() || !hex.len().is_multiple_of(width) {
        return Err("Länge eines \\X2\\- oder \\X4\\-Abschnitts falsch".into());
    }
    let mut out = String::new();
    for chunk in hex.chunks(width) {
        if !chunk
            .iter()
            .all(|c| c.is_ascii_digit() || (b'A'..=b'F').contains(c))
        {
            return Err("ungültige Hexziffer".into());
        }
        let s = std::str::from_utf8(chunk).map_err(|e| e.to_string())?;
        let code = u32::from_str_radix(s, 16).map_err(|e| e.to_string())?;
        out.push(char::from_u32(code).ok_or_else(|| format!("kein Zeichen: {code:X}"))?);
    }
    Ok(out)
}

/// Zeichenkette ab dem öffnenden Hochkomma; liefert den entmaskierten Text und die Stelle
/// hinter dem schließenden Hochkomma. Nur druckbares ASCII; `''`, `\\`, `\X2\…\X0\` und
/// `\X4\…\X0\` werden entmaskiert, jeder andere Rückstrich ist ein Fehler.
fn string(b: &[u8], start: usize) -> Result<(String, usize), String> {
    let mut out = String::new();
    let mut i = start + 1;
    loop {
        let Some(&c) = b.get(i) else {
            return Err(format!("Zeichenkette ab Stelle {start} nicht geschlossen"));
        };
        if !(b' '..=b'~').contains(&c) {
            return Err(format!(
                "Steuerzeichen in der Zeichenkette ab Stelle {start}"
            ));
        }
        match c {
            b'\'' if b.get(i + 1) == Some(&b'\'') => {
                out.push('\'');
                i += 2;
            }
            b'\'' => return Ok((out, i + 1)),
            b'\\' => {
                let rest = &b[i..];
                if rest.starts_with(b"\\\\") {
                    out.push('\\');
                    i += 2;
                } else if rest.starts_with(b"\\X2\\") || rest.starts_with(b"\\X4\\") {
                    let width = if rest[2] == b'2' { 4 } else { 8 };
                    let body = &rest[4..];
                    let end = body
                        .windows(4)
                        .position(|w| w == b"\\X0\\")
                        .ok_or("\\X0\\ fehlt")?;
                    out.push_str(&hex_chars(&body[..end], width)?);
                    i += 4 + end + 4;
                } else {
                    return Err(format!("unbekannte Maskierung ab Stelle {i}"));
                }
            }
            _ => {
                out.push(char::from(c));
                i += 1;
            }
        }
    }
}

struct Parser {
    t: Vec<Token>,
    i: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Token> {
        self.t.get(self.i)
    }

    fn bump(&mut self) -> Result<Token, String> {
        let t = self.t.get(self.i).cloned().ok_or("Datei endet zu früh")?;
        self.i += 1;
        Ok(t)
    }

    fn expect(&mut self, want: &Token) -> Result<(), String> {
        let got = self.bump()?;
        if got == *want {
            Ok(())
        } else {
            Err(format!("{want:?} erwartet, gefunden {got:?}"))
        }
    }

    fn keyword(&mut self, word: &str) -> Result<(), String> {
        self.expect(&Token::Keyword(word.to_string()))
    }

    /// `( wert, wert, … )` (auch leer).
    fn values(&mut self) -> Result<Vec<Value>, String> {
        self.expect(&Token::Open)?;
        let mut out = Vec::new();
        if self.peek() == Some(&Token::Close) {
            self.i += 1;
            return Ok(out);
        }
        loop {
            out.push(self.value()?);
            match self.bump()? {
                Token::Comma => {}
                Token::Close => return Ok(out),
                other => return Err(format!(", oder ) erwartet, gefunden {other:?}")),
            }
        }
    }

    fn value(&mut self) -> Result<Value, String> {
        Ok(match self.peek().ok_or("Wert fehlt")?.clone() {
            Token::Open => Value::List(self.values()?),
            Token::Keyword(name) => {
                self.i += 1;
                Value::Typed(name, self.values()?)
            }
            other => {
                self.i += 1;
                match other {
                    Token::Int(x) => Value::Int(x),
                    Token::Real(x) => Value::Real(x),
                    Token::Str(s) => Value::Str(s),
                    Token::Enum(s) => Value::Enum(s),
                    Token::Id(n) => Value::Ref(n),
                    Token::Dollar => Value::Unset,
                    Token::Star => Value::Derived,
                    t => return Err(format!("Wert erwartet, gefunden {t:?}")),
                }
            }
        })
    }

    fn record(&mut self) -> Result<Record, String> {
        match self.bump()? {
            Token::Keyword(name) => Ok(Record {
                name,
                args: self.values()?,
            }),
            other => Err(format!("Name erwartet, gefunden {other:?}")),
        }
    }
}

/// Alle Verweise in einem Wert.
fn collect_refs(v: &Value, out: &mut Vec<u64>) {
    match v {
        Value::Ref(n) => out.push(*n),
        Value::List(items) | Value::Typed(_, items) => {
            for x in items {
                collect_refs(x, out);
            }
        }
        _ => {}
    }
}

/// Liest eine ganze Datei: Kopf mit genau `FILE_DESCRIPTION`, `FILE_NAME`, `FILE_SCHEMA`,
/// Datenabschnitt, Ende; danach nur Leerraum. Prüft: jede Nummer einmal, jeder Verweis zeigt
/// auf einen vorhandenen Eintrag, Teile zusammengesetzter Einträge aufsteigend und verschieden.
pub fn parse(text: &str) -> Result<StepFile, String> {
    let mut p = Parser {
        t: tokens(text)?,
        i: 0,
    };
    p.keyword("ISO-10303-21")?;
    p.expect(&Token::Semicolon)?;
    p.keyword("HEADER")?;
    p.expect(&Token::Semicolon)?;
    let mut header = Vec::new();
    for want in ["FILE_DESCRIPTION", "FILE_NAME", "FILE_SCHEMA"] {
        let r = p.record()?;
        if r.name != want {
            return Err(format!("Kopf: {want} erwartet, gefunden {}", r.name));
        }
        p.expect(&Token::Semicolon)?;
        header.push(r);
    }
    p.keyword("ENDSEC")?;
    p.expect(&Token::Semicolon)?;
    p.keyword("DATA")?;
    p.expect(&Token::Semicolon)?;
    let mut entities = BTreeMap::new();
    loop {
        match p.bump()? {
            Token::Keyword(k) if k == "ENDSEC" => {
                p.expect(&Token::Semicolon)?;
                break;
            }
            Token::Id(id) => {
                p.expect(&Token::Equals)?;
                let entity = if p.peek() == Some(&Token::Open) {
                    p.i += 1;
                    let mut parts = Vec::new();
                    while p.peek() != Some(&Token::Close) {
                        parts.push(p.record()?);
                    }
                    p.i += 1;
                    if parts.len() < 2 {
                        return Err(format!(
                            "#{id}: zusammengesetzt mit weniger als zwei Teilen"
                        ));
                    }
                    if parts.windows(2).any(|w| w[0].name >= w[1].name) {
                        return Err(format!("#{id}: Teile nicht aufsteigend geordnet"));
                    }
                    Entity::Complex(parts)
                } else {
                    Entity::Simple(p.record()?)
                };
                p.expect(&Token::Semicolon)?;
                if entities.insert(id, entity).is_some() {
                    return Err(format!("#{id} doppelt"));
                }
            }
            other => return Err(format!("Eintrag erwartet, gefunden {other:?}")),
        }
    }
    p.keyword("END-ISO-10303-21")?;
    p.expect(&Token::Semicolon)?;
    if p.i != p.t.len() {
        return Err("Text nach dem Dateiende".into());
    }
    for (id, e) in &entities {
        let records: Vec<&Record> = match e {
            Entity::Simple(r) => vec![r],
            Entity::Complex(parts) => parts.iter().collect(),
        };
        let mut found = Vec::new();
        for r in records {
            for v in &r.args {
                collect_refs(v, &mut found);
            }
        }
        if let Some(missing) = found.iter().find(|n| !entities.contains_key(n)) {
            return Err(format!("#{id} verweist auf #{missing}, den es nicht gibt"));
        }
    }
    Ok(StepFile { header, entities })
}
