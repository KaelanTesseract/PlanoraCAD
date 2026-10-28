//! Formeln in Skizzenmaßen (Aufgabe 11): Das Maßfeld der offenen Skizze nimmt Zahlen oder
//! Formeln mit Parameternamen, die Liste der Abhängigkeiten markiert Formelmaße mit „fx“, der
//! Zeichenbereich zeigt weiter den Wert. Formeln liegen im Entwurf der Sitzung, sind Schritte
//! im Skizzenverlauf („Formel ändern“) und gehen mit „Skizze beenden“ als ein Schritt ins
//! Dokument. Mit der ganzen App (`eframe::App::ui`), echten Tasten (ein Zeichen je Bild) und
//! Mausklicks.

mod common;

use eframe::App as _;
use egui::epaint::{ClippedShape, Shape};
use egui::{Color32, Event, Key, Modifiers, PointerButton, Pos2, Rect, Vec2, vec2};
use planora_app::app::PlanoraApp;
use planora_app::formula_field::fx_label;
use planora_app::sketch_panel::{self, LIST_FX_LABEL};
use planora_app::sketch_state::{ConstraintAction, DRIVEN_NO_FORMULA, Item};
use planora_app::state::{AppState, StandardPlane, sketch_open_lock};
use planora_app::workspace::Workspace;
use planora_app::{fonts, shell_panels, theme};
use planora_feature::{FeatureId, FeatureKind, ParamEdit, ParamId, ParamUnit, SketchPlane};
use planora_project::config::Config;
use planora_project::library::Library;
use planora_sketch::constraint::{ConstraintId, ConstraintKind as K};
use planora_sketch::model::{CurveId, PointId};

const SIZE: Vec2 = vec2(1400.0, 900.0);
const MEDIUM: Vec2 = vec2(1100.0, 700.0);
const SMALL: Vec2 = vec2(800.0, 600.0);

// ---- Hilfen ---------------------------------------------------------------------------------

fn context(dark: bool) -> egui::Context {
    let c = egui::Context::default();
    c.set_fonts(fonts::definitions());
    theme::apply(&c, dark);
    c
}

thread_local! {
    static TIME: std::cell::Cell<f64> = const { std::cell::Cell::new(0.0) };
}

fn input(size: Vec2, events: Vec<Event>) -> egui::RawInput {
    let time = TIME.with(|t| {
        t.set(t.get() + 1.0 / 60.0);
        t.get()
    });
    egui::RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
        time: Some(time),
        events,
        ..Default::default()
    }
}

#[derive(Debug, Clone)]
struct Seen {
    text: String,
    rect: Rect,
    clip: Rect,
    color: Color32,
}

#[derive(Default)]
struct Shot {
    texts: Vec<Seen>,
}

fn walk(shape: &Shape, clip: Rect, s: &mut Shot) {
    match shape {
        Shape::Vec(v) => v.iter().for_each(|x| walk(x, clip, s)),
        Shape::Text(t) => {
            let section = t.galley.job.sections.first();
            let mut color = section.map_or(t.fallback_color, |x| x.format.color);
            if color == Color32::PLACEHOLDER {
                color = t.fallback_color;
            }
            if let Some(c) = t.override_text_color {
                color = c;
            }
            s.texts.push(Seen {
                text: t.galley.text().to_string(),
                rect: Rect::from_min_size(t.pos, t.galley.size()),
                clip,
                color,
            });
        }
        _ => {}
    }
}

impl Shot {
    fn of(shapes: &[ClippedShape]) -> Shot {
        let mut s = Shot::default();
        for c in shapes {
            walk(&c.shape, c.clip_rect, &mut s);
        }
        s
    }

    /// Text `exact` im Rechteck `r` (Mitte darin).
    fn find_in(&self, r: Rect, exact: &str) -> &Seen {
        self.texts
            .iter()
            .find(|t| t.text == exact && r.expand(0.5).contains(t.rect.center()))
            .unwrap_or_else(|| {
                panic!(
                    "„{exact}“ fehlt in {r:?}: {:?}",
                    self.texts
                        .iter()
                        .filter(|t| r.contains(t.rect.center()))
                        .map(|t| &t.text)
                        .collect::<Vec<_>>()
                )
            })
    }

    fn has_in(&self, r: Rect, exact: &str) -> bool {
        self.texts
            .iter()
            .any(|t| t.text == exact && r.expand(0.5).contains(t.rect.center()))
    }
}

fn library_ws() -> Workspace {
    let root = common::temp_root("planora-formula-sketch");
    Workspace::new(Some(Library::open(&root).unwrap()), Config::default())
}

fn state(app: &mut PlanoraApp) -> &mut AppState {
    &mut app.ws_mut().active_tab_mut().state
}

fn frame(c: &egui::Context, app: &mut PlanoraApp, size: Vec2, events: Vec<Event>) -> Shot {
    let mut f = eframe::Frame::_new_kittest();
    let out = c.run_ui(input(size, events), |ui| app.ui(ui, &mut f));
    let shot = Shot::of(&out.shapes);
    out.drop_without_applying_deltas();
    shot
}

fn settle(c: &egui::Context, app: &mut PlanoraApp, size: Vec2) -> Shot {
    for _ in 0..30 {
        frame(c, app, size, vec![]);
    }
    frame(c, app, size, vec![])
}

fn button(p: Pos2, pressed: bool) -> Event {
    Event::PointerButton {
        pos: p,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
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

fn click(c: &egui::Context, app: &mut PlanoraApp, size: Vec2, p: Pos2) -> Shot {
    frame(c, app, size, vec![Event::PointerMoved(p)]);
    frame(c, app, size, vec![Event::PointerMoved(p), button(p, true)]);
    frame(c, app, size, vec![button(p, false)]);
    frame(c, app, size, vec![])
}

fn double_click(c: &egui::Context, app: &mut PlanoraApp, size: Vec2, p: Pos2) {
    frame(c, app, size, vec![Event::PointerMoved(p)]);
    frame(c, app, size, vec![Event::PointerMoved(p), button(p, true)]);
    frame(c, app, size, vec![button(p, false)]);
    frame(c, app, size, vec![button(p, true)]);
    frame(c, app, size, vec![button(p, false)]);
    frame(c, app, size, vec![]);
}

fn side(c: &egui::Context) -> Rect {
    shell_panels::field_rects(c)
        .into_iter()
        .find(|(n, _)| *n == "Skizze")
        .expect("Skizzenfeld")
        .1
}

/// Ersetzt den Text des Feldes mit der Tastatur: alles markieren, dann ein Zeichen je Bild.
/// Nach jedem Zeichen hat dasselbe Feld die Tastatur (feste Kennung, auch wenn „fx“
/// erscheint oder verschwindet).
fn type_text(c: &egui::Context, app: &mut PlanoraApp, size: Vec2, text: &str) {
    let focused = c.memory(|m| m.focused());
    assert!(focused.is_some(), "kein Feld hat die Tastatur");
    frame(c, app, size, vec![key(Key::A, Modifiers::COMMAND)]);
    for ch in text.chars() {
        frame(c, app, size, vec![Event::Text(ch.to_string())]);
        assert_eq!(
            c.memory(|m| m.focused()),
            focused,
            "nach „{ch}“ hat das Maßfeld die Tastatur verloren"
        );
    }
}

fn enter(c: &egui::Context, app: &mut PlanoraApp, size: Vec2) {
    frame(c, app, size, vec![key(Key::Enter, Modifiers::NONE)]);
    frame(c, app, size, vec![]);
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

fn set_param(s: &mut AppState, id: ParamId, formula: &str) {
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

/// Wert des Maßes `id` im Entwurf (Winkel in Bogenmaß).
fn draft_value(s: &AppState, id: ConstraintId) -> f64 {
    match &s.session_sketch().unwrap().constraint(id).unwrap().kind {
        K::Distance(_, _, v) | K::Angle(_, _, v) | K::DistancePointLine(_, _, v) => *v,
        k => panic!("{k:?}"),
    }
}

fn draft_formula(s: &AppState, id: ConstraintId) -> Option<String> {
    s.sketch_draft.as_ref().unwrap().formulas.get(&id).cloned()
}

struct Rig {
    app: PlanoraApp,
    sketch: FeatureId,
    breite: ParamId,
    line: CurveId,
    a: PointId,
    b: PointId,
    /// Abstandsmaß 30 der Linie (bestimmend).
    dim: ConstraintId,
}

/// App mit Breite = 40 (Länge), w = 30 (Winkel) und offener Skizze auf XY mit der Linie
/// (0,0)–(30,0), festem Anfang, waagerecht und Abstandsmaß 30 (voll bestimmt).
fn rig() -> Rig {
    let mut app = PlanoraApp::for_tests(library_ws());
    let s = state(&mut app);
    let breite = param(s, "Breite", "40", ParamUnit::Length);
    param(s, "w", "30", ParamUnit::Angle);
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
    Rig {
        app,
        sketch,
        breite,
        line,
        a,
        b,
        dim,
    }
}

fn line_length(s: &AppState, line: CurveId) -> f64 {
    s.session_sketch().unwrap().line_length(line).unwrap()
}

// ---- (b) bestehendes Maß --------------------------------------------------------------------

#[test]
fn sketch_existing_dimension_formula() {
    let c = context(true);
    let mut r = rig();
    let id = r.dim;
    settle(&c, &mut r.app, SIZE);
    // Doppelklick auf das Maß in der Liste öffnet das Maßfeld mit dem Wert.
    let shot = settle(&c, &mut r.app, SIZE);
    let row = shot.find_in(side(&c), "Abstand 30,00 mm").rect.center();
    double_click(&c, &mut r.app, SIZE, row);
    assert!(sketch_panel::dimension_editor_open(&c), "Maßfeld offen");
    let shot = frame(&c, &mut r.app, SIZE, vec![]);
    assert!(shot.has_in(side(&c), "30,00"), "Vorgabe: der Wert");
    // „Breite“ mit echten Tasten, dann Eingabe.
    type_text(&c, &mut r.app, SIZE, "Breite");
    let shot = frame(&c, &mut r.app, SIZE, vec![]);
    assert!(shot.has_in(side(&c), "= 40,00 mm"), "Wert der Formel");
    assert!(shot.has_in(side(&c), "fx"), "fx vor dem Feld");
    enter(&c, &mut r.app, SIZE);
    assert!(!sketch_panel::dimension_editor_open(&c), "nach Eingabe zu");
    let s = state(&mut r.app);
    assert_eq!(draft_value(s, id), 40.0);
    assert_eq!(draft_formula(s, id).as_deref(), Some("Breite"));
    assert!((line_length(s, r.line) - 40.0).abs() < 1e-6);
    // Liste: „fx“ und die Formel unter dem Maß, der Name zeigt den Wert.
    let shot = settle(&c, &mut r.app, SIZE);
    let side_r = side(&c);
    let name = shot.find_in(side_r, "Abstand 40,00 mm").rect;
    let fx = shot.find_in(side_r, "fx").rect;
    let formula = shot.find_in(side_r, "Breite").rect;
    assert!(fx.top() >= name.bottom() - 1.0, "fx unter dem Maß");
    assert!(formula.left() > fx.right(), "Formel hinter fx");
    // Ein Schritt im Skizzenverlauf „Formel ändern“.
    assert_eq!(
        state(&mut r.app).sketch_undo_label(),
        Some("Formel ändern".to_string())
    );

    // Wieder öffnen: das Feld zeigt die Formel; „25“ löscht nur die Formel (P2).
    sketch_panel::open_existing_dimension_editor(&c, state(&mut r.app), id);
    let shot = settle(&c, &mut r.app, SIZE);
    assert!(shot.has_in(side(&c), "Breite"));
    assert!(shot.has_in(side(&c), "= 40,00 mm"));
    type_text(&c, &mut r.app, SIZE, "25");
    let shot = frame(&c, &mut r.app, SIZE, vec![]);
    assert!(
        !shot.has_in(side(&c), "= 40,00 mm"),
        "Zahl: kein Formelwert"
    );
    enter(&c, &mut r.app, SIZE);
    let s = state(&mut r.app);
    assert_eq!(draft_value(s, id), 25.0);
    assert_eq!(draft_formula(s, id), None);
    assert!((line_length(s, r.line) - 25.0).abs() < 1e-6);
    settle(&c, &mut r.app, SIZE);

    // Rückgängig in der Skizze holt Wert und Formel zurück, Wiederholen wieder die Zahl.
    let s = state(&mut r.app);
    assert_eq!(s.sketch_undo_label(), Some("Formel ändern".to_string()));
    assert!(s.undo_sketch());
    assert_eq!(draft_value(s, id), 40.0);
    assert_eq!(draft_formula(s, id).as_deref(), Some("Breite"));
    assert!(s.undo_sketch());
    assert_eq!(draft_value(s, id), 30.0);
    assert_eq!(draft_formula(s, id), None);
    assert!(s.redo_sketch());
    assert_eq!(draft_formula(s, id).as_deref(), Some("Breite"));
    assert!(s.redo_sketch());
    assert_eq!(draft_value(s, id), 25.0);
    assert_eq!(draft_formula(s, id), None);
    let _ = (r.sketch, r.breite, r.a, r.b);
}

#[test]
fn fx_in_the_list_opens_the_field_and_fx_in_the_field_takes_the_value() {
    let c = context(true);
    let mut r = rig();
    let id = r.dim;
    {
        let s = state(&mut r.app);
        let scope = s.document.param_scope();
        let (se, sf) = s.session_draft_mut().unwrap();
        se.set_dimension_input(sf, id, "Breite/2", &scope).unwrap();
    }
    let shot = settle(&c, &mut r.app, SIZE);
    assert!(!sketch_panel::dimension_editor_open(&c));
    // „fx“ in der Liste öffnet dasselbe Formelfeld mit der Formel.
    let fx = shot.find_in(side(&c), "fx").rect.center();
    click(&c, &mut r.app, SIZE, fx);
    assert!(
        sketch_panel::dimension_editor_open(&c),
        "fx öffnet das Maßfeld"
    );
    let shot = settle(&c, &mut r.app, SIZE);
    assert!(shot.has_in(side(&c), "Breite/2"));
    assert!(shot.has_in(side(&c), "= 20,00 mm"));
    assert!(c.egui_wants_keyboard_input(), "Maßfeld hat die Tastatur");
    // „fx“ im Feld (das obere, vor dem Feld) setzt den Wert ein; OK übernimmt die Zahl.
    let field_fx = shot
        .texts
        .iter()
        .filter(|t| t.text == "fx" && side(&c).contains(t.rect.center()))
        .min_by(|a, b| a.rect.top().total_cmp(&b.rect.top()))
        .unwrap()
        .rect
        .center();
    click(&c, &mut r.app, SIZE, field_fx);
    let shot = settle(&c, &mut r.app, SIZE);
    assert!(shot.has_in(side(&c), "20"), "der Wert steht im Feld");
    assert!(!shot.has_in(side(&c), "= 20,00 mm"));
    let ok = shot.find_in(side(&c), "OK").rect.center();
    click(&c, &mut r.app, SIZE, ok);
    let s = state(&mut r.app);
    assert_eq!(draft_formula(s, id), None, "Zahl statt Formel");
    assert_eq!(draft_value(s, id), 20.0);
}

// ---- (c) neues Maß --------------------------------------------------------------------------

#[test]
fn sketch_new_dimension_formula() {
    let c = context(true);
    let mut r = rig();
    // Neues Maß über die Oberfläche: Abstand Punkt–Punkt eines freien Punktes zum Anfang.
    let p = {
        let (se, sk) = state(&mut r.app).session_sketch_mut().unwrap();
        let p = sk.add_point(0.0, 10.0).unwrap();
        se.selection = vec![Item::Point(r.a), Item::Point(p)];
        p
    };
    state(&mut r.app).observe_sketch(false);
    sketch_panel::open_dimension_editor(&c, state(&mut r.app), ConstraintAction::Distance);
    settle(&c, &mut r.app, SIZE);
    type_text(&c, &mut r.app, SIZE, "Breite/2");
    enter(&c, &mut r.app, SIZE);
    assert!(!sketch_panel::dimension_editor_open(&c));
    let s = state(&mut r.app);
    let (id, v) = s
        .session_sketch()
        .unwrap()
        .constraints()
        .find_map(|(id, c)| match c.kind {
            K::Distance(x, y, v) if (x, y) == (r.a, p) || (x, y) == (p, r.a) => Some((id, v)),
            _ => None,
        })
        .expect("neues Maß");
    assert_eq!(v, 20.0);
    assert_eq!(draft_formula(s, id).as_deref(), Some("Breite/2"));
    settle(&c, &mut r.app, SIZE);
    assert_eq!(
        state(&mut r.app).sketch_undo_label(),
        Some("Maß setzen".to_string())
    );

    // Winkelmaß „w“ zwischen zwei Linien: gespeichert in Bogenmaß, bitgenau wie das Dokument.
    let s = state(&mut r.app);
    let scope = s.document.param_scope();
    let (se, sf) = s.session_draft_mut().unwrap();
    let q = sf.sketch.add_point(0.0, 20.0).unwrap();
    let other = sf.sketch.add_line(r.a, q).unwrap();
    se.selection = vec![Item::Curve(r.line), Item::Curve(other)];
    let angle = se
        .apply_dimension_input(sf, ConstraintAction::Angle, "w", &scope)
        .unwrap();
    match sf.sketch.constraint(angle).unwrap().kind {
        K::Angle(_, _, v) => assert_eq!(v.to_bits(), 30f64.to_radians().to_bits()),
        ref k => panic!("{k:?}"),
    }
    assert_eq!(sf.formulas.get(&angle).map(String::as_str), Some("w"));
    // Eine Länge im Winkelmaß: abgelehnt mit der Einheitenmeldung, nichts ändert sich.
    let before = sf.clone();
    se.selection = vec![Item::Curve(r.line), Item::Curve(other)];
    assert_eq!(
        se.set_dimension_input(sf, angle, "Breite", &scope),
        Err("Hier wird ein Winkel erwartet, die Formel ergibt eine Länge.".into())
    );
    assert_eq!(*sf, before);
    assert_eq!(
        se.message.as_deref(),
        Some("Hier wird ein Winkel erwartet, die Formel ergibt eine Länge.")
    );
}

#[test]
fn angle_display_is_rounded_or_the_formula() {
    let c = context(true);
    let mut r = rig();
    let angle = {
        let s = state(&mut r.app);
        let scope = s.document.param_scope();
        let (se, sf) = s.session_draft_mut().unwrap();
        let q = sf.sketch.add_point(0.0, 20.0).unwrap();
        let other = sf.sketch.add_line(r.a, q).unwrap();
        se.selection = vec![Item::Curve(r.line), Item::Curve(other)];
        let id = se
            .apply_dimension_input(sf, ConstraintAction::Angle, "w*2", &scope)
            .unwrap();
        se.selection.clear();
        id
    };
    let shot = settle(&c, &mut r.app, SIZE);
    // Liste: gerundet in Grad, keine Nachkommastellen-Reste aus dem Bogenmaß.
    assert!(shot.has_in(side(&c), "Winkel 60,0°"));
    assert!(shot.has_in(side(&c), "w*2"));
    // Maßfeld: die Formel und ihr Wert in Grad.
    sketch_panel::open_existing_dimension_editor(&c, state(&mut r.app), angle);
    let shot = settle(&c, &mut r.app, SIZE);
    assert!(shot.has_in(side(&c), "= 60,00°"));
    // Als Zahl: „45,5“ steht danach gerundet „45,5“ im Feld, nicht 45,49999….
    type_text(&c, &mut r.app, SIZE, "45,5");
    enter(&c, &mut r.app, SIZE);
    sketch_panel::open_existing_dimension_editor(&c, state(&mut r.app), angle);
    let shot = settle(&c, &mut r.app, SIZE);
    assert!(shot.has_in(side(&c), "45,5"), "Winkel als Zahl, gerundet");
    assert!(shot.has_in(side(&c), "°"));
}

// ---- (d) nicht lösbar -----------------------------------------------------------------------

/// Dreieck mit den Seiten 30, 30, 40 (Ecke fest, erste Seite waagerecht) in der offenen
/// Skizze; liefert die drei Abstandsmaße und die drei Ecken.
fn triangle(s: &mut AppState) -> ([ConstraintId; 3], [PointId; 3]) {
    let (se, sf) = s.session_draft_mut().unwrap();
    let sk = &mut sf.sketch;
    let p0 = sk.add_point(100.0, 0.0).unwrap();
    let p1 = sk.add_point(130.0, 0.0).unwrap();
    let p2 = sk.add_point(120.0, 20.0).unwrap();
    let l0 = sk.add_line(p0, p1).unwrap();
    sk.add_line(p1, p2).unwrap();
    sk.add_line(p2, p0).unwrap();
    sk.add_constraint(K::Fixed(p0, 100.0, 0.0)).unwrap();
    sk.add_constraint(K::Horizontal(
        planora_sketch::constraint::LineOrPoints::Line(l0),
    ))
    .unwrap();
    let da = sk.add_constraint(K::Distance(p0, p1, 30.0)).unwrap();
    let db = sk.add_constraint(K::Distance(p1, p2, 30.0)).unwrap();
    let dc = sk.add_constraint(K::Distance(p2, p0, 40.0)).unwrap();
    se.last_report = Some(sk.solve());
    ([da, db, dc], [p0, p1, p2])
}

#[test]
fn sketch_unsolvable_is_rejected_in_session() {
    let c = context(true);
    let mut r = rig();
    let ([_, _, dc], _) = triangle(state(&mut r.app));
    state(&mut r.app).observe_sketch(false);
    let before = state(&mut r.app).sketch_draft.clone().unwrap();
    let rev = before.sketch.revision();
    // Seite c = 2·Breite = 80 > 30 + 30: widersprüchlich, abgelehnt.
    sketch_panel::open_existing_dimension_editor(&c, state(&mut r.app), dc);
    settle(&c, &mut r.app, SIZE);
    type_text(&c, &mut r.app, SIZE, "Breite*2");
    enter(&c, &mut r.app, SIZE);
    let s = state(&mut r.app);
    let msg = s.sketch_session.as_ref().unwrap().message.clone().unwrap();
    assert!(
        msg == "Widerspruch: Die Änderung widerspricht vorhandenen Abhängigkeiten"
            || msg == "Die Skizze ließ sich nach der Änderung nicht lösen",
        "{msg}"
    );
    let after = s.sketch_draft.clone().unwrap();
    assert_eq!(after.sketch.revision(), rev, "Skizze unverändert");
    assert_eq!(after, before, "Entwurf und Formeltabelle unverändert");
    assert!(sketch_panel::dimension_editor_open(&c), "Feld bleibt offen");
    // Die Meldung steht rot im Skizzenfeld.
    let shot = settle(&c, &mut r.app, SIZE);
    let p = theme::palette(true);
    assert_eq!(shot.find_in(side(&c), &msg).color, p.error);
    // Kein Schritt im Verlauf.
    assert_ne!(
        state(&mut r.app).sketch_undo_label(),
        Some("Formel ändern".to_string())
    );
}

#[test]
fn unsolved_formula_sketch_shows_its_error_in_the_editor() {
    // Dokument: Dreieck mit Formel an Seite c, dann c zu groß → Fehlerstatus (P12). Öffnet man
    // die Skizze, steht der Fehler rot im Skizzenfeld; die Geometrie bleibt die letzte gültige.
    let c = context(false);
    let mut r = rig();
    let s = state(&mut r.app);
    s.dark = false;
    s.close_sketch();
    let cp = param(s, "c", "40", ParamUnit::Length);
    s.open_sketch(r.sketch).unwrap();
    let ([_, _, dc], [p0, _, p2]) = triangle(s);
    let scope = s.document.param_scope();
    let (se, sf) = s.session_draft_mut().unwrap();
    se.set_dimension_input(sf, dc, "c", &scope).unwrap();
    assert_eq!(se.message, None);
    s.close_sketch();
    assert_eq!(
        s.document.feature(r.sketch).unwrap().status,
        planora_feature::FeatureStatus::Ok
    );
    set_param(s, cp, "100");
    assert_eq!(
        s.document.feature(r.sketch).unwrap().status,
        planora_feature::FeatureStatus::Error(
            "Fehler in der Skizze: Mit den Werten der Formeln widersprechen sich Abhängigkeiten"
                .into()
        )
    );
    s.open_sketch(r.sketch).unwrap();
    let shot = settle(&c, &mut r.app, SIZE);
    let t = shot.find_in(
        side(&c),
        "Mit den Werten der Formeln widersprechen sich Abhängigkeiten",
    );
    assert_eq!(t.color, theme::palette(false).error);
    // Die Skizze zeigt die letzte gültige Lage (Seite c 40, nicht eine verzogene Form).
    let sk = state(&mut r.app).session_sketch().unwrap();
    let d = sk.point_xy(p0).unwrap().distance(sk.point_xy(p2).unwrap());
    assert!((d - 40.0).abs() < 1e-6, "Seite c {d}");
    // Eine erlaubte Änderung (Zahl an einer anderen Stelle) meldet weiter den Fehler der Formeln
    // statt nur „weiterhin Widersprüche“.
    let s = state(&mut r.app);
    let (se, sf) = s.session_draft_mut().unwrap();
    se.set_dimension_input(sf, r.dim, "35", &scope).unwrap();
    assert_eq!(
        se.message.as_deref(),
        Some("Mit den Werten der Formeln widersprechen sich Abhängigkeiten")
    );
}

// ---- (e) anzeigend --------------------------------------------------------------------------

#[test]
fn driven_drops_formula() {
    let c = context(true);
    let mut r = rig();
    let id = r.dim;
    {
        let s = state(&mut r.app);
        let scope = s.document.param_scope();
        let (se, sf) = s.session_draft_mut().unwrap();
        se.set_dimension_input(sf, id, "Breite", &scope).unwrap();
        // Über die Sitzung auf anzeigend: die Formel entfällt spätestens mit observe_sketch.
        se.set_driven(&mut sf.sketch, id, true).unwrap();
    }
    let s = state(&mut r.app);
    s.observe_sketch(false);
    assert_eq!(draft_formula(s, id), None);
    // Ein anzeigendes Maß nimmt keine Formel, eine Zahl schon.
    let scope = s.document.param_scope();
    let (se, sf) = s.session_draft_mut().unwrap();
    let before = sf.clone();
    assert_eq!(
        se.set_dimension_input(sf, id, "Breite", &scope),
        Err(DRIVEN_NO_FORMULA.to_string())
    );
    assert_eq!(*sf, before);
    se.set_driven(&mut sf.sketch, id, false).unwrap();

    // Über die Liste: Formel setzen, Haken „anzeigend“ → Formel sofort weg, kein „fx“.
    {
        let (se, sf) = s.session_draft_mut().unwrap();
        se.set_dimension_input(sf, id, "Breite", &scope).unwrap();
    }
    let shot = settle(&c, &mut r.app, SIZE);
    assert!(shot.has_in(side(&c), "fx"));
    let check = shot.find_in(side(&c), "anzeigend").rect.center();
    click(&c, &mut r.app, SIZE, check);
    let s = state(&mut r.app);
    assert!(s.session_sketch().unwrap().constraint(id).unwrap().driven);
    assert_eq!(draft_formula(s, id), None);
    let shot = settle(&c, &mut r.app, SIZE);
    assert!(!shot.has_in(side(&c), "fx"), "kein fx bei anzeigend");
    // Doppelklick auf das anzeigende Maß öffnet ein reines Zahlenfeld (ohne Formelfeld).
    sketch_panel::open_existing_dimension_editor(&c, state(&mut r.app), id);
    settle(&c, &mut r.app, SIZE);
    type_text(&c, &mut r.app, SIZE, "Breite");
    let shot = frame(&c, &mut r.app, SIZE, vec![]);
    assert!(!shot.has_in(side(&c), "fx"), "kein fx im Zahlenfeld");
    assert!(!shot.has_in(side(&c), "= 40,00 mm"));
    enter(&c, &mut r.app, SIZE);
    assert_eq!(
        state(&mut r.app)
            .sketch_session
            .as_ref()
            .unwrap()
            .message
            .as_deref(),
        Some(DRIVEN_NO_FORMULA)
    );
    assert_eq!(draft_formula(state(&mut r.app), id), None);
}

#[test]
fn a_new_redundant_dimension_drops_its_formula() {
    let mut r = rig();
    let s = state(&mut r.app);
    let scope = s.document.param_scope();
    let (se, sf) = s.session_draft_mut().unwrap();
    // Die Linie ist schon bestimmt: ein zweites Abstandsmaß (gleicher Wert 30) wird anzeigend.
    se.selection = vec![Item::Curve(r.line)];
    let id = se
        .apply_dimension_input(sf, ConstraintAction::Distance, "Breite-10", &scope)
        .unwrap();
    assert!(sf.sketch.constraint(id).unwrap().driven);
    assert_eq!(sf.formulas.get(&id), None, "anzeigend: keine Formel");
    assert_eq!(
        se.message.as_deref(),
        Some("Dieses Maß ist bereits durch andere Abhängigkeiten bestimmt und wird nur angezeigt")
    );
}

// ---- (f) Zeichenbereich ---------------------------------------------------------------------

#[test]
fn canvas_shows_value_not_formula() {
    let c = context(true);
    let mut r = rig();
    {
        let s = state(&mut r.app);
        let scope = s.document.param_scope();
        let (se, sf) = s.session_draft_mut().unwrap();
        se.set_dimension_input(sf, r.dim, "Breite", &scope).unwrap();
    }
    let shot = settle(&c, &mut r.app, SIZE);
    let side_r = side(&c);
    let canvas: Vec<&Seen> = shot
        .texts
        .iter()
        .filter(|t| !side_r.contains(t.rect.center()))
        .collect();
    assert!(
        canvas.iter().any(|t| t.text.contains("40,00")),
        "Wert im Zeichenbereich: {:?}",
        canvas.iter().map(|t| &t.text).collect::<Vec<_>>()
    );
    assert!(
        !canvas.iter().any(|t| t.text.contains("Breite")),
        "keine Formel im Zeichenbereich"
    );
}

// ---- (g)/(i) Beenden, Verwerfen ---------------------------------------------------------------

#[test]
fn close_writes_formulas() {
    let c = context(true);
    let mut r = rig();
    let id = r.dim;
    // Linie und Maß zuerst als eigener Schritt ins Dokument.
    state(&mut r.app).close_sketch();
    settle(&c, &mut r.app, SIZE);
    state(&mut r.app).open_sketch(r.sketch).unwrap();
    settle(&c, &mut r.app, SIZE);
    sketch_panel::open_existing_dimension_editor(&c, state(&mut r.app), id);
    settle(&c, &mut r.app, SIZE);
    type_text(&c, &mut r.app, SIZE, "Breite");
    enter(&c, &mut r.app, SIZE);
    settle(&c, &mut r.app, SIZE);
    let edits = state(&mut r.app).document.edit_count();
    // „Skizze beenden“ über den Knopf im Fuß.
    let shot = settle(&c, &mut r.app, SIZE);
    let foot = shell_panels::field_rects(&c)
        .into_iter()
        .find(|(n, _)| *n == "Fuß")
        .unwrap()
        .1;
    let end = shot.find_in(foot, "Skizze beenden").rect.center();
    click(&c, &mut r.app, SIZE, end);
    settle(&c, &mut r.app, SIZE);
    let s = state(&mut r.app);
    assert!(s.sketch_session.is_none(), "Skizze beendet");
    assert_eq!(
        s.document.edit_count(),
        edits + 1,
        "ein Schritt im Dokument"
    );
    let sf = match &s.document.feature(r.sketch).unwrap().kind {
        FeatureKind::Sketch(sf) => sf.clone(),
        k => panic!("{k:?}"),
    };
    assert_eq!(sf.formulas.get(&id).map(String::as_str), Some("Breite"));
    // Parameter ändern: das Maß folgt (Neuberechnung mit dem Formelwert).
    set_param(s, r.breite, "50");
    let sk = s.document.sketch_of(r.sketch).unwrap();
    match sk.constraint(id).unwrap().kind {
        K::Distance(_, _, v) => assert_eq!(v, 50.0),
        ref k => panic!("{k:?}"),
    }
    assert!((sk.line_length(r.line).unwrap() - 50.0).abs() < 1e-6);
    // Rückgängig (Dokument) nimmt die Parameteränderung, dann die Skizze als einen Schritt.
    let tab = r.app.ws_mut().active_tab_mut();
    assert_eq!(tab.undo_label(), Some("Parameter ändern".to_string()));
    tab.undo().unwrap();
    assert_eq!(tab.undo_label(), Some("Skizze bearbeiten".to_string()));
    tab.undo().unwrap();
    let sf = match &tab.state.document.feature(r.sketch).unwrap().kind {
        FeatureKind::Sketch(sf) => sf.clone(),
        k => panic!("{k:?}"),
    };
    assert!(
        sf.formulas.is_empty(),
        "ein Schritt nimmt die Formel zurück"
    );
    match sf.sketch.constraint(id).unwrap().kind {
        K::Distance(_, _, v) => assert_eq!(v, 30.0),
        ref k => panic!("{k:?}"),
    }
}

#[test]
fn discarding_the_sketch_drops_formula_edits() {
    // Rückgängig in der Skizze bis zum Anfang, dann beenden: das Dokument bleibt unverändert.
    let mut r = rig();
    let id = r.dim;
    let s = state(&mut r.app);
    s.close_sketch();
    let edits = s.document.edit_count();
    let snap = s.document.snapshot();
    s.open_sketch(r.sketch).unwrap();
    let scope = s.document.param_scope();
    let (se, sf) = s.session_draft_mut().unwrap();
    se.set_dimension_input(sf, id, "Breite", &scope).unwrap();
    s.observe_sketch(false);
    assert!(s.undo_sketch());
    s.close_sketch();
    assert_eq!(s.document.edit_count(), edits, "nichts übernommen");
    let sf = match &s.document.feature(r.sketch).unwrap().kind {
        FeatureKind::Sketch(sf) => sf.clone(),
        k => panic!("{k:?}"),
    };
    assert!(sf.formulas.is_empty());
    // Das Dokument wird zurückgeladen, während die Skizze mit Formel offen ist: der Entwurf
    // (samt Formel) entfällt.
    s.open_sketch(r.sketch).unwrap();
    let (se, sf) = s.session_draft_mut().unwrap();
    se.set_dimension_input(sf, id, "Breite", &scope).unwrap();
    s.restore_document(snap).unwrap();
    assert!(s.sketch_session.is_none() && s.sketch_draft.is_none());
    let sf = match &s.document.feature(r.sketch).unwrap().kind {
        FeatureKind::Sketch(sf) => sf.clone(),
        k => panic!("{k:?}"),
    };
    assert!(sf.formulas.is_empty(), "keine Formel im Dokument");
}

// ---- Parameter nur lesend, Meldungen, Layout ------------------------------------------------

#[test]
fn unknown_names_show_an_error_and_lock_ok() {
    let c = context(true);
    let mut r = rig();
    let id = r.dim;
    settle(&c, &mut r.app, SIZE);
    sketch_panel::open_existing_dimension_editor(&c, state(&mut r.app), id);
    settle(&c, &mut r.app, SIZE);
    type_text(&c, &mut r.app, SIZE, "Breit");
    let shot = settle(&c, &mut r.app, SIZE);
    let msg = "Den Namen „Breit“ gibt es nicht.";
    let seen = shot.find_in(side(&c), msg);
    assert_eq!(seen.color, theme::palette(true).error);
    assert!(
        shot.has_in(side(&c), "Namen: Breite, w"),
        "Hinweis mit den Namen"
    );
    // „OK“ gesperrt: ein Klick ändert nichts.
    let before = state(&mut r.app).sketch_draft.clone();
    let ok = shot.find_in(side(&c), "OK").rect.center();
    click(&c, &mut r.app, SIZE, ok);
    assert_eq!(state(&mut r.app).sketch_draft, before);
    assert!(sketch_panel::dimension_editor_open(&c));
    assert_eq!(
        state(&mut r.app).sketch_session.as_ref().unwrap().message,
        None,
        "gesperrt: kein Versuch"
    );
    // Eingabe: abgelehnt mit derselben Meldung (einmal gezeigt), nichts ändert sich.
    sketch_panel::open_existing_dimension_editor(&c, state(&mut r.app), id);
    settle(&c, &mut r.app, SIZE);
    type_text(&c, &mut r.app, SIZE, "Breit");
    enter(&c, &mut r.app, SIZE);
    let shot = settle(&c, &mut r.app, SIZE);
    assert_eq!(state(&mut r.app).sketch_draft, before);
    assert_eq!(
        shot.texts.iter().filter(|t| t.text == msg).count(),
        1,
        "Meldung einmal"
    );
    // Das Fenster „Parameter“ bleibt bei offener Skizze gesperrt.
    let s = state(&mut r.app);
    assert_eq!(s.add_parameter(), Err(sketch_open_lock().to_string()));
}

#[test]
fn the_editor_fits_small_screens_in_both_themes() {
    for dark in [true, false] {
        for size in [MEDIUM, SMALL] {
            let c = context(dark);
            let mut r = rig();
            state(&mut r.app).dark = dark;
            settle(&c, &mut r.app, size);
            sketch_panel::open_existing_dimension_editor(&c, state(&mut r.app), r.dim);
            settle(&c, &mut r.app, size);
            type_text(&c, &mut r.app, size, "Breite");
            let shot = settle(&c, &mut r.app, size);
            let side_r = side(&c);
            assert_eq!(side_r.width(), theme::PANEL_WIDTH, "{size:?}");
            let inner = side_r.shrink(theme::SPACE_3);
            let p = theme::palette(dark);
            for text in ["fx", "Breite", "= 40,00 mm", "OK", "Abbrechen"] {
                let t = shot.find_in(side_r, text);
                assert!(
                    inner.expand(0.5).contains_rect(t.rect),
                    "{dark} {size:?}: „{text}“ {:?} ragt aus {inner:?}",
                    t.rect
                );
                assert!(
                    t.clip.contains_rect(t.rect),
                    "{dark} {size:?}: „{text}“ abgeschnitten"
                );
            }
            // Wert in der Farbe des Themas, kein fremder Farbton.
            let value = shot.find_in(side_r, "= 40,00 mm");
            assert!(
                [p.text_primary, p.text_secondary].contains(&value.color),
                "{dark}: Wertfarbe {:?}",
                value.color
            );
            // Fehler in der Fehlerfarbe des Themas.
            type_text(&c, &mut r.app, size, "x");
            let shot = settle(&c, &mut r.app, size);
            let e = shot.find_in(side_r, "Den Namen „x“ gibt es nicht.");
            assert_eq!(e.color, p.error, "{dark}");
            assert!(inner.expand(0.5).contains_rect(e.rect), "{dark} {size:?}");
        }
    }
}

#[test]
fn accessibility_names_of_field_and_fx() {
    use egui::accesskit::Role;
    let c = context(true);
    c.enable_accesskit();
    let mut r = rig();
    {
        let s = state(&mut r.app);
        let scope = s.document.param_scope();
        let (se, sf) = s.session_draft_mut().unwrap();
        se.set_dimension_input(sf, r.dim, "Breite", &scope).unwrap();
    }
    settle(&c, &mut r.app, SIZE);
    sketch_panel::open_existing_dimension_editor(&c, state(&mut r.app), r.dim);
    settle(&c, &mut r.app, SIZE);
    let mut f = eframe::Frame::_new_kittest();
    let mut out = c.run_ui(input(SIZE, vec![]), |ui| r.app.ui(ui, &mut f));
    let update = out.platform_output.accesskit_update.take().expect("Baum");
    out.drop_without_applying_deltas();
    let node = |label: &str| {
        update
            .nodes
            .iter()
            .map(|(_, n)| n)
            .find(|n| n.label() == Some(label))
            .unwrap_or_else(|| panic!("kein Knoten „{label}“"))
            .clone()
    };
    assert_eq!(node(LIST_FX_LABEL).role(), Role::Button);
    assert_eq!(node(&fx_label()).role(), Role::Button);
    assert_eq!(node("Abstand").role(), Role::TextInput);
}
