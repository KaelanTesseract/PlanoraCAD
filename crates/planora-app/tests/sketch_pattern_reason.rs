//! Reste der Härtungen nach Skizzen 5 (2026-11-22): Geht das Gitter eines Musters erst beim
//! Lösen nicht mehr, nennen der abgelehnte Zug und der Status den Grund.

use planora_app::sketch_state::{
    Severity, SketchSession, apply_unsolved_message, drag_blocked_message,
};
use planora_app::state::{AppState, StandardPlane};
use planora_feature::SketchPlane;
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
        let text = drag_blocked_message(Some(&report), None).text();
        assert_eq!(
            text,
            "Das lässt sich nicht dorthin ziehen: Die Richtungslinien des Musters sind parallel."
        );
        // Beim Greifen noch gut: der Grund.
        let mut good = report.clone();
        good.status = SolveStatus::Solved;
        good.pattern_problem = None;
        assert!(
            drag_blocked_message(Some(&report), Some(&good))
                .text()
                .contains("parallel")
        );
        // Ohne Musterproblem bleibt die allgemeine Meldung.
        let general = "Das lässt sich wegen der Abhängigkeiten nicht dorthin ziehen";
        assert_eq!(drag_blocked_message(None, None).text(), general);
        // War das Gitter schon beim Greifen kaputt, ist es nicht der Grund der Ablehnung.
        assert_eq!(
            drag_blocked_message(Some(&report), Some(&report)).text(),
            general
        );
        // Ein Widerspruch geht vor: nicht vom Mustergrund verdecken.
        let mut conflict = report.clone();
        conflict.status = SolveStatus::Conflicting;
        assert_eq!(drag_blocked_message(Some(&conflict), None).text(), general);
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

/// Prüferfund W1: Auch der Zug am Endpunkt der Richtungslinie (Punktzug an der Sitzung) nennt
/// den Grund, wenn er die Linie parallel legt.
#[test]
fn a_point_drag_that_lays_the_direction_parallel_names_the_reason() {
    with_lang(&german(), || {
        let mut s = AppState::new();
        let i = s
            .add_sketch(SketchPlane::Standard(StandardPlane::XY))
            .unwrap();
        s.open_sketch(i).unwrap();
        let b = {
            let sk = s.session_sketch_mut().unwrap().1;
            let r = sk
                .add_rectangle(DVec2::new(0.0, 0.0), DVec2::new(10.0, 6.0))
                .unwrap();
            let a = sk.add_point(0.0, 30.0).unwrap();
            let b = sk.add_point(5.0, 40.0).unwrap();
            let dir = sk.add_line(a, b).unwrap();
            sk.add_pattern(&PatternPlan {
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
            assert_eq!(sk.solve().status, SolveStatus::Solved);
            b
        };
        s.observe_sketch(false);
        let (se, sk) = s.session_sketch_mut().unwrap();
        let grab = sk.point_xy(b).unwrap();
        assert!(se.begin_drag(sk, grab, 0.5), "Endpunkt gegriffen");
        se.drag_to(sk, DVec2::new(20.0, 30.0));
        let text = se.message_text().unwrap_or_default();
        assert!(
            text.contains("Die Richtungslinien des Musters sind parallel."),
            "{text:?}"
        );
    });
}

/// Handtest Punkt 8: Macht eine neue Abhängigkeit (etwa „Waagerecht“ an der Richtungslinie) das
/// Gitter ungültig, nennt die Ablehnung den Grund.
#[test]
fn an_applied_constraint_names_the_pattern_reason() {
    with_lang(&german(), || {
        let report = parallel_report();
        let mut good = report.clone();
        good.status = SolveStatus::Solved;
        good.pattern_problem = None;
        assert_eq!(
            apply_unsolved_message("Waagerecht", &report, Some(&good)).text(),
            "„Waagerecht“ ließ sich nicht lösen: Die Richtungslinien des Musters sind parallel."
        );
        // War das Gitter schon vorher kaputt: allgemein.
        assert_eq!(
            apply_unsolved_message("Waagerecht", &report, Some(&report)).text(),
            "„Waagerecht“ ließ sich nicht lösen"
        );
    });
}
