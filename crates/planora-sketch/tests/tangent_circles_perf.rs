//! Messtest: Tangentialkreis an Linie, Kreis und Bogen, 200 Zeigerlagen.
//! `cargo test --release -p planora-sketch --test tangent_circles_perf -- --ignored --nocapture`

use planora_math::{DVec2, Frame, Tolerance};
use planora_sketch::Sketch;
use planora_sketch::tangent_circles::tangent_circle_three;
use std::time::Instant;

#[test]
#[ignore = "Messtest, nur im Release-Build sinnvoll"]
fn two_hundred_pointers_median_under_4_ms() {
    let mut s = Sketch::new(Frame::WORLD, "S", Tolerance::default());
    let (a, b) = (
        s.add_point(-50.0, 0.0).unwrap(),
        s.add_point(50.0, 0.0).unwrap(),
    );
    let l = s.add_line(a, b).unwrap();
    let m = s.add_point(0.0, 30.0).unwrap();
    let c = s.add_circle(m, 10.0).unwrap();
    let (am, a0, a1) = (
        s.add_point(30.0, 30.0).unwrap(),
        s.add_point(30.0, 38.0).unwrap(),
        s.add_point(22.0, 30.0).unwrap(),
    );
    let arc = s.add_arc(am, a0, a1).unwrap();

    let mut times: Vec<f64> = (0..200)
        .map(|i| {
            let (x, y) = (f64::from(i % 20), f64::from(i / 20));
            let p = DVec2::new(5.0 + x * 1.3, 2.0 + y * 2.1);
            let t = Instant::now();
            let got = tangent_circle_three(&s, [l, c, arc], p);
            let ms = t.elapsed().as_secs_f64() * 1e3;
            std::hint::black_box(got);
            ms
        })
        .collect();
    times.sort_by(f64::total_cmp);
    let (median, worst) = (times[100], times[199]);
    println!("Median {median:.3} ms, schlechtester Lauf {worst:.3} ms");
    assert!(median < 4.0, "Median {median} ms");
}
