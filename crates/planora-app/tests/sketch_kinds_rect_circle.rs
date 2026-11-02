//! Skizzen 3, Task 6: Rechteck 3 Punkte und Mitte, Kreis 2 und 3 Punkte, 2 und 3 tangential auf der
//! Stufenmaschine. Ohne Oberfläche über `click`/`commit_input`, je Art eine Fahrt über Rohereignisse
//! bei 1920×1080 (`PlanoraApp::for_tests`).

mod common;

use eframe::App as _;
use egui::{Event, Key, Modifiers, PointerButton, Pos2, Rect, Vec2, pos2, vec2};
use planora_app::app::PlanoraApp;
use planora_app::sketch_input::FieldKind as F;
use planora_app::sketch_kinds::ToolKind;
use planora_app::sketch_state::{ConstraintAction, Item, SketchSession, Snap};
use planora_app::state::AppState;
use planora_app::workspace::Workspace;
use planora_app::{fonts, theme};
use planora_feature::{
    FeatureId, ParamEdit, ParamScope, ParamUnit, SketchFeature, SketchPlane, StandardPlane,
};
use planora_i18n::{DisplayUnit, t, with_unit};
use planora_math::{DVec2, Frame, Tolerance};
use planora_project::config::Config;
use planora_project::library::Library;
use planora_sketch::constraint::{ConstraintId, ConstraintKind as K};
use planora_sketch::model::{CurveId, CurveKind, PointId, Sketch};
use planora_sketch::shapes::ShapePlan;
use planora_sketch::solver::{SolveReport, SolveStatus};

const R: f64 = 0.5;
const EPS: f64 = 1e-9;
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

fn line(sk: &mut Sketch, a: (f64, f64), b: (f64, f64)) -> CurveId {
    let a = sk.add_point(a.0, a.1).unwrap();
    let b = sk.add_point(b.0, b.1).unwrap();
    sk.add_line(a, b).unwrap()
}

fn circle(sk: &mut Sketch, m: (f64, f64), r: f64) -> CurveId {
    let m = sk.add_point(m.0, m.1).unwrap();
    sk.add_circle(m, r).unwrap()
}

/// Bogen um `m` von Winkel 0 bis 90°.
fn arc(sk: &mut Sketch, m: (f64, f64), r: f64) -> CurveId {
    let c = sk.add_point(m.0, m.1).unwrap();
    let s = sk.add_point(m.0 + r, m.1).unwrap();
    let e = sk.add_point(m.0, m.1 + r).unwrap();
    sk.add_arc(c, s, e).unwrap()
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

fn kinds(sk: &Sketch) -> Vec<K> {
    sk.constraints().map(|(_, c)| c.kind.clone()).collect()
}

fn count(sk: &Sketch, pick: impl Fn(&K) -> bool) -> usize {
    kinds(sk).iter().filter(|k| pick(k)).count()
}

fn lines(sk: &Sketch, construction: bool) -> Vec<CurveId> {
    sk.curves()
        .filter(|(_, d)| matches!(d.kind, CurveKind::Line { .. }) && d.construction == construction)
        .map(|(c, _)| c)
        .collect()
}

fn ends(sk: &Sketch, c: CurveId) -> (PointId, PointId) {
    match sk.curve(c).unwrap().kind {
        CurveKind::Line { a, b } => (a, b),
        _ => panic!("keine Linie"),
    }
}

fn xy(sk: &Sketch, p: PointId) -> DVec2 {
    sk.point_xy(p).unwrap()
}

/// Die Ecken der vier normalen Linien in Reihenfolge der Linien.
fn corners(sk: &Sketch) -> Vec<DVec2> {
    lines(sk, false)
        .iter()
        .map(|&l| xy(sk, ends(sk, l).0))
        .collect()
}

fn only_circle(sk: &Sketch) -> (CurveId, DVec2, f64) {
    let (c, center) = sk
        .curves()
        .filter_map(|(c, d)| match d.kind {
            CurveKind::Circle { center, .. } => Some((c, center)),
            _ => None,
        })
        .last()
        .expect("Kreis");
    (c, xy(sk, center), sk.radius(c).unwrap())
}

fn expect_refused(se: &mut SketchSession, sk: &mut Sketch, raw: DVec2, key: &str) {
    let before = bits(sk);
    let (pos, curves) = (se.pending_pos.clone(), se.pending_curves.clone());
    let text = se.click(sk, raw, R, false).unwrap_err();
    assert_eq!(text, t(key));
    assert_eq!(se.message_text().as_deref(), Some(t(key).as_str()));
    assert_eq!(bits(sk), before, "Skizze bitgleich");
    assert_eq!(se.pending_pos, pos, "Lagen bleiben");
    assert_eq!(se.pending_curves, curves, "Kurven bleiben");
    assert!(se.created.is_none());
}

// ---- Hilfen mit getippten Werten ----------------------------------------------------------------

/// Offene, leere Skizze auf XY; `Breite = 40` als Parameter.
fn rig(kind: ToolKind) -> Workspace {
    let root = common::temp_root("planora-sketch-kinds-rc");
    let mut w = Workspace::new(Some(Library::open(&root).unwrap()), Config::default());
    let s = &mut w.active_tab_mut().state;
    let id = s.add_parameter().unwrap();
    s.edit_parameter(
        id,
        ParamEdit {
            name: "Breite".into(),
            formula: "40".into(),
            unit: ParamUnit::Length,
            comment: String::new(),
        },
    )
    .unwrap();
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

fn draft(w: &mut Workspace) -> &SketchFeature {
    parts(w).1
}

fn dims(sk: &Sketch) -> Vec<(ConstraintId, K)> {
    sk.constraints()
        .filter(|(_, c)| c.kind.is_dimension())
        .map(|(id, c)| (id, c.kind.clone()))
        .collect()
}

// ---- 3-Punkt-Rechteck ---------------------------------------------------------------------------

#[test]
fn rect_3p_three_clicks_make_a_rectangle_that_keeps_five_freedoms() {
    let (mut se, mut sk) = session(ToolKind::Rect3P);
    click(&mut se, &mut sk, 1.0, 1.0);
    click(&mut se, &mut sk, 11.0, 1.0);
    assert_eq!(sk.curves().count(), 0, "bis zum letzten Klick nichts");
    click(&mut se, &mut sk, 4.0, 5.0);
    let c = corners(&sk);
    assert_eq!(c.len(), 4);
    for (got, want) in c
        .iter()
        .zip([at(1., 1.), at(11., 1.), at(11., 5.), at(1., 5.)])
    {
        assert!(close(*got, want), "{got:?} {want:?}");
    }
    assert_eq!(count(&sk, |k| matches!(k, K::Perpendicular(..))), 1);
    assert_eq!(count(&sk, |k| matches!(k, K::Parallel(..))), 2);
    let r = assert_solved(&sk);
    assert_eq!(r.dof, 5);
    assert_eq!(
        se.last_report.as_ref().unwrap().iterations,
        0,
        "nichts lief"
    );
    assert!(se.pending_pos.is_empty() && se.pending_snaps.is_empty());
    assert!(se.created.is_some());
}

#[test]
fn rect_3p_at_the_origin_is_fixed_there_and_oblique_works_the_same() {
    let (mut se, mut sk) = session(ToolKind::Rect3P);
    click(&mut se, &mut sk, 0.0, 0.0);
    click(&mut se, &mut sk, 10.0, 5.0);
    click(&mut se, &mut sk, 2.0, 7.0);
    let c = corners(&sk);
    assert!(close(c[0], at(0.0, 0.0)));
    assert!(close(c[1], at(10.0, 5.0)));
    let (u, v) = (c[1] - c[0], c[2] - c[1]);
    assert!(u.dot(v).abs() < 1e-9, "rechtwinklig");
    assert_eq!(count(&sk, |k| matches!(k, K::Fixed(..))), 1);
    let r = assert_solved(&sk);
    assert_eq!(r.dof, 3, "Ursprung nimmt zwei von fünf");
}

#[test]
fn rect_3p_pointer_on_the_other_side_turns_the_depth() {
    let (mut se, mut sk) = session(ToolKind::Rect3P);
    click(&mut se, &mut sk, 1.0, 1.0);
    click(&mut se, &mut sk, 11.0, 1.0);
    let above = se.kind_plan(&sk, at(4.0, 5.0)).unwrap();
    let below = se.kind_plan(&sk, at(4.0, -3.0)).unwrap();
    let (ShapePlan::Quad(a), ShapePlan::Quad(b)) = (above, below) else {
        panic!("Viereck erwartet");
    };
    assert!(close(a[2], at(11.0, 5.0)));
    assert!(close(b[2], at(11.0, -3.0)));
}

#[test]
fn rect_3p_typed_width_and_angle_lie_on_the_ray_and_become_dimensions() {
    let mut w = rig(ToolKind::Rect3P);
    commit(&mut w, 1.0, 1.0).unwrap();
    type_in(&mut w, F::Width, "30");
    type_in(&mut w, F::Angle, "30");
    let ray = at(1.0, 1.0) + DVec2::from_angle(30f64.to_radians()) * 30.0;
    assert!(close(resolved(&mut w, 20.0, 15.0), ray));
    // Zeiger unterhalb der x-Achse: anderer Strahl.
    let down = at(1.0, 1.0) + DVec2::from_angle(-30f64.to_radians()) * 30.0;
    assert!(close(resolved(&mut w, 20.0, -15.0), down));
    commit(&mut w, 20.0, 15.0).unwrap();
    assert_eq!(parts(&mut w).0.pending_pos.len(), 2);
    assert!(close(parts(&mut w).0.pending_pos[1], ray));
    type_in(&mut w, F::Depth, "12");
    commit(&mut w, 0.0, 30.0).unwrap();

    let sketch = sk(&w);
    let c = corners(sketch);
    assert!(close(c[1], ray), "{:?}", c[1]);
    assert!((c[1].distance(c[2]) - 12.0).abs() < 1e-6);
    assert!((c[1] - c[0]).dot(c[2] - c[1]).abs() < 1e-6);
    let ds = dims(sketch);
    assert_eq!(ds.len(), 3, "{ds:?}");
    let lens: Vec<f64> = ds
        .iter()
        .filter_map(|(_, k)| match k {
            K::Distance(_, _, d) => Some(*d),
            _ => None,
        })
        .collect();
    assert_eq!(lens, [30.0, 12.0]);
    assert_eq!(count(sketch, |k| matches!(k, K::Angle(..))), 1);
    let r = report(sketch);
    assert_eq!(r.status, SolveStatus::Solved);
    assert!(r.redundant.is_empty());
    // Die Achshilfe und die vier Seiten.
    assert_eq!(lines(sketch, true).len(), 1);
    assert_eq!(lines(sketch, false).len(), 4);
}

#[test]
fn rect_3p_depth_alone_follows_the_pointer_side() {
    let mut w = rig(ToolKind::Rect3P);
    commit(&mut w, 1.0, 1.0).unwrap();
    commit(&mut w, 11.0, 1.0).unwrap();
    type_in(&mut w, F::Depth, "12");
    assert!((resolved(&mut w, 4.0, 3.0).y - 13.0).abs() < 1e-9);
    assert!((resolved(&mut w, 4.0, -3.0).y + 11.0).abs() < 1e-9);
    commit(&mut w, 4.0, -3.0).unwrap();
    let sketch = sk(&w);
    let c = corners(sketch);
    assert!(close(c[2], at(11.0, -11.0)), "{:?}", c[2]);
    let ds = dims(sketch);
    assert_eq!(ds.len(), 1);
    let K::Distance(p, q, d) = ds[0].1 else {
        panic!("Abstand erwartet");
    };
    assert_eq!(d, 12.0);
    assert!(close(xy(sketch, p), c[1]) && close(xy(sketch, q), c[2]));
}

// ---- Mittelpunkt-Rechteck -----------------------------------------------------------------------

#[test]
fn rect_center_has_center_diagonals_and_two_midpoints() {
    let (mut se, mut sk) = session(ToolKind::RectCenter);
    click(&mut se, &mut sk, 5.0, 5.0);
    click(&mut se, &mut sk, 8.0, 7.0);
    let c = corners(&sk);
    for (got, want) in c
        .iter()
        .zip([at(2., 3.), at(8., 3.), at(8., 7.), at(2., 7.)])
    {
        assert!(close(*got, want), "{got:?} {want:?}");
    }
    assert_eq!(lines(&sk, true).len(), 2, "Diagonalen sind Konstruktion");
    let mids: Vec<PointId> = kinds(&sk)
        .iter()
        .filter_map(|k| match k {
            K::Midpoint(m, _) => Some(*m),
            _ => None,
        })
        .collect();
    assert_eq!(mids.len(), 2);
    assert_eq!(mids[0], mids[1]);
    assert!(close(xy(&sk, mids[0]), at(5.0, 5.0)));
    assert_eq!(assert_solved(&sk).dof, 4);
}

#[test]
fn rect_center_typed_values_are_full_sizes_and_the_pointer_gives_the_sign() {
    let mut w = rig(ToolKind::RectCenter);
    commit(&mut w, 5.0, 5.0).unwrap();
    type_in(&mut w, F::Width, "20");
    assert!((resolved(&mut w, 8.0, 7.0).x - 15.0).abs() < EPS);
    assert!((resolved(&mut w, 1.0, 7.0).x + 5.0).abs() < EPS);
    type_in(&mut w, F::Height, "6");
    assert!((resolved(&mut w, 1.0, 1.0).y - 2.0).abs() < EPS);
    commit(&mut w, 1.0, 7.0).unwrap();
    let sketch = sk(&w);
    let c = corners(sketch);
    for (got, want) in c
        .iter()
        .zip([at(-5., 2.), at(15., 2.), at(15., 8.), at(-5., 8.)])
    {
        assert!(close(*got, want), "{got:?} {want:?}");
    }
    let ds = dims(sketch);
    assert_eq!(ds.len(), 2);
    assert!(matches!(ds[0].1, K::Distance(_, _, d) if d == 20.0));
    assert!(matches!(ds[1].1, K::Distance(_, _, d) if d == 6.0));
    assert_eq!(report(sketch).dof, 2);
}

#[test]
fn field_values_show_the_shape_before_the_first_key() {
    let mut w = rig(ToolKind::RectCenter);
    commit(&mut w, 5.0, 5.0).unwrap();
    let (se, sf, _) = parts(&mut w);
    se.update_cursor(&sf.sketch, Some(at(8.0, 7.0)), R);
    assert_eq!(se.field_value(&sf.sketch, F::Width), Some(6.0));
    assert_eq!(se.field_value(&sf.sketch, F::Height), Some(4.0));

    let mut w = rig(ToolKind::Rect3P);
    commit(&mut w, 1.0, 1.0).unwrap();
    let (se, sf, _) = parts(&mut w);
    se.update_cursor(&sf.sketch, Some(at(4.0, 5.0)), R);
    assert!((se.field_value(&sf.sketch, F::Width).unwrap() - 5.0).abs() < EPS);
    let angle = se.field_value(&sf.sketch, F::Angle).unwrap();
    assert!((angle - 4f64.atan2(3.0)).abs() < EPS);
    commit(&mut w, 11.0, 1.0).unwrap();
    let (se, sf, _) = parts(&mut w);
    se.update_cursor(&sf.sketch, Some(at(4.0, -3.0)), R);
    assert!((se.field_value(&sf.sketch, F::Depth).unwrap() - 4.0).abs() < EPS);

    let mut w = rig(ToolKind::Circle2P);
    commit(&mut w, 1.0, 1.0).unwrap();
    let (se, sf, _) = parts(&mut w);
    se.update_cursor(&sf.sketch, Some(at(4.0, 5.0)), R);
    assert!((se.field_value(&sf.sketch, F::Diameter).unwrap() - 5.0).abs() < EPS);

    let (mut se, mut s) = session(ToolKind::Circle2T);
    line(&mut s, (-20.0, 1.0), (20.0, 1.0));
    line(&mut s, (21.0, -20.0), (21.0, 20.0));
    click(&mut se, &mut s, 0.0, 1.1);
    click(&mut se, &mut s, 21.1, 0.0);
    se.sync_input(&s);
    se.update_cursor(&s, Some(at(17.0, 5.0)), R);
    let (_, r) = match se.kind_plan(&s, at(17.0, 5.0)).unwrap() {
        ShapePlan::Circle(m, r) => (m, r),
        other => panic!("{other:?}"),
    };
    assert!((se.field_value(&s, F::Diameter).unwrap() - 2.0 * r).abs() < EPS);
}

// ---- 2-Punkt-Kreis ------------------------------------------------------------------------------

#[test]
fn circle_2p_the_two_clicks_are_a_diameter() {
    let (mut se, mut sk) = session(ToolKind::Circle2P);
    click(&mut se, &mut sk, 1.0, 1.0);
    click(&mut se, &mut sk, 11.0, 1.0);
    let (_, m, r) = only_circle(&sk);
    assert!(close(m, at(6.0, 1.0)));
    assert!((r - 5.0).abs() < EPS);
    assert_eq!(assert_solved(&sk).dof, 3);
    assert!(sk.constraints().next().is_none());
}

#[test]
fn circle_2p_typed_diameter_keeps_the_first_point_and_the_direction() {
    let mut w = rig(ToolKind::Circle2P);
    commit(&mut w, 1.0, 1.0).unwrap();
    type_in(&mut w, F::Diameter, "30");
    assert!(close(resolved(&mut w, 11.0, 1.0), at(31.0, 1.0)));
    commit(&mut w, 11.0, 1.0).unwrap();
    let (c, m, r) = only_circle(sk(&w));
    assert!((r - 15.0).abs() < 1e-9);
    assert!(close(m, at(16.0, 1.0)));
    assert!(matches!(dims(sk(&w))[0].1, K::Diameter(d, v) if d == c && v == 30.0));
}

#[test]
fn circle_2p_on_two_existing_points_binds_both() {
    let (mut se, mut sk) = session(ToolKind::Circle2P);
    let p = sk.add_point(2.0, 2.0).unwrap();
    let q = sk.add_point(12.0, 2.0).unwrap();
    click(&mut se, &mut sk, 2.1, 2.1);
    click(&mut se, &mut sk, 12.1, 1.9);
    let (c, m, _) = only_circle(&sk);
    assert!(close(m, at(7.0, 2.0)));
    assert!(
        kinds(&sk)
            .iter()
            .any(|k| matches!(k, K::PointOnCurve(a, b) if *a == p && *b == c))
    );
    assert!(
        kinds(&sk)
            .iter()
            .any(|k| matches!(k, K::PointOnCurve(a, b) if *a == q && *b == c))
    );
    assert_solved(&sk);
}

// ---- 3-Punkt-Kreis ------------------------------------------------------------------------------

#[test]
fn circle_3p_shows_a_line_then_the_circle_and_makes_the_circumcircle() {
    let (mut se, mut sk) = session(ToolKind::Circle3P);
    assert!(se.kind_preview(&sk, at(1.0, 1.0)).is_none());
    click(&mut se, &mut sk, 1.0, 1.0);
    assert!(
        se.kind_plan(&sk, at(11.0, 1.0)).is_none(),
        "noch kein Kreis"
    );
    let first = se.kind_preview(&sk, at(11.0, 1.0)).unwrap();
    assert_eq!(first, [vec![at(1.0, 1.0), at(11.0, 1.0)]]);
    click(&mut se, &mut sk, 11.0, 1.0);
    let Some(ShapePlan::Circle(m, r)) = se.kind_plan(&sk, at(6.0, 6.0)) else {
        panic!("Kreis erwartet");
    };
    assert!(se.kind_preview(&sk, at(6.0, 6.0)).unwrap()[0].len() > 10);
    click(&mut se, &mut sk, 6.0, 6.0);
    let (_, center, radius) = only_circle(&sk);
    assert!(close(center, m) && (radius - r).abs() < EPS);
    assert!(close(center, at(6.0, 1.0)), "{center:?}");
    assert_eq!(assert_solved(&sk).dof, 3);
}

#[test]
fn circle_3p_on_three_points_binds_all() {
    let (mut se, mut sk) = session(ToolKind::Circle3P);
    for (x, y) in [(2.0, 2.0), (12.0, 2.0), (7.0, 9.0)] {
        sk.add_point(x, y).unwrap();
    }
    click(&mut se, &mut sk, 2.1, 2.1);
    click(&mut se, &mut sk, 11.9, 2.1);
    click(&mut se, &mut sk, 7.0, 9.1);
    assert_eq!(count(&sk, |k| matches!(k, K::PointOnCurve(..))), 3);
    assert_solved(&sk);
}

// ---- 2-Tangenten-Kreis --------------------------------------------------------------------------

/// Zwei Linien, die sich in (20, 1) träfen, schon gewählt.
fn two_lines_picked() -> (SketchSession, Sketch, CurveId, CurveId) {
    let (mut se, mut sk) = session(ToolKind::Circle2T);
    let a = line(&mut sk, (-20.0, 1.0), (15.0, 1.0));
    let b = line(&mut sk, (20.0, 5.0), (20.0, 30.0));
    click(&mut se, &mut sk, 0.0, 1.1);
    click(&mut se, &mut sk, 20.1, 20.0);
    assert_eq!(se.pending_curves, [a, b]);
    (se, sk, a, b)
}

#[test]
fn circle_2t_takes_lines_only() {
    let (mut se, mut sk) = session(ToolKind::Circle2T);
    let a = line(&mut sk, (-20.0, 1.0), (15.0, 1.0));
    arc(&mut sk, (0.0, 30.0), 5.0);
    expect_refused(&mut se, &mut sk, at(5.0, 30.1), "sketch-tangent-lines-only");
    click(&mut se, &mut sk, 0.0, 1.1);
    assert_eq!(se.pending_curves, [a]);
    expect_refused(&mut se, &mut sk, at(5.0, 30.1), "sketch-tangent-lines-only");
}

#[test]
fn circle_2t_shows_nothing_before_the_last_stage() {
    let (mut se, mut sk) = session(ToolKind::Circle2T);
    line(&mut sk, (-20.0, 1.0), (15.0, 1.0));
    assert!(se.kind_preview(&sk, at(0.0, 5.0)).is_none());
    click(&mut se, &mut sk, 0.0, 1.1);
    assert!(se.kind_preview(&sk, at(0.0, 5.0)).is_none());
}

#[test]
fn circle_2t_typed_diameter_makes_two_tangents_and_a_dimension() {
    let mut w = rig(ToolKind::Circle2T);
    let (a, b) = {
        let (_, sf, _) = parts(&mut w);
        (
            line(&mut sf.sketch, (-20.0, 1.0), (15.0, 1.0)),
            line(&mut sf.sketch, (20.0, 5.0), (20.0, 30.0)),
        )
    };
    commit(&mut w, 0.0, 1.1).unwrap();
    commit(&mut w, 20.1, 20.0).unwrap();
    type_in(&mut w, F::Diameter, "10");
    let before: Vec<DVec2> = sk(&w).points().map(|(p, _)| xy(sk(&w), p)).collect();
    commit(&mut w, 17.0, 5.0).unwrap();
    let sketch = sk(&w);
    let (c, m, r) = only_circle(sketch);
    assert!((r - 5.0).abs() < 1e-9);
    assert!(close(m, at(15.0, 6.0)), "{m:?}");
    assert!(kinds(sketch).contains(&K::Tangent(a, c)));
    assert!(kinds(sketch).contains(&K::Tangent(b, c)));
    assert!(matches!(dims(sketch)[0].1, K::Diameter(d, v) if d == c && v == 10.0));
    let rep = report(sketch);
    assert_eq!(rep.status, SolveStatus::Solved);
    assert!(rep.redundant.is_empty());
    // Zwei freie Linien (8), der Kreis nimmt sein Maß und beide Tangenten: kein Freiheitsgrad mehr.
    assert_eq!(rep.dof, 8);
    for ((p, _), old) in sketch.points().zip(before) {
        assert!(close(xy(sketch, p), old), "kein Punkt wandert");
    }
}

#[test]
fn circle_2t_without_a_value_has_one_freedom_and_the_pointer_chooses_the_sector() {
    let (mut se, mut sk, a, b) = two_lines_picked();
    let Some(ShapePlan::Circle(m, r)) = se.kind_plan(&sk, at(17.0, 5.0)) else {
        panic!("Kreis erwartet");
    };
    assert!((m.x + r - 20.0).abs() < 1e-9 && (m.y - r - 1.0).abs() < 1e-9);
    let Some(ShapePlan::Circle(other, _)) = se.kind_plan(&sk, at(23.0, 5.0)) else {
        panic!("Kreis erwartet");
    };
    assert!(other.x > 20.0 && other.y > 1.0, "{other:?}");
    let Some(ShapePlan::Circle(below, _)) = se.kind_plan(&sk, at(17.0, -3.0)) else {
        panic!("Kreis erwartet");
    };
    assert!(below.x < 20.0 && below.y < 1.0, "{below:?}");
    let raw = at(17.0, 5.0);
    click(&mut se, &mut sk, raw.x, raw.y);
    let (c, center, radius) = only_circle(&sk);
    assert!(close(center, m) && (radius - r).abs() < 1e-6);
    assert!(kinds(&sk).contains(&K::Tangent(a, c)));
    assert!(kinds(&sk).contains(&K::Tangent(b, c)));
    assert_eq!(assert_solved(&sk).dof, 8 + 1);
}

#[test]
fn circle_2t_last_click_near_a_line_is_not_a_snap() {
    let (mut se, sk, a, _) = two_lines_picked();
    se.update_cursor(&sk, Some(at(10.0, 1.2)), R);
    let cur = se.draw_cursor.clone().unwrap();
    assert_eq!(cur.snap, Snap::None, "{a:?}");
    assert_eq!(cur.at, at(10.0, 1.2));
    assert!(se.kind_plan(&sk, cur.at).is_some());
}

#[test]
fn circle_2t_parallel_lines_only_take_their_own_diameter() {
    let mut w = rig(ToolKind::Circle2T);
    {
        let (_, sf, _) = parts(&mut w);
        line(&mut sf.sketch, (-20.0, 1.0), (20.0, 1.0));
        line(&mut sf.sketch, (-20.0, 11.0), (20.0, 11.0));
    }
    commit(&mut w, 0.0, 1.1).unwrap();
    commit(&mut w, 0.0, 11.1).unwrap();
    let before = bits(sk(&w));
    type_in(&mut w, F::Diameter, "4");
    assert_eq!(
        commit(&mut w, 3.0, 8.0).unwrap_err(),
        t("sketch-tangent-no-solution")
    );
    assert_eq!(bits(sk(&w)), before);
    assert_eq!(parts(&mut w).0.pending_curves.len(), 2);
    type_in(&mut w, F::Diameter, "");
    commit(&mut w, 3.0, 8.0).unwrap();
    let (_, m, r) = only_circle(sk(&w));
    assert!(close(m, at(3.0, 6.0)) && (r - 5.0).abs() < 1e-9);
}

// ---- 3-Tangenten-Kreis --------------------------------------------------------------------------

/// Vorschau an der Rohlage und Ergebnis sind derselbe Kreis, kein Punkt der Skizze wandert.
fn third_click_matches_its_preview(se: &mut SketchSession, sk: &mut Sketch, raw: DVec2) {
    let before: Vec<(PointId, DVec2)> = sk.points().map(|(p, _)| (p, xy(sk, p))).collect();
    se.update_cursor(sk, Some(raw), R);
    let Some(ShapePlan::Circle(m, r)) = se.kind_plan(sk, raw) else {
        panic!("Vorschau erwartet");
    };
    assert!(se.kind_preview(sk, raw).is_some());
    se.click(sk, raw, R, false).unwrap();
    let (c, center, radius) = only_circle(sk);
    assert!(center.distance(m) < 1e-6, "{center:?} {m:?}");
    assert!((radius - r).abs() < 1e-6);
    assert_eq!(count(sk, |k| matches!(k, K::Tangent(_, x) if *x == c)), 3);
    for (p, old) in before {
        assert!(xy(sk, p).distance(old) < 1e-6, "{p:?} wandert");
    }
    assert_eq!(assert_solved(sk).iterations, 0);
    assert!(se.pending_curves.is_empty());
}

#[test]
fn circle_3t_at_three_lines_is_what_the_preview_shows() {
    let (mut se, mut sk) = session(ToolKind::Circle3T);
    line(&mut sk, (0.5, 0.0), (10.0, 0.0));
    line(&mut sk, (0.0, 0.5), (0.0, 10.0));
    line(&mut sk, (10.0, 1.0), (1.0, 10.0));
    click(&mut se, &mut sk, 5.0, 0.1);
    click(&mut se, &mut sk, 0.1, 5.0);
    assert!(se.kind_preview(&sk, at(5.0, 6.1)).is_none(), "ohne Kurve");
    third_click_matches_its_preview(&mut se, &mut sk, at(5.6, 5.6));
}

#[test]
fn circle_3t_at_line_circle_and_arc_is_what_the_preview_shows() {
    let (mut se, mut sk) = session(ToolKind::Circle3T);
    line(&mut sk, (-30.0, -2.0), (30.0, -2.0));
    circle(&mut sk, (-15.0, 12.0), 5.0);
    arc(&mut sk, (15.0, 12.0), 5.0);
    click(&mut se, &mut sk, 0.0, -1.9);
    click(&mut se, &mut sk, -10.0, 12.0);
    third_click_matches_its_preview(&mut se, &mut sk, at(19.0, 15.0));
}

#[test]
fn circle_3t_same_curve_twice_and_click_without_curve_do_nothing() {
    let (mut se, mut sk) = session(ToolKind::Circle3T);
    let a = line(&mut sk, (0.5, 0.0), (10.0, 0.0));
    let b = line(&mut sk, (0.0, 0.5), (0.0, 10.0));
    click(&mut se, &mut sk, 5.0, 0.1);
    click(&mut se, &mut sk, 6.0, 0.1);
    assert_eq!(se.pending_curves, [a], "nicht zweimal");
    click(&mut se, &mut sk, 0.1, 5.0);
    assert_eq!(se.pending_curves, [a, b]);
    let before = bits(&sk);
    // Auf einer schon gewählten Kurve oder im Leeren passiert in Stufe 2 nichts.
    click(&mut se, &mut sk, 7.0, 0.1);
    click(&mut se, &mut sk, 30.0, 30.0);
    assert_eq!(se.pending_curves, [a, b]);
    assert_eq!(bits(&sk), before);
    assert!(se.message.is_none());
}

#[test]
fn circle_3t_refuses_an_ellipse() {
    let (mut se, mut sk) = session(ToolKind::Circle3T);
    let a = line(&mut sk, (0.5, 0.0), (10.0, 0.0));
    let b = line(&mut sk, (0.0, 0.5), (0.0, 10.0));
    let m = sk.add_point(30.0, 30.0).unwrap();
    sk.add_ellipse(m, at(5.0, 0.0), 2.0).unwrap();
    expect_refused(&mut se, &mut sk, at(35.0, 30.0), "sketch-tangent-kinds");
    click(&mut se, &mut sk, 5.0, 0.1);
    click(&mut se, &mut sk, 0.1, 5.0);
    assert_eq!(se.pending_curves, [a, b]);
    expect_refused(&mut se, &mut sk, at(35.0, 30.0), "sketch-tangent-kinds");
}

#[test]
fn circle_3t_without_a_solution_says_so_and_keeps_the_curves() {
    let (mut se, mut sk) = session(ToolKind::Circle3T);
    let a = line(&mut sk, (0.0, 1.0), (20.0, 1.0));
    let b = line(&mut sk, (0.0, 6.0), (20.0, 6.0));
    line(&mut sk, (0.0, 11.0), (20.0, 11.0));
    click(&mut se, &mut sk, 5.0, 1.1);
    click(&mut se, &mut sk, 5.0, 6.1);
    se.update_cursor(&sk, Some(at(5.0, 11.1)), R);
    assert!(se.kind_plan(&sk, at(5.0, 11.1)).is_none());
    expect_refused(
        &mut se,
        &mut sk,
        at(5.0, 11.1),
        "sketch-tangent-no-solution",
    );
    assert_eq!(se.pending_curves, [a, b]);
}

#[test]
fn a_solution_that_moves_when_solved_is_refused() {
    // Ein fester Punkt, den die Skizze noch nicht erfüllt: Beim Lösen dreht er die Linie, die
    // Rechnung sah sie noch an der alten Stelle.
    let (mut se, mut sk) = session(ToolKind::Circle3T);
    line(&mut sk, (0.5, 0.0), (10.0, 0.0));
    line(&mut sk, (0.0, 0.5), (0.0, 10.0));
    let c = line(&mut sk, (10.0, 1.0), (1.0, 10.0));
    let (p, q) = ends(&sk, c);
    sk.add_constraint(K::Fixed(p, 10.0, 1.0)).unwrap();
    sk.add_constraint(K::Fixed(q, 15.0, 30.0)).unwrap();
    click(&mut se, &mut sk, 5.0, 0.1);
    click(&mut se, &mut sk, 0.1, 5.0);
    expect_refused(&mut se, &mut sk, at(5.6, 5.6), "sketch-tangent-no-solution");
    assert_eq!(se.pending_curves.len(), 2);
}

#[test]
fn a_fixed_line_stays_and_so_does_the_circle() {
    let (mut se, mut sk) = session(ToolKind::Circle3T);
    let a = line(&mut sk, (0.5, 0.0), (10.0, 0.0));
    let (p, q) = ends(&sk, a);
    sk.add_constraint(K::Fixed(p, 0.5, 0.0)).unwrap();
    sk.add_constraint(K::Fixed(q, 10.0, 0.0)).unwrap();
    line(&mut sk, (0.0, 0.5), (0.0, 10.0));
    line(&mut sk, (10.0, 1.0), (1.0, 10.0));
    se.last_report = Some(sk.solve());
    click(&mut se, &mut sk, 5.0, 0.1);
    click(&mut se, &mut sk, 0.1, 5.0);
    third_click_matches_its_preview(&mut se, &mut sk, at(5.6, 5.6));
}

// ---- Strg ---------------------------------------------------------------------------------------

#[test]
fn ctrl_binds_nothing_but_picks_still_pick() {
    let (mut se, mut sk) = session(ToolKind::Rect3P);
    sk.add_point(2.0, 2.0).unwrap();
    sk.add_point(12.0, 2.0).unwrap();
    se.cursor_opts.suppressed = true;
    click(&mut se, &mut sk, 2.1, 2.1);
    assert_eq!(se.pending_pos, [at(2.1, 2.1)], "kein Fang");
    click(&mut se, &mut sk, 11.9, 2.1);
    click(&mut se, &mut sk, 6.0, 8.0);
    assert_eq!(count(&sk, |k| matches!(k, K::Coincident(..))), 0);

    let (mut se, mut sk) = session(ToolKind::Circle2P);
    sk.add_point(2.0, 2.0).unwrap();
    se.cursor_opts.suppressed = true;
    click(&mut se, &mut sk, 2.1, 2.1);
    click(&mut se, &mut sk, 12.0, 2.1);
    assert_eq!(count(&sk, |k| matches!(k, K::PointOnCurve(..))), 0);

    // Eine Kurve wählen ist kein Fang: Das geht auch mit Strg.
    for kind in [ToolKind::Circle2T, ToolKind::Circle3T] {
        let (mut se, mut sk) = session(kind);
        let a = line(&mut sk, (0.5, 0.0), (10.0, 0.0));
        let b = line(&mut sk, (0.0, 0.5), (0.0, 10.0));
        let c = line(&mut sk, (10.0, 1.0), (1.0, 10.0));
        se.cursor_opts.suppressed = true;
        let cur = se.resolve_cursor(&sk, at(5.0, 0.1), R);
        assert_eq!(cur.snap, Snap::Curve(a), "{kind:?}");
        click(&mut se, &mut sk, 5.0, 0.1);
        click(&mut se, &mut sk, 0.1, 5.0);
        assert_eq!(se.pending_curves, [a, b], "{kind:?}");
        if kind == ToolKind::Circle3T {
            let cur = se.resolve_cursor(&sk, at(5.6, 5.6), R);
            assert_eq!(cur.snap, Snap::Curve(c));
            click(&mut se, &mut sk, 5.6, 5.6);
            assert_eq!(count(&sk, |k| matches!(k, K::Tangent(..))), 3);
        }
    }
}

// ---- Review-Focus 1: Unmögliches ----------------------------------------------------------------

#[test]
fn the_same_place_twice_is_refused_at_any_stage() {
    let (mut se, mut sk) = session(ToolKind::Rect3P);
    click(&mut se, &mut sk, 1.0, 1.0);
    assert!(se.kind_preview(&sk, at(1.0, 1.0)).is_none(), "keine Länge");
    expect_refused(&mut se, &mut sk, at(1.0, 1.0), "sketch-err-rect-size");
    assert_eq!(se.kind_stage(), 1);

    let (mut se, mut sk) = session(ToolKind::RectCenter);
    click(&mut se, &mut sk, 1.0, 1.0);
    assert!(se.kind_preview(&sk, at(1.0, 1.0)).is_none());
    expect_refused(&mut se, &mut sk, at(1.0, 1.0), "sketch-err-rect-size");
    // Nur in einer Richtung: auch keine Fläche.
    assert!(se.kind_preview(&sk, at(5.0, 1.0)).is_none());
    expect_refused(&mut se, &mut sk, at(5.0, 1.0), "sketch-err-rect-size");

    let (mut se, mut sk) = session(ToolKind::Circle2P);
    click(&mut se, &mut sk, 1.0, 1.0);
    assert!(se.kind_preview(&sk, at(1.0, 1.0)).is_none());
    expect_refused(&mut se, &mut sk, at(1.0, 1.0), "sketch-radius-positive");

    let (mut se, mut sk) = session(ToolKind::Circle3P);
    click(&mut se, &mut sk, 1.0, 1.0);
    expect_refused(&mut se, &mut sk, at(1.0, 1.0), "sketch-circle-collinear");
    click(&mut se, &mut sk, 5.0, 1.0);
    assert!(se.kind_preview(&sk, at(5.0, 1.0)).is_none());
    expect_refused(&mut se, &mut sk, at(5.0, 1.0), "sketch-circle-collinear");
}

#[test]
fn three_points_on_a_line_are_refused() {
    let (mut se, mut sk) = session(ToolKind::Circle3P);
    click(&mut se, &mut sk, 1.0, 1.0);
    click(&mut se, &mut sk, 5.0, 3.0);
    assert!(se.kind_preview(&sk, at(9.0, 5.0)).is_none());
    expect_refused(&mut se, &mut sk, at(9.0, 5.0), "sketch-circle-collinear");
    // Die Form läuft weiter.
    click(&mut se, &mut sk, 9.0, 1.0);
    assert_eq!(sk.curves().count(), 1);
}

#[test]
fn a_rect_3p_without_depth_is_refused_and_continues() {
    let (mut se, mut sk) = session(ToolKind::Rect3P);
    click(&mut se, &mut sk, 1.0, 1.0);
    click(&mut se, &mut sk, 11.0, 6.0);
    assert!(se.kind_preview(&sk, at(5.0, 3.0)).is_none());
    expect_refused(&mut se, &mut sk, at(5.0, 3.0), "sketch-err-rect-size");
    assert!(se.kind_preview(&sk, at(5.0, 8.0)).is_some());
    click(&mut se, &mut sk, 5.0, 8.0);
    assert_eq!(lines(&sk, false).len(), 4);
}

// ---- Review-Focus 2: Fang auf Festem -------------------------------------------------------------

fn fixed_point(sk: &mut Sketch, x: f64, y: f64) -> PointId {
    let p = sk.add_point(x, y).unwrap();
    sk.add_constraint(K::Fixed(p, x, y)).unwrap();
    p
}

#[test]
fn rect_3p_between_two_fixed_points_is_solved_and_width_cannot_break_it() {
    let (mut se, mut sk) = session(ToolKind::Rect3P);
    let p = fixed_point(&mut sk, 2.0, 2.0);
    let q = fixed_point(&mut sk, 12.0, 2.0);
    se.last_report = Some(sk.solve());
    click(&mut se, &mut sk, 2.1, 2.1);
    click(&mut se, &mut sk, 11.9, 1.9);
    click(&mut se, &mut sk, 6.0, 8.0);
    let side = lines(&sk, false)[0];
    let (a, b) = ends(&sk, side);
    assert!(kinds(&sk).contains(&K::Coincident(a, p)));
    assert!(kinds(&sk).contains(&K::Coincident(b, q)));
    let r = assert_solved(&sk);
    assert_eq!(se.last_report.as_ref().unwrap().status, SolveStatus::Solved);
    assert_eq!(r.dof, 1, "nur die Tiefe");

    let before = bits(&sk);
    se.selection = vec![Item::Point(a), Item::Point(b)];
    let refused = se.apply(&mut sk, ConstraintAction::Distance, Some(20.0));
    assert!(refused.is_err(), "{refused:?}");
    assert_eq!(bits(&sk), before, "nicht kaputt");
    assert!(close(xy(&sk, a), at(2.0, 2.0)) && close(xy(&sk, b), at(12.0, 2.0)));
}

#[test]
fn a_snap_stays_true_when_a_dimension_changes() {
    let (mut se, mut sk) = session(ToolKind::Rect3P);
    let p = fixed_point(&mut sk, 2.0, 2.0);
    se.last_report = Some(sk.solve());
    click(&mut se, &mut sk, 2.1, 2.1);
    click(&mut se, &mut sk, 12.0, 2.0);
    click(&mut se, &mut sk, 6.0, 8.0);
    let (a, b) = ends(&sk, lines(&sk, false)[0]);
    assert!(kinds(&sk).contains(&K::Coincident(a, p)));
    se.selection = vec![Item::Point(a), Item::Point(b)];
    se.apply(&mut sk, ConstraintAction::Distance, Some(25.0))
        .unwrap();
    assert!(xy(&sk, a).distance(at(2.0, 2.0)) < 1e-6);
    assert!((xy(&sk, a).distance(xy(&sk, b)) - 25.0).abs() < 1e-6);
    assert_solved(&sk);
}

#[test]
fn center_on_the_origin_is_fixed_and_on_a_point_coincident() {
    let (mut se, mut sk) = session(ToolKind::RectCenter);
    click(&mut se, &mut sk, 0.1, 0.1);
    click(&mut se, &mut sk, 8.0, 7.0);
    assert_eq!(
        count(
            &sk,
            |k| matches!(k, K::Fixed(_, x, y) if *x == 0.0 && *y == 0.0)
        ),
        1
    );
    assert_eq!(assert_solved(&sk).dof, 2);
    let m = kinds(&sk)
        .iter()
        .find_map(|k| match k {
            K::Midpoint(m, _) => Some(*m),
            _ => None,
        })
        .unwrap();
    assert!(close(xy(&sk, m), at(0.0, 0.0)));

    let (mut se, mut sk) = session(ToolKind::RectCenter);
    let p = sk.add_point(5.0, 5.0).unwrap();
    let q = sk.add_point(8.0, 7.0).unwrap();
    click(&mut se, &mut sk, 5.1, 5.1);
    click(&mut se, &mut sk, 8.1, 6.9);
    assert_eq!(count(&sk, |k| matches!(k, K::Coincident(..))), 2);
    assert!(
        kinds(&sk)
            .iter()
            .any(|k| matches!(k, K::Coincident(_, x) if *x == p))
    );
    assert!(
        kinds(&sk)
            .iter()
            .any(|k| matches!(k, K::Coincident(_, x) if *x == q))
    );
    assert_solved(&sk);
}

// ---- Review-Focus 5: Zoll und Formeln ------------------------------------------------------------

#[test]
fn inch_width_stores_its_unit_and_a_formula_stays_at_the_dimension() {
    with_unit(DisplayUnit::Zoll, || {
        let mut w = rig(ToolKind::Rect3P);
        commit(&mut w, 1.0, 1.0).unwrap();
        type_in(&mut w, F::Width, "1");
        assert!(close(resolved(&mut w, 40.0, 1.0), at(26.4, 1.0)));
        commit(&mut w, 40.0, 1.0).unwrap();
        type_in(&mut w, F::Depth, "Breite / 2");
        commit(&mut w, 5.0, 9.0).unwrap();
        let ds = dims(sk(&w));
        assert_eq!(ds.len(), 2);
        let (width, depth) = (ds[0].0, ds[1].0);
        assert!(matches!(ds[0].1, K::Distance(_, _, d) if d == 25.4));
        assert!(matches!(ds[1].1, K::Distance(_, _, d) if d == 20.0));
        assert_eq!(
            draft(&mut w).formulas.get(&width).map(String::as_str),
            Some("1 in")
        );
        assert_eq!(
            draft(&mut w).formulas.get(&depth).map(String::as_str),
            Some("Breite / 2")
        );

        let mut w = rig(ToolKind::RectCenter);
        commit(&mut w, 1.0, 1.0).unwrap();
        type_in(&mut w, F::Width, "Breite / 2");
        type_in(&mut w, F::Height, "1");
        commit(&mut w, 5.0, 9.0).unwrap();
        let ds = dims(sk(&w));
        assert_eq!(
            draft(&mut w).formulas.get(&ds[0].0).map(String::as_str),
            Some("Breite / 2")
        );
        assert_eq!(
            draft(&mut w).formulas.get(&ds[1].0).map(String::as_str),
            Some("1 in")
        );
    });
}

// ---- Felder -------------------------------------------------------------------------------------

#[test]
fn a_typed_value_survives_pointer_moves_and_bad_text_blocks_the_last_click() {
    let mut w = rig(ToolKind::Circle2P);
    commit(&mut w, 1.0, 1.0).unwrap();
    type_in(&mut w, F::Diameter, "30");
    for (x, y) in [(5.0, 9.0), (-3.0, 2.0), (40.0, -7.0)] {
        assert!((resolved(&mut w, x, y).distance(at(1.0, 1.0)) - 30.0).abs() < 1e-9);
    }
    let before = bits(sk(&w));
    type_in(&mut w, F::Diameter, "2 +");
    assert!(commit(&mut w, 5.0, 9.0).is_err());
    assert_eq!(bits(sk(&w)), before);
    assert_eq!(parts(&mut w).0.pending_pos.len(), 1);
    type_in(&mut w, F::Diameter, "30");
    commit(&mut w, 5.0, 9.0).unwrap();
    assert_eq!(dims(sk(&w)).len(), 1);
}

// ---- Fahrten über Rohereignisse bei 1920×1080, Review-Focus 3 ------------------------------------

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
        let root = common::temp_root("planora-sketch-kinds-rc-gui");
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
        se.set_tool(sk, planora_app::sketch_state::Tool::Line);
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

/// `drive` zeichnet genau eine Form: ein Schritt, Rückgängig nimmt alles, Wiederholen stellt es her.
fn one_undo_step(gui: &mut Gui, drive: impl FnOnce(&mut Gui)) -> Sketch {
    let (before, steps) = (state_of(&gui.sketch()), gui.steps());
    drive(gui);
    gui.settle();
    let done = gui.sketch();
    assert_ne!(state_of(&done), before, "es entstand etwas");
    assert_eq!(gui.steps(), steps + 1, "genau ein Schritt");
    gui.undo();
    assert_eq!(state_of(&gui.sketch()), before, "alles weg");
    gui.redo();
    assert_eq!(state_of(&gui.sketch()), state_of(&done), "bitgleich zurück");
    done
}

#[test]
fn rect_3p_with_fields_by_raw_events_is_one_undo_step() {
    let mut gui = Gui::new();
    gui.choose(ToolKind::Rect3P);
    let done = one_undo_step(&mut gui, |g| {
        g.click(800.0, 500.0);
        g.frame(vec![Event::PointerMoved(pos2(1000.0, 400.0))]);
        g.settle();
        g.type_text("30");
        g.key(Key::Tab);
        g.type_text("30");
        g.key(Key::Enter);
        assert_eq!(g.st().sketch_session.as_ref().unwrap().pending_pos.len(), 2);
        g.frame(vec![Event::PointerMoved(pos2(1000.0, 700.0))]);
        g.settle();
        g.type_text("12");
        g.key(Key::Enter);
    });
    assert_eq!(lines(&done, false).len(), 4);
    assert_eq!(dims(&done).len(), 3, "Breite, Winkel, Tiefe");
    assert_eq!(lines(&done, true).len(), 1, "Achshilfe gehört zum Schritt");
}

#[test]
fn rect_center_by_raw_events_is_one_undo_step() {
    let mut gui = Gui::new();
    gui.choose(ToolKind::RectCenter);
    let done = one_undo_step(&mut gui, |g| {
        g.click(800.0, 500.0);
        g.click(1000.0, 620.0);
    });
    assert_eq!(lines(&done, false).len(), 4);
    assert_eq!(lines(&done, true).len(), 2);
}

#[test]
fn circle_2p_and_3p_by_raw_events_are_one_undo_step_each() {
    let mut gui = Gui::new();
    gui.choose(ToolKind::Circle2P);
    let done = one_undo_step(&mut gui, |g| {
        g.click(800.0, 500.0);
        g.click(1000.0, 620.0);
    });
    assert_eq!(done.curves().count(), 1);

    let mut gui = Gui::new();
    gui.choose(ToolKind::Circle3P);
    let done = one_undo_step(&mut gui, |g| {
        g.click(800.0, 500.0);
        g.click(1000.0, 520.0);
        g.click(950.0, 700.0);
    });
    assert_eq!(done.curves().count(), 1);
}

#[test]
fn snapped_circle_by_raw_events_takes_the_binding_along() {
    let mut gui = Gui::new();
    gui.choose(planora_app::sketch_kinds::ToolKind::Rect2P);
    gui.click(800.0, 500.0);
    gui.click(1000.0, 620.0);
    assert_eq!(lines(&gui.sketch(), false).len(), 4);
    gui.choose(ToolKind::Circle2P);
    let done = one_undo_step(&mut gui, |g| {
        // Auf zwei Ecken des Rechtecks.
        g.click(800.0, 500.0);
        g.click(1000.0, 620.0);
    });
    assert_eq!(count(&done, |k| matches!(k, K::PointOnCurve(..))), 2);
}

#[test]
fn circle_2t_by_raw_events_is_one_undo_step() {
    let mut gui = Gui::new();
    gui.draw_line(pos2(600.0, 400.0), pos2(1100.0, 400.0));
    gui.draw_line(pos2(1300.0, 300.0), pos2(1300.0, 800.0));
    gui.choose(ToolKind::Circle2T);
    let done = one_undo_step(&mut gui, |g| {
        g.click(850.0, 400.0);
        g.click(1300.0, 550.0);
        g.frame(vec![Event::PointerMoved(pos2(1150.0, 520.0))]);
        g.settle();
        g.type_text("10");
        g.key(Key::Enter);
    });
    assert_eq!(count(&done, |k| matches!(k, K::Tangent(..))), 2);
    let (_, _, r) = only_circle(&done);
    assert!((r - 5.0).abs() < 1e-9);
    assert_eq!(dims(&done).len(), 1);
}

#[test]
fn circle_3t_by_raw_events_is_one_undo_step_and_matches_the_preview() {
    let mut gui = Gui::new();
    gui.draw_line(pos2(600.0, 700.0), pos2(1100.0, 700.0));
    gui.draw_line(pos2(550.0, 650.0), pos2(550.0, 250.0));
    gui.draw_line(pos2(600.0, 250.0), pos2(1100.0, 650.0));
    gui.choose(ToolKind::Circle3T);
    let done = one_undo_step(&mut gui, |g| {
        g.click(850.0, 700.0);
        g.click(550.0, 450.0);
        let p = pos2(850.0, 450.0);
        g.frame(vec![Event::PointerMoved(p)]);
        g.frame(vec![Event::PointerMoved(p)]);
        let plan = {
            let raw = {
                let se = g.st().sketch_session.as_ref().unwrap();
                se.draw_cursor.as_ref().expect("Zeiger über der Skizze").raw
            };
            let sk = g.sketch();
            let se = g.st().sketch_session.as_ref().unwrap();
            se.kind_plan(&sk, raw)
        };
        let Some(ShapePlan::Circle(m, r)) = plan else {
            panic!("Vorschau fehlt: {plan:?}");
        };
        g.click(850.0, 450.0);
        let (_, center, radius) = only_circle(&g.sketch());
        assert!(center.distance(m) < 1e-6 && (radius - r).abs() < 1e-6);
    });
    assert_eq!(count(&done, |k| matches!(k, K::Tangent(..))), 3);
}

#[test]
fn escape_leaves_nothing_of_a_picked_tangent_circle() {
    let mut gui = Gui::new();
    gui.draw_line(pos2(600.0, 400.0), pos2(1100.0, 400.0));
    gui.choose(ToolKind::Circle2T);
    let (before, steps) = (state_of(&gui.sketch()), gui.steps());
    gui.click(850.0, 400.0);
    assert_eq!(gui.st().sketch_session.as_ref().unwrap().kind_stage(), 1);
    gui.key(Key::Escape);
    assert_eq!(gui.st().sketch_session.as_ref().unwrap().kind_stage(), 0);
    assert_eq!(state_of(&gui.sketch()), before);
    assert_eq!(gui.steps(), steps);
}

#[test]
fn messages_exist_in_both_languages() {
    for lang in [planora_i18n::german(), planora_i18n::english()] {
        planora_i18n::with_lang(&lang, || {
            for key in [
                "sketch-circle-collinear",
                "sketch-tangent-kinds",
                "sketch-tangent-lines-only",
                "sketch-tangent-no-solution",
            ] {
                assert!(!t(key).is_empty() && t(key) != key, "{key}");
            }
        });
    }
}
