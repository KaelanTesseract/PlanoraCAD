//! Zeichnungsmodus mit schwebenden Feldern (Graphit, Etappe C, Aufgabe 4): Werkzeugfeld unter
//! dem oberen Feld, Ansichtsfeld rechts, Hinweisfeld unten; das Blatt wird über das ganze Fenster
//! gemalt, eingepasst und zentriert wird es in der freien Fläche. Ohne Fenster: Rohereignisse je
//! Bild gegen einen egui-Kontext, mit der ganzen App (`eframe::App::ui`) oder nur dem
//! Zeichnungsbereich (`drawing_view::show_with`, ohne Shell).

mod common;

use eframe::App as _;
use egui::epaint::{ClippedShape, Shape};
use egui::{Color32, Event, Modifiers, PointerButton, Pos2, Rect, Vec2, pos2, vec2};
use planora_app::app::PlanoraApp;
use planora_app::browser::ERROR_COLOR;
use planora_app::dimension_tool::{READ_ONLY_TIP, TOOL};
use planora_app::drawing_view::{self, FIT_MARGIN_PX};
use planora_app::shell::{self, ShellMode, ShellState};
use planora_app::state::AppState;
use planora_app::workspace::Workspace;
use planora_app::{fonts, icons, shell_panels, theme};
use planora_drawing::dimension::{
    DimId, DimKind, DimSpec, GeoRef, GeoRefData, Tolerance as DimTolerance,
};
use planora_drawing::{DrawingId, Paper, Placement, Projection, Scale, Sheet, ViewId, ViewKind};
use planora_feature::geo_ref_for_edge;
use planora_math::{DVec2, DVec3, Tolerance};
use planora_project::config::Config;
use planora_project::library::Library;

const BIG: Vec2 = vec2(1400.0, 900.0);
const MID: Vec2 = vec2(1100.0, 700.0);
const HARNESS: Vec2 = vec2(1200.0, 800.0);

const A4_QUER: Sheet = Sheet {
    paper: Paper::A4,
    landscape: true,
};

fn context() -> egui::Context {
    let c = egui::Context::default();
    c.set_fonts(fonts::definitions());
    theme::apply(&c, true);
    // Hinweistexte sofort zeigen.
    c.all_styles_mut(|s| s.interaction.tooltip_delay = 0.0);
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

#[derive(Debug, Clone)]
struct Seen {
    text: String,
    rect: Rect,
    clip: Rect,
    color: Color32,
}

#[derive(Default)]
struct Shot {
    texts: Vec<Seen>,
    fills: Vec<(Rect, Color32)>,
}

fn walk(shape: &Shape, clip: Rect, s: &mut Shot) {
    match shape {
        Shape::Vec(v) => v.iter().for_each(|x| walk(x, clip, s)),
        Shape::Text(t) => {
            let section = t.galley.job.sections.first();
            let mut color = section.map_or(t.fallback_color, |x| x.format.color);
            if color == Color32::PLACEHOLDER {
                color = t.fallback_color;
            }
            if let Some(c) = t.override_text_color {
                color = c;
            }
            s.texts.push(Seen {
                text: t.galley.text().to_string(),
                rect: Rect::from_min_size(t.pos, t.galley.size()),
                clip,
                color,
            });
        }
        Shape::Rect(r) => s.fills.push((r.rect, r.fill)),
        _ => {}
    }
}

impl Shot {
    fn of(shapes: &[ClippedShape]) -> Shot {
        let mut s = Shot::default();
        for c in shapes {
            walk(&c.shape, c.clip_rect, &mut s);
        }
        s
    }

    fn find_in(&self, r: Rect, exact: &str) -> &Seen {
        self.texts
            .iter()
            .find(|t| t.text == exact && r.expand(0.5).contains(t.rect.center()))
            .unwrap_or_else(|| {
                panic!(
                    "„{exact}“ fehlt in {r:?}: {:?}",
                    self.texts
                        .iter()
                        .filter(|t| r.contains(t.rect.center()))
                        .map(|t| &t.text)
                        .collect::<Vec<_>>()
                )
            })
    }

    fn has(&self, exact: &str) -> bool {
        self.texts.iter().any(|t| t.text == exact)
    }

    fn contains(&self, part: &str) -> bool {
        self.texts.iter().any(|t| t.text.contains(part))
    }
}

fn moved(p: Pos2) -> Event {
    Event::PointerMoved(p)
}

fn press(p: Pos2, button: PointerButton, pressed: bool) -> Event {
    Event::PointerButton {
        pos: p,
        button,
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

fn field(c: &egui::Context, name: &str) -> Rect {
    shell_panels::field_rects(c)
        .into_iter()
        .find(|(n, _)| *n == name)
        .unwrap_or_else(|| panic!("Feld {name} fehlt: {:?}", shell_panels::field_rects(c)))
        .1
}

fn names(c: &egui::Context) -> Vec<&'static str> {
    shell_panels::field_rects(c)
        .into_iter()
        .map(|(n, _)| n)
        .collect()
}

/// Echte Überlappung (bloßes Berühren der Kanten zählt nicht).
fn overlaps(a: Rect, b: Rect) -> bool {
    a.min.x < b.max.x && b.min.x < a.max.x && a.min.y < b.max.y && b.min.y < a.max.y
}

fn compute(st: &mut AppState) {
    st.drawing_ui.tick(&st.bodies, &st.document, true);
}

/// Quader 100 × 60 × 40 und Zeichnung „Blatt 1“ (A4 quer) mit Vorderansicht 1:1 in der Mitte,
/// geöffnet und berechnet.
fn drawing_state() -> (AppState, DrawingId, ViewId) {
    let mut st = AppState::new();
    st.create_box(DVec3::new(100.0, 60.0, 40.0)).unwrap();
    let d = st
        .document
        .add_drawing("Blatt 1", A4_QUER, Projection::First)
        .unwrap();
    let front = st
        .document
        .add_view(
            d,
            ViewKind::Front,
            Scale { num: 1, den: 1 },
            Placement::Free { x: 148.5, y: 105.0 },
            None,
        )
        .unwrap();
    st.drawing_ui.open(d);
    compute(&mut st);
    (st, d, front)
}

fn view(st: &AppState, d: DrawingId, v: ViewId) -> Option<planora_drawing::ViewSpec> {
    st.document.drawing(d).unwrap().view(v).cloned()
}

// ---- Harness ohne Shell: nur `drawing_view::show_with` ---------------------------------------

struct Bare {
    c: egui::Context,
    size: Vec2,
    editable: bool,
}

impl Bare {
    fn new(size: Vec2, editable: bool) -> Bare {
        Bare {
            c: context(),
            size,
            editable,
        }
    }

    fn frame(&mut self, st: &mut AppState, events: Vec<Event>) -> Shot {
        let editable = self.editable;
        let out = self.c.run_ui(input(self.size, events), |ui| {
            drawing_view::show_with(ui, st, editable);
        });
        let shot = Shot::of(&out.shapes);
        out.drop_without_applying_deltas();
        shot
    }

    fn settle(&mut self, st: &mut AppState) -> Shot {
        self.frame(st, vec![]);
        self.frame(st, vec![]);
        self.frame(st, vec![])
    }

    fn click(&mut self, st: &mut AppState, p: Pos2) -> Shot {
        self.frame(st, vec![moved(p)]);
        self.frame(st, vec![moved(p), press(p, PointerButton::Primary, true)]);
        self.frame(st, vec![press(p, PointerButton::Primary, false)]);
        self.frame(st, vec![])
    }

    /// Klick auf den Text `exact` im Feld `name`.
    fn click_in(&mut self, st: &mut AppState, name: &str, exact: &str) -> Shot {
        let shot = self.settle(st);
        let r = field(&self.c, name);
        let p = shot.find_in(r, exact).rect.center();
        self.click(st, p)
    }

    fn wheel_at(&mut self, st: &mut AppState, p: Pos2) {
        self.frame(st, vec![moved(p)]);
        self.frame(st, vec![moved(p), wheel(-120.0)]);
        // Das Mausrad wirkt sanft über mehrere Bilder: ausklingen lassen, bevor der Zeiger geht.
        for _ in 0..30 {
            self.frame(st, vec![]);
        }
    }

    fn drag(&mut self, st: &mut AppState, from: Pos2, by: Vec2, button: PointerButton) {
        let to = from + by;
        self.frame(st, vec![moved(from)]);
        self.frame(st, vec![press(from, button, true)]);
        self.frame(st, vec![moved(from + by * 0.5)]);
        self.frame(st, vec![moved(to)]);
        self.frame(st, vec![press(to, button, false)]);
        self.frame(st, vec![]);
    }

    fn layout(&self) -> drawing_view::DrawingLayout {
        drawing_view::last_layout(&self.c).expect("Zeichnung gezeichnet")
    }
}

/// Geometrie der Felder ohne Shell, wie `drawing_view` sie selbst berechnet.
fn bare_rects(size: Vec2, read_only: bool) -> shell::ShellRects {
    shell::layout_in_mode(
        Rect::from_min_size(Pos2::ZERO, size),
        ShellMode::Drawing,
        &ShellState::default(),
        (size.x, 0.0),
        f32::INFINITY,
        0.0,
        read_only,
    )
}

// ---- Harness mit der ganzen App --------------------------------------------------------------

fn library_ws() -> Workspace {
    let root = common::temp_root("planora-drawing-fields");
    Workspace::new(Some(Library::open(&root).unwrap()), Config::default())
}

fn app_with_drawing() -> PlanoraApp {
    let mut app = PlanoraApp::for_tests(library_ws());
    let (st, _, _) = drawing_state();
    app.ws_mut().active_tab_mut().state = st;
    app
}

fn app_frame(c: &egui::Context, app: &mut PlanoraApp, size: Vec2, events: Vec<Event>) -> Shot {
    let mut f = eframe::Frame::_new_kittest();
    let out = c.run_ui(input(size, events), |ui| app.ui(ui, &mut f));
    let shot = Shot::of(&out.shapes);
    out.drop_without_applying_deltas();
    shot
}

fn app_settle(c: &egui::Context, app: &mut PlanoraApp, size: Vec2) -> Shot {
    app_frame(c, app, size, vec![]);
    app_frame(c, app, size, vec![]);
    app_frame(c, app, size, vec![])
}

fn app_click(c: &egui::Context, app: &mut PlanoraApp, size: Vec2, p: Pos2) -> Shot {
    app_frame(c, app, size, vec![moved(p)]);
    app_frame(
        c,
        app,
        size,
        vec![moved(p), press(p, PointerButton::Primary, true)],
    );
    app_frame(c, app, size, vec![press(p, PointerButton::Primary, false)]);
    app_frame(c, app, size, vec![])
}

fn app_state(app: &mut PlanoraApp) -> &mut AppState {
    &mut app.ws_mut().active_tab_mut().state
}

/// Papier auf dem Bildschirm aus der zuletzt gezeichneten Aufteilung.
fn paper(l: &drawing_view::DrawingLayout) -> Rect {
    let to = |p: DVec2| drawing_view::sheet_to_screen(l.canvas, l.sheet_mm, l.zoom, l.pan, p);
    Rect::from_two_pos(to(DVec2::ZERO), to(DVec2::new(l.sheet_mm.0, l.sheet_mm.1)))
}

// (c) ------------------------------------------------------------------------------------------

#[test]
fn sheet_paints_over_the_whole_window_and_fits_into_free() {
    for size in [BIG, MID] {
        let c = context();
        let mut app = app_with_drawing();
        let shot = app_settle(&c, &mut app, size);
        let window = Rect::from_min_size(Pos2::ZERO, size);
        let r = shell_panels::last_layout(&c).unwrap();
        let l = drawing_view::last_layout(&c).unwrap();
        assert_eq!(l.paint, window, "{size:?}: Malfläche = Fenster");
        assert_eq!(l.area, window);
        assert_eq!(l.canvas, r.free, "{size:?}: Einpassfläche = freie Fläche");
        assert_eq!(Some(l.toolbar), r.tool);
        assert_eq!(l.hint, r.bottom);
        // Die Umgebung des Blatts füllt das ganze Fenster (kein Hintergrund des 3D-Bereichs).
        let surround = theme::drawing_surround(true);
        assert!(
            shot.fills
                .iter()
                .any(|(rr, f)| *f == surround && *rr == window),
            "{size:?}: Umgebung über das ganze Fenster"
        );
        assert!(
            !shot
                .fills
                .iter()
                .any(|(rr, f)| *f == theme::viewport_background(true) && *rr == window),
            "{size:?}: kein 3D-Hintergrund unter dem Blatt"
        );

        // Vergrößern und verschieben, dann „Blatt einpassen“: das Papier sitzt in der freien
        // Fläche mit dem Rand FIT_MARGIN_PX und füllt sie (mindestens 90 % einer Richtung).
        app_state(&mut app).drawing_ui.zoom = 3.0;
        app_state(&mut app).drawing_ui.pan = vec2(-200.0, 120.0);
        let shot = app_settle(&c, &mut app, size);
        let tool = field(&c, "Werkzeugfeld");
        let p = shot.find_in(tool, "Blatt einpassen").rect.center();
        app_click(&c, &mut app, size, p);
        let shot = app_settle(&c, &mut app, size);
        let l = drawing_view::last_layout(&c).unwrap();
        assert_eq!(l.zoom, 1.0);
        let paper = paper(&l);
        let inner = r.free.shrink(FIT_MARGIN_PX - 0.5);
        assert!(
            inner.contains_rect(paper),
            "{size:?}: Papier {paper:?} nicht in der freien Fläche {:?}",
            r.free
        );
        let fill = (paper.width() / (r.free.width() - 2.0 * FIT_MARGIN_PX))
            .max(paper.height() / (r.free.height() - 2.0 * FIT_MARGIN_PX));
        assert!(fill > 0.9, "{size:?}: Füllgrad {fill}");
        assert!((paper.center() - r.free.center()).length() < 1.0, "mittig");
        // Das Papier ist wirklich so gemalt.
        assert!(
            shot.fills.iter().any(|(rr, f)| *f == theme::PAPER
                && (rr.min - paper.min).length() < 1.0
                && (rr.max - paper.max).length() < 1.0),
            "{size:?}: Papier nicht gemalt"
        );
        // Kein Feld liegt in der freien Fläche.
        for (name, f) in shell_panels::field_rects(&c) {
            assert!(
                !overlaps(f, r.free),
                "{size:?}: {name} {f:?} in der freien Fläche"
            );
        }
    }
}

// (d) ------------------------------------------------------------------------------------------

#[test]
fn tool_field_has_all_toolbar_texts_in_two_rows() {
    for size in [MID, BIG] {
        let c = context();
        let mut app = app_with_drawing();
        let shot = app_settle(&c, &mut app, size);
        let tool = field(&c, "Werkzeugfeld");
        assert_eq!(Some(tool), shell_panels::last_layout(&c).unwrap().tool);
        let zoom_out = icons::glyph(icons::Icon::ZoomOut).to_string();
        let zoom_in = icons::glyph(icons::Icon::ZoomIn).to_string();
        let mut rows: Vec<f32> = Vec::new();
        for must in [
            "3D-Modell",
            "Zeichnung: Blatt 1",
            "Ansicht hinzufügen",
            "Blatt einpassen",
            zoom_out.as_str(),
            zoom_in.as_str(),
            drawing_view::SUGGEST,
            TOOL,
        ] {
            let t = shot.find_in(tool, must);
            assert!(
                tool.contains_rect(t.rect),
                "{size:?}: „{must}“ ragt aus dem Werkzeugfeld {tool:?}: {:?}",
                t.rect
            );
            assert!(
                t.clip.expand(0.5).contains_rect(t.rect),
                "{size:?}: „{must}“ abgeschnitten ({:?} außerhalb {:?})",
                t.rect,
                t.clip
            );
            let y = t.rect.center().y;
            if !rows.iter().any(|r| (r - y).abs() < 6.0) {
                rows.push(y);
            }
        }
        assert_eq!(rows.len(), 2, "{size:?}: zwei Zeilen: {rows:?}");
        // Die Einstellungen der Ansicht stehen nicht mehr in der Leiste.
        assert!(
            !shot
                .texts
                .iter()
                .any(|t| tool.contains(t.rect.center()) && t.text == "Ansicht löschen")
        );
    }
}

// (e) ------------------------------------------------------------------------------------------

#[test]
fn view_settings_field_shows_view_state_and_acts() {
    let (mut st, d, front) = drawing_state();
    let mut h = Bare::new(HARNESS, true);
    // Ohne Auswahl: Hinweis.
    let shot = h.settle(&mut st);
    let side = field(&h.c, "Ansichtsfeld");
    shot.find_in(side, "Ansicht");
    shot.find_in(side, drawing_view::NO_VIEW_SELECTED);
    assert!(!shot.has("Verdeckte Kanten zeigen"));
    assert!(!shot.has("Ansicht löschen"));
    // Mit Auswahl: Name, Maßstab, Schalter und „Ansicht löschen“.
    st.drawing_ui.selected_view = Some(front);
    let shot = h.settle(&mut st);
    let side = field(&h.c, "Ansichtsfeld");
    assert_eq!(side, bare_rects(HARNESS, false).right.intersect(side));
    for must in [
        "Ansicht „Vorne“",
        "Maßstab",
        "1:1",
        "Verdeckte Kanten zeigen",
        drawing_view::CENTERLINES,
        "Ansicht löschen",
    ] {
        let t = shot.find_in(side, must);
        assert!(
            t.clip.expand(0.5).contains_rect(t.rect),
            "„{must}“ abgeschnitten"
        );
    }
    assert!(!shot.has(drawing_view::NO_VIEW_SELECTED));

    // Verdeckte Kanten: Schalter ändert das Dokument.
    let before = view(&st, d, front).unwrap().show_hidden;
    h.click_in(&mut st, "Ansichtsfeld", "Verdeckte Kanten zeigen");
    assert_eq!(view(&st, d, front).unwrap().show_hidden, !before);
    // Mittellinien.
    let before = view(&st, d, front).unwrap().centerlines;
    h.click_in(&mut st, "Ansichtsfeld", drawing_view::CENTERLINES);
    assert_eq!(view(&st, d, front).unwrap().centerlines, !before);
    // Maßstab: Auswahl öffnen und 1:2 wählen.
    h.click_in(&mut st, "Ansichtsfeld", "1:1");
    let shot = h.settle(&mut st);
    let p = shot
        .texts
        .iter()
        .find(|t| t.text == "1:2")
        .expect("Auswahl 1:2")
        .rect
        .center();
    h.click(&mut st, p);
    assert_eq!(view(&st, d, front).unwrap().scale, Scale { num: 1, den: 2 });
    // Ansicht löschen (keine abgeleiteten Ansichten, keine Maße: sofort).
    h.click_in(&mut st, "Ansichtsfeld", "Ansicht löschen");
    assert!(view(&st, d, front).is_none(), "Ansicht gelöscht");
    let shot = h.settle(&mut st);
    shot.find_in(field(&h.c, "Ansichtsfeld"), drawing_view::NO_VIEW_SELECTED);
}

// (f) ------------------------------------------------------------------------------------------

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

fn edge_where(st: &AppState, body: usize, pred: impl Fn(DVec3) -> bool) -> GeoRefData {
    let solid = &st.bodies[body].solid;
    let (id, _) = solid
        .edges
        .iter()
        .find(|(_, e)| {
            let a = solid.vertices.get(e.start).unwrap().point;
            let b = solid.vertices.get(e.end).unwrap().point;
            e.start != e.end && pred(a) && pred(b)
        })
        .expect("Kante nicht gefunden");
    geo_ref_for_edge(solid, id, &Tolerance::default()).unwrap()
}

fn spec(view: ViewId, kind: DimKind, refs: Vec<GeoRef>, offset: f64) -> DimSpec {
    DimSpec {
        id: DimId(0),
        view,
        kind,
        refs,
        offset,
        angle: 45.0,
        text_shift: (0.0, 0.0),
        tolerance: DimTolerance::None,
        decimals: 1,
        text_override: None,
        chain: None,
        suggested: false,
    }
}

fn add_width(st: &mut AppState, d: DrawingId, view: ViewId, body: usize, offset: f64) -> DimId {
    let (lo, hi) = st.bodies[body].bounds;
    let refs = vec![
        GeoRef::Edge(edge_where(st, body, |p| near(p.x, lo.x) && near(p.y, lo.y))),
        GeoRef::Edge(edge_where(st, body, |p| near(p.x, hi.x) && near(p.y, lo.y))),
    ];
    st.document
        .add_dimension(d, spec(view, DimKind::Horizontal, refs, offset))
        .unwrap()
}

fn add_height(st: &mut AppState, d: DrawingId, view: ViewId, body: usize, offset: f64) -> DimId {
    let (lo, hi) = st.bodies[body].bounds;
    let refs = vec![
        GeoRef::Edge(edge_where(st, body, |p| near(p.z, lo.z) && near(p.y, lo.y))),
        GeoRef::Edge(edge_where(st, body, |p| near(p.z, hi.z) && near(p.y, lo.y))),
    ];
    st.document
        .add_dimension(d, spec(view, DimKind::Vertical, refs, offset))
        .unwrap()
}

#[test]
fn hint_field_shows_dimension_hints_and_messages() {
    // Zwei Quader; ein Maß am zweiten (danach gelöscht: Bezug verloren), eins weit außerhalb.
    let mut st = AppState::new();
    st.create_box(DVec3::new(10.0, 20.0, 30.0)).unwrap();
    let b = st.create_box(DVec3::new(10.0, 20.0, 30.0)).unwrap();
    let d = st
        .document
        .add_drawing("Blatt 1", A4_QUER, Projection::First)
        .unwrap();
    let front = st
        .document
        .add_view(
            d,
            ViewKind::Front,
            Scale { num: 1, den: 1 },
            Placement::Free { x: 120.0, y: 100.0 },
            None,
        )
        .unwrap();
    add_width(&mut st, d, front, 1, 10.0);
    st.drawing_ui.open(d);
    compute(&mut st);
    let mut h = Bare::new(HARNESS, true);
    h.settle(&mut st);
    st.delete_feature(b).unwrap();
    compute(&mut st);
    add_height(&mut st, d, front, 0, 5000.0);
    let shot = h.settle(&mut st);
    let hint = field(&h.c, "Hinweisfeld");
    assert_eq!(h.layout().hint, hint);
    let lost = shot.find_in(hint, "1 Maß hat seinen Bezug verloren.");
    assert_eq!(lost.color, ERROR_COLOR, "verlorene Maße rot");
    shot.find_in(hint, "1 Maß liegt außerhalb des Blatts.");
    assert!(
        shot.texts
            .iter()
            .any(|t| t.text.contains(drawing_view::HINT) && hint.contains(t.rect.center())),
        "Hinweis im Hinweisfeld"
    );
    // Meldung (Vorschlag) im Feld.
    st.drawing_ui.dim_message = Some("3 Maße wurden hinzugefügt.".into());
    st.drawing_ui.dim_message_key = Some((
        st.document.edit_count(),
        st.drawing_ui.selected_view,
        st.drawing_ui.selected_dim,
    ));
    let shot = h.settle(&mut st);
    shot.find_in(hint, "3 Maße wurden hinzugefügt.");
    // Werkzeug „Maß“: Hinweis des Werkzeugs; ein Klick ins Leere wird rot abgelehnt.
    st.drawing_ui.dim_message = None;
    h.click_in(&mut st, "Werkzeugfeld", TOOL);
    assert!(st.dimension_tool.is_active());
    let shot = h.settle(&mut st);
    let tool_hint = st.dimension_tool.hint().expect("Werkzeughinweis");
    assert!(
        shot.texts
            .iter()
            .any(|t| t.text == tool_hint && hint.contains(t.rect.center())),
        "Werkzeughinweis „{tool_hint}“ im Hinweisfeld"
    );
    let free = h.layout().canvas;
    h.click(&mut st, free.left_top() + vec2(30.0, 30.0));
    let shot = h.settle(&mut st);
    let rejected = st.dimension_tool.message().expect("Ablehnung").to_owned();
    let t = shot.find_in(hint, &rejected);
    assert_eq!(t.color, ERROR_COLOR, "Ablehnung rot");
    // Alles bleibt im Feld: höchstens zwei Zeilen, nichts ragt hinaus.
    let mut rows: Vec<f32> = Vec::new();
    for t in shot.texts.iter().filter(|t| hint.contains(t.rect.center())) {
        assert!(
            hint.expand(0.5).contains_rect(t.rect) || t.clip.expand(0.5).contains_rect(t.rect),
            "„{}“ ragt hinaus",
            t.text
        );
        let y = t.rect.top();
        if !rows.iter().any(|r| (r - y).abs() < 4.0) {
            rows.push(y);
        }
    }
    assert!(rows.len() <= 2, "höchstens zwei Zeilen: {rows:?}");
}

// (g) ------------------------------------------------------------------------------------------

#[test]
fn pointer_on_drawing_fields_does_not_reach_the_sheet() {
    let (mut st, _, front) = drawing_state();
    let mut h = Bare::new(HARNESS, true);
    st.drawing_ui.selected_view = Some(front);
    h.settle(&mut st);
    let tool = field(&h.c, "Werkzeugfeld");
    let side = field(&h.c, "Ansichtsfeld");
    let hint = field(&h.c, "Hinweisfeld");
    // Freie Stellen in den Feldern (keine Knöpfe), etwas innerhalb des Rands.
    let points = [
        ("Werkzeugfeld", tool.right_bottom() - vec2(12.0, 12.0)),
        ("Ansichtsfeld", side.right_bottom() - vec2(12.0, 8.0)),
        ("Hinweisfeld", hint.right_bottom() - vec2(6.0, 6.0)),
        ("Werkzeugfeld (Rand)", tool.left_center() + vec2(2.0, 0.0)),
        ("Hinweisfeld (Rand)", hint.center_top() + vec2(0.0, 2.0)),
    ];
    for (name, p) in points {
        let (zoom, pan) = (st.drawing_ui.zoom, st.drawing_ui.pan);
        h.wheel_at(&mut st, p);
        assert_eq!(st.drawing_ui.zoom, zoom, "{name}: Mausrad zoomt das Blatt");
        h.drag(&mut st, p, vec2(-40.0, 30.0), PointerButton::Middle);
        assert_eq!(
            st.drawing_ui.pan, pan,
            "{name}: Ziehen verschiebt das Blatt"
        );
        h.click(&mut st, p);
        assert_eq!(
            st.drawing_ui.selected_view,
            Some(front),
            "{name}: Klick erreicht das Blatt"
        );
    }
    // Gegenprobe auf dem Blatt in der freien Fläche (neben der Ansicht).
    let free = h.layout().canvas;
    let p = free.left_top() + vec2(40.0, 40.0);
    h.wheel_at(&mut st, p);
    assert_ne!(st.drawing_ui.zoom, 1.0, "Mausrad zoomt auf dem Blatt");
    let pan = st.drawing_ui.pan;
    h.drag(&mut st, p, vec2(-40.0, 30.0), PointerButton::Middle);
    assert_ne!(st.drawing_ui.pan, pan, "Ziehen verschiebt auf dem Blatt");
    h.click(&mut st, p);
    assert_eq!(
        st.drawing_ui.selected_view, None,
        "Klick ins Leere wählt ab"
    );
    // Außerhalb der freien Fläche, aber nicht unter einem Feld, erreicht man das Blatt auch
    // (es ist über das ganze Fenster gemalt).
    let outside = pos2(6.0, 400.0);
    let zoom = st.drawing_ui.zoom;
    h.wheel_at(&mut st, outside);
    assert_ne!(st.drawing_ui.zoom, zoom, "Mausrad am Fensterrand zoomt");
}

// (h) ------------------------------------------------------------------------------------------

#[test]
fn drawing_tests_without_shell_still_work() {
    for (size, editable) in [(HARNESS, true), (BIG, true), (HARNESS, false)] {
        let (mut st, _, front) = drawing_state();
        st.drawing_ui.selected_view = Some(front);
        let mut h = Bare::new(size, editable);
        h.settle(&mut st);
        let r = bare_rects(size, !editable);
        let mut n = names(&h.c);
        n.sort_unstable();
        assert_eq!(
            n,
            ["Ansichtsfeld", "Hinweisfeld", "Werkzeugfeld"],
            "{size:?}"
        );
        assert_eq!(field(&h.c, "Werkzeugfeld"), r.tool.unwrap());
        assert_eq!(field(&h.c, "Hinweisfeld"), r.bottom);
        let side = field(&h.c, "Ansichtsfeld");
        assert_eq!(side.min, r.right.min);
        assert!(r.right.expand(0.5).contains_rect(side));
        let l = h.layout();
        assert_eq!(l.paint, Rect::from_min_size(Pos2::ZERO, size));
        assert_eq!(l.canvas, r.free);
        assert_eq!(l.toolbar, r.tool.unwrap());
        assert_eq!(l.hint, r.bottom);
        // `show` (bearbeitbar) zeichnet dieselben Felder.
        if editable {
            let c = context();
            let out = c.run_ui(input(size, vec![]), |ui| drawing_view::show(ui, &mut st));
            out.drop_without_applying_deltas();
            let out = c.run_ui(input(size, vec![]), |ui| drawing_view::show(ui, &mut st));
            out.drop_without_applying_deltas();
            assert_eq!(shell_panels::field_rects(&c).len(), 3);
        }
    }
}

// (i) ------------------------------------------------------------------------------------------

#[test]
fn read_only_locks_like_before() {
    let (mut st, d, front) = drawing_state();
    st.drawing_ui.selected_view = Some(front);
    let mut h = Bare::new(HARNESS, false);
    let before = view(&st, d, front).unwrap();
    // Ansicht hinzufügen: das Menü öffnet sich nicht.
    let shot = h.click_in(&mut st, "Werkzeugfeld", "Ansicht hinzufügen");
    assert!(!shot.has("Isometrisch"), "Menü in alter Version offen");
    // Schalter, Maßstab und Löschen ändern nichts.
    h.click_in(&mut st, "Ansichtsfeld", "Verdeckte Kanten zeigen");
    h.click_in(&mut st, "Ansichtsfeld", drawing_view::CENTERLINES);
    h.click_in(&mut st, "Ansichtsfeld", "1:1");
    let shot = h.settle(&mut st);
    assert!(!shot.has("1:2"), "Maßstab-Auswahl in alter Version offen");
    h.click_in(&mut st, "Ansichtsfeld", "Ansicht löschen");
    assert_eq!(view(&st, d, front).unwrap(), before);
    assert_eq!(st.document.drawing(d).unwrap().views.len(), 1);
    // Maße vorschlagen setzt nichts, das Werkzeug „Maß“ bleibt aus und nennt den Grund.
    h.click_in(&mut st, "Werkzeugfeld", drawing_view::SUGGEST);
    assert!(st.document.drawing(d).unwrap().dimensions.is_empty());
    let shot = h.click_in(&mut st, "Werkzeugfeld", TOOL);
    assert!(!st.dimension_tool.is_active());
    let _ = shot;
    let tool = field(&h.c, "Werkzeugfeld");
    let shot = h.settle(&mut st);
    let p = shot.find_in(tool, TOOL).rect.center();
    h.frame(&mut st, vec![moved(p)]);
    h.frame(&mut st, vec![moved(p)]);
    let shot = h.frame(&mut st, vec![moved(p)]);
    assert!(shot.contains(READ_ONLY_TIP), "Grund am gesperrten Werkzeug");
}
