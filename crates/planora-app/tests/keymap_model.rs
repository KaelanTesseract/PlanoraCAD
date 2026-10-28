//! Fusion-Kürzel im Modell (E Skizze extrudieren, F Verrunden, V Sichtbarkeit, Entf Löschen) und
//! „Letzten Befehl wiederholen“ (Leertaste): gleiche Wirkung wie Menü und Befehlsfeld (alle über
//! `commands::run_action`), gemerkt wird erst ein abgeschlossener Befehl, die Leertaste gehört
//! einem Textfeld oder fokussierten Knopf, nicht aber dem Marker der Verlaufsleiste.
//!
//! Mit echten Bildern der ganzen App (`PlanoraApp::for_tests`, `eframe::App::ui`) bzw. einem
//! nackten egui-Kontext; Bibliothek und Konfiguration nur im Temp-Ordner.

mod common;

use eframe::App as _;
use egui::{Event, Key, Modifiers, PointerButton, Pos2, Rect, Vec2, vec2};
use planora_app::app::{self, PlanoraApp};
use planora_app::blend_dialog::BlendDialog;
use planora_app::commands::{self, LastCommand};
use planora_app::create_dialog::CreateDialog;
use planora_app::keymap::{Action, Area, Binding, KeyChord, Keymap, SetResult, shortcut_hint};
use planora_app::state::AppState;
use planora_app::workspace::Workspace;
use planora_app::{browser, fonts, history_bar, shell_panels, theme, viewport};
use planora_feature::{FeatureId, SketchPlane, StandardPlane};
use planora_math::{DVec2, DVec3};
use planora_project::config::Config;
use planora_project::library::Library;

const SIZE: Vec2 = vec2(1400.0, 900.0);
const NONE: Modifiers = Modifiers::NONE;

// ---- Hilfen ---------------------------------------------------------------------------------

fn ws() -> Workspace {
    let root = common::temp_root("planora-keymap-model");
    let mut w = Workspace::new(Some(Library::open(&root).unwrap()), Config::default());
    w.config_path = root.join("config.toml");
    w
}

fn chord(key: Key, ctrl: bool, shift: bool, alt: bool) -> KeyChord {
    KeyChord {
        key,
        ctrl,
        shift,
        alt,
    }
}

fn plain(key: Key) -> Binding {
    Binding::Key(chord(key, false, false, false))
}

fn key(k: Key, modifiers: Modifiers) -> Event {
    Event::Key {
        key: k,
        physical_key: Some(k),
        pressed: true,
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

/// Ein gezeichneter Text mit seinem Rechteck.
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
    /// Blieben nach dem Bild Tastenereignisse übrig (nicht verbraucht)?
    left_over: bool,
}

impl Rig {
    fn new(w: Workspace) -> Rig {
        let ctx = egui::Context::default();
        ctx.set_fonts(fonts::definitions());
        theme::apply(&ctx, true);
        PlanoraApp::setup_context(&ctx);
        ctx.all_styles_mut(|s| s.interaction.tooltip_delay = 0.0);
        let mut rig = Rig {
            ctx,
            app: PlanoraApp::for_tests(w),
            time: 0.0,
            left_over: false,
        };
        rig.settle();
        rig
    }

    fn ws(&mut self) -> &mut Workspace {
        self.app.ws_mut()
    }

    fn st(&mut self) -> &mut AppState {
        &mut self.app.ws_mut().active_tab_mut().state
    }

    fn frame(&mut self, events: Vec<Event>) -> Vec<Seen> {
        self.time += 1.0 / 60.0;
        let raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, SIZE)),
            time: Some(self.time),
            events,
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
            walk(&c.shape, &mut seen);
        }
        out.drop_without_applying_deltas();
        seen
    }

    fn settle(&mut self) -> Vec<Seen> {
        self.frame(vec![]);
        self.frame(vec![]);
        self.frame(vec![])
    }

    fn press(&mut self, k: Key, m: Modifiers) {
        self.frame(vec![key(k, m)]);
    }

    /// Klick an `p` (hin, drücken, loslassen, ein Bild danach).
    fn click(&mut self, p: Pos2) {
        self.frame(vec![Event::PointerMoved(p)]);
        self.frame(vec![button(p, true)]);
        self.frame(vec![button(p, false)]);
        self.frame(vec![]);
    }

    /// Texte, während die Maus über `p` steht (Tooltip ohne Verzögerung).
    fn hover(&mut self, p: Pos2) -> Vec<String> {
        let mut seen = vec![];
        for _ in 0..5 {
            self.time += 0.3;
            seen = self.frame(vec![Event::PointerMoved(p)]);
        }
        seen.into_iter().map(|s| s.text).collect()
    }

    /// Klick auf den sichtbaren Text `text` (bei mehreren der unterste, also im Fenster vor
    /// der Menüleiste).
    fn click_text(&mut self, text: &str) {
        let seen = self.settle();
        let at = seen
            .iter()
            .filter(|s| s.text == text)
            .max_by(|a, b| a.rect.top().total_cmp(&b.rect.top()))
            .unwrap_or_else(|| panic!("„{text}“ nicht zu sehen"))
            .rect
            .center();
        self.click(at);
    }

    /// Klick auf den obersten sichtbaren Text `text` (Menüleiste vor Fenstern).
    fn click_top(&mut self, text: &str) {
        let seen = self.settle();
        let at = seen
            .iter()
            .filter(|s| s.text == text)
            .min_by(|a, b| a.rect.top().total_cmp(&b.rect.top()))
            .unwrap_or_else(|| panic!("„{text}“ nicht zu sehen"))
            .rect
            .center();
        self.click(at);
    }

    /// Echter Weg durch die Menüleiste: Menü `menu` öffnen, Eintrag `entry` anklicken.
    fn menu(&mut self, menu: &str, entry: &str) {
        self.click_top(menu);
        self.click_top(entry);
    }

    /// Das offene Fenster Verrunden ausführen (wie „Erzeugen“).
    fn finish_fillet(&mut self) {
        let tab = self.ws().active_tab_mut();
        let mut d = tab.blend_dialog.take().expect("Fenster Verrunden");
        assert!(!d.execute(&mut tab.state), "{:?}", d.error());
        self.settle();
    }

    /// Klick auf den Knopf `label` im Befehlsfeld.
    fn click_bottom(&mut self, label: &str) {
        let at = bottom_button(&self.ctx, label).center();
        self.click(at);
    }

    fn features(&mut self) -> usize {
        self.st().document.features().len()
    }

    fn last(&mut self) -> Option<Action> {
        self.ws().last_command.last()
    }
}

fn bottom_button(ctx: &egui::Context, label: &str) -> Rect {
    shell_panels::bottom_buttons(ctx)
        .into_iter()
        .find(|(l, _)| l == label)
        .unwrap_or_else(|| panic!("Knopf „{label}“ fehlt"))
        .1
}

/// Eine Skizze mit Rechteck (geschlossenes Profil), im Baum gewählt.
fn sketch_ws() -> (Workspace, FeatureId) {
    let mut w = ws();
    let s = &mut w.active_tab_mut().state;
    let sk = s
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    s.open_sketch(sk).unwrap();
    s.session_sketch_mut()
        .unwrap()
        .1
        .add_rectangle(DVec2::ZERO, DVec2::new(5.0, 10.0))
        .unwrap();
    s.close_sketch();
    s.selected_feature = Some(sk);
    w.observe_history(false);
    (w, sk)
}

/// Ein Quader 20 mm, im Baum gewählt; dazu eine gewählte Kante (für Verrunden).
fn box_ws() -> (Workspace, FeatureId) {
    let mut w = ws();
    let s = &mut w.active_tab_mut().state;
    let b = s.create_box(DVec3::splat(20.0)).unwrap();
    let e = s.bodies[0].solid.edges.iter().next().unwrap().0;
    s.set_selection(Some(planora_render::Pick::Edge(planora_render::EdgeRef {
        body: 0,
        edge: e,
    })));
    s.selected_body = Some(0);
    w.observe_history(false);
    (w, b)
}

/// Index des sichtbaren Körpers, den `feature` erzeugt.
fn body_of(s: &AppState, feature: FeatureId) -> usize {
    s.bodies
        .iter()
        .position(|b| b.source.feature == feature)
        .expect("Körper sichtbar")
}

/// Verrunden über F öffnen und im Fenster ausführen (wie „Erzeugen“).
fn fillet_via_key(rig: &mut Rig) {
    rig.press(Key::F, NONE);
    rig.settle();
    let tab = rig.ws().active_tab_mut();
    let mut d = tab.blend_dialog.take().expect("Fenster Verrunden");
    assert!(!d.execute(&mut tab.state), "{:?}", d.error());
    rig.settle();
}

// ---- Tabelle und Belegung -------------------------------------------------------------------

#[test]
fn model_defaults_and_conflicts_with_all_actions() {
    let km = Keymap::defaults();
    for (a, k) in [
        (Action::ExtrudeSketch, Key::E),
        (Action::Fillet, Key::F),
        (Action::ToggleVisibility, Key::V),
        (Action::DeleteSelection, Key::Delete),
        (Action::RepeatLast, Key::Space),
    ] {
        assert_eq!(a.area(), Area::Model, "{a:?}");
        assert_eq!(km.bindings(a), [plain(k)], "{a:?}");
        let c = chord(k, false, false, false);
        assert_eq!(km.action_for_key(Area::Model, &c), Some(a));
        // In Skizze, Zeichnung und „Überall“ gilt die Modelltaste nicht (die Leertaste hat in
        // der Skizze ihre eigene Aktion).
        for area in [Area::Sketch, Area::Drawing, Area::All] {
            let want = (a == Action::RepeatLast && area == Area::Sketch)
                .then_some(Action::SketchRepeatLast);
            assert_eq!(km.action_for_key(area, &c), want, "{a:?} in {area:?}");
        }
    }
    // Strg+S im Modell: die „Überall“-Aktion Speichern.
    let ctrl_s = chord(Key::S, true, false, false);
    assert_eq!(km.action_for_key(Area::Model, &ctrl_s), Some(Action::Save));
    // Doppelbelegung zwischen Modell und „Überall“ (beide Richtungen) und im Modell.
    assert_eq!(
        km.conflict_for(Action::ToggleVisibility, &Binding::Key(ctrl_s)),
        Some(Action::Save)
    );
    assert_eq!(
        km.conflict_for(Action::Save, &plain(Key::E)),
        Some(Action::ExtrudeSketch)
    );
    assert_eq!(
        km.conflict_for(Action::ExtrudeSketch, &plain(Key::F)),
        Some(Action::Fillet)
    );
    let mut km2 = km.clone();
    assert_eq!(
        km2.set(Action::ToggleVisibility, 0, Binding::Key(ctrl_s)),
        SetResult::Conflict(Action::Save)
    );
    assert_eq!(km2, km, "nichts geändert");
    // Wiederholbar sind im Modell nur die Erzeugen-/Bearbeiten-Befehle (die Skizzenwerkzeuge
    // prüft `keymap_sketch_drawing.rs`).
    let repeatable: Vec<Action> = Action::ALL
        .iter()
        .copied()
        .filter(|a| a.repeatable() && a.area() == Area::Model)
        .collect();
    assert_eq!(repeatable, [Action::ExtrudeSketch, Action::Fillet]);
}

#[test]
fn fusion_keys_without_a_function_are_not_bound() {
    let km = Keymap::defaults();
    // M Verschieben, H Bohrung, I Messen, Q Drücken/Ziehen, A Erscheinungsbild, 1/2/3
    // Auswahlarten, Strg+B Alles neu berechnen, Strg+C/X/V: gibt es (noch) nicht.
    for c in [
        chord(Key::M, false, false, false),
        chord(Key::H, false, false, false),
        chord(Key::I, false, false, false),
        chord(Key::Q, false, false, false),
        chord(Key::A, false, false, false),
        chord(Key::Num1, false, false, false),
        chord(Key::Num2, false, false, false),
        chord(Key::Num3, false, false, false),
        chord(Key::B, true, false, false),
        chord(Key::C, true, false, false),
        chord(Key::X, true, false, false),
        chord(Key::V, true, false, false),
    ] {
        assert_eq!(km.action_for_key(Area::Model, &c), None, "{c:?}");
    }
}

// ---- Tasten wie Menü ------------------------------------------------------------------------

#[test]
fn each_model_key_runs_its_command() {
    // E: wie „Erzeugen → Skizze extrudieren …“.
    let (w, sk) = sketch_ws();
    let mut rig = Rig::new(w);
    rig.press(Key::E, NONE);
    assert!(!rig.left_over, "E verbraucht");
    let by_key = rig.ws().active_tab().dialog.clone();
    let (mut twin, _) = sketch_ws();
    commands::run_action(&mut twin, Action::ExtrudeSketch);
    assert_eq!(by_key, twin.active_tab().dialog);
    assert_eq!(by_key, Some(CreateDialog::extrude_sketch(sk)));

    // F: wie „Bearbeiten → Verrunden …“.
    let (w, _) = box_ws();
    let mut rig = Rig::new(w);
    rig.press(Key::F, NONE);
    let title = rig
        .ws()
        .active_tab()
        .blend_dialog
        .as_ref()
        .map(BlendDialog::title);
    let (mut twin, _) = box_ws();
    commands::run_action(&mut twin, Action::Fillet);
    assert_eq!(
        title,
        twin.active_tab().blend_dialog.as_ref().map(|d| d.title())
    );
    assert_eq!(title, Some(BlendDialog::fillet().title()));

    // V: wie das Auge im Feature-Baum (Körper ausblenden); erneut gewählt: wieder ein.
    let (w, _) = box_ws();
    let mut rig = Rig::new(w);
    rig.press(Key::V, NONE);
    assert!(rig.st().is_body_hidden(0), "V blendet aus");
    rig.st().selected_body = Some(0);
    rig.press(Key::V, NONE);
    assert!(!rig.st().is_body_hidden(0), "V blendet wieder ein");

    // Entf: wie „Löschen“ im Kontextmenü (ohne Abhängige sofort).
    let (w, b) = box_ws();
    let mut rig = Rig::new(w);
    rig.press(Key::Delete, NONE);
    assert!(rig.st().document.feature(b).is_none(), "gelöscht");
    let (mut twin, _) = box_ws();
    browser::request_delete(&mut twin.active_tab_mut().state, b);
    assert_eq!(
        rig.st().document.features().len(),
        twin.active_tab().state.document.features().len()
    );
}

#[test]
fn keys_do_nothing_without_a_matching_selection() {
    // E ohne Skizze: Hinweis, kein Fenster, nichts Neues.
    let mut rig = Rig::new(ws());
    rig.press(Key::E, NONE);
    assert!(rig.ws().active_tab().dialog.is_none());
    assert_eq!(
        rig.st().info.as_deref(),
        Some(commands::need_sketch().as_str())
    );
    assert_eq!(rig.features(), 0);

    // E mit einer Skizze ohne geschlossenes Profil und E mit gewähltem Körper statt Skizze.
    let mut w = ws();
    let s = &mut w.active_tab_mut().state;
    let empty = s
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    s.selected_feature = Some(empty);
    let mut rig = Rig::new(w);
    rig.press(Key::E, NONE);
    assert!(rig.ws().active_tab().dialog.is_none(), "kein Profil");
    assert_eq!(
        rig.st().info.as_deref(),
        Some(commands::need_sketch().as_str())
    );
    let (w, _) = box_ws();
    let mut rig = Rig::new(w);
    rig.press(Key::E, NONE);
    assert!(
        rig.ws().active_tab().dialog.is_none(),
        "Körper statt Skizze"
    );

    // F, V, Entf ohne Körper: Hinweis, nichts geschieht.
    let (w, sk) = sketch_ws();
    let mut rig = Rig::new(w);
    rig.press(Key::F, NONE);
    assert!(rig.ws().active_tab().blend_dialog.is_none());
    assert!(rig.st().info.is_some(), "Hinweis in der Statuszeile");
    rig.st().selected_feature = None;
    rig.press(Key::V, NONE);
    assert_eq!(
        rig.st().info.as_deref(),
        Some(commands::need_body().as_str())
    );
    rig.press(Key::Delete, NONE);
    assert_eq!(
        rig.st().info.as_deref(),
        Some(commands::need_selection().as_str())
    );
    assert!(rig.st().document.feature(sk).is_some());
}

#[test]
fn model_keys_stay_out_of_an_open_sketch() {
    // Offene Skizze: Entf gehört der Skizze, der gewählte Körper bleibt.
    let (mut w, b) = box_ws();
    let s = &mut w.active_tab_mut().state;
    let sk = s
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    s.open_sketch(sk).unwrap();
    s.selected_body = Some(0);
    let mut rig = Rig::new(w);
    for k in [Key::Delete, Key::E, Key::F, Key::V] {
        rig.press(k, NONE);
    }
    rig.settle();
    assert!(rig.st().document.feature(b).is_some());
    assert!(!rig.st().is_body_hidden(0));
    assert!(rig.ws().active_tab().dialog.is_none());
    assert!(rig.ws().active_tab().blend_dialog.is_none());
}

#[test]
fn delete_key_asks_before_deleting_dependents_and_works_on_locked_bodies() {
    // Skizze mit Extrusion: Rückfrage, noch nichts gelöscht.
    let (mut w, sk) = sketch_ws();
    w.active_tab_mut().state.extrude_sketch(sk, 0, 2.0).unwrap();
    w.active_tab_mut().state.selected_feature = Some(sk);
    w.observe_history(false);
    let mut rig = Rig::new(w);
    rig.press(Key::Delete, NONE);
    assert!(rig.st().pending_delete.is_some(), "Rückfrage");
    assert!(rig.st().document.feature(sk).is_some());

    // Zurückgesetzter Körper (Marker vor dem Verschieben): Löschen bleibt erlaubt.
    let mut w = ws();
    let s = &mut w.active_tab_mut().state;
    let b = s.create_box(DVec3::splat(10.0)).unwrap();
    s.move_body(0, DVec3::X).unwrap();
    let m = s.document.features().last().unwrap().id;
    history_bar::move_marker(s, m, 1).unwrap();
    assert!(history_bar::is_locked(s, m));
    s.selected_body = Some(body_of(s, b));
    w.observe_history(false);
    let mut rig = Rig::new(w);
    rig.press(Key::Delete, NONE);
    assert_eq!(rig.st().message, None, "keine Sperre");
    assert!(
        rig.st().pending_delete.is_some(),
        "Rückfrage: Verschieben hängt daran"
    );
    rig.st().pending_delete = None;
    rig.settle();
    // Der zurückgesetzte Schritt selbst (gewählt wie in der Verlaufsleiste).
    rig.st().selected_feature = Some(m);
    rig.press(Key::Delete, NONE);
    assert!(rig.st().document.feature(m).is_none(), "gelöscht");
}

// ---- Leertaste ------------------------------------------------------------------------------

#[test]
fn space_repeats_the_last_command_via_menu_and_via_key() {
    // Über die Taste: F, ausgeführt; Leertaste öffnet Verrunden wieder.
    let (w, _) = box_ws();
    let mut rig = Rig::new(w);
    assert_eq!(rig.last(), None);
    fillet_via_key(&mut rig);
    assert_eq!(rig.last(), Some(Action::Fillet));
    rig.press(Key::Space, NONE);
    assert!(!rig.left_over, "Leertaste verbraucht");
    assert!(
        rig.ws().active_tab().blend_dialog.is_some(),
        "Verrunden wieder offen"
    );

    // Über das Befehlsfeld: „Skizze extrudieren …“, im Fenster „Erzeugen“.
    let (w, sk) = sketch_ws();
    let mut rig = Rig::new(w);
    rig.ws().last_command.remember(Action::Fillet);
    rig.click_bottom(&app::CreateKind::Extrusion.label());
    assert!(rig.ws().active_tab().dialog.is_some(), "Fenster offen");
    assert_eq!(
        rig.last(),
        Some(Action::Fillet),
        "erst beim Abschluss gemerkt"
    );
    let before = rig.features();
    rig.click_text("Erzeugen");
    assert!(rig.ws().active_tab().dialog.is_none(), "erzeugt");
    assert_eq!(rig.features(), before + 1);
    assert_eq!(rig.last(), Some(Action::ExtrudeSketch));
    // Die Leertaste startet Extrudieren wieder wie das Menü (ohne gewählte Skizze: die letzte).
    rig.st().selected_feature = None;
    rig.press(Key::Space, NONE);
    assert_eq!(
        rig.ws().active_tab().dialog,
        Some(CreateDialog::extrude_sketch(sk))
    );

    // Über den Menüweg (`run_action`, wie „Bearbeiten → Verrunden …“).
    let (w, _) = box_ws();
    let mut rig = Rig::new(w);
    commands::run_action(rig.ws(), Action::Fillet);
    let tab = rig.ws().active_tab_mut();
    let mut d = tab.blend_dialog.take().unwrap();
    assert!(!d.execute(&mut tab.state), "{:?}", d.error());
    rig.settle();
    assert_eq!(rig.last(), Some(Action::Fillet));
}

#[test]
fn space_does_nothing_with_nothing_to_repeat() {
    let (w, _) = box_ws();
    let mut rig = Rig::new(w);
    let before = rig.features();
    rig.press(Key::Space, NONE);
    rig.settle();
    assert!(!rig.ws().active_tab().dialog_open());
    assert_eq!(rig.features(), before);
    assert_eq!(rig.st().message, None);
    assert_eq!(rig.st().info, None);
    assert_eq!(rig.last(), None);
}

#[test]
fn space_is_ignored_while_a_button_has_focus_and_while_typing() {
    // Tippen: das Namensfeld im Fenster „Umbenennen“ hat die Tastatur.
    let (mut w, b) = box_ws();
    w.last_command.remember(Action::Fillet);
    w.active_tab_mut().state.rename_draft = Some((b, "Teil".into()));
    let mut rig = Rig::new(w);
    let at = rig
        .settle()
        .into_iter()
        .find(|s| s.text == "Teil")
        .expect("Feld im Fenster „Umbenennen“")
        .rect
        .center();
    rig.click(at);
    assert!(viewport::typing(&rig.ctx));
    rig.press(Key::Space, NONE);
    rig.settle();
    assert!(
        rig.ws().active_tab().blend_dialog.is_none(),
        "beim Tippen nicht"
    );

    // Ein Knopf hat den Tastaturfokus (Tab): die Leertaste drückt ihn, nicht Wiederholen.
    let (mut w, _) = box_ws();
    w.last_command.remember(Action::Fillet);
    let mut rig = Rig::new(w);
    for _ in 0..10 {
        if viewport::typing(&rig.ctx) {
            break;
        }
        rig.press(Key::Tab, NONE);
        rig.settle();
    }
    assert!(viewport::typing(&rig.ctx), "ein Knopf hat den Fokus");
    rig.press(Key::Space, NONE);
    rig.settle();
    assert!(
        rig.ws().active_tab().blend_dialog.is_none(),
        "Knopf hat Vorrang"
    );
    // Nach einem Mausklick auf einen Knopf bleibt kein Fokus: die Leertaste wiederholt.
    let at = bottom_button(&rig.ctx, "Kugel …").center();
    rig.click(at);
    rig.ws().active_tab_mut().dialog = None;
    rig.settle();
    assert!(!viewport::typing(&rig.ctx), "kein Fokus nach dem Klick");
    rig.press(Key::Space, NONE);
    assert!(
        rig.ws().active_tab().blend_dialog.is_some(),
        "Wiederholen nach Klick"
    );

    // Offenes Fenster: keine Wirkung (das Fenster bleibt, wie es ist).
    rig.settle();
    rig.ws().active_tab_mut().blend_dialog = Some(BlendDialog::chamfer());
    rig.press(Key::Space, NONE);
    assert_eq!(
        rig.ws()
            .active_tab()
            .blend_dialog
            .as_ref()
            .map(|d| d.title()),
        Some(BlendDialog::chamfer().title())
    );
}

#[test]
fn space_after_clicking_a_history_step_repeats_and_does_not_press_the_marker() {
    // Skizze, Extrusion, Verschieben; der Körper ist gewählt, die Leiste zeigt seine Schritte.
    let (mut w, sk) = sketch_ws();
    let s = &mut w.active_tab_mut().state;
    let e = s.extrude_sketch(sk, 0, 2.0).unwrap();
    s.move_body(body_of(s, e), DVec3::X).unwrap();
    let m = s.document.features().last().unwrap().id;
    s.select_body(Some(m));
    s.selected_feature = None;
    w.observe_history(false);
    w.last_command.remember(Action::ExtrudeSketch);
    let mut rig = Rig::new(w);
    let steps = history_bar::bar_rects(&rig.ctx).steps;
    assert_eq!(steps.len(), 3, "{steps:?}");
    rig.click(steps[1].1.center());
    assert_eq!(rig.st().selected_feature, Some(e));
    assert!(
        rig.ctx.memory(|m| m.focused()).is_some(),
        "der Marker hat den Fokus"
    );
    assert!(!viewport::typing(&rig.ctx), "Marker zählt nicht als Tippen");
    rig.frame(vec![key(Key::Space, NONE)]);
    assert!(
        !rig.left_over,
        "Leertaste vor dem Marker verbraucht (kein Ersatzklick)"
    );
    assert_eq!(
        rig.ws().active_tab().dialog,
        Some(CreateDialog::extrude_sketch(sk))
    );
    assert_eq!(rig.st().selected_feature, Some(e), "Auswahl unverändert");
}

#[test]
fn cancelled_dialog_is_not_remembered() {
    let (w, _) = sketch_ws();
    let mut rig = Rig::new(w);
    rig.ws().last_command.remember(Action::Fillet);
    rig.press(Key::E, NONE);
    assert!(rig.ws().active_tab().dialog.is_some());
    // Solange das Fenster offen ist, wiederholt die Leertaste nichts (Fenster bleibt).
    let open = rig.ws().active_tab().dialog.clone();
    rig.press(Key::Space, NONE);
    assert_eq!(rig.ws().active_tab().dialog, open);
    assert!(rig.ws().active_tab().blend_dialog.is_none());
    let before = rig.features();
    rig.click_text("Abbrechen");
    assert!(rig.ws().active_tab().dialog.is_none(), "abgebrochen");
    assert_eq!(rig.features(), before);
    assert_eq!(rig.last(), Some(Action::Fillet), "Abbrechen merkt nichts");
    // Ein anderer Befehl ersetzt das offene Fenster: auch nichts gemerkt.
    rig.press(Key::E, NONE);
    app::do_create(rig.ws(), app::CreateKind::Box);
    rig.click_text("Erzeugen");
    assert_eq!(rig.features(), before + 1, "Quader erzeugt");
    assert_eq!(rig.last(), Some(Action::Fillet));
}

#[test]
fn save_undo_navigation_are_not_repeatable() {
    let mut last = LastCommand::default();
    for &a in Action::ALL {
        last.remember(a);
    }
    // Nur die wiederholbaren zählen; die letzte davon bleibt.
    assert_eq!(last.last(), Some(Action::Fillet));
    for a in [
        Action::New,
        Action::Projects,
        Action::Save,
        Action::Close,
        Action::Export,
        Action::Undo,
        Action::Redo,
        Action::ViewPan,
        Action::ViewOrbit,
        Action::ViewZoom,
        Action::ToggleVisibility,
        Action::DeleteSelection,
        Action::RepeatLast,
    ] {
        assert!(!a.repeatable(), "{a:?}");
        last.remember(a);
        assert_eq!(last.last(), Some(Action::Fillet), "{a:?}");
    }

    // In der App: Rückgängig über Strg+Z ersetzt den gemerkten Befehl nicht.
    let (w, _) = box_ws();
    let mut rig = Rig::new(w);
    fillet_via_key(&mut rig);
    rig.press(Key::Z, Modifiers::COMMAND);
    rig.settle();
    rig.press(Key::V, NONE);
    rig.settle();
    assert_eq!(rig.last(), Some(Action::Fillet));
}

#[test]
fn rebound_model_key_works_and_old_key_is_free() {
    let (w, _) = sketch_ws();
    let mut rig = Rig::new(w);
    let mut km = rig.ws().keymap.clone();
    assert_eq!(
        km.set(Action::ExtrudeSketch, 0, plain(Key::X)),
        SetResult::Ok
    );
    rig.ws().keymap = km;
    rig.press(Key::E, NONE);
    assert!(rig.left_over, "E bleibt liegen");
    assert!(rig.ws().active_tab().dialog.is_none());
    rig.settle();
    rig.press(Key::X, NONE);
    assert!(rig.ws().active_tab().dialog.is_some(), "X extrudiert");
}

#[test]
fn tooltips_show_the_current_keys() {
    // Befehlsfeld: Tooltip mit Taste.
    let (mut w, _) = sketch_ws();
    w.active_tab_mut()
        .state
        .create_box(DVec3::splat(10.0))
        .unwrap();
    w.observe_history(false);
    let mut rig = Rig::new(w);
    let extrude = app::CreateKind::Extrusion.label();
    let at = bottom_button(&rig.ctx, &extrude).center();
    let texts = rig.hover(at);
    assert!(texts.contains(&format!("{extrude} (E)")), "{texts:?}");
    let at = bottom_button(&rig.ctx, "Verrunden …").center();
    let texts = rig.hover(at);
    assert!(texts.iter().any(|t| t == "Verrunden … (F)"), "{texts:?}");
    // Nach dem Ändern sofort die neue Taste.
    let mut km = rig.ws().keymap.clone();
    km.set(
        Action::ExtrudeSketch,
        0,
        Binding::Key(chord(Key::E, false, true, false)),
    );
    rig.ws().keymap = km.clone();
    let at = bottom_button(&rig.ctx, &extrude).center();
    let texts = rig.hover(at);
    assert!(
        texts.contains(&format!("{extrude} (Umschalt+E)")),
        "{texts:?}"
    );
    assert_eq!(shortcut_hint(&km, Action::ExtrudeSketch), "Umschalt+E");

    // Menüs „Erzeugen“ und „Bearbeiten“: die Taste rechts.
    let menu_lines = |w: &mut Workspace, edit: bool| {
        let ctx = egui::Context::default();
        ctx.set_fonts(fonts::definitions());
        let mut texts = String::new();
        for _ in 0..2 {
            let raw = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, SIZE)),
                ..Default::default()
            };
            let out = ctx.run_ui(raw, |ui| {
                if edit {
                    app::edit_menu(ui, w);
                } else {
                    app::create_menu(ui, w);
                }
            });
            texts = planora_app::testing::collect_texts(&out);
            out.drop_without_applying_deltas();
        }
        texts.lines().map(str::to_owned).collect::<Vec<_>>()
    };
    let (mut w, _) = sketch_ws();
    w.active_tab_mut()
        .state
        .create_box(DVec3::splat(10.0))
        .unwrap();
    let lines = menu_lines(&mut w, false);
    assert!(lines.iter().any(|l| l == "E"), "{lines:?}");
    let lines = menu_lines(&mut w, true);
    assert!(lines.iter().any(|l| l == "F"), "{lines:?}");
    w.keymap = km;
    let lines = menu_lines(&mut w, false);
    assert!(lines.iter().any(|l| l == "Umschalt+E"), "{lines:?}");
    assert!(!lines.iter().any(|l| l == "E"), "{lines:?}");
}

// ---- Fix-Runde 1: Menüwege, laufender Befehl, Hinweis, Zeichnung ------------------------------

#[test]
fn erzeugen_menu_extrude_is_remembered_and_space_repeats_it() {
    let (w, sk) = sketch_ws();
    let mut rig = Rig::new(w);
    rig.menu("Erzeugen", &app::CreateKind::Extrusion.label());
    assert_eq!(
        rig.ws().active_tab().dialog,
        Some(CreateDialog::extrude_sketch(sk)),
        "Menüeintrag öffnet das Fenster"
    );
    assert_eq!(rig.last(), None, "erst beim Abschluss");
    rig.click_text("Erzeugen");
    assert!(rig.ws().active_tab().dialog.is_none(), "erzeugt");
    assert_eq!(rig.last(), Some(Action::ExtrudeSketch));
    rig.st().selected_feature = None;
    rig.press(Key::Space, NONE);
    assert_eq!(
        rig.ws().active_tab().dialog,
        Some(CreateDialog::extrude_sketch(sk))
    );
}

#[test]
fn bearbeiten_menu_fillet_is_remembered() {
    let (w, _) = box_ws();
    let mut rig = Rig::new(w);
    rig.menu("Bearbeiten", "Verrunden …");
    assert!(
        rig.ws().active_tab().blend_dialog.is_some(),
        "Fenster offen"
    );
    assert_eq!(rig.last(), None);
    rig.finish_fillet();
    assert_eq!(rig.last(), Some(Action::Fillet));
}

#[test]
fn tree_context_menu_extrude_is_remembered() {
    let (w, sk) = sketch_ws();
    let mut rig = Rig::new(w);
    // Wie „Extrudieren …“ im Kontextmenü der Skizzenzeile (`browser::Action::Extrude`).
    rig.st()
        .request_sketch_dialog(planora_app::state::SketchRequest::Extrude(sk));
    rig.settle();
    assert_eq!(
        rig.ws().active_tab().dialog,
        Some(CreateDialog::extrude_sketch(sk))
    );
    rig.click_text("Erzeugen");
    assert_eq!(rig.last(), Some(Action::ExtrudeSketch));
}

#[test]
fn a_command_without_window_keeps_the_running_one() {
    // Verrunden offen, dann „Skizze extrudieren“ ohne Skizze (nur ein Hinweis): Verrunden läuft
    // weiter und wird nach dem Abschluss gemerkt.
    let (w, _) = box_ws();
    let mut rig = Rig::new(w);
    rig.press(Key::F, NONE);
    commands::run_action(rig.ws(), Action::ExtrudeSketch);
    assert!(rig.ws().active_tab().dialog.is_none());
    rig.settle();
    rig.finish_fillet();
    assert_eq!(rig.last(), Some(Action::Fillet));

    // Ebenso mit einer gesperrten Skizze (Körper zurückgesetzt): `do_create` meldet nur.
    let (mut w, sk) = sketch_ws();
    let s = &mut w.active_tab_mut().state;
    let e = s.extrude_sketch(sk, 0, 2.0).unwrap();
    s.move_body(body_of(s, e), DVec3::X).unwrap();
    let m = s.document.features().last().unwrap().id;
    history_bar::move_marker(s, m, 2).unwrap();
    // Ein zweiter, freier Körper mit gewählter Kante für Verrunden.
    let b = s.create_box(DVec3::splat(20.0)).unwrap();
    let j = body_of(s, b);
    let edge = s.bodies[j].solid.edges.iter().next().unwrap().0;
    s.set_selection(Some(planora_render::Pick::Edge(planora_render::EdgeRef {
        body: j,
        edge,
    })));
    w.observe_history(false);
    let mut rig = Rig::new(w);
    rig.press(Key::F, NONE);
    assert!(rig.ws().active_tab().blend_dialog.is_some());
    rig.st().selected_feature = Some(sk);
    commands::run_action(rig.ws(), Action::ExtrudeSketch);
    assert_eq!(
        rig.st().message.as_deref(),
        Some(history_bar::lock_hint().as_str()),
        "nur eine Meldung"
    );
    assert!(rig.ws().active_tab().dialog.is_none());
    rig.st().message = None;
    rig.settle();
    rig.finish_fillet();
    assert_eq!(rig.last(), Some(Action::Fillet));
}

#[test]
fn a_feature_from_another_command_does_not_complete_the_running_one() {
    // Verrunden offen; nebenher entsteht über das Menü ein Quader; dann Verrunden abbrechen.
    let (w, _) = box_ws();
    let mut rig = Rig::new(w);
    rig.ws().last_command.remember(Action::ExtrudeSketch);
    rig.press(Key::F, NONE);
    let before = rig.features();
    // Wie ein Quader über „Erzeugen“ (dessen Fenster hätte denselben Knopf wie Verrunden).
    rig.st().create_box(DVec3::splat(5.0)).unwrap();
    rig.settle();
    assert_eq!(rig.features(), before + 1, "Quader erzeugt");
    assert!(
        rig.ws().active_tab().blend_dialog.is_some(),
        "Verrunden noch offen"
    );
    rig.ws().active_tab_mut().blend_dialog = None;
    rig.settle();
    assert_eq!(
        rig.last(),
        Some(Action::ExtrudeSketch),
        "abgebrochenes Verrunden zählt nicht"
    );
}

#[test]
fn repeating_an_extrude_without_sketch_shows_the_hint() {
    let (w, _) = box_ws();
    let mut rig = Rig::new(w);
    rig.ws().last_command.remember(Action::ExtrudeSketch);
    rig.press(Key::Space, NONE);
    assert!(rig.ws().active_tab().dialog.is_none());
    assert_eq!(
        rig.st().info.as_deref(),
        Some("Zuerst eine Skizze anlegen.")
    );
}

#[test]
fn model_keys_do_nothing_while_a_drawing_is_shown() {
    let (mut w, b) = box_ws();
    let s = &mut w.active_tab_mut().state;
    let d = s
        .document
        .add_drawing(
            "Blatt 1",
            planora_drawing::Sheet {
                paper: planora_drawing::Paper::A4,
                landscape: true,
            },
            planora_drawing::Projection::First,
        )
        .unwrap();
    s.drawing_ui.open(d);
    w.last_command.remember(Action::Fillet);
    let mut rig = Rig::new(w);
    assert_eq!(
        planora_app::keymap::dispatch::current_area(rig.st()),
        Area::Drawing
    );
    rig.st().selected_body = Some(0);
    for k in [Key::Delete, Key::E, Key::F, Key::V, Key::Space] {
        rig.press(k, NONE);
    }
    rig.settle();
    assert!(rig.st().document.feature(b).is_some(), "Körper bleibt");
    assert!(!rig.st().is_body_hidden(0));
    assert!(!rig.ws().active_tab().dialog_open());
}

#[test]
fn tree_context_menu_and_eye_tooltip_show_the_current_keys() {
    let (mut w, _) = box_ws();
    let sk = {
        let s = &mut w.active_tab_mut().state;
        let sk = s
            .add_sketch(SketchPlane::Standard(StandardPlane::XY))
            .unwrap();
        s.open_sketch(sk).unwrap();
        s.session_sketch_mut()
            .unwrap()
            .1
            .add_rectangle(DVec2::ZERO, DVec2::new(5.0, 10.0))
            .unwrap();
        s.close_sketch();
        sk
    };
    w.observe_history(false);
    let name = w
        .active_tab()
        .state
        .document
        .feature(sk)
        .unwrap()
        .name
        .clone();
    let mut rig = Rig::new(w);
    let row = rig
        .settle()
        .into_iter()
        .filter(|s| s.text == name)
        .map(|s| s.rect.center())
        .next()
        .unwrap_or_else(|| panic!("Zeile „{name}“ nicht zu sehen"));
    let right = |rig: &mut Rig, p: Pos2| {
        rig.frame(vec![Event::PointerMoved(p)]);
        for pressed in [true, false] {
            rig.frame(vec![Event::PointerButton {
                pos: p,
                button: PointerButton::Secondary,
                pressed,
                modifiers: NONE,
            }]);
        }
        rig.frame(vec![])
            .into_iter()
            .map(|s| s.text)
            .collect::<Vec<_>>()
    };
    let texts = right(&mut rig, row);
    assert!(texts.iter().any(|t| t == "Löschen"), "{texts:?}");
    for want in ["Entf", "E"] {
        assert!(texts.iter().any(|t| t == want), "{want}: {texts:?}");
    }
    // Nach dem Umbelegen die neue Taste.
    rig.press(Key::Escape, NONE);
    let mut km = rig.ws().keymap.clone();
    km.set(Action::DeleteSelection, 0, plain(Key::J));
    km.set(Action::ExtrudeSketch, 0, plain(Key::K));
    rig.ws().keymap = km;
    rig.settle();
    let texts = right(&mut rig, row);
    assert!(texts.iter().any(|t| t == "J"), "{texts:?}");
    assert!(texts.iter().any(|t| t == "K"), "{texts:?}");
    assert!(!texts.iter().any(|t| t == "Entf"), "{texts:?}");
}

#[test]
fn eye_tooltip_shows_the_current_key() {
    let (w, _) = box_ws();
    let mut rig = Rig::new(w);
    rig.settle();
    let (_, eye_rect) = browser::eye_rects(&rig.ctx)
        .into_iter()
        .find(|(n, _)| n == "Quader 1")
        .expect("Auge");
    let eye = eye_rect.center();
    let texts = rig.hover(eye);
    assert!(
        texts
            .iter()
            .any(|t| t == "Körper ausblenden (nur in der Ansicht) (V)"),
        "{texts:?}"
    );
    let mut km = rig.ws().keymap.clone();
    km.set(Action::ToggleVisibility, 0, plain(Key::J));
    rig.ws().keymap = km;
    rig.settle();
    let texts = rig.hover(eye);
    assert!(
        texts
            .iter()
            .any(|t| t == "Körper ausblenden (nur in der Ansicht) (J)"),
        "{texts:?}"
    );
}

#[test]
fn area_keys_do_nothing_while_the_projects_window_is_open() {
    // Fenster offen: nichts geschieht, die Tasten bleiben liegen.
    let (w, b) = box_ws();
    let mut rig = Rig::new(w);
    rig.app.windows_mut().1.open();
    rig.settle();
    for k in [Key::V, Key::E, Key::F, Key::Space, Key::Delete] {
        rig.press(k, NONE);
        assert!(rig.left_over, "{k:?} bleibt liegen");
    }
    assert!(!rig.st().is_body_hidden(0), "V blendet nichts aus");
    assert!(
        !rig.ws().active_tab().dialog_open(),
        "kein Fenster dahinter"
    );
    assert!(rig.st().document.feature(b).is_some(), "nichts gelöscht");
    // Dateikürzel („Überall“) gelten weiter.
    rig.press(Key::N, Modifiers::COMMAND);
    assert!(!rig.left_over, "Strg+N wird verbraucht");

    // Fenster zu: dieselben Tasten wirken.
    let (w, _) = box_ws();
    let mut rig = Rig::new(w);
    rig.press(Key::V, NONE);
    assert!(!rig.left_over && rig.st().is_body_hidden(0), "V wirkt");
    let (w, _) = box_ws();
    let mut rig = Rig::new(w);
    rig.press(Key::F, NONE);
    assert!(rig.ws().active_tab().blend_dialog.is_some(), "F wirkt");
    let (w, b) = box_ws();
    let mut rig = Rig::new(w);
    rig.press(Key::Delete, NONE);
    assert!(rig.st().document.feature(b).is_none(), "Entf wirkt");
}
