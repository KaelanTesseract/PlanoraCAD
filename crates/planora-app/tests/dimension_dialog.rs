//! Fenster „Maß bearbeiten“ (Teilprojekt 12, Aufgabe 13) – mit echten egui-Ereignissen wie
//! `tests/drawing_ui.rs`; die Zustandsfunktionen werden zusätzlich direkt geprüft.
//!
//! Sollwerte aus den Eingabemaßen (von Hand gerechnet): Quader 10 × 20 × 30 (x, y, z),
//! Vorderansicht 1:1 mit Mitte (100, 100). Das Breitenmaß misst 10 mm; mit einer Nachkommastelle
//! steht dort „10,0“. Toleranz 0,2 / −0,1 an 10 mm: Höchstmaß 10,2, Mindestmaß 9,9.

use egui::epaint::Shape;
use egui::{Color32, Event, Key, Modifiers, PointerButton, Pos2, Rect, vec2};
use planora_app::browser::ERROR_COLOR;
use planora_app::dimension_browser::row_text;
use planora_app::dimension_dialog::*;
use planora_app::dimension_state::resolve_in;
use planora_app::drawing_view;
use planora_app::fonts;
use planora_app::state::*;
use planora_app::testing::collect_texts;
use planora_drawing::dimension::{
    DimId, DimKind, DimSpec, GeoRef, GeoRefData, Tolerance as DimTolerance,
};
use planora_drawing::{DrawingId, Paper, Placement, Projection, Scale, Sheet, ViewId, ViewKind};
use planora_feature::geo_ref_for_edge;
use planora_math::{DVec3, Tolerance};

const W: f32 = 1200.0;
const H: f32 = 800.0;

const A4_QUER: Sheet = Sheet {
    paper: Paper::A4,
    landscape: true,
};

const BOX: DVec3 = DVec3::new(10.0, 20.0, 30.0);

struct Seen {
    text: String,
    rect: Rect,
    color: Color32,
}

fn walk(shape: &Shape, out: &mut Vec<Seen>) {
    match shape {
        Shape::Vec(v) => v.iter().for_each(|s| walk(s, out)),
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
            .center()
    }
}

struct Harness {
    ctx: egui::Context,
    time: f64,
    dlg: DimensionDialog,
    open: bool,
}

impl Harness {
    fn new(dlg: DimensionDialog) -> Harness {
        let ctx = egui::Context::default();
        ctx.set_fonts(fonts::definitions());
        Harness {
            ctx,
            time: 0.0,
            dlg,
            open: true,
        }
    }

    fn frame(&mut self, st: &mut AppState, events: Vec<Event>) -> Shot {
        self.time += 0.05;
        let mut all = vec![Event::ModifiersChanged(Modifiers::NONE)];
        all.extend(events);
        let raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(W, H))),
            events: all,
            time: Some(self.time),
            ..Default::default()
        };
        let dlg = &mut self.dlg;
        let open = &mut self.open;
        let out = self.ctx.run_ui(raw, |ui| {
            if *open {
                *open = dlg.show(ui.ctx(), st);
            }
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
        self.frame(st, vec![]);
        self.frame(st, vec![])
    }

    fn click_at(&mut self, st: &mut AppState, p: Pos2) {
        let btn = |pressed| Event::PointerButton {
            pos: p,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        self.frame(st, vec![Event::PointerMoved(p)]);
        self.frame(st, vec![btn(true)]);
        self.frame(st, vec![btn(false)]);
        self.idle(st);
    }

    fn click(&mut self, st: &mut AppState, exact: &str) {
        let p = self.idle(st).pos_of(exact);
        self.click_at(st, p);
    }

    fn enabled(&self, label: &str) -> bool {
        self.dlg
            .inputs
            .iter()
            .find(|(l, _)| l == label)
            .unwrap_or_else(|| panic!("Bedienelement „{label}“ fehlt: {:?}", self.dlg.inputs))
            .1
    }
}

fn new_drawing(st: &mut AppState) -> DrawingId {
    st.document
        .add_drawing("Blatt 1", A4_QUER, Projection::First)
        .unwrap()
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

fn add_width(st: &mut AppState, d: DrawingId, view: ViewId, body: usize) -> DimId {
    let (lo, hi) = st.bodies[body].bounds;
    let refs = vec![
        GeoRef::Edge(edge_where(st, body, |p| near(p.x, lo.x) && near(p.y, lo.y))),
        GeoRef::Edge(edge_where(st, body, |p| near(p.x, hi.x) && near(p.y, lo.y))),
    ];
    st.document
        .add_dimension(d, spec(view, DimKind::Horizontal, refs, 10.0))
        .unwrap()
}

/// Quader, Zeichnung mit Vorderansicht und Breitenmaß (10 mm), geöffnet und berechnet.
fn state() -> (AppState, DrawingId, ViewId, DimId) {
    let mut st = AppState::new();
    st.create_box(BOX).unwrap();
    let d = new_drawing(&mut st);
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
    let w = add_width(&mut st, d, front, 0);
    st.drawing_ui.open(d);
    st.drawing_ui.tick(&st.bodies, &st.document, true);
    (st, d, front, w)
}

fn dialog(st: &AppState, d: DrawingId, id: DimId, read_only: bool) -> DimensionDialog {
    let s = st.document.drawing(d).unwrap().dimension(id).unwrap();
    DimensionDialog::new(d, s, read_only)
}

fn spec_of(st: &AppState, d: DrawingId, id: DimId) -> DimSpec {
    st.document
        .drawing(d)
        .unwrap()
        .dimension(id)
        .unwrap()
        .clone()
}

/// Text der Zeile im Browser für das Maß (aufgelöst wie in der Oberfläche).
fn text_of(st: &mut AppState, d: DrawingId, id: DimId) -> String {
    let dims = resolve_in(st, d);
    let (s, r) = dims
        .by_view
        .values()
        .flatten()
        .find(|(s, _)| s.id == id)
        .expect("Maß vorhanden");
    row_text(s, r)
}

// ---------------------------------------------------------------------------------------------
// (a)

#[test]
fn dialog_changes_decimals_and_tolerance_kinds() {
    let (mut st, d, _, w) = state();
    let before = spec_of(&st, d, w);
    assert_eq!(text_of(&mut st, d, w), "10,0");

    // Symmetrisch ±0,1 mit 2 Nachkommastellen: „10,00 ±0,10“.
    let mut dlg = dialog(&st, d, w, false);
    assert_eq!(dlg.kind, ToleranceKind::None);
    dlg.decimals = 2;
    dlg.kind = ToleranceKind::Symmetric;
    dlg.symmetric = "0,1".into();
    let e0 = st.document.edit_count();
    dlg.apply(&mut st).unwrap();
    assert_eq!(st.document.edit_count(), e0 + 1, "ein Schritt");
    let s = spec_of(&st, d, w);
    assert_eq!(s.tolerance, DimTolerance::Symmetric(0.1));
    assert_eq!(s.decimals, 2);
    assert_eq!(text_of(&mut st, d, w), "10,00 ±0,10");
    // Alles andere bleibt, wie es war.
    assert_eq!(
        (
            &s.refs,
            s.offset,
            s.angle,
            s.text_shift,
            s.chain,
            s.suggested
        ),
        (
            &before.refs,
            before.offset,
            before.angle,
            before.text_shift,
            before.chain,
            before.suggested
        )
    );

    // Oben und unten +0,2 / −0,1 (Komma und echtes Minus), 1 Stelle.
    let mut dlg = dialog(&st, d, w, false);
    assert_eq!(dlg.kind, ToleranceKind::Symmetric);
    assert_eq!(dlg.symmetric, "0,1");
    dlg.decimals = 1;
    dlg.kind = ToleranceKind::Limits;
    dlg.upper = "0,2".into();
    dlg.lower = "\u{2212}0,1".into();
    dlg.apply(&mut st).unwrap();
    assert_eq!(
        spec_of(&st, d, w).tolerance,
        DimTolerance::Limits {
            upper: 0.2,
            lower: -0.1
        }
    );
    assert_eq!(text_of(&mut st, d, w), "10,0 +0,2 / \u{2212}0,1");

    // Grenzmaße: Abweichungen +0,2 / −0,1 → Höchstmaß 10,2, Mindestmaß 9,9 (nicht 0,2 / −0,1).
    let mut dlg = dialog(&st, d, w, false);
    assert_eq!((dlg.upper.as_str(), dlg.lower.as_str()), ("0,2", "-0,1"));
    dlg.kind = ToleranceKind::LimitSizes;
    dlg.apply(&mut st).unwrap();
    assert_eq!(
        spec_of(&st, d, w).tolerance,
        DimTolerance::LimitSizes {
            upper: 0.2,
            lower: -0.1
        }
    );
    assert_eq!(text_of(&mut st, d, w), "10,2 / 9,9");
    let dlg = dialog(&st, d, w, false);
    assert_eq!(dlg.kind, ToleranceKind::LimitSizes);
    assert_eq!((dlg.upper.as_str(), dlg.lower.as_str()), ("0,2", "-0,1"));

    // Passung ist nur Text.
    let mut dlg = dialog(&st, d, w, false);
    dlg.kind = ToleranceKind::Fit;
    dlg.fit = "H7".into();
    dlg.decimals = 3;
    dlg.apply(&mut st).unwrap();
    assert_eq!(spec_of(&st, d, w).tolerance, DimTolerance::Fit("H7".into()));
    assert_eq!(spec_of(&st, d, w).decimals, 3);
    assert_eq!(text_of(&mut st, d, w), "10,000 H7");

    // Zurück auf „Keine“ und 0 Stellen.
    let mut dlg = dialog(&st, d, w, false);
    assert_eq!(dlg.fit, "H7");
    dlg.kind = ToleranceKind::None;
    dlg.decimals = 0;
    dlg.apply(&mut st).unwrap();
    assert_eq!(spec_of(&st, d, w).tolerance, DimTolerance::None);
    assert_eq!(text_of(&mut st, d, w), "10");

    // Ohne Änderung bleibt das Dokument unberührt.
    let e = st.document.edit_count();
    dialog(&st, d, w, false).apply(&mut st).unwrap();
    assert_eq!(st.document.edit_count(), e);

    // Über die Oberfläche: Auswahl der Art, Zahl eintippen, „Übernehmen“.
    let mut h = Harness::new(dialog(&st, d, w, false));
    let shot = h.idle(&mut st);
    for t in [
        "Maß",
        "Toleranz",
        "Art",
        "Wert",
        "Nachkommastellen",
        "Maßtext",
    ] {
        assert!(shot.find(t).is_some(), "„{t}“ fehlt:\n{}", shot.text);
    }
    assert!(shot.find("Waagerecht").is_some(), "Art: {}", shot.text);
    h.dlg.kind = ToleranceKind::Symmetric;
    h.dlg.symmetric = "0,25".into();
    h.dlg.decimals = 2;
    h.idle(&mut st);
    h.click(&mut st, "Übernehmen");
    assert!(!h.open, "Fenster geschlossen");
    assert_eq!(
        spec_of(&st, d, w).tolerance,
        DimTolerance::Symmetric(0.25),
        "genau eingegebene Zahl"
    );
    assert_eq!(text_of(&mut st, d, w), "10,00 ±0,25");
}

// ---------------------------------------------------------------------------------------------
// (b)

#[test]
fn invalid_numbers_block_apply_with_red_text() {
    let (mut st, d, _, w) = state();
    let mut h = Harness::new(dialog(&st, d, w, false));
    h.dlg.kind = ToleranceKind::Symmetric;
    let e0 = st.document.edit_count();
    for (input, error) in [
        ("abc", "Abweichung: „abc“ ist keine Zahl."),
        ("", "Abweichung: Bitte eine Zahl eingeben."),
        ("1001", "Abweichung: Die Zahl darf höchstens 1000 betragen."),
        (
            "-1000,5",
            "Abweichung: Die Zahl darf höchstens 1000 betragen.",
        ),
        ("NaN", "Abweichung: „NaN“ ist keine Zahl."),
        ("inf", "Abweichung: „inf“ ist keine Zahl."),
        ("1e999", "Abweichung: „1e999“ ist keine Zahl."),
        ("0,1,2", "Abweichung: „0,1,2“ ist keine Zahl."),
        (
            "-0,1",
            "Abweichung: Die Abweichung darf nicht negativ sein.",
        ),
        (
            "\u{2212}1000",
            "Abweichung: Die Abweichung darf nicht negativ sein.",
        ),
    ] {
        h.dlg.symmetric = input.into();
        let shot = h.idle(&mut st);
        let t = shot
            .find(error)
            .unwrap_or_else(|| panic!("„{error}“ fehlt für „{input}“:\n{}", shot.text));
        assert_eq!(t.color, ERROR_COLOR, "rot");
        assert!(!h.enabled("Übernehmen"), "gesperrt für „{input}“");
        // Ein Klick auf „Übernehmen“ ändert nichts, das Fenster bleibt.
        h.click(&mut st, "Übernehmen");
        assert!(h.open);
        assert_eq!(st.document.edit_count(), e0);
        assert!(h.dlg.apply(&mut st).is_err());
        assert_eq!(st.document.edit_count(), e0);
    }
    // Die Grenzen selbst gehen: 1000 und 0 (eine symmetrische Abweichung ist nie negativ).
    for ok in ["1000", "0", "-0", "0.5", "+0,5", "  0,5  "] {
        h.dlg.symmetric = ok.into();
        h.idle(&mut st);
        assert!(h.enabled("Übernehmen"), "„{ok}“ muss gehen");
        assert!(h.dlg.errors().is_empty(), "{ok}");
    }
    // Oben und unten: beide Felder werden geprüft, beide Fehler stehen da.
    h.dlg.kind = ToleranceKind::Limits;
    h.dlg.upper = "x".into();
    h.dlg.lower = "y".into();
    let shot = h.idle(&mut st);
    assert!(
        shot.find("Oben: „x“ ist keine Zahl.").is_some(),
        "{}",
        shot.text
    );
    assert!(
        shot.find("Unten: „y“ ist keine Zahl.").is_some(),
        "{}",
        shot.text
    );
    assert!(!h.enabled("Übernehmen"));
    // Obere Abweichung kleiner als die untere: abgelehnt (gleich geht), in beiden Arten.
    for (kind, label) in [
        (ToleranceKind::Limits, "Oben"),
        (ToleranceKind::LimitSizes, "Obere Abweichung"),
    ] {
        h.dlg.kind = kind;
        h.dlg.upper = "\u{2212}0,2".into();
        h.dlg.lower = "-0,1".into();
        let shot = h.idle(&mut st);
        let want = format!("{label}: Die obere Abweichung darf nicht kleiner als die untere sein.");
        let t = shot
            .find(&want)
            .unwrap_or_else(|| panic!("„{want}“ fehlt:\n{}", shot.text));
        assert_eq!(t.color, ERROR_COLOR, "rot");
        assert!(!h.enabled("Übernehmen"));
        assert!(h.dlg.apply(&mut st).is_err());
        assert_eq!(st.document.edit_count(), e0);
        h.dlg.upper = "-0,1".into();
        h.idle(&mut st);
        assert!(h.enabled("Übernehmen"), "gleich groß geht");
    }
    // Passung: leer, 51 Zeichen zu lang, 50 gehen.
    h.dlg.kind = ToleranceKind::Fit;
    for empty in ["", "   ", "\t "] {
        h.dlg.fit = empty.into();
        let shot = h.idle(&mut st);
        assert!(
            shot.find("Passung: Bitte eine Passung eingeben.").is_some(),
            "nur Leerzeichen zählt als keine Passung: „{empty}“"
        );
    }
    h.dlg.fit = "H".repeat(51);
    let shot = h.idle(&mut st);
    let t = shot
        .find("Passung: Der Text darf höchstens 50 Zeichen haben.")
        .unwrap_or_else(|| panic!("{}", shot.text));
    assert_eq!(t.color, ERROR_COLOR);
    assert!(!h.enabled("Übernehmen"));
    h.dlg.fit = "H".repeat(50);
    h.idle(&mut st);
    assert!(h.enabled("Übernehmen"));
    // Maßtext: 201 Zeichen zu lang, 200 gehen (gezählt in Zeichen, nicht in Bytes).
    h.dlg.kind = ToleranceKind::None;
    h.dlg.text = "ä".repeat(201);
    let shot = h.idle(&mut st);
    let t = shot
        .find("Maßtext: Der Text darf höchstens 200 Zeichen haben.")
        .unwrap_or_else(|| panic!("{}", shot.text));
    assert_eq!(t.color, ERROR_COLOR);
    assert!(!h.enabled("Übernehmen"));
    h.dlg.text = "ä".repeat(200);
    h.idle(&mut st);
    assert!(h.enabled("Übernehmen"));
    h.dlg.apply(&mut st).unwrap();
    assert_eq!(spec_of(&st, d, w).text_override, Some("ä".repeat(200)));
}

#[test]
fn typing_into_a_number_field_goes_through_the_dialog() {
    let (mut st, d, _, w) = state();
    let mut h = Harness::new(dialog(&st, d, w, false));
    h.dlg.kind = ToleranceKind::Symmetric;
    h.dlg.symmetric = "0,1".into();
    h.idle(&mut st);
    // Klick ins Feld, alles markieren, tippen.
    h.click(&mut st, "0,1");
    h.frame(
        &mut st,
        vec![Event::Key {
            key: Key::A,
            physical_key: Some(Key::A),
            pressed: true,
            repeat: false,
            modifiers: Modifiers::COMMAND,
        }],
    );
    h.frame(&mut st, vec![Event::Text("x7".into())]);
    h.idle(&mut st);
    assert_eq!(h.dlg.symmetric, "x7");
    let shot = h.idle(&mut st);
    assert!(shot.find("Abweichung: „x7“ ist keine Zahl.").is_some());
    assert!(!h.enabled("Übernehmen"));
}

// ---------------------------------------------------------------------------------------------
// (c)

#[test]
fn override_text_with_placeholder() {
    let (mut st, d, _, w) = state();
    let mut h = Harness::new(dialog(&st, d, w, false));
    let shot = h.idle(&mut st);
    assert!(
        shot.find("<> steht für den berechneten Wert").is_some(),
        "Hinweis fehlt:\n{}",
        shot.text
    );
    h.dlg.text = "Breite <> mm".into();
    h.click(&mut st, "Übernehmen");
    assert!(!h.open);
    assert_eq!(
        spec_of(&st, d, w).text_override,
        Some("Breite <> mm".to_string())
    );
    // Der Wert steht an Stelle von „<>“.
    assert_eq!(text_of(&mut st, d, w), "Breite 10,0 mm");

    // Wieder öffnen: das Feld zeigt den Text; leeren heißt „berechneten Wert zeigen“.
    let mut h = Harness::new(dialog(&st, d, w, false));
    assert_eq!(h.dlg.text, "Breite <> mm");
    h.dlg.text = String::new();
    h.idle(&mut st);
    h.click(&mut st, "Übernehmen");
    assert_eq!(spec_of(&st, d, w).text_override, None);
    assert_eq!(text_of(&mut st, d, w), "10,0");
    // Nur Leerzeichen ist kein eigener Text; Leerzeichen am Rand fallen weg, auch bei der
    // Passung.
    let mut dlg = dialog(&st, d, w, false);
    dlg.text = "   ".into();
    dlg.apply(&mut st).unwrap();
    assert_eq!(spec_of(&st, d, w).text_override, None);
    assert_eq!(text_of(&mut st, d, w), "10,0");
    dlg.text = "  Breite <>  ".into();
    dlg.kind = ToleranceKind::Fit;
    dlg.fit = " H7 ".into();
    dlg.apply(&mut st).unwrap();
    assert_eq!(
        spec_of(&st, d, w).text_override,
        Some("Breite <>".to_string())
    );
    assert_eq!(spec_of(&st, d, w).tolerance, DimTolerance::Fit("H7".into()));
    dlg.kind = ToleranceKind::None;
    dlg.text = String::new();
    dlg.apply(&mut st).unwrap();

    // Nachkommastellen wirken auch im eigenen Text: 3 Stellen → „<>“ wird „10,000“.
    let mut dlg = dialog(&st, d, w, false);
    dlg.text = "<>".into();
    dlg.decimals = 3;
    dlg.apply(&mut st).unwrap();
    assert_eq!(text_of(&mut st, d, w), "10,000");
}

// ---------------------------------------------------------------------------------------------
// (d)

#[test]
fn delete_button_removes_the_dimension_and_closes() {
    let (mut st, d, _, w) = state();
    st.drawing_ui.selected_dim = Some(w);
    let mut h = Harness::new(dialog(&st, d, w, false));
    h.idle(&mut st);
    let e0 = st.document.edit_count();
    h.click(&mut st, "Maß löschen");
    assert!(!h.open, "Fenster geschlossen");
    assert!(st.document.drawing(d).unwrap().dimension(w).is_none());
    assert_eq!(st.document.edit_count(), e0 + 1, "ein Schritt");
    assert_eq!(st.drawing_ui.selected_dim, None);
    assert!(st.pending_delete_dimension.is_none());
    // Das Fenster eines Maßes, das es nicht mehr gibt, schließt sich von selbst.
    let (mut st, d, _, w) = state();
    let mut h = Harness::new(dialog(&st, d, w, false));
    h.idle(&mut st);
    st.document.delete_dimension(d, w).unwrap();
    h.idle(&mut st);
    assert!(!h.open);
}

// ---------------------------------------------------------------------------------------------
// (e)

#[test]
fn lost_dimension_shows_the_reason_and_the_last_value_is_not_shown() {
    // Zwei Quader; das Maß gehört zum zweiten, der danach gelöscht wird.
    let mut st = AppState::new();
    st.create_box(BOX).unwrap();
    let b = st.create_box(BOX).unwrap();
    let d = new_drawing(&mut st);
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
    let w = add_width(&mut st, d, front, 1);
    st.document
        .update_dimension(d, w, |s| s.decimals = 3)
        .unwrap();
    st.drawing_ui.open(d);
    st.drawing_ui.tick(&st.bodies, &st.document, true);
    let mut h = Harness::new(dialog(&st, d, w, false));
    let shot = h.idle(&mut st);
    assert!(shot.find("10,000").is_some(), "Wert vorher:\n{}", shot.text);
    st.delete_feature(b).unwrap();
    st.drawing_ui.tick(&st.bodies, &st.document, true);
    let shot = h.idle(&mut st);
    assert!(
        !shot.text.contains("10,000"),
        "alter Wert darf nicht stehen:\n{}",
        shot.text
    );
    let red = shot
        .texts
        .iter()
        .find(|t| t.text.starts_with("Das Maß hat seinen Bezug verloren: "))
        .unwrap_or_else(|| panic!("Grund fehlt:\n{}", shot.text));
    assert_eq!(red.color, ERROR_COLOR);
    assert!(
        red.text.contains("an der das Maß hing, gibt es nicht mehr"),
        "{}",
        red.text
    );
    // Nachkommastellen und Toleranz lassen sich trotzdem ändern.
    h.dlg.decimals = 1;
    h.dlg.kind = ToleranceKind::Symmetric;
    h.dlg.symmetric = "0,5".into();
    h.idle(&mut st);
    assert!(h.enabled("Übernehmen"));
    h.click(&mut st, "Übernehmen");
    assert_eq!(spec_of(&st, d, w).tolerance, DimTolerance::Symmetric(0.5));
}

// ---------------------------------------------------------------------------------------------
// (f)

#[test]
fn read_only_tab_disables_every_input() {
    let (mut st, d, _, w) = state();
    let before = st.document.drawing(d).unwrap().clone();
    let mut h = Harness::new(dialog(&st, d, w, true));
    // Alle Toleranzarten durchgehen: die Felder jeder Art sind gesperrt.
    let mut labels = Vec::new();
    for k in ToleranceKind::ALL {
        h.dlg.kind = k;
        let shot = h.idle(&mut st);
        assert!(
            shot.find("In einer alten Version lässt sich nichts ändern.")
                .is_some(),
            "{}",
            shot.text
        );
        assert!(!h.dlg.inputs.is_empty());
        for (l, on) in &h.dlg.inputs {
            assert!(!on, "„{l}“ ist bedienbar ({k:?})");
            labels.push(l.clone());
        }
    }
    for l in [
        "Nachkommastellen",
        "Maßtext",
        "Toleranzart",
        "Abweichung ±",
        "Oben",
        "Unten",
        "Obere Abweichung",
        "Untere Abweichung",
        "Passung",
        "Übernehmen",
        "Maß löschen",
    ] {
        assert!(labels.iter().any(|x| x == l), "„{l}“ fehlt: {labels:?}");
    }
    // Tippen in jedes Feld ändert nichts (die Felder sind wirklich gesperrt).
    h.dlg.symmetric = "0,1".into();
    h.dlg.upper = "0,2".into();
    h.dlg.lower = "0,3".into();
    h.dlg.fit = "H7".into();
    h.dlg.text = "Text".into();
    let snapshot = (
        h.dlg.symmetric.clone(),
        h.dlg.upper.clone(),
        h.dlg.lower.clone(),
        h.dlg.fit.clone(),
        h.dlg.text.clone(),
    );
    for k in ToleranceKind::ALL {
        h.dlg.kind = k;
        let shot = h.idle(&mut st);
        for t in ["0,1", "0,2", "0,3", "H7", "Text"] {
            if let Some(f) = shot.find(t) {
                let p = f.rect.center();
                h.click_at(&mut st, p);
                h.frame(
                    &mut st,
                    vec![Event::Key {
                        key: Key::A,
                        physical_key: Some(Key::A),
                        pressed: true,
                        repeat: false,
                        modifiers: Modifiers::COMMAND,
                    }],
                );
                h.frame(&mut st, vec![Event::Text("zz".into())]);
                h.idle(&mut st);
            }
        }
        assert_eq!(h.dlg.kind, k);
    }
    assert_eq!(
        snapshot,
        (
            h.dlg.symmetric.clone(),
            h.dlg.upper.clone(),
            h.dlg.lower.clone(),
            h.dlg.fit.clone(),
            h.dlg.text.clone()
        ),
        "gesperrte Felder lassen sich nicht ändern"
    );
    // Weder die Knöpfe noch der Zustand ändern das Dokument.
    h.click(&mut st, "Maß löschen");
    h.click(&mut st, "Übernehmen");
    assert!(h.dlg.apply(&mut st).is_err());
    assert_eq!(st.document.drawing(d).unwrap(), &before);
    assert!(h.open, "bleibt offen");
    // Doppelklick auf ein Maß im Blatt öffnet das Fenster nur zum Ansehen.
    h.click(&mut st, "Schließen");
    assert!(!h.open);
}

// ---------------------------------------------------------------------------------------------
// (g)

const ENGLISH: [&str; 23] = [
    "dimension",
    "dimensions",
    "decimals",
    "tolerance",
    "apply",
    "cancel",
    "delete",
    "close",
    "value",
    "kind",
    "symmetric",
    "upper",
    "lower",
    "limit",
    "limits",
    "fit",
    "override",
    "placeholder",
    "error",
    "lost",
    "invalid",
    "number",
    "reference",
];

fn english_words(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .map(str::to_lowercase)
        .filter(|w| ENGLISH.contains(&w.as_str()))
        .collect()
}

#[test]
fn german_texts_only() {
    let (mut st, d, _, w) = state();
    let mut all = String::new();
    let mut h = Harness::new(dialog(&st, d, w, false));
    for k in ToleranceKind::ALL {
        h.dlg.kind = k;
        h.dlg.symmetric = "x".into();
        h.dlg.upper = "".into();
        h.dlg.fit = "H".repeat(60);
        h.dlg.text = "t".repeat(201);
        all.push_str(&h.idle(&mut st).text);
        all.push_str(k.label());
    }
    let mut ro = Harness::new(dialog(&st, d, w, true));
    all.push_str(&ro.idle(&mut st).text);
    for k in [
        DimKind::Horizontal,
        DimKind::Vertical,
        DimKind::Aligned,
        DimKind::Diameter,
        DimKind::Radius,
        DimKind::Angle,
    ] {
        all.push_str(kind_label(k));
        all.push('\n');
    }
    all.push_str(PLACEHOLDER_HINT);
    let bad = english_words(&all);
    assert!(bad.is_empty(), "englische Wörter {bad:?} in:\n{all}");
    assert_eq!(english_words("Apply the Tolerance"), ["apply", "tolerance"]);
    // Die Beschriftungen sind die vorgegebenen deutschen.
    for t in [
        "Maß bearbeiten",
        "Übernehmen",
        "Abbrechen",
        "Maß löschen",
        "Keine",
        "Symmetrisch ±",
        "Oben und unten",
        "Grenzmaße",
        "Passung",
        "Nachkommastellen",
    ] {
        assert!(all.contains(t), "„{t}“ fehlt:\n{all}");
    }
    // Keine Hinweise auf die Herkunft der Daten.
    for w in ["Kernel", "Solid", "Exact", "Snapshot", "Datei", "STEP"] {
        assert!(!all.contains(w), "{w}");
    }
}

// ---------------------------------------------------------------------------------------------
// (h)

#[test]
fn double_click_on_the_sheet_dimension_opens_the_dialog() {
    let (mut st, d, _, w) = state();
    let ctx = egui::Context::default();
    ctx.set_fonts(fonts::definitions());
    let time = std::cell::Cell::new(0.0_f64);
    let frame = |st: &mut AppState, events: Vec<Event>, editable: bool| {
        time.set(time.get() + 0.05);
        let raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(W, H))),
            events,
            time: Some(time.get()),
            ..Default::default()
        };
        let out = ctx.run_ui(raw, |ui| drawing_view::show_with(ui, st, editable));
        let mut texts = Vec::new();
        for c in &out.shapes {
            walk(&c.shape, &mut texts);
        }
        out.drop_without_applying_deltas();
        texts
    };
    frame(&mut st, vec![], true);
    let texts = frame(&mut st, vec![], true);
    // Der Text „10,0“ steht über der Maßlinie des Breitenmaßes.
    let p = texts
        .iter()
        .find(|t| t.text == "10,0")
        .expect("Maßtext auf dem Blatt")
        .rect
        .center();
    // Einfacher Klick wählt nur.
    let btn = |pressed| Event::PointerButton {
        pos: p,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    };
    frame(&mut st, vec![Event::PointerMoved(p)], true);
    frame(&mut st, vec![btn(true), btn(false)], true);
    assert_eq!(st.drawing_ui.selected_dim, Some(w));
    assert!(st.dimension_dialog.is_none(), "ein Klick öffnet nichts");
    // Doppelklick öffnet das Fenster für dieses Maß.
    time.set(time.get() + 1.0);
    frame(&mut st, vec![Event::PointerMoved(p)], true);
    frame(
        &mut st,
        vec![btn(true), btn(false), btn(true), btn(false)],
        true,
    );
    let dlg = st.dimension_dialog.as_ref().expect("Fenster offen");
    assert_eq!((dlg.drawing, dlg.dim, dlg.read_only), (d, w, false));
    // In einer alten Version öffnet es sich nur zum Ansehen.
    st.dimension_dialog = None;
    time.set(time.get() + 1.0);
    frame(&mut st, vec![Event::PointerMoved(p)], false);
    frame(
        &mut st,
        vec![btn(true), btn(false), btn(true), btn(false)],
        false,
    );
    let dlg = st.dimension_dialog.as_ref().expect("Fenster offen");
    assert!(dlg.read_only);
}

// ---------------------------------------------------------------------------------------------
// Veraltete Fenster

#[test]
fn stale_dialog_closes_without_panic() {
    // Maß auf anderem Weg gelöscht.
    let (mut st, d, _, w) = state();
    let mut h = Harness::new(dialog(&st, d, w, false));
    h.idle(&mut st);
    st.document.delete_dimension(d, w).unwrap();
    assert!(!h.dlg.show(&h.ctx.clone(), &mut st));
    // Zeichnung gelöscht, während das Fenster offen ist.
    let (mut st, d, _, w) = state();
    let mut h = Harness::new(dialog(&st, d, w, false));
    h.idle(&mut st);
    st.document.delete_drawing(d).unwrap();
    h.idle(&mut st);
    assert!(!h.open, "Fenster geschlossen");
    assert!(!h.dlg.show(&h.ctx.clone(), &mut st));
    // Übernehmen und Löschen auf ein fehlendes Maß geben eine Meldung statt eines Absturzes.
    assert!(h.dlg.apply(&mut st).is_err());
    assert!(planora_app::dimension_browser::request_delete_dimension(&mut st, d, w).is_err());
}
