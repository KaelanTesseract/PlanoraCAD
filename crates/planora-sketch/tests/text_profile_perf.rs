//! Messtests für Text in der Skizze ohne Schriften (Skizzen 8, Task 9). Die Messungen mit echten
//! Glyphen liegen in `planora-text/tests/real_glyph_profiles.rs`.
//!
//! `cargo test --release -p planora-sketch --test text_profile_perf -- --ignored`

use planora_kernel::Curve;
use planora_math::nurbs::NurbsCurve;
use planora_math::{DVec2, DVec3, Frame, Tolerance};
use planora_sketch::helpers::ProjectStyle;
use planora_sketch::model::Sketch;
use planora_sketch::solver::{Drag, SolveStatus};
use planora_sketch::text::{HAlign, TextParams};
use planora_sketch::text_shape::{Contour, Glyph, Seg, ShapedLine, TextShape, TextStyle};
use std::sync::Arc;
use std::time::Instant;

/// Läufe je Messung; gemeldet und geprüft wird der Median.
const RUNS: usize = 11;
/// Vorschub je Zeichen (Einheiten der Höhe).
const ADV: f64 = 0.8;

fn v(x: f64, y: f64) -> DVec2 {
    DVec2::new(x, y)
}

/// Je Zeichen ein Quadrat mit Loch aus Strecke, quadratischer und kubischer Bézierkurve
/// (8 Kurven), auf einer Zeile.
fn shape(chars: usize) -> TextShape {
    let glyph = |x: f64| Glyph {
        origin: v(x, 0.0),
        advance: ADV,
        contours: vec![
            Contour {
                start: v(0.0, 0.0),
                segs: vec![
                    Seg::Line(v(0.6, 0.0)),
                    Seg::Line(v(0.6, 0.7)),
                    Seg::Line(v(0.0, 0.7)),
                    Seg::Line(v(0.0, 0.0)),
                ],
            },
            Contour {
                start: v(0.2, 0.2),
                segs: vec![
                    Seg::Line(v(0.4, 0.2)),
                    Seg::Quad(v(0.5, 0.35), v(0.4, 0.5)),
                    Seg::Cubic(v(0.35, 0.55), v(0.25, 0.55), v(0.2, 0.5)),
                    Seg::Line(v(0.2, 0.2)),
                ],
            },
        ],
    };
    TextShape {
        lines: vec![ShapedLine {
            glyphs: (0..chars).map(|i| glyph(i as f64 * ADV)).collect(),
            width: chars as f64 * ADV,
        }],
        ascender: 0.9,
        descender: -0.2,
        line_height: 1.2,
        substituted: None,
        synthetic: false,
        missing_chars: false,
    }
}

fn params(chars: usize) -> TextParams {
    TextParams {
        content: "x".repeat(chars),
        style: TextStyle {
            font: "Test".into(),
            bold: false,
            italic: false,
            spacing: 0.0,
        },
        height: 5.0,
        flip_h: false,
        flip_v: false,
    }
}

/// Kubische Freiformkurve mit `spans` Bézier-Stücken (gleichmäßige, geklemmte Knoten), eine
/// Welle längs x mit 4 mm je Stück.
fn wave(spans: usize) -> NurbsCurve {
    let n = spans + 3;
    let mut knots = vec![0.0; 4];
    knots.extend((1..spans).map(|k| k as f64));
    knots.extend([spans as f64; 4]);
    let control: Vec<DVec3> = (0..n)
        .map(|i| {
            let x = 4.0 * i as f64;
            DVec3::new(x, 6.0 * (0.37 * i as f64).sin(), 0.0)
        })
        .collect();
    NurbsCurve::new(3, knots, control, vec![1.0; n]).unwrap()
}

fn median(mut ms: Vec<f64>) -> f64 {
    ms.sort_by(f64::total_cmp);
    ms[ms.len() / 2]
}

/// Ein Lösen (vor und nach dem Lösen wird der Text auf dem Pfad neu gelegt) mit 200 Zeichen
/// auf einer projizierten Freiformkurve mit vielen Knoten. Der Pfad bleibt gleich, die
/// Längentabelle wird nur beim ersten Mal gebaut.
#[test]
#[ignore = "Messtest, nur im Release-Build sinnvoll"]
fn path_text_on_long_nurbs_refresh_under_20ms() {
    for spans in [250, 1000] {
        let mut s = Sketch::new(Frame::WORLD, "S", Tolerance::default());
        let edge = Curve::Nurbs(Arc::new(wave(spans)));
        let path = s
            .add_projected_curve(
                &edge,
                0.0,
                spans as f64,
                ProjectStyle { construction: true },
            )
            .unwrap();
        let t = s
            .add_text_path(
                path,
                params(200),
                true,
                false,
                HAlign::Center,
                Arc::new(shape(200)),
            )
            .unwrap();
        let curves = s.text(t).unwrap().curves.len();
        let ms: Vec<f64> = (0..RUNS)
            .map(|_| {
                let t0 = Instant::now();
                let r = s.solve();
                let dt = t0.elapsed().as_secs_f64() * 1e3;
                assert_eq!(r.status, SolveStatus::Solved);
                dt
            })
            .collect();
        let med = median(ms.clone());
        println!(
            "Text auf Freiformkurve ({spans} Stücke, 200 Zeichen, {curves} Kurven): Lösen Median \
             {med:.2} ms ({ms:.2?})"
        );
        assert!(med <= 20.0, "{spans} Stücke: Median {med:.2} ms");
    }
}

/// Ein Bild beim Ziehen am Pfad: Spline mit 40 Kontrollpunkten (37 Bézier-Stücke), 200 Zeichen.
/// Hier ändert sich der Pfad in jedem Bild, die Längentabelle wird jedes Mal neu gebaut.
#[test]
#[ignore = "Messtest, nur im Release-Build sinnvoll"]
fn path_text_on_dragged_spline_under_20ms() {
    let mut s = Sketch::new(Frame::WORLD, "S", Tolerance::default());
    let points: Vec<_> = (0..40)
        .map(|i| {
            s.add_point(25.0 * i as f64, 10.0 * (0.5 * i as f64).sin())
                .unwrap()
        })
        .collect();
    let path = s.add_control_spline(&points, 3).unwrap();
    let t = s
        .add_text_path(
            path,
            params(200),
            true,
            false,
            HAlign::Center,
            Arc::new(shape(200)),
        )
        .unwrap();
    let curves = s.text(t).unwrap().curves.len();
    let grip = points[20];
    let start = s.point_xy(grip).unwrap();
    let first = s.point_xy(s.text(t).unwrap().points[0]).unwrap();
    let ms: Vec<f64> = (0..RUNS)
        .map(|k| {
            let target = start + v(0.0, 0.5 * (k + 1) as f64);
            let t0 = Instant::now();
            let r = s
                .drag(&[Drag {
                    point: grip,
                    target,
                }])
                .unwrap();
            let dt = t0.elapsed().as_secs_f64() * 1e3;
            assert_eq!(r.status, SolveStatus::Solved);
            dt
        })
        .collect();
    let moved = s.point_xy(s.text(t).unwrap().points[0]).unwrap();
    assert!(moved.distance(first) > 0.0, "der Text folgt dem Pfad");
    let med = median(ms.clone());
    println!(
        "Zug am Spline-Pfad (37 Stücke, 200 Zeichen, {curves} Kurven): Median {med:.2} ms \
         ({ms:.2?})"
    );
    assert!(med <= 20.0, "Median {med:.2} ms");
}
