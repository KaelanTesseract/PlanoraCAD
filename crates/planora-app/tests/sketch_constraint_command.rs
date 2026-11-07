//! Handtest Skizzen 5, Korrektur 4: Die Knöpfe der geometrischen Abhängigkeiten wirken wie in
//! Fusion auch „erst Befehl, dann Auswahl“. Ohne passende Auswahl startet der Knopf den Befehl
//! (`Tool::Constraint`); die Klicks in der Ansicht wählen die Bezüge, sobald es reicht, entsteht
//! die Abhängigkeit, und der Befehl bleibt für das nächste Paar, bis Esc oder Rechtsklick.

use planora_app::sketch_state::{ConstraintAction as A, Item, SketchSession, Tool};
use planora_app::state::{AppState, StandardPlane};
use planora_app::{fonts, sketch_panel, sketch_view};
use planora_feature::SketchPlane;
use planora_i18n::{german, tr, with_lang};
use planora_math::DVec2;
use planora_sketch::constraint::{ConstraintKind as K, LineOrPoints};
use planora_sketch::model::{CurveId, PointId, Sketch};

const W: f32 = 1920.0;
const H: f32 = 1080.0;
const SCREEN: egui::Rect = egui::Rect {
    min: egui::pos2(0.0, 0.0),
    max: egui::pos2(W, H),
};

type Frame = Vec<egui::Event>;

fn v(x: f64, y: f64) -> DVec2 {
    DVec2::new(x, y)
}

fn context() -> egui::Context {
    let ctx = egui::Context::default();
    ctx.set_fonts(fonts::definitions());
    ctx
}

fn sketch_with<T>(build: impl FnOnce(&mut Sketch) -> T) -> (AppState, T) {
    let mut s = AppState::new();
    let i = s
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    s.open_sketch(i).unwrap();
    let out = build(s.session_sketch_mut().unwrap().1);
    s.observe_sketch(false);
    (s, out)
}

fn sk(s: &AppState) -> &Sketch {
    s.session_sketch().unwrap()
}

fn se(s: &AppState) -> &SketchSession {
    s.sketch_session.as_ref().unwrap()
}

fn live(s: &AppState) -> Vec<K> {
    sk(s).constraints().map(|(_, c)| c.kind.clone()).collect()
}

fn message(s: &AppState) -> Option<String> {
    se(s).message_text()
}

fn circle(sk: &mut Sketch, x: f64, y: f64, r: f64) -> (CurveId, PointId) {
    let m = sk.add_point(x, y).unwrap();
    (sk.add_circle(m, r).unwrap(), m)
}

fn line(sk: &mut Sketch, a: (f64, f64), b: (f64, f64)) -> (CurveId, PointId, PointId) {
    let p = sk.add_point(a.0, a.1).unwrap();
    let q = sk.add_point(b.0, b.1).unwrap();
    (sk.add_line(p, q).unwrap(), p, q)
}

fn screen(s: &AppState, p: DVec2) -> egui::Pos2 {
    let frame = *sk(s).frame();
    let mut cam = s.camera;
    cam.set_viewport(W, H);
    let q = sketch_view::sketch_to_screen(&cam, &frame, glam::Vec2::new(W, H), p).unwrap();
    egui::pos2(q.x, q.y)
}

fn moved(p: egui::Pos2) -> egui::Event {
    egui::Event::PointerMoved(p)
}

fn button(p: egui::Pos2, b: egui::PointerButton, pressed: bool) -> egui::Event {
    egui::Event::PointerButton {
        pos: p,
        button: b,
        pressed,
        modifiers: egui::Modifiers::default(),
    }
}

fn esc() -> egui::Event {
    egui::Event::Key {
        key: egui::Key::Escape,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::default(),
    }
}

thread_local! {
    static TIME: std::cell::Cell<f64> = const { std::cell::Cell::new(0.0) };
}

/// Ein Bild der Ansicht; danach schreibt es den Verlauf fort wie die App. Die Zeit läuft je Bild
/// eine halbe Sekunde, so zählt egui zwei Klicks nie als Doppelklick (außer [`double_click`]).
fn frame_at(ctx: &egui::Context, s: &mut AppState, events: Frame, step: f64) {
    let time = TIME.with(|t| {
        t.set(t.get() + step);
        t.get()
    });
    let raw = egui::RawInput {
        screen_rect: Some(SCREEN),
        time: Some(time),
        events,
        ..Default::default()
    };
    let mut gesture = false;
    let out = ctx.run_ui(raw, |ui| {
        sketch_view::show_overlay(ui, SCREEN, s);
        gesture = ui.input(|i| i.pointer.any_down());
    });
    out.drop_without_applying_deltas();
    s.observe_sketch(gesture);
}

fn frame(ctx: &egui::Context, s: &mut AppState, events: Frame) {
    frame_at(ctx, s, events, 0.5);
}

/// Linksklick in der Ansicht an der Skizzenstelle `at`.
fn click(ctx: &egui::Context, s: &mut AppState, at: DVec2) {
    let p = screen(s, at);
    let left = egui::PointerButton::Primary;
    frame(ctx, s, vec![moved(p)]);
    frame(ctx, s, vec![moved(p), button(p, left, true)]);
    frame(ctx, s, vec![button(p, left, false)]);
    frame(ctx, s, vec![]);
}

/// Doppelklick in der Ansicht an der Skizzenstelle `at` (vorher ist der Zeiger dort).
fn double_click(ctx: &egui::Context, s: &mut AppState, at: DVec2) {
    let p = screen(s, at);
    let left = egui::PointerButton::Primary;
    frame(ctx, s, vec![moved(p)]);
    for _ in 0..2 {
        frame_at(ctx, s, vec![button(p, left, true)], 0.02);
        frame_at(ctx, s, vec![button(p, left, false)], 0.02);
    }
    frame(ctx, s, vec![]);
}

fn right_click(ctx: &egui::Context, s: &mut AppState, at: DVec2) {
    let p = screen(s, at);
    let right = egui::PointerButton::Secondary;
    frame(ctx, s, vec![moved(p)]);
    frame(ctx, s, vec![moved(p), button(p, right, true)]);
    frame(ctx, s, vec![button(p, right, false)]);
    frame(ctx, s, vec![]);
}

/// Der Knopf im Skizzenfeld (ohne Fenster; das Skizzenfeld ruft genau das).
fn press(s: &mut AppState, action: A) {
    let (se, sk) = s.session_sketch_mut().unwrap();
    let _ = se.constraint_button(sk, action);
    s.observe_sketch(false);
}

fn undo(s: &mut AppState) {
    assert!(s.undo_sketch());
}

#[test]
fn concentric_button_first_then_two_circles() {
    with_lang(&german(), || {
        let ctx = context();
        let (mut s, ((_, m1), (_, m2))) =
            sketch_with(|sk| (circle(sk, -10.0, 0.0, 4.0), circle(sk, 12.0, 6.0, 5.0)));
        let dof_before = sk(&s).clone().solve().dof;
        let before = live(&s).len();
        frame(&ctx, &mut s, vec![]);
        press(&mut s, A::Concentric);
        assert_eq!(se(&s).tool, Tool::Constraint(A::Concentric));
        assert_eq!(live(&s).len(), before, "ohne Auswahl entsteht noch nichts");
        // Die Statuszeile sagt, was zu wählen ist.
        assert_eq!(se(&s).active_hint(), tr!("sketch-command-concentric-hint"));
        assert_eq!(
            sketch_panel::tool_hint(Tool::Constraint(A::Concentric)),
            tr!("sketch-command-concentric-hint")
        );
        click(&ctx, &mut s, v(-6.0, 0.0));
        assert_eq!(live(&s).len(), before, "ein Kreis reicht nicht");
        assert_eq!(se(&s).selection.len(), 1);
        click(&ctx, &mut s, v(17.0, 6.0));
        let now = live(&s);
        assert_eq!(now.len(), before + 1, "{:?}", message(&s));
        assert!(
            matches!(now.last(), Some(K::Coincident(a, b)) if [*a, *b] == [m1, m2] || [*a, *b] == [m2, m1]),
            "{now:?}"
        );
        // Auswahl leer, der Befehl bleibt (Fusion wiederholt); die Freiheitsgrade stimmen.
        assert!(se(&s).selection.is_empty());
        assert_eq!(se(&s).tool, Tool::Constraint(A::Concentric));
        assert_eq!(se(&s).last_report.as_ref().unwrap().dof, dof_before - 2);
        assert_eq!(
            se(&s).status(sk(&s)).0,
            tr!("sketch-status-dof", n = dof_before - 2)
        );
        // Ein Rückgängig nimmt genau diese Abhängigkeit zurück.
        undo(&mut s);
        assert_eq!(live(&s).len(), before);
        let p = sk(&s);
        assert!(p.point_xy(m1).unwrap().distance(p.point_xy(m2).unwrap()) > 10.0);
    });
}

#[test]
fn collinear_and_a_second_pair_repeat_and_undo_takes_one() {
    with_lang(&german(), || {
        let ctx = context();
        let (mut s, _) = sketch_with(|sk| {
            line(sk, (-20.0, 0.0), (-10.0, 1.0));
            line(sk, (0.0, 5.0), (10.0, 8.0));
            line(sk, (-20.0, -10.0), (-10.0, -12.0));
            line(sk, (0.0, -15.0), (10.0, -14.0));
        });
        let before = live(&s).len();
        frame(&ctx, &mut s, vec![]);
        press(&mut s, A::Collinear);
        click(&ctx, &mut s, v(-15.0, 0.5));
        click(&ctx, &mut s, v(5.0, 6.5));
        assert_eq!(live(&s).len(), before + 1, "{:?}", message(&s));
        assert!(matches!(live(&s).last(), Some(K::Collinear(..))));
        // Das zweite Paar im selben Befehl.
        click(&ctx, &mut s, v(-15.0, -11.0));
        click(&ctx, &mut s, v(5.0, -14.5));
        assert_eq!(live(&s).len(), before + 2, "{:?}", message(&s));
        assert_eq!(se(&s).tool, Tool::Constraint(A::Collinear));
        undo(&mut s);
        assert_eq!(live(&s).len(), before + 1, "genau eine zurück");
        assert!(matches!(live(&s).last(), Some(K::Collinear(..))));
    });
}

#[test]
fn horizontal_sets_with_the_first_line_and_with_two_points() {
    with_lang(&german(), || {
        let ctx = context();
        let (mut s, ((l, ..), (_, p, q))) = sketch_with(|sk| {
            (
                line(sk, (-20.0, 0.0), (-5.0, 4.0)),
                line(sk, (5.0, -10.0), (20.0, 10.0)),
            )
        });
        let before = live(&s).len();
        frame(&ctx, &mut s, vec![]);
        press(&mut s, A::Horizontal);
        click(&ctx, &mut s, v(-12.5, 2.0));
        assert_eq!(live(&s).len(), before + 1, "{:?}", message(&s));
        assert!(matches!(
            live(&s).last(),
            Some(K::Horizontal(LineOrPoints::Line(c))) if *c == l
        ));
        // Zwei Punkte gehen auch (die Enden der zweiten Linie).
        click(&ctx, &mut s, v(5.0, -10.0));
        assert_eq!(live(&s).len(), before + 1, "ein Punkt reicht nicht");
        click(&ctx, &mut s, v(20.0, 10.0));
        assert_eq!(live(&s).len(), before + 2, "{:?}", message(&s));
        assert!(matches!(
            live(&s).last(),
            Some(K::Horizontal(LineOrPoints::Points(a, b))) if [*a, *b] == [p, q] || [*a, *b] == [q, p]
        ));
    });
}

#[test]
fn a_line_is_refused_by_concentric_and_the_command_goes_on() {
    with_lang(&german(), || {
        let ctx = context();
        let (mut s, _) = sketch_with(|sk| {
            circle(sk, -10.0, 0.0, 4.0);
            circle(sk, 12.0, 6.0, 5.0);
            line(sk, (-20.0, -15.0), (20.0, -15.0));
        });
        let before = live(&s).len();
        let rev = sk(&s).revision();
        frame(&ctx, &mut s, vec![]);
        press(&mut s, A::Concentric);
        click(&ctx, &mut s, v(0.0, -15.0));
        assert_eq!(message(&s), Some(tr!("sketch-concentric-kinds")));
        assert!(
            se(&s).selection.is_empty(),
            "der Klick wird nicht übernommen"
        );
        assert_eq!(sk(&s).revision(), rev);
        // Nach einem Kreis ebenso: der Kreis bleibt gewählt.
        click(&ctx, &mut s, v(-6.0, 0.0));
        click(&ctx, &mut s, v(0.0, -15.0));
        assert_eq!(message(&s), Some(tr!("sketch-concentric-kinds")));
        assert_eq!(se(&s).selection.len(), 1);
        click(&ctx, &mut s, v(17.0, 6.0));
        assert_eq!(live(&s).len(), before + 1, "{:?}", message(&s));
    });
}

#[test]
fn esc_and_right_click_end_the_command_without_rest() {
    with_lang(&german(), || {
        let ctx = context();
        let (mut s, _) = sketch_with(|sk| {
            circle(sk, -10.0, 0.0, 4.0);
            circle(sk, 12.0, 6.0, 5.0);
        });
        let rev = sk(&s).revision();
        let steps = s.sketch_history().unwrap().undo_len();
        frame(&ctx, &mut s, vec![]);
        press(&mut s, A::Concentric);
        click(&ctx, &mut s, v(-6.0, 0.0));
        assert_eq!(se(&s).selection.len(), 1);
        frame(&ctx, &mut s, vec![esc()]);
        frame(&ctx, &mut s, vec![]);
        assert_eq!(se(&s).tool, Tool::Select);
        assert!(se(&s).selection.is_empty());
        assert_eq!(se(&s).curve_picks, Default::default());
        assert_eq!(sk(&s).revision(), rev);
        assert_eq!(s.sketch_history().unwrap().undo_len(), steps);
        // Rechtsklick ebenso.
        press(&mut s, A::Concentric);
        click(&ctx, &mut s, v(-6.0, 0.0));
        right_click(&ctx, &mut s, v(0.0, 20.0));
        assert_eq!(se(&s).tool, Tool::Select);
        assert!(se(&s).selection.is_empty());
        // Ein anderes Werkzeug beendet ihn auch, ohne die halbe Auswahl mitzunehmen.
        press(&mut s, A::Concentric);
        click(&ctx, &mut s, v(-6.0, 0.0));
        {
            let (se, sk) = s.session_sketch_mut().unwrap();
            se.choose_tool(sk, Tool::Line);
        }
        assert_eq!(se(&s).tool, Tool::Line);
        assert!(se(&s).selection.is_empty());
        assert_eq!(sk(&s).revision(), rev);
    });
}

/// Bogen und Spline, die beide Enden teilen (geschlossene Schleife).
fn closed_loop(sk: &mut Sketch) -> (CurveId, CurveId, PointId, PointId) {
    let m = sk.add_point(0.0, 0.0).unwrap();
    let st = sk.add_point(10.0, 0.0).unwrap();
    let en = sk.add_point(0.0, 10.0).unwrap();
    let arc = sk.add_arc(m, st, en).unwrap();
    let mid = sk.add_point(-12.0, -12.0).unwrap();
    let sp = sk.add_spline(&[en, mid, st]).unwrap();
    (arc, sp, st, en)
}

fn on_curve(s: &AppState, c: CurveId, t: f64) -> DVec2 {
    let pl = sk(s).curve_polyline(c, 256).unwrap();
    pl[((pl.len() - 1) as f64 * t).round() as usize]
}

#[test]
fn smooth_takes_the_clicked_end_when_the_button_comes_first() {
    with_lang(&german(), || {
        let ctx = context();
        for near_end in [true, false] {
            for arc_first in [true, false] {
                let (mut s, (arc, sp, st, en)) = sketch_with(closed_loop);
                let (ta, ts) = if near_end { (0.8, 0.15) } else { (0.2, 0.85) };
                let (on_arc, on_sp) = (on_curve(&s, arc, ta), on_curve(&s, sp, ts));
                let clicks = if arc_first {
                    [on_arc, on_sp]
                } else {
                    [on_sp, on_arc]
                };
                frame(&ctx, &mut s, vec![]);
                press(&mut s, A::Smooth);
                for at in clicks {
                    click(&ctx, &mut s, at);
                }
                let want = if near_end { en } else { st };
                let Some(K::Smooth(_, pa, _, pb)) = live(&s).last().cloned() else {
                    panic!("kein Glatt: {:?}", message(&s));
                };
                assert_eq!(
                    (pa, pb),
                    (want, want),
                    "Ende {near_end}, Bogen zuerst {arc_first}"
                );
            }
        }
    });
}

#[test]
fn smooth_keeps_the_click_of_a_preselected_first_curve() {
    with_lang(&german(), || {
        let ctx = context();
        for near_end in [true, false] {
            let (mut s, (arc, sp, st, en)) = sketch_with(closed_loop);
            let (ta, ts) = if near_end { (0.8, 0.15) } else { (0.2, 0.85) };
            let (on_arc, on_sp) = (on_curve(&s, arc, ta), on_curve(&s, sp, ts));
            frame(&ctx, &mut s, vec![]);
            // Erst den Bogen im Auswählen anklicken, dann der Knopf, dann der Spline.
            click(&ctx, &mut s, on_arc);
            assert_eq!(se(&s).selection, vec![Item::Curve(arc)]);
            press(&mut s, A::Smooth);
            assert_eq!(se(&s).tool, Tool::Constraint(A::Smooth));
            assert_eq!(
                se(&s).selection,
                vec![Item::Curve(arc)],
                "zählt als erster Bezug"
            );
            click(&ctx, &mut s, on_sp);
            let want = if near_end { en } else { st };
            let Some(K::Smooth(_, pa, _, pb)) = live(&s).last().cloned() else {
                panic!("kein Glatt: {:?}", message(&s));
            };
            assert_eq!((pa, pb), (want, want), "Ende {near_end}");
        }
    });
}

#[test]
fn a_fitting_selection_still_sets_at_once_and_an_unfitting_one_says_why() {
    with_lang(&german(), || {
        let ctx = context();
        let (mut s, ((c1, _), (c2, _), (l, ..))) = sketch_with(|sk| {
            (
                circle(sk, -10.0, 0.0, 4.0),
                circle(sk, 12.0, 6.0, 5.0),
                line(sk, (-20.0, -15.0), (20.0, -15.0)),
            )
        });
        frame(&ctx, &mut s, vec![]);
        let before = live(&s).len();
        s.sketch_session.as_mut().unwrap().selection = vec![Item::Curve(c1), Item::Curve(c2)];
        press(&mut s, A::Concentric);
        assert_eq!(live(&s).len(), before + 1, "wie bisher sofort");
        assert_eq!(se(&s).tool, Tool::Select, "kein Befehl");
        // Kreis und Linie: Die Meldung sagt warum, der Befehl startet ohne diese Auswahl.
        s.sketch_session.as_mut().unwrap().selection = vec![Item::Curve(c1), Item::Curve(l)];
        press(&mut s, A::Concentric);
        assert_eq!(live(&s).len(), before + 1);
        assert_eq!(message(&s), Some(tr!("sketch-concentric-kinds")));
        assert_eq!(se(&s).tool, Tool::Constraint(A::Concentric));
        assert!(se(&s).selection.is_empty());
    });
}

#[test]
fn in_the_command_a_drag_moves_nothing_and_a_double_click_picks_once() {
    with_lang(&german(), || {
        let ctx = context();
        let (mut s, (c1, m1)) = sketch_with(|sk| {
            let c = circle(sk, -10.0, 0.0, 4.0);
            circle(sk, 12.0, 6.0, 5.0);
            c
        });
        frame(&ctx, &mut s, vec![]);
        press(&mut s, A::Concentric);
        let rev = sk(&s).revision();
        // Ziehen an der Mitte verschiebt nichts und beginnt keinen Rahmen.
        let (from, to) = (screen(&s, v(-10.0, 0.0)), screen(&s, v(-2.0, 8.0)));
        let left = egui::PointerButton::Primary;
        frame(&ctx, &mut s, vec![moved(from)]);
        frame(&ctx, &mut s, vec![button(from, left, true)]);
        frame(&ctx, &mut s, vec![moved(from.lerp(to, 0.5))]);
        frame(&ctx, &mut s, vec![moved(to)]);
        frame(&ctx, &mut s, vec![button(to, left, false)]);
        frame(&ctx, &mut s, vec![]);
        assert_eq!(sk(&s).revision(), rev);
        assert_eq!(sk(&s).point_xy(m1).unwrap(), v(-10.0, 0.0));
        assert!(se(&s).box_select.is_none());
        // Ein Doppelklick auf den Kreis wählt ihn einmal (keine Kette, kein Abwählen).
        double_click(&ctx, &mut s, v(-6.0, 0.0));
        assert_eq!(se(&s).selection, vec![Item::Curve(c1)]);
        assert_eq!(se(&s).tool, Tool::Constraint(A::Concentric));
    });
}

#[test]
fn every_geometric_button_has_a_hint_in_both_languages() {
    use planora_i18n::english;
    let geometric: Vec<A> = sketch_panel::ACTIONS
        .into_iter()
        .filter(|a| !a.is_dimension())
        .collect();
    assert_eq!(geometric.len(), 14);
    for lang in [german(), english()] {
        with_lang(&lang, || {
            let mut seen = std::collections::BTreeSet::new();
            for &a in &geometric {
                let hint = sketch_panel::tool_hint(Tool::Constraint(a));
                assert!(
                    !hint.is_empty() && !hint.starts_with("sketch-"),
                    "{a:?}: {hint}"
                );
                assert!(seen.insert(hint), "{a:?} doppelt");
                assert_eq!(sketch_panel::tool_label(Tool::Constraint(a)), a.label());
            }
        });
    }
}
