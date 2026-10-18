//! Stückliste in der App (Teilprojekt 3c-2, Aufgabe 6): Tabelle auf dem Zeichnungsblatt mit
//! Platzreserve, Stücklistenblätter, Abschnitt „Stückliste“ im Fenster „Blattangaben“ und die
//! Regeln im Browser – mit der ganzen App (`eframe::App::ui`) bzw. dem Browser und Rohereignissen
//! je Bild, ohne Fenster.

mod common;

use eframe::App as _;
use egui::{Color32, Event, Key, Modifiers, PointerButton, Pos2, Rect, Vec2, vec2};
use planora_app::app::PlanoraApp;
use planora_app::drawing_browser;
use planora_app::drawing_dialog::{NewDrawingDialog, SKETCH_BLOCKED};
use planora_app::drawing_state::{WorkArea, estimate_view_size, suggest_scale_in};
use planora_app::drawing_view::{self, NO_VIEWS_ON_PARTS_SHEET};
use planora_app::parts_section::{
    CREATE, NO_PARTS, OPEN, SECTION, SWITCH, TOO_MANY_ON_SHEET, TOO_MANY_ROWS, create_or_open,
};
use planora_app::sheet_dialog::{CUT_WARNING, SheetInfoDialog};
use planora_app::state::{AppState, StandardPlane};
use planora_app::workspace::Workspace;
use planora_app::{browser, fonts, theme, viewport};
use planora_drawing::parts::PARTS_HEAD;
use planora_drawing::sheet_frame::{PartsSpace, REV_VISIBLE, TitleField, sheet_layout_with};
use planora_drawing::sheet_info::{FieldValue, Stamp, resolve_texts};
use planora_drawing::{
    DrawingId, DrawingKind, Paper, Placement, Projection, RowOverride, Sheet, ViewKind,
};
use planora_feature::{FOLLOW_PAGE_DELETE, FeatureId, PARTS_SHEET_RENAME, SketchPlane};
use planora_math::{DVec2, DVec3};
use planora_project::config::Config;
use planora_project::library::Library;

const SMALL: Vec2 = vec2(800.0, 600.0);
const BIG: Vec2 = vec2(1400.0, 900.0);
/// Hoch genug, dass viel vom Fenster ohne Rollen zu sehen ist.
const TALL: Vec2 = vec2(1400.0, 1700.0);

const A4_HOCH: Sheet = Sheet {
    paper: Paper::A4,
    landscape: false,
};
const A4_QUER: Sheet = Sheet {
    paper: Paper::A4,
    landscape: true,
};
const A3_QUER: Sheet = Sheet {
    paper: Paper::A3,
    landscape: true,
};

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

#[derive(Debug, Clone)]
struct Seen {
    text: String,
    rect: Rect,
    clip: Rect,
}

#[derive(Default)]
struct Shot {
    texts: Vec<Seen>,
    /// Die Texte, die nicht schon im Bild ohne Fenster standen.
    dialog: Vec<Seen>,
    fills: Vec<(Rect, Color32)>,
}

impl Shot {
    fn of(out: &egui::FullOutput, base: &[(String, Rect)]) -> Shot {
        fn walk(shape: &egui::epaint::Shape, clip: Rect, s: &mut Shot) {
            match shape {
                egui::epaint::Shape::Vec(v) => v.iter().for_each(|x| walk(x, clip, s)),
                egui::epaint::Shape::Text(t) => s.texts.push(Seen {
                    text: t.galley.text().to_string(),
                    rect: Rect::from_min_size(t.pos, t.galley.size()),
                    clip,
                }),
                egui::epaint::Shape::Rect(r) => s.fills.push((r.rect, r.fill)),
                _ => {}
            }
        }
        let mut s = Shot::default();
        for c in &out.shapes {
            walk(&c.shape, c.clip_rect, &mut s);
        }
        s.dialog = s
            .texts
            .iter()
            .filter(|t| {
                !base
                    .iter()
                    .any(|(text, r)| *text == t.text && (r.min - t.rect.min).abs().max_elem() < 0.5)
            })
            .cloned()
            .collect();
        s
    }

    /// Texte des Fensters `text`, ganz in `r` und nicht abgeschnitten.
    fn dlg(&self, text: &str, r: Rect) -> Vec<&Seen> {
        self.dialog
            .iter()
            .filter(|t| {
                t.text == text
                    && r.expand(0.5).contains_rect(t.rect)
                    && t.clip.expand(0.5).contains_rect(t.rect)
            })
            .collect()
    }

    fn has(&self, text: &str) -> bool {
        self.texts.iter().any(|t| t.text == text)
    }

    /// Texte, deren Mitte in `r` liegt.
    fn texts_in(&self, r: Rect) -> Vec<String> {
        self.texts
            .iter()
            .filter(|t| r.contains(t.rect.center()))
            .map(|t| t.text.clone())
            .collect()
    }
}

fn library_ws() -> Workspace {
    let root = common::temp_root("planora-parts-sheet");
    Workspace::new(Some(Library::open(&root).unwrap()), Config::default())
}

fn stamp() -> Stamp {
    Stamp {
        date: "2026-10-18".into(),
        user: "dennis".into(),
    }
}

/// `n` Quader 40 × 30 × 20 nebeneinander („Quader 1“ …); die Merkmale in Reihenfolge.
fn boxes(n: usize) -> (AppState, Vec<FeatureId>) {
    let mut st = AppState::new();
    let ids = (0..n)
        .map(|_| st.create_box(DVec3::new(40.0, 30.0, 20.0)).unwrap())
        .collect();
    (st, ids)
}

/// Zeichnung über „Neue Zeichnung“ auf `sheet` (Grundansicht vorne, vorgeschlagener Maßstab).
fn new_drawing(st: &mut AppState, sheet: Sheet) -> DrawingId {
    let mut dialog = NewDrawingDialog::new(st);
    dialog.paper = sheet.paper;
    dialog.landscape = sheet.landscape;
    dialog.scale = dialog.suggestion(st);
    dialog.create(st).unwrap()
}

fn required_format(st: &AppState) -> u32 {
    planora_project::required_format(&st.document.snapshot())
}

struct H {
    c: egui::Context,
    app: PlanoraApp,
    size: Vec2,
    base: Vec<(String, Rect)>,
}

impl H {
    fn new(size: Vec2, dark: bool, mut st: AppState) -> H {
        let c = egui::Context::default();
        c.set_fonts(fonts::definitions());
        theme::apply(&c, dark);
        c.all_styles_mut(|s| s.interaction.tooltip_delay = 0.0);
        let mut app = PlanoraApp::for_tests(library_ws());
        st.dark = dark;
        st.drawing_ui.tick(&st.bodies, &st.document, true);
        app.ws_mut().active_tab_mut().state = st;
        H {
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
        let shot = Shot::of(&out, &self.base);
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
        let btn = |pressed| Event::PointerButton {
            pos: p,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        self.frame(vec![Event::PointerMoved(p)]);
        self.frame(vec![Event::PointerMoved(p), btn(true)]);
        self.frame(vec![btn(false)]);
        self.settle()
    }

    fn hover(&mut self, p: Pos2) -> Shot {
        self.frame(vec![Event::PointerMoved(p)]);
        self.frame(vec![Event::PointerMoved(p)]);
        self.frame(vec![Event::PointerMoved(p)])
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

    fn dialog_rect(&mut self) -> Option<Rect> {
        self.st().sheet_dialog.as_ref().and_then(|d| d.last_rect)
    }

    fn is_open(&mut self) -> bool {
        self.st().sheet_dialog.is_some()
    }

    fn dialog(&mut self) -> &mut SheetInfoDialog {
        self.st().sheet_dialog.as_mut().expect("Fenster offen")
    }

    fn capture_base(&mut self) {
        self.base.clear();
        let shot = self.settle();
        self.base = shot
            .texts
            .iter()
            .map(|t| (t.text.clone(), t.rect))
            .collect();
    }

    /// Öffnet das Fenster „Blattangaben“ für die offene Zeichnung.
    fn open_dialog(&mut self) {
        self.capture_base();
        let WorkArea::Drawing(id) = self.st().drawing_ui.area else {
            panic!("keine Zeichnung offen");
        };
        planora_app::sheet_dialog::open(self.st(), id);
        self.settle();
    }

    fn wheel(&mut self, dy: f32) {
        let at = self.dialog_rect().expect("Fenster offen").center();
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

    /// Rollt das Fenster von oben, bis `text` ganz zu sehen ist, und liefert seine Lage.
    fn scroll_to(&mut self, text: &str) -> Rect {
        self.try_scroll_to(text)
            .unwrap_or_else(|| panic!("„{text}“ nie zu sehen"))
    }

    fn try_scroll_to(&mut self, text: &str) -> Option<Rect> {
        self.settle();
        for _ in 0..3 {
            self.wheel(4000.0);
        }
        for _ in 0..80 {
            let shot = self.settle();
            let rect = self.dialog_rect().expect("Fenster offen");
            if let Some(s) = shot.dlg(text, rect).first() {
                return Some(s.rect);
            }
            self.wheel(-150.0);
        }
        None
    }

    fn click_text(&mut self, text: &str) {
        let r = self.scroll_to(text);
        self.click(r.center());
    }

    fn type_into_hinted(&mut self, hint: &str, text: &str) {
        let r = self.scroll_to(hint);
        self.click(r.center());
        self.typed(text);
    }

    /// Alle sichtbaren Texte des Fensters beim Rollen von oben nach unten.
    fn all_dialog_texts(&mut self) -> Vec<Seen> {
        self.settle();
        for _ in 0..3 {
            self.wheel(4000.0);
        }
        let mut all = Vec::new();
        let mut last: Vec<String> = Vec::new();
        for _ in 0..200 {
            let shot = self.settle();
            let rect = self.dialog_rect().expect("Fenster offen");
            let seen: Vec<Seen> = shot
                .dialog
                .into_iter()
                .filter(|t| rect.expand(0.5).contains_rect(t.rect))
                .filter(|t| t.clip.expand(0.5).contains_rect(t.rect))
                .collect();
            let now: Vec<String> = seen.iter().map(|t| t.text.clone()).collect();
            all.extend(seen);
            // Am Ende ändert das Rollen nichts mehr.
            if now == last {
                break;
            }
            last = now;
            self.wheel(-200.0);
        }
        all
    }

    /// Bildschirmrechteck eines Blattrechtecks (mm).
    fn screen_box(&self, b: (DVec2, DVec2)) -> Rect {
        let l = drawing_view::last_layout(&self.c).expect("kein Zeichnungsbereich");
        let a = drawing_view::sheet_to_screen(l.canvas, l.sheet_mm, l.zoom, l.pan, b.0);
        let z = drawing_view::sheet_to_screen(l.canvas, l.sheet_mm, l.zoom, l.pan, b.1);
        Rect::from_two_pos(a, z)
    }

    /// Bildschirmrechteck der Stückliste der offenen Zeichnung (wie gezeichnet).
    fn table_rect(&mut self) -> Option<Rect> {
        let WorkArea::Drawing(id) = self.st().drawing_ui.area else {
            return None;
        };
        let st = &self.app.ws_mut().active_tab_mut().state;
        let d = st.document.drawing(id)?;
        let parts = drawing_view::layout_of(st, d).parts?;
        Some(self.screen_box(parts))
    }

    /// Texte in der Tabellenfläche der offenen Zeichnung.
    fn table_texts(&mut self) -> Vec<String> {
        let shot = self.settle();
        match self.table_rect() {
            Some(r) => shot.texts_in(r.expand(0.5)),
            None => Vec::new(),
        }
    }

    fn edits(&mut self) -> u64 {
        self.st().document.edit_count()
    }
}

// (a) ------------------------------------------------------------------------------------------

#[test]
fn table_on_sheet_and_reserved_space() {
    for (size, dark) in [(SMALL, false), (BIG, true)] {
        let (mut st, _) = boxes(3);
        let id = new_drawing(&mut st, A3_QUER);
        let d = st.document.drawing(id).unwrap().clone();
        assert!(d.info.as_ref().unwrap().parts_list, "drei Körper: an");
        // Platzreserve: Anordnung mit 10 Zeilen, gezeichnet mit 3.
        let notes = {
            let ctx = drawing_view::sheet_context(&st, id).unwrap();
            resolve_texts(&d, st.document.project_info(), &ctx, None)
                .notes
                .len()
        };
        let placement = drawing_view::placement_layout(&st, &d);
        let reserved = sheet_layout_with(
            d.sheet,
            REV_VISIBLE,
            notes,
            PartsSpace::OnSheet { rows: 10 },
        );
        assert_eq!(placement.free, reserved.free, "{size:?}");
        assert_eq!(placement.parts, reserved.parts);
        let drawn = drawing_view::layout_of(&st, &d);
        assert_eq!(drawn.parts_space, PartsSpace::OnSheet { rows: 3 });
        let (lo, hi) = drawn.parts.unwrap();
        assert!(
            ((hi.y - lo.y) - drawn.mm(7.0) * 4.0).abs() < 1e-9,
            "Kopf + 3 Zeilen"
        );
        assert!(drawn.free.1.y - drawn.free.0.y > placement.free.1.y - placement.free.0.y - 1e-9);
        // Die Grundansicht liegt in der Mitte des freien Bereichs mit Platzreserve.
        let Placement::Free { x, y } = d.views[0].placement else {
            panic!("frei");
        };
        let (flo, fhi) = placement.free;
        assert!(
            (x - (flo.x + fhi.x) / 2.0).abs() < 1e-9 && (y - (flo.y + fhi.y) / 2.0).abs() < 1e-9
        );

        let mut h = H::new(size, dark, st);
        let texts = h.table_texts();
        for must in [
            "Pos.",
            "Benennung",
            "Menge",
            "Quader 1",
            "Quader 2",
            "Quader 3",
        ] {
            assert!(
                texts.iter().any(|t| t == must),
                "{size:?}: „{must}“ in der Tabelle: {texts:?}"
            );
        }

        // Ein Körper: Schalter aus, keine Tabelle; ein weiterer Körper ändert das nicht (F4).
        let (mut st, _) = boxes(1);
        let one = new_drawing(&mut st, A3_QUER);
        assert!(
            !st.document
                .drawing(one)
                .unwrap()
                .info
                .as_ref()
                .unwrap()
                .parts_list
        );
        let mut h = H::new(size, dark, st);
        let shot = h.settle();
        assert!(!shot.has("Benennung"), "{size:?}: keine Tabelle");
        assert!(h.table_rect().is_none());
        h.st().create_box(DVec3::new(40.0, 30.0, 20.0)).unwrap();
        let shot = h.settle();
        assert!(
            !h.st()
                .document
                .drawing(one)
                .unwrap()
                .info
                .as_ref()
                .unwrap()
                .parts_list
        );
        assert!(!shot.has("Benennung"), "{size:?}: bleibt aus");
    }
}

#[test]
fn new_drawing_reserves_the_table_for_the_scale_suggestion() {
    // Ein hohes Teil auf A4 hoch: mit Platz für die volle Stückliste ein kleinerer Maßstab.
    let size =
        |l: &planora_drawing::SheetLayout| (l.free.1.x - l.free.0.x, l.free.1.y - l.free.0.y);
    let mut found = false;
    for h in [60.0, 80.0, 100.0, 120.0, 140.0, 160.0, 180.0] {
        let mut st = AppState::new();
        st.create_box(DVec3::new(30.0, 20.0, h)).unwrap();
        st.create_box(DVec3::new(10.0, 10.0, 10.0)).unwrap();
        let mut dialog = NewDrawingDialog::new(&st);
        dialog.paper = Paper::A4;
        dialog.landscape = false;
        let with = drawing_view::new_sheet_layout(&st, A4_HOCH, Projection::First);
        assert_eq!(with.parts_space, PartsSpace::OnSheet { rows: 10 });
        // Gleiche Hinweiszeilen, aber ohne Stückliste.
        let notes = with.notes.map(|_| 3).unwrap_or(0);
        let without = sheet_layout_with(A4_HOCH, REV_VISIBLE, notes, PartsSpace::None);
        let s_with = suggest_scale_in(&st.bodies, size(&with), ViewKind::Front);
        let s_without = suggest_scale_in(&st.bodies, size(&without), ViewKind::Front);
        assert_eq!(dialog.suggestion(&st), s_with, "Höhe {h}");
        found |= s_with != s_without;
    }
    assert!(
        found,
        "Testvoraussetzung: bei einer Höhe entscheidet der Platz"
    );
    // Ein Körper: kein Platz für die Stückliste.
    let (st1, _) = boxes(1);
    assert_eq!(
        drawing_view::new_sheet_layout(&st1, A4_HOCH, Projection::First).parts_space,
        PartsSpace::None
    );
}

// (b) ------------------------------------------------------------------------------------------

#[test]
fn more_than_ten_rows() {
    let (mut st, _) = boxes(12);
    new_drawing(&mut st, A3_QUER);
    let mut h = H::new(BIG, false, st);
    let texts = h.table_texts();
    for n in 2..=10 {
        assert!(texts.contains(&n.to_string()), "Pos. {n}: {texts:?}");
        assert!(texts.contains(&format!("Quader {n}")), "{texts:?}");
    }
    for n in [11, 12] {
        assert!(
            !texts.contains(&n.to_string()),
            "Pos. {n} nicht auf dem Blatt"
        );
        assert!(!texts.contains(&format!("Quader {n}")));
    }
    h.open_dialog();
    h.scroll_to(TOO_MANY_ON_SHEET);
    // Ausgeschaltet: kein Hinweis.
    h.dialog().sheet.parts_list = false;
    assert!(h.try_scroll_to(TOO_MANY_ON_SHEET).is_none());
}

// (c) ------------------------------------------------------------------------------------------

type Span = ((f64, f64), (f64, f64));

fn view_spans(st: &AppState, id: DrawingId) -> Vec<(ViewKind, Span)> {
    let d = st.document.drawing(id).unwrap();
    let mut centres: Vec<(planora_drawing::ViewId, (f64, f64))> = Vec::new();
    let mut out = Vec::new();
    for v in &d.views {
        let c = match v.placement {
            Placement::Free { x, y } => (x, y),
            Placement::Aligned { distance } => {
                let p = v.parent.unwrap();
                let pc = centres.iter().find(|(i, _)| *i == p).unwrap().1;
                let pk = d.view(p).unwrap().kind;
                let (dx, dy) = planora_drawing::align_dir(d.projection, pk, v.kind).unwrap();
                (pc.0 + dx * distance, pc.1 + dy * distance)
            }
        };
        centres.push((v.id, c));
        let (w, hgt) = estimate_view_size(&st.bodies, v.kind);
        let f = v.scale.factor();
        out.push((
            v.kind,
            (
                (c.0 - w * f / 2.0, c.1 - hgt * f / 2.0),
                (c.0 + w * f / 2.0, c.1 + hgt * f / 2.0),
            ),
        ));
    }
    out
}

#[test]
fn views_never_under_the_table() {
    // Der Block aus 3c-1 ist Vorne, Seite (im Erstwinkel „Links“ rechts daneben) und Oben mit
    // der Isometrie im leeren Viertel; „Rechts“ liegt im Erstwinkel auf der anderen Seite der
    // Grundansicht (links, fern der Tabelle).
    let sets = [
        [ViewKind::Top, ViewKind::Left, ViewKind::Isometric],
        [ViewKind::Top, ViewKind::Right, ViewKind::Isometric],
    ];
    for sheet in [A4_HOCH, A4_QUER, A3_QUER] {
        for (size, kinds) in [
            (DVec3::new(40.0, 30.0, 20.0), sets[0]),
            (DVec3::new(90.0, 50.0, 40.0), sets[0]),
            (DVec3::new(40.0, 30.0, 20.0), sets[1]),
        ] {
            let mut st = AppState::new();
            for _ in 0..3 {
                st.create_box(size).unwrap();
            }
            let id = new_drawing(&mut st, sheet);
            for kind in kinds {
                drawing_view::add_view(&mut st, kind).unwrap();
            }
            let d = st.document.drawing(id).unwrap().clone();
            assert!(d.info.as_ref().unwrap().parts_list);
            let layout = drawing_view::placement_layout(&st, &d);
            let (tlo, thi) = layout.parts.unwrap();
            // Die volle Tabelle: Kopf und 10 Zeilen.
            assert!(((thi.y - tlo.y) - layout.mm(7.0) * 11.0).abs() < 1e-9);
            let (flo, fhi) = layout.free;
            for (kind, ((x0, y0), (x1, y1))) in view_spans(&st, id) {
                let ctx = format!("{sheet:?} {size:?} {kind:?}");
                let e = 1e-6;
                let overlap = x0 < thi.x && tlo.x < x1 && y0 < thi.y && tlo.y < y1;
                assert!(!overlap, "{ctx}: überdeckt die Tabelle {tlo:?}…{thi:?}");
                if kind == ViewKind::Right {
                    continue;
                }
                // 3c-1: Passt die Isometrie selbst eine Stufe kleiner nicht in den freien
                // Bereich, sitzt sie bündig in seiner Ecke und ragt hinaus (nach bestem Wissen);
                // die Tabelle überdeckt sie trotzdem nicht (oben geprüft).
                if kind == ViewKind::Isometric
                    && (x1 - x0 > fhi.x - flo.x + e || y1 - y0 > fhi.y - flo.y + e)
                {
                    assert!(
                        (y0 - flo.y).abs() < e || (y1 - fhi.y).abs() < e,
                        "{ctx}: bündig"
                    );
                    continue;
                }
                assert!(
                    x0 >= flo.x - e && y0 >= flo.y - e && x1 <= fhi.x + e && y1 <= fhi.y + e,
                    "{ctx}: ({x0},{y0})…({x1},{y1}) im freien Bereich {flo:?}…{fhi:?}"
                );
            }
        }
    }
}

// (d) ------------------------------------------------------------------------------------------

/// Felder der Höhe 26 px, die den Punkt enthalten (Textfelder).
fn text_fields_at(shot: &Shot, p: Pos2) -> usize {
    shot.fills
        .iter()
        .filter(|(r, _)| r.contains(p) && (r.height() - 26.0).abs() < 1.0 && r.width() > 10.0)
        .count()
}

#[test]
fn section_edit_apply_reset() {
    let (mut st, features) = boxes(3);
    new_drawing(&mut st, A3_QUER);
    let mut h = H::new(TALL, false, st);
    let before = h.edits();
    let format = required_format(h.st());
    h.open_dialog();
    assert_eq!(h.edits(), before, "Öffnen ändert nichts");
    // Abschnitt, Schalter, Pos. als Text (ohne Feld).
    h.scroll_to(SECTION);
    h.scroll_to(SWITCH);
    let pos = h.scroll_to("Pos. 1");
    let shot = h.settle();
    let pos_now = shot
        .dlg("Pos. 1", h.dialog_rect().unwrap())
        .first()
        .map(|s| s.rect)
        .unwrap_or(pos);
    assert_eq!(text_fields_at(&shot, pos_now.center()), 0, "Pos. ohne Feld");
    // Benennung in Zeile 1.
    h.type_into_hinted("Quader 1", "Grundplatte");
    let rect = h.dialog_rect().unwrap();
    assert!(!h.settle().dlg("11/60", rect).is_empty(), "Länge 11/60");
    assert_eq!(h.edits(), before, "Tippen ändert das Dokument nicht");
    let key1 = h.st().document.parts_rows()[0].keys[0];
    assert_eq!(
        h.dialog()
            .parts
            .overrides
            .get(&key1)
            .map(|o| o.name.clone()),
        Some(FieldValue::Text("Grundplatte".into()))
    );
    h.click_text("Übernehmen");
    assert!(!h.is_open());
    assert_eq!(h.edits(), before + 1, "genau eine Änderung");
    assert_eq!(
        h.st().document.parts_overrides().get(&key1).unwrap().name,
        FieldValue::Text("Grundplatte".into())
    );
    assert!(h.table_texts().contains(&"Grundplatte".to_string()));
    assert!(!h.table_texts().contains(&"Quader 1".to_string()));
    assert_eq!(required_format(h.st()), format.max(7));

    // Umbenennen des Features: der eigene Text bleibt.
    h.st().rename_feature(features[0], "Platte").unwrap();
    assert!(h.table_texts().contains(&"Grundplatte".to_string()));

    // Zurücksetzen und Übernehmen: wieder der Körpername.
    h.open_dialog();
    h.click_text("Zurücksetzen");
    assert!(h.dialog().parts.overrides.is_empty());
    h.click_text("Übernehmen");
    assert!(h.st().document.parts_overrides().is_empty());
    let texts = h.table_texts();
    assert!(texts.contains(&"Platte".to_string()) && !texts.contains(&"Grundplatte".to_string()));

    // Abbrechen ändert nichts.
    let before = h.edits();
    h.open_dialog();
    h.type_into_hinted("Quader 2", "Deckel");
    h.click_text("Abbrechen");
    assert!(!h.is_open());
    assert_eq!(h.edits(), before);
    assert!(h.st().document.parts_overrides().is_empty());
}

#[test]
fn edits_go_to_the_key_of_their_row() {
    let (mut st, _) = boxes(3);
    new_drawing(&mut st, A3_QUER);
    let mut h = H::new(TALL, false, st);
    h.open_dialog();
    // Zeile 2, Werkstoff (automatisch „–“ steht in allen Zeilen): über die Benennung der Zeile
    // die Zeile finden.
    h.type_into_hinted("Quader 2", "Deckel");
    h.click_text("Übernehmen");
    let rows = h.st().document.parts_rows();
    let o = h.st().document.parts_overrides().clone();
    assert_eq!(o.len(), 1);
    assert_eq!(
        o.get(&rows[1].keys[0]).unwrap().name,
        FieldValue::Text("Deckel".into())
    );
    let cells = h.st().document.parts_cells();
    assert_eq!(cells[1][1], "Deckel");
    assert_eq!(cells[0][1], "Quader 1");
    assert_eq!(cells[2][1], "Quader 3");
    // Auf dem Blatt in Zeile 2 (gleiche Höhe wie die Pos. „2“).
    let shot = h.settle();
    let table = h.table_rect().unwrap();
    let find = |t: &str| {
        shot.texts
            .iter()
            .find(|s| s.text == t && table.contains(s.rect.center()))
            .unwrap_or_else(|| panic!("„{t}“"))
            .rect
    };
    assert!((find("Deckel").center().y - find("2").center().y).abs() < 2.0);
}

#[test]
fn limits_and_bad_texts() {
    let (mut st, _) = boxes(2);
    new_drawing(&mut st, A3_QUER);
    let mut h = H::new(TALL, false, st);
    h.open_dialog();
    h.type_into_hinted("Quader 1", &"W".repeat(61));
    let key = h.st().document.parts_rows()[0].keys[0];
    assert_eq!(
        h.dialog().parts.overrides[&key].name,
        FieldValue::Text("W".repeat(60)),
        "Benennung höchstens 60 Zeichen"
    );
    let rect = h.dialog_rect().unwrap();
    assert!(!h.settle().dlg("60/60", rect).is_empty());
    // 60 Zeichen passen nicht in die Spalte der Tabelle auf dem Blatt.
    assert_eq!(h.settle().dlg(CUT_WARNING, rect).len(), 1, "gekürzt");
    assert!(!h.settle().dlg("0/80", rect).is_empty(), "Bemerkung „0/80“");
    // Zu langer Text aus der Kopie (nicht über das Feld): „Übernehmen“ schreibt nichts.
    let before = h.edits();
    h.dialog().sheet.title = FieldValue::Text("Neu".into());
    h.dialog().parts.overrides.insert(
        key,
        RowOverride {
            note: FieldValue::Text("y".repeat(81)),
            ..RowOverride::default()
        },
    );
    let d = h.dialog().clone();
    let r = d.apply(h.st());
    assert!(r.is_err(), "{r:?}");
    assert_eq!(h.edits(), before, "nichts halb geschrieben");
}

// (e) ------------------------------------------------------------------------------------------

#[test]
fn switch_toggle() {
    let (mut st, _) = boxes(3);
    let id = new_drawing(&mut st, A3_QUER);
    let mut h = H::new(TALL, false, st);
    assert_eq!(required_format(h.st()), 7);
    let area = |h: &mut H| {
        let st = &h.app.ws_mut().active_tab_mut().state;
        let (lo, hi) = drawing_view::layout_of(st, st.document.drawing(id).unwrap()).free;
        (hi.x - lo.x) * (hi.y - lo.y)
    };
    let on = area(&mut h);
    h.open_dialog();
    h.click_text(SWITCH);
    assert!(!h.dialog().sheet.parts_list);
    assert!(
        h.st()
            .document
            .drawing(id)
            .unwrap()
            .info
            .as_ref()
            .unwrap()
            .parts_list,
        "erst mit Übernehmen"
    );
    h.click_text("Übernehmen");
    assert!(
        !h.st()
            .document
            .drawing(id)
            .unwrap()
            .info
            .as_ref()
            .unwrap()
            .parts_list
    );
    h.base.clear();
    let shot = h.settle();
    assert!(!shot.has("Benennung"), "Tabelle weg");
    assert!(area(&mut h) > on, "freier Bereich größer");
    assert_eq!(required_format(h.st()), 6);
    h.open_dialog();
    h.click_text(SWITCH);
    h.click_text("Übernehmen");
    assert!(h.table_texts().contains(&"Benennung".to_string()));
    assert_eq!(required_format(h.st()), 7);
    assert!((area(&mut h) - on).abs() < 1e-9);
}

// (f) ------------------------------------------------------------------------------------------

#[test]
fn create_and_open_parts_sheet() {
    let (mut st, _) = boxes(3);
    let first = new_drawing(&mut st, A3_QUER);
    let mut h = H::new(TALL, false, st);
    h.open_dialog();
    assert!(h.try_scroll_to(OPEN).is_none());
    let before = h.edits();
    h.click_text(CREATE);
    assert_eq!(h.edits(), before + 1, "Anlegen ist ein Schritt");
    let sheets = h.st().document.parts_sheets();
    assert_eq!(sheets.len(), 1);
    let p1 = sheets[0];
    assert_eq!(
        h.st().drawing_ui.area,
        WorkArea::Drawing(p1),
        "Blatt ist offen"
    );
    assert!(h.is_open(), "das Fenster bleibt");
    assert_eq!(h.dialog().drawing, first, "für sein Blatt");
    let d = h.st().document.drawing(p1).unwrap().clone();
    assert_eq!(d.sheet, A4_HOCH, "Vorgabe A4 hoch");
    let texts = drawing_view::sheet_texts(h.st(), &d);
    assert_eq!(texts.get(TitleField::DocType), "Stückliste");
    assert_eq!(texts.get(TitleField::Title), "Stückliste");
    assert_eq!(texts.get(TitleField::SheetOf), "2 von 2");
    // Gemalt: Kopf, Zeilen und „2 von 2“.
    h.st().sheet_dialog = None;
    h.base.clear();
    let shot = h.settle();
    assert!(shot.has("2 von 2"));
    let table = h.table_texts();
    for t in PARTS_HEAD.iter().copied().chain(["Quader 1", "Quader 3"]) {
        assert!(table.iter().any(|x| x == t), "„{t}“: {table:?}");
    }
    // Jetzt heißt der Knopf „öffnen“ und öffnet Seite 1.
    h.st().drawing_ui.open(first);
    h.open_dialog();
    assert!(h.try_scroll_to(CREATE).is_none());
    h.st().drawing_ui.area = WorkArea::Model;
    let before = h.edits();
    h.click_text(OPEN);
    assert_eq!(h.st().drawing_ui.area, WorkArea::Drawing(p1));
    assert_eq!(h.edits(), before, "Öffnen ändert nichts");
    assert_eq!(h.st().document.parts_sheets().len(), 1);

    // Papier A3 quer gewählt: so angelegt.
    let (mut st, _) = boxes(2);
    new_drawing(&mut st, A3_QUER);
    let mut h = H::new(TALL, true, st);
    h.open_dialog();
    h.dialog().parts.paper = Paper::A3;
    h.click_text("quer");
    assert!(h.dialog().parts.landscape);
    h.click_text(CREATE);
    let p = h.st().document.parts_sheets()[0];
    assert_eq!(h.st().document.drawing(p).unwrap().sheet, A3_QUER);
}

#[test]
fn many_parts_fill_follow_pages_and_page_numbers_move() {
    // 45 Körper auf A4 hoch (44 Zeilen je Blatt): zwei Blätter, Blatt 2 zeigt Pos. 45.
    let (mut st, features) = boxes(45);
    let first = st
        .document
        .add_drawing_stamped("Blatt 1", A3_QUER, Projection::First, &stamp())
        .unwrap();
    st.drawing_ui.open(first);
    let p1 = create_or_open(&mut st, A4_HOCH).unwrap();
    let sheets = st.document.parts_sheets();
    assert_eq!(sheets.len(), 2);
    let p2 = sheets[1];
    assert_eq!(st.document.drawing(p2).unwrap().sheet, A4_HOCH);
    let after = st
        .document
        .add_drawing_stamped("Blatt X", A3_QUER, Projection::First, &stamp())
        .unwrap();
    let sheet_of = |st: &AppState, id| {
        let d = st.document.drawing(id).unwrap();
        drawing_view::sheet_texts(st, d)
            .get(TitleField::SheetOf)
            .to_string()
    };
    assert_eq!(sheet_of(&st, first), "1 von 4");
    assert_eq!(sheet_of(&st, p1), "2 von 4");
    assert_eq!(sheet_of(&st, p2), "3 von 4");
    assert_eq!(sheet_of(&st, after), "4 von 4");
    let mut h = H::new(BIG, false, st);
    h.st().drawing_ui.open(p2);
    let table = h.table_texts();
    assert!(table.contains(&"45".to_string()), "{table:?}");
    assert!(table.contains(&"Quader 45".to_string()));
    assert!(
        !table.contains(&"Quader 44".to_string()),
        "Zeile 44 steht auf Blatt 1"
    );
    let shot = h.settle();
    assert!(shot.has("3 von 4"));

    // Einen Körper löschen: Blatt 2 verschwindet; der offene Bereich fällt aufs Modell zurück.
    h.st().delete_feature(features[44]).unwrap();
    h.settle();
    assert_eq!(h.st().drawing_ui.area, WorkArea::Model);
    assert_eq!(h.st().document.parts_sheets(), vec![p1]);
    assert_eq!(sheet_of(h.st(), after), "3 von 3");
    // „Blatt X“ offen; ein neuer Körper schiebt das Folgeblatt davor: „4 von 4“, auch gemalt.
    h.st().drawing_ui.open(after);
    let shot = h.settle();
    assert!(shot.has("3 von 3"));
    h.st().create_box(DVec3::new(40.0, 30.0, 20.0)).unwrap();
    let shot = h.settle();
    assert_eq!(h.st().document.parts_sheets().len(), 2);
    assert_eq!(sheet_of(h.st(), after), "4 von 4");
    assert!(
        shot.has("4 von 4") && !shot.has("3 von 3"),
        "das Blatt folgt"
    );
}

#[test]
fn the_switch_of_a_parts_sheet_is_ignored() {
    // Eine Datei kann den Schalter auf einem Stücklistenblatt gesetzt haben: ohne Wirkung.
    let (mut st, _) = boxes(3);
    let first = new_drawing(&mut st, A3_QUER);
    let p1 = create_or_open(&mut st, A4_HOCH).unwrap();
    let plain = {
        let d = st.document.drawing(p1).unwrap();
        drawing_view::layout_of(&st, d)
    };
    st.document
        .update_drawing(p1, |d| d.info.as_mut().unwrap().parts_list = true)
        .unwrap();
    // Laden lässt es zu.
    let snapshot = st.document.snapshot();
    let loaded = AppState::from_snapshot(snapshot).unwrap();
    let d = loaded.document.drawing(p1).unwrap().clone();
    assert!(d.info.as_ref().unwrap().parts_list);
    assert_eq!(
        drawing_view::parts_space(&loaded, &d, false),
        PartsSpace::ListPage
    );
    assert_eq!(
        drawing_view::parts_space(&loaded, &d, true),
        PartsSpace::ListPage
    );
    let l = drawing_view::layout_of(&loaded, &d);
    assert_eq!(
        (l.parts, l.free, l.notes),
        (plain.parts, plain.free, plain.notes)
    );
    // Das Fenster zeigt den Schalter auf dem Stücklistenblatt nicht, wohl aber auf Blatt 1.
    let mut h = H::new(TALL, false, loaded);
    h.st().drawing_ui.open(p1);
    h.open_dialog();
    h.scroll_to(SECTION);
    assert!(h.try_scroll_to(SWITCH).is_none(), "kein Schalter");
    assert!(
        h.try_scroll_to("Eigene Hinweise für dieses Blatt")
            .is_none(),
        "kein Hinweis-Editor"
    );
    let before = h.edits();
    h.click_text("Übernehmen");
    assert_eq!(h.edits(), before);
    h.st().drawing_ui.open(first);
    h.open_dialog();
    h.scroll_to(SWITCH);
    h.scroll_to("Eigene Hinweise für dieses Blatt");
}

// (g) ------------------------------------------------------------------------------------------

struct Browser {
    ctx: egui::Context,
    time: f64,
}

impl Browser {
    fn new() -> Browser {
        let ctx = egui::Context::default();
        ctx.set_fonts(fonts::definitions());
        ctx.all_styles_mut(|s| s.interaction.tooltip_delay = 0.0);
        Browser { ctx, time: 0.0 }
    }

    fn frame(&mut self, st: &mut AppState, events: Vec<Event>) -> Vec<Seen> {
        self.time += 0.05;
        let raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(420.0, 2000.0))),
            events,
            time: Some(self.time),
            ..Default::default()
        };
        let out = self.ctx.run_ui(raw, |ui| browser::show_with(ui, st, false));
        let shot = Shot::of(&out, &[]);
        out.drop_without_applying_deltas();
        shot.texts
    }

    fn idle(&mut self, st: &mut AppState) -> Vec<Seen> {
        self.frame(st, vec![]);
        self.frame(st, vec![])
    }

    fn pos(texts: &[Seen], t: &str) -> Pos2 {
        texts
            .iter()
            .find(|s| s.text == t)
            .unwrap_or_else(|| panic!("„{t}“ fehlt"))
            .rect
            .left_center()
            + vec2(6.0, 0.0)
    }

    fn click_at(&mut self, st: &mut AppState, p: Pos2, b: PointerButton) {
        let press = |pressed| Event::PointerButton {
            pos: p,
            button: b,
            pressed,
            modifiers: Modifiers::NONE,
        };
        self.frame(st, vec![Event::PointerMoved(p)]);
        self.frame(st, vec![press(true)]);
        self.frame(st, vec![press(false)]);
    }

    /// Kontextmenü der Zeile `row`; Hinweis am Eintrag `entry` (Texte beim Überfahren) und ein
    /// Klick darauf.
    fn menu_entry(&mut self, st: &mut AppState, row: &str, entry: &str) -> Vec<String> {
        let texts = self.idle(st);
        let p = Self::pos(&texts, row);
        self.click_at(st, p, PointerButton::Secondary);
        let texts = self.idle(st);
        let e = Self::pos(&texts, entry);
        self.frame(st, vec![Event::PointerMoved(e)]);
        let mut hover = Vec::new();
        for _ in 0..4 {
            hover.extend(self.frame(st, vec![]));
        }
        self.click_at(st, e, PointerButton::Primary);
        self.idle(st);
        // Menü schließen.
        self.frame(
            st,
            vec![Event::Key {
                key: Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::NONE,
            }],
        );
        self.idle(st);
        hover.into_iter().map(|s| s.text).collect()
    }
}

#[test]
fn browser_rules() {
    // 27 Körper auf A4 quer (26 Zeilen je Blatt): zwei Stücklistenblätter.
    let (mut st, _) = boxes(27);
    let first = st
        .document
        .add_drawing_stamped("Blatt 1", A3_QUER, Projection::First, &stamp())
        .unwrap();
    st.drawing_ui.open(first);
    let p1 = create_or_open(&mut st, A4_QUER).unwrap();
    let p2 = st.document.parts_sheets()[1];
    let names: Vec<String> = st
        .document
        .drawings()
        .iter()
        .map(|d| d.name.clone())
        .collect();
    assert_eq!(names, ["Blatt 1", "Stückliste", "Stückliste (2)"]);
    let mut b = Browser::new();
    b.idle(&mut st);

    // Umbenennen gesperrt (beide Blätter), mit Hinweis.
    for row in ["Stückliste", "Stückliste (2)"] {
        let hover = b.menu_entry(&mut st, row, "Umbenennen");
        assert!(
            hover.contains(&PARTS_SHEET_RENAME.to_string()),
            "{row}: {hover:?}"
        );
        assert!(st.rename_drawing_draft.is_none(), "{row}: gesperrt");
    }
    // Auch der zweite Riegel.
    drawing_browser::request_rename(&mut st, p1);
    assert!(st.rename_drawing_draft.is_none());
    assert_eq!(st.message.as_deref(), Some(PARTS_SHEET_RENAME));
    // Eine gewöhnliche Zeichnung lässt sich umbenennen.
    drawing_browser::request_rename(&mut st, first);
    assert!(st.rename_drawing_draft.is_some());
    st.rename_drawing_draft = None;

    // Löschen bei Blatt 2 gesperrt.
    let hover = b.menu_entry(&mut st, "Stückliste (2)", "Löschen");
    assert!(hover.contains(&FOLLOW_PAGE_DELETE.to_string()), "{hover:?}");
    assert!(st.pending_delete_drawing.is_none());
    drawing_browser::request_delete_drawing(&mut st, p2);
    assert!(st.pending_delete_drawing.is_none());
    assert_eq!(st.message.as_deref(), Some(FOLLOW_PAGE_DELETE));
    assert!(st.document.drawing(p2).is_some());

    // Löschen bei Blatt 1 fragt mit der Zahl der Blätter und entfernt beide; Blatt 2 war offen
    // und gewählt: alles fällt sofort zurück.
    st.drawing_ui.open(p2);
    st.drawing_ui.selected_drawing = Some(p2);
    b.menu_entry(&mut st, "Stückliste", "Löschen");
    let plan = st.pending_delete_drawing.clone().expect("Rückfrage");
    assert_eq!(plan.text, "Stückliste mit 2 Blättern löschen?");
    let texts = b.idle(&mut st);
    assert!(texts.iter().any(|t| t.text == plan.text));
    drawing_browser::apply_delete(&mut st, &plan).unwrap();
    assert!(st.document.parts_sheets().is_empty());
    assert!(st.document.drawing(first).is_some());
    assert_eq!(st.drawing_ui.area, WorkArea::Model, "sofort, ohne Bild");
    assert_eq!(st.drawing_ui.selected_drawing, None);

    // Ein einzelnes Blatt (A4 hoch: 44 Zeilen): kürzere Frage.
    create_or_open(&mut st, A4_HOCH).unwrap();
    assert_eq!(st.document.parts_sheets().len(), 1);
    let p = st.document.parts_sheets()[0];
    drawing_browser::request_delete_drawing(&mut st, p);
    assert_eq!(
        st.pending_delete_drawing.as_ref().unwrap().text,
        "Stückliste löschen?"
    );
}

#[test]
fn no_views_on_a_parts_sheet() {
    let (mut st, _) = boxes(3);
    new_drawing(&mut st, A3_QUER);
    let p1 = create_or_open(&mut st, A4_HOCH).unwrap();
    let before = st.document.edit_count();
    assert_eq!(
        drawing_view::add_view(&mut st, ViewKind::Front),
        Err(NO_VIEWS_ON_PARTS_SHEET.to_string())
    );
    assert!(st.document.drawing(p1).unwrap().views.is_empty());
    assert_eq!(st.document.edit_count(), before);
    // Im Werkzeugfeld gesperrt mit Hinweis.
    let mut h = H::new(BIG, false, st);
    let shot = h.settle();
    let p = shot
        .texts
        .iter()
        .find(|t| t.text == "Ansicht hinzufügen")
        .expect("Knopf")
        .rect
        .center();
    let shot = h.click(p);
    assert!(!shot.has("Isometrisch"), "Menü geht nicht auf");
    let tip = h.hover(p);
    assert!(tip.has(NO_VIEWS_ON_PARTS_SHEET), "Hinweis fehlt");
    assert!(h.st().document.drawing(p1).unwrap().views.is_empty());
}

// Weitere Fälle --------------------------------------------------------------------------------

#[test]
fn sheet_follows_names_and_own_texts_without_a_drawing_change() {
    let (mut st, features) = boxes(3);
    let id = new_drawing(&mut st, A3_QUER);
    let mut h = H::new(BIG, false, st);
    assert!(h.table_texts().contains(&"Quader 2".to_string()));
    let drawing = h.st().document.drawing(id).unwrap().clone();
    // Eigener Text direkt im Dokument (ändert weder Zeichnung noch Modell).
    let key = h.st().document.parts_rows()[1].keys[0];
    h.st()
        .document
        .set_parts_override(
            key,
            RowOverride {
                material: FieldValue::Text("Stahl".into()),
                ..RowOverride::default()
            },
        )
        .unwrap();
    assert_eq!(*h.st().document.drawing(id).unwrap(), drawing);
    assert!(
        h.table_texts().contains(&"Stahl".to_string()),
        "eigener Text"
    );
    // Umbenennen.
    h.st().rename_feature(features[2], "Welle").unwrap();
    let t = h.table_texts();
    assert!(t.contains(&"Welle".to_string()) && !t.contains(&"Quader 3".to_string()));
    // Neuer Körper.
    h.st().create_box(DVec3::new(40.0, 30.0, 20.0)).unwrap();
    assert!(h.table_texts().contains(&"Quader 4".to_string()));
}

#[test]
fn typing_in_a_row_does_not_trigger_shortcuts() {
    let (mut st, _) = boxes(3);
    let id = new_drawing(&mut st, A3_QUER);
    let front = st.document.drawing(id).unwrap().views[0].id;
    let mut h = H::new(TALL, false, st);
    h.st().drawing_ui.selected_view = Some(front);
    h.st().drawing_ui.zoom = 1.5;
    h.open_dialog();
    let r = h.scroll_to("Quader 2");
    h.click(r.center());
    assert!(viewport::typing(&h.c), "das Feld hat die Tastatur");
    h.key(Key::Delete);
    h.typed("d");
    h.typed("+");
    h.key(Key::Escape);
    assert!(
        h.st().document.drawing(id).unwrap().view(front).is_some(),
        "Ansicht bleibt"
    );
    assert_eq!(h.st().drawing_ui.selected_view, Some(front));
    assert_eq!(h.st().drawing_ui.zoom, 1.5);
    let key = h.st().document.parts_rows()[1].keys[0];
    assert_eq!(
        h.dialog().parts.overrides[&key].name,
        FieldValue::Text("d+".into())
    );
}

#[test]
fn locked_in_old_versions_and_in_the_sketch_mode() {
    // Skizzen-Modus: kein Stücklistenblatt.
    let (mut st, _) = boxes(2);
    new_drawing(&mut st, A3_QUER);
    let sk = st
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    st.open_sketch(sk).unwrap();
    let before = st.document.edit_count();
    assert_eq!(
        create_or_open(&mut st, A4_HOCH),
        Err(SKETCH_BLOCKED.to_string())
    );
    assert!(st.document.parts_sheets().is_empty());
    assert_eq!(st.document.edit_count(), before);

    // Alte Version: das Fenster schließt sich.
    let (mut st, _) = boxes(2);
    new_drawing(&mut st, A3_QUER);
    let mut h = H::new(BIG, false, st);
    h.open_dialog();
    assert!(h.is_open());
    h.app.ws_mut().active_tab_mut().read_only = true;
    h.settle();
    assert!(!h.is_open(), "in einer alten Version kein Fenster");
}

// (h) ------------------------------------------------------------------------------------------

#[test]
fn texts_are_plain() {
    for dark in [false, true] {
        let (mut st, _) = boxes(12);
        new_drawing(&mut st, A3_QUER);
        let mut h = H::new(TALL, dark, st);
        h.open_dialog();
        let mut all: Vec<String> = h.all_dialog_texts().into_iter().map(|t| t.text).collect();
        // Und auf einem Stücklistenblatt (Knopf „öffnen“).
        let id = h.dialog().drawing;
        h.click_text(CREATE);
        let p1 = h.st().document.parts_sheets()[0];
        h.st().sheet_dialog = None;
        h.st().drawing_ui.open(p1);
        h.open_dialog();
        all.extend(h.all_dialog_texts().into_iter().map(|t| t.text));
        all.sort();
        all.dedup();
        for must in [
            SECTION,
            SWITCH,
            TOO_MANY_ON_SHEET,
            OPEN,
            "Pos. 12",
            "Benennung",
        ] {
            assert!(all.iter().any(|t| t == must), "„{must}“ fehlt: {all:?}");
        }
        for t in &all {
            for banned in ["Spec", "ISO", "DXF", "Format 7", "Kennung", "TODO"] {
                // Die Vorgabe der Allgemeintoleranz steht auch auf dem Blatt (3c-1).
                if t.contains("2768") {
                    continue;
                }
                assert!(!t.contains(banned), "„{t}“ enthält „{banned}“");
            }
        }
        let _ = id;
    }
    for t in [
        SECTION,
        SWITCH,
        TOO_MANY_ON_SHEET,
        TOO_MANY_ROWS,
        NO_PARTS,
        CREATE,
        OPEN,
        NO_VIEWS_ON_PARTS_SHEET,
    ] {
        for banned in ["Spec", "ISO", "DXF", "Format 7", "Kennung", "TODO"] {
            assert!(!t.contains(banned), "„{t}“");
        }
    }
}

/// Der Abschnitt allein in einem hohen Bild mit `n` künstlichen Zeilen; liefert die Texte und ob
/// der Knopf geklickt wurde.
fn section_alone(n: usize, switch: bool, kind: DrawingKind) -> Vec<String> {
    use planora_drawing::sheet_info::SheetInfo;
    use planora_drawing::{BodyKey, PartsRow};
    let rows: Vec<PartsRow> = (0..n)
        .map(|i| PartsRow {
            pos: i as u32 + 1,
            name: format!("Teil {}", i + 1),
            qty: 1,
            keys: vec![BodyKey {
                feature: i as u64 + 1,
                index: 0,
            }],
        })
        .collect();
    let auto: Vec<planora_drawing::PartsCells> = rows
        .iter()
        .map(|r| {
            [
                r.pos.to_string(),
                r.name.clone(),
                "1".into(),
                "–".into(),
                String::new(),
            ]
        })
        .collect();
    let mut info = SheetInfo::new(&stamp());
    info.parts_list = switch;
    let st = AppState::new();
    let mut draft = planora_app::parts_section::PartsDraft::new(&st);
    let layout = sheet_layout_with(A3_QUER, 0, 0, PartsSpace::OnSheet { rows: n });
    let ctx = egui::Context::default();
    ctx.set_fonts(fonts::definitions());
    let mut texts = Vec::new();
    for _ in 0..2 {
        let out = ctx.run_ui(input(vec2(720.0, 200_000.0), vec![]), |ui| {
            planora_app::parts_section::show_section(
                ui, false, kind, &mut info, &mut draft, &rows, &auto, &layout, false,
            );
        });
        texts = Shot::of(&out, &[])
            .texts
            .into_iter()
            .map(|t| t.text)
            .collect();
        out.drop_without_applying_deltas();
    }
    texts
}

#[test]
fn section_messages_and_row_limit() {
    let has = |t: &[String], s: &str| t.iter().any(|x| x == s);
    // 201 Zeilen: Hinweis, nur 200 Zeilen im Abschnitt.
    let t = section_alone(201, true, DrawingKind::Views);
    assert!(has(&t, TOO_MANY_ROWS) && has(&t, TOO_MANY_ON_SHEET));
    assert!(has(&t, "Pos. 200") && !has(&t, "Pos. 201"));
    // Genau 200: kein Hinweis zu den Stücklistenblättern.
    let t = section_alone(200, true, DrawingKind::Views);
    assert!(!has(&t, TOO_MANY_ROWS) && has(&t, TOO_MANY_ON_SHEET));
    // 10 Zeilen passen aufs Blatt, 11 nicht; ausgeschaltet kein Hinweis.
    assert!(!has(
        &section_alone(10, true, DrawingKind::Views),
        TOO_MANY_ON_SHEET
    ));
    assert!(has(
        &section_alone(11, true, DrawingKind::Views),
        TOO_MANY_ON_SHEET
    ));
    assert!(!has(
        &section_alone(11, false, DrawingKind::Views),
        TOO_MANY_ON_SHEET
    ));
    // Auf einem Stücklistenblatt weder Schalter noch Hinweis zum Zeichnungsblatt.
    let t = section_alone(11, true, DrawingKind::PartsList { page: 1 });
    assert!(!has(&t, SWITCH) && !has(&t, TOO_MANY_ON_SHEET));
    assert!(has(&section_alone(11, true, DrawingKind::Views), SWITCH));
    // Ohne Zeilen.
    let t = section_alone(0, true, DrawingKind::Views);
    assert!(has(&t, NO_PARTS) && has(&t, CREATE));
}

#[test]
fn messages_without_parts() {
    // Ohne Körper: „Es gibt noch keine Teile.“
    let mut st = AppState::new();
    let id = st
        .document
        .add_drawing_stamped("Blatt 1", A3_QUER, Projection::First, &stamp())
        .unwrap();
    st.drawing_ui.open(id);
    let mut h = H::new(TALL, false, st);
    h.open_dialog();
    h.scroll_to(NO_PARTS);
    assert!(h.try_scroll_to("Pos. 1").is_none());
}

// (i) ------------------------------------------------------------------------------------------

#[test]
fn fits_small_window() {
    for dark in [false, true] {
        let (mut st, _) = boxes(3);
        new_drawing(&mut st, A3_QUER);
        let mut h = H::new(SMALL, dark, st);
        h.open_dialog();
        let window = Rect::from_min_size(Pos2::ZERO, SMALL);
        let rect = h.dialog_rect().unwrap();
        assert!(window.contains_rect(rect), "{rect:?}");
        // Einen eigenen Text, damit „Zurücksetzen“ erscheint.
        h.type_into_hinted("Quader 3", "Bolzen");
        for t in [SECTION, "Pos. 3", "Bolzen", "Zurücksetzen", CREATE, "quer"] {
            h.scroll_to(t);
            let shot = h.settle();
            let rect = h.dialog_rect().unwrap();
            for s in &shot.dialog {
                if s.rect.intersects(rect) && s.clip.intersects(s.rect) {
                    assert!(
                        rect.expand(0.5).contains_rect(s.rect) || !rect.contains(s.rect.center()),
                        "„{}“ ragt aus dem Fenster {rect:?}: {:?}",
                        s.text,
                        s.rect
                    );
                    assert!(
                        s.rect.right() <= rect.right() + 0.5,
                        "„{}“ ragt rechts hinaus",
                        s.text
                    );
                }
            }
            // „Zurücksetzen“ bleibt links vom Rollbalken (mit Abstand).
            let resets = shot.dlg("Zurücksetzen", rect);
            assert!(t != "Zurücksetzen" || !resets.is_empty());
            if let Some(reset) = resets.first() {
                let bar = shot
                    .fills
                    .iter()
                    .map(|(r, _)| *r)
                    .filter(|r| {
                        r.width() <= 14.0
                            && r.height() > 60.0
                            && r.left() > rect.center().x
                            && rect.contains_rect(*r)
                    })
                    .min_by(|a, b| a.left().total_cmp(&b.left()))
                    .expect("Rollbalken");
                let button = shot
                    .fills
                    .iter()
                    .map(|(r, _)| *r)
                    .filter(|r| r.contains(reset.rect.center()) && r.width() < 200.0)
                    .min_by(|a, b| a.area().total_cmp(&b.area()))
                    .unwrap_or(reset.rect);
                assert!(
                    button.right() + bar.width() <= bar.left() + 0.5,
                    "Zurücksetzen {button:?} am Rollbalken {bar:?}"
                );
            }
            // Felder (26 px hoch) bleiben im Fenster.
            for (f, _) in &shot.fills {
                if (f.height() - 26.0).abs() < 1.0 && rect.contains(f.center()) {
                    assert!(f.right() <= rect.right() + 0.5, "Feld {f:?} in {rect:?}");
                }
            }
        }
    }
}

// (j) ------------------------------------------------------------------------------------------

#[test]
fn old_drawing_unchanged() {
    let (mut st, _) = boxes(3);
    let id = st
        .document
        .add_drawing("Alt", A3_QUER, Projection::First)
        .unwrap();
    st.document
        .add_view(
            id,
            ViewKind::Front,
            planora_drawing::Scale { num: 1, den: 1 },
            Placement::Free { x: 150.0, y: 150.0 },
            None,
        )
        .unwrap();
    st.drawing_ui.open(id);
    let format = required_format(&st);
    let mut h = H::new(BIG, false, st);
    let shot = h.settle();
    assert!(!shot.has("Benennung"), "keine Tabelle");
    assert!(h.table_rect().is_none());
    let before = h.edits();
    h.open_dialog();
    assert!(!h.dialog().sheet.parts_list, "Schalter aus");
    h.click_text("Abbrechen");
    assert_eq!(h.edits(), before);
    assert_eq!(required_format(h.st()), format);
    assert!(h.st().document.drawing(id).unwrap().info.is_none());
    let d = h.st().document.drawing(id).unwrap().clone();
    assert_eq!(d.kind, DrawingKind::Views);
    assert_eq!(
        drawing_view::parts_space(h.st(), &d, true),
        PartsSpace::None
    );
}
