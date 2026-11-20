//! Plug-in-Katalog Task 2: Katalog holen, Zustand und Zwischenspeicher. Ohne echtes Netz (eine
//! Nachbildung von `RepoFetch`) und ohne echte Benutzerordner (Temp-Ordner).

mod common;

use planora_app::plugin_catalog::{Catalog, CatalogState};
use planora_app::plugins::Plugins;
use planora_project::config::Config;
use planora_update::catalog::{CATALOG_OWNER, CATALOG_REPO, latest_release_url};
use planora_update::plugin_source::{MAX_RELEASES_BYTES, REQUEST_TIMEOUT};
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

const JETZT: u64 = 1_800_000_000;
const TAG: &str = "katalog-1";

type Answer = Result<(u16, Vec<u8>), String>;

/// Die Adresse des Katalog-Repos ist fest: Tests mit der Nachbildung laufen nacheinander.
static REIHE: Mutex<()> = Mutex::new(());
/// Antworten der Nachbildung (Adresse → Antwort) und die gefragten Adressen mit Grenzen.
static NETZ: Mutex<Vec<(String, Answer)>> = Mutex::new(Vec::new());
static GEFRAGT: Mutex<Vec<(String, u64, Duration)>> = Mutex::new(Vec::new());

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// Sperrt die Reihe und leert die Nachbildung.
fn anfang() -> MutexGuard<'static, ()> {
    let g = lock(&REIHE);
    lock(&NETZ).clear();
    lock(&GEFRAGT).clear();
    g
}

fn antwort(url: &str, a: Answer) {
    let mut netz = lock(&NETZ);
    netz.retain(|(u, _)| u != url);
    netz.push((url.to_string(), a));
}

/// Die Nachbildung: Antwort aus der Tabelle (zu groß ⇒ Fehler wie beim echten Weg), sonst 404.
fn netz(url: &str, max: u64, timeout: Duration) -> Answer {
    lock(&GEFRAGT).push((url.to_string(), max, timeout));
    let found = lock(&NETZ)
        .iter()
        .find(|(u, _)| u == url)
        .map(|(_, a)| a.clone());
    match found {
        Some(Ok((_, body))) if body.len() as u64 > max => Err("zu groß".to_string()),
        Some(a) => a,
        None => Ok((404, b"Not Found".to_vec())),
    }
}

/// Kein Netz.
fn netz_aus(url: &str, _max: u64, _timeout: Duration) -> Answer {
    lock(&GEFRAGT).push((url.to_string(), 0, Duration::ZERO));
    Err("keine Verbindung".to_string())
}

fn gefragt(url: &str) -> usize {
    lock(&GEFRAGT).iter().filter(|(u, _, _)| u == url).count()
}

fn asset_url(name: &str) -> String {
    format!("https://github.com/{CATALOG_OWNER}/{CATALOG_REPO}/releases/download/{TAG}/{name}")
}

/// `katalog.json` mit Einträgen (Kennung, Symbol) im Format `format`.
fn katalog(format: u32, entries: &[(&str, Option<&str>)]) -> String {
    let list: Vec<String> = entries
        .iter()
        .map(|(id, sym)| {
            let sym = sym
                .map(|s| format!(", \"symbol\": \"{s}\""))
                .unwrap_or_default();
            format!(
                "{{ \"id\": \"{id}\", \"name\": {{ \"de\": \"N {id}\", \"en\": \"N {id}\" }}, \
                 \"kategorie\": \"test\", \"repo\": \"https://github.com/besitzer/{id}\"{sym} }}"
            )
        })
        .collect();
    format!(
        "{{ \"format\": {format}, \"eintraege\": [{}] }}",
        list.join(",")
    )
}

/// Legt ein Katalog-Release mit `katalog.json` = `text` und den Symbolen in die Nachbildung.
fn release(text: &str, symbols: &[(&str, &[u8])]) {
    let mut assets = vec![format!(
        "{{ \"name\": \"katalog.json\", \"browser_download_url\": \"{}\", \"size\": {} }}",
        asset_url("katalog.json"),
        text.len()
    )];
    for (name, bytes) in symbols {
        assets.push(format!(
            "{{ \"name\": \"{name}\", \"browser_download_url\": \"{}\", \"size\": {} }}",
            asset_url(name),
            bytes.len()
        ));
        antwort(&asset_url(name), Ok((200, bytes.to_vec())));
    }
    let json = format!(
        "{{ \"tag_name\": \"{TAG}\", \"draft\": false, \"assets\": [{}] }}",
        assets.join(",")
    );
    antwort(&latest_release_url(), Ok((200, json.into_bytes())));
    antwort(
        &asset_url("katalog.json"),
        Ok((200, text.as_bytes().to_vec())),
    );
}

fn ordner(name: &str) -> PathBuf {
    common::temp_root(&format!("planora-plugin-catalog-{name}")).join("katalog")
}

fn ready(c: &Catalog) -> (&Vec<planora_update::catalog::CatalogEntry>, u64, bool) {
    match c.state() {
        CatalogState::Ready {
            entries,
            fetched_at,
            offline,
            ..
        } => (entries, *fetched_at, *offline),
        other => panic!("nicht bereit: {other:?}"),
    }
}

/// Holt einmal und wartet auf das Ergebnis.
fn holen(c: &mut Catalog, fetch: planora_app::plugin_repo::RepoFetch, now: u64) {
    c.request(fetch, now, true);
    assert_eq!(c.state(), &CatalogState::Loading);
    assert!(c.wait(now));
}

#[test]
fn loads_and_caches() {
    let _g = anfang();
    let dir = ordner("laden");
    let png: &[u8] = b"\x89PNG-bild";
    release(
        &katalog(1, &[("planora.a", Some("a.png")), ("planora.b", None)]),
        &[("a.png", png)],
    );
    let mut c = Catalog::new(dir.clone());
    assert_eq!(c.state(), &CatalogState::Never);
    holen(&mut c, netz, JETZT);
    let (entries, fetched_at, offline) = ready(&c);
    assert_eq!(entries.len(), 2);
    assert_eq!(fetched_at, JETZT);
    assert!(!offline);
    let CatalogState::Ready { symbols, .. } = c.state() else {
        unreachable!()
    };
    assert_eq!(symbols.get("a.png").map(Vec::as_slice), Some(png));
    // Grenzen beim Laden: Release-Liste 4 MiB, katalog.json 1 MiB, Symbol 256 KiB, je 30 s.
    let calls = lock(&GEFRAGT).clone();
    let limit = |u: &str| {
        calls
            .iter()
            .find(|(x, _, _)| x == u)
            .map(|(_, m, t)| (*m, *t))
    };
    assert_eq!(
        limit(&latest_release_url()),
        Some((MAX_RELEASES_BYTES, REQUEST_TIMEOUT))
    );
    assert_eq!(
        limit(&asset_url("katalog.json")),
        Some((1 << 20, REQUEST_TIMEOUT))
    );
    assert_eq!(
        limit(&asset_url("a.png")),
        Some((256 << 10, REQUEST_TIMEOUT))
    );
    // Dateien im Cacheordner; Symbol nie unter dem Namen aus dem Katalog.
    assert!(dir.join("katalog.json").is_file());
    assert!(dir.join("stand.json").is_file());
    assert!(!dir.join("a.png").exists());
    let files: Vec<String> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(files.len(), 3, "{files:?}");
    assert!(
        files
            .iter()
            .any(|f| f.len() == 64 + 4 && f.ends_with(".png"))
    );
    // Neu gelesen: derselbe Stand.
    let again = Catalog::new(dir);
    assert_eq!(again.state(), c.state());
}

#[test]
fn offline_uses_cache() {
    let _g = anfang();
    let dir = ordner("offline");
    release(&katalog(1, &[("planora.a", None)]), &[]);
    let mut c = Catalog::new(dir.clone());
    holen(&mut c, netz, JETZT);
    holen(&mut c, netz_aus, JETZT + 100);
    let (entries, fetched_at, offline) = ready(&c);
    assert_eq!(entries.len(), 1);
    assert_eq!(fetched_at, JETZT);
    assert!(offline);
    // Auch frisch aus dem Zwischenspeicher gelesen, dann ohne Netz.
    let mut fresh = Catalog::new(dir);
    holen(&mut fresh, netz_aus, JETZT + 200);
    let (_, fetched_at, offline) = ready(&fresh);
    assert_eq!((fetched_at, offline), (JETZT, true));
}

#[test]
fn never_loaded_fails() {
    let _g = anfang();
    let mut c = Catalog::new(ordner("nie"));
    holen(&mut c, netz_aus, JETZT);
    assert!(matches!(c.state(), CatalogState::Failed(t) if !t.is_empty()));
}

#[test]
fn broken_cache_is_never() {
    let _g = anfang();
    let dir = ordner("kaputt");
    release(&katalog(1, &[("planora.a", None)]), &[]);
    let mut c = Catalog::new(dir.clone());
    holen(&mut c, netz, JETZT);
    let text = std::fs::read_to_string(dir.join("katalog.json")).unwrap();
    std::fs::write(dir.join("katalog.json"), &text[..text.len() / 2]).unwrap();
    assert_eq!(Catalog::new(dir.clone()).state(), &CatalogState::Never);
    // Auch ein kaputtes stand.json.
    std::fs::write(dir.join("katalog.json"), &text).unwrap();
    assert!(matches!(
        Catalog::new(dir.clone()).state(),
        CatalogState::Ready { .. }
    ));
    std::fs::write(dir.join("stand.json"), "{ \"fetched_at\": ").unwrap();
    assert_eq!(Catalog::new(dir).state(), &CatalogState::Never);
}

#[test]
fn throttled_ten_minutes() {
    let _g = anfang();
    release(&katalog(1, &[("planora.a", None)]), &[]);
    let mut c = Catalog::new(ordner("drossel"));
    c.request(netz, JETZT, false);
    assert!(c.wait(JETZT));
    assert_eq!(gefragt(&latest_release_url()), 1);
    c.request(netz, JETZT + 300, false);
    assert!(!c.wait(JETZT + 300));
    assert_eq!(gefragt(&latest_release_url()), 1);
    c.request(netz, JETZT + 601, false);
    assert!(c.wait(JETZT + 601));
    assert_eq!(gefragt(&latest_release_url()), 2);
    // Mit `force` immer.
    c.request(netz, JETZT + 602, true);
    assert!(c.wait(JETZT + 602));
    assert_eq!(gefragt(&latest_release_url()), 3);
}

#[test]
fn too_new_format() {
    let _g = anfang();
    let dir = ordner("neuer");
    release(&katalog(1, &[("planora.a", None)]), &[]);
    let mut c = Catalog::new(dir.clone());
    holen(&mut c, netz, JETZT);
    let before = std::fs::read(dir.join("katalog.json")).unwrap();
    let stand = std::fs::read(dir.join("stand.json")).unwrap();
    release(&katalog(2, &[("planora.b", None)]), &[]);
    holen(&mut c, netz, JETZT + 700);
    assert_eq!(c.state(), &CatalogState::TooNew(2));
    assert_eq!(std::fs::read(dir.join("katalog.json")).unwrap(), before);
    assert_eq!(std::fs::read(dir.join("stand.json")).unwrap(), stand);
}

#[test]
fn only_named_symbols_loaded() {
    let _g = anfang();
    let svg: &[u8] = b"<svg/>";
    release(
        &katalog(
            1,
            &[
                ("planora.a", Some("b.svg")),
                ("planora.c", Some("fehlt.png")),
            ],
        ),
        &[("a.png", b"a"), ("b.svg", svg), ("c.png", b"c")],
    );
    let mut c = Catalog::new(ordner("symbole"));
    holen(&mut c, netz, JETZT);
    let downloads = lock(&GEFRAGT)
        .iter()
        .filter(|(u, _, _)| u.ends_with(".png") || u.ends_with(".svg"))
        .count();
    assert_eq!(downloads, 1);
    assert_eq!(gefragt(&asset_url("b.svg")), 1);
    let CatalogState::Ready { symbols, .. } = c.state() else {
        panic!("{:?}", c.state())
    };
    assert_eq!(symbols.len(), 1);
    assert_eq!(symbols.get("b.svg").map(Vec::as_slice), Some(svg));
}

#[test]
fn catalog_too_large_rejected_up_front() {
    let _g = anfang();
    release(&katalog(1, &[("planora.a", None)]), &[]);
    // Laut Release größer als 1 MiB: gar nicht erst laden.
    let json = format!(
        "{{ \"tag_name\": \"{TAG}\", \"draft\": false, \"assets\": [{{ \"name\": \"katalog.json\", \
         \"browser_download_url\": \"{}\", \"size\": {} }}] }}",
        asset_url("katalog.json"),
        (1u64 << 20) + 1
    );
    antwort(&latest_release_url(), Ok((200, json.into_bytes())));
    let mut c = Catalog::new(ordner("gross"));
    holen(&mut c, netz, JETZT);
    assert!(matches!(c.state(), CatalogState::Failed(_)));
    assert_eq!(gefragt(&asset_url("katalog.json")), 0);
}

#[test]
fn daily_check_requests_catalog() {
    let _g = anfang();
    release(&katalog(1, &[("planora.a", None)]), &[]);
    // Aus: kein Abruf.
    let mut plugins = Plugins::empty();
    plugins.repo_fetch = netz;
    plugins.catalog = Catalog::new(ordner("taeglich-aus"));
    let mut config = Config {
        update_check: false,
        last_plugin_check: None,
        ..Default::default()
    };
    plugins.auto_check(&mut config, JETZT);
    assert!(!plugins.catalog.wait(JETZT));
    assert_eq!(gefragt(&latest_release_url()), 0);
    // An: genau ein Abruf, auch ohne installierte Plug-ins.
    let mut plugins = Plugins::empty();
    plugins.repo_fetch = netz;
    plugins.catalog = Catalog::new(ordner("taeglich-an"));
    config.update_check = true;
    plugins.auto_check(&mut config, JETZT);
    assert!(plugins.catalog.wait(JETZT));
    assert_eq!(gefragt(&latest_release_url()), 1);
    assert!(matches!(
        plugins.catalog.state(),
        CatalogState::Ready { .. }
    ));
    // Ohne installierte Plug-ins bleibt `last_plugin_check` (und damit die Einstellungsdatei) unberührt.
    assert_eq!(config.last_plugin_check, None);
    // Gleich danach noch einmal: nicht fällig, kein weiterer Abruf.
    plugins.auto_check(&mut config, JETZT + 60);
    assert!(!plugins.catalog.wait(JETZT + 60));
    assert_eq!(gefragt(&latest_release_url()), 1);
    // Ein Tag später: wieder fällig.
    plugins.auto_check(&mut config, JETZT + 24 * 3600);
    assert!(plugins.catalog.wait(JETZT + 24 * 3600));
    assert_eq!(gefragt(&latest_release_url()), 2);
    // Ohne Zwischenspeicher (Plugins::empty, Test-App) nie automatisch: kein Netz.
    let mut plugins = Plugins::empty();
    plugins.repo_fetch = netz;
    plugins.auto_check(&mut config, JETZT + 48 * 3600);
    assert!(!plugins.catalog.wait(JETZT + 48 * 3600));
    assert_eq!(gefragt(&latest_release_url()), 2);
}
