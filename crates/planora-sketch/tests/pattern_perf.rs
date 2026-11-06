//! Messtest der Muster (Skizzen 5, Task 5) im Release-Build:
//! `cargo test --release -p planora-sketch --test pattern_perf -- --ignored --nocapture`

use planora_math::{DVec2, Frame, Tolerance};
use planora_sketch::constraint::{ConstraintId, ConstraintKind as K};
use planora_sketch::model::Sketch;
use planora_sketch::pattern::{PatternDir, PatternId, PatternKind, PatternPlan, PatternSlot};
use planora_sketch::solver::SolveStatus;
use std::time::Instant;

/// Rechteck 10×6 mit fester Ecke und Maßen für Breite und Höhe als Vorlage eines Rechteckmusters
/// `n`×`n` mit Abstand 15. Liefert das Breitenmaß und den ersten Abstand.
fn pattern(n: u32) -> (Sketch, PatternId, ConstraintId, ConstraintId) {
    let mut s = Sketch::new(Frame::WORLD, "S", Tolerance::default());
    let r = s
        .add_rectangle(DVec2::new(0.0, 0.0), DVec2::new(10.0, 6.0))
        .unwrap();
    let [p0, p1, _, p3] = r.points;
    s.add_constraint(K::Fixed(p0, 0.0, 0.0)).unwrap();
    let width = s.add_constraint(K::Distance(p0, p1, 10.0)).unwrap();
    s.add_constraint(K::Distance(p0, p3, 6.0)).unwrap();
    let id = s
        .add_pattern(&PatternPlan {
            kind: PatternKind::Rect {
                dirs: [PatternDir::SketchX, PatternDir::SketchY],
                extent: [false, false],
                symmetric: [false, false],
            },
            sources: r.lines.to_vec(),
            counts: [n, n],
            values: [15.0, 15.0],
            suppressed: Vec::new(),
        })
        .unwrap();
    assert_eq!(s.pattern(id).unwrap().instances.len(), (n * n - 1) as usize);
    let spacing = *s
        .pattern(id)
        .unwrap()
        .values
        .iter()
        .find(|&&c| {
            matches!(s.constraint(c).unwrap().kind,
                K::PatternValue(_, slot, _) if slot == PatternSlot::Spacing1)
        })
        .unwrap();
    (s, id, width, spacing)
}

/// Median in ms über 20 Läufe: Wert `values(k)` setzen, lösen, mit `faces` auch die Flächen
/// suchen (so viele müssen es sein).
fn median_ms(
    s: &mut Sketch,
    c: ConstraintId,
    values: impl Fn(usize) -> f64,
    faces: Option<usize>,
) -> f64 {
    let mut times = Vec::new();
    for k in 0..20 {
        let t = Instant::now();
        s.set_constraint_value(c, values(k)).unwrap();
        let r = s.solve();
        let found = faces.map(|_| s.profiles().profiles.len());
        times.push(t.elapsed().as_secs_f64() * 1e3);
        assert_eq!(r.status, SolveStatus::Solved, "{r:?}");
        assert_eq!(found, faces);
    }
    times.sort_by(f64::total_cmp);
    times[times.len() / 2]
}

#[test]
#[ignore = "Messtest, nur im Release-Build sinnvoll"]
fn changing_a_10x10_pattern_with_profiles_and_a_30x30_pattern_stays_under_100_ms() {
    let spacing = |k: usize| 15.0 + 0.1 * (k % 2) as f64 + 0.01 * k as f64;
    let width = |k: usize| 10.0 + 0.1 * (k % 2) as f64 + 0.01 * k as f64;

    let (mut s, _, w, a) = pattern(10);
    let ms = median_ms(&mut s, a, spacing, Some(100));
    println!("10×10 Abstand ändern mit profiles(): Median {ms:.2} ms");
    assert!(ms < 100.0, "10×10 Abstand: {ms:.2} ms");
    let ms = median_ms(&mut s, w, width, Some(100));
    println!("10×10 Vorlagenmaß ändern mit profiles(): Median {ms:.2} ms");
    assert!(ms < 100.0, "10×10 Vorlagenmaß: {ms:.2} ms");

    let (mut s, _, w, a) = pattern(30);
    let ms = median_ms(&mut s, a, spacing, None);
    println!("30×30 Abstand ändern: Median {ms:.2} ms");
    assert!(ms < 100.0, "30×30 Abstand: {ms:.2} ms");
    let ms = median_ms(&mut s, w, width, None);
    println!("30×30 Vorlagenmaß ändern: Median {ms:.2} ms");
    assert!(ms < 100.0, "30×30 Vorlagenmaß: {ms:.2} ms");
}
