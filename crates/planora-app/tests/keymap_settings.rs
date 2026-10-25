//! Einstellungen → Abschnitt „Tastenkürzel“: Tabelle mit Suche, Ändern durch Erfassen einer Taste
//! oder Mausgeste (Esc bricht ab, die erfasste Taste löst nichts aus), Doppelbelegung mit
//! „Tauschen“ und „Abbrechen“, Entfernen, Zurücksetzen, Speicherfehler und Hinweise vom Laden.
//!
//! Mit echten Bildern der ganzen App (`PlanoraApp::for_tests`, `eframe::App::ui`) bei
//! 1920 × 1080; Bibliothek und Konfiguration nur im Temp-Ordner.

mod common;

use eframe::App as _;
use egui::{Event, Key, Modifiers, PointerButton, Pos2, Rect, Vec2, pos2, vec2};
use planora_app::app::PlanoraApp;
use planora_app::keymap::settings_ui::*;
use planora_app::keymap::text::binding_text;
use planora_app::keymap::{Action, Area, Binding, Gesture, KeyChord, Keymap, MouseButton, search};
use planora_app::settings_window::RootMessage;
use planora_app::workspace::Workspace;
use planora_app::{fonts, theme};
use planora_project::config::Config;
use planora_project::library::Library;
use std::collections::BTreeMap;
use std::path::PathBuf;

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

fn ws_with(config: Config) -> (Workspace, PathBuf) {
    let root = common::temp_root("planora-keymap-settings");
    let mut w = Workspace::new(Some(Library::open(&root).unwrap()), config);
    let tmp = common::temp_root("planora-keymap-settings-tmp");
    std::fs::create_dir_all(&tmp).unwrap();
    w.config_path = tmp.join("config.json");
    (w, tmp)
}

fn chord(key: Key, ctrl: bool, shift: bool, alt: bool) -> Binding {
    Binding::Key(KeyChord {
        key,
        ctrl,
        shift,
        alt,
    })
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

/// Wiederholung der gehaltenen Taste (Autorepeat des Systems).
fn repeat_event(k: Key, modifiers: Modifiers) -> Event {
    Event::Key {
        key: k,
        physical_key: Some(k),
        pressed: true,
        repeat: true,
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

fn button(p: Pos2, b: PointerButton, pressed: bool, modifiers: Modifiers) -> Event {
    Event::PointerButton {
        pos: p,
        button: b,
        pressed,
        modifiers,
    }
}

struct Seen {
    text: String,
    rect: Rect,
    /// Ausschnitt, in dem der Text gezeichnet wird (rollende Tabelle).
    clip: Rect,
}

impl Seen {
    /// Ganz zu sehen (nicht vom Ausschnitt abgeschnitten)?
    fn visible(&self) -> bool {
        self.clip.contains_rect(self.rect)
    }
}

fn walk(shape: &egui::epaint::Shape, clip: Rect, out: &mut Vec<Seen>) {
    match shape {
        egui::epaint::Shape::Vec(v) => v.iter().for_each(|s| walk(s, clip, out)),
        egui::epaint::Shape::Text(t) => out.push(Seen {
            text: t.galley.text().to_string(),
            rect: Rect::from_min_size(t.pos, t.galley.size()),
            clip,
        }),
        _ => {}
    }
}

struct Rig {
    ctx: egui::Context,
    app: PlanoraApp,
    time: f64,
    /// Blieben nach dem Bild Tastenereignisse übrig (nicht verbraucht)?
    left_over: bool,
    /// Texte des letzten Bilds.
    seen: Vec<Seen>,
    /// Größe des Programmfensters.
    size: Vec2,
}

impl Rig {
    fn new(w: Workspace) -> Rig {
        Rig::sized(w, SIZE)
    }

    fn sized(w: Workspace, size: Vec2) -> Rig {
        let ctx = egui::Context::default();
        ctx.set_fonts(fonts::definitions());
        theme::apply(&ctx, true);
        PlanoraApp::setup_context(&ctx);
        let mut rig = Rig {
            ctx,
            app: PlanoraApp::for_tests(w),
            time: 0.0,
            left_over: false,
            seen: Vec::new(),
            size,
        };
        rig.settle();
        rig.app.windows_mut().2.open(false);
        rig.settle();
        rig
    }

    fn ws(&mut self) -> &mut Workspace {
        self.app.ws_mut()
    }

    fn km(&mut self) -> Keymap {
        self.ws().keymap.clone()
    }

    fn state(&mut self) -> &mut CaptureState {
        self.app.windows_mut().2.keys_mut()
    }

    /// Ein Bild mit `events`; `modifiers` sind die gehaltenen Umschalttasten.
    fn frame_with(&mut self, events: Vec<Event>, modifiers: Modifiers) {
        self.time += 1.0 / 60.0;
        let mut all = vec![Event::ModifiersChanged(modifiers)];
        all.extend(events);
        let raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, self.size)),
            time: Some(self.time),
            events: all,
            ..Default::default()
        };
        let mut f = eframe::Frame::_new_kittest();
        let app = &mut self.app;
        let mut left_over = false;
        let out = self.ctx.run_ui(raw, |ui| {
            app.ui(ui, &mut f);
            left_over = ui.input(|i| {
                i.events
                    .iter()
                    .any(|e| matches!(e, Event::Key { pressed: true, .. }))
            });
        });
        self.left_over = left_over;
        let mut seen = Vec::new();
        for c in &out.shapes {
            walk(&c.shape, c.clip_rect, &mut seen);
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

    /// Taste drücken, im nächsten Bild loslassen.
    fn press(&mut self, k: Key, m: Modifiers) {
        self.frame_with(vec![key_event(k, m)], m);
        self.frame_with(vec![release_event(k, m)], NONE);
        self.settle();
    }

    fn texts(&self) -> Vec<&str> {
        self.seen.iter().map(|s| s.text.as_str()).collect()
    }

    fn sees(&self, text: &str) -> bool {
        self.seen.iter().any(|s| s.text == text)
    }

    /// Zeigt nur die Zeilen, die zu `filter` passen.
    fn filter(&mut self, filter: &str) {
        self.state().filter = filter.into();
        self.settle();
    }

    /// Mitte des `n`-ten sichtbaren Texts `text` (von oben nach unten, links nach rechts).
    fn find(&self, text: &str, n: usize) -> Pos2 {
        let mut all: Vec<&Seen> = self.seen.iter().filter(|s| s.text == text).collect();
        all.sort_by(|a, b| {
            (a.rect.top().round(), a.rect.left())
                .partial_cmp(&(b.rect.top().round(), b.rect.left()))
                .unwrap()
        });
        all.get(n)
            .unwrap_or_else(|| panic!("„{text}“ Nr. {n} nicht zu sehen: {:?}", self.texts()))
            .rect
            .center()
    }

    fn click_at(&mut self, p: Pos2) {
        self.frame(vec![Event::PointerMoved(p)]);
        self.frame(vec![button(p, PointerButton::Primary, true, NONE)]);
        self.frame(vec![button(p, PointerButton::Primary, false, NONE)]);
        self.settle();
    }

    fn click(&mut self, text: &str, n: usize) {
        self.settle();
        let p = self.find(text, n);
        self.click_at(p);
    }

    fn tabs(&mut self) -> usize {
        self.ws().tabs.len()
    }

    fn saved_keys(&mut self) -> BTreeMap<String, Vec<String>> {
        let path = self.ws().config_path.clone();
        Config::load_from(&path).keys
    }
}

fn rig() -> Rig {
    Rig::new(ws_with(Config::default()).0)
}

// ---- Tests ----------------------------------------------------------------------------------

#[test]
fn section_lists_every_action_with_current_keys() {
    let mut r = rig();
    assert!(r.sees(SECTION), "{:?}", r.texts());
    assert!(r.sees(RESET_ALL));
    assert!(r.sees(COLUMN_ACTION) && r.sees(COLUMN_AREA) && r.sees(COLUMN_BINDINGS));
    // „Schließen“ bleibt bei 1920 × 1080 zu sehen, das Fenster passt.
    assert!(r.sees(planora_app::dialog::CLOSE_LABEL));
    let window = r.app.windows_mut().2.last_rect.unwrap();
    assert!(
        Rect::from_min_size(Pos2::ZERO, SIZE).contains_rect(window),
        "{window:?}"
    );
    // Ohne Suche zeigt die Tabelle die ersten Aktionen in Tabellenreihenfolge.
    let new = r.find(Action::New.label(), 0);
    let save = r.find(Action::Save.label(), 0);
    assert!(new.y < save.y);
    // Jede Aktion mit Bereich und allen Belegungen (über die Suche nach ihrem Namen sichtbar).
    for &a in Action::ALL {
        r.filter(a.label());
        assert!(r.sees(a.label()), "{a:?}: {:?}", r.texts());
        assert!(r.sees(a.area().label()), "{a:?}");
        for b in Keymap::defaults().bindings(a) {
            assert!(r.sees(&binding_text(b)), "{a:?}: {}", binding_text(b));
        }
        let changes = r.seen.iter().filter(|s| s.text == CHANGE).count();
        assert!(changes >= a.default_bindings().len(), "{a:?}");
        assert!(r.sees(ADD) && r.sees(RESET), "{a:?}");
    }
}

#[test]
fn search_filters_by_name_area_and_key_text() {
    let km = Keymap::defaults();
    // Name, ohne Groß- und Kleinschreibung.
    assert!(search::matches(Action::ExtrudeSketch, &km, "extrud"));
    assert!(!search::matches(Action::Save, &km, "extrud"));
    // Bereich.
    assert!(search::matches(Action::SketchLine, &km, "SKIZZE"));
    assert!(!search::matches(Action::Fillet, &km, "zeichnung"));
    // Tastentext.
    assert!(search::matches(Action::Save, &km, "strg+s"));
    assert!(!search::matches(Action::Close, &km, "strg+s"));
    assert!(search::matches(Action::ViewPan, &km, "mittlere"));
    // Mehrere Wörter: alle müssen passen; leer passt immer.
    assert!(search::matches(Action::SketchCircle, &km, "skizze c"));
    assert!(!search::matches(Action::SketchCircle, &km, "skizze zoomen"));
    assert!(search::matches(Action::Undo, &km, "  "));
    // Nach einer Änderung zählt die neue Taste.
    let mut changed = km.clone();
    changed.swap(Action::Save, 0, chord(Key::F5, false, false, false));
    assert!(search::matches(Action::Save, &changed, "f5"));
    assert!(!search::matches(Action::Save, &changed, "strg+s"));

    // In der Oberfläche.
    let mut r = rig();
    r.filter("strg+s");
    assert!(r.sees(Action::Save.label()));
    assert!(!r.sees(Action::Undo.label()));
    r.filter("Zeichnung");
    assert!(r.sees(Action::DrawingDimension.label()) && r.sees(Action::DrawingBalloon.label()));
    assert!(!r.sees(Action::Save.label()));
    r.filter("gibt es nicht");
    assert!(r.sees(NOTHING_FOUND));
}

#[test]
fn capturing_a_key_sets_it_and_frees_the_old_one() {
    let mut r = rig();
    r.filter(Action::Save.label());
    r.click(CHANGE, 0);
    assert!(r.sees(PRESS_KEY), "{:?}", r.texts());
    assert_eq!(r.state().capturing, Some((Action::Save, 0)));
    // Eine Umschalttaste allein tut nichts.
    r.press(Key::ShiftLeft, Modifiers::SHIFT);
    r.press(Key::ControlLeft, CTRL);
    assert!(r.sees(PRESS_KEY));
    // Die nächste Taste mit den gehaltenen Umschalttasten wird die Belegung.
    let alt_shift = Modifiers {
        alt: true,
        shift: true,
        ..NONE
    };
    r.press(Key::K, alt_shift);
    assert!(!r.sees(PRESS_KEY));
    let new = chord(Key::K, false, true, true);
    assert_eq!(r.km().bindings(Action::Save), &[new]);
    assert!(r.sees("Umschalt+Alt+K"), "{:?}", r.texts());
    // Strg+S ist frei, gespeichert ist nur die Abweichung.
    let old = chord(Key::S, true, false, false);
    assert_eq!(r.km().conflict_for(Action::Undo, &old), None);
    assert_eq!(
        r.saved_keys(),
        BTreeMap::from([("datei.speichern".into(), vec!["Umschalt+Alt+K".into()])])
    );
    // Hinzufügen hängt eine zweite Belegung an.
    r.click(ADD, 0);
    assert!(r.sees(PRESS_KEY));
    r.press(Key::F5, NONE);
    assert_eq!(
        r.km().bindings(Action::Save),
        &[new, chord(Key::F5, false, false, false)]
    );
    assert_eq!(
        r.km().action_for_key(
            Area::Model,
            &KeyChord {
                key: Key::F5,
                ctrl: false,
                shift: false,
                alt: false
            }
        ),
        Some(Action::Save)
    );
}

#[test]
fn capturing_does_not_trigger_the_captured_key() {
    let mut r = rig();
    let tabs = r.tabs();
    r.filter(Action::Save.label());
    r.click(CHANGE, 0);
    assert!(viewport_typing(&r));
    // Strg+N gehört „Neues Dokument“ (Überall, wirkt auch bei offenem Fenster): beim Erfassen
    // öffnet es keinen Tab, sondern wird als Doppelbelegung gemeldet.
    r.frame_with(vec![key_event(Key::N, CTRL)], CTRL);
    assert!(!r.left_over, "die Taste ist verbraucht");
    r.frame_with(vec![release_event(Key::N, CTRL)], NONE);
    r.settle();
    assert_eq!(r.tabs(), tabs);
    assert!(r.sees(&conflict_text(
        &chord(Key::N, true, false, false),
        Action::New
    )));
    // Kein Erfassen mehr: die Sperre ist weg, Strg+N wirkt wieder.
    assert!(!viewport_typing(&r));
    r.click(CANCEL, 0);
    r.press(Key::N, CTRL);
    assert_eq!(r.tabs(), tabs + 1);
}

fn viewport_typing(r: &Rig) -> bool {
    planora_app::viewport::typing(&r.ctx)
}

#[test]
fn esc_cancels_capturing() {
    let mut r = rig();
    r.filter(Action::Save.label());
    r.click(CHANGE, 0);
    assert!(r.sees(PRESS_KEY));
    r.frame_with(vec![key_event(Key::Escape, NONE)], NONE);
    assert!(!r.left_over);
    r.frame(vec![release_event(Key::Escape, NONE)]);
    r.settle();
    assert!(!r.sees(PRESS_KEY));
    assert_eq!(r.state().capturing, None);
    assert_eq!(r.km(), Keymap::defaults());
    assert!(
        r.app.windows_mut().2.is_open(),
        "Esc schließt das Fenster nicht"
    );
    assert!(!viewport_typing(&r));
    // Auch „Abbrechen“ neben der Aufforderung bricht ab.
    r.click(CHANGE, 0);
    r.click(CANCEL, 0);
    assert!(!r.sees(PRESS_KEY));
    assert_eq!(r.km(), Keymap::defaults());
}

#[test]
fn conflict_shows_swap_or_cancel() {
    let mut r = rig();
    r.filter(Action::Save.label());
    r.click(CHANGE, 0);
    r.press(Key::N, CTRL);
    let n = chord(Key::N, true, false, false);
    assert!(r.sees(&conflict_text(&n, Action::New)), "{:?}", r.texts());
    assert_eq!(
        conflict_text(&n, Action::New),
        "Strg+N ist schon für „Neues Dokument“ belegt."
    );
    assert!(r.sees(SWAP) && r.sees(CANCEL));
    // Noch nichts geändert.
    assert_eq!(r.km(), Keymap::defaults());
    r.click(CANCEL, 0);
    assert!(!r.sees(SWAP));
    assert_eq!(r.km(), Keymap::defaults());
    assert_eq!(r.state().pending_conflict, None);
    // Auch zwischen Modell und „Überall“ (Festlegung 6): Tauschen wird angeboten.
    r.filter(Action::Fillet.label());
    r.click(CHANGE, 0);
    r.press(Key::S, CTRL);
    assert!(r.sees(&conflict_text(
        &chord(Key::S, true, false, false),
        Action::Save
    )));
    assert!(r.sees(SWAP));
}

#[test]
fn swap_moves_the_key() {
    let mut r = rig();
    r.filter(Action::Save.label());
    r.click(CHANGE, 0);
    r.press(Key::N, CTRL);
    r.click(SWAP, 0);
    let n = chord(Key::N, true, false, false);
    assert_eq!(r.km().bindings(Action::Save), &[n]);
    assert!(r.km().bindings(Action::New).is_empty());
    assert!(!r.sees(SWAP));
    assert_eq!(
        r.saved_keys(),
        BTreeMap::from([
            ("datei.neu".into(), vec![]),
            ("datei.speichern".into(), vec!["Strg+N".into()]),
        ])
    );
    // „Überall“ gegen Modell: Verrunden bekommt Strg+S, Speichern verliert es nicht (es hat es
    // nicht mehr), Rückgängig gibt Strg+Z ab.
    r.filter(Action::Fillet.label());
    r.click(CHANGE, 0);
    r.press(Key::Z, CTRL);
    r.click(SWAP, 0);
    assert_eq!(
        r.km().bindings(Action::Fillet),
        &[chord(Key::Z, true, false, false)]
    );
    assert!(r.km().bindings(Action::Undo).is_empty());
}

#[test]
fn capturing_a_mouse_gesture_needs_a_drag_or_wheel_not_a_click() {
    let mut r = rig();
    r.filter(Action::ViewPan.label());
    r.click(CHANGE, 0);
    assert!(r.sees(DRAG_NOW), "{:?}", r.texts());
    let at = r.find(SECTION, 0);
    // Ein bloßer Klick (auch mit der rechten Taste) ist keine Geste, eine Taste auch nicht.
    r.frame(vec![Event::PointerMoved(at)]);
    r.frame(vec![button(at, PointerButton::Secondary, true, NONE)]);
    r.frame(vec![button(at, PointerButton::Secondary, false, NONE)]);
    r.press(Key::P, NONE);
    // Das Rad zählt nicht fürs Schwenken.
    r.frame(vec![Event::MouseWheel {
        unit: egui::MouseWheelUnit::Line,
        delta: vec2(0.0, 1.0),
        modifiers: NONE,
        phase: egui::TouchPhase::Move,
    }]);
    r.settle();
    assert!(r.sees(DRAG_NOW));
    assert_eq!(r.km(), Keymap::defaults());
    // Gedrückt halten, 3 Pixel: noch nichts; ab 4 Pixel die Geste mit den gehaltenen Tasten.
    r.frame_with(vec![button(at, PointerButton::Secondary, true, CTRL)], CTRL);
    r.frame_with(vec![Event::PointerMoved(at + vec2(3.0, 0.0))], CTRL);
    assert_eq!(r.km(), Keymap::defaults());
    r.frame_with(vec![Event::PointerMoved(at + vec2(3.0, 3.0))], CTRL);
    let drag = Binding::Mouse(Gesture::Drag {
        button: MouseButton::Right,
        ctrl: true,
        shift: false,
        alt: false,
    });
    assert_eq!(r.km().bindings(Action::ViewPan)[0], drag);
    // Bis zum Loslassen bleibt alles gesperrt.
    r.frame_with(vec![], CTRL);
    assert!(viewport_typing(&r));
    r.frame(vec![button(at, PointerButton::Secondary, false, NONE)]);
    r.settle();
    assert!(!viewport_typing(&r));
    assert!(r.sees("Strg + Rechte Taste ziehen"), "{:?}", r.texts());

    // Zoomen: nur das Rad, mit den Umschalttasten des Radereignisses.
    r.filter(Action::ViewZoom.label());
    r.click(CHANGE, 0);
    assert!(r.sees(WHEEL_NOW));
    r.frame(vec![button(at, PointerButton::Middle, true, NONE)]);
    r.frame(vec![Event::PointerMoved(at + vec2(20.0, 0.0))]);
    r.frame(vec![button(at, PointerButton::Middle, false, NONE)]);
    r.settle();
    assert!(r.sees(WHEEL_NOW));
    let alt = Modifiers { alt: true, ..NONE };
    r.frame_with(
        vec![Event::MouseWheel {
            unit: egui::MouseWheelUnit::Line,
            delta: vec2(0.0, -1.0),
            modifiers: alt,
            phase: egui::TouchPhase::Move,
        }],
        alt,
    );
    r.settle();
    assert_eq!(
        r.km().bindings(Action::ViewZoom)[0],
        Binding::Mouse(Gesture::Wheel {
            ctrl: false,
            shift: false,
            alt: true
        })
    );
    assert!(!r.sees(WHEEL_NOW));
}

#[test]
fn reset_row_and_reset_all() {
    let mut r = rig();
    let mut km = Keymap::defaults();
    km.swap(Action::Save, 0, chord(Key::F5, false, false, false));
    km.swap(Action::Fillet, 0, chord(Key::G, false, false, false));
    assert!(r.ws().set_keymap(km));
    r.filter(Action::Save.label());
    r.click(RESET, 0);
    assert!(r.km().is_default(Action::Save));
    assert!(!r.km().is_default(Action::Fillet));
    r.click(RESET_ALL, 0);
    assert_eq!(r.km(), Keymap::defaults());
    assert!(r.saved_keys().is_empty());
    // Zurücksetzen holt eine weggegebene Voreinstellung zurück; die andere Aktion verliert sie
    // (keine Taste ist doppelt belegt) und der Hinweis sagt es.
    let mut km = Keymap::defaults();
    km.swap(Action::Export, 1, chord(Key::S, true, false, false));
    assert!(km.bindings(Action::Save).is_empty());
    assert!(r.ws().set_keymap(km));
    r.filter(Action::Save.label());
    r.click(RESET, 0);
    assert!(r.km().is_default(Action::Save));
    assert!(r.km().is_default(Action::Export));
    assert!(
        r.sees(&taken_text(
            &chord(Key::S, true, false, false),
            Action::Export
        )),
        "{:?}",
        r.texts()
    );
}

#[test]
fn save_failure_keeps_the_value_and_shows_a_hint() {
    let (mut w, tmp) = ws_with(Config::default());
    // Der Ordner der Konfiguration ist eine Datei: Schreiben ist unmöglich.
    let blocker = tmp.join("blockiert");
    std::fs::write(&blocker, b"x").unwrap();
    w.config_path = blocker.join("config.json");
    let mut r = Rig::new(w);
    r.filter(Action::Save.label());
    r.click(CHANGE, 0);
    r.press(Key::F5, NONE);
    assert_eq!(
        r.km().bindings(Action::Save),
        &[chord(Key::F5, false, false, false)]
    );
    assert!(r.ws().keys_pending);
    assert_eq!(
        r.state().message,
        Some(RootMessage::Warning(KEYS_NOT_SAVED.into()))
    );
    assert!(r.sees(KEYS_NOT_SAVED), "{:?}", r.texts());
    // Gelingt das nächste Speichern, ist der Hinweis weg.
    r.ws().config_path = tmp.join("config.json");
    r.click(ADD, 0);
    r.press(Key::F6, NONE);
    assert!(!r.sees(KEYS_NOT_SAVED));
    assert!(!r.ws().keys_pending);
}

#[test]
fn load_notes_are_shown() {
    let mut config = Config::default();
    config
        .keys
        .insert("gibt.es.nicht".into(), vec!["F1".into()]);
    config
        .keys
        .insert("datei.speichern".into(), vec!["Strg+Quatsch".into()]);
    let (w, _) = ws_with(config);
    assert_eq!(w.key_notes.len(), 2, "{:?}", w.key_notes);
    let notes = w.key_notes.clone();
    let mut r = Rig::new(w);
    assert!(r.sees(NOTES_HEADING), "{:?}", r.texts());
    for n in &notes {
        assert!(r.sees(n), "{n}");
    }
    // Nach einer gespeicherten Änderung steht nur noch die neue Belegung in der Datei: die
    // Hinweise sind erledigt.
    r.filter(Action::Fillet.label());
    r.click(CHANGE, 0);
    r.press(Key::G, NONE);
    assert!(r.ws().key_notes.is_empty());
    assert!(!r.sees(NOTES_HEADING));
    assert!(!r.saved_keys().contains_key("gibt.es.nicht"));

    // Höchstens 20 Hinweise und „… und N weitere“.
    let many: BTreeMap<String, Vec<String>> = (0..30)
        .map(|i| (format!("unbekannt.{i:02}"), vec!["F1".to_string()]))
        .collect();
    let (_, notes) = Keymap::from_overrides(&many);
    assert_eq!(notes.len(), 21);
    assert_eq!(notes[20], "… und 10 weitere");
}

#[test]
fn removing_the_last_binding_keeps_the_action_listed_as_unbound() {
    let mut r = rig();
    r.filter(Action::Redo.label());
    // Wiederholen hat zwei Belegungen: die zweite entfernen, die erste bleibt.
    r.click(REMOVE, 1);
    assert_eq!(
        r.km().bindings(Action::Redo),
        &[chord(Key::Y, true, false, false)]
    );
    r.click(REMOVE, 0);
    assert!(r.km().bindings(Action::Redo).is_empty());
    assert!(r.sees(Action::Redo.label()));
    assert!(r.sees(UNBOUND), "{:?}", r.texts());
    assert!(r.sees(ADD));
    assert_eq!(
        r.saved_keys(),
        BTreeMap::from([("bearbeiten.wiederholen".into(), vec![])])
    );
    // Strg+Y löst nichts mehr aus.
    assert_eq!(
        r.km().action_for_key(
            Area::Model,
            &KeyChord {
                key: Key::Y,
                ctrl: true,
                shift: false,
                alt: false
            }
        ),
        None
    );
}

// ---- Nacharbeit nach der Prüfung --------------------------------------------------------------

#[test]
fn holding_the_captured_key_does_not_fire_it_until_released() {
    let mut r = rig();
    let tabs = r.tabs();
    // „Neues Dokument“ bekommt F5; F5 bleibt gehalten: die Wiederholungen öffnen keinen Tab.
    r.filter(Action::New.label());
    r.click(CHANGE, 0);
    r.frame(vec![key_event(Key::F5, NONE)]);
    assert_eq!(
        r.km().bindings(Action::New),
        &[chord(Key::F5, false, false, false)]
    );
    assert_eq!(r.state().capturing, None);
    for _ in 0..10 {
        r.frame(vec![repeat_event(Key::F5, NONE)]);
        assert!(viewport_typing(&r), "gesperrt, solange F5 unten ist");
    }
    assert_eq!(r.tabs(), tabs, "die Wiederholungen lösen nichts aus");
    r.frame(vec![release_event(Key::F5, NONE)]);
    r.settle();
    assert!(!viewport_typing(&r));
    assert_eq!(r.tabs(), tabs);
    // Losgelassen: die neue Belegung wirkt.
    r.press(Key::F5, NONE);
    assert_eq!(r.tabs(), tabs + 1);
    // Auch nach einer Doppelbelegung bleibt die gehaltene Taste gesperrt.
    r.filter(Action::Save.label());
    r.click(CHANGE, 0);
    r.frame_with(vec![key_event(Key::N, CTRL)], CTRL);
    for _ in 0..5 {
        r.frame_with(vec![repeat_event(Key::N, CTRL)], CTRL);
    }
    assert_eq!(r.tabs(), tabs + 1, "Strg+N gehalten öffnet keinen Tab");
    r.frame_with(vec![release_event(Key::N, CTRL)], NONE);
    r.settle();
    assert!(!viewport_typing(&r));
}

/// Beginnt das Erfassen bei „Speichern“.
fn start_capture(r: &mut Rig) {
    r.filter(Action::Save.label());
    r.click(CHANGE, 0);
    assert!(r.state().capturing.is_some());
    assert!(viewport_typing(r));
}

/// Nach dem Schließen ist die Sperre weg und Strg+N öffnet wieder einen Tab.
fn assert_unlocked(r: &mut Rig, route: &str) {
    r.settle();
    assert!(!r.app.windows_mut().2.is_open(), "{route}: Fenster zu");
    assert!(
        !viewport_typing(r),
        "{route}: Sperre nach dem Schließen weg"
    );
    let tabs = r.tabs();
    r.press(Key::N, CTRL);
    assert_eq!(r.tabs(), tabs + 1, "{route}: Strg+N wirkt wieder");
}

#[test]
fn closing_the_window_while_capturing_frees_the_shortcuts() {
    // `SettingsWindow::close()`.
    let mut r = rig();
    start_capture(&mut r);
    r.app.windows_mut().2.close();
    assert_unlocked(&mut r, "close()");

    // Fußzeile „Schließen“ (der unterste Text „Schließen“; darüber steht die Aktion).
    let mut r = rig();
    start_capture(&mut r);
    r.settle();
    let at = r
        .seen
        .iter()
        .filter(|s| s.text == planora_app::dialog::CLOSE_LABEL)
        .max_by(|a, b| a.rect.top().total_cmp(&b.rect.top()))
        .unwrap()
        .rect
        .center();
    r.click_at(at);
    assert_unlocked(&mut r, "Schließen");

    // ✕ der Kopfzeile.
    let mut r = rig();
    start_capture(&mut r);
    let close = planora_app::icons::glyph(planora_app::icons::Icon::Close).to_string();
    r.settle();
    let window = r.app.windows_mut().2.last_rect.unwrap();
    let at = r
        .seen
        .iter()
        .find(|s| s.text == close && window.contains_rect(s.rect))
        .unwrap()
        .rect
        .center();
    r.click_at(at);
    assert_unlocked(&mut r, "✕");

    // Projekte öffnen (der Weg von Strg+O und „Projekte“ im Menü).
    let mut r = rig();
    start_capture(&mut r);
    let (ws, projects, settings) = r.app.windows_mut();
    planora_app::app::open_projects(ws, projects, settings);
    r.settle();
    assert!(!r.app.windows_mut().2.is_open());
    assert!(!viewport_typing(&r), "Sperre nach dem Schließen weg");

    // Ein Dateifenster geht auf: die Fensterregeln schließen die Einstellungen.
    let mut r = rig();
    start_capture(&mut r);
    let dir = std::env::temp_dir();
    r.ws().active_tab_mut().import_dialog =
        Some(planora_app::import_dialog::ImportDialog::new(dir));
    r.settle();
    assert!(!r.app.windows_mut().2.is_open());
    assert!(!viewport_typing(&r), "Sperre nach dem Schließen weg");
    r.ws().active_tab_mut().import_dialog = None;
    assert_unlocked(&mut r, "Dateifenster");
}

#[test]
fn swap_names_the_action_that_lost_the_key() {
    let mut r = rig();
    r.filter(Action::Save.label());
    r.click(CHANGE, 0);
    r.press(Key::N, CTRL);
    r.click(SWAP, 0);
    let n = chord(Key::N, true, false, false);
    assert_eq!(
        swapped_text(&n, Action::New),
        "Strg+N gilt nicht mehr für „Neues Dokument“."
    );
    assert!(r.sees(&swapped_text(&n, Action::New)), "{:?}", r.texts());
}

#[test]
fn a_click_elsewhere_ends_capturing() {
    // Klick ins Suchfeld: das Erfassen endet, Tippen geht ins Feld.
    let mut r = rig();
    start_capture(&mut r);
    // Das Suchfeld zeigt den Suchtext „Speichern“; der oberste Treffer ist das Feld.
    let field = r.find(Action::Save.label(), 0);
    r.click_at(field);
    assert_eq!(r.state().capturing, None);
    assert!(!r.sees(PRESS_KEY));
    assert_eq!(r.km(), Keymap::defaults());

    // „Projektverzeichnis ändern …“: Erfassen endet, die Auswahl öffnet.
    let mut r = rig();
    start_capture(&mut r);
    r.click(planora_app::settings_window::CHANGE_DIRECTORY, 0);
    assert_eq!(r.state().capturing, None);
    assert!(r.app.windows_mut().2.chooser_mut().is_some());

    // „Ändern“ einer anderen Zeile wechselt das Erfassen dorthin.
    let mut r = rig();
    r.filter("strg");
    r.click(CHANGE, 0);
    assert_eq!(r.state().capturing, Some((Action::New, 0)));
    // Die erfassende Zeile zeigt kein „Ändern“: das erste ist jetzt das von „Projekte“.
    r.click(CHANGE, 0);
    assert_eq!(r.state().capturing, Some((Action::Projects, 0)));
    // Klick außerhalb des Fensters (in die Ansicht).
    r.click_at(pos2(60.0, SIZE.y / 2.0));
    assert_eq!(r.state().capturing, None);
    assert!(!viewport_typing(&r));
}

#[test]
fn changing_to_the_actions_own_default_key_is_no_conflict() {
    let mut r = rig();
    let cfg = r.ws().config_path.clone();
    r.filter(Action::Save.label());
    r.click(CHANGE, 0);
    r.press(Key::S, CTRL);
    assert!(!r.sees(SWAP), "keine Doppelbelegung mit sich selbst");
    assert_eq!(r.state().pending_conflict, None);
    assert_eq!(r.km(), Keymap::defaults());
    assert!(!cfg.exists(), "nichts geschrieben");
    // Die zweite Belegung von „Wiederholen“ auf die erste (Strg+Y): keine Rückfrage, die
    // doppelte Zeile entfällt.
    r.filter(Action::Redo.label());
    r.click(CHANGE, 1);
    r.press(Key::Y, CTRL);
    assert!(!r.sees(SWAP));
    assert_eq!(
        r.km().bindings(Action::Redo),
        &[chord(Key::Y, true, false, false)]
    );
}

#[test]
fn keys_the_views_need_cannot_be_bound() {
    let mut r = rig();
    for (action, key, text) in [
        (
            Action::SketchLine,
            Key::Delete,
            "„Entf“ ist in der Skizze fest belegt.",
        ),
        (
            Action::SketchCircle,
            Key::Enter,
            "„Eingabe“ ist in der Skizze fest belegt.",
        ),
        (
            Action::DrawingBalloon,
            Key::Delete,
            "„Entf“ ist in der Zeichnung fest belegt.",
        ),
        (
            Action::Fillet,
            Key::Tab,
            "„Tab“ ist für den Wechsel zwischen Bedienelementen fest belegt.",
        ),
        (
            Action::Fillet,
            Key::ArrowLeft,
            "„Pfeil links“ ist an der Verlaufsleiste fest belegt.",
        ),
    ] {
        r.filter(action.label());
        r.click(CHANGE, 0);
        r.press(key, NONE);
        assert_eq!(r.km(), Keymap::defaults(), "{action:?} {key:?}");
        assert_eq!(r.state().capturing, None);
        assert!(r.sees(text), "{text}: {:?}", r.texts());
    }
    // Eingabe in der Zeichnung ist dagegen frei, Entf im Modell auch.
    r.filter(Action::DrawingBalloon.label());
    r.click(CHANGE, 0);
    r.press(Key::Enter, NONE);
    assert_eq!(
        r.km().bindings(Action::DrawingBalloon),
        &[chord(Key::Enter, false, false, false)]
    );
}

#[test]
fn a_conflict_below_the_last_visible_row_is_scrolled_into_view() {
    for size in [vec2(1920.0, 1040.0), vec2(1400.0, 900.0)] {
        let mut r = Rig::sized(ws_with(Config::default()).0, size);
        // Die Tabelle so weit rollen, dass eine Tastenaktion unten am Rand steht: ihr „Ändern“
        // ist ganz zu sehen, für die Rückfrage darunter ist aber kein Platz mehr.
        let mut target = None;
        for _ in 0..200 {
            r.frame(vec![]);
            target = bottom_key_row(&r);
            if target.is_some() {
                break;
            }
            // Über einem sichtbaren „Ändern“ (also über der Tabelle) drehen.
            let at = r
                .seen
                .iter()
                .find(|s| s.text == CHANGE && s.visible())
                .expect("ein „Ändern“ zu sehen")
                .rect
                .center();
            r.frame(vec![
                Event::PointerMoved(at),
                Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: vec2(0.0, -3.0),
                    modifiers: NONE,
                    phase: egui::TouchPhase::Move,
                },
            ]);
        }
        let (a, at) = target.unwrap_or_else(|| panic!("{size:?}: keine Zeile am unteren Rand"));
        r.click_at(at);
        assert_eq!(r.state().capturing.map(|c| c.0), Some(a), "{size:?}");
        // Strg+N gehört „Neues Dokument“ (Überall): Doppelbelegung mit jeder Tastenaktion.
        r.press(Key::N, CTRL);
        for _ in 0..30 {
            r.frame(vec![]);
        }
        let text = conflict_text(&chord(Key::N, true, false, false), Action::New);
        let prompt = r
            .seen
            .iter()
            .find(|s| s.text == text)
            .unwrap_or_else(|| panic!("{size:?}: Rückfrage fehlt"));
        assert!(
            prompt.visible(),
            "{size:?}: Rückfrage {:?} in {:?}",
            prompt.rect,
            prompt.clip
        );
        for b in [SWAP, CANCEL] {
            let seen = r.seen.iter().find(|s| s.text == b).unwrap();
            assert!(seen.visible(), "{size:?}: „{b}“ zu sehen");
        }
        // „Tauschen“ lässt sich anklicken.
        let swap = r
            .seen
            .iter()
            .find(|s| s.text == SWAP)
            .unwrap()
            .rect
            .center();
        r.click_at(swap);
        assert_eq!(
            r.km().bindings(a),
            &[chord(Key::N, true, false, false)],
            "{size:?}"
        );
        assert!(r.km().bindings(Action::New).is_empty());
    }
}

/// Die unterste ganz sichtbare Zeile einer Tastenaktion (nicht „Neues Dokument“, nur eine
/// Belegung), wenn darunter für die Rückfrage kein Platz mehr ist: Aktion und Mitte ihres
/// „Ändern“.
fn bottom_key_row(r: &Rig) -> Option<(Action, Pos2)> {
    let change = r
        .seen
        .iter()
        .filter(|s| s.text == CHANGE && s.visible())
        .max_by(|a, b| a.rect.top().total_cmp(&b.rect.top()))?;
    if change.clip.bottom() - change.rect.bottom() > 20.0 {
        return None;
    }
    let label = r
        .seen
        .iter()
        .filter(|s| {
            s.clip == change.clip
                && (s.rect.top() - change.rect.top()).abs() < 6.0
                && s.rect.left() < change.rect.left()
        })
        .min_by(|a, b| a.rect.left().total_cmp(&b.rect.left()))?;
    let a = Action::ALL
        .iter()
        .copied()
        .find(|a| a.label() == label.text)?;
    (!is_mouse_action(a) && a != Action::New && a.default_bindings().len() == 1)
        .then(|| (a, change.rect.center()))
}
