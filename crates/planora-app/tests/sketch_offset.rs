//! Skizzen 4, Task 8: Versatz in der App. Klicks über die Sitzung (`commit_input`) und eine Fahrt
//! über Rohereignisse bei 1920×1080, Muster `sketch_fillet.rs`.

mod common;

use eframe::App as _;
use egui::{Event, Key, Modifiers, PointerButton, Pos2, Rect, Vec2, pos2};
use planora_app::app::PlanoraApp;
use planora_app::sketch_input::FieldKind as F;
use planora_app::sketch_state::{Snap, Tool};
use planora_app::state::{AppState, StandardPlane};
use planora_app::workspace::Workspace;
use planora_app::{fonts, sketch_view, theme};
use planora_feature::{ParamEdit, ParamId, ParamUnit, SketchPlane};
use planora_i18n::{DisplayUnit, t, with_unit};
use planora_math::DVec2;
use planora_project::config::Config;
use planora_project::library::Library;
use planora_sketch::constraint::{ConstraintId, ConstraintKind as K};
use planora_sketch::model::{CurveId, CurveKind, PointId, Sketch};
use planora_sketch::offset::OffsetProblem;
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

/// Geschlossener Linienzug aus freien Linien mit gemeinsamen Punkten.
fn polygon(sk: &mut Sketch, corners: &[(f64, f64)]) -> Vec<CurveId> {
    let points: Vec<PointId> = corners
        .iter()
        .map(|c| sk.add_point(c.0, c.1).unwrap())
        .collect();
    (0..points.len())
        .map(|i| {
            sk.add_line(points[i], points[(i + 1) % points.len()])
                .unwrap()
        })
        .collect()
}

const SQUARE: [(f64, f64); 4] = [(0.0, 0.0), (40.0, 0.0), (40.0, 40.0), (0.0, 40.0)];

/// Quadrat (0,0)–(40,40) aus freien Linien.
fn free_square() -> AppState {
    sketch_with(|sk| {
        polygon(sk, &SQUARE);
    })
}

/// Quadrat (0,0)–(40,40) mit dem Rechteckwerkzeug, also samt H und V.
fn tool_square() -> AppState {
    let mut s = sketch_with(|_| {});
    set_tool(&mut s, Tool::Rectangle);
    click(&mut s, v(0.0, 0.0)).unwrap();
    click(&mut s, v(40.0, 40.0)).unwrap();
    set_tool(&mut s, Tool::Select);
    s.observe_sketch(false);
    s
}

fn open_line() -> AppState {
    sketch_with(|sk| {
        let a = sk.add_point(0.0, 0.0).unwrap();
        let b = sk.add_point(40.0, 0.0).unwrap();
        sk.add_line(a, b).unwrap();
    })
}

fn set_tool(s: &mut AppState, tool: Tool) {
    let (se, sk) = s.session_sketch_mut().unwrap();
    se.set_tool(sk, tool);
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

fn message(s: &AppState) -> Option<String> {
    s.sketch_session.as_ref().unwrap().message_text()
}

fn picked(s: &AppState) -> usize {
    s.sketch_session.as_ref().unwrap().pending_curves.len()
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
    let at = se.draw_cursor.as_ref().unwrap().at;
    se.offset_preview(sk, at)
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
    format!("{curves:?}\n{:?}\n{constraints:?}", bits(sk))
}

fn bits(sk: &Sketch) -> Vec<u64> {
    sk.variables().iter().map(|x| x.to_bits()).collect()
}

fn count(sk: &Sketch, wanted: impl Fn(&K) -> bool) -> usize {
    sk.constraints().filter(|(_, c)| wanted(&c.kind)).count()
}

fn normal_lines(sk: &Sketch) -> Vec<CurveId> {
    sk.curves()
        .filter(|(_, c)| !c.construction && matches!(c.kind, CurveKind::Line { .. }))
        .map(|(id, _)| id)
        .collect()
}

fn line_ends(sk: &Sketch, c: CurveId) -> (DVec2, DVec2) {
    let CurveKind::Line { a, b } = sk.curve(c).unwrap().kind else {
        panic!("keine Linie");
    };
    (sk.point_xy(a).unwrap(), sk.point_xy(b).unwrap())
}

/// Die Bits der Enden der ersten `n` normalen Linien.
fn line_bits(sk: &Sketch, n: usize) -> Vec<[u64; 4]> {
    normal_lines(sk)
        .into_iter()
        .take(n)
        .map(|c| {
            let (a, b) = line_ends(sk, c);
            [a.x.to_bits(), a.y.to_bits(), b.x.to_bits(), b.y.to_bits()]
        })
        .collect()
}

fn distance_dim(sk: &Sketch) -> ConstraintId {
    let mut found = sk
        .constraints()
        .filter(|(_, c)| matches!(c.kind, K::Radius(..)))
        .map(|(id, _)| id);
    let dim = found.next().expect("Radiusmaß");
    assert!(found.next().is_none(), "nur ein Maß");
    dim
}

fn radius_of(sk: &Sketch, dim: ConstraintId) -> f64 {
    match sk.constraint(dim).unwrap().kind {
        K::Radius(_, r) => r,
        _ => panic!("kein Radius"),
    }
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

fn lo_hi(line: &[DVec2]) -> (DVec2, DVec2) {
    let lo = line.iter().copied().reduce(DVec2::min).unwrap();
    let hi = line.iter().copied().reduce(DVec2::max).unwrap();
    (lo, hi)
}

fn set_value(s: &mut AppState, dim: ConstraintId, value: f64) -> Result<(), String> {
    let (se, sk) = s.session_sketch_mut().unwrap();
    let result = se.set_dimension_value(sk, dim, value);
    s.observe_sketch(false);
    result
}

fn pick_with(s: &mut AppState, at: DVec2, chain: bool) {
    set_tool(s, Tool::Offset);
    s.sketch_session.as_mut().unwrap().offset_chain = chain;
    click(s, at).unwrap();
}

fn regular_polygon(sk: &mut Sketch, n: usize, r: f64) -> Vec<CurveId> {
    let corners: Vec<(f64, f64)> = (0..n)
        .map(|i| {
            let a = std::f64::consts::TAU * i as f64 / n as f64;
            (r * a.cos(), r * a.sin())
        })
        .collect();
    polygon(sk, &corners)
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

fn button(p: Pos2, pressed: bool) -> Event {
    Event::PointerButton {
        pos: p,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
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

fn raw_click(ctx: &egui::Context, s: &mut AppState, at: DVec2) {
    let p = screen(s, at);
    play(
        ctx,
        s,
        vec![
            vec![Event::PointerMoved(p)],
            vec![button(p, true)],
            vec![button(p, false)],
            vec![],
        ],
    );
}

// ---- Die ganze App -----------------------------------------------------------------------------

struct Rig {
    ctx: egui::Context,
    app: PlanoraApp,
    time: f64,
    /// Eine Fläche, die wie die Statuszeile den Fokus annimmt, aber keine Tasten liest.
    surface: Option<egui::Id>,
}

impl Rig {
    fn new() -> Rig {
        Rig::with_params(&[])
    }

    /// Mit Parametern: Bei offener Skizze lassen sie sich nicht mehr anlegen.
    fn with_params(params: &[(&str, &str)]) -> Rig {
        let ctx = egui::Context::default();
        ctx.set_fonts(fonts::definitions());
        theme::apply(&ctx, true);
        PlanoraApp::setup_context(&ctx);
        let root = common::temp_root("planora-sketch-offset");
        let mut w = Workspace::new(Some(Library::open(&root).unwrap()), Config::default());
        w.config_path = root.join("config.toml");
        let s = &mut w.active_tab_mut().state;
        for (name, formula) in params {
            param(s, name, formula);
        }
        let sk = s
            .add_sketch(SketchPlane::Standard(StandardPlane::XY))
            .unwrap();
        s.open_sketch(sk).unwrap();
        w.observe_history(false);
        let mut rig = Rig {
            ctx,
            app: PlanoraApp::for_tests(w),
            time: 0.0,
            surface: None,
        };
        rig.settle();
        rig
    }

    /// Nimmt dem Feld am Zeiger den Fokus: Er liegt danach auf einer Fläche, die nicht tippt.
    fn focus_elsewhere(&mut self) {
        let id = egui::Id::new("planora-test-surface");
        self.surface = Some(id);
        planora_app::viewport::focus_is_not_typing(&self.ctx, id);
        self.frame(vec![]);
        self.ctx.memory_mut(|m| m.request_focus(id));
        self.settle();
        assert!(self.ctx.memory(|m| m.has_focus(id)), "Fokus woanders");
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
        let surface = self.surface;
        self.ctx
            .run_ui(raw, |ui| {
                if let Some(id) = surface {
                    planora_app::viewport::focus_is_not_typing(ui.ctx(), id);
                }
                app.ui(ui, &mut f);
                if let Some(id) = surface {
                    let spot = Rect::from_min_size(pos2(W - 4.0, H - 4.0), Vec2::splat(2.0));
                    ui.interact(spot, id, egui::Sense::click());
                }
            })
            .drop_without_applying_deltas();
    }

    fn settle(&mut self) {
        for _ in 0..3 {
            self.frame(vec![]);
        }
    }

    fn press(&mut self, k: Key) {
        self.frame(vec![key(k)]);
        self.settle();
    }

    fn at_pixel(&mut self, p: Pos2) -> DVec2 {
        self.frame(vec![Event::PointerMoved(p)]);
        self.frame(vec![]);
        let se = self.st().sketch_session.as_ref().unwrap();
        se.draw_cursor.as_ref().expect("Zeiger").raw
    }

    fn click(&mut self, p: Pos2) {
        self.frame(vec![Event::PointerMoved(p)]);
        self.frame(vec![button(p, true)]);
        self.frame(vec![button(p, false)]);
        self.frame(vec![]);
    }
}

/// Fahrt: `O`, Klick auf eine Seite, Abstand tippen, Eingabe.
#[test]
fn the_whole_trip_with_o_a_click_a_typed_distance_and_enter() {
    let mut rig = Rig::new();
    rig.press(Key::R);
    rig.click(pos2(800.0, 350.0));
    rig.click(pos2(1200.0, 700.0));
    rig.press(Key::Escape);
    assert_eq!(sketch(rig.st()).curves().count(), 4);

    rig.press(Key::O);
    assert_eq!(rig.st().sketch_session.as_ref().unwrap().tool, Tool::Offset);
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
        lo_hi(&xy)
    };
    let steps = undo_len(rig.st());
    rig.click(pixel(v((lo.x + hi.x) / 2.0, lo.y)));
    assert_eq!(picked(rig.st()), 4, "Kette ist die Vorgabe");
    let fields = rig.st().sketch_session.as_ref().unwrap().input.fields[0].kind;
    assert_eq!(fields, F::OffsetDistance);

    // Zeiger innen, „5“ getippt, Eingabe im Feld am Zeiger.
    rig.frame(vec![Event::PointerMoved(pixel(v(
        (lo.x + hi.x) / 2.0,
        lo.y + 1.0,
    )))]);
    rig.settle();
    rig.frame(vec![Event::Text("5".into())]);
    rig.settle();
    rig.press(Key::Enter);

    let sk = sketch(rig.st()).clone();
    assert_eq!(normal_lines(&sk).len(), 8, "vier Linien dazu");
    let dim = distance_dim(&sk);
    assert_eq!(radius_of(&sk, dim), 5.0);
    assert_clean(&sk);
    assert_eq!(undo_len(rig.st()), steps + 1, "ein Schritt");
    let se = rig.st().sketch_session.as_ref().unwrap();
    assert_eq!(se.tool, Tool::Offset, "das Werkzeug bleibt");
    assert!(se.pending_curves.is_empty(), "ohne Kette");
}

/// Rechteck zeichnen und die Mitte seiner Unterkante, die Mitte seiner linken Kante und einen Punkt
/// innen als Pixel liefern.
fn drawn_rectangle(rig: &mut Rig) -> [Pos2; 3] {
    rig.press(Key::R);
    rig.click(pos2(800.0, 350.0));
    rig.click(pos2(1200.0, 700.0));
    rig.press(Key::Escape);
    assert_eq!(sketch(rig.st()).curves().count(), 4);
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
        lo_hi(&xy)
    };
    [
        pixel(v((lo.x + hi.x) / 2.0, lo.y)),
        pixel(v(lo.x, (lo.y + hi.y) / 2.0)),
        pixel(v((lo.x + hi.x) / 2.0, lo.y + 1.0)),
    ]
}

fn formulas_of(rig: &mut Rig) -> Vec<String> {
    let (_, sf) = rig.st().session_draft_mut().unwrap();
    sf.formulas.values().cloned().collect()
}

/// Gesamtprüfung Skizzen 4: Hat das Feld den Fokus nicht mehr, nimmt die Eingabe auf der
/// Zeichenfläche denselben Weg wie die im Feld: Die Formel bleibt am Maß.
#[test]
fn enter_outside_the_field_keeps_the_formula_of_an_offset() {
    let mut rig = Rig::with_params(&[("d", "2")]);
    let [bottom, _, inside] = drawn_rectangle(&mut rig);
    rig.press(Key::O);
    rig.click(bottom);
    assert_eq!(picked(rig.st()), 4);
    rig.frame(vec![Event::PointerMoved(inside)]);
    rig.settle();
    rig.frame(vec![Event::Text("d*2".into())]);
    rig.settle();
    rig.focus_elsewhere();
    let steps = undo_len(rig.st());
    rig.press(Key::Enter);

    assert_eq!(formulas_of(&mut rig), vec!["d*2".to_string()]);
    let sk = sketch(rig.st()).clone();
    assert_eq!(radius_of(&sk, distance_dim(&sk)), 4.0);
    assert_eq!(undo_len(rig.st()), steps + 1, "ein Schritt");
}

#[test]
fn enter_outside_the_field_keeps_the_formula_of_a_fillet() {
    let mut rig = Rig::with_params(&[("d", "2")]);
    let [bottom, left, inside] = drawn_rectangle(&mut rig);
    rig.press(Key::F);
    rig.click(bottom);
    rig.click(left);
    let fields = rig.st().sketch_session.as_ref().unwrap().input.fields[0].kind;
    assert_eq!(fields, F::Radius);
    rig.frame(vec![Event::PointerMoved(inside)]);
    rig.settle();
    rig.frame(vec![Event::Text("d*2".into())]);
    rig.settle();
    rig.focus_elsewhere();
    rig.press(Key::Enter);

    assert_eq!(formulas_of(&mut rig), vec!["d*2".to_string()]);
    let sk = sketch(rig.st()).clone();
    let radius = sk
        .constraints()
        .find_map(|(_, c)| match c.kind {
            K::Radius(_, r) => Some(r),
            _ => None,
        })
        .expect("Radiusmaß");
    assert_eq!(radius, 4.0);
}

// ---- Wählen und Abschließen ---------------------------------------------------------------------

#[test]
fn chain_picks_all_four_sides_and_the_preview_equals_the_result() {
    for mut s in [tool_square(), free_square()] {
        let before = sketch(&s).clone();
        let revision = before.revision();
        let steps = undo_len(&s);
        pick_with(&mut s, v(20.0, 0.0), true);
        assert_eq!(picked(&s), 4);
        assert_eq!(sketch(&s).revision(), revision, "Wählen ändert nichts");

        let at = v(20.0, -5.0);
        let shown = preview(&mut s, at).expect("Vorschau");
        assert_eq!(shown.len(), 1);
        assert_eq!(shown[0].len(), 5, "geschlossen: vier Ecken und zurück");
        let (lo, hi) = lo_hi(&shown[0]);
        assert!(close(lo, v(-5.0, -5.0)), "außen: {lo:?}");
        assert!(close(hi, v(45.0, 45.0)), "außen: {hi:?}");

        click(&mut s, at).unwrap();
        let sk = sketch(&s).clone();
        let lines = normal_lines(&sk);
        assert_eq!(lines.len(), 8, "vier Linien dazu");
        assert_eq!(count(&sk, |k| matches!(k, K::Radius(..))), 1);
        assert_eq!(undo_len(&s), steps + 1, "ein Schritt");
        assert_clean(&sk);
        let report = s.sketch_session.as_ref().unwrap().last_report.clone();
        assert_eq!(report.unwrap().iterations, 0, "nichts bewegt sich");
        let old = bits(&before);
        assert_eq!(&bits(&sk)[..old.len()], &old[..], "Vorlage bitgleich");
        // Vorschau = Ergebnis: Jede Linie der Kopie ist ein Stück des gezeigten Zugs.
        for (i, &c) in lines[4..].iter().enumerate() {
            let (a, b) = line_ends(&sk, c);
            assert!(close(a, shown[0][i]), "{a:?} {:?}", shown[0][i]);
            assert!(close(b, shown[0][i + 1]), "{b:?} {:?}", shown[0][i + 1]);
        }
        let se = s.sketch_session.as_ref().unwrap();
        assert_eq!(se.tool, Tool::Offset);
        assert!(se.pending_curves.is_empty());
        assert_eq!(s.sketch_undo_label().unwrap(), t("undo-offset"));
    }
}

#[test]
fn without_chain_only_the_line_with_its_cross_lines() {
    let mut s = free_square();
    pick_with(&mut s, v(20.0, 0.0), false);
    assert_eq!(picked(&s), 1);
    let shown = preview(&mut s, v(20.0, -5.0)).unwrap();
    assert_eq!(shown, vec![vec![v(0.0, -5.0), v(40.0, -5.0)]]);
    click(&mut s, v(20.0, -5.0)).unwrap();
    let sk = sketch(&s).clone();
    assert_eq!(normal_lines(&sk).len(), 5);
    let (a, b) = line_ends(&sk, normal_lines(&sk)[4]);
    assert!(close(a, v(0.0, -5.0)) && close(b, v(40.0, -5.0)));
    let helpers: Vec<_> = sk.curves().filter(|(_, c)| c.construction).collect();
    let cross = helpers
        .iter()
        .filter(|(_, c)| matches!(c.kind, CurveKind::Line { .. }))
        .count();
    assert_eq!(cross, 2, "zwei Querlinien");
    assert_eq!(helpers.len(), 3, "und ein Rollkreis");
    assert_clean(&sk);
}

/// Eine frühere Kopie hängt über Querlinien an der Vorlage; die Kette läuft nicht hinüber.
#[test]
fn the_chain_does_not_run_over_the_cross_lines_of_an_earlier_offset() {
    let mut s = open_line();
    pick_with(&mut s, v(20.0, 0.0), true);
    click(&mut s, v(20.0, 5.0)).unwrap();
    assert_eq!(normal_lines(sketch(&s)).len(), 2);
    click(&mut s, v(20.0, 0.0)).unwrap();
    assert_eq!(picked(&s), 1, "nur die Vorlage, nicht auch die Kopie");
}

#[test]
fn a_double_clicked_chain_goes_straight_to_the_distance() {
    let mut s = free_square();
    let first = normal_lines(sketch(&s))[0];
    {
        let (se, sk) = s.session_sketch_mut().unwrap();
        se.select_chain(sk, first, false);
    }
    set_tool(&mut s, Tool::Offset);
    assert_eq!(picked(&s), 4);
    let (se, sk) = s.session_sketch_mut().unwrap();
    assert!(!se.picks_curves());
    se.sync_input(sk);
    assert_eq!(se.input.fields.len(), 1);
    assert_eq!(se.input.fields[0].kind, F::OffsetDistance);
    click(&mut s, v(20.0, -5.0)).unwrap();
    assert_eq!(normal_lines(sketch(&s)).len(), 8);
    let se = s.sketch_session.as_ref().unwrap();
    assert!(se.selection.is_empty(), "ohne Kette weiter");
    assert!(se.picks_curves(), "der nächste Klick wählt");
}

/// Doppelklick auf eine Linie mit früherem Versatz: Die Auswahl hat Vorlage, Querlinien und Kopie;
/// genommen wird eine Kette wie beim Klick.
#[test]
fn a_preselection_over_cross_lines_is_one_chain_without_helpers() {
    let mut s = open_line();
    pick_with(&mut s, v(20.0, 0.0), true);
    click(&mut s, v(20.0, 5.0)).unwrap();
    let template = normal_lines(sketch(&s))[0];
    set_tool(&mut s, Tool::Select);
    {
        let (se, sk) = s.session_sketch_mut().unwrap();
        se.select_chain(sk, template, false);
        assert!(se.selection.len() >= 3, "{:?}", se.selection);
    }
    set_tool(&mut s, Tool::Offset);
    assert_eq!(picked(&s), 1);
    assert!(preview(&mut s, v(20.0, -7.0)).is_some());
    click(&mut s, v(20.0, -7.0)).unwrap();
    assert_eq!(normal_lines(sketch(&s)).len(), 3);

    // Rollkreise und Querlinien in einem Rahmen mit Linien: Sie stören nicht.
    set_tool(&mut s, Tool::Select);
    {
        let (se, sk) = s.session_sketch_mut().unwrap();
        let all = sk
            .curves()
            .map(|(c, _)| planora_app::sketch_state::Item::Curve(c));
        se.selection = all.collect();
    }
    set_tool(&mut s, Tool::Offset);
    assert_eq!(picked(&s), 1);
    assert_eq!(message(&s), None);
    let shown = preview(&mut s, v(20.0, -9.0));
    assert!(shown.is_some(), "keine Kreuzung");
}

/// Zwei Ketten mit entgegengesetztem Umlauf: Der Zeiger zählt je Kette, die Richtung nicht.
#[test]
fn two_preselected_chains_go_to_the_pointer_side_whatever_their_direction() {
    let mut s = sketch_with(|sk| {
        polygon(sk, &SQUARE);
        polygon(
            sk,
            &[(60.0, 0.0), (60.0, 40.0), (100.0, 40.0), (100.0, 0.0)],
        );
    });
    {
        let lines = normal_lines(sketch(&s));
        let (se, sk) = s.session_sketch_mut().unwrap();
        se.select_chain(sk, lines[0], false);
        se.select_chain(sk, lines[4], true);
    }
    set_tool(&mut s, Tool::Offset);
    assert_eq!(picked(&s), 8);
    type_in(&mut s, 0, "3");
    let shown = preview(&mut s, v(50.0, 20.0)).expect("Vorschau");
    assert_eq!(shown.len(), 2);
    let (lo, hi) = lo_hi(&shown[0]);
    assert!(close(lo, v(-3.0, -3.0)) && close(hi, v(43.0, 43.0)));
    let (lo, hi) = lo_hi(&shown[1]);
    assert!(close(lo, v(57.0, -3.0)) && close(hi, v(103.0, 43.0)));
}

// ---- Seite und Abstand ---------------------------------------------------------------------------

#[test]
fn a_typed_distance_takes_the_side_from_the_pointer_and_a_minus_changes_it() {
    let mut s = free_square();
    pick_with(&mut s, v(20.0, 0.0), true);
    type_in(&mut s, 0, "10");
    let (lo, hi) = lo_hi(&preview(&mut s, v(20.0, 2.0)).unwrap()[0]);
    assert!(
        close(lo, v(10.0, 10.0)) && close(hi, v(30.0, 30.0)),
        "innen"
    );
    let (lo, hi) = lo_hi(&preview(&mut s, v(20.0, -2.0)).unwrap()[0]);
    assert!(close(lo, v(-10.0, -10.0)) && close(hi, v(50.0, 50.0)));

    type_in(&mut s, 0, "-10");
    let (lo, hi) = lo_hi(&preview(&mut s, v(20.0, 2.0)).unwrap()[0]);
    assert!(
        close(lo, v(-10.0, -10.0)) && close(hi, v(50.0, 50.0)),
        "außen"
    );
    click(&mut s, v(20.0, 2.0)).unwrap();
    let sk = sketch(&s).clone();
    let dim = distance_dim(&sk);
    assert_eq!(radius_of(&sk, dim), 10.0, "das Maß ist der Betrag");
    let (_, sf) = s.session_draft_mut().unwrap();
    assert_eq!(sf.formulas.get(&dim), None, "eine Zahl ist keine Formel");
    assert_clean(&sk);
}

/// Der Zeiger läuft in kleinen Schritten über die untere Seite: Die Unterkante der Vorschau liegt
/// immer genau beim Zeiger, die Seite wechselt also dort und sonst springt nichts.
#[test]
fn a_dense_pointer_sequence_changes_the_side_only_on_the_chain() {
    let mut s = free_square();
    pick_with(&mut s, v(20.0, 0.0), true);
    let revision = sketch(&s).revision();
    for i in 0..=2000 {
        let y = -19.991 + 0.017 * f64::from(i);
        let shown = preview(&mut s, v(20.0, y)).expect("Vorschau");
        let (lo, hi) = lo_hi(&shown[0]);
        assert!((lo.y - y).abs() < 1e-9, "Unterkante bei {y}: {lo:?}");
        assert!((lo.x - y).abs() < 1e-9, "linke Kante bei {y}: {lo:?}");
        assert!(
            (hi.y - (40.0 - y)).abs() < 1e-9,
            "Oberkante bei {y}: {hi:?}"
        );
        let (se, sk) = s.session_sketch_mut().unwrap();
        se.sync_input(sk);
        let value = se.field_value(sk, F::OffsetDistance).unwrap();
        assert!((value - y.abs()).abs() < 1e-9, "Feld bei {y}: {value}");
    }
    assert_eq!(
        sketch(&s).revision(),
        revision,
        "die Vorschau ändert nichts"
    );
}

#[test]
fn an_open_line_takes_the_side_from_the_pointer() {
    let mut s = open_line();
    pick_with(&mut s, v(20.0, 0.0), true);
    for i in 1..400 {
        let y = 0.05 * f64::from(i);
        for side in [y, -y] {
            let shown = preview(&mut s, v(17.0, side)).unwrap();
            assert_eq!(shown[0].len(), 2);
            assert!(close(shown[0][0], v(0.0, side)), "{side}: {:?}", shown[0]);
            assert!(close(shown[0][1], v(40.0, side)), "{side}: {:?}", shown[0]);
        }
    }
    assert_eq!(
        preview(&mut s, v(20.0, 0.0)),
        None,
        "auf der Linie keine Seite"
    );
    let before = snapshot(&s);
    click(&mut s, v(20.0, 0.0)).unwrap();
    assert_eq!(snapshot(&s), before, "der Klick auf die Linie tut nichts");
    assert_eq!(picked(&s), 1, "die Stufe bleibt");
    assert_eq!(message(&s), None);
}

#[test]
fn the_inner_l_shape_stays_closed_and_shows_no_crossing() {
    const L: [(f64, f64); 6] = [
        (0.0, 0.0),
        (40.0, 0.0),
        (40.0, 10.0),
        (10.0, 10.0),
        (10.0, 40.0),
        (0.0, 40.0),
    ];
    let mut s = sketch_with(|sk| {
        polygon(sk, &L);
    });
    pick_with(&mut s, v(20.0, 0.0), true);
    for i in 1..490 {
        let d = 0.01 * f64::from(i);
        let (se, sk) = s.session_sketch_mut().unwrap();
        se.update_cursor(sk, Some(v(25.0, d)), PICK);
        let plan = se.offset_plan_at(sk, v(25.0, d)).expect("Plan");
        assert!(plan.is_closed_like_source(), "{d}");
        assert_eq!(plan.loops.len(), 1, "{d}");
        assert_eq!(plan.piece_count(), 6, "{d}");
    }
    click(&mut s, v(25.0, 2.0)).unwrap();
    let sk = sketch(&s).clone();
    assert_eq!(normal_lines(&sk).len(), 12);
    assert_clean(&sk);
}

// ---- Kein Fang ----------------------------------------------------------------------------------

/// `is_drawing_tool` gilt für den Versatz: Raster, Hilfslinien und automatische Abhängigkeiten
/// dürfen weder das Wählen noch den Zeiger der Abstandsstufe berühren.
#[test]
fn no_grid_guide_or_auto_constraint_touches_the_pointer() {
    let mut s = free_square();
    {
        let se = s.sketch_session.as_mut().unwrap();
        se.cursor_opts.grid_step = Some(5.0);
        se.auto_constrain = true;
    }
    set_tool(&mut s, Tool::Offset);
    let probe = |s: &mut AppState, at: DVec2| {
        let (se, sk) = s.session_sketch_mut().unwrap();
        se.update_cursor(sk, Some(at), PICK);
        se.draw_cursor.clone().unwrap()
    };
    // Wählen: neben allem, auf einer Linie und nah an einer Ecke.
    let off = probe(&mut s, v(17.3, 12.1));
    assert_eq!(off.at, v(17.3, 12.1), "kein Raster");
    assert_eq!(off.snap, Snap::None);
    assert!(off.guides.x.is_none() && off.guides.y.is_none());
    assert!(off.autos.is_empty());
    let corner = probe(&mut s, v(0.2, 0.2));
    assert!(matches!(corner.snap, Snap::Curve(_)), "{:?}", corner.snap);

    click(&mut s, v(20.0, 0.0)).unwrap();
    // Abstand: Der Zeiger bleibt die Rohlage, auch neben Ecken, Mitten und auf Achsen der Punkte.
    for at in [
        v(17.3, 12.1),
        v(40.1, 19.9),
        v(0.1, 0.1),
        v(20.05, 39.9),
        v(39.95, 55.0),
    ] {
        let cur = probe(&mut s, at);
        assert_eq!(cur.at, at, "Rohlage");
        assert_eq!(cur.snap, Snap::None);
        assert!(cur.guides.x.is_none() && cur.guides.y.is_none());
        assert!(cur.autos.is_empty());
    }
    let hv = count(sketch(&s), |k| {
        matches!(k, K::Horizontal(..) | K::Vertical(..))
    });
    click(&mut s, v(17.3, -2.1)).unwrap();
    assert_eq!(
        count(sketch(&s), |k| matches!(
            k,
            K::Horizontal(..) | K::Vertical(..)
        )),
        hv,
        "keine automatische Abhängigkeit"
    );
    let dim = distance_dim(sketch(&s));
    assert!(
        (radius_of(sketch(&s), dim) - 2.1).abs() < 1e-9,
        "nicht aufs Raster"
    );
}

// ---- Das Maß bewegt nur die Kopie ---------------------------------------------------------------

#[test]
fn the_dimension_moves_only_the_copy_over_a_dense_sequence() {
    let mut s = free_square();
    pick_with(&mut s, v(20.0, 0.0), true);
    type_in(&mut s, 0, "5");
    click(&mut s, v(20.0, -2.0)).unwrap();
    let sk = sketch(&s).clone();
    let dim = distance_dim(&sk);
    let template = line_bits(&sk, 4);
    let copy = normal_lines(&sk)[4..].to_vec();

    let steps = undo_len(&s);
    set_value(&mut s, dim, 12.0).unwrap();
    assert_eq!(undo_len(&s), steps + 1, "ein Schritt");
    assert_eq!(line_bits(sketch(&s), 4), template, "Vorlage bitgleich");
    assert_clean(sketch(&s));
    for i in 0..300 {
        let d = 0.25 + 0.1 * f64::from(i);
        set_value(&mut s, dim, d).unwrap();
        let sk = sketch(&s);
        assert_eq!(line_bits(sk, 4), template, "Vorlage bei {d}");
        let report = sk.clone().solve();
        assert!(report.redundant.is_empty(), "{d}");
        // Die Kopie liegt außen im genauen Abstand, die Seite wechselt nie.
        let (a, b) = line_ends(sk, copy[0]);
        let near = |p: DVec2, q: DVec2| p.distance(q) < 1e-6;
        assert!(
            near(a, v(-d, -d)) && near(b, v(40.0 + d, -d)),
            "{d}: {a:?} {b:?}"
        );
    }
}

/// Ein Wert, bei dem der Bogen der Kopie verschwände, wird abgelehnt, auch wenn das Halten der
/// Vorlage dabei scheitert und das normale Lösen dran ist.
#[test]
fn a_value_that_makes_an_arc_vanish_is_refused_and_nothing_moves() {
    let mut s = sketch_with(|sk| {
        let m = sk.add_point(0.0, 0.0).unwrap();
        let a = sk.add_point(20.0, 0.0).unwrap();
        let b = sk.add_point(0.0, 20.0).unwrap();
        sk.add_arc(m, a, b).unwrap();
    });
    pick_with(&mut s, v(20.0, 0.0), true);
    type_in(&mut s, 0, "5");
    click(&mut s, v(5.0, 5.0)).unwrap();
    let dim = distance_dim(sketch(&s));
    let before = snapshot(&s);
    let steps = undo_len(&s);
    assert!(set_value(&mut s, dim, 25.0).is_err());
    assert_eq!(snapshot(&s), before, "der Stand bleibt");
    assert_eq!(undo_len(&s), steps);
    assert!(message(&s).is_some());
    for i in 1..190 {
        let d = 0.1 * f64::from(i);
        set_value(&mut s, dim, d).unwrap();
        let sk = sketch(&s);
        let m = sk.point_xy(sk.points().next().unwrap().0).unwrap();
        assert_eq!(m, v(0.0, 0.0), "{d}");
    }
}

/// Die Rundung trägt ihr eigenes Maß: Bei einem Abstand ab ihrem Radius verschwände der Bogen der
/// Kopie, und auch das normale Lösen findet nichts. Darunter steht die Vorlage bitgleich.
#[test]
fn an_inner_offset_of_a_fillet_stops_at_its_radius() {
    let mut s = tool_square();
    {
        let (se, sk) = s.session_sketch_mut().unwrap();
        se.choose_kind(sk, planora_app::sketch_kinds::ToolKind::Fillet);
    }
    click(&mut s, v(20.0, 0.0)).unwrap();
    click(&mut s, v(40.0, 20.0)).unwrap();
    type_in(&mut s, 0, "6");
    click(&mut s, v(35.0, 5.0)).unwrap();
    pick_with(&mut s, v(20.0, 0.0), true);
    assert_eq!(picked(&s), 5);
    type_in(&mut s, 0, "3");
    click(&mut s, v(20.0, 2.0)).unwrap();
    let sk = sketch(&s).clone();
    assert_clean(&sk);
    let dim = sk
        .constraints()
        .find(|(_, c)| matches!(c.kind, K::Radius(r, _) if sk.curve(r).unwrap().construction))
        .expect("Maß des Rollkreises")
        .0;
    let template = line_bits(&sk, 4);
    for i in 1..=46 {
        let d = 0.125 * f64::from(i);
        set_value(&mut s, dim, d).unwrap();
        assert_eq!(line_bits(sketch(&s), 4), template, "{d}");
    }
    let before = snapshot(&s);
    let steps = undo_len(&s);
    for d in [6.0, 6.5, 8.0, 15.0] {
        assert!(set_value(&mut s, dim, d).is_err(), "{d}");
        assert_eq!(snapshot(&s), before, "{d}");
        assert_eq!(undo_len(&s), steps);
    }
}

/// Ein Kreis, den jemand von Hand gezeichnet und bemaßt hat, läuft wie vorher: dasselbe Ergebnis
/// wie derselbe Weg ohne Halten.
#[test]
fn a_hand_circle_behaves_as_before() {
    let build = |construction: bool| {
        sketch_with(|sk| {
            let a = sk.add_point(0.0, 0.0).unwrap();
            let b = sk.add_point(40.0, 0.0).unwrap();
            let line = sk.add_line(a, b).unwrap();
            let m = sk.add_point(20.0, 15.0).unwrap();
            let c = sk.add_circle(m, 5.0).unwrap();
            sk.set_construction(c, construction).unwrap();
            sk.add_constraint(K::Tangent(line, c)).ok();
            sk.add_constraint(K::Radius(c, 5.0)).unwrap();
        })
    };
    for construction in [false, true] {
        let mut s = build(construction);
        let dim = distance_dim(sketch(&s));
        assert_eq!(sketch(&s).offset_hold_points(dim), None);
        let mut plain = sketch(&s).clone();
        plain.prepare_dimension_change(dim, 9.0);
        plain.set_constraint_value(dim, 9.0).unwrap();
        plain.solve();
        set_value(&mut s, dim, 9.0).unwrap();
        assert_eq!(bits(sketch(&s)), bits(&plain), "bitgleich");
    }
}

/// Grenze des Musters, festgehalten, damit ein späterer Umbau bewusst geschieht: Ein von Hand
/// gezeichneter Konstruktionskreis am Ende einer Linie, der eine Parallele berührt und bemaßt ist,
/// sieht aus wie ein Rollkreis. Dann bleibt die Linie stehen, und nur die Parallele geht.
#[test]
fn a_hand_circle_at_a_line_end_touching_a_parallel_counts_as_roller() {
    let mut s = sketch_with(|sk| {
        let a = sk.add_point(0.0, 0.0).unwrap();
        let b = sk.add_point(40.0, 0.0).unwrap();
        let line = sk.add_line(a, b).unwrap();
        let p = sk.add_point(0.0, 10.0).unwrap();
        let q = sk.add_point(40.0, 10.0).unwrap();
        let other = sk.add_line(p, q).unwrap();
        sk.add_constraint(K::Parallel(line, other)).unwrap();
        let c = sk.add_circle(a, 10.0).unwrap();
        sk.set_construction(c, true).unwrap();
        sk.add_constraint(K::Tangent(other, c)).unwrap();
        sk.add_constraint(K::Radius(c, 10.0)).unwrap();
    });
    let dim = distance_dim(sketch(&s));
    assert!(sketch(&s).offset_hold_points(dim).is_some(), "erkannt");
    let template = line_bits(sketch(&s), 1);
    set_value(&mut s, dim, 20.0).unwrap();
    assert_eq!(line_bits(sketch(&s), 1), template, "die Linie steht");
    let other = normal_lines(sketch(&s))[1];
    let (p, q) = line_ends(sketch(&s), other);
    assert!(
        (p.y - 20.0).abs() < 1e-6 && (q.y - 20.0).abs() < 1e-6,
        "{p:?} {q:?}"
    );
}

/// Innen bis fast an die Grenze: Auch ein großer Sprung geht, die Vorlage steht; darüber sagt der
/// Text immer dasselbe.
#[test]
fn the_inner_square_reaches_its_limit_and_says_the_same_beyond_it() {
    for (from, to) in [(19.0, 19.99), (5.0, 19.99), (1.0, 19.99), (0.5, 19.9)] {
        let mut s = free_square();
        pick_with(&mut s, v(20.0, 0.0), true);
        type_in(&mut s, 0, &format!("{from}"));
        click(&mut s, v(20.0, 2.0)).unwrap();
        let dim = distance_dim(sketch(&s));
        let template = line_bits(sketch(&s), 4);
        set_value(&mut s, dim, to).unwrap();
        assert_eq!(line_bits(sketch(&s), 4), template, "{from} {to}");
        assert_clean(sketch(&s));
    }
    let mut s = free_square();
    pick_with(&mut s, v(20.0, 0.0), true);
    type_in(&mut s, 0, "5");
    click(&mut s, v(20.0, 2.0)).unwrap();
    let dim = distance_dim(sketch(&s));
    let before = snapshot(&s);
    for d in [20.0, 21.0, 25.0, 100.0] {
        assert!(set_value(&mut s, dim, d).is_err(), "{d}");
        assert_eq!(message(&s).unwrap(), t("sketch-offset-vanished"), "{d}");
        assert_eq!(snapshot(&s), before, "{d}");
    }
}

/// Der Wert passt, nur eine feste Ecke der Kopie lässt ihn nicht zu: Das ist ein Widerspruch, nicht
/// „nichts bleibt übrig“.
#[test]
fn a_fixed_corner_on_the_copy_is_a_conflict_not_a_vanished_offset() {
    let mut s = free_square();
    pick_with(&mut s, v(20.0, 0.0), true);
    type_in(&mut s, 0, "5");
    click(&mut s, v(20.0, -2.0)).unwrap();
    let dim = distance_dim(sketch(&s));
    {
        let sk = s.session_sketch_mut().unwrap().1;
        let copy = normal_lines(sk)[4];
        let CurveKind::Line { a, .. } = sk.curve(copy).unwrap().kind else {
            panic!()
        };
        let at = sk.point_xy(a).unwrap();
        sk.add_constraint(K::Fixed(a, at.x, at.y)).unwrap();
        sk.solve();
    }
    s.observe_sketch(false);
    let before = snapshot(&s);
    assert!(set_value(&mut s, dim, 6.0).is_err());
    assert_eq!(message(&s).unwrap(), t("sketch-change-conflict"));
    assert_eq!(snapshot(&s), before);
}

/// Zwei eigene Linien schließen Vorlage und Kopie, und eine trägt `Parallel` zur anderen oder zu
/// einer anderen Linie der Vorlage: Die Vorlage bleibt trotzdem stehen.
#[test]
fn parallel_closing_lines_do_not_let_the_template_move() {
    let build = |corners: &[(f64, f64)], at: DVec2, side: DVec2| {
        let mut s = sketch_with(|sk| {
            let points: Vec<PointId> = corners
                .iter()
                .map(|c| sk.add_point(c.0, c.1).unwrap())
                .collect();
            for w in points.windows(2) {
                sk.add_line(w[0], w[1]).unwrap();
            }
        });
        pick_with(&mut s, at, true);
        type_in(&mut s, 0, "5");
        click(&mut s, side).unwrap();
        s
    };
    type Close = fn(&mut Sketch, CurveId, CurveId, CurveId);
    let walls_parallel: Close = |sk, w1, w2, _| {
        sk.add_constraint(K::Parallel(w1, w2)).unwrap();
    };
    let reversed: Close = |sk, w1, w2, _| {
        sk.add_constraint(K::Parallel(w2, w1)).unwrap();
    };
    let to_template: Close = |sk, _, w2, far| {
        sk.add_constraint(K::Parallel(far, w2)).unwrap();
    };
    for close_up in [walls_parallel, reversed, to_template] {
        // Eine Linie; dann ein U, dessen Schenkel parallel zu den Wänden stehen können.
        let u = [(0.0, 40.0), (0.0, 0.0), (60.0, 0.0), (60.0, 40.0)];
        let line = [(0.0, 0.0), (60.0, 0.0)];
        for (corners, at, side) in [
            (&line[..], v(30.0, 0.0), v(30.0, -2.0)),
            (&u[..], v(30.0, 0.0), v(30.0, -2.0)),
        ] {
            let mut s = build(corners, at, side);
            let n = corners.len() - 1;
            {
                let sk = s.session_sketch_mut().unwrap().1;
                let lines = normal_lines(sk);
                let ends = |sk: &Sketch, c: CurveId| match sk.curve(c).unwrap().kind {
                    CurveKind::Line { a, b } => [a, b],
                    _ => panic!(),
                };
                let (first_q, last_q) = (ends(sk, lines[0])[0], ends(sk, lines[n - 1])[1]);
                let (first_k, last_k) = (ends(sk, lines[n])[0], ends(sk, lines[2 * n - 1])[1]);
                let w1 = sk.add_line(first_q, first_k).unwrap();
                let w2 = sk.add_line(last_q, last_k).unwrap();
                close_up(sk, w1, w2, lines[0]);
                sk.solve();
            }
            s.observe_sketch(false);
            let dim = sketch(&s)
                .constraints()
                .find(|(_, c)| matches!(c.kind, K::Radius(..)))
                .unwrap()
                .0;
            assert!(sketch(&s).offset_hold_points(dim).is_some(), "erkannt");
            let template = line_bits(sketch(&s), n);
            for d in [12.0, 7.5, 20.0, 3.0] {
                if set_value(&mut s, dim, d).is_ok() {
                    assert_eq!(radius_of(sketch(&s), dim), d);
                }
                assert_eq!(line_bits(sketch(&s), n), template, "n {n}, Wert {d}");
            }
        }
    }
}

// ---- Review-Focus 1: unmögliche Eingaben ---------------------------------------------------------

#[test]
fn too_large_a_distance_has_no_preview_a_message_and_changes_nothing() {
    let mut s = free_square();
    pick_with(&mut s, v(20.0, 0.0), true);
    type_in(&mut s, 0, "25");
    let before = snapshot(&s);
    let steps = undo_len(&s);
    assert_eq!(preview(&mut s, v(20.0, 2.0)), None);
    assert!(click(&mut s, v(20.0, 2.0)).is_err());
    assert_eq!(message(&s).unwrap(), t("sketch-offset-vanished"));
    assert_eq!(snapshot(&s), before);
    assert_eq!(picked(&s), 4, "die Stufe bleibt");
    assert_eq!(undo_len(&s), steps);
    let (se, sk) = s.session_sketch_mut().unwrap();
    assert_eq!(
        se.offset_plan_at(sk, v(20.0, 2.0)),
        Err(OffsetProblem::Vanished)
    );
}

#[test]
fn an_ellipse_inside_too_far_is_a_cusp() {
    let mut s = sketch_with(|sk| {
        let m = sk.add_point(0.0, 0.0).unwrap();
        sk.add_ellipse(m, v(40.0, 0.0), 20.0).unwrap();
    });
    pick_with(&mut s, v(40.0, 0.0), true);
    assert_eq!(picked(&s), 1);
    type_in(&mut s, 0, "15");
    let before = snapshot(&s);
    assert_eq!(preview(&mut s, v(30.0, 0.0)), None);
    assert!(click(&mut s, v(30.0, 0.0)).is_err());
    assert_eq!(message(&s).unwrap(), t("sketch-offset-cusp"));
    assert_eq!(snapshot(&s), before);
    assert_eq!(picked(&s), 1);
}

#[test]
fn a_t_piece_branches_with_the_chain_and_works_without() {
    let build = || {
        sketch_with(|sk| {
            let a = sk.add_point(0.0, 0.0).unwrap();
            let b = sk.add_point(40.0, 0.0).unwrap();
            let m = sk.add_point(20.0, 0.0).unwrap();
            let c = sk.add_point(20.0, 30.0).unwrap();
            sk.add_line(a, m).unwrap();
            sk.add_line(m, b).unwrap();
            sk.add_line(m, c).unwrap();
        })
    };
    let mut s = build();
    let before = snapshot(&s);
    set_tool(&mut s, Tool::Offset);
    assert!(click(&mut s, v(10.0, 0.0)).is_err());
    assert_eq!(message(&s).unwrap(), t("sketch-offset-branching"));
    assert_eq!(picked(&s), 0, "die Stufe bleibt");
    assert_eq!(snapshot(&s), before);

    // Dieselbe Verzweigung als Vorauswahl: keine Vorschau, beim Klick die Meldung.
    let mut s = build();
    let first = normal_lines(sketch(&s))[0];
    {
        let (se, sk) = s.session_sketch_mut().unwrap();
        se.select_chain(sk, first, false);
    }
    set_tool(&mut s, Tool::Offset);
    assert_eq!(picked(&s), 3);
    assert_eq!(preview(&mut s, v(10.0, 5.0)), None);
    assert!(click(&mut s, v(10.0, 5.0)).is_err());
    assert_eq!(message(&s).unwrap(), t("sketch-offset-branching"));
    assert_eq!(snapshot(&s), before);

    let mut s = build();
    pick_with(&mut s, v(10.0, 0.0), false);
    assert_eq!(picked(&s), 1);
    assert!(preview(&mut s, v(10.0, 5.0)).is_some());
}

// ---- Review-Focus 3: Abbruch und Rückgängig --------------------------------------------------------

#[test]
fn esc_after_the_chain_leaves_nothing() {
    let ctx = context();
    let mut s = free_square();
    let before = snapshot(&s);
    let revision = sketch(&s).revision();
    let steps = undo_len(&s);
    set_tool(&mut s, Tool::Offset);
    play(&ctx, &mut s, vec![vec![]]);
    raw_click(&ctx, &mut s, v(20.0, 0.0));
    assert_eq!(picked(&s), 4);
    play(&ctx, &mut s, vec![vec![key(Key::Escape)], vec![]]);
    let se = s.sketch_session.as_ref().unwrap();
    assert_eq!(se.tool, Tool::Select, "Esc beendet");
    assert!(se.pending_curves.is_empty());
    assert_eq!(snapshot(&s), before);
    assert_eq!(sketch(&s).revision(), revision);
    assert_eq!(undo_len(&s), steps);
}

#[test]
fn changing_the_tool_or_undoing_drops_the_chain() {
    let mut s = tool_square();
    pick_with(&mut s, v(20.0, 0.0), true);
    let before = snapshot(&s);
    set_tool(&mut s, Tool::Line);
    assert_eq!(picked(&s), 0);
    assert_eq!(snapshot(&s), before);

    pick_with(&mut s, v(20.0, 0.0), true);
    assert_eq!(picked(&s), 4);
    assert!(s.undo_sketch());
    assert_eq!(picked(&s), 0);
}

/// Rohereignisse, der Zeiger ein Bildpunkt neben der Linie: Der zweite Klick eines Doppelklicks
/// legt keine hauchdünne Kopie an, und die Kette bleibt.
#[test]
fn a_double_click_beside_the_curve_picks_the_chain_and_commits_nothing() {
    let ctx = context();
    let mut s = free_square();
    set_tool(&mut s, Tool::Offset);
    play(&ctx, &mut s, vec![vec![]]);
    let before = snapshot(&s);
    let on_line = screen(&s, v(20.0, 0.0));
    let p = pos2(on_line.x, on_line.y + 1.0);
    play(
        &ctx,
        &mut s,
        vec![
            vec![Event::PointerMoved(p)],
            vec![button(p, true)],
            vec![button(p, false)],
            vec![button(p, true)],
            vec![button(p, false)],
            vec![],
        ],
    );
    assert_eq!(picked(&s), 4, "die Kette ist gewählt");
    assert_eq!(snapshot(&s), before, "keine Kopie");
    assert_eq!(normal_lines(sketch(&s)).len(), 4);

    // Ein Doppelklick in der Abstandsstufe legt genau eine Kopie an.
    let (lo, _) = lo_hi(&[v(20.0, -5.0)]);
    let q = screen(&s, lo);
    play(
        &ctx,
        &mut s,
        vec![
            vec![Event::PointerMoved(q)],
            vec![button(q, true)],
            vec![button(q, false)],
            vec![button(q, true)],
            vec![button(q, false)],
            vec![],
        ],
    );
    assert_eq!(normal_lines(sketch(&s)).len(), 8, "eine Kopie");
    assert_eq!(picked(&s), 0);
}

/// Eingabe ohne Feld schließt ab wie ein Klick; genau auf der Kette gibt es keinen Abstand.
#[test]
fn enter_commits_from_the_pointer() {
    let mut s = free_square();
    pick_with(&mut s, v(20.0, 0.0), true);
    let before = snapshot(&s);
    {
        let (se, sk) = s.session_sketch_mut().unwrap();
        se.update_cursor(sk, Some(v(20.0, 0.0)), PICK);
        se.finish_tool(sk).unwrap();
    }
    assert_eq!(picked(&s), 4);
    assert_eq!(snapshot(&s), before);
    {
        let (se, sk) = s.session_sketch_mut().unwrap();
        se.update_cursor(sk, Some(v(20.0, -5.0)), PICK);
        se.finish_tool(sk).unwrap();
    }
    s.observe_sketch(false);
    assert_eq!(picked(&s), 0);
    assert_eq!(normal_lines(sketch(&s)).len(), 8);
    assert_clean(sketch(&s));
}

#[test]
fn one_undo_takes_everything_and_redo_is_bit_equal() {
    let mut s = free_square();
    let before = snapshot(&s);
    pick_with(&mut s, v(20.0, 0.0), true);
    type_in(&mut s, 0, "5");
    click(&mut s, v(20.0, -2.0)).unwrap();
    let after = snapshot(&s);
    assert_ne!(after, before);
    assert!(s.undo_sketch());
    assert_eq!(snapshot(&s), before, "alle Kurven, Rollkreise, Maß");
    assert_eq!(picked(&s), 0);
    assert!(s.redo_sketch());
    assert_eq!(snapshot(&s), after, "Wiederholen bitgleich");
}

#[test]
fn a_deleted_curve_falls_out_of_the_chain() {
    let mut s = open_line();
    let line = normal_lines(sketch(&s))[0];
    pick_with(&mut s, v(20.0, 0.0), true);
    {
        let (se, sk) = s.session_sketch_mut().unwrap();
        se.selection = vec![planora_app::sketch_state::Item::Curve(line)];
        se.delete_selection(sk);
    }
    s.observe_sketch(false);
    assert_eq!(picked(&s), 0, "wieder die Wahl");
    assert!(s.sketch_session.as_ref().unwrap().picks_curves());
    assert_eq!(preview(&mut s, v(20.0, 5.0)), None);
    click(&mut s, v(20.0, 5.0)).unwrap();
    assert_eq!(sketch(&s).curves().count(), 0);
}

// ---- Review-Focus 5: Zoll und Formeln --------------------------------------------------------------

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
fn one_inch_is_stored_as_inches_and_its_sign_changes_only_the_side() {
    with_unit(DisplayUnit::Zoll, || {
        for (text, y) in [("1", 25.4), ("-1", -25.4)] {
            let mut s = sketch_with(|sk| {
                let a = sk.add_point(0.0, 0.0).unwrap();
                let b = sk.add_point(100.0, 0.0).unwrap();
                sk.add_line(a, b).unwrap();
            });
            pick_with(&mut s, v(50.0, 0.0), true);
            type_in(&mut s, 0, text);
            // Der Zeiger liegt auf der Seite, zu der die positive Richtung zeigt.
            click(&mut s, v(50.0, 3.0)).unwrap();
            let sk = sketch(&s).clone();
            let dim = distance_dim(&sk);
            assert!((radius_of(&sk, dim) - 25.4).abs() < 1e-9);
            let (a, _) = line_ends(&sk, normal_lines(&sk)[1]);
            assert!((a.y - y).abs() < 1e-9, "{text}: {a:?}");
            let (_, sf) = s.session_draft_mut().unwrap();
            assert_eq!(sf.formulas.get(&dim).map(String::as_str), Some("1 in"));
        }
    });
}

#[test]
fn a_formula_stays_on_the_roller_and_the_copy_follows_the_parameter() {
    let mut s = AppState::new();
    let breite = param(&mut s, "Breite", "20");
    let i = s
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    s.open_sketch(i).unwrap();
    {
        let sk = s.session_sketch_mut().unwrap().1;
        polygon(sk, &SQUARE);
        sk.solve();
    }
    s.observe_sketch(false);
    pick_with(&mut s, v(20.0, 0.0), true);
    // Ein Minus wechselt nur die Seite und bleibt nicht im Text des Maßes.
    type_in(&mut s, 0, "-Breite / 4");
    click(&mut s, v(20.0, 2.0)).unwrap();
    let (_, sf) = s.session_draft_mut().unwrap();
    let dim = distance_dim(&sf.sketch);
    assert_eq!(radius_of(&sf.sketch, dim), 5.0);
    assert_eq!(
        sf.formulas.get(&dim).map(String::as_str),
        Some("Breite / 4")
    );
    let copy = normal_lines(&sf.sketch)[4];
    let (a, _) = line_ends(&sf.sketch, copy);
    assert!(close(a, v(-5.0, -5.0)), "außen: {a:?}");
    let template_before = line_bits(&sf.sketch, 4);

    s.close_sketch();
    let p = s.document.parameter(breite).unwrap().clone();
    s.edit_parameter(
        breite,
        ParamEdit {
            name: p.name,
            formula: "40".into(),
            unit: p.unit,
            comment: p.comment,
        },
    )
    .unwrap();
    let sk = s.document.sketch_of(i).unwrap();
    assert_eq!(radius_of(sk, dim), 10.0, "der Rollkreis folgt");
    let (a, b) = line_ends(sk, copy);
    // Die Kopie folgt: Abstand zur Vorlage 10, gemessen an der Lage, die die Vorlage jetzt hat.
    let first = normal_lines(sk)[0];
    let (p0, p1) = line_ends(sk, first);
    let dir = (p1 - p0).normalize();
    assert!((dir.perp_dot(a - p0).abs() - 10.0).abs() < 1e-6, "{a:?}");
    assert!((dir.perp_dot(b - p0).abs() - 10.0).abs() < 1e-6, "{b:?}");
    // Grenze, festgehalten: Der Weg über Formeln hält die Vorlage nicht fest (Entscheidung 4 des
    // Plans). Hält er sie eines Tages, soll dieser Test das bewusst machen.
    assert_ne!(line_bits(sk, 4), template_before, "die Vorlage geht mit");
}

// ---- Lange Ketten und Näherungen --------------------------------------------------------------------

#[test]
fn a_long_chain_gets_no_ties_and_says_so() {
    let mut s = sketch_with(|sk| {
        regular_polygon(sk, 65, 100.0);
    });
    let at = {
        let sk = sketch(&s);
        let (a, b) = line_ends(sk, normal_lines(sk)[0]);
        (a + b) * 0.5
    };
    pick_with(&mut s, at, true);
    assert_eq!(picked(&s), 65);
    type_in(&mut s, 0, "5");
    let inside = at * 0.5;
    click(&mut s, inside).unwrap();
    assert_eq!(message(&s).unwrap(), t("sketch-offset-not-tied"));
    let sk = sketch(&s).clone();
    assert_eq!(normal_lines(&sk).len(), 130);
    assert_eq!(sk.constraints().count(), 0, "ohne Abhängigkeiten");
    assert_eq!(sk.curves().filter(|(_, c)| c.construction).count(), 0);
}

/// Auch eine reine Näherung ist nicht gebunden; getippt legt sie kein zweites Maß an.
#[test]
fn an_ellipse_gets_no_ties_and_the_typed_value_makes_no_dimension() {
    let mut s = sketch_with(|sk| {
        let m = sk.add_point(0.0, 0.0).unwrap();
        sk.add_ellipse(m, v(40.0, 0.0), 20.0).unwrap();
    });
    pick_with(&mut s, v(40.0, 0.0), true);
    type_in(&mut s, 0, "5");
    click(&mut s, v(60.0, 0.0)).unwrap();
    assert_eq!(message(&s).unwrap(), t("sketch-offset-not-tied"));
    let sk = sketch(&s).clone();
    assert_eq!(sk.constraints().count(), 0);
    assert_eq!(sk.curves().count(), 3, "Ellipse und zwei Splines");
}

/// Messwert: `cargo test --release -p planora-app --test sketch_offset -- --ignored`.
#[test]
#[ignore = "Messtest, nur im Release sinnvoll"]
fn offset_preview_of_200_curves_stays_under_16_ms() {
    let mut s = sketch_with(|sk| {
        regular_polygon(sk, 200, 300.0);
    });
    let at = {
        let sk = sketch(&s);
        let (a, b) = line_ends(sk, normal_lines(sk)[0]);
        (a + b) * 0.5
    };
    pick_with(&mut s, at, true);
    assert_eq!(picked(&s), 200);
    let (se, sk) = s.session_sketch_mut().unwrap();
    let mut best = f64::INFINITY;
    for i in 0..20 {
        let p = at * (0.9 + 0.002 * f64::from(i));
        let start = std::time::Instant::now();
        let shown = se.offset_preview(sk, p).expect("Vorschau");
        best = best.min(start.elapsed().as_secs_f64() * 1000.0);
        assert_eq!(shown.len(), 1);
    }
    println!("offset_preview 200 Kurven: {best:.2} ms");
    assert!(best < 16.0, "{best} ms");
}
