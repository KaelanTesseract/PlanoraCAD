//! Skizzen 3, Task 9: Polygon umbeschrieben, einbeschrieben und über eine Kante auf der
//! Stufenmaschine. Ohne Oberfläche über `click`/`commit_input`, eine Fahrt über Rohereignisse bei
//! 1920×1080 (`PlanoraApp::for_tests`) und das Ziehen des ganzen Polygons am Auswahlwerkzeug.

mod common;

use eframe::App as _;
use egui::{Event, Key, Modifiers, PointerButton, Pos2, Rect, Vec2, pos2, vec2};
use planora_app::app::PlanoraApp;
use planora_app::sketch_input::FieldKind as F;
use planora_app::sketch_kinds::ToolKind;
use planora_app::sketch_state::{Item, SketchSession, Tool};
use planora_app::state::{AppState, StandardPlane};
use planora_app::workspace::Workspace;
use planora_app::{fonts, theme};
use planora_feature::{FeatureId, ParamEdit, ParamScope, ParamUnit, SketchFeature, SketchPlane};
use planora_i18n::{DisplayUnit, t, with_unit};
use planora_math::{DVec2, Frame, Tolerance};
use planora_project::config::Config;
use planora_project::library::Library;
use planora_sketch::constraint::{ConstraintId, ConstraintKind as K};
use planora_sketch::model::{CurveId, CurveKind, PointId, Sketch};
use planora_sketch::shapes::ShapePlan;
use planora_sketch::solver::{SolveReport, SolveStatus};

const R: f64 = 0.5;
const SIZE: Vec2 = vec2(1920.0, 1080.0);
const NONE: Modifiers = Modifiers::NONE;
const KINDS: [ToolKind; 3] = [
    ToolKind::PolygonCircumscribed,
    ToolKind::PolygonInscribed,
    ToolKind::PolygonEdge,
];

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

/// Die Klickfolge einer Art: Mitte und Ecke bzw. Seitenmitte, oder Kante und Seite.
fn draw(se: &mut SketchSession, sk: &mut Sketch, kind: ToolKind) {
    match kind {
        ToolKind::PolygonEdge => {
            click(se, sk, 5.0, -3.0);
            click(se, sk, 17.0, -3.0);
            click(se, sk, 11.0, 8.0);
        }
        _ => {
            click(se, sk, 5.0, -3.0);
            click(se, sk, 15.0, -3.0);
        }
    }
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
    sk.constraints().filter(|(_, c)| pick(&c.kind)).count()
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

/// Die Ecken in Umlaufreihenfolge: Anfangspunkte der Seiten.
fn corners(sk: &Sketch) -> Vec<DVec2> {
    lines(sk, false)
        .into_iter()
        .map(|l| xy(sk, ends(sk, l).0))
        .collect()
}

/// Der Konstruktionskreis: Kurve, Mitte, Radius.
fn guide(sk: &Sketch) -> (CurveId, DVec2, f64) {
    let (c, center) = sk
        .curves()
        .find_map(|(c, d)| match d.kind {
            CurveKind::Circle { center, .. } if d.construction => Some((c, center)),
            _ => None,
        })
        .expect("Konstruktionskreis");
    (c, xy(sk, center), sk.radius(c).unwrap())
}

/// Regelmäßig: alle Ecken auf dem Konstruktionskreis, alle Seiten gleich lang.
#[track_caller]
fn assert_regular(sk: &Sketch, sides: usize) {
    let (_, m, r) = guide(sk);
    let cs = corners(sk);
    assert_eq!(cs.len(), sides);
    let side = cs[0].distance(cs[1]);
    for (i, c) in cs.iter().enumerate() {
        assert!((c.distance(m) - r).abs() < 1e-6, "Ecke {i} auf dem Kreis");
        assert!(
            (c.distance(cs[(i + 1) % sides]) - side).abs() < 1e-6,
            "Seite {i} gleich lang"
        );
    }
}

#[track_caller]
fn expect_refused(se: &mut SketchSession, sk: &mut Sketch, raw: DVec2, key: &str) {
    let before = bits(sk);
    let pos = se.pending_pos.clone();
    let text = se.click(sk, raw, R, false).unwrap_err();
    assert_eq!(text, t(key));
    assert_eq!(se.message_text().as_deref(), Some(t(key).as_str()));
    assert_eq!(bits(sk), before, "Skizze bitgleich");
    assert_eq!(se.pending_pos, pos, "Lagen bleiben");
    assert!(se.created.is_none());
}

// ---- Hilfen mit getippten Werten ----------------------------------------------------------------

/// Offene, leere Skizze auf XY; `Breite = 40` als Parameter.
fn rig(kind: ToolKind) -> Workspace {
    let root = common::temp_root("planora-sketch-kinds-polygon");
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

fn preview_sides(w: &mut Workspace, x: f64, y: f64) -> usize {
    let (se, sf, _) = parts(w);
    let outline = se.kind_preview(&sf.sketch, at(x, y)).expect("Vorschau");
    outline[0].len() - 1
}

// ---- Alle drei Arten ------------------------------------------------------------------------------

#[test]
fn every_kind_with_3_6_and_64_sides_keeps_four_freedoms_and_one_face() {
    for kind in KINDS {
        for n in [3, 6, 64] {
            let (mut se, mut sk) = session(kind);
            se.polygon_sides = n;
            draw(&mut se, &mut sk, kind);
            let r = assert_solved(&sk);
            assert_eq!(r.dof, 4, "{kind:?} {n}");
            assert_eq!(r.iterations, 0, "{kind:?} {n}: keine Bewegung");
            assert_regular(&sk, n);
            assert_eq!(lines(&sk, true).len(), 0);
            assert_eq!(sk.curves().filter(|(_, d)| d.construction).count(), 1);
            assert_eq!(sk.profiles().profiles.len(), 1, "{kind:?} {n}: eine Fläche");
            assert_eq!(count(&sk, |k| matches!(k, K::PointOnCurve(..))), n);
            assert_eq!(count(&sk, |k| matches!(k, K::Equal(..))), n - 1);
            assert!(se.pending_pos.is_empty());
            assert!(dims(&sk).is_empty());
        }
    }
}

#[test]
fn circumscribed_puts_the_side_middle_where_the_pointer_was() {
    let (mut se, mut sk) = session(ToolKind::PolygonCircumscribed);
    click(&mut se, &mut sk, 5.0, -3.0);
    click(&mut se, &mut sk, 15.0, -3.0);
    let first = lines(&sk, false)[0];
    let (a, b) = ends(&sk, first);
    assert!(close((xy(&sk, a) + xy(&sk, b)) / 2.0, at(15.0, -3.0)));
    let (_, m, _) = guide(&sk);
    assert!(close(m, at(5.0, -3.0)));
}

#[test]
fn circumscribed_typed_size_is_the_distance_to_the_side_and_no_radius() {
    let mut w = rig(ToolKind::PolygonCircumscribed);
    commit(&mut w, 5.0, -3.0).unwrap();
    type_in(&mut w, F::Radius, "10");
    assert!(
        close(
            resolved(&mut w, 15.0, 7.0),
            at(5.0, -3.0) + at(1.0, 1.0).normalize() * 10.0
        ),
        "Zeiger gibt die Richtung"
    );
    commit(&mut w, 15.0, -3.0).unwrap();
    let ds = dims(sk(&w));
    assert_eq!(ds.len(), 1);
    assert!(matches!(ds[0].1, K::DistancePointLine(_, _, d) if (d - 10.0).abs() < 1e-9));
    assert_eq!(count(sk(&w), |k| matches!(k, K::Radius(..))), 0);
    assert_solved(sk(&w));
    assert_regular(sk(&w), 6);
    let first = lines(sk(&w), false)[0];
    let (a, b) = ends(sk(&w), first);
    assert!(close((xy(sk(&w), a) + xy(sk(&w), b)) / 2.0, at(15.0, -3.0)));
}

#[test]
fn inscribed_typed_radius_is_the_radius_of_the_circle_and_the_first_corner_follows_the_pointer() {
    let mut w = rig(ToolKind::PolygonInscribed);
    commit(&mut w, 0.0, 0.0).unwrap();
    type_in(&mut w, F::Radius, "20");
    commit(&mut w, 3.0, 4.0).unwrap();
    let (circle, _, r) = guide(sk(&w));
    assert!((r - 20.0).abs() < 1e-9);
    let ds = dims(sk(&w));
    assert_eq!(ds.len(), 1);
    assert!(matches!(ds[0].1, K::Radius(c, v) if c == circle && (v - 20.0).abs() < 1e-9));
    let first = corners(sk(&w))[0];
    assert!(
        close(first, at(12.0, 16.0)),
        "auf dem Strahl zum Zeiger: {first:?}"
    );
    assert_solved(sk(&w));
    assert_regular(sk(&w), 6);
}

#[test]
fn edge_typed_length_is_the_first_side_and_stage_two_chooses_the_side() {
    for (pointer, below) in [((6.0, -5.0), true), ((6.0, 5.0), false)] {
        let mut w = rig(ToolKind::PolygonEdge);
        commit(&mut w, 0.0, 0.0).unwrap();
        type_in(&mut w, F::Length, "12");
        commit(&mut w, 40.0, 1.0).unwrap();
        // Die Länge wartet bis zum Abschluss.
        assert_eq!(parts(&mut w).0.pending_pos.len(), 2);
        assert!(dims(sk(&w)).is_empty());
        commit(&mut w, pointer.0, pointer.1).unwrap();
        let ds = dims(sk(&w));
        assert_eq!(ds.len(), 1);
        assert!(matches!(ds[0].1, K::Distance(_, _, d) if (d - 12.0).abs() < 1e-9));
        let (_, m, _) = guide(sk(&w));
        assert_eq!(m.y < 0.0, below, "Mitte auf der Seite des Zeigers");
        let cs = corners(sk(&w));
        assert!(cs[0].distance(at(0.0, 0.0)) < 1e-9);
        assert!((cs[0].distance(cs[1]) - 12.0).abs() < 1e-9);
        assert_solved(sk(&w));
        assert_regular(sk(&w), 6);
    }
}

#[test]
fn a_dimension_changed_in_the_app_keeps_the_shape() {
    for kind in KINDS {
        let mut w = rig(kind);
        commit(&mut w, 5.0, -3.0).unwrap();
        match kind {
            ToolKind::PolygonEdge => {
                type_in(&mut w, F::Length, "12");
                commit(&mut w, 17.0, -3.0).unwrap();
                commit(&mut w, 11.0, 8.0).unwrap();
            }
            _ => {
                type_in(&mut w, F::Radius, "10");
                commit(&mut w, 15.0, -3.0).unwrap();
            }
        }
        let id = dims(sk(&w))[0].0;
        for value in ["40", "4"] {
            let (se, sf, scope) = parts(&mut w);
            se.set_dimension_input(sf, id, value, &scope).unwrap();
            assert_solved(&sf.sketch);
            assert_regular(&sf.sketch, 6);
        }
    }
}

// ---- Seitenzahl -----------------------------------------------------------------------------------

#[test]
fn sides_default_to_six_tab_switches_and_eight_is_remembered() {
    let mut w = rig(ToolKind::PolygonInscribed);
    commit(&mut w, 0.0, 0.0).unwrap();
    {
        let (se, sf, _) = parts(&mut w);
        se.sync_input(&sf.sketch);
        let fields: Vec<F> = se.input.fields.iter().map(|f| f.kind).collect();
        assert_eq!(fields, [F::Radius, F::Sides], "Größe vor Seitenzahl");
        assert_eq!(se.input.active, 0);
        se.next_field(false);
        assert_eq!(se.input.active, 1);
        se.next_field(false);
        assert_eq!(se.input.active, 0);
        assert_eq!(se.current_sides(), 6);
        assert_eq!(se.field_value(&sf.sketch, F::Sides), Some(6.0));
    }
    assert_eq!(preview_sides(&mut w, 10.0, 0.0), 6);
    type_in(&mut w, F::Sides, "8");
    assert_eq!(preview_sides(&mut w, 10.0, 0.0), 8);
    commit(&mut w, 10.0, 0.0).unwrap();
    assert_eq!(corners(sk(&w)).len(), 8);
    assert_eq!(parts(&mut w).0.polygon_sides, 8);
    // Das nächste Polygon beginnt wieder mit 8.
    commit(&mut w, 40.0, 40.0).unwrap();
    assert_eq!(preview_sides(&mut w, 50.0, 40.0), 8);
    commit(&mut w, 50.0, 40.0).unwrap();
    assert_eq!(corners(sk(&w)).len(), 16);
    // 8 Seiten sind kein Maß und keine Formel.
    assert!(dims(sk(&w)).is_empty());
    assert!(draft(&mut w).formulas.is_empty());
}

#[test]
fn an_aborted_polygon_does_not_count_for_the_next_one() {
    let mut w = rig(ToolKind::PolygonInscribed);
    commit(&mut w, 0.0, 0.0).unwrap();
    type_in(&mut w, F::Sides, "5");
    assert_eq!(parts(&mut w).0.current_sides(), 5);
    {
        let (se, sf, _) = parts(&mut w);
        se.set_tool(&mut sf.sketch, Tool::Select);
        se.choose_kind(&mut sf.sketch, ToolKind::PolygonInscribed);
    }
    commit(&mut w, 0.0, 0.0).unwrap();
    assert_eq!(parts(&mut w).0.current_sides(), 6);
    assert_eq!(sk(&w).curves().count(), 0, "nichts entstanden");
}

#[test]
fn a_bad_side_count_names_the_range_and_refuses_the_last_click() {
    for text in ["2", "65", "6,5", "abc"] {
        let mut w = rig(ToolKind::PolygonInscribed);
        commit(&mut w, 0.0, 0.0).unwrap();
        type_in(&mut w, F::Sides, text);
        let before = bits(sk(&w));
        let err = commit(&mut w, 10.0, 0.0).unwrap_err();
        assert_eq!(err, t("sketch-field-sides-range"), "{text}");
        assert_eq!(bits(sk(&w)), before, "{text}");
        assert_eq!(parts(&mut w).0.pending_pos.len(), 1, "{text}");
        // Ein gültiger Wert hebt die Sperre auf.
        type_in(&mut w, F::Sides, "7");
        commit(&mut w, 10.0, 0.0).unwrap();
        assert_eq!(corners(sk(&w)).len(), 7);
    }
}

#[test]
fn edge_takes_the_side_count_of_stage_one_into_stage_two() {
    let mut w = rig(ToolKind::PolygonEdge);
    commit(&mut w, 0.0, 0.0).unwrap();
    type_in(&mut w, F::Sides, "5");
    commit(&mut w, 10.0, 0.0).unwrap();
    {
        let (se, sf, _) = parts(&mut w);
        se.sync_input(&sf.sketch);
        let fields: Vec<F> = se.input.fields.iter().map(|f| f.kind).collect();
        assert_eq!(fields, [F::Sides]);
        assert_eq!(se.field_value(&sf.sketch, F::Sides), Some(5.0));
    }
    assert_eq!(preview_sides(&mut w, 4.0, 6.0), 5);
    commit(&mut w, 4.0, 6.0).unwrap();
    assert_eq!(corners(sk(&w)).len(), 5);
    assert_regular(sk(&w), 5);
    assert!(dims(sk(&w)).is_empty());
}

#[test]
fn inches_keep_six_sides_and_one_inch_is_25_4_mm_and_sides_make_no_dimension() {
    with_unit(DisplayUnit::Zoll, || {
        let mut w = rig(ToolKind::PolygonInscribed);
        commit(&mut w, 0.0, 0.0).unwrap();
        type_in(&mut w, F::Radius, "1");
        type_in(&mut w, F::Sides, "6");
        assert!(close(resolved(&mut w, 40.0, 0.0), at(25.4, 0.0)));
        commit(&mut w, 40.0, 0.0).unwrap();
        assert_eq!(corners(sk(&w)).len(), 6);
        let ds = dims(sk(&w));
        assert_eq!(ds.len(), 1, "nur das Maß der Größe");
        assert!(matches!(ds[0].1, K::Radius(_, d) if d == 25.4));
        assert_eq!(draft(&mut w).formulas.len(), 1);
        assert_eq!(
            draft(&mut w).formulas.get(&ds[0].0).map(String::as_str),
            Some("1 in")
        );
        assert_eq!(parts(&mut w).0.polygon_sides, 6);

        let mut w = rig(ToolKind::PolygonCircumscribed);
        commit(&mut w, 0.0, 0.0).unwrap();
        type_in(&mut w, F::Radius, "Breite / 2");
        commit(&mut w, 40.0, 0.0).unwrap();
        let ds = dims(sk(&w));
        assert!(matches!(ds[0].1, K::DistancePointLine(_, _, d) if d == 20.0));
        assert_eq!(
            draft(&mut w).formulas.get(&ds[0].0).map(String::as_str),
            Some("Breite / 2")
        );
    });
}

// ---- Review-Focus 1: Unmögliches ------------------------------------------------------------------

#[test]
fn pointer_on_the_center_or_on_the_first_side_says_so_and_creates_nothing() {
    for kind in [ToolKind::PolygonCircumscribed, ToolKind::PolygonInscribed] {
        let (mut se, mut sk) = session(kind);
        click(&mut se, &mut sk, 5.0, 5.0);
        assert!(se.kind_preview(&sk, at(5.0, 5.0)).is_none(), "{kind:?}");
        expect_refused(&mut se, &mut sk, at(5.0, 5.0), "sketch-radius-positive");
        assert_eq!(sk.curves().count(), 0);
        // Die Form läuft weiter.
        click(&mut se, &mut sk, 9.0, 5.0);
        assert_eq!(corners(&sk).len(), 6);
    }

    let (mut se, mut sk) = session(ToolKind::PolygonEdge);
    click(&mut se, &mut sk, 1.0, 1.0);
    expect_refused(&mut se, &mut sk, at(1.0, 1.0), "sketch-err-line-zero");
    click(&mut se, &mut sk, 9.0, 1.0);
    assert!(
        se.kind_preview(&sk, at(4.0, 1.0)).is_none(),
        "Seite auf der Kante"
    );
    expect_refused(&mut se, &mut sk, at(4.0, 1.0), "sketch-polygon-edge-side");
    assert_eq!(se.kind_stage(), 2);
    assert_eq!(sk.curves().count(), 0);
    click(&mut se, &mut sk, 4.0, 6.0);
    assert_eq!(corners(&sk).len(), 6);
}

// ---- Review-Focus 2: Fang -------------------------------------------------------------------------

#[test]
fn center_on_the_origin_is_fixed_and_a_radius_change_stays_regular() {
    let (mut se, mut sk) = session(ToolKind::PolygonInscribed);
    click(&mut se, &mut sk, 0.1, 0.1);
    click(&mut se, &mut sk, 8.0, 3.0);
    assert_eq!(
        count(
            &sk,
            |k| matches!(k, K::Fixed(_, x, y) if *x == 0.0 && *y == 0.0)
        ),
        1
    );
    assert_eq!(assert_solved(&sk).dof, 2);
    let (circle, m, _) = guide(&sk);
    assert!(close(m, at(0.0, 0.0)));
    sk.add_constraint(K::Radius(circle, 15.0)).unwrap();
    sk.solve();
    assert_eq!(assert_solved(&sk).dof, 1);
    assert_regular(&sk, 6);
    assert!((guide(&sk).2 - 15.0).abs() < 1e-6);
}

#[test]
fn center_on_a_line_middle_binds_and_a_point_at_the_side_middle_becomes_a_midpoint() {
    let (mut se, mut sk) = session(ToolKind::PolygonInscribed);
    let a = sk.add_point(0.0, 20.0).unwrap();
    let b = sk.add_point(20.0, 20.0).unwrap();
    let wall = sk.add_line(a, b).unwrap();
    click(&mut se, &mut sk, 10.05, 20.05);
    click(&mut se, &mut sk, 14.0, 24.0);
    assert!(
        kinds(&sk)
            .iter()
            .any(|k| matches!(k, K::Midpoint(_, l) if *l == wall))
    );
    assert_solved(&sk);

    let (mut se, mut sk) = session(ToolKind::PolygonCircumscribed);
    let q = sk.add_point(15.0, -3.0).unwrap();
    click(&mut se, &mut sk, 5.0, -3.0);
    click(&mut se, &mut sk, 15.05, -2.95);
    let first = lines(&sk, false)[0];
    assert!(kinds(&sk).contains(&K::Midpoint(q, first)));
    assert!(close(xy(&sk, q), at(15.0, -3.0)), "der Punkt bleibt");
    assert_solved(&sk);
    assert_regular(&sk, 6);
}

#[test]
fn edge_corners_on_existing_points_bind() {
    let (mut se, mut sk) = session(ToolKind::PolygonEdge);
    let p = sk.add_point(0.0, 0.0).unwrap();
    let q = sk.add_point(10.0, 0.0).unwrap();
    click(&mut se, &mut sk, 0.1, 0.1);
    click(&mut se, &mut sk, 9.9, -0.1);
    click(&mut se, &mut sk, 5.0, 6.0);
    let all = kinds(&sk);
    assert!(
        all.iter()
            .any(|k| matches!(k, K::Coincident(_, x) if *x == p))
    );
    assert!(
        all.iter()
            .any(|k| matches!(k, K::Coincident(_, x) if *x == q))
    );
    assert_solved(&sk);
    assert_regular(&sk, 6);
}

// ---- Vorschau ---------------------------------------------------------------------------------------

#[test]
fn preview_and_result_are_the_same_plan() {
    for kind in KINDS {
        let (mut se, mut sk) = session(kind);
        let last = if kind == ToolKind::PolygonEdge {
            click(&mut se, &mut sk, 5.0, -3.0);
            click(&mut se, &mut sk, 17.0, -3.0);
            at(11.0, 8.0)
        } else {
            click(&mut se, &mut sk, 5.0, -3.0);
            at(15.0, -3.0)
        };
        let Some(ShapePlan::Polygon(plan)) = se.kind_plan(&sk, last) else {
            panic!("Plan fehlt: {kind:?}");
        };
        se.click(&mut sk, last, R, false).unwrap();
        let cs = corners(&sk);
        assert_eq!(cs.len(), plan.corners.len());
        for (a, b) in cs.iter().zip(&plan.corners) {
            assert!(close(*a, *b), "{kind:?}");
        }
    }
    // Vor der Seite zeigt die Kantenart die Linie zum Zeiger.
    let (mut se, mut sk) = session(ToolKind::PolygonEdge);
    click(&mut se, &mut sk, 5.0, -3.0);
    let line = se.kind_preview(&sk, at(9.0, -3.0)).unwrap();
    assert_eq!(line, vec![vec![at(5.0, -3.0), at(9.0, -3.0)]]);
}

// ---- Ganzes Polygon ziehen ---------------------------------------------------------------------------

#[test]
fn dragging_the_whole_polygon_keeps_it_regular_and_is_one_step() {
    for kind in KINDS {
        let mut s = AppState::new();
        let i = s
            .add_sketch(SketchPlane::Standard(StandardPlane::XY))
            .unwrap();
        s.open_sketch(i).unwrap();
        {
            let (se, sk) = s.session_sketch_mut().unwrap();
            se.choose_kind(sk, kind);
            se.polygon_sides = 7;
            draw(se, sk, kind);
            se.set_tool(sk, Tool::Select);
        }
        s.observe_sketch(false);
        let steps = s.sketch_history().unwrap().undo_len();
        let sides = 7;
        let r0 = guide(s.session_sketch().unwrap()).2;
        let (first, grab) = {
            let sk = s.session_sketch().unwrap();
            let first = lines(sk, false)[0];
            let (a, b) = ends(sk, first);
            (first, xy(sk, a) * 0.7 + xy(sk, b) * 0.3)
        };
        {
            let (se, sk) = s.session_sketch_mut().unwrap();
            // Doppelklick wählt die ganze Kette.
            se.select_chain(sk, first, false);
            assert_eq!(se.selection.len(), sides, "{kind:?}");
            assert!(se.selection.iter().all(|i| matches!(i, Item::Curve(_))));
            assert!(se.begin_drag(sk, grab, R));
        }
        let before = corners(s.session_sketch().unwrap());
        for step in 1..=4 {
            let (se, sk) = s.session_sketch_mut().unwrap();
            se.drag_to(sk, grab + at(2.0 * step as f64, 1.5 * step as f64));
            s.observe_sketch(true);
        }
        {
            let (se, sk) = s.session_sketch_mut().unwrap();
            se.end_drag(sk);
        }
        s.observe_sketch(false);
        let sk = s.session_sketch().unwrap();
        assert_regular(sk, sides);
        assert!((guide(sk).2 - r0).abs() < 1e-6, "Radius bleibt");
        assert_ne!(corners(sk), before, "es bewegte sich");
        assert_eq!(report(sk).status, SolveStatus::Solved);
        assert_eq!(
            s.sketch_history().unwrap().undo_len(),
            steps + 1,
            "{kind:?}"
        );
    }
}

// ---- Fahrt über Rohereignisse bei 1920×1080, Review-Focus 3 ---------------------------------------

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
        let root = common::temp_root("planora-sketch-kinds-polygon-gui");
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

    fn undo_label(&mut self) -> Option<String> {
        self.app.ws_mut().active_tab_mut().undo_label()
    }

    fn undo(&mut self) {
        self.app.ws_mut().active_tab_mut().undo().unwrap();
        self.settle();
    }

    fn redo(&mut self) {
        self.app.ws_mut().active_tab_mut().redo().unwrap();
        self.settle();
    }
}

fn state_of(sk: &Sketch) -> (Vec<u64>, usize, usize) {
    (
        sk.variables().iter().map(|v| v.to_bits()).collect(),
        sk.curves().count(),
        sk.constraints().count(),
    )
}

#[test]
fn a_64_gon_with_fields_by_raw_events_is_one_undo_step_with_its_dimension() {
    let mut gui = Gui::new();
    gui.choose(ToolKind::PolygonInscribed);
    let (before, steps) = (state_of(&gui.sketch()), gui.steps());
    gui.click(800.0, 500.0);
    gui.frame(vec![Event::PointerMoved(pos2(1000.0, 400.0))]);
    gui.settle();
    gui.type_text("30");
    gui.key(Key::Tab);
    gui.type_text("64");
    gui.key(Key::Enter);
    gui.settle();
    let done = gui.sketch();
    assert_eq!(corners(&done).len(), 64);
    assert_eq!(dims(&done).len(), 1);
    assert_solved(&done);
    assert_eq!(gui.steps(), steps + 1, "genau ein Schritt");
    assert_eq!(gui.undo_label().as_deref(), Some("Polygon zeichnen"));
    gui.undo();
    assert_eq!(state_of(&gui.sketch()), before, "alles weg");
    gui.redo();
    assert_eq!(state_of(&gui.sketch()), state_of(&done), "bitgleich zurück");
}

#[test]
fn each_kind_by_raw_events_is_one_undo_step_and_escape_leaves_nothing() {
    for kind in KINDS {
        let mut gui = Gui::new();
        gui.choose(kind);
        let (before, steps) = (state_of(&gui.sketch()), gui.steps());
        gui.click(800.0, 500.0);
        gui.click(1000.0, 500.0);
        if kind == ToolKind::PolygonEdge {
            gui.key(Key::Escape);
            assert_eq!(
                state_of(&gui.sketch()),
                before,
                "{kind:?}: Esc lässt nichts"
            );
            assert_eq!(gui.steps(), steps);
            gui.click(800.0, 500.0);
            gui.click(1000.0, 500.0);
            gui.click(900.0, 700.0);
        }
        gui.settle();
        let done = gui.sketch();
        assert_eq!(corners(&done).len(), 6, "{kind:?}");
        assert_eq!(gui.steps(), steps + 1, "{kind:?}");
        gui.undo();
        assert_eq!(state_of(&gui.sketch()), before, "{kind:?}");
        gui.redo();
        assert_eq!(state_of(&gui.sketch()), state_of(&done), "{kind:?}");
    }
}

#[test]
fn messages_exist_in_both_languages() {
    for lang in [planora_i18n::german(), planora_i18n::english()] {
        planora_i18n::with_lang(&lang, || {
            for key in [
                "sketch-polygon-edge-side",
                "sketch-field-sides-range",
                "sketch-radius-positive",
                "sketch-err-line-zero",
            ] {
                assert!(!t(key).is_empty() && t(key) != key, "{key}");
            }
        });
    }
}
