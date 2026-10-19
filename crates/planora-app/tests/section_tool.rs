//! Werkzeug „Schnitt“ (Teilprojekt 3d, Aufgabe 9): zwei Klicks legen eine Schnittansicht an,
//! Ansichtsfeld, Löschen, Maßstab, Speichern – mit echten egui-Ereignissen gegen den
//! Zeichnungsbereich (`drawing_view::show_with`, ohne Shell) bzw. die ganze App.
//!
//! Prüfkörper: Quader 40 × 20 × 60 (x, y, z), Vorderansicht 1:1 frei bei (150, 200) auf A3 quer.
//! In der Vorderansicht liegt die Ansichtskoordinate x auf der Weltachse x, y auf z; die Schnitte
//! daraus sind „Oben“ (waagerechte Linie) bzw. „Links“ (senkrechte Linie).

mod common;

use eframe::App as _;
use egui::{Color32, Event, Key, Modifiers, PointerButton, Pos2, Rect, Vec2, pos2, vec2};
use planora_app::app::PlanoraApp;
use planora_app::browser::error_color;
use planora_app::drawing_browser;
use planora_app::drawing_view;
use planora_app::section_tool::{
    self, FULL, HINT_FIRST, HINT_SECOND, NO_STRAIGHT_TIP, NOT_A_NUMBER, NOT_FROM_HERE, NUDGED,
    PARTS_SHEET_TIP, READ_ONLY_TIP, SNAP_PX, TOO_SHORT, TOOL,
};
use planora_app::state::AppState;
use planora_app::workspace::Workspace;
use planora_app::{fonts, shell_panels, theme};
use planora_drawing::annotate::{Mark, MarkKind};
use planora_drawing::sheet::view_transform;
use planora_drawing::{
    BodyKey, Drawing, DrawingId, DrawingKind, Paper, Placement, Projection, Scale, Sheet, ViewId,
    ViewKind, ViewLines,
};
use planora_feature::FeatureKind;
use planora_kernel::boolean::BoolOp;
use planora_math::{DVec2, DVec3};
use planora_project::config::Config;
use planora_project::library::Library;
use std::collections::BTreeMap;

const BIG: Vec2 = vec2(1400.0, 900.0);
const SMALL: Vec2 = vec2(800.0, 600.0);
const MID: Vec2 = vec2(1100.0, 700.0);

const A3_QUER: Sheet = Sheet {
    paper: Paper::A3,
    landscape: true,
};
const A4_QUER: Sheet = Sheet {
    paper: Paper::A4,
    landscape: true,
};

const NAME: &str = "Schnitt A\u{2013}A";

fn s11() -> Scale {
    Scale { num: 1, den: 1 }
}

fn context() -> egui::Context {
    let c = egui::Context::default();
    c.set_fonts(fonts::definitions());
    c.all_styles_mut(|s| s.interaction.tooltip_delay = 0.0);
    c
}

// ---- Bild ----------------------------------------------------------------------------------

#[derive(Clone, Debug)]
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

fn walk(shape: &egui::epaint::Shape, clip: Rect, s: &mut Shot) {
    use egui::epaint::Shape;
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
}

fn button(pos: Pos2, pressed: bool) -> Event {
    Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    }
}

fn key_events(key: Key, modifiers: Modifiers) -> [Event; 2] {
    let ev = |pressed| Event::Key {
        key,
        physical_key: Some(key),
        pressed,
        repeat: false,
        modifiers,
    };
    [ev(true), ev(false)]
}

// ---- Zeichnungsbereich ohne Shell ------------------------------------------------------------

struct Harness {
    ctx: egui::Context,
    time: f64,
    size: Vec2,
    editable: bool,
}

impl Harness {
    fn new(size: Vec2) -> Harness {
        Harness {
            ctx: context(),
            time: 0.0,
            size,
            editable: true,
        }
    }

    fn frame(&mut self, st: &mut AppState, events: Vec<Event>) -> Shot {
        // Mehr als die Doppelklick-Zeit zwischen den Bildern: jeder Klick zählt einzeln.
        self.time += 0.2;
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
        // Wie die App am Ende jedes Bildes: merken, ob ein Eingabefeld die Tastatur hatte.
        planora_app::viewport::remember_keyboard_focus(&self.ctx);
        let shot = Shot::of(&out);
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

    fn key(&mut self, st: &mut AppState, key: Key) -> Shot {
        let [down, up] = key_events(key, Modifiers::NONE);
        self.frame(st, vec![down]);
        self.frame(st, vec![up]);
        self.idle(st)
    }

    fn layout(&self) -> drawing_view::DrawingLayout {
        drawing_view::last_layout(&self.ctx).expect("kein Zeichnungsbereich gezeichnet")
    }

    fn screen(&self, p: DVec2) -> Pos2 {
        let l = self.layout();
        drawing_view::sheet_to_screen(l.canvas, l.sheet_mm, l.zoom, l.pan, p)
    }

    fn rect_of(&self, view: ViewId) -> Rect {
        self.layout()
            .views
            .iter()
            .find(|v| v.id == view)
            .unwrap_or_else(|| panic!("Ansicht {view:?} nicht gezeichnet"))
            .rect
    }

    fn marks(&self, name: &str) -> Vec<Mark> {
        drawing_view::last_sheet(&self.ctx)
            .expect("Blatt gezeichnet")
            .views
            .iter()
            .find(|v| v.name == name)
            .unwrap_or_else(|| panic!("keine Ansicht „{name}“"))
            .marks
            .clone()
    }
}

fn kind_of(m: &Mark) -> MarkKind {
    match m {
        Mark::Line { kind, .. }
        | Mark::Arrow { kind, .. }
        | Mark::Arc { kind, .. }
        | Mark::Text { kind, .. } => *kind,
    }
}

fn count_kind(marks: &[Mark], k: fn(MarkKind) -> bool) -> usize {
    marks.iter().filter(|m| k(kind_of(m))).count()
}

fn hatch_count(marks: &[Mark]) -> usize {
    count_kind(marks, |k| k == MarkKind::Hatch)
}

fn section_line_count(marks: &[Mark]) -> usize {
    count_kind(marks, |k| matches!(k, MarkKind::SectionLine(_)))
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

fn drawing(st: &AppState, d: DrawingId) -> Drawing {
    st.document.drawing(d).unwrap().clone()
}

fn sections(st: &AppState, d: DrawingId) -> Vec<planora_drawing::ViewSpec> {
    drawing(st, d)
        .views
        .into_iter()
        .filter(|v| v.section.is_some())
        .collect()
}

/// Hüllbox der Ansicht in Ansichtskoordinaten.
fn bbox(st: &AppState, d: DrawingId, view: ViewId) -> ((f64, f64), (f64, f64)) {
    let dr = drawing(st, d);
    let model = st.document.model_edit_count();
    st.drawing_ui
        .results_for(&dr, model)
        .get(&view)
        .expect("Ansicht gerechnet")
        .bbox
}

/// Blattpunkt zum Ansichtspunkt `p` der Ansicht `view`.
fn pt(st: &AppState, d: DrawingId, view: ViewId, p: (f64, f64)) -> DVec2 {
    let dr = drawing(st, d);
    let model = st.document.model_edit_count();
    let owned = st.drawing_ui.results_for(&dr, model);
    let refs: BTreeMap<ViewId, &ViewLines> = owned.iter().map(|(k, l)| (*k, l.as_ref())).collect();
    view_transform(&dr, view, &refs)
        .expect("Ansicht gezeichnet")
        .apply(DVec2::new(p.0, p.1))
}

/// Klick auf den Ansichtspunkt `p` der Ansicht `view`.
fn click_v(h: &mut Harness, st: &mut AppState, d: DrawingId, view: ViewId, p: (f64, f64)) -> Shot {
    let at = pt(st, d, view, p);
    h.click_at(st, at)
}

fn near(a: f64, b: f64, eps: f64) -> bool {
    (a - b).abs() <= eps
}

/// Quader 40 × 20 × 60, Vorne frei bei (150, 200) auf `sheet`.
fn box_front(sheet: Sheet, size: Vec2) -> (AppState, DrawingId, ViewId, Harness) {
    let mut st = AppState::new();
    st.create_box(DVec3::new(40.0, 20.0, 60.0)).unwrap();
    let d = st
        .document
        .add_drawing("Blatt 1", sheet, Projection::First)
        .unwrap();
    let front = st
        .document
        .add_view(
            d,
            ViewKind::Front,
            s11(),
            Placement::Free { x: 150.0, y: 200.0 },
            None,
        )
        .unwrap();
    let mut h = open(&mut st, d, size);
    compute(&mut st, &mut h);
    (st, d, front, h)
}

/// Legt per `create` einen waagerechten Schnitt bei der Ansichtskoordinate `y` an und rechnet.
fn cut(st: &mut AppState, h: &mut Harness, d: DrawingId, front: ViewId, y: f64) -> ViewId {
    let id = section_tool::create(st, d, front, true, y).expect("Schnitt");
    compute(st, h);
    id
}

// (a) -----------------------------------------------------------------------------------------

#[test]
fn two_clicks_make_a_section() {
    let (mut st, d, front, mut h) = box_front(A3_QUER, BIG);
    let ((x0, y0), (x1, y1)) = bbox(&st, d, front);
    let mid_y = 0.5 * (y0 + y1);
    st.section_tool.set_active(true);
    let shot = h.idle(&mut st);
    assert!(shot.has(HINT_FIRST));
    let e0 = st.document.edit_count();

    // Waagerecht knapp neben der Mitte (0,3 mm sind weit unter der Fangweite).
    click_v(&mut h, &mut st, d, front, (x0 + 5.0, mid_y + 0.3));
    assert!(st.section_tool.start().is_some());
    assert_eq!(
        st.document.edit_count(),
        e0,
        "der erste Klick ändert nichts"
    );
    assert!(sections(&st, d).is_empty());
    let shot = h.idle(&mut st);
    assert!(shot.has(HINT_SECOND));
    assert!(h.layout().preview_drawn > 0, "Vorschau mit Pfeilen");
    click_v(&mut h, &mut st, d, front, (x1 - 5.0, mid_y + 0.3));
    assert_eq!(st.document.edit_count(), e0 + 1, "genau eine Änderung");
    let made = sections(&st, d);
    assert_eq!(made.len(), 1);
    let sec = &made[0];
    assert_eq!(sec.kind, ViewKind::Top);
    let s = sec.section.unwrap();
    assert_eq!(s.letter, 'A');
    assert!(matches!(sec.placement, Placement::Aligned { .. }));
    assert_eq!(sec.parent, Some(front));
    assert_eq!(sec.scale, s11());
    assert!(!sec.show_hidden);
    assert_eq!(s.offset, mid_y, "Mitte der Hüllbox, exakt");
    assert_eq!(st.drawing_ui.selected_view, Some(sec.id), "Schnitt gewählt");
    h.idle(&mut st);
    assert!(
        !st.section_tool.is_active(),
        "mit gewähltem Schnitt ist das Werkzeug gesperrt und aus"
    );

    // Weit von der Mitte: die Lage folgt dem Klick.
    st.drawing_ui.selected_view = None;
    st.section_tool.set_active(true);
    h.idle(&mut st);
    click_v(&mut h, &mut st, d, front, (x0 + 5.0, y0 + 15.0));
    click_v(&mut h, &mut st, d, front, (x1 - 5.0, y0 + 15.0));
    let second = sections(&st, d);
    assert_eq!(second.len(), 2);
    assert_eq!(second[1].section.unwrap().letter, 'B');
    assert!(near(second[1].section.unwrap().offset, y0 + 15.0, 0.02));

    // Senkrecht: Ansicht „Links“, Lage aus der x-Koordinate.
    st.drawing_ui.selected_view = None;
    st.section_tool.set_active(true);
    h.idle(&mut st);
    click_v(&mut h, &mut st, d, front, (x0 + 10.0, y0 + 10.0));
    click_v(&mut h, &mut st, d, front, (x0 + 10.0, y0 + 40.0));
    let third = sections(&st, d);
    assert_eq!(third.len(), 3);
    assert_eq!(third[2].kind, ViewKind::Left);
    assert!(near(third[2].section.unwrap().offset, x0 + 10.0, 0.02));

    // Esc nach dem ersten Klick: nichts angelegt, das Werkzeug bleibt; ein zweites Esc beendet.
    st.drawing_ui.selected_view = None;
    st.section_tool.set_active(true);
    h.idle(&mut st);
    let e1 = st.document.edit_count();
    click_v(&mut h, &mut st, d, front, (x0 + 5.0, y0 + 30.0));
    assert!(st.section_tool.start().is_some());
    h.key(&mut st, Key::Escape);
    assert!(st.section_tool.start().is_none());
    assert!(st.section_tool.is_active());
    assert_eq!(st.document.edit_count(), e1);
    assert_eq!(sections(&st, d).len(), 3);
    h.key(&mut st, Key::Escape);
    assert!(!st.section_tool.is_active());

    // Zu kurz: zwei fast gleiche Punkte.
    st.section_tool.set_active(true);
    h.idle(&mut st);
    click_v(&mut h, &mut st, d, front, (x0 + 5.0, y0 + 30.0));
    let shot = click_v(&mut h, &mut st, d, front, (x0 + 5.2, y0 + 30.1));
    assert_eq!(st.section_tool.message(), Some(TOO_SHORT));
    assert_eq!(shot.find(TOO_SHORT).color, error_color(st.dark), "rot");
    assert_eq!(sections(&st, d).len(), 3);
}

/// Das Einrasten gilt nur innerhalb von `SNAP_PX`, auf der quer liegenden Achse.
#[test]
fn snap_rules() {
    use planora_math::DVec2 as V;
    let bbox = ((0.0, 0.0), (40.0, 60.0));
    // Waagerecht, wenn |dx| ≥ |dy| (auch bei Gleichheit).
    assert!(section_tool::snap(V::new(0.0, 0.0), V::new(10.0, 10.0)));
    assert!(section_tool::snap(V::new(0.0, 0.0), V::new(-10.0, 3.0)));
    assert!(!section_tool::snap(V::new(0.0, 0.0), V::new(3.0, 10.0)));
    assert!(!section_tool::snap(V::new(0.0, 0.0), V::new(-3.0, -10.0)));
    // 2 Pixel je Einheit: innerhalb von 8 px = 4 Einheiten rastet auf die Mitte (30).
    let (h, c) = section_tool::line_for(V::new(5.0, 27.0), V::new(30.0, 27.0), bbox, 2.0);
    assert!(h);
    assert_eq!(c, 30.0);
    let (h, c) = section_tool::line_for(V::new(5.0, 25.0), V::new(30.0, 25.0), bbox, 2.0);
    assert!(h);
    assert_eq!(c, 25.0, "5 Einheiten = 10 px sind zu weit");
    // Senkrecht: die x-Koordinate rastet auf 20.
    let (h, c) = section_tool::line_for(V::new(18.0, 5.0), V::new(18.0, 50.0), bbox, 2.0);
    assert!(!h);
    assert_eq!(c, 20.0);
    // Genau auf der Grenze (8 px) rastet nicht mehr.
    let edge = f64::from(SNAP_PX) / 2.0;
    let (_, c) = section_tool::line_for(
        V::new(0.0, 30.0 + edge),
        V::new(9.0, 30.0 + edge),
        bbox,
        2.0,
    );
    assert_eq!(c, 30.0 + edge);
}

// (b) -----------------------------------------------------------------------------------------

#[test]
fn blocked_cases() {
    let (mut st, d, front, mut h) = box_front(A3_QUER, BIG);
    let iso = st
        .document
        .add_view(
            d,
            ViewKind::Isometric,
            s11(),
            Placement::Free { x: 320.0, y: 200.0 },
            None,
        )
        .unwrap();
    compute(&mut st, &mut h);
    let dr = drawing(&st, d);
    assert_eq!(section_tool::blocked(&dr, None, true), None);
    assert_eq!(
        section_tool::blocked(&dr, dr.view(front), true),
        None,
        "gerade Ansicht gewählt"
    );
    assert_eq!(
        section_tool::blocked(&dr, dr.view(iso), true),
        Some(NOT_FROM_HERE),
        "Isometrie gewählt"
    );
    assert_eq!(
        section_tool::blocked(&dr, None, false),
        Some(READ_ONLY_TIP),
        "alte Version"
    );
    // Stücklistenblatt.
    let mut parts = Drawing::new(
        DrawingId(99),
        "Stückliste".into(),
        A3_QUER,
        Projection::First,
    );
    parts.kind = DrawingKind::PartsList { page: 1 };
    assert_eq!(
        section_tool::blocked(&parts, None, true),
        Some(PARTS_SHEET_TIP)
    );
    // Keine gerade Ansicht: nur Isometrie.
    let mut only_iso = Drawing::new(DrawingId(98), "Iso".into(), A3_QUER, Projection::First);
    only_iso.add_view(
        ViewKind::Isometric,
        s11(),
        Placement::Free { x: 1.0, y: 1.0 },
        None,
    );
    assert_eq!(
        section_tool::blocked(&only_iso, None, true),
        Some(NO_STRAIGHT_TIP)
    );

    // Schalter in der Leiste: mit gewählter Isometrie gesperrt, sonst frei.
    st.drawing_ui.selected_view = Some(iso);
    h.idle(&mut st);
    h.click_text(&mut st, TOOL);
    assert!(!st.section_tool.is_active(), "Knopf bei Isometrie gesperrt");
    st.drawing_ui.selected_view = None;
    h.idle(&mut st);
    h.click_text(&mut st, TOOL);
    assert!(st.section_tool.is_active(), "Knopf frei ohne Auswahl");
    // Klick in die Isometrie: Ablehnung, nichts gemerkt.
    let iso_pt = h
        .layout()
        .views
        .iter()
        .find(|v| v.id == iso)
        .unwrap()
        .rect
        .center();
    let e0 = st.document.edit_count();
    let shot = h.click(&mut st, iso_pt);
    assert_eq!(st.section_tool.message(), Some(NOT_FROM_HERE));
    assert_eq!(shot.find(NOT_FROM_HERE).color, error_color(st.dark));
    assert!(st.section_tool.start().is_none());
    assert_eq!(st.document.edit_count(), e0);
    st.section_tool.set_active(false);

    // Gewählter Schnitt: Knopf gesperrt.
    let sec = cut(&mut st, &mut h, d, front, 30.0);
    st.drawing_ui.selected_view = Some(sec);
    h.idle(&mut st);
    let dr = drawing(&st, d);
    assert_eq!(
        section_tool::blocked(&dr, dr.view(sec), true),
        Some(NOT_FROM_HERE)
    );
    h.click_text(&mut st, TOOL);
    assert!(!st.section_tool.is_active(), "Knopf bei Schnitt gesperrt");

    // Alte Version: das eingeschaltete Werkzeug geht aus.
    st.drawing_ui.selected_view = None;
    st.section_tool.set_active(true);
    h.editable = false;
    h.idle(&mut st);
    assert!(!st.section_tool.is_active());
    h.editable = true;

    // 12 Schnitte sind das Äußerste.
    for i in 1..12 {
        section_tool::create(&mut st, d, front, true, 3.0 + 4.0 * f64::from(i)).unwrap();
    }
    let dr = drawing(&st, d);
    assert_eq!(sections(&st, d).len(), 12);
    assert_eq!(section_tool::blocked(&dr, None, true), Some(FULL));
    let before = st.document.edit_count();
    assert_eq!(
        section_tool::create(&mut st, d, front, true, 5.0),
        Err(FULL.to_string())
    );
    assert_eq!(st.document.edit_count(), before);
    st.drawing_ui.selected_view = None;
    h.idle(&mut st);
    h.click_text(&mut st, TOOL);
    assert!(
        !st.section_tool.is_active(),
        "Knopf bei 12 Schnitten gesperrt"
    );
}

// (c) -----------------------------------------------------------------------------------------

#[test]
fn letters_in_order() {
    let (mut st, d, front, mut h) = box_front(A3_QUER, BIG);
    let a = cut(&mut st, &mut h, d, front, 10.0);
    let b = cut(&mut st, &mut h, d, front, 30.0);
    let c = cut(&mut st, &mut h, d, front, 50.0);
    let letter = |st: &AppState, v: ViewId| {
        st.document
            .drawing(d)
            .unwrap()
            .view(v)
            .unwrap()
            .section
            .unwrap()
            .letter
    };
    assert_eq!(
        [letter(&st, a), letter(&st, b), letter(&st, c)],
        ['A', 'B', 'C']
    );
    // Gleiche Richtung (alle „Oben“): jede neue Ansicht liegt hinter der vorigen, ohne Überlappung.
    let dist = |v: ViewId| match st.document.drawing(d).unwrap().view(v).unwrap().placement {
        Placement::Aligned { distance } => distance,
        Placement::Free { .. } => panic!("Schnitt muss ausgerichtet sein"),
    };
    assert!(
        dist(a) < dist(b) && dist(b) < dist(c),
        "{} {} {}",
        dist(a),
        dist(b),
        dist(c)
    );
    for (x, y) in [(a, b), (b, c)] {
        let (rx, ry) = (h.rect_of(x), h.rect_of(y));
        assert!(!rx.intersects(ry), "Schnitte überlappen: {rx:?} {ry:?}");
    }
    // B löschen (mit Rückfrage), der nächste Schnitt bekommt wieder B.
    drawing_browser::request_delete_view(&mut st, d, b);
    let plan = st.pending_delete_drawing.clone().expect("Rückfrage");
    drawing_browser::apply_delete(&mut st, &plan).unwrap();
    assert_eq!(sections(&st, d).len(), 2);
    let again = cut(&mut st, &mut h, d, front, 40.0);
    assert_eq!(letter(&st, again), 'B');
    assert_eq!(
        drawing(&st, d).view_names().get(&again).map(String::as_str),
        Some("Schnitt B\u{2013}B")
    );
}

// (d) -----------------------------------------------------------------------------------------

#[test]
fn properties_edit() {
    let (mut st, d, front, mut h) = box_front(A3_QUER, BIG);
    let sec = cut(&mut st, &mut h, d, front, 30.0);
    let spec = |st: &AppState| st.document.drawing(d).unwrap().view(sec).unwrap().clone();

    // Schnittposition „12,5“: neuer Auftrag.
    let e0 = st.document.edit_count();
    section_tool::set_offset_text(&mut st, d, sec, "12,5").unwrap();
    assert_eq!(spec(&st).section.unwrap().offset, 12.5);
    assert_eq!(st.document.edit_count(), e0 + 1);
    compute(&mut st, &mut h);
    // „abc“: Meldung, unverändert.
    let before = spec(&st);
    assert_eq!(
        section_tool::set_offset_text(&mut st, d, sec, "abc"),
        Err(NOT_A_NUMBER.to_string())
    );
    assert_eq!(spec(&st), before);
    assert_eq!(st.document.edit_count(), e0 + 1);

    // Umdrehen: Unten, Lage der Ebene bleibt, die Ansicht wandert auf die andere Seite.
    let below = h.rect_of(sec).center().y > h.rect_of(front).center().y;
    assert!(below, "Oben liegt im Erstwinkel unter Vorne");
    let e1 = st.document.edit_count();
    section_tool::flip(&mut st, d, sec).unwrap();
    assert_eq!(st.document.edit_count(), e1 + 1);
    assert_eq!(spec(&st).kind, ViewKind::Bottom);
    assert_eq!(spec(&st).section.unwrap().offset, 12.5);
    compute(&mut st, &mut h);
    assert!(h.rect_of(sec).center().y < h.rect_of(front).center().y);
    section_tool::flip(&mut st, d, sec).unwrap();
    assert_eq!(spec(&st).kind, ViewKind::Top);
    compute(&mut st, &mut h);

    // Schraffur: aus → keine Hatch-Marken, an → wieder da.
    assert!(hatch_count(&h.marks(NAME)) > 0);
    section_tool::set_hatch(&mut st, d, sec, false).unwrap();
    h.idle(&mut st);
    assert_eq!(hatch_count(&h.marks(NAME)), 0);
    section_tool::set_hatch(&mut st, d, sec, true).unwrap();
    h.idle(&mut st);
    assert!(hatch_count(&h.marks(NAME)) > 0);

    // Abstand: 5 wird übernommen (weniger Linien), 0,3 auf A3 ist zu fein.
    let dense = hatch_count(&h.marks(NAME));
    section_tool::set_spacing_text(&mut st, d, sec, "5").unwrap();
    h.idle(&mut st);
    assert!(hatch_count(&h.marks(NAME)) < dense);
    let before = spec(&st);
    let e2 = st.document.edit_count();
    assert_eq!(
        section_tool::set_spacing_text(&mut st, d, sec, "0,3"),
        Err(format!(
            "Der Schraffurabstand von „{NAME}“ wäre auf diesem Blatt kleiner als 0,5 mm."
        ))
    );
    assert_eq!(spec(&st), before);
    assert_eq!(st.document.edit_count(), e2);
    assert_eq!(
        section_tool::set_spacing_text(&mut st, d, sec, "xyz"),
        Err(NOT_A_NUMBER.to_string())
    );
    assert!(section_tool::set_spacing_text(&mut st, d, sec, "99").is_err());
    assert_eq!(spec(&st), before);
}

/// Das Ansichtsfeld einer Schnittansicht mit Maus und Tastatur: Position mit Enter übernehmen,
/// ungültige Eingabe rot melden, Umdrehen, Schraffur, gesperrter Maßstab.
#[test]
fn view_field_edits_with_events() {
    let (mut st, d, front, mut h) = box_front(A3_QUER, BIG);
    let sec = cut(&mut st, &mut h, d, front, 30.0);
    st.drawing_ui.selected_view = Some(sec);
    let shot = h.idle(&mut st);
    assert!(shot.has(NAME), "Kopf „{NAME}“");
    assert!(shot.has("Schnittposition (mm)"));
    assert!(shot.has("Blickrichtung umdrehen"));
    assert!(shot.has("Schraffur"));
    assert!(shot.has("Schraffurabstand (mm)"));
    assert!(
        !shot.has("Ansicht „Schnitt A\u{2013}A“"),
        "Kopf ohne Zusatz"
    );
    let spec = |st: &AppState| st.document.drawing(d).unwrap().view(sec).unwrap().clone();

    let field_of = |shot: &Shot, label: &str| {
        let l = shot.find(label).rect;
        pos2(l.left() + 20.0, l.bottom() + 17.0)
    };
    // Schnittposition: Feld anklicken, alles ersetzen, Enter.
    let p = field_of(&shot, "Schnittposition (mm)");
    h.click(&mut st, p);
    let [a_down, a_up] = key_events(Key::A, Modifiers::COMMAND);
    h.frame(&mut st, vec![a_down.clone(), a_up.clone()]);
    h.frame(&mut st, vec![Event::Text("7,5".into())]);
    let e0 = st.document.edit_count();
    h.key(&mut st, Key::Enter);
    assert_eq!(spec(&st).section.unwrap().offset, 7.5);
    assert_eq!(st.document.edit_count(), e0 + 1);
    let shot = h.idle(&mut st);
    assert!(shot.has("7,5"), "Feld zeigt den Wert");

    // Ungültig: rote Meldung, Dokument unverändert, Feld zeigt wieder den alten Wert.
    let p = field_of(&shot, "Schnittposition (mm)");
    h.click(&mut st, p);
    h.frame(&mut st, vec![a_down.clone(), a_up.clone()]);
    h.frame(&mut st, vec![Event::Text("abc".into())]);
    let e1 = st.document.edit_count();
    let shot = h.key(&mut st, Key::Enter);
    assert_eq!(st.document.edit_count(), e1);
    assert_eq!(spec(&st).section.unwrap().offset, 7.5);
    assert_eq!(st.section_tool.message(), Some(NOT_A_NUMBER));
    assert_eq!(shot.find(NOT_A_NUMBER).color, error_color(st.dark));
    assert!(shot.has("7,5"));
    assert!(!shot.has("abc"));

    // Umdrehen per Knopf.
    let e2 = st.document.edit_count();
    h.click_text(&mut st, "Blickrichtung umdrehen");
    assert_eq!(spec(&st).kind, ViewKind::Bottom);
    assert_eq!(st.document.edit_count(), e2 + 1);
    assert_eq!(
        st.section_tool.message(),
        None,
        "Erfolg räumt die Meldung ab"
    );

    // Schalter „Schraffur“.
    compute(&mut st, &mut h);
    assert!(spec(&st).section.unwrap().hatch.on);
    h.click_text(&mut st, "Schraffur");
    assert!(!spec(&st).section.unwrap().hatch.on);

    // Maßstab gesperrt: der Wert ist da, die Auswahl ändert sich nicht.
    let shot = h.idle(&mut st);
    assert!(shot.has("Maßstab"));
    assert!(shot.has("1:1"));
}

// (j) -----------------------------------------------------------------------------------------

/// „Ansicht hinzufügen“ hängt nie eine Ansicht unter eine Schnittansicht.
#[test]
fn new_views_never_hang_under_a_section() {
    let (mut st, d, front, mut h) = box_front(A3_QUER, BIG);
    let sec = cut(&mut st, &mut h, d, front, 30.0);
    st.drawing_ui.selected_view = Some(sec);
    let new = drawing_view::add_view(&mut st, ViewKind::Left).unwrap();
    let dr = drawing(&st, d);
    let v = dr.view(new).unwrap();
    assert_eq!(
        v.parent,
        Some(front),
        "unter der Quelle, nicht unter dem Schnitt"
    );
    assert!(v.section.is_none());
    assert!(dr.views.iter().all(|x| x.parent != Some(sec)));
}

/// „Maß“, „Position“ und „Schnitt“ schließen sich aus.
#[test]
fn tools_exclude_each_other() {
    let (mut st, _, _, mut h) = box_front(A3_QUER, BIG);
    h.click_text(&mut st, TOOL);
    assert!(st.section_tool.is_active());
    h.click_text(&mut st, planora_app::dimension_tool::TOOL);
    assert!(st.dimension_tool.is_active());
    assert!(!st.section_tool.is_active(), "Maß schaltet Schnitt aus");
    h.click_text(&mut st, TOOL);
    assert!(st.section_tool.is_active());
    assert!(!st.dimension_tool.is_active(), "Schnitt schaltet Maß aus");
    h.click_text(&mut st, planora_app::balloon_tool::TOOL);
    assert!(st.balloon_tool.is_active());
    assert!(
        !st.section_tool.is_active(),
        "Position schaltet Schnitt aus"
    );
    h.click_text(&mut st, TOOL);
    assert!(st.section_tool.is_active());
    assert!(!st.balloon_tool.is_active());
    // Der Knopf schaltet auch wieder aus.
    h.click_text(&mut st, TOOL);
    assert!(!st.section_tool.is_active());
}

// (k) Nachbesserung: Maßstab über die Leiste, Vorzeichen, Abstände, Esc, Fangweite, Vorschau -----

/// Mitte des Auswahlfelds mit dem Text `text` im Ansichtsfeld (das rechteste; das Schriftfeld
/// trägt denselben Text weiter links).
fn combo_pos(shot: &Shot, text: &str) -> Pos2 {
    shot.texts
        .iter()
        .filter(|t| t.text == text)
        .max_by(|a, b| a.rect.left().total_cmp(&b.rect.left()))
        .unwrap_or_else(|| panic!("„{text}“ fehlt"))
        .rect
        .center()
}

fn scale_of(st: &AppState, d: DrawingId, v: ViewId) -> Scale {
    st.document.drawing(d).unwrap().view(v).unwrap().scale
}

/// Der Maßstab der Quelle über das Auswahlfeld im Ansichtsfeld: der Schnitt folgt in derselben
/// Änderung; im Ansichtsfeld der Schnittansicht ist die Auswahl gesperrt.
#[test]
fn scale_combo_forwards_and_is_locked_on_a_section() {
    let (mut st, d, front, mut h) = box_front(A3_QUER, BIG);
    let sec = cut(&mut st, &mut h, d, front, 30.0);
    let half = Scale { num: 1, den: 2 };
    st.drawing_ui.selected_view = Some(front);
    h.idle(&mut st);
    let e0 = st.document.edit_count();
    let p = combo_pos(&h.idle(&mut st), "1:1");
    h.click(&mut st, p);
    h.click_text(&mut st, "1:2");
    assert_eq!(scale_of(&st, d, front), half);
    assert_eq!(scale_of(&st, d, sec), half, "Schnitt folgt über die Leiste");
    assert_eq!(st.document.edit_count(), e0 + 1, "eine Änderung");

    // Schnittansicht gewählt: das Auswahlfeld öffnet sich nicht, der Maßstab bleibt.
    st.drawing_ui.selected_view = Some(sec);
    h.idle(&mut st);
    let e1 = st.document.edit_count();
    let p = combo_pos(&h.idle(&mut st), "1:2");
    let shot = h.click(&mut st, p);
    assert!(!shot.has("1:5"), "keine Auswahl geöffnet");
    assert!(!shot.has("2:1"), "keine Auswahl geöffnet");
    assert_eq!(scale_of(&st, d, sec), half);
    assert_eq!(st.document.edit_count(), e1);
    // Der Tooltip erklärt, warum.
    let p = combo_pos(&shot, "1:2");
    h.frame(&mut st, vec![Event::PointerMoved(p)]);
    h.frame(&mut st, vec![Event::PointerMoved(p)]);
    let shot = h.frame(&mut st, vec![Event::PointerMoved(p)]);
    assert!(shot.has(section_tool::SCALE_LOCKED_TIP));
}

/// Lage der Ebene aus den Klicks, auch auf der negativen Seite und bei einer Quellansicht, deren
/// Achse gegen die Weltachse läuft.
#[test]
fn offset_signs_from_clicks() {
    let mut st = AppState::new();
    st.add_feature(
        None,
        FeatureKind::Box {
            size: DVec3::new(40.0, 20.0, 60.0),
            origin: DVec3::new(-50.0, -10.0, -30.0),
        },
    )
    .unwrap();
    st.refresh();
    let d = st
        .document
        .add_drawing("Blatt 1", A3_QUER, Projection::First)
        .unwrap();
    let mut views = Vec::new();
    for (kind, x) in [(ViewKind::Front, 100.0), (ViewKind::Back, 300.0)] {
        views.push(
            st.document
                .add_view(d, kind, s11(), Placement::Free { x, y: 200.0 }, None)
                .unwrap(),
        );
    }
    let mut h = open(&mut st, d, BIG);
    compute(&mut st, &mut h);
    let offset_of = |st: &AppState, i: usize| sections(st, d)[i].section.unwrap().offset;
    let click_pair =
        |st: &mut AppState, h: &mut Harness, v: ViewId, a: (f64, f64), b: (f64, f64)| {
            st.drawing_ui.selected_view = None;
            st.section_tool.set_active(true);
            h.idle(st);
            click_v(h, st, d, v, a);
            click_v(h, st, d, v, b);
        };
    let ((x0, y0), (x1, y1)) = bbox(&st, d, views[0]);
    // Vorne, senkrecht links der Mitte: Welt-x = Ansichts-x (negativ).
    click_pair(
        &mut st,
        &mut h,
        views[0],
        (x0 + 10.0, y0 + 5.0),
        (x0 + 10.0, y0 + 40.0),
    );
    assert_eq!(sections(&st, d)[0].kind, ViewKind::Left);
    assert!(
        near(offset_of(&st, 0), x0 + 10.0, 0.02),
        "{}",
        offset_of(&st, 0)
    );
    assert!(offset_of(&st, 0) < 0.0);
    // Vorne, waagerecht unter der Mitte: Welt-z = Ansichts-y (negativ).
    click_pair(
        &mut st,
        &mut h,
        views[0],
        (x0 + 5.0, y0 + 10.0),
        (x1 - 5.0, y0 + 10.0),
    );
    assert_eq!(sections(&st, d)[1].kind, ViewKind::Top);
    assert!(
        near(offset_of(&st, 1), y0 + 10.0, 0.02),
        "{}",
        offset_of(&st, 1)
    );
    assert!(offset_of(&st, 1) < 0.0 && y1 > 0.0);
    // Hinten: die Ansichts-x-Achse läuft gegen die Welt-x-Achse.
    let ((bx0, by0), _) = bbox(&st, d, views[1]);
    click_pair(
        &mut st,
        &mut h,
        views[1],
        (bx0 + 10.0, by0 + 5.0),
        (bx0 + 10.0, by0 + 40.0),
    );
    let back_sec = sections(&st, d)[2].clone();
    assert!(
        near(back_sec.section.unwrap().offset, -(bx0 + 10.0), 0.02),
        "{} gegen {}",
        back_sec.section.unwrap().offset,
        bx0 + 10.0
    );
}

/// Abstand hinter Geschwistern: Ausdehnung mal Maßstab der Quelle; Geschwister in anderer Richtung
/// zählen nicht.
#[test]
fn sibling_distance_uses_source_scale_and_direction() {
    use planora_app::drawing_state::estimate_view_size;
    let (mut st, d, front, mut h) = box_front(A3_QUER, BIG);
    section_tool::set_scale(&mut st, d, front, Scale { num: 1, den: 2 }).unwrap();
    let dist =
        |st: &AppState, v: ViewId| match st.document.drawing(d).unwrap().view(v).unwrap().placement
        {
            Placement::Aligned { distance } => distance,
            Placement::Free { .. } => panic!("nicht ausgerichtet"),
        };
    let a = cut(&mut st, &mut h, d, front, 10.0);
    let b = cut(&mut st, &mut h, d, front, 40.0);
    let ext = estimate_view_size(&st.bodies, ViewKind::Top).1;
    let expected = dist(&st, a) + ext * 0.5 + planora_drawing::GAP_MM;
    assert!(
        near(dist(&st, b), expected, 1e-9),
        "{} gegen {expected}",
        dist(&st, b)
    );
    // Ein weit entferntes Geschwister in anderer Richtung (Links, waagerecht) zählt nicht.
    let side = section_tool::create(&mut st, d, front, false, 20.0).unwrap();
    st.document
        .update_drawing(d, |dr| {
            if let Some(v) = dr.views.iter_mut().find(|v| v.id == side) {
                v.placement = Placement::Aligned { distance: 500.0 };
            }
        })
        .unwrap();
    let c = section_tool::create(&mut st, d, front, true, 50.0).unwrap();
    let expected = dist(&st, b) + ext * 0.5 + planora_drawing::GAP_MM;
    assert!(
        near(dist(&st, c), expected, 1e-9),
        "{} gegen {expected}",
        dist(&st, c)
    );
}

/// Esc im Textfeld bricht weder die Auswahl noch sonst etwas ab.
#[test]
fn escape_while_typing_keeps_the_selection() {
    let (mut st, d, front, mut h) = box_front(A3_QUER, BIG);
    let sec = cut(&mut st, &mut h, d, front, 30.0);
    st.drawing_ui.selected_view = Some(sec);
    let shot = h.idle(&mut st);
    let l = shot.find("Schnittposition (mm)").rect;
    h.click(&mut st, pos2(l.left() + 20.0, l.bottom() + 17.0));
    let e0 = st.document.edit_count();
    h.frame(&mut st, vec![Event::Text("9".into())]);
    h.key(&mut st, Key::Escape);
    assert_eq!(st.drawing_ui.selected_view, Some(sec), "Auswahl bleibt");
    assert_eq!(st.document.edit_count(), e0, "Esc verwirft die Eingabe");
    let offset = st
        .document
        .drawing(d)
        .unwrap()
        .view(sec)
        .unwrap()
        .section
        .unwrap()
        .offset;
    assert_eq!(offset, 30.0);
    // Ohne Fokus im Feld hebt Esc die Auswahl auf.
    h.key(&mut st, Key::Escape);
    assert_eq!(st.drawing_ui.selected_view, None);
}

/// Fangweite für die Schnittlinie: 5 px treffen, 12 px nicht.
#[test]
fn section_line_pick_radius() {
    let (mut st, d, front, mut h) = box_front(A3_QUER, BIG);
    let sec = cut(&mut st, &mut h, d, front, 30.0);
    h.idle(&mut st);
    let dr = drawing(&st, d);
    let model = st.document.model_edit_count();
    let owned = st.drawing_ui.results_for(&dr, model);
    let refs: BTreeMap<ViewId, &ViewLines> = owned.iter().map(|(k, l)| (*k, l.as_ref())).collect();
    let (_, a, b) = section_tool::section_lines(&dr, &refs)[0];
    let m = h.screen((a + b) * 0.5);
    st.drawing_ui.selected_view = None;
    h.click(&mut st, m + vec2(0.0, 5.0));
    assert_eq!(st.drawing_ui.selected_view, Some(sec), "5 px treffen");
    st.drawing_ui.selected_view = None;
    h.click(&mut st, m + vec2(0.0, 12.0));
    assert_eq!(
        st.drawing_ui.selected_view,
        Some(front),
        "12 px treffen nicht"
    );
}

/// Die Vorschau zeigt die Pfeile in Blickrichtung: waagerechte Linie nach unten, senkrechte nach
/// rechts; die Pfeile zeigen gegen die Blickrichtung.
#[test]
fn preview_arrows_point_the_way() {
    let (mut st, d, front, mut h) = box_front(A3_QUER, BIG);
    let ((x0, y0), (x1, _)) = bbox(&st, d, front);
    let k = f64::from(h.layout().px_per_mm);
    let arrows = |marks: &[Mark]| {
        marks
            .iter()
            .filter_map(|m| match m {
                Mark::Arrow { tip, dir, .. } => Some((*tip, *dir)),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    st.section_tool.set_active(true);
    h.idle(&mut st);
    click_v(&mut h, &mut st, d, front, (x0 + 5.0, y0 + 15.0));
    let at = pt(&st, d, front, (x1 - 5.0, y0 + 15.0));
    let marks = section_tool::preview(&st, d, at, k);
    let line_y = pt(&st, d, front, (x0, y0 + 15.0)).y;
    let list = arrows(&marks);
    assert_eq!(list.len(), 2);
    for (tip, dir) in list {
        assert_eq!(
            dir,
            DVec2::new(0.0, 1.0),
            "zeigt gegen die Blickrichtung (nach unten)"
        );
        assert!(tip.y < line_y, "Spitze liegt unter der Linie");
    }
    // Senkrecht: Blick nach rechts, Pfeile zeigen nach links.
    st.section_tool.cancel();
    click_v(&mut h, &mut st, d, front, (x0 + 10.0, y0 + 10.0));
    let at = pt(&st, d, front, (x0 + 10.0, y0 + 40.0));
    let marks = section_tool::preview(&st, d, at, k);
    let line_x = pt(&st, d, front, (x0 + 10.0, y0)).x;
    let list = arrows(&marks);
    assert_eq!(list.len(), 2);
    for (tip, dir) in list {
        assert_eq!(dir, DVec2::new(-1.0, 0.0));
        assert!(tip.x > line_x, "Spitze liegt rechts der Linie");
    }
}

/// Rückfrage beim Löschen einer Schnittansicht mit Maßen: einfacher Satz ohne Komma vor „und“.
#[test]
fn delete_question_with_dimensions_reads_plainly() {
    use planora_drawing::dimension::{
        DimId, DimKind, DimSpec, GeoRef, GeoRefData, TagData, Tolerance as DimTolerance,
    };
    let (mut st, d, front, mut h) = box_front(A3_QUER, BIG);
    let sec = cut(&mut st, &mut h, d, front, 30.0);
    for _ in 0..2 {
        let spec = DimSpec {
            id: DimId(0),
            view: sec,
            kind: DimKind::Diameter,
            refs: vec![GeoRef::Face(GeoRefData::Face(TagData {
                origin: 1,
                role: "Mantel".into(),
                hint: DVec3::ZERO,
            }))],
            offset: 10.0,
            angle: 45.0,
            text_shift: (0.0, 0.0),
            tolerance: DimTolerance::None,
            decimals: 1,
            text_override: None,
            chain: None,
            suggested: false,
        };
        st.document.add_dimension(d, spec).unwrap();
    }
    drawing_browser::request_delete_view(&mut st, d, sec);
    let plan = st.pending_delete_drawing.clone().expect("Rückfrage");
    assert_eq!(
        plan.text,
        "Die Schnittansicht „Schnitt A\u{2013}A“ wird gelöscht. Mit ihr verschwinden die Schnittlinie in „Vorne“ und 2 Maße."
    );
}

// (e) -----------------------------------------------------------------------------------------

#[test]
fn delete_both_ways() {
    let (mut st, d, front, mut h) = box_front(A3_QUER, BIG);
    let sec = cut(&mut st, &mut h, d, front, 30.0);
    h.idle(&mut st);
    assert!(
        section_line_count(&h.marks("Vorne")) > 0,
        "Schnittlinie in Vorne"
    );

    // Schnittansicht löschen: immer mit Rückfrage, die Schnittlinie geht mit.
    let e0 = st.document.edit_count();
    drawing_browser::request_delete_view(&mut st, d, sec);
    let plan = st.pending_delete_drawing.clone().expect("Rückfrage");
    assert_eq!(
        plan.text,
        "Die Schnittansicht „Schnitt A\u{2013}A“ wird gelöscht. Mit ihr verschwindet die Schnittlinie in „Vorne“."
    );
    assert_eq!(st.document.edit_count(), e0, "noch nichts gelöscht");
    assert_eq!(drawing(&st, d).views.len(), 2);
    drawing_browser::apply_delete(&mut st, &plan).unwrap();
    assert_eq!(drawing(&st, d).views.len(), 1);
    assert_eq!(st.document.edit_count(), e0 + 1);
    compute(&mut st, &mut h);
    assert_eq!(section_line_count(&h.marks("Vorne")), 0, "Schnittlinie weg");

    // Die Quelle löschen: die Rückfrage nennt „Schnitt A–A“, beide gehen.
    let sec = cut(&mut st, &mut h, d, front, 30.0);
    let _ = sec;
    drawing_browser::request_delete_view(&mut st, d, front);
    let plan = st.pending_delete_drawing.clone().expect("Rückfrage");
    assert!(plan.text.contains("Schnitt A\u{2013}A"), "{}", plan.text);
    drawing_browser::apply_delete(&mut st, &plan).unwrap();
    assert!(drawing(&st, d).views.is_empty());
}

#[test]
fn click_on_section_line_selects_the_section() {
    let (mut st, d, front, mut h) = box_front(A3_QUER, BIG);
    let sec = cut(&mut st, &mut h, d, front, 30.0);
    h.idle(&mut st);
    st.drawing_ui.selected_view = None;
    let dr = drawing(&st, d);
    let model = st.document.model_edit_count();
    let owned = st.drawing_ui.results_for(&dr, model);
    let refs: BTreeMap<ViewId, &ViewLines> = owned.iter().map(|(k, l)| (*k, l.as_ref())).collect();
    let lines = section_tool::section_lines(&dr, &refs);
    assert_eq!(lines.len(), 1);
    let (id, a, b) = lines[0];
    assert_eq!(id, sec);
    // Ein Klick auf die Mitte der Schnittlinie (innerhalb von Vorne) wählt den Schnitt …
    h.click_at(&mut st, (a + b) * 0.5);
    assert_eq!(st.drawing_ui.selected_view, Some(sec));
    // … ein Klick in Vorne abseits der Linie wählt Vorne.
    let ((x0, y0), _) = bbox(&st, d, front);
    click_v(&mut h, &mut st, d, front, (x0 + 20.0, y0 + 8.0));
    assert_eq!(st.drawing_ui.selected_view, Some(front));
}

// (f) -----------------------------------------------------------------------------------------

#[test]
fn nudge_note() {
    // Stufenkörper: unten 40 × 20 × 30, oben 20 × 20 × 30; die Stufenfläche liegt bei z = 30,
    // genau in der Mitte der Hüllbox.
    let mut st = AppState::new();
    for (size, origin) in [
        (DVec3::new(40.0, 20.0, 30.0), DVec3::ZERO),
        (DVec3::new(20.0, 20.0, 30.0), DVec3::new(0.0, 0.0, 30.0)),
    ] {
        st.add_feature(None, FeatureKind::Box { size, origin })
            .unwrap();
    }
    st.refresh();
    st.add_boolean(0, 1, BoolOp::Union).unwrap();
    st.refresh();
    assert_eq!(st.bodies.len(), 1);
    let d = st
        .document
        .add_drawing("Blatt 1", A3_QUER, Projection::First)
        .unwrap();
    let front = st
        .document
        .add_view(
            d,
            ViewKind::Front,
            s11(),
            Placement::Free { x: 150.0, y: 200.0 },
            None,
        )
        .unwrap();
    let mut h = open(&mut st, d, BIG);
    compute(&mut st, &mut h);
    let ((_, y0), (_, y1)) = bbox(&st, d, front);
    assert!(near(0.5 * (y0 + y1), 30.0, 1e-6));
    let sec = section_tool::create(&mut st, d, front, true, 0.5 * (y0 + y1)).unwrap();
    compute(&mut st, &mut h);
    st.drawing_ui.selected_view = Some(sec);
    let shot = h.idle(&mut st);
    assert!(
        shot.has(NUDGED),
        "Hinweis fehlt: {:?}",
        shot.texts.iter().map(|t| &t.text).collect::<Vec<_>>()
    );
    // Eine Ebene mitten durch Material braucht keinen Hinweis.
    section_tool::set_offset_text(&mut st, d, sec, "12").unwrap();
    compute(&mut st, &mut h);
    assert!(!h.idle(&mut st).has(NUDGED));
}

// (g) -----------------------------------------------------------------------------------------

#[test]
fn scale_and_sheet() {
    let (mut st, d, front, mut h) = box_front(A3_QUER, BIG);
    let sec = cut(&mut st, &mut h, d, front, 30.0);
    let scale_of =
        |st: &AppState, v: ViewId| st.document.drawing(d).unwrap().view(v).unwrap().scale;
    let half = Scale { num: 1, den: 2 };
    // Maßstab der Quelle: der Schnitt folgt in derselben Änderung.
    let e0 = st.document.edit_count();
    section_tool::set_scale(&mut st, d, front, half).unwrap();
    assert_eq!(st.document.edit_count(), e0 + 1);
    assert_eq!(scale_of(&st, front), half);
    assert_eq!(scale_of(&st, sec), half);
    // Über die Leiste: Vorne wählen und den Maßstab im Ansichtsfeld ändern.
    st.drawing_ui.selected_view = Some(front);
    h.idle(&mut st);
    let two = Scale { num: 2, den: 1 };
    section_tool::set_scale(&mut st, d, front, two).unwrap();
    assert_eq!(scale_of(&st, sec), two);
    section_tool::set_scale(&mut st, d, front, s11()).unwrap();

    // Allein den Maßstab der Quelle zu ändern, lehnt das Dokument ab (Schnitt folgt der Quelle).
    let before = drawing(&st, d);
    assert!(
        st.document
            .update_drawing(d, |dr| dr.views[0].scale = half)
            .is_err()
    );
    assert_eq!(drawing(&st, d), before);

    // Blattwechsel A3 → A4 mit Abstand 0,6: abgelehnt, Blatt unverändert.
    section_tool::set_spacing_text(&mut st, d, sec, "0,6").unwrap();
    let r = st.document.update_drawing(d, |dr| dr.sheet = A4_QUER);
    let msg = r.expect_err("zu fein auf A4").to_string();
    assert!(
        msg.contains(&format!(
            "Der Schraffurabstand von „{NAME}“ wäre auf diesem Blatt kleiner als 0,5 mm."
        )),
        "{msg}"
    );
    assert_eq!(drawing(&st, d).sheet, A3_QUER);
    // Mit 1 mm geht A4.
    section_tool::set_spacing_text(&mut st, d, sec, "1").unwrap();
    st.document
        .update_drawing(d, |dr| dr.sheet = A4_QUER)
        .unwrap();
    assert_eq!(drawing(&st, d).sheet, A4_QUER);
}

// (h) -----------------------------------------------------------------------------------------

#[test]
fn save_load() {
    let (mut st, d, front, mut h) = box_front(A3_QUER, BIG);
    // Eine Positionsnummer macht die Datei ohne Schnitt zu Format 7.
    let key = BodyKey {
        feature: st.document.features()[0].id.0,
        index: 0,
    };
    st.document
        .add_balloon(d, front, key, (0.5, 0.5), (5.0, 5.0))
        .unwrap();
    let sec = cut(&mut st, &mut h, d, front, 30.0);
    section_tool::set_spacing_text(&mut st, d, sec, "4").unwrap();
    compute(&mut st, &mut h);
    let snapshot = st.document.snapshot();
    assert_eq!(planora_project::required_format(&snapshot), 8);
    let text = planora_project::encode_version(&snapshot).unwrap();
    assert!(text.contains("\"format\": 8"), "Format 8 in der Datei");
    let back = planora_project::decode_version(&text, "Probe").unwrap();
    let mut loaded = AppState::from_snapshot(back).unwrap();
    assert_eq!(
        loaded.document.drawing(d).unwrap(),
        st.document.drawing(d).unwrap(),
        "gleiche Zeichnung"
    );
    let mut h2 = open(&mut loaded, d, BIG);
    compute(&mut loaded, &mut h2);
    let (a, b) = (h.marks(NAME), h2.marks(NAME));
    assert!(hatch_count(&a) > 0);
    assert_eq!(a, b, "gleiche Schraffur und Titel nach dem Laden");
    assert_eq!(h.marks("Vorne"), h2.marks("Vorne"), "gleiche Schnittlinie");

    // Schnitt gelöscht: wieder Format 7.
    drawing_browser::request_delete_view(&mut st, d, sec);
    let plan = st.pending_delete_drawing.clone().unwrap();
    drawing_browser::apply_delete(&mut st, &plan).unwrap();
    let snapshot = st.document.snapshot();
    assert_eq!(planora_project::required_format(&snapshot), 7);
    let text = planora_project::encode_version(&snapshot).unwrap();
    assert!(text.contains("\"format\": 7"));
}

// (i) -----------------------------------------------------------------------------------------

fn library_ws() -> Workspace {
    let root = common::temp_root("planora-section-tool");
    Workspace::new(Some(Library::open(&root).unwrap()), Config::default())
}

thread_local! {
    static TIME: std::cell::Cell<f64> = const { std::cell::Cell::new(0.0) };
}

struct App {
    c: egui::Context,
    app: PlanoraApp,
    size: Vec2,
}

impl App {
    fn new(size: Vec2, dark: bool, mut st: AppState) -> App {
        let c = context();
        theme::apply(&c, dark);
        let mut app = PlanoraApp::for_tests(library_ws());
        st.drawing_ui.tick(&st.bodies, &st.document, true);
        st.dark = dark;
        app.ws_mut().active_tab_mut().state = st;
        App { c, app, size }
    }

    fn frame(&mut self) -> Shot {
        let time = TIME.with(|t| {
            t.set(t.get() + 1.0 / 60.0);
            t.get()
        });
        let raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, self.size)),
            time: Some(time),
            ..Default::default()
        };
        let mut f = eframe::Frame::_new_kittest();
        let out = self.c.run_ui(raw, |ui| self.app.ui(ui, &mut f));
        let shot = Shot::of(&out);
        out.drop_without_applying_deltas();
        shot
    }

    fn settle(&mut self) -> Shot {
        for _ in 0..3 {
            self.frame();
        }
        self.frame()
    }
}

fn field_rect(c: &egui::Context, name: &str) -> Rect {
    shell_panels::field_rects(c)
        .into_iter()
        .find(|(n, _)| *n == name)
        .unwrap_or_else(|| panic!("{name}"))
        .1
}

fn scrollbar_in(shot: &Shot, tool: Rect) -> bool {
    let band = Rect::from_min_max(pos2(tool.left(), tool.bottom() - 12.0), tool.max);
    shot.fills
        .iter()
        .any(|(r, f)| band.contains_rect(*r) && r.height() <= 8.0 && r.width() >= 20.0 && f.a() > 0)
}

#[test]
fn window_sizes() {
    for dark in [false, true] {
        for size in [BIG, MID, SMALL] {
            let (mut st, d, front, mut h) = box_front(A3_QUER, BIG);
            let sec = cut(&mut st, &mut h, d, front, 30.0);
            let _ = d;
            st.drawing_ui.selected_view = Some(sec);
            let mut a = App::new(size, dark, st);
            let shot = a.settle();
            let tool = field_rect(&a.c, "Werkzeugfeld");
            let view = field_rect(&a.c, "Ansichtsfeld");
            let ctx = format!("{size:?} dunkel={dark}");

            // Werkzeugzeile: der neue Knopf steht in Zeile 1 und schiebt nichts hinaus.
            if size != SMALL {
                let add = shot
                    .texts
                    .iter()
                    .find(|t| t.text == "Ansicht hinzufügen" && tool.contains(t.rect.center()))
                    .unwrap_or_else(|| panic!("{ctx}: „Ansicht hinzufügen“ fehlt"));
                let tool_btn = shot
                    .texts
                    .iter()
                    .find(|t| t.text == TOOL && tool.contains(t.rect.center()))
                    .unwrap_or_else(|| panic!("{ctx}: „{TOOL}“ fehlt im Werkzeugfeld"));
                assert!(
                    (tool_btn.rect.center().y - add.rect.center().y).abs() < 6.0,
                    "{ctx}"
                );
                assert!(
                    tool_btn.rect.left() > add.rect.right(),
                    "{ctx}: nach „Ansicht hinzufügen“"
                );
                for t in shot.texts.iter().filter(|t| tool.contains(t.rect.center())) {
                    assert!(
                        tool.contains_rect(t.rect) && t.clip.expand(0.5).contains_rect(t.rect),
                        "{ctx}: „{}“ abgeschnitten",
                        t.text
                    );
                }
                assert!(!scrollbar_in(&shot, tool), "{ctx}: Rollbalken sichtbar");
            } else {
                assert!(
                    scrollbar_in(&shot, tool),
                    "{ctx}: Werkzeugfeld rollt bei 800 × 600"
                );
            }

            // Ansichtsfeld: alle Texte liegen in der Breite des Felds, nichts ragt seitlich hinaus.
            let mut seen = 0;
            for t in shot.texts.iter().filter(|t| view.contains(t.rect.center())) {
                seen += 1;
                assert!(
                    t.rect.left() >= view.left() - 0.5 && t.rect.right() <= view.right() + 0.5,
                    "{ctx}: „{}“ ragt seitlich hinaus: {:?} in {view:?}",
                    t.text,
                    t.rect
                );
            }
            assert!(seen >= 4, "{ctx}: Ansichtsfeld leer");
            assert!(
                shot.texts
                    .iter()
                    .any(|t| t.text == NAME && view.contains(t.rect.center())),
                "{ctx}: Kopf fehlt"
            );
        }
    }
}
