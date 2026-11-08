//! Skizzen 6, Task 13: „Skizzenebene neu festlegen“ wie in Fusion. Kontextmenü einer Skizze im
//! Browser oder in der Verlaufsleiste ⇒ Wahlmodus; ein Klick auf eine ebene Fläche, eine
//! Konstruktionsebene oder eine Grundebene im Ordner „Ursprung“ setzt die Skizze darauf (ein
//! Schritt). Esc und Rechtsklick beenden ohne Änderung. Bilder bei 1920×1080.

mod common;

use egui::{Event, Key, Modifiers, PointerButton, Pos2, Rect, pos2};
use planora_app::browser::{self, Folder};
use planora_app::commands;
use planora_app::keymap::{Action, Area, Keymap};
use planora_app::plane_dialog::PlaneCommand;
use planora_app::state::{AppState, PendingPick};
use planora_app::workspace::Workspace;
use planora_app::{app, fonts, viewport};
use planora_feature::planes::{PlaneBase, PlaneDef};
use planora_feature::{FeatureId, SketchPlane, StandardPlane};
use planora_math::{DVec2, DVec3};
use planora_project::config::Config;
use planora_project::library::Library;

const SCREEN: Rect = Rect {
    min: pos2(0.0, 0.0),
    max: pos2(1920.0, 1080.0),
};
const VIEWPORT: glam::Vec2 = glam::Vec2::new(1920.0, 1080.0);

fn ws() -> Workspace {
    let root = common::temp_root("planora-redefine");
    Workspace::new(Some(Library::open(&root).unwrap()), Config::default())
}

fn undo_len(w: &Workspace) -> usize {
    w.active_tab().history().undo_len()
}

fn ctx() -> egui::Context {
    let c = egui::Context::default();
    c.set_fonts(fonts::definitions());
    c
}

fn raw(events: Vec<Event>) -> egui::RawInput {
    egui::RawInput {
        screen_rect: Some(SCREEN),
        events,
        ..Default::default()
    }
}

fn button(pos: Pos2, button: PointerButton, pressed: bool) -> Event {
    Event::PointerButton {
        pos,
        button,
        pressed,
        modifiers: Modifiers::default(),
    }
}

fn click(pos: Pos2, b: PointerButton) -> Vec<Event> {
    vec![
        Event::PointerMoved(pos),
        button(pos, b, true),
        button(pos, b, false),
    ]
}

fn escape() -> Vec<Event> {
    vec![Event::Key {
        key: Key::Escape,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::default(),
    }]
}

/// Mitte des ersten Textes, der mit `prefix` beginnt.
fn find_text(out: &egui::FullOutput, prefix: &str) -> Option<Pos2> {
    fn walk(shape: &egui::epaint::Shape, prefix: &str) -> Option<Pos2> {
        match shape {
            egui::epaint::Shape::Text(t) if t.galley.text().starts_with(prefix) => {
                Some(t.visual_bounding_rect().center())
            }
            egui::epaint::Shape::Vec(v) => v.iter().find_map(|s| walk(s, prefix)),
            _ => None,
        }
    }
    out.shapes.iter().find_map(|c| walk(&c.shape, prefix))
}

/// Ein Bild des Baums; liefert alle Texte und die Lage von `prefix`.
fn tree_frame(
    c: &egui::Context,
    s: &mut AppState,
    events: Vec<Event>,
    prefix: &str,
) -> (String, Option<Pos2>) {
    let out = c.run_ui(raw(events), |ui| browser::show(ui, s));
    let r = (
        planora_app::testing::collect_texts(&out),
        find_text(&out, prefix),
    );
    out.drop_without_applying_deltas();
    r
}

/// Rechtsklick auf die Zeile `row`, dann Klick auf den Menüeintrag `entry`; liefert die Texte
/// des offenen Menüs.
fn context_menu(s: &mut AppState, row: &str, entry: &str) -> String {
    let c = ctx();
    let (_, pos) = tree_frame(&c, s, vec![], row);
    let pos = pos.unwrap_or_else(|| panic!("Zeile {row}"));
    tree_frame(&c, s, click(pos, PointerButton::Secondary), "");
    let (menu, item) = tree_frame(&c, s, vec![], entry);
    let item = item.unwrap_or_else(|| panic!("Menüeintrag {entry}:\n{menu}"));
    tree_frame(&c, s, click(item, PointerButton::Primary), "");
    menu
}

/// Klick auf die Zeile `row` des Baums.
fn click_row(s: &mut AppState, row: &str) {
    let c = ctx();
    let (texts, pos) = tree_frame(&c, s, vec![], row);
    let pos = pos.unwrap_or_else(|| panic!("Zeile {row}:\n{texts}"));
    tree_frame(&c, s, click(pos, PointerButton::Primary), "");
    tree_frame(&c, s, vec![], "");
}

/// Bilder der 3D-Ansicht über das ganze Fenster (wie in der App) mit `events` nacheinander.
fn view_frames(s: &mut AppState, events: Vec<Vec<Event>>) {
    let c = ctx();
    for e in events {
        let out = c.run_ui(raw(e), |ui| viewport::show_in(ui, s, None));
        out.drop_without_applying_deltas();
    }
}

/// Klick ohne Ziehen in die Ansicht an `p` (mit Bildern davor und danach).
fn view_click(s: &mut AppState, p: Pos2, b: PointerButton) {
    view_frames(
        s,
        vec![
            vec![Event::PointerMoved(p)],
            vec![button(p, b, true)],
            vec![button(p, b, false)],
            vec![],
        ],
    );
}

/// Kamera schräg von oben auf `target`, Bild 1920×1080.
fn look_from_above(s: &mut AppState, target: glam::Vec3) {
    s.camera = planora_render::OrbitCamera {
        target,
        distance: 200.0,
        pitch: 1.2,
        ..planora_render::OrbitCamera::default()
    };
    s.camera.set_viewport(VIEWPORT.x, VIEWPORT.y);
}

fn screen(s: &AppState, p: glam::Vec3) -> Pos2 {
    let v = s.camera.project(p, VIEWPORT).unwrap();
    pos2(v.x, v.y)
}

fn plane_of(s: &AppState, id: FeatureId) -> SketchPlane {
    match &s.document.feature(id).unwrap().kind {
        planora_feature::FeatureKind::Sketch(sk) => sk.plane.clone(),
        k => panic!("keine Skizze: {k:?}"),
    }
}

/// Quader 20 mm, danach „Skizze 1“ auf XY (die Skizze liegt hinter dem Quader).
fn box_then_sketch(w: &mut Workspace) -> FeatureId {
    let s = &mut w.active_tab_mut().state;
    s.create_box(DVec3::splat(20.0)).unwrap();
    let id = s
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    w.observe_history(false);
    id
}

#[test]
fn redefine_via_face_click() {
    let mut w = ws();
    let sketch = box_then_sketch(&mut w);
    let before = undo_len(&w);
    let s = &mut w.active_tab_mut().state;
    // Kontextmenü der Skizze im Browser.
    let menu = context_menu(s, "Skizze 1", "Skizzenebene neu festlegen");
    assert!(menu.contains("Extrudieren"), "{menu}");
    assert_eq!(s.pending_pick, Some(PendingPick::SketchPlane(sketch)));
    assert_eq!(s.selected_feature, Some(sketch));
    // Klick auf die Deckfläche des Quaders, dicht an einer Kante: Der Modus wählt Flächen, keine
    // Kanten.
    look_from_above(s, glam::Vec3::new(10.0, 10.0, 10.0));
    let near_edge = screen(s, glam::Vec3::new(10.0, 0.3, 20.0));
    let edits = s.document.edit_count();
    view_click(s, near_edge, PointerButton::Primary);
    assert_eq!(s.pending_pick, None, "Modus beendet");
    assert_eq!(s.message, None);
    assert!(matches!(plane_of(s, sketch), SketchPlane::OnFace(_)));
    assert_eq!(s.document.edit_count(), edits + 1, "eine Änderung");
    // Die Skizze liegt jetzt auf der Deckfläche (z = 20).
    let frame = *s.document.sketch_of(sketch).unwrap().frame();
    assert!((frame.origin.z - 20.0).abs() < 1e-9, "{frame:?}");
    w.observe_history(false);
    assert_eq!(undo_len(&w), before + 1);
    assert_eq!(
        w.active_tab().undo_label().as_deref(),
        Some("Skizzenebene neu festlegen")
    );
}

#[test]
fn redefine_via_origin_plane() {
    let mut w = ws();
    let plane = {
        let s = &mut w.active_tab_mut().state;
        s.add_plane(PlaneDef::Offset {
            base: PlaneBase::Standard(StandardPlane::XY),
            distance: 10.0,
        })
        .unwrap()
    };
    let sketch = box_then_sketch(&mut w);
    let before = undo_len(&w);
    let s = &mut w.active_tab_mut().state;
    s.tree_folders.set_open(Folder::Origin, true);
    s.tree_folders.set_open(Folder::Construction, true);
    // Ohne Wahlmodus ist der Ordner „Ursprung“ nur Anzeige.
    click_row(s, "YZ-Ebene");
    assert_eq!(
        plane_of(s, sketch),
        SketchPlane::Standard(StandardPlane::XY)
    );
    // Im Wahlmodus setzt ein Klick auf die Grundebene die Skizze darauf.
    s.start_redefine_sketch_plane(sketch).unwrap();
    click_row(s, "YZ-Ebene");
    w.observe_history(false);
    let s = &mut w.active_tab_mut().state;
    assert_eq!(s.pending_pick, None);
    assert_eq!(
        plane_of(s, sketch),
        SketchPlane::Standard(StandardPlane::YZ)
    );
    // Eine Konstruktionsebene im Ordner „Konstruktion“ ebenso.
    s.start_redefine_sketch_plane(sketch).unwrap();
    click_row(s, "Ebene 1");
    assert_eq!(s.pending_pick, None);
    assert_eq!(plane_of(s, sketch), SketchPlane::OnPlane(plane));
    let frame = *s.document.sketch_of(sketch).unwrap().frame();
    assert!((frame.origin.z - 10.0).abs() < 1e-9, "{frame:?}");
    // Die Eigenschaften nennen die Ebene.
    let kind = s.document.feature(sketch).unwrap().kind.clone();
    let rows = planora_app::properties::parameter_rows(s, &kind);
    assert!(rows.iter().any(|(_, v)| v == "Ebene 1"), "{rows:?}");
    w.observe_history(false);
    assert_eq!(undo_len(&w), before + 2, "je Wahl ein Schritt");
}

#[test]
fn esc_leaves_nothing() {
    let mut w = ws();
    let sketch = box_then_sketch(&mut w);
    let before = undo_len(&w);
    let s = &mut w.active_tab_mut().state;
    look_from_above(s, glam::Vec3::new(10.0, 10.0, 10.0));
    let top = screen(s, glam::Vec3::new(10.0, 10.0, 20.0));
    let edits = s.document.edit_count();
    // Esc in der Ansicht.
    s.start_redefine_sketch_plane(sketch).unwrap();
    view_frames(s, vec![vec![Event::PointerMoved(top)], escape(), vec![]]);
    assert_eq!(s.pending_pick, None);
    // Rechtsklick in der Ansicht.
    s.start_redefine_sketch_plane(sketch).unwrap();
    view_click(s, top, PointerButton::Secondary);
    assert_eq!(s.pending_pick, None);
    // Danach wählt ein Klick wieder normal (die Fläche) und ändert nichts.
    view_click(s, top, PointerButton::Primary);
    assert!(s.selection.is_some());
    assert_eq!(s.document.edit_count(), edits);
    assert_eq!(
        plane_of(s, sketch),
        SketchPlane::Standard(StandardPlane::XY)
    );
    // Eine offene Skizze wird vor dem Modus beendet (wie Extrudieren aus dem Kontextmenü).
    s.open_sketch(sketch).unwrap();
    s.start_redefine_sketch_plane(sketch).unwrap();
    assert_eq!(s.open_sketch_id(), None);
    assert_eq!(s.pending_pick, Some(PendingPick::SketchPlane(sketch)));
    s.cancel_pending_pick();
    w.observe_history(false);
    assert_eq!(undo_len(&w), before);
}

#[test]
fn rejected_shows_message_and_keeps_document() {
    let mut w = ws();
    // Skizze vor dem Quader: Dessen Flächen liegen hinter der Skizze im Verlauf.
    let sketch = {
        let s = &mut w.active_tab_mut().state;
        let id = s
            .add_sketch(SketchPlane::Standard(StandardPlane::XY))
            .unwrap();
        s.create_box(DVec3::splat(20.0)).unwrap();
        id
    };
    w.observe_history(false);
    let before = undo_len(&w);
    let s = &mut w.active_tab_mut().state;
    look_from_above(s, glam::Vec3::new(10.0, 10.0, 10.0));
    let edits = s.document.edit_count();
    s.start_redefine_sketch_plane(sketch).unwrap();
    view_click(
        s,
        screen(s, glam::Vec3::new(10.0, 10.0, 20.0)),
        PointerButton::Primary,
    );
    let text = planora_i18n::t("feature-err-forward-reference");
    let message = s.message.clone().expect("Meldung");
    assert!(
        message.contains(text.split('{').next().unwrap().trim()),
        "{message}"
    );
    assert_eq!(s.pending_pick, None, "Modus beendet");
    assert_eq!(s.document.edit_count(), edits);
    assert_eq!(
        plane_of(s, sketch),
        SketchPlane::Standard(StandardPlane::XY)
    );
    w.observe_history(false);
    assert_eq!(undo_len(&w), before);
}

#[test]
fn curved_face_keeps_mode() {
    let mut s = AppState::new();
    s.create_cylinder(10.0, 20.0).unwrap();
    let sketch = s
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    look_from_above(&mut s, glam::Vec3::new(0.0, 0.0, 10.0));
    let edits = s.document.edit_count();
    s.start_redefine_sketch_plane(sketch).unwrap();
    // Mantelfläche (seitlich, halbe Höhe).
    let side = screen(&s, glam::Vec3::new(0.0, -10.0, 10.0));
    view_click(&mut s, side, PointerButton::Primary);
    assert_eq!(
        s.message.as_deref(),
        Some(planora_i18n::t("state-sketch-planar-only").as_str())
    );
    assert_eq!(s.pending_pick, Some(PendingPick::SketchPlane(sketch)));
    assert_eq!(s.document.edit_count(), edits);
}

#[test]
fn undo_restores_old_plane() {
    let mut w = ws();
    let sketch = box_then_sketch(&mut w);
    let before = undo_len(&w);
    w.active_tab_mut()
        .state
        .redefine_sketch_plane(sketch, SketchPlane::Standard(StandardPlane::XZ))
        .unwrap();
    w.observe_history(false);
    assert_eq!(undo_len(&w), before + 1);
    // Gleiche Ebene: kein Schritt.
    w.active_tab_mut()
        .state
        .redefine_sketch_plane(sketch, SketchPlane::Standard(StandardPlane::XZ))
        .unwrap();
    w.observe_history(false);
    assert_eq!(undo_len(&w), before + 1);
    commands::run_action(&mut w, Action::Undo);
    let s = &w.active_tab().state;
    assert_eq!(
        plane_of(s, sketch),
        SketchPlane::Standard(StandardPlane::XY)
    );
    let frame = *s.document.sketch_of(sketch).unwrap().frame();
    assert!(frame.z.z > 0.99, "{frame:?}");
    commands::run_action(&mut w, Action::Redo);
    assert_eq!(
        plane_of(&w.active_tab().state, sketch),
        SketchPlane::Standard(StandardPlane::XZ)
    );
}

#[test]
fn key_starts_the_mode_and_windows_exclude_it() {
    // Belegbare Aktion ohne Vorgabetaste (wie Fusion).
    let a = Action::SketchRedefinePlane;
    assert_eq!(a.id(), "skizze.ebene_neu");
    assert_eq!(a.area(), Area::Model);
    assert!(a.default_bindings().is_empty());
    assert_eq!(a.label(), "Skizzenebene neu festlegen");
    assert!(Keymap::defaults().bindings(a).is_empty());
    let mut w = ws();
    let sketch = box_then_sketch(&mut w);
    // Ohne gewählte Skizze nur ein Hinweis.
    w.active_tab_mut().state.selected_feature = None;
    commands::run_action(&mut w, a);
    assert_eq!(w.active_tab().state.pending_pick, None);
    assert_eq!(
        w.active_tab().state.info.as_deref(),
        Some("Zuerst eine Skizze wählen.")
    );
    // Mit gewählter Skizze beginnt der Modus; ein offenes Ebenen-Fenster schließt dabei.
    app::open_plane(&mut w, PlaneCommand::Offset);
    assert!(w.active_tab().plane_dialog.is_some());
    w.active_tab_mut().state.selected_feature = Some(sketch);
    commands::run_action(&mut w, a);
    let tab = w.active_tab_mut();
    assert_eq!(
        tab.state.pending_pick,
        Some(PendingPick::SketchPlane(sketch))
    );
    assert!(tab.plane_dialog.is_none());
    assert!(tab.state.plane_pick.is_none());
    // Umgekehrt beendet „Verrunden“ den Modus.
    app::open_blend(&mut w, false);
    assert_eq!(w.active_tab().state.pending_pick, None);
    assert!(w.active_tab().blend_dialog.is_some());
    // Aus dem Kontextmenü der Verlaufsleiste (Browser-Weg siehe oben) bei offenem
    // „Verrunden“: Das Fenster schließt im nächsten Bild der App.
    w.active_tab_mut()
        .state
        .start_redefine_sketch_plane(sketch)
        .unwrap();
    w.active_tab_mut().exclude_pick_modes();
    assert!(w.active_tab().blend_dialog.is_none());
    assert!(!w.active_tab().state.edge_picking);
}

#[test]
fn history_bar_menu_starts_the_mode() {
    // Extrudiertes Rechteck: Die Skizze steht als Schritt in der Verlaufsleiste.
    let mut s = AppState::new();
    let sketch = s
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    s.open_sketch(sketch).unwrap();
    s.session_sketch_mut()
        .unwrap()
        .1
        .add_rectangle(DVec2::new(0.0, 0.0), DVec2::new(10.0, 10.0))
        .unwrap();
    s.close_sketch();
    s.extrude_sketch(sketch, 0, 5.0).unwrap();
    bar_menu(&mut s, sketch, "Skizzenebene neu festlegen");
    assert_eq!(s.pending_pick, Some(PendingPick::SketchPlane(sketch)));
}

/// Ein Bild nur mit der Verlaufsleiste.
fn bar_frame(
    c: &egui::Context,
    s: &mut AppState,
    time: &mut f64,
    events: Vec<Event>,
) -> egui::FullOutput {
    *time += 0.05;
    let raw = egui::RawInput {
        screen_rect: Some(SCREEN),
        time: Some(*time),
        events,
        ..Default::default()
    };
    c.run_ui(raw, |ui| planora_app::history_bar::show(ui.ctx(), &mut *s))
}

/// Rechtsklick auf den Schritt `id` der Verlaufsleiste, dann Klick auf `entry`.
fn bar_menu(s: &mut AppState, id: FeatureId, entry: &str) {
    let c = ctx();
    let mut time = 0.0;
    for _ in 0..3 {
        bar_frame(&c, s, &mut time, vec![]).drop_without_applying_deltas();
    }
    let rect = planora_app::history_bar::bar_rects(&c)
        .steps
        .into_iter()
        .find(|(f, _)| *f == id)
        .expect("Schritt in der Leiste")
        .1;
    bar_frame(
        &c,
        s,
        &mut time,
        click(rect.center(), PointerButton::Secondary),
    )
    .drop_without_applying_deltas();
    let out = bar_frame(&c, s, &mut time, vec![]);
    let item = find_text(&out, entry).unwrap_or_else(|| {
        panic!(
            "{entry}:
{}",
            planora_app::testing::collect_texts(&out)
        )
    });
    out.drop_without_applying_deltas();
    for events in [
        vec![Event::PointerMoved(item)],
        vec![button(item, PointerButton::Primary, true)],
        vec![button(item, PointerButton::Primary, false)],
        vec![],
    ] {
        bar_frame(&c, s, &mut time, events).drop_without_applying_deltas();
    }
}
