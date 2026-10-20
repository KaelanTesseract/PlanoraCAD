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
//! `+`, `-`, `min`, `max` passt eine reine Zahl zu allem; Länge mit Winkel (allgemein zwei
//! verschiedene Einheiten) ist ein Fehler. `*` und `/` addieren bzw. ziehen Exponenten ab;
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

const EMPTY: &str = "Die Formel ist leer.";
const TOO_LONG: &str = "Die Formel ist zu lang (höchstens 200 Zeichen).";
const TOO_DEEP: &str = "Die Formel ist zu tief verschachtelt (höchstens 64 Ebenen).";
const END: &str = "Die Formel endet unerwartet.";
const NO_CLOSE: &str = "Es fehlt eine schließende Klammer.";
const DIV_ZERO: &str = "Division durch null.";
const SQRT_NEG: &str = "Die Wurzel einer negativen Zahl gibt es nicht.";
const SQRT_UNIT: &str = "„sqrt“ passt hier nicht zur Einheit.";
const POW_UNIT: &str =
    "Eine Länge oder ein Winkel lässt sich nur mit einer ganzen Zahl potenzieren.";
const MIX: &str = "Länge und Winkel lassen sich nicht zusammenrechnen.";
const NOT_FINITE: &str = "Das Ergebnis ist zu groß oder keine Zahl.";
const NAME_EMPTY: &str = "Der Name darf nicht leer sein.";
const NAME_FORM: &str =
    "Ein Name beginnt mit einem Buchstaben oder „_“ und enthält nur Buchstaben, Ziffern und „_“.";
const NAME_LONG: &str = "Der Name ist zu lang (höchstens 64 Zeichen).";
const TOO_MANY: &str = "Höchstens 500 Parameter.";

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
    pub fn label(self) -> &'static str {
        match self {
            ParamUnit::Length => "Länge",
            ParamUnit::Angle => "Winkel",
            ParamUnit::Number => "Zahl",
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
    ExprError(format!(
        "Unerwartetes Zeichen „{}“ an Stelle {}.",
        t.first, t.pos
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
            return fail(TOO_DEEP);
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
            return fail(END);
        };
        let text = &self.text[t.start..t.end];
        match t.kind {
            Tok::Num => {
                let v: f64 = text.replace(',', ".").parse().map_err(|_| unexpected(&t))?;
                if !v.is_finite() {
                    return fail(NOT_FINITE);
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
            None => fail(NO_CLOSE),
        }
    }

    fn call(&mut self, name: &str) -> Result<Node, ExprError> {
        let Some(func) = Func::from_lower(&name.to_lowercase()) else {
            return fail(format!("Die Funktion „{name}“ gibt es nicht."));
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
                    None => return fail(NO_CLOSE),
                }
            }
        }
        self.leave();
        if func.many() {
            if args.len() < 2 {
                return fail(format!(
                    "„{}“ braucht mindestens zwei Werte (Werte mit „;“ oder „, “ trennen).",
                    func.name()
                ));
            }
        } else if args.len() != 1 {
            return fail(format!("„{}“ braucht genau einen Wert.", func.name()));
        }
        Ok(Node::Call(func, args))
    }
}

/// Zerlegt eine Formel; prüft Länge, Leere, Schreibweise, Funktionsnamen, Wertezahl und
/// Verschachtelungstiefe (unbekannte Parameternamen erst beim Auswerten).
pub fn parse(text: &str) -> Result<Expr, ExprError> {
    if text.chars().count() > MAX_FORMULA_CHARS {
        return fail(TOO_LONG);
    }
    if text.trim().is_empty() {
        return fail(EMPTY);
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

/// Gemeinsame Einheit für `+`, `-`, `min`, `max`: eine reine Zahl passt zu allem.
fn join(a: Dim, b: Dim) -> Result<Dim, ExprError> {
    if a == b || b == Dim::NUMBER {
        Ok(a)
    } else if a == Dim::NUMBER {
        Ok(b)
    } else {
        fail(MIX)
    }
}

fn dim_op(a: Dim, b: Dim, f: fn(i32, i32) -> Option<i32>) -> Result<Dim, ExprError> {
    match (f(a.length, b.length), f(a.angle, b.angle)) {
        (Some(length), Some(angle)) => Ok(Dim { length, angle }),
        _ => fail(NOT_FINITE),
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

fn eval_node(node: &Node, lookup: Lookup<'_>) -> Result<(f64, Dim), ExprError> {
    let (v, d) = match node {
        Node::Num(v) => (*v, Dim::NUMBER),
        Node::Pi => (PI, Dim::NUMBER),
        Node::Name(name) => match lookup(name) {
            Some((v, unit)) => (v, Dim::of(unit)),
            None => return fail(format!("Den Namen „{name}“ gibt es nicht.")),
        },
        Node::Neg(inner) => {
            let (v, d) = eval_node(inner, lookup)?;
            (-v, d)
        }
        Node::Bin(op, l, r) => {
            let (a, da) = eval_node(l, lookup)?;
            let (b, db) = eval_node(r, lookup)?;
            match op {
                Op::Add => (a + b, join(da, db)?),
                Op::Sub => (a - b, join(da, db)?),
                Op::Mul => (a * b, dim_op(da, db, i32::checked_add)?),
                Op::Div => {
                    if b == 0.0 {
                        return fail(DIV_ZERO);
                    }
                    (a / b, dim_op(da, db, i32::checked_sub)?)
                }
                Op::Pow => {
                    if db != Dim::NUMBER {
                        return fail(POW_UNIT);
                    }
                    let d = if da == Dim::NUMBER {
                        Dim::NUMBER
                    } else {
                        if b.fract() != 0.0 {
                            return fail(POW_UNIT);
                        }
                        if b.abs() > f64::from(i32::MAX) {
                            return fail(NOT_FINITE);
                        }
                        let n = Dim {
                            length: b as i32,
                            angle: b as i32,
                        };
                        dim_op(da, n, i32::checked_mul)?
                    };
                    (a.powf(b), d)
                }
            }
        }
        Node::Call(func, args) => {
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
                    fail(format!(
                        "„{}“ braucht einen Winkel oder eine Zahl.",
                        func.name()
                    ))
                }
            };
            match func {
                Func::Sqrt => {
                    if dx.length % 2 != 0 || dx.angle % 2 != 0 {
                        return fail(SQRT_UNIT);
                    }
                    if x < 0.0 {
                        return fail(SQRT_NEG);
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
                        return fail(NOT_FINITE);
                    }
                    (no_neg_zero(sin_deg(x) / c), Dim::NUMBER)
                }
                Func::Abs => (x.abs(), dx),
                Func::Round => (x.round(), dx),
                Func::Min | Func::Max => {
                    let mut acc = (x, dx);
                    for &(v, d) in &vals[1..] {
                        let m = if *func == Func::Min {
                            acc.0.min(v)
                        } else {
                            acc.0.max(v)
                        };
                        acc = (m, join(acc.1, d)?);
                    }
                    acc
                }
            }
        }
    };
    if !v.is_finite() {
        return fail(NOT_FINITE);
    }
    Ok((v, d))
}

/// Wertet eine zerlegte Formel aus. `lookup` bekommt Namen wie geschrieben und vergleicht
/// selbst ohne Rücksicht auf Groß-/Kleinschreibung.
pub fn evaluate(
    e: &Expr,
    lookup: &dyn Fn(&str) -> Option<(f64, ParamUnit)>,
) -> Result<(f64, Dim), ExprError> {
    eval_node(&e.root, lookup)
}

fn dim_text(d: Dim) -> &'static str {
    if d == Dim::of(ParamUnit::Length) {
        "eine Länge"
    } else if d == Dim::of(ParamUnit::Angle) {
        "einen Winkel"
    } else if d == Dim::NUMBER {
        "eine Zahl"
    } else {
        "eine andere Einheit"
    }
}

/// Prüft, ob ein Ergebnis in ein Feld der Art `want` passt (seine Einheit oder eine Zahl).
pub fn fit_unit(value: f64, dim: Dim, want: ParamUnit) -> Result<f64, ExprError> {
    if !value.is_finite() {
        return fail(NOT_FINITE);
    }
    if dim == Dim::of(want) || dim == Dim::NUMBER {
        return Ok(value);
    }
    let soll = match want {
        ParamUnit::Length => "eine Länge",
        ParamUnit::Angle => "ein Winkel",
        ParamUnit::Number => "eine Zahl",
    };
    fail(format!(
        "Hier wird {soll} erwartet, die Formel ergibt {}.",
        dim_text(dim)
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

/// Zahl als Text mit Komma, verlustfrei: `plain_number(&number_text(v)) == Some(v)`.
pub fn number_text(v: f64) -> String {
    v.to_string().replace('.', ",")
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
        return fail(NAME_EMPTY);
    }
    if name.chars().count() > MAX_PARAM_NAME_CHARS {
        return fail(NAME_LONG);
    }
    let mut chars = name.chars();
    let form_ok = chars.next().is_some_and(is_name_start) && chars.all(is_name_char);
    if !form_ok {
        return fail(NAME_FORM);
    }
    if RESERVED.contains(&name.to_lowercase().as_str()) {
        return fail(format!("„{name}“ ist schon eine Funktion oder Konstante."));
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
        return Err(at(MAX_PARAMETERS, TOO_MANY.into()));
    }
    let n = defs.len();
    let mut by_name: HashMap<String, usize> = HashMap::with_capacity(n);
    for (i, d) in defs.iter().enumerate() {
        check_name(d.name).map_err(|e| at(i, e.0))?;
        if by_name.insert(d.name.to_lowercase(), i).is_some() {
            return Err(at(i, format!("Den Namen „{}“ gibt es schon.", d.name)));
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
                    return Err(at(
                        i,
                        format!("Der Parameter „{}“ hängt von sich selbst ab.", d.name),
                    ));
                }
                Some(&j) => mine.push(j),
                None => return Err(at(i, format!("Den Namen „{name}“ gibt es nicht."))),
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
            .map(|&i| format!("„{}“", defs[i].name))
            .collect();
        return Err(at(
            ring[0],
            format!(
                "Die Parameter {} hängen im Kreis voneinander ab.",
                list.join(", ")
            ),
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
