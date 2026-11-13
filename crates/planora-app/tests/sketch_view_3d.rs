//! 3D-Abbildung und Anzeige von 3D-Kurven (Skizzen 10, Task 8): Bildschirm ↔ beliebige Ebene in
//! Skizzenkoordinaten, Linienzug einer 3D-Kurve auf dem Bildschirm und Auswahl über die
//! projizierte Lage (Klick und Rahmen) gegen das Overlay bei 1920 × 1080.

use egui::{Pos2, Rect, pos2};
use glam::{Vec2, Vec3};
use planora_app::sketch_state::Item;
use planora_app::sketch_view::{self, Mapper, curve_screen_path, screen_to_plane};
use planora_app::state::{AppState, StandardPlane};
use planora_app::{fonts, theme};
use planora_feature::SketchPlane;
use planora_math::{DVec3, Frame, Tolerance};
use planora_render::OrbitCamera;
use planora_sketch::model::{CurveId, Sketch};
use planora_sketch::solver::SolveStatus;

const W: f32 = 1920.0;
const H: f32 = 1080.0;
const SCREEN: Rect = Rect {
    min: pos2(0.0, 0.0),
    max: pos2(W, H),
};

fn camera(target: Vec3, distance: f32, to_camera: Vec3, up: Vec3) -> OrbitCamera {
    let mut cam = OrbitCamera {
        target,
        distance,
        ..Default::default()
    };
    cam.look_along(to_camera, up);
    cam.set_viewport(W, H);
    cam
}

fn viewport() -> Vec2 {
    Vec2::new(W, H)
}

#[test]
fn screen_to_plane_hits_known_point() {
    // Von vorn (entlang −X) auf die YZ-Ebene durch den Ursprung.
    let cam = camera(Vec3::ZERO, 100.0, Vec3::X, Vec3::Z);
    let center = viewport() * 0.5;
    let p = screen_to_plane(
        &cam,
        &Frame::WORLD,
        viewport(),
        center,
        DVec3::ZERO,
        DVec3::X,
    )
    .expect("die Bildmitte trifft die Ebene");
    assert!(p.length() < 1e-4, "{p:?}");

    // Hin und zurück über die 3D-Abbildung: ein Punkt der Ebene landet wieder bei sich.
    let map = Mapper {
        camera: cam,
        frame: Frame::WORLD,
        rect: SCREEN,
    };
    for q in [DVec3::new(0.0, 12.0, -7.0), DVec3::new(0.0, -30.0, 22.0)] {
        let s = map.to_screen3(q).expect("vor der Kamera");
        let back = screen_to_plane(
            &cam,
            &Frame::WORLD,
            viewport(),
            Vec2::new(s.x, s.y),
            DVec3::ZERO,
            DVec3::X,
        )
        .unwrap();
        assert!((back - q).length() < 1e-3, "{q:?} → {s:?} → {back:?}");
    }
    // Eine verschobene Skizzenebene: Ergebnis in Skizzenkoordinaten.
    let frame = Frame::from_origin_normal(DVec3::new(0.0, 0.0, 5.0), DVec3::Z);
    let p = screen_to_plane(
        &cam,
        &frame,
        viewport(),
        center,
        DVec3::ZERO,
        frame.to_local(frame.origin + DVec3::X),
    )
    .unwrap();
    assert!(
        (frame.point(p.x, p.y, p.z) - DVec3::ZERO).length() < 1e-4,
        "{p:?}"
    );
}

#[test]
fn grazing_plane_returns_none() {
    // Blick 4° gegen die Ebene XY: unter MIN_PICK_ANGLE = 5°, kein Punkt.
    let at = |deg: f32| {
        let r = deg.to_radians();
        let cam = camera(Vec3::ZERO, 100.0, Vec3::new(r.cos(), 0.0, r.sin()), Vec3::Z);
        screen_to_plane(
            &cam,
            &Frame::WORLD,
            viewport(),
            viewport() * 0.5,
            DVec3::ZERO,
            DVec3::Z,
        )
    };
    assert_eq!(at(4.0), None);
    assert_eq!(at(-4.0), None, "auch von unten");
    let p = at(6.0).expect("6° reicht");
    assert!(p.length() < 1e-3, "{p:?}");
    assert!((sketch_view::MIN_PICK_ANGLE - 5f64.to_radians()).abs() < 1e-12);
    // Eine nicht normierbare Normale trifft nichts.
    let cam = camera(Vec3::ZERO, 100.0, Vec3::Z, Vec3::Y);
    let none = screen_to_plane(
        &cam,
        &Frame::WORLD,
        viewport(),
        viewport() * 0.5,
        DVec3::ZERO,
        DVec3::ZERO,
    );
    assert_eq!(none, None);
}

#[test]
fn circle3_path_projects_as_ellipse_on_screen() {
    let mut sk = Sketch::new(Frame::WORLD, "Skizze", Tolerance::default());
    let m = sk.add_point(0.0, 0.0).unwrap();
    // Um 60° um X gekippt: von oben gesehen ist die Höhe halb so groß wie die Breite.
    let tilt = 60f64.to_radians();
    let c = sk
        .add_circle3(m, 10.0, DVec3::new(0.0, -tilt.sin(), tilt.cos()))
        .unwrap();
    assert!(sk.is_3d_curve(c));
    let map = Mapper {
        camera: camera(Vec3::ZERO, 2000.0, Vec3::Z, Vec3::Y),
        frame: Frame::WORLD,
        rect: SCREEN,
    };
    let path = curve_screen_path(&sk, c, &map);
    assert!(path.len() > 16, "{}", path.len());
    assert!(
        path[0].distance(*path.last().unwrap()) < 1e-3,
        "geschlossen"
    );
    let (lo, hi) = path.iter().fold(
        (Pos2::new(f32::MAX, f32::MAX), Pos2::new(f32::MIN, f32::MIN)),
        |(lo, hi), p| (lo.min(*p), hi.max(*p)),
    );
    let (w, h) = (hi.x - lo.x, hi.y - lo.y);
    assert!(w > 10.0, "sichtbar groß: {w}");
    assert!(
        ((h / w) - 0.5).abs() < 0.02,
        "Ellipse mit 1 : 2, nicht Kreis: {w} × {h}"
    );
    // Die Mitte der Bahn liegt auf der Mitte des Kreises.
    let mid = map.to_screen3(DVec3::ZERO).unwrap();
    assert!(lo.lerp(hi, 0.5).distance(mid) < 0.5);
}

// ---- Auswahl gegen das Overlay --------------------------------------------------------------

/// Skizze auf XY mit einer 3D-Linie in 20 mm Höhe; schräge Ansicht, damit Draufsicht und Raumlage
/// auf dem Bildschirm weit auseinander liegen.
fn lifted_line() -> (AppState, CurveId) {
    let mut s = AppState::new();
    let i = s
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    s.open_sketch(i).unwrap();
    let c = {
        let sk = s.session_sketch_mut().unwrap().1;
        let a = sk.add_point3(DVec3::new(0.0, 0.0, 20.0)).unwrap();
        let b = sk.add_point3(DVec3::new(20.0, 0.0, 20.0)).unwrap();
        let c = sk.add_line(a, b).unwrap();
        assert_eq!(sk.solve().status, SolveStatus::Solved);
        c
    };
    s.observe_sketch(false);
    s.camera = camera(
        Vec3::new(10.0, 0.0, 10.0),
        150.0,
        Vec3::new(0.3, -1.0, 0.6),
        Vec3::Z,
    );
    (s, c)
}

fn context() -> egui::Context {
    let ctx = egui::Context::default();
    ctx.set_fonts(fonts::definitions());
    theme::apply(&ctx, true);
    ctx
}

fn map_of(s: &AppState) -> Mapper {
    let mut cam = s.camera;
    cam.set_viewport(W, H);
    Mapper {
        camera: cam,
        frame: *s.session_sketch().unwrap().frame(),
        rect: SCREEN,
    }
}

fn at3(s: &AppState, p: DVec3) -> Pos2 {
    map_of(s).to_screen3(p).unwrap()
}

fn moved(p: Pos2) -> egui::Event {
    egui::Event::PointerMoved(p)
}

fn button(p: Pos2, pressed: bool) -> egui::Event {
    egui::Event::PointerButton {
        pos: p,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::default(),
    }
}

fn frame(ctx: &egui::Context, s: &mut AppState, events: Vec<egui::Event>) {
    let raw = egui::RawInput {
        screen_rect: Some(SCREEN),
        events,
        ..Default::default()
    };
    let mut gesture = false;
    let out = ctx.run_ui(raw, |ui| {
        sketch_view::show_overlay(ui, SCREEN, s);
        gesture = ui.input(|i| i.pointer.any_down());
    });
    out.drop_without_applying_deltas();
    s.observe_sketch(gesture);
}

fn click(ctx: &egui::Context, s: &mut AppState, p: Pos2) {
    frame(ctx, s, vec![]);
    frame(ctx, s, vec![moved(p)]);
    frame(ctx, s, vec![button(p, true)]);
    frame(ctx, s, vec![button(p, false)]);
    frame(ctx, s, vec![]);
}

fn selection(s: &AppState) -> Vec<Item> {
    s.sketch_session.as_ref().unwrap().selection.clone()
}

#[test]
fn pick_selects_3d_line_by_screen_distance() {
    let ctx = context();
    let (mut s, c) = lifted_line();
    let top = at3(&s, DVec3::new(10.0, 0.0, 0.0));
    let lifted = at3(&s, DVec3::new(10.0, 0.0, 20.0));
    assert!(top.distance(lifted) > 50.0, "{top:?} {lifted:?}");
    // Die Draufsicht der Linie trifft nichts: Dort ist sie nicht zu sehen.
    click(&ctx, &mut s, top);
    assert_eq!(selection(&s), vec![]);
    // Wenige Pixel neben der gezeichneten Linie wählt sie.
    click(&ctx, &mut s, lifted + egui::vec2(0.0, 4.0));
    assert_eq!(selection(&s), vec![Item::Curve(c)]);
    // Daneben (mehr als der Fangradius) wählt nichts.
    click(&ctx, &mut s, lifted + egui::vec2(0.0, 30.0));
    assert_eq!(selection(&s), vec![]);
}

fn drag_box(ctx: &egui::Context, s: &mut AppState, from: Pos2, to: Pos2) {
    frame(ctx, s, vec![]);
    frame(ctx, s, vec![moved(from)]);
    frame(ctx, s, vec![button(from, true)]);
    for t in [0.25, 0.5, 0.75, 1.0] {
        frame(ctx, s, vec![moved(from.lerp(to, t))]);
    }
    frame(ctx, s, vec![button(to, false)]);
    frame(ctx, s, vec![]);
}

#[test]
fn box_select_uses_projected_3d_position() {
    let ctx = context();
    let (mut s, c) = lifted_line();
    let ends = [
        at3(&s, DVec3::new(0.0, 0.0, 20.0)),
        at3(&s, DVec3::new(20.0, 0.0, 20.0)),
    ];
    let shown = Rect::from_two_pos(ends[0], ends[1]).expand(20.0);
    let tops = [
        at3(&s, DVec3::new(0.0, 0.0, 0.0)),
        at3(&s, DVec3::new(20.0, 0.0, 0.0)),
    ];
    assert!(
        tops.iter().all(|p| !shown.contains(*p)),
        "die Draufsicht liegt außerhalb des Rahmens"
    );
    // Fenster (links nach rechts) um die gezeichnete Lage: nimmt die Linie.
    drag_box(&ctx, &mut s, shown.left_top(), shown.right_bottom());
    assert_eq!(selection(&s), vec![Item::Curve(c)]);
    // Fenster um die Draufsicht: nimmt nichts.
    let top = Rect::from_two_pos(tops[0], tops[1]).expand(20.0);
    drag_box(&ctx, &mut s, top.left_top(), top.right_bottom());
    assert_eq!(selection(&s), vec![]);
    // Kreuzung (rechts nach links) durch die Mitte der gezeichneten Linie: nimmt sie.
    let mid = ends[0].lerp(ends[1], 0.5);
    let cross = Rect::from_center_size(mid, egui::vec2(40.0, 40.0));
    drag_box(&ctx, &mut s, cross.right_top(), cross.left_bottom());
    assert_eq!(selection(&s), vec![Item::Curve(c)]);
}
