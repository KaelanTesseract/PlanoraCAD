//! Reste der Härtungen nach Skizzen 5 (2026-11-22): Geht das Gitter eines Musters erst beim
//! Lösen nicht mehr, nennen der abgelehnte Zug und der Status den Grund.

use planora_app::sketch_state::{Severity, SketchSession, drag_blocked_message};
use planora_i18n::{german, with_lang};
use planora_math::{DVec2, Frame, Tolerance};
use planora_sketch::model::Sketch;
use planora_sketch::pattern::{PatternDir, PatternKind, PatternPlan};
use planora_sketch::solver::SolveStatus;

/// Skizze mit Rechteckmuster, dessen zweite Richtung einer Linie folgt; die Linie liegt danach
/// waagerecht, also parallel zur ersten Richtung.
fn parallel_report() -> planora_sketch::solver::SolveReport {
    let mut s = Sketch::new(Frame::WORLD, "S", Tolerance::default());
    let r = s
        .add_rectangle(DVec2::new(0.0, 0.0), DVec2::new(10.0, 6.0))
        .unwrap();
    let a = s.add_point(0.0, 30.0).unwrap();
    let b = s.add_point(5.0, 40.0).unwrap();
    let dir = s.add_line(a, b).unwrap();
    s.add_pattern(&PatternPlan {
        kind: PatternKind::Rect {
            dirs: [PatternDir::SketchX, PatternDir::Line(dir)],
            extent: [false, false],
            symmetric: [false, false],
        },
        sources: r.lines.to_vec(),
        counts: [2, 2],
        values: [15.0, 10.0],
        suppressed: vec![],
    })
    .unwrap();
    s.set_point_xy(b, 20.0, 30.0).unwrap();
    let report = s.solve();
    assert_eq!(report.status, SolveStatus::NotConverged);
    report
}

#[test]
fn a_blocked_drag_names_the_pattern_reason() {
    with_lang(&german(), || {
        let report = parallel_report();
        let text = drag_blocked_message(Some(&report)).text();
        assert_eq!(
            text,
            "Das lässt sich nicht dorthin ziehen. Die Richtungslinien des Musters sind parallel."
        );
        // Ohne Musterproblem bleibt die allgemeine Meldung.
        assert_eq!(
            drag_blocked_message(None).text(),
            "Das lässt sich wegen der Abhängigkeiten nicht dorthin ziehen"
        );
    });
}

#[test]
fn the_status_names_the_pattern_reason() {
    with_lang(&german(), || {
        let report = parallel_report();
        let (text, severity) = SketchSession::status_of(&report);
        assert_eq!(
            text,
            "Nicht lösbar: Die Richtungslinien des Musters sind parallel."
        );
        assert_eq!(severity, Severity::Error);
    });
}
