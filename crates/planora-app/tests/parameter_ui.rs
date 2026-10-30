//! Formelfeld und Fenster „Parameter“ (Aufgabe 9): „fx“, Wertanzeige und Fehler im Feld,
//! Übernahme bei Enter oder Verlassen des Feldes, Löschen mit Rückfrage und „Durch den Wert
//! ersetzen“, Umbenennen, Fehler bleiben in der Zeile, Sperren, Rückgängig bei offenem Fenster,
//! Größe bei 800 × 600 und 1100 × 700 in beiden Themen. Mit echten Bildern (`run_ui`) und
//! Mausklicks; die ganze App über `PlanoraApp::for_tests`.

mod common;

use eframe::App as _;
use egui::{Event, Key, Modifiers, PointerButton, Pos2, Rect, Vec2, vec2};
use planora_app::app::{PlanoraApp, parameters_entry};
use planora_app::feature_dialog::FeatureDialog;
use planora_app::formula_field::*;
use planora_app::icons::{self, Icon};
use planora_app::parameter_dialog::{self, *};
use planora_app::state::*;
use planora_app::workspace::*;
use planora_app::{fonts, theme};
use planora_feature::{
    FeatureId, FeatureKind, FieldKey, Formulas, ParamEdit, ParamId, ParamUnit, SketchPlane,
    StandardPlane,
};
use planora_math::DVec3;
use planora_project::config::Config;
use planora_project::library::Library;

// ---- Hilfen ---------------------------------------------------------------------------------

/// Ein gezeichneter Text mit Rechteck, Ausschnitt und Farbe.
#[derive(Clone, Debug)]
struct Seen {
    text: String,
    rect: Rect,
    clip: Rect,
    color: egui::Color32,
    opacity: f32,
    elided: bool,
}

fn walk(shape: &egui::epaint::Shape, clip: Rect, out: &mut Vec<Seen>) {
    match shape {
        egui::epaint::Shape::Vec(v) => v.iter().for_each(|s| walk(s, clip, out)),
        egui::epaint::Shape::Text(t) => {
            let section = t.galley.job.sections.first();
            let mut color = section.map_or(t.fallback_color, |x| x.format.color);
            if color == egui::Color32::PLACEHOLDER {
                color = t.fallback_color;
            }
            if let Some(c) = t.override_text_color {
                color = c;
            }
            out.push(Seen {
                text: t.galley.text().to_string(),
                rect: Rect::from_min_size(t.pos, t.galley.size()),
                clip,
                color,
                opacity: t.opacity_factor,
                elided: t.galley.elided,
            })
        }
        _ => {}
    }
}

fn texts(out: egui::FullOutput) -> Vec<Seen> {
    let mut seen = Vec::new();
    for c in &out.shapes {
        walk(&c.shape, c.clip_rect, &mut seen);
    }
    out.drop_without_applying_deltas();
    seen
}

fn find<'a>(seen: &'a [Seen], text: &str) -> Option<&'a Seen> {
    seen.iter().find(|s| s.text == text)
}

fn visible(seen: &[Seen], text: &str, inside: Rect) -> bool {
    find(seen, text)
        .is_some_and(|s| s.clip.contains_rect(s.rect) && inside.expand(0.5).contains_rect(s.rect))
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

/// Bilder mit einem eigenen Kontext, der Inhalt in einer Fläche über das ganze Bild.
struct Pane {
    ctx: egui::Context,
    time: f64,
    size: Vec2,
}

impl Pane {
    fn new(dark: bool) -> Pane {
        let ctx = egui::Context::default();
        ctx.set_fonts(fonts::definitions());
        theme::apply(&ctx, dark);
        Pane {
            ctx,
            time: 0.0,
            size: vec2(800.0, 600.0),
        }
    }

    /// Ein Bild; `f` bekommt die Fläche.
    fn frame(&mut self, events: Vec<Event>, f: &mut dyn FnMut(&mut egui::Ui)) -> Vec<Seen> {
        self.time += 0.05;
        let raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, self.size)),
            events,
            time: Some(self.time),
            ..Default::default()
        };
        texts(self.ctx.run_ui(raw, |ui| f(ui)))
    }

    fn settle(&mut self, f: &mut dyn FnMut(&mut egui::Ui)) -> Vec<Seen> {
        self.frame(vec![], f);
        self.frame(vec![], f);
        self.frame(vec![], f)
    }

    fn click(&mut self, p: Pos2, f: &mut dyn FnMut(&mut egui::Ui)) {
        self.frame(vec![Event::PointerMoved(p)], f);
        self.frame(vec![button(p, true)], f);
        self.frame(vec![button(p, false)], f);
        self.frame(vec![], f);
    }
}

/// Neuer Parameter über die App: Name, Formel, Art.
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

fn box_width(s: &AppState, id: FeatureId) -> f64 {
    match &s.document.feature(id).unwrap().kind {
        FeatureKind::Box { size, .. } => size.x,
        k => panic!("kein Quader: {k:?}"),
    }
}

fn ws() -> Workspace {
    let root = common::temp_root("planora-parameter-ui");
    Workspace::new(Some(Library::open(&root).unwrap()), Config::default())
}

/// Breite = 40 und ein Quader mit der Breite-Formel „Breite“; beides je ein Schritt.
fn ws_with_box() -> (Workspace, ParamId, FeatureId) {
    let mut w = ws();
    let s = &mut w.active_tab_mut().state;
    let breite = param(s, "Breite", "40", ParamUnit::Length);
    let f = s
        .create_box_with(
            DVec3::new(40.0, 20.0, 10.0),
            formulas(&[(FieldKey::Breite, "Breite")]),
        )
        .unwrap();
    w.observe_history(false);
    (w, breite, f)
}

// ---- Ganze App -------------------------------------------------------------------------------

const SIZE: Vec2 = vec2(1400.0, 900.0);

struct Rig {
    ctx: egui::Context,
    app: PlanoraApp,
    time: f64,
    size: Vec2,
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
            size: SIZE,
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
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, self.size)),
            time: Some(self.time),
            events,
            ..Default::default()
        };
        let mut f = eframe::Frame::_new_kittest();
        let app = &mut self.app;
        texts(self.ctx.run_ui(raw, |ui| app.ui(ui, &mut f)))
    }

    fn settle(&mut self) -> Vec<Seen> {
        self.frame(vec![]);
        self.frame(vec![]);
        self.frame(vec![])
    }

    fn click(&mut self, p: Pos2) {
        self.frame(vec![Event::PointerMoved(p)]);
        self.frame(vec![button(p, true)]);
        self.frame(vec![button(p, false)]);
        self.frame(vec![]);
    }

    fn dialog_rect(&mut self) -> Rect {
        self.tab()
            .parameter_dialog
            .as_ref()
            .and_then(|d| d.last_rect)
            .expect("Fenster „Parameter“ gezeichnet")
    }

    /// Klickt in das Feld mit dem Text `text` im Fenster und markiert alles.
    fn focus_field(&mut self, text: &str) {
        let seen = self.settle();
        let rect = self.dialog_rect();
        // Das letzte passende: Fenster liegen über der Ansicht und werden danach gezeichnet.
        let field = seen
            .iter()
            .rfind(|s| s.text == text && rect.contains_rect(s.rect))
            .unwrap_or_else(|| panic!("Feld mit „{text}“ fehlt"))
            .rect;
        self.click(field.center());
        self.frame(vec![key(Key::A, Modifiers::COMMAND)]);
    }

    fn type_text(&mut self, text: &str) {
        self.frame(vec![Event::Text(text.into())]);
        self.frame(vec![]);
    }
}

// ---- (a) -------------------------------------------------------------------------------------

#[test]
fn classify_and_texts() {
    assert_eq!(classify("12,5"), Some(FieldInput::Number(12.5)));
    assert_eq!(
        classify(" Breite/2 "),
        Some(FieldInput::Formula("Breite/2".into()))
    );
    assert_eq!(classify("  "), None);
    // Zahlen zuerst: „1e3“ ist eine Zahl, keine Formel.
    assert_eq!(classify("1e3"), Some(FieldInput::Number(1000.0)));
    assert_eq!(value_text(20.0, ParamUnit::Length), "= 20,00 mm");
    assert_eq!(value_text(45.0, ParamUnit::Angle), "= 45,00°");
    assert_eq!(value_text(1.5, ParamUnit::Number), "= 1,50");
    // Winkel aus Bogenmaß zurückgerechnet: gerundet, nicht „30,000000000000004“.
    assert_eq!(
        value_text(30f64.to_radians().to_degrees(), ParamUnit::Angle),
        "= 30,00°"
    );

    let mut s = AppState::new();
    assert_eq!(names_hint(&s.document.param_scope()), None);
    for i in 1..=8 {
        param(&mut s, &format!("p{i}"), "1", ParamUnit::Number);
    }
    assert_eq!(
        names_hint(&s.document.param_scope()).unwrap(),
        "Namen: p1, p2, p3, p4, p5, p6, p7, p8",
        "genau acht: ohne „…“"
    );
    param(&mut s, "p9", "1", ParamUnit::Number);
    param(&mut s, "p10", "1", ParamUnit::Number);
    let hint = names_hint(&s.document.param_scope()).unwrap();
    assert_eq!(hint, "Namen: p1, p2, p3, p4, p5, p6, p7, p8 …");
    assert!(!hint.contains("p9"));

    let scope = s.document.param_scope();
    assert_eq!(
        resolve("p1*3", ParamUnit::Length, &scope),
        Ok((3.0, Some("p1*3".into())))
    );
    assert_eq!(resolve(" 7,5 ", ParamUnit::Length, &scope), Ok((7.5, None)));
    assert_eq!(
        resolve(" ", ParamUnit::Length, &scope),
        Err("Bitte eine Zahl oder Formel eingeben.".into())
    );
}

// ---- (b), (c) --------------------------------------------------------------------------------

/// Breite = 40 als Länge.
fn breite_scope() -> planora_feature::ParamScope {
    let mut s = AppState::new();
    param(&mut s, "Breite", "40", ParamUnit::Length);
    s.document.param_scope()
}

fn draw_field(pane: &mut Pane, text: &mut String) -> (Vec<Seen>, bool, bool) {
    let scope = breite_scope();
    let mut flags = (false, false);
    let seen = pane.settle(&mut |ui| {
        let o = formula_field(ui, "feld", text, ParamUnit::Length, &scope, 100.0);
        flags.0 |= o.fx_clicked;
        flags.1 |= o.committed;
    });
    (seen, flags.0, flags.1)
}

#[test]
fn field_shows_fx_value_and_error() {
    for dark in [true, false] {
        let mut pane = Pane::new(dark);
        let mut text = "Breite/2".to_string();
        let (seen, ..) = draw_field(&mut pane, &mut text);
        assert!(find(&seen, FX).is_some(), "„fx“ vor der Formel");
        assert!(
            find(&seen, "= 20,00 mm").is_some(),
            "Wert hinter der Formel"
        );
        assert!(
            find(&seen, "mm").is_none(),
            "keine einzelne Einheit bei Formel"
        );

        let mut text = "5".to_string();
        let (seen, ..) = draw_field(&mut pane, &mut text);
        assert!(find(&seen, FX).is_none(), "kein „fx“ bei einer Zahl");
        assert!(find(&seen, "mm").is_some(), "Einheit wie bisher");
        assert!(!seen.iter().any(|s| s.text.starts_with("= ")));

        let mut text = "Breit".to_string();
        let (seen, ..) = draw_field(&mut pane, &mut text);
        let e = find(&seen, "Den Namen „Breit“ gibt es nicht.").expect("Fehlermeldung");
        assert_eq!(e.color, theme::palette(dark).error, "Fehlerfarbe");
        assert!(
            !seen.iter().any(|s| s.text.starts_with("= ")),
            "kein Wert bei Fehler"
        );
        let field = find(&seen, "Breit").unwrap();
        assert!(
            e.rect.top() >= field.rect.bottom() - 0.5,
            "Meldung unter dem Feld"
        );
    }
}

#[test]
fn fx_click_sets_value() {
    let mut pane = Pane::new(true);
    let scope = breite_scope();
    let mut text = "Breite/2".to_string();
    let mut clicked = false;
    let mut f = |ui: &mut egui::Ui| {
        let o = formula_field(ui, "feld", &mut text, ParamUnit::Length, &scope, 100.0);
        clicked |= o.fx_clicked;
    };
    let seen = pane.settle(&mut f);
    let fx = find(&seen, FX).unwrap().rect.center();
    pane.click(fx, &mut f);
    drop(f);
    assert!(clicked, "fx_clicked");
    assert_eq!(text, "20");

    // Ein unrunder Wert wird verlustfrei eingesetzt, nicht gerundet.
    let mut text = "Breite/3".to_string();
    let mut f = |ui: &mut egui::Ui| {
        formula_field(ui, "feld", &mut text, ParamUnit::Length, &scope, 100.0);
    };
    let seen = pane.settle(&mut f);
    let fx = find(&seen, FX).unwrap().rect.center();
    pane.click(fx, &mut f);
    drop(f);
    assert_eq!(text, planora_feature::expr::number_text(40.0 / 3.0));
    assert_eq!(classify(&text), Some(FieldInput::Number(40.0 / 3.0)));
}

/// Übernahme bei Enter und beim Verlassen nach einer Änderung, nicht bei jeder Taste.
#[test]
fn field_commits_on_enter_or_leaving() {
    let mut pane = Pane::new(true);
    let scope = breite_scope();
    let mut text = "5".to_string();
    let commits = std::cell::Cell::new(0);
    let mut f = |ui: &mut egui::Ui| {
        let o = formula_field(ui, "feld", &mut text, ParamUnit::Length, &scope, 100.0);
        commits.set(commits.get() + usize::from(o.committed));
        ui.add_space(200.0);
        ui.label("daneben");
    };
    let seen = pane.settle(&mut f);
    let field = find(&seen, "5").unwrap().rect.center();
    let elsewhere = find(&seen, "daneben").unwrap().rect.center();
    pane.click(field, &mut f);
    pane.frame(vec![Event::Text("7".into())], &mut f);
    pane.frame(vec![Event::Text("8".into())], &mut f);
    pane.settle(&mut f);
    assert_eq!(commits.get(), 0, "Tippen übernimmt nicht");
    pane.frame(vec![key(Key::Enter, Modifiers::NONE)], &mut f);
    pane.settle(&mut f);
    assert_eq!(commits.get(), 1, "Enter übernimmt");
    // Verlassen ohne Änderung übernimmt nicht, nach einer Änderung schon.
    pane.click(field, &mut f);
    pane.click(elsewhere, &mut f);
    assert_eq!(commits.get(), 1, "Verlassen ohne Änderung");
    pane.click(field, &mut f);
    pane.frame(vec![Event::Text("1".into())], &mut f);
    pane.click(elsewhere, &mut f);
    assert_eq!(commits.get(), 2, "Verlassen nach Änderung übernimmt");
}

// ---- (d) -------------------------------------------------------------------------------------

#[test]
fn menu_opens_window() {
    let mut rig = Rig::new(ws());
    let seen = rig.settle();
    let bar = seen
        .iter()
        .filter(|s| s.text == "Bearbeiten")
        .min_by(|a, b| a.rect.top().total_cmp(&b.rect.top()))
        .expect("Menü „Bearbeiten“")
        .rect
        .center();
    rig.click(bar);
    let seen = rig.settle();
    let entry = find(&seen, &parameters_entry()).expect("Eintrag „Parameter …“");
    let glyph = icons::glyph(Icon::Parameters).to_string();
    assert_eq!(icons::glyph(Icon::Parameters), '\u{ebe4}');
    assert_eq!(icons::phosphor_name(Icon::Parameters), "function");
    assert!(
        seen.iter().any(|s| s.text == glyph
            && (s.rect.center().y - entry.rect.center().y).abs() < 4.0
            && s.rect.right() <= entry.rect.left()),
        "Symbol links neben dem Eintrag"
    );
    // Reihenfolge: nach Rückgängig/Wiederholen, vor den Booleschen Operationen.
    let redo = seen
        .iter()
        .find(|s| s.text.starts_with("Wiederholen"))
        .unwrap();
    let union = find(&seen, "Vereinigen …").unwrap();
    assert!(redo.rect.top() < entry.rect.top() && entry.rect.top() < union.rect.top());
    let p = entry.rect.center();
    rig.click(p);
    let seen = rig.settle();
    assert!(rig.tab().parameter_dialog.is_some(), "Fenster offen");
    let rect = rig.dialog_rect();
    assert!(
        visible(&seen, &parameter_dialog::title(), rect),
        "Titel „Parameter“"
    );
    // Ohne Zeilen der Hinweis, mit einer Zeile alle Spaltenköpfe.
    assert!(visible(&seen, &parameter_dialog::empty_table(), rect));
    let add = find(&seen, &parameter_dialog::add_row())
        .unwrap()
        .rect
        .center();
    rig.click(add);
    let seen = rig.settle();
    for head in ["Name", "Formel", "Wert", "Art", "Kommentar"] {
        assert!(visible(&seen, head, rect), "Spaltenkopf „{head}“");
    }
    assert!(
        visible(&seen, "Parameter1", rect),
        "{:?}",
        find(&seen, "Parameter1").map(|s| (s.rect, s.clip, rect))
    );
    assert!(visible(&seen, "10,00 mm", rect), "Wert ohne „= “");
    assert!(
        !rig.tab().dialog_open(),
        "zählt nicht als Fenster, das Strg+Z sperrt"
    );
}

#[test]
fn menu_entry_locked_while_sketch_open() {
    let mut w = ws();
    let s = &mut w.active_tab_mut().state;
    let sk = s
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    s.open_sketch(sk).unwrap();
    let mut rig = Rig::new(w);
    let seen = rig.settle();
    let bar = seen
        .iter()
        .filter(|s| s.text == "Bearbeiten")
        .min_by(|a, b| a.rect.top().total_cmp(&b.rect.top()))
        .unwrap()
        .rect
        .center();
    rig.click(bar);
    let seen = rig.settle();
    let p = find(&seen, &parameters_entry())
        .expect("Eintrag")
        .rect
        .center();
    rig.click(p);
    rig.settle();
    assert!(
        rig.tab().parameter_dialog.is_none(),
        "gesperrter Eintrag öffnet nichts"
    );
}

// ---- (e) -------------------------------------------------------------------------------------

#[test]
fn edit_commits_on_enter_only() {
    let mut rig = Rig::new(ws());
    parameter_dialog::open(rig.tab());
    let seen = rig.settle();
    let add = find(&seen, &parameter_dialog::add_row())
        .expect("Knopf")
        .rect
        .center();
    rig.click(add);
    rig.settle();
    let s = &rig.tab().state;
    assert_eq!(s.document.parameters().len(), 1);
    let id = s.document.parameters()[0].id;
    assert_eq!(s.document.parameters()[0].name, "Parameter1");
    assert_eq!(rig.tab().history().undo_label(), Some("Parameter anlegen"));
    let steps = rig.tab().history().undo_len();
    let count = rig.tab().state.document.edit_count();

    rig.focus_field("10");
    rig.type_text("40");
    rig.settle();
    let tab = rig.tab();
    assert_eq!(
        tab.state.document.parameter(id).unwrap().formula,
        "10",
        "ohne Enter nichts"
    );
    assert_eq!(tab.state.document.edit_count(), count);
    assert_eq!(
        tab.parameter_dialog
            .as_ref()
            .unwrap()
            .text(id, Column::Formula),
        Some("40"),
        "Entwurf"
    );
    rig.frame(vec![key(Key::Enter, Modifiers::NONE)]);
    rig.settle();
    let tab = rig.tab();
    assert_eq!(tab.state.document.parameter(id).unwrap().formula, "40");
    assert_eq!(tab.state.document.edit_count(), count + 1);
    assert_eq!(tab.history().undo_len(), steps + 1, "genau ein Schritt");
    assert_eq!(tab.history().undo_label(), Some("Parameter ändern"));

    // Verlassen des Feldes nach einer Änderung übernimmt ebenso (Klick auf den Titel).
    rig.focus_field("40");
    rig.type_text("55");
    let title = rig.dialog_rect().left_top() + vec2(30.0, 20.0);
    rig.click(title);
    rig.settle();
    let tab = rig.tab();
    assert_eq!(tab.state.document.parameter(id).unwrap().formula, "55");
    assert_eq!(tab.history().undo_len(), steps + 2);
}

/// Die Auswahl „Art“ im Fenster wirkt sofort, ohne Enter.
#[test]
fn unit_choice_applies_at_once() {
    let mut s = AppState::new();
    let id = param(&mut s, "w", "30", ParamUnit::Length);
    let mut d = ParameterDialog::new(&s);
    let mut pane = Pane::new(true);
    let mut f = |ui: &mut egui::Ui| {
        assert!(d.show(ui.ctx(), &mut s, false));
    };
    let seen = pane.settle(&mut f);
    let combo = find(&seen, "Länge").expect("Auswahl Art").rect.center();
    pane.click(combo, &mut f);
    let seen = pane.settle(&mut f);
    let angle = find(&seen, "Winkel").expect("Eintrag Winkel").rect.center();
    pane.click(angle, &mut f);
    pane.settle(&mut f);
    drop(f);
    assert_eq!(s.document.parameter(id).unwrap().unit, ParamUnit::Angle);
}

/// Art wirkt sofort; Kommentar übernimmt mit der Zeile.
#[test]
fn unit_applies_at_once_and_comment_with_row() {
    let mut s = AppState::new();
    let mut d = ParameterDialog::new(&s);
    let id = d.add_row(&mut s).unwrap();
    d.set_text(id, Column::Formula, "33");
    d.set_unit(id, ParamUnit::Angle, &mut s).unwrap();
    let p = s.document.parameter(id).unwrap();
    assert_eq!(p.unit, ParamUnit::Angle);
    assert_eq!(p.formula, "10", "Art allein übernimmt keinen Entwurf");
    d.set_text(id, Column::Comment, "Winkel der Fase");
    d.commit(id, &mut s).unwrap();
    assert_eq!(s.document.parameter(id).unwrap().comment, "Winkel der Fase");
}

// ---- (f) -------------------------------------------------------------------------------------

#[test]
fn delete_used_offers_replace() {
    let (w, breite, f) = ws_with_box();
    let mut rig = Rig::new(w);
    parameter_dialog::open(rig.tab());
    rig.settle();
    let before = rig.tab().state.snapshot();
    let steps = rig.tab().history().undo_len();
    {
        let tab = rig.tab();
        let d = tab.parameter_dialog.as_mut().unwrap();
        d.request_delete(breite, &mut tab.state).unwrap();
        assert_eq!(
            d.confirm_text(),
            Some("Breite wird in Quader 1 benutzt.".to_string())
        );
    }
    assert!(
        rig.tab().state.snapshot().same_content(&before),
        "noch nichts gelöscht"
    );
    let seen = rig.settle();
    let rect = rig.dialog_rect();
    assert!(visible(&seen, "Breite wird in Quader 1 benutzt.", rect));
    assert!(
        visible(&seen, &parameter_dialog::replace(), rect),
        "„Durch den Wert ersetzen“"
    );
    assert!(
        visible(&seen, &parameter_dialog::cancel(), rect),
        "„Abbrechen“"
    );

    // Abbrechen: nichts ändert sich.
    let cancel = find(&seen, &parameter_dialog::cancel())
        .unwrap()
        .rect
        .center();
    rig.click(cancel);
    rig.settle();
    let tab = rig.tab();
    assert_eq!(tab.parameter_dialog.as_ref().unwrap().confirm_text(), None);
    assert!(tab.state.snapshot().same_content(&before));
    assert_eq!(tab.history().undo_len(), steps);

    // Löschsymbol in der Zeile öffnet die Rückfrage wieder; ersetzen.
    let seen = rig.settle();
    let trash = icons::glyph(Icon::Delete).to_string();
    let rect = rig.dialog_rect();
    let p = seen
        .iter()
        .find(|s| s.text == trash && rect.contains_rect(s.rect))
        .expect("Löschsymbol")
        .rect
        .center();
    rig.click(p);
    let seen = rig.settle();
    let replace = find(&seen, &parameter_dialog::replace())
        .expect("Rückfrage")
        .rect
        .center();
    rig.click(replace);
    rig.settle();
    let tab = rig.tab();
    assert!(
        tab.state.document.parameter(breite).is_none(),
        "Parameter weg"
    );
    assert!(
        feature_formulas(&tab.state, f).is_empty(),
        "Quader ohne Formel"
    );
    assert_eq!(box_width(&tab.state, f), 40.0, "Wert gleich");
    assert_eq!(tab.history().undo_len(), steps + 1, "ein Schritt");
    assert_eq!(tab.history().undo_label(), Some("Parameter löschen"));
    assert!(tab.parameter_dialog.as_ref().unwrap().row_ids().is_empty());
}

/// Unbenutzt: sofort gelöscht; Ersetzen bei fehlerhaftem Benutzer: Meldung im Fenster.
#[test]
fn delete_unused_and_refused_replace() {
    let mut s = AppState::new();
    let a = param(&mut s, "a", "5", ParamUnit::Length);
    let mut d = ParameterDialog::new(&s);
    d.request_delete(a, &mut s).unwrap();
    assert!(s.document.parameters().is_empty());
    assert_eq!(d.confirm_text(), None);

    // Quader mit Breite „b - 40“ bei b = 40: Wertfehler, Ersetzen wird abgelehnt.
    let b = param(&mut s, "b", "50", ParamUnit::Length);
    let f = s
        .create_box_with(
            DVec3::new(10.0, 20.0, 10.0),
            formulas(&[(FieldKey::Breite, "b - 40")]),
        )
        .unwrap();
    s.edit_parameter(
        b,
        ParamEdit {
            name: "b".into(),
            formula: "40".into(),
            unit: ParamUnit::Length,
            comment: String::new(),
        },
    )
    .unwrap();
    let mut d = ParameterDialog::new(&s);
    d.request_delete(b, &mut s).unwrap();
    assert!(d.confirm_text().is_some());
    let before = s.snapshot();
    let e = d.replace_confirmed(&mut s).unwrap_err();
    assert_eq!(
        e,
        "Parameter „b“ lässt sich nicht durch den Wert ersetzen: „Quader 1“ hat gerade einen \
         Fehler. Beheben Sie zuerst den Fehler."
    );
    assert_eq!(d.error(), Some(e.clone()));
    assert!(s.snapshot().same_content(&before));
    assert_eq!(feature_formulas(&s, f).len(), 1);
}

// ---- (g) -------------------------------------------------------------------------------------

#[test]
fn rename_row() {
    let (w, breite, f) = ws_with_box();
    let mut rig = Rig::new(w);
    parameter_dialog::open(rig.tab());
    let steps = rig.tab().history().undo_len();
    rig.focus_field("Breite");
    rig.type_text("Weite");
    rig.frame(vec![key(Key::Enter, Modifiers::NONE)]);
    rig.settle();
    let tab = rig.tab();
    assert_eq!(tab.state.document.parameter(breite).unwrap().name, "Weite");
    assert_eq!(
        feature_formulas(&tab.state, f)
            .get(&FieldKey::Breite)
            .map(String::as_str),
        Some("Weite")
    );
    assert_eq!(tab.history().undo_len(), steps + 1, "ein Schritt");
    assert_eq!(tab.history().undo_label(), Some("Parameter ändern"));
}

// ---- (h) -------------------------------------------------------------------------------------

#[test]
fn error_stays_in_row() {
    let mut s = AppState::new();
    let a = param(&mut s, "a", "10", ParamUnit::Length);
    let b = param(&mut s, "b", "a + 2", ParamUnit::Length);
    let mut d = ParameterDialog::new(&s);
    let before = s.snapshot();
    let count = s.document.edit_count();
    d.set_text(a, Column::Formula, "b");
    let e = d.commit(a, &mut s).unwrap_err();
    assert!(e.contains("im Kreis"), "{e}");
    assert_eq!(d.error(), Some(e.clone()));
    assert!(s.snapshot().same_content(&before), "Dokument unverändert");
    assert_eq!(s.document.edit_count(), count);
    assert_eq!(d.text(a, Column::Formula), Some("b"), "Entwurf bleibt");
    assert_eq!(d.text(b, Column::Formula), Some("a + 2"));

    // Im Bild: rote Meldung unter der Tabelle, das Feld zeigt den Entwurf mit Kreis-Meldung.
    for dark in [true, false] {
        let mut pane = Pane::new(dark);
        let seen = pane.settle(&mut |ui| {
            assert!(d.show(ui.ctx(), &mut s, false));
        });
        let red: Vec<&Seen> = seen
            .iter()
            .filter(|t| t.text.contains("im Kreis"))
            .collect();
        assert!(!red.is_empty(), "Meldung sichtbar");
        assert!(red.iter().all(|t| t.color == theme::palette(dark).error));
        assert!(find(&seen, "b").is_some(), "Entwurf im Feld");
    }
    // Ein Selbstbezug rechnet nicht mit dem alten Wert.
    d.set_text(a, Column::Formula, "a + 1");
    let mut pane = Pane::new(true);
    let seen = pane.settle(&mut |ui| {
        assert!(d.show(ui.ctx(), &mut s, false));
    });
    assert!(
        find(&seen, "= 11,00 mm").is_none(),
        "kein Wert aus dem alten a"
    );
    assert!(seen.iter().any(|t| t.text.contains("von sich selbst ab")));
}

// ---- (i) -------------------------------------------------------------------------------------

fn locked_case(s: &mut AppState, read_only: bool, hint: &str) {
    let id = s.document.parameters()[0].id;
    let mut d = ParameterDialog::new(s);
    let mut pane = Pane::new(true);
    let mut f = |ui: &mut egui::Ui| {
        assert!(d.show(ui.ctx(), s, read_only));
    };
    let seen = pane.settle(&mut f);
    assert!(find(&seen, hint).is_some(), "Hinweis „{hint}“");
    let field = find(&seen, "Breite").unwrap().rect.center();
    pane.click(field, &mut f);
    pane.frame(vec![Event::Text("x".into())], &mut f);
    pane.frame(vec![key(Key::Enter, Modifiers::NONE)], &mut f);
    let seen = pane.settle(&mut f);
    let add = find(&seen, &parameter_dialog::add_row())
        .unwrap()
        .rect
        .center();
    pane.click(add, &mut f);
    let trash = icons::glyph(Icon::Delete).to_string();
    let p = find(&seen, &trash).unwrap().rect.center();
    pane.click(p, &mut f);
    pane.settle(&mut f);
    drop(f);
    assert_eq!(d.text(id, Column::Name), Some("Breite"), "Feld gesperrt");
    assert_eq!(
        s.document.parameters().len(),
        1,
        "nichts angelegt oder gelöscht"
    );
    assert_eq!(d.confirm_text(), None);
}

#[test]
fn locked_while_sketch_open() {
    let mut s = AppState::new();
    param(&mut s, "Breite", "40", ParamUnit::Length);
    let sk = s
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    s.open_sketch(sk).unwrap();
    locked_case(&mut s, false, "Beenden Sie zuerst die Skizze.");

    let mut s = AppState::new();
    param(&mut s, "Breite", "40", ParamUnit::Length);
    locked_case(
        &mut s,
        true,
        &planora_app::drawing_browser::read_only_hint(),
    );
}

// ---- (j) -------------------------------------------------------------------------------------

#[test]
fn undo_keeps_window_open() {
    let (w, breite, f) = ws_with_box();
    let mut rig = Rig::new(w);
    parameter_dialog::open(rig.tab());
    rig.settle();
    {
        let tab = rig.tab();
        let d = tab.parameter_dialog.as_mut().unwrap();
        d.set_text(breite, Column::Formula, "60");
        d.commit(breite, &mut tab.state).unwrap();
        tab.after_parameter_change();
    }
    assert_eq!(box_width(&rig.tab().state, f), 60.0);
    rig.settle();
    rig.frame(vec![key(Key::Z, Modifiers::COMMAND)]);
    rig.settle();
    let tab = rig.tab();
    assert_eq!(tab.state.document.parameter(breite).unwrap().formula, "40");
    assert_eq!(box_width(&tab.state, f), 40.0, "Dokument zurück");
    let d = tab.parameter_dialog.as_ref().expect("Fenster bleibt offen");
    assert_eq!(
        d.text(breite, Column::Formula),
        Some("40"),
        "Zeile zeigt den alten Wert"
    );
    let seen = rig.settle();
    let rect = rig.dialog_rect();
    assert!(visible(&seen, "40,00 mm", rect));

    // Beim Tippen im Fenster gehört Strg+Z dem Feld.
    let steps = rig.tab().history().undo_len();
    rig.focus_field("40");
    rig.type_text("7");
    assert_eq!(
        rig.tab()
            .parameter_dialog
            .as_ref()
            .unwrap()
            .text(breite, Column::Formula),
        Some("7"),
        "getippt"
    );
    rig.frame(vec![key(Key::Z, Modifiers::COMMAND)]);
    rig.settle();
    let tab = rig.tab();
    assert_eq!(tab.state.document.parameter(breite).unwrap().formula, "40");
    assert_eq!(
        tab.history().undo_len(),
        steps,
        "kein Rückgängig am Dokument"
    );
    assert_eq!(tab.state.document.parameters().len(), 1);
    assert!(tab.state.document.feature(f).is_some(), "Quader noch da");
}

/// Wiederholen und ein ersetzter Zustand (Zurückladen, anderer Tab) laden die Zeilen neu;
/// Kennungen eines anderen Stands bleiben nicht hängen.
#[test]
fn reload_after_redo_and_replaced_state() {
    let (mut w, breite, _f) = ws_with_box();
    let tab = w.active_tab_mut();
    parameter_dialog::open(tab);
    let d = tab.parameter_dialog.as_mut().unwrap();
    d.set_text(breite, Column::Formula, "60");
    d.commit(breite, &mut tab.state).unwrap();
    tab.after_parameter_change();
    tab.undo().unwrap();
    tab.parameter_dialog
        .as_mut()
        .unwrap()
        .set_text(breite, Column::Formula, "Entwurf");
    tab.redo().unwrap();
    let d = tab.parameter_dialog.as_ref().unwrap();
    assert_eq!(
        d.text(breite, Column::Formula),
        Some("60"),
        "Entwurf verworfen"
    );

    // Neuer Stand ohne Parameter, dann einer mit anderen Kennungen.
    let mut other = AppState::new();
    let x = param(&mut other, "x", "1", ParamUnit::Number);
    let y = param(&mut other, "y", "2", ParamUnit::Number);
    other.delete_parameter(x).unwrap();
    tab.replace_state(AppState::from_snapshot(other.snapshot()).unwrap());
    let d = tab.parameter_dialog.as_ref().unwrap();
    assert_eq!(d.row_ids(), vec![y]);
    assert_eq!(
        d.text(breite, Column::Formula),
        None,
        "alte Kennung entfällt"
    );

    // Der Zustand wechselt ohne Bescheid (anderes Dokument): `show` lädt neu.
    let mut d = ParameterDialog::new(&tab.state);
    let mut third = AppState::new();
    let z = param(&mut third, "z", "3", ParamUnit::Number);
    let mut pane = Pane::new(true);
    pane.settle(&mut |ui| {
        d.show(ui.ctx(), &mut third, false);
    });
    assert_eq!(d.row_ids(), vec![z]);
}

/// R-a: Ein offenes Bearbeiten-Fenster schließt nach einer Parameteränderung mit Hinweis.
/// R-b: Die Übernahme ist ein eigener Schritt, auch während die Maustaste (für ein Ziehen)
/// gedrückt ist; eine andere Änderung, die noch offen war, weil das Feld die Tastatur hielt
/// (etwa ein Ziehen in der Ansicht), wird vorher ihr eigener Schritt.
#[test]
fn parameter_change_closes_edit_dialog_and_ends_the_step() {
    let (w, breite, f) = ws_with_box();
    let mut rig = Rig::new(w);
    parameter_dialog::open(rig.tab());
    let steps = rig.tab().history().undo_len();
    rig.focus_field("40");
    rig.type_text("50");
    // Das Bearbeiten-Fenster erst jetzt (es läge sonst über dem Feld). Dazu eine andere
    // Änderung, die offen bleibt, solange das Feld die Tastatur hat.
    {
        let tab = rig.tab();
        tab.feature_dialog = FeatureDialog::new(&tab.state, f);
        assert!(tab.feature_dialog.is_some());
        tab.state.create_box(DVec3::new(5.0, 5.0, 5.0)).unwrap();
    }
    rig.frame(vec![]);
    assert_eq!(
        rig.tab().history().undo_len(),
        steps,
        "offen während des Tippens"
    );
    // Maustaste in der Ansicht (weit weg vom Fenster) gedrückt halten, dann Enter.
    let away = Pos2::new(700.0, SIZE.y - 120.0);
    rig.frame(vec![Event::PointerMoved(away)]);
    rig.frame(vec![button(away, true)]);
    assert_eq!(
        rig.tab()
            .parameter_dialog
            .as_ref()
            .unwrap()
            .text(breite, Column::Formula),
        Some("50"),
        "eine andere Änderung am Dokument verwirft den Entwurf nicht"
    );
    rig.frame(vec![key(Key::Enter, Modifiers::NONE)]);
    let tab = rig.tab();
    assert_eq!(box_width(&tab.state, f), 50.0, "übernommen");
    assert_eq!(
        tab.history().undo_len(),
        steps + 2,
        "Quader und Parameteränderung je ein Schritt, obwohl die Maustaste gedrückt ist"
    );
    assert_eq!(tab.history().undo_label(), Some("Parameter ändern"));
    assert!(
        tab.feature_dialog.is_none(),
        "Bearbeiten-Fenster geschlossen"
    );
    assert_eq!(
        tab.state.info.as_deref(),
        Some(parameter_dialog::dialog_closed().as_str())
    );
    rig.frame(vec![button(away, false)]);
    rig.settle();
    let tab = rig.tab();
    tab.undo().unwrap();
    assert_eq!(
        box_width(&tab.state, f),
        40.0,
        "Rückgängig nimmt nur den Parameter zurück"
    );
    assert_eq!(tab.state.bodies.len(), 2, "der zweite Quader bleibt");
}

// ---- (k) -------------------------------------------------------------------------------------

#[test]
fn fits_small_and_both_themes() {
    for (w, h) in [(800.0, 600.0), (1100.0, 700.0)] {
        for dark in [true, false] {
            let mut s = AppState::new();
            // Viele Zeilen: die Tabelle rollt, „Zeile hinzufügen“ bleibt erreichbar.
            for i in 1..=30 {
                param(
                    &mut s,
                    &format!("Laenge{i}"),
                    &format!("{i}"),
                    ParamUnit::Length,
                );
            }
            let mut d = ParameterDialog::new(&s);
            let mut pane = Pane::new(dark);
            pane.size = vec2(w, h);
            let last = std::cell::Cell::new(None);
            let mut f = |ui: &mut egui::Ui| {
                assert!(d.show(ui.ctx(), &mut s, false));
                last.set(d.last_rect);
            };
            let seen = pane.settle(&mut f);
            let win = Rect::from_min_size(Pos2::ZERO, vec2(w, h));
            let rect = last.get().expect("gezeichnet");
            assert!(win.contains_rect(rect), "{w}×{h}: {rect:?}");
            assert!((rect.width() - 720.0).abs() <= 1.0, "{rect:?}");
            assert!((rect.center().x - w / 2.0).abs() <= 1.0, "mittig {rect:?}");
            for head in ["Name", "Formel", "Wert", "Art", "Kommentar"] {
                assert!(visible(&seen, head, rect), "Spaltenkopf „{head}“");
            }
            // Jede sichtbare Zeile liegt ganz im Fenster (nichts ragt rechts hinaus).
            for t in seen.iter().filter(|t| t.text.starts_with("Laenge")) {
                if t.clip.intersects(t.rect) {
                    assert!(rect.contains_rect(t.rect), "{}: {:?}", t.text, t.rect);
                }
            }
            let trash = icons::glyph(Icon::Delete).to_string();
            let first_trash = find(&seen, &trash).expect("Löschsymbol");
            assert!(
                rect.contains_rect(first_trash.rect),
                "Löschsymbol im Fenster"
            );
            assert!(
                visible(&seen, &parameter_dialog::add_row(), rect),
                "„Zeile hinzufügen“ ohne Rollen"
            );
            assert!(
                !visible(&seen, "Laenge30", rect),
                "{w}×{h}: die Tabelle rollt (letzte Zeile erst nach dem Rollen)"
            );
            let add = find(&seen, &parameter_dialog::add_row())
                .unwrap()
                .rect
                .center();
            pane.click(add, &mut f);
            drop(f);
            assert_eq!(
                s.document.parameters().len(),
                31,
                "Klick legt eine Zeile an"
            );
        }
    }
}

// ---- Laufzeit (von Hand, im Release-Bau) ----------------------------------------------------

/// 500 Zeilen (Kette p_i = p_(i-1) + 1), 1100 × 700: Zeit je Bild ohne Tessellieren.
/// `cargo test --release -p planora-app --test parameter_ui perf_500_rows -- --ignored --nocapture`
#[test]
#[ignore]
fn perf_500_rows() {
    for count in [60, 500] {
        let mut s = AppState::new();
        for i in 1..=count {
            let f = if i == 1 {
                "10".to_string()
            } else {
                format!("p{} + 1", i - 1)
            };
            param(&mut s, &format!("p{i}"), &f, ParamUnit::Length);
        }
        let mut d = ParameterDialog::new(&s);
        let mut pane = Pane::new(true);
        pane.size = vec2(1100.0, 700.0);
        let mut f = |ui: &mut egui::Ui| {
            d.show(ui.ctx(), &mut s, false);
        };
        pane.settle(&mut f);
        let n = 40;
        let t0 = std::time::Instant::now();
        for _ in 0..n {
            pane.frame(vec![], &mut f);
        }
        let rows = t0.elapsed().as_secs_f64() * 1000.0 / n as f64;
        drop(f);
        let t0 = std::time::Instant::now();
        for _ in 0..n {
            pane.frame(vec![], &mut |ui| {
                ui.label("x");
            });
        }
        let empty = t0.elapsed().as_secs_f64() * 1000.0 / n as f64;
        eprintln!("PERF {count} Zeilen: {rows:.2} ms/Bild, leeres Bild {empty:.2} ms");
    }
}
// ---- Nachbesserung (Review F1–F7) ------------------------------------------------------------

fn text_event(c: char) -> Event {
    Event::Text(c.to_string())
}

fn draft(rig: &mut Rig, id: ParamId, column: Column) -> String {
    rig.tab()
        .parameter_dialog
        .as_ref()
        .unwrap()
        .text(id, column)
        .unwrap()
        .to_string()
}

/// F1: Eine Formel lässt sich in einer Zeile Zeichen für Zeichen tippen (echte Tasten, ein
/// Zeichen je Bild); das Feld behält die Tastatur, wenn aus der Zahl eine Formel wird und
/// umgekehrt.
#[test]
fn typing_formula_in_row_keeps_focus() {
    let (w, _breite, _f) = ws_with_box();
    let mut rig = Rig::new(w);
    parameter_dialog::open(rig.tab());
    rig.settle();
    let id = {
        let tab = rig.tab();
        let d = tab.parameter_dialog.as_mut().unwrap();
        let id = d.add_row(&mut tab.state).unwrap();
        tab.after_parameter_change();
        id
    };
    rig.focus_field("10");
    let focus = rig.ctx.memory(|m| m.focused());
    assert!(focus.is_some(), "Feld hat die Tastatur");
    let mut typed = String::new();
    for c in "Breite*2".chars() {
        rig.frame(vec![text_event(c)]);
        typed.push(c);
        assert_eq!(draft(&mut rig, id, Column::Formula), typed);
        assert_eq!(
            rig.ctx.memory(|m| m.focused()),
            focus,
            "Fokus nach „{typed}“"
        );
    }
    let steps = rig.tab().history().undo_len();
    rig.frame(vec![key(Key::Enter, Modifiers::NONE)]);
    rig.settle();
    let tab = rig.tab();
    assert_eq!(
        tab.state.document.parameter(id).unwrap().formula,
        "Breite*2"
    );
    assert_eq!(tab.history().undo_len(), steps + 1);
    // Rückweg: Formel zu Zahl.
    rig.focus_field("Breite*2");
    let focus = rig.ctx.memory(|m| m.focused());
    for (c, want) in [('7', "7"), ('5', "75")] {
        rig.frame(vec![text_event(c)]);
        assert_eq!(draft(&mut rig, id, Column::Formula), want);
        assert_eq!(
            rig.ctx.memory(|m| m.focused()),
            focus,
            "Fokus nach „{want}“"
        );
    }
    rig.frame(vec![key(Key::Enter, Modifiers::NONE)]);
    rig.settle();
    assert_eq!(
        rig.tab().state.document.parameter(id).unwrap().formula,
        "75"
    );
}

/// F1 im Baustein selbst: „10“ → „B“ … „Breite/4“ behält die Tastatur, Enter übernimmt.
#[test]
fn typing_formula_in_widget_keeps_focus() {
    let mut pane = Pane::new(true);
    let scope = breite_scope();
    let mut text = "10".to_string();
    let commits = std::cell::Cell::new(0);
    let mut f = |ui: &mut egui::Ui| {
        let o = formula_field(ui, "feld", &mut text, ParamUnit::Length, &scope, 100.0);
        commits.set(commits.get() + usize::from(o.committed));
    };
    let seen = pane.settle(&mut f);
    let field = find(&seen, "10").unwrap().rect.center();
    pane.click(field, &mut f);
    pane.frame(vec![key(Key::A, Modifiers::COMMAND)], &mut f);
    let focus = pane.ctx.memory(|m| m.focused());
    assert!(focus.is_some());
    for c in "Breite/4".chars() {
        pane.frame(vec![text_event(c)], &mut f);
        assert_eq!(pane.ctx.memory(|m| m.focused()), focus, "Fokus nach „{c}“");
    }
    pane.frame(vec![key(Key::A, Modifiers::COMMAND)], &mut f);
    for c in "12".chars() {
        pane.frame(vec![text_event(c)], &mut f);
        assert_eq!(
            pane.ctx.memory(|m| m.focused()),
            focus,
            "Fokus zurück zur Zahl"
        );
    }
    pane.frame(vec![key(Key::Enter, Modifiers::NONE)], &mut f);
    drop(f);
    assert_eq!(text, "12");
    assert_eq!(commits.get(), 1);
}

/// F3: gesperrtes Feld nimmt nichts an und übernimmt nie; „fx“ ist gesperrt.
#[test]
fn disabled_field_is_read_only() {
    let mut pane = Pane::new(false);
    let scope = breite_scope();
    let mut text = "Breite/2".to_string();
    let flags = std::cell::Cell::new((false, false));
    let mut f = |ui: &mut egui::Ui| {
        let o = FormulaField::new("gesperrt", &mut text, ParamUnit::Length)
            .enabled(false)
            .show(ui, &scope);
        let (a, b) = flags.get();
        flags.set((a || o.fx_clicked, b || o.committed));
    };
    let seen = pane.settle(&mut f);
    assert!(find(&seen, "= 20,00 mm").is_some(), "Wert wird gezeigt");
    let fx = find(&seen, FX).unwrap().rect.center();
    let field = find(&seen, "Breite/2").unwrap().rect.center();
    pane.click(fx, &mut f);
    pane.click(field, &mut f);
    pane.frame(vec![text_event('9')], &mut f);
    pane.frame(vec![key(Key::Enter, Modifiers::NONE)], &mut f);
    drop(f);
    assert_eq!(text, "Breite/2");
    assert_eq!(flags.get(), (false, false));
}

/// F7 (R4): „fx“ ist bei ungültiger Formel gesperrt; die Meldung kommt auch als Ergebnis.
#[test]
fn fx_disabled_for_invalid_formula() {
    let mut pane = Pane::new(true);
    let scope = breite_scope();
    let mut text = "Breit*2".to_string();
    let clicked = std::cell::Cell::new(false);
    let error = std::cell::RefCell::new(None);
    let mut f = |ui: &mut egui::Ui| {
        let o = formula_field(ui, "feld", &mut text, ParamUnit::Length, &scope, 100.0);
        clicked.set(clicked.get() || o.fx_clicked);
        *error.borrow_mut() = o.error;
    };
    let seen = pane.settle(&mut f);
    let fx_invalid = find(&seen, FX).expect("„fx“ steht da").clone();
    let fx = fx_invalid.rect.center();
    pane.click(fx, &mut f);
    drop(f);
    assert_eq!(text, "Breit*2");
    // Gesperrt: blass (Deckkraft gesperrter Bedienelemente), nicht wie ein freier Knopf.
    assert!(
        fx_invalid.color.a() < 255 || fx_invalid.opacity < 1.0,
        "{fx_invalid:?}"
    );
    assert!(!clicked.get());
    assert_eq!(
        error.borrow().as_deref(),
        Some("Den Namen „Breit“ gibt es nicht.")
    );
}

/// F2: Nur die sichtbaren Zeilen werden gezeichnet; die letzte erreicht man durch Rollen.
#[test]
fn table_draws_only_visible_rows() {
    let mut s = AppState::new();
    for i in 1..=200 {
        param(&mut s, &format!("q{i}"), "1", ParamUnit::Number);
    }
    let mut d = ParameterDialog::new(&s);
    let mut pane = Pane::new(true);
    let last = std::cell::Cell::new(None);
    let mut f = |ui: &mut egui::Ui| {
        assert!(d.show(ui.ctx(), &mut s, false));
        last.set(d.last_rect);
    };
    pane.ctx.enable_accesskit();
    let seen = pane.settle(&mut f);
    // Gezeichnete Namensfelder zählen (auch abgeschnittene Felder hätten einen Knoten).
    pane.time += 0.05;
    let raw = egui::RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, pane.size)),
        time: Some(pane.time),
        ..Default::default()
    };
    let mut out = pane.ctx.run_ui(raw, |ui| f(ui));
    let update = out.platform_output.accesskit_update.take().expect("Baum");
    out.drop_without_applying_deltas();
    let fields = update
        .nodes
        .iter()
        .filter(|(_, n)| n.label().is_some_and(|l| l.starts_with("Name von q")))
        .count();
    assert!((5..40).contains(&fields), "{fields} Zeilen gezeichnet");
    let rect = last.get().unwrap();
    let mut seen = seen;
    for _ in 0..80 {
        if visible(&seen, "q200", rect) {
            break;
        }
        pane.frame(vec![Event::PointerMoved(rect.center())], &mut f);
        pane.frame(
            vec![Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: vec2(0.0, -400.0),
                phase: egui::TouchPhase::Move,
                modifiers: Modifiers::NONE,
            }],
            &mut f,
        );
        seen = pane.settle(&mut f);
    }
    assert!(visible(&seen, "q200", rect), "letzte Zeile nach dem Rollen");
}

/// F4: Sehr große Werte verschieben keine Spalten; die Zellen schneiden ab.
#[test]
fn huge_values_keep_columns() {
    let mut s = AppState::new();
    param(&mut s, "a", "1000", ParamUnit::Length);
    let b = param(&mut s, "b", "a * 1000000000", ParamUnit::Length);
    let mut d = ParameterDialog::new(&s);
    d.set_text(b, Column::Formula, "a * 1000000000 + 1");
    let mut pane = Pane::new(true);
    let seen = pane.settle(&mut |ui| {
        assert!(d.show(ui.ctx(), &mut s, false));
    });
    let arts: Vec<&Seen> = seen.iter().filter(|t| t.text == "Länge").collect();
    assert_eq!(arts.len(), 2, "zwei Zeilen");
    let head = find(&seen, "Art").unwrap();
    for t in &arts {
        assert!(
            (t.rect.left() - arts[0].rect.left()).abs() < 0.5,
            "Art steht untereinander: {:?}",
            arts.iter().map(|t| t.rect).collect::<Vec<_>>()
        );
        assert!(t.rect.left() >= head.rect.left() - 0.5);
    }
    let value_head = find(&seen, "Wert").unwrap();
    for t in seen
        .iter()
        .filter(|t| t.text.contains("mm") && t.rect.top() > head.rect.bottom())
    {
        let shown = t.clip.intersect(t.rect);
        assert!(
            shown.right() <= head.rect.left() + 0.5,
            "„{}“ ragt in die Spalte Art: {shown:?}",
            t.text
        );
        let _ = value_head;
    }
}

/// F5: Öffnet sich eine Skizze, verfallen Entwürfe; danach zeigt die Zeile das Dokument.
#[test]
fn sketch_lock_discards_drafts() {
    let mut s = AppState::new();
    let a = param(&mut s, "a", "40", ParamUnit::Length);
    let mut d = ParameterDialog::new(&s);
    d.set_text(a, Column::Formula, "55");
    let mut pane = Pane::new(true);
    pane.settle(&mut |ui| {
        d.show(ui.ctx(), &mut s, false);
    });
    assert_eq!(d.text(a, Column::Formula), Some("55"));
    let sk = s
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    s.open_sketch(sk).unwrap();
    pane.settle(&mut |ui| {
        d.show(ui.ctx(), &mut s, false);
    });
    s.try_close_sketch().unwrap();
    let seen = pane.settle(&mut |ui| {
        d.show(ui.ctx(), &mut s, false);
    });
    assert_eq!(d.text(a, Column::Formula), Some("40"), "Dokumentwert");
    assert!(find(&seen, "55").is_none());
    assert_eq!(s.document.parameter(a).unwrap().formula, "40");
}

/// F6 (und R26): Namen für Bildschirmleser.
#[test]
fn fields_have_accessible_names() {
    use egui::accesskit::Role;
    let mut s = AppState::new();
    param(&mut s, "Breite", "40", ParamUnit::Length);
    let mut d = ParameterDialog::new(&s);
    let mut pane = Pane::new(true);
    pane.ctx.enable_accesskit();
    pane.settle(&mut |ui| {
        d.show(ui.ctx(), &mut s, false);
    });
    pane.time += 0.05;
    let raw = egui::RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, pane.size)),
        time: Some(pane.time),
        ..Default::default()
    };
    let mut out = pane.ctx.run_ui(raw, |ui| {
        d.show(ui.ctx(), &mut s, false);
    });
    let update = out.platform_output.accesskit_update.take().expect("Baum");
    out.drop_without_applying_deltas();
    let role_of = |label: &str| {
        update
            .nodes
            .iter()
            .map(|(_, n)| n)
            .find(|n| n.label() == Some(label))
            .map(|n| n.role())
    };
    assert_eq!(role_of("Name von Breite"), Some(Role::TextInput));
    assert_eq!(role_of("Formel von Breite"), Some(Role::TextInput));
    assert_eq!(role_of("Kommentar von Breite"), Some(Role::TextInput));
    assert_eq!(role_of("Art von Breite"), Some(Role::ComboBox));
    assert_eq!(role_of(&parameter_dialog::delete_row()), Some(Role::Button));
}

/// F7 (R11/R12): Name und Formel ohne Leerraum am Rand übernommen.
#[test]
fn commit_trims_name_and_formula() {
    let mut s = AppState::new();
    let a = param(&mut s, "a", "1", ParamUnit::Length);
    let mut d = ParameterDialog::new(&s);
    d.set_text(a, Column::Name, "  a2 ");
    d.set_text(a, Column::Formula, " 7 ");
    d.commit(a, &mut s).unwrap();
    let p = s.document.parameter(a).unwrap();
    assert_eq!((p.name.as_str(), p.formula.as_str()), ("a2", "7"));
}

/// F7 (R15): Löschwunsch bei offener Skizze: Sperrmeldung, keine Rückfrage, nichts gelöscht.
#[test]
fn request_delete_while_sketch_open() {
    let mut s = AppState::new();
    let a = param(&mut s, "a", "10", ParamUnit::Length);
    s.create_box_with(
        DVec3::new(10.0, 10.0, 10.0),
        formulas(&[(FieldKey::Breite, "a")]),
    )
    .unwrap();
    let b = param(&mut s, "b", "3", ParamUnit::Length);
    let sk = s
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    s.open_sketch(sk).unwrap();
    let mut d = ParameterDialog::new(&s);
    for id in [a, b] {
        assert_eq!(
            d.request_delete(id, &mut s),
            Err(sketch_open_lock().to_string())
        );
        assert_eq!(d.confirm_text(), None);
        assert!(s.document.parameter(id).is_some());
    }
}

/// F7 (R19): Anderer Zustand mit derselben Kennung: der alte Entwurf wandert nicht mit.
#[test]
fn new_instance_with_same_ids_reloads() {
    let mut first = AppState::new();
    let a = param(&mut first, "a", "1", ParamUnit::Length);
    let mut d = ParameterDialog::new(&first);
    d.set_text(a, Column::Formula, "99");
    let mut second = AppState::new();
    let b = param(&mut second, "b", "3", ParamUnit::Length);
    assert_eq!(a, b, "gleiche Kennung");
    // Gleicher Zählerstand, nur der Zustand ist ein anderer.
    assert_eq!(first.document.edit_count(), second.document.edit_count());
    let mut pane = Pane::new(true);
    pane.settle(&mut |ui| {
        d.show(ui.ctx(), &mut second, false);
    });
    assert_eq!(d.text(b, Column::Formula), Some("3"));
    assert_eq!(d.text(b, Column::Name), Some("b"));
}

/// F7 (R20): Was während der Sperre aus einem Feld käme (Fokusverlust nach einer Änderung),
/// wird verworfen, ohne Meldung.
#[test]
fn actions_dropped_while_locked() {
    let mut s = AppState::new();
    let a = param(&mut s, "a", "10", ParamUnit::Length);
    let mut d = ParameterDialog::new(&s);
    let mut pane = Pane::new(true);
    let seen = pane.settle(&mut |ui| {
        d.show(ui.ctx(), &mut s, false);
    });
    let name = find(&seen, "a").unwrap().rect.center();
    pane.click(name, &mut |ui| {
        d.show(ui.ctx(), &mut s, false);
    });
    pane.frame(vec![text_event('x')], &mut |ui| {
        d.show(ui.ctx(), &mut s, false);
    });
    let sk = s
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    s.open_sketch(sk).unwrap();
    s.message = None;
    pane.settle(&mut |ui| {
        d.show(ui.ctx(), &mut s, false);
    });
    assert_eq!(d.error(), None, "keine Meldung im Fenster");
    assert_eq!(s.message, None, "keine Meldung in der Statuszeile");
    assert_eq!(s.document.parameter(a).unwrap().name, "a");
}

/// F7 (R24): Fehler einer anderen Zeile nennen deren Namen.
#[test]
fn errors_of_other_rows_are_prefixed() {
    let mut s = AppState::new();
    let a = param(&mut s, "a", "10", ParamUnit::Length);
    param(&mut s, "b", "a*10/(a-5)", ParamUnit::Length);
    let mut d = ParameterDialog::new(&s);
    d.set_text(a, Column::Formula, "2+3");
    let mut pane = Pane::new(true);
    let seen = pane.settle(&mut |ui| {
        d.show(ui.ctx(), &mut s, false);
    });
    let e = seen
        .iter()
        .find(|t| t.text.starts_with("Parameter „b“: "))
        .expect("Meldung mit dem Namen der anderen Zeile");
    assert_eq!(e.color, theme::palette(true).error);
}

/// Englisch: Nennt die Meldung einer anderen Zeile deren Namen schon (hier der Kreis, gemeldet
/// an der ersten Zeile des Kreises), kommt kein Präfix davor – die Prüfung muss die englischen
/// Anführungszeichen kennen (`param-quoted-name`), sonst stünde „Parameter "b": “ doppelt.
#[test]
fn errors_of_other_rows_naming_the_row_get_no_prefix_in_english() {
    planora_i18n::with_lang(&planora_i18n::english(), || {
        let mut s = AppState::new();
        let b = param(&mut s, "b", "10", ParamUnit::Length);
        let a = param(&mut s, "a", "10", ParamUnit::Length);
        s.edit_parameter(
            b,
            ParamEdit {
                name: "b".into(),
                formula: "a*2".into(),
                unit: ParamUnit::Length,
                comment: String::new(),
            },
        )
        .unwrap();
        let mut d = ParameterDialog::new(&s);
        d.set_text(a, Column::Formula, "b");
        let mut pane = Pane::new(true);
        let seen = pane.settle(&mut |ui| {
            d.show(ui.ctx(), &mut s, false);
        });
        let e = seen
            .iter()
            .find(|t| t.text.contains("depend on each other"))
            .expect("cycle message in English");
        assert!(e.text.starts_with("The parameters "), "{}", e.text);
        assert!(!e.text.contains("Parameter \"b\": "), "{}", e.text);
    });
}

/// F7 (R40): Eine abgelehnte Änderung schließt das Bearbeiten-Fenster nicht.
#[test]
fn rejected_change_keeps_edit_dialog() {
    let (w, breite, f) = ws_with_box();
    let mut rig = Rig::new(w);
    parameter_dialog::open(rig.tab());
    rig.focus_field("40");
    rig.type_text("Breit");
    {
        let tab = rig.tab();
        tab.feature_dialog = FeatureDialog::new(&tab.state, f);
    }
    let steps = rig.tab().history().undo_len();
    rig.frame(vec![key(Key::Enter, Modifiers::NONE)]);
    rig.settle();
    let tab = rig.tab();
    assert_eq!(tab.state.document.parameter(breite).unwrap().formula, "40");
    assert!(tab.feature_dialog.is_some(), "Bearbeiten-Fenster bleibt");
    assert_eq!(tab.state.info, None);
    assert_eq!(tab.history().undo_len(), steps);
    assert!(
        tab.parameter_dialog.as_ref().unwrap().error().is_some(),
        "Meldung im Fenster"
    );
}

/// F7 (R41): Erneutes Öffnen über das Menü behält offene Entwürfe.
#[test]
fn reopening_keeps_drafts() {
    let (mut w, breite, _f) = ws_with_box();
    let tab = w.active_tab_mut();
    parameter_dialog::open(tab);
    tab.parameter_dialog
        .as_mut()
        .unwrap()
        .set_text(breite, Column::Formula, "77");
    parameter_dialog::open(tab);
    assert_eq!(
        tab.parameter_dialog
            .as_ref()
            .unwrap()
            .text(breite, Column::Formula),
        Some("77")
    );
}

/// F7 (R42): Die Rückfrage verschwindet, wenn ihr Parameter anderweitig verschwindet.
#[test]
fn delete_prompt_vanishes_with_its_parameter() {
    let mut s = AppState::new();
    let a = param(&mut s, "a", "10", ParamUnit::Length);
    s.create_box_with(
        DVec3::new(10.0, 10.0, 10.0),
        formulas(&[(FieldKey::Breite, "a")]),
    )
    .unwrap();
    let mut d = ParameterDialog::new(&s);
    d.request_delete(a, &mut s).unwrap();
    assert!(d.confirm_text().is_some());
    s.replace_parameter(a).unwrap();
    let mut pane = Pane::new(true);
    let seen = pane.settle(&mut |ui| {
        d.show(ui.ctx(), &mut s, false);
    });
    assert_eq!(d.confirm_text(), None);
    assert!(find(&seen, &parameter_dialog::replace()).is_none());
}

/// F3: Wird ein Feld mitten im Tippen gesperrt, übernimmt es nicht (der Fokusverlust ist dann
/// keine Übernahme).
#[test]
fn field_locked_while_typing_does_not_commit() {
    let mut pane = Pane::new(true);
    let scope = breite_scope();
    let mut text = "5".to_string();
    let enabled = std::cell::Cell::new(true);
    let commits = std::cell::Cell::new(0);
    let mut f = |ui: &mut egui::Ui| {
        let o = FormulaField::new("feld", &mut text, ParamUnit::Length)
            .enabled(enabled.get())
            .show(ui, &scope);
        commits.set(commits.get() + usize::from(o.committed));
    };
    let seen = pane.settle(&mut f);
    let field = find(&seen, "5").unwrap().rect.center();
    pane.click(field, &mut f);
    pane.frame(vec![text_event('7')], &mut f);
    enabled.set(false);
    pane.settle(&mut f);
    pane.frame(vec![key(Key::Enter, Modifiers::NONE)], &mut f);
    drop(f);
    assert_eq!(commits.get(), 0);
}

// ---- Nachbesserung 2 (Re-Review G1–G5) -------------------------------------------------------

/// Zustand mit `count` Zeilen q1 … q{count} (Formel = Nummer, Zahl).
fn many(count: usize) -> (AppState, Vec<ParamId>) {
    let mut s = AppState::new();
    let ids = (1..=count)
        .map(|i| param(&mut s, &format!("q{i}"), &format!("{i}"), ParamUnit::Number))
        .collect();
    (s, ids)
}

/// Bildschirmleser-Baum eines Bildes: (Name, Rolle).
fn access_nodes(
    pane: &mut Pane,
    f: &mut dyn FnMut(&mut egui::Ui),
) -> Vec<(String, egui::accesskit::Role)> {
    pane.ctx.enable_accesskit();
    pane.frame(vec![], f);
    pane.time += 0.05;
    let raw = egui::RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, pane.size)),
        time: Some(pane.time),
        ..Default::default()
    };
    let mut out = pane.ctx.run_ui(raw, |ui| f(ui));
    let update = out.platform_output.accesskit_update.take().expect("Baum");
    out.drop_without_applying_deltas();
    update
        .nodes
        .iter()
        .filter_map(|(_, n)| n.label().map(|l| (l.to_string(), n.role())))
        .collect()
}

/// Tab-Lauf durch die Tabelle: Liefert die Zeilen in der Reihenfolge, in der ihre Textfelder
/// die Tastatur bekamen; prüft dabei, dass jede neu erreichte Zeile im Rollbereich zu sehen ist.
fn tab_through(count: usize, back: bool) -> (Vec<ParamId>, Vec<ParamId>) {
    let (mut s, ids) = many(count);
    let mut d = ParameterDialog::new(&s);
    let mut pane = Pane::new(true);
    pane.size = vec2(1100.0, 700.0);
    let last = std::cell::Cell::new(None);
    let focused = std::cell::Cell::new(None);
    let mut f = |ui: &mut egui::Ui| {
        assert!(d.show(ui.ctx(), &mut s, false));
        last.set(d.last_rect);
        focused.set(d.focused_row());
    };
    let seen = pane.settle(&mut f);
    let name = find(&seen, "q1").unwrap().rect.center();
    pane.click(name, &mut f);
    let mut order = vec![ids[0]];
    let modifiers = if back {
        Modifiers::SHIFT
    } else {
        Modifiers::NONE
    };
    // Rückwärts: zuerst vorwärts bis ans Ende.
    if back {
        for _ in 0..count * 7 {
            pane.frame(vec![key(Key::Tab, Modifiers::NONE)], &mut f);
            if focused.get() == ids.last().copied() {
                break;
            }
        }
        assert_eq!(focused.get(), ids.last().copied(), "Ende erreicht");
        order = vec![*ids.last().unwrap()];
    }
    for _ in 0..count * 7 {
        pane.frame(vec![key(Key::Tab, modifiers)], &mut f);
        pane.frame(vec![], &mut f);
        pane.frame(vec![], &mut f);
        pane.frame(vec![], &mut f);
        pane.frame(vec![], &mut f);
        let seen2 = pane.frame(vec![], &mut f);
        let _ = seen;
        if let Some(r) = focused.get()
            && order.last() != Some(&r)
        {
            order.push(r);
            let i = ids.iter().position(|x| *x == r).unwrap();
            let rect = last.get().unwrap();
            let shown = seen2.iter().any(|t| {
                t.text == format!("q{}", i + 1)
                    && t.clip.contains_rect(t.rect)
                    && rect.contains_rect(t.rect)
            });
            assert!(shown, "Zeile q{} ist nach Tab zu sehen", i + 1);
        }
        let done = if back { ids[0] } else { *ids.last().unwrap() };
        if order.last() == Some(&done) && order.len() > 1 {
            break;
        }
    }
    (order, ids)
}

/// G1: Bis 60 Zeilen sind alle gezeichnet: Tab erreicht jede, Bildschirmleser sehen alle.
#[test]
fn tab_reaches_all_of_40_rows() {
    let (order, ids) = tab_through(40, false);
    assert_eq!(order, ids, "jede Zeile in Reihenfolge");
    let (mut s, _) = many(40);
    let mut d = ParameterDialog::new(&s);
    let mut pane = Pane::new(true);
    pane.size = vec2(1100.0, 700.0);
    let nodes = access_nodes(&mut pane, &mut |ui| {
        d.show(ui.ctx(), &mut s, false);
    });
    let names = nodes
        .iter()
        .filter(|(l, _)| l.starts_with("Name von q"))
        .count();
    assert_eq!(names, 40, "alle Zeilen im Baum");
}

/// G1: Über 60 Zeilen (nur sichtbare gezeichnet) führt Tab trotzdem durch alle Zeilen, der
/// Rollbereich folgt; Umschalt+Tab ebenso zurück.
#[test]
fn tab_reaches_all_of_100_rows_and_scroll_follows() {
    let (order, ids) = tab_through(100, false);
    assert_eq!(order, ids, "vorwärts jede Zeile");
    let (order, ids) = tab_through(100, true);
    let mut back = ids.clone();
    back.reverse();
    assert_eq!(order, back, "rückwärts jede Zeile");
}

/// G4 (Zeilenhöhe): Ausschnitt → gezeichnete Zeilen, und im Bild liegen die Zeilen lückenlos
/// im Zeilenabstand.
#[test]
fn scroll_positions_map_to_rows() {
    let stride = theme::FIELD_H + theme::SPACE_2;
    assert_eq!(visible_rows(0.0, 340.0, 100), 0..11);
    assert_eq!(visible_rows(340.0, 680.0, 100), 9..21);
    assert_eq!(visible_rows(3300.0, 3400.0, 100), 96..100);
    let (mut s, _) = many(100);
    let mut d = ParameterDialog::new(&s);
    let mut pane = Pane::new(true);
    pane.size = vec2(1100.0, 700.0);
    let mut f = |ui: &mut egui::Ui| {
        assert!(d.show(ui.ctx(), &mut s, false));
    };
    let mut seen = pane.settle(&mut f);
    for step in 0..6 {
        let mut rows: Vec<(usize, Rect, Rect)> = seen
            .iter()
            .filter_map(|t| {
                let n: usize = t.text.strip_prefix('q')?.parse().ok()?;
                t.clip.intersects(t.rect).then_some((n, t.rect, t.clip))
            })
            .collect();
        rows.sort_by_key(|r| r.0);
        assert!(rows.len() >= 8, "Schritt {step}: {} Zeilen", rows.len());
        for w in rows.windows(2) {
            assert_eq!(w[1].0, w[0].0 + 1, "lückenlos");
            assert!(
                (w[1].1.top() - w[0].1.top() - stride).abs() < 0.5,
                "Zeilenabstand {stride}: {:?} {:?}",
                w[0].1,
                w[1].1
            );
        }
        let clip = rows[0].2;
        assert!(rows[0].1.top() <= clip.top() + stride, "oben keine Lücke");
        assert!(
            rows.last().unwrap().1.bottom() >= clip.bottom() - stride
                || rows.last().unwrap().0 == 100,
            "unten keine Lücke"
        );
        pane.frame(vec![Event::PointerMoved(clip.center())], &mut f);
        pane.frame(
            vec![Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: vec2(0.0, -517.0),
                phase: egui::TouchPhase::Move,
                modifiers: Modifiers::NONE,
            }],
            &mut f,
        );
        seen = pane.settle(&mut f);
    }
}

/// G2: Ein Entwurf, dessen Feld aus dem Bild rollt, wird übernommen.
#[test]
fn draft_scrolled_out_is_committed() {
    let (mut s, ids) = many(100);
    let mut d = ParameterDialog::new(&s);
    let mut pane = Pane::new(true);
    pane.size = vec2(1100.0, 700.0);
    let last = std::cell::Cell::new(None);
    let mut f = |ui: &mut egui::Ui| {
        assert!(d.show(ui.ctx(), &mut s, false));
        last.set(d.last_rect);
    };
    let seen = pane.settle(&mut f);
    let field = find(&seen, "1").expect("Formel von q1").rect.center();
    pane.click(field, &mut f);
    pane.frame(vec![key(Key::A, Modifiers::COMMAND)], &mut f);
    pane.frame(vec![text_event('7')], &mut f);
    pane.frame(vec![text_event('7')], &mut f);
    let center = last.get().unwrap().center();
    pane.frame(vec![Event::PointerMoved(center)], &mut f);
    for _ in 0..4 {
        pane.frame(
            vec![Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: vec2(0.0, -600.0),
                phase: egui::TouchPhase::Move,
                modifiers: Modifiers::NONE,
            }],
            &mut f,
        );
    }
    pane.settle(&mut f);
    drop(f);
    assert_eq!(s.document.parameter(ids[0]).unwrap().formula, "77");
}

/// G2: Ein Entwurf von früher (etwa nach dem Zurückrollen ohne Übernahme) wird beim
/// Hineinklicken und Verlassen übernommen, weil er vom Dokument abweicht.
#[test]
fn click_in_and_out_commits_stale_draft() {
    let mut s = AppState::new();
    let a = param(&mut s, "a", "40", ParamUnit::Length);
    let mut d = ParameterDialog::new(&s);
    d.set_text(a, Column::Formula, "42");
    d.set_text(a, Column::Comment, "Rand");
    let mut pane = Pane::new(true);
    let last = std::cell::Cell::new(None);
    let mut f = |ui: &mut egui::Ui| {
        assert!(d.show(ui.ctx(), &mut s, false));
        last.set(d.last_rect);
    };
    let seen = pane.settle(&mut f);
    let field = find(&seen, "42").unwrap().rect.center();
    let title = last.get().unwrap().left_top() + vec2(30.0, 20.0);
    pane.click(field, &mut f);
    pane.click(title, &mut f);
    let p = s.document.parameter(a).unwrap().clone();
    assert_eq!(p.formula, "42");
    assert_eq!(p.comment, "Rand", "die ganze Zeile");
    // Ohne Abweichung übernimmt Hinein und Hinaus nichts.
    let count = s.document.edit_count();
    let mut f = |ui: &mut egui::Ui| {
        assert!(d.show(ui.ctx(), &mut s, false));
    };
    let seen = pane.settle(&mut f);
    let field = find(&seen, "42").unwrap().rect.center();
    pane.click(field, &mut f);
    pane.click(title, &mut f);
    drop(f);
    assert_eq!(s.document.edit_count(), count);
    // Ebenso ein alter Entwurf im Namen und im Kommentar.
    for (column, text) in [(Column::Name, "a2"), (Column::Comment, "Mitte")] {
        d.set_text(a, column, text);
        let mut f = |ui: &mut egui::Ui| {
            assert!(d.show(ui.ctx(), &mut s, false));
        };
        let seen = pane.settle(&mut f);
        let field = find(&seen, text).unwrap().rect.center();
        pane.click(field, &mut f);
        pane.click(title, &mut f);
        drop(f);
        let p = s.document.parameter(a).unwrap();
        let got = if column == Column::Name {
            &p.name
        } else {
            &p.comment
        };
        assert_eq!(got, text);
    }
}

/// G3: „Zeile hinzufügen“ rollt die neue Zeile ins Bild und gibt ihrem Namen die Tastatur;
/// bei voller Tabelle eine Meldung.
#[test]
fn add_row_scrolls_into_view_and_focuses_name() {
    let (mut s, _) = many(100);
    let mut d = ParameterDialog::new(&s);
    let mut pane = Pane::new(true);
    pane.size = vec2(1100.0, 700.0);
    let last = std::cell::Cell::new(None);
    let focused = std::cell::Cell::new(None);
    let mut f = |ui: &mut egui::Ui| {
        assert!(d.show(ui.ctx(), &mut s, false));
        last.set(d.last_rect);
        focused.set(d.focused_row());
    };
    let seen = pane.settle(&mut f);
    let add = find(&seen, &parameter_dialog::add_row())
        .unwrap()
        .rect
        .center();
    pane.click(add, &mut f);
    let seen = pane.settle(&mut f);
    drop(f);
    let new = s.document.parameters().last().unwrap().id;
    assert_eq!(s.document.parameters().len(), 101);
    assert!(
        visible(&seen, "Parameter1", last.get().unwrap()),
        "neue Zeile im Bild"
    );
    assert_eq!(focused.get(), Some(new), "Name hat die Tastatur");

    let (mut s, _) = many(500);
    let mut d = ParameterDialog::new(&s);
    let mut pane = Pane::new(true);
    let mut f = |ui: &mut egui::Ui| {
        assert!(d.show(ui.ctx(), &mut s, false));
    };
    let seen = pane.settle(&mut f);
    let add = find(&seen, &parameter_dialog::add_row())
        .unwrap()
        .rect
        .center();
    pane.click(add, &mut f);
    let seen = pane.settle(&mut f);
    drop(f);
    assert_eq!(s.document.parameters().len(), 500);
    let e = seen
        .iter()
        .find(|t| t.text.contains("500"))
        .expect("Meldung „voll“");
    assert_eq!(e.color, theme::palette(true).error);
}

/// G4: „fx“ in einer Zeile des Fensters setzt den Wert ein und übernimmt sofort.
#[test]
fn fx_in_row_commits_value() {
    let mut s = AppState::new();
    param(&mut s, "a", "40", ParamUnit::Length);
    let b = param(&mut s, "b", "a/3", ParamUnit::Length);
    let mut d = ParameterDialog::new(&s);
    let mut pane = Pane::new(true);
    let count = s.document.edit_count();
    let mut f = |ui: &mut egui::Ui| {
        assert!(d.show(ui.ctx(), &mut s, false));
    };
    let seen = pane.settle(&mut f);
    let fx = find(&seen, FX).expect("„fx“ in der Zeile b").rect.center();
    pane.click(fx, &mut f);
    drop(f);
    assert_eq!(
        s.document.parameter(b).unwrap().formula,
        planora_feature::expr::number_text(40.0 / 3.0)
    );
    assert_eq!(s.document.edit_count(), count + 1);
}

/// G4: Große Werte im Formelfeld werden gekürzt, der Hinweis zeigt den ganzen Wert.
#[test]
fn huge_value_truncated_with_full_tooltip() {
    let mut s = AppState::new();
    param(&mut s, "a", "1000000000000", ParamUnit::Length);
    let scope = s.document.param_scope();
    let mut text = "a*1".to_string();
    let mut pane = Pane::new(true);
    pane.ctx
        .all_styles_mut(|st| st.interaction.tooltip_delay = 0.0);
    let full = "= 1000000000000,00 mm";
    let mut f = |ui: &mut egui::Ui| {
        ui.allocate_ui(vec2(210.0, 60.0), |ui| {
            formula_field(ui, "feld", &mut text, ParamUnit::Length, &scope, 60.0);
        });
    };
    let seen = pane.settle(&mut f);
    let value = seen
        .iter()
        .find(|t| t.text.starts_with("= 1"))
        .expect("Wertanzeige");
    assert!(value.elided, "gekürzt");
    assert!(value.rect.right() <= 210.5, "{:?}", value.rect);
    let at = value.rect.center();
    pane.frame(vec![Event::PointerMoved(at)], &mut f);
    let seen = pane.settle(&mut f);
    assert!(
        seen.iter().any(|t| t.text == full && !t.elided),
        "Hinweis mit dem ganzen Wert"
    );
}

/// G4: Der Änderungsmerker gilt ab dem Fokus: Eine Änderung, deren Feld die Tastatur ohne
/// „verlassen“ verlor (nicht gezeichnet), wird beim nächsten Hinein und Hinaus nicht
/// übernommen (ohne gespeicherten Stand zählt nur, was seit dem Fokus getippt wurde).
#[test]
fn dirty_flag_restarts_on_focus() {
    let mut pane = Pane::new(true);
    let scope = breite_scope();
    let mut text = "5".to_string();
    let drawn = std::cell::Cell::new(true);
    let commits = std::cell::Cell::new(0);
    let mut f = |ui: &mut egui::Ui| {
        if drawn.get() {
            let o = formula_field(ui, "feld", &mut text, ParamUnit::Length, &scope, 100.0);
            commits.set(commits.get() + usize::from(o.committed));
        }
        ui.add_space(200.0);
        ui.label("daneben");
    };
    let seen = pane.settle(&mut f);
    let field = find(&seen, "5").unwrap().rect.center();
    let elsewhere = find(&seen, "daneben").unwrap().rect.center();
    pane.click(field, &mut f);
    pane.frame(vec![text_event('7')], &mut f);
    drawn.set(false);
    pane.settle(&mut f);
    drawn.set(true);
    pane.settle(&mut f);
    pane.click(field, &mut f);
    pane.click(elsewhere, &mut f);
    drop(f);
    assert_eq!(commits.get(), 0);
}

/// G4: Der Rollbereich gibt dem Inhalt den Stil des Aufrufers zurück (nicht die Farben der
/// Rollgriffe).
#[test]
fn scroll_area_viewport_keeps_content_style() {
    let mut pane = Pane::new(true);
    let inner = std::cell::Cell::new(None);
    let outer = std::cell::Cell::new(None);
    pane.settle(&mut |ui| {
        outer.set(Some(ui.visuals().widgets.inactive.fg_stroke.color));
        theme::scroll_area_viewport(ui, egui::ScrollArea::vertical(), |ui, _| {
            inner.set(Some(ui.visuals().widgets.inactive.fg_stroke.color));
        });
        assert_eq!(
            Some(ui.visuals().widgets.inactive.fg_stroke.color),
            outer.get(),
            "danach wie vorher"
        );
    });
    assert_eq!(inner.get(), outer.get());
}

/// G5: „fx“ heißt für Bildschirmleser „In Wert umwandeln“; die Fußzeile zählt weitere Fehler.
#[test]
fn fx_name_and_error_count() {
    let mut s = AppState::new();
    let a = param(&mut s, "a", "1", ParamUnit::Length);
    let b = param(&mut s, "b", "2", ParamUnit::Length);
    let c = param(&mut s, "c", "a*3", ParamUnit::Length);
    let mut d = ParameterDialog::new(&s);
    let mut pane = Pane::new(true);
    let nodes = access_nodes(&mut pane, &mut |ui| {
        d.show(ui.ctx(), &mut s, false);
    });
    assert!(
        nodes
            .iter()
            .any(|(l, r)| *l == fx_label() && *r == egui::accesskit::Role::Button),
        "{nodes:?}"
    );
    d.set_text(a, Column::Formula, "x1");
    d.set_text(b, Column::Formula, "x2");
    let seen = pane.settle(&mut |ui| {
        d.show(ui.ctx(), &mut s, false);
    });
    assert!(
        find(&seen, "Den Namen „x1“ gibt es nicht. (+ 1 weiterer Fehler)").is_some(),
        "{:?}",
        seen.iter().map(|t| &t.text).collect::<Vec<_>>()
    );
    d.set_text(c, Column::Formula, "x3");
    let seen = pane.settle(&mut |ui| {
        d.show(ui.ctx(), &mut s, false);
    });
    assert!(find(&seen, "Den Namen „x1“ gibt es nicht. (+ 2 weitere Fehler)").is_some());
}

// ---- Schlussreview F1, F2, F4 ----------------------------------------------------------------

/// F1: In einer Zeile führt Tab vom Namen direkt in die Formel (nicht auf „fx“); Leertaste
/// und Enter danach ändern die Formel nie, Umschalt+Tab führt zurück zum Namen.
#[test]
fn tab_in_row_skips_fx() {
    for key_after in [Key::Space, Key::Enter] {
        let mut s = AppState::new();
        param(&mut s, "a", "40", ParamUnit::Length);
        let b = param(&mut s, "b", "a/2", ParamUnit::Length);
        let mut d = ParameterDialog::new(&s);
        let mut pane = Pane::new(true);
        let count = s.document.edit_count();
        let mut f = |ui: &mut egui::Ui| {
            assert!(d.show(ui.ctx(), &mut s, false));
        };
        let seen = pane.settle(&mut f);
        let name = find(&seen, "b").unwrap().rect.center();
        pane.click(name, &mut f);
        pane.frame(vec![key(Key::Tab, Modifiers::NONE)], &mut f);
        pane.frame(vec![key(key_after, Modifiers::NONE)], &mut f);
        pane.settle(&mut f);
        drop(f);
        assert_eq!(
            s.document.parameter(b).unwrap().formula,
            "a/2",
            "{key_after:?}"
        );
        assert_eq!(s.document.edit_count(), count);
    }
    // Tab landet im Formelfeld (Getipptes kommt an), Umschalt+Tab wieder im Namen.
    let mut s = AppState::new();
    param(&mut s, "a", "40", ParamUnit::Length);
    let b = param(&mut s, "b", "a/2", ParamUnit::Length);
    let mut d = ParameterDialog::new(&s);
    let mut pane = Pane::new(true);
    let mut f = |ui: &mut egui::Ui| {
        assert!(d.show(ui.ctx(), &mut s, false));
    };
    let seen = pane.settle(&mut f);
    let name = find(&seen, "b").unwrap().rect.center();
    pane.click(name, &mut f);
    pane.frame(vec![key(Key::Tab, Modifiers::NONE)], &mut f);
    pane.frame(vec![key(Key::End, Modifiers::NONE)], &mut f);
    pane.frame(vec![text_event('9')], &mut f);
    pane.frame(vec![key(Key::Tab, Modifiers::SHIFT)], &mut f);
    pane.frame(vec![key(Key::End, Modifiers::NONE)], &mut f);
    pane.frame(vec![text_event('z')], &mut f);
    pane.settle(&mut f);
    drop(f);
    assert_eq!(d.text(b, Column::Formula), Some("a/29"));
    assert_eq!(d.text(b, Column::Name), Some("bz"));
}

/// F2: Was die Zeile als Zahl zeigt („1e3“, „+5“, „5.“, „5,“), nimmt die Übernahme auch an
/// und schreibt es als Zahl.
#[test]
fn row_numbers_commit_like_they_show() {
    for (typed, stored) in [
        ("1e3", "1000"),
        ("+5", "5"),
        ("5.", "5"),
        ("5,", "5"),
        (" 7 ", "7"),
    ] {
        let mut s = AppState::new();
        let a = param(&mut s, "a", "40", ParamUnit::Length);
        let mut d = ParameterDialog::new(&s);
        d.set_text(a, Column::Formula, typed);
        let mut pane = Pane::new(true);
        let seen = pane.settle(&mut |ui| {
            d.show(ui.ctx(), &mut s, false);
        });
        assert!(
            !seen.iter().any(|t| t.color == theme::palette(true).error),
            "„{typed}“ gilt als gültig"
        );
        d.commit(a, &mut s).unwrap();
        assert_eq!(s.document.parameter(a).unwrap().formula, stored, "{typed}");
    }
    // Eine gültige Formel bleibt wie getippt.
    let mut s = AppState::new();
    let a = param(&mut s, "a", "40", ParamUnit::Length);
    let mut d = ParameterDialog::new(&s);
    d.set_text(a, Column::Formula, "0.50");
    d.commit(a, &mut s).unwrap();
    assert_eq!(s.document.parameter(a).unwrap().formula, "0.50");
}

/// F4: Die Wertanzeige „= …“ einer Zeile folgt dem getippten Entwurf, die Spalte „Wert“ zeigt
/// weiter das Dokument.
#[test]
fn row_preview_follows_the_draft() {
    let mut s = AppState::new();
    param(&mut s, "a", "10", ParamUnit::Length);
    let b = param(&mut s, "b", "a*2", ParamUnit::Length);
    let mut d = ParameterDialog::new(&s);
    let mut pane = Pane::new(true);
    let seen = pane.settle(&mut |ui| {
        d.show(ui.ctx(), &mut s, false);
    });
    assert!(find(&seen, "= 20,00 mm").is_some());
    d.set_text(b, Column::Formula, "a*3");
    let seen = pane.settle(&mut |ui| {
        d.show(ui.ctx(), &mut s, false);
    });
    assert!(find(&seen, "= 30,00 mm").is_some(), "Vorschau des Entwurfs");
    assert!(find(&seen, "= 20,00 mm").is_none());
    assert!(find(&seen, "20,00 mm").is_some(), "Spalte Wert: Dokument");
}
