//! Plug-ins im Programm (Plug-ins Etappe 1, Task 5): Datei → „Zusatzmodule“, Meldungen in der
//! Statuszeile, Rechte-Nachfrage, Netzanfragen und die Fehlerleiste. Ohne Fenster: Rohereignisse
//! je Bild gegen einen egui-Kontext. Die Test-Pakete aus `planora-plugin-host` werden in eigene
//! Temp-Fundorte installiert, die Konfiguration liegt in Temp; das Netz ist eine Nachbildung.

mod common;

use common::{Seen, collect_seen, find, join};
use egui::{Event, Key, Modifiers, PointerButton, Pos2, Rect, vec2};
use planora_app::app::{self, MenuRequest, PlanoraApp};
use planora_app::plugins::{self, Plugins};
use planora_app::settings_window::{ChooserPurpose, SettingsSection};
use planora_app::shell::ShellState;
use planora_app::workspace::{Prompt, Workspace};
use planora_app::{fonts, theme};
use planora_plugin_host::EntryState;
use planora_plugin_host::manifest::Right;
use planora_plugin_host::package::Package;
use planora_plugin_host::places::{self as plugin_places, Places};
use planora_plugin_host::rights::{Answer, Grant};
use planora_project::config::{Config, PluginConfig};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant};

const HALLO: &str = "planora.beispiel-hallo";
const FAELLE: &str = "planora.test-faelle";

/// Fundorte in Temp mit den Paketen `pakete` (Namen der Dateien in den Testdaten des Hosts).
fn orte(name: &str, pakete: &[&str]) -> Places {
    let root = common::temp_root(&format!("planora-plugins-{name}"));
    let p = Places {
        bundled: vec![root.join("mitgeliefert")],
        user: root.join("benutzer"),
        cache: root.join("cache"),
    };
    for paket in pakete {
        let file = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../planora-plugin-host/tests/data")
            .join(format!("{paket}.pcplugin"));
        let pkg = Package::read(&std::fs::read(file).unwrap()).unwrap();
        plugin_places::install(&p, &pkg).unwrap();
    }
    p
}

/// Arbeitsbereich mit Konfiguration in Temp und gestarteten Plug-ins aus `pakete`; `config` ist
/// der gespeicherte Stand beim Start.
fn arbeitsbereich(name: &str, pakete: &[&str], config: Config) -> (Workspace, egui::Context) {
    let mut ws = Workspace::new(None, config);
    let cfg = common::temp_root(&format!("planora-plugins-cfg-{name}"));
    std::fs::create_dir_all(&cfg).unwrap();
    ws.config_path = cfg.join("config.json");
    ws.plugins = Plugins::new(orte(name, pakete), &ws.config);
    ws.plugins.host.set_language("de");
    ws.plugins.fetch = netz_ok;
    let ctx = egui::Context::default();
    ctx.set_fonts(fonts::definitions());
    theme::apply(&ctx, true);
    warten(&mut ws, &ctx);
    for paket in pakete {
        assert_eq!(state(&ws, &format!("planora.{paket}")), EntryState::Running);
    }
    (ws, ctx)
}

fn state(ws: &Workspace, id: &str) -> EntryState {
    ws.plugins
        .host
        .entries()
        .iter()
        .find(|e| e.id == id)
        .unwrap()
        .state
        .clone()
}

fn net_grant(ws: &Workspace) -> Option<Grant> {
    ws.plugins
        .host
        .entries()
        .iter()
        .find(|e| e.id == FAELLE)
        .unwrap()
        .rights
        .state(Right::Net)
}

/// Ein Bild mit `events`; zeichnet nur, was `plugins::show` zeichnet. Liefert die Texte.
fn bild(ws: &mut Workspace, ctx: &egui::Context, events: Vec<Event>) -> Vec<Seen> {
    let raw = egui::RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1920.0, 1080.0))),
        events,
        ..Default::default()
    };
    let out = ctx.run_ui(raw, |ui| plugins::show(ui.ctx(), ws));
    let seen = collect_seen(&out.shapes);
    out.drop_without_applying_deltas();
    seen
}

/// Bilder, bis kein Plug-in mehr lädt und kein Netz-Auftrag mehr läuft (höchstens 60 s).
fn warten(ws: &mut Workspace, ctx: &egui::Context) -> Vec<Seen> {
    let start = Instant::now();
    loop {
        let seen = bild(ws, ctx, vec![]);
        let busy = ws
            .plugins
            .host
            .entries()
            .iter()
            .any(|e| e.state == EntryState::Loading)
            || !ws.plugins.net_jobs.is_empty();
        if !busy {
            return seen;
        }
        assert!(start.elapsed() < Duration::from_secs(60), "lädt zu lange");
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// Klick auf den Text `text` im letzten Bild `seen` (drei Bilder: hin, drücken, loslassen).
fn klick(ws: &mut Workspace, ctx: &egui::Context, seen: &[Seen], text: &str) -> Vec<Seen> {
    let at = find(seen, text)
        .unwrap_or_else(|| panic!("„{text}“ fehlt: {}", join(seen)))
        .rect
        .center();
    let b = |pressed| Event::PointerButton {
        pos: at,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    };
    bild(ws, ctx, vec![Event::PointerMoved(at)]);
    bild(ws, ctx, vec![b(true)]);
    bild(ws, ctx, vec![b(false)])
}

fn escape() -> Vec<Event> {
    vec![Event::Key {
        key: Key::Escape,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::NONE,
    }]
}

fn netz_ok(_url: &str) -> Result<(u16, Vec<u8>), String> {
    Ok((200, b"ok".to_vec()))
}

static ADRESSEN: Mutex<Vec<String>> = Mutex::new(Vec::new());

fn netz_merken(url: &str) -> Result<(u16, Vec<u8>), String> {
    ADRESSEN.lock().unwrap().push(url.to_string());
    Ok((404, b"nicht da".to_vec()))
}

fn gespeichert(ws: &Workspace) -> Option<PluginConfig> {
    Config::load_from(&ws.config_path)
        .plugins
        .get(FAELLE)
        .cloned()
}

/// Ein Bild der Menüleiste, Rückgabe: Texte und der Menü-Wunsch.
fn menue_bild(
    ws: &mut Workspace,
    ctx: &egui::Context,
    shell: &mut ShellState,
    events: Vec<Event>,
    got: &mut MenuRequest,
) -> Vec<Seen> {
    let raw = egui::RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1920.0, 1080.0))),
        events,
        ..Default::default()
    };
    let out = ctx.run_ui(raw, |ui| {
        let r = app::menu_bar(ui, ws, shell);
        if r != MenuRequest::None {
            *got = r;
        }
    });
    let seen = collect_seen(&out.shapes);
    out.drop_without_applying_deltas();
    seen
}

fn menue_klick(
    ws: &mut Workspace,
    ctx: &egui::Context,
    shell: &mut ShellState,
    seen: &[Seen],
    text: &str,
    got: &mut MenuRequest,
) -> Vec<Seen> {
    let at = find(seen, text)
        .unwrap_or_else(|| panic!("„{text}“ fehlt: {}", join(seen)))
        .rect
        .center();
    let b = |pressed| Event::PointerButton {
        pos: at,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    };
    menue_bild(ws, ctx, shell, vec![Event::PointerMoved(at)], got);
    menue_bild(ws, ctx, shell, vec![b(true)], got);
    menue_bild(ws, ctx, shell, vec![b(false)], got);
    menue_bild(ws, ctx, shell, vec![], got)
}

#[test]
fn menu_lists_plugin_command() {
    let (mut ws, ctx) = arbeitsbereich("menue", &["beispiel-hallo"], Config::default());
    let mut shell = ShellState::default();
    let mut got = MenuRequest::None;
    let seen = menue_bild(&mut ws, &ctx, &mut shell, vec![], &mut got);
    assert!(
        find(&seen, "Hilfsprogramme").is_none(),
        "kein eigenes Menü in der Leiste"
    );
    let seen = menue_klick(&mut ws, &ctx, &mut shell, &seen, "Datei", &mut got);
    // „Zusatzmodule“ steht direkt über „Einstellungen …“.
    let zusatz = find(&seen, "Zusatzmodule").unwrap_or_else(|| panic!("{}", join(&seen)));
    let einstellungen = find(&seen, &app::settings_entry()).unwrap().rect;
    assert!(zusatz.rect.top() < einstellungen.top());
    assert!(
        einstellungen.top() - zusatz.rect.bottom() < 16.0,
        "direkt darüber: {:?} {einstellungen:?}",
        zusatz.rect
    );
    let seen = menue_klick(&mut ws, &ctx, &mut shell, &seen, "Zusatzmodule", &mut got);
    assert!(find(&seen, "Verwalten …").is_some(), "{}", join(&seen));
    assert!(find(&seen, "Hallo sagen").is_some(), "{}", join(&seen));

    // Der Befehl läuft: das Beispiel öffnet seinen Dialog.
    menue_klick(&mut ws, &ctx, &mut shell, &seen, "Hallo sagen", &mut got);
    assert_eq!(got, MenuRequest::None);
    bild(&mut ws, &ctx, vec![]);
    assert_eq!(ws.plugins.dialogs.len(), 1, "Dialog vermerkt");
    assert_eq!(ws.plugins.dialogs[0].plugin, HALLO);
    assert_eq!(ws.plugins.dialogs[0].spec.id, "gruss");
}

#[test]
fn plugin_message_in_status_line() {
    let (mut ws, ctx) = arbeitsbereich("meldung", &["test-faelle"], Config::default());
    ws.plugins.host.run_command("planora.test-faelle.fehler");
    bild(&mut ws, &ctx, vec![]);
    let msg = ws.active_tab().state.message.clone().unwrap_or_default();
    assert!(msg.starts_with("Test-Fälle: "), "{msg}");
    assert!(msg.contains("absichtlich"), "{msg}");
    assert_eq!(state(&ws, FAELLE), EntryState::Running, "kein Ausfall");
}

#[test]
fn ask_then_allow_saves_config() {
    let (mut ws, ctx) = arbeitsbereich("erlauben", &["test-faelle"], Config::default());
    ws.plugins.host.run_command("planora.test-faelle.recht");
    let seen = bild(&mut ws, &ctx, vec![]);
    assert_eq!(ws.plugins.prompts.len(), 1);
    let seen = if find(&seen, "Erlauben").is_some() {
        seen
    } else {
        bild(&mut ws, &ctx, vec![])
    };
    assert!(
        find(&seen, "Recht für Test-Fälle").is_some(),
        "{}",
        join(&seen)
    );
    assert!(
        find(
            &seen,
            "Das Plug-in Test-Fälle möchte Verbindungen zu example.org aufbauen."
        )
        .is_some(),
        "{}",
        join(&seen)
    );
    assert!(find(&seen, "Nur diesmal").is_some());
    assert!(find(&seen, "Ablehnen").is_some());

    klick(&mut ws, &ctx, &seen, "Erlauben");
    bild(&mut ws, &ctx, vec![]);
    assert!(ws.plugins.prompts.is_empty());
    let rights = &ws.config.plugins[FAELLE].rights;
    assert_eq!(rights.get("net").map(String::as_str), Some("allowed"));
    assert_eq!(
        gespeichert(&ws).unwrap().rights["net"],
        "allowed",
        "auf der Platte"
    );
    // Das Plug-in gilt als berechtigt (sein `on_result(Granted)` ist zugestellt).
    assert_eq!(net_grant(&ws), Some(Grant::Allowed));
    assert_eq!(state(&ws, FAELLE), EntryState::Running);
}

#[test]
fn once_not_saved() {
    let (mut ws, ctx) = arbeitsbereich("einmal", &["test-faelle"], Config::default());
    ws.plugins.host.run_command("planora.test-faelle.recht");
    bild(&mut ws, &ctx, vec![]);
    assert_eq!(ws.plugins.prompts.len(), 1);
    ws.plugins.answer(Answer::Once);
    bild(&mut ws, &ctx, vec![]);
    bild(&mut ws, &ctx, vec![]);
    assert!(!ws.config.plugins.contains_key(FAELLE));
    assert_eq!(gespeichert(&ws), None);

    // In derselben Sitzung ohne neue Nachfrage.
    ws.plugins.host.run_command("planora.test-faelle.recht");
    bild(&mut ws, &ctx, vec![]);
    bild(&mut ws, &ctx, vec![]);
    assert!(ws.plugins.prompts.is_empty());
    assert_eq!(net_grant(&ws), Some(Grant::Allowed));
}

#[test]
fn net_without_right_asks_first() {
    let (mut ws, ctx) = arbeitsbereich("netz-fragt", &["test-faelle"], Config::default());
    ws.plugins.fetch = netz_merken;
    ws.plugins.host.run_command("planora.test-faelle.netz");
    bild(&mut ws, &ctx, vec![]);
    assert_eq!(ws.plugins.prompts.len(), 1, "erst fragen");
    assert!(ws.plugins.net_jobs.is_empty(), "noch nicht ins Netz");
    assert_eq!(ws.plugins.prompts[0].right, Right::Net);

    ws.plugins.answer(Answer::Allow);
    bild(&mut ws, &ctx, vec![]);
    assert_eq!(ws.plugins.net_jobs.len(), 1, "jetzt ins Netz");
    warten(&mut ws, &ctx);
    assert!(
        ADRESSEN
            .lock()
            .unwrap()
            .iter()
            .any(|u| u == "https://example.org/"),
        "genau die Adresse des Plug-ins"
    );
    // Ein 404 ist eine Antwort, kein Ausfall.
    bild(&mut ws, &ctx, vec![]);
    assert_eq!(state(&ws, FAELLE), EntryState::Running);
    assert!(ws.plugins.banners.is_empty());
}

#[test]
fn net_denied_gives_denied() {
    let (mut ws, ctx) = arbeitsbereich("netz-nein", &["test-faelle"], Config::default());
    ws.plugins.host.run_command("planora.test-faelle.netz");
    let mut seen = bild(&mut ws, &ctx, vec![]);
    if find(&seen, "Ablehnen").is_none() {
        seen = bild(&mut ws, &ctx, vec![]);
    }
    assert!(find(&seen, "Ablehnen").is_some(), "{}", join(&seen));
    // Esc wirkt wie „Ablehnen“.
    bild(&mut ws, &ctx, escape());
    bild(&mut ws, &ctx, vec![]);
    assert!(ws.plugins.prompts.is_empty());
    assert!(ws.plugins.net_jobs.is_empty(), "kein Auftrag gestartet");
    assert_eq!(net_grant(&ws), Some(Grant::Denied));
    assert_eq!(gespeichert(&ws).unwrap().rights["net"], "denied");

    // Danach ohne Nachfrage abgelehnt, wieder ohne Auftrag.
    ws.plugins.host.run_command("planora.test-faelle.netz");
    bild(&mut ws, &ctx, vec![]);
    assert!(ws.plugins.prompts.is_empty());
    assert!(ws.plugins.net_jobs.is_empty());
}

#[test]
fn escape_answers_only_one_prompt() {
    let (mut ws, ctx) = arbeitsbereich(
        "esc-einmal",
        &["test-faelle", "beispiel-hallo"],
        Config::default(),
    );
    ws.plugins.host.run_command("planora.test-faelle.recht");
    bild(&mut ws, &ctx, vec![]);
    // Eine zweite Nachfrage wartet (von Hand eingereiht: das Beispiel beantragt nichts).
    let mut zweite = ws.plugins.prompts[0].clone();
    zweite.request += 1000;
    ws.plugins.prompts.push_back(zweite);
    bild(&mut ws, &ctx, vec![]);
    bild(&mut ws, &ctx, escape());
    assert_eq!(
        ws.plugins.prompts.len(),
        1,
        "nur eine Nachfrage beantwortet"
    );
}

#[test]
fn late_answer_dropped() {
    let (mut ws, ctx) = arbeitsbereich("spaet", &["test-faelle"], Config::default());
    ws.plugins.host.run_command("planora.test-faelle.recht");
    bild(&mut ws, &ctx, vec![]);
    assert_eq!(ws.plugins.prompts.len(), 1);
    // Neustart am Host vorbei: die offene Nachfrage gehört zur alten Generation.
    ws.plugins.host.restart(FAELLE);
    ws.plugins.answer(Answer::Allow);
    warten(&mut ws, &ctx);
    bild(&mut ws, &ctx, vec![]);
    assert_eq!(state(&ws, FAELLE), EntryState::Running);
    assert!(
        !ws.config.plugins.contains_key(FAELLE),
        "nichts gespeichert"
    );
    assert_eq!(
        net_grant(&ws),
        None,
        "die neue Instanz ist nicht berechtigt"
    );
    ws.plugins.host.run_command("planora.test-faelle.recht");
    bild(&mut ws, &ctx, vec![]);
    assert_eq!(ws.plugins.prompts.len(), 1, "die neue Instanz fragt wieder");

    // Über `Plugins::restart` fällt die offene Nachfrage gleich weg.
    ws.plugins.restart(FAELLE);
    assert!(ws.plugins.prompts.is_empty());
    warten(&mut ws, &ctx);
}

#[test]
fn late_net_result_dropped() {
    let mut config = Config::default();
    config.plugins.insert(
        FAELLE.to_string(),
        PluginConfig {
            enabled: true,
            rights: BTreeMap::from([("net".to_string(), "allowed".to_string())]),
        },
    );
    let (mut ws, ctx) = arbeitsbereich("netz-spaet", &["test-faelle"], config);
    ws.plugins.host.run_command("planora.test-faelle.netz");
    bild(&mut ws, &ctx, vec![]);
    assert_eq!(ws.plugins.net_jobs.len(), 1, "erlaubt: gleich ins Netz");
    // Aus- und wieder eingeschaltet am Host vorbei: das Ergebnis gehört zur alten Generation
    // und wird verworfen, die neue Instanz läuft ungestört.
    ws.plugins.host.set_enabled(FAELLE, false);
    ws.plugins.host.set_enabled(FAELLE, true);
    warten(&mut ws, &ctx);
    bild(&mut ws, &ctx, vec![]);
    assert_eq!(state(&ws, FAELLE), EntryState::Running);
    assert!(ws.plugins.banners.is_empty());

    // Über `Plugins::set_enabled` fällt ein laufender Auftrag gleich weg.
    ws.plugins.host.run_command("planora.test-faelle.netz");
    bild(&mut ws, &ctx, vec![]);
    assert_eq!(ws.plugins.net_jobs.len(), 1);
    ws.plugins.set_enabled(FAELLE, false);
    assert!(ws.plugins.net_jobs.is_empty());
    bild(&mut ws, &ctx, vec![]);
    assert!(
        !ws.config.plugins[FAELLE].enabled,
        "ausgeschaltet gespeichert"
    );
    assert!(!gespeichert(&ws).unwrap().enabled);
}

#[test]
fn failure_banner_and_restart() {
    let (mut ws, ctx) = arbeitsbereich("ausfall", &["test-faelle"], Config::default());
    let failed = "Das Plug-in Test-Fälle wurde wegen eines Fehlers angehalten.";
    for runde in 1..=2 {
        ws.plugins.host.run_command("planora.test-faelle.panik");
        let mut seen = bild(&mut ws, &ctx, vec![]);
        if find(&seen, failed).is_none() {
            seen = bild(&mut ws, &ctx, vec![]);
        }
        assert_eq!(ws.plugins.banners.len(), 1, "Runde {runde}");
        assert!(find(&seen, failed).is_some(), "{}", join(&seen));
        assert!(find(&seen, "Schließen").is_some());
        klick(&mut ws, &ctx, &seen, "Neu starten");
        assert!(ws.plugins.banners.is_empty(), "Leiste weg nach Neustart");
        warten(&mut ws, &ctx);
        assert_eq!(state(&ws, FAELLE), EntryState::Running, "Runde {runde}");
    }
    // Der dritte Ausfall: bleibt aus, kein „Neu starten“.
    ws.plugins.host.run_command("planora.test-faelle.panik");
    let mut seen = bild(&mut ws, &ctx, vec![]);
    let stopped = "Das Plug-in Test-Fälle bleibt bis zum Neustart des Programms aus.";
    if find(&seen, stopped).is_none() {
        seen = bild(&mut ws, &ctx, vec![]);
    }
    assert_eq!(state(&ws, FAELLE), EntryState::Stopped);
    assert!(find(&seen, stopped).is_some(), "{}", join(&seen));
    assert!(find(&seen, "Neu starten").is_none());
    klick(&mut ws, &ctx, &seen, "Schließen");
    assert!(ws.plugins.banners.is_empty());
}

#[test]
fn addins_menu_opens_settings_section() {
    let (mut ws, ctx) = arbeitsbereich("einstellungen", &[], Config::default());
    let mut shell = ShellState::default();
    let mut got = MenuRequest::None;
    let seen = menue_bild(&mut ws, &ctx, &mut shell, vec![], &mut got);
    let seen = menue_klick(&mut ws, &ctx, &mut shell, &seen, "Datei", &mut got);
    let seen = menue_klick(&mut ws, &ctx, &mut shell, &seen, "Zusatzmodule", &mut got);
    // Ohne Plug-ins nur der Eintrag „Verwalten …“.
    menue_klick(&mut ws, &ctx, &mut shell, &seen, "Verwalten …", &mut got);
    assert_eq!(got, MenuRequest::Settings(Some(SettingsSection::Plugins)));

    let mut app = PlanoraApp::for_tests(ws);
    app.apply_menu_request(&ctx, got);
    assert!(app.settings_window().is_open());
    assert_eq!(
        app.settings_window().scroll_target(),
        Some(SettingsSection::Plugins)
    );
    // Datei → „Einstellungen …“ öffnet ohne Abschnitt.
    app.settings_window_mut().close();
    app.apply_menu_request(&ctx, MenuRequest::Settings(None));
    assert!(app.settings_window().is_open());
    assert_eq!(app.settings_window().scroll_target(), None);
}

#[test]
fn persist_writes_plugins_only_when_pending() {
    let (mut ws, _ctx) = arbeitsbereich("speichern", &[], Config::default());
    let eintrag = PluginConfig {
        enabled: false,
        rights: BTreeMap::new(),
    };
    ws.config
        .plugins
        .insert(FAELLE.to_string(), eintrag.clone());
    // Ohne Vormerkung schreibt der bestehende Weg nur seine Felder.
    assert!(ws.try_persist_config());
    assert_eq!(gespeichert(&ws), None);
    ws.plugins_pending = true;
    assert!(ws.try_persist_config());
    assert!(!ws.plugins_pending);
    assert_eq!(gespeichert(&ws), Some(eintrag.clone()));
    // Ein anderes Feld zu speichern lässt die Plug-ins auf der Platte stehen.
    ws.set_theme(false);
    assert_eq!(gespeichert(&ws), Some(eintrag));
}

#[test]
fn manage_disabled_while_prompt_open() {
    let (mut ws, ctx) = arbeitsbereich("verwalten-gesperrt", &[], Config::default());
    ctx.all_styles_mut(|s| s.interaction.tooltip_delay = 0.0);
    ws.prompt = Some(Prompt::Notice("offen".to_string()));
    let mut shell = ShellState::default();
    let mut got = MenuRequest::None;
    let seen = menue_bild(&mut ws, &ctx, &mut shell, vec![], &mut got);
    let seen = menue_klick(&mut ws, &ctx, &mut shell, &seen, "Datei", &mut got);
    let seen = menue_klick(&mut ws, &ctx, &mut shell, &seen, "Zusatzmodule", &mut got);
    // Wie „Einstellungen“: der Grund beim Überfahren.
    let at = find(&seen, "Verwalten …").unwrap().rect.center();
    // Tooltips brauchen vergehende Zeit: Bilder im Abstand von 0,1 s.
    let mut hovered = Vec::new();
    for i in 0..10 {
        let raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1920.0, 1080.0))),
            events: vec![Event::PointerMoved(at)],
            time: Some(100.0 + f64::from(i) * 0.1),
            ..Default::default()
        };
        let out = ctx.run_ui(raw, |ui| {
            app::menu_bar(ui, &mut ws, &mut shell);
        });
        hovered = collect_seen(&out.shapes);
        out.drop_without_applying_deltas();
    }
    assert!(
        find(&hovered, "Zuerst die offene Rückfrage beantworten.").is_some(),
        "{}",
        join(&hovered)
    );
    menue_klick(&mut ws, &ctx, &mut shell, &hovered, "Verwalten …", &mut got);
    assert_eq!(
        got,
        MenuRequest::None,
        "bei offener Rückfrage nicht wählbar"
    );
}

#[test]
fn settings_request_leaves_window_alone_while_prompt_open() {
    let (ws, ctx) = arbeitsbereich("einstellungen-rueckfrage", &[], Config::default());
    let mut app = PlanoraApp::for_tests(ws);
    app.apply_menu_request(&ctx, MenuRequest::Settings(None));
    assert!(app.settings_window().is_open());
    app.settings_window_mut().start_choosing_plugin();
    app.ws_mut().prompt = Some(Prompt::Notice("offen".to_string()));
    // Ein schon offenes Fenster bleibt, wie es ist (keine neue Öffnung, kein Rollziel).
    app.apply_menu_request(&ctx, MenuRequest::Settings(Some(SettingsSection::Plugins)));
    assert!(app.settings_window().is_open());
    assert_eq!(
        app.settings_window().chooser_purpose(),
        Some(ChooserPurpose::Plugin),
        "die Auswahl bleibt offen"
    );
    assert_eq!(app.settings_window().scroll_target(), None);
    // Ein geschlossenes öffnet bei offener Rückfrage nicht.
    app.settings_window_mut().close();
    app.apply_menu_request(&ctx, MenuRequest::Settings(Some(SettingsSection::Plugins)));
    assert!(!app.settings_window().is_open());
    assert_eq!(app.settings_window().scroll_target(), None);
}
