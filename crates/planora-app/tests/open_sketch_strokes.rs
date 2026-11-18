//! Die geöffnete Skizze als Linien und Flächen für die GPU mit Tiefenprüfung (Skizzen 12,
//! Task 5): Kurven, Vorschau des Werkzeugs und Profilflächen verdecken Körper wie in Fusion;
//! Punkte, Zeichen, Maße und Marken bleiben im egui-Overlay obenauf.

use egui::epaint::{ClippedShape, ColorMode, Shape};
use planora_app::sketch_state::{Item, Tool};
use planora_app::state::{AppState, StandardPlane};
use planora_app::testing::{fill_triangles, line_triangles};
use planora_app::viewport::stroke_view;
use planora_app::{fonts, sketch_view, theme};
use planora_feature::{FeatureId, SketchPlane};
use planora_math::DVec2;
use planora_render::StrokeVertex;
use planora_sketch::model::CurveId;

const SCREEN: egui::Rect =
    egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1920.0, 1080.0));

/// Geöffnete Skizze auf XY mit einem geschlossenen Rechteck (vier Linien, ein Profil) und einer
/// Konstruktionslinie; Rückgabe: Zustand, Skizze, erste Linie des Rechtecks.
fn open_rectangle() -> (AppState, FeatureId, CurveId) {
    let mut s = AppState::new();
    s.camera.set_viewport(SCREEN.width(), SCREEN.height());
    s.camera.set_iso_view();
    let id = s
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    s.open_sketch(id).unwrap();
    let (_, sk) = s.session_sketch_mut().unwrap();
    let p: Vec<_> = [(0.0, 0.0), (20.0, 0.0), (20.0, 10.0), (0.0, 10.0)]
        .iter()
        .map(|&(x, y)| sk.add_point(x, y).unwrap())
        .collect();
    let first = sk.add_line(p[0], p[1]).unwrap();
    for i in 1..4 {
        sk.add_line(p[i], p[(i + 1) % 4]).unwrap();
    }
    let c = sk.add_point(0.0, 20.0).unwrap();
    let d = sk.add_point(20.0, 20.0).unwrap();
    let l = sk.add_line(c, d).unwrap();
    sk.set_construction(l, true).unwrap();
    (s, id, first)
}

fn strokes(s: &AppState) -> Vec<StrokeVertex> {
    let mut out = Vec::new();
    sketch_view::open_sketch_strokes(s, &stroke_view(s, SCREEN, 1.0), &mut out);
    out
}

/// Formen eines Overlay-Bilds (zwei Bilder, das zweite zählt).
fn overlay_shapes(s: &mut AppState) -> Vec<Shape> {
    let ctx = egui::Context::default();
    ctx.set_fonts(fonts::definitions());
    let mut shapes = Vec::new();
    for _ in 0..2 {
        let raw = egui::RawInput {
            screen_rect: Some(SCREEN),
            ..Default::default()
        };
        let out = ctx.run_ui(raw, |ui| {
            sketch_view::show_overlay(ui, SCREEN, s);
        });
        shapes = out
            .shapes
            .iter()
            .map(|c: &ClippedShape| c.shape.clone())
            .collect();
        out.drop_without_applying_deltas();
    }
    shapes
}

/// Zahl der egui-Linienstücke in der Farbe `color` (auch in verschachtelten Formen).
fn overlay_lines(shapes: &[Shape], color: egui::Color32) -> usize {
    shapes
        .iter()
        .map(|s| match s {
            Shape::Path(p) => {
                usize::from(matches!(p.stroke.color, ColorMode::Solid(c) if c == color))
            }
            Shape::LineSegment { stroke, .. } => usize::from(stroke.color == color),
            Shape::Vec(v) => overlay_lines(v, color),
            _ => 0,
        })
        .sum()
}

#[test]
fn open_sketch_curves_become_strokes() {
    let (mut s, _, first) = open_rectangle();
    let col = theme::sketch_colors(s.dark);
    let ink = sketch_view::curve_color(&s, first);
    let out = strokes(&s);
    assert!(
        line_triangles(&out, ink) >= 8,
        "vier Linien, je ein Viereck aus zwei Dreiecken"
    );
    let dashed = line_triangles(&out, col.construction);
    assert!(
        dashed > 2,
        "Konstruktion gestrichelt: viele Stücke ({dashed})"
    );
    // Im Overlay stehen die Kurven nicht mehr.
    let shapes = overlay_shapes(&mut s);
    assert_eq!(overlay_lines(&shapes, ink), 0, "keine egui-Kurven");
    assert_eq!(overlay_lines(&shapes, col.construction), 0);
    // Ohne geöffnete Skizze: nichts.
    s.close_sketch();
    assert!(strokes(&s).is_empty());
}

#[test]
fn selected_curve_uses_selection_color() {
    let (mut s, _, first) = open_rectangle();
    let col = theme::sketch_colors(s.dark);
    assert_eq!(line_triangles(&strokes(&s), col.selected), 0);
    s.sketch_session.as_mut().unwrap().selection = vec![Item::Curve(first)];
    assert_eq!(sketch_view::curve_color(&s, first), col.selected);
    assert_eq!(
        line_triangles(&strokes(&s), col.selected),
        2,
        "die gewählte Linie in der Auswahlfarbe"
    );
}

#[test]
fn preview_line_is_stroked() {
    let (mut s, _, _) = open_rectangle();
    let col = theme::sketch_colors(s.dark);
    assert_eq!(line_triangles(&strokes(&s), col.preview), 0);
    {
        let (se, sk) = s.session_sketch_mut().unwrap();
        se.set_tool(sk, Tool::Line);
        let a = sk.add_point(40.0, 0.0).unwrap();
        se.pending = vec![a];
        se.cursor = Some(DVec2::new(60.0, 15.0));
    }
    let n = line_triangles(&strokes(&s), col.preview);
    assert!(n > 2, "Vorschau gestrichelt: viele Stücke ({n})");
    let shapes = overlay_shapes(&mut s);
    assert_eq!(
        overlay_lines(&shapes, col.preview),
        0,
        "keine egui-Vorschaulinie"
    );
}

#[test]
fn profiles_follow_display_switch() {
    let (s, id, _) = open_rectangle();
    let col = theme::sketch_colors(s.dark);
    assert!(s.document.sketch_display(id).profiles, "Vorgabe an");
    assert!(
        fill_triangles(&strokes(&s), col.profile_fill) >= 2,
        "das Rechteck als Fläche"
    );
    let mut s = s;
    let mut d = s.document.sketch_display(id);
    d.profiles = false;
    s.document.set_sketch_display(id, d);
    assert_eq!(
        fill_triangles(&strokes(&s), col.profile_fill),
        0,
        "aus: keine Flächen"
    );
}
