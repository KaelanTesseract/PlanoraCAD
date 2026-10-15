//! Browser-Abschnitt „Zeichnungen“ (Teilprojekt 11, Aufgabe 16): Zeilen, Öffnen, Umbenennen,
//! Löschen mit Rückfrage, Speichern und Laden, Ungespeichert-Marke.
//!
//! Alle Eingaben sind echte egui-Ereignisse; die erwarteten Texte stehen wörtlich im Test.

use egui::epaint::Shape;
use egui::{Event, Modifiers, PointerButton, Pos2, Rect, vec2};
use planora_app::drawing_browser::{self, DrawingDeletePlan};
use planora_app::drawing_state::WorkArea;
use planora_app::state::AppState;
use planora_app::workspace::*;
use planora_app::{browser, fonts, tabs, testing::collect_texts};
use planora_drawing::{
    DrawingId, Paper, Placement, Projection, Scale, Sheet, ViewId, ViewKind, ViewLines,
};
use planora_math::DVec3;
use planora_project::config::Config;
use planora_project::docstore::LockMode;
use planora_project::fsutil::new_id;
use planora_project::library::Library;
use std::time::{Duration, Instant};

const W: f32 = 400.0;
const H: f32 = 700.0;

const A3_QUER: Sheet = Sheet {
    paper: Paper::A3,
    landscape: true,
};
const A4_HOCH: Sheet = Sheet {
    paper: Paper::A4,
    landscape: false,
};

fn s(num: u32, den: u32) -> Scale {
    Scale { num, den }
}

struct Seen {
    text: String,
    rect: Rect,
}

fn walk(shape: &Shape, out: &mut Vec<Seen>) {
    match shape {
        Shape::Text(t) => out.push(Seen {
            text: t.galley.text().to_string(),
            rect: Rect::from_min_size(t.pos, t.galley.size()),
        }),
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

    /// Bild ohne Eingabe; zwei Bilder, damit neue Fenster und Zeilen vermessen sind.
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
        // Weit genug nach dem vorigen Klick, damit es nicht als dritter Klick zählt.
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

    /// Rechtsklick auf die Zeile `row`; liefert das Bild mit offenem Menü.
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
}

fn box_state() -> AppState {
    let mut st = AppState::new();
    st.create_box(DVec3::new(100.0, 60.0, 40.0)).unwrap();
    st
}

/// „Blatt 1“ (A3 quer, Erstwinkel) mit Vorne, Oben (abgeleitet), Vorne 2 (frei, 1:2) und
/// „Blatt 2“ (A4 hoch, Dritter Winkel) mit einer Ansicht.
fn two_drawings(st: &mut AppState) -> (DrawingId, DrawingId, [ViewId; 3]) {
    let d1 = st
        .document
        .add_drawing("Blatt 1", A3_QUER, Projection::First)
        .unwrap();
    let front = st
        .document
        .add_view(
            d1,
            ViewKind::Front,
            s(1, 1),
            Placement::Free { x: 200.0, y: 150.0 },
            None,
        )
        .unwrap();
    let top = st
        .document
        .add_view(
            d1,
            ViewKind::Top,
            s(1, 1),
            Placement::Aligned { distance: 70.0 },
            Some(front),
        )
        .unwrap();
    let front2 = st
        .document
        .add_view(
            d1,
            ViewKind::Front,
            s(1, 2),
            Placement::Free { x: 80.0, y: 250.0 },
            None,
        )
        .unwrap();
    let d2 = st
        .document
        .add_drawing("Blatt 2", A4_HOCH, Projection::Third)
        .unwrap();
    st.document
        .add_view(
            d2,
            ViewKind::Right,
            s(2, 1),
            Placement::Free { x: 100.0, y: 100.0 },
            None,
        )
        .unwrap();
    (d1, d2, [front, top, front2])
}

// (a) ------------------------------------------------------------------------------------------

#[test]
fn section_lists_drawings_and_views_with_german_labels() {
    let mut st = box_state();
    let (_, _, _) = two_drawings(&mut st);
    let mut h = Harness::new();
    let shot = h.idle(&mut st);
    for must in [
        "Zeichnungen",
        "Blatt 1",
        "A3 quer, Erstwinkel",
        "Vorne",
        "Oben",
        "Vorne 2",
        "1:2",
        "Blatt 2",
        "A4 hoch, Dritter Winkel",
        "Rechts",
        "2:1",
    ] {
        assert!(shot.find(must).is_some(), "„{must}“ fehlt:\n{}", shot.text);
    }
    // Reihenfolge von oben nach unten und Einrückung der Ansichten.
    let y = |t: &str| shot.find(t).unwrap().rect.center().y;
    let x = |t: &str| shot.find(t).unwrap().rect.left();
    assert!(y("Blatt 1") < y("Vorne"));
    assert!(y("Vorne") < y("Oben"));
    assert!(y("Oben") < y("Vorne 2"));
    assert!(y("Vorne 2") < y("Blatt 2"));
    assert!(y("Blatt 2") < y("Rechts"));
    assert!(x("Vorne") > x("Blatt 1") + 10.0, "Ansicht eingerückt");
    assert_eq!(x("Vorne"), x("Oben"));
    // Die Zeichnungen stehen unter dem Feature-Baum.
    assert!(y("Feature-Baum") < y("Quader 1"));
    assert!(y("Quader 1") < y("Zeichnungen"));
    // Klein und schwach dahinter: der Blattvermerk steht rechts vom Namen, in der Zeile.
    let name = shot.find("Blatt 1").unwrap().rect;
    let note = shot.find("A3 quer, Erstwinkel").unwrap().rect;
    assert!(note.left() > name.right());
    assert!((note.center().y - name.center().y).abs() < 1.0);
    assert!(note.height() < name.height(), "kleinere Schrift");
}

#[test]
fn paper_projection_and_orientation_texts_are_exact() {
    let mut st = box_state();
    for (name, sheet, proj, expect) in [
        ("Z1", A3_QUER, Projection::First, "A3 quer, Erstwinkel"),
        ("Z2", A4_HOCH, Projection::Third, "A4 hoch, Dritter Winkel"),
        (
            "Z3",
            Sheet {
                paper: Paper::A0,
                landscape: true,
            },
            Projection::Third,
            "A0 quer, Dritter Winkel",
        ),
    ] {
        let id = st.document.add_drawing(name, sheet, proj).unwrap();
        assert_eq!(
            drawing_browser::drawing_detail(st.document.drawing(id).unwrap()),
            expect
        );
    }
}

// (e) ------------------------------------------------------------------------------------------

#[test]
fn empty_state_text() {
    let mut st = box_state();
    let mut h = Harness::new();
    let shot = h.idle(&mut st);
    assert!(shot.find("Zeichnungen").is_some(), "{}", shot.text);
    assert!(
        shot.find("Noch keine Zeichnung. Über „Zeichnung“ im Menü lässt sich eine anlegen.")
            .is_some(),
        "{}",
        shot.text
    );
    two_drawings(&mut st);
    let shot = h.idle(&mut st);
    assert!(!shot.text.contains("Noch keine Zeichnung"), "{}", shot.text);
}

// (b) ------------------------------------------------------------------------------------------

#[test]
fn double_click_opens_the_drawing() {
    let mut st = box_state();
    let (d1, d2, [front, _, front2]) = two_drawings(&mut st);
    let mut h = Harness::new();
    // Ein Klick wählt nur.
    h.click(&mut st, "Blatt 2");
    assert_eq!(st.drawing_ui.selected_drawing, Some(d2));
    assert_eq!(st.drawing_ui.area, WorkArea::Model, "Klick öffnet nicht");
    // Doppelklick öffnet.
    h.double_click(&mut st, "Blatt 2");
    assert_eq!(st.drawing_ui.area, WorkArea::Drawing(d2));
    // Klick auf eine Ansicht öffnet die Zeichnung und wählt die Ansicht.
    h.click(&mut st, "Vorne 2");
    assert_eq!(st.drawing_ui.area, WorkArea::Drawing(d1));
    assert_eq!(st.drawing_ui.selected_view, Some(front2));
    h.click(&mut st, "Vorne");
    assert_eq!(st.drawing_ui.area, WorkArea::Drawing(d1));
    assert_eq!(st.drawing_ui.selected_view, Some(front));
    // Weder 3D-Auswahl noch Feature bleiben hervorgehoben.
    assert_eq!(st.selected_feature, None);
}

#[test]
fn opening_the_same_drawing_again_keeps_zoom_pan_and_selection() {
    let mut st = box_state();
    let (d1, d2, [front, _, _]) = two_drawings(&mut st);
    let mut h = Harness::new();
    st.drawing_ui.open(d1);
    st.drawing_ui.selected_view = Some(front);
    st.drawing_ui.zoom = 2.5;
    st.drawing_ui.pan = vec2(12.0, -7.0);
    st.drawing_ui.area = WorkArea::Model;
    h.double_click(&mut st, "Blatt 1");
    assert_eq!(st.drawing_ui.area, WorkArea::Drawing(d1));
    assert_eq!(st.drawing_ui.zoom, 2.5);
    assert_eq!(st.drawing_ui.pan, vec2(12.0, -7.0));
    assert_eq!(st.drawing_ui.selected_view, Some(front));
    // Eine andere Zeichnung beginnt frisch.
    h.double_click(&mut st, "Blatt 2");
    assert_eq!(st.drawing_ui.area, WorkArea::Drawing(d2));
    assert_eq!(st.drawing_ui.zoom, 1.0);
    assert_eq!(st.drawing_ui.pan, egui::Vec2::ZERO);
    assert_eq!(st.drawing_ui.selected_view, None);
}

#[test]
fn the_context_menu_opens_a_drawing() {
    let mut st = box_state();
    let (_, d2, _) = two_drawings(&mut st);
    let mut h = Harness::new();
    h.menu(&mut st, "Blatt 2", "Öffnen");
    assert_eq!(st.drawing_ui.area, WorkArea::Drawing(d2));
}

// (c) ------------------------------------------------------------------------------------------

#[test]
fn rename_and_delete_with_confirmation() {
    let mut st = box_state();
    let (d1, d2, _) = two_drawings(&mut st);
    let mut h = Harness::new();

    // Umbenennen: Fenster mit Textfeld, Fehler rot.
    h.menu(&mut st, "Blatt 2", "Umbenennen");
    assert_eq!(st.rename_drawing_draft.as_ref().map(|r| r.0), Some(d2));
    let shot = h.idle(&mut st);
    assert!(shot.find("Übernehmen").is_some(), "{}", shot.text);
    st.rename_drawing_draft.as_mut().unwrap().1 = "  ".into();
    let shot = h.idle(&mut st);
    assert!(
        shot.find("Bitte einen Namen eingeben.").is_some(),
        "{}",
        shot.text
    );
    st.rename_drawing_draft.as_mut().unwrap().1 = "BLATT 1".into();
    let shot = h.idle(&mut st);
    assert!(
        shot.find("Eine Zeichnung mit diesem Namen gibt es schon.")
            .is_some(),
        "{}",
        shot.text
    );
    assert!(drawing_browser::apply_rename(&mut st).is_err());
    assert_eq!(st.document.drawing(d2).unwrap().name, "Blatt 2");
    assert!(st.rename_drawing_draft.is_some(), "Fenster bleibt offen");
    // Ein gültiger Name, auch nur in anderer Schreibweise des eigenen Namens.
    st.rename_drawing_draft.as_mut().unwrap().1 = "Gehäuse".into();
    h.idle(&mut st);
    h.click(&mut st, "Übernehmen");
    assert_eq!(st.document.drawing(d2).unwrap().name, "Gehäuse");
    assert!(st.rename_drawing_draft.is_none());
    let shot = h.idle(&mut st);
    assert!(shot.find("Gehäuse").is_some() && shot.find("Blatt 2").is_none());
    // Abbrechen lässt den Namen.
    h.menu(&mut st, "Gehäuse", "Umbenennen");
    st.rename_drawing_draft.as_mut().unwrap().1 = "Anderes".into();
    h.idle(&mut st);
    h.click(&mut st, "Abbrechen");
    assert!(st.rename_drawing_draft.is_none());
    assert_eq!(st.document.drawing(d2).unwrap().name, "Gehäuse");

    // Löschen: erst die Rückfrage.
    st.drawing_ui.open(d2);
    h.menu(&mut st, "Gehäuse", "Löschen");
    assert!(st.document.drawing(d2).is_some(), "erst fragen");
    assert!(st.pending_delete_drawing.is_some());
    let shot = h.idle(&mut st);
    assert!(
        shot.find("Zeichnung „Gehäuse“ mit 1 Ansicht löschen?")
            .is_some(),
        "{}",
        shot.text
    );
    // Abbrechen lässt alles.
    h.click(&mut st, "Abbrechen");
    assert!(st.pending_delete_drawing.is_none());
    assert!(st.document.drawing(d2).is_some());
    assert_eq!(st.drawing_ui.area, WorkArea::Drawing(d2));
    // Löschen entfernt die offene Zeichnung; der Bereich fällt auf das Modell zurück.
    h.menu(&mut st, "Gehäuse", "Löschen");
    h.idle(&mut st);
    h.click(&mut st, "Löschen");
    assert!(st.document.drawing(d2).is_none());
    assert!(st.pending_delete_drawing.is_none());
    assert_eq!(st.drawing_ui.area, WorkArea::Model);
    assert!(st.document.drawing(d1).is_some(), "die andere bleibt");
    // Eine nicht offene Zeichnung zu löschen ändert den Bereich nicht.
    st.drawing_ui.open(d1);
    st.drawing_ui.area = WorkArea::Model;
    let d3 = st
        .document
        .add_drawing("Dritte", A4_HOCH, Projection::First)
        .unwrap();
    h.menu(&mut st, "Dritte", "Löschen");
    // Eine Zeichnung ohne Ansichten: kürzere Frage.
    let shot = h.idle(&mut st);
    assert!(
        shot.find("Zeichnung „Dritte“ löschen?").is_some(),
        "{}",
        shot.text
    );
    h.click(&mut st, "Löschen");
    assert!(st.document.drawing(d3).is_none());
}

#[test]
fn the_question_counts_the_views() {
    let mut st = box_state();
    let (d1, _, _) = two_drawings(&mut st);
    drawing_browser::request_delete_drawing(&mut st, d1);
    let plan: &DrawingDeletePlan = st.pending_delete_drawing.as_ref().unwrap();
    assert_eq!(plan.text, "Zeichnung „Blatt 1“ mit 3 Ansichten löschen?");
}

// (d) ------------------------------------------------------------------------------------------

#[test]
fn delete_view_names_dependents_in_the_prompt() {
    let mut st = box_state();
    let (d1, _, [front, top, front2]) = two_drawings(&mut st);
    // Ein weiteres Kind von Vorne: Links.
    let left = st
        .document
        .add_view(
            d1,
            ViewKind::Left,
            s(1, 1),
            Placement::Aligned { distance: 100.0 },
            Some(front),
        )
        .unwrap();
    let mut h = Harness::new();
    // Eine Ansicht mit abgeleiteten Ansichten: erst die Rückfrage mit den Namen.
    h.menu(&mut st, "Vorne", "Löschen");
    assert!(
        st.document.drawing(d1).unwrap().view(front).is_some(),
        "erst fragen"
    );
    let shot = h.idle(&mut st);
    assert!(
        shot.find(
            "Die Ansicht „Vorne“ hat abgeleitete Ansichten: Oben, Links. Sie werden mit gelöscht."
        )
        .is_some(),
        "{}",
        shot.text
    );
    h.click(&mut st, "Abbrechen");
    assert!(st.document.drawing(d1).unwrap().view(front).is_some());
    h.menu(&mut st, "Vorne", "Löschen");
    h.idle(&mut st);
    h.click(&mut st, "Löschen");
    let d = st.document.drawing(d1).unwrap();
    assert!(d.view(front).is_none() && d.view(top).is_none() && d.view(left).is_none());
    assert!(d.view(front2).is_some(), "andere Ansichten bleiben");
    // Ohne abgeleitete Ansichten wird sofort gelöscht.
    h.menu(&mut st, "Vorne", "Löschen");
    assert!(st.pending_delete_drawing.is_none());
    assert!(st.document.drawing(d1).unwrap().views.is_empty());
}

#[test]
fn a_view_can_hide_or_show_its_hidden_edges_from_the_menu() {
    let mut st = box_state();
    let (d1, _, [front, top, _]) = two_drawings(&mut st);
    let mut h = Harness::new();
    // Normansichten zeigen verdeckte Kanten von Anfang an (Werkstattzeichnung).
    {
        let d = st.document.drawing(d1).unwrap();
        assert!(d.view(top).unwrap().show_hidden);
        assert!(d.view(front).unwrap().show_hidden);
    }
    let shot = h.open_menu(&mut st, "Oben");
    assert!(
        shot.find("Verdeckte Kanten ausblenden").is_some(),
        "{}",
        shot.text
    );
    assert!(shot.find("Löschen").is_some());
    assert!(
        shot.find("Umbenennen").is_none(),
        "Ansichten haben keinen eigenen Namen"
    );
    let p = shot.pos_of("Verdeckte Kanten ausblenden");
    h.click_at(&mut st, p, PointerButton::Primary);
    h.idle(&mut st);
    let d = st.document.drawing(d1).unwrap();
    assert!(!d.view(top).unwrap().show_hidden);
    assert!(d.view(front).unwrap().show_hidden);
    let shot = h.open_menu(&mut st, "Oben");
    assert!(
        shot.find("Verdeckte Kanten zeigen").is_some(),
        "{}",
        shot.text
    );
    let p = shot.pos_of("Verdeckte Kanten zeigen");
    h.click_at(&mut st, p, PointerButton::Primary);
    assert!(
        st.document
            .drawing(d1)
            .unwrap()
            .view(top)
            .unwrap()
            .show_hidden
    );
}

#[test]
fn deleting_the_open_view_clears_the_selection_and_stale_questions_vanish() {
    let mut st = box_state();
    let (d1, _, [front, ..]) = two_drawings(&mut st);
    st.drawing_ui.open(d1);
    st.drawing_ui.selected_view = Some(front);
    drawing_browser::request_delete_view(&mut st, d1, front);
    assert!(st.pending_delete_drawing.is_some());
    // Die Zeichnung verschwindet, bevor bestätigt wird: nichts passiert, die Frage geht weg.
    let plan = st.pending_delete_drawing.clone().unwrap();
    st.document.delete_drawing(d1).unwrap();
    assert!(drawing_browser::apply_delete(&mut st, &plan).is_err());
    assert!(st.pending_delete_drawing.is_none());
    assert_eq!(st.drawing_ui.selected_view, None);
}

#[test]
fn deleting_a_view_of_another_drawing_keeps_the_selection_of_the_open_one() {
    // Ansichtskennungen zählen je Zeichnung: „Vorne“ in Blatt 1 und „Rechts“ in Blatt 2 haben
    // dieselbe Kennung. Löschen in Blatt 2 darf die Auswahl im offenen Blatt 1 nicht leeren.
    let mut st = box_state();
    let (d1, d2, [front, ..]) = two_drawings(&mut st);
    let other = st.document.drawing(d2).unwrap().views[0].id;
    assert_eq!(other, front, "gleiche Kennung in beiden Zeichnungen");
    st.drawing_ui.open(d1);
    st.drawing_ui.selected_view = Some(front);
    drawing_browser::request_delete_view(&mut st, d2, other);
    assert!(st.document.drawing(d2).unwrap().views.is_empty());
    assert_eq!(st.drawing_ui.selected_view, Some(front));
    // Im offenen Blatt geleert wird sie weiterhin.
    drawing_browser::request_delete_view(&mut st, d1, ViewId(3));
    assert_eq!(st.drawing_ui.selected_view, Some(front));
    drawing_browser::request_delete_view(&mut st, d1, front);
    let plan = st.pending_delete_drawing.clone().unwrap();
    drawing_browser::apply_delete(&mut st, &plan).unwrap();
    assert_eq!(st.drawing_ui.selected_view, None);
}

// (f) ------------------------------------------------------------------------------------------

#[test]
fn rows_use_full_width_and_equal_heights() {
    let mut st = box_state();
    two_drawings(&mut st);
    let mut h = Harness::new();
    h.idle(&mut st);
    let rows = h.rows();
    let names: Vec<&str> = rows.iter().map(|(n, _)| n.as_str()).collect();
    for must in [
        "Quader 1", "Blatt 1", "Vorne", "Oben", "Vorne 2", "Blatt 2", "Rechts",
    ] {
        assert!(names.contains(&must), "{must} fehlt in {names:?}");
    }
    let width = rows[0].1.width();
    assert!(width > W - 40.0, "volle Breite: {width}");
    for (n, r) in &rows {
        assert_eq!(
            r.height(),
            28.0,
            "Zeile {n}: gleiche Höhe wie die Feature-Zeilen"
        );
        assert_eq!(r.width(), width, "Zeile {n}: gleiche Breite");
        assert_eq!(r.left(), rows[0].1.left(), "Zeile {n} beginnt links");
    }
}

// (g) ------------------------------------------------------------------------------------------

fn workspace() -> (Workspace, std::path::PathBuf) {
    let root = std::env::temp_dir().join(format!("planora-db-{}", new_id()));
    let w = Workspace::new(Some(Library::open(&root).unwrap()), Config::default());
    (w, root)
}

#[test]
fn drawings_survive_save_and_load() {
    let (mut w, root) = workspace();
    let p = w.library.as_ref().unwrap().create_project("P").unwrap();
    {
        let st = &mut w.active_tab_mut().state;
        st.create_box(DVec3::new(100.0, 60.0, 40.0)).unwrap();
        two_drawings(st);
        let d = st.document.drawings()[0].id;
        let v = st.document.drawings()[0].views[1].id;
        st.document
            .update_drawing(d, |dr| {
                let view = dr.views.iter_mut().find(|x| x.id == v).unwrap();
                // vom Vorgabewert abweichend, damit das Speichern ihn wirklich tragen muss
                view.show_hidden = !view.show_hidden;
            })
            .unwrap();
    }
    let before: Vec<_> = w.active_tab().state.document.drawings().to_vec();
    assert_eq!(before.len(), 2);
    w.save_as(&p, "Teil", "Zeichnungen").unwrap();
    let path = w.active_tab().path.clone().unwrap();
    w.close_tab(0);

    let mut w2 = Workspace::new(Some(Library::open(&root).unwrap()), Config::default());
    let r = w2.open_document(&path, LockMode::Acquire).unwrap();
    let tab = &mut w2.tabs[r.tab];
    assert_eq!(tab.state.document.drawings(), before.as_slice());
    // Berechnung startet neu: nach dem Laden ist nichts berechnet, das Öffnen rechnet.
    assert_eq!(tab.state.drawing_ui.spawned(), 0);
    let d1 = before[0].id;
    tab.state.drawing_ui.open(d1);
    tab.state
        .drawing_ui
        .tick(&tab.state.bodies, &tab.state.document, true);
    assert!(
        tab.state.drawing_ui.spawned() >= 2,
        "Vorne und Oben rechnen"
    );
    let model = tab.state.document.model_edit_count();
    let start = Instant::now();
    loop {
        let drawing = tab.state.document.drawing(d1).unwrap();
        let ready = tab.state.drawing_ui.results_for(drawing, model);
        if ready.len() == drawing.views.len() {
            let lines: &ViewLines = ready.values().next().unwrap();
            assert!(!lines.strokes.is_empty());
            break;
        }
        assert!(start.elapsed() < Duration::from_secs(20), "Rechnung hängt");
        tab.state
            .drawing_ui
            .tick(&tab.state.bodies, &tab.state.document, true);
        std::thread::sleep(Duration::from_millis(5));
    }
    // Das frisch geladene Dokument ist nicht „geändert“.
    assert!(!tab.is_dirty());
}

// (h) ------------------------------------------------------------------------------------------

#[test]
fn unsaved_marker_after_drawing_edit() {
    let (mut w, _root) = workspace();
    let p = w.library.as_ref().unwrap().create_project("P").unwrap();
    w.active_tab_mut()
        .state
        .create_box(DVec3::splat(10.0))
        .unwrap();
    w.save_as(&p, "Teil", "").unwrap();
    let ctx = {
        let c = egui::Context::default();
        c.set_fonts(fonts::definitions());
        c
    };
    let tabs_text = |w: &mut Workspace| {
        let out = ctx.run_ui(egui::RawInput::default(), |ui| tabs::show(ui, w));
        let t = collect_texts(&out);
        out.drop_without_applying_deltas();
        t
    };
    assert!(!tabs_text(&mut w).contains('●'));
    w.active_tab_mut()
        .state
        .document
        .add_drawing("Blatt 1", A3_QUER, Projection::First)
        .unwrap();
    assert!(w.active_tab().is_dirty());
    let t = tabs_text(&mut w);
    assert!(t.contains("Teil ●"), "{t}");
    w.save_active("Mit Zeichnung", None).unwrap();
    assert!(!w.active_tab().is_dirty());
    let t = tabs_text(&mut w);
    assert!(!t.contains('●'), "{t}");
    // Auch eine Umbenennung und das Löschen einer Zeichnung gelten als Änderung.
    let id = w.active_tab().state.document.drawings()[0].id;
    drawing_browser::request_delete_drawing(&mut w.active_tab_mut().state, id);
    let plan = w.active_tab().state.pending_delete_drawing.clone().unwrap();
    drawing_browser::apply_delete(&mut w.active_tab_mut().state, &plan).unwrap();
    assert!(w.active_tab().is_dirty());
}

// Alte Versionen (schreibgeschützt) --------------------------------------------------------------

#[test]
fn a_read_only_tab_can_open_and_select_but_not_change() {
    let mut st = box_state();
    let (d1, d2, [front, ..]) = two_drawings(&mut st);
    let before = st.document.drawings().to_vec();
    let mut h = Harness::new();
    h.read_only = true;
    // Öffnen und Wählen gehen.
    h.double_click(&mut st, "Blatt 2");
    assert_eq!(st.drawing_ui.area, WorkArea::Drawing(d2));
    h.click(&mut st, "Vorne");
    assert_eq!(st.drawing_ui.area, WorkArea::Drawing(d1));
    assert_eq!(st.drawing_ui.selected_view, Some(front));
    // Das Menü zeigt „Umbenennen“ und „Löschen“ gesperrt.
    h.open_menu(&mut st, "Blatt 1");
    let shot = h.idle(&mut st);
    for entry in ["Öffnen", "Umbenennen", "Löschen"] {
        assert!(shot.find(entry).is_some(), "{entry}: {}", shot.text);
    }
    let p = shot.pos_of("Umbenennen");
    h.click_at(&mut st, p, PointerButton::Primary);
    h.idle(&mut st);
    assert!(st.rename_drawing_draft.is_none(), "gesperrt");
    // Das Menü kann noch offen sein; der Löschen-Eintrag tut nichts.
    let shot = h.open_menu(&mut st, "Blatt 1");
    let p = shot.pos_of("Löschen");
    h.click_at(&mut st, p, PointerButton::Primary);
    h.idle(&mut st);
    assert!(st.pending_delete_drawing.is_none(), "gesperrt");
    // Ansicht: Löschen und Verdeckte Kanten gesperrt.
    // (Normansicht: verdeckte Kanten sind vorgegeben an, der Eintrag heißt „ausblenden“.)
    let shot = h.open_menu(&mut st, "Oben");
    let p = shot.pos_of("Verdeckte Kanten ausblenden");
    h.click_at(&mut st, p, PointerButton::Primary);
    h.idle(&mut st);
    assert_eq!(st.document.drawings(), before.as_slice());
}

#[test]
fn a_selected_drawing_row_is_unselected_when_a_feature_is_clicked() {
    let mut st = box_state();
    let (d1, ..) = two_drawings(&mut st);
    let mut h = Harness::new();
    h.click(&mut st, "Blatt 1");
    assert_eq!(st.drawing_ui.selected_drawing, Some(d1));
    h.click(&mut st, "Quader 1");
    assert_eq!(st.drawing_ui.selected_drawing, None);
    assert!(st.selected_feature.is_some());
}
