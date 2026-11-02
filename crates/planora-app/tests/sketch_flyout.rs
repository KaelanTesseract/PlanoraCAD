//! Dreieck und Liste der Arten an der Skizzenleiste, Einträge im Menü „Erzeugen“ (Skizzen 3):
//! ganze App bei 1920×1080, Rohereignisse Bild für Bild. Die Zeit zählt der Prüfstand selbst, damit
//! Halten und Doppelklick genau messbar sind.

mod common;

use eframe::App as _;
use egui::epaint::Shape;
use egui::{Color32, Event, Key, Modifiers, PointerButton, Pos2, Rect, Vec2, pos2, vec2};
use planora_app::app::PlanoraApp;
use planora_app::keymap::{Action, Binding, KeyChord, SetResult};
use planora_app::sketch_kinds::ToolKind;
use planora_app::sketch_panel::{self, TOOL_ORDER};
use planora_app::sketch_state::{SketchSession, Tool};
use planora_app::state::AppState;
use planora_app::workspace::Workspace;
use planora_app::{fonts, theme};
use planora_feature::{SketchPlane, StandardPlane};
use planora_project::config::Config;
use planora_project::library::Library;

const SIZE: Vec2 = vec2(1920.0, 1080.0);
/// Freie Stellen der Zeichnung, abseits von Leiste und Feldern.
const A: Pos2 = pos2(800.0, 500.0);
const B: Pos2 = pos2(1000.0, 500.0);
const C: Pos2 = pos2(1000.0, 650.0);

struct Seen {
    text: String,
    rect: Rect,
    color: Color32,
}

#[derive(Default)]
struct Shot {
    texts: Vec<Seen>,
    /// Gefüllte Dreiecke (Eckpunkte, Füllung).
    triangles: Vec<(Vec<Pos2>, Color32)>,
    /// Jedes gezeigte Textstück mit der Schriftfamilie, in der es gesetzt ist.
    runs: Vec<(String, egui::FontFamily)>,
}

fn walk(shape: &Shape, s: &mut Shot) {
    match shape {
        Shape::Vec(v) => v.iter().for_each(|x| walk(x, s)),
        Shape::Text(t) => {
            let section = t.galley.job.sections.first();
            let mut color = section.map_or(t.fallback_color, |x| x.format.color);
            if color == Color32::PLACEHOLDER {
                color = t.fallback_color;
            }
            if let Some(c) = t.override_text_color {
                color = c;
            }
            let whole = t.galley.text();
            for sec in &t.galley.job.sections {
                // Unsichtbarer Text (der Platzhalter für das Änderungszeichen im Tab) zählt nicht.
                if sec.format.color == Color32::TRANSPARENT {
                    continue;
                }
                if let Some(run) = whole.get(sec.byte_range.start.0..sec.byte_range.end.0) {
                    s.runs
                        .push((run.to_string(), sec.format.font_id.family.clone()));
                }
            }
            s.texts.push(Seen {
                text: t.galley.text().to_string(),
                rect: Rect::from_min_size(t.pos, t.galley.size()),
                color,
            });
        }
        Shape::Path(p) if p.points.len() == 3 && p.closed && p.fill != Color32::TRANSPARENT => {
            s.triangles.push((p.points.clone(), p.fill));
        }
        _ => {}
    }
}

impl Shot {
    fn has(&self, exact: &str) -> bool {
        self.texts.iter().any(|t| t.text == exact)
    }

    fn find(&self, exact: &str) -> &Seen {
        self.texts
            .iter()
            .find(|t| t.text == exact)
            .unwrap_or_else(|| panic!("„{exact}“ fehlt"))
    }
}

fn pointer(p: Pos2, b: PointerButton, pressed: bool) -> Event {
    Event::PointerButton {
        pos: p,
        button: b,
        pressed,
        modifiers: Modifiers::NONE,
    }
}

fn key(k: Key) -> Event {
    Event::Key {
        key: k,
        physical_key: Some(k),
        pressed: true,
        repeat: false,
        modifiers: Modifiers::NONE,
    }
}

struct Rig {
    ctx: egui::Context,
    app: PlanoraApp,
    time: f64,
}

impl Rig {
    fn plain() -> Rig {
        let root = common::temp_root("planora-sketch-flyout");
        let w = Workspace::new(Some(Library::open(&root).unwrap()), Config::default());
        let ctx = egui::Context::default();
        ctx.set_fonts(fonts::definitions());
        theme::apply(&ctx, true);
        PlanoraApp::setup_context(&ctx);
        ctx.all_styles_mut(|s| s.interaction.tooltip_delay = 0.0);
        Rig {
            ctx,
            app: PlanoraApp::for_tests(w),
            time: 0.0,
        }
    }

    /// Leere Skizze auf XY, offen und einmal gezeichnet.
    fn new() -> Rig {
        let mut rig = Rig::plain();
        let id = rig
            .st()
            .add_sketch(SketchPlane::Standard(StandardPlane::XY))
            .unwrap();
        rig.st().open_sketch(id).unwrap();
        rig.settle();
        rig
    }

    fn st(&mut self) -> &mut AppState {
        &mut self.app.ws_mut().active_tab_mut().state
    }

    fn se(&mut self) -> &SketchSession {
        self.st().sketch_session.as_ref().expect("Skizze offen")
    }

    fn set_tool(&mut self, tool: Tool) {
        let (se, sk) = self.st().session_sketch_mut().unwrap();
        se.set_tool(sk, tool);
        self.settle();
    }

    fn curves(&mut self) -> usize {
        self.st().session_sketch().unwrap().curves().count()
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

    /// Lässt `secs` Sekunden vergehen, ohne die Maus zu bewegen.
    fn wait(&mut self, secs: f64) {
        let until = self.time + secs;
        while self.time < until {
            self.frame(vec![]);
        }
    }

    fn click_with(&mut self, p: Pos2, b: PointerButton) -> Shot {
        self.frame(vec![Event::PointerMoved(p)]);
        self.frame(vec![pointer(p, b, true)]);
        self.frame(vec![pointer(p, b, false)]);
        self.settle()
    }

    fn click(&mut self, p: Pos2) -> Shot {
        // Kein Doppelklick mit dem vorigen Klick.
        self.time += 1.0;
        self.click_with(p, PointerButton::Primary)
    }

    fn right_click(&mut self, p: Pos2) -> Shot {
        self.time += 1.0;
        self.click_with(p, PointerButton::Secondary)
    }

    fn press(&mut self, k: Key) -> Shot {
        self.frame(vec![key(k)]);
        self.settle()
    }

    fn strip_rect(&self, label: &str) -> Rect {
        sketch_panel::strip_buttons(&self.ctx)
            .into_iter()
            .find(|(l, _)| l == label)
            .unwrap_or_else(|| panic!("Werkzeug „{label}“ fehlt"))
            .1
    }

    fn strip(&self, label: &str) -> Pos2 {
        self.strip_rect(label).center()
    }

    fn open(&self) -> bool {
        sketch_panel::flyout_open(&self.ctx)
    }

    fn entry(&self, label: &str) -> Pos2 {
        sketch_panel::flyout_entries(&self.ctx)
            .into_iter()
            .find(|(l, _)| l == label)
            .unwrap_or_else(|| panic!("Eintrag „{label}“ fehlt"))
            .1
            .center()
    }

    fn names(&self) -> Vec<String> {
        sketch_panel::flyout_entries(&self.ctx)
            .into_iter()
            .map(|(l, _)| l)
            .collect()
    }

    /// Hält den Knopf `p` `secs` Sekunden gedrückt und lässt los.
    fn hold(&mut self, p: Pos2, secs: f64) {
        self.time += 1.0;
        self.frame(vec![Event::PointerMoved(p)]);
        self.frame(vec![pointer(p, PointerButton::Primary, true)]);
        self.wait(secs);
        self.frame(vec![pointer(p, PointerButton::Primary, false)]);
        self.settle();
    }

    /// Linienwerkzeug mit einem laufenden Linienzug (eine Linie, letzter Punkt offen).
    fn running_chain(&mut self) {
        self.set_tool(Tool::Line);
        self.click(A);
        self.click(B);
        assert_eq!(self.curves(), 1);
        assert_eq!(self.se().pending.len(), 1, "Linienzug läuft");
    }
}

#[test]
fn only_shapes_with_kinds_have_a_triangle() {
    for tool in TOOL_ORDER {
        let expected = matches!(
            tool,
            Tool::Rectangle | Tool::Circle | Tool::Arc | Tool::Polygon | Tool::Slot
        );
        assert_eq!(sketch_panel::has_flyout(tool), expected, "{tool:?}");
    }
    let mut rig = Rig::new();
    let shot = rig.settle();
    for (tool, label, _) in sketch_panel::tools() {
        let r = rig.strip_rect(&label);
        let corner = Rect::from_min_max(r.center(), r.max);
        let found: Vec<_> = shot
            .triangles
            .iter()
            .filter(|(pts, _)| pts.iter().all(|p| corner.contains(*p)))
            .collect();
        if !sketch_panel::has_flyout(tool) {
            assert!(found.is_empty(), "{label}: kein Dreieck");
            continue;
        }
        assert_eq!(found.len(), 1, "{label}: ein Dreieck unten rechts");
        let (pts, fill) = found[0];
        let size = Rect::from_points(pts).size();
        assert_eq!(size, Vec2::splat(5.0), "{label}: 5 px");
        // Farbe wie das Symbol: nicht gewählt ist das `text_secondary`.
        assert_eq!(*fill, theme::palette(true).text_secondary, "{label}");
    }
}

#[test]
fn left_click_chooses_the_last_kind_and_opens_nothing() {
    let mut rig = Rig::new();
    let p = rig.strip("Rechteck");
    rig.click(p);
    assert_eq!(rig.se().tool, Tool::Rectangle);
    assert_eq!(rig.se().kind_of(Tool::Rectangle), Some(ToolKind::Rect2P));
    assert!(!rig.open());
    assert!(rig.names().is_empty());
}

#[test]
fn right_click_opens_all_kinds_beside_the_button_with_keys() {
    let mut rig = Rig::new();
    for tool in TOOL_ORDER
        .into_iter()
        .filter(|t| sketch_panel::has_flyout(*t))
    {
        let label = sketch_panel::tool_label(tool);
        let button = rig.strip_rect(&label);
        let shot = rig.right_click(button.center());
        assert!(rig.open(), "{label}: Liste offen");
        assert_eq!(rig.se().tool, Tool::Select, "{label}: Werkzeug bleibt");
        let expected: Vec<String> = ToolKind::of(tool).iter().map(|k| k.label()).collect();
        assert_eq!(rig.names(), expected, "{label}");
        for (name, rect) in sketch_panel::flyout_entries(&rig.ctx) {
            assert!(shot.has(&name), "{name}: Name zu sehen");
            assert!(
                rect.left() >= button.right() - 0.5,
                "{name}: rechts daneben"
            );
            assert!(
                Rect::from_min_size(Pos2::ZERO, SIZE).contains_rect(rect),
                "{name}: im Fenster: {rect:?}"
            );
            assert!(rect.height() < 30.0, "{name}: eine Zeile: {rect:?}");
        }
        rig.press(Key::Escape);
        assert!(!rig.open(), "{label}: Esc schließt");
    }
    // Das Kürzel der Art steht in ihrer Zeile.
    let p = rig.strip("Rechteck");
    let shot = rig.right_click(p);
    let row = sketch_panel::flyout_entries(&rig.ctx)[0].1;
    assert!(
        row.contains(shot.find("R").rect.center()),
        "Taste R in der Zeile"
    );
}

#[test]
fn a_rebound_key_shows_in_the_list() {
    let mut rig = Rig::new();
    let mut km = rig.app.ws_mut().keymap.clone();
    let chord = KeyChord {
        key: Key::J,
        ctrl: false,
        shift: false,
        alt: false,
    };
    assert_eq!(
        km.set(Action::SketchRect3P, 0, Binding::Key(chord)),
        SetResult::Ok
    );
    rig.app.ws_mut().keymap = km;
    rig.settle();
    let p = rig.strip("Rechteck");
    let shot = rig.right_click(p);
    let row = sketch_panel::flyout_entries(&rig.ctx)[1].clone();
    assert_eq!(row.0, "3-Punkt-Rechteck");
    assert!(
        row.1.contains(shot.find("J").rect.center()),
        "Taste J in der Zeile"
    );
}

#[test]
fn picking_an_entry_sets_kind_and_tool_and_the_icon_keeps_it() {
    let mut rig = Rig::new();
    let p = rig.strip("Rechteck");
    rig.right_click(p);
    let e = rig.entry("3-Punkt-Rechteck");
    rig.click(e);
    assert_eq!(rig.se().tool, Tool::Rectangle);
    assert_eq!(rig.se().kind_of(Tool::Rectangle), Some(ToolKind::Rect3P));
    assert!(!rig.open(), "Liste zu");
    assert_eq!(rig.curves(), 0, "Klick in die Liste zeichnet nichts");
    assert!(rig.se().pending_pos.is_empty());
    // Später nimmt der Klick auf das Symbol dieselbe Art.
    let line = rig.strip("Linie");
    rig.click(line);
    assert_eq!(rig.se().tool, Tool::Line);
    rig.click(p);
    assert_eq!(rig.se().tool, Tool::Rectangle);
    assert_eq!(rig.se().kind_of(Tool::Rectangle), Some(ToolKind::Rect3P));
    assert!(!rig.open());
}

#[test]
fn holding_opens_the_list_and_the_release_is_no_click() {
    let mut rig = Rig::new();
    let p = rig.strip("Kreis");
    rig.hold(p, 0.5);
    assert!(rig.open(), "Liste bleibt nach dem Loslassen offen");
    assert_eq!(rig.se().tool, Tool::Select, "nicht umgeschaltet");
    assert_eq!(rig.names().len(), 5);
    // Danach zählt ein gewöhnlicher Klick wieder (und die Liste geht zu).
    rig.press(Key::Escape);
    rig.click(p);
    assert_eq!(rig.se().tool, Tool::Circle);
    assert!(!rig.open());
}

#[test]
fn a_short_press_is_an_ordinary_click() {
    let mut rig = Rig::new();
    let p = rig.strip("Kreis");
    rig.hold(p, 0.2);
    assert!(!rig.open(), "0,2 s öffnet nichts");
    assert_eq!(rig.se().tool, Tool::Circle, "0,2 s ist ein Klick");
}

#[test]
fn the_list_opens_while_holding_without_mouse_movement() {
    let mut rig = Rig::new();
    let p = rig.strip("Kreis");
    rig.frame(vec![Event::PointerMoved(p)]);
    rig.frame(vec![pointer(p, PointerButton::Primary, true)]);
    rig.wait(0.45);
    assert!(rig.open(), "schon vor dem Loslassen offen");
    rig.frame(vec![pointer(p, PointerButton::Primary, false)]);
}

#[test]
fn esc_closes_only_the_list_and_the_next_esc_cancels() {
    let mut rig = Rig::new();
    rig.running_chain();
    let p = rig.strip("Rechteck");
    rig.right_click(p);
    assert!(rig.open());
    assert_eq!(rig.se().pending.len(), 1, "Liste öffnen lässt den Zug");
    rig.press(Key::Escape);
    assert!(!rig.open(), "Liste zu");
    assert_eq!(rig.se().tool, Tool::Line);
    assert_eq!(rig.se().pending.len(), 1, "Linienzug läuft weiter");
    rig.press(Key::Escape);
    assert!(rig.se().pending.is_empty(), "zweites Esc bricht ab");
}

#[test]
fn esc_with_the_keyboard_in_a_draw_field_closes_only_the_list() {
    let mut rig = Rig::new();
    rig.running_chain();
    let p = rig.strip("Rechteck");
    rig.right_click(p);
    assert!(rig.open());
    // Das Feld am Zeiger hat die Tastatur, das Getippte landet dort.
    rig.frame(vec![Event::PointerMoved(C)]);
    rig.frame(vec![Event::Text("5".into())]);
    rig.settle();
    assert!(planora_app::viewport::draw_field_has_keyboard(&rig.ctx));
    assert!(rig.open(), "Tippen schließt die Liste nicht");
    rig.press(Key::Escape);
    assert!(!rig.open());
    assert_eq!(rig.se().pending.len(), 1, "Linienzug läuft weiter");
}

#[test]
fn enter_with_an_open_list_sets_no_point_and_ends_nothing() {
    let mut rig = Rig::new();
    rig.running_chain();
    let p = rig.strip("Rechteck");
    rig.right_click(p);
    rig.frame(vec![Event::PointerMoved(C)]);
    rig.press(Key::Enter);
    assert_eq!(rig.curves(), 1, "kein Punkt");
    assert_eq!(rig.se().pending.len(), 1, "Linienzug läuft weiter");
    assert!(rig.open());
    // Auch mit getipptem Wert im Feld am Zeiger.
    rig.frame(vec![Event::Text("5".into())]);
    rig.settle();
    rig.press(Key::Enter);
    assert_eq!(rig.curves(), 1, "kein Punkt aus dem Feld");
    assert_eq!(rig.se().pending.len(), 1, "Linienzug läuft weiter");
}

#[test]
fn a_press_in_the_drawing_closes_the_list_and_sets_no_point() {
    let mut rig = Rig::new();
    rig.running_chain();
    let p = rig.strip("Rechteck");
    rig.right_click(p);
    rig.frame(vec![Event::PointerMoved(C)]);
    rig.frame(vec![pointer(C, PointerButton::Primary, true)]);
    rig.settle();
    assert!(!rig.open(), "schon das Drücken schließt");
    rig.frame(vec![pointer(C, PointerButton::Primary, false)]);
    rig.settle();
    assert_eq!(rig.curves(), 1, "kein Punkt");
    assert_eq!(rig.se().pending.len(), 1, "Linienzug läuft weiter");
    // Der nächste Klick zählt, auch gleich danach an derselben Stelle: kein Doppelklick.
    rig.click_with(C, PointerButton::Primary);
    assert_eq!(rig.curves(), 2, "der nächste Klick setzt den Punkt");
    assert_eq!(rig.se().pending.len(), 1, "Linienzug läuft weiter");
}

#[test]
fn a_right_press_in_the_drawing_closes_only_the_list() {
    let mut rig = Rig::new();
    rig.running_chain();
    let p = rig.strip("Rechteck");
    rig.right_click(p);
    rig.right_click(C);
    assert!(!rig.open());
    assert_eq!(rig.se().pending.len(), 1, "Linienzug läuft weiter");
}

#[test]
fn a_drag_from_the_press_that_closes_the_list_starts_nothing() {
    let mut rig = Rig::new();
    let p = rig.strip("Rechteck");
    rig.right_click(p);
    rig.frame(vec![Event::PointerMoved(A)]);
    rig.frame(vec![pointer(A, PointerButton::Primary, true)]);
    rig.frame(vec![Event::PointerMoved(B)]);
    rig.frame(vec![Event::PointerMoved(C)]);
    assert!(rig.se().box_select.is_none(), "kein Rahmen");
    rig.frame(vec![pointer(C, PointerButton::Primary, false)]);
    rig.settle();
    assert!(!rig.open());
}

#[test]
fn key_l_with_an_open_list_chooses_the_line_and_closes_it() {
    let mut rig = Rig::new();
    let p = rig.strip("Rechteck");
    rig.right_click(p);
    assert!(rig.open());
    rig.press(Key::L);
    assert_eq!(rig.se().tool, Tool::Line);
    assert!(!rig.open(), "Liste zu");
}

#[test]
fn a_press_on_another_button_closes_the_list() {
    let mut rig = Rig::new();
    let p = rig.strip("Rechteck");
    rig.right_click(p);
    let other = rig.strip("Kreis");
    rig.right_click(other);
    assert!(rig.open(), "Liste des anderen Knopfs");
    assert_eq!(rig.names().len(), 5);
    let line = rig.strip("Linie");
    rig.click(line);
    assert!(!rig.open());
    assert_eq!(rig.se().tool, Tool::Line);
}

#[test]
fn typing_in_a_draw_field_opens_nothing() {
    let mut rig = Rig::new();
    rig.running_chain();
    rig.frame(vec![Event::PointerMoved(C)]);
    rig.frame(vec![Event::Text("5".into())]);
    rig.settle();
    assert!(!rig.open(), "Tippen öffnet nichts");
}

#[test]
fn the_list_speaks_english() {
    let en = planora_i18n::english();
    let (names, keys) = planora_i18n::with_lang(&en, || {
        let mut rig = Rig::new();
        let p = rig.strip("Rectangle");
        let shot = rig.right_click(p);
        (rig.names(), shot.has("R"))
    });
    assert_eq!(
        names,
        ["2-Point Rectangle", "3-Point Rectangle", "Center Rectangle"]
    );
    assert!(keys);
}

#[test]
fn dark_and_light_use_their_own_palette() {
    let mut seen = Vec::new();
    for dark in [true, false] {
        let mut rig = Rig::new();
        rig.st().dark = dark;
        theme::apply(&rig.ctx, dark);
        rig.settle();
        let p = rig.strip("Rechteck");
        rig.right_click(p);
        // Die Liste blendet ein; danach gilt die volle Farbe.
        rig.wait(0.5);
        let shot = rig.settle();
        let color = shot.find("3-Punkt-Rechteck").color;
        let palette = theme::palette(dark);
        assert!(
            [palette.text_primary, palette.text_secondary].contains(&color),
            "{dark}: {color:?}"
        );
        seen.push(color);
        // Das Dreieck folgt dem Thema ebenso.
        assert!(
            shot.triangles
                .iter()
                .any(|(_, fill)| *fill == palette.text_secondary),
            "{dark}: Dreieck"
        );
    }
    assert_ne!(seen[0], seen[1]);
}

#[test]
fn another_kind_of_the_same_shape_drops_positions_the_same_kind_keeps_them() {
    let mut rig = Rig::new();
    let (se, sk) = rig.st().session_sketch_mut().unwrap();
    se.choose_kind(sk, ToolKind::Rect3P);
    rig.settle();
    rig.click(A);
    assert_eq!(rig.se().pending_pos.len(), 1, "erste Lage gesetzt");
    let p = rig.strip("Rechteck");
    // Gleiche Art: Lagen bleiben.
    rig.right_click(p);
    assert_eq!(rig.se().pending_pos.len(), 1, "Öffnen lässt die Lage");
    let e = rig.entry("3-Punkt-Rechteck");
    rig.click(e);
    assert_eq!(rig.se().pending_pos.len(), 1, "gleiche Art: Lagen bleiben");
    // Andere Art: Lagen weg.
    rig.right_click(p);
    let e = rig.entry("Mittelpunkt-Rechteck");
    rig.click(e);
    assert!(rig.se().pending_pos.is_empty(), "andere Art: Lagen weg");
    assert_eq!(
        rig.se().kind_of(Tool::Rectangle),
        Some(ToolKind::RectCenter)
    );
}

#[test]
fn the_create_menu_lists_all_kinds_while_a_sketch_is_open() {
    let mut rig = Rig::new();
    let mut count = 0;
    for tool in TOOL_ORDER
        .into_iter()
        .filter(|t| sketch_panel::has_flyout(*t))
    {
        let label = sketch_panel::tool_label(tool);
        let shot = rig.settle();
        let menu = shot.find("Erzeugen").rect.center();
        let shot = rig.click(menu);
        let sub = shot
            .texts
            .iter()
            .rfind(|t| t.text == label)
            .unwrap_or_else(|| panic!("Untermenü „{label}“ fehlt"))
            .rect
            .center();
        let shot = rig.click(sub);
        for kind in ToolKind::of(tool) {
            let name = kind.label();
            assert!(shot.has(&name), "{name} im Untermenü");
            count += 1;
        }
        if tool == Tool::Rectangle {
            let y = shot.find("2-Punkt-Rechteck").rect.center().y;
            let key = shot.find("R").rect.center().y;
            assert!((y - key).abs() < 4.0, "Taste R in der Zeile");
        }
        // Der letzte Eintrag wird gewählt und schließt das ganze Menü.
        let last = ToolKind::of(tool).last().unwrap();
        let at = shot.find(&last.label()).rect.center();
        let shot = rig.click(at);
        assert_eq!(rig.se().tool, tool);
        assert_eq!(rig.se().kind_of(tool), Some(*last), "wie in der Liste");
        assert!(!shot.has(&last.label()), "Menü zu");
    }
    assert_eq!(count, 19);
}

/// Hat die Schrift in ihrer Zeichentabelle einen Eintrag für `c`? (Wie in tests/icons.rs.)
fn font_has_char(bytes: &[u8], c: char) -> bool {
    use skrifa::MetadataProvider;
    skrifa::FontRef::new(bytes)
        .expect("gültige Schriftdatei")
        .charmap()
        .map(c)
        .is_some()
}

/// Kein gezeigter Text enthält ein Zeichen, das keine Schrift der Familie hat (es erschiene als
/// „?“ oder Kästchen).
fn assert_no_missing_glyphs(shot: &Shot, what: &str) {
    let defs = fonts::definitions();
    for (run, family) in &shot.runs {
        let chain = &defs.families[family];
        for c in run.chars().filter(|c| !c.is_whitespace()) {
            let found = chain
                .iter()
                .any(|name| font_has_char(&defs.font_data[name].font, c));
            assert!(
                found,
                "{what}: {c:?} (U+{:04X}) in „{run}“ fehlt in {family:?}",
                c as u32
            );
        }
    }
}

#[test]
fn the_submenu_arrows_of_the_create_menu_use_a_glyph_the_fonts_have() {
    let mut rig = Rig::new();
    let shot = rig.settle();
    let menu = shot.find("Erzeugen").rect.center();
    let shot = rig.click(menu);
    assert_no_missing_glyphs(&shot, "Menü Erzeugen");
    // Alle fünf Untermenüs mit Pfeil stehen im offenen Menü.
    let arrow = planora_app::icons::glyph(planora_app::icons::Icon::PathSeparator).to_string();
    let arrows = shot.texts.iter().filter(|t| t.text == arrow).count();
    assert_eq!(arrows, 5, "ein Pfeil je Untermenü");
    // Auch mit offenem Untermenü.
    let sub = shot
        .texts
        .iter()
        .rfind(|t| t.text == "Rechteck")
        .expect("Untermenü Rechteck")
        .rect
        .center();
    let shot = rig.click(sub);
    assert_no_missing_glyphs(&shot, "Untermenü Rechteck");
}

#[test]
fn without_a_sketch_the_create_menu_has_no_kinds() {
    let mut rig = Rig::plain();
    let shot = rig.settle();
    let menu = shot.find("Erzeugen").rect.center();
    let shot = rig.click(menu);
    assert!(shot.has("Quader …"), "Menü offen");
    for tool in [Tool::Rectangle, Tool::Polygon, Tool::Slot] {
        assert!(!shot.has(&sketch_panel::tool_label(tool)), "{tool:?}");
    }
}

#[test]
fn hovering_a_button_names_the_last_used_kind() {
    let mut rig = Rig::new();
    let km = rig.app.ws_mut().keymap.clone();
    let title = |rig: &mut Rig, tool| {
        let se = rig.st().sketch_session.as_ref().unwrap();
        sketch_panel::strip_title(&km, se, tool)
    };
    assert_eq!(title(&mut rig, Tool::Rectangle), "2-Punkt-Rechteck (R)");
    assert_eq!(title(&mut rig, Tool::Line), "Linie (L)");
    assert_eq!(title(&mut rig, Tool::Select), "Auswahl");
    let (se, sk) = rig.st().session_sketch_mut().unwrap();
    se.choose_kind(sk, ToolKind::Rect3P);
    assert_eq!(title(&mut rig, Tool::Rectangle), "3-Punkt-Rechteck");
    assert_eq!(title(&mut rig, Tool::Polygon), "Umbeschriebenes Polygon");
    rig.settle();
    let p = rig.strip("Rechteck");
    rig.frame(vec![Event::PointerMoved(p)]);
    rig.frame(vec![Event::PointerMoved(p)]);
    let shot = rig.frame(vec![]);
    assert!(shot.has("3-Punkt-Rechteck"), "Name der Art im Hinweistext");
    assert!(shot.has(&ToolKind::Rect3P.hint()), "Hinweis der Art");
}
