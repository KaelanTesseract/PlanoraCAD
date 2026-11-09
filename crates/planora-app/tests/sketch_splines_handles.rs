//! Skizzen 7, Task 13: Griffe in der Oberfläche (Spec B6, B7, D3, D4): gedämpfte Griffe, der erste Zug
//! schaltet ein, Tangenten- und Krümmungsgriff, Rho-Griff, Grad-Feld und Kontrollpunkt einfügen. Die
//! Sitzung ohne Oberfläche und die ganze App bei 1920×1080.

mod common;

use eframe::App as _;
use egui::{Event, Key, Modifiers, PointerButton, Pos2, Rect, Vec2, vec2};
use planora_app::app::PlanoraApp;
use planora_app::keymap::{Action, Keymap};
use planora_app::sketch_handles::{HandlePart, HandleView};
use planora_app::sketch_kinds::ToolKind;
use planora_app::sketch_panel;
use planora_app::sketch_state::{ConstraintAction, Item, SketchSession, Snap, Tool};
use planora_app::state::{AppState, StandardPlane};
use planora_app::workspace::{DocTab, Workspace};
use planora_app::{fonts, theme};
use planora_feature::SketchPlane;
use planora_i18n::{english, german, tr, with_lang};
use planora_math::DVec2;
use planora_project::config::Config;
use planora_project::library::Library;
use planora_sketch::constraint::ConstraintKind as K;
use planora_sketch::model::{CurveId, CurveKind, PointId, Sketch};
use planora_sketch::pattern::{PatternDir, PatternKind, PatternPlan};
use planora_sketch::solver::SolveStatus;
use planora_sketch::{HandleCurvature, SplineHandle, conic_shoulder};

const R: f64 = 0.5;
const WAVE: [(f64, f64); 5] = [
    (0.0, 0.0),
    (10.0, 6.0),
    (21.0, -3.0),
    (30.0, 8.0),
    (42.0, 5.0),
];

fn v(x: f64, y: f64) -> DVec2 {
    DVec2::new(x, y)
}

fn with_sketch(build: impl FnOnce(&mut Sketch)) -> AppState {
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

fn add_spline(sk: &mut Sketch, xy: &[(f64, f64)]) -> (CurveId, Vec<PointId>) {
    let pts: Vec<PointId> = xy
        .iter()
        .map(|&(x, y)| sk.add_point(x, y).unwrap())
        .collect();
    (sk.add_spline(&pts).unwrap(), pts)
}

/// Eine Skizze mit einem Spline und einer Linie, die mit ihm nichts zu tun hat.
fn wave() -> (AppState, CurveId, Vec<PointId>) {
    let mut out = None;
    let s = with_sketch(|sk| {
        out = Some(add_spline(sk, &WAVE));
        let a = sk.add_point(0.0, 40.0).unwrap();
        let b = sk.add_point(20.0, 40.0).unwrap();
        sk.add_line(a, b).unwrap();
    });
    let (c, pts) = out.unwrap();
    (s, c, pts)
}

/// Derselbe Stand: gleiche Daten und gleiche Zähler, so sieht der Verlauf keine Änderung.
fn unchanged(a: &Sketch, b: &Sketch) -> bool {
    a.revision() == b.revision() && a.to_data() == b.to_data()
}

fn sk(s: &AppState) -> &Sketch {
    s.session_sketch().unwrap()
}

fn se(s: &mut AppState) -> &mut SketchSession {
    s.sketch_session.as_mut().unwrap()
}

fn select(s: &mut AppState, items: &[Item]) {
    se(s).selection = items.to_vec();
}

fn views(s: &AppState) -> Vec<HandleView> {
    s.sketch_session.as_ref().unwrap().visible_handles(sk(s))
}

fn ghost(s: &AppState, part: HandlePart, p: PointId) -> HandleView {
    views(s)
        .into_iter()
        .find(|h| h.ghost && h.part == part && h.point == p)
        .expect("kein gedämpfter Griff")
}

fn handle(s: &AppState, c: CurveId, p: PointId) -> Option<SplineHandle> {
    let CurveKind::Spline { handles, .. } = &sk(s).curve(c).unwrap().kind else {
        panic!("kein Spline");
    };
    handles.iter().find(|h| h.at == p).copied()
}

fn pos(s: &AppState, p: PointId) -> DVec2 {
    sk(s).point_xy(p).unwrap()
}

fn begin(s: &mut AppState, at: DVec2) -> bool {
    let (se, sk) = s.session_sketch_mut().unwrap();
    se.begin_drag(sk, at, R)
}

fn drag(s: &mut AppState, at: DVec2) {
    let (se, sk) = s.session_sketch_mut().unwrap();
    se.drag_to(sk, at);
}

fn end(s: &mut AppState) {
    let (se, sk) = s.session_sketch_mut().unwrap();
    se.end_drag(sk);
    s.observe_sketch(false);
}

fn undo_len(s: &AppState) -> usize {
    s.sketch_history().unwrap().undo_len()
}

/// Linienzug der Kurve zum Vergleich der Form.
fn shape(s: &AppState, c: CurveId) -> Vec<DVec2> {
    sk(s).curve_polyline(c, 64).unwrap()
}

fn same_shape(a: &[DVec2], b: &[DVec2], tol: f64) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(p, q)| p.distance(*q) <= tol)
}

#[test]
fn selecting_spline_shows_ghost_handles() {
    let (mut s, c, pts) = wave();
    assert!(views(&s).is_empty(), "nichts gewählt, nichts zu sehen");
    select(&mut s, &[Item::Curve(c)]);
    let shown = views(&s);
    assert_eq!(shown.len(), pts.len(), "je Stützpunkt einer");
    for h in &shown {
        assert!(h.ghost);
        assert_eq!(h.part, HandlePart::Tangent);
        assert_eq!(h.spline, c);
        assert_eq!(h.at, pos(&s, h.point));
        assert!(h.tip.distance(h.at) > 0.1, "{h:?}");
    }
    // Anderes Werkzeug: Der erste Zug geht nur im Auswahlwerkzeug, gedämpft steht nichts.
    {
        let (se, sk) = s.session_sketch_mut().unwrap();
        se.set_tool(sk, Tool::Line);
    }
    select(&mut s, &[Item::Curve(c)]);
    assert!(views(&s).is_empty());
}

#[test]
fn first_drag_activates_as_one_undo_step() {
    let (mut s, c, pts) = wave();
    select(&mut s, &[Item::Curve(c)]);
    let form = shape(&s, c);
    let steps = undo_len(&s);
    let g = ghost(&s, HandlePart::Tangent, pts[2]);
    assert!(begin(&mut s, g.tip), "der Griff lässt sich greifen");
    assert!(s.sketch_session.as_ref().unwrap().is_dragging());
    // Eingeschaltet, bevor sich etwas bewegt: Die Form ist dieselbe.
    assert!(same_shape(&form, &shape(&s, c), 1e-9));
    let h = handle(&s, c, pts[2]).expect("eingeschaltet");
    assert_eq!(pos(&s, h.tip), g.tip);
    assert_eq!(
        s.sketch_session.as_ref().unwrap().selection,
        vec![Item::Curve(c)]
    );
    let to = g.tip + v(1.0, 2.0);
    drag(&mut s, to);
    drag(&mut s, to + v(1.0, 0.0));
    end(&mut s);
    assert!(pos(&s, h.tip).distance(to + v(1.0, 0.0)) < 1e-6);
    assert_eq!(
        undo_len(&s),
        steps + 1,
        "Einschalten und Zug sind ein Schritt"
    );
    assert!(!same_shape(&form, &shape(&s, c), 1e-3), "der Griff wirkt");
    assert!(s.undo_sketch());
    assert!(handle(&s, c, pts[2]).is_none());
    assert!(same_shape(&form, &shape(&s, c), 1e-12));
}

#[test]
fn esc_during_first_drag_leaves_nothing() {
    type Abort = fn(&mut AppState);
    let aborts: [(&str, Abort); 2] = [
        ("Esc", |s| {
            let (se, sk) = s.session_sketch_mut().unwrap();
            se.cancel(sk);
        }),
        ("Werkzeugwechsel", |s| {
            let (se, sk) = s.session_sketch_mut().unwrap();
            se.set_tool(sk, Tool::Line);
        }),
    ];
    for (name, abort) in aborts {
        let (mut s, c, pts) = wave();
        select(&mut s, &[Item::Curve(c)]);
        let before = sk(&s).clone();
        let steps = undo_len(&s);
        let g = ghost(&s, HandlePart::Tangent, pts[1]);
        assert!(begin(&mut s, g.tip));
        drag(&mut s, g.tip + v(3.0, 3.0));
        abort(&mut s);
        assert!(!s.sketch_session.as_ref().unwrap().is_dragging(), "{name}");
        assert!(unchanged(sk(&s), &before), "{name}: gleicher Stand");
        s.observe_sketch(false);
        assert_eq!(undo_len(&s), steps, "{name}: kein Schritt");
        assert_eq!(sk(&s).curves().count(), 2, "{name}");
    }
    // Rechtsklick: derselbe Weg, und bis alle Tasten los sind, ist nichts ein Klick.
    let (mut s, c, pts) = wave();
    select(&mut s, &[Item::Curve(c)]);
    let before = sk(&s).clone();
    let g = ghost(&s, HandlePart::Tangent, pts[1]);
    assert!(begin(&mut s, g.tip));
    let (se_, sk_) = s.session_sketch_mut().unwrap();
    assert!(se_.abort_handle_drag(sk_));
    assert!(se_.box_aborted);
    assert!(unchanged(sk(&s), &before));
    assert_eq!(
        s.sketch_session.as_ref().unwrap().selection,
        vec![Item::Curve(c)]
    );
}

/// Ein Griff, der schon an ist, und die Lage seiner Spitze.
fn with_active_tangent(c: CurveId, s: &mut AppState, at: PointId) -> (SplineHandle, DVec2) {
    select(s, &[Item::Curve(c)]);
    let g = ghost(s, HandlePart::Tangent, at);
    assert!(begin(s, g.tip));
    drag(s, g.tip + v(1.0, 1.0));
    end(s);
    let h = handle(s, c, at).expect("eingeschaltet");
    (h, pos(s, h.tip))
}

fn change_tool(s: &mut AppState) {
    let (se_, sk_) = s.session_sketch_mut().unwrap();
    se_.set_tool(sk_, Tool::Line);
    s.observe_sketch(false);
}

/// Wie beim Punktzug: Der Werkzeugwechsel übernimmt den Zug an einem Griff, der schon an ist.
#[test]
fn a_tool_change_takes_over_the_drag_on_an_active_handle() {
    let (mut s, c, pts) = wave();
    let (h, tip) = with_active_tangent(c, &mut s, pts[2]);
    let steps = undo_len(&s);
    assert!(begin(&mut s, tip));
    let to = tip + v(2.0, 1.0);
    drag(&mut s, to);
    change_tool(&mut s);
    let session = s.sketch_session.as_ref().unwrap();
    assert!(!session.is_dragging());
    assert_eq!(session.tool, Tool::Line);
    assert!(pos(&s, h.tip).distance(to) < 1e-6);
    assert_eq!(undo_len(&s), steps + 1, "ein Schritt");
    assert!(s.undo_sketch());
    assert_eq!(pos(&s, h.tip), tip);
    assert!(handle(&s, c, pts[2]).is_some(), "der Griff bleibt an");
}

/// Dasselbe, wenn der Stützpunkt seinen Griff mitnimmt.
#[test]
fn a_tool_change_takes_over_the_drag_of_a_fit_point_with_a_handle() {
    let (mut s, c, pts) = wave();
    let (h, tip) = with_active_tangent(c, &mut s, pts[2]);
    let at = pos(&s, pts[2]);
    let steps = undo_len(&s);
    assert!(begin(&mut s, at));
    drag(&mut s, at + v(1.0, 2.0));
    change_tool(&mut s);
    assert!(pos(&s, pts[2]).distance(at + v(1.0, 2.0)) < 1e-6);
    assert!(pos(&s, h.tip).distance(tip + v(1.0, 2.0)) < 1e-6);
    assert_eq!(undo_len(&s), steps + 1);
}

/// Der Rho-Griff ist immer da: Es gibt nichts einzuschalten, also ist es ein gewöhnlicher Zug.
#[test]
fn a_tool_change_takes_over_the_rho_drag() {
    let (mut s, c) = conic_sketch(0.5);
    select(&mut s, &[Item::Curve(c)]);
    let at = conic_shoulder(v(0.0, 0.0), v(10.0, 12.0), v(20.0, 0.0), 0.5);
    let steps = undo_len(&s);
    assert!(begin(&mut s, at));
    drag(&mut s, at + v(0.0, 2.4));
    change_tool(&mut s);
    assert!((rho_of(&s, c) - 0.7).abs() < 1e-9);
    assert_eq!(undo_len(&s), steps + 1);
    assert!(s.undo_sketch());
    assert_eq!(rho_of(&s, c), 0.5);
}

/// Der erste Zug schaltet erst ein und bleibt beim Werkzeugwechsel nie halb stehen, auch nicht, wenn
/// am Stützpunkt schon der Tangentengriff an ist und nur der Krümmungsgriff dazukäme.
#[test]
fn a_tool_change_during_the_curvature_ghost_leaves_the_tangent_handle_only() {
    let (mut s, c, pts) = wave();
    let (_, tip) = with_active_tangent(c, &mut s, pts[2]);
    let before = sk(&s).clone();
    let steps = undo_len(&s);
    let g = ghost(&s, HandlePart::Curvature, pts[2]);
    assert!(begin(&mut s, g.tip));
    drag(&mut s, g.tip + v(1.0, 1.0));
    change_tool(&mut s);
    assert!(unchanged(sk(&s), &before));
    assert_eq!(undo_len(&s), steps);
    assert_eq!(pos(&s, handle(&s, c, pts[2]).unwrap().tip), tip);
}

/// Esc und die rechte Taste brechen auch den Zug an einem Griff ab, der schon an ist.
#[test]
fn esc_and_right_button_still_abort_the_drag_on_an_active_handle() {
    type Abort = fn(&mut AppState);
    let aborts: [Abort; 2] = [
        |s| {
            let (se, sk) = s.session_sketch_mut().unwrap();
            se.cancel(sk);
        },
        |s| {
            let (se, sk) = s.session_sketch_mut().unwrap();
            assert!(se.abort_handle_drag(sk));
        },
    ];
    for abort in aborts {
        let (mut s, c, pts) = wave();
        let (_, tip) = with_active_tangent(c, &mut s, pts[2]);
        let before = sk(&s).clone();
        let steps = undo_len(&s);
        assert!(begin(&mut s, tip));
        drag(&mut s, tip + v(2.0, 1.0));
        abort(&mut s);
        s.observe_sketch(false);
        assert!(unchanged(sk(&s), &before));
        assert_eq!(undo_len(&s), steps);
    }
}

#[test]
fn undo_during_the_first_drag_only_takes_the_drag_back() {
    let (mut s, c, pts) = wave();
    select(&mut s, &[Item::Curve(c)]);
    let before = sk(&s).clone();
    let steps = undo_len(&s);
    let g = ghost(&s, HandlePart::Tangent, pts[1]);
    assert!(begin(&mut s, g.tip));
    drag(&mut s, g.tip + v(3.0, 3.0));
    assert!(s.undo_sketch());
    assert!(unchanged(sk(&s), &before));
    assert_eq!(undo_len(&s), steps);
}

#[test]
fn undo_removes_the_handle() {
    let (mut s, c, pts) = wave();
    select(&mut s, &[Item::Point(pts[2])]);
    let form = shape(&s, c);
    let count = sk(&s).curves().count();
    {
        let (se, sk) = s.session_sketch_mut().unwrap();
        se.toggle_tangent_handle(sk).unwrap();
    }
    s.observe_sketch(false);
    assert!(handle(&s, c, pts[2]).is_some());
    assert_eq!(sk(&s).curves().count(), count + 1);
    assert!(s.undo_sketch());
    assert!(handle(&s, c, pts[2]).is_none());
    assert_eq!(sk(&s).curves().count(), count);
    assert!(same_shape(&form, &shape(&s, c), 1e-12));
}

#[test]
fn active_handle_hides_when_deselected_unless_constrained() {
    let (mut s, c, pts) = wave();
    let (plain, with_dim) = {
        let sk = s.session_sketch_mut().unwrap().1;
        let plain = sk.activate_tangent_handle(c, pts[1]).unwrap();
        let with_dim = sk.activate_tangent_handle(c, pts[3]).unwrap();
        (plain, with_dim)
    };
    let tips = (
        handle(&s, c, pts[1]).unwrap().tip,
        handle(&s, c, pts[3]).unwrap().tip,
    );
    // Der Spline, ein Griff, sein Stützpunkt oder seine Spitze gewählt: alle aktiven zu sehen.
    let active = |s: &AppState| {
        views(s)
            .into_iter()
            .filter(|h| !h.ghost && h.part == HandlePart::Tangent)
            .count()
    };
    for item in [
        Item::Curve(c),
        Item::Curve(plain),
        Item::Point(pts[1]),
        Item::Point(pts[4]),
        Item::Point(tips.0),
    ] {
        select(&mut s, &[item]);
        assert_eq!(active(&s), 2, "{item:?}");
    }
    select(&mut s, &[]);
    assert_eq!(active(&s), 0, "abgewählt: weg");
    // Mit einem Maß des Nutzers bleibt genau dieser Griff stehen.
    {
        let sk = s.session_sketch_mut().unwrap().1;
        sk.add_constraint(K::Horizontal(planora_sketch::LineOrPoints::Line(with_dim)))
            .unwrap();
        sk.solve();
    }
    let shown = views(&s);
    assert_eq!(shown.len(), 1, "{shown:?}");
    assert_eq!(shown[0].point, pts[3]);
    assert!(!shown[0].ghost);
    let session = s.sketch_session.as_ref().unwrap();
    let visible = session.visible_handle_points(sk(&s));
    assert!(visible.contains(&tips.1) && !visible.contains(&tips.0));
    // Die interne Zeile eines Krümmungsgriffs zählt nicht als Abhängigkeit des Nutzers.
    {
        let sk = s.session_sketch_mut().unwrap().1;
        sk.activate_curvature_handle(c, pts[1]).unwrap();
    }
    assert_eq!(views(&s).len(), 1);
}

#[test]
fn hidden_handle_is_not_picked() {
    let (mut s, c, pts) = wave();
    let line = s
        .session_sketch_mut()
        .unwrap()
        .1
        .activate_tangent_handle(c, pts[2])
        .unwrap();
    let h = handle(&s, c, pts[2]).unwrap();
    let tip_at = pos(&s, h.tip);
    let mid = (tip_at + pos(&s, pts[2])) * 0.5;
    let session = s.sketch_session.as_ref().unwrap();
    // Unsichtbar: Die Spitze ist kein Punkt und die Linie keine Kurve.
    assert_eq!(session.snap(sk(&s), tip_at, 0.01), Snap::None);
    assert_ne!(session.snap(sk(&s), mid, 0.001), Snap::Curve(line));
    assert!(session.is_free_at(sk(&s), tip_at, 0.01));
    // Der Klick wählt nichts, und ein Zug greift nichts.
    {
        let (se, sk) = s.session_sketch_mut().unwrap();
        se.click(sk, tip_at, 0.01, false).unwrap();
        assert!(se.selection.is_empty());
        assert!(!se.begin_drag(sk, tip_at, 0.01));
    }
    // Der Rahmen nimmt ihn nicht.
    {
        let (se, sk) = s.session_sketch_mut().unwrap();
        se.select_rect(sk, v(-100.0, -100.0), v(100.0, 100.0), false, &|p| Some(p));
        assert!(!se.selection.contains(&Item::Curve(line)));
        assert!(se.selection.contains(&Item::Curve(c)));
    }
    // Beim Zeichnen fängt die Spitze nicht (Punkt 5: die Liste kommt aus `visible_handles`) …
    {
        let (se, sk) = s.session_sketch_mut().unwrap();
        se.selection.clear();
        se.set_tool(sk, Tool::Line);
        se.update_cursor(sk, Some(tip_at), 0.01);
        assert_ne!(se.draw_cursor.as_ref().unwrap().snap, Snap::Point(h.tip));
    }
    // … und sichtbar schon.
    {
        let (se, sk) = s.session_sketch_mut().unwrap();
        se.selection = vec![Item::Curve(c)];
        se.update_cursor(sk, Some(tip_at), 0.01);
        assert_eq!(se.draw_cursor.as_ref().unwrap().snap, Snap::Point(h.tip));
        se.set_tool(sk, Tool::Select);
    }
    // Sichtbar wählbar und greifbar.
    let session = s.sketch_session.as_ref().unwrap();
    assert_eq!(session.snap(sk(&s), tip_at, 0.01), Snap::Point(h.tip));
    assert!(begin(&mut s, tip_at));
    end(&mut s);
}

#[test]
fn tangent_button_toggles_at_selected_point() {
    let (mut s, c, pts) = wave();
    let form = shape(&s, c);
    // Ohne Stützpunkt: Meldung, nichts ändert sich.
    {
        let (se, sk) = s.session_sketch_mut().unwrap();
        let before = sk.clone();
        assert!(se.toggle_tangent_handle(sk).is_err());
        assert!(se.message.is_some());
        assert!(unchanged(sk, &before));
    }
    select(&mut s, &[Item::Point(pts[2])]);
    {
        let (se, sk) = s.session_sketch_mut().unwrap();
        se.toggle_tangent_handle(sk).unwrap();
    }
    s.observe_sketch(false);
    let h = handle(&s, c, pts[2]).expect("an");
    assert!(
        same_shape(&form, &shape(&s, c), 1e-9),
        "Einschalten ändert die Form nicht"
    );
    // Auch mit der gewählten Spitze wirkt der Knopf auf diesen Griff: Er schaltet aus.
    select(&mut s, &[Item::Point(h.tip)]);
    let steps = undo_len(&s);
    {
        let (se, sk) = s.session_sketch_mut().unwrap();
        se.toggle_tangent_handle(sk).unwrap();
        assert!(se.selection.is_empty(), "die Spitze gibt es nicht mehr");
    }
    s.observe_sketch(false);
    assert!(handle(&s, c, pts[2]).is_none());
    assert_eq!(undo_len(&s), steps + 1);
    assert!(same_shape(&form, &shape(&s, c), 1e-12));
}

#[test]
fn curvature_button_toggles_and_keeps_the_tangent_handle() {
    let (mut s, c, pts) = wave();
    select(&mut s, &[Item::Point(pts[2])]);
    let form = shape(&s, c);
    {
        let (se, sk) = s.session_sketch_mut().unwrap();
        se.toggle_curvature_handle(sk).unwrap();
    }
    s.observe_sketch(false);
    assert!(matches!(
        handle(&s, c, pts[2]).unwrap().curvature,
        HandleCurvature::Arc { .. }
    ));
    assert!(same_shape(&form, &shape(&s, c), 1e-6));
    {
        let (se, sk) = s.session_sketch_mut().unwrap();
        se.toggle_curvature_handle(sk).unwrap();
    }
    s.observe_sketch(false);
    assert_eq!(
        handle(&s, c, pts[2]).unwrap().curvature,
        HandleCurvature::Form
    );
}

/// Punkt 4: Die Ablehnung steht als Text in der Meldung, und nichts bleibt.
#[test]
fn a_refused_curvature_handle_says_why_and_changes_nothing() {
    let mut ids = None;
    let mut s = with_sketch(|sk| {
        let a = sk.add_point(-10.0, 0.0).unwrap();
        let joint = sk.add_point(0.0, 0.0).unwrap();
        let line = sk.add_line(a, joint).unwrap();
        let q: Vec<PointId> = [(10.0, 3.0), (20.0, -2.0)]
            .iter()
            .map(|&(x, y)| sk.add_point(x, y).unwrap())
            .collect();
        let sp = sk.add_spline(&[joint, q[0], q[1]]).unwrap();
        sk.add_constraint(K::Smooth(line, joint, sp, joint))
            .unwrap();
        ids = Some((sp, joint));
    });
    let (sp, joint) = ids.unwrap();
    select(&mut s, &[Item::Point(joint), Item::Curve(sp)]);
    let (se_, sk_) = s.session_sketch_mut().unwrap();
    let before = sk_.clone();
    let e = se_.toggle_curvature_handle(sk_).unwrap_err();
    assert!(e.contains(&tr!("sketch-err-handle-smooth-flat")), "{e}");
    assert_eq!(se_.message_text().as_deref(), Some(e.as_str()));
    assert!(unchanged(sk_, &before));
}

#[test]
fn curvature_ghost_on_active_handle_activates() {
    let (mut s, c, pts) = wave();
    s.session_sketch_mut()
        .unwrap()
        .1
        .activate_tangent_handle(c, pts[2])
        .unwrap();
    s.observe_sketch(false);
    select(&mut s, &[Item::Curve(c)]);
    let form = shape(&s, c);
    let steps = undo_len(&s);
    let g = ghost(&s, HandlePart::Curvature, pts[2]);
    assert!(begin(&mut s, g.tip));
    let h = handle(&s, c, pts[2]).unwrap();
    let HandleCurvature::Arc { other, center, .. } = h.curvature else {
        panic!("kein Bogen: {:?}", h.curvature);
    };
    assert!(
        same_shape(&form, &shape(&s, c), 1e-6),
        "Einschalten ändert nichts"
    );
    // Der Griff bleibt stehen, auch das Stützpunkt-Ende und die Spitze; der Zug ändert den Radius.
    let (at0, tip0, r0) = (
        pos(&s, pts[2]),
        pos(&s, h.tip),
        pos(&s, center).distance(pos(&s, pts[2])),
    );
    let inward = (pos(&s, center) - at0).normalize();
    let target = g.tip + inward * 0.5;
    drag(&mut s, target);
    drag(&mut s, target + inward * 0.2);
    end(&mut s);
    assert_eq!(undo_len(&s), steps + 1);
    assert!(pos(&s, pts[2]).distance(at0) < 1e-6);
    assert!(pos(&s, h.tip).distance(tip0) < 1e-6);
    assert!(
        pos(&s, center).distance(at0) < r0,
        "näher zur Mitte heißt kleinerer Radius"
    );
    let _ = other;
    // Danach gibt es dort keine Marke mehr, und der Bogen ist zu sehen.
    assert!(views(&s).iter().all(|g| !(g.ghost && g.point == pts[2])));
    assert!(
        views(&s)
            .iter()
            .any(|g| !g.ghost && g.part == HandlePart::Curvature && g.point == pts[2])
    );
}

#[test]
fn esc_during_the_curvature_ghost_leaves_the_tangent_handle_only() {
    let (mut s, c, pts) = wave();
    s.session_sketch_mut()
        .unwrap()
        .1
        .activate_tangent_handle(c, pts[2])
        .unwrap();
    s.observe_sketch(false);
    select(&mut s, &[Item::Curve(c)]);
    let before = sk(&s).clone();
    let g = ghost(&s, HandlePart::Curvature, pts[2]);
    assert!(begin(&mut s, g.tip));
    drag(&mut s, g.tip + v(0.3, 0.3));
    let (se_, sk_) = s.session_sketch_mut().unwrap();
    se_.cancel(sk_);
    assert!(unchanged(sk(&s), &before));
}

/// Punkt 4 im ersten Zug: Der Griff an der Linie wird abgelehnt; der Grund steht da, es läuft kein Zug.
#[test]
fn a_refused_first_drag_says_why_and_does_not_start() {
    let mut ids = None;
    let mut s = with_sketch(|sk| {
        let a = sk.add_point(-10.0, 0.0).unwrap();
        let joint = sk.add_point(0.0, 0.0).unwrap();
        let line = sk.add_line(a, joint).unwrap();
        let q: Vec<PointId> = [(10.0, 3.0), (20.0, -2.0)]
            .iter()
            .map(|&(x, y)| sk.add_point(x, y).unwrap())
            .collect();
        let sp = sk.add_spline(&[joint, q[0], q[1]]).unwrap();
        sk.add_constraint(K::Smooth(line, joint, sp, joint))
            .unwrap();
        sk.activate_tangent_handle(sp, joint).unwrap();
        sk.solve();
        ids = Some((sp, joint));
    });
    let (sp, joint) = ids.unwrap();
    s.observe_sketch(false);
    select(&mut s, &[Item::Curve(sp)]);
    let before = sk(&s).clone();
    let g = ghost(&s, HandlePart::Curvature, joint);
    assert!(!begin(&mut s, g.tip));
    let session = s.sketch_session.as_ref().unwrap();
    assert!(!session.is_dragging());
    let text = session.message_text().unwrap();
    assert!(
        text.contains(&tr!("sketch-err-handle-smooth-flat")),
        "{text}"
    );
    assert!(unchanged(sk(&s), &before));
}

/// Punkt 6: Zug am Stützpunkt nimmt Griffspitze und Krümmungsbogen starr mit.
#[test]
fn dragging_a_fit_point_carries_tip_and_arc_rigidly() {
    let (mut s, c, pts) = wave();
    s.session_sketch_mut()
        .unwrap()
        .1
        .activate_curvature_handle(c, pts[2])
        .unwrap();
    s.observe_sketch(false);
    select(&mut s, &[Item::Curve(c)]);
    let h = handle(&s, c, pts[2]).unwrap();
    let HandleCurvature::Arc { center, other, .. } = h.curvature else {
        panic!("kein Bogen");
    };
    let start: Vec<DVec2> = [pts[2], h.tip, center, other]
        .iter()
        .map(|&p| pos(&s, p))
        .collect();
    let others: Vec<DVec2> = [pts[1], pts[3]].iter().map(|&p| pos(&s, p)).collect();
    let steps = undo_len(&s);
    assert!(begin(&mut s, start[0]));
    let delta = v(1.5, -2.0);
    drag(&mut s, start[0] + delta * 0.5);
    drag(&mut s, start[0] + delta);
    end(&mut s);
    assert_eq!(undo_len(&s), steps + 1);
    for (k, &p) in [pts[2], h.tip, center, other].iter().enumerate() {
        assert!(
            pos(&s, p).distance(start[k] + delta) < 1e-6,
            "Punkt {k}: {:?} statt {:?}",
            pos(&s, p),
            start[k] + delta
        );
    }
    assert_eq!(pos(&s, pts[1]), others[0], "die Nachbarn bleiben");
    assert_eq!(pos(&s, pts[3]), others[1]);
}

/// Punkt und Griff um dieselbe Verschiebung: Was `at` wirklich schafft, gilt für alle.
fn handle_vector(s: &AppState, c: CurveId, p: PointId) -> (DVec2, DVec2, DVec2) {
    let h = handle(s, c, p).unwrap();
    let HandleCurvature::Arc { center, other, .. } = h.curvature else {
        panic!("kein Bogen");
    };
    let at = pos(s, p);
    (pos(s, h.tip) - at, pos(s, center) - at, pos(s, other) - at)
}

/// Ein fester Stützpunkt bleibt, und der Griff darf die Kurve dabei nicht verbiegen.
#[test]
fn dragging_a_fixed_fit_point_changes_nothing() {
    let (mut s, c, pts) = wave();
    {
        let sk = s.session_sketch_mut().unwrap().1;
        sk.activate_curvature_handle(c, pts[2]).unwrap();
        let at = sk.point_xy(pts[2]).unwrap();
        sk.add_constraint(K::Fixed(pts[2], at.x, at.y)).unwrap();
        sk.solve();
    }
    s.observe_sketch(false);
    select(&mut s, &[Item::Curve(c)]);
    let form = shape(&s, c);
    let vectors = handle_vector(&s, c, pts[2]);
    let at = pos(&s, pts[2]);
    let steps = undo_len(&s);
    assert!(begin(&mut s, at));
    drag(&mut s, at + v(1.5, 1.0));
    drag(&mut s, at + v(3.0, 2.0));
    end(&mut s);
    assert!(same_shape(&form, &shape(&s, c), 1e-9), "die Form bleibt");
    let now = handle_vector(&s, c, pts[2]);
    assert!(
        now.0.distance(vectors.0) < 1e-9,
        "{now:?} statt {vectors:?}"
    );
    assert_eq!(undo_len(&s), steps, "es hat sich nichts geändert");
}

/// Ein Stützpunkt, der nur waagerecht kann: Der Griff geht genau so weit mit wie er.
#[test]
fn a_fit_point_that_can_move_only_sideways_carries_only_what_it_made() {
    let (mut s, c, pts) = wave();
    {
        let sk = s.session_sketch_mut().unwrap().1;
        sk.activate_curvature_handle(c, pts[2]).unwrap();
        let at = sk.point_xy(pts[2]).unwrap();
        let q = sk.add_point(at.x - 30.0, at.y).unwrap();
        sk.add_constraint(K::Fixed(q, at.x - 30.0, at.y)).unwrap();
        sk.add_constraint(K::Horizontal(planora_sketch::LineOrPoints::Points(
            q, pts[2],
        )))
        .unwrap();
        sk.solve();
    }
    s.observe_sketch(false);
    select(&mut s, &[Item::Curve(c)]);
    let vectors = handle_vector(&s, c, pts[2]);
    let at = pos(&s, pts[2]);
    // Nur hoch: nichts geht.
    let form = shape(&s, c);
    assert!(begin(&mut s, at));
    drag(&mut s, at + v(0.0, 4.0));
    end(&mut s);
    assert!(pos(&s, pts[2]).distance(at) < 1e-9);
    assert!(same_shape(&form, &shape(&s, c), 1e-9), "die Form bleibt");
    // Schräg: Der Punkt schafft nur den waagerechten Teil, der Griff folgt ihm.
    assert!(begin(&mut s, at));
    drag(&mut s, at + v(3.0, 4.0));
    end(&mut s);
    assert!(pos(&s, pts[2]).distance(at + v(3.0, 0.0)) < 1e-3);
    let now = handle_vector(&s, c, pts[2]);
    assert!(
        now.0.distance(vectors.0) < 1e-6,
        "{now:?} statt {vectors:?}"
    );
    assert!(
        now.1.distance(vectors.1) < 1e-6,
        "{now:?} statt {vectors:?}"
    );
    assert!(
        now.2.distance(vectors.2) < 1e-6,
        "{now:?} statt {vectors:?}"
    );
}

/// Der Lauf mit gehaltenen Punkten darf die Zahlen nach dem Zug nicht färben.
#[test]
fn a_handle_drag_keeps_freedom_and_free_points() {
    let free = |s: &AppState| {
        let r = s
            .sketch_session
            .as_ref()
            .unwrap()
            .last_report
            .clone()
            .unwrap();
        let mut f = r.free_points.clone();
        f.sort();
        (r.dof, f)
    };
    let (mut s, c, pts) = wave();
    {
        let (se_, sk_) = s.session_sketch_mut().unwrap();
        sk_.activate_curvature_handle(c, pts[2]).unwrap();
        se_.last_report = Some(sk_.solve());
    }
    s.observe_sketch(false);
    select(&mut s, &[Item::Curve(c)]);
    let before = free(&s);
    let h = handle(&s, c, pts[2]).unwrap();
    let HandleCurvature::Arc { other, .. } = h.curvature else {
        panic!("kein Bogen");
    };
    for grabbed in [h.tip, other] {
        let at = pos(&s, grabbed);
        assert!(begin(&mut s, at));
        drag(&mut s, at + v(0.4, 0.5));
        end(&mut s);
        assert_eq!(free(&s), before, "nach dem Zug an {grabbed:?}");
        let fresh = sk(&s).clone().solve();
        assert_eq!(fresh.dof, before.0);
    }
}

/// Auch Griff und Stützpunkt leuchten, solange sie gezogen werden.
#[test]
fn dragged_handle_points_light_up() {
    let (mut s, c, pts) = wave();
    s.session_sketch_mut()
        .unwrap()
        .1
        .activate_tangent_handle(c, pts[2])
        .unwrap();
    s.observe_sketch(false);
    select(&mut s, &[Item::Curve(c)]);
    let h = handle(&s, c, pts[2]).unwrap();
    let lit = planora_app::theme::sketch_colors(s.dark).selected;
    let color = |s: &AppState, p| planora_app::sketch_view::point_color(s, p);
    assert_ne!(color(&s, h.tip), lit);
    let tip_at = pos(&s, h.tip);
    assert!(begin(&mut s, tip_at));
    assert_eq!(color(&s, h.tip), lit, "die gezogene Spitze");
    assert_ne!(color(&s, pts[2]), lit, "der gehaltene Stützpunkt nicht");
    end(&mut s);
    assert_ne!(color(&s, h.tip), lit, "nach dem Zug nicht mehr");
    let at = pos(&s, pts[2]);
    assert!(begin(&mut s, at));
    assert_eq!(color(&s, pts[2]), lit, "der gezogene Stützpunkt");
    end(&mut s);
}

/// Eine ausgeschaltete Abhängigkeit steht weiter in der Zeichnung und will gewählt werden.
#[test]
fn a_disabled_constraint_keeps_the_handle_visible() {
    let (mut s, c, pts) = wave();
    {
        let sk = s.session_sketch_mut().unwrap().1;
        let line = sk.activate_tangent_handle(c, pts[2]).unwrap();
        let k = sk
            .add_constraint(K::Horizontal(planora_sketch::LineOrPoints::Line(line)))
            .unwrap();
        sk.set_enabled(k, false).unwrap();
    }
    assert_eq!(views(&s).len(), 1);
    let session = s.sketch_session.as_ref().unwrap();
    assert!(session.hidden_handles(sk(&s)).curves.is_empty());
    assert_eq!(session.visible_handle_points(sk(&s)).len(), 1);
}

/// Der Rasterfang bindet nicht an eine unsichtbare Spitze, die zufällig auf dem Rasterpunkt liegt.
#[test]
fn the_grid_snap_does_not_bind_a_hidden_tip() {
    let (mut s, c, pts) = wave();
    {
        let sk = s.session_sketch_mut().unwrap().1;
        sk.activate_tangent_handle(c, pts[2]).unwrap();
    }
    let tip = handle(&s, c, pts[2]).unwrap().tip;
    let node = v(25.0, -2.0);
    let (se_, sk_) = s.session_sketch_mut().unwrap();
    sk_.drag(&[planora_sketch::solver::Drag {
        point: tip,
        target: node,
    }])
    .unwrap();
    se_.set_tool(sk_, Tool::Line);
    se_.cursor_opts.grid_step = Some(1.0);
    let raw = node + v(0.02, 0.02);
    se_.update_cursor(sk_, Some(raw), 0.001);
    assert_eq!(se_.draw_cursor.as_ref().unwrap().snap, Snap::Grid);
    se_.selection = vec![Item::Curve(c)];
    se_.update_cursor(sk_, Some(raw), 0.001);
    assert_eq!(se_.draw_cursor.as_ref().unwrap().snap, Snap::Point(tip));
}

/// Ein Zug am Griff ist ein Griffzug: An der Linie zieht er die Spitze, am Bogen sein freies Ende,
/// und der Stützpunkt bleibt.
#[test]
fn grabbing_the_handle_line_moves_only_the_tip() {
    let (mut s, c, pts) = wave();
    let line = s
        .session_sketch_mut()
        .unwrap()
        .1
        .activate_tangent_handle(c, pts[2])
        .unwrap();
    s.observe_sketch(false);
    select(&mut s, &[Item::Curve(c)]);
    let h = handle(&s, c, pts[2]).unwrap();
    let (at0, tip0) = (pos(&s, pts[2]), pos(&s, h.tip));
    let grab = at0 * 0.5 + tip0 * 0.5;
    let (se_, sk_) = s.session_sketch_mut().unwrap();
    assert_eq!(se_.snap(sk_, grab, 0.01), Snap::Curve(line));
    assert!(se_.begin_drag(sk_, grab, 0.01));
    se_.drag_to(sk_, grab + v(0.5, 1.0));
    se_.end_drag(sk_);
    assert_eq!(pos(&s, pts[2]), at0);
    assert!(pos(&s, h.tip).distance(tip0 + v(0.5, 1.0)) < 1e-6);
}

#[test]
fn drag_handle_does_not_jump_free_parts() {
    let (mut s, c, pts) = wave();
    s.session_sketch_mut()
        .unwrap()
        .1
        .activate_curvature_handle(c, pts[2])
        .unwrap();
    s.observe_sketch(false);
    select(&mut s, &[Item::Curve(c)]);
    let h = handle(&s, c, pts[2]).unwrap();
    let keep: Vec<(PointId, DVec2)> = sk(&s)
        .points()
        .map(|(p, _)| p)
        .filter(|&p| sk(&s).handle_of_point(p).is_none())
        .map(|p| (p, pos(&s, p)))
        .collect();
    let line_shape: Vec<Vec<DVec2>> = sk(&s)
        .curves()
        .filter(|(k, d)| matches!(d.kind, CurveKind::Line { .. }) && !sk(&s).is_handle_curve(*k))
        .map(|(k, _)| shape(&s, k))
        .collect();
    let tip0 = pos(&s, h.tip);
    assert!(begin(&mut s, tip0));
    // Viele kleine Schritte wie die Maus, um den Griff herum und weiter hinaus.
    for k in 1..=40 {
        let w = f64::from(k) / 40.0;
        drag(&mut s, tip0 + v(4.0 * w, 6.0 * w * w));
        for &(p, at) in &keep {
            assert!(
                pos(&s, p).distance(at) < 1e-6,
                "Schritt {k}: {p:?} sprang von {at:?} nach {:?}",
                pos(&s, p)
            );
        }
    }
    end(&mut s);
    assert!(pos(&s, h.tip).distance(tip0 + v(4.0, 6.0)) < 1e-5);
    for (k, line) in sk(&s)
        .curves()
        .filter(|(k, d)| matches!(d.kind, CurveKind::Line { .. }) && !sk(&s).is_handle_curve(*k))
        .map(|(k, _)| k)
        .enumerate()
    {
        assert_eq!(shape(&s, line), line_shape[k]);
    }
}

fn conic_sketch(rho: f64) -> (AppState, CurveId) {
    let mut out = None;
    let s = with_sketch(|sk| {
        let p = sk.add_point(0.0, 0.0).unwrap();
        let q = sk.add_point(20.0, 0.0).unwrap();
        let a = sk.add_point(10.0, 12.0).unwrap();
        out = Some(sk.add_conic(p, q, a, rho).unwrap());
    });
    (s, out.unwrap())
}

fn rho_of(s: &AppState, c: CurveId) -> f64 {
    match sk(s).curve(c).unwrap().kind {
        CurveKind::Conic { rho, .. } => rho,
        _ => panic!("kein Kegelschnitt"),
    }
}

#[test]
fn rho_grip_changes_rho_one_undo_step() {
    let (mut s, c) = conic_sketch(0.5);
    let session = s.sketch_session.as_ref().unwrap();
    assert!(session.rho_grips(sk(&s)).is_empty(), "ungewählt kein Griff");
    select(&mut s, &[Item::Curve(c)]);
    let session = s.sketch_session.as_ref().unwrap();
    let grips = session.rho_grips(sk(&s));
    assert_eq!(grips.len(), 1);
    let at = conic_shoulder(v(0.0, 0.0), v(10.0, 12.0), v(20.0, 0.0), 0.5);
    assert!(grips[0].1.distance(at) < 1e-12);
    let steps = undo_len(&s);
    assert!(begin(&mut s, at));
    // Der Griff lebt auf M–A: Eine Bewegung quer dazu ändert nichts.
    drag(&mut s, at + v(3.0, 0.0));
    assert!((rho_of(&s, c) - 0.5).abs() < 1e-12);
    drag(&mut s, at + v(0.0, 1.2));
    drag(&mut s, at + v(0.0, 2.4));
    assert!((rho_of(&s, c) - 0.7).abs() < 1e-9, "{}", rho_of(&s, c));
    end(&mut s);
    assert_eq!(undo_len(&s), steps + 1);
    assert!(s.undo_sketch());
    assert_eq!(rho_of(&s, c), 0.5);
    // Abbruch lässt nichts.
    select(&mut s, &[Item::Curve(c)]);
    let before = sk(&s).clone();
    assert!(begin(&mut s, at));
    drag(&mut s, at + v(0.0, 3.0));
    let (se_, sk_) = s.session_sketch_mut().unwrap();
    se_.cancel(sk_);
    assert!(unchanged(sk(&s), &before));
    // Zurück auf den Anfang ist kein Schritt.
    let steps = undo_len(&s);
    select(&mut s, &[Item::Curve(c)]);
    assert!(begin(&mut s, at));
    drag(&mut s, at + v(0.0, 3.0));
    drag(&mut s, at);
    end(&mut s);
    assert_eq!(undo_len(&s), steps);
}

#[test]
fn rho_grip_stays_in_range() {
    let (mut s, c) = conic_sketch(0.5);
    select(&mut s, &[Item::Curve(c)]);
    let at = conic_shoulder(v(0.0, 0.0), v(10.0, 12.0), v(20.0, 0.0), 0.5);
    assert!(begin(&mut s, at));
    drag(&mut s, at + v(0.0, 500.0));
    assert_eq!(rho_of(&s, c), 0.999);
    drag(&mut s, at + v(0.0, -500.0));
    assert_eq!(rho_of(&s, c), 0.001);
    end(&mut s);
}

#[test]
fn a_rho_outside_the_range_is_refused_with_a_message() {
    let (mut s, c) = conic_sketch(0.5);
    let (se_, sk_) = s.session_sketch_mut().unwrap();
    for bad in [0.0, 1.0, -0.5, 0.0000001, f64::NAN] {
        let e = se_.set_conic_rho(sk_, c, bad).unwrap_err();
        assert_eq!(e, tr!("sketch-err-conic-rho"));
    }
    se_.set_conic_rho(sk_, c, 0.000001).unwrap();
    se_.set_conic_rho(sk_, c, 0.25).unwrap();
    assert_eq!(rho_of(&s, c), 0.25);
}

/// Annahme aus Task 5 geprüft: Rho an Vorlage oder Exemplar löst das Muster auf, mit Meldung.
#[test]
fn changing_rho_of_a_pattern_source_dissolves_the_pattern_with_a_note() {
    let (mut s, c) = conic_sketch(0.5);
    let plan = PatternPlan {
        kind: PatternKind::Rect {
            dirs: [PatternDir::SketchX, PatternDir::Perpendicular],
            extent: [false, false],
            symmetric: [false, false],
        },
        sources: vec![c],
        counts: [3, 1],
        values: [40.0, 40.0],
        suppressed: Vec::new(),
    };
    let (se_, sk_) = s.session_sketch_mut().unwrap();
    sk_.add_pattern(&plan).unwrap();
    assert_eq!(sk_.pattern_count(), 1);
    se_.set_conic_rho(sk_, c, 0.25).unwrap();
    assert_eq!(sk_.pattern_count(), 0);
    assert!(
        se_.message_text()
            .unwrap()
            .contains(&planora_i18n::tr!("sketch-pattern-dissolved", n = 1_usize)),
        "{:?}",
        se_.message_text()
    );
}

/// Punkt 3: Die Oberfläche sieht als Vorlage den Spline, nie seine Griff-Kurven.
#[test]
fn a_handle_in_a_pattern_counts_as_its_spline() {
    let (mut s, c, pts) = wave();
    let line = s
        .session_sketch_mut()
        .unwrap()
        .1
        .activate_tangent_handle(c, pts[2])
        .unwrap();
    s.observe_sketch(false);
    // Neu: Der gewählte Griff steht für seinen Spline.
    select(&mut s, &[Item::Curve(line)]);
    let (se_, sk_) = s.session_sketch_mut().unwrap();
    se_.choose_kind(sk_, ToolKind::PatternRect);
    assert_eq!(se_.pattern.plan.as_ref().unwrap().sources, vec![c]);
    se_.set_tool(sk_, Tool::Select);
    // Bestehend: Doppelklick auf den Griff öffnet das Muster mit dem Spline als Vorlage.
    let plan = PatternPlan {
        kind: PatternKind::Rect {
            dirs: [PatternDir::SketchX, PatternDir::Perpendicular],
            extent: [false, false],
            symmetric: [false, false],
        },
        sources: vec![c],
        counts: [2, 1],
        values: [60.0, 60.0],
        suppressed: Vec::new(),
    };
    sk_.add_pattern(&plan).unwrap();
    assert!(se_.begin_pattern_edit_at(sk_, line));
    assert_eq!(se_.pattern.plan.as_ref().unwrap().sources, vec![c]);
}

/// Auch am Exemplar: Rho löst das Muster auf, die Meldung sagt es, und ein Fehler ändert nichts.
#[test]
fn changing_rho_of_a_pattern_instance_dissolves_the_pattern_with_a_note() {
    let (mut s, c) = conic_sketch(0.5);
    let plan = PatternPlan {
        kind: PatternKind::Rect {
            dirs: [PatternDir::SketchX, PatternDir::Perpendicular],
            extent: [false, false],
            symmetric: [false, false],
        },
        sources: vec![c],
        counts: [3, 1],
        values: [40.0, 40.0],
        suppressed: Vec::new(),
    };
    let (se_, sk_) = s.session_sketch_mut().unwrap();
    let id = sk_.add_pattern(&plan).unwrap();
    let instance = sk_
        .pattern(id)
        .unwrap()
        .instances
        .iter()
        .find_map(|i| i.curves.first().copied().filter(|k| *k != c))
        .expect("ein Exemplar");
    assert!(se_.set_conic_rho(sk_, instance, 1.5).is_err());
    assert_eq!(sk_.pattern_count(), 1, "abgelehnt: das Muster bleibt");
    se_.set_conic_rho(sk_, instance, 0.25).unwrap();
    assert_eq!(sk_.pattern_count(), 0);
    let text = se_.message_text().unwrap();
    assert!(
        text.contains(&planora_i18n::tr!("sketch-pattern-dissolved", n = 1_usize)),
        "{text}"
    );
    assert_eq!(rho_of(&s, instance), 0.25);
    assert_eq!(rho_of(&s, c), 0.5, "die Vorlage behält ihr Rho");
}

#[test]
fn a_handle_on_a_pattern_dissolves_it_with_a_note() {
    let (mut s, c, pts) = wave();
    let plan = PatternPlan {
        kind: PatternKind::Rect {
            dirs: [PatternDir::SketchX, PatternDir::Perpendicular],
            extent: [false, false],
            symmetric: [false, false],
        },
        sources: vec![c],
        counts: [2, 1],
        values: [60.0, 60.0],
        suppressed: Vec::new(),
    };
    s.session_sketch_mut()
        .unwrap()
        .1
        .add_pattern(&plan)
        .unwrap();
    s.observe_sketch(false);
    select(&mut s, &[Item::Curve(c)]);
    let g = ghost(&s, HandlePart::Tangent, pts[2]);
    assert!(begin(&mut s, g.tip));
    drag(&mut s, g.tip + v(1.0, 1.0));
    end(&mut s);
    let session = s.sketch_session.as_ref().unwrap();
    assert_eq!(sk(&s).pattern_count(), 0);
    assert!(
        session
            .message_text()
            .unwrap()
            .contains(&planora_i18n::tr!("sketch-pattern-dissolved", n = 1_usize)),
        "{:?}",
        session.message_text()
    );
}

fn control_sketch(xy: &[(f64, f64)]) -> (AppState, CurveId) {
    let mut out = None;
    let s = with_sketch(|sk| {
        let pts: Vec<PointId> = xy
            .iter()
            .map(|&(x, y)| sk.add_point(x, y).unwrap())
            .collect();
        out = Some(sk.add_control_spline(&pts, 5).unwrap());
    });
    (s, out.unwrap())
}

fn control_points(s: &AppState, c: CurveId) -> (Vec<PointId>, u8) {
    match &sk(s).curve(c).unwrap().kind {
        CurveKind::ControlSpline { points, degree, .. } => (points.clone(), *degree),
        _ => panic!("kein Kontrollpunkt-Spline"),
    }
}

#[test]
fn insert_control_point_by_click_one_undo_step() {
    let xy = [
        (0.0, 0.0),
        (10.0, 12.0),
        (20.0, -8.0),
        (30.0, 12.0),
        (40.0, -8.0),
        (50.0, 12.0),
        (60.0, 0.0),
    ];
    let (mut s, c) = control_sketch(&xy);
    select(&mut s, &[Item::Curve(c)]);
    let samples = |s: &AppState| -> Vec<DVec2> {
        let (lo, hi) = sk(s).curve_domain(c).unwrap();
        (0..=40)
            .map(|k| {
                sk(s)
                    .curve_eval(c, lo + (hi - lo) * f64::from(k) / 40.0)
                    .unwrap()
            })
            .collect()
    };
    let form = samples(&s);
    let (before, _) = control_points(&s, c);
    let steps = undo_len(&s);
    let at = sk(&s).curve_eval(c, 1.7).unwrap();
    {
        let (se, sk) = s.session_sketch_mut().unwrap();
        se.choose_tool(sk, Tool::InsertControlPoint);
        se.click(sk, at, R, false).unwrap();
        // Das Werkzeug bleibt; die Kurve bleibt gewählt.
        assert_eq!(se.tool, Tool::InsertControlPoint);
        assert_eq!(se.selection, vec![Item::Curve(c)]);
    }
    s.observe_sketch(false);
    let (after, degree) = control_points(&s, c);
    assert_eq!(after.len(), before.len() + 1);
    assert_eq!(degree, 5);
    assert!(same_shape(&form, &samples(&s), 1e-9), "die Form bleibt");
    assert_eq!(undo_len(&s), steps + 1, "ein Schritt");
    assert_eq!(
        s.sketch_undo_label().as_deref(),
        Some(tr!("undo-control-point-insert").as_str())
    );
    // Neben die Kurve geklickt: nichts.
    {
        let (se, sk) = s.session_sketch_mut().unwrap();
        se.click(sk, v(500.0, 500.0), R, false).unwrap();
    }
    s.observe_sketch(false);
    assert_eq!(undo_len(&s), steps + 1);
    assert!(s.undo_sketch());
    assert_eq!(control_points(&s, c).0, before);
    // Esc beendet das Werkzeug und lässt nichts.
    let (se_, sk_) = s.session_sketch_mut().unwrap();
    se_.choose_tool(sk_, Tool::InsertControlPoint);
    let b = sk_.clone();
    se_.cancel(sk_);
    assert_eq!(se_.tool, Tool::Select);
    assert!(unchanged(sk_, &b));
}

#[test]
fn insert_means_the_control_spline_beside_a_nearer_line() {
    let (mut s, c) = control_sketch(&[(0.0, 0.0), (10.0, 12.0), (20.0, -8.0), (30.0, 6.0)]);
    let at = sk(&s).curve_eval(c, 0.5).unwrap();
    let (se_, sk_) = s.session_sketch_mut().unwrap();
    // Eine Linie liegt näher an der Klickstelle: Der Befehl meint trotzdem die Kurve in Reichweite.
    let a = sk_.add_point(at.x - 1.0, at.y + 0.01).unwrap();
    let b = sk_.add_point(at.x + 1.0, at.y + 0.01).unwrap();
    sk_.add_line(a, b).unwrap();
    se_.choose_tool(sk_, Tool::InsertControlPoint);
    se_.click(sk_, at + v(0.0, 0.01), R, false).unwrap();
    let (points, _) = control_points(&s, c);
    assert_eq!(points.len(), 5);
}

#[test]
fn degree_action_switches_between_three_and_five() {
    let xy = [
        (0.0, 0.0),
        (10.0, 12.0),
        (20.0, -8.0),
        (30.0, 12.0),
        (40.0, -8.0),
        (50.0, 12.0),
        (60.0, 0.0),
    ];
    let (mut s, c) = control_sketch(&xy);
    // Ohne Auswahl: Meldung.
    {
        let (se, sk) = s.session_sketch_mut().unwrap();
        assert!(se.cycle_spline_degree(sk).is_err());
        assert!(se.message.is_some());
    }
    select(&mut s, &[Item::Curve(c)]);
    let steps = undo_len(&s);
    {
        let (se, sk) = s.session_sketch_mut().unwrap();
        se.cycle_spline_degree(sk).unwrap();
    }
    s.observe_sketch(false);
    assert_eq!(control_points(&s, c).1, 3);
    assert_eq!(undo_len(&s), steps + 1);
    {
        let (se, sk) = s.session_sketch_mut().unwrap();
        se.set_spline_degree(sk, c, 5).unwrap();
    }
    s.observe_sketch(false);
    assert_eq!(control_points(&s, c).1, 5);
    // Derselbe Grad: kein Schritt.
    let steps = undo_len(&s);
    {
        let (se, sk) = s.session_sketch_mut().unwrap();
        se.set_spline_degree(sk, c, 5).unwrap();
    }
    s.observe_sketch(false);
    assert_eq!(undo_len(&s), steps);
}

#[test]
fn the_four_actions_are_bindable_without_a_default_key() {
    let keymap = Keymap::default();
    for (action, id) in [
        (Action::SketchTangentHandle, "skizze.tangentengriff"),
        (Action::SketchCurvatureHandle, "skizze.kruemmungsgriff"),
        (
            Action::SketchInsertControlPoint,
            "skizze.kontrollpunkt_einfuegen",
        ),
        (Action::SketchSplineDegree, "skizze.spline_grad"),
    ] {
        assert_eq!(action.id(), id);
        assert!(
            keymap.bindings(action).is_empty(),
            "{id} hat eine Vorgabetaste"
        );
    }
    with_lang(&german(), || {
        assert_eq!(Action::SketchTangentHandle.label(), "Tangentengriff");
        assert_eq!(Action::SketchCurvatureHandle.label(), "Krümmungsgriff");
        assert_eq!(
            Action::SketchInsertControlPoint.label(),
            "Kontrollpunkt einfügen"
        );
    });
    with_lang(&english(), || {
        assert_eq!(Action::SketchTangentHandle.label(), "Tangent Handle");
        assert_eq!(Action::SketchCurvatureHandle.label(), "Curvature Handle");
        assert_eq!(
            Action::SketchInsertControlPoint.label(),
            "Insert Control Point"
        );
    });
}

// ---- Ganze App bei 1920×1080 ---------------------------------------------------------------------

const SIZE: Vec2 = vec2(1920.0, 1080.0);
const NONE: Modifiers = Modifiers::NONE;

fn key(k: Key) -> Event {
    Event::Key {
        key: k,
        physical_key: Some(k),
        pressed: true,
        repeat: false,
        modifiers: NONE,
    }
}

fn pointer(p: Pos2, b: PointerButton, pressed: bool) -> Event {
    Event::PointerButton {
        pos: p,
        button: b,
        pressed,
        modifiers: NONE,
    }
}

struct Rig {
    ctx: egui::Context,
    app: PlanoraApp,
    time: f64,
}

impl Rig {
    fn new(build: impl FnOnce(&mut Sketch)) -> Rig {
        let root = common::temp_root("planora-sketch-splines-handles");
        let mut w = Workspace::new(Some(Library::open(&root).unwrap()), Config::default());
        w.config_path = root.join("config.toml");
        let s = &mut w.active_tab_mut().state;
        let id = s
            .add_sketch(SketchPlane::Standard(StandardPlane::XY))
            .unwrap();
        s.open_sketch(id).unwrap();
        {
            let sk = s.session_sketch_mut().unwrap().1;
            build(sk);
            sk.solve();
        }
        s.observe_sketch(false);
        w.observe_history(false);
        let ctx = egui::Context::default();
        ctx.set_fonts(fonts::definitions());
        theme::apply(&ctx, true);
        PlanoraApp::setup_context(&ctx);
        let mut rig = Rig {
            ctx,
            app: PlanoraApp::for_tests(w),
            time: 0.0,
        };
        rig.settle();
        rig
    }

    fn tab(&mut self) -> &mut DocTab {
        self.app.ws_mut().active_tab_mut()
    }

    fn st(&mut self) -> &mut AppState {
        &mut self.tab().state
    }

    fn se(&mut self) -> &mut SketchSession {
        self.st().sketch_session.as_mut().expect("Skizze offen")
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
        self.frame(vec![]);
        self.frame(vec![]);
        self.frame(vec![]);
    }

    fn click(&mut self, p: Pos2) {
        self.time += 1.0;
        self.frame(vec![Event::PointerMoved(p)]);
        self.frame(vec![pointer(p, PointerButton::Primary, true)]);
        self.frame(vec![pointer(p, PointerButton::Primary, false)]);
        self.settle();
    }

    fn select(&mut self, items: &[Item]) {
        self.se().selection = items.to_vec();
        self.settle();
    }

    fn undo_len(&mut self) -> usize {
        self.st().sketch_history().unwrap().undo_len()
    }

    /// Die Skizzenstelle unter dem Zeiger bei `p`.
    fn raw_at(&mut self, p: Pos2) -> DVec2 {
        self.frame(vec![Event::PointerMoved(p)]);
        self.se().cursor.expect("Zeiger auf der Skizze")
    }

    /// Bildschirmlage der Skizzenstelle `at`: Die Ansicht von vorn bildet die Ebene affin ab.
    fn screen(&mut self, at: DVec2) -> Pos2 {
        let p0 = Pos2::new(900.0, 500.0);
        let r0 = self.raw_at(p0);
        let rx = self.raw_at(p0 + vec2(100.0, 0.0)) - r0;
        let ry = self.raw_at(p0 + vec2(0.0, 100.0)) - r0;
        let d = at - r0;
        let det = rx.x * ry.y - rx.y * ry.x;
        let a = (d.x * ry.y - d.y * ry.x) / det;
        let b = (rx.x * d.y - rx.y * d.x) / det;
        p0 + vec2(a as f32 * 100.0, b as f32 * 100.0)
    }
}

/// Zahl der Kreise des letzten Bilds: Jeder Punkt der Skizze ist einer.
fn shape_count(shape: &egui::epaint::Shape) -> usize {
    match shape {
        egui::epaint::Shape::Vec(v) => v.iter().map(shape_count).sum(),
        egui::epaint::Shape::Circle(_) => 1,
        _ => 0,
    }
}

/// Ruft `f` für jede Ecke einer Linie oder eines Linienzugs.
fn stroke_points(shape: &egui::epaint::Shape, f: &mut impl FnMut(Pos2)) {
    use egui::epaint::Shape as S;
    match shape {
        S::Vec(v) => v.iter().for_each(|x| stroke_points(x, f)),
        S::LineSegment { points, .. } => points.iter().for_each(|p| f(*p)),
        S::Path(path) => path.points.iter().for_each(|p| f(*p)),
        _ => {}
    }
}

impl Rig {
    /// Zahl der Linienecken im letzten Bild, die auf der Strecke `from`–`to` liegen.
    fn strokes_on(&mut self, from: Pos2, to: Pos2) -> usize {
        // Der Zeiger aus `screen` darf nichts hervorheben.
        self.frame(vec![Event::PointerMoved(Pos2::new(1400.0, 900.0))]);
        self.settle();
        self.time += 1.0 / 60.0;
        let raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, SIZE)),
            time: Some(self.time),
            ..Default::default()
        };
        let mut f = eframe::Frame::_new_kittest();
        let app = &mut self.app;
        let out = self.ctx.run_ui(raw, |ui| app.ui(ui, &mut f));
        let mut n = 0;
        let along = to - from;
        for c in &out.shapes {
            stroke_points(&c.shape, &mut |p| {
                let t = (p - from).dot(along) / along.length_sq();
                if (0.0..=1.0).contains(&t) && (from + along * t).distance(p) < 0.7 {
                    n += 1;
                }
            });
        }
        out.drop_without_applying_deltas();
        n
    }

    fn drawn(&mut self) -> usize {
        self.settle();
        self.time += 1.0 / 60.0;
        let raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, SIZE)),
            time: Some(self.time),
            ..Default::default()
        };
        let mut f = eframe::Frame::_new_kittest();
        let app = &mut self.app;
        let out = self.ctx.run_ui(raw, |ui| app.ui(ui, &mut f));
        let n = out.shapes.iter().map(|c| shape_count(&c.shape)).sum();
        out.drop_without_applying_deltas();
        n
    }
}

/// Unsichtbar heißt auch: nicht gezeichnet. Ein Griff ohne Auswahl ändert das Bild nicht; mit Auswahl
/// sind er und die gedämpften Griffe da.
#[test]
fn a_hidden_handle_is_not_drawn() {
    let mut ids = None;
    let mut rig = Rig::new(|sk| {
        ids = Some(add_spline(sk, &WAVE));
    });
    let (c, pts) = ids.unwrap();
    let plain = rig.drawn();
    rig.st()
        .session_sketch_mut()
        .unwrap()
        .1
        .activate_tangent_handle(c, pts[2])
        .unwrap();
    assert_eq!(rig.drawn(), plain, "unsichtbar");
    let h = {
        let st = rig.st();
        let CurveKind::Spline { handles, .. } =
            &st.session_sketch().unwrap().curve(c).unwrap().kind
        else {
            panic!("kein Spline");
        };
        handles[0]
    };
    let (at, tip) = {
        let sk = rig.st().session_sketch().unwrap();
        (sk.point_xy(h.at).unwrap(), sk.point_xy(h.tip).unwrap())
    };
    // Die äußere Hälfte der Griff-Linie: Dort läuft keine andere Kurve.
    let (from, to) = (rig.screen((at + tip) * 0.5), rig.screen(tip));
    assert_eq!(
        rig.strokes_on(from, to),
        0,
        "die Linie wird nicht gezeichnet"
    );
    rig.select(&[Item::Curve(c)]);
    let (from, to) = (rig.screen((at + tip) * 0.5), rig.screen(tip));
    assert!(rig.strokes_on(from, to) > 0, "gewählt zu sehen");
    assert!(rig.drawn() > plain, "gewählt zu sehen");
}

/// Eine fertige Skizze hat keine Auswahl: nur Griffe mit einer Abhängigkeit des Nutzers sind zu sehen.
#[test]
fn a_finished_sketch_shows_only_constrained_handles() {
    let rect = Rect::from_min_size(Pos2::ZERO, SIZE);
    let shapes = |with_handle: bool, constrained: bool| {
        let (mut s, c, pts) = wave();
        {
            let sk = s.session_sketch_mut().unwrap().1;
            if with_handle {
                let line = sk.activate_tangent_handle(c, pts[2]).unwrap();
                if constrained {
                    sk.add_constraint(K::Horizontal(planora_sketch::LineOrPoints::Line(line)))
                        .unwrap();
                }
            }
            sk.solve();
        }
        s.observe_sketch(false);
        s.close_sketch();
        planora_app::sketch_view::finished_sketch_shapes(&s, rect).len()
    };
    let plain = shapes(false, false);
    assert!(plain > 0);
    assert_eq!(shapes(true, false), plain, "ohne Abhängigkeit unsichtbar");
    assert!(shapes(true, true) > plain, "mit Abhängigkeit zu sehen");
}

#[test]
fn rho_field_in_panel() {
    let mut conic = None;
    let mut rig = Rig::new(|sk| {
        let p = sk.add_point(0.0, 0.0).unwrap();
        let q = sk.add_point(20.0, 0.0).unwrap();
        let a = sk.add_point(10.0, 12.0).unwrap();
        conic = Some(sk.add_conic(p, q, a, 0.5).unwrap());
    });
    let c = conic.unwrap();
    assert!(
        sketch_panel::rho_field(&rig.ctx).is_none(),
        "ohne Wahl kein Feld"
    );
    rig.select(&[Item::Curve(c)]);
    let field = sketch_panel::rho_field(&rig.ctx).expect("Rho-Feld");
    assert!(field.width() > 0.0);
    let steps = rig.undo_len();
    let type_value = |rig: &mut Rig, text: &str| {
        rig.click(field.center());
        rig.frame(vec![Event::Key {
            key: Key::A,
            physical_key: Some(Key::A),
            pressed: true,
            repeat: false,
            modifiers: Modifiers::COMMAND,
        }]);
        rig.frame(vec![Event::Text(text.to_string())]);
        rig.frame(vec![key(Key::Enter)]);
        rig.settle();
    };
    type_value(&mut rig, "0,25");
    let rho = |rig: &mut Rig| match rig.st().session_sketch().unwrap().curve(c).unwrap().kind {
        CurveKind::Conic { rho, .. } => rho,
        _ => panic!("kein Kegelschnitt"),
    };
    assert_eq!(rho(&mut rig), 0.25);
    assert_eq!(rig.undo_len(), steps + 1, "ein Schritt");
    // Außerhalb: abgelehnt mit Meldung, kein Schritt.
    type_value(&mut rig, "1,5");
    assert_eq!(rho(&mut rig), 0.25);
    assert_eq!(rig.undo_len(), steps + 1);
    let text = rig.se().message_text().expect("Meldung");
    assert_eq!(text, tr!("sketch-err-conic-rho"));
    // Rho nimmt keine Formel.
    type_value(&mut rig, "1/4 + 0,1");
    assert_eq!(rho(&mut rig), 0.25);
    rig.select(&[]);
    assert!(sketch_panel::rho_field(&rig.ctx).is_none());
}

/// Verschwindet das Feld mit einem Entwurf, kommt der nicht wieder: Sonst stünde beim nächsten Wählen
/// der alte Text darin, und Eingabe wendete ihn an.
#[test]
fn a_rho_draft_does_not_survive_the_field() {
    let mut conic = None;
    let mut rig = Rig::new(|sk| {
        let p = sk.add_point(0.0, 0.0).unwrap();
        let q = sk.add_point(20.0, 0.0).unwrap();
        let a = sk.add_point(10.0, 12.0).unwrap();
        conic = Some(sk.add_conic(p, q, a, 0.5).unwrap());
    });
    let c = conic.unwrap();
    rig.select(&[Item::Curve(c)]);
    let field = sketch_panel::rho_field(&rig.ctx).expect("Rho-Feld");
    rig.click(field.center());
    rig.frame(vec![Event::Text("7".to_string())]);
    rig.settle();
    // Das Feld geht weg, während es den Fokus hat, und kommt wieder.
    rig.select(&[]);
    assert!(sketch_panel::rho_field(&rig.ctx).is_none());
    rig.select(&[Item::Curve(c)]);
    let field = sketch_panel::rho_field(&rig.ctx).expect("Rho-Feld");
    rig.click(field.center());
    rig.frame(vec![key(Key::Enter)]);
    rig.settle();
    let rho = match rig.st().session_sketch().unwrap().curve(c).unwrap().kind {
        CurveKind::Conic { rho, .. } => rho,
        _ => panic!("kein Kegelschnitt"),
    };
    assert_eq!(rho, 0.5, "der alte Entwurf gilt nicht");
    assert!(rig.se().message.is_none(), "{:?}", rig.se().message_text());
}

/// Das Rho-Feld übernimmt nur mit Eingabe oder Tab wie die Felder am Zeiger. Wer es sonst verlässt,
/// verwirft den Entwurf: Kein Wert, kein Schritt, keine Meldung.
#[test]
fn the_rho_field_takes_a_value_only_with_enter_or_tab() {
    let mut conic = None;
    let mut rig = Rig::new(|sk| {
        let p = sk.add_point(0.0, 0.0).unwrap();
        let q = sk.add_point(20.0, 0.0).unwrap();
        let a = sk.add_point(10.0, 12.0).unwrap();
        conic = Some(sk.add_conic(p, q, a, 0.5).unwrap());
    });
    let c = conic.unwrap();
    let rho = |rig: &mut Rig| match rig.st().session_sketch().unwrap().curve(c).unwrap().kind {
        CurveKind::Conic { rho, .. } => rho,
        _ => panic!("kein Kegelschnitt"),
    };
    let type_draft = |rig: &mut Rig, text: &str| {
        rig.select(&[Item::Curve(c)]);
        let field = sketch_panel::rho_field(&rig.ctx).expect("Rho-Feld");
        rig.click(field.center());
        rig.frame(vec![Event::Key {
            key: Key::A,
            physical_key: Some(Key::A),
            pressed: true,
            repeat: false,
            modifiers: Modifiers::COMMAND,
        }]);
        rig.frame(vec![Event::Text(text.to_string())]);
    };
    let steps = rig.undo_len();
    // Klick in die Zeichnung: Der Wert, auch ein falscher, verfällt ohne Meldung.
    let empty = rig.screen(v(10.0, -6.0));
    for draft in ["0,25", "1,5"] {
        type_draft(&mut rig, draft);
        rig.click(empty);
        assert!(
            rig.se().selection.is_empty(),
            "der Klick traf die Zeichnung"
        );
        assert_eq!(rho(&mut rig), 0.5, "{draft}");
        assert_eq!(rig.undo_len(), steps, "{draft}");
        assert!(rig.se().message.is_none(), "{draft}");
    }
    // Klick auf die Kurve selbst: Sie bleibt gewählt, das Feld bleibt stehen und verliert nur den
    // Fokus. Das ist der Fall, in dem früher der halb getippte Wert galt.
    let on_curve = rig
        .st()
        .session_sketch()
        .unwrap()
        .curve_polyline(c, 64)
        .unwrap()[12];
    let on_curve = rig.screen(on_curve);
    for draft in ["0,25", "1,5"] {
        type_draft(&mut rig, draft);
        rig.click(on_curve);
        assert_eq!(rig.se().selection, vec![Item::Curve(c)]);
        assert!(sketch_panel::rho_field(&rig.ctx).is_some(), "{draft}");
        assert_eq!(rho(&mut rig), 0.5, "{draft}");
        assert_eq!(rig.undo_len(), steps, "{draft}");
        assert!(rig.se().message.is_none(), "{draft}");
    }
    // Der Entwurf kommt nicht wieder.
    rig.select(&[Item::Curve(c)]);
    let field = sketch_panel::rho_field(&rig.ctx).expect("Rho-Feld");
    rig.click(field.center());
    rig.frame(vec![key(Key::Enter)]);
    rig.settle();
    assert_eq!(rho(&mut rig), 0.5);
    assert_eq!(rig.undo_len(), steps);
    // Esc verwirft.
    type_draft(&mut rig, "0,25");
    rig.frame(vec![key(Key::Escape)]);
    rig.settle();
    assert_eq!(rho(&mut rig), 0.5);
    assert_eq!(rig.undo_len(), steps);
    // Tab übernimmt, ein Schritt. Der Fokus geht ans nächste Feld; solange zählt der Verlauf das
    // Tippen als Geste wie überall, und der Schritt entsteht beim Verlassen.
    type_draft(&mut rig, "0,25");
    rig.frame(vec![key(Key::Tab)]);
    rig.settle();
    assert_eq!(rho(&mut rig), 0.25);
    rig.click(empty);
    assert_eq!(rho(&mut rig), 0.25);
    assert_eq!(rig.undo_len(), steps + 1);
    // Eingabe übernimmt, ein Schritt.
    type_draft(&mut rig, "0,75");
    rig.frame(vec![key(Key::Enter)]);
    rig.settle();
    assert_eq!(rho(&mut rig), 0.75);
    assert_eq!(rig.undo_len(), steps + 2);
}

/// Die vier Aktionen laufen wie aus der Tastenbelegung über `run_action`.
#[test]
fn the_four_actions_work_through_run_action() {
    use planora_app::commands::run_action;
    let mut ids = None;
    let mut rig = Rig::new(|sk| {
        let (c, pts) = add_spline(sk, &WAVE);
        let cps: Vec<PointId> = (0..7)
            .map(|k| {
                sk.add_point(f64::from(k) * 8.0, 30.0 + f64::from(k % 2) * 9.0)
                    .unwrap()
            })
            .collect();
        ids = Some((c, pts, sk.add_control_spline(&cps, 5).unwrap()));
    });
    let (c, pts, control) = ids.unwrap();
    let state = |rig: &mut Rig| {
        let st = rig.st();
        let session = st.sketch_session.as_ref().unwrap();
        session.handle_state(st.session_sketch().unwrap(), c, pts[2])
    };
    rig.select(&[Item::Point(pts[2])]);
    run_action(rig.app.ws_mut(), Action::SketchTangentHandle);
    assert_eq!(state(&mut rig), (true, false));
    run_action(rig.app.ws_mut(), Action::SketchCurvatureHandle);
    assert_eq!(state(&mut rig), (true, true));
    run_action(rig.app.ws_mut(), Action::SketchCurvatureHandle);
    assert_eq!(state(&mut rig), (true, false));
    run_action(rig.app.ws_mut(), Action::SketchTangentHandle);
    assert_eq!(state(&mut rig), (false, false));
    rig.select(&[Item::Curve(control)]);
    run_action(rig.app.ws_mut(), Action::SketchSplineDegree);
    let degree = |rig: &mut Rig| match rig
        .st()
        .session_sketch()
        .unwrap()
        .curve(control)
        .unwrap()
        .kind
    {
        CurveKind::ControlSpline { degree, .. } => degree,
        _ => panic!("kein Kontrollpunkt-Spline"),
    };
    assert_eq!(degree(&mut rig), 3);
    run_action(rig.app.ws_mut(), Action::SketchInsertControlPoint);
    assert_eq!(rig.se().tool, Tool::InsertControlPoint);
    // Ohne passende Auswahl: nur die Meldung.
    rig.select(&[]);
    run_action(rig.app.ws_mut(), Action::SketchTangentHandle);
    assert_eq!(rig.se().message_text(), Some(tr!("sketch-handle-no-point")));
    run_action(rig.app.ws_mut(), Action::SketchSplineDegree);
    assert_eq!(
        rig.se().message_text(),
        Some(tr!("sketch-spline-degree-no-selection"))
    );
}

#[test]
fn degree_field_for_selected_control_spline_shows_effective_degree() {
    let mut four = None;
    let mut seven = None;
    let mut rig = Rig::new(|sk| {
        // Fünf Punkte bei Wahl von Grad 5: wirksam 4.
        let pts: Vec<PointId> = [
            (0.0, 0.0),
            (10.0, 9.0),
            (20.0, -6.0),
            (30.0, 9.0),
            (40.0, 0.0),
        ]
        .iter()
        .map(|&(x, y)| sk.add_point(x, y).unwrap())
        .collect();
        four = Some(sk.add_control_spline(&pts, 5).unwrap());
        let pts: Vec<PointId> = (0..7)
            .map(|k| {
                sk.add_point(f64::from(k) * 8.0, 30.0 + f64::from(k % 2) * 9.0)
                    .unwrap()
            })
            .collect();
        seven = Some(sk.add_control_spline(&pts, 5).unwrap());
    });
    let (four, seven) = (four.unwrap(), seven.unwrap());
    assert!(sketch_panel::selected_degree(&rig.ctx).is_none());
    rig.select(&[Item::Curve(four)]);
    let (shown, buttons) = sketch_panel::selected_degree(&rig.ctx).expect("Grad-Feld");
    assert_eq!(shown, 4, "der wirksame Grad, nicht 5");
    assert_eq!(buttons.iter().map(|b| b.0).collect::<Vec<_>>(), vec![5, 3]);
    rig.select(&[Item::Curve(seven)]);
    let (shown, buttons) = sketch_panel::selected_degree(&rig.ctx).unwrap();
    assert_eq!(shown, 5);
    // Ein Klick ändert den Grad, ein Schritt.
    let steps = rig.undo_len();
    let three = buttons.iter().find(|b| b.0 == 3).unwrap().1.center();
    rig.click(three);
    assert_eq!(sketch_panel::selected_degree(&rig.ctx).unwrap().0, 3);
    assert_eq!(rig.undo_len(), steps + 1);
    rig.select(&[]);
    assert!(sketch_panel::selected_degree(&rig.ctx).is_none());
}

#[test]
fn handle_buttons_in_the_edit_section_work() {
    let mut ids = None;
    let mut rig = Rig::new(|sk| {
        ids = Some(add_spline(sk, &WAVE));
    });
    let (c, pts) = ids.unwrap();
    with_lang(&german(), || {
        assert!(
            sketch_panel::handle_buttons(&rig.ctx).is_empty(),
            "nichts gewählt"
        );
        rig.select(&[Item::Point(pts[2])]);
        let buttons = sketch_panel::handle_buttons(&rig.ctx);
        let tangent = buttons
            .iter()
            .find(|b| b.0 == "Tangentengriff")
            .expect("Knopf")
            .1
            .center();
        assert!(buttons.iter().any(|b| b.0 == "Krümmungsgriff"));
        let steps = rig.undo_len();
        rig.click(tangent);
        let on = |rig: &mut Rig| {
            let CurveKind::Spline { handles, .. } =
                &rig.st().session_sketch().unwrap().curve(c).unwrap().kind
            else {
                panic!("kein Spline");
            };
            handles.len()
        };
        assert_eq!(on(&mut rig), 1);
        assert_eq!(rig.undo_len(), steps + 1);
        rig.click(tangent);
        assert_eq!(on(&mut rig), 0, "derselbe Knopf schaltet aus");
    });
}

/// Mit echten Ereignissen: Zug am gedämpften Griff, die rechte Taste nimmt ihn zurück; danach schaltet
/// ein neuer Zug ein. Ein Klick auf den Griff ändert die Auswahl nicht.
#[test]
fn the_first_drag_with_real_events_and_right_button_abort() {
    let mut ids = None;
    let mut rig = Rig::new(|sk| {
        ids = Some(add_spline(sk, &WAVE));
    });
    let (c, pts) = ids.unwrap();
    rig.select(&[Item::Curve(c)]);
    let before = rig.st().session_sketch().unwrap().clone();
    let steps = rig.undo_len();
    let (at, ghost_tip) = {
        let st = rig.st();
        let session = st.sketch_session.as_ref().unwrap();
        let g = session
            .visible_handles(st.session_sketch().unwrap())
            .into_iter()
            .find(|h| h.point == pts[2])
            .unwrap();
        (g.at, g.tip)
    };
    let _ = at;
    let p = rig.screen(ghost_tip);
    let to = p + vec2(40.0, 30.0);
    // Klick: Auswahl bleibt, nichts entsteht.
    rig.click(p);
    assert_eq!(rig.se().selection, vec![Item::Curve(c)]);
    assert!(unchanged(rig.st().session_sketch().unwrap(), &before));
    // Zug, rechte Taste.
    rig.time += 1.0;
    rig.frame(vec![Event::PointerMoved(p)]);
    rig.frame(vec![pointer(p, PointerButton::Primary, true)]);
    rig.frame(vec![Event::PointerMoved(p + vec2(10.0, 10.0))]);
    rig.frame(vec![Event::PointerMoved(to)]);
    assert!(rig.se().is_dragging());
    rig.frame(vec![pointer(to, PointerButton::Secondary, true)]);
    rig.frame(vec![pointer(to, PointerButton::Secondary, false)]);
    rig.frame(vec![pointer(to, PointerButton::Primary, false)]);
    rig.settle();
    assert!(!rig.se().is_dragging());
    assert!(unchanged(rig.st().session_sketch().unwrap(), &before));
    assert_eq!(rig.undo_len(), steps);
    // Zug, Esc: nichts, und die Auswahl bleibt.
    rig.time += 1.0;
    rig.frame(vec![Event::PointerMoved(p)]);
    rig.frame(vec![pointer(p, PointerButton::Primary, true)]);
    rig.frame(vec![Event::PointerMoved(p + vec2(10.0, 10.0))]);
    rig.frame(vec![Event::PointerMoved(to)]);
    rig.frame(vec![key(Key::Escape)]);
    rig.frame(vec![pointer(to, PointerButton::Primary, false)]);
    rig.settle();
    assert!(unchanged(rig.st().session_sketch().unwrap(), &before));
    assert_eq!(rig.undo_len(), steps);
    assert_eq!(rig.se().selection, vec![Item::Curve(c)]);
    // Zug bis zum Ende: ein Schritt.
    rig.time += 1.0;
    rig.frame(vec![Event::PointerMoved(p)]);
    rig.frame(vec![pointer(p, PointerButton::Primary, true)]);
    rig.frame(vec![Event::PointerMoved(p + vec2(10.0, 10.0))]);
    rig.frame(vec![Event::PointerMoved(to)]);
    rig.frame(vec![pointer(to, PointerButton::Primary, false)]);
    rig.settle();
    assert_eq!(rig.undo_len(), steps + 1);
    let sk = rig.st().session_sketch().unwrap().clone();
    let CurveKind::Spline { handles, .. } = &sk.curve(c).unwrap().kind else {
        panic!("kein Spline");
    };
    assert_eq!(handles.len(), 1);
}

impl Rig {
    /// Zieht mit echten Ereignissen von `from` nach `to` (Bildschirm) und lässt los.
    fn pull(&mut self, from: Pos2, to: Pos2) {
        self.time += 1.0;
        self.frame(vec![Event::PointerMoved(from)]);
        self.frame(vec![pointer(from, PointerButton::Primary, true)]);
        self.frame(vec![Event::PointerMoved(from + (to - from) * 0.25)]);
        self.frame(vec![Event::PointerMoved(to)]);
        self.frame(vec![pointer(to, PointerButton::Primary, false)]);
        self.settle();
    }

    fn sketch(&mut self) -> Sketch {
        self.st().session_sketch().unwrap().clone()
    }
}

fn tip_of(sk: &Sketch, c: CurveId, at: PointId) -> Option<PointId> {
    let CurveKind::Spline { handles, .. } = &sk.curve(c).unwrap().kind else {
        panic!("kein Spline");
    };
    handles.iter().find(|h| h.at == at).map(|h| h.tip)
}

/// Handtest Skizzen 7, Fehler 1: Spline durch vier Stützpunkte mit Krümmungsgriff am Ende. Der erste
/// Zug am gedämpften Griff eines inneren Stützpunkts schaltet ihn ein und zieht ihn mit, die Kurve
/// folgt, ein Schritt. Ein zweiter Zug am nun aktiven Griff zieht weiter. An beiden inneren Punkten und
/// nach beiden Seiten der Spitze (länger und quer über den Stützpunkt hinweg).
#[test]
fn the_inner_handle_follows_the_mouse_beside_a_curvature_handle() {
    const FOUR: [(f64, f64); 4] = [(10.66, -16.14), (17.0, -5.0), (26.0, -15.0), (36.0, -7.0)];
    for k in [1usize, 2] {
        for pull in [vec2(60.0, -40.0), vec2(-90.0, 30.0)] {
            let mut ids = None;
            let mut rig = Rig::new(|sk| {
                let (c, pts) = add_spline(sk, &FOUR);
                sk.activate_curvature_handle(c, pts[3]).unwrap();
                ids = Some((c, pts));
            });
            let (c, pts) = ids.unwrap();
            rig.select(&[Item::Curve(c)]);
            let form = rig.sketch().curve_polyline(c, 64).unwrap();
            let steps = rig.undo_len();
            let ghost_tip = {
                let st = rig.st();
                let session = st.sketch_session.as_ref().unwrap();
                session
                    .visible_handles(st.session_sketch().unwrap())
                    .into_iter()
                    .find(|h| h.ghost && h.part == HandlePart::Tangent && h.point == pts[k])
                    .expect("gedämpfter Griff")
                    .tip
            };
            let p = rig.screen(ghost_tip);
            let to = p + pull;
            let want = ghost_tip + (rig.raw_at(to) - rig.raw_at(p));
            rig.pull(p, to);
            let what = format!("Punkt {k}, Zug {pull:?}");
            let sk = rig.sketch();
            let tip = tip_of(&sk, c, pts[k]).unwrap_or_else(|| panic!("{what}: eingeschaltet"));
            let got = sk.point_xy(tip).unwrap();
            assert!(
                got.distance(want) < 1e-6 * want.length().max(1.0),
                "{what}: Spitze {got} statt {want}"
            );
            assert!(
                sk.point_xy(pts[k])
                    .unwrap()
                    .distance(v(FOUR[k].0, FOUR[k].1))
                    < 1e-9,
                "{what}: Stützpunkt bleibt"
            );
            assert!(
                !same_shape(&form, &sk.curve_polyline(c, 64).unwrap(), 1e-3),
                "{what}: die Kurve folgt"
            );
            assert_eq!(rig.undo_len(), steps + 1, "{what}: ein Schritt");
            assert_eq!(
                rig.st().sketch_undo_label().as_deref(),
                Some(tr!("undo-handle-tangent").as_str()),
                "{what}"
            );
            // Zweiter Zug am aktiven Griff, ein eigener Schritt.
            let p2 = rig.screen(got);
            let to2 = p2 + vec2(-pull.y, pull.x) * 0.5;
            let want2 = got + (rig.raw_at(to2) - rig.raw_at(p2));
            rig.pull(p2, to2);
            let sk = rig.sketch();
            let got2 = sk.point_xy(tip).unwrap();
            assert!(
                got2.distance(want2) < 1e-6 * want2.length().max(1.0),
                "{what}: zweiter Zug {got2} statt {want2}"
            );
            assert_eq!(rig.undo_len(), steps + 2, "{what}: zweiter Schritt");
            // Ein Rückgängig nimmt den zweiten Zug, das zweite den Griff.
            assert!(rig.st().undo_sketch());
            assert!(rig.st().undo_sketch());
            rig.settle();
            assert!(tip_of(&rig.sketch(), c, pts[k]).is_none(), "{what}");
            assert!(same_shape(
                &form,
                &rig.sketch().curve_polyline(c, 64).unwrap(),
                1e-9
            ));
        }
    }
}

/// Wie der Spline am Bogen hängt: ein gemeinsamer Punkt (so legt ihn das Zeichnen mit Fang an) oder
/// zwei Punkte mit „Deckungsgleich“.
#[derive(Clone, Copy, Debug)]
enum Joint {
    Shared,
    Coincident,
}

/// Bogen um `m` mit Radius `r` von `from` bis `to` (Winkel), am Ende `fit` des Splines angesetzt.
struct ArcAtSpline {
    spline: CurveId,
    pts: Vec<PointId>,
    arc: CurveId,
    center: PointId,
    free: PointId,
}

fn arc_at_spline(sk: &mut Sketch, at_end: bool, joint: Joint, handle: bool) -> ArcAtSpline {
    const FOUR: [(f64, f64); 4] = [(10.0, -16.0), (17.0, -5.0), (26.0, -15.0), (36.0, -7.0)];
    let (spline, pts) = add_spline(sk, &FOUR);
    let fit = if at_end { pts[3] } else { pts[0] };
    let f = sk.point_xy(fit).unwrap();
    // Die Mitte liegt außen vor dem Ende, der Bogen läuft vom Ende weg und knickt dort ab.
    let m = if at_end {
        f + v(4.0, -6.0)
    } else {
        f + v(-6.0, -4.0)
    };
    let r = f.distance(m);
    let a0 = (f - m).y.atan2((f - m).x);
    let a1 = a0 + 2.0;
    let center = sk.add_point(m.x, m.y).unwrap();
    let free = sk
        .add_point(m.x + r * a1.cos(), m.y + r * a1.sin())
        .unwrap();
    let start = match joint {
        Joint::Shared => fit,
        Joint::Coincident => {
            let p = sk.add_point(f.x, f.y).unwrap();
            sk.add_constraint(K::Coincident(p, fit)).unwrap();
            p
        }
    };
    let arc = sk.add_arc(center, start, free).unwrap();
    if handle {
        sk.activate_tangent_handle(spline, fit).unwrap();
    }
    ArcAtSpline {
        spline,
        pts,
        arc,
        center,
        free,
    }
}

/// Handtest Skizzen 7, Fehler 2: „Glatt“ zwischen Spline und einem Bogen, der an seinem Anfang oder
/// Ende ansetzt. Geklickt wird der Bogen nahe seinem freien Ende, der Spline nahe der Fuge. Das Glatt
/// gilt an der Fuge, ohne Sprung und ohne Überzähligkeit; der Bogen bleibt, wo er war; ein Schritt
/// namens „Glatt“. In beiden Wahlreihenfolgen, auch mit schon eingeschaltetem Griff.
#[test]
fn smooth_between_spline_and_an_attached_arc_stays_put() {
    for at_end in [false, true] {
        for joint in [Joint::Shared, Joint::Coincident] {
            for handle in [false, true] {
                for arc_first in [true, false] {
                    let what = format!(
                        "Ende {at_end}, {joint:?}, Griff {handle}, Bogen zuerst {arc_first}"
                    );
                    let mut ids = None;
                    let mut rig =
                        Rig::new(|sk| ids = Some(arc_at_spline(sk, at_end, joint, handle)));
                    let ids = ids.unwrap();
                    let before = rig.sketch();
                    let fit = if at_end { ids.pts[3] } else { ids.pts[0] };
                    let free_before = before.point_xy(ids.free).unwrap();
                    let center_before = before.point_xy(ids.center).unwrap();
                    // Klickstellen: der Bogen bei 80 % zum freien Ende, der Spline nahe der Fuge.
                    let on_arc = {
                        let (lo, hi) = before.curve_domain(ids.arc).unwrap();
                        before.curve_eval(ids.arc, lo + 0.8 * (hi - lo)).unwrap()
                    };
                    let on_arc = if on_arc.distance(free_before)
                        < before
                            .curve_eval(ids.arc, before.curve_domain(ids.arc).unwrap().0)
                            .unwrap()
                            .distance(free_before)
                    {
                        on_arc
                    } else {
                        let (lo, hi) = before.curve_domain(ids.arc).unwrap();
                        before.curve_eval(ids.arc, lo + 0.2 * (hi - lo)).unwrap()
                    };
                    let on_spline = {
                        let (lo, hi) = before.curve_domain(ids.spline).unwrap();
                        let t = if at_end { hi - 0.3 } else { lo + 0.3 };
                        before.curve_eval(ids.spline, t).unwrap()
                    };
                    let steps = rig.undo_len();
                    {
                        let (se, sk) = rig.st().session_sketch_mut().unwrap();
                        se.set_tool(sk, Tool::Constraint(ConstraintAction::Smooth));
                    }
                    rig.settle();
                    let clicks = if arc_first {
                        [on_arc, on_spline]
                    } else {
                        [on_spline, on_arc]
                    };
                    for at in clicks {
                        let p = rig.screen(at);
                        rig.click(p);
                    }
                    let sk = rig.sketch();
                    let smooth: Vec<_> = sk
                        .constraints()
                        .filter_map(|(_, d)| match d.kind {
                            K::Smooth(a, pa, b, pb) => Some((a, pa, b, pb)),
                            _ => None,
                        })
                        .collect();
                    assert_eq!(smooth.len(), 1, "{what}: ein Glatt");
                    let (a, pa, b, pb) = smooth[0];
                    let (arc_end, spline_end) = if a == ids.arc { (pa, pb) } else { (pb, pa) };
                    assert!(
                        [a, b].contains(&ids.arc) && [a, b].contains(&ids.spline),
                        "{what}"
                    );
                    assert_eq!(spline_end, fit, "{what}: am Spline die Fuge");
                    assert_ne!(arc_end, ids.free, "{what}: am Bogen die Fuge");
                    let report = rig.se().last_report.clone().unwrap();
                    assert_eq!(report.status, SolveStatus::Solved, "{what}: {report:?}");
                    assert!(
                        report.redundant.is_empty() && report.conflicting.is_empty(),
                        "{what}: {report:?}"
                    );
                    assert!(
                        sk.point_xy(ids.free).unwrap().distance(free_before) < 1e-6,
                        "{what}: der Bogen springt"
                    );
                    assert!(
                        sk.point_xy(ids.center).unwrap().distance(center_before) < 1e-6,
                        "{what}: die Bogenmitte springt"
                    );
                    assert_eq!(rig.undo_len(), steps + 1, "{what}: ein Schritt");
                    assert_eq!(
                        rig.st().sketch_undo_label().as_deref(),
                        Some(tr!("sketch-constraint-smooth").as_str()),
                        "{what}"
                    );
                }
            }
        }
    }
}
