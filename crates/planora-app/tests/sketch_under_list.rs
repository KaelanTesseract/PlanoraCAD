//! „Unterbestimmte finden“ und „Skizze berechnen“ (Skizzen 11a, Task 16): Liste im Skizzenfeld,
//! Zeilenklick, X, Dämpfen und Berechnen. Ohne Fenster.

mod common;

use planora_app::commands::run_action;
use planora_app::keymap::Action;
use planora_app::sketch_panel;
use planora_app::sketch_state::{Item, Tool};
use planora_app::sketch_under::{FocusMove, focus_move};
use planora_app::sketch_view;
use planora_app::state::{AppState, StandardPlane};
use planora_app::theme;
use planora_app::workspace::Workspace;
use planora_feature::SketchPlane;
use planora_math::DVec2;
use planora_project::config::Config;
use planora_project::library::Library;
use planora_sketch::check::{CheckItem, SketchStatus};
use planora_sketch::constraint::{ConstraintKind as K, LineOrPoints};
use planora_sketch::model::{CurveId, PointId};

fn ws() -> Workspace {
    let root = common::temp_root("planora-sketch-under");
    Workspace::new(Some(Library::open(&root).unwrap()), Config::default())
}

fn state(w: &mut Workspace) -> &mut AppState {
    &mut w.active_tab_mut().state
}

/// Offene Skizze mit einer freien Linie und einem freien Punkt.
fn open_line_and_point(w: &mut Workspace) -> (CurveId, PointId) {
    let s = state(w);
    let id = s
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    s.open_sketch(id).unwrap();
    let (se, sk) = s.session_sketch_mut().unwrap();
    let a = sk.add_point(0.0, 0.0).unwrap();
    let b = sk.add_point(5.0, 1.0).unwrap();
    let line = sk.add_line(a, b).unwrap();
    let lone = sk.add_point(20.0, 20.0).unwrap();
    se.last_report = Some(sk.solve());
    w.observe_history(false);
    (line, lone)
}

fn names(w: &mut Workspace) -> Vec<String> {
    let (se, sk) = state(w).session_sketch_mut().unwrap();
    se.under_rows(sk).into_iter().map(|r| r.1).collect()
}

fn undo_len(w: &Workspace) -> usize {
    w.active_tab().state.sketch_history().unwrap().undo_len()
}

#[test]
fn list_shows_free_elements_with_names() {
    let mut w = ws();
    let (line, lone) = open_line_and_point(&mut w);
    assert_eq!(names(&mut w), ["Linie 1", "Punkt 1"]);
    let (se, sk) = state(&mut w).session_sketch_mut().unwrap();
    let rows = se.under_rows(sk);
    assert_eq!(rows[0].0, CheckItem::Curve(line));
    assert_eq!(rows[1].0, CheckItem::Point(lone));
    // Punkte haben kein X (Ruling F5b), Linien schon.
    assert_eq!((rows[0].2, rows[1].2), (true, false));
}

#[test]
fn row_click_selects_only_that_element() {
    let mut w = ws();
    let (line, lone) = open_line_and_point(&mut w);
    let (se, sk) = state(&mut w).session_sketch_mut().unwrap();
    se.selection = vec![Item::Point(lone), Item::Curve(line)];
    assert!(se.focus_item(sk, CheckItem::Curve(line)));
    assert_eq!(se.selection, vec![Item::Curve(line)]);
    assert!(se.focus_item(sk, CheckItem::Point(lone)));
    assert_eq!(se.selection, vec![Item::Point(lone)]);
}

fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> (DVec2, DVec2) {
    (DVec2::new(x0, y0), DVec2::new(x1, y1))
}

#[test]
fn row_click_pans_or_zooms() {
    let view = rect(0.0, 0.0, 100.0, 100.0);
    // Mit Rand sichtbar: nichts.
    assert_eq!(
        focus_move(view, rect(40.0, 40.0, 60.0, 60.0), 0.15),
        FocusMove::Keep
    );
    // Im Bild, aber im Rand: passt in der Größe, also verschieben (Mitte des Elements in die Mitte).
    assert_eq!(
        focus_move(view, rect(2.0, 40.0, 12.0, 60.0), 0.15),
        FocusMove::Pan(DVec2::new(-43.0, 0.0))
    );
    // Außerhalb, klein: verschieben.
    assert_eq!(
        focus_move(view, rect(250.0, 40.0, 260.0, 50.0), 0.15),
        FocusMove::Pan(DVec2::new(205.0, -5.0))
    );
    // Zu groß für die Ansicht: zoomen.
    match focus_move(view, rect(-100.0, 0.0, 300.0, 50.0), 0.15) {
        FocusMove::Zoom {
            center,
            half_height,
        } => {
            assert_eq!(center, DVec2::new(100.0, 25.0));
            // Breite 400 mit Rand 30 % auf 100 mm Höhe und gleiches Seitenverhältnis: 400/0,7.
            assert!(
                (half_height - 400.0 / 0.7 / 2.0).abs() < 1e-9,
                "{half_height}"
            );
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn row_click_moves_the_camera_of_the_state() {
    let mut w = ws();
    let (_, _) = open_line_and_point(&mut w);
    let s = state(&mut w);
    let (se, sk) = s.session_sketch_mut().unwrap();
    let a = sk.add_point(900.0, 0.0).unwrap();
    let b = sk.add_point(905.0, 3.0).unwrap();
    let far = sk.add_line(a, b).unwrap();
    se.last_report = Some(sk.solve());
    let before = s.camera.target;
    s.focus_under_item(CheckItem::Curve(far));
    assert!(
        s.camera.target.x > before.x + 100.0,
        "{:?}",
        s.camera.target
    );
    let (se, _) = s.session_sketch_mut().unwrap();
    assert_eq!(se.selection, vec![Item::Curve(far)]);
}

#[test]
fn x_deletes_one_undo_step() {
    let mut w = ws();
    let (line, _) = open_line_and_point(&mut w);
    let n = undo_len(&w);
    let (se, sk) = state(&mut w).session_sketch_mut().unwrap();
    assert!(se.delete_under_item(sk, CheckItem::Curve(line)));
    assert!(sk.curve(line).is_none());
    w.observe_history(false);
    assert_eq!(undo_len(&w), n + 1);
}

#[test]
fn x_is_refused_for_a_lone_point() {
    let mut w = ws();
    let (_, lone) = open_line_and_point(&mut w);
    let (se, sk) = state(&mut w).session_sketch_mut().unwrap();
    assert!(!se.delete_under_item(sk, CheckItem::Point(lone)));
    assert!(sk.point_xy(lone).is_ok());
}

#[test]
fn under_list_follows_delete_and_undo() {
    let mut w = ws();
    let (line, _) = open_line_and_point(&mut w);
    assert_eq!(names(&mut w), ["Linie 1", "Punkt 1"]);
    {
        let (se, sk) = state(&mut w).session_sketch_mut().unwrap();
        assert!(se.delete_under_item(sk, CheckItem::Curve(line)));
        // Gleich nach dem Löschen steht der Bericht der Skizze noch im Bild; ein Zeilenklick darauf
        // ist harmlos.
        se.last_report = Some(sk.solve());
    }
    w.observe_history(false);
    assert_eq!(names(&mut w), ["Punkt 1"], "X: Zeile weg");
    // Veraltetes Element: Klick und X tun nichts.
    {
        let (se, sk) = state(&mut w).session_sketch_mut().unwrap();
        se.selection.clear();
        assert!(!se.focus_item(sk, CheckItem::Curve(line)));
        assert!(se.selection.is_empty());
        assert!(!se.delete_under_item(sk, CheckItem::Curve(line)));
    }
    // Rückgängig: Die Zeile ist wieder da.
    assert!(state(&mut w).undo_sketch());
    assert_eq!(
        names(&mut w),
        ["Linie 1", "Punkt 1"],
        "Rückgängig: Zeile da"
    );
    // Das Element lebt wieder, der Klick wirkt.
    let (se, sk) = state(&mut w).session_sketch_mut().unwrap();
    let again = se.under_rows(sk)[0].0;
    assert!(se.focus_item(sk, again));
}

#[test]
fn list_stays_open_while_dimensioning() {
    let mut w = ws();
    let s = state(&mut w);
    let id = s
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    s.open_sketch(id).unwrap();
    let (se, sk) = s.session_sketch_mut().unwrap();
    let a = sk.add_point(0.0, 0.0).unwrap();
    let b = sk.add_point(5.0, 0.0).unwrap();
    sk.add_line(a, b).unwrap();
    se.last_report = Some(sk.solve());
    se.open_under_list();
    se.tool = Tool::Dimension;
    assert_eq!(se.under_rows(sk).len(), 1);
    let status = |sk: &planora_sketch::model::Sketch,
                  se: &planora_app::sketch_state::SketchSession| {
        sk.status(se.last_report.as_ref().unwrap())
    };
    assert_eq!(status(sk, se), SketchStatus::Under);
    // Lage und Länge setzen, wie es die Maße des Werkzeugs tun.
    sk.add_constraint(K::Fixed(a, 0.0, 0.0)).unwrap();
    sk.add_constraint(K::Horizontal(LineOrPoints::Points(a, b)))
        .unwrap();
    sk.add_constraint(K::Distance(a, b, 5.0)).unwrap();
    se.last_report = Some(sk.solve());
    assert!(se.under_list.is_some(), "Liste bleibt offen");
    assert_eq!(se.tool, Tool::Dimension);
    assert!(se.under_rows(sk).is_empty(), "Zeile der Linie weg");
    assert_eq!(status(sk, se), SketchStatus::Determined);
    let (text, _) = sketch_panel::status_line(sk, se.last_report.as_ref().unwrap());
    assert_eq!(text, "Status: Bestimmt");
}

#[test]
fn hide_determined_dims_geometry() {
    let mut s = AppState::new();
    let id = s
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    s.open_sketch(id).unwrap();
    let (se, sk) = s.session_sketch_mut().unwrap();
    let a = sk.add_point(0.0, 0.0).unwrap();
    let b = sk.add_point(5.0, 0.0).unwrap();
    let fixed = sk.add_line(a, b).unwrap();
    sk.add_constraint(K::Fixed(a, 0.0, 0.0)).unwrap();
    sk.add_constraint(K::Horizontal(LineOrPoints::Points(a, b)))
        .unwrap();
    sk.add_constraint(K::Distance(a, b, 5.0)).unwrap();
    let c = sk.add_point(0.0, 9.0).unwrap();
    let d = sk.add_point(5.0, 8.0).unwrap();
    let free = sk.add_line(c, d).unwrap();
    se.last_report = Some(sk.solve());
    se.open_under_list();
    let col = theme::sketch_colors(s.dark);
    assert_eq!(sketch_view::curve_color(&s, fixed), col.determined);
    let (se, _) = s.session_sketch_mut().unwrap();
    se.under_list.as_mut().unwrap().hide_determined = true;
    let dimmed = sketch_view::curve_color(&s, fixed);
    assert_ne!(dimmed, col.determined);
    assert!(dimmed.a() < col.determined.a() / 2, "{dimmed:?}");
    // Freie Geometrie bleibt, wie sie ist.
    assert_eq!(sketch_view::curve_color(&s, free), col.free);
}

#[test]
fn compute_sketch_no_step_without_change() {
    let mut w = ws();
    let s = state(&mut w);
    let id = s
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    s.open_sketch(id).unwrap();
    let (se, sk) = s.session_sketch_mut().unwrap();
    let a = sk.add_point(0.0, 0.0).unwrap();
    let b = sk.add_point(5.0, 0.0).unwrap();
    sk.add_line(a, b).unwrap();
    sk.add_constraint(K::Fixed(a, 0.0, 0.0)).unwrap();
    sk.add_constraint(K::Horizontal(LineOrPoints::Points(a, b)))
        .unwrap();
    sk.add_constraint(K::Distance(a, b, 5.0)).unwrap();
    se.last_report = Some(sk.solve());
    w.observe_history(false);
    let n = undo_len(&w);
    let (se, sk) = state(&mut w).session_sketch_mut().unwrap();
    se.compute_sketch(sk);
    assert_eq!(se.last_report.as_ref().unwrap().dof, 0);
    w.observe_history(false);
    assert_eq!(undo_len(&w), n, "ohne Änderung kein Schritt");
    assert!(
        state(&mut w)
            .session_sketch_mut()
            .unwrap()
            .0
            .edit_label
            .is_none()
    );
    // Wurde ein Wert bei der Eingabe verstellt, ohne zu lösen, ändert das Berechnen ihn: ein Schritt.
    let (se, sk) = state(&mut w).session_sketch_mut().unwrap();
    sk.set_point_xy(b, 6.0, 1.0).unwrap();
    se.last_report = None;
    w.observe_history(false);
    let n = undo_len(&w);
    let (se, sk) = state(&mut w).session_sketch_mut().unwrap();
    se.compute_sketch(sk);
    w.observe_history(false);
    assert_eq!(undo_len(&w), n + 1);
    let (_, sk) = state(&mut w).session_sketch_mut().unwrap();
    assert!(sk.point_xy(b).unwrap().distance(DVec2::new(5.0, 0.0)) < 1e-6);
}

#[test]
fn list_closes_when_leaving_sketch() {
    let mut w = ws();
    open_line_and_point(&mut w);
    let s = state(&mut w);
    let (se, _) = s.session_sketch_mut().unwrap();
    se.open_under_list();
    assert!(se.under_list.is_some());
    let id = s.open_sketch_id().unwrap();
    s.close_sketch();
    assert!(s.session_sketch_mut().is_none());
    s.open_sketch(id).unwrap();
    let (se, _) = s.session_sketch_mut().unwrap();
    assert!(se.under_list.is_none(), "neue Sitzung, Liste zu");
}

#[test]
fn actions_have_no_default_binding() {
    let mut w = ws();
    assert!(
        w.keymap
            .bindings(Action::SketchFindUnderConstrained)
            .is_empty()
    );
    assert!(w.keymap.bindings(Action::SketchCompute).is_empty());
    assert_eq!(
        Action::SketchFindUnderConstrained.id(),
        "skizze.unterbestimmte_finden"
    );
    assert_eq!(Action::SketchCompute.id(), "skizze.berechnen");
    open_line_and_point(&mut w);
    run_action(&mut w, Action::SketchFindUnderConstrained);
    assert!(
        state(&mut w)
            .session_sketch_mut()
            .unwrap()
            .0
            .under_list
            .is_some()
    );
    run_action(&mut w, Action::SketchCompute);
    assert!(
        state(&mut w)
            .session_sketch_mut()
            .unwrap()
            .0
            .last_report
            .is_some()
    );
}

/// Messtest (Release): 500 freie Bögen, wie eine eingefügte DXF-Bogenkette.
/// `cargo test -p planora-app --release --test sketch_under_list -- --ignored --nocapture`
#[test]
#[ignore = "Messtest, nur im Release"]
fn under_rows_of_500_arcs_stay_fast() {
    let mut w = ws();
    let s = state(&mut w);
    let id = s
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    s.open_sketch(id).unwrap();
    let (se, sk) = s.session_sketch_mut().unwrap();
    for i in 0..500 {
        let x = f64::from(i) * 30.0;
        let m = sk.add_point(x, 0.0).unwrap();
        let a = sk.add_point(x + 10.0, 0.0).unwrap();
        let b = sk.add_point(x, 10.0).unwrap();
        sk.add_arc(m, a, b).unwrap();
    }
    se.last_report = Some(sk.solve());
    assert_eq!(se.under_rows(sk).len(), 500);
    let runs = 20;
    let t = std::time::Instant::now();
    for _ in 0..runs {
        assert_eq!(se.under_rows(sk).len(), 500);
    }
    let ms = t.elapsed().as_secs_f64() * 1000.0 / f64::from(runs);
    println!("under_rows (500 Bögen): {ms:.3} ms");
    assert!(ms < 2.0, "{ms} ms");
}

/// Lage (Mitte, y) des ersten Textes `text` im Bild.
fn text_y(out: &egui::FullOutput, text: &str) -> Option<f32> {
    fn walk(shape: &egui::epaint::Shape, text: &str) -> Option<f32> {
        match shape {
            egui::epaint::Shape::Text(t) if t.galley.text() == text => {
                Some(t.visual_bounding_rect().center().y)
            }
            egui::epaint::Shape::Vec(v) => v.iter().find_map(|s| walk(s, text)),
            _ => None,
        }
    }
    out.shapes.iter().find_map(|c| walk(&c.shape, text))
}

/// Offene Skizze mit `n` freien Linien und offener Liste; liefert den Abstand von der letzten
/// Zeile („Linie n“) zur Überschrift darunter („Abhängigkeiten“) im Skizzenfeld.
fn gap_below_list(n: u32) -> f32 {
    let ctx = egui::Context::default();
    ctx.set_fonts(planora_app::fonts::definitions());
    theme::apply(&ctx, true);
    let mut w = ws();
    let s = state(&mut w);
    let id = s
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    s.open_sketch(id).unwrap();
    {
        let (se, sk) = s.session_sketch_mut().unwrap();
        for i in 0..n {
            let y = f64::from(i) * 3.0;
            let a = sk.add_point(0.0, y).unwrap();
            let b = sk.add_point(10.0, y + 1.0).unwrap();
            sk.add_line(a, b).unwrap();
        }
        se.last_report = Some(sk.solve());
        se.open_under_list();
    }
    let raw = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1920.0, 1080.0),
        )),
        ..Default::default()
    };
    let mut gap = None;
    for _ in 0..3 {
        let out = ctx.run_ui(raw.clone(), |ui| sketch_panel::show(ui.ctx(), s));
        let last = text_y(&out, &format!("Linie {n}"));
        let below = text_y(&out, "Abhängigkeiten");
        gap = last.zip(below).map(|(a, b)| b - a);
        out.drop_without_applying_deltas();
    }
    gap.expect("letzte Zeile und Überschrift zu sehen")
}

/// Task 16, Mitnahme in Task 18: Die Zeilenhöhe für `show_rows` zählt den Abstand nicht doppelt
/// (`show_rows` addiert `item_spacing` selbst). Sonst wächst die Liste je Zeile um einen Abstand
/// zu hoch (bei 500 Zeilen rund 2000 Pixel Leerraum): Unter der letzten Zeile bleibt dann umso
/// mehr Platz, je mehr Zeilen die Liste hat.
#[test]
fn under_list_rows_count_spacing_once() {
    let (two, three) = (gap_below_list(1), gap_below_list(2));
    assert!(
        (three - two).abs() < 0.5,
        "1 Zeile: {two}, 2 Zeilen: {three}"
    );
}

/// Handtest 2, H2-2: Das X an einer Linie löscht wie die Entf-Taste: Endpunkte und Maße der Linie
/// gehen mit, ein Rückgängig-Schritt.
#[test]
fn x_on_a_line_removes_its_points_and_dimensions_like_delete() {
    let mut w = ws();
    let (line, lone) = open_line_and_point(&mut w);
    // Längenmaß an der Linie.
    {
        let (se, sk) = state(&mut w).session_sketch_mut().unwrap();
        let (a, b) = {
            let pts = sk.curve(line).unwrap().kind.points();
            (pts[0], pts[1])
        };
        sk.add_constraint(K::Distance(a, b, 5.0)).unwrap();
        se.last_report = Some(sk.solve());
    }
    w.observe_history(false);
    let before = undo_len(&w);
    let (se, sk) = state(&mut w).session_sketch_mut().unwrap();
    assert!(se.delete_under_item(sk, CheckItem::Curve(line)));
    assert_eq!(sk.constraints().count(), 0, "Maß muss mit");
    let points: Vec<_> = sk.points().map(|p| p.0).collect();
    assert_eq!(points, vec![lone], "nur der freie Punkt bleibt");
    w.observe_history(false);
    assert_eq!(undo_len(&w), before + 1);
}
