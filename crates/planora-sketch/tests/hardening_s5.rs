//! Härtungen nach Skizzen 5 (Skizzen 12, Task 13): Schleifen über zwei Muster, große Indizes,
//! Glatt mit veraltetem Ende beim Laden, eigene Meldung für parallele Richtungen und die
//! Löser-Kosten von Vorschau und `add_constraint`.

use std::time::Instant;

use planora_math::{DVec2, Frame, Tolerance};
use planora_sketch::constraint::{ConstraintKind as K, LineOrPoints};
use planora_sketch::model::{CurveId, PointId, Sketch};
use planora_sketch::pattern::{PatternDir, PatternId, PatternKind, PatternPlan, PatternProblem};
use planora_sketch::solver::SolveStatus;
use planora_sketch::{ConstraintId, ConstraintRecord};

fn v(x: f64, y: f64) -> DVec2 {
    DVec2::new(x, y)
}

fn sketch() -> Sketch {
    Sketch::new(Frame::WORLD, "S", Tolerance::default())
}

fn plan(sources: Vec<CurveId>, counts: [u32; 2], values: [f64; 2]) -> PatternPlan {
    PatternPlan {
        kind: PatternKind::Rect {
            dirs: [PatternDir::SketchX, PatternDir::SketchY],
            extent: [false, false],
            symmetric: [false, false],
        },
        sources,
        counts,
        values,
        suppressed: vec![],
    }
}

/// Punkt des Exemplars an `at`, der zur Vorlagenecke `corner` gehört.
fn instance_point(s: &Sketch, id: PatternId, corner: PointId, at: [i32; 2]) -> PointId {
    let d = s.pattern(id).unwrap();
    let k = d.source_points.iter().position(|&x| x == corner).unwrap();
    d.instances.iter().find(|i| i.at == at).unwrap().points[k]
}

fn dist(s: &Sketch, a: PointId, b: PointId) -> f64 {
    s.point_xy(a).unwrap().distance(s.point_xy(b).unwrap())
}

struct Loop {
    s: Sketch,
    a: PatternId,
    b: PatternId,
    /// Abstand Vorlage B → Exemplar von A: schlösse die Schleife A → B → A.
    closing: K,
    /// Die erste Hälfte der Schleife (Vorlage A → Exemplar von B).
    first: ConstraintId,
}

/// Muster A (Rechteck im Ursprung) und B (Rechteck oben), je 3×1. Ein Abstand bindet A's
/// Vorlagenecke an ein Exemplar von B (A liest B). `closing` bände B's Vorlagenecke an ein
/// Exemplar von A (B liest A): Dann folgte A über B sich selbst.
fn two_patterns() -> Loop {
    let mut s = sketch();
    let ra = s.add_rectangle(v(0.0, 0.0), v(10.0, 6.0)).unwrap();
    let a = s
        .add_pattern(&plan(ra.lines.to_vec(), [3, 1], [20.0, 10.0]))
        .unwrap();
    let rb = s.add_rectangle(v(0.0, 50.0), v(4.0, 54.0)).unwrap();
    let b = s
        .add_pattern(&plan(rb.lines.to_vec(), [3, 1], [10.0, 10.0]))
        .unwrap();
    let (sa, sb) = (ra.points[0], rb.points[0]);
    let bi = instance_point(&s, b, sb, [2, 0]);
    let ai = instance_point(&s, a, sa, [1, 0]);
    let d1 = dist(&s, sa, bi);
    let first = s.add_constraint(K::Distance(sa, bi, d1)).unwrap();
    assert_eq!(s.solve().status, SolveStatus::Solved);
    let closing = K::Distance(sb, ai, dist(&s, sb, ai));
    Loop {
        s,
        a,
        b,
        closing,
        first,
    }
}

#[test]
fn loop_across_two_patterns_is_refused() {
    let mut t = two_patterns();
    let before = t.s.to_data();
    let e = t.s.add_constraint(t.closing.clone()).unwrap_err();
    assert!(e.to_string().contains("eigenen Exemplar"), "{e}");
    assert_eq!(t.s.to_data(), before, "unverändert");
    // Dasselbe über Aus- und wieder Einschalten der ersten Hälfte: Ist sie aus, geht die zweite;
    // das Einschalten schlösse dann die Schleife.
    t.s.set_enabled(t.first, false).unwrap();
    let second = t.s.add_constraint(t.closing.clone()).unwrap();
    let before = t.s.to_data();
    let e = t.s.set_enabled(t.first, true).unwrap_err();
    assert!(e.to_string().contains("eigenen Exemplar"), "{e}");
    assert_eq!(t.s.to_data(), before, "unverändert");
    assert!(t.s.constraint(second).is_some());
    assert_eq!(t.s.solve().status, SolveStatus::Solved);
}

#[test]
fn loop_across_two_patterns_on_load_dissolves() {
    let t = two_patterns();
    let mut data = t.s.to_data();
    // Von Hand geänderte Datei: die zweite Hälfte steht schon drin.
    let template: ConstraintRecord = data.constraints[t.first.index()].clone();
    data.constraints.push(ConstraintRecord {
        kind: t.closing.clone(),
        ..template
    });
    let (mut s, dissolved) = Sketch::from_data_counted(data).unwrap();
    assert_eq!(dissolved, 2, "beide Muster der Schleife aufgelöst");
    assert!(s.pattern(t.a).is_none());
    assert!(s.pattern(t.b).is_none());
    assert_eq!(s.solve().status, SolveStatus::Solved);
}

#[test]
fn huge_index_is_refused() {
    let big = u32::MAX as usize + 1;
    assert_eq!(PointId::try_from_index(big), None);
    assert_eq!(CurveId::try_from_index(big + 7), None);
    assert_eq!(PatternId::try_from_index(usize::MAX), None);
    assert_eq!(
        PointId::try_from_index(7).map(|p| p.index()),
        Some(7),
        "kleine Indizes gehen"
    );
    assert_eq!(
        PointId::try_from_index(u32::MAX as usize).map(|p| p.index()),
        Some(u32::MAX as usize)
    );
    // `from_index` schneidet nicht mehr still ab: Eine zu große Zahl trifft kein vorhandenes
    // Objekt (früher wurde aus 2³² + 3 die 3).
    let mut s = sketch();
    for i in 0..5 {
        s.add_point(f64::from(i), 0.0).unwrap();
    }
    let p = PointId::from_index(big + 3);
    assert_ne!(p.index(), 3);
    assert!(s.point_xy(p).is_err());
}

#[test]
fn stale_smooth_end_is_corrupt() {
    let mut s = sketch();
    let (a, joint) = (
        s.add_point(-10.0, 0.0).unwrap(),
        s.add_point(0.0, 0.0).unwrap(),
    );
    let line = s.add_line(a, joint).unwrap();
    let mid = s.add_point(5.0, 3.0).unwrap();
    let end = s.add_point(10.0, 0.0).unwrap();
    let sp = s.add_spline(&[joint, mid, end]).unwrap();
    let smooth = s.add_constraint(K::Smooth(line, joint, sp, joint)).unwrap();
    assert_eq!(s.solve().status, SolveStatus::Solved);
    let good = s.to_data();
    assert!(Sketch::from_data(good.clone()).is_ok());
    // Von Hand geändert: das Glatt sitzt am mittleren Stützpunkt, der kein Ende ist.
    let mut bad = good.clone();
    bad.constraints[smooth.index()].kind = K::Smooth(line, joint, sp, mid);
    let e = Sketch::from_data(bad.clone()).unwrap_err();
    assert!(e.to_string().contains("kein Ende"), "{e}");
    // Auch ausgeschaltet oder anzeigend: `settle_constraints` hätte es entfernt.
    bad.constraints[smooth.index()].enabled = false;
    assert!(Sketch::from_data(bad.clone()).is_err());
    // Gelöscht darf es so bleiben (Stutzen lässt das zurück).
    bad.constraints[smooth.index()].alive = false;
    assert!(Sketch::from_data(bad).is_ok());
}

#[test]
fn parallel_grid_reports_reason() {
    let mut s = sketch();
    let r = s.add_rectangle(v(0.0, 0.0), v(10.0, 6.0)).unwrap();
    let a = s.add_point(0.0, 30.0).unwrap();
    let b = s.add_point(20.0, 30.0).unwrap();
    let dir = s.add_line(a, b).unwrap();
    let mut p = plan(r.lines.to_vec(), [2, 2], [15.0, 10.0]);
    p.kind = PatternKind::Rect {
        dirs: [PatternDir::SketchX, PatternDir::Line(dir)],
        extent: [false, false],
        symmetric: [false, false],
    };
    assert_eq!(
        s.pattern_transforms(&p),
        Err(PatternProblem::ParallelDirections)
    );
    let e = s.add_pattern(&p).unwrap_err();
    assert!(
        e.to_string()
            .ends_with("Die Richtungslinien des Musters sind parallel."),
        "{e}"
    );
    let e = s.pattern_preview(&p, 64).unwrap_err();
    assert!(e.to_string().contains("parallel"), "{e}");
    // Eine fehlende Richtungslinie meldet weiter den allgemeinen Grund.
    p.kind = PatternKind::Rect {
        dirs: [PatternDir::Perpendicular, PatternDir::SketchY],
        extent: [false, false],
        symmetric: [false, false],
    };
    assert_eq!(s.pattern_transforms(&p), Err(PatternProblem::BadDirection));
}

#[test]
fn preview_does_not_clone_unchanged_sketch() {
    let mut s = sketch();
    let r = s.add_rectangle(v(0.0, 0.0), v(10.0, 6.0)).unwrap();
    let a = s.add_point(0.0, 30.0).unwrap();
    let e = s.add_point(5.0, 34.0).unwrap();
    let m = s.add_point(0.0, 34.0).unwrap();
    let arc = s.add_arc(m, a, e).unwrap();
    let mut sources = r.lines.to_vec();
    sources.push(arc);
    let p = plan(sources.clone(), [3, 2], [20.0, 50.0]);
    let before = s.to_data();
    assert_eq!(s.preview_clones(), 0);
    let first = s.pattern_preview(&p, 64).unwrap();
    assert_eq!(s.preview_clones(), 1);
    // Unverändert: dieselbe Arbeitskopie, dasselbe Ergebnis, auch mit anderem Plan.
    for _ in 0..5 {
        assert_eq!(s.pattern_preview(&p, 64).unwrap(), first);
    }
    let other = s
        .pattern_preview(&plan(sources.clone(), [2, 1], [30.0, 10.0]), 64)
        .unwrap();
    assert_eq!(other.len(), 1);
    assert_eq!(s.pattern_preview(&p, 64).unwrap(), first);
    assert_eq!(s.preview_clones(), 1);
    assert_eq!(s.to_data(), before, "die Vorschau ändert die Skizze nicht");
    // Ein Fehler in der Vorschau lässt die Arbeitskopie brauchbar.
    let far = plan(sources.clone(), [3, 1], [1e9, 10.0]);
    assert!(s.pattern_preview(&far, 64).is_err());
    assert_eq!(s.pattern_preview(&p, 64).unwrap(), first);
    assert_eq!(s.preview_clones(), 1);
    // Nach einer Änderung neu kopiert, und das Ergebnis folgt.
    s.set_point_xy(r.points[0], -1.0, 0.0).unwrap();
    let moved = s.pattern_preview(&p, 64).unwrap();
    assert_eq!(s.preview_clones(), 2);
    assert_ne!(moved, first);
    let fresh = s.clone();
    assert_eq!(
        fresh.preview_clones(),
        0,
        "eine Kopie hat ihre eigene Arbeitskopie"
    );
    assert_eq!(fresh.pattern_preview(&p, 64).unwrap(), moved);
}

/// Zeit für `n` Abhängigkeiten an einer Skizze mit Muster.
fn add_constraints_ms(n: usize) -> f64 {
    let mut s = sketch();
    let r = s.add_rectangle(v(0.0, 0.0), v(10.0, 6.0)).unwrap();
    s.add_pattern(&plan(r.lines.to_vec(), [3, 1], [20.0, 10.0]))
        .unwrap();
    let mut lines = Vec::with_capacity(n);
    for i in 0..n {
        let y = 100.0 + i as f64;
        let a = s.add_point(0.0, y).unwrap();
        let b = s.add_point(5.0, y + 1.0).unwrap();
        lines.push(s.add_line(a, b).unwrap());
    }
    let t = Instant::now();
    for &l in &lines {
        s.add_constraint(K::Horizontal(LineOrPoints::Line(l)))
            .unwrap();
    }
    t.elapsed().as_secs_f64() * 1000.0
}

#[test]
#[ignore = "Zeitmessung; im Release laufen lassen: cargo test --release -- --ignored"]
fn add_constraint_groups_scale_linearly() {
    let best = |n: usize| {
        (0..3)
            .map(|_| add_constraints_ms(n))
            .fold(f64::MAX, f64::min)
    };
    let small = best(250);
    let large = best(1000);
    let ratio = large / small.max(1e-3);
    println!("250: {small:.2} ms, 1000: {large:.2} ms, Verhältnis {ratio:.1}");
    // Linear: etwa 4, quadratisch: etwa 16.
    assert!(ratio < 8.0, "nicht linear: Verhältnis {ratio:.1}");
}

/// Reste der Härtungen (2026-11-22): Wird das Gitter erst beim Lösen ungültig (die Richtungslinie
/// liegt danach parallel), nennt der Bericht den Grund.
#[test]
fn grid_failing_while_solving_names_the_reason() {
    let mut s = sketch();
    let r = s.add_rectangle(v(0.0, 0.0), v(10.0, 6.0)).unwrap();
    let a = s.add_point(0.0, 30.0).unwrap();
    let b = s.add_point(5.0, 40.0).unwrap();
    let dir = s.add_line(a, b).unwrap();
    let mut p = plan(r.lines.to_vec(), [2, 2], [15.0, 10.0]);
    p.kind = PatternKind::Rect {
        dirs: [PatternDir::SketchX, PatternDir::Line(dir)],
        extent: [false, false],
        symmetric: [false, false],
    };
    s.add_pattern(&p).unwrap();
    let ok = s.solve();
    assert_eq!(ok.status, SolveStatus::Solved);
    assert_eq!(ok.pattern_problem, None);
    // Die Richtungslinie waagerecht: parallel zur Skizzen-X-Richtung.
    s.set_point_xy(b, 20.0, 30.0).unwrap();
    let bad = s.solve();
    assert_eq!(bad.status, SolveStatus::NotConverged);
    assert_eq!(
        bad.pattern_problem,
        Some(PatternProblem::ParallelDirections)
    );
    // Wieder schräg: kein Grund mehr.
    s.set_point_xy(b, 5.0, 40.0).unwrap();
    let again = s.solve();
    assert_eq!(again.status, SolveStatus::Solved);
    assert_eq!(again.pattern_problem, None);
}
