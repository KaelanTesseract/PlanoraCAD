//! Formelfeld und Fenster „Parameter“ (Aufgabe 9): „fx“, Wertanzeige und Fehler im Feld,
//! Übernahme bei Enter oder Verlassen des Feldes, Löschen mit Rückfrage und „Durch den Wert
//! ersetzen“, Umbenennen, Fehler bleiben in der Zeile, Sperren, Rückgängig bei offenem Fenster,
//! Größe bei 800 × 600 und 1100 × 700 in beiden Themen. Mit echten Bildern (`run_ui`) und
//! Mausklicks; die ganze App über `PlanoraApp::for_tests`.

mod common;

use eframe::App as _;
use egui::{Event, Key, Modifiers, PointerButton, Pos2, Rect, Vec2, vec2};
use planora_app::app::{PARAMETERS_ENTRY, PlanoraApp};
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
        let o = formula_field(ui, text, ParamUnit::Length, &scope, 100.0);
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
        let o = formula_field(ui, &mut text, ParamUnit::Length, &scope, 100.0);
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
        formula_field(ui, &mut text, ParamUnit::Length, &scope, 100.0);
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
        let o = formula_field(ui, &mut text, ParamUnit::Length, &scope, 100.0);
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
    let entry = find(&seen, PARAMETERS_ENTRY).expect("Eintrag „Parameter …“");
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
    assert!(visible(&seen, TITLE, rect), "Titel „Parameter“");
    // Ohne Zeilen der Hinweis, mit einer Zeile alle Spaltenköpfe.
    assert!(visible(&seen, EMPTY_TABLE, rect));
    let add = find(&seen, ADD_ROW).unwrap().rect.center();
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
    let p = find(&seen, PARAMETERS_ENTRY)
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
    let add = find(&seen, ADD_ROW).expect("Knopf").rect.center();
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
        assert_eq!(d.confirm_text(), Some("Breite wird in Quader 1 benutzt."));
    }
    assert!(
        rig.tab().state.snapshot().same_content(&before),
        "noch nichts gelöscht"
    );
    let seen = rig.settle();
    let rect = rig.dialog_rect();
    assert!(visible(&seen, "Breite wird in Quader 1 benutzt.", rect));
    assert!(visible(&seen, REPLACE, rect), "„Durch den Wert ersetzen“");
    assert!(visible(&seen, CANCEL, rect), "„Abbrechen“");

    // Abbrechen: nichts ändert sich.
    let cancel = find(&seen, CANCEL).unwrap().rect.center();
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
    let replace = find(&seen, REPLACE).expect("Rückfrage").rect.center();
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
    assert_eq!(d.error(), Some(e.as_str()));
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
    assert_eq!(d.error(), Some(e.as_str()));
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
    let add = find(&seen, ADD_ROW).unwrap().rect.center();
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
    locked_case(&mut s, true, planora_app::drawing_browser::READ_ONLY_HINT);
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
    assert_eq!(tab.state.info.as_deref(), Some(DIALOG_CLOSED));
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
                visible(&seen, ADD_ROW, rect),
                "„Zeile hinzufügen“ ohne Rollen"
            );
            assert!(
                !visible(&seen, "Laenge30", rect),
                "{w}×{h}: die Tabelle rollt (letzte Zeile erst nach dem Rollen)"
            );
            let add = find(&seen, ADD_ROW).unwrap().rect.center();
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
