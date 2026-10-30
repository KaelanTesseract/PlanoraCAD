//! Browser-Abschnitt „Maße“ (Teilprojekt 12, Aufgabe 13): Zeilen, Wählen, Bearbeiten, Löschen mit
//! Rückfrage bei Maßketten, Speichern und Laden – mit echten egui-Ereignissen wie
//! `tests/drawing_browser.rs`; die erwarteten Texte stehen wörtlich im Test.
//!
//! Sollwerte aus den Eingabemaßen (von Hand gerechnet): Quader 10 × 20 × 30 (x, y, z), Zylinder
//! r 5, h 12. Breite 10, Höhe 30, Winkel zwischen linker und unterer Vorderkante 90°,
//! Durchmesser 10, Radius 5. Mit einer Nachkommastelle: „10,0“, „30,0“, „90,0°“, „Ø10,0“,
//! „R5,0“.

use egui::epaint::Shape;
use egui::{Color32, Event, Modifiers, PointerButton, Pos2, Rect, vec2};
use planora_app::browser::{self, ERROR_COLOR};
use planora_app::dimension_browser::{chain_ask, empty_text};
use planora_app::drawing_state::WorkArea;
use planora_app::state::AppState;
use planora_app::workspace::*;
use planora_app::{fonts, testing::collect_texts};
use planora_drawing::dimension::{
    ChainMode, ChainSpec, DimId, DimKind, DimSpec, GeoRef, GeoRefData, Tolerance as DimTolerance,
};
use planora_drawing::{DrawingId, Paper, Placement, Projection, Scale, Sheet, ViewId, ViewKind};
use planora_feature::geo_ref_for_edge;
use planora_math::{DVec3, Tolerance};
use planora_project::config::Config;
use planora_project::docstore::LockMode;
use planora_project::fsutil::new_id;
use planora_project::library::Library;

const W: f32 = 400.0;
const H: f32 = 900.0;

const A4_QUER: Sheet = Sheet {
    paper: Paper::A4,
    landscape: true,
};

struct Seen {
    text: String,
    rect: Rect,
    color: Color32,
}

fn walk(shape: &Shape, out: &mut Vec<Seen>) {
    match shape {
        Shape::Text(t) => {
            let section = t.galley.job.sections.first();
            let color = section
                .map(|s| s.format.color)
                .filter(|c| *c != Color32::PLACEHOLDER)
                .unwrap_or(t.fallback_color);
            out.push(Seen {
                text: t.galley.text().to_string(),
                rect: Rect::from_min_size(t.pos, t.galley.size()),
                color,
            });
        }
        Shape::Vec(v) => v.iter().for_each(|s| walk(s, out)),
        _ => {}
    }
}

struct Shot {
    text: String,
    texts: Vec<Seen>,
}

impl Shot {
    fn find(&self, exact: &str) -> Option<&Seen> {
        self.texts.iter().find(|t| t.text == exact)
    }

    fn pos_of(&self, exact: &str) -> Pos2 {
        self.find(exact)
            .unwrap_or_else(|| panic!("„{exact}“ nicht sichtbar:\n{}", self.text))
            .rect
            .left_center()
            + vec2(6.0, 0.0)
    }
}

struct Harness {
    ctx: egui::Context,
    time: f64,
    read_only: bool,
}

impl Harness {
    fn new() -> Harness {
        let ctx = egui::Context::default();
        ctx.set_fonts(fonts::definitions());
        Harness {
            ctx,
            time: 0.0,
            read_only: false,
        }
    }

    fn read_only() -> Harness {
        Harness {
            read_only: true,
            ..Harness::new()
        }
    }

    fn frame(&mut self, st: &mut AppState, events: Vec<Event>) -> Shot {
        self.time += 0.05;
        let ro = self.read_only;
        let raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(W, H))),
            events,
            time: Some(self.time),
            ..Default::default()
        };
        let out = self.ctx.run_ui(raw, |ui| browser::show_with(ui, st, ro));
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
        self.frame(st, vec![]);
        self.frame(st, vec![])
    }

    fn press(p: Pos2, b: PointerButton, pressed: bool) -> Event {
        Event::PointerButton {
            pos: p,
            button: b,
            pressed,
            modifiers: Modifiers::NONE,
        }
    }

    fn click_at(&mut self, st: &mut AppState, p: Pos2, b: PointerButton) {
        self.frame(st, vec![Event::PointerMoved(p)]);
        self.frame(st, vec![Self::press(p, b, true)]);
        self.frame(st, vec![Self::press(p, b, false)]);
    }

    fn click(&mut self, st: &mut AppState, exact: &str) {
        let p = self.idle(st).pos_of(exact);
        self.click_at(st, p, PointerButton::Primary);
        self.idle(st);
    }

    fn double_click(&mut self, st: &mut AppState, exact: &str) {
        self.time += 1.0;
        let p = self.idle(st).pos_of(exact);
        self.frame(st, vec![Event::PointerMoved(p)]);
        let b = PointerButton::Primary;
        self.frame(
            st,
            vec![
                Self::press(p, b, true),
                Self::press(p, b, false),
                Self::press(p, b, true),
                Self::press(p, b, false),
            ],
        );
        self.idle(st);
    }

    fn open_menu(&mut self, st: &mut AppState, row: &str) -> Shot {
        let p = self.idle(st).pos_of(row);
        self.click_at(st, p, PointerButton::Secondary);
        self.idle(st)
    }

    fn menu(&mut self, st: &mut AppState, row: &str, entry: &str) {
        let shot = self.open_menu(st, row);
        let p = shot.pos_of(entry);
        self.click_at(st, p, PointerButton::Primary);
        self.idle(st);
    }

    fn rows(&self) -> Vec<(String, Rect)> {
        browser::row_rects(&self.ctx)
    }

    /// Texte der letzten `n` Zeilen.
    fn tail(&self, n: usize) -> Vec<String> {
        let rows = self.rows();
        rows[rows.len() - n..].iter().map(|r| r.0.clone()).collect()
    }
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
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

/// Obere Kreiskante des Körpers `body`.
fn top_circle(st: &AppState, body: usize) -> GeoRefData {
    let solid = &st.bodies[body].solid;
    let top = st.bodies[body].bounds.1.z;
    let (id, _) = solid
        .edges
        .iter()
        .find(|(_, e)| e.start == e.end && near(solid.vertices.get(e.start).unwrap().point.z, top))
        .expect("Kreis nicht gefunden");
    geo_ref_for_edge(solid, id, &Tolerance::default()).unwrap()
}

fn spec(view: ViewId, kind: DimKind, refs: Vec<GeoRef>) -> DimSpec {
    DimSpec {
        id: DimId(0),
        view,
        kind,
        refs,
        offset: 10.0,
        angle: 45.0,
        text_shift: (0.0, 0.0),
        tolerance: DimTolerance::None,
        decimals: 1,
        text_override: None,
        chain: None,
        suggested: false,
    }
}

fn width_refs(st: &AppState, body: usize) -> Vec<GeoRef> {
    let (lo, hi) = st.bodies[body].bounds;
    vec![
        GeoRef::Edge(edge_where(st, body, |p| near(p.x, lo.x) && near(p.y, lo.y))),
        GeoRef::Edge(edge_where(st, body, |p| near(p.x, hi.x) && near(p.y, lo.y))),
    ]
}

fn height_refs(st: &AppState, body: usize) -> Vec<GeoRef> {
    let (lo, hi) = st.bodies[body].bounds;
    vec![
        GeoRef::Edge(edge_where(st, body, |p| near(p.z, lo.z) && near(p.y, lo.y))),
        GeoRef::Edge(edge_where(st, body, |p| near(p.z, hi.z) && near(p.y, lo.y))),
    ]
}

struct Fixture {
    drawing: DrawingId,
    ids: Vec<DimId>,
}

/// Quader 10 × 20 × 30 und Zylinder r 5, h 12; Zeichnung „Blatt 1“ mit Vorderansicht (Breite
/// mit ±0,1, Höhe mit +0,2/−0,1, Höhe als Grenzmaße mit 2 Stellen, Winkel mit eigenem Text) und
/// Draufsicht (Durchmesser, Radius mit Passung H7). Die Zeichnung ist noch nicht geöffnet.
fn build(st: &mut AppState) -> Fixture {
    st.create_box(DVec3::new(10.0, 20.0, 30.0)).unwrap();
    st.create_cylinder(5.0, 12.0).unwrap();
    let d = st
        .document
        .add_drawing("Blatt 1", A4_QUER, Projection::First)
        .unwrap();
    let one = Scale { num: 1, den: 1 };
    let front = st
        .document
        .add_view(
            d,
            ViewKind::Front,
            one,
            Placement::Free { x: 100.0, y: 100.0 },
            None,
        )
        .unwrap();
    let top = st
        .document
        .add_view(
            d,
            ViewKind::Top,
            one,
            Placement::Free { x: 230.0, y: 100.0 },
            None,
        )
        .unwrap();
    let (lo, _) = st.bodies[0].bounds;
    let add = |st: &mut AppState, s: DimSpec| st.document.add_dimension(d, s).unwrap();
    let mut s = spec(front, DimKind::Horizontal, width_refs(st, 0));
    s.tolerance = DimTolerance::Symmetric(0.1);
    let w = add(st, s);
    let mut s = spec(front, DimKind::Vertical, height_refs(st, 0));
    s.tolerance = DimTolerance::Limits {
        upper: 0.2,
        lower: -0.1,
    };
    let h1 = add(st, s);
    let mut s = spec(front, DimKind::Vertical, height_refs(st, 0));
    s.offset = 17.0;
    s.decimals = 2;
    s.tolerance = DimTolerance::LimitSizes {
        upper: 0.2,
        lower: -0.1,
    };
    let h2 = add(st, s);
    let refs = vec![
        GeoRef::Edge(edge_where(st, 0, |p| near(p.x, lo.x) && near(p.y, lo.y))),
        GeoRef::Edge(edge_where(st, 0, |p| near(p.z, lo.z) && near(p.y, lo.y))),
    ];
    let mut s = spec(front, DimKind::Angle, refs);
    s.text_override = Some("Winkel <>".into());
    let a = add(st, s);
    let circle = GeoRef::Edge(top_circle(st, 1));
    let dia = add(st, spec(top, DimKind::Diameter, vec![circle.clone()]));
    let mut s = spec(top, DimKind::Radius, vec![circle]);
    s.tolerance = DimTolerance::Fit("H7".into());
    let r = add(st, s);
    Fixture {
        drawing: d,
        ids: vec![w, h1, h2, a, dia, r],
    }
}

/// Zustand mit geöffneter, berechneter Zeichnung.
fn opened() -> (AppState, Fixture, Harness) {
    let mut st = AppState::new();
    let f = build(&mut st);
    st.drawing_ui.open(f.drawing);
    st.drawing_ui.tick(&st.bodies, &st.document, true);
    let mut h = Harness::new();
    h.idle(&mut st);
    (st, f, h)
}

const ALL_ROWS: [&str; 8] = [
    "Vorne",
    "10,0 ±0,1",
    "30,0 +0,2 / \u{2212}0,1",
    "30,20 / 29,90",
    "Winkel 90,0°",
    "Oben",
    "Ø10,0",
    "R5,0 H7",
];

// ---------------------------------------------------------------------------------------------
// (i)

#[test]
fn rows_and_texts_for_all_kinds() {
    let (mut st, _, mut h) = opened();
    let shot = h.idle(&mut st);
    assert!(shot.find("Maße").is_some(), "Überschrift:\n{}", shot.text);
    let mut expected: Vec<String> = ALL_ROWS.iter().map(|s| s.to_string()).collect();
    expected[0] = ViewKind::Front.label().to_string();
    expected[5] = ViewKind::Top.label().to_string();
    assert_eq!(h.tail(8), expected);
    for r in &ALL_ROWS[1..5] {
        assert!(shot.find(r).is_some(), "„{r}“ fehlt:\n{}", shot.text);
    }
    // Nichts davon ist rot.
    for t in &shot.texts {
        if ALL_ROWS.contains(&t.text.as_str()) {
            assert_ne!(t.color, ERROR_COLOR, "{}", t.text);
        }
    }
    // Der Abschnitt steht unter „Zeichnungen“, die Zeilen unter der Überschrift „Maße“.
    let y = |s: &str| shot.find(s).unwrap().rect.top();
    assert!(y("Zeichnungen") < y("Maße") && y("Maße") < y("10,0 ±0,1"));
    // Dieselben Texte wie auf dem Blatt: der Maßtext der Breite steht auch dort.
    // Die Rechnung folgt dem Modell: Quader 10 → 12 breit.
    st.document
        .update(st.document.features()[0].id, |k| {
            if let planora_feature::FeatureKind::Box { size, .. } = k {
                size.x = 12.5;
            }
        })
        .unwrap();
    st.refresh();
    st.drawing_ui.tick(&st.bodies, &st.document, true);
    let shot = h.idle(&mut st);
    assert!(
        shot.find("12,5 ±0,1").is_some(),
        "Wert folgt dem Modell:\n{}",
        shot.text
    );
    assert!(shot.find("10,0 ±0,1").is_none());
}

#[test]
fn no_section_without_an_open_drawing() {
    let mut st = AppState::new();
    build(&mut st);
    let mut h = Harness::new();
    let shot = h.idle(&mut st);
    assert!(shot.find("Maße").is_none(), "{}", shot.text);
    assert!(!shot.text.contains("Noch keine Maße"));
    st.drawing_ui.area = WorkArea::Model;
    assert!(h.idle(&mut st).find("Maße").is_none());
}

// ---------------------------------------------------------------------------------------------
// (j)

#[test]
fn lost_row_is_red_with_reason() {
    // Zweiter Quader trägt das Maß; er wird gelöscht.
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
            Placement::Free { x: 100.0, y: 100.0 },
            None,
        )
        .unwrap();
    let refs = width_refs(&st, 1);
    st.document
        .add_dimension(d, spec(front, DimKind::Horizontal, refs))
        .unwrap();
    st.drawing_ui.open(d);
    st.drawing_ui.tick(&st.bodies, &st.document, true);
    let mut h = Harness::new();
    let shot = h.idle(&mut st);
    assert!(shot.find("10,0").is_some());
    st.delete_feature(b).unwrap();
    st.drawing_ui.tick(&st.bodies, &st.document, true);
    let shot = h.idle(&mut st);
    assert!(
        shot.find("10,0").is_none(),
        "kein stehengebliebener Wert:\n{}",
        shot.text
    );
    let q = shot.find("?").expect("Zeile mit Fragezeichen");
    assert_eq!(q.color, ERROR_COLOR);
    assert_eq!(h.tail(2), [ViewKind::Front.label().as_str(), "?"]);
    // Hinweistext mit dem Grund.
    let p = q.rect.center();
    h.frame(&mut st, vec![Event::PointerMoved(p)]);
    let mut text = String::new();
    for _ in 0..30 {
        text = h.idle(&mut st).text;
    }
    assert!(
        text.contains("an der das Maß hing, gibt es nicht mehr"),
        "Grund fehlt:\n{text}"
    );
    // Das Maß selbst lässt sich wählen und löschen.
    h.click(&mut st, "?");
    assert!(st.drawing_ui.selected_dim.is_some());
}

// ---------------------------------------------------------------------------------------------
// (k)

#[test]
fn click_selects_double_click_edits_context_menu_deletes() {
    let (mut st, f, mut h) = opened();
    let w = f.ids[0];
    // Klick wählt und lässt die Zeichnung offen.
    st.drawing_ui.selected_dim = None;
    h.click(&mut st, "10,0 ±0,1");
    assert_eq!(st.drawing_ui.selected_dim, Some(w));
    assert_eq!(st.drawing_ui.selected_view, None);
    assert_eq!(st.drawing_ui.area, WorkArea::Drawing(f.drawing));
    assert!(
        st.dimension_dialog.is_none(),
        "ein Klick öffnet kein Fenster"
    );
    // Klick aus dem 3D-Modell heraus öffnet die Zeichnung.
    st.drawing_ui.area = WorkArea::Model;
    st.drawing_ui.selected_dim = None;
    // (im Modell gibt es den Abschnitt nicht; das Wählen der Zeichnung selbst öffnet sie)
    h.click(&mut st, "Blatt 1");
    h.double_click(&mut st, "Blatt 1");
    assert_eq!(st.drawing_ui.area, WorkArea::Drawing(f.drawing));
    // Doppelklick öffnet „Maß bearbeiten“ für genau dieses Maß.
    let hgt = f.ids[1];
    h.double_click(&mut st, "30,0 +0,2 / \u{2212}0,1");
    let dlg = st.dimension_dialog.as_ref().expect("Fenster offen");
    assert_eq!(
        (dlg.drawing, dlg.dim, dlg.read_only),
        (f.drawing, hgt, false)
    );
    assert_eq!(st.drawing_ui.selected_dim, Some(hgt));
    st.dimension_dialog = None;
    // Rechtsklick → „Bearbeiten“.
    h.menu(&mut st, "Ø10,0", "Bearbeiten");
    let dlg = st.dimension_dialog.as_ref().expect("Fenster offen");
    assert_eq!(dlg.dim, f.ids[4]);
    st.dimension_dialog = None;
    // Rechtsklick → „Löschen“: kein Kettenglied, also ohne Rückfrage, ein Schritt.
    let e0 = st.document.edit_count();
    h.menu(&mut st, "R5,0 H7", "Löschen");
    assert!(st.pending_delete_dimension.is_none());
    assert_eq!(st.document.edit_count(), e0 + 1);
    let d = st.document.drawing(f.drawing).unwrap();
    assert!(d.dimension(f.ids[5]).is_none());
    assert_eq!(d.dimensions.len(), 5);
    assert!(h.idle(&mut st).find("R5,0 H7").is_none());

    // Alte Version: Wählen und Ansehen gehen, Löschen ist gesperrt.
    let (mut st, f, _) = opened();
    let mut h = Harness::read_only();
    h.idle(&mut st);
    let before = st.document.drawing(f.drawing).unwrap().clone();
    h.click(&mut st, "Ø10,0");
    assert_eq!(st.drawing_ui.selected_dim, Some(f.ids[4]));
    h.double_click(&mut st, "Ø10,0");
    let dlg = st.dimension_dialog.as_ref().expect("zum Ansehen offen");
    assert!(dlg.read_only);
    st.dimension_dialog = None;
    let shot = h.open_menu(&mut st, "Ø10,0");
    assert!(shot.find("Ansehen").is_some(), "{}", shot.text);
    assert!(shot.find("Bearbeiten").is_none(), "{}", shot.text);
    let p = shot.pos_of("Löschen");
    h.click_at(&mut st, p, PointerButton::Primary);
    h.idle(&mut st);
    assert_eq!(st.document.drawing(f.drawing).unwrap(), &before);
    assert!(st.pending_delete_dimension.is_none());
}

// ---------------------------------------------------------------------------------------------
// (l)

/// Breite und zweite Breite (mit 2 Stellen) als Kette, dazu ein einzelnes Maß.
fn chain_state() -> (AppState, DrawingId, [DimId; 3], Harness) {
    let mut st = AppState::new();
    st.create_box(DVec3::new(10.0, 20.0, 30.0)).unwrap();
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
            Placement::Free { x: 100.0, y: 100.0 },
            None,
        )
        .unwrap();
    let mut a = spec(front, DimKind::Horizontal, width_refs(&st, 0));
    a.chain = Some(ChainSpec {
        group: 1,
        mode: ChainMode::Chain,
        order: 0,
    });
    let mut b = a.clone();
    b.decimals = 2;
    b.chain = Some(ChainSpec {
        group: 1,
        mode: ChainMode::Chain,
        order: 1,
    });
    let c = spec(front, DimKind::Vertical, height_refs(&st, 0));
    let ids = [
        st.document.add_dimension(d, a).unwrap(),
        st.document.add_dimension(d, b).unwrap(),
        st.document.add_dimension(d, c).unwrap(),
    ];
    st.drawing_ui.open(d);
    st.drawing_ui.tick(&st.bodies, &st.document, true);
    let mut h = Harness::new();
    h.idle(&mut st);
    (st, d, ids, h)
}

#[test]
fn chain_delete_asks_first() {
    let (mut st, d, ids, mut h) = chain_state();
    assert_eq!(
        h.tail(4),
        [ViewKind::Front.label().as_str(), "10,0", "10,00", "30,0"]
    );
    let e0 = st.document.edit_count();
    h.menu(&mut st, "10,00", "Löschen");
    // Rückfrage mit dem vorgegebenen Text; noch ist nichts gelöscht.
    assert_eq!(
        chain_ask().text(),
        "Dieses Maß gehört zu einer Kette; die anderen Maße bleiben"
    );
    let plan = st.pending_delete_dimension.clone().expect("Rückfrage");
    assert_eq!(
        (plan.drawing, plan.dim, plan.text.text()),
        (d, ids[1], chain_ask().text())
    );
    assert_eq!(st.document.edit_count(), e0);
    assert_eq!(st.document.drawing(d).unwrap().dimensions.len(), 3);
    let shot = h.idle(&mut st);
    assert!(shot.find(&chain_ask().text()).is_some(), "{}", shot.text);
    // „Abbrechen“: nichts passiert.
    h.click(&mut st, "Abbrechen");
    assert!(st.pending_delete_dimension.is_none());
    assert_eq!(st.document.drawing(d).unwrap().dimensions.len(), 3);
    assert_eq!(st.document.edit_count(), e0);
    // Noch einmal und bestätigen: nur dieses Maß verschwindet, die anderen bleiben.
    h.menu(&mut st, "10,00", "Löschen");
    assert!(st.pending_delete_dimension.is_some());
    h.click(&mut st, "Löschen");
    assert!(st.pending_delete_dimension.is_none());
    let dr = st.document.drawing(d).unwrap();
    assert_eq!(
        dr.dimensions.iter().map(|m| m.id).collect::<Vec<_>>(),
        [ids[0], ids[2]]
    );
    assert_eq!(st.document.edit_count(), e0 + 1);
    assert_eq!(
        h.tail(3),
        [ViewKind::Front.label().as_str(), "10,0", "30,0"]
    );
    // Ein Maß, das nicht zu einer Kette gehört, wird ohne Rückfrage gelöscht.
    h.menu(&mut st, "30,0", "Löschen");
    assert!(st.pending_delete_dimension.is_none());
    assert_eq!(st.document.drawing(d).unwrap().dimensions.len(), 1);
}

// ---------------------------------------------------------------------------------------------
// (m)

#[test]
fn empty_state_text() {
    let mut st = AppState::new();
    st.create_box(DVec3::new(10.0, 20.0, 30.0)).unwrap();
    let d = st
        .document
        .add_drawing("Blatt 1", A4_QUER, Projection::First)
        .unwrap();
    st.document
        .add_view(
            d,
            ViewKind::Front,
            Scale { num: 1, den: 1 },
            Placement::Free { x: 100.0, y: 100.0 },
            None,
        )
        .unwrap();
    st.drawing_ui.open(d);
    st.drawing_ui.tick(&st.bodies, &st.document, true);
    let mut h = Harness::new();
    let shot = h.idle(&mut st);
    assert_eq!(
        empty_text(),
        "Noch keine Maße. Mit dem Werkzeug „Maß“ oder „Maße vorschlagen“ lassen sich welche anlegen."
    );
    assert!(shot.find(&empty_text()).is_some(), "{}", shot.text);
    assert!(shot.find("Maße").is_some());
}

// ---------------------------------------------------------------------------------------------
// (n)

#[test]
fn rows_use_full_width_and_equal_heights() {
    let (mut st, _, mut h) = opened();
    h.idle(&mut st);
    let rows = h.rows();
    assert!(rows.len() >= 8 + 4, "{rows:?}");
    let width = rows[0].1.width();
    assert!(width > W - 40.0, "volle Breite: {width}");
    for (n, r) in &rows {
        assert_eq!(
            r.height(),
            planora_app::theme::ROW_H,
            "Zeile {n}: gleiche Höhe wie die Feature-Zeilen"
        );
        assert_eq!(r.width(), width, "Zeile {n}: gleiche Breite");
        assert_eq!(r.left(), rows[0].1.left(), "Zeile {n} beginnt links");
    }
    // Die Maßzeilen gehören dazu.
    let names: Vec<&str> = rows.iter().map(|r| r.0.as_str()).collect();
    assert!(names.contains(&"Ø10,0") && names.contains(&"30,20 / 29,90"));
}

// ---------------------------------------------------------------------------------------------
// (o)

#[test]
fn deleting_the_last_dimension_clears_selection() {
    let mut st = AppState::new();
    st.create_box(DVec3::new(10.0, 20.0, 30.0)).unwrap();
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
            Placement::Free { x: 100.0, y: 100.0 },
            None,
        )
        .unwrap();
    let refs = width_refs(&st, 0);
    let w = st
        .document
        .add_dimension(d, spec(front, DimKind::Horizontal, refs))
        .unwrap();
    st.drawing_ui.open(d);
    st.drawing_ui.tick(&st.bodies, &st.document, true);
    let mut h = Harness::new();
    h.click(&mut st, "10,0");
    assert_eq!(st.drawing_ui.selected_dim, Some(w));
    h.menu(&mut st, "10,0", "Löschen");
    assert_eq!(st.drawing_ui.selected_dim, None);
    assert!(st.document.drawing(d).unwrap().dimensions.is_empty());
    let shot = h.idle(&mut st);
    assert!(shot.find(&empty_text()).is_some(), "{}", shot.text);
    // Auch das offene Fenster „Maß bearbeiten“ für dieses Maß verschwindet.
    let mut st2 = AppState::new();
    let f = build(&mut st2);
    st2.drawing_ui.open(f.drawing);
    st2.drawing_ui.tick(&st2.bodies, &st2.document, true);
    let mut h2 = Harness::new();
    h2.double_click(&mut st2, "Ø10,0");
    assert!(st2.dimension_dialog.is_some());
    h2.menu(&mut st2, "Ø10,0", "Löschen");
    assert!(st2.dimension_dialog.is_none());
}

// ---------------------------------------------------------------------------------------------
// (p)

fn workspace() -> (Workspace, std::path::PathBuf) {
    let root = std::env::temp_dir().join(format!("planora-dim-{}", new_id()));
    let w = Workspace::new(Some(Library::open(&root).unwrap()), Config::default());
    (w, root)
}

#[test]
fn save_and_load_keep_dimensions_through_workspace() {
    let (mut w, root) = workspace();
    let p = w.library.as_ref().unwrap().create_project("P").unwrap();
    let f = build(&mut w.active_tab_mut().state);
    let before = w
        .active_tab()
        .state
        .document
        .drawing(f.drawing)
        .unwrap()
        .dimensions
        .clone();
    assert_eq!(before.len(), 6);
    w.save_as(&p, "Teil", "Maße").unwrap();
    let path = w.active_tab().path.clone().unwrap();
    w.close_tab(0);

    let mut w2 = Workspace::new(Some(Library::open(&root).unwrap()), Config::default());
    let r = w2.open_document(&path, LockMode::Acquire).unwrap();
    let tab = &mut w2.tabs[r.tab];
    let st = &mut tab.state;
    assert_eq!(
        st.document.drawing(f.drawing).unwrap().dimensions,
        before,
        "alle Maße unverändert"
    );
    // Die Werte werden neu berechnet: dieselben Zeilen wie vor dem Speichern.
    st.drawing_ui.open(f.drawing);
    st.drawing_ui.tick(&st.bodies, &st.document, true);
    let mut h = Harness::new();
    h.idle(st);
    let mut expected: Vec<String> = ALL_ROWS.iter().map(|s| s.to_string()).collect();
    expected[0] = ViewKind::Front.label().to_string();
    expected[5] = ViewKind::Top.label().to_string();
    assert_eq!(h.tail(8), expected);
    assert!(!tab.is_dirty());
}

#[test]
fn pending_question_is_dropped_when_its_target_disappears() {
    let (mut st, d, ids, mut h) = chain_state();
    h.menu(&mut st, "10,00", "Löschen");
    assert!(st.pending_delete_dimension.is_some());
    st.document.delete_dimension(d, ids[1]).unwrap();
    h.idle(&mut st);
    assert!(st.pending_delete_dimension.is_none());
    h.menu(&mut st, "10,0", "Löschen");
    assert!(st.pending_delete_dimension.is_some());
    st.document.delete_drawing(d).unwrap();
    h.idle(&mut st);
    assert!(st.pending_delete_dimension.is_none());
}
