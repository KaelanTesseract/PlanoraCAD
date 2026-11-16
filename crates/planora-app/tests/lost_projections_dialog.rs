//! Skizzen 11a, Task 18: Dialog „Verlorene Projektionen verwalten“ (Fusion „Manage Lost
//! Projections“): Menüeintrag nur bei verlorenen Projektionen, Zeilen, je Aktion ein
//! Rückgängig-Schritt, „Neu verknüpfen“ über die Auswahl von „Projizieren“, Esc ohne Spur, gelbe
//! Markierung und „Übrige Geometrie abblenden“. Ohne Fenster bei 1920 × 1080.

use egui::{Event, Modifiers, PointerButton, Pos2, Rect, pos2};
use planora_app::commands::run_action;
use planora_app::detach::DIALOG_KEYS;
use planora_app::keymap::Action;
use planora_app::lost_projections_dialog::{LostAction, LostProjectionsDialog};
use planora_app::state::{AppState, IncludePick, ProjectTarget, StandardPlane};
use planora_app::surface_project_dialog::DialogOutcome;
use planora_app::workspace::Workspace;
use planora_app::{browser, fonts, history_bar, sketch_view, theme};
use planora_feature::{CopySource, CopyTarget, FeatureId, FeatureKind, SketchFeature, SketchPlane};
use planora_kernel::{Edge, Id};
use planora_math::{DVec2, DVec3};
use planora_project::config::Config;
use planora_render::{EdgeRef, Pick};
use planora_sketch::model::{CurveId, CurveKind};

const SCREEN: Rect = Rect {
    min: pos2(0.0, 0.0),
    max: pos2(1920.0, 1080.0),
};

const MENU: &str = "Verlorene Projektionen verwalten";

/// Quader A (am Ursprung), Quader B (bei x = 30), dahinter „Skizze 1“ auf XY mit `n` (1 oder 2)
/// verknüpft projizierten Unterkanten von A und einer eigenen Linie.
struct Rig {
    w: Workspace,
    a: FeatureId,
    b: FeatureId,
    sketch: FeatureId,
    curves: Vec<CurveId>,
    own: CurveId,
}

impl Rig {
    fn s(&mut self) -> &mut AppState {
        &mut self.w.active_tab_mut().state
    }

    fn targets(&self) -> Vec<CopyTarget> {
        self.curves.iter().map(|c| CopyTarget::Curve(*c)).collect()
    }

    /// Löscht Quader A: Alle Projektionen der Skizze sind verloren.
    fn lose(&mut self) {
        let a = self.a;
        self.s().delete_feature(a).unwrap();
        self.w.observe_history(false);
        let n = self.curves.len();
        let sketch = self.sketch;
        let f = self.s().document.feature(sketch).unwrap();
        assert_eq!(f.lost_projections().len(), n);
    }

    fn undo_len(&self) -> usize {
        self.w.active_tab().history().undo_len()
    }
}

fn body_index(s: &AppState, feature: FeatureId) -> usize {
    s.bodies
        .iter()
        .position(|b| b.source.feature == feature)
        .expect("Körper sichtbar")
}

/// Die Unterkante (z = 0) bei `y` des sichtbaren Körpers `body`.
fn bottom_edge(s: &AppState, body: usize, y: f64) -> Id<Edge> {
    let solid = &s.bodies[body].solid;
    solid
        .edges
        .iter()
        .find(|(_, e)| {
            let on = |p: DVec3| p.z.abs() < 1e-9 && (p.y - y).abs() < 1e-9;
            on(solid.vertices[e.start].point) && on(solid.vertices[e.end].point)
        })
        .map(|(id, _)| id)
        .expect("Unterkante")
}

fn rig(n: usize) -> Rig {
    let mut w = Workspace::new(None, Config::default());
    let s = &mut w.active_tab_mut().state;
    let a = s.create_box(DVec3::new(10.0, 20.0, 5.0)).unwrap();
    let b = s
        .document
        .add_feature(
            "Quader B",
            FeatureKind::Box {
                size: DVec3::new(10.0, 20.0, 5.0),
                origin: DVec3::new(30.0, 0.0, 0.0),
            },
        )
        .unwrap();
    s.refresh();
    let sketch = s
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    s.open_sketch(sketch).unwrap();
    s.observe_sketch(false);
    let ia = body_index(s, a);
    for y in [0.0, 20.0].into_iter().take(n) {
        let e = bottom_edge(s, ia, y);
        s.project_edge_into_sketch(ia, e).unwrap();
        s.observe_sketch(false);
    }
    let own = {
        let sk = s.session_sketch_mut().unwrap().1;
        let p = sk.add_point(0.0, 40.0).unwrap();
        let q = sk.add_point(10.0, 45.0).unwrap();
        sk.add_line(p, q).unwrap()
    };
    s.observe_sketch(false);
    let curves: Vec<CurveId> = s
        .sketch_draft
        .as_ref()
        .unwrap()
        .copies
        .iter()
        .filter_map(|c| c.curve())
        .collect();
    assert_eq!(curves.len(), n);
    s.close_sketch();
    w.observe_history(false);
    Rig {
        w,
        a,
        b,
        sketch,
        curves,
        own,
    }
}

fn def(s: &AppState, id: FeatureId) -> &SketchFeature {
    match &s.document.feature(id).unwrap().kind {
        FeatureKind::Sketch(sf) => sf,
        k => panic!("keine Skizze: {k:?}"),
    }
}

fn ctx() -> egui::Context {
    let c = egui::Context::default();
    c.set_fonts(fonts::definitions());
    theme::apply(&c, true);
    c
}

fn raw(events: Vec<Event>) -> egui::RawInput {
    egui::RawInput {
        screen_rect: Some(SCREEN),
        events,
        ..Default::default()
    }
}

fn click(pos: Pos2, b: PointerButton) -> Vec<Event> {
    let button = |pressed| Event::PointerButton {
        pos,
        button: b,
        pressed,
        modifiers: Modifiers::default(),
    };
    vec![Event::PointerMoved(pos), button(true), button(false)]
}

/// Mitte des ersten Textes, der mit `prefix` beginnt.
fn find_text(out: &egui::FullOutput, prefix: &str) -> Option<Pos2> {
    fn walk(shape: &egui::epaint::Shape, prefix: &str) -> Option<Pos2> {
        match shape {
            egui::epaint::Shape::Text(t) if t.galley.text().starts_with(prefix) => {
                Some(t.visual_bounding_rect().center())
            }
            egui::epaint::Shape::Vec(v) => v.iter().find_map(|s| walk(s, prefix)),
            _ => None,
        }
    }
    out.shapes.iter().find_map(|c| walk(&c.shape, prefix))
}

/// Ein Bild des Baums; liefert alle Texte und die Lage von `prefix`.
fn tree_frame(
    c: &egui::Context,
    s: &mut AppState,
    events: Vec<Event>,
    prefix: &str,
) -> (String, Option<Pos2>) {
    let out = c.run_ui(raw(events), |ui| browser::show(ui, s));
    let r = (
        planora_app::testing::collect_texts(&out),
        find_text(&out, prefix),
    );
    out.drop_without_applying_deltas();
    r
}

/// Rechtsklick auf die Zeile `row` des Baums; liefert die Texte des offenen Menüs und die Lage
/// von `entry` darin.
fn tree_menu(
    c: &egui::Context,
    s: &mut AppState,
    row: &str,
    entry: &str,
) -> (String, Option<Pos2>) {
    let (texts, pos) = tree_frame(c, s, vec![], row);
    let pos = pos.unwrap_or_else(|| panic!("Zeile {row}:\n{texts}"));
    tree_frame(c, s, click(pos, PointerButton::Secondary), "");
    tree_frame(c, s, vec![], entry)
}

/// Farben der Linien unter den Formen (durchgezogen oder Strichstücke).
fn stroke_colors(shapes: &[egui::Shape]) -> Vec<egui::Color32> {
    let mut out = Vec::new();
    for shape in shapes {
        match shape {
            egui::Shape::Path(p) => {
                if let egui::epaint::ColorMode::Solid(c) = p.stroke.color {
                    out.push(c);
                }
            }
            egui::Shape::LineSegment { stroke, .. } => out.push(stroke.color),
            egui::Shape::Vec(v) => out.extend(stroke_colors(v)),
            _ => {}
        }
    }
    out
}

/// Der Dialog zeigt ein Bild; liefert alle Texte.
fn dialog_frame(c: &egui::Context, s: &mut AppState) -> String {
    let mut text = String::new();
    for _ in 0..2 {
        let out = c.run_ui(raw(vec![]), |ui| {
            planora_app::lost_projections_dialog::show_open(ui.ctx(), s);
        });
        text = planora_app::testing::collect_texts(&out);
        out.drop_without_applying_deltas();
    }
    text
}

fn line_ends(s: &AppState, sketch: FeatureId, c: CurveId) -> (DVec2, DVec2) {
    let sk = s.document.sketch_of(sketch).unwrap();
    match sk.curve(c).unwrap().kind {
        CurveKind::Line { a, b } => (sk.point_xy(a).unwrap(), sk.point_xy(b).unwrap()),
        ref k => panic!("keine Linie: {k:?}"),
    }
}

#[test]
fn menu_entry_only_with_lost() {
    let mut r = rig(1);
    let sketch = r.sketch;
    // Ohne verlorene Projektion: kein Eintrag, weder in der Verlaufsleiste noch im Baum.
    let labels = history_bar::step_menu_labels(r.s(), sketch);
    assert!(
        labels.iter().any(|l| l == "Skizzenebene neu festlegen"),
        "{labels:?}"
    );
    assert!(!labels.iter().any(|l| l == MENU), "{labels:?}");
    let c = ctx();
    let (menu, _) = tree_menu(&c, r.s(), "Skizze 1", MENU);
    assert!(menu.contains("Skizzenebene neu festlegen"), "{menu}");
    assert!(!menu.contains(MENU), "{menu}");
    // Nach dem Löschen des Quaders: der Eintrag in beiden Menüs; ein Klick öffnet den Dialog.
    r.lose();
    let labels = history_bar::step_menu_labels(r.s(), sketch);
    assert!(labels.iter().any(|l| l == MENU), "{labels:?}");
    assert_eq!(labels.last().map(String::as_str), Some("Löschen"));
    let c = ctx();
    let (menu, item) = tree_menu(&c, r.s(), "Skizze 1", MENU);
    let item = item.unwrap_or_else(|| panic!("Menüeintrag:\n{menu}"));
    tree_frame(&c, r.s(), click(item, PointerButton::Primary), "");
    assert_eq!(
        r.s().lost_dialog.as_ref().map(|d| d.sketch),
        Some(sketch),
        "Dialog offen"
    );
    // Nicht an anderen Features.
    let b = r.b;
    assert!(
        !history_bar::step_menu_labels(r.s(), b)
            .iter()
            .any(|l| l == MENU)
    );
}

#[test]
fn action_opens_dialog_without_default_binding() {
    let mut r = rig(1);
    assert_eq!(
        Action::ManageLostProjections.id(),
        "modell.verlorene_projektionen"
    );
    assert!(
        r.w.keymap
            .bindings(Action::ManageLostProjections)
            .is_empty()
    );
    let sketch = r.sketch;
    r.s().selected_feature = Some(sketch);
    run_action(&mut r.w, Action::ManageLostProjections);
    assert!(
        r.s().lost_dialog.is_none(),
        "ohne verlorene Projektion nichts"
    );
    r.lose();
    r.s().selected_feature = Some(sketch);
    run_action(&mut r.w, Action::ManageLostProjections);
    assert_eq!(r.s().lost_dialog.as_ref().map(|d| d.sketch), Some(sketch));
}

#[test]
fn dialog_lists_lost_rows() {
    let mut r = rig(2);
    r.lose();
    let sketch = r.sketch;
    assert!(r.s().open_lost_projections(sketch));
    let d = r.s().lost_dialog.clone().unwrap();
    let rows = d.rows(r.s());
    let names: Vec<&str> = rows.iter().map(|x| x.name.as_str()).collect();
    assert_eq!(names, ["Linie 1", "Linie 2"]);
    assert_eq!(
        rows.iter().map(|x| x.target).collect::<Vec<_>>(),
        r.targets()
    );
    assert!(
        rows.iter().all(|x| x.relinkable),
        "Kanten lassen sich neu wählen"
    );
    let c = ctx();
    let text = dialog_frame(&c, r.s());
    for want in [
        MENU,
        "Linie 1",
        "Linie 2",
        "Neu verknüpfen",
        "Verknüpfung aufheben",
        "Geometrie löschen",
        "Alle löschen",
        "Alle Verknüpfungen aufheben",
        "Übrige Geometrie abblenden",
    ] {
        assert!(text.contains(want), "{want} fehlt:\n{text}");
    }
    assert!(r.s().lost_dialog.is_some(), "bleibt offen");
}

#[test]
fn unlink_row_is_one_undo_step() {
    let mut r = rig(2);
    r.lose();
    let sketch = r.sketch;
    r.s().open_lost_projections(sketch);
    let mut d = r.s().lost_dialog.clone().unwrap();
    let before = r.undo_len();
    let t = r.targets()[0];
    let c0 = r.curves[0];
    let ends = line_ends(r.s(), sketch, c0);
    assert_eq!(
        d.finish(r.s(), Some(LostAction::Unlink(t)), false, false, true),
        DialogOutcome::Open,
        "eine Zeile bleibt"
    );
    r.w.observe_history(false);
    assert_eq!(r.undo_len(), before + 1, "genau ein Schritt");
    assert_eq!(
        r.w.active_tab().undo_label().as_deref(),
        Some("Verknüpfung aufheben")
    );
    assert!(def(r.s(), sketch).copies.iter().all(|c| c.target != t));
    assert_eq!(line_ends(r.s(), sketch, c0), ends, "Kurve bleibt");
    assert_eq!(d.rows(r.s()).len(), 1);
    // Rückgängig holt die Verknüpfung zurück (die Kurve ist wieder verloren).
    r.w.active_tab_mut().undo().unwrap();
    assert!(def(r.s(), sketch).copies.iter().any(|c| c.target == t));
    assert_eq!(
        r.s()
            .document
            .feature(sketch)
            .unwrap()
            .lost_projections()
            .len(),
        2
    );
}

#[test]
fn delete_all_is_one_undo_step() {
    let mut r = rig(2);
    r.lose();
    let sketch = r.sketch;
    r.s().open_lost_projections(sketch);
    let mut d = r.s().lost_dialog.clone().unwrap();
    let before = r.undo_len();
    // Ohne Zeilen schließt der Dialog von selbst.
    assert_eq!(
        d.finish(r.s(), Some(LostAction::DeleteAll), false, false, true),
        DialogOutcome::Applied
    );
    r.w.observe_history(false);
    assert_eq!(r.undo_len(), before + 1, "genau ein Schritt");
    assert_eq!(
        r.w.active_tab().undo_label().as_deref(),
        Some("Geometrie löschen")
    );
    let (own, curves) = (r.own, r.curves.clone());
    let sk = r.s().document.sketch_of(sketch).unwrap();
    assert!(curves.iter().all(|c| sk.curve(*c).is_none()));
    assert!(sk.curve(own).is_some(), "die eigene Linie bleibt");
    assert!(def(r.s(), sketch).copies.is_empty());
    let f = r.s().document.feature(sketch).unwrap();
    assert!(f.warning().is_none(), "{:?}", f.warning());
}

#[test]
fn break_all_is_one_undo_step() {
    let mut r = rig(2);
    r.lose();
    let sketch = r.sketch;
    r.s().open_lost_projections(sketch);
    let mut d = r.s().lost_dialog.clone().unwrap();
    let before = r.undo_len();
    d.finish(r.s(), Some(LostAction::UnlinkAll), false, false, true);
    r.w.observe_history(false);
    assert_eq!(r.undo_len(), before + 1);
    assert!(def(r.s(), sketch).copies.is_empty());
    let curves = r.curves.clone();
    let sk = r.s().document.sketch_of(sketch).unwrap();
    assert!(
        curves.iter().all(|c| sk.curve(*c).is_some()),
        "Kurven bleiben"
    );
}

#[test]
fn dim_rest_dims_other_curves() {
    let mut r = rig(1);
    r.lose();
    let sketch = r.sketch;
    let col = theme::sketch_colors(r.s().dark);
    r.s().open_lost_projections(sketch);
    // Ohne Haken: die eigene Linie in voller Farbe.
    let colors = stroke_colors(&sketch_view::finished_sketch_shapes(r.s(), SCREEN));
    assert!(colors.contains(&col.lost), "{colors:?}");
    assert!(colors.contains(&col.finished), "{colors:?}");
    // Haken an: alles außer der verlorenen Kurve gedämpft.
    r.s().lost_dialog.as_mut().unwrap().dim_rest = true;
    let colors = stroke_colors(&sketch_view::finished_sketch_shapes(r.s(), SCREEN));
    assert!(colors.contains(&col.lost), "{colors:?}");
    assert!(!colors.contains(&col.finished), "{colors:?}");
    assert!(
        colors.contains(&col.finished.gamma_multiply(0.2)),
        "{colors:?}"
    );
    // In der offenen Skizze ebenso.
    let (c, own) = (r.curves[0], r.own);
    r.s().open_sketch(sketch).unwrap();
    assert_eq!(sketch_view::curve_color(r.s(), c), col.lost);
    assert_eq!(
        sketch_view::curve_color(r.s(), own),
        col.free.gamma_multiply(0.2)
    );
    // Dialog zu: nichts mehr gedämpft.
    r.s().lost_dialog = None;
    assert_eq!(sketch_view::curve_color(r.s(), own), col.free);
}

#[test]
fn relink_pick_then_click_edge() {
    let mut r = rig(1);
    r.lose();
    let sketch = r.sketch;
    r.s().open_lost_projections(sketch);
    let mut d = r.s().lost_dialog.clone().unwrap();
    let t = r.targets()[0];
    d.finish(r.s(), Some(LostAction::Relink(t)), false, false, true);
    assert_eq!(d.relink, Some(t));
    assert_eq!(r.s().open_sketch_id(), Some(sketch), "Skizze geöffnet");
    let pick = r
        .s()
        .sketch_session
        .as_ref()
        .unwrap()
        .relink
        .clone()
        .unwrap();
    assert_eq!(pick.target, t);
    assert!(matches!(pick.family, CopySource::Edge(_)));
    let steps = r.s().sketch_history().unwrap().undo_len();
    // Der Klick auf die Kante von B wird wie bei „Projizieren“ zum Ziel.
    let b = r.b;
    let ib = body_index(r.s(), b);
    let e = bottom_edge(r.s(), ib, 0.0);
    let target = sketch_view::relink_target(
        &pick.family,
        Some(Pick::Edge(EdgeRef { body: ib, edge: e })),
        None,
    );
    assert_eq!(target, Some(ProjectTarget::Edge(ib, e)));
    r.s().relink_pick(target.unwrap()).unwrap();
    r.w.observe_history(false);
    let c0 = r.curves[0];
    let s = r.s();
    assert_eq!(
        s.sketch_history().unwrap().undo_len(),
        steps + 1,
        "ein Schritt"
    );
    assert_eq!(s.sketch_undo_label().as_deref(), Some("Neu verknüpfen"));
    assert!(
        s.sketch_session.as_ref().unwrap().relink.is_none(),
        "Wahl beendet"
    );
    assert!(s.lost_rows(sketch).is_empty(), "Zeile weg");
    // Die Kurve liegt schon im Entwurf auf der Kante von B.
    let draft = s.sketch_draft.as_ref().unwrap();
    let CurveKind::Line { a, b: q } = draft.sketch.curve(c0).unwrap().kind else {
        panic!("Linie");
    };
    let xs = [
        draft.sketch.point_xy(a).unwrap().x,
        draft.sketch.point_xy(q).unwrap().x,
    ];
    assert!(
        xs.iter().all(|x| (30.0 - 1e-9..=40.0 + 1e-9).contains(x)),
        "{xs:?}"
    );
    // Beendet: verknüpft, keine Warnung mehr.
    s.close_sketch();
    let f = s.document.feature(sketch).unwrap();
    assert!(f.lost_projections().is_empty());
    assert!(f.warning().is_none(), "{:?}", f.warning());
}

#[test]
fn relink_wrong_pick_keeps_choice() {
    let mut r = rig(1);
    r.lose();
    let sketch = r.sketch;
    let t = r.targets()[0];
    r.s().start_relink(sketch, t).unwrap();
    let steps = r.s().sketch_history().unwrap().undo_len();
    let b = r.b;
    let ib = body_index(r.s(), b);
    let face = r.s().bodies[ib].solid.faces.iter().next().unwrap().0;
    // Eine Fläche passt nicht zu einer Kante: nichts geändert, die Wahl bleibt.
    assert!(r.s().relink_pick(ProjectTarget::Face(ib, face)).is_err());
    r.w.observe_history(false);
    assert_eq!(r.s().sketch_history().unwrap().undo_len(), steps);
    assert!(r.s().sketch_session.as_ref().unwrap().relink.is_some());
}

#[test]
fn relink_esc_leaves_nothing() {
    let mut r = rig(1);
    r.lose();
    let sketch = r.sketch;
    r.s().open_lost_projections(sketch);
    let mut d = r.s().lost_dialog.clone().unwrap();
    let t = r.targets()[0];
    d.finish(r.s(), Some(LostAction::Relink(t)), false, false, true);
    let steps = r.s().sketch_history().unwrap().undo_len();
    {
        let (se, sk) = r.s().session_sketch_mut().unwrap();
        assert!(se.relink.is_some());
        // Esc (wie in der Ansicht): nimmt nur die Wahl zurück.
        se.cancel(sk);
        assert!(se.relink.is_none());
    }
    r.w.observe_history(false);
    assert_eq!(
        r.s().sketch_history().unwrap().undo_len(),
        steps,
        "nichts im Verlauf"
    );
    assert_eq!(r.s().lost_rows(sketch), vec![t], "noch verloren");
    // Die Zeile der Wahl gilt nicht mehr; der Dialog bleibt offen.
    assert_eq!(
        d.finish(r.s(), None, false, false, true),
        DialogOutcome::Open
    );
    assert_eq!(d.relink, None);
    // Ein Schließen während der Wahl nimmt sie ebenfalls ohne Spur zurück.
    d.finish(r.s(), Some(LostAction::Relink(t)), false, false, true);
    assert_eq!(
        d.finish(r.s(), None, false, true, true),
        DialogOutcome::Cancelled
    );
    assert!(r.s().sketch_session.as_ref().unwrap().relink.is_none());
    r.w.observe_history(false);
    assert_eq!(r.s().sketch_history().unwrap().undo_len(), steps);
}

#[test]
fn delete_body_dialog_no_longer_names_sketch() {
    let mut r = rig(1);
    let a = r.a;
    let plan = browser::delete_plan(r.s(), a);
    assert!(plan.dependents.is_empty(), "{:?}", plan.dependents);
    assert!(!plan.needs_confirmation());
    assert!(!plan.text().contains("Skizze 1"), "{}", plan.text());
}

#[test]
fn lost_curve_drawn_yellow() {
    let mut r = rig(1);
    let col = theme::sketch_colors(r.s().dark);
    let sketch = r.sketch;
    // Verknüpft: lila, nicht gelb.
    let colors = stroke_colors(&sketch_view::finished_sketch_shapes(r.s(), SCREEN));
    assert!(colors.contains(&col.projected), "{colors:?}");
    assert!(!colors.contains(&col.lost), "{colors:?}");
    r.lose();
    // Verloren: gelb wie die Warnung, in der 3D-Ansicht und in der offenen Skizze.
    assert_eq!(col.lost, theme::palette(r.s().dark).warning);
    let colors = stroke_colors(&sketch_view::finished_sketch_shapes(r.s(), SCREEN));
    assert!(colors.contains(&col.lost), "{colors:?}");
    assert!(!colors.contains(&col.projected), "{colors:?}");
    let c = r.curves[0];
    r.s().open_sketch(sketch).unwrap();
    assert_eq!(sketch_view::curve_color(r.s(), c), col.lost);
    let ends = r
        .s()
        .sketch_draft
        .as_ref()
        .unwrap()
        .sketch
        .curve(c)
        .unwrap()
        .kind
        .points();
    for p in ends {
        assert_eq!(sketch_view::point_color(r.s(), p), col.lost);
    }
    // Ausgewählt hat die Auswahl Vorrang.
    r.s().sketch_session.as_mut().unwrap().selection =
        vec![planora_app::sketch_state::Item::Curve(c)];
    assert_eq!(sketch_view::curve_color(r.s(), c), col.selected);
}

#[test]
fn dialog_has_dock_key() {
    assert!(DIALOG_KEYS.contains(&"lost_projections"));
    let d = LostProjectionsDialog::new(FeatureId(1));
    assert!(!d.dim_rest);
    assert_eq!(d.relink, None);
}

// ---- Ruling T18-1: „Neu verknüpfen“ für eingezogene Ecken, Ebenen, Achsen und den Ursprung ----

/// Quader A (am Ursprung) und B (bei x = 30), dahinter „Skizze 1“ auf XY, offen, mit der über
/// „3D-Geometrie einbeziehen“ eingezogenen Wahl `pick` (von `pick_of`, mit dem Index von A).
/// Liefert Arbeitsbereich, A, B, Skizze und die Ziele der neuen Kopien.
fn included(
    pick_of: impl Fn(&AppState, usize) -> IncludePick,
) -> (Workspace, FeatureId, FeatureId, FeatureId, Vec<CopyTarget>) {
    let mut w = Workspace::new(None, Config::default());
    let s = &mut w.active_tab_mut().state;
    let a = s.create_box(DVec3::new(10.0, 20.0, 5.0)).unwrap();
    let b = s
        .document
        .add_feature(
            "Quader B",
            FeatureKind::Box {
                size: DVec3::new(10.0, 20.0, 5.0),
                origin: DVec3::new(30.0, 0.0, 0.0),
            },
        )
        .unwrap();
    s.refresh();
    let sketch = s
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    s.open_sketch(sketch).unwrap();
    s.observe_sketch(false);
    let ia = body_index(s, a);
    let pick = pick_of(s, ia);
    s.include_into_sketch(pick).unwrap();
    s.observe_sketch(false);
    let targets = s
        .sketch_draft
        .as_ref()
        .unwrap()
        .copies
        .iter()
        .map(|c| c.target)
        .collect();
    w.observe_history(false);
    (w, a, b, sketch, targets)
}

fn vertex_of(s: &AppState, body: usize) -> IncludePick {
    IncludePick::Vertex {
        body,
        edge: bottom_edge(s, body, 0.0),
        end: false,
    }
}

fn draft_sources(s: &AppState) -> Vec<CopySource> {
    s.sketch_draft
        .as_ref()
        .unwrap()
        .copies
        .iter()
        .map(|c| c.source.clone())
        .collect()
}

fn steps(s: &AppState) -> usize {
    s.sketch_history().unwrap().undo_len()
}

#[test]
fn relink_lost_vertex_to_other_corner() {
    let (mut w, a, b, sketch, targets) = included(vertex_of);
    assert!(matches!(targets[..], [CopyTarget::Point(_)]));
    let s = &mut w.active_tab_mut().state;
    s.close_sketch();
    s.delete_feature(a).unwrap();
    assert_eq!(s.lost_rows(sketch), targets, "Ecke verloren");
    s.open_lost_projections(sketch);
    let rows = s.lost_dialog.clone().unwrap().rows(s);
    assert!(rows[0].relinkable, "Ecken lassen sich neu wählen");
    s.start_relink(sketch, targets[0]).unwrap();
    let before = steps(s);
    let ib = body_index(s, b);
    let pick = vertex_of(s, ib);
    s.relink_include_pick(pick).unwrap();
    s.observe_sketch(false);
    assert_eq!(steps(s), before + 1, "ein Schritt");
    assert_eq!(s.sketch_undo_label().as_deref(), Some("Neu verknüpfen"));
    let CopyTarget::Point(p) = targets[0] else {
        unreachable!()
    };
    let at = s.sketch_draft.as_ref().unwrap().sketch.point_xy(p).unwrap();
    assert!(
        [30.0, 40.0].iter().any(|x| (at.x - x).abs() < 1e-9) && at.y.abs() < 1e-9,
        "auf einer Ecke von B: {at:?}"
    );
    s.close_sketch();
    assert!(
        s.document
            .feature(sketch)
            .unwrap()
            .lost_projections()
            .is_empty()
    );
}

#[test]
fn relink_plane_to_other_plane() {
    use planora_feature::{IncludeRef, PlaneId};
    let (mut w, _, _, sketch, targets) =
        included(|_, _| IncludePick::Plane(PlaneId::Standard(StandardPlane::XZ)));
    assert_eq!(targets.len(), 4, "vier Randlinien");
    let s = &mut w.active_tab_mut().state;
    s.start_relink(sketch, targets[0]).unwrap();
    let before = steps(s);
    s.relink_include_pick(IncludePick::Plane(PlaneId::Standard(StandardPlane::YZ)))
        .unwrap();
    s.observe_sketch(false);
    assert_eq!(steps(s), before + 1, "ein Schritt für die ganze Gruppe");
    let yz = CopySource::Include(IncludeRef::Plane(PlaneId::Standard(StandardPlane::YZ)));
    assert!(
        draft_sources(s).iter().all(|src| *src == yz),
        "{:?}",
        draft_sources(s)
    );
    // Alle vier Linien liegen jetzt in der YZ-Ebene (x = 0).
    let sk = &s.sketch_draft.as_ref().unwrap().sketch;
    for t in &targets {
        let CopyTarget::Curve(c) = *t else { panic!() };
        for p in sk.curve(c).unwrap().kind.points() {
            assert!(sk.point_xyz(p).unwrap().x.abs() < 1e-9);
        }
    }
}

#[test]
fn relink_axis_to_other_axis() {
    use planora_feature::{AxisId, IncludeRef};
    let (mut w, _, _, sketch, targets) = included(|_, _| IncludePick::Axis(AxisId::X));
    let s = &mut w.active_tab_mut().state;
    s.start_relink(sketch, targets[0]).unwrap();
    let before = steps(s);
    s.relink_include_pick(IncludePick::Axis(AxisId::Y)).unwrap();
    s.observe_sketch(false);
    assert_eq!(steps(s), before + 1);
    assert_eq!(
        draft_sources(s),
        vec![CopySource::Include(IncludeRef::Axis(AxisId::Y))]
    );
    let sk = &s.sketch_draft.as_ref().unwrap().sketch;
    let CopyTarget::Curve(c) = targets[0] else {
        panic!()
    };
    for p in sk.curve(c).unwrap().kind.points() {
        assert!(sk.point_xyz(p).unwrap().x.abs() < 1e-9, "auf der y-Achse");
    }
}

#[test]
fn relink_origin_refuses_other_kind() {
    use planora_feature::IncludeRef;
    let (mut w, _, b, sketch, targets) = included(|_, _| IncludePick::Origin);
    let s = &mut w.active_tab_mut().state;
    s.start_relink(sketch, targets[0]).unwrap();
    let before = steps(s);
    // Eine Ecke ist eine andere Art als der Ursprung: nichts geändert, die Wahl bleibt.
    let ib = body_index(s, b);
    let pick = vertex_of(s, ib);
    assert!(s.relink_include_pick(pick).is_err());
    assert!(s.sketch_session.as_ref().unwrap().relink.is_some());
    // Der Ursprung selbst passt (gleiche Herkunft: kein Schritt).
    s.relink_include_pick(IncludePick::Origin).unwrap();
    s.observe_sketch(false);
    assert_eq!(steps(s), before);
    assert_eq!(
        draft_sources(s),
        vec![CopySource::Include(IncludeRef::Origin)]
    );
    assert!(s.sketch_session.as_ref().unwrap().relink.is_none());
}

#[test]
fn surface_kinds_stay_locked() {
    use planora_app::state::relink_pickable;
    use planora_feature::{IncludeRef, IntersectionSpec, PlaneId, ProjectType, SurfaceProjection};
    let on = CopySource::OnSurface(SurfaceProjection {
        sources: Vec::new(),
        faces: Vec::new(),
        kind: ProjectType::AlongVector(DVec3::Z),
    });
    let cut = CopySource::Intersection(IntersectionSpec {
        curves: Vec::new(),
        targets: Vec::new(),
    });
    assert!(!relink_pickable(&on));
    assert!(!relink_pickable(&cut));
    assert!(!relink_pickable(&CopySource::Include(
        IncludeRef::ConstructionPoint(FeatureId(1))
    )));
    for r in [
        IncludeRef::Origin,
        IncludeRef::Axis(planora_feature::AxisId::Z),
        IncludeRef::Plane(PlaneId::Standard(StandardPlane::XY)),
    ] {
        assert!(relink_pickable(&CopySource::Include(r)));
    }
}
