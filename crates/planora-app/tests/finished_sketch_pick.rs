//! Skizzen 12, Task 8: Fertige Skizzen in 3D anklicken. Punkte vor Kurven, die nächste gewinnt,
//! Körperkante vor Skizze, verdeckte Stellen und ausgeblendete Skizzen zählen nicht; das
//! Rechtsklickmenü bearbeitet oder blendet die Skizze aus, Entf tut nichts. Bei 1920 × 1080.

use glam::{Vec2, Vec3};
use planora_app::commands;
use planora_app::keymap::Action;
use planora_app::sketch_state::Tool;
use planora_app::state::{AppState, PickFilter, StandardPlane};
use planora_app::workspace::Workspace;
use planora_feature::{FeatureId, GroupItem, SketchPlane};
use planora_math::DVec3;
use planora_project::config::Config;
use planora_render::Pick;
use planora_sketch::model::{CurveId, CurveKind, PointId};

const VIEW: Vec2 = Vec2::new(1920.0, 1080.0);

/// Quader 10 × 20 × 5 (z 0..5), Blick von schräg oben auf seine Mitte.
fn scene_with_box() -> AppState {
    let mut s = AppState::new();
    s.create_box(DVec3::new(10.0, 20.0, 5.0)).unwrap();
    look_from_above(&mut s);
    s
}

fn look_from_above(s: &mut AppState) {
    s.camera.target = Vec3::new(10.0, 10.0, 2.5);
    s.camera.distance = 100.0;
    s.camera.yaw = 0.3;
    s.camera.pitch = 1.5;
}

/// Eine fertige 3D-Skizze auf XY mit einer Linie von `a` nach `b` (Skizzenkoordinaten = Welt);
/// liefert Skizze, Linie und die beiden Endpunkte.
fn finished_line(s: &mut AppState, a: DVec3, b: DVec3) -> (FeatureId, CurveId, [PointId; 2]) {
    let id = s
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    s.open_sketch(id).unwrap();
    s.observe_sketch(false);
    {
        let (se, sk) = s.session_sketch_mut().unwrap();
        se.set_three_d(sk, true);
        se.choose_tool(sk, Tool::Line);
        se.place_point_3d(sk, a, None).unwrap();
        se.place_point_3d(sk, b, None).unwrap();
        se.choose_tool(sk, Tool::Select);
    }
    s.observe_sketch(false);
    let (c, p, q) = s
        .session_sketch()
        .unwrap()
        .curves()
        .find_map(|(c, d)| match d.kind {
            CurveKind::Line { a, b } => Some((c, a, b)),
            _ => None,
        })
        .expect("Linie");
    s.close_sketch();
    (id, c, [p, q])
}

/// Bildschirmort eines Weltpunkts.
fn at(s: &AppState, p: DVec3) -> Vec2 {
    s.camera
        .project(Vec3::new(p.x as f32, p.y as f32, p.z as f32), VIEW)
        .expect("im Bild")
}

fn select(s: &AppState, cursor: Vec2) -> Option<Pick> {
    s.pick_with(cursor, VIEW, PickFilter::SELECT)
}

#[test]
fn click_selects_finished_sketch_curve() {
    let mut s = scene_with_box();
    let (id, c, _) = finished_line(
        &mut s,
        DVec3::new(20.0, 0.0, 0.0),
        DVec3::new(20.0, 10.0, 0.0),
    );
    let mid = at(&s, DVec3::new(20.0, 5.0, 0.0));
    assert_eq!(
        select(&s, mid),
        Some(Pick::SketchCurve {
            sketch: id,
            curve: c
        })
    );
    // Weit daneben: nichts von der Skizze.
    let away = at(&s, DVec3::new(20.0, 5.0, 0.0)) + Vec2::new(60.0, 0.0);
    assert!(!matches!(
        select(&s, away),
        Some(Pick::SketchCurve { .. } | Pick::SketchPoint { .. })
    ));
    // Die Auswahl übernimmt die Kurve und ersetzt keinen Körper.
    s.set_selection(select(&s, mid));
    assert_eq!(
        s.selection,
        Some(Pick::SketchCurve {
            sketch: id,
            curve: c
        })
    );
    // Die anderen Filter finden keine Skizzen.
    assert!(!matches!(
        s.pick_with(mid, VIEW, PickFilter::GEOMETRY),
        Some(Pick::SketchCurve { .. })
    ));
}

#[test]
fn point_wins_over_curve() {
    let mut s = scene_with_box();
    let a = DVec3::new(20.0, 0.0, 0.0);
    let (id, c, [p, q]) = finished_line(&mut s, a, DVec3::new(20.0, 10.0, 0.0));
    // Am Endpunkt liegt die Kurve auch im Fangkreis; der Punkt gewinnt.
    let end = at(&s, a);
    let got = select(&s, end + Vec2::new(2.0, 2.0));
    assert!(
        matches!(got, Some(Pick::SketchPoint { sketch, point }) if sketch == id && (point == p || point == q)),
        "Punkt erwartet: {got:?}"
    );
    // In der Mitte trifft nur die Kurve.
    let mid = at(&s, DVec3::new(20.0, 5.0, 0.0));
    assert_eq!(
        select(&s, mid),
        Some(Pick::SketchCurve {
            sketch: id,
            curve: c
        })
    );
}

#[test]
fn occluded_curve_is_not_pickable() {
    let mut s = scene_with_box();
    // Unter dem Quader: von oben vom Körper verdeckt.
    finished_line(
        &mut s,
        DVec3::new(5.0, 5.0, -10.0),
        DVec3::new(5.0, 15.0, -10.0),
    );
    let mid = at(&s, DVec3::new(5.0, 10.0, -10.0));
    let got = select(&s, mid);
    assert!(
        !matches!(
            got,
            Some(Pick::SketchCurve { .. } | Pick::SketchPoint { .. })
        ),
        "verdeckt: {got:?}"
    );
    // Der Körper darüber wird getroffen.
    assert!(matches!(got, Some(Pick::Face(_))));
}

#[test]
fn visible_part_of_partly_covered_curve_is_pickable() {
    let mut s = scene_with_box();
    let (id, c, _) = finished_line(
        &mut s,
        DVec3::new(5.0, 10.0, -10.0),
        DVec3::new(45.0, 10.0, -10.0),
    );
    let covered = at(&s, DVec3::new(6.0, 10.0, -10.0));
    assert!(!matches!(
        select(&s, covered),
        Some(Pick::SketchCurve { .. })
    ));
    let free = at(&s, DVec3::new(35.0, 10.0, -10.0));
    assert_eq!(
        select(&s, free),
        Some(Pick::SketchCurve {
            sketch: id,
            curve: c
        })
    );
}

#[test]
fn hidden_sketch_is_not_pickable() {
    let mut s = scene_with_box();
    let (id, c, _) = finished_line(
        &mut s,
        DVec3::new(20.0, 0.0, 0.0),
        DVec3::new(20.0, 10.0, 0.0),
    );
    let mid = at(&s, DVec3::new(20.0, 5.0, 0.0));
    let pick = Pick::SketchCurve {
        sketch: id,
        curve: c,
    };
    assert_eq!(select(&s, mid), Some(pick));

    // Auge der Skizze.
    s.set_selection(Some(pick));
    s.set_feature_hidden(id, true);
    assert!(!matches!(select(&s, mid), Some(Pick::SketchCurve { .. })));
    assert_eq!(s.selection, None, "Auswahl auf Ausgeblendetem entfällt");
    s.set_feature_hidden(id, false);
    assert_eq!(select(&s, mid), Some(pick));

    // Ausgeblendete Gruppe.
    let g = s
        .document
        .create_sketch_group(&[GroupItem::Sketch(id)])
        .unwrap();
    s.set_sketch_group_hidden(g, true);
    assert!(!matches!(select(&s, mid), Some(Pick::SketchCurve { .. })));
    s.set_sketch_group_hidden(g, false);
    assert_eq!(select(&s, mid), Some(pick));

    // Ordner „Skizzen“ aus.
    s.toggle_folder_hidden(planora_app::browser::Folder::Sketches);
    assert!(!matches!(select(&s, mid), Some(Pick::SketchCurve { .. })));
}

#[test]
fn body_edge_wins_over_sketch_curve() {
    let mut s = scene_with_box();
    // Eine Skizzenlinie genau auf der Kante vorn oben (x 0..10, y = 0, z = 5).
    finished_line(
        &mut s,
        DVec3::new(0.0, 0.0, 5.0),
        DVec3::new(10.0, 0.0, 5.0),
    );
    let mid = at(&s, DVec3::new(5.0, 0.0, 5.0));
    let got = select(&s, mid);
    assert!(
        matches!(got, Some(Pick::Edge(_))),
        "Körperkante vor Skizze: {got:?}"
    );
}

#[test]
fn context_menu_edit_sketch_opens_it() {
    let mut w = Workspace::new(None, Config::default());
    let (id, c) = {
        let s = &mut w.active_tab_mut().state;
        s.create_box(DVec3::new(10.0, 20.0, 5.0)).unwrap();
        let (id, c, _) = finished_line(s, DVec3::new(20.0, 0.0, 0.0), DVec3::new(20.0, 10.0, 0.0));
        (id, c)
    };
    let pick = Pick::SketchCurve {
        sketch: id,
        curve: c,
    };
    w.active_tab_mut().state.set_selection(Some(pick));
    assert!(w.active_tab().state.open_sketch_id().is_none());
    commands::run_action(&mut w, Action::SketchEditPicked);
    assert_eq!(
        w.active_tab().state.open_sketch_id(),
        Some(id),
        "„Skizze bearbeiten“ öffnet die Skizze"
    );

    // „Ausblenden“: das Auge der Skizze geht aus, die Auswahl entfällt.
    w.active_tab_mut().state.close_sketch();
    w.active_tab_mut().state.set_selection(Some(pick));
    commands::run_action(&mut w, Action::SketchHidePicked);
    let s = &w.active_tab().state;
    assert!(s.own_feature_hidden(id));
    assert_eq!(s.selection, None);
}

#[test]
fn delete_on_sketch_selection_does_nothing() {
    let mut w = Workspace::new(None, Config::default());
    let (id, c, features, bodies) = {
        let s = &mut w.active_tab_mut().state;
        s.create_box(DVec3::new(10.0, 20.0, 5.0)).unwrap();
        let (id, c, _) = finished_line(s, DVec3::new(20.0, 0.0, 0.0), DVec3::new(20.0, 10.0, 0.0));
        (id, c, s.document.features().len(), s.bodies.len())
    };
    let pick = Pick::SketchCurve {
        sketch: id,
        curve: c,
    };
    w.active_tab_mut().state.set_selection(Some(pick));
    commands::run_action(&mut w, Action::DeleteSelection);
    let s = &w.active_tab().state;
    assert_eq!(s.document.features().len(), features, "nichts gelöscht");
    assert_eq!(s.bodies.len(), bodies);
    assert_eq!(s.selection, Some(pick), "Auswahl bleibt");
    assert!(s.pending_delete.is_none(), "keine Rückfrage");
}
