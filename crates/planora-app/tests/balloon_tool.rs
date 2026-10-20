//! Werkzeug „Position“ (Teilprojekt 3c-2, Aufgabe 7): Positionsnummern setzen, ziehen, wählen,
//! löschen; verlorene Kreise; Nummern nach der Stückliste; Sperren – mit echten egui-Ereignissen
//! gegen den Zeichnungsbereich (`drawing_view::show_with`, ohne Shell) bzw. die ganze App.
//!
//! Sollwerte von Hand: A4 quer hat den Blattfaktor f = 297 / 420; der Vorgabeversatz ist
//! (10, 10) · f, der Kreisradius 4 · f, die Kreislinie 0,35 · f (nie unter 0,18) und die
//! Hinweislinie 0,25 · f (nie unter 0,18) Millimeter. Quader 10 × 20 × 30, Vorderansicht 1:1 in
//! (100, 100): Kanten x 95 … 105, y 85 … 115.

mod common;

use eframe::App as _;
use egui::epaint::{ColorMode, Shape};
use egui::{Color32, Event, Key, Modifiers, PointerButton, Pos2, Rect, Vec2, pos2, vec2};
use planora_app::app::PlanoraApp;
use planora_app::balloon_tool::{
    self, HINT, LOST_TIP, MISSED, PARTS_SHEET_TIP, READ_ONLY_TIP, TOOL, TOOL_TIP, lost_note,
};
use planora_app::browser::error_color;
use planora_app::dimension_tool::{self, NO_VIEWS_TIP, ToolMode};
use planora_app::drawing_dialog::{NewDrawingDialog, SKETCH_BLOCKED};
use planora_app::drawing_state::WorkArea;
use planora_app::drawing_view;
use planora_app::state::{AppState, StandardPlane};
use planora_app::workspace::Workspace;
use planora_app::{fonts, icons, shell_panels, theme, viewport};
use planora_drawing::annotate::{Mark, MarkKind};
use planora_drawing::balloon::{BalloonMarks, PositionPen};
use planora_drawing::out2d::Out2d;
use planora_drawing::sheet::SheetLine;
use planora_drawing::sheet_info::{FieldValue, Stamp};
use planora_drawing::stroke::StrokeOrigin;
use planora_drawing::{
    BalloonId, BodyKey, DrawingId, Paper, Placement, Projection, Scale, Sheet, ViewId, ViewKind,
    Visibility,
};
use planora_feature::{FeatureId, FeatureKind, SketchFeature, SketchPlane, TOO_MANY_BALLOONS};
use planora_math::{DVec2, DVec3, Frame, Tolerance};
use planora_project::config::Config;
use planora_project::library::Library;
use planora_sketch::model::Sketch;

const BIG: Vec2 = vec2(1400.0, 900.0);
const SMALL: Vec2 = vec2(800.0, 600.0);
const MID: Vec2 = vec2(1100.0, 700.0);

const A4_QUER: Sheet = Sheet {
    paper: Paper::A4,
    landscape: true,
};
const A0_QUER: Sheet = Sheet {
    paper: Paper::A0,
    landscape: true,
};

/// Blattfaktor von A4 quer.
const F_A4: f64 = 297.0 / 420.0;

fn s11() -> Scale {
    Scale { num: 1, den: 1 }
}

fn context() -> egui::Context {
    let c = egui::Context::default();
    c.set_fonts(fonts::definitions());
    c.all_styles_mut(|s| s.interaction.tooltip_delay = 0.0);
    c
}

// ---- Bild: Texte, Linienzüge, Strecken -------------------------------------------------------

#[derive(Clone, Debug)]
struct Seen {
    text: String,
    rect: Rect,
    clip: Rect,
    color: Color32,
}

#[derive(Clone, Debug)]
struct Painted {
    points: Vec<Pos2>,
    width: f32,
    color: Color32,
}

#[derive(Default)]
struct Shot {
    texts: Vec<Seen>,
    paths: Vec<Painted>,
    segs: Vec<Painted>,
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
        Shape::Path(p) => {
            if let ColorMode::Solid(c) = p.stroke.color {
                s.paths.push(Painted {
                    points: p.points.clone(),
                    width: p.stroke.width,
                    color: c,
                });
            }
        }
        Shape::LineSegment { points, stroke } => s.segs.push(Painted {
            points: points.to_vec(),
            width: stroke.width,
            color: stroke.color,
        }),
        Shape::Rect(r) => s.fills.push((r.rect, r.fill)),
        _ => {}
    }
}

impl Shot {
    fn of(out: &egui::FullOutput) -> Shot {
        let mut s = Shot::default();
        for c in &out.shapes {
            walk(&c.shape, c.clip_rect, &mut s);
        }
        s
    }

    fn has(&self, exact: &str) -> bool {
        self.texts.iter().any(|t| t.text == exact)
    }

    fn find(&self, exact: &str) -> &Seen {
        self.texts
            .iter()
            .find(|t| t.text == exact)
            .unwrap_or_else(|| {
                panic!(
                    "„{exact}“ nicht sichtbar: {:?}",
                    self.texts.iter().map(|t| &t.text).collect::<Vec<_>>()
                )
            })
    }

    /// Geschlossene Linienzüge (Kreise) um `c` mit Radius `r` Pixel.
    fn circles(&self, c: Pos2, r: f32) -> Vec<&Painted> {
        self.paths
            .iter()
            .filter(|p| {
                p.points.len() >= 4 && p.points.iter().all(|q| ((*q - c).length() - r).abs() < 1.0)
            })
            .collect()
    }
}

// ---- Zeichnungsbereich ohne Shell ------------------------------------------------------------

struct Harness {
    ctx: egui::Context,
    time: f64,
    size: Vec2,
    editable: bool,
    /// Bediennamen der angeklickten Bedienelemente (in der Reihenfolge der Klicks).
    clicked: Vec<String>,
}

impl Harness {
    fn new(size: Vec2) -> Harness {
        Harness {
            ctx: context(),
            time: 0.0,
            size,
            editable: true,
            clicked: Vec::new(),
        }
    }

    fn frame(&mut self, st: &mut AppState, events: Vec<Event>) -> Shot {
        // Mehr als die Doppelklick-Zeit zwischen den Bildern: jeder Klick zählt einzeln.
        self.frame_dt(st, events, 0.2)
    }

    fn frame_dt(&mut self, st: &mut AppState, events: Vec<Event>, dt: f64) -> Shot {
        self.time += dt;
        let mut all = vec![Event::ModifiersChanged(Modifiers::NONE)];
        all.extend(events);
        let raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, self.size)),
            events: all,
            time: Some(self.time),
            ..Default::default()
        };
        let editable = self.editable;
        let out = self
            .ctx
            .run_ui(raw, |ui| drawing_view::show_with(ui, st, editable));
        let shot = Shot::of(&out);
        for e in &out.platform_output.events {
            if let egui::output::OutputEvent::Clicked(info) = e {
                self.clicked.push(info.label.clone().unwrap_or_default());
            }
        }
        out.drop_without_applying_deltas();
        shot
    }

    fn idle(&mut self, st: &mut AppState) -> Shot {
        self.frame(st, vec![]);
        self.frame(st, vec![])
    }

    fn click(&mut self, st: &mut AppState, p: Pos2) -> Shot {
        self.frame(st, vec![Event::PointerMoved(p)]);
        self.frame(st, vec![button(p, true)]);
        self.frame(st, vec![button(p, false)]);
        self.idle(st)
    }

    fn click_text(&mut self, st: &mut AppState, exact: &str) -> Shot {
        let shot = self.idle(st);
        let p = shot.find(exact).rect.center();
        self.click(st, p)
    }

    fn click_at(&mut self, st: &mut AppState, p: DVec2) -> Shot {
        let pos = self.screen(p);
        self.click(st, pos)
    }

    fn hover(&mut self, st: &mut AppState, p: Pos2) -> Shot {
        self.frame(st, vec![Event::PointerMoved(p)]);
        self.frame(st, vec![Event::PointerMoved(p)]);
        self.frame(st, vec![Event::PointerMoved(p)])
    }

    /// Drückt bei `from`, zieht in vier Schritten um `by` und lässt los; vor dem Loslassen wird
    /// `mid` mit dem Zustand aufgerufen (Ziehen läuft noch).
    fn drag(
        &mut self,
        st: &mut AppState,
        from: Pos2,
        by: Vec2,
        mut mid: impl FnMut(&mut AppState, &Harness),
    ) {
        self.frame_dt(st, vec![Event::PointerMoved(from)], 1.0);
        self.frame_dt(st, vec![button(from, true)], 0.02);
        for f in [0.25, 0.5, 0.75, 1.0] {
            self.frame_dt(st, vec![Event::PointerMoved(from + by * f)], 0.02);
        }
        mid(st, self);
        self.frame_dt(st, vec![button(from + by, false)], 0.02);
        self.idle(st);
    }

    fn key(&mut self, st: &mut AppState, key: Key) -> Shot {
        let ev = |pressed| Event::Key {
            key,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers: Modifiers::NONE,
        };
        self.frame(st, vec![ev(true)]);
        self.frame(st, vec![ev(false)]);
        self.idle(st)
    }

    fn layout(&self) -> drawing_view::DrawingLayout {
        drawing_view::last_layout(&self.ctx).expect("kein Zeichnungsbereich gezeichnet")
    }

    fn k(&self) -> f64 {
        f64::from(self.layout().px_per_mm)
    }

    fn screen(&self, p: DVec2) -> Pos2 {
        let l = self.layout();
        drawing_view::sheet_to_screen(l.canvas, l.sheet_mm, l.zoom, l.pan, p)
    }

    /// Gezeichneter Kreis `b` (aus der letzten Aufteilung).
    fn balloon(&self, b: BalloonId) -> BalloonMarks {
        self.layout()
            .balloons
            .into_iter()
            .find(|x| x.id == b)
            .expect("Kreis gezeichnet")
    }

    fn lines(&self, view: ViewId) -> Vec<SheetLine> {
        drawing_view::last_sheet(&self.ctx)
            .expect("Blatt gezeichnet")
            .views
            .iter()
            .find(|v| v.id == view)
            .expect("Ansicht gezeichnet")
            .lines
            .clone()
    }

    /// Mitte einer Blattlinie der Ansicht, für die `pred` gilt.
    fn line_mid(&self, view: ViewId, what: &str, pred: impl Fn(&SheetLine) -> bool) -> DVec2 {
        let lines = self.lines(view);
        let l = lines
            .iter()
            .filter(|l| pred(l))
            // Die längste: weit weg von den anderen Linien.
            .max_by(|a, b| a.curve.length().total_cmp(&b.curve.length()))
            .unwrap_or_else(|| panic!("keine Linie „{what}“: {lines:?}"));
        mid(&l.curve)
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

fn mid(c: &Out2d) -> DVec2 {
    match c {
        Out2d::Line { a, b } => (*a + *b) * 0.5,
        Out2d::Arc { start, end, .. } => c.point(0.5 * (start + end)),
        other => panic!("unerwartete Kurve {other:?}"),
    }
}

fn body_of(l: &SheetLine) -> usize {
    match l.origin {
        StrokeOrigin::Edge { body, .. } | StrokeOrigin::Silhouette { body, .. } => body,
    }
}

fn add_view(st: &mut AppState, d: DrawingId, kind: ViewKind, x: f64, y: f64) -> ViewId {
    st.document
        .add_view(d, kind, s11(), Placement::Free { x, y }, None)
        .unwrap()
}

fn new_drawing(st: &mut AppState, sheet: Sheet) -> DrawingId {
    st.document
        .add_drawing("Blatt 1", sheet, Projection::First)
        .unwrap()
}

fn open(st: &mut AppState, d: DrawingId, size: Vec2) -> Harness {
    st.drawing_ui.open(d);
    st.drawing_ui.tick(&st.bodies, &st.document, true);
    let mut h = Harness::new(size);
    h.idle(st);
    h.idle(st);
    h
}

fn compute(st: &mut AppState, h: &mut Harness) {
    st.drawing_ui.tick(&st.bodies, &st.document, true);
    h.idle(st);
    h.idle(st);
}

fn balloons(st: &AppState, d: DrawingId) -> Vec<planora_drawing::Balloon> {
    st.document.drawing(d).unwrap().balloons.clone()
}

fn balloon_of(st: &AppState, d: DrawingId, b: BalloonId) -> Option<planora_drawing::Balloon> {
    st.document.drawing(d).unwrap().balloon(b).cloned()
}

fn key(f: FeatureId) -> BodyKey {
    BodyKey {
        feature: f.0,
        index: 0,
    }
}

/// Quader 10 × 20 × 30, Vorderansicht 1:1 in (100, 100) auf A4 quer.
fn box_front(size: Vec2) -> (AppState, FeatureId, DrawingId, ViewId, Harness) {
    let mut st = AppState::new();
    let f = st.create_box(DVec3::new(10.0, 20.0, 30.0)).unwrap();
    let d = new_drawing(&mut st, A4_QUER);
    let v = add_view(&mut st, d, ViewKind::Front, 100.0, 100.0);
    let h = open(&mut st, d, size);
    (st, f, d, v, h)
}

/// L-Profil (Skizze in xy, Höhe 5): in der Vorderansicht ist die innere Stufe verdeckt.
fn l_profile(st: &mut AppState) -> FeatureId {
    let mut sk = Sketch::new(Frame::WORLD, "Skizze", Tolerance::default());
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
    let s = st
        .add_feature(
            Some("Skizze"),
            FeatureKind::Sketch(SketchFeature {
                sketch: sk,
                plane: SketchPlane::Standard(StandardPlane::XY),
                copies: vec![],
                formulas: Default::default(),
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
    .unwrap()
}

/// Setzt mit eingeschaltetem Werkzeug einen Kreis per Klick auf `p` und liefert ihn.
fn place_by_click(
    st: &mut AppState,
    h: &mut Harness,
    d: DrawingId,
    p: DVec2,
) -> planora_drawing::Balloon {
    let before = balloons(st, d).len();
    let e0 = st.document.edit_count();
    h.click_at(st, p);
    let all = balloons(st, d);
    assert_eq!(
        all.len(),
        before + 1,
        "kein Kreis gesetzt bei {p:?}: {:?}",
        st.balloon_tool.message()
    );
    assert_eq!(st.document.edit_count(), e0 + 1, "eine Änderung je Kreis");
    all.last().unwrap().clone()
}

fn near(a: f64, b: f64, eps: f64) -> bool {
    (a - b).abs() <= eps
}

fn assert_default_balloon(b: &planora_drawing::Balloon, view: ViewId, body: BodyKey) {
    assert_eq!(b.view, view);
    assert_eq!(b.body, body);
    assert!(
        (0.0..=1.0).contains(&b.anchor.0) && (0.0..=1.0).contains(&b.anchor.1),
        "{:?}",
        b.anchor
    );
    assert!(near(b.offset.0, 10.0 * F_A4, 1e-12), "{:?}", b.offset);
    assert!(near(b.offset.1, 10.0 * F_A4, 1e-12), "{:?}", b.offset);
}

/// Der Kreis ist gemalt: Kreislinie mit Radius 4 · f um seine Mitte und die Nummer `text`
/// darin.
fn assert_painted(h: &Harness, shot: &Shot, b: BalloonId, text: &str) {
    let bm = h.balloon(b);
    assert!(near(bm.radius, 4.0 * F_A4, 1e-12));
    let c = h.screen(bm.centre);
    let r = (bm.radius * h.k()) as f32;
    assert!(
        !shot.circles(c, r).is_empty(),
        "kein Kreis um {c:?} mit r {r}"
    );
    assert!(
        shot.texts
            .iter()
            .any(|t| t.text == text && t.rect.expand(2.0).contains(c)),
        "Nummer „{text}“ nicht im Kreis"
    );
}

// (a) -----------------------------------------------------------------------------------------

#[test]
fn click_sets_balloon_on_every_line_kind() {
    for size in [BIG, SMALL] {
        // Sichtbare und verdeckte Kante: L-Profil, Vorderansicht.
        let mut st = AppState::new();
        let fl = l_profile(&mut st);
        let d = new_drawing(&mut st, A4_QUER);
        let front = add_view(&mut st, d, ViewKind::Front, 100.0, 100.0);
        st.document
            .update_drawing(d, |dr| dr.views[0].show_hidden = true)
            .unwrap();
        let mut h = open(&mut st, d, size);
        if size == BIG {
            h.click_text(&mut st, TOOL);
        } else {
            // Bei 800 × 600 rollt das Werkzeugfeld; der Knopf liegt rechts außerhalb.
            st.balloon_tool.set_active(true);
        }
        assert!(st.balloon_tool.is_active(), "{size:?}");
        let shot = h.idle(&mut st);
        assert!(shot.has(HINT), "{size:?}: Hinweis fehlt");
        let mut b0 = None;
        for (what, vis) in [
            ("sichtbare Kante", Visibility::Visible),
            ("verdeckte Kante", Visibility::Hidden),
        ] {
            let p = h.line_mid(front, what, |l| {
                l.vis == vis
                    && matches!(l.origin, StrokeOrigin::Edge { .. })
                    && matches!(l.curve, Out2d::Line { .. })
            });
            let b = place_by_click(&mut st, &mut h, d, p);
            b0.get_or_insert(b.id);
            assert_default_balloon(&b, front, key(fl));
            let shot = h.idle(&mut st);
            assert_painted(&h, &shot, b.id, "1");
        }
        // Klick daneben: Meldung, nichts gesetzt.
        let n = balloons(&st, d).len();
        let e0 = st.document.edit_count();
        let shot = h.click_at(&mut st, DVec2::new(30.0, 30.0));
        assert_eq!(balloons(&st, d).len(), n);
        assert_eq!(st.document.edit_count(), e0);
        assert_eq!(st.balloon_tool.message(), Some(MISSED));
        let m = shot.find(MISSED);
        assert_eq!(m.color, error_color(st.dark), "Meldung rot");
        // Der nächste Treffer nimmt die Meldung weg (bei 800 × 600 läge der Klick schon im
        // Fangbereich eines Kreises).
        if size == BIG {
            let p = h.line_mid(front, "verdeckte Kante", |l| l.vis == Visibility::Hidden);
            let extra = place_by_click(&mut st, &mut h, d, p);
            assert_eq!(st.balloon_tool.message(), None);
            assert!(!h.idle(&mut st).has(MISSED));
            balloon_tool::delete(&mut st, d, extra.id);
            // Ebenso ein Klick, der einen Kreis wählt.
            h.click_at(&mut st, DVec2::new(30.0, 30.0));
            assert_eq!(st.balloon_tool.message(), Some(MISSED));
            let b0 = b0.expect("erster Kreis");
            let c = h.screen(h.balloon(b0).centre);
            h.click(&mut st, c);
            assert_eq!(st.drawing_ui.selected_balloon, Some(b0));
            assert_eq!(st.balloon_tool.message(), None);
        }

        // Umriss und Kreisbogen: Zylinder, Vorder- und Draufsicht.
        let mut st = AppState::new();
        let fc = st.create_cylinder(5.0, 12.0).unwrap();
        let d = new_drawing(&mut st, A4_QUER);
        let front = add_view(&mut st, d, ViewKind::Front, 80.0, 100.0);
        let top = add_view(&mut st, d, ViewKind::Top, 200.0, 100.0);
        let mut h = open(&mut st, d, size);
        st.balloon_tool.set_active(true);
        h.idle(&mut st);
        let p = h.line_mid(front, "Umriss", |l| {
            matches!(l.origin, StrokeOrigin::Silhouette { .. })
        });
        let b = place_by_click(&mut st, &mut h, d, p);
        assert_default_balloon(&b, front, key(fc));
        let p = h.line_mid(top, "Kreisbogen", |l| {
            matches!(l.origin, StrokeOrigin::Edge { .. }) && matches!(l.curve, Out2d::Arc { .. })
        });
        let b = place_by_click(&mut st, &mut h, d, p);
        assert_default_balloon(&b, top, key(fc));
        let shot = h.idle(&mut st);
        assert_painted(&h, &shot, b.id, "1");
        // Die Hinweislinie endet am Anker auf der getroffenen Linie (Punkt am Teil).
        let bm = h.balloon(b.id);
        let at = bm.centre - DVec2::new(b.offset.0, b.offset.1);
        assert!(at.distance(p) < 6.0 / h.k() + 1e-9, "{at:?} {p:?}");
    }
}

// (b) -----------------------------------------------------------------------------------------

#[test]
fn drag_moves_circle_only() {
    for size in [BIG, SMALL] {
        let (mut st, _, d, _, mut h) = box_front(size);
        st.balloon_tool.set_active(true);
        h.idle(&mut st);
        let b = place_by_click(&mut st, &mut h, d, DVec2::new(95.0, 100.0));
        // Ohne Werkzeug, dann mit Werkzeug: jedes Mal wandert nur der Kreis.
        for tool in [false, true] {
            st.balloon_tool.set_active(tool);
            h.idle(&mut st);
            let before = balloons(&st, d)[0].clone();
            let centre = h.balloon(b.id).centre;
            let k = h.k();
            let e0 = st.document.edit_count();
            let from = h.screen(centre);
            h.drag(&mut st, from, vec2(40.0, -20.0), |st, h| {
                // Während des Ziehens: Dokument unverändert, der Kreis folgt dem Zeiger.
                assert_eq!(st.document.edit_count(), e0, "keine Änderung je Bild");
                assert_eq!(balloons(st, d)[0], before);
                let now = h.balloon(b.id).centre;
                assert!(near(now.x - centre.x, 40.0 / k, 1e-4), "{size:?} {now:?}");
                assert!(near(now.y - centre.y, 20.0 / k, 1e-4), "{size:?} {now:?}");
            });
            let after = balloons(&st, d)[0].clone();
            assert_eq!(st.document.edit_count(), e0 + 1, "eine Änderung je Ziehen");
            assert_eq!(after.anchor, before.anchor, "Anker bleibt");
            assert!(near(after.offset.0 - before.offset.0, 40.0 / k, 1e-4));
            assert!(near(after.offset.1 - before.offset.1, 20.0 / k, 1e-4));
            assert_eq!(st.drawing_ui.selected_balloon, Some(b.id));
            assert_eq!(st.balloon_tool.is_active(), tool);
            assert_eq!(balloons(&st, d).len(), 1, "Ziehen setzt keinen Kreis");
        }
        // Weit über den Rand gezogen: die Kreismitte bleibt auf dem Papier.
        st.balloon_tool.set_active(false);
        st.drawing_ui.zoom = 0.25;
        h.idle(&mut st);
        let from = h.screen(h.balloon(b.id).centre);
        h.drag(&mut st, from, vec2(350.0, 250.0), |_, _| {});
        let c = h.balloon(b.id).centre;
        assert!(near(c.x, 297.0, 1e-9) && near(c.y, 0.0, 1e-9), "{c:?}");
        let from = h.screen(c);
        h.drag(&mut st, from, vec2(-450.0, -300.0), |_, _| {});
        let c = h.balloon(b.id).centre;
        assert!(near(c.x, 0.0, 1e-9) && near(c.y, 210.0, 1e-9), "{c:?}");
    }
}

/// Ein Kreis wird vor der Ansicht und dem Maß getroffen; bis 6 px außerhalb des Kreisrands.
#[test]
fn balloon_hit_reaches_pick_px_outside_the_circle() {
    let (mut st, _, d, front, mut h) = box_front(BIG);
    st.balloon_tool.set_active(true);
    h.idle(&mut st);
    let b = place_by_click(&mut st, &mut h, d, DVec2::new(95.0, 100.0));
    st.balloon_tool.set_active(false);
    st.drawing_ui.selected_balloon = None;
    h.idle(&mut st);
    let bm = h.balloon(b.id);
    let c = h.screen(bm.centre);
    let r = (bm.radius * h.k()) as f32;
    let to_screen = |p: DVec2| h.screen(p);
    let list = [bm.clone()];
    assert_eq!(
        balloon_tool::balloon_hit(&list, &to_screen, c + vec2(r + 5.5, 0.0)),
        Some(b.id)
    );
    assert_eq!(
        balloon_tool::balloon_hit(&list, &to_screen, c + vec2(0.0, -(r + 6.5))),
        None
    );
    assert_eq!(
        balloon_tool::balloon_hit(&list, &to_screen, c + vec2(-(r + 6.2), 0.0)),
        None,
        "knapp über PICK_PX"
    );
    // Im Bild: Klick knapp außerhalb wählt den Kreis, weiter weg die Ansicht.
    h.click(&mut st, c + vec2(r + 5.0, 0.0));
    assert_eq!(st.drawing_ui.selected_balloon, Some(b.id));
    assert_eq!(st.drawing_ui.selected_view, None);
    let left_edge = h.screen(DVec2::new(95.0, 90.0));
    h.click(&mut st, left_edge);
    assert_eq!(st.drawing_ui.selected_balloon, None);
    assert_eq!(st.drawing_ui.selected_view, Some(front));
}

// (c) -----------------------------------------------------------------------------------------

#[test]
fn select_delete_escape() {
    let (mut st, _, d, _, mut h) = box_front(BIG);
    h.click_text(&mut st, TOOL);
    let b = place_by_click(&mut st, &mut h, d, DVec2::new(95.0, 100.0));
    // Mit Werkzeug: Klick auf den Kreis wählt ihn und setzt keinen neuen.
    h.click_at(&mut st, DVec2::new(30.0, 30.0));
    assert_eq!(
        st.drawing_ui.selected_balloon,
        Some(b.id),
        "Fehlklick ändert nichts"
    );
    st.drawing_ui.selected_balloon = None;
    let e0 = st.document.edit_count();
    let c = h.screen(h.balloon(b.id).centre);
    h.click(&mut st, c);
    assert_eq!(st.drawing_ui.selected_balloon, Some(b.id));
    assert_eq!(balloons(&st, d).len(), 1);
    assert_eq!(st.document.edit_count(), e0);
    // Esc beendet das Werkzeug; der Kreis bleibt.
    h.key(&mut st, Key::Escape);
    assert!(!st.balloon_tool.is_active());
    assert_eq!(balloons(&st, d).len(), 1);
    // Leerer Klick hebt die Auswahl auf; Entf ohne Auswahl löscht nichts.
    h.click_at(&mut st, DVec2::new(30.0, 30.0));
    assert_eq!(st.drawing_ui.selected_balloon, None);
    let e0 = st.document.edit_count();
    h.key(&mut st, Key::Delete);
    assert_eq!(balloons(&st, d).len(), 1);
    assert_eq!(st.document.edit_count(), e0);
    // Klick auf den Kreis wählt ihn (ohne Werkzeug), Esc hebt die Auswahl auf.
    let c = h.screen(h.balloon(b.id).centre);
    h.click(&mut st, c);
    assert_eq!(st.drawing_ui.selected_balloon, Some(b.id));
    h.key(&mut st, Key::Escape);
    assert_eq!(st.drawing_ui.selected_balloon, None);
    // Wählen und Entf: gelöscht, eine Änderung.
    h.click(&mut st, c);
    assert_eq!(st.drawing_ui.selected_balloon, Some(b.id));
    let e0 = st.document.edit_count();
    h.key(&mut st, Key::Delete);
    assert!(balloons(&st, d).is_empty());
    assert_eq!(st.document.edit_count(), e0 + 1);
    assert_eq!(st.drawing_ui.selected_balloon, None);
    assert!(
        !balloon_tool::delete_selected(&mut st),
        "nichts mehr gewählt"
    );
}

// ---- Ganze App -------------------------------------------------------------------------------

thread_local! {
    static TIME: std::cell::Cell<f64> = const { std::cell::Cell::new(0.0) };
}

fn input(size: Vec2, events: Vec<Event>) -> egui::RawInput {
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
    let root = common::temp_root("planora-balloon-tool");
    Workspace::new(Some(Library::open(&root).unwrap()), Config::default())
}

struct App {
    c: egui::Context,
    app: PlanoraApp,
    size: Vec2,
    /// Texte vor dem Öffnen des Fensters (sie gehören nicht zum Fenster).
    base: Vec<(String, Rect)>,
}

impl App {
    fn new(size: Vec2, mut st: AppState) -> App {
        let c = context();
        theme::apply(&c, true);
        let mut app = PlanoraApp::for_tests(library_ws());
        st.drawing_ui.tick(&st.bodies, &st.document, true);
        app.ws_mut().active_tab_mut().state = st;
        App {
            c,
            app,
            size,
            base: Vec::new(),
        }
    }

    fn st(&mut self) -> &mut AppState {
        &mut self.app.ws_mut().active_tab_mut().state
    }

    fn frame(&mut self, events: Vec<Event>) -> Shot {
        let mut f = eframe::Frame::_new_kittest();
        let out = self
            .c
            .run_ui(input(self.size, events), |ui| self.app.ui(ui, &mut f));
        let shot = Shot::of(&out);
        out.drop_without_applying_deltas();
        shot
    }

    fn settle(&mut self) -> Shot {
        for _ in 0..3 {
            self.frame(vec![]);
        }
        self.frame(vec![])
    }

    fn click(&mut self, p: Pos2) -> Shot {
        self.frame(vec![Event::PointerMoved(p)]);
        self.frame(vec![Event::PointerMoved(p), button(p, true)]);
        self.frame(vec![button(p, false)]);
        self.settle()
    }

    fn typed(&mut self, text: &str) {
        self.frame(vec![Event::Text(text.to_string())]);
        self.settle();
    }

    fn key(&mut self, key: Key) {
        let ev = |pressed| Event::Key {
            key,
            physical_key: Some(key),
            pressed,
            repeat: false,
            modifiers: Modifiers::NONE,
        };
        self.frame(vec![ev(true)]);
        self.frame(vec![ev(false)]);
        self.settle();
    }

    fn dialog_rect(&mut self) -> Rect {
        self.st()
            .sheet_dialog
            .as_ref()
            .and_then(|d| d.last_rect)
            .expect("Fenster offen")
    }

    fn wheel(&mut self, dy: f32) {
        let at = self.dialog_rect().center();
        self.frame(vec![Event::PointerMoved(at)]);
        self.frame(vec![
            Event::PointerMoved(at),
            Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: vec2(0.0, dy),
                phase: egui::TouchPhase::Move,
                modifiers: Modifiers::NONE,
            },
        ]);
        for _ in 0..8 {
            self.frame(vec![]);
        }
    }

    /// Rollt das Fenster von oben, bis `text` ganz im Fenster zu sehen ist.
    fn scroll_to(&mut self, text: &str) -> Rect {
        self.settle();
        for _ in 0..3 {
            self.wheel(4000.0);
        }
        for _ in 0..80 {
            let shot = self.settle();
            let rect = self.dialog_rect();
            let base = &self.base;
            if let Some(s) = shot.texts.iter().find(|t| {
                t.text == text
                    && !base
                        .iter()
                        .any(|(b, r)| *b == t.text && (r.min - t.rect.min).length() < 0.5)
                    && rect.contains_rect(t.rect)
                    && t.clip.expand(0.5).contains_rect(t.rect)
            }) {
                return s.rect;
            }
            self.wheel(-150.0);
        }
        panic!("„{text}“ nie zu sehen")
    }
}

/// Entf wirkt nur auf das Blatt: nicht bei offenem Fenster „Blattangaben“ (ohne Feld mit
/// Fokus) und nicht, wenn zuletzt eine Browserzeile angeklickt wurde.
#[test]
fn delete_key_only_reaches_the_sheet() {
    let (st, f, d, front, _) = box_front(BIG);
    let mut st = st;
    let b = st
        .document
        .add_balloon(d, front, key(f), (0.0, 0.5), (-15.0, 10.0))
        .unwrap();
    let mut a = App::new(BIG, st);
    a.settle();
    let count = |a: &mut App| a.st().document.drawing(d).unwrap().balloons.len();
    // Fenster offen, kein Feld hat die Tastatur.
    a.st().drawing_ui.selected_balloon = Some(b);
    planora_app::sheet_dialog::open(a.st(), d);
    a.settle();
    assert!(!viewport::typing(&a.c));
    a.key(Key::Delete);
    assert_eq!(count(&mut a), 1, "Entf bei offenem Fenster");
    a.st().sheet_dialog = None;
    a.settle();
    // Klick auf die Zeile im Browser wählt den Kreis; Entf dort löscht ihn nicht.
    let shot = a.settle();
    let row = shot
        .texts
        .iter()
        .find(|t| t.text.starts_with("Pos. 1 – "))
        .expect("Zeile im Browser")
        .rect
        .center();
    a.click(row);
    assert_eq!(a.st().drawing_ui.selected_balloon, Some(b));
    a.key(Key::Delete);
    assert_eq!(count(&mut a), 1, "Entf nach Klick in den Browser");
    // Klick auf den Kreis im Blatt, dann Entf: gelöscht.
    let l = drawing_view::last_layout(&a.c).unwrap();
    let bm = l.balloons.iter().find(|x| x.id == b).unwrap().clone();
    let c = drawing_view::sheet_to_screen(l.canvas, l.sheet_mm, l.zoom, l.pan, bm.centre);
    a.click(c);
    assert_eq!(a.st().drawing_ui.selected_balloon, Some(b));
    // Hat ein Bedienelement die Tastatur (Tab), gilt Entf ihm.
    a.key(Key::Tab);
    assert!(
        a.c.memory(|m| m.focused()).is_some(),
        "Tab gibt einem Knopf den Fokus"
    );
    a.key(Key::Delete);
    assert_eq!(count(&mut a), 1, "Entf bei Fokus auf einem Knopf");
    a.c.memory_mut(|m| m.stop_text_input());
    a.c.memory_mut(|m| {
        if let Some(f) = m.focused() {
            m.surrender_focus(f);
        }
    });
    a.click(c);
    a.key(Key::Delete);
    assert_eq!(count(&mut a), 0, "Entf im Blatt löscht");
}

#[test]
fn typing_in_the_sheet_dialog_does_not_reach_the_balloon() {
    let mut st = AppState::new();
    for _ in 0..3 {
        st.create_box(DVec3::new(40.0, 30.0, 20.0)).unwrap();
    }
    let mut dialog = NewDrawingDialog::new(&st);
    dialog.paper = Paper::A3;
    dialog.landscape = true;
    dialog.scale = dialog.suggestion(&st);
    let d = dialog.create(&mut st).unwrap();
    let front = st.document.drawing(d).unwrap().views[0].id;
    let k0 = st.document.body_key(st.bodies[0].source);
    let b = st
        .document
        .add_balloon(d, front, k0, (0.5, 0.5), (10.0, 10.0))
        .unwrap();
    let mut a = App::new(BIG, st);
    a.st().drawing_ui.selected_balloon = Some(b);
    a.st().balloon_tool.set_active(true);
    a.settle();
    assert_eq!(a.st().drawing_ui.area, WorkArea::Drawing(d));
    a.base = a
        .settle()
        .texts
        .into_iter()
        .map(|t| (t.text, t.rect))
        .collect();
    planora_app::sheet_dialog::open(a.st(), d);
    a.settle();
    let r = a.scroll_to("Quader 2");
    a.click(r.center());
    assert!(viewport::typing(&a.c), "das Feld hat die Tastatur");
    a.key(Key::Delete);
    a.typed("d");
    a.typed("+");
    a.key(Key::Escape);
    assert_eq!(balloons(a.st(), d).len(), 1, "Kreis bleibt");
    assert_eq!(a.st().drawing_ui.selected_balloon, Some(b));
    assert!(a.st().balloon_tool.is_active(), "Werkzeug bleibt");
    let key = a.st().document.parts_rows()[1].keys[0];
    assert_eq!(
        a.st().sheet_dialog.as_ref().unwrap().parts.overrides[&key].name,
        FieldValue::Text("d+".into())
    );
}

// (d) -----------------------------------------------------------------------------------------

#[test]
fn exclusive_tools() {
    let (mut st, _, _, _, mut h) = box_front(BIG);
    h.click_text(&mut st, TOOL);
    assert!(st.balloon_tool.is_active());
    assert_eq!(st.dimension_tool.mode, ToolMode::Off);
    h.click_text(&mut st, dimension_tool::TOOL);
    assert!(st.dimension_tool.is_active());
    assert!(!st.balloon_tool.is_active(), "Maß an → Position aus");
    h.click_text(&mut st, TOOL);
    assert!(st.balloon_tool.is_active());
    assert!(!st.dimension_tool.is_active(), "Position an → Maß aus");
    // Eine Maßart wählen schaltet „Maß“ ein und „Position“ aus.
    h.click_text(
        &mut st,
        ToolMode::Linear(dimension_tool::LinearDir::Horizontal).label(),
    );
    h.click_text(
        &mut st,
        ToolMode::Linear(dimension_tool::LinearDir::Vertical).label(),
    );
    assert_eq!(
        st.dimension_tool.mode,
        ToolMode::Linear(dimension_tool::LinearDir::Vertical)
    );
    assert!(!st.balloon_tool.is_active());
    // Mit „Maß“ wählt ein Klick auf eine Linie einen Bezug und setzt keinen Kreis.
    h.click_at(&mut st, DVec2::new(95.0, 100.0));
    assert_eq!(st.dimension_tool.pending().len(), 1);
}

// (e) -----------------------------------------------------------------------------------------

/// Zwei Quader nebeneinander, Vorderansicht 1:1 in (120, 100) auf A4 quer.
fn two_boxes(size: Vec2) -> (AppState, [FeatureId; 2], DrawingId, ViewId, Harness) {
    let mut st = AppState::new();
    let a = st.create_box(DVec3::new(10.0, 20.0, 30.0)).unwrap();
    let b = st.create_box(DVec3::new(40.0, 10.0, 10.0)).unwrap();
    let d = new_drawing(&mut st, A4_QUER);
    let v = add_view(&mut st, d, ViewKind::Front, 120.0, 100.0);
    let h = open(&mut st, d, size);
    (st, [a, b], d, v, h)
}

/// Mitte der längsten sichtbaren Linie des Körpers `body`.
fn line_of(h: &Harness, view: ViewId, body: usize) -> DVec2 {
    h.line_mid(view, "Kante des Körpers", |l| {
        body_of(l) == body && l.vis == Visibility::Visible
    })
}

/// Texte der Nummernkreise im Blattmodell (je Kreis die Zahl).
fn balloon_texts(h: &Harness) -> Vec<(BalloonId, String, MarkKind)> {
    h.layout()
        .balloons
        .iter()
        .map(|bm| {
            let (t, k) = bm
                .marks
                .iter()
                .find_map(|m| match m {
                    Mark::Text { text, kind, .. } => Some((text.clone(), *kind)),
                    _ => None,
                })
                .expect("Zahl");
            (bm.id, t, k)
        })
        .collect()
}

#[test]
fn lost_balloon_is_shown() {
    let (mut st, [_, fb], d, front, mut h) = two_boxes(BIG);
    let p = line_of(&h, front, 1);
    let b = balloon_tool::place(&mut st, d, front, 1, p).unwrap();
    assert_eq!(balloon_of(&st, d, b).map(|x| x.body), Some(key(fb)));
    st.delete_feature(fb).unwrap();
    compute(&mut st, &mut h);
    st.drawing_ui.selected_balloon = None;
    let shot = h.idle(&mut st);
    // Rot mit „?“.
    assert_eq!(
        balloon_texts(&h),
        vec![(b, "?".to_string(), MarkKind::Lost)]
    );
    let bm = h.balloon(b);
    let c = h.screen(bm.centre);
    let circles = shot.circles(c, (bm.radius * h.k()) as f32);
    assert!(!circles.is_empty());
    assert!(circles.iter().all(|p| p.color == theme::SHEET_LOST));
    let q = shot
        .texts
        .iter()
        .find(|t| t.text == "?" && t.rect.expand(2.0).contains(c))
        .expect("„?“ im Kreis");
    assert_eq!(q.color, theme::SHEET_LOST);
    // Hinweiszeile rot.
    let note = shot.find(&lost_note(1));
    assert_eq!(note.color, error_color(st.dark));
    // Tooltip über dem Kreis.
    let shot = h.hover(&mut st, c);
    assert!(shot.has(LOST_TIP), "Tooltip fehlt");
    // Über einem Kreis mit Teil kein solcher Tooltip.
    let ok = st
        .document
        .add_balloon(
            d,
            front,
            key(st.document.features()[0].id),
            (1.0, 0.0),
            (15.0, -15.0),
        )
        .unwrap();
    h.idle(&mut st);
    let c_ok = h.screen(h.balloon(ok).centre);
    let shot = h.hover(&mut st, c_ok);
    assert!(!shot.has(LOST_TIP), "Tooltip über einem Kreis mit Teil");
    st.document
        .update_drawing(d, |dr| {
            dr.remove_balloon(ok);
        })
        .unwrap();
    // Zwei verlorene Kreise: Mehrzahl.
    assert_eq!(lost_note(1), "1 Positionsnummer hat ihr Teil verloren.");
    assert_eq!(lost_note(2), "2 Positionsnummern haben ihr Teil verloren.");
    let k1 = st.document.drawing(d).unwrap().balloons[0].body;
    st.document
        .add_balloon(d, front, k1, (0.2, 0.2), (-10.0, 10.0))
        .unwrap();
    let shot = h.idle(&mut st);
    assert!(shot.has(&lost_note(2)));
    assert!(!shot.has(&lost_note(1)));
}

// (f) -----------------------------------------------------------------------------------------

#[test]
fn numbers_follow_parts_list() {
    let (mut st, [fa, fb], d, front, mut h) = two_boxes(BIG);
    st.rename_feature(fa, "Bolzen").unwrap();
    st.rename_feature(fb, "Bolzen").unwrap();
    compute(&mut st, &mut h);
    h.idle(&mut st);
    let ba = balloon_tool::place(&mut st, d, front, 0, line_of(&h, front, 0)).unwrap();
    let bb = balloon_tool::place(&mut st, d, front, 1, line_of(&h, front, 1)).unwrap();
    st.drawing_ui.selected_balloon = None;
    let shot = h.idle(&mut st);
    let pen = MarkKind::Position(PositionPen::Circle);
    assert_eq!(
        balloon_texts(&h),
        vec![(ba, "1".into(), pen), (bb, "1".into(), pen)]
    );
    assert_painted(&h, &shot, ba, "1");
    assert_painted(&h, &shot, bb, "1");
    // Den zweiten umbenennen: eigene Zeile 2.
    st.rename_feature(fb, "Achse").unwrap();
    compute(&mut st, &mut h);
    let shot = h.idle(&mut st);
    assert_eq!(
        balloon_texts(&h),
        vec![(ba, "1".into(), pen), (bb, "2".into(), pen)]
    );
    assert_painted(&h, &shot, bb, "2");
    // Den ersten auch „Achse“ nennen: wieder eine Zeile, beide „1“.
    st.rename_feature(fa, "Achse").unwrap();
    compute(&mut st, &mut h);
    h.idle(&mut st);
    assert_eq!(
        balloon_texts(&h),
        vec![(ba, "1".into(), pen), (bb, "1".into(), pen)]
    );
}

// (g) -----------------------------------------------------------------------------------------

#[test]
fn survives_recompute_and_reorder() {
    let (mut st, [fa, fb], d, front, mut h) = two_boxes(BIG);
    let pen = MarkKind::Position(PositionPen::Circle);
    // Kreis über das Werkzeug an den zweiten Körper (Index 1).
    st.balloon_tool.set_active(true);
    h.idle(&mut st);
    let p = line_of(&h, front, 1);
    let b = place_by_click(&mut st, &mut h, d, p);
    assert_eq!(b.body, key(fb));
    st.drawing_ui.selected_balloon = None;
    h.idle(&mut st);
    assert_eq!(balloon_texts(&h), vec![(b.id, "2".into(), pen)]);
    // Maß des ersten Quaders ändern: Kreis bleibt am zweiten.
    st.document
        .update(fa, |k| {
            if let FeatureKind::Box { size, .. } = k {
                *size = DVec3::new(15.0, 25.0, 35.0);
            }
        })
        .unwrap();
    st.refresh();
    compute(&mut st, &mut h);
    assert_eq!(balloon_texts(&h), vec![(b.id, "2".into(), pen)]);
    // Früheren Körper löschen: der zweite ist jetzt Index 0 und Zeile 1.
    st.delete_feature(fa).unwrap();
    compute(&mut st, &mut h);
    assert_eq!(st.bodies.len(), 1);
    assert_eq!(balloon_texts(&h), vec![(b.id, "1".into(), pen)]);
    assert_eq!(balloons(&st, d)[0].body, key(fb));
    // Verschieben anhängen: Kreis bleibt am Teil.
    st.move_body(0, DVec3::new(5.0, 0.0, 0.0)).unwrap();
    compute(&mut st, &mut h);
    assert_ne!(
        st.bodies[0].source.feature, fb,
        "Körper kommt aus dem Verschieben"
    );
    assert_eq!(balloon_texts(&h), vec![(b.id, "1".into(), pen)]);
    // Ein neuer Kreis am verschobenen Körper merkt sich den ursprünglichen Quader.
    let p = line_of(&h, front, 0);
    let b2 = balloon_tool::place(&mut st, d, front, 0, p).unwrap();
    assert_eq!(balloon_of(&st, d, b2).map(|x| x.body), Some(key(fb)));
    assert_eq!(
        balloon_tool::body_key_of_stroke(&st, 0),
        Some(key(fb)),
        "Kennung über body_key"
    );
}

/// Netz-Körper zählen bei der Körpernummer der Linien nicht mit.
#[test]
fn body_index_counts_only_exact_bodies() {
    let mut st = AppState::new();
    // Ein Netz-Körper vorneweg: in den Ansichten gibt es ihn nicht.
    st.import_mesh("Würfel", std::sync::Arc::new(cube_mesh()), 10.0)
        .unwrap();
    let fa = st.create_box(DVec3::new(10.0, 20.0, 30.0)).unwrap();
    let fb = st.create_box(DVec3::new(40.0, 10.0, 10.0)).unwrap();
    assert_eq!(balloon_tool::body_key_of_stroke(&st, 0), Some(key(fa)));
    assert_eq!(balloon_tool::body_key_of_stroke(&st, 1), Some(key(fb)));
    assert_eq!(balloon_tool::body_key_of_stroke(&st, 2), None);
    assert_eq!(st.bodies.len(), 3);
}

fn cube_mesh() -> planora_exchange::ExchangeMesh {
    planora_exchange::ExchangeMesh {
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

// (h) -----------------------------------------------------------------------------------------

/// Rollbalken in Ruhe im unteren Rand des Werkzeugfelds?
fn scrollbar_in(shot: &Shot, tool: Rect) -> bool {
    let band = Rect::from_min_max(pos2(tool.left(), tool.bottom() - 12.0), tool.max);
    shot.fills
        .iter()
        .any(|(r, f)| band.contains_rect(*r) && r.height() <= 8.0 && r.width() >= 20.0 && f.a() > 0)
}

fn tool_field(c: &egui::Context) -> Rect {
    shell_panels::field_rects(c)
        .into_iter()
        .find(|(n, _)| *n == "Werkzeugfeld")
        .expect("Werkzeugfeld")
        .1
}

#[test]
fn toolbar_fits_1100x700() {
    let fit = icons::glyph(icons::Icon::FitSheet).to_string();
    for size in [MID, BIG] {
        let mut width = None;
        for mode in ToolMode::CHOICES {
            let (mut st, _, _, _, _) = box_front(BIG);
            // Zuletzt gewählte Maßart (das Auswahlfeld zeigt sie), Werkzeug aus.
            st.dimension_tool.set_mode(mode);
            st.dimension_tool.set_mode(ToolMode::Off);
            let mut a = App::new(size, st);
            let shot = a.settle();
            let tool = tool_field(&a.c);
            let inside = |t: &str| {
                shot.texts
                    .iter()
                    .find(|s| s.text == t && tool.contains(s.rect.center()))
                    .unwrap_or_else(|| panic!("{size:?} {mode:?}: „{t}“ fehlt im Werkzeugfeld"))
                    .clone()
            };
            // Nichts ragt heraus oder ist abgeschnitten.
            for t in shot.texts.iter().filter(|t| tool.contains(t.rect.center())) {
                assert!(
                    tool.contains_rect(t.rect) && t.clip.expand(0.5).contains_rect(t.rect),
                    "{size:?} {mode:?}: „{}“ abgeschnitten",
                    t.text
                );
            }
            let pos = inside(TOOL);
            let dim = inside(dimension_tool::TOOL);
            let combo = inside(mode.label());
            // „Blatt einpassen“ als Symbol neben den Lupen in Zeile 2.
            let fit_r = inside(&fit);
            assert!((fit_r.rect.center().y - dim.rect.center().y).abs() < 6.0);
            assert!(!shot.has("Blatt einpassen"), "nur noch als Symbol");
            assert!(
                (pos.rect.center().y - dim.rect.center().y).abs() < 6.0,
                "{size:?} {mode:?}: Zeile 2"
            );
            assert!(
                pos.rect.left() > combo.rect.right(),
                "{size:?} {mode:?}: rechts vom Auswahlfeld"
            );
            assert!(
                !scrollbar_in(&shot, tool),
                "{size:?} {mode:?}: Rollbalken sichtbar"
            );
            // Die Zeile ist bei jeder Maßart gleich breit (Auswahlfeld fester Breite).
            let right = pos.rect.right();
            match width {
                None => width = Some(right),
                Some(w) => assert!((w - right).abs() < 0.5, "{size:?} {mode:?}: {w} vs {right}"),
            }
        }
    }
    // 800 × 600: zu schmal, das Feld rollt (Balken in Ruhe sichtbar).
    let (st, _, _, _, _) = box_front(BIG);
    let mut a = App::new(SMALL, st);
    let shot = a.settle();
    assert!(
        scrollbar_in(&shot, tool_field(&a.c)),
        "kein Rollbalken bei 800 × 600"
    );
}

// (i) -----------------------------------------------------------------------------------------

#[test]
fn blocked_states() {
    // Alte Version: Schalter gesperrt, eingeschaltetes Werkzeug geht aus; Wählen geht,
    // Ziehen und Entf ändern nichts.
    let (mut st, _, d, front, _) = box_front(BIG);
    let k0 = key(st.document.features()[0].id);
    let b = st
        .document
        .add_balloon(d, front, k0, (0.0, 0.5), (-10.0, 10.0))
        .unwrap();
    let mut h = open(&mut st, d, BIG);
    h.editable = false;
    h.idle(&mut st);
    let drawing = st.document.drawing(d).unwrap().clone();
    assert_eq!(balloon_tool::blocked(&drawing, false), Some(READ_ONLY_TIP));
    let e0 = st.document.edit_count();
    h.click_text(&mut st, TOOL);
    assert!(!st.balloon_tool.is_active());
    st.balloon_tool.set_active(true);
    h.idle(&mut st);
    assert!(!st.balloon_tool.is_active(), "gesperrt heißt aus");
    h.click_at(&mut st, DVec2::new(95.0, 100.0));
    let c = h.screen(h.balloon(b).centre);
    h.click(&mut st, c);
    assert_eq!(st.drawing_ui.selected_balloon, Some(b), "Wählen geht");
    h.key(&mut st, Key::Delete);
    h.drag(&mut st, c, vec2(40.0, 40.0), |_, _| {});
    assert_eq!(balloons(&st, d).len(), 1);
    assert_eq!(st.document.edit_count(), e0);

    // Keine Ansicht.
    let mut st = AppState::new();
    st.create_box(DVec3::new(10.0, 20.0, 30.0)).unwrap();
    let d = new_drawing(&mut st, A4_QUER);
    let mut h = open(&mut st, d, BIG);
    let drawing = st.document.drawing(d).unwrap().clone();
    assert_eq!(balloon_tool::blocked(&drawing, true), Some(NO_VIEWS_TIP));
    h.click_text(&mut st, TOOL);
    assert!(!st.balloon_tool.is_active());

    // Stücklistenblatt.
    let stamp = Stamp {
        date: "2026-10-18".into(),
        user: "dennis".into(),
    };
    let p = st.document.add_parts_sheets(A4_QUER, &stamp).unwrap();
    let mut h = open(&mut st, p, BIG);
    let drawing = st.document.drawing(p).unwrap().clone();
    assert_eq!(balloon_tool::blocked(&drawing, true), Some(PARTS_SHEET_TIP));
    h.click_text(&mut st, TOOL);
    assert!(!st.balloon_tool.is_active());
    st.balloon_tool.set_active(true);
    h.idle(&mut st);
    assert!(!st.balloon_tool.is_active());
    assert_eq!(
        balloon_tool::place(&mut st, p, ViewId(1), 0, DVec2::new(10.0, 10.0)),
        Err(PARTS_SHEET_TIP.to_string())
    );

    // Nur Isometrie: erlaubt.
    let mut st = AppState::new();
    let f = st.create_box(DVec3::new(10.0, 20.0, 30.0)).unwrap();
    let d = new_drawing(&mut st, A4_QUER);
    let iso = add_view(&mut st, d, ViewKind::Isometric, 140.0, 100.0);
    let mut h = open(&mut st, d, BIG);
    let drawing = st.document.drawing(d).unwrap().clone();
    assert_eq!(balloon_tool::blocked(&drawing, true), None);
    h.click_text(&mut st, TOOL);
    assert!(st.balloon_tool.is_active());
    let p = h.line_mid(iso, "Kante", |l| l.vis == Visibility::Visible);
    let b = place_by_click(&mut st, &mut h, d, p);
    assert_eq!((b.view, b.body), (iso, key(f)));

    // Skizzen-Modus: kein Kreis, Werkzeug aus.
    let sk = st
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    st.open_sketch(sk).unwrap();
    let before = st.document.edit_count();
    assert_eq!(
        balloon_tool::place(&mut st, d, iso, 0, p),
        Err(SKETCH_BLOCKED.to_string())
    );
    assert_eq!(st.document.edit_count(), before);
    h.idle(&mut st);
    assert!(!st.balloon_tool.is_active());
}

// (j) -----------------------------------------------------------------------------------------

#[test]
fn limit_message() {
    let (mut st, f, d, front, mut h) = box_front(BIG);
    for _ in 0..200 {
        st.document
            .add_balloon(d, front, key(f), (0.0, 0.0), (-30.0, -30.0))
            .unwrap();
    }
    st.balloon_tool.set_active(true);
    h.idle(&mut st);
    let e0 = st.document.edit_count();
    let shot = h.click_at(&mut st, DVec2::new(95.0, 100.0));
    assert_eq!(balloons(&st, d).len(), 200);
    assert_eq!(st.document.edit_count(), e0);
    assert_eq!(st.balloon_tool.message(), Some(TOO_MANY_BALLOONS));
    assert_eq!(shot.find(TOO_MANY_BALLOONS).color, error_color(st.dark));
}

// ---- Übertrag aus Aufgabe 4: Farbe und Strichbreite der Kreise -------------------------------

#[test]
fn balloons_paint_in_sheet_colour_with_paper_widths() {
    for sheet in [A4_QUER, A0_QUER] {
        let f = planora_drawing::sheet_frame::sheet_scale(sheet);
        let mut st = AppState::new();
        let fb = st.create_box(DVec3::new(10.0, 20.0, 30.0)).unwrap();
        let d = new_drawing(&mut st, sheet);
        let front = add_view(&mut st, d, ViewKind::Front, 100.0, 100.0);
        let b = st
            .document
            .add_balloon(d, front, key(fb), (0.0, 0.5), (-20.0, 20.0))
            .unwrap();
        let mut h = open(&mut st, d, BIG);
        // Groß genug, dass jede Strichbreite über einem Pixel liegt.
        st.drawing_ui.zoom = 8.0;
        let shot = h.idle(&mut st);
        let k = h.k();
        let bm = h.balloon(b);
        let c = h.screen(bm.centre);
        let circle = shot.circles(c, (bm.radius * k) as f32);
        assert_eq!(circle.len(), 1, "{sheet:?}");
        let w_circle = (PositionPen::Circle.width_at(f) * k) as f32;
        let w_leader = (PositionPen::Leader.width_at(f) * k) as f32;
        assert!(w_leader > 1.5, "{sheet:?}: Zoom zu klein");
        assert!((circle[0].width - w_circle).abs() < 1e-3, "{sheet:?}");
        assert_eq!(circle[0].color, theme::SHEET_DIM);
        // Punkt am Teil und Hinweislinie mit der Breite der Hinweislinie.
        let at = bm.centre - DVec2::new(-20.0, 20.0);
        let dot = shot.circles(h.screen(at), (0.5 * f * k) as f32);
        assert_eq!(dot.len(), 1, "{sheet:?}");
        assert!((dot[0].width - w_leader).abs() < 1e-3, "{sheet:?}");
        assert_eq!(dot[0].color, theme::SHEET_DIM);
        let leader: Vec<_> = shot
            .segs
            .iter()
            .filter(|s| (s.points[0] - h.screen(at)).length() < 1.0)
            .collect();
        assert_eq!(leader.len(), 1, "{sheet:?}");
        assert!((leader[0].width - w_leader).abs() < 1e-3);
        assert_eq!(leader[0].color, theme::SHEET_DIM);
        // Gewählt: Akzentfarbe, gleiche Breite.
        st.drawing_ui.selected_balloon = Some(b);
        let shot = h.idle(&mut st);
        let circle = shot.circles(c, (bm.radius * k) as f32);
        assert_eq!(circle.len(), 1);
        assert_eq!(circle[0].color, theme::SHEET_SELECTED);
        assert!((circle[0].width - w_circle).abs() < 1e-3);
    }
}

/// Texte und Tooltips in einfacher Sprache, als ganze Sätze.
#[test]
fn texts_are_plain_sentences() {
    for t in [
        TOOL_TIP,
        HINT,
        MISSED,
        READ_ONLY_TIP,
        PARTS_SHEET_TIP,
        LOST_TIP,
        &lost_note(1),
        &lost_note(3),
    ] {
        assert!(t.ends_with('.'), "{t}");
        for w in ["balloon", "body", "key", "index", "stroke", "edge"] {
            assert!(!t.to_lowercase().contains(w), "{t}");
        }
    }
    // Tooltip am Schalter.
    let (mut st, _, _, _, mut h) = box_front(BIG);
    let shot = h.idle(&mut st);
    let p = shot.find(TOOL).rect.center();
    let shot = h.hover(&mut st, p);
    assert!(
        shot.has(TOOL) && shot.has(TOOL_TIP),
        "Tooltip mit Name und Erklärung"
    );
}

/// Messung (kein Prüfwert): Zeit je Bild der ganzen App mit 45 Körpern und 200 Kreisen, in Ruhe
/// und beim Ziehen eines Kreises. `cargo test -p planora-app --test balloon_tool -- --ignored
/// --nocapture frame_cost`.
#[test]
#[ignore]
fn frame_cost() {
    for count in [0usize, 200] {
        frame_cost_with(count);
    }
}

fn frame_cost_with(count: usize) {
    let mut st = AppState::new();
    for i in 0..45 {
        st.document
            .add_feature(
                format!("Teil {}", i + 1),
                FeatureKind::Box {
                    size: DVec3::new(10.0, 10.0, 10.0),
                    origin: DVec3::new(i as f64 * 20.0, 0.0, 0.0),
                },
            )
            .unwrap();
    }
    st.refresh();
    let d = new_drawing(
        &mut st,
        Sheet {
            paper: Paper::A3,
            landscape: true,
        },
    );
    let front = st
        .document
        .add_view(
            d,
            ViewKind::Front,
            Scale { num: 1, den: 5 },
            Placement::Free { x: 210.0, y: 150.0 },
            None,
        )
        .unwrap();
    let keys: Vec<BodyKey> = st
        .bodies
        .iter()
        .map(|b| st.document.body_key(b.source))
        .collect();
    for i in 0..count {
        let u = (i % 20) as f64 / 19.0;
        let v = (i / 20) as f64 / 9.0;
        st.document
            .add_balloon(d, front, keys[i % keys.len()], (u, v), (8.0, 12.0))
            .unwrap();
    }
    st.drawing_ui.open(d);
    let mut a = App::new(BIG, st);
    let time = |a: &mut App, events: &dyn Fn(usize) -> Vec<Event>| {
        for i in 0..5 {
            a.frame(events(i));
        }
        let t = std::time::Instant::now();
        for i in 0..20 {
            a.frame(events(5 + i));
        }
        t.elapsed().as_secs_f64() * 1000.0 / 20.0
    };
    let rest = time(&mut a, &|_| vec![]);
    if count == 0 {
        println!("45 Körper, keine Kreise: Ruhe {rest:.1} ms");
        return;
    }
    let l = drawing_view::last_layout(&a.c).unwrap();
    let bm = l.balloons[0].clone();
    let c = drawing_view::sheet_to_screen(l.canvas, l.sheet_mm, l.zoom, l.pan, bm.centre);
    a.frame(vec![Event::PointerMoved(c)]);
    a.frame(vec![button(c, true)]);
    let drag = time(&mut a, &|i| {
        vec![Event::PointerMoved(c + vec2(i as f32, 0.5 * i as f32))]
    });
    a.frame(vec![button(c + vec2(30.0, 15.0), false)]);
    println!("45 Körper, 200 Kreise: Ruhe {rest:.1} ms, Ziehen {drag:.1} ms");
}

// ---- Lücken aus dem Review --------------------------------------------------------------------

fn bm(id: u64, centre: (f64, f64), radius: f64) -> BalloonMarks {
    BalloonMarks {
        view: ViewId(1),
        id: BalloonId(id),
        centre: DVec2::new(centre.0, centre.1),
        radius,
        marks: Vec::new(),
    }
}

/// Bei überlappenden Kreisen gewinnt der, in dem der Zeiger am tiefsten liegt.
#[test]
fn overlapping_circles_pick_the_closest_edge() {
    let to_screen = |p: DVec2| pos2((10.0 * p.x) as f32, (10.0 * p.y) as f32);
    let list = [bm(1, (0.0, 0.0), 2.0), bm(2, (3.0, 0.0), 2.0)];
    // 0,5 mm neben der Mitte von 2, 2,5 mm von der Mitte von 1 (beide getroffen).
    assert_eq!(
        balloon_tool::balloon_hit(&list, &to_screen, pos2(25.0, 0.0)),
        Some(BalloonId(2))
    );
    assert_eq!(
        balloon_tool::balloon_hit(&list, &to_screen, pos2(5.0, 0.0)),
        Some(BalloonId(1))
    );
    // Außerhalb beider: der nähere Rand.
    let list = [bm(1, (0.0, 0.0), 1.0), bm(2, (3.0, 0.0), 1.0)];
    assert_eq!(
        balloon_tool::balloon_hit(&list, &to_screen, pos2(18.0, 0.0)),
        Some(BalloonId(2))
    );
}

/// Quader-Vorderansicht mit einem waagerechten Maß (Maßlinie bei y ≈ 130) und einem Kreis, dessen
/// Mitte auf der Maßlinie liegt.
fn balloon_on_dimension() -> (AppState, DrawingId, ViewId, BalloonId, DVec2, Harness) {
    let (mut st, f, d, front, mut h) = box_front(BIG);
    st.dimension_tool
        .set_mode(ToolMode::Linear(dimension_tool::LinearDir::Horizontal));
    h.click_at(&mut st, DVec2::new(95.0, 100.0));
    h.click_at(&mut st, DVec2::new(105.0, 100.0));
    h.click_at(&mut st, DVec2::new(100.0, 130.0));
    st.dimension_tool.set_mode(ToolMode::Off);
    let dims = st.document.drawing(d).unwrap().dimensions.clone();
    assert_eq!(dims.len(), 1);
    let y = 115.0 + dims[0].offset;
    let target = DVec2::new(101.0, y);
    let b = st
        .document
        .add_balloon(d, front, key(f), (0.5, 0.5), (0.0, 0.0))
        .unwrap();
    h.idle(&mut st);
    let at = h.balloon(b).centre;
    st.document
        .update_drawing(d, |dr| {
            dr.balloons[0].offset = (target.x - at.x, target.y - at.y);
        })
        .unwrap();
    st.drawing_ui.selected_balloon = None;
    st.drawing_ui.selected_dim = None;
    h.idle(&mut st);
    assert!(h.balloon(b).centre.distance(target) < 1e-9);
    (st, d, front, b, target, h)
}

#[test]
fn balloon_wins_over_dimension_on_click_and_drag() {
    let (mut st, d, _, b, target, mut h) = balloon_on_dimension();
    let dim_before = st.document.drawing(d).unwrap().dimensions.clone();
    let p = h.screen(target);
    h.click(&mut st, p);
    assert_eq!(st.drawing_ui.selected_balloon, Some(b));
    assert_eq!(st.drawing_ui.selected_dim, None);
    // Ziehen am selben Punkt verschiebt den Kreis, nicht das Maß.
    st.drawing_ui.selected_balloon = None;
    let off0 = st.document.drawing(d).unwrap().balloons[0].offset;
    h.drag(&mut st, p, vec2(0.0, -30.0), |_, _| {});
    let dr = st.document.drawing(d).unwrap();
    assert_eq!(dr.dimensions, dim_before, "Maß unverändert");
    assert_ne!(dr.balloons[0].offset, off0, "Kreis gezogen");
    assert_eq!(st.drawing_ui.selected_balloon, Some(b));
}

/// Mit „Position“ lassen sich Ansichten und Maße nicht ziehen (Kreise schon).
#[test]
fn position_tool_does_not_drag_views_or_dimensions() {
    let (mut st, d, front, _, target, mut h) = balloon_on_dimension();
    // Kreis aus dem Weg (weit nach links oben).
    st.document
        .update_drawing(d, |dr| dr.balloons[0].offset = (-60.0, 60.0))
        .unwrap();
    st.balloon_tool.set_active(true);
    h.idle(&mut st);
    let before = st.document.drawing(d).unwrap().clone();
    let e0 = st.document.edit_count();
    // Auf der Maßlinie und auf der linken Kante der Ansicht.
    h.drag(&mut st, h.screen(target), vec2(0.0, -40.0), |_, _| {});
    h.drag(
        &mut st,
        h.screen(DVec2::new(95.0, 100.0)),
        vec2(50.0, 30.0),
        |_, _| {},
    );
    let after = st.document.drawing(d).unwrap();
    assert_eq!(after.dimensions, before.dimensions);
    assert_eq!(after.view(front), before.view(front));
    assert_eq!(st.document.edit_count(), e0);
    assert!(st.balloon_tool.is_active());
    // Ohne Werkzeug zieht dieselbe Bewegung die Ansicht.
    st.balloon_tool.set_active(false);
    h.idle(&mut st);
    h.drag(
        &mut st,
        h.screen(DVec2::new(95.0, 100.0)),
        vec2(50.0, 30.0),
        |_, _| {},
    );
    assert_ne!(
        st.document.drawing(d).unwrap().view(front),
        before.view(front)
    );
}

/// Ein Ziehen, das am Ausgangspunkt endet, ändert nichts (keine Änderung im Dokument).
#[test]
fn drag_back_to_start_commits_nothing() {
    let (mut st, f, d, front, mut h) = box_front(BIG);
    let b = st
        .document
        .add_balloon(d, front, key(f), (0.0, 0.5), (-15.0, 10.0))
        .unwrap();
    h.idle(&mut st);
    let c = h.screen(h.balloon(b).centre);
    let e0 = st.document.edit_count();
    h.frame_dt(&mut st, vec![Event::PointerMoved(c)], 1.0);
    h.frame_dt(&mut st, vec![button(c, true)], 0.02);
    for by in [vec2(20.0, 0.0), vec2(40.0, 10.0), vec2(0.0, 0.0)] {
        h.frame_dt(&mut st, vec![Event::PointerMoved(c + by)], 0.02);
    }
    h.frame_dt(&mut st, vec![button(c, false)], 0.02);
    h.idle(&mut st);
    assert_eq!(st.document.edit_count(), e0);
    assert_eq!(balloons(&st, d)[0].offset, (-15.0, 10.0));
}

/// Die Auswahl eines Kreises verfällt beim Wechsel der Zeichnung, wenn die Zeichnung oder der
/// Kreis verschwindet (Kennungen zählen je Zeichnung: eine alte Auswahl träfe sonst den Kreis
/// einer anderen Zeichnung).
#[test]
fn balloon_selection_is_cleared_when_it_goes_stale() {
    let (mut st, f, d1, v1, mut h) = box_front(BIG);
    let b1 = st
        .document
        .add_balloon(d1, v1, key(f), (0.0, 0.5), (-15.0, 10.0))
        .unwrap();
    let d2 = st
        .document
        .add_drawing("Blatt 2", A4_QUER, Projection::First)
        .unwrap();
    let v2 = add_view(&mut st, d2, ViewKind::Front, 100.0, 100.0);
    let b2 = st
        .document
        .add_balloon(d2, v2, key(f), (0.0, 0.5), (-15.0, 10.0))
        .unwrap();
    assert_eq!(b1, b2, "gleiche Kennung in beiden Zeichnungen");
    // Wechsel über den Arbeitsbereich (ohne `open`): die Auswahl verfällt, Entf trifft nichts.
    st.drawing_ui.selected_balloon = Some(b1);
    st.drawing_ui.area = WorkArea::Drawing(d2);
    st.drawing_ui.tick(&st.bodies, &st.document, true);
    assert_eq!(st.drawing_ui.selected_balloon, None);
    h.idle(&mut st);
    h.key(&mut st, Key::Delete);
    assert_eq!(
        balloons(&st, d2).len(),
        1,
        "Kreis der anderen Zeichnung bleibt"
    );
    // Wechsel über `open`.
    st.drawing_ui.selected_balloon = Some(b2);
    st.drawing_ui.open(d1);
    assert_eq!(st.drawing_ui.selected_balloon, None);
    // Kreis gelöscht (etwa im Browser): die Auswahl verfällt beim nächsten Bild.
    st.drawing_ui.selected_balloon = Some(b1);
    st.document
        .update_drawing(d1, |dr| {
            dr.remove_balloon(b1);
        })
        .unwrap();
    st.drawing_ui.tick(&st.bodies, &st.document, true);
    assert_eq!(st.drawing_ui.selected_balloon, None);
    // Zeichnung gelöscht.
    st.drawing_ui.open(d2);
    st.drawing_ui.selected_balloon = Some(b2);
    st.document.delete_drawing(d2).unwrap();
    st.drawing_ui.tick(&st.bodies, &st.document, true);
    assert_eq!(st.drawing_ui.selected_balloon, None);
}

/// Schalter: Bedienname „Position“, gesperrt mit dem Grund im Tooltip; „Blatt einpassen“ als
/// Symbol mit Bedienname.
#[test]
fn tool_button_name_lock_and_reason() {
    let fit = icons::glyph(icons::Icon::FitSheet).to_string();
    let (mut st, _, _, _, mut h) = box_front(BIG);
    h.click_text(&mut st, TOOL);
    assert!(
        h.clicked.iter().any(|l| l.contains(TOOL)),
        "{:?}",
        h.clicked
    );
    st.drawing_ui.zoom = 2.0;
    h.click_text(&mut st, &fit);
    assert_eq!(st.drawing_ui.zoom, 1.0);
    assert!(
        h.clicked.iter().any(|l| l == drawing_view::FIT_SHEET),
        "{:?}",
        h.clicked
    );
    let shot = h.idle(&mut st);
    let shot = h.hover(&mut st, shot.find(&fit).rect.center());
    assert!(
        shot.has(drawing_view::FIT_SHEET),
        "Tooltip „Blatt einpassen“"
    );
    // Gesperrt: Tooltip nennt den Grund, nicht die Erklärung.
    for (editable, parts, why) in [(false, false, READ_ONLY_TIP), (true, true, PARTS_SHEET_TIP)] {
        let (mut st, _, d, _, _) = box_front(BIG);
        let d = if parts {
            let stamp = Stamp {
                date: "2026-10-18".into(),
                user: "dennis".into(),
            };
            st.document.add_parts_sheets(A4_QUER, &stamp).unwrap()
        } else {
            d
        };
        let mut h = open(&mut st, d, BIG);
        h.editable = editable;
        let shot = h.idle(&mut st);
        let p = shot.find(TOOL).rect.center();
        let shot = h.hover(&mut st, p);
        let shot = (0..5).fold(shot, |_, _| h.hover(&mut st, p));
        assert!(shot.has(why), "Grund „{why}“ fehlt");
        assert!(!shot.has(TOOL_TIP), "gesperrt zeigt nicht die Erklärung");
        let n = h.clicked.len();
        h.click(&mut st, p);
        assert_eq!(h.clicked.len(), n, "gesperrter Knopf nimmt keinen Klick an");
        assert!(!st.balloon_tool.is_active());
    }
}
