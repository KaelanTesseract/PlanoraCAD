//! Messtest: Fang-Index für 200 Kurven.
//! `cargo test --release -p planora-sketch --test snap_perf -- --ignored`

use planora_math::{DVec2, Frame, Tolerance};
use planora_sketch::SnapIndex;
use planora_sketch::model::Sketch;
use std::time::Instant;

/// Wie in `profile_perf`: 20 Linien, 100 Kreise, 80 Dreiviertelbögen.
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
fn two_hundred_curves_find_and_align_under_1_ms_build_under_50_ms() {
    let s = grid_sketch();
    assert_eq!(s.curves().count(), 200);
    let mut builds: Vec<f64> = (0..5)
        .map(|_| {
            let t = Instant::now();
            let _ = SnapIndex::build(&s);
            t.elapsed().as_secs_f64() * 1e3
        })
        .collect();
    builds.sort_by(f64::total_cmp);
    let build = builds[2];
    println!("build: Median {build:.1} ms ({builds:?})");
    assert!(build < 50.0, "build {build:.1} ms");

    let idx = SnapIndex::build(&s);
    let mut per: Vec<f64> = (0..200)
        .map(|k| {
            let at = DVec2::new((k % 14) as f64 * 0.73 - 0.5, (k / 14) as f64 * 0.81 - 0.5);
            let t = Instant::now();
            let _ = idx.find(at, 0.05);
            let _ = idx.align(at, 0.03, &[]);
            t.elapsed().as_secs_f64() * 1e3
        })
        .collect();
    per.sort_by(f64::total_cmp);
    let median = per[100];
    println!("find+align: Median {median:.4} ms");
    assert!(median < 1.0, "Median {median:.4} ms");
}
