//! Zeichnungsbereich (Teilprojekt 11, Aufgabe 15): Blattansicht, Werkzeugleiste, Fenster
//! „Neue Zeichnung“, Menü „Zeichnung“ und Umschalten zwischen 3D-Modell und Zeichnung.
//!
//! Die erwarteten Zahlen sind von Hand gerechnet (Quader 100 × 60 × 40, A4 quer 297 × 210 mm):
//! Vorderansicht 100 × 40, Draufsicht 100 × 60, Seitenansicht 60 × 40; Abstand Vorne–Oben
//! 40/2 + 20 + 60/2 = 70 mm.

use egui::epaint::{ColorMode, Shape};
use egui::{Color32, Event, Modifiers, PointerButton, Pos2, Rect, pos2, vec2};
use planora_app::browser::ERROR_COLOR;
use planora_app::drawing_dialog::*;
use planora_app::drawing_state::*;
use planora_app::drawing_view::{self, DrawnState};
use planora_app::state::*;
use planora_app::testing::collect_texts;
use planora_app::{fonts, icons, theme};
use planora_drawing::out2d::Out2d;
use planora_drawing::sheet::build_sheet;
use planora_drawing::{
    DrawBody, DrawError, DrawingId, Paper, Placement, Projection, Scale, Sheet, ViewId, ViewKind,
    ViewLines, compute_view,
};
use planora_exchange::{ExchangeMesh, Progress};
use planora_feature::FeatureKind;
use planora_math::{DVec2, DVec3};
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

const W: f32 = 1200.0;
const H: f32 = 800.0;

const A4_QUER: Sheet = Sheet {
    paper: Paper::A4,
    landscape: true,
};

fn s(num: u32, den: u32) -> Scale {
    Scale { num, den }
}

fn context() -> egui::Context {
    let ctx = egui::Context::default();
    ctx.set_fonts(fonts::definitions());
    ctx
}

fn raw(events: Vec<Event>, time: f64) -> egui::RawInput {
    raw_mods(events, time, Modifiers::NONE)
}

fn raw_mods(events: Vec<Event>, time: f64, modifiers: Modifiers) -> egui::RawInput {
    let mut all = vec![Event::ModifiersChanged(modifiers)];
    all.extend(events);
    egui::RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(W, H))),
        events: all,
        time: Some(time),
        ..Default::default()
    }
}

/// Text einer Form mit Lage.
struct Seen {
    text: String,
    rect: Rect,
    color: Color32,
}

fn walk_texts(shape: &Shape, out: &mut Vec<Seen>) {
    match shape {
        Shape::Text(t) => {
            let color = t
                .galley
                .job
                .sections
                .first()
                .map(|s| s.format.color)
                .unwrap_or(t.fallback_color);
            out.push(Seen {
                text: t.galley.text().to_string(),
                rect: Rect::from_min_size(t.pos, t.galley.size()),
                color,
            });
        }
        Shape::Vec(v) => v.iter().for_each(|s| walk_texts(s, out)),
        _ => {}
    }
}

fn walk_shapes<'a>(shape: &'a Shape, out: &mut Vec<&'a Shape>) {
    match shape {
        Shape::Vec(v) => v.iter().for_each(|s| walk_shapes(s, out)),
        s => out.push(s),
    }
}

/// Ein Bild der Oberfläche.
struct Shot {
    text: String,
    texts: Vec<Seen>,
    shapes: Vec<Shape>,
    /// Text und Ausschnitt, in dem er gezeichnet wird.
    clips: Vec<(String, Rect)>,
}

fn text_clips(out: &egui::FullOutput) -> Vec<(String, Rect)> {
    let mut v = Vec::new();
    for c in &out.shapes {
        let mut seen = Vec::new();
        walk_texts(&c.shape, &mut seen);
        v.extend(seen.into_iter().map(|s| (s.text, c.clip_rect)));
    }
    v
}

impl Shot {
    fn pos_of(&self, exact: &str) -> Option<Pos2> {
        self.texts
            .iter()
            .find(|t| t.text == exact)
            .map(|t| t.rect.left_center() + vec2(6.0, 0.0))
    }

    /// Wie `pos_of`, aber nur Texte außerhalb des Blatts `canvas` (die Felder am Rand; auf dem
    /// Blatt steht im Schriftfeld derselbe Maßstab).
    fn pos_of_outside(&self, exact: &str, canvas: Rect) -> Option<Pos2> {
        self.texts
            .iter()
            .find(|t| t.text == exact && !canvas.contains(t.rect.center()))
            .map(|t| t.rect.left_center() + vec2(6.0, 0.0))
    }

    fn flat(&self) -> Vec<&Shape> {
        let mut out = Vec::new();
        for s in &self.shapes {
            walk_shapes(s, &mut out);
        }
        out
    }
}

/// Testgeschirr: ein `egui::Context`, Zeit und was gezeigt wird.
struct Harness {
    ctx: egui::Context,
    time: f64,
    mods: Modifiers,
}

#[derive(Clone, Copy)]
enum What {
    /// Der Zeichnungsbereich (wie das Pane „Viewport“ im Zeichnungsmodus).
    Area,
    /// Die Einträge des Menüs „Zeichnung“.
    Menu,
    /// Wie `Area`, aber schreibgeschützt (alte Version).
    AreaReadOnly,
    /// Wie `Menu`, aber schreibgeschützt.
    MenuReadOnly,
}

impl Harness {
    fn new() -> Harness {
        Harness {
            ctx: context(),
            time: 0.0,
            mods: Modifiers::NONE,
        }
    }

    fn frame_with(
        &mut self,
        st: &mut AppState,
        dialog: &mut Option<NewDrawingDialog>,
        what: What,
        events: Vec<Event>,
    ) -> Shot {
        self.time += 0.05;
        let out = self
            .ctx
            .run_ui(raw_mods(events, self.time, self.mods), |ui| match what {
                What::Area => drawing_view::show(ui, st),
                What::Menu => drawing_menu(ui, st, dialog, false),
                What::AreaReadOnly => drawing_view::show_with(ui, st, false),
                What::MenuReadOnly => drawing_menu(ui, st, dialog, true),
            });
        // Fenster „Neue Zeichnung“ wie in der App nach den Bereichen.
        let mut texts = Vec::new();
        for c in &out.shapes {
            walk_texts(&c.shape, &mut texts);
        }
        let shot = Shot {
            text: collect_texts(&out),
            texts,
            shapes: out.shapes.iter().map(|c| c.shape.clone()).collect(),
            clips: text_clips(&out),
        };
        out.drop_without_applying_deltas();
        shot
    }

    fn frame(&mut self, st: &mut AppState, events: Vec<Event>) -> Shot {
        self.frame_with(st, &mut None, What::Area, events)
    }

    fn idle(&mut self, st: &mut AppState) -> Shot {
        self.frame(st, vec![])
    }

    fn layout(&self) -> drawing_view::DrawingLayout {
        drawing_view::last_layout(&self.ctx).expect("kein Zeichnungsbereich gezeichnet")
    }

    /// Klick mit der linken Maustaste an `p` (Bewegen, Drücken, Loslassen, Nachbild).
    fn click(&mut self, st: &mut AppState, p: Pos2) -> Shot {
        self.frame(st, vec![Event::PointerMoved(p)]);
        self.frame(st, vec![button(p, PointerButton::Primary, true)]);
        self.frame(st, vec![button(p, PointerButton::Primary, false)]);
        self.idle(st)
    }

    /// Klick auf den Text `exact`.
    fn click_text(&mut self, st: &mut AppState, exact: &str) -> Shot {
        let shot = self.idle(st);
        let p = shot
            .pos_of(exact)
            .unwrap_or_else(|| panic!("„{exact}“ nicht sichtbar:\n{}", shot.text));
        self.click(st, p)
    }

    /// Ziehen mit `btn` von `from` um `by` (in zwei Schritten).
    fn drag(&mut self, st: &mut AppState, from: Pos2, by: egui::Vec2, btn: PointerButton) {
        self.drag_mods(st, from, by, btn, Modifiers::NONE);
    }

    fn drag_mods(
        &mut self,
        st: &mut AppState,
        from: Pos2,
        by: egui::Vec2,
        btn: PointerButton,
        mods: Modifiers,
    ) {
        let press = |p: Pos2, pressed| Event::PointerButton {
            pos: p,
            button: btn,
            pressed,
            modifiers: mods,
        };
        let to = from + by;
        self.mods = mods;
        self.frame(st, vec![Event::PointerMoved(from)]);
        self.frame(st, vec![press(from, true)]);
        self.frame(st, vec![Event::PointerMoved(from + by * 0.5)]);
        self.frame(st, vec![Event::PointerMoved(to)]);
        self.frame(st, vec![press(to, false)]);
        self.mods = Modifiers::NONE;
        self.frame(st, vec![]);
    }
}

fn button(pos: Pos2, button: PointerButton, pressed: bool) -> Event {
    Event::PointerButton {
        pos,
        button,
        pressed,
        modifiers: Modifiers::NONE,
    }
}

/// Text des Symbols „Verkleinern“ der Werkzeugleiste.
fn zoom_out() -> String {
    icons::glyph(icons::Icon::ZoomOut).to_string()
}

/// Text des Symbols „Vergrößern“ der Werkzeugleiste.
fn zoom_in() -> String {
    icons::glyph(icons::Icon::ZoomIn).to_string()
}

/// Zustand mit einem Quader 100 × 60 × 40 mm.
fn box_state() -> AppState {
    let mut st = AppState::new();
    st.create_box(DVec3::new(100.0, 60.0, 40.0)).unwrap();
    st
}

/// Zeichnung „Blatt 1“ (A4 quer, Erstwinkel) mit Vorderansicht 1:1 in der Blattmitte.
fn with_front(st: &mut AppState) -> (DrawingId, ViewId) {
    let d = st
        .document
        .add_drawing("Blatt 1", A4_QUER, Projection::First)
        .unwrap();
    let front = st
        .document
        .add_view(
            d,
            ViewKind::Front,
            s(1, 1),
            Placement::Free { x: 148.5, y: 105.0 },
            None,
        )
        .unwrap();
    st.drawing_ui.open(d);
    (d, front)
}

fn add_child(st: &mut AppState, d: DrawingId, parent: ViewId, kind: ViewKind, dist: f64) -> ViewId {
    st.document
        .add_view(
            d,
            kind,
            s(1, 1),
            Placement::Aligned { distance: dist },
            Some(parent),
        )
        .unwrap()
}

fn block(st: &mut AppState) {
    st.drawing_ui.tick(&st.bodies, &st.document, true);
}

fn placement(st: &AppState, d: DrawingId, v: ViewId) -> Placement {
    st.document.drawing(d).unwrap().view(v).unwrap().placement
}

/// Bildschirmpunkt eines Blattpunkts (mm) im zuletzt gezeichneten Bereich.
fn screen(h: &Harness, p: (f64, f64)) -> Pos2 {
    let l = h.layout();
    drawing_view::sheet_to_screen(l.canvas, l.sheet_mm, l.zoom, l.pan, DVec2::new(p.0, p.1))
}

/// Mitte der Hüllbox aller Linien einer Ansicht auf dem Blatt (mm), eigene Rechnung über
/// dichte Abtastung der Kurven.
fn line_center(st: &AppState, d: DrawingId, name: &str) -> DVec2 {
    let drawing = st.document.drawing(d).unwrap();
    let model = st.document.model_edit_count();
    let owned = st.drawing_ui.results_for(drawing, model);
    let refs: BTreeMap<ViewId, &ViewLines> = owned.iter().map(|(k, v)| (*k, v.as_ref())).collect();
    let sheet = build_sheet(drawing, &refs);
    let v = sheet
        .views
        .iter()
        .find(|v| v.name == name)
        .unwrap_or_else(|| panic!("Ansicht „{name}“ fehlt"));
    let mut lo = DVec2::splat(f64::INFINITY);
    let mut hi = DVec2::splat(f64::NEG_INFINITY);
    for l in &v.lines {
        for p in dense(&l.curve, 64) {
            lo = lo.min(p);
            hi = hi.max(p);
        }
    }
    (lo + hi) * 0.5
}

/// Dichte Punkte einer Kurve über ihren ganzen Parameterbereich.
fn dense(c: &Out2d, n: usize) -> Vec<DVec2> {
    let (t0, t1) = match c {
        Out2d::Line { .. } => (0.0, 1.0),
        Out2d::Arc { start, end, .. } | Out2d::Ellipse { start, end, .. } => (*start, *end),
        Out2d::Spline { degree, knots, .. } => (knots[*degree], knots[knots.len() - 1 - degree]),
    };
    (0..=n)
        .map(|i| c.point(t0 + (t1 - t0) * i as f64 / n as f64))
        .collect()
}

fn seg_dist(p: DVec2, a: DVec2, b: DVec2) -> f64 {
    let ab = b - a;
    let t = if ab.length_squared() == 0.0 {
        0.0
    } else {
        ((p - a).dot(ab) / ab.length_squared()).clamp(0.0, 1.0)
    };
    p.distance(a + ab * t)
}

fn wait_for_cancel(_: &[DrawBody], _: ViewKind, p: &Progress) -> Result<ViewLines, DrawError> {
    p.set_total(4);
    p.advance(1);
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(30) {
        p.check()?;
        std::thread::sleep(Duration::from_millis(2));
    }
    Err(DrawError::Cancelled)
}

fn top_fails(b: &[DrawBody], k: ViewKind, p: &Progress) -> Result<ViewLines, DrawError> {
    if k == ViewKind::Top {
        Err(DrawError::TooComplex(5))
    } else {
        compute_view(b, k, p)
    }
}

/// Wörter, die in der Oberfläche nicht vorkommen dürfen (englische Fachwörter).
const ENGLISH: [&str; 24] = [
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
    "landscape",
    "portrait",
    "projection",
    "angle",
    "create",
    "cancel",
    "ok",
    "loading",
];

fn english_words(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .map(str::to_lowercase)
        .filter(|w| ENGLISH.contains(&w.as_str()))
        .collect()
}

// ---------------------------------------------------------------------------------------------
// (a), (b) Fenster „Neue Zeichnung“

/// Zeichnet das Fenster in einem eigenen Bild; `true`, solange es offen bleibt.
fn dialog_frame(
    h: &mut Harness,
    d: &mut NewDrawingDialog,
    st: &mut AppState,
    ev: Vec<Event>,
) -> (bool, Shot) {
    h.time += 0.05;
    let mut keep = true;
    let out = h.ctx.run_ui(raw(ev, h.time), |ui| {
        keep = d.show(ui.ctx(), st);
    });
    let mut texts = Vec::new();
    for c in &out.shapes {
        walk_texts(&c.shape, &mut texts);
    }
    let shot = Shot {
        text: collect_texts(&out),
        texts,
        shapes: out.shapes.iter().map(|c| c.shape.clone()).collect(),
        clips: text_clips(&out),
    };
    out.drop_without_applying_deltas();
    (keep, shot)
}

/// Drei Bilder ohne Eingaben (ein neues Fenster erscheint erst nach dem Ausmessen); Texte des
/// letzten.
fn dialog_shot(h: &mut Harness, d: &mut NewDrawingDialog, st: &mut AppState) -> Shot {
    dialog_frame(h, d, st, vec![]);
    dialog_frame(h, d, st, vec![]);
    dialog_frame(h, d, st, vec![]).1
}

/// Klickt im Fenster auf `exact`; liefert, ob das Fenster offen bleibt.
fn dialog_click(h: &mut Harness, d: &mut NewDrawingDialog, st: &mut AppState, exact: &str) -> bool {
    dialog_frame(h, d, st, vec![]);
    let (_, shot) = dialog_frame(h, d, st, vec![]);
    let p = shot
        .pos_of(exact)
        .unwrap_or_else(|| panic!("„{exact}“ fehlt:\n{}", shot.text));
    let mut keep = true;
    for ev in [
        Event::PointerMoved(p),
        button(p, PointerButton::Primary, true),
        button(p, PointerButton::Primary, false),
    ] {
        keep &= dialog_frame(h, d, st, vec![ev]).0;
    }
    keep
}

#[test]
fn new_drawing_dialog_creates_a_drawing_with_suggested_scale() {
    let mut st = box_state();
    let mut d = NewDrawingDialog::new(&st);
    assert_eq!(d.name, "Zeichnung 1");
    assert_eq!(d.paper, Paper::A4);
    assert!(d.landscape, "Querformat ist Vorgabe");
    assert_eq!(d.projection, Projection::First);
    assert_eq!(d.base, ViewKind::Front);
    // A4 quer (Blattzubehör mal 0,707), freier Bereich 275,79 × 151,31: 1:1 passt
    // (100+60+20 = 180, 40+60+20 = 120), 2:1 nicht (80+120+20 = 220 > 151,31 in der Höhe).
    assert_eq!(d.scale, s(1, 1));
    let mut h = Harness::new();
    let shot = dialog_shot(&mut h, &mut d, &mut st);
    for word in [
        "Neue Zeichnung",
        "Querformat",
        "Erstwinkel (Europa)",
        "Dritter Winkel (USA)",
        "Anlegen",
        "Abbrechen",
        "1:1",
    ] {
        assert!(shot.text.contains(word), "„{word}“ fehlt:\n{}", shot.text);
    }
    let keep = dialog_click(&mut h, &mut d, &mut st, "Anlegen");
    assert!(!keep, "nach „Anlegen“ schließt das Fenster");
    let drawings = st.document.drawings();
    assert_eq!(drawings.len(), 1);
    let dr = &drawings[0];
    assert_eq!(dr.name, "Zeichnung 1");
    assert_eq!(dr.sheet, A4_QUER);
    assert_eq!(dr.projection, Projection::First);
    assert_eq!(dr.views.len(), 1);
    let v = &dr.views[0];
    assert_eq!(v.kind, ViewKind::Front);
    assert_eq!(v.scale, s(1, 1));
    assert_eq!(v.parent, None);
    // Die Grundansicht allein liegt in der Mitte des freien Bereichs für die Anordnung (mit Platz
    // für Kopf und 3 Änderungszeilen: 152,04 | 114,9); erst mit
    // der nächsten Ansicht rückt sie, damit der Block aus drei Ansichten mittig liegt.
    let Placement::Free { x, y } = v.placement else {
        panic!("frei");
    };
    assert!((x - 152.035_714_285_714).abs() < 1e-9 && (y - 114.9).abs() < 1e-9);
    assert_eq!(st.drawing_ui.area, WorkArea::Drawing(dr.id));
    // Der nächste Vorschlag nimmt die kleinste freie Zahl.
    assert_eq!(NewDrawingDialog::new(&st).name, "Zeichnung 2");
    st.document.rename_drawing(dr.id, "Zeichnung 2").unwrap();
    assert_eq!(NewDrawingDialog::new(&st).name, "Zeichnung 1");

    // Großer Quader 1000 × 600 × 400: 1:10 (100+20 ≤ 151,31 hoch; 1:5 wäre 200+20 zu hoch).
    let mut big = AppState::new();
    big.create_box(DVec3::new(1000.0, 600.0, 400.0)).unwrap();
    assert_eq!(NewDrawingDialog::new(&big).scale, s(1, 10));
}

#[test]
fn empty_and_duplicate_names_block_creation_with_a_clear_text() {
    let mut st = box_state();
    st.document
        .add_drawing("Zeichnung 1", A4_QUER, Projection::First)
        .unwrap();
    let mut h = Harness::new();
    let mut d = NewDrawingDialog::new(&st);
    assert_eq!(d.name, "Zeichnung 2");
    for (name, msg) in [
        ("", EMPTY_NAME),
        ("   ", EMPTY_NAME),
        ("zeichnung 1", DUPLICATE_NAME),
        ("  ZEICHNUNG 1 ", DUPLICATE_NAME),
    ] {
        d.name = name.into();
        assert_eq!(d.error(&st), Some(msg), "{name:?}");
        let shot = dialog_shot(&mut h, &mut d, &mut st);
        let seen = shot
            .texts
            .iter()
            .find(|t| t.text == msg)
            .unwrap_or_else(|| panic!("Meldung fehlt:\n{}", shot.text));
        assert_eq!(seen.color, ERROR_COLOR, "Meldung in Rot");
        let keep = dialog_click(&mut h, &mut d, &mut st, "Anlegen");
        assert!(keep, "gesperrt: Fenster bleibt offen");
        assert_eq!(
            st.document.drawings().len(),
            1,
            "nichts angelegt bei {name:?}"
        );
    }
    assert_eq!(EMPTY_NAME, "Bitte einen Namen eingeben.");
    assert_eq!(
        DUPLICATE_NAME,
        "Eine Zeichnung mit diesem Namen gibt es schon."
    );
    d.name = "Zeichnung 2".into();
    assert_eq!(d.error(&st), None);
    // „Abbrechen“ schließt ohne Anlegen.
    assert!(!dialog_click(&mut h, &mut d, &mut st, "Abbrechen"));
    assert_eq!(st.document.drawings().len(), 1);
}

#[test]
fn the_window_is_centred_720_wide_with_equal_cards_and_plain_texts() {
    let mut st = box_state();
    let mut d = NewDrawingDialog::new(&st);
    let mut h = Harness::new();
    for _ in 0..3 {
        dialog_frame(&mut h, &mut d, &mut st, vec![]);
    }
    let r = d.last_rect.unwrap();
    assert!(
        (r.width() - 720.0).abs() <= 1.0 && (r.center().x - W / 2.0).abs() <= 1.0,
        "720 breit und mittig: {r:?}"
    );
    assert!(d.last_cards.len() >= 2);
    assert!(
        d.last_cards
            .windows(2)
            .all(|w| (w[0].height() - w[1].height()).abs() < 0.5),
        "{:?}",
        d.last_cards
    );
}

// ---------------------------------------------------------------------------------------------
// (c), (d) Texte und Breite

#[test]
fn drawing_area_shows_all_texts_in_german() {
    // Zeichnung mit fertiger Vorderansicht (gewählt), einer gescheiterten Draufsicht und
    // einer noch rechnenden Ansicht in einem zweiten Zustand.
    let mut st = box_state();
    st.drawing_ui.set_compute(top_fails);
    let (d, front) = with_front(&mut st);
    add_child(&mut st, d, front, ViewKind::Top, 70.0);
    block(&mut st);
    st.drawing_ui.selected_view = Some(front);
    let mut h = Harness::new();
    h.idle(&mut st);
    let shot = h.idle(&mut st);
    let mut all = shot.text.clone();
    let fit = icons::glyph(icons::Icon::FitSheet).to_string();
    let too_complex = DrawError::TooComplex(5).to_string();
    for word in [
        "3D-Modell",
        "Zeichnung: Blatt 1",
        "Ansicht hinzufügen",
        fit.as_str(),
        "Verdeckte Kanten zeigen",
        "Ansicht löschen",
        "1:1",
        too_complex.as_str(),
        drawing_view::HINT,
    ] {
        assert!(shot.text.contains(word), "„{word}“ fehlt:\n{}", shot.text);
    }

    let mut pending = box_state();
    pending.drawing_ui.set_compute(wait_for_cancel);
    with_front(&mut pending);
    let mut h2 = Harness::new();
    let start = Instant::now();
    let text = loop {
        let t = h2.idle(&mut pending).text;
        if t.contains("wird berechnet … 25 %") || start.elapsed() > Duration::from_secs(10) {
            break t;
        }
        std::thread::sleep(Duration::from_millis(5));
    };
    assert!(text.contains("wird berechnet … 25 %"), "{text}");
    all.push_str(&text);

    let mut mesh = AppState::new();
    mesh.import_mesh("Würfel", Arc::new(cube_mesh()), 1.0)
        .unwrap();
    with_front(&mut mesh);
    block(&mut mesh);
    let mut h3 = Harness::new();
    all.push_str(&h3.idle(&mut mesh).text);

    let mut dialog = NewDrawingDialog::new(&st);
    let mut h4 = Harness::new();
    let first = dialog_shot(&mut h4, &mut dialog, &mut st).text;
    assert!(
        first.contains("Grundansicht") && first.contains("Papier"),
        "{first}"
    );
    all.push_str(&first);
    dialog.name.clear();
    let empty = dialog_shot(&mut h4, &mut dialog, &mut st).text;
    assert!(empty.contains(EMPTY_NAME), "{empty}");
    all.push_str(&empty);

    let bad = english_words(&all);
    assert!(bad.is_empty(), "englische Wörter {bad:?} in:\n{all}");
    // Gegenprobe des Prüfers.
    assert_eq!(english_words("Add View – Sheet"), ["add", "view", "sheet"]);
}

#[test]
fn drawing_area_fills_the_whole_width() {
    let mut st = box_state();
    with_front(&mut st);
    block(&mut st);
    let mut h = Harness::new();
    h.idle(&mut st);
    h.idle(&mut st);
    let l = h.layout();
    // Das Blatt wird über das ganze Fenster gemalt (Etappe C); eingepasst wird es in die freie
    // Fläche zwischen Werkzeugfeld (oben, so breit wie sie) und Hinweisfeld (unten).
    let window = Rect::from_min_size(Pos2::ZERO, vec2(W, H));
    assert_eq!(l.area, window, "{:?}", l.area);
    assert_eq!(l.paint, window, "{:?}", l.paint);
    assert!(window.contains_rect(l.canvas), "{:?}", l.canvas);
    assert_eq!(l.toolbar.width(), l.canvas.width(), "{:?}", l.toolbar);
    assert!(l.hint.width() <= l.canvas.width() + 0.5, "{:?}", l.hint);
    assert!(l.toolbar.bottom() <= l.canvas.top() + 0.5);
    assert!(l.canvas.bottom() <= l.hint.top() + 0.5);
}

// ---------------------------------------------------------------------------------------------
// (e) Ansicht hinzufügen

#[test]
fn adding_a_top_view_sits_below_front_in_first_angle() {
    let mut st = box_state();
    let (d, front) = with_front(&mut st);
    let top = drawing_view::add_view(&mut st, ViewKind::Top).unwrap();
    let v = st.document.drawing(d).unwrap().view(top).unwrap().clone();
    assert_eq!(v.parent, Some(front));
    assert_eq!(v.scale, s(1, 1));
    // 40/2 + 20 + 60/2
    assert_eq!(v.placement, Placement::Aligned { distance: 70.0 });
    block(&mut st);
    let f = line_center(&st, d, "Vorne");
    let t = line_center(&st, d, "Oben");
    assert!((f - DVec2::new(148.5, 105.0)).length() < 1e-6, "{f:?}");
    assert!((t - DVec2::new(148.5, 35.0)).length() < 1e-6, "{t:?}");

    // Seitenansicht von links: im Erstwinkel rechts neben „Vorne“ liegt die Ansicht von links.
    let left = drawing_view::add_view(&mut st, ViewKind::Left).unwrap();
    block(&mut st);
    let l = line_center(&st, d, "Links");
    // 100/2 + 20 + 60/2 = 100
    assert!((l - DVec2::new(248.5, 105.0)).length() < 1e-6, "{l:?}");
    assert_eq!(
        placement(&st, d, left),
        Placement::Aligned { distance: 100.0 }
    );

    // Isometrisch: ins leere Viertel rechts unten neben „Vorne“ (Erstwinkel), ab (218,5, 65).
    // Der freie Bereich (14,14…289,93 × 46,67…197,98) lässt dort nur 71,4 × 18,3 mm; die
    // Isometrie (113,1 × 98,0 bei 1:1, 56,6 × 49,0 bei 1:2) passt weder in 1:1 noch eine Stufe
    // kleiner. Nach bestem Wissen: 1:1, bündig in der Ecke rechts unten des freien Bereichs.
    let iso = drawing_view::add_view(&mut st, ViewKind::Isometric).unwrap();
    let v = st.document.drawing(d).unwrap().view(iso).unwrap().clone();
    assert_eq!(v.parent, None);
    assert_eq!(v.scale, s(1, 1));
    let Placement::Free { x, y } = v.placement else {
        panic!("frei");
    };
    let (iw, ih) = estimate_view_size(&st.bodies, ViewKind::Isometric);
    assert!(
        (x - (289.928_571_428_571 - iw / 2.0)).abs() < 1e-9
            && (y - (46.671_428_571_429 + ih / 2.0)).abs() < 1e-9,
        "({x}, {y})"
    );

    // Gewählte Normansicht ist die Ausgangsansicht; gleiche Art zweimal erlaubt.
    st.drawing_ui.selected_view = Some(top);
    let second = drawing_view::add_view(&mut st, ViewKind::Front).unwrap();
    let v = st
        .document
        .drawing(d)
        .unwrap()
        .view(second)
        .unwrap()
        .clone();
    assert_eq!(v.parent, Some(top));
    // Von „Oben“ aus liegt „Vorne“ im Erstwinkel darüber: 60/2 + 20 + 40/2.
    assert_eq!(v.placement, Placement::Aligned { distance: 70.0 });
    block(&mut st);
    let f2 = line_center(&st, d, "Vorne 2");
    assert!((f2 - DVec2::new(148.5, 105.0)).length() < 1e-6, "{f2:?}");

    // Gewählte isometrische Ansicht: Ausgangsansicht ist die Grundansicht.
    st.drawing_ui.selected_view = Some(iso);
    let right = drawing_view::add_view(&mut st, ViewKind::Right).unwrap();
    assert_eq!(
        st.document.drawing(d).unwrap().view(right).unwrap().parent,
        Some(front)
    );
}

#[test]
fn the_toolbar_menu_adds_a_view() {
    let mut st = box_state();
    let (d, front) = with_front(&mut st);
    block(&mut st);
    let mut h = Harness::new();
    h.click_text(&mut st, "Ansicht hinzufügen");
    let shot = h.idle(&mut st);
    for k in ViewKind::ALL {
        assert!(
            shot.pos_of(k.label()).is_some(),
            "„{}“ fehlt:\n{}",
            k.label(),
            shot.text
        );
    }
    h.click_text(&mut st, "Oben");
    let views = &st.document.drawing(d).unwrap().views;
    assert_eq!(views.len(), 2);
    assert_eq!(views[1].kind, ViewKind::Top);
    assert_eq!(views[1].parent, Some(front));
}

// ---------------------------------------------------------------------------------------------
// (f), (g) Ziehen

/// Vorderansicht und Draufsicht (70 mm darunter), fertig berechnet und einmal gezeichnet.
fn front_and_top() -> (AppState, DrawingId, ViewId, ViewId, Harness) {
    let mut st = box_state();
    let (d, front) = with_front(&mut st);
    let top = add_child(&mut st, d, front, ViewKind::Top, 70.0);
    block(&mut st);
    let mut h = Harness::new();
    h.idle(&mut st);
    h.idle(&mut st);
    (st, d, front, top, h)
}

#[test]
fn dragging_an_aligned_view_moves_only_along_its_axis() {
    let (mut st, d, _, top, mut h) = front_and_top();
    let k = h.layout().px_per_mm as f64;
    // Mitte der Draufsicht (148,5 | 35) – innerhalb ihrer Hüllbox.
    let p = screen(&h, (148.5, 35.0));
    h.drag(&mut st, p, vec2(30.0, 40.0), PointerButton::Primary);
    assert_eq!(st.drawing_ui.selected_view, Some(top));
    // Nach unten ziehen (40 px) vergrößert den Abstand nach unten um 40/k mm; die 30 px
    // quer zur Achse zählen nicht.
    let Placement::Aligned { distance } = placement(&st, d, top) else {
        panic!("bleibt abgeleitet");
    };
    assert!(
        (distance - (70.0 + 40.0 / k)).abs() < 1e-3,
        "{distance} vs {}",
        70.0 + 40.0 / k
    );
    h.idle(&mut st);
    let t = line_center(&st, d, "Oben");
    assert!((t.x - 148.5).abs() < 1e-6, "x bleibt: {t:?}");
    assert!((t.y - (35.0 - 40.0 / k)).abs() < 1e-3, "{t:?}");
}

#[test]
fn dragging_a_free_view_moves_both_axes() {
    let (mut st, d, front, top, mut h) = front_and_top();
    let k = h.layout().px_per_mm as f64;
    let p = screen(&h, (148.5, 105.0));
    h.drag(&mut st, p, vec2(30.0, 40.0), PointerButton::Primary);
    assert_eq!(st.drawing_ui.selected_view, Some(front));
    let Placement::Free { x, y } = placement(&st, d, front) else {
        panic!("bleibt frei");
    };
    assert!((x - (148.5 + 30.0 / k)).abs() < 1e-3, "{x}");
    assert!((y - (105.0 - 40.0 / k)).abs() < 1e-3, "{y}");
    // Die abgeleitete Ansicht zieht mit, ihr Abstand bleibt.
    assert_eq!(
        placement(&st, d, top),
        Placement::Aligned { distance: 70.0 }
    );
    let t = line_center(&st, d, "Oben");
    assert!((t - DVec2::new(x, y - 70.0)).length() < 1e-6, "{t:?}");
}

#[test]
fn panning_and_wheel_zoom_change_only_the_sheet_view() {
    let (mut st, d, front, _, mut h) = front_and_top();
    let before = st.document.drawing(d).unwrap().clone();
    let l0 = h.layout();
    // Mittlere Maustaste verschiebt.
    let p = screen(&h, (40.0, 180.0));
    h.drag(&mut st, p, vec2(25.0, -15.0), PointerButton::Middle);
    let l1 = h.layout();
    assert!(
        (l1.pan - (l0.pan + vec2(25.0, -15.0))).length() < 0.01,
        "{:?}",
        l1.pan
    );
    // Umschalt + Ziehen verschiebt ebenso (auch über einer Ansicht, ohne sie zu bewegen).
    let p = screen(&h, (148.5, 105.0));
    h.drag_mods(
        &mut st,
        p,
        vec2(-10.0, 20.0),
        PointerButton::Primary,
        Modifiers::SHIFT,
    );
    let l2 = h.layout();
    assert!(
        (l2.pan - (l1.pan + vec2(-10.0, 20.0))).length() < 0.01,
        "{:?}",
        l2.pan
    );
    assert_eq!(
        st.document.drawing(d).unwrap(),
        &before,
        "nichts verschoben"
    );
    let _ = front;

    // Mausrad: der Blattpunkt unter dem Zeiger bleibt stehen.
    let pointer = pos2(700.0, 300.0);
    let under = drawing_view::screen_to_sheet(l2.canvas, l2.sheet_mm, l2.zoom, l2.pan, pointer);
    h.frame(&mut st, vec![Event::PointerMoved(pointer)]);
    h.frame(
        &mut st,
        vec![Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: vec2(0.0, 120.0),
            phase: egui::TouchPhase::Move,
            modifiers: Modifiers::NONE,
        }],
    );
    for _ in 0..20 {
        h.idle(&mut st);
    }
    let l3 = h.layout();
    assert!(
        l3.zoom > l2.zoom * 1.05,
        "gezoomt: {} → {}",
        l2.zoom,
        l3.zoom
    );
    let back = drawing_view::sheet_to_screen(l3.canvas, l3.sheet_mm, l3.zoom, l3.pan, under);
    assert!((back - pointer).length() < 0.5, "{back:?} vs {pointer:?}");
    assert_eq!(st.document.drawing(d).unwrap(), &before);

    // Lupe −/+ und „Blatt einpassen“.
    h.click_text(&mut st, &zoom_out());
    assert!(h.layout().zoom < l3.zoom);
    h.click_text(&mut st, &icons::glyph(icons::Icon::FitSheet).to_string());
    let l4 = h.layout();
    assert_eq!((l4.zoom, l4.pan), (1.0, egui::Vec2::ZERO));
    // Eingepasst: das ganze Blatt liegt im Bereich und füllt ihn in einer Richtung fast aus.
    let a = screen(&h, (0.0, 0.0));
    let b = screen(&h, (297.0, 210.0));
    let paper = Rect::from_two_pos(a, b);
    assert!(
        l4.canvas.contains_rect(paper),
        "{paper:?} in {:?}",
        l4.canvas
    );
    let fill = (paper.width() / l4.canvas.width()).max(paper.height() / l4.canvas.height());
    assert!(fill > 0.9, "{fill}");
    h.click_text(&mut st, &zoom_in());
    assert!(h.layout().zoom > 1.0);
}

// ---------------------------------------------------------------------------------------------
// (h) Auswahl und Löschen

#[test]
fn selecting_by_click_on_a_line_and_deleting_removes_children() {
    let mut st = box_state();
    let (d, front) = with_front(&mut st);
    let top = add_child(&mut st, d, front, ViewKind::Top, 70.0);
    let left = add_child(&mut st, d, front, ViewKind::Left, 100.0);
    let iso = st
        .document
        .add_view(
            d,
            ViewKind::Isometric,
            s(1, 2),
            Placement::Free { x: 40.0, y: 170.0 },
            None,
        )
        .unwrap();
    block(&mut st);
    let mut h = Harness::new();
    h.idle(&mut st);
    h.idle(&mut st);
    // Linke Kante der Vorderansicht liegt bei x = 148,5 − 50 = 98,5 mm. Vier Pixel links
    // daneben liegt außerhalb der Hüllbox: nur die Nähe zur Linie wählt.
    let edge = screen(&h, (98.5, 105.0));
    h.click(&mut st, edge - vec2(4.0, 0.0));
    assert_eq!(st.drawing_ui.selected_view, Some(front));
    // Zu weit weg (12 px) wählt nichts und hebt die Wahl auf.
    h.click(&mut st, edge - vec2(12.0, 0.0));
    assert_eq!(st.drawing_ui.selected_view, None);
    // Klick in die Hüllbox der Draufsicht.
    h.click(&mut st, screen(&h, (148.5, 35.0)));
    assert_eq!(st.drawing_ui.selected_view, Some(top));
    h.click(&mut st, edge - vec2(4.0, 0.0));
    assert_eq!(st.drawing_ui.selected_view, Some(front));
    // Gewählte Ansicht: Rahmen in Akzentfarbe gestrichelt.
    let shot = h.idle(&mut st);
    let accent = theme::accent(st.dark);
    let dashed = shot
        .flat()
        .iter()
        .filter(|s| matches!(s, Shape::LineSegment { stroke, .. } if stroke.color == accent))
        .count();
    assert!(dashed >= 8, "gestrichelter Rahmen fehlt ({dashed})");

    h.click_text(&mut st, "Ansicht löschen");
    // Derselbe Weg wie im Browser: abgeleitete Ansichten werden erst genannt (die Rückfrage
    // zeigt der Browser), bestätigt wird über den Löschplan.
    assert_eq!(
        st.document.drawing(d).unwrap().views.len(),
        4,
        "erst fragen"
    );
    let plan = st.pending_delete_drawing.clone().expect("Rückfrage");
    assert_eq!(
        plan.text,
        "Die Ansicht „Vorne“ hat abgeleitete Ansichten: Oben, Links. Sie werden mit gelöscht."
    );
    planora_app::drawing_browser::apply_delete(&mut st, &plan).unwrap();
    let views: Vec<ViewId> = st
        .document
        .drawing(d)
        .unwrap()
        .views
        .iter()
        .map(|v| v.id)
        .collect();
    assert_eq!(views, vec![iso], "Vorne samt Oben und Links entfernt");
    let _ = (top, left);
    assert_eq!(st.drawing_ui.selected_view, None);
    let shot = h.idle(&mut st);
    assert!(
        shot.pos_of("Ansicht löschen").is_none(),
        "ohne Wahl kein Löschen"
    );
}

#[test]
fn toolbar_delete_of_a_view_with_dimensions_asks_first() {
    use planora_drawing::dimension::{
        DimKind, DimSpec, GeoRef, GeoRefData, TagData, Tolerance as DimTolerance,
    };
    let mut st = box_state();
    let (d, front) = with_front(&mut st);
    for _ in 0..2 {
        let spec = DimSpec {
            id: planora_drawing::dimension::DimId(0),
            view: front,
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
    block(&mut st);
    let mut h = Harness::new();
    h.idle(&mut st);
    st.drawing_ui.selected_view = Some(front);
    h.click_text(&mut st, "Ansicht löschen");
    let dr = st.document.drawing(d).unwrap();
    assert!(dr.view(front).is_some(), "nicht still gelöscht");
    assert_eq!(dr.dimensions.len(), 2, "Maße bleiben bis zur Bestätigung");
    assert_eq!(st.drawing_ui.selected_view, Some(front), "Auswahl bleibt");
    let plan = st.pending_delete_drawing.clone().expect("Rückfrage");
    assert_eq!(
        plan.text,
        "Die Ansicht „Vorne“ wird gelöscht, und 2 Maße werden mit gelöscht."
    );
    planora_app::drawing_browser::apply_delete(&mut st, &plan).unwrap();
    let dr = st.document.drawing(d).unwrap();
    assert!(dr.views.is_empty() && dr.dimensions.is_empty());
    assert_eq!(st.drawing_ui.selected_view, None);
}

#[test]
fn scale_and_hidden_edges_of_the_selected_view_can_be_changed() {
    let (mut st, d, front, top, mut h) = front_and_top();
    st.drawing_ui.selected_view = Some(top);
    // Vorgabe der Normansichten: verdeckte Kanten an; der Haken schaltet sie für Oben aus.
    assert!(
        st.document
            .drawing(d)
            .unwrap()
            .view(top)
            .unwrap()
            .show_hidden
    );
    h.click_text(&mut st, "Verdeckte Kanten zeigen");
    let dr = st.document.drawing(d).unwrap();
    assert!(!dr.view(top).unwrap().show_hidden);
    assert!(dr.view(front).unwrap().show_hidden);
    let canvas = h.layout().canvas;
    let shot = h.idle(&mut st);
    let p = shot.pos_of_outside("1:1", canvas).expect("Maßstabsfeld");
    h.click(&mut st, p);
    h.click_text(&mut st, "1:2");
    let dr = st.document.drawing(d).unwrap();
    assert_eq!(dr.view(top).unwrap().scale, s(1, 2));
    assert_eq!(dr.view(front).unwrap().scale, s(1, 1));
}

// ---------------------------------------------------------------------------------------------
// (i), (j) Fehler, Rechnen, Netze

#[test]
fn failed_view_shows_its_text_and_other_views_still_draw() {
    let mut st = box_state();
    st.drawing_ui.set_compute(top_fails);
    let (d, front) = with_front(&mut st);
    let top = add_child(&mut st, d, front, ViewKind::Top, 70.0);
    let iso = st
        .document
        .add_view(
            d,
            ViewKind::Isometric,
            s(1, 2),
            Placement::Free { x: 240.0, y: 170.0 },
            None,
        )
        .unwrap();
    block(&mut st);
    let mut h = Harness::new();
    h.idle(&mut st);
    let shot = h.idle(&mut st);
    let msg = DrawError::TooComplex(5).to_string();
    let l = h.layout();
    let state_of = |id: ViewId| l.views.iter().find(|v| v.id == id).map(|v| v.state.clone());
    assert_eq!(state_of(front), Some(DrawnState::Lines));
    assert_eq!(state_of(iso), Some(DrawnState::Lines));
    assert_eq!(state_of(top), Some(DrawnState::Failed(msg.clone())));
    assert!(
        l.lines_drawn >= 8,
        "Vorne (4) und Isometrisch zeichnen: {}",
        l.lines_drawn
    );
    // Der Text steht rot im Rahmen der Draufsicht (40/2 + 20 + 60/2 unter der Mitte).
    let frame = l.views.iter().find(|v| v.id == top).unwrap().rect;
    let expect = Rect::from_two_pos(screen(&h, (98.5, 65.0)), screen(&h, (198.5, 5.0)));
    assert!(
        (frame.min - expect.min).length() < 1.0 && (frame.max - expect.max).length() < 1.0,
        "{frame:?} vs {expect:?}"
    );
    let seen: Vec<&Seen> = shot
        .texts
        .iter()
        .filter(|t| t.text.contains(&msg))
        .collect();
    assert_eq!(seen.len(), 1, "{}", shot.text);
    assert_eq!(seen[0].color, theme::SHEET_LOST);
    assert!(
        frame.contains_rect(seen[0].rect),
        "{:?} in {frame:?}",
        seen[0].rect
    );
    assert!(
        seen[0].text.contains("Oben"),
        "mit dem Namen der Ansicht: {}",
        seen[0].text
    );
}

#[test]
fn a_long_failure_text_wraps_inside_a_small_view() {
    let mut st = box_state();
    st.drawing_ui.set_compute(top_fails);
    let (d, front) = with_front(&mut st);
    let top = add_child(&mut st, d, front, ViewKind::Top, 70.0);
    st.document
        .update_drawing(d, |dr| {
            dr.views.iter_mut().find(|v| v.id == top).unwrap().scale = s(1, 5);
        })
        .unwrap();
    block(&mut st);
    let mut h = Harness::new();
    h.idle(&mut st);
    let shot = h.idle(&mut st);
    let frame = h.layout().views.iter().find(|v| v.id == top).unwrap().rect;
    let msg = DrawError::TooComplex(5).to_string();
    let seen = shot.texts.iter().find(|t| t.text.contains(&msg)).unwrap();
    assert!(
        seen.rect.width() <= frame.width() + 0.5,
        "{:?} in {frame:?}",
        seen.rect
    );
    assert!(seen.rect.height() > 30.0, "umgebrochen: {:?}", seen.rect);
}

#[test]
fn pending_views_show_progress_and_resolve_through_the_frame_tick() {
    // Ohne blockierendes `tick`: nur die Bilder des Bereichs treiben die Rechnung voran.
    let mut st = box_state();
    let (d, front) = with_front(&mut st);
    let mut h = Harness::new();
    // Ein Bild mit einer anderen Zeichnung vorweg: dort entstehen die schwebenden Felder (ihr
    // erster, unsichtbarer Größen-Durchgang wird verworfen und das Bild wiederholt). Sonst liefe
    // das erste Bild hier zweimal, und die schnelle Rechnung wäre womöglich schon fertig.
    let mut warm = box_state();
    with_front(&mut warm);
    h.idle(&mut warm);
    let first = h.idle(&mut st);
    assert!(first.text.contains("wird berechnet …"), "{}", first.text);
    let pending = h.layout();
    assert_eq!(pending.lines_drawn, 0);
    let v = pending.views.iter().find(|v| v.id == front).unwrap();
    assert!(matches!(v.state, DrawnState::Pending(_)), "{:?}", v.state);
    // Platzhalter in der geschätzten Größe 100 × 40 mm um die Blattmitte.
    let expect = Rect::from_two_pos(screen(&h, (98.5, 125.0)), screen(&h, (198.5, 85.0)));
    assert!(
        (v.rect.min - expect.min).length() < 1.0 && (v.rect.max - expect.max).length() < 1.0,
        "{:?} vs {expect:?}",
        v.rect
    );
    let start = Instant::now();
    while h.layout().lines_drawn == 0 && start.elapsed() < Duration::from_secs(20) {
        std::thread::sleep(Duration::from_millis(10));
        h.idle(&mut st);
    }
    assert_eq!(h.layout().lines_drawn, 4, "Rechteck der Vorderansicht");
    assert_eq!(st.drawing_ui.spawned(), 1);
    let _ = d;
}

#[test]
fn pending_progress_bar_shows_the_fraction() {
    let mut st = box_state();
    st.drawing_ui.set_compute(wait_for_cancel);
    let (_, front) = with_front(&mut st);
    let mut h = Harness::new();
    let start = Instant::now();
    let shot = loop {
        let shot = h.idle(&mut st);
        if shot.text.contains("25 %") || start.elapsed() > Duration::from_secs(10) {
            break shot;
        }
        std::thread::sleep(Duration::from_millis(5));
    };
    assert!(shot.text.contains("wird berechnet … 25 %"), "{}", shot.text);
    let l = h.layout();
    let frame = l.views.iter().find(|v| v.id == front).unwrap();
    assert_eq!(frame.state, DrawnState::Pending(0.25));
    let bar = frame.bar.expect("Fortschrittsbalken");
    assert!(frame.rect.contains_rect(bar.0), "{bar:?}");
    assert!(
        (bar.1.width() - 0.25 * bar.0.width()).abs() < 0.5,
        "{bar:?}"
    );
}

/// Einheitswürfel als Netz.
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
fn mesh_only_document_shows_the_note_instead_of_lines() {
    let mut st = AppState::new();
    st.import_mesh("Würfel", Arc::new(cube_mesh()), 1.0)
        .unwrap();
    let (_, front) = with_front(&mut st);
    block(&mut st);
    let mut h = Harness::new();
    h.idle(&mut st);
    let shot = h.idle(&mut st);
    let note = "Alle Körper sind Netze; eine Zeichnung braucht exakte Körper.";
    let l = h.layout();
    assert_eq!(l.lines_drawn, 0);
    let hint: Vec<&Seen> = shot.texts.iter().filter(|t| t.text == note).collect();
    assert_eq!(hint.len(), 1, "Hinweiszeile:\n{}", shot.text);
    assert!(
        l.hint.contains_rect(hint[0].rect),
        "{:?} in {:?}",
        hint[0].rect,
        l.hint
    );
    assert!(matches!(
        &l.views.iter().find(|v| v.id == front).unwrap().state,
        DrawnState::Failed(m) if m == note
    ));
    assert_eq!(st.drawing_ui.spawned(), 0);
}

#[test]
fn an_empty_document_or_a_missing_drawing_does_not_panic() {
    let mut st = AppState::new();
    let mut h = Harness::new();
    st.drawing_ui.open(DrawingId(99));
    h.idle(&mut st);
    assert_eq!(
        st.drawing_ui.area,
        WorkArea::Model,
        "fehlende Zeichnung → 3D-Modell"
    );
    // Zeichnung ohne Körper und ohne Ansichten.
    let d = st
        .document
        .add_drawing("Leer", A4_QUER, Projection::Third)
        .unwrap();
    st.drawing_ui.open(d);
    let shot = h.idle(&mut st);
    let shot2 = h.idle(&mut st);
    assert!(
        shot2
            .text
            .contains("Es gibt keinen Körper, der sich zeichnen lässt."),
        "{}",
        shot.text
    );
    assert_eq!(h.layout().lines_drawn, 0);
    // Ansicht hinzufügen ohne Körper und ohne Grundansicht: frei in der Mitte des freien
    // Bereichs für die Anordnung (A4 quer, Blattzubehör mal 0,707, Platz für Kopf und
    // 3 Änderungszeilen: 14,14…289,93 × 46,67…183,13), 1:1.
    let v = drawing_view::add_view(&mut st, ViewKind::Top).unwrap();
    let spec = st.document.drawing(d).unwrap().view(v).unwrap().clone();
    let Placement::Free { x, y } = spec.placement else {
        panic!("frei");
    };
    assert!(
        (x - 152.035_714_285_714).abs() < 1e-9 && (y - 114.9).abs() < 1e-9,
        "({x}, {y})"
    );
    assert_eq!(spec.parent, None);
    assert_eq!(spec.scale, s(1, 1));
    h.idle(&mut st);
    h.idle(&mut st);
}

// ---------------------------------------------------------------------------------------------
// (k) dunkles Thema, Linienbreiten

#[test]
fn dark_theme_keeps_the_paper_white() {
    let (mut st, _, _, _, mut h) = front_and_top();
    st.dark = true;
    theme::apply(&h.ctx, true);
    h.idle(&mut st);
    let shot = h.idle(&mut st);
    let paper = Rect::from_two_pos(screen(&h, (0.0, 0.0)), screen(&h, (297.0, 210.0)));
    let rects: Vec<(Rect, Color32)> = shot
        .flat()
        .iter()
        .filter_map(|s| match s {
            Shape::Rect(r) => Some((r.rect, r.fill)),
            _ => None,
        })
        .collect();
    assert!(
        rects.iter().any(|(r, c)| *c == Color32::WHITE
            && (r.min - paper.min).length() < 0.5
            && (r.max - paper.max).length() < 0.5),
        "weißes Blatt {paper:?} fehlt: {rects:?}"
    );
    // Das Umfeld füllt die ganze Malfläche (Etappe C: das Fenster).
    let canvas = h.layout().paint;
    let surround = rects
        .iter()
        .find(|(r, _)| (r.min - canvas.min).length() < 0.5 && (r.max - canvas.max).length() < 0.5)
        .expect("Umfeld")
        .1;
    assert_eq!(surround, theme::drawing_surround(true));
    assert!(
        surround.r() < 80 && surround.g() < 80 && surround.b() < 80,
        "dunkles Umfeld"
    );
    assert_ne!(theme::drawing_surround(false), surround);
}

/// Breiten der schwarzen Striche im Bild (Linienzüge und Strecken).
fn ink_widths(shot: &Shot) -> Vec<f32> {
    shot.flat()
        .iter()
        .filter_map(|s| match s {
            Shape::Path(p) if p.stroke.color == ColorMode::Solid(theme::SHEET_INK) => {
                Some(p.stroke.width)
            }
            Shape::LineSegment { stroke, .. } if stroke.color == theme::SHEET_INK => {
                Some(stroke.width)
            }
            _ => None,
        })
        .collect()
}

/// Breiten der schwarzen Striche des leeren Blatts (Rahmen und Schriftfeld) im Fenster.
fn empty_sheet_ink() -> Vec<f32> {
    let mut st = box_state();
    let d = st
        .document
        .add_drawing("Blatt 1", A4_QUER, Projection::First)
        .unwrap();
    st.drawing_ui.open(d);
    let mut h = Harness::new();
    h.idle(&mut st);
    let shot = h.idle(&mut st);
    ink_widths(&shot)
}

#[test]
fn visible_lines_are_half_a_millimetre_wide_but_at_least_one_pixel() {
    let (mut st, _, _, _, mut h) = front_and_top();
    let shot = h.idle(&mut st);
    let k = h.layout().px_per_mm;
    // Ohne Rahmen und Schriftfeld des leeren Blatts bleiben die Linien der Ansichten.
    let mut ink = ink_widths(&shot);
    for w in empty_sheet_ink() {
        let i = ink
            .iter()
            .position(|x| (x - w).abs() < 1e-6)
            .expect("Rahmenlinie fehlt");
        ink.remove(i);
    }
    assert_eq!(ink.len(), 8, "4 Kanten je Ansicht: {ink:?}");
    let want = (0.5 * k).max(1.0);
    assert!(
        ink.iter().all(|w| (w - want).abs() < 1e-4),
        "{ink:?} vs {want}"
    );
    // Stark verkleinert: mindestens ein Pixel.
    st.drawing_ui.zoom = 0.1;
    let shot = h.idle(&mut st);
    let k = h.layout().px_per_mm;
    assert!(0.5 * k < 1.0);
    let widths = ink_widths(&shot);
    assert!(
        !widths.is_empty() && widths.iter().all(|w| *w == 1.0),
        "{widths:?}"
    );
}

#[test]
fn hidden_lines_are_dashed_three_and_one_millimetres() {
    let pts = [DVec2::new(0.0, 0.0), DVec2::new(10.0, 0.0)];
    let d = drawing_view::dashes(&pts, 3.0, 1.0);
    let ends: Vec<(f64, f64)> = d.iter().map(|p| (p[0].x, p[p.len() - 1].x)).collect();
    assert_eq!(ends, vec![(0.0, 3.0), (4.0, 7.0), (8.0, 10.0)]);
    // Über eine Ecke hinweg läuft das Muster weiter: 2 + 2 mm.
    let pts = [
        DVec2::new(0.0, 0.0),
        DVec2::new(2.0, 0.0),
        DVec2::new(2.0, 2.0),
    ];
    let d = drawing_view::dashes(&pts, 3.0, 1.0);
    assert_eq!(d.len(), 1);
    assert_eq!(
        d[0],
        vec![
            DVec2::new(0.0, 0.0),
            DVec2::new(2.0, 0.0),
            DVec2::new(2.0, 1.0)
        ]
    );
    assert_eq!(drawing_view::VISIBLE_MM, 0.5);
    assert_eq!(drawing_view::HIDDEN_MM, 0.25);
}

#[test]
fn curves_are_flattened_within_one_pixel() {
    let spline = Out2d::Spline {
        degree: 3,
        knots: vec![0.0, 0.0, 0.0, 0.0, 0.5, 1.0, 1.0, 1.0, 1.0],
        ctrl: vec![
            DVec2::new(0.0, 0.0),
            DVec2::new(30.0, 80.0),
            DVec2::new(60.0, -60.0),
            DVec2::new(90.0, 40.0),
            DVec2::new(120.0, 0.0),
        ],
        weights: vec![1.0, 2.0, 1.0, 0.5, 1.0],
    };
    let curves = [
        Out2d::Arc {
            c: DVec2::new(10.0, 20.0),
            r: 40.0,
            start: 0.3,
            end: 4.0,
        },
        Out2d::Ellipse {
            c: DVec2::new(-5.0, 3.0),
            major: DVec2::new(30.0, 40.0),
            ratio: 0.2,
            start: -1.0,
            end: 5.0,
        },
        spline,
    ];
    for c in &curves {
        let mut counts = Vec::new();
        for k in [0.5, 3.0, 30.0] {
            let poly = drawing_view::flatten(c, k);
            counts.push(poly.len());
            let truth = dense(c, 4000);
            assert!(
                poly[0].distance(truth[0]) < 1e-9
                    && poly.last().unwrap().distance(*truth.last().unwrap()) < 1e-9
            );
            let worst = truth
                .iter()
                .map(|p| {
                    poly.windows(2)
                        .map(|w| seg_dist(*p, w[0], w[1]))
                        .fold(f64::INFINITY, f64::min)
                })
                .fold(0.0, f64::max);
            assert!(worst * k <= 1.0, "{c:?} bei {k} px/mm: {} px", worst * k);
        }
        assert!(
            counts[0] < counts[2],
            "mehr Stücke bei stärkerem Zoom: {counts:?}"
        );
        // Nicht unnötig fein: eine Länge von L px braucht höchstens L/2 + 8 Stücke.
        let len_px = c.length() * 3.0;
        assert!(
            (counts[1] as f64) < len_px / 2.0 + 8.0,
            "{counts:?}, {len_px}"
        );
    }
    let line = Out2d::Line {
        a: DVec2::new(1.0, 2.0),
        b: DVec2::new(5.0, 2.0),
    };
    assert_eq!(drawing_view::flatten(&line, 10.0).len(), 2);
}

// ---------------------------------------------------------------------------------------------
// (l) Menü, (m) Kamera, Wechsel und Modelländerung

#[test]
fn menu_lists_drawings_and_is_blocked_in_sketch_mode() {
    let mut st = box_state();
    let a = st
        .document
        .add_drawing("Alpha", A4_QUER, Projection::First)
        .unwrap();
    let b = st
        .document
        .add_drawing("Beta", A4_QUER, Projection::First)
        .unwrap();
    let mut h = Harness::new();
    let mut dialog = None;
    let shot = h.frame_with(&mut st, &mut dialog, What::Menu, vec![]);
    for word in [
        "Neue Zeichnung …",
        "Öffnen: Alpha",
        "Öffnen: Beta",
        "Zum 3D-Modell",
    ] {
        assert!(
            shot.pos_of(word).is_some(),
            "„{word}“ fehlt:\n{}",
            shot.text
        );
    }
    let click =
        |h: &mut Harness, st: &mut AppState, dialog: &mut Option<NewDrawingDialog>, word: &str| {
            let shot = h.frame_with(st, dialog, What::Menu, vec![]);
            let p = shot
                .pos_of(word)
                .unwrap_or_else(|| panic!("„{word}“ fehlt"));
            h.frame_with(st, dialog, What::Menu, vec![Event::PointerMoved(p)]);
            h.frame_with(
                st,
                dialog,
                What::Menu,
                vec![button(p, PointerButton::Primary, true)],
            );
            h.frame_with(
                st,
                dialog,
                What::Menu,
                vec![button(p, PointerButton::Primary, false)],
            );
        };
    click(&mut h, &mut st, &mut dialog, "Öffnen: Beta");
    assert_eq!(st.drawing_ui.area, WorkArea::Drawing(b));
    click(&mut h, &mut st, &mut dialog, "Zum 3D-Modell");
    assert_eq!(st.drawing_ui.area, WorkArea::Model);
    click(&mut h, &mut st, &mut dialog, "Neue Zeichnung …");
    assert_eq!(
        dialog.as_ref().map(|d| d.name.as_str()),
        Some("Zeichnung 1")
    );
    dialog = None;

    // Skizzen-Modus: alles gesperrt.
    let sk = st
        .add_sketch(planora_feature::SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    st.open_sketch(sk).unwrap();
    assert!(st.sketch_session.is_some());
    assert!(drawing_menu_blocked(&st).is_some());
    for word in ["Öffnen: Alpha", "Neue Zeichnung …"] {
        click(&mut h, &mut st, &mut dialog, word);
    }
    assert_eq!(st.drawing_ui.area, WorkArea::Model);
    assert!(dialog.is_none());
    let _ = a;
}

#[test]
fn model_camera_is_unchanged_by_switching() {
    let (mut st, d, front, _, mut h) = front_and_top();
    st.camera.orbit(37.0, -12.0);
    st.camera.zoom(3.0);
    st.selected_body = Some(0);
    let cam = st.camera;
    let sel = (st.selected_body, st.selection);
    st.drawing_ui.selected_view = Some(front);
    // Ziehen und Zoomen im Blatt.
    h.drag(
        &mut st,
        screen(&h, (148.5, 105.0)),
        vec2(5.0, 5.0),
        PointerButton::Primary,
    );
    h.click_text(&mut st, &zoom_in());
    h.click_text(&mut st, "3D-Modell");
    assert_eq!(st.drawing_ui.area, WorkArea::Model);
    assert_eq!(st.camera, cam);
    assert_eq!((st.selected_body, st.selection), sel);
    // Zurück in dieselbe Zeichnung: Kamera weiterhin gleich.
    st.drawing_ui.open(d);
    h.idle(&mut st);
    assert_eq!(st.camera, cam);
}

#[test]
fn switching_drawings_resets_the_selection() {
    let (mut st, a, front, _, mut h) = front_and_top();
    let b = st
        .document
        .add_drawing("Blatt 2", A4_QUER, Projection::First)
        .unwrap();
    st.document
        .add_view(
            b,
            ViewKind::Front,
            s(1, 1),
            Placement::Free { x: 100.0, y: 100.0 },
            None,
        )
        .unwrap();
    h.click(&mut st, screen(&h, (148.5, 105.0)));
    assert_eq!(st.drawing_ui.selected_view, Some(front));
    let mut dialog = None;
    let shot = h.frame_with(&mut st, &mut dialog, What::Menu, vec![]);
    let p = shot.pos_of("Öffnen: Blatt 2").unwrap();
    h.frame_with(
        &mut st,
        &mut dialog,
        What::Menu,
        vec![Event::PointerMoved(p)],
    );
    h.frame_with(
        &mut st,
        &mut dialog,
        What::Menu,
        vec![button(p, PointerButton::Primary, true)],
    );
    h.frame_with(
        &mut st,
        &mut dialog,
        What::Menu,
        vec![button(p, PointerButton::Primary, false)],
    );
    assert_eq!(st.drawing_ui.area, WorkArea::Drawing(b));
    assert_eq!(
        st.drawing_ui.selected_view, None,
        "Kennungen gelten je Zeichnung"
    );
    let shot = h.idle(&mut st);
    assert!(shot.text.contains("Zeichnung: Blatt 2"), "{}", shot.text);
    assert!(shot.pos_of("Ansicht löschen").is_none());
    let _ = a;
}

#[test]
fn a_model_edit_while_open_recomputes_and_drops_old_lines() {
    let (mut st, d, front, _, mut h) = front_and_top();
    assert_eq!(st.drawing_ui.spawned(), 2);
    assert_eq!(h.layout().lines_drawn, 8);
    // Quader auf 50 × 60 × 40 verkleinern.
    let id = st.document.features()[0].id;
    st.document
        .update(id, |k| {
            if let FeatureKind::Box { size, .. } = k {
                size.x = 50.0;
            }
        })
        .unwrap();
    st.refresh();
    // Gleich im nächsten Bild: keine alten Linien mehr, beide Ansichten rechnen.
    h.idle(&mut st);
    assert_eq!(h.layout().lines_drawn, 0, "alte Linien verschwinden sofort");
    assert_eq!(st.drawing_ui.spawned(), 4);
    let start = Instant::now();
    while h.layout().lines_drawn < 8 && start.elapsed() < Duration::from_secs(20) {
        std::thread::sleep(Duration::from_millis(10));
        h.idle(&mut st);
    }
    assert_eq!(h.layout().lines_drawn, 8);
    assert_eq!(st.drawing_ui.spawned(), 4, "nur einmal neu gerechnet");
    let l = h.layout();
    let rect = l.views.iter().find(|v| v.id == front).unwrap().rect;
    let k = l.px_per_mm;
    assert!(
        (rect.width() - 50.0 * k).abs() < 0.5,
        "neue Breite 50 mm: {rect:?}"
    );
    let _ = d;
}

// ---------------------------------------------------------------------------------------------
// Aufgabe 16, Übernahme aus der Prüfung von Aufgabe 15

/// Klick auf `word` in der gewählten Darstellung (`What`), in vier Bildern wie `Harness::click`.
fn click_in(
    h: &mut Harness,
    st: &mut AppState,
    what: What,
    dialog: &mut Option<NewDrawingDialog>,
    word: &str,
) {
    let shot = h.frame_with(st, dialog, what, vec![]);
    let p = shot
        .pos_of(word)
        .unwrap_or_else(|| panic!("„{word}“ fehlt:\n{}", shot.text));
    h.frame_with(st, dialog, what, vec![Event::PointerMoved(p)]);
    h.frame_with(
        st,
        dialog,
        what,
        vec![button(p, PointerButton::Primary, true)],
    );
    h.frame_with(
        st,
        dialog,
        what,
        vec![button(p, PointerButton::Primary, false)],
    );
    h.frame_with(st, dialog, what, vec![]);
}

#[test]
fn the_menu_open_entry_keeps_the_sheet_state_of_the_same_drawing() {
    let (mut st, d, front, _, mut h) = front_and_top();
    let other = st
        .document
        .add_drawing("Zweite", A4_QUER, Projection::First)
        .unwrap();
    st.drawing_ui.selected_view = Some(front);
    st.drawing_ui.zoom = 3.0;
    st.drawing_ui.pan = vec2(20.0, -9.0);
    let mut dialog = None;
    // Zum 3D-Modell und mit „Öffnen: Blatt 1“ zurück: Zoom, Verschiebung und Auswahl bleiben.
    click_in(&mut h, &mut st, What::Menu, &mut dialog, "Zum 3D-Modell");
    assert_eq!(st.drawing_ui.area, WorkArea::Model);
    click_in(&mut h, &mut st, What::Menu, &mut dialog, "Öffnen: Blatt 1");
    assert_eq!(st.drawing_ui.area, WorkArea::Drawing(d));
    assert_eq!(st.drawing_ui.zoom, 3.0);
    assert_eq!(st.drawing_ui.pan, vec2(20.0, -9.0));
    assert_eq!(st.drawing_ui.selected_view, Some(front));
    // Eine andere Zeichnung beginnt frisch, und zurück ebenso (Blatt 1 ist nicht mehr die
    // zuletzt offene).
    click_in(&mut h, &mut st, What::Menu, &mut dialog, "Öffnen: Zweite");
    assert_eq!(st.drawing_ui.area, WorkArea::Drawing(other));
    assert_eq!(
        (st.drawing_ui.zoom, st.drawing_ui.selected_view),
        (1.0, None)
    );
    st.drawing_ui.zoom = 4.0;
    click_in(&mut h, &mut st, What::Menu, &mut dialog, "Öffnen: Blatt 1");
    assert_eq!(st.drawing_ui.area, WorkArea::Drawing(d));
    assert_eq!(st.drawing_ui.zoom, 1.0);
}

#[test]
fn dragging_a_view_does_not_start_a_computation_and_counts_one_edit_per_move() {
    let (mut st, d, front, _, mut h) = front_and_top();
    let spawned = st.drawing_ui.spawned();
    let model = st.document.model_edit_count();
    let edits = st.document.edit_count();
    let p = screen(&h, (148.5, 105.0));
    // `drag` bewegt in zwei Schritten (halbe und ganze Strecke).
    h.drag(&mut st, p, vec2(30.0, 40.0), PointerButton::Primary);
    assert_ne!(
        placement(&st, d, front),
        Placement::Free { x: 148.5, y: 105.0 }
    );
    assert_eq!(
        st.drawing_ui.spawned(),
        spawned,
        "Ziehen rechnet nichts neu"
    );
    assert_eq!(st.document.model_edit_count(), model, "Modellstand bleibt");
    // Jedes Bild mit Bewegung ist eine Änderung des Dokuments (`edit_count`): zwei Schritte.
    assert_eq!(st.document.edit_count() - edits, 2);
    // Ein Ziehen ohne Bewegung ändert nichts.
    let edits = st.document.edit_count();
    let p = screen(&h, (148.5, 105.0));
    h.drag(&mut st, p, vec2(0.0, 0.0), PointerButton::Primary);
    assert_eq!(st.document.edit_count(), edits);
}

#[test]
fn a_read_only_area_disables_the_toolbar_and_the_drag() {
    let (mut st, d, front, top, mut h) = front_and_top();
    let before = st.document.drawing(d).unwrap().clone();
    st.drawing_ui.selected_view = Some(top);
    let mut none = None;
    // Werkzeugleiste: nichts davon ändert das Dokument.
    for word in ["Verdeckte Kanten zeigen", "Ansicht löschen"] {
        click_in(&mut h, &mut st, What::AreaReadOnly, &mut none, word);
        assert_eq!(st.document.drawing(d).unwrap(), &before, "{word}");
    }
    // Das Aufklappmenü „Ansicht hinzufügen“ bleibt zu.
    click_in(
        &mut h,
        &mut st,
        What::AreaReadOnly,
        &mut none,
        "Ansicht hinzufügen",
    );
    let shot = h.frame_with(&mut st, &mut none, What::AreaReadOnly, vec![]);
    assert!(shot.pos_of("Isometrisch").is_none(), "{}", shot.text);
    // Der Maßstab lässt sich nicht öffnen und wählen.
    click_in(&mut h, &mut st, What::AreaReadOnly, &mut none, "1:1");
    let shot = h.frame_with(&mut st, &mut none, What::AreaReadOnly, vec![]);
    assert!(shot.pos_of("1:50").is_none(), "{}", shot.text);
    assert_eq!(st.document.drawing(d).unwrap(), &before);
    // Ziehen wählt, bewegt aber nichts.
    let p = screen(&h, (148.5, 105.0));
    let down = |pos| button(pos, PointerButton::Primary, true);
    let up = |pos| button(pos, PointerButton::Primary, false);
    let f = |h: &mut Harness, st: &mut AppState, ev: Vec<Event>| {
        h.frame_with(st, &mut None, What::AreaReadOnly, ev);
    };
    f(&mut h, &mut st, vec![Event::PointerMoved(p)]);
    f(&mut h, &mut st, vec![down(p)]);
    f(
        &mut h,
        &mut st,
        vec![Event::PointerMoved(p + vec2(15.0, 20.0))],
    );
    f(
        &mut h,
        &mut st,
        vec![Event::PointerMoved(p + vec2(30.0, 40.0))],
    );
    f(&mut h, &mut st, vec![up(p + vec2(30.0, 40.0))]);
    f(&mut h, &mut st, vec![]);
    assert_eq!(st.drawing_ui.selected_view, Some(front), "Wählen geht");
    assert_eq!(st.document.drawing(d).unwrap(), &before, "nichts bewegt");

    // Menü „Zeichnung“: „Neue Zeichnung …“ gesperrt, Öffnen geht.
    st.drawing_ui.area = WorkArea::Model;
    let mut dialog = None;
    click_in(
        &mut h,
        &mut st,
        What::MenuReadOnly,
        &mut dialog,
        "Neue Zeichnung …",
    );
    assert!(dialog.is_none(), "gesperrt in alten Versionen");
    click_in(
        &mut h,
        &mut st,
        What::MenuReadOnly,
        &mut dialog,
        "Öffnen: Blatt 1",
    );
    assert_eq!(st.drawing_ui.area, WorkArea::Drawing(d));
}

/// Ein Zylinder: runde Linien (Ellipsen, Bögen) in mehreren Ansichten.
fn disc_state() -> (AppState, DrawingId) {
    let mut st = AppState::new();
    st.create_cylinder(20.0, 30.0).unwrap();
    let d = st
        .document
        .add_drawing("Blatt 1", A4_QUER, Projection::First)
        .unwrap();
    let front = st
        .document
        .add_view(
            d,
            ViewKind::Front,
            s(1, 1),
            Placement::Free { x: 100.0, y: 105.0 },
            None,
        )
        .unwrap();
    st.document
        .add_view(
            d,
            ViewKind::Top,
            s(1, 1),
            Placement::Aligned { distance: 60.0 },
            Some(front),
        )
        .unwrap();
    st.document
        .add_view(
            d,
            ViewKind::Isometric,
            s(1, 1),
            Placement::Free { x: 220.0, y: 140.0 },
            None,
        )
        .unwrap();
    st.drawing_ui.open(d);
    (st, d)
}

#[test]
fn curves_are_flattened_once_per_model_and_zoom_and_never_on_an_unchanged_frame() {
    let (mut st, _) = disc_state();
    block(&mut st);
    let mut h = Harness::new();
    let c0 = drawing_view::flatten_calls();
    h.idle(&mut st);
    let lines = h.layout().lines_drawn as u64;
    let first = drawing_view::flatten_calls() - c0;
    assert!(lines >= 6, "runde und gerade Linien: {lines}");
    assert!(
        first > 0 && first <= lines,
        "höchstens einmal je Linie: {first} für {lines}"
    );
    // Unverändertes Bild: nichts wird neu zerlegt – auch beim Zeiger über dem Blatt.
    let c1 = drawing_view::flatten_calls();
    for _ in 0..3 {
        h.idle(&mut st);
    }
    h.frame(&mut st, vec![Event::PointerMoved(pos2(500.0, 300.0))]);
    assert_eq!(drawing_view::flatten_calls(), c1, "Bild ohne Änderung");
    // Ein Klick (Treffersuche) im selben Zoom nutzt dieselben Linienzüge.
    let p = screen(&h, (100.0, 105.0));
    h.click(&mut st, p);
    assert_eq!(
        drawing_view::flatten_calls(),
        c1,
        "Treffersuche ohne neues Zerlegen"
    );
    // Ein anderer Zoom zerlegt genau einmal je Linie neu.
    h.click_text(&mut st, &zoom_in());
    let after_zoom = drawing_view::flatten_calls() - c1;
    assert!(
        after_zoom > 0 && after_zoom <= lines,
        "{after_zoom} für {lines}"
    );
    let c2 = drawing_view::flatten_calls();
    h.idle(&mut st);
    h.idle(&mut st);
    assert_eq!(drawing_view::flatten_calls(), c2);
    // Eine Änderung am Modell (Linien fallen weg und kommen neu) zerlegt neu.
    st.create_box(DVec3::new(10.0, 10.0, 10.0)).unwrap();
    block(&mut st);
    h.idle(&mut st);
    h.idle(&mut st);
    let after_model = drawing_view::flatten_calls() - c2;
    assert!(after_model > 0, "neues Modell, neue Linien");
    let c3 = drawing_view::flatten_calls();
    h.idle(&mut st);
    assert_eq!(drawing_view::flatten_calls(), c3);
}

#[test]
fn failure_text_is_clipped_to_its_frame() {
    // Kleine Ansicht (1:5) mit langem Fehlertext: der Text wird auf den Rahmen beschnitten,
    // nicht auf das ganze Blatt.
    let mut st = box_state();
    st.drawing_ui.set_compute(top_fails);
    let (d, front) = with_front(&mut st);
    let top = add_child(&mut st, d, front, ViewKind::Top, 70.0);
    st.document
        .update_drawing(d, |dr| {
            dr.views.iter_mut().find(|v| v.id == top).unwrap().scale = s(1, 5);
        })
        .unwrap();
    block(&mut st);
    let mut h = Harness::new();
    h.idle(&mut st);
    let shot = h.idle(&mut st);
    let frame = h.layout().views.iter().find(|v| v.id == top).unwrap().rect;
    let msg = DrawError::TooComplex(5).to_string();
    let (_, clip) = shot
        .clips
        .iter()
        .find(|(t, _)| t.contains(&msg))
        .expect("Fehlertext gezeichnet");
    assert!(
        frame.expand(0.5).contains_rect(*clip),
        "Ausschnitt {clip:?} größer als der Rahmen {frame:?}"
    );
    // Ebenso der Platzhalter einer Berechnung.
    let mut st = box_state();
    st.drawing_ui.set_compute(wait_for_cancel);
    let (_, front) = with_front(&mut st);
    let mut h = Harness::new();
    let start = Instant::now();
    let shot = loop {
        let shot = h.idle(&mut st);
        if shot.text.contains("25 %") || start.elapsed() > Duration::from_secs(10) {
            break shot;
        }
        std::thread::sleep(Duration::from_millis(5));
    };
    let frame = h
        .layout()
        .views
        .iter()
        .find(|v| v.id == front)
        .unwrap()
        .rect;
    let (_, clip) = shot
        .clips
        .iter()
        .find(|(t, _)| t.contains("wird berechnet"))
        .expect("Platzhaltertext");
    assert!(
        frame.expand(0.5).contains_rect(*clip),
        "{clip:?} / {frame:?}"
    );
}
