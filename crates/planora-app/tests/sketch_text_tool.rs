//! Skizzen 8, Task 12: das Werkzeug „Text“ (Spec C1 bis C7, C11, C12). Menü „Erzeugen“, zwei Klicks
//! für den Rahmen oder ein Klick auf den Pfad, der Abschnitt „Text“ im Skizzenfeld mit allen
//! Feldern, Vorschau ohne Schreiben, OK als ein Rückgängig-Schritt, jeder Abbruchweg ohne Rest.
//! Die ganze App läuft ohne Fenster bei 1920×1080 mit den mitgelieferten Schriften
//! ([`FontLibrary::bundled_only`], nie Systemschriften), Rohereignisse Bild für Bild.

mod common;

use eframe::App as _;
use egui::epaint::Shape;
use egui::{Event, Key, Modifiers, PointerButton, Pos2, Rect, Vec2, pos2, vec2};
use planora_app::app::{PlanoraApp, SKETCH_CREATE_ACTIONS};
use planora_app::commands;
use planora_app::formula_field::value_field_text;
use planora_app::keymap::{Action, Area, Binding, KeyChord, Keymap, SetResult};
use planora_app::shell::{STRIP_BUTTON, STRIP_TOOLS};
use planora_app::sketch_panel::{self, TOOL_ORDER};
use planora_app::sketch_state::{SketchSession, Tool};
use planora_app::sketch_text::{FontChoice, TextTool, TextToolKind, default_font, font_choices};
use planora_app::state::AppState;
use planora_app::workspace::{DocTab, Workspace};
use planora_app::{fonts, theme};
use planora_feature::{FeatureKind, ParamScope, ParamUnit, SketchPlane, StandardPlane};
use planora_i18n::{DisplayUnit, tr, with_unit};
use planora_math::DVec2;
use planora_project::config::Config;
use planora_project::library::Library;
use planora_sketch::model::{CurveId, CurveKind, Sketch};
use planora_sketch::text::{HAlign, TextParams, TextPlacement, VAlign};
use planora_sketch::text_shape::{Contour, Seg, TextShaper, TextStyle};
use planora_text::FontLibrary;
use std::sync::Arc;

const SIZE: Vec2 = vec2(1920.0, 1080.0);
const NONE: Modifiers = Modifiers::NONE;
/// Anfang und Ende einer Linie und ihre Mitte (Klick auf den Pfad).
const LINE_A: Pos2 = pos2(700.0, 450.0);
const LINE_B: Pos2 = pos2(1000.0, 450.0);
const LINE_MID: Pos2 = pos2(850.0, 450.0);
/// Gegenüberliegende Ecken des Rahmens, abseits der Linie.
const FRAME_A: Pos2 = pos2(800.0, 650.0);
const FRAME_B: Pos2 = pos2(1100.0, 760.0);
/// Freie Stelle ohne Geometrie.
const AWAY: Pos2 = pos2(1300.0, 900.0);

fn key_with(k: Key, modifiers: Modifiers) -> Event {
    Event::Key {
        key: k,
        physical_key: Some(k),
        pressed: true,
        repeat: false,
        modifiers,
    }
}

/// Strg wie unter Windows: `ctrl` und `command`.
fn ctrl() -> Modifiers {
    Modifiers::CTRL | Modifiers::COMMAND
}

fn pointer(p: Pos2, b: PointerButton, pressed: bool) -> Event {
    Event::PointerButton {
        pos: p,
        button: b,
        pressed,
        modifiers: NONE,
    }
}

/// Texte eines Bilds mit ihrer Lage.
#[derive(Default)]
struct Shot {
    texts: Vec<(String, Rect)>,
}

fn walk(shape: &Shape, s: &mut Shot) {
    match shape {
        Shape::Vec(v) => v.iter().for_each(|x| walk(x, s)),
        Shape::Text(t) => s.texts.push((
            t.galley.text().to_string(),
            Rect::from_min_size(t.pos, t.galley.size()),
        )),
        _ => {}
    }
}

impl Shot {
    fn has(&self, text: &str) -> bool {
        self.texts.iter().any(|(t, _)| t == text)
    }

    /// Der letzte (oberste) Text `text`.
    fn find(&self, text: &str) -> Pos2 {
        self.texts
            .iter()
            .rfind(|(t, _)| t == text)
            .unwrap_or_else(|| panic!("„{text}“ fehlt"))
            .1
            .center()
    }

    /// Die Lage (oben) des letzten (obersten) Textes `text`.
    fn top_of(&self, text: &str) -> f32 {
        self.texts
            .iter()
            .rfind(|(t, _)| t == text)
            .unwrap_or_else(|| panic!("„{text}“ fehlt"))
            .1
            .top()
    }
}

struct Rig {
    ctx: egui::Context,
    app: PlanoraApp,
    time: f64,
}

impl Rig {
    /// App mit offener, leerer Skizze auf XY; Schriftliste: die mitgelieferten.
    fn new() -> Rig {
        let root = common::temp_root("planora-sketch-text-tool");
        let mut w = Workspace::new(Some(Library::open(&root).unwrap()), Config::default());
        w.config_path = root.join("config.toml");
        let s = &mut w.active_tab_mut().state;
        let id = s
            .add_sketch(SketchPlane::Standard(StandardPlane::XY))
            .unwrap();
        s.open_sketch(id).unwrap();
        w.observe_history(false);
        let ctx = egui::Context::default();
        ctx.set_fonts(fonts::definitions());
        theme::apply(&ctx, true);
        PlanoraApp::setup_context(&ctx);
        let mut rig = Rig {
            ctx,
            app: PlanoraApp::for_tests(w),
            time: 0.0,
        };
        rig.settle();
        rig
    }

    fn ws(&mut self) -> &mut Workspace {
        self.app.ws_mut()
    }

    fn tab(&mut self) -> &mut DocTab {
        self.app.ws_mut().active_tab_mut()
    }

    fn st(&mut self) -> &mut AppState {
        &mut self.tab().state
    }

    fn se(&mut self) -> &mut SketchSession {
        self.st().sketch_session.as_mut().expect("Skizze offen")
    }

    fn tool(&mut self) -> &mut TextTool {
        self.se().text.as_mut().expect("Werkzeug Text läuft")
    }

    fn sk(&mut self) -> &Sketch {
        self.st().session_sketch().expect("Skizze offen")
    }

    fn scope(&mut self) -> ParamScope {
        self.st().document.param_scope()
    }

    fn frame(&mut self, events: Vec<Event>) -> Shot {
        self.time += 1.0 / 60.0;
        let raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, SIZE)),
            time: Some(self.time),
            events,
            ..Default::default()
        };
        let mut f = eframe::Frame::_new_kittest();
        let app = &mut self.app;
        let out = self.ctx.run_ui(raw, |ui| app.ui(ui, &mut f));
        let mut shot = Shot::default();
        for c in &out.shapes {
            walk(&c.shape, &mut shot);
        }
        out.drop_without_applying_deltas();
        shot
    }

    fn settle(&mut self) -> Shot {
        self.frame(vec![]);
        self.frame(vec![]);
        self.frame(vec![])
    }

    fn press_with(&mut self, k: Key, modifiers: Modifiers) -> Shot {
        self.frame(vec![key_with(k, modifiers)]);
        self.settle()
    }

    fn press(&mut self, k: Key) -> Shot {
        self.press_with(k, NONE)
    }

    fn click_with(&mut self, p: Pos2, b: PointerButton) -> Shot {
        // Kein Doppelklick mit dem vorigen Klick.
        self.time += 1.0;
        self.frame(vec![Event::PointerMoved(p)]);
        self.frame(vec![pointer(p, b, true)]);
        self.frame(vec![pointer(p, b, false)]);
        self.settle()
    }

    fn click(&mut self, p: Pos2) -> Shot {
        self.click_with(p, PointerButton::Primary)
    }

    fn right_click(&mut self, p: Pos2) -> Shot {
        self.click_with(p, PointerButton::Secondary)
    }

    /// Tippt in das Feld, das die Tastatur hat.
    fn type_text(&mut self, text: &str) -> Shot {
        self.frame(vec![Event::Text(text.to_string())]);
        self.settle()
    }

    /// Klickt den Text `label` des aktuellen Bilds an (Knopf, Umschalter, Eintrag).
    fn click_label(&mut self, label: &str) -> Shot {
        let at = self.settle().find(label);
        self.click(at)
    }

    fn action(&mut self, a: Action) -> Shot {
        commands::run_action(self.app.ws_mut(), a);
        self.settle()
    }

    fn undo_len(&mut self) -> usize {
        self.st().sketch_history().unwrap().undo_len()
    }

    fn data(&mut self) -> String {
        format!("{:?}", self.sk().to_data())
    }

    /// Eine Linie über zwei Klicks (ein Schritt); liefert sie.
    fn line(&mut self) -> CurveId {
        self.action(Action::SketchLine);
        self.click(LINE_A);
        self.click(LINE_B);
        self.press(Key::Escape);
        self.press(Key::Escape);
        let lines: Vec<CurveId> = self
            .sk()
            .curves()
            .filter(|(_, d)| matches!(d.kind, CurveKind::Line { .. }))
            .map(|(c, _)| c)
            .collect();
        assert_eq!(lines.len(), 1, "eine Linie");
        lines[0]
    }

    /// Startet „Text“ über die Aktion und zieht den Rahmen auf: Der Abschnitt ist danach offen.
    fn frame_text(&mut self) -> Shot {
        self.action(Action::SketchText);
        assert_eq!(self.se().tool, Tool::Text);
        assert!(self.tool().placing_frame(), "wartet auf den Rahmen");
        self.click(FRAME_A);
        assert!(!self.tool().section_open(), "nach einem Klick noch zu");
        self.click(FRAME_B)
    }
}

/// Hüllquader aller Punkte der Konturen (Anfang, Kontroll- und Endpunkte).
fn contour_box(contours: &[Contour]) -> (DVec2, DVec2) {
    let mut lo = DVec2::splat(f64::INFINITY);
    let mut hi = DVec2::splat(f64::NEG_INFINITY);
    let mut add = |p: DVec2| {
        lo = lo.min(p);
        hi = hi.max(p);
    };
    for c in contours {
        add(c.start);
        for s in &c.segs {
            match *s {
                Seg::Line(p) => add(p),
                Seg::Quad(a, p) => {
                    add(a);
                    add(p);
                }
                Seg::Cubic(a, b, p) => {
                    add(a);
                    add(b);
                    add(p);
                }
            }
        }
    }
    (lo, hi)
}

/// Hüllquader der Punkte des Textes der Skizze, der `content` trägt.
fn text_box(sk: &Sketch, content: &str) -> (DVec2, DVec2) {
    let (_, d) = sk.texts().find(|(_, d)| d.content == content).unwrap();
    let mut lo = DVec2::splat(f64::INFINITY);
    let mut hi = DVec2::splat(f64::NEG_INFINITY);
    for &p in &d.points {
        let q = sk.point_xy(p).unwrap();
        lo = lo.min(q);
        hi = hi.max(q);
    }
    (lo, hi)
}

fn assert_same_box(a: (DVec2, DVec2), b: (DVec2, DVec2)) {
    assert!(
        a.0.distance(b.0) < 1e-9 && a.1.distance(b.1) < 1e-9,
        "Vorschau {a:?}, angelegt {b:?}"
    );
}

/// Erzeugt die mitgelieferten Schriften, wie die App sie in Tests hat.
fn bundled() -> FontLibrary {
    FontLibrary::bundled_only()
}

#[test]
fn create_menu_has_text_entry() {
    assert_eq!(SKETCH_CREATE_ACTIONS.len(), 5);
    assert!(SKETCH_CREATE_ACTIONS.contains(&Action::SketchText));
    assert_eq!(Action::SketchText.sketch_tool(), Some(Tool::Text));
    let mut rig = Rig::new();
    let shot = rig.settle();
    let menu = shot.find(&tr!("menu-create"));
    let shot = rig.click(menu);
    let label = Action::SketchText.label();
    assert_eq!(label, tr!("action-sketch-text"));
    assert!(shot.has(&label), "eigener Eintrag im Menü „Erzeugen“");
    let at = shot.find(&label);
    rig.click(at);
    assert_eq!(rig.se().tool, Tool::Text);
    assert!(rig.tool().placing_frame());
}

#[test]
fn strip_stays_20_buttons_813px() {
    assert_eq!(STRIP_TOOLS, 20);
    assert_eq!(TOOL_ORDER.len(), 20);
    assert!(
        !TOOL_ORDER.contains(&Tool::Text),
        "wie Fusion nicht in der Leiste"
    );
    assert_eq!(planora_app::shell::STRIP_H, 813.0);
    let mut rig = Rig::new();
    rig.action(Action::SketchText);
    let buttons = sketch_panel::strip_buttons(&rig.ctx);
    assert_eq!(buttons.len(), 20);
    let (left, right) = (buttons[0].1.left(), buttons[0].1.right());
    for (name, r) in &buttons {
        assert_eq!(r.width(), STRIP_BUTTON, "{name}: Breite");
        assert_eq!((r.left(), r.right()), (left, right), "{name}: Spalte");
        assert!(r.bottom() <= SIZE.y, "{name}: im Bild, ohne Rollen: {r:?}");
    }
    let last = buttons.last().unwrap().1.bottom() - buttons[0].1.top();
    assert!(last <= 813.0, "{last}");
}

#[test]
fn two_clicks_then_ok_creates_one_undo_step() {
    let mut rig = Rig::new();
    let before = rig.data();
    let steps = rig.undo_len();
    let shot = rig.frame_text();
    assert!(rig.tool().section_open(), "nach zwei Klicks offen");
    assert!(shot.has(&tr!("sketch-text-kind-path")), "Abschnitt Text");
    assert_eq!(rig.tool().corners.len(), 2);
    // Das Textfeld hat gleich die Tastatur.
    rig.type_text("Hallo");
    assert_eq!(rig.tool().params.content, "Hallo");
    // Vorschau ohne Schreiben.
    let s = rig.st();
    let se = s.sketch_session.as_ref().unwrap();
    let preview = se.text_preview(s.session_sketch().unwrap());
    let preview = preview.expect("Vorschau");
    assert!(!preview.is_empty());
    assert_eq!(rig.data(), before, "die Vorschau schreibt nichts");
    assert_eq!(rig.undo_len(), steps);
    // Die Vorschau beginnt links unten im Rahmen (links, unten).
    let corners = rig.tool().frame_corners().unwrap();
    let lo = corners[0];
    let left = preview
        .iter()
        .map(|c| c.start.x)
        .fold(f64::INFINITY, f64::min);
    let bottom = preview
        .iter()
        .map(|c| c.start.y)
        .fold(f64::INFINITY, f64::min);
    assert!(left >= lo.x - 1e-6 && left <= lo.x + 5.0, "{left} {lo:?}");
    assert!(
        bottom >= lo.y - 1e-6 && bottom <= lo.y + 5.0,
        "{bottom} {lo:?}"
    );
    let shown = contour_box(&preview);
    rig.click_label(&tr!("common-ok"));
    assert_eq!(rig.se().tool, Tool::Select, "nach OK zurück zur Auswahl");
    let texts: Vec<_> = rig
        .sk()
        .texts()
        .map(|(_, d)| (d.content.clone(), d.placement))
        .collect();
    assert_eq!(texts.len(), 1);
    assert_eq!(texts[0].0, "Hallo");
    let TextPlacement::Box { corners: ids, .. } = texts[0].1 else {
        panic!("Text im Rahmen erwartet");
    };
    // Der Rahmen steht, wo er aufgezogen wurde, und der Text genau wie in der Vorschau.
    let at = rig.sk().point_xy(ids[0]).unwrap();
    assert!(at.distance(lo) < 1e-6, "{at:?} {lo:?}");
    assert_same_box(shown, text_box(rig.sk(), "Hallo"));
    assert_eq!(rig.undo_len(), steps + 1, "ein Rückgängig-Schritt");
    assert_eq!(
        rig.st().sketch_undo_label(),
        Some(tr!("undo-sketch-text-create"))
    );
    // Rückgängig nimmt alles zurück.
    rig.press_with(Key::Z, ctrl());
    assert_eq!(rig.data(), before);
}

#[derive(Clone, Copy, Debug)]
enum Way {
    /// Esc, während das Textfeld die Tastatur hat.
    EscInField,
    /// Esc in der Zeichnung.
    Esc,
    RightClick,
    ToolSwitch,
    CancelButton,
}

#[test]
fn esc_rightclick_toolswitch_cancel_leave_nothing() {
    for way in [
        Way::EscInField,
        Way::Esc,
        Way::RightClick,
        Way::ToolSwitch,
        Way::CancelButton,
    ] {
        let mut rig = Rig::new();
        rig.line();
        let before = rig.data();
        let steps = rig.undo_len();
        // Die erste Ecke auf dem Ende der Linie: Erst OK bände sie an.
        rig.action(Action::SketchText);
        rig.click(LINE_A);
        rig.click(FRAME_B);
        rig.type_text("Hallo");
        assert!(rig.tool().section_open(), "{way:?}");
        match way {
            Way::EscInField => {
                rig.press(Key::Escape);
            }
            Way::Esc => {
                rig.click(AWAY);
                rig.press(Key::Escape);
            }
            Way::RightClick => {
                rig.right_click(AWAY);
            }
            Way::ToolSwitch => {
                rig.action(Action::SketchLine);
            }
            Way::CancelButton => {
                rig.click_label(&tr!("common-cancel"));
            }
        }
        rig.settle();
        let expected = match way {
            Way::ToolSwitch => Tool::Line,
            _ => Tool::Select,
        };
        assert_eq!(rig.se().tool, expected, "{way:?}");
        assert!(rig.se().text.is_none(), "{way:?}: kein Werkzeugzustand");
        assert_eq!(rig.data(), before, "{way:?}: Skizze unverändert");
        assert_eq!(rig.undo_len(), steps, "{way:?}: kein Schritt");
        assert_eq!(rig.sk().texts().count(), 0, "{way:?}");
        assert!(
            !rig.settle().has(&tr!("sketch-text-kind-path")),
            "{way:?}: Abschnitt zu"
        );
    }
}

#[test]
fn empty_text_ok_rejected_with_message() {
    // Leer, nur Leerzeichen, nur ein Zeichen ohne Umriss (Ledger Task 2).
    for content in ["", "   ", "\u{1F600}"] {
        let mut rig = Rig::new();
        let before = rig.data();
        let steps = rig.undo_len();
        rig.frame_text();
        if !content.is_empty() {
            rig.type_text(content);
        }
        rig.click_label(&tr!("common-ok"));
        assert_eq!(
            rig.se().message_text(),
            Some(tr!("sketch-text-empty")),
            "{content:?}"
        );
        assert_eq!(rig.se().tool, Tool::Text, "{content:?}: Abschnitt bleibt");
        assert!(rig.tool().section_open());
        assert_eq!(rig.data(), before, "{content:?}");
        assert_eq!(rig.undo_len(), steps);
        // Die Meldung steht im Skizzenfeld.
        assert!(rig.settle().has(&tr!("sketch-text-empty")), "{content:?}");
    }
}

#[test]
fn kind_switch_to_path_waits_for_curve_click() {
    let mut rig = Rig::new();
    let line = rig.line();
    // Ein Text, an dessen Buchstaben kein Pfad geht.
    let lib = bundled();
    let params = TextParams {
        content: "I".into(),
        style: TextStyle {
            font: "Montserrat".into(),
            bold: false,
            italic: false,
            spacing: 0.0,
        },
        height: 5.0,
        flip_h: false,
        flip_v: false,
    };
    let shape = Arc::new(lib.shape("I", &params.style).unwrap());
    let (glyph, frame_line) = {
        let (_, sk) = rig.st().session_sketch_mut().unwrap();
        let t = sk
            .add_text_box(
                DVec2::new(-200.0, -200.0),
                DVec2::new(-150.0, -190.0),
                params,
                HAlign::Left,
                VAlign::Bottom,
                shape,
            )
            .unwrap();
        sk.solve();
        let d = sk.text(t).unwrap();
        let TextPlacement::Box { lines, .. } = d.placement else {
            unreachable!()
        };
        (d.curves[0], lines[0])
    };
    rig.st().observe_sketch(false);
    let steps = rig.undo_len();
    let before = rig.data();
    rig.frame_text();
    rig.type_text("Pfad");
    rig.click_label(&tr!("sketch-text-kind-path"));
    assert_eq!(rig.tool().kind, TextToolKind::Path);
    assert_eq!(rig.tool().path, None, "wartet auf den Klick auf eine Kurve");
    assert_eq!(
        rig.tool().corners.len(),
        2,
        "der Rahmen bleibt nur Vorschau"
    );
    assert!(!rig.tool().placing_frame());
    assert!(!rig.se().is_drawing_tool(), "Klick wählt, zeichnet nicht");
    {
        let s = rig.st();
        let se = s.sketch_session.as_ref().unwrap();
        assert!(se.text_preview(s.session_sketch().unwrap()).is_none());
    }
    // OK ohne Pfad: Meldung, nichts angelegt.
    rig.click_label(&tr!("common-ok"));
    assert_eq!(rig.se().message_text(), Some(tr!("sketch-text-need-path")));
    assert_eq!(rig.data(), before);
    // Buchstaben und Rahmenlinien tragen keinen Text.
    for bad in [glyph, frame_line] {
        let at = rig.sk().curve_polyline(bad, 32).unwrap()[1];
        let (se, sk) = rig.st().session_sketch_mut().unwrap();
        assert!(se.click(sk, at, 0.01, false).is_err());
        assert_eq!(se.message_text(), Some(tr!("sketch-text-bad-path")));
        assert_eq!(se.text.as_ref().unwrap().path, None);
    }
    rig.click(LINE_MID);
    assert_eq!(rig.tool().path, Some(line));
    let shown = {
        let s = rig.st();
        let se = s.sketch_session.as_ref().unwrap();
        contour_box(
            &se.text_preview(s.session_sketch().unwrap())
                .expect("Vorschau"),
        )
    };
    assert_eq!(rig.data(), before, "auch auf dem Pfad nur Vorschau");
    rig.click_label(&tr!("common-ok"));
    assert_same_box(shown, text_box(rig.sk(), "Pfad"));
    let placements: Vec<TextPlacement> = rig
        .sk()
        .texts()
        .filter(|(_, d)| d.content == "Pfad")
        .map(|(_, d)| d.placement)
        .collect();
    assert_eq!(placements.len(), 1);
    assert!(
        matches!(placements[0], TextPlacement::Path { path, above: true, fit: false, h: HAlign::Left } if path == line),
        "{:?}",
        placements[0]
    );
    assert_eq!(rig.undo_len(), steps + 1);
    // Kein Rahmen: nur die Linie und die Kurven der beiden Texte.
    let frames = rig
        .sk()
        .texts()
        .filter(|(_, d)| matches!(d.placement, TextPlacement::Box { .. }))
        .count();
    assert_eq!(frames, 1, "nur der Text von vorher hat einen Rahmen");
}

#[test]
fn fit_disables_alignment_and_spacing() {
    let mut rig = Rig::new();
    rig.line();
    rig.frame_text();
    rig.type_text("Fit");
    rig.click_label(&tr!("sketch-text-kind-path"));
    rig.click(LINE_MID);
    assert!(!rig.tool().align_locked());
    // Ohne „An Pfad anpassen“ wirkt die Ausrichtung.
    rig.click_label(&tr!("sketch-text-align-center"));
    assert_eq!(rig.tool().h, HAlign::Center);
    rig.click_label(&tr!("sketch-text-fit"));
    assert!(rig.tool().fit);
    assert!(rig.tool().align_locked());
    // Gesperrt: Klicks auf die Ausrichtung ändern nichts.
    rig.click_label(&tr!("sketch-text-align-right"));
    assert_eq!(rig.tool().h, HAlign::Center, "Ausrichtung gesperrt");
    // Im Rahmen gibt es kein „An Pfad anpassen“: nichts gesperrt.
    rig.tool().kind = TextToolKind::Box;
    assert!(!rig.tool().align_locked());
    let shot = rig.settle();
    assert!(!shot.has(&tr!("sketch-text-fit")), "nur auf Pfad");
    assert!(
        shot.has(&tr!("sketch-text-align-top")),
        "senkrecht nur im Rahmen"
    );
    rig.tool().kind = TextToolKind::Path;
    let shot = rig.settle();
    assert!(!shot.has(&tr!("sketch-text-align-top")));
    assert!(shot.has(&tr!("sketch-text-above")));
}

#[test]
fn defaults_arial_else_montserrat_5mm() {
    assert_eq!(default_font(|f| f == "Arial"), "Arial");
    assert_eq!(default_font(|_| false), "Montserrat");
    assert_eq!(default_font(|f| f == "Montserrat"), "Montserrat");
    let mut rig = Rig::new();
    rig.action(Action::SketchText);
    let t = rig.tool().clone();
    assert_eq!(t.kind, TextToolKind::Box);
    assert_eq!(
        t.params.style.font, "Montserrat",
        "kein Arial in den Testschriften"
    );
    assert!(!t.params.style.bold && !t.params.style.italic);
    assert_eq!(t.params.height, 5.0);
    assert_eq!(t.params.style.spacing, 0.0);
    assert_eq!((t.h, t.v), (HAlign::Left, VAlign::Bottom));
    assert!(!t.params.flip_h && !t.params.flip_v);
    assert!(t.above && !t.fit);
    assert_eq!(t.angle, 0.0);
    assert!(t.params.content.is_empty());
    assert_eq!(t.height_text, value_field_text(5.0, ParamUnit::Length));
}

#[test]
fn inch_document_defaults_0_2_in() {
    with_unit(DisplayUnit::Zoll, || {
        let mut rig = Rig::new();
        rig.frame_text();
        let h = DisplayUnit::Zoll.from_display(0.2);
        assert_eq!(rig.tool().params.height, h);
        assert_eq!(
            rig.tool().height_text,
            value_field_text(h, ParamUnit::Length)
        );
        assert_eq!(
            value_field_text(h, ParamUnit::Length),
            value_field_text(5.08, ParamUnit::Length)
        );
        // Getippte Zahlen zählen in Zoll.
        let scope = rig.scope();
        rig.tool().set_height_text("0.5", &scope);
        assert!((rig.tool().params.height - 12.7).abs() < 1e-9);
        rig.type_text("I");
        rig.click_label(&tr!("common-ok"));
        let heights: Vec<f64> = rig.sk().texts().map(|(_, d)| d.height).collect();
        assert_eq!(heights.len(), 1);
        assert!((heights[0] - 12.7).abs() < 1e-9);
    });
}

#[test]
fn last_values_remembered_in_session() {
    let mut rig = Rig::new();
    rig.frame_text();
    rig.type_text("A");
    let scope = rig.scope();
    rig.tool().set_height_text("8", &scope);
    rig.tool().set_spacing_text("20", &scope);
    rig.click_label(&tr!("sketch-text-bold"));
    assert!(rig.tool().params.style.bold);
    rig.click_label(&tr!("sketch-text-align-center"));
    rig.click_label(&tr!("sketch-text-align-top"));
    rig.tool().params.flip_h = true;
    rig.tool().params.style.font = "Roboto".into();
    rig.click_label(&tr!("common-ok"));
    assert_eq!(rig.sk().texts().count(), 1);
    let check = |rig: &mut Rig, when: &str| {
        let t = rig.tool().clone();
        assert!(t.params.content.is_empty(), "{when}: der Text nicht");
        assert_eq!(t.kind, TextToolKind::Box, "{when}");
        assert_eq!(t.params.style.font, "Roboto", "{when}");
        assert!(t.params.style.bold, "{when}");
        assert_eq!(t.params.height, 8.0, "{when}");
        assert_eq!(t.height_text, value_field_text(8.0, ParamUnit::Length));
        assert_eq!(t.params.style.spacing, 20.0, "{when}");
        assert_eq!((t.h, t.v), (HAlign::Center, VAlign::Top), "{when}");
        assert!(t.params.flip_h, "{when}");
        assert!(t.corners.is_empty(), "{when}");
    };
    rig.action(Action::SketchText);
    check(&mut rig, "gleiche Sitzung");
    // Abbrechen ändert die gemerkten Werte nicht.
    rig.tool().params.style.italic = true;
    rig.press(Key::Escape);
    rig.action(Action::SketchText);
    check(&mut rig, "nach Abbruch");
    // Auch nach dem Schließen und Öffnen der Skizze (Programmlauf).
    rig.press(Key::Escape);
    let id = rig.st().open_sketch_id().unwrap();
    rig.st().close_sketch();
    rig.st().open_sketch(id).unwrap();
    rig.settle();
    rig.action(Action::SketchText);
    check(&mut rig, "Skizze neu geöffnet");
}

#[test]
fn ctrl_enter_is_ok_enter_is_newline() {
    let mut rig = Rig::new();
    let steps = rig.undo_len();
    rig.frame_text();
    rig.type_text("Ha");
    rig.press(Key::Enter);
    assert_eq!(rig.tool().params.content, "Ha\n", "Enter: neue Zeile");
    assert_eq!(rig.sk().texts().count(), 0);
    rig.type_text("llo");
    rig.press_with(Key::Enter, ctrl());
    assert_eq!(rig.se().tool, Tool::Select, "Strg+Enter: OK");
    let texts: Vec<(String, usize)> = rig
        .sk()
        .texts()
        .map(|(_, d)| (d.content.clone(), d.shape().map_or(0, |s| s.lines.len())))
        .collect();
    assert_eq!(texts, [("Ha\nllo".to_string(), 2)]);
    assert_eq!(rig.undo_len(), steps + 1);
}

#[test]
fn keymap_action_skizze_text_rebindable() {
    assert_eq!(Action::from_id("skizze.text"), Some(Action::SketchText));
    assert_eq!(Action::SketchText.id(), "skizze.text");
    assert_eq!(Action::SketchText.area(), Area::Sketch);
    let km = Keymap::defaults();
    assert!(
        km.bindings(Action::SketchText).is_empty(),
        "ohne Vorgabetaste"
    );
    let mut rig = Rig::new();
    let mut km = rig.ws().keymap.clone();
    let chord = KeyChord {
        key: Key::F9,
        ctrl: false,
        shift: false,
        alt: false,
    };
    assert_eq!(
        km.set(Action::SketchText, 0, Binding::Key(chord)),
        SetResult::Ok
    );
    rig.ws().keymap = km;
    rig.press(Key::F9);
    assert_eq!(rig.se().tool, Tool::Text);
}

#[test]
fn undo_while_text_section_open_cancels_first() {
    // Review Focus 5: Strg+Z bei offenem Abschnitt bricht ihn ab und nimmt den letzten Schritt
    // zurück.
    let mut rig = Rig::new();
    rig.line();
    let steps = rig.undo_len();
    assert_eq!(steps, 1);
    rig.frame_text();
    rig.type_text("Hallo");
    // Das Textfeld gibt die Tastatur ab (Klick daneben), dann Strg+Z.
    rig.click(AWAY);
    assert!(rig.tool().section_open());
    rig.press_with(Key::Z, ctrl());
    assert_eq!(rig.undo_len(), 0, "die Linie ist zurückgenommen");
    assert_eq!(rig.sk().curves().count(), 0);
    assert_eq!(rig.sk().texts().count(), 0);
    let t = rig.se().text.clone();
    let open = t.is_some_and(|t| t.section_open() || !t.params.content.is_empty());
    assert!(!open, "der Abschnitt ist abgebrochen");
    assert!(!rig.settle().has(&tr!("sketch-text-kind-path")));
    // Wiederholen bringt nur die Linie zurück.
    rig.press_with(Key::Y, ctrl());
    assert_eq!(rig.sk().curves().count(), 1);
    assert_eq!(rig.sk().texts().count(), 0);
}

#[test]
fn save_while_section_open_writes_without_preview() {
    let mut rig = Rig::new();
    rig.line();
    rig.frame_text();
    rig.type_text("Vorschau");
    assert!(rig.tool().section_open());
    let sketch = rig.st().open_sketch_id().unwrap();
    let p = rig
        .ws()
        .library
        .as_ref()
        .unwrap()
        .create_project("P")
        .unwrap_or_else(|_| vec!["P".to_string()]);
    rig.ws().save_as(&p, "Teil", "").unwrap();
    rig.settle();
    let s = rig.st();
    let FeatureKind::Sketch(def) = &s.document.feature(sketch).unwrap().kind else {
        panic!("Skizze erwartet");
    };
    assert_eq!(
        def.sketch.texts().count(),
        0,
        "ohne die Vorschau gespeichert"
    );
    assert_eq!(def.sketch.curves().count(), 1, "die Linie ist gespeichert");
    let snapshot = format!("{:?}", s.snapshot());
    assert!(!snapshot.contains("Vorschau"));
    assert!(!rig.tab().is_dirty(), "gespeichert");
}

#[test]
fn font_list_shows_missing_font_marked() {
    let families = vec![
        "Montserrat".to_string(),
        "Roboto".to_string(),
        "Arial".into(),
    ];
    let missing = "Gibtsnicht Grotesk";
    let list = font_choices(&families, missing, false, "");
    assert_eq!(
        list[0],
        FontChoice {
            family: missing.into(),
            label: tr!("sketch-text-font-missing", font = missing.to_string()),
        }
    );
    let names: Vec<&str> = list[1..].iter().map(|c| c.family.as_str()).collect();
    assert_eq!(
        names,
        ["Montserrat", "Roboto", "Arial"],
        "Reihenfolge der Liste"
    );
    // Vorhandene Schrift: nichts markiert; Suche ohne Groß/Klein.
    let list = font_choices(&families, "Roboto", true, "");
    assert_eq!(list.len(), 3);
    assert!(list.iter().all(|c| c.label == c.family));
    let list = font_choices(&families, "Roboto", true, "rob");
    assert_eq!(
        list.iter().map(|c| c.family.as_str()).collect::<Vec<_>>(),
        ["Roboto"]
    );

    let mut rig = Rig::new();
    rig.frame_text();
    rig.tool().params.style.font = missing.into();
    let marked = tr!("sketch-text-font-missing", font = missing.to_string());
    let shot = rig.settle();
    assert!(
        shot.has(&marked),
        "die gewählte fehlende Schrift steht markiert da"
    );
    let shot = rig.click(shot.find(&marked));
    // Offene Liste: die fehlende oben, dann die mitgelieferten.
    let (top_missing, top_m, top_r) = (
        shot.top_of(&marked),
        shot.top_of("Montserrat"),
        shot.top_of("Roboto"),
    );
    assert!(
        top_missing < top_m && top_m < top_r,
        "{top_missing} {top_m} {top_r}"
    );
    // Suche: das Suchfeld hat die Tastatur.
    let shot = rig.type_text("rob");
    assert!(!shot.has("Montserrat"), "gefiltert");
    let shot = rig.click(shot.find("Roboto"));
    assert_eq!(rig.tool().params.style.font, "Roboto");
    assert!(!shot.has(&marked));
}

#[test]
fn angle_field_turns_frame_in_one_step() {
    let mut rig = Rig::new();
    let steps = rig.undo_len();
    rig.frame_text();
    rig.type_text("Dreh");
    let scope = rig.scope();
    rig.tool().set_angle_text("30", &scope);
    assert!((rig.tool().angle - 30f64.to_radians()).abs() < 1e-12);
    let shot = rig.settle();
    assert!(
        shot.has(&tr!("sketch-text-angle-label")),
        "Winkel im Rahmen"
    );
    let shown = {
        let s = rig.st();
        let se = s.sketch_session.as_ref().unwrap();
        contour_box(
            &se.text_preview(s.session_sketch().unwrap())
                .expect("Vorschau"),
        )
    };
    let c0 = rig.tool().frame_corners().unwrap()[0];
    rig.click_label(&tr!("common-ok"));
    let (t, _) = rig.sk().texts().next().unwrap();
    let angle = rig.sk().text_angle(t).unwrap();
    assert!((angle - 30f64.to_radians()).abs() < 1e-9, "{angle}");
    let TextPlacement::Box { corners, .. } = rig.sk().text(t).unwrap().placement else {
        panic!("Rahmen erwartet");
    };
    assert!(
        rig.sk().point_xy(corners[0]).unwrap().distance(c0) < 1e-9,
        "um Ecke 0"
    );
    assert_same_box(shown, text_box(rig.sk(), "Dreh"));
    assert_eq!(rig.undo_len(), steps + 1, "Anlegen und Drehen: ein Schritt");
    // Auf dem Pfad gibt es kein Winkelfeld.
    rig.frame_text();
    rig.click_label(&tr!("sketch-text-kind-path"));
    assert!(!rig.settle().has(&tr!("sketch-text-angle-label")));
}

#[test]
fn frame_click_on_point_binds_coincident() {
    use planora_sketch::constraint::ConstraintKind as K;
    let mut rig = Rig::new();
    let line = rig.line();
    let CurveKind::Line { a, .. } = rig.sk().curve(line).unwrap().kind else {
        unreachable!()
    };
    rig.action(Action::SketchText);
    rig.click(LINE_A);
    rig.click(FRAME_B);
    rig.type_text("Fang");
    rig.click_label(&tr!("common-ok"));
    let (t, d) = rig.sk().texts().next().unwrap();
    let TextPlacement::Box { corners, .. } = d.placement else {
        panic!("Rahmen erwartet");
    };
    let bound: Vec<_> = rig
        .sk()
        .constraints()
        .filter(|(_, k)| match k.kind {
            K::Coincident(p, q) => {
                (p == a || q == a) && (corners.contains(&p) || corners.contains(&q))
            }
            _ => false,
        })
        .collect();
    assert_eq!(
        bound.len(),
        1,
        "eine Ecke liegt fest auf dem Ende der Linie"
    );
    assert!(rig.sk().text(t).is_some());
}
