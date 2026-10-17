//! Fenster „Blattangaben“ (Zeichnungsblatt, Aufgabe 6): Knopf und Menü, Eingabe mit Vorgaben,
//! Übernehmen und Abbrechen, Änderungen, Warnungen am Feld, Tastatur und Wortwahl – mit der ganzen
//! App (`eframe::App::ui`) und Rohereignissen je Bild, ohne Fenster.

mod common;

use eframe::App as _;
use egui::{Color32, Event, Key, Modifiers, PointerButton, Pos2, Rect, Vec2, pos2, vec2};
use planora_app::app::PlanoraApp;
use planora_app::drawing_browser::READ_ONLY_HINT;
use planora_app::drawing_dialog::SKETCH_BLOCKED;
use planora_app::drawing_state::WorkArea;
use planora_app::sheet_dialog::{
    BUTTON, CUT_WARNING, MENU_ENTRY, NO_DRAWING_OPEN, OWNER_MISSING, SheetInfoDialog,
};
use planora_app::state::{AppState, StandardPlane};
use planora_app::workspace::Workspace;
use planora_app::{drawing_view, fonts, icons, shell_panels, theme, viewport};
use planora_drawing::sheet_frame::TitleField;
use planora_drawing::sheet_info::{
    FieldValue, NotesValue, Stamp, TOO_MANY_REVISIONS, resolve_texts,
};
use planora_drawing::{DrawingId, Paper, Placement, Projection, Scale, Sheet, ViewId, ViewKind};
use planora_feature::SketchPlane;
use planora_math::DVec3;
use planora_project::config::Config;
use planora_project::library::Library;

const SMALL: Vec2 = vec2(800.0, 600.0);
const BIG: Vec2 = vec2(1400.0, 900.0);
/// Hoch genug, dass das ganze Fenster ohne Rollen zu sehen ist.
const TALL: Vec2 = vec2(1400.0, 1700.0);

const A4_QUER: Sheet = Sheet {
    paper: Paper::A4,
    landscape: true,
};

thread_local! {
    static TIME: std::cell::Cell<f64> = const { std::cell::Cell::new(0.0) };
}

fn input(size: Vec2, events: Vec<Event>) -> egui::RawInput {
    // Kurze Bildabstände: Klicks gelten nicht als langes Halten.
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

    /// Alle Stellen mit genau diesem Text, die ganz in `r` liegen und nicht abgeschnitten sind.
    fn visible_in(&self, text: &str, r: Rect) -> Vec<&Seen> {
        self.texts
            .iter()
            .filter(|t| {
                t.text == text && r.expand(0.5).contains_rect(t.rect) && {
                    t.clip.expand(0.5).contains_rect(t.rect)
                }
            })
            .collect()
    }

    /// Wie `visible_in`, aber nur Texte des Fensters (nicht, was schon vorher auf dem Bildschirm
    /// stand, etwa das Blatt dahinter).
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
}

fn library_ws() -> Workspace {
    let root = common::temp_root("planora-sheet-dialog");
    Workspace::new(Some(Library::open(&root).unwrap()), Config::default())
}

fn stamp() -> Stamp {
    Stamp {
        date: "2026-10-17".into(),
        user: "dennis".into(),
    }
}

/// Quader 100 × 60 × 40 und die Zeichnung „Blatt 1“ (A4 quer) mit einer Vorderansicht; `old`:
/// ohne Blattangaben und ohne Projektangaben wie in einer Datei aus der Zeit davor.
fn drawing_state(old: bool) -> (AppState, DrawingId, ViewId) {
    let mut st = AppState::new();
    st.create_box(DVec3::new(100.0, 60.0, 40.0)).unwrap();
    let d = if old {
        st.document
            .add_drawing("Blatt 1", A4_QUER, Projection::First)
            .unwrap()
    } else {
        st.document
            .add_drawing_stamped("Blatt 1", A4_QUER, Projection::First, &stamp())
            .unwrap()
    };
    let front = st
        .document
        .add_view(
            d,
            ViewKind::Front,
            Scale { num: 1, den: 1 },
            Placement::Free { x: 150.0, y: 140.0 },
            None,
        )
        .unwrap();
    st.drawing_ui.open(d);
    st.drawing_ui.tick(&st.bodies, &st.document, true);
    (st, d, front)
}

struct H {
    c: egui::Context,
    app: PlanoraApp,
    size: Vec2,
    /// Texte des Bildes ohne Fenster (das Blatt und die Felder dahinter).
    base: Vec<(String, Rect)>,
}

impl H {
    fn new(size: Vec2, dark: bool, old: bool) -> (H, DrawingId, ViewId) {
        let c = egui::Context::default();
        c.set_fonts(fonts::definitions());
        theme::apply(&c, dark);
        // Hinweistexte sofort zeigen.
        c.all_styles_mut(|s| s.interaction.tooltip_delay = 0.0);
        let mut app = PlanoraApp::for_tests(library_ws());
        let (mut st, d, v) = drawing_state(old);
        st.dark = dark;
        app.ws_mut().active_tab_mut().state = st;
        (
            H {
                c,
                app,
                size,
                base: Vec::new(),
            },
            d,
            v,
        )
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

    fn field_rect(&self, name: &str) -> Rect {
        shell_panels::field_rects(&self.c)
            .into_iter()
            .find(|(n, _)| *n == name)
            .unwrap_or_else(|| panic!("Feld {name} fehlt"))
            .1
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

    /// Merkt sich die Texte des Bildes ohne Fenster.
    fn capture_base(&mut self) {
        self.base.clear();
        let shot = self.settle();
        self.base = shot
            .texts
            .iter()
            .map(|t| (t.text.clone(), t.rect))
            .collect();
    }

    /// Rollt das Werkzeugfeld seitlich, bis `text` ganz zu sehen ist; liefert das Bild dazu
    /// (`None`, wenn es schon im ersten Bild ganz zu sehen war).
    fn reveal_in_toolbar(&mut self, text: &str) -> Option<Shot> {
        let tool = self.field_rect("Werkzeugfeld");
        let shot = self.settle();
        if !shot.visible_in(text, tool).is_empty() {
            return None;
        }
        let at = tool.center();
        self.frame(vec![Event::PointerMoved(at)]);
        for _ in 0..40 {
            let wheel = Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: vec2(-40.0, 0.0),
                phase: egui::TouchPhase::Move,
                modifiers: Modifiers::NONE,
            };
            self.frame(vec![Event::PointerMoved(at), wheel]);
            let shot = self.frame(vec![]);
            if !shot.visible_in(text, tool).is_empty() {
                return Some(self.settle());
            }
        }
        panic!("„{text}“ im Werkzeugfeld nie ganz zu sehen");
    }

    /// Klick auf den Knopf „Blattangaben“ im Werkzeugfeld.
    fn open_from_toolbar(&mut self) {
        self.capture_base();
        let shot = self
            .reveal_in_toolbar(BUTTON)
            .unwrap_or_else(|| self.settle());
        let tool = self.field_rect("Werkzeugfeld");
        let p = shot
            .visible_in(BUTTON, tool)
            .first()
            .expect("Knopf Blattangaben")
            .rect
            .center();
        self.click(p);
        self.settle();
    }

    /// Öffnet das Menü „Zeichnung“; liefert das Bild mit dem offenen Menü.
    fn open_menu(&mut self) -> Shot {
        self.capture_base();
        let shot = self.settle();
        let top = self.field_rect("oben");
        let menu = shot
            .visible_in("Zeichnung", top)
            .first()
            .expect("Menü Zeichnung")
            .rect
            .center();
        self.click(menu);
        self.frame(vec![])
    }

    /// Menü „Zeichnung“ öffnen und den Eintrag `entry` anklicken.
    fn menu_click(&mut self, entry: &str) -> Shot {
        let shot = self.open_menu();
        let p = shot
            .texts
            .iter()
            .find(|t| t.text == entry)
            .unwrap_or_else(|| panic!("Menüeintrag „{entry}“ fehlt"))
            .rect
            .center();
        self.click(p)
    }

    /// Mausrad im Fenster um `dy` Punkte.
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

    /// Rollt das Fenster, bis `text` ganz zu sehen ist, und liefert seine Lage.
    fn scroll_to(&mut self, text: &str) -> Rect {
        self.settle();
        for _ in 0..3 {
            self.wheel(4000.0);
        }
        for _ in 0..60 {
            let shot = self.settle();
            let rect = self.dialog_rect().expect("Fenster offen");
            if let Some(s) = shot.dlg(text, rect).first() {
                return s.rect;
            }
            self.wheel(-150.0);
        }
        panic!("„{text}“ nie zu sehen");
    }

    /// Klick auf den Text `text` im Fenster (nach dem Rollen).
    fn click_text(&mut self, text: &str) {
        let r = self.scroll_to(text);
        self.click(r.center());
    }

    /// Ein Textfeld, dessen Hinweistext `hint` ist: anklicken und `text` eintippen.
    fn type_into_hinted(&mut self, hint: &str, text: &str) {
        let r = self.scroll_to(hint);
        self.click(r.center());
        self.typed(text);
    }

    fn texts_in_dialog(&mut self) -> Vec<Seen> {
        let shot = self.settle();
        let rect = self.dialog_rect().expect("Fenster offen");
        shot.dialog
            .into_iter()
            .filter(|t| rect.expand(0.5).contains_rect(t.rect))
            .filter(|t| t.clip.expand(0.5).contains_rect(t.rect))
            .collect()
    }
}

fn edit_count(h: &mut H) -> u64 {
    h.st().document.edit_count()
}

/// Die Einträge des Menüs „Zeichnung“ allein in einem Fenster (wie `tests/drawing_ui.rs`): Zeiger
/// auf `MENU_ENTRY`, dann ein Klick. Liefert die Texte des Bildes mit dem Hinweis.
fn menu_entry_hint(st: &mut AppState, read_only: bool) -> Vec<String> {
    let c = egui::Context::default();
    c.set_fonts(fonts::definitions());
    c.all_styles_mut(|s| s.interaction.tooltip_delay = 0.0);
    let mut dialog = None;
    let mut run = |st: &mut AppState, events: Vec<Event>| -> Vec<Seen> {
        let out = c.run_ui(input(vec2(600.0, 400.0), events), |ui| {
            planora_app::drawing_dialog::drawing_menu(ui, st, &mut dialog, read_only);
        });
        let shot = Shot::of(&out, &[]);
        out.drop_without_applying_deltas();
        shot.texts
    };
    run(st, vec![]);
    let texts = run(st, vec![]);
    let p = texts
        .iter()
        .find(|t| t.text == MENU_ENTRY)
        .expect("Menüeintrag")
        .rect
        .center();
    let mut last = Vec::new();
    for _ in 0..3 {
        last = run(st, vec![Event::PointerMoved(p)]);
    }
    let btn = |pressed| Event::PointerButton {
        pos: p,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    };
    run(st, vec![btn(true)]);
    run(st, vec![btn(false)]);
    last.into_iter().map(|t| t.text).collect()
}

// (a) ------------------------------------------------------------------------------------------

#[test]
fn opens_from_toolbar_and_menu() {
    for (size, dark) in [(SMALL, false), (BIG, true), (SMALL, true), (BIG, false)] {
        let (mut h, _, _) = H::new(size, dark, false);
        let shot = h.settle();
        let tool = h.field_rect("Werkzeugfeld");
        let shot = h.reveal_in_toolbar(BUTTON).unwrap_or(shot);
        let fit = shot
            .texts
            .iter()
            .find(|t| t.text == "Blatt einpassen" && tool.contains(t.rect.center()))
            .expect("Blatt einpassen")
            .rect;
        let info = shot.visible_in(BUTTON, tool)[0].rect;
        assert!(
            info.left() > fit.right() && (info.center().y - fit.center().y).abs() < 6.0,
            "{size:?}: Knopf rechts neben „Blatt einpassen“: {fit:?} {info:?}"
        );
        // Zwischen den beiden Knöpfen steht nichts anderes.
        assert!(
            !shot.texts.iter().any(|t| tool.contains_rect(t.rect)
                && !t.text.starts_with(icons::glyph(icons::Icon::SheetInfo))
                && t.rect.left() > fit.right()
                && t.rect.right() < info.left()),
            "{size:?}: Text zwischen den Knöpfen"
        );

        h.open_from_toolbar();
        let rect = h.dialog_rect().expect("Fenster offen");
        assert!((rect.width() - 720.0).abs() <= 1.0, "{size:?}: {rect:?}");
        assert!(
            (rect.center().x - size.x / 2.0).abs() <= 1.0
                && (rect.center().y - size.y / 2.0).abs() <= 1.5,
            "{size:?}: mittig {rect:?}"
        );
        let window = Rect::from_min_size(Pos2::ZERO, size);
        assert!(window.contains_rect(rect), "{size:?}: {rect:?}");
        let shot = h.settle();
        assert!(!shot.dlg("Projekt", rect).is_empty(), "Projekt");
        for section in ["Dieses Blatt", "Änderungen"] {
            h.scroll_to(section);
        }
        let first = h.dialog().clone().drawing;

        // Schließen-Symbol schließt; das Menü öffnet dasselbe Fenster.
        let close = icons::glyph(icons::Icon::Close).to_string();
        let shot = h.settle();
        let x = shot.dlg(&close, rect)[0].rect.center();
        h.click(x);
        assert!(!h.is_open(), "{size:?}: Schließen-Symbol");
        h.menu_click(MENU_ENTRY);
        assert!(h.is_open(), "{size:?}: Menü öffnet das Fenster");
        h.settle();
        assert_eq!(h.dialog().drawing, first);
        let again = h.dialog_rect().unwrap();
        assert!(
            (again.min - rect.min).abs().max_elem() <= 1.0
                && (again.size() - rect.size()).abs().max_elem() <= 1.0,
            "{size:?}: gleiches Fenster {rect:?} {again:?}"
        );
    }
}

#[test]
fn locked_in_old_versions_and_in_the_sketch_mode() {
    // Alte Version: Knopf und Menüeintrag gesperrt, mit Hinweis.
    let (mut h, _, _) = H::new(BIG, false, false);
    h.app.ws_mut().active_tab_mut().read_only = true;
    let shot = h.settle();
    let tool = h.field_rect("Werkzeugfeld");
    let p = shot.visible_in(BUTTON, tool)[0].rect.center();
    h.click(p);
    assert!(!h.is_open(), "Knopf gesperrt");
    let tip = h.hover(p);
    assert!(tip.has(READ_ONLY_HINT), "Hinweis am Knopf fehlt");
    let texts = menu_entry_hint(h.st(), true);
    assert!(
        texts.contains(&READ_ONLY_HINT.to_string()),
        "Hinweis am Menüeintrag fehlt: {texts:?}"
    );
    assert!(!h.is_open(), "Menüeintrag gesperrt");
    h.menu_click(MENU_ENTRY);
    assert!(!h.is_open(), "Menüeintrag in der App gesperrt");

    // Skizzen-Modus: ein offenes Fenster schließt sich, das Menü ist gesperrt.
    let (mut h, _, _) = H::new(BIG, false, false);
    h.open_from_toolbar();
    assert!(h.is_open());
    let id = h
        .st()
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    h.st().open_sketch(id).unwrap();
    h.settle();
    assert!(!h.is_open(), "Fenster schließt sich im Skizzen-Modus");
    assert_eq!(h.st().drawing_ui.area, WorkArea::Model);
    let texts = menu_entry_hint(h.st(), false);
    assert!(
        texts.contains(&SKETCH_BLOCKED.to_string()),
        "Hinweis fehlt: {texts:?}"
    );
    assert!(!h.is_open());
    h.menu_click(MENU_ENTRY);
    assert!(!h.is_open(), "Menüeintrag im Skizzen-Modus gesperrt");

    // Ohne offene Zeichnung (Modell): Menüeintrag gesperrt mit Hinweis.
    let (mut h, _, _) = H::new(BIG, false, false);
    h.st().drawing_ui.area = WorkArea::Model;
    let texts = menu_entry_hint(h.st(), false);
    assert!(
        texts.contains(&NO_DRAWING_OPEN.to_string()),
        "Hinweis fehlt: {texts:?}"
    );
    assert!(!h.is_open());
    h.menu_click(MENU_ENTRY);
    assert!(!h.is_open());
    // Und mit offener Zeichnung geht es.
    h.st().drawing_ui.area = WorkArea::Drawing(h.st().document.drawings()[0].id);
    h.menu_click(MENU_ENTRY);
    assert!(h.is_open());
}

// (b) ------------------------------------------------------------------------------------------

#[test]
fn edit_apply_and_reset() {
    let (mut h, d, _) = H::new(TALL, false, false);
    let before = edit_count(&mut h);
    h.open_from_toolbar();
    let rect = h.dialog_rect().unwrap();
    // Die Hinweistexte in den leeren Feldern sind die automatischen Werte.
    let shot = h.settle();
    let number = format!("{}-01", h.st().document_name);
    for auto in [
        "Blatt 1",
        "in Arbeit",
        "Fertigungszeichnung",
        number.as_str(),
    ] {
        assert!(!shot.dlg(auto, rect).is_empty(), "Hinweis „{auto}“");
    }
    h.type_into_hinted("Blatt 1", "Lager");
    h.type_into_hinted("in Arbeit", "freigegeben");
    let texts: Vec<String> = h.texts_in_dialog().into_iter().map(|t| t.text).collect();
    assert!(texts.contains(&"5/60".to_string()), "Länge 5/60: {texts:?}");
    assert!(
        texts.contains(&"11/80".to_string()),
        "Länge 11/80: {texts:?}"
    );
    assert_eq!(
        edit_count(&mut h),
        before,
        "Tippen ändert das Dokument nicht"
    );
    h.click_text("Übernehmen");
    assert!(!h.is_open());
    assert_eq!(edit_count(&mut h), before + 1);
    let shot = h.settle();
    assert!(shot.has("Lager"), "Blatt zeigt den Titel");
    assert!(shot.has("freigegeben"), "Blatt zeigt den Status");
    let info = h.st().document.drawing(d).unwrap().info.clone().unwrap();
    assert_eq!(info.title, FieldValue::Text("Lager".into()));
    assert_eq!(info.status, FieldValue::Text("freigegeben".into()));

    // Wieder öffnen: die Eingaben stehen da; „Zurücksetzen“ macht beide Felder automatisch.
    h.open_from_toolbar();
    let rect = h.dialog_rect().unwrap();
    let shot = h.settle();
    assert!(!shot.dlg("Lager", rect).is_empty());
    assert!(!shot.dlg("5/60", rect).is_empty());
    h.click_text("Zurücksetzen");
    h.click_text("Zurücksetzen");
    assert!(h.settle().dlg("Zurücksetzen", rect).is_empty());
    h.click_text("Übernehmen");
    assert!(!h.is_open());
    assert_eq!(edit_count(&mut h), before + 2);
    let shot = h.settle();
    assert!(!shot.has("Lager") && !shot.has("freigegeben"));
    assert!(shot.has("Blatt 1") && shot.has("in Arbeit"));
    let info = h.st().document.drawing(d).unwrap().info.clone().unwrap();
    assert_eq!(info.title, FieldValue::Auto);
    assert_eq!(info.status, FieldValue::Auto);

    // Übernehmen ohne Änderung lässt das Dokument unberührt.
    h.open_from_toolbar();
    h.click_text("Übernehmen");
    assert_eq!(edit_count(&mut h), before + 2);
}

// (c) ------------------------------------------------------------------------------------------

#[test]
fn cancel_changes_nothing() {
    let (mut h, d, _) = H::new(TALL, false, true);
    assert!(h.st().document.drawing(d).unwrap().info.is_none());
    assert!(h.st().document.project_info().is_none());
    let before = edit_count(&mut h);

    // Öffnen und Tippen, dann „Abbrechen“.
    h.open_from_toolbar();
    assert_eq!(edit_count(&mut h), before, "Öffnen ändert nichts");
    h.type_into_hinted("Blatt 1", "Lager");
    assert_eq!(h.dialog().sheet.title, FieldValue::Text("Lager".into()));
    h.click_text("Abbrechen");
    assert!(!h.is_open());
    assert!(h.st().document.drawing(d).unwrap().info.is_none());
    assert!(h.st().document.project_info().is_none());
    assert_eq!(edit_count(&mut h), before);

    // Öffnen und Tippen, dann das Schließen-Symbol.
    h.open_from_toolbar();
    h.type_into_hinted("Blatt 1", "Lager");
    let rect = h.dialog_rect().unwrap();
    let close = icons::glyph(icons::Icon::Close).to_string();
    let shot = h.settle();
    let x = shot.dlg(&close, rect)[0].rect.center();
    h.click(x);
    assert!(!h.is_open());
    assert!(h.st().document.drawing(d).unwrap().info.is_none());
    assert!(h.st().document.project_info().is_none());
    assert_eq!(edit_count(&mut h), before);

    // „Übernehmen“ ohne Eingabe setzt beide Sätze; das Ausgabedatum ist der Tag des Übernehmens.
    h.open_from_toolbar();
    h.click_text("Übernehmen");
    assert!(!h.is_open());
    let today = Stamp::now().date;
    let info = h.st().document.drawing(d).unwrap().info.clone().unwrap();
    assert_eq!(info.created, today);
    assert_eq!(info.title, FieldValue::Auto);
    assert!(h.st().document.project_info().is_some());
    assert!(edit_count(&mut h) > before);
    let drawing = h.st().document.drawing(d).unwrap().clone();
    let ctx = drawing_view::sheet_context(h.st(), d).unwrap();
    let texts = resolve_texts(&drawing, h.st().document.project_info(), &ctx, None);
    assert_eq!(texts.get(TitleField::Issued), today);
}

// (d) ------------------------------------------------------------------------------------------

#[test]
fn revisions_in_the_dialog() {
    let (mut h, d, _) = H::new(TALL, false, false);
    h.open_from_toolbar();
    for _ in 0..3 {
        h.click_text("Änderung hinzufügen");
    }
    assert_eq!(h.dialog().sheet.revisions.len(), 3);
    let today = Stamp::now().date;
    let creator = {
        let p = h.dialog().project.clone();
        p.creator.resolve(&p.login).to_string()
    };
    for r in &h.dialog().sheet.revisions.clone() {
        assert_eq!(r.date, today);
        assert_eq!(r.name, creator);
        assert_eq!(r.letter, FieldValue::Auto);
    }
    // Die Buchstaben stehen als Hinweis in den leeren Feldern der Tabelle.
    let rect = h.dialog_rect().unwrap();
    let shot = h.settle();
    // Nur die Spalte der Buchstaben (der Änderungsstand zeigt denselben Buchstaben).
    let letters = |shot: &Shot, rect: Rect| -> Vec<(String, f32)> {
        let mut v: Vec<(String, f32)> = shot
            .dialog
            .iter()
            .filter(|t| {
                t.text.chars().count() == 1
                    && t.text.chars().all(|c| c.is_ascii_uppercase())
                    && rect.contains_rect(t.rect)
                    && t.rect.left() < rect.left() + 60.0
            })
            .map(|t| (t.text.clone(), t.rect.center().y))
            .collect();
        v.sort_by(|a, b| a.1.total_cmp(&b.1));
        v
    };
    let rows = letters(&shot, rect);
    let names: Vec<&str> = rows.iter().map(|r| r.0.as_str()).collect();
    assert_eq!(names, ["A", "B", "C"], "Buchstaben als Hinweis: {rows:?}");
    let yb = rows[1].1;
    // Löschen der mittleren Zeile: es bleiben A und B.
    let del = shot
        .dlg("Löschen", rect)
        .into_iter()
        .find(|t| (t.rect.center().y - yb).abs() < 6.0)
        .expect("Löschen in der Zeile B")
        .rect
        .center();
    h.click(del);
    assert_eq!(h.dialog().sheet.revisions.len(), 2);
    let shot = h.settle();
    let rect = h.dialog_rect().unwrap();
    let names: Vec<String> = letters(&shot, rect).into_iter().map(|r| r.0).collect();
    assert_eq!(names, ["A", "B"]);
    h.click_text("Übernehmen");
    let drawing = h.st().document.drawing(d).unwrap().clone();
    assert_eq!(drawing.info.as_ref().unwrap().revisions.len(), 2);
    let ctx = drawing_view::sheet_context(h.st(), d).unwrap();
    let texts = resolve_texts(&drawing, h.st().document.project_info(), &ctx, None);
    assert_eq!(texts.get(TitleField::Revision), "B");

    // Grenze: ab der 24. Änderung nur die Meldung.
    h.open_from_toolbar();
    let s = Stamp::now();
    while h.dialog().sheet.revisions.len() < 23 {
        h.dialog().add_revision(&s, "x");
    }
    assert_eq!(h.dialog().message, None);
    h.click_text("Änderung hinzufügen");
    assert_eq!(h.dialog().sheet.revisions.len(), 23);
    assert_eq!(h.dialog().message.as_deref(), Some(TOO_MANY_REVISIONS));
    h.scroll_to(TOO_MANY_REVISIONS);
    // Die Meldung verschwindet mit dem Löschen einer Zeile.
    let shot = h.settle();
    let rect = h.dialog_rect().unwrap();
    let del = shot.dlg("Löschen", rect);
    if let Some(t) = del.first() {
        let p = t.rect.center();
        h.click(p);
        assert_eq!(h.dialog().sheet.revisions.len(), 22);
        assert_eq!(h.dialog().message, None);
    }
}

// (e) ------------------------------------------------------------------------------------------

#[test]
fn warnings_in_place() {
    let (mut h, _, _) = H::new(TALL, false, false);
    h.open_from_toolbar();
    let rect = h.dialog_rect().unwrap();
    let shot = h.settle();
    let label = shot.dlg("Eigentümer", rect)[0].rect;
    let warn = shot.dlg(OWNER_MISSING, rect);
    assert_eq!(warn.len(), 1, "Eigentümer fehlt");
    // Die Warnung steht direkt unter dem Feld ihrer Zeile.
    let dy = warn[0].rect.top() - label.bottom();
    assert!((0.0..40.0).contains(&dy), "Warnung unter dem Feld: {dy}");
    assert!(shot.dlg(CUT_WARNING, rect).is_empty());

    // 80 × „W“ im Feld Eigentümer: zu breit für das Feld im Schriftfeld.
    let field = pos2(label.left() + 250.0, label.center().y);
    h.click(field);
    h.typed(&"W".repeat(80));
    let rect = h.dialog_rect().unwrap();
    let shot = h.settle();
    let label = shot.dlg("Eigentümer", rect)[0].rect;
    assert!(
        shot.dlg(OWNER_MISSING, rect).is_empty(),
        "Eigentümer ist jetzt eingetragen"
    );
    let cut = shot.dlg(CUT_WARNING, rect);
    assert_eq!(cut.len(), 1, "nur dieses Feld ist zu lang");
    let dy = cut[0].rect.top() - label.bottom();
    assert!((0.0..40.0).contains(&dy), "Warnung unter dem Feld: {dy}");
    assert!(shot.dlg("80/80", rect).len() == 1);
}

// (f) ------------------------------------------------------------------------------------------

#[test]
fn typing_does_not_trigger_shortcuts() {
    let (mut h, d, front) = H::new(TALL, false, false);
    h.st().drawing_ui.selected_view = Some(front);
    h.st().drawing_ui.zoom = 1.5;
    let zoom = h.st().drawing_ui.zoom;
    h.open_from_toolbar();
    let rect = h.dialog_rect().unwrap();
    let hint = h.settle().dlg("Blatt 1", rect)[0].rect.center();
    h.click(hint);
    assert!(viewport::typing(&h.c), "das Titelfeld hat die Tastatur");
    h.key(Key::Delete);
    h.typed("d");
    h.typed("+");
    assert!(viewport::typing(&h.c));
    assert_eq!(h.dialog().sheet.title, FieldValue::Text("d+".into()));
    h.key(Key::Escape);
    assert!(
        h.st().document.drawing(d).unwrap().view(front).is_some(),
        "Ansicht bleibt"
    );
    assert_eq!(h.st().drawing_ui.selected_view, Some(front));
    assert_eq!(h.st().drawing_ui.zoom, zoom, "Zoom unverändert");
    assert_eq!(h.dialog().sheet.title, FieldValue::Text("d+".into()));
}

// (g) ------------------------------------------------------------------------------------------

#[test]
fn dialog_texts_are_plain() {
    // Eigene Angaben, damit nur die Texte des Fensters selbst geprüft werden (die Vorgabe der
    // Toleranz steht als Hinweistext auch auf dem Blatt).
    for dark in [false, true] {
        let (mut h, d, _) = H::new(TALL, dark, false);
        let mut p = h.st().document.project_info().cloned().expect("Projekt");
        p.tolerance = FieldValue::Text("mittel".into());
        p.notes = NotesValue::Lines(vec!["Kanten gebrochen".into()]);
        h.st().document.set_project_info(p).unwrap();
        h.open_from_toolbar();
        let s = Stamp::now();
        h.dialog().add_revision(&s, "dennis");
        h.dialog().sheet.notes = NotesValue::Lines(vec!["Nur hier".into()]);
        // Kein Text ragt seitlich aus dem Fenster.
        let rect = {
            h.settle();
            h.dialog_rect().unwrap()
        };
        let shot = h.settle();
        for t in &shot.dialog {
            if t.rect.intersects(rect) {
                assert!(
                    rect.expand(0.5).contains_rect(t.rect),
                    "„{}“ ragt aus dem Fenster {rect:?}: {:?}",
                    t.text,
                    t.rect
                );
            }
        }
        let mut all: Vec<String> = h.texts_in_dialog().into_iter().map(|t| t.text).collect();
        all.sort();
        all.dedup();
        for must in [
            "Blattangaben",
            "Projekt",
            "Dieses Blatt",
            "Änderungen",
            "Übernehmen",
            "Abbrechen",
            "Eigene Hinweise für dieses Blatt",
            "Änderung hinzufügen",
        ] {
            assert!(all.iter().any(|t| t == must), "„{must}“ fehlt: {all:?}");
        }
        for t in &all {
            for banned in [
                "Spec",
                "ISO",
                "DXF",
                "egui",
                "Format 6",
                "Datei stammt",
                "TODO",
            ] {
                assert!(!t.contains(banned), "„{t}“ enthält „{banned}“");
            }
        }
        let _ = d;
    }
}
