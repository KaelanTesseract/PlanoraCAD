//! Skizzen 10, Task 16: Werkzeug „3D-Geometrie einbeziehen“ (Spec 5, A4), Fang an Körperecken
//! (A7) und Projizieren/Schneiden mit Kurven fertiger 3D-Skizzen (Spec 3). Bei 1920 × 1080.

use egui::{Event, Modifiers, PointerButton, Pos2, Rect, pos2};
use glam::Vec3;
use planora_app::keymap::Action;
use planora_app::sketch_kinds::ToolKind;
use planora_app::sketch_state::{Tool, TriadPlane};
use planora_app::sketch_view::{self, Mapper};
use planora_app::state::{AppState, IncludePick, ProjectTarget, StandardPlane};
use planora_app::{fonts, theme};
use planora_feature::{
    CopySource, CopyTarget, FeatureId, IncludeRef, PlaneId, SketchFeature, SketchPlane,
    include_plane_half,
};
use planora_math::DVec3;
use planora_render::OrbitCamera;
use planora_sketch::constraint::ConstraintKind;
use planora_sketch::model::{CurveId, CurveKind, PointId, Sketch};

const W: f32 = 1920.0;
const H: f32 = 1080.0;
const SCREEN: Rect = Rect {
    min: pos2(0.0, 0.0),
    max: pos2(W, H),
};

// ---- Hilfen ---------------------------------------------------------------------------------------

/// Quader 10 × 20 × 5 und eine offene Skizze auf XY (ohne Haken „3D-Skizze“).
fn box_and_sketch() -> AppState {
    let mut s = AppState::new();
    s.create_box(DVec3::new(10.0, 20.0, 5.0)).unwrap();
    open_new(&mut s, StandardPlane::XY);
    s
}

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

fn near(a: DVec3, b: DVec3) -> bool {
    a.distance(b) < 1e-6
}

/// Eine Kante oben auf dem Quader (beide Enden bei z = 5), als Index des Körpers und Kante.
fn top_edge(s: &AppState) -> planora_kernel::Id<planora_kernel::Edge> {
    let solid = &s.bodies[0].solid;
    solid
        .edges
        .iter()
        .find(|(_, e)| {
            let a = solid.vertices.get(e.start).unwrap().point;
            let b = solid.vertices.get(e.end).unwrap().point;
            (a.z - 5.0).abs() < 1e-9 && (b.z - 5.0).abs() < 1e-9
        })
        .map(|(id, _)| id)
        .expect("Kante oben")
}

/// Die obere Fläche des Quaders (Normale +Z bei z = 5).
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

fn lines(sk: &Sketch) -> Vec<(CurveId, PointId, PointId)> {
    sk.curves()
        .filter_map(|(c, d)| match d.kind {
            CurveKind::Line { a, b } => Some((c, a, b)),
            _ => None,
        })
        .collect()
}

/// Eine fertige 3D-Skizze auf XY mit einer Linie von `a` nach `b` (Skizzenkoordinaten = Welt);
/// liefert Skizze und Linie. Die Skizze wird geschlossen und berechnet.
fn finished_3d_line(s: &mut AppState, a: DVec3, b: DVec3) -> (FeatureId, CurveId) {
    let id = open_new(s, StandardPlane::XY);
    {
        let (se, sk) = s.session_sketch_mut().unwrap();
        se.set_three_d(sk, true);
        se.choose_tool(sk, Tool::Line);
        se.place_point_3d(sk, a, None).unwrap();
        se.place_point_3d(sk, b, None).unwrap();
        se.choose_tool(sk, Tool::Select);
    }
    s.observe_sketch(false);
    let c = lines(sk(s))[0].0;
    s.close_sketch();
    (id, c)
}

// ---- Einbeziehen ----------------------------------------------------------------------------------

#[test]
fn include_box_edge_gives_purple_3d_line() {
    let mut s = box_and_sketch();
    let e = top_edge(&s);
    s.include_into_sketch(IncludePick::Edge(0, e)).unwrap();
    s.observe_sketch(false);
    let solid = s.bodies[0].solid.clone();
    let edge = solid.edges.get(e).unwrap();
    let (a0, b0) = (
        solid.vertices.get(edge.start).unwrap().point,
        solid.vertices.get(edge.end).unwrap().point,
    );
    let ls = lines(sk(&s));
    assert_eq!(ls.len(), 1);
    let (c, a, b) = ls[0];
    let sk = sk(&s);
    assert!(sk.is_3d_curve(c), "die Kante liegt über der Ebene");
    let (pa, pb) = (sk.point_xyz(a).unwrap(), sk.point_xyz(b).unwrap());
    assert!(
        (near(pa, a0) && near(pb, b0)) || (near(pa, b0) && near(pb, a0)),
        "Raumlage {pa} {pb}"
    );
    // Verknüpft mit der Kante (A4) und lila wie eine Projektion.
    let copies = &draft(&s).copies;
    assert_eq!(copies.len(), 1);
    assert_eq!(copies[0].target, CopyTarget::Curve(c));
    assert!(matches!(
        copies[0].source,
        CopySource::Include(IncludeRef::Edge(_))
    ));
    let col = theme::sketch_colors(s.dark);
    assert_eq!(sketch_view::curve_color(&s, c), col.projected);
    assert_eq!(sketch_view::point_color(&s, a), col.projected);
    // Der Schritt heißt wie der Befehl.
    assert_eq!(
        s.sketch_undo_label().as_deref(),
        Some("3D-Geometrie einbeziehen")
    );
}

#[test]
fn include_face_gives_boundary() {
    let mut s = box_and_sketch();
    let f = top_face(&s);
    s.include_into_sketch(IncludePick::Face(0, f)).unwrap();
    s.observe_sketch(false);
    let ls = lines(sk(&s));
    assert_eq!(ls.len(), 4, "vier Randkanten");
    for &(c, a, b) in &ls {
        assert!(sk(&s).is_3d_curve(c));
        assert!((sk(&s).point_xyz(a).unwrap().z - 5.0).abs() < 1e-9);
        assert!((sk(&s).point_xyz(b).unwrap().z - 5.0).abs() < 1e-9);
    }
    // Eine Gruppe: dieselbe Herkunft, Stücke 0 … 3 von 4.
    let copies = &draft(&s).copies;
    assert_eq!(copies.len(), 4);
    let mut pieces: Vec<u32> = copies.iter().map(|c| c.piece).collect();
    pieces.sort_unstable();
    assert_eq!(pieces, vec![0, 1, 2, 3]);
    assert!(copies.iter().all(|c| c.of == 4));
    assert!(copies.iter().all(|c| c.source == copies[0].source));
    assert!(matches!(
        copies[0].source,
        CopySource::Include(IncludeRef::FaceBoundary(_))
    ));
}

#[test]
fn include_plane_gives_four_lines() {
    let mut s = box_and_sketch();
    s.include_into_sketch(IncludePick::Plane(PlaneId::Standard(StandardPlane::XZ)))
        .unwrap();
    s.observe_sketch(false);
    let ls = lines(sk(&s));
    assert_eq!(ls.len(), 4);
    // Die Größe misst wie die Neuberechnung (Ecken der sichtbaren Körper).
    let solid = s.bodies[0].solid.clone();
    let h = include_plane_half(&[&solid]);
    for &(_, a, b) in &ls {
        let (pa, pb) = (sk(&s).point_xyz(a).unwrap(), sk(&s).point_xyz(b).unwrap());
        assert!(pa.y.abs() < 1e-9 && pb.y.abs() < 1e-9, "in der Ebene XZ");
        assert!((pa.distance(pb) - 2.0 * h).abs() < 1e-6);
    }
    assert!(
        draft(&s)
            .copies
            .iter()
            .all(|c| c.of == 4 && matches!(c.source, CopySource::Include(IncludeRef::Plane(_))))
    );
    // Beenden und neu berechnen: Die Gruppe passt (keine Warnung), die Lage bleibt.
    let id = s.open_sketch_id().unwrap();
    s.close_sketch();
    let f = s.document.feature(id).unwrap();
    assert_eq!(
        f.status,
        planora_feature::FeatureStatus::Ok,
        "{:?}",
        f.status
    );
}

#[test]
fn include_sketch_curve_from_earlier_sketch() {
    let mut s = AppState::new();
    let (a, b) = (DVec3::new(0.0, 10.0, 0.0), DVec3::new(0.0, 10.0, 20.0));
    let (first, c) = finished_3d_line(&mut s, a, b);
    open_new(&mut s, StandardPlane::XZ);
    s.include_into_sketch(IncludePick::SketchCurve {
        sketch: first,
        curve: c,
    })
    .unwrap();
    s.observe_sketch(false);
    let ls = lines(sk(&s));
    assert_eq!(ls.len(), 1);
    let frame = *sk(&s).frame();
    let world = |p: PointId| {
        let q = sk(&s).point_xyz(p).unwrap();
        frame.point(q.x, q.y, q.z)
    };
    let (pa, pb) = (world(ls[0].1), world(ls[0].2));
    assert!(
        (near(pa, a) && near(pb, b)) || (near(pa, b) && near(pb, a)),
        "{pa} {pb}"
    );
    assert!(matches!(
        draft(&s).copies[0].source,
        CopySource::Include(IncludeRef::SketchCurve { .. })
    ));
    // Eine Skizze nach der aktiven ist keine Quelle (Spec 10).
    let me = s.open_sketch_id().unwrap();
    s.close_sketch();
    let later = open_new(&mut s, StandardPlane::XY);
    s.close_sketch();
    s.open_sketch(me).unwrap();
    s.observe_sketch(false);
    let n = sk(&s).curves().count();
    assert!(
        s.include_into_sketch(IncludePick::SketchCurve {
            sketch: later,
            curve: c,
        })
        .is_err()
    );
    assert!(
        s.include_into_sketch(IncludePick::SketchCurve {
            sketch: me,
            curve: c
        })
        .is_err(),
        "nicht aus sich selbst"
    );
    assert_eq!(sk(&s).curves().count(), n);
}

#[test]
fn include_works_without_three_d_flag() {
    let mut s = box_and_sketch();
    assert!(!sk(&s).three_d);
    s.include_into_sketch(IncludePick::Edge(0, top_edge(&s)))
        .unwrap();
    let c = lines(sk(&s))[0].0;
    assert!(sk(&s).is_3d_curve(c), "trotzdem 3D");
    assert!(!sk(&s).three_d, "der Haken bleibt aus");
    // Ecke, Ursprung und Achse gehen ebenso.
    let solid = s.bodies[0].solid.clone();
    let (edge, e) = solid.edges.iter().next().unwrap();
    s.include_into_sketch(IncludePick::Vertex {
        body: 0,
        edge,
        end: false,
    })
    .unwrap();
    let corner = solid.vertices.get(e.start).unwrap().point;
    let copy = draft(&s).copies.last().unwrap().clone();
    let CopyTarget::Point(p) = copy.target else {
        panic!("Ecke wird ein Punkt: {copy:?}");
    };
    assert!(near(sk(&s).point_xyz(p).unwrap(), corner));
    s.include_into_sketch(IncludePick::Origin).unwrap();
    s.include_into_sketch(IncludePick::Axis(planora_feature::AxisId::Z))
        .unwrap();
    assert_eq!(draft(&s).copies.len(), 4);
    // Konstruktionspunkte gibt es noch nicht: Meldung, nichts entsteht.
    assert!(
        s.include_into_sketch(IncludePick::ConstructionPoint(FeatureId(999)))
            .is_err()
    );
    assert_eq!(draft(&s).copies.len(), 4);
}

#[test]
fn one_include_click_one_undo_step() {
    let mut s = box_and_sketch();
    let n = undo_len(&s);
    s.include_into_sketch(IncludePick::Face(0, top_face(&s)))
        .unwrap();
    s.observe_sketch(false);
    assert_eq!(undo_len(&s), n + 1, "vier Kurven, ein Schritt");
    assert!(s.undo_sketch());
    assert_eq!(sk(&s).curves().count(), 0);
    assert!(draft(&s).copies.is_empty());
}

#[test]
fn include_kind_in_flyout_without_key() {
    assert_eq!(
        ToolKind::of(Tool::ProjectEdge),
        &[
            ToolKind::Project,
            ToolKind::Intersect,
            ToolKind::Silhouette,
            ToolKind::Include3d
        ]
    );
    assert_eq!(ToolKind::Include3d.tool(), Tool::ProjectEdge);
    assert_eq!(ToolKind::Include3d.action(), Action::SketchInclude3d);
    assert!(Action::SketchInclude3d.default_bindings().is_empty());
    assert_eq!(ToolKind::Include3d.label(), "3D-Geometrie einbeziehen");
    let en = planora_i18n::with_lang(&planora_i18n::english(), || ToolKind::Include3d.label());
    assert_eq!(en, "Include 3D Geometry");
}

// ---- Projizieren und Schneiden mit 3D-Skizzenkurven ------------------------------------------------

#[test]
fn project_from_3d_sketch_into_other_sketch() {
    let mut s = AppState::new();
    // Linie schräg im Raum; in die Ebene XZ geworfen bleibt (x, z).
    let (a, b) = (DVec3::new(2.0, 10.0, 3.0), DVec3::new(12.0, 4.0, 20.0));
    let (first, c) = finished_3d_line(&mut s, a, b);
    open_new(&mut s, StandardPlane::XZ);
    let n = undo_len(&s);
    let added = s
        .project_into_sketch(ProjectTarget::SketchCurve(first, c), true)
        .unwrap();
    s.observe_sketch(false);
    assert_eq!(added, 1);
    assert_eq!(undo_len(&s), n + 1);
    let ls = lines(sk(&s));
    assert_eq!(ls.len(), 1);
    assert!(!sk(&s).is_3d_curve(ls[0].0), "projiziert: in der Ebene");
    let frame = *sk(&s).frame();
    let world = |p: PointId| {
        let q = sk(&s).point_xyz(p).unwrap();
        frame.point(q.x, q.y, q.z)
    };
    let flat = |p: DVec3| DVec3::new(p.x, 0.0, p.z);
    let (pa, pb) = (world(ls[0].1), world(ls[0].2));
    assert!(
        (near(pa, flat(a)) && near(pb, flat(b))) || (near(pa, flat(b)) && near(pb, flat(a))),
        "{pa} {pb}"
    );
    assert_eq!(
        draft(&s).copies[0].source,
        CopySource::SketchCurve {
            sketch: first,
            curve: c
        }
    );
}

#[test]
fn intersect_3d_sketch_curve_with_plane_gives_points() {
    let mut s = AppState::new();
    // Senkrecht durch die Ebene XZ (y = 0): ein Schnittpunkt bei (4, 0, 6).
    let (a, b) = (DVec3::new(4.0, -10.0, 6.0), DVec3::new(4.0, 10.0, 6.0));
    let (first, c) = finished_3d_line(&mut s, a, b);
    open_new(&mut s, StandardPlane::XZ);
    let n = undo_len(&s);
    let added = s
        .intersect_into_sketch(ProjectTarget::SketchCurve(first, c))
        .unwrap();
    s.observe_sketch(false);
    assert_eq!(added, 1);
    assert_eq!(undo_len(&s), n + 1);
    let copies = draft(&s).copies.clone();
    assert_eq!(copies.len(), 1);
    let CopyTarget::Point(p) = copies[0].target else {
        panic!("Schnitt ergibt Punkte: {copies:?}");
    };
    let frame = *sk(&s).frame();
    let q = sk(&s).point_xyz(p).unwrap();
    assert!(near(frame.point(q.x, q.y, q.z), DVec3::new(4.0, 0.0, 6.0)));
    assert_eq!(
        copies[0].source,
        CopySource::SketchCurve {
            sketch: first,
            curve: c
        }
    );
    // Eine Kurve in der Ebene schneidet nicht: Meldung, nichts entsteht.
    let (flat_sketch, flat) = {
        s.close_sketch();
        finished_3d_line(
            &mut s,
            DVec3::new(0.0, 0.0, 0.0),
            DVec3::new(10.0, 0.0, 10.0),
        )
    };
    open_new(&mut s, StandardPlane::XZ);
    assert!(
        s.intersect_into_sketch(ProjectTarget::SketchCurve(flat_sketch, flat))
            .is_err()
    );
    assert!(draft(&s).copies.is_empty());
}

// ---- Fang an Körperecken (A7) ------------------------------------------------------------------------

fn context() -> egui::Context {
    let ctx = egui::Context::default();
    ctx.set_fonts(fonts::definitions());
    theme::apply(&ctx, true);
    ctx
}

struct View {
    ctx: egui::Context,
    s: AppState,
    time: f64,
}

impl View {
    fn frame(&mut self, events: Vec<Event>) {
        self.time += 1.0 / 60.0;
        let raw = egui::RawInput {
            screen_rect: Some(SCREEN),
            time: Some(self.time),
            events,
            ..Default::default()
        };
        let s = &mut self.s;
        let mut gesture = false;
        let out = self.ctx.run_ui(raw, |ui| {
            sketch_view::show_overlay(ui, SCREEN, s);
            gesture = ui.input(|i| i.pointer.any_down());
        });
        out.drop_without_applying_deltas();
        self.s.observe_sketch(gesture);
    }

    fn at3(&self, p: DVec3) -> Pos2 {
        let mut cam = self.s.camera;
        cam.set_viewport(W, H);
        let map = Mapper {
            camera: cam,
            frame: *self.s.session_sketch().unwrap().frame(),
            rect: SCREEN,
        };
        map.to_screen3(p).expect("vor der Kamera")
    }

    fn click(&mut self, p: Pos2) {
        self.time += 1.0;
        self.frame(vec![Event::PointerMoved(p)]);
        self.frame(vec![Event::PointerMoved(p)]);
        self.frame(vec![button(p, true)]);
        self.frame(vec![button(p, false)]);
        self.frame(vec![]);
    }
}

fn button(p: Pos2, pressed: bool) -> Event {
    Event::PointerButton {
        pos: p,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    }
}

#[test]
fn snap_to_body_vertex_includes_point_with_coincident() {
    let mut s = box_and_sketch();
    {
        let (se, sk) = s.session_sketch_mut().unwrap();
        se.set_three_d(sk, true);
        se.choose_tool(sk, Tool::Line);
        se.triad.active = TriadPlane::Yz;
    }
    s.observe_sketch(false);
    let mut cam = OrbitCamera {
        target: Vec3::new(5.0, 10.0, 2.0),
        distance: 160.0,
        ..Default::default()
    };
    cam.look_along(Vec3::new(1.0, -0.6, 0.8), Vec3::Z);
    cam.set_viewport(W, H);
    s.camera = cam;
    let mut v = View {
        ctx: context(),
        s,
        time: 0.0,
    };
    for _ in 0..3 {
        v.frame(vec![]);
    }
    // Die Ecke oben, die am weitesten vom Ursprung liegt (nicht unter der Triade).
    let solid = v.s.bodies[0].solid.clone();
    let corner = solid
        .vertices
        .iter()
        .map(|(_, p)| p.point)
        .max_by(|a, b| a.length().total_cmp(&b.length()))
        .unwrap();
    assert!((corner.z - 5.0).abs() < 1e-9);
    let n = v.s.sketch_history().unwrap().undo_len();
    v.click(v.at3(corner));
    // Der erste Klick wartet noch, auch mit der eingezogenen Ecke.
    assert_eq!(v.s.sketch_history().unwrap().undo_len(), n);
    v.click(v.at3(DVec3::new(0.0, 4.0, 12.0)));
    assert_eq!(
        v.s.sketch_history().unwrap().undo_len(),
        n + 1,
        "Linie und Ecke: ein Schritt"
    );
    let sk = v.s.session_sketch().unwrap().clone();
    let ls = lines(&sk);
    assert_eq!(ls.len(), 1);
    let start = [ls[0].1, ls[0].2]
        .into_iter()
        .find(|&p| near(sk.point_xyz(p).unwrap(), corner))
        .expect("die Linie beginnt an der Ecke");
    // Ein eingezogener, verknüpfter Punkt an der Ecke, „Deckungsgleich“ mit dem Linienanfang.
    let copies = &v.s.sketch_draft.as_ref().unwrap().copies;
    assert_eq!(copies.len(), 1, "{copies:?}");
    let CopyTarget::Point(q) = copies[0].target else {
        panic!("Ziel ist ein Punkt");
    };
    assert_ne!(q, start);
    assert!(matches!(
        copies[0].source,
        CopySource::Include(IncludeRef::Vertex(_))
    ));
    assert!(near(sk.point_xyz(q).unwrap(), corner));
    assert!(sk.constraints().any(|(_, c)| matches!(
        c.kind,
        ConstraintKind::Coincident(a, b) if (a == q && b == start) || (a == start && b == q)
    )));
    assert_eq!(
        sketch_view::point_color(&v.s, q),
        theme::sketch_colors(v.s.dark).projected
    );
    // Rückgängig nimmt alles auf einmal.
    v.frame(vec![Event::Key {
        key: egui::Key::Escape,
        physical_key: Some(egui::Key::Escape),
        pressed: true,
        repeat: false,
        modifiers: Modifiers::NONE,
    }]);
    v.frame(vec![]);
    assert!(v.s.undo_sketch());
    assert_eq!(v.s.session_sketch().unwrap().points().count(), 0);
    assert!(v.s.sketch_draft.as_ref().unwrap().copies.is_empty());
}

#[test]
fn esc_after_vertex_snap_leaves_nothing() {
    let mut s = box_and_sketch();
    {
        let (se, sk) = s.session_sketch_mut().unwrap();
        se.set_three_d(sk, true);
        se.choose_tool(sk, Tool::Line);
        se.triad.active = TriadPlane::Yz;
    }
    s.observe_sketch(false);
    let mut cam = OrbitCamera {
        target: Vec3::new(5.0, 10.0, 2.0),
        distance: 160.0,
        ..Default::default()
    };
    cam.look_along(Vec3::new(1.0, -0.6, 0.8), Vec3::Z);
    cam.set_viewport(W, H);
    s.camera = cam;
    let mut v = View {
        ctx: context(),
        s,
        time: 0.0,
    };
    for _ in 0..3 {
        v.frame(vec![]);
    }
    let solid = v.s.bodies[0].solid.clone();
    let corner = solid
        .vertices
        .iter()
        .map(|(_, p)| p.point)
        .max_by(|a, b| a.length().total_cmp(&b.length()))
        .unwrap();
    let n = v.s.sketch_history().unwrap().undo_len();
    v.click(v.at3(corner));
    assert_eq!(v.s.session_sketch().unwrap().points().count(), 2);
    v.frame(vec![Event::Key {
        key: egui::Key::Escape,
        physical_key: Some(egui::Key::Escape),
        pressed: true,
        repeat: false,
        modifiers: Modifiers::NONE,
    }]);
    v.frame(vec![]);
    assert_eq!(v.s.session_sketch().unwrap().points().count(), 0);
    assert!(v.s.sketch_draft.as_ref().unwrap().copies.is_empty());
    assert_eq!(v.s.sketch_history().unwrap().undo_len(), n);
}

#[test]
fn include_click_in_view_takes_edge_and_earlier_sketch_curve() {
    let mut s = AppState::new();
    s.create_box(DVec3::new(10.0, 20.0, 5.0)).unwrap();
    // Eine fertige 3D-Skizze neben dem Quader.
    let (a, b) = (DVec3::new(-20.0, 0.0, 0.0), DVec3::new(-20.0, 0.0, 15.0));
    let (first, c) = finished_3d_line(&mut s, a, b);
    open_new(&mut s, StandardPlane::XY);
    {
        let (se, sk) = s.session_sketch_mut().unwrap();
        se.choose_kind(sk, ToolKind::Include3d);
        assert_eq!(se.tool, Tool::ProjectEdge);
    }
    let mut cam = OrbitCamera {
        target: Vec3::new(0.0, 10.0, 2.0),
        distance: 160.0,
        ..Default::default()
    };
    cam.look_along(Vec3::new(1.0, -0.6, 0.8), Vec3::Z);
    cam.set_viewport(W, H);
    s.camera = cam;
    let mut v = View {
        ctx: context(),
        s,
        time: 0.0,
    };
    for _ in 0..3 {
        v.frame(vec![]);
    }
    // Mitte einer oberen Kante des Quaders.
    let e = top_edge(&v.s);
    let solid = v.s.bodies[0].solid.clone();
    let edge = solid.edges.get(e).unwrap();
    let mid = edge.curve.point(0.5 * (edge.t0 + edge.t1));
    v.click(v.at3(mid));
    let copies = v.s.sketch_draft.as_ref().unwrap().copies.clone();
    assert_eq!(copies.len(), 1, "{copies:?}");
    assert!(matches!(
        copies[0].source,
        CopySource::Include(IncludeRef::Edge(_))
    ));
    // Mitte der Linie der früheren Skizze.
    v.click(v.at3((a + b) * 0.5));
    let copies = v.s.sketch_draft.as_ref().unwrap().copies.clone();
    assert_eq!(copies.len(), 2, "{copies:?}");
    assert_eq!(
        copies[1].source,
        CopySource::Include(IncludeRef::SketchCurve {
            sketch: first,
            curve: c
        })
    );
}
