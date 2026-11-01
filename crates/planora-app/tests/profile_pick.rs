//! Profile anzeigen und per Klick wählen (Skizzen 1, Task 11), wie Fusion: Im Skizzen-Modus sind
//! die Profile hell gefüllt (Schalter „Profil anzeigen“); „Skizze extrudieren“/„Skizze drehen“
//! und „Bearbeiten“ wählen Profile per Klick in die 3D-Ansicht (Klick schaltet um) oder über die
//! Liste mit Häkchen. Ohne Fenster bei 1920 × 1080.

use egui::epaint::{ClippedShape, Shape};
use planora_app::create_dialog::CreateDialog;
use planora_app::feature_dialog::FeatureDialog;
use planora_app::state::{AppState, ProfilePick, StandardPlane};
use planora_app::{fonts, sketch_panel, sketch_view, theme, viewport};
use planora_feature::{FeatureId, FeatureKind, FeatureStatus, SketchPlane};
use planora_math::DVec2;
use planora_render::ViewFace;
use planora_sketch::model::{CurveId, Sketch};

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

fn raw(events: Vec<egui::Event>) -> egui::RawInput {
    egui::RawInput {
        screen_rect: Some(SCREEN),
        events,
        ..Default::default()
    }
}

fn button(p: egui::Pos2, pressed: bool) -> egui::Event {
    egui::Event::PointerButton {
        pos: p,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::default(),
    }
}

/// Skizze auf XY, Inhalt aus `build`, danach beendet.
fn sketch_with<T>(build: impl FnOnce(&mut Sketch) -> T) -> (AppState, FeatureId, T) {
    let mut s = AppState::new();
    let id = s
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    s.open_sketch(id).unwrap();
    let t = build(s.session_sketch_mut().unwrap().1);
    s.close_sketch();
    (s, id, t)
}

/// Zwei getrennte Rechtecke links der Bildmitte (dort steht der Dialog): A (Profil 1) und
/// B (Profil 2).
fn two_rectangles() -> (AppState, FeatureId) {
    let (s, id, ()) = sketch_with(|sk| {
        sk.add_rectangle(DVec2::new(-60.0, 0.0), DVec2::new(-50.0, 10.0))
            .unwrap();
        sk.add_rectangle(DVec2::new(-40.0, 0.0), DVec2::new(-30.0, 10.0))
            .unwrap();
    });
    (s, id)
}

/// Mitte von Rechteck A und B.
const A: DVec2 = DVec2::new(-55.0, 5.0);
const B: DVec2 = DVec2::new(-35.0, 5.0);

/// Kamera senkrecht von oben oder schräg (isometrisch), auf die Skizze eingepasst.
fn aim(s: &mut AppState, oblique: bool) {
    if oblique {
        s.camera.set_iso_view();
    } else {
        s.camera.set_view(ViewFace::Top);
    }
    s.camera.target = glam::Vec3::new(30.0, 5.0, 0.0);
    s.camera.distance = 160.0;
}

/// Ein Bild wie in der App: 3D-Ansicht über das ganze Fenster, darüber der Dialog. Liefert die
/// Formen und ob der Dialog offen bleibt.
fn frame(
    ctx: &egui::Context,
    s: &mut AppState,
    dialog: &mut CreateDialog,
    events: Vec<egui::Event>,
) -> (Vec<ClippedShape>, bool) {
    let mut open = false;
    let out = ctx.run_ui(raw(events), |ui| {
        viewport::show_in(ui, s, None);
        open = dialog.show(ui.ctx(), s);
    });
    let shapes = out.shapes.clone();
    out.drop_without_applying_deltas();
    (shapes, open)
}

/// Bildschirmlage eines Punkts der Skizze `id` (Ansicht über das ganze Fenster).
fn screen(s: &AppState, id: FeatureId, p: DVec2) -> egui::Pos2 {
    let frame = *s.document.sketch_of(id).unwrap().frame();
    let v = sketch_view::sketch_to_screen(&s.camera, &frame, glam::Vec2::new(W, H), p).unwrap();
    egui::pos2(v.x, v.y)
}

/// Rechteck des Dialogfensters „Skizze extrudieren“ (im vorigen Bild).
fn dialog_rect(ctx: &egui::Context) -> egui::Rect {
    ctx.memory(|m| m.area_rect(egui::Id::new(("create_dialog", "extrude"))))
        .expect("Dialog gezeichnet")
}

/// Klick ohne Ziehen an `p` (mit Bildern davor und danach, wie in der App).
fn click(ctx: &egui::Context, s: &mut AppState, dialog: &mut CreateDialog, p: egui::Pos2) {
    for events in [
        vec![egui::Event::PointerMoved(p)],
        vec![button(p, true)],
        vec![button(p, false)],
        vec![],
    ] {
        frame(ctx, s, dialog, events);
    }
}

fn picked(dialog: &CreateDialog) -> Vec<usize> {
    dialog.profile_choice().unwrap().1.to_vec()
}

/// Alle Texte der Formen mit ihrer Lage.
fn find_text(shapes: &[ClippedShape], text: &str) -> Option<egui::Pos2> {
    fn walk(shape: &Shape, text: &str) -> Option<egui::Pos2> {
        match shape {
            Shape::Text(t) if t.galley.text() == text => Some(t.visual_bounding_rect().center()),
            Shape::Vec(v) => v.iter().find_map(|s| walk(s, text)),
            _ => None,
        }
    }
    shapes.iter().find_map(|c| walk(&c.shape, text))
}

fn texts(shapes: &[ClippedShape]) -> String {
    fn walk(shape: &Shape, out: &mut String) {
        match shape {
            Shape::Text(t) => {
                out.push_str(t.galley.text());
                out.push('\n');
            }
            Shape::Vec(v) => v.iter().for_each(|s| walk(s, out)),
            _ => {}
        }
    }
    let mut out = String::new();
    shapes.iter().for_each(|c| walk(&c.shape, &mut out));
    out
}

/// Zahl der Dreiecke in Netzen mit der Farbe `color`.
fn filled_triangles(shapes: &[Shape], color: egui::Color32) -> usize {
    shapes
        .iter()
        .map(|s| match s {
            Shape::Mesh(m) => m
                .indices
                .chunks(3)
                .filter(|t| m.vertices[t[0] as usize].color == color)
                .count(),
            Shape::Vec(v) => filled_triangles(v, color),
            _ => 0,
        })
        .sum()
}

fn all_shapes(shapes: &[ClippedShape]) -> Vec<Shape> {
    shapes.iter().map(|c| c.shape.clone()).collect()
}

// ---- Klick in die Ansicht -------------------------------------------------------------------

#[test]
fn a_click_into_a_rectangle_toggles_its_profile_from_above_and_oblique() {
    for oblique in [false, true] {
        let ctx = context();
        let (mut s, id) = two_rectangles();
        aim(&mut s, oblique);
        let mut dialog = CreateDialog::extrude_sketch(&s, id);
        assert_eq!(
            picked(&dialog),
            Vec::<usize>::new(),
            "zwei Profile: keins vorgewählt"
        );
        frame(&ctx, &mut s, &mut dialog, vec![]);
        let (shapes, _) = frame(&ctx, &mut s, &mut dialog, vec![]);
        let p = screen(&s, id, B);
        assert!(SCREEN.contains(p), "{oblique}: {p:?}");
        assert!(
            !dialog_rect(&ctx).contains(p),
            "{oblique}: Ziel unter dem Dialog"
        );
        assert!(
            texts(&shapes).contains("Profil 2"),
            "Liste zeigt beide Profile"
        );

        click(&ctx, &mut s, &mut dialog, p);
        assert_eq!(picked(&dialog), vec![1], "{oblique}: B gewählt");
        assert_eq!(
            s.profile_pick.as_ref().map(|p| p.selected.clone()),
            Some(vec![1]),
            "{oblique}: die Ansicht zeigt B als gewählt"
        );
        assert!(s.selection.is_none(), "{oblique}: keine Körperauswahl");

        let a = screen(&s, id, A);
        click(&ctx, &mut s, &mut dialog, a);
        assert_eq!(picked(&dialog), vec![0, 1], "{oblique}: A dazu");
        click(&ctx, &mut s, &mut dialog, p);
        assert_eq!(picked(&dialog), vec![0], "{oblique}: B wieder aus");
        // Klick daneben ändert nichts.
        let gap = screen(&s, id, DVec2::new(-45.0, 5.0));
        click(&ctx, &mut s, &mut dialog, gap);
        assert_eq!(picked(&dialog), vec![0], "{oblique}: daneben");
    }
}

#[test]
fn the_profile_under_the_pointer_is_highlighted_and_picked_ones_are_stronger() {
    let ctx = context();
    let (mut s, id) = two_rectangles();
    aim(&mut s, false);
    let mut dialog = CreateDialog::ExtrudeSketch {
        sketch: id,
        profiles: vec![0],
        height: 3.0,
    };
    frame(&ctx, &mut s, &mut dialog, vec![]);
    let b = screen(&s, id, B);
    frame(
        &ctx,
        &mut s,
        &mut dialog,
        vec![egui::Event::PointerMoved(b)],
    );
    assert_eq!(s.profile_pick.as_ref().and_then(|p| p.hover), Some(1));
    let col = theme::sketch_colors(s.dark);
    let shapes = sketch_view::finished_sketch_shapes(&s, SCREEN);
    assert!(
        filled_triangles(&shapes, col.profile_fill_selected) > 0,
        "A gewählt"
    );
    assert!(
        filled_triangles(&shapes, col.profile_fill_hover) > 0,
        "B überfahren"
    );
    // Ohne Profilwahl (kein Dialog) keine Flächen in 3D.
    s.profile_pick = None;
    let shapes = sketch_view::finished_sketch_shapes(&s, SCREEN);
    for c in [
        col.profile_fill,
        col.profile_fill_selected,
        col.profile_fill_hover,
    ] {
        assert_eq!(filled_triangles(&shapes, c), 0);
    }
}

// ---- Liste und Klick ------------------------------------------------------------------------

/// Profilwahl des zuletzt angelegten Features.
fn last_profiles(s: &AppState) -> planora_feature::ProfileSel {
    match &s.document.features().last().unwrap().kind {
        FeatureKind::Extrude { profiles, .. } | FeatureKind::Revolve { profiles, .. } => {
            profiles.clone()
        }
        k => panic!("{k:?}"),
    }
}

#[test]
fn the_list_and_a_click_give_the_same_feature() {
    let ctx = context();
    // Über den Klick in die Ansicht
    let (mut s, id) = two_rectangles();
    aim(&mut s, true);
    let mut dialog = CreateDialog::extrude_sketch(&s, id);
    frame(&ctx, &mut s, &mut dialog, vec![]);
    frame(&ctx, &mut s, &mut dialog, vec![]);
    let b = screen(&s, id, B);
    click(&ctx, &mut s, &mut dialog, b);
    assert!(dialog.create(&mut s), "{:?}", s.message);
    let by_click = last_profiles(&s);

    // Über das Häkchen in der Liste
    let ctx = context();
    let (mut t, id) = two_rectangles();
    aim(&mut t, true);
    let mut dialog = CreateDialog::extrude_sketch(&t, id);
    frame(&ctx, &mut t, &mut dialog, vec![]);
    let (shapes, _) = frame(&ctx, &mut t, &mut dialog, vec![]);
    let at = find_text(&shapes, "Profil 2: 100,00 mm²").expect("Eintrag in der Liste");
    for events in [
        vec![egui::Event::PointerMoved(at)],
        vec![button(at, true)],
        vec![button(at, false)],
        vec![],
    ] {
        frame(&ctx, &mut t, &mut dialog, events);
    }
    assert_eq!(picked(&dialog), vec![1], "Häkchen bei Profil 2");
    assert_eq!(
        t.profile_pick.as_ref().map(|p| p.selected.clone()),
        Some(vec![1]),
        "die Ansicht zeigt die Wahl der Liste"
    );
    assert!(dialog.create(&mut t), "{:?}", t.message);
    assert_eq!(last_profiles(&t), by_click);
    assert_eq!(t.bodies.len(), 1);
    assert_eq!(t.bodies[0].bounds, s.bodies[0].bounds);
}

#[test]
fn two_separate_profiles_give_two_bodies_and_adjacent_ones_one() {
    let (mut s, id) = two_rectangles();
    let id_ = s.extrude_profiles(id, &[0, 1], 3.0, Default::default());
    assert!(id_.is_ok(), "{:?}", s.message);
    assert_eq!(s.bodies.len(), 2, "getrennt: zwei Körper in einem Feature");
    assert_eq!(s.document.features().len(), 2);

    // Nebeneinander (gemeinsame Kante): ein Körper.
    let (mut s, id, ()) = sketch_with(|sk| {
        sk.add_rectangle(DVec2::ZERO, DVec2::new(10.0, 10.0))
            .unwrap();
        let a = sk.add_point(5.0, 0.0).unwrap();
        let b = sk.add_point(5.0, 10.0).unwrap();
        sk.add_line(a, b).unwrap();
    });
    let dialog = CreateDialog::ExtrudeSketch {
        sketch: id,
        profiles: vec![0, 1],
        height: 3.0,
    };
    assert!(dialog.create(&mut s), "{:?}", s.message);
    assert_eq!(s.bodies.len(), 1);
}

#[test]
fn revolve_takes_several_profiles() {
    let (mut s, id, axis) = sketch_with(|sk| -> CurveId {
        // je ein Rechteck mit einer Kante auf der Achse (Regel des Drehens)
        sk.add_rectangle(DVec2::new(0.0, 0.0), DVec2::new(2.0, 2.0))
            .unwrap();
        sk.add_rectangle(DVec2::new(0.0, 4.0), DVec2::new(3.0, 6.0))
            .unwrap();
        let a = sk.add_point(0.0, -1.0).unwrap();
        let b = sk.add_point(0.0, 7.0).unwrap();
        let axis = sk.add_line(a, b).unwrap();
        sk.set_construction(axis, true).unwrap();
        axis
    });
    let dialog = CreateDialog::RevolveSketch {
        sketch: id,
        profiles: vec![0, 1],
        axis: Some(axis),
    };
    assert!(dialog.create(&mut s), "{:?}", s.message);
    assert_eq!(s.bodies.len(), 2, "zwei Ringe");
}

// ---- Vorgabe wie Fusion -----------------------------------------------------------------------

#[test]
fn one_profile_is_preselected_several_are_not_and_create_waits() {
    let (s, id, ()) = sketch_with(|sk| {
        sk.add_rectangle(DVec2::ZERO, DVec2::new(10.0, 4.0))
            .unwrap();
    });
    assert_eq!(picked(&CreateDialog::extrude_sketch(&s, id)), vec![0]);
    assert_eq!(picked(&CreateDialog::revolve_sketch(&s, id)), vec![0]);

    let ctx = context();
    let (mut s, id) = two_rectangles();
    let mut dialog = CreateDialog::extrude_sketch(&s, id);
    assert!(picked(&dialog).is_empty());
    frame(&ctx, &mut s, &mut dialog, vec![]);
    let (shapes, _) = frame(&ctx, &mut s, &mut dialog, vec![]);
    let text = texts(&shapes);
    assert!(text.contains("Bitte ein Profil wählen"), "Hinweis:\n{text}");
    // „Erzeugen“ ist gesperrt: Der Klick legt nichts an, der Dialog bleibt.
    let at = find_text(&shapes, "Erzeugen").expect("Knopf");
    let mut open = true;
    for events in [
        vec![egui::Event::PointerMoved(at)],
        vec![button(at, true)],
        vec![button(at, false)],
    ] {
        open = frame(&ctx, &mut s, &mut dialog, events).1;
    }
    assert!(open, "bleibt offen");
    assert!(s.bodies.is_empty());
    assert!(!dialog.create(&mut s), "ohne Profil nichts erzeugt");
    assert!(
        s.message.as_deref().unwrap_or("").contains("Profil wählen"),
        "{:?}",
        s.message
    );
}

// ---- Bearbeiten -------------------------------------------------------------------------------

/// Kreis und Rechteck nebeneinander; die Extrusion wählt den Kreis.
fn extrusion_of_a_circle() -> (AppState, FeatureId, FeatureId, CurveId) {
    let (mut s, id, circle) = sketch_with(|sk| {
        sk.add_rectangle(DVec2::new(20.0, 0.0), DVec2::new(30.0, 10.0))
            .unwrap();
        let c = sk.add_point(0.0, 0.0).unwrap();
        sk.add_circle(c, 3.0).unwrap()
    });
    let circle_profile = s
        .profile_cache
        .get(s.document.sketch_of(id).unwrap())
        .curve_loops
        .iter()
        .position(|l| l == &vec![circle])
        .unwrap();
    let e = s.extrude_sketch(id, circle_profile, 2.0).unwrap();
    (s, id, e, circle)
}

#[test]
fn editing_shows_the_stored_profiles_checked() {
    let (s, _, e, _) = extrusion_of_a_circle();
    let d = FeatureDialog::new(&s, e).unwrap();
    assert_eq!(d.picked_profiles().len(), 1);
    assert!(d.picks_profiles());
}

#[test]
fn editing_a_feature_with_a_lost_profile_explains_it_and_allows_a_new_pick() {
    let ctx = context();
    let (mut s, id, e, circle) = extrusion_of_a_circle();
    s.open_sketch(id).unwrap();
    s.session_sketch_mut()
        .unwrap()
        .1
        .remove_curve(circle)
        .unwrap();
    s.close_sketch();
    assert!(matches!(
        s.document.feature(e).unwrap().status,
        FeatureStatus::Error(_)
    ));
    let rows = planora_app::properties::parameter_rows(&s, &s.document.feature(e).unwrap().kind);
    assert!(
        rows.iter().any(|(_, v)| v == "nicht mehr vorhanden"),
        "{rows:?}"
    );
    let mut d = FeatureDialog::new(&s, e).unwrap();
    assert!(d.picked_profiles().is_empty(), "verloren: nichts gewählt");
    let mut text = String::new();
    for _ in 0..3 {
        let out = ctx.run_ui(raw(vec![]), |ui| {
            d.show(ui.ctx(), &mut s);
        });
        text = planora_app::testing::collect_texts(&out);
        out.drop_without_applying_deltas();
    }
    assert!(
        text.contains("Ein Profil der Skizze ist nicht mehr vorhanden"),
        "{text}"
    );
    assert!(text.contains("Profil 1"), "{text}");
    assert_eq!(
        s.profile_pick,
        Some(ProfilePick {
            sketch: id,
            selected: vec![],
            hover: None,
            clicked: None,
        }),
        "der Dialog bietet die Profile zum Anklicken an"
    );
    d.toggle_profile(0);
    d.apply(&mut s).unwrap();
    assert_eq!(s.document.feature(e).unwrap().status, FeatureStatus::Ok);
    assert_eq!(s.bodies.len(), 1);
}

#[test]
fn editing_takes_a_click_into_the_view() {
    let ctx = context();
    let (mut s, id) = two_rectangles();
    let e = s.extrude_sketch(id, 0, 2.0).unwrap();
    aim(&mut s, false);
    let mut d = FeatureDialog::new(&s, e).unwrap();
    assert_eq!(d.picked_profiles(), [0]);
    let mut run = |s: &mut AppState, events: Vec<egui::Event>| {
        ctx.run_ui(raw(events), |ui| {
            viewport::show_in(ui, s, None);
            d.show(ui.ctx(), s);
        })
        .drop_without_applying_deltas();
    };
    run(&mut s, vec![]);
    run(&mut s, vec![]);
    let p = screen(&s, id, B);
    for events in [
        vec![egui::Event::PointerMoved(p)],
        vec![button(p, true)],
        vec![button(p, false)],
        vec![],
    ] {
        run(&mut s, events);
    }
    assert_eq!(d.picked_profiles(), [0, 1]);
    d.apply(&mut s).unwrap();
    assert_eq!(s.bodies.len(), 2);
}

// ---- Profil anzeigen --------------------------------------------------------------------------

#[test]
fn show_profile_fills_the_profiles_in_the_sketch_and_can_be_switched_off() {
    let ctx = context();
    let (mut s, id) = two_rectangles();
    assert!(s.show_sketch_profiles, "Vorgabe an");
    s.open_sketch(id).unwrap();
    let col = theme::sketch_colors(s.dark);
    let overlay = |s: &mut AppState| {
        let mut shapes = Vec::new();
        for _ in 0..2 {
            let out = ctx.run_ui(raw(vec![]), |ui| {
                sketch_view::show_overlay(ui, SCREEN, s);
            });
            shapes = all_shapes(&out.shapes);
            out.drop_without_applying_deltas();
        }
        filled_triangles(&shapes, col.profile_fill)
    };
    assert!(
        overlay(&mut s) >= 4,
        "zwei Rechtecke, je mindestens zwei Dreiecke"
    );
    s.show_sketch_profiles = false;
    assert_eq!(overlay(&mut s), 0, "aus: keine Flächen");
}

#[test]
fn the_sketch_field_has_the_show_profile_switch_on_top() {
    let ctx = context();
    let (mut s, id) = two_rectangles();
    s.open_sketch(id).unwrap();
    let mut shapes = Vec::new();
    for _ in 0..3 {
        let out = ctx.run_ui(raw(vec![]), |ui| sketch_panel::show(ui.ctx(), &mut s));
        shapes = out.shapes.clone();
        out.drop_without_applying_deltas();
    }
    let at = find_text(&shapes, "Profil anzeigen").expect("Häkchen im Skizzenfeld");
    let constraints = find_text(&shapes, "Abhängigkeiten").expect("Abschnitt");
    assert!(at.y < constraints.y, "oben im Feld");
    for events in [
        vec![egui::Event::PointerMoved(at)],
        vec![button(at, true)],
        vec![button(at, false)],
    ] {
        ctx.run_ui(raw(events), |ui| sketch_panel::show(ui.ctx(), &mut s))
            .drop_without_applying_deltas();
    }
    assert!(!s.show_sketch_profiles, "ausgeschaltet");
}

#[test]
fn without_a_profile_dialog_the_app_forgets_the_pick() {
    let mut w = planora_app::workspace::Workspace::new(None, Default::default());
    let tab = w.active_tab_mut();
    let (s, id) = two_rectangles();
    tab.state = s;
    tab.dialog = Some(CreateDialog::extrude_sketch(&tab.state, id));
    tab.state.profile_pick = Some(ProfilePick {
        sketch: id,
        selected: vec![0],
        hover: None,
        clicked: None,
    });
    planora_app::app::clear_profile_pick_without_dialog(tab);
    assert!(tab.state.profile_pick.is_some(), "Dialog offen: bleibt");
    tab.dialog = None;
    planora_app::app::clear_profile_pick_without_dialog(tab);
    assert!(tab.state.profile_pick.is_none(), "ohne Dialog: gelöscht");
}
