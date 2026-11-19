//! Fundorte der Plug-ins: mitgeliefert (vom Installer, nur lesbar) und vom Nutzer installiert;
//! dazu Installieren, Entfernen und der Zwischenspeicher für übersetzten Maschinencode.
//!
//! Ordnernamen: `planora-cad` wie bei `config.json` (Abweichung von der Spec, die `planoracad`
//! nennt), unter Windows `PlanoraCAD`.

use crate::manifest::Manifest;
use crate::package::{MAX_WASM, Package};
use crate::runtime::{Component, Engine, PluginError};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::hash::{Hash, Hasher};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// Ordnername unter Windows (`%APPDATA%`, `%LOCALAPPDATA%`).
const WINDOWS_DIR: &str = "PlanoraCAD";
/// Ordnername unter Linux (XDG-Ordner, `share`).
const LINUX_DIR: &str = "planora-cad";

/// Wo Plug-ins liegen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Places {
    /// Mitgelieferte Plug-ins (nur lesbar), in dieser Reihenfolge durchsucht.
    pub bundled: Vec<PathBuf>,
    /// Vom Nutzer installierte Plug-ins.
    pub user: PathBuf,
    /// Zwischenspeicher für übersetzten Maschinencode (`.cwasm`).
    pub cache: PathBuf,
}

impl Places {
    /// Die Fundorte dieses Programms (Programmordner und Umgebung des Prozesses).
    pub fn system() -> Places {
        let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("planora-cad"));
        Places::from_env(&exe, &|k| std::env::var_os(k), cfg!(windows))
    }

    /// Berechnet die Fundorte aus dem Pfad des Programms und den Umgebungswerten `env`
    /// (leere Werte zählen als nicht gesetzt). `windows` wählt die Ordner des Betriebssystems.
    pub fn from_env(exe: &Path, env: &dyn Fn(&str) -> Option<OsString>, windows: bool) -> Places {
        let var = |k: &str| env(k).filter(|v| !v.is_empty()).map(PathBuf::from);
        let exe_dir = exe
            .parent()
            .map_or_else(|| PathBuf::from("."), Path::to_path_buf);
        let home = || {
            var(if windows { "USERPROFILE" } else { "HOME" }).unwrap_or_else(|| PathBuf::from("."))
        };
        let mut bundled = vec![exe_dir.join("plugins")];
        if windows {
            let roaming = var("APPDATA").unwrap_or_else(|| home().join("AppData").join("Roaming"));
            let local = var("LOCALAPPDATA").unwrap_or_else(|| home().join("AppData").join("Local"));
            Places {
                bundled,
                user: roaming.join(WINDOWS_DIR).join("plugins"),
                cache: local.join(WINDOWS_DIR).join("plugin-cache"),
            }
        } else {
            bundled.push(
                exe_dir
                    .join("..")
                    .join("share")
                    .join(LINUX_DIR)
                    .join("plugins"),
            );
            let data = var("XDG_DATA_HOME").unwrap_or_else(|| home().join(".local").join("share"));
            let cache = var("XDG_CACHE_HOME").unwrap_or_else(|| home().join(".cache"));
            Places {
                bundled,
                user: data.join(LINUX_DIR).join("plugins"),
                cache: cache.join(LINUX_DIR).join("plugin-cache"),
            }
        }
    }
}

/// Woher ein Plug-in kommt.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Origin {
    /// Vom Installer mitgeliefert.
    Bundled,
    /// Vom Nutzer installiert.
    User,
}

/// Ein gefundener Plug-in-Ordner. Mit `problem` ist er nicht startbar; `manifest` ist dann ein
/// Platzhalter (Kennung und Name = Ordnername, Version 0.0.0), damit die Verwaltung ihn zeigen kann.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Found {
    pub dir: PathBuf,
    pub origin: Origin,
    pub manifest: Manifest,
    pub problem: Option<String>,
}

const MANIFEST_FILE: &str = "plugin.toml";
const WASM_FILE: &str = "plugin.wasm";

/// Ein Platzhalter-Manifest für einen kaputten Ordner.
fn placeholder(dir_name: &str) -> Manifest {
    Manifest {
        id: dir_name.to_string(),
        name: dir_name.to_string(),
        version: semver::Version::new(0, 0, 0),
        api: String::new(),
        author: String::new(),
        license: String::new(),
        description: String::new(),
        rights: Default::default(),
        net_hosts: Vec::new(),
        memory_mb: 0,
    }
}

/// Liest einen Plug-in-Ordner ein (nur das Manifest; `plugin.wasm` muss da sein).
fn examine(dir: &Path, origin: Origin) -> Found {
    let dir_name = dir
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let broken = |problem: String| Found {
        dir: dir.to_path_buf(),
        origin,
        manifest: placeholder(&dir_name),
        problem: Some(problem),
    };
    let text = match std::fs::read_to_string(dir.join(MANIFEST_FILE)) {
        Ok(t) => t,
        Err(e) => return broken(format!("{MANIFEST_FILE} ist nicht lesbar: {e}")),
    };
    let manifest = match Manifest::parse(&text) {
        Ok(m) => m,
        Err(e) => return broken(e.to_string()),
    };
    let problem = if manifest.id != dir_name {
        Some(format!(
            "Der Ordner „{dir_name}“ passt nicht zur Kennung „{}“.",
            manifest.id
        ))
    } else if !dir.join(WASM_FILE).is_file() {
        Some(format!("Im Plug-in-Ordner fehlt die Datei „{WASM_FILE}“."))
    } else {
        None
    };
    Found {
        dir: dir.to_path_buf(),
        origin,
        manifest,
        problem,
    }
}

/// Die Plug-in-Ordner direkt unter `root`, nach Namen sortiert. Versteckte Ordner (`.tmp-…`,
/// `.alt-…` einer laufenden oder abgebrochenen Installation) zählen nicht.
fn plugin_dirs(root: &Path) -> Vec<PathBuf> {
    let Ok(read) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    let mut dirs: Vec<PathBuf> = read
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .filter(|e| !e.file_name().to_string_lossy().starts_with('.'))
        .map(|e| e.path())
        .collect();
    dirs.sort();
    dirs
}

/// Alle Plug-in-Ordner: erst die mitgelieferten, dann die installierten. Fehlende Fundorte sind
/// kein Fehler.
pub fn scan(places: &Places) -> Vec<Found> {
    let mut found = Vec::new();
    for root in &places.bundled {
        found.extend(
            plugin_dirs(root)
                .iter()
                .map(|d| examine(d, Origin::Bundled)),
        );
    }
    found.extend(
        plugin_dirs(&places.user)
            .iter()
            .map(|d| examine(d, Origin::User)),
    );
    found
}

/// Welcher von mehreren Fundorten derselben Kennung läuft: der startbare mit der höchsten Version,
/// bei gleicher Version der installierte (der Nutzer hat ihn bewusst eingespielt).
pub fn pick_active(found: &[Found]) -> Option<usize> {
    found
        .iter()
        .enumerate()
        .filter(|(_, f)| f.problem.is_none())
        .max_by(|(_, a), (_, b)| {
            a.manifest
                .version
                .cmp(&b.manifest.version)
                .then(a.origin.cmp(&b.origin))
        })
        .map(|(i, _)| i)
}

/// Installiert ein Paket für den Nutzer: entpackt nach `<user>/.tmp-<id>`, schiebt ein altes
/// `<user>/<id>` nach `.alt-<id>`, benennt dann um und löscht das alte. Scheitert das Umbenennen,
/// kommt das alte zurück.
pub fn install(places: &Places, pkg: &Package) -> io::Result<PathBuf> {
    let id = &pkg.manifest.id;
    std::fs::create_dir_all(&places.user)?;
    let target = places.user.join(id);
    let tmp = places.user.join(format!(".tmp-{id}"));
    let old = places.user.join(format!(".alt-{id}"));
    for leftover in [&tmp, &old] {
        if leftover.exists() {
            std::fs::remove_dir_all(leftover)?;
        }
    }
    if let Err(e) = pkg.unpack_to(&tmp) {
        let _ = std::fs::remove_dir_all(&tmp);
        return Err(e);
    }
    let had_old = target.exists();
    if had_old && let Err(e) = std::fs::rename(&target, &old) {
        let _ = std::fs::remove_dir_all(&tmp);
        return Err(e);
    }
    if let Err(e) = std::fs::rename(&tmp, &target) {
        if had_old {
            let _ = std::fs::rename(&old, &target);
        }
        let _ = std::fs::remove_dir_all(&tmp);
        return Err(e);
    }
    if had_old {
        std::fs::remove_dir_all(&old)?;
    }
    Ok(target)
}

/// Entfernt ein vom Nutzer installiertes Plug-in. Mitgelieferte lassen sich nicht entfernen.
pub fn uninstall(places: &Places, id: &str) -> io::Result<()> {
    let installed = scan(places)
        .into_iter()
        .find(|f| f.origin == Origin::User && f.manifest.id == id);
    match installed {
        Some(f) => std::fs::remove_dir_all(f.dir),
        None => Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!(
                "Das Plug-in „{id}“ ist nicht vom Nutzer installiert und lässt sich nicht entfernen."
            ),
        )),
    }
}

/// Liest `plugin.wasm` und die Sprachdateien (`languages/<id>.ftl`) eines Plug-in-Ordners.
pub fn read_files(dir: &Path) -> io::Result<(Vec<u8>, BTreeMap<String, String>)> {
    let wasm_path = dir.join(WASM_FILE);
    if std::fs::metadata(&wasm_path)?.len() > MAX_WASM {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "{WASM_FILE} ist größer als erlaubt ({} MiB).",
                MAX_WASM >> 20
            ),
        ));
    }
    let wasm = std::fs::read(wasm_path)?;
    let mut languages = BTreeMap::new();
    if let Ok(read) = std::fs::read_dir(dir.join("languages")) {
        for entry in read.filter_map(Result::ok) {
            let name = entry.file_name().to_string_lossy().into_owned();
            if let Some(id) = name.strip_suffix(".ftl")
                && !id.is_empty()
                && let Ok(text) = std::fs::read_to_string(entry.path())
            {
                languages.insert(id.to_string(), text);
            }
        }
    }
    Ok((wasm, languages))
}

/// Ein `Hasher`, der alle Bytes in SHA-256 schreibt (für den Verträglichkeits-Hash der Engine,
/// der nur `Hash` anbietet).
struct ShaHasher(Sha256);

impl Hasher for ShaHasher {
    fn write(&mut self, bytes: &[u8]) {
        self.0.update(bytes);
    }

    fn finish(&self) -> u64 {
        0
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Der Schlüssel im Zwischenspeicher: SHA-256 des WebAssembly-Codes und der
/// Verträglichkeits-Hash der Engine (wasmtime-Version, Einstellungen, Prozessor).
fn cache_key(engine: &wasmtime::Engine, wasm: &[u8]) -> String {
    let mut compat = ShaHasher(Sha256::new());
    engine.precompile_compatibility_hash().hash(&mut compat);
    let compat = compat.0.finalize();
    format!("{}-{}", hex(&Sha256::digest(wasm)), hex(&compat[..8]))
}

/// Lädt die Komponente zu `wasm` aus dem Zwischenspeicher `cache` oder übersetzt sie und legt sie
/// dort ab (`<cache>/<schlüssel>.cwasm`). Eine kaputte oder unlesbare Datei wird neu übersetzt;
/// lässt sich der Zwischenspeicher nicht schreiben, läuft das Plug-in trotzdem.
pub fn load_component(
    engine: &Engine,
    cache: &Path,
    wasm: &[u8],
) -> Result<Component, PluginError> {
    let inner = engine.wasmtime();
    let file = cache.join(format!("{}.cwasm", cache_key(inner, wasm)));
    // Die Datei wird ganz gelesen statt eingeblendet: so bleibt sie unter Windows nicht gesperrt,
    // solange die Instanz läuft, und lässt sich reparieren oder aufräumen.
    if let Ok(bytes) = std::fs::read(&file) {
        // SAFETY: `deserialize` vertraut den Bytes als Maschinencode. Gelesen wird nur unser
        // eigener Zwischenspeicher-Ordner, die Datei wurde von `serialize` dieser Funktion
        // geschrieben (atomar: Temp-Datei und Umbenennen), und ihr Name enthält SHA-256 des Codes
        // und den Verträglichkeits-Hash der Engine. wasmtime prüft Kopf und Einstellungen beim
        // Lesen zusätzlich; abgeschnittene oder fremde Dateien ergeben einen Fehler, dann wird neu
        // übersetzt.
        if let Ok(c) = unsafe { Component::deserialize(inner, &bytes) } {
            return Ok(c);
        }
    }
    let component = Component::new(inner, wasm)
        .map_err(|e| PluginError::Load(format!("Das Plug-in ließ sich nicht übersetzen: {e:#}")))?;
    if let Ok(bytes) = component.serialize() {
        let _ = write_atomic(&file, &bytes);
    }
    Ok(component)
}

/// Schreibt `bytes` über eine Temp-Datei im selben Ordner und benennt sie dann um.
fn write_atomic(file: &Path, bytes: &[u8]) -> io::Result<()> {
    let dir = file.parent().unwrap_or(Path::new("."));
    std::fs::create_dir_all(dir)?;
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let tmp = dir.join(format!(
        ".tmp-{}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed),
        file.file_name()
            .map(|n| n.to_string_lossy())
            .unwrap_or_default()
    ));
    let result = std::fs::write(&tmp, bytes).and_then(|()| std::fs::rename(&tmp, file));
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}
