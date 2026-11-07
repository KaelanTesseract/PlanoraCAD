//! Skizzen 5, Task 10: das Werkzeug Kreisförmige Anordnung. Die ganze App läuft ohne Fenster bei
//! 1920×1080 (Fang auf den Ursprung, Winkelgriff, Esc, Rechtsklick, Werkzeugwechsel, Eingabe); die
//! reinen Teile (Vorschau gegen Ergebnis, Raster, Strg, Griffrechnung über dichte Folgen) gehen über
//! die Sitzung.

mod common;

use eframe::App as _;
use egui::epaint::{ClippedShape, Shape};
use egui::{Event, Key, Modifiers, PointerButton, Pos2, Rect, Vec2, pos2, vec2};
use planora_app::app::PlanoraApp;
use planora_app::commands;
use planora_app::keymap::Action;
use planora_app::sketch_input::FieldKind;
use planora_app::sketch_kinds::{PREVIEW_STEPS_PER_TURN, ToolKind};
use planora_app::sketch_pattern::PatternGrip;
use planora_app::sketch_state::{Item, Tool, TransformStage};
use planora_app::state::{AppState, StandardPlane};
use planora_app::workspace::Workspace;
use planora_app::{fonts, theme};
use planora_feature::SketchPlane;
use planora_i18n::{german, tr, with_lang};
use planora_math::DVec2;
use planora_project::config::Config;
use planora_project::library::Library;
use planora_sketch::constraint::ConstraintKind as K;
use planora_sketch::model::{CurveId, PointId, Sketch};
use planora_sketch::pattern::{
    CircularMode, PatternDir, PatternId, PatternKind, PatternPlan, PatternSlot,
};
use planora_sketch::solver::Drag;
use std::f64::consts::{PI, TAU};

const W: f32 = 1920.0;
const H: f32 = 1080.0;
const PICK: f64 = 0.5;

fn v(x: f64, y: f64) -> DVec2 {
    DVec2::new(x, y)
}

// ---- Die ganze App ----------------------------------------------------------------------------

struct Shot {
    texts: Vec<(String, Rect)>,
}

fn walk(shape: &Shape, out: &mut Vec<(String, Rect)>) {
    match shape {
        Shape::Vec(v) => v.iter().for_each(|x| walk(x, out)),
        Shape::Text(t) => out.push((
            t.galley.text().to_string(),
            Rect::from_min_size(t.pos, t.galley.size()),
        )),
        _ => {}
    }
}

impl Shot {
    fn of(shapes: &[ClippedShape]) -> Shot {
        let mut texts = Vec::new();
        for c in shapes {
            walk(&c.shape, &mut texts);
        }
        Shot { texts }
    }

    fn find(&self, exact: &str) -> Option<Pos2> {
        self.texts
            .iter()
            .find(|(t, _)| t == exact)
            .map(|(_, r)| r.center())
    }
}

struct Rig {
    ctx: egui::Context,
    app: PlanoraApp,
    time: f64,
}

impl Rig {
    /// App mit offener, leerer Skizze auf XY.
    fn new() -> Rig {
        let ctx = egui::Context::default();
        ctx.set_fonts(fonts::definitions());
        theme::apply(&ctx, true);
        PlanoraApp::setup_context(&ctx);
        let root = common::temp_root("planora-sketch-pattern-circular");
        let mut w = Workspace::new(Some(Library::open(&root).unwrap()), Config::default());
        w.config_path = root.join("config.toml");
        let s = &mut w.active_tab_mut().state;
        let sketch = s
            .add_sketch(SketchPlane::Standard(StandardPlane::XY))
            .unwrap();
        s.open_sketch(sketch).unwrap();
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

    fn sk(&mut self) -> &mut Sketch {
        self.st().session_sketch_mut().unwrap().1
    }

    fn frame(&mut self, events: Vec<Event>) -> Shot {
        self.time += 1.0 / 60.0;
        let raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(W, H))),
            time: Some(self.time),
            events,
            ..Default::default()
        };
        let mut f = eframe::Frame::_new_kittest();
        let app = &mut self.app;
        let out = self.ctx.run_ui(raw, |ui| app.ui(ui, &mut f));
        let shot = Shot::of(&out.shapes);
        out.drop_without_applying_deltas();
        shot
    }

    fn settle(&mut self) -> Shot {
        self.frame(vec![]);
        self.frame(vec![]);
        self.frame(vec![])
    }

    fn press(&mut self, k: Key) -> Shot {
        self.frame(vec![Event::Key {
            key: k,
            physical_key: Some(k),
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        }]);
        self.settle()
    }

    fn hover(&mut self, p: Pos2) -> Shot {
        self.frame(vec![Event::PointerMoved(p)]);
        self.frame(vec![Event::PointerMoved(p)])
    }

    fn button(p: Pos2, button: PointerButton, pressed: bool) -> Event {
        Event::PointerButton {
            pos: p,
            button,
            pressed,
            modifiers: Modifiers::NONE,
        }
    }

    fn click_with(&mut self, p: Pos2, button: PointerButton) -> Shot {
        self.frame(vec![Event::PointerMoved(p)]);
        self.frame(vec![Rig::button(p, button, true)]);
        self.frame(vec![Rig::button(p, button, false)]);
        self.settle()
    }

    fn click(&mut self, p: Pos2) -> Shot {
        self.click_with(p, PointerButton::Primary)
    }

    /// Zug mit der linken Taste über die Bildpunkte `path`; nach jedem Schritt der Wert `probe`.
    fn drag_path(&mut self, path: &[Pos2], mut probe: impl FnMut(&mut Rig)) {
        let b = PointerButton::Primary;
        self.frame(vec![Event::PointerMoved(path[0])]);
        self.frame(vec![Rig::button(path[0], b, true)]);
        for &p in &path[1..] {
            self.frame(vec![Event::PointerMoved(p)]);
            probe(self);
        }
        let end = *path.last().unwrap();
        self.frame(vec![Rig::button(end, b, false)]);
        self.settle();
    }

    /// Lage in der Skizze unter dem Bildpunkt `p` (mit einem Werkzeug, das die Rohlage zeigt).
    fn at_pixel(&mut self, p: Pos2) -> DVec2 {
        self.hover(p);
        let se = self.st().sketch_session.as_ref().unwrap();
        se.draw_cursor.as_ref().expect("Zeiger").raw
    }

    /// Bildpunkt einer Skizzenlage (die Ansicht von oben ist fast linear) und Länge je Bildpunkt.
    fn pixels(&mut self) -> (impl Fn(DVec2) -> Pos2 + use<>, f64) {
        self.use_tool(Tool::Dimension);
        let (a, b) = (pos2(900.0, 500.0), pos2(1100.0, 600.0));
        let (p, q) = (self.at_pixel(a), self.at_pixel(b));
        self.use_tool(Tool::Select);
        let per_px = (q.x - p.x) / f64::from(b.x - a.x);
        (
            move |s: DVec2| {
                let tx = (s.x - p.x) / (q.x - p.x);
                let ty = (s.y - p.y) / (q.y - p.y);
                pos2(a.x + (b.x - a.x) * tx as f32, a.y + (b.y - a.y) * ty as f32)
            },
            per_px,
        )
    }

    /// Genauer Bildpunkt der Skizzenlage `s` (perspektivische Kamera: nachgemessen).
    fn exact(&mut self, s: DVec2) -> Pos2 {
        let (px, per_px) = self.pixels();
        let mut p = px(s);
        self.use_tool(Tool::Dimension);
        for _ in 0..4 {
            let e = s - self.at_pixel(p);
            p += vec2((e.x / per_px) as f32, (-e.y / per_px) as f32);
        }
        assert!(self.at_pixel(p).distance(s) < 0.05);
        self.use_tool(Tool::Select);
        p
    }

    fn use_tool(&mut self, tool: Tool) {
        let (se, sk) = self.st().session_sketch_mut().unwrap();
        se.set_tool(sk, tool);
        self.settle();
    }

    fn action(&mut self, a: Action) {
        commands::run_action(self.app.ws_mut(), a);
        self.settle();
    }

    fn select(&mut self, items: Vec<Item>) {
        let se = self.st().sketch_session.as_mut().unwrap();
        se.selection = items;
        self.settle();
    }

    fn tool(&mut self) -> Tool {
        self.st().sketch_session.as_ref().unwrap().tool
    }

    fn stage(&mut self) -> TransformStage {
        self.st().sketch_session.as_ref().unwrap().pattern.stage
    }

    fn undo_len(&mut self) -> usize {
        self.st().sketch_history().unwrap().undo_len()
    }

    fn angles(&mut self) -> Vec<f64> {
        let s = self.st();
        let se = s.sketch_session.as_ref().unwrap();
        preview_angles(
            &se.pattern_preview(s.session_sketch().unwrap())
                .expect("Vorschau"),
        )
    }

    fn now(&mut self) -> Option<PatternPlan> {
        self.st().sketch_session.as_ref().unwrap().pattern_now()
    }

    fn type_field(&mut self, kind: FieldKind, text: &str) {
        type_field(self.st(), kind, text);
        self.settle();
    }
}

// ---- Hilfen -----------------------------------------------------------------------------------

/// Mitte eines Linienzugs (Rahmen; ein Kreis mit 96 Schritten liegt symmetrisch).
fn middle(lines: &[Vec<DVec2>]) -> DVec2 {
    let mut lo = DVec2::splat(f64::INFINITY);
    let mut hi = DVec2::splat(f64::NEG_INFINITY);
    for q in lines.iter().flatten() {
        lo = lo.min(*q);
        hi = hi.max(*q);
    }
    (lo + hi) * 0.5
}

/// Winkel (Grad, −180…180] der Mitte jedes Exemplars um den Ursprung, aufsteigend.
fn preview_angles(pre: &[([i32; 2], Vec<Vec<DVec2>>)]) -> Vec<f64> {
    let mut out: Vec<f64> = pre
        .iter()
        .map(|(_, l)| {
            let m = middle(l);
            m.y.atan2(m.x).to_degrees()
        })
        .collect();
    out.sort_by(f64::total_cmp);
    out
}

fn assert_angles(got: &[f64], want: &[f64]) {
    assert_eq!(got.len(), want.len(), "{got:?} statt {want:?}");
    for (g, w) in got.iter().zip(want) {
        assert!((g - w).abs() < 1e-7, "{got:?} statt {want:?}");
    }
}

fn data(sk: &Sketch) -> String {
    format!("{:?}", sk.to_data())
}

fn the_pattern(sk: &Sketch) -> (PatternId, Vec<[i32; 2]>) {
    let (id, d) = sk.patterns().last().expect("ein Muster");
    (id, d.instances.iter().map(|i| i.at).collect())
}

fn center_of(sk: &Sketch, id: PatternId) -> PointId {
    match sk.pattern(id).unwrap().kind {
        PatternKind::Circular { center, .. } => center,
        _ => panic!("kein Kreismuster"),
    }
}

fn value(sk: &Sketch, id: PatternId, slot: PatternSlot) -> f64 {
    sk.pattern(id)
        .unwrap()
        .values
        .iter()
        .find_map(|&c| match sk.constraint(c)?.kind {
            K::PatternValue(_, s, v) if s == slot => Some(v),
            _ => None,
        })
        .expect("Wert")
}

/// Offene Skizze auf XY mit einem Kreis (Mitte 20, 0, Radius 3), gewählt, Werkzeug Kreisförmige
/// Anordnung (wartet auf die Mitte). Dazu der Kreis und seine Mitte.
fn circle_state() -> (AppState, CurveId, PointId) {
    let mut s = AppState::new();
    let i = s
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    s.open_sketch(i).unwrap();
    let (circle, m) = {
        let (se, sk) = s.session_sketch_mut().unwrap();
        let m = sk.add_point(20.0, 0.0).unwrap();
        let circle = sk.add_circle(m, 3.0).unwrap();
        se.last_report = Some(sk.solve());
        se.selection = vec![Item::Curve(circle)];
        se.choose_kind(sk, ToolKind::PatternCircular);
        (circle, m)
    };
    s.observe_sketch(false);
    (s, circle, m)
}

/// Klick der Mitte an `at` mit Raster `grid` und Strg `ctrl`.
fn click_center(s: &mut AppState, at: DVec2, grid: Option<f64>, ctrl: bool) {
    let (se, sk) = s.session_sketch_mut().unwrap();
    se.cursor_opts.grid_step = grid;
    se.cursor_opts.suppressed = ctrl;
    se.click(sk, at, PICK, false).unwrap();
}

fn finish(s: &mut AppState) -> Result<(), String> {
    let (se, sf) = s.session_draft_mut().unwrap();
    let r = se.finish_pattern(sf);
    s.observe_sketch(false);
    r
}

fn type_field(s: &mut AppState, kind: FieldKind, text: &str) {
    let scope = s.document.param_scope();
    let (se, sk) = s.session_sketch_mut().unwrap();
    se.sync_input(sk);
    let i = se
        .input
        .fields
        .iter()
        .position(|f| f.kind == kind)
        .expect("Feld");
    se.type_field(i, text, &scope);
}

/// Art des Kreismusters wie der Schalter im Skizzenfeld.
fn set_mode(s: &mut AppState, mode: CircularMode) {
    let se = s.sketch_session.as_mut().unwrap();
    let plan = se.pattern.plan.as_mut().unwrap();
    let PatternKind::Circular { center, .. } = plan.kind else {
        panic!("kein Kreismuster");
    };
    plan.kind = PatternKind::Circular { center, mode };
}

fn session_angles(s: &AppState) -> Vec<f64> {
    let se = s.sketch_session.as_ref().unwrap();
    preview_angles(
        &se.pattern_preview(s.session_sketch().unwrap())
            .expect("Vorschau"),
    )
}

fn stage(s: &AppState) -> TransformStage {
    s.sketch_session.as_ref().unwrap().pattern.stage
}

/// Die Vorschau (berechnet und gezeichnet, ohne abgewählte) gegen die angelegten Kurven unter 1e-9.
fn assert_preview_is_result(s: &mut AppState) {
    let pre = {
        let se = s.sketch_session.as_ref().unwrap();
        se.pattern_preview(s.session_sketch().unwrap())
            .expect("Vorschau")
    };
    let drawn: Vec<([i32; 2], Vec<Vec<DVec2>>)> = {
        let se = s.sketch_session.as_ref().unwrap();
        se.pattern_preview_all(s.session_sketch().unwrap())
            .unwrap()
            .into_iter()
            .filter(|(_, suppressed, _)| !suppressed)
            .map(|(at, _, l)| (at, l))
            .collect()
    };
    finish(s).unwrap();
    let sk = s.session_sketch().unwrap();
    let (id, _) = the_pattern(sk);
    for lines in [&pre, &drawn] {
        let d = sk.pattern(id).unwrap();
        assert_eq!(lines.len(), d.instances.len());
        for (at, l) in lines {
            let inst = d.instances.iter().find(|i| i.at == *at).expect("Exemplar");
            assert_eq!(l.len(), inst.curves.len());
            for (line, &c) in l.iter().zip(&inst.curves) {
                let made = sk.curve_polyline(c, PREVIEW_STEPS_PER_TURN).unwrap();
                assert_eq!(line.len(), made.len());
                for (a, b) in line.iter().zip(&made) {
                    assert!(a.distance(*b) < 1e-9, "{at:?}: {a:?} gegen {b:?}");
                }
            }
        }
    }
}

// ---- Die ganze App ----------------------------------------------------------------------------

#[test]
fn circular_pattern_centre_on_the_origin_full_count_angle_symmetric_and_one_step() {
    with_lang(&german(), || {
        let mut rig = Rig::new();
        let m = rig.sk().add_point(20.0, 0.0).unwrap();
        let circle = rig.sk().add_circle(m, 3.0).unwrap();
        rig.sk().solve();
        rig.settle();
        let origin = rig.exact(v(0.0, 0.0));
        rig.select(vec![Item::Curve(circle)]);
        let steps = rig.undo_len();
        let before = data(rig.sk());
        let points = rig.sk().points().count();
        rig.action(Action::SketchPatternCircular);
        assert_eq!(rig.tool(), Tool::Pattern);
        assert_eq!(rig.stage(), TransformStage::SetBase);
        // Klick auf den Ursprung (Fang): Vorgabe 6, voll.
        rig.click(origin);
        assert_eq!(rig.stage(), TransformStage::Active);
        assert_angles(&rig.angles(), &[-120.0, -60.0, 60.0, 120.0, 180.0]);
        // Die Mitte entsteht erst beim Anlegen.
        assert_eq!(rig.sk().points().count(), points);
        assert_eq!(rig.undo_len(), steps);
        // Anzahl 8 ⇒ 45°-Schritt.
        rig.type_field(FieldKind::PatternCount, "8");
        assert_angles(
            &rig.angles(),
            &[-135.0, -90.0, -45.0, 45.0, 90.0, 135.0, 180.0],
        );
        // Art „Winkel“ im Skizzenfeld; 120° bei 3 ⇒ 0°, 60°, 120°.
        let shot = rig.settle();
        let winkel = shot
            .find(&tr!("sketch-pattern-angle-mode"))
            .expect("Winkel");
        rig.click(winkel);
        assert!(matches!(
            rig.now().unwrap().kind,
            PatternKind::Circular {
                mode: CircularMode::Angle,
                ..
            }
        ));
        assert_eq!(
            rig.now().unwrap().counts[0],
            8,
            "die getippte Anzahl bleibt"
        );
        rig.type_field(FieldKind::PatternCount, "3");
        rig.type_field(FieldKind::PatternAngle, "120");
        assert_angles(&rig.angles(), &[60.0, 120.0]);
        // „Symmetrisch“ ⇒ −60…60°.
        let shot = rig.settle();
        let sym = shot
            .texts
            .iter()
            .filter(|(t, _)| *t == tr!("sketch-pattern-symmetric"))
            .map(|(_, r)| r.center())
            .next()
            .expect("Symmetrisch");
        rig.click(sym);
        assert_angles(&rig.angles(), &[-60.0, -30.0, 30.0, 60.0]);
        assert_eq!(rig.undo_len(), steps);
        // Eingabe: ein Schritt mit Mitte, Muster und Werten.
        rig.press(Key::Enter);
        assert_eq!(rig.tool(), Tool::Select);
        let sk = rig.sk().clone();
        let (id, at) = the_pattern(&sk);
        assert_eq!(at.len(), 4);
        let c = center_of(&sk, id);
        assert!(sk.point_xy(c).unwrap().length() < 1e-9);
        assert_eq!(
            sk.points().count(),
            points + 1 + 4,
            "Mitte und vier Kreismitten"
        );
        assert_eq!(value(&sk, id, PatternSlot::Count), 3.0);
        assert!((value(&sk, id, PatternSlot::Angle) - 120f64.to_radians()).abs() < 1e-12);
        assert_eq!(rig.undo_len(), steps + 1);
        let after = data(&sk);
        assert!(rig.st().undo_sketch());
        assert_eq!(
            data(rig.sk()),
            before,
            "Rückgängig nimmt Mitte, Muster und Werte"
        );
        assert!(rig.st().redo_sketch());
        assert_eq!(data(rig.sk()), after, "Wiederholen bitgleich");
    });
}

#[test]
fn esc_right_click_and_tool_change_leave_nothing_not_even_the_centre() {
    with_lang(&german(), || {
        let mut rig = Rig::new();
        let m = rig.sk().add_point(20.0, 0.0).unwrap();
        let circle = rig.sk().add_circle(m, 3.0).unwrap();
        rig.sk().solve();
        rig.settle();
        let spot = rig.exact(v(-7.3, 4.1));
        let (px, _) = rig.pixels();
        for leave in 0..3 {
            rig.select(vec![Item::Curve(circle)]);
            let steps = rig.undo_len();
            let rev = rig.sk().revision();
            let before = data(rig.sk());
            rig.action(Action::SketchPatternCircular);
            rig.click(spot);
            assert_eq!(rig.stage(), TransformStage::Active, "{leave}");
            match leave {
                0 => {
                    rig.press(Key::Escape);
                }
                1 => {
                    rig.click_with(px(v(30.0, 20.0)), PointerButton::Secondary);
                }
                _ => {
                    rig.action(Action::SketchLine);
                    assert_eq!(rig.tool(), Tool::Line);
                }
            }
            assert_ne!(rig.tool(), Tool::Pattern, "{leave}");
            assert_eq!(rig.sk().revision(), rev, "{leave}");
            assert_eq!(rig.undo_len(), steps, "{leave}");
            assert_eq!(data(rig.sk()), before, "keine Mitte, {leave}");
            rig.use_tool(Tool::Select);
        }
    });
}

#[test]
fn dragging_the_angle_grip_follows_past_180_degrees_without_a_jump() {
    with_lang(&german(), || {
        let mut rig = Rig::new();
        let m = rig.sk().add_point(20.0, 0.0).unwrap();
        let circle = rig.sk().add_circle(m, 3.0).unwrap();
        rig.sk().solve();
        rig.settle();
        let origin = rig.exact(v(0.0, 0.0));
        let (px, _) = rig.pixels();
        rig.select(vec![Item::Curve(circle)]);
        rig.action(Action::SketchPatternCircular);
        rig.click(origin);
        set_mode(rig.st(), CircularMode::Angle);
        rig.settle();
        rig.type_field(FieldKind::PatternCount, "3");
        rig.type_field(FieldKind::PatternAngle, "120");
        // Der Griff sitzt am Ende des Bogens: Mitte der Vorlage um 120° gedreht. Zug im Kreis um
        // die Mitte bis 330° in Schritten von 3°.
        let path: Vec<Pos2> = (0..=70)
            .map(|k| {
                let a = (120.0 + 3.0 * f64::from(k)).to_radians();
                px(v(20.0 * a.cos(), 20.0 * a.sin()))
            })
            .collect();
        let mut seen = Vec::new();
        rig.drag_path(&path, |r| {
            seen.push(r.now().unwrap().values[0].to_degrees())
        });
        for w in seen.windows(2) {
            assert!(w[1] >= w[0] - 1e-9 && w[1] - w[0] < 6.0, "Sprung: {seen:?}");
        }
        let got = rig.now().unwrap().values[0].to_degrees();
        assert!((got - 330.0).abs() < 2.0, "{got}");
        assert_eq!(rig.tool(), Tool::Pattern);
    });
}

// ---- Über die Sitzung -------------------------------------------------------------------------

#[test]
fn the_centre_snaps_to_the_grid_ctrl_takes_the_raw_spot_and_a_point_binds() {
    // Raster 5: die Mitte auf (5, 0), ein neuer Punkt.
    let (mut s, _, _) = circle_state();
    assert_eq!(stage(&s), TransformStage::SetBase);
    let points = s.session_sketch().unwrap().points().count();
    click_center(&mut s, v(6.2, 1.1), Some(5.0), false);
    assert_eq!(stage(&s), TransformStage::Active);
    finish(&mut s).unwrap();
    let sk = s.session_sketch().unwrap();
    let (id, _) = the_pattern(sk);
    let c = center_of(sk, id);
    assert!(sk.point_xy(c).unwrap().distance(v(5.0, 0.0)) < 1e-9);
    assert_eq!(sk.points().count(), points + 1 + 5);

    // Strg: kein Fang, kein Raster; die Rohlage.
    let (mut s, _, _) = circle_state();
    click_center(&mut s, v(6.2, 1.1), Some(5.0), true);
    finish(&mut s).unwrap();
    let sk = s.session_sketch().unwrap();
    let (id, _) = the_pattern(sk);
    assert!(
        sk.point_xy(center_of(sk, id))
            .unwrap()
            .distance(v(6.2, 1.1))
            < 1e-9
    );

    // Auf einem vorhandenen Punkt: Er ist die Mitte, kein neuer Punkt.
    let (mut s, _, _) = circle_state();
    let q = {
        let (_, sk) = s.session_sketch_mut().unwrap();
        sk.add_point(-10.0, 3.0).unwrap()
    };
    let points = s.session_sketch().unwrap().points().count();
    click_center(&mut s, v(-10.1, 3.05), Some(5.0), false);
    finish(&mut s).unwrap();
    let sk = s.session_sketch().unwrap();
    let (id, _) = the_pattern(sk);
    assert_eq!(center_of(sk, id), q);
    assert_eq!(sk.points().count(), points + 5);
}

#[test]
fn a_centre_on_an_instance_of_another_pattern_is_refused_with_the_message() {
    with_lang(&german(), || {
        let (mut s, _, _) = circle_state();
        // Ein Rechteckmuster eines Punkts mit Linie: Exemplar bei (40, 10).
        let corner = {
            let (_, sk) = s.session_sketch_mut().unwrap();
            let a = sk.add_point(30.0, 10.0).unwrap();
            let b = sk.add_point(32.0, 10.0).unwrap();
            let line = sk.add_line(a, b).unwrap();
            let id = sk
                .add_pattern(&PatternPlan {
                    kind: PatternKind::Rect {
                        dirs: [PatternDir::SketchX, PatternDir::SketchY],
                        extent: [false; 2],
                        symmetric: [false; 2],
                    },
                    sources: vec![line],
                    counts: [2, 1],
                    values: [10.0, 5.0],
                    suppressed: Vec::new(),
                })
                .unwrap();
            sk.solve();
            let d = sk.pattern(id).unwrap();
            sk.point_xy(d.instances[0].points[0]).unwrap()
        };
        assert!(corner.distance(v(40.0, 10.0)) < 1e-9);
        let points = s.session_sketch().unwrap().points().count();
        click_center(&mut s, corner, None, false);
        assert_eq!(stage(&s), TransformStage::SetBase, "die Mitte fehlt noch");
        assert_eq!(
            s.sketch_session.as_ref().unwrap().message_text(),
            Some(tr!("sketch-pattern-instance-source"))
        );
        assert_eq!(s.session_sketch().unwrap().points().count(), points);
    });
}

#[test]
fn clicking_an_instance_suppresses_it_and_moving_the_centre_later_moves_the_instances() {
    let (mut s, _, _) = circle_state();
    // Mit Strg: ein freier Punkt (der Fang auf den Ursprung hielte die Mitte dort fest).
    click_center(&mut s, v(0.0, 0.0), None, true);
    // Klick auf das Exemplar bei 180° (Mitte −20, 0, Rand bei −17, 0).
    {
        let (se, sk) = s.session_sketch_mut().unwrap();
        se.click(sk, v(-17.0, 0.0), PICK, false).unwrap();
    }
    let plan = s.sketch_session.as_ref().unwrap().pattern_now().unwrap();
    assert_eq!(plan.suppressed.len(), 1);
    assert_angles(&session_angles(&s), &[-120.0, -60.0, 60.0, 120.0]);
    finish(&mut s).unwrap();
    let (id, at) = the_pattern(s.session_sketch().unwrap());
    assert_eq!(at.len(), 4);
    let c = center_of(s.session_sketch().unwrap(), id);
    // Mitte auf (10, 0) ziehen: Die Exemplare drehen um die neue Mitte.
    let (_, sk) = s.session_sketch_mut().unwrap();
    sk.drag(&[Drag {
        point: c,
        target: v(10.0, 0.0),
    }])
    .unwrap();
    sk.solve();
    assert!(sk.point_xy(c).unwrap().distance(v(10.0, 0.0)) < 1e-9);
    let d = sk.pattern(id).unwrap();
    let mut angles: Vec<f64> = d
        .instances
        .iter()
        .map(|i| {
            let lines: Vec<Vec<DVec2>> = i
                .curves
                .iter()
                .map(|&c| sk.curve_polyline(c, PREVIEW_STEPS_PER_TURN).unwrap())
                .collect();
            let m = middle(&lines) - v(10.0, 0.0);
            assert!((m.length() - 10.0).abs() < 1e-7, "{m:?}");
            m.y.atan2(m.x).to_degrees()
        })
        .collect();
    angles.sort_by(f64::total_cmp);
    assert_angles(&angles, &[-120.0, -60.0, 60.0, 120.0]);
}

#[test]
fn angle_360_with_four_gives_three_instances_none_on_the_source_and_count_one_still_creates() {
    // Review-Focus 1: „Winkel“ 360° bei 4.
    let (mut s, circle, _) = circle_state();
    click_center(&mut s, v(0.0, 0.0), None, false);
    set_mode(&mut s, CircularMode::Angle);
    type_field(&mut s, FieldKind::PatternCount, "4");
    type_field(&mut s, FieldKind::PatternAngle, "360");
    assert_angles(&session_angles(&s), &[-90.0, 90.0, 180.0]);
    finish(&mut s).unwrap();
    let sk = s.session_sketch().unwrap();
    let (id, at) = the_pattern(sk);
    assert_eq!(at.len(), 3);
    let source = middle(&[sk.curve_polyline(circle, PREVIEW_STEPS_PER_TURN).unwrap()]);
    for i in &sk.pattern(id).unwrap().instances {
        for &c in &i.curves {
            let m = middle(&[sk.curve_polyline(c, PREVIEW_STEPS_PER_TURN).unwrap()]);
            assert!(m.distance(source) > 1.0, "ein Exemplar deckt die Vorlage");
            assert!(m.is_finite());
        }
    }

    // „Symmetrisch“ mit 360° bei 3: fünf Lagen gleichmäßig verteilt (Annahme, offene Punkte).
    let (mut s, _, _) = circle_state();
    click_center(&mut s, v(0.0, 0.0), None, false);
    set_mode(&mut s, CircularMode::Symmetric);
    type_field(&mut s, FieldKind::PatternCount, "3");
    type_field(&mut s, FieldKind::PatternAngle, "360");
    assert_angles(&session_angles(&s), &[-144.0, -72.0, 72.0, 144.0]);

    // Anzahl 1: keine Exemplare, OK legt trotzdem an; danach änderbar.
    let (mut s, _, _) = circle_state();
    click_center(&mut s, v(0.0, 0.0), None, false);
    type_field(&mut s, FieldKind::PatternCount, "1");
    assert!(session_angles(&s).is_empty());
    let steps = s.sketch_history().unwrap().undo_len();
    finish(&mut s).unwrap();
    assert_eq!(s.sketch_history().unwrap().undo_len(), steps + 1);
    let (id, at) = the_pattern(s.session_sketch().unwrap());
    assert!(at.is_empty());
    assert_eq!(
        value(s.session_sketch().unwrap(), id, PatternSlot::Count),
        1.0
    );
    {
        let (se, sk) = s.session_sketch_mut().unwrap();
        assert!(se.begin_pattern_edit(sk, id));
    }
    type_field(&mut s, FieldKind::PatternCount, "4");
    finish(&mut s).unwrap();
    let (_, at) = the_pattern(s.session_sketch().unwrap());
    assert_eq!(at.len(), 3);
}

#[test]
fn the_preview_is_the_result_for_full_angle_symmetric_suppressed_and_a_new_centre() {
    for (mode, count, angle, suppress) in [
        (CircularMode::Full, "7", None, true),
        (CircularMode::Angle, "5", Some("-150"), false),
        (CircularMode::Symmetric, "4", Some("200"), true),
        (CircularMode::Symmetric, "3", Some("360"), false),
    ] {
        let (mut s, _, _) = circle_state();
        // Neue Mitte über das Raster.
        click_center(&mut s, v(-2.4, 3.1), Some(2.5), false);
        set_mode(&mut s, mode);
        type_field(&mut s, FieldKind::PatternCount, count);
        if let Some(a) = angle {
            type_field(&mut s, FieldKind::PatternAngle, a);
        }
        if suppress {
            let first = s
                .sketch_session
                .as_ref()
                .unwrap()
                .pattern_preview(s.session_sketch().unwrap())
                .unwrap()[0]
                .0;
            let se = s.sketch_session.as_mut().unwrap();
            se.pattern.plan.as_mut().unwrap().suppressed.push(first);
        }
        assert_preview_is_result(&mut s);
    }
}

#[test]
fn editing_a_full_pattern_switched_to_angle_starts_at_360_and_keeps_its_centre() {
    let (mut s, _, _) = circle_state();
    click_center(&mut s, v(0.0, 0.0), None, false);
    finish(&mut s).unwrap();
    let (id, _) = the_pattern(s.session_sketch().unwrap());
    let c = center_of(s.session_sketch().unwrap(), id);
    {
        let (se, sk) = s.session_sketch_mut().unwrap();
        assert!(se.begin_pattern_edit(sk, id));
    }
    set_mode(&mut s, CircularMode::Angle);
    let plan = s.sketch_session.as_ref().unwrap().pattern_now().unwrap();
    assert!((plan.values[0] - TAU).abs() < 1e-12, "{}", plan.values[0]);
    type_field(&mut s, FieldKind::PatternAngle, "90");
    let points = s.session_sketch().unwrap().points().count();
    assert_preview_is_result(&mut s);
    let sk = s.session_sketch().unwrap();
    assert_eq!(center_of(sk, id), c);
    assert_eq!(sk.points().count(), points, "die Mitte bleibt dieselbe");
    assert!((value(sk, id, PatternSlot::Angle) - PI / 2.0).abs() < 1e-12);
}

#[test]
fn the_angle_grip_unwraps_over_a_dense_turn_and_stops_at_360() {
    for (mode, factor) in [(CircularMode::Angle, 1.0), (CircularMode::Symmetric, 2.0)] {
        let (mut s, _, _) = circle_state();
        click_center(&mut s, v(0.0, 0.0), None, false);
        set_mode(&mut s, mode);
        type_field(&mut s, FieldKind::PatternAngle, "40");
        let (se, sk) = s.session_sketch_mut().unwrap();
        // Der Griff liegt am Ende des Bogens (symmetrisch: am halben Winkel).
        let start = 40f64.to_radians() / factor;
        let at = |a: f64| v(20.0 * a.cos(), 20.0 * a.sin());
        se.begin_pattern_grip(sk, PatternGrip::Angle, at(start));
        assert!(se.pattern.drag.is_some(), "{mode:?}");
        let mut last = 40f64.to_radians();
        for k in 1..=720 {
            let a = start + f64::from(k).to_radians() * 0.5;
            se.pattern_grip_to(sk, at(a));
            let got = se.pattern.plan.as_ref().unwrap().values[0];
            let want = (40f64.to_radians() + factor * f64::from(k).to_radians() * 0.5).min(TAU);
            assert!(
                (got - want).abs() < 1e-9,
                "{mode:?} {k}: {got} statt {want}"
            );
            assert!(got >= last - 1e-12 && got <= TAU + 1e-12);
            last = got;
        }
        // Zurück unter null: der Winkel wird negativ, ohne Sprung, höchstens −360°.
        se.abort_pattern_grip();
        assert!((se.pattern.plan.as_ref().unwrap().values[0] - 40f64.to_radians()).abs() < 1e-12);
        se.begin_pattern_grip(sk, PatternGrip::Angle, at(start));
        let mut last = 40f64.to_radians();
        for k in 1..=900 {
            let a = start - f64::from(k).to_radians() * 0.5;
            se.pattern_grip_to(sk, at(a));
            let got = se.pattern.plan.as_ref().unwrap().values[0];
            assert!(got <= last + 1e-12 && got >= -TAU - 1e-12, "{k}: {got}");
            assert!((got - last).abs() < 0.02, "Sprung bei {k}");
            last = got;
        }
        assert!((last + TAU).abs() < 1e-9);
    }
}

/// Die Namen der Felder `kinds` im Bild (je die erste Fundstelle).
fn field_rects(shot: &Shot, kinds: &[FieldKind]) -> Vec<Rect> {
    kinds
        .iter()
        .map(|k| {
            let name = k.label();
            shot.texts
                .iter()
                .find(|(t, _)| *t == name)
                .map(|(_, r)| *r)
                .unwrap_or_else(|| panic!("Feld {name}"))
        })
        .collect()
}

fn assert_same_rect(got: Rect, want: Rect, what: &str) {
    assert!(
        got.center().distance(want.center()) < 0.5,
        "{what}: {got:?} statt {want:?}"
    );
}

/// Handtest Skizzen 5, Fix 3: Die Felder des Kreismusters stehen fest rechts unterhalb des Rings
/// um die Mitte; Anzahl, Winkel und Zeiger verschieben sie nicht, und sie liegen auf keinem
/// Exemplar.
#[test]
fn the_circular_fields_stay_put_outside_the_ring() {
    with_lang(&german(), || {
        let mut rig = Rig::new();
        let m = rig.sk().add_point(20.0, 0.0).unwrap();
        let circle = rig.sk().add_circle(m, 3.0).unwrap();
        rig.sk().solve();
        rig.settle();
        let (px, _) = rig.pixels();
        let origin = rig.exact(v(0.0, 0.0));
        rig.select(vec![Item::Curve(circle)]);
        rig.action(Action::SketchPatternCircular);
        rig.click(origin);
        assert_eq!(rig.stage(), TransformStage::Active);
        let count = FieldKind::PatternCount;
        let start = field_rects(&rig.settle(), &[count])[0];
        // Außerhalb des Rings (Halbmesser 20 + 3): rechts von ihm und unter ihm.
        assert!(
            start.min.x > px(v(23.0, 0.0)).x && start.min.y > px(v(0.0, -23.0)).y,
            "{start:?}"
        );
        rig.hover(px(v(-20.0, 0.0)));
        assert_same_rect(field_rects(&rig.settle(), &[count])[0], start, "Zeiger");
        rig.type_field(count, "10");
        rig.hover(px(v(0.0, 20.0)));
        assert_same_rect(field_rects(&rig.settle(), &[count])[0], start, "Anzahl 10");
        // Art „Winkel“: ein Feld mehr, das erste bleibt; der Winkel verschiebt nichts.
        let shot = rig.settle();
        let winkel = shot
            .find(&tr!("sketch-pattern-angle-mode"))
            .expect("Winkel");
        rig.click(winkel);
        let kinds = [count, FieldKind::PatternAngle];
        let with_angle = field_rects(&rig.settle(), &kinds);
        assert_same_rect(with_angle[0], start, "Art Winkel");
        rig.type_field(FieldKind::PatternAngle, "200");
        rig.type_field(count, "4");
        let shot = rig.settle();
        assert_same_rect(field_rects(&shot, &kinds)[0], with_angle[0], "Winkel 200");
        assert_same_rect(field_rects(&shot, &kinds)[1], with_angle[1], "Winkel 200");
    });
}

/// Handtest Skizzen 5, Fix 3, rein: Der Anker der Felder des Kreismusters hängt nicht an Anzahl,
/// Art und Winkel; er liegt rechts unten am Quadrat um den Ring.
#[test]
fn pure_circular_field_anchor_ignores_count_mode_and_angle() {
    let (mut s, _, _) = circle_state();
    click_center(&mut s, v(0.0, 0.0), None, true);
    let anchor = |s: &AppState| {
        let se = s.sketch_session.as_ref().unwrap();
        se.pattern_field_anchor(s.session_sketch().unwrap())
            .expect("Anker")
    };
    let start = anchor(&s);
    let r = 20.0 + 3.0 * 2f64.sqrt();
    assert!(start.at.distance(v(r, -r)) < 1e-6, "{start:?}");
    assert!(start.away.x > 0.0 && start.away.y < 0.0);
    type_field(&mut s, FieldKind::PatternCount, "10");
    assert_eq!(anchor(&s), start);
    set_mode(&mut s, CircularMode::Angle);
    type_field(&mut s, FieldKind::PatternAngle, "200");
    assert_eq!(anchor(&s), start);
}
