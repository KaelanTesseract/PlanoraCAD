//! Messtest (Skizzen 2, Task 2): Die Fang-Suche bleibt bei 200 Kurven unter 4 ms je Bild.
//! Im Release laufen lassen:
//! `cargo test -p planora-app --release --test sketch_snap_perf -- --ignored`.

use planora_app::sketch_state::{SketchSession, Tool};
use planora_feature::FeatureId;
use planora_math::{DVec2, Frame, Tolerance};
use planora_sketch::model::Sketch;
use std::time::{Duration, Instant};

#[test]
#[ignore = "Messtest, nur im Release sinnvoll"]
fn snap_search_under_4_ms() {
    let mut sk = Sketch::new(Frame::WORLD, "S", Tolerance::default());
    // 200 Kurven in einem Raster: Linien, die sich mit den Nachbarn schneiden, und Kreise.
    for k in 0..200 {
        let x = f64::from(k % 20) * 40.0;
        let y = f64::from(k / 20) * 40.0;
        if k % 2 == 0 {
            let a = sk.add_point(x - 5.0, y - 3.0).unwrap();
            let b = sk.add_point(x + 45.0, y + 7.0).unwrap();
            sk.add_line(a, b).unwrap();
        } else {
            let m = sk.add_point(x, y).unwrap();
            sk.add_circle(m, 24.0).unwrap();
        }
    }
    let mut se = SketchSession::new(FeatureId(0));
    se.set_tool(&mut sk, Tool::Line);
    // Linie läuft, Index gebaut (das zählt nicht zur Suche je Bild).
    se.click(&mut sk, DVec2::new(-100.0, -100.0), 2.0, false)
        .unwrap();
    se.resolve_cursor(&sk, DVec2::ZERO, 2.0);

    let mut times: Vec<Duration> = (0..200)
        .map(|k| {
            let raw = DVec2::new(
                f64::from(k % 20) * 40.0 + 13.7,
                f64::from(k / 20) * 40.0 + 11.3,
            );
            let t = Instant::now();
            std::hint::black_box(se.resolve_cursor(&sk, raw, 2.0));
            t.elapsed()
        })
        .collect();
    times.sort();
    let median = times[times.len() / 2];
    println!("Median {median:?}, Maximum {:?}", times[times.len() - 1]);
    assert!(median < Duration::from_millis(4), "Median {median:?}");
}
