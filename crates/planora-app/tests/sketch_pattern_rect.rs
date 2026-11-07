//! Skizzen 5, Task 9: das Werkzeug Rechteckige Anordnung. Die ganze App läuft ohne Fenster bei
//! 1920×1080; die reinen Teile (Vorschau gegen Ergebnis, Lagen, Griffrechnung) gehen über die
//! Sitzung.

mod common;

use eframe::App as _;
use egui::epaint::{ClippedShape, Shape};
use egui::{Event, Key, Modifiers, PointerButton, Pos2, Rect, Vec2, pos2, vec2};
use planora_app::app::PlanoraApp;
use planora_app::commands;
use planora_app::keymap::Action;
use planora_app::sketch_input::FieldKind;
use planora_app::sketch_kinds::{PREVIEW_STEPS_PER_TURN, ToolKind};
use planora_app::sketch_panel;
use planora_app::sketch_pattern::{
    PatternGrip, PatternGripDrag, pattern_hit, pattern_layout, pattern_motion,
};
use planora_app::sketch_state::{Item, Tool, TransformStage};
use planora_app::state::{AppState, StandardPlane};
use planora_app::workspace::Workspace;
use planora_app::{fonts, theme};
use planora_feature::{FeatureId, FeatureStatus, ParamEdit, ParamUnit, SketchPlane};
use planora_i18n::{DisplayUnit, german, tr, with_lang, with_unit};
use planora_math::DVec2;
use planora_project::config::Config;
use planora_project::library::Library;
use planora_sketch::constraint::{ConstraintId, ConstraintKind as K};
use planora_sketch::model::{CurveId, Sketch};
use planora_sketch::pattern::{
    PatternDir, PatternId, PatternKind, PatternPlan, PatternPreview, PatternSlot,
};

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
    sketch: FeatureId,
}

impl Rig {
    /// App mit offener, leerer Skizze auf XY; vorher die Parameter `params` (Name, Formel, Einheit).
    fn new(params: &[(&str, &str, ParamUnit)]) -> Rig {
        let ctx = egui::Context::default();
        ctx.set_fonts(fonts::definitions());
        theme::apply(&ctx, true);
        PlanoraApp::setup_context(&ctx);
        let root = common::temp_root("planora-sketch-pattern-rect");
        let mut w = Workspace::new(Some(Library::open(&root).unwrap()), Config::default());
        w.config_path = root.join("config.toml");
        let s = &mut w.active_tab_mut().state;
        for (name, formula, unit) in params {
            let id = s.add_parameter().unwrap();
            s.edit_parameter(
                id,
                ParamEdit {
                    name: (*name).into(),
                    formula: (*formula).into(),
                    unit: *unit,
                    comment: String::new(),
                },
            )
            .unwrap();
        }
        let sketch = s
            .add_sketch(SketchPlane::Standard(StandardPlane::XY))
            .unwrap();
        s.open_sketch(sketch).unwrap();
        w.observe_history(false);
        let mut rig = Rig {
            ctx,
            app: PlanoraApp::for_tests(w),
            time: 0.0,
            sketch,
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

    fn key(&mut self, k: Key, modifiers: Modifiers) -> Shot {
        self.frame(vec![Event::Key {
            key: k,
            physical_key: Some(k),
            pressed: true,
            repeat: false,
            modifiers,
        }]);
        self.settle()
    }

    fn press(&mut self, k: Key) -> Shot {
        self.key(k, Modifiers::NONE)
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

    fn double_click(&mut self, p: Pos2) -> Shot {
        let b = PointerButton::Primary;
        self.frame(vec![Event::PointerMoved(p)]);
        self.frame(vec![Rig::button(p, b, true)]);
        self.frame(vec![Rig::button(p, b, false)]);
        self.frame(vec![Rig::button(p, b, true)]);
        self.frame(vec![Rig::button(p, b, false)]);
        self.settle()
    }

    /// Zug mit der linken Taste von `from` um `by` in kleinen Schritten; `abort`: vor dem
    /// Loslassen die rechte Taste.
    fn drag(&mut self, from: Pos2, by: egui::Vec2, abort: bool) {
        let b = PointerButton::Primary;
        self.frame(vec![Event::PointerMoved(from)]);
        self.frame(vec![Rig::button(from, b, true)]);
        let steps = 10;
        for k in 1..=steps {
            let p = from + by * (k as f32 / steps as f32);
            self.frame(vec![Event::PointerMoved(p)]);
        }
        let end = from + by;
        if abort {
            self.frame(vec![Rig::button(end, PointerButton::Secondary, true)]);
            self.frame(vec![Rig::button(end, PointerButton::Secondary, false)]);
        }
        self.frame(vec![Rig::button(end, b, false)]);
        self.settle();
    }

    /// Lage in der Skizze unter dem Bildpunkt `p` (mit einem Zeichenwerkzeug, das die Rohlage
    /// zeigt).
    fn at_pixel(&mut self, p: Pos2) -> DVec2 {
        self.hover(p);
        let se = self.st().sketch_session.as_ref().unwrap();
        se.draw_cursor.as_ref().expect("Zeiger").raw
    }

    /// Bildpunkt einer Skizzenlage (die Ansicht von oben ist linear) und Länge je Bildpunkt; über
    /// das Bemaßen-Werkzeug gemessen, danach wieder Auswahl.
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

    /// Genauer Bildpunkt der Skizzenlage `s` (die Kamera blickt perspektivisch, die lineare
    /// Abbildung aus [`pixels`](Self::pixels) liegt um einige Bildpunkte daneben): ab der linearen
    /// Lage nachgemessen. Nur im Auswahlwerkzeug.
    fn exact(&mut self, s: DVec2) -> Pos2 {
        let (px, per_px) = self.pixels();
        let mut p = px(s);
        self.use_tool(Tool::Dimension);
        for _ in 0..4 {
            let e = s - self.at_pixel(p);
            p += vec2((e.x / per_px) as f32, (-e.y / per_px) as f32);
        }
        assert!(
            self.at_pixel(p).distance(s) < 0.05,
            "{:?}",
            self.at_pixel(p)
        );
        self.use_tool(Tool::Select);
        p
    }

    fn use_tool(&mut self, tool: Tool) {
        let (se, sk) = self.st().session_sketch_mut().unwrap();
        se.set_tool(sk, tool);
        self.settle();
    }

    fn type_text(&mut self, text: &str) {
        let focused = self.ctx.memory(|m| m.focused());
        assert!(focused.is_some(), "kein Feld hat die Tastatur");
        self.frame(vec![Event::Key {
            key: Key::A,
            physical_key: Some(Key::A),
            pressed: true,
            repeat: false,
            modifiers: Modifiers::COMMAND,
        }]);
        for ch in text.chars() {
            self.frame(vec![Event::Text(ch.to_string())]);
        }
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

    fn undo_len(&mut self) -> usize {
        self.st().sketch_history().unwrap().undo_len()
    }

    fn preview(&mut self) -> Option<PatternPreview> {
        let s = self.st();
        let se = s.sketch_session.as_ref().unwrap();
        se.pattern_preview(s.session_sketch().unwrap())
    }

    fn now(&mut self) -> Option<PatternPlan> {
        self.st().sketch_session.as_ref().unwrap().pattern_now()
    }

    fn message(&mut self) -> Option<String> {
        self.st().sketch_session.as_ref().unwrap().message_text()
    }
}

fn lines(ids: &planora_sketch::RectIds) -> Vec<Item> {
    ids.lines.iter().map(|&c| Item::Curve(c)).collect()
}

fn ats<T>(p: &[([i32; 2], T)]) -> Vec<[i32; 2]> {
    p.iter().map(|(at, _)| *at).collect()
}

/// Der erste Punkt des ersten Linienzugs eines Exemplars der Vorschau.
fn first(p: &[([i32; 2], Vec<Vec<DVec2>>)], at: [i32; 2]) -> DVec2 {
    p.iter().find(|(x, _)| *x == at).expect("Lage").1[0][0]
}

/// Ein Text der Skizze, dessen Gleitkommazahlen bitgenau stehen (Debug schreibt die kürzeste
/// eindeutige Form).
fn data(sk: &Sketch) -> String {
    format!("{:?}", sk.to_data())
}

fn the_pattern(sk: &Sketch) -> (PatternId, Vec<[i32; 2]>) {
    let (id, d) = sk.patterns().next().expect("ein Muster");
    (id, d.instances.iter().map(|i| i.at).collect())
}

fn value_id(sk: &Sketch, id: PatternId, slot: PatternSlot) -> (ConstraintId, f64) {
    sk.pattern(id)
        .unwrap()
        .values
        .iter()
        .find_map(|&c| match sk.constraint(c)?.kind {
            K::PatternValue(_, s, v) if s == slot => Some((c, v)),
            _ => None,
        })
        .expect("Wert")
}

#[test]
fn rect_pattern_previews_follows_fields_suppresses_by_click_and_is_one_step() {
    with_lang(&german(), || {
        let mut rig = Rig::new(&[]);
        let ids = rig.sk().add_rectangle(v(0.0, 0.0), v(10.0, 5.0)).unwrap();
        rig.settle();
        let (px, _) = rig.pixels();
        let p45 = rig.exact(v(45.0, 0.0));
        rig.select(lines(&ids));
        let steps = rig.undo_len();
        let before = data(rig.sk());
        rig.action(Action::SketchPatternRect);
        assert_eq!(rig.tool(), Tool::Pattern);
        // Vorgabe 3 × 1 im Abstand 1,5 × Breite.
        let pre = rig.preview().expect("Vorschau");
        assert_eq!(ats(&pre), vec![[1, 0], [2, 0]]);
        assert!(first(&pre, [1, 0]).distance(v(15.0, 0.0)) < 1e-9);
        assert!(first(&pre, [2, 0]).distance(v(30.0, 0.0)) < 1e-9);
        // Anzahl „4“, Tab, Abstand „20“: Die Vorschau folgt.
        rig.type_text("4");
        rig.press(Key::Tab);
        rig.type_text("20");
        rig.settle();
        let pre = rig.preview().expect("Vorschau");
        assert_eq!(ats(&pre), vec![[1, 0], [2, 0], [3, 0]]);
        assert!(first(&pre, [3, 0]).distance(v(60.0, 0.0)) < 1e-9);
        // Klick auf Exemplar 2: abgewählt (gestrichelt), nochmal: zurück. Wählen ist kein Schritt.
        rig.click(p45);
        assert_eq!(rig.now().unwrap().suppressed, vec![[2, 0]]);
        assert_eq!(ats(&rig.preview().unwrap()), vec![[1, 0], [3, 0]]);
        let all = {
            let s = rig.st();
            let se = s.sketch_session.as_ref().unwrap();
            se.pattern_preview_all(s.session_sketch().unwrap()).unwrap()
        };
        assert!(all.iter().any(|(at, dashed, _)| *at == [2, 0] && *dashed));
        rig.click(p45);
        assert!(rig.now().unwrap().suppressed.is_empty());
        assert_eq!(rig.undo_len(), steps);
        // Eingabe: drei Exemplare, ein Schritt mit den Werten.
        rig.press(Key::Enter);
        assert_eq!(rig.tool(), Tool::Select);
        let sk = rig.sk().clone();
        let (id, at) = the_pattern(&sk);
        assert_eq!(at, vec![[1, 0], [2, 0], [3, 0]]);
        assert_eq!(value_id(&sk, id, PatternSlot::Count1).1, 4.0);
        assert_eq!(value_id(&sk, id, PatternSlot::Spacing1).1, 20.0);
        assert_eq!(rig.undo_len(), steps + 1);
        let after = data(&sk);
        assert!(rig.st().undo_sketch());
        assert_eq!(rig.sk().pattern_count(), 0);
        assert_eq!(data(rig.sk()), before, "Rückgängig nimmt Muster und Werte");
        assert!(rig.st().redo_sketch());
        assert_eq!(data(rig.sk()), after, "Wiederholen bitgleich");

        // Esc, Rechtsklick und ein Werkzeugwechsel lassen nichts zurück; je ein frisches Rechteck
        // (nicht die Vorlage des eben angelegten Musters).
        rig.settle();
        for leave in 0..3 {
            let y = -12.0 - 8.0 * f64::from(leave);
            let fresh = rig.sk().add_rectangle(v(0.0, y), v(10.0, y + 5.0)).unwrap();
            rig.sk().solve();
            rig.settle();
            rig.select(lines(&fresh));
            let steps = rig.undo_len();
            let rev = rig.sk().revision();
            rig.action(Action::SketchPatternRect);
            assert!(rig.preview().is_some());
            match leave {
                0 => {
                    rig.press(Key::Escape);
                }
                1 => {
                    rig.click_with(px(v(30.0, 20.0)), PointerButton::Secondary);
                }
                _ => {
                    // Die Tastatur hat das Feld; der Knopf der Leiste wechselt.
                    rig.action(Action::SketchLine);
                    assert_eq!(rig.tool(), Tool::Line);
                }
            }
            assert_ne!(rig.tool(), Tool::Pattern, "{leave}");
            assert_eq!(rig.sk().revision(), rev, "{leave}");
            assert_eq!(rig.undo_len(), steps, "{leave}");
            assert_eq!(rig.sk().pattern_count(), 1, "{leave}");
            if leave < 2 {
                let sel = rig.st().sketch_session.as_ref().unwrap().selection.clone();
                assert_eq!(sel, lines(&fresh), "die Auswahl bleibt, {leave}");
            }
            assert!(
                fresh
                    .lines
                    .iter()
                    .all(|&c| rig.sk().pattern_of_curve(c).is_none()),
                "{leave}"
            );
        }
    });
}

/// Fix-Runde Task 9 (I1): Ein abgebrochener Griffzug lässt die getippte Formel stehen.
#[test]
fn a_cancelled_grip_drag_keeps_the_typed_formula() {
    with_lang(&german(), || {
        let mut rig = Rig::new(&[("b", "10", ParamUnit::Length)]);
        let ids = rig.sk().add_rectangle(v(0.0, 0.0), v(10.0, 5.0)).unwrap();
        rig.settle();
        let (px, _) = rig.pixels();
        rig.select(lines(&ids));
        rig.action(Action::SketchPatternRect);
        rig.press(Key::Tab);
        rig.type_text("b * 2");
        rig.settle();
        assert_eq!(rig.now().unwrap().values[0], 20.0);
        let (last, _) = {
            let s = rig.st();
            let se = s.sketch_session.as_ref().unwrap();
            se.pattern_handles(s.session_sketch().unwrap())
                .unwrap()
                .arrows[0]
                .unwrap()
        };
        rig.drag(px(last) + vec2(30.0, 0.0), vec2(80.0, 0.0), true);
        assert_eq!(rig.now().unwrap().values[0], 20.0);
        rig.press(Key::Enter);
        assert_eq!(rig.tool(), Tool::Select);
        let (id, _) = the_pattern(rig.sk());
        let (c, value) = value_id(rig.sk(), id, PatternSlot::Spacing1);
        assert_eq!(value, 20.0);
        let formula = rig
            .st()
            .sketch_draft
            .as_ref()
            .unwrap()
            .formulas
            .get(&c)
            .cloned();
        assert_eq!(formula.as_deref(), Some("b * 2"));
        rig.st().close_sketch();
        let b = rig.st().document.parameters()[0].id;
        set_param(rig.st(), b, "30", ParamUnit::Length);
        let feature = rig.sketch;
        let sk = rig.st().document.sketch_of(feature).unwrap().clone();
        let first = sk.pattern(id).unwrap().instances[0].points[0];
        assert!(sk.point_xy(first).unwrap().distance(v(60.0, 0.0)) < 1e-9);
    });
}

/// Fix-Runde Task 9: Ein Doppelklick auf eine Ecke eines Exemplars (der Fang nimmt den Punkt)
/// öffnet das Muster ebenso.
#[test]
fn double_click_on_an_instance_corner_opens_the_pattern() {
    with_lang(&german(), || {
        let mut rig = Rig::new(&[]);
        let ids = rig.sk().add_rectangle(v(0.0, 0.0), v(5.0, 2.5)).unwrap();
        let plan = rect_plan(ids.lines.to_vec(), [4, 1], [10.0, 5.0]);
        let id = rig.sk().add_pattern(&plan).unwrap();
        rig.sk().solve();
        rig.settle();
        let at = rig.exact(v(35.0, 2.5));
        rig.double_click(at);
        assert_eq!(rig.tool(), Tool::Pattern);
        assert_eq!(
            rig.st().sketch_session.as_ref().unwrap().pattern.editing,
            Some(id)
        );
    });
}

#[test]
fn dragging_the_arrow_follows_the_projection_and_right_click_restores() {
    with_lang(&german(), || {
        let mut rig = Rig::new(&[]);
        let ids = rig.sk().add_rectangle(v(0.0, 0.0), v(10.0, 5.0)).unwrap();
        rig.settle();
        let (px, per_px) = rig.pixels();
        rig.select(lines(&ids));
        rig.action(Action::SketchPatternRect);
        let handles = {
            let s = rig.st();
            let se = s.sketch_session.as_ref().unwrap();
            se.pattern_handles(s.session_sketch().unwrap()).unwrap()
        };
        let (last, out) = handles.arrows[0].unwrap();
        assert!(last.distance(v(35.0, 2.5)) < 1e-9, "{last:?}");
        assert_eq!(out, DVec2::X);
        // Mitten auf den Pfeil (er reicht 60 Bildpunkte über das letzte Exemplar hinaus), schräg
        // ziehen: Nur die Projektion auf die Richtung zählt, verteilt auf zwei Schritte.
        let grip = px(last) + vec2(30.0, 0.0);
        // Die Kamera blickt perspektivisch: die Lagen unter Anfang und Ende des Zugs messen.
        let moved = rig.at_pixel(grip + vec2(100.0, -40.0)) - rig.at_pixel(grip);
        assert!((moved.x - 100.0 * per_px).abs() < 0.05 * moved.x.abs());
        rig.drag(grip, vec2(100.0, -40.0), false);
        let want = 15.0 + moved.x / 2.0;
        let got = rig.now().unwrap().values[0];
        assert!((got - want).abs() < 1e-9, "{got} statt {want}");
        assert_eq!(
            rig.now().unwrap().values[1],
            7.5,
            "die andere Richtung bleibt"
        );
        // Rechtsklick im Zug: der Wert davor.
        let (last, _) = {
            let s = rig.st();
            let se = s.sketch_session.as_ref().unwrap();
            se.pattern_handles(s.session_sketch().unwrap())
                .unwrap()
                .arrows[0]
                .unwrap()
        };
        rig.drag(px(last) + vec2(30.0, 0.0), vec2(80.0, 0.0), true);
        let back = rig.now().unwrap().values[0];
        assert!((back - got).abs() < 1e-12, "{back} statt {got}");
        assert_eq!(
            rig.tool(),
            Tool::Pattern,
            "der Rechtsklick nimmt nur den Zug"
        );
    });
}

#[test]
fn double_click_on_an_instance_edits_count_four_to_two_in_one_step_and_keeps_the_binding() {
    with_lang(&german(), || {
        let mut rig = Rig::new(&[]);
        let ids = rig.sk().add_rectangle(v(0.0, 0.0), v(5.0, 2.5)).unwrap();
        let plan = rect_plan(ids.lines.to_vec(), [4, 1], [10.0, 5.0]);
        let id = rig.sk().add_pattern(&plan).unwrap();
        // Eine fremde Linie bindet an Exemplar 1, eine zweite an Exemplar 3.
        let corner = |sk: &Sketch, at: [i32; 2]| {
            let d = sk.pattern(id).unwrap();
            d.instances.iter().find(|i| i.at == at).unwrap().points[0]
        };
        let (c1, c3) = (corner(rig.sk(), [1, 0]), corner(rig.sk(), [3, 0]));
        let f1 = rig.sk().add_point(10.0, -10.0).unwrap();
        let f3 = rig.sk().add_point(30.0, -10.0).unwrap();
        let g1 = rig.sk().add_point(10.0, -15.0).unwrap();
        let g3 = rig.sk().add_point(30.0, -15.0).unwrap();
        rig.sk().add_line(f1, g1).unwrap();
        rig.sk().add_line(f3, g3).unwrap();
        let k1 = rig.sk().add_constraint(K::Coincident(f1, c1)).unwrap();
        let k3 = rig.sk().add_constraint(K::Coincident(f3, c3)).unwrap();
        let report = rig.sk().solve();
        assert!(report.conflicting.is_empty());
        rig.settle();
        // Exemplar 3 liegt bei x = 30 … 35; der Doppelklick trifft seine obere Kante.
        let at = rig.exact(v(32.0, 2.5));
        let steps = rig.undo_len();
        rig.double_click(at);
        assert_eq!(rig.tool(), Tool::Pattern);
        let se = rig.st().sketch_session.as_ref().unwrap();
        assert_eq!(se.pattern.editing, Some(id));
        assert_eq!(se.pattern.stage, TransformStage::Active);
        assert_eq!(rig.undo_len(), steps, "Öffnen ist kein Schritt");
        rig.type_text("2");
        rig.press(Key::Enter);
        assert_eq!(rig.tool(), Tool::Select);
        let sk = rig.sk().clone();
        assert_eq!(the_pattern(&sk), (id, vec![[1, 0]]));
        assert!(sk.constraint(k1).is_some(), "Bindung an Exemplar 1 bleibt");
        assert!(sk.constraint(k3).is_none());
        assert_eq!(
            rig.message(),
            Some(tr!("sketch-pattern-dropped", n = 1)),
            "die Meldung nennt die entfallene Abhängigkeit"
        );
        assert_eq!(rig.undo_len(), steps + 1);
        assert!(rig.st().undo_sketch());
        let (_, at) = the_pattern(rig.sk());
        assert_eq!(at.len(), 3);
        assert!(rig.sk().constraint(k3).is_some());
    });
}

#[test]
fn the_list_shows_distance_one_its_field_keeps_a_formula_and_the_parameter_moves_instances() {
    with_lang(&german(), || {
        let mut rig = Rig::new(&[("b", "10", ParamUnit::Length)]);
        let ids = rig.sk().add_rectangle(v(0.0, 0.0), v(10.0, 5.0)).unwrap();
        let plan = rect_plan(ids.lines.to_vec(), [3, 1], [15.0, 7.5]);
        let id = rig.sk().add_pattern(&plan).unwrap();
        rig.settle();
        let shot = rig.settle();
        let row = format!("{} 15,00 mm", tr!("sketch-pattern-slot-spacing1"));
        let at = shot.find(&row).unwrap_or_else(|| panic!("{row} fehlt"));
        assert!(
            shot.find(&format!("{} 3", tr!("sketch-pattern-slot-count1")))
                .is_some()
        );
        rig.double_click(at);
        assert!(sketch_panel::dimension_editor_open(&rig.ctx));
        rig.type_text("b * 2");
        rig.press(Key::Enter);
        let (c, value) = value_id(rig.sk(), id, PatternSlot::Spacing1);
        assert_eq!(value, 20.0);
        let formula = rig
            .st()
            .sketch_draft
            .as_ref()
            .unwrap()
            .formulas
            .get(&c)
            .cloned();
        assert_eq!(formula.as_deref(), Some("b * 2"));
        let at1 = |sk: &Sketch| {
            let d = sk.pattern(id).unwrap();
            sk.point_xy(d.instances[0].points[0]).unwrap()
        };
        assert!(at1(rig.sk()).distance(v(20.0, 0.0)) < 1e-9);
        rig.st().close_sketch();
        let b = rig.st().document.parameters()[0].id;
        set_param(rig.st(), b, "30", ParamUnit::Length);
        let feature = rig.sketch;
        let sk = rig.st().document.sketch_of(feature).unwrap().clone();
        assert!(at1(&sk).distance(v(60.0, 0.0)) < 1e-9, "{:?}", at1(&sk));
    });
}

#[test]
fn delete_on_an_instance_suppresses_it_and_on_the_source_dissolves_with_a_message() {
    with_lang(&german(), || {
        let mut rig = Rig::new(&[]);
        let ids = rig.sk().add_rectangle(v(0.0, 0.0), v(10.0, 5.0)).unwrap();
        let plan = rect_plan(ids.lines.to_vec(), [3, 1], [15.0, 7.5]);
        let id = rig.sk().add_pattern(&plan).unwrap();
        // Ein fremder Punkt bindet an Exemplar 2: Seine Abhängigkeit entfällt mit dem Exemplar.
        let corner = rig.sk().pattern(id).unwrap().instances[1].points[0];
        let f = rig.sk().add_point(30.0, -10.0).unwrap();
        rig.sk().add_constraint(K::Coincident(f, corner)).unwrap();
        rig.sk().solve();
        rig.settle();
        let (p35, p5) = (rig.exact(v(35.0, 0.0)), rig.exact(v(5.0, 0.0)));
        let steps = rig.undo_len();
        rig.click(p35);
        rig.press(Key::Delete);
        let sk = rig.sk().clone();
        assert_eq!(the_pattern(&sk), (id, vec![[1, 0]]));
        assert_eq!(sk.pattern(id).unwrap().suppressed, vec![[2, 0]]);
        assert_eq!(
            sk.constraints().count(),
            4 + 4,
            "nur Rechteck und Musterwerte"
        );
        assert_eq!(
            rig.message(),
            Some(tr!("sketch-pattern-instance-dropped", n = 1))
        );
        assert_eq!(rig.undo_len(), steps + 1);
        // Die Vorlage löschen löst das Muster auf; die Meldung sagt es.
        rig.click(p5);
        rig.press(Key::Delete);
        assert_eq!(rig.sk().pattern_count(), 0);
        assert_eq!(rig.message(), Some(tr!("sketch-pattern-dissolved", n = 1)));
    });
}

// ---- Über die Sitzung ---------------------------------------------------------------------------

fn rect_plan(sources: Vec<CurveId>, counts: [u32; 2], values: [f64; 2]) -> PatternPlan {
    PatternPlan {
        kind: PatternKind::Rect {
            dirs: [PatternDir::SketchX, PatternDir::SketchY],
            extent: [false; 2],
            symmetric: [false; 2],
        },
        sources,
        counts,
        values,
        suppressed: Vec::new(),
    }
}

fn set_param(s: &mut AppState, id: planora_feature::ParamId, formula: &str, unit: ParamUnit) {
    let name = s
        .document
        .parameters()
        .iter()
        .find(|p| p.id == id)
        .unwrap()
        .name
        .clone();
    s.edit_parameter(
        id,
        ParamEdit {
            name,
            formula: formula.into(),
            unit,
            comment: String::new(),
        },
    )
    .unwrap();
}

/// Offene Skizze auf XY mit den Parametern `params`; `build` legt den Inhalt an und liefert die
/// Kurven der Vorlage, die gewählt werden.
fn sketch_with(
    params: &[(&str, &str, ParamUnit)],
    build: impl FnOnce(&mut Sketch) -> Vec<CurveId>,
) -> (AppState, FeatureId) {
    let mut s = AppState::new();
    for (name, formula, unit) in params {
        let id = s.add_parameter().unwrap();
        s.edit_parameter(
            id,
            ParamEdit {
                name: (*name).into(),
                formula: (*formula).into(),
                unit: *unit,
                comment: String::new(),
            },
        )
        .unwrap();
    }
    let i = s
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    s.open_sketch(i).unwrap();
    {
        let (se, sk) = s.session_sketch_mut().unwrap();
        let curves = build(sk);
        se.last_report = Some(sk.solve());
        se.selection = curves.into_iter().map(Item::Curve).collect();
        se.choose_kind(sk, ToolKind::PatternRect);
    }
    s.observe_sketch(false);
    (s, i)
}

fn finish(s: &mut AppState) -> Result<(), String> {
    let (se, sf) = s.session_draft_mut().unwrap();
    let r = se.finish_pattern(sf);
    s.observe_sketch(false);
    r
}

fn plan_mut(s: &mut AppState) -> &mut PatternPlan {
    s.sketch_session
        .as_mut()
        .unwrap()
        .pattern
        .plan
        .as_mut()
        .unwrap()
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

fn session_preview(s: &AppState) -> Vec<([i32; 2], Vec<Vec<DVec2>>)> {
    let se = s.sketch_session.as_ref().unwrap();
    se.pattern_preview(s.session_sketch().unwrap())
        .expect("Vorschau")
}

/// Rechteck, Kreis und Bogen als Vorlage; dazu eine schräge Linie (die letzte Kurve, nicht
/// gewählt) für die Richtung.
fn mixed(sk: &mut Sketch) -> Vec<CurveId> {
    let r = sk.add_rectangle(v(0.0, 0.0), v(10.0, 5.0)).unwrap();
    let m = sk.add_point(5.0, 12.0).unwrap();
    let circle = sk.add_circle(m, 3.0).unwrap();
    let c = sk.add_point(20.0, 0.0).unwrap();
    let a = sk.add_point(24.0, 0.0).unwrap();
    let b = sk.add_point(20.0, 4.0).unwrap();
    let arc = sk.add_arc(c, a, b).unwrap();
    let mut out = r.lines.to_vec();
    out.extend([circle, arc]);
    out
}

fn slanted_line(s: &mut AppState) -> CurveId {
    let (_, sk) = s.session_sketch_mut().unwrap();
    let p = sk.add_point(0.0, -20.0).unwrap();
    let q = sk.add_point(30.0, -5.0).unwrap();
    sk.add_line(p, q).unwrap()
}

/// Die Vorschau gegen die angelegten Kurven an derselben Lage: Linienzüge unter 1e-9.
fn assert_preview_is_result(s: &mut AppState, setup: impl FnOnce(&mut AppState)) {
    setup(s);
    let pre = session_preview(s);
    // Die gezeichnete Vorschau ohne die abgewählten Lagen ist dieselbe.
    let drawn = {
        let se = s.sketch_session.as_ref().unwrap();
        se.pattern_preview_all(s.session_sketch().unwrap()).unwrap()
    };
    let drawn: Vec<([i32; 2], Vec<Vec<DVec2>>)> = drawn
        .into_iter()
        .filter(|(_, suppressed, _)| !suppressed)
        .map(|(at, _, l)| (at, l))
        .collect();
    let editing = s.sketch_session.as_ref().unwrap().pattern.editing;
    finish(s).unwrap();
    let sk = s.session_sketch().unwrap();
    let id = editing.unwrap_or_else(|| sk.patterns().last().expect("Muster").0);
    assert_same_lines(sk, id, &pre);
    assert_same_lines(sk, id, &drawn);
}

fn assert_same_lines(sk: &Sketch, id: PatternId, pre: &[([i32; 2], Vec<Vec<DVec2>>)]) {
    let d = sk.pattern(id).unwrap();
    assert_eq!(pre.len(), d.instances.len());
    for (at, lines) in pre {
        let inst = d.instances.iter().find(|i| i.at == *at).expect("Exemplar");
        assert_eq!(lines.len(), inst.curves.len());
        for (line, &c) in lines.iter().zip(&inst.curves) {
            let made = sk.curve_polyline(c, PREVIEW_STEPS_PER_TURN).unwrap();
            assert_eq!(line.len(), made.len());
            for (a, b) in line.iter().zip(&made) {
                assert!(a.distance(*b) < 1e-9, "{at:?}: {a:?} gegen {b:?}");
            }
        }
    }
}

#[test]
fn the_preview_is_the_result_for_axes_symmetric_extent_lines_and_suppressed() {
    with_lang(&german(), || {
        // Achsen, 3 × 2.
        let (mut s, _) = sketch_with(&[], mixed);
        assert_preview_is_result(&mut s, |s| {
            type_field(s, FieldKind::PatternCount2, "2");
        });
        // Symmetrisch und Gesamtlänge, eine Lage abgewählt.
        let (mut s, _) = sketch_with(&[], mixed);
        assert_preview_is_result(&mut s, |s| {
            let p = plan_mut(s);
            p.kind = PatternKind::Rect {
                dirs: [PatternDir::SketchX, PatternDir::SketchY],
                extent: [true, false],
                symmetric: [true, true],
            };
            p.counts = [3, 2];
            p.suppressed = vec![[-2, 1]];
        });
        // Schräge Linie als Richtung, negativer Abstand.
        let (mut s, _) = sketch_with(&[], mixed);
        let l = slanted_line(&mut s);
        assert_preview_is_result(&mut s, |s| {
            let p = plan_mut(s);
            p.kind = PatternKind::Rect {
                dirs: [PatternDir::Line(l), PatternDir::SketchY],
                extent: [false; 2],
                symmetric: [false; 2],
            };
            p.values[0] = -12.5;
        });
    });
}

/// Fix-Runde Task 9: Auch beim Bearbeiten ist die Vorschau das Ergebnis.
#[test]
fn the_preview_is_the_result_when_editing() {
    with_lang(&german(), || {
        let (mut s, _) = sketch_with(&[], mixed);
        finish(&mut s).unwrap();
        let (id, _) = the_pattern(s.session_sketch().unwrap());
        let l = slanted_line(&mut s);
        {
            let (se, sk) = s.session_sketch_mut().unwrap();
            assert!(se.begin_pattern_edit(sk, id));
        }
        assert_preview_is_result(&mut s, |s| {
            type_field(s, FieldKind::PatternCount1, "4");
            type_field(s, FieldKind::PatternCount2, "2");
            let p = plan_mut(s);
            p.kind = PatternKind::Rect {
                dirs: [PatternDir::Line(l), PatternDir::Perpendicular],
                extent: [false, true],
                symmetric: [true, false],
            };
            p.suppressed = vec![[-1, 1], [2, 0]];
        });
    });
}

#[test]
fn direction_from_a_slanted_line_extent_and_symmetric_place_like_the_model() {
    with_lang(&german(), || {
        let (mut s, _) = sketch_with(&[], |sk| {
            sk.add_rectangle(v(0.0, 0.0), v(10.0, 5.0))
                .unwrap()
                .lines
                .to_vec()
        });
        let l = slanted_line(&mut s);
        let unit = v(30.0, 15.0).normalize();
        {
            let (se, sk) = s.session_sketch_mut().unwrap();
            se.pick_pattern_direction(0);
            assert!(se.pattern.picking_direction);
            // Ein Klick neben jede Linie setzt nichts und sagt es.
            se.click(sk, v(60.0, 60.0), PICK, false).unwrap();
            assert_eq!(
                se.message_text(),
                Some(tr!("sketch-pattern-direction-line"))
            );
            se.click(sk, v(15.0, -12.5), PICK, false).unwrap();
            assert!(!se.pattern.picking_direction);
        }
        let plan = s.sketch_session.as_ref().unwrap().pattern_now().unwrap();
        // Fix-Runde Task 9: Die zweite Richtung steht senkrecht zur ersten (Fusion).
        assert!(matches!(
            plan.kind,
            PatternKind::Rect { dirs: [PatternDir::Line(x), PatternDir::Perpendicular], .. } if x == l
        ));
        type_field(&mut s, FieldKind::PatternCount2, "2");
        let pre = session_preview(&s);
        assert!(first(&pre, [0, 1]).distance(unit.perp() * 7.5) < 1e-9);
        type_field(&mut s, FieldKind::PatternCount2, "1");
        // Abstand: Lage k bei k · 15 längs der Linie (der Rechteckpunkt (0, 0) zuerst).
        let pre = session_preview(&s);
        for (k, at) in [(1.0, [1, 0]), (2.0, [2, 0])] {
            assert!(first(&pre, at).distance(unit * 15.0 * k) < 1e-9);
        }
        // Gesamtlänge: 15 über zwei Schritte.
        if let PatternKind::Rect { extent, .. } = &mut plan_mut(&mut s).kind {
            extent[0] = true;
        }
        let pre = session_preview(&s);
        assert!(first(&pre, [2, 0]).distance(unit * 15.0) < 1e-9);
        assert!(first(&pre, [1, 0]).distance(unit * 7.5) < 1e-9);
        // Symmetrisch: je Seite drei Lagen samt Vorlage.
        if let PatternKind::Rect { symmetric, .. } = &mut plan_mut(&mut s).kind {
            symmetric[0] = true;
        }
        let pre = session_preview(&s);
        assert_eq!(ats(&pre), vec![[-2, 0], [-1, 0], [1, 0], [2, 0]]);
        assert!(first(&pre, [-2, 0]).distance(unit * -15.0) < 1e-9);
        // Dieselben Lagen wie das Modell (Task 3).
        let sk = s.session_sketch().unwrap();
        let now = s.sketch_session.as_ref().unwrap().pattern_now().unwrap();
        let model: Vec<[i32; 2]> = sk
            .pattern_transforms(&now)
            .unwrap()
            .into_iter()
            .map(|(at, _)| at)
            .collect();
        assert_eq!(model, ats(&pre));
    });
}

#[test]
fn count_through_a_formula_five_two_five_keeps_instance_ids_and_inches_store_one_in() {
    with_lang(&german(), || {
        let (mut s, feature) = sketch_with(&[("n", "5", ParamUnit::Number)], |sk| {
            sk.add_rectangle(v(0.0, 0.0), v(10.0, 5.0))
                .unwrap()
                .lines
                .to_vec()
        });
        finish(&mut s).unwrap();
        let (id, _) = the_pattern(s.session_sketch().unwrap());
        let (count, _) = value_id(s.session_sketch().unwrap(), id, PatternSlot::Count1);
        {
            let scope = s.document.param_scope();
            let (se, sf) = s.session_draft_mut().unwrap();
            se.set_dimension_input(sf, count, "n", &scope).unwrap();
            assert_eq!(sf.formulas.get(&count).map(String::as_str), Some("n"));
        }
        let first_ids = |sk: &Sketch| sk.pattern(id).unwrap().instances[0].clone();
        let one = first_ids(s.session_sketch().unwrap());
        assert_eq!(the_pattern(s.session_sketch().unwrap()).1.len(), 4);
        s.close_sketch();
        let n = s.document.parameters()[0].id;
        for (text, want) in [("2", 1), ("5", 4)] {
            set_param(&mut s, n, text, ParamUnit::Number);
            let sk = s.document.sketch_of(feature).unwrap();
            assert_eq!(the_pattern(sk).1.len(), want, "n = {text}");
            assert_eq!(first_ids(sk), one, "bleibende Exemplare behalten ihre Ids");
        }
        // Zoll: „1“ im Feld des Werkzeugs ⇒ 25,4 mm und Speichertext „1 in“, ebenso in der Liste.
        let (mut s, _) = sketch_with(&[], |sk| {
            sk.add_rectangle(v(0.0, 0.0), v(10.0, 5.0))
                .unwrap()
                .lines
                .to_vec()
        });
        with_unit(DisplayUnit::Zoll, || {
            type_field(&mut s, FieldKind::PatternSpacing1, "1");
            finish(&mut s).unwrap();
            let sk = s.session_sketch().unwrap().clone();
            let (id, _) = the_pattern(&sk);
            let (c, value) = value_id(&sk, id, PatternSlot::Spacing1);
            assert!((value - 25.4).abs() < 1e-12);
            let sf = s.sketch_draft.as_ref().unwrap();
            assert_eq!(sf.formulas.get(&c).map(String::as_str), Some("1 in"));
            let (c2, _) = value_id(&sk, id, PatternSlot::Spacing2);
            let scope = s.document.param_scope();
            let (se, sf) = s.session_draft_mut().unwrap();
            se.set_dimension_input(sf, c2, "1", &scope).unwrap();
            assert_eq!(sf.formulas.get(&c2).map(String::as_str), Some("1 in"));
        });
    });
}

#[test]
fn over_nine_hundred_shows_the_message_instead_of_a_preview_and_finish_refuses() {
    with_lang(&german(), || {
        let (mut s, _) = sketch_with(&[], |sk| {
            sk.add_rectangle(v(0.0, 0.0), v(10.0, 5.0))
                .unwrap()
                .lines
                .to_vec()
        });
        type_field(&mut s, FieldKind::PatternCount1, "31");
        type_field(&mut s, FieldKind::PatternCount2, "30");
        let (se, sk) = s.session_sketch_mut().unwrap();
        se.update_pattern_note(sk);
        assert!(se.pattern_preview(sk).is_none());
        let msg = se.message_text().unwrap();
        assert!(msg.contains("900"), "{msg}");
        let rev = sk.revision();
        assert!(finish(&mut s).is_err());
        assert_eq!(s.session_sketch().unwrap().revision(), rev);
        assert_eq!(s.sketch_session.as_ref().unwrap().tool, Tool::Pattern);
        // Wieder erlaubt: Die Meldung verschwindet.
        type_field(&mut s, FieldKind::PatternCount1, "30");
        let (se, sk) = s.session_sketch_mut().unwrap();
        se.update_pattern_note(sk);
        assert_eq!(se.message_text(), None);
        assert_eq!(se.pattern_preview(sk).unwrap().len(), 899);
        // Felder prüfen ihren Bereich.
        type_field(&mut s, FieldKind::PatternCount1, "2,5");
        type_field(&mut s, FieldKind::PatternSpacing1, "0");
        let se = s.sketch_session.as_ref().unwrap();
        let errors: Vec<_> = se
            .input
            .fields
            .iter()
            .filter_map(|f| f.error.clone())
            .collect();
        assert_eq!(errors.len(), 2, "{errors:?}");
    });
}

/// Fix-Runde Task 9 (B): Senkt ein Parameter die Anzahl, nennt die Meldung die entfallene
/// Abhängigkeit genau einmal, wie im Werkzeug.
#[test]
fn a_parameter_lowering_the_count_says_how_many_constraints_dropped() {
    with_lang(&german(), || {
        let (mut s, feature) = sketch_with(&[("n", "5", ParamUnit::Number)], |sk| {
            sk.add_rectangle(v(0.0, 0.0), v(10.0, 5.0))
                .unwrap()
                .lines
                .to_vec()
        });
        finish(&mut s).unwrap();
        let (id, _) = the_pattern(s.session_sketch().unwrap());
        let (count, _) = value_id(s.session_sketch().unwrap(), id, PatternSlot::Count1);
        {
            let scope = s.document.param_scope();
            let (se, sf) = s.session_draft_mut().unwrap();
            se.set_dimension_input(sf, count, "n", &scope).unwrap();
            let corner = sf.sketch.pattern(id).unwrap().instances[3].points[0];
            let f = sf.sketch.add_point(60.0, -10.0).unwrap();
            sf.sketch.add_constraint(K::Coincident(f, corner)).unwrap();
            sf.sketch.solve();
        }
        s.observe_sketch(false);
        s.close_sketch();
        s.message = None;
        let n = s.document.parameters()[0].id;
        set_param(&mut s, n, "2", ParamUnit::Number);
        assert_eq!(s.message, Some(tr!("sketch-pattern-dropped", n = 1)));
        assert_eq!(
            the_pattern(s.document.sketch_of(feature).unwrap()).1.len(),
            1
        );
        // Wieder mehr: keine Meldung.
        s.message = None;
        set_param(&mut s, n, "5", ParamUnit::Number);
        assert_eq!(s.message, None);
    });
}

/// Schlusskorrektur (Task 9, zurückgestellt): Die Zählung vorher kommt aus der Definition der
/// Skizze, nicht aus dem Ergebnis. So zählt auch eine Skizze, die gerade einen Fehler hat:
/// Anzahl 5 → 0 (Fehler) → 2 nennt die zwei entfallenen Abhängigkeiten.
#[test]
fn a_sketch_in_error_still_counts_its_dropped_constraints() {
    with_lang(&german(), || {
        let (mut s, feature) = sketch_with(&[("n", "5", ParamUnit::Number)], |sk| {
            sk.add_rectangle(v(0.0, 0.0), v(10.0, 5.0))
                .unwrap()
                .lines
                .to_vec()
        });
        finish(&mut s).unwrap();
        let (id, _) = the_pattern(s.session_sketch().unwrap());
        let (count, _) = value_id(s.session_sketch().unwrap(), id, PatternSlot::Count1);
        {
            let scope = s.document.param_scope();
            let (se, sf) = s.session_draft_mut().unwrap();
            se.set_dimension_input(sf, count, "n", &scope).unwrap();
            for k in [2, 3] {
                let corner = sf.sketch.pattern(id).unwrap().instances[k].points[0];
                let f = sf.sketch.add_point(60.0 + k as f64, -10.0).unwrap();
                sf.sketch.add_constraint(K::Coincident(f, corner)).unwrap();
            }
            sf.sketch.solve();
        }
        s.observe_sketch(false);
        s.close_sketch();
        let n = s.document.parameters()[0].id;
        set_param(&mut s, n, "0", ParamUnit::Number);
        assert!(
            s.document
                .features()
                .iter()
                .any(|f| f.id == feature && matches!(f.status, FeatureStatus::Error(_))),
            "Anzahl 0 ist ein Fehler"
        );
        s.message = None;
        set_param(&mut s, n, "2", ParamUnit::Number);
        assert_eq!(s.message, Some(tr!("sketch-pattern-dropped", n = 2)));
        assert_eq!(
            the_pattern(s.document.sketch_of(feature).unwrap()).1.len(),
            1
        );
    });
}

/// Fix-Runde Task 9: Eine Datei mit einer Schleife lädt über `convert.rs`, das Muster wird
/// aufgelöst, und die Meldung nach dem Öffnen nennt es.
#[test]
fn a_file_with_a_pattern_loop_loads_dissolved_and_says_so() {
    with_lang(&german(), || {
        let mut corner = None;
        let (mut s, _) = sketch_with(&[], |sk| {
            let r = sk.add_rectangle(v(0.0, 0.0), v(10.0, 6.0)).unwrap();
            corner = Some(r.points[0]);
            r.lines.to_vec()
        });
        type_field(&mut s, FieldKind::PatternCount2, "2");
        finish(&mut s).unwrap();
        let arc = {
            let (_, sk) = s.session_sketch_mut().unwrap();
            let (id, _) = the_pattern(sk);
            let m = sk.pattern(id).unwrap().clone();
            let k = m
                .source_points
                .iter()
                .position(|&p| Some(p) == corner)
                .unwrap();
            let at = |x: [i32; 2]| m.instances.iter().find(|i| i.at == x).unwrap().points[k];
            let free = sk.add_point(30.0, 30.0).unwrap();
            let arc = sk.add_arc(free, at([1, 0]), at([0, 1])).unwrap();
            sk.solve();
            arc
        };
        s.observe_sketch(false);
        s.close_sketch();
        let text = planora_project::encode_version(&s.document.snapshot()).unwrap();
        let mut j: serde_json::Value = serde_json::from_str(&text).unwrap();
        // Die Bogenmitte auf die Vorlagenecke: Vorlage und eigenes Exemplar hängen zusammen.
        j["features"][0]["art"]["skizze"]["kurven"][arc.index()]["art"]["mitte"] =
            corner.unwrap().index().into();
        let text = serde_json::to_string_pretty(&j).unwrap();
        let snap = planora_project::decode_version(&text, "x.json").unwrap();
        assert_eq!(snap.dissolved_patterns, 1);
        let loaded = AppState::from_snapshot(snap).unwrap();
        assert_eq!(
            loaded.message,
            Some(tr!("state-patterns-dissolved-on-load", n = 1))
        );
    });
}

#[test]
fn opening_a_file_with_a_dissolved_loop_says_so() {
    with_lang(&german(), || {
        let mut snap = AppState::new().document.snapshot();
        snap.dissolved_patterns = 2;
        let s = AppState::from_snapshot(snap).unwrap();
        assert_eq!(
            s.message,
            Some(tr!("state-patterns-dissolved-on-load", n = 2))
        );
    });
}

#[test]
fn pure_grip_layout_hit_and_motion() {
    let l = pattern_layout(
        pos2(100.0, 100.0),
        [Some(pos2(300.0, 100.0)), Some(pos2(100.0, 40.0))],
        None,
    );
    assert_eq!(l.origin, pos2(100.0, 100.0));
    // Der Pfeil reicht 60 Bildpunkte zurück von seiner Spitze.
    assert_eq!(
        pattern_hit(&l, pos2(270.0, 103.0)),
        Some(PatternGrip::Arrow(0))
    );
    assert_eq!(pattern_hit(&l, pos2(200.0, 100.0)), None);
    assert_eq!(
        pattern_hit(&l, pos2(100.0, 50.0)),
        Some(PatternGrip::Arrow(1))
    );
    assert_eq!(pattern_hit(&l, pos2(500.0, 500.0)), None);
    let d = PatternGripDrag {
        grip: PatternGrip::Arrow(0),
        press: v(10.0, 10.0),
        start: 15.0,
        divisor: 2.0,
        center: DVec2::ZERO,
        saved: None,
    };
    // Dichte Folge: Der Wert folgt der Projektion, quer zur Richtung nichts.
    for k in -200..=200 {
        let t = f64::from(k) * 0.25;
        let now = v(10.0, 10.0) + v(t, 3.0 * t);
        assert!((pattern_motion(&d, DVec2::X, now) - (15.0 + t / 2.0)).abs() < 1e-12);
        assert!((pattern_motion(&d, DVec2::Y, now) - (15.0 + 1.5 * t)).abs() < 1e-12);
    }
    assert_eq!(pattern_motion(&d, DVec2::X, v(f64::NAN, 0.0)), 15.0);
    let a = PatternGripDrag {
        grip: PatternGrip::Angle,
        press: v(10.0, 0.0),
        start: 1.0,
        divisor: 1.0,
        center: DVec2::ZERO,
        saved: None,
    };
    let quarter = pattern_motion(&a, DVec2::ZERO, v(0.0, 10.0));
    assert!((quarter - (1.0 + std::f64::consts::FRAC_PI_2)).abs() < 1e-12);
}

/// Release-Messung (Plan: unter 100 ms): 10 × 10 im Werkzeug angelegt, dann der Abstand im Feld
/// der Liste geändert, bis die Skizze gelöst ist.
#[test]
#[ignore = "Zeitmessung, nur mit --release --ignored"]
fn ten_by_ten_then_changing_the_spacing_solves_under_100_ms() {
    with_lang(&german(), || {
        let (mut s, _) = sketch_with(&[], |sk| {
            sk.add_rectangle(v(0.0, 0.0), v(10.0, 5.0))
                .unwrap()
                .lines
                .to_vec()
        });
        type_field(&mut s, FieldKind::PatternCount1, "10");
        type_field(&mut s, FieldKind::PatternCount2, "10");
        finish(&mut s).unwrap();
        let (id, at) = the_pattern(s.session_sketch().unwrap());
        assert_eq!(at.len(), 99);
        let (c, _) = value_id(s.session_sketch().unwrap(), id, PatternSlot::Spacing1);
        let scope = s.document.param_scope();
        let (se, sf) = s.session_draft_mut().unwrap();
        let t = std::time::Instant::now();
        se.set_dimension_input(sf, c, "17", &scope).unwrap();
        let ms = t.elapsed().as_secs_f64() * 1000.0;
        println!("10 × 10, Abstand ändern: {ms:.2} ms");
        let report = sf.sketch.clone().solve();
        assert!(report.conflicting.is_empty());
        assert!(ms < 100.0, "{ms} ms");
    });
}

/// Schlusskorrektur (Minor 8): Ein geöffnetes Muster, unverändert bestätigt, setzt keinen
/// Schrittnamen und keinen Schritt; eine echte Änderung heißt „Muster bearbeiten“.
#[test]
fn confirming_an_unchanged_pattern_edit_names_no_step() {
    with_lang(&german(), || {
        let mut rig = Rig::new(&[]);
        let ids = rig.sk().add_rectangle(v(0.0, 0.0), v(5.0, 2.5)).unwrap();
        let plan = rect_plan(ids.lines.to_vec(), [4, 1], [10.0, 5.0]);
        let id = rig.sk().add_pattern(&plan).unwrap();
        rig.sk().solve();
        rig.settle();
        let steps = rig.undo_len();
        let at = rig.exact(v(35.0, 2.5));
        rig.double_click(at);
        assert_eq!(
            rig.st().sketch_session.as_ref().unwrap().pattern.editing,
            Some(id)
        );
        {
            let (se, sf) = rig.st().session_draft_mut().unwrap();
            se.finish_pattern(sf).unwrap();
            assert_eq!(se.edit_label, None, "nichts geändert, kein Name");
        }
        rig.settle();
        assert_eq!(rig.undo_len(), steps);
        // Mit Änderung: der Name des Schritts.
        rig.double_click(at);
        rig.type_text("2");
        {
            let (se, sf) = rig.st().session_draft_mut().unwrap();
            se.finish_pattern(sf).unwrap();
            assert_eq!(se.edit_label, Some(planora_app::msg!("undo-pattern-edit")));
        }
        rig.settle();
        assert_eq!(rig.undo_len(), steps + 1);
    });
}

/// Schlusskorrektur (Task 12, Testlücke): Richtung 2 erst auf eine Linie, dann zurück auf
/// „Senkrecht zu 1“ (wie der Knopf): Die Vorschau ist das Ergebnis, das Anlegen ist ein Schritt,
/// und Richtung 2 folgt danach der ersten.
#[test]
fn direction_two_back_to_perpendicular_previews_the_result_in_one_step() {
    with_lang(&german(), || {
        let (mut s, _) = sketch_with(&[], mixed);
        let l = slanted_line(&mut s);
        let steps = s.sketch_history().unwrap().undo_len();
        assert_preview_is_result(&mut s, |s| {
            type_field(s, FieldKind::PatternCount2, "2");
            let se = s.sketch_session.as_mut().unwrap();
            se.set_pattern_direction(0, PatternDir::Line(l));
            se.set_pattern_direction(1, PatternDir::Line(l));
            se.set_pattern_direction(1, PatternDir::Perpendicular);
        });
        assert_eq!(s.sketch_history().unwrap().undo_len(), steps + 1);
        let (id, _) = the_pattern(s.session_sketch().unwrap());
        let PatternKind::Rect { dirs, .. } = s.session_sketch().unwrap().pattern(id).unwrap().kind
        else {
            panic!("rechteckig")
        };
        assert_eq!(dirs, [PatternDir::Line(l), PatternDir::Perpendicular]);
    });
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

fn assert_same_rects(got: &[Rect], want: &[Rect], what: &str) {
    assert_eq!(got.len(), want.len(), "{what}");
    for (g, w) in got.iter().zip(want) {
        assert!(
            g.center().distance(w.center()) < 0.5,
            "{what}: {g:?} statt {w:?}"
        );
    }
}

const RECT_KINDS: [FieldKind; 4] = [
    FieldKind::PatternCount1,
    FieldKind::PatternSpacing1,
    FieldKind::PatternCount2,
    FieldKind::PatternSpacing2,
];

/// Handtest Skizzen 5, Fix 3: Die Felder stehen fest an der Vorlage, auf der Seite, die den
/// Richtungen abgewandt ist. Anzahl 3 → 10, Abstand und Zeiger verschieben sie nicht.
#[test]
fn the_rect_fields_stay_put_beside_the_source_away_from_the_instances() {
    with_lang(&german(), || {
        let mut rig = Rig::new(&[]);
        let ids = rig.sk().add_rectangle(v(0.0, 0.0), v(10.0, 5.0)).unwrap();
        rig.settle();
        let (px, _) = rig.pixels();
        rig.select(lines(&ids));
        rig.action(Action::SketchPatternRect);
        let start = field_rects(&rig.settle(), &RECT_KINDS);
        // Links unterhalb der linken unteren Ecke: Die Richtungen gehen nach rechts und oben.
        let corner = px(v(0.0, 0.0));
        for r in &start {
            assert!(
                r.max.x < corner.x && r.min.y > corner.y,
                "{r:?} zu {corner:?}"
            );
        }
        // Der Zeiger wandert über die Vorschau: Die Felder bleiben.
        rig.hover(px(v(40.0, 20.0)));
        assert_same_rects(&field_rects(&rig.settle(), &RECT_KINDS), &start, "Zeiger");
        // 10 × 10 mit anderen Abständen, getippt wie im Handtest.
        rig.type_text("10");
        rig.press(Key::Tab);
        rig.type_text("25");
        rig.press(Key::Tab);
        rig.type_text("10");
        rig.press(Key::Tab);
        rig.type_text("12");
        rig.hover(px(v(60.0, 40.0)));
        let shot = rig.settle();
        assert_eq!(rig.now().unwrap().counts, [10, 10]);
        assert_eq!(rig.now().unwrap().values, [25.0, 12.0]);
        assert_same_rects(&field_rects(&shot, &RECT_KINDS), &start, "10 × 10");
        // Ein gezogener Abstand ebenso.
        rig.st()
            .sketch_session
            .as_mut()
            .unwrap()
            .pattern
            .plan
            .as_mut()
            .unwrap()
            .values = [40.0, 30.0];
        assert_same_rects(&field_rects(&rig.settle(), &RECT_KINDS), &start, "Abstand");
    });
}

/// Handtest Skizzen 5, Fix 3: Ein Klick auf ein Feld wählt kein Exemplar ab, auch wenn eines
/// darunter liegt (hier mit negativen Abständen dorthin gelegt).
#[test]
fn a_click_on_a_pattern_field_suppresses_no_instance_below_it() {
    with_lang(&german(), || {
        let mut rig = Rig::new(&[]);
        let ids = rig.sk().add_rectangle(v(0.0, 0.0), v(10.0, 5.0)).unwrap();
        rig.settle();
        rig.select(lines(&ids));
        rig.action(Action::SketchPatternRect);
        let p = field_rects(&rig.settle(), &[FieldKind::PatternCount2])[0].center();
        rig.press(Key::Escape);
        // Die Skizzenlage unter der Feldmitte.
        rig.use_tool(Tool::Dimension);
        let q = rig.at_pixel(p);
        rig.use_tool(Tool::Select);
        rig.select(lines(&ids));
        rig.action(Action::SketchPatternRect);
        assert_same_rects(
            &field_rects(&rig.settle(), &[FieldKind::PatternCount2]),
            &[Rect::from_center_size(p, Vec2::ZERO)],
            "wieder da",
        );
        // Exemplar [1, 1] mit seiner linken Kante genau unter der Feldmitte.
        {
            let plan = rig
                .st()
                .sketch_session
                .as_mut()
                .unwrap()
                .pattern
                .plan
                .as_mut()
                .unwrap();
            plan.counts = [2, 2];
            plan.values = [q.x, q.y - 2.5];
        }
        rig.settle();
        // Gegenprobe in der Sitzung: Dort träfe ein Klick das Exemplar.
        {
            let mut probe = rig.st().sketch_session.clone().unwrap();
            let mut sk = rig.sk().clone();
            probe.click(&mut sk, q, PICK, false).unwrap();
            assert_eq!(probe.pattern_now().unwrap().suppressed, vec![[1, 1]]);
        }
        rig.click(p);
        assert!(rig.now().unwrap().suppressed.is_empty(), "Klick aufs Feld");
        assert_eq!(rig.tool(), Tool::Pattern);
    });
}

/// Handtest Skizzen 5, Fix 3, rein: Der Anker der Felder hängt nicht an Anzahl und Abstand, steht
/// an der den Richtungen abgewandten Ecke, und die Feldlage hängt nur am Anker.
#[test]
fn pure_field_anchor_and_place_ignore_count_and_spacing() {
    use planora_app::sketch_inline::pattern_field_pos;
    use planora_app::sketch_pattern::rect_field_anchor;
    let (mut s, _) = sketch_with(&[], |sk| {
        sk.add_rectangle(v(0.0, 0.0), v(10.0, 5.0))
            .unwrap()
            .lines
            .to_vec()
    });
    let anchor = |s: &AppState| {
        let se = s.sketch_session.as_ref().unwrap();
        se.pattern_field_anchor(s.session_sketch().unwrap())
            .expect("Anker")
    };
    let start = anchor(&s);
    assert!(start.at.distance(v(0.0, 0.0)) < 1e-9, "{start:?}");
    assert!(start.away.x < 0.0 && start.away.y < 0.0);
    type_field(&mut s, FieldKind::PatternCount1, "10");
    type_field(&mut s, FieldKind::PatternCount2, "10");
    type_field(&mut s, FieldKind::PatternSpacing1, "40");
    assert_eq!(anchor(&s), start);
    plan_mut(&mut s).values = [3.0, 70.0];
    assert_eq!(anchor(&s), start);
    // Richtung 1 nach links: Die Ecke wechselt nach rechts unten.
    let left = rect_field_anchor(v(0.0, 0.0), v(10.0, 5.0), v(-1.0, 1.0));
    assert!(left.at.distance(v(10.0, 0.0)) < 1e-9 && left.away.x > 0.0 && left.away.y < 0.0);
    // Bildschirm: links unterhalb vom Anker, in der Fläche gehalten, Zeilen untereinander.
    let rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(W, H));
    let (at, away) = (pos2(800.0, 500.0), vec2(-1.0, 1.0));
    let p0 = pattern_field_pos(rect, at, away, 180.0, 0, 4);
    let p1 = pattern_field_pos(rect, at, away, 230.0, 1, 4);
    assert!(p0.x + 180.0 < at.x && p0.y > at.y, "{p0:?}");
    assert_eq!(p0.x, p1.x, "bündig, auch wenn eine Zeile breiter ist");
    assert!(p1.y > p0.y);
    let edge = pattern_field_pos(rect, pos2(5.0, H - 5.0), away, 180.0, 0, 4);
    assert!(edge.x >= 0.0 && edge.y + 4.0 * 30.0 <= H, "{edge:?}");
}
