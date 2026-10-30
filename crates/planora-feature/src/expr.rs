//! Formelbaustein für benannte Parameter: Zerlegen, Auswerten mit Einheitenprüfung,
//! Namensregeln, Umbenennen und Auswertung der Parametertabelle. Rein, ohne Oberfläche und
//! ohne Abhängigkeit auf [`crate::Document`].
//!
//! Grammatik (absteigender Parser):
//! `sum := prod (('+'|'-') prod)*`, `prod := unary (('*'|'/') unary)*`,
//! `unary := '-' unary | power`, `power := atom ('^' unary)?` (rechtsassoziativ),
//! `atom := zahl | name | name '(' args ')' | '(' sum ')'`.
//!
//! **P4 Einheiten:** Ausdrücke rechnen mit Exponenten von Länge und Winkel ([`Dim`]). Bei
//! `+`, `-`, `min`, `max` passt eine reine Zahl zu allem; Länge mit Winkel ist ein Fehler
//! („Mischung“), andere ungleiche Einheiten (mm + mm²) nennen beide Einheiten. `*` und `/` addieren bzw. ziehen Exponenten ab;
//! `^` mit Einheit nur mit ganzzahligem, einheitenlosem Exponenten; `sqrt` nur bei geraden
//! Exponenten. `sin`, `cos`, `tan` nehmen einen Winkel oder eine Zahl, beides in Grad, und
//! ergeben eine Zahl. Ein Feld nimmt seine Einheit oder eine reine Zahl an ([`fit_unit`]).
//!
//! **P5 Komma:** Ein Komma **zwischen zwei Ziffern** ist Dezimalzeichen („1,5“), sonst trennt
//! es Werte. Werte trennt man deshalb mit „;“ oder „, “: `min(1; 5)`, `min(a, b)`;
//! `min(1,5)` ist **ein** Wert 1,5. Eine Zahl hat höchstens ein Dezimalzeichen; ein weiteres
//! Komma zwischen Ziffern („1,5,3“) ist ein unerwartetes Zeichen.
//!
//! **P6 Namen:** Nachschlagen und Eindeutigkeit ohne Rücksicht auf Groß-/Kleinschreibung
//! (`to_lowercase`); Umbenennen ersetzt ganze Namen-Token (derselbe Zerleger wie beim
//! Parsen, keine Textsuche), der übrige Text bleibt zeichengleich, auch in nicht zerlegbaren
//! Formeln. Namen höchstens 64 Zeichen, Kommentare höchstens 200 Zeichen.

use planora_i18n::tr;
use std::collections::{HashMap, HashSet, VecDeque};
use std::f64::consts::{FRAC_1_SQRT_2, PI};

/// Höchstzahl der Parameter eines Dokuments.
pub const MAX_PARAMETERS: usize = 500;
/// Höchstlänge einer Formel in Zeichen (nicht Bytes).
pub const MAX_FORMULA_CHARS: usize = 200;
/// Höchste Verschachtelung: Klammern, Funktionen, unäre Minuszeichen, Potenzen.
pub const MAX_FORMULA_DEPTH: usize = 64;
/// Höchstlänge eines Parameternamens in Zeichen.
pub const MAX_PARAM_NAME_CHARS: usize = 64;
/// Höchstlänge eines Kommentars in Zeichen.
pub const MAX_COMMENT_CHARS: usize = 200;

fn empty() -> String {
    tr!("feature-expr-empty")
}
fn too_long() -> String {
    tr!("feature-expr-too-long")
}
fn too_deep() -> String {
    tr!("feature-expr-too-deep")
}
fn end() -> String {
    tr!("feature-expr-end")
}
fn no_close() -> String {
    tr!("feature-expr-no-close")
}
fn div_zero() -> String {
    tr!("feature-expr-div-zero")
}
fn sqrt_neg() -> String {
    tr!("feature-expr-sqrt-neg")
}
fn sqrt_unit() -> String {
    tr!("feature-expr-sqrt-unit")
}
fn pow_unit() -> String {
    tr!("feature-expr-pow-unit")
}
fn pow_exp_unit() -> String {
    tr!("feature-expr-pow-exp-unit")
}
fn mix() -> String {
    tr!("feature-expr-mix")
}
fn units_differ() -> String {
    tr!("feature-expr-units-differ")
}
fn value_too_long() -> String {
    tr!("feature-expr-value-too-long")
}
fn not_finite() -> String {
    tr!("feature-expr-not-finite")
}
fn name_empty() -> String {
    tr!("feature-expr-name-empty")
}
fn name_form() -> String {
    tr!("feature-expr-name-form")
}
fn name_long() -> String {
    tr!("feature-expr-name-long")
}
fn too_many() -> String {
    tr!("feature-expr-too-many")
}

/// Funktionen und Konstanten; als Parametername gesperrt (klein verglichen).
const RESERVED: [&str; 9] = [
    "sqrt", "sin", "cos", "tan", "abs", "min", "max", "round", "pi",
];

/// Art eines Parameters.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ParamUnit {
    /// Länge in mm.
    Length,
    /// Winkel in Grad.
    Angle,
    /// Zahl ohne Einheit.
    Number,
}

impl ParamUnit {
    /// Alle Arten in der Reihenfolge der Auswahlliste.
    pub const ALL: [ParamUnit; 3] = [ParamUnit::Length, ParamUnit::Angle, ParamUnit::Number];

    /// Anzeigename der Art.
    pub fn label(self) -> String {
        match self {
            ParamUnit::Length => tr!("feature-unit-length"),
            ParamUnit::Angle => tr!("blend-field-angle"),
            ParamUnit::Number => tr!("feature-unit-number"),
        }
    }

    /// Einheitenzeichen hinter einem Wert.
    pub fn suffix(self) -> &'static str {
        match self {
            ParamUnit::Length => "mm",
            ParamUnit::Angle => "°",
            ParamUnit::Number => "",
        }
    }
}

/// Einheit eines Zwischenergebnisses: Exponenten von Länge und Winkel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Dim {
    pub length: i32,
    pub angle: i32,
}

impl Dim {
    /// Reine Zahl.
    pub const NUMBER: Dim = Dim {
        length: 0,
        angle: 0,
    };

    /// Einheit eines Parameters der Art `unit`.
    pub const fn of(unit: ParamUnit) -> Dim {
        match unit {
            ParamUnit::Length => Dim {
                length: 1,
                angle: 0,
            },
            ParamUnit::Angle => Dim {
                length: 0,
                angle: 1,
            },
            ParamUnit::Number => Dim::NUMBER,
        }
    }
}

/// Fehler einer Formel oder eines Namens mit fertiger deutscher Meldung.
#[derive(Clone, Debug, PartialEq, thiserror::Error)]
#[error("{0}")]
pub struct ExprError(pub String);

fn fail<T>(message: impl Into<String>) -> Result<T, ExprError> {
    Err(ExprError(message.into()))
}

/// Anfang und Ende der Meldung „Die Parameter … hängen im Kreis voneinander ab“ in der
/// aktuellen Sprache (um sie von anderen Meldungen zu unterscheiden, ohne Wortlaut im Code).
pub(crate) fn cycle_affixes() -> (String, String) {
    let marker = "\u{1}";
    let text = tr!("feature-param-cycle", list = marker);
    match text.split_once(marker) {
        Some((a, b)) => (a.to_string(), b.to_string()),
        None => (text, String::new()),
    }
}

/// Zahlentext einer Formel-Ausgabe (Dezimalkomma, [`number_text`], [`short_number`]) mit dem
/// Dezimaltrenner der aktuellen Sprache, nur für Meldungen.
pub(crate) fn display_number(text: &str) -> String {
    planora_i18n::localize_decimal(&text.replace(',', "."))
}

// ---------------------------------------------------------------------------------------
// Zerleger (gemeinsam für Parser, Umbenennen und Verweissuche)
// ---------------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Tok {
    Num,
    Name,
    Plus,
    Minus,
    Star,
    Slash,
    Caret,
    Open,
    Close,
    Sep,
    /// Zeichen, das in keiner Formel vorkommen darf.
    Other,
}

#[derive(Clone, Copy, Debug)]
struct Token {
    kind: Tok,
    /// Byte-Bereich im Text.
    start: usize,
    end: usize,
    /// Stelle des ersten Zeichens, 1-basiert in Zeichen.
    pos: usize,
    first: char,
}

fn is_name_start(c: char) -> bool {
    c.is_alphabetic() || c == '_'
}

fn is_name_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Zerlegt den Text vollständig; unbekannte Zeichen werden zu [`Tok::Other`].
fn scan(text: &str) -> Vec<Token> {
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let n = chars.len();
    let digit = |k: usize| k < n && chars[k].1.is_ascii_digit();
    let byte = |k: usize| if k < n { chars[k].0 } else { text.len() };
    let mut out = Vec::new();
    let mut i = 0;
    while i < n {
        let c = chars[i].1;
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        let (kind, j) = if c.is_ascii_digit() {
            let mut j = i + 1;
            while digit(j) {
                j += 1;
            }
            if j < n && matches!(chars[j].1, '.' | ',') && digit(j + 1) {
                j += 2;
                while digit(j) {
                    j += 1;
                }
            }
            (Tok::Num, j)
        } else if c == '.' && digit(i + 1) {
            let mut j = i + 2;
            while digit(j) {
                j += 1;
            }
            (Tok::Num, j)
        } else if is_name_start(c) {
            let mut j = i + 1;
            while j < n && is_name_char(chars[j].1) {
                j += 1;
            }
            (Tok::Name, j)
        } else {
            let kind = match c {
                '+' => Tok::Plus,
                '-' => Tok::Minus,
                '*' => Tok::Star,
                '/' => Tok::Slash,
                '^' => Tok::Caret,
                '(' => Tok::Open,
                ')' => Tok::Close,
                ';' => Tok::Sep,
                // Komma zwischen zwei Ziffern gehört zu keiner Zahl mehr (zweites
                // Dezimalzeichen) und trennt auch nicht: unerwartet.
                ',' if i > 0 && digit(i - 1) && digit(i + 1) => Tok::Other,
                ',' => Tok::Sep,
                _ => Tok::Other,
            };
            (kind, i + 1)
        };
        out.push(Token {
            kind,
            start: byte(i),
            end: byte(j),
            pos: i + 1,
            first: c,
        });
        i = j;
    }
    out
}

fn unexpected(t: &Token) -> ExprError {
    ExprError(tr!(
        "feature-expr-unexpected",
        char = t.first.to_string(),
        pos = t.pos.to_string()
    ))
}

// ---------------------------------------------------------------------------------------
// Baum und Parser
// ---------------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Op {
    Add,
    Sub,
    Mul,
    Div,
    Pow,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Func {
    Sqrt,
    Sin,
    Cos,
    Tan,
    Abs,
    Min,
    Max,
    Round,
}

impl Func {
    fn from_lower(name: &str) -> Option<Func> {
        Some(match name {
            "sqrt" => Func::Sqrt,
            "sin" => Func::Sin,
            "cos" => Func::Cos,
            "tan" => Func::Tan,
            "abs" => Func::Abs,
            "min" => Func::Min,
            "max" => Func::Max,
            "round" => Func::Round,
            _ => return None,
        })
    }

    fn name(self) -> &'static str {
        match self {
            Func::Sqrt => "sqrt",
            Func::Sin => "sin",
            Func::Cos => "cos",
            Func::Tan => "tan",
            Func::Abs => "abs",
            Func::Min => "min",
            Func::Max => "max",
            Func::Round => "round",
        }
    }

    fn many(self) -> bool {
        matches!(self, Func::Min | Func::Max)
    }
}

#[derive(Clone, Debug, PartialEq)]
enum Node {
    Num(f64),
    Pi,
    Name(String),
    Neg(Box<Node>),
    Bin(Op, Box<Node>, Box<Node>),
    Call(Func, Vec<Node>),
}

/// Zerlegte Formel.
#[derive(Clone, Debug, PartialEq)]
pub struct Expr {
    root: Node,
}

impl Expr {
    /// Parameternamen der Formel wie geschrieben, in Reihenfolge des ersten Auftretens,
    /// ohne Doppelte (klein verglichen); ohne Funktionen und `pi`.
    pub fn names(&self) -> Vec<String> {
        let mut out = Vec::new();
        let mut seen = HashSet::new();
        let mut stack = vec![&self.root];
        while let Some(node) = stack.pop() {
            match node {
                Node::Num(_) | Node::Pi => {}
                Node::Name(name) => {
                    if seen.insert(name.to_lowercase()) {
                        out.push(name.clone());
                    }
                }
                Node::Neg(inner) => stack.push(inner),
                Node::Bin(_, l, r) => {
                    stack.push(r);
                    stack.push(l);
                }
                Node::Call(_, args) => stack.extend(args.iter().rev()),
            }
        }
        out
    }
}

struct Parser<'t> {
    text: &'t str,
    toks: Vec<Token>,
    at: usize,
    depth: usize,
}

impl Parser<'_> {
    fn peek(&self) -> Option<Tok> {
        self.toks.get(self.at).map(|t| t.kind)
    }

    fn next(&mut self) -> Option<Token> {
        let t = self.toks.get(self.at).copied();
        if t.is_some() {
            self.at += 1;
        }
        t
    }

    fn enter(&mut self) -> Result<(), ExprError> {
        self.depth += 1;
        if self.depth > MAX_FORMULA_DEPTH {
            return fail(too_deep());
        }
        Ok(())
    }

    fn leave(&mut self) {
        self.depth -= 1;
    }

    fn sum(&mut self) -> Result<Node, ExprError> {
        let mut left = self.prod()?;
        loop {
            let op = match self.peek() {
                Some(Tok::Plus) => Op::Add,
                Some(Tok::Minus) => Op::Sub,
                _ => return Ok(left),
            };
            self.at += 1;
            let right = self.prod()?;
            left = Node::Bin(op, Box::new(left), Box::new(right));
        }
    }

    fn prod(&mut self) -> Result<Node, ExprError> {
        let mut left = self.unary()?;
        loop {
            let op = match self.peek() {
                Some(Tok::Star) => Op::Mul,
                Some(Tok::Slash) => Op::Div,
                _ => return Ok(left),
            };
            self.at += 1;
            let right = self.unary()?;
            left = Node::Bin(op, Box::new(left), Box::new(right));
        }
    }

    fn unary(&mut self) -> Result<Node, ExprError> {
        if self.peek() == Some(Tok::Minus) {
            self.at += 1;
            self.enter()?;
            let inner = self.unary()?;
            self.leave();
            return Ok(Node::Neg(Box::new(inner)));
        }
        self.power()
    }

    fn power(&mut self) -> Result<Node, ExprError> {
        let base = self.atom()?;
        if self.peek() != Some(Tok::Caret) {
            return Ok(base);
        }
        self.at += 1;
        self.enter()?;
        let exp = self.unary()?;
        self.leave();
        Ok(Node::Bin(Op::Pow, Box::new(base), Box::new(exp)))
    }

    fn atom(&mut self) -> Result<Node, ExprError> {
        let Some(t) = self.next() else {
            return fail(end());
        };
        let text = &self.text[t.start..t.end];
        match t.kind {
            Tok::Num => {
                let v: f64 = text.replace(',', ".").parse().map_err(|_| unexpected(&t))?;
                if !v.is_finite() {
                    return fail(not_finite());
                }
                Ok(Node::Num(v))
            }
            Tok::Name if self.peek() == Some(Tok::Open) => self.call(text),
            Tok::Name if text.to_lowercase() == "pi" => Ok(Node::Pi),
            Tok::Name => Ok(Node::Name(text.to_string())),
            Tok::Open => {
                self.enter()?;
                let inner = self.sum()?;
                self.close()?;
                self.leave();
                Ok(inner)
            }
            _ => Err(unexpected(&t)),
        }
    }

    /// Erwartet „)“.
    fn close(&mut self) -> Result<(), ExprError> {
        match self.next() {
            Some(t) if t.kind == Tok::Close => Ok(()),
            Some(t) => Err(unexpected(&t)),
            None => fail(no_close()),
        }
    }

    fn call(&mut self, name: &str) -> Result<Node, ExprError> {
        let Some(func) = Func::from_lower(&name.to_lowercase()) else {
            return fail(tr!("feature-expr-no-function", name = name.to_string()));
        };
        self.at += 1; // „(“
        self.enter()?;
        let mut args = Vec::new();
        if self.peek() == Some(Tok::Close) {
            self.at += 1;
        } else {
            loop {
                args.push(self.sum()?);
                match self.next() {
                    Some(t) if t.kind == Tok::Sep => {}
                    Some(t) if t.kind == Tok::Close => break,
                    Some(t) => return Err(unexpected(&t)),
                    None => return fail(no_close()),
                }
            }
        }
        self.leave();
        if func.many() {
            if args.len() < 2 {
                return fail(tr!(
                    "feature-expr-needs-two",
                    name = func.name().to_string()
                ));
            }
        } else if args.len() != 1 {
            return fail(tr!(
                "feature-expr-needs-one",
                name = func.name().to_string()
            ));
        }
        Ok(Node::Call(func, args))
    }
}

/// Zerlegt eine Formel; prüft Länge, Leere, Schreibweise, Funktionsnamen, Wertezahl und
/// Verschachtelungstiefe (unbekannte Parameternamen erst beim Auswerten).
pub fn parse(text: &str) -> Result<Expr, ExprError> {
    if text.chars().count() > MAX_FORMULA_CHARS {
        return fail(too_long());
    }
    if text.trim().is_empty() {
        return fail(empty());
    }
    let mut p = Parser {
        text,
        toks: scan(text),
        at: 0,
        depth: 0,
    };
    let root = p.sum()?;
    if let Some(t) = p.next() {
        return Err(unexpected(&t));
    }
    Ok(Expr { root })
}

// ---------------------------------------------------------------------------------------
// Auswertung
// ---------------------------------------------------------------------------------------

type Lookup<'a> = &'a dyn Fn(&str) -> Option<(f64, ParamUnit)>;

/// Name einer Einheit in Meldungen über unpassende Einheiten.
fn dim_name(d: Dim) -> String {
    match (d.length, d.angle) {
        (1, 0) => tr!("feature-unit-length"),
        (2, 0) => tr!("feature-unit-area"),
        (3, 0) => tr!("feature-unit-volume"),
        (0, 1) => tr!("blend-field-angle"),
        _ => tr!("feature-unit-other"),
    }
}

/// Gemeinsame Einheit für `+`, `-`, `min`, `max`: eine reine Zahl passt zu allem.
fn join(a: Dim, b: Dim) -> Result<Dim, ExprError> {
    if a == b || b == Dim::NUMBER {
        return Ok(a);
    }
    if a == Dim::NUMBER {
        return Ok(b);
    }
    let length = Dim::of(ParamUnit::Length);
    let angle = Dim::of(ParamUnit::Angle);
    if (a == length && b == angle) || (a == angle && b == length) {
        return fail(mix());
    }
    let (na, nb) = (dim_name(a), dim_name(b));
    if na == nb {
        return fail(units_differ());
    }
    fail(tr!(
        "feature-expr-units-mismatch",
        na = na.to_string(),
        nb = nb.to_string()
    ))
}

fn dim_op(a: Dim, b: Dim, f: fn(i32, i32) -> Option<i32>) -> Result<Dim, ExprError> {
    match (f(a.length, b.length), f(a.angle, b.angle)) {
        (Some(length), Some(angle)) => Ok(Dim { length, angle }),
        _ => fail(not_finite()),
    }
}

/// Winkel in Grad zerlegt in Viertel (0–3) und Rest in [0°, 90°).
fn quadrant(deg: f64) -> (u8, f64) {
    let mut a = deg.rem_euclid(360.0);
    if a >= 360.0 {
        a = 0.0;
    }
    let q = (a / 90.0).floor();
    (q as u8, a - 90.0 * q)
}

/// Sinus für den Rest eines Viertels; glatte Winkel genau.
fn sin_part(r: f64) -> f64 {
    if r == 0.0 {
        0.0
    } else if r == 30.0 {
        0.5
    } else if r == 45.0 {
        FRAC_1_SQRT_2
    } else if r == 60.0 {
        3f64.sqrt() / 2.0
    } else {
        r.to_radians().sin()
    }
}

fn cos_part(r: f64) -> f64 {
    if r == 0.0 {
        1.0
    } else if r == 30.0 {
        3f64.sqrt() / 2.0
    } else if r == 45.0 {
        FRAC_1_SQRT_2
    } else if r == 60.0 {
        0.5
    } else {
        r.to_radians().cos()
    }
}

/// Ohne „-0“.
fn no_neg_zero(v: f64) -> f64 {
    if v == 0.0 { 0.0 } else { v }
}

fn sin_deg(deg: f64) -> f64 {
    let (q, r) = quadrant(deg);
    no_neg_zero(match q {
        0 => sin_part(r),
        1 => cos_part(r),
        2 => -sin_part(r),
        _ => -cos_part(r),
    })
}

fn cos_deg(deg: f64) -> f64 {
    let (q, r) = quadrant(deg);
    no_neg_zero(match q {
        0 => cos_part(r),
        1 => -sin_part(r),
        2 => -cos_part(r),
        _ => sin_part(r),
    })
}

/// Wertet einen Knoten aus. Jedes Zwischenergebnis muss endlich sein (kein stilles
/// `min(10^400; 1) = 1`) und „-0“ wird zu 0. Zweierknoten und Funktionsaufrufe stehen in
/// eigenen Funktionen, damit der Rahmen dieser rekursiven Funktion klein bleibt.
fn eval_node(node: &Node, lookup: Lookup<'_>) -> Result<(f64, Dim), ExprError> {
    let (v, d) = match node {
        Node::Num(v) => (*v, Dim::NUMBER),
        Node::Pi => (PI, Dim::NUMBER),
        Node::Name(name) => eval_name(name, lookup)?,
        Node::Neg(inner) => {
            let (v, d) = eval_node(inner, lookup)?;
            (-v, d)
        }
        Node::Bin(op, l, r) => eval_bin(*op, l, r, lookup)?,
        Node::Call(func, args) => eval_call(*func, args, lookup)?,
    };
    if !v.is_finite() {
        return fail(not_finite());
    }
    Ok((no_neg_zero(v), d))
}

#[inline(never)]
fn eval_name(name: &str, lookup: Lookup<'_>) -> Result<(f64, Dim), ExprError> {
    match lookup(name) {
        Some((v, unit)) => Ok((v, Dim::of(unit))),
        None => fail(tr!("feature-expr-no-name", name = name.to_string())),
    }
}

#[inline(never)]
fn eval_bin(op: Op, l: &Node, r: &Node, lookup: Lookup<'_>) -> Result<(f64, Dim), ExprError> {
    let (a, da) = eval_node(l, lookup)?;
    let (b, db) = eval_node(r, lookup)?;
    Ok(match op {
        Op::Add => (a + b, join(da, db)?),
        Op::Sub => (a - b, join(da, db)?),
        Op::Mul => (a * b, dim_op(da, db, i32::checked_add)?),
        Op::Div => {
            if b == 0.0 {
                return fail(div_zero());
            }
            (a / b, dim_op(da, db, i32::checked_sub)?)
        }
        Op::Pow => {
            if db != Dim::NUMBER {
                return fail(pow_exp_unit());
            }
            let d = if da == Dim::NUMBER {
                Dim::NUMBER
            } else {
                if b.fract() != 0.0 {
                    return fail(pow_unit());
                }
                if b.abs() > f64::from(i32::MAX) {
                    return fail(not_finite());
                }
                let n = Dim {
                    length: b as i32,
                    angle: b as i32,
                };
                dim_op(da, n, i32::checked_mul)?
            };
            (a.powf(b), d)
        }
    })
}

#[inline(never)]
fn eval_call(func: Func, args: &[Node], lookup: Lookup<'_>) -> Result<(f64, Dim), ExprError> {
    let mut vals = Vec::with_capacity(args.len());
    for arg in args {
        vals.push(eval_node(arg, lookup)?);
    }
    let (x, dx) = vals[0];
    let angle_ok = dx == Dim::NUMBER || dx == Dim::of(ParamUnit::Angle);
    let trig = || -> Result<(), ExprError> {
        if angle_ok {
            Ok(())
        } else {
            fail(tr!(
                "feature-expr-needs-angle",
                name = func.name().to_string()
            ))
        }
    };
    Ok(match func {
        Func::Sqrt => {
            if dx.length % 2 != 0 || dx.angle % 2 != 0 {
                return fail(sqrt_unit());
            }
            if x < 0.0 {
                return fail(sqrt_neg());
            }
            let d = Dim {
                length: dx.length / 2,
                angle: dx.angle / 2,
            };
            (x.sqrt(), d)
        }
        Func::Sin => {
            trig()?;
            (sin_deg(x), Dim::NUMBER)
        }
        Func::Cos => {
            trig()?;
            (cos_deg(x), Dim::NUMBER)
        }
        Func::Tan => {
            trig()?;
            let c = cos_deg(x);
            if c == 0.0 {
                return fail(not_finite());
            }
            (sin_deg(x) / c, Dim::NUMBER)
        }
        Func::Abs => (x.abs(), dx),
        Func::Round => (x.round(), dx),
        Func::Min | Func::Max => {
            let mut acc = (x, dx);
            for &(v, d) in &vals[1..] {
                let m = if func == Func::Min {
                    acc.0.min(v)
                } else {
                    acc.0.max(v)
                };
                acc = (m, join(acc.1, d)?);
            }
            acc
        }
    })
}

/// Wertet eine zerlegte Formel aus. `lookup` bekommt Namen wie geschrieben und vergleicht
/// selbst ohne Rücksicht auf Groß-/Kleinschreibung.
pub fn evaluate(
    e: &Expr,
    lookup: &dyn Fn(&str) -> Option<(f64, ParamUnit)>,
) -> Result<(f64, Dim), ExprError> {
    eval_node(&e.root, lookup)
}

fn dim_text(d: Dim) -> String {
    if d == Dim::of(ParamUnit::Length) {
        tr!("feature-unit-a-length")
    } else if d == Dim::of(ParamUnit::Angle) {
        tr!("feature-unit-an-angle-acc")
    } else if d == Dim::NUMBER {
        tr!("feature-unit-a-number")
    } else {
        tr!("feature-unit-other")
    }
}

/// Prüft, ob ein Ergebnis in ein Feld der Art `want` passt (seine Einheit oder eine Zahl).
pub fn fit_unit(value: f64, dim: Dim, want: ParamUnit) -> Result<f64, ExprError> {
    if !value.is_finite() {
        return fail(not_finite());
    }
    if dim == Dim::of(want) || dim == Dim::NUMBER {
        return Ok(no_neg_zero(value));
    }
    let soll = match want {
        ParamUnit::Length => tr!("feature-unit-a-length"),
        ParamUnit::Angle => tr!("feature-unit-an-angle"),
        ParamUnit::Number => tr!("feature-unit-a-number"),
    };
    fail(tr!(
        "feature-expr-expected",
        expected = soll.to_string(),
        actual = (dim_text(dim)).to_string()
    ))
}

/// Zerlegen, Auswerten und Einheit prüfen in einem Schritt.
pub fn eval_text(
    text: &str,
    want: ParamUnit,
    lookup: &dyn Fn(&str) -> Option<(f64, ParamUnit)>,
) -> Result<f64, ExprError> {
    let e = parse(text)?;
    let (v, d) = evaluate(&e, lookup)?;
    fit_unit(v, d, want)
}

// ---------------------------------------------------------------------------------------
// Zahlen als Text
// ---------------------------------------------------------------------------------------

/// Reine Zahl (Komma oder Punkt, auch „1e3“), sonst `None`; wie
/// `planora_app::sketch_view::parse_number`.
pub fn plain_number(text: &str) -> Option<f64> {
    text.trim()
        .replace(',', ".")
        .parse::<f64>()
        .ok()
        .filter(|v| v.is_finite())
}

/// Zahl als Text mit Komma, verlustfrei: `plain_number(&number_text(v)) == Some(v)`
/// („-0“ wird „0“). Kann bei sehr großen oder sehr kleinen Beträgen (ab etwa 1e200 bzw.
/// unter etwa 1e-190) länger als [`MAX_FORMULA_CHARS`] werden; wer den Text als Formel
/// speichert, nimmt [`value_text`].
pub fn number_text(v: f64) -> String {
    no_neg_zero(v).to_string().replace('.', ",")
}

/// Zahltext für eine Formel (zum Beispiel „Durch den Wert ersetzen“): wie [`number_text`],
/// aber nur, wenn der Wert endlich ist und der Text höchstens [`MAX_FORMULA_CHARS`] Zeichen
/// hat; dann ergibt `parse` + Auswerten bitgenau denselben Wert. Sonst eine Meldung, und
/// der Aufrufer lehnt ab.
pub fn value_text(v: f64) -> Result<String, ExprError> {
    if !v.is_finite() {
        return fail(value_too_long());
    }
    let text = number_text(v);
    if text.chars().count() > MAX_FORMULA_CHARS {
        return fail(value_too_long());
    }
    Ok(text)
}

/// Zahl für die Anzeige: höchstens zwei Nachkommastellen, ohne Nullen am Ende, mit Komma,
/// nie „-0“.
pub fn short_number(v: f64) -> String {
    let s = format!("{v:.2}");
    let s = if s.contains('.') {
        s.trim_end_matches('0').trim_end_matches('.')
    } else {
        &s
    };
    if s == "-0" {
        "0".to_string()
    } else {
        s.replace('.', ",")
    }
}

// ---------------------------------------------------------------------------------------
// Namen
// ---------------------------------------------------------------------------------------

/// Prüft einen Parameternamen (Form, Länge, nicht Funktion oder Konstante).
pub fn check_name(name: &str) -> Result<(), ExprError> {
    if name.is_empty() {
        return fail(name_empty());
    }
    if name.chars().count() > MAX_PARAM_NAME_CHARS {
        return fail(name_long());
    }
    let mut chars = name.chars();
    let form_ok = chars.next().is_some_and(is_name_start) && chars.all(is_name_char);
    if !form_ok {
        return fail(name_form());
    }
    if RESERVED.contains(&name.to_lowercase().as_str()) {
        return fail(tr!("feature-expr-reserved", name = name.to_string()));
    }
    Ok(())
}

/// Gleicher Name ohne Rücksicht auf Groß-/Kleinschreibung.
pub fn same_name(a: &str, b: &str) -> bool {
    a.to_lowercase() == b.to_lowercase()
}

/// Ersetzt jedes Namen-Token `old` (klein verglichen) durch `new`; der übrige Text bleibt
/// zeichengleich, auch wenn sich die Formel nicht zerlegen lässt.
pub fn rename_in(text: &str, old: &str, new: &str) -> String {
    let old = old.to_lowercase();
    let mut out = String::with_capacity(text.len());
    let mut last = 0;
    for t in scan(text) {
        if t.kind == Tok::Name && text[t.start..t.end].to_lowercase() == old {
            out.push_str(&text[last..t.start]);
            out.push_str(new);
            last = t.end;
        }
    }
    out.push_str(&text[last..]);
    out
}

/// Nennt der Text den Namen als ganzes Token (klein verglichen)?
pub fn references(text: &str, name: &str) -> bool {
    let name = name.to_lowercase();
    scan(text)
        .iter()
        .any(|t| t.kind == Tok::Name && text[t.start..t.end].to_lowercase() == name)
}

// ---------------------------------------------------------------------------------------
// Parametertabelle
// ---------------------------------------------------------------------------------------

/// Eine Zeile der Parametertabelle zum Auswerten.
pub struct ParamDef<'a> {
    pub name: &'a str,
    pub formula: &'a str,
    pub unit: ParamUnit,
}

/// Fehler der Tabelle: Zeile (0-basiert) und Meldung.
#[derive(Clone, Debug, PartialEq)]
pub struct TableError {
    pub index: usize,
    pub message: String,
}

/// Wertet die ganze Tabelle aus (Werte in Tabellenreihenfolge). Prüft Anzahl, Namen,
/// Eindeutigkeit, Formeln, Verweise, Selbstbezug, Kreise (ohne Rekursion) und Einheiten.
pub fn evaluate_table(defs: &[ParamDef<'_>]) -> Result<Vec<f64>, TableError> {
    let at = |index: usize, message: String| TableError { index, message };
    if defs.len() > MAX_PARAMETERS {
        return Err(at(MAX_PARAMETERS, too_many()));
    }
    let n = defs.len();
    let mut by_name: HashMap<String, usize> = HashMap::with_capacity(n);
    for (i, d) in defs.iter().enumerate() {
        check_name(d.name).map_err(|e| at(i, e.0))?;
        if by_name.insert(d.name.to_lowercase(), i).is_some() {
            return Err(at(i, tr!("feature-name-exists", name = d.name.to_string())));
        }
    }
    let mut exprs = Vec::with_capacity(n);
    let mut deps: Vec<Vec<usize>> = Vec::with_capacity(n);
    for (i, d) in defs.iter().enumerate() {
        let e = parse(d.formula).map_err(|e| at(i, e.0))?;
        let mut mine = Vec::new();
        for name in e.names() {
            match by_name.get(&name.to_lowercase()) {
                Some(&j) if j == i => {
                    return Err(at(i, tr!("feature-param-self", name = d.name.to_string())));
                }
                Some(&j) => mine.push(j),
                None => return Err(at(i, tr!("feature-expr-no-name", name = name.to_string()))),
            }
        }
        deps.push(mine);
        exprs.push(e);
    }

    // Kahn: Reihenfolge, in der jede Zeile nach ihren Abhängigkeiten kommt.
    let mut waiting: Vec<usize> = deps.iter().map(Vec::len).collect();
    let mut users: Vec<Vec<usize>> = vec![Vec::new(); n];
    for (i, mine) in deps.iter().enumerate() {
        for &j in mine {
            users[j].push(i);
        }
    }
    let mut queue: VecDeque<usize> = (0..n).filter(|&i| waiting[i] == 0).collect();
    let mut order = Vec::with_capacity(n);
    while let Some(i) = queue.pop_front() {
        order.push(i);
        for &u in &users[i] {
            waiting[u] -= 1;
            if waiting[u] == 0 {
                queue.push_back(u);
            }
        }
    }
    if order.len() < n {
        // Jede übrige Zeile hängt von einer übrigen ab: Weg ab der ersten übrigen Zeile
        // folgen, bis sich eine Zeile wiederholt; das Stück dazwischen ist ein Kreis.
        let open = |i: usize| waiting[i] > 0;
        let mut seen: Vec<Option<usize>> = vec![None; n];
        let mut path = Vec::new();
        let mut cur = (0..n).find(|&i| open(i)).expect("übrige Zeile");
        let start = loop {
            if let Some(p) = seen[cur] {
                break p;
            }
            seen[cur] = Some(path.len());
            path.push(cur);
            cur = *deps[cur]
                .iter()
                .find(|&&j| open(j))
                .expect("übrige Abhängigkeit");
        };
        let mut ring = path[start..].to_vec();
        ring.sort_unstable();
        let list: Vec<String> = ring
            .iter()
            .map(|&i| tr!("tree-quoted-name", name = defs[i].name.to_string()))
            .collect();
        return Err(at(
            ring[0],
            tr!("feature-param-cycle", list = (list.join(", ")).to_string()),
        ));
    }

    let mut values = vec![0.0; n];
    for &i in &order {
        let result = {
            let lookup = |name: &str| {
                by_name
                    .get(&name.to_lowercase())
                    .map(|&j| (values[j], defs[j].unit))
            };
            evaluate(&exprs[i], &lookup).and_then(|(v, d)| fit_unit(v, d, defs[i].unit))
        };
        values[i] = result.map_err(|e| at(i, e.0))?;
    }
    Ok(values)
}
