//! Kleine Konfiguration (gewählte Wurzel, Favoriten der Dateiauswahl), ohne neue Abhängigkeit.

use crate::error::{ProjectError, Result};
use crate::fsutil::atomic_write;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Kein `deny_unknown_fields`: ältere Programmstände lesen eine neuere Datei (mit Feldern, die
/// sie nicht kennen) weiter, und neue Felder haben `#[serde(default)]`, damit ältere Dateien
/// ohne sie samt Wurzel geladen werden.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Config {
    pub root: Option<PathBuf>,
    /// Lieblingsordner der Dateiauswahl, in der Reihenfolge des Hinzufügens.
    #[serde(default)]
    pub favorites: Vec<PathBuf>,
}

impl Config {
    /// `%APPDATA%\PlanoraCAD\config.json`, sonst `$XDG_CONFIG_HOME/planora-cad/config.json`,
    /// sonst `~/.config/planora-cad/config.json`.
    pub fn config_path() -> PathBuf {
        if let Some(a) = std::env::var_os("APPDATA") {
            return PathBuf::from(a).join("PlanoraCAD").join("config.json");
        }
        if let Some(x) = std::env::var_os("XDG_CONFIG_HOME") {
            return PathBuf::from(x).join("planora-cad").join("config.json");
        }
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_default();
        home.join(".config").join("planora-cad").join("config.json")
    }

    /// Vorgabe der Wurzel: `Dokumente\PlanoraCAD` im Benutzerordner.
    pub fn default_root() -> PathBuf {
        let home = std::env::var_os("USERPROFILE")
            .or_else(|| std::env::var_os("HOME"))
            .map(PathBuf::from)
            .unwrap_or_default();
        home.join("Documents").join("PlanoraCAD")
    }

    pub fn load() -> Config {
        Config::load_from(&Config::config_path())
    }

    pub fn save(&self) -> Result<()> {
        self.save_to(&Config::config_path())
    }

    /// Fehlende oder unlesbare Dateien ergeben die Vorgabe (kein Fehler).
    pub fn load_from(path: &Path) -> Config {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }

    /// Darf `path` überschrieben werden, ohne etwas zu verlieren? Ja, wenn es die Datei nicht
    /// gibt oder sie sich lesen lässt (eine unlesbare Datei könnte noch die Wurzel enthalten).
    pub fn safe_to_overwrite(path: &Path) -> bool {
        match std::fs::read_to_string(path) {
            Ok(t) => serde_json::from_str::<Config>(&t).is_ok(),
            Err(e) => e.kind() == std::io::ErrorKind::NotFound,
        }
    }

    pub fn save_to(&self, path: &Path) -> Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| ProjectError::io(dir, &e))?;
        }
        let text =
            serde_json::to_string_pretty(self).map_err(|e| ProjectError::Invalid(e.to_string()))?;
        atomic_write(path, text.as_bytes())
    }
}
