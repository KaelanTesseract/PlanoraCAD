//! Skizzen 11b, Task 3: Kopieren, Ausschneiden, Einfügen in der App. Über die Befehle
//! (`commands::run_action`) wie Taste und Menü und mit echten Bildern der ganzen App
//! (`PlanoraApp::for_tests`) für Tasten, Rechtsklickmenü und Textfelder.

mod common;

use eframe::App as _;
use egui::{Event, Key, Modifiers, PointerButton, Pos2, Rect, Vec2, vec2};
use planora_app::app::PlanoraApp;
use planora_app::commands;
use planora_app::keymap::Action;
use planora_app::sketch_state::{ConstraintAction, Item, Tool};
use planora_app::state::AppState;
use planora_app::workspace::Workspace;
use planora_app::{fonts, sketch_panel, theme, viewport};
use planora_feature::{SketchPlane, StandardPlane};
use planora_i18n::t;
use planora_math::DVec2;
use planora_project::config::Config;
use planora_project::library::Library;
use planora_sketch::constraint::ConstraintKind as K;
use planora_sketch::helpers::RectIds;
use planora_sketch::model::{CurveId, Sketch};
use planora_sketch::solver::SolveStatus;

const SIZE: Vec2 = vec2(1920.0, 1080.0);
const NONE: Modifiers = Modifiers::NONE;

// ---- Hilfen ---------------------------------------------------------------------------------

fn ws() -> Workspace {
    let root = common::temp_root("planora-sketch-clipboard");
    let mut w = Workspace::new(Some(Library::open(&root).unwrap()), Config::default());
    w.config_path = root.join("config.toml");
    w
}

fn v(x: f64, y: f64) -> DVec2 {
    DVec2::new(x, y)
}

fn state(w: &mut Workspace) -> &mut AppState {
    &mut w.active_tab_mut().state
}

/// Öffnet eine neue Skizze auf der Ebene `plane`.
fn open_sketch(w: &mut Workspace, plane: StandardPlane) {
    let s = state(w);
    let id = s.add_sketch(SketchPlane::Standard(plane)).unwrap();
    s.open_sketch(id).unwrap();
    w.observe_history(false);
}

fn sk(w: &mut Workspace) -> &Sketch {
    w.active_tab().state.session_sketch().unwrap()
}

fn dims(sk: &Sketch) -> usize {
    sk.constraints()
        .filter(|(_, c)| c.kind.is_dimension())
        .count()
}

/// Rechteck 20 × 10 mit zwei Maßen, alle vier Seiten gewählt.
fn rectangle_with_dims(w: &mut Workspace) -> RectIds {
    let (se, sk) = state(w).session_sketch_mut().unwrap();
    let ids = sk.add_rectangle(v(0.0, 0.0), v(20.0, 10.0)).unwrap();
    sk.add_constraint(K::Distance(ids.points[0], ids.points[1], 20.0))
        .unwrap();
    sk.add_constraint(K::Distance(ids.points[1], ids.points[2], 10.0))
        .unwrap();
    assert_eq!(sk.solve().status, SolveStatus::Solved);
    se.selection = ids.lines.iter().map(|&c| Item::Curve(c)).collect();
    state(w).observe_sketch(false);
    w.observe_history(false);
    ids
}

fn undo_len(w: &mut Workspace) -> usize {
    state(w).sketch_history().unwrap().undo_len()
}

fn session(w: &mut Workspace) -> &mut planora_app::sketch_state::SketchSession {
    state(w).sketch_session.as_mut().unwrap()
}

fn point_bits(sk: &Sketch) -> Vec<(u64, u64)> {
    let mut all: Vec<(u64, u64)> = sk
        .points()
        .map(|(id, _)| {
            let p = sk.point_xy(id).unwrap();
            (p.x.to_bits(), p.y.to_bits())
        })
        .collect();
    all.sort_unstable();
    all
}

// ---- Bilder der ganzen App --------------------------------------------------------------------

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

    fn ws(&mut self) -> &mut Workspace {
        self.app.ws_mut()
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
        let out = self.ctx.run_ui(raw, |ui| {
            app.ui(ui, &mut f);
        });
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

    fn click(&mut self, p: Pos2) {
        self.frame(vec![Event::PointerMoved(p)]);
        self.frame(vec![button(p, true)]);
        self.frame(vec![button(p, false)]);
        self.frame(vec![]);
    }
}

// ---- Kopieren und Einfügen ------------------------------------------------------------------

#[test]
fn copy_paste_between_sketches() {
    let mut w = ws();
    open_sketch(&mut w, StandardPlane::XY);
    rectangle_with_dims(&mut w);
    commands::run_action(&mut w, Action::SketchCopy);
    assert!(w.sketch_clipboard.is_some(), "kopiert");
    assert!(session(&mut w).message_text().unwrap().contains('4'));
    // Die Quelle bleibt, wie sie war.
    assert_eq!(sk(&mut w).curves().count(), 4);
    state(&mut w).close_sketch();
    w.observe_history(false);

    // Skizze B (andere Ebene): einfügen.
    open_sketch(&mut w, StandardPlane::XZ);
    commands::run_action(&mut w, Action::SketchPaste);
    assert_eq!(sk(&mut w).curves().count(), 4);
    assert_eq!(dims(sk(&mut w)), 2);
    let se = session(&mut w);
    assert_eq!(
        se.selection
            .iter()
            .filter(|i| matches!(i, Item::Curve(_)))
            .count(),
        4,
        "alle vier Kurven gewählt"
    );
    assert_eq!(se.tool, Tool::MoveCopy, "Verschieben/Kopieren offen");
    assert!(se.transform.pasted);
}

#[test]
fn paste_and_move_is_one_undo_step() {
    let mut w = ws();
    open_sketch(&mut w, StandardPlane::XY);
    rectangle_with_dims(&mut w);
    commands::run_action(&mut w, Action::SketchCopy);
    state(&mut w).close_sketch();
    open_sketch(&mut w, StandardPlane::XZ);
    let before = undo_len(&mut w);
    commands::run_action(&mut w, Action::SketchPaste);
    assert_eq!(undo_len(&mut w), before + 1, "Einfügen ist ein Schritt");
    assert_eq!(
        state(&mut w).sketch_undo_label().as_deref(),
        Some(t("undo-sketch-paste").as_str())
    );
    let at_source = point_bits(sk(&mut w));
    // Verschieben bestätigen.
    {
        let (se, sf) = state(&mut w).session_draft_mut().unwrap();
        se.transform.offset = v(5.0, 3.0);
        se.confirm_transform(sf).unwrap();
    }
    state(&mut w).observe_sketch(false);
    assert_eq!(
        undo_len(&mut w),
        before + 1,
        "das Verschieben legt keinen zweiten Schritt an"
    );
    assert_ne!(
        point_bits(sk(&mut w)),
        at_source,
        "die Kopie ist verschoben"
    );
    assert_eq!(
        state(&mut w).sketch_undo_label().as_deref(),
        Some(t("undo-sketch-paste").as_str()),
        "der Name bleibt"
    );
    // Rückgängig: die Skizze wie vor dem Einfügen.
    assert!(state(&mut w).undo_sketch());
    assert_eq!(sk(&mut w).curves().count(), 0);
    assert_eq!(sk(&mut w).constraints().count(), 0);
    // Wiederholen bringt die verschobene Kopie zurück.
    assert!(state(&mut w).redo_sketch());
    assert_eq!(sk(&mut w).curves().count(), 4);
}

#[test]
fn paste_then_cancel_move_keeps_copy() {
    let mut w = ws();
    open_sketch(&mut w, StandardPlane::XY);
    rectangle_with_dims(&mut w);
    commands::run_action(&mut w, Action::SketchCopy);
    state(&mut w).close_sketch();
    open_sketch(&mut w, StandardPlane::XY);
    let before = undo_len(&mut w);
    commands::run_action(&mut w, Action::SketchPaste);
    let at_source = point_bits(sk(&mut w));
    {
        let (se, sk) = state(&mut w).session_sketch_mut().unwrap();
        se.transform.offset = v(7.0, 0.0);
        se.cancel(sk);
    }
    state(&mut w).observe_sketch(false);
    assert_eq!(
        point_bits(sk(&mut w)),
        at_source,
        "die Kopie bleibt an der Quelllage"
    );
    assert_eq!(sk(&mut w).curves().count(), 4);
    assert_eq!(undo_len(&mut w), before + 1, "ein Schritt „Einfügen“");
    assert_eq!(
        state(&mut w).sketch_undo_label().as_deref(),
        Some(t("undo-sketch-paste").as_str())
    );
    let se = session(&mut w);
    assert_eq!(
        se.selection
            .iter()
            .filter(|i| matches!(i, Item::Curve(_)))
            .count(),
        4,
        "die Kopie bleibt gewählt"
    );
}

#[test]
fn cut_is_one_step_with_delete_rule() {
    let mut w = ws();
    open_sketch(&mut w, StandardPlane::XY);
    let line = {
        let (se, sk) = state(&mut w).session_sketch_mut().unwrap();
        let a = sk.add_point(0.0, 0.0).unwrap();
        let b = sk.add_point(10.0, 0.0).unwrap();
        let line = sk.add_line(a, b).unwrap();
        sk.add_constraint(K::Distance(a, b, 10.0)).unwrap();
        assert_eq!(sk.solve().status, SolveStatus::Solved);
        se.selection = vec![Item::Curve(line)];
        line
    };
    let _ = line;
    state(&mut w).observe_sketch(false);
    let before = undo_len(&mut w);
    commands::run_action(&mut w, Action::SketchCut);
    let s = sk(&mut w);
    assert_eq!(s.curves().count(), 0, "die Linie ist weg");
    assert_eq!(s.points().count(), 0, "die Endpunkte auch (Löschregel)");
    assert_eq!(s.constraints().count(), 0, "und das Maß");
    assert_eq!(undo_len(&mut w), before + 1, "ein Schritt");
    assert_eq!(
        state(&mut w).sketch_undo_label().as_deref(),
        Some(t("undo-sketch-cut").as_str())
    );
    assert!(w.sketch_clipboard.is_some());
    // Einfügen bringt Linie und Maß zurück.
    commands::run_action(&mut w, Action::SketchPaste);
    let s = sk(&mut w);
    assert_eq!(s.curves().count(), 1);
    assert_eq!(dims(s), 1);
}

#[test]
fn commands_greyed_without_sketch() {
    let mut w = ws();
    // Eine Zwischenablage aus einer Skizze, dann ohne offene Skizze.
    open_sketch(&mut w, StandardPlane::XY);
    rectangle_with_dims(&mut w);
    commands::run_action(&mut w, Action::SketchCopy);
    state(&mut w).close_sketch();
    w.observe_history(false);
    let clip_before = w
        .sketch_clipboard
        .clone()
        .map(|c| c.sketch.curves().count());
    let edits = w.active_tab().state.document.edit_count();
    for a in [Action::SketchCopy, Action::SketchCut, Action::SketchPaste] {
        commands::run_action(&mut w, a);
    }
    assert_eq!(
        w.active_tab().state.document.edit_count(),
        edits,
        "keine Änderung"
    );
    assert!(w.active_tab().state.sketch_session.is_none());
    assert_eq!(
        w.sketch_clipboard
            .clone()
            .map(|c| c.sketch.curves().count()),
        clip_before,
        "die Zwischenablage bleibt"
    );
}

#[test]
fn paste_from_empty_clipboard_says_so() {
    let mut w = ws();
    open_sketch(&mut w, StandardPlane::XY);
    commands::run_action(&mut w, Action::SketchPaste);
    assert_eq!(
        session(&mut w).message_text().as_deref(),
        Some(t("sketch-paste-empty").as_str())
    );
}

#[test]
fn copy_without_selection_changes_nothing() {
    let mut w = ws();
    open_sketch(&mut w, StandardPlane::XY);
    rectangle_with_dims(&mut w);
    session(&mut w).selection.clear();
    commands::run_action(&mut w, Action::SketchCopy);
    assert!(w.sketch_clipboard.is_none());
    assert_eq!(
        session(&mut w).message_text().as_deref(),
        Some(t("sketch-nothing-selected").as_str())
    );
}

#[test]
fn paste_message_counts_dropped_formulas() {
    // Das Maß trägt die Formel eines Parameters; im anderen Dokument gibt es ihn nicht: Er wird
    // zum Wert, und die Meldung zählt es.
    let mut w = ws();
    let breite = state(&mut w).add_parameter().unwrap();
    state(&mut w)
        .edit_parameter(
            breite,
            planora_feature::ParamEdit {
                name: "Breite".into(),
                formula: "20".into(),
                unit: planora_feature::ParamUnit::Length,
                comment: String::new(),
            },
        )
        .unwrap();
    open_sketch(&mut w, StandardPlane::XY);
    rectangle_with_dims(&mut w);
    {
        let (_, sf) = state(&mut w).session_draft_mut().unwrap();
        let dim = sf
            .sketch
            .constraints()
            .find(|(_, c)| matches!(c.kind, K::Distance(_, _, x) if x == 20.0))
            .map(|(id, _)| id)
            .unwrap();
        sf.formulas.insert(dim, "Breite".into());
    }
    commands::run_action(&mut w, Action::SketchCopy);
    let clip = w.sketch_clipboard.clone().unwrap();
    assert_eq!(clip.formulas.len(), 1, "die Formel liegt in der Ablage");

    let mut other = ws();
    other.sketch_clipboard = Some(clip);
    open_sketch(&mut other, StandardPlane::XY);
    commands::run_action(&mut other, Action::SketchPaste);
    assert_eq!(dims(sk(&mut other)), 2, "das Maß kommt mit seinem Wert");
    assert!(
        state(&mut other)
            .session_draft_mut()
            .unwrap()
            .1
            .formulas
            .is_empty()
    );
    let text = session(&mut other).message_text().unwrap();
    assert!(text.contains("1 Formel"), "{text}");
}

// ---- Tasten und Menü ------------------------------------------------------------------------

#[test]
fn keys_copy_and_paste_through_the_app() {
    let mut w = ws();
    open_sketch(&mut w, StandardPlane::XY);
    rectangle_with_dims(&mut w);
    let mut rig = Rig::new(w);
    rig.frame(vec![Event::Copy]);
    assert!(rig.ws().sketch_clipboard.is_some(), "Strg+C kopiert");
    // Strg+V mit leerer Systemzwischenablage: nur das Loslassen von V kommt.
    rig.frame(vec![Event::Key {
        key: Key::V,
        physical_key: Some(Key::V),
        pressed: false,
        repeat: false,
        modifiers: Modifiers::COMMAND,
    }]);
    rig.settle();
    assert_eq!(
        rig.ws()
            .active_tab()
            .state
            .session_sketch()
            .unwrap()
            .curves()
            .count(),
        8,
        "die Kopie liegt über dem Original"
    );
}

#[test]
fn formula_field_keeps_ctrl_c() {
    let mut w = ws();
    open_sketch(&mut w, StandardPlane::XY);
    let ids = rectangle_with_dims(&mut w);
    let mut rig = Rig::new(w);
    // Eine Linie wählen und das Maßfeld (Formelfeld) öffnen.
    rig.ws()
        .active_tab_mut()
        .state
        .sketch_session
        .as_mut()
        .unwrap()
        .selection = vec![Item::Curve(ids.lines[0])];
    let ctx = rig.ctx.clone();
    sketch_panel::open_dimension_editor(
        &ctx,
        &rig.ws().active_tab().state,
        ConstraintAction::Distance,
    );
    rig.settle();
    assert!(viewport::typing(&rig.ctx), "das Maßfeld hat die Tastatur");
    rig.frame(vec![Event::Copy]);
    rig.frame(vec![Event::Cut]);
    assert!(rig.ws().sketch_clipboard.is_none(), "nichts kopiert");
    assert_eq!(
        rig.ws()
            .active_tab()
            .state
            .session_sketch()
            .unwrap()
            .curves()
            .count(),
        4,
        "nichts ausgeschnitten"
    );
}

#[test]
fn right_click_menu_with_selection_or_clip() {
    let mut w = ws();
    open_sketch(&mut w, StandardPlane::XY);
    let ids = rectangle_with_dims(&mut w);
    // Nur ein Punkt gewählt: Das Menü öffnet.
    {
        let (se, sk) = state(&mut w).session_sketch_mut().unwrap();
        se.selection = vec![Item::Point(ids.points[0])];
        se.secondary_click(sk, (100.0, 200.0));
        assert_eq!(se.curve_menu, Some((100.0, 200.0)));
        se.curve_menu = None;
        // Nichts gewählt, nichts in der Zwischenablage: kein Menü.
        se.selection.clear();
        se.secondary_click(sk, (1.0, 2.0));
        assert_eq!(se.curve_menu, None);
    }
    // Mit voller Zwischenablage öffnet es auch ohne Auswahl.
    session(&mut w).selection = ids.lines.iter().map(|&c| Item::Curve(c)).collect();
    commands::run_action(&mut w, Action::SketchCopy);
    {
        let (se, sk) = state(&mut w).session_sketch_mut().unwrap();
        se.selection.clear();
        se.secondary_click(sk, (3.0, 4.0));
        assert_eq!(se.curve_menu, Some((3.0, 4.0)));
    }
}

#[test]
fn menu_entries_run_the_commands() {
    let mut w = ws();
    open_sketch(&mut w, StandardPlane::XY);
    rectangle_with_dims(&mut w);
    session(&mut w).curve_menu = Some((300.0, 300.0));
    let mut rig = Rig::new(w);
    let seen = rig.settle();
    let at = seen
        .iter()
        .find(|s| s.text == t("action-sketch-copy"))
        .expect("Eintrag „Kopieren“")
        .rect
        .center();
    assert!(
        seen.iter().any(|s| s.text == t("action-sketch-cut")),
        "Eintrag „Ausschneiden“"
    );
    rig.click(at);
    rig.settle();
    assert!(rig.ws().sketch_clipboard.is_some(), "Kopieren aus dem Menü");
}

#[test]
fn detached_curve_ids_are_not_needed() {
    // Die Zwischenablage hält Werte, keine Ids: Die Quelle darf gelöscht sein.
    let mut w = ws();
    open_sketch(&mut w, StandardPlane::XY);
    let ids = rectangle_with_dims(&mut w);
    commands::run_action(&mut w, Action::SketchCut);
    let _: &[CurveId] = &ids.lines;
    assert_eq!(sk(&mut w).curves().count(), 0);
    commands::run_action(&mut w, Action::SketchPaste);
    assert_eq!(sk(&mut w).curves().count(), 4);
    assert_eq!(dims(sk(&mut w)), 2);
}

// ---- Fester Spline ----------------------------------------------------------------------------

/// Eine Zwischenablage mit einem festen Spline (Freiformkante eines Körpers, als freie Kopie
/// ohne Bezug) aus einer 3D-Skizze auf XY.
fn clip_with_fixed_spline() -> planora_feature::FeatureClip {
    use planora_feature::{Document, FeatureKind, ProfileSel, SketchFeature};
    use planora_sketch::clipboard::ClipSet;
    use planora_sketch::helpers::ProjectStyle;
    let tol = planora_math::Tolerance::default();
    let mut base = Sketch::new(StandardPlane::XY.frame(), "Basis", tol);
    let p: Vec<_> = [(0.0, 0.0), (10.0, 0.0), (10.0, 5.0), (5.0, 7.0), (0.0, 5.0)]
        .iter()
        .map(|&(x, y)| base.add_point(x, y).unwrap())
        .collect();
    base.add_line(p[0], p[1]).unwrap();
    base.add_line(p[1], p[2]).unwrap();
    base.add_spline(&p[2..5]).unwrap();
    base.add_line(p[4], p[0]).unwrap();
    let mut d = Document::new();
    let bs = d
        .add_feature(
            "Basis",
            FeatureKind::Sketch(SketchFeature {
                sketch: base,
                plane: SketchPlane::Standard(StandardPlane::XY),
                copies: Vec::new(),
                formulas: Default::default(),
            }),
        )
        .unwrap();
    d.add_feature(
        "Körper",
        FeatureKind::Extrude {
            sketch: bs,
            profiles: ProfileSel::Index(0),
            height: 4.0,
        },
    )
    .unwrap();
    let solid = d.visible_bodies()[0].solid.clone();
    let (_, e) = solid
        .edges
        .iter()
        .find(|(_, e)| matches!(e.curve, planora_kernel::Curve::Nurbs(_)))
        .expect("Freiformkante");
    let mut sf = SketchFeature {
        sketch: Sketch::new(StandardPlane::XY.frame(), "", tol),
        plane: SketchPlane::Standard(StandardPlane::XY),
        copies: Vec::new(),
        formulas: Default::default(),
    };
    sf.sketch
        .add_projected_curve(
            &e.curve,
            e.t0,
            e.t1,
            ProjectStyle {
                construction: false,
            },
        )
        .unwrap();
    sf.sketch
        .add_point3(planora_math::DVec3::new(1.0, 1.0, 2.0))
        .unwrap();
    sf.sketch.set_three_d(true);
    let set = ClipSet {
        curves: sf.sketch.curves().map(|(c, _)| c).collect(),
        points: sf.sketch.points().map(|(p, _)| p).collect(),
        ..ClipSet::default()
    };
    planora_feature::copy_selection(&sf, &StandardPlane::XY.frame(), &set).unwrap()
}

#[test]
fn fixed_spline_paste_stays_selected_without_the_move_tool() {
    let mut w = ws();
    w.sketch_clipboard = Some(clip_with_fixed_spline());
    open_sketch(&mut w, StandardPlane::XY);
    let before = undo_len(&mut w);
    commands::run_action(&mut w, Action::SketchPaste);
    assert!(sk(&mut w).curves().count() > 0, "eingefügt");
    assert_eq!(undo_len(&mut w), before + 1);
    let se = session(&mut w);
    assert_ne!(se.tool, Tool::MoveCopy, "das Verschieben öffnet nicht");
    assert!(!se.transform.pasted);
    assert!(!se.selection.is_empty(), "die Kopie bleibt gewählt");
    assert_eq!(
        se.message_text().as_deref(),
        Some(t("sketch-paste-fixed-spline").as_str())
    );
}

#[test]
fn paste_error_from_fixed_spline_between_planes_changes_nothing() {
    let mut w = ws();
    w.sketch_clipboard = Some(clip_with_fixed_spline());
    open_sketch(&mut w, StandardPlane::YZ);
    // Schon 3D, damit das Einfügen auf die Lage der Quelle verschieben muss.
    state(&mut w)
        .session_draft_mut()
        .unwrap()
        .1
        .sketch
        .set_three_d(true);
    state(&mut w).observe_sketch(false);
    let before = undo_len(&mut w);
    let revision = sk(&mut w).revision();
    commands::run_action(&mut w, Action::SketchPaste);
    assert_eq!(sk(&mut w).curves().count(), 0, "nichts eingefügt");
    assert_eq!(sk(&mut w).revision(), revision);
    assert_eq!(undo_len(&mut w), before, "kein Schritt");
    let text = session(&mut w).message_text().unwrap();
    assert!(text.contains(&t("sketch-move-fixed")), "{text}");
}
