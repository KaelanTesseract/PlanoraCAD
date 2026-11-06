//! Skizzen 5, Task 11: Mitgenommenes aus früheren Teilprojekten. Das Langloch „Gesamt“ hält die
//! Gesamtlänge als eigenes Maß (`SlotOverall`), ein Doppelklick auf einen Maßtext in der Ansicht
//! öffnet sein Feld wie die Liste, und der Rasterfang bindet an einen Punkt, der genau auf dem
//! Rasterpunkt liegt. Die Fahrten über Rohereignisse laufen bei 1920×1080.

mod common;

use eframe::App as _;
use egui::epaint::{ClippedShape, Shape};
use egui::{Event, Key, Modifiers, PointerButton, Pos2, Rect, Vec2, pos2};
use planora_app::app::PlanoraApp;
use planora_app::sketch_input::FieldKind as F;
use planora_app::sketch_kinds::ToolKind;
use planora_app::sketch_state::{Item, SketchSession, Snap, Tool};
use planora_app::state::{AppState, StandardPlane};
use planora_app::workspace::Workspace;
use planora_app::{fonts, sketch_panel, sketch_view, theme};
use planora_feature::{
    FeatureId, ParamEdit, ParamId, ParamScope, ParamUnit, SketchFeature, SketchPlane,
};
use planora_i18n::{german, t, with_lang};
use planora_math::DVec2;
use planora_project::config::Config;
use planora_project::library::Library;
use planora_sketch::constraint::{ConstraintId, ConstraintKind as K};
use planora_sketch::model::{CurveId, CurveKind, PointId, Sketch};
use planora_sketch::solver::SolveStatus;

const W: f32 = 1920.0;
const H: f32 = 1080.0;
const R: f64 = 0.5;

fn v(x: f64, y: f64) -> DVec2 {
    DVec2::new(x, y)
}

#[track_caller]
fn near(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-6, "{a} statt {b}");
}

fn xy(sk: &Sketch, p: PointId) -> DVec2 {
    sk.point_xy(p).unwrap()
}

fn bits(sk: &Sketch) -> (u64, Vec<u64>) {
    (
        sk.revision(),
        sk.variables().iter().map(|x| x.to_bits()).collect(),
    )
}

#[track_caller]
fn assert_solved(sk: &Sketch) {
    let r = sk.clone().solve();
    assert_eq!(r.status, SolveStatus::Solved);
    assert!(r.redundant.is_empty(), "{:?}", r.redundant);
    assert!(r.conflicting.is_empty(), "{:?}", r.conflicting);
}

fn dims(sk: &Sketch) -> Vec<(ConstraintId, K)> {
    sk.constraints()
        .filter(|(_, c)| c.kind.is_dimension())
        .map(|(id, c)| (id, c.kind.clone()))
        .collect()
}

// ---- Langloch „Gesamt“ ----------------------------------------------------------------------------

fn param(s: &mut AppState, name: &str, formula: &str) -> ParamId {
    let id = s.add_parameter().unwrap();
    s.edit_parameter(
        id,
        ParamEdit {
            name: name.into(),
            formula: formula.into(),
            unit: ParamUnit::Length,
            comment: String::new(),
        },
    )
    .unwrap();
    id
}

fn set_param(s: &mut AppState, id: ParamId, formula: &str) {
    let p = s.document.parameter(id).unwrap().clone();
    s.edit_parameter(
        id,
        ParamEdit {
            name: p.name,
            formula: formula.into(),
            unit: p.unit,
            comment: p.comment,
        },
    )
    .unwrap();
}

/// Offene Skizze mit dem Langloch „Gesamt“ als Werkzeug; vorher die Parameter `params`.
fn slot_rig(params: &[(&str, &str)]) -> (Workspace, Vec<ParamId>) {
    let root = common::temp_root("planora-sketch-carried-s5");
    let mut w = Workspace::new(Some(Library::open(&root).unwrap()), Config::default());
    let s = &mut w.active_tab_mut().state;
    let ids = params.iter().map(|(n, f)| param(s, n, f)).collect();
    let sketch = s
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    s.open_sketch(sketch).unwrap();
    w.observe_history(false);
    let (se, sf, _) = parts(&mut w);
    se.choose_kind(&mut sf.sketch, ToolKind::SlotOverall);
    (w, ids)
}

fn parts(w: &mut Workspace) -> (&mut SketchSession, &mut SketchFeature, ParamScope) {
    let s = &mut w.active_tab_mut().state;
    let scope = s.document.param_scope();
    let (se, sf) = s.session_draft_mut().unwrap();
    (se, sf, scope)
}

fn commit(w: &mut Workspace, p: DVec2) -> Result<(), String> {
    let (se, sf, scope) = parts(w);
    se.commit_input(sf, p, R, false, &scope)
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

fn sk(w: &Workspace) -> &Sketch {
    w.active_tab().state.session_sketch().unwrap()
}

fn set(w: &mut Workspace, id: ConstraintId, text: &str) -> Result<(), String> {
    let (se, sf, scope) = parts(w);
    se.set_dimension_input(sf, id, text, &scope)
}

/// Langloch „Gesamt“ mit getippter Gesamtlänge `length` und Breite `width`, waagerecht ab (5, −3).
fn overall_slot(w: &mut Workspace, length: &str, width: &str) -> Result<(), String> {
    commit(w, v(5.0, -3.0))?;
    type_in(w, F::Length, length);
    commit(w, v(95.0, -3.0))?;
    type_in(w, F::Width, width);
    commit(w, v(30.0, 20.0))
}

/// Mitten, Kappe und Maße des einzigen Langlochs.
fn slot_parts(sk: &Sketch) -> ([PointId; 2], CurveId) {
    let (_, k) = dims(sk)
        .into_iter()
        .find(|(_, k)| matches!(k, K::SlotOverall(..)))
        .expect("Gesamtlänge als Maß");
    let K::SlotOverall(a, b, cap, _) = k else {
        unreachable!()
    };
    ([a, b], cap)
}

/// Außenlänge des Langlochs: Mittenabstand plus Breite.
fn outer_length(sk: &Sketch) -> f64 {
    let ([a, b], cap) = slot_parts(sk);
    xy(sk, a).distance(xy(sk, b)) + 2.0 * sk.radius(cap).unwrap()
}

#[test]
fn overall_slot_l60_w10_holds_the_overall_length_when_the_width_goes_to_20() {
    let (mut w, _) = slot_rig(&[]);
    overall_slot(&mut w, "60", "10").unwrap();
    let ds = dims(sk(&w));
    assert_eq!(ds.len(), 2, "{ds:?}");
    let ([a, b], cap) = slot_parts(sk(&w));
    assert!(
        ds.iter()
            .any(|(_, k)| *k == K::SlotOverall(a, b, cap, 60.0)),
        "{ds:?}"
    );
    let width = ds
        .iter()
        .find(|(_, k)| matches!(k, K::Diameter(c, d) if *c == cap && *d == 10.0))
        .expect("Durchmesser 10")
        .0;
    assert!(
        !ds.iter().any(|(_, k)| matches!(k, K::Distance(..))),
        "kein Mittenabstand mehr: {ds:?}"
    );
    assert_solved(sk(&w));
    near(xy(sk(&w), a).distance(xy(sk(&w), b)), 50.0);
    near(outer_length(sk(&w)), 60.0);
    assert!(sk(&w).needs_format_15(), "SlotOverall braucht Format 15");

    set(&mut w, width, "20").unwrap();
    assert_solved(sk(&w));
    near(xy(sk(&w), a).distance(xy(sk(&w), b)), 40.0);
    near(outer_length(sk(&w)), 60.0);
    near(sk(&w).radius(cap).unwrap(), 10.0);
    assert_eq!(sk(&w).profiles().profiles.len(), 1);
}

#[test]
fn a_formula_on_the_overall_length_follows_its_parameter() {
    let (mut w, ids) = slot_rig(&[("L", "60")]);
    overall_slot(&mut w, "L", "10").unwrap();
    let (id, _) = dims(sk(&w))
        .into_iter()
        .find(|(_, k)| matches!(k, K::SlotOverall(..)))
        .unwrap();
    let s = &mut w.active_tab_mut().state;
    let feature = s.sketch_session.as_ref().unwrap().sketch;
    assert_eq!(
        s.sketch_draft
            .as_ref()
            .unwrap()
            .formulas
            .get(&id)
            .map(String::as_str),
        Some("L")
    );
    s.close_sketch();
    for (formula, want) in [("80", 80.0), ("35", 35.0), ("60", 60.0)] {
        set_param(s, ids[0], formula);
        let sketch = s.document.sketch_of(feature).unwrap();
        let k = &sketch.constraint(id).unwrap().kind;
        assert!(
            matches!(k, K::SlotOverall(_, _, _, x) if (*x - want).abs() < 1e-9),
            "{formula}: {k:?}"
        );
        assert_solved(sketch);
        near(outer_length(sketch), want);
        near(sketch.radius(slot_parts(sketch).1).unwrap(), 5.0);
    }
}

/// Mindestabstand der Breite unter der Gesamtlänge (0,1 % der Gesamtlänge, wie die App).
const MIN_GAP: f64 = 1e-3;

/// Liegt `gap` so dicht an der Grenze `MIN_GAP · l`, dass Rundung entscheidet? Diese Proben
/// fallen aus.
fn at_the_limit(l: f64, gap: f64) -> bool {
    (gap - MIN_GAP * l).abs() < 1e-6
}

/// Ruling Task 1: `|a − b|` ist bei a = b nicht differenzierbar. Dicht an L = W über das Werkzeug:
/// ab 0,1 % Abstand entsteht das Langloch mit genau dieser Gesamtlänge, darunter wird es mit
/// Meldung abgelehnt und nichts bleibt.
#[test]
fn overall_lengths_densely_above_the_width_are_exact_or_refused_with_a_message() {
    let (mut w, _) = slot_rig(&[]);
    let base_sf = parts(&mut w).1.clone();
    let scope = parts(&mut w).2;
    let (mut made, mut refused) = (0, 0);
    for k in 1..=400 {
        let gap = f64::from(k) * 0.000_25;
        let length = 10.0 + gap;
        if at_the_limit(length, gap) {
            continue;
        }
        let mut sf = base_sf.clone();
        let mut se = SketchSession::new(FeatureId(0));
        se.choose_kind(&mut sf.sketch, ToolKind::SlotOverall);
        se.commit_input(&mut sf, v(5.0, -3.0), R, false, &scope)
            .unwrap();
        se.sync_input(&sf.sketch);
        let field =
            |se: &SketchSession, kind| se.input.fields.iter().position(|f| f.kind == kind).unwrap();
        let i = field(&se, F::Length);
        se.type_field(i, &format!("{length}"), &scope);
        se.commit_input(&mut sf, v(95.0, -3.0), R, false, &scope)
            .unwrap();
        se.sync_input(&sf.sketch);
        let i = field(&se, F::Width);
        se.type_field(i, "10", &scope);
        let before = bits(&sf.sketch);
        let result = se.commit_input(&mut sf, v(30.0, 20.0), R, false, &scope);
        if gap >= MIN_GAP * length {
            result.unwrap_or_else(|e| panic!("{length}: {e}"));
            made += 1;
            assert_solved(&sf.sketch);
            let ([a, b], cap) = slot_parts(&sf.sketch);
            let (pa, pb) = (xy(&sf.sketch, a), xy(&sf.sketch, b));
            assert!(
                (pa.distance(pb) + 2.0 * sf.sketch.radius(cap).unwrap() - length).abs() < 1e-6,
                "{length}"
            );
            // Die Mitten bleiben in Zeichenrichtung (kein Vertauschen).
            assert!(pb.x > pa.x, "{length}: {pa:?} {pb:?}");
        } else {
            refused += 1;
            assert_eq!(
                result.unwrap_err(),
                t("sketch-slot-overall-gap"),
                "{length}"
            );
            assert_eq!(bits(&sf.sketch), before, "{length}: nichts bleibt");
        }
    }
    assert!(made > 300 && refused > 30, "{made} {refused}");
}

/// Ein Wert über das Maßfeld (wie die Liste) an einem frischen Stand.
fn set_on(
    sf: &mut SketchFeature,
    id: ConstraintId,
    value: f64,
    scope: &ParamScope,
) -> Result<(), String> {
    let mut se = SketchSession::new(FeatureId(0));
    se.set_dimension_input(sf, id, &format!("{value}"), scope)
}

/// Dicht an W = L über die Maßfelder von Breite und Gesamtlänge, jeweils hin und zurück: ab 0,1 %
/// Abstand genau, darunter abgelehnt mit Meldung, die Lage bleibt dann bitgleich.
#[test]
fn widths_and_lengths_densely_at_the_limit_are_exact_or_refused_and_the_state_stays() {
    for (l0, w0) in [(20.0, 10.0), (60.0, 10.0), (11.0, 1.0)] {
        let (mut w, _) = slot_rig(&[]);
        overall_slot(&mut w, &format!("{l0}"), &format!("{w0}")).unwrap();
        let base = parts(&mut w).1.clone();
        let scope = parts(&mut w).2;
        let ([a, b], cap) = slot_parts(&base.sketch);
        let find = |pick: fn(&K) -> bool| {
            dims(&base.sketch)
                .into_iter()
                .find(|(_, k)| pick(k))
                .unwrap()
                .0
        };
        let width = find(|k| matches!(k, K::Diameter(..)));
        let length = find(|k| matches!(k, K::SlotOverall(..)));
        let exact = |sf: &SketchFeature, l: f64, wd: f64| {
            assert_solved(&sf.sketch);
            near(outer_length(&sf.sketch), l);
            near(2.0 * sf.sketch.radius(cap).unwrap(), wd);
            let (pa, pb) = (xy(&sf.sketch, a), xy(&sf.sketch, b));
            assert!(pb.x > pa.x, "{l} {wd}: vertauscht");
        };
        let (mut ok, mut refused) = (0, 0);
        for k in 1..=2000 {
            let gap = f64::from(k) * 0.000_25;
            if at_the_limit(l0, gap) || at_the_limit(w0 + gap, gap) {
                continue;
            }
            // Breite bis dicht an die Gesamtlänge und zurück.
            let mut sf = base.clone();
            let before = bits(&sf.sketch);
            match set_on(&mut sf, width, l0 - gap, &scope) {
                Ok(()) => {
                    assert!(gap >= MIN_GAP * l0, "{l0} {gap}: unter der Grenze angelegt");
                    exact(&sf, l0, l0 - gap);
                    set_on(&mut sf, width, w0, &scope)
                        .unwrap_or_else(|e| panic!("{l0} {gap} zurück: {e}"));
                    exact(&sf, l0, w0);
                    ok += 1;
                }
                Err(e) => {
                    assert!(gap < MIN_GAP * l0, "{l0} {gap}: {e}");
                    assert_eq!(e, t("sketch-slot-overall-gap"));
                    assert_eq!(bits(&sf.sketch), before);
                    refused += 1;
                }
            }
            // Gesamtlänge bis dicht an die Breite und zurück.
            let mut sf = base.clone();
            let before = bits(&sf.sketch);
            let l = w0 + gap;
            match set_on(&mut sf, length, l, &scope) {
                Ok(()) => {
                    assert!(gap >= MIN_GAP * l, "{l}: unter der Grenze angelegt");
                    exact(&sf, l, w0);
                    set_on(&mut sf, length, l0, &scope)
                        .unwrap_or_else(|e| panic!("{l} zurück: {e}"));
                    exact(&sf, l0, w0);
                    ok += 1;
                }
                Err(e) => {
                    assert!(gap < MIN_GAP * l, "{l}: {e}");
                    assert_eq!(e, t("sketch-slot-overall-gap"));
                    assert_eq!(bits(&sf.sketch), before);
                    refused += 1;
                }
            }
        }
        assert!(ok > 3000 && refused > 0, "{l0}: {ok} {refused}");
    }
}

// ---- Rasterfang bindet --------------------------------------------------------------------------

/// Fangradius für 8 px bei 0,05 mm je Bildpunkt; 8,5 px sind dann 0,425 mm.
const PICK: f64 = 0.4;

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

fn sketch(s: &AppState) -> &Sketch {
    s.session_sketch().unwrap()
}

fn grid(s: &mut AppState, ctrl: bool) {
    let se = s.sketch_session.as_mut().unwrap();
    se.cursor_opts.grid_step = Some(1.0);
    se.cursor_opts.suppressed = ctrl;
}

/// Klick über `commit_input` wie die Oberfläche, danach der Verlauf.
fn click(s: &mut AppState, at: DVec2) -> Result<(), String> {
    let scope = s.document.param_scope();
    let (se, sf) = s.session_draft_mut().unwrap();
    se.update_cursor(&sf.sketch, Some(at), PICK);
    let result = se.commit_input(sf, at, PICK, false, &scope);
    s.observe_sketch(false);
    result
}

fn set_tool(s: &mut AppState, tool: Tool) {
    let (se, sk) = s.session_sketch_mut().unwrap();
    se.set_tool(sk, tool);
}

fn choose(s: &mut AppState, kind: ToolKind) {
    let (se, sk) = s.session_sketch_mut().unwrap();
    se.choose_kind(sk, kind);
}

fn line_ends(sk: &Sketch, c: CurveId) -> (PointId, PointId) {
    let CurveKind::Line { a, b } = sk.curve(c).unwrap().kind else {
        panic!("keine Linie");
    };
    (a, b)
}

fn lines(sk: &Sketch) -> Vec<CurveId> {
    sk.curves()
        .filter(|(_, c)| matches!(c.kind, CurveKind::Line { .. }) && !c.construction)
        .map(|(id, _)| id)
        .collect()
}

/// 8,5 px schräg neben (30, 10): weder Punkt- noch Kurvenfang (8 px) noch Ausrichtung (4,8 px).
fn beside_corner() -> DVec2 {
    v(30.0, 10.0) + DVec2::splat(0.425 / 2.0_f64.sqrt())
}

/// Linie (10, 10)–(30, 10) aus dem Linienwerkzeug, das danach wieder frei ist.
fn first_line(ctrl: bool) -> (AppState, PointId) {
    let mut s = sketch_with(|_| {});
    set_tool(&mut s, Tool::Line);
    grid(&mut s, false);
    click(&mut s, v(10.0, 10.0)).unwrap();
    click(&mut s, v(30.0, 10.0)).unwrap();
    let (se, sk) = s.session_sketch_mut().unwrap();
    se.cancel(sk);
    let end = line_ends(sketch(&s), lines(sketch(&s))[0]).1;
    assert_eq!(xy(sketch(&s), end), v(30.0, 10.0));
    grid(&mut s, ctrl);
    (s, end)
}

#[test]
fn a_line_started_on_a_grid_point_over_a_point_binds_to_it_and_the_corner_rounds() {
    let (mut s, end) = first_line(false);
    let points = sketch(&s).points().count();
    {
        let (se, sk) = s.session_sketch_mut().unwrap();
        let cur = se.resolve_cursor(sk, beside_corner(), PICK);
        assert_eq!(cur.snap, Snap::Point(end), "{cur:?}");
        assert_eq!(cur.at, v(30.0, 10.0));
    }
    click(&mut s, beside_corner()).unwrap();
    click(&mut s, v(30.0, 30.0)).unwrap();
    let ls = lines(sketch(&s));
    assert_eq!(ls.len(), 2);
    assert_eq!(line_ends(sketch(&s), ls[1]).0, end, "beginnt am Punkt");
    assert_eq!(
        sketch(&s).points().count(),
        points + 1,
        "kein zweiter Punkt"
    );
    assert_solved(sketch(&s));

    // Die Rundung rundet diese Ecke (Handtest Skizzen 4).
    {
        let (se, sk) = s.session_sketch_mut().unwrap();
        se.cancel(sk);
    }
    choose(&mut s, ToolKind::Fillet);
    click(&mut s, v(20.0, 10.0)).unwrap();
    click(&mut s, v(30.0, 20.0)).unwrap();
    let scope = s.document.param_scope();
    {
        let (se, sk) = s.session_sketch_mut().unwrap();
        se.sync_input(sk);
        se.type_field(0, "3", &scope);
    }
    click(&mut s, v(28.0, 12.0)).unwrap();
    let arcs: Vec<_> = sketch(&s)
        .curves()
        .filter(|(_, c)| matches!(c.kind, CurveKind::Arc { .. }))
        .map(|(id, _)| id)
        .collect();
    assert_eq!(arcs.len(), 1, "die Ecke ist gerundet");
    near(sketch(&s).radius(arcs[0]).unwrap(), 3.0);
    assert_solved(sketch(&s));
}

#[test]
fn with_ctrl_the_start_beside_the_point_stays_a_free_point() {
    let (mut s, end) = first_line(true);
    let points = sketch(&s).points().count();
    {
        let (se, sk) = s.session_sketch_mut().unwrap();
        let cur = se.resolve_cursor(sk, beside_corner(), PICK);
        assert_eq!(cur.snap, Snap::None);
        assert_eq!(cur.at, beside_corner());
    }
    click(&mut s, beside_corner()).unwrap();
    click(&mut s, v(30.0, 30.0)).unwrap();
    let ls = lines(sketch(&s));
    let start = line_ends(sketch(&s), ls[1]).0;
    assert_ne!(start, end);
    assert_eq!(xy(sketch(&s), start), beside_corner());
    assert_eq!(sketch(&s).points().count(), points + 2);
}

#[test]
fn a_grid_point_beside_every_point_stays_the_grid() {
    // Nur genau auf dem Punkt bindet das Raster; 1 mm daneben bleibt es ein Rasterpunkt.
    let (mut s, _) = first_line(false);
    let (se, sk) = s.session_sketch_mut().unwrap();
    let cur = se.resolve_cursor(sk, v(31.3, 11.3), PICK);
    assert_eq!(cur.snap, Snap::Grid);
    assert_eq!(cur.at, v(31.0, 11.0));
}

#[test]
fn a_circle_centered_on_a_grid_point_over_a_point_takes_that_point() {
    let mut s = sketch_with(|sk| {
        sk.add_point(5.0, 5.0).unwrap();
    });
    let p = sketch(&s).points().next().unwrap().0;
    choose(&mut s, ToolKind::CircleCenter);
    grid(&mut s, false);
    click(&mut s, v(5.0, 5.0) + DVec2::splat(0.3)).unwrap();
    click(&mut s, v(15.0, 5.0)).unwrap();
    let sk = sketch(&s);
    let circle = sk
        .curves()
        .find_map(|(_, c)| match c.kind {
            CurveKind::Circle { center, .. } => Some(center),
            _ => None,
        })
        .expect("Kreis");
    assert_eq!(circle, p, "die Mitte ist der vorhandene Punkt");
    assert_eq!(sk.points().count(), 1);
}

#[test]
fn point_at_finds_a_live_point_within_the_tolerance_only() {
    let mut sk = Sketch::new(
        planora_math::Frame::WORLD,
        "S",
        planora_math::Tolerance::default(),
    );
    let a = sk.add_point(1.0, 2.0).unwrap();
    let b = sk.add_point(1.0, 2.0 + 1e-3).unwrap();
    let tol = sk.tolerance().linear;
    assert_eq!(sk.point_at(v(1.0, 2.0), tol), Some(a));
    assert_eq!(sk.point_at(v(1.0, 2.0 + tol * 0.5), tol), Some(a));
    assert_eq!(sk.point_at(v(1.0, 2.0 + 1e-3), tol), Some(b));
    assert_eq!(sk.point_at(v(1.0, 2.0005), tol), None);
    sk.remove_point(a).unwrap();
    assert_eq!(sk.point_at(v(1.0, 2.0), tol), None);
}

// ---- Doppelklick auf Maßtexte (die ganze App) -----------------------------------------------------

struct Shot {
    texts: Vec<(String, Rect)>,
}

fn walk(shape: &Shape, out: &mut Vec<(String, Rect)>) {
    match shape {
        Shape::Vec(v) => v.iter().for_each(|x| walk(x, out)),
        Shape::Text(t) => out.push((
            t.galley.text().to_string(),
            Rect::from_min_size(t.pos, t.galley.size()),
        )),
        _ => {}
    }
}

impl Shot {
    fn of(shapes: &[ClippedShape]) -> Shot {
        let mut texts = Vec::new();
        for c in shapes {
            walk(&c.shape, &mut texts);
        }
        Shot { texts }
    }
}

struct Rig {
    ctx: egui::Context,
    app: PlanoraApp,
    time: f64,
}

impl Rig {
    fn new() -> Rig {
        let ctx = egui::Context::default();
        ctx.set_fonts(fonts::definitions());
        theme::apply(&ctx, true);
        PlanoraApp::setup_context(&ctx);
        let root = common::temp_root("planora-sketch-carried-s5-ui");
        let mut w = Workspace::new(Some(Library::open(&root).unwrap()), Config::default());
        w.config_path = root.join("config.toml");
        let s = &mut w.active_tab_mut().state;
        let sk = s
            .add_sketch(SketchPlane::Standard(StandardPlane::XY))
            .unwrap();
        s.open_sketch(sk).unwrap();
        w.observe_history(false);
        let mut rig = Rig {
            ctx,
            app: PlanoraApp::for_tests(w),
            time: 0.0,
        };
        rig.settle();
        rig
    }

    fn st(&mut self) -> &mut AppState {
        &mut self.app.ws_mut().active_tab_mut().state
    }

    fn sk(&mut self) -> &mut Sketch {
        self.st().session_sketch_mut().unwrap().1
    }

    fn se(&mut self) -> &mut SketchSession {
        self.st().sketch_session.as_mut().unwrap()
    }

    fn frame(&mut self, events: Vec<Event>) -> Shot {
        self.time += 1.0 / 60.0;
        let raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(W, H))),
            time: Some(self.time),
            events,
            ..Default::default()
        };
        let mut f = eframe::Frame::_new_kittest();
        let app = &mut self.app;
        let out = self.ctx.run_ui(raw, |ui| app.ui(ui, &mut f));
        let shot = Shot::of(&out.shapes);
        out.drop_without_applying_deltas();
        shot
    }

    fn settle(&mut self) -> Shot {
        self.frame(vec![]);
        self.frame(vec![]);
        self.frame(vec![])
    }

    fn press(&mut self, k: Key) -> Shot {
        self.frame(vec![Event::Key {
            key: k,
            physical_key: Some(k),
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        }]);
        self.settle()
    }

    fn hover(&mut self, p: Pos2) {
        self.frame(vec![Event::PointerMoved(p)]);
        self.frame(vec![Event::PointerMoved(p)]);
    }

    fn button(p: Pos2, pressed: bool) -> Event {
        Event::PointerButton {
            pos: p,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        }
    }

    fn click(&mut self, p: Pos2) -> Shot {
        self.frame(vec![Event::PointerMoved(p)]);
        self.frame(vec![Rig::button(p, true)]);
        self.frame(vec![Rig::button(p, false)]);
        self.settle()
    }

    fn double_click(&mut self, p: Pos2) -> Shot {
        self.frame(vec![Event::PointerMoved(p)]);
        self.frame(vec![Rig::button(p, true)]);
        self.frame(vec![Rig::button(p, false)]);
        self.frame(vec![Rig::button(p, true)]);
        self.frame(vec![Rig::button(p, false)]);
        self.settle()
    }

    /// Bildpunkt einer Skizzenlage, aus zwei Zeigerlagen (die Ansicht von oben ist linear).
    fn pixels(&mut self) -> impl Fn(DVec2) -> Pos2 + use<> {
        let (a, b) = (pos2(900.0, 500.0), pos2(1100.0, 600.0));
        let mut at = |p: Pos2| {
            self.hover(p);
            self.se()
                .draw_cursor
                .as_ref()
                .map(|c| c.raw)
                .or(self.se().cursor)
        };
        let (p, q) = (at(a).expect("Zeiger"), at(b).expect("Zeiger"));
        move |s: DVec2| {
            let tx = (s.x - p.x) / (q.x - p.x);
            let ty = (s.y - p.y) / (q.y - p.y);
            pos2(a.x + (b.x - a.x) * tx as f32, a.y + (b.y - a.y) * ty as f32)
        }
    }

    fn type_text(&mut self, text: &str) {
        assert!(
            self.ctx.memory(|m| m.focused()).is_some(),
            "kein Feld hat die Tastatur"
        );
        self.frame(vec![Event::Key {
            key: Key::A,
            physical_key: Some(Key::A),
            pressed: true,
            repeat: false,
            modifiers: Modifiers::COMMAND,
        }]);
        for ch in text.chars() {
            self.frame(vec![Event::Text(ch.to_string())]);
        }
    }

    fn editor_open(&self) -> bool {
        sketch_panel::dimension_editor_open(&self.ctx)
    }
}

/// Kreis um (0, 0) mit Ø 10 und eine waagerechte Linie (20, 0)–(60, 0) mit Länge 40.
fn circle_and_line(rig: &mut Rig) -> (CurveId, ConstraintId, CurveId, ConstraintId) {
    let sk = rig.sk();
    let m = sk.add_point(0.0, 0.0).unwrap();
    let c = sk.add_circle(m, 5.0).unwrap();
    let dia = sk.add_constraint(K::Diameter(c, 10.0)).unwrap();
    let a = sk.add_point(20.0, 0.0).unwrap();
    let b = sk.add_point(60.0, 0.0).unwrap();
    let l = sk.add_line(a, b).unwrap();
    let len = sk.add_constraint(K::Distance(a, b, 40.0)).unwrap();
    sk.solve();
    rig.settle();
    (c, dia, l, len)
}

#[test]
fn a_double_click_on_the_diameter_text_opens_its_field_like_the_list() {
    with_lang(&german(), || {
        let mut rig = Rig::new();
        let (c, dia, _, _) = circle_and_line(&mut rig);
        let px = rig.pixels();
        let shot = rig.settle();
        let hits = sketch_view::label_hits(&rig.ctx);
        let rect = hits
            .iter()
            .find(|(id, _)| *id == dia)
            .map(|(_, r)| *r)
            .expect("Maßtext Ø gemerkt");
        // Das gemerkte Rechteck ist das des Texts „Ø 10,00 mm“.
        assert!(
            shot.texts
                .iter()
                .any(|(t, r)| t == "Ø 10,00 mm" && rect.contains(r.center())),
            "{rect:?} {:?}",
            shot.texts
        );
        // Eine Klicklage auf dem Kreis, die nach dem Doppelklick nicht mehr gelten darf.
        rig.click(px(v(5.0, 0.0)));
        assert_eq!(rig.se().selection, vec![Item::Curve(c)]);
        assert!(!rig.se().curve_picks.at.is_empty());
        let shot = rig.double_click(rect.center());
        assert!(rig.editor_open(), "Feld offen");
        assert_eq!(rig.se().selection, vec![Item::Constraint(dia)]);
        assert!(rig.se().curve_picks.at.is_empty(), "keine alte Klicklage");
        assert!(
            shot.texts.iter().any(|(t, _)| t == "10,00" || t == "10"),
            "Wert im Feld: {:?}",
            shot.texts
        );
        // Wie aus der Liste: 12 und Eingabe ändern genau dieses Maß.
        rig.type_text("12");
        rig.press(Key::Enter);
        assert!(!rig.editor_open());
        assert_eq!(rig.sk().constraint(dia).unwrap().kind, K::Diameter(c, 12.0));
    });
}

#[test]
fn a_double_click_beside_the_text_selects_the_chain_as_before() {
    with_lang(&german(), || {
        let mut rig = Rig::new();
        let (_, _, l, len) = circle_and_line(&mut rig);
        let px = rig.pixels();
        rig.settle();
        let rect = sketch_view::label_hits(&rig.ctx)
            .into_iter()
            .find(|(id, _)| *id == len)
            .map(|(_, r)| r)
            .expect("Maßtext der Länge gemerkt");
        let beside = px(v(30.0, 0.0));
        assert!(!rect.expand(4.0).contains(beside), "{rect:?} {beside:?}");
        rig.double_click(beside);
        assert!(!rig.editor_open());
        assert_eq!(rig.se().selection, vec![Item::Curve(l)]);
    });
}

// ---- Eingabe nach abgelehntem Wegklick (Task 8, geparkt) ---------------------------------------

#[test]
fn enter_after_a_refused_click_away_keeps_plan_and_text_and_says_why() {
    with_lang(&german(), || {
        let mut rig = Rig::new();
        rig.sk().add_point(0.0, 0.0).unwrap();
        rig.sk().add_point(40.0, 0.0).unwrap();
        rig.settle();
        rig.press(Key::D);
        let px = rig.pixels();
        rig.click(px(v(0.0, 0.0)));
        rig.click(px(v(40.0, 0.0)));
        rig.click(px(v(20.0, 15.0)));
        assert!(rig.editor_open());
        rig.type_text("0");
        let before = bits(rig.sk());
        // Klick ins Leere: 0 geht nicht, Maß und Text bleiben, die Meldung sagt es.
        rig.click(px(v(20.0, 25.0)));
        assert!(rig.editor_open());
        assert!(rig.se().dim.plan.is_some());
        let refused = rig.se().message_text();
        assert!(refused.is_some());
        // Eingabe (das Feld hat die Tastatur nicht mehr): wieder dieselbe Ablehnung, nichts fällt
        // stumm weg.
        assert!(
            rig.ctx.memory(|m| m.focused()).is_none(),
            "Feld ohne Tastatur"
        );
        rig.se().message = None;
        rig.press(Key::Enter);
        assert!(rig.editor_open(), "Feld bleibt offen");
        assert!(rig.se().dim.plan.is_some(), "Maß bleibt gesetzt");
        assert_eq!(rig.se().dim.text.as_deref(), Some("0"));
        assert_eq!(rig.se().message_text(), refused);
        assert_eq!(bits(rig.sk()), before);
    });
}
