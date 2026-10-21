//! Formelfelder in den Fenstern „… erzeugen“, „… bearbeiten“, „Verrunden“/„Fase“ und im
//! Eigenschaftenfeld des Netz-Körpers (Aufgabe 10): Formeln werden gespeichert und bestimmen ihr
//! Feld, eine getippte Zahl löscht nur die Formel ihres Feldes, fehlerhafte Formeln ändern nichts
//! und sperren „Erzeugen“/„Übernehmen“, die variable Rundung bleibt bei Zahlen. Mit echten
//! Bildern (`run_ui`), Mausklicks und Tastatur; „Quader erzeugen“ über die ganze App.

mod common;

use eframe::App as _;
use egui::{Event, Key, Modifiers, PointerButton, Pos2, Rect, Vec2, vec2};
use planora_app::app::PlanoraApp;
use planora_app::blend_dialog::{ArtChoice, BlendDialog, angle_message};
use planora_app::create_dialog::CreateDialog;
use planora_app::feature_dialog::FeatureDialog;
use planora_app::state::*;
use planora_app::workspace::*;
use planora_app::{fonts, properties, theme};
use planora_exchange::ExchangeMesh;
use planora_feature::{
    BlendArt, FeatureId, FeatureKind, FieldKey, Formulas, MAX_CHAMFER_ANGLE_DEG,
    MIN_CHAMFER_ANGLE_DEG, ParamEdit, ParamId, ParamUnit, SketchPlane, StandardPlane,
};
use planora_kernel::blend::topo::{edge_uses, face_name};
use planora_kernel::{Edge, Id, Solid};
use planora_math::{DVec2, DVec3};
use planora_project::config::Config;
use planora_project::library::Library;
use planora_render::{EdgeRef, Pick};
use std::collections::BTreeMap;
use std::sync::Arc;

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

/// Bilder mit einem eigenen Kontext; `f` bekommt die Fläche.
struct Pane {
    ctx: egui::Context,
    time: f64,
    size: Vec2,
}

impl Pane {
    fn new(dark: bool, size: Vec2) -> Pane {
        let ctx = egui::Context::default();
        ctx.set_fonts(fonts::definitions());
        theme::apply(&ctx, dark);
        Pane {
            ctx,
            time: 0.0,
            size,
        }
    }

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

    /// Klickt in das Feld rechts neben der Bezeichnung `label` und markiert alles.
    fn focus_beside(&mut self, label: &str, f: &mut dyn FnMut(&mut egui::Ui)) {
        let seen = self.settle(f);
        let l = find(&seen, label)
            .unwrap_or_else(|| panic!("„{label}“ fehlt"))
            .rect;
        let field = seen
            .iter()
            .filter(|s| {
                s.rect.left() > l.right() + 2.0 && (s.rect.center().y - l.center().y).abs() < 6.0
            })
            .min_by(|a, b| a.rect.left().total_cmp(&b.rect.left()))
            .unwrap_or_else(|| panic!("kein Feld neben „{label}“"))
            .rect;
        // „fx“ steht vor einer Formel: dann das nächste Element.
        let field = if seen.iter().any(|s| s.text == "fx" && s.rect == field) {
            seen.iter()
                .filter(|s| {
                    s.rect.left() > field.right() + 1.0
                        && (s.rect.center().y - l.center().y).abs() < 6.0
                })
                .min_by(|a, b| a.rect.left().total_cmp(&b.rect.left()))
                .unwrap()
                .rect
        } else {
            field
        };
        self.click(field.center(), f);
        self.frame(vec![key(Key::A, Modifiers::COMMAND)], f);
    }

    /// Tippt `text` in das Feld neben `label` (alles ersetzt), ohne Enter.
    fn type_beside(&mut self, label: &str, text: &str, f: &mut dyn FnMut(&mut egui::Ui)) {
        self.focus_beside(label, f);
        self.frame(vec![Event::Text(text.into())], f);
        self.frame(vec![], f);
    }

    fn enter(&mut self, f: &mut dyn FnMut(&mut egui::Ui)) -> Vec<Seen> {
        self.frame(vec![key(Key::Enter, Modifiers::NONE)], f);
        self.settle(f)
    }
}

/// Neuer Parameter über die App.
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

fn set_formula(s: &mut AppState, id: ParamId, formula: &str) {
    let p = s.document.parameter(id).unwrap().clone();
    s.edit_parameter(
        id,
        ParamEdit {
            name: p.name,
            formula: formula.into(),
            unit: p.unit,
            comment: p.comment,
        },
    )
    .unwrap();
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

/// Zustand mit Breite = 40 (Länge).
fn with_width() -> (AppState, ParamId) {
    let mut s = AppState::new();
    let b = param(&mut s, "Breite", "40", ParamUnit::Length);
    (s, b)
}

fn ws() -> Workspace {
    let root = common::temp_root("planora-formula-dialogs");
    Workspace::new(Some(Library::open(&root).unwrap()), Config::default())
}

/// Einheitswürfel als Netz.
fn cube() -> ExchangeMesh {
    ExchangeMesh {
        positions: vec![
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
            [1.0, 0.0, 1.0],
            [1.0, 1.0, 1.0],
            [0.0, 1.0, 1.0],
        ],
        indices: vec![
            0, 2, 1, 0, 3, 2, 4, 5, 6, 4, 6, 7, 0, 1, 5, 0, 5, 4, 3, 7, 6, 3, 6, 2, 0, 4, 7, 0, 7,
            3, 1, 2, 6, 1, 6, 5,
        ],
    }
}

fn edge_between(s: &Solid, a: &str, b: &str) -> Id<Edge> {
    s.edges
        .iter()
        .map(|(id, _)| id)
        .find(|&id| {
            let n: Vec<String> = edge_uses(s, id)
                .iter()
                .map(|u| face_name(s, u.face))
                .collect();
            n.len() == 2 && ((n[0] == a && n[1] == b) || (n[0] == b && n[1] == a))
        })
        .unwrap()
}

// ---- Ganze App -------------------------------------------------------------------------------

const SIZE: Vec2 = vec2(1400.0, 900.0);

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

    /// Text `text` unterhalb des Fenstertitels `title` (das Fenster liegt über der Ansicht).
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

// ---- (a) Quader erzeugen mit Formeln ----------------------------------------------------------

#[test]
fn create_box_with_formula() {
    let mut w = ws();
    param(
        &mut w.active_tab_mut().state,
        "Breite",
        "40",
        ParamUnit::Length,
    );
    w.observe_history(false);
    let mut rig = Rig::new(w);
    let title = "Quader erzeugen";
    rig.tab().dialog = Some(CreateDialog::Box {
        x: 11.0,
        y: 12.0,
        z: 13.0,
    });
    rig.retype(title, "11", "Breite");
    rig.retype(title, "12", "20");
    rig.retype(title, "13", "Breite/4");
    let seen = rig.settle();
    assert!(has(&seen, "fx"), "„fx“ vor der Formel");
    assert!(has(&seen, "= 40,00 mm"));
    assert!(has(&seen, "= 10,00 mm"));
    // Ohne „Erzeugen“ geschieht nichts.
    assert!(rig.tab().state.document.features().is_empty());
    let steps = rig.tab().history().undo_len();
    let erzeugen = rig.in_window(title, "Erzeugen").expect("Knopf „Erzeugen“");
    rig.click(erzeugen.center());
    rig.settle();
    let tab = rig.tab();
    assert!(tab.dialog.is_none(), "nach Erfolg geschlossen");
    let f = tab.state.document.features()[0].id;
    assert_eq!(box_size(&tab.state, f), DVec3::new(40.0, 20.0, 10.0));
    assert_eq!(
        feature_formulas(&tab.state, f),
        formulas(&[(FieldKey::Breite, "Breite"), (FieldKey::Hoehe, "Breite/4")])
    );
    assert_eq!(tab.history().undo_len(), steps + 1, "genau ein Schritt");
    assert_eq!(tab.history().undo_label(), Some("Quader anlegen"));

    // Neu geöffnet: wieder die Vorgaben, nicht die Texte von eben.
    rig.tab().dialog = Some(CreateDialog::Box {
        x: 11.0,
        y: 12.0,
        z: 13.0,
    });
    assert!(rig.in_window(title, "11").is_some(), "Vorgabe 11");
    assert!(rig.in_window(title, "Breite/4").is_none());

    // Fehlerhafte Formel: Meldung mit Feldname, „Erzeugen“ gesperrt, nichts entsteht.
    rig.retype(title, "11", "Breit");
    let seen = rig.settle();
    let msg = "Breite (x): Den Namen „Breit“ gibt es nicht.";
    let m = find(&seen, msg).unwrap_or_else(|| panic!("Meldung fehlt"));
    assert_eq!(m.color, theme::palette(true).error);
    let erzeugen = rig.in_window(title, "Erzeugen").unwrap();
    rig.tab().state.message = None;
    rig.click(erzeugen.center());
    rig.settle();
    let tab = rig.tab();
    assert!(tab.dialog.is_some(), "Fenster bleibt offen");
    assert_eq!(tab.state.message, None, "„Erzeugen“ gesperrt: kein Versuch");
    assert_eq!(tab.state.document.features().len(), 1, "kein neues Feature");
    // Direkt: dieselbe Meldung in der Statuszeile.
    let d = tab.dialog.clone().unwrap();
    let texts: BTreeMap<FieldKey, String> = [(FieldKey::Breite, "Breit".to_string())].into();
    assert!(!d.create_with(&mut tab.state, &texts));
    assert_eq!(tab.state.message.as_deref(), Some(msg));
    assert_eq!(tab.state.document.features().len(), 1);

    // Auf anderem Weg geschlossen (etwa Rückgängig): die Texte sind vergessen.
    rig.tab().dialog = None;
    rig.settle();
    rig.tab().dialog = Some(CreateDialog::Box {
        x: 11.0,
        y: 12.0,
        z: 13.0,
    });
    assert!(
        rig.in_window(title, "Breit").is_none(),
        "alter Text vergessen"
    );
    assert!(rig.in_window(title, "11").is_some());
}

#[test]
fn plain_numbers_create_the_same_body_as_before() {
    let mut a = AppState::new();
    let mut b = AppState::new();
    let d = CreateDialog::Cylinder {
        radius: 15.0,
        height: 30.0,
    };
    let texts: BTreeMap<FieldKey, String> = [
        (FieldKey::Radius, "4,5".to_string()),
        (FieldKey::Hoehe, "40".to_string()),
    ]
    .into();
    assert!(d.create_with(&mut a, &texts));
    b.create_cylinder(4.5, 40.0).unwrap();
    let (fa, fb) = (&a.document.features()[0], &b.document.features()[0]);
    assert_eq!(fa.kind, fb.kind);
    assert!(fa.formulas().is_empty());
    assert_eq!(fa.name, fb.name);
    // Ohne Texte: die Vorgaben.
    assert!(d.create(&mut a));
    match &a.document.features()[1].kind {
        FeatureKind::Cylinder { radius, height, .. } => {
            assert_eq!((*radius, *height), (15.0, 30.0))
        }
        k => panic!("{k:?}"),
    }
    // Ein Wert ≤ 0 aus einer Formel: abgelehnt, Meldung, nichts entsteht.
    let (mut s, _) = with_width();
    let bad: BTreeMap<FieldKey, String> = [(FieldKey::Radius, "Breite-40".to_string())].into();
    assert!(!d.create_with(&mut s, &bad));
    assert!(s.document.features().is_empty());
    assert!(s.message.as_deref().unwrap().contains("größer als null"));
    // Ein Winkel-Parameter in einem Längenfeld: die Einheitenmeldung.
    param(&mut s, "w", "30", ParamUnit::Angle);
    let bad: BTreeMap<FieldKey, String> = [(FieldKey::Radius, "w".to_string())].into();
    assert!(!d.create_with(&mut s, &bad));
    assert_eq!(
        s.message.as_deref(),
        Some("Radius: Hier wird eine Länge erwartet, die Formel ergibt einen Winkel.")
    );
}

// ---- (b) Bearbeiten zeigt und löscht Formeln --------------------------------------------------

#[test]
fn edit_dialog_shows_and_clears_formula() {
    let (mut s, breite) = with_width();
    let f = s
        .create_box_with(
            DVec3::new(40.0, 20.0, 10.0),
            formulas(&[(FieldKey::Breite, "Breite"), (FieldKey::Hoehe, "Breite/4")]),
        )
        .unwrap();
    let mut d = FeatureDialog::new(&s, f).unwrap();
    assert_eq!(d.text("Breite"), Some("Breite"));
    assert_eq!(d.text("Höhe"), Some("Breite/4"));
    assert_eq!(d.text("Tiefe"), Some("20"));
    assert_eq!(d.text("Gibt es nicht"), None);
    assert!(d.set_text("Breite", "7"));
    d.apply(&mut s).unwrap();
    assert_eq!(box_size(&s, f).x, 7.0);
    assert_eq!(
        feature_formulas(&s, f),
        formulas(&[(FieldKey::Hoehe, "Breite/4")]),
        "nur die Breite-Formel entfällt"
    );
    set_formula(&mut s, breite, "80");
    assert_eq!(box_size(&s, f), DVec3::new(7.0, 20.0, 20.0));
    let mut d = FeatureDialog::new(&s, f).unwrap();
    assert_eq!(d.text("Breite"), Some("7"));
    assert!(d.set_text("Breite", "Breite*2"));
    d.apply(&mut s).unwrap();
    assert_eq!(box_size(&s, f).x, 160.0);
    assert_eq!(
        feature_formulas(&s, f),
        formulas(&[
            (FieldKey::Breite, "Breite*2"),
            (FieldKey::Hoehe, "Breite/4")
        ])
    );
    // Übernehmen ohne Änderung behält alle Formeln (nicht still verworfen).
    let d = FeatureDialog::new(&s, f).unwrap();
    d.apply(&mut s).unwrap();
    assert_eq!(feature_formulas(&s, f).len(), 2);
}

// ---- (c) Fehlerhafte Formel ändert nichts -----------------------------------------------------

#[test]
fn edit_dialog_rejects_bad_formula() {
    let (mut s, _) = with_width();
    let f = s.create_box(DVec3::new(40.0, 20.0, 10.0)).unwrap();
    let before = s.document.snapshot();
    let edits = s.document.edit_count();
    let mut d = FeatureDialog::new(&s, f).unwrap();
    d.set_text("Breite", "Breit");
    assert_eq!(
        d.apply(&mut s),
        Err("Breite: Den Namen „Breit“ gibt es nicht.".to_string())
    );
    assert!(s.document.snapshot().same_content(&before));
    assert_eq!(s.document.edit_count(), edits);
    // Ein Winkel-Parameter im Längenfeld: Einheitenmeldung, nichts ändert sich.
    param(&mut s, "w", "30", ParamUnit::Angle);
    let before = s.document.snapshot();
    let mut d = FeatureDialog::new(&s, f).unwrap();
    d.set_text("Position x", "w");
    assert_eq!(
        d.apply(&mut s),
        Err("Position x: Hier wird eine Länge erwartet, die Formel ergibt einen Winkel.".into())
    );
    assert!(s.document.snapshot().same_content(&before));
}

#[test]
fn the_edit_window_shows_fx_value_and_locks_apply_on_error() {
    for dark in [true, false] {
        let (mut s, breite) = with_width();
        set_formula(&mut s, breite, "80");
        let f = s
            .create_box_with(
                DVec3::new(80.0, 20.0, 10.0),
                formulas(&[(FieldKey::Breite, "Breite")]),
            )
            .unwrap();
        let mut d = FeatureDialog::new(&s, f).unwrap();
        let mut pane = Pane::new(dark, vec2(1100.0, 700.0));
        let mut open = true;
        let mut draw = |ui: &mut egui::Ui| {
            open = d.show(ui.ctx(), &mut s);
        };
        let seen = pane.settle(&mut draw);
        assert!(has(&seen, "fx"));
        // Der Wert steht ganz da, nicht auf „…“ gekürzt.
        let v = find(&seen, "= 80,00 mm").expect("Wert");
        assert!(v.rect.width() > 50.0, "Wert {:?} gekürzt", v.rect);
        assert!(v.clip.contains_rect(v.rect));
        assert!(has(&seen, "Namen: Breite"));
        // Abstand der Zeilen Breite → Tiefe (das Fenster selbst darf mit der Meldung wachsen).
        // Die Bezeichnungen stehen vor den Feldern (das erste „Breite“ ist die Bezeichnung).
        let top = |seen: &[Seen], t: &str| seen.iter().find(|s| s.text == t).unwrap().rect.top();
        let row_gap = |seen: &[Seen]| top(seen, "Tiefe") - top(seen, "Breite");
        let gap = row_gap(&seen);
        drop(draw);
        d.set_text("Breite", "Breit");
        let mut draw = |ui: &mut egui::Ui| {
            open = d.show(ui.ctx(), &mut s);
        };
        let seen = pane.settle(&mut draw);
        let m = find(&seen, "Breite: Den Namen „Breit“ gibt es nicht.").expect("Meldung");
        assert_eq!(m.color, theme::palette(dark).error);
        assert!(m.clip.contains_rect(m.rect), "Meldung ganz sichtbar");
        assert_eq!(
            row_gap(&seen),
            gap,
            "die Meldung verschiebt die Felder nicht"
        );
        let apply = find(&seen, "Übernehmen").unwrap().rect.center();
        pane.click(apply, &mut draw);
        drop(draw);
        assert!(open, "Fenster bleibt offen");
        assert_eq!(d.error(), None, "„Übernehmen“ gesperrt: kein Versuch");
        assert_eq!(
            feature_formulas(&s, f),
            formulas(&[(FieldKey::Breite, "Breite")])
        );
    }
}

// ---- (d) Verschieben und Extrusion ------------------------------------------------------------

#[test]
fn move_and_extrude() {
    let (mut s, _) = with_width();
    s.create_box(DVec3::splat(10.0)).unwrap();
    s.move_body(0, DVec3::new(1.0, 0.0, 0.0)).unwrap();
    let mv = s.document.features().last().unwrap().id;
    let mut d = FeatureDialog::new(&s, mv).unwrap();
    assert_eq!(d.labels(), ["Δx", "Δy", "Δz"]);
    d.set_text("Δx", "Breite/2");
    d.apply(&mut s).unwrap();
    match &s.document.feature(mv).unwrap().kind {
        FeatureKind::Move { delta, .. } => assert_eq!(delta.x, 20.0),
        k => panic!("{k:?}"),
    }
    assert_eq!(
        feature_formulas(&s, mv),
        formulas(&[(FieldKey::X, "Breite/2")])
    );
    assert_eq!(
        FeatureDialog::new(&s, mv).unwrap().text("Δx"),
        Some("Breite/2")
    );

    let sk = s
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    s.open_sketch(sk).unwrap();
    s.session_sketch_mut()
        .unwrap()
        .1
        .add_rectangle(DVec2::new(50.0, 0.0), DVec2::new(54.0, 3.0))
        .unwrap();
    s.close_sketch();
    let texts: BTreeMap<FieldKey, String> = [(FieldKey::Hoehe, "Breite/8".to_string())].into();
    assert!(CreateDialog::extrude_sketch(sk).create_with(&mut s, &texts));
    let e = s.document.features().last().unwrap().id;
    match &s.document.feature(e).unwrap().kind {
        FeatureKind::Extrude { height, .. } => assert_eq!(*height, 5.0),
        k => panic!("{k:?}"),
    }
    assert_eq!(
        feature_formulas(&s, e),
        formulas(&[(FieldKey::Hoehe, "Breite/8")])
    );
    assert_eq!(
        FeatureDialog::new(&s, e).unwrap().text("Höhe"),
        Some("Breite/8")
    );
}

// ---- (e) Variable Rundung bleibt bei Zahlen ---------------------------------------------------

/// Quader 20³, Rundung r = 2 an der Kante Deckel/Wand 1.
fn with_fillet() -> (AppState, FeatureId) {
    let (mut s, _) = with_width();
    s.create_box(DVec3::splat(20.0)).unwrap();
    let e = edge_between(&s.bodies[0].solid, "Deckel", "Wand 1");
    let id = s
        .add_blend(0, &[e], BlendArt::Rundung { radius: 2.0 }, true)
        .unwrap();
    (s, id)
}

#[test]
fn variable_fillet_stays_numeric() {
    let (mut s, id) = with_fillet();
    let mut d = FeatureDialog::new(&s, id).unwrap();
    d.set_blend_art(ArtChoice::RundungVariabel);
    d.set_text("Position 1", "Breite");
    let edits = s.document.edit_count();
    assert_eq!(
        d.apply(&mut s),
        Err("Position 1: Hier sind nur Zahlen möglich.".to_string())
    );
    assert_eq!(s.document.edit_count(), edits);
    // Zahlen gehen wie bisher.
    d.set_text("Position 1", "0");
    d.set_text("Radius 2", "1,5");
    d.apply(&mut s).unwrap();
    assert!(feature_formulas(&s, id).is_empty());
}

#[test]
fn the_blend_edit_keeps_formulas_of_fields_that_stay() {
    let (mut s, id) = with_fillet();
    let mut d = FeatureDialog::new(&s, id).unwrap();
    d.set_text("Radius", "Breite/20");
    d.apply(&mut s).unwrap();
    assert_eq!(
        feature_formulas(&s, id),
        formulas(&[(FieldKey::Radius, "Breite/20")])
    );
    let mut d = FeatureDialog::new(&s, id).unwrap();
    assert_eq!(d.text("Radius"), Some("Breite/20"));
    // Fase mit Winkel: neue Felder, die Radius-Formel gibt es dort nicht mehr.
    d.set_blend_art(ArtChoice::FaseWinkel);
    assert_eq!(d.text("Abstand"), Some("2"));
    d.set_text("Winkel", "Breite");
    assert_eq!(
        d.apply(&mut s),
        Err("Winkel: Hier wird ein Winkel erwartet, die Formel ergibt eine Länge.".to_string())
    );
    d.set_text("Winkel", "30");
    d.apply(&mut s).unwrap();
    assert!(feature_formulas(&s, id).is_empty());
}

// ---- Verrunden/Fase erzeugen mit Formeln ------------------------------------------------------

#[test]
fn the_blend_create_window_takes_formulas() {
    let (mut s, _) = with_width();
    param(&mut s, "w", "30", ParamUnit::Angle);
    s.create_box(DVec3::splat(20.0)).unwrap();
    let e = edge_between(&s.bodies[0].solid, "Deckel", "Wand 1");
    s.set_selection(Some(Pick::Edge(EdgeRef { body: 0, edge: e })));
    let mut d = BlendDialog::fillet();
    d.first = "w".into();
    assert!(d.execute(&mut s), "Winkel im Radius: offen");
    assert_eq!(
        d.error(),
        Some("Radius: Hier wird eine Länge erwartet, die Formel ergibt einen Winkel.")
    );
    assert_eq!(s.document.features().len(), 1);
    d.first = "Breite/20".into();
    assert!(!d.execute(&mut s), "{:?}", d.error());
    let f = s.document.features().last().unwrap().id;
    assert_eq!(
        feature_formulas(&s, f),
        formulas(&[(FieldKey::Radius, "Breite/20")])
    );
    match &s.document.feature(f).unwrap().kind {
        FeatureKind::Verrunden { art, .. } => assert_eq!(*art, BlendArt::Rundung { radius: 2.0 }),
        k => panic!("{k:?}"),
    }

    // Fase mit Abstand und Winkel aus Parametern.
    let (mut s, _) = with_width();
    param(&mut s, "w", "30", ParamUnit::Angle);
    s.create_box(DVec3::splat(20.0)).unwrap();
    let e = edge_between(&s.bodies[0].solid, "Deckel", "Wand 1");
    s.set_selection(Some(Pick::Edge(EdgeRef { body: 0, edge: e })));
    let mut d = BlendDialog::chamfer();
    d.set_art(ArtChoice::FaseWinkel);
    d.first = "Breite/40".into();
    d.second = "w".into();
    assert!(!d.execute(&mut s), "{:?}", d.error());
    let f = s.document.features().last().unwrap().id;
    assert_eq!(
        feature_formulas(&s, f),
        formulas(&[(FieldKey::Abstand, "Breite/40"), (FieldKey::Winkel, "w")])
    );
    // Ein Winkel aus einer Formel außerhalb 1°…179°: dieselbe Meldung wie bei einer Zahl.
    let mut d = BlendDialog::chamfer();
    d.set_art(ArtChoice::FaseWinkel);
    d.second = "w*10".into();
    assert_eq!(
        d.art_value_with(&s.document.param_scope()),
        Err(angle_message("w*10"))
    );
    // Zwei Abstände: zweites Feld „Abstand 2“.
    d.set_art(ArtChoice::FaseZwei);
    d.second = "Breite/20".into();
    let (art, f) = d.art_value_with(&s.document.param_scope()).unwrap();
    assert_eq!(
        art,
        BlendArt::FaseZwei {
            abstand1: 1.0,
            abstand2: 2.0
        }
    );
    assert_eq!(f, formulas(&[(FieldKey::Abstand2, "Breite/20")]));
}

#[test]
fn the_dialog_angle_limits_are_the_document_limits() {
    // Kein eigener Wert im Fenster: die Grenzen des Dokuments gelten (Abweichung unmöglich).
    let mut d = BlendDialog::chamfer();
    d.set_art(ArtChoice::FaseWinkel);
    let below = MIN_CHAMFER_ANGLE_DEG - 0.01;
    let above = MAX_CHAMFER_ANGLE_DEG + 0.01;
    for (w, ok) in [
        (MIN_CHAMFER_ANGLE_DEG, true),
        (MAX_CHAMFER_ANGLE_DEG, true),
        (below, false),
        (above, false),
    ] {
        d.second = w.to_string();
        assert_eq!(d.art_value().is_ok(), ok, "{w}");
    }
    assert!(angle_message("0").contains(&format!(
        "zwischen {MIN_CHAMFER_ANGLE_DEG:.0}° und {MAX_CHAMFER_ANGLE_DEG:.0}°"
    )));
}

// ---- (f) Netz-Körper im Eigenschaftenfeld -----------------------------------------------------

fn offset(s: &AppState) -> DVec3 {
    s.mesh_transform(0).unwrap().offset
}

#[test]
fn mesh_body_fields() {
    let (mut s, breite) = with_width();
    s.import_mesh("N", Arc::new(cube()), 1.0).unwrap();
    let fid = s.document.features()[0].id;
    let t0 = s.mesh_transform(0).unwrap();
    let mut pane = Pane::new(true, vec2(1100.0, 700.0));
    let edits = s.document.edit_count();
    let mut draw = |ui: &mut egui::Ui| {
        panel(ui, &mut s);
    };
    let seen = pane.settle(&mut draw);
    assert!(has(&seen, "Drehung x / y / z in Grad"), "Drehung bleibt");
    // Tippen allein wirkt nicht.
    pane.type_beside("Position x", "Breite", &mut draw);
    drop(draw);
    assert_eq!(s.document.edit_count(), edits, "keine Wirkung ohne Enter");
    let mut draw = |ui: &mut egui::Ui| {
        panel(ui, &mut s);
    };
    let seen = pane.enter(&mut draw);
    assert!(
        has(&seen, "fx") && has(&seen, "= 40,00 mm"),
        "Formel mit Wert"
    );
    drop(draw);
    assert_eq!(offset(&s).x, 40.0);
    assert_eq!(
        feature_formulas(&s, fid),
        formulas(&[(FieldKey::X, "Breite")])
    );

    // Parameter ändern: das Feld zeigt den neuen Wert.
    set_formula(&mut s, breite, "50");
    assert_eq!(offset(&s).x, 50.0);
    let mut draw = |ui: &mut egui::Ui| {
        panel(ui, &mut s);
    };
    let seen = pane.settle(&mut draw);
    assert!(has(&seen, "= 50,00 mm"));

    // Eine Zahl löscht die Formel.
    pane.type_beside("Position x", "7", &mut draw);
    pane.enter(&mut draw);
    drop(draw);
    assert_eq!(offset(&s).x, 7.0);
    assert!(feature_formulas(&s, fid).is_empty());

    // Länge im Maßstab: rote Meldung, Lage unverändert.
    let before = s.mesh_transform(0).unwrap();
    let edits = s.document.edit_count();
    let mut draw = |ui: &mut egui::Ui| {
        panel(ui, &mut s);
    };
    pane.type_beside("Maßstab", "Breite", &mut draw);
    let seen = pane.enter(&mut draw);
    let m = find(
        &seen,
        "Hier wird eine Zahl erwartet, die Formel ergibt eine Länge.",
    )
    .expect("Meldung");
    assert_eq!(m.color, theme::palette(true).error);
    drop(draw);
    assert_eq!(s.mesh_transform(0).unwrap(), before);
    assert_eq!(s.document.edit_count(), edits);

    // Eine Parameteränderung lädt die Felder neu: der verworfene Entwurf ist weg.
    set_formula(&mut s, breite, "60");
    let mut draw = |ui: &mut egui::Ui| {
        panel(ui, &mut s);
    };
    // Fokus weg (Klick ins Leere), dann neu zeichnen.
    pane.click(pos2_free(), &mut draw);
    let seen = pane.settle(&mut draw);
    assert!(
        !has(
            &seen,
            "Hier wird eine Zahl erwartet, die Formel ergibt eine Länge."
        ),
        "Entwurf verworfen"
    );
    drop(draw);
    assert_eq!(s.mesh_transform(0).unwrap().scale, t0.scale);

    // Maßstab „0“ (Zahl, unzulässig): abgelehnt, Text und Meldung bleiben im Feld.
    let mut draw = |ui: &mut egui::Ui| {
        panel(ui, &mut s);
    };
    pane.type_beside("Maßstab", "0", &mut draw);
    let seen = pane.enter(&mut draw);
    assert!(
        seen.iter()
            .any(|x| x.text.contains("Maßstab") && x.color == theme::palette(true).error),
        "Meldung der Ablehnung im Feld"
    );
    drop(draw);
    assert_eq!(s.mesh_transform(0).unwrap().scale, t0.scale);
}

/// Eigenschaftenfeld in Bereichsbreite (340 px).
fn panel(ui: &mut egui::Ui, s: &mut AppState) {
    ui.set_max_width(340.0);
    properties::show(ui, s);
}

/// Eine Stelle rechts im Bild ohne Bedienelement.
fn pos2_free() -> Pos2 {
    Pos2::new(900.0, 680.0)
}

#[test]
fn undo_reloads_the_mesh_fields() {
    let mut w = ws();
    let s = &mut w.active_tab_mut().state;
    param(s, "Breite", "40", ParamUnit::Length);
    s.import_mesh("N", Arc::new(cube()), 1.0).unwrap();
    w.observe_history(false);
    let s = &mut w.active_tab_mut().state;
    s.set_mesh_formula(0, FieldKey::X, "Breite").unwrap();
    w.observe_history(false);
    let mut pane = Pane::new(false, vec2(1100.0, 700.0));
    let mut draw = |ui: &mut egui::Ui| {
        let s = &mut w.active_tab_mut().state;
        panel(ui, s);
    };
    let seen = pane.settle(&mut draw);
    assert!(has(&seen, "Breite"), "Formel im Feld");
    drop(draw);
    w.active_tab_mut().undo().unwrap();
    let mut draw = |ui: &mut egui::Ui| {
        let s = &mut w.active_tab_mut().state;
        panel(ui, s);
    };
    let seen = pane.settle(&mut draw);
    assert!(!has(&seen, "fx"), "nach Rückgängig keine Formel");
    drop(draw);
    assert_eq!(
        w.active_tab_mut().state.mesh_transform(0).unwrap().offset.x,
        0.0
    );
}

// ---- (g) Namenshinweis und Aussehen -----------------------------------------------------------

#[test]
fn names_hint_shown() {
    let (mut s, _) = with_width();
    let mut d = CreateDialog::Box {
        x: 10.0,
        y: 20.0,
        z: 30.0,
    };
    let mut pane = Pane::new(true, vec2(1100.0, 700.0));
    let seen = pane.settle(&mut |ui| {
        d.show(ui.ctx(), &mut s);
    });
    assert!(has(&seen, "Namen: Breite"));
    assert!(has(&seen, "mm"), "Einheit wie bisher");
    // Ohne Parameter kein Hinweis.
    let mut s = AppState::new();
    let seen = pane.settle(&mut |ui| {
        d.show(ui.ctx(), &mut s);
    });
    assert!(!seen.iter().any(|x| x.text.starts_with("Namen:")));
}

#[test]
fn closing_the_window_forgets_its_texts() {
    let (mut s, _) = with_width();
    let mut d = CreateDialog::Sphere { radius: 15.0 };
    let mut pane = Pane::new(true, vec2(1100.0, 700.0));
    d.set_text(&pane.ctx, &s, FieldKey::Radius, "Breite/4");
    let mut open = true;
    let mut draw = |ui: &mut egui::Ui| {
        if open {
            open = d.show(ui.ctx(), &mut s);
        }
    };
    let seen = pane.settle(&mut draw);
    assert!(has(&seen, "= 10,00 mm"));
    let cancel = find(&seen, "Abbrechen").unwrap().rect.center();
    pane.click(cancel, &mut draw);
    drop(draw);
    assert!(!open);
    assert_eq!(d.texts(&pane.ctx, &s)[&FieldKey::Radius], "15", "Vorgabe");
    assert!(s.document.features().is_empty());
}

#[test]
fn create_window_fits_small_screens_in_both_themes() {
    for size in [vec2(800.0, 600.0), vec2(1100.0, 700.0)] {
        for dark in [true, false] {
            let (mut s, _) = with_width();
            let d = CreateDialog::Cylinder {
                radius: 15.0,
                height: 30.0,
            };
            let mut pane = Pane::new(dark, size);
            d.set_text(&pane.ctx, &s, FieldKey::Radius, "Breit");
            let mut d = d;
            let mut draw = |ui: &mut egui::Ui| {
                d.show(ui.ctx(), &mut s);
            };
            let seen = pane.settle(&mut draw);
            let screen = Rect::from_min_size(Pos2::ZERO, size);
            let p = theme::palette(dark);
            for text in [
                "Zylinder erzeugen",
                "Radius: Den Namen „Breit“ gibt es nicht.",
                "Namen: Breite",
                "Erzeugen",
                "Abbrechen",
            ] {
                let t = find(&seen, text).unwrap_or_else(|| panic!("„{text}“ fehlt"));
                assert!(
                    screen.contains_rect(t.rect) && t.clip.contains_rect(t.rect),
                    "{size:?} {dark}: „{text}“ {:?}",
                    t.rect
                );
            }
            assert_eq!(
                find(&seen, "Radius: Den Namen „Breit“ gibt es nicht.")
                    .unwrap()
                    .color,
                p.error
            );
            // Die Höhe steht unter dem Radius, mit Meldung genau so weit wie ohne.
            let gap = |seen: &[Seen]| {
                find(seen, "Höhe").unwrap().rect.top() - find(seen, "Radius").unwrap().rect.top()
            };
            drop(draw);
            let (mut s2, _) = with_width();
            let mut plain = CreateDialog::Cylinder {
                radius: 15.0,
                height: 30.0,
            };
            let mut base = Pane::new(dark, size);
            let without = base.settle(&mut |ui| {
                plain.show(ui.ctx(), &mut s2);
            });
            assert_eq!(
                gap(&seen),
                gap(&without),
                "die Meldung verschiebt die Felder nicht"
            );
        }
    }
}

// ---- R-a: Bearbeiten-Fenster nach Parameteränderung geschlossen -------------------------------

#[test]
fn edit_window_closes_after_a_parameter_change() {
    let mut w = ws();
    let tab = w.active_tab_mut();
    param(&mut tab.state, "Breite", "40", ParamUnit::Length);
    let f = tab
        .state
        .create_box_with(
            DVec3::new(40.0, 20.0, 10.0),
            formulas(&[(FieldKey::Breite, "Breite")]),
        )
        .unwrap();
    tab.feature_dialog = FeatureDialog::new(&tab.state, f);
    assert_eq!(
        tab.feature_dialog.as_ref().unwrap().text("Breite"),
        Some("Breite")
    );
    let id = tab.state.document.parameters()[0].id;
    set_formula(&mut tab.state, id, "60");
    tab.after_parameter_change();
    assert!(tab.feature_dialog.is_none());
    assert_eq!(
        tab.state.info.as_deref(),
        Some(planora_app::parameter_dialog::DIALOG_CLOSED)
    );
    assert_eq!(feature_formulas(&tab.state, f).len(), 1, "Formel bleibt");
}

// ---- Nachbesserung (Review Q1–Q6): echtes Tippen, ein Zeichen je Bild ------------------------

impl Pane {
    /// Tippt `text` Zeichen für Zeichen; das Feld behält dabei die Tastatur (auch beim Wechsel
    /// Zahl ↔ Formel, wenn „fx“ erscheint oder verschwindet).
    fn type_chars(&mut self, text: &str, f: &mut dyn FnMut(&mut egui::Ui)) {
        let focused = self.ctx.memory(|m| m.focused());
        assert!(focused.is_some(), "kein Feld hat die Tastatur");
        for c in text.chars() {
            self.frame(vec![Event::Text(c.to_string())], f);
            assert_eq!(
                self.ctx.memory(|m| m.focused()),
                focused,
                "Tastatur verloren nach „{c}“"
            );
        }
        self.frame(vec![], f);
    }
}

impl Rig {
    /// Klickt in das Feld mit `old` im Fenster `title`, markiert alles und tippt `text` Zeichen
    /// für Zeichen; die Tastatur bleibt im Feld.
    fn type_chars(&mut self, title: &str, old: &str, text: &str) {
        let r = self
            .in_window(title, old)
            .unwrap_or_else(|| panic!("Feld „{old}“ fehlt"));
        self.click(r.center());
        self.frame(vec![key(Key::A, Modifiers::COMMAND)]);
        let focused = self.ctx.memory(|m| m.focused());
        assert!(focused.is_some());
        for c in text.chars() {
            self.frame(vec![Event::Text(c.to_string())]);
            assert_eq!(
                self.ctx.memory(|m| m.focused()),
                focused,
                "Tastatur verloren nach „{c}“"
            );
        }
        self.frame(vec![]);
    }
}

fn ws_with_width() -> Workspace {
    let mut w = ws();
    param(
        &mut w.active_tab_mut().state,
        "Breite",
        "40",
        ParamUnit::Length,
    );
    w.observe_history(false);
    w
}

fn quader() -> CreateDialog {
    CreateDialog::Box {
        x: 11.0,
        y: 12.0,
        z: 13.0,
    }
}

#[test]
fn real_typing_in_the_create_window() {
    let mut rig = Rig::new(ws_with_width());
    let title = "Quader erzeugen";
    rig.tab().dialog = Some(quader());
    // Zahl → Formel (beim „B“ erscheint „fx“) und Formel → Zahl im selben Feld.
    rig.type_chars(title, "11", "Breite*2");
    rig.type_chars(title, "13", "Breite");
    rig.type_chars(title, "Breite", "7");
    let seen = rig.settle();
    assert!(has(&seen, "= 80,00 mm"));
    let steps = rig.tab().history().undo_len();
    let e = rig.in_window(title, "Erzeugen").unwrap();
    rig.click(e.center());
    rig.settle();
    let tab = rig.tab();
    let f = tab.state.document.features()[0].id;
    assert_eq!(box_size(&tab.state, f), DVec3::new(80.0, 12.0, 7.0));
    assert_eq!(
        feature_formulas(&tab.state, f),
        formulas(&[(FieldKey::Breite, "Breite*2")])
    );
    assert_eq!(tab.history().undo_len(), steps + 1);
    assert_eq!(tab.history().undo_label(), Some("Quader anlegen"));
}

#[test]
fn a_failed_creation_keeps_the_window_and_the_text() {
    let mut rig = Rig::new(ws_with_width());
    let title = "Quader erzeugen";
    rig.tab().dialog = Some(quader());
    rig.type_chars(title, "11", "Breite-40");
    assert!(has(&rig.settle(), "= 0,00 mm"));
    let e = rig.in_window(title, "Erzeugen").unwrap();
    rig.click(e.center());
    rig.settle();
    assert!(rig.tab().dialog.is_some(), "Fenster bleibt offen");
    assert!(rig.in_window(title, "Breite-40").is_some(), "Text bleibt");
    assert!(rig.tab().state.document.features().is_empty());
    assert!(
        rig.tab()
            .state
            .message
            .as_deref()
            .unwrap()
            .contains("größer als null")
    );
}

#[test]
fn switching_the_create_window_forgets_the_other_texts() {
    let mut rig = Rig::new(ws_with_width());
    rig.tab().dialog = Some(quader());
    rig.type_chars("Quader erzeugen", "11", "Breite");
    rig.tab().dialog = Some(CreateDialog::Sphere { radius: 5.0 });
    assert!(rig.in_window("Kugel erzeugen", "5").is_some());
    rig.tab().dialog = Some(quader());
    assert!(rig.in_window("Quader erzeugen", "11").is_some(), "Vorgabe");
    assert!(rig.in_window("Quader erzeugen", "Breite").is_none());
}

#[test]
fn create_texts_belong_to_their_tab() {
    let (mut a, _) = with_width();
    let (mut b, _) = with_width();
    let mut pane = Pane::new(true, vec2(1100.0, 700.0));
    let d = quader();
    d.set_text(&pane.ctx, &a, FieldKey::Breite, "Breite");
    let mut shown = d.clone();
    let seen = pane.settle(&mut |ui| {
        shown.show(ui.ctx(), &mut b);
    });
    assert!(has(&seen, "11"), "Tab B zeigt die Vorgabe");
    assert!(!has(&seen, "Breite"));
    assert_eq!(
        d.texts(&pane.ctx, &a)[&FieldKey::Breite],
        "Breite",
        "Tab A behält"
    );
    let seen = pane.settle(&mut |ui| {
        shown.show(ui.ctx(), &mut a);
    });
    assert!(has(&seen, "= 40,00 mm"));
}

/// Quader 40 × 20 × 10 mit Formeln in Breite, Tiefe und Höhe.
fn box_with_three_formulas() -> (AppState, FeatureId) {
    let (mut s, _) = with_width();
    let f = s
        .create_box_with(
            DVec3::new(40.0, 20.0, 10.0),
            formulas(&[
                (FieldKey::Breite, "Breite"),
                (FieldKey::Tiefe, "Breite/2"),
                (FieldKey::Hoehe, "Breite/4"),
            ]),
        )
        .unwrap();
    (s, f)
}

#[test]
fn real_typing_in_the_edit_window() {
    let (mut s, f) = box_with_three_formulas();
    let mut d = FeatureDialog::new(&s, f).unwrap();
    let mut pane = Pane::new(false, vec2(1100.0, 700.0));
    let mut open = true;
    let edits = s.document.edit_count();
    let mut draw = |ui: &mut egui::Ui| {
        if open {
            open = d.show(ui.ctx(), &mut s);
        }
    };
    // Formel → Zahl (das „fx“ verschwindet beim ersten Zeichen).
    pane.focus_beside("Tiefe", &mut draw);
    pane.type_chars("25", &mut draw);
    // Zahl → Formel.
    pane.focus_beside("Position x", &mut draw);
    let before = pane.ctx.memory(|m| m.focused());
    pane.type_chars("Breite/8", &mut draw);
    assert_eq!(pane.ctx.memory(|m| m.focused()), before);
    let seen = pane.settle(&mut draw);
    assert!(has(&seen, "= 5,00 mm"));
    let apply = find(&seen, "Übernehmen").unwrap().rect.center();
    pane.click(apply, &mut draw);
    drop(draw);
    assert!(!open, "nach Erfolg geschlossen");
    assert_eq!(s.document.edit_count(), edits + 1);
    assert_eq!(box_size(&s, f), DVec3::new(40.0, 25.0, 10.0));
    assert_eq!(
        feature_formulas(&s, f),
        formulas(&[
            (FieldKey::Breite, "Breite"),
            (FieldKey::Hoehe, "Breite/4"),
            (FieldKey::X, "Breite/8")
        ])
    );
    match &s.document.feature(f).unwrap().kind {
        FeatureKind::Box { origin, .. } => assert_eq!(origin.x, 5.0),
        k => panic!("{k:?}"),
    }
}

#[test]
fn edit_values_are_shown_in_full() {
    let (mut s, _) = with_width();
    s.create_box(DVec3::splat(10.0)).unwrap();
    s.move_body(0, DVec3::new(1.0, 0.0, 0.0)).unwrap();
    let mv = s.document.features().last().unwrap().id;
    let mut d = FeatureDialog::new(&s, mv).unwrap();
    d.set_text("Δx", "Breite/2");
    for size in [vec2(800.0, 600.0), vec2(1400.0, 900.0)] {
        let mut pane = Pane::new(true, size);
        let seen = pane.settle(&mut |ui| {
            d.show(ui.ctx(), &mut s);
        });
        let v = find(&seen, "= 20,00 mm").expect("Wert");
        assert!(v.rect.width() > 50.0, "{size:?}: Wert gekürzt {:?}", v.rect);
        assert!(v.clip.contains_rect(v.rect));
    }
}

/// Quader mit der gespeicherten, kaputten Breite-Formel „Gibtsnicht“ (wie aus einer Datei).
fn box_with_broken_formula() -> (AppState, FeatureId) {
    let (mut s, _) = with_width();
    let f = s
        .create_box_with(
            DVec3::new(40.0, 20.0, 10.0),
            formulas(&[(FieldKey::Breite, "Breite")]),
        )
        .unwrap();
    let mut snap = s.document.snapshot();
    snap.features[0]
        .formulas
        .insert(FieldKey::Breite, "Gibtsnicht".into());
    s.document.restore(snap).unwrap();
    s.refresh();
    assert!(matches!(
        s.document.feature(f).unwrap().status,
        planora_feature::FeatureStatus::Error(_)
    ));
    (s, f)
}

#[test]
fn a_stored_broken_formula_does_not_lock_other_fields() {
    let (mut s, f) = box_with_broken_formula();
    let mut d = FeatureDialog::new(&s, f).unwrap();
    let mut pane = Pane::new(true, vec2(1100.0, 700.0));
    let mut open = true;
    let mut draw = |ui: &mut egui::Ui| {
        if open {
            open = d.show(ui.ctx(), &mut s);
        }
    };
    let msg = "Breite: Den Namen „Gibtsnicht“ gibt es nicht.";
    let seen = pane.settle(&mut draw);
    assert!(has(&seen, msg), "Meldung bleibt sichtbar");
    pane.focus_beside("Tiefe", &mut draw);
    pane.type_chars("25", &mut draw);
    let seen = pane.settle(&mut draw);
    let apply = find(&seen, "Übernehmen").unwrap().rect.center();
    pane.click(apply, &mut draw);
    drop(draw);
    assert!(!open, "übernommen");
    assert_eq!(box_size(&s, f).y, 25.0);
    assert_eq!(
        feature_formulas(&s, f),
        formulas(&[(FieldKey::Breite, "Gibtsnicht")]),
        "die kaputte Formel bleibt unverändert"
    );

    // Das kaputte Feld selbst geändert: normal geprüft, „Übernehmen“ gesperrt.
    let (mut s, f) = box_with_broken_formula();
    let mut d = FeatureDialog::new(&s, f).unwrap();
    let edits = s.document.edit_count();
    let mut open = true;
    let mut draw = |ui: &mut egui::Ui| {
        if open {
            open = d.show(ui.ctx(), &mut s);
        }
    };
    let mut pane = Pane::new(true, vec2(1100.0, 700.0));
    pane.focus_beside("Breite", &mut draw);
    pane.type_chars("Gibtsnich", &mut draw);
    let seen = pane.settle(&mut draw);
    assert!(has(&seen, "Breite: Den Namen „Gibtsnich“ gibt es nicht."));
    let apply = find(&seen, "Übernehmen").unwrap().rect.center();
    pane.click(apply, &mut draw);
    drop(draw);
    assert!(open, "gesperrt");
    assert_eq!(d.error(), None, "kein Versuch");
    assert_eq!(s.document.edit_count(), edits);
    d.set_text("Breite", "Gibtsnich2");
    assert_eq!(
        d.apply(&mut s),
        Err("Breite: Den Namen „Gibtsnich2“ gibt es nicht.".to_string())
    );
}

#[test]
fn art_change_keeps_formula_texts_of_fields_that_stay() {
    let (mut s, id) = with_fillet();
    let mut d = FeatureDialog::new(&s, id).unwrap();
    d.set_blend_art(ArtChoice::FaseGleich);
    d.set_text("Abstand", "Breite/40");
    d.apply(&mut s).unwrap();
    let mut d = FeatureDialog::new(&s, id).unwrap();
    d.set_blend_art(ArtChoice::FaseWinkel);
    assert_eq!(d.text("Abstand"), Some("Breite/40"), "Formel bleibt");
    assert_eq!(d.text("Winkel"), Some("45"));
    // Ungespeichert getippte Formel bleibt ebenso.
    d.set_text("Abstand", "Breite/20");
    d.set_blend_art(ArtChoice::FaseZwei);
    d.set_blend_art(ArtChoice::FaseWinkel);
    assert_eq!(
        d.text("Abstand"),
        Some("1"),
        "Abstand 1 ist ein anderes Feld"
    );
    d.set_text("Abstand", "Breite/20");
    d.add_point();
    assert_eq!(d.text("Abstand"), Some("Breite/20"));
}

#[test]
fn the_blend_window_locks_create_on_a_formula_error() {
    let (mut s, _) = with_width();
    param(&mut s, "w", "30", ParamUnit::Angle);
    s.create_box(DVec3::splat(20.0)).unwrap();
    let e = edge_between(&s.bodies[0].solid, "Deckel", "Wand 1");
    s.set_selection(Some(Pick::Edge(EdgeRef { body: 0, edge: e })));
    let mut d = BlendDialog::fillet();
    let mut pane = Pane::new(false, vec2(1100.0, 700.0));
    let mut draw = |ui: &mut egui::Ui| {
        d.show(ui.ctx(), &mut s);
    };
    pane.focus_beside("Radius", &mut draw);
    pane.type_chars("w", &mut draw);
    let seen = pane.settle(&mut draw);
    let m = find(
        &seen,
        "Radius: Hier wird eine Länge erwartet, die Formel ergibt einen Winkel.",
    )
    .expect("Meldung");
    assert_eq!(m.color, theme::palette(false).error);
    let run = find(&seen, "Erzeugen").unwrap().rect.center();
    pane.click(run, &mut draw);
    drop(draw);
    assert_eq!(d.error(), None, "kein Versuch");
    assert_eq!(s.document.features().len(), 1);
}

#[test]
fn mesh_drafts_belong_to_their_body() {
    let (mut s, _) = with_width();
    s.import_mesh("A", Arc::new(cube()), 1.0).unwrap();
    s.import_mesh("B", Arc::new(cube()), 2.0).unwrap();
    s.selected_body = Some(0);
    let mut pane = Pane::new(true, vec2(1100.0, 700.0));
    let mut draw = |ui: &mut egui::Ui| panel(ui, &mut s);
    pane.focus_beside("Maßstab", &mut draw);
    pane.type_chars("0", &mut draw);
    let seen = pane.enter(&mut draw);
    let red = |seen: &[Seen]| {
        seen.iter()
            .any(|x| x.text.contains("Maßstab") && x.color == theme::palette(true).error)
    };
    assert!(red(&seen), "abgelehnt, Meldung im Feld");
    drop(draw);
    // Anderer Netz-Körper: sein eigener Maßstab, nicht der Entwurf des ersten.
    s.selected_body = Some(1);
    let mut draw = |ui: &mut egui::Ui| panel(ui, &mut s);
    let seen = pane.settle(&mut draw);
    assert!(!red(&seen), "Entwurf des anderen Körpers");
    assert!(has(&seen, "2"), "Maßstab 2 des zweiten Körpers");
    drop(draw);
    assert_eq!(s.mesh_transform(1).unwrap().scale, 2.0);
}

#[test]
fn real_typing_in_the_mesh_fields() {
    let (mut s, _) = with_width();
    s.import_mesh("N", Arc::new(cube()), 1.0).unwrap();
    let fid = s.document.features()[1 - 1].id;
    let mut pane = Pane::new(true, vec2(1100.0, 700.0));
    let mut draw = |ui: &mut egui::Ui| panel(ui, &mut s);
    pane.focus_beside("Position y", &mut draw);
    pane.type_chars("Breite*2", &mut draw);
    pane.enter(&mut draw);
    drop(draw);
    assert_eq!(s.mesh_transform(0).unwrap().offset.y, 80.0);
    assert_eq!(
        feature_formulas(&s, fid),
        formulas(&[(FieldKey::Y, "Breite*2")])
    );
}
