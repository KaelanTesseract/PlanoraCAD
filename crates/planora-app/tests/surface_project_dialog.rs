//! Skizzen 10, Task 19: Dialoge „Auf Fläche projizieren“ (Spec 6) und „Schnittkurve“ (Spec 7).
//! Bei 1920 × 1080.

mod common;

use common::Screen;
use egui::{Event, Modifiers};
use planora_app::keymap::Action;
use planora_app::sketch_kinds::ToolKind;
use planora_app::sketch_state::Tool;
use planora_app::state::{AppState, IncludePick, StandardPlane};
use planora_app::surface_project_dialog::{
    self as spd, DialogOutcome, Field, IntersectionCurveDialog, ProjectKind, SurfaceDialog,
    SurfaceProjectDialog,
};
use planora_feature::surface_project::{recompute_budget_for_tests, slow_fit_for_tests};
use planora_feature::{AxisId, CopySource, FeatureId, SketchFeature, SketchPlane};
use planora_math::DVec3;
use planora_sketch::model::{CurveId, Sketch};
use std::time::Duration;

const W: f32 = 1920.0;
const H: f32 = 1080.0;

// ---- Hilfen ---------------------------------------------------------------------------------------

/// Neue Skizze auf `plane`, geöffnet und mit leerem Verlauf.
fn open_new(s: &mut AppState, plane: StandardPlane) -> FeatureId {
    let id = s.add_sketch(SketchPlane::Standard(plane)).unwrap();
    s.open_sketch(id).unwrap();
    s.observe_sketch(false);
    id
}

fn draft(s: &AppState) -> &SketchFeature {
    s.sketch_draft.as_ref().unwrap()
}

fn sk(s: &AppState) -> &Sketch {
    s.session_sketch().unwrap()
}

fn undo_len(s: &AppState) -> usize {
    s.sketch_history().unwrap().undo_len()
}

/// Linie von `a` nach `b` (Skizzenkoordinaten) in der offenen Skizze.
fn add_line(s: &mut AppState, a: (f64, f64), b: (f64, f64)) -> CurveId {
    let (_, sk) = s.session_sketch_mut().unwrap();
    let pa = sk.add_point(a.0, a.1).unwrap();
    let pb = sk.add_point(b.0, b.1).unwrap();
    let c = sk.add_line(pa, pb).unwrap();
    s.observe_sketch(false);
    c
}

/// Eine fertige Skizze auf `plane` mit einer Linie; liefert Skizze und Linie.
fn finished_line(
    s: &mut AppState,
    plane: StandardPlane,
    a: (f64, f64),
    b: (f64, f64),
) -> (FeatureId, CurveId) {
    let id = open_new(s, plane);
    let c = add_line(s, a, b);
    s.close_sketch();
    (id, c)
}

/// Die obere Fläche des Quaders (alle Ecken bei z = 5).
fn top_face(s: &AppState) -> planora_kernel::Id<planora_kernel::Face> {
    let solid = &s.bodies[0].solid;
    solid
        .faces
        .iter()
        .find(|(_, f)| {
            f.outer.half_edges.iter().all(|he| {
                let e = solid.edges.get(he.edge).unwrap();
                let a = solid.vertices.get(e.start).unwrap().point;
                (a.z - 5.0).abs() < 1e-9
            })
        })
        .map(|(id, _)| id)
        .expect("Fläche oben")
}

/// Quader 10 × 20 × 5, eine fertige Skizze auf XY mit einer Linie (über dem Quader oder bei
/// `far` weit daneben) und eine offene neue Skizze auf XY.
fn box_line_and_sketch(far: bool) -> (AppState, FeatureId, CurveId) {
    let mut s = AppState::new();
    s.create_box(DVec3::new(10.0, 20.0, 5.0)).unwrap();
    let (a, b) = if far {
        ((100.0, 100.0), (110.0, 120.0))
    } else {
        ((2.0, 2.0), (8.0, 15.0))
    };
    let (first, line) = finished_line(&mut s, StandardPlane::XY, a, b);
    open_new(&mut s, StandardPlane::XY);
    (s, first, line)
}

/// Wählt die Art `kind` am Knopf „Projizieren/Einbeziehen“; der Dialog öffnet.
fn choose(s: &mut AppState, kind: ToolKind) {
    let (se, sk) = s.session_sketch_mut().unwrap();
    se.choose_kind(sk, kind);
    spd::sync(s);
}

/// Der offene Dialog „Auf Fläche projizieren“.
fn project_dialog(s: &mut AppState) -> &mut SurfaceProjectDialog {
    match s.surface_dialog.as_mut() {
        Some(SurfaceDialog::Project(d)) => d,
        other => panic!("Dialog „Auf Fläche projizieren“ erwartet: {other:?}"),
    }
}

fn key(k: egui::Key) -> Event {
    Event::Key {
        key: k,
        physical_key: Some(k),
        pressed: true,
        repeat: false,
        modifiers: Modifiers::NONE,
    }
}

/// Ein Bild mit dem offenen Dialog und den Ereignissen; liefert das letzte Ergebnis.
fn frame(screen: &mut Screen, s: &mut AppState, events: Vec<Event>) -> Option<DialogOutcome> {
    let mut out = None;
    screen.frame(events, &mut |ctx| {
        if let Some(o) = spd::show_open(ctx, s) {
            out = Some(o);
        }
    });
    out
}

/// Bereitet „Auf Fläche projizieren“ vor: Oberseite als Fläche, die Linie der ersten Skizze als
/// Kurve (über den Dialog im Zustand).
fn filled_project(s: &mut AppState, first: FeatureId, line: CurveId) {
    choose(s, ToolKind::ProjectToSurface);
    let face = top_face(s);
    assert!(spd::deliver(s, IncludePick::Face(0, face)));
    project_dialog(s).focus = Field::Curves;
    assert!(spd::deliver(
        s,
        IncludePick::SketchCurve {
            sketch: first,
            curve: line,
        }
    ));
}

// ---- Auf Fläche projizieren -----------------------------------------------------------------------

#[test]
fn project_dialog_ok_adds_curves_one_undo() {
    let (mut s, first, line) = box_line_and_sketch(false);
    filled_project(&mut s, first, line);
    {
        let d = project_dialog(&mut s);
        assert_eq!(d.faces.len(), 1);
        assert_eq!(d.curves.len(), 1);
        assert_eq!(d.kind, ProjectKind::ClosestPoint);
        assert!(d.ok_enabled());
    }
    let n = undo_len(&s);
    let mut screen = Screen::new(W, H);
    let seen = screen.settle(2, &mut |ctx| {
        spd::show_open(ctx, &mut s);
    });
    // Titel und Felder wie in Fusion.
    for label in [
        "Auf Fläche projizieren",
        "Flächen",
        "Kurven",
        "Projektionsart",
        "Nächster Punkt",
        "Entlang Vektor",
    ] {
        assert!(common::find(&seen, label).is_some(), "„{label}“ fehlt");
    }
    // Enter übernimmt.
    let out = frame(&mut screen, &mut s, vec![key(egui::Key::Enter)]);
    assert_eq!(out, Some(DialogOutcome::Applied));
    assert!(s.surface_dialog.is_none(), "der Dialog ist zu");
    assert_eq!(s.sketch_session.as_ref().unwrap().tool, Tool::Select);
    s.observe_sketch(false);
    assert_eq!(undo_len(&s), n + 1, "ein OK, ein Schritt");
    assert_eq!(
        s.sketch_undo_label().as_deref(),
        Some("Auf Fläche projizieren")
    );
    // 3D-Geometrie auf der Oberseite, verknüpft mit der Herkunft (lila).
    let copies = &draft(&s).copies;
    assert!(!copies.is_empty());
    assert!(
        copies
            .iter()
            .all(|c| matches!(c.source, CopySource::OnSurface(_)))
    );
    let of = copies.len() as u32;
    assert!(copies.iter().all(|c| c.of == of));
    let curves: Vec<CurveId> = sk(&s).curves().map(|(c, _)| c).collect();
    assert!(!curves.is_empty());
    for c in &curves {
        assert!(sk(&s).is_3d_curve(*c));
        let (k, t0, t1) = sk(&s).curve_world(*c).unwrap();
        for i in 0..=8 {
            let p = k.point(t0 + (t1 - t0) * f64::from(i) / 8.0);
            assert!((p.z - 5.0).abs() < 1e-6, "auf der Oberseite: {p}");
        }
    }
    // Rückgängig nimmt alles.
    assert!(s.undo_sketch());
    assert_eq!(sk(&s).curves().count(), 0);
    assert!(draft(&s).copies.is_empty());
}

#[test]
fn project_dialog_esc_leaves_nothing() {
    let (mut s, first, line) = box_line_and_sketch(false);
    filled_project(&mut s, first, line);
    let n = undo_len(&s);
    let mut screen = Screen::new(W, H);
    screen.settle(2, &mut |ctx| {
        spd::show_open(ctx, &mut s);
    });
    let out = frame(&mut screen, &mut s, vec![key(egui::Key::Escape)]);
    assert_eq!(out, Some(DialogOutcome::Cancelled));
    assert!(s.surface_dialog.is_none());
    assert_eq!(s.sketch_session.as_ref().unwrap().tool, Tool::Select);
    s.observe_sketch(false);
    assert_eq!(undo_len(&s), n);
    assert_eq!(sk(&s).curves().count(), 0);
    assert!(draft(&s).copies.is_empty());
    // Ein Werkzeugwechsel schließt den Dialog ebenso ohne Spur.
    filled_project(&mut s, first, line);
    let (se, sk) = s.session_sketch_mut().unwrap();
    se.choose_tool(sk, Tool::Line);
    spd::sync(&mut s);
    assert!(s.surface_dialog.is_none());
    assert_eq!(undo_len(&s), n);
}

#[test]
fn along_vector_needs_direction_ok_disabled() {
    let (mut s, first, line) = box_line_and_sketch(false);
    filled_project(&mut s, first, line);
    let d = project_dialog(&mut s);
    d.set_kind(ProjectKind::AlongVector);
    assert_eq!(d.focus, Field::Direction, "die Richtung fehlt noch");
    assert!(!d.ok_enabled());
    let n = undo_len(&s);
    let mut screen = Screen::new(W, H);
    let seen = screen.settle(2, &mut |ctx| {
        spd::show_open(ctx, &mut s);
    });
    assert!(common::find(&seen, "Richtung").is_some());
    // Enter übernimmt nicht, solange die Richtung fehlt.
    let out = frame(&mut screen, &mut s, vec![key(egui::Key::Enter)]);
    assert_eq!(out, Some(DialogOutcome::Open));
    s.observe_sketch(false);
    assert_eq!(undo_len(&s), n);
    assert_eq!(sk(&s).curves().count(), 0);
    // Eine ebene Fläche gilt als Richtung (ihre Normale), der Ursprung nicht, die Z-Achse schon.
    let face = top_face(&s);
    assert!(
        spd::deliver(&mut s, IncludePick::Face(0, face)),
        "ebene Fläche = Normale"
    );
    let d = project_dialog(&mut s);
    assert!(d.ok_enabled());
    d.direction = None;
    d.focus = Field::Direction;
    assert!(!spd::deliver(&mut s, IncludePick::Origin));
    assert!(!project_dialog(&mut s).ok_enabled());
    assert!(spd::deliver(&mut s, IncludePick::Axis(AxisId::Z)));
    let mut d = project_dialog(&mut s).clone();
    assert!(d.ok_enabled());
    assert_eq!(d.direction.as_ref().unwrap().vector, DVec3::Z);
    // OK legt die Linie entlang Z auf die Oberseite.
    assert_eq!(d.ok(&mut s), Ok(1));
    s.observe_sketch(false);
    assert_eq!(undo_len(&s), n + 1);
}

#[test]
fn active_sketch_curve_not_pickable() {
    let (mut s, first, line) = box_line_and_sketch(false);
    let own = add_line(&mut s, (0.0, 0.0), (5.0, 5.0));
    let me = s.open_sketch_id().unwrap();
    choose(&mut s, ToolKind::ProjectToSurface);
    project_dialog(&mut s).focus = Field::Curves;
    assert!(!spd::deliver(
        &mut s,
        IncludePick::SketchCurve {
            sketch: me,
            curve: own,
        }
    ));
    let d = project_dialog(&mut s);
    assert!(d.curves.is_empty());
    assert_eq!(
        d.error().as_deref(),
        Some("Kurven der aktiven Skizze sind nicht wählbar; bitte eine neue Skizze beginnen.")
    );
    // Eine Kurve einer früheren Skizze geht; ein zweiter Klick nimmt sie wieder heraus.
    let pick = IncludePick::SketchCurve {
        sketch: first,
        curve: line,
    };
    assert!(spd::deliver(&mut s, pick.clone()));
    assert_eq!(project_dialog(&mut s).curves.len(), 1);
    assert!(project_dialog(&mut s).error().is_none());
    assert!(spd::deliver(&mut s, pick));
    assert!(project_dialog(&mut s).curves.is_empty());
    // Auch die Schnittkurve nimmt keine Kurve der aktiven Skizze, ebenso keine 3D-Kurve.
    choose(&mut s, ToolKind::IntersectionCurve);
    let mut d = IntersectionCurveDialog::new();
    assert_eq!(d.focus, Field::Curve);
    assert!(!d.take_pick(
        &s,
        IncludePick::SketchCurve {
            sketch: me,
            curve: own
        }
    ));
    assert!(d.curve.is_none());
    assert_eq!(
        d.error().as_deref(),
        Some("Kurven der aktiven Skizze sind nicht wählbar; bitte eine neue Skizze beginnen.")
    );
    s.close_sketch();
    let three_d = open_new(&mut s, StandardPlane::XY);
    let c3 = {
        let (se, sk) = s.session_sketch_mut().unwrap();
        se.set_three_d(sk, true);
        se.choose_tool(sk, Tool::Line);
        se.place_point_3d(sk, DVec3::new(0.0, 0.0, 0.0), None)
            .unwrap();
        se.place_point_3d(sk, DVec3::new(0.0, 5.0, 9.0), None)
            .unwrap();
        se.choose_tool(sk, Tool::Select);
        sk.curves().map(|(c, _)| c).next().unwrap()
    };
    s.observe_sketch(false);
    s.close_sketch();
    open_new(&mut s, StandardPlane::XY);
    assert!(!d.take_pick(
        &s,
        IncludePick::SketchCurve {
            sketch: three_d,
            curve: c3
        }
    ));
    assert_eq!(
        d.error().as_deref(),
        Some("Nur 2D-Skizzenkurven sind wählbar; diese Kurve liegt nicht in ihrer Skizzenebene.")
    );
}

// ---- Schnittkurve ---------------------------------------------------------------------------------

#[test]
fn intersection_dialog_two_sketches() {
    let mut s = AppState::new();
    // Kreis r = 10 oben, Linie z = x von vorn.
    let top = open_new(&mut s, StandardPlane::XY);
    let (circle, beside) = {
        let (_, sk) = s.session_sketch_mut().unwrap();
        let m = sk.add_point(0.0, 0.0).unwrap();
        let circle = sk.add_circle(m, 10.0).unwrap();
        let a = sk.add_point(20.0, -5.0).unwrap();
        let b = sk.add_point(20.0, 5.0).unwrap();
        (circle, sk.add_line(a, b).unwrap())
    };
    s.observe_sketch(false);
    s.close_sketch();
    let (front, line) = finished_line(&mut s, StandardPlane::XZ, (-15.0, -15.0), (15.0, 15.0));
    open_new(&mut s, StandardPlane::XY);
    choose(&mut s, ToolKind::IntersectionCurve);
    let n = undo_len(&s);
    let mut screen = Screen::new(W, H);
    let seen = screen.settle(2, &mut |ctx| {
        spd::show_open(ctx, &mut s);
    });
    for label in ["Schnittkurve", "Kurve", "Kurven/Flächen"] {
        assert!(common::find(&seen, label).is_some(), "„{label}“ fehlt");
    }
    assert!(spd::deliver(
        &mut s,
        IncludePick::SketchCurve {
            sketch: top,
            curve: circle,
        }
    ));
    let Some(SurfaceDialog::Intersection(d)) = s.surface_dialog.as_ref() else {
        panic!("Dialog „Schnittkurve“ erwartet");
    };
    assert!(d.curve.is_some());
    assert_eq!(d.focus, Field::Targets, "weiter zu „Kurven/Flächen“");
    assert!(!d.ok_enabled());
    // Ein Ziel aus derselben Skizze wie die „Kurve“ wird abgelehnt (Spec 7: andere Skizzen).
    assert!(!spd::deliver(
        &mut s,
        IncludePick::SketchCurve {
            sketch: top,
            curve: beside,
        }
    ));
    let Some(SurfaceDialog::Intersection(d)) = s.surface_dialog.as_ref() else {
        panic!("Dialog „Schnittkurve“ erwartet");
    };
    assert!(d.targets.is_empty());
    assert_eq!(
        d.error().as_deref(),
        Some("Kurve und Kurven/Flächen müssen aus verschiedenen Skizzen stammen.")
    );
    assert!(spd::deliver(
        &mut s,
        IncludePick::SketchCurve {
            sketch: front,
            curve: line,
        }
    ));
    // Mit der normalen Zeitgrenze (`SURFACE_PROJECT_BUDGET`).
    let out = frame(&mut screen, &mut s, vec![key(egui::Key::Enter)]);
    let why = match s.surface_dialog.as_ref() {
        Some(SurfaceDialog::Intersection(d)) => d.error(),
        _ => None,
    };
    assert_eq!(out, Some(DialogOutcome::Applied), "{why:?}");
    s.observe_sketch(false);
    assert_eq!(undo_len(&s), n + 1);
    assert_eq!(s.sketch_undo_label().as_deref(), Some("Schnittkurve"));
    let copies = &draft(&s).copies;
    assert!(!copies.is_empty());
    assert!(
        copies
            .iter()
            .all(|c| matches!(c.source, CopySource::Intersection(_)))
    );
    let mut zmax = f64::MIN;
    for (c, _) in sk(&s).curves() {
        assert!(sk(&s).is_3d_curve(c));
        let (k, t0, t1) = sk(&s).curve_world(c).unwrap();
        for i in 0..=32 {
            let p = k.point(t0 + (t1 - t0) * f64::from(i) / 32.0);
            assert!(
                (p.x.hypot(p.y) - 10.0).abs() < 0.05,
                "auf dem Zylinder: {p}"
            );
            assert!((p.z - p.x).abs() < 0.05, "auf dem Auszug der Linie: {p}");
            zmax = zmax.max(p.z);
        }
    }
    assert!((zmax - 10.0).abs() < 0.1, "höchster Punkt {zmax}");
}

// ---- Meldungen ------------------------------------------------------------------------------------

#[test]
fn nohit_shows_message() {
    let (mut s, first, line) = box_line_and_sketch(true);
    filled_project(&mut s, first, line);
    let n = undo_len(&s);
    let mut screen = Screen::new(W, H);
    screen.settle(2, &mut |ctx| {
        spd::show_open(ctx, &mut s);
    });
    let out = frame(&mut screen, &mut s, vec![key(egui::Key::Enter)]);
    assert_eq!(out, Some(DialogOutcome::Open), "der Dialog bleibt offen");
    let seen = screen.settle(2, &mut |ctx| {
        spd::show_open(ctx, &mut s);
    });
    let text = "Die Kurven treffen die Flächen nicht";
    assert!(common::find(&seen, text).is_some(), "Meldung im Dialog");
    assert_eq!(project_dialog(&mut s).error().as_deref(), Some(text));
    s.observe_sketch(false);
    assert_eq!(undo_len(&s), n);
    assert_eq!(sk(&s).curves().count(), 0);
}

#[test]
fn tooslow_shows_message_leaves_nothing() {
    let (mut s, first, line) = box_line_and_sketch(false);
    filled_project(&mut s, first, line);
    let n = undo_len(&s);
    let result = {
        let _budget = recompute_budget_for_tests(Duration::ZERO);
        let _slow = slow_fit_for_tests(Duration::from_millis(5));
        let mut d = project_dialog(&mut s).clone();
        let r = d.ok(&mut s);
        (r, d.error())
    };
    let msg = "Die Rechnung dauert zu lange (über 2 s) und wurde abgebrochen; es entsteht nichts.";
    assert_eq!(result.0, Err(msg.to_string()));
    assert_eq!(result.1.as_deref(), Some(msg));
    s.observe_sketch(false);
    assert_eq!(undo_len(&s), n);
    assert_eq!(sk(&s).curves().count(), 0);
    assert!(draft(&s).copies.is_empty());
    let en = planora_i18n::with_lang(&planora_i18n::english(), || {
        planora_feature::surface_project::ProjectError::TooSlow.message()
    });
    assert_eq!(
        en,
        "The computation takes too long (over 2 s) and was cancelled; nothing is created."
    );
}

// ---- Klappliste und Tasten ------------------------------------------------------------------------

#[test]
fn surface_kinds_in_flyout_without_key() {
    assert_eq!(
        ToolKind::of(Tool::ProjectEdge),
        &[
            ToolKind::Project,
            ToolKind::Intersect,
            ToolKind::Silhouette,
            ToolKind::Include3d,
            ToolKind::ProjectToSurface,
            ToolKind::IntersectionCurve,
            // Skizzen 11b, Task 14: bewusst angepasst, „Drehprofil“ steht dahinter.
            ToolKind::SpunProfile,
            // Skizzen 11b, Task 16: bewusst angepasst, „Isoparametrische Kurve“ steht dahinter.
            ToolKind::Isoparametric,
        ]
    );
    for (kind, action, de, en) in [
        (
            ToolKind::ProjectToSurface,
            Action::SketchProjectToSurface,
            "Auf Fläche projizieren",
            "Project To Surface",
        ),
        (
            ToolKind::IntersectionCurve,
            Action::SketchIntersectionCurve,
            "Schnittkurve",
            "Intersection Curve",
        ),
    ] {
        assert_eq!(kind.tool(), Tool::ProjectEdge);
        assert_eq!(kind.action(), action);
        assert!(action.default_bindings().is_empty());
        assert_eq!(kind.label(), de);
        let e = planora_i18n::with_lang(&planora_i18n::english(), || kind.label());
        assert_eq!(e, en);
    }
}

// ---- Hervorhebung der Wahl und Meldungen nur im Dialog (Task H3) ----------------------------------

/// Ist eine Fläche der Szene in der Auswahlfarbe getönt?
fn face_tinted(s: &AppState) -> bool {
    let selected = s.palette().dimmed().face_selected;
    s.scene.face_vertices.iter().any(|v| v.color == selected)
}

/// Zeichnet die Ansicht die Kurven fertiger Skizzen in der Auswahlfarbe?
fn curve_lit(s: &AppState) -> bool {
    let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(W, H));
    let selected = planora_app::theme::sketch_colors(s.dark).selected;
    fn lit(shape: &egui::Shape, c: egui::Color32) -> bool {
        match shape {
            egui::Shape::Vec(v) => v.iter().any(|s| lit(s, c)),
            egui::Shape::Path(p) => p.stroke.color == egui::epaint::ColorMode::Solid(c),
            egui::Shape::LineSegment { stroke, .. } => stroke.color == c,
            _ => false,
        }
    }
    planora_app::sketch_view::finished_sketch_shapes(s, rect)
        .iter()
        .any(|shape| lit(shape, selected))
}

#[test]
fn picked_items_are_highlighted_in_the_view() {
    let (mut s, first, line) = box_line_and_sketch(false);
    assert!(
        !face_tinted(&s) && !curve_lit(&s),
        "vorher nichts hervorgehoben"
    );
    filled_project(&mut s, first, line);
    let face = IncludePick::Face(0, top_face(&s));
    let curve = IncludePick::SketchCurve {
        sketch: first,
        curve: line,
    };
    assert!(s.dialog_marks.contains(&face), "{:?}", s.dialog_marks);
    assert!(s.dialog_marks.contains(&curve), "{:?}", s.dialog_marks);
    assert!(face_tinted(&s), "Fläche getönt");
    assert!(curve_lit(&s), "Kurve in der Auswahlfarbe");
    // Ein zweiter Klick nimmt die Kurve wieder heraus.
    assert!(spd::deliver(&mut s, curve.clone()));
    assert!(!s.dialog_marks.contains(&curve));
    assert!(!curve_lit(&s));
    // Die Richtung von „Entlang Vektor“ steht ebenso in der Liste.
    project_dialog(&mut s).set_kind(ProjectKind::AlongVector);
    let axis = IncludePick::Axis(AxisId::Z);
    assert!(spd::deliver(&mut s, axis.clone()));
    assert!(s.dialog_marks.contains(&axis));
    let view = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(W, H));
    assert_eq!(
        planora_app::sketch_view::dialog_mark_shapes(&s, view).len(),
        1,
        "Achse in der Auswahlfarbe"
    );
    // Eine Körperecke im Feld „Kurven“ steht als Punkt in der Ansicht.
    let edge = s.bodies[0].solid.edges.iter().next().unwrap().0;
    let vertex = IncludePick::Vertex {
        body: 0,
        edge,
        end: false,
    };
    project_dialog(&mut s).focus = Field::Curves;
    assert!(spd::deliver(&mut s, vertex.clone()));
    assert!(s.dialog_marks.contains(&vertex));
    assert_eq!(
        planora_app::sketch_view::dialog_mark_shapes(&s, view).len(),
        2,
        "Achse und Ecke"
    );
    // „Leeren“ und Abbrechen nehmen die Hervorhebung weg.
    let mut screen = Screen::new(W, H);
    screen.settle(2, &mut |ctx| {
        spd::show_open(ctx, &mut s);
    });
    project_dialog(&mut s).set_kind(ProjectKind::ClosestPoint);
    frame(&mut screen, &mut s, vec![]);
    assert!(
        !s.dialog_marks.contains(&axis),
        "Richtung nur bei Entlang Vektor"
    );
    let out = frame(&mut screen, &mut s, vec![key(egui::Key::Escape)]);
    assert_eq!(out, Some(DialogOutcome::Cancelled));
    assert!(s.dialog_marks.is_empty(), "{:?}", s.dialog_marks);
    assert!(!face_tinted(&s));
    // Ein Werkzeugwechsel ebenso.
    filled_project(&mut s, first, line);
    assert!(face_tinted(&s));
    let (se, sk) = s.session_sketch_mut().unwrap();
    se.choose_tool(sk, Tool::Line);
    spd::sync(&mut s);
    assert!(s.dialog_marks.is_empty());
    assert!(!face_tinted(&s));
}

#[test]
fn intersection_picks_are_highlighted_in_the_view() {
    let mut s = AppState::new();
    s.create_box(DVec3::new(10.0, 20.0, 5.0)).unwrap();
    let (first, line) = finished_line(&mut s, StandardPlane::XZ, (2.0, -2.0), (8.0, 9.0));
    open_new(&mut s, StandardPlane::XY);
    choose(&mut s, ToolKind::IntersectionCurve);
    let curve = IncludePick::SketchCurve {
        sketch: first,
        curve: line,
    };
    assert!(spd::deliver(&mut s, curve.clone()));
    let face = IncludePick::Face(0, top_face(&s));
    assert!(spd::deliver(&mut s, face.clone()));
    assert!(s.dialog_marks.contains(&curve), "{:?}", s.dialog_marks);
    assert!(s.dialog_marks.contains(&face), "{:?}", s.dialog_marks);
    assert!(face_tinted(&s) && curve_lit(&s));
}

/// Meldungen der Dialoge (abgelehnter Klick, Klick ins Leere, „Die Kurven treffen die Flächen
/// nicht“) stehen nur im Dialog, nicht zusätzlich in der Skizzenpalette.
#[test]
fn dialog_messages_stay_out_of_the_palette() {
    let palette_message = |s: &AppState| s.sketch_session.as_ref().unwrap().message.clone();
    let (mut s, first, line) = box_line_and_sketch(true);
    filled_project(&mut s, first, line);
    let mut screen = Screen::new(W, H);
    screen.settle(2, &mut |ctx| {
        spd::show_open(ctx, &mut s);
    });
    frame(&mut screen, &mut s, vec![key(egui::Key::Enter)]);
    let text = "Die Kurven treffen die Flächen nicht";
    assert_eq!(project_dialog(&mut s).error().as_deref(), Some(text));
    assert_eq!(palette_message(&s), None);
    // Ein unpassender Klick: Grund im Dialog.
    project_dialog(&mut s).focus = Field::Faces;
    assert!(!spd::deliver(&mut s, IncludePick::Axis(AxisId::X)));
    assert!(project_dialog(&mut s).error().is_some());
    assert_eq!(palette_message(&s), None);
    // Ein Klick ins Leere: Hinweis im Dialog.
    project_dialog(&mut s).focus = Field::Curves;
    spd::refuse(&mut s);
    let hint = planora_i18n::tr!("sketch-pick-project-to-surface");
    assert_eq!(project_dialog(&mut s).error(), Some(hint));
    assert_eq!(palette_message(&s), None);
}
