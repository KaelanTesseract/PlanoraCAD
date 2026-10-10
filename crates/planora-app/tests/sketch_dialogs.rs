//! Dialoge „Skizze extrudieren“ / „Skizze drehen“ und die Anzeige fertiger Skizzen in 3D.

use planora_app::create_dialog::CreateDialog;
use planora_app::fonts;
use planora_app::sketch_view;
use planora_app::state::{AppState, BodyKind, StandardPlane};
use planora_math::DVec2;

fn context() -> egui::Context {
    let ctx = egui::Context::default();
    ctx.set_fonts(fonts::definitions());
    ctx
}

fn state_with_sketch() -> AppState {
    let mut s = AppState::new();
    let i = s.new_sketch(AppState::standard_frame(StandardPlane::XY));
    s.sketches[i]
        .add_rectangle(DVec2::ZERO, DVec2::new(10.0, 4.0))
        .unwrap();
    // offene Kette, die abgelehnt wird
    let a = s.sketches[i].add_point(30.0, 0.0).unwrap();
    let b = s.sketches[i].add_point(35.0, 0.0).unwrap();
    s.sketches[i].add_line(a, b).unwrap();
    s
}

/// Alle Texte der Ausgabe (auch in verschachtelten Formen).
fn collect_text(shape: &egui::epaint::Shape, seen: &mut String) {
    match shape {
        egui::epaint::Shape::Text(t) => {
            seen.push_str(t.galley.text());
            seen.push('\n');
        }
        egui::epaint::Shape::Vec(v) => v.iter().for_each(|s| collect_text(s, seen)),
        _ => {}
    }
}

#[test]
fn the_extrude_dialog_creates_a_body_and_stays_open_on_errors() {
    let mut s = state_with_sketch();
    let ok = CreateDialog::ExtrudeSketch {
        sketch: 0,
        profile: 0,
        height: 3.0,
    };
    ok.create(&mut s);
    assert_eq!(s.bodies.len(), 1);
    assert_eq!(s.bodies[0].kind, BodyKind::Extrusion);
    assert!(s.message.is_none());

    let bad = CreateDialog::ExtrudeSketch {
        sketch: 0,
        profile: 7,
        height: 3.0,
    };
    bad.create(&mut s);
    assert_eq!(s.bodies.len(), 1, "unverändert");
    assert!(s.message.is_some());
}

#[test]
fn the_dialogs_draw_and_list_profiles_and_rejections() {
    let ctx = context();
    let mut s = state_with_sketch();
    let mut dialog = CreateDialog::ExtrudeSketch {
        sketch: 0,
        profile: 0,
        height: 3.0,
    };
    let mut text = String::new();
    for _ in 0..2 {
        let out = ctx.run_ui(egui::RawInput::default(), |ui| {
            assert!(dialog.show(ui.ctx(), &mut s), "bleibt ohne Klick offen");
        });
        for c in &out.shapes {
            collect_text(&c.shape, &mut text);
        }
        out.drop_without_applying_deltas();
    }
    assert!(text.contains("Profil 1"), "{text}");
    assert!(
        text.contains("Nicht verwendbar"),
        "abgelehnte Kette wird erklärt:\n{text}"
    );
    let mut revolve = CreateDialog::RevolveSketch {
        sketch: 0,
        profile: 0,
        axis: None,
    };
    ctx.run_ui(egui::RawInput::default(), |ui| {
        revolve.show(ui.ctx(), &mut s);
    })
    .drop_without_applying_deltas();
    assert_eq!(s.bodies.len(), 0);
}

#[test]
fn revolve_dialog_reports_a_missing_axis() {
    let mut s = state_with_sketch();
    let d = CreateDialog::RevolveSketch {
        sketch: 0,
        profile: 0,
        axis: None,
    };
    d.create(&mut s);
    assert!(s.bodies.is_empty());
    assert!(
        s.message.as_deref().unwrap_or("").contains("Drehachse"),
        "{:?}",
        s.message
    );
}

// ---- Fertige Skizzen in der 3D-Ansicht (R13) -----------------------------------------------

const SCREEN: egui::Rect = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(800.0, 600.0));

fn prepared(mut s: AppState) -> AppState {
    s.camera.set_viewport(SCREEN.width(), SCREEN.height());
    s.camera.set_iso_view();
    s
}

#[test]
fn finished_sketches_are_drawn_as_lines_in_3d() {
    let empty = prepared(AppState::new());
    assert!(
        sketch_view::finished_sketch_shapes(&empty, SCREEN).is_empty(),
        "ohne Skizzen wird nichts gezeichnet"
    );

    let s = prepared(state_with_sketch());
    let shapes = sketch_view::finished_sketch_shapes(&s, SCREEN);
    assert!(
        !shapes.is_empty(),
        "eine fertige Skizze erscheint als Linien"
    );
}

#[test]
fn the_sketch_in_edit_is_not_drawn_twice() {
    let mut s = prepared(state_with_sketch());
    s.open_sketch(0).unwrap();
    assert!(
        sketch_view::finished_sketch_shapes(&s, SCREEN).is_empty(),
        "die geöffnete Skizze zeichnet das Skizzen-Overlay"
    );
    s.close_sketch();
    assert!(!sketch_view::finished_sketch_shapes(&s, SCREEN).is_empty());
}

#[test]
fn construction_geometry_of_finished_sketches_is_dashed() {
    let mut s = prepared(AppState::new());
    let i = s.new_sketch(AppState::standard_frame(StandardPlane::XY));
    let a = s.sketches[i].add_point(0.0, 0.0).unwrap();
    let b = s.sketches[i].add_point(20.0, 0.0).unwrap();
    let line = s.sketches[i].add_line(a, b).unwrap();
    let solid = sketch_view::finished_sketch_shapes(&s, SCREEN).len();
    s.sketches[i].set_construction(line, true).unwrap();
    let dashed = sketch_view::finished_sketch_shapes(&s, SCREEN).len();
    assert_eq!(solid, 1, "eine Linie ist ein Linienzug");
    assert!(dashed > 1, "gestrichelt zerfällt sie in Stücke ({dashed})");
}

#[test]
fn the_viewport_draws_finished_sketches_without_errors() {
    let ctx = context();
    let mut s = state_with_sketch();
    for _ in 0..2 {
        ctx.run_ui(egui::RawInput::default(), |ui| {
            planora_app::viewport::show(ui, &mut s)
        })
        .drop_without_applying_deltas();
    }
    assert!(s.message.is_none());
}

#[test]
fn browser_requests_become_sketch_dialogs() {
    use planora_app::app::dialog_for_request;
    use planora_app::state::SketchRequest;
    let mut s = state_with_sketch();
    assert_eq!(dialog_for_request(&mut s), None);
    s.sketch_request = Some(SketchRequest::Extrude(0));
    assert_eq!(
        dialog_for_request(&mut s),
        Some(CreateDialog::extrude_sketch(0))
    );
    assert_eq!(s.sketch_request, None, "der Wunsch wird verbraucht");

    // Drehen: die erste Konstruktionslinie ist als Achse vorgewählt.
    let a = s.sketches[0].add_point(0.0, -5.0).unwrap();
    let b = s.sketches[0].add_point(10.0, -5.0).unwrap();
    let axis = s.sketches[0].add_line(a, b).unwrap();
    s.sketches[0].set_construction(axis, true).unwrap();
    s.sketch_request = Some(SketchRequest::Revolve(0));
    assert_eq!(
        dialog_for_request(&mut s),
        Some(CreateDialog::RevolveSketch {
            sketch: 0,
            profile: 0,
            axis: Some(axis),
        })
    );
}

#[test]
fn the_revolve_dialog_creates_a_body_around_a_construction_line() {
    let mut s = AppState::new();
    let i = s.new_sketch(AppState::standard_frame(StandardPlane::XY));
    s.sketches[i]
        .add_rectangle(DVec2::new(0.0, 0.0), DVec2::new(5.0, 10.0))
        .unwrap();
    let a = s.sketches[i].add_point(0.0, -2.0).unwrap();
    let b = s.sketches[i].add_point(0.0, 12.0).unwrap();
    let axis = s.sketches[i].add_line(a, b).unwrap();
    s.sketches[i].set_construction(axis, true).unwrap();
    let d = CreateDialog::revolve_sketch(&s, i);
    assert!(d.create(&mut s), "{:?}", s.message);
    assert_eq!(s.bodies.len(), 1);
    assert_eq!(s.bodies[0].kind, BodyKind::Revolution);
}

// ---- Kontextmenü im Browser -----------------------------------------------------------------

fn click(pos: egui::Pos2, button: egui::PointerButton) -> Vec<egui::Event> {
    let press = |pressed| egui::Event::PointerButton {
        pos,
        button,
        pressed,
        modifiers: egui::Modifiers::default(),
    };
    vec![egui::Event::PointerMoved(pos), press(true), press(false)]
}

fn raw(events: Vec<egui::Event>) -> egui::RawInput {
    egui::RawInput {
        screen_rect: Some(SCREEN),
        events,
        ..Default::default()
    }
}

/// Mitte des ersten Textes, der mit `prefix` beginnt (auch in verschachtelten Formen).
fn find_text(shapes: &[egui::epaint::ClippedShape], prefix: &str) -> Option<egui::Pos2> {
    fn walk(shape: &egui::epaint::Shape, prefix: &str) -> Option<egui::Pos2> {
        match shape {
            egui::epaint::Shape::Text(t) if t.galley.text().starts_with(prefix) => {
                Some(t.visual_bounding_rect().center())
            }
            egui::epaint::Shape::Vec(v) => v.iter().find_map(|s| walk(s, prefix)),
            _ => None,
        }
    }
    shapes.iter().find_map(|c| walk(&c.shape, prefix))
}

/// Ein Bild des Browsers mit `events`; liefert die gezeichneten Formen.
fn browser_frame(
    ctx: &egui::Context,
    s: &mut AppState,
    events: Vec<egui::Event>,
) -> Vec<egui::epaint::ClippedShape> {
    let out = ctx.run_ui(raw(events), |ui| planora_app::browser::show(ui, s));
    let shapes = out.shapes.clone();
    out.drop_without_applying_deltas();
    shapes
}

/// Rechtsklick auf „Skizze 1“ und Klick auf den Menüeintrag `entry`.
fn use_context_menu(s: &mut AppState, entry: &str) {
    let ctx = context();
    let shapes = browser_frame(&ctx, s, vec![]);
    let row = find_text(&shapes, "Skizze 1").expect("Skizzenzeile");
    browser_frame(&ctx, s, click(row, egui::PointerButton::Secondary));
    let shapes = browser_frame(&ctx, s, vec![]);
    let item = find_text(&shapes, entry).unwrap_or_else(|| panic!("Menüeintrag {entry}"));
    browser_frame(&ctx, s, click(item, egui::PointerButton::Primary));
}

#[test]
fn the_browser_context_menu_extrudes_after_closing_the_sketch() {
    let mut s = state_with_sketch();
    s.open_sketch(0).unwrap();
    use_context_menu(&mut s, "Extrudieren");
    assert!(s.sketch_session.is_none(), "die Skizze wird vorher beendet");
    assert_eq!(
        s.sketch_request,
        Some(planora_app::state::SketchRequest::Extrude(0))
    );
}

#[test]
fn the_browser_context_menu_deletes_and_edits() {
    let mut s = state_with_sketch();
    use_context_menu(&mut s, "Bearbeiten");
    assert_eq!(s.sketch_session.as_ref().map(|se| se.sketch), Some(0));
    use_context_menu(&mut s, "Löschen");
    assert!(s.sketches.is_empty());
    assert!(s.sketch_session.is_none());
}

// ---- Abschlusswelle -----------------------------------------------------------------------

#[test]
fn the_browser_context_menu_clears_the_body_selection() {
    for entry in ["Extrudieren", "Drehen"] {
        let mut s = state_with_sketch();
        assert!(CreateDialog::extrude_sketch(0).create(&mut s));
        s.selected_body = Some(0);
        use_context_menu(&mut s, entry);
        assert!(s.sketch_request.is_some(), "{entry}");
        assert_eq!(s.selected_body, None, "{entry}: Körperauswahl aufgehoben");
        assert_eq!(s.selection, None, "{entry}: 3D-Auswahl aufgehoben");
        assert_eq!(s.selected_sketch, Some(0));
    }
}

#[test]
fn a_stale_revolve_axis_is_rejected() {
    let mut s = AppState::new();
    let i = s.new_sketch(AppState::standard_frame(StandardPlane::XY));
    s.sketches[i]
        .add_rectangle(DVec2::new(0.0, 0.0), DVec2::new(5.0, 10.0))
        .unwrap();
    let a = s.sketches[i].add_point(0.0, -2.0).unwrap();
    let b = s.sketches[i].add_point(0.0, 12.0).unwrap();
    let axis = s.sketches[i].add_line(a, b).unwrap();
    s.sketches[i].set_construction(axis, true).unwrap();
    let d = CreateDialog::revolve_sketch(&s, i);
    // Die Achse wird gelöscht, während der Dialog offen ist.
    s.sketches[i].remove_curve(axis).unwrap();
    assert!(!d.create(&mut s));
    assert!(s.bodies.is_empty());
    let msg = s.message.clone().unwrap_or_default();
    assert!(msg.contains("Drehachse"), "{msg}");
    // Eine Linie einer anderen Skizze (Index verschoben) gilt ebenso nicht.
    let j = s.new_sketch(AppState::standard_frame(StandardPlane::XY));
    s.sketches[j]
        .add_rectangle(DVec2::new(1.0, 0.0), DVec2::new(5.0, 10.0))
        .unwrap();
    let foreign = CreateDialog::RevolveSketch {
        sketch: j,
        profile: 0,
        axis: Some(planora_kernel::Id::from_index(40)),
    };
    assert!(!foreign.create(&mut s));
    assert!(s.bodies.is_empty());
}

#[test]
fn profiles_are_computed_again_only_after_a_change() {
    let mut s = state_with_sketch();
    let ctx = context();
    let mut d = CreateDialog::extrude_sketch(0);
    for _ in 0..3 {
        let out = ctx.run_ui(raw(vec![]), |ui| {
            d.show(ui.ctx(), &mut s);
        });
        out.drop_without_applying_deltas();
    }
    assert_eq!(
        s.profile_cache.computations(),
        1,
        "drei Bilder, eine Berechnung"
    );
    let n = s.profile_cache.get(&s.sketches[0]).profiles.len();
    assert_eq!(s.profile_cache.computations(), 1);
    // Eine Änderung (zweites Rechteck) macht den Speicher ungültig.
    s.sketches[0]
        .add_rectangle(DVec2::new(50.0, 0.0), DVec2::new(60.0, 4.0))
        .unwrap();
    let after = s.profile_cache.get(&s.sketches[0]);
    assert_eq!(s.profile_cache.computations(), 2);
    assert_eq!(after.profiles.len(), n + 1);
    // Auch reines Verschieben eines Punktes ändert den Stand.
    let p = s.sketches[0].points().next().unwrap().0;
    s.sketches[0].set_point_xy(p, -1.0, -1.0).unwrap();
    s.profile_cache.get(&s.sketches[0]);
    assert_eq!(s.profile_cache.computations(), 3);
}
