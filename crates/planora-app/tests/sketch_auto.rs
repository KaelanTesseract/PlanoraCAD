//! Automatische Abhängigkeiten beim Zeichnen (Skizzen 2, Task 3): Was der aufgelöste Zeiger als
//! Zeichen nennt, entsteht beim Klick, und nichts sonst; Überzähliges fällt still weg. Die
//! Oberflächentests laufen ohne Fenster bei 1920 × 1080.

use planora_app::sketch_cursor::{AutoConstraint as A, DrawCursor, plan_direction};
use planora_app::sketch_state::{SketchSession, Snap, Tool};
use planora_app::state::{AppState, StandardPlane};
use planora_app::{fonts, sketch_view};
use planora_feature::{FeatureId, SketchPlane};
use planora_math::{DVec2, Frame, Tolerance};
use planora_sketch::constraint::{ConstraintKind as K, LineOrPoints};
use planora_sketch::model::{CurveId, CurveKind, PointId, Sketch};
use planora_sketch::solver::SolveStatus;

/// Fangradius in Skizzeneinheiten.
const R: f64 = 8.0;

fn setup(tool: Tool) -> (Sketch, SketchSession) {
    let mut sk = Sketch::new(Frame::WORLD, "S", Tolerance::default());
    let mut se = SketchSession::new(FeatureId(0));
    se.set_tool(&mut sk, tool);
    (sk, se)
}

fn line(sk: &mut Sketch, a: (f64, f64), b: (f64, f64)) -> CurveId {
    let p = sk.add_point(a.0, a.1).unwrap();
    let q = sk.add_point(b.0, b.1).unwrap();
    sk.add_line(p, q).unwrap()
}

fn end_of(sk: &Sketch, c: CurveId) -> PointId {
    match sk.curve(c).unwrap().kind {
        CurveKind::Line { b, .. } => b,
        CurveKind::Arc { end, .. } => end,
        _ => panic!("weder Linie noch Bogen"),
    }
}

/// Bogen um `m` mit Radius `r` von `from` bis `to` (Grad, gegen den Uhrzeigersinn).
fn arc(sk: &mut Sketch, m: (f64, f64), r: f64, from: f64, to: f64) -> CurveId {
    let on = |deg: f64| {
        let a = deg.to_radians();
        (m.0 + r * a.cos(), m.1 + r * a.sin())
    };
    let (s, e) = (on(from), on(to));
    let c = sk.add_point(m.0, m.1).unwrap();
    let s = sk.add_point(s.0, s.1).unwrap();
    let e = sk.add_point(e.0, e.1).unwrap();
    sk.add_arc(c, s, e).unwrap()
}

fn rotated(d: DVec2, deg: f64) -> DVec2 {
    let (s, c) = deg.to_radians().sin_cos();
    DVec2::new(d.x * c - d.y * s, d.x * s + d.y * c)
}

fn resolve(se: &mut SketchSession, sk: &Sketch, at: DVec2) -> DrawCursor {
    se.resolve_cursor(sk, at, R)
}

fn click(se: &mut SketchSession, sk: &mut Sketch, at: DVec2) {
    se.click(sk, at, R, false).unwrap();
}

fn xy(x: f64, y: f64) -> DVec2 {
    DVec2::new(x, y)
}

fn kinds(sk: &Sketch) -> Vec<K> {
    sk.constraints().map(|(_, c)| c.kind.clone()).collect()
}

fn newest_curve(sk: &Sketch) -> CurveId {
    sk.curves().map(|(c, _)| c).last().unwrap()
}

fn solved(se: &SketchSession) -> bool {
    se.last_report
        .as_ref()
        .is_some_and(|r| r.status == SolveStatus::Solved)
}

fn positions(sk: &Sketch) -> Vec<(PointId, DVec2)> {
    sk.points()
        .map(|(p, _)| (p, sk.point_xy(p).unwrap()))
        .collect()
}

fn assert_kept(sk: &Sketch, before: &[(PointId, DVec2)]) {
    for (p, at) in before {
        let now = sk.point_xy(*p).unwrap();
        assert!(now.distance(*at) < 1e-9, "{p:?}: {now:?} statt {at:?}");
    }
}

/// Die Abhängigkeit, die ein Zeichen an der neuen Kurve `c` verspricht.
fn promised(auto: A, c: CurveId) -> K {
    match auto {
        A::Horizontal => K::Horizontal(LineOrPoints::Line(c)),
        A::Vertical => K::Vertical(LineOrPoints::Line(c)),
        A::Parallel(r) => K::Parallel(c, r),
        A::Perpendicular(r) => K::Perpendicular(c, r),
        A::Tangent(r) => K::Tangent(c, r),
    }
}

/// Klickt an `raw` und prüft: Genau die Zeichen des Zeigers sind als Abhängigkeiten dazugekommen,
/// die Linie endet an der gezeigten Lage, nichts Vorhandenes hat sich bewegt.
fn click_and_compare(se: &mut SketchSession, sk: &mut Sketch, raw: DVec2) -> DrawCursor {
    let cur = resolve(se, sk, raw);
    let (old, kept) = (kinds(sk), positions(sk));
    click(se, sk, raw);
    let c = newest_curve(sk);
    let new: Vec<K> = kinds(sk).into_iter().skip(old.len()).collect();
    let want: Vec<K> = cur.autos.iter().map(|&a| promised(a, c)).collect();
    assert_eq!(new, want, "alles aus `autos` entsteht, und nichts sonst");
    assert_kept(sk, &kept);
    let end = sk.point_xy(end_of(sk, c)).unwrap();
    assert!(end.distance(cur.at) < 1e-9, "{end:?} statt {:?}", cur.at);
    assert!(solved(se));
    cur
}

/// Linienwerkzeug mit gesetztem Anfang.
fn started_at(sk: &mut Sketch, se: &mut SketchSession, x: f64, y: f64) -> DVec2 {
    se.set_tool(sk, Tool::Line);
    click(se, sk, xy(x, y));
    xy(x, y)
}

// ---- Waagerecht, senkrecht ----------------------------------------------------------------

#[test]
fn a_nearly_horizontal_line_is_exactly_horizontal_in_preview_and_result() {
    let (mut sk, mut se) = setup(Tool::Line);
    started_at(&mut sk, &mut se, 100.0, 100.0);
    let cur = click_and_compare(&mut se, &mut sk, xy(200.0, 103.0));
    assert_eq!(cur.autos, vec![A::Horizontal]);
    assert_eq!(cur.at, xy(200.0, 100.0), "Vorschau exakt waagerecht");
    assert_eq!(sk.constraints().count(), 1);
}

#[test]
fn a_nearly_vertical_line_also_when_drawn_downwards() {
    let (mut sk, mut se) = setup(Tool::Line);
    started_at(&mut sk, &mut se, 100.0, 300.0);
    let cur = click_and_compare(&mut se, &mut sk, xy(103.0, 200.0));
    assert_eq!(cur.autos, vec![A::Vertical]);
    assert_eq!(cur.at, xy(100.0, 200.0));
}

#[test]
fn the_grid_only_acts_along_a_horizontal_direction() {
    let (mut sk, mut se) = setup(Tool::Line);
    se.cursor_opts.grid_step = Some(10.0);
    started_at(&mut sk, &mut se, 100.0, 100.0);
    let cur = resolve(&mut se, &sk, xy(203.0, 103.0));
    assert_eq!(cur.autos, vec![A::Horizontal]);
    assert_eq!(cur.at, xy(200.0, 100.0));
    assert_eq!(cur.snap, Snap::Grid);
}

// ---- Parallel, rechtwinklig ---------------------------------------------------------------

/// Bezugslinie unter 35° (weit von den Achsen), Länge 100.
fn reference(sk: &mut Sketch) -> (CurveId, DVec2) {
    let d = rotated(DVec2::X, 35.0);
    let a = xy(100.0, 500.0);
    let b = a + d * 100.0;
    (line(sk, (a.x, a.y), (b.x, b.y)), d)
}

#[test]
fn an_oblique_line_two_degrees_beside_another_becomes_parallel_and_points_stay_exact() {
    let (mut sk, mut se) = setup(Tool::Line);
    let (r, d) = reference(&mut sk);
    let start = started_at(&mut sk, &mut se, 400.0, 100.0);
    let raw = start + rotated(d, 2.0) * 120.0;
    let cur = click_and_compare(&mut se, &mut sk, raw);
    assert_eq!(cur.autos, vec![A::Parallel(r)]);
    assert!((cur.at - start).perp_dot(d).abs() < 1e-9, "exakt parallel");
    assert_ne!(cur.at, raw);
}

#[test]
fn the_grid_does_not_pull_an_oblique_line_off_its_direction() {
    let (mut sk, mut se) = setup(Tool::Line);
    let (r, d) = reference(&mut sk);
    se.cursor_opts.grid_step = Some(10.0);
    let start = started_at(&mut sk, &mut se, 400.0, 100.0);
    let cur = resolve(&mut se, &sk, start + rotated(d, 2.0) * 120.0);
    assert_eq!(cur.autos, vec![A::Parallel(r)]);
    assert!((cur.at - start).perp_dot(d).abs() < 1e-9);
}

#[test]
fn eighty_eight_degrees_become_perpendicular() {
    let (mut sk, mut se) = setup(Tool::Line);
    let (r, d) = reference(&mut sk);
    let start = started_at(&mut sk, &mut se, 400.0, 100.0);
    let cur = click_and_compare(&mut se, &mut sk, start + rotated(d, 88.0) * 120.0);
    assert_eq!(cur.autos, vec![A::Perpendicular(r)]);
    assert!((cur.at - start).dot(d).abs() < 1e-9, "exakt rechtwinklig");
}

#[test]
fn five_degrees_beside_a_line_give_nothing() {
    let (mut sk, mut se) = setup(Tool::Line);
    let (_, d) = reference(&mut sk);
    let start = started_at(&mut sk, &mut se, 400.0, 100.0);
    let raw = start + rotated(d, 5.0) * 120.0;
    let cur = click_and_compare(&mut se, &mut sk, raw);
    assert!(cur.autos.is_empty());
    assert_eq!(cur.at, raw);
}

#[test]
fn horizontal_beats_parallel() {
    let (mut sk, mut se) = setup(Tool::Line);
    line(&mut sk, (100.0, 500.0), (200.0, 503.5));
    started_at(&mut sk, &mut se, 400.0, 100.0);
    let cur = click_and_compare(&mut se, &mut sk, xy(520.0, 102.0));
    assert_eq!(cur.autos, vec![A::Horizontal]);
}

#[test]
fn the_best_angle_wins_and_on_a_tie_the_smaller_id() {
    let (mut sk, _) = setup(Tool::Line);
    let d = rotated(DVec2::X, 35.0);
    let far = |deg: f64, y: f64| {
        let e = rotated(DVec2::X, deg);
        ((100.0, y), (100.0 + 100.0 * e.x, y + 100.0 * e.y))
    };
    let (a1, b1) = far(37.0, 500.0);
    let (a2, b2) = far(36.0, 700.0);
    let (a3, b3) = far(36.0, 900.0);
    let _worse = line(&mut sk, a1, b1);
    let best = line(&mut sk, a2, b2);
    let _tie = line(&mut sk, a3, b3);
    let s = sk.add_point(400.0, 100.0).unwrap();
    let plan = plan_direction(
        &sk,
        Tool::Line,
        s,
        None,
        xy(400.0, 100.0) + d * 100.0,
        false,
    );
    assert_eq!(plan.map(|p| p.0), Some(A::Parallel(best)));
    let (_, dir) = plan.unwrap();
    assert!((dir.length() - 1.0).abs() < 1e-12, "normiert");
    assert!(dir.dot(d) > 0.0, "zeigt zum Zeiger");
}

#[test]
fn only_the_line_tool_plans_a_direction() {
    let (mut sk, _) = setup(Tool::Line);
    let s = sk.add_point(100.0, 100.0).unwrap();
    assert_eq!(
        plan_direction(&sk, Tool::Line, s, None, xy(200.0, 101.0), false).map(|p| p.0),
        Some(A::Horizontal)
    );
    for tool in [Tool::Circle, Tool::Spline, Tool::Arc, Tool::Rectangle] {
        assert_eq!(
            plan_direction(&sk, tool, s, None, xy(200.0, 101.0), false),
            None
        );
    }
}

#[test]
fn the_running_chain_is_no_reference() {
    let (mut sk, mut se) = setup(Tool::Line);
    let d = rotated(DVec2::X, 35.0);
    let a = started_at(&mut sk, &mut se, 100.0, 100.0);
    let b = a + d * 100.0;
    click(&mut se, &mut sk, b);
    let c = b + rotated(d, 60.0) * 100.0;
    click(&mut se, &mut sk, c);
    assert!(kinds(&sk).is_empty());
    // Fast parallel zur ersten Linie der Kette, zu keiner anderen.
    let cur = resolve(&mut se, &sk, c + rotated(d, 2.0) * 100.0);
    assert!(cur.autos.is_empty(), "{:?}", cur.autos);
    // Eine neue Kette darf sich auf die alte beziehen.
    se.finish_tool(&mut sk).unwrap();
    started_at(&mut sk, &mut se, 600.0, 100.0);
    let cur = resolve(&mut se, &sk, xy(600.0, 100.0) + rotated(d, 2.0) * 100.0);
    assert!(matches!(cur.autos.as_slice(), [A::Parallel(_)]));
}

// ---- Tangential ---------------------------------------------------------------------------

/// Bogen um (500, 500), r = 100, von 0° bis 45°; Fortsetzung am Ende unter 135°.
fn tangent_setup() -> (Sketch, SketchSession, CurveId, DVec2, DVec2) {
    let (mut sk, mut se) = setup(Tool::Line);
    let a = arc(&mut sk, (500.0, 500.0), 100.0, 0.0, 45.0);
    let end = sk.point_xy(end_of(&sk, a)).unwrap();
    se.set_tool(&mut sk, Tool::Line);
    click(&mut se, &mut sk, end);
    (sk, se, a, end, rotated(DVec2::X, 135.0))
}

#[test]
fn a_line_leaving_an_arc_in_its_direction_is_tangent() {
    let (mut sk, mut se, a, end, forward) = tangent_setup();
    let cur = click_and_compare(&mut se, &mut sk, end + rotated(forward, 2.0) * 80.0);
    assert_eq!(cur.autos, vec![A::Tangent(a)]);
    assert!(
        (cur.at - end).perp_dot(forward).abs() < 1e-9,
        "exakt tangential"
    );
}

#[test]
fn a_line_going_back_along_the_arc_is_not_tangent() {
    let (mut sk, mut se, _, end, forward) = tangent_setup();
    let cur = click_and_compare(&mut se, &mut sk, end - rotated(forward, 2.0) * 80.0);
    assert!(cur.autos.is_empty(), "{:?}", cur.autos);
}

#[test]
fn at_the_start_of_an_arc_the_direction_is_the_other_way_round() {
    let (mut sk, mut se) = setup(Tool::Line);
    let a = arc(&mut sk, (500.0, 500.0), 100.0, 30.0, 80.0);
    let CurveKind::Arc { start, .. } = sk.curve(a).unwrap().kind else {
        panic!()
    };
    let s = sk.point_xy(start).unwrap();
    se.set_tool(&mut sk, Tool::Line);
    click(&mut se, &mut sk, s);
    // Bei 30° läuft der Bogen unter 120° los; weg von ihm geht es also unter -60°.
    let cur = click_and_compare(&mut se, &mut sk, s + rotated(DVec2::X, -58.0) * 80.0);
    assert_eq!(cur.autos, vec![A::Tangent(a)]);
}

/// Linie unter 35° mit Ende `s`; der Bogen läuft dort weiter, wenn die Mitte links davon liegt.
fn arc_on_line() -> (Sketch, SketchSession, CurveId, DVec2, DVec2) {
    let (mut sk, mut se) = setup(Tool::Arc);
    let (l, d) = reference(&mut sk);
    let s = sk.point_xy(end_of(&sk, l)).unwrap();
    se.set_tool(&mut sk, Tool::Arc);
    (sk, se, l, s, d)
}

#[test]
fn an_arc_on_the_end_of_a_line_is_tangent_when_finished_and_marked_before() {
    let (mut sk, mut se, l, s, d) = arc_on_line();
    let kept = positions(&sk);
    // Mitte 2° neben der Senkrechten auf der Linie, der Bogen läuft in Laufrichtung weiter.
    let center = s + rotated(d, 92.0) * 50.0;
    click(&mut se, &mut sk, center);
    assert_eq!(resolve(&mut se, &sk, s).autos, vec![A::Tangent(l)]);
    click(&mut se, &mut sk, s);
    assert!(kinds(&sk).is_empty(), "erst beim Abschluss");
    let m = sk.point_xy(se.pending[0]).unwrap();
    // Die Mitte liegt jetzt exakt auf der Senkrechten, die Vorschau zeigt schon den Bogen.
    assert!((m - s).dot(d).abs() < 1e-9);
    // Weit vom Bogen entfernt; das Ende landet auf dem Radius.
    let far = m + rotated(d, 40.0) * 150.0;
    let cur = resolve(&mut se, &sk, far);
    assert_eq!(cur.autos, vec![A::Tangent(l)], "Zeichen in der Endstufe");
    click(&mut se, &mut sk, far);
    let c = newest_curve(&sk);
    assert_eq!(kinds(&sk), vec![K::Tangent(c, l)]);
    assert_kept(&sk, &kept);
    assert!(solved(&se));
    assert_eq!(se.auto_tangent, None);
}

#[test]
fn an_arc_running_back_over_the_line_is_not_tangent() {
    let (mut sk, mut se, _, s, d) = arc_on_line();
    let center = s + rotated(d, -90.0) * 50.0;
    click(&mut se, &mut sk, center);
    assert!(resolve(&mut se, &sk, s).autos.is_empty());
    click(&mut se, &mut sk, s);
    let far = center + rotated(d, 60.0) * 150.0;
    assert!(resolve(&mut se, &sk, far).autos.is_empty());
    click(&mut se, &mut sk, far);
    assert!(kinds(&sk).is_empty());
}

#[test]
fn an_arc_around_an_existing_point_needs_the_exact_direction() {
    let (mut sk, mut se, _, s, d) = arc_on_line();
    let center = s + rotated(d, 92.0) * 50.0;
    let m = sk.add_point(center.x, center.y).unwrap();
    click(&mut se, &mut sk, center);
    assert_eq!(se.pending, vec![m], "gefangen");
    assert!(resolve(&mut se, &sk, s).autos.is_empty());
    click(&mut se, &mut sk, s);
    assert_eq!(sk.point_xy(m).unwrap(), center, "der Punkt bleibt");
    let far = center + rotated(d, 40.0) * 150.0;
    click(&mut se, &mut sk, far);
    assert!(kinds(&sk).is_empty());
}

#[test]
fn an_arc_on_the_end_of_an_arc_is_tangent() {
    let (mut sk, mut se) = setup(Tool::Arc);
    let first = arc(&mut sk, (500.0, 500.0), 100.0, 0.0, 45.0);
    let s = sk.point_xy(end_of(&sk, first)).unwrap();
    let toward = (xy(500.0, 500.0) - s).normalize();
    let kept = positions(&sk);
    let center = s + rotated(toward, 2.0) * 40.0;
    se.set_tool(&mut sk, Tool::Arc);
    click(&mut se, &mut sk, center);
    assert_eq!(resolve(&mut se, &sk, s).autos, vec![A::Tangent(first)]);
    click(&mut se, &mut sk, s);
    let m = sk.point_xy(se.pending[0]).unwrap();
    let far = m + rotated(toward, 120.0) * 200.0;
    assert_eq!(resolve(&mut se, &sk, far).autos, vec![A::Tangent(first)]);
    click(&mut se, &mut sk, far);
    assert_eq!(kinds(&sk), vec![K::Tangent(newest_curve(&sk), first)]);
    assert_kept(&sk, &kept);
    assert!(solved(&se));
}

#[test]
fn ctrl_in_the_last_stage_drops_the_tangent() {
    let (mut sk, mut se, _, s, d) = arc_on_line();
    click(&mut se, &mut sk, s + rotated(d, 92.0) * 50.0);
    click(&mut se, &mut sk, s);
    se.cursor_opts.suppressed = true;
    let m = sk.point_xy(se.pending[0]).unwrap();
    let far = m + rotated(d, 40.0) * 150.0;
    assert!(resolve(&mut se, &sk, far).autos.is_empty());
    click(&mut se, &mut sk, far);
    assert!(kinds(&sk).is_empty());
}

#[test]
fn a_failed_last_click_keeps_the_tangent_plan_and_cancel_forgets_it() {
    let (mut sk, mut se, l, s, d) = arc_on_line();
    click(&mut se, &mut sk, s + rotated(d, 92.0) * 50.0);
    click(&mut se, &mut sk, s);
    assert_eq!(se.auto_tangent, Some(l));
    assert!(
        se.click(&mut sk, s, R, false).is_err(),
        "Ende auf dem Anfang"
    );
    assert_eq!(se.auto_tangent, Some(l));
    se.cancel(&mut sk);
    assert_eq!(se.auto_tangent, None);
}

// ---- Gleich entsteht nie ------------------------------------------------------------------

#[test]
fn equal_never_appears_by_itself() {
    let (mut sk, mut se) = setup(Tool::Line);
    let (r, d) = reference(&mut sk);
    let start = started_at(&mut sk, &mut se, 400.0, 100.0);
    let length = sk.line_length(r).unwrap();
    click(&mut se, &mut sk, start + rotated(d, 1.0) * length);
    assert!(!kinds(&sk).iter().any(|k| matches!(k, K::Equal(..))));
}

// ---- Überzählig, widersprüchlich, ungelöst (Review-Focus 3) -------------------------------

fn fixed_point(sk: &mut Sketch, x: f64, y: f64) -> PointId {
    let p = sk.add_point(x, y).unwrap();
    sk.add_constraint(K::Fixed(p, x, y)).unwrap();
    p
}

#[test]
fn a_line_between_two_fixed_points_gets_no_horizontal_and_no_redundancy() {
    let (mut sk, mut se) = setup(Tool::Line);
    fixed_point(&mut sk, 100.0, 100.0);
    fixed_point(&mut sk, 200.0, 100.0);
    se.last_report = Some(sk.solve());
    se.set_tool(&mut sk, Tool::Line);
    let (old, kept) = (kinds(&sk), positions(&sk));
    click(&mut se, &mut sk, xy(100.0, 100.0));
    click(&mut se, &mut sk, xy(200.0, 100.0));
    assert_eq!(sk.curves().count(), 1);
    assert_eq!(kinds(&sk), old, "still weggelassen");
    assert_kept(&sk, &kept);
    let report = se.last_report.as_ref().unwrap();
    assert_eq!(report.status, SolveStatus::Solved);
    assert!(report.redundant.is_empty(), "{:?}", report.redundant);
    assert!(se.message.is_none(), "keine Meldung");
}

#[test]
fn a_parallel_on_fully_determined_geometry_falls_away_and_nothing_moves() {
    let (mut sk, mut se) = setup(Tool::Line);
    let d = rotated(DVec2::X, 35.0);
    let a = fixed_point(&mut sk, 100.0, 500.0);
    let tip = xy(100.0, 500.0) + d * 100.0;
    let b = fixed_point(&mut sk, tip.x, tip.y);
    sk.add_line(a, b).unwrap();
    let q = xy(400.0, 100.0) + d * 100.0;
    fixed_point(&mut sk, 400.0, 100.0);
    fixed_point(&mut sk, q.x, q.y);
    se.last_report = Some(sk.solve());
    se.set_tool(&mut sk, Tool::Line);
    let (old, kept) = (kinds(&sk), positions(&sk));
    click(&mut se, &mut sk, xy(400.0, 100.0));
    click(&mut se, &mut sk, q);
    assert_eq!(sk.curves().count(), 2);
    assert_eq!(kinds(&sk), old);
    assert_kept(&sk, &kept);
    assert!(se.last_report.as_ref().unwrap().redundant.is_empty());
}

#[test]
fn in_an_unsolved_sketch_nothing_is_planned_or_added() {
    let (mut sk, mut se) = setup(Tool::Line);
    let p = sk.add_point(700.0, 700.0).unwrap();
    sk.add_constraint(K::Fixed(p, 700.0, 700.0)).unwrap();
    sk.add_constraint(K::Fixed(p, 750.0, 750.0)).unwrap();
    se.last_report = Some(sk.solve());
    assert!(!solved(&se));
    se.set_tool(&mut sk, Tool::Line);
    let old = kinds(&sk);
    click(&mut se, &mut sk, xy(100.0, 100.0));
    let raw = xy(200.0, 103.0);
    let cur = resolve(&mut se, &sk, raw);
    assert!(cur.autos.is_empty());
    assert_eq!(cur.at, raw, "die Vorschau folgt dem Zeiger");
    click(&mut se, &mut sk, raw);
    assert_eq!(kinds(&sk), old);
}

#[test]
fn a_line_onto_an_existing_point_two_degrees_off_is_not_straightened() {
    let (mut sk, mut se) = setup(Tool::Line);
    // So weit weg, dass der Anfang sich nicht an ihm ausrichtet.
    let target = xy(300.0, 100.0 + 200.0 * 2f64.to_radians().tan());
    let p = sk.add_point(target.x, target.y).unwrap();
    se.set_tool(&mut sk, Tool::Line);
    click(&mut se, &mut sk, xy(100.0, 100.0));
    let cur = resolve(&mut se, &sk, target);
    assert_eq!(cur.snap, Snap::Point(p));
    assert!(cur.autos.is_empty());
    click(&mut se, &mut sk, target);
    assert!(kinds(&sk).is_empty());
    assert_eq!(sk.point_xy(p).unwrap(), target, "unbewegt");
}

#[test]
fn a_line_onto_an_existing_point_exactly_horizontal_is_horizontal() {
    let (mut sk, mut se) = setup(Tool::Line);
    let p = sk.add_point(200.0, 100.0).unwrap();
    se.set_tool(&mut sk, Tool::Line);
    click(&mut se, &mut sk, xy(100.0, 100.0));
    let cur = resolve(&mut se, &sk, xy(201.0, 101.0));
    assert_eq!(cur.snap, Snap::Point(p));
    assert_eq!(cur.autos, vec![A::Horizontal]);
    click(&mut se, &mut sk, xy(201.0, 101.0));
    assert_eq!(kinds(&sk).len(), 1);
}

#[test]
fn a_suppressed_cursor_has_no_direction_and_follows_the_raw_pointer() {
    let (mut sk, mut se) = setup(Tool::Line);
    started_at(&mut sk, &mut se, 100.0, 100.0);
    se.cursor_opts.suppressed = true;
    let raw = xy(200.0, 103.0);
    let cur = resolve(&mut se, &sk, raw);
    assert!(cur.autos.is_empty());
    assert_eq!(cur.at, raw);
    click(&mut se, &mut sk, raw);
    assert!(kinds(&sk).is_empty());
    assert_eq!(sk.point_xy(end_of(&sk, newest_curve(&sk))).unwrap(), raw);
}

#[test]
fn switched_off_only_the_direction_goes_and_the_binding_stays() {
    let (mut sk, mut se) = setup(Tool::Line);
    let target = line(&mut sk, (150.0, 50.0), (150.0, 250.0));
    se.auto_constrain = false;
    started_at(&mut sk, &mut se, 100.0, 100.0);
    // Auf der senkrechten Linie, fast waagerecht zum Anfang.
    let raw = xy(150.0, 101.0);
    let cur = resolve(&mut se, &sk, raw);
    assert!(cur.autos.is_empty());
    assert_eq!(cur.snap, Snap::Curve(target));
    click(&mut se, &mut sk, raw);
    let end = end_of(&sk, newest_curve(&sk));
    assert_eq!(kinds(&sk), vec![K::PointOnCurve(end, target)]);

    // Frei: keine Richtung, die Vorschau folgt dem Zeiger.
    let free = xy(300.0, 106.0);
    let cur = resolve(&mut se, &sk, free);
    assert!(cur.autos.is_empty());
    assert_eq!(cur.at, free);
}

// ---- try_auto -----------------------------------------------------------------------------

#[test]
fn try_auto_refuses_what_would_move_a_point() {
    let (mut sk, mut se) = setup(Tool::Line);
    let a = line(&mut sk, (100.0, 100.0), (200.0, 100.0));
    let b = line(&mut sk, (100.0, 300.0), (200.0, 303.5));
    se.last_report = Some(sk.solve());
    let stand = sk.clone();
    assert!(
        !se.try_auto(&mut sk, K::Parallel(b, a)),
        "2° daneben bewegt"
    );
    assert_eq!(kinds(&sk), kinds(&stand));
    assert_kept(&sk, &positions(&stand));
    let c = line(&mut sk, (100.0, 500.0), (200.0, 500.0));
    se.last_report = Some(sk.solve());
    assert!(se.try_auto(&mut sk, K::Parallel(c, a)));
    assert_eq!(kinds(&sk), vec![K::Parallel(c, a)]);
    assert!(solved(&se));
}

#[test]
fn try_auto_does_nothing_without_a_solved_sketch_before() {
    let (mut sk, mut se) = setup(Tool::Line);
    let a = line(&mut sk, (100.0, 100.0), (200.0, 100.0));
    assert!(se.last_report.is_none());
    assert!(!se.try_auto(&mut sk, K::Horizontal(LineOrPoints::Line(a))));
    assert!(kinds(&sk).is_empty());
}

// ---- Oberfläche und Rückgängig ------------------------------------------------------------

const W: f32 = 1920.0;
const H: f32 = 1080.0;
const SCREEN: egui::Rect = egui::Rect {
    min: egui::pos2(0.0, 0.0),
    max: egui::pos2(W, H),
};

fn context() -> egui::Context {
    let ctx = egui::Context::default();
    ctx.set_fonts(fonts::definitions());
    ctx
}

fn sketch_with(build: impl FnOnce(&mut Sketch)) -> AppState {
    let mut s = AppState::new();
    let i = s
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    s.open_sketch(i).unwrap();
    {
        let sk = s.session_sketch_mut().unwrap().1;
        build(sk);
        sk.solve();
    }
    s.observe_sketch(false);
    s
}

fn app_click(s: &mut AppState, at: DVec2) {
    let (se, sk) = s.session_sketch_mut().unwrap();
    se.click(sk, at, 0.5, false).unwrap();
    s.observe_sketch(false);
}

fn set_tool(s: &mut AppState, tool: Tool) {
    let (se, sk) = s.session_sketch_mut().unwrap();
    se.set_tool(sk, tool);
}

#[test]
fn a_line_with_a_constraint_is_named_as_the_line_and_undoes_in_one_step() {
    let mut s = sketch_with(|_| {});
    set_tool(&mut s, Tool::Line);
    app_click(&mut s, xy(10.0, 10.0));
    app_click(&mut s, xy(20.0, 10.2));
    let label = s.sketch_undo_label().unwrap();
    assert!(label.contains(&planora_i18n::tr!("undo-line")), "{label}");
    assert_eq!(s.session_sketch().unwrap().constraints().count(), 1);
    assert!(s.undo_sketch());
    let sk = s.session_sketch().unwrap();
    assert_eq!(sk.curves().count(), 0);
    assert_eq!(sk.constraints().count(), 0);
}

#[test]
fn a_tangent_arc_is_one_step_named_as_the_arc() {
    let d = rotated(DVec2::X, 35.0);
    let s0 = xy(10.0, 10.0);
    let tip = s0 + d * 10.0;
    let mut s = sketch_with(|sk| {
        line(sk, (s0.x, s0.y), (tip.x, tip.y));
    });
    set_tool(&mut s, Tool::Arc);
    let steps = s.sketch_history().unwrap().undo_len();
    app_click(&mut s, tip + rotated(d, 92.0) * 5.0);
    app_click(&mut s, tip);
    assert_eq!(
        s.sketch_history().unwrap().undo_len(),
        steps,
        "der Mittelpunkt rückt dabei zurecht, der Schritt bleibt offen"
    );
    let m = {
        let (se, sk) = s.session_sketch_mut().unwrap();
        sk.point_xy(se.pending[0]).unwrap()
    };
    app_click(&mut s, m + rotated(d, 40.0) * 15.0);
    assert_eq!(s.sketch_history().unwrap().undo_len(), steps + 1);
    let label = s.sketch_undo_label().unwrap();
    assert!(label.contains(&planora_i18n::tr!("undo-arc")), "{label}");
    assert!(
        kinds(s.session_sketch().unwrap())
            .iter()
            .any(|k| matches!(k, K::Tangent(..)))
    );
    assert!(s.undo_sketch());
    let sk = s.session_sketch().unwrap();
    assert_eq!((sk.curves().count(), sk.constraints().count()), (1, 0));
    assert_eq!(sk.points().count(), 2, "Mitte und Bogenende sind mit weg");
}

#[test]
fn marks_stand_at_the_new_line_and_at_the_reference() {
    let d = rotated(DVec2::X, 35.0);
    let (mut sk, mut se) = setup(Tool::Line);
    let (r, _) = reference(&mut sk);
    let start = started_at(&mut sk, &mut se, 400.0, 100.0);
    assert!(sketch_view::auto_marks(&sk, &se).is_empty(), "kein Zeiger");
    let raw = start + rotated(d, 2.0) * 120.0;
    se.update_cursor(&sk, Some(raw), R);
    let at = se.draw_cursor.as_ref().unwrap().at;
    let marks = sketch_view::auto_marks(&sk, &se);
    assert_eq!(marks.len(), 2, "{marks:?}");
    assert_eq!(marks[0], ((start + at) * 0.5, A::Parallel(r)));
    let tip = xy(100.0, 500.0) + d * 100.0;
    assert!(marks[1].0.distance((xy(100.0, 500.0) + tip) * 0.5) < 1e-9);
    // Waagerecht und senkrecht haben keine Bezugslinie.
    se.update_cursor(&sk, Some(xy(500.0, 102.0)), R);
    assert_eq!(
        sketch_view::auto_marks(&sk, &se),
        vec![(xy(450.0, 100.0), A::Horizontal)]
    );
}

#[test]
fn the_overlay_paints_the_mark_while_drawing() {
    let ctx = context();
    let mut s = sketch_with(|_| {});
    set_tool(&mut s, Tool::Line);
    app_click(&mut s, xy(10.0, 10.0));
    let frame = *s.session_sketch().unwrap().frame();
    let mut cam = s.camera;
    cam.set_viewport(W, H);
    let v =
        sketch_view::sketch_to_screen(&cam, &frame, glam::Vec2::new(W, H), xy(40.0, 10.4)).unwrap();
    let p = egui::pos2(v.x, v.y);
    let mut shapes = Vec::new();
    for events in [vec![], vec![egui::Event::PointerMoved(p)], vec![]] {
        let raw = egui::RawInput {
            screen_rect: Some(SCREEN),
            events,
            ..Default::default()
        };
        let output = ctx.run_ui(raw, |ui| {
            sketch_view::show_overlay(ui, SCREEN, &mut s);
        });
        shapes = output.shapes.clone();
        output.drop_without_applying_deltas();
    }
    let cur = s.sketch_session.as_ref().unwrap().draw_cursor.clone();
    assert_eq!(cur.unwrap().autos, vec![A::Horizontal]);
    let painted = shapes.iter().any(|c| match &c.shape {
        egui::Shape::Text(t) => t.galley.text() == "H",
        _ => false,
    });
    assert!(painted, "Zeichen H gemalt");
    assert_eq!(s.session_sketch().unwrap().curves().count(), 0);
}
