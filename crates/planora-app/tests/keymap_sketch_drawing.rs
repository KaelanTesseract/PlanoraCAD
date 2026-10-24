//! Fusion-Kürzel in Skizze und Zeichnung: L Linie, R Rechteck, C Kreis, P Körperkante kopieren in
//! der offenen Skizze, D Maß und B Position in der Zeichnung; dieselbe Wirkung wie der Klick auf
//! die Schaltfläche, Tooltips mit Taste, keine Wirkung beim Tippen in Maß- und Formelfeldern, Esc,
//! Enter und Entf der Skizze unverändert, „Letztes Skizzenwerkzeug wiederholen“ (Leertaste).
//!
//! Mit echten Bildern der ganzen App (`PlanoraApp::for_tests`, `eframe::App::ui`); Bibliothek und
//! Konfiguration nur im Temp-Ordner.

mod common;

use eframe::App as _;
use egui::{Event, Key, Modifiers, PointerButton, Pos2, Rect, Vec2, vec2};
use planora_app::app::PlanoraApp;
use planora_app::commands::LastCommand;
use planora_app::keymap::{Action, Area, Binding, KeyChord, Keymap, SetResult, shortcut_hint};
use planora_app::sketch_state::{ConstraintAction, Item, Tool};
use planora_app::state::AppState;
use planora_app::workspace::Workspace;
use planora_app::{balloon_tool, fonts, sketch_panel, theme, viewport};
use planora_drawing::{Paper, Placement, Projection, Scale, Sheet, ViewKind};
use planora_feature::{SketchPlane, StandardPlane};
use planora_math::{DVec2, DVec3};
use planora_project::config::Config;
use planora_project::library::Library;

const SIZE: Vec2 = vec2(1400.0, 900.0);
const NONE: Modifiers = Modifiers::NONE;

// ---- Hilfen ---------------------------------------------------------------------------------

fn ws() -> Workspace {
    let root = common::temp_root("planora-keymap-sketch");
    let mut w = Workspace::new(Some(Library::open(&root).unwrap()), Config::default());
    w.config_path = root.join("config.toml");
    w
}

fn chord(key: Key, ctrl: bool) -> KeyChord {
    KeyChord {
        key,
        ctrl,
        shift: false,
        alt: false,
    }
}

fn plain(key: Key) -> Binding {
    Binding::Key(chord(key, false))
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

fn button(p: Pos2, pressed: bool) -> Event {
    Event::PointerButton {
        pos: p,
        button: PointerButton::Primary,
        pressed,
        modifiers: NONE,
    }
}

struct Seen {
    text: String,
    rect: Rect,
}

fn walk(shape: &egui::epaint::Shape, out: &mut Vec<Seen>) {
    match shape {
        egui::epaint::Shape::Vec(v) => v.iter().for_each(|s| walk(s, out)),
        egui::epaint::Shape::Text(t) => out.push(Seen {
            text: t.galley.text().to_string(),
            rect: Rect::from_min_size(t.pos, t.galley.size()),
        }),
        _ => {}
    }
}

struct Rig {
    ctx: egui::Context,
    app: PlanoraApp,
    time: f64,
    /// Blieben nach dem Bild Tastenereignisse übrig (nicht verbraucht)?
    left_over: bool,
}

impl Rig {
    fn new(w: Workspace) -> Rig {
        let ctx = egui::Context::default();
        ctx.set_fonts(fonts::definitions());
        theme::apply(&ctx, true);
        PlanoraApp::setup_context(&ctx);
        ctx.all_styles_mut(|s| s.interaction.tooltip_delay = 0.0);
        let mut rig = Rig {
            ctx,
            app: PlanoraApp::for_tests(w),
            time: 0.0,
            left_over: false,
        };
        rig.settle();
        rig
    }

    fn ws(&mut self) -> &mut Workspace {
        self.app.ws_mut()
    }

    fn st(&mut self) -> &mut AppState {
        &mut self.app.ws_mut().active_tab_mut().state
    }

    fn frame(&mut self, events: Vec<Event>) -> Vec<Seen> {
        self.time += 1.0 / 60.0;
        let raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, SIZE)),
            time: Some(self.time),
            events,
            ..Default::default()
        };
        let mut f = eframe::Frame::_new_kittest();
        let app = &mut self.app;
        let mut left_over = false;
        let out = self.ctx.run_ui(raw, |ui| {
            app.ui(ui, &mut f);
            left_over = ui.input(|i| {
                i.events
                    .iter()
                    .any(|e| matches!(e, Event::Key { pressed: true, .. }))
            });
        });
        self.left_over = left_over;
        let mut seen = Vec::new();
        for c in &out.shapes {
            walk(&c.shape, &mut seen);
        }
        out.drop_without_applying_deltas();
        seen
    }

    fn settle(&mut self) -> Vec<Seen> {
        self.frame(vec![]);
        self.frame(vec![]);
        self.frame(vec![])
    }

    fn press(&mut self, k: Key, m: Modifiers) {
        self.frame(vec![key(k, m)]);
    }

    fn click(&mut self, p: Pos2) {
        self.frame(vec![Event::PointerMoved(p)]);
        self.frame(vec![button(p, true)]);
        self.frame(vec![button(p, false)]);
        self.frame(vec![]);
    }

    fn hover(&mut self, p: Pos2) -> Vec<String> {
        let mut seen = vec![];
        for _ in 0..5 {
            self.time += 0.3;
            seen = self.frame(vec![Event::PointerMoved(p)]);
        }
        seen.into_iter().map(|s| s.text).collect()
    }

    /// Werkzeug der offenen Skizze.
    fn tool(&mut self) -> Tool {
        self.st()
            .sketch_session
            .as_ref()
            .expect("Skizze offen")
            .tool
    }

    /// Lage des Werkzeugknopfs `label` in der Skizzenleiste.
    fn strip(&self, label: &str) -> Pos2 {
        sketch_panel::strip_buttons(&self.ctx)
            .into_iter()
            .find(|(l, _)| *l == label)
            .unwrap_or_else(|| panic!("Werkzeug „{label}“ fehlt"))
            .1
            .center()
    }

    /// Klick in die Skizze mit dem aktuellen Werkzeug (wie in der Ansicht).
    fn sketch_click(&mut self, x: f64, y: f64) {
        let (se, sk) = self.st().session_sketch_mut().expect("Skizze offen");
        se.click(sk, DVec2::new(x, y), 0.01, false).unwrap();
        self.settle();
    }

    fn sketch_tool_memory(&mut self) -> Option<Action> {
        self.ws().last_command.sketch_tool()
    }
}

/// Ein Quader und eine offene, leere Skizze auf XY.
fn sketch_open_ws() -> Workspace {
    let mut w = ws();
    let s = &mut w.active_tab_mut().state;
    s.create_box(DVec3::splat(20.0)).unwrap();
    let sk = s
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    s.open_sketch(sk).unwrap();
    w.observe_history(false);
    w
}

/// Ein Quader und eine gezeigte Zeichnung mit einer Vorderansicht.
fn drawing_ws() -> Workspace {
    drawing_ws_with(Some(ViewKind::Front))
}

/// Ein Quader und eine gezeigte Zeichnung, mit einer Ansicht der Art `view` oder ohne Ansicht.
fn drawing_ws_with(view: Option<ViewKind>) -> Workspace {
    let mut w = ws();
    let s = &mut w.active_tab_mut().state;
    s.create_box(DVec3::splat(20.0)).unwrap();
    let d = s
        .document
        .add_drawing(
            "Blatt 1",
            Sheet {
                paper: Paper::A4,
                landscape: true,
            },
            Projection::First,
        )
        .unwrap();
    if let Some(kind) = view {
        s.document
            .add_view(
                d,
                kind,
                Scale { num: 1, den: 2 },
                Placement::Free { x: 150.0, y: 110.0 },
                None,
            )
            .unwrap();
    }
    s.drawing_ui.open(d);
    w.observe_history(false);
    w
}

const SKETCH_KEYS: [(Action, Key, Tool, &str); 4] = [
    (Action::SketchLine, Key::L, Tool::Line, "Linie"),
    (Action::SketchRectangle, Key::R, Tool::Rectangle, "Rechteck"),
    (Action::SketchCircle, Key::C, Tool::Circle, "Kreis"),
    (
        Action::SketchProjectEdge,
        Key::P,
        Tool::ProjectEdge,
        "Körperkante kopieren",
    ),
];

// ---- Tests ----------------------------------------------------------------------------------

#[test]
fn sketch_keys_select_the_tools() {
    let km = Keymap::defaults();
    for (a, k, _, label) in SKETCH_KEYS {
        assert_eq!(a.area(), Area::Sketch, "{a:?}");
        assert_eq!(km.bindings(a), [plain(k)], "{a:?}");
        assert_eq!(a.label(), label, "Name wie der Werkzeugknopf");
        assert_eq!(km.action_for_key(Area::Sketch, &chord(k, false)), Some(a));
    }
    // Versatz O, Stutzen T, Skizzenbemaßung D: gibt es (noch) nicht.
    for k in [Key::O, Key::T, Key::D] {
        assert_eq!(
            km.action_for_key(Area::Sketch, &chord(k, false)),
            None,
            "{k:?}"
        );
    }

    let mut rig = Rig::new(sketch_open_ws());
    assert_eq!(rig.tool(), Tool::Select);
    for (_, k, tool, label) in SKETCH_KEYS {
        rig.press(k, NONE);
        assert!(!rig.left_over, "{k:?} verbraucht");
        assert_eq!(rig.tool(), tool, "{k:?}");
        // Wie der Klick auf den Knopf: Auswahl → Werkzeug über die Leiste ergibt dasselbe.
        rig.settle();
        let at = rig.strip("Auswahl");
        rig.click(at);
        assert_eq!(rig.tool(), Tool::Select);
        let at = rig.strip(label);
        rig.click(at);
        assert_eq!(rig.tool(), tool, "Knopf „{label}“");
        rig.click(rig.strip("Auswahl"));
    }

    // Wie ein Klick auf den schon gewählten Knopf: ein laufender Linienzug bleibt.
    rig.press(Key::L, NONE);
    rig.sketch_click(0.0, 0.0);
    assert_eq!(rig.st().sketch_session.as_ref().unwrap().pending.len(), 1);
    rig.press(Key::L, NONE);
    assert_eq!(rig.st().sketch_session.as_ref().unwrap().pending.len(), 1);
    // Ein anderes Werkzeug bricht ihn ab (wie der Knopf).
    rig.press(Key::C, NONE);
    assert!(rig.st().sketch_session.as_ref().unwrap().pending.is_empty());

    // Tooltips der Leiste nennen die Taste, nach dem Ändern die neue.
    rig.settle();
    let texts = rig.hover(rig.strip("Linie"));
    assert!(texts.iter().any(|t| t == "Linie (L)"), "{texts:?}");
    let texts = rig.hover(rig.strip("Körperkante kopieren"));
    assert!(
        texts.iter().any(|t| t == "Körperkante kopieren (P)"),
        "{texts:?}"
    );
    let mut km = rig.ws().keymap.clone();
    assert_eq!(km.set(Action::SketchLine, 0, plain(Key::K)), SetResult::Ok);
    rig.ws().keymap = km;
    let texts = rig.hover(rig.strip("Linie"));
    assert!(texts.iter().any(|t| t == "Linie (K)"), "{texts:?}");
    // Werkzeuge ohne Kürzel: nur der Name.
    let texts = rig.hover(rig.strip("Bogen"));
    assert!(texts.iter().any(|t| t == "Bogen"), "{texts:?}");
}

#[test]
fn same_letter_means_different_things_in_sketch_and_drawing_and_nothing_in_model() {
    let km = Keymap::defaults();
    let c = chord(Key::C, false);
    assert_eq!(
        km.action_for_key(Area::Sketch, &c),
        Some(Action::SketchCircle)
    );
    // Mittelpunktmarke gibt es (noch) nicht: C ist in der Zeichnung frei, im Modell auch.
    assert_eq!(km.action_for_key(Area::Drawing, &c), None);
    assert_eq!(km.action_for_key(Area::Model, &c), None);
    assert_eq!(km.action_for_key(Area::All, &c), None);
    // D: Maß in der Zeichnung, nichts in Skizze und Modell.
    let d = chord(Key::D, false);
    assert_eq!(
        km.action_for_key(Area::Drawing, &d),
        Some(Action::DrawingDimension)
    );
    assert_eq!(km.action_for_key(Area::Sketch, &d), None);
    assert_eq!(km.action_for_key(Area::Model, &d), None);

    // Gleiche Taste in Skizze und Zeichnung ist erlaubt: B (Position) auf L legen.
    let mut km2 = km.clone();
    assert_eq!(
        km2.conflict_for(Action::DrawingBalloon, &plain(Key::L)),
        None
    );
    assert_eq!(
        km2.set(Action::DrawingBalloon, 0, plain(Key::L)),
        SetResult::Ok
    );
    let l = chord(Key::L, false);
    assert_eq!(
        km2.action_for_key(Area::Sketch, &l),
        Some(Action::SketchLine)
    );
    assert_eq!(
        km2.action_for_key(Area::Drawing, &l),
        Some(Action::DrawingBalloon)
    );
    assert_eq!(km2.action_for_key(Area::Model, &l), None);
    // Im selben Bereich ist es eine Doppelbelegung.
    assert_eq!(
        km.conflict_for(Action::SketchLine, &plain(Key::C)),
        Some(Action::SketchCircle)
    );
    assert_eq!(
        km.conflict_for(Action::DrawingBalloon, &plain(Key::D)),
        Some(Action::DrawingDimension)
    );
    // Skizze gegen „Überall“, in beide Richtungen.
    let ctrl_s = Binding::Key(chord(Key::S, true));
    assert_eq!(
        km.conflict_for(Action::SketchLine, &ctrl_s),
        Some(Action::Save)
    );
    assert_eq!(
        km.conflict_for(Action::Save, &plain(Key::L)),
        Some(Action::SketchLine)
    );
    assert_eq!(
        km.conflict_for(Action::Undo, &plain(Key::B)),
        Some(Action::DrawingBalloon)
    );
    // Modell gegen Skizze: keine Doppelbelegung (E Extrudieren, Skizze frei).
    assert_eq!(km.conflict_for(Action::SketchLine, &plain(Key::E)), None);

    // In der App: C in der Skizze wählt Kreis.
    let mut rig = Rig::new(sketch_open_ws());
    rig.press(Key::C, NONE);
    assert_eq!(rig.tool(), Tool::Circle);

    // C in der Zeichnung: kein Werkzeug, die Taste bleibt liegen.
    let mut rig = Rig::new(drawing_ws());
    rig.press(Key::C, NONE);
    assert!(rig.left_over, "C bleibt in der Zeichnung liegen");
    assert!(!rig.st().dimension_tool.is_active());
    assert!(!rig.st().balloon_tool.is_active());
    assert!(!rig.st().section_tool.is_active());

    // C im Modell: nichts.
    let mut w = ws();
    w.active_tab_mut()
        .state
        .create_box(DVec3::splat(10.0))
        .unwrap();
    let mut rig = Rig::new(w);
    let before = rig.st().document.features().len();
    rig.press(Key::C, NONE);
    assert!(rig.left_over, "C bleibt im Modell liegen");
    assert!(rig.st().sketch_session.is_none());
    assert!(!rig.ws().active_tab().dialog_open());
    assert_eq!(rig.st().document.features().len(), before);
}

#[test]
fn sketch_keys_are_ignored_while_typing_in_dimension_or_formula_fields() {
    let mut rig = Rig::new(sketch_open_ws());
    rig.press(Key::L, NONE);
    rig.sketch_click(0.0, 0.0);
    rig.sketch_click(10.0, 0.0);
    rig.press(Key::Escape, NONE);
    rig.click(rig.strip("Auswahl"));
    // Die Linie wählen und das Maßfeld (Formelfeld) für „Abstand“ öffnen.
    let line = {
        let sk = rig.st().session_sketch().unwrap();
        sk.curves().next().unwrap().0
    };
    rig.st().sketch_session.as_mut().unwrap().selection = vec![Item::Curve(line)];
    sketch_panel::open_dimension_editor(
        &rig.ctx,
        &rig.app.ws_mut().active_tab().state,
        ConstraintAction::Distance,
    );
    rig.settle();
    assert!(sketch_panel::dimension_editor_open(&rig.ctx));
    assert!(viewport::typing(&rig.ctx), "das Maßfeld hat die Tastatur");
    for k in [Key::L, Key::R, Key::C, Key::P, Key::Space] {
        rig.frame(vec![key(k, NONE)]);
        assert_eq!(rig.tool(), Tool::Select, "{k:?} gehört dem Feld");
    }
    // Auch mit dem Text dazu (wie beim echten Tippen): das Feld bleibt offen und hat die
    // Tastatur, das Werkzeug bleibt.
    rig.frame(vec![key(Key::L, NONE), Event::Text("l".into())]);
    rig.settle();
    assert_eq!(rig.tool(), Tool::Select);
    assert!(sketch_panel::dimension_editor_open(&rig.ctx));
    assert!(viewport::typing(&rig.ctx));
    // Feld verlassen (Abbrechen): jetzt wirkt L wieder.
    let at = rig
        .settle()
        .into_iter()
        .find(|s| s.text == "Abbrechen")
        .expect("Abbrechen im Maßfeld")
        .rect
        .center();
    rig.click(at);
    assert!(!sketch_panel::dimension_editor_open(&rig.ctx));
    rig.press(Key::L, NONE);
    assert_eq!(rig.tool(), Tool::Line);
}

#[test]
fn esc_enter_delete_in_sketch_unchanged() {
    // Nicht in der Tabelle: keine Aktion der Skizze hat Esc, Enter oder Entf.
    let km = Keymap::defaults();
    for k in [Key::Escape, Key::Enter, Key::Delete] {
        assert_eq!(
            km.action_for_key(Area::Sketch, &chord(k, false)),
            None,
            "{k:?}"
        );
    }
    let mut rig = Rig::new(sketch_open_ws());
    // Esc bricht den laufenden Linienzug ab, das Werkzeug bleibt.
    rig.press(Key::L, NONE);
    rig.sketch_click(0.0, 0.0);
    rig.press(Key::Escape, NONE);
    assert!(rig.st().sketch_session.as_ref().unwrap().pending.is_empty());
    assert_eq!(rig.tool(), Tool::Line);
    // Enter legt einen Spline an (Werkzeug über die Leiste, ohne Kürzel).
    rig.click(rig.strip("Spline"));
    assert_eq!(rig.tool(), Tool::Spline);
    rig.sketch_click(0.0, 0.0);
    rig.sketch_click(5.0, 5.0);
    rig.sketch_click(10.0, 0.0);
    let before = rig.st().session_sketch().unwrap().curves().count();
    rig.press(Key::Enter, NONE);
    assert_eq!(
        rig.st().session_sketch().unwrap().curves().count(),
        before + 1,
        "Enter legt den Spline an"
    );
    // Entf löscht die Auswahl der Skizze (nicht den Körper des Modells).
    let curve = rig
        .st()
        .session_sketch()
        .unwrap()
        .curves()
        .next()
        .unwrap()
        .0;
    rig.st().sketch_session.as_mut().unwrap().selection = vec![Item::Curve(curve)];
    rig.press(Key::Delete, NONE);
    assert!(rig.st().session_sketch().unwrap().curve(curve).is_none());
    assert_eq!(rig.st().bodies.len(), 1, "Körper bleibt");
}

#[test]
fn drawing_tool_keys_select_the_tools() {
    let km = Keymap::defaults();
    for (a, k) in [
        (Action::DrawingDimension, Key::D),
        (Action::DrawingBalloon, Key::B),
    ] {
        assert_eq!(a.area(), Area::Drawing, "{a:?}");
        assert_eq!(km.bindings(a), [plain(k)], "{a:?}");
    }
    // Mittelpunktmarke C, Verschieben M, Projizierte Ansicht P, Text T: gibt es (noch) nicht.
    // Entf behält in der Zeichnung sein festes Verhalten (nicht in der Tabelle).
    for k in [Key::C, Key::M, Key::P, Key::T, Key::Delete] {
        assert_eq!(
            km.action_for_key(Area::Drawing, &chord(k, false)),
            None,
            "{k:?}"
        );
    }

    let mut rig = Rig::new(drawing_ws());
    // D schaltet „Maß“ ein und wieder aus (wie der Schalter).
    rig.press(Key::D, NONE);
    assert!(!rig.left_over);
    assert!(rig.st().dimension_tool.is_active());
    rig.press(Key::D, NONE);
    assert!(!rig.st().dimension_tool.is_active());
    // B schaltet „Position“ ein; D danach schaltet „Position“ aus (die Werkzeuge schließen sich
    // aus, wie bei den Schaltern).
    rig.press(Key::B, NONE);
    assert!(rig.st().balloon_tool.is_active());
    rig.press(Key::D, NONE);
    assert!(rig.st().dimension_tool.is_active());
    assert!(!rig.st().balloon_tool.is_active());
    rig.press(Key::B, NONE);
    assert!(rig.st().balloon_tool.is_active());
    assert!(!rig.st().dimension_tool.is_active());
    // Esc schaltet wie bisher aus.
    rig.press(Key::Escape, NONE);
    assert!(!rig.st().balloon_tool.is_active());

    // Tooltip des Schalters mit Taste.
    let seen = rig.settle();
    let at = seen
        .iter()
        .find(|s| s.text == balloon_tool::TOOL)
        .expect("Schalter Position")
        .rect
        .center();
    let texts = rig.hover(at);
    assert!(texts.iter().any(|t| t == "Position (B)"), "{texts:?}");
    assert!(
        texts.iter().any(|t| t == balloon_tool::TOOL_TIP),
        "{texts:?}"
    );
    assert_eq!(
        shortcut_hint(&rig.ws().keymap, Action::DrawingDimension),
        "D"
    );

    // Gesperrter Schalter (Zeichnung ohne Ansicht): die Taste tut nichts.
    let mut rig = Rig::new(drawing_ws_with(None));
    rig.press(Key::D, NONE);
    rig.press(Key::B, NONE);
    assert!(!rig.st().dimension_tool.is_active());
    assert!(!rig.st().balloon_tool.is_active());
}

#[test]
fn rebinding_a_sketch_key_frees_the_old_one() {
    let mut rig = Rig::new(sketch_open_ws());
    let mut km = rig.ws().keymap.clone();
    assert_eq!(
        km.set(Action::SketchCircle, 0, plain(Key::K)),
        SetResult::Ok
    );
    rig.ws().keymap = km;
    rig.press(Key::C, NONE);
    assert!(rig.left_over, "C bleibt liegen");
    assert_eq!(rig.tool(), Tool::Select);
    rig.press(Key::K, NONE);
    assert!(!rig.left_over);
    assert_eq!(rig.tool(), Tool::Circle);
    // Auf eine Taste von „Überall“ geht es nur mit Tauschen.
    let mut km = rig.ws().keymap.clone();
    assert_eq!(
        km.set(Action::SketchCircle, 0, Binding::Key(chord(Key::S, true))),
        SetResult::Conflict(Action::Save)
    );
    km.swap(Action::SketchCircle, 0, Binding::Key(chord(Key::S, true)));
    assert!(km.bindings(Action::Save).is_empty());
    assert_eq!(
        km.action_for_key(Area::Sketch, &chord(Key::S, true)),
        Some(Action::SketchCircle)
    );
}

#[test]
fn repeat_last_remembers_sketch_tools() {
    // Wiederholbar sind die Skizzenwerkzeuge; ihr Gedächtnis ist getrennt vom Modell.
    for (a, ..) in SKETCH_KEYS {
        assert!(a.repeatable(), "{a:?}");
    }
    assert!(!Action::SketchRepeatLast.repeatable());
    assert!(!Action::DrawingDimension.repeatable());
    assert!(!Action::DrawingBalloon.repeatable());
    assert_eq!(Action::SketchRepeatLast.area(), Area::Sketch);
    assert_eq!(
        Keymap::defaults().bindings(Action::SketchRepeatLast),
        [plain(Key::Space)]
    );
    let mut last = LastCommand::default();
    last.remember(Action::Fillet);
    last.remember(Action::SketchLine);
    assert_eq!(last.take(), Some(Action::Fillet), "Modell bleibt");
    assert_eq!(last.sketch_tool(), Some(Action::SketchLine));

    let mut w = sketch_open_ws();
    w.last_command.remember(Action::ExtrudeSketch);
    let mut rig = Rig::new(w);
    // Nichts zu wiederholen: die Leertaste tut nichts (Werkzeug bleibt).
    rig.press(Key::Space, NONE);
    assert!(!rig.left_over, "Leertaste verbraucht");
    assert_eq!(rig.tool(), Tool::Select);
    // Nur gewählt und mit Esc verlassen: nicht gemerkt.
    rig.press(Key::C, NONE);
    rig.press(Key::Escape, NONE);
    rig.click(rig.strip("Auswahl"));
    assert_eq!(rig.sketch_tool_memory(), None);
    rig.press(Key::Space, NONE);
    assert_eq!(rig.tool(), Tool::Select);

    // Gewählt (Taste) und benutzt: gemerkt; die Leertaste wählt es wieder.
    rig.press(Key::L, NONE);
    rig.sketch_click(0.0, 0.0);
    assert_eq!(rig.sketch_tool_memory(), None, "erst mit einer Linie");
    rig.sketch_click(10.0, 0.0);
    assert_eq!(rig.sketch_tool_memory(), Some(Action::SketchLine));
    rig.press(Key::Escape, NONE);
    rig.click(rig.strip("Auswahl"));
    rig.press(Key::Space, NONE);
    assert_eq!(rig.tool(), Tool::Line);
    // Das gemerkte Extrudieren des Modells bleibt.
    assert_eq!(rig.ws().last_command.take(), Some(Action::ExtrudeSketch));

    // Über den Knopf gewählt und benutzt: ebenso gemerkt.
    rig.click(rig.strip("Rechteck"));
    rig.sketch_click(20.0, 20.0);
    rig.sketch_click(30.0, 30.0);
    assert_eq!(rig.sketch_tool_memory(), Some(Action::SketchRectangle));
    // Ein Werkzeug ohne Eintrag in der Tabelle (Bogen) ersetzt das Gedächtnis nicht.
    rig.click(rig.strip("Bogen"));
    rig.sketch_click(50.0, 0.0);
    rig.sketch_click(55.0, 0.0);
    rig.sketch_click(50.0, 5.0);
    assert_eq!(rig.sketch_tool_memory(), Some(Action::SketchRectangle));
    rig.click(rig.strip("Auswahl"));
    rig.press(Key::Space, NONE);
    assert_eq!(rig.tool(), Tool::Rectangle);

    // Rückgängig und Wiederholen mit gewähltem Werkzeug zählen nicht als Benutzen.
    rig.press(Key::C, NONE);
    rig.press(Key::Z, Modifiers::COMMAND);
    rig.settle();
    rig.press(Key::Y, Modifiers::COMMAND);
    rig.settle();
    assert_eq!(rig.sketch_tool_memory(), Some(Action::SketchRectangle));

    // Beim Tippen im Maßfeld gehört die Leertaste dem Feld.
    rig.click(rig.strip("Auswahl"));
    let line = rig
        .st()
        .session_sketch()
        .unwrap()
        .curves()
        .next()
        .unwrap()
        .0;
    rig.st().sketch_session.as_mut().unwrap().selection = vec![Item::Curve(line)];
    sketch_panel::open_dimension_editor(
        &rig.ctx,
        &rig.app.ws_mut().active_tab().state,
        ConstraintAction::Distance,
    );
    rig.settle();
    assert!(viewport::typing(&rig.ctx));
    rig.press(Key::Space, NONE);
    assert_eq!(rig.tool(), Tool::Select, "nicht beim Tippen");
}

// ---- Fix-Runde 1 ----------------------------------------------------------------------------

/// Ist die Kurve `c` der offenen Skizze eine Konstruktionslinie?
fn construction(rig: &mut Rig, c: planora_sketch::model::CurveId) -> bool {
    rig.st()
        .session_sketch()
        .unwrap()
        .curve(c)
        .unwrap()
        .construction
}

#[test]
fn construction_key_toggles_like_the_button() {
    let km = Keymap::defaults();
    assert_eq!(Action::SketchConstruction.area(), Area::Sketch);
    assert_eq!(Action::SketchConstruction.id(), "skizze.konstruktion");
    assert_eq!(
        Action::SketchConstruction.label(),
        sketch_panel::CONSTRUCTION,
        "Name wie der Knopf"
    );
    assert_eq!(km.bindings(Action::SketchConstruction), [plain(Key::X)]);
    assert!(!Action::SketchConstruction.repeatable());
    assert_eq!(km.action_for_key(Area::Model, &chord(Key::X, false)), None);

    let mut rig = Rig::new(sketch_open_ws());
    rig.press(Key::L, NONE);
    rig.sketch_click(0.0, 0.0);
    rig.sketch_click(10.0, 0.0);
    rig.press(Key::Escape, NONE);
    rig.click(rig.strip("Auswahl"));
    let memory = rig.sketch_tool_memory();
    let line = rig
        .st()
        .session_sketch()
        .unwrap()
        .curves()
        .next()
        .unwrap()
        .0;
    assert!(!construction(&mut rig, line));

    // Ohne Auswahl: keine Wirkung (der Knopf ist dann gesperrt).
    rig.st().sketch_session.as_mut().unwrap().selection.clear();
    rig.press(Key::X, NONE);
    assert!(!rig.left_over, "X verbraucht");
    assert!(!construction(&mut rig, line));

    // Mit gewählter Kurve: umschalten und zurück, wie der Knopf.
    rig.st().sketch_session.as_mut().unwrap().selection = vec![Item::Curve(line)];
    rig.press(Key::X, NONE);
    assert!(construction(&mut rig, line), "X schaltet auf Konstruktion");
    rig.press(Key::X, NONE);
    assert!(!construction(&mut rig, line), "X schaltet zurück");
    let at = rig
        .settle()
        .into_iter()
        .find(|s| s.text == sketch_panel::CONSTRUCTION)
        .expect("Knopf Konstruktion umschalten")
        .rect
        .center();
    rig.click(at);
    assert!(construction(&mut rig, line), "Knopf schaltet ebenso");
    // Tooltip mit Taste; nicht wiederholbar (Gedächtnis unverändert).
    let texts = rig.hover(at);
    assert!(
        texts.iter().any(|t| t == "Konstruktion umschalten (X)"),
        "{texts:?}"
    );
    assert_eq!(rig.sketch_tool_memory(), memory);

    // Beim Tippen im Maßfeld gehört X dem Feld.
    sketch_panel::open_dimension_editor(
        &rig.ctx,
        &rig.app.ws_mut().active_tab().state,
        ConstraintAction::Distance,
    );
    rig.settle();
    assert!(viewport::typing(&rig.ctx));
    rig.frame(vec![key(Key::X, NONE), Event::Text("x".into())]);
    rig.settle();
    assert!(construction(&mut rig, line), "nicht beim Tippen");

    // Umbelegt auf J: X bleibt liegen, J schaltet.
    let mut rig = Rig::new(sketch_open_ws());
    rig.press(Key::L, NONE);
    rig.sketch_click(0.0, 0.0);
    rig.sketch_click(10.0, 0.0);
    rig.press(Key::Escape, NONE);
    let line = rig
        .st()
        .session_sketch()
        .unwrap()
        .curves()
        .next()
        .unwrap()
        .0;
    rig.st().sketch_session.as_mut().unwrap().selection = vec![Item::Curve(line)];
    let mut km = rig.ws().keymap.clone();
    assert_eq!(
        km.set(Action::SketchConstruction, 0, plain(Key::J)),
        SetResult::Ok
    );
    rig.ws().keymap = km;
    rig.press(Key::X, NONE);
    assert!(rig.left_over, "X bleibt liegen");
    assert!(!construction(&mut rig, line));
    rig.press(Key::J, NONE);
    assert!(construction(&mut rig, line), "J schaltet");
}

#[test]
fn dimension_key_does_nothing_on_an_isometric_only_drawing() {
    // Nur Isometrie: „Maß“ ist gesperrt, „Position“ nicht.
    let mut rig = Rig::new(drawing_ws_with(Some(ViewKind::Isometric)));
    rig.press(Key::B, NONE);
    assert!(rig.st().balloon_tool.is_active(), "Position ein");
    rig.press(Key::D, NONE);
    assert!(!rig.st().dimension_tool.is_active(), "Maß bleibt aus");
    assert!(
        rig.st().balloon_tool.is_active(),
        "Position bleibt eingeschaltet"
    );
}
