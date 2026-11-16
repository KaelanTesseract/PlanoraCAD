//! Krümmungskamm in der Oberfläche (Skizzen 11a, Task 14): Ein/Aus mit Dialog, Rechtsklickmenü,
//! Felder im Skizzenfeld und Zeichnen in der Skizze und in der 3D-Ansicht.

mod common;

use eframe::App as _;
use egui::epaint::{ClippedShape, Shape};
use egui::{Event, Modifiers, PointerButton, Pos2, Rect, Vec2, pos2, vec2};
use planora_app::app::PlanoraApp;
use planora_app::commands::run_action;
use planora_app::curvature_dialog::{CurvatureDialog, comb_shapes};
use planora_app::detach::DIALOG_KEYS;
use planora_app::keymap::Action;
use planora_app::sketch_state::{CombToggle, Item, Tool};
use planora_app::state::{AppState, StandardPlane};
use planora_app::surface_project_dialog::DialogOutcome;
use planora_app::workspace::Workspace;
use planora_app::{fonts, shell_panels, sketch_view, theme};
use planora_feature::{FeatureId, SketchPlane};
use planora_math::{DVec2, DVec3, Frame, Tolerance};
use planora_project::config::Config;
use planora_project::library::Library;
use planora_render::ViewFace;
use planora_sketch::comb::CombSettings;
use planora_sketch::model::{CurveId, Sketch};

const SIZE: Vec2 = vec2(1920.0, 1080.0);
const SCREEN: Rect = Rect {
    min: pos2(0.0, 0.0),
    max: pos2(1920.0, 1080.0),
};

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
    let root = common::temp_root("planora-sketch-comb");
    Workspace::new(Some(Library::open(&root).unwrap()), Config::default())
}

/// Offene Skizze auf XY mit einem Bogen (Mitte 0|0, Radius 10, Viertelkreis) und einer Linie.
fn open_sketch(w: &mut Workspace) -> (CurveId, CurveId) {
    let s = &mut w.active_tab_mut().state;
    let id = s
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    s.open_sketch(id).unwrap();
    let (_, sk) = s.session_sketch_mut().unwrap();
    let both = arc_and_line(sk);
    w.observe_history(false);
    both
}

fn arc_and_line(sk: &mut Sketch) -> (CurveId, CurveId) {
    let m = sk.add_point(0.0, 0.0).unwrap();
    let a = sk.add_point(10.0, 0.0).unwrap();
    let b = sk.add_point(0.0, 10.0).unwrap();
    let arc = sk.add_arc(m, a, b).unwrap();
    let c = sk.add_point(20.0, 0.0).unwrap();
    let d = sk.add_point(30.0, 0.0).unwrap();
    let line = sk.add_line(c, d).unwrap();
    (arc, line)
}

fn state(w: &mut Workspace) -> &mut AppState {
    &mut w.active_tab_mut().state
}

fn select(w: &mut Workspace, items: Vec<Item>) {
    let se = &mut state(w).session_sketch_mut().unwrap().0;
    se.tool = Tool::Select;
    se.selection = items;
}

fn comb_of(w: &mut Workspace, c: CurveId) -> Option<CombSettings> {
    state(w).session_sketch().unwrap().comb(c)
}

fn undo_len(w: &Workspace) -> usize {
    w.active_tab().state.sketch_history().unwrap().undo_len()
}

/// Zahl der Striche (Einzelstrich oder Linienzug) in der Farbe `color`.
fn strokes_in(shapes: &[Shape], color: egui::Color32) -> usize {
    shapes
        .iter()
        .map(|s| match s {
            Shape::LineSegment { stroke, .. } if stroke.color == color => 1,
            Shape::Path(p) if p.stroke.color == egui::epaint::ColorMode::Solid(color) => 1,
            Shape::Vec(v) => strokes_in(v, color),
            _ => 0,
        })
        .sum()
}

// ---- Ein/Aus und Dialog -----------------------------------------------------------------------

#[test]
fn toggle_on_opens_dialog_ok_sets_comb() {
    let mut w = ws();
    let (arc, _) = open_sketch(&mut w);
    select(&mut w, vec![Item::Curve(arc)]);
    let n = undo_len(&w);
    let (se, sk) = state(&mut w).session_sketch_mut().unwrap();
    let CombToggle::Dialog(curves) = se.toggle_curvature(sk) else {
        panic!("erwartet: Dialog");
    };
    assert_eq!(curves, vec![arc]);
    // Das Öffnen ändert nichts.
    assert_eq!(comb_of(&mut w, arc), None);
    w.observe_history(false);
    assert_eq!(undo_len(&w), n, "kein Schritt vor OK");

    let d = CurvatureDialog::new(curves);
    assert_eq!(
        (d.density, d.scale),
        (
            CombSettings::default().density,
            CombSettings::default().scale
        )
    );
    assert!(d.apply(state(&mut w)));
    w.observe_history(false);
    assert_eq!(comb_of(&mut w, arc), Some(CombSettings::default()));
    assert_eq!(undo_len(&w), n + 1, "ein Schritt");
    w.active_tab_mut().undo().unwrap();
    assert_eq!(comb_of(&mut w, arc), None, "Rückgängig nimmt ihn weg");
}

#[test]
fn toggle_off_without_dialog() {
    let mut w = ws();
    let (arc, _) = open_sketch(&mut w);
    state(&mut w)
        .session_sketch_mut()
        .unwrap()
        .1
        .set_comb(arc, Some(CombSettings::default()))
        .unwrap();
    w.observe_history(false);
    select(&mut w, vec![Item::Curve(arc)]);
    let n = undo_len(&w);
    let (se, sk) = state(&mut w).session_sketch_mut().unwrap();
    assert!(matches!(se.toggle_curvature(sk), CombToggle::Off(1)));
    assert_eq!(comb_of(&mut w, arc), None);
    w.observe_history(false);
    assert_eq!(undo_len(&w), n + 1);
}

#[test]
fn dialog_cancel_leaves_nothing() {
    let mut w = ws();
    let (arc, _) = open_sketch(&mut w);
    select(&mut w, vec![Item::Curve(arc)]);
    let n = undo_len(&w);
    let mut d = CurvatureDialog::new(vec![arc]);
    d.density = 50;
    d.scale = 3.0;
    // Abbrechen, ✕ und Esc: Ergebnis „abgebrochen“, die Skizze bleibt unberührt.
    assert_eq!(
        d.finish(state(&mut w), false, true, true),
        DialogOutcome::Cancelled
    );
    assert_eq!(
        d.finish(state(&mut w), false, false, false),
        DialogOutcome::Cancelled
    );
    assert_eq!(
        d.finish(state(&mut w), false, false, true),
        DialogOutcome::Open
    );
    w.observe_history(false);
    assert_eq!(comb_of(&mut w, arc), None);
    assert_eq!(undo_len(&w), n);
    // OK dagegen setzt die Werte, begrenzt auf die Bereiche.
    d.density = 500;
    d.scale = 1.0e6;
    assert_eq!(
        d.finish(state(&mut w), true, false, true),
        DialogOutcome::Applied
    );
    let got = comb_of(&mut w, arc).unwrap();
    assert_eq!(got.density, 100);
    assert_eq!(got.scale, 100.0);
}

#[test]
fn lines_only_refused() {
    let mut w = ws();
    let (_, line) = open_sketch(&mut w);
    let (se, sk) = state(&mut w).session_sketch_mut().unwrap();
    assert!(!se.can_toggle_curvature(sk), "nichts gewählt");
    assert!(matches!(se.toggle_curvature(sk), CombToggle::Refused(_)));
    select(&mut w, vec![Item::Curve(line)]);
    let (se, sk) = state(&mut w).session_sketch_mut().unwrap();
    assert!(!se.can_toggle_curvature(sk), "Linie: Menüpunkt gedämpft");
    assert!(matches!(se.toggle_curvature(sk), CombToggle::Refused(_)));
    assert_eq!(sk.comb(line), None);
}

#[test]
fn action_opens_the_dialog_and_has_no_default_key() {
    let mut w = ws();
    let (arc, _) = open_sketch(&mut w);
    assert!(w.keymap.bindings(Action::SketchToggleCurvature).is_empty());
    assert_eq!(
        Action::SketchToggleCurvature.id(),
        "skizze.kruemmungsanzeige"
    );
    select(&mut w, vec![Item::Curve(arc)]);
    run_action(&mut w, Action::SketchToggleCurvature);
    assert!(state(&mut w).curvature_dialog.is_some());
    assert_eq!(comb_of(&mut w, arc), None);
}

// ---- Felder im Skizzenfeld ----------------------------------------------------------------------

#[test]
fn panel_edits_density_one_step() {
    let mut w = ws();
    let (arc, _) = open_sketch(&mut w);
    select(&mut w, vec![Item::Curve(arc)]);
    let (se, sk) = state(&mut w).session_sketch_mut().unwrap();
    assert_eq!(se.single_comb(sk), None, "ohne Kamm keine Felder");
    se.set_comb_selected(sk, true);
    w.observe_history(false);
    let n = undo_len(&w);
    let (se, sk) = state(&mut w).session_sketch_mut().unwrap();
    assert_eq!(se.single_comb(sk), Some((arc, CombSettings::default())));
    se.edit_comb(sk, arc, Some(35), None);
    w.observe_history(false);
    assert_eq!(comb_of(&mut w, arc).unwrap().density, 35);
    assert_eq!(comb_of(&mut w, arc).unwrap().scale, 1.0);
    assert_eq!(undo_len(&w), n + 1);
    // Werte außerhalb der Bereiche werden begrenzt.
    let (se, sk) = state(&mut w).session_sketch_mut().unwrap();
    se.edit_comb(sk, arc, Some(1), Some(0.0));
    let got = comb_of(&mut w, arc).unwrap();
    assert_eq!((got.density, got.scale), (2, 0.01));
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

fn has(texts: &[(String, Rect)], label: &str) -> bool {
    texts.iter().any(|(t, _)| t == label)
}

#[test]
fn the_panel_switch_sets_the_comb_and_shows_the_fields() {
    let c = context();
    let mut app = PlanoraApp::for_tests(ws());
    let (arc, line) = open_sketch(app.ws_mut());
    select(app.ws_mut(), vec![Item::Curve(arc)]);
    let t = settle(&c, &mut app);
    assert!(has(&t, "Krümmungskamm"), "Schalter fehlt: {t:?}");
    assert!(!has(&t, "Dichte"), "Felder erst mit Kamm");
    let area = shell_panels::field_rects(&c)
        .into_iter()
        .find(|(n, _)| *n == "Skizze")
        .expect("Skizzenfeld")
        .1;
    let at = t
        .iter()
        .find(|(s, r)| s == "Krümmungskamm" && area.contains(r.center()))
        .unwrap()
        .1
        .center();
    click(&c, &mut app, at);
    assert_eq!(comb_of(app.ws_mut(), arc), Some(CombSettings::default()));
    let t = settle(&c, &mut app);
    assert!(has(&t, "Dichte") && has(&t, "Maßstab"), "{t:?}");
    // Nur eine Linie gewählt: weder Schalter noch Felder.
    select(app.ws_mut(), vec![Item::Curve(line)]);
    let t = settle(&c, &mut app);
    assert!(!has(&t, "Krümmungskamm") && !has(&t, "Dichte"), "{t:?}");
}

// ---- Zeichnen ---------------------------------------------------------------------------------

fn finished(comb: bool, hidden: bool) -> (AppState, FeatureId) {
    let mut s = AppState::new();
    let id = s
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    s.open_sketch(id).unwrap();
    {
        let sk = s.session_sketch_mut().unwrap().1;
        let (arc, _) = arc_and_line(sk);
        if comb {
            sk.set_comb(arc, Some(CombSettings::default())).unwrap();
        }
    }
    s.close_sketch();
    s.camera.set_view(ViewFace::Top);
    s.camera.target = glam::Vec3::new(10.0, 5.0, 0.0);
    s.camera.distance = 80.0;
    if hidden {
        s.set_feature_hidden(id, true);
    }
    (s, id)
}

#[test]
fn comb_drawn_outside_sketch_mode() {
    let (s, _) = finished(true, false);
    let col = theme::sketch_colors(s.dark);
    let shapes = sketch_view::finished_sketch_shapes(&s, SCREEN);
    // 20 Zähne und die Hüllkurve
    let n = strokes_in(&shapes, col.comb);
    assert!(n >= 20, "{n}");
    let (s, _) = finished(false, false);
    let shapes = sketch_view::finished_sketch_shapes(&s, SCREEN);
    assert_eq!(strokes_in(&shapes, col.comb), 0, "ohne Kamm nichts");
}

#[test]
fn hidden_sketch_draws_no_comb() {
    let (s, _) = finished(true, true);
    let col = theme::sketch_colors(s.dark);
    let shapes = sketch_view::finished_sketch_shapes(&s, SCREEN);
    assert_eq!(strokes_in(&shapes, col.comb), 0);
}

#[test]
fn comb_shapes_teeth_and_envelope() {
    let mut sk = Sketch::new(Frame::WORLD, "Skizze", Tolerance::default());
    let (arc, _) = arc_and_line(&mut sk);
    let to_screen = |p: DVec3| pos2(p.x as f32 * 10.0, -p.y as f32 * 10.0);
    assert!(comb_shapes(&sk, to_screen, 10.0, egui::Color32::RED).is_empty());
    sk.set_comb(
        arc,
        Some(CombSettings {
            density: 7,
            scale: 1.0,
        }),
    )
    .unwrap();
    let shapes = comb_shapes(&sk, to_screen, 10.0, egui::Color32::RED);
    let teeth = shapes
        .iter()
        .filter(|s| matches!(s, Shape::LineSegment { .. }))
        .count();
    assert_eq!(teeth, 7, "ein Zahn je Stelle");
    assert_eq!(
        shapes
            .iter()
            .filter(|s| matches!(s, Shape::Path(_)))
            .count(),
        1,
        "eine Hüllkurve"
    );
}

// ---- Rechtsklick und Dialog -------------------------------------------------------------------

#[test]
fn right_click_menu_only_in_select_with_curve() {
    let mut w = ws();
    let (arc, _) = open_sketch(&mut w);
    // Auswahlwerkzeug mit gewählter Kurve: Menü.
    select(&mut w, vec![Item::Curve(arc)]);
    let (se, sk) = state(&mut w).session_sketch_mut().unwrap();
    se.secondary_click(sk, (100.0, 200.0));
    assert_eq!(se.curve_menu, Some((100.0, 200.0)));
    // Ohne gewählte Kurve: kein Menü.
    se.curve_menu = None;
    se.selection.clear();
    se.secondary_click(sk, (1.0, 2.0));
    assert_eq!(se.curve_menu, None);
    // Anderes Werkzeug: Rechtsklick bricht ab wie bisher (das Linienwerkzeug bleibt, der gesetzte
    // erste Punkt ist weg), ein Menü gibt es nicht.
    se.selection = vec![Item::Curve(arc)];
    se.set_tool(sk, Tool::Line);
    let points = sk.points().count();
    se.click(sk, DVec2::new(50.0, 50.0), 0.5, false).unwrap();
    assert!(sk.points().count() > points, "erster Punkt gesetzt");
    se.secondary_click(sk, (5.0, 5.0));
    assert_eq!(se.curve_menu, None);
    assert_eq!(sk.points().count(), points, "erster Klick zurückgenommen");
}

#[test]
fn dialog_has_dock_key() {
    assert!(DIALOG_KEYS.contains(&"curvature_display"));
}

// ---- Ansicht: Dialog und Menü in einem Bild ---------------------------------------------------

#[test]
fn the_dialog_and_the_menu_are_drawn_and_the_preview_leaves_the_sketch_alone() {
    let c = context();
    let mut app = PlanoraApp::for_tests(ws());
    let (arc, _) = open_sketch(app.ws_mut());
    select(app.ws_mut(), vec![Item::Curve(arc)]);
    // Menü
    state(app.ws_mut())
        .session_sketch_mut()
        .unwrap()
        .0
        .curve_menu = Some((800.0, 400.0));
    let t = settle(&c, &mut app);
    assert!(
        has(&t, "Krümmungsanzeige ein/aus"),
        "Menüpunkt fehlt: {t:?}"
    );
    // Der Menüpunkt öffnet den Dialog und schließt das Menü.
    let at = t
        .iter()
        .find(|(s, _)| s == "Krümmungsanzeige ein/aus")
        .unwrap()
        .1
        .center();
    click(&c, &mut app, at);
    assert!(state(app.ws_mut()).curvature_dialog.is_some());
    assert!(
        state(app.ws_mut())
            .sketch_session
            .as_ref()
            .is_some_and(|se| se.curve_menu.is_none()),
        "das Menü ist zu"
    );
    let t = settle(&c, &mut app);
    assert!(has(&t, "Krümmungsanzeige einrichten"), "{t:?}");
    assert!(has(&t, "Dichte") && has(&t, "Maßstab"));
    // Die Vorschau zeichnet, die Skizze bleibt ohne Kamm.
    assert_eq!(comb_of(app.ws_mut(), arc), None);
}

// ---- Nacharbeit -------------------------------------------------------------------------------

#[test]
fn right_click_during_a_drag_cancels_like_before() {
    let mut w = ws();
    let (arc, _) = open_sketch(&mut w);
    select(&mut w, vec![Item::Curve(arc)]);
    let (se, sk) = state(&mut w).session_sketch_mut().unwrap();
    // Zug an einem Bogenpunkt (Endpunkt bei 10|0)
    assert!(se.begin_drag(sk, DVec2::new(10.0, 0.0), 0.5));
    assert!(se.is_dragging());
    se.secondary_click(sk, (10.0, 20.0));
    assert_eq!(se.curve_menu, None, "kein Menü im Zug");
    assert!(!se.is_dragging(), "der Zug ist beendet");
}

#[test]
fn hidden_curves_draw_no_comb() {
    let mut sk = Sketch::new(Frame::WORLD, "Skizze", Tolerance::default());
    let (arc, _) = arc_and_line(&mut sk);
    sk.set_comb(arc, Some(CombSettings::default())).unwrap();
    let to_screen = |p: DVec3| pos2(p.x as f32 * 10.0, -p.y as f32 * 10.0);
    let none = std::collections::HashSet::new();
    let hidden: std::collections::HashSet<CurveId> = [arc].into_iter().collect();
    let red = egui::Color32::RED;
    assert!(
        !planora_app::curvature_dialog::comb_shapes_skipping(&sk, to_screen, 10.0, red, &none)
            .is_empty()
    );
    assert!(
        planora_app::curvature_dialog::comb_shapes_skipping(&sk, to_screen, 10.0, red, &hidden)
            .is_empty()
    );
}

#[test]
fn dragging_the_density_field_is_one_undo_step() {
    let c = context();
    let mut app = PlanoraApp::for_tests(ws());
    let (arc, _) = open_sketch(app.ws_mut());
    state(app.ws_mut())
        .session_sketch_mut()
        .unwrap()
        .1
        .set_comb(arc, Some(CombSettings::default()))
        .unwrap();
    app.ws_mut().observe_history(false);
    select(app.ws_mut(), vec![Item::Curve(arc)]);
    let t = settle(&c, &mut app);
    let area = shell_panels::field_rects(&c)
        .into_iter()
        .find(|(n, _)| *n == "Skizze")
        .expect("Skizzenfeld")
        .1;
    let label = t
        .iter()
        .find(|(s, r)| s == "Dichte" && area.contains(r.center()))
        .expect("Dichte")
        .1;
    // Das Feld rechts neben dem Namen zeigt den Wert 20.
    let field = t
        .iter()
        .find(|(s, r)| {
            s == "20" && area.contains(r.center()) && (r.center().y - label.center().y).abs() < 6.0
        })
        .expect("Wert 20")
        .1
        .center();
    let n = undo_len(app.ws_mut());
    frame(&c, &mut app, vec![Event::PointerMoved(field)]);
    frame(
        &c,
        &mut app,
        vec![Event::PointerMoved(field), button(field, true)],
    );
    for i in 1..=6 {
        let p = pos2(field.x + 8.0 * i as f32, field.y);
        frame(&c, &mut app, vec![Event::PointerMoved(p)]);
    }
    let end = pos2(field.x + 48.0, field.y);
    frame(&c, &mut app, vec![button(end, false)]);
    frame(&c, &mut app, vec![]);
    frame(&c, &mut app, vec![]);
    let got = comb_of(app.ws_mut(), arc).unwrap().density;
    assert!(got > 20, "das Ziehen ändert die Dichte: {got}");
    assert_eq!(undo_len(app.ws_mut()), n + 1, "ein Zug ist ein Schritt");
}
