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
    /// Nur die eine Anfrage erlauben, die nachgefragt hat (und die bis dahin wartenden Anfragen
    /// desselben Rechts); die nächste fragt wieder. Wird nicht in [`Rights`] vermerkt.
    Once,
    /// Dauerhaft ablehnen.
    Deny,
}

/// Woher ein Plug-in kommt: Rechte gehen beim Ersetzen nur auf eines derselben Herkunft über.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Source {
    /// Mitgeliefert (mit dem Programm installiert).
    Bundled,
    /// Vom Nutzer aus einer Datei installiert (ohne `herkunft.json`).
    File,
    /// Über einen Repo-Link installiert (der Link aus `herkunft.json`, klein geschrieben).
    Link(String),
}

impl Source {
    /// Eine Herkunft über einen Repo-Link (Groß- und Kleinschreibung zählt nicht).
    pub fn link(link: &str) -> Source {
        Source::Link(link.to_ascii_lowercase())
    }

    /// Die Form in der Konfiguration: `"bundled"`, `"file"` oder der Link.
    pub fn to_config(&self) -> String {
        match self {
            Source::Bundled => "bundled".to_string(),
            Source::File => "file".to_string(),
            Source::Link(l) => l.clone(),
        }
    }

    /// Liest die Form aus [`Source::to_config`].
    pub fn from_config(text: &str) -> Source {
        match text {
            "bundled" => Source::Bundled,
            "file" => Source::File,
            link => Source::link(link),
        }
    }
}

/// Die dauerhaften Entscheidungen eines Plug-ins („Nur dieses Mal“ gehört nicht dazu, das
/// erledigt der Host je Anfrage).
#[derive(Clone, Debug, Default)]
pub struct Rights {
    saved: BTreeMap<Right, Grant>,
}

impl Rights {
    /// Aktuelle Entscheidung; `None` heißt: nachfragen.
    pub fn state(&self, r: Right) -> Option<Grant> {
        self.saved.get(&r).copied()
    }

    /// Entscheidung unter Beachtung des Manifests: ein nicht deklariertes Recht und eines, das
    /// diese Programmversion noch nicht umsetzt ([`Right::available`]), wird immer abgelehnt,
    /// ohne Nachfrage.
    pub fn decide(&self, declared: &BTreeSet<Right>, r: Right) -> Option<Grant> {
        if !declared.contains(&r) || !r.available() {
            return Some(Grant::Denied);
        }
        self.state(r)
    }

    /// Antwort des Nutzers festhalten (`Once` ändert nichts, siehe [`Answer::Once`]).
    pub fn answer(&mut self, r: Right, a: Answer) {
        match a {
            Answer::Allow => {
                self.saved.insert(r, Grant::Allowed);
            }
            Answer::Deny => {
                self.saved.insert(r, Grant::Denied);
            }
            Answer::Once => {}
        }
    }

    /// Entscheidung zurücknehmen: es wird wieder nachgefragt.
    pub fn revoke(&mut self, r: Right) {
        self.saved.remove(&r);
    }

    /// Die dauerhaft beantworteten Rechte (zum Speichern).
    pub fn saved(&self) -> impl Iterator<Item = (Right, Grant)> + '_ {
        self.saved.iter().map(|(r, g)| (*r, *g))
    }

    /// Was von den dauerhaften Entscheidungen bleibt, wenn das Plug-in mit dem Manifest `old`
    /// (Herkunft `old_source`) durch eines mit `new` (Herkunft `new_source`) ersetzt wird (die
    /// Pakete sind nicht signiert, die Kennung allein sagt nicht, wer es ist): nichts ohne altes
    /// Manifest, bei anderem `author` oder anderer Herkunft ([`Source`]); `net` fällt weg, wenn
    /// sich die Rechner in `net-hosts` geändert haben (ohne Rücksicht auf Groß- und
    /// Kleinschreibung); Rechte, die `new` nicht mehr verlangt, fallen weg.
    pub fn carry_over(
        &self,
        old: Option<(&Manifest, &Source)>,
        new: &Manifest,
        new_source: &Source,
    ) -> Rights {
        match old {
            Some((old, source)) => self.carry_over_from(
                Some(&old.author),
                &old.net_hosts,
                Some(source),
                new,
                new_source,
            ),
            None => Rights::default(),
        }
    }

    /// Wie [`Rights::carry_over`], nur mit Autor, Rechnern und Herkunft statt des alten
    /// Manifests (so stehen sie neben den Rechten in der Konfiguration): ohne Autor oder
    /// Herkunft (`None`, unbekannt) oder mit anderen bleibt nichts.
    pub fn carry_over_from(
        &self,
        old_author: Option<&str>,
        old_hosts: &[String],
        old_source: Option<&Source>,
        new: &Manifest,
        new_source: &Source,
    ) -> Rights {
        let mut out = Rights::default();
        if old_author != Some(new.author.as_str()) || old_source != Some(new_source) {
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

/// Längste Netz-Adresse, die ein Plug-in abrufen darf (Bytes).
pub const MAX_URL_LEN: usize = 8 << 10;

/// Prüft eine Netz-Adresse gegen das Manifest: nur `https://`, Host exakt in
/// `net-hosts` (ohne Groß-/Kleinschreibung), keine Zugangsdaten, Port nur 443, höchstens
/// [`MAX_URL_LEN`] Bytes lang.
pub fn check_net_url(manifest: &Manifest, url: &str) -> Result<(), String> {
    if url.len() > MAX_URL_LEN {
        return Err(tr!("plugin-url-too-long", max = MAX_URL_LEN.to_string()));
    }
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
