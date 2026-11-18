//! Zeitmessung: Zeichnen fertiger Skizzen in der 3D-Ansicht (Skizzen 12, Task 4).
//!
//! Nur von Hand im Release-Build:
//! `cargo test --release -p planora-app --test sketch_draw_timing -- --ignored --nocapture`.
//! Gemessen wird, was je Bild auf der CPU entsteht: die Linien fertiger Skizzen (seit Task 4
//! [`sketch_view::finished_sketch_strokes`] für die GPU plus was im egui-Overlay bleibt).
//! (a) eine fertige Skizze mit 2000 Kurven, (b) Drehen: 20 fertige Skizzen, 60 Kamerastellungen.
//! Task 5 erweitert den Test um die geöffnete Skizze.

use planora_app::sketch_view;
use planora_app::state::AppState;
use planora_app::viewport::stroke_view;
use planora_feature::{FeatureKind, SketchFeature, SketchPlane, StandardPlane};
use planora_math::{Frame, Tolerance};
use planora_render::StrokeVertex;
use planora_sketch::model::Sketch;
use std::hint::black_box;
use std::time::{Duration, Instant};

const SCREEN: egui::Rect =
    egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1920.0, 1080.0));
/// Pixelfaktor eines üblichen Bildschirms mit 150 % Skalierung.
const PPP: f32 = 1.5;

/// Skizze mit `n` Kurven in einem Raster: Linien, Kreise, Bögen und gestrichelte
/// Konstruktionslinien im Wechsel, um `(dx, dy)` verschoben.
fn sketch(n: usize, dx: f64, dy: f64) -> Sketch {
    let mut sk = Sketch::new(Frame::WORLD, "Skizze", Tolerance::default());
    let cols = (n as f64).sqrt().ceil() as usize;
    for i in 0..n {
        let (x, y) = (dx + 6.0 * (i % cols) as f64, dy + 6.0 * (i / cols) as f64);
        match i % 4 {
            0 | 3 => {
                let a = sk.add_point(x, y).unwrap();
                let b = sk.add_point(x + 4.0, y + 3.0).unwrap();
                let c = sk.add_line(a, b).unwrap();
                if i % 4 == 3 {
                    sk.set_construction(c, true).unwrap();
                }
            }
            1 => {
                let c = sk.add_point(x + 2.0, y + 2.0).unwrap();
                sk.add_circle(c, 2.0).unwrap();
            }
            _ => {
                let c = sk.add_point(x + 2.0, y).unwrap();
                let a = sk.add_point(x + 4.0, y).unwrap();
                let b = sk.add_point(x, y).unwrap();
                sk.add_arc(c, a, b).unwrap();
            }
        }
    }
    sk
}

fn state(sketches: usize, curves: usize) -> AppState {
    let mut s = AppState::new();
    for k in 0..sketches {
        let sk = sketch(curves, 0.0, 80.0 * k as f64);
        s.add_feature(
            Some("Skizze"),
            FeatureKind::Sketch(SketchFeature {
                sketch: sk,
                plane: SketchPlane::Standard(StandardPlane::XY),
                copies: vec![],
                formulas: Default::default(),
                text_formulas: Default::default(),
            }),
        )
        .unwrap();
    }
    s.camera.set_viewport(SCREEN.width(), SCREEN.height());
    s.camera.set_iso_view();
    s
}

/// Was ein Bild für die fertigen Skizzen baut; Rückgabe: Zahl der Formen plus Eckpunkte.
fn frame(s: &AppState, strokes: &mut Vec<StrokeVertex>) -> usize {
    strokes.clear();
    let view = stroke_view(s, SCREEN, PPP);
    sketch_view::finished_sketch_strokes(s, &view, strokes);
    let shapes = sketch_view::finished_sketch_shapes(s, SCREEN);
    shapes.len() + strokes.len()
}

fn median(mut v: Vec<Duration>) -> Duration {
    v.sort();
    v[v.len() / 2]
}

#[test]
#[ignore = "Zeitmessung, nur im Release-Build von Hand"]
fn finished_sketch_draw_timing() {
    let mut strokes = Vec::new();

    // (a) Eine Skizze mit 2000 Kurven.
    let s = state(1, 2000);
    black_box(frame(&s, &mut strokes));
    let runs: Vec<Duration> = (0..30)
        .map(|_| {
            let t = Instant::now();
            black_box(frame(&s, &mut strokes));
            t.elapsed()
        })
        .collect();
    println!(
        "(a) 2000 Kurven, fertige Skizze: Median {:.3} ms je Bild",
        median(runs).as_secs_f64() * 1e3
    );

    // (b) Drehen: 20 Skizzen zu je 100 Kurven, 60 Kamerastellungen.
    let mut s = state(20, 100);
    s.camera.distance = 1500.0;
    let yaw0 = s.camera.yaw;
    let mut per_frame = Vec::new();
    for _ in 0..3 {
        for k in 0..60 {
            s.camera.yaw = yaw0 + k as f32 * std::f32::consts::TAU / 60.0;
            let t = Instant::now();
            black_box(frame(&s, &mut strokes));
            per_frame.push(t.elapsed());
        }
    }
    println!(
        "(b) Drehen, 20 Skizzen, 60 Stellungen: Median {:.3} ms je Bild",
        median(per_frame).as_secs_f64() * 1e3
    );
}
