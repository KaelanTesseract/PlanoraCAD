//! Eigener PDF-Leser für die Tests der PDF-Ausgabe. Nur `std`, ohne Bezug auf die Bibliothek
//! oder andere Prüfwerkzeuge (die App-Tests binden die Datei über `#[path]` ein).
//!
//! Der Leser ist bewusst streng: Kopf `%PDF-1.4`, nur ASCII, jeder Versatz der Querverweistabelle
//! zeigt genau auf `n 0 obj`, die Objekte liegen lückenlos hintereinander, `/Length` stimmt
//! auf das Byte, `/Size`, `startxref` und `%%EOF\n` am Ende stimmen, der Seitenbaum ist
//! vollständig (`/Count` = Zahl der Kinder, `/Parent` zeigt zurück), im Inhaltsstrom stehen nur
//! die erlaubten Anweisungen mit der richtigen Zahl an Operanden. Texte werden mit einer eigenen
//! WinAnsi-Tabelle zurückübersetzt, Breiten kommen aus einer eigenen Tabelle (nach Breite
//! geordnet, Helvetica-AFM).
#![allow(dead_code)]

use std::collections::BTreeMap;

/// Punkt in pt.
pub type P = (f64, f64);

/// Ein PDF-Wert.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Num(f64),
    Name(String),
    Str(Vec<u8>),
    Arr(Vec<Value>),
    Dict(BTreeMap<String, Value>),
    Ref(u32),
    Stream(BTreeMap<String, Value>, Vec<u8>),
}

impl Value {
    pub fn num(&self) -> Option<f64> {
        match self {
            Value::Num(x) => Some(*x),
            _ => None,
        }
    }

    pub fn name(&self) -> Option<&str> {
        match self {
            Value::Name(n) => Some(n),
            _ => None,
        }
    }

    pub fn dict(&self) -> Option<&BTreeMap<String, Value>> {
        match self {
            Value::Dict(d) | Value::Stream(d, _) => Some(d),
            _ => None,
        }
    }
}

/// Ein Pfadstück in pt.
#[derive(Clone, Debug, PartialEq)]
pub enum Seg {
    Line(P, P),
    Cubic(P, P, P, P),
}

impl Seg {
    pub fn start(&self) -> P {
        match self {
            Seg::Line(a, _) | Seg::Cubic(a, _, _, _) => *a,
        }
    }

    pub fn end(&self) -> P {
        match self {
            Seg::Line(_, b) | Seg::Cubic(_, _, _, b) => *b,
        }
    }

    /// Punkt beim Parameter `t ∈ [0, 1]`.
    pub fn point_at(&self, t: f64) -> P {
        match self {
            Seg::Line(a, b) => (a.0 + t * (b.0 - a.0), a.1 + t * (b.1 - a.1)),
            Seg::Cubic(a, c1, c2, b) => {
                let s = 1.0 - t;
                let w = [s * s * s, 3.0 * s * s * t, 3.0 * s * t * t, t * t * t];
                (
                    w[0] * a.0 + w[1] * c1.0 + w[2] * c2.0 + w[3] * b.0,
                    w[0] * a.1 + w[1] * c1.1 + w[2] * c2.1 + w[3] * b.1,
                )
            }
        }
    }
}

/// Ein gezeichnetes Element einer Seite.
#[derive(Clone, Debug, PartialEq)]
pub enum Item {
    Stroke {
        rgb: [f64; 3],
        width: f64,
        dash: Vec<f64>,
        subpaths: Vec<Vec<Seg>>,
    },
    Fill {
        rgb: [f64; 3],
        subpaths: Vec<Vec<Seg>>,
    },
    Text {
        rgb: [f64; 3],
        size: f64,
        matrix: [f64; 6],
        tz: f64,
        text: String,
    },
}

/// Eine Anweisung des Inhaltsstroms.
#[derive(Clone, Debug, PartialEq)]
pub struct Op {
    pub op: String,
    pub args: Vec<Value>,
}

#[derive(Clone, Debug)]
pub struct Page {
    pub media_box: [f64; 4],
    pub ops: Vec<Op>,
    pub items: Vec<Item>,
}

#[derive(Clone, Debug)]
pub struct Pdf {
    pub pages: Vec<Page>,
    pub info: BTreeMap<String, Value>,
    pub objects: BTreeMap<u32, Value>,
}

/// pt → mm.
pub fn mm(pt: f64) -> f64 {
    pt * 25.4 / 72.0
}

/// Text eines UTF-16BE-Strings mit BOM (`<FEFF…>`); sonst Panik.
pub fn utf16_text(v: &Value) -> String {
    let Value::Str(b) = v else {
        panic!("kein String: {v:?}");
    };
    assert!(
        b.len() >= 2 && b[0] == 0xFE && b[1] == 0xFF,
        "ohne BOM: {b:?}"
    );
    assert!(b.len() % 2 == 0, "ungerade Länge");
    let units: Vec<u16> = b[2..]
        .chunks(2)
        .map(|c| (u16::from(c[0]) << 8) | u16::from(c[1]))
        .collect();
    String::from_utf16(&units).expect("gültiges UTF-16")
}

// ---------------------------------------------------------------------------------------------
// Zeichensatz und Breiten (eigene Tabellen)
// ---------------------------------------------------------------------------------------------

/// WinAnsi 0x80..=0x9F (Byte, Zeichen); die fünf freien Plätze fehlen.
const WIN_HIGH: [(u8, char); 27] = [
    (0x80, '€'),
    (0x82, '‚'),
    (0x83, 'ƒ'),
    (0x84, '„'),
    (0x85, '…'),
    (0x86, '†'),
    (0x87, '‡'),
    (0x88, 'ˆ'),
    (0x89, '‰'),
    (0x8A, 'Š'),
    (0x8B, '‹'),
    (0x8C, 'Œ'),
    (0x8E, 'Ž'),
    (0x91, '‘'),
    (0x92, '’'),
    (0x93, '“'),
    (0x94, '”'),
    (0x95, '•'),
    (0x96, '–'),
    (0x97, '—'),
    (0x98, '˜'),
    (0x99, '™'),
    (0x9A, 'š'),
    (0x9B, '›'),
    (0x9C, 'œ'),
    (0x9E, 'ž'),
    (0x9F, 'Ÿ'),
];

/// Zeichen eines WinAnsi-Bytes.
pub fn decode_byte(b: u8) -> Option<char> {
    match b {
        0x20..=0x7E | 0xA0..=0xFF => Some(char::from(b)),
        _ => WIN_HIGH.iter().find(|(x, _)| *x == b).map(|(_, c)| *c),
    }
}

/// Alle Zeichen, die eine PDF-Seite darstellen kann (in Bytereihenfolge).
pub fn all_chars() -> Vec<char> {
    (0x20u8..=0xFF).filter_map(decode_byte).collect()
}

/// Helvetica-Breiten (1/1000 Geviert), nach Breite geordnet.
const WIDTH_GROUPS: [(u16, &str); 27] = [
    (191, "'"),
    (222, "ijl\u{201A}\u{2018}\u{2019}"),
    (260, "|\u{00A6}"),
    (
        278,
        " !,./:;I[\\]ft\u{00A0}\u{00B7}\u{00CC}\u{00CD}\u{00CE}\u{00CF}\u{00EC}\u{00ED}\u{00EE}\u{00EF}",
    ),
    (
        333,
        "()-`r\u{201E}\u{02C6}\u{2039}\u{201C}\u{201D}\u{02DC}\u{203A}\u{00A1}\u{00A8}\u{00AD}\u{00AF}\u{00B2}\u{00B3}\u{00B4}\u{00B8}\u{00B9}",
    ),
    (334, "{}"),
    (350, "\u{2022}"),
    (355, "\""),
    (365, "\u{00BA}"),
    (370, "\u{00AA}"),
    (389, "*"),
    (400, "\u{00B0}"),
    (469, "^"),
    (500, "Jcksvxyz\u{0161}\u{017E}\u{00E7}\u{00FD}\u{00FF}"),
    (537, "\u{00B6}"),
    (
        556,
        "#$0123456789?L_abdeghnopqu\u{20AC}\u{0192}\u{2020}\u{2021}\u{2013}\u{00A2}\u{00A3}\u{00A4}\u{00A5}\u{00A7}\u{00AB}\u{00B5}\u{00BB}\u{00E0}\u{00E1}\u{00E2}\u{00E3}\u{00E4}\u{00E5}\u{00E8}\u{00E9}\u{00EA}\u{00EB}\u{00F0}\u{00F1}\u{00F2}\u{00F3}\u{00F4}\u{00F5}\u{00F6}\u{00F9}\u{00FA}\u{00FB}\u{00FC}\u{00FE}",
    ),
    (584, "+<=>~\u{00AC}\u{00B1}\u{00D7}\u{00F7}"),
    (611, "FTZ\u{017D}\u{00BF}\u{00DF}\u{00F8}"),
    (
        667,
        "&ABEKPSVXY\u{0160}\u{0178}\u{00C0}\u{00C1}\u{00C2}\u{00C3}\u{00C4}\u{00C5}\u{00C8}\u{00C9}\u{00CA}\u{00CB}\u{00DD}\u{00DE}",
    ),
    (
        722,
        "CDHNRUw\u{00C7}\u{00D0}\u{00D1}\u{00D9}\u{00DA}\u{00DB}\u{00DC}",
    ),
    (737, "\u{00A9}\u{00AE}"),
    (778, "GOQ\u{00D2}\u{00D3}\u{00D4}\u{00D5}\u{00D6}\u{00D8}"),
    (833, "Mm"),
    (834, "\u{00BC}\u{00BD}\u{00BE}"),
    (889, "%\u{00E6}"),
    (944, "W\u{0153}"),
    (1000, "\u{2026}\u{2030}\u{0152}\u{2014}\u{2122}\u{00C6}"),
];

/// Helvetica-Breite eines Zeichens (1/1000 Geviert); `@` steht als einziges Zeichen mit 1015
/// außerhalb der Gruppen.
pub fn char_width(c: char) -> u16 {
    if c == '@' {
        return 1015;
    }
    WIDTH_GROUPS
        .iter()
        .find(|(_, s)| s.contains(c))
        .map(|(w, _)| *w)
        .unwrap_or_else(|| panic!("keine Breite für {c:?}"))
}

/// Laufweite eines Texts in pt (Schriftgröße `size` in pt, Stauchung `tz` in Prozent).
pub fn text_width_pt(text: &str, size: f64, tz: f64) -> f64 {
    let units: u32 = text.chars().map(|c| u32::from(char_width(c))).sum();
    f64::from(units) / 1000.0 * size * tz / 100.0
}

// ---------------------------------------------------------------------------------------------
// Zerlegen
// ---------------------------------------------------------------------------------------------

fn is_ws(b: u8) -> bool {
    matches!(b, b' ' | b'\n' | b'\r' | b'\t' | 0x0C | 0)
}

fn is_delim(b: u8) -> bool {
    matches!(
        b,
        b'(' | b')' | b'<' | b'>' | b'[' | b']' | b'{' | b'}' | b'/' | b'%'
    )
}

enum Tok {
    Val(Value),
    Kw(String),
    ArrEnd,
    DictEnd,
}

struct Lex<'a> {
    b: &'a [u8],
    i: usize,
}

impl<'a> Lex<'a> {
    fn new(b: &'a [u8], i: usize) -> Self {
        Lex { b, i }
    }

    /// Überspringt Leerraum. Kommentare (`%`) sind in Objekten und Inhaltsströmen nicht
    /// erlaubt: `%` bleibt stehen und ist dann ein Fehler in [`Lex::token`].
    fn skip_ws(&mut self) {
        while self.i < self.b.len() && is_ws(self.b[self.i]) {
            self.i += 1;
        }
    }

    fn at_end(&mut self) -> bool {
        self.skip_ws();
        self.i >= self.b.len()
    }

    fn regular(&mut self) -> String {
        let s = self.i;
        while self.i < self.b.len() && !is_ws(self.b[self.i]) && !is_delim(self.b[self.i]) {
            self.i += 1;
        }
        String::from_utf8_lossy(&self.b[s..self.i]).into_owned()
    }

    /// Ein Token; `refs`: `n g R` wird als Verweis gelesen (nicht im Inhaltsstrom).
    fn token(&mut self, refs: bool) -> Result<Tok, String> {
        self.skip_ws();
        let Some(&c) = self.b.get(self.i) else {
            return Err("unerwartetes Ende".into());
        };
        match c {
            b'/' => {
                self.i += 1;
                Ok(Tok::Val(Value::Name(self.regular())))
            }
            b'(' => Ok(Tok::Val(Value::Str(self.literal()?))),
            b'<' if self.b.get(self.i + 1) == Some(&b'<') => {
                self.i += 2;
                let mut d = BTreeMap::new();
                loop {
                    match self.token(refs)? {
                        Tok::DictEnd => break,
                        Tok::Val(Value::Name(k)) => {
                            let v = self.value(refs)?;
                            if d.insert(k.clone(), v).is_some() {
                                return Err(format!("Schlüssel doppelt: {k}"));
                            }
                        }
                        _ => return Err("Schlüssel ist kein Name".into()),
                    }
                }
                Ok(Tok::Val(Value::Dict(d)))
            }
            b'<' => {
                self.i += 1;
                let mut hex = Vec::new();
                loop {
                    let Some(&h) = self.b.get(self.i) else {
                        return Err("Hex-String ohne Ende".into());
                    };
                    self.i += 1;
                    match h {
                        b'>' => break,
                        h if is_ws(h) => {}
                        h if h.is_ascii_hexdigit() => hex.push(h),
                        _ => return Err(format!("kein Hex-Zeichen {h}")),
                    }
                }
                if hex.len() % 2 == 1 {
                    return Err("Hex-String mit ungerader Länge".into());
                }
                let v = hex
                    .chunks(2)
                    .map(|p| u8::from_str_radix(std::str::from_utf8(p).unwrap(), 16).unwrap())
                    .collect();
                Ok(Tok::Val(Value::Str(v)))
            }
            b'>' if self.b.get(self.i + 1) == Some(&b'>') => {
                self.i += 2;
                Ok(Tok::DictEnd)
            }
            b'[' => {
                self.i += 1;
                let mut a = Vec::new();
                loop {
                    match self.token(refs)? {
                        Tok::ArrEnd => break,
                        Tok::Val(v) => a.push(v),
                        _ => return Err("unerwartetes Token in Feld".into()),
                    }
                }
                Ok(Tok::Val(Value::Arr(a)))
            }
            b']' => {
                self.i += 1;
                Ok(Tok::ArrEnd)
            }
            b'+' | b'-' | b'.' | b'0'..=b'9' => {
                let t = self.regular();
                let x = parse_num(&t)?;
                if refs && is_uint(&t) {
                    let save = self.i;
                    self.skip_ws();
                    let g = self.regular();
                    self.skip_ws();
                    let r = self.regular();
                    if is_uint(&g) && r == "R" {
                        if g != "0" {
                            return Err("Generation ungleich 0".into());
                        }
                        return Ok(Tok::Val(Value::Ref(t.parse().map_err(|_| "Verweis")?)));
                    }
                    self.i = save;
                }
                Ok(Tok::Val(Value::Num(x)))
            }
            c if is_delim(c) => Err(format!("unerwartetes Zeichen {:?}", char::from(c))),
            _ => Ok(Tok::Kw(self.regular())),
        }
    }

    fn value(&mut self, refs: bool) -> Result<Value, String> {
        match self.token(refs)? {
            Tok::Val(v) => Ok(v),
            Tok::Kw(k) => Err(format!("Wert erwartet, nicht {k}")),
            _ => Err("Wert erwartet".into()),
        }
    }

    fn literal(&mut self) -> Result<Vec<u8>, String> {
        self.i += 1;
        let mut depth = 1;
        let mut out = Vec::new();
        loop {
            let Some(&c) = self.b.get(self.i) else {
                return Err("String ohne Ende".into());
            };
            self.i += 1;
            match c {
                b'\\' => {
                    let Some(&e) = self.b.get(self.i) else {
                        return Err("String ohne Ende".into());
                    };
                    self.i += 1;
                    match e {
                        b'n' => out.push(b'\n'),
                        b'r' => out.push(b'\r'),
                        b't' => out.push(b'\t'),
                        b'b' => out.push(8),
                        b'f' => out.push(12),
                        b'(' | b')' | b'\\' => out.push(e),
                        b'\n' => {}
                        b'0'..=b'7' => {
                            let mut v = u32::from(e - b'0');
                            for _ in 0..2 {
                                match self.b.get(self.i) {
                                    Some(&d) if (b'0'..=b'7').contains(&d) => {
                                        v = v * 8 + u32::from(d - b'0');
                                        self.i += 1;
                                    }
                                    _ => break,
                                }
                            }
                            out.push(u8::try_from(v).map_err(|_| "Oktal zu groß")?);
                        }
                        _ => return Err(format!("unbekannte Maskierung \\{}", char::from(e))),
                    }
                }
                b'(' => {
                    depth += 1;
                    out.push(c);
                }
                b')' => {
                    depth -= 1;
                    if depth == 0 {
                        return Ok(out);
                    }
                    out.push(c);
                }
                _ => out.push(c),
            }
        }
    }
}

fn is_uint(t: &str) -> bool {
    !t.is_empty() && t.bytes().all(|b| b.is_ascii_digit())
}

/// Zahl in der Schreibweise P2 des Schreibers (strenger als PDF erlaubt): optional `-`, ganze
/// Ziffern ohne führende Null (außer „0“), optional `.` mit 1 bis 4 Nachkommastellen ohne
/// nachgestellte Null; kein `+`, kein Exponent, kein „-0“.
fn parse_num(t: &str) -> Result<f64, String> {
    let bad = || format!("Zahl nicht in der Schreibweise P2: {t}");
    let neg = t.starts_with('-');
    let body = if neg { &t[1..] } else { t };
    let (int, frac) = match body.split_once('.') {
        Some((i, f)) => (i, Some(f)),
        None => (body, None),
    };
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    if !digits(int) || (int.len() > 1 && int.starts_with('0')) {
        return Err(bad());
    }
    if let Some(f) = frac
        && (!digits(f) || f.len() > 4 || f.ends_with('0'))
    {
        return Err(bad());
    }
    let v: f64 = body.parse().map_err(|_| bad())?;
    if neg && v == 0.0 {
        return Err(bad());
    }
    Ok(if neg { -v } else { v })
}

// ---------------------------------------------------------------------------------------------
// Dateiaufbau
// ---------------------------------------------------------------------------------------------

fn find_last(hay: &[u8], needle: &[u8]) -> Option<usize> {
    (0..=hay.len().checked_sub(needle.len())?)
        .rev()
        .find(|&i| &hay[i..i + needle.len()] == needle)
}

fn expect(b: &[u8], at: usize, s: &[u8]) -> Result<(), String> {
    if b.len() >= at + s.len() && &b[at..at + s.len()] == s {
        Ok(())
    } else {
        Err(format!(
            "bei {at} erwartet {:?}, gefunden {:?}",
            String::from_utf8_lossy(s),
            String::from_utf8_lossy(&b[at.min(b.len())..(at + s.len()).min(b.len())])
        ))
    }
}

fn digits_at(b: &[u8], at: usize, n: usize) -> Result<u64, String> {
    let s = b.get(at..at + n).ok_or("zu kurz")?;
    if !s.iter().all(u8::is_ascii_digit) {
        return Err(format!("Ziffern erwartet bei {at}"));
    }
    Ok(std::str::from_utf8(s).unwrap().parse().unwrap())
}

fn get<'a>(d: &'a BTreeMap<String, Value>, k: &str) -> Result<&'a Value, String> {
    d.get(k).ok_or_else(|| format!("/{k} fehlt"))
}

fn name_is(d: &BTreeMap<String, Value>, k: &str, v: &str) -> Result<(), String> {
    match get(d, k)? {
        Value::Name(n) if n == v => Ok(()),
        o => Err(format!("/{k} ist {o:?}, erwartet /{v}")),
    }
}

fn reference(v: &Value) -> Result<u32, String> {
    match v {
        Value::Ref(r) => Ok(*r),
        o => Err(format!("Verweis erwartet: {o:?}")),
    }
}

/// Liest und prüft eine PDF-Datei.
pub fn read(bytes: &[u8]) -> Result<Pdf, String> {
    const HEAD: &[u8] = b"%PDF-1.4\n";
    if !bytes.starts_with(HEAD) {
        return Err("Kopf fehlt".into());
    }
    if let Some(i) = bytes.iter().position(|&b| b >= 0x80) {
        return Err(format!("Byte ≥ 0x80 bei {i}"));
    }
    if !bytes.ends_with(b"%%EOF\n") {
        return Err("%%EOF fehlt am Ende".into());
    }
    let sx = find_last(bytes, b"\nstartxref\n").ok_or("startxref fehlt")? + 1;
    let num_start = sx + b"startxref\n".len();
    let num_end = num_start
        + bytes[num_start..]
            .iter()
            .position(|&b| b == b'\n')
            .ok_or("startxref ohne Zeilenende")?;
    if num_end + 1 + b"%%EOF\n".len() != bytes.len() {
        return Err("nach startxref steht mehr als %%EOF".into());
    }
    let xref_at = digits_at(bytes, num_start, num_end - num_start)? as usize;
    expect(bytes, xref_at, b"xref\n0 ")?;
    let mut p = xref_at + b"xref\n0 ".len();
    let nl = p + bytes[p..].iter().position(|&b| b == b'\n').ok_or("xref")?;
    let size = digits_at(bytes, p, nl - p)? as usize;
    p = nl + 1;
    let mut offsets = Vec::with_capacity(size);
    for i in 0..size {
        let e = bytes.get(p..p + 20).ok_or("xref zu kurz")?;
        if i == 0 {
            if e != b"0000000000 65535 f \n" {
                return Err("Eintrag 0 falsch".into());
            }
        } else {
            let off = digits_at(bytes, p, 10)? as usize;
            if &e[10..] != b" 00000 n \n" {
                return Err(format!(
                    "Eintrag {i} falsch: {:?}",
                    String::from_utf8_lossy(e)
                ));
            }
            offsets.push(off);
        }
        p += 20;
    }
    expect(bytes, p, b"trailer\n")?;
    let mut lx = Lex::new(bytes, p + b"trailer\n".len());
    let trailer = lx.value(true)?;
    if lx.i != sx - 1 {
        return Err("zwischen Trailer und startxref steht etwas".into());
    }
    let Value::Dict(trailer) = trailer else {
        return Err("Trailer ist kein Wörterbuch".into());
    };
    if trailer.len() != 3 {
        return Err(format!("Trailer mit fremden Schlüsseln: {trailer:?}"));
    }
    if get(&trailer, "Size")?.num() != Some(size as f64) {
        return Err("/Size falsch".into());
    }

    // Objekte: jeder Versatz zeigt genau auf „n 0 obj“; lückenlos von nach dem Kopf bis xref.
    let mut objects = BTreeMap::new();
    let mut spans = Vec::new();
    for (k, &off) in offsets.iter().enumerate() {
        let n = k as u32 + 1;
        let head = format!("{n} 0 obj\n");
        expect(bytes, off, head.as_bytes()).map_err(|e| format!("Objekt {n}: {e}"))?;
        let mut lx = Lex::new(bytes, off + head.len());
        let v = lx.value(true)?;
        let (v, end) = if bytes[lx.i..].starts_with(b"\nstream\n") {
            let Value::Dict(d) = v else {
                return Err("Strom ohne Wörterbuch".into());
            };
            let len = get(&d, "Length")?.num().ok_or("/Length keine Zahl")?;
            if len < 0.0 || len.fract() != 0.0 {
                return Err("/Length ungültig".into());
            }
            let s = lx.i + b"\nstream\n".len();
            let e = s + len as usize;
            let data = bytes.get(s..e).ok_or("Strom über das Dateiende")?.to_vec();
            expect(bytes, e, b"\nendstream\nendobj\n")
                .map_err(|m| format!("Objekt {n}: /Length passt nicht: {m}"))?;
            (Value::Stream(d, data), e + b"\nendstream\nendobj\n".len())
        } else {
            expect(bytes, lx.i, b"\nendobj\n").map_err(|m| format!("Objekt {n}: {m}"))?;
            (v, lx.i + b"\nendobj\n".len())
        };
        spans.push((off, end));
        objects.insert(n, v);
    }
    spans.sort_unstable();
    let mut at = HEAD.len();
    for &(s, e) in &spans {
        if s != at {
            return Err(format!(
                "Lücke oder Überlappung vor Versatz {s} (erwartet {at})"
            ));
        }
        at = e;
    }
    if at != xref_at {
        return Err(format!("xref steht bei {xref_at}, Objekte enden bei {at}"));
    }

    let obj = |n: u32| objects.get(&n).ok_or(format!("Objekt {n} fehlt"));
    let root = reference(get(&trailer, "Root")?)?;
    let catalog = obj(root)?.dict().ok_or("Katalog")?;
    name_is(catalog, "Type", "Catalog")?;
    let pages_id = reference(get(catalog, "Pages")?)?;
    let pages_node = obj(pages_id)?.dict().ok_or("Seitenbaum")?;
    name_is(pages_node, "Type", "Pages")?;
    let Value::Arr(kids) = get(pages_node, "Kids")? else {
        return Err("/Kids ist kein Feld".into());
    };
    if get(pages_node, "Count")?.num() != Some(kids.len() as f64) {
        return Err("/Count passt nicht zu /Kids".into());
    }
    let mut pages = Vec::new();
    for kid in kids {
        let pd = obj(reference(kid)?)?.dict().ok_or("Seite")?;
        name_is(pd, "Type", "Page")?;
        if reference(get(pd, "Parent")?)? != pages_id {
            return Err("/Parent zeigt nicht auf den Seitenbaum".into());
        }
        let Value::Arr(mb) = get(pd, "MediaBox")? else {
            return Err("/MediaBox".into());
        };
        let mb: Vec<f64> = mb
            .iter()
            .map(|x| x.num().ok_or(format!("/MediaBox mit Nicht-Zahl {x:?}")))
            .collect::<Result<_, _>>()?;
        let media_box: [f64; 4] = mb.try_into().map_err(|_| "/MediaBox mit 4 Zahlen")?;
        let res = get(pd, "Resources")?.dict().ok_or("/Resources")?;
        let fonts = get(res, "Font")?.dict().ok_or("/Font")?;
        let font = obj(reference(get(fonts, "F1")?)?)?
            .dict()
            .ok_or("Schrift")?;
        name_is(font, "Type", "Font")?;
        name_is(font, "Subtype", "Type1")?;
        name_is(font, "BaseFont", "Helvetica")?;
        name_is(font, "Encoding", "WinAnsiEncoding")?;
        let Value::Stream(_, data) = obj(reference(get(pd, "Contents")?)?)? else {
            return Err("/Contents ist kein Strom".into());
        };
        let (ops, items) = content(data)?;
        pages.push(Page {
            media_box,
            ops,
            items,
        });
    }
    let info = obj(reference(get(&trailer, "Info")?)?)?
        .dict()
        .ok_or("Info")?
        .clone();
    Ok(Pdf {
        pages,
        info,
        objects,
    })
}

// ---------------------------------------------------------------------------------------------
// Inhaltsstrom
// ---------------------------------------------------------------------------------------------

/// Erlaubte Anweisungen mit der Zahl ihrer Operanden.
const OPS: &[(&str, usize)] = &[
    ("J", 1),
    ("j", 1),
    ("w", 1),
    ("d", 2),
    ("RG", 3),
    ("rg", 3),
    ("G", 1),
    ("g", 1),
    ("m", 2),
    ("l", 2),
    ("c", 6),
    ("h", 0),
    ("re", 4),
    ("S", 0),
    ("f", 0),
    ("BT", 0),
    ("ET", 0),
    ("Tf", 2),
    ("Tm", 6),
    ("Td", 2),
    ("Tz", 1),
    ("Tj", 1),
];

fn nums(op: &Op) -> Result<Vec<f64>, String> {
    op.args
        .iter()
        .map(|a| a.num().ok_or(format!("{}: Zahl erwartet: {a:?}", op.op)))
        .collect()
}

/// Liest einen Inhaltsstrom für sich (für Prüfungen der Strenge des Lesers).
pub fn read_content(data: &[u8]) -> Result<(Vec<Op>, Vec<Item>), String> {
    content(data)
}

fn content(data: &[u8]) -> Result<(Vec<Op>, Vec<Item>), String> {
    // Genau eine Anweisung je Zeile (P3): jede Zeile sind Operanden und am Ende ein Operator.
    let mut ops = Vec::new();
    for (no, line) in data.split(|&b| b == b'\n').enumerate() {
        if line.contains(&b'\r') {
            return Err(format!("Zeile {}: Wagenrücklauf", no + 1));
        }
        let mut lx = Lex::new(line, 0);
        let mut args = Vec::new();
        let mut on_line = 0;
        while !lx.at_end() {
            match lx.token(false)? {
                Tok::Val(v) => args.push(v),
                Tok::Kw(k) => {
                    let Some(&(_, n)) = OPS.iter().find(|(o, _)| *o == k) else {
                        return Err(format!("unerlaubte Anweisung {k}"));
                    };
                    if args.len() != n {
                        return Err(format!("{k} mit {} statt {n} Operanden", args.len()));
                    }
                    ops.push(Op {
                        op: k,
                        args: std::mem::take(&mut args),
                    });
                    on_line += 1;
                }
                _ => return Err("unerwartetes Token im Inhalt".into()),
            }
        }
        if !args.is_empty() {
            return Err(format!("Zeile {}: Operanden ohne Anweisung", no + 1));
        }
        if on_line != 1 {
            return Err(format!(
                "Zeile {}: {on_line} Anweisungen statt einer",
                no + 1
            ));
        }
    }

    let mut items = Vec::new();
    let (mut stroke_rgb, mut fill_rgb) = ([0.0; 3], [0.0; 3]);
    let mut width = 1.0;
    let mut dash: Vec<f64> = Vec::new();
    let mut path: Vec<Vec<Seg>> = Vec::new();
    let mut cur: Option<P> = None;
    let mut start: P = (0.0, 0.0);
    let mut in_text = false;
    let mut size: Option<f64> = None;
    let mut matrix = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];
    let mut tz = 100.0;
    for op in &ops {
        let path_op = matches!(op.op.as_str(), "m" | "l" | "c" | "h" | "re" | "S" | "f");
        let text_op = matches!(op.op.as_str(), "Tf" | "Tm" | "Td" | "Tj" | "ET");
        if path_op && in_text {
            return Err(format!("{} im Textblock", op.op));
        }
        if text_op && !in_text {
            return Err(format!("{} außerhalb eines Textblocks", op.op));
        }
        match op.op.as_str() {
            "J" | "j" => {
                nums(op)?;
            }
            "w" => width = nums(op)?[0],
            "d" => {
                let Value::Arr(a) = &op.args[0] else {
                    return Err("d ohne Feld".into());
                };
                dash = a
                    .iter()
                    .map(|v| v.num().ok_or("Muster keine Zahl"))
                    .collect::<Result<_, _>>()?;
                if nums(&Op {
                    op: "d".into(),
                    args: vec![op.args[1].clone()],
                })?[0]
                    != 0.0
                {
                    return Err("Musterversatz ungleich 0".into());
                }
            }
            "RG" => {
                let v = nums(op)?;
                stroke_rgb = [v[0], v[1], v[2]];
            }
            "rg" => {
                let v = nums(op)?;
                fill_rgb = [v[0], v[1], v[2]];
            }
            "G" => stroke_rgb = [nums(op)?[0]; 3],
            "g" => fill_rgb = [nums(op)?[0]; 3],
            "m" => {
                let v = nums(op)?;
                path.push(Vec::new());
                cur = Some((v[0], v[1]));
                start = (v[0], v[1]);
            }
            "l" => {
                let v = nums(op)?;
                let a = cur.ok_or("l ohne Anfangspunkt")?;
                let b = (v[0], v[1]);
                path.last_mut().unwrap().push(Seg::Line(a, b));
                cur = Some(b);
            }
            "c" => {
                let v = nums(op)?;
                let a = cur.ok_or("c ohne Anfangspunkt")?;
                let b = (v[4], v[5]);
                path.last_mut()
                    .unwrap()
                    .push(Seg::Cubic(a, (v[0], v[1]), (v[2], v[3]), b));
                cur = Some(b);
            }
            "h" => {
                let a = cur.ok_or("h ohne Pfad")?;
                if a != start {
                    path.last_mut().unwrap().push(Seg::Line(a, start));
                }
                cur = Some(start);
            }
            "re" => {
                let v = nums(op)?;
                let (x, y, w, h) = (v[0], v[1], v[2], v[3]);
                let c = [(x, y), (x + w, y), (x + w, y + h), (x, y + h)];
                path.push((0..4).map(|i| Seg::Line(c[i], c[(i + 1) % 4])).collect());
                cur = Some((x, y));
                start = (x, y);
            }
            "S" | "f" => {
                if path.is_empty() {
                    return Err(format!("{} ohne Pfad", op.op));
                }
                let subpaths = std::mem::take(&mut path);
                items.push(if op.op == "S" {
                    Item::Stroke {
                        rgb: stroke_rgb,
                        width,
                        dash: dash.clone(),
                        subpaths,
                    }
                } else {
                    Item::Fill {
                        rgb: fill_rgb,
                        subpaths,
                    }
                });
                cur = None;
            }
            "BT" => {
                if in_text {
                    return Err("BT im Textblock".into());
                }
                if !path.is_empty() {
                    return Err("offener Pfad vor BT".into());
                }
                in_text = true;
                matrix = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];
            }
            "ET" => in_text = false,
            "Tf" => {
                if op.args[0].name() != Some("F1") {
                    return Err("unbekannte Schrift".into());
                }
                size = Some(
                    nums(&Op {
                        op: "Tf".into(),
                        args: vec![op.args[1].clone()],
                    })?[0],
                );
            }
            "Tm" => {
                let v = nums(op)?;
                matrix = [v[0], v[1], v[2], v[3], v[4], v[5]];
            }
            "Td" => {
                let v = nums(op)?;
                matrix[4] += v[0] * matrix[0] + v[1] * matrix[2];
                matrix[5] += v[0] * matrix[1] + v[1] * matrix[3];
            }
            "Tz" => tz = nums(op)?[0],
            "Tj" => {
                let Value::Str(b) = &op.args[0] else {
                    return Err("Tj ohne String".into());
                };
                let text = b
                    .iter()
                    .map(|&x| decode_byte(x).ok_or(format!("Byte {x:#x} ohne Zeichen")))
                    .collect::<Result<String, _>>()?;
                items.push(Item::Text {
                    rgb: fill_rgb,
                    size: size.ok_or("Tj ohne Tf")?,
                    matrix,
                    tz,
                    text,
                });
            }
            o => return Err(format!("unerlaubte Anweisung {o}")),
        }
    }
    if in_text || !path.is_empty() {
        return Err("Strom endet mitten in Text oder Pfad".into());
    }
    Ok((ops, items))
}
