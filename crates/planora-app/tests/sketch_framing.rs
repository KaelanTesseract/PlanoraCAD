//! Eine neue Skizze erscheint mittig und herangezoomt (Handprüfung Etappe C, Haltepunkt 3):
//! Skizze auf einer gewählten Fläche → die Fläche füllt das freie Rechteck zwischen den Feldern,
//! mittig, mit Blick senkrecht auf die Fläche. Ohne Fenster: die ganze App Bild für Bild gegen
//! einen egui-Kontext; das freie Rechteck kommt aus `shell_panels::last_layout`.

mod common;

use eframe::App as _;
use egui::{Pos2, Rect, Vec2, vec2};
use planora_app::app::PlanoraApp;
use planora_app::state::{AppState, StandardPlane};
use planora_app::workspace::Workspace;
use planora_app::{fonts, shell_panels, theme};
use planora_feature::SketchPlane;
use planora_kernel::{Face, Id};
use planora_math::{DVec2, DVec3};
use planora_project::config::Config;
use planora_project::library::Library;
use planora_render::{FaceRef, Pick};
use planora_sketch::plane_of_face;

const SIZE: Vec2 = vec2(1400.0, 900.0);

fn context() -> egui::Context {
    let c = egui::Context::default();
    c.set_fonts(fonts::definitions());
    theme::apply(&c, true);
    c
}

fn new_app() -> PlanoraApp {
    let root = common::temp_root("planora-sketch-framing");
    PlanoraApp::for_tests(Workspace::new(
        Some(Library::open(&root).unwrap()),
        Config::default(),
    ))
}

fn state(app: &mut PlanoraApp) -> &mut AppState {
    &mut app.ws_mut().active_tab_mut().state
}

fn frame(c: &egui::Context, app: &mut PlanoraApp) {
    let mut f = eframe::Frame::_new_kittest();
    c.run_ui(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, SIZE)),
            ..Default::default()
        },
        |ui| app.ui(ui, &mut f),
    )
    .drop_without_applying_deltas();
}

fn settle(c: &egui::Context, app: &mut PlanoraApp) {
    for _ in 0..3 {
        frame(c, app);
    }
}

fn free(c: &egui::Context) -> Rect {
    shell_panels::last_layout(c).expect("Feldgeometrie").free
}

/// Ebene Fläche von Körper `body`, deren Normale am ehesten `normal` ist.
fn face_along(s: &AppState, body: usize, normal: DVec3) -> Id<Face> {
    let solid = &s.bodies[body].solid;
    solid
        .faces
        .iter()
        .filter_map(|(id, _)| plane_of_face(solid, id).map(|f| (id, f.z.dot(normal))))
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .filter(|(_, d)| *d > 0.999)
        .map(|(id, _)| id)
        .expect("Fläche mit dieser Normalen")
}

/// Eckpunkte der Fläche aus dem Anzeigenetz.
fn face_points(s: &AppState, body: usize, face: Id<Face>) -> Vec<DVec3> {
    let mesh = &s.bodies[body].mesh;
    let range = mesh
        .face_ranges
        .iter()
        .find(|r| r.face == face)
        .expect("Fläche im Netz");
    let first = range.first_index as usize;
    mesh.indices[first..first + range.index_count as usize]
        .iter()
        .map(|&i| mesh.positions[i as usize])
        .collect()
}

/// Bildschirmlage der Punkte (Hülle) mit der aktuellen Kamera.
fn screen_box(s: &AppState, points: &[DVec3]) -> Rect {
    let full = glam::Vec2::new(SIZE.x, SIZE.y);
    let mut r = Rect::NOTHING;
    for p in points {
        let q = s
            .camera
            .project(glam::Vec3::new(p.x as f32, p.y as f32, p.z as f32), full)
            .expect("vor der Kamera");
        r.extend_with(Pos2::new(q.x, q.y));
    }
    r
}

/// Die Punkte liegen ganz im freien Rechteck, mittig (auf 3 px) und füllen mindestens 70 % seiner
/// Breite oder Höhe.
fn assert_framed(what: &str, s: &AppState, free: Rect, points: &[DVec3]) {
    let b = screen_box(s, points);
    assert!(
        free.expand(0.5).contains_rect(b),
        "{what}: {b:?} liegt nicht im freien Rechteck {free:?}"
    );
    assert!(
        (b.center() - free.center()).length() < 3.0,
        "{what}: Mitte {:?} statt {:?}",
        b.center(),
        free.center()
    );
    let fill = (b.width() / free.width()).max(b.height() / free.height());
    assert!(fill >= 0.7, "{what}: füllt nur {:.0} %", fill * 100.0);
}

/// Blickrichtung (vom Auge zum Ziel) weicht höchstens 1° von `dir` ab.
fn assert_looks_along(what: &str, s: &AppState, dir: DVec3) {
    let v = (s.camera.target - s.camera.eye()).normalize();
    let d = glam::Vec3::new(dir.x as f32, dir.y as f32, dir.z as f32).normalize();
    assert!(
        v.dot(d) > 1f32.to_radians().cos(),
        "{what}: Blick {v:?} statt {d:?}"
    );
}

/// App mit einem Quader 100 × 60 × 40 (ein Bild gezeichnet, damit die Kamera das freie
/// Rechteck des Modellmodus kennt).
fn app_with_box(c: &egui::Context) -> PlanoraApp {
    let mut app = new_app();
    let s = state(&mut app);
    s.create_box(DVec3::new(100.0, 60.0, 40.0)).unwrap();
    s.frame_all();
    settle(c, &mut app);
    app
}

/// Wie das Menü „Skizze auf gewählter Fläche“: Fläche wählen, Skizze anlegen und öffnen. Das
/// geschieht vor der 3D-Ansicht des nächsten Bildes, die Kamera kennt also noch das freie
/// Rechteck des Modellmodus.
fn sketch_on_face(app: &mut PlanoraApp, body: usize, face: Id<Face>) {
    let s = state(app);
    s.set_selection(Some(Pick::Face(FaceRef { body, face })));
    let id = s.new_sketch_on_face(body, face).unwrap();
    s.open_sketch(id).unwrap();
}

#[test]
fn a_new_sketch_on_the_top_face_fills_the_free_area() {
    let c = context();
    let mut app = app_with_box(&c);
    let top = face_along(state(&mut app), 0, DVec3::Z);
    let before = state(&mut app).camera;
    sketch_on_face(&mut app, 0, top);
    settle(&c, &mut app);
    let free = free(&c);
    let s = state(&mut app);
    assert!(s.sketch_session.is_some());
    assert_framed("Deckel", s, free, &face_points(s, 0, top));
    assert_looks_along("Deckel", s, -DVec3::Z);

    // Beenden stellt die Kamera von vorher wieder her.
    s.close_sketch();
    assert_eq!(
        (
            s.camera.target,
            s.camera.distance,
            s.camera.yaw,
            s.camera.pitch
        ),
        (before.target, before.distance, before.yaw, before.pitch)
    );
    frame(&c, &mut app);
    assert_eq!(state(&mut app).camera.target, before.target);
}

#[test]
fn a_new_sketch_on_a_side_face_fills_the_free_area() {
    let c = context();
    let mut app = app_with_box(&c);
    for normal in [DVec3::NEG_Y, DVec3::X] {
        let face = face_along(state(&mut app), 0, normal);
        sketch_on_face(&mut app, 0, face);
        settle(&c, &mut app);
        let free = free(&c);
        let s = state(&mut app);
        assert_framed(&format!("{normal:?}"), s, free, &face_points(s, 0, face));
        assert_looks_along(&format!("{normal:?}"), s, -normal);
        s.close_sketch();
        settle(&c, &mut app);
    }
}

#[test]
fn a_face_of_an_off_centre_body_is_centred() {
    let c = context();
    let mut app = app_with_box(&c);
    {
        let s = state(&mut app);
        s.create_box(DVec3::new(30.0, 20.0, 10.0)).unwrap();
        s.move_body(1, DVec3::new(250.0, -120.0, 35.0)).unwrap();
        s.frame_all();
    }
    settle(&c, &mut app);
    let top = face_along(state(&mut app), 1, DVec3::Z);
    sketch_on_face(&mut app, 1, top);
    settle(&c, &mut app);
    let free = free(&c);
    let s = state(&mut app);
    assert_framed("versetzter Deckel", s, free, &face_points(s, 1, top));
    assert_looks_along("versetzter Deckel", s, -DVec3::Z);
}

#[test]
fn the_first_frame_already_uses_the_free_area_of_the_sketch_mode() {
    // Die Kamera kennt beim Öffnen ein ganz anderes freies Rechteck (hier ein flaches, schmales):
    // Das erste Bild im Skizzenmodus passt mit dem richtigen nach.
    let c = context();
    let mut app = app_with_box(&c);
    let top = face_along(state(&mut app), 0, DVec3::Z);
    state(&mut app).camera.set_visible(
        glam::Vec2::new(SIZE.x, SIZE.y),
        (glam::Vec2::new(10.0, 10.0), glam::Vec2::new(300.0, 60.0)),
    );
    sketch_on_face(&mut app, 0, top);
    frame(&c, &mut app);
    let free = free(&c);
    let s = state(&mut app);
    assert_framed("erstes Bild", s, free, &face_points(s, 0, top));
}

#[test]
fn moving_the_view_after_opening_is_kept() {
    let c = context();
    let mut app = app_with_box(&c);
    let top = face_along(state(&mut app), 0, DVec3::Z);
    // Gezoomt nach 0, 1 und 3 Bildern: Das Nachpassen überschreibt die Ansicht des Benutzers nie.
    for frames_before in [0, 1, 3] {
        sketch_on_face(&mut app, 0, top);
        for _ in 0..frames_before {
            frame(&c, &mut app);
        }
        let s = state(&mut app);
        s.camera.zoom(300.0);
        s.camera.pan(40.0, -25.0, SIZE.y);
        let (zoomed, target) = (s.camera.distance, s.camera.target);
        settle(&c, &mut app);
        let s = state(&mut app);
        assert_eq!(
            (s.camera.distance, s.camera.target),
            (zoomed, target),
            "nach {frames_before} Bildern: Ansicht bleibt"
        );
        s.close_sketch();
        settle(&c, &mut app);
    }
}

#[test]
fn an_empty_sketch_on_an_origin_plane_frames_the_bodies() {
    let c = context();
    let mut app = app_with_box(&c);
    let id = state(&mut app)
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    state(&mut app).open_sketch(id).unwrap();
    settle(&c, &mut app);
    let free = free(&c);
    let s = state(&mut app);
    // Hülle der Körper, in die Ebene gelegt.
    let (lo, hi) = s.bodies[0].bounds;
    let corners: Vec<DVec3> = [(lo.x, lo.y), (hi.x, lo.y), (hi.x, hi.y), (lo.x, hi.y)]
        .iter()
        .map(|&(x, y)| DVec3::new(x, y, 0.0))
        .collect();
    assert_framed("XY mit Quader", s, free, &corners);
    assert_looks_along("XY mit Quader", s, -DVec3::Z);
}

#[test]
fn an_empty_sketch_on_an_origin_plane_without_bodies_frames_50_mm() {
    let c = context();
    let mut app = new_app();
    settle(&c, &mut app);
    for (plane, normal) in [
        (StandardPlane::XY, DVec3::Z),
        (StandardPlane::XZ, DVec3::NEG_Y),
        (StandardPlane::YZ, DVec3::X),
    ] {
        let s = state(&mut app);
        let id = s.add_sketch(SketchPlane::Standard(plane)).unwrap();
        s.open_sketch(id).unwrap();
        settle(&c, &mut app);
        let free = free(&c);
        let s = state(&mut app);
        let frame = *s.session_sketch().unwrap().frame();
        let corners: Vec<DVec3> = [(-25.0, -25.0), (25.0, -25.0), (25.0, 25.0), (-25.0, 25.0)]
            .iter()
            .map(|&(x, y)| frame.point_2d(DVec2::new(x, y)))
            .collect();
        assert_framed(&format!("{plane:?}"), s, free, &corners);
        assert_looks_along(&format!("{plane:?}"), s, -frame.z);
        let _ = normal;
        s.close_sketch();
        settle(&c, &mut app);
    }
}

#[test]
fn a_sketch_with_geometry_frames_its_geometry() {
    let c = context();
    let mut app = app_with_box(&c);
    let s = state(&mut app);
    let id = s
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    s.open_sketch(id).unwrap();
    {
        let (_, sk) = s.session_sketch_mut().unwrap();
        let a = sk.add_point(20.0, 10.0).unwrap();
        let b = sk.add_point(60.0, 10.0).unwrap();
        let d = sk.add_point(60.0, 40.0).unwrap();
        sk.add_line(a, b).unwrap();
        sk.add_line(b, d).unwrap();
    }
    s.close_sketch();
    settle(&c, &mut app);
    state(&mut app).open_sketch(id).unwrap();
    settle(&c, &mut app);
    let free = free(&c);
    let s = state(&mut app);
    let corners: Vec<DVec3> = [(20.0, 10.0), (60.0, 10.0), (60.0, 40.0), (20.0, 40.0)]
        .iter()
        .map(|&(x, y)| DVec3::new(x, y, 0.0))
        .collect();
    assert_framed("Skizze mit Linien", s, free, &corners);
}

#[test]
fn a_face_that_no_longer_resolves_falls_back_to_the_bodies() {
    let c = context();
    let mut app = app_with_box(&c);
    let top = face_along(state(&mut app), 0, DVec3::Z);
    let s = state(&mut app);
    let id = s.new_sketch_on_face(0, top).unwrap();
    // Die Fläche verliert ihre Marke: Der Verweis der Skizze findet sie nicht mehr.
    s.bodies[0].solid.faces[top].tag = None;
    s.open_sketch(id).unwrap();
    settle(&c, &mut app);
    let free = free(&c);
    let s = state(&mut app);
    assert!(s.sketch_session.is_some());
    assert!(s.camera.target.is_finite() && s.camera.distance.is_finite());
    assert_looks_along("ohne Fläche", s, -DVec3::Z);
    // Ersatz: die Hülle der Körper in der Skizzenebene.
    let frame = *s.session_sketch().unwrap().frame();
    let (lo, hi) = s.bodies[0].bounds;
    let corners: Vec<DVec3> = [(lo.x, lo.y), (hi.x, lo.y), (hi.x, hi.y), (lo.x, hi.y)]
        .iter()
        .map(|&(x, y)| DVec3::new(x, y, frame.origin.z))
        .collect();
    assert_framed("ohne Fläche", s, free, &corners);
}

#[test]
fn wheel_over_the_cube_zooms_in_sketch_mode_without_orbit() {
    let c = context();
    let mut app = app_with_box(&c);
    let top = face_along(state(&mut app), 0, DVec3::Z);
    sketch_on_face(&mut app, 0, top);
    settle(&c, &mut app);
    let p = planora_app::viewport::cube_rect(free(&c)).center();
    let (yaw, pitch, dist) = {
        let k = state(&mut app).camera;
        (k.yaw, k.pitch, k.distance)
    };
    let mut f = eframe::Frame::_new_kittest();
    for events in [
        vec![egui::Event::PointerMoved(p)],
        vec![
            egui::Event::PointerMoved(p),
            egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: vec2(0.0, 40.0),
                phase: egui::TouchPhase::Move,
                modifiers: egui::Modifiers::NONE,
            },
        ],
        vec![],
    ] {
        c.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, SIZE)),
                events,
                ..Default::default()
            },
            |ui| app.ui(ui, &mut f),
        )
        .drop_without_applying_deltas();
    }
    let k = state(&mut app).camera;
    assert_ne!(k.distance, dist, "Mausrad zoomt über dem Würfel");
    assert_eq!((k.yaw, k.pitch), (yaw, pitch));
}
