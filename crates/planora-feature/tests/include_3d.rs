//! Skizzen 10, Task 15: Herkunft mit Stücken und Skizzen als Quelle (Spec 5 und 10).
//! Einbezogene 3D-Geometrie folgt ihrer Quelle; die Stücke einer Quelle werden zusammen neu
//! gerechnet, eine andere Stückzahl lässt alles stehen und warnt („Bezug passt nicht mehr“).
//! Eine Skizze als Quelle gilt nur vor der aktiven im Verlauf; gelöscht oder dahinter geschoben
//! ist der Bezug verloren (Warnung, Kurve bleibt).

use planora_feature::*;
use planora_i18n::tr;
use planora_kernel::{BoolOp, Solid};
use planora_math::{DVec3, Frame, Tolerance};
use planora_sketch::helpers::Included;
use planora_sketch::model::{CurveId, CurveKind, PointId, Sketch};

fn tol() -> Tolerance {
    Tolerance::default()
}

fn add_box(d: &mut Document, name: &str, size: DVec3, origin: DVec3) -> FeatureId {
    d.add_feature(name, FeatureKind::Box { size, origin })
        .unwrap()
}

fn body(d: &Document, feature: FeatureId) -> Solid {
    d.body(BodyRef { feature, index: 0 }).unwrap().clone()
}

fn sketch_kind(sketch: Sketch, plane: StandardPlane, copies: Vec<EdgeCopy>) -> FeatureKind {
    FeatureKind::Sketch(SketchFeature {
        sketch,
        plane: SketchPlane::Standard(plane),
        copies,
        formulas: Default::default(),
        text_formulas: Default::default(),
    })
}

fn sketch_def(d: &Document, s: FeatureId) -> &SketchFeature {
    match &d.feature(s).unwrap().kind {
        FeatureKind::Sketch(sf) => sf,
        k => panic!("keine Skizze: {k:?}"),
    }
}

fn set_box_x(d: &mut Document, b: FeatureId, x: f64) {
    d.update(b, |k| {
        if let FeatureKind::Box { size, .. } = k {
            size.x = x;
        }
    })
    .unwrap();
}

/// Die 3D-Enden der Linie `c` in der berechneten Skizze `s`.
fn line_ends3(d: &Document, s: FeatureId, c: CurveId) -> (DVec3, DVec3) {
    let sk = d.sketch_of(s).unwrap();
    let CurveKind::Line { a, b } = sk.curve(c).unwrap().kind else {
        panic!("keine Linie");
    };
    (sk.point_xyz(a).unwrap(), sk.point_xyz(b).unwrap())
}

fn close3(a: DVec3, b: DVec3) -> bool {
    (a - b).length() < 1e-7
}

fn warning(d: &Document, s: FeatureId) -> String {
    d.feature(s)
        .unwrap()
        .warning()
        .unwrap_or_default()
        .to_string()
}

fn ok(d: &Document, s: FeatureId) {
    let f = d.feature(s).unwrap();
    assert_eq!(f.status, FeatureStatus::Ok, "{:?}", f.status);
}

/// Die senkrechte Kante des Quaders bei größtem x und kleinstem y.
fn vertical_edge(solid: &Solid) -> EdgeRef {
    let (lo, hi) = bounds(solid);
    for (id, e) in solid.edges.iter() {
        let p0 = solid.vertices[e.start].point;
        let p1 = solid.vertices[e.end].point;
        let at = |p: DVec3| (p.x - hi.x).abs() < 1e-9 && (p.y - lo.y).abs() < 1e-9;
        if at(p0) && at(p1) {
            return EdgeRef::from_edge(solid, id, &tol()).unwrap();
        }
    }
    panic!("keine senkrechte Kante");
}

fn bounds(solid: &Solid) -> (DVec3, DVec3) {
    solid.vertices.iter().map(|(_, v)| v.point).fold(
        (DVec3::splat(f64::MAX), DVec3::splat(f64::MIN)),
        |(lo, hi), p| (lo.min(p), hi.max(p)),
    )
}

/// Die Deckfläche (Normale +z, oben) des Körpers.
fn top_face(solid: &Solid) -> FaceRef {
    let (_, hi) = bounds(solid);
    let (id, _) = solid
        .faces
        .iter()
        .find(|(f, _)| {
            planora_sketch::plane_of_face(solid, *f)
                .is_some_and(|fr| fr.z.z > 0.5 && (fr.origin.z - hi.z).abs() < 1e-9)
        })
        .unwrap();
    FaceRef::from_face(solid, id, &tol()).unwrap()
}

/// Legt die Stücke von `r` (für eine Skizze auf XY am Ende des Verlaufs) in `sk` an und liefert
/// die Kopien als Gruppe mit Stückindex.
fn include_into(d: &Document, sk: &mut Sketch, r: IncludeRef) -> Vec<EdgeCopy> {
    let pieces = included_pieces(d, d.features().len(), &Frame::WORLD, &r).unwrap();
    pieces
        .iter()
        .enumerate()
        .map(|(k, g)| {
            let ids = sk.add_included(g).unwrap();
            let target = match (ids.curves.first(), ids.points.first()) {
                (Some(c), _) => CopyTarget::Curve(*c),
                (None, Some(p)) => CopyTarget::Point(*p),
                _ => panic!("nichts angelegt"),
            };
            EdgeCopy {
                target,
                source: CopySource::Include(r.clone()),
                piece: k as u32,
                of: pieces.len() as u32,
            }
        })
        .collect()
}

fn curve_of(c: &EdgeCopy) -> CurveId {
    c.curve().unwrap()
}

fn point_of(c: &EdgeCopy) -> PointId {
    c.point().unwrap()
}

#[test]
fn include_edge_follows_box_resize() {
    let mut d = Document::new();
    let b = add_box(&mut d, "Quader", DVec3::new(10.0, 20.0, 5.0), DVec3::ZERO);
    let edge = vertical_edge(&body(&d, b));
    let mut sk = Sketch::new(Frame::WORLD, "Skizze", tol());
    let copies = include_into(&d, &mut sk, IncludeRef::Edge(edge));
    assert_eq!(copies.len(), 1);
    let line = curve_of(&copies[0]);
    let s = d
        .add_feature("Skizze", sketch_kind(sk, StandardPlane::XY, copies))
        .unwrap();
    ok(&d, s);
    assert!(d.feature(s).unwrap().warning().is_none());
    let (p0, p1) = line_ends3(&d, s, line);
    assert!((p0.x - 10.0).abs() < 1e-9 && (p1.x - 10.0).abs() < 1e-9);
    assert!(((p0.z - p1.z).abs() - 5.0).abs() < 1e-9, "{p0} {p1}");

    set_box_x(&mut d, b, 30.0);
    ok(&d, s);
    assert!(d.feature(s).unwrap().warning().is_none());
    let (p0, p1) = line_ends3(&d, s, line);
    assert!(
        (p0.x - 30.0).abs() < 1e-9 && (p1.x - 30.0).abs() < 1e-9,
        "{p0} {p1}"
    );
    assert!(((p0.z - p1.z).abs() - 5.0).abs() < 1e-9);
    // Die Kurve ist 3D (Raumlage) und lila verknüpft.
    assert!(d.sketch_of(s).unwrap().is_3d_curve(line));
    assert!(sketch_def(&d, s).copies[0].curve() == Some(line));
}

#[test]
fn include_face_boundary_four_pieces() {
    let mut d = Document::new();
    let b = add_box(&mut d, "Quader", DVec3::new(10.0, 20.0, 5.0), DVec3::ZERO);
    let face = top_face(&body(&d, b));
    let mut sk = Sketch::new(Frame::WORLD, "Skizze", tol());
    let copies = include_into(&d, &mut sk, IncludeRef::FaceBoundary(face));
    assert_eq!(copies.len(), 4);
    assert_eq!(
        copies.iter().map(|c| c.piece).collect::<Vec<_>>(),
        [0, 1, 2, 3]
    );
    let lines: Vec<CurveId> = copies.iter().map(curve_of).collect();
    let s = d
        .add_feature("Skizze", sketch_kind(sk, StandardPlane::XY, copies))
        .unwrap();
    ok(&d, s);
    assert!(d.feature(s).unwrap().warning().is_none());
    for &c in &lines {
        let (p0, p1) = line_ends3(&d, s, c);
        assert!((p0.z - 5.0).abs() < 1e-9 && (p1.z - 5.0).abs() < 1e-9);
    }

    set_box_x(&mut d, b, 30.0);
    ok(&d, s);
    assert!(d.feature(s).unwrap().warning().is_none());
    let mut xs: Vec<f64> = lines
        .iter()
        .flat_map(|&c| {
            let (p0, p1) = line_ends3(&d, s, c);
            [p0.x, p1.x]
        })
        .collect();
    xs.sort_by(f64::total_cmp);
    xs.dedup_by(|a, b| (*a - *b).abs() < 1e-9);
    assert_eq!(xs.len(), 2, "{xs:?}");
    assert!(xs[0].abs() < 1e-9 && (xs[1] - 30.0).abs() < 1e-9, "{xs:?}");
    // Alle Kopien tragen dieselbe (nachgeführte) Herkunft.
    let def = sketch_def(&d, s);
    assert!(def.copies.iter().all(|c| c.source == def.copies[0].source));
}

#[test]
fn piece_count_change_keeps_curves_and_warns() {
    let mut d = Document::new();
    let a = add_box(&mut d, "A", DVec3::new(10.0, 20.0, 5.0), DVec3::ZERO);
    let n = add_box(
        &mut d,
        "Kerbe",
        DVec3::new(4.0, 4.0, 10.0),
        DVec3::new(-2.0, -2.0, -2.0),
    );
    let face = top_face(&body(&d, a));
    let mut sk = Sketch::new(Frame::WORLD, "Skizze", tol());
    let copies = include_into(&d, &mut sk, IncludeRef::FaceBoundary(face));
    assert_eq!(copies.len(), 4);
    let lines: Vec<CurveId> = copies.iter().map(curve_of).collect();
    let s = d
        .add_feature("Skizze", sketch_kind(sk, StandardPlane::XY, copies))
        .unwrap();
    ok(&d, s);
    assert!(d.feature(s).unwrap().warning().is_none());
    let before: Vec<(DVec3, DVec3)> = lines.iter().map(|&c| line_ends3(&d, s, c)).collect();
    let constraints = d.sketch_of(s).unwrap().constraints().count();

    // Vor der Skizze eine Kerbe aus der Ecke schneiden: Die Deckfläche hat dann sechs Kanten.
    d.set_marker(2).unwrap();
    d.add_feature(
        "Differenz",
        FeatureKind::Boolean {
            target: BodyRef {
                feature: a,
                index: 0,
            },
            tool: BodyRef {
                feature: n,
                index: 0,
            },
            op: BoolOp::Difference,
        },
    )
    .unwrap();
    d.set_marker(d.features().len()).unwrap();
    ok(&d, s);
    let w = warning(&d, s);
    assert!(
        w.contains(&tr!("feature-sketch-reference-mismatch", count = 4)),
        "{w}"
    );
    // Die Kurven bleiben, wie sie waren, samt Abhängigkeiten; die Kopien bleiben verknüpft.
    for (k, &c) in lines.iter().enumerate() {
        let (p0, p1) = line_ends3(&d, s, c);
        assert!(close3(p0, before[k].0) && close3(p1, before[k].1), "{k}");
    }
    assert_eq!(d.sketch_of(s).unwrap().constraints().count(), constraints);
    assert_eq!(sketch_def(&d, s).copies.len(), 4);
    d.check_invariants().unwrap();
}

/// Skizze 1 auf XZ mit einer Linie (im Raum von (0, 0, 0) nach (10, 0, 10)), Skizze 2 auf XY
/// bezieht sie ein. Liefert Dokument, beide Skizzen, die Linie in Skizze 1 und die Kopie in 2.
fn two_sketches() -> (Document, FeatureId, FeatureId, CurveId, CurveId) {
    let mut d = Document::new();
    let mut sk1 = Sketch::new(StandardPlane::XZ.frame(), "Skizze1", tol());
    let p = sk1.add_point(0.0, 0.0).unwrap();
    let q = sk1.add_point(10.0, 10.0).unwrap();
    let src = sk1.add_line(p, q).unwrap();
    let s1 = d
        .add_feature("Skizze1", sketch_kind(sk1, StandardPlane::XZ, vec![]))
        .unwrap();
    let r = IncludeRef::SketchCurve {
        sketch: s1,
        curve: src,
    };
    let mut sk2 = Sketch::new(Frame::WORLD, "Skizze2", tol());
    let copies = include_into(&d, &mut sk2, r);
    let line = curve_of(&copies[0]);
    let s2 = d
        .add_feature("Skizze2", sketch_kind(sk2, StandardPlane::XY, copies))
        .unwrap();
    ok(&d, s2);
    assert!(d.feature(s2).unwrap().warning().is_none());
    let (a, b) = line_ends3(&d, s2, line);
    // XZ: Skizzen-x = Welt-x, Skizzen-y = Welt-z.
    assert!(
        close3(a, DVec3::ZERO) && close3(b, DVec3::new(10.0, 0.0, 10.0)),
        "{a} {b}"
    );
    (d, s1, s2, src, line)
}

#[test]
fn sketch_source_after_active_is_refused() {
    let (mut d, s1, s2, _, _) = two_sketches();
    let b = add_box(&mut d, "Quader", DVec3::ONE, DVec3::ZERO);
    assert!(sketch_source_allowed(&d, s2, s1));
    assert!(!sketch_source_allowed(&d, s1, s2), "spätere Skizze");
    assert!(!sketch_source_allowed(&d, s2, s2), "sich selbst");
    let later = add_box(&mut d, "Quader2", DVec3::ONE, DVec3::ZERO);
    assert!(sketch_source_allowed(&d, later, s1));
    assert!(!sketch_source_allowed(&d, later, b), "kein Skizzen-Feature");
    assert!(!sketch_source_allowed(&d, s2, b), "dahinter");
    assert!(!sketch_source_allowed(&d, s2, FeatureId(999)), "unbekannt");
    // Eine Quellskizze hinter der Skizze wird nicht gelesen, auch wenn sie ein Ergebnis hat:
    // Skizze 1 bezieht eine Kurve aus Skizze 2 ein → verlorener Bezug.
    let line2 = sketch_def(&d, s2).copies[0].curve().unwrap();
    d.update(s1, |k| {
        if let FeatureKind::Sketch(sf) = k {
            let a = sf.sketch.add_point(50.0, 50.0).unwrap();
            let b = sf.sketch.add_point(60.0, 50.0).unwrap();
            let c = sf.sketch.add_line(a, b).unwrap();
            sf.copies.push(EdgeCopy::new(
                c,
                CopySource::Include(IncludeRef::SketchCurve {
                    sketch: s2,
                    curve: line2,
                }),
            ));
        }
    })
    .unwrap();
    ok(&d, s1);
    let w = warning(&d, s1);
    assert!(
        w.contains(&tr!("feature-sketch-projection-lost", count = 1)),
        "{w}"
    );
    d.check_invariants().unwrap();
}

#[test]
fn sketch_curve_follows_its_source_sketch() {
    let (mut d, s1, s2, src, line) = two_sketches();
    d.update(s1, |k| {
        if let FeatureKind::Sketch(sf) = k {
            let CurveKind::Line { b, .. } = sf.sketch.curve(src).unwrap().kind else {
                panic!("keine Linie");
            };
            sf.sketch.set_point_xy(b, 20.0, 5.0).unwrap();
        }
    })
    .unwrap();
    ok(&d, s2);
    assert!(d.feature(s2).unwrap().warning().is_none());
    let (a, b) = line_ends3(&d, s2, line);
    assert!(
        close3(a, DVec3::ZERO) && close3(b, DVec3::new(20.0, 0.0, 5.0)),
        "{a} {b}"
    );
}

#[test]
fn deleted_source_sketch_warns_curve_stays() {
    let (mut d, s1, s2, _, line) = two_sketches();
    let before = line_ends3(&d, s2, line);
    // Ein loser Verweis: Die Quelle lässt sich löschen, ohne die Skizze mitzunehmen.
    assert!(d.dependents(s1).is_empty());
    d.delete(s1).unwrap();
    ok(&d, s2);
    let w = warning(&d, s2);
    assert!(
        w.contains(&tr!("feature-sketch-projection-lost", count = 1)),
        "{w}"
    );
    let after = line_ends3(&d, s2, line);
    assert!(close3(after.0, before.0) && close3(after.1, before.1));
    assert_eq!(sketch_def(&d, s2).copies.len(), 1);
    d.check_invariants().unwrap();
}

#[test]
fn reorder_source_behind_warns() {
    let (d, _, s2, _, line) = two_sketches();
    let before = line_ends3(&d, s2, line);
    let mut snap = d.snapshot();
    snap.features.swap(0, 1);
    let mut d2 = d.clone();
    d2.restore(snap).unwrap();
    ok(&d2, s2);
    let w = warning(&d2, s2);
    assert!(
        w.contains(&tr!("feature-sketch-projection-lost", count = 1)),
        "{w}"
    );
    let after = line_ends3(&d2, s2, line);
    assert!(close3(after.0, before.0) && close3(after.1, before.1));
    d2.check_invariants().unwrap();
}

#[test]
fn sketch_curve_projection_and_section_points_follow() {
    let mut d = Document::new();
    // Skizze 1 auf XZ: Kreis um den Ursprung mit r = 5 und eine schräge Linie.
    let mut sk1 = Sketch::new(StandardPlane::XZ.frame(), "Skizze1", tol());
    let m = sk1.add_point(0.0, 0.0).unwrap();
    let circle = sk1.add_circle(m, 5.0).unwrap();
    let p = sk1.add_point(0.0, 2.0).unwrap();
    let q = sk1.add_point(10.0, 12.0).unwrap();
    let slanted = sk1.add_line(p, q).unwrap();
    let s1 = d
        .add_feature("Skizze1", sketch_kind(sk1, StandardPlane::XZ, vec![]))
        .unwrap();
    let world = |c: CurveId| d.sketch_of(s1).unwrap().curve_world(c).unwrap();
    let mut sk2 = Sketch::new(Frame::WORLD, "Skizze2", tol());
    // Projizieren: die Linie in die XY-Ebene geworfen.
    let (lc, l0, l1) = world(slanted);
    let projected = sk2
        .add_projected_curve(
            &lc,
            l0,
            l1,
            planora_sketch::helpers::ProjectStyle {
                construction: false,
            },
        )
        .unwrap();
    // Schneiden: der Kreis trifft die XY-Ebene in (±5, 0, 0).
    let section = CopySource::SketchCurve {
        sketch: s1,
        curve: circle,
    };
    let mut copies = vec![EdgeCopy::new(
        projected,
        CopySource::SketchCurve {
            sketch: s1,
            curve: slanted,
        },
    )];
    for (k, x) in [5.0, -5.0].into_iter().enumerate() {
        let ids = sk2
            .add_included(&Included::Point(DVec3::new(x, 0.0, 0.0)))
            .unwrap();
        copies.push(EdgeCopy {
            target: CopyTarget::Point(ids.points[0]),
            source: section.clone(),
            piece: k as u32,
            of: 2,
        });
    }
    let s2 = d
        .add_feature("Skizze2", sketch_kind(sk2, StandardPlane::XY, copies))
        .unwrap();
    ok(&d, s2);
    assert!(
        d.feature(s2).unwrap().warning().is_none(),
        "{}",
        warning(&d, s2)
    );

    // Verschobener Kreis und längere Linie: Projektion und Schnittpunkte folgen.
    d.update(s1, |k| {
        if let FeatureKind::Sketch(sf) = k {
            sf.sketch.set_point_xy(m, 3.0, 0.0).unwrap();
            let CurveKind::Line { b, .. } = sf.sketch.curve(slanted).unwrap().kind else {
                panic!("keine Linie");
            };
            sf.sketch.set_point_xy(b, 30.0, 12.0).unwrap();
        }
    })
    .unwrap();
    ok(&d, s2);
    assert!(
        d.feature(s2).unwrap().warning().is_none(),
        "{}",
        warning(&d, s2)
    );
    let sk = d.sketch_of(s2).unwrap();
    let def = sketch_def(&d, s2);
    let mut xs: Vec<f64> = def.copies[1..]
        .iter()
        .map(|c| sk.point_xyz(point_of(c)).unwrap())
        .inspect(|p| assert!(p.y.abs() < 1e-7 && p.z.abs() < 1e-9, "{p}"))
        .map(|p| p.x)
        .collect();
    xs.sort_by(f64::total_cmp);
    assert!(
        (xs[0] + 2.0).abs() < 1e-6 && (xs[1] - 8.0).abs() < 1e-6,
        "{xs:?}"
    );
    let (a, b) = line_ends3(&d, s2, projected);
    assert!(
        close3(a, DVec3::ZERO) && close3(b, DVec3::new(30.0, 0.0, 0.0)),
        "{a} {b}"
    );
}

#[test]
fn include_plane_axis_vertex_and_origin() {
    let mut d = Document::new();
    let b = add_box(&mut d, "Quader", DVec3::new(10.0, 20.0, 5.0), DVec3::ZERO);
    let edge = vertical_edge(&body(&d, b));
    let mut sk = Sketch::new(Frame::WORLD, "Skizze", tol());
    let mut copies = include_into(
        &d,
        &mut sk,
        IncludeRef::Plane(PlaneId::Standard(StandardPlane::XZ)),
    );
    assert_eq!(copies.len(), 4, "vier Randlinien");
    copies.extend(include_into(
        &d,
        &mut sk,
        IncludeRef::Axis(planes::StdAxis::Z),
    ));
    copies.extend(include_into(
        &d,
        &mut sk,
        IncludeRef::Vertex(VertexRef {
            edge: edge.clone(),
            end: true,
        }),
    ));
    copies.extend(include_into(&d, &mut sk, IncludeRef::Origin));
    let vertex = point_of(&copies[5]);
    let s = d
        .add_feature("Skizze", sketch_kind(sk, StandardPlane::XY, copies))
        .unwrap();
    ok(&d, s);
    assert!(
        d.feature(s).unwrap().warning().is_none(),
        "{}",
        warning(&d, s)
    );
    let v = d.sketch_of(s).unwrap().point_xyz(vertex).unwrap();
    assert!((v.x - 10.0).abs() < 1e-9 && v.y.abs() < 1e-9, "{v}");
    set_box_x(&mut d, b, 30.0);
    ok(&d, s);
    assert!(
        d.feature(s).unwrap().warning().is_none(),
        "{}",
        warning(&d, s)
    );
    let v = d.sketch_of(s).unwrap().point_xyz(vertex).unwrap();
    assert!((v.x - 30.0).abs() < 1e-9, "{v}");
    // Das Rechteck der Ebene wächst mit den Körpern (0,6 × Diagonale), die Lage bleibt in XZ.
    let def = sketch_def(&d, s);
    let half = include_plane_half(&[&body(&d, b)]);
    for c in &def.copies[..4] {
        let (p0, p1) = line_ends3(&d, s, curve_of(c));
        assert!(p0.y.abs() < 1e-9 && p1.y.abs() < 1e-9);
        assert!((p0.x.abs() - half).abs() < 1e-9 || (p0.z.abs() - half).abs() < 1e-9);
    }
}

#[test]
fn construction_point_placeholder_is_a_lost_reference() {
    let mut d = Document::new();
    let mut sk = Sketch::new(Frame::WORLD, "Skizze", tol());
    let ids = sk
        .add_included(&Included::Point(DVec3::new(1.0, 2.0, 3.0)))
        .unwrap();
    let copy = EdgeCopy {
        target: CopyTarget::Point(ids.points[0]),
        source: CopySource::Include(IncludeRef::ConstructionPoint(FeatureId(77))),
        piece: 0,
        of: 1,
    };
    let s = d
        .add_feature("Skizze", sketch_kind(sk, StandardPlane::XY, vec![copy]))
        .unwrap();
    ok(&d, s);
    assert!(warning(&d, s).contains(&tr!("feature-sketch-projection-lost", count = 1)));
    let p = d.sketch_of(s).unwrap().point_xyz(ids.points[0]).unwrap();
    assert!(close3(p, DVec3::new(1.0, 2.0, 3.0)));
}

#[test]
fn deleting_highest_piece_gives_no_mismatch() {
    let mut d = Document::new();
    let b = add_box(&mut d, "Quader", DVec3::new(10.0, 20.0, 5.0), DVec3::ZERO);
    let face = top_face(&body(&d, b));
    let mut sk = Sketch::new(Frame::WORLD, "Skizze", tol());
    let copies = include_into(&d, &mut sk, IncludeRef::FaceBoundary(face));
    assert_eq!(copies.len(), 4);
    assert!(copies.iter().all(|c| c.of == 4));
    let lines: Vec<CurveId> = copies.iter().map(curve_of).collect();
    let s = d
        .add_feature("Skizze", sketch_kind(sk, StandardPlane::XY, copies))
        .unwrap();
    ok(&d, s);
    // Der Benutzer löscht das Stück mit dem höchsten Index.
    d.update(s, |k| {
        if let FeatureKind::Sketch(sf) = k {
            sf.sketch.remove_curve(lines[3]).unwrap();
            assert!(sf.prune_copies());
        }
    })
    .unwrap();
    ok(&d, s);
    assert!(
        d.feature(s).unwrap().warning().is_none(),
        "{}",
        warning(&d, s)
    );
    assert_eq!(sketch_def(&d, s).copies.len(), 3);

    set_box_x(&mut d, b, 30.0);
    ok(&d, s);
    assert!(
        d.feature(s).unwrap().warning().is_none(),
        "{}",
        warning(&d, s)
    );
    // Stücke 0 bis 2 folgen nach ihrem Index; das gelöschte bleibt gelöscht.
    let sk = d.sketch_of(s).unwrap();
    assert!(sk.curve(lines[3]).is_none());
    let expected = included_pieces(
        &d,
        d.index_of(s).unwrap(),
        &Frame::WORLD,
        &IncludeRef::FaceBoundary(top_face(&body(&d, b))),
    )
    .unwrap();
    for (k, &c) in lines[..3].iter().enumerate() {
        let Included::Line(e0, e1) = expected[k] else {
            panic!("keine Linie");
        };
        let (p0, p1) = line_ends3(&d, s, c);
        assert!(
            close3(p0, e0) && close3(p1, e1),
            "{k}: {p0} {p1} / {e0} {e1}"
        );
    }
    assert_eq!(sketch_def(&d, s).copies.len(), 3);
    d.check_invariants().unwrap();
}
