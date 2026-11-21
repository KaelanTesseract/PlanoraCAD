//! Plug-ins Etappe 1, Task 8: Installieren und Aktualisieren über den Repo-Link. Das Netz ist
//! eine Nachbildung in [`Plugins::repo_fetch`] (Antworten je Adresse aus einer Tabelle; jeder
//! Test benutzt eigene Besitzernamen, die Tests laufen nebeneinander). Fundorte und Konfiguration
//! liegen in Temp; gezeichnet wird bei 1920 × 1080.

mod common;

use common::{Seen, collect_seen, find, join};
use egui::{Event, Modifiers, PointerButton, Pos2, Rect, vec2};
use planora_app::addins_window::AddinsWindow;
use planora_app::plugins::{self, Plugins, check_downloaded};
use planora_app::workspace::Workspace;
use planora_app::{fonts, prompts, theme};
use planora_plugin_host::EntryState;
use planora_plugin_host::manifest::Right;
use planora_plugin_host::package::Package;
use planora_plugin_host::places::{self as plugin_places, Places, Provenance};
use planora_plugin_host::rights::Grant;
use planora_project::config::{Config, PluginConfig};
use planora_update::plugin_source::{RepoLink, parse_plugin_releases};
use planora_update::sha256::hex_digest;
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant};

const HALLO: &str = "planora.beispiel-hallo";
const FAELLE: &str = "planora.test-faelle";

type Answer = Result<(u16, Vec<u8>), String>;

/// Antworten der Nachbildung (Adresse → Antwort) und die gefragten Adressen.
static NETZ: Mutex<Vec<(String, Answer)>> = Mutex::new(Vec::new());
static GEFRAGT: Mutex<Vec<String>> = Mutex::new(Vec::new());
/// Die Zeitgrenze je gefragter Adresse.
static ZEITEN: Mutex<Vec<(String, Duration)>> = Mutex::new(Vec::new());

fn antwort(url: &str, a: Answer) {
    let mut netz = NETZ.lock().unwrap();
    netz.retain(|(u, _)| u != url);
    netz.push((url.to_string(), a));
}

/// Die Nachbildung des Netzwegs (nach allen Weiterleitungen): Antwort aus der Tabelle, sonst 404.
fn netz(url: &str, max: u64, timeout: Duration) -> Answer {
    GEFRAGT.lock().unwrap().push(url.to_string());
    ZEITEN.lock().unwrap().push((url.to_string(), timeout));
    let found = NETZ
        .lock()
        .unwrap()
        .iter()
        .find(|(u, _)| u == url)
        .map(|(_, a)| a.clone());
    match found {
        Some(Ok((_, body))) if body.len() as u64 > max => Err("zu groß".to_string()),
        Some(a) => a,
        None => Ok((404, b"Not Found".to_vec())),
    }
}

/// Wie [`netz`], ein Paket aber erst nach einer halben Sekunde (so ist „Wird geholt…“ zu sehen).
fn netz_langsam(url: &str, max: u64, timeout: Duration) -> Answer {
    if url.ends_with(".pcplugin") {
        std::thread::sleep(Duration::from_millis(500));
    }
    netz(url, max, timeout)
}

/// Die gefragten Adressen, die mit `prefix` beginnen.
/// Die Zeitgrenzen, mit denen genau `url` gefragt wurde.
fn zeiten(url: &str) -> Vec<Duration> {
    ZEITEN
        .lock()
        .unwrap()
        .iter()
        .filter(|(u, _)| u == url)
        .map(|(_, t)| *t)
        .collect()
}

fn gefragt(prefix: &str) -> Vec<String> {
    GEFRAGT
        .lock()
        .unwrap()
        .iter()
        .filter(|u| u.starts_with(prefix))
        .cloned()
        .collect()
}

fn data_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../planora-plugin-host/tests/data")
}

fn datei(name: &str) -> Vec<u8> {
    std::fs::read(data_dir().join(format!("{name}.pcplugin"))).unwrap()
}

/// Ein Paket aus dem Wasm von beispiel-hallo mit eigenem Manifest (Kennung, Version, Rechte).
fn paket_bauen(id: &str, version: &str, rights: &[&str]) -> Vec<u8> {
    let hallo = Package::read(&datei("beispiel-hallo")).unwrap();
    let mut toml = format!(
        "id = \"{id}\"\nname = \"Beispiel-Hallo\"\nversion = \"{version}\"\napi = \"0.1\"\n\
         author = \"Dennis Guse\"\nlicense = \"GPL-3.0-only\"\ndescription = \"Test\"\n"
    );
    if !rights.is_empty() {
        let list: Vec<String> = rights.iter().map(|r| format!("\"{r}\"")).collect();
        toml.push_str(&format!("rights = [{}]\n", list.join(", ")));
    }
    if rights.contains(&"net") {
        toml.push_str("net-hosts = [\"example.org\"]\n");
    }
    let mut zip = planora_exchange::ZipWriter::new(Vec::new());
    zip.add_entry("plugin.toml", &|w| w.write_all(toml.as_bytes()))
        .unwrap();
    zip.add_entry("plugin.wasm", &|w| w.write_all(&hallo.wasm))
        .unwrap();
    for (lang, text) in &hallo.languages {
        zip.add_entry(&format!("languages/{lang}.ftl"), &|w| {
            w.write_all(text.as_bytes())
        })
        .unwrap();
    }
    let bytes = zip.finish().unwrap();
    // Gegenprobe: das Paket ist lesbar.
    Package::read(&bytes).unwrap();
    bytes
}

fn link(owner: &str) -> String {
    format!("https://github.com/{owner}/hallo")
}

fn releases_url(owner: &str) -> String {
    format!("https://api.github.com/repos/{owner}/hallo/releases?per_page=100")
}

fn asset_url(owner: &str, tag: &str, name: &str) -> String {
    format!("https://github.com/{owner}/hallo/releases/download/{tag}/{name}")
}

/// Ein Release `tag` mit dem Paket `bytes` (und, mit `sum`, der Prüfsummendatei mit diesem
/// Inhalt) in die Nachbildung; gibt das JSON des Releases zurück.
fn release(owner: &str, tag: &str, bytes: &[u8], sum: Option<String>) -> String {
    let name = "hallo.pcplugin";
    let pkg_url = asset_url(owner, tag, name);
    antwort(&pkg_url, Ok((200, bytes.to_vec())));
    let mut assets = vec![format!(
        r#"{{"name":"{name}","browser_download_url":"{pkg_url}","size":{}}}"#,
        bytes.len()
    )];
    if let Some(sum) = sum {
        let sum_url = asset_url(owner, tag, &format!("{name}.sha256"));
        antwort(&sum_url, Ok((200, sum.into_bytes())));
        assets.push(format!(
            r#"{{"name":"{name}.sha256","browser_download_url":"{sum_url}","size":80}}"#
        ));
    }
    format!(
        r#"{{"tag_name":"{tag}","draft":false,"prerelease":false,"body":"Neu in {tag}","assets":[{}]}}"#,
        assets.join(",")
    )
}

fn richtige_summe(bytes: &[u8]) -> Option<String> {
    Some(format!("{}  hallo.pcplugin\n", hex_digest(bytes)))
}

fn releases(owner: &str, list: &[String]) {
    antwort(
        &releases_url(owner),
        Ok((200, format!("[{}]", list.join(",")).into_bytes())),
    );
}

/// Fundorte in Temp; `installiert`: (Paketbytes, Herkunft) vom Nutzer installiert.
fn orte(name: &str, installiert: &[(Vec<u8>, Option<Provenance>)]) -> Places {
    let root = common::temp_root(&format!("planora-plugin-repo-{name}"));
    let p = Places {
        bundled: vec![root.join("mitgeliefert")],
        user: root.join("benutzer"),
        cache: root.join("cache"),
    };
    for (bytes, herkunft) in installiert {
        let pkg = Package::read(bytes).unwrap();
        plugin_places::install(&p, &pkg, herkunft.as_ref()).unwrap();
    }
    std::fs::create_dir_all(&p.user).unwrap();
    p
}

fn herkunft(owner: &str, tag: &str) -> Option<Provenance> {
    Some(Provenance {
        link: link(owner),
        tag: tag.to_string(),
        catalog: false,
    })
}

fn fresh_ctx() -> egui::Context {
    let ctx = egui::Context::default();
    ctx.set_fonts(fonts::definitions());
    theme::apply(&ctx, true);
    ctx.all_styles_mut(|s| s.animation_time = 0.0);
    ctx
}

struct H {
    ws: Workspace,
    ctx: egui::Context,
    w: AddinsWindow,
}

fn harness(name: &str, places: Places, config: Config) -> H {
    let mut ws = Workspace::new(None, config);
    let cfg = common::temp_root(&format!("planora-plugin-repo-cfg-{name}"));
    std::fs::create_dir_all(&cfg).unwrap();
    ws.config_path = cfg.join("config.json");
    ws.plugins = Plugins::new(places, &ws.config);
    ws.plugins.host.set_language("de");
    ws.plugins.repo_fetch = netz;
    let mut w = AddinsWindow::default();
    w.open();
    let mut h = H {
        ws,
        ctx: fresh_ctx(),
        w,
    };
    warten(&mut h);
    h
}

fn bild(h: &mut H, events: Vec<Event>) -> Vec<Seen> {
    let raw = egui::RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1920.0, 1080.0))),
        events,
        ..Default::default()
    };
    let (ws, w) = (&mut h.ws, &mut h.w);
    let out = h.ctx.run_ui(raw, |ui| {
        let ctx = ui.ctx().clone();
        prompts::show(&ctx, ws);
        w.show_in(&ctx, ws);
        plugins::show(&ctx, ws);
    });
    let seen = collect_seen(&out.shapes);
    out.drop_without_applying_deltas();
    seen
}

/// Bilder, bis nichts mehr lädt, geholt oder geprüft wird (höchstens 60 s).
fn warten(h: &mut H) -> Vec<Seen> {
    let start = Instant::now();
    loop {
        bild(h, vec![]);
        let p = &h.ws.plugins;
        let busy = p
            .host
            .entries()
            .iter()
            .any(|e| e.state == EntryState::Loading)
            || p.is_reading()
            || p.is_fetching()
            || p.is_checking();
        if !busy {
            bild(h, vec![]);
            return bild(h, vec![]);
        }
        assert!(start.elapsed() < Duration::from_secs(60), "dauert zu lange");
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn klick(h: &mut H, seen: &[Seen], text: &str) -> Vec<Seen> {
    let at = seen
        .iter()
        .rev()
        .find(|s| s.text == text)
        .unwrap_or_else(|| panic!("„{text}“ fehlt: {}", join(seen)))
        .rect
        .center();
    let b = |pressed| Event::PointerButton {
        pos: at,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    };
    bild(h, vec![Event::PointerMoved(at)]);
    bild(h, vec![b(true)]);
    bild(h, vec![b(false)]);
    bild(h, vec![])
}

fn has(seen: &[Seen], text: &str) -> bool {
    find(seen, text).is_some()
}

fn active_version(h: &H, id: &str) -> Option<String> {
    let e = h.ws.plugins.host.entries().iter().find(|e| e.id == id)?;
    e.active.map(|a| e.found[a].manifest.version.to_string())
}

fn grant(h: &H, id: &str, r: Right) -> Option<Grant> {
    h.ws.plugins
        .host
        .entries()
        .iter()
        .find(|e| e.id == id)
        .and_then(|e| e.rights.state(r))
}

#[test]
fn install_from_link_shows_dialog_then_runs() {
    let owner = "inst-ok";
    let bytes = datei("beispiel-hallo");
    releases(
        owner,
        &[release(owner, "v0.1.0", &bytes, richtige_summe(&bytes))],
    );
    let mut h = harness("inst-ok", orte("inst-ok", &[]), Config::default());
    let seen = bild(&mut h, vec![]);
    for text in ["Aus Datei…", "Holen"] {
        assert!(has(&seen, text), "„{text}“ fehlt: {}", join(&seen));
    }
    h.ws.plugins.link_text = format!("{}/", link(owner));
    klick(&mut h, &seen, "Holen");
    let seen = warten(&mut h);
    assert!(h.ws.plugins.install.is_some(), "{}", join(&seen));
    for text in [
        "Plug-in installieren",
        "Beispiel-Hallo",
        "Version: 0.1.0",
        "Lizenz: GPL-3.0-only",
    ] {
        assert!(has(&seen, text), "„{text}“ fehlt: {}", join(&seen));
    }
    assert!(
        seen.iter()
            .any(|s| s.text.contains(&format!("Quelle: {}", link(owner)))),
        "{}",
        join(&seen)
    );
    // Noch nichts auf der Platte.
    let user = h.ws.plugins.host.places().user.clone();
    assert!(!user.join(HALLO).exists());
    klick(&mut h, &seen, "Installieren");
    warten(&mut h);
    assert!(h.ws.plugins.install.is_none());
    assert!(
        h.ws.plugins
            .host
            .commands()
            .iter()
            .any(|c| c.full_id == "planora.beispiel-hallo.hallo"),
        "Befehl im Menü"
    );
    assert_eq!(
        plugin_places::read_provenance(&user.join(HALLO)),
        Some(Provenance {
            link: link(owner),
            tag: "v0.1.0".to_string(),
            catalog: false,
        })
    );
    assert_eq!(
        gefragt(&format!("https://api.github.com/repos/{owner}/")),
        [releases_url(owner)]
    );
}

/// Aus dem Katalog installiert: derselbe Weg wie über den Link, in der Herkunft aber vermerkt.
#[test]
fn install_from_catalog_marks_provenance() {
    let owner = "inst-katalog";
    let bytes = datei("beispiel-hallo");
    releases(
        owner,
        &[release(owner, "v0.1.0", &bytes, richtige_summe(&bytes))],
    );
    let mut h = harness("inst-katalog", orte("inst-katalog", &[]), Config::default());
    let entry = planora_update::catalog::CatalogEntry {
        id: HALLO.to_string(),
        name: BTreeMap::new(),
        description: BTreeMap::new(),
        category: String::new(),
        repo: planora_update::plugin_source::parse_repo_link(&link(owner)).unwrap(),
        symbol: None,
    };
    h.ws.plugins.install_from_catalog(&entry);
    assert!(h.ws.plugins.is_fetching());
    let seen = warten(&mut h);
    assert!(h.ws.plugins.install.is_some(), "{}", join(&seen));
    klick(&mut h, &seen, "Installieren");
    warten(&mut h);
    let user = h.ws.plugins.host.places().user.clone();
    assert_eq!(
        plugin_places::read_provenance(&user.join(HALLO)),
        Some(Provenance {
            link: link(owner),
            tag: "v0.1.0".to_string(),
            catalog: true,
        })
    );
}

/// Der Katalog nennt `planora.elektronik`, das Release enthält aber `foo.bar`: kein Dialog,
/// nichts installiert, der Fehler steht im Fenster.
#[test]
fn install_from_catalog_rejects_foreign_id() {
    let owner = "inst-katalog-fremd";
    let bytes = paket_bauen("foo.bar", "0.1.0", &[]);
    releases(
        owner,
        &[release(owner, "v0.1.0", &bytes, richtige_summe(&bytes))],
    );
    let mut h = harness(
        "inst-katalog-fremd",
        orte("inst-katalog-fremd", &[]),
        Config::default(),
    );
    let entry = planora_update::catalog::CatalogEntry {
        id: "planora.elektronik".to_string(),
        name: BTreeMap::new(),
        description: BTreeMap::new(),
        category: String::new(),
        repo: planora_update::plugin_source::parse_repo_link(&link(owner)).unwrap(),
        symbol: None,
    };
    h.ws.plugins.install_from_catalog(&entry);
    let seen = warten(&mut h);
    let expected = "Das Release v0.1.0 enthält ein anderes Plug-in (foo.bar).";
    assert_eq!(h.ws.plugins.note.as_deref(), Some(expected));
    assert!(has(&seen, expected), "{}", join(&seen));
    assert!(h.ws.plugins.install.is_none(), "kein Dialog");
    let user = h.ws.plugins.host.places().user.clone();
    assert!(!user.join("foo.bar").exists(), "nichts installiert");
    assert!(
        !user.join("planora.elektronik").exists(),
        "nichts installiert"
    );
}

#[test]
fn install_from_link_errors() {
    let bytes = datei("beispiel-hallo");
    let falsch = Some(format!("{}  hallo.pcplugin", "0".repeat(64)));
    let cases: Vec<(&str, Einrichten, String)> = vec![
        (
            "err-leer",
            Box::new(|o: &str| releases(o, &[])),
            "In diesem Repo gibt es kein Release mit einer .pcplugin-Datei.".to_string(),
        ),
        (
            "err-summe",
            Box::new(move |o: &str| releases(o, &[release(o, "v0.1.0", &bytes, falsch.clone())])),
            "Die Prüfsumme stimmt nicht.".to_string(),
        ),
        (
            "err-tag",
            Box::new(|o: &str| {
                let b = datei("beispiel-hallo");
                releases(o, &[release(o, "v0.2.0", &b, None)])
            }),
            "Das Release v0.2.0 passt nicht zur Plug-in-Version 0.1.0.".to_string(),
        ),
        (
            "err-netz",
            Box::new(|o: &str| antwort(&releases_url(o), Err("Zeitüberschreitung".to_string()))),
            "Keine Verbindung zu GitHub: Zeitüberschreitung".to_string(),
        ),
    ];
    let mut h = harness("err", orte("err", &[]), Config::default());
    let user = h.ws.plugins.host.places().user.clone();
    // Ungültiger Link: gar keine Anfrage.
    h.ws.plugins
        .install_from_link("https://gitlab.com/err-ungueltig/hallo");
    assert_eq!(
        h.ws.plugins.note.as_deref(),
        Some("Das ist kein GitHub-Repo-Link (https://github.com/besitzer/repo).")
    );
    assert!(!h.ws.plugins.is_fetching());
    let seen = warten(&mut h);
    assert!(has(
        &seen,
        "Das ist kein GitHub-Repo-Link (https://github.com/besitzer/repo)."
    ));
    assert!(gefragt("https://api.github.com/repos/err-ungueltig").is_empty());
    for (owner, setup, expected) in cases {
        setup(owner);
        h.ws.plugins.install_from_link(&link(owner));
        let seen = warten(&mut h);
        assert_eq!(
            h.ws.plugins.note.as_deref(),
            Some(expected.as_str()),
            "{owner}"
        );
        assert!(has(&seen, &expected), "{owner}: {}", join(&seen));
        assert!(h.ws.plugins.install.is_none(), "{owner}: kein Dialog");
        assert!(!user.join(HALLO).exists(), "{owner}: nichts installiert");
    }
}

/// Paket 0.1.0 (Recht `model-read`, erteilt) mit Link von `owner`, gestartet.
fn installiert_010(name: &str, owner: &str) -> H {
    let alt = paket_bauen(HALLO, "0.1.0", &["model-read"]);
    let mut config = Config::default();
    config.plugins.insert(
        HALLO.to_string(),
        PluginConfig {
            enabled: true,
            rights: BTreeMap::from([("model-read".to_string(), "allowed".to_string())]),
            author: Some("Dennis Guse".to_string()),
            net_hosts: Vec::new(),
            source: Some(link(owner)),
        },
    );
    let h = harness(
        name,
        orte(name, &[(alt, herkunft(owner, "v0.1.0"))]),
        config,
    );
    assert_eq!(active_version(&h, HALLO).as_deref(), Some("0.1.0"));
    assert_eq!(grant(&h, HALLO, Right::ModelRead), Some(Grant::Allowed));
    h
}

#[test]
fn update_found_and_applied() {
    let owner = "upd-ok";
    let neu = paket_bauen(HALLO, "0.2.0", &["model-read", "net"]);
    releases(
        owner,
        &[release(owner, "v0.2.0", &neu, richtige_summe(&neu))],
    );
    let mut h = installiert_010("upd-ok", owner);
    let before = h.ws.plugins.host.generation(HALLO).unwrap();
    h.ws.plugins.check_updates(true);
    let seen = warten(&mut h);
    let updates = h.ws.plugins.updates();
    assert_eq!(updates.len(), 1, "{updates:?}");
    assert_eq!(updates[0].id, HALLO);
    assert_eq!(updates[0].from.to_string(), "0.1.0");
    assert_eq!(updates[0].to.version.to_string(), "0.2.0");
    assert!(
        has(&seen, "Für 1 Plug-in gibt es ein Update."),
        "{}",
        join(&seen)
    );
    // Die Prüfung holt nur die Release-Liste (30 s); das Paket erst mit „Aktualisieren“.
    let pkg_url = asset_url(owner, "v0.2.0", "hallo.pcplugin");
    assert!(gefragt(&pkg_url).is_empty(), "Paket schon geladen");
    assert_eq!(zeiten(&releases_url(owner)), [Duration::from_secs(30)]);
    assert!(h.ws.plugins.ready_update(HALLO).is_none());
    assert!(
        !h.ws.plugins.apply_update(HALLO, &mut h.ws.config),
        "ohne Paket"
    );
    h.ws.plugins.open_update(HALLO);
    warten(&mut h);
    assert_eq!(zeiten(&pkg_url), [Duration::from_secs(300)], "Paket: 5 min");
    assert_eq!(
        h.ws.plugins.updates()[0].new_rights,
        BTreeSet::from([Right::Net])
    );
    assert!(h.ws.plugins.apply_update(HALLO, &mut h.ws.config));
    // Gleich danach (vor dem nächsten Bild) steht der neue Stand in der Konfiguration.
    assert_eq!(
        h.ws.config.plugins[HALLO].net_hosts,
        ["example.org"],
        "kein veralteter Stand"
    );
    warten(&mut h);
    assert_eq!(active_version(&h, HALLO).as_deref(), Some("0.2.0"));
    let after = h.ws.plugins.host.generation(HALLO).unwrap();
    assert!(after > before, "neue Generation: {before} → {after}");
    assert_eq!(grant(&h, HALLO, Right::Net), None, "net noch nicht erteilt");
    assert_eq!(grant(&h, HALLO, Right::ModelRead), Some(Grant::Allowed));
    assert!(h.ws.plugins.updates().is_empty());
    let user = h.ws.plugins.host.places().user.clone();
    assert_eq!(
        plugin_places::read_provenance(&user.join(HALLO)).map(|p| p.tag),
        Some("v0.2.0".to_string())
    );
    // Der Stand ist gleich in der Konfiguration (mit Autor und Rechnern des neuen Manifests).
    let c = &h.ws.config.plugins[HALLO];
    assert_eq!(c.author.as_deref(), Some("Dennis Guse"));
    assert_eq!(c.net_hosts, ["example.org"]);
    assert_eq!(
        c.rights,
        BTreeMap::from([("model-read".to_string(), "allowed".to_string())])
    );
}

#[test]
fn update_dialog_shows_versions_notes_and_new_rights() {
    let owner = "upd-dialog";
    let neu = paket_bauen(HALLO, "0.2.0", &["model-read", "net"]);
    releases(owner, &[release(owner, "v0.2.0", &neu, None)]);
    let mut h = installiert_010("upd-dialog", owner);
    h.ws.plugins.repo_fetch = netz_langsam;
    h.ws.plugins.check_updates(true);
    let seen = warten(&mut h);
    let seen = klick(&mut h, &seen, "Anzeigen");
    assert!(
        h.ws.plugins.take_show_window(),
        "„Anzeigen“ öffnet das Fenster „Zusatzmodule“"
    );
    // Gewählt (rechts) und „Aktualisieren auf 0.2.0“ gleich zu sehen.
    h.w.select(HALLO);
    bild(&mut h, vec![]);
    let seen2 = bild(&mut h, vec![]);
    let _ = seen;
    for text in ["Nach Updates suchen", "Aktualisieren auf 0.2.0"] {
        assert!(has(&seen2, text), "„{text}“ fehlt: {}", join(&seen2));
    }
    assert!(
        seen2
            .iter()
            .any(|s| s.text.contains(&format!("Quelle: {}", link(owner)))),
        "{}",
        join(&seen2)
    );
    let seen = klick(&mut h, &seen2, "Aktualisieren auf 0.2.0");
    // Erst jetzt wird das Paket geholt; solange nur die Versionen.
    for text in [
        "Plug-in aktualisieren",
        "Beispiel-Hallo: Version 0.1.0 → 0.2.0",
        "Wird geholt…",
    ] {
        assert!(has(&seen, text), "„{text}“ fehlt: {}", join(&seen));
    }
    assert!(!has(&seen, "Neu in v0.2.0"));
    let seen = warten(&mut h);
    assert!(!has(&seen, "Wird geholt…"));
    for text in [
        "Plug-in aktualisieren",
        "Beispiel-Hallo: Version 0.1.0 → 0.2.0",
        "Neu in v0.2.0",
        "Neu verlangte Rechte: Netz (example.org)",
        "Das Programm fragt beim ersten Gebrauch nach.",
    ] {
        assert!(has(&seen, text), "„{text}“ fehlt: {}", join(&seen));
    }
    // Abbrechen lässt alles, wie es ist.
    let seen = klick(&mut h, &seen, "Abbrechen");
    assert!(!has(&seen, "Plug-in aktualisieren"));
    assert_eq!(active_version(&h, HALLO).as_deref(), Some("0.1.0"));
    let seen = klick(&mut h, &seen, "Aktualisieren auf 0.2.0");
    klick(&mut h, &seen, "Aktualisieren");
    warten(&mut h);
    assert_eq!(active_version(&h, HALLO).as_deref(), Some("0.2.0"));
}

#[test]
fn no_downgrade_and_no_foreign_id() {
    let owner = "upd-alt";
    let alt = paket_bauen(HALLO, "0.0.9", &["model-read"]);
    releases(owner, &[release(owner, "v0.0.9", &alt, None)]);
    let mut h = installiert_010("upd-alt", owner);
    h.ws.plugins.check_updates(true);
    warten(&mut h);
    assert!(h.ws.plugins.updates().is_empty(), "keine ältere Version");
    assert_eq!(h.ws.plugins.note.as_deref(), Some("Kein Update gefunden."));
    // Ein Release mit einem anderen Plug-in: beim Holen des Pakets („Aktualisieren“) abgelehnt.
    let fremd = paket_bauen("planora.fremd", "0.2.0", &["model-read"]);
    releases(owner, &[release(owner, "v0.2.0", &fremd, None)]);
    h.ws.plugins.check_updates(true);
    warten(&mut h);
    assert_eq!(h.ws.plugins.updates().len(), 1);
    h.ws.plugins.open_update(HALLO);
    warten(&mut h);
    assert!(h.ws.plugins.updates().is_empty());
    assert_eq!(h.ws.plugins.update_dialog, None);
    assert_eq!(
        h.ws.plugins.note.as_deref(),
        Some("Das Release v0.2.0 enthält ein anderes Plug-in (planora.fremd).")
    );
    assert_eq!(active_version(&h, HALLO).as_deref(), Some("0.1.0"));
    assert!(
        h.ws.plugins
            .host
            .entries()
            .iter()
            .all(|e| e.id != "planora.fremd")
    );
}

/// Ein Update, das beim Abholen oder Einspielen nicht mehr höher ist als die installierte
/// Version (inzwischen anders installiert), fällt weg: kein Herabstufen.
#[test]
fn no_downgrade_through_race() {
    let owner = "upd-rennen";
    let neu = paket_bauen(HALLO, "0.2.0", &["model-read"]);
    releases(owner, &[release(owner, "v0.2.0", &neu, None)]);
    let mut h = installiert_010("upd-rennen", owner);
    // Während die Prüfung läuft, wird 0.2.0 aus einer Datei installiert.
    h.ws.plugins.check_updates(false);
    let places = h.ws.plugins.host.places().clone();
    plugin_places::install(&places, &Package::read(&neu).unwrap(), None).unwrap();
    h.ws.plugins.host.rescan(&BTreeMap::new());
    warten(&mut h);
    assert_eq!(active_version(&h, HALLO).as_deref(), Some("0.2.0"));
    assert!(h.ws.plugins.updates().is_empty(), "beim Abholen verworfen");

    // Gefunden und geladen, dann 0.3.0 von Hand installiert: das Einspielen stuft nicht herab.
    let owner = "upd-rennen2";
    releases(owner, &[release(owner, "v0.2.0", &neu, None)]);
    let mut h = installiert_010("upd-rennen2", owner);
    h.ws.plugins.check_updates(false);
    warten(&mut h);
    h.ws.plugins.open_update(HALLO);
    warten(&mut h);
    assert!(h.ws.plugins.ready_update(HALLO).is_some());
    let places = h.ws.plugins.host.places().clone();
    let drei = paket_bauen(HALLO, "0.3.0", &["model-read"]);
    plugin_places::install(&places, &Package::read(&drei).unwrap(), None).unwrap();
    h.ws.plugins.host.rescan(&BTreeMap::new());
    warten(&mut h);
    assert!(!h.ws.plugins.apply_update(HALLO, &mut h.ws.config));
    assert!(h.ws.plugins.updates().is_empty());
    warten(&mut h);
    assert_eq!(active_version(&h, HALLO).as_deref(), Some("0.3.0"));
}

#[test]
fn check_downloaded_rules() {
    let l = RepoLink {
        owner: "a".to_string(),
        repo: "b".to_string(),
    };
    let json = |tag: &str| {
        format!(
            r#"[{{"tag_name":"{tag}","assets":[{{"name":"p.pcplugin","browser_download_url":"https://github.com/a/b/releases/download/{tag}/p.pcplugin","size":1}}]}}]"#
        )
    };
    let rel = |tag: &str| parse_plugin_releases(&json(tag), &l).unwrap().remove(0);
    let pkg = Package::read(&datei("beispiel-hallo")).unwrap();
    assert_eq!(check_downloaded(&pkg, &rel("v0.1.0"), None), Ok(()));
    assert_eq!(check_downloaded(&pkg, &rel("0.1.0"), Some(HALLO)), Ok(()));
    assert_eq!(
        check_downloaded(&pkg, &rel("v0.1.1"), None),
        Err("Das Release v0.1.1 passt nicht zur Plug-in-Version 0.1.0.".to_string())
    );
    assert_eq!(
        check_downloaded(&pkg, &rel("v0.1.0"), Some(FAELLE)),
        Err("Das Release v0.1.0 enthält ein anderes Plug-in (planora.beispiel-hallo).".to_string())
    );
}

/// Richtet die Antworten der Nachbildung für einen Besitzer ein.
type Einrichten = Box<dyn Fn(&str)>;

const STUNDE: u64 = 3600;
const JETZT: u64 = 1_900_000_000;

#[test]
fn auto_check_daily() {
    let a = "auto-a";
    releases(a, &[]);
    let hallo = datei("beispiel-hallo");
    let faelle = datei("test-faelle");
    let places = orte("auto", &[(hallo, herkunft(a, "v0.1.0")), (faelle, None)]);
    let mut h = harness("auto", places, Config::default());
    let prefix = "https://api.github.com/repos/auto-";
    // Aus: keine Anfrage.
    h.ws.config.update_check = false;
    h.ws.config.last_plugin_check = Some(JETZT - 25 * STUNDE);
    assert!(!h.ws.plugins.auto_check(&mut h.ws.config, JETZT));
    warten(&mut h);
    assert!(gefragt(prefix).is_empty());
    // An und vor 25 Stunden: eine Anfrage je Plug-in mit Link, danach der Zeitpunkt gesetzt.
    h.ws.config.update_check = true;
    assert!(h.ws.plugins.auto_check(&mut h.ws.config, JETZT));
    warten(&mut h);
    assert_eq!(gefragt(prefix), [releases_url(a)]);
    assert_eq!(h.ws.config.last_plugin_check, Some(JETZT));
    // Gleich gespeichert.
    assert_eq!(
        Config::load_from(&h.ws.config_path).last_plugin_check,
        Some(JETZT)
    );
    // Vor einer Stunde: keine.
    h.ws.config.last_plugin_check = Some(JETZT - STUNDE);
    assert!(!h.ws.plugins.auto_check(&mut h.ws.config, JETZT));
    warten(&mut h);
    assert_eq!(gefragt(prefix).len(), 1);
    // Automatisch ohne Update: kein Hinweis.
    assert_eq!(h.ws.plugins.note, None);
}

#[test]
fn bundled_plugins_not_checked() {
    let owner = "mitgeliefert";
    releases(owner, &[]);
    let places = orte("mitgeliefert", &[]);
    // Mitgeliefert (wie vom Installer), sogar mit herkunft.json.
    let bundled = Places {
        bundled: Vec::new(),
        user: places.bundled[0].clone(),
        cache: places.cache.clone(),
    };
    let pkg = Package::read(&datei("beispiel-hallo")).unwrap();
    plugin_places::install(&bundled, &pkg, herkunft(owner, "v0.1.0").as_ref()).unwrap();
    let config = Config {
        last_plugin_check: Some(JETZT - 25 * STUNDE),
        ..Default::default()
    };
    let mut h = harness("mitgeliefert", places, config);
    assert_eq!(active_version(&h, HALLO).as_deref(), Some("0.1.0"));
    h.ws.plugins.check_updates(true);
    assert!(!h.ws.plugins.auto_check(&mut h.ws.config, JETZT));
    warten(&mut h);
    assert!(gefragt(&format!("https://api.github.com/repos/{owner}/")).is_empty());
    assert!(h.ws.plugins.updates().is_empty());
}
