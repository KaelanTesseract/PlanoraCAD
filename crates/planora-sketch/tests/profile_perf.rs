//! Messtest: Profile für 200 Kurven unter 50 ms im Release-Build.
//! `cargo test --release -p planora-sketch --test profile_perf -- --ignored`

use planora_math::{Frame, Tolerance};
use planora_sketch::model::Sketch;
use std::time::Instant;

/// 10×10-Gitter aus 20 Linien, 100 Kreise (r = 0,4) an den Gitterpunkten, 80 Dreiviertelbögen
/// (r = 0,35) in den Zellen, die die Kreise an den Ecken schneiden.
fn grid_sketch() -> Sketch {
    let mut s = Sketch::new(Frame::WORLD, "S", Tolerance::default());
    for i in 0..10 {
        let x = i as f64;
        let a = s.add_point(x, -1.0).unwrap();
        let b = s.add_point(x, 10.0).unwrap();
        s.add_line(a, b).unwrap();
        let a = s.add_point(-1.0, x).unwrap();
        let b = s.add_point(10.0, x).unwrap();
        s.add_line(a, b).unwrap();
    }
    for i in 0..10 {
        for j in 0..10 {
            let c = s.add_point(i as f64, j as f64).unwrap();
            s.add_circle(c, 0.4).unwrap();
        }
    }
    for k in 0..80 {
        let (x, y) = ((k % 9) as f64 + 0.5, (k / 9) as f64 + 0.5);
        let c = s.add_point(x, y).unwrap();
        let a = s.add_point(x + 0.35, y).unwrap();
        let b = s.add_point(x, y - 0.35).unwrap();
        s.add_arc(c, a, b).unwrap();
    }
    s
}

#[test]
#[ignore = "Messtest, nur im Release-Build sinnvoll"]
fn two_hundred_curves_under_50_ms() {
    let s = grid_sketch();
    assert_eq!(s.curves().count(), 200);
    let first = s.profiles();
    assert!(first.rejected.is_empty(), "{:?}", first.rejected);
    assert!(first.profiles.len() > 400, "{}", first.profiles.len());
    let mut ms: Vec<f64> = (0..5)
        .map(|_| {
            let t = Instant::now();
            let e = s.profiles();
            let dt = t.elapsed().as_secs_f64() * 1e3;
            assert_eq!(e.profiles.len(), first.profiles.len());
            dt
        })
        .collect();
    ms.sort_by(f64::total_cmp);
    let median = ms[2];
    println!("profiles() für 200 Kurven: Median {median:.1} ms ({ms:?})");
    assert!(median < 50.0, "Median {median:.1} ms");
}
