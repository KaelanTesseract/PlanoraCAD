//! Mausgesten über die Tastenbelegung (Tastenkürzel, Aufgabe 3): 3D-Ansicht schwenkt, dreht und
//! zoomt nach der Tabelle, der Navigationswürfel dreht weiter mit der linken Taste, Skizze und
//! Zeichnung schwenken und zoomen nur. Ohne Fenster: Rohereignisse je Bild gegen einen
//! egui-Kontext, geprüft wird die Kamera bzw. Blattlage.

use egui::{Event, Modifiers, PointerButton, Pos2, Rect, pos2, vec2};
use planora_app::keymap::{Action, Binding, Gesture, Keymap, MouseButton, mouse};
use planora_app::state::{AppState, StandardPlane};
use planora_app::{drawing_view, fonts, sketch_view, viewport};
use planora_drawing::sheet_info::Stamp;
use planora_drawing::{Paper, Placement, Projection, Scale, Sheet, ViewKind};
use planora_feature::SketchPlane;
use planora_math::{DVec2, DVec3};

const SCREEN: Rect = Rect {
    min: pos2(0.0, 0.0),
    max: pos2(800.0, 600.0),
};

const FREE: Rect = Rect {
    min: pos2(278.0, 90.0),
    max: pos2(700.0, 500.0),
};

const NONE: Modifiers = Modifiers::NONE;
const SHIFT: Modifiers = Modifiers::SHIFT;
/// Strg wie unter Windows (egui-winit meldet `ctrl` und `command`).
const CTRL: Modifiers = Modifiers {
    alt: false,
    ctrl: true,
    shift: false,
    mac_cmd: false,
    command: true,
};

/// Welche Ansicht ein Bild zeichnet.
#[derive(Clone, Copy, PartialEq)]
enum View {
    /// 3D-Ansicht wie in der App (auch mit offener Skizze).
    Model,
    /// Nur die Skizzenfläche über dem ganzen Fenster.
    Overlay,
    /// Zeichnungsansicht.
    Drawing,
}

struct Rig {
    ctx: egui::Context,
    km: Keymap,
    view: View,
    time: f64,
}

impl Rig {
    fn new(view: View) -> Rig {
        let ctx = egui::Context::default();
        ctx.set_fonts(fonts::definitions());
        Rig {
            ctx,
            km: Keymap::defaults(),
            view,
            time: 0.0,
        }
    }

    fn frame(&mut self, s: &mut AppState, mods: Modifiers, events: Vec<Event>) {
        let mut all = vec![Event::ModifiersChanged(mods)];
        all.extend(events);
        self.time += 1.0 / 60.0;
        let raw = egui::RawInput {
            screen_rect: Some(SCREEN),
            events: all,
            time: Some(self.time),
            ..Default::default()
        };
        let (km, view) = (&self.km, self.view);
        self.ctx
            .run_ui(raw, |ui| {
                mouse::set_current(ui.ctx(), km);
                match view {
                    View::Model => viewport::show_in(ui, s, Some(FREE)),
                    View::Overlay => {
                        sketch_view::show_overlay(ui, SCREEN, s);
                    }
                    View::Drawing => drawing_view::show(ui, s),
                }
                viewport::remember_keyboard_focus(ui.ctx());
            })
            .drop_without_applying_deltas();
    }

    /// Zug mit `button` und gehaltenen `mods` von `from` nach `to` in sechs Schritten.
    fn drag(
        &mut self,
        s: &mut AppState,
        button: PointerButton,
        mods: Modifiers,
        from: Pos2,
        to: Pos2,
    ) {
        let ev = |p: Pos2, pressed: bool| Event::PointerButton {
            pos: p,
            button,
            pressed,
            modifiers: mods,
        };
        self.frame(s, mods, vec![]);
        self.frame(s, mods, vec![Event::PointerMoved(from)]);
        self.frame(s, mods, vec![Event::PointerMoved(from), ev(from, true)]);
        for i in 1..=6 {
            let p = from + (to - from) * (i as f32 / 6.0);
            self.frame(s, mods, vec![Event::PointerMoved(p)]);
        }
        self.frame(s, mods, vec![ev(to, false)]);
        self.frame(s, NONE, vec![]);
    }

    /// Ein Klick mit `button` ohne Bewegung.
    fn click(&mut self, s: &mut AppState, button: PointerButton, mods: Modifiers, at: Pos2) {
        let ev = |pressed: bool| Event::PointerButton {
            pos: at,
            button,
            pressed,
            modifiers: mods,
        };
        self.frame(s, mods, vec![Event::PointerMoved(at)]);
        self.frame(s, mods, vec![ev(true)]);
        self.frame(s, mods, vec![ev(false)]);
        self.frame(s, NONE, vec![]);
    }

    /// Ein Rastschritt des Mausrads über `at` mit `mods`, danach Bilder zum Ausglätten.
    fn wheel(&mut self, s: &mut AppState, mods: Modifiers, at: Pos2) {
        self.frame(s, mods, vec![Event::PointerMoved(at)]);
        self.frame(
            s,
            mods,
            vec![
                Event::PointerMoved(at),
                Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: vec2(0.0, 40.0),
                    phase: egui::TouchPhase::Move,
                    modifiers: mods,
                },
            ],
        );
        for _ in 0..30 {
            self.frame(s, mods, vec![]);
        }
    }
}

fn model() -> AppState {
    let mut s = AppState::new();
    s.create_box(DVec3::splat(100.0)).unwrap();
    s.camera.home();
    s
}

fn sketch() -> AppState {
    let mut s = AppState::new();
    let i = s
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    s.open_sketch(i).unwrap();
    s
}

fn drawing() -> AppState {
    drawing_with_id().0
}

fn drawing_with_id() -> (AppState, planora_drawing::DrawingId) {
    let mut s = AppState::new();
    s.create_box(DVec3::new(100.0, 60.0, 40.0)).unwrap();
    let stamp = Stamp {
        date: "2026-10-24".into(),
        user: "test".into(),
    };
    let sheet = Sheet {
        paper: Paper::A4,
        landscape: true,
    };
    let d = s
        .document
        .add_drawing_stamped("Blatt 1", sheet, Projection::First, &stamp)
        .unwrap();
    s.drawing_ui.open(d);
    s.drawing_ui.tick(&s.bodies, &s.document, true);
    (s, d)
}

/// Freie Stelle in der 3D-Ansicht (nicht am Würfel).
const IN_VIEW: Pos2 = pos2(400.0, 300.0);
const IN_VIEW_TO: Pos2 = pos2(470.0, 340.0);

fn angles(s: &AppState) -> (f32, f32) {
    (s.camera.yaw, s.camera.pitch)
}

fn drag_gesture(button: MouseButton, ctrl: bool, shift: bool, alt: bool) -> Binding {
    Binding::Mouse(Gesture::Drag {
        button,
        ctrl,
        shift,
        alt,
    })
}

#[test]
fn left_drag_still_orbits_in_3d() {
    let mut r = Rig::new(View::Model);
    let mut s = model();
    r.frame(&mut s, NONE, vec![]);
    let (a, t) = (angles(&s), s.camera.target);
    r.drag(&mut s, PointerButton::Primary, NONE, IN_VIEW, IN_VIEW_TO);
    assert_ne!(angles(&s), a, "links ziehen dreht");
    assert_eq!(s.camera.target, t, "links ziehen schwenkt nicht");
}

#[test]
fn middle_drag_pans_in_3d() {
    let mut r = Rig::new(View::Model);
    let mut s = model();
    r.frame(&mut s, NONE, vec![]);
    let (a, t) = (angles(&s), s.camera.target);
    r.drag(&mut s, PointerButton::Middle, NONE, IN_VIEW, IN_VIEW_TO);
    assert_ne!(s.camera.target, t, "Mitte ziehen schwenkt");
    assert_eq!(angles(&s), a, "Mitte ziehen dreht nicht");
}

#[test]
fn shift_middle_drag_orbits_in_3d() {
    let mut r = Rig::new(View::Model);
    let mut s = model();
    r.frame(&mut s, NONE, vec![]);
    let (a, t) = (angles(&s), s.camera.target);
    r.drag(&mut s, PointerButton::Middle, SHIFT, IN_VIEW, IN_VIEW_TO);
    assert_ne!(angles(&s), a, "Umschalt+Mitte dreht");
    assert_eq!(s.camera.target, t, "Umschalt+Mitte schwenkt nicht");
}

#[test]
fn shift_left_drag_still_pans_in_3d() {
    let mut r = Rig::new(View::Model);
    let mut s = model();
    r.frame(&mut s, NONE, vec![]);
    let (a, t) = (angles(&s), s.camera.target);
    r.drag(&mut s, PointerButton::Primary, SHIFT, IN_VIEW, IN_VIEW_TO);
    assert_ne!(s.camera.target, t, "Umschalt+links schwenkt");
    assert_eq!(angles(&s), a, "Umschalt+links dreht nicht");
}

#[test]
fn ctrl_or_alt_drag_does_nothing_by_default() {
    // Umschalttasten zählen genau: Strg+Mitte ist nicht Mitte, Alt+links nicht links.
    for (button, mods) in [
        (PointerButton::Middle, CTRL),
        (PointerButton::Primary, Modifiers::ALT),
    ] {
        let mut r = Rig::new(View::Model);
        let mut s = model();
        r.frame(&mut s, NONE, vec![]);
        let (a, t) = (angles(&s), s.camera.target);
        r.drag(&mut s, button, mods, IN_VIEW, IN_VIEW_TO);
        assert_eq!(angles(&s), a, "{button:?} {mods:?} dreht nicht");
        assert_eq!(s.camera.target, t, "{button:?} {mods:?} schwenkt nicht");
    }
}

#[test]
fn wheel_zooms_in_3d_sketch_and_drawing() {
    // 3D
    let mut r = Rig::new(View::Model);
    let mut s = model();
    r.frame(&mut s, NONE, vec![]);
    let d = s.camera.distance;
    r.wheel(&mut s, NONE, IN_VIEW);
    assert!(s.camera.distance < d, "3D: Rad nach vorn zoomt hinein");
    // Skizze (in der 3D-Ansicht)
    let mut r = Rig::new(View::Model);
    let mut s = sketch();
    r.frame(&mut s, NONE, vec![]);
    r.frame(&mut s, NONE, vec![]);
    let d = s.camera.distance;
    r.wheel(&mut s, NONE, IN_VIEW);
    assert!(s.camera.distance < d, "Skizze: Rad zoomt");
    // Zeichnung
    let mut r = Rig::new(View::Drawing);
    let mut s = drawing();
    r.frame(&mut s, NONE, vec![]);
    let z = s.drawing_ui.zoom;
    r.wheel(&mut s, NONE, IN_VIEW);
    assert!(s.drawing_ui.zoom > z, "Zeichnung: Rad zoomt hinein");
}

#[test]
fn wheel_modifiers_are_exact() {
    // Voreinstellung „Mausrad“ und „Strg + Mausrad“: Umschalt+Rad und Alt+Rad zoomen nicht.
    for mods in [SHIFT, Modifiers::ALT] {
        let mut r = Rig::new(View::Model);
        let mut s = model();
        r.frame(&mut s, NONE, vec![]);
        let d = s.camera.distance;
        r.wheel(&mut s, mods, IN_VIEW);
        assert_eq!(s.camera.distance, d, "{mods:?}+Rad ist nicht belegt");
        let mut r = Rig::new(View::Drawing);
        let mut s = drawing();
        r.frame(&mut s, NONE, vec![]);
        let z = s.drawing_ui.zoom;
        r.wheel(&mut s, mods, IN_VIEW);
        assert_eq!(
            s.drawing_ui.zoom, z,
            "Zeichnung: {mods:?}+Rad ist nicht belegt"
        );
    }

    // Umbelegt auf Strg+Rad: nur noch das wirkt, in 3D und Zeichnung.
    let mut km = Keymap::defaults();
    km.set(
        Action::ViewZoom,
        0,
        Binding::Mouse(Gesture::Wheel {
            ctrl: true,
            shift: false,
            alt: false,
        }),
    );
    for view in [View::Model, View::Drawing] {
        let mut r = Rig::new(view);
        r.km = km.clone();
        let mut s = if view == View::Model {
            model()
        } else {
            drawing()
        };
        r.frame(&mut s, NONE, vec![]);
        let (d, z) = (s.camera.distance, s.drawing_ui.zoom);
        r.wheel(&mut s, NONE, IN_VIEW);
        assert_eq!(
            (s.camera.distance, s.drawing_ui.zoom),
            (d, z),
            "Rad ohne Strg zoomt nicht mehr"
        );
        r.wheel(&mut s, CTRL, IN_VIEW);
        if view == View::Model {
            assert!(s.camera.distance < d, "3D: Strg+Rad zoomt");
        } else {
            assert!(s.drawing_ui.zoom > z, "Zeichnung: Strg+Rad zoomt");
        }
    }
}

#[test]
fn ctrl_wheel_zooms_by_default_in_3d_sketch_and_drawing() {
    // Strg+Rad ist die zweite Voreinstellung (so zoomt auch das Zusammenziehen auf
    // Windows-Touchpads, das als Strg+Rad ankommt).
    let mut r = Rig::new(View::Model);
    let mut s = model();
    r.frame(&mut s, NONE, vec![]);
    let d = s.camera.distance;
    r.wheel(&mut s, CTRL, IN_VIEW);
    assert!(s.camera.distance < d, "3D: Strg+Rad zoomt hinein");
    let mut r = Rig::new(View::Model);
    let mut s = sketch();
    r.frame(&mut s, NONE, vec![]);
    r.frame(&mut s, NONE, vec![]);
    let d = s.camera.distance;
    r.wheel(&mut s, CTRL, IN_VIEW);
    assert!(s.camera.distance < d, "Skizze: Strg+Rad zoomt hinein");
    let mut r = Rig::new(View::Drawing);
    let mut s = drawing();
    r.frame(&mut s, NONE, vec![]);
    let z = s.drawing_ui.zoom;
    r.wheel(&mut s, CTRL, IN_VIEW);
    assert!(s.drawing_ui.zoom > z, "Zeichnung: Strg+Rad zoomt hinein");
}

#[test]
fn ctrl_wheel_zooms_the_drawing_at_the_old_rate() {
    // Vor der Tastenbelegung zoomte Strg+Rad das Blatt über egui's `zoom_delta`, also um
    // exp(scroll_zoom_speed · Punkte); das bleibt so (nicht die langsamere Rate des Rads).
    let mut r = Rig::new(View::Drawing);
    let mut s = drawing();
    r.frame(&mut s, NONE, vec![]);
    let z = s.drawing_ui.zoom;
    r.wheel(&mut s, CTRL, IN_VIEW);
    let speed = r.ctx.options(|o| o.input_options.scroll_zoom_speed);
    let want = z * (speed * 40.0).exp();
    assert!(
        (s.drawing_ui.zoom - want).abs() < 1e-3 * want,
        "Strg+Rad: {} statt {want}",
        s.drawing_ui.zoom
    );
}

#[test]
fn rebinding_pan_to_right_drag_moves_the_gesture() {
    let mut km = Keymap::defaults();
    // Mitte (Stelle 0) wird rechts; Umschalt+links bleibt.
    km.set(
        Action::ViewPan,
        0,
        drag_gesture(MouseButton::Right, false, false, false),
    );
    let mut r = Rig::new(View::Model);
    r.km = km.clone();
    let mut s = model();
    r.frame(&mut s, NONE, vec![]);
    let (a, t) = (angles(&s), s.camera.target);
    r.drag(&mut s, PointerButton::Middle, NONE, IN_VIEW, IN_VIEW_TO);
    assert_eq!(s.camera.target, t, "Mitte schwenkt nicht mehr");
    assert_eq!(angles(&s), a, "Mitte dreht auch nicht");
    r.drag(&mut s, PointerButton::Secondary, NONE, IN_VIEW, IN_VIEW_TO);
    assert_ne!(s.camera.target, t, "rechts schwenkt");
    assert_eq!(angles(&s), a, "rechts dreht nicht");

    // Auch in Skizze und Zeichnung (eine Belegung für alle Ansichten).
    let mut r = Rig::new(View::Model);
    r.km = km.clone();
    let mut s = sketch();
    r.frame(&mut s, NONE, vec![]);
    let t = s.camera.target;
    r.drag(&mut s, PointerButton::Middle, NONE, IN_VIEW, IN_VIEW_TO);
    assert_eq!(s.camera.target, t, "Skizze: Mitte schwenkt nicht mehr");
    r.drag(&mut s, PointerButton::Secondary, NONE, IN_VIEW, IN_VIEW_TO);
    assert_ne!(s.camera.target, t, "Skizze: rechts schwenkt");

    let mut r = Rig::new(View::Drawing);
    r.km = km;
    let mut s = drawing();
    r.frame(&mut s, NONE, vec![]);
    let p = s.drawing_ui.pan;
    r.drag(&mut s, PointerButton::Middle, NONE, IN_VIEW, IN_VIEW_TO);
    assert_eq!(s.drawing_ui.pan, p, "Zeichnung: Mitte schwenkt nicht mehr");
    r.drag(&mut s, PointerButton::Secondary, NONE, IN_VIEW, IN_VIEW_TO);
    assert_ne!(s.drawing_ui.pan, p, "Zeichnung: rechts schwenkt");
}

#[test]
fn sketch_and_drawing_have_no_orbit() {
    // Skizze: weder links (Drehen-Zusatz) noch Umschalt+Mitte drehen oder schwenken.
    for (button, mods) in [
        (PointerButton::Primary, NONE),
        (PointerButton::Middle, SHIFT),
    ] {
        let mut r = Rig::new(View::Model);
        let mut s = sketch();
        r.frame(&mut s, NONE, vec![]);
        let (a, t) = (angles(&s), s.camera.target);
        r.drag(&mut s, button, mods, IN_VIEW, IN_VIEW_TO);
        assert_eq!(angles(&s), a, "Skizze: {button:?} {mods:?} dreht nicht");
        assert_eq!(
            s.camera.target, t,
            "Skizze: {button:?} {mods:?} schwenkt nicht"
        );
    }
    // Skizze: Mitte und Umschalt+links schwenken weiter.
    for (button, mods) in [
        (PointerButton::Middle, NONE),
        (PointerButton::Primary, SHIFT),
    ] {
        let mut r = Rig::new(View::Model);
        let mut s = sketch();
        r.frame(&mut s, NONE, vec![]);
        let (a, t) = (angles(&s), s.camera.target);
        r.drag(&mut s, button, mods, IN_VIEW, IN_VIEW_TO);
        assert_eq!(angles(&s), a, "Skizze: {button:?} {mods:?} dreht nicht");
        assert_ne!(s.camera.target, t, "Skizze: {button:?} {mods:?} schwenkt");
    }
    // Zeichnung: Umschalt+Mitte (Drehen) tut nichts, Mitte und Umschalt+links schwenken.
    let mut r = Rig::new(View::Drawing);
    let mut s = drawing();
    r.frame(&mut s, NONE, vec![]);
    let p = s.drawing_ui.pan;
    r.drag(&mut s, PointerButton::Middle, SHIFT, IN_VIEW, IN_VIEW_TO);
    assert_eq!(
        s.drawing_ui.pan, p,
        "Zeichnung: Umschalt+Mitte schwenkt nicht"
    );
    for (button, mods) in [
        (PointerButton::Middle, NONE),
        (PointerButton::Primary, SHIFT),
    ] {
        let mut r = Rig::new(View::Drawing);
        let mut s = drawing();
        r.frame(&mut s, NONE, vec![]);
        let p = s.drawing_ui.pan;
        r.drag(&mut s, button, mods, IN_VIEW, IN_VIEW_TO);
        assert_ne!(
            s.drawing_ui.pan, p,
            "Zeichnung: {button:?} {mods:?} schwenkt"
        );
    }
}

#[test]
fn a_click_is_not_a_drag_gesture() {
    // Unmittelbar: auf einer Fläche meldet `drag_action` bei einem Klick nichts, bei einem Zug
    // die Aktion.
    let ctx = egui::Context::default();
    let km = Keymap::defaults();
    let area = Rect::from_min_max(pos2(100.0, 100.0), pos2(500.0, 500.0));
    let mut seen: Vec<Option<Action>> = Vec::new();
    let run = |events: Vec<Event>, seen: &mut Vec<Option<Action>>| {
        ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(SCREEN),
                events,
                ..Default::default()
            },
            |ui| {
                let resp =
                    ui.interact(area, egui::Id::new("fläche"), egui::Sense::click_and_drag());
                seen.push(mouse::drag_action(
                    &km,
                    ui,
                    &resp,
                    &[Action::ViewPan, Action::ViewOrbit],
                ));
            },
        )
        .drop_without_applying_deltas();
    };
    let p = pos2(300.0, 300.0);
    let middle = |pressed: bool| Event::PointerButton {
        pos: p,
        button: PointerButton::Middle,
        pressed,
        modifiers: NONE,
    };
    run(vec![Event::PointerMoved(p)], &mut seen);
    run(vec![middle(true)], &mut seen);
    run(vec![middle(false)], &mut seen);
    run(vec![], &mut seen);
    assert!(seen.iter().all(Option::is_none), "Klick: {seen:?}");
    seen.clear();
    run(vec![middle(true)], &mut seen);
    for i in 1..=4 {
        run(
            vec![Event::PointerMoved(p + vec2(10.0 * i as f32, 0.0))],
            &mut seen,
        );
    }
    assert!(seen.contains(&Some(Action::ViewPan)), "Zug: {seen:?}");

    // In der 3D-Ansicht: Klicks mit Mitte und Umschalt+links bewegen die Kamera nicht.
    let mut r = Rig::new(View::Model);
    let mut s = model();
    r.frame(&mut s, NONE, vec![]);
    let (a, t) = (angles(&s), s.camera.target);
    r.click(&mut s, PointerButton::Middle, NONE, IN_VIEW);
    r.click(&mut s, PointerButton::Middle, SHIFT, IN_VIEW);
    assert_eq!((angles(&s), s.camera.target), (a, t), "Klick bewegt nichts");
}

#[test]
fn cube_drag_still_orbits() {
    let cube = viewport::cube_rect(FREE);
    let from = cube.center();
    let to = from + vec2(-30.0, 12.0);
    for sketching in [false, true] {
        let what = if sketching { "Skizze" } else { "Modell" };
        let mut r = Rig::new(View::Model);
        let mut s = if sketching { sketch() } else { model() };
        r.frame(&mut s, NONE, vec![]);
        r.frame(&mut s, NONE, vec![]);
        let (a, t) = (angles(&s), s.camera.target);
        r.drag(&mut s, PointerButton::Primary, NONE, from, to);
        assert_ne!(angles(&s), a, "{what}: links am Würfel dreht");
        assert_eq!(s.camera.target, t, "{what}: links am Würfel schwenkt nicht");

        // Umschalt+links am Würfel schwenkt wie in der Ansicht.
        let (a, t) = (angles(&s), s.camera.target);
        r.drag(&mut s, PointerButton::Primary, SHIFT, from, to);
        assert_eq!(
            angles(&s),
            a,
            "{what}: Umschalt+links am Würfel dreht nicht"
        );
        assert_ne!(
            s.camera.target, t,
            "{what}: Umschalt+links am Würfel schwenkt"
        );

        // Umschalt+Mitte am Würfel dreht ebenfalls.
        let a = angles(&s);
        r.drag(&mut s, PointerButton::Middle, SHIFT, from, to);
        assert_ne!(angles(&s), a, "{what}: Umschalt+Mitte am Würfel dreht");
    }
}

#[test]
fn drag_in_sketch_with_left_still_draws_not_pans() {
    // Skizzenfläche über dem ganzen Fenster: die Mitte ist der Ursprung der Skizze.
    let mut r = Rig::new(View::Overlay);
    let mut s = sketch();
    let p = s
        .session_sketch_mut()
        .unwrap()
        .1
        .add_point(0.0, 0.0)
        .unwrap();
    r.frame(&mut s, NONE, vec![]);
    let (a, t) = (angles(&s), s.camera.target);
    r.drag(
        &mut s,
        PointerButton::Primary,
        NONE,
        pos2(400.0, 300.0),
        pos2(460.0, 300.0),
    );
    let xy = s.session_sketch().unwrap().point_xy(p).unwrap();
    assert!(xy.x > 1.0 && xy.y.abs() < 1e-3, "Punkt gezogen: {xy:?}");
    assert_eq!(s.camera.target, t, "links ziehen schwenkt nicht");
    assert_eq!(angles(&s), a, "links ziehen dreht nicht");

    // Auch wenn Schwenken auf links (ohne Umschalttasten) gelegt wird, zeichnet links weiter.
    let mut km = Keymap::defaults();
    km.swap(
        Action::ViewPan,
        0,
        drag_gesture(MouseButton::Left, false, false, false),
    );
    r.km = km;
    let t = s.camera.target;
    let before = s.session_sketch().unwrap().point_xy(p).unwrap();
    // Der Punkt liegt jetzt rechts der Mitte; von dort weiter ziehen.
    let from = pos2(400.0, 300.0) + vec2(60.0, 0.0);
    r.drag(
        &mut s,
        PointerButton::Primary,
        NONE,
        from,
        from + vec2(40.0, 0.0),
    );
    let xy = s.session_sketch().unwrap().point_xy(p).unwrap();
    assert!(
        xy.x > before.x + 1.0,
        "Punkt weiter gezogen: {before:?} → {xy:?}"
    );
    assert_eq!(s.camera.target, t, "links schwenkt in der Skizze nicht");
}

#[test]
fn sketch_pan_on_ctrl_left_does_not_drag_a_point() {
    // Schwenken auf Strg+links umgelegt: ein Zug, der auf einem Punkt beginnt, schwenkt nur.
    let mut r = Rig::new(View::Overlay);
    let mut km = Keymap::defaults();
    km.set(
        Action::ViewPan,
        1,
        drag_gesture(MouseButton::Left, true, false, false),
    );
    r.km = km;
    let mut s = sketch();
    let p = s
        .session_sketch_mut()
        .unwrap()
        .1
        .add_point(0.0, 0.0)
        .unwrap();
    r.frame(&mut s, NONE, vec![]);
    let t = s.camera.target;
    let before = s.session_sketch().unwrap().point_xy(p).unwrap();
    // Kleiner erster Schritt (7 px, knapp über egui's Ziehschwelle): der Druckpunkt liegt nach
    // dem ersten Schwenken noch im Fangradius des Punkts.
    let ev = |x: f32, pressed: bool| Event::PointerButton {
        pos: pos2(x, 300.0),
        button: PointerButton::Primary,
        pressed,
        modifiers: CTRL,
    };
    r.frame(&mut s, CTRL, vec![Event::PointerMoved(pos2(400.0, 300.0))]);
    r.frame(
        &mut s,
        CTRL,
        vec![Event::PointerMoved(pos2(400.0, 300.0)), ev(400.0, true)],
    );
    for x in [407.0, 420.0, 440.0, 460.0] {
        r.frame(&mut s, CTRL, vec![Event::PointerMoved(pos2(x, 300.0))]);
        assert!(
            s.sketch_session.as_ref().unwrap().dragging.is_none(),
            "kein Punktziehen bei {x}"
        );
    }
    r.frame(&mut s, CTRL, vec![ev(460.0, false)]);
    r.frame(&mut s, NONE, vec![]);
    assert_ne!(s.camera.target, t, "Strg+links schwenkt");
    let xy = s.session_sketch().unwrap().point_xy(p).unwrap();
    assert_eq!(xy, before, "der Punkt bleibt");
    assert!(s.sketch_session.as_ref().unwrap().dragging.is_none());
}

#[test]
fn drawing_left_drag_selects_even_if_pan_is_bound_to_left() {
    // Schwenken auf links (ohne Umschalttasten) umgelegt: links wählt und zieht im Blatt weiter.
    let (mut s, d) = drawing_with_id();
    let v = s
        .document
        .add_view(
            d,
            ViewKind::Front,
            Scale { num: 1, den: 2 },
            Placement::Free { x: 150.0, y: 110.0 },
            None,
        )
        .unwrap();
    s.drawing_ui.tick(&s.bodies, &s.document, true);
    let mut r = Rig::new(View::Drawing);
    let mut km = Keymap::defaults();
    km.swap(
        Action::ViewPan,
        0,
        drag_gesture(MouseButton::Left, false, false, false),
    );
    r.km = km;
    r.frame(&mut s, NONE, vec![]);
    r.frame(&mut s, NONE, vec![]);
    let l = drawing_view::last_layout(&r.ctx).expect("kein Zeichnungsbereich");
    let from = drawing_view::sheet_to_screen(
        l.canvas,
        l.sheet_mm,
        l.zoom,
        l.pan,
        DVec2::new(150.0, 110.0),
    );
    let pan = s.drawing_ui.pan;
    let start = s.document.drawing(d).unwrap().view(v).unwrap().placement;
    r.drag(
        &mut s,
        PointerButton::Primary,
        NONE,
        from,
        from + vec2(40.0, 20.0),
    );
    assert_eq!(
        s.drawing_ui.pan, pan,
        "links schwenkt in der Zeichnung nicht"
    );
    assert_eq!(
        s.drawing_ui.selected_view,
        Some(v),
        "links wählt die Ansicht"
    );
    assert_ne!(
        s.document.drawing(d).unwrap().view(v).unwrap().placement,
        start,
        "links zieht die Ansicht"
    );
}
