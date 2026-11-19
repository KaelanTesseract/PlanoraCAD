//! „Skizze erstellen“ wie in Fusion: Mit passender Vorauswahl (Grundebene, Konstruktionsebene,
//! ebene Fläche) entsteht die Skizze sofort; sonst startet ein Wahlmodus, in dem ein Klick in der
//! Ansicht oder im Baum die Ebene wählt. Esc und Rechtsklick beenden ohne Rest. Bilder bei
//! 1920×1080.

mod common;

use egui::{Event, Key, Modifiers, PointerButton, Pos2, Rect, pos2};
use planora_app::browser::{self, Folder};
use planora_app::history_bar;
use planora_app::state::{AppState, PendingPick};
use planora_app::{fonts, viewport};
use planora_feature::planes::{PlaneBase, PlaneDef};
use planora_feature::{FeatureId, SketchPlane, StandardPlane};
use planora_kernel::{Face, Id};
use planora_math::DVec3;
use planora_render::{FaceRef, Pick};

const SCREEN: Rect = Rect {
    min: pos2(0.0, 0.0),
    max: pos2(1920.0, 1080.0),
};
const VIEWPORT: glam::Vec2 = glam::Vec2::new(1920.0, 1080.0);

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

fn tree_frame(
    c: &egui::Context,
    s: &mut AppState,
    events: Vec<Event>,
    prefix: &str,
) -> Option<Pos2> {
    let out = c.run_ui(raw(events), |ui| browser::show(ui, s));
    let r = find_text(&out, prefix);
    out.drop_without_applying_deltas();
    r
}

/// Klick auf die Zeile `row` des Baums.
fn click_row(s: &mut AppState, row: &str) {
    let c = ctx();
    let pos = tree_frame(&c, s, vec![], row).unwrap_or_else(|| panic!("Zeile {row}"));
    tree_frame(&c, s, click(pos, PointerButton::Primary), "");
    tree_frame(&c, s, vec![], "");
}

fn view_frames(s: &mut AppState, events: Vec<Vec<Event>>) {
    let c = ctx();
    for e in events {
        let out = c.run_ui(raw(e), |ui| viewport::show_in(ui, s, None));
        out.drop_without_applying_deltas();
    }
}

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

fn feature_count(s: &AppState) -> usize {
    s.document.features().len()
}

fn top_face(s: &AppState, i: usize) -> Id<Face> {
    let solid = &s.bodies[i].solid;
    let top_z = s.bodies[i].bounds.1.z;
    solid
        .faces
        .iter()
        .find(|(id, _)| {
            planora_sketch::plane_of_face(solid, *id)
                .is_some_and(|f| f.z.z > 0.5 && (f.origin.z - top_z).abs() < 1e-9)
        })
        .map(|(id, _)| id)
        .expect("Deckfläche")
}

#[test]
fn create_sketch_on_selected_origin_plane_opens_at_once() {
    let mut s = AppState::new();
    s.select_origin_plane(StandardPlane::XZ);
    let before = feature_count(&s);
    let id = s.start_create_sketch().unwrap().expect("sofort angelegt");
    assert_eq!(s.pending_pick, None);
    assert_eq!(s.open_sketch_id(), Some(id));
    assert_eq!(plane_of(&s, id), SketchPlane::Standard(StandardPlane::XZ));
    assert_eq!(feature_count(&s), before + 1);
}

#[test]
fn create_sketch_on_selected_construction_plane() {
    let mut s = AppState::new();
    let plane = s
        .add_plane(PlaneDef::Offset {
            base: PlaneBase::Standard(StandardPlane::XY),
            distance: 10.0,
        })
        .unwrap();
    s.selected_feature = Some(plane);
    let id = s.start_create_sketch().unwrap().expect("sofort angelegt");
    assert_eq!(plane_of(&s, id), SketchPlane::OnPlane(plane));
    assert_eq!(s.open_sketch_id(), Some(id));
}

#[test]
fn create_sketch_on_selected_planar_face() {
    let mut s = AppState::new();
    s.create_box(DVec3::splat(20.0)).unwrap();
    let face = top_face(&s, 0);
    s.set_selection(Some(Pick::Face(FaceRef { body: 0, face })));
    let id = s.start_create_sketch().unwrap().expect("sofort angelegt");
    assert!(matches!(plane_of(&s, id), SketchPlane::OnFace(_)));
    let frame = *s.document.sketch_of(id).unwrap().frame();
    assert!((frame.origin.z - 20.0).abs() < 1e-9, "{frame:?}");
}

#[test]
fn create_sketch_without_selection_starts_pick() {
    let mut s = AppState::new();
    let before = feature_count(&s);
    assert_eq!(s.start_create_sketch(), Ok(None));
    assert_eq!(s.pending_pick, Some(PendingPick::NewSketch));
    assert_eq!(feature_count(&s), before);
    assert_eq!(s.open_sketch_id(), None);
}

#[test]
fn create_sketch_with_edge_selected_clears_and_picks() {
    let mut s = AppState::new();
    s.create_box(DVec3::splat(20.0)).unwrap();
    let edge = s.bodies[0].solid.edges.iter().next().unwrap().0;
    s.set_selection(Some(Pick::Edge(planora_render::EdgeRef { body: 0, edge })));
    assert_eq!(s.start_create_sketch(), Ok(None));
    assert_eq!(s.selection, None);
    assert_eq!(s.pending_pick, Some(PendingPick::NewSketch));
}

#[test]
fn create_sketch_pick_standard_plane_creates_and_opens() {
    let mut s = AppState::new();
    s.start_create_sketch().unwrap();
    s.tree_folders.set_open(Folder::Origin, true);
    click_row(&mut s, "YZ-Ebene");
    assert_eq!(s.pending_pick, None);
    let id = s.open_sketch_id().expect("Skizze offen");
    assert_eq!(plane_of(&s, id), SketchPlane::Standard(StandardPlane::YZ));
}

#[test]
fn create_sketch_pick_construction_plane_in_tree() {
    let mut s = AppState::new();
    let plane = s
        .add_plane(PlaneDef::Offset {
            base: PlaneBase::Standard(StandardPlane::XY),
            distance: 10.0,
        })
        .unwrap();
    s.selected_feature = None;
    s.set_selection(None);
    s.start_create_sketch().unwrap();
    s.tree_folders.set_open(Folder::Construction, true);
    click_row(&mut s, "Ebene 1");
    assert_eq!(s.pending_pick, None);
    let id = s.open_sketch_id().expect("Skizze offen");
    assert_eq!(plane_of(&s, id), SketchPlane::OnPlane(plane));
}

#[test]
fn create_sketch_pick_face_in_view() {
    let mut s = AppState::new();
    s.create_box(DVec3::splat(20.0)).unwrap();
    s.set_selection(None);
    s.selected_body = None;
    s.start_create_sketch().unwrap();
    look_from_above(&mut s, glam::Vec3::new(10.0, 10.0, 10.0));
    let top = screen(&s, glam::Vec3::new(10.0, 10.0, 20.0));
    view_click(&mut s, top, PointerButton::Primary);
    assert_eq!(s.pending_pick, None);
    let id = s.open_sketch_id().expect("Skizze offen");
    assert!(matches!(plane_of(&s, id), SketchPlane::OnFace(_)));
}

#[test]
fn create_sketch_pick_curved_face_keeps_mode() {
    let mut s = AppState::new();
    s.create_cylinder(10.0, 20.0).unwrap();
    s.set_selection(None);
    s.selected_body = None;
    look_from_above(&mut s, glam::Vec3::new(0.0, 0.0, 10.0));
    let before = feature_count(&s);
    s.start_create_sketch().unwrap();
    let side = screen(&s, glam::Vec3::new(0.0, -10.0, 10.0));
    view_click(&mut s, side, PointerButton::Primary);
    assert_eq!(
        s.message.as_deref(),
        Some(planora_i18n::t("state-sketch-planar-only").as_str())
    );
    assert_eq!(s.pending_pick, Some(PendingPick::NewSketch));
    assert_eq!(feature_count(&s), before);
}

#[test]
fn create_sketch_cancel_leaves_nothing() {
    let mut s = AppState::new();
    s.create_box(DVec3::splat(20.0)).unwrap();
    s.set_selection(None);
    s.selected_body = None;
    for p in [StandardPlane::XY, StandardPlane::XZ, StandardPlane::YZ] {
        s.set_origin_plane_shown(p, false);
    }
    look_from_above(&mut s, glam::Vec3::new(10.0, 10.0, 10.0));
    let top = screen(&s, glam::Vec3::new(10.0, 10.0, 20.0));
    let before = feature_count(&s);
    let edits = s.document.edit_count();
    // Esc.
    s.start_create_sketch().unwrap();
    view_frames(
        &mut s,
        vec![vec![Event::PointerMoved(top)], escape(), vec![]],
    );
    assert_eq!(s.pending_pick, None);
    // Rechtsklick.
    s.start_create_sketch().unwrap();
    view_click(&mut s, top, PointerButton::Secondary);
    assert_eq!(s.pending_pick, None);
    assert_eq!(feature_count(&s), before);
    assert_eq!(s.document.edit_count(), edits);
    assert_eq!(s.selection, None);
    // Der Zeiger steht nach dem Rechtsklick noch über der Deckfläche: Normale Hervorhebung, keine
    // Grundebene mehr.
    assert_eq!(s.origin_hover, None);
    for p in [StandardPlane::XY, StandardPlane::XZ, StandardPlane::YZ] {
        assert!(!s.is_origin_plane_shown(p), "{p:?} bleibt ausgeblendet");
    }
}

#[test]
fn create_sketch_closes_open_sketch_first() {
    let mut s = AppState::new();
    let a = s
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    s.open_sketch(a).unwrap();
    let before = feature_count(&s);
    assert_eq!(s.start_create_sketch(), Ok(None));
    assert_eq!(s.open_sketch_id(), None, "A geschlossen");
    assert_eq!(s.pending_pick, Some(PendingPick::NewSketch));
    s.cancel_pending_pick();
    assert_eq!(s.open_sketch_id(), None, "A bleibt geschlossen");
    assert_eq!(feature_count(&s), before);
}

#[test]
fn create_sketch_blocked_face_reports_and_does_not_pick() {
    let mut s = AppState::new();
    s.create_box(DVec3::splat(10.0)).unwrap();
    s.move_body(0, DVec3::X).unwrap();
    let m = s.document.features().last().unwrap().id;
    history_bar::move_marker(&mut s, m, 1).unwrap();
    let face = top_face(&s, 0);
    s.set_selection(Some(Pick::Face(FaceRef { body: 0, face })));
    let before = feature_count(&s);
    assert_eq!(s.start_create_sketch(), Err(history_bar::lock_hint()));
    assert_eq!(
        s.message.as_deref(),
        Some(history_bar::lock_hint().as_str())
    );
    assert_eq!(s.pending_pick, None);
    assert_eq!(feature_count(&s), before);
}

#[test]
fn create_sketch_mode_ends_on_restore() {
    let mut s = AppState::new();
    let snap = s.document.snapshot();
    s.start_create_sketch().unwrap();
    s.restore_document(snap).unwrap();
    assert_eq!(s.pending_pick, None);
}

/// Richtung von der Mitte zur Kamera und Bild-oben in Weltkoordinaten.
fn view_axes(s: &AppState) -> (glam::Vec3, glam::Vec3) {
    let to_cam = (s.camera.eye() - s.camera.target).normalize();
    let v = s.camera.view();
    let up = glam::Vec3::new(v.x_axis.y, v.y_axis.y, v.z_axis.y).normalize();
    (to_cam, up)
}

#[test]
fn create_sketch_view_direction_like_fusion() {
    use glam::Vec3;
    for (plane, to_cam, up) in [
        (StandardPlane::XY, Vec3::Z, Vec3::Y),
        (StandardPlane::XZ, -Vec3::Y, Vec3::Z),
        (StandardPlane::YZ, Vec3::X, Vec3::Z),
    ] {
        let mut s = AppState::new();
        s.camera.set_viewport(VIEWPORT.x, VIEWPORT.y);
        s.select_origin_plane(plane);
        s.start_create_sketch().unwrap().expect("angelegt");
        let (c, u) = view_axes(&s);
        // Senkrecht von oben klemmt die Kamera den Höhenwinkel knapp unter 90° (`MAX_PITCH`).
        assert!((c - to_cam).length() < 0.02, "{plane:?}: Blick {c:?}");
        assert!((u - up).length() < 0.02, "{plane:?}: oben {u:?}");
    }
}
