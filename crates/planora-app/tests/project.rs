//! Skizzen 6, Task 9: Fusion „Projizieren“ mit Kanten, Flächen und Körpern, Auswahlfilter
//! „Angegebene Elemente“ / „Körper“, Schalter „Projektionsverknüpfung“ und das Klappmenü
//! „Projizieren/Einschließen“ in der Skizzenleiste (ohne Fenster, 1920×1080).

use planora_app::shell::{STRIP_H, STRIP_TOOLS};
use planora_app::sketch_kinds::ToolKind;
use planora_app::sketch_panel::{self, TOOL_ORDER};
use planora_app::sketch_state::{ProjectFilter, ProjectState, Tool};
use planora_app::state::{AppState, ProjectTarget, StandardPlane};
use planora_app::{fonts, sketch_view};
use planora_feature::{CopySource, FeatureId, FeatureKind, ProfileSel, SketchFeature, SketchPlane};
use planora_i18n::tr;
use planora_kernel::{Face, Id};
use planora_math::{DVec2, DVec3, Frame, Tolerance};
use planora_render::{EdgeRef, FaceRef, Pick};
use planora_sketch::model::{CurveKind, Sketch};

fn draft(s: &AppState) -> &SketchFeature {
    s.sketch_draft.as_ref().unwrap()
}

fn kinds(s: &AppState) -> Vec<&'static str> {
    let sk = &draft(s).sketch;
    sk.curves()
        .map(|(_, c)| match c.kind {
            CurveKind::Line { .. } => "Linie",
            CurveKind::Circle { .. } => "Kreis",
            CurveKind::Arc { .. } => "Bogen",
            CurveKind::Ellipse { .. } => "Ellipse",
            CurveKind::EllipticArc { .. } => "Ellipsenbogen",
            CurveKind::Spline { .. } => "Spline",
        })
        .collect()
}

fn undo_len(s: &AppState) -> usize {
    s.sketch_history().unwrap().undo_len()
}

/// Die Fläche des Körpers `body`, deren Ebene nach oben zeigt und am höchsten liegt.
fn top_face(s: &AppState, body: usize) -> Id<Face> {
    let solid = &s.bodies[body].solid;
    let top_z = s.bodies[body].bounds.1.z;
    solid
        .faces
        .iter()
        .find(|(id, _)| {
            planora_sketch::plane_of_face(solid, *id)
                .is_some_and(|f| f.z.z > 0.5 && (f.origin.z - top_z).abs() < 1e-9)
        })
        .map(|(id, _)| id)
        .expect("Deckfläche")
}

/// Quader 10 × 20 × 5 und eine offene Skizze auf seiner Deckfläche.
fn box_with_sketch_on_top() -> AppState {
    let mut s = AppState::new();
    s.create_box(DVec3::new(10.0, 20.0, 5.0)).unwrap();
    let top = top_face(&s, 0);
    let id = s.new_sketch_on_face(0, top).unwrap();
    s.open_sketch(id).unwrap();
    s.observe_sketch(false);
    s
}

/// Quader 10 × 20 × 5 und eine offene Skizze auf XY.
fn box_with_sketch_on_xy() -> AppState {
    let mut s = AppState::new();
    s.create_box(DVec3::new(10.0, 20.0, 5.0)).unwrap();
    let id = s
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    s.open_sketch(id).unwrap();
    s.observe_sketch(false);
    s
}

/// Fügt eine Skizze `sk` auf XY als Feature an.
fn add_sketch(s: &mut AppState, sk: Sketch) -> FeatureId {
    let id = s
        .document
        .add_feature(
            "Skizze",
            FeatureKind::Sketch(SketchFeature {
                sketch: sk,
                plane: SketchPlane::Standard(StandardPlane::XY),
                copies: vec![],
                formulas: Default::default(),
            }),
        )
        .unwrap();
    s.refresh();
    id
}

fn extrude(s: &mut AppState, sketch: FeatureId, height: f64) {
    s.document
        .add_feature(
            "Extrusion",
            FeatureKind::Extrude {
                sketch,
                profiles: ProfileSel::Index(0),
                height,
            },
        )
        .unwrap();
    s.refresh();
}

#[test]
fn project_box_body_gives_four_lines_without_duplicates() {
    // Review Focus 1: Ober- und Unterkante liegen übereinander, die senkrechten Kanten werden
    // Punkte ⇒ genau vier Linien, keine doppelten, kein Fehler.
    let mut s = box_with_sketch_on_top();
    let n = s.project_into_sketch(ProjectTarget::Body(0), true).unwrap();
    assert_eq!(n, 4);
    assert_eq!(kinds(&s), vec!["Linie"; 4]);
    assert_eq!(draft(&s).copies.len(), 4);
    let len: f64 = {
        let sk = &draft(&s).sketch;
        sk.curves().filter_map(|(c, _)| sk.line_length(c)).sum()
    };
    assert!((len - 60.0).abs() < 1e-9, "Umfang 2·(10+20): {len}");
    assert!(s.sketch_session.as_ref().unwrap().message.is_none());
    // Noch einmal: nichts Neues (auch gegen die schon projizierten Kurven), Skizze unverändert.
    s.observe_sketch(false);
    let steps = undo_len(&s);
    let err = s
        .project_into_sketch(ProjectTarget::Body(0), true)
        .unwrap_err();
    assert_eq!(err, tr!("sketch-project-nothing-new"));
    assert_eq!(kinds(&s).len(), 4);
    assert_eq!(draft(&s).copies.len(), 4);
    // Auch die Deckfläche allein und eine einzelne Kante bringen nichts Neues.
    let top = top_face(&s, 0);
    assert!(
        s.project_into_sketch(ProjectTarget::Face(0, top), true)
            .is_err()
    );
    // Eine waagerechte Kante (keine senkrechte, die wäre nur ein Punkt).
    let edge = s.bodies[0]
        .solid
        .edges
        .iter()
        .find(|(_, e)| (e.curve.point(e.t0).z - e.curve.point(e.t1).z).abs() < 1e-9)
        .unwrap()
        .0;
    let err = s
        .project_into_sketch(ProjectTarget::Edge(0, edge), true)
        .unwrap_err();
    assert_eq!(err, tr!("sketch-project-nothing-new"));
    s.observe_sketch(false);
    assert_eq!(undo_len(&s), steps, "kein Schritt");
    // Beim Beenden bleibt alles stehen, die Skizze folgt dem Körper ohne Warnung.
    let id = s.open_sketch_id().unwrap();
    s.close_sketch();
    let f = s.document.feature(id).unwrap();
    assert!(f.warning().is_none(), "{:?}", f.warning());
    assert_eq!(s.document.sketch_of(id).unwrap().curves().count(), 4);
}

#[test]
fn project_face_with_hole_gives_both_loops() {
    let mut s = AppState::new();
    // Rechteck 20 × 10 mit einem Kreis r = 2 darin: Profil 0 ist der Ring.
    let mut sk = Sketch::new(Frame::WORLD, "Skizze", Tolerance::default());
    sk.add_rectangle(DVec2::ZERO, DVec2::new(20.0, 10.0))
        .unwrap();
    let m = sk.add_point(10.0, 5.0).unwrap();
    sk.add_circle(m, 2.0).unwrap();
    let base = add_sketch(&mut s, sk);
    extrude(&mut s, base, 5.0);
    let body = s.bodies.len() - 1;
    let top = top_face(&s, body);
    assert!(
        !s.bodies[body].solid.faces[top].inner.is_empty(),
        "Deckfläche mit Loch"
    );
    let id = s.new_sketch_on_face(body, top).unwrap();
    s.open_sketch(id).unwrap();
    s.observe_sketch(false);
    let n = s
        .project_into_sketch(ProjectTarget::Face(body, top), true)
        .unwrap();
    let k = kinds(&s);
    assert_eq!(n, k.len());
    assert_eq!(k.iter().filter(|k| **k == "Linie").count(), 4, "{k:?}");
    // Der Rand des Lochs: ein Kreis oder Bögen, zusammen ein voller Umlauf mit r = 2.
    let sk = &draft(&s).sketch;
    let round: Vec<_> = sk
        .curves()
        .filter(|(_, c)| matches!(c.kind, CurveKind::Circle { .. } | CurveKind::Arc { .. }))
        .collect();
    assert!(!round.is_empty(), "{k:?}");
    for (c, _) in &round {
        assert!((sk.radius(*c).unwrap() - 2.0).abs() < 1e-9);
    }
    assert_eq!(draft(&s).copies.len(), n, "je Kante ein Verweis");
}

#[test]
fn project_edge_is_linked_and_not_construction() {
    let mut s = box_with_sketch_on_xy();
    let solid = s.bodies[0].solid.clone();
    // Eine Kante, die in der Ebene liegt (keine senkrechte).
    let edge = solid
        .edges
        .iter()
        .find(|(_, e)| {
            let (a, b) = (e.curve.point(e.t0), e.curve.point(e.t1));
            a.z.abs() < 1e-9 && b.z.abs() < 1e-9
        })
        .unwrap()
        .0;
    let n = s
        .project_into_sketch(ProjectTarget::Edge(0, edge), true)
        .unwrap();
    assert_eq!(n, 1);
    let d = draft(&s);
    assert_eq!(d.copies.len(), 1);
    let c = d.copies[0].curve;
    assert!(matches!(d.copies[0].source, CopySource::Edge(_)));
    let data = d.sketch.curve(c).unwrap();
    assert!(!data.construction, "normale Geometrie (Skizzen 6)");
    for p in data.kind.points() {
        let point = d.sketch.points().find(|(id, _)| *id == p).unwrap().1;
        assert!(!point.construction);
    }
    // Kurzform: derselbe Weg.
    let other = solid
        .edges
        .iter()
        .find(|(id, e)| {
            *id != edge && e.curve.point(e.t0).z.abs() < 1e-9 && e.curve.point(e.t1).z.abs() < 1e-9
        })
        .unwrap()
        .0;
    s.project_edge_into_sketch(0, other).unwrap();
    assert_eq!(draft(&s).copies.len(), 2);
    // Eine senkrechte Kante wird ein Punkt: einzeln gewählt ist das ein Fehler mit Meldung.
    let vertical = solid
        .edges
        .iter()
        .find(|(_, e)| (e.curve.point(e.t0).z - e.curve.point(e.t1).z).abs() > 1.0)
        .unwrap()
        .0;
    let err = s
        .project_into_sketch(ProjectTarget::Edge(0, vertical), true)
        .unwrap_err();
    assert!(!err.is_empty());
    assert_eq!(
        s.sketch_session.as_ref().unwrap().message_text().as_deref(),
        Some(err.as_str())
    );
    assert_eq!(draft(&s).copies.len(), 2);
}

#[test]
fn link_off_creates_no_copy() {
    let mut s = box_with_sketch_on_top();
    let n = s
        .project_into_sketch(ProjectTarget::Body(0), false)
        .unwrap();
    assert_eq!(n, 4);
    assert_eq!(kinds(&s).len(), 4);
    assert!(draft(&s).copies.is_empty(), "feste Geometrie ohne Bezug");
    // Auch ohne Verknüpfung heißt der Schritt „Projizieren“.
    s.observe_sketch(false);
    assert_eq!(s.sketch_undo_label().as_deref(), Some("Projizieren"));
    let sk = &draft(&s).sketch;
    assert!(sk.curves().all(|(_, c)| !c.construction));
}

#[test]
fn freeform_edge_is_skipped_with_count() {
    let mut s = AppState::new();
    // Profil aus zwei Linien, einem Spline und einer Linie (wie tests/nurbs_app.rs).
    let mut sk = Sketch::new(Frame::WORLD, "Skizze", Tolerance::default());
    let p: Vec<_> = [(0.0, 0.0), (10.0, 0.0), (10.0, 5.0), (5.0, 7.0), (0.0, 5.0)]
        .iter()
        .map(|&(x, y)| sk.add_point(x, y).unwrap())
        .collect();
    sk.add_line(p[0], p[1]).unwrap();
    sk.add_line(p[1], p[2]).unwrap();
    sk.add_spline(&p[2..5]).unwrap();
    sk.add_line(p[4], p[0]).unwrap();
    let base = add_sketch(&mut s, sk);
    extrude(&mut s, base, 4.0);
    let id = s
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    s.open_sketch(id).unwrap();
    let body = s.bodies.len() - 1;
    let n = s
        .project_into_sketch(ProjectTarget::Body(body), true)
        .unwrap();
    assert_eq!(n, 3, "drei Linien");
    assert_eq!(kinds(&s), vec!["Linie"; 3]);
    let text = s.sketch_session.as_ref().unwrap().message_text().unwrap();
    assert!(
        text.contains(&tr!("sketch-project-skipped", count = 2)),
        "unten und oben je ein Spline: {text}"
    );
    assert!(!text.contains("ließen sich nicht"), "nur Freiform: {text}");
    // Die Freiformkante allein: Fehler mit dem Grund.
    let solid = s.bodies[body].solid.clone();
    let spline = solid
        .edges
        .iter()
        .find(|(_, e)| matches!(e.curve, planora_kernel::Curve::Nurbs(_)))
        .unwrap()
        .0;
    let err = s
        .project_into_sketch(ProjectTarget::Edge(body, spline), true)
        .unwrap_err();
    assert!(err.contains(&tr!("sketch-err-freeform-edge")), "{err}");
    // Die Fläche aus dem Spline: alles übersprungen ⇒ Fehler mit Zahl, nichts angelegt.
    let face = solid
        .faces
        .iter()
        .find(|(_, f)| f.surface.kind() == planora_kernel::SurfaceKind::Nurbs)
        .unwrap()
        .0;
    let err = s
        .project_into_sketch(ProjectTarget::Face(body, face), true)
        .unwrap_err();
    assert!(
        err.contains(&tr!("sketch-project-skipped", count = 2)),
        "{err}"
    );
    assert_eq!(kinds(&s).len(), 3);
}

#[test]
fn cross_hole_gives_lines_and_nothing_skipped() {
    // Quader 20 × 20 × 10 mit senkrechtem Loch r = 2 (Kreise in waagerechten Ebenen); die
    // Skizze liegt auf der Seitenfläche y = 0, steht also senkrecht auf den Kreisen. Wie in
    // Fusion werden die Kreise Linien (Durchmesser), nichts wird übersprungen.
    let mut s = AppState::new();
    s.document
        .add_feature(
            "Quader",
            FeatureKind::Box {
                size: DVec3::new(20.0, 20.0, 10.0),
                origin: DVec3::ZERO,
            },
        )
        .unwrap();
    s.document
        .add_feature(
            "Zylinder",
            FeatureKind::Cylinder {
                radius: 2.0,
                height: 14.0,
                origin: DVec3::new(10.0, 10.0, -2.0),
            },
        )
        .unwrap();
    s.refresh();
    s.add_boolean(0, 1, planora_kernel::boolean::BoolOp::Difference)
        .unwrap();
    let body = s.bodies.len() - 1;
    let side = {
        let solid = &s.bodies[body].solid;
        solid
            .faces
            .iter()
            .find(|(id, _)| {
                planora_sketch::plane_of_face(solid, *id)
                    .is_some_and(|f| f.z.y < -0.5 && f.origin.y.abs() < 1e-9)
            })
            .map(|(id, _)| id)
            .expect("Seitenfläche y = 0")
    };
    let id = s.new_sketch_on_face(body, side).unwrap();
    s.open_sketch(id).unwrap();
    s.observe_sketch(false);
    let n = s
        .project_into_sketch(ProjectTarget::Body(body), true)
        .unwrap();
    assert!(
        s.sketch_session.as_ref().unwrap().message.is_none(),
        "{:?}",
        s.sketch_session.as_ref().unwrap().message_text()
    );
    assert_eq!(kinds(&s), vec!["Linie"; n]);
    let sk = &draft(&s).sketch;
    let mut lengths: Vec<f64> = sk.curves().filter_map(|(c, _)| sk.line_length(c)).collect();
    lengths.sort_by(f64::total_cmp);
    // Rand der Seite (20, 20, 10, 10) und die beiden Lochkreise als Linien der Länge 4.
    assert_eq!(n, 6, "{lengths:?}");
    assert!(
        (lengths[0] - 4.0).abs() < 1e-9 && (lengths[1] - 4.0).abs() < 1e-9,
        "{lengths:?}"
    );
}

#[test]
fn skip_reasons_are_counted_separately() {
    use planora_app::sketch_project::ProjectOutcome;
    let mut s = box_with_sketch_on_top();
    let (se, sk) = s.session_sketch_mut().unwrap();
    let line = planora_kernel::Curve::Line {
        origin: DVec3::new(0.0, 0.0, 5.0),
        dir: DVec3::X,
    };
    // Ein Stück ohne endlichen Bereich geht nicht, ist aber keine Freiform.
    let out = se
        .project_pieces(
            sk,
            &[(line.clone(), 0.0, f64::INFINITY), (line, 0.0, 3.0)],
            &[],
            false,
        )
        .unwrap();
    assert_eq!(out.skipped_freeform, 0);
    assert_eq!(out.skipped_other, 1);
    assert_eq!(out.added.len(), 1);
    assert_ne!(out, ProjectOutcome::default());
    // Die Texte sind verschieden; „andere“ nennt keine Freiform.
    let other = tr!("sketch-project-skipped-other", count = 1);
    assert_ne!(other, tr!("sketch-project-skipped", count = 1));
    assert!(!other.contains("Freiform"), "{other}");
    let en = planora_i18n::with_lang(&planora_i18n::english(), || {
        tr!("sketch-project-skipped-other", count = 2)
    });
    assert!(en.contains('2') && !en.contains("freeform"), "{en}");
}

#[test]
fn project_click_is_one_undo_step() {
    let mut s = box_with_sketch_on_top();
    let steps = undo_len(&s);
    s.project_into_sketch(ProjectTarget::Body(0), true).unwrap();
    s.observe_sketch(false);
    assert_eq!(undo_len(&s), steps + 1, "ein Schritt für den ganzen Klick");
    assert_eq!(s.sketch_undo_label().as_deref(), Some("Projizieren"));
    assert!(s.undo_sketch());
    assert!(kinds(&s).is_empty());
    assert!(draft(&s).copies.is_empty());
}

#[test]
fn esc_leaves_nothing() {
    let mut s = box_with_sketch_on_top();
    let steps = undo_len(&s);
    {
        let (se, sk) = s.session_sketch_mut().unwrap();
        se.set_tool(sk, Tool::ProjectEdge);
        assert_eq!(se.project, ProjectState::default());
        assert_eq!(se.project.filter, ProjectFilter::Entities);
        assert!(se.project.link, "Vorgabe an wie Fusion");
        se.project.filter = ProjectFilter::Bodies;
        se.project.link = false;
        se.cancel(sk);
    }
    s.observe_sketch(false);
    assert!(kinds(&s).is_empty());
    assert!(draft(&s).copies.is_empty());
    assert_eq!(undo_len(&s), steps);
    // Filter und Schalter gelten für die Sitzung, auch über einen Werkzeugwechsel.
    let (se, sk) = s.session_sketch_mut().unwrap();
    se.set_tool(sk, Tool::Line);
    se.set_tool(sk, Tool::ProjectEdge);
    assert_eq!(se.project.filter, ProjectFilter::Bodies);
    assert!(!se.project.link);
}

#[test]
fn pick_becomes_target_by_filter() {
    let edge = Pick::Edge(EdgeRef {
        body: 2,
        edge: Id::from_index(3),
    });
    let face = Pick::Face(FaceRef {
        body: 1,
        face: Id::from_index(4),
    });
    use ProjectFilter::*;
    // Skizzen 6: `project_target` liefert `Option` (Ebenen und Ecken projiziert es nicht).
    assert_eq!(
        sketch_view::project_target(edge, Entities),
        Some(ProjectTarget::Edge(2, Id::from_index(3)))
    );
    assert_eq!(
        sketch_view::project_target(face, Entities),
        Some(ProjectTarget::Face(1, Id::from_index(4)))
    );
    assert_eq!(
        sketch_view::project_target(edge, Bodies),
        Some(ProjectTarget::Body(2))
    );
    assert_eq!(
        sketch_view::project_target(face, Bodies),
        Some(ProjectTarget::Body(1))
    );
    assert_eq!(
        sketch_view::project_target(Pick::Plane(planora_feature::FeatureId(7)), Entities),
        None
    );
}

#[test]
fn project_panel_shows_filter_and_link() {
    let ctx = egui::Context::default();
    ctx.set_fonts(fonts::definitions());
    let mut s = box_with_sketch_on_top();
    {
        let (se, sk) = s.session_sketch_mut().unwrap();
        se.set_tool(sk, Tool::ProjectEdge);
    }
    let raw = || egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1920.0, 1080.0),
        )),
        ..Default::default()
    };
    let mut text = String::new();
    for _ in 0..3 {
        let out = ctx.run_ui(raw(), |ui| sketch_panel::show(ui.ctx(), &mut s));
        text = planora_app::testing::collect_texts(&out);
        out.drop_without_applying_deltas();
    }
    for t in [
        "Auswahlfilter",
        "Angegebene Elemente",
        "Körper",
        "Projektionsverknüpfung",
    ] {
        assert!(text.contains(t), "{t} fehlt: {text}");
    }
    // Ein anderes Werkzeug: kein Feld.
    {
        let (se, sk) = s.session_sketch_mut().unwrap();
        se.set_tool(sk, Tool::Line);
    }
    for _ in 0..3 {
        let out = ctx.run_ui(raw(), |ui| sketch_panel::show(ui.ctx(), &mut s));
        text = planora_app::testing::collect_texts(&out);
        out.drop_without_applying_deltas();
    }
    assert!(!text.contains("Projektionsverknüpfung"));
}

#[test]
fn strip_still_fits_1080() {
    assert_eq!(STRIP_TOOLS, 20);
    assert_eq!(TOOL_ORDER.len(), 20);
    // Alle 20 Knöpfe liegen bei 1920 × 1080 ganz im Fenster, ohne Rollen.
    let ctx = egui::Context::default();
    ctx.set_fonts(fonts::definitions());
    let mut s = box_with_sketch_on_top();
    let raw = || egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1920.0, 1080.0),
        )),
        ..Default::default()
    };
    for _ in 0..3 {
        ctx.run_ui(raw(), |ui| sketch_panel::show(ui.ctx(), &mut s))
            .drop_without_applying_deltas();
    }
    let buttons = sketch_panel::strip_buttons(&ctx);
    assert_eq!(buttons.len(), 20);
    let top = buttons
        .iter()
        .map(|(_, r)| r.top())
        .fold(f32::MAX, f32::min);
    let bottom = buttons.iter().map(|(_, r)| r.bottom()).fold(0.0, f32::max);
    assert!(bottom - top <= STRIP_H, "{top}…{bottom}");
    for (label, r) in &buttons {
        assert!(r.top() >= 0.0 && r.bottom() <= 1080.0, "{label} {r:?}");
    }
    assert!(buttons.iter().any(|(l, _)| l == "Projizieren/Einschließen"));
    // Der Knopf ist ein Klappmenü „Projizieren/Einschließen“ mit dem Eintrag „Projizieren“ (P).
    // Skizzen 6, Task 10: bewusst angepasst, „Schneiden“ und „Silhouette“ stehen dahinter.
    assert!(sketch_panel::has_flyout(Tool::ProjectEdge));
    assert_eq!(
        ToolKind::of(Tool::ProjectEdge),
        &[ToolKind::Project, ToolKind::Intersect, ToolKind::Silhouette]
    );
    assert_eq!(
        sketch_panel::tool_label(Tool::ProjectEdge),
        "Projizieren/Einschließen"
    );
    assert_eq!(ToolKind::Project.label(), "Projizieren");
    assert_eq!(
        planora_app::keymap::Action::SketchProjectEdge.id(),
        "skizze.projizieren"
    );
    let en = planora_i18n::with_lang(&planora_i18n::english(), || {
        (
            sketch_panel::tool_label(Tool::ProjectEdge),
            ToolKind::Project.label(),
        )
    });
    assert_eq!(en, ("Project/Include".to_string(), "Project".to_string()));
}
