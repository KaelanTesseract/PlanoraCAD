//! Messtest: 10 000 Linien einfügen (Skizzen 9, Spec B „Leistung“).
//! `cargo test -p planora-sketch --release --test import_perf -- --ignored --nocapture`

use planora_exchange::import2d::{Element2d, Import2d, Layer2d};
use planora_math::{DVec2, Frame, Tolerance};
use planora_sketch::import::{InsertOptions, Placement};
use planora_sketch::model::Sketch;
use planora_sketch::text_shape::{TextShape, TextShapeError, TextShaper, TextStyle};
use std::time::Instant;

struct NoText;

impl TextShaper for NoText {
    fn shape(&self, _: &str, _: &TextStyle) -> Result<TextShape, TextShapeError> {
        Err(TextShapeError::Empty)
    }
}

/// 2500 Quadrate (50 × 50) aus je vier Linien: 10 000 Linien, 2500 Profile.
fn squares() -> Import2d {
    let mut elements = Vec::with_capacity(10_000);
    for i in 0..50 {
        for j in 0..50 {
            let o = DVec2::new(i as f64 * 3.0, j as f64 * 3.0);
            let p = [
                o,
                o + DVec2::new(2.0, 0.0),
                o + DVec2::new(2.0, 2.0),
                o + DVec2::new(0.0, 2.0),
            ];
            for k in 0..4 {
                elements.push(Element2d::Line(p[k], p[(k + 1) % 4]));
            }
        }
    }
    Import2d {
        layers: vec![Layer2d {
            name: "0".into(),
            elements,
        }],
        ..Default::default()
    }
}

fn inserted(fixed: bool) -> (Sketch, f64) {
    let imp = squares();
    let mut s = Sketch::new(Frame::WORLD, "S", Tolerance::default());
    let opt = InsertOptions {
        fixed,
        ..Default::default()
    };
    let t = Instant::now();
    let r = s
        .insert_geometry(&imp, &Placement::default(), &opt, &NoText)
        .unwrap();
    let secs = t.elapsed().as_secs_f64();
    assert_eq!(r.curves, 10_000);
    assert_eq!(r.points, 10_000);
    (s, secs)
}

#[test]
#[ignore = "Messtest, nur im Release-Build sinnvoll"]
fn ten_thousand_lines_insert_under_2s() {
    for fixed in [false, true] {
        let (_, secs) = inserted(fixed);
        println!("10 000 Linien einfügen (fest: {fixed}): {:.3} s", secs);
        assert!(secs < 2.0, "{secs} s");
    }
}

#[test]
#[ignore = "Messtest, nur im Release-Build sinnvoll"]
fn ten_thousand_lines_profiles_measured() {
    let (s, _) = inserted(false);
    let t = Instant::now();
    let p = s.profiles();
    let secs = t.elapsed().as_secs_f64();
    println!(
        "Profile von 10 000 Linien: {} Profile in {:.3} s",
        p.profiles.len(),
        secs
    );
    assert_eq!(p.profiles.len(), 2500);
}
