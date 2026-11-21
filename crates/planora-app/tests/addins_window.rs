//! Plug-ins Etappe 1, Task 7 und Handtest W1/W2: das Fenster „Zusatzmodule“ (Verwaltung der
//! Plug-ins, früher ein Abschnitt der Einstellungen) und der Installationsdialog. Gezeichnet
//! wird, was die App je Bild zeichnet: Rückfragen, das Fenster und die Plug-ins (mit dem
//! Installationsdialog), bei 1920 × 1080.

mod common;

use common::{Seen, collect_seen, find, join};
use egui::{Event, Modifiers, PointerButton, Pos2, Rect, vec2};
use planora_app::addins_window::{AddinsTab, AddinsWindow};
use planora_app::plugins::{self, Plugins};
use planora_app::workspace::{Prompt, Workspace};
use planora_app::{fonts, prompts, theme};
use planora_plugin_host::EntryState;
use planora_plugin_host::manifest::Right;
use planora_plugin_host::package::{Package, Signature};
use planora_plugin_host::places::{self as plugin_places, Origin, Places, Provenance};
use planora_plugin_host::rights::Grant;
use planora_project::config::{Config, PluginConfig};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};
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
    w: AddinsWindow,
    /// Bildschirmgröße (Vorgabe 1920 × 1080).
    size: egui::Vec2,
}

/// Arbeitsbereich mit Konfiguration in Temp und gestarteten Plug-ins; das Fenster
/// „Zusatzmodule“ ist offen.
fn harness(name: &str, places: Places, config: Config) -> H {
    let mut ws = Workspace::new(None, config);
    let cfg = common::temp_root(&format!("planora-plugin-settings-cfg-{name}"));
    std::fs::create_dir_all(&cfg).unwrap();
    ws.config_path = cfg.join("config.json");
    ws.plugins = Plugins::new(places, &ws.config);
    ws.plugins.host.set_language("de");
    let ctx = fresh_ctx();
    let mut w = AddinsWindow::default();
    w.open();
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

/// Wählt `id` in der Liste (ein Klick auf den Namen): rechts stehen seine Details.
fn waehlen(h: &mut H, id: &str) -> Vec<Seen> {
    let name = h.ws.plugins.plugin_name(id);
    let seen = bild(h, vec![]);
    let toggle = h.w.last_toggles[id];
    // Der Name steht in derselben Zeile rechts neben dem Schalter.
    let at = seen
        .iter()
        .find(|s| {
            s.text == name
                && s.rect.left() > toggle.right()
                && (s.rect.center().y - toggle.center().y).abs() < 4.0
        })
        .unwrap_or_else(|| panic!("„{name}“ fehlt in der Liste: {}", join(&seen)))
        .rect
        .center();
    klick_at(h, at);
    assert_eq!(h.w.selected_in(&h.ws.plugins).as_deref(), Some(id));
    bild(h, vec![])
}

/// Klick auf den Schalter von `id` in der Liste.
fn schalten(h: &mut H, id: &str) -> Vec<Seen> {
    bild(h, vec![]);
    let at = h.w.last_toggles[id].center();
    klick_at(h, at)
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
    assert!(h.w.is_choosing());
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
    let seen = schalten(&mut h, FAELLE);
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
    schalten(&mut h, FAELLE);
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
    let seen = waehlen(&mut h, FAELLE);
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
    let seen = waehlen(&mut h, FAELLE);
    assert!(has(&seen, "abgelehnt"), "{}", join(&seen));
}

#[test]
fn remove_user_plugin_only() {
    let places = orte("entfernen", &["beispiel-hallo"], &["test-faelle"]);
    let user_dir = places.user.join(FAELLE);
    let mut h = harness("entfernen", places, Config::default());
    // Im Fenster, ganz zu sehen.
    let window = h.w.last_rect.unwrap();
    let removes = |seen: &[Seen]| -> Vec<Rect> {
        seen.iter()
            .filter(|s| {
                s.text == "Entfernen"
                    && window.contains_rect(s.rect)
                    && s.clip.contains_rect(s.rect)
            })
            .map(|s| s.rect)
            .collect()
    };
    let seen = waehlen(&mut h, HALLO);
    assert!(has(&seen, "Verlangt keine Rechte."), "{}", join(&seen));
    assert!(removes(&seen).is_empty(), "mitgeliefert: {}", join(&seen));
    let seen = waehlen(&mut h, FAELLE);
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

/// Handtest W1/W2 bei 1920 × 1080: das Fenster steht ganz im Bild unter der Menüleiste, links
/// die Liste, rechts die Details des gewählten Plug-ins; seine Rechte stehen mit Stand und
/// „Zurücknehmen“ gleich da, ohne Aufklappen.
#[test]
fn window_shows_list_and_details_with_rights() {
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
    let places = orte("fenster", &["beispiel-hallo"], &["test-faelle"]);
    let user = places.user.clone();
    let mut h = harness("fenster", places, config);
    let seen = bild(&mut h, vec![]);
    let window = h.w.last_rect.unwrap();
    let screen = Rect::from_min_size(Pos2::ZERO, h.size);
    assert!(screen.contains_rect(window), "{window:?}");
    assert!(
        window.top() >= theme::HEADER_H,
        "unter der Menüleiste: {window:?}"
    );
    for text in [
        "Zusatzmodule",
        "Plug-ins",
        "Aus Datei…",
        "Holen",
        "Schließen",
    ] {
        assert!(has(&seen, text), "„{text}“ fehlt: {}", join(&seen));
    }
    // Der Ordner der installierten Plug-ins steht da (so fällt ein falscher Ordner auf).
    let folder = format!(
        "Installierte Plug-ins liegen in: {}",
        planora_app::file_chooser::display_path(&user)
    );
    assert!(has(&seen, &folder), "{}", join(&seen));
    // Ohne Wahl das erste der Liste (nach Kennung: Beispiel-Hallo), rechts seine Details.
    assert_eq!(h.w.selected_in(&h.ws.plugins).as_deref(), Some(HALLO));
    assert!(has(&seen, "Verlangt keine Rechte."), "{}", join(&seen));
    // Test-Fälle wählen: Rechte mit Stand und „Zurücknehmen“ sofort sichtbar.
    let seen = waehlen(&mut h, FAELLE);
    let toggle = h.w.last_toggles[FAELLE];
    for text in ["Netz (example.org)", "erlaubt", "Zurücknehmen", "Entfernen"] {
        let s = find(&seen, text).unwrap_or_else(|| panic!("„{text}“ fehlt: {}", join(&seen)));
        assert!(window.contains_rect(s.rect), "„{text}“ im Fenster");
        assert!(s.clip.contains_rect(s.rect), "„{text}“ ganz zu sehen");
        assert!(
            s.rect.left() > toggle.right() + 200.0,
            "„{text}“ rechts der Liste"
        );
    }
    assert!(!has(&seen, "Verlangt keine Rechte."), "{}", join(&seen));
}

/// Ein Plug-in-Ordner, der da ist, sich aber nicht lesen lässt, steht als Hinweis im Fenster
/// (sonst hieße es nur „Keine Plug-ins gefunden.“).
#[test]
fn unreadable_folder_is_reported() {
    let places = orte("unlesbar", &[], &[]);
    std::fs::remove_dir_all(&places.user).unwrap();
    std::fs::write(&places.user, b"keine Ordner").unwrap();
    let mut h = harness("unlesbar", places, Config::default());
    let seen = bild(&mut h, vec![]);
    assert!(has(&seen, "Keine Plug-ins gefunden."), "{}", join(&seen));
    assert!(
        seen.iter().any(|s| s
            .text
            .starts_with("Der Ordner der installierten Plug-ins lässt sich nicht lesen:")),
        "{}",
        join(&seen)
    );
}

/// Die Einstellungen haben keinen Abschnitt „Plug-ins“ mehr.
#[test]
fn settings_have_no_plugin_section() {
    let mut h = harness(
        "einstellungen",
        orte("einstellungen", &[], &["test-faelle"]),
        Config::default(),
    );
    h.w.close();
    let mut settings = planora_app::settings_window::SettingsWindow::default();
    settings.open(false);
    let raw = egui::RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, h.size)),
        ..Default::default()
    };
    let mut seen = Vec::new();
    for _ in 0..3 {
        let out = h.ctx.run_ui(raw.clone(), |ui| {
            let ctx = ui.ctx().clone();
            settings.show_in(&ctx, &mut h.ws);
        });
        seen = collect_seen(&out.shapes);
        out.drop_without_applying_deltas();
    }
    assert!(has(&seen, "Tastenkürzel"), "{}", join(&seen));
    for text in ["Plug-ins", "Test-Fälle", "Aus Datei…", "Repo-Link"] {
        assert!(!has(&seen, text), "„{text}“: {}", join(&seen));
    }
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

/// Handtest F1: Beispiel-Hallo ausschalten, Test-Fälle entfernen, Programm neu starten (neues
/// `Plugins::new` auf denselben Ordnern und der geschriebenen Konfiguration). Das ausgeschaltete
/// Plug-in bleibt in der Liste (Haken aus) und sein Zustand in der Datei.
#[test]
fn disabled_plugin_survives_remove_and_restart() {
    let places = orte("aus-bleibt", &[], &["beispiel-hallo", "test-faelle"]);
    let mut h = harness("aus-bleibt", places.clone(), Config::default());
    assert_eq!(entry_state(&h, HALLO), Some(EntryState::Running));
    schalten(&mut h, HALLO);
    assert_eq!(entry_state(&h, HALLO), Some(EntryState::Off));
    assert_eq!(
        gespeichert(&h, HALLO).map(|c| c.enabled),
        Some(false),
        "Ausschalten gleich gespeichert"
    );
    plugins::remove(&mut h.ws, FAELLE);
    warten(&mut h);
    assert_eq!(
        gespeichert(&h, HALLO).map(|c| c.enabled),
        Some(false),
        "Entfernen eines anderen ändert nichts"
    );
    // Neustart: das alte Programm endet, das neue liest Ordner und Datei.
    let config_path = h.ws.config_path.clone();
    drop(h);
    let mut ws = Workspace::new(None, Config::load_from(&config_path));
    ws.config_path = config_path;
    ws.plugins = Plugins::new(places, &ws.config);
    ws.plugins.host.set_language("de");
    let mut w = AddinsWindow::default();
    w.open();
    let mut h = H {
        ws,
        ctx: fresh_ctx(),
        w,
        size: vec2(1920.0, 1080.0),
    };
    let seen = warten(&mut h);
    assert_eq!(entry_state(&h, HALLO), Some(EntryState::Off));
    assert_eq!(entry_state(&h, FAELLE), None);
    assert!(has(&seen, "Beispiel-Hallo"), "{}", join(&seen));
    assert!(!has(&seen, "Keine Plug-ins gefunden."), "{}", join(&seen));
    assert_eq!(gespeichert(&h, HALLO).map(|c| c.enabled), Some(false));
    // Das Protokoll sagt, wie viele Plug-ins der Start gefunden hat (eine leere Liste fällt so
    // im Protokoll auf).
    assert!(
        planora_app::diag::recent_lines(planora_app::diag::RING_LINES)
            .iter()
            .any(|l| l.ends_with("Plug-ins found: 1 (installed: 1, switched off: 1)")),
        "{:?}",
        planora_app::diag::recent_lines(20)
    );
}

/// Wie [`disabled_plugin_survives_remove_and_restart`], aber wie im Handtest: Test-Fälle ist
/// vorher dreimal abgestürzt (angehalten), dann erst wird Beispiel-Hallo ausgeschaltet.
#[test]
fn disabled_plugin_survives_remove_of_stopped_plugin() {
    let places = orte("aus-bleibt-2", &[], &["beispiel-hallo", "test-faelle"]);
    let mut h = harness("aus-bleibt-2", places.clone(), Config::default());
    for _ in 0..3 {
        h.ws.plugins.host.run_command(FAELLE, "panik");
        let start = Instant::now();
        while !matches!(
            entry_state(&h, FAELLE),
            Some(EntryState::Failed(_) | EntryState::Stopped)
        ) {
            bild(&mut h, vec![]);
            assert!(start.elapsed() < Duration::from_secs(60));
            std::thread::sleep(Duration::from_millis(10));
        }
        h.ws.plugins.restart(FAELLE);
        warten(&mut h);
    }
    schalten(&mut h, HALLO);
    assert_eq!(gespeichert(&h, HALLO).map(|c| c.enabled), Some(false));
    plugins::remove(&mut h.ws, FAELLE);
    warten(&mut h);
    assert_eq!(gespeichert(&h, HALLO).map(|c| c.enabled), Some(false));
    let config_path = h.ws.config_path.clone();
    drop(h);
    let mut ws = Workspace::new(None, Config::load_from(&config_path));
    ws.config_path = config_path;
    ws.plugins = Plugins::new(places, &ws.config);
    assert_eq!(
        ws.plugins
            .host
            .entries()
            .iter()
            .map(|e| (e.id.clone(), e.state.clone()))
            .collect::<Vec<_>>(),
        vec![(HALLO.to_string(), EntryState::Off)]
    );
}

// ---------------------------------------------------------------------------------------------
// Plug-in-Katalog Task 3: der Reiter „Katalog“. Das Netz ist eine Nachbildung (Antworten je
// Adresse aus einer Tabelle); die Adresse des Katalog-Repos ist fest, darum laufen diese Tests
// nacheinander (`REIHE`). Zwischenspeicher und Konfiguration liegen in Temp.
// ---------------------------------------------------------------------------------------------

type Answer = Result<(u16, Vec<u8>), String>;

static REIHE: Mutex<()> = Mutex::new(());
static NETZ: Mutex<Vec<(String, Answer)>> = Mutex::new(Vec::new());
static GEFRAGT: Mutex<Vec<String>> = Mutex::new(Vec::new());
/// Kein Netz: jede Frage scheitert.
static NETZ_AUS: Mutex<bool> = Mutex::new(false);

const KATALOG_TAG: &str = "katalog-1";

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

fn antwort(url: &str, a: Answer) {
    let mut netz = lock(&NETZ);
    netz.retain(|(u, _)| u != url);
    netz.push((url.to_string(), a));
}

fn netz(url: &str, max: u64, _timeout: Duration) -> Answer {
    lock(&GEFRAGT).push(url.to_string());
    if *lock(&NETZ_AUS) {
        return Err("keine Verbindung".to_string());
    }
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

fn gefragt(url: &str) -> usize {
    lock(&GEFRAGT).iter().filter(|u| u.as_str() == url).count()
}

/// Sperrt die Reihe und leert die Nachbildung.
fn netz_anfang() -> MutexGuard<'static, ()> {
    let g = lock(&REIHE);
    lock(&NETZ).clear();
    lock(&GEFRAGT).clear();
    *lock(&NETZ_AUS) = false;
    g
}

fn katalog_asset(name: &str) -> String {
    format!(
        "https://github.com/{}/{}/releases/download/{KATALOG_TAG}/{name}",
        planora_update::catalog::CATALOG_OWNER,
        planora_update::catalog::CATALOG_REPO
    )
}

/// Ein Eintrag der `katalog.json`: Kennung, Name (de), Beschreibung (de), Kategorie, Repo.
type Eintrag = (
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    String,
);

/// Legt ein Katalog-Release mit `text` als `katalog.json` in die Nachbildung.
fn katalog_text(text: &str) {
    let release = format!(
        "{{\"tag_name\":\"{KATALOG_TAG}\",\"draft\":false,\"assets\":[{{\"name\":\"katalog.json\",\
         \"browser_download_url\":\"{}\",\"size\":{}}}]}}",
        katalog_asset("katalog.json"),
        text.len()
    );
    antwort(
        &planora_update::catalog::latest_release_url(),
        Ok((200, release.into_bytes())),
    );
    antwort(
        &katalog_asset("katalog.json"),
        Ok((200, text.as_bytes().to_vec())),
    );
}

/// Legt ein Katalog-Release mit diesen Einträgen (Format `format`) in die Nachbildung.
fn katalog_release(format: u32, entries: &[Eintrag]) {
    let list: Vec<String> = entries
        .iter()
        .map(|(id, name, desc, cat, repo)| {
            format!(
                "{{\"id\":\"{id}\",\"name\":{{\"de\":\"{name}\",\"en\":\"{name}\"}},\
                 \"beschreibung\":{{\"de\":\"{desc}\",\"en\":\"{desc}\"}},\
                 \"kategorie\":\"{cat}\",\"repo\":\"{repo}\"}}"
            )
        })
        .collect();
    katalog_text(&format!(
        "{{\"format\":{format},\"eintraege\":[{}]}}",
        list.join(",")
    ));
}

const HALLO_REPO: &str = "https://github.com/KaelanTesseract/PlanoraCAD-Plugin-Hallo";

/// Zwei Einträge: Elektronik-Helfer (elektronik) und Hallo-Beispiel (beispiele).
fn zwei_eintraege() -> Vec<Eintrag> {
    vec![
        (
            "planora.elektronik-helfer",
            "Elektronik-Helfer",
            "Schaltpläne und Bauteile",
            "elektronik",
            "https://github.com/jemand/elektronik-helfer".to_string(),
        ),
        (
            HALLO,
            "Hallo-Beispiel",
            "Ein kleines Beispiel",
            "beispiele",
            HALLO_REPO.to_string(),
        ),
    ]
}

/// Fenster auf dem Reiter „Katalog“ mit der Nachbildung als Netz; wartet, bis nichts mehr lädt.
fn katalog_harness(name: &str, places: Places) -> H {
    let mut h = harness(name, places, Config::default());
    h.ws.plugins.repo_fetch = netz;
    h.w.open_at(AddinsTab::Catalog);
    katalog_warten(&mut h);
    h
}

/// Bilder, bis weder der Katalog noch ein Holen oder Prüfen läuft (höchstens 60 s).
fn katalog_warten(h: &mut H) -> Vec<Seen> {
    let start = Instant::now();
    loop {
        bild(h, vec![]);
        let p = &h.ws.plugins;
        if !(p.catalog.is_loading() || p.is_fetching() || p.is_checking()) {
            bild(h, vec![]);
            return bild(h, vec![]);
        }
        assert!(start.elapsed() < Duration::from_secs(60), "lädt zu lange");
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn leere_orte(name: &str) -> Places {
    orte(name, &[], &[])
}

#[test]
fn catalog_tab_lists_entries() {
    let _g = netz_anfang();
    katalog_release(1, &zwei_eintraege());
    let mut h = katalog_harness("kat-liste", leere_orte("kat-liste"));
    let seen = bild(&mut h, vec![]);
    assert_eq!(h.w.tab(), AddinsTab::Catalog);
    for text in [
        "Installiert",
        "Katalog",
        "Elektronik-Helfer",
        "Hallo-Beispiel",
        "Elektronik",
        "Beispiele",
    ] {
        assert!(has(&seen, text), "„{text}“ fehlt: {}", join(&seen));
    }
    // Der Reiter „Installiert“ bringt die bisherige Ansicht zurück.
    klick(&mut h, &seen, "Installiert");
    assert_eq!(h.w.tab(), AddinsTab::Installed);
    let seen = bild(&mut h, vec![]);
    assert!(has(&seen, "Keine Plug-ins gefunden."), "{}", join(&seen));
    assert!(!has(&seen, "Hallo-Beispiel"), "{}", join(&seen));
}

#[test]
fn switching_to_catalog_requests_it() {
    let _g = netz_anfang();
    katalog_release(1, &zwei_eintraege());
    let mut h = harness("kat-wechsel", leere_orte("kat-wechsel"), Config::default());
    h.ws.plugins.repo_fetch = netz;
    let seen = bild(&mut h, vec![]);
    assert_eq!(h.w.tab(), AddinsTab::Installed);
    assert_eq!(gefragt(&planora_update::catalog::latest_release_url()), 0);
    klick(&mut h, &seen, "Katalog");
    assert_eq!(h.w.tab(), AddinsTab::Catalog);
    katalog_warten(&mut h);
    assert_eq!(gefragt(&planora_update::catalog::latest_release_url()), 1);
}

#[test]
fn search_filters() {
    let _g = netz_anfang();
    katalog_release(1, &zwei_eintraege());
    let mut h = katalog_harness("kat-suche", leere_orte("kat-suche"));
    h.w.set_search("ELEK");
    let seen = bild(&mut h, vec![]);
    assert!(has(&seen, "Elektronik-Helfer"), "{}", join(&seen));
    assert!(!has(&seen, "Hallo-Beispiel"), "{}", join(&seen));
    // Auch die Beschreibung zählt, ohne Groß/Klein.
    h.w.set_search("kleines bei");
    let seen = bild(&mut h, vec![]);
    assert!(has(&seen, "Hallo-Beispiel"), "{}", join(&seen));
    assert!(!has(&seen, "Elektronik-Helfer"), "{}", join(&seen));
    h.w.set_search("gibtesnicht");
    let seen = bild(&mut h, vec![]);
    assert!(
        has(&seen, "Keine passenden Plug-ins gefunden."),
        "{}",
        join(&seen)
    );
}

#[test]
fn category_filter() {
    let _g = netz_anfang();
    katalog_release(1, &zwei_eintraege());
    let mut h = katalog_harness("kat-kategorie", leere_orte("kat-kategorie"));
    assert_eq!(h.w.categories(&h.ws.plugins), ["elektronik", "beispiele"]);
    h.w.set_category(Some("beispiele"));
    let seen = bild(&mut h, vec![]);
    assert!(has(&seen, "Hallo-Beispiel"), "{}", join(&seen));
    assert!(!has(&seen, "Elektronik-Helfer"), "{}", join(&seen));
    h.w.set_category(None);
    let seen = bild(&mut h, vec![]);
    assert!(has(&seen, "Elektronik-Helfer"), "{}", join(&seen));
    assert!(has(&seen, "Hallo-Beispiel"), "{}", join(&seen));
}

#[test]
fn unknown_category_is_other() {
    let _g = netz_anfang();
    katalog_release(
        1,
        &[(
            "planora.seltsam",
            "Seltsam",
            "x",
            "voellig-unbekannt",
            "https://github.com/jemand/seltsam".to_string(),
        )],
    );
    let mut h = katalog_harness("kat-sonstiges", leere_orte("kat-sonstiges"));
    let seen = bild(&mut h, vec![]);
    assert!(has(&seen, "Sonstiges"), "{}", join(&seen));
    assert!(!has(&seen, "voellig-unbekannt"), "{}", join(&seen));
}

#[test]
fn details_show_source_and_rights_note() {
    let _g = netz_anfang();
    katalog_release(1, &zwei_eintraege());
    let mut h = katalog_harness("kat-details", leere_orte("kat-details"));
    h.w.select_catalog(HALLO);
    let seen = bild(&mut h, vec![]);
    for text in [
        "Ein kleines Beispiel",
        "Kategorie: Beispiele",
        &format!("Quelle: {HALLO_REPO}"),
        "Neueste Version: –",
        "Rechte werden beim Installieren angezeigt",
        "Installieren",
    ] {
        assert!(has(&seen, text), "„{text}“ fehlt: {}", join(&seen));
    }
}

#[test]
fn install_button_starts_link_install() {
    let _g = netz_anfang();
    katalog_release(1, &zwei_eintraege());
    let mut h = katalog_harness("kat-install", leere_orte("kat-install"));
    h.w.select_catalog(HALLO);
    let seen = bild(&mut h, vec![]);
    klick(&mut h, &seen, "Installieren");
    // Der Abruf läuft für die Adresse des Katalogeintrags.
    katalog_warten(&mut h);
    let url = "https://api.github.com/repos/KaelanTesseract/PlanoraCAD-Plugin-Hallo/releases?per_page=100";
    assert_eq!(gefragt(url), 1, "{:?}", lock(&GEFRAGT));
}

/// Hallo-Beispiel (0.1.0) ist installiert; `owner` ist sein Repo.
fn installiert_aus_katalog(name: &str, owner: &str, catalog: bool) -> Places {
    let p = leere_orte(name);
    plugin_places::install(
        &p,
        &paket("beispiel-hallo"),
        Some(&Provenance {
            link: format!("https://github.com/{owner}/hallo"),
            tag: "v0.1.0".to_string(),
            catalog,
        }),
    )
    .unwrap();
    p
}

fn hallo_eintrag(owner: &str) -> Vec<Eintrag> {
    vec![(
        HALLO,
        "Hallo-Beispiel",
        "Ein kleines Beispiel",
        "beispiele",
        format!("https://github.com/{owner}/hallo"),
    )]
}

#[test]
fn installed_shows_installed() {
    let _g = netz_anfang();
    katalog_release(1, &hallo_eintrag("kat-inst"));
    let mut h = katalog_harness(
        "kat-installiert",
        installiert_aus_katalog("kat-installiert", "kat-inst", true),
    );
    h.w.select_catalog(HALLO);
    let seen = bild(&mut h, vec![]);
    assert!(has(&seen, "Neueste Version: 0.1.0"), "{}", join(&seen));
    assert!(!has(&seen, "Installieren"), "{}", join(&seen));
    assert!(
        !seen.iter().any(|s| s.text.starts_with("Aktualisieren auf")),
        "{}",
        join(&seen)
    );
    // Der Reiter „Installiert“ und der abgeschaltete Knopf „Installiert“.
    let n = seen.iter().filter(|s| s.text == "Installiert").count();
    assert_eq!(n, 2, "{}", join(&seen));
    // Ein Klick auf den Knopf tut nichts.
    klick(&mut h, &seen, "Installiert");
    assert!(!h.ws.plugins.is_fetching());
    assert_eq!(h.w.tab(), AddinsTab::Catalog);
}

/// Review Focus 5: ist ein Update gefunden, steht statt „Installiert“ „Aktualisieren auf …“
/// und der Knopf öffnet den bestehenden Aktualisieren-Dialog.
#[test]
fn update_shows_update_button() {
    let _g = netz_anfang();
    let owner = "kat-upd";
    katalog_release(1, &hallo_eintrag(owner));
    // Ein Release v0.2.0 des Plug-ins in der Nachbildung.
    let neu = paket_version("0.2.0");
    let name = "hallo.pcplugin";
    let pkg_url = format!("https://github.com/{owner}/hallo/releases/download/v0.2.0/{name}");
    antwort(&pkg_url, Ok((200, neu.clone())));
    let sum_url = format!("{pkg_url}.sha256");
    let summe = format!("{}  {name}\n", planora_update::sha256::hex_digest(&neu));
    antwort(&sum_url, Ok((200, summe.into_bytes())));
    let json = format!(
        "[{{\"tag_name\":\"v0.2.0\",\"draft\":false,\"prerelease\":false,\"body\":\"Neu\",\
         \"assets\":[{{\"name\":\"{name}\",\"browser_download_url\":\"{pkg_url}\",\"size\":{}}},\
         {{\"name\":\"{name}.sha256\",\"browser_download_url\":\"{sum_url}\",\"size\":80}}]}}]",
        neu.len()
    );
    antwort(
        &format!("https://api.github.com/repos/{owner}/hallo/releases?per_page=100"),
        Ok((200, json.into_bytes())),
    );
    let mut h = katalog_harness(
        "kat-update",
        installiert_aus_katalog("kat-update", owner, true),
    );
    h.ws.plugins.check_updates(true);
    katalog_warten(&mut h);
    assert_eq!(h.ws.plugins.updates().len(), 1);
    h.w.select_catalog(HALLO);
    let seen = bild(&mut h, vec![]);
    assert!(has(&seen, "Aktualisieren auf 0.2.0"), "{}", join(&seen));
    assert!(has(&seen, "Neueste Version: 0.2.0"), "{}", join(&seen));
    assert!(!has(&seen, "Installieren"), "{}", join(&seen));
    klick(&mut h, &seen, "Aktualisieren auf 0.2.0");
    assert_eq!(h.ws.plugins.update_dialog.as_deref(), Some(HALLO));
}

/// Ein Paket aus dem Wasm von beispiel-hallo mit der Version `version`.
fn paket_version(version: &str) -> Vec<u8> {
    let hallo = paket("beispiel-hallo");
    let toml = format!(
        "id = \"{HALLO}\"\nname = \"Beispiel-Hallo\"\nversion = \"{version}\"\napi = \"0.1\"\n\
         author = \"Dennis Guse\"\nlicense = \"GPL-3.0-only\"\ndescription = \"Test\"\n"
    );
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
    zip.finish().unwrap()
}

#[test]
fn failed_shows_retry() {
    let _g = netz_anfang();
    // Kein Release in der Nachbildung: 404, kein Zwischenspeicher.
    let mut h = katalog_harness("kat-fehler", leere_orte("kat-fehler"));
    let seen = bild(&mut h, vec![]);
    for text in [
        "Der Katalog konnte nicht geladen werden",
        "Erneut versuchen",
    ] {
        assert!(has(&seen, text), "„{text}“ fehlt: {}", join(&seen));
    }
    let url = planora_update::catalog::latest_release_url();
    let vorher = gefragt(&url);
    assert!(vorher >= 1);
    // „Erneut versuchen“ holt sofort neu (auch innerhalb der Wartezeit); jetzt gibt es ihn.
    katalog_release(1, &zwei_eintraege());
    klick(&mut h, &seen, "Erneut versuchen");
    katalog_warten(&mut h);
    assert_eq!(gefragt(&url), vorher + 1);
    let seen = bild(&mut h, vec![]);
    assert!(has(&seen, "Hallo-Beispiel"), "{}", join(&seen));
    assert!(!has(&seen, "Erneut versuchen"), "{}", join(&seen));
}

#[test]
fn offline_hint_shown() {
    let _g = netz_anfang();
    katalog_release(1, &zwei_eintraege());
    let mut h = katalog_harness("kat-offline", leere_orte("kat-offline"));
    let seen = bild(&mut h, vec![]);
    assert!(
        !seen.iter().any(|s| s.text.starts_with("offline")),
        "{}",
        join(&seen)
    );
    let fetched = h.ws.plugins.catalog.fetched_at().unwrap();
    // Netz weg, neu holen: der alte Stand bleibt, dazu der Hinweis mit dem Datum.
    *lock(&NETZ_AUS) = true;
    let spaeter = planora_app::update_check::now_unix() + 3600;
    h.ws.plugins.catalog.request(netz, spaeter, true);
    katalog_warten(&mut h);
    let seen = bild(&mut h, vec![]);
    assert!(has(&seen, "Hallo-Beispiel"), "{}", join(&seen));
    let hinweis = format!(
        "offline – Stand vom {}",
        planora_app::local_time::unix_time_text(fetched as i64)
    );
    assert!(has(&seen, &hinweis), "„{hinweis}“ fehlt: {}", join(&seen));
}

#[test]
fn too_new_hint_shown() {
    let _g = netz_anfang();
    katalog_release(99, &zwei_eintraege());
    let mut h = katalog_harness("kat-zu-neu", leere_orte("kat-zu-neu"));
    let seen = bild(&mut h, vec![]);
    assert!(
        has(&seen, "Der Katalog braucht eine neuere PlanoraCAD-Version."),
        "{}",
        join(&seen)
    );
    assert!(!has(&seen, "Hallo-Beispiel"), "{}", join(&seen));
}

#[test]
fn empty_name_shows_id() {
    let _g = netz_anfang();
    katalog_text(
        "{\"format\":1,\"eintraege\":[{\"id\":\"planora.ohne-namen\",\"name\":{},\
         \"kategorie\":\"werkzeuge\",\"repo\":\"https://github.com/jemand/ohne-namen\"}]}",
    );
    let mut h = katalog_harness("kat-ohne-namen", leere_orte("kat-ohne-namen"));
    let seen = bild(&mut h, vec![]);
    assert!(has(&seen, "planora.ohne-namen"), "{}", join(&seen));
}

#[test]
fn catalog_provenance_shown() {
    let _g = netz_anfang();
    let mut h = harness(
        "kat-herkunft",
        installiert_aus_katalog("kat-herkunft", "kat-h", true),
        Config::default(),
    );
    let seen = bild(&mut h, vec![]);
    assert_eq!(h.w.tab(), AddinsTab::Installed);
    assert!(has(&seen, "aus dem Katalog"), "{}", join(&seen));
    // Über einen Link installiert: kein Vermerk.
    let mut h = harness(
        "kat-herkunft-link",
        installiert_aus_katalog("kat-herkunft-link", "kat-h", false),
        Config::default(),
    );
    let seen = bild(&mut h, vec![]);
    assert!(!has(&seen, "aus dem Katalog"), "{}", join(&seen));
}

#[test]
fn layout_1920x1080_fits() {
    let _g = netz_anfang();
    let eintraege: Vec<Eintrag> = (0..30)
        .map(|i| {
            let id: &'static str =
                Box::leak(format!("planora.langer-eintrag-{i}").into_boxed_str());
            (
                id,
                "Ein Plug-in mit einem ziemlich langen Namen für den Katalog",
                "Eine lange Beschreibung, die über mehrere Zeilen umbrechen muss, damit man sieht, \
                 dass nichts aus dem Fenster ragt und alles in der Breite des Fensters bleibt.",
                "elektronik",
                format!("https://github.com/jemand/langer-eintrag-{i}"),
            )
        })
        .collect();
    katalog_release(1, &eintraege);
    let mut h = katalog_harness("kat-layout", leere_orte("kat-layout"));
    let seen = bild(&mut h, vec![]);
    let window = h.w.last_rect.unwrap();
    let screen = Rect::from_min_size(Pos2::ZERO, h.size);
    assert!(screen.contains_rect(window), "{window:?}");
    assert!(window.top() >= theme::HEADER_H, "{window:?}");
    for text in [
        "Installiert",
        "Katalog",
        "Installieren",
        "Rechte werden beim Installieren angezeigt",
        "Schließen",
    ] {
        let s = find(&seen, text).unwrap_or_else(|| panic!("„{text}“ fehlt: {}", join(&seen)));
        assert!(window.contains_rect(s.rect), "„{text}“ im Fenster");
        assert!(s.clip.contains_rect(s.rect), "„{text}“ ganz zu sehen");
    }
    // Nichts ragt rechts aus dem Fenster.
    for s in &seen {
        if s.clip.intersects(window) && window.intersects(s.rect) {
            assert!(
                s.rect.right() <= window.right() + 0.5,
                "„{}“ ragt rechts heraus",
                s.text
            );
        }
    }
}

/// Symbole: ein PNG wird gelesen; SVG, kaputte und zu große Bilder ergeben `None` (dann steht das
/// Standardsymbol), ein Eintrag mit kaputtem Symbol erscheint trotzdem in der Liste.
#[test]
fn symbols_decode_or_fall_back() {
    use planora_app::addins_window::decode_symbol;
    let png = planora_app::logo::ICON_64;
    let image = decode_symbol(png).expect("PNG lesbar");
    assert_eq!(image.size, [64, 64]);
    assert!(decode_symbol(b"<svg xmlns='http://www.w3.org/2000/svg'/>").is_none());
    assert!(decode_symbol(&png[..40]).is_none(), "abgeschnitten");
    assert!(decode_symbol(b"kein Bild").is_none());
    // Kopf mit 100000 x 100000 Pixeln: abgelehnt, ohne zu entpacken.
    let mut riesig = png.to_vec();
    riesig[16..20].copy_from_slice(&100_000u32.to_be_bytes());
    riesig[20..24].copy_from_slice(&100_000u32.to_be_bytes());
    assert!(decode_symbol(&riesig).is_none());
}

#[test]
fn broken_symbol_entry_still_listed() {
    let _g = netz_anfang();
    let text = "{\"format\":1,\"eintraege\":[{\"id\":\"planora.mit-symbol\",\
        \"name\":{\"de\":\"Mit Symbol\"},\"kategorie\":\"werkzeuge\",\
        \"repo\":\"https://github.com/jemand/mit-symbol\",\"symbol\":\"s.png\"}]}";
    let release = format!(
        "{{\"tag_name\":\"{KATALOG_TAG}\",\"draft\":false,\"assets\":[\
         {{\"name\":\"katalog.json\",\"browser_download_url\":\"{}\",\"size\":{}}},\
         {{\"name\":\"s.png\",\"browser_download_url\":\"{}\",\"size\":9}}]}}",
        katalog_asset("katalog.json"),
        text.len(),
        katalog_asset("s.png")
    );
    antwort(
        &planora_update::catalog::latest_release_url(),
        Ok((200, release.into_bytes())),
    );
    antwort(
        &katalog_asset("katalog.json"),
        Ok((200, text.as_bytes().to_vec())),
    );
    antwort(&katalog_asset("s.png"), Ok((200, b"kein Bild".to_vec())));
    let mut h = katalog_harness("kat-symbol", leere_orte("kat-symbol"));
    let seen = bild(&mut h, vec![]);
    assert!(has(&seen, "Mit Symbol"), "{}", join(&seen));
}

/// Fix-Runde 1: Symbole über 256 px Kantenlänge gelten als unbrauchbar (Standardsymbol); der
/// Kopf wird vorab geprüft.
#[test]
fn symbol_size_is_capped() {
    use planora_app::addins_window::{SYMBOL_MAX_PIXELS, decode_symbol};
    assert_eq!(SYMBOL_MAX_PIXELS, 256);
    let png = planora_app::logo::ICON_256;
    // Das 256-px-Symbol des Programms liegt genau auf der Grenze und wird gelesen.
    assert_eq!(decode_symbol(png).expect("256 px").size, [256, 256]);
    let mut breit = png.to_vec();
    breit[16..20].copy_from_slice(&257u32.to_be_bytes());
    breit[20..24].copy_from_slice(&1u32.to_be_bytes());
    assert!(decode_symbol(&breit).is_none(), "257 x 1");
    let mut hoch = png.to_vec();
    hoch[16..20].copy_from_slice(&1u32.to_be_bytes());
    hoch[20..24].copy_from_slice(&257u32.to_be_bytes());
    assert!(decode_symbol(&hoch).is_none(), "1 x 257");
}

/// Ein Katalog mit dem Eintrag `id` und dem Symbol `s.png` mit den Bytes `png`.
fn katalog_mit_symbol(png: &[u8]) {
    let text = "{\"format\":1,\"eintraege\":[{\"id\":\"planora.mit-symbol\",\
        \"name\":{\"de\":\"Mit Symbol\"},\"kategorie\":\"werkzeuge\",\
        \"repo\":\"https://github.com/jemand/mit-symbol\",\"symbol\":\"s.png\"}]}";
    let release = format!(
        "{{\"tag_name\":\"{KATALOG_TAG}\",\"draft\":false,\"assets\":[\
         {{\"name\":\"katalog.json\",\"browser_download_url\":\"{}\",\"size\":{}}},\
         {{\"name\":\"s.png\",\"browser_download_url\":\"{}\",\"size\":{}}}]}}",
        katalog_asset("katalog.json"),
        text.len(),
        katalog_asset("s.png"),
        png.len()
    );
    antwort(
        &planora_update::catalog::latest_release_url(),
        Ok((200, release.into_bytes())),
    );
    antwort(
        &katalog_asset("katalog.json"),
        Ok((200, text.as_bytes().to_vec())),
    );
    antwort(&katalog_asset("s.png"), Ok((200, png.to_vec())));
}

/// Fix-Runde 1: ein neuer Stand mit gleichem Symbolnamen und gleicher Länge, aber anderem Inhalt
/// ergibt eine neue Textur (der Zwischenspeicher hängt am Stand, nicht an Name und Länge).
#[test]
fn symbol_texture_refreshed_on_new_catalog() {
    let _g = netz_anfang();
    let alt = planora_app::logo::ICON_64.to_vec();
    // Gleicher Name, gleiche Länge (hier dasselbe Bild): maßgeblich ist der neue Stand, nicht
    // Name und Länge.
    let neu = alt.clone();
    katalog_mit_symbol(&alt);
    let mut h = katalog_harness("kat-textur", leere_orte("kat-textur"));
    bild(&mut h, vec![]);
    let erste = h.w.symbol_texture_id("s.png").expect("Textur");
    // Derselbe Stand: dieselbe Textur.
    bild(&mut h, vec![]);
    assert_eq!(h.w.symbol_texture_id("s.png"), Some(erste));
    // Neuer Stand (anderer Zeitpunkt), gleicher Name, gleiche Länge: neue Textur.
    katalog_mit_symbol(&neu);
    let spaeter = planora_app::update_check::now_unix() + 3600;
    h.ws.plugins.catalog.request(netz, spaeter, true);
    katalog_warten(&mut h);
    bild(&mut h, vec![]);
    let zweite = h.w.symbol_texture_id("s.png").expect("neue Textur");
    assert_ne!(zweite, erste, "alter Stand bleibt nicht stehen");
}

/// Fix-Runde 1: eine gewählte Kategorie, die nach dem Neuladen fehlt, wird zu „Alle“.
#[test]
fn missing_category_resets_to_all() {
    let _g = netz_anfang();
    katalog_release(1, &zwei_eintraege());
    let mut h = katalog_harness("kat-kat-weg", leere_orte("kat-kat-weg"));
    h.w.set_category(Some("beispiele"));
    let seen = bild(&mut h, vec![]);
    assert!(!has(&seen, "Elektronik-Helfer"), "{}", join(&seen));
    assert_eq!(h.w.category(), Some("beispiele"));
    // Der neue Stand kennt nur noch Elektronik.
    katalog_release(1, &zwei_eintraege()[..1]);
    let spaeter = planora_app::update_check::now_unix() + 3600;
    h.ws.plugins.catalog.request(netz, spaeter, true);
    katalog_warten(&mut h);
    let seen = bild(&mut h, vec![]);
    assert_eq!(h.w.category(), None);
    assert!(has(&seen, "Elektronik-Helfer"), "{}", join(&seen));
}
