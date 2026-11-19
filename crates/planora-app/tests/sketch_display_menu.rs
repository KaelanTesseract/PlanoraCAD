//! Anzeigeoptionen hinter einem Symbol im Skizzenfeld (Skizzen 12, Wunsch nach Handtest 2):
//! Das Symbol in der Namenszeile öffnet ein Aufklappfeld mit den neun Haken in normaler Schrift.
//! Ohne Fenster bei 1920 × 1080; Bibliothek und Konfiguration nur in Temp-Ordnern.

mod common;

use eframe::App as _;
use egui::{Event, Key, Modifiers, PointerButton, Pos2, Rect, Vec2, vec2};
use planora_app::app::PlanoraApp;
use planora_app::commands;
use planora_app::keymap::Action;
use planora_app::state::{AppState, StandardPlane};
use planora_app::workspace::Workspace;
use planora_app::{fonts, sketch_panel, theme};
use planora_feature::{FeatureId, SketchDisplay, SketchPlane};
use planora_i18n::tr;
use planora_project::config::Config;
use planora_project::library::Library;

const SIZE: Vec2 = vec2(1920.0, 1080.0);
const NONE: Modifiers = Modifiers::NONE;

struct Rig {
    ctx: egui::Context,
    app: PlanoraApp,
    time: f64,
    shapes: Vec<egui::epaint::ClippedShape>,
    sketch: FeatureId,
}

impl Rig {
    fn new() -> Rig {
        let root = common::temp_root("planora-sketch-display-menu");
        let mut w = Workspace::new(Some(Library::open(&root).unwrap()), Config::default());
        w.config_path = root.join("config.json");
        let s = &mut w.active_tab_mut().state;
        let sketch = s
            .add_sketch(SketchPlane::Standard(StandardPlane::XY))
            .unwrap();
        s.open_sketch(sketch).unwrap();
        w.observe_history(false);
        let ctx = egui::Context::default();
        ctx.set_fonts(fonts::definitions());
        theme::apply(&ctx, true);
        PlanoraApp::setup_context(&ctx);
        let mut rig = Rig {
            ctx,
            app: PlanoraApp::for_tests(w),
            time: 0.0,
            shapes: vec![],
            sketch,
        };
        rig.settle();
        rig
    }

    fn st(&mut self) -> &mut AppState {
        &mut self.app.ws_mut().active_tab_mut().state
    }

    fn display(&mut self) -> SketchDisplay {
        let id = self.sketch;
        self.st().document.sketch_display(id)
    }

    fn steps(&mut self) -> usize {
        let w = self.app.ws_mut();
        w.observe_history(false);
        w.active_tab().history().undo_len()
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
        let out = self.ctx.run_ui(raw, |ui| app.ui(ui, &mut f));
        self.shapes = out.shapes.clone();
        out.drop_without_applying_deltas();
    }

    fn settle(&mut self) {
        for _ in 0..3 {
            self.frame(vec![]);
        }
    }

    fn click(&mut self, at: Pos2) {
        let b = |pressed| Event::PointerButton {
            pos: at,
            button: PointerButton::Primary,
            pressed,
            modifiers: NONE,
        };
        self.frame(vec![Event::PointerMoved(at)]);
        self.frame(vec![b(true)]);
        self.frame(vec![b(false)]);
        self.settle();
    }

    fn press(&mut self, key: Key) {
        let e = |pressed| Event::Key {
            key,
            physical_key: Some(key),
            pressed,
            repeat: false,
            modifiers: NONE,
        };
        self.frame(vec![e(true), e(false)]);
        self.settle();
    }

    fn button(&self) -> Rect {
        sketch_panel::display_menu_button(&self.ctx).expect("Symbol Anzeige")
    }

    fn open(&self) -> bool {
        sketch_panel::display_menu_open(&self.ctx)
    }

    fn entry(&self, label: &str) -> Rect {
        sketch_panel::display_menu_entries(&self.ctx)
            .into_iter()
            .find(|(l, _)| l == label)
            .unwrap_or_else(|| panic!("Eintrag {label} fehlt"))
            .1
    }

    /// Höhe der Schrift des ersten gezeichneten Textes `text`.
    fn text_height(&self, text: &str) -> f32 {
        common::collect_seen(&self.shapes)
            .into_iter()
            .find(|s| s.text == text)
            .unwrap_or_else(|| panic!("Text {text} nicht gezeichnet"))
            .rect
            .height()
    }
}

fn labels() -> [String; 9] {
    [
        tr!("sketch-palette-grid"),
        tr!("sketch-palette-snap"),
        tr!("sketch-palette-slice"),
        tr!("sketch-show-profile"),
        tr!("sketch-show-points"),
        tr!("sketch-show-dimensions"),
        tr!("sketch-show-constraints"),
        tr!("sketch-show-construction"),
        tr!("sketch-show-projected"),
    ]
}

#[test]
fn the_icon_opens_the_menu_with_nine_switches_in_order() {
    let mut rig = Rig::new();
    assert!(!rig.open(), "zu Beginn zu");
    // Der enge Block im Feld ist weg: Kein Haken steht ohne das Aufklappfeld im Bild.
    assert!(
        sketch_panel::display_menu_entries(&rig.ctx).is_empty(),
        "geschlossen: keine Einträge"
    );
    let name = sketch_panel::sketch_name_label(&rig.ctx)
        .expect("Namenszeile")
        .1;
    let button = rig.button();
    assert!(
        (button.center().y - name.center().y).abs() < 8.0,
        "Symbol in der Namenszeile: {button:?} {name:?}"
    );
    rig.click(button.center());
    assert!(rig.open(), "Klick auf das Symbol öffnet");
    let entries = sketch_panel::display_menu_entries(&rig.ctx);
    let names: Vec<String> = entries.iter().map(|(l, _)| l.clone()).collect();
    assert_eq!(names, labels().to_vec(), "neun Haken in dieser Reihenfolge");
    // Untereinander, direkt unter dem Symbol, jeder mit voller Klickfläche in normaler Höhe.
    let below = button.bottom();
    let mut y = below - 1.0;
    for (label, r) in &entries {
        assert!(r.top() >= y - 0.5, "{label} untereinander: {r:?} nach {y}");
        y = r.top();
        assert!(
            r.height() >= 18.0,
            "{label}: volle Klickfläche, Höhe {}",
            r.height()
        );
    }
    assert!(entries[0].1.top() >= below - 1.0, "unter dem Symbol");
}

#[test]
fn the_menu_text_is_not_smaller_than_the_field_text() {
    let mut rig = Rig::new();
    let click = rig.button().center();
    rig.click(click);
    assert!(rig.open());
    // Zum Vergleich der Knopf „Unterbestimmte finden“ im Skizzenfeld (normale Größe) und die
    // früheren 9 px.
    let field = rig.text_height(&tr!("sketch-check-find"));
    let menu = rig.text_height(&tr!("sketch-show-construction"));
    assert!(
        menu >= field - 0.5,
        "Menüschrift {menu} px nicht kleiner als Feldschrift {field} px"
    );
    assert!(menu > 12.0, "keine 9 px: {menu}");
}

#[test]
fn toggling_keeps_the_menu_open_and_is_one_undo_step_each() {
    let mut rig = Rig::new();
    let click = rig.button().center();
    rig.click(click);
    let before = rig.display();
    let steps = rig.steps();
    let at = rig.entry(&tr!("sketch-show-points")).center();
    rig.click(at);
    assert!(rig.open(), "bleibt offen");
    assert!(!rig.display().points, "Punkte aus");
    assert_eq!(rig.steps(), steps + 1, "ein Schritt");
    let at = rig.entry(&tr!("sketch-show-projected")).center();
    rig.click(at);
    assert!(rig.open(), "bleibt offen");
    assert!(!rig.display().projected);
    assert_eq!(rig.steps(), steps + 2, "ein Schritt je Umschalten");
    let at = rig.entry(&tr!("sketch-palette-slice")).center();
    rig.click(at);
    assert!(rig.display().slice, "Slice an");
    assert!(rig.open());
    // Die übrigen bleiben, wie sie waren.
    let d = rig.display();
    assert_eq!(
        (d.profiles, d.dimensions, d.constraints, d.construction),
        (
            before.profiles,
            before.dimensions,
            before.constraints,
            before.construction
        )
    );
    assert_eq!(rig.st().open_sketch_id(), Some(rig.sketch));
}

#[test]
fn grid_and_snap_switches_stay_what_they_are() {
    let mut rig = Rig::new();
    let click = rig.button().center();
    rig.click(click);
    let (grid, snap) = (rig.st().sketch_show_grid, rig.st().sketch_snap_grid);
    let steps = rig.steps();
    let at = rig.entry(&tr!("sketch-palette-grid")).center();
    rig.click(at);
    assert_eq!(rig.st().sketch_show_grid, !grid);
    let at = rig.entry(&tr!("sketch-palette-snap")).center();
    rig.click(at);
    assert_eq!(rig.st().sketch_snap_grid, !snap);
    assert!(rig.open());
    assert_eq!(
        rig.steps(),
        steps,
        "Raster und Fang sind keine Dokumentschritte"
    );
}

#[test]
fn closes_beside_on_escape_on_the_icon_and_with_the_sketch() {
    let mut rig = Rig::new();
    let button = rig.button().center();
    // Klick daneben (in die Zeichenfläche).
    rig.click(button);
    assert!(rig.open());
    rig.click(Pos2::new(700.0, 600.0));
    assert!(!rig.open(), "Klick daneben schließt");
    // Esc.
    rig.click(button);
    assert!(rig.open());
    rig.press(Key::Escape);
    assert!(!rig.open(), "Esc schließt");
    assert_eq!(
        rig.st().open_sketch_id(),
        Some(rig.sketch),
        "Skizze bleibt offen"
    );
    // Erneuter Klick auf das Symbol.
    rig.click(button);
    assert!(rig.open());
    rig.click(button);
    assert!(!rig.open(), "zweiter Klick auf das Symbol schließt");
    // Beim Beenden der Skizze.
    rig.click(button);
    assert!(rig.open());
    rig.st().close_sketch();
    rig.settle();
    assert!(!rig.open(), "Skizze beendet: zu");
    let id = rig.sketch;
    rig.st().open_sketch(id).unwrap();
    rig.settle();
    assert!(!rig.open(), "bleibt zu, wenn die Skizze wieder aufgeht");
}

#[test]
fn the_command_opens_the_menu_and_has_no_default_key() {
    let mut rig = Rig::new();
    assert!(Action::ALL.contains(&Action::SketchDisplayMenu));
    assert!(
        rig.app
            .ws_mut()
            .keymap
            .bindings(Action::SketchDisplayMenu)
            .is_empty(),
        "keine Vorgabetaste"
    );
    commands::run_action(rig.app.ws_mut(), Action::SketchDisplayMenu);
    rig.settle();
    assert!(rig.open(), "Befehl öffnet");
    commands::run_action(rig.app.ws_mut(), Action::SketchDisplayMenu);
    rig.settle();
    assert!(!rig.open(), "zweiter Aufruf schließt");
}
