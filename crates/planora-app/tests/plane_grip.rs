//! Ziehgriff an der Vorschau einer Konstruktionsebene (wie Fusion): Pfeil für den Abstand der
//! Versatzebene, Drehpfeil für den Winkel der Ebene unter Winkel. Mit echten Zeigerereignissen
//! durch Ansicht und Fenster im selben Kontext.

use egui::{Event, Key, Modifiers, PointerButton, Pos2, Rect, pos2, vec2};
use planora_app::plane_dialog::{PlaneClick, PlaneCommand, PlaneDialog, SlotValue};
use planora_app::plane_grip::{self, GripShape};
use planora_app::state::{AppState, StandardPlane};
use planora_app::{fonts, viewport};
use planora_feature::FeatureKind;
use planora_feature::planes::{AxisRef, PlaneDef, StdAxis};

const SCREEN: Rect = Rect {
    min: pos2(0.0, 0.0),
    max: pos2(1920.0, 1080.0),
};

struct Rig {
    ctx: egui::Context,
    s: AppState,
    d: PlaneDialog,
    open: bool,
}

impl Rig {
    fn new(d: PlaneDialog) -> Rig {
        let ctx = egui::Context::default();
        ctx.set_fonts(fonts::definitions());
        let mut s = AppState::new();
        s.create_box(planora_math::DVec3::splat(20.0)).unwrap();
        // Schräg von vorn oben: Normale und Achse stehen sichtbar im Bild.
        s.camera = planora_render::OrbitCamera {
            // Das Modell links im Bild: Die Mitte gehört dem Fenster.
            target: glam::Vec3::new(-140.0, 60.0, 0.0),
            distance: 400.0,
            pitch: 0.5,
            yaw: 0.6,
            ..planora_render::OrbitCamera::default()
        };
        s.camera.set_viewport(SCREEN.width(), SCREEN.height());
        let mut r = Rig {
            ctx,
            s,
            d,
            open: true,
        };
        r.frame(vec![]);
        r.frame(vec![]);
        r
    }

    fn frame(&mut self, events: Vec<Event>) {
        let raw = egui::RawInput {
            screen_rect: Some(SCREEN),
            events,
            ..Default::default()
        };
        let (s, d, open) = (&mut self.s, &mut self.d, &mut self.open);
        let out = self.ctx.run_ui(raw, |ui| {
            viewport::show_in(ui, s, None);
            *open = d.show(ui.ctx(), s);
        });
        out.drop_without_applying_deltas();
    }

    fn grip(&self) -> GripShape {
        self.s.plane_pick.and_then(|p| p.grip).expect("Griff").shape
    }

    fn grip_at(&self) -> Pos2 {
        let p = plane_grip::grip_point(&self.s, SCREEN).expect("Griff im Bild");
        assert!(
            p.x < 600.0 && SCREEN.contains(p),
            "Griff frei neben dem Fenster: {p:?}"
        );
        p
    }

    fn press(&mut self, p: Pos2) {
        self.frame(vec![Event::PointerMoved(p)]);
        self.frame(vec![button(p, true)]);
    }

    fn move_to(&mut self, p: Pos2) {
        self.frame(vec![Event::PointerMoved(p)]);
        self.frame(vec![]);
    }

    fn release(&mut self, p: Pos2) {
        self.frame(vec![button(p, false)]);
        self.frame(vec![]);
    }

    fn value(&self) -> f64 {
        match self.d.definition(&self.s).unwrap().0 {
            PlaneDef::Offset { distance, .. } => distance,
            PlaneDef::Angle { angle, .. } => angle,
            other => panic!("{other:?}"),
        }
    }
}

fn button(pos: Pos2, pressed: bool) -> Event {
    Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::default(),
    }
}

fn esc() -> Event {
    Event::Key {
        key: Key::Escape,
        physical_key: Some(Key::Escape),
        pressed: true,
        repeat: false,
        modifiers: Modifiers::default(),
    }
}

fn offset_rig() -> Rig {
    let mut d = PlaneDialog::new(PlaneCommand::Offset);
    let s = AppState::new();
    assert!(d.take_click(&s, PlaneClick::Standard(StandardPlane::XY)));
    Rig::new(d)
}

#[test]
fn dragging_the_arrow_sets_the_offset_and_ok_keeps_it() {
    let mut r = offset_rig();
    let GripShape::Distance { dir, value, .. } = r.grip() else {
        panic!("Pfeil erwartet")
    };
    assert_eq!(value, 0.0, "beginnt bei 0 (wie Fusion)");
    assert!(
        (dir - planora_math::DVec3::Z).length() < 1e-9,
        "entlang der Normale"
    );
    // Nach oben ziehen: Die Normale von XY zeigt auf dem Bildschirm nach oben.
    let p = r.grip_at();
    r.press(p);
    assert!(
        r.s.plane_pick.unwrap().grip.unwrap().drag.is_some(),
        "gegriffen"
    );
    let camera = r.s.camera;
    r.move_to(p - vec2(0.0, 120.0));
    assert_eq!(r.s.camera.yaw, camera.yaw, "die Ansicht dreht nicht mit");
    assert_eq!(r.s.camera.pitch, camera.pitch);
    let up = r.value();
    assert!(up > 5.0, "{up}");
    assert_eq!(
        (up * 100.0).round() / 100.0,
        up,
        "auf 0,01 mm gerundet (wie Fusion)"
    );
    // Die Vorschau wandert mit: Ihre Mitte liegt im neuen Abstand.
    let rect = r.s.plane_pick.unwrap().preview.unwrap();
    assert!((rect.frame.origin.z - up).abs() < 1e-6);
    // Der Pfeil sitzt an der Vorschau, die Rechnung aber gegen den Stand beim Greifen: weiter
    // nach oben heißt mehr.
    r.move_to(p - vec2(0.0, 200.0));
    let more = r.value();
    assert!(more > up, "{more} > {up}");
    r.release(p - vec2(0.0, 200.0));
    assert!(r.s.plane_pick.unwrap().grip.unwrap().drag.is_none());
    assert_eq!(r.value(), more, "Loslassen behält den Wert");
    // Nach unten: negativ.
    let p = r.grip_at();
    r.press(p);
    r.move_to(p + vec2(0.0, 400.0));
    r.release(p + vec2(0.0, 400.0));
    let down = r.value();
    assert!(down < more, "{down} < {more}");
    // OK legt die Ebene mit genau diesem Abstand an.
    let id = r.d.ok(&mut r.s).unwrap();
    match &r.s.document.feature(id).unwrap().kind {
        FeatureKind::Plane(pf) => match pf.def {
            PlaneDef::Offset { distance, .. } => assert_eq!(distance, down),
            ref other => panic!("{other:?}"),
        },
        other => panic!("{other:?}"),
    }
}

#[test]
fn esc_during_a_drag_takes_back_only_the_drag() {
    let mut r = offset_rig();
    let p = r.grip_at();
    r.press(p);
    r.move_to(p - vec2(0.0, 120.0));
    assert!(r.value() > 5.0);
    r.frame(vec![esc()]);
    r.frame(vec![]);
    assert!(r.open, "das Fenster bleibt offen");
    assert_eq!(r.value(), 0.0, "der Zug ist zurück");
    assert!(r.s.plane_pick.unwrap().grip.unwrap().drag.is_none());
    // Ohne Zug schließt Esc das Fenster wie bisher.
    r.release(p - vec2(0.0, 120.0));
    r.frame(vec![esc()]);
    assert!(!r.open);
}

#[test]
fn a_click_on_the_arrow_fills_no_slot() {
    let mut r = offset_rig();
    let before = r.d.slot(0).cloned();
    let p = r.grip_at();
    r.press(p);
    r.release(p);
    assert_eq!(r.d.slot(0).cloned(), before, "der Klick gehört dem Griff");
    assert_eq!(r.value(), 0.0);
}

#[test]
fn dragging_the_turn_arrow_sets_the_angle() {
    let mut d = PlaneDialog::new(PlaneCommand::Angle);
    d.fill(SlotValue::Axis(AxisRef::Standard(StdAxis::X)));
    let mut r = Rig::new(d);
    let GripShape::Angle { axis, value, .. } = r.grip() else {
        panic!("Drehpfeil erwartet")
    };
    assert_eq!(value, 0.0);
    assert!(
        (axis - planora_math::DVec3::X).length() < 1e-9,
        "um die gewählte Achse"
    );
    let p = r.grip_at();
    r.press(p);
    r.move_to(p + vec2(0.0, 150.0));
    r.release(p + vec2(0.0, 150.0));
    let a = r.value();
    assert!(a.abs() > 5.0 && a.abs() < 180.0, "{a}");
    assert_eq!(
        (a * 10.0).round() / 10.0,
        a,
        "auf 0,1° gerundet (wie Fusion)"
    );
    // Zurück an den Ausgangspunkt: wieder (fast) 0.
    let q = r.grip_at();
    r.press(q);
    r.move_to(q - vec2(0.0, 150.0));
    r.release(q - vec2(0.0, 150.0));
    assert!(r.value().abs() < a.abs(), "{} < {a}", r.value());
}

#[test]
fn other_plane_commands_have_no_grip() {
    for cmd in [PlaneCommand::Mid, PlaneCommand::ThreePoints] {
        let r = Rig::new(PlaneDialog::new(cmd));
        assert!(r.s.plane_pick.unwrap().grip.is_none(), "{cmd:?}");
    }
}

#[test]
fn enter_after_a_drag_creates_the_plane() {
    let mut r = offset_rig();
    let p = r.grip_at();
    r.press(p);
    r.move_to(p - vec2(0.0, 120.0));
    r.release(p - vec2(0.0, 120.0));
    let v = r.value();
    r.frame(vec![Event::Key {
        key: Key::Enter,
        physical_key: Some(Key::Enter),
        pressed: true,
        repeat: false,
        modifiers: Modifiers::default(),
    }]);
    assert!(!r.open, "Enter übernimmt (wie Fusion)");
    let planes: Vec<_> =
        r.s.document
            .features()
            .iter()
            .filter_map(|f| match &f.kind {
                FeatureKind::Plane(pf) => Some(pf.def.clone()),
                _ => None,
            })
            .collect();
    assert!(
        matches!(planes.as_slice(), [PlaneDef::Offset { distance, .. }] if *distance == v),
        "{planes:?}"
    );
}
