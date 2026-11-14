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
