//! Schleifen zwischen Vorlage und eigenem Exemplar über Befehle an einem anderen Muster
//! (Skizzen 5, Task 5, Korrektur 3): Fallen Exemplare von P weg oder wird P aufgelöst, bekommen
//! fremde Kurven freie Punkte, und das kann Q in eine Schleife bringen. Ändern, Anzahl und Abwahl
//! lehnen das ab; Auflösen löst Q mit auf.

use planora_math::{DVec2, Frame, Tolerance};
use planora_sketch::constraint::{ConstraintId, ConstraintKind as K};
use planora_sketch::model::{CurveId, PointId, Sketch};
use planora_sketch::pattern::{PatternDir, PatternId, PatternKind, PatternPlan, PatternSlot};
use planora_sketch::solver::SolveStatus;

fn v(x: f64, y: f64) -> DVec2 {
    DVec2::new(x, y)
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

fn value_of(s: &Sketch, id: PatternId, slot: PatternSlot) -> ConstraintId {
    *s.pattern(id)
        .unwrap()
        .values
        .iter()
        .find(|&&c| matches!(s.constraint(c).unwrap().kind, K::PatternValue(_, x, _) if x == slot))
        .unwrap()
}

struct Two {
    s: Sketch,
    q: PatternId,
    p: PatternId,
    p_lines: Vec<CurveId>,
}

/// Q: Rechteck im Ursprung, 3×1 mit Abstand 20. P: kleines Rechteck oben, 3×1 mit Abstand 10.
/// Bogen F2: Mitte auf P [2, 0], Enden auf Q [1, 0] und Q [2, 0] (liest nur Gehaltenes). Bogen F1:
/// Mitte auf Q's Vorlagenecke, Anfang auf P [2, 0], Ende frei. Ohne Schleife, solange P [2, 0]
/// lebt; fällt es weg, verbindet sein freier Ersatzpunkt Q's Vorlage mit Q's Exemplaren.
fn p7() -> Two {
    let mut s = Sketch::new(Frame::WORLD, "S", Tolerance::default());
    let rq = s.add_rectangle(v(0.0, 0.0), v(10.0, 6.0)).unwrap();
    let q = s
        .add_pattern(&plan(rq.lines.to_vec(), [3, 1], [20.0, 10.0]))
        .unwrap();
    let src = rq.points[0];
    let (qi1, qi2) = (
        instance_point(&s, q, src, [1, 0]),
        instance_point(&s, q, src, [2, 0]),
    );
    let rp = s.add_rectangle(v(10.0, 50.0), v(14.0, 54.0)).unwrap();
    let p = s
        .add_pattern(&plan(rp.lines.to_vec(), [3, 1], [10.0, 10.0]))
        .unwrap();
    let vpt = instance_point(&s, p, rp.points[0], [2, 0]);
    s.add_arc(vpt, qi1, qi2).unwrap();
    let e = s.add_point(-30.0, 50.0).unwrap();
    s.add_arc(src, vpt, e).unwrap();
    s.solve();
    Two {
        s,
        q,
        p,
        p_lines: rp.lines.to_vec(),
    }
}

#[test]
fn an_edit_of_p_that_would_put_q_into_a_loop_is_refused() {
    let mut t = p7();
    let before = t.s.to_data();
    let e =
        t.s.edit_pattern(t.p, &plan(t.p_lines.clone(), [2, 1], [10.0, 10.0]))
            .unwrap_err();
    assert!(e.to_string().contains("eigenen Exemplar"), "{e}");
    assert_eq!(t.s.to_data(), before, "unverändert");
    // Dieselbe Änderung über den Wert der Anzahl.
    let count = value_of(&t.s, t.p, PatternSlot::Count1);
    let e = t.s.set_constraint_value(count, 2.0).unwrap_err();
    assert!(e.to_string().contains("eigenen Exemplar"), "{e}");
    assert_eq!(t.s.to_data(), before, "unverändert");
    // Abwählen der Lage [2, 0] ebenso.
    let e = t.s.suppress_instance(t.p, [2, 0], true).unwrap_err();
    assert!(e.to_string().contains("eigenen Exemplar"), "{e}");
    assert_eq!(t.s.to_data(), before, "unverändert");
    // Q bleibt bedienbar.
    let srcs = t.s.pattern(t.q).unwrap().sources.clone();
    t.s.edit_pattern(t.q, &plan(srcs, [3, 1], [25.0, 10.0]))
        .unwrap();
    assert_eq!(t.s.solve().status, SolveStatus::Solved);
}

#[test]
fn edits_without_a_new_loop_still_work() {
    let mut t = p7();
    // Anzahl 3 → 4 nimmt nichts weg.
    t.s.edit_pattern(t.p, &plan(t.p_lines.clone(), [4, 1], [10.0, 10.0]))
        .unwrap();
    let count = value_of(&t.s, t.p, PatternSlot::Count1);
    t.s.set_constraint_value(count, 5.0).unwrap();
    // Eine Lage abwählen, an der nichts hängt.
    t.s.suppress_instance(t.p, [1, 0], true).unwrap();
    assert_eq!(t.s.pattern(t.p).unwrap().instances.len(), 3);
    assert_eq!(t.s.solve().status, SolveStatus::Solved);
}

#[test]
fn dissolving_p_dissolves_q_when_q_would_be_in_a_loop() {
    let mut s = Sketch::new(Frame::WORLD, "S", Tolerance::default());
    let rq = s.add_rectangle(v(0.0, 0.0), v(10.0, 6.0)).unwrap();
    let q = s
        .add_pattern(&plan(rq.lines.to_vec(), [3, 1], [20.0, 10.0]))
        .unwrap();
    let src = rq.points[0];
    let qi1 = instance_point(&s, q, src, [1, 0]);
    let rp = s.add_rectangle(v(10.0, 50.0), v(14.0, 54.0)).unwrap();
    let p = s
        .add_pattern(&plan(rp.lines.to_vec(), [3, 1], [10.0, 10.0]))
        .unwrap();
    let vpt = instance_point(&s, p, rp.points[0], [1, 0]);
    let d0 = s.point_xy(vpt).unwrap().distance(s.point_xy(src).unwrap());
    s.add_constraint(K::Distance(src, vpt, d0)).unwrap();
    let d1 = s.point_xy(vpt).unwrap().distance(s.point_xy(qi1).unwrap());
    // Nur Exemplare: `add_constraint` legt es anzeigend an (Schlusskorrektur I1). Treibend nur
    // aus einer alten Datei; die lädt weiter.
    let between = s.add_constraint(K::Distance(vpt, qi1, d1)).unwrap();
    assert!(s.constraint(between).unwrap().driven);
    let mut data = s.to_data();
    data.constraints[between.index()].driven = false;
    let mut s = Sketch::from_data(data).unwrap();
    assert_eq!(s.solve().status, SolveStatus::Solved);
    let positions: Vec<DVec2> = s.points().map(|(id, _)| s.point_xy(id).unwrap()).collect();

    s.dissolve_pattern(p).unwrap();
    assert!(s.pattern(p).is_none());
    assert!(s.pattern(q).is_none(), "Q mit aufgelöst");
    let r = s.solve();
    assert_eq!(r.status, SolveStatus::Solved, "{r:?}");
    let after: Vec<DVec2> = s.points().map(|(id, _)| s.point_xy(id).unwrap()).collect();
    assert_eq!(after.len(), positions.len(), "Geometrie bleibt");
    for (a, b) in after.iter().zip(&positions) {
        assert!(a.distance(*b) < 1e-9, "{a:?} {b:?}");
    }
    let (a, b) = (
        s.add_point(-500.0, 0.0).unwrap(),
        s.add_point(-490.0, 0.0).unwrap(),
    );
    s.add_constraint(K::Distance(a, b, 10.0)).unwrap();
    assert_eq!(s.solve().status, SolveStatus::Solved);
}
