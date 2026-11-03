//! Verschieben, Drehen, Skalieren und Kopieren als reine Rechnung (Skizzen 4, Task 5): Was innen
//! gilt, bleibt ohne Solver genau; was nach außen bindet, entfällt oder verhindert die Bewegung;
//! dichte Wertefolgen statt Stichproben.

use planora_math::{DVec2, Frame, PI, TAU, Tolerance};
use planora_sketch::constraint::{ConstraintId, ConstraintKind, LineOrPoints};
use planora_sketch::fillet::{ChamferKind, CurvePick, FilletIds};
use planora_sketch::model::{CurveId, CurveKind, PointId, Sketch};
use planora_sketch::offset::OffsetIds;
use planora_sketch::shapes::{
    SlotPlan, SlotStyle, folded_over, polygon_inscribed, rect_three_point, sweep_crossed_half_turn,
};
use planora_sketch::solver::{SolveReport, SolveStatus};
use planora_sketch::transform::{CopyMap, Similarity, TransformBlock, TransformSet};
use planora_sketch::{RectIds, SketchError};

use ConstraintKind as K;

const TOL: f64 = 1e-6;

fn sketch() -> Sketch {
    Sketch::new(Frame::WORLD, "S", Tolerance::default())
}

fn v(x: f64, y: f64) -> DVec2 {
    DVec2::new(x, y)
}

fn pt(s: &mut Sketch, x: f64, y: f64) -> PointId {
    s.add_point(x, y).unwrap()
}

fn at(s: &Sketch, p: PointId) -> DVec2 {
    s.point_xy(p).unwrap()
}

fn moved(offset: DVec2) -> Similarity {
    Similarity {
        offset,
        ..Similarity::identity(DVec2::ZERO)
    }
}

fn turned(base: DVec2, degrees: f64) -> Similarity {
    Similarity {
        angle: degrees.to_radians(),
        ..Similarity::identity(base)
    }
}

fn scaled(base: DVec2, factor: f64) -> Similarity {
    Similarity {
        factor,
        ..Similarity::identity(base)
    }
}

fn line_ends(s: &Sketch, l: CurveId) -> (PointId, PointId) {
    match s.curve(l).unwrap().kind {
        CurveKind::Line { a, b } => (a, b),
        ref k => panic!("keine Linie: {k:?}"),
    }
}

fn line_dir(s: &Sketch, l: CurveId) -> DVec2 {
    let (a, b) = line_ends(s, l);
    at(s, b) - at(s, a)
}

fn only_curves(s: &Sketch, curves: &[CurveId]) -> TransformSet {
    s.transform_set(curves, &[])
}

fn has(s: &Sketch, id: ConstraintId) -> bool {
    s.constraint(id).is_some()
}

/// Solved, kein Widerspruch.
fn assert_solved(r: &SolveReport) {
    assert_eq!(r.status, SolveStatus::Solved, "{r:?}");
    assert!(r.conflicting.is_empty(), "Widerspruch: {r:?}");
}

/// Beim Lösen bewegt sich nichts.
fn assert_still(s: &Sketch) -> SolveReport {
    let mut solved = s.clone();
    let r = solved.solve();
    assert_solved(&r);
    assert_eq!(r.iterations, 0, "{r:?}");
    assert_eq!(solved.variables(), s.variables());
    r
}

/// Jede aktive Abhängigkeit (auch interne) hält an den aktuellen Werten bis auf `limit`. Eine
/// Tangente am gemeinsamen Ende mit falschem Vorzeichen stünde hier bei ±π.
fn assert_holds(s: &Sketch, limit: f64) {
    for (id, d) in s.constraints().chain(s.internal_constraints()) {
        if !d.enabled || d.driven {
            continue;
        }
        for r in s.evaluate(id, s.variables()).unwrap().residuals {
            assert!(r.v.abs() <= limit, "{:?}: {} über {limit}", d.kind, r.v);
        }
    }
}

/// Die Skizze bleibt bitgleich: Werte, Daten, Stand.
fn assert_untouched(after: &Sketch, before: &Sketch) {
    assert_eq!(after.variables(), before.variables());
    assert_eq!(after.to_data(), before.to_data());
    assert_eq!(after.revision(), before.revision());
}

fn assert_blocked(s: &mut Sketch, set: &TransformSet, t: &Similarity, block: TransformBlock) {
    let before = s.clone();
    assert_eq!(s.check_transform(set, t), Err(block.clone()));
    assert_eq!(s.apply_transform(set, t).unwrap_err(), block);
    assert_untouched(s, &before);
    assert!(s.transform_preview(set, t, 64).is_empty());
    assert_untouched(s, &before);
}

/// Nichts klappt um, keine Seite wechselt: Jede Linie zeigt, wohin `t` sie schickt, jeder Bogen
/// läuft gleich herum und gleich weit, Zweige und die Seite der Mitte an einer Linie mit Tangente
/// bleiben, und das Lösen danach klappt ebenfalls nichts um.
fn assert_sides_kept(before: &Sketch, after: &Sketch, t: &Similarity) {
    for (c, d) in after.curves() {
        if before.curve(c).is_none_or(|o| o.kind != d.kind) {
            continue;
        }
        match d.kind {
            CurveKind::Line { a, b } => {
                let want = t.apply(at(before, b)) - t.apply(at(before, a));
                let now = at(after, b) - at(after, a);
                assert!(want.dot(now) > 0.0, "umgeklappt: {t:?}");
            }
            CurveKind::Arc { center, start, end } => {
                let sweep = |s: &Sketch| {
                    let (u, w) = (at(s, start) - at(s, center), at(s, end) - at(s, center));
                    u.perp_dot(w).atan2(u.dot(w)).rem_euclid(TAU)
                };
                assert!((sweep(before) - sweep(after)).abs() < 1e-6, "Bogen: {t:?}");
            }
            _ => {}
        }
    }
    let side = |s: &Sketch, line: CurveId, round: CurveId| {
        let CurveKind::Line { a, b } = s.curve(line).unwrap().kind else {
            return None;
        };
        let (CurveKind::Arc { center, .. } | CurveKind::Circle { center, .. }) =
            s.curve(round).unwrap().kind
        else {
            return None;
        };
        Some((at(s, b) - at(s, a)).perp_dot(at(s, center) - at(s, a)) > 0.0)
    };
    for (id, d) in after.constraints() {
        let Some(old) = before.constraint(id) else {
            continue;
        };
        assert_eq!(d.branch(), old.branch(), "{:?}: {t:?}", d.kind);
        if let K::Tangent(x, y) = d.kind {
            assert_eq!(side(before, x, y), side(after, x, y), "{:?}: {t:?}", d.kind);
        }
    }
    let mut solved = after.clone();
    solved.solve();
    assert!(!folded_over(after, &solved), "{t:?}");
}

/// Rechteck 20×10 ab dem Ursprung mit H, V und zwei Abständen.
fn dimensioned_rect() -> (Sketch, RectIds, ConstraintId, ConstraintId) {
    let mut s = sketch();
    let r = s.add_rectangle(v(0.0, 0.0), v(20.0, 10.0)).unwrap();
    let w = s
        .add_constraint(K::Distance(r.points[0], r.points[1], 20.0))
        .unwrap();
    let h = s
        .add_constraint(K::Distance(r.points[1], r.points[2], 10.0))
        .unwrap();
    (s, r, w, h)
}

// ---------------------------------------------------------------------------------------------
// Rechnung
// ---------------------------------------------------------------------------------------------

#[test]
fn a_similarity_scales_and_turns_about_its_base_and_then_shifts() {
    let t = Similarity {
        base: v(1.0, 1.0),
        offset: v(10.0, 0.0),
        angle: 0.5 * PI,
        factor: 2.0,
    };
    assert!(t.apply(v(2.0, 1.0)).distance(v(11.0, 3.0)) < 1e-14);
    assert_eq!(t.apply(v(1.0, 1.0)), v(11.0, 1.0));
    let same = Similarity::identity(v(4.0, 4.0));
    assert_eq!(same.apply(v(-3.0, 8.5)), v(-3.0, 8.5));
    assert!(same.is_identity(TOL));
    assert!(!t.is_identity(TOL));
    for not in [
        moved(v(0.0, 1e-3)),
        turned(v(0.0, 0.0), 1.0),
        scaled(v(0.0, 0.0), 1.001),
    ] {
        assert!(!not.is_identity(TOL), "{not:?}");
    }
    assert!(turned(v(3.0, 3.0), 360.0).is_identity(TOL));
    assert!(!scaled(v(0.0, 0.0), f64::NAN).is_identity(TOL));
}

#[test]
fn a_transform_set_holds_the_points_of_its_curves_once_and_only_live_objects() {
    let mut s = sketch();
    let (a, b, c) = (
        pt(&mut s, 0.0, 0.0),
        pt(&mut s, 5.0, 0.0),
        pt(&mut s, 9.0, 9.0),
    );
    let l = s.add_line(a, b).unwrap();
    let gone = s.add_circle(c, 2.0).unwrap();
    s.remove_curve(gone).unwrap();
    let set = s.transform_set(&[l, l, gone], &[b, c, c]);
    assert_eq!(set.curves, vec![l]);
    assert_eq!(set.points, vec![a, b, c]);
}

// ---------------------------------------------------------------------------------------------
// Verschieben und Drehen
// ---------------------------------------------------------------------------------------------

#[test]
fn a_moved_rectangle_keeps_every_constraint_and_lies_exactly() {
    let (mut s, r, w, h) = dimensioned_rect();
    let count = s.constraints().count();
    let set = only_curves(&s, &r.lines);
    let t = moved(v(7.0, -3.0));
    assert_eq!(s.check_transform(&set, &t), Ok(()));
    let report = s.apply_transform(&set, &t).unwrap();
    assert!(report.dropped.is_empty(), "{report:?}");
    assert!(report.scaled.is_empty(), "{report:?}");
    assert_eq!(s.constraints().count(), count);
    assert!(has(&s, w) && has(&s, h));
    assert_still(&s);
    let want = [v(7.0, -3.0), v(27.0, -3.0), v(27.0, 7.0), v(7.0, 7.0)];
    for (p, q) in r.points.iter().zip(want) {
        assert_eq!(at(&s, *p), q);
    }
}

#[test]
fn turning_a_rectangle_drops_horizontal_and_vertical_and_keeps_the_rest() {
    let (mut s, r, w, h) = dimensioned_rect();
    let hv: Vec<ConstraintId> = s
        .constraints()
        .filter(|(_, d)| matches!(d.kind, K::Horizontal(_) | K::Vertical(_)))
        .map(|(id, _)| id)
        .collect();
    assert_eq!(hv.len(), 4);
    let set = only_curves(&s, &r.lines);
    let report = s.apply_transform(&set, &turned(v(5.0, 5.0), 30.0)).unwrap();
    let mut dropped = report.dropped.clone();
    dropped.sort_by_key(|c| c.index());
    assert_eq!(dropped, hv);
    assert!(hv.iter().all(|&c| !has(&s, c)));
    assert!(has(&s, w) && has(&s, h));
    let (bottom, right) = (line_dir(&s, r.lines[0]), line_dir(&s, r.lines[1]));
    assert!((bottom.length() - 20.0).abs() < 1e-12);
    assert!((right.length() - 10.0).abs() < 1e-12);
    assert!(bottom.dot(right).abs() < 1e-11);
    assert!((bottom.y.atan2(bottom.x) - 30f64.to_radians()).abs() < 1e-12);
    assert_still(&s);
}

#[test]
fn horizontal_and_vertical_also_fall_at_multiples_of_ninety_degrees() {
    for degrees in [90.0, 180.0, 270.0, -90.0] {
        let (mut s, r, ..) = dimensioned_rect();
        let set = only_curves(&s, &r.lines);
        let report = s
            .apply_transform(&set, &turned(v(5.0, 5.0), degrees))
            .unwrap();
        assert_eq!(report.dropped.len(), 4, "{degrees}");
        assert_still(&s);
    }
}

#[test]
fn a_three_point_rectangle_keeps_parallel_and_perpendicular_when_turned() {
    let mut s = sketch();
    let corners = rect_three_point(v(1.0, 2.0), v(21.0, 9.0), v(15.0, 30.0), TOL).unwrap();
    let r = s.add_rect_three_point(corners).unwrap();
    let kinds: Vec<K> = s.constraints().map(|(_, d)| d.kind.clone()).collect();
    assert!(kinds.iter().any(|k| matches!(k, K::Parallel(..))));
    assert!(kinds.iter().any(|k| matches!(k, K::Perpendicular(..))));
    let count = kinds.len();
    let set = only_curves(&s, &r.lines);
    let report = s
        .apply_transform(&set, &turned(v(-3.0, 4.0), 47.0))
        .unwrap();
    assert!(report.dropped.is_empty(), "{report:?}");
    assert_eq!(s.constraints().count(), count);
    assert_still(&s);
}

/// Ein Bogen tangential am Ende einer Linie, mit Radius und Länge. Der Radius ist `Radius` an einem
/// Bogen, nur der Bogen hat ein freies Ende.
fn tangent_arc_on_line() -> (Sketch, CurveId, CurveId, ConstraintId) {
    let mut s = sketch();
    let (a, b) = (pt(&mut s, 0.0, 0.0), pt(&mut s, 20.0, 0.0));
    let (m, e) = (pt(&mut s, 20.0, 5.0), pt(&mut s, 25.0, 5.0));
    let l = s.add_line(a, b).unwrap();
    let arc = s.add_arc(m, b, e).unwrap();
    s.add_constraint(K::Tangent(l, arc)).unwrap();
    let dim = s.add_constraint(K::Radius(arc, 5.0)).unwrap();
    s.add_constraint(K::Distance(a, b, 20.0)).unwrap();
    (s, l, arc, dim)
}

fn polygon_of(sides: usize) -> Sketch {
    let plan = polygon_inscribed(v(7.0, 3.0), v(27.0, 11.0), sides, TOL).unwrap();
    let mut s = sketch();
    s.add_polygon(&plan).unwrap();
    s
}

fn slot() -> Sketch {
    let plan = SlotPlan {
        a: v(0.0, 0.0),
        b: v(40.0, 12.0),
        radius: 5.0,
    };
    let mut s = sketch();
    s.add_slot(&plan, SlotStyle::CenterToCenter).unwrap();
    s
}

fn midpoint(s: &Sketch, l: CurveId) -> DVec2 {
    let (a, b) = line_ends(s, l);
    (at(s, a) + at(s, b)) * 0.5
}

/// Rechteck 50×30 mit H, V, zwei Abständen und vier Rundungen r 5 (eine mit Maß, drei mit `Equal`).
fn rounded_rect() -> (Sketch, Vec<FilletIds>) {
    let mut s = sketch();
    let r = s.add_rectangle(v(0.0, 0.0), v(50.0, 30.0)).unwrap();
    s.add_constraint(K::Distance(r.points[0], r.points[1], 50.0))
        .unwrap();
    s.add_constraint(K::Distance(r.points[1], r.points[2], 30.0))
        .unwrap();
    let mut ids: Vec<FilletIds> = Vec::new();
    for i in 0..4 {
        let (a, b) = (r.lines[i], r.lines[(i + 1) % 4]);
        let pick = |s: &Sketch, curve: CurveId| CurvePick {
            curve,
            at: midpoint(s, curve),
        };
        let plan = s.fillet_plan(pick(&s, a), pick(&s, b), 5.0).unwrap();
        ids.push(s.add_fillet(&plan, ids.first().map(|f| f.arc)).unwrap());
    }
    (s, ids)
}

/// Dreht und verschiebt `s` als Ganzes über die dichte Folge: Winkel 0…360° in 1°, Versatz bis 500.
fn sweep_motion(s: &Sketch, step: usize) {
    let set = s.whole_sketch_set();
    let base = v(5.0, 5.0);
    let before = s.clone().solve();
    assert_solved(&before);
    for degree in (0..=360).step_by(step) {
        let far = 500.0 * f64::from(degree) / 360.0;
        let offset = DVec2::from_angle(f64::from(degree) * 0.7) * far;
        let angle = f64::from(degree).to_radians();
        for t in [
            Similarity {
                angle,
                ..Similarity::identity(base)
            },
            Similarity {
                angle,
                offset,
                ..Similarity::identity(base)
            },
            Similarity {
                offset,
                ..Similarity::identity(base)
            },
        ] {
            if t.is_identity(TOL) {
                let mut same = s.clone();
                assert_eq!(
                    same.apply_transform(&set, &t),
                    Err(TransformBlock::NothingToDo)
                );
                continue;
            }
            let mut after = s.clone();
            let report = after.apply_transform(&set, &t).unwrap();
            for &p in &set.points {
                let want = t.apply(at(s, p));
                assert!(at(&after, p).distance(want) <= 1e-9, "{t:?}");
            }
            assert_holds(&after, 1e-9);
            assert_sides_kept(s, &after, &t);
            let r = assert_still(&after);
            let left: Vec<ConstraintId> = before
                .redundant
                .iter()
                .copied()
                .filter(|c| !report.dropped.contains(c))
                .collect();
            assert_eq!(r.redundant, left, "{t:?}");
        }
    }
}

#[test]
fn a_tangent_arc_stays_exact_over_a_dense_motion() {
    sweep_motion(&tangent_arc_on_line().0, 1);
}

#[test]
fn a_polygon_of_12_stays_exact_over_a_dense_motion() {
    sweep_motion(&polygon_of(12), 1);
}

/// Das 64-Eck kostet im Debug-Build je Lösen viel: gleiche Prüfung, gröbere Folge.
#[test]
fn a_polygon_of_64_stays_exact_over_a_coarse_motion() {
    sweep_motion(&polygon_of(64), 15);
}

#[test]
fn a_slot_stays_exact_over_a_dense_motion() {
    sweep_motion(&slot(), 1);
}

#[test]
fn a_rounded_rectangle_stays_exact_over_a_dense_motion() {
    sweep_motion(&rounded_rect().0, 1);
}

/// Setzt ein Maß wie die Oberfläche: tangentialen Bogen neu legen, direkt lösen, sonst in Schritten.
fn change(s: &Sketch, id: ConstraintId, old: f64, new: f64) -> Option<Sketch> {
    let mut trial = s.clone();
    trial.set_constraint_value(id, new).unwrap();
    let unsolved = trial.clone();
    trial.prepare_dimension_change(id, new);
    let folds = |t: &Sketch| folded_over(&unsolved, t) || sweep_crossed_half_turn(&unsolved, t);
    let r = trial.solve();
    if r.status == SolveStatus::Solved && !folds(&trial) {
        return Some(trial);
    }
    let accept = |t: &Sketch, r: &SolveReport| r.status == SolveStatus::Solved && !folds(t);
    unsolved
        .solve_in_steps(&[(id, old, new)], accept)
        .map(|x| x.0)
}

/// Wie eine Tangente am gemeinsamen Ende: Nach Drehen und Skalieren gilt ihr Vorzeichen weiter, und
/// eine spätere Maßänderung klappt nichts um.
#[test]
fn tangents_of_a_rounded_rectangle_survive_a_later_radius_change_after_a_transform() {
    let (s, ids) = rounded_rect();
    let dim = ids[0].radius_dim.unwrap();
    let set = s.whole_sketch_set();
    for degree in (0..360).step_by(15) {
        for k in [0.5, 1.0, 2.0] {
            let t = Similarity {
                base: v(5.0, 5.0),
                offset: v(30.0, -80.0),
                angle: f64::from(degree).to_radians(),
                factor: k,
            };
            let mut turned = s.clone();
            turned.apply_transform(&set, &t).unwrap();
            assert_holds(&turned, 1e-9 * k.max(1.0));
            assert_still(&turned);
            for tenth in (10..=140).step_by(5) {
                let r = f64::from(tenth) / 10.0 * k;
                let label = format!("{degree}° k {k} r {r}");
                let after = change(&turned, dim, 5.0 * k, r).expect(&label);
                assert_holds(&after, 1e-6 * k.max(1.0));
                assert!(!folded_over(&turned, &after), "{label}");
                assert!(!sweep_crossed_half_turn(&turned, &after), "{label}");
                for f in &ids {
                    assert!(
                        (after.radius(f.arc).unwrap() - r).abs() < 1e-6 * k.max(1.0),
                        "{label}"
                    );
                }
            }
        }
    }
}

#[test]
fn a_line_with_a_tangent_arc_keeps_its_sign_through_a_transform_and_a_radius_change() {
    let (s, l, arc, dim) = tangent_arc_on_line();
    let set = s.whole_sketch_set();
    for degree in (0..360).step_by(5) {
        for k in [0.1, 0.5, 1.0, 3.0, 50.0] {
            let t = Similarity {
                base: v(-4.0, 9.0),
                offset: v(100.0, 0.0),
                angle: f64::from(degree).to_radians(),
                factor: k,
            };
            let mut turned = s.clone();
            turned.apply_transform(&set, &t).unwrap();
            assert_holds(&turned, 1e-9 * k.max(1.0));
            assert_still(&turned);
            for tenth in [2, 5, 15, 30, 80] {
                let r = f64::from(tenth) * 0.5 * k;
                let label = format!("{degree}° k {k} r {r}");
                let after = change(&turned, dim, 5.0 * k, r).expect(&label);
                assert_holds(&after, 1e-6 * k.max(1.0));
                assert!(!folded_over(&turned, &after), "{label}");
                // Die Mitte bleibt auf derselben Seite der Linie.
                let CurveKind::Arc { center, .. } = after.curve(arc).unwrap().kind else {
                    panic!();
                };
                let (a, _) = line_ends(&after, l);
                let side = line_dir(&after, l).perp_dot(at(&after, center) - at(&after, a));
                assert!(side > 0.0, "{label}");
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Äußeres
// ---------------------------------------------------------------------------------------------

#[test]
fn a_single_line_of_a_rectangle_leaves_the_others_on_new_points() {
    let (mut s, r, w, h) = dimensioned_rect();
    let [p0, p1, p2, p3] = r.points;
    let [bottom, right, top, left] = r.lines;
    let (c0, c3) = (at(&s, p0), at(&s, p3));
    let set = only_curves(&s, &[bottom]);
    let report = s.apply_transform(&set, &moved(v(3.0, -8.0))).unwrap();
    // Die Linie liegt neu und behält ihre Punkte.
    assert_eq!(line_ends(&s, bottom), (p0, p1));
    assert_eq!(at(&s, p0), c0 + v(3.0, -8.0));
    assert_eq!(at(&s, p1), v(23.0, -8.0));
    assert!(has(&s, w), "innen");
    // Rechts und links hängen an neuen Punkten an der alten Lage, oben ist unberührt.
    let (ra, rb) = line_ends(&s, right);
    assert!(ra != p1 && rb == p2);
    assert_eq!(at(&s, ra), v(20.0, 0.0));
    let (la, lb) = line_ends(&s, left);
    assert!(la == p3 && lb != p0);
    assert_eq!(at(&s, lb), c0);
    assert_eq!(line_ends(&s, top), (p2, p3));
    assert_eq!(at(&s, p3), c3);
    // Kein Punkt ist mehr geteilt, kein `Coincident` hängt an der Linie.
    assert!(!s.curves_at_point(p0).contains(&left));
    assert!(!s.curves_at_point(p1).contains(&right));
    assert!(
        !s.constraints()
            .any(|(_, d)| matches!(d.kind, K::Coincident(..)))
    );
    // Das Maß der rechten Linie gehört zur rechten Linie und hält an ihrem neuen Punkt.
    assert_eq!(s.constraint(h).unwrap().kind, K::Distance(ra, p2, 10.0));
    assert!(report.dropped.is_empty(), "{report:?}");
    assert_still(&s);
}

#[test]
fn a_coincident_to_another_line_goes_when_it_no_longer_holds() {
    let mut s = sketch();
    let (a, b) = (pt(&mut s, 0.0, 0.0), pt(&mut s, 10.0, 0.0));
    let (c, d) = (pt(&mut s, 10.0, 0.0), pt(&mut s, 10.0, 10.0));
    let (l1, _) = (s.add_line(a, b).unwrap(), s.add_line(c, d).unwrap());
    let join = s.add_constraint(K::Coincident(b, c)).unwrap();
    let set = only_curves(&s, &[l1]);
    let report = s.apply_transform(&set, &moved(v(0.0, -5.0))).unwrap();
    assert_eq!(report.dropped, vec![join]);
    assert!(!has(&s, join));
    assert_eq!(at(&s, c), v(10.0, 0.0));
    assert_still(&s);
}

#[test]
fn a_distance_to_a_foreign_point_stays_only_while_it_holds() {
    let build = || {
        let mut s = sketch();
        let (a, b) = (pt(&mut s, 10.0, 0.0), pt(&mut s, 20.0, 0.0));
        let x = pt(&mut s, 0.0, 0.0);
        let l = s.add_line(a, b).unwrap();
        let dim = s.add_constraint(K::Distance(a, x, 10.0)).unwrap();
        (s, l, x, dim)
    };
    let (mut s, l, _, dim) = build();
    let set = only_curves(&s, &[l]);
    let report = s.apply_transform(&set, &moved(v(1.0, 1.0))).unwrap();
    assert_eq!(report.dropped, vec![dim]);
    assert_still(&s);

    let (mut s, l, x, dim) = build();
    let set = only_curves(&s, &[l]);
    let t = turned(at(&s, x), 77.0);
    let report = s.apply_transform(&set, &t).unwrap();
    assert!(report.dropped.is_empty(), "{report:?}");
    assert!(has(&s, dim));
    assert_still(&s);

    // Beim Skalieren um den fremden Punkt gilt das Maß nicht mehr, und es ist kein inneres.
    let (mut s, l, x, dim) = build();
    let set = only_curves(&s, &[l]);
    let report = s.apply_transform(&set, &scaled(at(&s, x), 2.0)).unwrap();
    assert_eq!(report.dropped, vec![dim]);
    assert!(report.scaled.is_empty(), "{report:?}");
}

#[test]
fn a_tangent_at_a_split_end_on_a_foreign_arc_goes() {
    let (mut s, l, arc, _) = tangent_arc_on_line();
    let tangent = s
        .constraints()
        .find(|(_, d)| matches!(d.kind, K::Tangent(..)))
        .unwrap()
        .0;
    let CurveKind::Arc { start: old, .. } = s.curve(arc).unwrap().kind else {
        panic!();
    };
    let arc_before = s.curve_polyline(arc, 64).unwrap();
    let set = only_curves(&s, &[l]);
    let report = s.apply_transform(&set, &moved(v(0.0, -4.0))).unwrap();
    assert!(report.dropped.contains(&tangent), "{report:?}");
    assert!(!has(&s, tangent));
    let CurveKind::Arc { start, .. } = s.curve(arc).unwrap().kind else {
        panic!();
    };
    assert_ne!(start, old);
    assert_eq!(s.curve_polyline(arc, 64).unwrap(), arc_before);
    assert_still(&s);
}

#[test]
fn a_tangent_on_a_line_slid_along_itself_still_holds_and_stays() {
    let (mut s, l, _, _) = tangent_arc_on_line();
    let tangent = s
        .constraints()
        .find(|(_, d)| matches!(d.kind, K::Tangent(..)))
        .unwrap()
        .0;
    let set = only_curves(&s, &[l]);
    s.apply_transform(&set, &moved(v(-6.0, 0.0))).unwrap();
    assert!(has(&s, tangent));
    assert_still(&s);
}

#[test]
fn a_foreign_curve_sharing_two_moved_points_keeps_them_together() {
    let mut s = sketch();
    let (a, b, c) = (
        pt(&mut s, 0.0, 0.0),
        pt(&mut s, 10.0, 0.0),
        pt(&mut s, 20.0, 5.0),
    );
    let moved_line = s.add_line(a, b).unwrap();
    let (u, w) = (s.add_line(b, c).unwrap(), s.add_line(c, a).unwrap());
    let third = {
        let d = pt(&mut s, 10.0, -9.0);
        s.add_line(b, d).unwrap()
    };
    let set = only_curves(&s, &[moved_line]);
    s.apply_transform(&set, &moved(v(0.0, 3.0))).unwrap();
    let (ua, _) = line_ends(&s, u);
    let (ta, _) = line_ends(&s, third);
    assert_ne!(ua, b);
    assert_eq!(ua, ta, "die fremden Kurven bleiben untereinander verbunden");
    let (_, wb) = line_ends(&s, w);
    assert_ne!(wb, a);
    assert_eq!(at(&s, ua), v(10.0, 0.0));
}

#[test]
fn a_point_that_does_not_move_is_not_split() {
    let mut s = sketch();
    let (a, b, c) = (
        pt(&mut s, 0.0, 0.0),
        pt(&mut s, 10.0, 0.0),
        pt(&mut s, 10.0, 8.0),
    );
    let (l, m) = (s.add_line(a, b).unwrap(), s.add_line(b, c).unwrap());
    let set = only_curves(&s, &[l]);
    s.apply_transform(&set, &turned(at(&s, b), 40.0)).unwrap();
    assert_eq!(line_ends(&s, m), (b, c));
    assert_eq!(at(&s, b), v(10.0, 0.0));
    assert_still(&s);
}

#[test]
fn a_chosen_point_alone_leaves_its_curve_behind() {
    let mut s = sketch();
    let (a, b) = (pt(&mut s, 0.0, 0.0), pt(&mut s, 10.0, 0.0));
    let l = s.add_line(a, b).unwrap();
    let set = s.transform_set(&[], &[b]);
    s.apply_transform(&set, &moved(v(0.0, 5.0))).unwrap();
    let (la, lb) = line_ends(&s, l);
    assert_eq!(la, a);
    assert_ne!(lb, b);
    assert_eq!(at(&s, lb), v(10.0, 0.0));
    assert_eq!(at(&s, b), v(10.0, 5.0));
}

// ---------------------------------------------------------------------------------------------
// Fest
// ---------------------------------------------------------------------------------------------

fn fixed_line(s: &mut Sketch, a: DVec2, b: DVec2) -> (CurveId, PointId, PointId) {
    let (p, q) = (pt(s, a.x, a.y), pt(s, b.x, b.y));
    let l = s.add_line(p, q).unwrap();
    s.add_constraint(K::Fixed(p, a.x, a.y)).unwrap();
    s.add_constraint(K::Fixed(q, b.x, b.y)).unwrap();
    s.set_construction(l, true).unwrap();
    (l, p, q)
}

#[test]
fn a_fixed_point_in_the_set_blocks_and_nothing_changes() {
    let mut s = sketch();
    let (a, b) = (pt(&mut s, 0.0, 0.0), pt(&mut s, 10.0, 0.0));
    let l = s.add_line(a, b).unwrap();
    s.add_constraint(K::Fixed(b, 10.0, 0.0)).unwrap();
    let set = only_curves(&s, &[l]);
    assert_blocked(
        &mut s,
        &set,
        &moved(v(1.0, 1.0)),
        TransformBlock::FixedInSet(b),
    );
    assert_blocked(
        &mut s,
        &set,
        &scaled(v(0.0, 0.0), 3.0),
        TransformBlock::FixedInSet(b),
    );
    // Eine projizierte Kante in der Auswahl ebenso.
    let mut s = sketch();
    let (edge, p, _) = fixed_line(&mut s, v(0.0, 0.0), v(9.0, 0.0));
    let set = only_curves(&s, &[edge]);
    assert_blocked(
        &mut s,
        &set,
        &moved(v(1.0, 0.0)),
        TransformBlock::FixedInSet(p),
    );
}

#[test]
fn a_fixed_point_at_the_centre_of_a_turn_does_not_move_and_does_not_block() {
    let mut s = sketch();
    let (a, b) = (pt(&mut s, 0.0, 0.0), pt(&mut s, 10.0, 0.0));
    let l = s.add_line(a, b).unwrap();
    s.add_constraint(K::Fixed(a, 0.0, 0.0)).unwrap();
    let set = only_curves(&s, &[l]);
    s.apply_transform(&set, &turned(v(0.0, 0.0), 25.0)).unwrap();
    assert_eq!(at(&s, a), v(0.0, 0.0));
    assert_still(&s);
}

#[test]
fn a_point_on_a_projected_line_may_slide_along_it_but_not_off_it() {
    let build = || {
        let mut s = sketch();
        let (edge, ..) = fixed_line(&mut s, v(-50.0, 0.0), v(50.0, 0.0));
        let (a, b) = (pt(&mut s, 0.0, 0.0), pt(&mut s, 0.0, 10.0));
        let l = s.add_line(a, b).unwrap();
        let on = s.add_constraint(K::PointOnCurve(a, edge)).unwrap();
        (s, l, on)
    };
    let (mut s, l, on) = build();
    let set = only_curves(&s, &[l]);
    assert_blocked(
        &mut s,
        &set,
        &moved(v(0.0, 5.0)),
        TransformBlock::BoundToFixed(on),
    );
    assert_blocked(
        &mut s,
        &set,
        &turned(v(9.0, 9.0), 30.0),
        TransformBlock::BoundToFixed(on),
    );

    let (mut s, l, on) = build();
    let set = only_curves(&s, &[l]);
    let t = moved(v(12.0, 0.0));
    assert_eq!(s.check_transform(&set, &t), Ok(()));
    let report = s.apply_transform(&set, &t).unwrap();
    assert!(report.dropped.is_empty(), "{report:?}");
    assert!(has(&s, on));
    assert_still(&s);
}

#[test]
fn every_position_binding_to_fixed_geometry_blocks() {
    // Deckungsgleich mit einem festen Punkt.
    let mut s = sketch();
    let o = pt(&mut s, 0.0, 0.0);
    s.add_constraint(K::Fixed(o, 0.0, 0.0)).unwrap();
    let (a, b) = (pt(&mut s, 0.0, 0.0), pt(&mut s, 10.0, 0.0));
    let l = s.add_line(a, b).unwrap();
    let c = s.add_constraint(K::Coincident(a, o)).unwrap();
    let set = only_curves(&s, &[l]);
    assert_blocked(
        &mut s,
        &set,
        &moved(v(1.0, 0.0)),
        TransformBlock::BoundToFixed(c),
    );

    // Mitte einer festen Linie.
    let mut s = sketch();
    let (edge, ..) = fixed_line(&mut s, v(-5.0, 0.0), v(5.0, 0.0));
    let (a, b) = (pt(&mut s, 0.0, 0.0), pt(&mut s, 0.0, 7.0));
    let l = s.add_line(a, b).unwrap();
    let c = s.add_constraint(K::Midpoint(a, edge)).unwrap();
    let set = only_curves(&s, &[l]);
    assert_blocked(
        &mut s,
        &set,
        &moved(v(1.0, 0.0)),
        TransformBlock::BoundToFixed(c),
    );

    // Symmetrisch zu einer festen Achse.
    let mut s = sketch();
    let (axis, ..) = fixed_line(&mut s, v(0.0, -9.0), v(0.0, 9.0));
    let (a, b) = (pt(&mut s, -4.0, 1.0), pt(&mut s, 4.0, 1.0));
    let l = s.add_line(a, b).unwrap();
    let c = s.add_constraint(K::Symmetric(a, b, axis)).unwrap();
    let set = only_curves(&s, &[l]);
    assert_blocked(
        &mut s,
        &set,
        &moved(v(1.0, 0.0)),
        TransformBlock::BoundToFixed(c),
    );
    // Entlang der Achse bleibt die Bindung erfüllt.
    s.apply_transform(&set, &moved(v(0.0, 3.0))).unwrap();
    assert!(has(&s, c));
}

#[test]
fn angles_and_distances_to_fixed_geometry_only_go() {
    let mut s = sketch();
    let (edge, p, _) = fixed_line(&mut s, v(0.0, 0.0), v(50.0, 0.0));
    let (a, b) = (pt(&mut s, 0.0, 5.0), pt(&mut s, 10.0, 15.0));
    let l = s.add_line(a, b).unwrap();
    let angle = s.add_constraint(K::Angle(edge, l, 0.25 * PI)).unwrap();
    let height = s
        .add_constraint(K::DistancePointLine(a, edge, 5.0))
        .unwrap();
    let span = s.add_constraint(K::Distance(a, p, 5.0)).unwrap();
    let set = only_curves(&s, &[l]);
    let report = s
        .apply_transform(&set, &turned(v(40.0, 40.0), 20.0))
        .unwrap();
    let mut dropped = report.dropped.clone();
    dropped.sort_by_key(|c| c.index());
    assert_eq!(dropped, vec![angle, height, span]);
    assert_still(&s);
}

#[test]
fn a_fixed_circle_or_ellipse_keeps_its_size_and_axes_about_its_centre() {
    let mut s = sketch();
    let m = pt(&mut s, 3.0, 4.0);
    s.add_constraint(K::Fixed(m, 3.0, 4.0)).unwrap();
    let circle = s.add_circle(m, 8.0).unwrap();
    s.add_constraint(K::Radius(circle, 8.0)).unwrap();
    let set = only_curves(&s, &[circle]);
    let block = TransformBlock::FixedInSet(m);
    assert_blocked(&mut s, &set, &scaled(v(3.0, 4.0), 2.0), block);
    // Um die eigene Mitte gedreht ändert sich nichts.
    let nothing = TransformBlock::NothingToDo;
    assert_blocked(&mut s, &set, &turned(v(3.0, 4.0), 30.0), nothing);

    let mut s = sketch();
    let m = pt(&mut s, 3.0, 4.0);
    s.add_constraint(K::Fixed(m, 3.0, 4.0)).unwrap();
    let ellipse = s.add_ellipse(m, v(9.0, 0.0), 4.0).unwrap();
    s.add_constraint(K::EllipseMajor(ellipse, 9.0)).unwrap();
    s.add_constraint(K::EllipseMinor(ellipse, 4.0)).unwrap();
    s.add_constraint(K::EllipseAngle(ellipse, 0.0)).unwrap();
    let set = only_curves(&s, &[ellipse]);
    let block = TransformBlock::FixedInSet(m);
    assert_blocked(&mut s, &set, &turned(v(3.0, 4.0), 30.0), block.clone());
    assert_blocked(&mut s, &set, &scaled(v(3.0, 4.0), 2.0), block);
}

#[test]
fn a_transform_that_moves_nothing_is_nothing_to_do() {
    let mut s = sketch();
    let p = pt(&mut s, 3.0, 4.0);
    let set = s.transform_set(&[], &[p]);
    let block = TransformBlock::NothingToDo;
    assert_blocked(&mut s, &set, &turned(v(3.0, 4.0), 30.0), block.clone());
    assert_blocked(&mut s, &set, &scaled(v(3.0, 4.0), 2.0), block.clone());

    let mut s = sketch();
    let m = pt(&mut s, 3.0, 4.0);
    let c = s.add_circle(m, 5.0).unwrap();
    let set = only_curves(&s, &[c]);
    assert_blocked(&mut s, &set, &turned(v(3.0, 4.0), 30.0), block);
    // Gestreckt um die Mitte ändert sich der Radius, das zählt.
    s.apply_transform(&set, &scaled(v(3.0, 4.0), 2.0)).unwrap();
    assert_eq!(s.radius(c), Some(10.0));

    // Eine freie Ellipse ändert sich beim Drehen um die Mitte.
    let mut s = sketch();
    let m = pt(&mut s, 0.0, 0.0);
    let e = s.add_ellipse(m, v(9.0, 0.0), 4.0).unwrap();
    let set = only_curves(&s, &[e]);
    s.apply_transform(&set, &turned(v(0.0, 0.0), 90.0)).unwrap();
    assert!(s.ellipse_point(e, 0.0).unwrap().distance(v(0.0, 9.0)) < 1e-12);
}

#[test]
fn no_set_or_an_identity_is_nothing_to_do() {
    let (mut s, r, ..) = dimensioned_rect();
    let set = only_curves(&s, &r.lines);
    let block = TransformBlock::NothingToDo;
    assert_blocked(
        &mut s,
        &TransformSet::default(),
        &moved(v(1.0, 0.0)),
        block.clone(),
    );
    assert_blocked(
        &mut s,
        &set,
        &Similarity::identity(v(3.0, 3.0)),
        block.clone(),
    );
    assert_blocked(&mut s, &set, &turned(v(3.0, 3.0), 360.0), block.clone());
    // Alles Gewählte ist inzwischen gelöscht.
    let gone = TransformSet {
        curves: vec![r.lines[0]],
        points: vec![],
    };
    s.remove_curve(r.lines[0]).unwrap();
    assert_blocked(&mut s, &gone, &moved(v(1.0, 0.0)), block);
}

#[test]
fn a_disabled_or_driven_constraint_stays_but_does_not_block() {
    let mut s = sketch();
    let (edge, ..) = fixed_line(&mut s, v(-50.0, 0.0), v(50.0, 0.0));
    let (a, b) = (pt(&mut s, 0.0, 0.0), pt(&mut s, 0.0, 10.0));
    let x = pt(&mut s, 30.0, 0.0);
    let l = s.add_line(a, b).unwrap();
    let on = s.add_constraint(K::PointOnCurve(a, edge)).unwrap();
    let dim = s.add_constraint(K::Distance(a, x, 30.0)).unwrap();
    s.set_enabled(on, false).unwrap();
    s.set_driven(dim, true).unwrap();
    let set = only_curves(&s, &[l]);
    let report = s.apply_transform(&set, &moved(v(0.0, 7.0))).unwrap();
    assert!(report.dropped.is_empty(), "{report:?}");
    assert!(has(&s, on) && has(&s, dim));
    assert_still(&s);
}

// ---------------------------------------------------------------------------------------------
// Skalieren
// ---------------------------------------------------------------------------------------------

struct Everything {
    s: Sketch,
    /// Längenmaße mit ihrem Wert.
    lengths: Vec<(ConstraintId, f64)>,
    angles: Vec<ConstraintId>,
    circle: CurveId,
    arc: CurveId,
    ellipse: CurveId,
    elliptic_arc: CurveId,
    spline: CurveId,
    spline_points: Vec<PointId>,
}

/// Von jeder Kurve eine, mit jedem Längenmaß und beiden Winkeln.
fn everything() -> Everything {
    let mut s = sketch();
    let (a, b) = (pt(&mut s, 10.0, 0.0), pt(&mut s, 30.0, 0.0));
    let c = pt(&mut s, 30.0, 12.0);
    let (line, side) = (s.add_line(a, b).unwrap(), s.add_line(b, c).unwrap());
    let x = pt(&mut s, 15.0, 10.0);
    let lm = pt(&mut s, 50.0, 20.0);
    let circle = s.add_circle(lm, 6.0).unwrap();
    let lm2 = pt(&mut s, 80.0, 20.0);
    let other = s.add_circle(lm2, 4.0).unwrap();
    let (am, a0, a1) = (
        pt(&mut s, 50.0, 60.0),
        pt(&mut s, 58.0, 60.0),
        pt(&mut s, 50.0, 68.0),
    );
    let arc = s.add_arc(am, a0, a1).unwrap();
    let em = pt(&mut s, -30.0, 10.0);
    let major = DVec2::from_angle(0.4) * 20.0;
    let ellipse = s.add_ellipse(em, major, 8.0).unwrap();
    let eam = pt(&mut s, -30.0, 60.0);
    let (e0, e1) = (pt(&mut s, -10.0, 60.0), pt(&mut s, -30.0, 70.0));
    let elliptic_arc = s.add_elliptic_arc(eam, v(20.0, 0.0), 10.0, e0, e1).unwrap();
    let sp: Vec<PointId> = [(0.0, 90.0), (10.0, 100.0), (25.0, 92.0), (40.0, 105.0)]
        .iter()
        .map(|&(x, y)| pt(&mut s, x, y))
        .collect();
    let spline = s.add_spline(&sp).unwrap();
    let on_spline = {
        let at_half = s.spline_point(spline, 1.5).unwrap();
        pt(&mut s, at_half.x, at_half.y)
    };
    s.add_constraint(K::PointOnCurve(on_spline, spline))
        .unwrap();
    let lengths = vec![
        (s.add_constraint(K::Distance(a, b, 20.0)).unwrap(), 20.0),
        (
            s.add_constraint(K::DistancePointLine(x, line, 10.0))
                .unwrap(),
            10.0,
        ),
        (s.add_constraint(K::Radius(circle, 6.0)).unwrap(), 6.0),
        (s.add_constraint(K::Diameter(other, 8.0)).unwrap(), 8.0),
        (s.add_constraint(K::Radius(arc, 8.0)).unwrap(), 8.0),
        (
            s.add_constraint(K::EllipseMajor(ellipse, 20.0)).unwrap(),
            20.0,
        ),
        (
            s.add_constraint(K::EllipseMinor(ellipse, 8.0)).unwrap(),
            8.0,
        ),
        (
            s.add_constraint(K::EllipseMajor(elliptic_arc, 20.0))
                .unwrap(),
            20.0,
        ),
        (
            s.add_constraint(K::EllipseMinor(elliptic_arc, 10.0))
                .unwrap(),
            10.0,
        ),
    ];
    let angles = vec![
        s.add_constraint(K::Angle(line, side, 0.5 * PI)).unwrap(),
        s.add_constraint(K::EllipseAngle(ellipse, 0.4)).unwrap(),
    ];
    Everything {
        s,
        lengths,
        angles,
        circle,
        arc,
        ellipse,
        elliptic_arc,
        spline,
        spline_points: sp,
    }
}

fn value_of(s: &Sketch, id: ConstraintId) -> f64 {
    match s.constraint(id).unwrap().kind {
        K::Distance(_, _, x)
        | K::DistancePointLine(_, _, x)
        | K::Radius(_, x)
        | K::Diameter(_, x)
        | K::Angle(_, _, x)
        | K::EllipseMajor(_, x)
        | K::EllipseMinor(_, x)
        | K::EllipseAngle(_, x) => x,
        ref k => panic!("kein Maß: {k:?}"),
    }
}

#[test]
fn scaling_by_two_doubles_lengths_and_dimensions_and_keeps_angles() {
    let e = everything();
    let mut s = e.s.clone();
    let set = s.whole_sketch_set();
    let report = s.apply_transform(&set, &scaled(v(0.0, 0.0), 2.0)).unwrap();
    assert!(report.dropped.is_empty(), "{report:?}");
    let want: Vec<ConstraintId> = e.lengths.iter().map(|l| l.0).collect();
    assert_eq!(report.scaled, want);
    for &(id, value) in &e.lengths {
        assert_eq!(value_of(&s, id), value * 2.0);
    }
    for &id in &e.angles {
        assert_eq!(value_of(&s, id), value_of(&e.s, id));
    }
    assert_eq!(s.radius(e.circle), Some(12.0));
    assert_eq!(s.radius(e.arc), Some(16.0));
    assert_still(&s);
    assert_holds(&s, 1e-9);
    let before = e.s.ellipse_point(e.ellipse, 1.0).unwrap();
    assert!(
        s.ellipse_point(e.ellipse, 1.0)
            .unwrap()
            .distance(before * 2.0)
            < 1e-12
    );
    let before = e.s.ellipse_point(e.elliptic_arc, 2.0).unwrap();
    assert!(
        s.ellipse_point(e.elliptic_arc, 2.0)
            .unwrap()
            .distance(before * 2.0)
            < 1e-12
    );
    for t in [0.0, 0.3, 1.5, 2.9] {
        let before = e.s.spline_point(e.spline, t).unwrap();
        assert!(s.spline_point(e.spline, t).unwrap().distance(before * 2.0) < 1e-12);
    }
    assert_eq!(
        at(&s, e.spline_points[1]),
        at(&e.s, e.spline_points[1]) * 2.0
    );
}

#[test]
fn turning_an_ellipse_turns_its_angle_dimension() {
    let e = everything();
    let mut s = e.s.clone();
    let set = s.whole_sketch_set();
    let t = Similarity {
        base: v(3.0, -7.0),
        offset: v(11.0, 13.0),
        angle: 0.7,
        factor: 1.5,
    };
    s.apply_transform(&set, &t).unwrap();
    let angle = e.angles[1];
    assert!((value_of(&s, angle) - (0.4 + 0.7)).abs() < 1e-15);
    assert_holds(&s, 1e-9);
    assert_still(&s);
    let (before, after) = (
        e.s.ellipse_point(e.elliptic_arc, 0.9).unwrap(),
        s.ellipse_point(e.elliptic_arc, 0.9).unwrap(),
    );
    assert!(after.distance(t.apply(before)) < 1e-12);
}

#[test]
fn scaling_stays_exact_over_a_dense_range_of_factors() {
    let e = everything();
    let set = e.s.whole_sketch_set();
    let base = v(4.0, -2.0);
    for step in 0..=400 {
        let k = 10f64.powf(-2.0 + 4.0 * f64::from(step) / 400.0);
        let t = scaled(base, k);
        if t.is_identity(TOL) {
            continue;
        }
        let mut s = e.s.clone();
        let report = s.apply_transform(&set, &t).unwrap();
        assert_eq!(report.scaled.len(), e.lengths.len(), "k {k}");
        assert!(report.dropped.is_empty(), "k {k}: {report:?}");
        let limit = 1e-6 * k;
        for &p in &set.points {
            assert!(at(&s, p).distance(t.apply(at(&e.s, p))) <= limit, "k {k}");
        }
        for &(id, value) in &e.lengths {
            assert!((value_of(&s, id) - value * k).abs() <= limit, "k {k}");
            let kind = s.constraint(id).unwrap().kind.clone();
            assert!(
                (s.measure(&kind).unwrap() - value * k).abs() <= limit,
                "k {k}: {kind:?}"
            );
        }
        assert_holds(&s, limit);
        assert_still(&s);
        assert!(!folded_over(&e.s, &s), "k {k}");
    }
}

/// Innere Lage-Bindungen und die Tangente an Ellipse und Spline (mit Kurvenparameter als Zusatz-
/// variable) folgen ohne Solver, beim Kopieren ebenso.
#[test]
fn inner_position_bindings_and_parametric_tangents_stay_exact() {
    let mut s = sketch();
    let (a, b) = (pt(&mut s, 0.0, 0.0), pt(&mut s, 20.0, 0.0));
    let axis = s.add_line(a, b).unwrap();
    let mid = pt(&mut s, 10.0, 0.0);
    s.add_constraint(K::Midpoint(mid, axis)).unwrap();
    let (u, w) = (pt(&mut s, 5.0, 4.0), pt(&mut s, 5.0, -4.0));
    s.add_constraint(K::Symmetric(u, w, axis)).unwrap();
    let on = pt(&mut s, 12.0, 0.0);
    s.add_constraint(K::PointOnCurve(on, axis)).unwrap();
    let joined = pt(&mut s, 20.0, 0.0);
    s.add_constraint(K::Coincident(joined, b)).unwrap();
    // Linie tangential an die Ellipse, Linie tangential an den Spline.
    let m = pt(&mut s, 60.0, 0.0);
    let ellipse = s.add_ellipse(m, v(12.0, 0.0), 5.0).unwrap();
    let (e0, e1) = (pt(&mut s, 50.0, 5.0), pt(&mut s, 70.0, 5.0));
    let on_ellipse = s.add_line(e0, e1).unwrap();
    s.add_constraint(K::Tangent(on_ellipse, ellipse)).unwrap();
    let sp: Vec<PointId> = [(0.0, 40.0), (10.0, 50.0), (20.0, 40.0)]
        .iter()
        .map(|&(x, y)| pt(&mut s, x, y))
        .collect();
    let spline = s.add_spline(&sp).unwrap();
    let (s0, s1) = (pt(&mut s, 0.0, 50.0), pt(&mut s, 20.0, 50.0));
    let on_spline = s.add_line(s0, s1).unwrap();
    s.add_constraint(K::Tangent(on_spline, spline)).unwrap();
    assert_solved(&s.solve());

    let count = s.constraints().count();
    let set = s.whole_sketch_set();
    let t = Similarity {
        base: v(3.0, 8.0),
        offset: v(-40.0, 90.0),
        angle: 1.1,
        factor: 1.7,
    };
    let mut copied = s.clone();
    let map = copied.copy_transformed(&set, &t).unwrap();
    assert_eq!(map.constraints.len(), count);
    // Die Stelle auf der Kurve wird übernommen, nicht neu gesucht.
    for (&from, &to) in &map.constraints {
        let extra = |id: ConstraintId| copied.constraint(id).unwrap().extra();
        if let (Some(a), Some(b)) = (extra(from), extra(to)) {
            assert_eq!(copied.variables()[a], copied.variables()[b]);
        }
    }
    assert_holds(&copied, 1e-7);
    assert_still(&copied);
    s.apply_transform(&set, &t).unwrap();
    assert_eq!(s.constraints().count(), count);
    assert_holds(&s, 1e-7);
    assert_still(&s);
}

#[test]
fn a_point_may_slide_along_a_foreign_spline_because_its_place_is_searched_anew() {
    let mut s = sketch();
    let sp: Vec<PointId> = [(0.0, 0.0), (10.0, 0.0), (20.0, 0.0), (30.0, 0.0)]
        .iter()
        .map(|&(x, y)| pt(&mut s, x, y))
        .collect();
    s.add_spline(&sp).unwrap();
    let spline = s.curves().next().unwrap().0;
    let on = pt(&mut s, 5.0, 0.0);
    let bind = s.add_constraint(K::PointOnCurve(on, spline)).unwrap();
    let set = s.transform_set(&[], &[on]);
    let report = s.apply_transform(&set, &moved(v(12.0, 0.0))).unwrap();
    assert!(report.dropped.is_empty(), "{report:?}");
    assert!(has(&s, bind));
    assert_still(&s);
}

#[test]
fn a_bad_factor_is_degenerate_and_changes_nothing() {
    let (mut s, r, ..) = dimensioned_rect();
    let set = only_curves(&s, &r.lines);
    for k in [
        0.0,
        -1.0,
        -0.001,
        f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
    ] {
        assert_blocked(
            &mut s,
            &set,
            &scaled(v(0.0, 0.0), k),
            TransformBlock::Degenerate,
        );
    }
    for t in [
        moved(v(f64::NAN, 0.0)),
        moved(v(0.0, f64::INFINITY)),
        turned(v(0.0, 0.0), f64::NAN),
        scaled(v(f64::NAN, 0.0), 2.0),
    ] {
        assert_blocked(&mut s, &set, &t, TransformBlock::Degenerate);
    }
    // Kreis und Maße fielen unter die Toleranz, Koordinaten über die Grenze.
    assert_blocked(
        &mut s,
        &set,
        &scaled(v(0.0, 0.0), 1e-9),
        TransformBlock::Degenerate,
    );
    assert_blocked(
        &mut s,
        &set,
        &moved(v(2e9, 0.0)),
        TransformBlock::Degenerate,
    );
    let mut s = sketch();
    let m = pt(&mut s, 1.0, 1.0);
    let c = s.add_circle(m, 5.0).unwrap();
    let set = only_curves(&s, &[c]);
    assert_blocked(
        &mut s,
        &set,
        &scaled(v(1.0, 1.0), 1e-8),
        TransformBlock::Degenerate,
    );
    assert_blocked(
        &mut s,
        &set,
        &scaled(v(1.0, 1.0), 1e9),
        TransformBlock::Degenerate,
    );
}

#[test]
fn a_foreign_reference_is_not_scaled_and_goes_or_stays_by_the_rule() {
    let mut s = sketch();
    let (a, b) = (pt(&mut s, 0.0, 0.0), pt(&mut s, 10.0, 0.0));
    let x = pt(&mut s, 0.0, 9.0);
    let (foreign_a, foreign_b) = (pt(&mut s, -5.0, -5.0), pt(&mut s, -5.0, 5.0));
    let foreign = s.add_line(foreign_a, foreign_b).unwrap();
    let l = s.add_line(a, b).unwrap();
    let stays = s.add_constraint(K::Distance(a, x, 9.0)).unwrap();
    let goes = s
        .add_constraint(K::Distance(b, x, x_len(&s, b, x)))
        .unwrap();
    let height = s
        .add_constraint(K::DistancePointLine(b, foreign, 15.0))
        .unwrap();
    let equal = s.add_constraint(K::Equal(l, foreign)).unwrap();
    let inner = s.add_constraint(K::Distance(a, b, 10.0)).unwrap();
    let set = only_curves(&s, &[l]);
    let report = s.apply_transform(&set, &scaled(v(0.0, 0.0), 2.0)).unwrap();
    assert_eq!(report.scaled, vec![inner]);
    assert_eq!(report.dropped, vec![goes, height, equal]);
    assert_eq!(
        value_of(&s, stays),
        9.0,
        "der Bezugspunkt bewegt sich nicht"
    );
    assert_eq!(value_of(&s, inner), 20.0);
    assert_still(&s);
}

fn x_len(s: &Sketch, a: PointId, b: PointId) -> f64 {
    at(s, a).distance(at(s, b))
}

// ---------------------------------------------------------------------------------------------
// Ganze Skizze
// ---------------------------------------------------------------------------------------------

/// Fester Ursprung, Achshilfslinie, Kantenkopien (Linie, Kreis, Ellipse), Rechteck mit Ecke im
/// Ursprung, freier Punkt.
struct Whole {
    s: Sketch,
    origin: PointId,
    axis: CurveId,
    edges: Vec<CurveId>,
    rect: RectIds,
    free: PointId,
    corner_bind: ConstraintId,
}

fn whole() -> Whole {
    let mut s = sketch();
    let origin = pt(&mut s, 0.0, 0.0);
    s.add_constraint(K::Fixed(origin, 0.0, 0.0)).unwrap();
    let (axis, ..) = fixed_line(&mut s, v(-100.0, 0.0), v(100.0, 0.0));
    let (edge, ..) = fixed_line(&mut s, v(0.0, -50.0), v(40.0, -50.0));
    let m = pt(&mut s, 70.0, 70.0);
    s.add_constraint(K::Fixed(m, 70.0, 70.0)).unwrap();
    let circle = s.add_circle(m, 8.0).unwrap();
    s.add_constraint(K::Radius(circle, 8.0)).unwrap();
    let em = pt(&mut s, -70.0, 70.0);
    s.add_constraint(K::Fixed(em, -70.0, 70.0)).unwrap();
    let ellipse = s.add_ellipse(em, v(9.0, 0.0), 4.0).unwrap();
    s.add_constraint(K::EllipseMajor(ellipse, 9.0)).unwrap();
    s.add_constraint(K::EllipseMinor(ellipse, 4.0)).unwrap();
    s.add_constraint(K::EllipseAngle(ellipse, 0.0)).unwrap();
    let rect = s.add_rectangle(v(0.0, 0.0), v(20.0, 10.0)).unwrap();
    let corner_bind = s
        .add_constraint(K::Coincident(rect.points[0], origin))
        .unwrap();
    let free = pt(&mut s, 33.0, 44.0);
    Whole {
        s,
        origin,
        axis,
        edges: vec![edge, circle, ellipse],
        rect,
        free,
        corner_bind,
    }
}

#[test]
fn the_whole_sketch_leaves_out_fixed_geometry() {
    let w = whole();
    let set = w.s.whole_sketch_set();
    assert_eq!(set.curves, w.rect.lines.to_vec());
    assert!(!set.curves.contains(&w.axis));
    assert!(w.edges.iter().all(|e| !set.curves.contains(e)));
    assert!(!set.points.contains(&w.origin));
    assert!(set.points.contains(&w.free));
    assert_eq!(set.points.len(), 5);
}

#[test]
fn the_whole_sketch_keeps_a_curve_with_a_fixed_centre_but_free_size() {
    let mut s = sketch();
    let m = pt(&mut s, 1.0, 1.0);
    s.add_constraint(K::Fixed(m, 1.0, 1.0)).unwrap();
    let c = s.add_circle(m, 8.0).unwrap();
    assert_eq!(s.whole_sketch_set().curves, vec![c]);
}

#[test]
fn the_whole_sketch_scales_about_the_origin_with_a_corner_bound_there() {
    let mut w = whole();
    let set = w.s.whole_sketch_set();
    let report =
        w.s.apply_transform(&set, &scaled(v(0.0, 0.0), 2.0))
            .unwrap();
    assert!(report.dropped.is_empty(), "{report:?}");
    assert!(has(&w.s, w.corner_bind));
    assert_eq!(at(&w.s, w.rect.points[2]), v(40.0, 20.0));
    assert_eq!(at(&w.s, w.origin), v(0.0, 0.0));
    assert_eq!(at(&w.s, w.free), v(66.0, 88.0));
    assert_eq!(w.s.radius(w.edges[1]), Some(8.0));
    assert_still(&w.s);
}

#[test]
fn the_whole_sketch_is_blocked_at_another_base() {
    let mut w = whole();
    let set = w.s.whole_sketch_set();
    let block = TransformBlock::BoundToFixed(w.corner_bind);
    assert_blocked(&mut w.s, &set, &scaled(v(5.0, 5.0), 2.0), block);
}

#[test]
fn a_dimension_to_an_edge_goes_when_the_whole_sketch_scales() {
    let mut w = whole();
    let dim =
        w.s.add_constraint(K::DistancePointLine(w.free, w.axis, 44.0))
            .unwrap();
    let set = w.s.whole_sketch_set();
    let report =
        w.s.apply_transform(&set, &scaled(v(0.0, 0.0), 2.0))
            .unwrap();
    assert_eq!(report.dropped, vec![dim]);
    assert!(report.scaled.is_empty(), "{report:?}");
    assert_still(&w.s);
}

// ---------------------------------------------------------------------------------------------
// Kopieren
// ---------------------------------------------------------------------------------------------

fn map_kind(kind: &K, m: &CopyMap) -> K {
    let (p, c) = (&m.points, &m.curves);
    match kind {
        K::Coincident(a, b) => K::Coincident(p[a], p[b]),
        K::PointOnCurve(a, l) => K::PointOnCurve(p[a], c[l]),
        K::Horizontal(LineOrPoints::Line(l)) => K::Horizontal(LineOrPoints::Line(c[l])),
        K::Vertical(LineOrPoints::Line(l)) => K::Vertical(LineOrPoints::Line(c[l])),
        K::Parallel(a, b) => K::Parallel(c[a], c[b]),
        K::Perpendicular(a, b) => K::Perpendicular(c[a], c[b]),
        K::Tangent(a, b) => K::Tangent(c[a], c[b]),
        K::Equal(a, b) => K::Equal(c[a], c[b]),
        K::Distance(a, b, x) => K::Distance(p[a], p[b], *x),
        K::DistancePointLine(a, l, x) => K::DistancePointLine(p[a], c[l], *x),
        K::Radius(a, x) => K::Radius(c[a], *x),
        K::Diameter(a, x) => K::Diameter(c[a], *x),
        K::Angle(a, b, x) => K::Angle(c[a], c[b], *x),
        K::EllipseMajor(a, x) => K::EllipseMajor(c[a], *x),
        K::EllipseMinor(a, x) => K::EllipseMinor(c[a], *x),
        K::EllipseAngle(a, x) => K::EllipseAngle(c[a], *x),
        k => panic!("nicht erwartet: {k:?}"),
    }
}

/// Rechteck mit Maßen, Bogen, Ellipse, Ellipsenbogen, Spline, Mittellinie und Konstruktionskreis.
fn to_copy() -> (
    Sketch,
    RectIds,
    ConstraintId,
    ConstraintId,
    CurveId,
    CurveId,
) {
    let (mut s, r, w, h) = dimensioned_rect();
    s.set_driven(h, true).unwrap();
    let (am, a0, a1) = (
        pt(&mut s, 50.0, 0.0),
        pt(&mut s, 58.0, 0.0),
        pt(&mut s, 50.0, 8.0),
    );
    let arc = s.add_arc(am, a0, a1).unwrap();
    s.add_constraint(K::Radius(arc, 8.0)).unwrap();
    let em = pt(&mut s, 90.0, 0.0);
    let ellipse = s.add_ellipse(em, v(12.0, 0.0), 6.0).unwrap();
    s.add_constraint(K::EllipseMajor(ellipse, 12.0)).unwrap();
    s.add_constraint(K::EllipseMinor(ellipse, 6.0)).unwrap();
    s.add_constraint(K::EllipseAngle(ellipse, 0.0)).unwrap();
    let eam = pt(&mut s, 90.0, 40.0);
    let (e0, e1) = (pt(&mut s, 102.0, 40.0), pt(&mut s, 90.0, 46.0));
    s.add_elliptic_arc(eam, v(12.0, 0.0), 6.0, e0, e1).unwrap();
    let sp: Vec<PointId> = [(0.0, 40.0), (10.0, 50.0), (25.0, 42.0)]
        .iter()
        .map(|&(x, y)| pt(&mut s, x, y))
        .collect();
    let spline = s.add_spline(&sp).unwrap();
    let on = {
        let q = s.spline_point(spline, 0.5).unwrap();
        pt(&mut s, q.x, q.y)
    };
    s.add_constraint(K::PointOnCurve(on, spline)).unwrap();
    let (ca, cb) = (pt(&mut s, -10.0, -10.0), pt(&mut s, -10.0, 30.0));
    let center = s.add_line(ca, cb).unwrap();
    s.set_centerline(center, true).unwrap();
    let dead = s.add_constraint(K::Distance(ca, cb, 40.0)).unwrap();
    s.set_enabled(dead, false).unwrap();
    let cm = pt(&mut s, -30.0, -30.0);
    let helper = s.add_circle(cm, 3.0).unwrap();
    s.set_construction(helper, true).unwrap();
    (s, r, w, dead, center, helper)
}

#[test]
fn a_copy_takes_its_inner_constraints_and_styles_and_leaves_the_original() {
    let (mut s, _, w, dead, center, helper) = to_copy();
    let original = s.clone();
    let set = s.whole_sketch_set();
    let t = moved(v(200.0, -30.0));
    let map = s.copy_transformed(&set, &t).unwrap();
    // Das Original bleibt bitgleich.
    assert_eq!(
        &s.variables()[..original.variables().len()],
        original.variables()
    );
    for (id, d) in original.constraints() {
        assert_eq!(s.constraint(id).unwrap().kind, d.kind);
    }
    assert_eq!(map.points.len(), set.points.len());
    assert_eq!(map.curves.len(), set.curves.len());
    for (&p, &q) in &map.points {
        assert_ne!(p, q);
        assert_eq!(at(&s, q), t.apply(at(&s, p)));
    }
    for (&c, &n) in &map.curves {
        let (a, b) = (s.curve(c).unwrap(), s.curve(n).unwrap());
        assert_eq!(a.style(), b.style());
        assert_eq!(a.kind.points().len(), b.kind.points().len());
        for (p, q) in a.kind.points().iter().zip(b.kind.points()) {
            assert_eq!(map.points[p], q);
        }
    }
    assert_eq!(map.constraints.len(), original.constraints().count());
    for (id, d) in original.constraints() {
        let copy = s.constraint(map.constraints[&id]).unwrap();
        assert_eq!(copy.kind, map_kind(&d.kind, &map), "{:?}", d.kind);
        assert_eq!(copy.driven, d.driven);
        assert_eq!(copy.enabled, d.enabled);
    }
    assert!(s.constraint(map.constraints[&dead]).is_some());
    assert!(s.curve(map.curves[&center]).unwrap().centerline);
    assert!(s.curve(map.curves[&helper]).unwrap().construction);
    let r = assert_still(&s);
    assert!(r.redundant.is_empty(), "{r:?}");
    assert_holds(&s, 1e-9);

    // Ein Maß an der Kopie ändern lässt das Original stehen.
    let width = map.constraints[&w];
    s.set_constraint_value(width, 33.0).unwrap();
    let r = s.solve();
    assert_solved(&r);
    assert!(r.redundant.is_empty(), "{r:?}");
    assert_eq!(
        &s.variables()[..original.variables().len()],
        original.variables()
    );
    let CurveKind::Line { a, b } = s.curve(map.curves[&set.curves[0]]).unwrap().kind else {
        panic!();
    };
    assert!((at(&s, a).distance(at(&s, b)) - 33.0).abs() < 1e-6);
}

#[test]
fn a_fixed_constraint_does_not_come_along() {
    let mut s = sketch();
    let (a, b) = (pt(&mut s, 0.0, 0.0), pt(&mut s, 10.0, 0.0));
    let l = s.add_line(a, b).unwrap();
    let fixed = s.add_constraint(K::Fixed(a, 0.0, 0.0)).unwrap();
    let inner = s.add_constraint(K::Distance(a, b, 10.0)).unwrap();
    let set = only_curves(&s, &[l]);
    let map = s.copy_transformed(&set, &moved(v(0.0, 5.0))).unwrap();
    assert!(!map.constraints.contains_key(&fixed));
    assert!(map.constraints.contains_key(&inner));
    assert_eq!(s.constraints().count(), 3);
    assert_still(&s);
    // Die Kopie ist frei: Ziehen am kopierten Punkt geht, das Original bleibt.
    let free = map.points[&a];
    assert!(
        !s.constraints()
            .any(|(_, d)| matches!(d.kind, K::Fixed(p, ..) if p == free))
    );
}

#[test]
fn an_outer_constraint_does_not_come_along() {
    let mut s = sketch();
    let (a, b) = (pt(&mut s, 0.0, 0.0), pt(&mut s, 10.0, 0.0));
    let x = pt(&mut s, 5.0, 5.0);
    let l = s.add_line(a, b).unwrap();
    s.add_constraint(K::Distance(a, x, x_len(&s, a, x)))
        .unwrap();
    let set = only_curves(&s, &[l]);
    let map = s.copy_transformed(&set, &moved(v(0.0, 5.0))).unwrap();
    assert!(map.constraints.is_empty());
    assert_still(&s);
}

#[test]
fn a_point_alone_can_be_copied() {
    let mut s = sketch();
    let p = pt(&mut s, 3.0, 4.0);
    let set = s.transform_set(&[], &[p]);
    let map = s.copy_transformed(&set, &moved(v(1.0, 1.0))).unwrap();
    assert_eq!(map.points.len(), 1);
    assert!(map.curves.is_empty());
    assert_eq!(at(&s, map.points[&p]), v(4.0, 5.0));
    assert_eq!(at(&s, p), v(3.0, 4.0));
}

#[test]
fn a_shared_point_stays_shared_in_the_copy() {
    let mut s = sketch();
    let (a, b, c) = (
        pt(&mut s, 0.0, 0.0),
        pt(&mut s, 10.0, 0.0),
        pt(&mut s, 10.0, 8.0),
    );
    let (l, m) = (s.add_line(a, b).unwrap(), s.add_line(b, c).unwrap());
    let set = only_curves(&s, &[l, m]);
    let map = s.copy_transformed(&set, &moved(v(0.0, 20.0))).unwrap();
    let (_, end) = line_ends(&s, map.curves[&l]);
    let (start, _) = line_ends(&s, map.curves[&m]);
    assert_eq!(end, start);
    assert_eq!(end, map.points[&b]);
    assert_eq!(map.points.len(), 3);
}

#[test]
fn a_turned_copy_has_horizontal_and_vertical_only_when_they_hold() {
    let (mut s, r, ..) = dimensioned_rect();
    let set = only_curves(&s, &r.lines);
    let map = s
        .copy_transformed(&set, &turned(v(5.0, 5.0), 30.0))
        .unwrap();
    let copied: Vec<&K> = map
        .constraints
        .values()
        .map(|&c| &s.constraint(c).unwrap().kind)
        .collect();
    assert_eq!(copied.len(), 2, "{copied:?}");
    assert!(copied.iter().all(|k| matches!(k, K::Distance(..))));
    assert_still(&s);

    let (mut s, r, ..) = dimensioned_rect();
    let set = only_curves(&s, &r.lines);
    let map = s
        .copy_transformed(&set, &turned(v(5.0, 5.0), 180.0))
        .unwrap();
    assert_eq!(map.constraints.len(), 6);
    assert_still(&s);
}

#[test]
fn a_scaled_and_turned_copy_scales_lengths_and_turns_the_ellipse_angle() {
    let e = everything();
    let mut s = e.s.clone();
    let original = s.clone();
    let set = s.whole_sketch_set();
    let t = Similarity {
        base: v(1.0, 2.0),
        offset: v(300.0, 0.0),
        angle: 0.25,
        factor: 3.0,
    };
    let map = s.copy_transformed(&set, &t).unwrap();
    assert_eq!(
        &s.variables()[..original.variables().len()],
        original.variables()
    );
    for &(id, value) in &e.lengths {
        let copy = map.constraints[&id];
        assert!((value_of(&s, copy) - value * 3.0).abs() < 1e-12);
        assert_eq!(value_of(&s, id), value);
    }
    assert!((value_of(&s, map.constraints[&e.angles[1]]) - 0.65).abs() < 1e-15);
    assert_eq!(value_of(&s, map.constraints[&e.angles[0]]), 0.5 * PI);
    let q = s.ellipse_point(map.curves[&e.elliptic_arc], 1.1).unwrap();
    assert!(q.distance(t.apply(e.s.ellipse_point(e.elliptic_arc, 1.1).unwrap())) < 1e-12);
    let q = s.spline_point(map.curves[&e.spline], 1.7).unwrap();
    assert!(q.distance(t.apply(e.s.spline_point(e.spline, 1.7).unwrap())) < 1e-12);
    assert_holds(&s, 1e-9);
    assert_still(&s);
}

#[test]
fn a_bad_copy_is_refused_and_leaves_no_trace() {
    let (mut s, r, ..) = dimensioned_rect();
    let before = s.clone();
    let set = only_curves(&s, &r.lines);
    for k in [0.0, -2.0, f64::NAN, 1e-9] {
        assert!(
            matches!(
                s.copy_transformed(&set, &scaled(v(0.0, 0.0), k)),
                Err(SketchError::Degenerate(_))
            ),
            "k {k}"
        );
        assert_untouched(&s, &before);
    }
    assert!(matches!(
        s.copy_transformed(&set, &moved(v(f64::NAN, 0.0))),
        Err(SketchError::Degenerate(_))
    ));
    assert_untouched(&s, &before);
    assert!(matches!(
        s.copy_transformed(&TransformSet::default(), &moved(v(1.0, 0.0))),
        Err(SketchError::InvalidInput(_))
    ));
    assert_untouched(&s, &before);
}

/// Bricht eine Ellipse in zwei Bögen, die dieselben Halbachsen-Variablen teilen.
fn broken_ellipse() -> (Sketch, CurveId, CurveId) {
    let mut s = sketch();
    let m = pt(&mut s, 0.0, 0.0);
    let e = s.add_ellipse(m, v(20.0, 0.0), 10.0).unwrap();
    let (a, b) = (pt(&mut s, 0.0, -30.0), pt(&mut s, 0.0, 30.0));
    let (c, d) = (pt(&mut s, -30.0, 0.0), pt(&mut s, 30.0, 0.0));
    s.add_line(a, b).unwrap();
    s.add_line(c, d).unwrap();
    let parts = s
        .break_curve(e, v(20.0, 0.0).rotate(DVec2::from_angle(0.3)))
        .unwrap();
    let other = parts[1..].iter().find(|&&p| p != e).copied().unwrap();
    (s, e, other)
}

fn axis_vars(s: &Sketch, c: CurveId) -> (usize, usize, usize) {
    match s.curve(c).unwrap().kind {
        CurveKind::EllipticArc { ax, ay, minor, .. } => (ax, ay, minor),
        ref k => panic!("kein Ellipsenbogen: {k:?}"),
    }
}

#[test]
fn moving_one_elliptic_arc_leaves_its_sibling_on_its_own_axes() {
    let (mut s, e, other) = broken_ellipse();
    assert_eq!(
        axis_vars(&s, e),
        axis_vars(&s, other),
        "die Bögen teilen die Halbachsen"
    );
    let sibling = s.curve_polyline(other, 64).unwrap();
    let mine = s.curve_polyline(e, 64).unwrap();
    let set = only_curves(&s, &[e]);
    let t = Similarity {
        base: v(1.0, 1.0),
        offset: v(100.0, 0.0),
        angle: 0.5,
        factor: 2.0,
    };
    s.apply_transform(&set, &t).unwrap();
    assert_ne!(axis_vars(&s, e), axis_vars(&s, other));
    let still = s.curve_polyline(other, 64).unwrap();
    assert_eq!(still.len(), sibling.len());
    for (a, b) in still.iter().zip(&sibling) {
        assert!(a.distance(*b) < 1e-9);
    }
    let now = s.curve_polyline(e, 64).unwrap();
    assert_eq!(now.len(), mine.len());
    for (a, b) in now.iter().zip(&mine) {
        assert!(a.distance(t.apply(*b)) < 1e-9);
    }
    assert_holds(&s, 1e-9);
}

#[test]
fn both_elliptic_arcs_turn_their_shared_axes_once_and_share_them_in_a_copy() {
    let (mut s, e, other) = broken_ellipse();
    let before = s.clone();
    let set = only_curves(&s, &[e, other]);
    let t = Similarity {
        base: v(0.0, 0.0),
        offset: v(0.0, 0.0),
        angle: 0.5,
        factor: 2.0,
    };
    let map = s.copy_transformed(&set, &t).unwrap();
    assert_eq!(
        axis_vars(&s, map.curves[&e]),
        axis_vars(&s, map.curves[&other])
    );
    s.apply_transform(&set, &t).unwrap();
    assert_eq!(axis_vars(&s, e), axis_vars(&s, other));
    let q = s.ellipse_point(e, 1.0).unwrap();
    assert!(q.distance(t.apply(before.ellipse_point(e, 1.0).unwrap())) < 1e-12);
    assert_holds(&s, 1e-9);
}

// ---------------------------------------------------------------------------------------------
// Vorschau gleich Ergebnis
// ---------------------------------------------------------------------------------------------

#[test]
fn the_preview_equals_the_result_over_a_dense_sweep_and_touches_nothing() {
    let e = everything();
    let set = e.s.whole_sketch_set();
    for step in 0..=120 {
        let u = f64::from(step) / 120.0;
        let t = Similarity {
            base: v(5.0, -5.0),
            offset: v(300.0 * u, -120.0 * u),
            angle: u * 2.0 * PI + 0.01,
            factor: 0.25 + 4.0 * u,
        };
        let revision = e.s.revision();
        let preview = e.s.transform_preview(&set, &t, 64);
        assert_eq!(e.s.revision(), revision);
        let mut after = e.s.clone();
        after.apply_transform(&set, &t).unwrap();
        assert_sides_kept(&e.s, &after, &t);
        assert_eq!(preview.len(), set.curves.len());
        for (pl, &c) in preview.iter().zip(&set.curves) {
            let want = after.curve_polyline(c, 64).unwrap();
            assert_eq!(pl.len(), want.len());
            for (a, b) in pl.iter().zip(&want) {
                assert!(a.distance(*b) < 1e-12, "{t:?}");
            }
        }
    }
}

#[test]
fn check_and_apply_agree_and_a_good_check_changes_nothing() {
    let (mut s, r, ..) = dimensioned_rect();
    let before = s.clone();
    let set = only_curves(&s, &r.lines);
    assert_eq!(s.check_transform(&set, &moved(v(1.0, 1.0))), Ok(()));
    assert_untouched(&s, &before);
    s.apply_transform(&set, &moved(v(1.0, 1.0))).unwrap();
    assert_ne!(s.revision(), before.revision());
}

// ---------------------------------------------------------------------------------------------
// Sichtbare Auswahl: Hilfsgeometrie zieht mit
// ---------------------------------------------------------------------------------------------

fn is_length(kind: &K) -> bool {
    matches!(
        kind,
        K::Distance(..)
            | K::DistancePointLine(..)
            | K::Radius(..)
            | K::Diameter(..)
            | K::EllipseMajor(..)
            | K::EllipseMinor(..)
    )
}

fn the_three_motions() -> [Similarity; 3] {
    [
        moved(v(100.0, -7.0)),
        turned(v(-3.0, 11.0), 35.0),
        scaled(v(-3.0, 11.0), 2.0),
    ]
}

/// Bewegt, was der Nutzer sieht (`picked`), und zwar die ganze Skizze: Alles geht mit, nichts bleibt
/// an der alten Lage, kein neuer Punkt, Maße mitskaliert, und `dropped` nennt nur H und V beim
/// Drehen. Danach geht das Maß `dim` noch und klappt nichts um. Gibt je Bewegung die Skizze zurück.
fn assert_whole_shape_follows(
    name: &str,
    s: &Sketch,
    picked: &[CurveId],
    dim: ConstraintId,
) -> Vec<(Similarity, Sketch)> {
    let solved = s.clone().solve();
    assert_solved(&solved);
    assert!(solved.redundant.is_empty(), "{name}: {solved:?}");
    let mut out = Vec::new();
    for t in the_three_motions() {
        let label = format!("{name} {t:?}");
        let set = s.transform_set(picked, &[]);
        assert_eq!(set.curves.len(), s.curves().count(), "{label}");
        assert_eq!(set.points.len(), s.points().count(), "{label}");

        let preview = s.transform_preview(&set, &t, 64);
        let mut after = s.clone();
        let report = after
            .apply_transform(&set, &t)
            .unwrap_or_else(|b| panic!("{label}: {b:?}"));
        for (pl, &c) in preview.iter().zip(&set.curves) {
            assert_eq!(pl, &after.curve_polyline(c, 64).unwrap(), "{label}");
        }
        assert_eq!(after.points().count(), s.points().count(), "{label}");
        for (p, _) in s.points() {
            assert!(at(&after, p).distance(t.apply(at(s, p))) <= 1e-9, "{label}");
        }
        for (c, d) in s.curves() {
            assert_eq!(after.curve(c).unwrap().kind, d.kind, "{label}");
        }
        let hv: Vec<ConstraintId> = s
            .constraints()
            .filter(|(_, d)| matches!(d.kind, K::Horizontal(_) | K::Vertical(_)))
            .map(|(id, _)| id)
            .collect();
        let mut dropped = report.dropped.clone();
        dropped.sort_by_key(|c| c.index());
        if t.angle != 0.0 {
            assert_eq!(dropped, hv, "{label}");
        } else {
            assert!(dropped.is_empty(), "{label}: {report:?}");
        }
        let lengths: Vec<ConstraintId> = s
            .constraints()
            .filter(|(_, d)| is_length(&d.kind))
            .map(|(id, _)| id)
            .collect();
        if t.factor != 1.0 {
            assert_eq!(report.scaled, lengths, "{label}");
        } else {
            assert!(report.scaled.is_empty(), "{label}");
        }
        for id in lengths {
            let want = value_of(s, id) * t.factor;
            assert!((value_of(&after, id) - want).abs() < 1e-9, "{label}");
            let kind = after.constraint(id).unwrap().kind.clone();
            assert!(
                (after.measure(&kind).unwrap() - want).abs() < 1e-8,
                "{label}: {kind:?}"
            );
        }
        assert_holds(&after, 1e-9 * t.factor);
        assert_sides_kept(s, &after, &t);
        let r = assert_still(&after);
        assert!(r.redundant.is_empty(), "{label}: {r:?}");

        let old = value_of(&after, dim);
        let new = old * 1.2;
        let changed = change(&after, dim, old, new).unwrap_or_else(|| panic!("{label}: abgelehnt"));
        let kind = changed.constraint(dim).unwrap().kind.clone();
        assert!(
            (changed.measure(&kind).unwrap() - new).abs() < 1e-6,
            "{label}"
        );
        assert_holds(&changed, 1e-6 * t.factor);
        assert!(!folded_over(&after, &changed), "{label}");
        out.push((t, after));
    }
    out
}

struct Vee {
    s: Sketch,
    a: PointId,
    p: PointId,
    l1: CurveId,
    l2: CurveId,
}

/// Zwei Linien mit gemeinsamer Ecke im Ursprung, 90°, Länge 40.
fn vee() -> Vee {
    let mut s = sketch();
    let (p, a, b) = (
        pt(&mut s, 0.0, 0.0),
        pt(&mut s, 40.0, 0.0),
        pt(&mut s, 0.0, 40.0),
    );
    let (l1, l2) = (s.add_line(a, p).unwrap(), s.add_line(p, b).unwrap());
    Vee { s, a, p, l1, l2 }
}

fn vee_picks(f: &Vee) -> (CurvePick, CurvePick) {
    let pick = |curve| CurvePick {
        curve,
        at: midpoint(&f.s, curve),
    };
    (pick(f.l1), pick(f.l2))
}

#[test]
fn a_chamfer_goes_along_with_its_visible_lines() {
    let kinds = [
        ChamferKind::Equal(5.0),
        ChamferKind::TwoDistances(5.0, 7.0),
        ChamferKind::DistanceAngle(5.0, 0.9),
    ];
    for kind in kinds {
        let mut f = vee();
        let (a, b) = vee_picks(&f);
        let plan = f.s.chamfer_plan(a, b, kind).unwrap();
        let ids = f.s.add_chamfer(&plan).unwrap();
        assert_whole_shape_follows(
            &format!("{kind:?}"),
            &f.s,
            &[f.l1, f.l2, ids.line],
            ids.dims[0],
        );
    }
}

#[test]
fn a_fillet_with_a_held_corner_goes_along_with_its_visible_curves() {
    let mut f = vee();
    let corner_dim = f.s.add_constraint(K::Distance(f.a, f.p, 40.0)).unwrap();
    let (a, b) = vee_picks(&f);
    let plan = f.s.fillet_plan(a, b, 5.0).unwrap();
    let ids = f.s.add_fillet(&plan, None).unwrap();
    assert_eq!(ids.corner, Some(f.p), "die Ecke bleibt");
    let picked = [f.l1, f.l2, ids.arc];
    assert_whole_shape_follows("Rundung", &f.s, &picked, ids.radius_dim.unwrap());
    let results = assert_whole_shape_follows("Eckmaß", &f.s, &picked, corner_dim);
    for (t, after) in results {
        assert_eq!(value_of(&after, corner_dim), 40.0 * t.factor, "{t:?}");
    }
}

#[test]
fn a_slot_takes_its_axis_along() {
    for style in [SlotStyle::CenterToCenter, SlotStyle::CenterPoint] {
        let plan = SlotPlan {
            a: v(0.0, 0.0),
            b: v(40.0, 12.0),
            radius: 5.0,
        };
        let mut s = sketch();
        let ids = s.add_slot(&plan, style).unwrap();
        let dim = s.add_constraint(K::Radius(ids.caps[0], 5.0)).unwrap();
        let picked = [ids.sides[0], ids.sides[1], ids.caps[0], ids.caps[1]];
        assert_whole_shape_follows(&format!("{style:?}"), &s, &picked, dim);
    }
}

#[test]
fn a_polygon_takes_its_circle_along() {
    let plan = polygon_inscribed(v(7.0, 3.0), v(27.0, 11.0), 6, TOL).unwrap();
    let mut s = sketch();
    let ids = s.add_polygon(&plan).unwrap();
    let side = at(&s, ids.points[0]).distance(at(&s, ids.points[1]));
    let dim = s
        .add_constraint(K::Distance(ids.points[0], ids.points[1], side))
        .unwrap();
    assert_whole_shape_follows("Vieleck", &s, &ids.lines, dim);
}

#[test]
fn a_helper_with_a_foreign_binding_or_without_a_link_stays() {
    let mut s = sketch();
    let (a, b) = (pt(&mut s, 0.0, 0.0), pt(&mut s, 10.0, 0.0));
    let l = s.add_line(a, b).unwrap();
    // Frei daneben: keine Verbindung.
    let m = pt(&mut s, 50.0, 50.0);
    let free = s.add_circle(m, 3.0).unwrap();
    s.set_construction(free, true).unwrap();
    // Am Ende der Linie, aber auch an einem fremden Punkt gebunden.
    let (c, d) = (pt(&mut s, 0.0, 9.0), pt(&mut s, 7.0, 9.0));
    let outside = pt(&mut s, -9.0, 9.0);
    let bound = s.add_line(a, c).unwrap();
    s.set_construction(bound, true).unwrap();
    s.add_constraint(K::Distance(c, outside, 9.0)).unwrap();
    // Nur an der Linie: kommt mit, auch der freie Endpunkt.
    let tag = s.add_line(b, d).unwrap();
    s.set_construction(tag, true).unwrap();
    let set = only_curves(&s, &[l]);
    assert_eq!(set.curves, vec![l, tag]);
    assert!(set.points.contains(&d));
    assert!(!set.points.contains(&m) && !set.points.contains(&c));
    // Die Menge ist abgeschlossen.
    assert_eq!(s.transform_set(&set.curves, &set.points), set);
}

/// Die ganze Vorlage und ihre Kopie.
struct Offset {
    s: Sketch,
    source: Vec<CurveId>,
    ids: OffsetIds,
}

fn offset_of(mut s: Sketch, source: Vec<CurveId>, d: f64) -> Offset {
    let chains = s.ordered_chains(&source).unwrap();
    let plan = s.offset_plan(&chains, d).unwrap();
    let ids = s.add_offset(&plan).unwrap();
    assert!(ids.tied);
    Offset { s, source, ids }
}

/// Rechteck 50×30 mit H, V, Maßen.
fn offset_rect() -> Offset {
    let mut s = sketch();
    let r = s.add_rectangle(v(0.0, 0.0), v(50.0, 30.0)).unwrap();
    s.add_constraint(K::Distance(r.points[0], r.points[1], 50.0))
        .unwrap();
    s.add_constraint(K::Distance(r.points[1], r.points[2], 30.0))
        .unwrap();
    offset_of(s, r.lines.to_vec(), 5.0)
}

/// Offene Kette: Linie, tangentialer Bogen, Linie. Mit Querlinien an den Enden.
fn offset_open_chain() -> Offset {
    let mut f = vee();
    let (a, b) = vee_picks(&f);
    let plan = f.s.fillet_plan(a, b, 10.0).unwrap();
    let ids = f.s.add_fillet(&plan, None).unwrap();
    let source = vec![f.l1, ids.arc, f.l2];
    offset_of(f.s, source, 3.0)
}

fn offset_circle() -> Offset {
    let mut s = sketch();
    let m = pt(&mut s, 3.0, 4.0);
    let c = s.add_circle(m, 20.0).unwrap();
    s.add_constraint(K::Radius(c, 20.0)).unwrap();
    offset_of(s, vec![c], 5.0)
}

fn offset_arc() -> Offset {
    let mut s = sketch();
    let (m, a, b) = (
        pt(&mut s, 0.0, 0.0),
        pt(&mut s, 20.0, 0.0),
        pt(&mut s, 0.0, 20.0),
    );
    let arc = s.add_arc(m, a, b).unwrap();
    offset_of(s, vec![arc], 5.0)
}

fn offsets() -> Vec<(&'static str, Offset)> {
    vec![
        ("Rechteck", offset_rect()),
        ("offene Kette", offset_open_chain()),
        ("Kreis", offset_circle()),
        ("Bogen", offset_arc()),
    ]
}

/// Abstand eines Punktes der Kopie zur Vorlage, als Maß für „die Kopie folgt“.
fn gap(s: &Sketch, f: &Offset) -> f64 {
    let piece = f.ids.curves[0];
    let q = s
        .curve_eval(piece, s.curve_domain(piece).unwrap().0)
        .unwrap();
    f.source
        .iter()
        .map(|&c| s.nearest_on_curve(c, q).unwrap().distance(q))
        .fold(f64::INFINITY, f64::min)
}

#[test]
fn a_moved_offset_source_takes_its_copy_along_and_the_copy_keeps_following() {
    for (name, f) in offsets() {
        let dim = f.ids.distance_dim.unwrap();
        let d = value_of(&f.s, dim);
        let results = assert_whole_shape_follows(name, &f.s, &f.source, dim);
        for (t, after) in results {
            assert!(
                (gap(&after, &f) - d * t.factor).abs() < 1e-8,
                "{name} {t:?}"
            );
            // Das Versatzmaß verschiebt die Kopie, die Vorlage bleibt stehen.
            let new = d * t.factor * 1.2;
            let mut trial = after.clone();
            trial.set_constraint_value(dim, new).unwrap();
            let hold = after.offset_hold_points(dim).unwrap();
            assert!(trial.solve_holding(&hold), "{name} {t:?}");
            for p in hold {
                assert_eq!(at(&trial, p), at(&after, p), "{name}");
            }
            assert!((gap(&trial, &f) - new).abs() < 1e-6, "{name} {t:?}");
            assert_holds(&trial, 1e-6 * t.factor);
            assert!(!folded_over(&after, &trial), "{name} {t:?}");
        }
    }
}

#[test]
fn an_offset_source_with_its_copy_picked_goes_along_with_its_rollers() {
    for (name, f) in offsets() {
        let mut picked = f.source.clone();
        picked.extend_from_slice(&f.ids.curves);
        let dim = f.ids.distance_dim.unwrap();
        assert_whole_shape_follows(name, &f.s, &picked, dim);
    }
}

#[test]
fn a_change_at_the_moved_source_still_moves_the_copy() {
    let f = offset_rect();
    let width =
        f.s.constraints()
            .find(|(_, d)| matches!(d.kind, K::Distance(_, _, x) if x == 50.0))
            .unwrap()
            .0;
    let set = f.s.transform_set(&f.source, &[]);
    for t in the_three_motions() {
        let mut after = f.s.clone();
        after.apply_transform(&set, &t).unwrap();
        let old = 50.0 * t.factor;
        let changed = change(&after, width, old, old * 1.2).unwrap();
        assert!((gap(&changed, &f) - 5.0 * t.factor).abs() < 1e-6, "{t:?}");
        let mut any_moved = false;
        for &c in &f.ids.curves {
            for p in changed.curve(c).unwrap().kind.points() {
                any_moved |= at(&changed, p).distance(at(&after, p)) > 1.0;
            }
        }
        assert!(any_moved, "{t:?}");
    }
}

#[test]
fn a_moved_offset_copy_alone_stops_following_and_the_report_says_so() {
    for (name, f) in offsets() {
        let dim = f.ids.distance_dim.unwrap();
        for t in the_three_motions() {
            let label = format!("{name} {t:?}");
            let set = f.s.transform_set(&f.ids.curves, &[]);
            for c in f.ids.rollers.iter().chain(&f.source) {
                assert!(!set.curves.contains(c), "{label}");
            }
            let mut after = f.s.clone();
            let report = after.apply_transform(&set, &t).unwrap();
            // Die Bindungen an Rollkreis und Vorlage sind weg, und der Bericht nennt sie.
            for (id, d) in f.s.constraints() {
                let ties = match d.kind {
                    K::Tangent(a, b) => f.ids.rollers.contains(&a) || f.ids.rollers.contains(&b),
                    K::Parallel(q, k) => f.source.contains(&q) && f.ids.curves.contains(&k),
                    _ => false,
                };
                if ties {
                    assert!(report.dropped.contains(&id), "{label}: {:?}", d.kind);
                }
            }
            assert!(!report.dropped.is_empty(), "{label}");
            for id in &report.dropped {
                assert!(!has(&after, *id), "{label}");
            }
            assert_eq!(value_of(&after, dim), value_of(&f.s, dim), "{label}");
            // Vorlage und Rollkreise stehen, die Kopie liegt neu.
            for c in f.source.iter().chain(&f.ids.rollers) {
                // Eine geteilte Bogenmitte wird getrennt: neuer Punkt, dieselbe Lage.
                assert_eq!(
                    after.curve_polyline(*c, 64),
                    f.s.curve_polyline(*c, 64),
                    "{label}"
                );
            }
            for &p in &set.points {
                let want = t.apply(at(&f.s, p));
                assert!(at(&after, p).distance(want) < 1e-9, "{label}");
            }
            let r = assert_still(&after);
            assert!(r.redundant.is_empty(), "{label}: {r:?}");
            // Das Versatzmaß erreicht die Kopie nicht mehr.
            let mut trial = after.clone();
            trial
                .set_constraint_value(dim, value_of(&after, dim) * 1.3)
                .unwrap();
            assert_solved(&trial.solve());
            for &c in &f.ids.curves {
                for p in after.curve(c).unwrap().kind.points() {
                    if set.points.contains(&p) {
                        assert_eq!(at(&trial, p), at(&after, p), "{label}");
                    }
                }
            }
        }
    }
}

#[test]
fn part_of_an_offset_source_moves_its_own_piece_and_stays_solved() {
    let f = offset_rect();
    let set = f.s.transform_set(&f.source[..1], &[]);
    assert!(set.curves.contains(&f.ids.rollers[0]), "{set:?}");
    assert!(f.ids.curves.iter().any(|c| set.curves.contains(c)));
    assert!(!f.source[1..].iter().any(|c| set.curves.contains(c)));
    for t in the_three_motions() {
        let mut after = f.s.clone();
        after.apply_transform(&set, &t).unwrap();
        let r = after.clone().solve();
        assert_solved(&r);
        assert!(r.redundant.is_empty(), "{t:?}: {r:?}");
    }
}

#[test]
fn a_copy_of_a_visible_selection_takes_the_helpers_along() {
    let mut f = vee();
    let (a, b) = vee_picks(&f);
    let plan = f.s.chamfer_plan(a, b, ChamferKind::Equal(5.0)).unwrap();
    let ids = f.s.add_chamfer(&plan).unwrap();
    let mut s = f.s.clone();
    let count = (s.curves().count(), s.constraints().count());
    let set = s.transform_set(&[f.l1, f.l2, ids.line], &[]);
    let map = s.copy_transformed(&set, &moved(v(90.0, 0.0))).unwrap();
    assert_eq!(s.curves().count(), count.0 * 2);
    assert_eq!(s.constraints().count(), count.1 * 2);
    assert_eq!(map.constraints.len(), count.1);
    let r = assert_still(&s);
    assert!(r.redundant.is_empty(), "{r:?}");

    let f = offset_rect();
    let mut s = f.s.clone();
    let count = s.constraints().count();
    let set = s.transform_set(&f.source, &[]);
    s.copy_transformed(&set, &moved(v(0.0, 200.0))).unwrap();
    assert_eq!(s.constraints().count(), count * 2);
    let r = assert_still(&s);
    assert!(r.redundant.is_empty(), "{r:?}");
}

#[test]
fn the_report_counts_the_points_split_from_outside_curves() {
    let mut s = sketch();
    let r = s.add_rectangle(v(0.0, 0.0), v(20.0, 10.0)).unwrap();
    let one = s.transform_set(&[r.lines[0]], &[]);
    let report = s
        .clone()
        .apply_transform(&one, &moved(v(0.0, -7.0)))
        .unwrap();
    assert_eq!(report.split, 2);
    let all = s.transform_set(&r.lines, &[]);
    let report = s.apply_transform(&all, &moved(v(0.0, -7.0))).unwrap();
    assert_eq!(report.split, 0);
}

/// Unter der Grenze, bis zu der eine Bindung als erfüllt gilt, bewirken Winkel und Weg nichts.
#[test]
fn a_turn_or_shift_below_the_settle_limit_counts_as_none() {
    let mut s = sketch();
    let a = pt(&mut s, 0.0, 0.0);
    let b = pt(&mut s, 26.0, 0.0);
    let line = s.add_line(a, b).unwrap();
    let set = s.transform_set(&[line], &[]);
    let tiny = Similarity {
        angle: 1e-7,
        ..moved(v(50.0, 0.0))
    };
    let mut t = s.clone();
    t.apply_transform(&set, &tiny).unwrap();
    assert_eq!(at(&t, a), v(50.0, 0.0));
    assert_eq!(at(&t, b), v(76.0, 0.0));
    let mut t = s.clone();
    let map = t.copy_transformed(&set, &tiny).unwrap();
    assert_eq!(at(&t, map.points[&b]), v(76.0, 0.0));
    for offset in [v(5e-6, 0.0), v(0.0, -9e-6)] {
        assert_eq!(
            s.check_transform(&set, &moved(offset)),
            Err(TransformBlock::NothingToDo)
        );
    }
    let mut t = s.clone();
    t.apply_transform(&set, &moved(v(2e-5, 0.0))).unwrap();
    assert_eq!(at(&t, a), v(2e-5, 0.0));
}
