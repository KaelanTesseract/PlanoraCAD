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

#[test]
fn create_sketch_closes_the_insert_windows() {
    // Rest aus dem Teilprojekt: DXF-, SVG- und Leinwand-Fenster wählen selbst Ebenen; mit dem
    // Wahlmodus von „Skizze erstellen“ gehen sie zu (wie die Ebenen-Fenster), ohne Rest.
    for open in [Action::InsertDxf, Action::InsertSvg, Action::InsertCanvas] {
        let mut ws = ws_with_box();
        run_action(&mut ws, open);
        let tab = ws.active_tab();
        assert!(
            tab.dxf_dialog.is_some() || tab.svg_dialog.is_some() || tab.canvas_dialog.is_some(),
            "{open:?} offen"
        );
        run_action(&mut ws, Action::CreateSketch);
        let tab = ws.active_tab();
        assert_eq!(
            tab.state.pending_pick,
            Some(PendingPick::NewSketch),
            "{open:?}"
        );
        assert!(tab.dxf_dialog.is_none(), "{open:?}");
        assert!(tab.svg_dialog.is_none(), "{open:?}");
        assert!(tab.canvas_dialog.is_none(), "{open:?}");
        assert!(tab.state.canvas_preview.is_none(), "{open:?}");
        assert!(tab.state.insert_grips.is_none(), "{open:?}");
    }
}

#[test]
fn switching_to_a_drawing_ends_the_pick() {
    // Rest aus dem Teilprojekt: In der Zeichnung blieb der Hinweis „Ebene oder ebene Fläche
    // wählen“ in der Statuszeile stehen. Der Wahlmodus gehört zur 3D-Ansicht und endet mit dem
    // Wechsel, ohne Rest.
    let mut ws = ws_with_box();
    let d = ws
        .active_tab_mut()
        .state
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
    run_action(&mut ws, Action::CreateSketch);
    assert_eq!(
        ws.active_tab().state.pending_pick,
        Some(PendingPick::NewSketch)
    );
    let before = ws.active_tab().state.document.features().len();
    ws.active_tab_mut().state.drawing_ui.show_drawing(d);
    observe(&mut ws);
    let st = &ws.active_tab().state;
    assert_eq!(st.pending_pick, None);
    assert_eq!(st.document.features().len(), before, "nichts angelegt");
    // Zurück im Modell: kein Wahlmodus mehr.
    ws.active_tab_mut().state.drawing_ui.area = planora_app::drawing_state::WorkArea::Model;
    observe(&mut ws);
    assert_eq!(ws.active_tab().state.pending_pick, None);
}

#[test]
fn undo_and_redo_rest_while_a_pick_runs() {
    // Fusion: Solange ein Befehl läuft, ruht der Verlauf. Strg+Z beendete den Wahlmodus sonst
    // still und nahm dabei noch den letzten Schritt zurück.
    let mut ws = ws_with_box();
    ws.observe_history(false);
    let sketch = ws
        .active_tab_mut()
        .state
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    ws.observe_history(false);
    let features = ws.active_tab().state.document.features().len();
    for pick in [PendingPick::NewSketch, PendingPick::SketchPlane(sketch)] {
        ws.active_tab_mut().state.pending_pick = Some(pick);
        let tab = ws.active_tab_mut();
        let why = planora_app::workspace::pick_running();
        assert_eq!(tab.undo_blocked(), Some(why.clone()), "{pick:?}");
        assert_eq!(tab.redo_blocked(), Some(why.clone()), "{pick:?}");
        assert_eq!(tab.undo(), Err(why.clone()), "{pick:?}");
        assert_eq!(tab.redo(), Err(why), "{pick:?}");
        let st = &ws.active_tab().state;
        assert_eq!(st.pending_pick, Some(pick), "der Modus läuft weiter");
        assert_eq!(st.document.features().len(), features, "nichts zurück");
        ws.active_tab_mut().state.cancel_pending_pick();
    }
    // Nach Esc geht Rückgängig wieder.
    assert_eq!(ws.active_tab_mut().undo(), Ok(()));
    assert_eq!(
        ws.active_tab().state.document.features().len(),
        features - 1
    );
}
