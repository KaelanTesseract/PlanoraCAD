//! Plug-ins aus einem GitHub-Repo (Plug-ins Etappe 1, Task 8): die Arbeitsschritte der Aufträge
//! zum Installieren und Aktualisieren über den Repo-Link. Alles hier wartet auf das Netz und
//! läuft nur im Arbeitsthread eines [`crate::jobs::Job`]; der Netzweg wird übergeben
//! ([`crate::plugins::Plugins::repo_fetch`], in Tests eine Nachbildung).
//!
//! Ablauf: Release-Liste holen → passendes Release wählen → Paket (und gegebenenfalls die
//! Prüfsummendatei) laden → Prüfsumme prüfen → [`Package::read`] → [`check_downloaded`]. Auf die
//! Platte kommt dabei nichts; das tut erst „Installieren“ bzw. „Aktualisieren“.

use planora_i18n::tr;
use planora_plugin_host::package::{MAX_PACKAGE, Package, PackageError};
use planora_update::Channel;
use planora_update::install::checksum_hex;
use planora_update::plugin_source::{
    MAX_CHECKSUM_BYTES, MAX_RELEASES_BYTES, PluginRelease, RepoLink, parse_plugin_releases,
    pick_plugin_release,
};
use planora_update::sha256::hex_digest;

/// Ein GET mit selbst geprüften Weiterleitungen und Größengrenze: (Status, Inhalt) der letzten
/// Antwort oder ein Fehlertext ([`planora_update::plugin_source::download`]).
pub type RepoFetch = fn(&str, u64) -> Result<(u16, Vec<u8>), String>;

/// Ein geholtes Paket, bereit für den Installationsdialog.
#[derive(Clone, Debug)]
pub struct Fetched {
    pub link: RepoLink,
    pub release: PluginRelease,
    pub package: Package,
}

/// Fehlertext für ein Problem mit dem Netz.
fn network(detail: &str) -> String {
    tr!("plugin-link-network", detail = detail)
}

/// Lädt `url` (höchstens `max` Bytes); nur eine Antwort 2xx zählt.
fn get(fetch: RepoFetch, url: &str, max: u64) -> Result<Vec<u8>, String> {
    let (status, body) = fetch(url, max).map_err(|e| network(&e))?;
    if !(200..300).contains(&status) {
        return Err(network(&tr!(
            "plugin-link-status",
            status = status.to_string()
        )));
    }
    Ok(body)
}

/// Die Releases des Repos mit genau einem Paket.
pub fn fetch_releases(fetch: RepoFetch, link: &RepoLink) -> Result<Vec<PluginRelease>, String> {
    let body = get(fetch, &link.releases_url(), MAX_RELEASES_BYTES)?;
    let text = String::from_utf8_lossy(&body);
    parse_plugin_releases(&text, link).map_err(|e| network(&e))
}

/// Prüft ein geladenes Paket gegen sein Release: der Tag ohne `v` muss die Version im Manifest
/// sein, und beim Aktualisieren (`installed_id`) muss es dasselbe Plug-in sein.
pub fn check_downloaded(
    pkg: &Package,
    release: &PluginRelease,
    installed_id: Option<&str>,
) -> Result<(), String> {
    let m = &pkg.manifest;
    let tag = release.tag.strip_prefix('v').unwrap_or(&release.tag);
    if tag != m.version.to_string() {
        return Err(tr!(
            "plugin-link-mismatch",
            tag = release.tag.as_str(),
            version = m.version.to_string()
        ));
    }
    if let Some(id) = installed_id
        && m.id != id
    {
        return Err(tr!(
            "plugin-link-foreign-id",
            tag = release.tag.as_str(),
            id = m.id.as_str()
        ));
    }
    Ok(())
}

/// Lädt das Paket des Releases (mit Prüfsumme, falls das Release eine hat), liest und prüft es.
pub fn fetch_package(
    fetch: RepoFetch,
    release: &PluginRelease,
    installed_id: Option<&str>,
) -> Result<Package, String> {
    let unusable = |e: String| tr!("plugin-link-package", error = e);
    if release.package.size > MAX_PACKAGE {
        return Err(unusable(PackageError::TooLarge.to_string()));
    }
    let data = get(fetch, &release.package.url, MAX_PACKAGE)?;
    if let Some(sum) = &release.sha256 {
        let text = get(fetch, &sum.url, MAX_CHECKSUM_BYTES)?;
        let expected = checksum_hex(&String::from_utf8_lossy(&text), &release.package.name)
            .map_err(|_| tr!("plugin-link-checksum"))?;
        if hex_digest(&data) != expected {
            return Err(tr!("plugin-link-checksum"));
        }
    }
    let pkg = Package::read(&data).map_err(|e| unusable(e.to_string()))?;
    check_downloaded(&pkg, release, installed_id)?;
    Ok(pkg)
}

/// Auftrag „Holen“ im Installationsdialog: das neueste Release im Kanal samt geprüftem Paket.
pub fn fetch_for_install(
    fetch: RepoFetch,
    link: RepoLink,
    channel: Channel,
) -> Result<Fetched, String> {
    let releases = fetch_releases(fetch, &link)?;
    let release = pick_plugin_release(&releases, None, channel)
        .cloned()
        .ok_or_else(|| tr!("plugin-link-no-release"))?;
    let package = fetch_package(fetch, &release, None)?;
    Ok(Fetched {
        link,
        release,
        package,
    })
}

/// Auftrag „Nach Updates suchen“ für ein Plug-in: ein Release im Kanal über `installed`, oder
/// keines.
pub fn check_release(
    fetch: RepoFetch,
    link: &RepoLink,
    installed: &semver::Version,
    channel: Channel,
) -> Result<Option<PluginRelease>, String> {
    let releases = fetch_releases(fetch, link)?;
    Ok(pick_plugin_release(&releases, Some(installed), channel).cloned())
}
