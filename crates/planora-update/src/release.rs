//! Release-Liste von GitHub lesen und das passende Update wählen.
//!
//! Gelesen wird die Antwort von `GET /repos/{REPO}/releases` (eine JSON-Liste). Unpassende
//! Einträge werden still ausgelassen; ein Fehler entsteht nur, wenn die Antwort selbst nicht
//! lesbar ist.

use std::fmt;

use serde::Deserialize;

use crate::channel::Channel;
use crate::version::Version;

/// Das Repository, aus dem Updates kommen; fest, nicht einstellbar.
pub const REPO: &str = "KaelanTesseract/PlanoraCAD";

/// Dateiname des Installers im Release.
pub const SETUP_NAME: &str = "planoracad-setup-x64.exe";

/// Dateiname der Prüfsummen-Datei des Installers im Release.
pub const SHA256_NAME: &str = "planoracad-setup-x64.sha256";

/// Dateiname der Lizenzhinweise im Release (optional).
pub const LICENSES_NAME: &str = "THIRD-PARTY-LICENSES.txt";

/// Höchstens so viele Releases erscheinen in den Patchnotes eines Updates.
pub const MAX_NOTES_RELEASES: usize = 20;

/// Höchstens so viele Zeichen Patchnotes insgesamt in einem Update.
pub const MAX_NOTES_CHARS: usize = 20_000;

/// Eine Datei eines Releases.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetRef {
    /// Dateiname.
    pub name: String,
    /// Download-Adresse; immer unter der Release-Adresse von [`REPO`].
    pub url: String,
    /// Größe in Bytes laut GitHub.
    pub size: u64,
}

/// Ein installierbares Release.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    /// Version aus dem Tag.
    pub version: Version,
    /// Tag wie auf GitHub, z. B. `v0.2.0-dev.1`.
    pub tag: String,
    /// Patchnotes (Markdown, Beschreibung des Releases); leer, wenn keine vorhanden.
    pub notes: String,
    /// Installer.
    pub setup: AssetRef,
    /// Prüfsummen-Datei.
    pub sha256: AssetRef,
    /// Lizenzhinweise, falls genau eine passende Datei vorhanden ist.
    pub licenses: Option<AssetRef>,
}

/// Ein angebotenes Update.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Update {
    /// Die angebotene (neueste passende, nicht übersprungene) Version.
    pub latest: Release,
    /// Patchnotes aller passenden Versionen über der laufenden bis einschließlich `latest`,
    /// neueste zuerst; begrenzt auf [`MAX_NOTES_RELEASES`] und [`MAX_NOTES_CHARS`].
    pub notes_between: Vec<(Version, String)>,
    /// Ob Notizen wegen der Obergrenzen weggelassen oder gekürzt wurden.
    pub truncated: bool,
}

/// Fehler beim Lesen der Release-Liste.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReleaseError {
    /// Die Antwort ist kein lesbares JSON.
    Json(String),
    /// Die Antwort ist JSON, aber keine Liste (z. B. Rate-Limit-Meldung von GitHub).
    NotAList,
}

impl fmt::Display for ReleaseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ReleaseError::Json(msg) => write!(f, "Antwort von GitHub nicht lesbar: {msg}"),
            ReleaseError::NotAList => write!(f, "Antwort von GitHub ist keine Release-Liste"),
        }
    }
}

impl std::error::Error for ReleaseError {}

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

/// Liest die Release-Liste im GitHub-Format. Ausgelassen werden Entwürfe, Einträge mit
/// unlesbarer Version oder unpassendem Aufbau und Einträge, die nicht genau eine
/// [`SETUP_NAME`]- und eine [`SHA256_NAME`]-Datei unter der Release-Adresse von [`REPO`]
/// haben. Die Lizenzdatei ist optional; fehlt sie, ist sie doppelt oder liegt sie woanders,
/// bleibt `licenses` leer. Die Reihenfolge der Antwort bleibt erhalten.
pub fn parse_releases(json: &str) -> Result<Vec<Release>, ReleaseError> {
    let value: serde_json::Value =
        serde_json::from_str(json).map_err(|e| ReleaseError::Json(e.to_string()))?;
    let serde_json::Value::Array(items) = value else {
        return Err(ReleaseError::NotAList);
    };
    Ok(items.into_iter().filter_map(release_from).collect())
}

fn release_from(item: serde_json::Value) -> Option<Release> {
    let raw: RawRelease = serde_json::from_value(item).ok()?;
    if raw.draft {
        return None;
    }
    let version = Version::parse(&raw.tag_name).ok()?;
    let setup = single_asset(&raw, SETUP_NAME)?;
    let sha256 = single_asset(&raw, SHA256_NAME)?;
    let licenses = single_asset(&raw, LICENSES_NAME);
    Some(Release {
        version,
        tag: raw.tag_name,
        notes: raw.body.unwrap_or_default(),
        setup,
        sha256,
        licenses,
    })
}

/// Genau eine Datei mit diesem Namen, deren Adresse die Release-Adresse dieses Tags ist.
fn single_asset(raw: &RawRelease, name: &str) -> Option<AssetRef> {
    let mut found = raw.assets.iter().filter(|a| a.name == name);
    let asset = found.next()?;
    if found.next().is_some() {
        return None;
    }
    let expected = format!(
        "https://github.com/{REPO}/releases/download/{}/{name}",
        raw.tag_name
    );
    // Mit dem Testschalter auch eine Datei der lokalen Test-Quelle mit diesem Namen.
    let local = crate::install::local_test_url_ok(&asset.browser_download_url)
        && asset.browser_download_url.ends_with(&format!("/{name}"));
    if asset.browser_download_url != expected && !local {
        return None;
    }
    Some(AssetRef {
        name: asset.name.clone(),
        url: asset.browser_download_url.clone(),
        size: asset.size,
    })
}

/// Wählt das Update für die laufende Version: die neueste Version im Kanal, die höher ist als
/// `running` und nicht `skipped`. Eine übersprungene Version blockiert nur sich selbst.
pub fn pick_update(
    releases: &[Release],
    running: &Version,
    channel: Channel,
    skipped: Option<&Version>,
) -> Option<Update> {
    let mut candidates: Vec<&Release> = releases
        .iter()
        .filter(|r| channel.includes(&r.version) && r.version > *running)
        .collect();
    // Neueste zuerst; gleiche Versionen (z. B. Tags mit und ohne `v`) nur einmal.
    candidates.sort_by_key(|r| std::cmp::Reverse(r.version));
    candidates.dedup_by(|a, b| a.version == b.version);

    let latest_index = candidates
        .iter()
        .position(|r| Some(&r.version) != skipped)?;
    let latest = candidates[latest_index].clone();

    let mut notes_between = Vec::new();
    let mut truncated = false;
    let mut used = 0usize;
    for release in &candidates[latest_index..] {
        if notes_between.len() == MAX_NOTES_RELEASES || used == MAX_NOTES_CHARS {
            truncated = true;
            break;
        }
        let remaining = MAX_NOTES_CHARS - used;
        match release.notes.char_indices().nth(remaining) {
            // Länger als der Rest: an einer Zeichengrenze kürzen und aufhören.
            Some((cut, _)) => {
                notes_between.push((release.version, release.notes[..cut].to_string()));
                truncated = true;
                break;
            }
            None => {
                used += release.notes.chars().count();
                notes_between.push((release.version, release.notes.clone()));
            }
        }
    }

    Some(Update {
        latest,
        notes_between,
        truncated,
    })
}
