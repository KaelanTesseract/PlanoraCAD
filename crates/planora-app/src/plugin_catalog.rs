//! Plug-in-Katalog (Task 2): die Liste aus dem festen Katalog-Repo holen, ihren Zustand halten
//! und den letzten erfolgreichen Stand im Benutzer-Cacheordner (`plugin-cache/katalog/`)
//! ablegen. Holen und Schreiben des Zwischenspeichers laufen in einem [`Job`]; der Netzweg wird
//! übergeben (in Tests eine Nachbildung).
//!
//! Ablauf: neuestes Release (≤ [`MAX_RELEASES_BYTES`], 30 s) → [`parse_latest_release`] →
//! `katalog.json` (≤ [`MAX_CATALOG_BYTES`]; laut Release größer ⇒ gar nicht laden) →
//! [`parse_catalog`] → nur die Symbole, die ein Eintrag nennt und die im Release liegen (je ≤
//! [`MAX_SYMBOL_BYTES`]). Lässt sich ein Symbol nicht laden, bleibt ein schon vorhandenes mit
//! demselben Namen; sonst fehlt es (Standardsymbol).
//!
//! Zwischenspeicher: `katalog.json` (wie geladen), `stand.json` ([`CacheIndex`]) und je Symbol
//! eine Datei mit dem SHA-256 seines Namens als Namen – nie der Name aus dem Katalog. Jede Datei
//! wird atomar geschrieben (Zwischendatei, dann umbenennen); `stand.json` zuletzt, mit der
//! Prüfsumme der `katalog.json`, damit ein halber Stand als kaputt erkannt wird. Gelesen wird
//! beim Start mit festen Größengrenzen; zu große Dateien gelten als kaputt.

use crate::diag;
use crate::jobs::{Job, Poll};
use crate::plugin_repo::{self, RepoFetch};
use planora_exchange::Progress;
use planora_i18n::tr;
use planora_project::fsutil::atomic_write_unique;
use planora_update::catalog::{
    CacheIndex, CatalogEntry, CatalogError, MAX_CATALOG_BYTES, MAX_SYMBOL_BYTES,
    latest_release_url, parse_catalog, parse_latest_release,
};
use planora_update::plugin_source::{MAX_RELEASES_BYTES, REQUEST_TIMEOUT};
use planora_update::sha256::hex_digest;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// Name der Liste im Zwischenspeicher.
const CATALOG_FILE: &str = "katalog.json";
/// Name des Verzeichnisses im Zwischenspeicher ([`CacheIndex`]).
const INDEX_FILE: &str = "stand.json";
/// Größtes `stand.json`, das gelesen wird (Bytes).
pub const MAX_INDEX_BYTES: u64 = 64 << 10;
/// Ohne `force` wird frühestens so lange nach dem letzten Versuch erneut geholt (Sekunden).
pub const RETRY_SECS: u64 = 600;
/// Zwischendateien (`*.tmp`), die mindestens so alt sind, gelten als übrig geblieben.
const STALE_TMP_AGE: Duration = Duration::from_secs(600);
/// Höchstens so viele übergangene Einträge kommen einzeln ins Protokoll.
const MAX_SKIP_LINES: usize = 20;
/// Höchstlänge eines Grundes im Protokoll (Zeichen).
const MAX_REASON_CHARS: usize = 200;

/// Warum ein gezeigter Stand nicht der neueste ist.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StaleReason {
    /// Das letzte Holen scheiterte am Netz (keine Verbindung, Zeitgrenze, Status).
    Offline,
    /// Das letzte Holen lieferte einen fehlerhaften Katalog oder Release.
    Invalid,
}

impl StaleReason {
    /// Hinweis für die Oberfläche.
    pub fn text(self) -> String {
        match self {
            StaleReason::Offline => tr!("catalog-stale-offline"),
            StaleReason::Invalid => tr!("catalog-stale-invalid"),
        }
    }
}

/// Ein Symbol des Katalogs: der Inhalt (PNG, ungeprüft) und sein Hash. Der Hash wird einmal
/// gebildet, wenn das Symbol geholt oder aus dem Zwischenspeicher gelesen wird; die Oberfläche
/// erkennt daran ein geändertes Symbol, ohne in jedem Bild die Bytes zu hashen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Symbol {
    pub bytes: Vec<u8>,
    pub hash: u64,
}

impl Symbol {
    pub fn new(bytes: Vec<u8>) -> Symbol {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        bytes.hash(&mut hasher);
        Symbol {
            hash: hasher.finish(),
            bytes,
        }
    }
}

/// Symbolname aus dem Katalog → Symbol.
pub type Symbols = BTreeMap<String, Symbol>;

/// Zustand des Katalogs. Ob gerade geholt wird, sagt [`Catalog::is_loading`]; der Zustand bleibt
/// solange der letzte.
#[derive(Debug, Clone, PartialEq)]
pub enum CatalogState {
    /// Noch nie geladen und kein (lesbarer) Zwischenspeicher.
    Never,
    /// Ein Stand: frisch geholt oder aus dem Zwischenspeicher. `symbols`: Symbolname aus dem
    /// Katalog → Symbol (Inhalt und Hash). `fetched_at`: Zeitpunkt des Holens (Sekunden
    /// seit 1970). `stale`: das letzte Holen misslang (warum), gezeigt wird dieser ältere Stand.
    Ready {
        entries: Vec<CatalogEntry>,
        symbols: Symbols,
        fetched_at: u64,
        stale: Option<StaleReason>,
    },
    /// Der Katalog hat ein neueres Format, als dieses Programm kennt.
    TooNew(u32),
    /// Holen misslang und es gibt keinen Stand (Fehlertext).
    Failed(String),
}

/// Ergebnis eines Holens aus dem Arbeitsthread.
enum Outcome {
    Loaded {
        entries: Vec<CatalogEntry>,
        symbols: Symbols,
        /// Gründe für übergangene Einträge.
        skipped: Vec<String>,
        /// Der Zwischenspeicher ließ sich nicht schreiben (Grund).
        cache_error: Option<String>,
    },
    TooNew(u32),
    Failed(String, StaleReason),
}

/// Der Katalog: Zustand, laufendes Holen und Zwischenspeicher.
pub struct Catalog {
    /// Ordner des Zwischenspeichers; leer ⇒ ohne Zwischenspeicher (Tests, [`Plugins::empty`]).
    ///
    /// [`Plugins::empty`]: crate::plugins::Plugins::empty
    dir: PathBuf,
    state: CatalogState,
    /// Das laufende Holen und sein Zeitpunkt (wird zu `fetched_at`).
    job: Option<(Job<Outcome>, u64)>,
    /// Zeitpunkt des letzten Versuchs (Sekunden seit 1970).
    last_attempt: Option<u64>,
}

impl Catalog {
    /// Liest den Zwischenspeicher in `cache_dir` (mit Größengrenzen); fehlt er oder ist er
    /// kaputt ⇒ [`CatalogState::Never`].
    pub fn new(cache_dir: PathBuf) -> Catalog {
        let state = read_cache(&cache_dir).unwrap_or(CatalogState::Never);
        Catalog {
            dir: cache_dir,
            state,
            job: None,
            last_attempt: None,
        }
    }

    /// Der letzte Zustand (auch während des Holens).
    pub fn state(&self) -> &CatalogState {
        &self.state
    }

    /// Wird gerade geholt?
    pub fn is_loading(&self) -> bool {
        self.job.is_some()
    }

    /// Hat der Katalog einen Zwischenspeicher (sonst holt die tägliche Prüfung nicht)?
    pub fn has_cache(&self) -> bool {
        !self.dir.as_os_str().is_empty()
    }

    /// Zeitpunkt des gezeigten Stands, falls es einen gibt.
    pub fn fetched_at(&self) -> Option<u64> {
        match &self.state {
            CatalogState::Ready { fetched_at, .. } => Some(*fetched_at),
            _ => None,
        }
    }

    /// Startet das Holen, wenn `force` oder der letzte Versuch mindestens [`RETRY_SECS`] her ist
    /// (ein Versuch „in der Zukunft“, etwa nach Zurückstellen der Uhr, zählt nicht), und nicht
    /// schon geholt wird.
    pub fn request(&mut self, fetch: RepoFetch, now: u64, force: bool) {
        if self.job.is_some() {
            return;
        }
        let recent = self
            .last_attempt
            .is_some_and(|t| t <= now && now - t < RETRY_SECS);
        if recent && !force {
            return;
        }
        self.last_attempt = Some(now);
        let old = match &self.state {
            CatalogState::Ready { symbols, .. } => symbols.clone(),
            _ => BTreeMap::new(),
        };
        let dir = self.dir.clone();
        let job = Job::spawn(move |progress| load(fetch, &dir, now, &old, progress));
        self.job = Some((job, now));
    }

    /// Holt das Ergebnis ab, falls fertig; `true`, wenn sich der Zustand geändert hat.
    pub fn poll(&mut self) -> bool {
        self.take(false)
    }

    /// Wie [`Catalog::poll`], wartet aber auf das Ergebnis (Tests und Abläufe ohne Fenster).
    pub fn wait(&mut self) -> bool {
        self.take(true)
    }

    fn take(&mut self, block: bool) -> bool {
        let Some((job, started)) = &self.job else {
            return false;
        };
        let started = *started;
        let outcome = match job.fetch(block) {
            Poll::Pending => return false,
            Poll::Ready(o) => o,
            Poll::Lost => Outcome::Failed(tr!("catalog-load-failed"), StaleReason::Offline),
        };
        self.job = None;
        match outcome {
            Outcome::Loaded {
                entries,
                symbols,
                skipped,
                cache_error,
            } => {
                log_skipped(&skipped);
                if let Some(e) = cache_error {
                    diag::warn(&format!("Plug-in catalog: cache not written: {e}"));
                }
                self.state = CatalogState::Ready {
                    entries,
                    symbols,
                    fetched_at: started,
                    stale: None,
                };
            }
            Outcome::TooNew(n) => {
                diag::warn(&format!("Plug-in catalog: format {n} is too new"));
                self.state = CatalogState::TooNew(n);
            }
            Outcome::Failed(e, reason) => {
                diag::warn(&format!("Plug-in catalog: {}", truncate(&e)));
                match &mut self.state {
                    CatalogState::Ready { stale, .. } => *stale = Some(reason),
                    // Ein neueres Format bleibt der bessere Hinweis als ein Netzfehler.
                    CatalogState::TooNew(_) => {}
                    other => *other = CatalogState::Failed(e),
                }
            }
        }
        true
    }
}

/// Holt Release, Liste und Symbole und schreibt den Zwischenspeicher (im Arbeitsthread).
/// `old`: die Symbole des bisherigen Stands (bleiben, wenn ihr Download misslingt).
fn load(fetch: RepoFetch, dir: &Path, now: u64, old: &Symbols, progress: &Progress) -> Outcome {
    let (text, entries, symbols, skipped) = match fetch_all(fetch, old, progress) {
        Ok(Fetched::Catalog(t, e, s, k)) => (t, e, s, k),
        Ok(Fetched::TooNew(n)) => return Outcome::TooNew(n),
        Err((e, reason)) => return Outcome::Failed(e, reason),
    };
    if progress.is_cancelled() {
        return Outcome::Failed(tr!("catalog-load-failed"), StaleReason::Offline);
    }
    let cache_error = write_cache(dir, &text, &symbols, old, now).err();
    Outcome::Loaded {
        entries,
        symbols,
        skipped,
        cache_error,
    }
}

/// Was beim Holen herauskam.
enum Fetched {
    /// Text, Einträge, Symbole, übergangene Einträge.
    Catalog(String, Vec<CatalogEntry>, Symbols, Vec<String>),
    TooNew(u32),
}

fn fetch_all(
    fetch: RepoFetch,
    old: &Symbols,
    progress: &Progress,
) -> Result<Fetched, (String, StaleReason)> {
    let net = |e| (e, StaleReason::Offline);
    let bad = |e| (e, StaleReason::Invalid);
    let body = plugin_repo::get(
        fetch,
        &latest_release_url(),
        MAX_RELEASES_BYTES,
        REQUEST_TIMEOUT,
    )
    .map_err(net)?;
    let release = parse_latest_release(&String::from_utf8_lossy(&body)).map_err(bad)?;
    if release.catalog.size > MAX_CATALOG_BYTES {
        return Err(bad(tr!("catalog-too-large")));
    }
    let body = plugin_repo::get(
        fetch,
        &release.catalog.url,
        MAX_CATALOG_BYTES,
        REQUEST_TIMEOUT,
    )
    .map_err(net)?;
    let text = String::from_utf8(body).map_err(|_| bad(tr!("catalog-invalid")))?;
    let (entries, skipped) = match parse_catalog(&text) {
        Ok(v) => v,
        Err(CatalogError::TooNew(n)) => return Ok(Fetched::TooNew(n)),
        Err(CatalogError::Invalid(e)) => return Err(bad(e)),
    };
    let wanted: BTreeSet<&str> = entries.iter().filter_map(|e| e.symbol.as_deref()).collect();
    let mut symbols = BTreeMap::new();
    for asset in release
        .symbols
        .iter()
        .filter(|a| wanted.contains(a.name.as_str()))
    {
        if progress.is_cancelled() {
            return Err(net(tr!("catalog-load-failed")));
        }
        if symbols.contains_key(&asset.name) {
            continue;
        }
        match plugin_repo::get(fetch, &asset.url, MAX_SYMBOL_BYTES, REQUEST_TIMEOUT) {
            Ok(bytes) => {
                symbols.insert(asset.name.clone(), Symbol::new(bytes));
            }
            Err(e) => {
                diag::warn(&format!(
                    "Plug-in catalog: symbol not loaded: {}",
                    truncate(&e)
                ));
                if let Some(symbol) = old.get(&asset.name) {
                    symbols.insert(asset.name.clone(), symbol.clone());
                }
            }
        }
    }
    Ok(Fetched::Catalog(text, entries, symbols, skipped))
}

/// `text` auf [`MAX_REASON_CHARS`] Zeichen gekürzt (mit „…“).
fn truncate(text: &str) -> String {
    match text.char_indices().nth(MAX_REASON_CHARS) {
        Some((i, _)) => format!("{}…", &text[..i]),
        None => text.to_string(),
    }
}

/// Übergangene Einträge ins Protokoll: höchstens [`MAX_SKIP_LINES`] Zeilen und eine Summe.
fn log_skipped(skipped: &[String]) {
    for reason in skipped.iter().take(MAX_SKIP_LINES) {
        diag::warn(&format!("Plug-in catalog: {}", truncate(reason)));
    }
    if skipped.len() > MAX_SKIP_LINES {
        diag::warn(&format!(
            "Plug-in catalog: … and {} more skipped entries",
            skipped.len() - MAX_SKIP_LINES
        ));
    }
}

/// Liest `file`, wenn es höchstens `max` Bytes hat.
fn read_limited(file: &Path, max: u64) -> Option<Vec<u8>> {
    if std::fs::metadata(file).ok()?.len() > max {
        return None;
    }
    let bytes = std::fs::read(file).ok()?;
    (bytes.len() as u64 <= max).then_some(bytes)
}

/// Liest den Stand aus `dir`; `None`, wenn er fehlt, zu groß oder kaputt ist oder nicht
/// zusammenpasst.
fn read_cache(dir: &Path) -> Option<CatalogState> {
    if dir.as_os_str().is_empty() {
        return None;
    }
    let index = read_limited(&dir.join(INDEX_FILE), MAX_INDEX_BYTES)?;
    let index = CacheIndex::parse(std::str::from_utf8(&index).ok()?)?;
    let text = read_limited(&dir.join(CATALOG_FILE), MAX_CATALOG_BYTES)?;
    let text = String::from_utf8(text).ok()?;
    if hex_digest(text.as_bytes()) != index.catalog_sha256 {
        return None;
    }
    // Ein Format, das zu neu ist, wird nie gespeichert; im Zwischenspeicher gilt es als kaputt.
    let (entries, _) = parse_catalog(&text).ok()?;
    let mut symbols = BTreeMap::new();
    for (name, file) in &index.symbols {
        // Nur Dateinamen der eigenen Form (kein Pfad aus einer veränderten stand.json).
        if !CacheIndex::is_symbol_file(file) || *file != CacheIndex::symbol_file(name) {
            continue;
        }
        if let Some(bytes) = read_limited(&dir.join(file), MAX_SYMBOL_BYTES) {
            symbols.insert(name.clone(), Symbol::new(bytes));
        }
    }
    Some(CatalogState::Ready {
        entries,
        symbols,
        fetched_at: index.fetched_at,
        stale: None,
    })
}

/// Schreibt den Stand nach `dir` (ohne Ordner nichts): Symbole (unveränderte aus `old` nicht
/// neu), Liste, zuletzt `stand.json`; danach fallen Symboldateien weg, die nicht mehr
/// dazugehören. Vorher werden übrig gebliebene Zwischendateien weggeräumt.
fn write_cache(
    dir: &Path,
    text: &str,
    symbols: &Symbols,
    old: &Symbols,
    now: u64,
) -> Result<(), String> {
    if dir.as_os_str().is_empty() {
        return Ok(());
    }
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    remove_stale_tmp(dir);
    let mut index = CacheIndex {
        fetched_at: now,
        catalog_sha256: hex_digest(text.as_bytes()),
        symbols: BTreeMap::new(),
    };
    for (name, symbol) in symbols {
        let file = CacheIndex::symbol_file(name);
        let path = dir.join(&file);
        let unchanged = old.get(name) == Some(symbol)
            && std::fs::metadata(&path).is_ok_and(|m| m.len() == symbol.bytes.len() as u64);
        if !unchanged {
            atomic_write_unique(&path, &symbol.bytes).map_err(|e| e.to_string())?;
        }
        index.symbols.insert(name.clone(), file);
    }
    atomic_write_unique(&dir.join(CATALOG_FILE), text.as_bytes()).map_err(|e| e.to_string())?;
    atomic_write_unique(&dir.join(INDEX_FILE), index.to_json().as_bytes())
        .map_err(|e| e.to_string())?;
    let keep: BTreeSet<&String> = index.symbols.values().collect();
    if let Ok(read) = std::fs::read_dir(dir) {
        for entry in read.filter_map(Result::ok) {
            let name = entry.file_name().to_string_lossy().into_owned();
            if CacheIndex::is_symbol_file(&name) && !keep.contains(&name) {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }
    Ok(())
}

/// Löscht `*.tmp` in `dir`, die mindestens [`STALE_TMP_AGE`] alt sind (jüngere könnten gerade
/// von einem zweiten Programmfenster geschrieben werden).
fn remove_stale_tmp(dir: &Path) {
    let Ok(read) = std::fs::read_dir(dir) else {
        return;
    };
    let now = SystemTime::now();
    for entry in read.filter_map(Result::ok) {
        if !entry.file_name().to_string_lossy().ends_with(".tmp") {
            continue;
        }
        let old = entry
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| now.duration_since(t).ok())
            .is_some_and(|age| age >= STALE_TMP_AGE);
        if old {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}
