//! Muster ändern, abwählen und auflösen (Skizzen 5, Task 4): Anzahl über den Wert, bleibende
//! Exemplare behalten ihre Ids, Abwahl, Auflösen beim Löschen und vor Stutzen, Brechen und Rundung,
//! Zusammenspiel mit Versatz, Spiegeln, Verschieben, Skalieren und Kopieren, Bearbeiten als ein
//! Aufruf und die Wahl eines Exemplars per Klick.

use planora_math::{DVec2, Frame, PI, Tolerance};
use planora_sketch::constraint::{ConstraintId, ConstraintKind as K};
use planora_sketch::fillet::{ChamferKind, CurvePick};
use planora_sketch::model::{CurveId, PointId, Sketch};
use planora_sketch::pattern::{
    CircularMode, PatternDir, PatternEdit, PatternId, PatternKind, PatternPlan, PatternSlot,
};
use planora_sketch::shapes::{folded_over, sweep_crossed_half_turn};
use planora_sketch::solver::{SolveReport, SolveStatus};
use planora_sketch::transform::{Similarity, TransformBlock};
use std::collections::HashMap;

fn sketch() -> Sketch {
    Sketch::new(Frame::WORLD, "S", Tolerance::default())
}

fn v(x: f64, y: f64) -> DVec2 {
    DVec2::new(x, y)
}

fn at(s: &Sketch, p: PointId) -> DVec2 {
    s.point_xy(p).unwrap()
}

fn close(a: DVec2, b: DVec2, tol: f64) -> bool {
    a.distance(b) <= tol
}

fn solved(s: &mut Sketch) -> SolveReport {
    let r = s.solve();
    assert_eq!(r.status, SolveStatus::Solved, "{r:?}");
    assert!(r.conflicting.is_empty(), "{r:?}");
    r
}

/// Rechteck 10×6 mit der Ecke unten links in `o`, ohne Maße (nur waagerecht/senkrecht).
fn rect(s: &mut Sketch, o: DVec2) -> Vec<CurveId> {
    s.add_rectangle(o, o + v(10.0, 6.0)).unwrap().lines.to_vec()
}

/// Rechteck 10×6 mit fester Ecke unten links und Maßen für Breite und Höhe: voll bestimmt.
fn dimensioned_rect(s: &mut Sketch) -> (Vec<CurveId>, [PointId; 4], ConstraintId) {
    let ids = s.add_rectangle(v(0.0, 0.0), v(10.0, 6.0)).unwrap();
    let [p0, p1, _, p3] = ids.points;
    s.add_constraint(K::Fixed(p0, 0.0, 0.0)).unwrap();
    let w = s.add_constraint(K::Distance(p0, p1, 10.0)).unwrap();
    s.add_constraint(K::Distance(p0, p3, 6.0)).unwrap();
    solved(s);
    (ids.lines.to_vec(), ids.points, w)
}

fn rect_plan(sources: Vec<CurveId>, counts: [u32; 2], values: [f64; 2]) -> PatternPlan {
    PatternPlan {
        kind: PatternKind::Rect {
            dirs: [PatternDir::SketchX, PatternDir::SketchY],
            extent: [false, false],
            symmetric: [false, false],
        },
        sources,
        counts,
        values,
        suppressed: Vec::new(),
    }
}

fn circ_plan(
    sources: Vec<CurveId>,
    center: PointId,
    mode: CircularMode,
    n: u32,
    angle: f64,
) -> PatternPlan {
    PatternPlan {
        kind: PatternKind::Circular { center, mode },
        sources,
        counts: [n, 1],
        values: [angle, 0.0],
        suppressed: Vec::new(),
    }
}

/// Der Wert des Musters im Feld `slot`.
fn value_id(s: &Sketch, id: PatternId, slot: PatternSlot) -> ConstraintId {
    *s.pattern(id)
        .unwrap()
        .values
        .iter()
        .find(|&&c| matches!(s.constraint(c).unwrap().kind, K::PatternValue(_, x, _) if x == slot))
        .unwrap()
}

fn value(s: &Sketch, id: PatternId, slot: PatternSlot) -> f64 {
    match s.constraint(value_id(s, id, slot)).unwrap().kind {
        K::PatternValue(_, _, v) => v,
        _ => unreachable!(),
    }
}

/// Der Plan eines angelegten Musters aus seinen Werten.
fn current_plan(s: &Sketch, id: PatternId) -> PatternPlan {
    let data = s.pattern(id).unwrap();
    let val = |slot| {
        data.values
            .iter()
            .find_map(|&c| match s.constraint(c).unwrap().kind {
                K::PatternValue(_, x, v) if x == slot => Some(v),
                _ => None,
            })
    };
    let (counts, values) = match data.kind {
        PatternKind::Rect { .. } => (
            [
                val(PatternSlot::Count1).unwrap() as u32,
                val(PatternSlot::Count2).unwrap() as u32,
            ],
            [
                val(PatternSlot::Spacing1).unwrap(),
                val(PatternSlot::Spacing2).unwrap(),
            ],
        ),
        PatternKind::Circular { .. } => (
            [val(PatternSlot::Count).unwrap() as u32, 1],
            [val(PatternSlot::Angle).unwrap_or(0.0), 0.0],
        ),
    };
    PatternPlan {
        kind: data.kind,
        sources: data.sources.clone(),
        counts,
        values,
        suppressed: data.suppressed.clone(),
    }
}

/// Jedes Exemplar ist die mit seiner Lage bewegte Vorlage, und es gibt genau die Lagen aus
/// `pattern_transforms`.
fn assert_instances_follow(s: &Sketch, id: PatternId, tol: f64) {
    let data = s.pattern(id).unwrap();
    let lattice = s.pattern_transforms(&current_plan(s, id)).unwrap();
    assert_eq!(lattice.len(), data.instances.len());
    for inst in &data.instances {
        let t = lattice.iter().find(|(i, _)| *i == inst.at).unwrap().1;
        for (k, &q) in inst.points.iter().enumerate() {
            let want = t.apply(at(s, data.source_points[k]));
            assert!(
                close(at(s, q), want, tol),
                "{:?} {:?} {want:?}",
                inst.at,
                at(s, q)
            );
        }
        for &c in &inst.curves {
            assert!(s.curve(c).is_some(), "Kurve des Exemplars lebt");
        }
    }
}

/// Lagen der Exemplare je Index: die Punkte.
fn positions(s: &Sketch, id: PatternId) -> HashMap<[i32; 2], Vec<DVec2>> {
    s.pattern(id)
        .unwrap()
        .instances
        .iter()
        .map(|i| (i.at, i.points.iter().map(|&p| at(s, p)).collect()))
        .collect()
}

fn user_constraints(s: &Sketch) -> usize {
    s.constraints().filter(|(_, d)| !d.internal).count()
}

fn pick(curve: CurveId, x: f64, y: f64) -> CurvePick {
    CurvePick { curve, at: v(x, y) }
}

// ---- Anzahl über den Wert --------------------------------------------------------------------

#[test]
fn count_changes_keep_remaining_instances_and_their_bindings() {
    let mut s = sketch();
    let (src, _, _) = dimensioned_rect(&mut s);
    let id = s
        .add_pattern(&rect_plan(src, [3, 1], [15.0, 10.0]))
        .unwrap();
    let inst = |s: &Sketch, i: i32| {
        s.pattern(id)
            .unwrap()
            .instances
            .iter()
            .find(|x| x.at == [i, 0])
            .cloned()
    };
    let one = inst(&s, 1).unwrap();
    let two = inst(&s, 2).unwrap();
    let f1 = s.add_point(20.0, -10.0).unwrap();
    let f2 = s.add_point(35.0, -10.0).unwrap();
    let d1 = s
        .add_constraint(K::Distance(f1, one.points[0], 10.0))
        .unwrap();
    let d2 = s
        .add_constraint(K::Distance(f2, two.points[0], 10.0))
        .unwrap();
    solved(&mut s);
    let count = value_id(&s, id, PatternSlot::Count1);

    // 3 → 5: neue Exemplare kommen dazu, die alten behalten ihre Ids.
    s.set_constraint_value(count, 5.0).unwrap();
    solved(&mut s);
    assert_eq!(s.pattern(id).unwrap().instances.len(), 4);
    assert_eq!(inst(&s, 1).unwrap(), one);
    assert_eq!(inst(&s, 2).unwrap(), two);
    assert_instances_follow(&s, id, 1e-9);
    assert!(s.constraint(d1).is_some() && s.constraint(d2).is_some());

    // Dasselbe Bearbeiten über `edit_pattern` meldet die Zahl der entfallenen Abhängigkeiten.
    let mut t = s.clone();
    let mut plan = current_plan(&t, id);
    plan.counts[0] = 2;
    let edit = t.edit_pattern(id, &plan).unwrap();
    assert_eq!(
        edit,
        PatternEdit {
            added: 0,
            removed: 3,
            dropped_constraints: 1
        }
    );

    // 5 → 2: (2,0) fällt weg und nimmt seinen Abstand mit; (1,0) bleibt samt Abstand.
    let before = user_constraints(&s);
    s.set_constraint_value(count, 2.0).unwrap();
    assert_eq!(user_constraints(&s), before - 1, "genau eine entfallen");
    assert!(s.constraint(d1).is_some());
    assert!(s.constraint(d2).is_none());
    assert!(two.curves.iter().all(|&c| s.curve(c).is_none()));
    assert!(two.points.iter().all(|&p| s.point_xy(p).is_err()));
    solved(&mut s);
    assert_eq!(inst(&s, 1).unwrap(), one);
    assert!(inst(&s, 2).is_none());
    assert_instances_follow(&s, id, 1e-9);
    assert_eq!(t.pattern(id).unwrap(), s.pattern(id).unwrap());

    // 2 → 3: (2,0) kommt an der richtigen Lage wieder.
    s.set_constraint_value(count, 3.0).unwrap();
    solved(&mut s);
    assert_eq!(inst(&s, 1).unwrap(), one);
    assert!(inst(&s, 2).is_some());
    assert_instances_follow(&s, id, 1e-9);
    assert!((at(&s, f1).distance(at(&s, one.points[0])) - 10.0).abs() < 1e-9);

    // Eine Anzahl, die die Obergrenze sprengt, wird abgelehnt; die Skizze bleibt.
    let rev = s.revision();
    let c2 = value_id(&s, id, PatternSlot::Count2);
    assert!(s.set_constraint_value(c2, 400.0).is_err());
    assert_eq!(s.revision(), rev);
}

// ---- Abwählen --------------------------------------------------------------------------------

#[test]
fn suppressing_removes_and_restores_an_instance() {
    let mut s = sketch();
    let (src, _, w) = dimensioned_rect(&mut s);
    let id = s
        .add_pattern(&rect_plan(src, [3, 2], [15.0, 10.0]))
        .unwrap();
    let old = s.pattern(id).unwrap().instances[0].clone();
    assert_eq!(old.at, [1, 0]);
    s.suppress_instance(id, [1, 0], true).unwrap();
    assert!(old.curves.iter().all(|&c| s.curve(c).is_none()));
    assert!(s.pattern(id).unwrap().suppressed.contains(&[1, 0]));
    assert!(
        s.pattern(id)
            .unwrap()
            .instances
            .iter()
            .all(|i| i.at != [1, 0])
    );
    solved(&mut s);
    assert_instances_follow(&s, id, 1e-9);
    // Zweimal abwählen ändert nichts.
    let rev = s.revision();
    s.suppress_instance(id, [1, 0], true).unwrap();
    assert_eq!(s.revision(), rev);

    // Die Vorlage ändert sich, solange (1,0) abgewählt ist; beim Anwählen liegt es richtig.
    s.set_constraint_value(w, 12.0).unwrap();
    solved(&mut s);
    s.suppress_instance(id, [1, 0], false).unwrap();
    assert!(!s.pattern(id).unwrap().suppressed.contains(&[1, 0]));
    solved(&mut s);
    assert_instances_follow(&s, id, 1e-9);
    let back = positions(&s, id)[&[1, 0]].clone();
    let source: Vec<DVec2> = s
        .pattern(id)
        .unwrap()
        .source_points
        .iter()
        .map(|&p| at(&s, p))
        .collect();
    for (q, p) in back.iter().zip(&source) {
        assert!(close(*q, *p + v(15.0, 0.0), 1e-9));
    }

    // Löschen eines Exemplars in der Oberfläche: über `suppress_instance`, das Muster bleibt.
    let c = s.pattern(id).unwrap().instances[2].curves[1];
    let (pid, Some(index)) = s.pattern_of_curve(c).unwrap() else {
        panic!("Exemplar");
    };
    s.suppress_instance(pid, index, true).unwrap();
    assert_eq!(s.pattern_count(), 1);
    assert!(s.curve(c).is_none());
    solved(&mut s);

    // Die Vorlage und Lagen außerhalb des Gitters lassen sich nicht abwählen.
    assert!(s.suppress_instance(id, [0, 0], true).is_err());
    assert!(s.suppress_instance(id, [7, 0], true).is_err());
}

// ---- Auflösen --------------------------------------------------------------------------------

/// Löst die Handlung `act` das Muster auf, bleiben die Exemplare bitgleich, ihre Variablen werden
/// frei und die Skizze bleibt gelöst.
fn assert_dissolves(s: &mut Sketch, id: PatternId, act: impl FnOnce(&mut Sketch)) {
    assert_dissolves_except(s, id, &[], act);
}

/// Wie `assert_dissolves`; die Punkte `moving` darf die Handlung bewegen (Dehnen).
fn assert_dissolves_except(
    s: &mut Sketch,
    id: PatternId,
    moving: &[PointId],
    act: impl FnOnce(&mut Sketch),
) {
    let before = solved(s).dof;
    let kept: Vec<(PointId, DVec2)> = s
        .pattern(id)
        .unwrap()
        .instances
        .iter()
        .flat_map(|i| i.points.clone())
        .filter(|p| !moving.contains(p))
        .map(|p| (p, at(s, p)))
        .collect();
    let count = s.pattern_count();
    act(s);
    assert_eq!(s.pattern_count(), count - 1, "aufgelöst");
    assert!(s.pattern(id).is_none());
    let check = |s: &Sketch| {
        for &(p, xy) in &kept {
            if let Ok(now) = s.point_xy(p) {
                assert_eq!(now.x.to_bits(), xy.x.to_bits(), "bitgleich");
                assert_eq!(now.y.to_bits(), xy.y.to_bits(), "bitgleich");
            }
        }
    };
    check(s);
    let r = solved(s);
    assert!(r.dof > before, "frei: {} → {}", before, r.dof);
    check(s);
    // Die Werte des Musters sind mit ihm verschwunden.
    assert!(
        s.constraints()
            .all(|(_, d)| !matches!(d.kind, K::PatternValue(..)))
    );
}

/// Muster 3×1 an einem voll bestimmten Rechteck und eine senkrechte Linie, die die untere Kante
/// der Vorlage und des Exemplars (1,0) bei x = 5 bzw. 20 schneidet.
fn bench() -> (Sketch, PatternId, Vec<CurveId>) {
    let mut s = sketch();
    let (src, _, _) = dimensioned_rect(&mut s);
    for x in [5.0, 20.0] {
        let a = s.add_point(x, -3.0).unwrap();
        let b = s.add_point(x, 3.0).unwrap();
        s.add_line(a, b).unwrap();
    }
    let id = s
        .add_pattern(&rect_plan(src.clone(), [3, 1], [15.0, 10.0]))
        .unwrap();
    solved(&mut s);
    (s, id, src)
}

fn instance_curve(s: &Sketch, id: PatternId, index: [i32; 2], k: usize) -> CurveId {
    s.pattern(id)
        .unwrap()
        .instances
        .iter()
        .find(|i| i.at == index)
        .unwrap()
        .curves[k]
}

#[test]
fn deleting_a_source_direction_or_center_dissolves() {
    let (mut s, id, src) = bench();
    assert_dissolves(&mut s, id, |s| s.remove_curve(src[2]).unwrap());

    // Richtungslinie
    let mut s = sketch();
    let (src, _, _) = dimensioned_rect(&mut s);
    let a = s.add_point(0.0, -20.0).unwrap();
    let b = s.add_point(10.0, -15.0).unwrap();
    let dir = s.add_line(a, b).unwrap();
    s.add_constraint(K::Fixed(a, 0.0, -20.0)).unwrap();
    s.add_constraint(K::Fixed(b, 10.0, -15.0)).unwrap();
    let mut plan = rect_plan(src, [3, 1], [15.0, 10.0]);
    plan.kind = PatternKind::Rect {
        dirs: [PatternDir::Line(dir), PatternDir::SketchY],
        extent: [false, false],
        symmetric: [false, false],
    };
    let id = s.add_pattern(&plan).unwrap();
    assert_dissolves(&mut s, id, |s| s.remove_curve(dir).unwrap());

    // Mitte; ein freier Punkt als Mitte überlebt das Aufräumen unbenutzter Punkte.
    let mut s = sketch();
    let (src, _, _) = dimensioned_rect(&mut s);
    let c = s.add_point(-20.0, 0.0).unwrap();
    let id = s
        .add_pattern(&circ_plan(src, c, CircularMode::Full, 4, 0.0))
        .unwrap();
    assert_eq!(s.prune_unused_points(), 0);
    assert!(s.pattern(id).is_some());
    s.add_constraint(K::Fixed(c, -20.0, 0.0)).unwrap();
    assert_dissolves(&mut s, id, |s| s.remove_point(c).unwrap());

    // `dissolve_pattern` selbst; ein zweites Mal ist das Muster unbekannt.
    let (mut s, id, _) = bench();
    assert_dissolves(&mut s, id, |s| s.dissolve_pattern(id).unwrap());
    assert!(s.dissolve_pattern(id).is_err());
}

#[test]
fn trim_break_and_fillet_at_source_or_instance_dissolve() {
    // Stutzen an der Vorlage und am Exemplar.
    let (mut s, id, src) = bench();
    assert_dissolves(&mut s, id, |s| {
        s.trim(src[0], v(2.0, 0.0)).unwrap();
    });
    let (mut s, id, _) = bench();
    let c = instance_curve(&s, id, [1, 0], 0);
    assert_dissolves(&mut s, id, |s| {
        s.trim(c, v(17.0, 0.0)).unwrap();
    });
    // Brechen an der Vorlage und am Exemplar.
    let (mut s, id, src) = bench();
    assert_dissolves(&mut s, id, |s| {
        assert_eq!(s.break_curve(src[0], v(2.0, 0.0)).unwrap().len(), 2);
    });
    let (mut s, id, _) = bench();
    let c = instance_curve(&s, id, [1, 0], 0);
    assert_dissolves(&mut s, id, |s| {
        assert_eq!(s.break_curve(c, v(17.0, 0.0)).unwrap().len(), 2);
    });
    // Rundung an einer Ecke des Exemplars (1,0): unten rechts bei (25, 0).
    let (mut s, id, _) = bench();
    let (bottom, right) = (
        instance_curve(&s, id, [1, 0], 0),
        instance_curve(&s, id, [1, 0], 1),
    );
    let plan = s
        .fillet_plan(pick(bottom, 23.0, 0.0), pick(right, 25.0, 2.0), 1.0)
        .unwrap();
    assert_dissolves(&mut s, id, |s| {
        s.add_fillet(&plan, None).unwrap();
    });
}

// ---- Stil ------------------------------------------------------------------------------------

#[test]
fn construction_follows_the_source_style() {
    let mut s = sketch();
    let (src, _, _) = dimensioned_rect(&mut s);
    let id = s
        .add_pattern(&rect_plan(src.clone(), [3, 1], [15.0, 10.0]))
        .unwrap();
    s.set_construction(src[1], true).unwrap();
    for inst in &s.pattern(id).unwrap().instances {
        assert!(s.curve(inst.curves[1]).unwrap().construction);
        assert!(!s.curve(inst.curves[0]).unwrap().construction);
    }
    // Am Exemplar umgeschaltet gilt es für Vorlage und alle Exemplare.
    let c = instance_curve(&s, id, [2, 0], 1);
    s.set_construction(c, false).unwrap();
    assert!(!s.curve(src[1]).unwrap().construction);
    for inst in &s.pattern(id).unwrap().instances {
        assert!(!s.curve(inst.curves[1]).unwrap().construction);
    }
    assert_eq!(s.pattern_count(), 1);
}

// ---- Zusammenspiel ---------------------------------------------------------------------------

#[test]
fn an_offset_of_an_instance_chain_follows_the_source() {
    let mut s = sketch();
    let (src, _, w) = dimensioned_rect(&mut s);
    let id = s
        .add_pattern(&rect_plan(src, [2, 1], [15.0, 10.0]))
        .unwrap();
    let inst = s.pattern(id).unwrap().instances[0].clone();
    let chains = s.ordered_chains(&inst.curves).unwrap();
    let d = 1.5;
    let plan = s.offset_plan(&chains, d).unwrap();
    let ids = s.add_offset(&plan).unwrap();
    assert!(ids.tied);
    solved(&mut s);
    for width in [14.0, 11.0, 18.0] {
        s.set_constraint_value(w, width).unwrap();
        solved(&mut s);
        assert_instances_follow(&s, id, 1e-9);
        for &c in &ids.curves {
            let line = s.curve_polyline(c, 64).unwrap();
            for p in line {
                let dist = inst
                    .curves
                    .iter()
                    .map(|&q| s.distance_to_curve(q, p).unwrap())
                    .fold(f64::INFINITY, f64::min);
                assert!((dist - d).abs() < 1e-6, "{width}: {dist}");
            }
        }
    }
}

#[test]
fn a_mirrored_instance_follows_the_source() {
    let mut s = sketch();
    let (src, _, w) = dimensioned_rect(&mut s);
    let id = s
        .add_pattern(&rect_plan(src, [2, 1], [15.0, 10.0]))
        .unwrap();
    let a = s.add_point(-5.0, -10.0).unwrap();
    let b = s.add_point(-5.0, 10.0).unwrap();
    let axis = s.add_line(a, b).unwrap();
    s.add_constraint(K::Fixed(a, -5.0, -10.0)).unwrap();
    s.add_constraint(K::Fixed(b, -5.0, 10.0)).unwrap();
    let inst = s.pattern(id).unwrap().instances[0].clone();
    s.add_mirrored(&inst.curves, axis).unwrap();
    solved(&mut s);
    let pairs: Vec<(PointId, PointId)> = s
        .constraints()
        .filter_map(|(_, d)| match d.kind {
            K::Symmetric(p, q, x) if x == axis => Some((p, q)),
            _ => None,
        })
        .collect();
    assert_eq!(pairs.len(), 4);
    for width in [13.0, 7.0] {
        s.set_constraint_value(w, width).unwrap();
        solved(&mut s);
        assert_instances_follow(&s, id, 1e-9);
        for &(p, q) in &pairs {
            let (p, q) = (at(&s, p), at(&s, q));
            assert!(close(q, v(-10.0 - p.x, p.y), 1e-6), "{width}: {p:?} {q:?}");
        }
    }
}

#[test]
fn moving_an_instance_is_blocked_and_moving_the_source_takes_the_instances() {
    let mut s = sketch();
    let src = rect(&mut s, v(0.0, 0.0));
    solved(&mut s);
    let id = s
        .add_pattern(&rect_plan(src.clone(), [3, 2], [15.0, 10.0]))
        .unwrap();
    let mut t = Similarity::identity(DVec2::ZERO);
    t.offset = v(5.0, 7.0);
    // Ein Exemplar in der Menge, auch zusammen mit der Vorlage.
    let c = instance_curve(&s, id, [1, 1], 0);
    for curves in [vec![c], vec![src[0], src[1], c]] {
        let set = s.transform_set(&curves, &[]);
        let rev = s.revision();
        assert_eq!(
            s.apply_transform(&set, &t),
            Err(TransformBlock::PatternInstance)
        );
        assert_eq!(
            s.check_transform(&set, &t),
            Err(TransformBlock::PatternInstance)
        );
        assert_eq!(s.revision(), rev, "Skizze unverändert");
    }
    // Ein Punkt eines Exemplars ebenso, und Drehen ist genauso gesperrt.
    let q = s.pattern(id).unwrap().instances[0].points[0];
    let set = s.transform_set(&[], &[q]);
    assert_eq!(
        s.apply_transform(&set, &t),
        Err(TransformBlock::PatternInstance)
    );
    let mut turn = Similarity::identity(DVec2::ZERO);
    turn.angle = 0.5;
    let set = s.transform_set(&[c], &[]);
    assert_eq!(
        s.apply_transform(&set, &turn),
        Err(TransformBlock::PatternInstance)
    );

    // Die Vorlage verschieben: Die Exemplare folgen.
    let before = positions(&s, id);
    let set = s.transform_set(&src, &[]);
    s.apply_transform(&set, &t).unwrap();
    solved(&mut s);
    assert_eq!(s.pattern_count(), 1);
    assert_instances_follow(&s, id, 1e-9);
    for (index, pts) in positions(&s, id) {
        for (p, q) in pts.iter().zip(&before[&index]) {
            assert!(close(*p, *q + t.offset, 1e-9), "{index:?}");
        }
    }
}

#[test]
fn scaling_the_whole_sketch_scales_spacings_and_keeps_counts_and_angles() {
    let mut s = sketch();
    let (src, _, _) = dimensioned_rect(&mut s);
    let id = s
        .add_pattern(&rect_plan(src, [3, 2], [15.0, 10.0]))
        .unwrap();
    let circle_src = {
        let c = s.add_point(40.0, 30.0).unwrap();
        s.add_circle(c, 2.0).unwrap()
    };
    let center = s.add_point(30.0, 30.0).unwrap();
    let round = s
        .add_pattern(&circ_plan(
            vec![circle_src],
            center,
            CircularMode::Angle,
            4,
            PI / 2.0,
        ))
        .unwrap();
    solved(&mut s);
    let set = s.whole_sketch_set();
    // Exemplare sind nicht in der Menge.
    for (_, d) in s.patterns() {
        for inst in &d.instances {
            assert!(inst.curves.iter().all(|c| !set.curves.contains(c)));
            assert!(inst.points.iter().all(|p| !set.points.contains(p)));
        }
    }
    let mut t = Similarity::identity(DVec2::ZERO);
    t.factor = 2.0;
    let report = s.apply_transform(&set, &t).unwrap();
    assert!(
        report
            .scaled
            .contains(&value_id(&s, id, PatternSlot::Spacing1))
    );
    assert_eq!(value(&s, id, PatternSlot::Spacing1), 30.0);
    assert_eq!(value(&s, id, PatternSlot::Spacing2), 20.0);
    assert_eq!(value(&s, id, PatternSlot::Count1), 3.0);
    assert_eq!(value(&s, id, PatternSlot::Count2), 2.0);
    assert_eq!(value(&s, round, PatternSlot::Angle), PI / 2.0);
    assert_eq!(value(&s, round, PatternSlot::Count), 4.0);
    // Die Lagen stimmen schon ohne Lösen und danach.
    assert_instances_follow(&s, id, 1e-9);
    assert_instances_follow(&s, round, 1e-9);
    solved(&mut s);
    assert_instances_follow(&s, id, 1e-9);
    assert_instances_follow(&s, round, 1e-9);
    let p = positions(&s, id)[&[2, 1]][0];
    assert!(close(p, v(60.0, 20.0), 1e-9), "{p:?}");
    assert_eq!(s.radius(instance_curve(&s, round, [1, 0], 0)), Some(4.0));
}

#[test]
fn copying_an_instance_gives_a_loose_copy() {
    let mut s = sketch();
    let (src, _, w) = dimensioned_rect(&mut s);
    let id = s
        .add_pattern(&rect_plan(src, [2, 1], [15.0, 10.0]))
        .unwrap();
    let inst = s.pattern(id).unwrap().instances[0].clone();
    let set = s.transform_set(&inst.curves, &[]);
    let mut t = Similarity::identity(DVec2::ZERO);
    t.offset = v(0.0, 30.0);
    let map = s.copy_transformed(&set, &t).unwrap();
    assert_eq!(s.pattern_count(), 1);
    let copies: Vec<CurveId> = inst.curves.iter().map(|c| map.curves[c]).collect();
    assert!(copies.iter().all(|&c| s.pattern_of_curve(c).is_none()));
    let corner = map.points[&inst.points[0]];
    let before = at(&s, corner);
    assert!(close(before, v(15.0, 30.0), 1e-12));
    s.set_constraint_value(w, 14.0).unwrap();
    solved(&mut s);
    assert!(close(at(&s, corner), before, 1e-9), "lose: folgt nicht");
}

#[test]
fn instances_turning_with_the_angle_do_not_count_as_folding() {
    let mut s = sketch();
    let src = {
        let a = s.add_point(20.0, 0.0).unwrap();
        let b = s.add_point(30.0, 0.0).unwrap();
        let l = s.add_line(a, b).unwrap();
        s.add_constraint(K::Fixed(a, 20.0, 0.0)).unwrap();
        s.add_constraint(K::Fixed(b, 30.0, 0.0)).unwrap();
        vec![l]
    };
    let c = s.add_point(0.0, 0.0).unwrap();
    s.add_constraint(K::Fixed(c, 0.0, 0.0)).unwrap();
    let id = s
        .add_pattern(&circ_plan(src, c, CircularMode::Angle, 4, PI / 2.0))
        .unwrap();
    solved(&mut s);
    let before = s.clone();
    let angle = value_id(&s, id, PatternSlot::Angle);
    s.set_constraint_value(angle, 300.0_f64.to_radians())
        .unwrap();
    solved(&mut s);
    assert_instances_follow(&s, id, 1e-9);
    // Das Exemplar (1,0) hat sich von 30° auf 100° gedreht, seine Linie also umgekehrt.
    assert!(!folded_over(&before, &s));
    assert!(!sweep_crossed_half_turn(&before, &s));
}

// ---- Bearbeiten ------------------------------------------------------------------------------

#[test]
fn editing_equals_a_freshly_created_pattern() {
    let build = || {
        let mut s = sketch();
        let (src, _, _) = dimensioned_rect(&mut s);
        (s, src)
    };
    let (mut s, src) = build();
    let id = s
        .add_pattern(&rect_plan(src.clone(), [3, 2], [15.0, 10.0]))
        .unwrap();
    let mut plan = rect_plan(src.clone(), [3, 2], [36.0, 10.0]);
    plan.kind = PatternKind::Rect {
        dirs: [PatternDir::SketchX, PatternDir::SketchY],
        extent: [true, false],
        symmetric: [false, true],
    };
    plan.suppressed = vec![[1, -1], [2, 1]];
    let rev = s.revision();
    let edit = s.edit_pattern(id, &plan).unwrap();
    assert_ne!(s.revision(), rev);
    // 3×2 → 3×3 (symmetrisch), zwei abgewählt: 8 − 2 Exemplare, vorher 5.
    assert_eq!(s.pattern(id).unwrap().instances.len(), 6);
    assert_eq!(edit.added as i64 - edit.removed as i64, 1);
    assert_eq!(edit.dropped_constraints, 0);
    assert_eq!(current_plan(&s, id), plan);
    solved(&mut s);

    let (mut fresh, src2) = build();
    assert_eq!(src, src2);
    let mut plan2 = plan.clone();
    plan2.sources = src2;
    let id2 = fresh.add_pattern(&plan2).unwrap();
    solved(&mut fresh);
    let (a, b) = (positions(&s, id), positions(&fresh, id2));
    assert_eq!(a.len(), b.len());
    for (index, pts) in &a {
        for (p, q) in pts.iter().zip(&b[index]) {
            assert!(close(*p, *q, 1e-9), "{index:?}");
        }
    }
    // Derselbe Plan noch einmal ändert nichts.
    let rev = s.revision();
    assert_eq!(s.edit_pattern(id, &plan).unwrap(), PatternEdit::default());
    assert_eq!(s.revision(), rev);
    // Ein ungültiger Plan lässt die Skizze unverändert.
    let mut bad = plan.clone();
    bad.values[0] = 0.0;
    assert!(s.edit_pattern(id, &bad).is_err());
    assert_eq!(s.revision(), rev);
    // Kreismuster: Voll → Winkel bekommt den Winkelwert, zurück verliert ihn.
    let mut s = sketch();
    let src = rect(&mut s, v(20.0, 0.0));
    let c = s.add_point(0.0, 0.0).unwrap();
    let id = s
        .add_pattern(&circ_plan(src.clone(), c, CircularMode::Full, 6, 0.0))
        .unwrap();
    s.edit_pattern(id, &circ_plan(src.clone(), c, CircularMode::Angle, 3, PI))
        .unwrap();
    assert_eq!(value(&s, id, PatternSlot::Angle), PI);
    assert_eq!(s.pattern(id).unwrap().instances.len(), 2);
    solved(&mut s);
    assert_instances_follow(&s, id, 1e-9);
    s.edit_pattern(id, &circ_plan(src, c, CircularMode::Full, 6, 0.0))
        .unwrap();
    assert_eq!(s.pattern(id).unwrap().values.len(), 1);
    solved(&mut s);
    assert_instances_follow(&s, id, 1e-9);
}

// ---- Klick auf ein Exemplar ------------------------------------------------------------------

#[test]
fn clicking_picks_an_instance_but_not_the_source() {
    let mut s = sketch();
    let src = rect(&mut s, v(0.0, 0.0));
    let mut plan = rect_plan(src, [3, 2], [15.0, 10.0]);
    // Rein: geht ohne angelegtes Muster, auch für abgewählte Lagen.
    assert_eq!(
        s.pattern_instance_at(&plan, v(20.0, 10.0), 0.5),
        Some([1, 1])
    );
    assert_eq!(s.pattern_instance_at(&plan, v(5.0, 0.0), 0.5), None);
    assert_eq!(s.pattern_instance_at(&plan, v(12.5, 3.0), 0.5), None);
    plan.suppressed.push([1, 1]);
    assert_eq!(
        s.pattern_instance_at(&plan, v(20.0, 10.0), 0.5),
        Some([1, 1])
    );
    s.add_pattern(&plan).unwrap();
    assert_eq!(
        s.pattern_instance_at(&plan, v(35.0, 10.2), 0.5),
        Some([2, 1])
    );
}

// ---- Korrektur 1 -----------------------------------------------------------------------------

#[test]
fn extend_and_chamfer_at_source_or_instance_dissolve() {
    // Eine freie Linie (0,0)–(5,0) als Vorlage, Exemplare bei y = 10 und 20, eine feste
    // senkrechte Linie bei x = 8 als Ziel des Dehnens.
    let build = || {
        let mut s = sketch();
        let a = s.add_point(0.0, 0.0).unwrap();
        let b = s.add_point(5.0, 0.0).unwrap();
        let line = s.add_line(a, b).unwrap();
        let (c, d) = (
            s.add_point(8.0, -5.0).unwrap(),
            s.add_point(8.0, 25.0).unwrap(),
        );
        s.add_line(c, d).unwrap();
        s.add_constraint(K::Fixed(c, 8.0, -5.0)).unwrap();
        s.add_constraint(K::Fixed(d, 8.0, 25.0)).unwrap();
        let id = s
            .add_pattern(&rect_plan(vec![line], [1, 3], [15.0, 10.0]))
            .unwrap();
        solved(&mut s);
        (s, id, line)
    };
    let (mut s, id, line) = build();
    assert_dissolves(&mut s, id, |s| {
        assert!(s.extend(line, v(4.0, 0.0)).unwrap())
    });
    let (mut s, id, _) = build();
    let inst = s.pattern(id).unwrap().instances[0].clone();
    assert_eq!(inst.at, [0, 1]);
    assert_dissolves_except(&mut s, id, &[inst.points[1]], |s| {
        assert!(s.extend(inst.curves[0], v(4.0, 10.0)).unwrap());
    });
    // Fase an einer Ecke des Exemplars (1,0) unten rechts bei (25, 0) und an der Vorlage.
    let (mut s, id, _) = bench();
    let (bottom, right) = (
        instance_curve(&s, id, [1, 0], 0),
        instance_curve(&s, id, [1, 0], 1),
    );
    let plan = s
        .chamfer_plan(
            pick(bottom, 23.0, 0.0),
            pick(right, 25.0, 2.0),
            ChamferKind::Equal(1.0),
        )
        .unwrap();
    assert_dissolves(&mut s, id, |s| {
        s.add_chamfer(&plan).unwrap();
    });
    let (mut s, id, src) = bench();
    let plan = s
        .chamfer_plan(
            pick(src[1], 10.0, 4.0),
            pick(src[2], 8.0, 6.0),
            ChamferKind::Equal(1.0),
        )
        .unwrap();
    assert_dissolves(&mut s, id, |s| {
        s.add_chamfer(&plan).unwrap();
    });
}

/// Muster 3×1 und eine fremde Linie, die am Punkt 0 des Exemplars (2,0) beginnt (wie es ein
/// Zeichenwerkzeug mit Fang anlegt). Liefert Skizze, Muster, Linie, ihren freien Punkt und die
/// alte Lage des geteilten Endes.
fn shared_corner() -> (Sketch, PatternId, CurveId, PointId, DVec2) {
    let mut s = sketch();
    let (src, _, _) = dimensioned_rect(&mut s);
    let id = s
        .add_pattern(&rect_plan(src, [3, 1], [15.0, 10.0]))
        .unwrap();
    let corner = s.pattern(id).unwrap().instances[1].points[0];
    let q = s.add_point(30.0, -20.0).unwrap();
    let line = s.add_line(corner, q).unwrap();
    solved(&mut s);
    let old = at(&s, corner);
    (s, id, line, q, old)
}

/// Die Linie lebt noch, ihr Ende liegt an der alten Lage, kein Punkt ist verwaist, gelöst.
fn assert_line_kept(s: &mut Sketch, line: CurveId, q: PointId, old: DVec2) {
    let Some(planora_sketch::model::CurveKind::Line { a, b }) =
        s.curve(line).map(|d| d.kind.clone())
    else {
        panic!("Linie gelöscht");
    };
    assert_eq!(b, q);
    assert!(close(at(s, a), old, 1e-9), "{:?}", at(s, a));
    solved(s);
    assert!(close(at(s, a), old, 1e-9), "{:?}", at(s, a));
    assert_eq!(s.prune_unused_points(), 0, "kein verwaister Punkt");
}

#[test]
fn a_foreign_line_at_a_dropped_instance_keeps_its_end() {
    // (a) Anzahl 3 → 2 über den Wert und über `edit_pattern`.
    let (mut s, id, line, q, old) = shared_corner();
    let count = value_id(&s, id, PatternSlot::Count1);
    s.set_constraint_value(count, 2.0).unwrap();
    assert_line_kept(&mut s, line, q, old);
    let (mut s, id, line, q, old) = shared_corner();
    let mut plan = current_plan(&s, id);
    plan.counts[0] = 2;
    let edit = s.edit_pattern(id, &plan).unwrap();
    assert_eq!(edit.removed, 1);
    assert_eq!(edit.dropped_constraints, 0);
    assert_line_kept(&mut s, line, q, old);
    // (b) Abwählen; wieder angewählt entsteht ein neues Exemplar, die Linie bleibt lose.
    let (mut s, id, line, q, old) = shared_corner();
    s.suppress_instance(id, [2, 0], true).unwrap();
    assert_line_kept(&mut s, line, q, old);
    s.suppress_instance(id, [2, 0], false).unwrap();
    assert_line_kept(&mut s, line, q, old);
    // Eine Abhängigkeit zwischen der Linie und dem Punkt des Exemplars geht mit dem Exemplar.
    let (mut s, id, line, q, old) = shared_corner();
    let inst = s.pattern(id).unwrap().instances[1].clone();
    let other = inst.points[2];
    let tie = s.add_constraint(K::Distance(q, other, 20.0)).unwrap();
    solved(&mut s);
    let edit = {
        let mut plan = current_plan(&s, id);
        plan.counts[0] = 2;
        s.edit_pattern(id, &plan).unwrap()
    };
    assert_eq!(edit.dropped_constraints, 1);
    assert!(s.constraint(tie).is_none());
    assert_line_kept(&mut s, line, q, old);
    // (c) Auflösen behält ohnehin alles.
    let (mut s, id, line, q, old) = shared_corner();
    s.dissolve_pattern(id).unwrap();
    let Some(planora_sketch::model::CurveKind::Line { a, .. }) =
        s.curve(line).map(|d| d.kind.clone())
    else {
        panic!("Linie gelöscht");
    };
    assert_eq!(a, inst_point(&shared_corner()));
    assert_line_kept(&mut s, line, q, old);
}

/// Der geteilte Punkt in einer frischen Skizze von `shared_corner` (Ids sind gleich gebaut).
fn inst_point(bench: &(Sketch, PatternId, CurveId, PointId, DVec2)) -> PointId {
    bench.0.pattern(bench.1).unwrap().instances[1].points[0]
}
