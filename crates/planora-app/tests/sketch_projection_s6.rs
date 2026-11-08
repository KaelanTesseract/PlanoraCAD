//! Skizzen 6, Task 8: projizierte Geometrie lila (wie Fusion), „Verknüpfung aufheben“ als ein
//! Rückgängig-Schritt, Aufräumen gelöschter Projektionen beim Beenden und die Warnung bei
//! verlorenem Bezug in Browser und Verlaufsleiste.

use planora_app::history_bar::{self, StepState};
use planora_app::keymap::Action;
use planora_app::sketch_state::Item;
use planora_app::state::{AppState, StandardPlane};
use planora_app::workspace::Workspace;
use planora_app::{browser, commands, sketch_view, theme};
use planora_feature::{FeatureId, FeatureKind, SketchFeature, SketchPlane};
use planora_i18n::tr;
use planora_math::{DVec2, DVec3};
use planora_project::config::Config;
use planora_sketch::model::CurveId;

const SCREEN: egui::Rect = egui::Rect {
    min: egui::pos2(0.0, 0.0),
    max: egui::pos2(1920.0, 1080.0),
};

/// Quader und eine offene Skizze auf XY mit einer projizierten Kante; liefert Skizze und Kurve.
fn projected() -> (AppState, FeatureId, CurveId) {
    let mut s = AppState::new();
    let (id, curves) = build(&mut s, 1);
    (s, id, curves[0])
}

/// Legt in `s` einen Quader und eine offene Skizze auf XY mit `n` projizierten Kanten an.
fn build(s: &mut AppState, n: usize) -> (FeatureId, Vec<CurveId>) {
    s.create_box(DVec3::new(10.0, 20.0, 5.0)).unwrap();
    let id = s
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    s.open_sketch(id).unwrap();
    s.observe_sketch(false);
    let solid = s.bodies[0].solid.clone();
    for (e, _) in solid.edges.iter() {
        if draft(s).copies.len() < n && s.project_edge_into_sketch(0, e).is_ok() {
            s.observe_sketch(false);
        }
    }
    let curves: Vec<CurveId> = draft(s).copies.iter().map(|c| c.curve).collect();
    assert_eq!(curves.len(), n, "genug Kanten projiziert");
    (id, curves)
}

/// Arbeitsbereich mit [`build`] im aktiven Reiter: für Knopf und Aktion (derselbe Weg).
fn projected_ws(n: usize) -> (Workspace, FeatureId, Vec<CurveId>) {
    let mut w = Workspace::new(None, Config::default());
    let (id, curves) = build(&mut w.active_tab_mut().state, n);
    w.observe_history(false);
    (w, id, curves)
}

fn st(w: &mut Workspace) -> &mut AppState {
    &mut w.active_tab_mut().state
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

/// Knopf und Aktion „Verknüpfung aufheben“ laufen beide über
/// `AppState::break_projection_links` mit den gewählten Projektionen; hier über die Aktion.
#[test]
fn break_link_is_one_undo_step() {
    let (mut w, id, c) = projected_ws(2);
    // Beide Projektionen und eine eigene Linie gewählt: ein Schritt löst beide.
    let own = {
        let sk = st(&mut w).session_sketch_mut().unwrap().1;
        let a = sk.add_point(40.0, 40.0).unwrap();
        let b = sk.add_point(50.0, 45.0).unwrap();
        sk.add_line(a, b).unwrap()
    };
    w.observe_history(false);
    select(
        st(&mut w),
        &[Item::Curve(c[0]), Item::Curve(own), Item::Curve(c[1])],
    );
    assert_eq!(st(&mut w).selected_projections(), vec![c[0], c[1]]);
    let before = undo_len(st(&mut w));
    commands::run_action(&mut w, Action::SketchBreakLink);
    w.observe_history(false);
    let s = st(&mut w);
    assert!(draft(s).copies.is_empty(), "beide Verweise gelöst");
    for curve in &c {
        assert!(
            draft(s).sketch.curve(*curve).is_some(),
            "die Kurven bleiben"
        );
    }
    assert_eq!(undo_len(s), before + 1, "genau ein Schritt für beide");
    assert_eq!(
        s.sketch_undo_label(),
        Some(tr!("action-sketch-break-link")),
        "Name des Schritts"
    );
    assert!(s.selected_projections().is_empty());
    // Ohne passende Auswahl tut die Aktion nichts (kein Schritt, keine Meldung).
    s.message = None;
    commands::run_action(&mut w, Action::SketchBreakLink);
    w.observe_history(false);
    assert_eq!(undo_len(st(&mut w)), before + 1);
    assert!(st(&mut w).message.is_none());
    // Rückgängig stellt beide Verknüpfungen wieder her, Wiederholen löst sie wieder.
    assert!(st(&mut w).undo_sketch());
    let restored: Vec<CurveId> = draft(st(&mut w)).copies.iter().map(|x| x.curve).collect();
    assert_eq!(restored, c);
    assert!(st(&mut w).redo_sketch());
    assert!(draft(st(&mut w)).copies.is_empty());
    st(&mut w).close_sketch();
    assert!(def(st(&mut w), id).copies.is_empty(), "gelöst gespeichert");
}

#[test]
fn break_projection_links_rejects_curves_without_projection() {
    let (mut s, _, c) = projected();
    s.break_projection_links(&[c]).unwrap();
    let before = undo_len(&s);
    // Schon gelöst: abgelehnt, Meldung gesetzt, nichts geändert.
    assert!(s.break_projection_links(&[c]).is_err());
    assert!(s.message.is_some());
    assert_eq!(undo_len(&s), before);
    assert!(
        s.break_projection_link(c).is_err(),
        "Kurzform für eine Kurve"
    );
    // Ohne offene Skizze ebenso.
    s.close_sketch();
    assert!(s.break_projection_links(&[c]).is_err());
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
