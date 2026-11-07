//! Handtest Skizzen 5, Korrektur 5: Ein freier 3-Punkt-Bogen (mit dem Werkzeug gezeichnet, mit
//! nichts verbunden, ohne Maße) bleibt bitgenau stehen, wenn anderswo ein Maß geändert oder ein
//! Glatt gesetzt wird. Ohne Fenster, Skizze auf XZ wie im Handtest.

mod common;

use planora_app::app::PlanoraApp;
use planora_app::sketch_kinds::ToolKind;
use planora_app::sketch_state::{ConstraintAction, Item};
use planora_app::state::{AppState, StandardPlane};
use planora_app::workspace::Workspace;
use planora_feature::SketchPlane;
use planora_math::DVec2;
use planora_project::config::Config;
use planora_project::library::Library;
use planora_sketch::constraint::ConstraintKind as K;
use planora_sketch::model::{CurveKind, PointId, Sketch};
use planora_sketch::solver::SolveStatus;

const R: f64 = 0.5;

fn at(x: f64, y: f64) -> DVec2 {
    DVec2::new(x, y)
}

fn state(app: &mut PlanoraApp) -> &mut AppState {
    &mut app.ws_mut().active_tab_mut().state
}

fn sk(app: &mut PlanoraApp) -> &mut Sketch {
    state(app).session_sketch_mut().unwrap().1
}

/// App mit offener, leerer Skizze auf XZ.
fn sketching() -> PlanoraApp {
    let root = common::temp_root("planora-sketch-free-parts");
    let ws = Workspace::new(Some(Library::open(&root).unwrap()), Config::default());
    let mut app = PlanoraApp::for_tests(ws);
    let s = state(&mut app);
    let id = s
        .add_sketch(SketchPlane::Standard(StandardPlane::XZ))
        .unwrap();
    s.open_sketch(id).unwrap();
    app
}

fn click(app: &mut PlanoraApp, p: DVec2) {
    let (se, sk) = state(app).session_sketch_mut().unwrap();
    se.click(sk, p, R, false).unwrap();
}

/// Zeichnet mit dem Werkzeug einen 3-Punkt-Bogen durch `start`, `end` und `on`; liefert seine
/// drei Punkte.
fn draw_arc(app: &mut PlanoraApp, start: DVec2, end: DVec2, on: DVec2) -> [PointId; 3] {
    {
        let (se, sk) = state(app).session_sketch_mut().unwrap();
        se.choose_kind(sk, ToolKind::Arc3P);
    }
    click(app, start);
    click(app, end);
    click(app, on);
    let s = sk(app);
    let (_, data) = s
        .curves()
        .find(|(_, c)| matches!(c.kind, CurveKind::Arc { .. }))
        .expect("Bogen angelegt");
    let CurveKind::Arc { center, start, end } = data.kind else {
        unreachable!()
    };
    [center, start, end]
}

fn bits(s: &Sketch, points: &[PointId]) -> Vec<u64> {
    points
        .iter()
        .flat_map(|&p| {
            let q = s.point_xy(p).unwrap();
            [q.x.to_bits(), q.y.to_bits()]
        })
        .collect()
}

fn pt(app: &mut PlanoraApp, x: f64, y: f64) -> PointId {
    sk(app).add_point(x, y).unwrap()
}

fn assert_solved(app: &mut PlanoraApp) {
    let r = sk(app).clone().solve();
    assert_eq!(r.status, SolveStatus::Solved, "{r:?}");
}

#[test]
fn a_drawn_free_arc_stays_bit_exact_when_a_dimension_elsewhere_changes() {
    let mut app = sketching();
    let arc = draw_arc(&mut app, at(3.1, 7.3), at(28.7, 11.9), at(17.2, 22.6));
    // Keine Bindung entstanden (die innere des Bogens zählt `constraints` nicht).
    assert_eq!(sk(&mut app).constraints().count(), 0);
    let a = pt(&mut app, -40.0, -5.0);
    let b = pt(&mut app, -10.3, -4.1);
    sk(&mut app).add_line(a, b).unwrap();
    sk(&mut app)
        .add_constraint(K::Fixed(a, -40.0, -5.0))
        .unwrap();
    let dim = sk(&mut app)
        .add_constraint(K::Distance(a, b, 30.0))
        .unwrap();
    assert_solved(&mut app);
    let before = bits(sk(&mut app), &arc);
    for value in [45.0, 12.5, 80.0] {
        let (se, s) = state(&mut app).session_sketch_mut().unwrap();
        se.set_dimension_value(s, dim, value).unwrap();
        assert_eq!(bits(s, &arc), before, "Bogen rutscht bei Maß {value}");
    }
}

#[test]
fn a_drawn_free_arc_stays_bit_exact_when_smooth_is_added_elsewhere() {
    let mut app = sketching();
    let arc = draw_arc(&mut app, at(50.3, 7.3), at(78.7, 11.9), at(67.2, 22.6));
    let a = pt(&mut app, -10.0, 0.0);
    let joint = pt(&mut app, 0.0, 0.0);
    let l = sk(&mut app).add_line(a, joint).unwrap();
    let q1 = pt(&mut app, 5.0, 3.0);
    let q2 = pt(&mut app, 10.0, 0.0);
    let sp = sk(&mut app).add_spline(&[joint, q1, q2]).unwrap();
    sk(&mut app)
        .add_constraint(K::Fixed(a, -10.0, 0.0))
        .unwrap();
    assert_solved(&mut app);
    let before = bits(sk(&mut app), &arc);
    let (se, s) = state(&mut app).session_sketch_mut().unwrap();
    se.selection = vec![Item::Curve(l), Item::Curve(sp)];
    se.apply(s, ConstraintAction::Smooth, None).unwrap();
    assert!(matches!(s.constraints().last(), Some((_, c)) if matches!(c.kind, K::Smooth(..))));
    assert_eq!(bits(s, &arc), before, "Bogen rutscht beim Glatt");
}
