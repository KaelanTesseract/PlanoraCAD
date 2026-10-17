//! Rauchtests ohne Fenster: Werkzeugleiste und Darstellung des Skizzen-Modus werden mit einem
//! egui-Kontext gezeichnet; einfache Eingaben werden als Rohereignisse eingespeist.

use planora_app::sketch_state::{Item, Tool};
use planora_app::state::{AppState, StandardPlane};
use planora_app::{browser, fonts, sketch_panel, sketch_view, viewport};
use planora_feature::SketchPlane;
use planora_math::DVec2;
use planora_sketch::constraint::ConstraintKind as K;

fn context() -> egui::Context {
    let ctx = egui::Context::default();
    ctx.set_fonts(fonts::definitions());
    ctx
}

fn populated() -> AppState {
    let mut s = AppState::new();
    let i = s
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    // Inhalt wie im Skizzen-Modus anlegen und zurückschreiben; danach neu öffnen.
    s.open_sketch(i).unwrap();
    {
        let sk = s.session_sketch_mut().unwrap().1;
        let r = sk
            .add_rectangle(DVec2::ZERO, DVec2::new(40.0, 30.0))
            .unwrap();
        sk.add_constraint(K::Distance(r.points[0], r.points[1], 40.0))
            .unwrap();
        let c = sk.add_point(20.0, 15.0).unwrap();
        sk.add_circle(c, 5.0).unwrap();
        sk.solve();
    }
    s.close_sketch();
    s.open_sketch(i).unwrap();
    s
}

/// Wechselt in der offenen Skizze das Werkzeug.
fn set_tool(s: &mut AppState, tool: Tool) {
    let (se, sk) = s.session_sketch_mut().unwrap();
    se.set_tool(sk, tool);
}

fn empty_sketch() -> AppState {
    let mut s = AppState::new();
    let i = s
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    s.open_sketch(i).unwrap();
    s
}

const SCREEN: egui::Rect = egui::Rect {
    min: egui::pos2(0.0, 0.0),
    max: egui::pos2(800.0, 600.0),
};

fn input(events: Vec<egui::Event>, shift: bool) -> egui::RawInput {
    let modifiers = egui::Modifiers {
        shift,
        ..Default::default()
    };
    let mut all = vec![egui::Event::ModifiersChanged(modifiers)];
    all.extend(events);
    egui::RawInput {
        screen_rect: Some(SCREEN),
        events: all,
        ..Default::default()
    }
}

fn moved(x: f32, y: f32) -> egui::Event {
    egui::Event::PointerMoved(egui::pos2(x, y))
}

fn button(x: f32, y: f32, pressed: bool, shift: bool) -> egui::Event {
    egui::Event::PointerButton {
        pos: egui::pos2(x, y),
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers {
            shift,
            ..Default::default()
        },
    }
}

/// Spielt eine Folge von Rohereignissen (je Eintrag ein Frame) gegen das Overlay ab. Ein leerer
/// Frame vorweg: egui ordnet Zeiger-Ereignisse den Flächen des vorigen Frames zu.
fn play_overlay(ctx: &egui::Context, s: &mut AppState, frames: Vec<Vec<egui::Event>>) {
    for events in std::iter::once(vec![]).chain(frames) {
        ctx.run_ui(input(events, false), |ui| {
            sketch_view::show_overlay(ui, SCREEN, s);
        })
        .drop_without_applying_deltas();
    }
}

#[test]
fn the_panel_and_overlay_draw_without_changing_the_sketch() {
    let ctx = context();
    let mut s = populated();
    let before = s.session_sketch().unwrap().variables().to_vec();
    for _ in 0..3 {
        ctx.run_ui(egui::RawInput::default(), |ui| {
            sketch_panel::show(ui.ctx(), &mut s);
            let rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(800.0, 600.0));
            sketch_view::show_overlay(ui, rect, &mut s);
        })
        .drop_without_applying_deltas();
    }
    assert_eq!(s.session_sketch().unwrap().variables(), before.as_slice());
    assert!(s.sketch_session.is_some());
}

#[test]
fn every_tool_and_a_selection_can_be_drawn() {
    let ctx = context();
    let mut s = populated();
    let first_curve = s.session_sketch().unwrap().curves().next().unwrap().0;
    for tool in [
        Tool::Select,
        Tool::Point,
        Tool::Line,
        Tool::Rectangle,
        Tool::Circle,
        Tool::Arc,
        Tool::Ellipse,
        Tool::Spline,
        Tool::ProjectEdge,
    ] {
        {
            set_tool(&mut s, tool);
            let se = s.sketch_session.as_mut().unwrap();
            se.selection = vec![Item::Curve(first_curve)];
            se.cursor = Some(DVec2::new(10.0, 10.0));
        }
        ctx.run_ui(egui::RawInput::default(), |ui| {
            sketch_panel::show(ui.ctx(), &mut s);
            let rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(640.0, 480.0));
            sketch_view::show_overlay(ui, rect, &mut s);
        })
        .drop_without_applying_deltas();
    }
}

#[test]
fn texts_in_the_panel_are_german() {
    let ctx = context();
    ctx.enable_accesskit();
    let mut s = populated();
    let mut seen = String::new();
    // Ein neues Feld wird im ersten Frame nur vermessen (unsichtbar); geprüft wird der letzte.
    // Die Werkzeugleiste zeigt Symbole; ihre Namen stehen in der Knopfbeschreibung (für
    // Bildschirmleser) und im Hinweistext beim Überfahren.
    for frame in 0..3 {
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
            sketch_panel::show(ui.ctx(), &mut s);
        });
        if frame == 2 {
            seen.push_str(&planora_app::testing::collect_texts(&out));
            let update = out.platform_output.accesskit_update.take().unwrap();
            for (_, node) in &update.nodes {
                if let Some(label) = node.label() {
                    seen.push_str(label);
                    seen.push('\n');
                }
            }
        }
        out.drop_without_applying_deltas();
    }
    for must in ["Linie", "Kreis", "Skizze beenden", "Abhängigkeiten"] {
        assert!(
            seen.lines().any(|l| l == must),
            "„{must}“ fehlt in den Skizzenfeldern:\n{seen}"
        );
    }
}

#[test]
fn numbers_are_shown_with_a_comma_and_read_with_comma_or_dot() {
    assert_eq!(sketch_view::format_number(40.0, 2), "40,00");
    assert_eq!(sketch_view::format_number(-0.5, 1), "-0,5");
    assert_eq!(sketch_view::parse_number("12,5"), Some(12.5));
    assert_eq!(sketch_view::parse_number(" 12.5 "), Some(12.5));
    assert_eq!(sketch_view::parse_number("abc"), None);
    assert_eq!(sketch_view::parse_number("inf"), None);
    assert_eq!(sketch_view::parse_number(""), None);
}

#[test]
fn a_click_in_the_overlay_places_a_point_at_the_cursor() {
    let ctx = context();
    let mut s = empty_sketch();
    set_tool(&mut s, Tool::Point);
    play_overlay(
        &ctx,
        &mut s,
        vec![
            vec![moved(400.0, 300.0), button(400.0, 300.0, true, false)],
            vec![button(400.0, 300.0, false, false)],
            vec![],
        ],
    );
    let sk = s.session_sketch().unwrap();
    assert_eq!(sk.points().count(), 1, "{:?}", s.sketch_session);
    let (p, _) = sk.points().next().unwrap();
    let xy = sk.point_xy(p).unwrap();
    assert!(
        xy.length() < 1e-3,
        "Bildmitte = Ursprung der Skizze: {xy:?}"
    );
}

#[test]
fn dragging_a_point_in_the_select_tool_moves_it() {
    let ctx = context();
    let mut s = empty_sketch();
    let p = s
        .session_sketch_mut()
        .unwrap()
        .1
        .add_point(0.0, 0.0)
        .unwrap();
    play_overlay(
        &ctx,
        &mut s,
        vec![
            vec![moved(400.0, 300.0), button(400.0, 300.0, true, false)],
            vec![moved(420.0, 300.0)],
            vec![moved(440.0, 300.0)],
            vec![moved(460.0, 300.0)],
            vec![button(460.0, 300.0, false, false)],
            vec![],
        ],
    );
    let xy = s.session_sketch().unwrap().point_xy(p).unwrap();
    assert!(
        xy.x > 1.0 && xy.y.abs() < 1e-3,
        "nach rechts gezogen: {xy:?}"
    );
    assert!(s.sketch_session.as_ref().unwrap().dragging.is_none());
}

#[test]
fn in_sketch_mode_the_viewport_does_not_orbit_but_still_pans() {
    let ctx = context();
    let mut s = empty_sketch();
    let (yaw, pitch, target) = (s.camera.yaw, s.camera.pitch, s.camera.target);
    let drag = |shift: bool| {
        vec![
            vec![],
            vec![moved(200.0, 300.0), button(200.0, 300.0, true, shift)],
            vec![moved(260.0, 330.0)],
            vec![moved(320.0, 360.0)],
            vec![button(320.0, 360.0, false, shift)],
        ]
    };
    for (frames, shift) in [(drag(false), false), (drag(true), true)] {
        for events in frames {
            ctx.run_ui(input(events, shift), |ui| viewport::show(ui, &mut s))
                .drop_without_applying_deltas();
        }
        if !shift {
            assert_eq!(
                (s.camera.yaw, s.camera.pitch),
                (yaw, pitch),
                "Orbit gesperrt"
            );
            assert_eq!(
                s.camera.target, target,
                "Ziehen ohne Umschalt verschiebt nicht"
            );
        }
    }
    assert_eq!(
        (s.camera.yaw, s.camera.pitch),
        (yaw, pitch),
        "Orbit gesperrt"
    );
    assert_ne!(s.camera.target, target, "Umschalt + Ziehen verschiebt");
}

#[test]
fn the_tree_lists_sketches_and_marks_the_open_one() {
    let ctx = context();
    let mut s = populated();
    s.add_sketch(SketchPlane::Standard(StandardPlane::YZ))
        .unwrap();
    let mut seen = String::new();
    let out = ctx.run_ui(egui::RawInput::default(), |ui| browser::show(ui, &mut s));
    seen.push_str(&planora_app::testing::collect_texts(&out));
    out.drop_without_applying_deltas();
    // Seit dem Feature-Baum (Task 9) gibt es keine eigenen Überschriften „Körper“/„Skizzen“
    // mehr; Skizzen stehen als Zeilen der Zeitleiste im Baum.
    for must in ["Feature-Baum", "Skizze 1 (in Bearbeitung)", "Skizze 2"] {
        assert!(
            seen.contains(must),
            "„{must}“ fehlt im Browser:
{seen}"
        );
    }
}

#[test]
fn shift_on_the_click_itself_extends_the_selection() {
    let ctx = context();
    let mut s = empty_sketch();
    let a = s
        .session_sketch_mut()
        .unwrap()
        .1
        .add_point(0.0, 0.0)
        .unwrap();
    // Zweiter Punkt rechts der Bildmitte; Lage auf dem Bildschirm aus der Abbildung.
    let b = s
        .session_sketch_mut()
        .unwrap()
        .1
        .add_point(10.0, 0.0)
        .unwrap();
    let frame = *s.session_sketch().unwrap().frame();
    let mut cam = s.camera;
    cam.set_viewport(800.0, 600.0);
    let pb = sketch_view::sketch_to_screen(
        &cam,
        &frame,
        glam::Vec2::new(800.0, 600.0),
        DVec2::new(10.0, 0.0),
    )
    .unwrap();
    play_overlay(
        &ctx,
        &mut s,
        vec![
            vec![moved(400.0, 300.0), button(400.0, 300.0, true, false)],
            vec![button(400.0, 300.0, false, false)],
            vec![],
            vec![moved(pb.x, pb.y), button(pb.x, pb.y, true, true)],
            // Die Umschalttaste ist im Frame des Loslassens schon wieder oben; nur das
            // Klick-Ereignis trägt sie.
            vec![button(pb.x, pb.y, false, true)],
            vec![],
        ],
    );
    let sel = &s.sketch_session.as_ref().unwrap().selection;
    assert_eq!(sel, &vec![Item::Point(a), Item::Point(b)]);
}

// ---- Fix-Runde 1 ----------------------------------------------------------------------------

fn key(k: egui::Key) -> egui::Event {
    egui::Event::Key {
        key: k,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::default(),
    }
}

fn click_at(x: f32, y: f32) -> Vec<Vec<egui::Event>> {
    vec![
        vec![moved(x, y), button(x, y, true, false)],
        vec![button(x, y, false, false)],
    ]
}

#[test]
fn two_fast_clicks_at_different_places_are_two_clicks() {
    let ctx = context();
    let mut s = empty_sketch();
    set_tool(&mut s, Tool::Point);
    // Aufeinanderfolgende Frames liegen weit unter der Doppelklickzeit von egui.
    let mut frames = click_at(300.0, 300.0);
    frames.extend(click_at(500.0, 300.0));
    frames.push(vec![]);
    play_overlay(&ctx, &mut s, frames);
    assert_eq!(
        s.session_sketch().unwrap().points().count(),
        2,
        "zweiter Punkt nicht verschluckt"
    );
}

#[test]
fn a_real_double_click_still_finishes_the_tool() {
    let ctx = context();
    let mut s = empty_sketch();
    set_tool(&mut s, Tool::Spline);
    let mut frames = click_at(300.0, 300.0);
    frames.push(vec![]);
    frames.extend(click_at(500.0, 250.0));
    frames.extend(click_at(500.0, 250.0)); // Doppelklick an derselben Stelle
    frames.push(vec![]);
    play_overlay(&ctx, &mut s, frames);
    let se = s.sketch_session.as_ref().unwrap();
    assert_eq!(
        s.session_sketch().unwrap().curves().count(),
        1,
        "Spline angelegt: {se:?}"
    );
    assert!(se.pending.is_empty());
}

/// Skizze mit einer Linie (ausgewählt) und einem laufenden Linienwerkzeug.
fn with_line_and_pending() -> AppState {
    let mut s = empty_sketch();
    let sk = s.session_sketch_mut().unwrap().1;
    let a = sk.add_point(-10.0, -10.0).unwrap();
    let b = sk.add_point(10.0, -10.0).unwrap();
    let line = sk.add_line(a, b).unwrap();
    set_tool(&mut s, Tool::Line);
    let se = s.sketch_session.as_mut().unwrap();
    se.pending = vec![a];
    se.selection = vec![Item::Curve(line)];
    s
}

fn press_key_in_overlay(s: &mut AppState, k: egui::Key, with_editor: bool) {
    let ctx = context();
    if with_editor {
        sketch_panel::open_dimension_editor(
            &ctx,
            s,
            planora_app::sketch_state::ConstraintAction::Distance,
        );
    }
    let frames = vec![vec![], vec![], vec![key(k)], vec![]];
    for events in frames {
        ctx.run_ui(input(events, false), |ui| {
            if with_editor {
                sketch_panel::show(ui.ctx(), s);
            }
            sketch_view::show_overlay(ui, SCREEN, s);
            viewport::remember_keyboard_focus(ui.ctx());
        })
        .drop_without_applying_deltas();
    }
}

#[test]
fn escape_cancels_the_running_tool() {
    let mut s = with_line_and_pending();
    press_key_in_overlay(&mut s, egui::Key::Escape, false);
    assert!(s.sketch_session.as_ref().unwrap().pending.is_empty());
}

#[test]
fn enter_finishes_a_spline() {
    let mut s = empty_sketch();
    let sk = s.session_sketch_mut().unwrap().1;
    let a = sk.add_point(0.0, 0.0).unwrap();
    let b = sk.add_point(10.0, 5.0).unwrap();
    set_tool(&mut s, Tool::Spline);
    let se = s.sketch_session.as_mut().unwrap();
    se.pending = vec![a, b];
    press_key_in_overlay(&mut s, egui::Key::Enter, false);
    assert_eq!(s.session_sketch().unwrap().curves().count(), 1);
    assert!(s.sketch_session.as_ref().unwrap().pending.is_empty());
}

#[test]
fn delete_removes_the_selection() {
    let mut s = with_line_and_pending();
    press_key_in_overlay(&mut s, egui::Key::Delete, false);
    assert_eq!(s.session_sketch().unwrap().curves().count(), 0);
    assert!(s.sketch_session.as_ref().unwrap().selection.is_empty());
}

#[test]
fn keys_do_not_reach_the_overlay_while_the_dimension_field_has_focus() {
    for k in [egui::Key::Escape, egui::Key::Enter, egui::Key::Delete] {
        let mut s = with_line_and_pending();
        let before = s.sketch_session.as_ref().unwrap().clone();
        press_key_in_overlay(&mut s, k, true);
        let se = s.sketch_session.as_ref().unwrap();
        assert_eq!(
            se.pending, before.pending,
            "{k:?}: laufendes Werkzeug unverändert"
        );
        assert_eq!(se.selection, before.selection, "{k:?}: Auswahl unverändert");
        assert_eq!(
            s.session_sketch().unwrap().curves().count(),
            1,
            "{k:?}: nichts gelöscht"
        );
    }
}

#[test]
fn the_dimension_editor_closes_when_the_selection_changes() {
    let ctx = context();
    let mut s = populated();
    let curves: Vec<_> = s
        .session_sketch()
        .unwrap()
        .curves()
        .map(|(c, _)| c)
        .collect();
    s.sketch_session.as_mut().unwrap().selection = vec![Item::Curve(curves[1])];
    sketch_panel::open_dimension_editor(
        &ctx,
        &s,
        planora_app::sketch_state::ConstraintAction::Distance,
    );
    let frame = |s: &mut AppState| {
        ctx.run_ui(egui::RawInput::default(), |ui| {
            sketch_panel::show(ui.ctx(), s)
        })
        .drop_without_applying_deltas();
    };
    frame(&mut s);
    assert!(sketch_panel::dimension_editor_open(&ctx));
    s.sketch_session.as_mut().unwrap().selection = vec![Item::Curve(curves[2])];
    frame(&mut s);
    assert!(
        !sketch_panel::dimension_editor_open(&ctx),
        "das Zahlenfeld gehört zur alten Auswahl"
    );
}
