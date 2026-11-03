//! Skizzen 4, Task 10: Skalieren in der App, über die Sitzung wie die Oberfläche: Stufen, Griff,
//! Feld, Formeln, ganze Skizze, feste Geometrie, Abbruch und Rückgängig.

use planora_app::sketch_grips::Grip;
use planora_app::sketch_input::FieldKind as F;
use planora_app::sketch_state::{Item, Tool, TransformStage};
use planora_app::state::{AppState, StandardPlane};
use planora_feature::{ParamEdit, ParamUnit, SketchPlane};
use planora_i18n::{DisplayUnit, t, with_unit};
use planora_math::DVec2;
use planora_sketch::constraint::{ConstraintId, ConstraintKind as K};
use planora_sketch::model::{CurveId, CurveKind, PointId, Sketch};
use planora_sketch::solver::SolveStatus;
use std::f64::consts::FRAC_PI_4;

const PICK: f64 = 0.5;
const TOL: f64 = 1e-9;

fn v(x: f64, y: f64) -> DVec2 {
    DVec2::new(x, y)
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
        assert_eq!(sk.solve().status, SolveStatus::Solved);
    }
    s.observe_sketch(false);
    s
}

fn sketch(s: &AppState) -> &Sketch {
    s.session_sketch().unwrap()
}

fn se(s: &AppState) -> &planora_app::sketch_state::SketchSession {
    s.sketch_session.as_ref().unwrap()
}

fn pos(sk: &Sketch, p: PointId) -> DVec2 {
    sk.point_xy(p).unwrap()
}

fn bits(sk: &Sketch) -> Vec<u64> {
    sk.variables().iter().map(|x| x.to_bits()).collect()
}

fn snapshot(sk: &Sketch) -> String {
    let curves: Vec<String> = sk
        .curves()
        .map(|(id, c)| format!("{} {:?} {}", id.index(), c.kind, c.construction))
        .collect();
    let constraints: Vec<String> = sk
        .constraints()
        .map(|(id, c)| format!("{} {:?}", id.index(), c.kind))
        .collect();
    format!("{curves:?}\n{:?}\n{constraints:?}", bits(sk))
}

fn undo_len(s: &AppState) -> usize {
    s.sketch_history().unwrap().undo_len()
}

fn message(s: &AppState) -> Option<String> {
    se(s).message_text()
}

fn curves_of(ids: &[CurveId]) -> Vec<Item> {
    ids.iter().map(|&c| Item::Curve(c)).collect()
}

/// Wählt `items` vor, startet den Maßstab (Stufe `SetBase`) und setzt den Bezugspunkt mit einem
/// Klick bei `base`; mit Fang auf einen Punkt, wo einer liegt.
fn scale_about(s: &mut AppState, items: Vec<Item>, base: DVec2) {
    let (se, sk) = s.session_sketch_mut().unwrap();
    se.selection = items;
    se.set_tool(sk, Tool::Scale);
    assert_eq!(se.transform.stage, TransformStage::SetBase);
    se.click(sk, base, PICK, false).unwrap();
    assert_eq!(se.transform.stage, TransformStage::Active);
    se.sync_input(sk);
}

fn type_in(s: &mut AppState, kind: F, text: &str) {
    let scope = s.document.param_scope();
    let (se, sk) = s.session_sketch_mut().unwrap();
    se.sync_input(sk);
    let i = se
        .input
        .fields
        .iter()
        .position(|f| f.kind == kind)
        .expect("Feld");
    se.type_field(i, text, &scope);
}

fn finish(s: &mut AppState) -> Result<(), String> {
    let (se, sf) = s.session_draft_mut().unwrap();
    let result = se.confirm_transform(sf);
    s.observe_sketch(false);
    result
}

fn polylines(sk: &Sketch, ids: &[CurveId]) -> Vec<Vec<DVec2>> {
    ids.iter()
        .map(|&c| {
            sk.curve_polyline(c, planora_app::sketch_kinds::PREVIEW_STEPS_PER_TURN)
                .unwrap()
        })
        .collect()
}

fn assert_same_lines(a: &[Vec<DVec2>], b: &[Vec<DVec2>]) {
    assert_eq!(a.len(), b.len());
    for (x, y) in a.iter().zip(b) {
        assert_eq!(x.len(), y.len());
        for (p, q) in x.iter().zip(y) {
            assert!(p.distance(*q) < TOL, "Vorschau {p} gegen Ergebnis {q}");
        }
    }
}

fn value_of(sk: &Sketch, id: ConstraintId) -> f64 {
    match sk.constraint(id).unwrap().kind {
        K::Distance(_, _, x) | K::Radius(_, x) | K::Angle(_, _, x) => x,
        ref k => panic!("kein Maß: {k:?}"),
    }
}

fn line_ends(sk: &Sketch, c: CurveId) -> (DVec2, DVec2) {
    match sk.curve(c).unwrap().kind {
        CurveKind::Line { a, b } => (pos(sk, a), pos(sk, b)),
        _ => panic!("keine Linie"),
    }
}

/// Rechteck mit zwei Abständen, Kreis mit Radius, gewinkelte Linie mit Winkel.
struct Shapes {
    rect: planora_sketch::helpers::RectIds,
    width: ConstraintId,
    height: ConstraintId,
    circle: CurveId,
    radius: ConstraintId,
    lines: [CurveId; 2],
    angle: ConstraintId,
}

fn shapes() -> (AppState, Shapes) {
    let mut out = None;
    let s = sketch_with(|sk| {
        let rect = sk.add_rectangle(v(0.0, 0.0), v(20.0, 10.0)).unwrap();
        let width = sk
            .add_constraint(K::Distance(rect.points[0], rect.points[1], 20.0))
            .unwrap();
        let height = sk
            .add_constraint(K::Distance(rect.points[1], rect.points[2], 10.0))
            .unwrap();
        let m = sk.add_point(40.0, 5.0).unwrap();
        let circle = sk.add_circle(m, 4.0).unwrap();
        let radius = sk.add_constraint(K::Radius(circle, 4.0)).unwrap();
        let p = sk.add_point(60.0, 0.0).unwrap();
        let q = sk.add_point(70.0, 0.0).unwrap();
        let r = sk.add_point(67.0, 7.0).unwrap();
        let l1 = sk.add_line(p, q).unwrap();
        let l2 = sk.add_line(p, r).unwrap();
        let angle = sk.add_constraint(K::Angle(l1, l2, FRAC_PI_4)).unwrap();
        out = Some(Shapes {
            rect,
            width,
            height,
            circle,
            radius,
            lines: [l1, l2],
            angle,
        });
    });
    (s, out.unwrap())
}

// ---- Ablauf ------------------------------------------------------------------------------------

#[test]
fn a_preselection_waits_for_the_base_and_a_click_makes_it_active() {
    let (mut s, sh) = shapes();
    {
        let (se, sk) = s.session_sketch_mut().unwrap();
        se.selection = curves_of(&sh.rect.lines);
        se.set_tool(sk, Tool::Scale);
        assert_eq!(se.transform.stage, TransformStage::SetBase);
        assert!(!se.transform.whole);
        assert_eq!(se.transform.base, Some(v(10.0, 5.0)));
        se.sync_input(sk);
        assert!(se.input.fields.is_empty());
        assert!(!se.grips_shown(), "beim Bezugspunkt keine Griffe");
        // Daneben geklickt, der Fang zieht auf die Ecke.
        se.click(sk, v(20.1, 10.1), PICK, false).unwrap();
        assert_eq!(se.transform.stage, TransformStage::Active);
        assert_eq!(se.transform.base, Some(v(20.0, 10.0)));
        se.sync_input(sk);
        let kinds: Vec<F> = se.input.fields.iter().map(|f| f.kind).collect();
        assert_eq!(kinds, [F::ScaleFactor]);
        assert!(se.grips_shown());
        assert!(!se.transform.copy);
    }
    // Eingabe beim Bezugspunkt nimmt die Mitte der Hülle.
    let (mut s, sh) = shapes();
    let (se, sk) = s.session_sketch_mut().unwrap();
    se.selection = curves_of(&sh.rect.lines);
    se.set_tool(sk, Tool::Scale);
    se.finish_tool(sk).unwrap();
    assert_eq!(se.transform.stage, TransformStage::Active);
    assert_eq!(se.transform.base, Some(v(10.0, 5.0)));
}

#[test]
fn picking_shows_the_grip_as_soon_as_something_is_selected() {
    let (mut s, sh) = shapes();
    let (se, sk) = s.session_sketch_mut().unwrap();
    se.selection.clear();
    se.set_tool(sk, Tool::Scale);
    assert_eq!(se.transform.stage, TransformStage::Picking);
    assert!(!se.grips_shown());
    se.click(sk, v(10.0, 0.0), PICK, false).unwrap();
    assert_eq!(se.selection, vec![Item::Curve(sh.rect.lines[0])]);
    assert!(se.grips_shown(), "Griffe schon beim Wählen");
    // Der Griff beendet das Wählen: Faktor mit dem Zeiger, Bezugspunkt in der Mitte der Hülle.
    let base = se.transform.base.unwrap();
    assert_eq!(base, v(10.0, 0.0));
    se.begin_grip(Grip::Scale, base + v(6.0, 0.0));
    se.grip_to(base + v(12.0, 0.0));
    se.end_grip();
    assert_eq!(se.transform.stage, TransformStage::Active);
    assert!((se.transform.factor - 2.0).abs() < 1e-12);
    assert!((se.transform_now().unwrap().factor - 2.0).abs() < 1e-12);
    // Ein zweiter Klick auf die Linie schaltet sie wieder ab; Eingabe ohne Auswahl nimmt die Skizze.
    let (mut s, _) = shapes();
    let (se, sk) = s.session_sketch_mut().unwrap();
    se.selection.clear();
    se.set_tool(sk, Tool::Scale);
    se.click(sk, v(10.0, 0.0), PICK, false).unwrap();
    se.click(sk, v(10.0, 0.0), PICK, false).unwrap();
    assert!(se.selection.is_empty() && !se.grips_shown());
}

#[test]
fn enter_while_picking_goes_to_the_base_with_a_selection() {
    let (mut s, sh) = shapes();
    let (se, sk) = s.session_sketch_mut().unwrap();
    se.selection.clear();
    se.set_tool(sk, Tool::Scale);
    se.click(sk, v(10.0, 0.0), PICK, false).unwrap();
    se.finish_tool(sk).unwrap();
    assert_eq!(se.transform.stage, TransformStage::SetBase);
    assert!(!se.transform.whole);
    assert_eq!(se.transform.set.curves, vec![sh.rect.lines[0]]);
    assert_eq!(se.message, None);
}

#[test]
fn the_grip_gives_the_ratio_of_the_distances_and_a_field_wins() {
    let (mut s, sh) = shapes();
    scale_about(&mut s, curves_of(&sh.rect.lines), v(0.0, 0.0));
    {
        let se = s.sketch_session.as_mut().unwrap();
        se.begin_grip(Grip::Scale, v(6.0, 0.0));
        se.grip_to(v(12.0, 0.0));
        assert!((se.transform_now().unwrap().factor - 2.0).abs() < 1e-12);
        // Dichte Folge entlang einer Geraden durch den Bezugspunkt und über ihn hinaus: stetig und
        // nie null oder negativ.
        let mut last = se.transform.factor;
        for k in 0..=2400 {
            let x = 12.0 - f64::from(k) * 0.01;
            se.grip_to(v(x, 0.0));
            let f = se.transform.factor;
            assert!(f > 0.0 && f.is_finite(), "x {x} f {f}");
            assert!((f - x.abs() / 6.0).abs() < 1e-5, "x {x} f {f}");
            assert!((f - last).abs() < 0.01 / 6.0 + 1e-9, "Sprung bei {x}");
            last = f;
        }
        // Schräg: nur der Abstand zählt.
        se.grip_to(v(3.0, 4.0));
        assert!((se.transform.factor - 5.0 / 6.0).abs() < 1e-12);
        se.end_grip();
    }
    // Das Feld gewinnt vor dem Griff; leer gilt wieder der Griff.
    type_in(&mut s, F::ScaleFactor, "3");
    assert_eq!(se(&s).transform_now().unwrap().factor, 3.0);
    type_in(&mut s, F::ScaleFactor, "");
    assert!((se(&s).transform_now().unwrap().factor - 5.0 / 6.0).abs() < 1e-12);
}

// ---- Rechnung: Vorschau = Ergebnis ---------------------------------------------------------------

#[test]
fn factor_two_about_a_corner_doubles_the_dimensions_and_the_preview_is_the_result() {
    let (mut s, sh) = shapes();
    let mut curves = sh.rect.lines.to_vec();
    curves.push(sh.circle);
    curves.extend(sh.lines);
    scale_about(&mut s, curves_of(&curves), v(0.0, 0.0));
    type_in(&mut s, F::ScaleFactor, "2");
    let corner = sh.rect.points[0];
    let lines = se(&s)
        .transform_preview(sketch(&s))
        .expect("Vorschau")
        .clone();
    let undo = undo_len(&s);
    finish(&mut s).unwrap();
    let sk = sketch(&s);
    assert_eq!(value_of(sk, sh.width), 40.0);
    assert_eq!(value_of(sk, sh.height), 20.0);
    assert_eq!(value_of(sk, sh.radius), 8.0);
    assert!(
        (value_of(sk, sh.angle) - FRAC_PI_4).abs() < TOL,
        "Winkel gleich"
    );
    assert_eq!(sk.radius(sh.circle), Some(8.0));
    assert_eq!(pos(sk, corner), v(0.0, 0.0), "die Ecke steht");
    assert_eq!(pos(sk, sh.rect.points[2]), v(40.0, 20.0));
    let (a, b) = line_ends(sk, sh.lines[1]);
    assert!(a.distance(v(60.0, 0.0)) < 1e-9 || a.distance(v(120.0, 0.0)) < 1e-9);
    assert!((a.distance(b) - 2.0 * v(7.0, 7.0).length()).abs() < 1e-9);
    let report = se(&s).last_report.as_ref().unwrap();
    assert_eq!(report.status, SolveStatus::Solved);
    assert_eq!(report.iterations, 0);
    assert!(report.redundant.is_empty() && report.conflicting.is_empty());
    assert_same_lines(&lines, &polylines(sk, &curves));
    assert_eq!(se(&s).tool, Tool::Select);
    assert_eq!(se(&s).selection.len(), curves.len(), "die Auswahl bleibt");
    assert_eq!(undo_len(&s), undo + 1);
}

/// Dichte Folge von Faktoren: In jedem Stand ist die Vorschau das Ergebnis, die Maße folgen, oder
/// beide lehnen ab und die Skizze bleibt bitgleich.
#[test]
fn the_preview_is_the_result_over_a_dense_sequence_of_factors() {
    let mut rejected = 0;
    let mut accepted = 0;
    for k in 0..=240 {
        let factor = 10f64.powf(-9.0 + f64::from(k) * 0.0575);
        let (mut s, sh) = shapes();
        let mut curves = sh.rect.lines.to_vec();
        curves.push(sh.circle);
        scale_about(&mut s, curves_of(&curves), v(0.0, 0.0));
        let before = snapshot(sketch(&s));
        let undo = undo_len(&s);
        let lines = {
            let (se, sk) = s.session_sketch_mut().unwrap();
            se.transform.factor = factor;
            se.transform_preview(sk)
        };
        let done = finish(&mut s);
        match lines {
            Some(lines) => {
                done.unwrap_or_else(|e| panic!("{factor}: {e}"));
                accepted += 1;
                let sk = sketch(&s);
                assert_same_lines(&lines, &polylines(sk, &curves));
                let tol = 1e-9 * factor.max(1.0);
                assert!(
                    (value_of(sk, sh.width) - 20.0 * factor).abs() < tol,
                    "{factor}"
                );
                assert!(
                    (value_of(sk, sh.height) - 10.0 * factor).abs() < tol,
                    "{factor}"
                );
                assert!(
                    (value_of(sk, sh.radius) - 4.0 * factor).abs() < tol,
                    "{factor}"
                );
                let report = se(&s).last_report.as_ref().unwrap();
                assert_eq!(report.status, SolveStatus::Solved, "{factor}");
                assert_eq!(report.iterations, 0, "{factor}");
                assert_eq!(undo_len(&s), undo + 1);
            }
            None => {
                rejected += 1;
                assert!(done.is_err(), "{factor}");
                assert_eq!(snapshot(sketch(&s)), before, "{factor}");
                assert_eq!(undo_len(&s), undo);
                assert_eq!(se(&s).tool, Tool::Scale, "die Stufe bleibt");
                assert_eq!(se(&s).transform.stage, TransformStage::Active);
                assert!(message(&s).is_some(), "{factor}");
            }
        }
    }
    assert!(rejected > 0, "ein winziger Faktor wird abgelehnt");
    assert!(accepted > 100);
}

// ---- Formeln -----------------------------------------------------------------------------------

/// Rechteck 20×10 und gewinkelte Linie; Breite = 40 und Winkel = 45°. Das Abstandsmaß hängt an
/// `Breite / 2`, das Winkelmaß an `Wink`.
fn formula_scene() -> (
    AppState,
    Shapes,
    planora_feature::FeatureId,
    planora_feature::ParamId,
) {
    let (mut s, sh) = shapes();
    let id = s.open_sketch_id().unwrap();
    s.close_sketch();
    let breite = s.add_parameter().unwrap();
    s.edit_parameter(
        breite,
        ParamEdit {
            name: "Breite".into(),
            formula: "40".into(),
            unit: ParamUnit::Length,
            comment: String::new(),
        },
    )
    .unwrap();
    let wink = s.add_parameter().unwrap();
    s.edit_parameter(
        wink,
        ParamEdit {
            name: "Wink".into(),
            formula: "45".into(),
            unit: ParamUnit::Angle,
            comment: String::new(),
        },
    )
    .unwrap();
    s.open_sketch(id).unwrap();
    let scope = s.document.param_scope();
    {
        let (se, sf) = s.session_draft_mut().unwrap();
        se.set_dimension_input(sf, sh.width, "Breite / 2", &scope)
            .unwrap();
        se.set_dimension_input(sf, sh.angle, "Wink", &scope)
            .unwrap();
    }
    s.observe_sketch(false);
    (s, sh, id, breite)
}

fn set_breite(s: &mut AppState, breite: planora_feature::ParamId, formula: &str) {
    let p = s.document.parameter(breite).unwrap().clone();
    s.edit_parameter(
        breite,
        ParamEdit {
            name: p.name,
            formula: formula.into(),
            unit: p.unit,
            comment: p.comment,
        },
    )
    .unwrap();
}

fn formulas(s: &AppState) -> std::collections::BTreeMap<ConstraintId, String> {
    s.sketch_draft.as_ref().unwrap().formulas.clone()
}

#[test]
fn a_formula_dimension_becomes_old_times_k_and_follows_its_parameter() {
    let (mut s, sh, id, breite) = formula_scene();
    let mut curves = sh.rect.lines.to_vec();
    curves.extend(sh.lines);
    assert_eq!(formulas(&s)[&sh.width], "Breite / 2");

    scale_about(&mut s, curves_of(&curves), v(0.0, 0.0));
    type_in(&mut s, F::ScaleFactor, "3");
    finish(&mut s).unwrap();
    let f = formulas(&s);
    assert_eq!(f[&sh.width], "(Breite / 2) * 3");
    assert_eq!(f[&sh.angle], "Wink", "ein Winkel bleibt unberührt");
    assert!(!f.contains_key(&sh.height), "ohne Formel bleibt ohne");
    assert_eq!(value_of(sketch(&s), sh.width), 60.0);
    assert_eq!(value_of(sketch(&s), sh.height), 30.0);

    // Zum zweiten Mal skaliert: verschachtelt, mit Dezimalpunkt.
    scale_about(&mut s, curves_of(&curves), v(0.0, 0.0));
    type_in(&mut s, F::ScaleFactor, "0,5");
    finish(&mut s).unwrap();
    assert_eq!(formulas(&s)[&sh.width], "((Breite / 2) * 3) * 0.5");
    assert_eq!(value_of(sketch(&s), sh.width), 30.0);

    // Der Parameter treibt weiter: Breite 50 gibt 25 · 3 · 0,5.
    s.close_sketch();
    set_breite(&mut s, breite, "50");
    let sk = s.document.sketch_of(id).unwrap();
    assert_eq!(value_of(sk, sh.width), 37.5);
    assert!((value_of(sk, sh.angle) - FRAC_PI_4).abs() < 1e-9);
    assert_eq!(value_of(sk, sh.height), 15.0);
    let status = sk.clone().solve();
    assert_eq!(status.status, SolveStatus::Solved);
}

#[test]
fn a_scaled_formula_gives_the_value_after_the_parameter_changes_whatever_k() {
    for factor in ["2", "0.25", "7", "1,5"] {
        let (mut s, sh, id, breite) = formula_scene();
        let curves = curves_of(&sh.rect.lines);
        scale_about(&mut s, curves, v(0.0, 0.0));
        type_in(&mut s, F::ScaleFactor, factor);
        let k = se(&s).transform_now().unwrap().factor;
        finish(&mut s).unwrap();
        assert_eq!(value_of(sketch(&s), sh.width), 20.0 * k, "{factor}");
        s.close_sketch();
        set_breite(&mut s, breite, "60");
        let sk = s.document.sketch_of(id).unwrap();
        assert!(
            (value_of(sk, sh.width) - 30.0 * k).abs() < 1e-9,
            "{factor}: {}",
            value_of(sk, sh.width)
        );
    }
}

#[test]
fn undo_restores_the_formulas_and_redo_brings_them_back_bit_for_bit() {
    let (mut s, sh, _, _) = formula_scene();
    scale_about(&mut s, curves_of(&sh.rect.lines), v(0.0, 0.0));
    type_in(&mut s, F::ScaleFactor, "3");
    let (before, before_f) = (snapshot(sketch(&s)), formulas(&s));
    let undo = undo_len(&s);
    finish(&mut s).unwrap();
    let (after, after_f) = (snapshot(sketch(&s)), formulas(&s));
    assert_ne!(before_f, after_f);
    assert_eq!(undo_len(&s), undo + 1, "ein Schritt");
    assert_eq!(s.sketch_undo_label().unwrap(), t("undo-scale"));
    assert!(s.undo_sketch());
    assert_eq!(snapshot(sketch(&s)), before);
    assert_eq!(formulas(&s), before_f, "die Formeln kommen zurück");
    assert!(s.redo_sketch());
    assert_eq!(snapshot(sketch(&s)), after, "bitgleich");
    assert_eq!(formulas(&s), after_f);
    assert_eq!(undo_len(&s), undo + 1);
}

#[test]
fn inches_leave_the_factor_alone_and_the_length_dimension_stays_right() {
    with_unit(DisplayUnit::Zoll, || {
        let (mut s, sh) = shapes();
        // Ein Maß mit Formel in Zoll: „1“ ist ein Zoll und hat den Speichertext „1 in“.
        let scope = s.document.param_scope();
        {
            let (se, sf) = s.session_draft_mut().unwrap();
            se.set_dimension_input(sf, sh.height, "1", &scope).unwrap();
        }
        s.observe_sketch(false);
        let formula = formulas(&s).get(&sh.height).cloned();
        scale_about(&mut s, curves_of(&sh.rect.lines), v(0.0, 0.0));
        type_in(&mut s, F::ScaleFactor, "2");
        assert_eq!(se(&s).transform_now().unwrap().factor, 2.0);
        {
            let (se, sf) = s.session_draft_mut().unwrap();
            assert_eq!(
                planora_app::sketch_inline::suggestion(se, sf, F::ScaleFactor),
                "2"
            );
        }
        finish(&mut s).unwrap();
        let sk = sketch(&s);
        assert_eq!(value_of(sk, sh.width), 40.0, "mm bleiben mm");
        assert!((value_of(sk, sh.height) - 50.8).abs() < 1e-9);
        let f = formula.expect("in Zoll hat auch eine Zahl einen Speichertext");
        assert_eq!(formulas(&s)[&sh.height], format!("({f}) * 2"));
    });
}

#[test]
fn the_suggestion_of_the_factor_is_a_plain_number_in_every_unit() {
    for unit in [DisplayUnit::Zoll, DisplayUnit::Millimeter] {
        with_unit(unit, || {
            let (mut s, sh) = shapes();
            scale_about(&mut s, curves_of(&sh.rect.lines), v(0.0, 0.0));
            for (factor, text) in [(1.0, "1"), (2.0, "2"), (0.5, "0,5"), (12.25, "12,25")] {
                s.sketch_session.as_mut().unwrap().transform.factor = factor;
                let (se, sf) = s.session_draft_mut().unwrap();
                let shown = planora_app::sketch_inline::suggestion(se, sf, F::ScaleFactor);
                assert_eq!(shown.replace('.', ","), text, "{unit:?}");
            }
        });
    }
}

// ---- Ganze Skizze --------------------------------------------------------------------------------

struct Whole {
    origin: PointId,
    axis: CurveId,
    rect: planora_sketch::helpers::RectIds,
    spoke: CurveId,
    angle: ConstraintId,
    width: ConstraintId,
    edge: CurveId,
}

fn whole() -> (AppState, Whole) {
    let mut out = None;
    let s = sketch_with(|sk| {
        let origin = sk.add_point(0.0, 0.0).unwrap();
        sk.add_constraint(K::Fixed(origin, 0.0, 0.0)).unwrap();
        let fixed_line = |sk: &mut Sketch, a: DVec2, b: DVec2| {
            let (p, q) = (
                sk.add_point(a.x, a.y).unwrap(),
                sk.add_point(b.x, b.y).unwrap(),
            );
            let l = sk.add_line(p, q).unwrap();
            sk.add_constraint(K::Fixed(p, a.x, a.y)).unwrap();
            sk.add_constraint(K::Fixed(q, b.x, b.y)).unwrap();
            sk.set_construction(l, true).unwrap();
            l
        };
        let axis = fixed_line(sk, v(-100.0, 0.0), v(100.0, 0.0));
        let edge = fixed_line(sk, v(0.0, -50.0), v(40.0, -50.0));
        let rect = sk.add_rectangle(v(0.0, 0.0), v(20.0, 10.0)).unwrap();
        sk.add_constraint(K::Coincident(rect.points[0], origin))
            .unwrap();
        let width = sk
            .add_constraint(K::Distance(rect.points[0], rect.points[1], 20.0))
            .unwrap();
        let q = sk.add_point(30.0, 30.0).unwrap();
        let spoke = sk.add_line(rect.points[0], q).unwrap();
        let angle = sk.add_constraint(K::Angle(axis, spoke, FRAC_PI_4)).unwrap();
        out = Some(Whole {
            origin,
            axis,
            rect,
            spoke,
            angle,
            width,
            edge,
        });
    });
    (s, out.unwrap())
}

fn to_whole(s: &mut AppState, base: DVec2) {
    let (se, sk) = s.session_sketch_mut().unwrap();
    se.selection.clear();
    se.set_tool(sk, Tool::Scale);
    assert_eq!(se.transform.stage, TransformStage::Picking);
    se.finish_tool(sk).unwrap();
    assert_eq!(se.transform.stage, TransformStage::SetBase);
    assert!(se.transform.whole);
    assert_eq!(se.message, None);
    se.click(sk, base, PICK, false).unwrap();
    assert_eq!(se.transform.stage, TransformStage::Active);
    se.sync_input(sk);
}

#[test]
fn enter_without_a_selection_takes_the_whole_sketch_and_keeps_fixed_geometry() {
    let (mut s, w) = whole();
    let held = (
        line_ends(sketch(&s), w.axis),
        line_ends(sketch(&s), w.edge),
        pos(sketch(&s), w.origin),
    );
    to_whole(&mut s, v(0.0, 0.0));
    assert_eq!(
        se(&s).transform.base,
        Some(v(0.0, 0.0)),
        "Fang auf den Ursprung"
    );
    type_in(&mut s, F::ScaleFactor, "2");
    let curves = [w.rect.lines.to_vec(), vec![w.spoke]].concat();
    let preview = se(&s).transform_preview(sketch(&s)).expect("Vorschau");
    assert_eq!(preview.len(), curves.len(), "ohne die feste Geometrie");
    finish(&mut s).unwrap();
    let sk = sketch(&s);
    assert_eq!(line_ends(sk, w.axis), held.0, "Achshilfslinie bleibt");
    assert_eq!(line_ends(sk, w.edge), held.1, "Kantenkopie bleibt");
    assert_eq!(pos(sk, w.origin), held.2);
    assert_eq!(pos(sk, w.rect.points[0]), v(0.0, 0.0));
    assert_eq!(pos(sk, w.rect.points[2]), v(40.0, 20.0));
    assert_eq!(value_of(sk, w.width), 40.0);
    // Der Winkel zur Achshilfslinie gilt weiter, die Abhängigkeit lebt.
    assert!((value_of(sk, w.angle) - FRAC_PI_4).abs() < TOL);
    let (a, b) = line_ends(sk, w.spoke);
    assert!(((b - a).angle_to(v(1.0, 0.0)).abs() - FRAC_PI_4).abs() < 1e-9);
    assert!((a.distance(b) - 2.0 * 30.0 * 2f64.sqrt()).abs() < 1e-9);
    let report = se(&s).last_report.as_ref().unwrap();
    assert_eq!(report.status, SolveStatus::Solved);
    assert_eq!(report.iterations, 0);
    assert!(report.redundant.is_empty() && report.conflicting.is_empty());
    assert_same_lines(&preview, &polylines(sk, &curves));
}

#[test]
fn the_whole_sketch_at_another_base_is_refused_at_once_and_bit_for_bit() {
    let (mut s, _) = whole();
    to_whole(&mut s, v(5.0, 5.0));
    type_in(&mut s, F::ScaleFactor, "2");
    let before = snapshot(sketch(&s));
    let undo = undo_len(&s);
    {
        let (se, sk) = s.session_sketch_mut().unwrap();
        se.update_transform_block(sk);
    }
    assert_eq!(message(&s).unwrap(), t("sketch-move-fixed"), "sofort");
    assert!(se(&s).transform_preview(sketch(&s)).is_none());
    assert!(finish(&mut s).is_err());
    assert_eq!(message(&s).unwrap(), t("sketch-move-fixed"));
    assert_eq!(snapshot(sketch(&s)), before, "bitgleich");
    assert_eq!(undo_len(&s), undo);
    assert_eq!(se(&s).tool, Tool::Scale);
    assert_eq!(se(&s).transform.stage, TransformStage::Active);
    // Mit Faktor 1 gibt es nichts zu sagen, und die Meldung der Sperre verschwindet.
    type_in(&mut s, F::ScaleFactor, "");
    {
        let (se, sk) = s.session_sketch_mut().unwrap();
        se.update_transform_block(sk);
    }
    assert_eq!(message(&s), None);
}

#[test]
fn a_dimension_to_the_axis_line_goes_when_the_whole_sketch_scales() {
    let (mut s, w) = whole();
    {
        let sk = s.session_sketch_mut().unwrap().1;
        sk.add_constraint(K::DistancePointLine(w.rect.points[2], w.axis, 10.0))
            .unwrap();
        assert_eq!(sk.solve().status, SolveStatus::Solved);
    }
    s.observe_sketch(false);
    to_whole(&mut s, v(0.0, 0.0));
    type_in(&mut s, F::ScaleFactor, "2");
    finish(&mut s).unwrap();
    let text = message(&s).expect("Meldung");
    assert!(
        text.contains(&planora_i18n::tr!("sketch-move-dropped", n = 1)),
        "{text}"
    );
}

// ---- Auswahl ---------------------------------------------------------------------------------------

#[test]
fn a_subset_drops_a_dimension_to_a_stranger_and_leaves_the_others_alone() {
    let (mut s, sh) = shapes();
    let cross = {
        let sk = s.session_sketch_mut().unwrap().1;
        // Maß zwischen einer Ecke des Rechtecks und dem Kreismittelpunkt.
        let CurveKind::Circle { center, .. } = sk.curve(sh.circle).unwrap().kind else {
            panic!()
        };
        let id = sk
            .add_constraint(K::Distance(sh.rect.points[1], center, 20.0))
            .unwrap();
        assert_eq!(sk.solve().status, SolveStatus::Solved);
        id
    };
    s.observe_sketch(false);
    let outside: Vec<DVec2> = {
        let sk = sketch(&s);
        let CurveKind::Circle { center, .. } = sk.curve(sh.circle).unwrap().kind else {
            panic!()
        };
        let (a, b) = line_ends(sk, sh.lines[0]);
        vec![pos(sk, center), a, b]
    };
    scale_about(&mut s, curves_of(&sh.rect.lines), v(0.0, 0.0));
    type_in(&mut s, F::ScaleFactor, "2");
    finish(&mut s).unwrap();
    let sk = sketch(&s);
    assert!(
        sk.constraint(cross).is_none(),
        "das Maß nach außen entfällt"
    );
    let text = message(&s).expect("Meldung");
    assert!(
        text.contains(&planora_i18n::tr!("sketch-move-dropped", n = 1)),
        "{text}"
    );
    assert_eq!(value_of(sk, sh.width), 40.0);
    // Kreis und Linien außerhalb: Punkte und Radius unberührt.
    let CurveKind::Circle { center, .. } = sk.curve(sh.circle).unwrap().kind else {
        panic!()
    };
    let (a, b) = line_ends(sk, sh.lines[0]);
    assert_eq!(vec![pos(sk, center), a, b], outside);
    assert_eq!(sk.radius(sh.circle), Some(4.0));
    assert_eq!(value_of(sk, sh.radius), 4.0);
    assert_eq!(value_of(sk, sh.angle), FRAC_PI_4);
}

#[test]
fn a_fixed_point_in_the_selection_gives_the_message_at_once() {
    let mut line = None;
    let mut s = sketch_with(|sk| {
        let a = sk.add_point(0.0, 0.0).unwrap();
        let b = sk.add_point(10.0, 0.0).unwrap();
        line = Some(sk.add_line(a, b).unwrap());
        sk.add_constraint(K::Fixed(b, 10.0, 0.0)).unwrap();
    });
    let line = line.unwrap();
    // Um den festen Punkt herum geht es nicht anders: er bewegt sich nur, wenn der Bezug anderswo liegt.
    scale_about(&mut s, vec![Item::Curve(line)], v(0.0, 0.0));
    type_in(&mut s, F::ScaleFactor, "2");
    let before = snapshot(sketch(&s));
    let undo = undo_len(&s);
    {
        let (se, sk) = s.session_sketch_mut().unwrap();
        se.update_transform_block(sk);
    }
    assert_eq!(message(&s).unwrap(), t("sketch-move-fixed"));
    assert!(finish(&mut s).is_err());
    assert_eq!(snapshot(sketch(&s)), before);
    assert_eq!(undo_len(&s), undo);
    assert_eq!(se(&s).transform.stage, TransformStage::Active);
    // Um den festen Punkt selbst bewegt sich nichts: Der Bezug im festen Punkt geht.
    let (se, sk) = s.session_sketch_mut().unwrap();
    se.begin_set_base();
    se.click(sk, v(10.0, 0.0), PICK, false).unwrap();
    se.update_transform_block(sk);
    assert_eq!(se.message, None);
}

// ---- Unmögliche Eingaben -----------------------------------------------------------------------------

#[test]
fn zero_negative_and_nothing_are_refused_or_do_nothing() {
    let (mut s, sh) = shapes();
    scale_about(&mut s, curves_of(&sh.rect.lines), v(0.0, 0.0));
    let before = snapshot(sketch(&s));
    let undo = undo_len(&s);
    for text in ["0", "-1", "-0.5", "kein Maß +"] {
        type_in(&mut s, F::ScaleFactor, text);
        assert!(finish(&mut s).is_err(), "{text}");
        if text != "kein Maß +" {
            assert_eq!(
                message(&s).unwrap(),
                t("sketch-field-factor-positive"),
                "{text}"
            );
        }
        assert_eq!(snapshot(sketch(&s)), before, "{text}");
        assert_eq!(undo_len(&s), undo);
        assert_eq!(se(&s).tool, Tool::Scale);
        assert_eq!(se(&s).transform.stage, TransformStage::Active);
    }
    // Leer und 1 ändern nichts und sagen es.
    for text in ["", "1"] {
        type_in(&mut s, F::ScaleFactor, text);
        assert!(finish(&mut s).is_err(), "{text:?}");
        assert_eq!(message(&s).unwrap(), t("sketch-move-nothing"), "{text:?}");
        assert_eq!(snapshot(sketch(&s)), before);
        assert_eq!(undo_len(&s), undo);
    }
    // Eine Zahl außerhalb des Bereichs.
    type_in(&mut s, F::ScaleFactor, "1e30");
    assert!(se(&s).transform_preview(sketch(&s)).is_none());
    assert!(finish(&mut s).is_err());
    assert_eq!(snapshot(sketch(&s)), before);
}

// ---- Abbruch und Rückgängig ------------------------------------------------------------------------

#[test]
fn esc_in_every_stage_leaves_no_trace_and_restores_the_selection() {
    for (stage, whole_sketch) in [
        (TransformStage::Picking, false),
        (TransformStage::SetBase, false),
        (TransformStage::SetBase, true),
        (TransformStage::Active, false),
        (TransformStage::Active, true),
    ] {
        let (mut s, sh) = shapes();
        {
            let (se, sk) = s.session_sketch_mut().unwrap();
            se.selection = if whole_sketch {
                Vec::new()
            } else {
                curves_of(&sh.rect.lines[..2])
            };
            if stage == TransformStage::Picking {
                se.selection.clear();
            }
            se.set_tool(sk, Tool::Scale);
            if stage == TransformStage::Picking {
                se.click(sk, v(10.0, 0.0), PICK, false).unwrap();
                assert_eq!(se.selection.len(), 1);
            } else if whole_sketch {
                se.finish_tool(sk).unwrap();
            }
            if stage == TransformStage::Active {
                se.finish_tool(sk).unwrap();
                se.transform.factor = 3.0;
            }
        }
        assert_eq!(se(&s).transform.stage, stage, "{whole_sketch}");
        let selection = se(&s).transform.selection_before.clone();
        assert_eq!(
            selection.len(),
            if stage == TransformStage::Picking || whole_sketch {
                0
            } else {
                2
            }
        );
        let (rev, undo, before) = (sketch(&s).revision(), undo_len(&s), snapshot(sketch(&s)));
        {
            let (se, sk) = s.session_sketch_mut().unwrap();
            se.cancel(sk);
        }
        s.observe_sketch(false);
        assert_eq!(se(&s).tool, Tool::Select, "{stage:?}");
        assert_eq!(sketch(&s).revision(), rev, "{stage:?}");
        assert_eq!(undo_len(&s), undo, "{stage:?}");
        assert_eq!(snapshot(sketch(&s)), before, "{stage:?}");
        assert_eq!(se(&s).transform.factor, 1.0);
        assert!(!se(&s).transform.whole);
        assert_eq!(se(&s).selection, selection, "{stage:?} {whole_sketch}");
    }
}

#[test]
fn esc_takes_back_only_the_grip_first() {
    let (mut s, sh) = shapes();
    scale_about(&mut s, curves_of(&sh.rect.lines), v(0.0, 0.0));
    let (se, sk) = s.session_sketch_mut().unwrap();
    se.begin_grip(Grip::Scale, v(6.0, 0.0));
    se.grip_to(v(18.0, 0.0));
    assert!((se.transform.factor - 3.0).abs() < 1e-12);
    se.cancel(sk);
    assert_eq!(se.transform.factor, 1.0);
    assert_eq!(se.tool, Tool::Scale, "nur der Zug");
    assert!(se.grip_drag.is_none());
}

#[test]
fn one_step_named_sketch_scale_and_redo_is_identical() {
    let (mut s, sh) = shapes();
    let mut curves = sh.rect.lines.to_vec();
    curves.push(sh.circle);
    scale_about(&mut s, curves_of(&curves), v(0.0, 0.0));
    type_in(&mut s, F::ScaleFactor, "2");
    let before = snapshot(sketch(&s));
    let undo = undo_len(&s);
    finish(&mut s).unwrap();
    let after = snapshot(sketch(&s));
    assert_eq!(undo_len(&s), undo + 1);
    assert_eq!(s.sketch_undo_label().unwrap(), t("undo-scale"));
    assert!(s.undo_sketch());
    assert_eq!(snapshot(sketch(&s)), before);
    assert!(s.redo_sketch());
    assert_eq!(snapshot(sketch(&s)), after, "bitgleich");
    assert_eq!(undo_len(&s), undo + 1);
}

#[test]
fn undo_clears_a_running_scale() {
    let (mut s, sh) = shapes();
    scale_about(&mut s, curves_of(&sh.rect.lines), v(0.0, 0.0));
    s.observe_sketch(false);
    {
        let (se, sk) = s.session_sketch_mut().unwrap();
        se.selection = curves_of(&sh.rect.lines[..1]);
        se.delete_selection(sk);
    }
    s.observe_sketch(false);
    assert!(s.undo_sketch());
    let se = s.sketch_session.as_ref().unwrap();
    assert_eq!(se.transform.stage, TransformStage::Picking);
    assert!(!se.transform.whole && se.grip_drag.is_none());
}
