//! Messtest: Vorschau beim Stutzen (`trim_piece`) unter 16 ms im Release-Build.
//! `cargo test --release -p planora-sketch --test trim_perf -- --ignored`

use planora_math::{DVec2, Frame, Tolerance};
use planora_sketch::model::{CurveId, Sketch};
use std::time::Instant;

/// Dieselbe Skizze wie in `profile_perf.rs`: 10×10-Gitter aus 20 Linien, 100 Kreise
/// (r = 0,4) an den Gitterpunkten, 80 Dreiviertelbögen (r = 0,35) in den Zellen.
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
fn trim_hover_under_16_ms() {
    let s = grid_sketch();
    assert_eq!(s.curves().count(), 200);
    let curves: Vec<CurveId> = s.curves().map(|(c, _)| c).collect();
    let mut worst: f64 = 0.0;
    for k in 0..50 {
        // Zeigerstellen über die Skizze verteilt, Kurve wie beim Überfahren: die nächste
        let at = DVec2::new(
            (k * 7 % 50) as f64 * 0.2 - 0.3,
            (k * 13 % 50) as f64 * 0.2 - 0.2,
        );
        let c = s.pick_curve(at, 1.0).unwrap_or(curves[k % curves.len()]);
        let t = Instant::now();
        let piece = s.trim_piece(c, at).expect("Stück");
        let dt = t.elapsed().as_secs_f64() * 1e3;
        assert!(piece.polyline.len() >= 2);
        worst = worst.max(dt);
    }
    println!("trim_piece: längste {worst:.2} ms");
    assert!(worst < 16.0, "längste {worst:.2} ms");
}
