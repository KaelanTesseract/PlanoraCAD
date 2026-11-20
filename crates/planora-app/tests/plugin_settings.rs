//! Plug-ins Etappe 1, Task 7: Abschnitt „Plug-ins“ in den Einstellungen und der
//! Installationsdialog. Gezeichnet wird, was die App je Bild zeichnet: Rückfragen, Einstellungen
//! und die Plug-ins (mit dem Installationsdialog), bei 1920 × 1080.

mod common;

use common::{Seen, collect_seen, find, join};
use egui::{Event, Modifiers, PointerButton, Pos2, Rect, vec2};
use planora_app::plugins::{self, Plugins};
use planora_app::settings_window::{
    ChooserPurpose, SettingsSection, SettingsWindow, plugin_details_id,
};
use planora_app::workspace::{Prompt, Workspace};
use planora_app::{fonts, prompts, theme};
use planora_plugin_host::EntryState;
use planora_plugin_host::manifest::Right;
use planora_plugin_host::package::{Package, Signature};
use planora_plugin_host::places::{self as plugin_places, Origin, Places};
use planora_plugin_host::rights::Grant;
use planora_project::config::{Config, PluginConfig};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

const HALLO: &str = "planora.beispiel-hallo";
const FAELLE: &str = "planora.test-faelle";

fn data_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../planora-plugin-host/tests/data")
}

fn paket(name: &str) -> Package {
    let file = data_dir().join(format!("{name}.pcplugin"));
    Package::read(&std::fs::read(file).unwrap()).unwrap()
}

/// Fundorte in Temp: `mitgeliefert` und `installiert` sind Paketnamen der Testdaten.
fn orte(name: &str, mitgeliefert: &[&str], installiert: &[&str]) -> Places {
    let root = common::temp_root(&format!("planora-plugin-settings-{name}"));
    let p = Places {
        bundled: vec![root.join("mitgeliefert")],
        user: root.join("benutzer"),
        cache: root.join("cache"),
    };
    let bundled = Places {
        bundled: Vec::new(),
        user: p.bundled[0].clone(),
        cache: p.cache.clone(),
    };
    for name in mitgeliefert {
        plugin_places::install(&bundled, &paket(name), None).unwrap();
    }
    for name in installiert {
        plugin_places::install(&p, &paket(name), None).unwrap();
    }
    std::fs::create_dir_all(&p.user).unwrap();
    p
}

fn fresh_ctx() -> egui::Context {
    let ctx = egui::Context::default();
    ctx.set_fonts(fonts::definitions());
    theme::apply(&ctx, true);
    // Aufklappen ohne Animation (die Tests lassen keine Zeit vergehen).
    ctx.all_styles_mut(|s| s.animation_time = 0.0);
    ctx
}

struct H {
    ws: Workspace,
    ctx: egui::Context,
    w: SettingsWindow,
    /// Bildschirmgröße (Vorgabe 1920 × 1080).
    size: egui::Vec2,
}

/// Arbeitsbereich mit Konfiguration in Temp und gestarteten Plug-ins; Einstellungen offen am
/// Abschnitt „Plug-ins“.
fn harness(name: &str, places: Places, config: Config) -> H {
    let mut ws = Workspace::new(None, config);
    let cfg = common::temp_root(&format!("planora-plugin-settings-cfg-{name}"));
    std::fs::create_dir_all(&cfg).unwrap();
    ws.config_path = cfg.join("config.json");
    ws.plugins = Plugins::new(places, &ws.config);
    ws.plugins.host.set_language("de");
    let ctx = fresh_ctx();
    let mut w = SettingsWindow::default();
    w.open_at(false, Some(SettingsSection::Plugins));
    let mut h = H {
        ws,
        ctx,
        w,
        size: vec2(1920.0, 1080.0),
    };
    warten(&mut h);
    h
}

fn bild(h: &mut H, events: Vec<Event>) -> Vec<Seen> {
    let raw = egui::RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, h.size)),
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

/// Bilder, bis kein Plug-in mehr lädt und kein Paket mehr gelesen wird (höchstens 60 s).
fn warten(h: &mut H) -> Vec<Seen> {
    let start = Instant::now();
    loop {
        bild(h, vec![]);
        let busy =
            h.ws.plugins
                .host
                .entries()
                .iter()
                .any(|e| e.state == EntryState::Loading)
                || h.ws.plugins.is_reading();
        if !busy {
            // Noch zwei Bilder, damit Fenster und Rollen stehen.
            bild(h, vec![]);
            return bild(h, vec![]);
        }
        assert!(start.elapsed() < Duration::from_secs(60), "lädt zu lange");
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// Klick auf das letzte Vorkommen von `text` im Bild `seen` (das oberste Fenster zeichnet
/// zuletzt).
fn klick(h: &mut H, seen: &[Seen], text: &str) -> Vec<Seen> {
    let at = seen
        .iter()
        .rev()
        .find(|s| s.text == text)
        .unwrap_or_else(|| panic!("„{text}“ fehlt: {}", join(seen)))
        .rect
        .center();
    klick_at(h, at)
}

fn klick_at(h: &mut H, at: Pos2) -> Vec<Seen> {
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

fn aufklappen(h: &mut H, id: &str) -> Vec<Seen> {
    klappen(h, id, true)
}

fn klappen(h: &mut H, id: &str, open: bool) -> Vec<Seen> {
    let mut state = egui::collapsing_header::CollapsingState::load_with_default_open(
        &h.ctx,
        plugin_details_id(id),
        false,
    );
    state.set_open(open);
    state.store(&h.ctx);
    bild(h, vec![]);
    bild(h, vec![])
}

fn entry_state(h: &H, id: &str) -> Option<EntryState> {
    h.ws.plugins
        .host
        .entries()
        .iter()
        .find(|e| e.id == id)
        .map(|e| e.state.clone())
}

fn gespeichert(h: &H, id: &str) -> Option<PluginConfig> {
    Config::load_from(&h.ws.config_path)
        .plugins
        .get(id)
        .cloned()
}

fn has(seen: &[Seen], text: &str) -> bool {
    find(seen, text).is_some()
}

#[test]
fn install_flow_starts_plugin() {
    let mut h = harness(
        "installieren",
        orte("installieren", &[], &[]),
        Config::default(),
    );
    let seen = bild(&mut h, vec![]);
    assert!(has(&seen, "Keine Plug-ins gefunden."), "{}", join(&seen));
    klick(&mut h, &seen, "Aus Datei…");
    assert_eq!(h.w.chooser_purpose(), Some(ChooserPurpose::Plugin));
    let c = h.w.chooser_mut().unwrap();
    c.dir_text = data_dir().display().to_string();
    assert!(c.go_to_text());
    let names: Vec<&str> = c.entries().iter().map(|e| e.name.as_str()).collect();
    assert!(names.contains(&"test-faelle.pcplugin"), "{names:?}");
    assert!(
        !names.iter().any(|n| n.ends_with(".quelle-sha256")),
        "nur *.pcplugin: {names:?}"
    );
    c.select("test-faelle.pcplugin");
    let seen = bild(&mut h, vec![]);
    klick(&mut h, &seen, "Öffnen");
    assert!(h.w.chooser_mut().is_none(), "Wählen schließt die Auswahl");
    let seen = warten(&mut h);
    assert!(h.ws.plugins.install.is_some());
    for text in [
        "Plug-in installieren",
        "Test-Fälle",
        "Version: 0.1.0",
        "Autor: Dennis Guse",
        "Lizenz: GPL-3.0-only",
        "Netz (example.org)",
        "Das Programm fragt beim ersten Gebrauch nach.",
    ] {
        assert!(has(&seen, text), "„{text}“ fehlt: {}", join(&seen));
    }
    assert!(!has(&seen, "Lizenz unbekannt oder nicht GPL-verträglich"));
    assert!(!seen.iter().any(|s| s.text.starts_with("Speicherbedarf")));
    assert!(!seen.iter().any(|s| s.text.contains("ersetzen durch")));
    klick(&mut h, &seen, "Installieren");
    assert!(h.ws.plugins.install.is_none());
    let seen = warten(&mut h);
    // Ohne Neustart: das Plug-in läuft, installiert, und steht im Abschnitt.
    assert_eq!(entry_state(&h, FAELLE), Some(EntryState::Running));
    let e =
        h.ws.plugins
            .host
            .entries()
            .iter()
            .find(|e| e.id == FAELLE)
            .unwrap();
    assert_eq!(e.found[e.active.unwrap()].origin, Origin::User);
    assert!(
        h.ws.plugins
            .host
            .commands()
            .iter()
            .any(|c| c.plugin == FAELLE)
    );
    for text in ["Test-Fälle", "0.1.0", "installiert", "läuft"] {
        assert!(has(&seen, text), "„{text}“ fehlt: {}", join(&seen));
    }
    assert_eq!(h.ws.plugins.note, None);
}

#[test]
fn install_shows_license_hint() {
    let mut h = harness("lizenz", orte("lizenz", &[], &[]), Config::default());
    let mut pkg = paket("test-faelle");
    let signed = match pkg.signature {
        Signature::None => "Signatur: nicht signiert",
        Signature::Present => "Signatur: signiert, nicht geprüft",
    };
    pkg.manifest.license = "MIT AND Proprietary".to_string();
    pkg.manifest.memory_mb = 256;
    h.ws.plugins
        .offer_install(pkg.clone(), Some("C:/irgendwo/test-faelle.pcplugin".into()));
    let seen = bild(&mut h, vec![]);
    for text in [
        "Lizenz: MIT AND Proprietary",
        "Lizenz unbekannt oder nicht GPL-verträglich",
        "Speicherbedarf: 256 MB",
        signed,
        "Quelle: C:/irgendwo/test-faelle.pcplugin",
    ] {
        assert!(has(&seen, text), "„{text}“ fehlt: {}", join(&seen));
    }
    // „Abbrechen“ installiert nichts.
    klick(&mut h, &seen, "Abbrechen");
    assert!(h.ws.plugins.install.is_none());
    warten(&mut h);
    assert_eq!(entry_state(&h, FAELLE), None);

    // Eine verträgliche Lizenz (eine Seite eines OR genügt) zeigt keinen Hinweis.
    pkg.manifest.license = "MIT OR Proprietary".to_string();
    h.ws.plugins.offer_install(pkg, None);
    let seen = bild(&mut h, vec![]);
    assert!(has(&seen, "Lizenz: MIT OR Proprietary"), "{}", join(&seen));
    assert!(!has(&seen, "Lizenz unbekannt oder nicht GPL-verträglich"));
    assert!(!seen.iter().any(|s| s.text.starts_with("Quelle:")));
}

/// Rechte, die diese Version noch nicht umsetzt, zeigt der Dialog als „noch nicht verfügbar“.
#[test]
fn install_shows_unavailable_rights() {
    let mut h = harness(
        "nicht-verfuegbar",
        orte("nicht-verfuegbar", &[], &[]),
        Config::default(),
    );
    let mut pkg = paket("test-faelle");
    pkg.manifest.rights.insert(Right::ModelRead);
    h.ws.plugins.offer_install(pkg, None);
    let seen = bild(&mut h, vec![]);
    assert!(
        has(&seen, "in dieser Version noch nicht verfügbar"),
        "{}",
        join(&seen)
    );
    assert!(
        has(&seen, "wird beim ersten Gebrauch gefragt"),
        "{}",
        join(&seen)
    );
    // Nur ein nicht verfügbares Recht: kein Hinweis aufs Nachfragen.
    let mut pkg = paket("test-faelle");
    pkg.manifest.rights = [Right::FsFolder].into();
    pkg.manifest.net_hosts.clear();
    h.ws.plugins.offer_install(pkg, None);
    let seen = bild(&mut h, vec![]);
    assert!(
        has(&seen, "in dieser Version noch nicht verfügbar"),
        "{}",
        join(&seen)
    );
    assert!(!has(&seen, "Das Programm fragt beim ersten Gebrauch nach."));
}

#[test]
fn install_replace_asks() {
    let places = orte("ersetzen", &[], &["test-faelle"]);
    // Die installierte Fassung wird zu 0.0.9 (wie eine ältere Version).
    let toml = places.user.join(FAELLE).join("plugin.toml");
    let text = std::fs::read_to_string(&toml).unwrap();
    std::fs::write(
        &toml,
        text.replace("version = \"0.1.0\"", "version = \"0.0.9\""),
    )
    .unwrap();
    let mut h = harness("ersetzen", places, Config::default());
    assert_eq!(entry_state(&h, FAELLE), Some(EntryState::Running));
    h.ws.plugins.offer_install(paket("test-faelle"), None);
    let seen = bild(&mut h, vec![]);
    assert!(
        has(&seen, "Version 0.0.9 ersetzen durch Version 0.1.0?"),
        "{}",
        join(&seen)
    );
    klick(&mut h, &seen, "Installieren");
    warten(&mut h);
    let e =
        h.ws.plugins
            .host
            .entries()
            .iter()
            .find(|e| e.id == FAELLE)
            .unwrap();
    assert_eq!(e.found.len(), 1, "ersetzt, nicht daneben");
    assert_eq!(e.found[0].manifest.version.to_string(), "0.1.0");
    assert_eq!(e.state, EntryState::Running, "startet ohne Neustart neu");
}

#[test]
fn toggle_off_stops_and_saves() {
    let mut h = harness(
        "schalter",
        orte("schalter", &[], &["test-faelle"]),
        Config::default(),
    );
    assert_eq!(entry_state(&h, FAELLE), Some(EntryState::Running));
    let seen = bild(&mut h, vec![]);
    assert!(has(&seen, "läuft"), "{}", join(&seen));
    let seen = klick(&mut h, &seen, "Test-Fälle");
    assert_eq!(entry_state(&h, FAELLE), Some(EntryState::Off));
    assert_eq!(
        gespeichert(&h, FAELLE),
        Some(PluginConfig {
            enabled: false,
            rights: BTreeMap::new(),
            author: Some("Dennis Guse".to_string()),
            net_hosts: vec!["example.org".to_string()],
            source: Some("file".to_string()),
        }),
        "gleich gespeichert"
    );
    let seen = if has(&seen, "aus") {
        seen
    } else {
        bild(&mut h, vec![])
    };
    assert!(has(&seen, "aus"), "{}", join(&seen));
    // Wieder an: läuft, und der Eintrag (Voreinstellung) fällt aus der Datei.
    klick(&mut h, &seen, "Test-Fälle");
    warten(&mut h);
    assert_eq!(entry_state(&h, FAELLE), Some(EntryState::Running));
    assert_eq!(gespeichert(&h, FAELLE), None);
}

#[test]
fn revoke_returns_to_ask() {
    let mut config = Config::default();
    config.plugins.insert(
        FAELLE.to_string(),
        PluginConfig {
            enabled: true,
            rights: BTreeMap::from([("net".to_string(), "allowed".to_string())]),
            author: Some("Dennis Guse".to_string()),
            net_hosts: vec!["example.org".to_string()],
            source: Some("file".to_string()),
        },
    );
    let mut h = harness(
        "zuruecknehmen",
        orte("zuruecknehmen", &[], &["test-faelle"]),
        config,
    );
    let seen = aufklappen(&mut h, FAELLE);
    for text in [
        "Autor: Dennis Guse",
        "Lizenz: GPL-3.0-only",
        "Verlangte Rechte:",
        "Netz (example.org)",
        "erlaubt",
    ] {
        assert!(has(&seen, text), "„{text}“ fehlt: {}", join(&seen));
    }
    klick(&mut h, &seen, "Zurücknehmen");
    let e =
        h.ws.plugins
            .host
            .entries()
            .iter()
            .find(|e| e.id == FAELLE)
            .unwrap();
    assert_eq!(e.rights.state(Right::Net), None);
    assert_eq!(gespeichert(&h, FAELLE), None, "gleich gespeichert");
    let seen = bild(&mut h, vec![]);
    assert!(
        has(&seen, "wird beim ersten Gebrauch gefragt"),
        "{}",
        join(&seen)
    );
}

#[test]
fn denied_right_shown() {
    let mut config = Config::default();
    config.plugins.insert(
        FAELLE.to_string(),
        PluginConfig {
            enabled: true,
            rights: BTreeMap::from([("net".to_string(), "denied".to_string())]),
            author: Some("Dennis Guse".to_string()),
            net_hosts: vec!["example.org".to_string()],
            source: Some("file".to_string()),
        },
    );
    let mut h = harness(
        "abgelehnt",
        orte("abgelehnt", &[], &["test-faelle"]),
        config,
    );
    let e =
        h.ws.plugins
            .host
            .entries()
            .iter()
            .find(|e| e.id == FAELLE)
            .unwrap();
    assert_eq!(e.rights.state(Right::Net), Some(Grant::Denied));
    let seen = aufklappen(&mut h, FAELLE);
    assert!(has(&seen, "abgelehnt"), "{}", join(&seen));
}

#[test]
fn remove_user_plugin_only() {
    let places = orte("entfernen", &["beispiel-hallo"], &["test-faelle"]);
    let user_dir = places.user.join(FAELLE);
    let mut h = harness("entfernen", places, Config::default());
    // Nur zwischen den Überschriften „Plug-ins“ und „Projekte“ (die Tastenkürzel-Tabelle hat
    // eigene „Entfernen“), ganz zu sehen.
    let removes = |seen: &[Seen]| -> Vec<Rect> {
        let top = find(seen, "Plug-ins").unwrap().rect.bottom();
        let bottom = find(seen, "Projekte").unwrap().rect.top();
        seen.iter()
            .filter(|s| {
                s.text == "Entfernen"
                    && s.rect.top() > top
                    && s.rect.bottom() < bottom
                    && s.clip.contains_rect(s.rect)
            })
            .map(|s| s.rect)
            .collect()
    };
    let seen = aufklappen(&mut h, HALLO);
    assert!(has(&seen, "Verlangt keine Rechte."), "{}", join(&seen));
    assert!(removes(&seen).is_empty(), "mitgeliefert: {}", join(&seen));
    klappen(&mut h, HALLO, false);
    let seen = aufklappen(&mut h, FAELLE);
    let found = removes(&seen);
    assert_eq!(found.len(), 1, "installiert: {}", join(&seen));
    let at = found[0].center();
    let seen = klick_at(&mut h, at);
    assert!(
        matches!(&h.ws.prompt, Some(Prompt::RemovePlugin { id, .. }) if id == FAELLE),
        "erst die Rückfrage"
    );
    assert!(
        has(&seen, "Plug-in Test-Fälle entfernen?"),
        "{}",
        join(&seen)
    );
    assert!(user_dir.exists());
    // „Abbrechen“ lässt es stehen.
    klick(&mut h, &seen, "Abbrechen");
    assert!(h.ws.prompt.is_none());
    assert!(user_dir.exists());
    let seen = klick_at(&mut h, at);
    assert!(h.ws.prompt.is_some());
    // Das letzte „Entfernen“ ist das der Rückfrage (sie zeichnet zuoberst).
    klick(&mut h, &seen, "Entfernen");
    assert!(h.ws.prompt.is_none());
    warten(&mut h);
    assert!(!user_dir.exists());
    assert_eq!(entry_state(&h, FAELLE), None);
    assert_eq!(entry_state(&h, HALLO), Some(EntryState::Running));
    assert_eq!(h.ws.plugins.note, None);
}

#[test]
fn shadowed_entry_shown() {
    let places = orte("ueberdeckt", &["test-faelle"], &["test-faelle"]);
    // Die mitgelieferte Fassung ist neuer und läuft.
    let toml = places.bundled[0].join(FAELLE).join("plugin.toml");
    let text = std::fs::read_to_string(&toml).unwrap();
    std::fs::write(
        &toml,
        text.replace("version = \"0.1.0\"", "version = \"0.2.0\""),
    )
    .unwrap();
    let mut h = harness("ueberdeckt", places, Config::default());
    let seen = bild(&mut h, vec![]);
    for text in [
        "0.2.0",
        "mitgeliefert",
        "installiert 0.1.0: überdeckt von Version 0.2.0",
    ] {
        assert!(has(&seen, text), "„{text}“ fehlt: {}", join(&seen));
    }
}

#[test]
fn opening_at_plugins_scrolls_there() {
    // Bei 1920 × 1080 steht der Abschnitt ohnehin im Bild; rollen muss das Fenster erst auf einem
    // kleineren Bildschirm (1366 × 768), wo Darstellung und Updates untereinander stehen.
    let header_top = |at: Option<SettingsSection>, name: &str| {
        let mut h = harness(name, orte(name, &[], &[]), Config::default());
        // Eigener Kontext: kein Rollstand aus den Bildern des Aufbaus.
        h.ctx = fresh_ctx();
        h.size = vec2(1366.0, 768.0);
        h.w.close();
        h.w.open_at(false, at);
        for _ in 0..4 {
            bild(&mut h, vec![]);
        }
        assert_eq!(h.w.scroll_target(), None, "abgeholt, sobald hingerollt ist");
        let seen = bild(&mut h, vec![]);
        let window = h.w.last_rect.unwrap();
        let header = find(&seen, "Plug-ins").unwrap_or_else(|| panic!("{}", join(&seen)));
        (header.rect.top() - window.top(), window)
    };
    let (plain, _) = header_top(None, "rollen-ohne");
    let (scrolled, window) = header_top(Some(SettingsSection::Plugins), "rollen");
    assert!(
        scrolled < plain - 100.0,
        "hingerollt: {scrolled} statt {plain} (Fenster {window:?})"
    );
    assert!(scrolled > 0.0, "im Fenster: {scrolled}");
}

// --- Fix-Runde 1: Rechte bei Entfernen und Ersetzen -------------------------------------------

fn netz_erlaubt() -> Config {
    let mut config = Config::default();
    config.plugins.insert(
        FAELLE.to_string(),
        PluginConfig {
            enabled: true,
            rights: BTreeMap::from([("net".to_string(), "allowed".to_string())]),
            author: Some("Dennis Guse".to_string()),
            net_hosts: vec!["example.org".to_string()],
            source: Some("file".to_string()),
        },
    );
    config
}

fn net_state(h: &H) -> Option<Grant> {
    h.ws.plugins
        .host
        .entries()
        .iter()
        .find(|e| e.id == FAELLE)
        .unwrap()
        .rights
        .state(Right::Net)
}

#[test]
fn remove_then_reinstall_has_no_grants() {
    let mut h = harness(
        "neu-ohne-rechte",
        orte("neu-ohne-rechte", &[], &["test-faelle"]),
        netz_erlaubt(),
    );
    assert_eq!(net_state(&h), Some(Grant::Allowed));
    plugins::remove(&mut h.ws, FAELLE);
    assert!(!h.ws.config.plugins.contains_key(FAELLE));
    assert_eq!(gespeichert(&h, FAELLE), None, "ganzer Eintrag gelöscht");
    warten(&mut h);
    h.ws.plugins.offer_install(paket("test-faelle"), None);
    assert_eq!(
        h.ws.plugins.install.as_ref().unwrap().grants,
        vec![(Right::Net, None)]
    );
    let seen = bild(&mut h, vec![]);
    assert!(
        has(&seen, "wird beim ersten Gebrauch gefragt"),
        "{}",
        join(&seen)
    );
    assert!(has(&seen, "Das Programm fragt beim ersten Gebrauch nach."));
    klick(&mut h, &seen, "Installieren");
    warten(&mut h);
    assert_eq!(entry_state(&h, FAELLE), Some(EntryState::Running));
    assert_eq!(net_state(&h), None, "fragt wieder");
    assert_eq!(gespeichert(&h, FAELLE), None);
}

#[test]
fn fresh_install_ignores_stale_config() {
    // Kein Fundort, aber ein alter Eintrag in der Konfiguration (etwa von Hand entfernt).
    let mut h = harness(
        "alter-eintrag",
        orte("alter-eintrag", &[], &[]),
        netz_erlaubt(),
    );
    h.ws.plugins.offer_install(paket("test-faelle"), None);
    assert_eq!(
        h.ws.plugins.install.as_ref().unwrap().grants,
        vec![(Right::Net, None)]
    );
    assert!(h.ws.plugins.install_offered(&mut h.ws.config));
    warten(&mut h);
    assert_eq!(entry_state(&h, FAELLE), Some(EntryState::Running));
    assert_eq!(net_state(&h), None);
    assert!(!h.ws.config.plugins.contains_key(FAELLE));
}

#[test]
fn replace_same_author_keeps_grants_and_restarts() {
    let mut h = harness(
        "ersetzen-rechte",
        orte("ersetzen-rechte", &[], &["test-faelle"]),
        netz_erlaubt(),
    );
    h.ws.plugins.offer_install(paket("test-faelle"), None);
    assert_eq!(
        h.ws.plugins.install.as_ref().unwrap().grants,
        vec![(Right::Net, Some(Grant::Allowed))]
    );
    let seen = bild(&mut h, vec![]);
    assert!(has(&seen, "bereits erlaubt"), "{}", join(&seen));
    assert!(
        !has(&seen, "Das Programm fragt beim ersten Gebrauch nach."),
        "nichts mehr zu fragen"
    );
    assert!(has(&seen, "Version 0.1.0 ersetzen durch Version 0.1.0?"));
    // Gleiche Version: startet trotzdem neu, damit der neue Code läuft.
    assert!(h.ws.plugins.install_offered(&mut h.ws.config));
    assert_eq!(entry_state(&h, FAELLE), Some(EntryState::Loading));
    warten(&mut h);
    assert_eq!(entry_state(&h, FAELLE), Some(EntryState::Running));
    assert_eq!(net_state(&h), Some(Grant::Allowed));
}

#[test]
fn dialog_shows_dropped_grants() {
    let mut h = harness(
        "ersetzen-anzeige",
        orte("ersetzen-anzeige", &[], &["test-faelle"]),
        netz_erlaubt(),
    );
    // Anderer Autor: alles wird wieder gefragt.
    let mut pkg = paket("test-faelle");
    pkg.manifest.author = "Jemand anders".to_string();
    h.ws.plugins.offer_install(pkg, None);
    assert_eq!(
        h.ws.plugins.install.as_ref().unwrap().grants,
        vec![(Right::Net, None)]
    );
    let seen = bild(&mut h, vec![]);
    assert!(
        has(&seen, "wird beim ersten Gebrauch gefragt"),
        "{}",
        join(&seen)
    );
    assert!(!has(&seen, "bereits erlaubt"));
    // Andere Rechner: `net` wird wieder gefragt; nur anders geschrieben: bleibt.
    let mut pkg = paket("test-faelle");
    pkg.manifest.net_hosts = vec!["example.com".to_string()];
    h.ws.plugins.offer_install(pkg, None);
    assert_eq!(
        h.ws.plugins.install.as_ref().unwrap().grants,
        vec![(Right::Net, None)]
    );
    let mut pkg = paket("test-faelle");
    pkg.manifest.net_hosts = vec!["Example.ORG".to_string()];
    h.ws.plugins.offer_install(pkg, None);
    assert_eq!(
        h.ws.plugins.install.as_ref().unwrap().grants,
        vec![(Right::Net, Some(Grant::Allowed))]
    );
}

#[test]
fn removed_user_copy_leaves_bundled_without_grants() {
    let mut h = harness(
        "mitgeliefert-ohne-rechte",
        orte(
            "mitgeliefert-ohne-rechte",
            &["test-faelle"],
            &["test-faelle"],
        ),
        netz_erlaubt(),
    );
    let e =
        h.ws.plugins
            .host
            .entries()
            .iter()
            .find(|e| e.id == FAELLE)
            .unwrap();
    assert_eq!(e.found[e.active.unwrap()].origin, Origin::User);
    assert_eq!(net_state(&h), Some(Grant::Allowed));
    plugins::remove(&mut h.ws, FAELLE);
    warten(&mut h);
    let e =
        h.ws.plugins
            .host
            .entries()
            .iter()
            .find(|e| e.id == FAELLE)
            .unwrap();
    assert_eq!(e.found.len(), 1);
    assert_eq!(e.found[0].origin, Origin::Bundled);
    assert_eq!(entry_state(&h, FAELLE), Some(EntryState::Running));
    assert_eq!(net_state(&h), None);
    assert_eq!(gespeichert(&h, FAELLE), None);
}

#[test]
fn install_locked_while_prompt_open() {
    let mut h = harness("gesperrt", orte("gesperrt", &[], &[]), Config::default());
    h.ws.prompt = Some(Prompt::Notice("offen".to_string()));
    h.ws.plugins.offer_install(paket("test-faelle"), None);
    let seen = bild(&mut h, vec![]);
    klick(&mut h, &seen, "Installieren");
    assert!(h.ws.plugins.install.is_some(), "gesperrt");
    assert_eq!(entry_state(&h, FAELLE), None);
    // Grund beim Überfahren (Tooltips brauchen vergehende Zeit).
    h.ctx.all_styles_mut(|s| s.interaction.tooltip_delay = 0.0);
    let at = seen
        .iter()
        .rev()
        .find(|s| s.text == "Installieren")
        .unwrap()
        .rect
        .center();
    let mut hovered = Vec::new();
    for i in 0..10 {
        let raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, h.size)),
            events: vec![Event::PointerMoved(at)],
            time: Some(100.0 + f64::from(i) * 0.1),
            ..Default::default()
        };
        let (ws, w) = (&mut h.ws, &mut h.w);
        let out = h.ctx.run_ui(raw, |ui| {
            let ctx = ui.ctx().clone();
            w.show_in(&ctx, ws);
            plugins::show(&ctx, ws);
        });
        hovered = collect_seen(&out.shapes);
        out.drop_without_applying_deltas();
    }
    assert!(
        has(&hovered, "Zuerst die offene Rückfrage beantworten."),
        "{}",
        join(&hovered)
    );
    // Rückfrage beantwortet: jetzt geht es.
    h.ws.prompt = None;
    let seen = bild(&mut h, vec![]);
    klick(&mut h, &seen, "Installieren");
    assert!(h.ws.plugins.install.is_none());
    warten(&mut h);
    assert_eq!(entry_state(&h, FAELLE), Some(EntryState::Running));
}
