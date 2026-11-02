//! Mittellinie als dritter Linienstil (Skizzen 3, Task 4): Schalter im Skizzenfeld und Aktion,
//! Zusammenspiel mit „Konstruktion umschalten“, Rückgängig, Darstellung als Strichpunkt und
//! Drehachse.

mod common;

use eframe::App as _;
use egui::epaint::{ClippedShape, Shape};
use egui::{Event, Modifiers, PointerButton, Pos2, Rect, Stroke, Vec2, pos2, vec2};
use planora_app::app::PlanoraApp;
use planora_app::commands::run_action;
use planora_app::create_dialog::{CreateDialog, axis_candidates};
use planora_app::keymap::{Action, Binding, KeyChord, SetResult};
use planora_app::sketch_state::{Item, Tool};
use planora_app::state::{AppState, BodyKind, StandardPlane};
use planora_app::workspace::Workspace;
use planora_app::{fonts, shell_panels, sketch_view, theme};
use planora_feature::SketchPlane;
use planora_i18n::{english, with_lang};
use planora_math::DVec2;
use planora_project::config::Config;
use planora_project::library::Library;
use planora_sketch::model::{CurveId, LineStyle};

const SIZE: Vec2 = vec2(1920.0, 1080.0);

// ---- Hilfen ---------------------------------------------------------------------------------

fn context() -> egui::Context {
    let c = egui::Context::default();
    c.set_fonts(fonts::definitions());
    theme::apply(&c, true);
    c
}

thread_local! {
    static TIME: std::cell::Cell<f64> = const { std::cell::Cell::new(0.0) };
}

fn input(events: Vec<Event>) -> egui::RawInput {
    let time = TIME.with(|t| {
        t.set(t.get() + 1.0 / 60.0);
        t.get()
    });
    egui::RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, SIZE)),
        time: Some(time),
        events,
        ..Default::default()
    }
}

fn ws() -> Workspace {
    let root = common::temp_root("planora-sketch-centerline");
    Workspace::new(Some(Library::open(&root).unwrap()), Config::default())
}

fn state(app: &mut PlanoraApp) -> &mut AppState {
    &mut app.ws_mut().active_tab_mut().state
}

fn sketching() -> PlanoraApp {
    let mut app = PlanoraApp::for_tests(ws());
    let s = state(&mut app);
    let id = s
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    s.open_sketch(id).unwrap();
    app
}

fn add_line(app: &mut PlanoraApp, y: f64) -> CurveId {
    let (_, sk) = state(app).session_sketch_mut().unwrap();
    let a = sk.add_point(-10.0, y).unwrap();
    let b = sk.add_point(10.0, y).unwrap();
    sk.add_line(a, b).unwrap()
}

fn select(app: &mut PlanoraApp, items: Vec<Item>) {
    state(app).session_sketch_mut().unwrap().0.selection = items;
}

fn style(app: &mut PlanoraApp, c: CurveId) -> LineStyle {
    let (_, sk) = state(app).session_sketch_mut().unwrap();
    sk.curve(c).unwrap().style()
}

fn texts(shapes: &[ClippedShape]) -> Vec<(String, Rect)> {
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
    let mut out = Vec::new();
    for c in shapes {
        walk(&c.shape, &mut out);
    }
    out
}

fn frame(c: &egui::Context, app: &mut PlanoraApp, events: Vec<Event>) -> Vec<(String, Rect)> {
    let mut f = eframe::Frame::_new_kittest();
    let out = c.run_ui(input(events), |ui| app.ui(ui, &mut f));
    let t = texts(&out.shapes);
    out.drop_without_applying_deltas();
    t
}

fn settle(c: &egui::Context, app: &mut PlanoraApp) -> Vec<(String, Rect)> {
    frame(c, app, vec![]);
    frame(c, app, vec![]);
    frame(c, app, vec![])
}

fn button(p: Pos2, pressed: bool) -> Event {
    Event::PointerButton {
        pos: p,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    }
}

fn click(c: &egui::Context, app: &mut PlanoraApp, p: Pos2) {
    frame(c, app, vec![Event::PointerMoved(p)]);
    frame(c, app, vec![Event::PointerMoved(p), button(p, true)]);
    frame(c, app, vec![button(p, false)]);
    frame(c, app, vec![]);
}

fn side(c: &egui::Context) -> Rect {
    shell_panels::field_rects(c)
        .into_iter()
        .find(|(n, _)| *n == "Skizze")
        .expect("Skizzenfeld")
        .1
}

fn label_rect(texts: &[(String, Rect)], area: Rect, label: &str) -> Rect {
    texts
        .iter()
        .find(|(t, r)| t == label && area.contains(r.center()))
        .unwrap_or_else(|| panic!("„{label}“ fehlt in {area:?}"))
        .1
}

// ---- Sitzung --------------------------------------------------------------------------------

#[test]
fn the_switch_needs_a_line_and_acts_on_every_selected_line() {
    let mut app = sketching();
    let l1 = add_line(&mut app, 0.0);
    let l2 = add_line(&mut app, 5.0);
    let circle = {
        let (_, sk) = state(&mut app).session_sketch_mut().unwrap();
        let m = sk.add_point(0.0, 20.0).unwrap();
        sk.add_circle(m, 3.0).unwrap()
    };
    let (se, sk) = state(&mut app).session_sketch_mut().unwrap();
    assert!(!se.can_toggle_centerline(sk), "nichts gewählt");
    se.selection = vec![Item::Curve(circle)];
    assert!(!se.can_toggle_centerline(sk), "ein Kreis ist keine Linie");
    se.toggle_centerline_if_possible(sk);
    assert_eq!(sk.curve(circle).unwrap().style(), LineStyle::Normal);

    se.selection = vec![Item::Curve(l1), Item::Curve(l2), Item::Curve(circle)];
    assert!(se.can_toggle_centerline(sk));
    se.toggle_centerline_if_possible(sk);
    assert_eq!(sk.curve(l1).unwrap().style(), LineStyle::Centerline);
    assert_eq!(sk.curve(l2).unwrap().style(), LineStyle::Centerline);
    assert_eq!(sk.curve(circle).unwrap().style(), LineStyle::Normal);
    // zweimal: zurück zu normal
    se.toggle_centerline_if_possible(sk);
    assert_eq!(sk.curve(l1).unwrap().style(), LineStyle::Normal);
    assert!(!sk.curve(l1).unwrap().construction);
}

#[test]
fn toggling_construction_on_a_centerline_makes_it_normal() {
    let mut app = sketching();
    let l = add_line(&mut app, 0.0);
    select(&mut app, vec![Item::Curve(l)]);
    run_action(app.ws_mut(), Action::SketchCenterline);
    assert_eq!(style(&mut app, l), LineStyle::Centerline);
    run_action(app.ws_mut(), Action::SketchConstruction);
    assert_eq!(style(&mut app, l), LineStyle::Normal);
    // Konstruktion bleibt Konstruktion und wird mit der Mittellinie eine Mittellinie.
    run_action(app.ws_mut(), Action::SketchConstruction);
    assert_eq!(style(&mut app, l), LineStyle::Construction);
    run_action(app.ws_mut(), Action::SketchCenterline);
    assert_eq!(style(&mut app, l), LineStyle::Centerline);
}

#[test]
fn the_action_has_no_default_key_but_can_be_bound() {
    let mut w = ws();
    assert!(w.keymap.bindings(Action::SketchCenterline).is_empty());
    assert_eq!(Action::SketchCenterline.id(), "skizze.mittellinie");
    let mut km = w.keymap.clone();
    let chord = KeyChord {
        key: egui::Key::J,
        ctrl: false,
        shift: false,
        alt: false,
    };
    assert_eq!(
        km.set(Action::SketchCenterline, 0, Binding::Key(chord)),
        SetResult::Ok
    );
    w.keymap = km;
    assert_eq!(w.keymap.bindings(Action::SketchCenterline).len(), 1);
}

#[test]
fn one_undo_step_named_toggle_centerline() {
    let mut w = ws();
    let id = {
        let s = &mut w.active_tab_mut().state;
        let id = s
            .add_sketch(SketchPlane::Standard(StandardPlane::XY))
            .unwrap();
        s.open_sketch(id).unwrap();
        id
    };
    let _ = id;
    w.observe_history(false);
    let line = {
        let (se, sk) = w.active_tab_mut().state.session_sketch_mut().unwrap();
        se.set_tool(sk, Tool::Line);
        se.click(sk, DVec2::new(0.0, 0.0), 0.5, false).unwrap();
        se.click(sk, DVec2::new(10.0, 0.0), 0.5, false).unwrap();
        se.set_tool(sk, Tool::Select);
        sk.curves().next().unwrap().0
    };
    w.observe_history(false);
    let n = w.active_tab().state.sketch_history().unwrap().undo_len();
    w.active_tab_mut()
        .state
        .session_sketch_mut()
        .unwrap()
        .0
        .selection = vec![Item::Curve(line)];
    run_action(&mut w, Action::SketchCenterline);
    w.observe_history(false);
    assert_eq!(
        w.active_tab().state.sketch_history().unwrap().undo_len(),
        n + 1
    );
    assert_eq!(
        w.active_tab().undo_label(),
        Some("Mittellinie umschalten".to_string())
    );
    w.active_tab_mut().undo().unwrap();
    let sk = w.active_tab().state.session_sketch().unwrap();
    assert_eq!(sk.curve(line).unwrap().style(), LineStyle::Normal);
}

// ---- Schalter im Skizzenfeld bei 1920×1080 ----------------------------------------------------

#[test]
fn the_panel_switch_is_locked_without_a_line_and_toggles_the_selection() {
    let c = context();
    let mut app = sketching();
    let line = add_line(&mut app, -10.0);
    let t = settle(&c, &mut app);
    let area = side(&c);
    let at = label_rect(&t, area, "Mittellinie").center();

    click(&c, &mut app, at);
    assert_eq!(style(&mut app, line), LineStyle::Normal, "gesperrt");

    select(&mut app, vec![Item::Curve(line)]);
    settle(&c, &mut app);
    click(&c, &mut app, at);
    assert_eq!(style(&mut app, line), LineStyle::Centerline, "an");
    click(&c, &mut app, at);
    assert_eq!(style(&mut app, line), LineStyle::Normal, "zurück");
}

#[test]
fn the_english_switch_fits_the_field_width() {
    with_lang(&english(), || {
        let c = context();
        let mut app = sketching();
        let line = add_line(&mut app, -10.0);
        select(&mut app, vec![Item::Curve(line)]);
        let t = settle(&c, &mut app);
        let area = side(&c);
        assert!((area.width() - 250.0).abs() < 1.0, "{area:?}");
        let r = label_rect(&t, area, "Centerline");
        assert!(
            r.min.x >= area.min.x && r.max.x <= area.max.x,
            "{r:?} liegt nicht in {area:?}"
        );
        click(&c, &mut app, r.center());
        assert_eq!(style(&mut app, line), LineStyle::Centerline);
    });
}

// ---- Darstellung ------------------------------------------------------------------------------

fn segment_lengths(shapes: &[Shape]) -> Vec<f32> {
    shapes
        .iter()
        .map(|s| match s {
            Shape::LineSegment { points, .. } => points[0].distance(points[1]),
            other => panic!("kein Strich: {other:?}"),
        })
        .collect()
}

#[test]
fn a_centerline_alternates_long_and_short_dashes() {
    let path = vec![pos2(0.0, 0.0), pos2(200.0, 0.0)];
    let stroke = Stroke::new(1.0, egui::Color32::WHITE);
    let shapes = sketch_view::style_shapes(path.clone(), stroke, LineStyle::Centerline);
    let lengths = segment_lengths(&shapes);
    assert!(lengths.len() >= 8, "{lengths:?}");
    for (i, l) in lengths[..lengths.len() - 1].iter().enumerate() {
        let want = if i % 2 == 0 { 12.0 } else { 2.0 };
        assert!((l - want).abs() < 1e-3, "Strich {i}: {l} statt {want}");
    }
}

#[test]
fn normal_is_one_polyline_and_construction_is_dashed_as_before() {
    let path = vec![pos2(0.0, 0.0), pos2(100.0, 0.0), pos2(100.0, 50.0)];
    let stroke = Stroke::new(1.0, egui::Color32::WHITE);
    let normal = sketch_view::style_shapes(path.clone(), stroke, LineStyle::Normal);
    assert_eq!(normal.len(), 1);
    assert!(matches!(normal[0], Shape::Path(_)));
    let construction = sketch_view::style_shapes(path.clone(), stroke, LineStyle::Construction);
    let before = Shape::dashed_line(&path, stroke, 6.0, 4.0);
    assert_eq!(construction.len(), before.len());
    assert_eq!(construction, before);
}

// ---- Drehachse --------------------------------------------------------------------------------

#[test]
fn a_centerline_is_listed_as_its_own_axis_and_revolving_around_it_works() {
    let mut s = AppState::new();
    let id = s
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    s.open_sketch(id).unwrap();
    {
        let sk = s.session_sketch_mut().unwrap().1;
        sk.add_rectangle(DVec2::new(0.0, 0.0), DVec2::new(5.0, 10.0))
            .unwrap();
        let a = sk.add_point(0.0, -2.0).unwrap();
        let b = sk.add_point(0.0, 12.0).unwrap();
        let axis = sk.add_line(a, b).unwrap();
        sk.set_centerline(axis, true).unwrap();
        let c = sk.add_point(20.0, 0.0).unwrap();
        let d = sk.add_point(20.0, 3.0).unwrap();
        let k = sk.add_line(c, d).unwrap();
        sk.set_construction(k, true).unwrap();
        let names: Vec<String> = axis_candidates(sk).into_iter().map(|(_, n)| n).collect();
        assert!(names[0].starts_with("Konstruktionslinie 1"), "{names:?}");
        assert!(names[1].starts_with("Mittellinie 1"), "{names:?}");
        assert!(names[2].starts_with("Linie 1"), "{names:?}");
    }
    s.close_sketch();
    let d = CreateDialog::revolve_sketch(&s, id);
    if let CreateDialog::RevolveSketch { axis, .. } = &d {
        assert!(axis.is_some());
    }
    // Nur die Mittellinie als Achse wählen.
    let CreateDialog::RevolveSketch {
        sketch, profiles, ..
    } = d
    else {
        panic!("kein Drehdialog")
    };
    s.open_sketch(id).unwrap();
    let centre = {
        let sk = s.session_sketch_mut().unwrap().1;
        axis_candidates(sk)
            .into_iter()
            .find(|(_, n)| n.starts_with("Mittellinie"))
            .unwrap()
            .0
    };
    s.close_sketch();
    let d = CreateDialog::RevolveSketch {
        sketch,
        profiles,
        axis: Some(centre),
    };
    assert!(d.create(&mut s), "{:?}", s.message);
    assert_eq!(s.bodies.len(), 1);
    assert_eq!(s.bodies[0].kind, BodyKind::Revolution);
}
