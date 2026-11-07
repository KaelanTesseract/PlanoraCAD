//! Skizzen 6, Task 8: projizierte Geometrie lila (wie Fusion), „Verknüpfung aufheben“ als ein
//! Rückgängig-Schritt, Aufräumen gelöschter Projektionen beim Beenden und die Warnung bei
//! verlorenem Bezug in Browser und Verlaufsleiste.

use planora_app::history_bar::{self, StepState};
use planora_app::keymap::Action;
use planora_app::sketch_state::Item;
use planora_app::state::{AppState, StandardPlane};
use planora_app::{browser, sketch_view, theme};
use planora_feature::{FeatureId, FeatureKind, SketchFeature, SketchPlane};
use planora_i18n::tr;
use planora_math::{DVec2, DVec3};
use planora_sketch::model::CurveId;

const SCREEN: egui::Rect = egui::Rect {
    min: egui::pos2(0.0, 0.0),
    max: egui::pos2(1920.0, 1080.0),
};

/// Quader und eine offene Skizze auf XY mit einer projizierten Kante; liefert Skizze und Kurve.
fn projected() -> (AppState, FeatureId, CurveId) {
    let mut s = AppState::new();
    s.create_box(DVec3::new(10.0, 20.0, 5.0)).unwrap();
    let id = s
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    s.open_sketch(id).unwrap();
    s.observe_sketch(false);
    let solid = s.bodies[0].solid.clone();
    solid
        .edges
        .iter()
        .map(|(e, _)| e)
        .find(|e| s.project_edge_into_sketch(0, *e).is_ok())
        .expect("eine Kante lässt sich projizieren");
    s.observe_sketch(false);
    let curve = draft(&s).copies[0].curve;
    (s, id, curve)
}

fn draft(s: &AppState) -> &SketchFeature {
    s.sketch_draft.as_ref().unwrap()
}

fn def(s: &AppState, id: FeatureId) -> &SketchFeature {
    match &s.document.feature(id).unwrap().kind {
        FeatureKind::Sketch(sf) => sf,
        k => panic!("keine Skizze: {k:?}"),
    }
}

fn select(s: &mut AppState, items: &[Item]) {
    s.sketch_session.as_mut().unwrap().selection = items.to_vec();
}

fn undo_len(s: &AppState) -> usize {
    s.sketch_history().unwrap().undo_len()
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

#[test]
fn projected_curve_uses_projected_color() {
    let (mut s, id, c) = projected();
    let col = theme::sketch_colors(s.dark);
    assert_eq!(col.projected, theme::BODY_COLORS[3].get(s.dark));
    assert_eq!(sketch_view::curve_color(&s, c), col.projected);
    // Die Punkte der projizierten Kurve sind auch lila (wie Fusion).
    let points = draft(&s).sketch.curve(c).unwrap().kind.points();
    assert!(!points.is_empty());
    for p in &points {
        assert_eq!(sketch_view::point_color(&s, *p), col.projected);
    }
    // Als Konstruktion bleibt sie lila (gestrichelt), gewählt hat die Auswahl Vorrang.
    s.session_sketch_mut()
        .unwrap()
        .1
        .set_construction(c, true)
        .unwrap();
    assert_eq!(sketch_view::curve_color(&s, c), col.projected);
    select(&mut s, &[Item::Curve(c)]);
    assert_eq!(sketch_view::curve_color(&s, c), col.selected);
    select(&mut s, &[]);
    s.session_sketch_mut()
        .unwrap()
        .1
        .set_construction(c, false)
        .unwrap();
    // Eine eigene Linie ist nicht lila.
    let own = {
        let sk = s.session_sketch_mut().unwrap().1;
        let a = sk.add_point(40.0, 40.0).unwrap();
        let b = sk.add_point(50.0, 45.0).unwrap();
        sk.add_line(a, b).unwrap()
    };
    assert_ne!(sketch_view::curve_color(&s, own), col.projected);
    // Fertige Skizze in der 3D-Ansicht: die Projektion lila, die eigene Linie nicht.
    s.close_sketch();
    assert!(s.open_sketch_id().is_none());
    assert_eq!(def(&s, id).copies.len(), 1);
    let colors = stroke_colors(&sketch_view::finished_sketch_shapes(&s, SCREEN));
    assert!(colors.contains(&col.projected), "{colors:?}");
    assert!(colors.contains(&col.finished), "{colors:?}");
}

#[test]
fn break_link_is_one_undo_step() {
    let (mut s, id, c) = projected();
    let before = undo_len(&s);
    s.break_projection_link(c).unwrap();
    assert!(draft(&s).copies.is_empty(), "Verweis gelöst");
    assert!(draft(&s).sketch.curve(c).is_some(), "die Kurve bleibt");
    assert_eq!(undo_len(&s), before + 1, "genau ein Schritt");
    assert_eq!(
        s.sketch_undo_label(),
        Some(tr!("action-sketch-break-link")),
        "Name des Schritts"
    );
    assert_ne!(
        sketch_view::curve_color(&s, c),
        theme::sketch_colors(s.dark).projected
    );
    // Eine nicht projizierte Kurve: abgelehnt, nichts geändert.
    assert!(s.break_projection_link(c).is_err());
    assert_eq!(undo_len(&s), before + 1);
    // Rückgängig stellt die Verknüpfung wieder her.
    assert!(s.undo_sketch());
    assert_eq!(draft(&s).copies.len(), 1);
    assert_eq!(draft(&s).copies[0].curve, c);
    assert!(s.redo_sketch());
    assert!(draft(&s).copies.is_empty());
    s.close_sketch();
    assert!(def(&s, id).copies.is_empty(), "gelöst gespeichert");
}

#[test]
fn break_link_action_needs_a_selected_projection() {
    let (mut s, _, c) = projected();
    // Ohne Auswahl tut die Aktion nichts.
    select(&mut s, &[]);
    assert!(
        !s.sketch_session
            .as_ref()
            .unwrap()
            .can_break_links(draft(&s))
    );
    select(&mut s, &[Item::Curve(c)]);
    assert!(
        s.sketch_session
            .as_ref()
            .unwrap()
            .can_break_links(draft(&s))
    );
    // Die Aktion löst die gewählten Projektionen in einem Schritt.
    let before = undo_len(&s);
    {
        let (se, sf) = s.session_draft_mut().unwrap();
        assert!(se.break_links(sf));
    }
    s.observe_sketch(false);
    assert!(draft(&s).copies.is_empty());
    assert_eq!(undo_len(&s), before + 1);
    assert!(
        !s.sketch_session
            .as_ref()
            .unwrap()
            .can_break_links(draft(&s))
    );
    assert_eq!(Action::SketchBreakLink.id(), "skizze.verknuepfung_aufheben");
    assert_eq!(
        Action::SketchBreakLink.label(),
        tr!("action-sketch-break-link")
    );
}

#[test]
fn deleting_projected_curve_prunes_copy() {
    let (mut s, id, c) = projected();
    select(&mut s, &[Item::Curve(c)]);
    {
        let (se, sk) = s.session_sketch_mut().unwrap();
        se.delete_selection(sk);
    }
    s.observe_sketch(false);
    assert!(draft(&s).sketch.curve(c).is_none());
    s.close_sketch();
    assert!(
        def(&s, id).copies.is_empty(),
        "kein Verweis auf eine gelöschte Kurve"
    );
    let f = s.document.feature(id).unwrap();
    assert!(f.warning().is_none(), "keine Warnung: {:?}", f.warning());
}

#[test]
fn warning_shows_in_browser_and_history() {
    let (mut s, id, c) = projected();
    // Eigenes Profil abseits des Quaders für eine Extrusion.
    s.session_sketch_mut()
        .unwrap()
        .1
        .add_rectangle(DVec2::new(50.0, 50.0), DVec2::new(55.0, 55.0))
        .unwrap();
    s.close_sketch();
    let e = s.extrude_sketch(id, 0, 2.0).unwrap();
    assert!(s.document.feature(id).unwrap().warning().is_none());
    assert_eq!(browser::sketch_row_warning(&s, id), None);
    let step = |s: &AppState, f: FeatureId| {
        history_bar::steps(s, e)
            .into_iter()
            .find(|v| v.id == f)
            .expect("Schritt in der Leiste")
            .state
    };
    assert_eq!(step(&s, id), StepState::Active);

    // Bezug verloren: Die Projektion zeigt auf einen Kreis, die Kante ist eine Gerade.
    s.document
        .update(id, |k| {
            if let FeatureKind::Sketch(sf) = k {
                let m = sf.sketch.add_point(80.0, 80.0).unwrap();
                let circle = sf.sketch.add_circle(m, 3.0).unwrap();
                for copy in &mut sf.copies {
                    if copy.curve == c {
                        copy.curve = circle;
                    }
                }
            }
        })
        .unwrap();
    s.refresh();
    let warning = s
        .document
        .feature(id)
        .unwrap()
        .warning()
        .expect("Warnung bei verlorenem Bezug")
        .to_string();
    assert_eq!(browser::sketch_row_warning(&s, id), Some(warning.clone()));
    assert_eq!(step(&s, id), StepState::Warning(warning));
    assert_eq!(
        step(&s, e),
        StepState::Active,
        "die Extrusion bleibt gültig"
    );
}
