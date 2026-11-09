//! Skizzen 6, Task 10: Fusion „Schneiden“ und „Silhouette“ in der Skizze. Die Kurven sind fest,
//! normal (nicht Konstruktion) und folgen dem Körper; Fehler des Rechners erscheinen als Meldung
//! des Befehls, und es entsteht nichts (ohne Fenster, 1920×1080).

use planora_app::keymap::Action;
use planora_app::shell::STRIP_TOOLS;
use planora_app::sketch_kinds::ToolKind;
use planora_app::sketch_panel;
use planora_app::sketch_state::Tool;
use planora_app::state::{AppState, ProjectTarget, StandardPlane};
use planora_feature::planes::{AxisRef, PlaneBase, PlaneDef, PlaneFeature, StdAxis};
use planora_feature::{CopySource, FeatureId, FeatureKind, ProfileSel, SketchFeature, SketchPlane};
use planora_i18n::tr;
use planora_kernel::{Face, Id, SurfaceKind};
use planora_math::{DVec3, Frame, Tolerance};
use planora_sketch::model::{CurveId, CurveKind, Sketch};

fn draft(s: &AppState) -> &SketchFeature {
    s.sketch_draft.as_ref().unwrap()
}

fn kinds(sk: &Sketch) -> Vec<&'static str> {
    sk.curves()
        .map(|(_, c)| match c.kind {
            CurveKind::Line { .. } => "Linie",
            CurveKind::Circle { .. } => "Kreis",
            CurveKind::Arc { .. } => "Bogen",
            CurveKind::Ellipse { .. } => "Ellipse",
            CurveKind::EllipticArc { .. } => "Ellipsenbogen",
            CurveKind::Spline { .. } => "Spline",
            CurveKind::ControlSpline { .. } => "Spline mit Kontrollpunkten",
            CurveKind::Conic { .. } => "Kegelschnitt",
            CurveKind::FixedSpline { .. } => "Fester Spline",
        })
        .collect()
}

fn undo_len(s: &AppState) -> usize {
    s.sketch_history().unwrap().undo_len()
}

/// Die erste Fläche des Körpers `body` mit der Flächenart `kind`.
fn face_of_kind(s: &AppState, body: usize, kind: SurfaceKind) -> Id<Face> {
    s.bodies[body]
        .solid
        .faces
        .iter()
        .find(|(_, f)| f.surface.kind() == kind)
        .map(|(id, _)| id)
        .expect("Fläche der Art")
}

/// Halbachsen (klein, groß) der Ellipse `c`.
fn semi_axes(sk: &Sketch, c: CurveId) -> (f64, f64) {
    let CurveKind::Ellipse { center, .. } = sk.curve(c).unwrap().kind else {
        panic!("keine Ellipse");
    };
    let m = sk.point_xy(center).unwrap();
    let a = sk.ellipse_point(c, 0.0).unwrap().distance(m);
    let b = sk
        .ellipse_point(c, std::f64::consts::FRAC_PI_2)
        .unwrap()
        .distance(m);
    (a.min(b), a.max(b))
}

/// Zylinder r = 5, Höhe 20 um die z-Achse von z = −10 bis 10, eine um 30° um X gedrehte
/// Konstruktionsebene und eine offene Skizze darauf (über das Feature, die Oberfläche für Ebenen
/// kommt mit Task 12). Die Ebene schneidet nur den Mantel.
fn cylinder_with_tilted_sketch() -> (AppState, FeatureId, FeatureId) {
    let mut s = AppState::new();
    let cyl = s
        .document
        .add_feature(
            "Zylinder",
            FeatureKind::Cylinder {
                radius: 5.0,
                height: 20.0,
                origin: DVec3::new(0.0, 0.0, -10.0),
            },
        )
        .unwrap();
    let plane = s
        .document
        .add_feature(
            "Ebene",
            FeatureKind::Plane(PlaneFeature {
                def: PlaneDef::Angle {
                    axis: AxisRef::Standard(StdAxis::X),
                    angle: 30.0,
                },
            }),
        )
        .unwrap();
    s.refresh();
    let id = s.add_sketch(SketchPlane::OnPlane(plane)).unwrap();
    s.open_sketch(id).unwrap();
    s.observe_sketch(false);
    (s, cyl, id)
}

fn set_radius(s: &mut AppState, cyl: FeatureId, r: f64) {
    s.document
        .update(cyl, |k| {
            if let FeatureKind::Cylinder { radius, .. } = k {
                *radius = r;
            }
        })
        .unwrap();
    s.refresh();
}

#[test]
fn intersect_cylinder_gives_ellipse_on_tilted_plane() {
    let (mut s, _, _) = cylinder_with_tilted_sketch();
    let n = s.intersect_into_sketch(ProjectTarget::Body(0)).unwrap();
    assert_eq!(n, 1);
    let sk = &draft(&s).sketch;
    assert_eq!(kinds(sk), vec!["Ellipse"]);
    let (c, curve) = sk.curves().next().unwrap();
    assert!(!curve.construction, "normale Geometrie wie Fusion");
    let (minor, major) = semi_axes(sk, c);
    let cos30 = 30f64.to_radians().cos();
    assert!((minor - 5.0).abs() < 1e-7, "{minor}");
    assert!((major - 5.0 / cos30).abs() < 1e-7, "{major}");
    // Verknüpft mit dem Mantel, erstes Stück dieser Fläche.
    let copies = &draft(&s).copies;
    assert_eq!(copies.len(), 1);
    assert_eq!(copies[0].curve, c);
    assert!(matches!(
        copies[0].source,
        CopySource::Section { index: 0, .. }
    ));
    assert!(s.sketch_session.as_ref().unwrap().message.is_none());
    // Die Mantelfläche allein ergibt dasselbe; es ist schon da ⇒ nichts Neues.
    let side = face_of_kind(&s, 0, SurfaceKind::Cylinder);
    let err = s
        .intersect_into_sketch(ProjectTarget::Face(0, side))
        .unwrap_err();
    assert_eq!(err, tr!("sketch-intersect-nothing-new"));
    // Eine Deckfläche trifft die Ebene nicht.
    let cap = face_of_kind(&s, 0, SurfaceKind::Plane);
    let err = s
        .intersect_into_sketch(ProjectTarget::Face(0, cap))
        .unwrap_err();
    assert_eq!(err, tr!("sketch-intersect-nothing"));
    // Kanten lassen sich nicht schneiden.
    let edge = s.bodies[0].solid.edges.iter().next().unwrap().0;
    assert_eq!(
        s.intersect_into_sketch(ProjectTarget::Edge(0, edge))
            .unwrap_err(),
        tr!("sketch-pick-intersect")
    );
    assert_eq!(draft(&s).copies.len(), 1);
}

#[test]
fn intersect_follows_radius_change() {
    let (mut s, cyl, id) = cylinder_with_tilted_sketch();
    s.intersect_into_sketch(ProjectTarget::Body(0)).unwrap();
    s.close_sketch();
    let f = s.document.feature(id).unwrap();
    assert!(f.warning().is_none(), "{:?}", f.warning());
    set_radius(&mut s, cyl, 8.0);
    let f = s.document.feature(id).unwrap();
    assert!(f.warning().is_none(), "{:?}", f.warning());
    let sk = s.document.sketch_of(id).unwrap();
    assert_eq!(kinds(sk), vec!["Ellipse"]);
    let c = sk.curves().next().unwrap().0;
    let (minor, major) = semi_axes(sk, c);
    let cos30 = 30f64.to_radians().cos();
    assert!((minor - 8.0).abs() < 1e-7, "{minor}");
    assert!((major - 8.0 / cos30).abs() < 1e-7, "{major}");
}

#[test]
fn intersect_freeform_reports_and_creates_nothing() {
    let mut s = AppState::new();
    // Profil aus drei Linien und einem Spline (wie tests/project.rs), 4 hoch extrudiert.
    let mut sk = Sketch::new(Frame::WORLD, "Skizze", Tolerance::default());
    let p: Vec<_> = [(0.0, 0.0), (10.0, 0.0), (10.0, 5.0), (5.0, 7.0), (0.0, 5.0)]
        .iter()
        .map(|&(x, y)| sk.add_point(x, y).unwrap())
        .collect();
    sk.add_line(p[0], p[1]).unwrap();
    sk.add_line(p[1], p[2]).unwrap();
    sk.add_spline(&p[2..5]).unwrap();
    sk.add_line(p[4], p[0]).unwrap();
    let base = s
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
    s.document
        .add_feature(
            "Extrusion",
            FeatureKind::Extrude {
                sketch: base,
                profiles: ProfileSel::Index(0),
                height: 4.0,
            },
        )
        .unwrap();
    // Ebene 2 über XY: schneidet die Freiformfläche mitten durch.
    let plane = s
        .document
        .add_feature(
            "Ebene",
            FeatureKind::Plane(PlaneFeature {
                def: PlaneDef::Offset {
                    base: PlaneBase::Standard(StandardPlane::XY),
                    distance: 2.0,
                },
            }),
        )
        .unwrap();
    s.refresh();
    let id = s.add_sketch(SketchPlane::OnPlane(plane)).unwrap();
    s.open_sketch(id).unwrap();
    s.observe_sketch(false);
    let body = s.bodies.len() - 1;
    let steps = undo_len(&s);
    let err = s
        .intersect_into_sketch(ProjectTarget::Body(body))
        .unwrap_err();
    assert_eq!(err, tr!("sketch-intersect-freeform"));
    assert!(!err.contains("Ebenenschnitt"), "Text des Befehls: {err}");
    assert_eq!(
        s.sketch_session.as_ref().unwrap().message_text().as_deref(),
        Some(err.as_str())
    );
    assert!(
        draft(&s).sketch.curves().next().is_none(),
        "nichts angelegt"
    );
    assert!(draft(&s).copies.is_empty());
    s.observe_sketch(false);
    assert_eq!(undo_len(&s), steps, "kein Schritt");
    // Die Freiformfläche allein: derselbe Text; Silhouette hat ihren eigenen.
    let nurbs = face_of_kind(&s, body, SurfaceKind::Nurbs);
    assert_eq!(
        s.intersect_into_sketch(ProjectTarget::Face(body, nurbs))
            .unwrap_err(),
        tr!("sketch-intersect-freeform")
    );
    assert_eq!(
        s.silhouette_into_sketch(body, nurbs).unwrap_err(),
        tr!("sketch-silhouette-freeform")
    );
    assert!(draft(&s).sketch.curves().next().is_none());
}

#[test]
fn silhouette_of_cylinder_side() {
    // Zylinder um z, Skizze auf XZ (Blick entlang der Normalen): die Mantellinien bei x = ±r.
    let mut s = AppState::new();
    s.create_cylinder(5.0, 20.0).unwrap();
    let id = s
        .add_sketch(SketchPlane::Standard(StandardPlane::XZ))
        .unwrap();
    s.open_sketch(id).unwrap();
    s.observe_sketch(false);
    let side = face_of_kind(&s, 0, SurfaceKind::Cylinder);
    let n = s.silhouette_into_sketch(0, side).unwrap();
    assert_eq!(n, 2);
    let sk = &draft(&s).sketch;
    assert_eq!(kinds(sk), vec!["Linie"; 2]);
    let mut xs: Vec<f64> = sk
        .curves()
        .map(|(c, curve)| {
            assert!(!curve.construction);
            let CurveKind::Line { a, b } = curve.kind else {
                unreachable!()
            };
            let (pa, pb) = (sk.point_xy(a).unwrap(), sk.point_xy(b).unwrap());
            assert!((pa.x - pb.x).abs() < 1e-7, "senkrecht: {c:?}");
            assert!((pa.distance(pb) - 20.0).abs() < 1e-7, "ganze Höhe");
            pa.x
        })
        .collect();
    xs.sort_by(f64::total_cmp);
    // Der erste Zylinder steht im Ursprung; die x-Achse der XZ-Skizze ist die Welt-x-Achse.
    assert!((xs[0] + 5.0).abs() < 1e-7, "{xs:?}");
    assert!((xs[1] - 5.0).abs() < 1e-7, "{xs:?}");
    let indices: Vec<usize> = draft(&s)
        .copies
        .iter()
        .map(|c| match c.source {
            CopySource::Silhouette { index, .. } => index,
            ref other => panic!("{other:?}"),
        })
        .collect();
    assert_eq!(indices, vec![0, 1]);
    // Eine ebene Fläche hat keinen Umriss.
    let cap = face_of_kind(&s, 0, SurfaceKind::Plane);
    assert_eq!(
        s.silhouette_into_sketch(0, cap).unwrap_err(),
        tr!("sketch-silhouette-nothing")
    );
    // Beim Beenden bleibt alles, ohne Warnung.
    s.close_sketch();
    let f = s.document.feature(id).unwrap();
    assert!(f.warning().is_none(), "{:?}", f.warning());
    assert_eq!(s.document.sketch_of(id).unwrap().curves().count(), 2);
}

#[test]
fn intersect_is_one_undo_step() {
    let (mut s, _, _) = cylinder_with_tilted_sketch();
    let steps = undo_len(&s);
    s.intersect_into_sketch(ProjectTarget::Body(0)).unwrap();
    s.observe_sketch(false);
    assert_eq!(undo_len(&s), steps + 1, "ein Schritt für den ganzen Klick");
    assert_eq!(s.sketch_undo_label().as_deref(), Some("Schneiden"));
    assert!(s.undo_sketch());
    assert!(draft(&s).sketch.curves().next().is_none());
    assert!(draft(&s).copies.is_empty());
    // Silhouette ebenso, mit eigenem Namen.
    let mut s = AppState::new();
    s.create_cylinder(5.0, 20.0).unwrap();
    let id = s
        .add_sketch(SketchPlane::Standard(StandardPlane::XZ))
        .unwrap();
    s.open_sketch(id).unwrap();
    s.observe_sketch(false);
    let steps = undo_len(&s);
    let side = face_of_kind(&s, 0, SurfaceKind::Cylinder);
    s.silhouette_into_sketch(0, side).unwrap();
    s.observe_sketch(false);
    assert_eq!(undo_len(&s), steps + 1);
    assert_eq!(s.sketch_undo_label().as_deref(), Some("Silhouette"));
}

#[test]
fn flyout_has_intersect_and_silhouette_without_keys() {
    assert_eq!(
        ToolKind::of(Tool::ProjectEdge),
        &[ToolKind::Project, ToolKind::Intersect, ToolKind::Silhouette]
    );
    assert_eq!(STRIP_TOOLS, 20);
    assert_eq!(sketch_panel::TOOL_ORDER.len(), 20);
    assert_eq!(ToolKind::Intersect.action(), Action::SketchIntersect);
    assert_eq!(ToolKind::Silhouette.action(), Action::SketchSilhouette);
    assert_eq!(Action::SketchIntersect.id(), "skizze.schneiden");
    assert_eq!(Action::SketchSilhouette.id(), "skizze.silhouette");
    assert!(Action::SketchIntersect.default_bindings().is_empty());
    assert!(Action::SketchSilhouette.default_bindings().is_empty());
    assert_eq!(ToolKind::Intersect.label(), "Schneiden");
    assert_eq!(ToolKind::Silhouette.label(), "Silhouette");
    assert_eq!(
        Action::SketchIntersect.sketch_tool(),
        Some(Tool::ProjectEdge)
    );
}

#[test]
fn panel_shows_filter_for_intersect_and_nothing_for_silhouette() {
    let ctx = egui::Context::default();
    ctx.set_fonts(planora_app::fonts::definitions());
    let (mut s, _, _) = cylinder_with_tilted_sketch();
    let raw = || egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1920.0, 1080.0),
        )),
        ..Default::default()
    };
    let texts = |s: &mut AppState, kind: ToolKind| {
        {
            let (se, sk) = s.session_sketch_mut().unwrap();
            se.choose_kind(sk, kind);
        }
        let mut text = String::new();
        for _ in 0..3 {
            let out = ctx.run_ui(raw(), |ui| sketch_panel::show(ui.ctx(), s));
            text = planora_app::testing::collect_texts(&out);
            out.drop_without_applying_deltas();
        }
        text
    };
    let text = texts(&mut s, ToolKind::Intersect);
    assert!(text.contains("Auswahlfilter"), "{text}");
    assert!(!text.contains("Projektionsverknüpfung"), "immer verknüpft");
    let text = texts(&mut s, ToolKind::Silhouette);
    assert!(!text.contains("Auswahlfilter"), "{text}");
    let text = texts(&mut s, ToolKind::Project);
    assert!(text.contains("Projektionsverknüpfung"), "{text}");
}

/// Skizzen 7, Task 11: Der schräge Kegel wird nicht mehr abgelehnt. Hyperbel als fester Spline,
/// dazu die Sehne auf dem Boden; ein zweiter Aufruf legt keine Doppelten an.
#[test]
fn intersect_oblique_cone_gives_a_fixed_spline() {
    let mut s = AppState::new();
    s.document
        .add_feature(
            "Kegel",
            FeatureKind::Cone {
                radius: 5.0,
                height: 10.0,
                origin: DVec3::new(0.0, 2.0, 0.0),
            },
        )
        .unwrap();
    s.refresh();
    let id = s
        .add_sketch(SketchPlane::Standard(StandardPlane::XZ))
        .unwrap();
    s.open_sketch(id).unwrap();
    s.observe_sketch(false);
    let before = undo_len(&s);
    let n = s.intersect_into_sketch(ProjectTarget::Body(0)).unwrap();
    assert_eq!(n, 2);
    s.observe_sketch(false);
    assert_eq!(undo_len(&s), before + 1, "ein Rückgängig-Schritt");
    let mut found = kinds(&draft(&s).sketch);
    found.sort_unstable();
    assert_eq!(found, vec!["Fester Spline", "Linie"]);
    assert!(s.sketch_session.as_ref().unwrap().message.is_none());
    let cone = face_of_kind(&s, 0, SurfaceKind::Cone);
    let err = s
        .intersect_into_sketch(ProjectTarget::Face(0, cone))
        .unwrap_err();
    assert_eq!(err, tr!("sketch-intersect-nothing-new"));
}
