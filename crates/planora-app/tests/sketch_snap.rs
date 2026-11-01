//! Fang, Anbindung und Ausrichtung beim Zeichnen (Skizzen 2, Task 2): Vorschau und Klick nutzen
//! denselben aufgelösten Zeiger; die Oberflächentests laufen ohne Fenster bei 1920 × 1080.

use planora_app::sketch_cursor::{CursorOptions, DrawCursor};
use planora_app::sketch_state::{Item, SketchSession, Snap, Tool};
use planora_app::state::{AppState, StandardPlane};
use planora_app::{fonts, sketch_view};
use planora_feature::{FeatureId, SketchPlane};
use planora_math::{DVec2, Frame, Tolerance};
use planora_sketch::constraint::ConstraintKind as K;
use planora_sketch::model::{CurveId, PointId, Sketch};
use std::sync::Arc;

/// Fangradius in Skizzeneinheiten; entspricht 8 px, ein Pixel ist also `PX`.
const R: f64 = 8.0;
const PX: f64 = R / 8.0;

fn setup(tool: Tool) -> (Sketch, SketchSession) {
    let mut sk = Sketch::new(Frame::WORLD, "S", Tolerance::default());
    let mut se = SketchSession::new(FeatureId(0));
    se.set_tool(&mut sk, tool);
    (sk, se)
}

fn line(sk: &mut Sketch, a: (f64, f64), b: (f64, f64)) -> CurveId {
    let p = sk.add_point(a.0, a.1).unwrap();
    let q = sk.add_point(b.0, b.1).unwrap();
    sk.add_line(p, q).unwrap()
}

fn click(se: &mut SketchSession, sk: &mut Sketch, x: f64, y: f64) {
    se.click(sk, DVec2::new(x, y), R, false).unwrap();
}

fn kinds(sk: &Sketch) -> Vec<K> {
    sk.constraints().map(|(_, c)| c.kind.clone()).collect()
}

fn newest_point(sk: &Sketch) -> PointId {
    sk.points().map(|(p, _)| p).last().unwrap()
}

fn newest_curve(sk: &Sketch) -> CurveId {
    sk.curves().map(|(c, _)| c).last().unwrap()
}

fn assert_at(sk: &Sketch, p: PointId, x: f64, y: f64) {
    let q = sk.point_xy(p).unwrap();
    assert!(
        (q.x - x).abs() < 1e-6 && (q.y - y).abs() < 1e-6,
        "{q:?} statt ({x}, {y})"
    );
}

// ---- Anbindung ----------------------------------------------------------------------------

#[test]
fn a_click_on_the_middle_of_a_line_adds_a_midpoint_constraint() {
    let (mut sk, mut se) = setup(Tool::Point);
    let l = line(&mut sk, (100.0, 100.0), (200.0, 100.0));
    click(&mut se, &mut sk, 151.0, 102.0);
    let p = newest_point(&sk);
    assert_eq!(kinds(&sk), vec![K::Midpoint(p, l)]);
    assert_at(&sk, p, 150.0, 100.0);
}

#[test]
fn a_click_on_the_middle_of_an_arc_binds_to_the_curve() {
    let (mut sk, mut se) = setup(Tool::Point);
    let m = sk.add_point(100.0, 100.0).unwrap();
    let s = sk.add_point(200.0, 100.0).unwrap();
    let e = sk.add_point(100.0, 200.0).unwrap();
    let arc = sk.add_arc(m, s, e).unwrap();
    let before = kinds(&sk).len();
    let d = 100.0 * std::f64::consts::FRAC_1_SQRT_2;
    click(&mut se, &mut sk, 100.0 + d + 1.0, 100.0 + d);
    let p = newest_point(&sk);
    assert_eq!(kinds(&sk)[before..], [K::PointOnCurve(p, arc)]);
    assert_at(&sk, p, 100.0 + d, 100.0 + d);
}

#[test]
fn a_click_on_a_crossing_binds_to_both_curves() {
    let (mut sk, mut se) = setup(Tool::Point);
    let a = line(&mut sk, (100.0, 100.0), (300.0, 300.0));
    let b = line(&mut sk, (100.0, 240.0), (340.0, 120.0));
    let cur = se.resolve_cursor(&sk, DVec2::new(192.0, 193.0), R);
    assert_eq!(cur.snap, Snap::Intersection(a, b));
    let q = cur.at;
    click(&mut se, &mut sk, q.x + 1.0, q.y - 1.0);
    let p = newest_point(&sk);
    assert_eq!(
        kinds(&sk),
        vec![K::PointOnCurve(p, a), K::PointOnCurve(p, b)]
    );
    assert!(sk.point_xy(p).unwrap().distance(q) < 1e-6);
}

#[test]
fn the_origin_gets_a_fixed_point_and_then_snaps_as_that_point() {
    let (mut sk, mut se) = setup(Tool::Point);
    click(&mut se, &mut sk, 1.0, -1.0);
    let p = newest_point(&sk);
    assert_at(&sk, p, 0.0, 0.0);
    assert_eq!(kinds(&sk), vec![K::Fixed(p, 0.0, 0.0)]);

    click(&mut se, &mut sk, 1.0, 1.0);
    assert_eq!(
        sk.points().count(),
        1,
        "zweiter Klick nutzt denselben Punkt"
    );

    se.set_tool(&mut sk, Tool::Line);
    click(&mut se, &mut sk, -1.0, 0.5);
    assert_eq!(se.pending, vec![p]);
    assert_eq!(kinds(&sk).len(), 1);
}

#[test]
fn a_line_on_a_point_shares_it() {
    let (mut sk, mut se) = setup(Tool::Line);
    let l = line(&mut sk, (100.0, 100.0), (200.0, 130.0));
    let ends = sk.curve(l).unwrap().kind.points();
    click(&mut se, &mut sk, 201.0, 131.0);
    assert!(ends.contains(&se.pending[0]));
    assert_eq!(sk.points().count(), 2);
}

#[test]
fn a_rectangle_binds_both_corners_to_points_and_curves() {
    let (mut sk, mut se) = setup(Tool::Rectangle);
    let start = sk.add_point(100.0, 100.0).unwrap();
    let wall = line(&mut sk, (300.0, 0.0), (300.0, 500.0));

    click(&mut se, &mut sk, 101.0, 99.0);
    assert!(kinds(&sk).is_empty(), "die erste Ecke legt noch nichts an");
    click(&mut se, &mut sk, 301.0, 200.0);

    let all = kinds(&sk);
    let coincident: Vec<_> = all
        .iter()
        .filter(|k| matches!(k, K::Coincident(..)))
        .collect();
    assert_eq!(coincident.len(), 1, "{all:?}");
    let K::Coincident(corner, other) = *coincident[0] else {
        unreachable!()
    };
    assert_eq!(other, start);
    assert_at(&sk, corner, 100.0, 100.0);

    let on_wall: Vec<_> = all
        .iter()
        .filter(|k| matches!(k, K::PointOnCurve(_, c) if *c == wall))
        .collect();
    assert_eq!(on_wall.len(), 1, "{all:?}");
    let K::PointOnCurve(far, _) = *on_wall[0] else {
        unreachable!()
    };
    assert_at(&sk, far, 300.0, 200.0);
    assert!(se.pending_snaps.is_empty());
    assert!(se.pending_pos.is_empty());
}

#[test]
fn a_rectangle_drawn_backwards_binds_the_right_corners() {
    let (mut sk, mut se) = setup(Tool::Rectangle);
    let a = sk.add_point(300.0, 300.0).unwrap();
    let b = sk.add_point(100.0, 100.0).unwrap();
    click(&mut se, &mut sk, 300.0, 300.0);
    click(&mut se, &mut sk, 100.0, 100.0);
    let pairs: Vec<(PointId, PointId)> = kinds(&sk)
        .iter()
        .filter_map(|k| match k {
            K::Coincident(x, y) => Some((*x, *y)),
            _ => None,
        })
        .collect();
    assert_eq!(pairs.len(), 2);
    assert_at(&sk, pairs[0].0, 300.0, 300.0);
    assert_eq!(pairs[0].1, a);
    assert_at(&sk, pairs[1].0, 100.0, 100.0);
    assert_eq!(pairs[1].1, b);
}

#[test]
fn a_failed_rectangle_keeps_the_first_corner_and_its_snap() {
    let (mut sk, mut se) = setup(Tool::Rectangle);
    sk.add_point(100.0, 100.0).unwrap();
    click(&mut se, &mut sk, 100.0, 100.0);
    let at = DVec2::new(100.0, 100.0);
    assert!(se.click(&mut sk, at, R, false).is_err());
    assert_eq!(se.pending_pos.len(), 1);
    assert_eq!(se.pending_snaps.len(), 1);
}

#[test]
fn stopping_the_tool_forgets_the_first_corner_snap() {
    let (mut sk, mut se) = setup(Tool::Rectangle);
    sk.add_point(100.0, 100.0).unwrap();
    click(&mut se, &mut sk, 100.0, 100.0);
    se.cancel(&mut sk);
    assert!(se.pending_snaps.is_empty());
    click(&mut se, &mut sk, 100.0, 100.0);
    se.finish_tool(&mut sk).unwrap();
    assert!(se.pending_snaps.is_empty());
    click(&mut se, &mut sk, 100.0, 100.0);
    se.after_history_jump(&sk);
    assert!(se.pending_snaps.is_empty());
    click(&mut se, &mut sk, 100.0, 100.0);
    se.set_tool(&mut sk, Tool::Rectangle);
    assert!(se.pending_snaps.is_empty());
}

#[test]
fn a_first_corner_whose_curve_was_deleted_stays_free() {
    let (mut sk, mut se) = setup(Tool::Rectangle);
    let wall = line(&mut sk, (100.0, 0.0), (100.0, 500.0));
    click(&mut se, &mut sk, 101.0, 100.0);
    se.selection = vec![Item::Curve(wall)];
    se.delete_selection(&mut sk);
    click(&mut se, &mut sk, 300.0, 300.0);
    assert!(
        !kinds(&sk).iter().any(|k| matches!(k, K::PointOnCurve(..))),
        "{:?}",
        kinds(&sk)
    );
    assert_eq!(sk.curves().count(), 4);
}

#[test]
fn a_circle_rim_on_a_point_binds_the_point_and_otherwise_nothing() {
    let (mut sk, mut se) = setup(Tool::Circle);
    let rim = sk.add_point(200.0, 100.0).unwrap();
    click(&mut se, &mut sk, 100.0, 100.0);
    click(&mut se, &mut sk, 201.0, 101.0);
    let circle = newest_curve(&sk);
    assert_eq!(kinds(&sk), vec![K::PointOnCurve(rim, circle)]);
    assert!((sk.radius(circle).unwrap() - 100.0).abs() < 1e-6);

    // Rand auf einer Kurve: Lage ja, Abhängigkeit nein.
    let (mut sk, mut se) = setup(Tool::Circle);
    line(&mut sk, (300.0, 0.0), (300.0, 500.0));
    click(&mut se, &mut sk, 100.0, 77.0);
    click(&mut se, &mut sk, 301.0, 77.0);
    assert!(kinds(&sk).is_empty());
    let circle = newest_curve(&sk);
    assert!((sk.radius(circle).unwrap() - 200.0).abs() < 1e-6);
}

#[test]
fn an_ellipse_binds_a_snapped_major_end() {
    let (mut sk, mut se) = setup(Tool::Ellipse);
    let major = sk.add_point(300.0, 100.0).unwrap();
    click(&mut se, &mut sk, 100.0, 100.0);
    click(&mut se, &mut sk, 301.0, 101.0);
    click(&mut se, &mut sk, 100.0, 180.0);
    let ellipse = newest_curve(&sk);
    assert!(
        kinds(&sk).contains(&K::PointOnCurve(major, ellipse)),
        "{:?}",
        kinds(&sk)
    );
    assert!(se.pending_snaps.is_empty());
}

#[test]
fn the_end_of_an_arc_binds_only_when_it_lands_on_the_snap() {
    // Das Ende liegt auf dem Radius, die Fanglage auf der Linie weit außerhalb: keine Anbindung.
    let (mut sk, mut se) = setup(Tool::Arc);
    line(&mut sk, (500.0, -500.0), (500.0, 500.0));
    click(&mut se, &mut sk, 100.0, 77.0);
    click(&mut se, &mut sk, 200.0, 77.0);
    click(&mut se, &mut sk, 501.0, 300.0);
    assert!(
        !kinds(&sk).iter().any(|k| matches!(k, K::PointOnCurve(..))),
        "{:?}",
        kinds(&sk)
    );
}

// ---- Ausrichtung ----------------------------------------------------------------------------

#[test]
fn near_the_extension_of_a_point_the_cursor_aligns_without_a_constraint() {
    let (mut sk, mut se) = setup(Tool::Point);
    let a = sk.add_point(100.0, 100.0).unwrap();
    let cur = se.resolve_cursor(&sk, DVec2::new(100.0 + 3.0 * PX, 400.0), R);
    assert_eq!(cur.at.x, 100.0);
    assert_eq!(cur.at.y, 400.0);
    assert_eq!(cur.guides.x, Some((a, 100.0)));
    assert_eq!(cur.guides.y, None);
    assert_eq!(cur.snap, Snap::None);

    click(&mut se, &mut sk, 100.0 + 3.0 * PX, 400.0);
    assert!(kinds(&sk).is_empty());
    assert_eq!(sk.point_xy(newest_point(&sk)).unwrap().x, 100.0);
}

#[test]
fn the_cursor_aligns_to_two_points_at_once() {
    let (mut sk, mut se) = setup(Tool::Point);
    let a = sk.add_point(100.0, 100.0).unwrap();
    let b = sk.add_point(400.0, 400.0).unwrap();
    let cur = se.resolve_cursor(&sk, DVec2::new(102.0, 398.0), R);
    assert_eq!(cur.at, DVec2::new(100.0, 400.0));
    assert_eq!(cur.guides.x, Some((a, 100.0)));
    assert_eq!(cur.guides.y, Some((b, 400.0)));
}

#[test]
fn alignment_reaches_only_six_tenths_of_the_radius() {
    let (mut sk, mut se) = setup(Tool::Point);
    sk.add_point(100.0, 100.0).unwrap();
    let raw = DVec2::new(100.0 + 0.7 * R, 400.0);
    let cur = se.resolve_cursor(&sk, raw, R);
    assert_eq!(cur.at, raw);
    assert_eq!(cur.guides.x, None);
}

#[test]
fn a_curve_goes_before_alignment() {
    let (mut sk, mut se) = setup(Tool::Point);
    sk.add_point(100.0, 100.0).unwrap();
    let l = line(&mut sk, (0.0, 400.0), (1000.0, 400.0));
    let cur = se.resolve_cursor(&sk, DVec2::new(102.0, 403.0), R);
    assert_eq!(cur.snap, Snap::Curve(l));
    assert_eq!(cur.guides.x, None);
    assert!((cur.at.x - 102.0).abs() < 1e-9 && (cur.at.y - 400.0).abs() < 1e-9);
}

#[test]
fn the_running_points_do_not_align() {
    let (mut sk, mut se) = setup(Tool::Line);
    // Fusion-Verhalten, Skizzen 2: Sonst würde die fast senkrechte Linie exakt senkrecht.
    se.auto_constrain = false;
    click(&mut se, &mut sk, 100.0, 100.0);
    let cur = se.resolve_cursor(&sk, DVec2::new(102.0, 400.0), R);
    assert_eq!(cur.guides.x, None);
    assert_eq!(cur.at, DVec2::new(102.0, 400.0));
}

#[test]
fn the_first_rectangle_corner_does_not_align_over_its_position() {
    let (mut sk, mut se) = setup(Tool::Rectangle);
    sk.add_point(100.0, 100.0).unwrap();
    click(&mut se, &mut sk, 100.0, 100.0);
    let cur = se.resolve_cursor(&sk, DVec2::new(102.0, 400.0), R);
    assert_eq!(cur.guides.x, None);
}

#[test]
fn the_grid_applies_to_free_axes_only_and_binds_nothing() {
    let (mut sk, mut se) = setup(Tool::Point);
    se.cursor_opts.grid_step = Some(10.0);
    let cur = se.resolve_cursor(&sk, DVec2::new(123.0, 456.0), R);
    assert_eq!(cur.at, DVec2::new(120.0, 460.0));
    assert_eq!(cur.snap, Snap::Grid);

    let a = sk.add_point(103.0, 77.0).unwrap();
    let cur = se.resolve_cursor(&sk, DVec2::new(105.0, 456.0), R);
    assert_eq!(cur.at, DVec2::new(103.0, 460.0), "Ausrichtung + Raster");
    assert_eq!(cur.guides.x, Some((a, 103.0)));

    let cur = se.resolve_cursor(&sk, DVec2::new(104.0, 78.0), R);
    assert_eq!(cur.snap, Snap::Point(a), "Geometrie geht vor Raster");

    click(&mut se, &mut sk, 123.0, 456.0);
    assert!(kinds(&sk).is_empty());
    assert_at(&sk, newest_point(&sk), 120.0, 460.0);
}

// ---- Eigenschaften des aufgelösten Zeigers --------------------------------------------------

fn same(a: &DrawCursor, b: &DrawCursor) -> bool {
    a.raw.to_array().map(f64::to_bits) == b.raw.to_array().map(f64::to_bits)
        && a.at.to_array().map(f64::to_bits) == b.at.to_array().map(f64::to_bits)
        && a.snap == b.snap
        && a.snap_at == b.snap_at
        && a.guides == b.guides
        && a.autos == b.autos
}

#[test]
fn resolving_twice_is_bit_identical_and_builds_the_index_once() {
    let (mut sk, mut se) = setup(Tool::Line);
    line(&mut sk, (100.0, 100.0), (200.0, 130.0));
    let raw = DVec2::new(151.3, 116.7);
    let first = se.resolve_cursor(&sk, raw, R);
    let index = se.snap_index.clone().unwrap();
    assert_eq!(index.revision(), sk.revision());
    let second = se.resolve_cursor(&sk, raw, R);
    assert!(same(&first, &second), "{first:?} / {second:?}");
    assert!(Arc::ptr_eq(&index, se.snap_index.as_ref().unwrap()));

    sk.add_point(0.5, 900.0).unwrap();
    se.resolve_cursor(&sk, raw, R);
    assert!(!Arc::ptr_eq(&index, se.snap_index.as_ref().unwrap()));
}

#[test]
fn only_drawing_tools_build_an_index_and_snap() {
    for tool in [
        Tool::Select,
        Tool::ProjectEdge,
        Tool::Trim,
        Tool::Extend,
        Tool::Break,
    ] {
        let (mut sk, mut se) = setup(tool);
        assert!(!se.is_drawing_tool());
        line(&mut sk, (100.0, 100.0), (200.0, 130.0));
        let raw = DVec2::new(101.0, 101.0);
        se.update_cursor(&sk, Some(raw), R);
        assert!(se.draw_cursor.is_none(), "{tool:?}");
        assert_eq!(se.resolve_cursor(&sk, raw, R).at, raw);
        assert!(se.snap_index.is_none(), "{tool:?}");
    }
    for tool in [
        Tool::Point,
        Tool::Line,
        Tool::Rectangle,
        Tool::Circle,
        Tool::Arc,
        Tool::Ellipse,
        Tool::Spline,
    ] {
        let (sk, mut se) = setup(tool);
        assert!(se.is_drawing_tool());
        se.update_cursor(&sk, Some(DVec2::new(50.0, 50.0)), R);
        assert!(se.draw_cursor.is_some(), "{tool:?}");
        se.update_cursor(&sk, None, R);
        assert!(se.draw_cursor.is_none(), "{tool:?}");
    }
}

#[test]
fn the_selection_snap_still_knows_only_points_and_curves() {
    let (mut sk, se) = setup(Tool::Select);
    let l = line(&mut sk, (100.0, 100.0), (200.0, 100.0));
    assert_eq!(
        se.snap(&sk, DVec2::new(150.0, 101.0), R),
        Snap::Curve(l),
        "die Linienmitte ist für die Auswahl nur die Kurve"
    );
    assert_eq!(se.snap(&sk, DVec2::new(0.0, 0.0), R), Snap::None);
}

// ---- Strg (Review-Focus 2) ------------------------------------------------------------------

#[test]
fn with_ctrl_a_click_on_a_point_makes_a_new_free_point() {
    let (mut sk, mut se) = setup(Tool::Line);
    let l = line(&mut sk, (100.0, 100.0), (200.0, 130.0));
    se.cursor_opts = CursorOptions {
        suppressed: true,
        grid_step: Some(10.0),
    };
    let raw = DVec2::new(201.0, 131.0);
    let cur = se.resolve_cursor(&sk, raw, R);
    assert_eq!(cur.at, raw);
    assert_eq!(cur.snap, Snap::None);
    assert_eq!(cur.guides, Default::default());

    se.click(&mut sk, raw, R, false).unwrap();
    assert!(!sk.curve(l).unwrap().kind.points().contains(&se.pending[0]));
    assert_eq!(sk.points().count(), 3);
    assert_eq!(sk.point_xy(se.pending[0]).unwrap(), raw);
    assert!(kinds(&sk).is_empty());

    // Auch keine automatische Abhängigkeit: Fast waagerecht bleibt ohne H.
    se.click(&mut sk, DVec2::new(400.0, 133.0), R, false)
        .unwrap();
    assert!(kinds(&sk).is_empty());
}

// ---- Oberfläche bei 1920 × 1080 -------------------------------------------------------------

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

fn set_tool(s: &mut AppState, tool: Tool) {
    let (se, sk) = s.session_sketch_mut().unwrap();
    se.set_tool(sk, tool);
}

fn screen(s: &AppState, x: f64, y: f64) -> egui::Pos2 {
    let frame = *s.session_sketch().unwrap().frame();
    let mut cam = s.camera;
    cam.set_viewport(W, H);
    let v = sketch_view::sketch_to_screen(&cam, &frame, glam::Vec2::new(W, H), DVec2::new(x, y))
        .unwrap();
    egui::pos2(v.x, v.y)
}

/// Skizzeneinheiten je Pixel in der Mitte des Bildes.
fn unit_per_px(s: &AppState) -> f64 {
    let frame = *s.session_sketch().unwrap().frame();
    let mut cam = s.camera;
    cam.set_viewport(W, H);
    let mid = glam::Vec2::new(W / 2.0, H / 2.0);
    sketch_view::pick_radius_units(&cam, &frame, glam::Vec2::new(W, H), mid, 1.0).unwrap()
}

fn moved(p: egui::Pos2) -> egui::Event {
    egui::Event::PointerMoved(p)
}

fn button(p: egui::Pos2, pressed: bool, modifiers: egui::Modifiers) -> egui::Event {
    egui::Event::PointerButton {
        pos: p,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers,
    }
}

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

fn click_with(ctx: &egui::Context, s: &mut AppState, p: egui::Pos2, modifiers: egui::Modifiers) {
    play(
        ctx,
        s,
        vec![
            vec![moved(p)],
            vec![button(p, true, modifiers)],
            vec![button(p, false, modifiers)],
            vec![],
        ],
    );
}

fn cursor(s: &AppState) -> DrawCursor {
    s.sketch_session
        .as_ref()
        .unwrap()
        .draw_cursor
        .clone()
        .expect("aufgelöster Zeiger")
}

#[test]
fn the_overlay_shows_marks_and_guides_without_changing_the_sketch() {
    let ctx = context();
    let mut s = sketch_with(|sk| {
        line(sk, (10.0, 10.0), (30.0, 18.0));
    });
    set_tool(&mut s, Tool::Line);
    let px = unit_per_px(&s);
    let stand = s.session_sketch().unwrap().revision();

    // Zeichen am Punkt, an der Linienmitte, am Ursprung
    let p = screen(&s, 10.0, 10.0);
    play(&ctx, &mut s, vec![vec![moved(p + egui::vec2(2.0, 1.0))]]);
    assert!(matches!(cursor(&s).snap, Snap::Point(_)));
    let p = screen(&s, 20.0, 14.0);
    play(&ctx, &mut s, vec![vec![moved(p + egui::vec2(1.0, 1.0))]]);
    assert!(matches!(cursor(&s).snap, Snap::Midpoint(_)));
    let p = screen(&s, 0.0, 0.0);
    play(&ctx, &mut s, vec![vec![moved(p + egui::vec2(1.0, 1.0))]]);
    assert_eq!(cursor(&s).snap, Snap::Origin);

    // Hilfslinie: 3 px neben der Senkrechten durch (10, 10), weit weg von allem anderen.
    let p = screen(&s, 10.0 + 3.0 * px, 10.0 + 200.0 * px);
    play(&ctx, &mut s, vec![vec![moved(p)]]);
    let cur = cursor(&s);
    assert!(cur.guides.x.is_some(), "{cur:?}");
    assert_eq!(cur.at.x, 10.0);
    assert_eq!(cur.snap, Snap::None);

    assert_eq!(
        s.session_sketch().unwrap().revision(),
        stand,
        "Zeichen ändern die Skizze nicht"
    );
    assert!(kinds(s.session_sketch().unwrap()).is_empty());

    // Der Klick setzt genau die gezeigte Lage.
    click_with(&ctx, &mut s, p, egui::Modifiers::default());
    let se = s.sketch_session.as_ref().unwrap();
    let sk = s.session_sketch().unwrap();
    assert_eq!(sk.point_xy(se.pending[0]).unwrap(), cur.at);
}

#[test]
fn ctrl_only_in_the_click_event_still_switches_snapping_off() {
    let ctx = context();
    let mut s = sketch_with(|sk| {
        line(sk, (10.0, 10.0), (30.0, 18.0));
    });
    set_tool(&mut s, Tool::Line);
    let p = screen(&s, 30.0, 18.0);
    let ends = {
        let sk = s.session_sketch().unwrap();
        let (c, _) = sk.curves().next().unwrap();
        sk.curve(c).unwrap().kind.points()
    };
    // RawInput::modifiers bleibt leer: Die Taste steht nur im Ereignis des Klicks.
    click_with(&ctx, &mut s, p, egui::Modifiers::CTRL);

    let se = s.sketch_session.as_ref().unwrap();
    let sk = s.session_sketch().unwrap();
    assert_eq!(se.pending.len(), 1);
    assert!(
        !ends.contains(&se.pending[0]),
        "neuer Punkt, nicht der alte"
    );
    assert_eq!(sk.points().count(), 3);
    assert!(kinds(sk).is_empty());

    // Ohne Strg rastet das nächste Überfahren wieder.
    play(
        &ctx,
        &mut s,
        vec![vec![moved(p + egui::vec2(9.0, 40.0))], vec![moved(p)]],
    );
    assert!(matches!(cursor(&s).snap, Snap::Point(_)));
}

#[test]
fn a_held_ctrl_key_switches_the_marks_off() {
    let ctx = context();
    let mut s = sketch_with(|sk| {
        line(sk, (10.0, 10.0), (30.0, 18.0));
    });
    set_tool(&mut s, Tool::Line);
    let p = screen(&s, 30.0, 18.0);
    play(
        &ctx,
        &mut s,
        vec![
            vec![egui::Event::ModifiersChanged(egui::Modifiers::CTRL)],
            vec![moved(p)],
        ],
    );
    let cur = cursor(&s);
    assert_eq!(cur.snap, Snap::None);
    assert_eq!(cur.at, cur.raw);
}

#[test]
fn a_fixed_at_the_origin_has_no_label() {
    let ctx = context();
    let mut sk = Sketch::new(Frame::WORLD, "S", Tolerance::default());
    let p = sk.add_point(0.0, 0.0).unwrap();
    let q = sk.add_point(5.0, 0.0).unwrap();
    let at_origin = K::Fixed(p, 0.0, 0.0);
    let elsewhere = K::Fixed(q, 5.0, 0.0);
    ctx.run_ui(egui::RawInput::default(), |_| {})
        .drop_without_applying_deltas();
    assert_eq!(sketch_view::overlay_text(&ctx, &sk, &at_origin, false), "");
    assert_eq!(
        sketch_view::overlay_text(&ctx, &sk, &elsewhere, false),
        "fix"
    );
}
