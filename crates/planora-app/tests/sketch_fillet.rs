//! Skizzen 4, Task 7: Rundung und Fase in der App. Klicks über die Sitzung (`commit_input`) und je
//! eine Fahrt über Rohereignisse bei 1920×1080, Muster `sketch_trim.rs`.

mod common;

use eframe::App as _;
use egui::{Event, Key, Modifiers, PointerButton, Pos2, Rect, Vec2, pos2};
use planora_app::app::PlanoraApp;
use planora_app::sketch_input::FieldKind as F;
use planora_app::sketch_kinds::ToolKind;
use planora_app::sketch_state::{Item, Snap, Tool};
use planora_app::state::{AppState, StandardPlane};
use planora_app::workspace::Workspace;
use planora_app::{fonts, sketch_view, theme};
use planora_feature::{ParamEdit, ParamId, ParamUnit, SketchPlane};
use planora_i18n::{DisplayUnit, t, with_unit};
use planora_math::DVec2;
use planora_project::config::Config;
use planora_project::library::Library;
use planora_sketch::constraint::{ConstraintId, ConstraintKind as K};
use planora_sketch::model::{CurveId, CurveKind, Sketch};
use planora_sketch::shapes::ShapePlan;
use planora_sketch::solver::SolveStatus;

const W: f32 = 1920.0;
const H: f32 = 1080.0;
const SCREEN: Rect = Rect {
    min: pos2(0.0, 0.0),
    max: pos2(W, H),
};
const PICK: f64 = 0.5;

// ---- Hilfen ---------------------------------------------------------------------------------

fn v(x: f64, y: f64) -> DVec2 {
    DVec2::new(x, y)
}

/// Offene, leere Skizze auf XY; `build` legt den Inhalt an.
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

/// Rechteck (0,0)–(50,30) mit dem Rechteckwerkzeug, also samt H und V.
fn rectangle() -> AppState {
    let mut s = sketch_with(|_| {});
    set_tool(&mut s, Tool::Rectangle);
    click(&mut s, v(0.0, 0.0)).unwrap();
    click(&mut s, v(50.0, 30.0)).unwrap();
    set_tool(&mut s, Tool::Select);
    s.observe_sketch(false);
    s
}

/// Zwei Linien mit gemeinsamem Punkt im Ursprung, rechter Winkel, ganz frei.
fn corner() -> AppState {
    sketch_with(|sk| {
        let a = sk.add_point(40.0, 0.0).unwrap();
        let p = sk.add_point(0.0, 0.0).unwrap();
        let b = sk.add_point(0.0, 40.0).unwrap();
        sk.add_line(a, p).unwrap();
        sk.add_line(p, b).unwrap();
    })
}

fn set_tool(s: &mut AppState, tool: Tool) {
    let (se, sk) = s.session_sketch_mut().unwrap();
    se.set_tool(sk, tool);
}

fn choose(s: &mut AppState, kind: ToolKind) {
    let (se, sk) = s.session_sketch_mut().unwrap();
    se.choose_kind(sk, kind);
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

/// Text in das Feld `i` der aktuellen Stufe.
fn type_in(s: &mut AppState, i: usize, text: &str) {
    let scope = s.document.param_scope();
    let (se, sk) = s.session_sketch_mut().unwrap();
    se.sync_input(sk);
    se.type_field(i, text, &scope);
}

fn stage(s: &AppState) -> usize {
    s.sketch_session.as_ref().unwrap().kind_stage()
}

fn message(s: &AppState) -> Option<String> {
    s.sketch_session.as_ref().unwrap().message_text()
}

fn undo_len(s: &AppState) -> usize {
    s.sketch_history().unwrap().undo_len()
}

fn sketch(s: &AppState) -> &Sketch {
    s.session_sketch().unwrap()
}

fn preview(s: &mut AppState, at: DVec2) -> Option<Vec<Vec<DVec2>>> {
    let (se, sk) = s.session_sketch_mut().unwrap();
    se.update_cursor(sk, Some(at), PICK);
    se.kind_preview(sk, at)
}

/// Vergleichbarer Stand mit den genauen Bits.
fn snapshot(s: &AppState) -> String {
    let sk = sketch(s);
    let curves: Vec<String> = sk
        .curves()
        .map(|(id, c)| format!("{} {:?} {}", id.index(), c.kind, c.construction))
        .collect();
    let constraints: Vec<String> = sk
        .constraints()
        .map(|(id, c)| format!("{} {:?}", id.index(), c.kind))
        .collect();
    let bits: Vec<u64> = sk.variables().iter().map(|x| x.to_bits()).collect();
    format!("{curves:?}\n{bits:?}\n{constraints:?}")
}

fn count(sk: &Sketch, wanted: impl Fn(&K) -> bool) -> usize {
    sk.constraints().filter(|(_, c)| wanted(&c.kind)).count()
}

fn arcs(sk: &Sketch) -> Vec<CurveId> {
    sk.curves()
        .filter(|(_, c)| matches!(c.kind, CurveKind::Arc { .. }))
        .map(|(id, _)| id)
        .collect()
}

fn arc_parts(sk: &Sketch, arc: CurveId) -> (DVec2, DVec2, DVec2) {
    let CurveKind::Arc { center, start, end } = sk.curve(arc).unwrap().kind else {
        panic!("kein Bogen");
    };
    (
        sk.point_xy(center).unwrap(),
        sk.point_xy(start).unwrap(),
        sk.point_xy(end).unwrap(),
    )
}

fn radius_dim(sk: &Sketch, arc: CurveId) -> ConstraintId {
    sk.constraints()
        .find(|(_, c)| matches!(c.kind, K::Radius(a, _) if a == arc))
        .expect("Radiusmaß")
        .0
}

fn line_ends(sk: &Sketch, c: CurveId) -> (DVec2, DVec2) {
    let CurveKind::Line { a, b } = sk.curve(c).unwrap().kind else {
        panic!("keine Linie");
    };
    (sk.point_xy(a).unwrap(), sk.point_xy(b).unwrap())
}

/// Die normale Linie, auf der `at` liegt.
fn line_through(sk: &Sketch, at: DVec2) -> CurveId {
    sk.curves()
        .find(|(id, c)| {
            !c.construction
                && matches!(c.kind, CurveKind::Line { .. })
                && sk.nearest_on_curve(*id, at).unwrap().distance(at) < 1e-6
        })
        .expect("Linie an der Stelle")
        .0
}

fn assert_clean(sk: &Sketch) {
    let report = sk.clone().solve();
    assert_eq!(report.status, SolveStatus::Solved);
    assert!(report.redundant.is_empty(), "{:?}", report.redundant);
    assert!(report.conflicting.is_empty(), "{:?}", report.conflicting);
}

fn close(a: DVec2, b: DVec2) -> bool {
    a.distance(b) < 1e-9
}

// ---- Rohereignisse ohne Fenster ---------------------------------------------------------------

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

fn button_with(p: Pos2, pressed: bool, modifiers: Modifiers) -> Event {
    Event::PointerButton {
        pos: p,
        button: PointerButton::Primary,
        pressed,
        modifiers,
    }
}

fn esc() -> Event {
    Event::Key {
        key: Key::Escape,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::NONE,
    }
}

fn play(ctx: &egui::Context, s: &mut AppState, frames: Vec<Vec<Event>>) {
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

fn raw_click_with(ctx: &egui::Context, s: &mut AppState, at: DVec2, modifiers: Modifiers) {
    let p = screen(s, at);
    let mut input = egui::RawInput {
        screen_rect: Some(SCREEN),
        ..Default::default()
    };
    let mut frames: Vec<Vec<Event>> = vec![
        vec![Event::PointerMoved(p)],
        vec![button_with(p, true, modifiers)],
        vec![button_with(p, false, modifiers)],
        vec![],
    ];
    for events in frames.drain(..) {
        input.events = events;
        let mut gesture = false;
        ctx.run_ui(input.clone(), |ui| {
            sketch_view::show_overlay(ui, SCREEN, s);
            gesture = ui.input(|i| i.pointer.any_down());
        })
        .drop_without_applying_deltas();
        s.observe_sketch(gesture);
    }
}

fn raw_click(ctx: &egui::Context, s: &mut AppState, at: DVec2) {
    play(ctx, s, vec![vec![]]);
    raw_click_with(ctx, s, at, Modifiers::NONE);
}

// ---- Die ganze App -----------------------------------------------------------------------------

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
        let root = common::temp_root("planora-sketch-fillet");
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

    fn frame(&mut self, events: Vec<Event>) {
        self.time += 1.0 / 60.0;
        let raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(W, H))),
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

    fn press(&mut self, k: Key) {
        self.frame(vec![Event::Key {
            key: k,
            physical_key: Some(k),
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        }]);
        self.settle();
    }

    /// Lage in der Skizze unter dem Bildpunkt `p`.
    fn at_pixel(&mut self, p: Pos2) -> DVec2 {
        self.frame(vec![Event::PointerMoved(p)]);
        self.frame(vec![]);
        let se = self.st().sketch_session.as_ref().unwrap();
        se.draw_cursor.as_ref().expect("Zeiger").raw
    }

    fn click(&mut self, p: Pos2) {
        self.frame(vec![Event::PointerMoved(p)]);
        self.frame(vec![button_with(p, true, Modifiers::NONE)]);
        self.frame(vec![button_with(p, false, Modifiers::NONE)]);
        self.frame(vec![]);
    }
}

#[test]
fn the_whole_trip_with_r_f_two_sides_a_typed_radius_and_a_click() {
    let mut rig = Rig::new();
    rig.press(Key::R);
    rig.click(pos2(800.0, 350.0));
    rig.click(pos2(1200.0, 700.0));
    rig.press(Key::Escape);
    let sk = sketch(rig.st()).clone();
    assert_eq!(sk.curves().count(), 4);
    let dof = sk.clone().solve().dof;
    let hv = count(&sk, |k| matches!(k, K::Horizontal(..) | K::Vertical(..)));

    rig.press(Key::F);
    assert_eq!(rig.st().sketch_session.as_ref().unwrap().tool, Tool::Fillet);
    // Untere und rechte Kante, je in der Mitte. Das Raster hat die Ecken verschoben, also die Lage
    // auf dem Schirm aus zwei Zeigerlagen und den Punkten der Skizze.
    let (x0, y0) = (
        rig.at_pixel(pos2(900.0, 500.0)),
        rig.at_pixel(pos2(1100.0, 600.0)),
    );
    let pixel = |p: DVec2| {
        let tx = (p.x - x0.x) / (y0.x - x0.x);
        let ty = (p.y - x0.y) / (y0.y - x0.y);
        pos2(900.0 + 200.0 * tx as f32, 500.0 + 100.0 * ty as f32)
    };
    let (lo, hi) = {
        let sk = sketch(rig.st());
        let xy: Vec<DVec2> = sk.points().map(|(p, _)| sk.point_xy(p).unwrap()).collect();
        let fold = |f: fn(DVec2, DVec2) -> DVec2| xy.iter().copied().reduce(f).unwrap();
        (fold(DVec2::min), fold(DVec2::max))
    };
    rig.click(pixel(v((lo.x + hi.x) / 2.0, lo.y)));
    rig.click(pixel(v(hi.x, (lo.y + hi.y) / 2.0)));
    assert_eq!(stage(rig.st()), 2);
    let radius_field = rig.st().sketch_session.as_ref().unwrap().input.fields[0].kind;
    assert_eq!(radius_field, F::Radius);
    type_in(rig.st(), 0, "5");
    rig.frame(vec![]);
    let steps = undo_len(rig.st());
    let at = pixel(v(hi.x - 12.0, lo.y + 8.0));
    let shown = {
        rig.frame(vec![Event::PointerMoved(at)]);
        rig.frame(vec![]);
        let s = rig.st();
        let (se, sk) = s.session_sketch_mut().unwrap();
        let cur = se.draw_cursor.clone().expect("Zeiger");
        se.kind_preview(sk, cur.at).expect("Vorschau")
    };
    rig.click(at);

    let sk = sketch(rig.st()).clone();
    let found = arcs(&sk);
    assert_eq!(found.len(), 1, "ein Bogen");
    let (m, start, end) = arc_parts(&sk, found[0]);
    assert!((m.distance(start) - 5.0).abs() < 1e-9);
    assert!(close(shown[0][0], start), "{:?} {start:?}", shown[0][0]);
    assert!(close(*shown[0].last().unwrap(), end));
    assert_eq!(count(&sk, |k| matches!(k, K::Tangent(..))), 2);
    assert_eq!(count(&sk, |k| matches!(k, K::Radius(..))), 1);
    assert_eq!(
        count(&sk, |k| matches!(k, K::Horizontal(..) | K::Vertical(..))),
        hv,
        "H und V bleiben"
    );
    assert_clean(&sk);
    assert_eq!(sk.clone().solve().dof, dof, "dof wie vorher");
    assert_eq!(undo_len(rig.st()), steps + 1, "ein Schritt");
}

// ---- Rundung ------------------------------------------------------------------------------------

#[test]
fn fillet_preview_equals_the_result_and_the_run_keeps_the_radius() {
    let mut s = rectangle();
    let dof = sketch(&s).clone().solve().dof;
    let steps = undo_len(&s);
    choose(&mut s, ToolKind::Fillet);
    click(&mut s, v(25.0, 0.0)).unwrap();
    assert_eq!(stage(&s), 1);
    assert!(s.sketch_session.as_ref().unwrap().input.fields.is_empty());
    click(&mut s, v(50.0, 15.0)).unwrap();
    assert_eq!(stage(&s), 2);
    type_in(&mut s, 0, "5");
    let at = v(40.0, 10.0);
    let shown = preview(&mut s, at).expect("Vorschau");
    click(&mut s, at).unwrap();

    let sk = sketch(&s);
    let first = arcs(sk)[0];
    let (m, start, end) = arc_parts(sk, first);
    assert!(close(m, v(45.0, 5.0)), "{m:?}");
    assert!(close(shown[0][0], start));
    assert!(close(*shown[0].last().unwrap(), end));
    for p in &shown[0] {
        assert!(
            (p.distance(m) - 5.0).abs() < 1e-9,
            "Vorschau liegt auf dem Bogen"
        );
    }
    assert_eq!(count(sk, |k| matches!(k, K::Radius(_, r) if *r == 5.0)), 1);
    assert_eq!(count(sk, |k| matches!(k, K::Tangent(..))), 2);
    assert_clean(sk);
    assert_eq!(sk.clone().solve().dof, dof);
    assert_eq!(undo_len(&s), steps + 1);
    assert_eq!(
        s.sketch_undo_label().as_deref(),
        Some(t("undo-fillet").as_str())
    );
    let se = s.sketch_session.as_ref().unwrap();
    assert_eq!(se.fillet_run, Some((first, 5.0)));
    assert_eq!(se.modify_edits.1, Some(Tool::Fillet));
    assert_eq!(stage(&s), 0);

    // Nächste Ecke: zwei Klicks, sofort fertig, ohne Feld, `Equal` statt eines zweiten Maßes.
    click(&mut s, v(25.0, 30.0)).unwrap();
    assert_eq!(
        arcs(sketch(&s)).len(),
        1,
        "der erste Klick legt noch nichts an"
    );
    click(&mut s, v(50.0, 18.0)).unwrap();
    let sk = sketch(&s);
    assert_eq!(arcs(sk).len(), 2);
    assert_eq!(count(sk, |k| matches!(k, K::Radius(..))), 1);
    assert_eq!(count(sk, |k| matches!(k, K::Equal(..))), 1);
    assert_clean(sk);
    assert_eq!(sk.clone().solve().dof, dof);
    assert_eq!(undo_len(&s), steps + 2);

    // Das eine Maß ändert beide.
    let dim = radius_dim(sketch(&s), first);
    let (se, sk) = s.session_sketch_mut().unwrap();
    se.set_dimension_value(sk, dim, 8.0).unwrap();
    for arc in arcs(sk) {
        assert!((sk.radius(arc).unwrap() - 8.0).abs() < 1e-9);
    }
    s.observe_sketch(false);

    // Im Lauf gilt der neue Radius: `Equal` nähme keinen anderen an.
    click(&mut s, v(25.0, 30.0)).unwrap();
    click(&mut s, v(0.0, 15.0)).unwrap();
    assert_eq!(arcs(sketch(&s)).len(), 3, "{:?}", message(&s));
    assert_clean(sketch(&s));

    // Esc beendet; eine neue Rundung fragt wieder nach dem Radius.
    let (se, sk) = s.session_sketch_mut().unwrap();
    se.cancel(sk);
    assert_eq!(se.tool, Tool::Select);
    assert_eq!(se.fillet_run, None);
    choose(&mut s, ToolKind::Fillet);
    click(&mut s, v(25.0, 0.0)).unwrap();
    click(&mut s, v(0.0, 15.0)).unwrap();
    assert_eq!(stage(&s), 2, "wieder mit Feld");
    assert_eq!(arcs(sketch(&s)).len(), 3);
}

#[test]
fn a_click_on_the_shared_point_is_enough_but_not_with_three_curves() {
    let mut s = rectangle();
    choose(&mut s, ToolKind::Fillet);
    click(&mut s, v(50.0, 0.0)).unwrap();
    assert_eq!(stage(&s), 2);
    type_in(&mut s, 0, "5");
    click(&mut s, v(40.0, 10.0)).unwrap();
    assert!(close(
        arc_parts(sketch(&s), arcs(sketch(&s))[0]).0,
        v(45.0, 5.0)
    ));
    // Im Lauf schließt auch der Klick auf eine Ecke sofort ab.
    click(&mut s, v(50.0, 30.0)).unwrap();
    assert_eq!(arcs(sketch(&s)).len(), 2);
    assert_eq!(stage(&s), 0);

    let mut s = sketch_with(|sk| {
        let p = sk.add_point(0.0, 0.0).unwrap();
        for to in [(30.0, 0.0), (0.0, 30.0), (-30.0, -30.0)] {
            let q = sk.add_point(to.0, to.1).unwrap();
            sk.add_line(p, q).unwrap();
        }
    });
    let before = snapshot(&s);
    let steps = undo_len(&s);
    choose(&mut s, ToolKind::Fillet);
    assert!(click(&mut s, v(0.0, 0.0)).is_err());
    assert_eq!(message(&s), Some(t("sketch-fillet-point-curves")));
    assert_eq!(stage(&s), 0);
    assert_eq!(snapshot(&s), before);
    assert_eq!(undo_len(&s), steps);
}

#[test]
fn farther_pointer_means_a_larger_arc_without_jumps() {
    let mut s = corner();
    let revision = sketch(&s).revision();
    choose(&mut s, ToolKind::Fillet);
    click(&mut s, v(20.0, 0.0)).unwrap();
    click(&mut s, v(0.0, 20.0)).unwrap();
    let mut last: Option<(DVec2, f64)> = None;
    for i in 0..500 {
        let dist = 0.1 + 0.05 * f64::from(i);
        // Der Zeiger pendelt um die Winkelhalbierende: nur der Abstand zählt.
        let angle = 0.8 + 0.6 * (f64::from(i) * 0.37).sin();
        let at = v(dist * angle.cos(), dist * angle.sin());
        let (se, sk) = s.session_sketch_mut().unwrap();
        se.update_cursor(sk, Some(at), PICK);
        let Some(ShapePlan::Arc(arc)) = se.kind_plan(sk, at) else {
            panic!("kein Plan bei {dist}");
        };
        assert!((arc.radius() - dist).abs() < 1e-9, "{dist}");
        assert!(arc.center.x > 0.0 && arc.center.y > 0.0, "{dist}");
        if let Some((m, r)) = last {
            assert!(arc.radius() > r, "größer");
            assert!(arc.center.distance(m) < 0.1, "stetig bei {dist}");
        }
        last = Some((arc.center, arc.radius()));
        let value = se.field_value(sk, F::Radius);
        assert!((value.unwrap() - dist).abs() < 1e-9);
    }
    assert_eq!(
        sketch(&s).revision(),
        revision,
        "die Vorschau ändert nichts"
    );
}

#[test]
fn ctrl_changes_nothing_about_picking_and_no_snap_touches_the_pointer() {
    let ctx = context();
    let mut s = corner();
    s.sketch_show_grid = true;
    s.sketch_snap_grid = true;
    choose(&mut s, ToolKind::Fillet);
    play(&ctx, &mut s, vec![vec![]]);
    raw_click_with(&ctx, &mut s, v(20.0, 0.0), Modifiers::CTRL);
    assert_eq!(stage(&s), 1, "Strg wählt wie sonst");
    raw_click_with(&ctx, &mut s, v(0.0, 20.0), Modifiers::CTRL);
    assert_eq!(stage(&s), 2, "Strg wählt wie sonst");
    let se = s.sketch_session.as_ref().unwrap();
    let spots = se.pick_points.clone();
    assert!(close(spots[0], v(20.0, 0.0)) || spots[0].distance(v(20.0, 0.0)) < 1.0);

    // Stufe 2 und Wählen: weder Raster noch Hilfslinie noch Automatik, auch ohne Strg.
    let (se, sk) = s.session_sketch_mut().unwrap();
    se.cursor_opts.suppressed = false;
    se.cursor_opts.grid_step = Some(10.0);
    let raw = v(13.37, 7.77);
    let cur = se.resolve_cursor(sk, raw, PICK);
    assert_eq!(cur.at, raw);
    assert_eq!(cur.snap, Snap::None);
    assert!(cur.autos.is_empty());
    assert!(cur.guides.x.is_none() && cur.guides.y.is_none());

    se.set_tool(sk, Tool::Fillet);
    se.cursor_opts.grid_step = Some(10.0);
    let raw = v(13.37, 0.2);
    let cur = se.resolve_cursor(sk, raw, PICK);
    assert!(matches!(cur.snap, Snap::Curve(_)), "{:?}", cur.snap);
    assert!(cur.autos.is_empty());
    assert!(cur.guides.x.is_none() && cur.guides.y.is_none());
    assert!((cur.at.x - 13.37).abs() < 1e-9, "kein Raster: {:?}", cur.at);
}

#[test]
fn the_click_spots_choose_the_quadrant_of_a_cross() {
    for (a, b, center) in [
        (v(10.0, 0.0), v(0.0, 10.0), v(5.0, 5.0)),
        (v(-10.0, 0.0), v(0.0, 10.0), v(-5.0, 5.0)),
        (v(-10.0, 0.0), v(0.0, -10.0), v(-5.0, -5.0)),
        (v(10.0, 0.0), v(0.0, -10.0), v(5.0, -5.0)),
    ] {
        let mut s = sketch_with(|sk| {
            line(sk, (-20.0, 0.0), (20.0, 0.0));
            line(sk, (0.0, -20.0), (0.0, 20.0));
        });
        let (h, w) = (line_through(sketch(&s), a), line_through(sketch(&s), b));
        choose(&mut s, ToolKind::Fillet);
        click(&mut s, a).unwrap();
        click(&mut s, b).unwrap();
        type_in(&mut s, 0, "5");
        click(&mut s, center).unwrap();
        let sk = sketch(&s);
        let (m, ..) = arc_parts(sk, arcs(sk)[0]);
        assert!(close(m, center), "{m:?} statt {center:?}");
        // Behalten wird das geklickte Stück bis zum fernen Ende.
        for (line, click_at) in [(h, a), (w, b)] {
            let (p, q) = line_ends(sk, line);
            let far = click_at * 2.0;
            assert!(close(p, far) || close(q, far), "{p:?} {q:?}");
        }
        assert_clean(sk);
    }
}

#[test]
fn line_and_circle_the_circle_stays_whole_and_the_line_is_shortened() {
    let mut circle = None;
    let mut s = sketch_with(|sk| {
        line(sk, (-30.0, 0.0), (30.0, 0.0));
        let c = sk.add_point(0.0, 0.0).unwrap();
        circle = Some(sk.add_circle(c, 10.0).unwrap());
    });
    let circle = circle.unwrap();
    let on_circle = v(10.0 * 0.5_f64.cos(), 10.0 * 0.5_f64.sin());
    let l = line_through(sketch(&s), v(20.0, 0.0));
    choose(&mut s, ToolKind::Fillet);
    click(&mut s, v(20.0, 0.0)).unwrap();
    click(&mut s, on_circle).unwrap();
    assert_eq!(stage(&s), 2);
    type_in(&mut s, 0, "3");
    click(&mut s, v(14.0, 5.0)).unwrap();
    let sk = sketch(&s);
    assert_eq!(arcs(sk).len(), 1, "{:?}", message(&s));
    assert!(matches!(
        sk.curve(circle).unwrap().kind,
        CurveKind::Circle { .. }
    ));
    assert!((sk.radius(circle).unwrap() - 10.0).abs() < 1e-9);
    let (p, q) = line_ends(sk, l);
    let (near, far) = if close(q, v(30.0, 0.0)) {
        (p, q)
    } else {
        (q, p)
    };
    assert!(close(far, v(30.0, 0.0)), "{far:?}");
    assert!(near.x > 10.0, "gekürzt: {near:?}");
    assert_clean(sk);
}

// ---- Unmögliche Eingaben (Review-Focus 1) --------------------------------------------------------

#[test]
fn impossible_picks_give_a_message_and_leave_stage_and_revision() {
    // Parallele Linien.
    let mut s = sketch_with(|sk| {
        line(sk, (0.0, 0.0), (40.0, 0.0));
        line(sk, (0.0, 10.0), (40.0, 10.0));
    });
    let revision = sketch(&s).revision();
    let steps = undo_len(&s);
    choose(&mut s, ToolKind::Fillet);
    click(&mut s, v(20.0, 0.0)).unwrap();
    assert!(click(&mut s, v(20.0, 10.0)).is_err());
    assert_eq!(message(&s), Some(t("sketch-fillet-no-corner")));
    assert_eq!(stage(&s), 1, "die Stufe bleibt");
    assert_eq!(preview(&mut s, v(25.0, 5.0)), None);

    // Dieselbe Kurve zweimal.
    assert!(click(&mut s, v(30.0, 0.0)).is_err());
    assert_eq!(message(&s), Some(t("sketch-fillet-no-corner")));
    assert_eq!(stage(&s), 1);

    // Klick ins Leere tut nichts und nimmt die Meldung.
    click(&mut s, v(20.0, 30.0)).unwrap();
    assert_eq!(stage(&s), 1);
    assert_eq!(message(&s), None);
    assert_eq!(sketch(&s).revision(), revision);
    assert_eq!(undo_len(&s), steps);
}

#[test]
fn a_radius_beyond_the_line_has_no_preview_and_a_message() {
    let mut s = corner();
    let before = snapshot(&s);
    let revision = sketch(&s).revision();
    let steps = undo_len(&s);
    choose(&mut s, ToolKind::Fillet);
    click(&mut s, v(20.0, 0.0)).unwrap();
    click(&mut s, v(0.0, 20.0)).unwrap();
    type_in(&mut s, 0, "100");
    assert_eq!(preview(&mut s, v(10.0, 10.0)), None);
    assert!(click(&mut s, v(10.0, 10.0)).is_err());
    assert_eq!(message(&s), Some(t("sketch-fillet-too-big")));
    assert_eq!(stage(&s), 2);
    assert_eq!(sketch(&s).revision(), revision);
    assert_eq!(snapshot(&s), before);
    assert_eq!(undo_len(&s), steps);
    // Ein passender Wert geht danach.
    type_in(&mut s, 0, "10");
    click(&mut s, v(10.0, 10.0)).unwrap();
    assert_eq!(arcs(sketch(&s)).len(), 1);
}

#[test]
fn a_radius_that_does_not_fit_an_ellipse_has_its_own_message() {
    let mut s = sketch_with(|sk| {
        line(sk, (-60.0, 0.0), (60.0, 0.0));
        let c = sk.add_point(0.0, 0.0).unwrap();
        sk.add_ellipse(c, DVec2::new(30.0, 0.0), 15.0).unwrap();
    });
    choose(&mut s, ToolKind::Fillet);
    click(&mut s, v(50.0, 0.0)).unwrap();
    let on = sketch(&s)
        .curves()
        .find(|(_, c)| matches!(c.kind, CurveKind::Ellipse { .. }))
        .map(|(id, _)| sketch(&s).nearest_on_curve(id, v(0.0, 15.0)).unwrap())
        .unwrap();
    click(&mut s, on).unwrap();
    type_in(&mut s, 0, "100000");
    assert!(click(&mut s, v(20.0, 20.0)).is_err());
    assert_eq!(message(&s), Some(t("sketch-fillet-kinds")));
    assert_eq!(stage(&s), 2);
}

// ---- Abbruch und Rückgängig (Review-Focus 3) -----------------------------------------------------

#[test]
fn esc_in_each_stage_over_raw_events_leaves_nothing() {
    for clicks in [1, 2] {
        // Ein eigener Kontext: Sonst zählte der erste Klick der zweiten Runde als Doppelklick.
        let ctx = context();
        let mut s = corner();
        let before = snapshot(&s);
        let revision = sketch(&s).revision();
        let steps = undo_len(&s);
        choose(&mut s, ToolKind::Fillet);
        play(&ctx, &mut s, vec![vec![]]);
        raw_click(&ctx, &mut s, v(20.0, 0.0));
        if clicks == 2 {
            raw_click(&ctx, &mut s, v(0.0, 20.0));
        }
        assert_eq!(stage(&s), clicks);
        play(&ctx, &mut s, vec![vec![esc()], vec![]]);
        let se = s.sketch_session.as_ref().unwrap();
        assert_eq!(se.tool, Tool::Select);
        assert!(se.pending_curves.is_empty() && se.pick_points.is_empty());
        assert_eq!(snapshot(&s), before);
        assert_eq!(sketch(&s).revision(), revision);
        assert_eq!(undo_len(&s), steps, "kein Rückgängig-Schritt");
    }
}

#[test]
fn one_undo_takes_everything_and_redo_is_bit_equal() {
    let mut s = rectangle();
    let before = snapshot(&s);
    choose(&mut s, ToolKind::Fillet);
    click(&mut s, v(25.0, 0.0)).unwrap();
    click(&mut s, v(50.0, 15.0)).unwrap();
    type_in(&mut s, 0, "5");
    click(&mut s, v(40.0, 10.0)).unwrap();
    let after = snapshot(&s);
    assert_ne!(after, before);

    assert!(s.undo_sketch());
    assert_eq!(snapshot(&s), before, "ein Rückgängig nimmt alles");
    assert_eq!(s.sketch_session.as_ref().unwrap().fillet_run, None);
    assert!(s.redo_sketch());
    assert_eq!(snapshot(&s), after, "Wiederholen bitgleich");
    assert_eq!(s.sketch_session.as_ref().unwrap().fillet_run, None);
}

#[test]
fn a_deleted_curve_falls_out_of_the_picks_and_of_the_run() {
    let mut s = rectangle();
    let bottom = line_through(sketch(&s), v(25.0, 0.0));
    choose(&mut s, ToolKind::Fillet);
    click(&mut s, v(25.0, 0.0)).unwrap();
    click(&mut s, v(50.0, 15.0)).unwrap();
    assert_eq!(stage(&s), 2);
    let (se, sk) = s.session_sketch_mut().unwrap();
    se.selection = vec![Item::Curve(bottom)];
    se.delete_selection(sk);
    assert_eq!(se.pending_curves.len(), 1);
    assert_eq!(se.pick_points.len(), 1);
    assert!(close(se.pick_points[0], v(50.0, 15.0)));
    assert_eq!(se.kind_stage(), 1);
    assert_eq!(se.kind_preview(sk, v(1.0, 1.0)), None);

    let mut s = rectangle();
    choose(&mut s, ToolKind::Fillet);
    click(&mut s, v(25.0, 0.0)).unwrap();
    click(&mut s, v(50.0, 15.0)).unwrap();
    type_in(&mut s, 0, "5");
    click(&mut s, v(40.0, 10.0)).unwrap();
    let arc = arcs(sketch(&s))[0];
    let (se, sk) = s.session_sketch_mut().unwrap();
    assert!(se.fillet_run.is_some());
    se.selection = vec![Item::Curve(arc)];
    se.delete_selection(sk);
    assert_eq!(
        se.fillet_run, None,
        "der Lauf vergisst die gelöschte Rundung"
    );
}

// ---- Zoll und Formeln (Review-Focus 5) -----------------------------------------------------------

#[test]
fn one_inch_is_stored_as_inches_and_gives_25_4() {
    with_unit(DisplayUnit::Zoll, || {
        let mut s = sketch_with(|sk| {
            let a = sk.add_point(100.0, 0.0).unwrap();
            let p = sk.add_point(0.0, 0.0).unwrap();
            let b = sk.add_point(0.0, 100.0).unwrap();
            sk.add_line(a, p).unwrap();
            sk.add_line(p, b).unwrap();
        });
        choose(&mut s, ToolKind::Fillet);
        click(&mut s, v(50.0, 0.0)).unwrap();
        click(&mut s, v(0.0, 50.0)).unwrap();
        type_in(&mut s, 0, "1");
        let shown = preview(&mut s, v(30.0, 30.0)).expect("Vorschau");
        click(&mut s, v(30.0, 30.0)).unwrap();
        let (_, sf) = s.session_draft_mut().unwrap();
        let arc = arcs(&sf.sketch)[0];
        let (m, ..) = arc_parts(&sf.sketch, arc);
        assert!(close(m, v(25.4, 25.4)), "{m:?}");
        assert!((shown[0][0].distance(m) - 25.4).abs() < 1e-9);
        let dim = radius_dim(&sf.sketch, arc);
        assert_eq!(sf.formulas.get(&dim).map(String::as_str), Some("1 in"));
    });
}

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

#[test]
fn a_formula_stays_on_the_radius_and_the_arc_follows_the_parameter() {
    let mut s = AppState::new();
    let breite = param(&mut s, "Breite", "20");
    let i = s
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    s.open_sketch(i).unwrap();
    {
        let sk = s.session_sketch_mut().unwrap().1;
        let a = sk.add_point(40.0, 0.0).unwrap();
        let p = sk.add_point(0.0, 0.0).unwrap();
        let b = sk.add_point(0.0, 40.0).unwrap();
        sk.add_line(a, p).unwrap();
        sk.add_line(p, b).unwrap();
        sk.solve();
    }
    s.observe_sketch(false);
    choose(&mut s, ToolKind::Fillet);
    click(&mut s, v(20.0, 0.0)).unwrap();
    click(&mut s, v(0.0, 20.0)).unwrap();
    type_in(&mut s, 0, "Breite / 2");
    click(&mut s, v(5.0, 5.0)).unwrap();
    let (_, sf) = s.session_draft_mut().unwrap();
    let arc = arcs(&sf.sketch)[0];
    let CurveKind::Arc { center, .. } = sf.sketch.curve(arc).unwrap().kind else {
        panic!()
    };
    assert!(close(sf.sketch.point_xy(center).unwrap(), v(10.0, 10.0)));
    let dim = radius_dim(&sf.sketch, arc);
    assert_eq!(
        sf.formulas.get(&dim).map(String::as_str),
        Some("Breite / 2")
    );

    s.close_sketch();
    let p = s.document.parameter(breite).unwrap().clone();
    s.edit_parameter(
        breite,
        ParamEdit {
            name: p.name,
            formula: "30".into(),
            unit: p.unit,
            comment: p.comment,
        },
    )
    .unwrap();
    let sk = s.document.sketch_of(i).unwrap();
    let m = sk.point_xy(center).unwrap();
    assert!(close(m, v(15.0, 15.0)), "{m:?}");
}

#[test]
fn the_radius_field_runs_the_guard_and_refuses_the_limit_of_the_rectangle() {
    let mut s = rectangle();
    {
        let sk = s.session_sketch_mut().unwrap().1;
        let (right, bottom) = (
            line_through(sk, v(50.0, 15.0)),
            line_through(sk, v(25.0, 0.0)),
        );
        let ends = |sk: &Sketch, c: CurveId| match sk.curve(c).unwrap().kind {
            CurveKind::Line { a, b } => (a, b),
            _ => panic!(),
        };
        let (a, b) = ends(sk, right);
        sk.add_constraint(K::Distance(a, b, 30.0)).unwrap();
        let (a, b) = ends(sk, bottom);
        sk.add_constraint(K::Distance(a, b, 50.0)).unwrap();
        sk.solve();
    }
    s.observe_sketch(false);
    choose(&mut s, ToolKind::Fillet);
    click(&mut s, v(25.0, 0.0)).unwrap();
    click(&mut s, v(50.0, 15.0)).unwrap();
    type_in(&mut s, 0, "5");
    click(&mut s, v(40.0, 10.0)).unwrap();
    click(&mut s, v(25.0, 30.0)).unwrap();
    click(&mut s, v(50.0, 18.0)).unwrap();
    assert_eq!(arcs(sketch(&s)).len(), 2, "{:?}", message(&s));
    let dim = radius_dim(sketch(&s), arcs(sketch(&s))[0]);
    let scope = s.document.param_scope();

    let (se, sf) = s.session_draft_mut().unwrap();
    se.set_dimension_input(sf, dim, "14.9", &scope).unwrap();
    for arc in arcs(&sf.sketch) {
        assert!((sf.sketch.radius(arc).unwrap() - 14.9).abs() < 1e-9);
    }
    assert_clean(&sf.sketch);
    let before: Vec<u64> = sf.sketch.variables().iter().map(|x| x.to_bits()).collect();
    assert!(se.set_dimension_input(sf, dim, "16", &scope).is_err());
    let after: Vec<u64> = sf.sketch.variables().iter().map(|x| x.to_bits()).collect();
    assert_eq!(before, after, "abgelehnt, Skizze unverändert");
}

// ---- Fase ----------------------------------------------------------------------------------------

/// Rechteck-Ecke rechts unten: untere und rechte Kante wählen; `flip`: rechte zuerst.
fn chamfer_picks(s: &mut AppState, kind: ToolKind, flip: bool) {
    choose(s, kind);
    let (a, b) = if flip {
        (v(50.0, 15.0), v(25.0, 0.0))
    } else {
        (v(25.0, 0.0), v(50.0, 15.0))
    };
    click(s, a).unwrap();
    click(s, b).unwrap();
    assert_eq!(stage(s), 2);
}

/// Die eine Linie, die weder Vorlage noch Konstruktion ist: von `from` nach `to`.
fn has_line(sk: &Sketch, from: DVec2, to: DVec2) -> bool {
    sk.curves().any(|(id, c)| {
        if c.construction || !matches!(c.kind, CurveKind::Line { .. }) {
            return false;
        }
        let (p, q) = line_ends(sk, id);
        (close(p, from) && close(q, to)) || (close(p, to) && close(q, from))
    })
}

fn assert_corner_held(sk: &Sketch) {
    let on_corner = sk
        .points()
        .map(|(p, _)| p)
        .find(|&p| sk.point_xy(p).unwrap().distance(v(50.0, 0.0)) < 1e-9)
        .expect("Ecke bleibt");
    assert_eq!(
        count(
            sk,
            |k| matches!(k, K::PointOnCurve(p, _) if *p == on_corner)
        ),
        2
    );
}

#[test]
fn chamfer_equal_distance_has_its_dimension_and_keeps_the_corner() {
    let mut s = rectangle();
    let dof = sketch(&s).clone().solve().dof;
    let steps = undo_len(&s);
    chamfer_picks(&mut s, ToolKind::ChamferEqual, false);
    let fields: Vec<F> = s
        .sketch_session
        .as_ref()
        .unwrap()
        .input
        .fields
        .iter()
        .map(|f| f.kind)
        .collect();
    assert_eq!(fields, [F::Distance]);
    type_in(&mut s, 0, "5");
    let at = v(40.0, 10.0);
    let shown = preview(&mut s, at).expect("Vorschau");
    click(&mut s, at).unwrap();
    let sk = sketch(&s);
    assert!(has_line(sk, v(45.0, 0.0), v(50.0, 5.0)), "die Fase");
    assert!(
        close(shown[0][0], v(45.0, 0.0)) && close(shown[0][1], v(50.0, 5.0)),
        "Vorschau = Ergebnis"
    );
    assert_corner_held(sk);
    assert_eq!(
        count(sk, |k| matches!(k, K::Distance(_, _, d) if *d == 5.0)),
        1
    );
    assert_eq!(count(sk, |k| matches!(k, K::Equal(..))), 1);
    assert_clean(sk);
    assert_eq!(sk.clone().solve().dof, dof);
    assert_eq!(undo_len(&s), steps + 1);
    assert_eq!(
        s.sketch_undo_label().as_deref(),
        Some(t("undo-chamfer").as_str())
    );
    assert_eq!(
        s.sketch_session.as_ref().unwrap().modify_edits.1,
        Some(Tool::Fillet)
    );
    assert_eq!(s.sketch_session.as_ref().unwrap().fillet_run, None);
}

#[test]
fn chamfer_two_distances_uses_the_first_for_the_first_line() {
    let mut s = rectangle();
    let dof = sketch(&s).clone().solve().dof;
    chamfer_picks(&mut s, ToolKind::ChamferTwoDistances, false);
    type_in(&mut s, 0, "5");
    type_in(&mut s, 1, "8");
    let shown = preview(&mut s, v(40.0, 10.0)).expect("Vorschau");
    click(&mut s, v(40.0, 10.0)).unwrap();
    let sk = sketch(&s);
    assert!(has_line(sk, v(45.0, 0.0), v(50.0, 8.0)));
    assert!(close(shown[0][0], v(45.0, 0.0)) && close(shown[0][1], v(50.0, 8.0)));
    assert_corner_held(sk);
    assert_eq!(count(sk, |k| matches!(k, K::Distance(..))), 2);
    assert_clean(sk);
    assert_eq!(sk.clone().solve().dof, dof);
}

#[test]
fn tab_switches_between_distance_and_angle() {
    let mut s = rectangle();
    chamfer_picks(&mut s, ToolKind::ChamferDistanceAngle, false);
    let se = s.sketch_session.as_mut().unwrap();
    let fields: Vec<F> = se.input.fields.iter().map(|f| f.kind).collect();
    assert_eq!(fields, [F::Distance, F::Angle]);
    assert_eq!(se.input.active, 0);
    se.next_field(false);
    assert_eq!(se.input.active, 1);
    se.next_field(false);
    assert_eq!(se.input.active, 0);
    // Ohne getippten Winkel zeigt das Feld die Vorgabe, die Länge den Zeiger.
    let (se, sk) = s.session_sketch_mut().unwrap();
    se.update_cursor(sk, Some(v(40.0, 0.0)), PICK);
    assert!((se.field_value(sk, F::Angle).unwrap() - 45_f64.to_radians()).abs() < 1e-12);
    assert!((se.field_value(sk, F::Distance).unwrap() - 10.0).abs() < 1e-9);
}

#[test]
fn chamfer_distance_angle_shows_the_typed_angle_and_never_folds_when_changed() {
    for flip in [false, true] {
        let mut s = rectangle();
        // Rechteck ganz bestimmt: Ohne Maße teilte der Löser den Schritt auf die Rechtecklinien auf.
        {
            let sk = s.session_sketch_mut().unwrap().1;
            for at in [v(50.0, 15.0), v(25.0, 0.0)] {
                let CurveKind::Line { a, b } = sk.curve(line_through(sk, at)).unwrap().kind else {
                    panic!();
                };
                let length = sk.point_xy(a).unwrap().distance(sk.point_xy(b).unwrap());
                sk.add_constraint(K::Distance(a, b, length)).unwrap();
            }
            sk.solve();
        }
        s.observe_sketch(false);
        chamfer_picks(&mut s, ToolKind::ChamferDistanceAngle, flip);
        type_in(&mut s, 0, "6");
        type_in(&mut s, 1, "45");
        let shown = preview(&mut s, v(40.0, 10.0)).expect("Vorschau");
        click(&mut s, v(40.0, 10.0)).unwrap();
        let sk = sketch(&s);
        let t1 = if flip { v(50.0, 6.0) } else { v(44.0, 0.0) };
        let t2 = if flip { v(44.0, 0.0) } else { v(50.0, 6.0) };
        assert!(has_line(sk, t1, t2), "{flip}: {:?}", message(&s));
        assert!(close(shown[0][0], t1) && close(shown[0][1], t2));
        assert_corner_held(sk);
        assert_clean(sk);
        let (dim, value) = sk
            .constraints()
            .find_map(|(id, c)| match c.kind {
                K::Angle(_, _, x) => Some((id, x)),
                _ => None,
            })
            .expect("Winkelmaß");
        assert!(
            (value - 45_f64.to_radians()).abs() < 1e-9,
            "{flip}: Das Maß zeigt den getippten Winkel, nicht {}",
            value.to_degrees()
        );

        // Winkel über eine dichte Folge ändern: Ecke bleibt, `T1` bleibt, `T2` wandert, kein Umklappen.
        let mut last = t2;
        for i in 0..150 {
            let deg = 45.0 - 0.1 * f64::from(i);
            let (se, sk) = s.session_sketch_mut().unwrap();
            se.set_dimension_value(sk, dim, deg.to_radians()).unwrap();
            let expected = 6.0 * deg.to_radians().tan();
            let now = if flip {
                v(50.0 - expected, 0.0)
            } else {
                v(50.0, expected)
            };
            assert!(has_line(sk, t1, now), "{flip}: {deg}°");
            assert!(now.distance(last) < 0.1, "stetig");
            last = now;
            assert_corner_held(sk);
        }
        let sk = sketch(&s);
        assert_clean(sk);
    }
}

#[test]
fn a_typed_angle_formula_stays_on_the_dimension() {
    for flip in [false, true] {
        let mut s = AppState::new();
        param(&mut s, "Breite", "10");
        let i = s
            .add_sketch(SketchPlane::Standard(StandardPlane::XY))
            .unwrap();
        s.open_sketch(i).unwrap();
        set_tool(&mut s, Tool::Rectangle);
        click(&mut s, v(0.0, 0.0)).unwrap();
        click(&mut s, v(50.0, 30.0)).unwrap();
        set_tool(&mut s, Tool::Select);
        chamfer_picks(&mut s, ToolKind::ChamferDistanceAngle, flip);
        type_in(&mut s, 0, "Breite / 2");
        type_in(&mut s, 1, "30");
        click(&mut s, v(40.0, 10.0)).unwrap();
        let (_, sf) = s.session_draft_mut().unwrap();
        let (dist, angle) =
            sf.sketch
                .constraints()
                .fold((None, None), |acc, (id, c)| match c.kind {
                    K::Distance(..) => (Some(id), acc.1),
                    K::Angle(..) => (acc.0, Some(id)),
                    _ => acc,
                });
        assert_eq!(
            sf.formulas.get(&dist.unwrap()).map(String::as_str),
            Some("Breite / 2")
        );
        let value = sf.sketch.constraint(angle.unwrap()).unwrap().kind.clone();
        assert!(matches!(value, K::Angle(_, _, x) if (x - 30_f64.to_radians()).abs() < 1e-9));
    }
}

#[test]
fn an_arc_is_no_chamfer_curve_and_a_long_distance_is_refused() {
    let mut s = sketch_with(|sk| {
        let c = sk.add_point(0.0, 0.0).unwrap();
        let a = sk.add_point(10.0, 0.0).unwrap();
        let b = sk.add_point(0.0, 10.0).unwrap();
        sk.add_arc(c, a, b).unwrap();
        let q = sk.add_point(40.0, 0.0).unwrap();
        sk.add_line(a, q).unwrap();
    });
    let revision = sketch(&s).revision();
    choose(&mut s, ToolKind::ChamferEqual);
    let on_arc = v(10.0 * 0.8_f64.cos(), 10.0 * 0.8_f64.sin());
    assert!(click(&mut s, on_arc).is_err());
    assert_eq!(message(&s), Some(t("sketch-chamfer-lines")));
    assert_eq!(stage(&s), 0);
    // Der gemeinsame Punkt von Bogen und Linie wählt auch einen Bogen.
    assert!(click(&mut s, v(10.0, 0.0)).is_err());
    assert_eq!(message(&s), Some(t("sketch-chamfer-lines")));
    assert_eq!(stage(&s), 0);
    assert_eq!(sketch(&s).revision(), revision);

    let mut s = corner();
    let revision = sketch(&s).revision();
    let steps = undo_len(&s);
    choose(&mut s, ToolKind::ChamferEqual);
    click(&mut s, v(20.0, 0.0)).unwrap();
    click(&mut s, v(0.0, 20.0)).unwrap();
    type_in(&mut s, 0, "41");
    assert_eq!(preview(&mut s, v(5.0, 5.0)), None);
    assert!(click(&mut s, v(5.0, 5.0)).is_err());
    assert_eq!(message(&s), Some(t("sketch-chamfer-too-big")));
    assert_eq!(stage(&s), 2);
    assert_eq!(sketch(&s).revision(), revision);
    assert_eq!(undo_len(&s), steps);
}

#[test]
fn a_fixed_near_end_refuses_the_chamfer_with_its_own_message() {
    let mut s = sketch_with(|sk| {
        let a = sk.add_point(40.0, 0.0).unwrap();
        let p = sk.add_point(0.0, 0.0).unwrap();
        let b = sk.add_point(0.0, 40.0).unwrap();
        sk.add_line(a, p).unwrap();
        sk.add_line(p, b).unwrap();
        sk.add_constraint(K::Fixed(p, 0.0, 0.0)).unwrap();
    });
    let revision = sketch(&s).revision();
    choose(&mut s, ToolKind::ChamferEqual);
    click(&mut s, v(20.0, 0.0)).unwrap();
    assert!(click(&mut s, v(0.0, 20.0)).is_err());
    assert_eq!(message(&s), Some(t("sketch-chamfer-fixed")));
    assert_eq!(stage(&s), 1);
    assert_eq!(sketch(&s).revision(), revision);
}

#[test]
fn a_chamfer_over_raw_events_needs_two_clicks_and_a_third_at_the_pointer() {
    let ctx = context();
    let mut s = corner();
    let steps = undo_len(&s);
    choose(&mut s, ToolKind::ChamferEqual);
    play(&ctx, &mut s, vec![vec![]]);
    raw_click(&ctx, &mut s, v(20.0, 0.0));
    raw_click(&ctx, &mut s, v(0.0, 20.0));
    assert_eq!(stage(&s), 2);
    // Der Abstand kommt vom Zeiger: Abstand zur Ecke.
    let at = v(6.0, 8.0);
    let p = screen(&s, at);
    play(&ctx, &mut s, vec![vec![Event::PointerMoved(p)], vec![]]);
    let raw = s
        .sketch_session
        .as_ref()
        .unwrap()
        .draw_cursor
        .as_ref()
        .unwrap()
        .raw;
    let d = raw.length();
    let shown = {
        let (se, sk) = s.session_sketch_mut().unwrap();
        se.kind_preview(sk, raw).expect("Vorschau")
    };
    raw_click(&ctx, &mut s, at);
    let sk = sketch(&s);
    assert!(
        has_line(sk, shown[0][0], shown[0][1]),
        "Vorschau = Ergebnis"
    );
    assert!((shown[0][0].length() - d).abs() < 1e-3);
    assert_clean(sk);
    assert_eq!(undo_len(&s), steps + 1);
}

// ---- Fase über die Ecke hinaus (Korrekturrunde 1) --------------------------------------------------

/// Rechteck für die Fase; `dims`: mit Breite und Höhe ganz bestimmt, sonst frei bis auf H und V.
fn chamfer_rectangle(dims: bool) -> AppState {
    let mut s = rectangle();
    if dims {
        let sk = s.session_sketch_mut().unwrap().1;
        for at in [v(50.0, 15.0), v(25.0, 0.0)] {
            let CurveKind::Line { a, b } = sk.curve(line_through(sk, at)).unwrap().kind else {
                panic!();
            };
            let length = sk.point_xy(a).unwrap().distance(sk.point_xy(b).unwrap());
            sk.add_constraint(K::Distance(a, b, length)).unwrap();
        }
        sk.solve();
        s.observe_sketch(false);
    }
    s
}

/// Der gehaltene Eckpunkt und die beiden Linien, auf denen er liegt.
fn held_corner(sk: &Sketch) -> (planora_sketch::model::PointId, CurveId, CurveId) {
    let on: Vec<_> = sk
        .constraints()
        .filter_map(|(_, c)| match c.kind {
            K::PointOnCurve(p, l) => Some((p, l)),
            _ => None,
        })
        .collect();
    assert_eq!(on.len(), 2, "{on:?}");
    assert_eq!(on[0].0, on[1].0);
    (on[0].0, on[0].1, on[1].1)
}

/// Liegen beide Enden der Fase auf der behaltenen Seite der Ecke? Das behaltene Stück ist je Linie
/// das Ende, das auch die Fase trägt, bis zum anderen Ende.
fn ends_on_kept_side(sk: &Sketch) -> bool {
    let (p, l1, l2) = held_corner(sk);
    let corner = sk.point_xy(p).unwrap();
    let ends = |c: CurveId| match sk.curve(c).unwrap().kind {
        CurveKind::Line { a, b } => [a, b],
        _ => panic!(),
    };
    let chamfer = sk
        .curves()
        .filter(|(id, d)| !d.construction && *id != l1 && *id != l2)
        .find_map(|(_, d)| match d.kind {
            CurveKind::Line { a, b }
                if (ends(l1).contains(&a) && ends(l2).contains(&b))
                    || (ends(l1).contains(&b) && ends(l2).contains(&a)) =>
            {
                Some([a, b])
            }
            _ => None,
        })
        .expect("Fase");
    [l1, l2].iter().all(|&l| {
        let [a, b] = ends(l);
        let (t, far) = if chamfer.contains(&a) { (a, b) } else { (b, a) };
        let (t, far) = (sk.point_xy(t).unwrap(), sk.point_xy(far).unwrap());
        let along = (t - corner).dot((far - corner).normalize());
        along > 0.0 && along < corner.distance(far)
    })
}

fn state_bits(s: &AppState) -> (u64, String) {
    (sketch(s).revision(), snapshot(s))
}

/// Setzt das Maß. Geht es, liegen die Enden noch richtig; sonst ist die Skizze bitgleich.
fn step_dimension(s: &mut AppState, dim: ConstraintId, value: f64) -> bool {
    let before = state_bits(s);
    let (se, sk) = s.session_sketch_mut().unwrap();
    let accepted = se.set_dimension_value(sk, dim, value).is_ok();
    if accepted {
        assert!(ends_on_kept_side(sketch(s)), "{value}: Fase über die Ecke");
    } else {
        assert_eq!(state_bits(s), before, "{value}: abgelehnt, also bitgleich");
    }
    accepted
}

fn angle_dim(sk: &Sketch) -> ConstraintId {
    sk.constraints()
        .find(|(_, c)| matches!(c.kind, K::Angle(..)))
        .expect("Winkelmaß")
        .0
}

/// Die Abstandsmaße der Fase: Sie zählen von der Ecke und sind kürzer als die Rechteckmaße, die
/// ebenfalls an der Ecke hängen.
fn corner_dims(sk: &Sketch) -> Vec<ConstraintId> {
    let (p, ..) = held_corner(sk);
    sk.constraints()
        .filter(|(_, c)| matches!(c.kind, K::Distance(a, b, d) if (a == p || b == p) && d < 20.0))
        .map(|(id, _)| id)
        .collect()
}

/// Fase rechts unten mit den getippten Werten.
fn chamfered(dims: bool, kind: ToolKind, flip: bool, typed: &[&str]) -> AppState {
    let mut s = chamfer_rectangle(dims);
    chamfer_picks(&mut s, kind, flip);
    for (i, text) in typed.iter().enumerate() {
        type_in(&mut s, i, text);
    }
    click(&mut s, v(40.0, 10.0)).unwrap();
    assert!(ends_on_kept_side(sketch(&s)));
    s
}

#[test]
fn the_chamfer_angle_walks_up_to_the_limit_and_is_refused_past_the_corner() {
    for dims in [true, false] {
        for flip in [false, true] {
            let mut s = chamfered(dims, ToolKind::ChamferDistanceAngle, flip, &["6", "45"]);
            let dim = angle_dim(sketch(&s));
            let mut accepted_below = 0;
            for i in 0..=268 {
                let deg = 45.5 + 0.5 * f64::from(i);
                let accepted = step_dimension(&mut s, dim, deg.to_radians());
                if deg > 90.0 {
                    assert!(!accepted, "{dims} {flip}: {deg}° ist hinter der Ecke");
                } else if accepted {
                    accepted_below += 1;
                }
            }
            assert!(accepted_below >= 60, "{dims} {flip}: {accepted_below}");
            // Der Sprung ist genauso wenig erlaubt, mit der Meldung dazu.
            let mut s = chamfered(dims, ToolKind::ChamferDistanceAngle, flip, &["6", "45"]);
            let dim = angle_dim(sketch(&s));
            assert!(
                !step_dimension(&mut s, dim, 170_f64.to_radians()),
                "{dims} {flip}"
            );
            assert_eq!(message(&s), Some(t("sketch-dimension-folds")));
            // Und was passt, geht danach noch.
            assert!(
                step_dimension(&mut s, dim, 30_f64.to_radians()),
                "{dims} {flip}"
            );
        }
    }
}

#[test]
fn chamfer_distances_walk_up_to_the_limit_and_never_cross_the_corner() {
    let kinds: [(ToolKind, &[&str]); 3] = [
        (ToolKind::ChamferEqual, &["5"]),
        (ToolKind::ChamferTwoDistances, &["5", "7"]),
        (ToolKind::ChamferDistanceAngle, &["5", "45"]),
    ];
    for dims in [true, false] {
        for flip in [false, true] {
            for (kind, typed) in kinds {
                let probe = chamfered(dims, kind, flip, typed);
                for which in 0..corner_dims(sketch(&probe)).len() {
                    let mut s = chamfered(dims, kind, flip, typed);
                    let dim = corner_dims(sketch(&s))[which];
                    let start = sketch(&s)
                        .constraint(dim)
                        .unwrap()
                        .kind
                        .length_value()
                        .unwrap();
                    let (mut refused_at, mut refused) = (None, 0);
                    for i in 1..=320 {
                        let d = start + 0.25 * f64::from(i);
                        if !step_dimension(&mut s, dim, d) {
                            refused_at.get_or_insert(d);
                            // Ablehnen ist teuer (Schritte des Wächters); ein paar reichen.
                            refused += 1;
                            if refused == 4 {
                                break;
                            }
                        }
                    }
                    if dims {
                        // Länger als die Linie geht im ganz bestimmten Rechteck nicht.
                        let limit = refused_at.expect("Grenze");
                        assert!(limit <= 50.25, "{kind:?} {flip} {which}: {limit}");
                    }
                    // Der große Sprung und zurück.
                    let mut s = chamfered(dims, kind, flip, typed);
                    let dim = corner_dims(sketch(&s))[which];
                    step_dimension(&mut s, dim, 200.0);
                    step_dimension(&mut s, dim, 2.0);
                }
            }
        }
    }
}

#[test]
fn the_old_corner_of_a_chamfer_counts_only_normal_curves() {
    let mut s = rectangle();
    chamfer_picks(&mut s, ToolKind::ChamferEqual, false);
    type_in(&mut s, 0, "5");
    click(&mut s, v(40.0, 10.0)).unwrap();
    let before = state_bits(&s);
    set_tool(&mut s, Tool::Select);
    choose(&mut s, ToolKind::Fillet);
    assert!(click(&mut s, v(50.0, 0.0)).is_err());
    assert_eq!(message(&s), Some(t("sketch-fillet-point-curves")));
    assert_eq!(stage(&s), 0);
    assert_eq!(state_bits(&s), before);
}

#[test]
fn at_stage_one_the_shared_point_picks_the_other_curve() {
    for kind in [ToolKind::Fillet, ToolKind::ChamferEqual] {
        let mut s = corner();
        choose(&mut s, kind);
        click(&mut s, v(20.0, 0.0)).unwrap();
        let first = s.sketch_session.as_ref().unwrap().pending_curves[0];
        click(&mut s, v(0.0, 0.0)).unwrap();
        assert_eq!(stage(&s), 2, "{kind:?}: {:?}", message(&s));
        let se = s.sketch_session.as_ref().unwrap();
        assert_eq!(se.pending_curves[0], first);
        assert_ne!(se.pending_curves[1], first);
        type_in(&mut s, 0, "5");
        click(&mut s, v(10.0, 10.0)).unwrap();
        assert_clean(sketch(&s));
    }

    // Enden dort mehrere andere Kurven, ist nicht klar, welche gemeint ist.
    let mut s = sketch_with(|sk| {
        let p = sk.add_point(0.0, 0.0).unwrap();
        for to in [(30.0, 0.0), (0.0, 30.0), (-30.0, -30.0)] {
            let q = sk.add_point(to.0, to.1).unwrap();
            sk.add_line(p, q).unwrap();
        }
    });
    choose(&mut s, ToolKind::Fillet);
    click(&mut s, v(20.0, 0.0)).unwrap();
    assert!(click(&mut s, v(0.0, 0.0)).is_err());
    assert_eq!(message(&s), Some(t("sketch-fillet-point-curves")));
    assert_eq!(stage(&s), 1);
}
