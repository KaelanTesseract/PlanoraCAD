//! Stutzen, Dehnen und Brechen in der App (Skizzen 1, Task 10): Klick und Ziehen in der Ansicht,
//! Vorschau beim Überfahren, Rückgängig mit Schrittnamen, Esc während des Ziehens, Hinweis ohne
//! Ziel, die Leiste mit zwölf Knöpfen und die englischen Namen.
//!
//! Ohne Fenster, bei 1920 × 1080: das Overlay der Skizze bekommt Rohereignisse, nach jedem Bild
//! wird wie in der App der Verlauf der Skizze fortgeschrieben (Geste = Maustaste gedrückt).

use planora_app::sketch_panel;
use planora_app::sketch_state::Tool;
use planora_app::state::{AppState, StandardPlane};
use planora_app::{fonts, sketch_view};
use planora_feature::SketchPlane;
use planora_math::DVec2;
use planora_sketch::model::{CurveId, CurveKind, Sketch};

const W: f32 = 1920.0;
const H: f32 = 1080.0;
const SCREEN: egui::Rect = egui::Rect {
    min: egui::pos2(0.0, 0.0),
    max: egui::pos2(W, H),
};

fn context() -> egui::Context {
    let ctx = egui::Context::default();
    ctx.set_fonts(fonts::definitions());
    ctx
}

/// Offene, leere Skizze auf XY; `build` legt den Inhalt an, danach ist der Stand ein Bild des
/// Verlaufs (wie nach dem Zeichnen).
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

fn line(sk: &mut Sketch, a: (f64, f64), b: (f64, f64)) -> CurveId {
    let p = sk.add_point(a.0, a.1).unwrap();
    let q = sk.add_point(b.0, b.1).unwrap();
    sk.add_line(p, q).unwrap()
}

fn set_tool(s: &mut AppState, tool: Tool) {
    let (se, sk) = s.session_sketch_mut().unwrap();
    se.set_tool(sk, tool);
}

/// Bildschirmlage eines Skizzenpunkts.
fn screen(s: &AppState, p: (f64, f64)) -> egui::Pos2 {
    let frame = *s.session_sketch().unwrap().frame();
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

fn button(p: egui::Pos2, pressed: bool) -> egui::Event {
    egui::Event::PointerButton {
        pos: p,
        button: egui::PointerButton::Primary,
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

/// Spielt je Eintrag ein Bild gegen das Overlay ab und schreibt danach den Verlauf fort wie die
/// App (`observe_sketch` mit „Maustaste gedrückt“ als Geste). Ein leeres Bild vorweg: egui ordnet
/// Zeiger-Ereignisse den Flächen des vorigen Bilds zu.
fn play(ctx: &egui::Context, s: &mut AppState, frames: Vec<Vec<egui::Event>>) {
    for events in std::iter::once(vec![]).chain(frames) {
        let raw = egui::RawInput {
            screen_rect: Some(SCREEN),
            events,
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

fn click(ctx: &egui::Context, s: &mut AppState, p: egui::Pos2) {
    play(
        ctx,
        s,
        vec![
            vec![moved(p)],
            vec![button(p, true)],
            vec![button(p, false)],
            vec![],
        ],
    );
}

/// Zug mit gedrückter linker Taste von `from` nach `to` in `n` Schritten (ohne Loslassen).
fn drag_frames(from: egui::Pos2, to: egui::Pos2, n: usize) -> Vec<Vec<egui::Event>> {
    let mut frames = vec![vec![moved(from)], vec![button(from, true)]];
    for k in 1..=n {
        let t = k as f32 / n as f32;
        frames.push(vec![moved(from + (to - from) * t)]);
    }
    frames
}

fn line_ends(sk: &Sketch, c: CurveId) -> Option<(DVec2, DVec2)> {
    match sk.curve(c)?.kind {
        CurveKind::Line { a, b } => Some((sk.point_xy(a).ok()?, sk.point_xy(b).ok()?)),
        _ => None,
    }
}

/// Vergleichbarer Stand: Kurven, Punktlagen und Abhängigkeiten.
fn snapshot(s: &AppState) -> String {
    let sk = s.session_sketch().unwrap();
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
    format!(
        "{curves:?}
{points:?}
{constraints:?}"
    )
}

fn undo_len(s: &AppState) -> usize {
    s.sketch_history().unwrap().undo_len()
}

/// Ein Kreuz: waagerechte Linie (-20,0)–(20,0) und senkrechte (0,-20)–(0,20).
fn cross() -> (AppState, CurveId, CurveId) {
    let mut ids = Vec::new();
    let s = sketch_with(|sk| {
        ids.push(line(sk, (-20.0, 0.0), (20.0, 0.0)));
        ids.push(line(sk, (0.0, -20.0), (0.0, 20.0)));
    });
    (s, ids[0], ids[1])
}

#[test]
fn clicking_a_cross_arm_trims_it_and_undo_restores_it() {
    let ctx = context();
    let (mut s, h, v) = cross();
    let before = snapshot(&s);
    let steps = undo_len(&s);
    set_tool(&mut s, Tool::Trim);

    // Vorschau beim Überfahren: das rechte Stück von 0 bis 20.
    let at = screen(&s, (10.0, 0.0));
    play(&ctx, &mut s, vec![vec![moved(at)], vec![moved(at)]]);
    let preview = s
        .sketch_session
        .as_ref()
        .unwrap()
        .hover_preview
        .clone()
        .expect("Vorschau über dem Stück");
    assert!(preview.len() >= 2);
    for p in &preview {
        assert!(
            p.y.abs() < 1e-6 && p.x > -1e-6 && p.x < 20.0 + 1e-6,
            "{p:?}"
        );
    }
    // Neben allen Kurven: keine Vorschau.
    let away = screen(&s, (12.0, 12.0));
    play(&ctx, &mut s, vec![vec![moved(away)]]);
    assert!(s.sketch_session.as_ref().unwrap().hover_preview.is_none());

    click(&ctx, &mut s, at);
    let sk = s.session_sketch().unwrap();
    let (a, b) = line_ends(sk, h).expect("linker Arm bleibt");
    assert!(a.distance(DVec2::new(-20.0, 0.0)) < 1e-6, "{a:?}");
    assert!(b.distance(DVec2::ZERO) < 1e-6, "{b:?}");
    assert!(line_ends(sk, v).is_some(), "die senkrechte Linie bleibt");
    assert_eq!(undo_len(&s), steps + 1, "ein Schritt");
    assert_eq!(s.sketch_undo_label().as_deref(), Some("Stutzen"));
    assert_eq!(s.sketch_session.as_ref().unwrap().tool, Tool::Trim);

    assert!(s.undo_sketch());
    assert_eq!(snapshot(&s), before);
}

#[test]
fn dragging_over_three_curves_trims_all_three_in_one_step() {
    let ctx = context();
    // Drei senkrechte Linien, unten von einer waagerechten gekreuzt (bei y = 0).
    let mut ids = Vec::new();
    let mut s = sketch_with(|sk| {
        line(sk, (-20.0, 0.0), (20.0, 0.0));
        for x in [-10.0, 0.0, 10.0] {
            ids.push(line(sk, (x, -10.0), (x, 10.0)));
        }
    });
    let before = snapshot(&s);
    let steps = undo_len(&s);
    set_tool(&mut s, Tool::Trim);

    let from = screen(&s, (-15.0, 5.0));
    let to = screen(&s, (15.0, 5.0));
    let mut frames = drag_frames(from, to, 12);
    frames.push(vec![button(to, false)]);
    frames.push(vec![]);
    play(&ctx, &mut s, frames);

    let sk = s.session_sketch().unwrap();
    for (&c, x) in ids.iter().zip([-10.0, 0.0, 10.0]) {
        let (a, b) = line_ends(sk, c).expect("unteres Stück bleibt");
        let top = a.y.max(b.y);
        assert!(
            top.abs() < 1e-6,
            "Linie bei x = {x}: oberes Stück gestutzt ({a:?}, {b:?})"
        );
    }
    assert_eq!(
        undo_len(&s),
        steps + 1,
        "Ziehen über drei Kurven ist ein Schritt"
    );
    assert_eq!(s.sketch_undo_label().as_deref(), Some("Stutzen"));
    assert!(s.undo_sketch());
    assert_eq!(snapshot(&s), before, "ein Rückgängig stellt alle drei her");
}

#[test]
fn esc_while_dragging_ends_the_tool_and_keeps_what_was_trimmed() {
    let ctx = context();
    let mut ids = Vec::new();
    let mut s = sketch_with(|sk| {
        line(sk, (-20.0, 0.0), (20.0, 0.0));
        for x in [-10.0, 0.0, 10.0] {
            ids.push(line(sk, (x, -10.0), (x, 10.0)));
        }
    });
    let steps = undo_len(&s);
    set_tool(&mut s, Tool::Trim);

    // Über die ersten beiden Linien ziehen, Esc, dann weiter über die dritte und loslassen.
    let from = screen(&s, (-15.0, 5.0));
    let mid = screen(&s, (5.0, 5.0));
    let to = screen(&s, (15.0, 5.0));
    let mut frames = drag_frames(from, mid, 8);
    frames.push(vec![esc()]);
    for k in 1..=4 {
        frames.push(vec![moved(mid + (to - mid) * (k as f32 / 4.0))]);
    }
    frames.push(vec![button(to, false)]);
    frames.push(vec![]);
    play(&ctx, &mut s, frames);

    assert_eq!(s.sketch_session.as_ref().unwrap().tool, Tool::Select);
    let sk = s.session_sketch().unwrap();
    let top = |c: CurveId| {
        let (a, b) = line_ends(sk, c).unwrap();
        a.y.max(b.y)
    };
    assert!(top(ids[0]).abs() < 1e-6, "erste Linie gestutzt");
    assert!(top(ids[1]).abs() < 1e-6, "zweite Linie gestutzt");
    assert!(
        (top(ids[2]) - 10.0).abs() < 1e-6,
        "dritte Linie nach Esc unberührt"
    );
    assert_eq!(undo_len(&s), steps + 1);
    assert_eq!(s.sketch_undo_label().as_deref(), Some("Stutzen"));
}

#[test]
fn dragging_close_past_an_intersection_trims_only_what_the_path_touches() {
    // Kreuz am Ursprung; Fangradius 1. Der Zug kommt von oben rechts, endet ein Bild lang bei
    // P = (0,5 | 0,7) (nächste Kurve dort: die senkrechte Linie, ihr oberes Stück wird
    // gestutzt) und geht dann wieder nach oben rechts. Die waagerechte Linie berührt der Weg nur
    // an P, wo schon gestutzt wurde: sie bleibt.
    let (mut s, h, v) = cross();
    set_tool(&mut s, Tool::Trim);
    let (se, sk) = s.session_sketch_mut().unwrap();
    se.begin_trim_drag(sk, DVec2::new(5.0, 5.0), 1.0);
    se.trim_drag_to(sk, DVec2::new(0.5, 0.7), 1.0);
    let (a, b) = line_ends(sk, v).unwrap();
    assert!(
        a.y.max(b.y).abs() < 1e-6,
        "oberes Stück der Senkrechten gestutzt"
    );
    se.trim_drag_to(sk, DVec2::new(5.0, 5.0), 1.0);
    se.end_trim_drag();
    let (a, b) = line_ends(sk, h).unwrap();
    assert!(a.distance(DVec2::new(-20.0, 0.0)) < 1e-6, "{a:?}");
    assert!(
        b.distance(DVec2::new(20.0, 0.0)) < 1e-6,
        "waagerechte Linie unberührt: {b:?}"
    );
    let (a, b) = line_ends(sk, v).unwrap();
    assert!(a.y.min(b.y) < -19.0, "unteres Stück der Senkrechten bleibt");
}

#[test]
fn extend_without_target_shows_the_hint_and_changes_nothing() {
    let ctx = context();
    let mut s = sketch_with(|sk| {
        line(sk, (0.0, 0.0), (20.0, 0.0));
    });
    let before = snapshot(&s);
    let steps = undo_len(&s);
    set_tool(&mut s, Tool::Extend);
    let at = screen(&s, (18.0, 0.0));
    click(&ctx, &mut s, at);
    assert_eq!(snapshot(&s), before);
    assert_eq!(undo_len(&s), steps);
    assert_eq!(
        s.sketch_session.as_ref().unwrap().message_text().as_deref(),
        Some(planora_i18n::tr!("sketch-extend-no-hit").as_str())
    );
}

#[test]
fn extend_reaches_the_next_curve_with_preview_and_undo_name() {
    let ctx = context();
    let mut ids = Vec::new();
    let mut s = sketch_with(|sk| {
        ids.push(line(sk, (0.0, 0.0), (10.0, 0.0)));
        ids.push(line(sk, (20.0, -10.0), (20.0, 10.0)));
    });
    set_tool(&mut s, Tool::Extend);
    let at = screen(&s, (9.0, 0.0));
    play(&ctx, &mut s, vec![vec![moved(at)], vec![moved(at)]]);
    let preview = s.sketch_session.as_ref().unwrap().hover_preview.clone();
    let preview = preview.expect("Vorschau des Dehnens");
    assert!(preview.last().unwrap().distance(DVec2::new(20.0, 0.0)) < 1e-6);
    click(&ctx, &mut s, at);
    let (a, b) = line_ends(s.session_sketch().unwrap(), ids[0]).unwrap();
    assert!(a.distance(DVec2::ZERO) < 1e-6);
    assert!(b.distance(DVec2::new(20.0, 0.0)) < 1e-6, "{b:?}");
    assert_eq!(s.sketch_undo_label().as_deref(), Some("Dehnen"));
}

#[test]
fn break_splits_the_clicked_curve_with_undo_name() {
    let ctx = context();
    let (mut s, _, _) = cross();
    let curves = s.session_sketch().unwrap().curves().count();
    set_tool(&mut s, Tool::Break);
    let at = screen(&s, (10.0, 0.0));
    click(&ctx, &mut s, at);
    assert_eq!(s.session_sketch().unwrap().curves().count(), curves + 1);
    assert_eq!(s.sketch_undo_label().as_deref(), Some("Brechen"));
}

#[test]
fn the_strip_shows_fourteen_tools_that_fit_at_1080() {
    let ctx = context();
    let (mut s, _, _) = cross();
    for _ in 0..3 {
        let raw = egui::RawInput {
            screen_rect: Some(SCREEN),
            ..Default::default()
        };
        ctx.run_ui(raw, |ui| sketch_panel::show(ui.ctx(), &mut s))
            .drop_without_applying_deltas();
    }
    let buttons = sketch_panel::strip_buttons(&ctx);
    let names: Vec<&str> = buttons.iter().map(|(n, _)| n.as_str()).collect();
    // Skizzen 3: Polygon und Langloch kamen dazu.
    assert_eq!(names.len(), 14, "{names:?}");
    assert_eq!(&names[11..], ["Stutzen", "Dehnen", "Brechen"]);
    for (name, r) in &buttons {
        assert!(r.top() >= 0.0 && r.bottom() <= H, "{name}: {r:?}");
        assert_eq!(r.height(), 36.0, "{name}");
    }
    // Trennlinie vor „Stutzen“: größerer Abstand als zwischen den übrigen Knöpfen.
    let gap = |i: usize| buttons[i + 1].1.top() - buttons[i].1.bottom();
    assert!((gap(9) - 4.0).abs() < 0.01, "{}", gap(9));
    assert!((gap(10) - 13.0).abs() < 0.01, "{}", gap(10));
}

#[test]
fn english_names_are_trim_extend_break() {
    use planora_i18n::{english, with_lang};
    with_lang(&english(), || {
        assert_eq!(sketch_panel::tool_label(Tool::Trim), "Trim");
        assert_eq!(sketch_panel::tool_label(Tool::Extend), "Extend");
        assert_eq!(sketch_panel::tool_label(Tool::Break), "Break");
        assert_eq!(planora_app::keymap::Action::SketchTrim.label(), "Trim");
        for t in [Tool::Trim, Tool::Extend, Tool::Break] {
            assert!(!sketch_panel::tool_hint(t).is_empty());
        }
    });
    assert_eq!(sketch_panel::tool_label(Tool::Trim), "Stutzen");
    assert_eq!(sketch_panel::tool_label(Tool::Extend), "Dehnen");
    assert_eq!(sketch_panel::tool_label(Tool::Break), "Brechen");
}
