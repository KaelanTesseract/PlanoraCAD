//! Kleine Konfiguration (gewählte Wurzel, Favoriten der Dateiauswahl, Durchsichtigkeit der
//! Panels, Abweichungen der Tastenbelegung, Einstellungen der Updates, Design, Sprache), ohne neue
//! Abhängigkeit.

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

/// Wert von `theme` für das helle Design.
pub const THEME_LIGHT: &str = "hell";

/// Wert von `theme` für das dunkle Design (ohne Wahl gilt es ebenfalls).
pub const THEME_DARK: &str = "dunkel";

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
    /// Update-Kanal als Kennung (`"dev"`, `"beta"`, `"stable"`; Einstellungen → Updates).
    /// Unbekannte, leere oder kaputte Werte gelten als [`DEFAULT_UPDATE_CHANNEL`]; die
    /// Voreinstellung wird nicht geschrieben.
    #[serde(
        default = "default_update_channel",
        skip_serializing_if = "is_default_update_channel",
        deserialize_with = "lenient_update_channel"
    )]
    pub update_channel: String,
    /// Beim Start nach Updates suchen (Voreinstellung an; nur „aus“ wird geschrieben). Kein
    /// Wahrheitswert → Voreinstellung.
    #[serde(
        default = "default_true",
        skip_serializing_if = "is_true",
        deserialize_with = "lenient_update_check"
    )]
    pub update_check: bool,
    /// Zeitpunkt der letzten Prüfung in Sekunden seit 1970 (UTC). Keine ganze Zahl ab 0 →
    /// keiner.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient_seconds"
    )]
    pub last_update_check: Option<u64>,
    /// Übersprungene Version als Text („Diese Version überspringen“); geprüft wird erst im
    /// Programm. Kein oder ein leerer Text → keine.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient_text"
    )]
    pub skipped_version: Option<String>,
    /// Begonnenes Update (von Version, auf Version), damit nach dem Neustart der Hinweis
    /// „Aktualisiert auf …“ kommt. Fehlt ein Teil oder ist er kein Text → keines.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient_pending_update"
    )]
    pub update_pending: Option<PendingUpdate>,
    /// Gewähltes Design (Ansicht → Helles/Dunkles Design): [`THEME_LIGHT`] oder [`THEME_DARK`].
    /// Keine Wahl, ein falscher Typ oder ein anderer Text → keine (es gilt das dunkle Design);
    /// ohne Wahl wird nichts geschrieben.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient_theme"
    )]
    pub theme: Option<String>,
    /// Ausführliches Protokoll (Einstellungen → Updates): mehr Zeilen in der Protokolldatei, etwa
    /// jeder Befehl mit Dauer. Voreinstellung aus; nur „an“ wird geschrieben. Kein
    /// Wahrheitswert → aus.
    #[serde(
        default,
        skip_serializing_if = "is_false",
        deserialize_with = "lenient_bool_false"
    )]
    pub verbose_log: bool,
    /// Gewählte Sprache als Id (BCP 47, etwa `de`, `en`, `pt-BR`; Einstellungen → Darstellung).
    /// Ohne Wahl gilt beim Start die Sprache von Windows. Kein oder ein leerer Text → keine
    /// Wahl; ob es die Sprache gibt, prüft erst das Programm. Ohne Wahl wird nichts geschrieben.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient_language"
    )]
    pub language: Option<String>,
    /// Gewählte Maßeinheit der Anzeige als Id (`mm` oder `in`; Einstellungen → Darstellung).
    /// Kein oder ein leerer Text → keine Wahl (Millimeter); ob es die Einheit gibt, prüft erst
    /// das Programm. Ohne Wahl wird nichts geschrieben.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient_language"
    )]
    pub length_unit: Option<String>,
}

/// Voreinstellung des Update-Kanals (Kennung).
pub const DEFAULT_UPDATE_CHANNEL: &str = "dev";

/// Kennungen der Update-Kanäle, die die Konfiguration kennt (gleich `planora_update::Channel::id`
/// in der Reihenfolge von `Channel::ALL`; ein Test im Programm hält beide gleich).
pub const UPDATE_CHANNEL_IDS: [&str; 3] = ["dev", "beta", "stable"];

/// Begonnenes Update: Versionen als Text, geprüft wird erst im Programm. Dazu das Fenster der
/// alten Version, damit die neue in derselben Größe und Lage öffnet (fehlt es, die Vorgabe).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PendingUpdate {
    pub from: String,
    pub to: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window: Option<WindowState>,
}

/// Größte Breite und Höhe eines gemerkten Fensters in Punkten.
pub const MAX_WINDOW_SIZE: f32 = 16384.0;

/// Größter Betrag der gemerkten Lage in Punkten; weiter draußen (etwa -32000/-32000, die Lage,
/// die Windows für ein minimiertes Fenster meldet) gilt sie als unbrauchbar.
pub const MAX_WINDOW_POS: f32 = 10000.0;

/// Größe, Lage und Zustand eines Programmfensters in Punkten: `x`, `y` die linke obere Ecke
/// des Fensters samt Rahmen (fehlen sie, setzt das System das Fenster), `width`, `height` die
/// Innenfläche.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct WindowState {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub x: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub y: Option<f32>,
    pub width: f32,
    pub height: f32,
    #[serde(default)]
    pub maximized: bool,
}

impl WindowState {
    /// Nur Plausibles: ohne endliche Größe über 0 keines; die Größe höchstens
    /// [`MAX_WINDOW_SIZE`]. Die Lage bleibt nur, wenn sie endlich ist, ihr Betrag höchstens
    /// [`MAX_WINDOW_POS`] beträgt und das Fenster wenigstens zum Teil im Bereich 0 bis
    /// [`MAX_WINDOW_SIZE`] liegt; sonst fehlt sie (dann setzt das System das Fenster), damit
    /// das Fenster nie unsichtbar öffnet.
    pub fn sanitized(self) -> Option<WindowState> {
        let size_ok = |n: f32| n.is_finite() && n > 0.0;
        if !size_ok(self.width) || !size_ok(self.height) {
            return None;
        }
        let width = self.width.min(MAX_WINDOW_SIZE);
        let height = self.height.min(MAX_WINDOW_SIZE);
        let pos_ok = |p: Option<f32>, len: f32| {
            p.is_some_and(|p| {
                p.is_finite() && p.abs() <= MAX_WINDOW_POS && p + len > 0.0 && p < MAX_WINDOW_SIZE
            })
        };
        let keep = pos_ok(self.x, width) && pos_ok(self.y, height);
        Some(WindowState {
            x: self.x.filter(|_| keep),
            y: self.y.filter(|_| keep),
            width,
            height,
            maximized: self.maximized,
        })
    }
}

/// Liest ein Fenster aus einem JSON-Wert und prüft es ([`WindowState::sanitized`]): ohne
/// brauchbare Größe keines. Fehlt `maximized` oder ist es kein Wahrheitswert, gilt „nicht
/// maximiert“.
fn lenient_window(value: Option<&serde_json::Value>) -> Option<WindowState> {
    let value = value?;
    let num = |key: &str| {
        value
            .get(key)
            .and_then(serde_json::Value::as_f64)
            .map(|n| n as f32)
    };
    WindowState {
        x: num("x"),
        y: num("y"),
        width: num("width")?,
        height: num("height")?,
        maximized: value
            .get("maximized")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false),
    }
    .sanitized()
}

impl Default for Config {
    fn default() -> Self {
        Config {
            root: None,
            favorites: Vec::new(),
            panel_transparency: DEFAULT_PANEL_TRANSPARENCY,
            keys: BTreeMap::new(),
            update_channel: DEFAULT_UPDATE_CHANNEL.to_string(),
            update_check: true,
            last_update_check: None,
            skipped_version: None,
            update_pending: None,
            theme: None,
            verbose_log: false,
            language: None,
            length_unit: None,
        }
    }
}

fn default_update_channel() -> String {
    DEFAULT_UPDATE_CHANNEL.to_string()
}

fn is_default_update_channel(id: &str) -> bool {
    id == DEFAULT_UPDATE_CHANNEL
}

fn default_true() -> bool {
    true
}

fn is_true(b: &bool) -> bool {
    *b
}

fn is_false(b: &bool) -> bool {
    !*b
}

/// Liest einen Schalter mit Voreinstellung „aus“ aus jedem JSON-Wert: kein Wahrheitswert → aus.
fn lenient_bool_false<'de, D: Deserializer<'de>>(d: D) -> std::result::Result<bool, D::Error> {
    Ok(serde_json::Value::deserialize(d)?
        .as_bool()
        .unwrap_or(false))
}

/// Liest `update_channel` aus jedem JSON-Wert: nur eine bekannte Kennung in genauer
/// Schreibweise gilt, alles andere ergibt [`DEFAULT_UPDATE_CHANNEL`].
fn lenient_update_channel<'de, D: Deserializer<'de>>(
    d: D,
) -> std::result::Result<String, D::Error> {
    let value = serde_json::Value::deserialize(d)?;
    let id = value
        .as_str()
        .filter(|id| UPDATE_CHANNEL_IDS.contains(id))
        .unwrap_or(DEFAULT_UPDATE_CHANNEL);
    Ok(id.to_string())
}

/// Liest `update_check` aus jedem JSON-Wert: kein Wahrheitswert → an.
fn lenient_update_check<'de, D: Deserializer<'de>>(d: D) -> std::result::Result<bool, D::Error> {
    Ok(serde_json::Value::deserialize(d)?.as_bool().unwrap_or(true))
}

/// Liest `theme` aus jedem JSON-Wert: nur "hell" oder "dunkel" in genauer Schreibweise gilt.
fn lenient_theme<'de, D: Deserializer<'de>>(d: D) -> std::result::Result<Option<String>, D::Error> {
    let value = serde_json::Value::deserialize(d)?;
    Ok(value
        .as_str()
        .filter(|t| *t == THEME_LIGHT || *t == THEME_DARK)
        .map(String::from))
}

/// Liest `language` aus jedem JSON-Wert: ein Text ohne Leerraum am Rand, sonst keine Wahl.
fn lenient_language<'de, D: Deserializer<'de>>(
    d: D,
) -> std::result::Result<Option<String>, D::Error> {
    let value = serde_json::Value::deserialize(d)?;
    Ok(value
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from))
}

/// Liest Sekunden aus jedem JSON-Wert: nur ganze Zahlen ab 0, sonst keine.
fn lenient_seconds<'de, D: Deserializer<'de>>(d: D) -> std::result::Result<Option<u64>, D::Error> {
    Ok(serde_json::Value::deserialize(d)?.as_u64())
}

/// Liest einen Text aus jedem JSON-Wert: kein oder ein leerer Text → keiner.
fn lenient_text<'de, D: Deserializer<'de>>(d: D) -> std::result::Result<Option<String>, D::Error> {
    let value = serde_json::Value::deserialize(d)?;
    Ok(value.as_str().filter(|s| !s.is_empty()).map(String::from))
}

/// Liest `update_pending` aus jedem JSON-Wert: ein Objekt mit den Texten `from` und `to`,
/// sonst keines (weitere Felder werden übergangen); dazu nachsichtig das Fenster
/// ([`lenient_window`]).
fn lenient_pending_update<'de, D: Deserializer<'de>>(
    d: D,
) -> std::result::Result<Option<PendingUpdate>, D::Error> {
    let value = serde_json::Value::deserialize(d)?;
    let text = |key: &str| value.get(key).and_then(serde_json::Value::as_str);
    Ok(match (text("from"), text("to")) {
        (Some(from), Some(to)) => Some(PendingUpdate {
            from: from.to_string(),
            to: to.to_string(),
            window: lenient_window(value.get("window")),
        }),
        _ => None,
    })
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
    /// Ob das dunkle Design gilt (ohne Wahl: ja).
    pub fn dark(&self) -> bool {
        self.theme.as_deref() != Some(THEME_LIGHT)
    }

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
