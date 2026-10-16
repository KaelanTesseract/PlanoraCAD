//! Eigener SVG-Leser der Tests, unabhängig von `svg.rs` und `out2d.rs` geschrieben.
//!
//! Er zerlegt die Datei in Elemente (streng: Verschachtelung, Anführungszeichen, Maskierung),
//! sammelt die Transformationen der umgebenden Gruppen, zerlegt `d` in `M/L/C/Q/A/Z`-Stücke
//! und rechnet alles auf **Blattkoordinaten** (y nach oben) zurück: erst die Matrizen der
//! Gruppen (Benutzer- → Bildschirmkoordinaten), dann `y_blatt = Höhe − y_bild`. Bögen werden nach
//! den Implementierungshinweisen der SVG-Spezifikation (F.6.5/F.6.6, Endpunkt- zu
//! Mittelpunktsform) umgerechnet. Texte (`<text>`) werden mit der vollständigen Matrix der
//! umgebenden Elemente gelesen (`matrix`, `translate`, `scale`, `rotate`, auch hintereinander);
//! der Leser liefert ihre Lage, die Richtung der Textachse und die Richtung „oben“ der Schrift
//! in Blattkoordinaten.
#![allow(dead_code)]

use std::collections::HashSet;

pub type P = (f64, f64);

/// Ein Pfadstück in Blattkoordinaten.
#[derive(Clone, Debug)]
pub enum Seg {
    Line {
        a: P,
        b: P,
    },
    Quad {
        a: P,
        c: P,
        b: P,
    },
    Cubic {
        a: P,
        c1: P,
        c2: P,
        b: P,
    },
    /// Mittelpunktsform: `P(θ) = c + u·cos θ + v·sin θ`, `θ = th1 + t·dth`, `t ∈ [0, 1]`.
    Arc {
        c: P,
        u: P,
        v: P,
        th1: f64,
        dth: f64,
        a: P,
        b: P,
    },
}

impl Seg {
    /// Der Befehlsbuchstabe, aus dem das Stück stammt.
    pub fn kind(&self) -> char {
        match self {
            Seg::Line { .. } => 'L',
            Seg::Quad { .. } => 'Q',
            Seg::Cubic { .. } => 'C',
            Seg::Arc { .. } => 'A',
        }
    }

    pub fn start(&self) -> P {
        match self {
            Seg::Line { a, .. }
            | Seg::Quad { a, .. }
            | Seg::Cubic { a, .. }
            | Seg::Arc { a, .. } => *a,
        }
    }

    pub fn end(&self) -> P {
        match self {
            Seg::Line { b, .. }
            | Seg::Quad { b, .. }
            | Seg::Cubic { b, .. }
            | Seg::Arc { b, .. } => *b,
        }
    }

    /// Punkt bei `t ∈ [0, 1]`.
    pub fn point_at(&self, t: f64) -> P {
        match self {
            Seg::Line { a, b } => (a.0 + t * (b.0 - a.0), a.1 + t * (b.1 - a.1)),
            Seg::Quad { a, c, b } => {
                let s = 1.0 - t;
                (
                    s * s * a.0 + 2.0 * s * t * c.0 + t * t * b.0,
                    s * s * a.1 + 2.0 * s * t * c.1 + t * t * b.1,
                )
            }
            Seg::Cubic { a, c1, c2, b } => {
                let s = 1.0 - t;
                let (w0, w1, w2, w3) = (s * s * s, 3.0 * s * s * t, 3.0 * s * t * t, t * t * t);
                (
                    w0 * a.0 + w1 * c1.0 + w2 * c2.0 + w3 * b.0,
                    w0 * a.1 + w1 * c1.1 + w2 * c2.1 + w3 * b.1,
                )
            }
            Seg::Arc {
                c, u, v, th1, dth, ..
            } => {
                let th = th1 + t * dth;
                let (s, co) = th.sin_cos();
                (c.0 + u.0 * co + v.0 * s, c.1 + u.1 * co + v.1 * s)
            }
        }
    }

    /// Länge, numerisch mit 4096 Sehnen.
    pub fn length(&self) -> f64 {
        const N: usize = 4096;
        let mut prev = self.point_at(0.0);
        let mut sum = 0.0;
        for i in 1..=N {
            let p = self.point_at(i as f64 / N as f64);
            sum += dist(prev, p);
            prev = p;
        }
        sum
    }

    /// Umschließendes Rechteck `(min, max)`: Kontrollpolygon (konvexe Hülle) bzw. dicht
    /// abgetastete Bogenpunkte plus Spielraum für die Sehnenfehler.
    pub fn bbox(&self) -> (P, P) {
        let pts: Vec<P> = match self {
            Seg::Line { a, b } => vec![*a, *b],
            Seg::Quad { a, c, b } => vec![*a, *c, *b],
            Seg::Cubic { a, c1, c2, b } => vec![*a, *c1, *c2, *b],
            Seg::Arc { .. } => (0..=256).map(|i| self.point_at(i as f64 / 256.0)).collect(),
        };
        let mut lo = (f64::INFINITY, f64::INFINITY);
        let mut hi = (f64::NEG_INFINITY, f64::NEG_INFINITY);
        for q in pts {
            lo = (lo.0.min(q.0), lo.1.min(q.1));
            hi = (hi.0.max(q.0), hi.1.max(q.1));
        }
        let slack = if matches!(self, Seg::Arc { .. }) {
            1e-3 * (hi.0 - lo.0 + hi.1 - lo.1) + 1e-6
        } else {
            0.0
        };
        ((lo.0 - slack, lo.1 - slack), (hi.0 + slack, hi.1 + slack))
    }

    /// Abstand eines Punkts zum Stück (grob abtasten, dann Parameter verfeinern).
    pub fn distance_to(&self, p: P) -> f64 {
        const N: usize = 128;
        let f = |t: f64| dist(self.point_at(t), p);
        let mut best = (f64::INFINITY, 0usize);
        for i in 0..=N {
            let d = f(i as f64 / N as f64);
            if d < best.0 {
                best = (d, i);
            }
        }
        let mut lo = best.1.saturating_sub(1) as f64 / N as f64;
        let mut hi = ((best.1 + 1).min(N)) as f64 / N as f64;
        let g = 0.5 * (5f64.sqrt() - 1.0);
        for _ in 0..80 {
            let m1 = hi - g * (hi - lo);
            let m2 = lo + g * (hi - lo);
            if f(m1) < f(m2) {
                hi = m2;
            } else {
                lo = m1;
            }
        }
        best.0.min(f(0.5 * (lo + hi)))
    }
}

pub fn dist(a: P, b: P) -> f64 {
    ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt()
}

/// Ein gelesener `<path>` mit seinen Darstellungsmerkmalen.
#[derive(Clone, Debug)]
pub struct ReadPath {
    /// Kennung der nächsten umgebenden Gruppe mit `id` (entmaskiert).
    pub group: String,
    pub dashed: bool,
    pub dasharray: Option<String>,
    pub width: f64,
    pub stroke: String,
    pub fill: String,
    pub linecap: String,
    /// Anzahl der `M`-Teilpfade.
    pub subpaths: usize,
    pub segments: Vec<Seg>,
}

impl ReadPath {
    pub fn length(&self) -> f64 {
        self.segments.iter().map(Seg::length).sum()
    }

    pub fn distance_to(&self, p: P) -> f64 {
        let mut best = f64::INFINITY;
        for s in &self.segments {
            // Schneller Ausschluss über das umschließende Rechteck.
            let (lo, hi) = s.bbox();
            let (dx, dy) = (
                (lo.0 - p.0).max(p.0 - hi.0).max(0.0),
                (lo.1 - p.1).max(p.1 - hi.1).max(0.0),
            );
            if (dx * dx + dy * dy).sqrt() > best {
                continue;
            }
            best = best.min(s.distance_to(p));
        }
        best
    }

    /// Die Befehlsbuchstaben aller Stücke in Reihenfolge.
    pub fn kinds(&self) -> String {
        self.segments.iter().map(Seg::kind).collect()
    }
}

/// Ein gelesener `<text>` in Blattkoordinaten (y nach oben).
#[derive(Clone, Debug)]
pub struct ReadText {
    pub group: String,
    /// Ankerpunkt auf dem Blatt.
    pub pos: P,
    /// Richtung der Grundlinie (Einheitsvektor, Schreibrichtung) auf dem Blatt.
    pub axis: P,
    /// Richtung „oben“ der Buchstaben (Einheitsvektor) auf dem Blatt.
    pub up: P,
    /// Rechtshändig: `axis × up > 0` (Schrift ist nicht gespiegelt).
    pub upright: bool,
    /// Winkel der Schreibrichtung in Grad, gegen den Uhrzeigersinn auf dem Blatt, in (−180, 180].
    pub angle_deg: f64,
    pub font_size: f64,
    pub anchor: String,
    pub fill: String,
    pub font_family: String,
    /// Textinhalt, entmaskiert.
    pub text: String,
}

#[derive(Clone, Debug)]
pub struct ReadRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub stroke_width: f64,
    pub fill: String,
}

#[derive(Clone, Debug)]
pub struct ReadSvg {
    pub width_attr: String,
    pub height_attr: String,
    pub viewbox: [f64; 4],
    pub rect: Option<ReadRect>,
    pub paths: Vec<ReadPath>,
    pub texts: Vec<ReadText>,
    /// Kennungen aller Gruppen mit `id` (entmaskiert) in Reihenfolge.
    pub group_ids: Vec<String>,
    /// Zahl der Elemente mit `transform`-Attribut.
    pub transform_count: usize,
}

impl ReadSvg {
    /// Alle Pfade einer Gruppe.
    pub fn paths_of<'a>(&'a self, group: &'a str) -> impl Iterator<Item = &'a ReadPath> {
        self.paths.iter().filter(move |p| p.group == group)
    }

    pub fn texts_of<'a>(&'a self, group: &'a str) -> impl Iterator<Item = &'a ReadText> {
        self.texts.iter().filter(move |t| t.group == group)
    }

    pub fn visible<'a>(&'a self, group: &'a str) -> Vec<&'a ReadPath> {
        self.paths_of(group).filter(|p| !p.dashed).collect()
    }

    pub fn hidden<'a>(&'a self, group: &'a str) -> Vec<&'a ReadPath> {
        self.paths_of(group).filter(|p| p.dashed).collect()
    }
}

// ---------------------------------------------------------------------------------------------
// Elemente
// ---------------------------------------------------------------------------------------------

struct Tag {
    name: String,
    attrs: Vec<(String, String)>,
    open: bool,
    close: bool,
    /// Zeichen hinter dem Tag bis zum nächsten `<` (roh, noch nicht entmaskiert).
    after: String,
}

fn unescape(v: &str) -> String {
    assert!(!v.contains('<'), "rohes '<' im Attributwert: {v:?}");
    let mut out = String::new();
    let mut rest = v;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        let tail = &rest[i..];
        let end = tail
            .find(';')
            .unwrap_or_else(|| panic!("'&' ohne ';' in {v:?}"));
        let ent = &tail[1..end];
        match ent {
            "amp" => out.push('&'),
            "lt" => out.push('<'),
            "gt" => out.push('>'),
            "quot" => out.push('"'),
            "apos" => out.push('\''),
            _ if ent.starts_with('#') => {
                let code = if let Some(h) = ent.strip_prefix("#x") {
                    u32::from_str_radix(h, 16)
                } else {
                    ent[1..].parse::<u32>()
                }
                .unwrap_or_else(|_| panic!("ungültige Zeichenreferenz &{ent};"));
                out.push(char::from_u32(code).expect("ungültiges Zeichen"));
            }
            _ => panic!("unbekannte Entität &{ent}; in {v:?}"),
        }
        rest = &tail[end + 1..];
    }
    out.push_str(rest);
    out
}

fn tags(svg: &str) -> Vec<Tag> {
    let b: Vec<char> = svg.chars().collect();
    let mut i = 0;
    let mut out: Vec<Tag> = Vec::new();
    while i < b.len() {
        if b[i] != '<' {
            assert!(
                !b[i].is_control() || b[i].is_whitespace(),
                "Steuerzeichen im Text"
            );
            if let Some(last) = out.last_mut() {
                last.after.push(b[i]);
            }
            i += 1;
            continue;
        }
        // Tag-Ende suchen, Anführungszeichen beachten.
        let mut j = i + 1;
        let mut quote: Option<char> = None;
        while j < b.len() {
            match quote {
                Some(q) => {
                    if b[j] == q {
                        quote = None;
                    }
                }
                None => {
                    if b[j] == '"' || b[j] == '\'' {
                        quote = Some(b[j]);
                    } else if b[j] == '>' {
                        break;
                    }
                }
            }
            j += 1;
        }
        assert!(j < b.len(), "Tag nicht geschlossen");
        let body: String = b[i + 1..j].iter().collect();
        i = j + 1;
        if body.starts_with('?') || body.starts_with('!') {
            continue;
        }
        let (close, body) = match body.strip_prefix('/') {
            Some(r) => (true, r.to_string()),
            None => (false, body),
        };
        let (empty, body) = match body.strip_suffix('/') {
            Some(r) => (true, r.to_string()),
            None => (false, body),
        };
        let body = body.trim();
        let name_end = body.find(char::is_whitespace).unwrap_or(body.len());
        let name = body[..name_end].to_string();
        let mut rest = body[name_end..].trim_start();
        let mut attrs = Vec::new();
        while !rest.is_empty() {
            let eq = rest
                .find('=')
                .unwrap_or_else(|| panic!("Attribut ohne '=': {rest:?}"));
            let key = rest[..eq].trim().to_string();
            let after = rest[eq + 1..].trim_start();
            let q = after.chars().next().expect("Wert fehlt");
            assert!(q == '"', "Attributwert nicht in \"…\": {after:?}");
            let close_q = after[1..]
                .find('"')
                .unwrap_or_else(|| panic!("Anführungszeichen offen: {after:?}"));
            let val = unescape(&after[1..1 + close_q]);
            assert!(
                !attrs.iter().any(|(k, _): &(String, String)| *k == key),
                "doppeltes Attribut {key}"
            );
            attrs.push((key, val));
            rest = after[close_q + 2..].trim_start();
        }
        out.push(Tag {
            name,
            attrs,
            open: !close,
            close: close || empty,
            after: String::new(),
        });
    }
    out
}

fn attr<'a>(t: &'a Tag, k: &str) -> Option<&'a str> {
    t.attrs
        .iter()
        .find(|(n, _)| n == k)
        .map(|(_, v)| v.as_str())
}

fn fnum(s: &str) -> f64 {
    s.parse::<f64>()
        .unwrap_or_else(|_| panic!("keine Zahl: {s:?}"))
}

// ---------------------------------------------------------------------------------------------
// Matrizen `[a b c d e f]`: x' = a·x + c·y + e, y' = b·x + d·y + f
// ---------------------------------------------------------------------------------------------

type Aff = [f64; 6];
const ID: Aff = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

/// `outer ∘ inner` (erst `inner`, dann `outer`).
fn compose(o: Aff, i: Aff) -> Aff {
    [
        o[0] * i[0] + o[2] * i[1],
        o[1] * i[0] + o[3] * i[1],
        o[0] * i[2] + o[2] * i[3],
        o[1] * i[2] + o[3] * i[3],
        o[0] * i[4] + o[2] * i[5] + o[4],
        o[1] * i[4] + o[3] * i[5] + o[5],
    ]
}

fn apply(m: Aff, p: P) -> P {
    (
        m[0] * p.0 + m[2] * p.1 + m[4],
        m[1] * p.0 + m[3] * p.1 + m[5],
    )
}

fn apply_lin(m: Aff, p: P) -> P {
    (m[0] * p.0 + m[2] * p.1, m[1] * p.0 + m[3] * p.1)
}

/// Liest eine Liste von `matrix(…)`, `translate(…)`, `scale(…)`, `rotate(…)` (Grad); die Liste
/// gilt von links nach rechts verschachtelt: `translate(…) scale(…)` wendet erst `scale` an.
fn parse_transform(s: &str) -> Aff {
    let mut rest = s.trim();
    let mut total = ID;
    while !rest.is_empty() {
        let open = rest
            .find('(')
            .unwrap_or_else(|| panic!("Transformation ohne '(': {rest:?}"));
        let close = rest
            .find(')')
            .unwrap_or_else(|| panic!("Transformation ohne ')': {rest:?}"));
        let name = rest[..open].trim();
        let v: Vec<f64> = rest[open + 1..close]
            .split(|c: char| c == ',' || c.is_whitespace())
            .filter(|t| !t.is_empty())
            .map(fnum)
            .collect();
        let m: Aff = match (name, v.len()) {
            ("matrix", 6) => [v[0], v[1], v[2], v[3], v[4], v[5]],
            ("translate", 1) => [1.0, 0.0, 0.0, 1.0, v[0], 0.0],
            ("translate", 2) => [1.0, 0.0, 0.0, 1.0, v[0], v[1]],
            ("scale", 1) => [v[0], 0.0, 0.0, v[0], 0.0, 0.0],
            ("scale", 2) => [v[0], 0.0, 0.0, v[1], 0.0, 0.0],
            ("rotate", 1) => {
                let (s, c) = v[0].to_radians().sin_cos();
                [c, s, -s, c, 0.0, 0.0]
            }
            _ => panic!("Transformation nicht lesbar: {name}({v:?})"),
        };
        total = compose(total, m);
        rest = rest[close + 1..].trim_start();
    }
    total
}

// ---------------------------------------------------------------------------------------------
// Pfaddaten
// ---------------------------------------------------------------------------------------------

struct Lexer<'a> {
    s: &'a [u8],
    i: usize,
}

impl Lexer<'_> {
    fn skip(&mut self) {
        while self.i < self.s.len()
            && (self.s[self.i].is_ascii_whitespace() || self.s[self.i] == b',')
        {
            self.i += 1;
        }
    }

    fn peek_cmd(&mut self) -> Option<char> {
        self.skip();
        let c = *self.s.get(self.i)? as char;
        c.is_ascii_alphabetic().then_some(c)
    }

    fn at_number(&mut self) -> bool {
        self.skip();
        self.s
            .get(self.i)
            .is_some_and(|c| c.is_ascii_digit() || *c == b'-' || *c == b'+' || *c == b'.')
    }

    fn number(&mut self) -> f64 {
        self.skip();
        let st = self.i;
        while self.i < self.s.len()
            && matches!(
                self.s[self.i],
                b'0'..=b'9' | b'-' | b'+' | b'.' | b'e' | b'E'
            )
        {
            // Vorzeichen nur am Anfang oder nach Exponent.
            if (self.s[self.i] == b'-' || self.s[self.i] == b'+')
                && self.i > st
                && !matches!(self.s[self.i - 1], b'e' | b'E')
            {
                break;
            }
            self.i += 1;
        }
        let t = std::str::from_utf8(&self.s[st..self.i]).unwrap();
        fnum(t)
    }
}

/// Zerlegt `d`; Punkte sind rohe Benutzerkoordinaten. `Arc` wird als Endpunktform übergeben und
/// vom Aufrufer umgerechnet.
enum Raw {
    Line(P, P),
    Quad(P, P, P),
    Cubic(P, P, P, P),
    Arc {
        a: P,
        rx: f64,
        ry: f64,
        phi: f64,
        fa: bool,
        fs: bool,
        b: P,
    },
}

fn parse_d(d: &str) -> (Vec<Raw>, usize) {
    let mut lx = Lexer {
        s: d.as_bytes(),
        i: 0,
    };
    let mut out = Vec::new();
    let mut subpaths = 0;
    let (mut cur, mut start) = ((0.0, 0.0), (0.0, 0.0));
    let mut have_cur = false;
    while let Some(cmd) = lx.peek_cmd() {
        lx.i += 1;
        assert!(
            matches!(cmd, 'M' | 'L' | 'C' | 'Q' | 'A' | 'Z'),
            "Befehl {cmd} wird nicht gelesen"
        );
        if cmd == 'Z' {
            assert!(have_cur, "Z ohne Anfang");
            if dist(cur, start) > 0.0 {
                out.push(Raw::Line(cur, start));
            }
            cur = start;
            continue;
        }
        let mut first = true;
        loop {
            if !first && !lx.at_number() {
                break;
            }
            let eff = if cmd == 'M' && !first { 'L' } else { cmd };
            match eff {
                'M' => {
                    cur = (lx.number(), lx.number());
                    start = cur;
                    have_cur = true;
                    subpaths += 1;
                }
                'L' => {
                    assert!(have_cur, "Zeichnen ohne M");
                    let b = (lx.number(), lx.number());
                    out.push(Raw::Line(cur, b));
                    cur = b;
                }
                'Q' => {
                    assert!(have_cur, "Zeichnen ohne M");
                    let c = (lx.number(), lx.number());
                    let b = (lx.number(), lx.number());
                    out.push(Raw::Quad(cur, c, b));
                    cur = b;
                }
                'C' => {
                    assert!(have_cur, "Zeichnen ohne M");
                    let c1 = (lx.number(), lx.number());
                    let c2 = (lx.number(), lx.number());
                    let b = (lx.number(), lx.number());
                    out.push(Raw::Cubic(cur, c1, c2, b));
                    cur = b;
                }
                'A' => {
                    assert!(have_cur, "Zeichnen ohne M");
                    let (rx, ry, phi) = (lx.number(), lx.number(), lx.number());
                    let (fa, fs) = (lx.number(), lx.number());
                    assert!(fa == 0.0 || fa == 1.0, "large-arc-flag {fa}");
                    assert!(fs == 0.0 || fs == 1.0, "sweep-flag {fs}");
                    let b = (lx.number(), lx.number());
                    out.push(Raw::Arc {
                        a: cur,
                        rx,
                        ry,
                        phi,
                        fa: fa == 1.0,
                        fs: fs == 1.0,
                        b,
                    });
                    cur = b;
                }
                _ => unreachable!(),
            }
            first = false;
        }
    }
    lx.skip();
    assert!(lx.i == lx.s.len(), "Rest in d: {:?}", &d[lx.i..]);
    (out, subpaths)
}

/// Endpunkt- zu Mittelpunktsform nach SVG 1.1, Anhang F.6.5 und F.6.6. Liefert
/// `(cx, cy, rx, ry, θ1, Δθ)` im Benutzerraum, `φ` bleibt Eingabe.
fn arc_center(
    p1: P,
    p2: P,
    rx: f64,
    ry: f64,
    phi_deg: f64,
    fa: bool,
    fs: bool,
) -> (f64, f64, f64, f64, f64, f64) {
    let phi = phi_deg.to_radians();
    let (sp, cp) = phi.sin_cos();
    let (mut rx, mut ry) = (rx.abs(), ry.abs());
    // F.6.5 Schritt 1
    let dx2 = (p1.0 - p2.0) / 2.0;
    let dy2 = (p1.1 - p2.1) / 2.0;
    let x1p = cp * dx2 + sp * dy2;
    let y1p = -sp * dx2 + cp * dy2;
    // F.6.6 Radien vergrößern, wenn zu klein
    let lambda = x1p * x1p / (rx * rx) + y1p * y1p / (ry * ry);
    if lambda > 1.0 {
        let s = lambda.sqrt();
        rx *= s;
        ry *= s;
    }
    // Schritt 2
    let num = rx * rx * ry * ry - rx * rx * y1p * y1p - ry * ry * x1p * x1p;
    let den = rx * rx * y1p * y1p + ry * ry * x1p * x1p;
    let sign = if fa == fs { -1.0 } else { 1.0 };
    let co = sign * (num / den).max(0.0).sqrt();
    let cxp = co * (rx * y1p / ry);
    let cyp = co * (-(ry * x1p) / rx);
    // Schritt 3
    let cx = cp * cxp - sp * cyp + (p1.0 + p2.0) / 2.0;
    let cy = sp * cxp + cp * cyp + (p1.1 + p2.1) / 2.0;
    // Schritt 4
    let ux = (x1p - cxp) / rx;
    let uy = (y1p - cyp) / ry;
    let vx = (-x1p - cxp) / rx;
    let vy = (-y1p - cyp) / ry;
    let ang = |ax: f64, ay: f64, bx: f64, by: f64| (ax * by - ay * bx).atan2(ax * bx + ay * by);
    let th1 = ang(1.0, 0.0, ux, uy);
    let mut dth = ang(ux, uy, vx, vy);
    let tau = std::f64::consts::TAU;
    if !fs && dth > 0.0 {
        dth -= tau;
    } else if fs && dth < 0.0 {
        dth += tau;
    }
    (cx, cy, rx, ry, th1, dth)
}

// ---------------------------------------------------------------------------------------------
// Gesamte Datei
// ---------------------------------------------------------------------------------------------

/// Liest eine Datei des Schreibers. Die Prüfungen der Struktur (Verschachtelung, doppelte
/// Kennungen) sind scharf: ein Verstoß ist ein Absturz des Tests.
pub fn read(svg: &str) -> ReadSvg {
    let tags = tags(svg);
    let root = tags.first().expect("leere Datei");
    assert_eq!(root.name, "svg");
    assert_eq!(attr(root, "xmlns"), Some("http://www.w3.org/2000/svg"));
    let vb: Vec<f64> = attr(root, "viewBox")
        .expect("viewBox fehlt")
        .split_whitespace()
        .map(fnum)
        .collect();
    assert_eq!(vb.len(), 4);
    let height = vb[3];
    // Benutzer → Bild (Gruppenmatrizen), dann Bild → Blatt: y' = Höhe − y.
    let to_sheet: Aff = [1.0, 0.0, 0.0, -1.0, 0.0, height];
    let mut res = ReadSvg {
        width_attr: attr(root, "width").unwrap_or("").to_string(),
        height_attr: attr(root, "height").unwrap_or("").to_string(),
        viewbox: [vb[0], vb[1], vb[2], vb[3]],
        rect: None,
        paths: Vec::new(),
        texts: Vec::new(),
        group_ids: Vec::new(),
        transform_count: 0,
    };
    let mut stack: Vec<(String, Aff, Option<String>)> = Vec::new();
    let mut ids: HashSet<String> = HashSet::new();
    for t in &tags {
        let (acc, group): (Aff, String) = match stack.last() {
            Some((_, m, _)) => (
                *m,
                stack
                    .iter()
                    .rev()
                    .find_map(|(_, _, id)| id.clone())
                    .unwrap_or_default(),
            ),
            None => (ID, String::new()),
        };
        if t.open {
            let own = attr(t, "transform").map_or(ID, parse_transform);
            if attr(t, "transform").is_some() {
                res.transform_count += 1;
            }
            let m = compose(acc, own);
            if let Some(id) = attr(t, "id") {
                assert!(ids.insert(id.to_string()), "doppelte id {id:?}");
                if t.name == "g" {
                    res.group_ids.push(id.to_string());
                }
            }
            match t.name.as_str() {
                "path" => {
                    let d = attr(t, "d").expect("path ohne d");
                    let (raw, subpaths) = parse_d(d);
                    let full = compose(to_sheet, m);
                    let segments = raw
                        .into_iter()
                        .map(|r| convert(r, full))
                        .collect::<Vec<_>>();
                    res.paths.push(ReadPath {
                        group,
                        dashed: attr(t, "stroke-dasharray").is_some(),
                        dasharray: attr(t, "stroke-dasharray").map(str::to_string),
                        width: fnum(attr(t, "stroke-width").unwrap_or("1")),
                        stroke: attr(t, "stroke").unwrap_or("").to_string(),
                        fill: attr(t, "fill").unwrap_or("").to_string(),
                        linecap: attr(t, "stroke-linecap").unwrap_or("").to_string(),
                        subpaths,
                        segments,
                    });
                }
                "text" => {
                    let full = compose(to_sheet, m);
                    let pos = apply(full, (0.0, 0.0));
                    let unit = |v: P| {
                        let l = (v.0 * v.0 + v.1 * v.1).sqrt();
                        (v.0 / l, v.1 / l)
                    };
                    let axis = unit(apply_lin(full, (1.0, 0.0)));
                    // Schrift wächst gegen die Bildschirm-y-Richtung: lokal (0, −1).
                    let up = unit(apply_lin(full, (0.0, -1.0)));
                    res.texts.push(ReadText {
                        group,
                        pos,
                        axis,
                        up,
                        upright: axis.0 * up.1 - axis.1 * up.0 > 0.0,
                        angle_deg: axis.1.atan2(axis.0).to_degrees(),
                        font_size: fnum(attr(t, "font-size").expect("font-size fehlt")),
                        anchor: attr(t, "text-anchor").unwrap_or("start").to_string(),
                        fill: attr(t, "fill").unwrap_or("").to_string(),
                        font_family: attr(t, "font-family").unwrap_or("").to_string(),
                        text: unescape(&t.after),
                    });
                }
                "rect" => {
                    res.rect = Some(ReadRect {
                        x: fnum(attr(t, "x").unwrap_or("0")),
                        y: fnum(attr(t, "y").unwrap_or("0")),
                        width: fnum(attr(t, "width").unwrap()),
                        height: fnum(attr(t, "height").unwrap()),
                        stroke_width: fnum(attr(t, "stroke-width").unwrap_or("1")),
                        fill: attr(t, "fill").unwrap_or("").to_string(),
                    });
                }
                _ => {}
            }
            if !t.close {
                stack.push((t.name.clone(), m, attr(t, "id").map(str::to_string)));
            }
        } else {
            let (name, _, _) = stack.pop().expect("schließendes Tag ohne öffnendes");
            assert_eq!(name, t.name, "Verschachtelung");
        }
    }
    assert!(stack.is_empty(), "nicht geschlossene Elemente");
    res
}

fn convert(r: Raw, m: Aff) -> Seg {
    match r {
        Raw::Line(a, b) => Seg::Line {
            a: apply(m, a),
            b: apply(m, b),
        },
        Raw::Quad(a, c, b) => Seg::Quad {
            a: apply(m, a),
            c: apply(m, c),
            b: apply(m, b),
        },
        Raw::Cubic(a, c1, c2, b) => Seg::Cubic {
            a: apply(m, a),
            c1: apply(m, c1),
            c2: apply(m, c2),
            b: apply(m, b),
        },
        Raw::Arc {
            a,
            rx,
            ry,
            phi,
            fa,
            fs,
            b,
        } => {
            if rx == 0.0 || ry == 0.0 || a == b {
                // Laut Spezifikation eine Gerade bzw. nichts.
                return Seg::Line {
                    a: apply(m, a),
                    b: apply(m, b),
                };
            }
            let (cx, cy, rx, ry, th1, dth) = arc_center(a, b, rx, ry, phi, fa, fs);
            let (sp, cp) = phi.to_radians().sin_cos();
            let u = (rx * cp, rx * sp);
            let v = (-ry * sp, ry * cp);
            Seg::Arc {
                c: apply(m, (cx, cy)),
                u: apply_lin(m, u),
                v: apply_lin(m, v),
                th1,
                dth,
                a: apply(m, a),
                b: apply(m, b),
            }
        }
    }
}
