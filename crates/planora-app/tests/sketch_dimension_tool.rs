//! Skizzen 5, Task 8: das Werkzeug Skizzenbemaßung (Taste D). Die ganze App läuft ohne Fenster
//! bei 1920×1080; die reinen Teile (Zeigerregel, Vorschau gegen Klick) gehen über die Sitzung.

mod common;

use eframe::App as _;
use egui::epaint::{ClippedShape, Shape};
use egui::{Event, Key, Modifiers, PointerButton, Pos2, Rect, Vec2, pos2};
use planora_app::app::PlanoraApp;
use planora_app::sketch_panel;
use planora_app::sketch_state::Tool;
use planora_app::state::{AppState, StandardPlane};
use planora_app::workspace::Workspace;
use planora_app::{fonts, sketch_view, theme};
use planora_feature::{ParamEdit, ParamUnit, SketchPlane};
use planora_i18n::{DisplayUnit, german, tr, with_lang, with_unit};
use planora_math::DVec2;
use planora_project::config::Config;
use planora_project::library::Library;
use planora_sketch::constraint::ConstraintKind as K;
use planora_sketch::dimension::{DimMode, DimPlan};
use planora_sketch::model::{PointId, Sketch};

const W: f32 = 1920.0;
const H: f32 = 1080.0;
const PICK: f64 = 0.5;

fn v(x: f64, y: f64) -> DVec2 {
    DVec2::new(x, y)
}

// ---- Die ganze App ----------------------------------------------------------------------------

struct Shot {
    texts: Vec<String>,
}

fn walk(shape: &Shape, out: &mut Vec<String>) {
    match shape {
        Shape::Vec(v) => v.iter().for_each(|x| walk(x, out)),
        Shape::Text(t) => out.push(t.galley.text().to_string()),
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

    fn has(&self, exact: &str) -> bool {
        self.texts.iter().any(|t| t == exact)
    }
}

struct Rig {
    ctx: egui::Context,
    app: PlanoraApp,
    time: f64,
}

impl Rig {
    /// App mit offener, leerer Skizze auf XY; vorher die Parameter `params` (Name, Formel).
    fn new(params: &[(&str, &str)]) -> Rig {
        let ctx = egui::Context::default();
        ctx.set_fonts(fonts::definitions());
        theme::apply(&ctx, true);
        PlanoraApp::setup_context(&ctx);
        let root = common::temp_root("planora-sketch-dimension-tool");
        let mut w = Workspace::new(Some(Library::open(&root).unwrap()), Config::default());
        w.config_path = root.join("config.toml");
        let s = &mut w.active_tab_mut().state;
        for (name, formula) in params {
            let id = s.add_parameter().unwrap();
            s.edit_parameter(
                id,
                ParamEdit {
                    name: (*name).into(),
                    formula: (*formula).into(),
                    unit: ParamUnit::Length,
                    comment: String::new(),
                },
            )
            .unwrap();
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

    fn click_with(&mut self, p: Pos2, button: PointerButton) -> Shot {
        let ev = |pressed| Event::PointerButton {
            pos: p,
            button,
            pressed,
            modifiers: Modifiers::NONE,
        };
        self.frame(vec![Event::PointerMoved(p)]);
        self.frame(vec![ev(true)]);
        self.frame(vec![ev(false)]);
        self.settle()
    }

    fn click(&mut self, p: Pos2) -> Shot {
        self.click_with(p, PointerButton::Primary)
    }

    /// Lage in der Skizze unter dem Bildpunkt `p` (die Bemaßung zeigt die Rohlage).
    fn at_pixel(&mut self, p: Pos2) -> DVec2 {
        self.hover(p);
        let se = self.st().sketch_session.as_ref().unwrap();
        se.draw_cursor.as_ref().expect("Zeiger").raw
    }

    /// Bildpunkt einer Skizzenlage, aus zwei Zeigerlagen (die Ansicht von oben ist linear).
    fn pixels(&mut self) -> impl Fn(DVec2) -> Pos2 + use<> {
        let (a, b) = (pos2(900.0, 500.0), pos2(1100.0, 600.0));
        let (p, q) = (self.at_pixel(a), self.at_pixel(b));
        move |s: DVec2| {
            let tx = (s.x - p.x) / (q.x - p.x);
            let ty = (s.y - p.y) / (q.y - p.y);
            pos2(a.x + (b.x - a.x) * tx as f32, a.y + (b.y - a.y) * ty as f32)
        }
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

    fn tool(&mut self) -> Tool {
        self.st().sketch_session.as_ref().unwrap().tool
    }

    fn picks(&mut self) -> usize {
        self.st().sketch_session.as_ref().unwrap().dim.picks.len()
    }

    fn preview(&mut self) -> Option<DimPlan> {
        let s = self.st();
        let se = s.sketch_session.as_ref().unwrap();
        se.dimension_preview(s.session_sketch().unwrap())
    }

    fn undo_len(&mut self) -> usize {
        self.st().sketch_history().unwrap().undo_len()
    }

    fn live(&mut self) -> Vec<K> {
        self.sk()
            .constraints()
            .map(|(_, c)| c.kind.clone())
            .collect()
    }

    fn editor_open(&self) -> bool {
        sketch_panel::dimension_editor_open(&self.ctx)
    }
}

fn xy(sk: &Sketch, p: PointId) -> DVec2 {
    sk.point_xy(p).unwrap()
}

#[test]
fn d_two_points_above_the_span_give_forty_horizontal_and_one_undo_takes_it_all() {
    with_lang(&german(), || {
        let mut rig = Rig::new(&[]);
        let a = rig.sk().add_point(0.0, 0.0).unwrap();
        let b = rig.sk().add_point(40.0, 0.0).unwrap();
        rig.settle();
        rig.press(Key::D);
        assert_eq!(rig.tool(), Tool::Dimension);
        let px = rig.pixels();
        let steps = rig.undo_len();
        rig.click(px(v(0.0, 0.0)));
        rig.click(px(v(40.0, 0.0)));
        assert_eq!(rig.picks(), 2);
        // Wählen ist kein Schritt.
        assert_eq!(rig.undo_len(), steps);
        let shot = rig.hover(px(v(20.0, 15.0)));
        let plan = rig.preview().expect("Vorschau");
        assert_eq!(plan.mode, DimMode::Horizontal);
        assert_eq!(plan.kind, K::DistanceX(a, b, 40.0));
        assert!(shot.has("40,00 mm"), "Vorschau am Zeiger: {:?}", shot.texts);
        // Klick setzt: Das Feld zeigt den Wert, angelegt ist noch nichts.
        let shot = rig.click(px(v(20.0, 15.0)));
        assert!(rig.editor_open());
        assert!(shot.has("40,00"), "{:?}", shot.texts);
        assert!(shot.has(&format!("{}:", tr!("sketch-constraint-distance-x"))));
        assert!(rig.live().is_empty());
        assert_eq!(rig.undo_len(), steps);
        rig.type_text("55");
        rig.press(Key::Enter);
        assert!(!rig.editor_open());
        let live = rig.live();
        assert!(
            matches!(live.as_slice(), [K::DistanceX(p, q, w)] if (*p, *q) == (a, b) && *w == 55.0),
            "{live:?}"
        );
        let sk = rig.sk().clone();
        assert!((xy(&sk, b).x - xy(&sk, a).x - 55.0).abs() < 1e-9);
        assert_eq!(rig.tool(), Tool::Dimension, "das Werkzeug bleibt");
        // Ein Rückgängig nimmt Maß, Wert und Lage.
        assert_eq!(rig.undo_len(), steps + 1);
        assert!(rig.st().undo_sketch());
        assert!(rig.live().is_empty());
        let sk = rig.sk().clone();
        assert!((xy(&sk, b).x - xy(&sk, a).x - 40.0).abs() < 1e-9);

        // Esc im Feld: nichts angelegt, die Skizze unverändert.
        rig.settle();
        rig.click(px(v(0.0, 0.0)));
        rig.click(px(v(40.0, 0.0)));
        rig.click(px(v(20.0, 15.0)));
        assert!(rig.editor_open());
        let rev = rig.sk().revision();
        let steps = rig.undo_len();
        rig.press(Key::Escape);
        assert!(!rig.editor_open());
        assert!(rig.live().is_empty());
        assert_eq!(rig.sk().revision(), rev);
        assert_eq!(rig.undo_len(), steps);
        assert_eq!(
            rig.tool(),
            Tool::Dimension,
            "Esc im Feld beendet nur das Maß"
        );
        assert!(rig.preview().is_none());
    });
}

#[test]
fn point_and_centerline_preview_diameter_and_tab_makes_it_the_distance() {
    with_lang(&german(), || {
        let mut rig = Rig::new(&[]);
        let l0 = rig.sk().add_point(-50.0, 0.0).unwrap();
        let l1 = rig.sk().add_point(50.0, 0.0).unwrap();
        let l = rig.sk().add_line(l0, l1).unwrap();
        rig.sk().set_centerline(l, true).unwrap();
        let p = rig.sk().add_point(10.0, 15.0).unwrap();
        rig.settle();
        rig.press(Key::D);
        let px = rig.pixels();
        rig.click(px(v(10.0, 15.0)));
        rig.click(px(v(-30.0, 0.0)));
        let shot = rig.hover(px(v(30.0, 25.0)));
        let plan = rig.preview().expect("Vorschau");
        assert_eq!(plan.mode, DimMode::CenterlineDiameter);
        assert_eq!(plan.kind, K::CenterlineDiameter(p, l, 30.0));
        assert!(shot.has("Ø 30,00 mm"), "{:?}", shot.texts);
        let shot = rig.click(px(v(30.0, 25.0)));
        assert!(shot.has("30,00"), "{:?}", shot.texts);
        assert!(shot.has(&sketch_panel::toggle_label()), "Knopf Ø/R am Feld");
        // Tab im Feld: Abstand statt Durchmesser.
        let shot = rig.press(Key::Tab);
        assert!(rig.editor_open());
        assert!(shot.has("15,00"), "{:?}", shot.texts);
        assert!(shot.has("15,00 mm"), "die Vorschau folgt: {:?}", shot.texts);
        rig.press(Key::Enter);
        assert_eq!(rig.live(), vec![K::DistancePointLine(p, l, 15.0)]);
    });
}

#[test]
fn tab_while_picking_toggles_the_preview_too() {
    with_lang(&german(), || {
        let mut rig = Rig::new(&[]);
        let m = rig.sk().add_point(0.0, 0.0).unwrap();
        let c = rig.sk().add_circle(m, 15.0).unwrap();
        rig.settle();
        rig.press(Key::D);
        let px = rig.pixels();
        rig.click(px(v(15.0, 0.0)));
        let shot = rig.hover(px(v(30.0, 30.0)));
        assert!(shot.has("Ø 30,00 mm"), "{:?}", shot.texts);
        let shot = rig.press(Key::Tab);
        assert_eq!(rig.preview().unwrap().kind, K::Radius(c, 15.0));
        assert!(shot.has("R 15,00 mm"), "{:?}", shot.texts);
        rig.click(px(v(30.0, 30.0)));
        rig.press(Key::Enter);
        assert_eq!(rig.live(), vec![K::Radius(c, 15.0)]);
    });
}

#[test]
fn the_toggle_button_on_a_centerline_diameter_halves_value_and_formula_in_one_step() {
    with_lang(&german(), || {
        let mut rig = Rig::new(&[("d", "30")]);
        let l0 = rig.sk().add_point(-50.0, 0.0).unwrap();
        let l1 = rig.sk().add_point(50.0, 0.0).unwrap();
        let l = rig.sk().add_line(l0, l1).unwrap();
        rig.sk().set_centerline(l, true).unwrap();
        let p = rig.sk().add_point(10.0, 15.0).unwrap();
        let id = rig
            .sk()
            .add_constraint(K::CenterlineDiameter(p, l, 30.0))
            .unwrap();
        rig.st()
            .sketch_draft
            .as_mut()
            .unwrap()
            .formulas
            .insert(id, "d".into());
        rig.settle();
        let steps = rig.undo_len();
        let label = sketch_panel::toggle_label();
        // Der Knopf in der Liste: über den Namen finden (die Ansicht hat keinen gleichen Text).
        let at = {
            let shot = rig.settle();
            assert!(shot.has(&label), "{:?}", shot.texts);
            find(&mut rig, &label)
        };
        rig.click(at);
        let draft = rig.st().sketch_draft.clone().unwrap();
        assert_eq!(
            draft.sketch.constraint(id).unwrap().kind,
            K::DistancePointLine(p, l, 15.0)
        );
        assert_eq!(draft.formulas.get(&id).map(String::as_str), Some("d * 0.5"));
        assert_eq!(rig.undo_len(), steps + 1);
        assert!(rig.st().undo_sketch());
        let draft = rig.st().sketch_draft.clone().unwrap();
        assert_eq!(
            draft.sketch.constraint(id).unwrap().kind,
            K::CenterlineDiameter(p, l, 30.0)
        );
        assert_eq!(draft.formulas.get(&id).map(String::as_str), Some("d"));
        // Über die Sitzung hin und zurück: die Formel wird wieder `d` (Faktor zusammengefasst).
        let s = rig.st();
        let (se, sf) = s.session_draft_mut().unwrap();
        se.toggle_radius_diameter(sf, id).unwrap();
        se.toggle_radius_diameter(sf, id).unwrap();
        assert_eq!(
            sf.sketch.constraint(id).unwrap().kind,
            K::CenterlineDiameter(p, l, 30.0)
        );
        assert_eq!(sf.formulas.get(&id).map(String::as_str), Some("d"));
    });
}

/// Mitte des Texts `exact` im letzten Bild.
fn find(rig: &mut Rig, exact: &str) -> Pos2 {
    rig.time += 1.0 / 60.0;
    let raw = egui::RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(W, H))),
        time: Some(rig.time),
        ..Default::default()
    };
    let mut f = eframe::Frame::_new_kittest();
    let app = &mut rig.app;
    let out = rig.ctx.run_ui(raw, |ui| app.ui(ui, &mut f));
    let mut found = None;
    fn look(shape: &Shape, exact: &str, found: &mut Option<Pos2>) {
        match shape {
            Shape::Vec(v) => v.iter().for_each(|x| look(x, exact, found)),
            Shape::Text(t) if t.galley.text() == exact => {
                *found = Some(Rect::from_min_size(t.pos, t.galley.size()).center());
            }
            _ => {}
        }
    }
    for c in &out.shapes {
        look(&c.shape, exact, &mut found);
    }
    out.drop_without_applying_deltas();
    found.unwrap_or_else(|| panic!("„{exact}“ fehlt"))
}

#[test]
fn tool_change_right_click_and_undo_while_picking_leave_nothing() {
    with_lang(&german(), || {
        let mut rig = Rig::new(&[]);
        rig.sk().add_point(0.0, 0.0).unwrap();
        rig.sk().add_point(40.0, 0.0).unwrap();
        rig.settle();
        rig.press(Key::D);
        let px = rig.pixels();
        let steps = rig.undo_len();
        let rev = rig.sk().revision();
        // Werkzeugwechsel.
        rig.click(px(v(0.0, 0.0)));
        assert_eq!(rig.picks(), 1);
        rig.press(Key::L);
        assert_eq!(rig.tool(), Tool::Line);
        rig.press(Key::Escape);
        rig.press(Key::D);
        assert_eq!(rig.picks(), 0);
        assert!(rig.preview().is_none());
        // Rechtsklick.
        rig.click(px(v(0.0, 0.0)));
        rig.click(px(v(40.0, 0.0)));
        assert_eq!(rig.picks(), 2);
        rig.click_with(px(v(20.0, 20.0)), PointerButton::Secondary);
        assert_eq!(rig.picks(), 0);
        assert!(rig.preview().is_none());
        assert_eq!(rig.undo_len(), steps);
        assert_eq!(rig.sk().revision(), rev);
        // Gesetzt, Feld offen, dann ein anderes Werkzeug: das Feld schließt, nichts angelegt.
        rig.click(px(v(0.0, 0.0)));
        rig.click(px(v(40.0, 0.0)));
        rig.click(px(v(20.0, 20.0)));
        assert!(rig.editor_open());
        let (se, sk) = rig.st().session_sketch_mut().unwrap();
        se.choose_tool(sk, Tool::Select);
        rig.settle();
        assert!(!rig.editor_open());
        assert!(rig.live().is_empty());
        assert_eq!(rig.undo_len(), steps);
        // Strg+Z mitten im Wählen: die Wahl entfällt.
        rig.press(Key::D);
        rig.click(px(v(0.0, 0.0)));
        assert_eq!(rig.picks(), 1);
        rig.key(Key::Z, Modifiers::COMMAND);
        assert_eq!(rig.picks(), 0);
        assert!(rig.live().is_empty());
    });
}

// ---- Über die Sitzung ---------------------------------------------------------------------------

/// Offene, leere Skizze auf XY mit den Parametern `params`; `build` legt den Inhalt an. Das
/// Werkzeug ist die Skizzenbemaßung.
fn sketch_with(params: &[(&str, &str)], build: impl FnOnce(&mut Sketch)) -> AppState {
    let mut s = AppState::new();
    for (name, formula) in params {
        let id = s.add_parameter().unwrap();
        s.edit_parameter(
            id,
            ParamEdit {
                name: (*name).into(),
                formula: (*formula).into(),
                unit: ParamUnit::Length,
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
        build(sk);
        se.last_report = Some(sk.solve());
        se.set_tool(sk, Tool::Dimension);
    }
    s.observe_sketch(false);
    s
}

fn pick(s: &mut AppState, at: DVec2) {
    let (se, sk) = s.session_sketch_mut().unwrap();
    se.click_dimension(sk, at, PICK);
}

fn point_to(s: &mut AppState, at: DVec2) -> Option<DimPlan> {
    let se = s.sketch_session.as_mut().unwrap();
    se.cursor = Some(at);
    se.draw_cursor = None;
    let se = s.sketch_session.as_ref().unwrap();
    se.dimension_preview(s.session_sketch().unwrap())
}

fn planned(s: &AppState) -> Option<DimPlan> {
    s.sketch_session.as_ref().unwrap().dim.plan.clone()
}

fn apply(
    s: &mut AppState,
    kind: K,
    text: &str,
) -> Result<planora_sketch::constraint::ConstraintId, String> {
    let scope = s.document.param_scope();
    let (se, sf) = s.session_draft_mut().unwrap();
    let r = se.apply_planned(sf, kind, text, &scope);
    s.observe_sketch(false);
    r
}

fn message(s: &AppState) -> Option<String> {
    s.sketch_session.as_ref().unwrap().message_text()
}

#[test]
fn circle_gives_diameter_arc_radius_and_two_slanted_lines_an_angle_in_degrees() {
    with_lang(&german(), || {
        let ctx = egui::Context::default();
        ctx.set_fonts(fonts::definitions());
        let mut ids = (None, None, None, None);
        let mut s = sketch_with(&[], |sk| {
            let m = sk.add_point(0.0, 0.0).unwrap();
            ids.0 = Some(sk.add_circle(m, 15.0).unwrap());
            let (am, a0, a1) = (
                sk.add_point(100.0, 0.0).unwrap(),
                sk.add_point(110.0, 0.0).unwrap(),
                sk.add_point(100.0, 10.0).unwrap(),
            );
            ids.1 = Some(sk.add_arc(am, a0, a1).unwrap());
            let (p, q) = (
                sk.add_point(0.0, 50.0).unwrap(),
                sk.add_point(20.0, 50.0).unwrap(),
            );
            ids.2 = Some(sk.add_line(p, q).unwrap());
            let t = 30f64.to_radians();
            let (r, u) = (
                sk.add_point(0.0, 60.0).unwrap(),
                sk.add_point(20.0 * t.cos(), 60.0 + 20.0 * t.sin()).unwrap(),
            );
            ids.3 = Some(sk.add_line(r, u).unwrap());
        });
        let (circle, arc, l1, l2) = (
            ids.0.unwrap(),
            ids.1.unwrap(),
            ids.2.unwrap(),
            ids.3.unwrap(),
        );
        let text = |s: &AppState, plan: &DimPlan| {
            sketch_view::dimension_text(&ctx, s.session_sketch().unwrap(), plan)
        };

        pick(&mut s, v(15.0, 0.0));
        let plan = point_to(&mut s, v(30.0, 30.0)).unwrap();
        assert_eq!(
            (plan.mode, plan.kind.clone()),
            (DimMode::Diameter, K::Diameter(circle, 30.0))
        );
        assert_eq!(text(&s, &plan), "Ø 30,00 mm");
        pick(&mut s, v(30.0, 30.0));
        assert_eq!(planned(&s), Some(plan));
        s.sketch_session.as_mut().unwrap().cancel_planned();

        let h = std::f64::consts::FRAC_1_SQRT_2 * 10.0;
        pick(&mut s, v(100.0 + h, h));
        let plan = point_to(&mut s, v(130.0, 30.0)).unwrap();
        assert_eq!(plan.mode, DimMode::Radius);
        assert!(matches!(plan.kind, K::Radius(c, r) if c == arc && (r - 10.0).abs() < 1e-9));
        assert_eq!(text(&s, &plan), "R 10,00 mm");
        pick(&mut s, v(130.0, 30.0));
        s.sketch_session.as_mut().unwrap().cancel_planned();

        pick(&mut s, v(10.0, 50.0));
        let t = 30f64.to_radians();
        pick(&mut s, v(10.0 * t.cos(), 60.0 + 10.0 * t.sin()));
        let plan = point_to(&mut s, v(30.0, 70.0)).unwrap();
        assert_eq!(plan.mode, DimMode::Angle);
        assert!(
            matches!(plan.kind, K::Angle(a, b, w) if (a, b) == (l1, l2) && (w - t).abs() < 1e-9),
            "{:?}",
            plan.kind
        );
        assert_eq!(text(&s, &plan), "30,0°");
        pick(&mut s, v(30.0, 70.0));
        // Getippt wird in Grad.
        let kind = planned(&s).unwrap().kind;
        let id = apply(&mut s, kind, "45").unwrap();
        let sk = s.session_sketch().unwrap();
        assert!(
            matches!(sk.constraint(id).unwrap().kind, K::Angle(_, _, w) if (w - 45f64.to_radians()).abs() < 1e-12)
        );
        let measured = sk.measure(&sk.constraint(id).unwrap().kind).unwrap();
        assert!((measured - 45f64.to_radians()).abs() < 1e-9);
        assert!(planned(&s).is_none());
    });
}

#[test]
fn inches_store_one_in_and_a_formula_stays_on_the_dimension() {
    with_lang(&german(), || {
        let mut pts = Vec::new();
        let mut s = sketch_with(&[("d", "30")], |sk| {
            pts.push(sk.add_point(0.0, 0.0).unwrap());
            pts.push(sk.add_point(40.0, 20.0).unwrap());
        });
        let (a, b) = (pts[0], pts[1]);
        with_unit(DisplayUnit::Zoll, || {
            let id = apply(&mut s, K::DistanceX(a, b, 40.0), "1").unwrap();
            let sf = s.sketch_draft.as_ref().unwrap();
            assert!(
                matches!(sf.sketch.constraint(id).unwrap().kind, K::DistanceX(_, _, w) if (w - 25.4).abs() < 1e-12)
            );
            assert_eq!(sf.formulas.get(&id).map(String::as_str), Some("1 in"));
            let dx = xy(&sf.sketch, b).x - xy(&sf.sketch, a).x;
            assert!((dx - 25.4).abs() < 1e-9);
        });
        let id = apply(&mut s, K::DistanceY(a, b, 20.0), "d").unwrap();
        let sf = s.sketch_draft.as_ref().unwrap();
        assert_eq!(sf.formulas.get(&id).map(String::as_str), Some("d"));
        assert!(!sf.sketch.constraint(id).unwrap().driven);
        let dy = xy(&sf.sketch, b).y - xy(&sf.sketch, a).y;
        assert!((dy - 30.0).abs() < 1e-9);
    });
}

#[test]
fn a_dimension_that_already_holds_becomes_driven_and_says_so() {
    with_lang(&german(), || {
        let mut pts = Vec::new();
        let mut s = sketch_with(&[("d", "40")], |sk| {
            let a = sk.add_point(0.0, 0.0).unwrap();
            let b = sk.add_point(40.0, 10.0).unwrap();
            sk.add_constraint(K::Fixed(a, 0.0, 0.0)).unwrap();
            sk.add_constraint(K::Fixed(b, 40.0, 10.0)).unwrap();
            pts.extend([a, b]);
        });
        let (a, b) = (pts[0], pts[1]);
        pick(&mut s, v(0.0, 0.0));
        pick(&mut s, v(40.0, 10.0));
        let plan = point_to(&mut s, v(20.0, 30.0)).unwrap();
        assert_eq!(plan.kind, K::DistanceX(a, b, 40.0));
        pick(&mut s, v(20.0, 30.0));
        let rev = s.session_sketch().unwrap().revision();
        let id = apply(&mut s, plan.kind, "d").unwrap();
        let sf = s.sketch_draft.as_ref().unwrap();
        let c = sf.sketch.constraint(id).unwrap();
        assert!(c.driven);
        assert!(!sf.formulas.contains_key(&id), "anzeigend: keine Formel");
        assert_eq!(message(&s), Some(tr!("sketch-became-driven")));
        assert_ne!(sf.sketch.revision(), rev);
        let report = sf.sketch.clone().solve();
        assert!(report.conflicting.is_empty());
        // Ein Rückgängig nimmt es.
        assert!(s.undo_sketch());
        assert_eq!(s.session_sketch().unwrap().constraints().count(), 2);
    });
}

#[test]
fn the_kind_changes_exactly_at_the_edge_of_the_span_and_preview_matches_the_click() {
    with_lang(&german(), || {
        let mut pts = Vec::new();
        let mut s = sketch_with(&[], |sk| {
            pts.push(sk.add_point(0.0, 0.0).unwrap());
            pts.push(sk.add_point(40.0, 30.0).unwrap());
        });
        pick(&mut s, v(0.0, 0.0));
        pick(&mut s, v(40.0, 30.0));
        // Dicht über die rechte Kante der x-Spanne (oberhalb der y-Spanne), 0,001 mm Schritt.
        let mut flips = Vec::new();
        let mut last = None;
        for i in 0..=20_000 {
            let x = 30.0 + i as f64 * 0.001;
            let plan = point_to(&mut s, v(x, 50.0)).unwrap();
            let horizontal = plan.mode == DimMode::Horizontal;
            assert_eq!(horizontal, x <= 40.0, "x = {x}");
            if let Some(m) = last
                && m != plan.mode
            {
                flips.push(x);
            }
            last = Some(plan.mode);
            // Was die Vorschau zeigt, setzt der Klick.
            if i % 997 == 0 {
                let mut t = s.sketch_session.clone().unwrap();
                t.click_dimension(s.session_sketch().unwrap(), v(x, 50.0), PICK);
                let set = t.dim.plan.unwrap();
                assert_eq!(set.kind, plan.kind);
                assert_eq!(set.mode, plan.mode);
                assert!(set.anchor.distance(plan.anchor) < 1e-9);
            }
        }
        assert_eq!(flips.len(), 1, "{flips:?}");
        assert!(flips[0] > 40.0 && flips[0] < 40.0 + 0.0011);
        // Senkrecht an der Kante der y-Spanne ebenso.
        for (y, vertical) in [(30.0, true), (30.001, false), (0.0, true), (-0.001, false)] {
            assert_eq!(
                point_to(&mut s, v(50.0, y)).unwrap().mode == DimMode::Vertical,
                vertical,
                "y = {y}"
            );
        }
    });
}

#[test]
fn stacked_points_never_give_a_horizontal_or_zero_dimension() {
    with_lang(&german(), || {
        let mut pts = Vec::new();
        let mut s = sketch_with(&[], |sk| {
            pts.push(sk.add_point(0.0, 0.0).unwrap());
            pts.push(sk.add_point(0.0, 30.0).unwrap());
        });
        pick(&mut s, v(0.0, 0.0));
        pick(&mut s, v(0.0, 30.0));
        for i in -40..=40 {
            for j in -20..=60 {
                let at = v(i as f64 * 0.5, j as f64);
                let plan = point_to(&mut s, at).unwrap();
                assert_ne!(plan.mode, DimMode::Horizontal, "{at:?}");
                let value = planora_app::sketch_view::stored_value(&plan.kind).unwrap();
                assert!(value > 1e-6, "{at:?}: {value}");
            }
        }
        // dx knapp über der Toleranz: waagerecht geht, nie mit Wert 0.
        let tol = s.session_sketch().unwrap().tolerance().linear;
        let mut pts = Vec::new();
        let mut s = sketch_with(&[], |sk| {
            pts.push(sk.add_point(0.0, 0.0).unwrap());
            pts.push(sk.add_point(3.0 * tol, 30.0).unwrap());
        });
        pick(&mut s, v(0.0, 0.0));
        pick(&mut s, v(3.0 * tol, 30.0));
        let plan = point_to(&mut s, v(1.5 * tol, 50.0)).unwrap();
        assert_eq!(plan.mode, DimMode::Horizontal);
        assert!(planora_app::sketch_view::stored_value(&plan.kind).unwrap() > tol);
    });
}

#[test]
fn a_second_object_without_a_dimension_does_not_count() {
    with_lang(&german(), || {
        let mut s = sketch_with(&[], |sk| {
            let p = sk.add_point(0.0, 0.0).unwrap();
            let m = sk.add_point(50.0, 0.0).unwrap();
            sk.add_ellipse(m, DVec2::new(10.0, 0.0), 5.0).unwrap();
            let _ = p;
        });
        pick(&mut s, v(0.0, 0.0));
        pick(&mut s, v(60.0, 0.0));
        assert_eq!(s.sketch_session.as_ref().unwrap().dim.picks.len(), 1);
        assert_eq!(message(&s), Some(tr!("sketch-dimension-none")));
    });
}

// ---- Korrekturrunde 1 ---------------------------------------------------------------------------

#[test]
fn toggling_twice_gives_the_formula_back_byte_for_byte() {
    with_lang(&german(), || {
        let mut ids = (None, None);
        let mut s = sketch_with(&[("d", "30")], |sk| {
            let m = sk.add_point(0.0, 0.0).unwrap();
            let c = sk.add_circle(m, 15.0).unwrap();
            ids = (
                Some(c),
                Some(sk.add_constraint(K::Diameter(c, 30.0)).unwrap()),
            );
        });
        let (c, id) = (ids.0.unwrap(), ids.1.unwrap());
        for formula in [
            "1 in",
            "d + 5",
            "2 * d",
            "(d)",
            "d / 2",
            "d",
            "d * 2",
            "(d + 5) * 0.5",
        ] {
            s.sketch_draft
                .as_mut()
                .unwrap()
                .formulas
                .insert(id, formula.to_string());
            let mut seen = Vec::new();
            for _ in 0..4 {
                let (se, sf) = s.session_draft_mut().unwrap();
                se.toggle_radius_diameter(sf, id).unwrap();
                let text = sf.formulas.get(&id).cloned().unwrap();
                assert!(planora_feature::expr::parse(&text).is_ok(), "{text}");
                seen.push(text);
            }
            assert_eq!(seen[1], formula, "zweimal: {seen:?}");
            assert_eq!(seen[3], formula, "viermal: {seen:?}");
            assert_eq!(seen[0], seen[2], "{seen:?}");
            assert_eq!(
                s.session_sketch().unwrap().constraint(id).unwrap().kind,
                K::Diameter(c, 30.0)
            );
        }
    });
}

#[test]
fn a_formula_that_cannot_be_rewritten_refuses_the_toggle_with_a_message() {
    with_lang(&german(), || {
        let mut ids = (None, None);
        let mut s = sketch_with(&[], |sk| {
            let m = sk.add_point(0.0, 0.0).unwrap();
            let c = sk.add_circle(m, 15.0).unwrap();
            ids = (
                Some(c),
                Some(sk.add_constraint(K::Radius(c, 15.0)).unwrap()),
            );
        });
        let (c, id) = (ids.0.unwrap(), ids.1.unwrap());
        // Ein Text, der schon nicht lesbar ist (etwa aus einer älteren Datei).
        s.sketch_draft
            .as_mut()
            .unwrap()
            .formulas
            .insert(id, "d +".into());
        let rev = s.session_sketch().unwrap().revision();
        let (se, sf) = s.session_draft_mut().unwrap();
        assert!(se.toggle_radius_diameter(sf, id).is_err());
        assert_eq!(sf.formulas.get(&id).map(String::as_str), Some("d +"));
        assert_eq!(sf.sketch.constraint(id).unwrap().kind, K::Radius(c, 15.0));
        assert_eq!(sf.sketch.revision(), rev);
        assert_eq!(message(&s), Some(tr!("sketch-toggle-formula-failed")));
    });
}

#[test]
fn a_click_beside_the_open_field_creates_the_dimension_in_one_step_and_picks_anew() {
    with_lang(&german(), || {
        let mut rig = Rig::new(&[]);
        let a = rig.sk().add_point(0.0, 0.0).unwrap();
        let b = rig.sk().add_point(40.0, 0.0).unwrap();
        // a fest: Der Klick daneben trifft a auch nach dem Anlegen.
        rig.sk().add_constraint(K::Fixed(a, 0.0, 0.0)).unwrap();
        let c = rig.sk().add_point(-40.0, 10.0).unwrap();
        let d = rig.sk().add_point(-20.0, 10.0).unwrap();
        rig.settle();
        rig.press(Key::D);
        let px = rig.pixels();
        let steps = rig.undo_len();
        rig.click(px(v(0.0, 0.0)));
        rig.click(px(v(40.0, 0.0)));
        rig.click(px(v(20.0, 15.0)));
        assert!(rig.editor_open());
        rig.type_text("55");
        // Klick auf den Punkt a: legt an und wählt a neu.
        rig.click(px(v(0.0, 0.0)));
        let live = rig.live();
        assert!(
            matches!(live.as_slice(), [_, K::DistanceX(p, q, w)] if (*p, *q) == (a, b) && *w == 55.0),
            "{live:?}"
        );
        assert!(!rig.editor_open());
        assert_eq!(rig.picks(), 1);
        assert_eq!(rig.undo_len(), steps + 1);
        // Unverändert: der gemessene Wert, genau, auch mit einem Klick ins Leere.
        rig.click_with(px(v(20.0, 20.0)), PointerButton::Secondary);
        assert_eq!(rig.picks(), 0);
        rig.click(px(v(-40.0, 10.0)));
        rig.click(px(v(-20.0, 10.0)));
        rig.click(px(v(-30.0, 18.0)));
        assert!(rig.editor_open());
        rig.click(px(v(-30.0, 25.0)));
        assert!(!rig.editor_open());
        assert_eq!(rig.live()[2], K::DistanceX(c, d, 20.0), "{:?}", rig.live());
        assert_eq!(rig.undo_len(), steps + 2);
        assert!(rig.st().undo_sketch());
        assert_eq!(rig.live().len(), 2);
    });
}

#[test]
fn tab_keeps_a_typed_number_in_the_planned_field() {
    with_lang(&german(), || {
        let mut rig = Rig::new(&[]);
        let l0 = rig.sk().add_point(-50.0, 0.0).unwrap();
        let l1 = rig.sk().add_point(50.0, 0.0).unwrap();
        let l = rig.sk().add_line(l0, l1).unwrap();
        rig.sk().set_centerline(l, true).unwrap();
        let p = rig.sk().add_point(10.0, 15.0).unwrap();
        rig.settle();
        rig.press(Key::D);
        let px = rig.pixels();
        rig.click(px(v(10.0, 15.0)));
        rig.click(px(v(-30.0, 0.0)));
        rig.click(px(v(30.0, 25.0)));
        rig.type_text("40");
        let shot = rig.press(Key::Tab);
        assert!(shot.has("40"), "{:?}", shot.texts);
        assert_eq!(
            rig.st()
                .sketch_session
                .as_ref()
                .unwrap()
                .dim
                .plan
                .as_ref()
                .unwrap()
                .kind,
            K::DistancePointLine(p, l, 15.0)
        );
        rig.press(Key::Enter);
        assert_eq!(rig.live(), vec![K::DistancePointLine(p, l, 40.0)]);
    });
}

#[test]
fn tab_in_an_open_list_field_is_not_taken_by_the_dimension_preview() {
    with_lang(&german(), || {
        let mut rig = Rig::new(&[]);
        let m = rig.sk().add_point(0.0, 0.0).unwrap();
        let c = rig.sk().add_circle(m, 15.0).unwrap();
        let q = rig.sk().add_point(60.0, 0.0).unwrap();
        let id = rig.sk().add_constraint(K::Distance(m, q, 60.0)).unwrap();
        rig.settle();
        rig.press(Key::D);
        let px = rig.pixels();
        rig.click(px(v(15.0, 0.0)));
        rig.hover(px(v(30.0, 30.0)));
        assert_eq!(rig.preview().unwrap().kind, K::Diameter(c, 30.0));
        let ctx = rig.ctx.clone();
        sketch_panel::open_existing_dimension_editor(&ctx, rig.st(), id);
        rig.settle();
        assert!(
            rig.ctx.memory(|mem| mem.focused()).is_some(),
            "Feld hat die Tastatur"
        );
        rig.press(Key::Tab);
        assert!(!rig.st().sketch_session.as_ref().unwrap().dim.toggled);
        assert_eq!(rig.preview().unwrap().kind, K::Diameter(c, 30.0));
    });
}
