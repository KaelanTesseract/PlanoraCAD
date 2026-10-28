//! Meldungen, die ein Fenster in seinem Zustand behält (Fehler eines Dialogs, Rückfrage): Sie
//! speichern den Grund (Id und Werte), nicht den fertigen Text, und werden erst beim Zeichnen
//! übersetzt. So wechselt eine offene Meldung mit der Sprache.
//!
//! `Raw` nimmt Text, der von einer Bibliothek oder vom Betriebssystem kommt und (noch) keine Id
//! hat.

use planora_i18n::{Arg, fmt_num, localize_decimal, t_args};

/// Eine Meldung: Id der Sprachdateien mit Werten, oder Text ohne Id.
#[derive(Clone, Debug, PartialEq)]
pub enum Msg {
    /// Id und benannte Werte (`msg!("id", name = wert)`).
    Id(&'static str, Vec<(&'static str, MsgArg)>),
    /// Text einer Bibliothek oder des Betriebssystems.
    Raw(String),
}

/// Ein Wert einer Meldung.
#[derive(Clone, Debug, PartialEq)]
pub enum MsgArg {
    /// Fester Text (Name, vom Benutzer getippter Text, Text einer Bibliothek).
    Text(String),
    /// Ganze Zahl (für die Mehrzahl).
    Int(i64),
    /// Kommazahl mit Nachkommastellen; Dezimaltrenner der Sprache beim Zeichnen.
    Num(f64, usize),
    /// Kommazahl mit höchstens sechs Nachkommastellen ohne Nullen am Ende ([`plain_number`]).
    Plain(f64),
    /// Eine eingebettete Meldung, beim Zeichnen übersetzt.
    Msg(Box<Msg>),
}

impl Msg {
    /// Text in der aktuellen Sprache.
    pub fn text(&self) -> String {
        match self {
            Msg::Raw(s) => s.clone(),
            Msg::Id(id, args) => {
                let owned: Vec<(&str, Arg)> = args
                    .iter()
                    .map(|(k, v)| {
                        let a = match v {
                            MsgArg::Text(s) => Arg::from(s.clone()),
                            MsgArg::Int(n) => Arg::from(*n),
                            MsgArg::Num(v, d) => Arg::from(fmt_num(*v, *d)),
                            MsgArg::Plain(v) => Arg::from(plain_number(*v)),
                            MsgArg::Msg(m) => Arg::from(m.text()),
                        };
                        (*k, a)
                    })
                    .collect();
                t_args(id, &owned)
            }
        }
    }
}

/// Zahl für Meldungen: Dezimaltrenner der Sprache, ohne überflüssige Nullen (höchstens sechs
/// Nachkommastellen).
pub fn plain_number(v: f64) -> String {
    let s = format!("{v:.6}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    localize_decimal(s)
}

impl From<Msg> for String {
    fn from(m: Msg) -> String {
        m.text()
    }
}

impl From<String> for MsgArg {
    fn from(s: String) -> Self {
        MsgArg::Text(s)
    }
}

impl From<&String> for MsgArg {
    fn from(s: &String) -> Self {
        MsgArg::Text(s.clone())
    }
}

impl From<&str> for MsgArg {
    fn from(s: &str) -> Self {
        MsgArg::Text(s.to_string())
    }
}

impl From<usize> for MsgArg {
    fn from(n: usize) -> Self {
        MsgArg::Int(i64::try_from(n).unwrap_or(i64::MAX))
    }
}

impl From<i64> for MsgArg {
    fn from(n: i64) -> Self {
        MsgArg::Int(n)
    }
}

impl From<Msg> for MsgArg {
    fn from(m: Msg) -> Self {
        MsgArg::Msg(Box::new(m))
    }
}

/// Meldung zur Id mit Werten: `msg!("id")` oder `msg!("id", name = wert, …)`. Die Id wird von
/// der Vollständigkeitsprüfung der Sprachdateien wie bei `tr!` gesucht.
#[macro_export]
macro_rules! msg {
    ($id:literal) => {
        $crate::msg::Msg::Id($id, ::std::vec::Vec::new())
    };
    ($id:literal, $($k:ident = $v:expr),+ $(,)?) => {
        $crate::msg::Msg::Id(
            $id,
            ::std::vec![$((stringify!($k), $crate::msg::MsgArg::from($v))),+],
        )
    };
}
