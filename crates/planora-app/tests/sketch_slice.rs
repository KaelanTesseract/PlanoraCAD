//! Skizzen 12, Task 6: Slice an der Skizzenebene. Nur bei geöffneter Skizze mit Haken, die
//! Kameraseite fällt weg; Anklicken trifft erst hinter der Ebene; ein Umschalten ist ein Schritt.

use glam::{Vec2, Vec3};
use planora_app::commands;
use planora_app::keymap::Action;
use planora_app::state::{AppState, PickFilter, StandardPlane};
use planora_app::workspace::Workspace;
use planora_feature::planes::{PlaneBase, PlaneDef};
use planora_feature::{FeatureId, SketchPlane};
use planora_math::DVec3;
use planora_project::config::Config;
use planora_render::{Pick, slice_cuts};

const VIEW: Vec2 = Vec2::new(1920.0, 1080.0);

/// Quader 10 × 20 × 5 im Ursprung (z 0..5) und eine Skizze auf der Ebene z = 2,5 mitten durch.
fn box_with_mid_sketch(s: &mut AppState) -> FeatureId {
    s.create_box(DVec3::new(10.0, 20.0, 5.0)).unwrap();
    let plane = s
        .add_plane(PlaneDef::Offset {
            base: PlaneBase::Standard(StandardPlane::XY),
            distance: 2.5,
        })
        .unwrap();
    s.add_sketch(SketchPlane::OnPlane(plane)).unwrap()
}

fn set_slice(s: &mut AppState, id: FeatureId, on: bool) {
    let mut d = s.document.sketch_display(id);
    d.slice = on;
    s.document.set_sketch_display(id, d);
}

/// Blick fast senkrecht von oben auf die Mitte des Quaders.
fn look_from_above(s: &mut AppState) {
    s.camera.target = Vec3::new(5.0, 10.0, 2.5);
    s.camera.distance = 100.0;
    s.camera.yaw = 0.3;
    s.camera.pitch = 1.3;
}

#[test]
fn slice_only_while_sketch_open() {
    let mut s = AppState::new();
    let id = box_with_mid_sketch(&mut s);
    set_slice(&mut s, id, true);
    assert!(s.active_slice().is_none(), "ohne offene Skizze kein Slice");
    s.open_sketch(id).unwrap();
    look_from_above(&mut s);
    let plane = s.active_slice().expect("offene Skizze mit Haken");
    // Ebene z = 2,5, Kamera oben: oben wird geschnitten, unten nicht.
    assert!(slice_cuts(plane, DVec3::new(5.0, 10.0, 4.0)));
    assert!(!slice_cuts(plane, DVec3::new(5.0, 10.0, 1.0)));
    assert!(
        !slice_cuts(plane, DVec3::new(5.0, 10.0, 2.5)),
        "Ebene selbst bleibt"
    );
    // Kamera unter die Ebene: die Seite wechselt.
    s.camera.pitch = -1.3;
    let plane = s.active_slice().unwrap();
    assert!(slice_cuts(plane, DVec3::new(5.0, 10.0, 1.0)));
    s.close_sketch();
    assert!(s.active_slice().is_none(), "nach dem Beenden kein Slice");
    assert!(
        s.document.sketch_display(id).slice,
        "Haken bleibt an der Skizze"
    );
    s.open_sketch(id).unwrap();
    assert!(
        s.active_slice().is_some(),
        "wirkt beim nächsten Öffnen wieder"
    );
    set_slice(&mut s, id, false);
    assert!(s.active_slice().is_none(), "Haken aus");
}

#[test]
fn pick_skips_sliced_faces() {
    let mut s = AppState::new();
    let id = box_with_mid_sketch(&mut s);
    s.open_sketch(id).unwrap();
    look_from_above(&mut s);
    let cursor = VIEW / 2.0;
    let (_, top, at) = s.face_point_under(cursor, VIEW).expect("trifft den Quader");
    assert!((at.z - 5.0).abs() < 1e-3, "ohne Slice die Deckfläche: {at}");
    let Some(Pick::Face(f)) = s.pick_with(cursor, VIEW, PickFilter::GEOMETRY) else {
        panic!("Fläche erwartet");
    };
    assert_eq!(f.face, top);

    set_slice(&mut s, id, true);
    let (_, bottom, at) = s
        .face_point_under(cursor, VIEW)
        .expect("trifft hinter der Ebene");
    assert!((at.z - 0.0).abs() < 1e-3, "mit Slice die Bodenfläche: {at}");
    assert_ne!(bottom, top);
    let Some(Pick::Face(f)) = s.pick_with(cursor, VIEW, PickFilter::GEOMETRY) else {
        panic!("Fläche erwartet");
    };
    assert_eq!(f.face, bottom, "Klick trifft die Fläche hinter der Ebene");
    assert_eq!(
        s.pick_face_under(cursor, VIEW).map(|f| f.face),
        Some(bottom)
    );
}

#[test]
fn slice_toggle_is_one_undo_step() {
    let mut w = Workspace::new(None, Config::default());
    let id = box_with_mid_sketch(&mut w.active_tab_mut().state);
    w.active_tab_mut().state.open_sketch(id).unwrap();
    w.observe_history(false);
    let steps = w.active_tab().history().undo_len();
    commands::run_action(&mut w, Action::SketchSlice);
    w.observe_history(false);
    let tab = w.active_tab();
    assert!(
        tab.state.document.sketch_display(id).slice,
        "Befehl schaltet ein"
    );
    assert_eq!(tab.history().undo_len(), steps + 1, "genau ein Schritt");
    assert!(tab.state.active_slice().is_some());
    // Wieder aus: zurück zur Vorgabe, wieder genau ein Schritt; dann wieder an.
    commands::run_action(&mut w, Action::SketchSlice);
    w.observe_history(false);
    assert!(!w.active_tab().state.document.sketch_display(id).slice);
    assert_eq!(w.active_tab().history().undo_len(), steps + 2);
    commands::run_action(&mut w, Action::SketchSlice);
    w.observe_history(false);
    // Nach dem Beenden (die Skizze ist unverändert) nimmt ein Rückgängig genau das letzte
    // Umschalten zurück.
    w.active_tab_mut().state.close_sketch();
    w.observe_history(false);
    assert_eq!(w.active_tab().history().undo_len(), steps + 3);
    w.active_tab_mut().undo().unwrap();
    assert!(!w.active_tab().state.document.sketch_display(id).slice);
    // Ohne offene Skizze tut der Befehl nichts.
    let before = w.active_tab().state.document.sketch_display(id);
    let len = w.active_tab().history().undo_len();
    commands::run_action(&mut w, Action::SketchSlice);
    w.observe_history(false);
    assert_eq!(w.active_tab().state.document.sketch_display(id), before);
    assert_eq!(w.active_tab().history().undo_len(), len);
}
