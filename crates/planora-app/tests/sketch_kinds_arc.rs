//! Skizzen 3, Task 7: Bogen 3 Punkte und tangential auf der Stufenmaschine. Ohne Oberfläche über
//! `click`/`commit_input`, je Art eine Fahrt über Rohereignisse bei 1920×1080
//! (`PlanoraApp::for_tests`).

mod common;

use eframe::App as _;
use egui::{Event, Key, Modifiers, PointerButton, Pos2, Rect, Vec2, pos2, vec2};
use planora_app::app::PlanoraApp;
use planora_app::sketch_input::FieldKind as F;
use planora_app::sketch_kinds::ToolKind;
use planora_app::sketch_state::{SketchSession, Snap, Tool};
use planora_app::state::AppState;
use planora_app::workspace::Workspace;
use planora_app::{fonts, theme};
use planora_feature::{FeatureId, ParamScope, SketchFeature, SketchPlane, StandardPlane};
use planora_i18n::t;
use planora_math::{DVec2, Frame, Tolerance};
use planora_project::config::Config;
use planora_project::library::Library;
use planora_sketch::constraint::ConstraintKind as K;
use planora_sketch::model::{CurveId, CurveKind, PointId, Sketch};
use planora_sketch::shapes::{ArcPlan, ShapePlan};
use planora_sketch::solver::{SolveReport, SolveStatus};
use std::f64::consts::{FRAC_PI_2, PI};

const R: f64 = 0.5;
const SIZE: Vec2 = vec2(1920.0, 1080.0);
const NONE: Modifiers = Modifiers::NONE;

// ---- Hilfen ohne Oberfläche ---------------------------------------------------------------------

fn at(x: f64, y: f64) -> DVec2 {
    DVec2::new(x, y)
}

fn close(a: DVec2, b: DVec2) -> bool {
    a.distance(b) < 1e-6
}

fn session(kind: ToolKind) -> (SketchSession, Sketch) {
    let mut sk = Sketch::new(Frame::WORLD, "S", Tolerance::default());
    let mut se = SketchSession::new(FeatureId(0));
    se.choose_kind(&mut sk, kind);
    (se, sk)
}

fn click(se: &mut SketchSession, sk: &mut Sketch, x: f64, y: f64) {
    se.click(sk, at(x, y), R, false).unwrap();
}

fn line_pts(sk: &mut Sketch, a: (f64, f64), b: (f64, f64)) -> (CurveId, PointId, PointId) {
    let a = sk.add_point(a.0, a.1).unwrap();
    let b = sk.add_point(b.0, b.1).unwrap();
    (sk.add_line(a, b).unwrap(), a, b)
}

fn line(sk: &mut Sketch, a: (f64, f64), b: (f64, f64)) -> CurveId {
    line_pts(sk, a, b).0
}

fn bits(sk: &Sketch) -> (u64, Vec<u64>) {
    (
        sk.revision(),
        sk.variables().iter().map(|v| v.to_bits()).collect(),
    )
}

fn report(sk: &Sketch) -> SolveReport {
    sk.clone().solve()
}

fn assert_solved(sk: &Sketch) -> SolveReport {
    let r = report(sk);
    assert_eq!(r.status, SolveStatus::Solved);
    assert!(r.redundant.is_empty(), "{:?}", r.redundant);
    r
}

fn count(sk: &Sketch, pick: impl Fn(&K) -> bool) -> usize {
    sk.constraints().filter(|(_, c)| pick(&c.kind)).count()
}

fn xy(sk: &Sketch, p: PointId) -> DVec2 {
    sk.point_xy(p).unwrap()
}

/// Ein fertiger Bogen, wie er gespeichert ist.
#[derive(Clone, Copy, Debug)]
struct Stored {
    id: CurveId,
    start_id: PointId,
    end_id: PointId,
    center: DVec2,
    start: DVec2,
    end: DVec2,
}

impl Stored {
    fn sweep(&self) -> f64 {
        ArcPlan {
            center: self.center,
            start: self.start,
            end: self.end,
        }
        .sweep()
    }

    fn radius(&self) -> f64 {
        self.start.distance(self.center)
    }
}

fn last_arc(sk: &Sketch) -> Stored {
    let (id, center, start, end) = sk
        .curves()
        .filter_map(|(c, d)| match d.kind {
            CurveKind::Arc { center, start, end } => Some((c, center, start, end)),
            _ => None,
        })
        .last()
        .expect("Bogen");
    Stored {
        id,
        start_id: start,
        end_id: end,
        center: xy(sk, center),
        start: xy(sk, start),
        end: xy(sk, end),
    }
}

fn arc_count(sk: &Sketch) -> usize {
    sk.curves()
        .filter(|(_, d)| matches!(d.kind, CurveKind::Arc { .. }))
        .count()
}

fn plan(se: &SketchSession, sk: &Sketch, x: f64, y: f64) -> ArcPlan {
    match se.kind_plan(sk, at(x, y)) {
        Some(ShapePlan::Arc(a)) => a,
        other => panic!("kein Bogenplan: {other:?}"),
    }
}

fn assert_same_arc(got: &Stored, want: &ArcPlan) {
    assert!(close(got.center, want.center), "{got:?} {want:?}");
    assert!(close(got.start, want.start), "{got:?} {want:?}");
    assert!(close(got.end, want.end), "{got:?} {want:?}");
}

/// Der Klick wird abgelehnt: Meldung, Skizze bitgleich, die Form steht, wo sie stand.
fn expect_refused(se: &mut SketchSession, sk: &mut Sketch, raw: DVec2, key: &str) {
    let before = bits(sk);
    let (pos, stage) = (se.pending_pos.clone(), se.kind_stage());
    let text = se.click(sk, raw, R, false).unwrap_err();
    assert_eq!(text, t(key));
    assert_eq!(se.message_text().as_deref(), Some(t(key).as_str()));
    assert_eq!(bits(sk), before, "Skizze bitgleich");
    assert_eq!(se.pending_pos, pos, "Lagen bleiben");
    assert_eq!(se.kind_stage(), stage, "Stufe bleibt");
    assert!(se.created.is_none());
}

/// Der Radius zum Punkt `p` steht senkrecht auf `leaving`: Das hält die Tangente fest.
fn assert_tangent(arc: &Stored, p: DVec2, leaving: DVec2) {
    let radial = p - arc.center;
    assert!(
        radial.dot(leaving).abs() < 1e-6 * radial.length().max(1.0),
        "nicht tangential: {arc:?}"
    );
}

/// Der Bogen läuft tangential in der Abgangsrichtung `leaving` aus `p` heraus (kein Knick zurück).
fn assert_leaves_along(arc: &Stored, p: DVec2, leaving: DVec2) {
    assert_tangent(arc, p, leaving);
    // Gespeichert läuft der Bogen gegen den Uhrzeigersinn: Am Start ist das seine Richtung, am Ende
    // die Gegenrichtung.
    let ccw = (p - arc.center).perp();
    let along = if close(p, arc.start) { ccw } else { -ccw };
    assert!(along.dot(leaving) > 0.0, "läuft zurück: {arc:?}");
}

// ---- Hilfen mit getippten Werten ----------------------------------------------------------------

fn rig(kind: ToolKind) -> Workspace {
    let root = common::temp_root("planora-sketch-kinds-arc");
    let mut w = Workspace::new(Some(Library::open(&root).unwrap()), Config::default());
    let s = &mut w.active_tab_mut().state;
    let sketch = s
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    s.open_sketch(sketch).unwrap();
    w.observe_history(false);
    let (se, sf, _) = parts(&mut w);
    se.choose_kind(&mut sf.sketch, kind);
    w
}

fn parts(w: &mut Workspace) -> (&mut SketchSession, &mut SketchFeature, ParamScope) {
    let s = &mut w.active_tab_mut().state;
    let scope = s.document.param_scope();
    let (se, sf) = s.session_draft_mut().unwrap();
    (se, sf, scope)
}

fn commit(w: &mut Workspace, x: f64, y: f64) -> Result<(), String> {
    let (se, sf, scope) = parts(w);
    se.commit_input(sf, at(x, y), R, false, &scope)
}

fn type_in(w: &mut Workspace, kind: F, text: &str) {
    let (se, sf, scope) = parts(w);
    se.sync_input(&sf.sketch);
    let i = se
        .input
        .fields
        .iter()
        .position(|f| f.kind == kind)
        .unwrap_or_else(|| panic!("Feld {kind:?} fehlt"));
    se.type_field(i, text, &scope);
}

fn resolved(w: &mut Workspace, x: f64, y: f64) -> DVec2 {
    let (se, sf, _) = parts(w);
    se.resolve_cursor(&sf.sketch, at(x, y), R).at
}

fn sk(w: &Workspace) -> &Sketch {
    w.active_tab().state.session_sketch().unwrap()
}

fn sk_mut(w: &mut Workspace) -> &mut Sketch {
    &mut parts(w).1.sketch
}

fn dims(sk: &Sketch) -> Vec<K> {
    sk.constraints()
        .filter(|(_, c)| c.kind.is_dimension())
        .map(|(_, c)| c.kind.clone())
        .collect()
}

fn construction_lines(sk: &Sketch) -> usize {
    sk.curves()
        .filter(|(_, d)| matches!(d.kind, CurveKind::Line { .. }) && d.construction)
        .count()
}

// ---- Bogen 3 Punkte -----------------------------------------------------------------------------

#[test]
fn arc_3p_makes_the_quarter_arc_with_the_ends_where_they_were_clicked() {
    let (mut se, mut sk) = session(ToolKind::Arc3P);
    let cut = 10.0 / 2f64.sqrt();
    assert!(se.kind_preview(&sk, at(5.0, 5.0)).is_none(), "Stufe 0");
    click(&mut se, &mut sk, 10.0, 0.0);
    assert!(se.kind_preview(&sk, at(5.0, 5.0)).is_none(), "Stufe 1");
    click(&mut se, &mut sk, 0.0, 10.0);
    assert_eq!(sk.curves().count(), 0, "bis zum letzten Klick nichts");
    assert!(se.kind_preview(&sk, at(cut, cut)).is_some(), "Stufe 2");
    let want = plan(&se, &sk, cut, cut);
    click(&mut se, &mut sk, cut, cut);

    let arc = last_arc(&sk);
    assert_same_arc(&arc, &want);
    assert!((arc.sweep() - FRAC_PI_2).abs() < 1e-9);
    assert!(close(arc.start, at(10.0, 0.0)) && close(arc.end, at(0.0, 10.0)));
    assert!(close(arc.center, at(0.0, 0.0)));
    let r = assert_solved(&sk);
    assert_eq!(r.dof, 5);
    assert_eq!(se.last_report.as_ref().unwrap().iterations, 0);
    assert!(se.pending_pos.is_empty() && se.pending_snaps.is_empty());
    assert!(se.created.is_some());
}

#[test]
fn arc_3p_through_the_other_side_is_the_long_way_and_keeps_the_ends() {
    let (mut se, mut sk) = session(ToolKind::Arc3P);
    let cut = 10.0 / 2f64.sqrt();
    click(&mut se, &mut sk, 10.0, 0.0);
    click(&mut se, &mut sk, 0.0, 10.0);
    let want = plan(&se, &sk, -cut, -cut);
    click(&mut se, &mut sk, -cut, -cut);
    let arc = last_arc(&sk);
    assert_same_arc(&arc, &want);
    assert!((arc.sweep() - 3.0 * FRAC_PI_2).abs() < 1e-9);
    // Gegen den Uhrzeigersinn gespeichert, also getauscht; die Enden liegen trotzdem an den Klicks.
    assert!(close(arc.start, at(0.0, 10.0)) && close(arc.end, at(10.0, 0.0)));
    assert_solved(&sk);
}

#[test]
fn arc_3p_snaps_share_the_points_at_both_ends_and_the_third_binds_on_the_rim() {
    for (x, y, swapped) in [(7.0, 7.0, false), (-7.0, -7.0, true)] {
        let (mut se, mut sk) = session(ToolKind::Arc3P);
        let (_, _, s) = line_pts(&mut sk, (20.0, 0.0), (10.0, 0.0));
        let (_, _, e) = line_pts(&mut sk, (0.0, 20.0), (0.0, 10.0));
        let through = sk.add_point(x, y).unwrap();
        click(&mut se, &mut sk, 10.1, 0.1);
        click(&mut se, &mut sk, 0.1, 10.1);
        click(&mut se, &mut sk, x + 0.1, y + 0.1);
        let arc = last_arc(&sk);
        let (first, second) = if swapped { (e, s) } else { (s, e) };
        assert_eq!(arc.start_id, first);
        assert_eq!(arc.end_id, second);
        assert_eq!(
            count(
                &sk,
                |k| matches!(k, K::PointOnCurve(p, c) if *p == through && *c == arc.id)
            ),
            1
        );
        assert_eq!(count(&sk, |k| matches!(k, K::Coincident(..))), 0);
        assert_solved(&sk);
    }
}

#[test]
fn arc_3p_with_ctrl_binds_nothing() {
    let (mut se, mut sk) = session(ToolKind::Arc3P);
    let p = sk.add_point(10.0, 0.0).unwrap();
    se.cursor_opts.suppressed = true;
    click(&mut se, &mut sk, 10.1, 0.0);
    click(&mut se, &mut sk, 0.0, 10.0);
    click(&mut se, &mut sk, 7.0, 7.0);
    assert_ne!(last_arc(&sk).start_id, p);
}

#[test]
fn arc_3p_radius_makes_a_dimension_and_is_exact() {
    let mut w = rig(ToolKind::Arc3P);
    commit(&mut w, 10.0, 0.0).unwrap();
    commit(&mut w, 0.0, 10.0).unwrap();
    type_in(&mut w, F::Radius, "10");
    let cut = 10.0 / 2f64.sqrt();
    assert!(close(resolved(&mut w, 7.0, 7.0), at(cut, cut)));
    commit(&mut w, 7.0, 7.0).unwrap();
    let sketch = sk(&w);
    let arc = last_arc(sketch);
    assert!((arc.radius() - 10.0).abs() < 1e-9);
    assert!((arc.sweep() - FRAC_PI_2).abs() < 1e-9);
    assert_eq!(dims(sketch).len(), 1);
    assert_eq!(
        count(
            sketch,
            |k| matches!(k, K::Radius(c, d) if *c == arc.id && *d == 10.0)
        ),
        1
    );
    assert_solved(sketch);
}

#[test]
fn arc_3p_radius_side_comes_from_the_pointer_and_far_means_the_large_arc() {
    // Sehne (10,0)–(0,10), Mitte (5,5); Zeiger auf der einen oder anderen Seite, nah oder weit.
    for (x, y, sweep) in [
        (7.0, 7.0, FRAC_PI_2),
        (3.0, 3.0, FRAC_PI_2),
        (20.0, 20.0, 3.0 * FRAC_PI_2),
        (-10.0, -10.0, 3.0 * FRAC_PI_2),
    ] {
        let mut w = rig(ToolKind::Arc3P);
        commit(&mut w, 10.0, 0.0).unwrap();
        commit(&mut w, 0.0, 10.0).unwrap();
        type_in(&mut w, F::Radius, "10");
        let want = {
            let p = resolved(&mut w, x, y);
            let (se, sf, _) = parts(&mut w);
            match se.kind_plan(&sf.sketch, p) {
                Some(ShapePlan::Arc(a)) => a,
                other => panic!("{other:?}"),
            }
        };
        commit(&mut w, x, y).unwrap();
        let arc = last_arc(sk(&w));
        assert_same_arc(&arc, &want);
        assert!((arc.radius() - 10.0).abs() < 1e-9, "{x} {y}");
        assert!(
            (arc.sweep() - sweep).abs() < 1e-9,
            "{x} {y}: {}",
            arc.sweep()
        );
        // Die Seite des Durchgangs ist die des Zeigers.
        let mid = at(5.0, 5.0);
        let n = at(1.0, 1.0);
        let side = (mid - arc.center).dot(n);
        let bulge = if arc.sweep() < PI { side } else { -side };
        assert!(bulge * (at(x, y) - mid).dot(n) > 0.0, "{x} {y}");
    }
}

#[test]
fn arc_3p_radius_of_exactly_half_the_chord_is_the_half_circle() {
    let mut w = rig(ToolKind::Arc3P);
    commit(&mut w, 20.0, 5.0).unwrap();
    commit(&mut w, 0.0, 5.0).unwrap();
    type_in(&mut w, F::Radius, "10");
    commit(&mut w, 10.0, 9.0).unwrap();
    let arc = last_arc(sk(&w));
    assert!((arc.radius() - 10.0).abs() < 1e-9);
    assert!((arc.sweep() - PI).abs() < 1e-9);
}

#[test]
fn arc_3p_radius_below_half_the_chord_is_refused_and_the_shape_runs_on() {
    let mut w = rig(ToolKind::Arc3P);
    commit(&mut w, 10.0, 0.0).unwrap();
    commit(&mut w, 0.0, 10.0).unwrap();
    type_in(&mut w, F::Radius, "5");
    let before = bits(sk(&w));
    {
        let (se, sf, _) = parts(&mut w);
        let p = se.resolve_cursor(&sf.sketch, at(7.0, 7.0), R).at;
        assert!(se.kind_preview(&sf.sketch, p).is_none(), "keine Vorschau");
        assert!(se.kind_plan(&sf.sketch, p).is_none());
    }
    let text = commit(&mut w, 7.0, 7.0).unwrap_err();
    assert_eq!(text, t("sketch-arc-radius-chord"));
    assert_eq!(
        parts(&mut w).0.message_text().as_deref(),
        Some(t("sketch-arc-radius-chord").as_str())
    );
    assert_eq!(bits(sk(&w)), before, "Skizze bitgleich");
    assert_eq!(sk(&w).curves().count(), 0);
    assert_eq!(parts(&mut w).0.kind_stage(), 2, "die Form läuft weiter");
    type_in(&mut w, F::Radius, "10");
    commit(&mut w, 7.0, 7.0).unwrap();
    assert_eq!(arc_count(sk(&w)), 1);
}

#[test]
fn arc_3p_refuses_start_on_end_and_a_straight_through() {
    let (mut se, mut sk) = session(ToolKind::Arc3P);
    click(&mut se, &mut sk, 1.0, 1.0);
    expect_refused(&mut se, &mut sk, at(1.0, 1.0), "sketch-arc-end-on-start");
    click(&mut se, &mut sk, 11.0, 1.0);
    assert!(se.kind_preview(&sk, at(6.0, 1.0)).is_none());
    expect_refused(&mut se, &mut sk, at(6.0, 1.0), "sketch-circle-collinear");
    expect_refused(&mut se, &mut sk, at(1.0, 1.0), "sketch-circle-collinear");
    click(&mut se, &mut sk, 6.0, 4.0);
    assert_eq!(arc_count(&sk), 1);
}

#[test]
fn arc_3p_field_shows_the_radius_of_the_preview() {
    let mut w = rig(ToolKind::Arc3P);
    commit(&mut w, 10.0, 0.0).unwrap();
    commit(&mut w, 0.0, 10.0).unwrap();
    let cut = 10.0 / 2f64.sqrt();
    let (se, sf, _) = parts(&mut w);
    se.update_cursor(&sf.sketch, Some(at(cut, cut)), R);
    let v = se.field_value(&sf.sketch, F::Radius).unwrap();
    assert!((v - 10.0).abs() < 1e-9, "{v}");
}

// ---- Tangential: Start --------------------------------------------------------------------------

/// Linie (2,0)–(10,0): abseits des Ursprungs, damit er nicht fängt.
fn tangent_on_line() -> (SketchSession, Sketch, CurveId, PointId, PointId) {
    let (se, mut sk) = session(ToolKind::ArcTangent);
    let (l, a, b) = line_pts(&mut sk, (2.0, 0.0), (10.0, 0.0));
    (se, sk, l, a, b)
}

#[test]
fn tangent_start_needs_the_end_of_a_line_or_arc() {
    let (mut se, mut sk) = session(ToolKind::ArcTangent);
    line(&mut sk, (2.0, 0.0), (10.0, 0.0));
    let free = sk.add_point(30.0, 30.0).unwrap();
    let m = sk.add_point(50.0, 50.0).unwrap();
    sk.add_circle(m, 5.0).unwrap();

    let msg = "sketch-arc-tangent-start";
    // Freie Fläche.
    expect_refused(&mut se, &mut sk, at(20.0, 20.0), msg);
    // Ein freier Punkt.
    let spot = xy(&sk, free);
    expect_refused(&mut se, &mut sk, spot, msg);
    // Die Mitte einer Linie.
    let mid = se.resolve_cursor(&sk, at(6.0, 0.0), R);
    assert!(matches!(mid.snap, Snap::Midpoint(_)), "{:?}", mid.snap);
    expect_refused(&mut se, &mut sk, at(6.0, 0.0), msg);
    // Eine Kreismitte.
    let center = se.resolve_cursor(&sk, at(50.0, 50.0), R);
    assert!(matches!(center.snap, Snap::Center(_) | Snap::Point(_)));
    expect_refused(&mut se, &mut sk, at(50.0, 50.0), msg);
    assert_eq!(se.kind_stage(), 0);
    assert!(se.auto_tangent.is_none());
}

#[test]
fn tangent_start_with_ctrl_has_no_snap_and_is_refused() {
    let (mut se, mut sk, ..) = tangent_on_line();
    se.cursor_opts.suppressed = true;
    expect_refused(&mut se, &mut sk, at(10.0, 0.0), "sketch-arc-tangent-start");
}

#[test]
fn tangent_start_changes_nothing_and_remembers_the_curve() {
    let (mut se, mut sk, l, ..) = tangent_on_line();
    let before = bits(&sk);
    click(&mut se, &mut sk, 10.1, 0.1);
    assert_eq!(se.kind_stage(), 1);
    assert_eq!(se.pending_pos, [at(10.0, 0.0)]);
    assert_eq!(se.auto_tangent, Some(l));
    assert_eq!(bits(&sk), before);
}

#[test]
fn tangent_start_takes_the_youngest_curve_at_the_point() {
    let (mut se, mut sk) = session(ToolKind::ArcTangent);
    let (_, _, p) = line_pts(&mut sk, (2.0, 0.0), (10.0, 0.0));
    let q = sk.add_point(10.0, 8.0).unwrap();
    let young = sk.add_line(q, p).unwrap();
    click(&mut se, &mut sk, 10.0, 0.0);
    assert_eq!(se.auto_tangent, Some(young));
}

/// Die Hilfslinien eines getippten Öffnungswinkels sind jünger als der Bogen, dürfen ihn aber nicht
/// als Vorgänger verdrängen.
#[test]
fn tangent_start_prefers_a_real_curve_over_younger_helper_lines() {
    let mut w = rig(ToolKind::ArcTangent);
    line(sk_mut(&mut w), (2.0, 0.0), (10.0, 0.0));
    commit(&mut w, 10.0, 0.0).unwrap();
    type_in(&mut w, F::Radius, "5");
    type_in(&mut w, F::Sweep, "90");
    commit(&mut w, 15.0, 5.0).unwrap();
    let arc = last_arc(sk(&w));
    assert_eq!(construction_lines(sk(&w)), 2);
    commit(&mut w, arc.end.x, arc.end.y).unwrap();
    assert_eq!(parts(&mut w).0.auto_tangent, Some(arc.id));
}

/// Nur eine Konstruktionslinie endet dort: Auch daran läuft der Bogen an.
#[test]
fn tangent_start_also_works_on_a_construction_line() {
    let (mut se, mut sk, l, ..) = tangent_on_line();
    sk.set_construction(l, true).unwrap();
    click(&mut se, &mut sk, 10.0, 0.0);
    assert_eq!(se.auto_tangent, Some(l));
}

// ---- Tangential: Ergebnis -----------------------------------------------------------------------

#[test]
fn tangent_left_of_a_line_end_preview_equals_the_result_and_nothing_moves() {
    let (mut se, mut sk, l, a, b) = tangent_on_line();
    click(&mut se, &mut sk, 10.0, 0.0);
    let want = plan(&se, &sk, 15.0, 5.0);
    assert!(se.kind_preview(&sk, at(15.0, 5.0)).is_some());
    let (pa, pb) = (xy(&sk, a), xy(&sk, b));
    click(&mut se, &mut sk, 15.0, 5.0);

    let arc = last_arc(&sk);
    assert_same_arc(&arc, &want);
    assert!(close(arc.center, at(10.0, 5.0)));
    assert_eq!(arc.start_id, b, "am Linienende geteilt, nicht getauscht");
    assert!(close(arc.end, at(15.0, 5.0)));
    assert_eq!(
        count(
            &sk,
            |k| matches!(k, K::Tangent(x, y) if *x == arc.id && *y == l)
        ),
        1
    );
    assert_leaves_along(&arc, at(10.0, 0.0), DVec2::X);
    assert_solved(&sk);
    assert_eq!(
        se.last_report.as_ref().unwrap().iterations,
        0,
        "nichts lief"
    );
    assert_eq!(xy(&sk, a), pa);
    assert_eq!(xy(&sk, b), pb);
    assert!(se.auto_tangent.is_none() && se.pending_pos.is_empty());
    assert!(se.created.is_some());
}

#[test]
fn tangent_right_of_a_line_end_is_stored_swapped_and_still_tangent() {
    let (mut se, mut sk, l, _, b) = tangent_on_line();
    click(&mut se, &mut sk, 10.0, 0.0);
    let want = plan(&se, &sk, 15.0, -5.0);
    click(&mut se, &mut sk, 15.0, -5.0);
    let arc = last_arc(&sk);
    assert_same_arc(&arc, &want);
    assert!(close(arc.center, at(10.0, -5.0)));
    assert_eq!(
        arc.end_id, b,
        "gegen den Uhrzeigersinn: Start und Ende getauscht"
    );
    assert!(close(arc.start, at(15.0, -5.0)));
    assert_eq!(
        count(
            &sk,
            |k| matches!(k, K::Tangent(x, y) if *x == arc.id && *y == l)
        ),
        1
    );
    assert_leaves_along(&arc, at(10.0, 0.0), DVec2::X);
    assert_solved(&sk);
    assert_eq!(se.last_report.as_ref().unwrap().iterations, 0);
}

/// Die Länge `length` an der Linie aus [`tangent_on_line`] nach einem Bogen nach links oder rechts.
fn arc_after_line_length(y: f64, length: f64) -> (Stored, DVec2, DVec2) {
    let (mut se, mut sk, _, a, b) = tangent_on_line();
    click(&mut se, &mut sk, 10.0, 0.0);
    click(&mut se, &mut sk, 15.0, y);
    // Skizzen 3, Task 10: über die App, also mit dem Falt-Wächter.
    let id = sk.add_constraint(K::Distance(a, b, 8.0)).unwrap();
    assert_eq!(sk.solve().status, SolveStatus::Solved);
    se.set_dimension_value(&mut sk, id, length).unwrap();
    let r = sk.clone().solve();
    assert_eq!(r.status, SolveStatus::Solved);
    assert!(r.redundant.is_empty(), "{:?}", r.redundant);
    let (pa, pb) = (xy(&sk, a), xy(&sk, b));
    assert!((pa.distance(pb) - length).abs() < 1e-6);
    let arc = last_arc(&sk);
    assert!(arc.start_id == b || arc.end_id == b);
    (arc, pa, pb)
}

#[test]
fn tangent_stays_tangent_when_the_line_gets_a_length() {
    // Skizzen 3, Task 10: Auch bei 100 läuft er in derselben Richtung weiter: Die Zeile am
    // gemeinsamen Ende hat nur noch eine Nullstelle, die Spitze ist keine.
    for y in [5.0, -5.0] {
        for length in [12.0, 20.0, 50.0, 100.0, 300.0] {
            let (arc, pa, pb) = arc_after_line_length(y, length);
            assert_leaves_along(&arc, pb, (pb - pa).normalize());
        }
    }
}

/// Kein Zusammenfallen: Der Radius bleibt über `least` und wächst bei langer Linie mit ihr (eine
/// Spitze zöge ihn auf wenige Millimeter), ohne sie zu überholen.
#[track_caller]
fn assert_sensible_radius(r: f64, least: f64, length: f64, y: f64) {
    assert!(r > least && r < length, "y={y} L={length}: r={r}");
    assert!(
        length < 100.0 || r > 0.2 * length,
        "y={y} L={length}: r={r}"
    );
}

/// Jede Länge bis 120 und jede zehnte bis 300; je Fall ein frischer Stand bei 8 mm.
fn dense_lengths() -> impl Iterator<Item = f64> {
    (9..=120).chain((130..=300).step_by(10)).map(f64::from)
}

#[test]
fn a_line_length_sweep_never_refuses_and_keeps_the_direction_of_a_tangent_arc() {
    for y in [5.0, -5.0, 2.0, -2.0] {
        for length in dense_lengths() {
            let (arc, pa, pb) = arc_after_line_length(y, length);
            assert_leaves_along(&arc, pb, (pb - pa).normalize());
            let r = arc.center.distance(pb);
            assert_sensible_radius(r, 1.0, length, y);
        }
    }
}

#[test]
fn a_line_length_sweep_never_refuses_and_keeps_the_direction_of_a_center_arc() {
    for y in [5.0, -5.0, 2.0, -2.0] {
        for length in dense_lengths() {
            // Mittelpunkt-Bogen wie der tangentiale: Mitte über dem Linienende, Lage von `y`.
            let (mut se, mut sk, l, a, b) = tangent_on_line();
            let center = sk.add_point(10.0, y).unwrap();
            let free = sk.add_point(10.0 + y.abs(), y).unwrap();
            let arc = if y > 0.0 {
                sk.add_arc(center, b, free).unwrap()
            } else {
                sk.add_arc(center, free, b).unwrap()
            };
            sk.add_constraint(K::Tangent(arc, l)).unwrap();
            let id = sk.add_constraint(K::Distance(a, b, 8.0)).unwrap();
            assert_eq!(sk.solve().status, SolveStatus::Solved);
            se.set_dimension_value(&mut sk, id, length)
                .unwrap_or_else(|e| panic!("y={y} L={length}: {e}"));
            assert_eq!(sk.clone().solve().status, SolveStatus::Solved);
            let (pa, pb) = (xy(&sk, a), xy(&sk, b));
            assert!((pa.distance(pb) - length).abs() < 1e-6, "y={y} L={length}");
            let stored = last_arc(&sk);
            assert_leaves_along(&stored, pb, (pb - pa).normalize());
            let r = stored.center.distance(pb);
            assert_sensible_radius(r, 0.25 * y.abs(), length, y);
        }
    }
}

#[test]
fn tangent_at_the_end_of_an_arc_runs_on_in_its_direction() {
    // Bogen um (30,30): Anfang (40,30), Ende (30,40). Am Ende geht es nach −x, am Anfang nach −y.
    let (mut se, mut sk) = session(ToolKind::ArcTangent);
    let m = sk.add_point(30.0, 30.0).unwrap();
    let s = sk.add_point(40.0, 30.0).unwrap();
    let e = sk.add_point(30.0, 40.0).unwrap();
    let first = sk.add_arc(m, s, e).unwrap();

    click(&mut se, &mut sk, 30.0, 40.0);
    assert_eq!(se.auto_tangent, Some(first));
    let want = plan(&se, &sk, 25.0, 45.0);
    click(&mut se, &mut sk, 25.0, 45.0);
    let arc = last_arc(&sk);
    assert_same_arc(&arc, &want);
    assert!(close(arc.center, at(30.0, 45.0)), "{arc:?}");
    assert_eq!(arc.end_id, e);
    assert_leaves_along(&arc, at(30.0, 40.0), -DVec2::X);
    assert_eq!(count(&sk, |k| matches!(k, K::Tangent(..))), 1);
    assert_solved(&sk);
    assert_eq!(se.last_report.as_ref().unwrap().iterations, 0);

    click(&mut se, &mut sk, 40.0, 30.0);
    let want = plan(&se, &sk, 45.0, 25.0);
    click(&mut se, &mut sk, 45.0, 25.0);
    let arc = last_arc(&sk);
    assert_same_arc(&arc, &want);
    assert_leaves_along(&arc, at(40.0, 30.0), -DVec2::Y);
    assert_solved(&sk);
}

#[test]
fn tangent_at_the_start_of_a_line_runs_against_it() {
    let (mut se, mut sk, l, a, _) = tangent_on_line();
    click(&mut se, &mut sk, 2.0, 0.0);
    assert_eq!(se.auto_tangent, Some(l));
    let want = plan(&se, &sk, -3.0, 5.0);
    click(&mut se, &mut sk, -3.0, 5.0);
    let arc = last_arc(&sk);
    assert_same_arc(&arc, &want);
    assert!(close(arc.center, at(2.0, 5.0)), "{arc:?}");
    assert_eq!(arc.end_id, a);
    assert_leaves_along(&arc, at(2.0, 0.0), -DVec2::X);
    assert_solved(&sk);
}

#[test]
fn tangent_end_on_an_existing_point_shares_it_and_on_a_curve_binds() {
    let (mut se, mut sk, _, _, _) = tangent_on_line();
    let p = sk.add_point(15.0, 5.0).unwrap();
    click(&mut se, &mut sk, 10.0, 0.0);
    click(&mut se, &mut sk, 15.1, 5.1);
    let arc = last_arc(&sk);
    assert_eq!(arc.end_id, p);
    assert_solved(&sk);

    let (mut se, mut sk, _, _, _) = tangent_on_line();
    let other = line(&mut sk, (12.0, 30.0), (40.0, 30.0));
    click(&mut se, &mut sk, 10.0, 0.0);
    click(&mut se, &mut sk, 20.0, 30.0);
    let arc = last_arc(&sk);
    assert_eq!(
        count(
            &sk,
            |k| matches!(k, K::PointOnCurve(q, c) if *q == arc.end_id && *c == other)
        ),
        1
    );
    assert_solved(&sk);
}

#[test]
fn tangent_with_radius_and_sweep_makes_both_dimensions_on_either_side() {
    for (x, y, end, center) in [
        (15.0, 3.0, at(15.0, 5.0), at(10.0, 5.0)),
        (15.0, -3.0, at(15.0, -5.0), at(10.0, -5.0)),
    ] {
        let mut w = rig(ToolKind::ArcTangent);
        line(sk_mut(&mut w), (2.0, 0.0), (10.0, 0.0));
        commit(&mut w, 10.0, 0.0).unwrap();
        type_in(&mut w, F::Radius, "5");
        type_in(&mut w, F::Sweep, "90");
        assert!(close(resolved(&mut w, x, y), end));
        commit(&mut w, x, y).unwrap();

        let sketch = sk(&w);
        let arc = last_arc(sketch);
        assert!(close(arc.center, center), "{arc:?}");
        assert!((arc.radius() - 5.0).abs() < 1e-9);
        assert!((arc.sweep() - FRAC_PI_2).abs() < 1e-9);
        assert_eq!(dims(sketch).len(), 2, "{:?}", dims(sketch));
        assert_eq!(
            count(
                sketch,
                |k| matches!(k, K::Radius(c, d) if *c == arc.id && *d == 5.0)
            ),
            1
        );
        assert_eq!(count(sketch, |k| matches!(k, K::Angle(..))), 1);
        assert_eq!(construction_lines(sketch), 2, "Hilfslinien wie der Bestand");
        assert_eq!(count(sketch, |k| matches!(k, K::Tangent(..))), 1);
        assert_leaves_along(&arc, at(10.0, 0.0), DVec2::X);
        assert_solved(sketch);
    }
}

#[test]
fn tangent_with_only_a_sweep_takes_the_radius_from_the_pointer() {
    let mut w = rig(ToolKind::ArcTangent);
    line(sk_mut(&mut w), (2.0, 0.0), (10.0, 0.0));
    commit(&mut w, 10.0, 0.0).unwrap();
    type_in(&mut w, F::Sweep, "90");
    let p = resolved(&mut w, 15.0, 5.0);
    assert!(close(p, at(15.0, 5.0)), "{p:?}");
    commit(&mut w, 15.0, 5.0).unwrap();
    let sketch = sk(&w);
    assert!((last_arc(sketch).sweep() - FRAC_PI_2).abs() < 1e-9);
    assert_eq!(count(sketch, |k| matches!(k, K::Angle(..))), 1);
    assert_solved(sketch);
}

#[test]
fn tangent_with_only_a_radius_ends_on_the_circle_toward_the_pointer() {
    let mut w = rig(ToolKind::ArcTangent);
    line(sk_mut(&mut w), (2.0, 0.0), (10.0, 0.0));
    commit(&mut w, 10.0, 0.0).unwrap();
    type_in(&mut w, F::Radius, "5");
    let p = resolved(&mut w, 30.0, 5.0);
    assert!(close(p, at(15.0, 5.0)), "{p:?}");
    commit(&mut w, 30.0, 5.0).unwrap();
    assert!((last_arc(sk(&w)).radius() - 5.0).abs() < 1e-9);
    assert_solved(sk(&w));
}

#[test]
fn tangent_field_values_show_the_preview() {
    let mut w = rig(ToolKind::ArcTangent);
    line(sk_mut(&mut w), (2.0, 0.0), (10.0, 0.0));
    commit(&mut w, 10.0, 0.0).unwrap();
    let (se, sf, _) = parts(&mut w);
    se.update_cursor(&sf.sketch, Some(at(15.0, -5.0)), R);
    let radius = se.field_value(&sf.sketch, F::Radius).unwrap();
    let sweep = se.field_value(&sf.sketch, F::Sweep).unwrap();
    assert!((radius - 5.0).abs() < 1e-9, "{radius}");
    assert!((sweep - FRAC_PI_2).abs() < 1e-9, "{sweep}");
}

#[test]
fn tangent_with_the_pointer_on_the_tangent_has_no_preview_and_says_so() {
    let (mut se, mut sk, ..) = tangent_on_line();
    click(&mut se, &mut sk, 10.0, 0.0);
    assert!(se.kind_preview(&sk, at(20.0, 0.0)).is_none());
    expect_refused(
        &mut se,
        &mut sk,
        at(20.0, 0.0),
        "sketch-arc-tangent-straight",
    );
    // Auch rückwärts auf der Geraden.
    expect_refused(
        &mut se,
        &mut sk,
        at(-5.0, 0.0),
        "sketch-arc-tangent-straight",
    );
    // Die Form läuft weiter.
    click(&mut se, &mut sk, 15.0, 5.0);
    assert_eq!(arc_count(&sk), 1);
}

#[test]
fn tangent_with_a_typed_value_on_the_tangent_has_no_preview_either() {
    let mut w = rig(ToolKind::ArcTangent);
    line(sk_mut(&mut w), (2.0, 0.0), (10.0, 0.0));
    commit(&mut w, 10.0, 0.0).unwrap();
    type_in(&mut w, F::Radius, "5");
    let before = bits(sk(&w));
    {
        let (se, sf, _) = parts(&mut w);
        let p = se.resolve_cursor(&sf.sketch, at(20.0, 0.0), R).at;
        assert!(se.kind_preview(&sf.sketch, p).is_none());
    }
    let text = commit(&mut w, 20.0, 0.0).unwrap_err();
    assert_eq!(text, t("sketch-arc-tangent-straight"));
    assert_eq!(bits(sk(&w)), before);
    assert_eq!(parts(&mut w).0.kind_stage(), 1);
}

// ---- Fahrten über Rohereignisse bei 1920×1080 ----------------------------------------------------

fn button(p: Pos2, pressed: bool) -> Event {
    Event::PointerButton {
        pos: p,
        button: PointerButton::Primary,
        pressed,
        modifiers: NONE,
    }
}

struct Gui {
    ctx: egui::Context,
    app: PlanoraApp,
    time: f64,
}

impl Gui {
    fn new() -> Gui {
        let root = common::temp_root("planora-sketch-kinds-arc-gui");
        let mut w = Workspace::new(Some(Library::open(&root).unwrap()), Config::default());
        w.config_path = root.join("config.toml");
        let s = &mut w.active_tab_mut().state;
        let sk = s
            .add_sketch(SketchPlane::Standard(StandardPlane::XY))
            .unwrap();
        s.open_sketch(sk).unwrap();
        w.observe_history(false);
        let ctx = egui::Context::default();
        ctx.set_fonts(fonts::definitions());
        theme::apply(&ctx, true);
        PlanoraApp::setup_context(&ctx);
        let mut gui = Gui {
            ctx,
            app: PlanoraApp::for_tests(w),
            time: 0.0,
        };
        gui.settle();
        gui
    }

    fn frame(&mut self, events: Vec<Event>) {
        self.time += 1.0 / 60.0;
        let raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, SIZE)),
            time: Some(self.time),
            events,
            ..Default::default()
        };
        let mut f = eframe::Frame::_new_kittest();
        let app = &mut self.app;
        let out = self.ctx.run_ui(raw, |ui| app.ui(ui, &mut f));
        out.drop_without_applying_deltas();
    }

    fn settle(&mut self) {
        for _ in 0..3 {
            self.frame(vec![]);
        }
    }

    fn click(&mut self, x: f32, y: f32) {
        let p = pos2(x, y);
        // Weit genug auseinander, dass nichts als Doppelklick zählt.
        self.time += 1.0;
        self.frame(vec![Event::PointerMoved(p)]);
        self.frame(vec![Event::PointerMoved(p)]);
        self.frame(vec![button(p, true)]);
        self.frame(vec![button(p, false)]);
        self.frame(vec![]);
    }

    fn type_text(&mut self, text: &str) {
        for ch in text.chars() {
            self.frame(vec![Event::Text(ch.to_string())]);
        }
    }

    fn key(&mut self, k: Key) {
        self.frame(vec![Event::Key {
            key: k,
            physical_key: Some(k),
            pressed: true,
            repeat: false,
            modifiers: NONE,
        }]);
        self.settle();
    }

    fn st(&mut self) -> &mut AppState {
        &mut self.app.ws_mut().active_tab_mut().state
    }

    fn choose(&mut self, kind: ToolKind) {
        let (se, sk) = self.st().session_sketch_mut().unwrap();
        se.choose_kind(sk, kind);
        self.settle();
    }

    fn sketch(&mut self) -> Sketch {
        self.st().session_sketch().unwrap().clone()
    }

    fn steps(&mut self) -> usize {
        self.st().sketch_history().unwrap().undo_len()
    }

    fn undo(&mut self) {
        self.app.ws_mut().active_tab_mut().undo().unwrap();
        self.settle();
    }

    fn redo(&mut self) {
        self.app.ws_mut().active_tab_mut().redo().unwrap();
        self.settle();
    }

    /// Eine Linie über den Bildschirm, angelegt wie vom Nutzer.
    fn draw_line(&mut self, a: Pos2, b: Pos2) {
        let (se, sk) = self.st().session_sketch_mut().unwrap();
        se.set_tool(sk, Tool::Line);
        self.settle();
        self.click(a.x, a.y);
        self.click(b.x, b.y);
        self.key(Key::Escape);
    }
}

/// Stand ohne Zähler: Rückgängig holt den alten Entwurf nicht zwingend mit derselben Revision.
fn state_of(sk: &Sketch) -> (Vec<u64>, usize, usize) {
    (
        sk.variables().iter().map(|v| v.to_bits()).collect(),
        sk.curves().count(),
        sk.constraints().count(),
    )
}

/// `drive` zeichnet genau einen Bogen: ein Schritt namens „Bogen zeichnen“, Rückgängig nimmt alles,
/// Wiederholen stellt es her.
fn one_undo_step(gui: &mut Gui, drive: impl FnOnce(&mut Gui)) -> Sketch {
    let (before, steps) = (state_of(&gui.sketch()), gui.steps());
    drive(gui);
    gui.settle();
    let done = gui.sketch();
    assert_ne!(state_of(&done), before, "es entstand etwas");
    assert_eq!(gui.steps(), steps + 1, "genau ein Schritt");
    assert_eq!(gui.st().sketch_undo_label(), Some(t("undo-arc")));
    gui.undo();
    assert_eq!(state_of(&gui.sketch()), before, "alles weg");
    gui.redo();
    assert_eq!(state_of(&gui.sketch()), state_of(&done), "bitgleich zurück");
    done
}

#[test]
fn undo_name_is_draw_arc() {
    assert_eq!(t("undo-arc"), "Bogen zeichnen");
}

#[test]
fn arc_3p_by_raw_events_is_one_undo_step() {
    let mut gui = Gui::new();
    gui.choose(ToolKind::Arc3P);
    let done = one_undo_step(&mut gui, |g| {
        g.click(800.0, 500.0);
        g.click(1000.0, 500.0);
        g.click(900.0, 420.0);
    });
    assert_eq!(arc_count(&done), 1);
    assert_solved(&done);
}

#[test]
fn arc_3p_with_a_typed_radius_by_raw_events_is_one_undo_step() {
    let mut gui = Gui::new();
    gui.choose(ToolKind::Arc3P);
    let done = one_undo_step(&mut gui, |g| {
        g.click(800.0, 500.0);
        g.click(1000.0, 500.0);
        g.frame(vec![Event::PointerMoved(pos2(900.0, 420.0))]);
        g.settle();
        g.type_text("500");
        g.key(Key::Enter);
    });
    assert!((last_arc(&done).radius() - 500.0).abs() < 1e-9);
    assert_eq!(dims(&done).len(), 1);
}

#[test]
fn arc_tangent_by_raw_events_is_one_undo_step_and_matches_the_preview() {
    let mut gui = Gui::new();
    gui.draw_line(pos2(600.0, 500.0), pos2(900.0, 500.0));
    gui.choose(ToolKind::ArcTangent);
    let done = one_undo_step(&mut gui, |g| {
        g.click(900.0, 500.0);
        let p = pos2(1000.0, 400.0);
        g.frame(vec![Event::PointerMoved(p)]);
        g.frame(vec![Event::PointerMoved(p)]);
        let want = {
            let sk = g.sketch();
            let se = g.st().sketch_session.as_ref().unwrap();
            let cur = se.draw_cursor.as_ref().expect("Zeiger über der Skizze").at;
            match se.kind_plan(&sk, cur) {
                Some(ShapePlan::Arc(a)) => a,
                other => panic!("Vorschau fehlt: {other:?}"),
            }
        };
        g.click(1000.0, 400.0);
        assert_same_arc(&last_arc(&g.sketch()), &want);
    });
    assert_eq!(count(&done, |k| matches!(k, K::Tangent(..))), 1);
    assert_solved(&done);
}

#[test]
fn escape_leaves_nothing_of_a_started_tangent_arc() {
    let mut gui = Gui::new();
    gui.draw_line(pos2(600.0, 500.0), pos2(900.0, 500.0));
    gui.choose(ToolKind::ArcTangent);
    let (before, steps) = (state_of(&gui.sketch()), gui.steps());
    gui.click(900.0, 500.0);
    assert_eq!(gui.st().sketch_session.as_ref().unwrap().kind_stage(), 1);
    gui.key(Key::Escape);
    let se = gui.st().sketch_session.as_ref().unwrap();
    assert_eq!(se.kind_stage(), 0);
    assert!(se.auto_tangent.is_none());
    assert_eq!(state_of(&gui.sketch()), before);
    assert_eq!(gui.steps(), steps);
}

#[test]
fn messages_exist_in_both_languages() {
    for lang in [planora_i18n::german(), planora_i18n::english()] {
        planora_i18n::with_lang(&lang, || {
            for key in [
                "sketch-arc-tangent-start",
                "sketch-arc-tangent-straight",
                "sketch-arc-radius-chord",
            ] {
                assert!(!t(key).is_empty() && t(key) != key, "{key}");
            }
        });
    }
}
