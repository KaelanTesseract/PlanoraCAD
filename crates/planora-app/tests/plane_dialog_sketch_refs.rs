//! Skizzen 12, Task 10: Skizzenlinien und -punkte fertiger Skizzen als Achse bzw. Punkt in den
//! Ebenen-Fenstern („Ebene unter Winkel“, „Ebene durch drei Punkte“). Bild 1920×1080.

use glam::{Vec2, Vec3};
use planora_app::plane_dialog::{PlaneClick, PlaneCommand, PlaneDialog, SlotValue};
use planora_app::sketch_state::Tool;
use planora_app::state::{AppState, StandardPlane};
use planora_feature::planes::{AxisRef, PlaneDef, PointRef, StdAxis};
use planora_feature::{FeatureId, SketchPlane};
use planora_math::DVec3;
use planora_render::Pick;
use planora_sketch::model::{CurveId, CurveKind, PointId};

const VIEW: Vec2 = Vec2::new(1920.0, 1080.0);

/// Kamera von oben, Ziel in der Mitte der Zeichnung.
fn look_from_above(s: &mut AppState) {
    s.camera = planora_render::OrbitCamera {
        target: Vec3::new(10.0, 5.0, 0.0),
        distance: 100.0,
        pitch: 1.5,
        ..planora_render::OrbitCamera::default()
    };
    s.camera.set_viewport(VIEW.x, VIEW.y);
}

/// Fertige 3D-Skizze mit einer Linie von `a` nach `b`.
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

/// Fertige Skizze mit einem Kreis (Mitte im Ursprung, Radius 10).
fn finished_circle(s: &mut AppState) -> (FeatureId, CurveId) {
    let id = s
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    s.open_sketch(id).unwrap();
    s.observe_sketch(false);
    let c = {
        let (_, sk) = s.session_sketch_mut().unwrap();
        let m = sk.add_point(0.0, 0.0).unwrap();
        sk.add_circle(m, 10.0).unwrap()
    };
    s.observe_sketch(false);
    s.close_sketch();
    (id, c)
}

fn at(s: &AppState, p: DVec3) -> Vec2 {
    s.camera
        .project(Vec3::new(p.x as f32, p.y as f32, p.z as f32), VIEW)
        .expect("im Bild")
}

/// Das aktive Feld bekommt den Klick an `cursor` mit seinem eigenen Filter.
fn click_slot(d: &mut PlaneDialog, s: &mut AppState, world: DVec3) -> Option<Pick> {
    let cursor = at(s, world);
    let _ = d.sync(s);
    let filter = s.plane_pick.unwrap().filter;
    let hit = s.pick_with(cursor, VIEW, filter);
    if let Some(h) = hit {
        s.plane_pick.as_mut().unwrap().clicked = Some(PlaneClick::View(h));
        let _ = d.sync(s);
    }
    hit
}

#[test]
fn angle_plane_takes_sketch_line_as_axis() {
    let mut s = AppState::new();
    let (id, c, _) = finished_line(
        &mut s,
        DVec3::new(0.0, 0.0, 0.0),
        DVec3::new(20.0, 0.0, 0.0),
    );
    look_from_above(&mut s);
    let mut d = PlaneDialog::new(PlaneCommand::Angle);
    let hit = click_slot(&mut d, &mut s, DVec3::new(10.0, 0.0, 0.0));
    assert_eq!(
        hit,
        Some(Pick::SketchCurve {
            sketch: id,
            curve: c
        })
    );
    assert_eq!(d.error(), None);
    assert_eq!(
        d.slot(0),
        Some(&SlotValue::Axis(AxisRef::SketchLine {
            sketch: id,
            curve: c
        }))
    );
    // Die Ebene lässt sich anlegen und geht durch die Linie.
    let plane = d.ok(&mut s).unwrap();
    let frame = s.document.plane_frame(plane).unwrap();
    assert!(frame.origin.z.abs() < 1e-9 && frame.origin.y.abs() < 1e-9);
}

#[test]
fn arc_is_refused_as_axis() {
    let mut s = AppState::new();
    let (id, c) = finished_circle(&mut s);
    look_from_above(&mut s);
    let mut d = PlaneDialog::new(PlaneCommand::Angle);
    let hit = click_slot(&mut d, &mut s, DVec3::new(10.0, 0.0, 0.0));
    assert_eq!(
        hit,
        Some(Pick::SketchCurve {
            sketch: id,
            curve: c
        }),
        "{hit:?}"
    );
    assert_eq!(d.slot(0), None, "Feld bleibt leer");
    let e = d.error().expect("Meldung");
    assert!(e.contains("passt hier nicht"), "{e}");
}

#[test]
fn three_point_plane_takes_sketch_points() {
    let mut s = AppState::new();
    let (id, _, [p, q]) = finished_line(
        &mut s,
        DVec3::new(0.0, 0.0, 0.0),
        DVec3::new(20.0, 0.0, 0.0),
    );
    let (id2, _, [r, _]) = finished_line(
        &mut s,
        DVec3::new(0.0, 10.0, 0.0),
        DVec3::new(20.0, 10.0, 0.0),
    );
    look_from_above(&mut s);
    let mut d = PlaneDialog::new(PlaneCommand::ThreePoints);
    for (world, want) in [
        (DVec3::new(0.0, 0.0, 0.0), (id, p)),
        (DVec3::new(20.0, 0.0, 0.0), (id, q)),
        (DVec3::new(0.0, 10.0, 0.0), (id2, r)),
    ] {
        let hit = click_slot(&mut d, &mut s, world);
        assert_eq!(
            hit,
            Some(Pick::SketchPoint {
                sketch: want.0,
                point: want.1
            }),
            "{hit:?}"
        );
        assert_eq!(d.error(), None);
    }
    assert_eq!(
        d.slot(0),
        Some(&SlotValue::Point(PointRef::SketchPoint {
            sketch: id,
            point: p
        }))
    );
    assert!(matches!(
        d.slot(2),
        Some(SlotValue::Point(PointRef::SketchPoint { .. }))
    ));
    let plane = d.ok(&mut s).unwrap();
    assert!(s.document.plane_frame(plane).is_some());
}

#[test]
fn editing_refuses_later_sketch() {
    let mut s = AppState::new();
    // Die Ebene entsteht vor der Skizze.
    let plane = s
        .add_plane(PlaneDef::Angle {
            axis: AxisRef::Standard(StdAxis::X),
            angle: 30.0,
        })
        .unwrap();
    finished_line(
        &mut s,
        DVec3::new(0.0, 0.0, 0.0),
        DVec3::new(20.0, 0.0, 0.0),
    );
    look_from_above(&mut s);
    let mut d = PlaneDialog::edit(&s, plane).unwrap();
    let hit = click_slot(&mut d, &mut s, DVec3::new(10.0, 0.0, 0.0));
    assert!(matches!(hit, Some(Pick::SketchCurve { .. })), "{hit:?}");
    assert_eq!(
        d.slot(0),
        Some(&SlotValue::Axis(AxisRef::Standard(StdAxis::X))),
        "Feld unverändert"
    );
    let e = d.error().expect("Meldung");
    assert!(e.contains("nach der Ebene"), "{e}");
    // Eine neue Ebene darf die Linie nehmen.
    let mut n = PlaneDialog::new(PlaneCommand::Angle);
    click_slot(&mut n, &mut s, DVec3::new(10.0, 0.0, 0.0));
    assert!(matches!(
        n.slot(0),
        Some(SlotValue::Axis(AxisRef::SketchLine { .. }))
    ));
}

#[test]
fn axis_field_takes_the_line_near_its_end() {
    let mut s = AppState::new();
    let (id, c, _) = finished_line(
        &mut s,
        DVec3::new(0.0, 0.0, 0.0),
        DVec3::new(20.0, 0.0, 0.0),
    );
    look_from_above(&mut s);
    let mut d = PlaneDialog::new(PlaneCommand::Angle);
    // Genau am Ende: die normale Auswahl träfe den Punkt, das Achsenfeld die Linie.
    let hit = click_slot(&mut d, &mut s, DVec3::new(0.1, 0.0, 0.0));
    assert_eq!(
        hit,
        Some(Pick::SketchCurve {
            sketch: id,
            curve: c
        }),
        "{hit:?}"
    );
    assert_eq!(d.error(), None);
    assert!(matches!(
        d.slot(0),
        Some(SlotValue::Axis(AxisRef::SketchLine { .. }))
    ));
}

#[test]
fn point_field_ignores_curves() {
    let mut s = AppState::new();
    let (id, _, [p, _]) = finished_line(
        &mut s,
        DVec3::new(0.0, 0.0, 0.0),
        DVec3::new(20.0, 0.0, 0.0),
    );
    look_from_above(&mut s);
    let mut d = PlaneDialog::new(PlaneCommand::ThreePoints);
    // Mitten auf der Linie: kein Punkt in der Nähe, also nichts und keine Meldung.
    let hit = click_slot(&mut d, &mut s, DVec3::new(10.0, 0.0, 0.0));
    assert_eq!(hit, None);
    assert_eq!(d.error(), None);
    assert_eq!(d.slot(0), None);
    // Nahe am Ende: der Punkt.
    let hit = click_slot(&mut d, &mut s, DVec3::new(0.1, 0.0, 0.0));
    assert_eq!(
        hit,
        Some(Pick::SketchPoint {
            sketch: id,
            point: p
        })
    );
}

#[test]
fn editing_refuses_later_sketch_point() {
    let mut s = AppState::new();
    let plane = s
        .add_plane(PlaneDef::ThreePoints([
            PointRef::Origin,
            PointRef::Origin,
            PointRef::Origin,
        ]))
        .unwrap();
    finished_line(
        &mut s,
        DVec3::new(0.0, 0.0, 0.0),
        DVec3::new(20.0, 0.0, 0.0),
    );
    look_from_above(&mut s);
    let mut d = PlaneDialog::edit(&s, plane).unwrap();
    let hit = click_slot(&mut d, &mut s, DVec3::new(20.0, 0.0, 0.0));
    assert!(matches!(hit, Some(Pick::SketchPoint { .. })), "{hit:?}");
    assert_eq!(d.slot(0), Some(&SlotValue::Point(PointRef::Origin)));
    let e = d.error().expect("Meldung");
    assert!(e.contains("nach der Ebene"), "{e}");
}
