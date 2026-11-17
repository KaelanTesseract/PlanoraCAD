//! Skizzen 11b, Task 2: Zwischenablage am Skizzen-Feature (`copy_selection`, `paste_clip`).

use planora_feature::*;
use planora_kernel::Solid;
use planora_math::{DVec3, Frame, Tolerance};
use planora_sketch::clipboard::ClipSet;
use planora_sketch::helpers::ProjectStyle;
use planora_sketch::model::Sketch;
use planora_sketch::{ConstraintId, ConstraintKind};

const LINKED: ProjectStyle = ProjectStyle {
    construction: false,
};

fn tol() -> Tolerance {
    Tolerance::default()
}

fn empty_on(plane: StandardPlane) -> SketchFeature {
    SketchFeature {
        sketch: Sketch::new(plane.frame(), "", tol()),
        plane: SketchPlane::Standard(plane),
        copies: Vec::new(),
        formulas: Default::default(),
    }
}

fn all_curves(sf: &SketchFeature) -> ClipSet {
    ClipSet {
        curves: sf.sketch.curves().map(|(c, _)| c).collect(),
        ..ClipSet::default()
    }
}

fn sketch_feature(d: &Document, id: FeatureId) -> &SketchFeature {
    match &d.feature(id).unwrap().kind {
        FeatureKind::Sketch(sf) => sf,
        k => panic!("keine Skizze: {k:?}"),
    }
}

/// Eine Linie (0,0)-(10,0) mit Abstandsmaß 10 und (falls `formula`) dieser Formel.
fn dimensioned_line(plane: StandardPlane, formula: Option<&str>) -> (SketchFeature, ConstraintId) {
    let mut sf = empty_on(plane);
    let a = sf.sketch.add_point(0.0, 0.0).unwrap();
    let b = sf.sketch.add_point(10.0, 0.0).unwrap();
    sf.sketch.add_line(a, b).unwrap();
    let dim = sf
        .sketch
        .add_constraint(ConstraintKind::Distance(a, b, 10.0))
        .unwrap();
    if let Some(f) = formula {
        sf.formulas.insert(dim, f.to_string());
    }
    (sf, dim)
}

fn box_solid(d: &mut Document) -> Solid {
    let a = d
        .add_feature(
            "Quader",
            FeatureKind::Box {
                size: DVec3::new(10.0, 20.0, 5.0),
                origin: DVec3::ZERO,
            },
        )
        .unwrap();
    d.body(BodyRef {
        feature: a,
        index: 0,
    })
    .unwrap()
    .clone()
}

/// Die Kante des Körpers bei z = 0 und y = 0.
fn bottom_edge(solid: &Solid) -> (EdgeRef, planora_kernel::Curve, f64, f64) {
    for (id, e) in solid.edges.iter() {
        let p0 = solid.vertices[e.start].point;
        let p1 = solid.vertices[e.end].point;
        let on = |p: DVec3| p.z.abs() < 1e-9 && p.y.abs() < 1e-9;
        if on(p0) && on(p1) {
            let r = EdgeRef::from_edge(solid, id, &tol()).unwrap();
            return (r, e.curve.clone(), e.t0, e.t1);
        }
    }
    panic!("keine Kante");
}

/// Skizze auf XY mit einer verknüpft projizierten Kante und einer eigenen Linie.
fn projected_sketch(solid: &Solid) -> SketchFeature {
    let mut sf = empty_on(StandardPlane::XY);
    let (r, curve, t0, t1) = bottom_edge(solid);
    let c = sf
        .sketch
        .add_projected_curve(&curve, t0, t1, LINKED)
        .unwrap();
    sf.copies.push(EdgeCopy::new(c, CopySource::Edge(r)));
    let p = sf.sketch.add_point(3.0, 4.0).unwrap();
    let q = sf.sketch.add_point(6.0, 8.0).unwrap();
    sf.sketch.add_line(p, q).unwrap();
    sf
}

fn world(frame: &Frame, p: DVec3) -> DVec3 {
    frame.origin + frame.x * p.x + frame.y * p.y + frame.z * p.z
}

fn sketch_3d(plane: StandardPlane) -> SketchFeature {
    let mut sf = empty_on(plane);
    sf.sketch.set_three_d(true);
    sf
}

/// Der Punkt im Clip, der zum Punkt der Quelle gehört (gleiche Lage).
fn clip_point(
    clip: &FeatureClip,
    src: &SketchFeature,
    p: planora_sketch::PointId,
) -> planora_sketch::PointId {
    let want = src.sketch.point_xyz(p).unwrap();
    clip.sketch
        .points()
        .find(|(q, _)| (clip.sketch.point_xyz(*q).unwrap() - want).length() < 1e-9)
        .map(|(q, _)| q)
        .unwrap()
}

#[test]
fn projection_becomes_free_curve() {
    let mut d = Document::new();
    let solid = box_solid(&mut d);
    let src = projected_sketch(&solid);
    assert_eq!(src.copies.len(), 1);
    let clip = copy_selection(&src, &StandardPlane::XY.frame(), &all_curves(&src)).unwrap();
    assert_eq!(clip.made_free, 1);
    let mut dst = empty_on(StandardPlane::XY);
    let rep = paste_clip(
        &mut dst,
        &StandardPlane::XY.frame(),
        &clip,
        &d.param_scope(),
    )
    .unwrap();
    assert!(dst.copies.is_empty());
    assert_eq!(rep.map.curves.len(), 2);
    assert!(!rep.made_3d);
    assert!(
        dst.sketch
            .constraints()
            .all(|(_, c)| c.kind.fixed_point().is_none())
    );
}

#[test]
fn formula_survives_when_parameter_exists() {
    let mut d = Document::new();
    d.add_parameter("Breite", "5", ParamUnit::Length, "")
        .unwrap();
    let (src, _) = dimensioned_line(StandardPlane::XY, Some("Breite*2"));
    let clip = copy_selection(&src, &StandardPlane::XY.frame(), &all_curves(&src)).unwrap();
    assert_eq!(clip.formulas.len(), 1);
    assert_eq!(clip.formulas[0].value, 10.0);
    let mut dst = empty_on(StandardPlane::XY);
    let rep = paste_clip(
        &mut dst,
        &StandardPlane::XY.frame(),
        &clip,
        &d.param_scope(),
    )
    .unwrap();
    assert_eq!(rep.formulas_dropped, 0);
    assert_eq!(dst.formulas.len(), 1);
    let (id, text) = dst.formulas.iter().next().unwrap();
    assert_eq!(text, "Breite*2");
    assert_eq!(dst.sketch.constraint(*id).unwrap().kind.value(), Some(10.0));
}

#[test]
fn formula_becomes_value_without_parameter() {
    let (src, _) = dimensioned_line(StandardPlane::XY, Some("Breite*2"));
    let clip = copy_selection(&src, &StandardPlane::XY.frame(), &all_curves(&src)).unwrap();
    let other = Document::new();
    let mut dst = empty_on(StandardPlane::XY);
    let rep = paste_clip(
        &mut dst,
        &StandardPlane::XY.frame(),
        &clip,
        &other.param_scope(),
    )
    .unwrap();
    assert!(dst.formulas.is_empty());
    assert_eq!(rep.formulas_dropped, 1);
    let dims: Vec<_> = dst
        .sketch
        .constraints()
        .filter_map(|(_, c)| c.kind.value())
        .collect();
    assert_eq!(dims, vec![10.0]);
}

#[test]
fn paste_2d_into_other_plane_keeps_xy() {
    let mut src = empty_on(StandardPlane::XY);
    let a = src.sketch.add_point(1.0, 2.0).unwrap();
    let b = src.sketch.add_point(3.0, 5.0).unwrap();
    src.sketch.add_line(a, b).unwrap();
    let clip = copy_selection(&src, &StandardPlane::XY.frame(), &all_curves(&src)).unwrap();
    let mut dst = empty_on(StandardPlane::XZ);
    let rep = paste_clip(
        &mut dst,
        &StandardPlane::XZ.frame(),
        &clip,
        &Document::new().param_scope(),
    )
    .unwrap();
    let mut got: Vec<(f64, f64)> = rep
        .map
        .points
        .values()
        .map(|p| {
            let v = dst.sketch.point_xy(*p).unwrap();
            (v.x, v.y)
        })
        .collect();
    got.sort_by(|a, b| a.partial_cmp(b).unwrap());
    assert_eq!(got, vec![(1.0, 2.0), (3.0, 5.0)]);
}

#[test]
fn paste_3d_into_3d_keeps_world() {
    let src_frame = StandardPlane::XY.frame();
    let mut src = sketch_3d(StandardPlane::XY);
    let pts = [DVec3::new(1.0, 2.0, 3.0), DVec3::new(4.0, -1.0, 2.5)];
    let a = src.sketch.add_point3(pts[0]).unwrap();
    let b = src.sketch.add_point3(pts[1]).unwrap();
    src.sketch.add_line(a, b).unwrap();
    let clip = copy_selection(&src, &src_frame, &all_curves(&src)).unwrap();
    let dst_frame = StandardPlane::YZ.frame();
    let mut dst = sketch_3d(StandardPlane::YZ);
    let rep = paste_clip(&mut dst, &dst_frame, &clip, &Document::new().param_scope()).unwrap();
    assert!(!rep.made_3d);
    for (src_id, p) in [(a, pts[0]), (b, pts[1])] {
        let new = rep.map.points[&clip_point(&clip, &src, src_id)];
        let w = world(&dst_frame, dst.sketch.point_xyz(new).unwrap());
        assert!((w - p).length() < 1e-9, "{w:?} != {p:?}");
    }
}

#[test]
fn paste_3d_into_2d_makes_3d() {
    let mut src = sketch_3d(StandardPlane::XY);
    let a = src.sketch.add_point3(DVec3::new(1.0, 2.0, 3.0)).unwrap();
    let b = src.sketch.add_point3(DVec3::new(4.0, 5.0, 6.0)).unwrap();
    src.sketch.add_line(a, b).unwrap();
    let clip = copy_selection(&src, &StandardPlane::XY.frame(), &all_curves(&src)).unwrap();
    let mut dst = empty_on(StandardPlane::XZ);
    let rep = paste_clip(
        &mut dst,
        &StandardPlane::XZ.frame(),
        &clip,
        &Document::new().param_scope(),
    )
    .unwrap();
    assert!(rep.made_3d);
    assert!(dst.sketch.has_3d_entry());
    let new = rep.map.points[&clip_point(&clip, &src, a)];
    assert!((dst.sketch.point_xyz(new).unwrap() - DVec3::new(1.0, 2.0, 3.0)).length() < 1e-9);
}

#[test]
fn failed_paste_leaves_target_unchanged() {
    let mut src = sketch_3d(StandardPlane::XY);
    let a = src.sketch.add_point3(DVec3::new(1.0, 2.0, 3.0)).unwrap();
    let mut clip = copy_selection(
        &src,
        &StandardPlane::XY.frame(),
        &ClipSet {
            points: vec![a],
            ..ClipSet::default()
        },
    )
    .unwrap();
    // Eine leere Ablage lässt sich nicht einfügen.
    clip.sketch = Sketch::new(StandardPlane::XY.frame(), "", tol());
    let mut dst = empty_on(StandardPlane::XY);
    let before = dst.clone();
    assert!(
        paste_clip(
            &mut dst,
            &StandardPlane::XY.frame(),
            &clip,
            &Document::new().param_scope()
        )
        .is_err()
    );
    assert_eq!(dst, before);
    assert!(!dst.sketch.has_3d_entry());
}

#[test]
fn paste_after_source_deleted_into_other_document() {
    let mut a = Document::new();
    a.add_parameter("Breite", "5", ParamUnit::Length, "")
        .unwrap();
    let solid = box_solid(&mut a);
    let mut src = projected_sketch(&solid);
    let p = src.sketch.add_point(0.0, 20.0).unwrap();
    let q = src.sketch.add_point(10.0, 20.0).unwrap();
    src.sketch.add_line(p, q).unwrap();
    let dim = src
        .sketch
        .add_constraint(ConstraintKind::Distance(p, q, 10.0))
        .unwrap();
    src.formulas.insert(dim, "Breite*2".to_string());
    let s = a.add_feature("Skizze", FeatureKind::Sketch(src)).unwrap();
    let sf = sketch_feature(&a, s).clone();
    let clip = copy_selection(&sf, &StandardPlane::XY.frame(), &all_curves(&sf)).unwrap();
    assert_eq!(clip.formulas.len(), 1);
    a.delete(s).unwrap();

    let mut b = Document::new();
    let t = b
        .add_feature("Ziel", FeatureKind::Sketch(empty_on(StandardPlane::XZ)))
        .unwrap();
    let mut target = sketch_feature(&b, t).clone();
    let rep = paste_clip(
        &mut target,
        &StandardPlane::XZ.frame(),
        &clip,
        &b.param_scope(),
    )
    .unwrap();
    assert_eq!(rep.formulas_dropped, clip.formulas.len());
    b.update(t, |k| *k = FeatureKind::Sketch(target.clone()))
        .unwrap();
    b.recompute();
    let f = b.feature(t).unwrap();
    assert_eq!(f.status, FeatureStatus::Ok, "{:?}", f.status);
    assert!(f.warning().is_none(), "{:?}", f.warning());
    let live = sketch_feature(&b, t);
    assert!(live.copies.is_empty());
    assert!(live.formulas.is_empty());
    let sk = &live.sketch;
    assert!(sk.constraints().count() > 0);
    for (_, c) in sk.constraints() {
        for p in c.kind.points() {
            assert!(sk.point_xyz(p).is_ok());
        }
    }
}

fn sketch_3d_in(frame: Frame) -> SketchFeature {
    let mut sf = empty_on(StandardPlane::XY);
    sf.sketch = Sketch::new(frame, "", tol());
    sf.sketch.set_three_d(true);
    sf
}

#[test]
fn paste_3d_into_3d_offset_planes_keeps_world() {
    let src_frame = Frame {
        origin: DVec3::new(1.0, 2.0, 3.0),
        ..StandardPlane::XY.frame()
    };
    let dst_frame = Frame {
        origin: DVec3::new(-4.0, 5.0, 0.5),
        ..StandardPlane::YZ.frame()
    };
    let mut src = sketch_3d_in(src_frame);
    let pts = [DVec3::new(1.0, 2.0, 3.0), DVec3::new(4.0, -1.0, 2.5)];
    let a = src.sketch.add_point3(pts[0]).unwrap();
    let b = src.sketch.add_point3(pts[1]).unwrap();
    src.sketch.add_line(a, b).unwrap();
    let clip = copy_selection(&src, &src_frame, &all_curves(&src)).unwrap();
    let mut dst = sketch_3d_in(dst_frame);
    let rep = paste_clip(&mut dst, &dst_frame, &clip, &Document::new().param_scope()).unwrap();
    assert_eq!(rep.constraints_dropped, 0);
    for (id, p) in [(a, pts[0]), (b, pts[1])] {
        let new = rep.map.points[&clip_point(&clip, &src, id)];
        let w = world(&dst_frame, dst.sketch.point_xyz(new).unwrap());
        let want = world(&src_frame, p);
        assert!((w - want).length() < 1e-9, "{w:?} != {want:?}");
    }
}

#[test]
fn moving_paste_keeps_constraints_and_formulas() {
    let mut d = Document::new();
    d.add_parameter("Breite", "5", ParamUnit::Length, "")
        .unwrap();
    let src_frame = StandardPlane::XY.frame();
    let dst_frame = StandardPlane::YZ.frame();
    let mut src = sketch_3d(StandardPlane::XY);
    let a = src.sketch.add_point3(DVec3::new(0.0, 0.0, 1.0)).unwrap();
    let b = src.sketch.add_point3(DVec3::new(10.0, 0.0, 1.0)).unwrap();
    src.sketch.add_line(a, b).unwrap();
    let dim = src
        .sketch
        .add_constraint(ConstraintKind::Distance(a, b, 10.0))
        .unwrap();
    src.formulas.insert(dim, "Breite*2".to_string());
    let clip = copy_selection(&src, &src_frame, &all_curves(&src)).unwrap();
    let mut dst = sketch_3d(StandardPlane::YZ);
    let rep = paste_clip(&mut dst, &dst_frame, &clip, &d.param_scope()).unwrap();
    assert_eq!(rep.constraints_dropped, 0);
    assert_eq!(rep.formulas_dropped, 0);
    assert_eq!(dst.formulas.len(), 1);
    let (id, _) = dst.formulas.iter().next().unwrap();
    assert!(dst.sketch.constraint(*id).is_some());
}

/// Dokument mit einem Körper mit Freiformkante; liefert die Skizze mit der projizierten
/// Freiformkante (fester Spline), auf 3D geschaltet und mit einem 3D-Punkt.
fn fixed_spline_3d() -> SketchFeature {
    let mut base = Sketch::new(Frame::WORLD, "Basis", tol());
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
    let mut sf = empty_on(StandardPlane::XY);
    sf.sketch
        .add_projected_curve(&e.curve, e.t0, e.t1, LINKED)
        .unwrap();
    sf.sketch.add_point3(DVec3::new(1.0, 1.0, 2.0)).unwrap();
    sf.sketch.set_three_d(true);
    sf
}

#[test]
fn fixed_spline_between_different_planes_is_refused() {
    let src = fixed_spline_3d();
    let set = ClipSet {
        curves: src.sketch.curves().map(|(c, _)| c).collect(),
        points: src.sketch.points().map(|(p, _)| p).collect(),
        ..ClipSet::default()
    };
    let clip = copy_selection(&src, &StandardPlane::XY.frame(), &set).unwrap();
    assert!(clip.sketch.has_3d_entry());
    let mut dst = sketch_3d(StandardPlane::YZ);
    let before = dst.clone();
    let rev = dst.sketch.revision();
    let err = paste_clip(
        &mut dst,
        &StandardPlane::YZ.frame(),
        &clip,
        &Document::new().param_scope(),
    )
    .unwrap_err();
    assert!(
        err.to_string()
            .contains(&planora_i18n::tr!("sketch-move-fixed")),
        "{err}"
    );
    assert_eq!(dst, before);
    assert_eq!(dst.sketch.revision(), rev);
    // Auf derselben Ebene geht dasselbe Einfügen (keine Bewegung nötig).
    let mut same = sketch_3d(StandardPlane::XY);
    paste_clip(
        &mut same,
        &StandardPlane::XY.frame(),
        &clip,
        &Document::new().param_scope(),
    )
    .unwrap();
}

#[test]
fn error_after_switch_to_3d_keeps_target_2d_and_unchanged() {
    // Eine 3D-Ablage ohne Inhalt: das Umschalten geschieht, das Einfügen scheitert danach.
    let mut clip = copy_selection(
        &dimensioned_line(StandardPlane::XY, None).0,
        &StandardPlane::XY.frame(),
        &all_curves(&dimensioned_line(StandardPlane::XY, None).0),
    )
    .unwrap();
    clip.sketch = Sketch::new(StandardPlane::XY.frame(), "", tol());
    clip.sketch.set_three_d(true);
    assert!(clip.sketch.has_3d_entry());
    let mut dst = empty_on(StandardPlane::XZ);
    let before = dst.clone();
    let rev = dst.sketch.revision();
    assert!(
        paste_clip(
            &mut dst,
            &StandardPlane::XZ.frame(),
            &clip,
            &Document::new().param_scope()
        )
        .is_err()
    );
    assert_eq!(dst, before);
    assert_eq!(dst.sketch.revision(), rev);
    assert!(!dst.sketch.has_3d_entry());
}
