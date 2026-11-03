//! Skizzen 4, Task 9: Verschieben und Kopieren in der App, über die Sitzung wie die Oberfläche:
//! Felder, Griffe, Bezugspunkt, Abschluss, Rückgängig und feste Geometrie.

use planora_app::sketch_cursor::DrawCursor;
use planora_app::sketch_grips::Grip;
use planora_app::sketch_input::FieldKind as F;
use planora_app::sketch_state::{Item, Snap, Tool, TransformStage};
use planora_app::state::{AppState, StandardPlane};
use planora_feature::{ParamEdit, ParamUnit, SketchPlane};
use planora_i18n::{DisplayUnit, t, with_unit};
use planora_math::DVec2;
use planora_sketch::constraint::{ConstraintId, ConstraintKind as K, LineOrPoints};
use planora_sketch::helpers::RectIds;
use planora_sketch::model::{CurveId, CurveKind, LineStyle, PointId, Sketch};
use planora_sketch::solver::SolveStatus;

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

fn rectangle() -> (AppState, RectIds) {
    let mut ids = None;
    let s = sketch_with(|sk| ids = Some(sk.add_rectangle(v(0.0, 0.0), v(20.0, 10.0)).unwrap()));
    (s, ids.unwrap())
}

fn sketch(s: &AppState) -> &Sketch {
    s.session_sketch().unwrap()
}

fn se(s: &AppState) -> &planora_app::sketch_state::SketchSession {
    s.sketch_session.as_ref().unwrap()
}

fn preselect(s: &mut AppState, items: Vec<Item>) {
    let (se, sk) = s.session_sketch_mut().unwrap();
    se.selection = items;
    se.set_tool(sk, Tool::MoveCopy);
    se.sync_input(sk);
}

fn curves_of(ids: &[CurveId]) -> Vec<Item> {
    ids.iter().map(|&c| Item::Curve(c)).collect()
}

/// Rechteck, alle Seiten vorgewählt, Werkzeug aktiv.
fn active_rectangle() -> (AppState, RectIds) {
    let (mut s, ids) = rectangle();
    preselect(&mut s, curves_of(&ids.lines));
    (s, ids)
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

fn message(s: &AppState) -> Option<String> {
    se(s).message_text()
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

fn pos(sk: &Sketch, p: PointId) -> DVec2 {
    sk.point_xy(p).unwrap()
}

fn line_pts(sk: &Sketch, c: CurveId) -> (DVec2, DVec2) {
    match sk.curve(c).unwrap().kind {
        CurveKind::Line { a, b } => (pos(sk, a), pos(sk, b)),
        _ => panic!("keine Linie"),
    }
}

/// Alle Punkte der Kurvenzüge der Skizze zu den Kurven `ids`, wie die Vorschau sie liefert.
fn polylines(sk: &Sketch, ids: impl IntoIterator<Item = CurveId>) -> Vec<Vec<DVec2>> {
    ids.into_iter()
        .map(|c| {
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

fn close(a: DVec2, b: DVec2) -> bool {
    a.distance(b) < TOL
}

// ---- Ablauf ------------------------------------------------------------------------------------

#[test]
fn a_preselection_goes_active_with_the_base_in_the_middle_of_the_hull() {
    let (s, _) = active_rectangle();
    let t = &se(&s).transform;
    assert_eq!(t.stage, TransformStage::Active);
    assert!(close(t.base.unwrap(), v(10.0, 5.0)));
    assert_eq!(t.set.curves.len(), 4);
    assert_eq!(t.factor, 1.0);
    assert!(se(&s).transform_now().unwrap().is_identity(1e-12));
    let kinds: Vec<F> = se(&s).input.fields.iter().map(|f| f.kind).collect();
    assert_eq!(kinds, [F::MoveX, F::MoveY, F::MoveAngle]);
}

#[test]
fn the_hull_counts_the_curve_not_its_points() {
    // Ein Halbkreis: Die Mitte der Hülle liegt nicht auf der Mitte des Bogens.
    let mut arc = None;
    let mut s = sketch_with(|sk| {
        let m = sk.add_point(0.0, 0.0).unwrap();
        let a = sk.add_point(10.0, 0.0).unwrap();
        let b = sk.add_point(-10.0, 0.0).unwrap();
        arc = Some(sk.add_arc(m, a, b).unwrap());
    });
    preselect(&mut s, vec![Item::Curve(arc.unwrap())]);
    let base = se(&s).transform.base.unwrap();
    assert!(base.distance(v(0.0, 5.0)) < 1e-2, "{base}");
}

#[test]
fn without_a_preselection_it_picks_first() {
    let (mut s, ids) = rectangle();
    {
        let (se, sk) = s.session_sketch_mut().unwrap();
        se.selection.clear();
        se.set_tool(sk, Tool::MoveCopy);
        assert_eq!(se.transform.stage, TransformStage::Picking);
        assert!(se.transform_now().is_none());
        assert!(se.transform_preview(sk).is_none());
        se.click(sk, v(10.0, 0.0), PICK, false).unwrap();
        assert_eq!(se.selection, vec![Item::Curve(ids.lines[0])]);
        // Auf einem Eckpunkt liegt der Punkt vor der Kurve.
        se.click(sk, v(20.0, 10.0), PICK, false).unwrap();
        assert_eq!(se.selection.len(), 2);
        assert!(matches!(se.selection[1], Item::Point(_)));
        se.click(sk, v(20.0, 10.0), PICK, false).unwrap();
        assert_eq!(se.selection.len(), 1, "der Klick schaltet");
        se.finish_tool(sk).unwrap();
        assert_eq!(se.transform.stage, TransformStage::Active);
        assert_eq!(se.transform.set.curves, vec![ids.lines[0]]);
        assert!(se.transform.set.points.is_empty());
    }
}

// ---- Verschieben -------------------------------------------------------------------------------

#[test]
fn fields_move_exactly_there_whatever_the_base() {
    for base in [None, Some(v(-33.0, 71.5))] {
        let (mut s, ids) = active_rectangle();
        if let Some(b) = base {
            s.sketch_session.as_mut().unwrap().transform.base = Some(b);
        }
        type_in(&mut s, F::MoveX, "10");
        type_in(&mut s, F::MoveY, "5");
        let before: Vec<DVec2> = ids.points.iter().map(|&p| pos(sketch(&s), p)).collect();
        let undo = undo_len(&s);
        finish(&mut s).unwrap();
        for (&p, b) in ids.points.iter().zip(&before) {
            assert_eq!(pos(sketch(&s), p), *b + v(10.0, 5.0));
        }
        assert_eq!(se(&s).tool, Tool::Select);
        assert_eq!(se(&s).selection.len(), 4, "die Auswahl bleibt");
        assert_eq!(se(&s).transform.stage, TransformStage::Picking);
        assert_eq!(undo_len(&s), undo + 1);
        assert_eq!(
            se(&s).last_report.as_ref().unwrap().status,
            SolveStatus::Solved
        );
    }
}

#[test]
fn a_field_wins_over_the_grip() {
    let (mut s, _) = active_rectangle();
    {
        let se = s.sketch_session.as_mut().unwrap();
        se.begin_grip(Grip::Free, v(0.0, 0.0));
        se.grip_to(v(7.0, 3.0));
        se.end_grip();
    }
    type_in(&mut s, F::MoveX, "10");
    let now = se(&s).transform_now().unwrap();
    assert_eq!(now.offset, v(10.0, 3.0), "x vom Feld, y vom Griff");
    // Auch ein Griff nach dem Tippen ändert das gesperrte x nicht.
    {
        let se = s.sketch_session.as_mut().unwrap();
        se.begin_grip(Grip::Free, v(0.0, 0.0));
        se.grip_to(v(100.0, 1.0));
        se.end_grip();
    }
    assert_eq!(se(&s).transform_now().unwrap().offset, v(10.0, 4.0));
    // Ist das Feld leer, gilt wieder der Griff.
    type_in(&mut s, F::MoveX, "");
    assert_eq!(se(&s).transform_now().unwrap().offset, v(107.0, 4.0));
}

#[test]
fn a_typed_angle_of_30_degrees_turns_about_the_base() {
    let (mut s, ids) = active_rectangle();
    type_in(&mut s, F::MoveAngle, "30");
    finish(&mut s).unwrap();
    let a = 30f64.to_radians();
    let turned = |p: DVec2| {
        let d = p - v(10.0, 5.0);
        v(10.0, 5.0) + v(d.x * a.cos() - d.y * a.sin(), d.x * a.sin() + d.y * a.cos())
    };
    let sk = sketch(&s);
    assert!(pos(sk, ids.points[0]).distance(turned(v(0.0, 0.0))) < 1e-6);
    assert!(pos(sk, ids.points[2]).distance(turned(v(20.0, 10.0))) < 1e-6);
    let sat = sk
        .constraints()
        .all(|(_, d)| !matches!(d.kind, K::Horizontal(_) | K::Vertical(_)));
    assert!(sat, "Waagerecht und senkrecht fallen beim Drehen weg");
}

#[test]
fn the_base_snaps_to_a_corner_and_turning_about_it_keeps_it_still() {
    let (mut s, ids) = active_rectangle();
    type_in(&mut s, F::MoveAngle, "40");
    {
        let (se, sk) = s.session_sketch_mut().unwrap();
        se.begin_set_base();
        assert_eq!(se.transform.stage, TransformStage::SetBase);
        assert!(
            (se.transform.angle - 40f64.to_radians()).abs() < 1e-12,
            "bleibt"
        );
        se.sync_input(sk);
        assert!(se.input.fields.is_empty());
        assert!(se.transform_now().is_some());
        // Daneben geklickt, der Fang zieht auf die Ecke.
        se.click(sk, v(20.1, 10.1), PICK, false).unwrap();
        assert_eq!(se.transform.stage, TransformStage::Active);
        assert_eq!(se.transform.base, Some(v(20.0, 10.0)));
        assert!((se.transform.angle - 40f64.to_radians()).abs() < 1e-12);
        se.sync_input(sk);
        assert_eq!(se.input.fields.len(), 3);
    }
    let corner = ids.points[2];
    let at = pos(sketch(&s), corner);
    finish(&mut s).unwrap();
    assert!(
        pos(sketch(&s), corner).distance(at) < 1e-9,
        "die Ecke steht"
    );
    assert!(pos(sketch(&s), ids.points[0]).distance(v(0.0, 0.0)) > 1.0);
}

fn resolved(s: &mut AppState, at: DVec2, ctrl: bool, grid: Option<f64>) -> DrawCursor {
    let (se, sk) = s.session_sketch_mut().unwrap();
    se.cursor_opts.suppressed = ctrl;
    se.cursor_opts.grid_step = grid;
    se.resolve_cursor(sk, at, PICK)
}

#[test]
fn ctrl_grid_and_snap_work_only_for_the_base() {
    let (mut s, _) = active_rectangle();
    // Im Aktiv-Zustand fängt nichts: kein Punkt, kein Raster, keine Automatik.
    for at in [v(20.1, 10.1), v(33.3, 44.4)] {
        let cur = resolved(&mut s, at, false, Some(5.0));
        assert_eq!(cur.at, at);
        assert_eq!(cur.snap, Snap::None);
        assert!(cur.autos.is_empty() && cur.snap_at.is_none());
        assert!(cur.guides.x.is_none() && cur.guides.y.is_none());
    }
    s.sketch_session.as_mut().unwrap().begin_set_base();
    // Fang auf die Ecke.
    let cur = resolved(&mut s, v(20.1, 10.1), false, Some(5.0));
    assert_eq!(cur.at, v(20.0, 10.0));
    assert!(matches!(cur.snap, Snap::Point(_)));
    assert!(cur.autos.is_empty());
    // Raster im Freien.
    let cur = resolved(&mut s, v(61.3, 83.9), false, Some(5.0));
    assert_eq!(cur.at, v(60.0, 85.0));
    assert_eq!(cur.snap, Snap::Grid);
    assert!(
        cur.autos.is_empty(),
        "ein Bezugspunkt kennt keine Automatik"
    );
    // Strg schaltet alles aus.
    let cur = resolved(&mut s, v(20.1, 10.1), true, Some(5.0));
    assert_eq!(cur.at, v(20.1, 10.1));
    assert_eq!(cur.snap, Snap::None);
    // Und der Klick nimmt, was der Zeiger zeigt.
    let (se, sk) = s.session_sketch_mut().unwrap();
    se.cursor_opts.suppressed = true;
    se.click(sk, v(20.1, 10.1), PICK, false).unwrap();
    assert_eq!(se.transform.base, Some(v(20.1, 10.1)));
}

#[test]
fn picking_sees_points_and_curves_but_never_snaps() {
    let (mut s, _) = rectangle();
    {
        let (se, sk) = s.session_sketch_mut().unwrap();
        se.selection.clear();
        se.set_tool(sk, Tool::MoveCopy);
    }
    let cur = resolved(&mut s, v(20.1, 10.1), false, Some(5.0));
    assert_eq!(cur.at, v(20.1, 10.1));
    assert!(matches!(cur.snap, Snap::Point(_)), "nur die Hervorhebung");
    assert!(cur.snap_at.is_none());
    let cur = resolved(&mut s, v(61.3, 83.9), false, Some(5.0));
    assert_eq!(cur.at, v(61.3, 83.9));
    assert_eq!(cur.snap, Snap::None);
    assert!(cur.autos.is_empty());
}

// ---- Vorschau = Ergebnis -----------------------------------------------------------------------

fn sample_sketch() -> (AppState, Vec<CurveId>) {
    let mut ids = Vec::new();
    let s = sketch_with(|sk| {
        let r = sk.add_rectangle(v(0.0, 0.0), v(20.0, 10.0)).unwrap();
        ids.extend(r.lines);
        let m = sk.add_point(40.0, 5.0).unwrap();
        ids.push(sk.add_circle(m, 4.0).unwrap());
        let m = sk.add_point(60.0, 5.0).unwrap();
        let a = sk.add_point(70.0, 5.0).unwrap();
        let b = sk.add_point(60.0, 15.0).unwrap();
        ids.push(sk.add_arc(m, a, b).unwrap());
        let m = sk.add_point(90.0, 5.0).unwrap();
        ids.push(sk.add_ellipse(m, v(8.0, 0.0), 3.0).unwrap());
        let a = sk.add_point(100.0, 0.0).unwrap();
        let b = sk.add_point(110.0, 10.0).unwrap();
        let c = sk.add_point(120.0, 0.0).unwrap();
        ids.push(sk.add_spline(&[a, b, c]).unwrap());
    });
    (s, ids)
}

/// Dasselbe mit Maßen und Tangenten: Das Lösen nach dem Abschluss darf nichts mehr bewegen.
fn dimensioned_sketch() -> (AppState, Vec<CurveId>) {
    let mut ids = Vec::new();
    let s = sketch_with(|sk| {
        let r = sk.add_rectangle(v(0.0, 0.0), v(20.0, 10.0)).unwrap();
        ids.extend(r.lines);
        sk.add_constraint(K::Distance(r.points[0], r.points[1], 20.0))
            .unwrap();
        sk.add_constraint(K::Distance(r.points[1], r.points[2], 10.0))
            .unwrap();
        let m = sk.add_point(40.0, 5.0).unwrap();
        let circle = sk.add_circle(m, 4.0).unwrap();
        sk.add_constraint(K::Radius(circle, 4.0)).unwrap();
        let a = sk.add_point(30.0, 9.0).unwrap();
        let b = sk.add_point(50.0, 9.0).unwrap();
        let line = sk.add_line(a, b).unwrap();
        sk.add_constraint(K::Tangent(line, circle)).unwrap();
        ids.extend([circle, line]);
        // Bogen, der tangential aus dem Ende einer Linie läuft.
        let (p, q) = (
            sk.add_point(60.0, 0.0).unwrap(),
            sk.add_point(70.0, 0.0).unwrap(),
        );
        let lead = sk.add_line(p, q).unwrap();
        let (c, e) = (
            sk.add_point(70.0, 5.0).unwrap(),
            sk.add_point(75.0, 5.0).unwrap(),
        );
        let arc = sk.add_arc(c, q, e).unwrap();
        sk.add_constraint(K::Tangent(lead, arc)).unwrap();
        sk.add_constraint(K::Radius(arc, 5.0)).unwrap();
        ids.extend([lead, arc]);
    });
    (s, ids)
}

/// Dichte Folge von Griffzügen: In jedem Stand ist die Vorschau das Ergebnis.
#[test]
fn the_preview_is_the_result_over_a_dense_sequence() {
    for copy in [false, true] {
        for k in 0..60 {
            let t = f64::from(k) * 0.37 - 11.0;
            let (mut s, ids) = if k % 2 == 0 {
                sample_sketch()
            } else {
                dimensioned_sketch()
            };
            preselect(&mut s, curves_of(&ids));
            let original = sketch(&s).clone();
            let lines = {
                let se = s.sketch_session.as_mut().unwrap();
                se.transform.copy = copy;
                se.transform.offset = v(t * 3.0, -t);
                se.transform.angle = t * 0.4;
                se.transform_preview(&original).expect("Vorschau")
            };
            finish(&mut s).unwrap();
            let sk = sketch(&s);
            let selected: Vec<CurveId> = se(&s)
                .selection
                .iter()
                .filter_map(|i| match *i {
                    Item::Curve(c) => Some(c),
                    _ => None,
                })
                .collect();
            if copy {
                assert_eq!(snapshot_head(sk, &original), bits(&original));
            } else {
                assert_eq!(selected, ids);
            }
            assert_eq!(selected.len(), ids.len());
            assert_same_lines(&lines, &polylines(sk, selected));
        }
    }
}

/// Fast keine Bewegung: Eine Bindung nach außen gilt bis zur zehnfachen Toleranz als erfüllt und das
/// Lösen zöge den Rest zurück. Dichte Folge von Winkeln und Wegen: Die Vorschau bleibt das Ergebnis,
/// oder beide lehnen ab.
#[test]
fn a_tiny_turn_or_shift_never_separates_preview_and_result() {
    let build = || {
        let mut line = None;
        let s = sketch_with(|sk| {
            let a = sk.add_point(-200.0, 0.0).unwrap();
            let b = sk.add_point(200.0, 0.0).unwrap();
            let edge = sk.add_line(a, b).unwrap();
            sk.add_constraint(K::Fixed(a, -200.0, 0.0)).unwrap();
            sk.add_constraint(K::Fixed(b, 200.0, 0.0)).unwrap();
            let p = sk.add_point(0.0, 0.0).unwrap();
            let q = sk.add_point(26.0, 13.0).unwrap();
            line = Some(sk.add_line(p, q).unwrap());
            sk.add_constraint(K::PointOnCurve(p, edge)).unwrap();
        });
        (s, line.unwrap())
    };
    // Der Fall aus der Prüfung genau, dann die Folge.
    let mut cases = vec![(1e-7, v(50.0, 0.0))];
    for k in 0..=80 {
        let e = 10f64.powf(-10.0 + f64::from(k) * 0.075);
        cases.push((e, v(50.0, 0.0)));
        cases.push((-e, v(50.0, 0.0)));
        cases.push((0.0, v(e, 0.0)));
        cases.push((0.0, v(0.0, e)));
        cases.push((0.0, v(50.0, 0.0)));
    }
    for (angle, offset) in cases {
        let (mut s, line) = build();
        preselect(&mut s, vec![Item::Curve(line)]);
        let before = snapshot(sketch(&s));
        let lines = {
            let (se, sk) = s.session_sketch_mut().unwrap();
            se.transform.angle = angle;
            se.transform.offset = offset;
            se.transform_preview(sk)
        };
        let done = finish(&mut s);
        match lines {
            Some(lines) => {
                done.unwrap();
                let result = polylines(sketch(&s), [line]);
                assert_same_lines(&lines, &result);
            }
            None => {
                assert!(done.is_err(), "{angle} {offset}");
                assert_eq!(snapshot(sketch(&s)), before);
            }
        }
    }
}

/// Die Variablen des Originals in der Skizze mit den Kopien: Die Kopien hängen hinten an.
fn snapshot_head(sk: &Sketch, original: &Sketch) -> Vec<u64> {
    bits(sk)[..original.variables().len()].to_vec()
}

#[test]
fn the_preview_draws_pulled_in_helpers_by_their_style() {
    // Zwei Linien mit einer Hilfslinie, die nur an ihnen hängt.
    let mut lines = Vec::new();
    let mut s = sketch_with(|sk| {
        let a = sk.add_point(0.0, 0.0).unwrap();
        let b = sk.add_point(10.0, 0.0).unwrap();
        let c = sk.add_point(10.0, 10.0).unwrap();
        lines.push(sk.add_line(a, b).unwrap());
        lines.push(sk.add_line(b, c).unwrap());
        let helper = sk.add_line(a, c).unwrap();
        sk.set_construction(helper, true).unwrap();
    });
    preselect(&mut s, curves_of(&lines));
    s.sketch_session.as_mut().unwrap().transform.offset = v(5.0, 5.0);
    let styled = se(&s).transform_preview_styled(sketch(&s)).unwrap();
    assert_eq!(styled.len(), 3, "die Hilfslinie zieht mit");
    let dashed = styled
        .iter()
        .filter(|(style, _)| *style == LineStyle::Construction)
        .count();
    assert_eq!(dashed, 1);
    let plain = se(&s).transform_preview(sketch(&s)).unwrap();
    let from_styled: Vec<Vec<DVec2>> = styled.into_iter().map(|(_, l)| l).collect();
    assert_eq!(plain, from_styled);
}

// ---- Kopieren ----------------------------------------------------------------------------------

fn dim_rectangle() -> (AppState, RectIds, ConstraintId) {
    let (mut s, ids) = rectangle();
    let dim;
    {
        let (a, b) = (ids.points[0], ids.points[1]);
        let (se, sf) = s.session_draft_mut().unwrap();
        let _ = se;
        dim = sf.sketch.add_constraint(K::Distance(a, b, 20.0)).unwrap();
        sf.sketch
            .add_constraint(K::Horizontal(LineOrPoints::Line(ids.lines[0])))
            .ok();
        assert_eq!(sf.sketch.solve().status, SolveStatus::Solved);
    }
    s.observe_sketch(false);
    (s, ids, dim)
}

#[test]
fn a_copy_leaves_the_original_and_carries_dimensions_and_formulas() {
    let (mut s, ids, dim) = dim_rectangle();
    // Parameter ändert man nur bei geschlossener Skizze.
    let id = s.open_sketch_id().unwrap();
    s.close_sketch();
    let breite = s.add_parameter().unwrap();
    s.edit_parameter(
        breite,
        ParamEdit {
            name: "Breite".into(),
            formula: "20".into(),
            unit: ParamUnit::Length,
            comment: String::new(),
        },
    )
    .unwrap();
    s.open_sketch(id).unwrap();
    let scope = s.document.param_scope();
    {
        let (se, sf) = s.session_draft_mut().unwrap();
        se.set_dimension_input(sf, dim, "Breite", &scope).unwrap();
    }
    s.observe_sketch(false);
    preselect(&mut s, curves_of(&ids.lines));
    let before = sketch(&s).clone();
    s.sketch_session.as_mut().unwrap().transform.copy = true;
    type_in(&mut s, F::MoveX, "50");
    let undo = undo_len(&s);
    finish(&mut s).unwrap();
    let sk = sketch(&s);
    assert_eq!(
        snapshot_head(sk, &before),
        bits(&before),
        "Original bitgleich"
    );
    assert_eq!(sk.curves().count(), 8);
    assert_eq!(undo_len(&s), undo + 1);

    let draft = s.sketch_draft.as_ref().unwrap();
    assert_eq!(draft.formulas.len(), 2, "die Formel geht mit");
    let new_dim = *draft.formulas.keys().find(|&&id| id != dim).unwrap();
    assert_eq!(draft.formulas[&dim], draft.formulas[&new_dim]);
    assert!(matches!(
        sk.constraint(new_dim).unwrap().kind,
        K::Distance(_, _, x) if x == 20.0
    ));
    // Auswahl: die Kopien.
    let selection = &se(&s).selection;
    assert_eq!(selection.len(), 4);
    for item in selection {
        assert!(matches!(item, Item::Curve(c) if !ids.lines.contains(c)));
    }
    // Der Parameter treibt beide.
    let p = s.document.parameter(breite).unwrap().clone();
    s.close_sketch();
    s.edit_parameter(
        breite,
        ParamEdit {
            name: p.name,
            formula: "30".into(),
            unit: p.unit,
            comment: p.comment,
        },
    )
    .unwrap();
    let sk = s.document.sketch_of(id).unwrap();
    for wanted in [dim, new_dim] {
        match sk.constraint(wanted).unwrap().kind {
            K::Distance(_, _, x) => assert_eq!(x, 30.0, "{wanted:?}"),
            _ => panic!("kein Abstand"),
        }
    }
}

#[test]
fn a_copy_of_fixed_geometry_works_and_is_free() {
    let mut line = None;
    let mut s = sketch_with(|sk| {
        let a = sk.add_point(0.0, 0.0).unwrap();
        let b = sk.add_point(10.0, 0.0).unwrap();
        line = Some(sk.add_line(a, b).unwrap());
        sk.add_constraint(K::Fixed(a, 0.0, 0.0)).unwrap();
    });
    let line = line.unwrap();
    preselect(&mut s, vec![Item::Curve(line)]);
    let before = snapshot(sketch(&s));
    let undo = undo_len(&s);

    // Verschieben geht nicht.
    type_in(&mut s, F::MoveX, "5");
    assert!(se(&s).transform_preview(sketch(&s)).is_none());
    assert!(finish(&mut s).is_err());
    assert_eq!(message(&s).unwrap(), t("sketch-move-fixed"));
    assert_eq!(snapshot(sketch(&s)), before, "bitgleich");
    assert_eq!(undo_len(&s), undo);
    assert_eq!(se(&s).tool, Tool::MoveCopy, "die Stufe bleibt");
    assert_eq!(se(&s).transform.stage, TransformStage::Active);

    // Kopieren geht, und die Kopie ist frei.
    s.sketch_session.as_mut().unwrap().transform.copy = true;
    assert!(se(&s).transform_preview(sketch(&s)).is_some());
    let fixed_before = count_fixed(sketch(&s));
    finish(&mut s).unwrap();
    let sk = sketch(&s);
    assert_eq!(count_fixed(sk), fixed_before, "kein Fest an der Kopie");
    let copy = sk.curves().map(|(c, _)| c).find(|&c| c != line).unwrap();
    assert_eq!(line_pts(sk, copy), (v(5.0, 0.0), v(15.0, 0.0)));
    let report = se(&s).last_report.as_ref().unwrap();
    assert_eq!(report.status, SolveStatus::Solved);
    assert!(report.free_curves.contains(&copy));
}

fn count_fixed(sk: &Sketch) -> usize {
    sk.constraints()
        .filter(|(_, d)| matches!(d.kind, K::Fixed(..)))
        .count()
}

// ---- Nachbarn, Meldungen, Nichts -----------------------------------------------------------------

#[test]
fn a_line_alone_leaves_its_neighbours_and_the_message_names_the_count() {
    let (mut s, ids, _) = dim_rectangle();
    let moved = ids.lines[0];
    // Ein Maß vom Nachbarpunkt zur Linie bindet nach außen; es gilt danach nicht mehr.
    {
        let sk = s.session_sketch_mut().unwrap().1;
        sk.add_constraint(K::DistancePointLine(ids.points[2], moved, 10.0))
            .unwrap();
        assert_eq!(sk.solve().status, SolveStatus::Solved);
    }
    s.observe_sketch(false);
    let others: Vec<(DVec2, DVec2)> = ids.lines[1..]
        .iter()
        .map(|&c| line_pts(sketch(&s), c))
        .collect();
    preselect(&mut s, vec![Item::Curve(moved)]);
    type_in(&mut s, F::MoveY, "-7");
    let was = line_pts(sketch(&s), moved);
    finish(&mut s).unwrap();
    let sk = sketch(&s);
    assert_eq!(
        line_pts(sk, moved),
        (was.0 + v(0.0, -7.0), was.1 + v(0.0, -7.0))
    );
    for (&c, old) in ids.lines[1..].iter().zip(&others) {
        assert_eq!(line_pts(sk, c), *old, "Nachbarn bleiben");
    }
    // Beide Enden der Linie hat ein Nachbar mitbenutzt: Das sagt die Meldung neben dem Maß.
    let text = message(&s).expect("Meldung");
    assert!(
        text.contains(&planora_i18n::tr!("sketch-move-dropped", n = 1)),
        "{text}"
    );
    assert!(
        text.contains(&planora_i18n::tr!("sketch-move-split", n = 2)),
        "{text}"
    );
    assert!(se(&s).selection.contains(&Item::Curve(moved)));
}

#[test]
fn a_split_alone_is_reported_and_a_whole_shape_reports_nothing() {
    let (mut s, ids) = rectangle();
    preselect(&mut s, vec![Item::Curve(ids.lines[0])]);
    type_in(&mut s, F::MoveY, "-7");
    finish(&mut s).unwrap();
    assert_eq!(
        message(&s).unwrap(),
        planora_i18n::tr!("sketch-move-split", n = 2)
    );
    // Das ganze Rechteck trennt nichts und sagt nichts.
    let (mut s, _) = active_rectangle();
    type_in(&mut s, F::MoveY, "-7");
    finish(&mut s).unwrap();
    assert_eq!(message(&s), None);
}

#[test]
fn the_note_of_the_move_keeps_the_warning_of_the_solver() {
    let mut s = AppState::new();
    let i = s
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    s.open_sketch(i).unwrap();
    let moved = {
        let (se, sk) = s.session_sketch_mut().unwrap();
        let r = sk.add_rectangle(v(0.0, 0.0), v(20.0, 10.0)).unwrap();
        // Abseits ein Widerspruch, der schon vorher besteht.
        let (a, b) = (
            sk.add_point(100.0, 0.0).unwrap(),
            sk.add_point(110.0, 0.0).unwrap(),
        );
        sk.add_line(a, b).unwrap();
        sk.add_constraint(K::Distance(a, b, 10.0)).unwrap();
        sk.add_constraint(K::Distance(a, b, 20.0)).unwrap();
        let report = sk.solve();
        assert_eq!(report.status, SolveStatus::Conflicting);
        se.last_report = Some(report);
        r.lines[0]
    };
    s.observe_sketch(false);
    preselect(&mut s, vec![Item::Curve(moved)]);
    type_in(&mut s, F::MoveY, "-7");
    finish(&mut s).unwrap();
    let text = message(&s).unwrap();
    assert!(text.contains(&t("sketch-still-conflicting")), "{text}");
    assert!(
        text.contains(&planora_i18n::tr!("sketch-move-split", n = 2)),
        "{text}"
    );
}

#[test]
fn a_blocked_move_says_so_at_once_and_stops_saying_it_when_it_goes() {
    let mut line = None;
    let mut s = sketch_with(|sk| {
        let a = sk.add_point(0.0, 0.0).unwrap();
        let b = sk.add_point(10.0, 0.0).unwrap();
        line = Some(sk.add_line(a, b).unwrap());
        sk.add_constraint(K::Fixed(a, 0.0, 0.0)).unwrap();
    });
    preselect(&mut s, vec![Item::Curve(line.unwrap())]);
    let update = |s: &mut AppState| {
        let (se, sk) = s.session_sketch_mut().unwrap();
        se.update_transform_block(sk);
    };
    update(&mut s);
    assert_eq!(message(&s), None, "ohne Bewegung noch nichts");
    type_in(&mut s, F::MoveX, "5");
    update(&mut s);
    assert_eq!(message(&s).unwrap(), t("sketch-move-fixed"));
    // Als Kopie geht es, die Meldung verschwindet; zurück kommt sie wieder.
    s.sketch_session.as_mut().unwrap().transform.copy = true;
    update(&mut s);
    assert_eq!(message(&s), None);
    s.sketch_session.as_mut().unwrap().transform.copy = false;
    update(&mut s);
    assert_eq!(message(&s).unwrap(), t("sketch-move-fixed"));
    // Eine andere Meldung bleibt unberührt, wenn die Sperre weg ist nur die eigene.
    type_in(&mut s, F::MoveX, "");
    update(&mut s);
    assert_eq!(message(&s), None);
    s.sketch_session.as_mut().unwrap().message = Some(planora_app::msg::Msg::Raw("x".into()));
    update(&mut s);
    assert_eq!(message(&s).as_deref(), Some("x"));
    // Esc nimmt die Meldung der Sperre mit.
    type_in(&mut s, F::MoveX, "5");
    update(&mut s);
    let (se, sk) = s.session_sketch_mut().unwrap();
    se.cancel(sk);
    assert_eq!(se.message, None);
}

#[test]
fn a_turn_by_half_a_circle_drops_axis_bindings_with_a_neutral_message() {
    let (mut s, _) = active_rectangle();
    type_in(&mut s, F::MoveAngle, "180");
    finish(&mut s).unwrap();
    let text = message(&s).unwrap();
    assert!(
        text.contains(&planora_i18n::tr!("sketch-move-dropped", n = 4)),
        "{text}"
    );
    assert!(!text.to_lowercase().contains("mehr"), "{text}");
}

#[test]
fn nothing_moved_says_so_and_stays() {
    let (mut s, _) = active_rectangle();
    let before = snapshot(sketch(&s));
    let undo = undo_len(&s);
    assert!(finish(&mut s).is_err());
    assert_eq!(message(&s).unwrap(), t("sketch-move-nothing"));
    assert_eq!(snapshot(sketch(&s)), before);
    assert_eq!(undo_len(&s), undo);
    assert_eq!(se(&s).tool, Tool::MoveCopy);
    assert_eq!(se(&s).transform.stage, TransformStage::Active);
}

#[test]
fn a_bad_field_refuses_and_coordinates_out_of_range_get_their_own_message() {
    let (mut s, _) = active_rectangle();
    let before = snapshot(sketch(&s));
    type_in(&mut s, F::MoveX, "kein Maß +");
    assert!(finish(&mut s).is_err());
    assert_eq!(snapshot(sketch(&s)), before);
    type_in(&mut s, F::MoveX, "1e12");
    assert!(se(&s).transform_preview(sketch(&s)).is_none());
    assert!(finish(&mut s).is_err());
    assert_eq!(message(&s).unwrap(), t("sketch-move-degenerate"));
    assert_eq!(snapshot(sketch(&s)), before);
}

#[test]
fn inches_are_read_in_the_display_unit() {
    with_unit(DisplayUnit::Zoll, || {
        let (mut s, ids) = active_rectangle();
        type_in(&mut s, F::MoveX, "1");
        assert_eq!(se(&s).transform_now().unwrap().offset, v(25.4, 0.0));
        let was = pos(sketch(&s), ids.points[0]);
        finish(&mut s).unwrap();
        assert_eq!(pos(sketch(&s), ids.points[0]), was + v(25.4, 0.0));
    });
}

#[test]
fn the_field_shows_the_grip_value_until_something_is_typed() {
    let (mut s, _) = active_rectangle();
    {
        let (se, sk) = s.session_sketch_mut().unwrap();
        se.begin_grip(Grip::Free, v(1.0, 1.0));
        se.grip_to(v(4.0, 6.0));
        se.end_grip();
        se.transform.angle = 0.5;
        assert_eq!(se.field_value(sk, F::MoveX), Some(3.0));
        assert_eq!(se.field_value(sk, F::MoveY), Some(5.0));
        assert_eq!(se.field_value(sk, F::MoveAngle), Some(0.5));
    }
    type_in(&mut s, F::MoveX, "12");
    let sk = sketch(&s);
    assert_eq!(se(&s).field_value(sk, F::MoveX), Some(12.0));
}

/// Der Griff an der Sitzung: Der Ring dreht um den versetzten Bezugspunkt, 720° bleiben 720°.
#[test]
fn the_session_counts_full_turns_and_abort_goes_back() {
    let (mut s, _) = active_rectangle();
    let se = s.sketch_session.as_mut().unwrap();
    se.transform.offset = v(3.0, 4.0);
    let pivot = v(13.0, 9.0);
    let at = |t: f64| pivot + DVec2::from_angle(t) * 6.0;
    se.begin_grip(Grip::Rotate, at(0.0));
    let n = 720;
    for k in 1..=n {
        se.grip_to(at(f64::from(k) / f64::from(n)
            * 2.0
            * std::f64::consts::TAU));
    }
    let turned = se.transform.angle;
    assert!(
        (turned - 2.0 * std::f64::consts::TAU).abs() < 1e-9,
        "{turned}"
    );
    assert_eq!(se.transform.offset, v(3.0, 4.0));
    se.abort_grip();
    assert_eq!(se.transform.angle, 0.0);
    assert!(se.grip_drag.is_none());
}

// ---- Abbruch und Rückgängig ----------------------------------------------------------------------

#[test]
fn esc_in_every_stage_leaves_no_trace() {
    for stage in [
        TransformStage::Picking,
        TransformStage::SetBase,
        TransformStage::Active,
    ] {
        let (mut s, ids) = rectangle();
        if stage == TransformStage::Picking {
            let (se, sk) = s.session_sketch_mut().unwrap();
            se.selection.clear();
            se.set_tool(sk, Tool::MoveCopy);
        } else {
            preselect(&mut s, curves_of(&ids.lines));
            let se = s.sketch_session.as_mut().unwrap();
            se.transform.offset = v(4.0, 4.0);
            if stage == TransformStage::SetBase {
                se.begin_set_base();
            }
        }
        assert_eq!(se(&s).transform.stage, stage);
        let selection = se(&s).transform.selection_before.clone();
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
        assert_eq!(se(&s).transform.offset, DVec2::ZERO);
        assert_eq!(se(&s).selection, selection, "{stage:?}");
    }
}

#[test]
fn one_step_per_command_named_move_copy_and_redo_is_identical() {
    for copy in [false, true] {
        let (mut s, ids, _) = dim_rectangle();
        preselect(&mut s, curves_of(&ids.lines));
        let before = snapshot(sketch(&s));
        s.sketch_session.as_mut().unwrap().transform.copy = copy;
        type_in(&mut s, F::MoveX, "10");
        type_in(&mut s, F::MoveAngle, "15");
        let undo = undo_len(&s);
        finish(&mut s).unwrap();
        let after = snapshot(sketch(&s));
        let formulas = s.sketch_draft.as_ref().unwrap().formulas.clone();
        assert_eq!(undo_len(&s), undo + 1);
        assert_eq!(s.sketch_undo_label().unwrap(), t("undo-move-copy"));
        assert!(s.undo_sketch());
        assert_eq!(snapshot(sketch(&s)), before);
        assert!(s.redo_sketch());
        assert_eq!(snapshot(sketch(&s)), after, "bitgleich");
        assert_eq!(s.sketch_draft.as_ref().unwrap().formulas, formulas);
        assert_eq!(undo_len(&s), undo + 1);
    }
}

#[test]
fn undo_clears_a_running_transform_and_a_deleted_curve_leaves_the_set() {
    let (mut s, ids) = active_rectangle();
    {
        let (se, sk) = s.session_sketch_mut().unwrap();
        se.selection = vec![Item::Curve(ids.lines[0])];
        se.delete_selection(sk);
        assert_eq!(se.transform.set.curves.len(), 3);
        assert!(!se.transform.set.curves.contains(&ids.lines[0]));
        se.selection = curves_of(&ids.lines[1..]);
        se.delete_selection(sk);
        assert!(se.transform.set.curves.is_empty());
        assert_eq!(se.transform.stage, TransformStage::Picking);
        assert!(se.transform_now().is_none());
    }
    s.observe_sketch(false);
    assert!(s.undo_sketch());
    let se = s.sketch_session.as_ref().unwrap();
    assert_eq!(se.transform.stage, TransformStage::Picking);
    assert!(se.grip_drag.is_none());
}

// ---- Versatz und feste Geometrie -----------------------------------------------------------------

#[test]
fn moving_a_source_takes_its_offset_copy_and_a_copy_alone_is_cut_free() {
    let build = || {
        let mut line = None;
        let mut s = sketch_with(|sk| {
            let a = sk.add_point(0.0, 0.0).unwrap();
            let b = sk.add_point(20.0, 0.0).unwrap();
            line = Some(sk.add_line(a, b).unwrap());
        });
        let line = line.unwrap();
        let (se, sk) = s.session_sketch_mut().unwrap();
        se.selection = vec![Item::Curve(line)];
        se.set_tool(sk, Tool::Offset);
        se.click(sk, v(10.0, 5.0), PICK, false).unwrap();
        se.set_tool(sk, Tool::Select);
        se.selection.clear();
        s.observe_sketch(false);
        let copy = sketch(&s)
            .curves()
            .find(|&(c, d)| c != line && !d.construction)
            .map(|(c, _)| c)
            .unwrap();
        (s, line, copy)
    };
    // Die Vorlage nimmt die Kopie mit.
    let (mut s, line, copy) = build();
    let (a, b) = line_pts(sketch(&s), copy);
    preselect(&mut s, vec![Item::Curve(line)]);
    type_in(&mut s, F::MoveX, "3");
    finish(&mut s).unwrap();
    assert_eq!(
        line_pts(sketch(&s), copy),
        (a + v(3.0, 0.0), b + v(3.0, 0.0))
    );

    // Die Kopie allein wird frei, und die Meldung sagt es.
    let (mut s, line, copy) = build();
    let source = line_pts(sketch(&s), line);
    preselect(&mut s, vec![Item::Curve(copy)]);
    type_in(&mut s, F::MoveY, "3");
    finish(&mut s).unwrap();
    assert_eq!(line_pts(sketch(&s), line), source, "die Vorlage bleibt");
    let text = message(&s).expect("Meldung");
    assert!(text.chars().any(|c| c.is_ascii_digit()), "{text}");
}

#[test]
fn a_point_bound_to_a_projected_edge_says_so_and_along_the_edge_it_goes() {
    let mut edge = None;
    let mut point = None;
    let mut s = sketch_with(|sk| {
        let a = sk.add_point(0.0, 0.0).unwrap();
        let b = sk.add_point(30.0, 0.0).unwrap();
        let e = sk.add_line(a, b).unwrap();
        sk.add_constraint(K::Fixed(a, 0.0, 0.0)).unwrap();
        sk.add_constraint(K::Fixed(b, 30.0, 0.0)).unwrap();
        let p = sk.add_point(10.0, 0.0).unwrap();
        sk.add_constraint(K::PointOnCurve(p, e)).unwrap();
        edge = Some(e);
        point = Some(p);
    });
    let _ = edge;
    let p = point.unwrap();
    preselect(&mut s, vec![Item::Point(p)]);
    let before = snapshot(sketch(&s));
    type_in(&mut s, F::MoveY, "4");
    assert!(finish(&mut s).is_err());
    assert_eq!(message(&s).unwrap(), t("sketch-move-fixed"));
    assert_eq!(snapshot(sketch(&s)), before);
    // Entlang der Kante.
    type_in(&mut s, F::MoveY, "");
    type_in(&mut s, F::MoveX, "5");
    finish(&mut s).unwrap();
    assert_eq!(pos(sketch(&s), p), v(15.0, 0.0));
    assert_eq!(
        se(&s).last_report.as_ref().unwrap().status,
        SolveStatus::Solved
    );
}

// ---- Messtest ------------------------------------------------------------------------------------

/// Im Release: `cargo test -p planora-app --release --test sketch_move_copy -- --ignored`.
#[test]
#[ignore = "Messtest, nur im Release sinnvoll"]
fn the_preview_of_200_curves_stays_under_16_ms() {
    for copy in [false, true] {
        let mut ids = Vec::new();
        let mut s = sketch_with(|sk| {
            let n = 200;
            let points: Vec<PointId> = (0..n)
                .map(|i| {
                    let a = std::f64::consts::TAU * f64::from(i) / f64::from(n);
                    sk.add_point(100.0 * a.cos(), 100.0 * a.sin()).unwrap()
                })
                .collect();
            for i in 0..n as usize {
                let next = (i + 1) % n as usize;
                let c = sk.add_line(points[i], points[next]).unwrap();
                if i > 0 {
                    sk.add_constraint(K::Equal(ids[0], c)).unwrap();
                }
                ids.push(c);
            }
        });
        preselect(&mut s, curves_of(&ids));
        let se = s.sketch_session.as_mut().unwrap();
        se.transform.copy = copy;
        se.transform.offset = v(5.0, 7.0);
        se.transform.angle = 0.3;
        let sk = s.sketch_draft.as_ref().unwrap().sketch.clone();
        let se = s.sketch_session.as_ref().unwrap();
        let mut best = f64::MAX;
        for _ in 0..10 {
            let t0 = std::time::Instant::now();
            let lines = se.transform_preview(&sk).unwrap();
            let ms = t0.elapsed().as_secs_f64() * 1000.0;
            assert_eq!(lines.len(), 200);
            best = best.min(ms);
        }
        println!("Vorschau 200 Kurven, Kopie {copy}: {best:.2} ms");
        assert!(best < 16.0, "{best} ms");
    }
}
