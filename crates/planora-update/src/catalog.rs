//! Plug-in-Katalog: der Katalog-Release des festen Katalog-Repos und seine `katalog.json`
//! lesen und prüfen. Ohne Netz; das Laden übernimmt das Programm. Je Eintrag wird
//! nachsichtig gelesen: ein fehlerhafter Eintrag wird übergangen (mit Grund fürs Protokoll),
//! der Rest bleibt.

use std::collections::{BTreeMap, HashSet};

use planora_i18n::tr;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::plugin_source::{RepoLink, asset_url_ok, parse_repo_link};
use crate::release::AssetRef;

/// Besitzer des festen Katalog-Repos.
pub const CATALOG_OWNER: &str = "KaelanTesseract";
/// Name des festen Katalog-Repos.
pub const CATALOG_REPO: &str = "PlanoraCAD-Plugins";
/// Größte erlaubte `katalog.json`.
pub const MAX_CATALOG_BYTES: u64 = 1 << 20;
/// Größtes erlaubtes Symbol.
pub const MAX_SYMBOL_BYTES: u64 = 256 << 10;
/// Höchstens so viele Symbole je Katalog-Release.
pub const MAX_SYMBOLS: usize = 100;
/// Höchstens so viele Einträge werden übernommen.
pub const MAX_ENTRIES: usize = 500;
/// Das Format der `katalog.json`, das dieses Programm versteht.
pub const CATALOG_FORMAT: u32 = 1;

const CATALOG_ASSET: &str = "katalog.json";
/// Längste erlaubte Plug-in-Kennung (wie in der Manifest-Regel des Plug-in-Hosts).
const MAX_ID_LEN: usize = 64;

/// Adresse des neuesten Katalog-Releases.
pub fn latest_release_url() -> String {
    format!("https://api.github.com/repos/{CATALOG_OWNER}/{CATALOG_REPO}/releases/latest")
}

/// Der Katalog-Release: Tag, `katalog.json` und die brauchbaren Symbol-Dateien.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogRelease {
    pub tag: String,
    pub catalog: AssetRef,
    pub symbols: Vec<AssetRef>,
}

#[derive(Deserialize)]
struct RawRelease {
    tag_name: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    assets: Vec<RawAsset>,
}

#[derive(Deserialize)]
struct RawAsset {
    name: String,
    browser_download_url: String,
    size: u64,
}

fn has_ext(name: &str, ext: &str) -> bool {
    name.len() > ext.len()
        && name
            .get(name.len() - ext.len()..)
            .is_some_and(|e| e.eq_ignore_ascii_case(ext))
}

/// Liest die Antwort auf [`latest_release_url`]. Pflicht ist das Asset `katalog.json`;
/// Symbole sind die übrigen `.png`/`.svg`-Assets bis [`MAX_SYMBOL_BYTES`], höchstens
/// [`MAX_SYMBOLS`]. Assets, deren Adresse nicht genau unter dem Release des Katalog-Repos
/// liegt, werden übergangen. Ein Entwurf ist ein Fehler.
pub fn parse_latest_release(json: &str) -> Result<CatalogRelease, String> {
    let raw: RawRelease = serde_json::from_str(json)
        .map_err(|e| tr!("update-release-unreadable", msg = e.to_string()))?;
    if raw.draft {
        return Err(tr!("catalog-release-draft"));
    }
    if raw.tag_name.is_empty() || raw.tag_name.contains(['/', '\\', '?', '#']) {
        return Err(tr!("catalog-release-no-catalog"));
    }
    let link = RepoLink {
        owner: CATALOG_OWNER.to_string(),
        repo: CATALOG_REPO.to_string(),
    };
    let tag = raw.tag_name;
    let to_ref = |a: &RawAsset| AssetRef {
        name: a.name.clone(),
        url: a.browser_download_url.clone(),
        size: a.size,
    };
    let mut catalog = None;
    let mut symbols = Vec::new();
    for a in &raw.assets {
        if !asset_url_ok(&link, &tag, &a.name, &a.browser_download_url) {
            continue;
        }
        if a.name == CATALOG_ASSET {
            if catalog.is_none() {
                catalog = Some(to_ref(a));
            }
        } else if (has_ext(&a.name, ".png") || has_ext(&a.name, ".svg"))
            && a.size <= MAX_SYMBOL_BYTES
            && symbols.len() < MAX_SYMBOLS
        {
            symbols.push(to_ref(a));
        }
    }
    let catalog = catalog.ok_or_else(|| tr!("catalog-release-no-catalog"))?;
    Ok(CatalogRelease {
        tag,
        catalog,
        symbols,
    })
}

/// Ein Eintrag des Katalogs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogEntry {
    pub id: String,
    /// Name je Sprache.
    pub name: BTreeMap<String, String>,
    /// Beschreibung je Sprache.
    pub description: BTreeMap<String, String>,
    /// Freier Bezeichner der Kategorie (leer, wenn keine genannt ist).
    pub category: String,
    pub repo: RepoLink,
    /// Dateiname eines Symbols im selben Katalog-Release.
    pub symbol: Option<String>,
}

fn pick<'a>(map: &'a BTreeMap<String, String>, lang: &str) -> &'a str {
    map.get(lang)
        .or_else(|| map.get("en"))
        .or_else(|| map.values().next())
        .map_or("", String::as_str)
}

impl CatalogEntry {
    /// Name in `lang`, sonst Englisch, sonst der erste vorhandene, sonst leer.
    pub fn name_in(&self, lang: &str) -> &str {
        pick(&self.name, lang)
    }

    /// Beschreibung in `lang`, sonst Englisch, sonst die erste vorhandene, sonst leer.
    pub fn description_in(&self, lang: &str) -> &str {
        pick(&self.description, lang)
    }
}

/// Fehler beim Lesen der `katalog.json` als Ganzes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CatalogError {
    /// Das `format` ist neuer, als dieses Programm versteht.
    TooNew(u32),
    /// Kein lesbarer Katalog (Text für das Protokoll).
    Invalid(String),
}

impl std::fmt::Display for CatalogError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CatalogError::TooNew(_) => f.write_str(&tr!("catalog-too-new")),
            CatalogError::Invalid(msg) => f.write_str(msg),
        }
    }
}

/// Ist `id` eine gültige Plug-in-Kennung (`^[a-z0-9]+(\.[a-z0-9-]+)+$`, höchstens 64 Zeichen)?
/// Dieselbe Regel wie `valid_id` im Plug-in-Host; hier bewusst nachgebaut, weil
/// `planora-update` nicht vom Host-Crate abhängen darf.
fn valid_id(id: &str) -> bool {
    if id.is_empty() || id.len() > MAX_ID_LEN {
        return false;
    }
    let plain = |c: char| c.is_ascii_lowercase() || c.is_ascii_digit();
    let mut parts = id.split('.');
    let first = parts.next().unwrap_or("");
    if first.is_empty() || !first.chars().all(plain) {
        return false;
    }
    let mut rest = 0;
    for part in parts {
        rest += 1;
        if part.is_empty() || !part.chars().all(|c| plain(c) || c == '-') {
            return false;
        }
    }
    rest > 0
}

/// Ein Symbol-Name muss ein einfacher Dateiname mit `.png` oder `.svg` sein.
fn symbol_ok(name: &str) -> bool {
    !name.is_empty()
        && !name.contains(['/', '\\', '?', '#', '@', ':'])
        && !name.contains("..")
        && (has_ext(name, ".png") || has_ext(name, ".svg"))
}

/// Texte je Sprache; Nicht-Texte und leere Texte fallen weg.
fn texts(v: Option<&Value>) -> BTreeMap<String, String> {
    let Some(Value::Object(map)) = v else {
        return BTreeMap::new();
    };
    map.iter()
        .filter_map(|(k, v)| match v {
            Value::String(s) if !s.trim().is_empty() => Some((k.clone(), s.clone())),
            _ => None,
        })
        .collect()
}

/// Liest einen Eintrag; `Err` ist der Grund, ihn zu übergehen.
fn parse_entry(v: &Value, seen: &HashSet<String>) -> Result<CatalogEntry, String> {
    let Value::Object(o) = v else {
        return Err(tr!("catalog-skip-not-object"));
    };
    let id = match o.get("id") {
        Some(Value::String(s)) if valid_id(s) => s.clone(),
        Some(Value::String(s)) => return Err(tr!("catalog-skip-bad-id", id = s.clone())),
        _ => return Err(tr!("catalog-skip-no-id")),
    };
    if seen.contains(&id) {
        return Err(tr!("catalog-skip-duplicate", id = id));
    }
    let repo = match o.get("repo") {
        Some(Value::String(s)) => parse_repo_link(s),
        _ => Err(tr!("plugin-link-invalid")),
    }
    .map_err(|e| tr!("catalog-skip-bad-repo", id = id.clone(), msg = e))?;
    let symbol = match o.get("symbol") {
        Some(Value::String(s)) if symbol_ok(s) => Some(s.clone()),
        _ => None,
    };
    let category = match o.get("kategorie") {
        Some(Value::String(s)) => s.trim().to_string(),
        _ => String::new(),
    };
    Ok(CatalogEntry {
        id,
        name: texts(o.get("name")),
        description: texts(o.get("beschreibung")),
        category,
        repo,
        symbol,
    })
}

/// Liest die `katalog.json`. Ergebnis: die gültigen Einträge (höchstens [`MAX_ENTRIES`]) und
/// die Gründe der übergangenen Einträge fürs Protokoll. Unbekannte Felder werden ignoriert.
pub fn parse_catalog(json: &str) -> Result<(Vec<CatalogEntry>, Vec<String>), CatalogError> {
    let invalid = |msg: String| CatalogError::Invalid(msg);
    let root: Value = serde_json::from_str(json)
        .map_err(|e| invalid(tr!("update-release-unreadable", msg = e.to_string())))?;
    let Value::Object(root) = root else {
        return Err(invalid(tr!("catalog-invalid")));
    };
    let format = root
        .get("format")
        .and_then(Value::as_u64)
        .ok_or_else(|| invalid(tr!("catalog-invalid")))?;
    if format > u64::from(CATALOG_FORMAT) {
        return Err(CatalogError::TooNew(
            u32::try_from(format).unwrap_or(u32::MAX),
        ));
    }
    let Some(Value::Array(list)) = root.get("eintraege") else {
        return Err(invalid(tr!("catalog-invalid")));
    };
    let mut entries = Vec::new();
    let mut skipped = Vec::new();
    let mut seen = HashSet::new();
    let mut too_many = false;
    for v in list {
        match parse_entry(v, &seen) {
            Ok(_) if entries.len() >= MAX_ENTRIES => too_many = true,
            Ok(e) => {
                seen.insert(e.id.clone());
                entries.push(e);
            }
            Err(reason) => skipped.push(reason),
        }
    }
    if too_many {
        skipped.push(tr!("catalog-skip-too-many", max = MAX_ENTRIES));
    }
    Ok((entries, skipped))
}

/// Verzeichnis des Zwischenspeichers (`stand.json`): wann geholt wurde, die Prüfsumme der
/// gespeicherten `katalog.json` und welche Datei zu welchem Symbol gehört. Die Dateinamen kommen
/// nie aus dem Katalog, sondern aus [`CacheIndex::symbol_file`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CacheIndex {
    /// Zeitpunkt des Holens (Sekunden seit 1970).
    pub fetched_at: u64,
    /// SHA-256 (hex) der gespeicherten `katalog.json`; passt sie nicht, gilt der Stand als kaputt.
    pub catalog_sha256: String,
    /// Symbolname aus dem Katalog → Dateiname im Zwischenspeicher.
    pub symbols: BTreeMap<String, String>,
}

impl CacheIndex {
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }

    /// Liest `stand.json`; unlesbar ⇒ `None`.
    pub fn parse(text: &str) -> Option<CacheIndex> {
        serde_json::from_str(text).ok()
    }

    /// Dateiname für das Symbol `name`: SHA-256 des Namens (hex) plus `.svg` oder `.png`.
    pub fn symbol_file(name: &str) -> String {
        let ext = if has_ext(name, ".svg") {
            ".svg"
        } else {
            ".png"
        };
        format!("{}{ext}", crate::sha256::hex_digest(name.as_bytes()))
    }

    /// Hat `file` die Form aus [`CacheIndex::symbol_file`] (64 Hex-Zeichen plus `.png`/`.svg`)?
    pub fn is_symbol_file(file: &str) -> bool {
        let Some(stem) = file
            .strip_suffix(".png")
            .or_else(|| file.strip_suffix(".svg"))
        else {
            return false;
        };
        stem.len() == 64
            && stem
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    }
}
