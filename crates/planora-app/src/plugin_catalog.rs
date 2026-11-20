//! Plug-in-Katalog (Task 2): die Liste aus dem festen Katalog-Repo holen, ihren Zustand halten
//! und den letzten erfolgreichen Stand im Benutzer-Cacheordner (`plugin-cache/katalog/`)
//! ablegen. Das Holen läuft in einem [`Job`]; der Netzweg wird übergeben (in Tests eine
//! Nachbildung).
//!
//! Ablauf: neuestes Release (≤ [`MAX_RELEASES_BYTES`], 30 s) → [`parse_latest_release`] →
//! `katalog.json` (≤ [`MAX_CATALOG_BYTES`]; laut Release größer ⇒ gar nicht laden) →
//! [`parse_catalog`] → nur die Symbole, die ein Eintrag nennt und die im Release liegen (je ≤
//! [`MAX_SYMBOL_BYTES`]). Ein Symbol, das sich nicht laden lässt, fehlt nur (Standardsymbol).
//!
//! Zwischenspeicher: `katalog.json` (wie geladen), `stand.json` ([`CacheIndex`]) und je Symbol
//! eine Datei mit dem SHA-256 seines Namens als Namen – nie der Name aus dem Katalog. Jede Datei
//! wird atomar geschrieben (Zwischendatei, dann umbenennen); `stand.json` zuletzt, mit der
//! Prüfsumme der `katalog.json`, damit ein halber Stand als kaputt erkannt wird.

use crate::diag;
use crate::jobs::{Job, Poll};
use crate::plugin_repo::{self, RepoFetch};
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

/// Name der Liste im Zwischenspeicher.
const CATALOG_FILE: &str = "katalog.json";
/// Name des Verzeichnisses im Zwischenspeicher ([`CacheIndex`]).
const INDEX_FILE: &str = "stand.json";
/// Ohne `force` wird frühestens so lange nach dem letzten Versuch erneut geholt (Sekunden).
pub const RETRY_SECS: u64 = 600;
/// Höchstens so viele übergangene Einträge kommen einzeln ins Protokoll.
const MAX_SKIP_LINES: usize = 20;
/// Höchstlänge eines Grundes im Protokoll (Zeichen).
const MAX_REASON_CHARS: usize = 200;

/// Zustand des Katalogs.
#[derive(Debug, Clone, PartialEq)]
pub enum CatalogState {
    /// Noch nie geladen und kein (lesbarer) Zwischenspeicher.
    Never,
    /// Wird gerade geholt.
    Loading,
    /// Ein Stand: frisch geholt oder aus dem Zwischenspeicher. `symbols`: Symbolname aus dem
    /// Katalog → Inhalt (PNG oder SVG, ungeprüft). `fetched_at`: Zeitpunkt des Holens (Sekunden
    /// seit 1970). `offline`: das letzte Holen misslang, gezeigt wird dieser ältere Stand.
    Ready {
        entries: Vec<CatalogEntry>,
        symbols: BTreeMap<String, Vec<u8>>,
        fetched_at: u64,
        offline: bool,
    },
    /// Der Katalog hat ein neueres Format, als dieses Programm kennt.
    TooNew(u32),
    /// Holen misslang und es gibt keinen Stand (Fehlertext).
    Failed(String),
}

/// Ergebnis eines Holens aus dem Arbeitsthread.
enum Outcome {
    Loaded {
        /// `katalog.json` wie geladen (für den Zwischenspeicher).
        text: String,
        entries: Vec<CatalogEntry>,
        symbols: BTreeMap<String, Vec<u8>>,
        /// Gründe für übergangene Einträge.
        skipped: Vec<String>,
    },
    TooNew(u32),
    Failed(String),
}

/// Der Katalog: Zustand, laufendes Holen und Zwischenspeicher.
pub struct Catalog {
    /// Ordner des Zwischenspeichers; leer ⇒ ohne Zwischenspeicher (Tests, [`Plugins::empty`]).
    ///
    /// [`Plugins::empty`]: crate::plugins::Plugins::empty
    dir: PathBuf,
    state: CatalogState,
    /// Der Stand vor dem laufenden Holen (für „offline“, falls es misslingt).
    previous: Option<CatalogState>,
    job: Option<Job<Outcome>>,
    /// Zeitpunkt des letzten Versuchs (Sekunden seit 1970).
    last_attempt: Option<u64>,
}

impl Catalog {
    /// Liest den Zwischenspeicher in `cache_dir`; fehlt er oder ist er kaputt ⇒ [`CatalogState::Never`].
    pub fn new(cache_dir: PathBuf) -> Catalog {
        let state = read_cache(&cache_dir).unwrap_or(CatalogState::Never);
        Catalog {
            dir: cache_dir,
            state,
            previous: None,
            job: None,
            last_attempt: None,
        }
    }

    pub fn state(&self) -> &CatalogState {
        &self.state
    }

    pub fn is_loading(&self) -> bool {
        self.job.is_some()
    }

    /// Hat der Katalog einen Zwischenspeicher (sonst holt die tägliche Prüfung nicht)?
    pub fn has_cache(&self) -> bool {
        !self.dir.as_os_str().is_empty()
    }

    /// Zeitpunkt des gezeigten (oder während des Holens des vorigen) Stands, falls es einen gibt.
    pub fn fetched_at(&self) -> Option<u64> {
        let state = match &self.state {
            CatalogState::Loading | CatalogState::TooNew(_) | CatalogState::Failed(_) => {
                self.previous.as_ref()?
            }
            s => s,
        };
        match state {
            CatalogState::Ready { fetched_at, .. } => Some(*fetched_at),
            _ => None,
        }
    }

    /// Startet das Holen, wenn `force` oder der letzte Versuch mindestens [`RETRY_SECS`] her ist,
    /// und nicht schon geholt wird.
    pub fn request(&mut self, fetch: RepoFetch, now: u64, force: bool) {
        if self.job.is_some() {
            return;
        }
        let recent = self
            .last_attempt
            .is_some_and(|t| now.saturating_sub(t) < RETRY_SECS);
        if recent && !force {
            return;
        }
        self.last_attempt = Some(now);
        let old = std::mem::replace(&mut self.state, CatalogState::Loading);
        if matches!(old, CatalogState::Ready { .. }) {
            self.previous = Some(old);
        }
        self.job = Some(Job::spawn(move |_| load(fetch)));
    }

    /// Holt das Ergebnis ab, falls fertig; `true`, wenn sich der Zustand geändert hat.
    pub fn poll(&mut self, now: u64) -> bool {
        self.take(now, false)
    }

    /// Wie [`Catalog::poll`], wartet aber auf das Ergebnis (Tests und Abläufe ohne Fenster).
    pub fn wait(&mut self, now: u64) -> bool {
        self.take(now, true)
    }

    fn take(&mut self, now: u64, block: bool) -> bool {
        let Some(job) = &self.job else {
            return false;
        };
        let outcome = match job.fetch(block) {
            Poll::Pending => return false,
            Poll::Ready(o) => o,
            Poll::Lost => Outcome::Failed(tr!("catalog-load-failed")),
        };
        self.job = None;
        match outcome {
            Outcome::Loaded {
                text,
                entries,
                symbols,
                skipped,
            } => {
                log_skipped(&skipped);
                if let Err(e) = write_cache(&self.dir, &text, &symbols, now) {
                    diag::warn(&format!("Plug-in catalog: cache not written: {e}"));
                }
                self.previous = None;
                self.state = CatalogState::Ready {
                    entries,
                    symbols,
                    fetched_at: now,
                    offline: false,
                };
            }
            Outcome::TooNew(n) => {
                diag::warn(&format!("Plug-in catalog: format {n} is too new"));
                self.state = CatalogState::TooNew(n);
            }
            Outcome::Failed(e) => {
                diag::warn(&format!("Plug-in catalog: {}", truncate(&e)));
                self.state = match self.previous.take() {
                    Some(CatalogState::Ready {
                        entries,
                        symbols,
                        fetched_at,
                        ..
                    }) => CatalogState::Ready {
                        entries,
                        symbols,
                        fetched_at,
                        offline: true,
                    },
                    _ => CatalogState::Failed(e),
                };
            }
        }
        true
    }
}

/// Holt Release, Liste und Symbole (im Arbeitsthread).
fn load(fetch: RepoFetch) -> Outcome {
    match try_load(fetch) {
        Ok(o) => o,
        Err(e) => Outcome::Failed(e),
    }
}

fn try_load(fetch: RepoFetch) -> Result<Outcome, String> {
    let body = plugin_repo::get(
        fetch,
        &latest_release_url(),
        MAX_RELEASES_BYTES,
        REQUEST_TIMEOUT,
    )?;
    let release = parse_latest_release(&String::from_utf8_lossy(&body))?;
    if release.catalog.size > MAX_CATALOG_BYTES {
        return Err(tr!("catalog-too-large"));
    }
    let body = plugin_repo::get(
        fetch,
        &release.catalog.url,
        MAX_CATALOG_BYTES,
        REQUEST_TIMEOUT,
    )?;
    let text = String::from_utf8(body).map_err(|_| tr!("catalog-invalid"))?;
    let (entries, skipped) = match parse_catalog(&text) {
        Ok(v) => v,
        Err(CatalogError::TooNew(n)) => return Ok(Outcome::TooNew(n)),
        Err(CatalogError::Invalid(e)) => return Err(e),
    };
    let wanted: BTreeSet<&str> = entries.iter().filter_map(|e| e.symbol.as_deref()).collect();
    let mut symbols = BTreeMap::new();
    for asset in release
        .symbols
        .iter()
        .filter(|a| wanted.contains(a.name.as_str()))
    {
        if symbols.contains_key(&asset.name) {
            continue;
        }
        match plugin_repo::get(fetch, &asset.url, MAX_SYMBOL_BYTES, REQUEST_TIMEOUT) {
            Ok(bytes) => {
                symbols.insert(asset.name.clone(), bytes);
            }
            Err(e) => diag::warn(&format!(
                "Plug-in catalog: symbol not loaded: {}",
                truncate(&e)
            )),
        }
    }
    Ok(Outcome::Loaded {
        text,
        entries,
        symbols,
        skipped,
    })
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

/// Liest den Stand aus `dir`; `None`, wenn er fehlt, kaputt ist oder nicht zusammenpasst.
fn read_cache(dir: &Path) -> Option<CatalogState> {
    if dir.as_os_str().is_empty() {
        return None;
    }
    let index = CacheIndex::parse(&std::fs::read_to_string(dir.join(INDEX_FILE)).ok()?)?;
    let text = std::fs::read_to_string(dir.join(CATALOG_FILE)).ok()?;
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
        let Ok(meta) = std::fs::metadata(dir.join(file)) else {
            continue;
        };
        if meta.len() > MAX_SYMBOL_BYTES {
            continue;
        }
        if let Ok(bytes) = std::fs::read(dir.join(file)) {
            symbols.insert(name.clone(), bytes);
        }
    }
    Some(CatalogState::Ready {
        entries,
        symbols,
        fetched_at: index.fetched_at,
        offline: false,
    })
}

/// Schreibt den Stand nach `dir` (ohne Ordner nichts): Symbole, Liste, zuletzt `stand.json`;
/// danach fallen Symboldateien weg, die nicht mehr dazugehören.
fn write_cache(
    dir: &Path,
    text: &str,
    symbols: &BTreeMap<String, Vec<u8>>,
    now: u64,
) -> Result<(), String> {
    if dir.as_os_str().is_empty() {
        return Ok(());
    }
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let mut index = CacheIndex {
        fetched_at: now,
        catalog_sha256: hex_digest(text.as_bytes()),
        symbols: BTreeMap::new(),
    };
    for (name, bytes) in symbols {
        let file = CacheIndex::symbol_file(name);
        atomic_write_unique(&dir.join(&file), bytes).map_err(|e| e.to_string())?;
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
