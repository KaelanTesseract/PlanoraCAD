//! Formelfelder und Zahlenfelder in der Anzeigeeinheit Zoll (Teilprojekt Zoll, Aufgabe 4):
//! Felder zeigen Werte in Zoll mit dem Zeichen „in“, eine getippte reine Zahl zählt in Zoll und
//! wird als „… in“ gespeichert, ein unverändert bestätigtes Feld behält seinen gespeicherten
//! Text zeichengleich, und das Umstellen der Einstellung ändert keinen Wert. Die Einheit gilt
//! je Test über `with_unit` (nur dieser Thread).

mod common;

use eframe::App as _;
use egui::{Event, Key, Modifiers, PointerButton, Pos2, Rect, Vec2, vec2};
use planora_app::app::PlanoraApp;
use planora_app::blend_dialog::BlendDialog;
use planora_app::create_dialog::CreateDialog;
use planora_app::feature_dialog::FeatureDialog;
use planora_app::formula_field;
use planora_app::history_bar;
use planora_app::parameter_dialog::{Column, ParameterDialog};
use planora_app::section_tool;
use planora_app::sketch_panel;
use planora_app::sketch_state::{ConstraintAction, Item};
use planora_app::state::*;
use planora_app::workspace::*;
use planora_app::{fonts, theme};
use planora_feature::{
    BlendArt, FeatureId, FeatureKind, FieldKey, Formulas, ParamEdit, ParamId, ParamUnit,
    SketchPlane,
};
use planora_i18n::{DisplayUnit, with_unit};
use planora_math::DVec3;
use planora_project::config::Config;
use planora_project::library::Library;
use planora_sketch::constraint::{ConstraintId, ConstraintKind as K};

const ZOLL: DisplayUnit = DisplayUnit::Zoll;

// ---- Hilfen ---------------------------------------------------------------------------------

#[derive(Clone, Debug)]
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

fn texts(out: egui::FullOutput) -> Vec<Seen> {
    let mut seen = Vec::new();
    for c in &out.shapes {
        walk(&c.shape, &mut seen);
    }
    out.drop_without_applying_deltas();
    seen
}

fn find<'a>(seen: &'a [Seen], text: &str) -> Option<&'a Seen> {
    seen.iter().rfind(|s| s.text == text)
}

fn has(seen: &[Seen], text: &str) -> bool {
    find(seen, text).is_some()
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
        modifiers: Modifiers::NONE,
    }
}

fn param(s: &mut AppState, name: &str, formula: &str, unit: ParamUnit) -> ParamId {
    let id = s.add_parameter().unwrap();
    s.edit_parameter(
        id,
        ParamEdit {
            name: name.into(),
            formula: formula.into(),
            unit,
            comment: String::new(),
        },
    )
    .unwrap();
    id
}

fn formulas(pairs: &[(FieldKey, &str)]) -> Formulas {
    pairs.iter().map(|(k, t)| (*k, t.to_string())).collect()
}

fn feature_formulas(s: &AppState, id: FeatureId) -> Formulas {
    s.document.feature(id).unwrap().formulas().clone()
}

fn box_size(s: &AppState, id: FeatureId) -> DVec3 {
    match &s.document.feature(id).unwrap().kind {
        FeatureKind::Box { size, .. } => *size,
        k => panic!("kein Quader: {k:?}"),
    }
}

fn ws() -> Workspace {
    let root = common::temp_root("planora-units-formula");
    Workspace::new(Some(Library::open(&root).unwrap()), Config::default())
}

const SIZE: Vec2 = vec2(1920.0, 1080.0);

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

    fn tab(&mut self) -> &mut DocTab {
        self.app.ws_mut().active_tab_mut()
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
        texts(self.ctx.run_ui(raw, |ui| app.ui(ui, &mut f)))
    }

    fn settle(&mut self) -> Vec<Seen> {
        for _ in 0..5 {
            self.frame(vec![]);
        }
        self.frame(vec![])
    }

    fn click(&mut self, p: Pos2) {
        self.frame(vec![Event::PointerMoved(p)]);
        self.frame(vec![button(p, true)]);
        self.frame(vec![button(p, false)]);
        self.frame(vec![]);
    }

    fn in_window(&mut self, title: &str, text: &str) -> Option<Rect> {
        let seen = self.settle();
        let t = find(&seen, title)?.rect;
        seen.iter()
            .rfind(|s| s.text == text && s.rect.top() > t.top() - 1.0)
            .map(|s| s.rect)
    }

    /// Tippt `text` in das Feld mit dem Text `old` im Fenster `title` (alles ersetzt).
    fn retype(&mut self, title: &str, old: &str, text: &str) {
        let r = self
            .in_window(title, old)
            .unwrap_or_else(|| panic!("Feld „{old}“ fehlt"));
        self.click(r.center());
        self.frame(vec![key(Key::A, Modifiers::COMMAND)]);
        self.frame(vec![Event::Text(text.into())]);
        self.frame(vec![]);
    }
}

// ---- Quader erzeugen in Zoll ------------------------------------------------------------------

#[test]
fn a_box_created_in_inches_stores_the_inch_formula() {
    let mut w = ws();
    w.observe_history(false);
    let mut rig = Rig::new(w);
    let title = "Quader erzeugen";
    let f = with_unit(ZOLL, || {
        rig.tab().dialog = Some(CreateDialog::Box {
            x: 11.0,
            y: 12.0,
            z: 13.0,
        });
        let seen = rig.settle();
        // Vorgaben in Zoll (11 mm, 12 mm, 13 mm) mit dem Zeichen „in“, nie „mm“.
        for shown in ["0,433071", "0,472441", "0,511811"] {
            assert!(has(&seen, shown), "Vorgabe „{shown}“ fehlt");
        }
        assert!(has(&seen, "in"), "Zeichen „in“ hinter den Feldern");
        assert!(!has(&seen, "mm"), "kein „mm“ im Fenster");
        rig.retype(title, "0,433071", "2");
        let erzeugen = rig.in_window(title, "Erzeugen").expect("Knopf „Erzeugen“");
        rig.click(erzeugen.center());
        rig.settle();
        let tab = rig.tab();
        assert!(tab.dialog.is_none(), "nach Erfolg geschlossen");
        tab.state.document.features()[0].id
    });
    let s = &rig.tab().state;
    // 2 in = 50,8 mm; die unveränderten Felder bleiben bitgleich bei ihrer Vorgabe.
    assert_eq!(box_size(s, f), DVec3::new(50.8, 12.0, 13.0));
    assert_eq!(
        feature_formulas(s, f),
        formulas(&[(FieldKey::Breite, "2 in")])
    );
    // In mm zeigt das Feld die gespeicherte Formel, der Körper bleibt gleich.
    let d = FeatureDialog::new(s, f).unwrap();
    assert_eq!(d.text("Breite"), Some("2 in"));
    assert_eq!(d.text("Tiefe"), Some("12"));
}

// ---- Gespeichertes „50“ ---------------------------------------------------------------------

#[test]
fn a_stored_fifty_shows_in_inches_and_stays_when_confirmed_unchanged() {
    let mut s = AppState::new();
    let f = s
        .create_box_with(
            DVec3::new(50.0, 20.0, 10.0),
            formulas(&[(FieldKey::Breite, "50")]),
        )
        .unwrap();
    with_unit(ZOLL, || {
        let d = FeatureDialog::new(&s, f).unwrap();
        assert_eq!(d.text("Breite"), Some("1,968504"));
        assert_eq!(d.text("Tiefe"), Some("0,787402"));
        d.apply(&mut s).unwrap();
    });
    assert_eq!(
        feature_formulas(&s, f),
        formulas(&[(FieldKey::Breite, "50")]),
        "unverändert bestätigt: zeichengleich"
    );
    assert_eq!(box_size(&s, f), DVec3::new(50.0, 20.0, 10.0));
    let d = FeatureDialog::new(&s, f).unwrap();
    assert_eq!(d.text("Breite"), Some("50"), "in mm wie gespeichert");
    assert_eq!(d.text("Tiefe"), Some("20"));
}

#[test]
fn the_edit_window_shows_inch_values_with_in() {
    let mut w = ws();
    let f = {
        let s = &mut w.active_tab_mut().state;
        s.create_box_with(
            DVec3::new(50.0, 20.0, 10.0),
            formulas(&[(FieldKey::Breite, "50")]),
        )
        .unwrap()
    };
    let mut rig = Rig::new(w);
    with_unit(ZOLL, || {
        let tab = rig.tab();
        tab.feature_dialog = FeatureDialog::new(&tab.state, f);
        let seen = rig.settle();
        assert!(has(&seen, "1,968504"));
        assert!(has(&seen, "in"));
        assert!(!has(&seen, "mm"), "kein „mm“ hinter einem Zollwert");
    });
}

#[test]
fn a_changed_formula_is_rewritten_and_marks_bare_millimetres() {
    let mut s = AppState::new();
    param(&mut s, "Breite", "40", ParamUnit::Length);
    let f = s
        .create_box_with(
            DVec3::new(45.0, 20.0, 10.0),
            formulas(&[(FieldKey::Breite, "Breite + 5")]),
        )
        .unwrap();
    with_unit(ZOLL, || {
        let mut d = FeatureDialog::new(&s, f).unwrap();
        assert_eq!(d.text("Breite"), Some("Breite + 5 mm"));
        assert!(d.set_text("Breite", "Breite + 1"));
        d.apply(&mut s).unwrap();
    });
    assert_eq!(
        feature_formulas(&s, f),
        formulas(&[(FieldKey::Breite, "Breite + 1 in")])
    );
    assert!((box_size(&s, f).x - 65.4).abs() < 1e-9);
}

// ---- Fenster „Parameter“ ------------------------------------------------------------------

#[test]
fn parameter_rows_show_inches_and_keep_unchanged_text() {
    let mut s = AppState::new();
    let id = param(&mut s, "Breite", "50", ParamUnit::Length);
    with_unit(ZOLL, || {
        let mut d = ParameterDialog::new(&s);
        assert_eq!(d.text(id, Column::Formula), Some("1,968504"));
        d.commit(id, &mut s).unwrap();
        assert_eq!(s.document.parameter(id).unwrap().formula, "50");
        d.set_text(id, Column::Formula, "2");
        d.commit(id, &mut s).unwrap();
    });
    assert_eq!(s.document.parameter(id).unwrap().formula, "2 in");
    assert_eq!(s.document.parameter_value(id), Some(50.8));
    let d = ParameterDialog::new(&s);
    assert_eq!(d.text(id, Column::Formula), Some("2 in"));
}

// ---- Verrunden ------------------------------------------------------------------------------

#[test]
fn blend_sizes_count_in_inches() {
    let s = AppState::new();
    let scope = s.document.param_scope();
    let mut d = BlendDialog::fillet();
    d.first = "0,1".into();
    let (art, f) = with_unit(ZOLL, || d.art_formulas(&scope)).unwrap();
    assert_eq!(art, BlendArt::Rundung { radius: 0.1 * 25.4 });
    assert_eq!(f, formulas(&[(FieldKey::Radius, "0,1 in")]));
    let mut v = BlendDialog::variable();
    v.points = vec![("0".into(), "0,1".into()), ("100".into(), "0,2".into())];
    let (art, _) = with_unit(ZOLL, || v.art_formulas(&scope)).unwrap();
    assert_eq!(
        art,
        BlendArt::RundungVariabel {
            punkte: vec![(0.0, 0.1 * 25.4), (100.0, 0.2 * 25.4)]
        }
    );
}

#[test]
fn untouched_defaults_in_inches_are_exactly_the_millimetre_defaults() {
    let s = AppState::new();
    let scope = s.document.param_scope();
    with_unit(ZOLL, || {
        // Verrunden: Vorgabe 1 mm, gezeigt in Zoll, unverändert genau 1 mm ohne Formel.
        let d = BlendDialog::fillet();
        assert_eq!(d.first, "0,03937");
        let (art, f) = d.art_formulas(&scope).unwrap();
        assert_eq!(art, BlendArt::Rundung { radius: 1.0 });
        assert!(f.is_empty(), "keine Formel für die Vorgabe");
        let v = BlendDialog::variable();
        let (art, _) = v.art_formulas(&scope).unwrap();
        assert_eq!(
            art,
            BlendArt::RundungVariabel {
                punkte: vec![(0.0, 1.0), (100.0, 1.0)]
            }
        );
        // Neuer Punkt: die alten Texte bleiben, der neue Radius ist genau 1 mm.
        let mut v = BlendDialog::variable();
        v.add_point();
        assert_eq!(v.points.len(), 3);
        assert_eq!(v.points[0].1, "0,03937");
        let (art, _) = v.art_formulas(&scope).unwrap();
        match art {
            BlendArt::RundungVariabel { punkte } => {
                assert_eq!(punkte[1].0, 50.0);
                assert!((punkte[1].1 - 1.0).abs() < 1e-12, "{}", punkte[1].1);
            }
            a => panic!("{a:?}"),
        }
        // Muster: Vorgabe 20 mm / 0, unverändert genau.
        assert_eq!(
            sketch_panel::pattern_offsets("0,787402", "0"),
            (Some(20.0), Some(0.0))
        );
        assert_eq!(
            sketch_panel::pattern_offsets("1", "0"),
            (Some(25.4), Some(0.0))
        );
    });
    // In mm wie bisher.
    assert_eq!(BlendDialog::fillet().first, "1");
    assert_eq!(
        sketch_panel::pattern_offsets("20", "0"),
        (Some(20.0), Some(0.0))
    );
}

// ---- Skizzenmaß -----------------------------------------------------------------------------

fn sketch_rig() -> (Rig, ConstraintId) {
    let mut w = ws();
    let dim = {
        let s = &mut w.active_tab_mut().state;
        let sketch = s
            .add_sketch(SketchPlane::Standard(StandardPlane::XY))
            .unwrap();
        s.open_sketch(sketch).unwrap();
        let (se, sf) = s.session_draft_mut().unwrap();
        let sk = &mut sf.sketch;
        let a = sk.add_point(0.0, 0.0).unwrap();
        let b = sk.add_point(30.0, 0.0).unwrap();
        let line = sk.add_line(a, b).unwrap();
        sk.add_constraint(K::Fixed(a, 0.0, 0.0)).unwrap();
        sk.add_constraint(K::Horizontal(
            planora_sketch::constraint::LineOrPoints::Line(line),
        ))
        .unwrap();
        se.selection = vec![Item::Curve(line)];
        let dim = se
            .apply(sk, ConstraintAction::Distance, Some(30.0))
            .unwrap();
        se.selection.clear();
        s.observe_sketch(false);
        dim
    };
    (Rig::new(w), dim)
}

fn draft_value(s: &AppState, id: ConstraintId) -> f64 {
    match &s.session_sketch().unwrap().constraint(id).unwrap().kind {
        K::Distance(_, _, v) => *v,
        k => panic!("{k:?}"),
    }
}

#[test]
fn a_sketch_dimension_of_one_inch_is_25_4_mm() {
    let (mut rig, dim) = sketch_rig();
    with_unit(ZOLL, || {
        let ctx = rig.ctx.clone();
        sketch_panel::open_existing_dimension_editor(&ctx, &rig.tab().state, dim);
        let seen = rig.settle();
        assert!(has(&seen, "1,181102"), "30 mm in Zoll");
        assert!(has(&seen, "in"));
        rig.frame(vec![key(Key::A, Modifiers::COMMAND)]);
        rig.frame(vec![Event::Text("1".into())]);
        rig.frame(vec![key(Key::Enter, Modifiers::NONE)]);
        rig.settle();
        assert!(!sketch_panel::dimension_editor_open(&ctx), "übernommen");
    });
    let s = &rig.tab().state;
    assert_eq!(draft_value(s, dim), 25.4);
    assert_eq!(
        s.sketch_draft
            .as_ref()
            .unwrap()
            .formulas
            .get(&dim)
            .map(String::as_str),
        Some("1 in")
    );
}

#[test]
fn a_sketch_dimension_confirmed_unchanged_keeps_its_value() {
    let (mut rig, dim) = sketch_rig();
    with_unit(ZOLL, || {
        let ctx = rig.ctx.clone();
        sketch_panel::open_existing_dimension_editor(&ctx, &rig.tab().state, dim);
        rig.settle();
        rig.frame(vec![key(Key::Enter, Modifiers::NONE)]);
        rig.settle();
    });
    let s = &rig.tab().state;
    assert_eq!(draft_value(s, dim), 30.0, "kein Rundungsverlust");
    assert!(!s.sketch_draft.as_ref().unwrap().formulas.contains_key(&dim));
}

// ---- Formelfeld-Bausteine --------------------------------------------------------------------

#[test]
fn field_helpers_follow_the_unit() {
    with_unit(ZOLL, || {
        assert_eq!(ParamUnit::Length.suffix(), "in");
        assert_eq!(
            formula_field::unit_value(25.4, ParamUnit::Length),
            "1,000 in"
        );
        assert_eq!(
            formula_field::value_text(50.8, ParamUnit::Length),
            "= 2,000 in"
        );
        assert_eq!(formula_field::unit_value(45.0, ParamUnit::Angle), "45,00°");
        assert_eq!(section_tool::offset_text(25.4), "1");
    });
    assert_eq!(ParamUnit::Length.suffix(), "mm");
    assert_eq!(
        formula_field::unit_value(40.0, ParamUnit::Length),
        "40,00 mm"
    );
    assert_eq!(section_tool::offset_text(12.5), "12,5");
}

// ---- Verlaufsleiste -------------------------------------------------------------------------

#[test]
fn the_history_hover_sorts_measures_in_any_unit() {
    assert!(history_bar::is_measure("12,50 mm"));
    assert!(history_bar::is_measure("0,492 in"));
    assert!(history_bar::is_measure("45°"));
    assert!(!history_bar::is_measure("Ja"));
    assert!(!history_bar::is_measure("Main"));
    let mut s = AppState::new();
    let f = s
        .create_box_with(DVec3::new(10.0, 20.0, 30.0), Formulas::new())
        .unwrap();
    let kind = s.document.feature(f).unwrap().kind.clone();
    let labels = |s: &AppState| -> Vec<String> {
        history_bar::hover_rows(s, &kind)
            .into_iter()
            .map(|(l, _)| l)
            .collect()
    };
    let mm = labels(&s);
    let zoll = with_unit(ZOLL, || labels(&s));
    assert_eq!(mm, zoll, "gleiche Reihenfolge in Zoll und mm");
}
