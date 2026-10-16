//! Werkzeug „Maß“ (Teilprojekt 12, Aufgabe 12): Maße von Hand setzen – mit echten
//! egui-Ereignissen wie `tests/drawing_ui.rs` und `tests/dimension_view.rs`.
//!
//! Sollwerte aus den Eingabemaßen (von Hand gerechnet):
//! - Quader 10 × 20 × 30 (x, y, z), Vorderansicht 1:1 mit Mitte (100, 100): Kanten x 95…105,
//!   y 85…115 (Blatt-mm).
//! - Zylinder r 5, h 12 (Achse z): Draufsicht in (100, 100) → Kreis um (100, 100) mit r 5;
//!   Vorderansicht in (100, 100) → Umrisse x 95 und 105, Deckel y 94 und 106.
//! - L-Profil (Skizze in xy: (0,0) (30,0) (30,10) (10,10) (10,30) (0,30), Höhe 5), Draufsicht
//!   in (100, 100), Hüllbox-Mitte (15, 15): Blatt = Modell + 85. Linke Außenkante x 85
//!   (y 85…115), innere senkrechte Kante x 95 (y 95…115), rechte Kante x 115 (y 85…95), innere
//!   waagerechte Kante y 95 (x 95…115), oberste Kante y 115 (x 85…95).

use egui::{Event, Key, Modifiers, PointerButton, Pos2, Rect, vec2};
use planora_app::dimension_state::{SuggestOutcome, full_text, marks_in, resolve_in, suggest_for};
use planora_app::dimension_tool::*;
use planora_app::drawing_view;
use planora_app::fonts;
use planora_app::state::*;
use planora_app::testing::collect_texts;
use planora_drawing::annotate::{Mark, MarkKind, ResolvedDims};
use planora_drawing::dimension::{ChainMode, DimId, DimKind, DimSpec, GeoRef};
use planora_drawing::out2d::Out2d;
use planora_drawing::stroke::StrokeOrigin;
use planora_drawing::{DrawingId, Paper, Placement, Projection, Scale, Sheet, ViewId, ViewKind};
use planora_exchange::ExchangeMesh;
use planora_feature::{
    BlendArt, FeatureId, FeatureKind, SketchFeature, SketchPlane, StandardPlane,
};
use planora_kernel::blend::topo::{edge_uses, face_name};
use planora_kernel::{Edge, Id, Solid};
use planora_math::{DVec2, DVec3, Frame, Tolerance};
use planora_sketch::model::Sketch;
use std::sync::Arc;

const W: f32 = 1200.0;
const H: f32 = 800.0;
const EPS: f64 = 1e-9;

const A4_QUER: Sheet = Sheet {
    paper: Paper::A4,
    landscape: true,
};

const BOX: DVec3 = DVec3::new(10.0, 20.0, 30.0);

fn s11() -> Scale {
    Scale { num: 1, den: 1 }
}

fn context() -> egui::Context {
    let ctx = egui::Context::default();
    ctx.set_fonts(fonts::definitions());
    ctx
}

fn raw(events: Vec<Event>, time: f64) -> egui::RawInput {
    let mut all = vec![Event::ModifiersChanged(Modifiers::NONE)];
    all.extend(events);
    egui::RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(W, H))),
        events: all,
        time: Some(time),
        ..Default::default()
    }
}

/// Ein Bild der Oberfläche: alle Texte und die Lage der Texte.
struct Shot {
    text: String,
    texts: Vec<(String, Rect)>,
}

impl Shot {
    fn pos_of(&self, exact: &str) -> Option<Pos2> {
        self.texts
            .iter()
            .find(|(t, _)| t == exact)
            .map(|(_, r)| r.left_center() + vec2(6.0, 0.0))
    }
}

fn walk(shape: &egui::epaint::Shape, texts: &mut Vec<(String, Rect)>) {
    match shape {
        egui::epaint::Shape::Vec(v) => v.iter().for_each(|s| walk(s, texts)),
        egui::epaint::Shape::Text(t) => texts.push((
            t.galley.text().to_string(),
            Rect::from_min_size(t.pos, t.galley.size()),
        )),
        _ => {}
    }
}

#[derive(Clone, Copy, PartialEq)]
enum What {
    Area,
    ReadOnly,
}

struct Harness {
    ctx: egui::Context,
    time: f64,
    what: What,
}

impl Harness {
    fn new() -> Harness {
        Harness {
            ctx: context(),
            time: 0.0,
            what: What::Area,
        }
    }

    fn read_only() -> Harness {
        Harness {
            what: What::ReadOnly,
            ..Harness::new()
        }
    }

    fn frame(&mut self, st: &mut AppState, events: Vec<Event>) -> Shot {
        // Mehr als die Doppelklick-Zeit zwischen den Bildern: jeder Klick zählt einzeln.
        self.frame_dt(st, events, 0.2)
    }

    /// Ein Bild `dt` Sekunden nach dem vorigen.
    fn frame_dt(&mut self, st: &mut AppState, events: Vec<Event>, dt: f64) -> Shot {
        self.time += dt;
        let what = self.what;
        let out = self.ctx.run_ui(raw(events, self.time), |ui| match what {
            What::Area => drawing_view::show(ui, st),
            What::ReadOnly => drawing_view::show_with(ui, st, false),
        });
        let mut texts = Vec::new();
        for c in &out.shapes {
            walk(&c.shape, &mut texts);
        }
        let shot = Shot {
            text: collect_texts(&out),
            texts,
        };
        out.drop_without_applying_deltas();
        shot
    }

    fn idle(&mut self, st: &mut AppState) -> Shot {
        self.frame(st, vec![])
    }

    fn layout(&self) -> drawing_view::DrawingLayout {
        drawing_view::last_layout(&self.ctx).expect("kein Zeichnungsbereich gezeichnet")
    }

    fn k(&self) -> f64 {
        f64::from(self.layout().px_per_mm)
    }

    fn click(&mut self, st: &mut AppState, p: Pos2) -> Shot {
        self.frame(st, vec![Event::PointerMoved(p)]);
        self.frame(st, vec![button(p, true)]);
        self.frame(st, vec![button(p, false)]);
        self.idle(st)
    }

    fn click_text(&mut self, st: &mut AppState, exact: &str) -> Shot {
        let shot = self.idle(st);
        let p = shot
            .pos_of(exact)
            .unwrap_or_else(|| panic!("„{exact}“ nicht sichtbar:\n{}", shot.text));
        self.click(st, p)
    }

    fn key(&mut self, st: &mut AppState, key: Key) -> Shot {
        self.frame(
            st,
            vec![Event::Key {
                key,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::NONE,
            }],
        );
        self.frame(
            st,
            vec![Event::Key {
                key,
                physical_key: None,
                pressed: false,
                repeat: false,
                modifiers: Modifiers::NONE,
            }],
        );
        self.idle(st)
    }

    /// Klick auf den Blattpunkt `p` (mm); liefert den genauen Blattpunkt des Klicks (Pixel
    /// gerundet wie auf dem Bildschirm).
    fn click_at(&mut self, st: &mut AppState, p: (f64, f64)) -> DVec2 {
        let pos = screen(self, p);
        self.click(st, pos);
        sheet_of(self, pos)
    }

    /// Doppelklick auf den Blattpunkt `p` (mm): zwei Klicks im Abstand von 0,02 s.
    fn double_click_at(&mut self, st: &mut AppState, p: (f64, f64)) -> DVec2 {
        let pos = screen(self, p);
        // Lange genug nach dem letzten Klick, damit egui keinen Dreifachklick zählt.
        self.frame_dt(st, vec![Event::PointerMoved(pos)], 1.0);
        for pressed in [true, false, true, false] {
            self.frame_dt(st, vec![button(pos, pressed)], 0.02);
        }
        self.idle(st);
        sheet_of(self, pos)
    }

    /// Zeiger auf den Blattpunkt `p` (mm) bewegen.
    fn hover_at(&mut self, st: &mut AppState, p: (f64, f64)) -> (Shot, DVec2) {
        let pos = screen(self, p);
        self.frame(st, vec![Event::PointerMoved(pos)]);
        (self.idle(st), sheet_of(self, pos))
    }
}

fn button(pos: Pos2, pressed: bool) -> Event {
    Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    }
}

fn screen(h: &Harness, p: (f64, f64)) -> Pos2 {
    let l = h.layout();
    drawing_view::sheet_to_screen(l.canvas, l.sheet_mm, l.zoom, l.pan, DVec2::new(p.0, p.1))
}

fn sheet_of(h: &Harness, pos: Pos2) -> DVec2 {
    let l = h.layout();
    drawing_view::screen_to_sheet(l.canvas, l.sheet_mm, l.zoom, l.pan, pos)
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() <= EPS
}

fn new_drawing(st: &mut AppState) -> DrawingId {
    st.document
        .add_drawing("Blatt 1", A4_QUER, Projection::First)
        .unwrap()
}

fn add_view_scaled(
    st: &mut AppState,
    d: DrawingId,
    kind: ViewKind,
    x: f64,
    y: f64,
    scale: Scale,
) -> ViewId {
    st.document
        .add_view(d, kind, scale, Placement::Free { x, y }, None)
        .unwrap()
}

fn add_view(st: &mut AppState, d: DrawingId, kind: ViewKind, x: f64, y: f64) -> ViewId {
    add_view_scaled(st, d, kind, x, y, s11())
}

fn open(st: &mut AppState, d: DrawingId) -> Harness {
    st.drawing_ui.open(d);
    st.drawing_ui.tick(&st.bodies, &st.document, true);
    let mut h = Harness::new();
    h.idle(st);
    h.idle(st);
    h
}

fn dims(st: &AppState, d: DrawingId) -> Vec<DimSpec> {
    st.document.drawing(d).unwrap().dimensions.clone()
}

/// Gemessener Wert eines Maßes (aufgelöst wie in der Oberfläche).
fn value(st: &mut AppState, d: DrawingId, id: DimId) -> f64 {
    let r = resolve_in(st, d);
    measured(&r, id)
}

fn measured(r: &ResolvedDims, id: DimId) -> f64 {
    r.by_view
        .values()
        .flatten()
        .find(|(s, _)| s.id == id)
        .map(|(_, m)| m.as_ref().expect("Maß gemessen").value)
        .expect("Maß vorhanden")
}

/// Quader 10 × 20 × 30, Vorderansicht in (100, 100), geöffnet und berechnet.
fn box_front() -> (AppState, FeatureId, DrawingId, ViewId, Harness) {
    let mut st = AppState::new();
    let f = st.create_box(BOX).unwrap();
    let d = new_drawing(&mut st);
    let front = add_view(&mut st, d, ViewKind::Front, 100.0, 100.0);
    let h = open(&mut st, d);
    (st, f, d, front, h)
}

/// Zylinder r 5, h 12 mit einer Ansicht `kind` in (100, 100).
fn cylinder(kind: ViewKind) -> (AppState, DrawingId, ViewId, Harness) {
    let mut st = AppState::new();
    st.create_cylinder(5.0, 12.0).unwrap();
    let d = new_drawing(&mut st);
    let v = add_view(&mut st, d, kind, 100.0, 100.0);
    let h = open(&mut st, d);
    (st, d, v, h)
}

/// Extrudierte Skizze in der xy-Ebene (Höhe 5).
fn extruded(st: &mut AppState, build: impl FnOnce(&mut Sketch)) {
    let mut sk = Sketch::new(Frame::WORLD, "Skizze", Tolerance::default());
    build(&mut sk);
    let s = st
        .add_feature(
            Some("Skizze"),
            FeatureKind::Sketch(SketchFeature {
                sketch: sk,
                plane: SketchPlane::Standard(StandardPlane::XY),
                copies: vec![],
            }),
        )
        .unwrap();
    st.add_feature(
        None,
        FeatureKind::Extrude {
            sketch: s,
            profile: 0,
            height: 5.0,
        },
    )
    .unwrap();
    assert!(st.message.is_none(), "{:?}", st.message);
    assert_eq!(st.bodies.len(), 1);
}

/// L-Profil (siehe Modulkommentar), Draufsicht in (100, 100).
fn l_profile() -> (AppState, DrawingId, ViewId, Harness) {
    let mut st = AppState::new();
    extruded(&mut st, |sk| {
        let p: Vec<_> = [
            (0.0, 0.0),
            (30.0, 0.0),
            (30.0, 10.0),
            (10.0, 10.0),
            (10.0, 30.0),
            (0.0, 30.0),
        ]
        .iter()
        .map(|&(x, y)| sk.add_point(x, y).unwrap())
        .collect();
        for i in 0..p.len() {
            sk.add_line(p[i], p[(i + 1) % p.len()]).unwrap();
        }
    });
    let d = new_drawing(&mut st);
    let top = add_view(&mut st, d, ViewKind::Top, 100.0, 100.0);
    let h = open(&mut st, d);
    (st, d, top, h)
}

/// Linien eines Maßes im zuletzt gezeichneten Blatt: gibt es die Strecke `a`–`b`?
fn has_line(marks: &[Mark], a: (f64, f64), b: (f64, f64)) -> bool {
    let (a, b) = (DVec2::new(a.0, a.1), DVec2::new(b.0, b.1));
    marks.iter().any(|m| match m {
        Mark::Line { a: p, b: q, kind } => {
            *kind == MarkKind::Dimension
                && ((p.distance(a) < 1e-9 && q.distance(b) < 1e-9)
                    || (p.distance(b) < 1e-9 && q.distance(a) < 1e-9))
        }
        _ => false,
    })
}

fn sheet_marks(h: &Harness, view: ViewId) -> Vec<Mark> {
    drawing_view::last_sheet(&h.ctx)
        .unwrap()
        .views
        .iter()
        .find(|v| v.id == view)
        .unwrap()
        .marks
        .clone()
}

/// Englische Fachwörter und Hinweise auf die Herkunft der Daten, die in Meldungen nicht
/// vorkommen dürfen.
const FORBIDDEN: [&str; 16] = [
    "edge",
    "face",
    "body",
    "ref",
    "geo",
    "origin",
    "error",
    "invalid",
    "lost",
    "index",
    "id",
    "körper 0",
    "kante 0",
    "fläche „",
    "georef",
    "silhouette",
];

fn assert_plain(text: &str) {
    let low = text.to_lowercase();
    let words: Vec<&str> = low.split(|c: char| !c.is_alphanumeric()).collect();
    for w in FORBIDDEN {
        let hit = if w.chars().all(char::is_alphanumeric) {
            words.contains(&w)
        } else {
            low.contains(w)
        };
        assert!(!hit, "„{w}“ in „{text}“");
    }
    assert!(text.ends_with('.'), "ganzer Satz: „{text}“");
}

/// Punkt auf dem Kreis r 5 um (100, 100) bei 37° (abseits möglicher Nahtstellen).
fn on_circle() -> (f64, f64) {
    let a = 37f64.to_radians();
    (100.0 + 5.0 * a.cos(), 100.0 + 5.0 * a.sin())
}

// ---------------------------------------------------------------------------------------------
// (a)

#[test]
fn horizontal_dimension_by_two_edge_clicks_and_placing() {
    let (mut st, _, d, front, mut h) = box_front();
    // Schalter „Maß“ in der Werkzeugleiste: Vorgabe waagerecht.
    h.click_text(&mut st, TOOL);
    assert_eq!(
        st.dimension_tool.mode,
        ToolMode::Linear(LinearDir::Horizontal)
    );
    let e0 = st.document.edit_count();
    let m0 = st.document.model_edit_count();
    // Linke und rechte senkrechte Kante (Mitte, weit weg von den Enden).
    h.click_at(&mut st, (95.0, 100.0));
    assert_eq!(st.dimension_tool.pending().len(), 1);
    assert_eq!(st.dimension_tool.view(), Some(front));
    h.click_at(&mut st, (105.0, 100.0));
    assert!(st.dimension_tool.is_ready());
    assert!(
        st.dimension_tool
            .pending()
            .iter()
            .all(|r| matches!(r, GeoRef::Edge(_))),
        "{:?}",
        st.dimension_tool.pending()
    );
    assert_eq!(st.document.edit_count(), e0, "Wählen ändert nichts");
    // Vorschau am Zeiger.
    let (_, _) = h.hover_at(&mut st, (100.0, 130.0));
    assert!(
        h.layout().preview_drawn >= 5,
        "{}",
        h.layout().preview_drawn
    );
    // Zweiter Klick setzt das Maß: Abstand = Zeigerlage − Oberkante (115).
    let p = h.click_at(&mut st, (100.0, 130.0));
    let all = dims(&st, d);
    assert_eq!(all.len(), 1);
    let s = &all[0];
    assert_eq!((s.kind, s.view), (DimKind::Horizontal, front));
    assert!(
        near(s.offset, p.y - 115.0),
        "{} vs {}",
        s.offset,
        p.y - 115.0
    );
    assert!((s.offset - 15.0).abs() < 0.5, "{}", s.offset);
    assert!(!s.suggested && s.chain.is_none());
    assert!(near(value(&mut st, d, s.id), BOX.x));
    assert_eq!(st.document.edit_count(), e0 + 1);
    assert_eq!(st.document.model_edit_count(), m0);
    // Danach ist nichts mehr gewählt, das Werkzeug bleibt an; die Maßlinie liegt bei der
    // Zeigerlage.
    assert!(st.dimension_tool.pending().is_empty());
    assert!(st.dimension_tool.is_active());
    let shot = h.idle(&mut st);
    assert!(shot.text.contains("1 Maß wurde gesetzt."), "{}", shot.text);
    assert!(has_line(&sheet_marks(&h, front), (95.0, p.y), (105.0, p.y)));
}

// ---------------------------------------------------------------------------------------------
// (b)

#[test]
fn edge_end_hit_when_clicking_near_the_end() {
    let (mut st, _, d, _, mut h) = box_front();
    st.dimension_tool
        .set_mode(ToolMode::Linear(LinearDir::Horizontal));
    let k = h.k();
    // 7,5 px über der Ecke (95, 85) auf der linken Kante: Kantenende.
    h.click_at(&mut st, (95.0, 85.0 + 7.5 / k));
    assert!(
        matches!(st.dimension_tool.pending()[0], GeoRef::EdgeEnd { .. }),
        "{:?}",
        st.dimension_tool.pending()
    );
    // 8,5 px über der Ecke (105, 85) auf der rechten Kante: ganze Kante.
    h.click_at(&mut st, (105.0, 85.0 + 8.5 / k));
    assert!(
        matches!(st.dimension_tool.pending()[1], GeoRef::Edge(_)),
        "{:?}",
        st.dimension_tool.pending()
    );
    h.click_at(&mut st, (100.0, 70.0));
    let all = dims(&st, d);
    assert_eq!(all.len(), 1);
    assert!(matches!(all[0].refs[0], GeoRef::EdgeEnd { .. }));
    assert!(matches!(all[0].refs[1], GeoRef::Edge(_)));
    assert!(near(value(&mut st, d, all[0].id), BOX.x));
    // Unterhalb: negativer Abstand bis zur Unterkante (85).
    assert!(all[0].offset < 0.0);
}

#[test]
fn click_near_a_circle_centre_gives_the_centre() {
    let (mut st, _, _, mut h) = cylinder(ViewKind::Top);
    st.dimension_tool
        .set_mode(ToolMode::Linear(LinearDir::Horizontal));
    let k = h.k();
    // 3 px neben der Mitte (der Kreis selbst ist 5 mm = 5·k px entfernt).
    h.click_at(&mut st, (100.0 + 3.0 / k, 100.0));
    assert!(
        matches!(st.dimension_tool.pending(), [GeoRef::Center(_)]),
        "{:?}",
        st.dimension_tool.pending()
    );
    // Auf dem Kreis: die Kante.
    st.dimension_tool.cancel();
    h.click_at(&mut st, on_circle());
    assert!(
        matches!(st.dimension_tool.pending(), [GeoRef::Edge(_)]),
        "{:?}",
        st.dimension_tool.pending()
    );
}

// ---------------------------------------------------------------------------------------------
// (c)

#[test]
fn vertical_and_aligned_modes() {
    let (mut st, _, d, front, mut h) = box_front();
    st.dimension_tool
        .set_mode(ToolMode::Linear(LinearDir::Vertical));
    h.click_at(&mut st, (100.0, 85.0));
    h.click_at(&mut st, (100.0, 115.0));
    let p = h.click_at(&mut st, (120.0, 100.0));
    let v = dims(&st, d)[0].clone();
    assert_eq!((v.kind, v.view), (DimKind::Vertical, front));
    // Senkrecht: positiv nach rechts, ab der rechten Kante (105).
    assert!(near(v.offset, p.x - 105.0), "{}", v.offset);
    assert!(near(value(&mut st, d, v.id), BOX.z));

    // Schräg zwischen den Ecken (95, 85) und (105, 115).
    st.dimension_tool
        .set_mode(ToolMode::Linear(LinearDir::Aligned));
    h.click_at(&mut st, (95.0, 85.0));
    h.click_at(&mut st, (105.0, 115.0));
    assert!(
        st.dimension_tool
            .pending()
            .iter()
            .all(|r| matches!(r, GeoRef::EdgeEnd { .. })),
        "{:?}",
        st.dimension_tool.pending()
    );
    let p = h.click_at(&mut st, (90.0, 110.0));
    let a = dims(&st, d)[1].clone();
    assert_eq!(a.kind, DimKind::Aligned);
    let want = (BOX.x * BOX.x + BOX.z * BOX.z).sqrt();
    assert!(near(value(&mut st, d, a.id), want));
    // Querachse: Messrichtung u = (10, 30)/|…| um 90° gedreht, nach rechts: w = (u.y, −u.x).
    let u = DVec2::new(10.0, 30.0) / want;
    let w = DVec2::new(u.y, -u.x);
    let want_offset = (p - DVec2::new(95.0, 85.0)).dot(w);
    assert!(want_offset < -1.0);
    assert!(near(a.offset, want_offset), "{} vs {want_offset}", a.offset);
}

// ---------------------------------------------------------------------------------------------
// (d)

#[test]
fn diameter_by_clicking_a_circle_in_top_view() {
    let (mut st, d, top, mut h) = cylinder(ViewKind::Top);
    st.dimension_tool.set_mode(ToolMode::Diameter);
    h.click_at(&mut st, on_circle());
    assert!(st.dimension_tool.is_ready());
    let r = 15.0 / 2f64.sqrt();
    let p = h.click_at(&mut st, (100.0 + r, 100.0 + r));
    let s = dims(&st, d)[0].clone();
    assert_eq!((s.kind, s.view), (DimKind::Diameter, top));
    assert!(matches!(s.refs[0], GeoRef::Edge(_)));
    assert!(near(value(&mut st, d, s.id), 10.0));
    let v = p - DVec2::new(100.0, 100.0);
    assert!(near(s.angle, v.y.atan2(v.x).to_degrees()), "{}", s.angle);
    assert!((s.angle - 45.0).abs() < 0.5, "{}", s.angle);
    assert!(near(s.offset, v.length() - 5.0), "{}", s.offset);
    let shot = h.idle(&mut st);
    assert!(shot.text.contains("Ø10,0"), "{}", shot.text);
}

// ---------------------------------------------------------------------------------------------
// (e)

#[test]
fn diameter_by_clicking_the_cylinder_silhouette_in_front_view() {
    let (mut st, d, front, mut h) = cylinder(ViewKind::Front);
    st.dimension_tool.set_mode(ToolMode::Diameter);
    h.click_at(&mut st, (95.0, 100.0));
    assert!(
        matches!(st.dimension_tool.pending(), [GeoRef::Face(_)]),
        "{:?} {:?}",
        st.dimension_tool.pending(),
        st.dimension_tool.message()
    );
    let p = h.click_at(&mut st, (100.0, 120.0));
    let s = dims(&st, d)[0].clone();
    assert_eq!((s.kind, s.view), (DimKind::Diameter, front));
    assert!(near(value(&mut st, d, s.id), 10.0));
    // Linear quer über den Zylinder: Abstand über dem Deckel (106).
    assert!(near(s.offset, p.y - 106.0), "{}", s.offset);
    let shot = h.idle(&mut st);
    assert!(shot.text.contains("Ø10,0"), "{}", shot.text);
}

// ---------------------------------------------------------------------------------------------
// (f)

fn edge_between(s: &Solid, a: &str, b: &str) -> Id<Edge> {
    s.edges
        .iter()
        .map(|(id, _)| id)
        .find(|&id| {
            let n: Vec<String> = edge_uses(s, id)
                .iter()
                .map(|u| face_name(s, u.face))
                .collect();
            n.len() == 2 && ((n[0] == a && n[1] == b) || (n[0] == b && n[1] == a))
        })
        .unwrap()
}

#[test]
fn radius_of_a_fillet_arc() {
    let mut st = AppState::new();
    st.create_box(DVec3::splat(10.0)).unwrap();
    let e = edge_between(&st.bodies[0].solid, "Deckel", "Wand 1");
    st.add_blend(0, &[e], BlendArt::Rundung { radius: 2.0 }, true)
        .unwrap();
    let d = new_drawing(&mut st);
    let two = Scale { num: 2, den: 1 };
    add_view_scaled(&mut st, d, ViewKind::Front, 70.0, 100.0, two);
    add_view_scaled(&mut st, d, ViewKind::Right, 160.0, 100.0, two);
    let mut h = open(&mut st, d);
    // Der Bogen der Rundung: r 2 mm · 2 = 4 mm auf dem Blatt, kein Vollkreis.
    let sheet = drawing_view::last_sheet(&h.ctx).unwrap();
    let (view, c, mid) = sheet
        .views
        .iter()
        .find_map(|v| {
            v.lines.iter().find_map(|l| match l.curve {
                Out2d::Arc { c, r, start, end } if (r - 4.0).abs() < 1e-9 && end - start < 6.0 => {
                    Some((v.id, c, 0.5 * (start + end)))
                }
                _ => None,
            })
        })
        .expect("Bogen der Rundung");
    st.dimension_tool.set_mode(ToolMode::Radius);
    let u = DVec2::new(mid.cos(), mid.sin());
    let on = c + u * 4.0;
    h.click_at(&mut st, (on.x, on.y));
    assert!(
        st.dimension_tool.is_ready(),
        "{:?}",
        st.dimension_tool.message()
    );
    let out = c + u * 14.0;
    let p = h.click_at(&mut st, (out.x, out.y));
    let s = dims(&st, d)[0].clone();
    assert_eq!((s.kind, s.view), (DimKind::Radius, view));
    assert!(near(value(&mut st, d, s.id), 2.0));
    assert!(near(s.offset, (p - c).length() - 4.0), "{}", s.offset);
    let shot = h.idle(&mut st);
    assert!(shot.text.contains("R2,0"), "{}", shot.text);
}

/// Bogen der Rundung r 2 an der Kante Deckel/Wand 1 im Maßstab 2:1 (r 4 auf dem Blatt):
/// Zustand, Zeichnung, Ansicht, Mitte, Anfang und Ende des Bogens (Radiant, gegen den
/// Uhrzeigersinn) und das Fenster.
fn fillet_arc_drawing() -> (AppState, DrawingId, ViewId, DVec2, f64, f64, Harness) {
    let mut st = AppState::new();
    st.create_box(DVec3::splat(10.0)).unwrap();
    let e = edge_between(&st.bodies[0].solid, "Deckel", "Wand 1");
    st.add_blend(0, &[e], BlendArt::Rundung { radius: 2.0 }, true)
        .unwrap();
    let d = new_drawing(&mut st);
    let two = Scale { num: 2, den: 1 };
    add_view_scaled(&mut st, d, ViewKind::Front, 70.0, 100.0, two);
    add_view_scaled(&mut st, d, ViewKind::Right, 160.0, 100.0, two);
    let h = open(&mut st, d);
    let sheet = drawing_view::last_sheet(&h.ctx).unwrap();
    let (view, c, start, end) = sheet
        .views
        .iter()
        .find_map(|v| {
            v.lines.iter().find_map(|l| match l.curve {
                Out2d::Arc { c, r, start, end } if (r - 4.0).abs() < 1e-9 && end - start < 6.0 => {
                    Some((v.id, c, start, end))
                }
                _ => None,
            })
        })
        .expect("Bogen der Rundung");
    // Ein Viertelkreis.
    assert!((end - start - std::f64::consts::FRAC_PI_2).abs() < 1e-9);
    (st, d, view, c, start, end, h)
}

/// Die Pfeilspitze der Führungslinie in den Markierungen.
fn leader_tip(marks: &[Mark]) -> DVec2 {
    let tips: Vec<DVec2> = marks
        .iter()
        .filter_map(|m| match m {
            Mark::Arrow { tip, .. } => Some(*tip),
            _ => None,
        })
        .collect();
    assert_eq!(tips.len(), 1, "{marks:?}");
    tips[0]
}

#[test]
fn radius_placed_beside_the_arc_keeps_its_tip_on_the_arc() {
    // Der Zeiger steht 120° hinter der Bogenmitte, also 75° hinter dem Bogenende: Der Winkel
    // wird auf das nähere Ende (Mitte + 45°) geklemmt, die Spitze sitzt dort auf dem Bogen.
    let (mut st, d, view, c, start, end, mut h) = fillet_arc_drawing();
    let mid = 0.5 * (start + end);
    let dir = |t: f64| DVec2::new(t.cos(), t.sin());
    st.dimension_tool.set_mode(ToolMode::Radius);
    let on = c + dir(mid) * 4.0;
    h.click_at(&mut st, (on.x, on.y));
    assert!(st.dimension_tool.is_ready());
    let p = c + dir(mid + 120f64.to_radians()) * 14.0;
    let preview = with_tool(&mut st, |t, s| t.hover(s, d, p)).expect("Vorschau");
    assert!(
        leader_tip(&preview).distance(c + dir(end) * 4.0) < 1e-6,
        "Vorschau: {preview:?}"
    );
    // Vorschau = Ergebnis.
    preview_equals_final(&mut st, d, p);
    let s = dims(&st, d)[0].clone();
    assert_eq!((s.kind, s.view), (DimKind::Radius, view));
    let want = end.to_degrees();
    let diff = (s.angle - want).rem_euclid(360.0);
    assert!(diff < 1e-6 || diff > 360.0 - 1e-6, "{} vs {want}", s.angle);
    // Abstand: Zeigerabstand − r, wie bisher.
    assert!(near(s.offset, 10.0), "{}", s.offset);
    let dims_now = resolve_in(&mut st, d);
    let all = marks_in(&mut st, d, &dims_now);
    let tip = leader_tip(&all.iter().find(|dm| dm.id == s.id).unwrap().marks);
    assert!(tip.distance(c + dir(end) * 4.0) < 1e-6, "{tip:?}");
    h.idle(&mut st);
}

// ---------------------------------------------------------------------------------------------
// (g)

#[test]
fn angle_between_two_lines_of_an_l_profile() {
    let (mut st, d, top, mut h) = l_profile();
    st.dimension_tool.set_mode(ToolMode::Angle);
    // Innere waagerechte und innere senkrechte Kante, Scheitel (95, 95).
    h.click_at(&mut st, (105.0, 95.0));
    assert_eq!(st.dimension_tool.pending().len(), 1);
    h.click_at(&mut st, (95.0, 105.0));
    assert!(st.dimension_tool.is_ready());
    let p = h.click_at(&mut st, (108.0, 108.0));
    let s = dims(&st, d)[0].clone();
    assert_eq!((s.kind, s.view), (DimKind::Angle, top));
    assert!(near(value(&mut st, d, s.id), 90.0));
    assert!(near(s.offset, (p - DVec2::new(95.0, 95.0)).length()));
    let shot = h.idle(&mut st);
    assert!(shot.text.contains("90,0°"), "{}", shot.text);
}

// ---------------------------------------------------------------------------------------------
// (h)

#[test]
fn chain_by_three_clicks_makes_two_dimensions_with_one_line() {
    let (mut st, d, top, mut h) = l_profile();
    st.dimension_tool.set_mode(ToolMode::Chain);
    let e0 = st.document.edit_count();
    h.click_at(&mut st, (85.0, 100.0));
    h.click_at(&mut st, (95.0, 105.0));
    h.click_at(&mut st, (115.0, 90.0));
    assert_eq!(st.dimension_tool.pending().len(), 3);
    assert_eq!(st.document.edit_count(), e0);
    // Zeiger über das Teil, Esc setzt die Kette.
    let (_, p) = h.hover_at(&mut st, (100.0, 125.0));
    h.key(&mut st, Key::Escape);
    let all = dims(&st, d);
    assert_eq!(all.len(), 2, "{all:?}");
    assert_eq!(st.document.edit_count(), e0 + 2);
    let off = p.y - 115.0;
    let group = all[0].chain.unwrap().group;
    for (i, (s, want)) in all.iter().zip([10.0, 20.0]).enumerate() {
        assert_eq!(s.kind, DimKind::Horizontal);
        assert!(near(s.offset, off), "{} vs {off}", s.offset);
        let c = s.chain.unwrap();
        assert_eq!(
            (c.group, c.mode, c.order),
            (group, ChainMode::Chain, i as u32)
        );
        assert!(near(value(&mut st, d, s.id), want));
    }
    assert!(group >= 1);
    // Eine gemeinsame Maßlinie: 85…95 und 95…115 auf derselben Höhe.
    h.idle(&mut st);
    let marks = sheet_marks(&h, top);
    let y = 115.0 + off;
    assert!(has_line(&marks, (85.0, y), (95.0, y)), "{marks:?}");
    assert!(has_line(&marks, (95.0, y), (115.0, y)), "{marks:?}");
    // Eine weitere Kette bekommt eine neue Gruppe.
    h.click_at(&mut st, (85.0, 100.0));
    h.click_at(&mut st, (95.0, 105.0));
    h.click_at(&mut st, (100.0, 70.0));
    let all = dims(&st, d);
    assert_eq!(all.len(), 3);
    assert_eq!(all[2].chain.unwrap().group, group + 1);
    assert!(all[2].offset < 0.0);
}

// ---------------------------------------------------------------------------------------------
// (i)

#[test]
fn baseline_stacks_dimension_lines_by_7_mm() {
    let (mut st, d, top, mut h) = l_profile();
    st.dimension_tool.set_mode(ToolMode::Baseline);
    h.click_at(&mut st, (85.0, 100.0));
    h.click_at(&mut st, (95.0, 105.0));
    h.click_at(&mut st, (115.0, 90.0));
    // Klick ins Leere setzt.
    let p = h.click_at(&mut st, (100.0, 125.0));
    let all = dims(&st, d);
    assert_eq!(all.len(), 2);
    let off = p.y - 115.0;
    for (i, (s, want)) in all.iter().zip([10.0, 30.0]).enumerate() {
        assert!(near(s.offset, off));
        let c = s.chain.unwrap();
        assert_eq!((c.mode, c.order), (ChainMode::Baseline, i as u32));
        assert!(near(value(&mut st, d, s.id), want));
    }
    assert_eq!(all[0].chain.unwrap().group, all[1].chain.unwrap().group);
    h.idle(&mut st);
    let marks = sheet_marks(&h, top);
    let y0 = 115.0 + off;
    assert!(has_line(&marks, (85.0, y0), (95.0, y0)), "{marks:?}");
    assert!(
        has_line(&marks, (85.0, y0 + 7.0), (115.0, y0 + 7.0)),
        "{marks:?}"
    );
}

// ---------------------------------------------------------------------------------------------
// (j)

fn cube_mesh() -> ExchangeMesh {
    ExchangeMesh {
        positions: vec![
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
            [1.0, 0.0, 1.0],
            [1.0, 1.0, 1.0],
            [0.0, 1.0, 1.0],
        ],
        indices: vec![
            0, 2, 1, 0, 3, 2, 4, 5, 6, 4, 6, 7, 0, 1, 5, 0, 5, 4, 3, 7, 6, 3, 6, 2, 0, 4, 7, 0, 7,
            3, 1, 2, 6, 1, 6, 5,
        ],
    }
}

#[test]
fn rejections_are_german_and_leave_the_document_unchanged() {
    // Kante nicht quer: waagerechtes Maß an linker Kante und Unterkante.
    let (mut st, _, d, _, mut h) = box_front();
    st.dimension_tool
        .set_mode(ToolMode::Linear(LinearDir::Horizontal));
    let e0 = st.document.edit_count();
    h.click_at(&mut st, (95.0, 100.0));
    h.click_at(&mut st, (100.0, 85.0));
    assert_eq!(
        st.dimension_tool.message(),
        Some("Die Kante steht nicht quer zum Maß.")
    );
    assert_eq!(st.dimension_tool.pending().len(), 1, "erster Bezug bleibt");
    let shot = h.idle(&mut st);
    assert!(
        shot.text.contains("Die Kante steht nicht quer zum Maß."),
        "{}",
        shot.text
    );
    // Ein Klick neben die Linien setzt nichts.
    h.click_at(&mut st, (60.0, 60.0));
    assert_eq!(st.dimension_tool.message(), Some(MISSED));
    assert_eq!(st.document.edit_count(), e0);
    assert!(dims(&st, d).is_empty());
    let mut texts = vec![
        "Die Kante steht nicht quer zum Maß.".to_string(),
        MISSED.to_string(),
    ];

    // Isometrie: Klick auf eine Linie der isometrischen Ansicht.
    let mut st = AppState::new();
    st.create_box(BOX).unwrap();
    let d = new_drawing(&mut st);
    add_view(&mut st, d, ViewKind::Front, 80.0, 100.0);
    let iso = add_view(&mut st, d, ViewKind::Isometric, 200.0, 100.0);
    let mut h = open(&mut st, d);
    st.dimension_tool
        .set_mode(ToolMode::Linear(LinearDir::Horizontal));
    let sheet = drawing_view::last_sheet(&h.ctx).unwrap();
    let mid = sheet
        .views
        .iter()
        .find(|v| v.id == iso)
        .unwrap()
        .lines
        .iter()
        .find_map(|l| match l.curve {
            Out2d::Line { a, b } if a.distance(b) > 5.0 => Some((a + b) * 0.5),
            _ => None,
        })
        .unwrap();
    let e0 = st.document.edit_count();
    h.click_at(&mut st, (mid.x, mid.y));
    assert_eq!(st.dimension_tool.message(), Some(NO_ISOMETRIC));
    assert!(st.dimension_tool.pending().is_empty());
    assert_eq!(st.document.edit_count(), e0);
    texts.push(NO_ISOMETRIC.into());

    // Nur Netz-Körper: keine Linie lässt sich bemaßen.
    let mut st = AppState::new();
    st.import_mesh("Würfel", Arc::new(cube_mesh()), 10.0)
        .unwrap();
    let d = new_drawing(&mut st);
    let front = add_view(&mut st, d, ViewKind::Front, 100.0, 100.0);
    open(&mut st, d);
    let mut tool = DimensionTool::default();
    tool.set_mode(ToolMode::Linear(LinearDir::Horizontal));
    let e0 = st.document.edit_count();
    let hit = Hit {
        origin: StrokeOrigin::Edge { body: 0, edge: 0 },
        at: DVec2::new(100.0, 100.0),
        px_per_mm: 3.0,
    };
    let out = tool.click(&st, d, front, &hit);
    let ToolOutcome::Rejected(t) = out else {
        panic!("{out:?}")
    };
    assert_eq!(
        t,
        "Alle Körper sind Netze; eine Zeichnung braucht exakte Körper."
    );
    assert_eq!(st.document.edit_count(), e0);
    texts.push(t);

    // Ellipsenkante: lässt sich nicht bemaßen (auch nicht als Durchmesser).
    let mut st = AppState::new();
    extruded(&mut st, |sk| {
        let c = sk.add_point(0.0, 0.0).unwrap();
        sk.add_ellipse(c, DVec2::new(5.0, 0.0), 3.0).unwrap();
    });
    let d = new_drawing(&mut st);
    add_view(&mut st, d, ViewKind::Top, 100.0, 100.0);
    let mut h = open(&mut st, d);
    let e0 = st.document.edit_count();
    for mode in [ToolMode::Linear(LinearDir::Horizontal), ToolMode::Diameter] {
        st.dimension_tool.set_mode(mode);
        h.click_at(
            &mut st,
            (100.0 + 5.0 * 1f64.cos(), 100.0 + 3.0 * 1f64.sin()),
        );
        assert_eq!(
            st.dimension_tool.message(),
            Some(NOT_DIMENSIONABLE),
            "{mode:?}"
        );
        assert!(st.dimension_tool.pending().is_empty());
    }
    assert_eq!(st.document.edit_count(), e0);
    texts.push(NOT_DIMENSIONABLE.into());

    // Auch Hinweise und Erklärungen des Werkzeugs.
    texts.extend(
        [
            TOOL_TIP,
            READ_ONLY_TIP,
            ISOMETRIC_TIP,
            NO_VIEWS_TIP,
            ANGLE_LINES,
            SILHOUETTE_NOTE,
        ]
        .map(String::from),
    );
    texts.push(incomplete_text(1, 2));
    for mode in ToolMode::CHOICES {
        let mut tool = DimensionTool::default();
        tool.set_mode(mode);
        texts.push(tool.hint().unwrap());
    }
    for t in &texts {
        assert_plain(t);
    }
}

// ---------------------------------------------------------------------------------------------
// (k)

#[test]
fn escape_cancels_without_changes() {
    let (mut st, _, d, _, mut h) = box_front();
    h.click_text(&mut st, TOOL);
    let e0 = st.document.edit_count();
    h.click_at(&mut st, (95.0, 100.0));
    h.key(&mut st, Key::Escape);
    assert!(st.dimension_tool.pending().is_empty());
    assert!(
        st.dimension_tool.is_active(),
        "erst das zweite Esc schaltet aus"
    );
    // Bereit zum Setzen, Zeiger an der Stelle der Maßlinie, Esc: nichts gesetzt.
    h.click_at(&mut st, (95.0, 100.0));
    h.click_at(&mut st, (105.0, 100.0));
    h.hover_at(&mut st, (100.0, 130.0));
    h.key(&mut st, Key::Escape);
    assert!(st.dimension_tool.pending().is_empty());
    assert_eq!(h.layout().preview_drawn, 0);
    assert!(dims(&st, d).is_empty());
    assert_eq!(st.document.edit_count(), e0);
    // Ohne gewählte Bezüge schaltet Esc das Werkzeug aus; danach wählt ein Klick wieder.
    h.key(&mut st, Key::Escape);
    assert_eq!(st.dimension_tool.mode, ToolMode::Off);
    h.click_at(&mut st, (95.0, 100.0));
    assert!(st.dimension_tool.pending().is_empty());
    assert!(st.drawing_ui.selected_view.is_some());
    assert_eq!(st.document.edit_count(), e0);
}

// ---------------------------------------------------------------------------------------------
// (l)

/// Vorschau an `p` und danach gesetzte Maße: dieselben Markierungen.
fn preview_equals_final(st: &mut AppState, d: DrawingId, p: DVec2) {
    let preview = with_tool(st, |t, s| t.hover(s, d, p)).expect("Vorschau");
    assert!(!preview.is_empty());
    let ids = with_tool(st, |t, s| t.finish(s, d, p)).unwrap();
    let dims = resolve_in(st, d);
    let all = marks_in(st, d, &dims);
    let finals: Vec<Mark> = ids
        .iter()
        .flat_map(|id| {
            all.iter()
                .find(|dm| dm.id == *id)
                .expect("Marken des Maßes")
                .marks
                .clone()
        })
        .collect();
    assert_eq!(preview, finals);
}

#[test]
fn preview_matches_the_final_dimension_marks() {
    // Waagerecht über und unter dem Teil.
    let (mut st, _, d, _, mut h) = box_front();
    st.dimension_tool
        .set_mode(ToolMode::Linear(LinearDir::Horizontal));
    for y in [131.3, 70.2] {
        h.click_at(&mut st, (95.0, 100.0));
        h.click_at(&mut st, (105.0, 100.0));
        preview_equals_final(&mut st, d, DVec2::new(100.0, y));
    }
    // Schräg.
    st.dimension_tool
        .set_mode(ToolMode::Linear(LinearDir::Aligned));
    h.click_at(&mut st, (95.0, 85.0));
    h.click_at(&mut st, (105.0, 115.0));
    preview_equals_final(&mut st, d, DVec2::new(88.0, 112.0));

    // Durchmesser mit Führungslinie.
    let (mut st, d, _, mut h) = cylinder(ViewKind::Top);
    st.dimension_tool.set_mode(ToolMode::Diameter);
    h.click_at(&mut st, on_circle());
    preview_equals_final(&mut st, d, DVec2::new(91.0, 117.0));

    // Winkel, Kette und Bezugsmaße am L-Profil.
    let (mut st, d, _, mut h) = l_profile();
    st.dimension_tool.set_mode(ToolMode::Angle);
    h.click_at(&mut st, (105.0, 95.0));
    h.click_at(&mut st, (95.0, 105.0));
    preview_equals_final(&mut st, d, DVec2::new(110.0, 104.0));
    for mode in [ToolMode::Chain, ToolMode::Baseline] {
        st.dimension_tool.set_mode(mode);
        h.click_at(&mut st, (85.0, 100.0));
        h.click_at(&mut st, (95.0, 105.0));
        h.click_at(&mut st, (115.0, 90.0));
        preview_equals_final(&mut st, d, DVec2::new(100.0, 129.5));
    }
    // Bezugsmaße stapeln auch in der Vorschau: zweite Stufe 7 mm weiter außen.
    st.dimension_tool.set_mode(ToolMode::Baseline);
    h.click_at(&mut st, (85.0, 100.0));
    h.click_at(&mut st, (95.0, 105.0));
    h.click_at(&mut st, (115.0, 90.0));
    let preview = with_tool(&mut st, |t, s| t.hover(s, d, DVec2::new(100.0, 140.0))).unwrap();
    // Abstand 140 − 115 = 25: Stufen auf 140 und 147.
    assert!(
        has_line(&preview, (85.0, 140.0), (95.0, 140.0)),
        "{preview:?}"
    );
    assert!(
        has_line(&preview, (85.0, 147.0), (115.0, 147.0)),
        "{preview:?}"
    );
}

// ---------------------------------------------------------------------------------------------
// (m)

#[test]
fn tool_is_disabled_in_read_only_tabs_and_isometric_view() {
    // Alte Version: der Schalter ist gesperrt, ein eingeschaltetes Werkzeug geht aus, Klicks
    // setzen nichts.
    let mut st = AppState::new();
    st.create_box(BOX).unwrap();
    let d = new_drawing(&mut st);
    add_view(&mut st, d, ViewKind::Front, 100.0, 100.0);
    st.drawing_ui.open(d);
    st.drawing_ui.tick(&st.bodies, &st.document, true);
    let mut h = Harness::read_only();
    h.idle(&mut st);
    h.idle(&mut st);
    let e0 = st.document.edit_count();
    h.click_text(&mut st, TOOL);
    assert_eq!(st.dimension_tool.mode, ToolMode::Off);
    st.dimension_tool
        .set_mode(ToolMode::Linear(LinearDir::Horizontal));
    h.idle(&mut st);
    assert_eq!(st.dimension_tool.mode, ToolMode::Off);
    h.click_at(&mut st, (95.0, 100.0));
    h.click_at(&mut st, (105.0, 100.0));
    h.click_at(&mut st, (100.0, 130.0));
    assert!(st.dimension_tool.pending().is_empty());
    assert_eq!(st.document.edit_count(), e0);

    // Zeichnung nur mit Isometrie: Schalter gesperrt; das Werkzeug lehnt die Ansicht ab.
    let mut st = AppState::new();
    st.create_box(BOX).unwrap();
    let d = new_drawing(&mut st);
    let iso = add_view(&mut st, d, ViewKind::Isometric, 140.0, 100.0);
    let mut h = open(&mut st, d);
    h.click_text(&mut st, TOOL);
    assert_eq!(st.dimension_tool.mode, ToolMode::Off);
    st.dimension_tool
        .set_mode(ToolMode::Linear(LinearDir::Horizontal));
    h.idle(&mut st);
    assert_eq!(st.dimension_tool.mode, ToolMode::Off, "gesperrt heißt aus");
    let mut tool = DimensionTool::default();
    tool.set_mode(ToolMode::Linear(LinearDir::Horizontal));
    let hit = Hit {
        origin: StrokeOrigin::Edge { body: 0, edge: 0 },
        at: DVec2::new(140.0, 100.0),
        px_per_mm: 3.0,
    };
    assert_eq!(
        tool.click(&st, d, iso, &hit),
        ToolOutcome::Rejected(NO_ISOMETRIC.into())
    );
    assert!(tool.pending().is_empty());
    assert_eq!(
        with_tool(&mut st, |t, s| t.finish(s, d, DVec2::new(140.0, 130.0))),
        Err(ToolError::NotReady)
    );
    assert!(dims(&st, d).is_empty());
}

// ---------------------------------------------------------------------------------------------
// (n)

fn resize_box(st: &mut AppState, f: FeatureId, size: DVec3) {
    st.document
        .update(f, |k| {
            if let FeatureKind::Box { size: s, .. } = k {
                *s = size;
            }
        })
        .unwrap();
    st.refresh();
}

#[test]
fn every_created_dimension_resolves_to_the_value_of_the_formula_after_model_edit() {
    let (mut st, f, d, _, mut h) = box_front();
    let mut made = Vec::new();
    for (mode, a, b, place) in [
        (
            LinearDir::Horizontal,
            (95.0, 100.0),
            (105.0, 100.0),
            (100.0, 128.0),
        ),
        (
            LinearDir::Vertical,
            (100.0, 85.0),
            (100.0, 115.0),
            (122.0, 100.0),
        ),
        (
            LinearDir::Aligned,
            (95.0, 85.0),
            (105.0, 115.0),
            (88.0, 112.0),
        ),
    ] {
        st.dimension_tool.set_mode(ToolMode::Linear(mode));
        h.click_at(&mut st, a);
        h.click_at(&mut st, b);
        h.click_at(&mut st, place);
        made.push(dims(&st, d).last().unwrap().id);
    }
    assert_eq!(dims(&st, d).len(), 3);
    let formula = |s: DVec3| [s.x, s.z, (s.x * s.x + s.z * s.z).sqrt()];
    for (id, want) in made.iter().zip(formula(BOX)) {
        assert!(near(value(&mut st, d, *id), want));
    }
    let bigger = DVec3::new(20.0, 20.0, 40.0);
    resize_box(&mut st, f, bigger);
    for (id, want) in made.iter().zip(formula(bigger)) {
        let v = value(&mut st, d, *id);
        assert!(near(v, want), "{v} vs {want}");
    }

    // Winkel (linke Kante, Unterkante) und Kette (linke, rechte Kante) folgen ebenso.
    resize_box(&mut st, f, BOX);
    st.dimension_tool.set_mode(ToolMode::Angle);
    h.click_at(&mut st, (95.0, 100.0));
    h.click_at(&mut st, (100.0, 85.0));
    h.click_at(&mut st, (110.0, 95.0));
    let angle = dims(&st, d).last().unwrap().id;
    st.dimension_tool.set_mode(ToolMode::Chain);
    h.click_at(&mut st, (95.0, 100.0));
    h.click_at(&mut st, (105.0, 100.0));
    h.click_at(&mut st, (100.0, 128.0));
    let chain = dims(&st, d).last().unwrap().clone();
    assert!(chain.chain.is_some());
    assert!(near(value(&mut st, d, angle), 90.0));
    assert!(near(value(&mut st, d, chain.id), BOX.x));
    resize_box(&mut st, f, bigger);
    assert!(near(value(&mut st, d, angle), 90.0));
    assert!(near(value(&mut st, d, chain.id), bigger.x));

    // Ø und R am Zylinder: Radius 5 → 8.
    let mut st = AppState::new();
    let f = st.create_cylinder(5.0, 12.0).unwrap();
    let d = new_drawing(&mut st);
    add_view(&mut st, d, ViewKind::Top, 100.0, 100.0);
    let mut h = open(&mut st, d);
    let mut round = Vec::new();
    for (mode, place) in [
        (ToolMode::Diameter, (115.0, 112.0)),
        (ToolMode::Radius, (86.0, 88.0)),
    ] {
        st.dimension_tool.set_mode(mode);
        h.click_at(&mut st, on_circle());
        h.click_at(&mut st, place);
        round.push(dims(&st, d).last().unwrap().id);
    }
    assert!(near(value(&mut st, d, round[0]), 10.0));
    assert!(near(value(&mut st, d, round[1]), 5.0));
    st.document
        .update(f, |k| {
            if let FeatureKind::Cylinder { radius, .. } = k {
                *radius = 8.0;
            }
        })
        .unwrap();
    st.refresh();
    assert!(near(value(&mut st, d, round[0]), 16.0));
    assert!(near(value(&mut st, d, round[1]), 8.0));
}

// ---------------------------------------------------------------------------------------------
// (o)

#[test]
fn each_created_dimension_is_one_document_edit() {
    let (mut st, d, _, mut h) = l_profile();
    let m0 = st.document.model_edit_count();
    let mut e = st.document.edit_count();
    // Waagerecht: eine Änderung.
    st.dimension_tool
        .set_mode(ToolMode::Linear(LinearDir::Horizontal));
    h.click_at(&mut st, (85.0, 100.0));
    h.click_at(&mut st, (115.0, 90.0));
    assert_eq!(st.document.edit_count(), e);
    h.click_at(&mut st, (100.0, 125.0));
    e += 1;
    assert_eq!(st.document.edit_count(), e);
    // Kette mit drei Punkten: zwei Maße, zwei Änderungen.
    st.dimension_tool.set_mode(ToolMode::Chain);
    h.click_at(&mut st, (85.0, 100.0));
    h.click_at(&mut st, (95.0, 105.0));
    h.click_at(&mut st, (115.0, 90.0));
    h.click_at(&mut st, (100.0, 70.0));
    e += 2;
    assert_eq!(st.document.edit_count(), e);
    // Abgelehnter Klick: keine Änderung.
    st.dimension_tool.set_mode(ToolMode::Angle);
    h.click_at(&mut st, (105.0, 95.0));
    h.click_at(&mut st, (110.0, 95.0));
    assert!(st.dimension_tool.message().is_some());
    assert_eq!(st.document.edit_count(), e);
    assert_eq!(dims(&st, d).len(), 3);
    assert_eq!(st.document.model_edit_count(), m0);
}

// ---------------------------------------------------------------------------------------------
// Vorschlag und Werkzeug bilden dieselben Bezüge: kein doppeltes Maß.

#[test]
fn suggest_does_not_repeat_a_hand_made_dimension() {
    let (mut st, _, d, _, mut h) = box_front();
    st.dimension_tool
        .set_mode(ToolMode::Linear(LinearDir::Horizontal));
    h.click_at(&mut st, (95.0, 100.0));
    h.click_at(&mut st, (105.0, 100.0));
    h.click_at(&mut st, (100.0, 128.0));
    assert_eq!(dims(&st, d).len(), 1);
    // Vorschlag: nur noch die Höhe.
    assert_eq!(suggest_for(&mut st, d, None), SuggestOutcome::Added(1));
    let all = dims(&st, d);
    assert_eq!(all.len(), 2);
    assert_eq!(all[1].kind, DimKind::Vertical);
}

// ---------------------------------------------------------------------------------------------
// Fix-Runde 1

#[test]
fn escape_never_gets_stuck_at_the_dimension_limit() {
    let (mut st, d, top, mut h) = l_profile();
    // Ein Maß von Hand, dann so viele Kopien, dass nur noch ein Maß Platz hat.
    st.dimension_tool
        .set_mode(ToolMode::Linear(LinearDir::Horizontal));
    h.click_at(&mut st, (85.0, 100.0));
    h.click_at(&mut st, (115.0, 90.0));
    h.click_at(&mut st, (100.0, 125.0));
    let one = dims(&st, d)[0].clone();
    while dims(&st, d).len() < planora_drawing::limits::MAX_DIMENSIONS - 1 {
        st.document.add_dimension(d, one.clone()).unwrap();
    }
    h.idle(&mut st);
    let e0 = st.document.edit_count();
    // Kette mit drei Punkten = zwei Maße: passt nicht mehr. Esc räumt trotzdem auf.
    st.dimension_tool.set_mode(ToolMode::Chain);
    h.click_at(&mut st, (85.0, 100.0));
    h.click_at(&mut st, (95.0, 105.0));
    h.click_at(&mut st, (115.0, 90.0));
    h.hover_at(&mut st, (100.0, 140.0));
    let shot = h.key(&mut st, Key::Escape);
    assert!(st.dimension_tool.pending().is_empty());
    assert!(!st.dimension_tool.is_ready());
    assert_eq!(st.dimension_tool.view(), None);
    assert_eq!(st.dimension_tool.message(), Some(full_text().as_str()));
    assert!(shot.text.contains(&full_text()), "{}", shot.text);
    assert_eq!(st.document.edit_count(), e0);
    assert_eq!(
        dims(&st, d).len(),
        planora_drawing::limits::MAX_DIMENSIONS - 1
    );
    // Ein zweites Esc schaltet das Werkzeug aus.
    h.key(&mut st, Key::Escape);
    assert_eq!(st.dimension_tool.mode, ToolMode::Off);
    let _ = top;
}

#[test]
fn double_click_finishes_only_chains() {
    let (mut st, _, d, _, mut h) = box_front();
    // Waagerecht: der Doppelklick wählt die zweite Kante, setzt aber nicht vorzeitig.
    st.dimension_tool
        .set_mode(ToolMode::Linear(LinearDir::Horizontal));
    h.click_at(&mut st, (95.0, 100.0));
    h.double_click_at(&mut st, (105.0, 100.0));
    assert!(
        st.dimension_tool.is_ready(),
        "{:?} {:?} {:?}",
        st.dimension_tool.pending(),
        st.dimension_tool.message(),
        dims(&st, d)
    );
    assert!(dims(&st, d).is_empty());
    let p = h.click_at(&mut st, (100.0, 128.0));
    assert_eq!(dims(&st, d).len(), 1);
    assert!(near(dims(&st, d)[0].offset, p.y - 115.0));
    // Kette: der Doppelklick auf den letzten Punkt nimmt ihn auf und setzt die Kette.
    st.dimension_tool.set_mode(ToolMode::Chain);
    h.click_at(&mut st, (95.0, 100.0));
    h.double_click_at(&mut st, (105.0, 100.0));
    let all = dims(&st, d);
    assert_eq!(all.len(), 2, "{all:?}");
    assert!(all[1].chain.is_some());
    assert!(near(value(&mut st, d, all[1].id), BOX.x));
    assert!(st.dimension_tool.pending().is_empty());
}

#[test]
fn chain_is_placed_by_a_click_in_empty_space() {
    let (mut st, _, d, _, mut h) = box_front();
    st.dimension_tool.set_mode(ToolMode::Chain);
    h.click_at(&mut st, (95.0, 100.0));
    h.click_at(&mut st, (105.0, 100.0));
    assert!(dims(&st, d).is_empty());
    let p = h.click_at(&mut st, (100.0, 130.0));
    let all = dims(&st, d);
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].chain.unwrap().mode, ChainMode::Chain);
    assert!(near(all[0].offset, p.y - 115.0));
    assert!(near(value(&mut st, d, all[0].id), BOX.x));
}

#[test]
fn silhouette_in_a_linear_dimension_measures_to_the_axis_and_says_so() {
    // Quader x 0…10 und Zylinder r 5 mit Achse bei x 25 daneben; Vorderansicht in (100, 100),
    // Hüllbox-Mitte (15, 15): Blatt = Modell + 85. Quader links x 85, Zylinderumriss x 105.
    let mut st = AppState::new();
    st.create_box(BOX).unwrap();
    st.create_cylinder(5.0, 12.0).unwrap();
    let d = new_drawing(&mut st);
    add_view(&mut st, d, ViewKind::Front, 100.0, 100.0);
    let mut h = open(&mut st, d);
    st.dimension_tool
        .set_mode(ToolMode::Linear(LinearDir::Horizontal));
    h.click_at(&mut st, (85.0, 100.0));
    assert_eq!(st.dimension_tool.notice(), None);
    h.click_at(&mut st, (105.0, 91.0));
    assert!(
        matches!(st.dimension_tool.pending()[1], GeoRef::Face(_)),
        "{:?}",
        st.dimension_tool.pending()
    );
    assert_eq!(st.dimension_tool.notice(), Some(SILHOUETTE_NOTE));
    let shot = h.idle(&mut st);
    assert!(shot.text.contains(SILHOUETTE_NOTE), "{}", shot.text);
    h.click_at(&mut st, (100.0, 125.0));
    let all = dims(&st, d);
    assert_eq!(all.len(), 1);
    // Von der linken Quaderkante (x 0) bis zur Zylinderachse (x 25).
    assert!(near(value(&mut st, d, all[0].id), 25.0));
    assert_eq!(st.dimension_tool.notice(), None);
    // Als Durchmesser kein Hinweis (dort ist der Umriss gemeint).
    st.dimension_tool.set_mode(ToolMode::Diameter);
    h.click_at(&mut st, (105.0, 91.0));
    assert!(st.dimension_tool.is_ready());
    assert_eq!(st.dimension_tool.notice(), None);
}

#[test]
fn tool_lock_texts_and_sentences() {
    let mut st = AppState::new();
    st.create_box(BOX).unwrap();
    let d = new_drawing(&mut st);
    let drawing = |st: &AppState| st.document.drawing(d).unwrap().clone();
    assert_eq!(
        drawing_view::tool_blocked(&drawing(&st), true),
        Some(NO_VIEWS_TIP)
    );
    add_view(&mut st, d, ViewKind::Isometric, 140.0, 100.0);
    assert_eq!(
        drawing_view::tool_blocked(&drawing(&st), true),
        Some(ISOMETRIC_TIP)
    );
    add_view(&mut st, d, ViewKind::Front, 60.0, 100.0);
    assert_eq!(drawing_view::tool_blocked(&drawing(&st), true), None);
    assert_eq!(
        drawing_view::tool_blocked(&drawing(&st), false),
        Some(READ_ONLY_TIP)
    );
    // Durchgereichte Texte enden mit einem Satzzeichen.
    assert_eq!(sentence("Das Maß gibt es nicht"), "Das Maß gibt es nicht.");
    assert_eq!(sentence("Schon ein Satz."), "Schon ein Satz.");
    assert_eq!(sentence("Wirklich?"), "Wirklich?");
    assert_eq!(
        incomplete_text(1, 2),
        "Die Maße sind unvollständig: nur 1 von 2 Maßen wurden gesetzt."
    );
}
