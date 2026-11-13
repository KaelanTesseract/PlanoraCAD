//! Skizzen 9, Task 13: Fenster „Leinwand“ (Einfügen und Bearbeiten), Ordner „Leinwände“ im Baum,
//! Zeichnen und Wählen in der Ansicht und die Sperre während einer offenen Skizze. Die Bilder
//! schreibt der Test selbst (PNG über das Crate `png`).

mod common;

use common::{Screen, find};
use egui::{Event, Modifiers, PointerButton, Pos2, Rect, pos2};
use glam::{Vec2, Vec3};
use planora_app::browser::{self, Folder};
use planora_app::icons::{self, Icon};
use planora_app::insert::DialogAction;
use planora_app::insert::canvas_dialog::CanvasDialog;
use planora_app::keymap::Action;
use planora_app::plane_dialog::PlaneClick;
use planora_app::state::*;
use planora_app::workspace::Workspace;
use planora_app::{app, fonts, theme};
use planora_feature::{CanvasFeature, FeatureId, FeatureKind, SketchPlane};
use planora_math::{DVec2, DVec3};
use planora_project::config::Config;
use planora_project::library::Library;
use std::fs;
use std::path::{Path, PathBuf};

const W: f32 = 1920.0;
const H: f32 = 1080.0;
const VIEW: Vec2 = Vec2::new(W, H);

/// PNG `w` × `h`, deckend rot; mit `clear_left` ist die linke Hälfte ganz durchsichtig.
fn png(w: u32, h: u32, clear_left: bool) -> Vec<u8> {
    let mut data = Vec::with_capacity((w * h * 4) as usize);
    for _y in 0..h {
        for x in 0..w {
            let a = if clear_left && x < w / 2 { 0 } else { 255 };
            data.extend_from_slice(&[200, 30, 30, a]);
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

fn temp_dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("planora-canvas-{}-{name}", std::process::id()));
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
    let root = common::temp_root("planora-canvas-ui");
    Workspace::new(Some(Library::open(&root).unwrap()), Config::default())
}

/// Fenster mit gelesenem Bild `path` und der Ebene XY.
fn loaded(dir: &Path, path: PathBuf, state: &AppState) -> CanvasDialog {
    let mut d = CanvasDialog::new(dir.to_path_buf(), state);
    d.load(path);
    d.tick(true);
    assert!(d.loaded.is_some(), "gelesen: {:?}", d.error_text());
    d.target
        .take_click(state, PlaneClick::Standard(StandardPlane::XY));
    d
}

/// Fügt eine Leinwand `w` × `h` px auf XY ein.
fn insert(state: &mut AppState, dir: &Path, w: u32, h: u32, clear_left: bool) -> FeatureId {
    let path = write(
        dir,
        &format!("b{w}x{h}{clear_left}.png"),
        &png(w, h, clear_left),
    );
    let mut d = loaded(dir, path, state);
    d.insert(state).unwrap()
}

/// Ein Bild der Ansicht (startet das Entschlüsseln im Hintergrund) und warten, bis alle Bilder
/// entschlüsselt sind.
fn ready(s: &mut AppState) {
    s.canvas_frame_data();
    s.canvas_textures.wait_all();
}

fn canvas(state: &AppState, id: FeatureId) -> CanvasFeature {
    match &state.document.feature(id).unwrap().kind {
        FeatureKind::Canvas(c) => c.clone(),
        k => panic!("keine Leinwand: {k:?}"),
    }
}

fn corners(state: &AppState, id: FeatureId) -> [DVec3; 4] {
    let c = canvas(state, id);
    let img = state.document.image(c.image).unwrap();
    c.corners(&state.document.canvas_frame(id).unwrap(), img)
}

#[test]
fn canvas_defaults_50_through_selectable_1px_1mm() {
    let dir = temp_dir("vorgaben");
    let s = AppState::new();
    let d = CanvasDialog::new(dir.clone(), &s);
    assert_eq!(d.opacity, 50);
    assert!(d.display_through && d.selectable);
    assert_eq!(d.scale, DVec2::ONE);
    assert_eq!(
        d.scale_xy,
        Some(1.0),
        "Skalierung XY, Seitenverhältnis bleibt"
    );
    assert!(!d.flip_h && !d.flip_v);
    assert_eq!((d.offset, d.angle_deg), (DVec2::ZERO, 0.0));

    let mut s = AppState::new();
    let id = insert(&mut s, &dir, 40, 20, false);
    let c = canvas(&s, id);
    assert_eq!(c.opacity, 50);
    assert!(c.display_through && c.selectable);
    assert_eq!(c.scale, DVec2::ONE);
    // 1 px = 1 mm: 40 × 20 mm, Mitte im Ursprung.
    let k = corners(&s, id);
    assert!(((k[1] - k[0]).length() - 40.0).abs() < 1e-9);
    assert!(((k[3] - k[0]).length() - 20.0).abs() < 1e-9);
    assert!(((k[0] + k[2]) * 0.5).length() < 1e-9);
    // Skalierung XY ändert beide Achsen; getrennt nur eine.
    let mut d = CanvasDialog::edit(dir.clone(), &s, id).unwrap();
    d.set_from_grips(DVec2::ZERO, 0.0, 2.0);
    assert_eq!(d.scale, DVec2::splat(2.0));
    d.set_scale_locked(false);
    assert_eq!(d.scale_xy, None);
    assert_eq!(d.field_labels().len(), 5, "X und Y getrennt");
    // „Renderfähig“ gibt es nicht.
    assert!(!d.field_labels().iter().any(|l| l.contains("Render")));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn canvas_ok_one_document_undo_step_at_marker() {
    let dir = temp_dir("marke");
    let mut ws = workspace();
    let tab = ws.active_tab_mut();
    let a = tab.state.create_box(DVec3::splat(10.0)).unwrap();
    let b = tab.state.create_box(DVec3::splat(5.0)).unwrap();
    ws.observe_history(false);
    let tab = ws.active_tab_mut();
    browser::roll_back_to(&mut tab.state, a).unwrap();
    ws.observe_history(false);
    let tab = ws.active_tab_mut();
    let id = insert(&mut tab.state, &dir, 8, 4, false);
    ws.observe_history(false);
    let s = &ws.active_tab().state;
    let order: Vec<FeatureId> = s.document.features().iter().map(|f| f.id).collect();
    assert_eq!(order, [a, id, b], "an der Verlaufsmarke");
    assert_eq!(s.snapshot().images.len(), 1);
    assert_eq!(
        s.selected_feature,
        Some(id),
        "die neue Leinwand ist gewählt"
    );
    assert_eq!(
        ws.active_tab().undo_label().as_deref(),
        Some("Leinwand einfügen")
    );
    let tab = ws.active_tab_mut();
    tab.undo().unwrap();
    assert!(!tab.state.has_canvases(), "ein Schritt");
    assert_eq!(tab.state.document.features().len(), 2);
    assert!(tab.state.snapshot().images.is_empty());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn edit_canvas_changes_feature_one_step() {
    let dir = temp_dir("bearbeiten");
    let mut ws = workspace();
    let tab = ws.active_tab_mut();
    let id = insert(&mut tab.state, &dir, 8, 4, false);
    ws.observe_history(false);
    let tab = ws.active_tab_mut();
    let mut d = CanvasDialog::edit(dir.clone(), &tab.state, id).expect("Fenster");
    assert_eq!(d.edit, Some(id));
    assert_eq!(d.opacity, 50);
    assert!(d.target.target().is_some(), "Träger eingetragen");
    // Felder wie im SVG-Fenster: Y nach unten, Winkel im Uhrzeigersinn.
    d.opacity = 20;
    d.selectable = false;
    d.offset = DVec2::new(5.0, 3.0);
    d.angle_deg = 90.0;
    d.flip_h = true;
    d.insert(&mut tab.state).unwrap();
    ws.observe_history(false);
    let s = &ws.active_tab().state;
    assert_eq!(s.canvas_ids(), [id], "keine neue Leinwand");
    let c = canvas(s, id);
    assert_eq!((c.opacity, c.selectable, c.flip_h), (20, false, true));
    assert_eq!(c.offset, DVec2::new(5.0, -3.0));
    assert!((c.angle + std::f64::consts::FRAC_PI_2).abs() < 1e-12);
    assert_eq!(s.snapshot().images.len(), 1, "dasselbe Bild");
    // Ein zweites Öffnen zeigt die neuen Werte.
    let again = CanvasDialog::edit(dir.clone(), s, id).unwrap();
    assert_eq!(again.offset, DVec2::new(5.0, 3.0));
    assert!((again.angle_deg - 90.0).abs() < 1e-9);
    assert_eq!(
        ws.active_tab().undo_label().as_deref(),
        Some("Leinwand bearbeiten")
    );
    let tab = ws.active_tab_mut();
    tab.undo().unwrap();
    let c = canvas(&tab.state, id);
    assert_eq!((c.opacity, c.selectable), (50, true), "ein Schritt zurück");
    assert!(tab.state.has_canvases());
    let _ = fs::remove_dir_all(&dir);
}

fn insert_menu_frame<'a>(ws: &'a mut Workspace) -> impl FnMut(&egui::Context) + 'a {
    move |ctx| {
        egui::Area::new(egui::Id::new("einfuegen"))
            .fixed_pos(Pos2::new(20.0, 20.0))
            .show(ctx, |ui| {
                ui.set_width(280.0);
                theme::menu_style(ui.style_mut());
                app::insert_menu(ui, ws);
            });
    }
}

#[test]
fn canvas_menu_disabled_while_sketch_open_with_tooltip() {
    let dir = temp_dir("sperre");
    let mut ws = workspace();
    let tab = ws.active_tab_mut();
    let canvas_id = insert(&mut tab.state, &dir, 8, 4, false);
    let sk = tab
        .state
        .add_sketch(SketchPlane::Standard(StandardPlane::XZ))
        .unwrap();
    tab.state.open_sketch(sk).unwrap();
    assert_eq!(
        tab.state.canvas_lock().as_deref(),
        Some("Erst die Skizze beenden")
    );
    let en = planora_i18n::with_lang(&planora_i18n::english(), || tab.state.canvas_lock());
    assert_eq!(en.as_deref(), Some("Finish the sketch first"));

    // Der Menüeintrag ist gesperrt: ein Klick öffnet nichts.
    let mut screen = Screen::new(W, H);
    screen
        .ctx
        .all_styles_mut(|st| st.interaction.tooltip_delay = 0.0);
    let seen = screen.settle(3, &mut insert_menu_frame(&mut ws));
    let entry = find(&seen, "Leinwand …").expect("Leinwand").rect;
    // Über dem gesperrten Eintrag steht der Grund im Tooltip.
    let mut seen = Vec::new();
    for _ in 0..5 {
        seen = screen.frame(
            vec![Event::PointerMoved(entry.center())],
            &mut insert_menu_frame(&mut ws),
        );
    }
    assert!(
        find(&seen, "Erst die Skizze beenden").is_some(),
        "Tooltip fehlt: {:?}",
        seen.iter().map(|t| t.text.as_str()).collect::<Vec<_>>()
    );
    screen.click(entry.center(), &mut insert_menu_frame(&mut ws));
    assert!(ws.active_tab().canvas_dialog.is_none());
    // Die Taste ebenso, mit dem Grund in der Statuszeile; die Skizze bleibt offen.
    planora_app::commands::run_action(&mut ws, Action::InsertCanvas);
    let tab = ws.active_tab_mut();
    assert!(tab.canvas_dialog.is_none());
    tab.state.message = None;
    // „Leinwand bearbeiten“ (Doppelklick im Baum) ebenso.
    browser::edit(&mut tab.state, canvas_id);
    assert_eq!(tab.state.edit_request, None);
    assert_eq!(
        tab.state.message.as_deref(),
        Some("Erst die Skizze beenden")
    );
    assert_eq!(
        tab.state.open_sketch_id(),
        Some(sk),
        "die Skizze bleibt offen"
    );
    tab.open_edit(canvas_id);
    assert!(tab.canvas_dialog.is_none());
    // Ein offenes Fenster schließt, sobald eine Skizze offen ist.
    tab.state.close_sketch();
    tab.open_edit(canvas_id);
    assert!(tab.canvas_dialog.is_some(), "ohne Skizze geht Bearbeiten");
    tab.state.open_sketch(sk).unwrap();
    let mut d = tab.canvas_dialog.take().unwrap();
    let mut last = DialogAction::Open;
    let state = &mut tab.state;
    screen.frame(vec![], &mut |ctx| {
        last = d.show(
            ctx,
            state,
            &mut None,
            &mut planora_app::places::Places::default(),
        );
    });
    assert_eq!(last, DialogAction::Closed);
    assert!(tab.state.canvas_preview.is_none() && tab.state.insert_grips.is_none());
    let _ = fs::remove_dir_all(&dir);
}

// ---- Baum ---------------------------------------------------------------------------------------

struct Tree {
    ctx: egui::Context,
    time: f64,
}

impl Tree {
    fn new() -> Tree {
        let ctx = egui::Context::default();
        ctx.set_fonts(fonts::definitions());
        theme::apply(&ctx, true);
        Tree { ctx, time: 0.0 }
    }

    fn frame(&mut self, s: &mut AppState, events: Vec<Event>) {
        self.time += 0.05;
        let raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_max(pos2(0.0, 0.0), pos2(W, H))),
            time: Some(self.time),
            events,
            ..Default::default()
        };
        let out = self.ctx.run_ui(raw, |ui| browser::show(ui, s));
        out.drop_without_applying_deltas();
    }

    fn settle(&mut self, s: &mut AppState) {
        self.frame(s, vec![]);
        self.frame(s, vec![]);
    }

    fn click(&mut self, s: &mut AppState, p: Pos2) {
        let button = |pressed| Event::PointerButton {
            pos: p,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        for events in [
            vec![Event::PointerMoved(p)],
            vec![button(true)],
            vec![button(false)],
            vec![],
        ] {
            self.frame(s, events);
        }
    }

    fn folders(&self) -> Vec<Folder> {
        browser::folder_rects(&self.ctx)
            .into_iter()
            .map(|(f, _)| f)
            .collect()
    }

    fn eye(&mut self, s: &mut AppState, name: &str) {
        self.settle(s);
        let eye = browser::eye_rects(&self.ctx)
            .into_iter()
            .find(|(n, _)| n == name)
            .map(|(_, r)| r)
            .unwrap_or_else(|| panic!("Auge bei „{name}“ fehlt"));
        self.click(s, eye.center());
    }
}

#[test]
fn canvases_folder_only_when_present() {
    let dir = temp_dir("ordner");
    let mut s = AppState::new();
    let mut t = Tree::new();
    t.settle(&mut s);
    assert!(!t.folders().contains(&Folder::Canvases));
    let id = insert(&mut s, &dir, 8, 4, false);
    t.settle(&mut s);
    let folders = t.folders();
    let pos = |f| folders.iter().position(|x| *x == f);
    assert!(pos(Folder::Canvases).is_some(), "Ordner da");
    assert!(pos(Folder::Sketches) < pos(Folder::Canvases));
    assert_eq!(Folder::Canvases.label(), "Leinwände");
    let row = browser::row_rects(&t.ctx)
        .into_iter()
        .find(|(n, _)| n == "Leinwand 1")
        .map(|(_, r)| r)
        .expect("Zeile „Leinwand 1“");
    // Ein Klick wählt die Leinwand.
    s.selected_feature = None;
    t.click(&mut s, row.center());
    assert_eq!(s.selected_feature, Some(id));
    // Die Leinwand ist kein Körper: keine Zeile im Ordner „Körper“.
    assert!(s.document.body_tips().is_empty());
    let rows: Vec<String> = browser::row_rects(&t.ctx)
        .into_iter()
        .map(|(n, _)| n)
        .collect();
    assert_eq!(rows, ["Leinwand 1"], "nur im Ordner „Leinwände“");
    // Doppelklick ergibt „Bearbeiten“.
    let p = row.center();
    let button = |pressed| Event::PointerButton {
        pos: p,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    };
    t.frame(&mut s, vec![Event::PointerMoved(p)]);
    t.frame(&mut s, vec![button(true), button(false)]);
    t.frame(&mut s, vec![button(true), button(false)]);
    assert_eq!(s.edit_request, Some(id), "Doppelklick bearbeitet");
    s.edit_request = None;
    // Mit der letzten Leinwand verschwindet der Ordner.
    let plan = browser::delete_plan(&s, id);
    browser::apply_delete(&mut s, &plan).unwrap();
    t.settle(&mut s);
    assert!(!t.folders().contains(&Folder::Canvases));
    assert_eq!(
        planora_i18n::with_lang(&planora_i18n::english(), || Folder::Canvases.label()),
        "Canvases"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn canvas_eye_hides_and_is_saved() {
    let dir = temp_dir("auge");
    let mut s = AppState::new();
    let id = insert(&mut s, &dir, 8, 4, false);
    ready(&mut s);
    assert_eq!(s.canvas_frame_data().0.len(), 1, "gezeichnet");
    let plain = planora_project::encode_version(&s.snapshot()).unwrap();
    let mut t = Tree::new();
    t.eye(&mut s, "Leinwand 1");
    assert!(s.own_feature_hidden(id) && s.is_feature_hidden(id));
    let (draws, textures) = s.canvas_frame_data();
    assert!(draws.is_empty() && textures.is_empty(), "nicht gezeichnet");
    // In der Datei gespeichert.
    let text = planora_project::encode_version(&s.snapshot()).unwrap();
    let back = planora_project::decode_version(&text, "Test").unwrap();
    let l = AppState::from_snapshot(back).unwrap();
    assert!(l.own_feature_hidden(id));
    // Die Taste „Ein-/Ausblenden“ wirkt auf die gewählte Leinwand.
    s.select_canvas(id);
    assert!(s.toggle_selected_visibility());
    assert!(!s.own_feature_hidden(id));
    assert!(s.toggle_selected_visibility());
    // Wieder an, dann der Ordner aus: auch gespeichert.
    t.eye(&mut s, "Leinwand 1");
    assert!(!s.is_feature_hidden(id));
    assert_eq!(
        planora_project::encode_version(&s.snapshot()).unwrap(),
        plain
    );
    t.eye(&mut s, &Folder::Canvases.label());
    assert!(s.is_folder_hidden(Folder::Canvases) && s.is_feature_hidden(id));
    assert!(!s.own_feature_hidden(id), "das Auge der Zeile bleibt");
    let text = planora_project::encode_version(&s.snapshot()).unwrap();
    let l =
        AppState::from_snapshot(planora_project::decode_version(&text, "Test").unwrap()).unwrap();
    assert!(l.is_folder_hidden(Folder::Canvases));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn delete_canvas_drops_image_from_snapshot() {
    let dir = temp_dir("loeschen");
    let mut ws = workspace();
    let tab = ws.active_tab_mut();
    let id = insert(&mut tab.state, &dir, 8, 4, false);
    ws.observe_history(false);
    let tab = ws.active_tab_mut();
    assert_eq!(tab.state.snapshot().images.len(), 1);
    let plan = browser::delete_plan(&tab.state, id);
    browser::apply_delete(&mut tab.state, &plan).unwrap();
    ws.observe_history(false);
    let tab = ws.active_tab_mut();
    assert!(!tab.state.has_canvases());
    assert!(tab.state.snapshot().images.is_empty(), "Bild mit weg");
    // Auch der Zwischenspeicher der Ansicht gibt das Bild frei.
    let (draws, _) = tab.state.canvas_frame_data();
    assert!(draws.is_empty() && tab.state.canvas_textures.is_empty());
    tab.undo().unwrap();
    assert!(tab.state.has_canvases());
    assert_eq!(tab.state.snapshot().images.len(), 1, "zurück mit Bild");
    let _ = fs::remove_dir_all(&dir);
}

/// Kamera auf das Rechteck ±`r` mm um den Ursprung in XY, von oben.
fn look_down(s: &mut AppState, r: f32) {
    s.camera.set_viewport(W, H);
    s.camera.set_view(planora_render::ViewFace::Top);
    s.camera
        .frame_bounds(Vec3::new(-r, -r, -0.1), Vec3::new(r, r, 0.1));
}

fn screen_of(s: &AppState, p: DVec3) -> Vec2 {
    s.camera.project(p.as_vec3(), VIEW).unwrap()
}

/// Punkt des Bildes bei `(u, v)` (0..=1 von oben links) aus den Ecken.
fn at(k: [DVec3; 4], u: f64, v: f64) -> DVec3 {
    k[0] + (k[1] - k[0]) * u + (k[3] - k[0]) * v
}

#[test]
fn not_selectable_canvas_is_clicked_through() {
    let dir = temp_dir("durchklicken");
    let mut s = AppState::new();
    // Linke Hälfte durchsichtig.
    let id = insert(&mut s, &dir, 64, 32, true);
    look_down(&mut s, 40.0);
    // Noch nicht entschlüsselt: nicht gezeichnet, also auch nicht getroffen.
    let center = screen_of(&s, at(corners(&s, id), 0.75, 0.5));
    assert_eq!(s.canvas_at(center, VIEW), None, "ohne Bild kein Treffer");
    ready(&mut s);
    let k = corners(&s, id);
    let right = screen_of(&s, at(k, 0.75, 0.5));
    let left = screen_of(&s, at(k, 0.25, 0.5));
    s.selected_feature = None;
    assert_eq!(
        s.canvas_click_at(right, VIEW),
        Some(id),
        "auswählbar: Treffer"
    );
    assert_eq!(s.canvas_click_at(left, VIEW), None, "Alpha 0 zählt nicht");
    // Überfahren hebt die Leinwand hervor (Rahmen), wie der Klick.
    let rect = Rect::from_min_max(pos2(0.0, 0.0), pos2(W, H));
    assert!(planora_app::canvas::canvas_outlines(&s, rect).is_empty());
    assert!(s.hover_canvas(right, VIEW));
    assert_eq!(s.canvas_hover, Some(id));
    assert_eq!(planora_app::canvas::canvas_outlines(&s, rect).len(), 1);
    assert!(
        !s.hover_canvas(left, VIEW),
        "durchsichtig: keine Hervorhebung"
    );
    assert!(s.click_canvas(right, VIEW));
    assert_eq!(s.selected_feature, Some(id), "gewählt wie im Baum");
    // Nicht auswählbar: durchklickt.
    let mut f = canvas(&s, id);
    f.selectable = false;
    s.update_canvas(id, f).unwrap();
    s.selected_feature = None;
    assert_eq!(s.canvas_click_at(right, VIEW), None);
    assert!(
        !s.hover_canvas(right, VIEW),
        "nicht auswählbar: keine Hervorhebung"
    );
    assert!(!s.click_canvas(right, VIEW));
    assert_eq!(s.selected_feature, None);
    // Ein Körper vor der Leinwand gewinnt.
    let mut f = canvas(&s, id);
    f.selectable = true;
    s.update_canvas(id, f).unwrap();
    s.create_box(DVec3::splat(100.0)).unwrap();
    look_down(&mut s, 60.0);
    ready(&mut s);
    let k = corners(&s, id);
    let right = screen_of(&s, at(k, 0.9, 0.25));
    assert!(
        s.canvas_at(right, VIEW).is_some(),
        "Leinwand unter dem Zeiger"
    );
    assert_eq!(
        s.canvas_click_at(right, VIEW),
        None,
        "der Körper liegt davor"
    );
    assert!(
        !s.hover_canvas(right, VIEW),
        "der Körper wird hervorgehoben"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn texture_decoded_once_in_background_and_sent_every_frame() {
    let dir = temp_dir("textur");
    let mut s = AppState::new();
    let a = insert(&mut s, &dir, 8, 4, false);
    // Dasselbe Bild ein zweites Mal: geteilt.
    let b = insert(&mut s, &dir, 8, 4, false);
    assert_ne!(a, b);
    assert_eq!(s.snapshot().images.len(), 1);
    // Wie nach dem Laden einer Datei: Das Bild ist noch nicht im Zwischenspeicher.
    let before = s.canvas_textures.decodes;
    let (draws, textures) = s.canvas_frame_data();
    assert!(draws.is_empty() && textures.is_empty(), "nichts gezeichnet");
    assert_eq!(s.canvas_textures.decodes, before + 1, "ein Auftrag je Bild");
    assert_eq!(s.canvas_textures.running(), 1);
    assert!(s.canvas_textures.pending(), "die Ansicht zeichnet bald neu");
    s.canvas_textures.wait_all();
    assert!(!s.canvas_textures.pending());
    for _ in 0..3 {
        let (draws, textures) = s.canvas_frame_data();
        assert_eq!(draws.len(), 2);
        assert_eq!(textures.len(), 1, "jedes gezeichnete Bild einmal");
        assert_eq!(textures[0].key, draws[0].key);
        assert!((draws[0].opacity - 0.5).abs() < 1e-6);
        assert!(draws[0].display_through);
    }
    assert_eq!(
        s.canvas_textures.decodes,
        before + 1,
        "nicht erneut entschlüsselt"
    );
    // Ein kaputtes Bild wird gemerkt und nicht in jedem Bild neu versucht.
    let id = s.canvas_ids()[0];
    let c = canvas(&s, id);
    let broken = planora_feature::CanvasImage {
        bytes: std::sync::Arc::from(&b"kein Bild"[..]),
        format: planora_exchange::ImageFormat::Png,
        width: 8,
        height: 4,
        hash: planora_feature::image_hash(b"kein Bild"),
    };
    s.insert_canvas(broken, c).unwrap();
    let n = s.canvas_textures.decodes;
    ready(&mut s);
    for _ in 0..3 {
        assert_eq!(s.canvas_frame_data().0.len(), 2, "das kaputte fehlt");
    }
    assert_eq!(s.canvas_textures.decodes, n + 1);
    let _ = fs::remove_dir_all(&dir);
}

/// Zeigt das Fenster `d` in einem Bild auf `screen`.
fn show(
    screen: &mut Screen,
    d: &mut CanvasDialog,
    s: &mut AppState,
    last: &mut DialogAction,
    events: Vec<Event>,
) -> Vec<common::Seen> {
    screen.frame(events, &mut |ctx| {
        if *last == DialogAction::Open {
            *last = d.show(
                ctx,
                s,
                &mut None,
                &mut planora_app::places::Places::default(),
            );
        }
    })
}

#[test]
fn esc_cancel_and_close_leave_nothing() {
    let dir = temp_dir("abbruch");
    let path = write(&dir, "b.png", &png(8, 4, false));
    let mut s = AppState::new();
    s.create_box(DVec3::splat(10.0)).unwrap();
    let before = s.snapshot();
    let mut screen = Screen::new(W, H);
    let left = |p: Pos2, pressed| Event::PointerButton {
        pos: p,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    };
    for way in ["Esc", "Abbrechen", "✕"] {
        let mut d = loaded(&dir, path.clone(), &s);
        let mut last = DialogAction::Open;
        let mut seen = Vec::new();
        for _ in 0..4 {
            seen = show(&mut screen, &mut d, &mut s, &mut last, vec![]);
        }
        assert!(
            s.canvas_preview.is_some() && s.insert_grips.is_some(),
            "{way}: Vorschau"
        );
        for label in [
            "Deckkraft",
            "Durch Objekte anzeigen",
            "Auswählbar",
            "X-Abstand",
            "Y-Abstand",
            "Z-Winkel",
            "Skalierung XY",
            "Horizontal spiegeln",
            "Vertikal spiegeln",
        ] {
            assert!(find(&seen, label).is_some(), "„{label}“ fehlt");
        }
        assert!(find(&seen, "Renderfähig").is_none());
        let events = match way {
            "Esc" => vec![Event::Key {
                key: egui::Key::Escape,
                physical_key: Some(egui::Key::Escape),
                pressed: true,
                repeat: false,
                modifiers: Modifiers::NONE,
            }],
            "Abbrechen" => {
                let p = find(&seen, "Abbrechen").expect("Abbrechen").rect.center();
                show(
                    &mut screen,
                    &mut d,
                    &mut s,
                    &mut last,
                    vec![Event::PointerMoved(p)],
                );
                vec![left(p, true), left(p, false)]
            }
            _ => {
                let glyph = icons::glyph(Icon::Close).to_string();
                let frame = d.last_rect.expect("Fenster");
                let p = seen
                    .iter()
                    .filter(|t| t.text == glyph && frame.contains(t.rect.center()))
                    .min_by(|a, b| a.rect.top().total_cmp(&b.rect.top()))
                    .expect("✕ im Fenster")
                    .rect
                    .center();
                show(
                    &mut screen,
                    &mut d,
                    &mut s,
                    &mut last,
                    vec![Event::PointerMoved(p)],
                );
                vec![left(p, true), left(p, false)]
            }
        };
        show(&mut screen, &mut d, &mut s, &mut last, events);
        show(&mut screen, &mut d, &mut s, &mut last, vec![]);
        assert_eq!(last, DialogAction::Closed, "{way} schließt");
        assert!(s.snapshot().same_content(&before), "{way}: nichts geändert");
        assert!(!s.has_canvases());
        assert!(
            s.canvas_preview.is_none() && s.insert_grips.is_none() && s.plane_pick.is_none(),
            "{way}: aufgeräumt"
        );
    }
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn wrong_extension_message() {
    let dir = temp_dir("endung");
    let path = write(&dir, "bild.jpg", &png(4, 2, false));
    let s = AppState::new();
    let mut d = CanvasDialog::new(dir.clone(), &s);
    d.load(path);
    d.tick(true);
    assert!(d.loaded.is_none());
    assert_eq!(
        d.error_text().as_deref(),
        Some("Die Dateiendung passt nicht zum Inhalt des Bildes.")
    );
    // Kein Bild: eigene Meldung, das Fenster bleibt offen.
    let path = write(&dir, "kaputt.png", b"kein Bild");
    d.load(path);
    d.tick(true);
    assert!(d.loaded.is_none());
    assert_eq!(
        d.error_text().as_deref(),
        Some("Dieses Bildformat wird nicht unterstützt; möglich sind PNG, JPG und TIFF.")
    );
    let _ = fs::remove_dir_all(&dir);
}
