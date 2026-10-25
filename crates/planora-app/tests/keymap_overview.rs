//! Fenster „Tastenkürzel“ (Übersicht): Menü Ansicht und Taste S öffnen es mit aktivem Suchfeld,
//! die Suche findet Aktion und Taste, solange das Suchfeld die Tastatur hat, löst kein Kürzel
//! aus, es zeigt die aktuelle Belegung und folgt den Regeln für offene Fenster.
//!
//! Mit echten Bildern der ganzen App (`PlanoraApp::for_tests`, `eframe::App::ui`) bei
//! 1920 × 1080; Bibliothek und Konfiguration nur im Temp-Ordner.

mod common;

use eframe::App as _;
use egui::{Event, Key, Modifiers, PointerButton, Pos2, Rect, Vec2, vec2};
use planora_app::app::{PlanoraApp, VIEW_SHORTCUTS_ENTRY, open_settings};
use planora_app::icons::{self, Icon};
use planora_app::keymap::overview::*;
use planora_app::keymap::{Action, Binding, KeyChord, Keymap, settings_ui, shortcut_hint};
use planora_app::workspace::Workspace;
use planora_app::{fonts, theme};
use planora_feature::{SketchPlane, StandardPlane};
use planora_math::DVec3;
use planora_project::config::Config;
use planora_project::library::Library;

const SIZE: Vec2 = vec2(1920.0, 1080.0);
const NONE: Modifiers = Modifiers::NONE;
const CTRL: Modifiers = Modifiers {
    alt: false,
    ctrl: true,
    shift: false,
    mac_cmd: false,
    command: true,
};

// ---- Hilfen ---------------------------------------------------------------------------------

fn ws() -> Workspace {
    let root = common::temp_root("planora-keymap-overview");
    let mut w = Workspace::new(Some(Library::open(&root).unwrap()), Config::default());
    let tmp = common::temp_root("planora-keymap-overview-tmp");
    std::fs::create_dir_all(&tmp).unwrap();
    w.config_path = tmp.join("config.json");
    w
}

fn key_event(k: Key, modifiers: Modifiers) -> Event {
    Event::Key {
        key: k,
        physical_key: Some(k),
        pressed: true,
        repeat: false,
        modifiers,
    }
}

fn release_event(k: Key, modifiers: Modifiers) -> Event {
    Event::Key {
        key: k,
        physical_key: Some(k),
        pressed: false,
        repeat: false,
        modifiers,
    }
}

fn button(p: Pos2, pressed: bool) -> Event {
    Event::PointerButton {
        pos: p,
        button: PointerButton::Primary,
        pressed,
        modifiers: NONE,
    }
}

struct Seen {
    text: String,
    rect: Rect,
}

fn walk(shape: &egui::epaint::Shape, out: &mut Vec<Seen>) {
    match shape {
        egui::epaint::Shape::Vec(v) => v.iter().for_each(|s| walk(s, out)),
        egui::epaint::Shape::Text(t) => out.push(Seen {
            text: t.galley.text().to_string(),
            rect: Rect::from_min_size(t.pos, t.galley.size()),
        }),
        _ => {}
    }
}

struct Rig {
    ctx: egui::Context,
    app: PlanoraApp,
    time: f64,
    seen: Vec<Seen>,
}

impl Rig {
    fn new(w: Workspace) -> Rig {
        let ctx = egui::Context::default();
        ctx.set_fonts(fonts::definitions());
        theme::apply(&ctx, true);
        PlanoraApp::setup_context(&ctx);
        let mut rig = Rig {
            ctx,
            app: PlanoraApp::for_tests(w),
            time: 0.0,
            seen: Vec::new(),
        };
        rig.settle();
        rig
    }

    fn ws(&mut self) -> &mut Workspace {
        self.app.ws_mut()
    }

    fn ov(&mut self) -> &mut OverviewState {
        self.app.overview_mut()
    }

    fn is_open(&mut self) -> bool {
        self.ov().open
    }

    fn frame_with(&mut self, events: Vec<Event>, modifiers: Modifiers) {
        self.time += 1.0 / 60.0;
        let mut all = vec![Event::ModifiersChanged(modifiers)];
        all.extend(events);
        let raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, SIZE)),
            time: Some(self.time),
            events: all,
            ..Default::default()
        };
        let mut f = eframe::Frame::_new_kittest();
        let app = &mut self.app;
        let out = self.ctx.run_ui(raw, |ui| app.ui(ui, &mut f));
        let mut seen = Vec::new();
        for c in &out.shapes {
            walk(&c.shape, &mut seen);
        }
        out.drop_without_applying_deltas();
        self.seen = seen;
    }

    fn frame(&mut self, events: Vec<Event>) {
        self.frame_with(events, NONE);
    }

    fn settle(&mut self) {
        for _ in 0..3 {
            self.frame(vec![]);
        }
    }

    /// Taste drücken (mit dem getippten Text wie unter Windows), im nächsten Bild loslassen.
    fn press(&mut self, k: Key, m: Modifiers) {
        let mut events = vec![key_event(k, m)];
        if m == NONE
            && let Some(t) = typed(k)
        {
            events.push(Event::Text(t.into()));
        }
        self.frame_with(events, m);
        self.frame_with(vec![release_event(k, m)], NONE);
        self.settle();
    }

    fn texts(&self) -> Vec<&str> {
        self.seen.iter().map(|s| s.text.as_str()).collect()
    }

    fn sees(&self, text: &str) -> bool {
        self.seen.iter().any(|s| s.text == text)
    }

    /// Mitte des untersten sichtbaren Texts `text` (Fußzeile vor gleichnamigen Zeilen darüber).
    fn find_lowest(&self, text: &str) -> Pos2 {
        self.seen
            .iter()
            .filter(|s| s.text == text)
            .max_by(|a, b| a.rect.top().partial_cmp(&b.rect.top()).unwrap())
            .unwrap_or_else(|| panic!("„{text}“ nicht zu sehen: {:?}", self.texts()))
            .rect
            .center()
    }

    fn click_at(&mut self, p: Pos2) {
        self.frame(vec![Event::PointerMoved(p)]);
        self.frame(vec![button(p, true)]);
        self.frame(vec![button(p, false)]);
        self.settle();
    }

    fn click(&mut self, text: &str) {
        self.settle();
        let p = self.find_lowest(text);
        self.click_at(p);
    }

    /// Menü „Ansicht“ (der oberste Text „Ansicht“) → „Tastenkürzel …“.
    fn open_from_menu(&mut self) {
        self.open_view_menu();
        self.click(VIEW_SHORTCUTS_ENTRY);
    }

    fn open_view_menu(&mut self) {
        self.settle();
        let menu = self
            .seen
            .iter()
            .filter(|s| s.text == "Ansicht")
            .min_by(|a, b| a.rect.top().partial_cmp(&b.rect.top()).unwrap())
            .unwrap_or_else(|| panic!("Menü „Ansicht“ fehlt: {:?}", self.texts()))
            .rect
            .center();
        self.click_at(menu);
    }

    fn search_has_focus(&mut self) -> bool {
        let id = search_id();
        self.ctx.memory(|m| m.has_focus(id))
    }

    fn filter(&mut self) -> String {
        self.ov().filter.clone()
    }

    fn set_filter(&mut self, f: &str) {
        self.ov().filter = f.into();
        self.settle();
    }

    fn info(&mut self) -> Option<String> {
        self.ws().active_tab().state.info.clone()
    }
}

/// Der Text, den Windows beim Drücken der Buchstabentaste schickt.
fn typed(k: Key) -> Option<&'static str> {
    match k {
        Key::S => Some("s"),
        Key::E => Some("e"),
        Key::V => Some("v"),
        _ => None,
    }
}

fn rig() -> Rig {
    Rig::new(ws())
}

fn sketch_rig() -> Rig {
    let mut w = ws();
    let s = &mut w.active_tab_mut().state;
    let sk = s
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    s.open_sketch(sk).unwrap();
    w.observe_history(false);
    Rig::new(w)
}

fn chord(key: Key, ctrl: bool) -> Binding {
    Binding::Key(KeyChord {
        key,
        ctrl,
        shift: false,
        alt: false,
    })
}

// ---- Tests ----------------------------------------------------------------------------------

#[test]
fn view_menu_entry_opens_the_overview() {
    let mut r = rig();
    assert!(!r.is_open());
    assert!(!r.sees(TITLE));
    // Der Menüeintrag nennt die Taste aus der Belegung.
    assert_eq!(shortcut_hint(&r.ws().keymap, Action::ShowShortcuts), "S");
    r.open_view_menu();
    assert!(r.sees(VIEW_SHORTCUTS_ENTRY), "{:?}", r.texts());
    assert!(r.sees("S"), "Taste im Menü: {:?}", r.texts());
    r.click(VIEW_SHORTCUTS_ENTRY);
    assert!(r.is_open());
    assert!(r.sees(TITLE), "{:?}", r.texts());
    assert!(r.search_has_focus(), "Suchfeld aktiv nach dem Menü");
    // Nach Bereich sortiert: Überall, Modell, Skizze, Zeichnung (je mit Überschrift).
    let km = r.ws().keymap.clone();
    let all = shown_actions(&km, "");
    assert_eq!(all.len(), Action::ALL.len());
    assert!(all.windows(2).all(|w| w[0].area() <= w[1].area()));
    for t in [
        "Überall",
        "Speichern",
        "Strg+S",
        "Modell",
        "Skizze extrudieren",
        "E",
    ] {
        assert!(r.sees(t), "„{t}“ fehlt: {:?}", r.texts());
    }
    // Der unterste Text (das Menü „Zeichnung“ steht oben in der Leiste).
    let top = |r: &Rig, t: &str| r.find_lowest(t).y;
    assert!(top(&r, "Überall") < top(&r, "Speichern"));
    assert!(top(&r, "Speichern") < top(&r, "Modell"));
    assert!(top(&r, "Modell") < top(&r, "Skizze extrudieren"));
    r.set_filter("wiederholen");
    for t in ["Überall", "Modell", "Skizze"] {
        assert!(r.sees(t), "„{t}“ fehlt: {:?}", r.texts());
    }
    assert!(top(&r, "Wiederholen") < top(&r, "Letzten Befehl wiederholen"));
    assert!(top(&r, "Letzten Befehl wiederholen") < top(&r, "Skizze"));
    assert!(top(&r, "Skizze") < top(&r, "Letztes Skizzenwerkzeug wiederholen"));
    r.set_filter("Maß");
    assert!(r.sees("D"), "{:?}", r.texts());
    assert!(top(&r, "Zeichnung") < top(&r, "Maß"));
    assert!(r.sees(TO_SETTINGS));
    r.click(TO_SETTINGS);
    assert!(
        !r.is_open(),
        "„In den Einstellungen ändern“ schließt die Übersicht"
    );
    assert!(
        r.app.windows_mut().2.is_open(),
        "… und öffnet die Einstellungen"
    );
}

#[test]
fn s_opens_it_with_the_search_focused() {
    let mut r = rig();
    r.press(Key::S, NONE);
    assert!(r.is_open(), "S öffnet die Übersicht");
    assert!(r.search_has_focus());
    assert_eq!(r.filter(), "", "das „s“ der Taste landet nicht im Suchfeld");
    // Esc schließt (auch mit Text im Suchfeld).
    r.frame(vec![Event::Text("x".into())]);
    r.settle();
    assert_eq!(r.filter(), "x");
    r.press(Key::Escape, NONE);
    assert!(!r.is_open(), "Esc schließt");
    assert!(!r.search_has_focus());

    // In der Skizze ebenso (eigene Aktion im Bereich Skizze).
    let mut r = sketch_rig();
    assert_eq!(
        shortcut_hint(&r.ws().keymap, Action::SketchShowShortcuts),
        "S"
    );
    r.press(Key::S, NONE);
    assert!(r.is_open(), "S öffnet die Übersicht in der Skizze");
    assert!(r.search_has_focus());
    // Esc schließt nur die Übersicht, die Skizze bleibt offen.
    r.press(Key::Escape, NONE);
    assert!(!r.is_open());
    assert!(r.ws().active_tab().state.sketch_session.is_some());

    // Nicht, solange ein anderes Textfeld die Tastatur hat: dann gehört S dem Feld.
    // Hier das Suchfeld der Einstellungen (dort ist S ohnehin gesperrt, weil ein Fenster offen
    // ist; das Feld bekommt das „s“).
    let mut r = rig();
    assert_eq!(r.ws().keymap, Keymap::defaults());
    r.app.windows_mut().2.open(false);
    r.settle();
    let p = r.find_lowest(settings_ui::SEARCH_HINT);
    r.click_at(p);
    r.press(Key::S, NONE);
    assert!(!r.is_open(), "S tippt ins Suchfeld der Einstellungen");
    assert_eq!(r.app.windows_mut().2.keys_mut().filter, "s");
}

#[test]
fn search_finds_by_action_and_by_key_text_and_shows_empty_text() {
    let mut r = rig();
    r.press(Key::S, NONE);
    r.set_filter("Extrudieren");
    assert!(r.sees("Skizze extrudieren"), "{:?}", r.texts());
    assert!(!r.sees("Speichern"));
    assert!(!r.sees(NOTHING_FOUND));
    r.set_filter("Strg+S");
    assert!(r.sees("Speichern"), "{:?}", r.texts());
    assert!(!r.sees("Skizze extrudieren"));
    assert!(!r.sees("Rückgängig"));
    // Nach der Taste „E“: auch die Aktion mit E findet sich.
    r.set_filter("E");
    assert!(r.sees("Skizze extrudieren"));
    r.set_filter("gibt es nicht");
    assert!(r.sees(NOTHING_FOUND), "{:?}", r.texts());
    assert!(!r.sees("Speichern"));
}

#[test]
fn typing_in_search_triggers_no_shortcuts() {
    let mut r = rig();
    r.press(Key::S, NONE);
    assert!(r.search_has_focus());
    let tabs = r.ws().tabs.len();
    // Strg+N (überall) und V (Modell) bei Fokus im Suchfeld: nichts geschieht, V wird getippt.
    r.press(Key::N, CTRL);
    r.press(Key::V, NONE);
    r.press(Key::E, NONE);
    assert_eq!(r.ws().tabs.len(), tabs, "Strg+N löst nicht aus");
    assert_eq!(r.info(), None, "V löst nicht aus");
    assert_eq!(r.filter(), "ve");
    assert!(r.is_open());
    // S tippt ebenfalls ins Feld, statt etwas zu öffnen.
    r.press(Key::S, NONE);
    assert_eq!(r.filter(), "ves");
}

#[test]
fn shows_current_not_default_keys_after_a_change() {
    let mut r = rig();
    let mut km = r.ws().keymap.clone();
    km.set(Action::Save, 0, chord(Key::K, true));
    km.remove(Action::Fillet, 0);
    r.ws().keymap = km;
    r.press(Key::S, NONE);
    r.set_filter("Speichern");
    assert!(r.sees("Strg+K"), "{:?}", r.texts());
    assert!(!r.sees("Strg+S"));
    r.set_filter("Verrunden");
    assert!(r.sees(UNBOUND), "ohne Belegung „–“: {:?}", r.texts());
    // Eine Änderung bei offenem Fenster zeigt es im nächsten Bild.
    let mut km = r.ws().keymap.clone();
    km.set(Action::Fillet, 0, chord(Key::G, false));
    r.ws().keymap = km;
    r.settle();
    assert!(r.sees("G"), "{:?}", r.texts());
    assert!(!r.sees(UNBOUND));
    // Eine geänderte Taste S öffnet nicht mehr, die neue schon.
    let mut km = r.ws().keymap.clone();
    km.set(Action::ShowShortcuts, 0, chord(Key::J, false));
    r.ws().keymap = km;
    r.press(Key::Escape, NONE);
    assert!(!r.is_open());
    r.press(Key::S, NONE);
    assert!(!r.is_open(), "S ist nicht mehr belegt");
    r.press(Key::J, NONE);
    assert!(r.is_open(), "J öffnet");
}

#[test]
fn view_menu_entry_shows_the_new_key_after_rebinding() {
    let mut r = rig();
    let mut km = r.ws().keymap.clone();
    km.set(Action::ShowShortcuts, 0, chord(Key::J, false));
    r.ws().keymap = km;
    r.open_view_menu();
    assert!(r.sees(VIEW_SHORTCUTS_ENTRY), "{:?}", r.texts());
    assert!(r.sees("J"), "neue Taste im Menü: {:?}", r.texts());
    assert!(!r.sees("S"), "alte Taste weg: {:?}", r.texts());
}

#[test]
fn overview_does_not_open_over_a_prompt() {
    let mut r = rig();
    r.ws().prompt = Some(planora_app::workspace::Prompt::Notice("Hinweis".into()));
    r.settle();
    {
        let (ws, projects, settings) = r.app.windows_mut();
        let mut overview = OverviewState::default();
        planora_app::app::open_overview(ws, projects, settings, &mut overview);
        assert!(!overview.open, "bei offener Rückfrage bleibt sie zu");
    }
}

#[test]
fn overview_follows_window_exclusion_rules() {
    let mut r = rig();
    // Offen, ohne Fokus im Suchfeld: Modelltasten und Rückgängig sind gesperrt wie bei jedem
    // Fenster, S noch einmal tut nichts.
    r.ov().open = true;
    r.settle();
    assert!(r.sees(TITLE));
    assert!(!r.search_has_focus());
    r.press(Key::V, NONE);
    assert_eq!(r.info(), None, "V gesperrt bei offener Übersicht");
    r.press(Key::S, NONE);
    assert!(r.is_open());
    assert_eq!(r.filter(), "", "S noch einmal tut nichts");
    // Kürzel für überall gelten weiter (wie bei den anderen Fenstern): Strg+O öffnet Projekte,
    // die Übersicht schließt dabei.
    r.press(Key::O, CTRL);
    assert!(r.app.windows_mut().1.is_open(), "Projekte offen");
    assert!(!r.is_open(), "Projekte schließt die Übersicht");
    // Die Übersicht schließt Projekte.
    r.open_from_menu();
    assert!(r.is_open());
    assert!(
        !r.app.windows_mut().1.is_open(),
        "Übersicht schließt Projekte"
    );
    // Einstellungen schließen die Übersicht, die Übersicht die Einstellungen.
    {
        let (ws, projects, settings) = r.app.windows_mut();
        open_settings(ws, projects, settings, false);
    }
    r.settle();
    assert!(!r.is_open(), "Einstellungen schließen die Übersicht");
    r.open_from_menu();
    assert!(r.is_open());
    assert!(
        !r.app.windows_mut().2.is_open(),
        "Übersicht schließt die Einstellungen"
    );
    // Ein Dateifenster des Tabs schließt sie (wie Projekte und Einstellungen).
    r.ws()
        .active_tab_mut()
        .state
        .create_box(DVec3::splat(10.0))
        .unwrap();
    r.settle();
    planora_app::commands::run_action(r.ws(), Action::Export);
    r.settle();
    assert!(r.ws().active_tab().export_dialog.is_some());
    assert!(!r.is_open(), "Exportieren schließt die Übersicht");
    // Die Übersicht schließt ein Dateifenster.
    r.open_from_menu();
    assert!(r.is_open());
    assert!(r.ws().active_tab().export_dialog.is_none());
    // „Schließen“ in der Fußzeile (die unterste; die Zeile „Schließen“ steht darüber) und ✕.
    r.set_filter("");
    r.click("Schließen");
    assert!(!r.is_open(), "Schließen in der Fußzeile");
    r.press(Key::S, NONE);
    assert!(r.is_open());
    r.click(&icons::glyph(Icon::Close).to_string());
    assert!(!r.is_open(), "✕ schließt");
}
