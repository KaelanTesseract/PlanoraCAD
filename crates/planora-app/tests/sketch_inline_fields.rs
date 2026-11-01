//! Maßfelder am Zeiger (Skizzen 2, Task 5): Das aktive Feld hat beim Zeichnen die Tastatur, Getipptes
//! sperrt und überlebt Zeigerbewegung und Tab, Eingabe setzt den Punkt, Esc bricht ab. Kein
//! Tastenkürzel wirkt beim Tippen, das Feld verdeckt nie den Zeiger, und ein Linienzug bleibt je
//! Linie ein Schritt für Rückgängig.
//!
//! Ohne Fenster, bei 1920 × 1080: `Overlay` spielt Rohereignisse gegen das Overlay der Skizze und
//! schreibt wie die App den Verlauf fort; `AppRig` zeichnet Bilder der ganzen App für die Kürzel.

mod common;

use eframe::App as _;
use egui::{Event, Key, Modifiers, PointerButton, Pos2, Rect, Vec2, pos2, vec2};
use planora_app::app::PlanoraApp;
use planora_app::sketch_state::{ConstraintAction, Item, SketchSession, Tool};
use planora_app::state::AppState;
use planora_app::workspace::Workspace;
use planora_app::{fonts, sketch_panel, sketch_view, theme, viewport};
use planora_feature::{ParamEdit, ParamUnit, SketchPlane, StandardPlane};
use planora_math::{DVec2, DVec3};
use planora_project::config::Config;
use planora_project::library::Library;
use planora_sketch::constraint::ConstraintKind as K;
use planora_sketch::model::{CurveKind, Sketch};

const W: f32 = 1920.0;
const H: f32 = 1080.0;
const SIZE: Vec2 = vec2(W, H);
const SCREEN: Rect = Rect {
    min: pos2(0.0, 0.0),
    max: pos2(W, H),
};
const START: Pos2 = pos2(960.0, 540.0);
const NONE: Modifiers = Modifiers::NONE;

// ---- Hilfen ---------------------------------------------------------------------------------

fn moved(p: Pos2) -> Event {
    Event::PointerMoved(p)
}

fn button(p: Pos2, pressed: bool) -> Event {
    Event::PointerButton {
        pos: p,
        button: PointerButton::Primary,
        pressed,
        modifiers: NONE,
    }
}

fn key(k: Key, modifiers: Modifiers) -> Event {
    Event::Key {
        key: k,
        physical_key: Some(k),
        pressed: true,
        repeat: false,
        modifiers,
    }
}

fn text(t: &str) -> Event {
    Event::Text(t.to_string())
}

/// Ein getipptes Zeichen, wie es das Fenster liefert: Taste und Text im selben Bild.
fn typed(k: Key, t: &str) -> Vec<Event> {
    vec![key(k, NONE), text(t)]
}

fn open_sketch() -> AppState {
    let mut s = AppState::new();
    let i = s
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    s.open_sketch(i).unwrap();
    s.observe_sketch(false);
    s
}

fn set_tool(s: &mut AppState, tool: Tool) {
    let (se, sk) = s.session_sketch_mut().unwrap();
    se.set_tool(sk, tool);
}

fn lines(sk: &Sketch) -> Vec<(DVec2, DVec2)> {
    sk.curves()
        .filter(|(_, d)| !d.construction)
        .filter_map(|(_, d)| match d.kind {
            CurveKind::Line { a, b } => Some((sk.point_xy(a).ok()?, sk.point_xy(b).ok()?)),
            _ => None,
        })
        .collect()
}

/// Das Overlay allein, mit Verlauf wie in der App.
struct Overlay {
    ctx: egui::Context,
    s: AppState,
    /// Über der Skizzenfläche (Rückgabe von `show_overlay`) im letzten Bild.
    over: bool,
    /// Alle Texte des letzten Bilds.
    seen: Vec<String>,
    /// Zeigt zusätzlich die Skizzenfelder (für das offene Maßfeld).
    with_panel: bool,
    /// Kennung der Zeichenfläche (wie in `show_overlay`).
    overlay_id: egui::Id,
}

impl Overlay {
    fn new(s: AppState) -> Overlay {
        let ctx = egui::Context::default();
        ctx.set_fonts(fonts::definitions());
        let mut o = Overlay {
            ctx,
            s,
            over: false,
            seen: Vec::new(),
            with_panel: false,
            overlay_id: egui::Id::NULL,
        };
        o.frame(vec![]);
        o
    }

    fn frame(&mut self, events: Vec<Event>) {
        let raw = egui::RawInput {
            screen_rect: Some(SCREEN),
            events,
            ..Default::default()
        };
        let (s, panel) = (&mut self.s, self.with_panel);
        let mut over = false;
        let mut gesture = false;
        let mut overlay_id = egui::Id::NULL;
        let out = self.ctx.run_ui(raw, |ui| {
            overlay_id = ui.id().with("planora-sketch-overlay");
            if panel {
                sketch_panel::show(ui.ctx(), s);
            }
            over = sketch_view::show_overlay(ui, SCREEN, s);
            gesture = ui.input(|i| i.pointer.any_down()) || viewport::typing_for_history(ui.ctx());
            viewport::remember_keyboard_focus(ui.ctx());
        });
        self.seen = common::collect_seen(&out.shapes)
            .into_iter()
            .map(|t| t.text)
            .collect();
        out.drop_without_applying_deltas();
        self.over = over;
        self.overlay_id = overlay_id;
        self.s.observe_sketch(gesture);
    }

    fn frames(&mut self, frames: Vec<Vec<Event>>) {
        for events in frames {
            self.frame(events);
        }
    }

    fn click(&mut self, p: Pos2) {
        self.frames(vec![
            vec![moved(p)],
            vec![button(p, true)],
            vec![button(p, false)],
            vec![],
            vec![],
        ]);
    }

    /// Tippt Zeichen für Zeichen, je Zeichen ein Bild.
    fn type_text(&mut self, t: &str) {
        for ch in t.chars() {
            self.frame(vec![text(&ch.to_string())]);
        }
    }

    fn se(&self) -> &SketchSession {
        self.s.sketch_session.as_ref().unwrap()
    }

    fn sk(&self) -> &Sketch {
        self.s.session_sketch().unwrap()
    }

    fn start_of_line(&self) -> DVec2 {
        self.sk()
            .point_xy(*self.se().pending.first().unwrap())
            .unwrap()
    }

    /// Länge der Vorschau: vom offenen Punkt bis zur Endlage des aufgelösten Zeigers.
    fn preview_length(&self) -> f64 {
        let at = self.se().draw_cursor.as_ref().expect("Zeiger da").at;
        at.distance(self.start_of_line())
    }
}

/// Linie begonnen: erster Punkt an `START`, Zeiger schräg daneben.
fn line_started() -> Overlay {
    let mut s = open_sketch();
    set_tool(&mut s, Tool::Line);
    let mut o = Overlay::new(s);
    o.click(START);
    o.frames(vec![vec![moved(START + vec2(200.0, -140.0))], vec![]]);
    o
}

/// Die ganze App bei 1920 × 1080 mit offener Skizze (für Kürzel und Rückgängig).
struct AppRig {
    ctx: egui::Context,
    app: PlanoraApp,
    time: f64,
}

impl AppRig {
    fn new() -> AppRig {
        let root = common::temp_root("planora-inline-fields");
        let mut w = Workspace::new(Some(Library::open(&root).unwrap()), Config::default());
        w.config_path = root.join("config.toml");
        {
            let s = &mut w.active_tab_mut().state;
            s.create_box(DVec3::splat(20.0)).unwrap();
            let sk = s
                .add_sketch(SketchPlane::Standard(StandardPlane::XY))
                .unwrap();
            s.open_sketch(sk).unwrap();
        }
        w.observe_history(false);
        let ctx = egui::Context::default();
        ctx.set_fonts(fonts::definitions());
        theme::apply(&ctx, true);
        PlanoraApp::setup_context(&ctx);
        let mut rig = AppRig {
            ctx,
            app: PlanoraApp::for_tests(w),
            time: 0.0,
        };
        rig.frames(vec![vec![], vec![], vec![]]);
        rig
    }

    fn st(&mut self) -> &mut AppState {
        &mut self.app.ws_mut().active_tab_mut().state
    }

    fn frame(&mut self, events: Vec<Event>) {
        self.time += 1.0 / 60.0;
        let raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, SIZE)),
            time: Some(self.time),
            events,
            ..Default::default()
        };
        let mut f = eframe::Frame::_new_kittest();
        let app = &mut self.app;
        self.ctx
            .run_ui(raw, |ui| app.ui(ui, &mut f))
            .drop_without_applying_deltas();
    }

    fn frames(&mut self, frames: Vec<Vec<Event>>) {
        for events in frames {
            self.frame(events);
        }
    }

    /// Klick mit genug Zeit danach, dass der nächste kein Doppelklick wird.
    fn click(&mut self, p: Pos2) {
        self.frames(vec![
            vec![moved(p)],
            vec![button(p, true)],
            vec![button(p, false)],
            vec![],
            vec![],
        ]);
        self.time += 1.0;
    }

    fn line_count(&mut self) -> usize {
        lines(self.st().session_sketch().unwrap()).len()
    }

    fn undo(&mut self) {
        let ctrl = Modifiers {
            ctrl: true,
            command: true,
            ..Modifiers::NONE
        };
        self.frames(vec![vec![key(Key::Z, ctrl)], vec![], vec![]]);
    }
}

// ---- Tastatur: Review-Focus 1 ---------------------------------------------------------------

/// Getippte Zeichen landen im Feld; kein Kürzel und keine Taste der Skizze wirkt, das Werkzeug und
/// der offene Punkt bleiben. Linie, Bogen und Kreis.
#[test]
fn typing_reaches_the_field_and_fires_no_shortcut() {
    for tool in [Tool::Line, Tool::Arc, Tool::Circle] {
        let mut rig = AppRig::new();
        set_tool(rig.st(), tool);
        rig.click(START);
        rig.frames(vec![vec![moved(START + vec2(120.0, 70.0))], vec![]]);
        assert!(
            viewport::draw_field_has_keyboard(&rig.ctx),
            "{tool:?}: Feld hat nach dem ersten Klick ohne weiteren Mausklick die Tastatur"
        );
        let pending = rig.st().sketch_session.as_ref().unwrap().pending.clone();
        assert_eq!(pending.len(), 1, "{tool:?}: erster Punkt gesetzt");
        for (k, t) in [
            (Key::L, "l"),
            (Key::C, "c"),
            (Key::T, "t"),
            (Key::R, "r"),
            (Key::Space, " "),
        ] {
            rig.frames(vec![typed(k, t), vec![]]);
        }
        rig.frames(vec![vec![key(Key::Delete, NONE)], vec![]]);
        let se = rig.st().sketch_session.as_ref().unwrap();
        assert_eq!(se.tool, tool, "{tool:?}: Werkzeug bleibt");
        assert_eq!(se.pending, pending, "{tool:?}: offener Punkt bleibt");
        assert_eq!(
            se.input.fields[0].text, "lctr ",
            "{tool:?}: alles steht im Feld"
        );
    }
}

#[test]
fn the_field_has_the_keyboard_after_the_first_click() {
    let mut s = open_sketch();
    set_tool(&mut s, Tool::Line);
    let mut o = Overlay::new(s);
    assert!(!viewport::draw_field_has_keyboard(&o.ctx), "noch kein Feld");
    assert!(!viewport::typing(&o.ctx));
    o.click(START);
    assert!(viewport::draw_field_has_keyboard(&o.ctx));
    assert!(viewport::typing(&o.ctx), "Kürzel gesperrt");
    assert!(
        !viewport::typing_for_history(&o.ctx),
        "für den Verlauf zählt das Zeichenfeld nicht"
    );
}

/// Hatte die Zeichenfläche den Fokus (Tab), nimmt das Feld beim Erscheinen die Tastatur trotzdem.
/// Wechselt man danach mit Absicht auf die Fläche, bleibt er dort, und Esc wirkt von ihr aus.
#[test]
fn focus_on_the_drawing_area_yields_when_fields_appear_and_stays_afterwards() {
    let mut s = open_sketch();
    set_tool(&mut s, Tool::Line);
    let mut o = Overlay::new(s);
    let area = o.overlay_id;
    o.ctx.memory_mut(|m| m.request_focus(area));
    o.frame(vec![]);
    assert_eq!(o.ctx.memory(|m| m.focused()), Some(area));
    o.click(START);
    assert!(viewport::draw_field_has_keyboard(&o.ctx), "Feld erschienen");

    o.ctx.memory_mut(|m| m.request_focus(area));
    o.frames(vec![vec![moved(START + vec2(90.0, 80.0))], vec![]]);
    assert_eq!(o.ctx.memory(|m| m.focused()), Some(area), "bleibt dort");
    assert!(!viewport::typing(&o.ctx));
    o.frames(vec![vec![text("3")], vec![]]);
    assert!(!o.se().input.fields[0].touched, "nichts im Feld");
    o.frames(vec![vec![key(Key::Escape, NONE)], vec![]]);
    assert!(
        o.se().pending.is_empty(),
        "Esc bricht von der Fläche aus ab"
    );
}

#[test]
fn a_typed_value_survives_pointer_moves_and_tab() {
    let mut o = line_started();
    o.type_text("25");
    assert_eq!(o.se().input.fields[0].locked, Some(25.0));
    assert!(
        (o.preview_length() - 25.0).abs() < 1e-9,
        "Vorschau folgt schon im Bild des Tippens: {}",
        o.preview_length()
    );
    for k in 1..=3 {
        o.frame(vec![moved(
            START + vec2(200.0, -140.0 + k as f32 * 40.0 / 3.0),
        )]);
        assert_eq!(o.se().input.fields[0].locked, Some(25.0));
        assert!((o.preview_length() - 25.0).abs() < 1e-9);
    }
    o.frame(vec![key(Key::Tab, NONE)]);
    assert_eq!(o.se().input.active, 1, "aktives Feld ist der Winkel");
    assert!(
        viewport::draw_field_has_keyboard(&o.ctx),
        "Tab bleibt in den Feldern"
    );
    o.frame(vec![]);
    o.frame(vec![moved(START + vec2(150.0, -180.0))]);
    assert_eq!(o.se().input.fields[0].locked, Some(25.0), "Sperre bleibt");
    assert!((o.preview_length() - 25.0).abs() < 1e-9);
    o.type_text("45");
    let fields = &o.se().input.fields;
    assert_eq!(fields[0].locked, Some(25.0));
    assert_eq!(fields[1].locked, Some(45f64.to_radians()));
    let at = o.se().draw_cursor.as_ref().unwrap().at - o.start_of_line();
    assert!((at.length() - 25.0).abs() < 1e-9);
    assert!((at.x.abs() - at.y.abs()).abs() < 1e-9, "45°: {at:?}");
    // Umschalt+Tab zurück; die Sperren bleiben, weiteres Tippen geht ins erste Feld.
    o.frame(vec![key(Key::Tab, Modifiers::SHIFT)]);
    assert_eq!(o.se().input.active, 0);
    o.frame(vec![]);
    o.type_text("0");
    assert_eq!(o.se().input.fields[0].locked, Some(250.0));
    assert_eq!(o.se().input.fields[1].locked, Some(45f64.to_radians()));
}

/// Tab und das nächste Zeichen im selben Bild: Das Zeichen gehört schon dem nächsten Feld.
#[test]
fn a_character_right_after_tab_lands_in_the_next_field() {
    let mut o = line_started();
    o.type_text("25");
    o.frame(vec![key(Key::Tab, NONE), text("4")]);
    let fields = &o.se().input.fields;
    assert_eq!(fields[0].text, "25");
    assert_eq!(fields[1].text, "4");
}

#[test]
fn enter_sets_the_point_and_the_chain_runs_on() {
    let mut o = line_started();
    o.type_text("25");
    o.frames(vec![vec![key(Key::Tab, NONE)], vec![]]);
    o.type_text("45");
    o.frames(vec![vec![key(Key::Enter, NONE)], vec![]]);
    let found = lines(o.sk());
    assert_eq!(found.len(), 1, "Linie angelegt");
    assert!((found[0].0.distance(found[0].1) - 25.0).abs() < 1e-6);
    let has = |o: &Overlay, f: fn(&K) -> bool| o.sk().constraints().any(|(_, c)| f(&c.kind));
    assert!(has(&o, |k| matches!(k, K::Distance(..))), "Länge als Maß");
    assert!(has(&o, |k| matches!(k, K::Angle(..))), "Winkel als Maß");
    let se = o.se();
    assert_eq!(se.tool, Tool::Line);
    assert_eq!(se.pending.len(), 1, "Linienzug läuft weiter");
    assert_eq!(se.input.active, 0);
    for f in &se.input.fields {
        assert!(f.locked.is_none() && !f.touched, "Feld neu: {f:?}");
    }
    assert!(
        viewport::draw_field_has_keyboard(&o.ctx),
        "Feld behält die Tastatur"
    );
    // Ohne getippten Wert setzt Eingabe an der Zeigerlage und legt kein Maß an.
    let dims = o.sk().constraints().count();
    o.frames(vec![vec![moved(START + vec2(-200.0, -90.0))], vec![]]);
    let at = o.se().draw_cursor.as_ref().unwrap().at;
    o.frames(vec![vec![key(Key::Enter, NONE)], vec![]]);
    let found = lines(o.sk());
    assert_eq!(found.len(), 2);
    assert!(
        found.iter().any(|l| l.1.distance(at) < 1e-6),
        "{found:?} {at:?}"
    );
    assert!(
        !o.sk()
            .constraints()
            .skip(dims)
            .any(|(_, c)| matches!(c.kind, K::Distance(..) | K::Angle(..))),
        "kein Maß ohne getippten Wert"
    );
}

#[test]
fn a_click_uses_the_lock() {
    let mut o = line_started();
    o.type_text("30");
    o.click(START + vec2(300.0, -210.0));
    let found = lines(o.sk());
    assert_eq!(found.len(), 1);
    assert!((found[0].0.distance(found[0].1) - 30.0).abs() < 1e-6);
    assert!(
        o.sk()
            .constraints()
            .any(|(_, c)| matches!(c.kind, K::Distance(..))),
        "Klick über `commit_input`: Maß angelegt"
    );
    assert_eq!(o.se().pending.len(), 1);
    assert!(o.se().input.fields[0].locked.is_none());
    assert!(
        viewport::draw_field_has_keyboard(&o.ctx),
        "das Feld hat die Tastatur wieder"
    );
}

#[test]
fn escape_cancels_once_and_keeps_selection_and_tool() {
    let mut s = open_sketch();
    let line = {
        let sk = s.session_sketch_mut().unwrap().1;
        let a = sk.add_point(-300.0, -200.0).unwrap();
        let b = sk.add_point(-250.0, -200.0).unwrap();
        sk.add_line(a, b).unwrap()
    };
    s.observe_sketch(false);
    set_tool(&mut s, Tool::Line);
    s.sketch_session.as_mut().unwrap().selection = vec![Item::Curve(line)];
    let mut o = Overlay::new(s);
    o.click(START);
    o.type_text("12");
    o.frames(vec![vec![key(Key::Escape, NONE)]]);
    let se = o.se();
    assert!(se.pending.is_empty(), "abgebrochen");
    assert_eq!(se.tool, Tool::Line, "Werkzeug bleibt");
    assert_eq!(se.selection, vec![Item::Curve(line)], "Auswahl bleibt");
    o.frames(vec![vec![], vec![]]);
    assert!(o.se().input.fields.is_empty());
    assert!(!viewport::draw_field_has_keyboard(&o.ctx));
    assert!(!viewport::typing(&o.ctx), "Tastatur wieder frei");
    assert_eq!(o.se().selection, vec![Item::Curve(line)]);
}

#[test]
fn a_formula_with_a_parameter_shows_fx() {
    let mut s = AppState::new();
    let id = s.add_parameter().unwrap();
    s.edit_parameter(
        id,
        ParamEdit {
            name: "Breite".into(),
            formula: "40".into(),
            unit: ParamUnit::Length,
            comment: String::new(),
        },
    )
    .unwrap();
    let i = s
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    s.open_sketch(i).unwrap();
    s.observe_sketch(false);
    set_tool(&mut s, Tool::Line);
    let mut o = Overlay::new(s);
    o.click(START);
    o.frames(vec![vec![moved(START + vec2(200.0, -140.0))], vec![]]);
    assert!(!o.seen.iter().any(|t| t == "fx"), "{:?}", o.seen);
    o.type_text("Breite/2");
    o.frame(vec![]);
    assert!(o.seen.iter().any(|t| t == "fx"), "{:?}", o.seen);
    assert_eq!(o.se().input.fields[0].locked, Some(20.0));
    assert!((o.preview_length() - 20.0).abs() < 1e-9);
    o.frames(vec![vec![key(Key::Enter, NONE)], vec![]]);
    let found = lines(o.sk());
    assert_eq!(found.len(), 1);
    assert!((found[0].0.distance(found[0].1) - 20.0).abs() < 1e-6);
}

#[test]
fn an_invalid_text_shows_its_message_and_enter_is_refused() {
    let mut o = line_started();
    o.type_text("zzz");
    o.frame(vec![]);
    let error = o.se().input.fields[0].error.clone().expect("Meldung");
    assert!(o.seen.contains(&error), "{:?}", o.seen);
    o.frames(vec![vec![key(Key::Enter, NONE)], vec![]]);
    assert!(lines(o.sk()).is_empty(), "abgelehnt");
    assert_eq!(o.se().pending.len(), 1);
    assert_eq!(o.se().input.fields[0].text, "zzz", "Text bleibt");
    assert!(viewport::draw_field_has_keyboard(&o.ctx));
}

#[test]
fn the_first_character_replaces_the_suggestion() {
    let mut o = line_started();
    let at = o.se().draw_cursor.as_ref().unwrap().at;
    let suggestion = planora_i18n::fmt_len_num(at.distance(o.start_of_line()));
    assert!(o.seen.contains(&suggestion), "{suggestion} in {:?}", o.seen);
    assert!(!o.se().input.fields[0].touched);
    o.frame(vec![text("7")]);
    let f = &o.se().input.fields[0];
    assert_eq!(f.text, "7");
    assert_eq!(f.locked, Some(7.0));
    assert!(f.touched);
    o.frame(vec![text("5")]);
    assert_eq!(o.se().input.fields[0].text, "75", "in Tippreihenfolge");
}

#[test]
fn backspace_before_the_first_character_empties_the_field() {
    let mut o = line_started();
    o.frame(vec![key(Key::Backspace, NONE)]);
    let f = &o.se().input.fields[0];
    assert!(f.touched);
    assert_eq!(f.text, "");
    assert_eq!(f.locked, None);
    o.frame(vec![text("4")]);
    assert_eq!(o.se().input.fields[0].text, "4");
    assert_eq!(o.se().input.fields[0].locked, Some(4.0));
    // Danach löscht die Rücktaste wie gewohnt ein Zeichen, leer gibt die Sperre frei.
    o.frame(vec![key(Key::Backspace, NONE)]);
    let f = &o.se().input.fields[0];
    assert_eq!(f.text, "");
    assert_eq!(f.locked, None);
}

#[test]
fn an_open_dimension_field_keeps_the_keyboard() {
    let mut s = open_sketch();
    let line = {
        let sk = s.session_sketch_mut().unwrap().1;
        let a = sk.add_point(-10.0, -10.0).unwrap();
        let b = sk.add_point(10.0, -10.0).unwrap();
        sk.add_line(a, b).unwrap()
    };
    s.observe_sketch(false);
    set_tool(&mut s, Tool::Line);
    {
        let (se, sk) = s.session_sketch_mut().unwrap();
        let start = sk.add_point(30.0, 30.0).unwrap();
        se.pending = vec![start];
        se.selection = vec![Item::Curve(line)];
    }
    let mut o = Overlay::new(s);
    o.with_panel = true;
    sketch_panel::open_dimension_editor(&o.ctx, &o.s, ConstraintAction::Distance);
    o.frames(vec![vec![], vec![], vec![]]);
    assert!(sketch_panel::dimension_editor_open(&o.ctx));
    let focus = o.ctx.memory(|m| m.focused());
    assert!(focus.is_some(), "Maßfeld hat die Tastatur");
    assert!(!viewport::draw_field_has_keyboard(&o.ctx));
    o.frames(vec![
        vec![moved(START + vec2(90.0, 80.0))],
        vec![text("3")],
        vec![],
    ]);
    assert_eq!(o.ctx.memory(|m| m.focused()), focus, "bleibt beim Maßfeld");
    let f = &o.se().input.fields[0];
    assert!(!f.touched && f.locked.is_none(), "nichts im Feld: {f:?}");
    // Tab gehört dann dem offenen Feld, nicht den Zeichenfeldern.
    o.frames(vec![vec![key(Key::Tab, NONE)], vec![]]);
    assert_eq!(o.se().input.active, 0);
}

/// Das Feld nimmt keine Mausereignisse an: In allen vier Ecken und dort, wo es eben noch stand,
/// meldet das Overlay den Zeiger als „über der Skizze“.
#[test]
fn the_field_never_covers_the_pointer() {
    let mut o = line_started();
    assert!(o.over);
    for p in [
        pos2(1.0, 1.0),
        pos2(W - 1.0, 1.0),
        pos2(1.0, H - 1.0),
        pos2(W - 1.0, H - 1.0),
        pos2(500.0, 500.0),
        // Genau auf das Feld der vorigen Lage (Zeiger + (24, 24) und etwas mehr).
        pos2(530.0, 532.0),
    ] {
        o.frames(vec![vec![moved(p)], vec![], vec![moved(p)]]);
        assert!(o.over, "Zeiger bei {p:?} nicht über der Skizze");
        assert_eq!(o.se().pending.len(), 1);
        assert!(viewport::draw_field_has_keyboard(&o.ctx));
    }
}

// ---- Rückgängig ------------------------------------------------------------------------------

#[test]
fn a_chain_of_three_lines_is_three_undo_steps() {
    let mut rig = AppRig::new();
    set_tool(rig.st(), Tool::Line);
    rig.click(START);
    for (i, p) in [
        START + vec2(200.0, -130.0),
        START + vec2(420.0, 40.0),
        START + vec2(150.0, 250.0),
    ]
    .into_iter()
    .enumerate()
    {
        rig.click(p);
        assert_eq!(rig.line_count(), i + 1);
        assert_eq!(
            rig.st().sketch_undo_label().as_deref(),
            Some("Linie zeichnen"),
            "Linie {i}"
        );
    }
    assert!(viewport::draw_field_has_keyboard(&rig.ctx), "Feld im Fokus");
    rig.undo();
    assert_eq!(rig.line_count(), 2, "Strg+Z nimmt die letzte Linie zurück");
    {
        let se = rig.st().sketch_session.as_ref().unwrap();
        assert!(
            se.pending.is_empty() && se.input.fields.is_empty(),
            "Felder leer"
        );
    }
    assert!(!viewport::typing(&rig.ctx), "Tastatur wieder frei");
    rig.undo();
    assert_eq!(rig.line_count(), 1);
    rig.undo();
    assert_eq!(rig.line_count(), 0, "drei Schritte, nicht einer");
}

#[test]
fn typed_values_and_automatic_constraints_stay_one_step_per_line() {
    let mut rig = AppRig::new();
    set_tool(rig.st(), Tool::Line);
    rig.click(START);
    // Fast waagerecht: Automatik, dazu die getippte Länge, abgeschlossen mit Eingabe.
    rig.frames(vec![vec![moved(START + vec2(300.0, 2.0))], vec![]]);
    rig.frames(vec![typed(Key::Num2, "2"), typed(Key::Num5, "5"), vec![]]);
    rig.frames(vec![vec![key(Key::Enter, NONE)], vec![], vec![]]);
    assert_eq!(rig.line_count(), 1);
    {
        let sk = rig.st().session_sketch().unwrap();
        assert!(
            sk.constraints()
                .any(|(_, c)| matches!(c.kind, K::Distance(..)))
        );
        assert!(
            sk.constraints()
                .any(|(_, c)| matches!(c.kind, K::Horizontal(_)))
        );
    }
    assert_eq!(
        rig.st().sketch_undo_label().as_deref(),
        Some("Linie zeichnen")
    );
    // Zweite Linie mit Länge und Winkel.
    rig.frames(vec![vec![moved(START + vec2(300.0, -200.0))], vec![]]);
    rig.frames(vec![
        typed(Key::Num9, "9"),
        vec![key(Key::Tab, NONE)],
        vec![],
    ]);
    rig.frames(vec![typed(Key::Num3, "3"), typed(Key::Num0, "0"), vec![]]);
    rig.frames(vec![vec![key(Key::Enter, NONE)], vec![], vec![]]);
    assert_eq!(rig.line_count(), 2);
    assert_eq!(
        rig.st().sketch_undo_label().as_deref(),
        Some("Linie zeichnen")
    );
    // Dritte Linie ohne Tippen, mit Eingabe an der Zeigerlage.
    rig.frames(vec![vec![moved(START + vec2(80.0, 260.0))], vec![]]);
    rig.frames(vec![vec![key(Key::Enter, NONE)], vec![], vec![]]);
    assert_eq!(rig.line_count(), 3);
    assert_eq!(
        rig.st().sketch_undo_label().as_deref(),
        Some("Linie zeichnen")
    );
    rig.undo();
    assert_eq!(rig.line_count(), 2);
    rig.undo();
    assert_eq!(rig.line_count(), 1);
    rig.undo();
    assert_eq!(rig.line_count(), 0);
    let sk = rig.st().session_sketch().unwrap();
    assert_eq!(sk.constraints().count(), 0, "mit den Linien auch die Maße");
}

// ---- Sprachen --------------------------------------------------------------------------------

#[test]
fn the_field_names_follow_the_language() {
    let mut o = line_started();
    o.frame(vec![]);
    assert!(o.seen.iter().any(|t| t == "Länge"), "{:?}", o.seen);
    assert!(o.seen.iter().any(|t| t == "Winkel"), "{:?}", o.seen);
    planora_i18n::with_lang(&planora_i18n::english(), || {
        let mut o = line_started();
        o.frame(vec![]);
        assert!(o.seen.iter().any(|t| t == "Length"), "{:?}", o.seen);
        assert!(o.seen.iter().any(|t| t == "Angle"), "{:?}", o.seen);
        assert!(!o.seen.iter().any(|t| t == "Länge"));
    });
}
