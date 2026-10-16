//! Maße und Mittellinien im Zeichnungsbereich (Teilprojekt 12, Aufgabe 11): Zeichnen der
//! Markierungen, Auswahl, Ziehen, Werkzeugleiste („Mittellinien“, „Maße vorschlagen“) und die
//! Hinweiszeile – mit echten egui-Ereignissen wie `tests/drawing_ui.rs`.
//!
//! Sollwerte aus den Eingabemaßen (von Hand gerechnet): Quader 10 × 20 × 30 (x, y, z),
//! Vorderansicht 1:1 mit Mitte (100, 100) → Kanten x 95…105, y 85…115. Breitenmaß mit
//! Abstand 10 → Maßlinie y = 125 von x 95 bis 105, Hilfslinien von y 116 bis 127, Pfeile
//! innen (Abstand 10 ≥ 2·3,5 + 1) mit Spitzen auf 95 und 105, Länge 3,5, halbe Breite 0,6. Der
//! Text „10,0“ (Breite 4·0,6·3,5 = 8,4) passt nicht zwischen die Pfeile (10 − 7 = 3) und
//! steht hinter dem rechten Ende: Anker `Start` bei (105 + 3,5 + 2; 125 + 1) = (110,5; 126).
//! Höhenmaß mit Abstand 10 → Maßlinie x = 115 von y 85 bis 115 (positiv = rechts).

use egui::epaint::Shape;
use egui::{Color32, Event, Modifiers, PointerButton, Pos2, Rect, vec2};
use planora_app::browser::ERROR_COLOR;
use planora_app::dimension_state::*;
use planora_app::drawing_view;
use planora_app::state::*;
use planora_app::testing::collect_texts;
use planora_app::{fonts, theme};
use planora_drawing::annotate::{Mark, MarkKind};
use planora_drawing::dimension::{
    DimId, DimKind, DimSpec, GeoRef, GeoRefData, Tolerance as DimTolerance,
};
use planora_drawing::{
    DrawBody, DrawError, DrawingId, Paper, Placement, Projection, Scale, Sheet, ViewId, ViewKind,
    ViewLines, compute_view,
};
use planora_exchange::Progress;
use planora_feature::geo_ref_for_edge;
use planora_math::{DVec2, DVec3, Tolerance};

const W: f32 = 1200.0;
const H: f32 = 800.0;

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

/// Ein Text mit Lage, Farbe und Schriftgröße.
struct Seen {
    text: String,
    rect: Rect,
    color: Color32,
    size: f32,
    angle: f32,
}

fn walk(shape: &Shape, texts: &mut Vec<Seen>, flat: &mut Vec<Shape>) {
    match shape {
        Shape::Vec(v) => v.iter().for_each(|s| walk(s, texts, flat)),
        Shape::Text(t) => {
            let section = t.galley.job.sections.first();
            let color = section
                .map(|s| s.format.color)
                .filter(|c| *c != Color32::PLACEHOLDER)
                .unwrap_or(t.fallback_color);
            texts.push(Seen {
                text: t.galley.text().to_string(),
                rect: Rect::from_min_size(t.pos, t.galley.size()),
                color,
                size: section.map_or(0.0, |s| s.format.font_id.size),
                angle: t.angle,
            });
            flat.push(shape.clone());
        }
        s => flat.push(s.clone()),
    }
}

/// Ein Bild der Oberfläche.
struct Shot {
    text: String,
    texts: Vec<Seen>,
    flat: Vec<Shape>,
}

impl Shot {
    fn pos_of(&self, exact: &str) -> Option<Pos2> {
        self.texts
            .iter()
            .find(|t| t.text == exact)
            .map(|t| t.rect.left_center() + vec2(6.0, 0.0))
    }

    fn text(&self, exact: &str) -> &Seen {
        self.texts
            .iter()
            .find(|t| t.text == exact)
            .unwrap_or_else(|| panic!("Text „{exact}“ fehlt:\n{}", self.text))
    }

    /// Strecken (`LineSegment`) mit Farbe und Breite.
    fn segments(&self) -> Vec<([Pos2; 2], Color32, f32)> {
        self.flat
            .iter()
            .filter_map(|s| match s {
                Shape::LineSegment { points, stroke } => {
                    Some((*points, stroke.color, stroke.width))
                }
                _ => None,
            })
            .collect()
    }

    /// Gefüllte Vielecke (Pfeile) mit Füllfarbe.
    fn polygons(&self) -> Vec<(Vec<Pos2>, Color32)> {
        self.flat
            .iter()
            .filter_map(|s| match s {
                Shape::Path(p) if p.closed && p.fill != Color32::TRANSPARENT => {
                    Some((p.points.clone(), p.fill))
                }
                _ => None,
            })
            .collect()
    }

    /// Offene Linienzüge mit einfarbigem Strich.
    fn paths(&self) -> Vec<(Vec<Pos2>, Color32, f32)> {
        self.flat
            .iter()
            .filter_map(|s| match s {
                Shape::Path(p) if !p.closed => match p.stroke.color {
                    egui::epaint::ColorMode::Solid(c) => {
                        Some((p.points.clone(), c, p.stroke.width))
                    }
                    _ => None,
                },
                _ => None,
            })
            .collect()
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
        self.time += 0.05;
        let what = self.what;
        let out = self.ctx.run_ui(raw(events, self.time), |ui| match what {
            What::Area => drawing_view::show(ui, st),
            What::ReadOnly => drawing_view::show_with(ui, st, false),
        });
        let mut texts = Vec::new();
        let mut flat = Vec::new();
        for c in &out.shapes {
            walk(&c.shape, &mut texts, &mut flat);
        }
        let shot = Shot {
            text: collect_texts(&out),
            texts,
            flat,
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

    /// Pixel je Blatt-mm im zuletzt gezeichneten Bild.
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

    /// Ziehen mit der linken Maustaste von `from` um `by` (in zwei Schritten).
    fn drag(&mut self, st: &mut AppState, from: Pos2, by: egui::Vec2) {
        let to = from + by;
        self.frame(st, vec![Event::PointerMoved(from)]);
        self.frame(st, vec![button(from, true)]);
        self.frame(st, vec![Event::PointerMoved(from + by * 0.5)]);
        self.frame(st, vec![Event::PointerMoved(to)]);
        self.frame(st, vec![button(to, false)]);
        self.idle(st);
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

/// Bildschirmpunkt eines Blattpunkts (mm) im zuletzt gezeichneten Bereich.
fn screen(h: &Harness, p: (f64, f64)) -> Pos2 {
    let l = h.layout();
    drawing_view::sheet_to_screen(l.canvas, l.sheet_mm, l.zoom, l.pan, DVec2::new(p.0, p.1))
}

fn close(a: Pos2, b: Pos2, tol: f32) -> bool {
    (a - b).length() <= tol
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

fn new_drawing(st: &mut AppState) -> DrawingId {
    st.document
        .add_drawing("Blatt 1", A4_QUER, Projection::First)
        .unwrap()
}

fn add_view(st: &mut AppState, d: DrawingId, kind: ViewKind, x: f64, y: f64) -> ViewId {
    st.document
        .add_view(d, kind, s11(), Placement::Free { x, y }, None)
        .unwrap()
}

fn compute(st: &mut AppState) {
    st.drawing_ui.tick(&st.bodies, &st.document, true);
}

/// Verweisdaten der geraden Kante des Körpers `body`, deren beide Eckpunkte `pred` erfüllen.
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

/// Breite (linke und rechte senkrechte Vorderkante) des Körpers `body`.
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

/// Höhe (untere und obere waagerechte Vorderkante) des Körpers `body`.
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

/// Quader 10 × 20 × 30, Zeichnung mit Vorderansicht in (100, 100), geöffnet und berechnet,
/// mit Breitenmaß (Abstand 10).
fn box_with_width() -> (AppState, DrawingId, ViewId, DimId, Harness) {
    let mut st = AppState::new();
    st.create_box(BOX).unwrap();
    let d = new_drawing(&mut st);
    let front = add_view(&mut st, d, ViewKind::Front, 100.0, 100.0);
    let w = add_width(&mut st, d, front, 0, 10.0);
    st.drawing_ui.open(d);
    compute(&mut st);
    let mut h = Harness::new();
    h.idle(&mut st);
    h.idle(&mut st);
    (st, d, front, w, h)
}

fn dim(st: &AppState, d: DrawingId, id: DimId) -> DimSpec {
    st.document
        .drawing(d)
        .unwrap()
        .dimension(id)
        .unwrap()
        .clone()
}

fn top_fails(b: &[DrawBody], k: ViewKind, p: &Progress) -> Result<ViewLines, DrawError> {
    if k == ViewKind::Top {
        Err(DrawError::TooComplex(5))
    } else {
        compute_view(b, k, p)
    }
}

// ---------------------------------------------------------------------------------------------
// (a)

#[test]
fn marks_are_painted_with_expected_screen_positions() {
    let (mut st, _, _, _, mut h) = box_with_width();
    let shot = h.idle(&mut st);
    let k = h.k();
    let segs = shot.segments();
    let width = (0.25 * k).max(1.0) as f32;
    // Maßlinie (95, 125)–(105, 125).
    let (a, b) = (screen(&h, (95.0, 125.0)), screen(&h, (105.0, 125.0)));
    assert!(
        segs.iter().any(|(p, c, w)| *c == theme::SHEET_DIM
            && (*w - width).abs() < 1e-4
            && ((close(p[0], a, 0.01) && close(p[1], b, 0.01))
                || (close(p[0], b, 0.01) && close(p[1], a, 0.01)))),
        "Maßlinie {a:?}–{b:?} fehlt: {segs:?}"
    );
    // Beide Hilfslinien von y 116 bis 127.
    for x in [95.0, 105.0] {
        let (a, b) = (screen(&h, (x, 116.0)), screen(&h, (x, 127.0)));
        assert!(
            segs.iter().any(|(p, c, _)| *c == theme::SHEET_DIM
                && ((close(p[0], a, 0.01) && close(p[1], b, 0.01))
                    || (close(p[0], b, 0.01) && close(p[1], a, 0.01)))),
            "Hilfslinie x {x} fehlt: {segs:?}"
        );
    }
    // Pfeile: Spitze und Ecken am Ende (3,5 lang, 0,6 halbe Breite), gefüllt.
    let polys = shot.polygons();
    for (tip, base) in [((95.0, 125.0), 98.5), ((105.0, 125.0), 101.5)] {
        let want = [
            screen(&h, tip),
            screen(&h, (base, 125.6)),
            screen(&h, (base, 124.4)),
        ];
        assert!(
            polys.iter().any(|(pts, c)| *c == theme::SHEET_DIM
                && pts.len() == 3
                && want.iter().all(|w| pts.iter().any(|p| close(*p, *w, 0.01)))),
            "Pfeil an {tip:?} fehlt: {polys:?}"
        );
    }
    // Text „10,0“: Anker Start (110,5; 126) auf der Grundlinie, Schrift 3,5·k Pixel, waagerecht.
    let t = shot.text("10,0");
    let anchor = screen(&h, (110.5, 126.0));
    assert!(
        (t.rect.left() - anchor.x).abs() < 0.01,
        "{:?} vs {anchor:?}",
        t.rect
    );
    assert!(
        t.rect.top() < anchor.y && anchor.y < t.rect.bottom(),
        "Grundlinie {anchor:?} in {:?}",
        t.rect
    );
    // Die Grundlinie liegt in der unteren Hälfte der Textzeile (über der Unterlänge).
    assert!(anchor.y > t.rect.center().y, "{:?} vs {anchor:?}", t.rect);
    assert!(
        // Auf halbe Pixel gerundet.
        (f64::from(t.size) - 3.5 * k).abs() <= 0.25 + 1e-6 && (t.size * 2.0).fract() == 0.0,
        "{} vs {}",
        t.size,
        3.5 * k
    );
    assert_eq!(t.angle, 0.0);
    assert_eq!(t.color, theme::SHEET_DIM);
    assert!(h.layout().marks_drawn >= 6, "{}", h.layout().marks_drawn);
}

#[test]
fn vertical_text_is_rotated() {
    // Senkrechtes Maß: Text um 90° gedreht (Bildschirm: −π/2 im Uhrzeigersinn), Linie x 115.
    let (mut st, d, front, _, mut h) = box_with_width();
    add_height(&mut st, d, front, 0, 10.0);
    let shot = h.idle(&mut st);
    let segs = shot.segments();
    let (a, b) = (screen(&h, (115.0, 85.0)), screen(&h, (115.0, 115.0)));
    assert!(
        segs.iter()
            .any(|(p, _, _)| (close(p[0], a, 0.01) && close(p[1], b, 0.01))
                || (close(p[0], b, 0.01) && close(p[1], a, 0.01))),
        "senkrechte Maßlinie fehlt"
    );
    let t = shot.text("30,0");
    assert!(
        (t.angle + std::f32::consts::FRAC_PI_2).abs() < 1e-6,
        "{}",
        t.angle
    );
}

// ---------------------------------------------------------------------------------------------
// (b), (k)

/// Zylinder r 10, h 30 (Achse senkrecht in der Vorderansicht), Vorderansicht in (100, 105).
fn cylinder_front() -> (AppState, DrawingId, ViewId, Harness) {
    let mut st = AppState::new();
    st.create_cylinder(10.0, 30.0).unwrap();
    let d = new_drawing(&mut st);
    let front = add_view(&mut st, d, ViewKind::Front, 100.0, 105.0);
    st.drawing_ui.open(d);
    compute(&mut st);
    let mut h = Harness::new();
    h.idle(&mut st);
    h.idle(&mut st);
    (st, d, front, h)
}

/// Strichstücke der Mittellinie auf x 100 (Bildschirm): Längen in Pixeln, sortiert.
fn centerline_pieces(shot: &Shot, h: &Harness) -> Vec<f32> {
    let x = screen(h, (100.0, 0.0)).x;
    let mut v: Vec<f32> = shot
        .paths()
        .iter()
        .filter(|(pts, c, _)| {
            *c == theme::SHEET_DIM && pts.len() >= 2 && pts.iter().all(|p| (p.x - x).abs() < 0.01)
        })
        .map(|(pts, _, _)| pts.windows(2).map(|w| (w[1] - w[0]).length()).sum())
        .collect();
    v.sort_by(f32::total_cmp);
    v
}

#[test]
fn centerlines_are_dashed_and_follow_the_toggle() {
    let (mut st, d, front, mut h) = cylinder_front();
    // Achse von y 90 bis 120, je 3 mm Überstand: (100, 87)–(100, 123), 36 mm lang.
    let sheet = drawing_view::last_sheet(&h.ctx).unwrap();
    let marks = &sheet.views.iter().find(|v| v.id == front).unwrap().marks;
    let center: Vec<&Mark> = marks
        .iter()
        .filter(|m| {
            matches!(
                m,
                Mark::Line {
                    kind: MarkKind::CenterLine,
                    ..
                }
            )
        })
        .collect();
    assert_eq!(center.len(), 1, "{marks:?}");
    let Mark::Line { a, b, .. } = center[0] else {
        unreachable!()
    };
    let (lo, hi) = if a.y < b.y { (a, b) } else { (b, a) };
    assert!(
        (lo.x - 100.0).abs() < 1e-9 && (hi.x - 100.0).abs() < 1e-9,
        "{a:?} {b:?}"
    );
    assert!(
        (lo.y - 87.0).abs() < 1e-9 && (hi.y - 123.0).abs() < 1e-9,
        "{a:?} {b:?}"
    );
    // Strichpunkt [12, 3, 2, 3] über 36 mm: 12, 2, 12, 1 (Rest) – in Pixeln.
    let shot = h.idle(&mut st);
    let k = h.k() as f32;
    let pieces = centerline_pieces(&shot, &h);
    let want = [1.0 * k, 2.0 * k, 12.0 * k, 12.0 * k];
    assert_eq!(pieces.len(), 4, "{pieces:?}");
    for (p, w) in pieces.iter().zip(want) {
        assert!((p - w).abs() < 0.01, "{pieces:?} vs {want:?}");
    }
    let width = shot
        .paths()
        .iter()
        .find(|(pts, c, _)| {
            *c == theme::SHEET_DIM
                && pts
                    .iter()
                    .all(|p| (p.x - screen(&h, (100.0, 0.0)).x).abs() < 0.01)
        })
        .unwrap()
        .2;
    assert!((width - (0.25 * k).max(1.0)).abs() < 1e-4, "{width}");

    // Schalter aus: keine Mittellinie mehr.
    st.drawing_ui.selected_view = Some(front);
    h.click_text(&mut st, drawing_view::CENTERLINES);
    assert!(
        !st.document
            .drawing(d)
            .unwrap()
            .view(front)
            .unwrap()
            .centerlines
    );
    let shot = h.idle(&mut st);
    assert!(centerline_pieces(&shot, &h).is_empty());
}

#[test]
fn toggle_centerlines_updates_the_sheet() {
    let (mut st, d, front, mut h) = cylinder_front();
    let has_center = |h: &Harness| {
        drawing_view::last_sheet(&h.ctx).unwrap().views[0]
            .marks
            .iter()
            .any(|m| {
                matches!(
                    m,
                    Mark::Line {
                        kind: MarkKind::CenterLine,
                        ..
                    }
                )
            })
    };
    assert!(
        st.document
            .drawing(d)
            .unwrap()
            .view(front)
            .unwrap()
            .centerlines
    );
    assert!(has_center(&h));
    st.drawing_ui.selected_view = Some(front);
    let (model, edits) = (st.document.model_edit_count(), st.document.edit_count());
    let spawned = st.drawing_ui.spawned();
    h.click_text(&mut st, drawing_view::CENTERLINES);
    assert!(
        !st.document
            .drawing(d)
            .unwrap()
            .view(front)
            .unwrap()
            .centerlines
    );
    assert!(!has_center(&h));
    assert_eq!(st.document.edit_count(), edits + 1, "eine Änderung");
    assert_eq!(st.document.model_edit_count(), model, "Modell bleibt");
    assert_eq!(st.drawing_ui.spawned(), spawned, "nichts neu berechnet");
    h.click_text(&mut st, drawing_view::CENTERLINES);
    assert!(
        st.document
            .drawing(d)
            .unwrap()
            .view(front)
            .unwrap()
            .centerlines
    );
    assert!(has_center(&h));
}

// ---------------------------------------------------------------------------------------------
// (c), (l)

/// Zwei Quader; das Breitenmaß gehört zum zweiten, der danach gelöscht wird.
fn lost_state() -> (AppState, DrawingId, ViewId, DimId, Harness) {
    let mut st = AppState::new();
    st.create_box(BOX).unwrap();
    let b = st.create_box(BOX).unwrap();
    let d = new_drawing(&mut st);
    let front = add_view(&mut st, d, ViewKind::Front, 120.0, 100.0);
    let id = add_width(&mut st, d, front, 1, 10.0);
    st.drawing_ui.open(d);
    compute(&mut st);
    let mut h = Harness::new();
    h.idle(&mut st);
    st.delete_feature(b).unwrap();
    compute(&mut st);
    h.idle(&mut st);
    h.idle(&mut st);
    (st, d, front, id, h)
}

#[test]
fn lost_dimension_is_red_and_has_a_tooltip_with_the_reason() {
    let (mut st, _, _, _, mut h) = lost_state();
    // Die rote Maßlinie liegt 10 mm über der Oberkante (y 125) und ist 10 mm lang.
    let sheet = drawing_view::last_sheet(&h.ctx).unwrap();
    let line = sheet.views[0]
        .marks
        .iter()
        .find_map(|m| match m {
            Mark::Line {
                a,
                b,
                kind: MarkKind::Lost,
            } if near(a.y, 125.0) && near(b.y, 125.0) && near((a.x - b.x).abs(), 10.0) => {
                Some((*a, *b))
            }
            _ => None,
        })
        .expect("rote Maßlinie");
    let shot = h.idle(&mut st);
    let (a, b) = (screen(&h, (line.0.x, 125.0)), screen(&h, (line.1.x, 125.0)));
    let segs = shot.segments();
    assert!(
        segs.iter().any(|(p, c, _)| *c == theme::SHEET_LOST
            && ((close(p[0], a, 0.01) && close(p[1], b, 0.01))
                || (close(p[0], b, 0.01) && close(p[1], a, 0.01)))),
        "rote Maßlinie fehlt: {segs:?}"
    );
    assert!(
        !segs
            .iter()
            .any(|(p, c, _)| *c == theme::SHEET_DIM
                && (close(p[0], a, 0.01) || close(p[1], a, 0.01))),
        "nicht schwarz"
    );
    assert!(shot.polygons().iter().any(|(_, c)| *c == theme::SHEET_LOST));
    let red_text = shot
        .texts
        .iter()
        .find(|t| t.text.starts_with("? "))
        .expect("Grund auf dem Blatt");
    assert_eq!(red_text.color, theme::SHEET_LOST);
    assert_ne!(theme::SHEET_LOST, theme::SHEET_DIM);

    // Tooltip: Zeiger über der roten Linie, die Zeit läuft weiter.
    let mid = a + (b - a) * 0.5;
    h.frame(&mut st, vec![Event::PointerMoved(mid)]);
    let mut text = String::new();
    for _ in 0..30 {
        text = h.idle(&mut st).text;
    }
    assert!(
        text.contains(drawing_view::LOST_TIP),
        "Tooltip fehlt:\n{text}"
    );
    assert!(
        text.contains("an der das Maß hing, gibt es nicht mehr"),
        "Grund fehlt:\n{text}"
    );
    // Über leerem Papier kein Tooltip.
    let empty = screen(&h, (250.0, 30.0));
    h.frame(&mut st, vec![Event::PointerMoved(empty)]);
    for _ in 0..30 {
        text = h.idle(&mut st).text;
    }
    assert!(!text.contains(drawing_view::LOST_TIP), "{text}");
}

#[test]
fn notes_line_shows_lost_and_outside_counts_in_german() {
    let (mut st, d, front, _, mut h) = lost_state();
    let far = add_height(&mut st, d, front, 0, 5000.0);
    let shot = h.idle(&mut st);
    let shot2 = h.idle(&mut st);
    for t in [
        "1 Maß hat seinen Bezug verloren.",
        "1 Maß liegt außerhalb des Blatts.",
    ] {
        assert!(shot2.text.contains(t), "„{t}“ fehlt:\n{}", shot2.text);
    }
    // Der Satz über verlorene Maße ist rot, der Hinweis bleibt daneben.
    assert_eq!(
        shot.text("1 Maß hat seinen Bezug verloren.").color,
        ERROR_COLOR
    );
    assert!(shot2.text.contains(drawing_view::HINT));
    let hint = h.layout().hint;
    assert!(
        hint.contains(
            shot2
                .text("1 Maß liegt außerhalb des Blatts.")
                .rect
                .center()
        )
    );
    // Die Hinweise kommen aus dem Zwischenspeicher: unveränderte Bilder rechnen nichts neu.
    let builds = st.dimension_ui.mark_builds();
    for _ in 0..3 {
        h.idle(&mut st);
    }
    assert_eq!(st.dimension_ui.mark_builds(), builds);
    // Zurück aufs Blatt: der Hinweis verschwindet.
    st.document
        .update_dimension(d, far, |m| m.offset = 10.0)
        .unwrap();
    let shot = h.idle(&mut st);
    assert!(!shot.text.contains("außerhalb"), "{}", shot.text);
    assert!(st.dimension_ui.mark_builds() > builds);
}

#[test]
fn notes_line_uses_the_plural_for_two_lost_and_two_outside() {
    // Zwei Maße am zweiten Quader (danach gelöscht), zwei weit außerhalb am ersten.
    let mut st = AppState::new();
    st.create_box(BOX).unwrap();
    let b = st.create_box(BOX).unwrap();
    let d = new_drawing(&mut st);
    let front = add_view(&mut st, d, ViewKind::Front, 120.0, 100.0);
    add_width(&mut st, d, front, 1, 10.0);
    add_height(&mut st, d, front, 1, 10.0);
    add_width(&mut st, d, front, 0, 5000.0);
    add_height(&mut st, d, front, 0, 5000.0);
    st.drawing_ui.open(d);
    compute(&mut st);
    st.delete_feature(b).unwrap();
    compute(&mut st);
    let mut h = Harness::new();
    h.idle(&mut st);
    let shot = h.idle(&mut st);
    for t in [
        "2 Maße haben ihren Bezug verloren.",
        "2 Maße liegen außerhalb des Blatts.",
    ] {
        assert!(shot.text.contains(t), "„{t}“ fehlt:\n{}", shot.text);
    }
    assert!(!shot.text.contains("1 Maß"), "{}", shot.text);
}

#[test]
fn hint_line_keeps_the_lost_count_first_and_nothing_leaves_the_line() {
    // Verlorenes Maß, Maß außerhalb und eine lange Meldung: alles bleibt in der Zeile, der
    // Satz über verlorene Maße steht vorn, der bisherige Hinweis ist (gekürzt) noch da.
    let (mut st, d, front, _, mut h) = lost_state();
    add_height(&mut st, d, front, 0, 5000.0);
    h.idle(&mut st);
    st.drawing_ui.dim_message = Some("Eine sehr lange Meldung. ".repeat(20));
    st.drawing_ui.dim_message_key = Some((
        st.document.edit_count(),
        st.drawing_ui.selected_view,
        st.drawing_ui.selected_dim,
    ));
    let shot = h.idle(&mut st);
    let hint = h.layout().hint;
    let in_line: Vec<&_> = shot
        .texts
        .iter()
        .filter(|t| hint.contains(t.rect.center()))
        .collect();
    let lost = shot.text("1 Maß hat seinen Bezug verloren.");
    assert!(
        in_line.iter().all(|t| t.rect.left() >= lost.rect.left()),
        "verlorene Maße zuerst"
    );
    assert!(
        in_line.iter().all(|t| t.rect.right() <= hint.right() + 0.5),
        "nichts ragt hinaus: {:?}",
        in_line
            .iter()
            .map(|t| (&t.text, t.rect))
            .collect::<Vec<_>>()
    );
    assert!(
        in_line
            .iter()
            .any(|t| t.text.starts_with("Eine sehr lange")),
        "Meldung fehlt"
    );
    assert!(
        in_line.iter().any(|t| t.text.starts_with("Klicken wählt")),
        "bisheriger Hinweis fehlt: {}",
        shot.text
    );
}

#[test]
fn the_suggest_message_disappears_after_the_next_change() {
    let (mut st, d, _, _, mut h) = front_and_top(None);
    h.click_text(&mut st, drawing_view::SUGGEST);
    assert!(h.idle(&mut st).text.contains("3 Maße wurden hinzugefügt."));
    // Eine Auswahl (hier: ein Maß) lässt die Meldung verfallen.
    h.click(&mut st, screen(&h, (100.0, 125.0)));
    assert!(st.drawing_ui.selected_dim.is_some());
    let shot = h.idle(&mut st);
    assert!(!shot.text.contains("hinzugefügt"), "{}", shot.text);
    assert_eq!(st.drawing_ui.dim_message, None);
    // Ebenso eine Änderung am Dokument.
    h.click(&mut st, screen(&h, (250.0, 30.0)));
    h.click_text(&mut st, drawing_view::SUGGEST);
    assert!(st.drawing_ui.dim_message.is_some());
    let w = st.document.drawing(d).unwrap().dimensions[0].id;
    st.document
        .update_dimension(d, w, |m| m.offset = 12.0)
        .unwrap();
    h.idle(&mut st);
    assert_eq!(st.drawing_ui.dim_message, None);
}

// ---------------------------------------------------------------------------------------------
// (d)

#[test]
fn click_selects_the_dimension_by_line_and_by_text() {
    let (mut st, _, front, w, mut h) = box_with_width();
    // Auf der Maßlinie.
    let shot = h.click(&mut st, screen(&h, (100.0, 125.0)));
    assert_eq!(st.drawing_ui.selected_dim, Some(w));
    assert_eq!(st.drawing_ui.selected_view, None);
    // Gewählt: in der Akzentfarbe, nicht mehr schwarz.
    let (a, b) = (screen(&h, (95.0, 125.0)), screen(&h, (105.0, 125.0)));
    let line = |c: Color32| {
        shot.segments().iter().any(|(p, col, _)| {
            *col == c
                && ((close(p[0], a, 0.01) && close(p[1], b, 0.01))
                    || (close(p[0], b, 0.01) && close(p[1], a, 0.01)))
        })
    };
    assert!(line(theme::SHEET_SELECTED) && !line(theme::SHEET_DIM));
    assert_eq!(shot.text("10,0").color, theme::SHEET_SELECTED);
    // Leeres Papier: nichts gewählt.
    h.click(&mut st, screen(&h, (250.0, 30.0)));
    assert_eq!(st.drawing_ui.selected_dim, None);
    // In die Hüllbox des Textes.
    let shot = h.idle(&mut st);
    let c = shot.text("10,0").rect.center();
    h.click(&mut st, c);
    assert_eq!(st.drawing_ui.selected_dim, Some(w));
    // 5 px neben der Linie trifft noch, 8 px nicht (Grenze 6 px).
    h.click(&mut st, screen(&h, (250.0, 30.0)));
    h.click(&mut st, screen(&h, (100.0, 125.0)) + vec2(0.0, -5.0));
    assert_eq!(st.drawing_ui.selected_dim, Some(w), "5 px");
    h.click(&mut st, screen(&h, (250.0, 30.0)));
    h.click(&mut st, screen(&h, (100.0, 125.0)) + vec2(0.0, -8.0));
    assert_eq!(st.drawing_ui.selected_dim, None, "8 px");
    // Klick in die Ansicht wählt die Ansicht und hebt die Maßwahl auf.
    h.click(&mut st, screen(&h, (100.0, 125.0)));
    h.click(&mut st, screen(&h, (100.0, 100.0)));
    assert_eq!(st.drawing_ui.selected_view, Some(front));
    assert_eq!(st.drawing_ui.selected_dim, None);
}

// ---------------------------------------------------------------------------------------------
// (e), (f), (g)

#[test]
fn dragging_the_dimension_line_changes_only_the_offset() {
    let (mut st, d, front, w, mut h) = box_with_width();
    let v = add_height(&mut st, d, front, 0, 10.0);
    h.idle(&mut st);
    let k = h.k();
    let (model, spawned) = (st.document.model_edit_count(), st.drawing_ui.spawned());
    let before = dim(&st, d, w);
    // Waagerechtes Maß: 20 px nach oben (+ 15 px seitlich, quer zur Normalen) → offset + 20/k.
    h.drag(
        &mut st,
        screen(&h, (100.0, 125.0)).round(),
        vec2(15.0, -20.0),
    );
    let after = dim(&st, d, w);
    assert!(
        (after.offset - (10.0 + 20.0 / k)).abs() < 1e-6,
        "{} vs {}",
        after.offset,
        10.0 + 20.0 / k
    );
    assert_eq!(
        DimSpec {
            offset: before.offset,
            ..after.clone()
        },
        before,
        "nur der Abstand ändert sich"
    );
    assert_eq!(st.drawing_ui.selected_dim, Some(w));
    // Senkrechtes Maß: 20 px nach rechts (+ 7 px nach unten) → offset + 20/k (rechts positiv).
    h.drag(&mut st, screen(&h, (115.0, 100.0)).round(), vec2(20.0, 7.0));
    let vert = dim(&st, d, v);
    assert!(
        (vert.offset - (10.0 + 20.0 / k)).abs() < 1e-6,
        "{} vs {}",
        vert.offset,
        10.0 + 20.0 / k
    );
    // Nach unten ziehen verkleinert den Abstand des waagerechten Maßes wieder.
    let p = screen(&h, (100.0, 125.0 + 20.0 / k));
    h.drag(&mut st, p.round(), vec2(0.0, 40.0));
    let back = dim(&st, d, w);
    assert!(
        (back.offset - (10.0 - 20.0 / k)).abs() < 1e-6,
        "{}",
        back.offset
    );
    assert_eq!(st.document.model_edit_count(), model, "Modell bleibt");
    assert_eq!(st.drawing_ui.spawned(), spawned, "nichts neu berechnet");
    // Die Linie liegt danach auf dem Blatt an der neuen Stelle.
    let shot = h.idle(&mut st);
    let y = 125.0 - 20.0 / k;
    let (a, b) = (screen(&h, (95.0, y)), screen(&h, (105.0, y)));
    assert!(
        shot.segments()
            .iter()
            .any(|(p, _, _)| (close(p[0], a, 0.05) && close(p[1], b, 0.05))
                || (close(p[0], b, 0.05) && close(p[1], a, 0.05)))
    );
}

#[test]
fn dragging_the_text_changes_only_the_text_shift() {
    let (mut st, d, _, w, mut h) = box_with_width();
    let k = h.k();
    let before = dim(&st, d, w);
    let shot = h.idle(&mut st);
    // Ganze Pixel, damit die Bewegung in f32 genau 30 und 12 Pixel ist.
    let c = shot.text("10,0").rect.center();
    h.drag(&mut st, c.round(), vec2(30.0, -12.0));
    let after = dim(&st, d, w);
    assert!(
        (after.text_shift.0 - 30.0 / k).abs() < 1e-6
            && (after.text_shift.1 - 12.0 / k).abs() < 1e-6,
        "{:?} vs ({}, {})",
        after.text_shift,
        30.0 / k,
        12.0 / k
    );
    assert_eq!(
        DimSpec {
            text_shift: before.text_shift,
            ..after
        },
        before,
        "nur die Textlage ändert sich"
    );
    // Der Text steht danach um genau diese Strecke verschoben.
    let moved = h.idle(&mut st).text("10,0").rect.center();
    assert!(
        close(moved, c + vec2(30.0, -12.0), 0.05),
        "{moved:?} vs {c:?}"
    );
}

#[test]
fn leader_drag_changes_angle_and_offset() {
    // Zylinder r 5, h 12; Draufsicht 1:1 in (100, 160): Kreis um (100, 160). Vorschlag: Ø10
    // mit Führungslinie unter 45°, Abstand 10 → Spitze c + u·5, Knick c + u·15.
    let mut st = AppState::new();
    st.create_cylinder(5.0, 12.0).unwrap();
    let d = new_drawing(&mut st);
    let top = add_view(&mut st, d, ViewKind::Top, 100.0, 160.0);
    st.drawing_ui.open(d);
    compute(&mut st);
    let out = suggest_for(&mut st, d, Some(top));
    assert_eq!(out, SuggestOutcome::Added(1), "{}", out.message());
    let id = st.document.drawing(d).unwrap().dimensions[0].id;
    let before = dim(&st, d, id);
    assert_eq!(before.kind, DimKind::Diameter);
    assert_eq!((before.angle, before.offset), (45.0, 10.0));
    let mut h = Harness::new();
    h.idle(&mut st);
    h.idle(&mut st);
    let k = h.k();
    let c = DVec2::new(100.0, 160.0);
    let u = DVec2::new(1.0, 1.0).normalize();
    let grab = c + u * 10.0;
    let knee = c + u * 15.0;
    // Der Knick soll nach c + (0, 20): Winkel 90°, Abstand 20 − 5 = 15.
    let target = c + DVec2::new(0.0, 20.0);
    let dm = target - knee;
    let by = vec2((dm.x * k) as f32, (-dm.y * k) as f32);
    h.drag(&mut st, screen(&h, (grab.x, grab.y)), by);
    let after = dim(&st, d, id);
    assert!((after.angle - 90.0).abs() < 1e-3, "{}", after.angle);
    assert!((after.offset - 15.0).abs() < 1e-3, "{}", after.offset);
    assert_eq!(after.text_shift, (0.0, 0.0));
    // Die Pfeilspitze sitzt jetzt oben auf dem Kreis (100, 165).
    let shot = h.idle(&mut st);
    let tip = screen(&h, (100.0, 165.0));
    assert!(
        shot.polygons()
            .iter()
            .any(|(pts, _)| pts.iter().any(|p| close(*p, tip, 0.05))),
        "Pfeil bei {tip:?}"
    );
}

// ---------------------------------------------------------------------------------------------
// (h)

#[test]
fn dragging_one_member_moves_the_whole_group() {
    use planora_drawing::dimension::{ChainMode, ChainSpec};
    // Sollwert: dasselbe Ziehen an einem einzelnen Maß.
    let want = {
        let (mut st, d, _, w, mut h) = box_with_width();
        h.drag(&mut st, screen(&h, (100.0, 125.0)), vec2(0.0, -20.0));
        dim(&st, d, w).offset
    };
    assert_ne!(want, 10.0);
    for mode in [ChainMode::Chain, ChainMode::Baseline] {
        let (mut st, d, front, w, mut h) = box_with_width();
        let w2 = add_width(&mut st, d, front, 0, 10.0);
        let other = add_height(&mut st, d, front, 0, 10.0);
        st.document
            .update_drawing(d, |dr| {
                for (order, id) in [w, w2].into_iter().enumerate() {
                    let m = dr.dimensions.iter_mut().find(|m| m.id == id).unwrap();
                    m.chain = Some(ChainSpec {
                        group: 1,
                        mode,
                        order: order as u32,
                    });
                }
            })
            .unwrap();
        h.idle(&mut st);
        h.idle(&mut st);
        // Gezogen wird das zweite Glied: Kette auf derselben Linie (y 125), Bezugsmaß eine
        // Stufe höher (125 + 7).
        let y = match mode {
            ChainMode::Chain => 125.0,
            ChainMode::Baseline => 132.0,
        };
        h.drag(&mut st, screen(&h, (100.0, y)), vec2(0.0, -20.0));
        let (a, b) = (dim(&st, d, w).offset, dim(&st, d, w2).offset);
        assert_eq!(a, want, "{mode:?}: gezogen wie ein einzelnes Maß");
        assert_eq!(a, b, "{mode:?}: gemeinsamer Abstand bleibt");
        assert_eq!(
            dim(&st, d, other).offset,
            10.0,
            "{mode:?}: anderes Maß bleibt"
        );
    }
}

#[test]
fn centerline_switch_is_locked_with_a_reason_in_the_isometric_view() {
    let (mut st, d, _, _, mut h) = box_with_width();
    let iso = add_view(&mut st, d, ViewKind::Isometric, 220.0, 100.0);
    compute(&mut st);
    st.drawing_ui.selected_view = Some(iso);
    let edits = st.document.edit_count();
    h.click_text(&mut st, drawing_view::CENTERLINES);
    assert!(
        !st.document
            .drawing(d)
            .unwrap()
            .view(iso)
            .unwrap()
            .centerlines
    );
    assert_eq!(st.document.edit_count(), edits, "gesperrt: keine Änderung");
    // Der Grund steht am gesperrten Schalter.
    let p = h
        .idle(&mut st)
        .pos_of(drawing_view::CENTERLINES)
        .expect("Schalter sichtbar");
    h.frame(&mut st, vec![Event::PointerMoved(p)]);
    let mut text = String::new();
    for _ in 0..30 {
        text = h.idle(&mut st).text;
    }
    assert!(
        text.contains(drawing_view::NO_ISO_CENTERLINES),
        "Erklärung fehlt:\n{text}"
    );
}

#[test]
fn read_only_tab_can_select_but_not_drag_or_toggle_or_suggest() {
    let (mut st, d, front, w, _) = box_with_width();
    let mut h = Harness::read_only();
    h.idle(&mut st);
    h.idle(&mut st);
    let before = st.document.drawing(d).unwrap().clone();
    // Ziehen wählt, ändert aber nichts.
    h.drag(&mut st, screen(&h, (100.0, 125.0)), vec2(0.0, -20.0));
    assert_eq!(st.drawing_ui.selected_dim, Some(w), "Wählen geht");
    assert_eq!(st.document.drawing(d).unwrap(), &before, "nichts gezogen");
    // Text ziehen ebenso.
    let shot = h.idle(&mut st);
    h.drag(&mut st, shot.text("10,0").rect.center(), vec2(30.0, 10.0));
    assert_eq!(st.document.drawing(d).unwrap(), &before);
    // „Maße vorschlagen“ und „Mittellinien“ sind gesperrt.
    h.click_text(&mut st, drawing_view::SUGGEST);
    assert_eq!(st.document.drawing(d).unwrap(), &before);
    assert_eq!(st.drawing_ui.dim_message, None);
    st.drawing_ui.selected_view = Some(front);
    h.click_text(&mut st, drawing_view::CENTERLINES);
    assert_eq!(st.document.drawing(d).unwrap(), &before);
}

// ---------------------------------------------------------------------------------------------
// (i), (j)

/// Quader 10 × 20 × 30 mit Vorderansicht (100, 100) und Draufsicht (100, 170), berechnet.
fn front_and_top(
    compute_fn: Option<planora_app::drawing_state::ComputeFn>,
) -> (AppState, DrawingId, ViewId, ViewId, Harness) {
    let mut st = AppState::new();
    if let Some(f) = compute_fn {
        st.drawing_ui.set_compute(f);
    }
    st.create_box(BOX).unwrap();
    let d = new_drawing(&mut st);
    let front = add_view(&mut st, d, ViewKind::Front, 100.0, 100.0);
    let top = add_view(&mut st, d, ViewKind::Top, 100.0, 170.0);
    st.drawing_ui.open(d);
    compute(&mut st);
    let mut h = Harness::new();
    h.idle(&mut st);
    h.idle(&mut st);
    (st, d, front, top, h)
}

#[test]
fn suggest_button_adds_dimensions_and_reports_the_count() {
    let (mut st, d, _, _, mut h) = front_and_top(None);
    let model = st.document.model_edit_count();
    h.click_text(&mut st, drawing_view::SUGGEST);
    assert_eq!(st.document.drawing(d).unwrap().dimensions.len(), 3);
    let shot = h.idle(&mut st);
    assert!(
        shot.text.contains("3 Maße wurden hinzugefügt."),
        "{}",
        shot.text
    );
    assert_eq!(st.document.model_edit_count(), model);
    // Die neuen Maße sind auf dem Blatt (Breite 10 vorn: Text „10,0“).
    assert!(shot.texts.iter().any(|t| t.text == "10,0"), "{}", shot.text);
    assert_eq!(st.message, None, "keine Fehlermeldung in der Statuszeile");

    // Mit gewählter Ansicht nur diese: Vorderansicht → Breite und Höhe.
    let (mut st, d, front, _, mut h) = front_and_top(None);
    st.drawing_ui.selected_view = Some(front);
    h.click_text(&mut st, drawing_view::SUGGEST);
    let dims = &st.document.drawing(d).unwrap().dimensions;
    assert_eq!(dims.len(), 2);
    assert!(dims.iter().all(|m| m.view == front));
    assert!(h.idle(&mut st).text.contains("2 Maße wurden hinzugefügt."));

    // Eine Ansicht ließ sich nicht berechnen: die Meldung sagt es.
    let (mut st, d, _, top, mut h) = front_and_top(Some(top_fails));
    let name = drawing_view::view_names(st.document.drawing(d).unwrap())[&top].clone();
    h.click_text(&mut st, drawing_view::SUGGEST);
    let text = h.idle(&mut st).text;
    let want = format!(
        "2 Maße wurden hinzugefügt. Die Ansicht „{name}“ ließ sich nicht berechnen und bekam keine Maße."
    );
    assert!(text.contains(&want), "„{want}“ fehlt:\n{text}");
    // Nur diese Ansicht gewählt: die Meldung nennt sie und den Grund.
    st.drawing_ui.selected_view = Some(top);
    h.click_text(&mut st, drawing_view::SUGGEST);
    let text = h.idle(&mut st).text;
    let want = format!(
        "Die Ansicht „{name}“ ließ sich nicht berechnen. {}",
        DrawError::TooComplex(5)
    );
    assert!(text.contains(&want), "„{want}“ fehlt:\n{text}");
}

#[test]
fn suggest_button_reports_nothing_new_on_second_press() {
    let (mut st, d, _, _, mut h) = front_and_top(None);
    h.click_text(&mut st, drawing_view::SUGGEST);
    assert_eq!(st.document.drawing(d).unwrap().dimensions.len(), 3);
    let edits = st.document.edit_count();
    h.click_text(&mut st, drawing_view::SUGGEST);
    assert_eq!(st.document.drawing(d).unwrap().dimensions.len(), 3);
    assert_eq!(st.document.edit_count(), edits, "keine Änderung");
    let text = h.idle(&mut st).text;
    assert!(
        text.contains("Es gibt keine neuen Maße vorzuschlagen."),
        "{text}"
    );
    assert!(!text.contains("hinzugefügt"), "{text}");
}

// ---------------------------------------------------------------------------------------------
// (m)

#[test]
fn dragging_does_not_reflatten_the_lines() {
    let (mut st, d, front, w, mut h) = box_with_width();
    h.idle(&mut st);
    let c0 = drawing_view::flatten_calls();
    h.drag(&mut st, screen(&h, (100.0, 125.0)), vec2(0.0, -20.0));
    let shot = h.idle(&mut st);
    assert_ne!(dim(&st, d, w).offset, 10.0, "gezogen");
    assert_eq!(
        drawing_view::flatten_calls(),
        c0,
        "Maß ziehen zerlegt nichts neu"
    );
    // Text ziehen und Mittellinien umschalten ebenso nicht.
    h.drag(&mut st, shot.text("10,0").rect.center(), vec2(10.0, 0.0));
    st.drawing_ui.selected_view = Some(front);
    h.click_text(&mut st, drawing_view::CENTERLINES);
    assert_eq!(drawing_view::flatten_calls(), c0);
    // Gegenprobe des Zählers: eine Ansicht verschieben ändert die Linien und zerlegt neu.
    h.drag(&mut st, screen(&h, (100.0, 100.0)), vec2(30.0, 0.0));
    assert!(drawing_view::flatten_calls() > c0, "Ansicht verschoben");
}

// ---------------------------------------------------------------------------------------------
// (n)

/// Wörter, die in der Oberfläche nicht vorkommen dürfen (englische Fachwörter).
const ENGLISH: [&str; 34] = [
    "view",
    "views",
    "sheet",
    "scale",
    "hidden",
    "delete",
    "add",
    "fit",
    "zoom",
    "drawing",
    "model",
    "pending",
    "computing",
    "failed",
    "error",
    "paper",
    "projection",
    "angle",
    "create",
    "cancel",
    "ok",
    "loading",
    "dimension",
    "dimensions",
    "centerline",
    "centerlines",
    "suggest",
    "lost",
    "outside",
    "offset",
    "leader",
    "reference",
    "selected",
    "toggle",
];

fn english_words(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .map(str::to_lowercase)
        .filter(|w| ENGLISH.contains(&w.as_str()))
        .collect()
}

#[test]
fn german_texts_only() {
    let mut all = String::new();
    // Werkzeugleiste mit gewählter Ansicht, Meldung des Vorschlags, Hinweise und Tooltip.
    let (mut st, d, front, _, mut h) = lost_state();
    add_height(&mut st, d, front, 0, 5000.0);
    st.drawing_ui.selected_view = Some(front);
    h.click_text(&mut st, drawing_view::SUGGEST);
    let shot = h.idle(&mut st);
    for w in [
        drawing_view::SUGGEST,
        drawing_view::CENTERLINES,
        "Maß hat seinen Bezug verloren",
        "außerhalb des Blatts",
    ] {
        assert!(shot.text.contains(w), "„{w}“ fehlt:\n{}", shot.text);
    }
    assert!(st.drawing_ui.dim_message.is_some());
    all.push_str(&shot.text);
    let sheet = drawing_view::last_sheet(&h.ctx).unwrap();
    let lost_line = sheet.views[0]
        .marks
        .iter()
        .find_map(|m| match m {
            Mark::Line {
                a,
                b,
                kind: MarkKind::Lost,
            } if near(a.y, b.y) => Some((*a + *b) * 0.5),
            _ => None,
        })
        .unwrap();
    h.frame(
        &mut st,
        vec![Event::PointerMoved(screen(&h, (lost_line.x, lost_line.y)))],
    );
    for _ in 0..30 {
        h.idle(&mut st);
    }
    let tip = h.idle(&mut st).text;
    assert!(tip.contains(drawing_view::LOST_TIP), "{tip}");
    all.push_str(&tip);
    // Hover-Texte des Knopfs (mit und ohne gewählte Ansicht) und alle Meldungen des Vorschlags.
    for t in [
        SuggestOutcome::Added(1).message(),
        SuggestOutcome::Added(3).message(),
        SuggestOutcome::NothingNew.message(),
        SuggestOutcome::Partly {
            added: 0,
            skipped: vec![NOT_BOUND.into()],
        }
        .message(),
        VIEWS_PENDING.into(),
        NO_ISOMETRIC.into(),
        NO_NORMAL_VIEW.into(),
    ] {
        all.push_str(&t);
        all.push('\n');
    }
    let (mut st2, _, _, _, mut h2) = front_and_top(None);
    let shot = h2.idle(&mut st2);
    let p = shot.pos_of(drawing_view::SUGGEST).unwrap();
    h2.frame(&mut st2, vec![Event::PointerMoved(p)]);
    let mut hover = String::new();
    for _ in 0..30 {
        hover = h2.idle(&mut st2).text;
    }
    assert!(hover.contains("Setzt die wichtigsten Maße"), "{hover}");
    all.push_str(&hover);
    all.push_str(drawing_view::HINT);
    let bad = english_words(&all);
    assert!(bad.is_empty(), "englische Wörter {bad:?} in:\n{all}");
    assert_eq!(
        english_words("Suggest Dimensions – Lost"),
        ["suggest", "dimensions", "lost"]
    );
}

// ---------------------------------------------------------------------------------------------
// (o)

#[test]
fn dark_theme_keeps_marks_black_on_white_paper() {
    let (mut st, _, _, w, mut h) = box_with_width();
    st.dark = true;
    theme::apply(&h.ctx, true);
    h.idle(&mut st);
    let shot = h.idle(&mut st);
    let paper = Rect::from_two_pos(screen(&h, (0.0, 0.0)), screen(&h, (297.0, 210.0)));
    assert!(
        shot.flat
            .iter()
            .any(|s| matches!(s, Shape::Rect(r) if r.fill == Color32::WHITE
            && (r.rect.min - paper.min).length() < 0.5
            && (r.rect.max - paper.max).length() < 0.5)),
        "weißes Blatt"
    );
    let ink = theme::SHEET_DIM;
    assert!(
        ink.r() < 40 && ink.g() < 40 && ink.b() < 40,
        "schwarz: {ink:?}"
    );
    let (a, b) = (screen(&h, (95.0, 125.0)), screen(&h, (105.0, 125.0)));
    assert!(shot.segments().iter().any(|(p, c, _)| *c == ink
        && ((close(p[0], a, 0.01) && close(p[1], b, 0.01))
            || (close(p[0], b, 0.01) && close(p[1], a, 0.01)))));
    assert!(shot.polygons().iter().any(|(_, c)| *c == ink));
    assert_eq!(shot.text("10,0").color, ink);
    // Gewählt: die kräftige Akzentfarbe, die auf Weiß lesbar ist (auch im dunklen Thema).
    h.click(&mut st, screen(&h, (100.0, 125.0)));
    assert_eq!(st.drawing_ui.selected_dim, Some(w));
    let shot = h.idle(&mut st);
    assert_eq!(shot.text("10,0").color, theme::SHEET_SELECTED);
    assert_eq!(theme::SHEET_SELECTED, theme::accent(false));
}

// ---------------------------------------------------------------------------------------------
// Ergänzungen

#[test]
fn dash_pattern_follows_the_centerline_pattern_across_corners() {
    // 20 mm gerade: 12 Strich, 3 Lücke, 2 Punkt, 3 Lücke.
    let pts = [DVec2::new(0.0, 0.0), DVec2::new(20.0, 0.0)];
    let d = drawing_view::dash_pattern(&pts, &[12.0, 3.0, 2.0, 3.0]);
    let ends: Vec<(f64, f64)> = d.iter().map(|p| (p[0].x, p[p.len() - 1].x)).collect();
    assert_eq!(ends, vec![(0.0, 12.0), (15.0, 17.0)]);
    // Über eine Ecke: 10 + 10 mm, der Strich läuft um die Ecke weiter.
    let pts = [
        DVec2::new(0.0, 0.0),
        DVec2::new(10.0, 0.0),
        DVec2::new(10.0, 10.0),
    ];
    let d = drawing_view::dash_pattern(&pts, &[12.0, 3.0, 2.0, 3.0]);
    assert_eq!(
        d[0],
        vec![
            DVec2::new(0.0, 0.0),
            DVec2::new(10.0, 0.0),
            DVec2::new(10.0, 2.0)
        ]
    );
    assert_eq!(d[1], vec![DVec2::new(10.0, 5.0), DVec2::new(10.0, 7.0)]);
    // Ungültige Muster: ganze Linie.
    assert_eq!(drawing_view::dash_pattern(&pts, &[1.0]).len(), 1);
    assert_eq!(drawing_view::dash_pattern(&pts, &[1.0, -1.0]).len(), 1);
    // `dashes` bleibt wie bisher.
    let pts = [DVec2::new(0.0, 0.0), DVec2::new(10.0, 0.0)];
    let ends: Vec<(f64, f64)> = drawing_view::dashes(&pts, 3.0, 1.0)
        .iter()
        .map(|p| (p[0].x, p[p.len() - 1].x))
        .collect();
    assert_eq!(ends, vec![(0.0, 3.0), (4.0, 7.0), (8.0, 10.0)]);
}

#[test]
fn switching_drawings_clears_the_dimension_selection_and_message() {
    let (mut st, _, _, w, mut h) = box_with_width();
    h.click(&mut st, screen(&h, (100.0, 125.0)));
    assert_eq!(st.drawing_ui.selected_dim, Some(w));
    st.drawing_ui.dim_message = Some("x".into());
    let other = st
        .document
        .add_drawing("Blatt 2", A4_QUER, Projection::First)
        .unwrap();
    st.drawing_ui.open(other);
    assert_eq!(st.drawing_ui.selected_dim, None);
    assert_eq!(st.drawing_ui.dim_message, None);
}

#[test]
fn selecting_a_view_keeps_the_canvas_in_place() {
    // Die Werkzeugleiste hat immer zwei Zeilen: Wählen einer Ansicht (mehr Knöpfe) verschiebt
    // die Blattfläche nicht – sonst spränge das Blatt mitten im Ziehen.
    let (mut st, _, front, _, mut h) = box_with_width();
    h.idle(&mut st);
    let before = h.layout().canvas;
    st.drawing_ui.selected_view = Some(front);
    let shot = h.idle(&mut st);
    assert!(
        shot.text.contains(drawing_view::CENTERLINES),
        "{}",
        shot.text
    );
    assert_eq!(h.layout().canvas, before);
}

// ---------------------------------------------------------------------------------------------
// Fix-Runde 1: Seite beim Ziehen, Drehen, Winkelbogen

#[test]
fn dragging_past_the_part_edge_keeps_the_line_on_its_side() {
    let (mut st, d, front, w, mut h) = box_with_width();
    let v = add_height(&mut st, d, front, 0, 10.0);
    h.idle(&mut st);
    let k = h.k();
    // Waagerecht 15 mm nach unten von Abstand 10: die Linie bliebe sonst 5 mm unter der
    // Oberkante (Seitenwechsel). Sie bleibt über der Oberkante y 115 stehen.
    let by = vec2(0.0, (15.0 * k) as f32);
    h.drag(&mut st, screen(&h, (100.0, 125.0)).round(), by);
    let off = dim(&st, d, w).offset;
    assert_eq!(off, drawing_view::MIN_DRAG_OFFSET_MM);
    let sheet = drawing_view::last_sheet(&h.ctx).unwrap();
    let line_y = sheet.views[0]
        .marks
        .iter()
        .filter_map(|m| match m {
            Mark::Line { a, b, .. } if near(a.y, b.y) && near((a.x - b.x).abs(), 10.0) => Some(a.y),
            _ => None,
        })
        .fold(f64::NAN, f64::max);
    assert!((line_y - 116.0).abs() < 1e-9, "Linie bei y {line_y}");
    // Senkrecht 15 mm nach links von Abstand 10: bleibt rechts der Kante x 105.
    let by = vec2(-(15.0 * k) as f32, 0.0);
    h.drag(&mut st, screen(&h, (115.0, 100.0)).round(), by);
    assert_eq!(dim(&st, d, v).offset, drawing_view::MIN_DRAG_OFFSET_MM);
    // Ein Maß mit negativem Abstand bleibt beim Ziehen nach oben auf der negativen Seite.
    st.document
        .update_dimension(d, w, |m| m.offset = -10.0)
        .unwrap();
    h.idle(&mut st);
    h.drag(
        &mut st,
        screen(&h, (100.0, 75.0)).round(),
        vec2(0.0, -(20.0 * k) as f32),
    );
    assert_eq!(dim(&st, d, w).offset, -drawing_view::MIN_DRAG_OFFSET_MM);
}

#[test]
fn rotating_a_large_diameter_line_changes_only_the_angle() {
    // Zylinder r 15 (Ø30 ≥ 25 auf dem Blatt): Maßlinie durch die Mitte (100, 120) unter 45°.
    let mut st = AppState::new();
    st.create_cylinder(15.0, 12.0).unwrap();
    let d = new_drawing(&mut st);
    let top = add_view(&mut st, d, ViewKind::Top, 100.0, 120.0);
    st.drawing_ui.open(d);
    compute(&mut st);
    let out = suggest_for(&mut st, d, Some(top));
    assert_eq!(out, SuggestOutcome::Added(1), "{}", out.message());
    let id = st.document.drawing(d).unwrap().dimensions[0].id;
    let before = dim(&st, d, id);
    assert_eq!(before.angle, 45.0);
    let mut h = Harness::new();
    h.idle(&mut st);
    h.idle(&mut st);
    let k = h.k();
    // Griff auf der Linie gegenüber dem Text: c − u·10 (Winkel 225°). Zeiger nach c + (−10, 0)
    // (Winkel 180°): Drehung −45° → Winkel 0.
    let c = DVec2::new(100.0, 120.0);
    let u = DVec2::new(1.0, 1.0).normalize();
    let grab = c - u * 10.0;
    let target = c + DVec2::new(-10.0, 0.0);
    let dm = target - grab;
    h.drag(
        &mut st,
        screen(&h, (grab.x, grab.y)),
        vec2((dm.x * k) as f32, (-dm.y * k) as f32),
    );
    let after = dim(&st, d, id);
    assert!(after.angle.abs() < 1e-3, "{}", after.angle);
    assert_eq!(
        DimSpec {
            angle: before.angle,
            ..after
        },
        before,
        "nur der Winkel ändert sich"
    );
    // Die Linie liegt jetzt waagerecht durch die Mitte: Pfeilspitzen bei x 85 und 115.
    let shot = h.idle(&mut st);
    for x in [85.0, 115.0] {
        let tip = screen(&h, (x, 120.0));
        assert!(
            shot.polygons().iter().any(|(p, _)| close(p[0], tip, 0.05)),
            "Spitze bei {tip:?}"
        );
    }
}

/// Linke senkrechte und untere waagerechte Vorderkante (Winkel 90° an der Ecke unten links).
fn add_corner_angle(st: &mut AppState, d: DrawingId, view: ViewId, offset: f64) -> DimId {
    let (lo, _) = st.bodies[0].bounds;
    let refs = vec![
        GeoRef::Edge(edge_where(st, 0, |p| near(p.x, lo.x) && near(p.y, lo.y))),
        GeoRef::Edge(edge_where(st, 0, |p| near(p.z, lo.z) && near(p.y, lo.y))),
    ];
    st.document
        .add_dimension(d, spec(view, DimKind::Angle, refs, offset))
        .unwrap()
}

#[test]
fn angle_arc_is_painted_within_a_pixel_and_radial_drag_changes_the_radius() {
    // Quader 10 × 20 × 30, Vorderansicht (100, 100): Ecke unten links (95, 85). Winkelmaß mit
    // Radius 10 → Bogen von 0 bis π/2 um die Ecke.
    let (mut st, d, front, _, mut h) = box_with_width();
    let a = add_corner_angle(&mut st, d, front, 10.0);
    h.idle(&mut st);
    let shot = h.idle(&mut st);
    let k = h.k();
    let vertex = DVec2::new(95.0, 85.0);
    let sheet = drawing_view::last_sheet(&h.ctx).unwrap();
    let arc = sheet.views[0]
        .marks
        .iter()
        .find_map(|m| match m {
            Mark::Arc {
                center,
                r,
                start,
                end,
                ..
            } => Some((*center, *r, *start, *end)),
            _ => None,
        })
        .expect("Bogen");
    assert!(
        (arc.0 - vertex).length() < 1e-9 && near(arc.1, 10.0),
        "{arc:?}"
    );
    assert!(near(arc.3 - arc.2, std::f64::consts::FRAC_PI_2), "{arc:?}");
    // Gezeichnet als Polylinie: Ecken auf dem Kreis, Sehnenmitten höchstens 1 px innen.
    let vc = screen(&h, (vertex.x, vertex.y));
    let r_px = (10.0 * k) as f32;
    let poly = shot
        .paths()
        .into_iter()
        .find(|(pts, c, _)| {
            *c == theme::SHEET_DIM
                && pts.len() >= 4
                && pts.iter().all(|p| ((*p - vc).length() - r_px).abs() < 0.05)
        })
        .expect("Bogen als Polylinie");
    for w in poly.0.windows(2) {
        let mid = w[0] + (w[1] - w[0]) * 0.5;
        assert!(r_px - (mid - vc).length() <= 1.0, "Abweichung");
    }
    // Ziehen am Bogen (Mitte unter 45°) um 5 mm nach außen → Radius 15.
    let grab = vertex + DVec2::new(1.0, 1.0).normalize() * 10.0;
    let out = DVec2::new(1.0, 1.0).normalize() * 5.0;
    h.drag(
        &mut st,
        screen(&h, (grab.x, grab.y)),
        vec2((out.x * k) as f32, (-out.y * k) as f32),
    );
    let after = dim(&st, d, a);
    assert!((after.offset - 15.0).abs() < 1e-3, "{}", after.offset);
    assert_eq!(after.text_shift, (0.0, 0.0));
}
