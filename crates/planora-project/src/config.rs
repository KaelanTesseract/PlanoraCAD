//! Kleine Konfiguration (gewählte Wurzel, Favoriten der Dateiauswahl, Durchsichtigkeit der
//! Panels, Abweichungen der Tastenbelegung), ohne neue Abhängigkeit.

use crate::error::{ProjectError, Result};
use crate::fsutil::atomic_write_unique;
use serde::{Deserialize, Deserializer, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Höchstzahl der Favoriten.
pub const MAX_FAVORITES: usize = 24;

/// Voreinstellung der Durchsichtigkeit der Panels in Prozent.
pub const DEFAULT_PANEL_TRANSPARENCY: u8 = 40;

/// Höchste Durchsichtigkeit der Panels in Prozent (mehr macht die Schrift unlesbar).
pub const MAX_PANEL_TRANSPARENCY: u8 = 80;

/// Begrenzt die Durchsichtigkeit auf 0 bis [`MAX_PANEL_TRANSPARENCY`].
pub fn clamp_panel_transparency(percent: u8) -> u8 {
    percent.min(MAX_PANEL_TRANSPARENCY)
}

/// Kein `deny_unknown_fields`: ältere Programmstände lesen eine neuere Datei (mit Feldern, die
/// sie nicht kennen) weiter, und neue Felder haben `#[serde(default)]`, damit ältere Dateien
/// ohne sie samt Wurzel geladen werden.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Config {
    pub root: Option<PathBuf>,
    /// Lieblingsordner der Dateiauswahl, in der Reihenfolge des Hinzufügens. Nachsichtig
    /// gelesen: ein kaputtes Feld kostet nie die Wurzel (siehe [`lenient_favorites`]).
    #[serde(default, deserialize_with = "lenient_favorites")]
    pub favorites: Vec<PathBuf>,
    /// Durchsichtigkeit der Panels in Prozent (Einstellungen → Darstellung). Fehlt das Feld
    /// (ältere Datei), gilt [`DEFAULT_PANEL_TRANSPARENCY`]; gelesen wird nachsichtig und auf 0
    /// bis [`MAX_PANEL_TRANSPARENCY`] begrenzt (siehe [`lenient_transparency`]).
    #[serde(
        default = "default_panel_transparency",
        deserialize_with = "lenient_transparency"
    )]
    pub panel_transparency: u8,
    /// Abweichungen der Tastenbelegung von der Voreinstellung: Id der Aktion → Belegungen als
    /// Text (eine leere Liste heißt „ohne Belegung“). Geprüft wird erst beim Aufbau der
    /// Belegung im Programm; hier nur nachsichtig gelesen (siehe [`lenient_keys`]). Ohne
    /// Abweichungen wird der Abschnitt nicht geschrieben.
    #[serde(
        default,
        skip_serializing_if = "BTreeMap::is_empty",
        deserialize_with = "lenient_keys"
    )]
    pub keys: BTreeMap<String, Vec<String>>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            root: None,
            favorites: Vec::new(),
            panel_transparency: DEFAULT_PANEL_TRANSPARENCY,
            keys: BTreeMap::new(),
        }
    }
}

/// Liest `keys` aus jedem JSON-Wert: Ist es kein Objekt, gilt es als leer. Einträge, die keine
/// Texte sind (eine Zahl in der Liste oder ein Wert statt einer Liste), bleiben als ihr
/// JSON-Text erhalten, damit die Tastenbelegung sie als ungültig melden kann, statt dass sie
/// still verschwinden.
fn lenient_keys<'de, D: Deserializer<'de>>(
    d: D,
) -> std::result::Result<BTreeMap<String, Vec<String>>, D::Error> {
    let text = |v: &serde_json::Value| v.as_str().map_or_else(|| v.to_string(), String::from);
    let value = serde_json::Value::deserialize(d)?;
    Ok(value
        .as_object()
        .map(|o| {
            o.iter()
                .map(|(id, v)| {
                    let list = v
                        .as_array()
                        .map_or_else(|| vec![v.to_string()], |a| a.iter().map(text).collect());
                    (id.clone(), list)
                })
                .collect()
        })
        .unwrap_or_default())
}

fn default_panel_transparency() -> u8 {
    DEFAULT_PANEL_TRANSPARENCY
}

/// Liest `panel_transparency` aus jedem JSON-Wert: Zahlen werden gerundet und auf 0 bis
/// [`MAX_PANEL_TRANSPARENCY`] begrenzt, alles andere ergibt die Voreinstellung (die Datei bleibt
/// lesbar).
fn lenient_transparency<'de, D: Deserializer<'de>>(d: D) -> std::result::Result<u8, D::Error> {
    let value = serde_json::Value::deserialize(d)?;
    Ok(value.as_f64().map_or(DEFAULT_PANEL_TRANSPARENCY, |v| {
        v.round().clamp(0.0, f64::from(MAX_PANEL_TRANSPARENCY)) as u8
    }))
}

/// Vergleichsform eines Ordners: unter Windows ohne Groß- und Kleinschreibung und mit `\`;
/// abschließende Trennzeichen zählen nicht.
pub fn path_key(path: &Path) -> String {
    let mut s = path.to_string_lossy().into_owned();
    if cfg!(windows) {
        s = s.replace('/', "\\").to_lowercase();
    }
    let t = s.trim_end_matches(['\\', '/']);
    if t.is_empty() { s } else { t.to_string() }
}

/// Favoriten ohne leere Einträge und Doppelte (nach [`path_key`]), höchstens
/// [`MAX_FAVORITES`], in der bisherigen Reihenfolge.
pub fn clean_favorites(list: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut seen = std::collections::HashSet::new();
    list.into_iter()
        .filter(|p| !p.to_string_lossy().trim().is_empty())
        .filter(|p| seen.insert(path_key(p)))
        .take(MAX_FAVORITES)
        .collect()
}

/// Liest `favorites` aus jedem JSON-Wert: `null`, ein falscher Typ oder Einträge, die keine
/// Texte sind, werden übergangen statt die ganze Datei unlesbar zu machen.
fn lenient_favorites<'de, D: Deserializer<'de>>(
    d: D,
) -> std::result::Result<Vec<PathBuf>, D::Error> {
    let value = serde_json::Value::deserialize(d)?;
    let list = value
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(serde_json::Value::as_str)
                .map(PathBuf::from)
                .collect()
        })
        .unwrap_or_default();
    Ok(clean_favorites(list))
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

    /// Liest die Datei frisch: fehlt sie, die Vorgabe; lässt sie sich nicht lesen (anderer
    /// Lesefehler oder kaputter Inhalt), ein Fehler.
    pub fn read_from(path: &Path) -> Result<Config> {
        match std::fs::read_to_string(path) {
            Ok(t) => serde_json::from_str(&t).map_err(|e| ProjectError::Invalid(e.to_string())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Config::default()),
            Err(e) => Err(ProjectError::io(path, &e)),
        }
    }

    /// Darf `path` überschrieben werden, ohne etwas zu verlieren? Ja, wenn es die Datei nicht
    /// gibt oder sie sich lesen lässt (eine unlesbare Datei könnte noch die Wurzel enthalten).
    pub fn safe_to_overwrite(path: &Path) -> bool {
        Config::read_from(path).is_ok()
    }

    /// Lesen, ändern, schreiben: lädt den Stand auf der Platte (nicht den im Speicher, den
    /// eine andere Instanz inzwischen überholt haben kann), ändert ihn mit `change` und
    /// schreibt ihn. Eine unlesbare Datei bleibt unangetastet (Fehler). Liefert den
    /// geschriebenen Stand.
    pub fn update_at(path: &Path, change: impl FnOnce(&mut Config)) -> Result<Config> {
        let mut c = Config::read_from(path)?;
        change(&mut c);
        c.save_to(path)?;
        Ok(c)
    }

    pub fn save_to(&self, path: &Path) -> Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| ProjectError::io(dir, &e))?;
        }
        let text =
            serde_json::to_string_pretty(self).map_err(|e| ProjectError::Invalid(e.to_string()))?;
        atomic_write_unique(path, text.as_bytes())
    }
}
