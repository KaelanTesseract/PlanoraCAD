//! Skizzen 12, Task 12b: Zwischenspeicher der fertigen Skizzen. Die Abtastung der Kurven läuft
//! einmal je Stand der Skizze, nicht in jedem Bild; jede Änderung (Kurve bearbeitet,
//! Rückgängig, Parameter, Anzeigeschalter, Sichtbarkeit, anderes Dokument) zeigt sofort den
//! neuen Stand. Bei 1920 × 1080.

use glam::{Vec2, Vec3};
use planora_app::sketch_view;
use planora_app::state::{AppState, PickFilter, StandardPlane};
use planora_app::viewport::stroke_view;
use planora_feature::{FeatureId, FeatureKind, ParamEdit, ParamUnit, SketchFeature, SketchPlane};
use planora_math::{DVec3, Frame, Tolerance};
use planora_render::{Pick, StrokeVertex};
use planora_sketch::constraint::{ConstraintKind, LineOrPoints};
use planora_sketch::model::{CurveId, PointId, Sketch};

const SCREEN: egui::Rect =
    egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1920.0, 1080.0));
const VIEW: Vec2 = Vec2::new(1920.0, 1080.0);

fn prepared(mut s: AppState) -> AppState {
    s.camera.set_viewport(SCREEN.width(), SCREEN.height());
    s.camera.set_iso_view();
    s
}

fn edit_sketch<T>(s: &mut AppState, id: FeatureId, f: impl FnOnce(&mut Sketch) -> T) -> T {
    s.open_sketch(id).unwrap();
    let r = f(s.session_sketch_mut().unwrap().1);
    s.close_sketch();
    r
}

/// Fertige Skizze auf XY mit einer Linie (0,0)–(20,0), einem Kreis und einer Konstruktionslinie.
fn scene() -> (AppState, FeatureId, PointId, CurveId) {
    let mut s = prepared(AppState::new());
    let id = s
        .add_sketch(SketchPlane::Standard(StandardPlane::XY))
        .unwrap();
    let (b, l) = edit_sketch(&mut s, id, |sk| {
        let a = sk.add_point(0.0, 0.0).unwrap();
        let b = sk.add_point(20.0, 0.0).unwrap();
        let l = sk.add_line(a, b).unwrap();
        let m = sk.add_point(30.0, 10.0).unwrap();
        sk.add_circle(m, 4.0).unwrap();
        let c = sk.add_point(0.0, 10.0).unwrap();
        let d = sk.add_point(20.0, 10.0).unwrap();
        let k = sk.add_line(c, d).unwrap();
        sk.set_construction(k, true).unwrap();
        (b, l)
    });
    (s, id, b, l)
}

/// Die GPU-Linien der fertigen Skizzen als vergleichbare Bitmuster (Ort, Farbe, Saum).
fn strokes(s: &AppState) -> Vec<[u32; 8]> {
    let mut out: Vec<StrokeVertex> = Vec::new();
    sketch_view::finished_sketch_strokes(s, &stroke_view(s, SCREEN, 1.0), &mut out);
    out.iter()
        .map(|v| {
            let [x, y, z] = v.ndc;
            let [r, g, b, a] = v.color;
            [x, y, z, r, g, b, a, v.edge].map(f32::to_bits)
        })
        .collect()
}

/// Bildschirmort eines Weltpunkts.
fn at(s: &AppState, p: DVec3) -> Vec2 {
    s.camera
        .project(Vec3::new(p.x as f32, p.y as f32, p.z as f32), VIEW)
        .expect("im Bild")
}

fn select(s: &AppState, p: DVec3) -> Option<Pick> {
    s.pick_with(at(s, p), VIEW, PickFilter::SELECT)
}

#[test]
fn sampling_runs_once_per_sketch_state() {
    let (s, _, _, _) = scene();
    let before = s.finished_cache.computations();
    let first = strokes(&s);
    assert_eq!(
        s.finished_cache.computations(),
        before + 1,
        "einmal abgetastet"
    );
    for _ in 0..5 {
        assert_eq!(strokes(&s), first, "jedes Bild gleich");
        let _ = sketch_view::finished_sketch_shapes(&s, SCREEN);
        let _ = select(&s, DVec3::new(10.0, 0.0, 0.0));
    }
    assert_eq!(
        s.finished_cache.computations(),
        before + 1,
        "weitere Bilder, der Kamm und das Anklicken tasten nicht neu ab"
    );
}

#[test]
fn edited_curve_changes_strokes_and_undo_restores_them() {
    let (mut s, id, b, l) = scene();
    let snapshot = s.snapshot();
    let old = strokes(&s);
    assert_eq!(
        select(&s, DVec3::new(10.0, 0.0, 0.0)),
        Some(Pick::SketchCurve {
            sketch: id,
            curve: l
        })
    );
    edit_sketch(&mut s, id, |sk| sk.set_point_xy(b, 20.0, -15.0).unwrap());
    let new = strokes(&s);
    assert_ne!(new, old, "nach der Änderung neue Linien");
    // Das Anklicken sieht die neue Lage: die Mitte der alten Linie trifft nichts mehr.
    assert_eq!(select(&s, DVec3::new(18.0, 0.0, 0.0)), None);
    assert_eq!(
        select(&s, DVec3::new(10.0, -7.5, 0.0)),
        Some(Pick::SketchCurve {
            sketch: id,
            curve: l
        })
    );
    s.restore_document(snapshot).unwrap();
    assert_eq!(strokes(&s), old, "nach Rückgängig wie vorher");
    assert_eq!(
        select(&s, DVec3::new(18.0, 0.0, 0.0)),
        Some(Pick::SketchCurve {
            sketch: id,
            curve: l
        })
    );
}

#[test]
fn parameter_change_resamples() {
    let mut s = prepared(AppState::new());
    let p = s.add_parameter().unwrap();
    let edit = |formula: &str| ParamEdit {
        name: "L".into(),
        formula: formula.into(),
        unit: ParamUnit::Length,
        comment: String::new(),
    };
    s.edit_parameter(p, edit("20")).unwrap();
    let mut sk = Sketch::new(Frame::WORLD, "Skizze", Tolerance::default());
    let a = sk.add_point(0.0, 0.0).unwrap();
    let b = sk.add_point(20.0, 0.0).unwrap();
    let l = sk.add_line(a, b).unwrap();
    sk.add_constraint(ConstraintKind::Fixed(a, 0.0, 0.0))
        .unwrap();
    sk.add_constraint(ConstraintKind::Horizontal(LineOrPoints::Line(l)))
        .unwrap();
    let dim = sk
        .add_constraint(ConstraintKind::Distance(a, b, 20.0))
        .unwrap();
    let mut def = SketchFeature {
        sketch: sk,
        plane: SketchPlane::Standard(StandardPlane::XY),
        copies: vec![],
        formulas: Default::default(),
        text_formulas: Default::default(),
    };
    def.formulas.insert(dim, "L".into());
    s.add_feature(Some("Skizze"), FeatureKind::Sketch(def))
        .unwrap();
    let short = strokes(&s);
    s.edit_parameter(p, edit("35")).unwrap();
    let long = strokes(&s);
    assert_ne!(long, short, "längere Linie nach dem Parameter");
    s.edit_parameter(p, edit("20")).unwrap();
    assert_eq!(strokes(&s), short);
}

#[test]
fn display_switch_and_visibility_apply_at_once() {
    let (mut s, id, _, _) = scene();
    let all = strokes(&s);
    let mut d = s.document.sketch_display(id);
    d.construction = false;
    s.document.set_sketch_display(id, d);
    let without = strokes(&s);
    assert!(
        !without.is_empty() && without.len() < all.len(),
        "Konstruktion fehlt"
    );
    d.construction = true;
    s.document.set_sketch_display(id, d);
    assert_eq!(strokes(&s), all);
    s.set_feature_hidden(id, true);
    assert!(strokes(&s).is_empty(), "ausgeblendet");
    assert_eq!(select(&s, DVec3::new(10.0, 0.0, 0.0)), None);
    s.set_feature_hidden(id, false);
    assert_eq!(strokes(&s), all);
}

#[test]
fn two_documents_do_not_mix() {
    // Gleiche Feature-Nummer, andere Skizze: im selben Zustand nach einem Dokumentwechsel und
    // in einem zweiten Zustand (Tab).
    let (mut s, id, _, _) = scene();
    let (mut other, other_id, b, _) = scene();
    assert_eq!(id, other_id);
    edit_sketch(&mut other, other_id, |sk| {
        sk.set_point_xy(b, 20.0, 15.0).unwrap()
    });
    let mine = strokes(&s);
    let theirs = strokes(&other);
    assert_ne!(mine, theirs);
    assert_eq!(strokes(&s), mine, "der erste Tab bleibt bei seiner Skizze");
    s.restore_document(other.snapshot()).unwrap();
    assert_eq!(
        strokes(&s),
        theirs,
        "nach dem Wechsel die Skizze des neuen Dokuments"
    );
}

#[test]
fn screen3_fn_equals_to_screen3() {
    let (s, _, _, _) = scene();
    let map = sketch_view::Mapper {
        camera: s.camera,
        frame: Frame::from_origin_normal(DVec3::new(1.0, 2.0, 3.0), DVec3::new(0.2, 0.3, 1.0)),
        rect: egui::Rect::from_min_size(egui::pos2(10.0, 20.0), egui::vec2(1920.0, 1080.0)),
    };
    let f = map.screen3_fn();
    for p in [
        DVec3::ZERO,
        DVec3::new(10.0, -5.0, 2.0),
        DVec3::new(-300.0, 40.0, 7.5),
        DVec3::new(0.0, 0.0, 1e6),
    ] {
        assert_eq!(f(p), map.to_screen3(p), "{p:?}");
    }
}
