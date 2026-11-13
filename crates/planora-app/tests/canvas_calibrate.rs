//! Skizzen 9, Task 14: „Kalibrieren“ einer Leinwand. Rechtsklick auf die Leinwand im Ordner
//! „Leinwände“ ⇒ Klickmodus in der Ansicht: zwei Punkte auf dem Bild, dann der wahre Abstand im
//! Feld am Zeiger (mit Einheiten und Formeln), Enter skaliert um den ersten Punkt in einem
//! Rückgängig-Schritt. Esc und Rechtsklick brechen ohne Spur ab; gesperrt, solange eine Skizze
//! offen ist. Bilder bei 1920×1080, die Bilder schreibt der Test selbst.

mod common;

use egui::{Event, Key, Modifiers, PointerButton, Pos2, Rect, pos2};
use glam::{Vec2, Vec3};
use planora_app::browser;
use planora_app::insert::calibrate::{self, calibrated, calibrated_scale};
use planora_app::keymap::{Action, Area};
use planora_app::state::AppState;
use planora_app::viewport;
use planora_app::workspace::Workspace;
use planora_feature::{
    CanvasBase, CanvasFeature, CanvasImage, FeatureId, FeatureKind, SketchPlane, StandardPlane,
    image_hash,
};
use planora_math::{DVec2, DVec3, Frame};
use planora_project::config::Config;
use planora_project::library::Library;
use std::sync::Arc;

const W: f32 = 1920.0;
const H: f32 = 1080.0;
const VIEW: Vec2 = Vec2::new(W, H);
const SCREEN: Rect = Rect {
    min: pos2(0.0, 0.0),
    max: pos2(W, H),
};

/// PNG `w` × `h`, deckend; die linke Hälfte ganz durchsichtig (Kalibrieren trifft trotzdem).
fn png(w: u32, h: u32) -> Vec<u8> {
    let mut data = Vec::with_capacity((w * h * 4) as usize);
    for _y in 0..h {
        for x in 0..w {
            let a = if x < w / 2 { 0 } else { 255 };
            data.extend_from_slice(&[30, 120, 200, a]);
        }
    }
    let mut out = Vec::new();
    {
        let mut e = png::Encoder::new(&mut out, w, h);
        e.set_color(png::ColorType::Rgba);
        e.set_depth(png::BitDepth::Eight);
        let mut wr = e.write_header().unwrap();
        wr.write_image_data(&data).unwrap();
    }
    out
}

fn image(w: u32, h: u32) -> CanvasImage {
    let bytes = png(w, h);
    CanvasImage {
        hash: image_hash(&bytes),
        bytes: Arc::from(bytes),
        format: planora_exchange::ImageFormat::Png,
        width: w,
        height: h,
    }
}

fn workspace() -> Workspace {
    let root = common::temp_root("planora-calibrate");
    Workspace::new(Some(Library::open(&root).unwrap()), Config::default())
}

/// Leinwand 64 × 32 px auf XY, gedreht, verschoben und horizontal gespiegelt.
fn insert(s: &mut AppState) -> FeatureId {
    let mut f = CanvasFeature::new(
        planora_feature::ImageId(0),
        CanvasBase::Standard(StandardPlane::XY),
    );
    f.angle = 0.3;
    f.offset = DVec2::new(3.0, -2.0);
    f.flip_h = true;
    s.insert_canvas(image(64, 32), f).unwrap()
}

fn canvas(s: &AppState, id: FeatureId) -> CanvasFeature {
    match &s.document.feature(id).unwrap().kind {
        FeatureKind::Canvas(c) => c.clone(),
        k => panic!("keine Leinwand: {k:?}"),
    }
}

fn corners(s: &AppState, id: FeatureId) -> [DVec3; 4] {
    let c = canvas(s, id);
    let img = s.document.image(c.image).unwrap();
    c.corners(&s.document.canvas_frame(id).unwrap(), img)
}

/// Punkt des Bildes bei `(u, v)` (0..=1 von oben links) aus den Ecken.
fn at(k: [DVec3; 4], u: f64, v: f64) -> DVec3 {
    k[0] + (k[1] - k[0]) * u + (k[3] - k[0]) * v
}

/// `(u, v)` eines Punktes der Ebene des Bildes aus den Ecken.
fn uv_of(k: [DVec3; 4], p: DVec3) -> (f64, f64) {
    let (eu, ev, w) = (k[1] - k[0], k[3] - k[0], p - k[0]);
    let (uu, uv, vv) = (eu.dot(eu), eu.dot(ev), ev.dot(ev));
    let det = uu * vv - uv * uv;
    let (wu, wv) = (w.dot(eu), w.dot(ev));
    ((wu * vv - wv * uv) / det, (wv * uu - wu * uv) / det)
}

fn look_down(s: &mut AppState, r: f32) {
    s.camera.set_viewport(W, H);
    s.camera.set_view(planora_render::ViewFace::Top);
    s.camera
        .frame_bounds(Vec3::new(-r, -r, -0.1), Vec3::new(r, r, 0.1));
}

fn screen_of(s: &AppState, p: DVec3) -> Pos2 {
    let v = s.camera.project(p.as_vec3(), VIEW).unwrap();
    pos2(v.x, v.y)
}

// ---- Bilder der Ansicht mit einem bleibenden egui-Kontext -----------------------------------

struct View {
    ctx: egui::Context,
    time: f64,
}

impl View {
    fn new() -> View {
        let ctx = egui::Context::default();
        ctx.set_fonts(planora_app::fonts::definitions());
        View { ctx, time: 0.0 }
    }

    fn frame(&mut self, s: &mut AppState, events: Vec<Event>) -> String {
        self.time += 0.05;
        let raw = egui::RawInput {
            screen_rect: Some(SCREEN),
            time: Some(self.time),
            events,
            ..Default::default()
        };
        let out = self.ctx.run_ui(raw, |ui| viewport::show_in(ui, s, None));
        let texts = planora_app::testing::collect_texts(&out);
        out.drop_without_applying_deltas();
        texts
    }

    fn click(&mut self, s: &mut AppState, p: Pos2, b: PointerButton) {
        let button = |pressed| Event::PointerButton {
            pos: p,
            button: b,
            pressed,
            modifiers: Modifiers::NONE,
        };
        for e in [
            vec![Event::PointerMoved(p)],
            vec![button(true)],
            vec![button(false)],
            vec![],
        ] {
            self.frame(s, e);
        }
    }

    fn key(&mut self, s: &mut AppState, key: Key) {
        let ev = |pressed| Event::Key {
            key,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers: Modifiers::NONE,
        };
        self.frame(s, vec![ev(true)]);
        self.frame(s, vec![ev(false)]);
        self.frame(s, vec![]);
    }

    fn text(&mut self, s: &mut AppState, t: &str) {
        self.frame(s, vec![Event::Text(t.to_string())]);
        self.frame(s, vec![]);
    }
}

/// Startet das Kalibrieren und klickt die Bildpunkte `(0.25, 0.5)` und `(0.75, 0.5)` an (32 mm
/// auseinander); liefert die Weltlage des ersten Punktes.
fn pick_two(v: &mut View, s: &mut AppState, id: FeatureId) -> DVec3 {
    s.start_calibrate(id).unwrap();
    let k = corners(s, id);
    let a = screen_of(s, at(k, 0.25, 0.5));
    let b = screen_of(s, at(k, 0.75, 0.5));
    v.click(s, a, PointerButton::Primary);
    let first = s.calibrate.as_ref().unwrap().first.expect("erster Punkt");
    v.click(s, b, PointerButton::Primary);
    assert!(
        s.calibrate.as_ref().unwrap().second.is_some(),
        "zweiter Punkt"
    );
    s.calibrate_world(first).unwrap()
}

#[test]
fn calibrated_scale_doubles() {
    // 10 mm gemessen, 20 mm eingegeben: Faktor 2, gleich für X und Y.
    let a = DVec2::new(0.0, 0.0);
    let b = DVec2::new(6.0, 8.0);
    assert_eq!(
        calibrated_scale(DVec2::ONE, a, b, 20.0),
        Some(DVec2::splat(2.0))
    );
    assert_eq!(
        calibrated_scale(DVec2::new(0.5, 1.5), a, b, 20.0),
        Some(DVec2::new(1.0, 3.0))
    );
}

#[test]
fn first_point_stays_fixed() {
    // Rein: gedrehte, gespiegelte, verschobene Leinwand auf XY.
    let mut f = CanvasFeature::new(
        planora_feature::ImageId(1),
        CanvasBase::Standard(StandardPlane::XY),
    );
    f.angle = 0.7;
    f.offset = DVec2::new(10.0, 5.0);
    f.scale = DVec2::new(0.5, 0.8);
    f.flip_v = true;
    let img = image(40, 20);
    let base = Frame {
        origin: DVec3::ZERO,
        x: DVec3::X,
        y: DVec3::Y,
        z: DVec3::Z,
    };
    let k = f.corners(&f.placed_frame(&base), &img);
    let (pa, pb) = (at(k, 0.2, 0.3), at(k, 0.8, 0.6));
    let (a, b) = (pa.truncate(), pb.truncate());
    let new = calibrated(&f, a, b, 3.0 * a.distance(b)).unwrap();
    assert!((new.scale - DVec2::new(1.5, 2.4)).length() < 1e-12);
    assert_eq!((new.angle, new.flip_h, new.flip_v), (0.7, false, true));
    let k2 = new.corners(&new.placed_frame(&base), &img);
    assert!((at(k2, 0.2, 0.3) - pa).length() < 1e-9, "a bleibt");
    assert!(
        (at(k2, 0.8, 0.6).distance(pa) - 3.0 * pb.distance(pa)).abs() < 1e-9,
        "b rückt auf den Abstand"
    );

    // In der App: über die Ansicht angeklickt.
    let mut s = AppState::new();
    let id = insert(&mut s);
    look_down(&mut s, 60.0);
    let mut v = View::new();
    let world = pick_two(&mut v, &mut s, id);
    let (u0, v0) = uv_of(corners(&s, id), world);
    v.text(&mut s, "64");
    v.key(&mut s, Key::Enter);
    assert!(s.calibrate.is_none(), "Enter beendet den Modus");
    let c = canvas(&s, id);
    assert!((c.scale.x - 2.0).abs() < 1e-2 && c.scale.x == c.scale.y);
    assert!(
        c.flip_h && !c.flip_v && c.angle == 0.3,
        "Drehen und Spiegeln bleiben"
    );
    assert!(
        (at(corners(&s, id), u0, v0) - world).length() < 1e-9,
        "der erste Punkt bleibt an seiner Stelle"
    );
}

#[test]
fn zero_distance_or_same_point_rejected() {
    let a = DVec2::new(1.0, 2.0);
    assert_eq!(calibrated_scale(DVec2::ONE, a, a, 10.0), None);
    for d in [0.0, -3.0, f64::NAN] {
        assert_eq!(calibrated_scale(DVec2::ONE, a, a + DVec2::X, d), None);
    }
    let mut s = AppState::new();
    let id = insert(&mut s);
    look_down(&mut s, 60.0);
    let mut v = View::new();
    s.start_calibrate(id).unwrap();
    let p = screen_of(&s, at(corners(&s, id), 0.6, 0.4));
    v.click(&mut s, p, PointerButton::Primary);
    v.click(&mut s, p, PointerButton::Primary);
    let c = s.calibrate.as_ref().unwrap();
    assert!(
        c.first.is_some() && c.second.is_none(),
        "derselbe Punkt zählt nicht"
    );
    assert_eq!(
        s.message.as_deref(),
        Some("Der zweite Punkt muss neben dem ersten liegen.")
    );
    // Daneben geklickt (außerhalb des Bildes): kein Punkt.
    v.click(&mut s, pos2(5.0, 5.0), PointerButton::Primary);
    assert!(s.calibrate.as_ref().unwrap().second.is_none());
    let q = screen_of(&s, at(corners(&s, id), 0.9, 0.4));
    v.click(&mut s, q, PointerButton::Primary);
    let before = s.document.edit_count();
    for bad in ["0", "-5 mm", "abc"] {
        s.calibrate_type(bad);
        assert!(s.apply_calibrate().is_err(), "{bad}");
        let c = s.calibrate.as_ref().expect("der Modus bleibt");
        assert!(c.error.is_some(), "Meldung im Feld bei {bad}");
    }
    assert_eq!(s.document.edit_count(), before, "nichts geändert");
}

#[test]
fn esc_leaves_nothing() {
    let mut ws = workspace();
    let id = insert(&mut ws.active_tab_mut().state);
    ws.observe_history(false);
    let steps = ws.active_tab().history().undo_len();
    let tab = ws.active_tab_mut();
    let s = &mut tab.state;
    look_down(s, 60.0);
    let before = canvas(s, id);
    let mut v = View::new();
    // Jede Stufe: Esc.
    for stage in 0..3 {
        s.start_calibrate(id).unwrap();
        let k = corners(s, id);
        if stage >= 1 {
            v.click(s, screen_of(s, at(k, 0.3, 0.5)), PointerButton::Primary);
        }
        if stage >= 2 {
            v.click(s, screen_of(s, at(k, 0.7, 0.5)), PointerButton::Primary);
            v.text(s, "100");
        }
        v.key(s, Key::Escape);
        assert!(s.calibrate.is_none(), "Esc in Stufe {stage}");
        assert!(calibrate::shapes(s, SCREEN).is_empty());
    }
    // Rechtsklick ebenso.
    s.start_calibrate(id).unwrap();
    let k = corners(s, id);
    v.click(s, screen_of(s, at(k, 0.3, 0.5)), PointerButton::Primary);
    v.click(s, screen_of(s, at(k, 0.4, 0.5)), PointerButton::Secondary);
    assert!(s.calibrate.is_none(), "Rechtsklick bricht ab");
    // Enter mit dem unveränderten gemessenen Abstand ändert nichts.
    pick_two(&mut v, s, id);
    v.key(s, Key::Enter);
    assert!(s.calibrate.is_none());
    assert_eq!(canvas(s, id), before);
    ws.observe_history(false);
    assert_eq!(ws.active_tab().history().undo_len(), steps, "kein Schritt");
}

#[test]
fn enter_is_one_document_undo_step() {
    let mut ws = workspace();
    let id = insert(&mut ws.active_tab_mut().state);
    ws.observe_history(false);
    let steps = ws.active_tab().history().undo_len();
    let s = &mut ws.active_tab_mut().state;
    look_down(s, 60.0);
    let before = canvas(s, id);
    let mut v = View::new();
    pick_two(&mut v, s, id);
    // Punkte und Linie werden gezeichnet.
    assert_eq!(calibrate::shapes(s, SCREEN).len(), 3);
    let texts = v.frame(s, vec![]);
    assert!(texts.contains("Abstand"), "Feld am Zeiger: {texts}");
    v.text(s, "16");
    v.key(s, Key::Enter);
    assert!(s.calibrate.is_none());
    ws.observe_history(false);
    assert_eq!(
        ws.active_tab().history().undo_len(),
        steps + 1,
        "ein Schritt"
    );
    assert_eq!(ws.active_tab().undo_label().as_deref(), Some("Kalibrieren"));
    let c = canvas(&ws.active_tab().state, id);
    assert!((c.scale.x - 0.5).abs() < 1e-3);
    let tab = ws.active_tab_mut();
    tab.undo().unwrap();
    assert_eq!(canvas(&tab.state, id), before, "ein Schritt zurück");
    // Der Name gilt nur für diesen Schritt.
    let mut f = canvas(&tab.state, id);
    f.opacity = 80;
    tab.state.update_canvas(id, f).unwrap();
    ws.observe_history(false);
    assert_eq!(
        ws.active_tab().undo_label().as_deref(),
        Some("Leinwand bearbeiten")
    );
    assert_eq!(
        planora_i18n::with_lang(&planora_i18n::english(), || {
            planora_i18n::tr!("undo-canvas-calibrate")
        }),
        "Calibrate"
    );
}

#[test]
fn formula_with_units_accepted() {
    let mut s = AppState::new();
    let id = insert(&mut s);
    s.start_calibrate(id).unwrap();
    let c = s.calibrate.as_mut().unwrap();
    c.first = Some(DVec2::new(0.0, 0.0));
    c.second = Some(DVec2::new(10.0, 0.0));
    // „1 in“ ergibt 25,4 mm.
    s.calibrate_type("1 in");
    s.apply_calibrate().unwrap();
    let f = canvas(&s, id);
    assert!((f.scale.x - 2.54).abs() < 1e-12 && f.scale.y == f.scale.x);
    // Formel mit Einheiten.
    s.start_calibrate(id).unwrap();
    let c = s.calibrate.as_mut().unwrap();
    c.first = Some(DVec2::new(0.0, 0.0));
    c.second = Some(DVec2::new(0.0, 25.4));
    s.calibrate_type("2 * 1 in - 25,4 mm");
    s.apply_calibrate().unwrap();
    assert!((canvas(&s, id).scale.x - 2.54).abs() < 1e-12);
}

#[test]
fn calibrate_disabled_while_sketch_open() {
    let mut ws = workspace();
    let tab = ws.active_tab_mut();
    let id = insert(&mut tab.state);
    let name = tab.state.document.feature(id).unwrap().name.clone();
    // Ohne Skizze: Kontextmenü im Baum startet den Modus.
    let menu = context_menu(&mut tab.state, &name, "Kalibrieren");
    assert!(menu.contains("Kalibrieren"), "{menu}");
    assert_eq!(tab.state.calibrate.as_ref().map(|c| c.canvas), Some(id));
    assert_eq!(
        calibrate::status_hint(tab.state.calibrate.as_ref().unwrap()),
        "Kalibrieren: ersten Punkt auf dem Bild anklicken (Esc bricht ab)"
    );
    // Eine Skizze öffnen beendet den Modus.
    let sk = tab
        .state
        .add_sketch(SketchPlane::Standard(StandardPlane::XZ))
        .unwrap();
    tab.state.open_sketch(sk).unwrap();
    assert!(
        tab.state.calibrate.is_none(),
        "die Skizze beendet den Modus"
    );
    // Gesperrt: Start, Menü und Taste.
    assert_eq!(
        tab.state.start_calibrate(id),
        Err("Erst die Skizze beenden".to_string())
    );
    assert!(tab.state.calibrate.is_none());
    let menu = context_menu(&mut tab.state, &name, "Kalibrieren");
    assert!(menu.contains("Kalibrieren"));
    assert!(tab.state.calibrate.is_none(), "gesperrter Eintrag");
    tab.state.select_canvas(id);
    tab.state.message = None;
    planora_app::commands::run_action(&mut ws, Action::CanvasCalibrate);
    let s = &ws.active_tab().state;
    assert!(s.calibrate.is_none());
    assert_eq!(s.message.as_deref(), Some("Erst die Skizze beenden"));
    // Ohne Skizze startet die Taste mit der gewählten Leinwand.
    ws.active_tab_mut().state.close_sketch();
    ws.active_tab_mut().state.select_canvas(id);
    planora_app::commands::run_action(&mut ws, Action::CanvasCalibrate);
    assert!(ws.active_tab().state.calibrate.is_some());
    assert_eq!(Action::CanvasCalibrate.id(), "einfuegen.kalibrieren");
    assert_eq!(Action::CanvasCalibrate.area(), Area::Model);
    // Der Platzhalter ist weg.
    let de = include_str!("../../planora-i18n/languages/de.ftl");
    let en = include_str!("../../planora-i18n/languages/en.ftl");
    assert!(
        !de.contains("tree-canvas-calibrate-later") && !en.contains("tree-canvas-calibrate-later")
    );
    let _ = browser::Folder::Canvases;
}

// ---- Baum ---------------------------------------------------------------------------------------

/// Mitte des ersten Textes, der genau `text` ist.
fn find_text(out: &egui::FullOutput, text: &str) -> Option<Pos2> {
    fn walk(shape: &egui::epaint::Shape, text: &str) -> Option<Pos2> {
        match shape {
            egui::epaint::Shape::Text(t) if t.galley.text() == text => {
                Some(t.visual_bounding_rect().center())
            }
            egui::epaint::Shape::Vec(v) => v.iter().find_map(|s| walk(s, text)),
            _ => None,
        }
    }
    out.shapes.iter().find_map(|c| walk(&c.shape, text))
}

fn tree_frame(
    c: &egui::Context,
    s: &mut AppState,
    events: Vec<Event>,
    text: &str,
) -> (String, Option<Pos2>) {
    let raw = egui::RawInput {
        screen_rect: Some(SCREEN),
        events,
        ..Default::default()
    };
    let out = c.run_ui(raw, |ui| browser::show(ui, s));
    let r = (
        planora_app::testing::collect_texts(&out),
        find_text(&out, text),
    );
    out.drop_without_applying_deltas();
    r
}

fn click(pos: Pos2, b: PointerButton) -> Vec<Event> {
    let button = |pressed| Event::PointerButton {
        pos,
        button: b,
        pressed,
        modifiers: Modifiers::NONE,
    };
    vec![Event::PointerMoved(pos), button(true), button(false)]
}

/// Rechtsklick auf die Zeile `row`, dann Klick auf den Menüeintrag `entry`; liefert die Texte
/// des offenen Menüs.
fn context_menu(s: &mut AppState, row: &str, entry: &str) -> String {
    let c = egui::Context::default();
    c.set_fonts(planora_app::fonts::definitions());
    let (texts, pos) = tree_frame(&c, s, vec![], row);
    let pos = pos.unwrap_or_else(|| panic!("Zeile {row}:\n{texts}"));
    tree_frame(&c, s, click(pos, PointerButton::Secondary), "");
    let (menu, item) = tree_frame(&c, s, vec![], entry);
    let item = item.unwrap_or_else(|| panic!("Menüeintrag {entry}:\n{menu}"));
    tree_frame(&c, s, click(item, PointerButton::Primary), "");
    tree_frame(&c, s, vec![], "");
    menu
}
