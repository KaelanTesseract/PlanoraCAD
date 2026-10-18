//! Browser-Abschnitt „Positionen“ (Teilprojekt 3c-2, Aufgabe 7): Gruppen je Ansicht, Zeilen
//! „Pos. n – Name“, Wählen, Löschen über das Kontextmenü, Sperre in alten Versionen, verlorene
//! Kreise – mit echten egui-Ereignissen wie `tests/dimension_browser.rs`.

use egui::epaint::Shape;
use egui::{Color32, Event, Modifiers, PointerButton, Pos2, Rect, vec2};
use planora_app::balloon_browser::{EMPTY_TEXT, SECTION, row_text};
use planora_app::balloon_tool::LOST_TIP;
use planora_app::browser::{self, ERROR_COLOR};
use planora_app::drawing_browser::READ_ONLY_HINT;
use planora_app::drawing_state::WorkArea;
use planora_app::drawing_view::view_names;
use planora_app::fonts;
use planora_app::state::AppState;
use planora_drawing::{
    BalloonId, BodyKey, DrawingId, Paper, Placement, Projection, Scale, Sheet, ViewId, ViewKind,
};
use planora_feature::FeatureId;
use planora_math::DVec3;

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
    texts: Vec<Seen>,
}

impl Shot {
    fn find(&self, exact: &str) -> Option<&Seen> {
        self.texts.iter().find(|t| t.text == exact)
    }

    fn has(&self, exact: &str) -> bool {
        self.find(exact).is_some()
    }

    fn pos_of(&self, exact: &str) -> Pos2 {
        self.find(exact)
            .unwrap_or_else(|| {
                panic!(
                    "„{exact}“ nicht sichtbar: {:?}",
                    self.texts.iter().map(|t| &t.text).collect::<Vec<_>>()
                )
            })
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
    fn new(read_only: bool) -> Harness {
        let ctx = egui::Context::default();
        ctx.set_fonts(fonts::definitions());
        ctx.all_styles_mut(|s| s.interaction.tooltip_delay = 0.0);
        Harness {
            ctx,
            time: 0.0,
            read_only,
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
        out.drop_without_applying_deltas();
        Shot { texts }
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

    fn menu(&mut self, st: &mut AppState, row: &str, entry: &str) {
        let p = self.idle(st).pos_of(row);
        self.click_at(st, p, PointerButton::Secondary);
        let p = self.idle(st).pos_of(entry);
        self.click_at(st, p, PointerButton::Primary);
        self.idle(st);
    }

    fn hover(&mut self, st: &mut AppState, exact: &str) -> Shot {
        let p = self.idle(st).pos_of(exact);
        self.frame(st, vec![Event::PointerMoved(p)]);
        for _ in 0..20 {
            self.frame(st, vec![]);
        }
        self.frame(st, vec![])
    }

    /// Zeilentexte des Browsers (in Reihenfolge).
    fn rows(&self) -> Vec<String> {
        browser::row_rects(&self.ctx)
            .into_iter()
            .map(|r| r.0)
            .collect()
    }
}

fn key(f: FeatureId) -> BodyKey {
    BodyKey {
        feature: f.0,
        index: 0,
    }
}

/// „Platte“ und „Bolzen“, Zeichnung mit Vorder- und Draufsicht; Kreise: zwei an der Platte
/// (vorne, oben), einer am Bolzen (vorne).
struct Fixture {
    st: AppState,
    drawing: DrawingId,
    front: ViewId,
    top: ViewId,
    bolt: FeatureId,
    ids: Vec<BalloonId>,
}

fn fixture() -> Fixture {
    let mut st = AppState::new();
    let plate = st.create_box(DVec3::new(40.0, 30.0, 5.0)).unwrap();
    let bolt = st.create_cylinder(3.0, 20.0).unwrap();
    st.rename_feature(plate, "Platte").unwrap();
    st.rename_feature(bolt, "Bolzen").unwrap();
    let d = st
        .document
        .add_drawing("Blatt 1", A4_QUER, Projection::First)
        .unwrap();
    let s11 = Scale { num: 1, den: 1 };
    let front = st
        .document
        .add_view(
            d,
            ViewKind::Front,
            s11,
            Placement::Free { x: 80.0, y: 100.0 },
            None,
        )
        .unwrap();
    let top = st
        .document
        .add_view(
            d,
            ViewKind::Top,
            s11,
            Placement::Free { x: 200.0, y: 100.0 },
            None,
        )
        .unwrap();
    let mut ids = Vec::new();
    for (view, f) in [(front, plate), (top, plate), (front, bolt)] {
        ids.push(
            st.document
                .add_balloon(d, view, key(f), (0.5, 0.5), (10.0, 10.0))
                .unwrap(),
        );
    }
    st.drawing_ui.open(d);
    Fixture {
        st,
        drawing: d,
        front,
        top,
        bolt,
        ids,
    }
}

#[test]
fn row_texts() {
    assert_eq!(row_text(Some(2), "Platte"), "Pos. 2 – Platte");
    assert_eq!(row_text(Some(12), "Bolzen"), "Pos. 12 – Bolzen");
    assert_eq!(row_text(None, "Platte"), "Pos. ? – Teil fehlt");
    assert_eq!(row_text(None, ""), "Pos. ? – Teil fehlt");
}

#[test]
fn section_lists_and_acts() {
    let Fixture {
        mut st,
        drawing: d,
        front,
        top,
        bolt,
        ids,
    } = fixture();
    let mut h = Harness::new(false);
    let shot = h.idle(&mut st);
    assert!(shot.has(SECTION));
    let names = view_names(st.document.drawing(d).unwrap());
    // Gruppen je Ansicht (in der Reihenfolge der Ansichten), darunter die Kreise.
    let rows = h.rows();
    let at = rows
        .iter()
        .rposition(|r| *r == names[&front])
        .expect("Gruppe vorne");
    assert_eq!(
        rows[at..at + 5].to_vec(),
        vec![
            names[&front].clone(),
            "Pos. 1 – Platte".into(),
            "Pos. 2 – Bolzen".into(),
            names[&top].clone(),
            "Pos. 1 – Platte".into(),
        ]
    );
    assert!(!shot.has(EMPTY_TEXT));
    // Klick wählt den Kreis (auch eine gewählte Ansicht oder ein Maß verfällt).
    st.drawing_ui.selected_view = Some(front);
    h.click(&mut st, "Pos. 2 – Bolzen");
    assert_eq!(st.drawing_ui.selected_balloon, Some(ids[2]));
    assert_eq!(st.drawing_ui.selected_view, None);
    assert_eq!(st.drawing_ui.area, WorkArea::Drawing(d));
    // Die eigene Benennung aus der Stückliste erscheint in der Zeile.
    let rk = st.document.parts_rows()[1].keys[0];
    st.document
        .set_parts_override(
            rk,
            planora_drawing::RowOverride {
                name: planora_drawing::sheet_info::FieldValue::Text("Achse".into()),
                ..Default::default()
            },
        )
        .unwrap();
    let shot = h.idle(&mut st);
    assert!(shot.has("Pos. 2 – Achse"), "eigene Benennung");
    // Kontextmenü „Löschen“: eine Änderung, die Auswahl verfällt.
    let e0 = st.document.edit_count();
    h.menu(&mut st, "Pos. 2 – Achse", "Löschen");
    let left: Vec<BalloonId> = st
        .document
        .drawing(d)
        .unwrap()
        .balloons
        .iter()
        .map(|b| b.id)
        .collect();
    assert_eq!(left, vec![ids[0], ids[1]]);
    assert_eq!(st.document.edit_count(), e0 + 1);
    assert_eq!(st.drawing_ui.selected_balloon, None);

    // Alte Version: Wählen geht, „Löschen“ ist gesperrt.
    let mut ro = Harness::new(true);
    ro.click(&mut st, "Pos. 1 – Platte");
    assert_eq!(st.drawing_ui.selected_balloon, Some(ids[0]));
    // Der Eintrag ist gesperrt und nennt den Grund.
    let p = ro.idle(&mut st).pos_of("Pos. 1 – Platte");
    ro.click_at(&mut st, p, PointerButton::Secondary);
    let q = ro.idle(&mut st).pos_of("Löschen");
    ro.frame(&mut st, vec![Event::PointerMoved(q)]);
    let mut shot = ro.idle(&mut st);
    for _ in 0..10 {
        shot = ro.idle(&mut st);
    }
    assert!(shot.has(READ_ONLY_HINT), "gesperrt mit Grund");
    let e0 = st.document.edit_count();
    ro.click_at(&mut st, q, PointerButton::Primary);
    ro.idle(&mut st);
    assert_eq!(st.document.drawing(d).unwrap().balloons.len(), 2);
    assert_eq!(st.document.edit_count(), e0);

    // Verlorener Kreis: rot, mit Grund als Hinweistext.
    st.document
        .add_balloon(d, front, key(bolt), (0.5, 0.5), (-10.0, 10.0))
        .unwrap();
    st.delete_feature(bolt).unwrap();
    let shot = h.idle(&mut st);
    let lost = shot.find("Pos. ? – Teil fehlt").expect("verlorene Zeile");
    assert_eq!(lost.color, ERROR_COLOR);
    let ok = shot.find("Pos. 1 – Platte").unwrap();
    assert_ne!(ok.color, ERROR_COLOR);
    let shot = h.hover(&mut st, "Pos. ? – Teil fehlt");
    assert!(shot.has(LOST_TIP), "Hinweistext fehlt");

    // Ohne Kreise: der Hinweis.
    st.document
        .update_drawing(d, |dr| dr.balloons.clear())
        .unwrap();
    let shot = h.idle(&mut st);
    assert!(shot.has(SECTION));
    assert!(shot.has(EMPTY_TEXT));
    assert!(!shot.texts.iter().any(|t| t.text.starts_with("Pos. ")));

    // Ohne geöffnete Zeichnung: kein Abschnitt.
    st.drawing_ui.area = WorkArea::Model;
    let shot = h.idle(&mut st);
    assert!(!shot.has(SECTION));
    assert!(!shot.has(EMPTY_TEXT));
}
