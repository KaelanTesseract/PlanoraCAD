//! Skizzen 10, Task 20: Verschieben/Kopieren in 3D in der App, über die Sitzung wie die
//! Oberfläche. Z-Abstand, X- und Y-Winkel und die Griffe dafür gibt es nur mit „3D-Skizze“ (A1);
//! eine Bewegung ist ein Rückgängig-Schritt, Esc hinterlässt nichts.

use planora_app::sketch_grips::{self, Grip};
use planora_app::sketch_input::FieldKind as F;
use planora_app::sketch_state::{Item, Tool};
use planora_app::state::{AppState, StandardPlane};
use planora_feature::SketchPlane;
use planora_i18n::{t, tr};
use planora_math::{DVec2, DVec3};
use planora_sketch::constraint::ConstraintKind as K;
use planora_sketch::helpers::RectIds;
use planora_sketch::model::{CurveKind, Sketch};
use planora_sketch::solver::SolveStatus;

fn v(x: f64, y: f64) -> DVec2 {
    DVec2::new(x, y)
}

/// Rechteck 20 × 10, alle Seiten vorgewählt, Verschieben/Kopieren aktiv; `three_d` setzt den
/// Haken „3D-Skizze“, `extra` darf noch etwas anlegen.
fn active(three_d: bool, extra: impl FnOnce(&mut Sketch, &RectIds)) -> (AppState, RectIds) {
    let mut s = AppState::new();
    let i = s
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    s.open_sketch(i).unwrap();
    let ids = {
        let sk = s.session_sketch_mut().unwrap().1;
        sk.set_three_d(three_d);
        let ids = sk.add_rectangle(v(0.0, 0.0), v(20.0, 10.0)).unwrap();
        extra(sk, &ids);
        assert_eq!(sk.solve().status, SolveStatus::Solved);
        ids
    };
    s.observe_sketch(false);
    let (se, sk) = s.session_sketch_mut().unwrap();
    se.selection = ids.lines.iter().map(|&c| Item::Curve(c)).collect();
    se.set_tool(sk, Tool::MoveCopy);
    se.sync_input(sk);
    (s, ids)
}

fn sketch(s: &AppState) -> &Sketch {
    s.session_sketch().unwrap()
}

fn kinds(s: &AppState) -> Vec<F> {
    let se = s.sketch_session.as_ref().unwrap();
    se.input.fields.iter().map(|f| f.kind).collect()
}

fn type_in(s: &mut AppState, kind: F, text: &str) {
    let scope = s.document.param_scope();
    let (se, sk) = s.session_sketch_mut().unwrap();
    se.sync_input(sk);
    let i = se
        .input
        .fields
        .iter()
        .position(|f| f.kind == kind)
        .expect("Feld");
    se.type_field(i, text, &scope);
}

fn finish(s: &mut AppState) -> Result<(), String> {
    let (se, sf) = s.session_draft_mut().unwrap();
    let result = se.confirm_transform(sf);
    s.observe_sketch(false);
    result
}

fn undo_len(s: &AppState) -> usize {
    s.sketch_history().unwrap().undo_len()
}

fn z_of(s: &AppState, ids: &RectIds) -> Vec<Option<f64>> {
    ids.points
        .iter()
        .map(|&p| {
            let sk = sketch(s);
            sk.is_3d_point(p).then(|| sk.point_xyz(p).unwrap().z)
        })
        .collect()
}

#[test]
fn z_fields_hidden_without_three_d() {
    let (mut s, _) = active(false, |_, _| {});
    assert_eq!(kinds(&s), [F::MoveX, F::MoveY, F::MoveAngle]);
    {
        let se = s.sketch_session.as_ref().unwrap();
        assert!(se.moving3(sketch(&s)).is_none());
    }
    // Der Haken kommt dazu: Die Felder für 3D erscheinen in der Reihenfolge von Fusion.
    {
        let (se, sk) = s.session_sketch_mut().unwrap();
        sk.set_three_d(true);
        se.sync_input(sk);
    }
    assert_eq!(
        kinds(&s),
        [
            F::MoveX,
            F::MoveY,
            F::MoveZ,
            F::MoveAngleX,
            F::MoveAngleY,
            F::MoveAngle
        ]
    );
    assert_eq!(F::MoveZ.label(), tr!("sketch-field-move-z"));
    // Ohne Haken nimmt die Sitzung keinen Griff für 3D an.
    let (mut s, _) = active(false, |_, _| {});
    let (se, _) = s.session_sketch_mut().unwrap();
    se.begin_grip3(Grip::AxisZ, DVec3::new(10.0, 5.0, 0.0));
    assert!(se.grip_drag.is_none());
}

#[test]
fn z_arrow_drag_moves_up() {
    let (mut s, ids) = active(true, |_, _| {});
    let undo = undo_len(&s);
    {
        let (se, sk) = s.session_sketch_mut().unwrap();
        let c = se.grip_center3().unwrap();
        assert!((c - DVec3::new(10.0, 5.0, 0.0)).length() < 1e-9);
        se.begin_grip3(Grip::AxisZ, c + DVec3::new(0.0, 0.0, 1.0));
        se.grip_to3(c + DVec3::new(3.0, -2.0, 8.0));
        se.end_grip();
        assert!(
            (se.transform.shift_z - 7.0).abs() < 1e-12,
            "nur die Höhe zählt"
        );
        assert_eq!(se.transform.offset, DVec2::ZERO);
        let lines = se.transform_preview3_styled(sk).expect("Vorschau in 3D");
        assert_eq!(lines.len(), 4);
        assert!(
            lines
                .iter()
                .flat_map(|(_, l)| l)
                .all(|p| (p.z - 7.0).abs() < 1e-9)
        );
    }
    finish(&mut s).unwrap();
    assert_eq!(z_of(&s, &ids), vec![Some(7.0); 4]);
    assert_eq!(undo_len(&s), undo + 1);
}

#[test]
fn rotate_x_arc_turns_the_rectangle_up() {
    let (mut s, ids) = active(true, |_, _| {});
    {
        let (se, _) = s.session_sketch_mut().unwrap();
        let c = se.grip_center3().unwrap();
        se.begin_grip3(Grip::RotateX, c + DVec3::new(0.0, 4.0, 0.0));
        se.grip_to3(c + DVec3::new(0.0, 0.0, 4.0));
        se.end_grip();
        assert!((se.transform.angle_x - std::f64::consts::FRAC_PI_2).abs() < 1e-12);
    }
    finish(&mut s).unwrap();
    // Um X durch (10, 5, 0): y − 5 wird z.
    let sk = sketch(&s);
    let p = sk.point_xyz(ids.points[2]).unwrap();
    assert!((p - DVec3::new(20.0, 5.0, 5.0)).length() < 1e-6, "{p:?}");
}

#[test]
fn move_message_names_removed_count() {
    let (mut s, ids) = active(true, |sk, ids| {
        let (a, b) = match sk.curve(ids.lines[0]).unwrap().kind {
            CurveKind::Line { a, b } => (a, b),
            _ => unreachable!(),
        };
        sk.add_constraint(K::DistanceX(a, b, 20.0)).unwrap();
        sk.add_constraint(K::Distance(a, b, 20.0)).unwrap();
    });
    type_in(&mut s, F::MoveZ, "5");
    finish(&mut s).unwrap();
    assert_eq!(z_of(&s, &ids), vec![Some(5.0); 4]);
    let text = s
        .sketch_session
        .as_ref()
        .unwrap()
        .message_text()
        .expect("Meldung");
    assert!(text.contains(&tr!("sketch-move-dropped", n = 1)), "{text}");
    assert!(
        text.contains(&tr!("sketch-move-converted", n = 1)),
        "{text}"
    );
}

#[test]
fn move_is_one_undo_step() {
    let (mut s, ids) = active(true, |_, _| {});
    let undo = undo_len(&s);
    type_in(&mut s, F::MoveZ, "4");
    type_in(&mut s, F::MoveAngleY, "30");
    finish(&mut s).unwrap();
    assert_eq!(undo_len(&s), undo + 1, "ein Schritt");
    assert!(z_of(&s, &ids).iter().all(Option::is_some));
    assert_eq!(s.sketch_undo_label().unwrap(), t("undo-move-copy"));
    assert!(s.undo_sketch());
    assert_eq!(z_of(&s, &ids), vec![None; 4], "zurück in der Ebene, 2D");
    assert_eq!(sketch(&s).profiles().profiles.len(), 1);
}

#[test]
fn esc_leaves_nothing() {
    let (mut s, ids) = active(true, |_, _| {});
    let undo = undo_len(&s);
    type_in(&mut s, F::MoveZ, "4");
    {
        let (se, sk) = s.session_sketch_mut().unwrap();
        let c = se.grip_center3().unwrap();
        se.begin_grip3(Grip::RotateY, c + DVec3::X);
        se.grip_to3(c + DVec3::Z);
        se.abort_grip();
        assert_eq!(
            se.transform.angle_y, 0.0,
            "Rechtsklick nimmt den Zug zurück"
        );
        se.cancel(sk);
    }
    s.observe_sketch(false);
    assert_eq!(undo_len(&s), undo);
    assert_eq!(z_of(&s, &ids), vec![None; 4]);
}

#[test]
fn copy_out_of_the_plane_selects_the_copies() {
    let (mut s, ids) = active(true, |_, _| {});
    s.sketch_session.as_mut().unwrap().transform.copy = true;
    type_in(&mut s, F::MoveZ, "6");
    finish(&mut s).unwrap();
    assert_eq!(z_of(&s, &ids), vec![None; 4], "das Original bleibt");
    let se = s.sketch_session.as_ref().unwrap();
    assert_eq!(se.selection.len(), 4);
    for item in &se.selection {
        let Item::Curve(c) = *item else {
            panic!("Kurve")
        };
        assert!(sketch(&s).is_3d_curve(c));
    }
}

#[test]
fn grip_motion3_measures_turns_about_their_axis() {
    let d = sketch_grips::GripDrag3 {
        grip: Grip::RotateY,
        press: DVec3::new(1.0, 7.0, 0.0),
        axis: DVec3::Y,
        start_z: 2.0,
        start_angle_x: 0.1,
        start_angle_y: 0.2,
    };
    // Um Y von +x nach −z ist +90° (rechte Hand); der Anteil längs der Achse zählt nicht.
    let (z, ax, ay) = sketch_grips::grip_motion3(&d, DVec3::ZERO, DVec3::new(0.0, -3.0, -1.0));
    assert_eq!((z, ax), (2.0, 0.1));
    assert!(
        (ay - (0.2 + std::f64::consts::FRAC_PI_2)).abs() < 1e-12,
        "{ay}"
    );
}

// ---- Fix-Runde 1 ---------------------------------------------------------------------------------

/// Wie `rotation3` angewandt um `pivot`.
fn turned(p: DVec3, pivot: DVec3, rot: planora_math::DQuat) -> DVec3 {
    pivot + rot * (p - pivot)
}

/// Erst 90° um Z, dann am X-Bogen ziehen: Der Bogen liegt um die gedrehte Achse (Rz·X = Welt-Y),
/// und genau um sie dreht die Geometrie.
#[test]
fn x_arc_after_z_turn_turns_about_the_shown_axis() {
    let (mut s, ids) = active(true, |_, _| {});
    type_in(&mut s, F::MoveAngle, "90");
    let before: Vec<DVec3> = ids
        .points
        .iter()
        .map(|&p| sketch(&s).point_xyz(p).unwrap())
        .collect();
    let (pivot, axis) = {
        let (se, _) = s.session_sketch_mut().unwrap();
        let c = se.grip_center3().unwrap();
        let [(axis, u, w), _] = se.grip_arc_axes();
        assert!((axis - DVec3::Y).length() < 1e-12, "{axis:?}");
        se.begin_grip3(Grip::RotateX, c + u * 4.0);
        se.grip_to3(c + w * 4.0);
        se.end_grip();
        assert!((se.transform.angle_x - std::f64::consts::FRAC_PI_2).abs() < 1e-12);
        (c, axis)
    };
    finish(&mut s).unwrap();
    // Ergebnis = 90° um die gezeigte Achse, nach den 90° um Z.
    let shown = planora_math::DQuat::from_axis_angle(axis, std::f64::consts::FRAC_PI_2)
        * planora_math::DQuat::from_rotation_z(std::f64::consts::FRAC_PI_2);
    for (&p, b) in ids.points.iter().zip(&before) {
        let got = sketch(&s).point_xyz(p).unwrap();
        let want = turned(*b, pivot, shown);
        assert!((got - want).length() < 1e-6, "{got:?} statt {want:?}");
    }
}

/// Zusammengesetzte Drehung aus den Feldern: Das Ergebnis ist Rz·Ry·Rx um den Bezugspunkt, und
/// jeder Bogen liegt um die Achse, um die sein Feldwinkel dreht.
#[test]
fn composite_field_turn_matches_the_result_and_the_arcs() {
    let (mut s, ids) = active(true, |_, _| {});
    type_in(&mut s, F::MoveAngleX, "30");
    type_in(&mut s, F::MoveAngleY, "20");
    type_in(&mut s, F::MoveAngle, "40");
    type_in(&mut s, F::MoveZ, "3");
    let before: Vec<DVec3> = ids
        .points
        .iter()
        .map(|&p| sketch(&s).point_xyz(p).unwrap())
        .collect();
    let (ax, ay, az) = (30f64.to_radians(), 20f64.to_radians(), 40f64.to_radians());
    let rot = sketch_grips::rotation3(ax, ay, az);
    let (pivot, arcs) = {
        let se = s.sketch_session.as_ref().unwrap();
        let m = se.transform_now3().unwrap();
        assert!((m.rot.dot(rot).abs() - 1.0).abs() < 1e-12);
        (m.pivot, se.grip_arc_axes())
    };
    // Ein kleiner Zusatz am X-Winkel ist eine Drehung um die Achse des X-Bogens (von links).
    let d = 1e-3;
    let more_x = sketch_grips::rotation3(ax + d, ay, az) * rot.inverse();
    let (axis, angle) = more_x.to_axis_angle();
    assert!((angle - d).abs() < 1e-9 && (axis - arcs[0].0).length() < 1e-6);
    let more_y = sketch_grips::rotation3(ax, ay + d, az) * rot.inverse();
    let (axis, _) = more_y.to_axis_angle();
    assert!((axis - arcs[1].0).length() < 1e-6, "{axis:?}");
    finish(&mut s).unwrap();
    for (&p, b) in ids.points.iter().zip(&before) {
        let got = sketch(&s).point_xyz(p).unwrap();
        let want = turned(*b, pivot, rot) + DVec3::new(0.0, 0.0, 3.0);
        assert!((got - want).length() < 1e-6, "{got:?} statt {want:?}");
    }
}

/// Pfeile X/Y, Ring, Z-Pfeil und Bögen liegen an derselben Stelle, auch nach einem Z-Zug in
/// schräger Ansicht.
#[test]
fn grip_cross_stays_together_after_a_z_move() {
    use egui::{Rect, pos2};
    let screen = Rect::from_min_max(pos2(0.0, 0.0), pos2(1920.0, 1080.0));
    let (mut s, _) = active(true, |_, _| {});
    s.camera.yaw = 0.7;
    s.camera.pitch = 0.5;
    s.sketch_session.as_mut().unwrap().transform.shift_z = 7.0;
    let flat = planora_app::sketch_view::grip_layout(&s, screen).expect("Griffe");
    let up = planora_app::sketch_view::grip_layout3(&s, screen).expect("Griffe 3D");
    assert!(
        flat.center.distance(up.center) < 0.01,
        "{:?} {:?}",
        flat.center,
        up.center
    );
}

/// Eine Kopie, die aus der Ebene nicht geht, meldet das sofort wie der 2D-Weg.
#[test]
fn a_failing_3d_copy_is_reported_at_once() {
    let (mut s, _) = active(true, |_, _| {});
    s.sketch_session.as_mut().unwrap().transform.copy = true;
    type_in(&mut s, F::MoveZ, "5000000000");
    let (se, sk) = s.session_sketch_mut().unwrap();
    se.update_transform_block(sk);
    assert!(
        se.message_text().is_some(),
        "Meldung schon vor dem Abschluss"
    );
    assert!(se.transform_preview3_styled(sk).is_none());
}

/// Ein neuer Bezugspunkt behält die Höhe der Menge.
#[test]
fn a_new_base_point_keeps_the_height() {
    let (mut s, ids) = active(true, |sk, ids| {
        let set = planora_sketch::transform::TransformSet {
            curves: ids.lines.to_vec(),
            points: Vec::new(),
        };
        let up = planora_sketch::transform::Rigid3 {
            rot: planora_math::DQuat::IDENTITY,
            shift: DVec3::new(0.0, 0.0, 6.0),
            pivot: DVec3::ZERO,
        };
        sk.move_rigid3(&set, &up, false).unwrap();
    });
    assert_eq!(z_of(&s, &ids), vec![Some(6.0); 4]);
    let (se, sk) = s.session_sketch_mut().unwrap();
    assert!((se.transform.base_z - 6.0).abs() < 1e-12);
    se.begin_set_base();
    se.click(sk, v(20.0, 10.0), 0.5, false).unwrap();
    assert_eq!(se.transform.base, Some(v(20.0, 10.0)));
    assert!(
        (se.transform.base_z - 6.0).abs() < 1e-12,
        "{}",
        se.transform.base_z
    );
}
