//! Dokument im Zustand ersetzen (Rückgängig/Wiederholen, Aufgabe 4): `AppState::restore_document`
//! tauscht nur das Dokument aus; Kamera, Hell/Dunkel und Kennung bleiben, Namenszähler, Körper,
//! Auswahl, Dialoge, Werkzeuge und der Zeichnungsbereich folgen dem zurückgeholten Stand.

use planora_app::browser::delete_plan;
use planora_app::dimension_browser::DimDeletePlan;
use planora_app::dimension_dialog::DimensionDialog;
use planora_app::dimension_tool::ToolMode;
use planora_app::drawing_browser::DrawingDeletePlan;
use planora_app::drawing_state::{DrawingUi, ViewResult, WorkArea};
use planora_app::sheet_dialog;
use planora_app::state::*;
use planora_drawing::dimension::{DimId, DimKind, DimSpec, Tolerance as DimTolerance};
use planora_drawing::{
    DrawingId, Paper, Placement, Projection, Scale, Sheet, ViewId, ViewKind, ViewLines,
};
use planora_feature::{BlendArt, FeatureId, FeatureStatus, RestoreKind, SketchPlane};
use planora_kernel::blend::topo::{edge_uses, face_name};
use planora_kernel::{Edge, Face, Id, Solid};
use planora_math::DVec3;
use planora_render::{EdgeRef, FaceRef, Highlight, Pick, build_scene};
use std::sync::Arc;

const A4_QUER: Sheet = Sheet {
    paper: Paper::A4,
    landscape: true,
};

fn s11() -> Scale {
    Scale { num: 1, den: 1 }
}

fn free(x: f64, y: f64) -> Placement {
    Placement::Free { x, y }
}

fn cube(st: &mut AppState) -> FeatureId {
    st.create_box(DVec3::splat(10.0)).unwrap()
}

fn new_drawing(st: &mut AppState, name: &str) -> DrawingId {
    st.document
        .add_drawing(name, A4_QUER, Projection::First)
        .unwrap()
}

fn add_view(st: &mut AppState, d: DrawingId, kind: ViewKind, x: f64, y: f64) -> ViewId {
    st.document
        .add_view(d, kind, s11(), free(x, y), None)
        .unwrap()
}

fn tick(st: &mut AppState, block: bool) {
    st.drawing_ui.tick(&st.bodies, &st.document, block);
}

fn ready(st: &AppState, kind: ViewKind) -> Arc<ViewLines> {
    match st.drawing_ui.result(kind, st.document.model_edit_count()) {
        ViewResult::Ready(v) => v,
        other => panic!("{kind:?}: {other:?}"),
    }
}

fn edge_between(s: &Solid, a: &str, b: &str) -> Id<Edge> {
    s.edges
        .iter()
        .map(|(id, _)| id)
        .find(|&id| {
            let n: Vec<String> = edge_uses(s, id)
                .iter()
                .map(|u| face_name(s, u.face))
                .collect();
            n.len() == 2 && ((n[0] == a && n[1] == b) || (n[0] == b && n[1] == a))
        })
        .unwrap()
}

/// Deckfläche (ebene Fläche mit Normale +z ganz oben) des Körpers `i`.
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

fn names_and_ids(st: &AppState) -> Vec<(FeatureId, String)> {
    st.document
        .features()
        .iter()
        .map(|f| (f.id, f.name.clone()))
        .collect()
}

fn dim_spec(view: ViewId) -> DimSpec {
    DimSpec {
        id: DimId(0),
        view,
        kind: DimKind::Horizontal,
        refs: Vec::new(),
        offset: 10.0,
        angle: 45.0,
        text_shift: (0.0, 0.0),
        tolerance: DimTolerance::None,
        decimals: 1,
        text_override: None,
        chain: None,
        suggested: false,
    }
}

/// Die Szene des Zustands als Text (zum Vergleich).
fn scene_text(st: &AppState) -> String {
    format!("{:?}", *st.scene)
}

/// Szene der Körper ohne Auswahl, Hervorhebung und Markierung, ungedämpft.
fn plain_scene(st: &AppState) -> String {
    let meshes: Vec<&planora_mesh::Mesh> = st.bodies.iter().map(|b| &b.mesh).collect();
    format!(
        "{:?}",
        build_scene(&meshes, &Highlight::default(), &st.palette())
    )
}

#[test]
fn camera_and_theme_stay() {
    let mut st = AppState::new();
    cube(&mut st);
    let picture = st.snapshot();
    cube(&mut st);
    assert_eq!(st.bodies.len(), 2);
    // Nach dem zweiten Körper gedreht und verschoben: Diese Ansicht muss bleiben.
    st.camera.yaw += 0.7;
    st.camera.pitch -= 0.2;
    st.camera.distance *= 3.0;
    st.camera.target += glam::Vec3::new(5.0, -3.0, 2.0);
    st.dark = false;
    st.fit_pending = true;
    let camera = st.camera;
    let instance = st.instance;
    assert_eq!(st.restore_document(picture), Ok(RestoreKind::Full));
    assert_eq!(st.camera, camera);
    assert_eq!(st.camera.yaw.to_bits(), camera.yaw.to_bits());
    assert_eq!(st.camera.distance.to_bits(), camera.distance.to_bits());
    assert_eq!(st.camera.target.x.to_bits(), camera.target.x.to_bits());
    assert!(!st.dark);
    assert!(st.fit_pending);
    assert_eq!(st.instance, instance);
    assert_eq!(st.bodies.len(), 1);
    // Die Szene zeigt den zurückgeholten Stand (ein Körper, ohne Hervorhebung).
    assert_eq!(scene_text(&st), plain_scene(&st));

    // Auch `fit_pending == false` bleibt, wie es ist.
    st.fit_pending = false;
    let picture = st.snapshot();
    cube(&mut st);
    st.restore_document(picture).unwrap();
    assert!(!st.fit_pending);
}

#[test]
fn names_continue_from_restored_state() {
    let mut st = AppState::new();
    cube(&mut st);
    cube(&mut st);
    let after_two = st.snapshot();
    cube(&mut st);
    assert_eq!(st.document.features()[2].name, "Quader 3");
    st.restore_document(after_two).unwrap();
    let id = cube(&mut st);
    assert_eq!(st.document.feature(id).unwrap().name, "Quader 3");

    // Auch nach vorn: Bild mit drei Quadern zurückgeholt, der nächste heißt „Quader 4“.
    let mut st = AppState::new();
    cube(&mut st);
    let one = st.snapshot();
    cube(&mut st);
    cube(&mut st);
    let three = st.snapshot();
    st.restore_document(one).unwrap();
    st.restore_document(three).unwrap();
    let id = cube(&mut st);
    assert_eq!(st.document.feature(id).unwrap().name, "Quader 4");
}

#[test]
fn delete_with_dependents_comes_back() {
    let mut st = AppState::new();
    let quader = cube(&mut st);
    st.move_body(0, DVec3::new(5.0, 0.0, 0.0)).unwrap();
    let e = edge_between(&st.bodies[0].solid, "Deckel", "Wand 1");
    st.add_blend(0, &[e], BlendArt::Rundung { radius: 2.0 }, true)
        .unwrap();
    let sketch = st.new_sketch_on_face(0, top_face(&st, 0)).unwrap();
    assert_eq!(st.document.features().len(), 4);
    let picture = st.snapshot();
    let before = names_and_ids(&st);
    let marker = st.document.marker();
    let volume = st.bodies[0].mass.volume;

    // Die Skizze ist beim Löschen offen.
    st.open_sketch(sketch).unwrap();
    st.delete_feature_with_dependents(quader).unwrap();
    assert!(st.document.features().is_empty());
    assert!(st.sketch_session.is_none());

    assert_eq!(st.restore_document(picture), Ok(RestoreKind::Full));
    assert_eq!(names_and_ids(&st), before);
    assert_eq!(st.document.marker(), marker);
    assert!(
        st.document
            .features()
            .iter()
            .all(|f| f.status == FeatureStatus::Ok),
        "{:?}",
        st.document
            .features()
            .iter()
            .map(|f| &f.status)
            .collect::<Vec<_>>()
    );
    assert_eq!(st.bodies.len(), 1);
    assert!((st.bodies[0].mass.volume - volume).abs() < 1e-9);
    assert!(st.sketch_session.is_none());
    assert!(st.sketch_draft.is_none());
    assert_eq!(scene_text(&st), plain_scene(&st));
}

#[test]
fn transient_ui_is_cleared() {
    let mut st = AppState::new();
    let q1 = cube(&mut st);
    let d = new_drawing(&mut st, "Blatt 1");
    let v = add_view(&mut st, d, ViewKind::Front, 100.0, 100.0);
    let one = st.snapshot();
    let q2 = cube(&mut st);
    // Ein laufender Zug am zweiten Körper.
    st.move_body_dragging(1, DVec3::new(1.0, 0.0, 0.0)).unwrap();
    assert!(st.drag_move().is_some());
    let e = edge_between(&st.bodies[1].solid, "Deckel", "Wand 1");
    st.edge_picking = true;
    st.toggle_edge_mark(EdgeRef { body: 1, edge: e }, false);
    assert!(!st.marked_edges.is_empty());
    st.set_selection(Some(Pick::Face(FaceRef {
        body: 1,
        face: top_face(&st, 1),
    })));
    st.set_hover(Some(Pick::Edge(EdgeRef { body: 1, edge: e })));
    assert_eq!(st.selected_body, Some(1));
    st.pending_delete = Some(delete_plan(&st, q2));
    st.pending_delete_drawing = Some(DrawingDeletePlan {
        drawing: d,
        view: None,
        text: "Löschen?".to_string().into(),
    });
    st.pending_delete_dimension = Some(DimDeletePlan {
        drawing: d,
        dim: DimId(1),
        text: "Maß löschen?".to_string().into(),
    });
    st.rename_draft = Some((q1, "Neu".into()));
    st.rename_drawing_draft = Some((d, "Blatt neu".into()));
    st.edit_request = Some(q1);
    st.sketch_request = Some(SketchRequest::Extrude(q1));
    st.dimension_dialog = Some(DimensionDialog::new(d, &dim_spec(v), false));
    sheet_dialog::open(&mut st, d);
    assert!(st.sheet_dialog.is_some());
    st.dimension_tool.set_mode(ToolMode::Diameter);
    st.balloon_tool.set_active(true);
    st.section_tool.set_active(true);
    st.selected_feature = Some(q2);
    st.message = Some("alte Meldung".into());

    assert_eq!(st.restore_document(one.clone()), Ok(RestoreKind::Full));
    assert!(st.pending_delete.is_none());
    assert!(st.pending_delete_drawing.is_none());
    assert!(st.pending_delete_dimension.is_none());
    assert!(st.rename_draft.is_none());
    assert!(st.rename_drawing_draft.is_none());
    assert!(st.edit_request.is_none());
    assert!(st.sketch_request.is_none());
    assert!(st.marked_edges.is_empty());
    assert!(!st.edge_picking);
    assert!(st.drag_move().is_none());
    assert!(st.dimension_dialog.is_none());
    assert!(st.sheet_dialog.is_none());
    assert_eq!(st.dimension_tool.mode, ToolMode::Off);
    assert!(!st.dimension_tool.is_active());
    assert!(!st.balloon_tool.is_active());
    assert!(!st.section_tool.is_active());
    assert!(st.message.is_none());
    // Der zweite Körper fehlt: Auswahl, Hervorhebung, gewählter Körper und Feature entfallen.
    assert_eq!(st.bodies.len(), 1);
    assert!(st.selection.is_none());
    assert!(st.hover.is_none());
    assert!(st.selected_body.is_none());
    assert!(st.selected_feature.is_none());
    // Die Szene trägt keine Markierung und keine Auswahl mehr.
    assert_eq!(scene_text(&st), plain_scene(&st), "Hervorhebung blieb");

    // Auf dem vollen Weg entfallen Flächen-/Kantenwahl und Hervorhebung auch auf einem Körper,
    // den es weiter gibt (er kann anders nummeriert sein); gewählter Körper und Feature bleiben.
    cube(&mut st);
    let face = top_face(&st, 0);
    st.set_selection(Some(Pick::Face(FaceRef { body: 0, face })));
    st.set_hover(Some(Pick::Face(FaceRef { body: 0, face })));
    st.selected_feature = Some(q1);
    assert_eq!(st.restore_document(one), Ok(RestoreKind::Full));
    assert_eq!(st.selection, None);
    assert_eq!(st.hover, None);
    assert_eq!(st.selected_body, Some(0));
    assert_eq!(st.selected_feature, Some(q1));
    assert_eq!(scene_text(&st), plain_scene(&st), "Hervorhebung blieb");

    // Der gewählte Körper folgt seinem Verweis auf den neuen Index.
    let mut st = AppState::new();
    cube(&mut st);
    let q2 = cube(&mut st);
    let both = st.snapshot();
    st.delete_feature(st.naming_feature(0).unwrap()).unwrap();
    st.selected_body = Some(0);
    assert_eq!(st.bodies[0].source.feature, q2);
    st.restore_document(both).unwrap();
    assert_eq!(st.selected_body, Some(1));
    assert_eq!(st.bodies[1].source.feature, q2);
}

#[test]
fn drawing_path_keeps_selection_hover_and_body() {
    let mut st = AppState::new();
    cube(&mut st);
    cube(&mut st);
    let d = new_drawing(&mut st, "Blatt 1");
    let picture = st.snapshot();
    add_view(&mut st, d, ViewKind::Front, 100.0, 100.0);
    let face = top_face(&st, 1);
    let e = edge_between(&st.bodies[0].solid, "Deckel", "Wand 1");
    st.set_selection(Some(Pick::Face(FaceRef { body: 1, face })));
    st.set_hover(Some(Pick::Edge(EdgeRef { body: 0, edge: e })));
    assert_eq!(st.selected_body, Some(1));
    let scene = Arc::clone(&st.scene);
    assert_eq!(st.restore_document(picture), Ok(RestoreKind::Annotations));
    assert_eq!(st.selection, Some(Pick::Face(FaceRef { body: 1, face })));
    assert_eq!(st.hover, Some(Pick::Edge(EdgeRef { body: 0, edge: e })));
    assert_eq!(st.selected_body, Some(1));
    assert!(Arc::ptr_eq(&scene, &st.scene));
}

#[test]
fn reused_drawing_id_starts_fresh() {
    // Rückgängig „Zeichnung anlegen“: Die nächste neue Zeichnung bekommt dieselbe Kennung und
    // darf Zoom und Verschiebung der zurückgenommenen nicht erben.
    let mut st = AppState::new();
    cube(&mut st);
    let without = st.snapshot();
    let d = new_drawing(&mut st, "Blatt 1");
    st.drawing_ui.open(d);
    st.drawing_ui.zoom = 3.0;
    st.drawing_ui.pan = egui::vec2(40.0, -20.0);
    st.restore_document(without).unwrap();
    assert_eq!(st.drawing_ui.area, WorkArea::Model);
    let again = new_drawing(&mut st, "Blatt 2");
    assert_eq!(again, d, "Kennung wird wiederverwendet");
    st.drawing_ui.show_drawing(again);
    assert_eq!(st.drawing_ui.zoom, 1.0);
    assert_eq!(st.drawing_ui.pan, egui::Vec2::ZERO);
    // Gegenprobe: ohne Rückgängig behält dieselbe Zeichnung ihre Ansicht.
    st.drawing_ui.zoom = 2.0;
    st.drawing_ui.area = WorkArea::Model;
    st.drawing_ui.show_drawing(again);
    assert_eq!(st.drawing_ui.zoom, 2.0);
}

#[test]
fn tab_dialogs_are_the_callers_duty() {
    // `restore_document` kennt die Fenster des Tabs nicht: Sie bleiben offen und zeigen auf
    // Features des alten Dokuments. Aufgabe 6 schließt sie mit `DocTab::close_dialogs()`.
    let mut ws =
        planora_app::workspace::Workspace::new(None, planora_project::config::Config::default());
    let tab = ws.active_tab_mut();
    let before = tab.state.snapshot();
    let q = cube(&mut tab.state);
    cube(&mut tab.state);
    tab.feature_dialog = planora_app::feature_dialog::FeatureDialog::new(&tab.state, q);
    tab.blend_dialog = Some(planora_app::blend_dialog::BlendDialog::fillet());
    tab.boolean_dialog = Some(planora_app::boolean_dialog::BooleanDialog::new(&tab.state));
    assert!(tab.feature_dialog.is_some());
    tab.state.restore_document(before).unwrap();
    assert!(tab.state.document.feature(q).is_none());
    assert_eq!(tab.feature_dialog.as_ref().map(|f| f.feature()), Some(q));
    assert!(tab.blend_dialog.is_some());
    assert!(tab.boolean_dialog.is_some());
}

#[test]
fn transient_ui_is_cleared_on_the_drawing_path_too() {
    // Reiner Zeichnungsschritt: Auch hier verfallen Rückfragen, Fenster, Werkzeuge und
    // Markierungen (die Szene wird dann ohne Markierung neu gebaut).
    let mut st = AppState::new();
    cube(&mut st);
    let d = new_drawing(&mut st, "Blatt 1");
    let picture = st.snapshot();
    let v = add_view(&mut st, d, ViewKind::Front, 100.0, 100.0);
    let e = edge_between(&st.bodies[0].solid, "Deckel", "Wand 1");
    st.toggle_edge_mark(EdgeRef { body: 0, edge: e }, false);
    st.edge_picking = true;
    st.dimension_dialog = Some(DimensionDialog::new(d, &dim_spec(v), false));
    st.section_tool.set_active(true);
    st.balloon_tool.set_active(true);
    st.dimension_tool.set_mode(ToolMode::Radius);
    st.pending_delete_drawing = Some(DrawingDeletePlan {
        drawing: d,
        view: Some(v),
        text: "Löschen?".to_string().into(),
    });
    st.message = Some("alt".into());
    let scene = Arc::clone(&st.scene);
    assert_eq!(st.restore_document(picture), Ok(RestoreKind::Annotations));
    assert!(st.marked_edges.is_empty());
    assert!(!st.edge_picking);
    assert!(st.dimension_dialog.is_none());
    assert!(st.pending_delete_drawing.is_none());
    assert!(!st.section_tool.is_active());
    assert!(!st.balloon_tool.is_active());
    assert!(!st.dimension_tool.is_active());
    assert!(st.message.is_none());
    assert!(
        !Arc::ptr_eq(&scene, &st.scene),
        "Markierung blieb in der Szene"
    );
    assert_eq!(scene_text(&st), plain_scene(&st));
}

#[test]
fn failure_changes_nothing() {
    let mut st = AppState::new();
    let q1 = cube(&mut st);
    cube(&mut st);
    let d = new_drawing(&mut st, "Blatt 1");
    let face = top_face(&st, 1);
    st.set_selection(Some(Pick::Face(FaceRef { body: 1, face })));
    st.pending_delete = Some(delete_plan(&st, q1));
    sheet_dialog::open(&mut st, d);
    st.section_tool.set_active(true);
    st.message = Some("alt".into());
    st.drawing_ui.open(d);

    // Voller Weg: Marker hinter dem letzten Feature.
    let mut bad = st.snapshot();
    bad.marker = bad.features.len() + 1;
    // Weg nur über Zeichnungen: ungültiger Zeichnungszähler bei gleichem Modell.
    let mut bad_drawings = st.snapshot();
    bad_drawings.next_drawing = 0;
    for bad in [bad, bad_drawings] {
        let before = st.snapshot();
        let edits = st.document.edit_count();
        let scene = Arc::clone(&st.scene);
        let err = st.restore_document(bad).unwrap_err();
        assert!(!err.is_empty());
        assert!(st.snapshot().same_content(&before));
        assert_eq!(st.document.edit_count(), edits);
        assert_eq!(st.bodies.len(), 2);
        assert!(Arc::ptr_eq(&scene, &st.scene));
        assert_eq!(st.selection, Some(Pick::Face(FaceRef { body: 1, face })));
        assert!(st.pending_delete.is_some());
        assert!(st.sheet_dialog.is_some());
        assert!(st.section_tool.is_active());
        assert_eq!(st.message.as_deref(), Some("alt"));
        assert_eq!(st.drawing_ui.area, WorkArea::Drawing(d));
    }
}

#[test]
fn drawing_step_keeps_view_results() {
    let mut st = AppState::new();
    cube(&mut st);
    let d = new_drawing(&mut st, "Blatt 1");
    let front = add_view(&mut st, d, ViewKind::Front, 100.0, 100.0);
    add_view(&mut st, d, ViewKind::Top, 100.0, 50.0);
    st.drawing_ui.open(d);
    tick(&mut st, true);
    let front_lines = ready(&st, ViewKind::Front);
    let top_lines = ready(&st, ViewKind::Top);
    let picture = st.snapshot();
    let old_place = st
        .document
        .drawing(d)
        .unwrap()
        .view(front)
        .unwrap()
        .placement;

    // Ansicht verschieben.
    st.document
        .update_drawing(d, |dr| {
            dr.views[0].placement = free(150.0, 120.0);
        })
        .unwrap();
    tick(&mut st, true);
    let spawned = st.drawing_ui.spawned();
    let model = st.document.model_edit_count();
    let edits = st.document.edit_count();
    let scene = Arc::clone(&st.scene);
    let positions = st.bodies[0].mesh.positions.as_ptr();

    assert_eq!(st.restore_document(picture), Ok(RestoreKind::Annotations));
    assert_eq!(st.document.model_edit_count(), model);
    assert!(st.document.edit_count() > edits, "Zähler muss steigen");
    assert_eq!(
        st.document
            .drawing(d)
            .unwrap()
            .view(front)
            .unwrap()
            .placement,
        old_place
    );
    // Körper und Szene unverändert, nichts neu tesselliert.
    assert!(Arc::ptr_eq(&scene, &st.scene));
    assert_eq!(st.bodies[0].mesh.positions.as_ptr(), positions);
    assert_eq!(st.drawing_ui.area, WorkArea::Drawing(d));
    tick(&mut st, false);
    assert_eq!(st.drawing_ui.spawned(), spawned, "neue Ansichtsberechnung");
    assert!(Arc::ptr_eq(&ready(&st, ViewKind::Front), &front_lines));
    assert!(Arc::ptr_eq(&ready(&st, ViewKind::Top), &top_lines));
}

#[test]
fn model_step_refreshes_drawing() {
    let mut st = AppState::new();
    cube(&mut st);
    let without_drawing = st.snapshot();
    let d = new_drawing(&mut st, "Blatt 1");
    add_view(&mut st, d, ViewKind::Front, 100.0, 100.0);
    st.drawing_ui.open(d);
    st.drawing_ui.selected_drawing = Some(d);
    tick(&mut st, true);
    let lines1 = ready(&st, ViewKind::Front);
    let one = st.snapshot();
    cube(&mut st);
    tick(&mut st, true);
    let lines2 = ready(&st, ViewKind::Front);
    let two = st.snapshot();
    cube(&mut st);
    tick(&mut st, true);
    let lines3 = ready(&st, ViewKind::Front);
    let width = |l: &ViewLines| l.bbox.1.0 - l.bbox.0.0;
    assert!(width(&lines2) > width(&lines1) + 5.0);
    assert!(width(&lines3) > width(&lines2) + 5.0);

    // Zweimal hintereinander zurück, jedes Mal das passende Ergebnis.
    let mut counts = vec![st.document.model_edit_count()];
    assert_eq!(st.restore_document(two), Ok(RestoreKind::Full));
    counts.push(st.document.model_edit_count());
    assert!(
        matches!(
            st.drawing_ui
                .result(ViewKind::Front, st.document.model_edit_count()),
            ViewResult::Pending { .. }
        ),
        "altes Ergebnis erscheint als aktuell"
    );
    tick(&mut st, true);
    let back2 = ready(&st, ViewKind::Front);
    assert!(!Arc::ptr_eq(&back2, &lines3));
    assert_eq!(back2.bbox, lines2.bbox);
    assert_eq!(st.restore_document(one), Ok(RestoreKind::Full));
    counts.push(st.document.model_edit_count());
    tick(&mut st, true);
    let back1 = ready(&st, ViewKind::Front);
    assert!(!Arc::ptr_eq(&back1, &back2));
    assert_eq!(back1.bbox, lines1.bbox, "nur Quader 1");
    assert!(counts.windows(2).all(|w| w[1] > w[0]), "{counts:?}");

    // Angelegte Zeichnung zurückgenommen: der Bereich geht zum Modell.
    let with_drawing = st.snapshot();
    assert_eq!(st.drawing_ui.area, WorkArea::Drawing(d));
    st.restore_document(without_drawing).unwrap();
    assert_eq!(st.drawing_ui.area, WorkArea::Model);
    assert_eq!(st.drawing_ui.selected_drawing, None);
    tick(&mut st, true);
    assert_eq!(st.drawing_ui.area, WorkArea::Model);
    // Wiederholt: Die Zeichnung ist wieder da und lässt sich öffnen.
    st.restore_document(with_drawing.clone()).unwrap();
    assert_eq!(st.drawing_ui.area, WorkArea::Model);
    st.drawing_ui.show_drawing(d);
    tick(&mut st, true);
    assert_eq!(ready(&st, ViewKind::Front).bbox, lines1.bbox);

    // Gelöschte Zeichnung zurückgeholt, während sie (noch) als geöffnet galt: sie bleibt offen.
    st.document.delete_drawing(d).unwrap();
    st.restore_document(with_drawing).unwrap();
    assert_eq!(st.drawing_ui.area, WorkArea::Drawing(d));
    tick(&mut st, true);
    assert_eq!(st.drawing_ui.area, WorkArea::Drawing(d));
    assert_eq!(ready(&st, ViewKind::Front).bbox, lines1.bbox);
}

#[test]
fn forget_missing_clears_only_missing_targets() {
    let mut st = AppState::new();
    cube(&mut st);
    let d = new_drawing(&mut st, "Blatt 1");
    let kept = add_view(&mut st, d, ViewKind::Front, 100.0, 100.0);
    let picture = st.snapshot();
    let gone = add_view(&mut st, d, ViewKind::Top, 100.0, 40.0);
    let mut ui = DrawingUi::default();
    ui.open(d);
    ui.zoom = 2.0;
    ui.selected_view = Some(gone);
    ui.selected_drawing = Some(d);
    let mut doc = planora_feature::Document::from_snapshot(picture, 0.05).unwrap();
    ui.forget_missing(&doc);
    assert_eq!(ui.selected_view, None);
    assert_eq!(ui.area, WorkArea::Drawing(d));
    assert_eq!(ui.selected_drawing, Some(d));
    assert_eq!(ui.zoom, 2.0);
    ui.selected_view = Some(kept);
    ui.selected_dim = Some(DimId(7));
    ui.selected_balloon = Some(planora_drawing::BalloonId(3));
    ui.forget_missing(&doc);
    assert_eq!(ui.selected_view, Some(kept));
    assert_eq!(ui.selected_dim, None);
    assert_eq!(ui.selected_balloon, None);
    // Ohne die Zeichnung: zurück zum Modell, alles gewählte entfällt.
    ui.selected_view = Some(kept);
    doc.delete_drawing(d).unwrap();
    ui.forget_missing(&doc);
    assert_eq!(ui.area, WorkArea::Model);
    assert_eq!(ui.selected_drawing, None);
    assert_eq!(ui.selected_view, None);
}

#[test]
fn open_sketch_is_discarded_and_model_camera_returns() {
    // Kommt über die Oberfläche nicht vor (R10: bei offener Skizze wirkt der Skizzenverlauf);
    // der Zustand bleibt trotzdem stimmig: Skizze verworfen, Kamera von vor dem Skizzenmodus.
    let mut st = AppState::new();
    cube(&mut st);
    let before_sketch = st.snapshot();
    let sketch = st
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    let picture = st.snapshot();
    st.camera.yaw += 0.5;
    let model_camera = st.camera;
    for (picture, kind) in [
        (picture, RestoreKind::Annotations),
        (before_sketch, RestoreKind::Full),
    ] {
        st.open_sketch(sketch).unwrap();
        assert_ne!(st.camera, model_camera);
        assert_ne!(scene_text(&st), plain_scene(&st), "Skizzenmodus dämpft");
        assert_eq!(st.restore_document(picture), Ok(kind));
        assert!(st.sketch_session.is_none());
        assert!(st.sketch_draft.is_none());
        assert_eq!(st.camera, model_camera);
        // Die Szene ist nicht mehr gedämpft.
        assert_eq!(scene_text(&st), plain_scene(&st));
    }
    assert!(st.document.feature(sketch).is_none());
}

/// Einheitswürfel als Netz (Dreiecke von außen gegen den Uhrzeigersinn).
fn cube_mesh() -> planora_exchange::ExchangeMesh {
    planora_exchange::ExchangeMesh {
        positions: vec![
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
            [1.0, 0.0, 1.0],
            [1.0, 1.0, 1.0],
            [0.0, 1.0, 1.0],
        ],
        indices: vec![
            0, 2, 1, 0, 3, 2, 4, 5, 6, 4, 6, 7, 0, 1, 5, 0, 5, 4, 3, 7, 6, 3, 6, 2, 0, 4, 7, 0, 7,
            3, 1, 2, 6, 1, 6, 5,
        ],
    }
}

#[test]
fn mesh_reports_follow_the_restored_meshes() {
    // Der volle Weg verwirft Netze, auf die kein Feature mehr verweist. Ein danach eingelesenes
    // Netz darf nie den Prüfbericht eines verworfenen tragen.
    let mut st = AppState::new();
    let empty = st.snapshot();
    st.import_mesh("Würfel", Arc::new(cube_mesh()), 10.0)
        .unwrap();
    let with_cube = st.snapshot();
    assert_eq!(st.bodies[0].mesh_report.unwrap().triangles, 12);
    assert_eq!(st.restore_document(empty), Ok(RestoreKind::Full));
    assert!(st.bodies.is_empty());
    let mut open = cube_mesh();
    open.indices.truncate(9);
    st.import_mesh("Offen", Arc::new(open), 10.0).unwrap();
    let report = st.bodies[0].mesh_report.unwrap();
    assert_eq!(report.triangles, 3);
    assert!(!report.closed);
    // Wiederholen holt den Würfel mit seinem eigenen Bericht zurück.
    assert_eq!(st.restore_document(with_cube), Ok(RestoreKind::Full));
    let report = st.bodies[0].mesh_report.unwrap();
    assert_eq!(report.triangles, 12);
    assert!(report.closed);
}
