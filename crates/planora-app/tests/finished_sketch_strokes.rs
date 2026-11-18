//! Fertige Skizzen, Profile in Dialogen und Konstruktionsebenen als Linien und Flächen für die
//! GPU mit Tiefenprüfung (Skizzen 12, Task 4): Körper verdecken sie wie in Fusion.

use planora_app::sketch_view;
use planora_app::state::{AppState, ProfilePick, StandardPlane};
use planora_app::testing::{fill_triangles, line_triangles};
use planora_app::theme;
use planora_app::viewport::stroke_view;
use planora_feature::planes::{PlaneBase, PlaneDef};
use planora_feature::{FeatureId, SketchPlane};
use planora_math::DVec2;
use planora_render::StrokeVertex;
use planora_sketch::model::Sketch;

const SCREEN: egui::Rect =
    egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1920.0, 1080.0));

fn prepared(mut s: AppState) -> AppState {
    s.camera.set_viewport(SCREEN.width(), SCREEN.height());
    s.camera.set_iso_view();
    s
}

fn edit_sketch<T>(s: &mut AppState, id: FeatureId, f: impl FnOnce(&mut Sketch) -> T) -> T {
    s.open_sketch(id).unwrap();
    let r = f(s.session_sketch_mut().unwrap().1);
    s.close_sketch();
    r
}

/// Fertige Skizze auf XY mit einer Linie und einer Konstruktionslinie.
fn line_and_construction() -> (AppState, FeatureId) {
    let mut s = prepared(AppState::new());
    let id = s
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    edit_sketch(&mut s, id, |sk| {
        let a = sk.add_point(0.0, 0.0).unwrap();
        let b = sk.add_point(20.0, 0.0).unwrap();
        sk.add_line(a, b).unwrap();
        let c = sk.add_point(0.0, 10.0).unwrap();
        let d = sk.add_point(20.0, 10.0).unwrap();
        let l = sk.add_line(c, d).unwrap();
        sk.set_construction(l, true).unwrap();
    });
    (s, id)
}

fn strokes(s: &AppState, ppp: f32) -> Vec<StrokeVertex> {
    let mut out = Vec::new();
    sketch_view::finished_sketch_strokes(s, &stroke_view(s, SCREEN, ppp), &mut out);
    out
}

/// Abstand der beiden ersten Ecken eines Linienvierecks in physischen Pixeln, ohne den Saum der
/// Kantenglättung (= Strichbreite).
fn quad_width(v: &[StrokeVertex], size_px: egui::Vec2) -> f32 {
    let px = |p: [f32; 3]| egui::vec2(p[0] * size_px.x * 0.5, p[1] * size_px.y * 0.5);
    (px(v[0].ndc) - px(v[1].ndc)).length() - 2.0 * planora_render::STROKE_FEATHER_PX
}

#[test]
fn finished_sketch_emits_strokes() {
    let (s, _) = line_and_construction();
    let col = theme::sketch_colors(s.dark);
    let out = strokes(&s, 1.0);
    let solid = line_triangles(&out, col.finished);
    let dashed = line_triangles(&out, col.construction);
    assert_eq!(solid, 2, "eine Linie ist ein Viereck aus zwei Dreiecken");
    assert!(
        dashed > 2,
        "Konstruktion gestrichelt: viele Stücke ({dashed})"
    );
    // Breiten mal Pixelfaktor: 1,2 und 1,0 Punkte bei 2 Pixeln je Punkt.
    let out = strokes(&s, 2.0);
    let size = SCREEN.size() * 2.0;
    let first = |c: egui::Color32| {
        let i = out
            .iter()
            .position(|v| v.color == sketch_view::stroke_rgba(c) && v.edge != 0.0)
            .unwrap();
        quad_width(&out[i..], size)
    };
    assert!(
        (first(col.finished) - 2.4).abs() < 1e-3,
        "{}",
        first(col.finished)
    );
    assert!((first(col.construction) - 2.0).abs() < 1e-3);
    // Die egui-Linien entfallen.
    assert!(sketch_view::finished_sketch_shapes(&s, SCREEN).is_empty());
}

#[test]
fn hidden_sketch_emits_nothing() {
    let (mut s, id) = line_and_construction();
    assert!(!strokes(&s, 1.0).is_empty());
    s.set_feature_hidden(id, true);
    assert!(strokes(&s, 1.0).is_empty());
}

#[test]
fn open_sketch_not_in_finished_strokes() {
    let (mut s, id) = line_and_construction();
    s.open_sketch(id).unwrap();
    assert!(
        strokes(&s, 1.0).is_empty(),
        "die geöffnete Skizze zeichnet das Skizzen-Overlay"
    );
    s.close_sketch();
    assert!(!strokes(&s, 1.0).is_empty());
}

#[test]
fn profile_pick_adds_fills() {
    let mut s = prepared(AppState::new());
    let id = s
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    edit_sketch(&mut s, id, |sk| {
        sk.add_rectangle(DVec2::ZERO, DVec2::new(10.0, 4.0))
            .unwrap();
    });
    let col = theme::sketch_colors(s.dark);
    assert_eq!(fill_triangles(&strokes(&s, 1.0), col.profile_fill), 0);
    s.profile_pick = Some(ProfilePick {
        sketch: id,
        selected: vec![],
        hover: vec![],
        clicked: None,
    });
    let out = strokes(&s, 1.0);
    assert!(
        fill_triangles(&out, col.profile_fill) >= 2,
        "Rechteck gefüllt"
    );
    s.profile_pick.as_mut().unwrap().selected = vec![0];
    let out = strokes(&s, 1.0);
    assert!(fill_triangles(&out, col.profile_fill_selected) >= 2);
    assert_eq!(fill_triangles(&out, col.profile_fill), 0);
    // Die Füllung liegt unter den Linien: zuerst im Puffer.
    let is_fill = |t: &[StrokeVertex]| t.iter().all(|v| v.edge == 0.0);
    let tris: Vec<&[StrokeVertex]> = out.chunks(3).collect();
    let first_line = tris.iter().position(|t| !is_fill(t)).unwrap();
    assert!(
        tris[first_line..].iter().all(|t| !is_fill(t)),
        "Flächen vor den Linien"
    );
    // Auch eine ausgeblendete Skizze zeigt ihre Profile, solange ein Dialog sie wählt.
    s.set_feature_hidden(id, true);
    assert!(fill_triangles(&strokes(&s, 1.0), col.profile_fill_selected) >= 2);
}

#[test]
fn planes_emit_fill_and_outline() {
    let mut s = prepared(AppState::new());
    let plane = s
        .add_plane(PlaneDef::Offset {
            base: PlaneBase::Standard(StandardPlane::XY),
            distance: 10.0,
        })
        .unwrap();
    let col = theme::sketch_colors(s.dark);
    let mut out = Vec::new();
    sketch_view::plane_strokes(&s, &stroke_view(&s, SCREEN, 1.0), &mut out);
    assert_eq!(fill_triangles(&out, col.plane_fill), 2, "ein Rechteck");
    assert!(line_triangles(&out, col.plane_edge) >= 8, "vier Kanten");
    // Halbdurchsichtig wie bisher.
    let fill = out.iter().find(|v| v.edge == 0.0).unwrap();
    assert!(fill.color[3] < 0.5, "{:?}", fill.color);
    assert_eq!(fill.color, sketch_view::stroke_rgba(col.plane_fill));
    // Gewählt: heller.
    s.selected_feature = Some(plane);
    out.clear();
    sketch_view::plane_strokes(&s, &stroke_view(&s, SCREEN, 1.0), &mut out);
    assert_eq!(fill_triangles(&out, col.plane_fill_active), 2);
    assert!(line_triangles(&out, col.plane_edge_active) >= 8);
}
