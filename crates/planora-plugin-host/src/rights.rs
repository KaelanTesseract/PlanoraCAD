//! Rechte, Nutzerantworten und Anfragen mit Kennung.

use crate::manifest::{Manifest, Right};
use std::collections::{BTreeMap, BTreeSet};

/// Entscheidung über ein Recht.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Grant {
    Allowed,
    Denied,
}

/// Antwort des Nutzers auf eine Rechte-Nachfrage.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Answer {
    /// Dauerhaft erlauben.
    Allow,
    /// Nur bis zum Ende des Aufrufs erlauben.
    Once,
    /// Dauerhaft ablehnen.
    Deny,
}

/// Gespeicherte und einmalige Entscheidungen eines Plug-ins.
#[derive(Clone, Debug, Default)]
pub struct Rights {
    saved: BTreeMap<Right, Grant>,
    once: BTreeSet<Right>,
}

impl Rights {
    /// Aktuelle Entscheidung; `None` heißt: nachfragen. "Einmal" zählt als erlaubt.
    pub fn state(&self, r: Right) -> Option<Grant> {
        if self.once.contains(&r) {
            return Some(Grant::Allowed);
        }
        self.saved.get(&r).copied()
    }

    /// Entscheidung unter Beachtung des Manifests: ein nicht deklariertes Recht
    /// wird immer abgelehnt, ohne Nachfrage.
    pub fn decide(&self, declared: &BTreeSet<Right>, r: Right) -> Option<Grant> {
        if !declared.contains(&r) {
            return Some(Grant::Denied);
        }
        self.state(r)
    }

    /// Antwort des Nutzers festhalten.
    pub fn answer(&mut self, r: Right, a: Answer) {
        match a {
            Answer::Allow => {
                self.once.remove(&r);
                self.saved.insert(r, Grant::Allowed);
            }
            Answer::Deny => {
                self.once.remove(&r);
                self.saved.insert(r, Grant::Denied);
            }
            Answer::Once => {
                self.once.insert(r);
            }
        }
    }

    /// Entscheidung zurücknehmen: es wird wieder nachgefragt.
    pub fn revoke(&mut self, r: Right) {
        self.saved.remove(&r);
        self.once.remove(&r);
    }

    /// Die dauerhaft beantworteten Rechte (zum Speichern; „Nur dieses Mal“ gehört nicht dazu).
    pub fn saved(&self) -> impl Iterator<Item = (Right, Grant)> + '_ {
        self.saved.iter().map(|(r, g)| (*r, *g))
    }

    /// Alle "Einmal"-Erlaubnisse verfallen lassen.
    pub fn reset_once(&mut self) {
        self.once.clear();
    }
}

/// Zähler für Anfragen mit Kennung (ab 1, je Instanz neu).
#[derive(Clone, Debug, Default)]
pub struct Requests {
    last: u64,
}

impl Requests {
    /// Die nächste Kennung (der Name ist durch den Vertrag vorgegeben).
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> u64 {
        self.last += 1;
        self.last
    }
}

/// Eine offene Anfrage, auf deren Antwort gewartet wird.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Pending {
    AskRight { request: u64, right: Right },
    Net { request: u64, url: String },
}

/// Prüft eine Netz-Adresse gegen das Manifest: nur `https://`, Host exakt in
/// `net-hosts` (ohne Groß-/Kleinschreibung), keine Zugangsdaten, Port nur 443.
pub fn check_net_url(manifest: &Manifest, url: &str) -> Result<(), String> {
    let (scheme, rest) = url.split_once("://").ok_or("keine gültige Adresse")?;
    if !scheme.eq_ignore_ascii_case("https") {
        return Err("nur https ist erlaubt".into());
    }
    // Rückstriche behandeln Browser wie '/', deshalb ablehnen.
    if rest.contains('\\') {
        return Err("ungültiges Zeichen in der Adresse".into());
    }
    // Die Autorität endet am ersten '/', '?' oder '#'.
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let authority = &rest[..end];
    if authority.contains('@') {
        return Err("Zugangsdaten in der Adresse sind nicht erlaubt".into());
    }
    if authority.starts_with('[') {
        return Err("IP-Adressen sind nicht erlaubt".into());
    }
    let host = match authority.split_once(':') {
        None => authority,
        Some((host, port)) => {
            if port != "443" {
                return Err("nur Port 443 ist erlaubt".into());
            }
            host
        }
    };
    let host = host.strip_suffix('.').unwrap_or(host);
    if host.is_empty() {
        return Err("kein Rechnername".into());
    }
    if manifest
        .net_hosts
        .iter()
        .any(|h| h.eq_ignore_ascii_case(host))
    {
        Ok(())
    } else {
        Err(format!("Rechner {host} ist nicht im Manifest freigegeben"))
    }
}
