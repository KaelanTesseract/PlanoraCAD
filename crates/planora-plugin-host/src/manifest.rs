//! Das Manifest `plugin.toml` eines Plug-in-Pakets: Kennung, Version, gewünschte Rechte.
//!
//! Unbekannte Schlüssel werden bewusst nicht abgelehnt, damit neuere Pakete lesbar bleiben.

use planora_i18n::tr;
use serde::Deserialize;
use std::collections::BTreeSet;

/// Ein Recht, das ein Plug-in beantragen kann.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Right {
    /// Zugriff auf die Rechner in `net-hosts`.
    Net,
    /// Zugriff auf einen vom Nutzer gewählten Ordner.
    FsFolder,
    /// Das Modell lesen.
    ModelRead,
    /// Das Modell ändern.
    ModelWrite,
}

impl Right {
    /// Die Kennung im Manifest.
    pub fn id(self) -> &'static str {
        match self {
            Right::Net => "net",
            Right::FsFolder => "fs-folder",
            Right::ModelRead => "model-read",
            Right::ModelWrite => "model-write",
        }
    }

    /// Das Recht zu einer Kennung aus dem Manifest oder der Konfiguration.
    pub fn from_id(id: &str) -> Option<Right> {
        [
            Right::Net,
            Right::FsFolder,
            Right::ModelRead,
            Right::ModelWrite,
        ]
        .into_iter()
        .find(|r| r.id() == id)
    }

    /// Setzt diese Programmversion das Recht schon um? Nur `net`; die übrigen (`fs-folder`,
    /// `model-read`, `model-write`) lehnt der Host ohne Nachfrage ab, der Installationsdialog
    /// zeigt sie als „noch nicht verfügbar“.
    pub fn available(self) -> bool {
        self == Right::Net
    }
}

/// Warum ein Manifest nicht gelesen wurde (Meldungen in der Sprache des Programms).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ManifestError {
    Toml(String),
    BadId(String),
    BadVersion(String),
    UnknownRight(String),
    NetWithoutHosts,
    HostsWithoutNet,
    BadHost(String),
    BadMemory(u32),
    Empty(&'static str),
}

impl std::fmt::Display for ManifestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let text = match self {
            ManifestError::Toml(e) => tr!("plugin-manifest-toml", error = e),
            ManifestError::BadId(id) => tr!("plugin-manifest-bad-id", id = id),
            ManifestError::BadVersion(v) => tr!("plugin-manifest-bad-version", version = v),
            ManifestError::UnknownRight(r) => tr!("plugin-manifest-unknown-right", right = r),
            ManifestError::NetWithoutHosts => tr!("plugin-manifest-net-without-hosts"),
            ManifestError::HostsWithoutNet => tr!("plugin-manifest-hosts-without-net"),
            ManifestError::BadHost(h) => tr!("plugin-manifest-bad-host", host = h),
            ManifestError::BadMemory(m) => {
                tr!("plugin-manifest-bad-memory", value = m.to_string())
            }
            ManifestError::Empty(field) => tr!("plugin-manifest-empty", field = *field),
        };
        f.write_str(&text)
    }
}

impl std::error::Error for ManifestError {}

/// Der Arbeitsspeicher, wenn das Manifest nichts angibt (in MiB).
const DEFAULT_MEMORY_MB: u32 = 512;
/// Größter erlaubter Arbeitsspeicher (in MiB).
const MAX_MEMORY_MB: u32 = 2048;
/// Längste Kennung.
const MAX_ID_LEN: usize = 64;

/// Der Inhalt von `plugin.toml`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Manifest {
    pub id: String,
    pub name: String,
    pub version: semver::Version,
    pub api: String,
    pub author: String,
    pub license: String,
    pub description: String,
    pub rights: BTreeSet<Right>,
    pub net_hosts: Vec<String>,
    pub memory_mb: u32,
}

#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
struct Raw {
    id: String,
    name: String,
    version: String,
    api: String,
    #[serde(default)]
    author: String,
    license: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    rights: Vec<String>,
    #[serde(default)]
    net_hosts: Vec<String>,
    #[serde(default = "default_memory")]
    memory_mb: u32,
}

fn default_memory() -> u32 {
    DEFAULT_MEMORY_MB
}

/// Muster `^[a-z0-9]+(\.[a-z0-9-]+)+$`, höchstens 64 Zeichen.
fn valid_id(id: &str) -> bool {
    if id.is_empty() || id.len() > MAX_ID_LEN {
        return false;
    }
    let mut parts = id.split('.');
    let first = parts.next().unwrap_or("");
    let plain = |c: char| c.is_ascii_lowercase() || c.is_ascii_digit();
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

/// Nur ein Rechnername: Buchstaben, Ziffern, Punkte, Bindestriche; keine leeren Teile. Keine
/// Zahlen-Adressen: das letzte Label nicht nur aus Ziffern, kein `0x…` (Hex-Schreibweise einer
/// IP-Adresse), nicht nur Ziffern und Punkte.
fn valid_host(host: &str) -> bool {
    let digits = |s: &str| s.chars().all(|c| c.is_ascii_digit());
    let numeric = host.rsplit('.').next().is_some_and(digits)
        || host.to_ascii_lowercase().starts_with("0x")
        || host.chars().all(|c| c.is_ascii_digit() || c == '.');
    !numeric
        && !host.is_empty()
        && host.len() <= 253
        && host
            .split('.')
            .all(|l| !l.is_empty() && !l.starts_with('-') && !l.ends_with('-'))
        && host
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
}

impl Manifest {
    /// Liest und prüft den Text von `plugin.toml`.
    pub fn parse(text: &str) -> Result<Manifest, ManifestError> {
        let raw: Raw = toml::from_str(text).map_err(|e| ManifestError::Toml(e.to_string()))?;
        if !valid_id(&raw.id) {
            return Err(ManifestError::BadId(raw.id));
        }
        if raw.name.trim().is_empty() {
            return Err(ManifestError::Empty("name"));
        }
        let version = semver::Version::parse(&raw.version)
            .map_err(|_| ManifestError::BadVersion(raw.version.clone()))?;
        if raw.api.trim().is_empty() {
            return Err(ManifestError::Empty("api"));
        }
        if raw.license.trim().is_empty() {
            return Err(ManifestError::Empty("license"));
        }
        let mut rights = BTreeSet::new();
        for id in &raw.rights {
            rights
                .insert(Right::from_id(id).ok_or_else(|| ManifestError::UnknownRight(id.clone()))?);
        }
        let has_net = rights.contains(&Right::Net);
        if has_net && raw.net_hosts.is_empty() {
            return Err(ManifestError::NetWithoutHosts);
        }
        if !has_net && !raw.net_hosts.is_empty() {
            return Err(ManifestError::HostsWithoutNet);
        }
        if let Some(bad) = raw.net_hosts.iter().find(|h| !valid_host(h)) {
            return Err(ManifestError::BadHost(bad.clone()));
        }
        if raw.memory_mb == 0 || raw.memory_mb > MAX_MEMORY_MB {
            return Err(ManifestError::BadMemory(raw.memory_mb));
        }
        Ok(Manifest {
            id: raw.id,
            name: raw.name,
            version,
            api: raw.api,
            author: raw.author,
            license: raw.license,
            description: raw.description,
            rights,
            net_hosts: raw.net_hosts,
            memory_mb: raw.memory_mb,
        })
    }
}
