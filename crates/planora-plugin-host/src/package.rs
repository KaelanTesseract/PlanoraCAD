//! Ein Plug-in-Paket (ZIP): `plugin.toml`, `plugin.wasm`, `languages/<id>.ftl`,
//! `icons/<name>`, `signature`. Gelesen wird mit den Grenzen des ZIP-Lesers; unbekannte Einträge
//! werden übergangen.

use crate::manifest::{Manifest, ManifestError};
use planora_exchange::zip_read::{ZipRead, ZipReadError, ZipReadLimits};
use planora_i18n::tr;
use std::collections::BTreeMap;
use std::io;
use std::path::Path;

/// Größe eines Pakets (die ZIP-Datei selbst und alle Einträge zusammen entpackt).
pub const MAX_PACKAGE: u64 = 64 << 20;
/// Größe von `plugin.wasm` (und jedes anderen Eintrags) entpackt.
pub const MAX_WASM: u64 = 32 << 20;
/// Höchstzahl der Einträge einer Paketdatei.
pub const MAX_ENTRIES: usize = 1000;

const MANIFEST_NAME: &str = "plugin.toml";
const WASM_NAME: &str = "plugin.wasm";
const SIGNATURE_NAME: &str = "signature";
const LANGUAGES_DIR: &str = "languages/";
const ICONS_DIR: &str = "icons/";

/// Ob das Paket eine Signatur mitbringt (geprüft wird sie erst später).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Signature {
    None,
    Present,
}

/// Warum ein Paket nicht gelesen wurde (Meldungen in der Sprache des Programms).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PackageError {
    #[error("{}", tr!("plugin-package-too-large", max_mib = (MAX_PACKAGE >> 20).to_string()))]
    TooLarge,
    #[error("{}", tr!("plugin-package-zip", error = .0.to_string()))]
    Zip(#[from] ZipReadError),
    #[error("{}", tr!("plugin-package-missing", file = .0))]
    Missing(String),
    #[error("{0}")]
    Manifest(#[from] ManifestError),
    #[error("{}", tr!("plugin-package-not-utf8", file = .0))]
    NotUtf8(String),
}

/// Der Inhalt eines gelesenen Pakets.
#[derive(Clone, Debug)]
pub struct Package {
    pub manifest: Manifest,
    pub wasm: Vec<u8>,
    /// Sprach-Id → Text der Fluent-Datei.
    pub languages: BTreeMap<String, String>,
    /// Dateiname → Inhalt.
    pub icons: BTreeMap<String, Vec<u8>>,
    pub signature: Signature,
    /// Urtext von `plugin.toml` und Inhalt von `signature` fürs Auspacken.
    manifest_text: String,
    signature_bytes: Vec<u8>,
}

/// Ein Name direkt unter `dir` (ohne weitere Ordner, nicht leer).
fn direct_child<'a>(name: &'a str, dir: &str) -> Option<&'a str> {
    let rest = name.strip_prefix(dir)?;
    (!rest.is_empty() && !rest.contains('/')).then_some(rest)
}

impl Package {
    /// Liest ein Paket aus den Bytes der ZIP-Datei.
    pub fn read(data: &[u8]) -> Result<Package, PackageError> {
        if data.len() as u64 > MAX_PACKAGE {
            return Err(PackageError::TooLarge);
        }
        let limits = ZipReadLimits {
            max_entries: MAX_ENTRIES,
            max_entry: MAX_WASM,
            max_total: MAX_PACKAGE,
        };
        let mut zip = ZipRead::open(data, &limits)?;
        let mut manifest_text = None;
        let mut wasm = None;
        let mut signature_bytes = None;
        let mut languages = BTreeMap::new();
        let mut icons = BTreeMap::new();
        for i in 0..zip.len() {
            let name = zip.name(i).to_string();
            if name == MANIFEST_NAME {
                let text = String::from_utf8(zip.read(i)?)
                    .map_err(|_| PackageError::NotUtf8(name.clone()))?;
                manifest_text = Some(text);
            } else if name == WASM_NAME {
                wasm = Some(zip.read(i)?);
            } else if name == SIGNATURE_NAME {
                signature_bytes = Some(zip.read(i)?);
            } else if let Some(id) =
                direct_child(&name, LANGUAGES_DIR).and_then(|f| f.strip_suffix(".ftl"))
                && !id.is_empty()
            {
                let text = String::from_utf8(zip.read(i)?)
                    .map_err(|_| PackageError::NotUtf8(name.clone()))?;
                languages.insert(id.to_string(), text);
            } else if let Some(file) = direct_child(&name, ICONS_DIR) {
                icons.insert(file.to_string(), zip.read(i)?);
            }
        }
        let manifest_text =
            manifest_text.ok_or_else(|| PackageError::Missing(MANIFEST_NAME.to_string()))?;
        let wasm = wasm.ok_or_else(|| PackageError::Missing(WASM_NAME.to_string()))?;
        let manifest = Manifest::parse(&manifest_text)?;
        Ok(Package {
            manifest,
            wasm,
            languages,
            icons,
            signature: if signature_bytes.is_some() {
                Signature::Present
            } else {
                Signature::None
            },
            manifest_text,
            signature_bytes: signature_bytes.unwrap_or_default(),
        })
    }

    /// Schreibt genau die bekannten Dateien (`plugin.toml`, `plugin.wasm`, `languages/*`,
    /// `icons/*`, `signature`) in den Ordner `dir`; unbekannte Einträge wurden schon beim Lesen
    /// übergangen.
    pub fn unpack_to(&self, dir: &Path) -> io::Result<()> {
        std::fs::create_dir_all(dir)?;
        std::fs::write(dir.join(MANIFEST_NAME), &self.manifest_text)?;
        std::fs::write(dir.join(WASM_NAME), &self.wasm)?;
        if self.signature == Signature::Present {
            std::fs::write(dir.join(SIGNATURE_NAME), &self.signature_bytes)?;
        }
        if !self.languages.is_empty() {
            let langs = dir.join("languages");
            std::fs::create_dir_all(&langs)?;
            for (id, text) in &self.languages {
                std::fs::write(langs.join(format!("{id}.ftl")), text)?;
            }
        }
        if !self.icons.is_empty() {
            let icons = dir.join("icons");
            std::fs::create_dir_all(&icons)?;
            for (name, bytes) in &self.icons {
                std::fs::write(icons.join(name), bytes)?;
            }
        }
        Ok(())
    }
}
