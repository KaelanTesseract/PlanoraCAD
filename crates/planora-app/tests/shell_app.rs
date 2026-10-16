//! Die ganze App mit schwebenden Feldern (Graphit, Etappe B, Aufgabe 6): `PlanoraApp::ui`
//! zeichnet die Felder, darunter den 3D-Bereich über das ganze Fenster (Bildmitte in der freien
//! Fläche) bzw. die Zeichnung in der freien Fläche, darüber die Dialoge. Ohne Fenster: Rohereignisse
//! je Bild gegen einen egui-Kontext, `eframe::App::ui` mit einem leeren Testrahmen.

use eframe::App as _;
use egui::{Event, Key, Modifiers, PointerButton, Pos2, Rect, Vec2, pos2, vec2};
use glam::Vec3;
use planora_app::app::PlanoraApp;
use planora_app::drawing_state::WorkArea;
use planora_app::shell::{self, SHELL_INSET, ShellRects, ShellState};
use planora_app::sketch_state::Tool;
use planora_app::state::{AppState, StandardPlane};
use planora_app::workspace::Workspace;
use planora_app::{drawing_view, fonts, shell_panels, theme, versions_panel, viewport};
use planora_drawing::{Paper, Projection, Sheet};
use planora_feature::SketchPlane;
use planora_math::DVec3;
use planora_project::config::Config;
use planora_project::docstore::LockMode;
use planora_project::fsutil::new_id;
use planora_project::library::Library;

const BIG: Vec2 = vec2(1400.0, 900.0);
const MID: Vec2 = vec2(1100.0, 700.0);
const SMALL: Vec2 = vec2(800.0, 600.0);

fn context() -> egui::Context {
    let c = egui::Context::default();
    c.set_fonts(fonts::definitions());
    theme::apply(&c, true);
    c
}

thread_local! {
    static TIME: std::cell::Cell<f64> = const { std::cell::Cell::new(0.0) };
}

fn input(size: Vec2, events: Vec<Event>) -> egui::RawInput {
    // Kurze Bildabstände: Klicks gelten nicht als langes Halten.
    let time = TIME.with(|t| {
        t.set(t.get() + 1.0 / 60.0);
        t.get()
    });
    egui::RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
        time: Some(time),
        events,
        ..Default::default()
    }
}

fn library_ws() -> Workspace {
    let root = std::env::temp_dir().join(format!("planora-shell-app-{}", new_id()));
    Workspace::new(Some(Library::open(&root).unwrap()), Config::default())
}

/// App mit einem Quader im aktiven Tab.
fn app_with_box() -> PlanoraApp {
    let mut app = PlanoraApp::for_tests(library_ws());
    app.ws_mut()
        .active_tab_mut()
        .state
        .create_box(DVec3::new(100.0, 60.0, 40.0))
        .unwrap();
    app
}

fn read_only_ws() -> Workspace {
    let mut w = library_ws();
    let p = w.library.as_ref().unwrap().create_project("P").unwrap();
    w.active_tab_mut()
        .state
        .create_box(DVec3::splat(80.0))
        .unwrap();
    w.save_as(&p, "Teil", "Anfang").unwrap();
    w.active_tab_mut()
        .state
        .create_box(DVec3::splat(3.0))
        .unwrap();
    w.save_active("Stand 2", None).unwrap();
    versions_panel::open_version(&mut w, 1);
    assert!(w.active_tab().read_only);
    w
}

/// Ein gezeichneter Text mit Lage.
#[derive(Debug, Clone)]
struct Seen {
    text: String,
    rect: Rect,
}

/// Was ein Bild gezeichnet hat: Texte und gefüllte Rechtecke.
#[derive(Default)]
struct Shot {
    texts: Vec<Seen>,
    fills: Vec<(Rect, egui::Color32)>,
}

impl Shot {
    fn of(out: &egui::FullOutput) -> Shot {
        fn walk(shape: &egui::epaint::Shape, s: &mut Shot) {
            match shape {
                egui::epaint::Shape::Vec(v) => v.iter().for_each(|x| walk(x, s)),
                egui::epaint::Shape::Text(t) => s.texts.push(Seen {
                    text: t.galley.text().to_string(),
                    rect: Rect::from_min_size(t.pos, t.galley.size()),
                }),
                egui::epaint::Shape::Rect(r) => s.fills.push((r.rect, r.fill)),
                _ => {}
            }
        }
        let mut s = Shot::default();
        for c in &out.shapes {
            walk(&c.shape, &mut s);
        }
        s
    }

    /// Alle Stellen mit genau diesem Text.
    fn all(&self, text: &str) -> Vec<Rect> {
        self.texts
            .iter()
            .filter(|t| t.text == text)
            .map(|t| t.rect)
            .collect()
    }

    /// Gibt es den Text genau so innerhalb von `r`?
    fn has_in(&self, text: &str, r: Rect) -> bool {
        self.all(text).iter().any(|t| r.contains_rect(*t))
    }

    /// Erste Stelle mit genau diesem Text außerhalb von `not_in`.
    fn find_outside(&self, text: &str, not_in: &[Rect]) -> Rect {
        *self
            .all(text)
            .iter()
            .find(|t| !not_in.iter().any(|r| r.intersects(**t)))
            .unwrap_or_else(|| panic!("„{text}“ nicht gezeichnet"))
    }
}

/// Ein Bild der ganzen App.
fn frame(c: &egui::Context, app: &mut PlanoraApp, size: Vec2, events: Vec<Event>) -> Shot {
    let mut f = eframe::Frame::_new_kittest();
    let out = c.run_ui(input(size, events), |ui| app.ui(ui, &mut f));
    let shot = Shot::of(&out);
    out.drop_without_applying_deltas();
    shot
}

/// Drei Bilder ohne Eingaben; liefert das letzte.
fn settle(c: &egui::Context, app: &mut PlanoraApp, size: Vec2) -> Shot {
    frame(c, app, size, vec![]);
    frame(c, app, size, vec![]);
    frame(c, app, size, vec![])
}

/// Tatsächlich belegte Rechtecke der Felder im letzten Bild.
fn fields(c: &egui::Context) -> Vec<(&'static str, Rect)> {
    shell_panels::field_rects(c)
}

fn field(c: &egui::Context, name: &str) -> Rect {
    fields(c)
        .into_iter()
        .find(|(n, _)| *n == name)
        .unwrap_or_else(|| panic!("Feld {name} fehlt"))
        .1
}

/// Geometrie, wie sie aus Fenster und Schreibschutz folgt (die freie Fläche hängt nicht von den
/// gemessenen Breiten ab; die Breiten oben, unten und des Status sind hier 0).
fn rects(app: &mut PlanoraApp, size: Vec2) -> ShellRects {
    let read_only = app.ws_mut().active_tab().read_only;
    shell::layout(
        Rect::from_min_size(Pos2::ZERO, size),
        &ShellState::default(),
        0.0,
        0.0,
        0.0,
        read_only,
    )
}

fn state(app: &mut PlanoraApp) -> &mut AppState {
    &mut app.ws_mut().active_tab_mut().state
}

fn moved(p: Pos2) -> Event {
    Event::PointerMoved(p)
}

fn button(p: Pos2, pressed: bool) -> Event {
    Event::PointerButton {
        pos: p,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    }
}

fn wheel(dy: f32) -> Event {
    Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        delta: vec2(0.0, dy),
        phase: egui::TouchPhase::Move,
        modifiers: Modifiers::NONE,
    }
}

fn key(k: Key, modifiers: Modifiers) -> Event {
    Event::Key {
        key: k,
        physical_key: Some(k),
        pressed: true,
        repeat: false,
        modifiers,
    }
}

/// Klick an `p` (drücken, loslassen, ein Bild danach); liefert das letzte Bild.
fn click(c: &egui::Context, app: &mut PlanoraApp, size: Vec2, p: Pos2) -> Shot {
    frame(c, app, size, vec![moved(p), button(p, true)]);
    frame(c, app, size, vec![button(p, false)]);
    frame(c, app, size, vec![])
}

/// Kürzel drücken und zwei Bilder nachlaufen lassen.
fn press(c: &egui::Context, app: &mut PlanoraApp, size: Vec2, k: Key, m: Modifiers) {
    frame(c, app, size, vec![key(k, m)]);
    frame(c, app, size, vec![]);
}

/// Pixel des Zielpunkts der Kamera im ganzen Fenster.
fn target_px(app: &mut PlanoraApp, size: Vec2) -> glam::Vec2 {
    let cam = state(app).camera;
    cam.project(cam.target, glam::Vec2::new(size.x, size.y))
        .unwrap()
}

/// Pixel der acht Ecken aller Körper.
fn corners_px(app: &mut PlanoraApp, size: Vec2) -> Vec<Pos2> {
    let s = state(app);
    let cam = s.camera;
    let mut out = Vec::new();
    for b in &s.bodies {
        let (min, max) = b.bounds;
        for i in 0..8 {
            let corner = Vec3::new(
                if i & 1 == 0 { min.x } else { max.x } as f32,
                if i & 2 == 0 { min.y } else { max.y } as f32,
                if i & 4 == 0 { min.z } else { max.z } as f32,
            );
            let px = cam
                .project(corner, glam::Vec2::new(size.x, size.y))
                .unwrap();
            out.push(pos2(px.x, px.y));
        }
    }
    out
}

fn assert_centred(app: &mut PlanoraApp, size: Vec2, what: &str) {
    let free = rects(app, size).free;
    let px = target_px(app, size);
    let d = (pos2(px.x, px.y) - free.center()).length();
    assert!(
        d < 0.5,
        "{what}: Ziel bei {px:?}, Mitte der freien Fläche {:?}",
        free.center()
    );
}

fn assert_inside_free(app: &mut PlanoraApp, size: Vec2, what: &str) {
    let free = rects(app, size).free;
    let corners = corners_px(app, size);
    assert!(!corners.is_empty(), "{what}: kein Körper");
    for p in corners {
        assert!(
            free.expand(0.5).contains(p),
            "{what}: Ecke bei {p:?} außerhalb der freien Fläche {free:?}"
        );
    }
}

/// Würfel ±`half` um den Ursprung liegt ganz in der freien Fläche.
fn assert_cube_inside_free(app: &mut PlanoraApp, size: Vec2, half: f32, what: &str) {
    let free = rects(app, size).free;
    let cam = state(app).camera;
    for i in 0..8 {
        let corner = Vec3::new(
            if i & 1 == 0 { -half } else { half },
            if i & 2 == 0 { -half } else { half },
            if i & 4 == 0 { -half } else { half },
        );
        let px = cam
            .project(corner, glam::Vec2::new(size.x, size.y))
            .unwrap();
        assert!(
            free.expand(0.5).contains(pos2(px.x, px.y)),
            "{what} {size:?}: Ecke bei {px:?} außerhalb der freien Fläche {free:?}"
        );
    }
}

/// Die Körper füllen die freie Fläche gut aus (nicht bloß winzig in der Mitte): die Hülle der
/// Ecken reicht in einer Richtung über mindestens die Hälfte.
fn assert_fills_free(app: &mut PlanoraApp, size: Vec2, what: &str) {
    let free = rects(app, size).free;
    let corners = corners_px(app, size);
    let hull = corners.iter().fold(Rect::NOTHING, |r, p| {
        r.union(Rect::from_min_size(*p, Vec2::ZERO))
    });
    assert!(
        hull.width() >= 0.5 * free.width() || hull.height() >= 0.5 * free.height(),
        "{what} {size:?}: Körper {hull:?} füllt die freie Fläche {free:?} kaum"
    );
}

/// Gespeichertes Dokument mit einem Quader `dims`, geschlossen; liefert Arbeitsbereich und Pfad.
fn saved_document(dims: DVec3) -> (Workspace, planora_project::library::NodePath) {
    let mut w = library_ws();
    let p = w.library.as_ref().unwrap().create_project("P").unwrap();
    w.active_tab_mut().state.create_box(dims).unwrap();
    w.save_as(&p, "Teil", "Anfang").unwrap();
    let path = w.active_tab().path.clone().unwrap();
    w.close_tab(0);
    (w, path)
}

/// Ein geöffnetes Dokument (Weg über den gespeicherten Stand) wird in die freie Fläche
/// eingepasst, bei jeder Fenstergröße; die Ansicht wird danach nicht noch einmal eingepasst.
#[test]
fn opened_documents_fit_inside_the_free_rect_at_all_sizes() {
    for size in [SMALL, MID, BIG] {
        for dims in [
            DVec3::new(300.0, 40.0, 200.0),
            DVec3::new(1000.0, 20.0, 20.0),
        ] {
            let c = context();
            let (w, path) = saved_document(dims);
            let mut app = PlanoraApp::for_tests(w);
            settle(&c, &mut app, size);
            app.ws_mut()
                .open_document(&path, LockMode::Acquire)
                .unwrap();
            assert!(state(&mut app).fit_pending, "geladen, noch nicht gezeigt");
            settle(&c, &mut app, size);
            let what = format!("Dokument {dims:?}");
            assert!(!state(&mut app).fit_pending, "{what}: einmal nachgeholt");
            assert_centred(&mut app, size, &what);
            assert_inside_free(&mut app, size, &what);
            assert_fills_free(&mut app, size, &what);
            // Danach bleibt eine Änderung der Ansicht durch den Nutzer stehen.
            state(&mut app).camera.zoom(-30.0);
            let d = state(&mut app).camera.distance;
            settle(&c, &mut app, size);
            assert_eq!(
                state(&mut app).camera.distance,
                d,
                "{what}: kein zweites Einpassen"
            );
        }
    }
}

/// Neuer Tab (Strg+N): die Standardansicht (±25 mm) liegt bei jeder Fenstergröße in der freien
/// Fläche, ohne dass von Hand eingepasst wird.
#[test]
fn new_tab_view_fits_the_free_rect_at_all_sizes() {
    for size in [SMALL, MID, BIG] {
        let c = context();
        let mut app = app_with_box();
        settle(&c, &mut app, size);
        press(&c, &mut app, size, Key::N, Modifiers::COMMAND);
        settle(&c, &mut app, size);
        assert_centred(&mut app, size, "neuer Tab");
        assert_cube_inside_free(&mut app, size, 25.0, "neuer Tab");
    }
}

/// Wird ein geladener Tab zuerst als Zeichnung oder in einer Skizze gezeigt, wartet das
/// Einpassen, bis die 3D-Ansicht ohne Skizze läuft.
#[test]
fn the_pending_fit_waits_for_the_model_view() {
    let c = context();
    let (w, path) = saved_document(DVec3::new(1000.0, 20.0, 20.0));
    let mut app = PlanoraApp::for_tests(w);
    settle(&c, &mut app, SMALL);
    app.ws_mut()
        .open_document(&path, LockMode::Acquire)
        .unwrap();
    let s = state(&mut app);
    let id = s
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    s.open_sketch(id).unwrap();
    settle(&c, &mut app, SMALL);
    assert!(
        state(&mut app).fit_pending,
        "in der Skizze nicht nachgeholt"
    );
    assert_centred(&mut app, SMALL, "Skizze");
    state(&mut app).close_sketch();
    settle(&c, &mut app, SMALL);
    assert!(!state(&mut app).fit_pending);
    assert_centred(&mut app, SMALL, "nach der Skizze");
    assert_inside_free(&mut app, SMALL, "nach der Skizze");

    // Zuerst als Zeichnung gezeigt.
    let (w, path) = saved_document(DVec3::new(1000.0, 20.0, 20.0));
    let mut app = PlanoraApp::for_tests(w);
    settle(&c, &mut app, SMALL);
    app.ws_mut()
        .open_document(&path, LockMode::Acquire)
        .unwrap();
    let s = state(&mut app);
    let sheet = Sheet {
        paper: Paper::A4,
        landscape: true,
    };
    let d = s
        .document
        .add_drawing("Blatt 1", sheet, Projection::First)
        .unwrap();
    s.drawing_ui.open(d);
    settle(&c, &mut app, SMALL);
    assert!(
        state(&mut app).fit_pending,
        "in der Zeichnung nicht nachgeholt"
    );
    state(&mut app).drawing_ui.area = WorkArea::Model;
    settle(&c, &mut app, SMALL);
    assert!(!state(&mut app).fit_pending);
    assert_inside_free(&mut app, SMALL, "nach der Zeichnung");
}

#[test]
fn free_rect_is_what_the_viewport_gets() {
    let c = context();
    for size in [BIG, MID, SMALL] {
        let mut app = app_with_box();
        settle(&c, &mut app, size);
        let used = shell_panels::last_layout(&c).unwrap().free;
        assert_eq!(used, rects(&mut app, size).free, "freie Fläche der App");
        assert_centred(&mut app, size, &format!("{size:?}"));
        // Seitenverhältnis und Höhenanteil der Kamera sind die der freien Fläche.
        let free = rects(&mut app, size).free;
        let cam = state(&mut app).camera;
        assert!((cam.visible_aspect - free.width() / free.height()).abs() < 1e-4);
        assert!((cam.visible_height_fraction - free.height() / size.y).abs() < 1e-4);
        assert!(
            (cam.aspect - size.x / size.y).abs() < 1e-4,
            "Viewport über das ganze Fenster"
        );
    }
}

#[test]
fn the_viewport_fills_the_window_under_the_fields() {
    let c = context();
    let mut app = app_with_box();
    let shot = settle(&c, &mut app, BIG);
    let bg = theme::viewport_background(true);
    let window = Rect::from_min_size(Pos2::ZERO, BIG);
    assert!(
        shot.fills.iter().any(|(r, f)| *f == bg && *r == window),
        "Hintergrund des 3D-Bereichs über das ganze Fenster"
    );
}

#[test]
fn view_menu_toggles_projects_and_versions_fields() {
    let c = context();
    let mut app = app_with_box();
    let shot = settle(&c, &mut app, BIG);
    let r = shell_panels::last_layout(&c).unwrap();
    assert!(shot.has_in("Projekte", r.left), "Projekte sichtbar");
    assert!(
        shot.has_in("Versionen", r.right),
        "Umschalter mit Versionen"
    );

    let fields = [r.left, r.right];
    // Menüs schließen bei einem Klick in ihnen (egui `PopupCloseBehavior::CloseOnClick`, die
    // Menüleiste ändert das nicht): jeder Umschalter braucht „Ansicht“ erneut.
    let menu_open = |shot: &Shot| !shot.all("Isometrisch").is_empty();
    for (entry, what) in [("Projekte", "Projekte"), ("Versionen", "Versionen")] {
        let shot = settle(&c, &mut app, BIG);
        assert!(!menu_open(&shot), "Menü ist zu");
        let shot = click(
            &c,
            &mut app,
            BIG,
            shot.find_outside("Ansicht", &fields).center(),
        );
        assert!(menu_open(&shot), "Ansicht ist offen");
        let shot = click(
            &c,
            &mut app,
            BIG,
            shot.find_outside(entry, &fields).center(),
        );
        assert!(!menu_open(&shot), "Klick auf {what} schließt das Menü");
    }
    assert!(!app.shell_mut().projects_visible);
    assert!(!app.shell_mut().versions_visible);
    let shot = settle(&c, &mut app, BIG);
    assert!(!shot.has_in("Projekte", r.left), "Projekte ausgeblendet");
    assert!(!shot.has_in("Versionen", r.right), "Versionen ausgeblendet");
    // Die Spalten bleiben gültig: Baum links, Eigenschaften rechts, freie Fläche unverändert.
    assert!(shot.has_in("Eigenschaften", r.right));
    assert!(shot.has_in("Quader 1", r.left), "Baum links");
    assert_centred(&mut app, BIG, "ohne Projekte und Versionen");
}

#[test]
fn ctrl_o_shows_the_projects_field() {
    let c = context();
    let mut app = app_with_box();
    app.shell_mut().projects_visible = false;
    let shot = settle(&c, &mut app, BIG);
    let r = shell_panels::last_layout(&c).unwrap();
    assert!(!shot.has_in("Projekte", r.left));
    press(&c, &mut app, BIG, Key::O, Modifiers::COMMAND);
    assert!(app.shell_mut().projects_visible);
    let shot = settle(&c, &mut app, BIG);
    assert!(
        shot.has_in("Projekte", r.left),
        "Strg+O blendet Projekte ein"
    );
}

/// Zustand mit Quader und offener Zeichnung (A4 quer).
fn app_with_drawing() -> PlanoraApp {
    let mut app = app_with_box();
    let s = state(&mut app);
    let sheet = Sheet {
        paper: Paper::A4,
        landscape: true,
    };
    let d = s
        .document
        .add_drawing("Blatt 1", sheet, Projection::First)
        .unwrap();
    s.drawing_ui.open(d);
    app
}

#[test]
fn drawing_area_stays_inside_the_free_rect() {
    for size in [BIG, MID, SMALL] {
        let c = context();
        let mut app = app_with_drawing();
        let shot = settle(&c, &mut app, size);
        assert!(matches!(
            state(&mut app).drawing_ui.area,
            WorkArea::Drawing(_)
        ));
        let r = rects(&mut app, size);
        let layout = drawing_view::last_layout(&c).expect("Zeichnung gezeichnet");
        let placed = fields(&c);
        assert_eq!(placed.len(), 5, "{placed:?}");
        assert!(
            r.free.expand(0.5).contains_rect(layout.area),
            "{size:?}: Zeichnung {:?} nicht in der freien Fläche {:?}",
            layout.area,
            r.free
        );
        for (name, field) in placed {
            let overlap = layout.area.intersect(field);
            assert!(
                overlap.width() <= 0.5 || overlap.height() <= 0.5,
                "{size:?}: Zeichnung überdeckt das Feld {name}: {overlap:?}"
            );
        }
        // Hintergrund über das ganze Fenster, darauf das Blatt.
        let window = Rect::from_min_size(Pos2::ZERO, size);
        let bg = theme::viewport_background(true);
        assert!(
            shot.fills.iter().any(|(rr, f)| *f == bg && *rr == window),
            "{size:?}: Hintergrund der Zeichnung über das ganze Fenster"
        );
    }
}

#[test]
fn dialogs_open_above_fields() {
    let c = context();
    let mut app = app_with_box();
    settle(&c, &mut app, MID);
    press(
        &c,
        &mut app,
        MID,
        Key::E,
        Modifiers::COMMAND | Modifiers::SHIFT,
    );
    let shot = settle(&c, &mut app, MID);
    assert!(app.ws_mut().active_tab().export_dialog.is_some());
    let placed = fields(&c);
    let title = shot.find_outside("Exportieren", &[field(&c, "oben")]);
    let dialog = c.layer_id_at(title.center()).expect("Ebene des Dialogs");
    assert_eq!(dialog.order, egui::Order::Foreground);
    let dialog_rect = c.memory(|m| m.area_rect(dialog.id)).unwrap();
    // Der Dialog (über die ganze Breite) liegt über beiden Seitenfeldern.
    for name in ["links", "rechts"] {
        let overlap = dialog_rect.intersect(field(&c, name));
        assert!(
            overlap.width() > 10.0 && overlap.height() > 10.0,
            "Dialog {dialog_rect:?} liegt über dem Feld {name}: {placed:?}"
        );
        assert_eq!(c.layer_id_at(overlap.center()), Some(dialog), "{name}");
    }
    // Klick auf einen Teil eines Felds außerhalb des Dialogs (Status, linkes Feld unten …).
    let outside = placed
        .iter()
        .flat_map(|(name, r)| {
            [
                r.center(),
                r.left_bottom() + vec2(8.0, -8.0),
                r.right_bottom() + vec2(-8.0, -8.0),
            ]
            .map(|p| (*name, p))
        })
        .find(|(_, p)| !dialog_rect.contains(*p))
        .expect("ein Feld ragt unter dem Dialog hervor");
    assert_eq!(c.layer_id_at(outside.1).unwrap().order, egui::Order::Middle);
    click(&c, &mut app, MID, outside.1);
    frame(&c, &mut app, MID, vec![]);
    assert!(
        app.ws_mut().active_tab().export_dialog.is_some(),
        "Dialog bleibt offen"
    );
    for name in ["links", "rechts"] {
        let overlap = dialog_rect.intersect(field(&c, name));
        assert_eq!(
            c.layer_id_at(overlap.center()),
            Some(dialog),
            "nach dem Klick auf {} bleibt der Dialog über dem Feld {name}",
            outside.0
        );
    }
}

#[test]
fn all_menu_entries_still_present_and_shortcuts_work() {
    let c = context();
    let mut app = app_with_box();
    let shot = settle(&c, &mut app, BIG);
    let top = field(&c, "oben");
    for m in ["Datei", "Erzeugen", "Zeichnung", "Bearbeiten", "Ansicht"] {
        assert!(shot.has_in(m, top), "Menü {m} im oberen Feld");
    }

    // Strg+N: neuer Tab.
    press(&c, &mut app, BIG, Key::N, Modifiers::COMMAND);
    assert_eq!(app.ws_mut().tabs.len(), 2);
    assert_eq!(app.ws_mut().active, 1);
    // Strg+W: schließt den (unveränderten) neuen Tab.
    press(&c, &mut app, BIG, Key::W, Modifiers::COMMAND);
    assert_eq!(app.ws_mut().tabs.len(), 1);
    // Strg+O: Projekte einblenden.
    app.shell_mut().projects_visible = false;
    press(&c, &mut app, BIG, Key::O, Modifiers::COMMAND);
    assert!(app.shell_mut().projects_visible);
    // Strg+Umschalt+E: Export.
    press(
        &c,
        &mut app,
        BIG,
        Key::E,
        Modifiers::COMMAND | Modifiers::SHIFT,
    );
    assert!(app.ws_mut().active_tab().export_dialog.is_some());
    app.ws_mut().active_tab_mut().export_dialog = None;
    // Strg+S: unbenannt → „Speichern unter“.
    press(&c, &mut app, BIG, Key::S, Modifiers::COMMAND);
    assert!(matches!(
        app.ws_mut().prompt,
        Some(planora_app::workspace::Prompt::SaveAs { .. })
    ));
}

#[test]
fn sketch_window_starts_right_of_the_left_field() {
    let c = context();
    let mut app = app_with_box();
    let s = state(&mut app);
    let id = s
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    s.open_sketch(id).unwrap();
    let shot = settle(&c, &mut app, BIG);
    let placed: Vec<Rect> = fields(&c).into_iter().map(|(_, r)| r).collect();
    let title = shot.find_outside("Skizze", &placed);
    let layer = c.layer_id_at(title.center()).unwrap();
    assert_eq!(layer.order, egui::Order::Foreground);
    let window = c.memory(|m| m.area_rect(layer.id)).unwrap();
    let want = pos2(SHELL_INSET + theme::PANEL_WIDTH + theme::SPACE_6, 90.0);
    assert!(
        (window.min.x - want.x).abs() < 0.5,
        "{:?} statt {want:?}",
        window.min
    );
    // Oben bei 90, außer egui schiebt das hohe Fenster zurück ins Fenster (Unterkante am Rand).
    assert!(
        (window.min.y - want.y).abs() < 0.5 || (window.bottom() - BIG.y).abs() < 0.5,
        "{window:?}"
    );
    let (left, top) = (field(&c, "links"), field(&c, "oben"));
    assert!(window.left() > left.right(), "rechts neben dem linken Feld");
    assert!(window.top() > top.bottom(), "unter dem oberen Feld");
}

#[test]
fn new_tab_open_document_frame_all_centres_in_the_free_rect() {
    let c = context();
    // Alles einpassen.
    let mut app = app_with_box();
    settle(&c, &mut app, BIG);
    state(&mut app).camera.orbit(80.0, 30.0);
    state(&mut app).camera.zoom(-40.0);
    state(&mut app).frame_all();
    settle(&c, &mut app, BIG);
    assert_centred(&mut app, BIG, "Alles einpassen");
    assert_inside_free(&mut app, BIG, "Alles einpassen");

    // Neuer Tab (Strg+N): die Standardansicht (Würfel ±25 mm um den Ursprung) liegt ohne
    // Zutun in der freien Fläche; der erste Körper wird beim Anlegen selbst eingepasst.
    press(&c, &mut app, BIG, Key::N, Modifiers::COMMAND);
    settle(&c, &mut app, BIG);
    assert_centred(&mut app, BIG, "neuer Tab");
    assert_cube_inside_free(&mut app, BIG, 25.0, "neuer Tab");
    state(&mut app)
        .create_box(DVec3::new(300.0, 20.0, 20.0))
        .unwrap();
    settle(&c, &mut app, BIG);
    assert_inside_free(&mut app, BIG, "neuer Tab, erster Körper");

    // Dokument öffnen: gespeichert, geschlossen, wieder geöffnet.
    let mut w = library_ws();
    let p = w.library.as_ref().unwrap().create_project("P").unwrap();
    w.active_tab_mut()
        .state
        .create_box(DVec3::new(300.0, 40.0, 200.0))
        .unwrap();
    w.save_as(&p, "Teil", "Anfang").unwrap();
    let path = w.active_tab().path.clone().unwrap();
    w.close_tab(0);
    let mut app = PlanoraApp::for_tests(w);
    settle(&c, &mut app, BIG);
    app.ws_mut()
        .open_document(&path, LockMode::Acquire)
        .unwrap();
    settle(&c, &mut app, BIG);
    assert_centred(&mut app, BIG, "Dokument geöffnet");
    assert_inside_free(&mut app, BIG, "Dokument geöffnet");

    // Skizze öffnen: Bildmitte in der freien Fläche.
    let s = state(&mut app);
    let id = s
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    s.open_sketch(id).unwrap();
    settle(&c, &mut app, BIG);
    assert_centred(&mut app, BIG, "Skizze geöffnet");
    // Nach der Skizze bleibt der Versatz.
    state(&mut app).close_sketch();
    settle(&c, &mut app, BIG);
    assert_centred(&mut app, BIG, "Skizze geschlossen");
}

#[test]
fn read_only_tab_shows_the_banner_and_centres_in_the_raised_free_rect() {
    let c = context();
    let mut app = PlanoraApp::for_tests(read_only_ws());
    let shot = settle(&c, &mut app, BIG);
    let r = rects(&mut app, BIG);
    let normal = shell::layout(
        Rect::from_min_size(Pos2::ZERO, BIG),
        &ShellState::default(),
        0.0,
        0.0,
        0.0,
        false,
    );
    assert!(r.free.top() > normal.free.top(), "freie Fläche oben tiefer");
    let banner = r.banner.expect("Hinweisfeld");
    assert!(
        shot.texts
            .iter()
            .any(|t| t.text.starts_with("Alte Version v1") && banner.contains_rect(t.rect)),
        "Hinweis im Hinweisfeld"
    );
    assert_centred(&mut app, BIG, "schreibgeschützt");
    assert_inside_free(&mut app, BIG, "schreibgeschützt");
}

/// Skizzen-Modus, freies Rechteck mittig (Versatz 0): nach dem Bild gelten Seitenverhältnis und
/// Höhenanteil des freien Rechtecks, auch wenn das Overlay zwischendurch `set_viewport` ruft.
#[test]
fn sketch_mode_with_a_centred_free_rect_keeps_its_visible_aspect() {
    let c = context();
    let mut s = AppState::new();
    let id = s
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    s.open_sketch(id).unwrap();
    let size = vec2(800.0, 600.0);
    let free = Rect::from_center_size(pos2(400.0, 300.0), vec2(400.0, 300.0));
    for _ in 0..2 {
        c.run_ui(input(size, vec![]), |ui| {
            viewport::show_in(ui, &mut s, Some(free))
        })
        .drop_without_applying_deltas();
    }
    assert!(
        (s.camera.visible_aspect - 400.0 / 300.0).abs() < 1e-4,
        "{}",
        s.camera.visible_aspect
    );
    assert!((s.camera.visible_height_fraction - 0.5).abs() < 1e-4);
    assert_eq!(s.camera.ndc_offset, glam::Vec2::ZERO);
}

/// Skizzen-Modus in der App: Klick und Mausrad auf einem Feld setzen keinen Punkt und zoomen
/// nicht; in der freien Fläche schon.
#[test]
fn sketch_mode_clicks_and_wheel_on_a_field_do_not_reach_the_sketch() {
    let c = context();
    let mut app = PlanoraApp::for_tests(library_ws());
    let s = state(&mut app);
    let id = s
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    s.open_sketch(id).unwrap();
    let (se, sk) = s.session_sketch_mut().unwrap();
    se.set_tool(sk, Tool::Point);
    settle(&c, &mut app, BIG);
    let r = rects(&mut app, BIG);
    let points = |app: &mut PlanoraApp| state(app).session_sketch().unwrap().points().count();

    for (name, p) in [
        ("links", field(&c, "links").center()),
        ("rechts", field(&c, "rechts").center()),
        ("oben", field(&c, "oben").left_center() + vec2(6.0, 0.0)),
        ("Status", field(&c, "Status").center()),
        ("unten", field(&c, "unten").right_center() - vec2(6.0, 0.0)),
    ] {
        assert_eq!(
            c.layer_id_at(p).unwrap().order,
            egui::Order::Middle,
            "{name}"
        );
        click(&c, &mut app, BIG, p);
        assert_eq!(points(&mut app), 0, "Klick auf {name} setzt keinen Punkt");
        let d = state(&mut app).camera.distance;
        frame(&c, &mut app, BIG, vec![moved(p), wheel(120.0)]);
        frame(&c, &mut app, BIG, vec![]);
        assert_eq!(
            state(&mut app).camera.distance,
            d,
            "Mausrad auf {name} zoomt nicht"
        );
    }

    // Gegenprobe in der freien Fläche (rechts unten, abseits des Skizzenfensters).
    let p = pos2(r.free.right() - 60.0, r.free.bottom() - 60.0);
    assert!(c.layer_id_at(p).unwrap().order == egui::Order::Background);
    let d = state(&mut app).camera.distance;
    frame(&c, &mut app, BIG, vec![moved(p), wheel(120.0)]);
    frame(&c, &mut app, BIG, vec![]);
    assert_ne!(
        state(&mut app).camera.distance,
        d,
        "Mausrad in der freien Fläche zoomt"
    );
    click(&c, &mut app, BIG, p);
    assert_eq!(
        points(&mut app),
        1,
        "Klick in der freien Fläche setzt einen Punkt"
    );
}
