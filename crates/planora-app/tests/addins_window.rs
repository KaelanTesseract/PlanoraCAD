//! Plug-ins Etappe 1, Task 7 und Handtest W1/W2: das Fenster „Zusatzmodule“ (Verwaltung der
//! Plug-ins, früher ein Abschnitt der Einstellungen) und der Installationsdialog. Gezeichnet
//! wird, was die App je Bild zeichnet: Rückfragen, das Fenster und die Plug-ins (mit dem
//! Installationsdialog), bei 1920 × 1080.

mod common;

use common::{Seen, collect_seen, find, join};
use egui::{Event, Modifiers, PointerButton, Pos2, Rect, vec2};
use planora_app::addins_window::AddinsWindow;
use planora_app::plugins::{self, Plugins};
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
