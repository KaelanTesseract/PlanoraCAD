//! Skizzen 11b, Task 10: Die Texthöhe als Formel. Das Feld „Höhe“ des Textwerkzeugs nimmt Formeln
//! mit Parametern; OK legt die Formel in `text_formulas` (ein Rückgängig-Schritt). Kopieren und
//! Einfügen tragen die Formel mit, fehlt der Parameter, bleibt nur der Wert und die Meldung zählt.
//!
//! Die ganze App läuft ohne Fenster bei 1920×1080 mit den mitgelieferten Schriften.

mod common;

use eframe::App as _;
use egui::epaint::Shape;
use egui::{Event, Modifiers, PointerButton, Pos2, Rect, Vec2, pos2, vec2};
use planora_app::app::PlanoraApp;
use planora_app::commands;
use planora_app::keymap::Action;
use planora_app::sketch_state::{Item, Tool};
use planora_app::workspace::Workspace;
use planora_app::{fonts, theme};
use planora_feature::{ParamEdit, ParamUnit, SketchPlane, StandardPlane};
use planora_i18n::tr;
use planora_math::DVec2;
use planora_project::config::Config;
use planora_project::library::Library;
use planora_sketch::model::Sketch;
use planora_sketch::text::{HAlign, TextId, TextParams, VAlign};
use planora_sketch::text_shape::{TextShaper, TextStyle};
use planora_text::FontLibrary;
use std::sync::Arc;

const SIZE: Vec2 = vec2(1920.0, 1080.0);
const NONE: Modifiers = Modifiers::NONE;
const FRAME_A: Pos2 = pos2(800.0, 600.0);
const FRAME_B: Pos2 = pos2(1200.0, 800.0);

fn ws(name: &str) -> Workspace {
    let root = common::temp_root(name);
    let mut w = Workspace::new(Some(Library::open(&root).unwrap()), Config::default());
    w.config_path = root.join("config.toml");
    w
}

/// Parameter `name` mit dem Wert `value` (Länge).
fn param(w: &mut Workspace, name: &str, value: &str) {
    let s = &mut w.active_tab_mut().state;
    let id = s.add_parameter().unwrap();
    s.edit_parameter(
        id,
        ParamEdit {
            name: name.into(),
            formula: value.into(),
            unit: ParamUnit::Length,
            comment: String::new(),
        },
    )
    .unwrap();
}

fn open_sketch(w: &mut Workspace) {
    let s = &mut w.active_tab_mut().state;
    let id = s
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    s.open_sketch(id).unwrap();
    w.observe_history(false);
}

fn text_formulas(w: &mut Workspace) -> Vec<(TextId, String)> {
    let (_, sf) = w.active_tab_mut().state.session_draft_mut().unwrap();
    sf.text_formulas
        .iter()
        .map(|(t, f)| (*t, f.clone()))
        .collect()
}

fn sketch(w: &mut Workspace) -> &Sketch {
    w.active_tab().state.session_sketch().unwrap()
}

// ---- Text über die Oberfläche ---------------------------------------------------------------

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

struct Rig {
    ctx: egui::Context,
    app: PlanoraApp,
    time: f64,
}

impl Rig {
    fn new(w: Workspace) -> Rig {
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

    fn click(&mut self, p: Pos2) -> Shot {
        self.time += 1.0;
        let b = PointerButton::Primary;
        self.frame(vec![Event::PointerMoved(p)]);
        self.frame(vec![Event::PointerButton {
            pos: p,
            button: b,
            pressed: true,
            modifiers: NONE,
        }]);
        self.frame(vec![Event::PointerButton {
            pos: p,
            button: b,
            pressed: false,
            modifiers: NONE,
        }]);
        self.settle()
    }

    fn type_text(&mut self, text: &str) -> Shot {
        self.frame(vec![Event::Text(text.to_string())]);
        self.settle()
    }

    fn click_label(&mut self, label: &str) -> Shot {
        let at = self
            .settle()
            .texts
            .iter()
            .rfind(|(t, _)| t == label)
            .unwrap_or_else(|| panic!("„{label}“ fehlt"))
            .1
            .center();
        self.click(at)
    }
}

#[test]
fn height_field_takes_formula() {
    let mut w = ws("planora-sketch-text-formula");
    param(&mut w, "H", "10");
    open_sketch(&mut w);
    let mut rig = Rig::new(w);
    let steps = rig
        .app
        .ws_mut()
        .active_tab_mut()
        .state
        .sketch_history()
        .unwrap()
        .undo_len();
    commands::run_action(rig.app.ws_mut(), Action::SketchText);
    rig.settle();
    rig.click(FRAME_A);
    rig.click(FRAME_B);
    rig.type_text("Hallo");
    {
        let st = &mut rig.app.ws_mut().active_tab_mut().state;
        let scope = st.document.param_scope();
        let tool = st.sketch_session.as_mut().unwrap().text.as_mut().unwrap();
        tool.set_height_text("H/2", &scope);
        assert_eq!(tool.params.height, 5.0, "Wert der Formel");
    }
    rig.settle();
    rig.click_label(&tr!("common-ok"));
    let w = rig.app.ws_mut();
    assert_eq!(
        w.active_tab().state.sketch_session.as_ref().unwrap().tool,
        Tool::Select
    );
    let formulas = text_formulas(w);
    assert_eq!(formulas.len(), 1, "{formulas:?}");
    assert_eq!(formulas[0].1, "H/2");
    let (id, _) = formulas[0].clone();
    assert_eq!(sketch(w).text(id).unwrap().height, 5.0);
    let st = &w.active_tab().state;
    assert_eq!(
        st.sketch_history().unwrap().undo_len(),
        steps + 1,
        "ein Rückgängig-Schritt"
    );
}

// ---- Kopieren und Einfügen ------------------------------------------------------------------

/// Text „Ho“ mit der Höhe 5 und der Formel „H/2“, ein Buchstabe gewählt.
fn text_with_formula(w: &mut Workspace) {
    let lib = FontLibrary::bundled_only();
    let style = TextStyle {
        font: "Montserrat".into(),
        bold: false,
        italic: false,
        spacing: 0.0,
    };
    let (se, sf) = w.active_tab_mut().state.session_draft_mut().unwrap();
    let t = sf
        .sketch
        .add_text_box(
            DVec2::new(0.0, 0.0),
            DVec2::new(60.0, 30.0),
            TextParams {
                content: "Ho".into(),
                style: style.clone(),
                height: 5.0,
                flip_h: false,
                flip_v: false,
            },
            HAlign::Left,
            VAlign::Bottom,
            Arc::new(lib.shape("Ho", &style).unwrap()),
        )
        .unwrap();
    sf.text_formulas.insert(t, "H/2".into());
    let glyph = sf.sketch.text(t).unwrap().curves[0];
    se.selection = vec![Item::Curve(glyph)];
    w.active_tab_mut().state.observe_sketch(false);
    w.observe_history(false);
}

#[test]
fn paste_text_keeps_height_formula() {
    let mut w = ws("planora-sketch-text-formula-copy");
    param(&mut w, "H", "10");
    open_sketch(&mut w);
    text_with_formula(&mut w);
    commands::run_action(&mut w, Action::SketchCopy);
    let clip = w.sketch_clipboard.clone().expect("kopiert");
    assert_eq!(
        clip.formulas.len(),
        1,
        "die Höhenformel liegt in der Ablage"
    );
    w.active_tab_mut().state.close_sketch();
    w.observe_history(false);

    // Neue Skizze im selben Dokument: der Parameter H ist da, die Formel kommt mit.
    open_sketch(&mut w);
    commands::run_action(&mut w, Action::SketchPaste);
    let formulas = text_formulas(&mut w);
    assert_eq!(formulas.len(), 1, "{formulas:?}");
    assert_eq!(formulas[0].1, "H/2");
    assert_eq!(sketch(&mut w).text(formulas[0].0).unwrap().height, 5.0);
    w.active_tab_mut().state.close_sketch();
    w.observe_history(false);

    // Anderes Dokument ohne H: nur der Wert, die Meldung zählt es.
    let mut other = ws("planora-sketch-text-formula-other");
    other.sketch_clipboard = Some(clip);
    open_sketch(&mut other);
    commands::run_action(&mut other, Action::SketchPaste);
    assert!(
        text_formulas(&mut other).is_empty(),
        "keine Formel ohne Parameter"
    );
    let (_, t) = sketch(&mut other).texts().next().expect("Text eingefügt");
    assert_eq!(t.height, 5.0, "der letzte Wert");
    let msg = other
        .active_tab()
        .state
        .sketch_session
        .as_ref()
        .unwrap()
        .message_text()
        .unwrap();
    assert!(msg.contains("1 Formel"), "{msg}");
}

#[test]
fn editing_keeps_the_formula_until_the_height_is_typed() {
    let mut w = ws("planora-sketch-text-formula-edit");
    param(&mut w, "H", "10");
    open_sketch(&mut w);
    text_with_formula(&mut w);
    let lib = FontLibrary::bundled_only();
    let id = text_formulas(&mut w)[0].0;

    // Öffnen und OK ohne das Feld anzufassen: die Formel bleibt, das Feld zeigt sie.
    let scope = w.active_tab().state.document.param_scope();
    let (se, sf) = w.active_tab_mut().state.session_draft_mut().unwrap();
    assert!(se.begin_text_edit(&mut sf.sketch, id));
    let tool = se.text.as_mut().unwrap();
    tool.show_height_formula(sf.text_formulas.get(&id).map(String::as_str), &scope);
    assert_eq!(tool.height_text, "H/2");
    se.confirm_text(sf, Some(&lib)).unwrap();
    assert_eq!(sf.text_formulas.get(&id).map(String::as_str), Some("H/2"));

    // Eine getippte Zahl löst die Formel.
    assert!(se.begin_text_edit(&mut sf.sketch, id));
    se.text.as_mut().unwrap().set_height_text("7", &scope);
    se.confirm_text(sf, Some(&lib)).unwrap();
    assert!(sf.text_formulas.is_empty(), "Zahl ersetzt die Formel");
    assert_eq!(sf.sketch.text(id).unwrap().height, 7.0);
}
