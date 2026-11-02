//! Tangentialer Bogen aus dem Linienwerkzeug (Skizzen 3, Task 8): Drücken auf dem letzten Punkt des
//! Linienzugs und Ziehen zeichnet den Bogen, danach läuft der Linienzug tangential weiter. Klicks,
//! Schwenken und das Ziehen im Auswahlwerkzeug bleiben, wie sie sind.
//!
//! Ohne Fenster, bei 1920 × 1080: das Overlay der Skizze bekommt Rohereignisse, nach jedem Bild wird
//! wie in der App der Verlauf der Skizze fortgeschrieben (Geste = Maustaste gedrückt).

use egui::{Modifiers, PointerButton};
use planora_app::sketch_input::FieldKind;
use planora_app::sketch_state::{SketchSession, Tool};
use planora_app::state::{AppState, StandardPlane};
use planora_app::{fonts, sketch_view};
use planora_feature::SketchPlane;
use planora_i18n::tr;
use planora_math::DVec2;
use planora_sketch::constraint::ConstraintKind;
use planora_sketch::model::{CurveId, CurveKind, PointId, Sketch};
use planora_sketch::solver::SolveStatus;

const W: f32 = 1920.0;
const H: f32 = 1080.0;
const SCREEN: egui::Rect = egui::Rect {
    min: egui::pos2(0.0, 0.0),
    max: egui::pos2(W, H),
};
/// Anfang und Ende der ersten Linie: schräg, damit weder Waagerecht noch Senkrecht greifen.
const A: (f64, f64) = (5.0, 5.0);
const B: (f64, f64) = (25.0, 12.0);
/// Bogenende links der Linie.
const E: (f64, f64) = (40.0, 31.0);

fn context() -> egui::Context {
    let ctx = egui::Context::default();
    ctx.set_fonts(fonts::definitions());
    ctx
}

/// Offene Skizze auf XY ohne Raster-Fang; `build` legt den Inhalt an, gewählt ist die Linie.
fn rig_with(build: impl FnOnce(&mut Sketch)) -> AppState {
    let mut s = AppState::new();
    let i = s
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    s.open_sketch(i).unwrap();
    s.sketch_snap_grid = false;
    // Etwa 6 Pixel je mm: Alle Lagen der Tests liegen im Bild.
    s.camera.distance *= 3.0;
    {
        let sk = s.session_sketch_mut().unwrap().1;
        build(sk);
        sk.solve();
    }
    s.observe_sketch(false);
    let (se, sk) = s.session_sketch_mut().unwrap();
    se.set_tool(sk, Tool::Line);
    s
}

fn rig() -> AppState {
    rig_with(|_| {})
}

fn se(s: &AppState) -> &SketchSession {
    s.sketch_session.as_ref().unwrap()
}

fn sk(s: &AppState) -> &Sketch {
    s.session_sketch().unwrap()
}

/// Bildschirmlage eines Skizzenpunkts.
fn screen(s: &AppState, p: (f64, f64)) -> egui::Pos2 {
    let frame = *sk(s).frame();
    let mut cam = s.camera;
    cam.set_viewport(W, H);
    let v =
        sketch_view::sketch_to_screen(&cam, &frame, glam::Vec2::new(W, H), DVec2::new(p.0, p.1))
            .unwrap();
    egui::pos2(v.x, v.y)
}

fn moved(p: egui::Pos2) -> egui::Event {
    egui::Event::PointerMoved(p)
}

fn mouse(p: egui::Pos2, button: PointerButton, pressed: bool, modifiers: Modifiers) -> egui::Event {
    egui::Event::PointerButton {
        pos: p,
        button,
        pressed,
        modifiers,
    }
}

fn button(p: egui::Pos2, pressed: bool) -> egui::Event {
    mouse(p, PointerButton::Primary, pressed, Modifiers::default())
}

fn esc() -> egui::Event {
    egui::Event::Key {
        key: egui::Key::Escape,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::default(),
    }
}

/// Spielt je Eintrag ein Bild gegen das Overlay ab und schreibt danach den Verlauf fort wie die App.
/// Ein leeres Bild vorweg: egui ordnet Zeiger-Ereignisse den Flächen des vorigen Bilds zu.
fn play_with(
    ctx: &egui::Context,
    s: &mut AppState,
    modifiers: Modifiers,
    frames: Vec<Vec<egui::Event>>,
) {
    for events in std::iter::once(vec![]).chain(frames) {
        let mut all = vec![egui::Event::ModifiersChanged(modifiers)];
        all.extend(events);
        let raw = egui::RawInput {
            screen_rect: Some(SCREEN),
            events: all,
            ..Default::default()
        };
        let mut gesture = false;
        ctx.run_ui(raw, |ui| {
            sketch_view::show_overlay(ui, SCREEN, s);
            gesture = ui.input(|i| i.pointer.any_down());
        })
        .drop_without_applying_deltas();
        s.observe_sketch(gesture);
    }
}

fn play(ctx: &egui::Context, s: &mut AppState, frames: Vec<Vec<egui::Event>>) {
    play_with(ctx, s, Modifiers::default(), frames);
}

fn click_at(ctx: &egui::Context, s: &mut AppState, p: (f64, f64)) {
    let at = screen(s, p);
    play(
        ctx,
        s,
        vec![
            vec![moved(at)],
            vec![button(at, true)],
            vec![button(at, false)],
            vec![],
        ],
    );
}

/// Länger warten, als ein Doppelklick dauern darf.
fn wait(ctx: &egui::Context, s: &mut AppState) {
    play(ctx, s, vec![vec![]; 40]);
}

/// Der Zeiger läuft in Schritten von `from` nach `to`.
fn steps(from: egui::Pos2, to: egui::Pos2) -> Vec<Vec<egui::Event>> {
    (1..=6)
        .map(|k| vec![moved(from + (to - from) * (k as f32 / 6.0))])
        .collect()
}

/// Drücken bei `from`, in Schritten nach `to`, ohne Loslassen.
fn press_and_move(s: &AppState, from: (f64, f64), to: (f64, f64)) -> Vec<Vec<egui::Event>> {
    let (from, to) = (screen(s, from), screen(s, to));
    let mut frames = vec![vec![moved(from)], vec![button(from, true)]];
    frames.extend(steps(from, to));
    frames
}

/// Ganzer Zug von `from` nach `to` samt Loslassen.
fn drag(ctx: &egui::Context, s: &mut AppState, from: (f64, f64), to: (f64, f64)) {
    let mut frames = press_and_move(s, from, to);
    frames.push(vec![button(screen(s, to), false)]);
    frames.push(vec![]);
    play(ctx, s, frames);
}

/// Zwei Klicks: erste Linie von `A` nach `B`, der Linienzug läuft bei `B`.
fn first_line(ctx: &egui::Context, s: &mut AppState) -> CurveId {
    click_at(ctx, s, A);
    click_at(ctx, s, B);
    assert_eq!(curves(s).len(), 1);
    assert!(near(pending_xy(s).unwrap(), B));
    curves(s)[0]
}

/// Liegt `p` auf einen halben Pixel genau bei `q`?
fn near(p: DVec2, q: (f64, f64)) -> bool {
    p.distance(DVec2::new(q.0, q.1)) < 0.1
}

fn curves(s: &AppState) -> Vec<CurveId> {
    sk(s).curves().map(|(c, _)| c).collect()
}

fn arcs(s: &AppState) -> Vec<CurveId> {
    sk(s)
        .curves()
        .filter(|(_, d)| matches!(d.kind, CurveKind::Arc { .. }))
        .map(|(c, _)| c)
        .collect()
}

fn lines(s: &AppState) -> Vec<CurveId> {
    sk(s)
        .curves()
        .filter(|(_, d)| matches!(d.kind, CurveKind::Line { .. }))
        .map(|(c, _)| c)
        .collect()
}

fn pending_xy(s: &AppState) -> Option<DVec2> {
    se(s).pending.first().map(|&p| sk(s).point_xy(p).unwrap())
}

/// Sind `a` und `b` über eine `Tangent`-Abhängigkeit verbunden?
fn tangent_between(s: &AppState, a: CurveId, b: CurveId) -> bool {
    sk(s).constraints().any(|(_, c)| {
        matches!(c.kind, ConstraintKind::Tangent(x, y) if (x, y) == (a, b) || (x, y) == (b, a))
    })
}

fn tangents(s: &AppState) -> usize {
    sk(s)
        .constraints()
        .filter(|(_, c)| matches!(c.kind, ConstraintKind::Tangent(..)))
        .count()
}

fn assert_solved(s: &AppState) {
    let report = se(s).last_report.as_ref().expect("Bericht");
    assert_eq!(report.status, SolveStatus::Solved);
    assert!(report.redundant.is_empty(), "{:?}", report.redundant);
}

/// Richtung, in der die Kurve `c` den Punkt `p` verlässt (Fortsetzung über das Ende hinaus).
fn leaving(s: &AppState, c: CurveId, p: PointId) -> DVec2 {
    let sk = sk(s);
    let at = sk.point_xy(p).unwrap();
    let dir = match sk.curve(c).unwrap().kind {
        CurveKind::Line { a, b } if b == p => at - sk.point_xy(a).unwrap(),
        CurveKind::Line { a, b } if a == p => at - sk.point_xy(b).unwrap(),
        CurveKind::Arc { center, end, .. } if end == p => {
            (at - sk.point_xy(center).unwrap()).perp()
        }
        CurveKind::Arc { center, start, .. } if start == p => {
            -(at - sk.point_xy(center).unwrap()).perp()
        }
        ref other => panic!("{other:?} endet nicht in {p:?}"),
    };
    dir.normalize()
}

fn line_ends(s: &AppState, c: CurveId) -> (DVec2, DVec2) {
    let CurveKind::Line { a, b } = sk(s).curve(c).unwrap().kind else {
        panic!("keine Linie");
    };
    (sk(s).point_xy(a).unwrap(), sk(s).point_xy(b).unwrap())
}

/// Stelle `len` von `from` entfernt, um `deg` Grad gegen die Richtung `dir` gedreht.
fn turned(from: DVec2, dir: DVec2, deg: f64, len: f64) -> (f64, f64) {
    let (sin, cos) = deg.to_radians().sin_cos();
    let d = DVec2::new(dir.x * cos - dir.y * sin, dir.x * sin + dir.y * cos);
    let p = from + d * len;
    (p.x, p.y)
}

/// Vergleichbarer Stand: Kurven, Punktlagen und Abhängigkeiten.
fn snapshot(s: &AppState) -> String {
    let sk = sk(s);
    let curves: Vec<String> = sk
        .curves()
        .map(|(id, c)| format!("{} {:?} {}", id.index(), c.kind, c.construction))
        .collect();
    let points: Vec<String> = sk
        .points()
        .map(|(id, _)| format!("{} {:?}", id.index(), sk.point_xy(id).ok()))
        .collect();
    let constraints: Vec<String> = sk
        .constraints()
        .map(|(id, c)| format!("{} {:?}", id.index(), c.kind))
        .collect();
    format!("{curves:?}\n{points:?}\n{constraints:?}")
}

fn undo_len(s: &AppState) -> usize {
    s.sketch_history().unwrap().undo_len()
}

fn type_in(s: &mut AppState, field: usize, text: &str) {
    let scope = s.document.param_scope();
    s.sketch_session
        .as_mut()
        .unwrap()
        .type_field(field, text, &scope);
}

fn hover(ctx: &egui::Context, s: &mut AppState, p: (f64, f64)) {
    let at = screen(s, p);
    play(ctx, s, vec![vec![moved(at)], vec![]]);
}

// ---- Der Bogen ------------------------------------------------------------------------------

#[test]
fn press_on_the_last_point_and_drag_draws_a_tangent_arc() {
    let ctx = context();
    let mut s = rig();
    let line = first_line(&ctx, &mut s);
    drag(&ctx, &mut s, B, E);

    assert_eq!(arcs(&s).len(), 1);
    assert_eq!(lines(&s).len(), 1);
    let arc = arcs(&s)[0];
    assert!(tangent_between(&s, arc, line));
    assert_solved(&s);
    assert!(near(pending_xy(&s).unwrap(), E), "läuft vom Bogenende");
    assert_eq!(se(&s).line_chain, Some(arc));
    assert_eq!(se(&s).line_after_arc, Some(arc));
    assert_eq!(se(&s).line_arc, None);
    assert_eq!(se(&s).tool, Tool::Line);
    // Der Bogen läuft am Anfang genau in Richtung der Linie weiter.
    let CurveKind::Line { b, .. } = sk(&s).curve(line).unwrap().kind else {
        panic!("keine Linie");
    };
    let out = leaving(&s, line, b);
    let into = -leaving(&s, arc, b);
    assert!(out.distance(into) < 1e-6, "{out:?} {into:?}");
}

#[test]
fn an_arc_to_the_right_of_the_line_also_runs_on_from_its_far_end() {
    let ctx = context();
    let mut s = rig();
    let line = first_line(&ctx, &mut s);
    // Rechts der Laufrichtung: Der Bogen liegt gegen den Uhrzeigersinn, also ist `B` sein Ende.
    let right = (40.0, 8.0);
    drag(&ctx, &mut s, B, right);
    let arc = arcs(&s)[0];
    assert!(tangent_between(&s, arc, line));
    assert_solved(&s);
    assert!(near(pending_xy(&s).unwrap(), right));
    let CurveKind::Arc { start, .. } = sk(&s).curve(arc).unwrap().kind else {
        panic!("kein Bogen");
    };
    assert_eq!(se(&s).pending[0], start);
}

#[test]
fn the_next_line_is_exactly_tangent_then_the_one_after_is_free() {
    let ctx = context();
    let mut s = rig();
    first_line(&ctx, &mut s);
    drag(&ctx, &mut s, B, E);
    let arc = arcs(&s)[0];
    let from = pending_xy(&s).unwrap();
    let dir = leaving(&s, arc, se(&s).pending[0]);

    // Der Zeiger steht 20° neben der Tangente.
    click_at(&ctx, &mut s, turned(from, dir, 20.0, 25.0));
    assert_eq!(lines(&s).len(), 2);
    let second = *lines(&s).last().unwrap();
    let (a, b) = line_ends(&s, second);
    assert!(a.distance(from) < 1e-9);
    let along = (b - a).normalize();
    assert!(along.perp_dot(dir).abs() < 1e-9, "{along:?} {dir:?}");
    assert!(along.dot(dir) > 0.0);
    assert!(tangent_between(&s, second, arc));
    assert_solved(&s);
    assert_eq!(se(&s).line_after_arc, None, "die Linie leert es");

    // Die dritte Linie folgt wieder dem Zeiger.
    let tangents_before = tangents(&s);
    let target = turned(b, along, 40.0, 25.0);
    click_at(&ctx, &mut s, target);
    let (_, end) = line_ends(&s, *lines(&s).last().unwrap());
    assert!(near(end, target), "{end:?} {target:?}");
    assert_eq!(tangents(&s), tangents_before);
}

#[test]
fn pointer_behind_the_arc_end_draws_a_free_line() {
    let ctx = context();
    let mut s = rig();
    first_line(&ctx, &mut s);
    drag(&ctx, &mut s, B, E);
    let arc = arcs(&s)[0];
    let from = pending_xy(&s).unwrap();
    let dir = leaving(&s, arc, se(&s).pending[0]);
    let tangents_before = tangents(&s);

    let target = turned(from, dir, 130.0, 25.0);
    click_at(&ctx, &mut s, target);
    assert_eq!(lines(&s).len(), 2);
    let (_, end) = line_ends(&s, *lines(&s).last().unwrap());
    assert!(near(end, target), "{end:?} {target:?}");
    assert_eq!(tangents(&s), tangents_before, "ohne Tangent");
    assert_eq!(se(&s).line_after_arc, None);
}

#[test]
fn ctrl_while_drawing_on_draws_a_free_line() {
    let ctx = context();
    let mut s = rig();
    first_line(&ctx, &mut s);
    drag(&ctx, &mut s, B, E);
    let arc = arcs(&s)[0];
    let from = pending_xy(&s).unwrap();
    let dir = leaving(&s, arc, se(&s).pending[0]);
    let tangents_before = tangents(&s);

    // Fast tangential: Mit Strg gilt trotzdem genau der Zeiger.
    let target = turned(from, dir, 2.0, 25.0);
    let at = screen(&s, target);
    let ctrl = Modifiers::CTRL;
    play_with(
        &ctx,
        &mut s,
        ctrl,
        vec![
            vec![moved(at)],
            vec![mouse(at, PointerButton::Primary, true, ctrl)],
            vec![mouse(at, PointerButton::Primary, false, ctrl)],
            vec![],
        ],
    );
    assert_eq!(lines(&s).len(), 2);
    let (_, end) = line_ends(&s, *lines(&s).last().unwrap());
    let along = (end - from).normalize();
    assert!(along.perp_dot(dir).abs() > 1e-3, "frei, nicht tangential");
    assert_eq!(tangents(&s), tangents_before);
    assert_eq!(se(&s).line_after_arc, None);
}

#[test]
fn two_arcs_one_after_the_other() {
    let ctx = context();
    let mut s = rig();
    let line = first_line(&ctx, &mut s);
    drag(&ctx, &mut s, B, E);
    let first = arcs(&s)[0];
    let from = pending_xy(&s).unwrap();
    let second_end = (22.0, 50.0);
    drag(&ctx, &mut s, (from.x, from.y), second_end);

    assert_eq!(arcs(&s).len(), 2);
    let second = arcs(&s)[1];
    assert!(tangent_between(&s, first, line));
    assert!(tangent_between(&s, second, first));
    assert_solved(&s);
    assert_eq!(se(&s).line_after_arc, Some(second));
    assert!(near(pending_xy(&s).unwrap(), second_end));
}

/// Am Punkt enden zwei Linien; die jüngere gehört nicht zum Linienzug: Der Bogen läuft an der Linie
/// des Zugs weiter, nicht an der jüngsten.
#[test]
fn the_arc_prefers_the_line_of_the_running_chain() {
    let ctx = context();
    let mut s = rig();
    let chain_line = first_line(&ctx, &mut s);
    let young = {
        let (se, sk) = s.session_sketch_mut().unwrap();
        let end = se.pending[0];
        let q = sk.add_point(60.0, -30.0).unwrap();
        sk.add_line(q, end).unwrap()
    };
    s.observe_sketch(false);
    drag(&ctx, &mut s, B, E);
    let arc = arcs(&s)[0];
    assert!(tangent_between(&s, arc, chain_line), "bevorzugt die Kette");
    assert!(!tangent_between(&s, arc, young));
}

#[test]
fn the_preview_during_the_drag_is_the_arc_that_comes_out() {
    let ctx = context();
    let mut s = rig();
    first_line(&ctx, &mut s);
    let frames = press_and_move(&s, B, E);
    play(&ctx, &mut s, frames);
    let at = se(&s).draw_cursor.as_ref().expect("Zeiger").at;
    let preview = se(&s).line_arc_preview(sk(&s), at).expect("Vorschau");
    let end = screen(&s, E);
    play(&ctx, &mut s, vec![vec![button(end, false)], vec![]]);

    let arc = arcs(&s)[0];
    let CurveKind::Arc { center, .. } = sk(&s).curve(arc).unwrap().kind else {
        panic!("kein Bogen");
    };
    let m = sk(&s).point_xy(center).unwrap();
    let r = sk(&s).radius(arc).unwrap();
    let all: Vec<DVec2> = preview.into_iter().flatten().collect();
    assert!(all.len() > 2);
    for p in all {
        assert!((p.distance(m) - r).abs() < 1e-6, "{p:?}");
    }
}

// ---- Wann nichts entsteht -------------------------------------------------------------------

#[test]
fn without_a_previous_curve_a_drag_on_the_start_does_nothing() {
    let ctx = context();
    let mut s = rig();
    click_at(&ctx, &mut s, A);
    let before = snapshot(&s);
    let start = se(&s).pending.clone();
    drag(&ctx, &mut s, A, B);

    assert_eq!(snapshot(&s), before);
    assert_eq!(se(&s).pending, start);
    assert_eq!(se(&s).line_arc, None);

    click_at(&ctx, &mut s, B);
    assert_eq!(lines(&s).len(), 1, "der nächste Klick zeichnet wie heute");
    assert!(arcs(&s).is_empty());
}

#[test]
fn a_drag_elsewhere_in_the_line_tool_does_nothing() {
    let ctx = context();
    let mut s = rig();
    first_line(&ctx, &mut s);
    let before = snapshot(&s);
    drag(&ctx, &mut s, (60.0, -20.0), (80.0, -35.0));
    assert_eq!(snapshot(&s), before);
    assert!(near(pending_xy(&s).unwrap(), B));
    assert_eq!(se(&s).line_arc, None);
}

#[test]
fn releasing_on_the_tangent_reports_and_the_chain_runs_on() {
    let ctx = context();
    let mut s = rig();
    // Waagerecht: Die Lage des Anfangs richtet das Ende genau auf die Tangente aus.
    let (a, b) = ((5.0, 5.0), (25.0, 5.0));
    click_at(&ctx, &mut s, a);
    click_at(&ctx, &mut s, b);
    let before = snapshot(&s);
    let steps = undo_len(&s);
    let from = pending_xy(&s).unwrap();
    drag(&ctx, &mut s, (from.x, from.y), (45.0, from.y));

    assert_eq!(snapshot(&s), before);
    assert_eq!(undo_len(&s), steps);
    assert_eq!(
        se(&s).message_text().as_deref(),
        Some(tr!("sketch-arc-tangent-straight").as_str())
    );
    assert_eq!(pending_xy(&s), Some(from));
    assert_eq!(se(&s).line_arc, None);
    assert_eq!(se(&s).line_after_arc, None);
    assert_eq!(lines(&s).len(), 1);
    click_at(&ctx, &mut s, (40.0, 30.0));
    assert_eq!(lines(&s).len(), 2, "der Linienzug läuft");
}

// ---- Klick unverändert ----------------------------------------------------------------------

/// Eine Klickfolge legt genau das an, was sie heute anlegt: Linien mit gemeinsamen Punkten und nur
/// die Abhängigkeiten der Richtung.
#[test]
fn a_click_sequence_stays_as_it_is() {
    let ctx = context();
    let mut s = rig();
    let path = [A, B, (45.0, 3.0), (70.0, 40.0)];
    for p in path {
        click_at(&ctx, &mut s, p);
    }
    assert_eq!(lines(&s).len(), 3);
    assert_eq!(sk(&s).points().count(), 4);
    assert_eq!(sk(&s).constraints().count(), 0);
    assert!(arcs(&s).is_empty());
    assert_eq!(se(&s).line_after_arc, None);
    for (&c, (from, to)) in lines(&s).iter().zip(path.iter().zip(&path[1..])) {
        let (a, b) = line_ends(&s, c);
        assert!(near(a, *from) && near(b, *to), "{a:?} {b:?}");
    }
    let chain = lines(&s);
    assert_eq!(line_ends(&s, chain[0]).1, line_ends(&s, chain[1]).0);
    assert_eq!(se(&s).line_chain, Some(chain[2]));
}

#[test]
fn a_click_on_the_last_point_reports_as_before() {
    let ctx = context();
    let mut s = rig();
    first_line(&ctx, &mut s);
    wait(&ctx, &mut s);
    let before = snapshot(&s);
    let at = pending_xy(&s).unwrap();
    click_at(&ctx, &mut s, (at.x, at.y));
    assert_eq!(snapshot(&s), before);
    assert!(se(&s).message.is_some(), "eine Linie der Länge null meldet");
    assert_eq!(pending_xy(&s), Some(at));
    assert!(arcs(&s).is_empty());
}

#[test]
fn a_double_click_ends_the_chain() {
    let ctx = context();
    let mut s = rig();
    first_line(&ctx, &mut s);
    wait(&ctx, &mut s);
    let p = (45.0, 3.0);
    click_at(&ctx, &mut s, p);
    click_at(&ctx, &mut s, p);
    assert!(se(&s).pending.is_empty());
    assert_eq!(se(&s).line_chain, None);
    assert!(arcs(&s).is_empty());
}

#[test]
fn movement_under_the_click_limit_stays_a_click() {
    let ctx = context();
    let mut s = rig();
    first_line(&ctx, &mut s);
    wait(&ctx, &mut s);
    let at = screen(&s, B);
    let near_by = at + egui::vec2(3.0, 2.0);
    play(
        &ctx,
        &mut s,
        vec![
            vec![moved(at)],
            vec![button(at, true)],
            vec![moved(near_by)],
            vec![button(near_by, false)],
            vec![],
        ],
    );
    assert!(arcs(&s).is_empty());
    assert_eq!(se(&s).line_arc, None);
    assert_eq!(lines(&s).len(), 1, "gefangen auf dem letzten Punkt");
    assert!(se(&s).message.is_some(), "wie ein Klick auf den Punkt");
}

// ---- Andere Züge ----------------------------------------------------------------------------

#[test]
fn shift_drag_on_the_last_point_pans() {
    let ctx = context();
    let mut s = rig();
    first_line(&ctx, &mut s);
    let before = snapshot(&s);
    let target = s.camera.target;
    let (from, to) = (screen(&s, B), screen(&s, E));
    let shift = Modifiers::SHIFT;
    let mut frames = vec![
        vec![moved(from)],
        vec![mouse(from, PointerButton::Primary, true, shift)],
    ];
    frames.extend(steps(from, to));
    frames.push(vec![mouse(to, PointerButton::Primary, false, shift)]);
    frames.push(vec![]);
    play_with(&ctx, &mut s, shift, frames);
    assert_eq!(snapshot(&s), before);
    assert_ne!(s.camera.target, target, "geschwenkt");
    assert_eq!(se(&s).line_arc, None);
}

#[test]
fn middle_drag_on_the_last_point_pans() {
    let ctx = context();
    let mut s = rig();
    first_line(&ctx, &mut s);
    let before = snapshot(&s);
    let target = s.camera.target;
    let (from, to) = (screen(&s, B), screen(&s, E));
    let none = Modifiers::default();
    let mut frames = vec![
        vec![moved(from)],
        vec![mouse(from, PointerButton::Middle, true, none)],
    ];
    frames.extend(steps(from, to));
    frames.push(vec![mouse(to, PointerButton::Middle, false, none)]);
    frames.push(vec![]);
    play(&ctx, &mut s, frames);
    assert_eq!(snapshot(&s), before);
    assert_ne!(s.camera.target, target, "geschwenkt");
    assert_eq!(se(&s).line_arc, None);
}

/// Umschalt mitten im Zug schwenkt nicht und nimmt ihm nichts (wie beim Rahmen).
#[test]
fn shift_during_the_drag_does_not_pan() {
    let ctx = context();
    let mut s = rig();
    first_line(&ctx, &mut s);
    let mid = (32.0, 20.0);
    let frames = press_and_move(&s, B, mid);
    play(&ctx, &mut s, frames);
    assert!(se(&s).line_arc.is_some());
    let target = s.camera.target;
    let (from, end) = (screen(&s, mid), screen(&s, E));
    play_with(&ctx, &mut s, Modifiers::SHIFT, steps(from, end));
    assert_eq!(s.camera.target, target);
    play(&ctx, &mut s, vec![vec![button(end, false)], vec![]]);
    assert_eq!(arcs(&s).len(), 1);
}

#[test]
fn drag_in_the_select_tool_still_moves_the_curve() {
    let ctx = context();
    let mut s = rig_with(|sk| {
        let p = sk.add_point(5.0, 5.0).unwrap();
        let q = sk.add_point(25.0, 12.0).unwrap();
        sk.add_line(p, q).unwrap();
    });
    {
        let (se, sk) = s.session_sketch_mut().unwrap();
        se.set_tool(sk, Tool::Select);
    }
    let line = curves(&s)[0];
    let before = line_ends(&s, line);
    drag(&ctx, &mut s, (15.0, 8.5), (15.0, 28.5));
    let after = line_ends(&s, line);
    assert!(after.0.distance(before.0) > 1.0);
    assert!(arcs(&s).is_empty());
    assert_eq!(se(&s).line_arc, None);
}

// ---- Abbruch --------------------------------------------------------------------------------

#[test]
fn esc_during_the_drag_drops_only_the_drag() {
    let ctx = context();
    let mut s = rig();
    first_line(&ctx, &mut s);
    let before = snapshot(&s);
    let frames = press_and_move(&s, B, E);
    play(&ctx, &mut s, frames);
    assert!(se(&s).line_arc.is_some(), "der Zug läuft");

    let end = screen(&s, E);
    play(
        &ctx,
        &mut s,
        vec![vec![esc()], vec![button(end, false)], vec![]],
    );
    assert_eq!(snapshot(&s), before, "kein Bogen");
    assert_eq!(se(&s).line_arc, None);
    assert!(near(pending_xy(&s).unwrap(), B), "der Linienzug läuft");
    assert!(se(&s).line_chain.is_some());

    play(&ctx, &mut s, vec![vec![esc()], vec![]]);
    assert!(se(&s).pending.is_empty(), "Esc danach beendet ihn");
}

#[test]
fn right_press_during_the_drag_drops_only_the_drag() {
    let ctx = context();
    let mut s = rig();
    first_line(&ctx, &mut s);
    let before = snapshot(&s);
    let frames = press_and_move(&s, B, E);
    play(&ctx, &mut s, frames);
    assert!(se(&s).line_arc.is_some());

    let end = screen(&s, E);
    let none = Modifiers::default();
    play(
        &ctx,
        &mut s,
        vec![
            vec![mouse(end, PointerButton::Secondary, true, none)],
            vec![mouse(end, PointerButton::Secondary, false, none)],
            vec![button(end, false)],
            vec![],
        ],
    );
    assert_eq!(snapshot(&s), before, "kein Bogen");
    assert_eq!(se(&s).line_arc, None);
    assert!(near(pending_xy(&s).unwrap(), B), "der Linienzug läuft");

    // Der nächste Rechtsklick beendet den Linienzug.
    play(
        &ctx,
        &mut s,
        vec![
            vec![mouse(end, PointerButton::Secondary, true, none)],
            vec![mouse(end, PointerButton::Secondary, false, none)],
            vec![],
        ],
    );
    assert!(se(&s).pending.is_empty());
}

// ---- Felder und Zeichen ---------------------------------------------------------------------

#[test]
fn during_the_drag_no_fields_and_no_direction_marks_then_fresh_fields() {
    let ctx = context();
    let mut s = rig();
    first_line(&ctx, &mut s);
    // Vor dem Zug hat die Stufe Felder, und eine Länge ist getippt.
    hover(&ctx, &mut s, (40.0, 20.0));
    assert_eq!(se(&s).input.fields.len(), 2);
    type_in(&mut s, 0, "12");

    // Fast waagerecht zum Punkt: Ohne den Zug gäbe das ein Richtungszeichen.
    let y = pending_xy(&s).unwrap().y;
    let flat = (45.0, y + 0.3);
    let frames = press_and_move(&s, B, flat);
    play(&ctx, &mut s, frames);
    assert!(se(&s).line_arc.is_some());
    assert!(se(&s).input.fields.is_empty(), "keine Felder");
    let cur = se(&s).draw_cursor.as_ref().expect("Zeiger");
    assert!(cur.autos.is_empty(), "keine Richtung: {:?}", cur.autos);
    assert!(sketch_view::auto_marks(sk(&s), se(&s)).is_empty());

    let target = screen(&s, E);
    play(
        &ctx,
        &mut s,
        vec![vec![moved(target)], vec![button(target, false)], vec![]],
    );
    assert_eq!(arcs(&s).len(), 1);
    // Die Länge von vorhin gilt nicht mehr: Das Ende liegt am Zeiger.
    assert!(near(pending_xy(&s).unwrap(), E));
    let fields = &se(&s).input.fields;
    assert_eq!(
        fields.iter().map(|f| f.kind).collect::<Vec<_>>(),
        [FieldKind::Length, FieldKind::Angle]
    );
    assert!(
        fields
            .iter()
            .all(|f| !f.touched && f.text.is_empty() && f.locked.is_none())
    );
}

#[test]
fn after_esc_the_fields_come_back_empty() {
    let ctx = context();
    let mut s = rig();
    first_line(&ctx, &mut s);
    hover(&ctx, &mut s, (40.0, 20.0));
    type_in(&mut s, 0, "12");
    let frames = press_and_move(&s, B, E);
    play(&ctx, &mut s, frames);
    let end = screen(&s, E);
    play(
        &ctx,
        &mut s,
        vec![
            vec![esc()],
            vec![button(end, false)],
            vec![moved(end)],
            vec![],
        ],
    );
    let fields = &se(&s).input.fields;
    assert_eq!(fields.len(), 2);
    assert!(
        fields
            .iter()
            .all(|f| !f.touched && f.text.is_empty() && f.locked.is_none())
    );
}

/// Eine getippte Länge lässt die Richtung tangential und wird ein Maß.
#[test]
fn a_typed_length_keeps_the_line_tangent() {
    let ctx = context();
    let mut s = rig();
    first_line(&ctx, &mut s);
    drag(&ctx, &mut s, B, E);
    let arc = arcs(&s)[0];
    let from = pending_xy(&s).unwrap();
    let dir = leaving(&s, arc, se(&s).pending[0]);
    let target = turned(from, dir, 15.0, 25.0);
    hover(&ctx, &mut s, target);
    type_in(&mut s, 0, "12");
    click_at(&ctx, &mut s, target);

    let second = *lines(&s).last().unwrap();
    let (a, b) = line_ends(&s, second);
    assert!((a.distance(b) - 12.0).abs() < 1e-6);
    assert!(tangent_between(&s, second, arc));
    assert!((b - a).normalize().perp_dot(dir).abs() < 1e-6);
    assert_solved(&s);
}

/// Ein getippter Winkel löscht die Tangente wie heute. Zum Bogen gibt es kein Winkelmaß, also zählt
/// er wie am Anfang eines Linienzugs zur x-Achse.
#[test]
fn a_typed_angle_after_the_arc_drops_the_tangent() {
    let ctx = context();
    let mut s = rig();
    first_line(&ctx, &mut s);
    drag(&ctx, &mut s, B, E);
    let tangents_before = tangents(&s);
    let from = pending_xy(&s).unwrap();
    let target = (from.x + 15.0, from.y + 20.0);
    hover(&ctx, &mut s, target);
    type_in(&mut s, 1, "30");
    click_at(&ctx, &mut s, target);

    assert_eq!(se(&s).message_text(), None);
    let second = lines(&s)
        .into_iter()
        .find(|&c| line_ends(&s, c).0.distance(from) < 1e-9)
        .expect("die Linie nach dem Bogen");
    let (a, b) = line_ends(&s, second);
    let along = (b - a).normalize();
    assert!(
        (along.y.atan2(along.x).to_degrees() - 30.0).abs() < 1e-6,
        "{along:?}"
    );
    assert_eq!(tangents(&s), tangents_before);
    assert!(
        sk(&s)
            .constraints()
            .any(|(_, c)| matches!(c.kind, ConstraintKind::Angle(..)))
    );
    assert_solved(&s);
    assert_eq!(se(&s).line_after_arc, None);
}

// ---- Fang ------------------------------------------------------------------------------------

#[test]
fn a_snap_at_the_arc_end_binds() {
    let ctx = context();
    let mut q = None;
    let mut s = rig_with(|sk| q = Some(sk.add_point(E.0, E.1).unwrap()));
    let q = q.unwrap();
    first_line(&ctx, &mut s);
    let points = sk(&s).points().count();
    drag(&ctx, &mut s, B, E);

    let arc = arcs(&s)[0];
    assert_eq!(
        se(&s).pending,
        [q],
        "der Linienzug läuft vom gefangenen Punkt"
    );
    assert!(sk(&s).curve(arc).unwrap().kind.points().contains(&q));
    assert_eq!(
        sk(&s).points().count(),
        points + 1,
        "nur die Mitte kommt dazu"
    );
    assert_solved(&s);
}

// ---- Rückgängig ------------------------------------------------------------------------------

#[test]
fn undo_takes_the_arc_and_the_line_in_their_own_steps() {
    let ctx = context();
    let mut s = rig();
    let first = first_line(&ctx, &mut s);
    let before_arc = snapshot(&s);
    let steps = undo_len(&s);
    drag(&ctx, &mut s, B, E);
    assert_eq!(undo_len(&s), steps + 1);
    assert_eq!(s.sketch_undo_label().as_deref(), Some("Bogen zeichnen"));
    let before_line = snapshot(&s);

    let from = pending_xy(&s).unwrap();
    let dir = leaving(&s, arcs(&s)[0], se(&s).pending[0]);
    click_at(&ctx, &mut s, turned(from, dir, 10.0, 25.0));
    assert_eq!(undo_len(&s), steps + 2);
    assert_eq!(s.sketch_undo_label().as_deref(), Some("Linie zeichnen"));

    assert!(s.undo_sketch());
    assert_eq!(snapshot(&s), before_line, "nur die Linie geht");
    assert!(s.undo_sketch());
    assert_eq!(snapshot(&s), before_arc, "der Bogen geht ganz");
    assert_eq!(lines(&s), [first], "die Linie davor bleibt");
    assert!(arcs(&s).is_empty());
}

#[test]
fn undo_during_the_drag_ends_it() {
    let ctx = context();
    let mut s = rig();
    first_line(&ctx, &mut s);
    let frames = press_and_move(&s, B, E);
    play(&ctx, &mut s, frames);
    assert!(se(&s).line_arc.is_some());
    assert!(s.undo_sketch());
    assert_eq!(se(&s).line_arc, None);
    let end = screen(&s, E);
    play(&ctx, &mut s, vec![vec![button(end, false)], vec![]]);
    assert!(arcs(&s).is_empty());
}

// ---- Feste Geometrie -------------------------------------------------------------------------

#[test]
fn an_arc_on_a_line_with_fixed_ends_is_solved_and_moves_nothing() {
    let ctx = context();
    let mut ends = None;
    let mut s = rig_with(|sk| {
        let p = sk.add_point(A.0, A.1).unwrap();
        let q = sk.add_point(B.0, B.1).unwrap();
        sk.add_line(p, q).unwrap();
        sk.add_constraint(ConstraintKind::Fixed(p, A.0, A.1))
            .unwrap();
        sk.add_constraint(ConstraintKind::Fixed(q, B.0, B.1))
            .unwrap();
        ends = Some((p, q));
    });
    let (p, q) = ends.unwrap();
    // Der Klick auf das Linienende fängt es: Die Linie ist der Vorgänger, ohne Linienzug.
    click_at(&ctx, &mut s, B);
    assert_eq!(se(&s).pending, [q]);
    wait(&ctx, &mut s);
    drag(&ctx, &mut s, B, E);

    assert_eq!(arcs(&s).len(), 1);
    assert_solved(&s);
    let report = se(&s).last_report.as_ref().unwrap();
    assert_eq!(report.iterations, 0, "keine Bewegung");
    assert_eq!(sk(&s).point_xy(p).unwrap(), DVec2::new(A.0, A.1));
    assert_eq!(sk(&s).point_xy(q).unwrap(), DVec2::new(B.0, B.1));
    assert!(near(pending_xy(&s).unwrap(), E));
}
