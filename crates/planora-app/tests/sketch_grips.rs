//! Skizzen 4, Task 9: Griffe von Verschieben/Kopieren. Die reine Rechnung und über Rohereignisse bei
//! 1920×1080 gegen das Overlay: Griffe, Rahmen, Kurvenzug, Liste und Schwenken kommen sich nicht in
//! die Quere.

mod common;

use eframe::App as _;
use egui::{Event, Key, Modifiers, PointerButton, Pos2, Rect, Vec2, pos2, vec2};
use planora_app::app::PlanoraApp;
use planora_app::sketch_grips::{
    GRIP_ARROW_PX, GRIP_HIT_PX, GRIP_RING_PX, Grip, GripDrag, GripLayout, grip_motion, hit, layout,
    unwrapped_angle,
};
use planora_app::sketch_state::{Item, Tool, TransformStage};
use planora_app::state::{AppState, StandardPlane};
use planora_app::workspace::Workspace;
use planora_app::{fonts, sketch_panel, sketch_view, theme};
use planora_feature::SketchPlane;
use planora_math::DVec2;
use planora_project::config::Config;
use planora_project::library::Library;
use planora_sketch::model::{CurveId, Sketch};
use std::f64::consts::TAU;

const W: f32 = 1920.0;
const H: f32 = 1080.0;
const SCREEN: Rect = Rect {
    min: pos2(0.0, 0.0),
    max: pos2(W, H),
};

fn v(x: f64, y: f64) -> DVec2 {
    DVec2::new(x, y)
}

// ---- Trefferprüfung ---------------------------------------------------------------------------

fn straight() -> GripLayout {
    layout(pos2(500.0, 400.0), vec2(1.0, 0.0), vec2(0.0, -1.0), false)
}

#[test]
fn each_grip_is_hit_in_pixels_with_its_margin() {
    let l = straight();
    let c = pos2(500.0, 400.0);
    assert_eq!(l.center, c);
    assert_eq!(l.x_tip, pos2(500.0 + GRIP_ARROW_PX, 400.0));
    assert_eq!(l.y_tip, pos2(500.0, 400.0 - GRIP_ARROW_PX));
    assert_eq!(l.ring, GRIP_RING_PX);

    assert_eq!(hit(&l, c), Some(Grip::Free));
    assert_eq!(hit(&l, c + vec2(GRIP_HIT_PX, 0.0)), Some(Grip::Free));
    assert_eq!(hit(&l, c + vec2(-GRIP_HIT_PX, 0.0)), Some(Grip::Free));
    assert_eq!(hit(&l, c + vec2(55.0, 0.0)), Some(Grip::AxisX));
    assert_eq!(hit(&l, c + vec2(55.0, GRIP_HIT_PX)), Some(Grip::AxisX));
    assert_eq!(hit(&l, c + vec2(55.0, GRIP_HIT_PX + 0.5)), None);
    assert_eq!(hit(&l, c + vec2(0.0, -55.0)), Some(Grip::AxisY));
    assert_eq!(hit(&l, c + vec2(GRIP_HIT_PX + 0.5, -55.0)), None);
    // Auf dem Ring links und unten, fern der Pfeile.
    assert_eq!(hit(&l, c + vec2(-GRIP_RING_PX, 0.0)), Some(Grip::Rotate));
    assert_eq!(hit(&l, c + vec2(0.0, GRIP_RING_PX)), Some(Grip::Rotate));
    let d = (GRIP_RING_PX + GRIP_HIT_PX) * std::f32::consts::FRAC_1_SQRT_2;
    assert_eq!(hit(&l, c + vec2(-d, d) * 0.999), Some(Grip::Rotate));
    assert_eq!(hit(&l, c + vec2(-d, d) * 1.02), None);
    // Hinter der Pfeilspitze ist nichts mehr.
    assert_eq!(
        hit(&l, c + vec2(GRIP_ARROW_PX + GRIP_HIT_PX + 0.5, 0.0)),
        None
    );
    // Abseits von allem.
    assert_eq!(hit(&l, c + vec2(20.0, 20.0)), None);
}

#[test]
fn the_order_is_center_arrows_ring() {
    let l = straight();
    let c = l.center;
    // Der Ring kreuzt die Pfeile: Dort gewinnt der Pfeil.
    assert_eq!(hit(&l, c + vec2(GRIP_RING_PX, 0.0)), Some(Grip::AxisX));
    assert_eq!(hit(&l, c + vec2(0.0, -GRIP_RING_PX)), Some(Grip::AxisY));
    // Die Mitte liegt auf beiden Pfeilen.
    assert_eq!(hit(&l, c + vec2(1.0, -1.0)), Some(Grip::Free));
}

#[test]
fn scaling_has_only_the_handle_on_the_right() {
    let l = layout(pos2(500.0, 400.0), vec2(1.0, 0.0), vec2(0.0, -1.0), true);
    assert_eq!(hit(&l, l.x_tip), Some(Grip::Scale));
    assert_eq!(hit(&l, l.x_tip + vec2(GRIP_HIT_PX, 0.0)), Some(Grip::Scale));
    assert_eq!(hit(&l, l.center), None);
    assert_eq!(hit(&l, l.y_tip), None);
    assert_eq!(hit(&l, l.center + vec2(-GRIP_RING_PX, 0.0)), None);
    assert_eq!(hit(&l, l.center + vec2(30.0, 0.0)), None);
}

#[test]
fn the_arrows_follow_a_tilted_mapping() {
    let (cos, sin) = (0.6f32, 0.8f32);
    let l = layout(pos2(300.0, 300.0), vec2(cos, sin), vec2(sin, -cos), false);
    let along = |d: Vec2, t: f32| l.center + d * t;
    assert_eq!(hit(&l, along(vec2(cos, sin), 55.0)), Some(Grip::AxisX));
    assert_eq!(hit(&l, along(vec2(sin, -cos), 55.0)), Some(Grip::AxisY));
    assert_eq!(hit(&l, l.center + vec2(55.0, 0.0)), None);
    assert!(((l.x_tip - l.center).length() - GRIP_ARROW_PX).abs() < 1e-3);
}

// ---- Griff-Rechnung --------------------------------------------------------------------------

fn drag(grip: Grip, press: DVec2) -> GripDrag {
    GripDrag {
        grip,
        press,
        start_offset: v(1.0, 2.0),
        start_angle: 0.25,
        start_factor: 1.5,
    }
}

#[test]
fn free_and_axis_grips_add_only_their_share() {
    let press = v(3.0, 4.0);
    let now = v(10.0, -1.0);
    let base = v(100.0, 100.0);
    assert_eq!(
        grip_motion(&drag(Grip::Free, press), base, now),
        (v(8.0, -3.0), 0.25, 1.5)
    );
    assert_eq!(
        grip_motion(&drag(Grip::AxisX, press), base, now),
        (v(8.0, 2.0), 0.25, 1.5)
    );
    assert_eq!(
        grip_motion(&drag(Grip::AxisY, press), base, now),
        (v(1.0, -3.0), 0.25, 1.5)
    );
}

#[test]
fn turning_runs_through_plus_minus_180_degrees_without_a_jump() {
    let base = v(5.0, -3.0);
    let at = |deg: f64| base + DVec2::from_angle(deg.to_radians()) * 7.0;
    // Einmal über die Naht von atan2.
    let d = drag(Grip::Rotate, at(170.0));
    let (_, a, _) = grip_motion(&d, base, at(-170.0));
    assert!((a - (0.25 + 20f64.to_radians())).abs() < 1e-12, "{a}");
    let (_, a, _) = grip_motion(&d, base, at(160.0));
    assert!((a - (0.25 - 10f64.to_radians())).abs() < 1e-12, "{a}");
}

/// Dichte Zeigerfolge über zwei volle Umdrehungen, in beide Richtungen und ab jeder Startlage.
#[test]
fn a_dense_circle_counts_720_degrees_and_never_jumps() {
    let base = v(-2.0, 9.0);
    for sign in [1.0, -1.0] {
        for start in 0..36 {
            let start = f64::from(start) * 10f64.to_radians();
            let at = |t: f64| base + DVec2::from_angle(start + sign * t) * 4.0;
            let d = GripDrag {
                start_angle: 0.0,
                ..drag(Grip::Rotate, at(0.0))
            };
            let mut angle = 0.0;
            let steps = 1440;
            for k in 1..=steps {
                let t = f64::from(k) / f64::from(steps) * 2.0 * TAU;
                let (_, raw, _) = grip_motion(&d, base, at(t));
                let next = unwrapped_angle(angle, raw);
                assert!(
                    (next - angle).abs() < 1e-2,
                    "Sprung bei {k}: {angle} → {next}"
                );
                angle = next;
            }
            assert!((angle - sign * 2.0 * TAU).abs() < 1e-9, "{angle}");
        }
    }
}

#[test]
fn the_factor_follows_the_distance_and_never_reaches_zero() {
    let base = v(1.0, 1.0);
    let d = GripDrag {
        start_factor: 1.0,
        ..drag(Grip::Scale, v(4.0, 1.0))
    };
    let (_, _, k) = grip_motion(&d, base, v(7.0, 1.0));
    assert!((k - 2.0).abs() < 1e-12);
    let (_, _, k) = grip_motion(&d, base, v(1.0, 2.5));
    assert!((k - 0.5).abs() < 1e-12);
    let (_, _, k) = grip_motion(&d, base, base);
    assert!(k > 0.0 && k.is_finite());
    // Dichte Folge durch den Bezugspunkt hindurch: stetig und nie null oder negativ.
    let mut last = 1.0f64;
    for i in 0..=4000 {
        let x = 4.0 - f64::from(i) * 0.002;
        let (_, _, k) = grip_motion(&d, base, v(x, 1.0));
        assert!(k > 0.0 && k.is_finite(), "{x}: {k}");
        assert!((k - last).abs() < 1e-2, "{x}: {last} → {k}");
        last = k;
    }
}

#[test]
fn a_press_on_the_base_never_gives_nan() {
    let base = v(2.0, 2.0);
    for grip in [
        Grip::Rotate,
        Grip::Scale,
        Grip::Free,
        Grip::AxisX,
        Grip::AxisY,
    ] {
        let d = drag(grip, base);
        for now in [base, v(3.0, 2.0), v(f64::NAN, 1.0), v(f64::INFINITY, 0.0)] {
            let (o, a, k) = grip_motion(&d, base, now);
            assert!(
                o.is_finite() && a.is_finite() && k.is_finite(),
                "{grip:?} {now}"
            );
            assert!(k > 0.0);
        }
    }
    // Der Griff bleibt beim Startwert.
    let d = drag(Grip::Rotate, base);
    assert_eq!(grip_motion(&d, base, v(3.0, 2.0)).1, 0.25);
    let d = drag(Grip::Scale, base);
    assert_eq!(grip_motion(&d, base, v(3.0, 2.0)).2, 1.5);
}

#[test]
fn dense_pointer_sequences_on_the_grips_are_continuous() {
    for grip in [Grip::Free, Grip::AxisX, Grip::AxisY] {
        let d = drag(grip, v(0.0, 0.0));
        let mut last = grip_motion(&d, v(9.0, 9.0), v(0.0, 0.0)).0;
        for i in 0..=2000 {
            let t = f64::from(i) * 0.01;
            let now = v(t.cos() * t, t.sin() * t);
            let o = grip_motion(&d, v(9.0, 9.0), now).0;
            assert!(o.distance(last) < 0.5, "{grip:?} {t}");
            last = o;
        }
    }
}

// ---- Rohereignisse ----------------------------------------------------------------------------

fn sketch_with(build: impl FnOnce(&mut Sketch)) -> AppState {
    let mut s = AppState::new();
    let i = s
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    s.open_sketch(i).unwrap();
    build(s.session_sketch_mut().unwrap().1);
    s.observe_sketch(false);
    s
}

fn square() -> (AppState, Vec<CurveId>) {
    let mut ids = Vec::new();
    let s = sketch_with(|sk| {
        let r = sk.add_rectangle(v(-5.0, -5.0), v(5.0, 5.0)).unwrap();
        ids = r.lines.to_vec();
    });
    (s, ids)
}

/// Quadrat, alles vorgewählt, Verschieben/Kopieren aktiv.
fn active_square() -> (AppState, Vec<CurveId>) {
    let (mut s, ids) = square();
    let (se, sk) = s.session_sketch_mut().unwrap();
    se.selection = ids.iter().map(|&c| Item::Curve(c)).collect();
    se.set_tool(sk, Tool::MoveCopy);
    (s, ids)
}

fn context() -> egui::Context {
    let ctx = egui::Context::default();
    ctx.set_fonts(fonts::definitions());
    ctx
}

fn screen(s: &AppState, p: DVec2) -> Pos2 {
    let frame = *s.session_sketch().unwrap().frame();
    let mut cam = s.camera;
    cam.set_viewport(W, H);
    let q = sketch_view::sketch_to_screen(&cam, &frame, glam::Vec2::new(W, H), p).unwrap();
    pos2(q.x, q.y)
}

fn to_sketch(s: &AppState, p: Pos2) -> DVec2 {
    let frame = *s.session_sketch().unwrap().frame();
    let mut cam = s.camera;
    cam.set_viewport(W, H);
    let q = glam::Vec2::new(p.x, p.y);
    sketch_view::screen_to_sketch(&cam, &frame, glam::Vec2::new(W, H), q).unwrap()
}

fn mods(shift: bool) -> Modifiers {
    Modifiers {
        shift,
        ..Default::default()
    }
}

fn pointer(p: Pos2, b: PointerButton, pressed: bool, shift: bool) -> Event {
    Event::PointerButton {
        pos: p,
        button: b,
        pressed,
        modifiers: mods(shift),
    }
}

fn key(k: Key) -> Event {
    Event::Key {
        key: k,
        physical_key: Some(k),
        pressed: true,
        repeat: false,
        modifiers: Modifiers::NONE,
    }
}

fn frame_with(ctx: &egui::Context, s: &mut AppState, mut events: Vec<Event>, shift: bool) {
    events.insert(0, Event::ModifiersChanged(mods(shift)));
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

fn frame(ctx: &egui::Context, s: &mut AppState, events: Vec<Event>) {
    frame_with(ctx, s, events, false);
}

fn left(p: Pos2, pressed: bool) -> Event {
    pointer(p, PointerButton::Primary, pressed, false)
}

/// Zeiger hin, drücken, in `steps` Schritten ziehen; die Taste bleibt unten.
fn press_and_move(ctx: &egui::Context, s: &mut AppState, from: Pos2, to: Pos2, steps: usize) {
    frame(ctx, s, vec![]);
    frame(ctx, s, vec![Event::PointerMoved(from)]);
    frame(ctx, s, vec![left(from, true)]);
    for i in 1..=steps {
        let t = i as f32 / steps as f32;
        frame(ctx, s, vec![Event::PointerMoved(from + (to - from) * t)]);
    }
}

fn release(ctx: &egui::Context, s: &mut AppState, at: Pos2) {
    frame(ctx, s, vec![left(at, false)]);
    frame(ctx, s, vec![]);
}

fn grips(s: &AppState) -> Option<GripLayout> {
    sketch_view::grip_layout(s, SCREEN)
}

fn bits(s: &AppState) -> Vec<u64> {
    let sk = s.session_sketch().unwrap();
    sk.variables().iter().map(|x| x.to_bits()).collect()
}

fn revision(s: &AppState) -> u64 {
    s.session_sketch().unwrap().revision()
}

fn undo_len(s: &AppState) -> usize {
    s.sketch_history().unwrap().undo_len()
}

fn sketch_delta(s: &AppState, at: Pos2, to: Pos2) -> DVec2 {
    to_sketch(s, to) - to_sketch(s, at)
}

#[test]
fn dragging_the_center_moves_only_the_preview() {
    let (mut s, _) = active_square();
    let ctx = context();
    frame(&ctx, &mut s, vec![]);
    let (rev, undo, before, sel) = (
        revision(&s),
        undo_len(&s),
        bits(&s),
        s.sketch_session.as_ref().unwrap().selection.clone(),
    );
    let l = grips(&s).expect("Griffe");
    let to = l.center + vec2(120.0, -40.0);
    press_and_move(&ctx, &mut s, l.center, to, 12);
    {
        let se = s.sketch_session.as_ref().unwrap();
        assert_eq!(se.grip_drag.map(|d| d.grip), Some(Grip::Free));
        assert!(se.curve_drag.is_none() && se.dragging.is_none());
        assert!(se.box_select.is_none());
    }
    assert_eq!(
        revision(&s),
        rev,
        "die Skizze bleibt beim Griffzug unberührt"
    );
    let want = sketch_delta(&s, l.center, to);
    release(&ctx, &mut s, to);
    let se = s.sketch_session.as_ref().unwrap();
    assert!(se.grip_drag.is_none());
    assert!(
        se.transform.offset.distance(want) < 1e-3,
        "{}",
        se.transform.offset
    );
    assert_eq!(se.selection, sel, "keine Rahmenwahl");
    assert_eq!(se.transform.stage, TransformStage::Active);
    assert!(se.transform_preview(s.session_sketch().unwrap()).is_some());
    assert_eq!(bits(&s), before);
    assert_eq!(revision(&s), rev);
    assert_eq!(undo_len(&s), undo);
}

#[test]
fn the_arrows_and_the_ring_drive_their_own_share() {
    let (mut s, _) = active_square();
    let ctx = context();
    frame(&ctx, &mut s, vec![]);
    let l = grips(&s).unwrap();
    let grip_at = l.center + vec2(50.0, 0.0);
    let to = grip_at + vec2(100.0, 70.0);
    press_and_move(&ctx, &mut s, grip_at, to, 10);
    let want = sketch_delta(&s, grip_at, to);
    release(&ctx, &mut s, to);
    let t = s.sketch_session.as_ref().unwrap().transform.clone();
    assert!((t.offset.x - want.x).abs() < 1e-3);
    assert_eq!(t.offset.y, 0.0, "nur die x-Achse");

    // Die Griffe stehen jetzt an der neuen Lage und folgen dem Bezugspunkt samt Verschiebung.
    let l = grips(&s).unwrap();
    assert!((l.center - screen(&s, t.base.unwrap() + t.offset)).length() < 1e-3);

    // Zwei volle Umdrehungen am Ring, dichte Folge.
    let c = l.center;
    let on_ring = |deg: f32| {
        let a = deg.to_radians();
        c + vec2(a.cos(), a.sin()) * GRIP_RING_PX
    };
    let start = on_ring(135.0);
    frame(&ctx, &mut s, vec![Event::PointerMoved(start)]);
    frame(&ctx, &mut s, vec![left(start, true)]);
    for k in 1..=720 {
        frame(
            &ctx,
            &mut s,
            vec![Event::PointerMoved(on_ring(135.0 + k as f32))],
        );
        // Bis egui den Zug erkennt, vergehen einige Bildpunkte; nichts davon geht verloren.
        if k > 20 {
            let se = s.sketch_session.as_ref().unwrap();
            assert_eq!(se.grip_drag.map(|d| d.grip), Some(Grip::Rotate));
        }
    }
    release(&ctx, &mut s, on_ring(135.0 + 720.0));
    let a = s.sketch_session.as_ref().unwrap().transform.angle;
    assert!((a.abs() - 2.0 * TAU).abs() < 1e-3, "720° bleibt 720°: {a}");
}

#[test]
fn dragging_beside_the_grips_does_nothing_while_active() {
    let (mut s, _) = active_square();
    let ctx = context();
    frame(&ctx, &mut s, vec![]);
    let (rev, before) = (revision(&s), bits(&s));
    let l = grips(&s).unwrap();
    let from = l.center + vec2(300.0, 200.0);
    let to = from + vec2(200.0, 100.0);
    press_and_move(&ctx, &mut s, from, to, 6);
    {
        let se = s.sketch_session.as_ref().unwrap();
        assert!(se.box_select.is_none(), "kein Rahmen");
        assert!(se.grip_drag.is_none() && se.curve_drag.is_none());
    }
    release(&ctx, &mut s, to);
    let se = s.sketch_session.as_ref().unwrap();
    assert_eq!(se.transform.offset, DVec2::ZERO);
    assert_eq!(se.selection.len(), 4);
    assert_eq!(bits(&s), before);
    assert_eq!(revision(&s), rev);
}

#[test]
fn while_picking_a_box_selects_and_the_grips_come_only_afterwards() {
    let (mut s, _) = square();
    let ctx = context();
    {
        let (se, sk) = s.session_sketch_mut().unwrap();
        se.selection.clear();
        se.set_tool(sk, Tool::MoveCopy);
        assert_eq!(se.transform.stage, TransformStage::Picking);
    }
    frame(&ctx, &mut s, vec![]);
    assert!(grips(&s).is_none(), "ohne Menge keine Griffe");
    let from = screen(&s, v(-20.0, 20.0));
    let to = screen(&s, v(20.0, -20.0));
    press_and_move(&ctx, &mut s, from, to, 6);
    assert!(s.sketch_session.as_ref().unwrap().box_select.is_some());
    assert!(grips(&s).is_none());
    release(&ctx, &mut s, to);
    {
        let se = s.sketch_session.as_ref().unwrap();
        assert_eq!(se.selection.len(), 4, "der Rahmen wählt");
        assert_eq!(se.transform.stage, TransformStage::Picking);
    }
    assert!(grips(&s).is_none());
    frame(&ctx, &mut s, vec![key(Key::Enter)]);
    frame(&ctx, &mut s, vec![]);
    let se = s.sketch_session.as_ref().unwrap();
    assert_eq!(se.transform.stage, TransformStage::Active);
    assert_eq!(se.transform.set.curves.len(), 4);
    let base = se.transform.base.unwrap();
    assert!(base.distance(v(0.0, 0.0)) < 1e-9, "Mitte der Hülle: {base}");
    assert!(grips(&s).is_some());
}

#[test]
fn clicks_toggle_while_picking_and_enter_without_a_choice_says_so() {
    let (mut s, ids) = square();
    let ctx = context();
    {
        let (se, sk) = s.session_sketch_mut().unwrap();
        se.selection.clear();
        se.set_tool(sk, Tool::MoveCopy);
    }
    frame(&ctx, &mut s, vec![]);
    frame(&ctx, &mut s, vec![key(Key::Enter)]);
    frame(&ctx, &mut s, vec![]);
    {
        let se = s.sketch_session.as_ref().unwrap();
        assert_eq!(se.transform.stage, TransformStage::Picking);
        assert!(se.message.is_some());
    }
    // Mitte der ersten Seite (y = -5), einfacher Klick ohne Umschalt.
    let side = screen(&s, v(0.0, -5.0));
    let (se, sk) = s.session_sketch_mut().unwrap();
    let radius = 0.5;
    se.click(sk, v(0.0, -5.0), radius, false).unwrap();
    let picked = se.selection.clone();
    assert_eq!(picked.len(), 1);
    assert!(matches!(picked[0], Item::Curve(c) if ids.contains(&c)));
    // Ein Klick neben alles lässt die Wahl; ein zweiter Klick nimmt die Kurve wieder heraus.
    se.click(sk, v(30.0, 30.0), radius, false).unwrap();
    assert_eq!(se.selection, picked);
    se.click(sk, v(0.0, -5.0), radius, false).unwrap();
    assert!(se.selection.is_empty());
    // Und über Rohereignisse.
    frame(&ctx, &mut s, vec![Event::PointerMoved(side)]);
    frame(&ctx, &mut s, vec![left(side, true)]);
    frame(&ctx, &mut s, vec![left(side, false)]);
    frame(&ctx, &mut s, vec![]);
    assert_eq!(s.sketch_session.as_ref().unwrap().selection, picked);
}

#[test]
fn a_double_click_while_picking_takes_the_chain_and_in_active_commits_nothing() {
    let (mut s, _) = square();
    let ctx = context();
    {
        let (se, sk) = s.session_sketch_mut().unwrap();
        se.selection.clear();
        se.set_tool(sk, Tool::MoveCopy);
    }
    let side = screen(&s, v(0.0, -5.0));
    let double = |ctx: &egui::Context, s: &mut AppState, p: Pos2| {
        frame(ctx, s, vec![Event::PointerMoved(p)]);
        frame(ctx, s, vec![left(p, true)]);
        frame(ctx, s, vec![left(p, false)]);
        frame(ctx, s, vec![left(p, true)]);
        frame(ctx, s, vec![left(p, false)]);
        frame(ctx, s, vec![]);
    };
    frame(&ctx, &mut s, vec![]);
    double(&ctx, &mut s, side);
    assert_eq!(
        s.sketch_session.as_ref().unwrap().selection.len(),
        4,
        "die Kette wie im Auswahlwerkzeug"
    );
    frame(&ctx, &mut s, vec![key(Key::Enter)]);
    frame(&ctx, &mut s, vec![]);
    let (rev, undo) = (revision(&s), undo_len(&s));
    {
        let se = s.sketch_session.as_mut().unwrap();
        se.transform.offset = v(3.0, 0.0);
    }
    let empty = screen(&s, v(40.0, 40.0));
    double(&ctx, &mut s, empty);
    let se = s.sketch_session.as_ref().unwrap();
    assert_eq!(se.tool, Tool::MoveCopy, "ein Doppelklick schließt nicht ab");
    assert_eq!(se.transform.stage, TransformStage::Active);
    assert_eq!(revision(&s), rev);
    assert_eq!(undo_len(&s), undo);
}

#[test]
fn shift_during_a_grip_does_not_pan_and_the_middle_button_always_does() {
    let (mut s, _) = active_square();
    let ctx = context();
    frame(&ctx, &mut s, vec![]);
    let l = grips(&s).unwrap();
    let target = s.camera.target;
    let to = l.center + vec2(80.0, 0.0);
    press_and_move(&ctx, &mut s, l.center, to, 4);
    // Umschalt kommt erst während des Griffs dazu.
    for i in 1..=4 {
        let p = to + vec2(10.0 * i as f32, 0.0);
        frame_with(&ctx, &mut s, vec![Event::PointerMoved(p)], true);
    }
    assert_eq!(
        s.camera.target, target,
        "Umschalt+links schwenkt im Griff nicht"
    );
    assert!(s.sketch_session.as_ref().unwrap().grip_drag.is_some());
    frame(&ctx, &mut s, vec![left(to, false)]);
    frame(&ctx, &mut s, vec![]);

    // Umschalt+links auf einem Griff: der Griff, nicht das Schwenken.
    let l = grips(&s).unwrap();
    frame_with(&ctx, &mut s, vec![Event::PointerMoved(l.center)], true);
    frame_with(
        &ctx,
        &mut s,
        vec![pointer(l.center, PointerButton::Primary, true, true)],
        true,
    );
    for i in 1..=5 {
        let p = l.center + vec2(0.0, 10.0 * i as f32);
        frame_with(&ctx, &mut s, vec![Event::PointerMoved(p)], true);
    }
    assert_eq!(s.camera.target, target);
    assert_eq!(
        s.sketch_session.as_ref().unwrap().grip_drag.map(|d| d.grip),
        Some(Grip::Free)
    );
    let up = l.center + vec2(0.0, 50.0);
    frame_with(
        &ctx,
        &mut s,
        vec![pointer(up, PointerButton::Primary, false, true)],
        true,
    );
    frame(&ctx, &mut s, vec![]);

    // Mitte neben den Griffen schwenkt.
    let l = grips(&s).unwrap();
    let from = l.center + vec2(300.0, 200.0);
    frame(&ctx, &mut s, vec![Event::PointerMoved(from)]);
    frame(
        &ctx,
        &mut s,
        vec![pointer(from, PointerButton::Middle, true, false)],
    );
    for i in 1..=6 {
        let p = from + vec2(15.0 * i as f32, 0.0);
        frame(&ctx, &mut s, vec![Event::PointerMoved(p)]);
    }
    assert_ne!(s.camera.target, target, "die Mitte schwenkt");
    let moved = s.camera.target;
    let up = from + vec2(90.0, 0.0);
    frame(
        &ctx,
        &mut s,
        vec![pointer(up, PointerButton::Middle, false, false)],
    );
    frame(&ctx, &mut s, vec![]);
    assert!(s.sketch_session.as_ref().unwrap().grip_drag.is_none());

    // Mitte auf einem Griff schwenkt auch und greift nicht.
    let l = grips(&s).unwrap();
    frame(&ctx, &mut s, vec![Event::PointerMoved(l.center)]);
    frame(
        &ctx,
        &mut s,
        vec![pointer(l.center, PointerButton::Middle, true, false)],
    );
    for i in 1..=6 {
        let p = l.center + vec2(15.0 * i as f32, 0.0);
        frame(&ctx, &mut s, vec![Event::PointerMoved(p)]);
    }
    assert_ne!(s.camera.target, moved);
    assert!(s.sketch_session.as_ref().unwrap().grip_drag.is_none());
    let up = l.center + vec2(90.0, 0.0);
    frame(
        &ctx,
        &mut s,
        vec![pointer(up, PointerButton::Middle, false, false)],
    );

    // Mitte dazu, während der Griff läuft: schwenkt ebenso, der Griff bleibt.
    frame(&ctx, &mut s, vec![]);
    let l = grips(&s).unwrap();
    let to = l.center + vec2(60.0, 0.0);
    press_and_move(&ctx, &mut s, l.center, to, 4);
    let during = s.camera.target;
    frame(
        &ctx,
        &mut s,
        vec![pointer(to, PointerButton::Middle, true, false)],
    );
    for i in 1..=6 {
        let p = to + vec2(0.0, 10.0 * i as f32);
        frame(&ctx, &mut s, vec![Event::PointerMoved(p)]);
    }
    assert_ne!(s.camera.target, during, "die Mitte schwenkt immer");
    assert!(s.sketch_session.as_ref().unwrap().grip_drag.is_some());
}

#[test]
fn the_right_button_and_esc_take_back_only_the_grip() {
    for use_esc in [false, true] {
        let (mut s, _) = active_square();
        let ctx = context();
        frame(&ctx, &mut s, vec![]);
        let rev = revision(&s);
        // Erster Griff: bleibt stehen.
        let l = grips(&s).unwrap();
        let first = l.center + vec2(100.0, 0.0);
        press_and_move(&ctx, &mut s, l.center, first, 5);
        release(&ctx, &mut s, first);
        let kept = s.sketch_session.as_ref().unwrap().transform.clone();
        assert!(kept.offset.length() > 0.0);

        // Zweiter Griff: wird zurückgenommen.
        let l = grips(&s).unwrap();
        let to = l.center + vec2(0.0, 100.0);
        press_and_move(&ctx, &mut s, l.center, to, 5);
        assert!(s.sketch_session.as_ref().unwrap().grip_drag.is_some());
        let event = if use_esc {
            key(Key::Escape)
        } else {
            pointer(to, PointerButton::Secondary, true, false)
        };
        frame(&ctx, &mut s, vec![event]);
        {
            let se = s.sketch_session.as_ref().unwrap();
            assert!(se.grip_drag.is_none());
            assert_eq!(se.tool, Tool::MoveCopy, "nur der Zug");
            assert_eq!(se.transform, kept, "Stand vor dem Griff");
        }
        // Weiterbewegen und Loslassen tun nichts, auch die rechte Taste ist kein Klick.
        let further = to + vec2(30.0, 30.0);
        frame(&ctx, &mut s, vec![Event::PointerMoved(further)]);
        let mut up = vec![left(further, false)];
        if !use_esc {
            up.push(pointer(further, PointerButton::Secondary, false, false));
        }
        frame(&ctx, &mut s, up);
        frame(&ctx, &mut s, vec![]);
        {
            let se = s.sketch_session.as_ref().unwrap();
            assert_eq!(se.tool, Tool::MoveCopy, "use_esc = {use_esc}");
            assert_eq!(se.transform, kept);
            assert!(se.grip_drag.is_none());
        }
        // Nächstes Esc beendet.
        frame(&ctx, &mut s, vec![key(Key::Escape)]);
        frame(&ctx, &mut s, vec![]);
        let se = s.sketch_session.as_ref().unwrap();
        assert_eq!(se.tool, Tool::Select);
        assert_eq!(revision(&s), rev);
    }
}

#[test]
fn the_fields_at_the_pointer_are_no_obstacle_for_the_grips() {
    let (mut s, _) = active_square();
    let ctx = context();
    frame(&ctx, &mut s, vec![]);
    let l = grips(&s).unwrap();
    // Die Felder stehen rechts unterhalb des Zeigers (24 Bildpunkte): Der Ring liegt so darunter.
    let rest = l.center + vec2(-5.0, -5.0);
    frame(&ctx, &mut s, vec![Event::PointerMoved(rest)]);
    frame(&ctx, &mut s, vec![]);
    assert_eq!(s.sketch_session.as_ref().unwrap().input.fields.len(), 3);
    let a = 45f32.to_radians();
    let on_ring = l.center + vec2(a.cos(), a.sin()) * GRIP_RING_PX;
    let to = on_ring + vec2(40.0, -30.0);
    frame(&ctx, &mut s, vec![left(on_ring, true)]);
    for i in 1..=5 {
        let p = on_ring + (to - on_ring) * (i as f32 / 5.0);
        frame(&ctx, &mut s, vec![Event::PointerMoved(p)]);
    }
    assert_eq!(
        s.sketch_session.as_ref().unwrap().grip_drag.map(|d| d.grip),
        Some(Grip::Rotate)
    );
    release(&ctx, &mut s, to);
    assert!(s.sketch_session.as_ref().unwrap().transform.angle != 0.0);
}

#[test]
fn the_select_tool_still_drags_curves_at_the_same_spot() {
    let (mut s, ids) = square();
    let ctx = context();
    {
        let (se, _) = s.session_sketch_mut().unwrap();
        se.selection = vec![Item::Curve(ids[0])];
    }
    frame(&ctx, &mut s, vec![]);
    assert!(grips(&s).is_none(), "keine Griffe im Auswahlwerkzeug");
    let on = screen(&s, v(0.0, -5.0));
    press_and_move(&ctx, &mut s, on, on + vec2(0.0, 60.0), 6);
    let se = s.sketch_session.as_ref().unwrap();
    assert!(se.curve_drag.is_some() && se.grip_drag.is_none());
}

// ---- Die ganze App: Liste und Strg+Z ----------------------------------------------------------

struct Rig {
    ctx: egui::Context,
    app: PlanoraApp,
    time: f64,
}

impl Rig {
    /// Quadrat vorgewählt, `M` gedrückt.
    fn new() -> Rig {
        let ctx = egui::Context::default();
        ctx.set_fonts(fonts::definitions());
        theme::apply(&ctx, true);
        PlanoraApp::setup_context(&ctx);
        let root = common::temp_root("planora-sketch-grips");
        let mut w = Workspace::new(Some(Library::open(&root).unwrap()), Config::default());
        w.config_path = root.join("config.toml");
        let s = &mut w.active_tab_mut().state;
        let sk = s
            .add_sketch(SketchPlane::Standard(StandardPlane::XY))
            .unwrap();
        s.open_sketch(sk).unwrap();
        let (se, sk) = s.session_sketch_mut().unwrap();
        let ids = sk.add_rectangle(v(-5.0, -5.0), v(5.0, 5.0)).unwrap().lines;
        se.selection = ids.iter().map(|&c| Item::Curve(c)).collect();
        w.observe_history(false);
        let mut rig = Rig {
            ctx,
            app: PlanoraApp::for_tests(w),
            time: 0.0,
        };
        rig.settle();
        rig.frame(vec![key(Key::M)]);
        rig.settle();
        assert_eq!(
            rig.st().sketch_session.as_ref().unwrap().tool,
            Tool::MoveCopy
        );
        rig
    }

    fn st(&mut self) -> &mut AppState {
        &mut self.app.ws_mut().active_tab_mut().state
    }

    fn frame(&mut self, events: Vec<Event>) {
        self.time += 1.0 / 60.0;
        let raw = egui::RawInput {
            screen_rect: Some(SCREEN),
            time: Some(self.time),
            events,
            ..Default::default()
        };
        let mut f = eframe::Frame::_new_kittest();
        let app = &mut self.app;
        self.ctx
            .run_ui(raw, |ui| app.ui(ui, &mut f))
            .drop_without_applying_deltas();
    }

    fn settle(&mut self) {
        for _ in 0..3 {
            self.frame(vec![]);
        }
    }

    fn raw_at(&mut self, p: Pos2) -> DVec2 {
        self.frame(vec![Event::PointerMoved(p)]);
        self.frame(vec![]);
        let se = self.st().sketch_session.as_ref().unwrap();
        se.cursor.expect("Zeiger")
    }

    /// Bildpunkt zur Skizzenlage `target`: Die Ansicht von oben bildet affin ab, drei Messungen
    /// reichen.
    fn pixel_of(&mut self, target: DVec2) -> Pos2 {
        let p0 = pos2(900.0, 500.0);
        let r0 = self.raw_at(p0);
        let rx = self.raw_at(p0 + vec2(100.0, 0.0)) - r0;
        let ry = self.raw_at(p0 + vec2(0.0, 100.0)) - r0;
        let d = target - r0;
        let det = rx.x * ry.y - rx.y * ry.x;
        let a = (d.x * ry.y - d.y * ry.x) / det;
        let b = (rx.x * d.y - rx.y * d.x) / det;
        p0 + vec2(a as f32 * 100.0, b as f32 * 100.0)
    }

    fn drag_from(&mut self, from: Pos2, to: Pos2) {
        self.frame(vec![Event::PointerMoved(from)]);
        self.frame(vec![pointer(from, PointerButton::Primary, true, false)]);
        for i in 1..=5 {
            let p = from + (to - from) * (i as f32 / 5.0);
            self.frame(vec![Event::PointerMoved(p)]);
        }
    }

    fn release(&mut self, at: Pos2) {
        self.frame(vec![pointer(at, PointerButton::Primary, false, false)]);
        self.settle();
    }
}

#[test]
fn a_press_on_a_grip_with_an_open_list_closes_only_the_list() {
    let mut rig = Rig::new();
    assert_eq!(
        rig.st().sketch_session.as_ref().unwrap().transform.stage,
        TransformStage::Active
    );
    let center = rig.pixel_of(DVec2::ZERO);

    // Rechtsklick auf „Rechteck“ öffnet die Liste, das Werkzeug bleibt.
    let label = sketch_panel::tool_label(Tool::Rectangle);
    let button = sketch_panel::strip_buttons(&rig.ctx)
        .into_iter()
        .find(|(l, _)| l == &label)
        .unwrap()
        .1
        .center();
    rig.frame(vec![Event::PointerMoved(button)]);
    rig.frame(vec![pointer(button, PointerButton::Secondary, true, false)]);
    rig.frame(vec![pointer(
        button,
        PointerButton::Secondary,
        false,
        false,
    )]);
    rig.settle();
    assert!(sketch_panel::flyout_open(&rig.ctx), "Liste offen");
    assert_eq!(
        rig.st().sketch_session.as_ref().unwrap().tool,
        Tool::MoveCopy
    );

    let to = center + vec2(100.0, 0.0);
    rig.drag_from(center, to);
    assert!(
        rig.st()
            .sketch_session
            .as_ref()
            .unwrap()
            .grip_drag
            .is_none(),
        "dieses Drücken schließt nur die Liste"
    );
    rig.release(to);
    assert!(!sketch_panel::flyout_open(&rig.ctx));
    {
        let se = rig.st().sketch_session.as_ref().unwrap();
        assert_eq!(se.transform.offset, DVec2::ZERO);
        assert_eq!(se.tool, Tool::MoveCopy);
    }

    // Gegenprobe: Dasselbe Drücken ohne Liste greift.
    rig.drag_from(center, to);
    assert_eq!(
        rig.st()
            .sketch_session
            .as_ref()
            .unwrap()
            .grip_drag
            .map(|d| d.grip),
        Some(Grip::Free)
    );
}

#[test]
fn undo_during_a_grip_drops_only_the_grip() {
    let mut rig = Rig::new();
    let center = rig.pixel_of(DVec2::ZERO);
    let rev = rig.st().session_sketch().unwrap().revision();
    let undo = rig.st().sketch_history().unwrap().undo_len();
    let to = center + vec2(100.0, 0.0);
    rig.drag_from(center, to);
    assert!(
        rig.st()
            .sketch_session
            .as_ref()
            .unwrap()
            .grip_drag
            .is_some()
    );
    let ctrl_z = Event::Key {
        key: Key::Z,
        physical_key: Some(Key::Z),
        pressed: true,
        repeat: false,
        modifiers: Modifiers::COMMAND,
    };
    rig.frame(vec![ctrl_z]);
    rig.frame(vec![]);
    {
        let st = rig.st();
        let se = st.sketch_session.as_ref().unwrap();
        assert!(se.grip_drag.is_none(), "der Zug fällt weg");
        assert_eq!(se.transform.offset, DVec2::ZERO);
        assert_eq!(se.tool, Tool::MoveCopy);
    }
    rig.release(to);
    let st = rig.st();
    assert_eq!(st.session_sketch().unwrap().revision(), rev);
    assert_eq!(st.sketch_history().unwrap().undo_len(), undo);
    let se = st.sketch_session.as_ref().unwrap();
    assert_eq!(se.transform.offset, DVec2::ZERO);
}

/// Texte eines Bilds der ganzen App mit ihrer Lage.
fn texts(shape: &egui::Shape, out: &mut Vec<(String, Rect)>) {
    match shape {
        egui::Shape::Vec(v) => v.iter().for_each(|s| texts(s, out)),
        egui::Shape::Text(t) => out.push((
            t.galley.text().to_string(),
            t.galley.rect.translate(t.pos.to_vec2()),
        )),
        _ => {}
    }
}

impl Rig {
    /// Lage des Texts `label` im Skizzenfeld.
    fn panel_text(&mut self, label: &str) -> Pos2 {
        self.time += 1.0 / 60.0;
        let raw = egui::RawInput {
            screen_rect: Some(SCREEN),
            time: Some(self.time),
            ..Default::default()
        };
        let mut f = eframe::Frame::_new_kittest();
        let app = &mut self.app;
        let out = self.ctx.run_ui(raw, |ui| app.ui(ui, &mut f));
        let mut seen = Vec::new();
        for c in &out.shapes {
            texts(&c.shape, &mut seen);
        }
        out.drop_without_applying_deltas();
        let right = planora_app::shell_panels::last_layout(&self.ctx)
            .unwrap()
            .right;
        seen.into_iter()
            .find(|(t, r)| t == label && right.contains(r.center()))
            .unwrap_or_else(|| panic!("„{label}“ fehlt im Skizzenfeld"))
            .1
            .center()
    }

    fn click_at(&mut self, p: Pos2) {
        // Kein Doppelklick mit dem vorigen Klick.
        self.time += 1.0;
        self.frame(vec![Event::PointerMoved(p)]);
        self.frame(vec![pointer(p, PointerButton::Primary, true, false)]);
        self.frame(vec![pointer(p, PointerButton::Primary, false, false)]);
        self.settle();
    }

    fn square_at_origin(&mut self) -> DVec2 {
        let sk = self.st().session_sketch().unwrap();
        let (first, _) = sk.curves().next().unwrap();
        let line = sk.curve_polyline(first, 96).unwrap();
        line[0]
    }
}

/// Fahrt: `M` mit Vorauswahl, Werte tippen, Tab, Eingabe im Feld schließt ab; ein Schritt.
#[test]
fn the_whole_trip_with_m_typed_values_and_enter() {
    let mut rig = Rig::new();
    let (undo, corner) = (
        rig.st().sketch_history().unwrap().undo_len(),
        rig.square_at_origin(),
    );
    rig.frame(vec![Event::Text("10".into())]);
    rig.frame(vec![key(Key::Tab)]);
    rig.frame(vec![Event::Text("5".into())]);
    rig.settle();
    assert_eq!(
        rig.st()
            .sketch_session
            .as_ref()
            .unwrap()
            .transform_now()
            .unwrap()
            .offset,
        v(10.0, 5.0),
        "das Feld hat die Tastatur"
    );
    rig.frame(vec![key(Key::Enter)]);
    rig.settle();
    assert_eq!(rig.square_at_origin(), corner + v(10.0, 5.0));
    let st = rig.st();
    let se = st.sketch_session.as_ref().unwrap();
    assert_eq!(se.tool, Tool::Select);
    assert_eq!(se.selection.len(), 4);
    assert_eq!(st.sketch_history().unwrap().undo_len(), undo + 1);
    assert_eq!(
        st.sketch_undo_label().unwrap(),
        planora_i18n::t("undo-move-copy")
    );
}

/// Haken, Bezugspunkt und OK im Skizzenfeld; Abbrechen beendet ohne Spur.
#[test]
fn the_panel_has_copy_base_point_ok_and_cancel() {
    let mut rig = Rig::new();
    let copy = rig.panel_text(&planora_i18n::t("sketch-move-copy"));
    rig.click_at(copy);
    assert!(rig.st().sketch_session.as_ref().unwrap().transform.copy);

    let base = rig.panel_text(&planora_i18n::t("sketch-move-set-base"));
    rig.click_at(base);
    assert_eq!(
        rig.st().sketch_session.as_ref().unwrap().transform.stage,
        TransformStage::SetBase
    );
    let spot = rig.pixel_of(v(30.0, 30.0));
    rig.click_at(spot);
    {
        let t = &rig.st().sketch_session.as_ref().unwrap().transform;
        assert_eq!(t.stage, TransformStage::Active);
        assert!(
            t.base.unwrap().distance(v(30.0, 30.0)) < 1.0,
            "{:?}",
            t.base
        );
    }
    rig.frame(vec![Event::Text("40".into())]);
    rig.settle();
    let ok = rig.panel_text(&planora_i18n::t("common-ok"));
    rig.click_at(ok);
    {
        let st = rig.st();
        assert_eq!(
            st.session_sketch().unwrap().curves().count(),
            8,
            "die Kopie"
        );
        assert_eq!(st.sketch_session.as_ref().unwrap().tool, Tool::Select);
    }

    // Abbrechen: wieder `M`, die Kopien sind vorgewählt.
    rig.frame(vec![key(Key::M)]);
    rig.settle();
    let (rev, undo) = {
        let st = rig.st();
        assert_eq!(
            st.sketch_session.as_ref().unwrap().transform.stage,
            TransformStage::Active
        );
        (
            st.session_sketch().unwrap().revision(),
            st.sketch_history().unwrap().undo_len(),
        )
    };
    let cancel = rig.panel_text(&planora_i18n::t("common-cancel"));
    rig.click_at(cancel);
    let st = rig.st();
    assert_eq!(st.sketch_session.as_ref().unwrap().tool, Tool::Select);
    assert_eq!(st.session_sketch().unwrap().revision(), rev);
    assert_eq!(st.sketch_history().unwrap().undo_len(), undo);
}

/// Das Feld hat die Tastatur: Esc im Griffzug nimmt auch dann nur den Zug zurück.
#[test]
fn esc_with_the_field_focused_takes_back_only_the_grip() {
    let mut rig = Rig::new();
    let center = rig.pixel_of(DVec2::ZERO);
    let to = center + vec2(100.0, 0.0);
    rig.drag_from(center, to);
    assert!(
        rig.st()
            .sketch_session
            .as_ref()
            .unwrap()
            .grip_drag
            .is_some()
    );
    rig.frame(vec![key(Key::Escape)]);
    {
        let se = rig.st().sketch_session.as_ref().unwrap();
        assert!(se.grip_drag.is_none());
        assert_eq!(se.tool, Tool::MoveCopy);
        assert_eq!(se.transform.offset, DVec2::ZERO);
    }
    rig.release(to);
    assert_eq!(
        rig.st().sketch_session.as_ref().unwrap().tool,
        Tool::MoveCopy
    );
    rig.frame(vec![key(Key::Escape)]);
    rig.settle();
    assert_eq!(rig.st().sketch_session.as_ref().unwrap().tool, Tool::Select);
}
