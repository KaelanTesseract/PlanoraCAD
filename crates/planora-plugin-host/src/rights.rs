//! Rechte, Nutzerantworten und Anfragen mit Kennung.

use crate::manifest::{Manifest, Right};
use planora_i18n::tr;
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

    /// Was von den dauerhaften Entscheidungen bleibt, wenn das Plug-in mit dem Manifest `old`
    /// durch eines mit `new` ersetzt wird (die Pakete sind nicht signiert, die Kennung allein
    /// sagt nicht, wer es ist): nichts ohne altes Manifest oder bei anderem `author`; `net` fällt
    /// weg, wenn sich die Rechner in `net-hosts` geändert haben (ohne Rücksicht auf Groß- und
    /// Kleinschreibung); Rechte, die `new` nicht mehr verlangt, fallen weg. „Nur dieses Mal“
    /// verfällt immer.
    pub fn carry_over(&self, old: Option<&Manifest>, new: &Manifest) -> Rights {
        match old {
            Some(old) => self.carry_over_from(Some(&old.author), &old.net_hosts, new),
            None => Rights::default(),
        }
    }

    /// Wie [`Rights::carry_over`], nur mit Autor und Rechnern statt des alten Manifests (so
    /// stehen sie neben den Rechten in der Konfiguration): ohne Autor (`None`, unbekannt) oder
    /// mit anderem bleibt nichts.
    pub fn carry_over_from(
        &self,
        old_author: Option<&str>,
        old_hosts: &[String],
        new: &Manifest,
    ) -> Rights {
        let mut out = Rights::default();
        if old_author != Some(new.author.as_str()) {
            return out;
        }
        let hosts = |list: &[String]| -> BTreeSet<String> {
            list.iter().map(|h| h.to_ascii_lowercase()).collect()
        };
        let same_hosts = hosts(old_hosts) == hosts(&new.net_hosts);
        for (r, g) in self.saved() {
            if new.rights.contains(&r) && (r != Right::Net || same_hosts) {
                out.saved.insert(r, g);
            }
        }
        out
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
    let (scheme, rest) = url
        .split_once("://")
        .ok_or_else(|| tr!("plugin-url-invalid"))?;
    if !scheme.eq_ignore_ascii_case("https") {
        return Err(tr!("plugin-url-https-only"));
    }
    // Rückstriche behandeln Browser wie '/', deshalb ablehnen.
    if rest.contains('\\') {
        return Err(tr!("plugin-url-bad-char"));
    }
    // Die Autorität endet am ersten '/', '?' oder '#'.
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let authority = &rest[..end];
    if authority.contains('@') {
        return Err(tr!("plugin-url-credentials"));
    }
    if authority.starts_with('[') {
        return Err(tr!("plugin-url-ip"));
    }
    let host = match authority.split_once(':') {
        None => authority,
        Some((host, port)) => {
            if port != "443" {
                return Err(tr!("plugin-url-port"));
            }
            host
        }
    };
    let host = host.strip_suffix('.').unwrap_or(host);
    if host.is_empty() {
        return Err(tr!("plugin-url-no-host"));
    }
    if manifest
        .net_hosts
        .iter()
        .any(|h| h.eq_ignore_ascii_case(host))
    {
        Ok(())
    } else {
        Err(tr!("plugin-url-host-not-allowed", host = host))
    }
}
