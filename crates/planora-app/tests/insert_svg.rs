//! Fenster „SVG einfügen“ (Skizzen 9, Task 12): Start mit der Hüllquader-Mitte auf dem Ursprung,
//! Felder wie in Fusion (Y-Abstand positiv nach unten, Z-Winkel positiv im Uhrzeigersinn),
//! Spiegeln, Griffe auf der Vorschau in beiden Richtungen mit den Feldern verbunden, fest
//! eingefügt in einem Rückgängig-Schritt, Hinweis zu Texten, Esc ohne Spur und der
//! Kontrollpunktrahmen in der Ansicht.
//!
//! Alle SVG-Dateien schreibt der Test selbst; `viewBox` und Breite in mm machen 1 Einheit = 1 mm.

mod common;

use common::{Screen, find};
use egui::{Event, Key, Modifiers, PointerButton, Pos2, Rect, pos2};
use planora_app::insert::DialogAction;
use planora_app::insert::pick::InsertTarget;
use planora_app::insert::svg_dialog::SvgDialog;
use planora_app::keymap::Action;
use planora_app::plane_dialog::PlaneClick;
use planora_app::sketch_grips::{Grip, GripDrag, grip_motion};
use planora_app::sketch_view;
use planora_app::state::*;
use planora_app::workspace::Workspace;
use planora_feature::SketchPlane;
use planora_math::DVec2;
use planora_project::config::Config;
use planora_project::library::Library;
use planora_sketch::constraint::ConstraintKind as K;
use planora_sketch::model::{PointId, Sketch};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

const W: f32 = 1920.0;
const H: f32 = 1080.0;
const SCREEN: Rect = Rect {
    min: pos2(0.0, 0.0),
    max: pos2(W, H),
};

/// SVG mit 1 Einheit = 1 mm.
fn svg(body: &str) -> Vec<u8> {
    format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="100mm" height="100mm" viewBox="0 0 100 100">{body}</svg>"#
    )
    .into_bytes()
}

/// Rechteck von (10, 10) bis (30, 20) in der Datei: in der Skizze 20 × 10 mm, Mitte (20, -15).
fn rect_svg() -> Vec<u8> {
    svg(r#"<path d="M 10 10 L 30 10 L 30 20 L 10 20 Z" fill="none" stroke="black"/>"#)
}

fn temp_dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("planora-svg-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

fn write(dir: &Path, name: &str, bytes: &[u8]) -> PathBuf {
    let p = dir.join(name);
    fs::write(&p, bytes).unwrap();
    p
}

fn workspace() -> Workspace {
    let root = common::temp_root("planora-svg-ui");
    Workspace::new(Some(Library::open(&root).unwrap()), Config::default())
}

fn loaded(dir: &Path, path: PathBuf, state: &AppState) -> SvgDialog {
    let mut d = SvgDialog::new(dir.to_path_buf(), state);
    d.load(path);
    d.tick(true);
    assert!(d.loaded.is_some(), "gelesen: {:?}", d.error_text());
    d
}

fn show_on<'a>(
    d: &'a mut SvgDialog,
    s: &'a mut AppState,
    last: &'a mut DialogAction,
) -> impl FnMut(&egui::Context) + 'a {
    let mut dir = None;
    move |ctx| {
        if *last == DialogAction::Open {
            *last = d.show(
                ctx,
                s,
                &mut dir,
                &mut planora_app::places::Places::default(),
            );
        }
    }
}

/// Gleich bis auf die Rechengenauigkeit von usvg (f32).
fn near(a: DVec2, b: DVec2) -> bool {
    (a - b).length() < 1e-5
}

/// Hülle aller Punkte einer Skizze.
fn hull(sk: &Sketch) -> (DVec2, DVec2) {
    sk.points().filter_map(|(p, _)| sk.point_xy(p).ok()).fold(
        (DVec2::splat(f64::MAX), DVec2::splat(f64::MIN)),
        |(lo, hi), p| (lo.min(p), hi.max(p)),
    )
}

fn fixed_points(sk: &Sketch) -> HashSet<PointId> {
    sk.constraints()
        .filter_map(|(_, d)| match d.kind {
            K::Fixed(p, ..) => Some(p),
            _ => None,
        })
        .collect()
}

/// Mitte des Rechtecks in der Datei (Skizzenkoordinaten, Y schon umgedreht).
fn rect_bounds() -> (DVec2, DVec2) {
    (DVec2::new(10.0, -20.0), DVec2::new(30.0, -10.0))
}

fn open_xy(s: &mut AppState) -> planora_feature::FeatureId {
    let id = s
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    s.open_sketch(id).unwrap();
    s.observe_sketch(false);
    id
}

// ---- Lage ---------------------------------------------------------------------------------------

#[test]
fn svg_starts_centered_on_origin() {
    let dir = temp_dir("mitte");
    let path = write(&dir, "r.svg", &rect_svg());
    let mut s = AppState::new();
    let mut d = loaded(&dir, path, &s);
    // Vorgaben: kein Abstand, kein Winkel, Maßstab 1 (Dateigröße in mm), nicht gespiegelt.
    assert_eq!(d.offset, DVec2::ZERO);
    assert_eq!(d.angle_deg, 0.0);
    assert_eq!(d.scale, 1.0);
    assert!(!d.flip_h && !d.flip_v && !d.frame);
    let place = d.placement(rect_bounds());
    assert!(near(place.apply(DVec2::new(20.0, -15.0)), DVec2::ZERO));
    // Eingefügt: Hülle 20 × 10 mm um den Ursprung.
    assert!(
        d.target
            .take_click(&s, PlaneClick::Standard(StandardPlane::XY))
    );
    d.insert(&mut s).unwrap();
    let id = s.sketch_features()[0].0;
    let (lo, hi) = hull(s.document.sketch_of(id).unwrap());
    assert!(near(lo, DVec2::new(-10.0, -5.0)), "{lo:?}");
    assert!(near(hi, DVec2::new(10.0, 5.0)), "{hi:?}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn field_y_positive_moves_up() {
    let dir = temp_dir("y");
    let path = write(&dir, "r.svg", &rect_svg());
    let s = AppState::new();
    let mut d = loaded(&dir, path, &s);
    d.offset = DVec2::new(5.0, 10.0);
    let place = d.placement(rect_bounds());
    // X nach rechts, Y positiv nach oben (Dennis' Festlegung, wie die Skizzenachse).
    assert!(near(
        place.apply(DVec2::new(20.0, -15.0)),
        DVec2::new(5.0, 10.0)
    ));

    // Über das Feld: „10“ im Y-Feld schiebt die Vorschau nach oben.
    let mut s = AppState::new();
    let mut d = loaded(&dir, dir.join("r.svg"), &s);
    d.target
        .take_click(&s, PlaneClick::Standard(StandardPlane::XY));
    let mut last = DialogAction::Open;
    let mut screen = Screen::new(W, H);
    screen.settle(3, &mut show_on(&mut d, &mut s, &mut last));
    d.texts[1] = "10".into();
    screen.settle(2, &mut show_on(&mut d, &mut s, &mut last));
    assert_eq!(d.offset, DVec2::new(0.0, 10.0));
    let g = s.insert_grips.as_ref().expect("Griffe");
    assert!(near(g.offset, DVec2::new(0.0, 10.0)), "{:?}", g.offset);
    let lines = &s.insert_preview.as_ref().expect("Vorschau").lines;
    let max_y = lines.iter().flatten().map(|p| p.y).fold(f64::MIN, f64::max);
    assert!((max_y - 15.0).abs() < 1e-5, "{max_y}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn angle_positive_is_clockwise() {
    let dir = temp_dir("winkel");
    let path = write(&dir, "r.svg", &rect_svg());
    let s = AppState::new();
    let mut d = loaded(&dir, path, &s);
    d.angle_deg = 90.0;
    let place = d.placement(rect_bounds());
    // Rechts der Mitte landet unten: im Uhrzeigersinn.
    let right = DVec2::new(30.0, -15.0);
    assert!(
        near(place.apply(right), DVec2::new(0.0, -10.0)),
        "{:?}",
        place.apply(right)
    );
    // Drehen um die Mitte, dann verschieben.
    d.offset = DVec2::new(1.0, 2.0);
    let place = d.placement(rect_bounds());
    assert!(near(place.apply(right), DVec2::new(1.0, -8.0)));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn flip_h_mirrors_about_y_axis() {
    let dir = temp_dir("spiegeln");
    let path = write(&dir, "r.svg", &rect_svg());
    let s = AppState::new();
    let mut d = loaded(&dir, path, &s);
    let p = DVec2::new(20.0 + 10.0, -15.0 + 3.0);
    d.flip_h = true;
    assert!(near(
        d.placement(rect_bounds()).apply(p),
        DVec2::new(-10.0, 3.0)
    ));
    d.flip_h = false;
    d.flip_v = true;
    assert!(near(
        d.placement(rect_bounds()).apply(p),
        DVec2::new(10.0, -3.0)
    ));
    // Maßstab: Faktor auf die Dateigröße.
    d.flip_v = false;
    d.scale = 2.0;
    assert!(near(
        d.placement(rect_bounds()).apply(p),
        DVec2::new(20.0, 6.0)
    ));
    let _ = fs::remove_dir_all(&dir);
}

// ---- Griffe -----------------------------------------------------------------------------------

fn context() -> egui::Context {
    let ctx = egui::Context::default();
    ctx.set_fonts(planora_app::fonts::definitions());
    ctx
}

fn overlay(ctx: &egui::Context, s: &mut AppState, events: Vec<Event>) {
    let raw = egui::RawInput {
        screen_rect: Some(SCREEN),
        events,
        ..Default::default()
    };
    ctx.run_ui(raw, |ui| {
        sketch_view::show_overlay(ui, SCREEN, s);
    })
    .drop_without_applying_deltas();
}

fn left(p: Pos2, pressed: bool) -> Event {
    Event::PointerButton {
        pos: p,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    }
}

fn to_screen(s: &AppState, p: DVec2) -> Pos2 {
    let frame = *s.session_sketch().unwrap().frame();
    let mut cam = s.camera;
    cam.set_viewport(W, H);
    let q = sketch_view::sketch_to_screen(&cam, &frame, glam::Vec2::new(W, H), p).unwrap();
    pos2(q.x, q.y)
}

#[test]
fn grip_drag_updates_fields_and_back() {
    let dir = temp_dir("griffe");
    let path = write(&dir, "r.svg", &rect_svg());
    let mut s = AppState::new();
    open_xy(&mut s);
    let mut d = loaded(&dir, path, &s);
    let mut last = DialogAction::Open;
    let mut screen = Screen::new(W, H);
    screen.settle(3, &mut show_on(&mut d, &mut s, &mut last));
    let g = s.insert_grips.clone().expect("Griffe");
    assert_eq!((g.offset, g.angle, g.factor), (DVec2::ZERO, 0.0, 1.0));

    // Die reine Rechnung: ein Zug um (10, 0) am freien Griff ergibt X-Abstand 10.
    let drag = GripDrag {
        grip: Grip::Free,
        press: DVec2::ZERO,
        start_offset: g.offset,
        start_angle: g.angle,
        start_factor: g.factor,
    };
    let (o, a, f) = grip_motion(&drag, g.offset, DVec2::new(10.0, 0.0));
    {
        let g = s.insert_grips.as_mut().unwrap();
        (g.offset, g.angle, g.factor, g.moved) = (o, a, f, true);
    }
    screen.settle(1, &mut show_on(&mut d, &mut s, &mut last));
    assert_eq!(d.offset, DVec2::new(10.0, 0.0));
    assert_eq!(d.texts[0], "10");

    // Feld auf 20: der Griff steht an der neuen Stelle.
    d.texts[0] = "20".into();
    screen.settle(1, &mut show_on(&mut d, &mut s, &mut last));
    assert_eq!(d.offset.x, 20.0);
    let ctx = context();
    overlay(&ctx, &mut s, vec![]);
    let (main, _) = sketch_view::insert_grip_layout(&s, SCREEN).expect("Griffe");
    let want = to_screen(&s, DVec2::new(20.0, 0.0));
    assert!(
        main.center.distance(want) < 0.5,
        "{:?} {want:?}",
        main.center
    );

    // Über Rohereignisse: der Y-Pfeil zieht nur Y (im Feld positiv nach unten); die Skizze
    // bleibt unberührt.
    let curves = s.session_sketch().unwrap().curves().count();
    let from = main.center + (main.y_tip - main.center) * 0.6;
    let to = to_screen(&s, DVec2::new(20.0, 0.0) + DVec2::new(0.0, 7.0)) + (from - main.center);
    overlay(&ctx, &mut s, vec![Event::PointerMoved(from)]);
    assert_eq!(s.insert_grips.as_ref().unwrap().hot, Some(Grip::AxisY));
    overlay(&ctx, &mut s, vec![left(from, true)]);
    for i in 1..=8 {
        let t = i as f32 / 8.0;
        overlay(
            &ctx,
            &mut s,
            vec![Event::PointerMoved(from + (to - from) * t)],
        );
    }
    overlay(&ctx, &mut s, vec![left(to, false)]);
    overlay(&ctx, &mut s, vec![]);
    assert!(
        s.insert_grips.as_ref().unwrap().drag.is_none(),
        "Zug beendet"
    );
    screen.settle(1, &mut show_on(&mut d, &mut s, &mut last));
    assert!((d.offset.x - 20.0).abs() < 1e-9, "{:?}", d.offset);
    assert!((d.offset.y - 7.0).abs() < 0.05, "{:?}", d.offset);
    assert_eq!(s.session_sketch().unwrap().curves().count(), curves);
    assert_eq!(last, DialogAction::Open);

    // Der Ring dreht um die Mitte: ein Viertel gegen den Uhrzeigersinn ist -90° im Feld.
    overlay(&ctx, &mut s, vec![]);
    let (main, _) = sketch_view::insert_grip_layout(&s, SCREEN).unwrap();
    // Am Ring oben rechts (45°) greifen, dort liegt kein Pfeil.
    let r = main.ring;
    let at = |deg: f32| {
        let t = deg.to_radians();
        main.center + egui::vec2(r * t.cos(), -r * t.sin())
    };
    let from = at(45.0);
    overlay(&ctx, &mut s, vec![Event::PointerMoved(from)]);
    assert_eq!(s.insert_grips.as_ref().unwrap().hot, Some(Grip::Rotate));
    overlay(&ctx, &mut s, vec![left(from, true)]);
    for i in 1..=12 {
        overlay(
            &ctx,
            &mut s,
            vec![Event::PointerMoved(at(45.0 + 90.0 * i as f32 / 12.0))],
        );
    }
    overlay(&ctx, &mut s, vec![left(at(135.0), false)]);
    screen.settle(1, &mut show_on(&mut d, &mut s, &mut last));
    // Bis auf die Zeigerlage in Bildpunkten (Ring 40 px).
    assert!((d.angle_deg - -90.0).abs() < 0.2, "{}", d.angle_deg);

    // Die Handhabe des Maßstabs: doppelter Abstand zur Mitte ist Faktor 2.
    overlay(&ctx, &mut s, vec![]);
    let (main, scale) = sketch_view::insert_grip_layout(&s, SCREEN).expect("Griffe");
    let from = scale.x_tip;
    let to = main.center + (from - main.center) * 2.0;
    overlay(&ctx, &mut s, vec![Event::PointerMoved(from)]);
    assert_eq!(s.insert_grips.as_ref().unwrap().hot, Some(Grip::Scale));
    overlay(&ctx, &mut s, vec![left(from, true)]);
    for i in 1..=6 {
        let t = i as f32 / 6.0;
        overlay(
            &ctx,
            &mut s,
            vec![Event::PointerMoved(from + (to - from) * t)],
        );
    }
    overlay(&ctx, &mut s, vec![left(to, false)]);
    screen.settle(1, &mut show_on(&mut d, &mut s, &mut last));
    assert!((d.scale - 2.0).abs() < 1e-3, "{}", d.scale);
    assert_eq!(
        d.texts[3],
        planora_app::formula_field::value_field_text(d.scale, planora_feature::ParamUnit::Number)
    );
    let _ = fs::remove_dir_all(&dir);
}

// ---- Einfügen ---------------------------------------------------------------------------------

#[test]
fn svg_ok_is_one_sketch_undo_step_and_fixed() {
    let dir = temp_dir("offen");
    let path = write(&dir, "r.svg", &rect_svg());
    let mut ws = workspace();
    let sketch = {
        let s = &mut ws.active_tab_mut().state;
        let id = open_xy(s);
        s.session_sketch_mut()
            .unwrap()
            .1
            .add_rectangle(DVec2::new(50.0, 50.0), DVec2::new(54.0, 53.0))
            .unwrap();
        id
    };
    ws.observe_history(false);
    let tab = ws.active_tab_mut();
    let before: HashSet<PointId> = tab
        .state
        .session_sketch()
        .unwrap()
        .points()
        .map(|(p, _)| p)
        .collect();
    let mut d = loaded(&dir, path, &tab.state);
    assert_eq!(d.target.target(), Some(InsertTarget::OpenSketch));
    d.insert(&mut tab.state).unwrap();
    assert_eq!(tab.state.sketch_features().len(), 1, "keine neue Skizze");
    let sk = tab.state.session_sketch().unwrap();
    assert_eq!(sk.curves().count(), 4 + 4);
    // Alle neuen Punkte sind fest (wie Fusion), die alten nicht.
    let fixed = fixed_points(sk);
    let new: Vec<PointId> = sk
        .points()
        .map(|(p, _)| p)
        .filter(|p| !before.contains(p))
        .collect();
    assert!(!new.is_empty());
    assert!(new.iter().all(|p| fixed.contains(p)), "alle fest");
    assert!(before.iter().all(|p| !fixed.contains(p)));
    ws.observe_history(false);
    assert_eq!(
        ws.active_tab().undo_label(),
        Some("SVG einfügen".to_string())
    );
    let tab = ws.active_tab_mut();
    tab.undo().unwrap();
    assert_eq!(tab.state.session_sketch().unwrap().curves().count(), 4);
    assert_eq!(tab.state.open_sketch_id(), Some(sketch));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn svg_without_open_sketch_creates_sketch() {
    let dir = temp_dir("neu");
    let path = write(&dir, "r.svg", &rect_svg());
    let mut ws = workspace();
    let tab = ws.active_tab_mut();
    let mut d = loaded(&dir, path, &tab.state);
    assert!(d.target.target().is_none());
    assert!(d.insert(&mut tab.state).is_err(), "ohne Ebene nichts");
    assert!(
        d.target
            .take_click(&tab.state, PlaneClick::Standard(StandardPlane::XZ))
    );
    d.insert(&mut tab.state).unwrap();
    ws.observe_history(false);
    let s = &ws.active_tab().state;
    let names: Vec<String> = s.sketch_features().into_iter().map(|(_, n)| n).collect();
    assert_eq!(names, ["Skizze 1"]);
    let sk = s.document.sketch_of(s.sketch_features()[0].0).unwrap();
    assert_eq!(sk.curves().count(), 4);
    let fixed = fixed_points(sk);
    assert!(sk.points().all(|(p, _)| fixed.contains(&p)), "fest");
    let tab = ws.active_tab_mut();
    tab.undo().unwrap();
    assert!(tab.state.sketch_features().is_empty(), "ein Schritt");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn svg_text_skipped_hint() {
    let dir = temp_dir("text");
    let body =
        r#"<text x="10" y="10">Hallo</text><path d="M 0 0 L 10 0" fill="none" stroke="black"/>"#;
    let path = write(&dir, "t.svg", &svg(body));
    let mut s = AppState::new();
    let mut d = loaded(&dir, path, &s);
    d.target
        .take_click(&s, PlaneClick::Standard(StandardPlane::XY));
    d.insert(&mut s).unwrap();
    assert_eq!(
        s.info.as_deref(),
        Some("1 Text übersprungen. Text vorher in Pfade umwandeln.")
    );
    assert!(s.info_outlives_step);
    // Ohne Text kein Hinweis.
    let path = write(&dir, "r.svg", &rect_svg());
    let mut s = AppState::new();
    let mut d = loaded(&dir, path, &s);
    d.target
        .take_click(&s, PlaneClick::Standard(StandardPlane::XY));
    d.insert(&mut s).unwrap();
    assert_eq!(s.info, None);
    // Englisch.
    let text = planora_i18n::with_lang(&planora_i18n::english(), || {
        let path = dir.join("t.svg");
        let mut s = AppState::new();
        let mut d = loaded(&dir, path, &s);
        d.target
            .take_click(&s, PlaneClick::Standard(StandardPlane::XY));
        d.insert(&mut s).unwrap();
        s.info.unwrap_or_default()
    });
    assert_eq!(text, "Skipped 1 text. Convert text to paths first.");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn esc_leaves_nothing() {
    let dir = temp_dir("esc");
    let path = write(&dir, "r.svg", &rect_svg());
    let mut s = AppState::new();
    s.select_origin_plane(StandardPlane::XY);
    let before = s.snapshot();
    let mut d = loaded(&dir, path, &s);
    let mut last = DialogAction::Open;
    let mut screen = Screen::new(W, H);
    let seen = screen.settle(4, &mut show_on(&mut d, &mut s, &mut last));
    assert!(s.insert_preview.is_some() && s.insert_grips.is_some());
    for label in [
        "X-Abstand",
        "Y-Abstand",
        "Z-Winkel",
        "Horizontal spiegeln",
        "Vertikal spiegeln",
        "Skalierung",
        "Kontrollpunktrahmen",
    ] {
        assert!(find(&seen, label).is_some(), "„{label}“ fehlt");
    }
    screen.frame(
        vec![Event::Key {
            key: Key::Escape,
            physical_key: Some(Key::Escape),
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        }],
        &mut show_on(&mut d, &mut s, &mut last),
    );
    assert_eq!(last, DialogAction::Closed);
    assert!(s.snapshot().same_content(&before), "nichts geändert");
    assert!(s.sketch_features().is_empty());
    assert!(
        s.insert_preview.is_none() && s.insert_grips.is_none() && s.plane_pick.is_none(),
        "aufgeräumt"
    );

    // Abbrechen ebenso; die Taste öffnet das Fenster.
    let mut d = loaded(&dir, dir.join("r.svg"), &s);
    let mut last = DialogAction::Open;
    let seen = screen.settle(3, &mut show_on(&mut d, &mut s, &mut last));
    let cancel = find(&seen, "Abbrechen").expect("Abbrechen").rect;
    screen.click(cancel.center(), &mut show_on(&mut d, &mut s, &mut last));
    assert_eq!(last, DialogAction::Closed);
    assert!(s.snapshot().same_content(&before));
    assert!(s.insert_grips.is_none());
    let mut ws = workspace();
    planora_app::commands::run_action(&mut ws, Action::InsertSvg);
    assert!(ws.active_tab().svg_dialog.is_some());
    let _ = fs::remove_dir_all(&dir);
}

// ---- Kontrollpunktrahmen --------------------------------------------------------------------------

#[test]
fn show_frame_curves_draw_polygon() {
    let dir = temp_dir("rahmen");
    let body = r#"<path d="M 0 0 C 10 -20 30 -20 40 0" fill="none" stroke="black"/>"#;
    let path = write(&dir, "c.svg", &svg(body));
    for frame in [true, false] {
        let mut s = AppState::new();
        open_xy(&mut s);
        let mut d = loaded(&dir, path.clone(), &s);
        d.frame = frame;
        d.insert(&mut s).unwrap();
        let se = s.sketch_session.as_ref().unwrap();
        assert!(se.selection.is_empty(), "nichts gewählt");
        let sk = s.session_sketch().unwrap();
        let guides = sketch_view::curve_guides(se, sk);
        if frame {
            // Ohne Auswahl immer da: das Kontrollpolygon der kubischen Kurve.
            assert_eq!(guides.len(), 1, "{guides:?}");
            assert_eq!(guides[0].len(), 4);
            assert_eq!(sketch_view::frame_polygons(sk).len(), 1);
        } else {
            assert!(guides.is_empty(), "{guides:?}");
            assert!(sketch_view::frame_polygons(sk).is_empty());
        }
    }
    let _ = fs::remove_dir_all(&dir);
}

// ---- Abbruch des Zugs ---------------------------------------------------------------------------

fn esc() -> Event {
    Event::Key {
        key: Key::Escape,
        physical_key: Some(Key::Escape),
        pressed: true,
        repeat: false,
        modifiers: Modifiers::NONE,
    }
}

#[test]
fn esc_or_right_click_aborts_only_the_grip_drag() {
    let dir = temp_dir("zugabbruch");
    let path = write(&dir, "r.svg", &rect_svg());
    let mut s = AppState::new();
    open_xy(&mut s);
    let mut d = loaded(&dir, path, &s);
    let mut last = DialogAction::Open;
    let mut screen = Screen::new(W, H);
    screen.settle(3, &mut show_on(&mut d, &mut s, &mut last));
    let ctx = context();
    overlay(&ctx, &mut s, vec![]);
    for abort in ["esc", "rechts"] {
        let (main, _) = sketch_view::insert_grip_layout(&s, SCREEN).expect("Griffe");
        let from = main.center;
        let to = from + egui::vec2(150.0, 60.0);
        overlay(&ctx, &mut s, vec![Event::PointerMoved(from)]);
        overlay(&ctx, &mut s, vec![left(from, true)]);
        for i in 1..=5 {
            let t = i as f32 / 5.0;
            overlay(
                &ctx,
                &mut s,
                vec![Event::PointerMoved(from + (to - from) * t)],
            );
        }
        // Die Felder folgen dem Zug.
        screen.settle(1, &mut show_on(&mut d, &mut s, &mut last));
        assert!(
            d.offset.x > 1.0 && d.offset.y < -1.0,
            "{abort}: {:?}",
            d.offset
        );
        // Abbruch: Ansicht und Fenster sehen dasselbe Ereignis im selben Bild.
        let event = if abort == "esc" {
            esc()
        } else {
            Event::PointerButton {
                pos: to,
                button: PointerButton::Secondary,
                pressed: true,
                modifiers: Modifiers::NONE,
            }
        };
        overlay(&ctx, &mut s, vec![event.clone()]);
        assert!(s.insert_grips.as_ref().unwrap().drag.is_none(), "{abort}");
        screen.frame(vec![event], &mut show_on(&mut d, &mut s, &mut last));
        assert_eq!(last, DialogAction::Open, "{abort}: Fenster bleibt offen");
        assert_eq!(d.offset, DVec2::ZERO, "{abort}: zurück am Anfang");
        assert_eq!((d.angle_deg, d.scale), (0.0, 1.0));
        assert_eq!(d.texts[0], "0");
        // Loslassen danach ändert nichts mehr.
        overlay(&ctx, &mut s, vec![left(to, false)]);
        overlay(&ctx, &mut s, vec![]);
        screen.settle(1, &mut show_on(&mut d, &mut s, &mut last));
        assert_eq!(d.offset, DVec2::ZERO, "{abort}");
        assert_eq!(s.session_sketch().unwrap().curves().count(), 0);
    }
    // Esc ohne Zug schließt das Fenster wie bisher.
    overlay(&ctx, &mut s, vec![esc()]);
    screen.frame(vec![esc()], &mut show_on(&mut d, &mut s, &mut last));
    assert_eq!(last, DialogAction::Closed);
    assert!(s.insert_grips.is_none());
    let _ = fs::remove_dir_all(&dir);
}
