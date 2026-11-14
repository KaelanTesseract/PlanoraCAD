//! Tastatur in abgedockten Fenstern (Review Focus 3): Tastenkürzel aus abgedockten Feldern und
//! Dialogen wirken genau einmal, Esc und Enter gehören dem Dialog, Esc im Hauptfenster bricht
//! einen abgedockten Befehlsdialog ab, Tippen in einem Feld löst keine Kürzel aus. Die ganze App
//! mit echten Kindfenstern (`Screen::with_viewports`).

mod common;

use common::{Screen, Seen, find, join};
use egui::{Event, Key, Modifiers, PointerButton, Pos2, Vec2, pos2, vec2};
use planora_app::app::{self, PlanoraApp};
use planora_app::commands;
use planora_app::detach::{self, Place};
use planora_app::keymap::Action;
use planora_app::plane_dialog::PlaneCommand;
use planora_app::sketch_state::{Item, Tool};
use planora_app::viewport;
use planora_app::workspace::{Prompt, Workspace};
use planora_app::{fonts, theme};
use planora_feature::{SketchPlane, StandardPlane};
use planora_math::DVec3;
use planora_project::config::{Config, DetachedWindow};
use planora_project::library::Library;
use planora_render::Pick;

const SIZE: Vec2 = vec2(1920.0, 1080.0);

/// Lage auf dem Hauptmonitor (unter Windows auf einem vorhandenen Monitor, sonst bliebe das
/// Fenster angedockt).
fn window_at(x: f32, y: f32) -> DetachedWindow {
    DetachedWindow {
        x,
        y,
        width: 420.0,
        height: 600.0,
    }
}

fn key(k: Key, modifiers: Modifiers) -> Event {
    Event::Key {
        key: k,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers,
    }
}

/// Die ganze App im Harness mit Kindfenstern.
struct Rig {
    screen: Screen,
    app: PlanoraApp,
    /// Ein Textfeld im Hauptfenster (vor der App gezeichnet), etwa für den Fokus dort.
    main_field: Option<String>,
}

/// Kennung des Textfelds [`Rig::main_field`].
fn main_field_id() -> egui::Id {
    egui::Id::new("detach_keys_hauptfeld")
}

impl Rig {
    /// App mit zwei Quadern (zwei Schritte im Verlauf); `detached` liegt im eigenen Fenster.
    fn new(detached: &[(Place, DetachedWindow)]) -> Rig {
        let screen = Screen::with_viewports(SIZE.x, SIZE.y);
        screen.ctx.set_fonts(fonts::definitions());
        theme::apply(&screen.ctx, true);
        PlanoraApp::setup_context(&screen.ctx);
        let root = common::temp_root("planora-detach-keys");
        let mut ws = Workspace::new(Some(Library::open(&root).unwrap()), Config::default());
        // Nie die echte Konfiguration schreiben.
        ws.config_path = root.join("config.json");
        ws.active_tab_mut()
            .state
            .create_box(DVec3::new(100.0, 60.0, 40.0))
            .unwrap();
        ws.observe_history(false);
        ws.active_tab_mut()
            .state
            .create_box(DVec3::new(20.0, 20.0, 20.0))
            .unwrap();
        ws.observe_history(false);
        for (p, w) in detached {
            ws.detach.set(*p, *w);
        }
        let mut rig = Rig {
            screen,
            app: PlanoraApp::for_tests(ws),
            main_field: None,
        };
        rig.settle(4);
        rig
    }

    fn ws(&mut self) -> &mut Workspace {
        self.app.ws_mut()
    }

    fn state(&mut self) -> &mut planora_app::state::AppState {
        &mut self.app.ws_mut().active_tab_mut().state
    }

    fn bodies(&mut self) -> usize {
        self.state().bodies.len()
    }

    fn features(&mut self) -> usize {
        self.state().document.features().len()
    }

    fn frame(&mut self, events: Vec<Event>) -> Vec<Seen> {
        let app = &mut self.app;
        let field = &mut self.main_field;
        self.screen.frame_ui(events, &mut |ui| {
            if let Some(text) = field.as_mut() {
                egui::Area::new(egui::Id::new("detach_keys_hauptfeld_flaeche"))
                    .fixed_pos(pos2(900.0, 500.0))
                    .show(ui.ctx(), |ui| {
                        ui.add(egui::TextEdit::singleline(text).id(main_field_id()));
                    });
            }
            let mut f = eframe::Frame::_new_kittest();
            eframe::App::ui(app, ui, &mut f);
        })
    }

    fn settle(&mut self, n: usize) -> Vec<Seen> {
        let mut seen = Vec::new();
        for _ in 0..n {
            seen = self.frame(vec![]);
        }
        seen
    }

    /// Ein Bild mit Ereignissen im Kindfenster `id`, danach zwei ruhige Bilder.
    fn press_in(&mut self, id: egui::ViewportId, events: Vec<Event>) {
        assert!(self.screen.child_runs(id) > 0, "Kindfenster {id:?} fehlt");
        self.screen.queue_child(id, events);
        self.frame(vec![]);
        self.settle(2);
    }

    /// Klick mit `button` im Kindfenster `id` an `at`, danach zwei Bilder.
    fn click_child(&mut self, id: egui::ViewportId, at: Pos2, button: PointerButton) {
        let press = |pressed| Event::PointerButton {
            pos: at,
            button,
            pressed,
            modifiers: Modifiers::NONE,
        };
        for e in [Event::PointerMoved(at), press(true), press(false)] {
            self.screen.queue_child(id, vec![e]);
            self.frame(vec![]);
        }
        self.settle(2);
    }

    /// Öffnet das Fenster „Versatzebene“ (abgedockt, wenn sein Platz gemerkt ist).
    fn open_plane_offset(&mut self) {
        commands::run_action(self.ws(), Action::PlaneOffset);
        self.settle(3);
        assert!(self.ws().active_tab().plane_dialog.is_some());
    }
}

fn center_of(seen: &[Seen], text: &str) -> Pos2 {
    find(seen, text)
        .unwrap_or_else(|| panic!("„{text}“ fehlt: {}", join(seen)))
        .rect
        .center()
}

fn plane_offset() -> Place {
    Place::Dialog(PlaneCommand::Offset.dock_key())
}

#[test]
fn ctrl_z_in_undocked_tree_undoes_once() {
    let mut rig = Rig::new(&[(Place::Tree, window_at(100.0, 100.0))]);
    assert_eq!(rig.bodies(), 2);
    let tree = detach::viewport_id(Place::Tree);
    rig.press_in(tree, vec![key(Key::Z, Modifiers::COMMAND)]);
    assert_eq!(
        rig.bodies(),
        1,
        "Strg+Z im Baumfenster: genau ein Schritt zurück"
    );
    rig.press_in(tree, vec![key(Key::Y, Modifiers::COMMAND)]);
    assert_eq!(
        rig.bodies(),
        2,
        "Strg+Y im Baumfenster: genau ein Schritt vor"
    );
    // Strg+Z im Hauptfenster wirkt weiter (genau einmal).
    rig.frame(vec![key(Key::Z, Modifiers::COMMAND)]);
    rig.settle(2);
    assert_eq!(rig.bodies(), 1);
}

#[test]
fn delete_in_undocked_tree_deletes_selected_feature() {
    let mut rig = Rig::new(&[(Place::Tree, window_at(100.0, 100.0))]);
    let features = rig.features();
    let last = rig.state().document.features().last().unwrap().id;
    rig.state().selected_feature = Some(last);
    rig.settle(1);
    let tree = detach::viewport_id(Place::Tree);
    rig.press_in(tree, vec![key(Key::Delete, Modifiers::NONE)]);
    assert_eq!(
        rig.features(),
        features - 1,
        "Entf im Baumfenster löscht genau eins"
    );
    assert!(rig.state().document.feature(last).is_none());
    assert_eq!(rig.bodies(), 1);
}

#[test]
fn s_in_undocked_tree_opens_overview() {
    let mut rig = Rig::new(&[(Place::Tree, window_at(100.0, 100.0))]);
    let tree = detach::viewport_id(Place::Tree);
    rig.press_in(
        tree,
        vec![key(Key::S, Modifiers::NONE), Event::Text("s".to_string())],
    );
    assert!(rig.app.overview_mut().open, "S im Baumfenster: Übersicht");
    assert_eq!(
        rig.app.overview_mut().filter,
        "",
        "der Text der Taste landet nicht im Suchfeld"
    );
}

#[test]
fn ctrl_s_in_undocked_dialog_saves_once() {
    let mut rig = Rig::new(&[(plane_offset(), window_at(200.0, 100.0))]);
    rig.open_plane_offset();
    let child = detach::viewport_id(plane_offset());
    rig.press_in(child, vec![key(Key::S, Modifiers::COMMAND)]);
    assert!(
        matches!(rig.ws().prompt, Some(Prompt::SaveAs { .. })),
        "Strg+S im abgedockten Dialog: Speichern"
    );
    rig.ws().prompt = None;
    rig.settle(2);
    // Genau einmal: Strg+N legt genau einen Tab an.
    assert_eq!(rig.ws().tabs.len(), 1);
    rig.press_in(child, vec![key(Key::N, Modifiers::COMMAND)]);
    assert_eq!(
        rig.ws().tabs.len(),
        2,
        "Strg+N im abgedockten Dialog: ein Tab"
    );
}

#[test]
fn model_keys_blocked_in_undocked_dialog() {
    let fillet = Place::Dialog("fillet");
    let mut rig = Rig::new(&[(fillet, window_at(200.0, 100.0))]);
    let features = rig.features();
    let last = rig.state().document.features().last().unwrap().id;
    rig.state().selected_feature = Some(last);
    app::open_blend(rig.ws(), false);
    rig.settle(3);
    assert!(rig.ws().active_tab().blend_dialog.is_some());
    let child = detach::viewport_id(fillet);
    rig.press_in(
        child,
        vec![
            key(Key::Delete, Modifiers::NONE),
            key(Key::S, Modifiers::NONE),
        ],
    );
    assert_eq!(rig.features(), features, "Entf im Dialog löscht nichts");
    assert!(!rig.app.overview_mut().open, "S im Dialog öffnet nichts");
    assert!(
        rig.ws().active_tab().blend_dialog.is_some(),
        "Dialog bleibt"
    );
    // Rückgängig ist wie im Hauptfenster gesperrt, solange der Dialog offen ist.
    rig.press_in(child, vec![key(Key::Z, Modifiers::COMMAND)]);
    assert_eq!(rig.bodies(), 2);
}

#[test]
fn escape_in_undocked_dialog_closes_it() {
    // Befehlsdialog (Esc liest der Dialog selbst).
    let mut rig = Rig::new(&[(plane_offset(), window_at(200.0, 100.0))]);
    rig.open_plane_offset();
    let child = detach::viewport_id(plane_offset());
    rig.press_in(child, vec![key(Key::Escape, Modifiers::NONE)]);
    assert!(
        rig.ws().active_tab().plane_dialog.is_none(),
        "Esc im abgedockten Dialog bricht ab"
    );

    // Übersicht „Tastenkürzel“ (Esc las sie bisher vor den Feldern im Hauptfenster), auch mit
    // dem Suchfeld in der Hand.
    let shortcuts = Place::Dialog("shortcuts");
    let mut rig = Rig::new(&[(shortcuts, window_at(200.0, 100.0))]);
    rig.app.overview_mut().open_focused();
    rig.settle(3);
    let child = detach::viewport_id(shortcuts);
    assert!(rig.screen.child_runs(child) > 0, "abgedockt");
    // Die Übersicht ist kein Befehl: Esc im Hauptfenster bleibt dem Hauptfenster.
    rig.frame(vec![key(Key::Escape, Modifiers::NONE)]);
    rig.settle(2);
    assert!(rig.app.overview_mut().open, "Esc im Hauptfenster");
    rig.press_in(child, vec![key(Key::Escape, Modifiers::NONE)]);
    assert!(!rig.app.overview_mut().open, "Esc schließt die Übersicht");
}

#[test]
fn enter_in_undocked_dialog_applies() {
    let rename = Place::Dialog("tree_rename");
    let mut rig = Rig::new(&[(rename, window_at(200.0, 100.0))]);
    let first = rig.state().document.features()[0].id;
    rig.state().rename_draft = Some((first, "Neu".to_string()));
    rig.settle(3);
    let child = detach::viewport_id(rename);
    let seen = rig.screen.child_seen(child);
    let field = center_of(&seen, "Neu");
    rig.click_child(child, field, PointerButton::Primary);
    // Enter im Hauptfenster bestätigt nicht.
    rig.frame(vec![key(Key::Enter, Modifiers::NONE)]);
    rig.settle(2);
    assert!(rig.state().rename_draft.is_some(), "Enter im Hauptfenster");
    rig.press_in(child, vec![key(Key::Enter, Modifiers::NONE)]);
    assert!(rig.state().rename_draft.is_none(), "Enter übernimmt");
    assert_eq!(rig.state().document.feature(first).unwrap().name, "Neu");
}

#[test]
fn escape_in_main_cancels_undocked_command_dialog() {
    let mut rig = Rig::new(&[(plane_offset(), window_at(200.0, 100.0))]);
    rig.open_plane_offset();
    // Enter im Hauptfenster bestätigt nicht.
    rig.frame(vec![key(Key::Enter, Modifiers::NONE)]);
    rig.settle(2);
    assert!(rig.ws().active_tab().plane_dialog.is_some());
    let features = rig.features();
    rig.frame(vec![key(Key::Escape, Modifiers::NONE)]);
    rig.settle(2);
    assert!(
        rig.ws().active_tab().plane_dialog.is_none(),
        "Esc im Hauptfenster bricht den abgedockten Befehl ab"
    );
    assert_eq!(rig.features(), features, "nichts angelegt");
}

#[test]
fn typing_in_undocked_field_does_not_trigger_shortcuts() {
    let mut rig = Rig::new(&[(Place::Tree, window_at(300.0, 100.0))]);
    let tree = detach::viewport_id(Place::Tree);
    let host = egui::Rect::from_min_size(pos2(300.0, 100.0), vec2(320.0, 700.0));
    rig.screen.child_info(tree, |i| {
        i.outer_rect = Some(host);
        i.inner_rect = Some(host);
    });
    rig.settle(2);
    // Umbenennen aus dem Kontextmenü des Baums: Unterdialog über dem Baumfenster.
    let row = center_of(&rig.screen.child_seen(tree), "Quader 1");
    rig.click_child(tree, row, PointerButton::Secondary);
    let entry = center_of(&rig.screen.child_seen(tree), "Umbenennen");
    rig.click_child(tree, entry, PointerButton::Primary);
    rig.settle(2);
    let sub = detach::sub_viewport_id(Place::Tree, egui::Id::new("Umbenennen"));
    let seen = rig.screen.child_seen(sub);
    let field = center_of(&seen, "Quader 1");
    rig.click_child(sub, field, PointerButton::Primary);
    let tabs = rig.ws().tabs.len();
    rig.press_in(
        sub,
        vec![
            key(Key::L, Modifiers::NONE),
            Event::Text("l".to_string()),
            key(Key::N, Modifiers::COMMAND),
            key(Key::Delete, Modifiers::NONE),
        ],
    );
    let draft = rig.state().rename_draft.clone().expect("Umbenennen offen");
    assert!(draft.1.contains('l'), "getippt: {:?}", draft.1);
    assert_eq!(rig.ws().tabs.len(), tabs, "Strg+N im Textfeld: kein Tab");
    assert!(rig.state().sketch_session.is_none(), "kein Werkzeug");
    assert_eq!(rig.features(), 2, "Entf im Textfeld löscht kein Feature");

    // Ohne Textfeld im Baumfenster wirkt dasselbe Strg+N.
    rig.state().rename_draft = None;
    rig.settle(3);
    rig.press_in(tree, vec![key(Key::N, Modifiers::COMMAND)]);
    assert_eq!(rig.ws().tabs.len(), tabs + 1);
}

/// Neue Skizze auf XY, offen; das abgedockte Skizzenfeld erscheint.
fn open_sketch(rig: &mut Rig) -> egui::ViewportId {
    let s = rig.state();
    let id = s
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    s.open_sketch(id).unwrap();
    rig.settle(3);
    let side = detach::viewport_id(Place::SketchSide);
    assert!(rig.screen.child_runs(side) > 0, "Skizzenfeld abgedockt");
    side
}

/// Eine Linie in der offenen Skizze.
fn add_line(rig: &mut Rig) -> planora_sketch::model::CurveId {
    let (_, sk) = rig.state().session_sketch_mut().unwrap();
    let a = sk.add_point(-10.0, 0.0).unwrap();
    let b = sk.add_point(10.0, 0.0).unwrap();
    sk.add_line(a, b).unwrap()
}

fn tool(rig: &mut Rig) -> Tool {
    rig.state().sketch_session.as_ref().unwrap().tool
}

#[test]
fn l_in_undocked_sketch_side_starts_line_tool() {
    let mut rig = Rig::new(&[(Place::SketchSide, window_at(1500.0, 100.0))]);
    let side = open_sketch(&mut rig);
    assert_eq!(tool(&mut rig), Tool::Select);
    rig.press_in(
        side,
        vec![key(Key::L, Modifiers::NONE), Event::Text("l".to_string())],
    );
    assert_eq!(tool(&mut rig), Tool::Line, "L im Skizzenfeld: Linie");
}

#[test]
fn escape_in_undocked_sketch_side_cancels_tool() {
    let mut rig = Rig::new(&[(Place::SketchSide, window_at(1500.0, 100.0))]);
    let side = open_sketch(&mut rig);
    // Bemaßen ohne offenes Maß: Esc beendet das Werkzeug (wie im Hauptfenster).
    {
        let (se, sk) = rig.state().session_sketch_mut().unwrap();
        se.set_tool(sk, Tool::Dimension);
    }
    rig.settle(2);
    rig.press_in(side, vec![key(Key::Escape, Modifiers::NONE)]);
    assert_eq!(tool(&mut rig), Tool::Select, "Esc im Skizzenfeld");
    // Auswahlwerkzeug: Esc leert die Auswahl, genau einmal (die Skizze bleibt offen).
    let curve = add_line(&mut rig);
    rig.state().sketch_session.as_mut().unwrap().selection = vec![Item::Curve(curve)];
    rig.settle(2);
    rig.press_in(side, vec![key(Key::Escape, Modifiers::NONE)]);
    assert!(
        rig.state()
            .sketch_session
            .as_ref()
            .unwrap()
            .selection
            .is_empty()
    );
    assert!(rig.state().sketch_session.is_some(), "Skizze bleibt offen");
}

#[test]
fn delete_in_undocked_sketch_side_deletes_selection() {
    let mut rig = Rig::new(&[(Place::SketchSide, window_at(1500.0, 100.0))]);
    let side = open_sketch(&mut rig);
    let curve = add_line(&mut rig);
    let other = add_line(&mut rig);
    rig.state().sketch_session.as_mut().unwrap().selection = vec![Item::Curve(curve)];
    rig.settle(2);
    let bodies = rig.bodies();
    rig.press_in(side, vec![key(Key::Delete, Modifiers::NONE)]);
    let (_, sk) = rig.state().session_sketch_mut().unwrap();
    assert!(sk.curve(curve).is_none(), "Entf löscht die Auswahl");
    assert!(sk.curve(other).is_some(), "nur sie");
    assert_eq!(rig.bodies(), bodies, "Körper bleiben");
}

#[test]
fn typing_in_undocked_sketch_value_field_triggers_nothing() {
    let mut rig = Rig::new(&[(Place::SketchSide, window_at(1500.0, 100.0))]);
    let side = open_sketch(&mut rig);
    let curve = add_line(&mut rig);
    rig.state().sketch_session.as_mut().unwrap().selection = vec![Item::Curve(curve)];
    rig.settle(2);
    // Abschnitt „Lineares Muster“ mit seinen Wertfeldern.
    let button = center_of(&rig.screen.child_seen(side), "Lineares Muster …");
    rig.click_child(side, button, PointerButton::Primary);
    let seen = rig.screen.child_seen(side);
    let label = find(&seen, "Versatz x")
        .unwrap_or_else(|| panic!("Versatz x fehlt: {}", join(&seen)))
        .rect;
    let field = pos2(label.right() + 30.0, label.center().y);
    rig.click_child(side, field, PointerButton::Primary);
    rig.press_in(side, vec![]);
    // Der Fokus liegt im Wertfeld des Kindfensters.
    assert!(
        viewport::typing_for_history(&rig.screen.ctx),
        "Wertfeld hat die Tastatur"
    );
    let curves = rig.state().session_sketch_mut().unwrap().1.curves().count();
    rig.press_in(
        side,
        vec![
            key(Key::L, Modifiers::NONE),
            Event::Text("l".to_string()),
            key(Key::Delete, Modifiers::NONE),
            key(Key::Escape, Modifiers::NONE),
        ],
    );
    assert_eq!(tool(&mut rig), Tool::Select, "L im Wertfeld: kein Werkzeug");
    assert_eq!(
        rig.state().session_sketch_mut().unwrap().1.curves().count(),
        curves,
        "Entf im Wertfeld löscht nichts"
    );
    assert_eq!(
        rig.state().sketch_session.as_ref().unwrap().selection,
        vec![Item::Curve(curve)],
        "Esc im Wertfeld lässt die Auswahl"
    );
    assert!(rig.state().sketch_session.is_some());
}

#[test]
fn escape_in_undocked_tree_clears_selection() {
    let mut rig = Rig::new(&[(Place::Tree, window_at(100.0, 100.0))]);
    let first = rig.state().document.features()[0].id;
    rig.state().selection = Some(Pick::Plane(first));
    rig.settle(2);
    assert!(rig.state().selection.is_some());
    let tree = detach::viewport_id(Place::Tree);
    rig.press_in(tree, vec![key(Key::Escape, Modifiers::NONE)]);
    assert!(
        rig.state().selection.is_none(),
        "Esc im Baumfenster hebt die Auswahl auf"
    );
}

#[test]
fn typing_in_undocked_field_counts_for_history() {
    let rename = Place::Dialog("tree_rename");
    let mut rig = Rig::new(&[(rename, window_at(200.0, 100.0))]);
    let first = rig.state().document.features()[0].id;
    rig.state().rename_draft = Some((first, "Neu".to_string()));
    rig.settle(3);
    assert!(!viewport::typing_for_history(&rig.screen.ctx));
    let child = detach::viewport_id(rename);
    let field = center_of(&rig.screen.child_seen(child), "Neu");
    rig.click_child(child, field, PointerButton::Primary);
    assert!(
        viewport::typing_for_history(&rig.screen.ctx),
        "Tippen im abgedockten Feld zählt für den Verlauf"
    );
    rig.state().rename_draft = None;
    rig.settle(3);
    assert!(!viewport::typing_for_history(&rig.screen.ctx), "Fenster zu");
}

#[test]
fn keys_from_undocked_field_reach_view_when_main_field_had_focus() {
    let mut rig = Rig::new(&[(Place::SketchSide, window_at(1500.0, 100.0))]);
    let side = open_sketch(&mut rig);
    // Ein Feld im Hauptfenster hat die Tastatur (etwa das Feld am Zeiger beim Zeichnen).
    rig.main_field = Some(String::new());
    rig.settle(1);
    rig.screen
        .ctx
        .memory_mut(|m| m.request_focus(main_field_id()));
    rig.settle(2);
    assert_eq!(
        rig.screen.ctx.memory(|m| m.focused()),
        Some(main_field_id()),
        "Feld im Hauptfenster hat den Fokus"
    );
    {
        let (se, sk) = rig.state().session_sketch_mut().unwrap();
        se.set_tool(sk, Tool::Dimension);
    }
    rig.settle(2);
    // Esc im abgedockten Skizzenfeld: das Hauptfenster gibt den Fokus ab, die Skizze sieht Esc.
    rig.press_in(side, vec![key(Key::Escape, Modifiers::NONE)]);
    assert_eq!(tool(&mut rig), Tool::Select, "Esc kommt in der Skizze an");
    assert_ne!(
        rig.screen.ctx.memory(|m| m.focused()),
        Some(main_field_id()),
        "Fokus des Hauptfensters abgegeben"
    );
    assert_eq!(
        rig.main_field.as_deref(),
        Some(""),
        "nichts ins Feld getippt"
    );
}

#[test]
fn delete_key_in_undocked_tree_opens_prompt_over_tree_window() {
    // Review I1: Entf im abgedockten Baum auf eine Skizze, an der eine Extrusion hängt, öffnet
    // die Rückfrage „Löschen mit Folgen“ als Unterdialog über dem Baumfenster, nicht im
    // Hauptfenster.
    let mut rig = Rig::new(&[(Place::Tree, window_at(100.0, 100.0))]);
    let s = rig.state();
    let sk = s
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    s.open_sketch(sk).unwrap();
    s.session_sketch_mut()
        .unwrap()
        .1
        .add_rectangle(
            planora_math::DVec2::ZERO,
            planora_math::DVec2::new(5.0, 10.0),
        )
        .unwrap();
    s.close_sketch();
    s.extrude_sketch(sk, 0, 2.0).unwrap();
    s.selected_feature = Some(sk);
    rig.ws().observe_history(false);
    rig.settle(2);
    let tree = detach::viewport_id(Place::Tree);
    rig.press_in(tree, vec![key(Key::Delete, Modifiers::NONE)]);
    assert!(rig.state().pending_delete.is_some(), "Rückfrage offen");
    assert!(
        rig.state().document.feature(sk).is_some(),
        "noch nichts gelöscht"
    );
    let main = rig.settle(3);
    let title = planora_i18n::tr!("tree-confirm-title");
    let sub = detach::sub_viewport_id(Place::Tree, egui::Id::new(title.as_str()));
    assert!(
        rig.screen.child_runs(sub) > 0,
        "Unterdialog über dem Baumfenster"
    );
    let seen = rig.screen.child_seen(sub);
    assert!(find(&seen, &title).is_some(), "{}", join(&seen));
    assert!(
        find(&main, &title).is_none(),
        "nicht im Hauptfenster: {}",
        join(&main)
    );
    assert!(
        detach::sub_dialog_open(&rig.screen.ctx, Place::Tree),
        "Baumfenster gesperrt"
    );
}

#[test]
fn switching_blend_kind_keeps_undocked_window() {
    // Review M1: Wechsel Rundung → Fase im abgedockten Fenster „Verrunden“ lässt es, wo es ist
    // (gleicher Platz, gleiches Fenster), statt ins Hauptfenster zu springen.
    use planora_app::blend_dialog::ArtChoice;
    let fillet = Place::Dialog("fillet");
    let mut rig = Rig::new(&[(fillet, window_at(200.0, 100.0))]);
    let last = rig.state().document.features().last().unwrap().id;
    rig.state().selected_feature = Some(last);
    app::open_blend(rig.ws(), false);
    rig.settle(3);
    let child = detach::viewport_id(fillet);
    let chamfer = detach::viewport_id(Place::Dialog("chamfer"));
    let dialog = rig.ws().active_tab_mut().blend_dialog.as_mut().unwrap();
    let (key, id) = (dialog.dock_key(), dialog.window_id());
    dialog.set_art(ArtChoice::FaseGleich);
    assert_eq!(dialog.dock_key(), key, "Platz bleibt");
    assert_eq!(dialog.window_id(), id, "Kennung bleibt");
    let runs = rig.screen.child_runs(child);
    rig.settle(3);
    assert!(
        rig.screen.child_runs(child) > runs,
        "weiter im eigenen Fenster"
    );
    assert_eq!(rig.screen.child_runs(chamfer), 0, "kein Fenster der Fase");
    let seen = rig.screen.child_seen(child);
    let title = planora_i18n::tr!("blend-chamfer-title");
    assert!(
        find(&seen, &title).is_some(),
        "Titel folgt der Art: {}",
        join(&seen)
    );
}

#[test]
fn exit_saves_last_detach_state() {
    // Review M2 / Spec „Speichern“: beim Beenden einmal den letzten Stand.
    let mut rig = Rig::new(&[]);
    let path = rig.ws().config_path.clone();
    rig.ws().detach.set(Place::Tree, window_at(100.0, 100.0));
    eframe::App::on_exit(&mut rig.app);
    let c = Config::load_from(&path);
    assert_eq!(
        c.detached.get("tree").copied(),
        Some(window_at(100.0, 100.0))
    );
}

#[test]
fn replacing_unreadable_config_keeps_current_detach_state() {
    // Review M5: Ist config.json unlesbar, ersetzt `try_persist_config` sie mit dem Stand im
    // Speicher; die abgedockten Fenster dabei mit dem aktuellen Stand, nicht dem vom Start.
    let root = common::temp_root("planora-detach-config");
    let mut ws = Workspace::new(Some(Library::open(&root).unwrap()), Config::default());
    ws.config_path = root.join("config.json");
    std::fs::write(&ws.config_path, b"{ kaputt").unwrap();
    ws.detach.set(Place::Tree, window_at(100.0, 100.0));
    assert!(ws.try_persist_config());
    let c = Config::load_from(&ws.config_path);
    assert_eq!(
        c.detached.get("tree").copied(),
        Some(window_at(100.0, 100.0)),
        "außerhalb eines Bilds"
    );

    // Während eines Bilds (Zustand im egui-Kontext): das Ende des Bilds schreibt ihn nach.
    std::fs::write(&ws.config_path, b"{ kaputt").unwrap();
    ws.detach.set(Place::Right, window_at(1500.0, 100.0));
    ws.detach.save_if_dirty(&root.join("anderswo.json"));
    let ctx = egui::Context::default();
    detach::install(&ctx, &mut ws.detach);
    assert!(ws.try_persist_config());
    detach::uninstall(&ctx, &mut ws.detach);
    ws.detach.save_if_dirty(&ws.config_path);
    let c = Config::load_from(&ws.config_path);
    assert_eq!(
        c.detached.get("right").copied(),
        Some(window_at(1500.0, 100.0)),
        "im Bild"
    );
    assert_eq!(c.detached.len(), 2);
}
