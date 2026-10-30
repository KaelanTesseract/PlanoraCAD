//! Versionsnummern nach SemVer mit genau zwei erlaubten Endungen: `-dev.N` und `-beta.N`.

use std::cmp::Ordering;
use std::fmt;

use planora_i18n::tr;

use crate::channel::Channel;

/// Art der Vorabversion. Die Reihenfolge der Varianten ist die Sortierreihenfolge (`Dev` < `Beta`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PreKind {
    /// Endung `-dev.N`.
    Dev,
    /// Endung `-beta.N`.
    Beta,
}

impl PreKind {
    fn as_str(self) -> &'static str {
        match self {
            PreKind::Dev => "dev",
            PreKind::Beta => "beta",
        }
    }
}

/// Endung einer Vorabversion, z. B. `dev.3`. Sortiert erst nach Art, dann nach Nummer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Pre {
    /// Dev oder Beta.
    pub kind: PreKind,
    /// Laufende Nummer hinter dem Punkt.
    pub number: u64,
}

/// Versionsnummer `MAJOR.MINOR.PATCH` mit optionaler Endung `-dev.N` oder `-beta.N`.
///
/// Reihenfolge nach SemVer: Zahlen werden als Zahlen verglichen; bei gleichen Zahlen gilt
/// `dev` < `beta` < ohne Endung.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Version {
    /// Hauptnummer.
    pub major: u64,
    /// Nebennummer.
    pub minor: u64,
    /// Korrekturnummer.
    pub patch: u64,
    /// Endung der Vorabversion; `None` für eine stabile Version.
    pub pre: Option<Pre>,
}

/// Fehler beim Lesen einer Versionsnummer; enthält die abgelehnte Eingabe.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    input: String,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&tr!(
            "update-version-invalid",
            input = format!("{:?}", self.input)
        ))
    }
}

impl std::error::Error for ParseError {}

/// Liest eine Zahl ohne Vorzeichen und ohne führende Null (außer der Null selbst).
fn number(part: &str) -> Option<u64> {
    if part.is_empty() || !part.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    if part.len() > 1 && part.starts_with('0') {
        return None;
    }
    part.parse().ok()
}

impl Version {
    /// Liest `0.2.0-dev.1`, `0.9.0-beta.2` oder `1.0.0`, wahlweise mit führendem `v`
    /// (wie in Git-Tags). Andere Endungen, Build-Angaben (`+…`), Leerzeichen und führende
    /// Nullen werden abgelehnt.
    pub fn parse(text: &str) -> Result<Version, ParseError> {
        Self::parse_inner(text).ok_or_else(|| ParseError {
            input: text.to_string(),
        })
    }

    fn parse_inner(text: &str) -> Option<Version> {
        let text = text.strip_prefix('v').unwrap_or(text);
        let (core, pre) = match text.split_once('-') {
            Some((core, pre)) => (core, Some(pre)),
            None => (text, None),
        };
        let mut parts = core.split('.');
        let major = number(parts.next()?)?;
        let minor = number(parts.next()?)?;
        let patch = number(parts.next()?)?;
        if parts.next().is_some() {
            return None;
        }
        let pre = match pre {
            None => None,
            Some(pre) => {
                let (kind, num) = pre.split_once('.')?;
                let kind = match kind {
                    "dev" => PreKind::Dev,
                    "beta" => PreKind::Beta,
                    _ => return None,
                };
                Some(Pre {
                    kind,
                    number: number(num)?,
                })
            }
        };
        Some(Version {
            major,
            minor,
            patch,
            pre,
        })
    }

    /// Version aus `env!("CARGO_PKG_VERSION")` des aufrufenden Crates. Das App-Crate legt
    /// damit seine laufende Version fest.
    ///
    /// # Panics
    ///
    /// Wenn die Cargo-Version nicht dem erlaubten Format entspricht. Das ist ein Fehler im
    /// Build (Versionsnummer in `Cargo.toml`), den ein Test im App-Crate sofort aufdeckt.
    pub fn from_cargo(cargo_version: &str) -> Version {
        match Self::parse(cargo_version) {
            Ok(version) => version,
            Err(err) => panic!("Cargo version unsuitable for updates: {err}"),
        }
    }

    /// Kanal, zu dem die Version gehört: Endung `dev` → Dev, `beta` → Beta, ohne → Stabil.
    pub fn channel(&self) -> Channel {
        match self.pre {
            None => Channel::Stable,
            Some(Pre {
                kind: PreKind::Dev, ..
            }) => Channel::Dev,
            Some(Pre {
                kind: PreKind::Beta,
                ..
            }) => Channel::Beta,
        }
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        (self.major, self.minor, self.patch)
            .cmp(&(other.major, other.minor, other.patch))
            .then_with(|| match (&self.pre, &other.pre) {
                (None, None) => Ordering::Equal,
                // Ohne Endung steht über jeder Vorabversion derselben Nummer.
                (None, Some(_)) => Ordering::Greater,
                (Some(_), None) => Ordering::Less,
                (Some(a), Some(b)) => a.cmp(b),
            })
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)?;
        if let Some(pre) = self.pre {
            write!(f, "-{}.{}", pre.kind.as_str(), pre.number)?;
        }
        Ok(())
    }
}
