//! Getippte Werte beim Zeichnen (Skizzen 2, Task 4, ohne Oberfläche): Sie sperren die Größe in
//! der Vorschau und beim Klick und werden beim Abschluss zu Maßen; Nicht-Getipptes bleibt frei.

mod common;

use planora_app::sketch_input::{DrawField, FieldKind as F};
use planora_app::sketch_state::{SketchSession, Snap, Tool};
use planora_app::workspace::*;
use planora_feature::{
    ParamEdit, ParamScope, ParamUnit, SketchFeature, SketchPlane, StandardPlane,
};
use planora_i18n::{DisplayUnit, tr, with_unit};
use planora_math::DVec2;
use planora_project::config::Config;
use planora_project::library::Library;
use planora_sketch::constraint::{ConstraintId, ConstraintKind as K, LineOrPoints};
use planora_sketch::model::{CurveId, CurveKind, PointId, Sketch};
use planora_sketch::solver::SolveStatus;

/// Fangradius in Skizzeneinheiten (klein: Fang nur, wo der Test ihn will).
const R: f64 = 0.5;
const EPS: f64 = 1e-9;

fn v(x: f64, y: f64) -> DVec2 {
    DVec2::new(x, y)
}

/// Offene, leere Skizze auf XY; `Breite = 40` als Parameter.
fn rig() -> Workspace {
    let root = common::temp_root("planora-sketch-input");
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
    w
}

fn parts(w: &mut Workspace) -> (&mut SketchSession, &mut SketchFeature, ParamScope) {
    let s = &mut w.active_tab_mut().state;
    let scope = s.document.param_scope();
    let (se, sf) = s.session_draft_mut().unwrap();
    (se, sf, scope)
}

fn tool(w: &mut Workspace, t: Tool) {
    let (se, sf, _) = parts(w);
    se.set_tool(&mut sf.sketch, t);
}

fn commit(w: &mut Workspace, at: DVec2) -> Result<(), String> {
    let (se, sf, scope) = parts(w);
    se.commit_input(sf, at, R, false, &scope)
}

/// Tippt `text` in das Feld der Art `kind` der aktuellen Stufe.
fn type_in(w: &mut Workspace, kind: F, text: &str) {
    let (se, sf, scope) = parts(w);
    se.sync_input(&sf.sketch);
    let i = field_index(se, kind);
    se.type_field(i, text, &scope);
}

fn field_index(se: &SketchSession, kind: F) -> usize {
    se.input
        .fields
        .iter()
        .position(|f| f.kind == kind)
        .unwrap_or_else(|| panic!("kein Feld {kind:?}: {:?}", se.input.fields))
}

fn field(w: &mut Workspace, kind: F) -> DrawField {
    let (se, sf, _) = parts(w);
    se.sync_input(&sf.sketch);
    se.input.fields[field_index(se, kind)].clone()
}

/// Lage, die Vorschau und Klick für die Rohlage ergeben.
fn at(w: &mut Workspace, raw: DVec2) -> DVec2 {
    let (se, sf, _) = parts(w);
    se.resolve_cursor(&sf.sketch, raw, R).at
}

fn sk(w: &Workspace) -> &Sketch {
    w.active_tab().state.session_sketch().unwrap()
}

fn draft(w: &Workspace) -> &SketchFeature {
    w.active_tab().state.sketch_draft.as_ref().unwrap()
}

fn dims(sk: &Sketch) -> Vec<(ConstraintId, K)> {
    sk.constraints()
        .filter(|(_, c)| c.kind.is_dimension())
        .map(|(id, c)| (id, c.kind.clone()))
        .collect()
}

fn lines(sk: &Sketch, construction: bool) -> Vec<CurveId> {
    sk.curves()
        .filter(|(_, c)| c.construction == construction && matches!(c.kind, CurveKind::Line { .. }))
        .map(|(id, _)| id)
        .collect()
}

fn ends(sk: &Sketch, c: CurveId) -> (DVec2, DVec2) {
    let CurveKind::Line { a, b } = sk.curve(c).unwrap().kind else {
        panic!("keine Linie");
    };
    (sk.point_xy(a).unwrap(), sk.point_xy(b).unwrap())
}

fn first_of(sk: &Sketch, pick: impl Fn(&CurveKind) -> bool) -> CurveId {
    sk.curves().find(|(_, c)| pick(&c.kind)).unwrap().0
}

/// Linie von (5, 5) mit gesetztem Start.
fn line_started(w: &mut Workspace) -> DVec2 {
    tool(w, Tool::Line);
    let start = v(5.0, 5.0);
    commit(w, start).unwrap();
    start
}

/// Zwei feste Punkte in der Skizze.
fn fixed_points(w: &mut Workspace, a: DVec2, b: DVec2) -> (PointId, PointId) {
    let (se, sf, _) = parts(w);
    let sk = &mut sf.sketch;
    let p = sk.add_point(a.x, a.y).unwrap();
    let q = sk.add_point(b.x, b.y).unwrap();
    sk.add_constraint(K::Fixed(p, a.x, a.y)).unwrap();
    sk.add_constraint(K::Fixed(q, b.x, b.y)).unwrap();
    se.last_report = Some(sk.solve());
    (p, q)
}

// ---- Linie --------------------------------------------------------------------------------

#[test]
fn length_locks_the_distance_and_becomes_a_dimension() {
    let mut w = rig();
    let start = line_started(&mut w);
    type_in(&mut w, F::Length, "25");
    assert_eq!(field(&mut w, F::Length).locked, Some(25.0));

    // Schräg: Ende auf dem Strahl im Abstand 25; anderer Zeiger, neue Richtung, gleiche Länge.
    let raw = start + v(30.0, 17.0);
    let end = at(&mut w, raw);
    assert!((end.distance(start) - 25.0).abs() < EPS);
    assert!((end - start).perp_dot(raw - start).abs() < 1e-9);
    let other = at(&mut w, start + v(-4.0, 21.0));
    assert!((other.distance(start) - 25.0).abs() < EPS);
    assert!(other.x < start.x);

    commit(&mut w, raw).unwrap();
    let sk = sk(&w);
    let line = lines(sk, false)[0];
    assert!((sk.line_length(line).unwrap() - 25.0).abs() < EPS);
    let ds = dims(sk);
    assert_eq!(ds.len(), 1, "{ds:?}");
    assert!(matches!(ds[0].1, K::Distance(_, _, d) if d == 25.0));
    assert!(
        draft(&w).formulas.is_empty(),
        "reine Zahl in mm: keine Formel"
    );
}

#[test]
fn angle_alone_locks_the_ray_and_uses_one_axis_helper() {
    let mut w = rig();
    let start = line_started(&mut w);
    type_in(&mut w, F::Angle, "30");
    assert_eq!(
        field(&mut w, F::Angle).locked,
        Some(30f64.to_radians()),
        "Winkel im Bogenmaß"
    );

    // Länge folgt dem Zeiger, die Zeigerseite wählt das Vorzeichen.
    let raw = start + v(20.0, 20.0);
    let up = at(&mut w, raw);
    let along = (up - start).normalize();
    assert!((along.y.atan2(along.x) - 30f64.to_radians()).abs() < EPS);
    let longer = at(&mut w, start + v(40.0, 40.0));
    assert!(longer.distance(start) > up.distance(start) * 1.5);
    let down = at(&mut w, start + v(20.0, -20.0));
    assert!(down.y < start.y);

    commit(&mut w, raw).unwrap();
    {
        let sk = sk(&w);
        let helpers = lines(sk, true);
        assert_eq!(helpers.len(), 1);
        assert_eq!(ends(sk, helpers[0]), (v(0.0, 0.0), v(10.0, 0.0)));
        let line = lines(sk, false)[0];
        let ds = dims(sk);
        assert_eq!(ds.len(), 1, "nur der Winkel: {ds:?}");
        assert!(
            matches!(ds[0].1, K::Angle(r, l, a) if r == helpers[0] && l == line
                && (a - 30f64.to_radians()).abs() < EPS)
        );
    }

    // Zweite Linie, neu begonnen: dieselbe Hilfe, und jetzt zum Zeiger nach unten.
    let (se, sf, _) = parts(&mut w);
    se.cancel(&mut sf.sketch);
    let start2 = v(50.0, 50.0);
    commit(&mut w, start2).unwrap();
    type_in(&mut w, F::Angle, "45");
    commit(&mut w, start2 + v(20.0, -20.0)).unwrap();
    let sk2 = sk(&w).clone();
    let helpers = lines(&sk2, true);
    assert_eq!(helpers.len(), 1, "die Hilfe wird wiederverwendet");
    let second = *lines(&sk2, false).last().unwrap();
    let (a, b) = ends(&sk2, second);
    let along = (b - a).normalize();
    assert!((along.y.atan2(along.x) + 45f64.to_radians()).abs() < EPS);
    // Gespeichert positiv über die Reihenfolge: von der Linie zur Hilfe.
    assert!(dims(&sk2).iter().any(
        |(_, k)| matches!(k, K::Angle(l, r, a) if *l == second && *r == helpers[0]
                && (*a - 45f64.to_radians()).abs() < EPS)
    ));

    // Die Hilfe selbst hat keinen Freiheitsgrad.
    let report = sk2.clone().solve();
    assert_eq!(report.status, SolveStatus::Solved);
    assert!(!report.free_curves.contains(&helpers[0]));
    let CurveKind::Line { a, b } = sk2.curve(helpers[0]).unwrap().kind else {
        panic!()
    };
    assert!(!report.free_points.contains(&a) && !report.free_points.contains(&b));
}

#[test]
fn angle_in_a_chain_refers_to_the_previous_line_forward() {
    let mut w = rig();
    let start = line_started(&mut w);
    // Vorige Linie schräg, damit keine Richtung passt.
    let knee = start + v(17.0, 9.0);
    commit(&mut w, knee).unwrap();
    type_in(&mut w, F::Angle, "90");
    commit(&mut w, knee + v(-5.0, 20.0)).unwrap();
    let sk = sk(&w);
    assert!(lines(sk, true).is_empty(), "Kette braucht keine Achshilfe");
    let ls = lines(sk, false);
    let (a0, b0) = ends(sk, ls[0]);
    let (a1, b1) = ends(sk, ls[1]);
    assert!((b0 - a0).dot(b1 - a1).abs() < 1e-7, "rechtwinklig");
    assert!((b0 - a0).perp_dot(b1 - a1) > 0.0, "zur Zeigerseite");
    assert!(dims(sk).iter().any(|(_, k)| matches!(
        k, K::Angle(p, l, a) if *p == ls[0] && *l == ls[1]
            && (*a - 90f64.to_radians()).abs() < EPS
    )));
    assert_eq!(sk.clone().solve().status, SolveStatus::Solved);
}

#[test]
fn length_and_angle_beat_the_snap() {
    let mut w = rig();
    let start = line_started(&mut w);
    let lure = start + v(30.0, 30.0);
    {
        let (_, sf, _) = parts(&mut w);
        sf.sketch.add_point(lure.x, lure.y).unwrap();
    }
    type_in(&mut w, F::Length, "10");
    type_in(&mut w, F::Angle, "90");
    let (se, sf, _) = parts(&mut w);
    let cur = se.resolve_cursor(&sf.sketch, lure, R);
    assert_eq!(cur.snap, Snap::None);
    assert!(cur.at.distance(start + v(0.0, 10.0)) < EPS);
    let points = sf.sketch.points().count();
    commit(&mut w, lure).unwrap();
    let sk = sk(&w);
    assert_eq!(sk.points().count(), points + 3, "Ende + zwei der Achshilfe");
    assert_eq!(dims(sk).len(), 2);
}

#[test]
fn suppressed_snap_keeps_the_locks() {
    let mut w = rig();
    let start = line_started(&mut w);
    type_in(&mut w, F::Length, "10");
    let (se, sf, _) = parts(&mut w);
    se.cursor_opts.suppressed = true;
    let end = se.resolve_cursor(&sf.sketch, start + v(7.0, 6.0), R).at;
    assert!((end.distance(start) - 10.0).abs() < EPS);
}

#[test]
fn length_with_nearly_horizontal_pointer_gets_both() {
    let mut w = rig();
    let start = line_started(&mut w);
    type_in(&mut w, F::Length, "25");
    commit(&mut w, start + v(30.0, 0.5)).unwrap();
    let sk = sk(&w);
    let line = lines(sk, false)[0];
    let (a, b) = ends(sk, line);
    assert_eq!(a.y.to_bits(), b.y.to_bits());
    assert!((a.distance(b) - 25.0).abs() < EPS);
    assert!(sk.constraints().any(|(_, c)| matches!(
        c.kind, K::Horizontal(LineOrPoints::Line(l)) if l == line
    )));
    assert!(
        dims(sk)
            .iter()
            .any(|(_, k)| matches!(k, K::Distance(_, _, d) if *d == 25.0))
    );
}

#[test]
fn untyped_values_make_no_dimension() {
    let mut w = rig();
    let start = line_started(&mut w);
    commit(&mut w, start + v(17.0, 9.0)).unwrap();
    assert!(dims(sk(&w)).is_empty());
}

// ---- Rechteck, Kreis, Bogen, Ellipse --------------------------------------------------------

#[test]
fn rectangle_width_goes_to_the_pointer_side_and_makes_one_dimension() {
    let mut w = rig();
    tool(&mut w, Tool::Rectangle);
    let corner = v(50.0, 10.0);
    commit(&mut w, corner).unwrap();
    type_in(&mut w, F::Width, "30");
    let raw = v(10.0, 27.0);
    let end = at(&mut w, raw);
    assert!((end.x - 20.0).abs() < EPS, "links vom Start: {end:?}");
    assert!((end.y - 27.0).abs() < EPS, "Höhe bleibt frei");
    commit(&mut w, raw).unwrap();
    let sk = sk(&w);
    let ds = dims(sk);
    assert_eq!(ds.len(), 1);
    assert!(matches!(ds[0].1, K::Distance(_, _, d) if d == 30.0));
    let xs: Vec<f64> = sk
        .points()
        .map(|(p, _)| sk.point_xy(p).unwrap().x)
        .collect();
    let min = xs.iter().copied().fold(f64::MAX, f64::min);
    let max = xs.iter().copied().fold(f64::MIN, f64::max);
    assert!((min - 20.0).abs() < EPS && (max - 50.0).abs() < EPS);
}

#[test]
fn rectangle_width_and_height_both_become_dimensions() {
    let mut w = rig();
    tool(&mut w, Tool::Rectangle);
    commit(&mut w, v(0.0, 0.0)).unwrap();
    type_in(&mut w, F::Width, "30");
    type_in(&mut w, F::Height, "12");
    commit(&mut w, v(3.0, -3.0)).unwrap();
    let sk = sk(&w);
    let ds = dims(sk);
    assert_eq!(ds.len(), 2);
    let ys: Vec<f64> = sk
        .points()
        .map(|(p, _)| sk.point_xy(p).unwrap().y)
        .collect();
    assert!(ys.iter().any(|y| (*y + 12.0).abs() < EPS), "unten: {ys:?}");
    assert!(
        ds.iter()
            .any(|(_, k)| matches!(k, K::Distance(_, _, d) if *d == 30.0))
    );
    assert!(
        ds.iter()
            .any(|(_, k)| matches!(k, K::Distance(_, _, d) if *d == 12.0))
    );
}

#[test]
fn circle_diameter() {
    let mut w = rig();
    tool(&mut w, Tool::Circle);
    commit(&mut w, v(5.0, 5.0)).unwrap();
    type_in(&mut w, F::Diameter, "20");
    commit(&mut w, v(40.0, 7.0)).unwrap();
    let sk = sk(&w);
    let c = first_of(sk, |k| matches!(k, CurveKind::Circle { .. }));
    assert!((sk.radius(c).unwrap() - 10.0).abs() < EPS);
    let ds = dims(sk);
    assert_eq!(ds.len(), 1);
    assert!(matches!(ds[0].1, K::Diameter(x, d) if x == c && d == 20.0));
}

#[test]
fn arc_radius_then_sweep_become_dimensions() {
    let mut w = rig();
    tool(&mut w, Tool::Arc);
    let m = v(5.0, 5.0);
    commit(&mut w, m).unwrap();
    type_in(&mut w, F::Radius, "12");
    commit(&mut w, m + v(30.0, 4.0)).unwrap();
    assert!(dims(sk(&w)).is_empty(), "das Maß folgt beim Abschluss");
    type_in(&mut w, F::Sweep, "270");
    commit(&mut w, m + v(-3.0, 9.0)).unwrap();

    let sk = sk(&w);
    let arc = first_of(sk, |k| matches!(k, CurveKind::Arc { .. }));
    let CurveKind::Arc { center, start, end } = sk.curve(arc).unwrap().kind else {
        panic!()
    };
    let (c, s, e) = (
        sk.point_xy(center).unwrap(),
        sk.point_xy(start).unwrap(),
        sk.point_xy(end).unwrap(),
    );
    assert!((s.distance(c) - 12.0).abs() < EPS);
    let (u, t) = (s - c, e - c);
    let sweep = u
        .perp_dot(t)
        .atan2(u.dot(t))
        .rem_euclid(std::f64::consts::TAU);
    assert!((sweep.to_degrees() - 270.0).abs() < 1e-7, "{sweep}");
    let ds = dims(sk);
    assert_eq!(ds.len(), 2, "{ds:?}");
    assert!(
        ds.iter()
            .any(|(_, k)| matches!(k, K::Radius(x, r) if *x == arc && *r == 12.0))
    );
    assert!(ds.iter().any(|(_, k)| matches!(
        k, K::Angle(_, _, a) if (*a - 270f64.to_radians()).abs() < EPS
    )));
    assert_eq!(lines(sk, true).len(), 2, "Mitte→Start und Mitte→Ende");
    let report = sk.clone().solve();
    assert_eq!(report.status, SolveStatus::Solved);
    assert!(report.redundant.is_empty(), "{:?}", report.redundant);
}

#[test]
fn ellipse_axes_become_dimensions_and_minor_too_big_stays_at_the_field() {
    let mut w = rig();
    tool(&mut w, Tool::Ellipse);
    let m = v(5.0, 5.0);
    commit(&mut w, m).unwrap();
    type_in(&mut w, F::MajorAxis, "30");
    commit(&mut w, m + v(10.0, 2.0)).unwrap();

    type_in(&mut w, F::MinorAxis, "31");
    let big = field(&mut w, F::MinorAxis);
    assert_eq!(big.locked, None);
    assert_eq!(big.error, Some(tr!("sketch-field-minor-too-big")));
    let before = draft(&w).clone();
    assert!(commit(&mut w, m + v(1.0, 5.0)).is_err());
    assert!(*draft(&w) == before);

    type_in(&mut w, F::MinorAxis, "10");
    assert_eq!(field(&mut w, F::MinorAxis).error, None);
    commit(&mut w, m + v(1.0, 5.0)).unwrap();
    let sk = sk(&w);
    let e = first_of(sk, |k| matches!(k, CurveKind::Ellipse { .. }));
    let ds = dims(sk);
    assert_eq!(ds.len(), 2);
    assert!(
        ds.iter()
            .any(|(_, k)| matches!(k, K::EllipseMajor(c, a) if *c == e && *a == 30.0))
    );
    assert!(
        ds.iter()
            .any(|(_, k)| matches!(k, K::EllipseMinor(c, b) if *c == e && *b == 10.0))
    );
    assert_eq!(sk.clone().solve().status, SolveStatus::Solved);
}

#[test]
fn point_and_spline_have_no_fields() {
    let mut w = rig();
    for t in [Tool::Point, Tool::Spline, Tool::Select] {
        tool(&mut w, t);
        let (se, sf, _) = parts(&mut w);
        se.sync_input(&sf.sketch);
        assert!(se.input.fields.is_empty(), "{t:?}");
    }
    tool(&mut w, Tool::Point);
    commit(&mut w, v(1.0, 1.0)).unwrap();
    assert_eq!(sk(&w).points().count(), 1);
}

// ---- Formeln, Einheiten, Fehler -------------------------------------------------------------

#[test]
fn formula_with_parameter_is_kept() {
    let mut w = rig();
    let start = line_started(&mut w);
    type_in(&mut w, F::Length, "Breite / 2");
    assert_eq!(field(&mut w, F::Length).locked, Some(20.0));
    commit(&mut w, start + v(30.0, 17.0)).unwrap();
    let (id, kind) = dims(sk(&w)).remove(0);
    assert!(matches!(kind, K::Distance(_, _, d) if d == 20.0));
    assert_eq!(
        draft(&w).formulas.get(&id).map(String::as_str),
        Some("Breite / 2")
    );
}

#[test]
fn inch_field_stores_the_unit_and_keeps_formulas() {
    with_unit(DisplayUnit::Zoll, || {
        let mut w = rig();
        let start = line_started(&mut w);
        type_in(&mut w, F::Length, "1");
        assert_eq!(field(&mut w, F::Length).locked, Some(25.4));
        commit(&mut w, start + v(30.0, 17.0)).unwrap();
        let (id, kind) = dims(sk(&w)).remove(0);
        assert!(matches!(kind, K::Distance(_, _, d) if d == 25.4));
        assert_eq!(
            draft(&w).formulas.get(&id).map(String::as_str),
            Some("1 in")
        );

        // Formel mit Parameter bleibt, Winkel kennen keine Zoll.
        let last = *lines(sk(&w), false).last().unwrap();
        let knee = ends(sk(&w), last).1;
        type_in(&mut w, F::Length, "Breite / 2");
        type_in(&mut w, F::Angle, "90");
        commit(&mut w, knee + v(-9.0, 20.0)).unwrap();
        let ds = dims(sk(&w));
        let (id, _) = ds
            .iter()
            .find(|(_, k)| matches!(k, K::Distance(_, _, d) if (*d - 20.0).abs() < EPS))
            .unwrap();
        assert_eq!(
            draft(&w).formulas.get(id).map(String::as_str),
            Some("Breite / 2")
        );
        let (angle, _) = ds.iter().find(|(_, k)| matches!(k, K::Angle(..))).unwrap();
        assert!(!draft(&w).formulas.contains_key(angle), "90 ist ein Winkel");
    });
}

#[test]
fn empty_frees_and_invalid_text_blocks_the_commit() {
    let mut w = rig();
    let start = line_started(&mut w);
    type_in(&mut w, F::Length, "25");
    type_in(&mut w, F::Length, "  ");
    let f = field(&mut w, F::Length);
    assert_eq!((f.locked, f.error), (None, None));
    let raw = start + v(30.0, 17.0);
    assert_eq!(at(&mut w, raw), raw);

    type_in(&mut w, F::Length, "2 +");
    let f = field(&mut w, F::Length);
    assert_eq!(f.locked, None);
    let shown = f.error.expect("Meldung am Feld");
    assert_eq!(at(&mut w, raw), raw, "ungültig gibt frei");
    let before = draft(&w).clone();
    assert_eq!(commit(&mut w, raw).unwrap_err(), shown);
    assert!(*draft(&w) == before);
    let (se, _, _) = parts(&mut w);
    assert_eq!(se.message_text(), Some(shown));
    assert_eq!(se.input.fields[0].text, "2 +", "der Text bleibt");
}

#[test]
fn out_of_range_values_give_a_message() {
    let mut w = rig();
    line_started(&mut w);
    let positive = tr!("sketch-field-positive");
    let range = tr!("sketch-field-angle-range");
    for (kind, text, want) in [
        (F::Length, "0", &positive),
        (F::Length, "-5", &positive),
        (F::Angle, "200", &range),
        (F::Angle, "-1", &range),
    ] {
        type_in(&mut w, kind, text);
        let f = field(&mut w, kind);
        assert_eq!(f.locked, None, "{text}");
        assert_eq!(f.error.as_ref(), Some(want), "{text}");
        type_in(&mut w, kind, "");
    }
    for ok in ["0", "180"] {
        type_in(&mut w, F::Angle, ok);
        assert_eq!(field(&mut w, F::Angle).error, None, "{ok}");
    }

    let mut w = rig();
    tool(&mut w, Tool::Arc);
    commit(&mut w, v(5.0, 5.0)).unwrap();
    commit(&mut w, v(25.0, 5.0)).unwrap();
    for bad in ["0", "360", "400"] {
        type_in(&mut w, F::Sweep, bad);
        let f = field(&mut w, F::Sweep);
        assert_eq!(f.locked, None, "{bad}");
        assert_eq!(f.error, Some(tr!("sketch-field-sweep-range")), "{bad}");
    }
}

#[test]
fn contradicting_dimension_rejects_the_whole_commit() {
    let mut w = rig();
    // Liegt noch im Fang des Endes (≤ tol.linear), aber weit über der Toleranz des Lösers.
    let (p, _) = fixed_points(&mut w, v(0.0, 0.0), v(25.0 + 5e-7, 0.0));
    tool(&mut w, Tool::Line);
    commit(&mut w, v(0.0, 0.0)).unwrap();
    assert_eq!(parts(&mut w).0.pending, vec![p]);
    type_in(&mut w, F::Length, "25");
    let before = draft(&w).clone();
    let bits: Vec<u64> = before
        .sketch
        .variables()
        .iter()
        .map(|x| x.to_bits())
        .collect();

    let e = commit(&mut w, v(25.0, 0.0)).unwrap_err();
    assert_eq!(e, tr!("sketch-field-rejected"));
    assert!(*draft(&w) == before);
    let now: Vec<u64> = sk(&w).variables().iter().map(|x| x.to_bits()).collect();
    assert_eq!(now, bits, "Skizze bitgleich");
    assert!(sk(&w).curves().next().is_none());
    let (se, _, _) = parts(&mut w);
    assert_eq!(se.pending, vec![p], "Sitzung wie vorher");
    assert_eq!(se.line_chain, None);
    assert_eq!(se.input.fields[0].locked, Some(25.0), "der Wert bleibt");
    assert_eq!(se.message_text(), Some(tr!("sketch-field-rejected")));
    assert!(se.created.is_none());
}

#[test]
fn superfluous_dimension_becomes_driven_with_a_message() {
    let mut w = rig();
    fixed_points(&mut w, v(0.0, 0.0), v(25.0, 0.0));
    tool(&mut w, Tool::Line);
    commit(&mut w, v(0.0, 0.0)).unwrap();
    type_in(&mut w, F::Length, "25");
    commit(&mut w, v(25.0, 0.0)).unwrap();
    let (id, _) = dims(sk(&w)).remove(0);
    assert!(sk(&w).constraint(id).unwrap().driven);
    assert!(draft(&w).formulas.is_empty());
    let (se, _, _) = parts(&mut w);
    assert_eq!(se.message_text(), Some(tr!("sketch-became-driven")));
}

// ---- Felder je Stufe ------------------------------------------------------------------------

#[test]
fn stage_change_and_new_chain_point_empty_the_fields() {
    let mut w = rig();
    let start = line_started(&mut w);
    type_in(&mut w, F::Length, "25");
    commit(&mut w, start + v(30.0, 17.0)).unwrap();
    let (se, sf, _) = parts(&mut w);
    se.sync_input(&sf.sketch);
    assert_eq!(se.input.fields.len(), 2, "Länge und Winkel");
    assert!(
        se.input
            .fields
            .iter()
            .all(|f| f.locked.is_none() && !f.touched)
    );

    let mut w = rig();
    arc_with_carried_radius(&mut w);
    let (se, sf, _) = parts(&mut w);
    se.sync_input(&sf.sketch);
    let kinds: Vec<F> = se.input.fields.iter().map(|f| f.kind).collect();
    assert_eq!(kinds, vec![F::Sweep]);
    assert!(se.input.fields[0].locked.is_none());
}

/// Bogen bis nach der ersten Stufe mit getipptem Radius.
fn arc_with_carried_radius(w: &mut Workspace) {
    tool(w, Tool::Arc);
    let m = v(5.0, 5.0);
    commit(w, m).unwrap();
    type_in(w, F::Radius, "12");
    commit(w, m + v(30.0, 4.0)).unwrap();
}

/// Ein neuer Bogen ohne Tippen darf kein altes Radiusmaß mehr bekommen.
fn assert_no_radius_after_restart(w: &mut Workspace) {
    let m = v(70.0, 70.0);
    commit(w, m).unwrap();
    commit(w, m + v(9.0, 1.0)).unwrap();
    commit(w, m + v(-2.0, 7.0)).unwrap();
    assert!(
        !dims(sk(w)).iter().any(|(_, k)| matches!(k, K::Radius(..))),
        "kein altes Maß"
    );
}

#[test]
fn cancel_forgets_carried_values() {
    let mut w = rig();
    arc_with_carried_radius(&mut w);
    let (se, sf, _) = parts(&mut w);
    se.cancel(&mut sf.sketch);
    assert_no_radius_after_restart(&mut w);
}

#[test]
fn set_tool_forgets_carried_values() {
    let mut w = rig();
    arc_with_carried_radius(&mut w);
    tool(&mut w, Tool::Line);
    tool(&mut w, Tool::Arc);
    assert_no_radius_after_restart(&mut w);
}

#[test]
fn history_jump_forgets_carried_values() {
    let mut w = rig();
    arc_with_carried_radius(&mut w);
    let (se, sf, _) = parts(&mut w);
    se.after_history_jump(&sf.sketch);
    assert_no_radius_after_restart(&mut w);
}

#[test]
fn next_field_cycles_and_keeps_locks() {
    let mut w = rig();
    line_started(&mut w);
    type_in(&mut w, F::Length, "25");
    let (se, _, _) = parts(&mut w);
    assert_eq!(se.input.active, 0);
    se.next_field(false);
    assert_eq!(se.input.active, 1);
    se.next_field(false);
    assert_eq!(se.input.active, 0);
    se.next_field(true);
    assert_eq!(se.input.active, 1);
    assert_eq!(se.input.fields[0].locked, Some(25.0));
}

#[test]
fn field_value_shows_the_preview_or_the_lock() {
    let mut w = rig();
    let start = line_started(&mut w);
    let (se, sf, _) = parts(&mut w);
    se.update_cursor(&sf.sketch, Some(start + v(30.0, 40.0)), R);
    assert!((se.field_value(&sf.sketch, F::Length).unwrap() - 50.0).abs() < EPS);
    let a = se.field_value(&sf.sketch, F::Angle).unwrap();
    assert!((a - (40f64 / 30.0).atan()).abs() < EPS);
    assert_eq!(se.field_value(&sf.sketch, F::Diameter), None);
    type_in(&mut w, F::Length, "25");
    let (se, sf, _) = parts(&mut w);
    assert_eq!(se.field_value(&sf.sketch, F::Length), Some(25.0));
}

#[test]
fn field_kinds_know_unit_and_label() {
    assert_eq!(F::Length.unit(), ParamUnit::Length);
    assert_eq!(F::Angle.unit(), ParamUnit::Angle);
    assert_eq!(F::Sweep.unit(), ParamUnit::Angle);
    assert_eq!(F::MinorAxis.unit(), ParamUnit::Length);
    assert_eq!(F::Length.label(), tr!("sketch-field-length"));
}

// ---- Ein Rückgängig-Schritt je Kurve --------------------------------------------------------

#[test]
fn commit_with_helper_and_dimensions_is_one_named_undo_step() {
    let mut w = rig();
    let start = line_started(&mut w);
    w.observe_history(false);
    let steps = w.active_tab().state.sketch_history().unwrap().undo_len();
    type_in(&mut w, F::Length, "25");
    type_in(&mut w, F::Angle, "30");
    w.observe_history(false);
    assert_eq!(
        w.active_tab().state.sketch_history().unwrap().undo_len(),
        steps,
        "Tippen ist kein Schritt"
    );
    commit(&mut w, start + v(30.0, 17.0)).unwrap();
    w.observe_history(false);
    let s = &w.active_tab().state;
    assert_eq!(s.sketch_history().unwrap().undo_len(), steps + 1);
    assert_eq!(s.sketch_undo_label(), Some(tr!("undo-line")));

    assert!(w.active_tab_mut().state.undo_sketch());
    let sk = sk(&w);
    assert!(sk.curves().next().is_none(), "Linie, Hilfe, Maße zusammen");
    assert!(dims(sk).is_empty());
}

#[test]
fn arc_with_sweep_is_named_arc() {
    let mut w = rig();
    arc_with_carried_radius(&mut w);
    type_in(&mut w, F::Sweep, "270");
    commit(&mut w, v(2.0, 14.0)).unwrap();
    w.observe_history(false);
    assert_eq!(
        w.active_tab().state.sketch_undo_label(),
        Some(tr!("undo-arc"))
    );
}

#[test]
fn failed_final_click_keeps_carried_values() {
    let mut w = rig();
    arc_with_carried_radius(&mut w);
    let start = {
        let (se, sf, _) = parts(&mut w);
        sf.sketch.point_xy(se.pending[1]).unwrap()
    };
    let before = draft(&w).clone();
    // Ende auf dem Anfang: abgelehnt, der Radius wartet weiter.
    assert!(commit(&mut w, start).is_err());
    assert!(*draft(&w) == before);
    commit(&mut w, v(2.0, 14.0)).unwrap();
    assert!(
        dims(sk(&w))
            .iter()
            .any(|(_, k)| matches!(k, K::Radius(_, r) if *r == 12.0))
    );
}

#[test]
fn minor_axis_equal_to_major_is_allowed() {
    let mut w = rig();
    tool(&mut w, Tool::Ellipse);
    let m = v(5.0, 5.0);
    commit(&mut w, m).unwrap();
    type_in(&mut w, F::MajorAxis, "30");
    commit(&mut w, m + v(10.0, 2.0)).unwrap();
    type_in(&mut w, F::MinorAxis, "30");
    assert_eq!(field(&mut w, F::MinorAxis).error, None);
    commit(&mut w, m + v(1.0, 5.0)).unwrap();
    assert_eq!(dims(sk(&w)).len(), 2);
    assert_eq!(sk(&w).clone().solve().status, SolveStatus::Solved);
}

// ---- Sperre gegen Automatik -----------------------------------------------------------------

#[test]
fn axis_angle_zero_replaces_the_horizontal_automatic() {
    let mut w = rig();
    let start = line_started(&mut w);
    type_in(&mut w, F::Angle, "0");
    let raw = start + v(30.0, 0.5);
    let (se, sf, _) = parts(&mut w);
    assert!(se.resolve_cursor(&sf.sketch, raw, R).autos.is_empty());
    commit(&mut w, raw).unwrap();
    let sk = sk(&w);
    assert!(
        !sk.constraints()
            .any(|(_, c)| matches!(c.kind, K::Horizontal(_))),
        "keine Automatik neben dem Winkel"
    );
    let (id, _) = dims(sk)
        .into_iter()
        .find(|(_, k)| matches!(k, K::Angle(..)))
        .unwrap();
    assert!(!sk.constraint(id).unwrap().driven, "der Winkel bestimmt");
}

#[test]
fn locked_radius_off_the_point_is_not_tangent() {
    let mut w = rig();
    // Linie endet in (20, 0); ein Bogen um (20, 10) liefe dort tangential weiter.
    let end = v(20.0, 0.0);
    {
        let (_, sf, _) = parts(&mut w);
        let a = sf.sketch.add_point(0.0, 0.0).unwrap();
        let b = sf.sketch.add_point(end.x, end.y).unwrap();
        sf.sketch.add_line(a, b).unwrap();
    }
    tool(&mut w, Tool::Arc);
    let m = v(20.0, 10.0);
    commit(&mut w, m).unwrap();
    let (se, sf, _) = parts(&mut w);
    assert!(
        !se.resolve_cursor(&sf.sketch, end, R).autos.is_empty(),
        "ohne Sperre tangential"
    );
    type_in(&mut w, F::Radius, "5");
    let (se, sf, _) = parts(&mut w);
    let cur = se.resolve_cursor(&sf.sketch, end, R);
    assert!(cur.autos.is_empty(), "{:?}", cur.autos);
    assert!(cur.at.distance(v(20.0, 5.0)) < EPS);
    commit(&mut w, end).unwrap();
    assert_eq!(parts(&mut w).0.auto_tangent, None);
    commit(&mut w, m + v(5.0, 1.0)).unwrap();
    assert!(
        !sk(&w)
            .constraints()
            .any(|(_, c)| matches!(c.kind, K::Tangent(..)))
    );
}

#[test]
fn pointer_behind_the_ray_keeps_its_distance_as_length() {
    let mut w = rig();
    let start = line_started(&mut w);
    type_in(&mut w, F::Angle, "120");
    // Zeiger fast waagerecht rechts: hinter dem Strahl, der nach links oben zeigt.
    let raw = start + v(20.0, 3.0);
    let end = at(&mut w, raw);
    let ray = DVec2::from_angle(120f64.to_radians());
    assert!(
        (end - start).dot(ray) > 0.0,
        "auf dem Strahl, nicht dahinter"
    );
    assert!((end - start).perp_dot(ray).abs() < EPS);
    assert!((end.distance(start) - raw.distance(start)).abs() < EPS);
    commit(&mut w, raw).unwrap();
    let line = lines(sk(&w), false)[0];
    assert!((sk(&w).line_length(line).unwrap() - raw.distance(start)).abs() < EPS);
}

#[test]
fn angles_at_the_ends_of_the_range_commit() {
    for (text, deg) in [("0", 0.0f64), ("180", 180.0)] {
        let mut w = rig();
        let start = line_started(&mut w);
        type_in(&mut w, F::Angle, text);
        commit(&mut w, start + v(20.0, 10.0)).unwrap();
        let sk = sk(&w);
        let (id, kind) = dims(sk).remove(0);
        assert!(
            matches!(kind, K::Angle(_, _, a) if (a - deg.to_radians()).abs() < EPS),
            "{text}: {kind:?}"
        );
        assert!(!sk.constraint(id).unwrap().driven, "{text}");
        let (a, b) = ends(sk, lines(sk, false)[0]);
        let want = if deg == 0.0 { 1.0 } else { -1.0 };
        assert!((b - a).x * want > 0.0 && (b.y - a.y).abs() < 1e-9, "{text}");
        assert_eq!(sk.clone().solve().status, SolveStatus::Solved, "{text}");
    }
}

#[test]
fn superfluous_sweep_leaves_no_helper_lines() {
    let mut w = rig();
    // Mitte, Anfang und Ende fest: Der Öffnungswinkel ist schon bestimmt.
    let m = v(100.0, 100.0);
    let (start, end) = (v(110.0, 100.0), v(100.0, 110.0));
    fixed_points(&mut w, m, start);
    fixed_points(&mut w, end, v(200.0, 200.0));
    tool(&mut w, Tool::Arc);
    commit(&mut w, m).unwrap();
    commit(&mut w, start).unwrap();
    type_in(&mut w, F::Sweep, "90");
    commit(&mut w, end).unwrap();
    w.observe_history(false);
    let sk = sk(&w);
    assert_eq!(
        sk.curves()
            .filter(|(_, c)| matches!(c.kind, CurveKind::Arc { .. }))
            .count(),
        1
    );
    assert!(lines(sk, true).is_empty(), "keine Hilfslinien");
    assert!(dims(sk).is_empty(), "{:?}", dims(sk));
    assert_eq!(sk.clone().solve().status, SolveStatus::Solved);
    assert_eq!(
        w.active_tab().state.sketch_undo_label(),
        Some(tr!("undo-arc"))
    );
    let (se, _, _) = parts(&mut w);
    assert_eq!(se.message_text(), Some(tr!("sketch-field-sweep-redundant")));
}

/// Handtest: Mit festgehaltener Länge sprang die Richtung von Rasterpunkt zu Rasterpunkt, weit weg
/// vom Zeiger.
#[test]
fn a_locked_length_aims_at_the_pointer_not_at_the_grid() {
    let mut w = rig();
    let from = line_started(&mut w);
    parts(&mut w).0.cursor_opts.grid_step = Some(10.0);
    type_in(&mut w, F::Length, "30");
    let raw = from + v(16.0, 13.0);
    let got = at(&mut w, raw);
    assert!(((got - from).length() - 30.0).abs() < 1e-9, "{got}");
    let (aim, seen) = ((raw - from).normalize(), (got - from).normalize());
    assert!(aim.distance(seen) < 1e-9, "{aim} {seen}");
}
