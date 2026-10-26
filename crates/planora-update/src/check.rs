//! Prüfen, ob es eine neue Version gibt: Schnittstelle zur Quelle der Release-Liste, Netzfehler,
//! der Ablauf einer Prüfung und der 24-Stunden-Takt.
//!
//! Die Quelle ist austauschbar ([`ReleaseSource`]): Das Programm benutzt [`HttpSource`] (GitHub
//! über HTTPS), Tests eine Nachbildung ohne Netz. Eine Prüfung stürzt nie ab; jeder Fehler wird
//! zu [`CheckOutcome::Failed`].

use std::fmt;
use std::sync::Arc;

use crate::channel::Channel;
use crate::release::{Update, parse_releases, pick_update};
use crate::version::Version;

pub use crate::net::{HttpSource, MAX_RESPONSE_BYTES, RELEASES_URL, TIMEOUT};

/// Takt der Prüfung beim Start: höchstens einmal je 24 Stunden.
pub const CHECK_INTERVAL_SECS: u64 = 86_400;

/// Fehler beim Abruf der Release-Liste.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NetError {
    /// Kein Netz: Name nicht auflösbar, Verbindung abgelehnt oder unterbrochen.
    Offline,
    /// Zeitüberschreitung beim Verbinden oder beim Warten auf die Antwort.
    Timeout,
    /// GitHub lehnt wegen zu vieler Anfragen ab (ohne Anmeldung 60 je Stunde).
    RateLimited,
    /// Andere Antwort als „OK“ (Statuscode).
    Http(u16),
    /// Alles andere, etwa eine unlesbare oder zu große Antwort.
    Other(String),
    /// Auf diesem System gibt es keine Aktualisierung über das Programm (alles außer Windows);
    /// es wird gar nicht erst gefragt.
    Unsupported,
}

impl fmt::Display for NetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NetError::Offline => write!(f, "keine Verbindung"),
            NetError::Timeout => write!(f, "Zeitüberschreitung"),
            NetError::RateLimited => write!(f, "zu viele Anfragen, später erneut versuchen"),
            NetError::Http(code) => write!(f, "Antwort mit Status {code}"),
            NetError::Other(msg) => write!(f, "{msg}"),
            NetError::Unsupported => write!(f, "auf diesem System nicht vorgesehen"),
        }
    }
}

impl std::error::Error for NetError {}

/// Liefert die Release-Liste von GitHub als JSON-Text (Antwort von [`RELEASES_URL`]).
pub trait ReleaseSource {
    fn fetch_releases(&self) -> Result<String, NetError>;
}

impl<S: ReleaseSource + ?Sized> ReleaseSource for Arc<S> {
    fn fetch_releases(&self) -> Result<String, NetError> {
        (**self).fetch_releases()
    }
}

impl<S: ReleaseSource + ?Sized> ReleaseSource for &S {
    fn fetch_releases(&self) -> Result<String, NetError> {
        (**self).fetch_releases()
    }
}

/// Ergebnis einer Prüfung.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckOutcome {
    /// Keine höhere, nicht übersprungene Version im Kanal.
    UpToDate,
    /// Ein Update steht bereit (in einer Box: das Update ist groß, die übrigen Fälle klein).
    Available(Box<Update>),
    /// Abruf oder Antwort gescheitert; wird still übergangen (bei manueller Suche ein Hinweis).
    Failed(NetError),
}

/// Eine Prüfung mit einer Quelle.
pub struct Checker<S: ReleaseSource> {
    source: S,
}

impl<S: ReleaseSource> Checker<S> {
    pub fn new(source: S) -> Self {
        Checker { source }
    }

    /// Fragt die Quelle (blockierend) und wählt das Update für die laufende Version
    /// ([`pick_update`]). Eine unlesbare Antwort wird zu [`NetError::Other`].
    pub fn check(
        &self,
        running: &Version,
        channel: Channel,
        skipped: Option<&Version>,
    ) -> CheckOutcome {
        let json = match self.source.fetch_releases() {
            Ok(json) => json,
            Err(e) => return CheckOutcome::Failed(e),
        };
        let releases = match parse_releases(&json) {
            Ok(r) => r,
            Err(e) => return CheckOutcome::Failed(NetError::Other(e.to_string())),
        };
        match pick_update(&releases, running, channel, skipped) {
            Some(update) => CheckOutcome::Available(Box::new(update)),
            None => CheckOutcome::UpToDate,
        }
    }
}

/// Ist eine Prüfung fällig? `now` und `last` in Sekunden seit 1970. Manuell immer (Takt und
/// Schalter zählen nicht); sonst nur mit eingeschaltetem Schalter, wenn noch nie geprüft wurde,
/// die letzte Prüfung mindestens [`CHECK_INTERVAL_SECS`] her ist oder in der Zukunft liegt
/// (verstellte Uhr: lieber einmal zu oft prüfen als nie wieder).
pub fn due(now: u64, last: Option<u64>, enabled: bool, manual: bool) -> bool {
    if manual {
        return true;
    }
    if !enabled {
        return false;
    }
    match last {
        None => true,
        Some(last) if last > now => true,
        Some(last) => now - last >= CHECK_INTERVAL_SECS,
    }
}
