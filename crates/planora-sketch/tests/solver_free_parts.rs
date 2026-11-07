//! Handtest Skizzen 5, Korrektur 5: Freie, unverbundene Teile rutschen beim Lösen nicht.
//!
//! Ein 3-Punkt-Bogen ohne Abhängigkeiten und Maße bleibt bitgenau stehen, wenn anderswo in der
//! Skizze ein Maß geändert oder ein Glatt gesetzt wird; ein frisch gezeichneter Bogen ist
//! konsistent, und ein zweites Lösen ändert nichts.

use planora_math::{DVec2, Frame, Tolerance};
use planora_sketch::constraint::ConstraintKind as K;
use planora_sketch::model::{CurveId, PointId, Sketch};
use planora_sketch::pattern::{PatternDir, PatternKind, PatternPlan, PatternSlot};
use planora_sketch::shapes::arc_through;
use planora_sketch::solver::{Drag, SolveStatus};

const TOL: f64 = 1e-6;

fn sketch() -> Sketch {
    Sketch::new(Frame::WORLD, "S", Tolerance::default())
}

fn v(x: f64, y: f64) -> DVec2 {
    DVec2::new(x, y)
}

fn pt(s: &mut Sketch, p: DVec2) -> PointId {
    s.add_point(p.x, p.y).unwrap()
}

/// 3-Punkt-Bogen wie im Werkzeug: Start, Ende und ein Punkt darauf; die Mitte rechnet
/// [`arc_through`].
fn free_arc(s: &mut Sketch, start: DVec2, end: DVec2, on: DVec2) -> (CurveId, [PointId; 3]) {
    let plan = arc_through(start, end, on, TOL).unwrap();
    let c = pt(s, plan.center);
    let a = pt(s, plan.start);
    let b = pt(s, plan.end);
    (s.add_arc(c, a, b).unwrap(), [c, a, b])
}

/// Bitmuster der Koordinaten der Punkte.
fn bits(s: &Sketch, points: &[PointId]) -> Vec<u64> {
    points
        .iter()
        .flat_map(|&p| {
            let q = s.point_xy(p).unwrap();
            [q.x.to_bits(), q.y.to_bits()]
        })
        .collect()
}

fn solved(s: &mut Sketch) {
    let r = s.solve();
    assert_eq!(r.status, SolveStatus::Solved, "{r:?}");
}

/// (a) Freier Bogen und eine Linie mit Maß: Das Maß ändern lässt den Bogen bitgleich stehen.
#[test]
fn a_free_arc_stays_bit_exact_when_a_dimension_elsewhere_changes() {
    let mut s = sketch();
    // Ungerade Werte, damit die Mitte des Bogens nicht zufällig exakt ist.
    let (_, arc_pts) = free_arc(&mut s, v(3.1, 7.3), v(28.7, 11.9), v(17.2, 22.6));
    let a = pt(&mut s, v(-40.0, -5.0));
    let b = pt(&mut s, v(-10.3, -4.1));
    s.add_line(a, b).unwrap();
    s.add_constraint(K::Fixed(a, -40.0, -5.0)).unwrap();
    let dim = s.add_constraint(K::Distance(a, b, 30.0)).unwrap();
    solved(&mut s);
    let before = bits(&s, &arc_pts);
    for value in [45.0, 12.5, 80.0] {
        s.set_constraint_value(dim, value).unwrap();
        solved(&mut s);
        assert_eq!(bits(&s, &arc_pts), before, "Bogen rutscht bei Maß {value}");
    }
}

/// (b) Glatt zwischen Spline und Linie setzen: Der freie Bogen bleibt bitgleich.
#[test]
fn a_free_arc_stays_bit_exact_when_smooth_is_added_elsewhere() {
    let mut s = sketch();
    let (_, arc_pts) = free_arc(&mut s, v(50.3, 7.3), v(78.7, 11.9), v(67.2, 22.6));
    let a = pt(&mut s, v(-10.0, 0.0));
    let joint = pt(&mut s, v(0.0, 0.0));
    let l = s.add_line(a, joint).unwrap();
    let q1 = pt(&mut s, v(5.0, 3.0));
    let q2 = pt(&mut s, v(10.0, 0.0));
    let sp = s.add_spline(&[joint, q1, q2]).unwrap();
    s.add_constraint(K::Fixed(a, -10.0, 0.0)).unwrap();
    solved(&mut s);
    let before = bits(&s, &arc_pts);
    s.add_constraint(K::Smooth(l, joint, sp, joint)).unwrap();
    solved(&mut s);
    assert_eq!(bits(&s, &arc_pts), before, "Bogen rutscht beim Glatt");
}

/// Wie (a) über die anderen Wege des Lösers: Ziehen eines fremden Punktes, Halten
/// (`solve_holding`, löst mit hundertfach schärferer Schranke nach) und mit einem Muster in der
/// Skizze (Nachführen der Exemplare in Runden).
#[test]
fn a_free_arc_stays_bit_exact_under_drag_hold_and_patterns() {
    let mut s = sketch();
    let (_, arc_pts) = free_arc(&mut s, v(3.1, 7.3), v(28.7, 11.9), v(17.2, 22.6));
    let r = s.add_rectangle(v(-60.0, -40.0), v(-50.0, -34.0)).unwrap();
    let [p0, p1, _, p3] = r.points;
    s.add_constraint(K::Fixed(p0, -60.0, -40.0)).unwrap();
    let width = s.add_constraint(K::Distance(p0, p1, 10.0)).unwrap();
    s.add_constraint(K::Distance(p0, p3, 6.0)).unwrap();
    let pattern = s
        .add_pattern(&PatternPlan {
            kind: PatternKind::Rect {
                dirs: [PatternDir::SketchX, PatternDir::SketchY],
                extent: [false, false],
                symmetric: [false, false],
            },
            sources: r.lines.to_vec(),
            counts: [3, 2],
            values: [15.0, 15.0],
            suppressed: Vec::new(),
        })
        .unwrap();
    let a = pt(&mut s, v(-40.0, 30.0));
    let b = pt(&mut s, v(-10.3, 31.1));
    s.add_line(a, b).unwrap();
    s.add_constraint(K::Fixed(a, -40.0, 30.0)).unwrap();
    solved(&mut s);
    let before = bits(&s, &arc_pts);

    s.set_constraint_value(width, 14.0).unwrap();
    solved(&mut s);
    assert_eq!(
        bits(&s, &arc_pts),
        before,
        "Bogen rutscht beim Maß der Vorlage"
    );

    let spacing = *s
        .pattern(pattern)
        .unwrap()
        .values
        .iter()
        .find(|&&c| {
            matches!(s.constraint(c).unwrap().kind,
                K::PatternValue(_, slot, _) if slot == PatternSlot::Spacing1)
        })
        .unwrap();
    s.set_constraint_value(spacing, 22.0).unwrap();
    solved(&mut s);
    assert_eq!(
        bits(&s, &arc_pts),
        before,
        "Bogen rutscht beim Abstand des Musters"
    );

    let r = s
        .drag(&[Drag {
            point: b,
            target: v(-5.0, 40.0),
        }])
        .unwrap();
    assert_eq!(r.status, SolveStatus::Solved, "{r:?}");
    assert_eq!(bits(&s, &arc_pts), before, "Bogen rutscht beim Ziehen");

    assert!(s.solve_holding(&[a, b]));
    assert_eq!(bits(&s, &arc_pts), before, "Bogen rutscht beim Halten");
}

/// (c) Ein frisch gezeichneter 3-Punkt-Bogen ist konsistent: Das erste Lösen braucht keinen
/// Schritt, und ein zweites Lösen ändert nichts.
#[test]
fn a_fresh_three_point_arc_is_consistent_and_solving_twice_changes_nothing() {
    for (start, end, on) in [
        (v(3.1, 7.3), v(28.7, 11.9), v(17.2, 22.6)),
        (v(-120.4, 33.3), v(97.1, -58.8), v(12.0, 140.0)),
        (v(0.0, 0.0), v(0.7, 0.1), v(0.3, 0.4)),
    ] {
        let mut s = sketch();
        let (_, arc_pts) = free_arc(&mut s, start, end, on);
        let drawn = bits(&s, &arc_pts);
        let r = s.solve();
        assert_eq!(r.status, SolveStatus::Solved, "{r:?}");
        assert_eq!(
            r.iterations, 0,
            "frischer Bogen braucht keinen Schritt: {r:?}"
        );
        assert_eq!(bits(&s, &arc_pts), drawn, "erstes Lösen bewegt den Bogen");
        let r = s.solve();
        assert_eq!(r.iterations, 0, "{r:?}");
        assert_eq!(bits(&s, &arc_pts), drawn, "zweites Lösen bewegt den Bogen");
    }
}
