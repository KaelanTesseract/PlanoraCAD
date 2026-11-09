//! Skizzen 7, Task 12: Spline mit Kontrollpunkten und Kegelschnitt als Werkzeuge. Klappmenü am
//! Spline-Knopf, Rho-Feld, Grad-Feld, Menü „Erzeugen“, belegbare Aktionen und die gestrichelten
//! Hilfslinien an der gewählten Kurve. Ganze App bei 1920×1080, Rohereignisse Bild für Bild.

mod common;

use eframe::App as _;
use egui::epaint::Shape;
use egui::{Event, Key, Modifiers, PointerButton, Pos2, Rect, Vec2, pos2, vec2};
use planora_app::app::{PlanoraApp, SKETCH_CREATE_ACTIONS};
use planora_app::keymap::{Action, Area, Binding, KeyChord, Keymap, SetResult};
use planora_app::shell::{STRIP_BUTTON, STRIP_TOOLS};
use planora_app::sketch_input::FieldKind;
use planora_app::sketch_kinds::ToolKind;
use planora_app::sketch_panel::{self, TOOL_ORDER};
use planora_app::sketch_state::{Item, SketchSession, Tool};
use planora_app::state::AppState;
use planora_app::workspace::{DocTab, Workspace};
use planora_app::{fonts, sketch_inline, sketch_view, theme};
use planora_feature::{ParamScope, SketchPlane, StandardPlane};
use planora_i18n::{english, german, t, with_lang};
use planora_math::DVec2;
use planora_project::config::Config;
use planora_project::library::Library;
use planora_sketch::model::{CurveId, CurveKind, Sketch};

const SIZE: Vec2 = vec2(1920.0, 1080.0);
const NONE: Modifiers = Modifiers::NONE;
/// Freie Stellen der Zeichnung, abseits von Leiste und Feldern.
const A: Pos2 = pos2(700.0, 500.0);
const B: Pos2 = pos2(1000.0, 500.0);
const C: Pos2 = pos2(1000.0, 700.0);
const D: Pos2 = pos2(850.0, 300.0);
const E: Pos2 = pos2(700.0, 800.0);
const F: Pos2 = pos2(1200.0, 300.0);
const G: Pos2 = pos2(1200.0, 800.0);
/// Stelle ohne Geometrie, an der der Zeiger nichts hervorhebt.
const AWAY: Pos2 = pos2(1400.0, 900.0);

fn key(k: Key) -> Event {
    Event::Key {
        key: k,
        physical_key: Some(k),
        pressed: true,
        repeat: false,
        modifiers: NONE,
    }
}

fn pointer(p: Pos2, b: PointerButton, pressed: bool) -> Event {
    Event::PointerButton {
        pos: p,
        button: b,
        pressed,
        modifiers: NONE,
    }
}

/// Texte und Strecken eines Bilds.
#[derive(Default)]
struct Shot {
    texts: Vec<(String, Rect)>,
    segments: usize,
}

fn walk(shape: &Shape, s: &mut Shot) {
    match shape {
        Shape::Vec(v) => v.iter().for_each(|x| walk(x, s)),
        Shape::Text(t) => s.texts.push((
            t.galley.text().to_string(),
            Rect::from_min_size(t.pos, t.galley.size()),
        )),
        Shape::LineSegment { .. } => s.segments += 1,
        _ => {}
    }
}

impl Shot {
    fn has(&self, text: &str) -> bool {
        self.texts.iter().any(|(t, _)| t == text)
    }

    /// Der letzte (oberste) Text `text`.
    fn find(&self, text: &str) -> Pos2 {
        self.texts
            .iter()
            .rfind(|(t, _)| t == text)
            .unwrap_or_else(|| panic!("„{text}“ fehlt"))
            .1
            .center()
    }
}

struct Rig {
    ctx: egui::Context,
    app: PlanoraApp,
    time: f64,
}

impl Rig {
    fn new() -> Rig {
        let root = common::temp_root("planora-sketch-splines-tools");
        let mut w = Workspace::new(Some(Library::open(&root).unwrap()), Config::default());
        w.config_path = root.join("config.toml");
        let s = &mut w.active_tab_mut().state;
        let id = s
            .add_sketch(SketchPlane::Standard(StandardPlane::XY))
            .unwrap();
        s.open_sketch(id).unwrap();
        w.observe_history(false);
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

    fn ws(&mut self) -> &mut Workspace {
        self.app.ws_mut()
    }

    fn tab(&mut self) -> &mut DocTab {
        self.app.ws_mut().active_tab_mut()
    }

    fn st(&mut self) -> &mut AppState {
        &mut self.tab().state
    }

    fn se(&mut self) -> &mut SketchSession {
        self.st().sketch_session.as_mut().expect("Skizze offen")
    }

    fn sk(&mut self) -> &Sketch {
        self.st().session_sketch().expect("Skizze offen")
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

    fn press(&mut self, k: Key) -> Shot {
        self.frame(vec![key(k)]);
        self.settle()
    }

    fn click_with(&mut self, p: Pos2, b: PointerButton) -> Shot {
        // Kein Doppelklick mit dem vorigen Klick.
        self.time += 1.0;
        self.frame(vec![Event::PointerMoved(p)]);
        self.frame(vec![pointer(p, b, true)]);
        self.frame(vec![pointer(p, b, false)]);
        self.settle()
    }

    fn click(&mut self, p: Pos2) -> Shot {
        self.click_with(p, PointerButton::Primary)
    }

    fn right_click(&mut self, p: Pos2) -> Shot {
        self.click_with(p, PointerButton::Secondary)
    }

    /// Zeiger ohne Klick an die Stelle.
    fn hover(&mut self, p: Pos2) -> Shot {
        self.frame(vec![Event::PointerMoved(p)]);
        self.settle()
    }

    /// Tippt in das Feld, das die Tastatur hat.
    fn type_text(&mut self, text: &str) {
        self.frame(vec![Event::Text(text.to_string())]);
        self.settle();
    }

    fn strip(&self, label: &str) -> Pos2 {
        sketch_panel::strip_buttons(&self.ctx)
            .into_iter()
            .find(|(l, _)| l == label)
            .unwrap_or_else(|| panic!("Werkzeug „{label}“ fehlt"))
            .1
            .center()
    }

    fn entry(&self, label: &str) -> Pos2 {
        sketch_panel::flyout_entries(&self.ctx)
            .into_iter()
            .find(|(l, _)| l == label)
            .unwrap_or_else(|| panic!("Eintrag „{label}“ fehlt"))
            .1
            .center()
    }

    fn choose(&mut self, kind: ToolKind) {
        let (se, sk) = self.st().session_sketch_mut().unwrap();
        se.choose_kind(sk, kind);
        self.settle();
    }

    fn undo_len(&mut self) -> usize {
        self.st().sketch_history().unwrap().undo_len()
    }

    fn curves(&mut self) -> Vec<CurveKind> {
        self.sk().curves().map(|(_, d)| d.kind.clone()).collect()
    }

    fn only_curve(&mut self) -> (CurveId, CurveKind) {
        let found: Vec<_> = self
            .sk()
            .curves()
            .map(|(c, d)| (c, d.kind.clone()))
            .collect();
        assert_eq!(found.len(), 1, "genau eine Kurve");
        found.into_iter().next().unwrap()
    }

    /// Punktzahl, Kurvenzahl, Abhängigkeiten und Variablen: Was ein Abbruch nicht ändern darf.
    fn fingerprint(&mut self) -> (usize, usize, usize, Vec<u64>) {
        let sk = self.sk();
        (
            sk.points().count(),
            sk.curves().count(),
            sk.constraints().count(),
            sk.variables().iter().map(|v| v.to_bits()).collect(),
        )
    }

    /// Start, Ende und Scheitel eines Kegelschnitts, das Rho-Feld steht danach am Zeiger.
    fn conic_up_to_rho(&mut self) {
        self.choose(ToolKind::Conic);
        self.click(A);
        self.click(B);
        self.click(D);
    }

    /// Ein fertiger Kontrollpunkt-Spline durch drei Klicks.
    fn control_spline(&mut self) -> CurveId {
        self.choose(ToolKind::SplineControl);
        self.click(A);
        self.click(B);
        self.click(C);
        self.press(Key::Enter);
        self.only_curve().0
    }

    /// Ein fertiger Kegelschnitt mit Rho 0,5.
    fn conic(&mut self) -> CurveId {
        self.conic_up_to_rho();
        self.press(Key::Enter);
        self.only_curve().0
    }

    /// Zwei Bilder ohne Hervorhebung: Strecken mit und ohne Wahl der Kurve `c`.
    fn segments_with_and_without(&mut self, c: CurveId) -> (usize, usize) {
        self.se().selection = vec![];
        let without = self.hover(AWAY).segments;
        self.se().selection = vec![Item::Curve(c)];
        let with = self.hover(AWAY).segments;
        (with, without)
    }
}

#[test]
fn spline_button_has_three_kinds() {
    use ToolKind as K;
    assert_eq!(
        ToolKind::of(Tool::Spline),
        [K::SplineFit, K::SplineControl, K::Conic]
    );
    assert!(sketch_panel::has_flyout(Tool::Spline));
    for kind in ToolKind::of(Tool::Spline) {
        assert_eq!(kind.tool(), Tool::Spline, "{kind:?}");
    }
    with_lang(&german(), || {
        assert_eq!(K::SplineFit.label(), "Spline durch Stützpunkte");
        assert_eq!(K::SplineControl.label(), "Spline mit Kontrollpunkten");
        assert_eq!(K::Conic.label(), "Kegelschnitt");
    });
    with_lang(&english(), || {
        assert_eq!(K::SplineFit.label(), "Fit Point Spline");
        assert_eq!(K::SplineControl.label(), "Control Point Spline");
        assert_eq!(K::Conic.label(), "Conic Curve");
    });

    let mut rig = Rig::new();
    assert_eq!(rig.se().kind_of(Tool::Spline), Some(K::SplineFit));
    assert_eq!(rig.se().spline_degree, 5);
    assert_eq!(rig.se().conic_rho, 0.5);
    let p = rig.strip("Spline");
    rig.right_click(p);
    assert!(sketch_panel::flyout_open(&rig.ctx));
    let names: Vec<String> = sketch_panel::flyout_entries(&rig.ctx)
        .into_iter()
        .map(|(l, _)| l)
        .collect();
    assert_eq!(
        names,
        [K::SplineFit, K::SplineControl, K::Conic].map(|k| k.label())
    );
    let e = rig.entry(&K::SplineControl.label());
    rig.click(e);
    assert_eq!(rig.se().tool, Tool::Spline);
    assert_eq!(rig.se().kind_of(Tool::Spline), Some(K::SplineControl));
    // Der Knopf zeigt die zuletzt gewählte Art: Ein Klick darauf wählt sie wieder.
    rig.choose(K::CircleCenter);
    rig.click(p);
    assert_eq!(rig.se().tool, Tool::Spline);
    assert_eq!(rig.se().kind_of(Tool::Spline), Some(K::SplineControl));
    let km = rig.ws().keymap.clone();
    assert_eq!(
        sketch_panel::strip_title(&km, rig.se(), Tool::Spline),
        K::SplineControl.label()
    );
}

#[test]
fn strip_keeps_twenty_tools_and_its_width() {
    assert_eq!(STRIP_TOOLS, 20);
    assert_eq!(TOOL_ORDER.len(), 20);
    assert_eq!(TOOL_ORDER.iter().filter(|t| **t == Tool::Spline).count(), 1);
    let rig = Rig::new();
    let buttons = sketch_panel::strip_buttons(&rig.ctx);
    assert_eq!(buttons.len(), 20);
    let (left, right) = (buttons[0].1.left(), buttons[0].1.right());
    for (name, r) in &buttons {
        assert_eq!(r.width(), STRIP_BUTTON, "{name}: Breite");
        assert_eq!((r.left(), r.right()), (left, right), "{name}: Spalte");
        assert!(r.bottom() <= SIZE.y, "{name}: im Bild, ohne Rollen: {r:?}");
    }
}

#[test]
fn control_spline_by_clicks_is_one_undo_step() {
    let mut rig = Rig::new();
    let before = rig.fingerprint();
    let steps = rig.undo_len();
    rig.choose(ToolKind::SplineControl);
    for p in [A, B, C, D] {
        rig.click(p);
    }
    assert!(rig.curves().is_empty(), "noch keine Kurve");
    assert_eq!(rig.se().pending.len(), 4);
    rig.press(Key::Enter);
    let (_, kind) = rig.only_curve();
    let CurveKind::ControlSpline { points, degree, .. } = kind else {
        panic!("Spline mit Kontrollpunkten erwartet: {kind:?}");
    };
    assert_eq!(points.len(), 4);
    assert_eq!(degree, 3, "Grad 5 gewählt, bei 4 Punkten wirksam 3");
    assert!(rig.se().pending.is_empty());
    assert_eq!(rig.undo_len(), steps + 1, "ein Rückgängig-Schritt");
    with_lang(&german(), || {
        assert_eq!(t("undo-spline"), "Spline zeichnen");
    });
    assert_eq!(
        rig.st().sketch_undo_label().as_deref(),
        Some(t("undo-spline").as_str())
    );
    rig.tab().undo().unwrap();
    assert_eq!(rig.fingerprint(), before, "ganz zurück");
}

#[test]
fn esc_after_first_click_leaves_nothing() {
    for kind in [ToolKind::SplineControl, ToolKind::Conic] {
        for cancel in [0, 1, 2] {
            let mut rig = Rig::new();
            let before = rig.fingerprint();
            let steps = rig.undo_len();
            rig.choose(kind);
            rig.click(A);
            match cancel {
                0 => {
                    rig.press(Key::Escape);
                }
                1 => {
                    rig.right_click(AWAY);
                }
                _ => {
                    let (se, sk) = rig.st().session_sketch_mut().unwrap();
                    se.set_tool(sk, Tool::Line);
                    rig.settle();
                }
            }
            assert_eq!(rig.fingerprint(), before, "{kind:?}, Abbruch {cancel}");
            assert_eq!(rig.undo_len(), steps, "{kind:?}, Abbruch {cancel}");
            assert!(rig.se().pending.is_empty(), "{kind:?}");
            assert!(rig.se().pending_pos.is_empty(), "{kind:?}");
        }
    }
    // Auch der erste Klick des nächsten Splines derselben Sitzung.
    let mut rig = Rig::new();
    rig.control_spline();
    let before = rig.fingerprint();
    let steps = rig.undo_len();
    rig.click(D);
    rig.press(Key::Escape);
    assert_eq!(rig.fingerprint(), before, "zweiter Spline: nichts bleibt");
    assert_eq!(rig.undo_len(), steps);
}

#[test]
fn conic_three_clicks_then_rho_enter() {
    let mut rig = Rig::new();
    let before = rig.fingerprint();
    let steps = rig.undo_len();
    rig.conic_up_to_rho();
    assert_eq!(
        rig.fingerprint(),
        before,
        "die Lagen ändern die Skizze nicht"
    );
    assert_eq!(rig.undo_len(), steps);
    let kinds: Vec<FieldKind> = rig.se().input.fields.iter().map(|f| f.kind).collect();
    assert_eq!(kinds, [FieldKind::Rho], "das Rho-Feld steht am Zeiger");
    with_lang(&german(), || assert_eq!(FieldKind::Rho.label(), "Rho"));
    with_lang(&english(), || assert_eq!(FieldKind::Rho.label(), "Rho"));
    let st = rig.st();
    let (se, sf) = st.session_draft_mut().unwrap();
    with_lang(&german(), || {
        assert_eq!(sketch_inline::suggestion(se, sf, FieldKind::Rho), "0,5");
    });
    rig.type_text("0,7");
    rig.press(Key::Enter);
    let (_, kind) = rig.only_curve();
    let CurveKind::Conic {
        start,
        end,
        apex,
        rho,
    } = kind
    else {
        panic!("Kegelschnitt erwartet: {kind:?}");
    };
    assert_eq!(rho, 0.7);
    let spots: Vec<DVec2> = [start, end, apex]
        .iter()
        .map(|&p| rig.sk().point_xy(p).unwrap())
        .collect();
    let chord = spots[1] - spots[0];
    assert!(chord.perp_dot(spots[2] - spots[0]).abs() > 1e-6);
    assert_eq!(rig.undo_len(), steps + 1, "ein Rückgängig-Schritt");
    assert_eq!(
        rig.st().sketch_undo_label().as_deref(),
        Some(t("undo-conic").as_str())
    );
    assert!(rig.se().input.fields.is_empty(), "das Feld ist fort");
    assert_eq!(rig.se().conic_rho, 0.7, "gilt für den nächsten");
    rig.tab().undo().unwrap();
    assert_eq!(rig.fingerprint(), before);

    // Auch ein Klick legt die Kurve an, mit dem vorgeschlagenen Wert.
    let mut rig = Rig::new();
    rig.conic_up_to_rho();
    let points = rig.sk().points().count();
    rig.click(AWAY);
    let (_, kind) = rig.only_curve();
    assert!(matches!(kind, CurveKind::Conic { rho, .. } if rho == 0.5));
    assert_eq!(
        rig.sk().points().count(),
        points + 3,
        "der Klick setzt keinen weiteren Punkt"
    );
}

#[test]
fn conic_apex_on_the_chord_is_refused_at_once() {
    let mut rig = Rig::new();
    rig.choose(ToolKind::Conic);
    rig.click(A);
    rig.click(B);
    let steps = rig.undo_len();
    rig.click(pos2(850.0, 500.0));
    assert_eq!(rig.se().pending_pos.len(), 2, "die Stufe bleibt");
    assert_eq!(rig.undo_len(), steps);
    assert!(rig.se().message.is_some(), "eine Meldung erklärt es");
    assert!(rig.curves().is_empty());
}

#[test]
fn conic_rho_field_refuses_out_of_range() {
    let mut rig = Rig::new();
    rig.conic_up_to_rho();
    let before = rig.fingerprint();
    let steps = rig.undo_len();
    let scope = ParamScope::default();
    for bad in ["0", "1", "1,5", "-0,2", "abc", "a + 1", "1 mm"] {
        let st = rig.st();
        let se = st.sketch_session.as_mut().unwrap();
        se.type_field(0, bad, &scope);
        assert!(se.input.fields[0].error.is_some(), "{bad}: Fehler im Feld");
        assert_eq!(se.input.fields[0].locked, None, "{bad}");
        rig.press(Key::Enter);
        assert_eq!(rig.fingerprint(), before, "{bad}: nichts angelegt");
        assert_eq!(rig.undo_len(), steps, "{bad}");
        assert_eq!(rig.se().pending_pos.len(), 3, "{bad}: die Stufe bleibt");
        assert!(rig.se().message.is_some(), "{bad}: eine Meldung erklärt es");
    }
    // Die Ränder knapp innerhalb gelten, Komma und Punkt.
    for (good, value) in [("0,001", 0.001), ("0.999", 0.999)] {
        let se = rig.se();
        se.type_field(0, good, &scope);
        assert_eq!(se.input.fields[0].error, None, "{good}");
        assert_eq!(se.input.fields[0].locked, Some(value), "{good}");
    }
    rig.press(Key::Enter);
    let (_, kind) = rig.only_curve();
    assert!(matches!(kind, CurveKind::Conic { rho, .. } if rho == 0.999));
}

#[test]
fn esc_in_rho_leaves_nothing() {
    for cancel in [0, 1, 2] {
        let mut rig = Rig::new();
        let before = rig.fingerprint();
        let steps = rig.undo_len();
        rig.conic_up_to_rho();
        rig.type_text("0,3");
        match cancel {
            0 => {
                rig.press(Key::Escape);
            }
            1 => {
                rig.right_click(AWAY);
            }
            _ => {
                let (se, sk) = rig.st().session_sketch_mut().unwrap();
                se.set_tool(sk, Tool::Select);
                rig.settle();
            }
        }
        assert_eq!(rig.fingerprint(), before, "Abbruch {cancel}");
        assert_eq!(rig.undo_len(), steps, "Abbruch {cancel}");
        assert!(rig.se().pending_pos.is_empty(), "Abbruch {cancel}");
        assert!(rig.se().input.fields.is_empty(), "Abbruch {cancel}");
        assert_eq!(rig.se().conic_rho, 0.5, "ein abgebrochener zählt nicht");
    }
}

#[test]
fn degree_field_applies_to_new_spline() {
    let mut rig = Rig::new();
    assert!(
        sketch_panel::degree_buttons(&rig.ctx).is_empty(),
        "nur beim Spline mit Kontrollpunkten"
    );
    rig.choose(ToolKind::SplineFit);
    assert!(sketch_panel::degree_buttons(&rig.ctx).is_empty());
    rig.choose(ToolKind::Conic);
    assert!(sketch_panel::degree_buttons(&rig.ctx).is_empty());
    rig.choose(ToolKind::SplineControl);
    let buttons = sketch_panel::degree_buttons(&rig.ctx);
    assert_eq!(buttons.iter().map(|b| b.0).collect::<Vec<u8>>(), [5, 3]);

    // Vorgabe 5.
    for p in [A, B, C, D, E, F] {
        rig.click(p);
    }
    rig.press(Key::Enter);
    let (first, kind) = rig.only_curve();
    assert!(matches!(kind, CurveKind::ControlSpline { degree: 5, .. }));

    // 3 gewählt: gilt für die neue Kurve, die vorhandene bleibt.
    let three = buttons.iter().find(|b| b.0 == 3).unwrap().1.center();
    rig.click(three);
    assert_eq!(rig.se().spline_degree, 3);
    for p in [
        G,
        AWAY,
        pos2(600.0, 300.0),
        pos2(600.0, 700.0),
        pos2(500.0, 500.0),
    ] {
        rig.click(p);
    }
    rig.press(Key::Enter);
    let curves: Vec<(CurveId, CurveKind)> = rig
        .sk()
        .curves()
        .map(|(c, d)| (c, d.kind.clone()))
        .collect();
    assert_eq!(curves.len(), 2);
    for (c, kind) in curves {
        let want = if c == first { 5 } else { 3 };
        assert!(
            matches!(kind, CurveKind::ControlSpline { degree, .. } if degree == want),
            "{kind:?}"
        );
    }

    // Die Wahl bleibt beim Wechsel der Art.
    rig.choose(ToolKind::SplineFit);
    rig.choose(ToolKind::SplineControl);
    assert_eq!(rig.se().spline_degree, 3);
}

#[test]
fn fit_spline_stays_what_it_was() {
    let mut rig = Rig::new();
    let p = rig.strip("Spline");
    rig.click(p);
    assert_eq!(rig.se().tool, Tool::Spline);
    assert_eq!(rig.se().planned_kind(), None);
    rig.click(A);
    rig.click(B);
    rig.click(C);
    rig.press(Key::Enter);
    let (_, kind) = rig.only_curve();
    assert!(matches!(kind, CurveKind::Spline { .. }), "{kind:?}");
}

#[test]
fn menu_create_has_conic() {
    assert!(SKETCH_CREATE_ACTIONS.contains(&Action::SketchConic));
    let mut rig = Rig::new();
    let shot = rig.settle();
    let menu = shot.find("Erzeugen");
    let shot = rig.click(menu);
    // Eigener Eintrag wie in Fusion, nicht im Untermenü „Spline“.
    let label = ToolKind::Conic.label();
    assert!(shot.has(&label), "eigener Eintrag im Menü");
    let sub = shot.find("Spline");
    let shot = rig.click(sub);
    assert!(shot.has(&ToolKind::SplineFit.label()));
    assert!(shot.has(&ToolKind::SplineControl.label()));
    let at = shot.find(&label);
    let shot = rig.click(at);
    assert_eq!(rig.se().tool, Tool::Spline);
    assert_eq!(rig.se().kind_of(Tool::Spline), Some(ToolKind::Conic));
    assert!(!shot.has(&ToolKind::SplineControl.label()), "Menü zu");
}

#[test]
fn actions_are_bindable_without_default_key() {
    use ToolKind as K;
    assert_eq!(
        Action::from_id("skizze.spline_kontrollpunkte"),
        Some(Action::SketchSplineControl)
    );
    assert_eq!(
        Action::from_id("skizze.kegelschnitt"),
        Some(Action::SketchConic)
    );
    assert_eq!(K::SplineControl.action(), Action::SketchSplineControl);
    assert_eq!(K::Conic.action(), Action::SketchConic);
    let km = Keymap::defaults();
    for kind in [K::SplineFit, K::SplineControl, K::Conic] {
        let a = kind.action();
        assert_eq!(a.area(), Area::Sketch, "{kind:?}");
        assert!(km.bindings(a).is_empty(), "{kind:?}: keine Vorgabetaste");
    }
    let mut rig = Rig::new();
    for kind in [K::SplineControl, K::Conic, K::SplineFit] {
        let mut km = rig.ws().keymap.clone();
        let chord = KeyChord {
            key: Key::F9,
            ctrl: false,
            shift: false,
            alt: false,
        };
        assert_eq!(km.set(kind.action(), 0, Binding::Key(chord)), SetResult::Ok);
        rig.ws().keymap = km;
        rig.press(Key::F9);
        assert_eq!(rig.se().tool, Tool::Spline, "{kind:?}");
        assert_eq!(rig.se().kind_of(Tool::Spline), Some(kind), "{kind:?}");
        let (se, sk) = rig.st().session_sketch_mut().unwrap();
        se.set_tool(sk, Tool::Select);
        let mut km = rig.ws().keymap.clone();
        km.remove(kind.action(), 0);
        rig.ws().keymap = km;
    }
}

#[test]
fn control_polygon_drawn_while_selected() {
    let mut rig = Rig::new();
    let c = rig.control_spline();
    let (se, sk) = {
        let st = rig.st();
        (
            st.sketch_session.clone().unwrap(),
            st.session_sketch().unwrap().clone(),
        )
    };
    assert!(
        sketch_view::curve_guides(&se, &sk).is_empty(),
        "nicht gewählt"
    );
    rig.se().selection = vec![Item::Curve(c)];
    let (se, sk) = {
        let st = rig.st();
        (
            st.sketch_session.clone().unwrap(),
            st.session_sketch().unwrap().clone(),
        )
    };
    let guides = sketch_view::curve_guides(&se, &sk);
    let CurveKind::ControlSpline { points, .. } = rig.only_curve().1 else {
        panic!("Spline mit Kontrollpunkten erwartet");
    };
    let want: Vec<DVec2> = points.iter().map(|&p| sk.point_xy(p).unwrap()).collect();
    assert_eq!(guides, vec![want], "die Kontrollpunkte der Reihe nach");
    let (with, without) = rig.segments_with_and_without(c);
    assert!(with >= without + 8, "gestrichelt: {with} gegen {without}");
    assert_eq!(rig.sk().curves().count(), 1, "das Polygon ist keine Kurve");

    // Auch beim Zeichnen: vom ersten Punkt bis zum Zeiger, dazu die Kurve.
    let mut rig = Rig::new();
    rig.choose(ToolKind::SplineControl);
    let idle = rig.hover(AWAY).segments;
    rig.click(A);
    rig.click(B);
    let drawing = rig.hover(C).segments;
    assert!(drawing >= idle + 8, "beim Zeichnen: {drawing} gegen {idle}");
    let (se, sk) = {
        let st = rig.st();
        (
            st.sketch_session.clone().unwrap(),
            st.session_sketch().unwrap().clone(),
        )
    };
    let cursor = se.draw_cursor.as_ref().unwrap().at;
    let curve = se.control_spline_preview(&sk, cursor).expect("Vorschau");
    assert!(curve.len() > 2, "die Kurve, nicht nur das Polygon");
    let first = sk.point_xy(se.pending[0]).unwrap();
    assert!(curve[0].distance(first) < 1e-9, "beginnt am ersten Punkt");
    assert!(
        curve[curve.len() - 1].distance(cursor) < 1e-9,
        "endet am Zeiger"
    );
}

#[test]
fn conic_guides_drawn_while_selected() {
    let mut rig = Rig::new();
    let c = rig.conic();
    let (se, sk) = {
        let st = rig.st();
        (
            st.sketch_session.clone().unwrap(),
            st.session_sketch().unwrap().clone(),
        )
    };
    assert!(
        sketch_view::curve_guides(&se, &sk).is_empty(),
        "nicht gewählt"
    );
    rig.se().selection = vec![Item::Curve(c)];
    let (se, sk) = {
        let st = rig.st();
        (
            st.sketch_session.clone().unwrap(),
            st.session_sketch().unwrap().clone(),
        )
    };
    let CurveKind::Conic {
        start, end, apex, ..
    } = rig.only_curve().1
    else {
        panic!("Kegelschnitt erwartet");
    };
    let xy = |p| sk.point_xy(p).unwrap();
    assert_eq!(
        sketch_view::curve_guides(&se, &sk),
        vec![vec![xy(start), xy(apex), xy(end)]],
        "Start–Scheitel und Scheitel–Ende"
    );
    let (with, without) = rig.segments_with_and_without(c);
    assert!(with >= without + 8, "gestrichelt: {with} gegen {without}");
    assert_eq!(
        rig.sk().curves().count(),
        1,
        "die Hilfslinien sind keine Kurven"
    );

    // Andere gewählte Kurven haben keine Hilfslinien.
    let mut rig = Rig::new();
    let p = rig.strip("Spline");
    rig.click(p);
    rig.click(A);
    rig.click(B);
    rig.press(Key::Enter);
    let (spline, _) = rig.only_curve();
    rig.se().selection = vec![Item::Curve(spline)];
    let st = rig.st();
    let se = st.sketch_session.clone().unwrap();
    assert!(sketch_view::curve_guides(&se, st.session_sketch().unwrap()).is_empty());
}

#[test]
fn conic_preview_follows_rho_through_the_shoulder() {
    let mut rig = Rig::new();
    rig.conic_up_to_rho();
    rig.hover(AWAY);
    let scope = ParamScope::default();
    for rho in [0.5, 0.2, 0.9] {
        let st = rig.st();
        let (se, sk) = st.session_sketch_mut().unwrap();
        se.type_field(0, &rho.to_string(), &scope);
        let at = se.draw_cursor.as_ref().unwrap().at;
        let lines = se.kind_preview(sk, at).expect("Vorschau");
        let (s, e, a) = (se.pending_pos[0], se.pending_pos[1], se.pending_pos[2]);
        let shoulder = (s + e) / 2.0 + (a - (s + e) / 2.0) * rho;
        let hit = lines.iter().flatten().any(|p| p.distance(shoulder) < 1e-9);
        assert!(hit, "Rho {rho}: die Kurve läuft durch den Schulterpunkt");
        assert!(lines.iter().any(|l| l == &vec![s, a, e]), "Hilfslinien");
    }
}

#[test]
fn rho_field_and_model_agree_at_the_extremes() {
    let scope = ParamScope::default();
    // Was das Modell ablehnt, lehnt schon das Feld ab, mit einer Meldung, die dafür stimmt.
    let mut rig = Rig::new();
    rig.conic_up_to_rho();
    let before = rig.fingerprint();
    for bad in ["0,9999999", "0,0000001"] {
        let se = rig.se();
        se.type_field(0, bad, &scope);
        assert_eq!(se.input.fields[0].locked, None, "{bad}");
        assert!(se.input.fields[0].error.is_some(), "{bad}: Fehler im Feld");
        rig.press(Key::Enter);
        assert_eq!(rig.fingerprint(), before, "{bad}");
    }
    with_lang(&german(), || {
        let text = t("sketch-field-rho-range");
        assert!(
            text.contains("0,000001") && text.contains("0,999999"),
            "{text}"
        );
        let text = t("sketch-err-conic-rho");
        assert!(
            text.contains("0,000001") && text.contains("0,999999"),
            "{text}"
        );
    });
    with_lang(&english(), || {
        let text = t("sketch-field-rho-range");
        assert!(
            text.contains("0.000001") && text.contains("0.999999"),
            "{text}"
        );
        let text = t("sketch-err-conic-rho");
        assert!(
            text.contains("0.000001") && text.contains("0.999999"),
            "{text}"
        );
    });
    // Die Grenzen selbst legen die Kurve an.
    for (good, value) in [("0,000001", 0.000001), ("0,999999", 0.999999)] {
        let mut rig = Rig::new();
        rig.conic_up_to_rho();
        rig.se().type_field(0, good, &scope);
        rig.press(Key::Enter);
        let (_, kind) = rig.only_curve();
        assert!(
            matches!(kind, CurveKind::Conic { rho, .. } if rho == value),
            "{good}"
        );
    }
}

#[test]
fn rho_proposal_is_a_value_the_field_accepts() {
    let scope = ParamScope::default();
    for lang in [german(), english()] {
        with_lang(&lang, || {
            for (rho, near) in [
                (0.00004, 0.00004),
                (0.99999, 0.99999),
                (0.000001, 0.000001),
                (0.999999, 0.999999),
                (0.5, 0.5),
                (0.12345678, 0.1235),
            ] {
                let mut rig = Rig::new();
                rig.conic_up_to_rho();
                rig.se().conic_rho = rho;
                let st = rig.st();
                let (se, sf) = st.session_draft_mut().unwrap();
                let shown = sketch_inline::suggestion(se, sf, FieldKind::Rho);
                se.type_field(0, &shown, &scope);
                assert_eq!(se.input.fields[0].error, None, "{rho}: „{shown}“");
                let got = se.input.fields[0].locked.expect("gesperrt");
                assert!((got - near).abs() < 1e-12, "{rho}: „{shown}“ ergibt {got}");
            }
        });
    }
}

#[test]
fn space_repeats_the_last_spline_kind() {
    for kind in [
        ToolKind::SplineFit,
        ToolKind::SplineControl,
        ToolKind::Conic,
    ] {
        let mut rig = Rig::new();
        rig.choose(kind);
        rig.click(A);
        rig.click(B);
        rig.click(D);
        rig.press(Key::Enter);
        assert_eq!(rig.sk().curves().count(), 1, "{kind:?}: gezeichnet");
        // Nur wählen zählt nicht: Eine andere Art ohne Kurve ersetzt das Gedächtnis nicht.
        let other = if kind == ToolKind::SplineFit {
            ToolKind::Conic
        } else {
            ToolKind::SplineFit
        };
        rig.choose(other);
        let (se, sk) = rig.st().session_sketch_mut().unwrap();
        se.set_tool(sk, Tool::Select);
        rig.settle();
        rig.press(Key::Space);
        assert_eq!(rig.se().tool, Tool::Spline, "{kind:?}");
        assert_eq!(rig.se().kind_of(Tool::Spline), Some(kind), "{kind:?}");
    }
}

#[test]
fn esc_leaves_nothing_at_every_stage() {
    // Der erste Klick des zweiten Splines durch Stützpunkte.
    let mut rig = Rig::new();
    rig.choose(ToolKind::SplineFit);
    rig.click(A);
    rig.click(B);
    rig.press(Key::Enter);
    assert_eq!(rig.sk().curves().count(), 1);
    let before = rig.fingerprint();
    let steps = rig.undo_len();
    rig.click(D);
    rig.press(Key::Escape);
    assert_eq!(rig.fingerprint(), before, "zweiter Spline: nichts bleibt");
    assert_eq!(rig.undo_len(), steps);

    // Kegelschnitt mit Start und Ende.
    let mut rig = Rig::new();
    let before = rig.fingerprint();
    let steps = rig.undo_len();
    rig.choose(ToolKind::Conic);
    rig.click(A);
    rig.click(B);
    assert_eq!(rig.se().pending_pos.len(), 2);
    rig.press(Key::Escape);
    assert_eq!(rig.fingerprint(), before, "Stufe 2");
    assert_eq!(rig.undo_len(), steps);
    assert!(rig.se().pending_pos.is_empty());
    assert_eq!(rig.se().kind_of(Tool::Spline), Some(ToolKind::Conic));

    // Mehrere Kontrollpunkte.
    let mut rig = Rig::new();
    let before = rig.fingerprint();
    let steps = rig.undo_len();
    rig.choose(ToolKind::SplineControl);
    for p in [A, B, C, D] {
        rig.click(p);
    }
    assert_eq!(rig.se().pending.len(), 4);
    rig.press(Key::Escape);
    assert_eq!(rig.fingerprint(), before, "vier Kontrollpunkte");
    assert_eq!(rig.undo_len(), steps);
    assert!(rig.se().pending.is_empty());
}

#[test]
fn changing_the_kind_while_drawing_leaves_nothing() {
    use ToolKind as K;
    for (from, to) in [
        (K::SplineControl, K::Conic),
        (K::SplineControl, K::SplineFit),
        (K::SplineFit, K::SplineControl),
        (K::Conic, K::SplineControl),
    ] {
        let mut rig = Rig::new();
        let before = rig.fingerprint();
        let steps = rig.undo_len();
        rig.choose(from);
        rig.click(A);
        rig.click(B);
        // Dieselbe Art wählen lässt die Lagen.
        rig.choose(from);
        assert_eq!(
            rig.se().pending.len() + rig.se().pending_pos.len(),
            2,
            "{from:?}: gleiche Art"
        );
        rig.choose(to);
        assert_eq!(rig.fingerprint(), before, "{from:?} zu {to:?}");
        assert_eq!(rig.undo_len(), steps, "{from:?} zu {to:?}");
        assert!(rig.se().pending.is_empty(), "{from:?} zu {to:?}");
        assert!(rig.se().pending_pos.is_empty(), "{from:?} zu {to:?}");
        assert_eq!(rig.se().tool, Tool::Spline);
        assert_eq!(rig.se().kind_of(Tool::Spline), Some(to));
    }
}

#[test]
fn fit_spline_action_has_its_id() {
    assert_eq!(
        Action::from_id("skizze.spline_stuetzpunkte"),
        Some(Action::SketchSplineFit)
    );
    assert_eq!(ToolKind::SplineFit.action(), Action::SketchSplineFit);
    with_lang(&english(), || {
        let hint = t("sketch-tool-spline-hint");
        assert!(hint.contains("fit points"), "{hint}");
    });
    with_lang(&german(), || {
        let hint = t("sketch-tool-spline-hint");
        assert!(hint.contains("Stützpunkte"), "{hint}");
    });
}
