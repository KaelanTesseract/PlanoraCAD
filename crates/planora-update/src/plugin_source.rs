//! Plug-ins aus einem GitHub-Repo (Plug-ins Etappe 1, Task 8): der Repo-Link, den der Nutzer
//! einfügt, die Releases dieses Repos mit genau einem Paket `*.pcplugin` (dazu wahlweise
//! `<paket>.sha256`), die Wahl des passenden Releases und das Laden mit selbst geprüften
//! Weiterleitungen.
//!
//! Anders als das Update des Programms gilt hier kein festes [`crate::REPO`]: jede Adresse muss
//! unter dem Repo des Links liegen, Weiterleitungen nur nach `github.com`, `api.github.com` und
//! `*.githubusercontent.com` (die Regel aus [`crate::install::redirect_target`] plus der
//! API-Rechner, der umbenannte Repos weiterleitet). Was ein Paket ist, weiß diese Bibliothek
//! nicht; das prüft das Programm.

use std::time::Duration;

use planora_i18n::tr;
use serde::Deserialize;

use crate::channel::Channel;
use crate::install::{MAX_REDIRECTS, checked_redirect, host_allowed, split_origin};
use crate::release::AssetRef;

/// Größte angenommene Release-Liste (Bytes).
pub const MAX_RELEASES_BYTES: u64 = 4 << 20;
/// Größte angenommene Prüfsummendatei (Bytes).
pub const MAX_CHECKSUM_BYTES: u64 = 64 << 10;
/// Zeitgrenze je Anfrage (je Weiterleitung neu).
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
/// Endung der Paketdatei im Release.
pub const PACKAGE_SUFFIX: &str = ".pcplugin";

/// Ein GitHub-Repo: `https://github.com/<owner>/<repo>`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RepoLink {
    pub owner: String,
    pub repo: String,
}

impl RepoLink {
    /// Adresse der Release-Liste.
    pub fn releases_url(&self) -> String {
        format!(
            "https://api.github.com/repos/{}/{}/releases",
            self.owner, self.repo
        )
    }

    /// Adresse des Repos im Browser (so wird der Link gespeichert und gezeigt).
    pub fn web_url(&self) -> String {
        format!("https://github.com/{}/{}", self.owner, self.repo)
    }
}

/// Ein Release mit genau einem Plug-in-Paket.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PluginRelease {
    /// Version aus dem Tag (mit oder ohne `v`).
    pub version: semver::Version,
    /// Tag wie auf GitHub.
    pub tag: String,
    /// Release-Notizen (Markdown); leer, wenn keine.
    pub notes: String,
    /// Das Paket `*.pcplugin`.
    pub package: AssetRef,
    /// Die Prüfsummendatei `<paket>.sha256`, falls vorhanden.
    pub sha256: Option<AssetRef>,
}

/// Ist `part` ein erlaubter Besitzer- oder Repo-Name (Buchstaben, Ziffern, `-`, `_`, `.`;
/// nicht `.`, kein `..`)?
fn name_ok(part: &str) -> bool {
    !part.is_empty()
        && part != "."
        && !part.contains("..")
        && part
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
}

/// Liest einen Repo-Link: `https://github.com/<besitzer>/<repo>`, auch ohne `https://`, mit
/// `/` am Ende oder `.git`; der Rechner ohne Rücksicht auf Groß- und Kleinschreibung. Alles
/// andere (anderer Rechner, `http://`, Anmeldedaten, Port, Anfrageteil, weitere Pfadteile,
/// `..`, Leerzeichen) ergibt den Fehlertext `plugin-link-invalid`.
pub fn parse_repo_link(text: &str) -> Result<RepoLink, String> {
    let invalid = || tr!("plugin-link-invalid");
    let text = text.trim();
    if text.is_empty() || text.contains(|c: char| c.is_whitespace() || c == '\\') {
        return Err(invalid());
    }
    if text.contains(['?', '#']) {
        return Err(invalid());
    }
    let rest = match text.split_once("://") {
        Some((scheme, rest)) if scheme.eq_ignore_ascii_case("https") => rest,
        Some(_) => return Err(invalid()),
        None => text,
    };
    let (host, path) = rest.split_once('/').ok_or_else(invalid)?;
    if !host.eq_ignore_ascii_case("github.com") {
        return Err(invalid());
    }
    let path = path.strip_suffix('/').unwrap_or(path);
    let (owner, repo) = path.split_once('/').ok_or_else(invalid)?;
    let repo = repo.strip_suffix(".git").unwrap_or(repo);
    if !name_ok(owner) || !name_ok(repo) {
        return Err(invalid());
    }
    Ok(RepoLink {
        owner: owner.to_string(),
        repo: repo.to_string(),
    })
}

#[derive(Deserialize)]
struct RawRelease {
    tag_name: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    assets: Vec<RawAsset>,
}

#[derive(Deserialize)]
struct RawAsset {
    name: String,
    browser_download_url: String,
    size: u64,
}

/// Version aus einem Tag (`v1.2.3` oder `1.2.3`).
pub fn tag_version(tag: &str) -> Option<semver::Version> {
    semver::Version::parse(tag.strip_prefix('v').unwrap_or(tag)).ok()
}

/// Liegt `url` genau unter `https://github.com/<owner>/<repo>/releases/download/<tag>/<name>`
/// (Besitzer und Repo ohne Rücksicht auf Groß- und Kleinschreibung)?
fn asset_url_ok(link: &RepoLink, tag: &str, name: &str, url: &str) -> bool {
    if name.is_empty() || name.contains(['/', '\\', '?', '#', '@']) || name.contains("..") {
        return false;
    }
    let base = format!("https://github.com/{}/{}/", link.owner, link.repo);
    let Some(head) = url.get(..base.len()) else {
        return false;
    };
    head.eq_ignore_ascii_case(&base)
        && url[base.len()..] == format!("releases/download/{tag}/{name}")
}

fn asset(a: &RawAsset) -> AssetRef {
    AssetRef {
        name: a.name.clone(),
        url: a.browser_download_url.clone(),
        size: a.size,
    }
}

fn plugin_release(raw: RawRelease, link: &RepoLink) -> Option<PluginRelease> {
    if raw.draft || raw.tag_name.contains(['/', '\\', '?', '#']) {
        return None;
    }
    let version = tag_version(&raw.tag_name)?;
    let mut packages = raw
        .assets
        .iter()
        .filter(|a| a.name.ends_with(PACKAGE_SUFFIX));
    let package = packages.next()?;
    if packages.next().is_some()
        || !asset_url_ok(
            link,
            &raw.tag_name,
            &package.name,
            &package.browser_download_url,
        )
    {
        return None;
    }
    let sum_name = format!("{}.sha256", package.name);
    let mut sums = raw.assets.iter().filter(|a| a.name == sum_name);
    let sha256 = match (sums.next(), sums.next()) {
        (None, _) => None,
        (Some(s), None) if asset_url_ok(link, &raw.tag_name, &s.name, &s.browser_download_url) => {
            Some(asset(s))
        }
        // Doppelt oder an fremder Adresse: das ganze Release fällt weg.
        _ => return None,
    };
    Some(PluginRelease {
        version,
        package: asset(package),
        sha256,
        notes: raw.body.unwrap_or_default(),
        tag: raw.tag_name,
    })
}

/// Liest die Release-Liste des Repos `link` (GitHub-Format). Übergangen werden Entwürfe,
/// Releases ohne Versionsmuster im Tag, ohne oder mit mehreren `*.pcplugin` und solche, deren
/// Dateien nicht unter der Release-Adresse dieses Repos liegen. Fehler (kein JSON, keine Liste)
/// als Text.
pub fn parse_plugin_releases(json: &str, link: &RepoLink) -> Result<Vec<PluginRelease>, String> {
    let value: serde_json::Value = serde_json::from_str(json)
        .map_err(|e| tr!("update-release-unreadable", msg = e.to_string()))?;
    let serde_json::Value::Array(items) = value else {
        return Err(tr!("update-release-not-a-list"));
    };
    Ok(items
        .into_iter()
        .filter_map(|item| serde_json::from_value::<RawRelease>(item).ok())
        .filter_map(|raw| plugin_release(raw, link))
        .collect())
}

/// Der Kanal einer Plug-in-Version: ohne Endung stabil, `beta…`/`rc…` Beta, jede andere Endung
/// Dev.
pub fn version_channel(v: &semver::Version) -> Channel {
    let pre = v.pre.as_str();
    if pre.is_empty() {
        Channel::Stable
    } else if pre.starts_with("beta") || pre.starts_with("rc") {
        Channel::Beta
    } else {
        Channel::Dev
    }
}

/// Zählt `v` zum Kanal `channel` (ein Kanal schließt die stabileren ein)?
pub fn channel_includes(channel: Channel, v: &semver::Version) -> bool {
    match channel {
        Channel::Dev => true,
        Channel::Beta => version_channel(v) != Channel::Dev,
        Channel::Stable => version_channel(v) == Channel::Stable,
    }
}

/// Das neueste Release im Kanal, das höher ist als `installed` (ohne: das neueste im Kanal).
pub fn pick_plugin_release<'a>(
    releases: &'a [PluginRelease],
    installed: Option<&semver::Version>,
    channel: Channel,
) -> Option<&'a PluginRelease> {
    releases
        .iter()
        .filter(|r| channel_includes(channel, &r.version))
        .filter(|r| installed.is_none_or(|i| r.version > *i))
        .max_by(|a, b| a.version.cmp(&b.version))
}

/// Erlaubte Rechner für Plug-in-Downloads: die aus [`crate::install::redirect_target`] und
/// `api.github.com`.
fn plugin_host_allowed(host: &str) -> bool {
    host == "api.github.com" || host_allowed(host)
}

/// Ziel einer Weiterleitung beim Laden für ein Plug-in: nur HTTPS nach `github.com`,
/// `api.github.com` oder `*.githubusercontent.com`, ohne Anmeldedaten und anderen Port;
/// relative Angaben gegen `current` aufgelöst. Sonst der Fehlertext.
pub fn redirect_target(current: &str, location: &str) -> Result<String, String> {
    checked_redirect(current, location, plugin_host_allowed)
}

/// Eine einzelne Antwort ohne Weiterleitung: Status, Kopfzeile `Location` und Inhalt.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hop {
    pub status: u16,
    pub location: Option<String>,
    pub body: Vec<u8>,
}

/// Ein GET mit selbst geprüften Weiterleitungen: `hop(url, max)` holt genau eine Antwort
/// (höchstens `max` Bytes Inhalt); jede 3xx-Antwort wird mit [`redirect_target`] geprüft,
/// höchstens [`MAX_REDIRECTS`] Mal. Die Startadresse muss selbst die Rechner-Regel erfüllen.
/// Ergebnis: Status und Inhalt der letzten Antwort (jeder Status außer 3xx), sonst ein
/// Fehlertext (Netz, Weiterleitung, mehr als `max` Bytes).
pub fn get_following(
    url: &str,
    max: u64,
    hop: &mut dyn FnMut(&str, u64) -> Result<Hop, String>,
) -> Result<(u16, Vec<u8>), String> {
    if !split_origin(url).is_some_and(|(_, host)| plugin_host_allowed(&host)) {
        return Err(tr!("update-detail-foreign-url"));
    }
    let mut current = url.to_string();
    for _ in 0..=MAX_REDIRECTS {
        let answer = hop(&current, max)?;
        if (300..400).contains(&answer.status) {
            let location = answer
                .location
                .ok_or_else(|| tr!("update-detail-redirect-no-target"))?;
            current = redirect_target(&current, &location)?;
            continue;
        }
        if answer.body.len() as u64 > max {
            return Err(tr!("update-net-too-big"));
        }
        return Ok((answer.status, answer.body));
    }
    Err(tr!("update-detail-too-many-redirects"))
}

/// [`get_following`] über das echte Netz ([`crate::net::get_once`], je Anfrage
/// [`REQUEST_TIMEOUT`]). Wartet: nur in einem Arbeitsthread rufen.
pub fn download(url: &str, max: u64) -> Result<(u16, Vec<u8>), String> {
    get_following(url, max, &mut |u, m| {
        crate::net::get_once(u, m, REQUEST_TIMEOUT)
    })
}
