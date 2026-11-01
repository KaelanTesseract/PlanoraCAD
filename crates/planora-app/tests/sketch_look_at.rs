//! „Blick auf“ (Fusion Look At) und die Kürzel für Skizzenraster und Fang (Skizzen 2, Task 9).
//! Ohne Fenster bei 1920 × 1080; Bibliothek und Konfiguration nur in Temp-Ordnern.

mod common;

use eframe::App as _;
use egui::{Event, Key, Modifiers, PointerButton, Pos2, Rect, Vec2, vec2};
use glam::Vec3;
use planora_app::app::PlanoraApp;
use planora_app::keymap::{Action, Binding, KeyChord, SetResult};
use planora_app::sketch_state::{ConstraintAction, Item, Tool};
use planora_app::state::{AppState, StandardPlane};
use planora_app::workspace::Workspace;
use planora_app::{fonts, sketch_panel, theme, viewport};
use planora_feature::SketchPlane;
use planora_kernel::{Face, Id};
use planora_math::DVec3;
use planora_project::config::Config;
use planora_project::library::Library;
use planora_sketch::plane_of_face;

const SIZE: Vec2 = vec2(1920.0, 1080.0);
const NONE: Modifiers = Modifiers::NONE;

fn v(d: DVec3) -> Vec3 {
    Vec3::new(d.x as f32, d.y as f32, d.z as f32)
}

/// Würfel mit offener Skizze auf `plane`, Ansicht von Hand verdreht.
fn state_with_sketch(plane: SketchPlane) -> AppState {
    let mut s = AppState::new();
    s.create_box(DVec3::splat(40.0)).unwrap();
    let id = s.add_sketch(plane).unwrap();
    s.open_sketch(id).unwrap();
    s.camera.set_viewport(SIZE.x, SIZE.y);
    s.camera.orbit(120.0, 80.0);
    s.camera.distance = 123.0;
    s.camera.target = Vec3::new(7.0, -9.0, 11.0);
    s
}

fn frame_z(s: &AppState) -> (planora_math::Frame, Vec3) {
    let f = *s.session_sketch().unwrap().frame();
    (f, v(f.z))
}

/// Blick senkrecht auf die Ebene, Ziel in der Ebene, oben = `frame.y`, Abstand unverändert.
fn assert_looks_at(s: &AppState, distance: f32) {
    let (f, n) = frame_z(s);
    let dir = (s.camera.eye() - s.camera.target).normalize();
    assert!(dir.dot(n) > 0.9999, "Blick {dir:?} nicht parallel zu {n:?}");
    assert_eq!(s.camera.distance, distance);
    let off = (s.camera.target - v(f.origin)).dot(n);
    assert!(off.abs() < 1e-3, "Ziel {off} mm neben der Ebene");
    let up = s.camera.view().inverse().transform_vector3(Vec3::Y);
    assert!(up.dot(v(f.y)) > 0.9999, "oben {up:?} statt {:?}", v(f.y));
}

#[test]
fn look_at_turns_the_view_square_onto_xy_and_xz() {
    for plane in [StandardPlane::XY, StandardPlane::XZ, StandardPlane::YZ] {
        let mut s = state_with_sketch(SketchPlane::Standard(plane));
        s.look_at_sketch();
        assert_looks_at(&s, 123.0);
    }
}

#[test]
fn look_at_works_on_a_face() {
    let mut s = AppState::new();
    s.create_box(DVec3::new(100.0, 60.0, 40.0)).unwrap();
    let solid = &s.bodies[0].solid;
    let side: Id<Face> = solid
        .faces
        .iter()
        .filter_map(|(id, _)| plane_of_face(solid, id).map(|f| (id, f.z.dot(DVec3::X))))
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(id, _)| id)
        .unwrap();
    let id = s.new_sketch_on_face(0, side).unwrap();
    s.open_sketch(id).unwrap();
    s.camera.set_viewport(SIZE.x, SIZE.y);
    s.camera.orbit(-200.0, 30.0);
    s.camera.target = Vec3::new(3.0, 4.0, 5.0);
    let d = s.camera.distance;
    s.look_at_sketch();
    assert_looks_at(&s, d);
}

#[test]
fn look_at_is_no_undo_step_and_nothing_jumps_afterwards() {
    let mut s = state_with_sketch(SketchPlane::Standard(StandardPlane::XY));
    let steps = s.document.features().len();
    s.look_at_sketch();
    assert_eq!(s.document.features().len(), steps);
    assert!(!s.undo_sketch(), "kein Rückgängig-Schritt");
    let cam = (
        s.camera.target,
        s.camera.distance,
        s.camera.yaw,
        s.camera.pitch,
    );
    for _ in 0..10 {
        s.refit_sketch_view();
    }
    assert_eq!(
        cam,
        (
            s.camera.target,
            s.camera.distance,
            s.camera.yaw,
            s.camera.pitch
        )
    );
}

#[test]
fn look_at_without_an_open_sketch_does_nothing() {
    let mut s = AppState::new();
    s.create_box(DVec3::splat(10.0)).unwrap();
    s.camera.orbit(50.0, 20.0);
    let before = (
        s.camera.target,
        s.camera.distance,
        s.camera.yaw,
        s.camera.pitch,
    );
    s.look_at_sketch();
    assert_eq!(
        before,
        (
            s.camera.target,
            s.camera.distance,
            s.camera.yaw,
            s.camera.pitch
        )
    );
}

// ---- Ganze App -------------------------------------------------------------------------------

fn plain(key: Key) -> Binding {
    Binding::Key(KeyChord {
        key,
        ctrl: false,
        shift: false,
        alt: false,
    })
}

fn key(k: Key) -> Event {
    Event::Key {
        key: k,
        physical_key: Some(k),
        pressed: true,
        repeat: false,
        modifiers: NONE,
    }
}

struct Rig {
    ctx: egui::Context,
    app: PlanoraApp,
    time: f64,
    shapes: Vec<egui::epaint::ClippedShape>,
}

impl Rig {
    fn new() -> Rig {
        let root = common::temp_root("planora-sketch-look-at");
        let mut w = Workspace::new(Some(Library::open(&root).unwrap()), Config::default());
        w.config_path = root.join("config.json");
        let s = &mut w.active_tab_mut().state;
        s.create_box(DVec3::splat(20.0)).unwrap();
        let sk = s
            .add_sketch(SketchPlane::Standard(StandardPlane::XY))
            .unwrap();
        s.open_sketch(sk).unwrap();
        w.observe_history(false);
        let ctx = egui::Context::default();
        ctx.set_fonts(fonts::definitions());
        theme::apply(&ctx, true);
        PlanoraApp::setup_context(&ctx);
        let mut rig = Rig {
            ctx,
            app: PlanoraApp::for_tests(w),
            time: 0.0,
            shapes: vec![],
        };
        rig.settle();
        rig
    }

    fn ws(&mut self) -> &mut Workspace {
        self.app.ws_mut()
    }

    fn st(&mut self) -> &mut AppState {
        &mut self.app.ws_mut().active_tab_mut().state
    }

    fn frame(&mut self, events: Vec<Event>) {
        self.time += 1.0 / 60.0;
        let raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, SIZE)),
            time: Some(self.time),
            events,
            ..Default::default()
        };
        let mut f = eframe::Frame::_new_kittest();
        let app = &mut self.app;
        let out = self.ctx.run_ui(raw, |ui| app.ui(ui, &mut f));
        self.shapes = out.shapes.clone();
        out.drop_without_applying_deltas();
    }

    fn settle(&mut self) {
        for _ in 0..3 {
            self.frame(vec![]);
        }
    }

    fn press(&mut self, k: Key) {
        self.frame(vec![key(k)]);
        self.settle();
    }

    fn text(&self, text: &str) -> Option<Rect> {
        common::collect_seen(&self.shapes)
            .into_iter()
            .find(|s| s.text == text)
            .map(|s| s.rect)
    }

    fn click(&mut self, at: Pos2) {
        let b = |pressed| Event::PointerButton {
            pos: at,
            button: PointerButton::Primary,
            pressed,
            modifiers: NONE,
        };
        self.frame(vec![Event::PointerMoved(at)]);
        self.frame(vec![b(true)]);
        self.frame(vec![b(false)]);
        self.settle();
    }

    fn tool(&mut self) -> Tool {
        self.st().sketch_session.as_ref().unwrap().tool
    }
}

fn twist(rig: &mut Rig) {
    rig.st().camera.orbit(150.0, 90.0);
}

#[test]
fn the_button_sits_in_the_palette_row_and_looks_at_the_sketch() {
    let mut rig = Rig::new();
    let row = rig.text("Skizzenraster").expect("Skizzenraster");
    let snap = rig.text("Fang").expect("Fang");
    let look = sketch_panel::look_at_button(&rig.ctx).expect("Knopf „Blick auf“");
    assert!(
        (look.center().y - row.center().y).abs() < 8.0,
        "gleiche Zeile: {look:?} / {row:?}"
    );
    assert!(look.min.x > snap.max.x, "rechts neben Fang");
    assert!(look.max.x < SIZE.x, "im Fenster");
    twist(&mut rig);
    let dist = rig.st().camera.distance;
    rig.click(look.center());
    let s = rig.st();
    assert_looks_at(s, dist);
    // Weiterhin nichts springt nach.
    rig.settle();
    rig.settle();
    assert_looks_at(rig.st(), dist);
}

#[test]
fn the_three_shortcuts_are_assignable_without_defaults_and_work() {
    let mut rig = Rig::new();
    let mut km = rig.ws().keymap.clone();
    for a in [
        Action::SketchLookAt,
        Action::SketchToggleGrid,
        Action::SketchToggleSnap,
    ] {
        assert!(Action::ALL.contains(&a), "{a:?}");
        assert!(km.bindings(a).is_empty(), "{a:?} ohne Vorgabetaste");
    }
    assert_eq!(Action::SketchLookAt.id(), "skizze.blick_auf");
    assert_eq!(Action::SketchToggleGrid.id(), "skizze.raster");
    assert_eq!(Action::SketchToggleSnap.id(), "skizze.fang");
    assert_eq!(
        km.set(Action::SketchLookAt, 0, plain(Key::H)),
        SetResult::Ok
    );
    assert_eq!(
        km.set(Action::SketchToggleGrid, 0, plain(Key::G)),
        SetResult::Ok
    );
    assert_eq!(
        km.set(Action::SketchToggleSnap, 0, plain(Key::N)),
        SetResult::Ok
    );
    rig.ws().keymap = km;

    twist(&mut rig);
    let dist = rig.st().camera.distance;
    rig.press(Key::H);
    assert_looks_at(rig.st(), dist);

    assert!(rig.st().sketch_show_grid && rig.st().sketch_snap_grid);
    rig.press(Key::G);
    assert!(!rig.st().sketch_show_grid);
    assert!(!rig.ws().config.sketch_grid, "in der Konfiguration");
    rig.press(Key::N);
    assert!(!rig.st().sketch_snap_grid);
    assert!(!rig.ws().config.sketch_snap);
    rig.press(Key::G);
    rig.press(Key::N);
    assert!(rig.st().sketch_show_grid && rig.st().sketch_snap_grid);
}

#[test]
fn the_shortcuts_do_nothing_while_typing_in_a_field() {
    let mut rig = Rig::new();
    let mut km = rig.ws().keymap.clone();
    km.set(Action::SketchLookAt, 0, plain(Key::H));
    km.set(Action::SketchToggleGrid, 0, plain(Key::G));
    km.set(Action::SketchToggleSnap, 0, plain(Key::N));
    rig.ws().keymap = km;
    // Eine Linie, gewählt, Maßfeld offen.
    {
        let (se, sk) = rig.st().session_sketch_mut().unwrap();
        se.set_tool(sk, Tool::Line);
        se.click(sk, planora_math::DVec2::new(0.0, 0.0), 0.01, false)
            .unwrap();
        se.click(sk, planora_math::DVec2::new(10.0, 0.0), 0.01, false)
            .unwrap();
        se.set_tool(sk, Tool::Select);
    }
    let line = rig
        .st()
        .session_sketch()
        .unwrap()
        .curves()
        .next()
        .unwrap()
        .0;
    rig.st().sketch_session.as_mut().unwrap().selection = vec![Item::Curve(line)];
    sketch_panel::open_dimension_editor(
        &rig.ctx,
        &rig.app.ws_mut().active_tab().state,
        ConstraintAction::Distance,
    );
    rig.settle();
    assert!(viewport::typing(&rig.ctx));
    twist(&mut rig);
    let cam = rig.st().camera;
    for k in [Key::H, Key::G, Key::N] {
        rig.frame(vec![key(k)]);
    }
    rig.settle();
    let s = rig.st();
    assert_eq!((s.camera.yaw, s.camera.pitch), (cam.yaw, cam.pitch));
    assert!(s.sketch_show_grid && s.sketch_snap_grid);
    assert_eq!(rig.tool(), Tool::Select);
}
