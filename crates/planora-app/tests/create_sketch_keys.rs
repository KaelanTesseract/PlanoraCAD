//! „Skizze erstellen“ über die Taste und „Letzten Befehl wiederholen“ (Rest aus dem Teilprojekt
//! Skizze erstellen): Die Taste wirkt auch bei offener Skizze (wie der Knopf schließt der Befehl
//! sie erst), nicht in der Zeichnung; ein gelungenes „Skizze erstellen“ merkt sich der
//! Wiederholen-Befehl, mit Vorauswahl und über den Wahlmodus.

use planora_app::commands::{observe, run_action};
use planora_app::keymap::dispatch::{current_area, pressed_in};
use planora_app::keymap::{Action, Area, Binding, KeyChord, Keymap};
use planora_app::plane_dialog::PlaneClick;
use planora_app::state::PendingPick;
use planora_app::workspace::Workspace;
use planora_feature::{FeatureKind, SketchPlane, StandardPlane};
use planora_math::DVec3;
use planora_project::config::Config;

fn ws_with_box() -> Workspace {
    let mut ws = Workspace::new(None, Config::default());
    ws.active_tab_mut()
        .state
        .create_box(DVec3::new(40.0, 30.0, 20.0))
        .unwrap();
    ws
}

fn sketches(ws: &Workspace) -> usize {
    ws.active_tab()
        .state
        .document
        .features()
        .iter()
        .filter(|f| matches!(f.kind, FeatureKind::Sketch(_)))
        .count()
}

fn key_n() -> KeyChord {
    KeyChord {
        key: egui::Key::N,
        ctrl: false,
        shift: false,
        alt: false,
    }
}

fn press_n() -> Vec<egui::Event> {
    vec![egui::Event::Key {
        key: egui::Key::N,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    }]
}

#[test]
fn the_key_works_with_an_open_sketch() {
    let mut km = Keymap::defaults();
    assert!(matches!(
        km.set(Action::CreateSketch, 0, Binding::Key(key_n())),
        planora_app::keymap::SetResult::Ok
    ));
    let mut ws = ws_with_box();
    let st = &mut ws.active_tab_mut().state;
    let id = st
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    st.open_sketch(id).unwrap();
    let area = current_area(&ws.active_tab().state);
    assert_eq!(area, Area::Sketch);
    let mut events = press_n();
    let hit = pressed_in(&mut events, false, &km, area, |_| true);
    assert_eq!(
        hit,
        vec![Action::CreateSketch],
        "die Taste gilt auch in der Skizze"
    );
    run_action(&mut ws, Action::CreateSketch);
    let st = &ws.active_tab().state;
    assert!(st.sketch_session.is_none(), "die offene Skizze ist zu");
    assert_eq!(st.pending_pick, Some(PendingPick::NewSketch));
}

#[test]
fn repeat_last_knows_create_sketch() {
    // Mit Vorauswahl: sofort gemerkt.
    let mut ws = ws_with_box();
    ws.active_tab_mut()
        .state
        .select_origin_plane(StandardPlane::XZ);
    run_action(&mut ws, Action::CreateSketch);
    observe(&mut ws);
    assert_eq!(sketches(&ws), 1);
    assert_eq!(ws.last_command.last(), Some(Action::CreateSketch));

    // Über den Wahlmodus: gemerkt, sobald die Ebene gewählt ist; abgebrochen: nicht.
    let mut ws = ws_with_box();
    run_action(&mut ws, Action::CreateSketch);
    observe(&mut ws);
    assert_eq!(ws.last_command.last(), None, "noch nichts entstanden");
    ws.active_tab_mut().state.cancel_pending_pick();
    observe(&mut ws);
    assert_eq!(ws.last_command.last(), None, "abgebrochen");
    run_action(&mut ws, Action::CreateSketch);
    observe(&mut ws);
    ws.active_tab_mut()
        .state
        .pick_sketch_plane(PlaneClick::Standard(StandardPlane::XY));
    observe(&mut ws);
    assert_eq!(sketches(&ws), 1);
    assert_eq!(ws.last_command.last(), Some(Action::CreateSketch));
}
