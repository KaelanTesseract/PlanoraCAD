//! Skizzen 11b, Task 11: Parameter im Text. Ein Text „{Breite} mm“ zeigt den Wert des Parameters;
//! nach einer Änderung der Parametertabelle, nach Rückgängig/Wiederholen einer solchen und nach
//! einem Wechsel der Längeneinheit setzt der Tab seine Texte im nächsten Bild neu, ohne eigenen
//! Rückgängig-Schritt. Die offene Skizze zeigt den ersetzten Inhalt, das Werkzeug den Rohinhalt.
//!
//! Die ganze App läuft ohne Fenster bei 1920×1080 mit den mitgelieferten Schriften.

mod common;

use eframe::App as _;
use egui::epaint::Shape;
use egui::{Event, Modifiers, PointerButton, Pos2, Rect, Vec2, pos2, vec2};
use planora_app::app::PlanoraApp;
use planora_app::commands;
use planora_app::keymap::Action;
use planora_app::workspace::Workspace;
use planora_app::{fonts, theme};
use planora_feature::{
    FeatureKind, FeatureStatus, ParamEdit, ParamId, ParamUnit, SketchPlane, StandardPlane,
};
use planora_i18n::{DisplayUnit, fmt_len_num, tr};
use planora_math::DVec2;
use planora_project::config::Config;
use planora_project::library::Library;
use planora_sketch::model::Sketch;
use planora_sketch::text::TextId;
use std::sync::Mutex;

const SIZE: Vec2 = vec2(1920.0, 1080.0);
const NONE: Modifiers = Modifiers::NONE;
const FRAME_A: Pos2 = pos2(800.0, 600.0);
const FRAME_B: Pos2 = pos2(1200.0, 800.0);

/// Ein Test wechselt die globale Längeneinheit; alle Tests dieser Datei laufen darum nacheinander.
static SERIAL: Mutex<()> = Mutex::new(());

fn ws(name: &str) -> Workspace {
    let root = common::temp_root(name);
    let mut w = Workspace::new(Some(Library::open(&root).unwrap()), Config::default());
    w.config_path = root.join("config.toml");
    w
}

fn length(w: &mut Workspace, name: &str, value: &str) -> ParamId {
    let s = &mut w.active_tab_mut().state;
    let id = s.add_parameter().unwrap();
    edit(w, id, name, value);
    w.observe_history(false);
    id
}

fn edit(w: &mut Workspace, id: ParamId, name: &str, value: &str) {
    w.active_tab_mut()
        .state
        .edit_parameter(
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

    fn ws(&mut self) -> &mut Workspace {
        self.app.ws_mut()
    }
}

/// Die erste Skizze des Dokuments (Definition).
fn def(w: &Workspace) -> &Sketch {
    w.active_tab()
        .state
        .document
        .features()
        .iter()
        .find_map(|f| match &f.kind {
            FeatureKind::Sketch(sf) => Some(&sf.sketch),
            _ => None,
        })
        .unwrap()
}

fn text_id(sk: &Sketch) -> TextId {
    sk.texts().next().unwrap().0
}

fn points(sk: &Sketch) -> Vec<DVec2> {
    let t = text_id(sk);
    sk.text(t)
        .unwrap()
        .points
        .iter()
        .map(|p| sk.point_xy(*p).unwrap())
        .collect()
}

fn shown(sk: &Sketch) -> Option<String> {
    sk.text(text_id(sk)).unwrap().shown.clone()
}

fn content(sk: &Sketch) -> String {
    sk.text(text_id(sk)).unwrap().content.clone()
}

fn undo_len(w: &Workspace) -> usize {
    w.active_tab().history().undo_len()
}

/// Parameter `Breite` = 20, eine Skizze mit dem Text „{Breite} mm“, über das Werkzeug „Text“
/// getippt; die Skizze ist danach wieder zu. Liefert die Kennung des Parameters.
fn text_rig(name: &str) -> (Rig, ParamId) {
    let mut w = ws(name);
    let p = length(&mut w, "Breite", "20");
    {
        let s = &mut w.active_tab_mut().state;
        let id = s
            .add_sketch(SketchPlane::Standard(StandardPlane::XY))
            .unwrap();
        s.open_sketch(id).unwrap();
    }
    w.observe_history(false);
    let mut rig = Rig::new(w);
    commands::run_action(rig.ws(), Action::SketchText);
    rig.settle();
    rig.click(FRAME_A);
    rig.click(FRAME_B);
    rig.frame(vec![Event::Text("{Breite} mm".into())]);
    rig.settle();
    rig.click_label(&tr!("common-ok"));
    {
        // In der offenen Skizze: Rohinhalt, gezeigt wird der Wert.
        let sk = rig.ws().active_tab().state.session_sketch().unwrap();
        assert_eq!(content(sk), "{Breite} mm");
        assert_eq!(shown(sk), Some(format!("{} mm", fmt_len_num(20.0))));
    }
    rig.ws().active_tab_mut().state.close_sketch();
    rig.ws().observe_history(false);
    rig.settle();
    (rig, p)
}

#[test]
fn parameter_change_reshapes_text() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let (mut rig, p) = text_rig("planora-text-params-change");
    let w = rig.ws();
    assert_eq!(shown(def(w)), Some(format!("{} mm", fmt_len_num(20.0))));
    let before = points(def(w));
    let steps = undo_len(w);
    edit(w, p, "Breite", "30");
    rig.frame(vec![]);
    let w = rig.ws();
    assert_eq!(shown(def(w)), Some(format!("{} mm", fmt_len_num(30.0))));
    assert_eq!(content(def(w)), "{Breite} mm");
    assert_ne!(points(def(w)), before, "Kurven neu gesetzt");
    rig.settle();
    let w = rig.ws();
    assert_eq!(
        undo_len(w),
        steps + 1,
        "nur der Schritt der Parameteränderung"
    );
    assert_eq!(
        w.active_tab().state.document.features()[0].status,
        FeatureStatus::Ok
    );
}

#[test]
fn undo_parameter_change_reshapes_back() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let (mut rig, p) = text_rig("planora-text-params-undo");
    let before = points(def(rig.ws()));
    edit(rig.ws(), p, "Breite", "30");
    rig.settle();
    let after = points(def(rig.ws()));
    assert_ne!(after, before);
    rig.ws().active_tab_mut().undo().unwrap();
    rig.settle();
    let w = rig.ws();
    assert_eq!(shown(def(w)), Some(format!("{} mm", fmt_len_num(20.0))));
    assert_eq!(points(def(w)), before);
    rig.ws().active_tab_mut().redo().unwrap();
    rig.settle();
    let w = rig.ws();
    assert_eq!(shown(def(w)), Some(format!("{} mm", fmt_len_num(30.0))));
    assert_eq!(points(def(w)), after);
}

#[test]
fn open_sketch_shows_substituted() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let (mut rig, p) = text_rig("planora-text-params-open");
    edit(rig.ws(), p, "Breite", "30");
    rig.settle();
    let want = points(def(rig.ws()));
    {
        let s = &mut rig.ws().active_tab_mut().state;
        let id = s.document.features()[0].id;
        s.open_sketch(id).unwrap();
    }
    rig.settle();
    let w = rig.ws();
    let sk = w.active_tab().state.session_sketch().unwrap();
    assert_eq!(shown(sk), Some(format!("{} mm", fmt_len_num(30.0))));
    assert_eq!(points(sk), want);
    // Ändern zeigt im Werkzeug den Rohinhalt.
    let st = &mut w.active_tab_mut().state;
    let t = text_id(st.session_sketch().unwrap());
    let (se, sf) = st.session_draft_mut().unwrap();
    assert!(se.begin_text_edit(&mut sf.sketch, t));
    assert_eq!(se.text.as_ref().unwrap().params.content, "{Breite} mm");
}

#[test]
fn unit_change_reshapes_text() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let (mut rig, _) = text_rig("planora-text-params-unit");
    let before = points(def(rig.ws()));
    rig.ws().set_length_unit(DisplayUnit::Zoll);
    rig.settle();
    let inch = shown(def(rig.ws()));
    let moved = points(def(rig.ws()));
    rig.ws().set_length_unit(DisplayUnit::Millimeter);
    rig.settle();
    assert_eq!(
        inch,
        Some(format!("{} mm", planora_i18n::fmt_num(20.0 / 25.4, 3)))
    );
    assert_ne!(moved, before);
    assert_eq!(points(def(rig.ws())), before);
}
