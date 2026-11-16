//! Skizzen 11a, Task 18: „Verlorene Projektionen verwalten“ (Fusion „Manage Lost Projections“)
//! am Dokument: Neu verknüpfen, Verknüpfung aufheben, Geometrie löschen. Dazu der Randfall aus
//! Task 17: Eine verlorene Kurve behält ihre letzte Lage auch über einen vollen Neubau, solange
//! die Skizze hinter dem Marker steht.

use planora_feature::*;
use planora_i18n::tr;
use planora_kernel::Solid;
use planora_math::{DVec2, DVec3, Frame, Tolerance};
use planora_sketch::ConstraintKind;
use planora_sketch::helpers::ProjectStyle;
use planora_sketch::model::{CurveId, CurveKind, Sketch};

const LINKED: ProjectStyle = ProjectStyle {
    construction: false,
};

const XY: SketchPlane = SketchPlane::Standard(StandardPlane::XY);

fn tol() -> Tolerance {
    Tolerance::default()
}

fn body(d: &Document, feature: FeatureId) -> Solid {
    d.body(BodyRef { feature, index: 0 }).unwrap().clone()
}

fn add_box(d: &mut Document, name: &str, origin: DVec3) -> FeatureId {
    d.add_feature(
        name,
        FeatureKind::Box {
            size: DVec3::new(10.0, 20.0, 5.0),
            origin,
        },
    )
    .unwrap()
}

/// Die Kante des Körpers in der Ebene z = 0 und y = `y` (Länge 10 in x).
fn bottom_edge(solid: &Solid, y: f64) -> (EdgeRef, planora_kernel::Curve, f64, f64) {
    for (id, e) in solid.edges.iter() {
        let p0 = solid.vertices[e.start].point;
        let p1 = solid.vertices[e.end].point;
        let on = |p: DVec3| p.z.abs() < 1e-9 && (p.y - y).abs() < 1e-9;
        if on(p0) && on(p1) {
            let r = EdgeRef::from_edge(solid, id, &tol()).unwrap();
            return (r, e.curve.clone(), e.t0, e.t1);
        }
    }
    panic!("keine Kante bei y = {y}");
}

fn line_ends(sk: &Sketch, c: CurveId) -> (DVec2, DVec2) {
    match sk.curve(c).unwrap().kind {
        CurveKind::Line { a, b } => (sk.point_xy(a).unwrap(), sk.point_xy(b).unwrap()),
        ref k => panic!("keine Linie: {k:?}"),
    }
}

fn close(a: DVec2, b: DVec2) -> bool {
    (a - b).length() < 1e-7
}

/// Gleiche Strecke, gleich welche Richtung.
fn same_segment(got: (DVec2, DVec2), want: (DVec2, DVec2)) -> bool {
    (close(got.0, want.0) && close(got.1, want.1)) || (close(got.0, want.1) && close(got.1, want.0))
}

fn def(d: &Document, s: FeatureId) -> &SketchFeature {
    match &d.feature(s).unwrap().kind {
        FeatureKind::Sketch(sf) => sf,
        k => panic!("keine Skizze: {k:?}"),
    }
}

fn lost_text(count: usize) -> String {
    tr!("feature-sketch-projection-lost", count = count)
}

fn not_lost(d: &Document, s: FeatureId) {
    let f = d.feature(s).unwrap();
    assert_eq!(f.status, FeatureStatus::Ok, "{:?}", f.status);
    assert!(f.warning().is_none(), "{:?}", f.warning());
    assert!(f.lost_projections().is_empty());
}

/// Quader A (am Ursprung) und B (bei x = 30), dahinter eine Skizze auf XY mit `n` (1 oder 2)
/// verknüpft projizierten Unterkanten von A (y = 0 und y = 20), einem freien Punkt und einem
/// anzeigenden Abstandsmaß vom Punkt zur ersten Kurve. Liefert Dokument, A, B, Skizze, Kurven
/// und das Maß.
struct Rig {
    d: Document,
    a: FeatureId,
    b: FeatureId,
    s: FeatureId,
    curves: Vec<CurveId>,
    dim: planora_sketch::ConstraintId,
}

fn rig(n: usize) -> Rig {
    let mut d = Document::new();
    let a = add_box(&mut d, "Quader A", DVec3::ZERO);
    let b = add_box(&mut d, "Quader B", DVec3::new(30.0, 0.0, 0.0));
    let solid = body(&d, a);
    let mut sk = Sketch::new(Frame::WORLD, "Skizze", tol());
    let mut copies = Vec::new();
    let mut curves = Vec::new();
    for y in [0.0, 20.0].into_iter().take(n) {
        let (r, curve, t0, t1) = bottom_edge(&solid, y);
        let c = sk.add_projected_curve(&curve, t0, t1, LINKED).unwrap();
        copies.push(EdgeCopy::new(c, CopySource::Edge(r)));
        curves.push(c);
    }
    let p = sk.add_point(5.0, -8.0).unwrap();
    let dim = sk
        .add_constraint(ConstraintKind::DistancePointLine(p, curves[0], 8.0))
        .unwrap();
    sk.set_driven(dim, true).unwrap();
    let s = d
        .add_feature(
            "Skizze",
            FeatureKind::Sketch(SketchFeature {
                sketch: sk,
                plane: XY,
                copies,
                formulas: Default::default(),
            }),
        )
        .unwrap();
    not_lost(&d, s);
    Rig {
        d,
        a,
        b,
        s,
        curves,
        dim,
    }
}

/// Wie [`rig`], danach Quader A 12 lang (die Kurven folgen) und gelöscht: Alle Kurven sind
/// verloren und stehen an ihrer letzten Lage (Länge 12), nicht an der Definition (Länge 10).
fn lost_rig(n: usize) -> Rig {
    let mut r = rig(n);
    r.d.update(r.a, |k| {
        if let FeatureKind::Box { size, .. } = k {
            size.x = 12.0;
        }
    })
    .unwrap();
    r.d.delete(r.a).unwrap();
    let f = r.d.feature(r.s).unwrap();
    assert_eq!(f.status, FeatureStatus::Ok, "{:?}", f.status);
    assert!(f.warning().unwrap_or_default().contains(&lost_text(n)));
    assert_eq!(f.lost_projections().len(), n);
    let sk = r.d.sketch_of(r.s).unwrap();
    assert!((sk.line_length(r.curves[0]).unwrap() - 12.0).abs() < 1e-9);
    r
}

fn targets(r: &Rig) -> Vec<CopyTarget> {
    r.curves.iter().map(|c| CopyTarget::Curve(*c)).collect()
}

#[test]
fn relink_edge_to_other_edge() {
    let mut r = lost_rig(1);
    let (edge_b, ..) = bottom_edge(&body(&r.d, r.b), 0.0);
    r.d.relink_projection(
        r.s,
        CopyTarget::Curve(r.curves[0]),
        CopySource::Edge(edge_b.clone()),
    )
    .unwrap();
    not_lost(&r.d, r.s);
    // Die Kurve liegt auf der Unterkante von B.
    let sk = r.d.sketch_of(r.s).unwrap();
    let got = line_ends(sk, r.curves[0]);
    assert!(
        same_segment(got, (DVec2::new(30.0, 0.0), DVec2::new(40.0, 0.0))),
        "{got:?}"
    );
    // Die Herkunft ist die Kante von B, das Maß an der Kurve bleibt.
    assert_eq!(def(&r.d, r.s).copies.len(), 1);
    assert_eq!(def(&r.d, r.s).copies[0].source, CopySource::Edge(edge_b));
    let c = sk.constraint(r.dim).expect("Maß bleibt");
    assert!(c.kind.curves().contains(&r.curves[0]));
    // Der Quader B ist jetzt ein schwacher Bezug der Skizze.
    assert_eq!(
        r.d.feature(r.s).unwrap().kind.weak_dependencies(),
        vec![r.b]
    );
}

/// M-8: „Neu verknüpfen“ probt an der Skizze und übernimmt dann in genau einer Änderung; das
/// Ergebnis gleicht dem der Neuberechnung (das Ziel ist nicht mehr verloren).
#[test]
fn relink_is_one_edit_without_document_copy() {
    let mut r = lost_rig(1);
    let (edge_b, ..) = bottom_edge(&body(&r.d, r.b), 0.0);
    let edits = r.d.edit_count();
    r.d.relink_projection(
        r.s,
        CopyTarget::Curve(r.curves[0]),
        CopySource::Edge(edge_b),
    )
    .unwrap();
    assert_eq!(r.d.edit_count(), edits + 1, "genau eine Änderung");
    not_lost(&r.d, r.s);
    assert_eq!(r.d.feature(r.s).unwrap().status, FeatureStatus::Ok);
}

#[test]
fn relink_other_family_refused() {
    let mut r = lost_rig(1);
    let solid = body(&r.d, r.b);
    let (fid, _) = solid.faces.iter().next().unwrap();
    let face = FaceRef::from_face(&solid, fid, &tol()).unwrap();
    let before = r.d.snapshot();
    let edits = r.d.edit_count();
    let err =
        r.d.relink_projection(
            r.s,
            CopyTarget::Curve(r.curves[0]),
            CopySource::Section { face, index: 0 },
        )
        .unwrap_err();
    assert_eq!(err, FeatureError::Sketch(tr!("feature-relink-kind")));
    assert_eq!(r.d.edit_count(), edits, "Dokument unverändert");
    assert!(r.d.snapshot().same_content(&before));
    assert_eq!(r.d.feature(r.s).unwrap().lost_projections().len(), 1);
}

#[test]
fn same_family_pairs() {
    let r = rig(1);
    let solid = body(&r.d, r.a);
    let (edge, ..) = bottom_edge(&solid, 0.0);
    let (fid, _) = solid.faces.iter().next().unwrap();
    let face = FaceRef::from_face(&solid, fid, &tol()).unwrap();
    let e = CopySource::Edge(edge.clone());
    let sec = CopySource::Section {
        face: face.clone(),
        index: 0,
    };
    let sil = CopySource::Silhouette {
        face: face.clone(),
        index: 1,
    };
    let inc_e = CopySource::Include(IncludeRef::Edge(edge));
    let inc_f = CopySource::Include(IncludeRef::FaceBoundary(face));
    let curve = CopySource::SketchCurve {
        sketch: r.s,
        curve: r.curves[0],
    };
    assert!(e.same_family(&e));
    assert!(sec.same_family(&CopySource::Section {
        face: match &sec {
            CopySource::Section { face, .. } => face.clone(),
            _ => unreachable!(),
        },
        index: 3,
    }));
    assert!(!e.same_family(&sec));
    assert!(!sec.same_family(&sil));
    assert!(inc_e.same_family(&inc_e));
    assert!(!inc_e.same_family(&inc_f), "andere IncludeRef-Art");
    assert!(!e.same_family(&inc_e));
    assert!(curve.same_family(&curve));
    assert!(!curve.same_family(&e));
}

#[test]
fn unlink_keeps_curve_in_place() {
    let mut r = lost_rig(1);
    let before = line_ends(r.d.sketch_of(r.s).unwrap(), r.curves[0]);
    let edits = r.d.edit_count();
    assert_eq!(r.d.unlink_projections(r.s, &targets(&r)).unwrap(), 1);
    assert_eq!(r.d.edit_count(), edits + 1, "eine Änderung");
    not_lost(&r.d, r.s);
    assert!(def(&r.d, r.s).copies.is_empty());
    // An der letzten Lage (Länge 12), nicht an der Definition (Länge 10).
    let after = line_ends(r.d.sketch_of(r.s).unwrap(), r.curves[0]);
    assert!(same_segment(after, before), "{after:?} statt {before:?}");
    // Ohne Eintrag ändert nichts.
    assert_eq!(r.d.unlink_projections(r.s, &targets(&r)).unwrap(), 0);
    assert_eq!(r.d.edit_count(), edits + 1);
}

#[test]
fn delete_lost_geometry_removes_curve() {
    let mut r = lost_rig(1);
    let edits = r.d.edit_count();
    assert_eq!(r.d.delete_lost_geometry(r.s, &targets(&r)).unwrap(), 1);
    assert_eq!(r.d.edit_count(), edits + 1, "eine Änderung");
    not_lost(&r.d, r.s);
    let sk = r.d.sketch_of(r.s).unwrap();
    assert!(sk.curve(r.curves[0]).is_none(), "Kurve gelöscht");
    assert!(sk.constraint(r.dim).is_none(), "das Maß geht mit");
    assert!(def(&r.d, r.s).copies.is_empty(), "Kopie-Eintrag weg");
    // Nur noch der eigene Punkt bleibt (die Enden der Kurve gehen mit).
    assert_eq!(
        sk.points().count(),
        1,
        "{:?}",
        sk.points().map(|(p, _)| p).collect::<Vec<_>>()
    );
}

#[test]
fn break_all_and_delete_all() {
    let mut r = lost_rig(2);
    let all = targets(&r);
    assert_eq!(r.d.unlink_projections(r.s, &all).unwrap(), 2);
    not_lost(&r.d, r.s);
    let mut r = lost_rig(2);
    assert_eq!(r.d.delete_lost_geometry(r.s, &all).unwrap(), 2);
    not_lost(&r.d, r.s);
    let sk = r.d.sketch_of(r.s).unwrap();
    assert!(r.curves.iter().all(|c| sk.curve(*c).is_none()));
}

/// Randfall aus Task 17: Quelle verloren, Marker vor die Skizze, voller Neubau, Marker zurück.
/// Die Kurve steht danach an der letzten Lage (Länge 12), nicht an der Definition (Länge 10).
#[test]
fn lost_curve_keeps_last_place_behind_marker() {
    let mut r = lost_rig(1);
    let idx = r.d.index_of(r.s).unwrap();
    r.d.set_marker(idx).unwrap();
    // Voller Neubau: Rückgängig auf ein Bild mit anderem Modell.
    let snap = r.d.snapshot();
    r.d.rename(r.b, "Quader C").unwrap();
    assert_eq!(r.d.restore(snap).unwrap(), RestoreKind::Full);
    // Ein zweiter voller Neubau, solange die Skizze ohne Ergebnis ist.
    let snap = r.d.snapshot();
    r.d.rename(r.b, "Quader D").unwrap();
    assert_eq!(r.d.restore(snap).unwrap(), RestoreKind::Full);
    let len = r.d.features().len();
    r.d.set_marker(len).unwrap();
    let f = r.d.feature(r.s).unwrap();
    assert_eq!(f.lost_projections().len(), 1);
    let sk = r.d.sketch_of(r.s).unwrap();
    let l = sk.line_length(r.curves[0]).unwrap();
    assert!(
        (l - 12.0).abs() < 1e-9,
        "letzte Lage, nicht die Definition: {l}"
    );
}

/// Quader A und eine Skizze auf XY mit zwei an Ecken von A eingezogenen festen Punkten (A7): An
/// Punkt `p` hängt eine Linie des Nutzers, Punkt `lone` steht allein. Danach ist A gelöscht,
/// beide Punkte sind verlorene Projektionen. Liefert Dokument, Skizze, `p`, `lone`, Linie.
fn lost_vertices() -> (
    Document,
    FeatureId,
    planora_sketch::model::PointId,
    planora_sketch::model::PointId,
    CurveId,
) {
    use planora_sketch::helpers::Included;
    let mut d = Document::new();
    let a = add_box(&mut d, "Quader A", DVec3::ZERO);
    let solid = body(&d, a);
    let (edge, ..) = bottom_edge(&solid, 0.0);
    let (far, ..) = bottom_edge(&solid, 20.0);
    let mut sk = Sketch::new(Frame::WORLD, "Skizze", tol());
    let mut copies = Vec::new();
    let mut points = Vec::new();
    for e in [edge, far] {
        let r = IncludeRef::Vertex(VertexRef {
            edge: e,
            end: false,
        });
        let pieces = included_pieces(&d, d.features().len(), &Frame::WORLD, &r).unwrap();
        let Included::Point(at) = pieces[0] else {
            panic!("kein Punkt: {:?}", pieces[0]);
        };
        let p = sk.add_included(&Included::Point(at)).unwrap().points[0];
        copies.push(EdgeCopy {
            target: CopyTarget::Point(p),
            source: CopySource::Include(r),
            piece: 0,
            of: 1,
        });
        points.push(p);
    }
    let q = sk.add_point(25.0, 25.0).unwrap();
    let line = sk.add_line(points[0], q).unwrap();
    let s = d
        .add_feature(
            "Skizze",
            FeatureKind::Sketch(SketchFeature {
                sketch: sk,
                plane: XY,
                copies,
                formulas: Default::default(),
            }),
        )
        .unwrap();
    not_lost(&d, s);
    d.delete(a).unwrap();
    assert_eq!(d.feature(s).unwrap().lost_projections().len(), 2);
    (d, s, points[0], points[1], line)
}

/// Fusion: „Geometrie löschen“ an einer verlorenen Ecke nimmt nur den projizierten Punkt; eine
/// Linie des Nutzers an dieser Ecke bleibt, ihr Ende wird ein gewöhnlicher (nicht fester) Punkt.
#[test]
fn delete_lost_vertex_keeps_user_line() {
    let (mut d, s, p, lone, line) = lost_vertices();
    let targets = [CopyTarget::Point(p), CopyTarget::Point(lone)];
    assert_eq!(d.delete_lost_geometry(s, &targets).unwrap(), 2);
    not_lost(&d, s);
    assert!(def(&d, s).copies.is_empty(), "beide Kopie-Einträge weg");
    let sk = d.sketch_of(s).unwrap();
    assert!(sk.curve(line).is_some(), "die Linie des Nutzers bleibt");
    assert!(sk.point_xy(p).is_ok(), "ihr Ende bleibt");
    assert!(
        sk.constraints()
            .all(|(_, c)| c.kind.fixed_point() != Some(p)),
        "das Ende ist nicht mehr fest"
    );
    assert!(sk.point_xy(lone).is_err(), "der allein stehende Punkt geht");
}

/// Ruling T18-2: Kann die Kurve der neuen Quelle nicht folgen (senkrechte Kante, die auf XY nur
/// ein Punkt wäre), lehnt `relink_projection` ab; das Dokument bleibt unverändert.
#[test]
fn relink_to_unfollowable_source_refused() {
    let mut r = lost_rig(1);
    let solid = body(&r.d, r.b);
    let vertical = solid
        .edges
        .iter()
        .find(|(_, e)| {
            let (p0, p1) = (solid.vertices[e.start].point, solid.vertices[e.end].point);
            (p0.x - p1.x).abs() < 1e-9 && (p0.y - p1.y).abs() < 1e-9
        })
        .map(|(id, _)| EdgeRef::from_edge(&solid, id, &tol()).unwrap())
        .expect("senkrechte Kante");
    let before = r.d.snapshot();
    let edits = r.d.edit_count();
    let err =
        r.d.relink_projection(
            r.s,
            CopyTarget::Curve(r.curves[0]),
            CopySource::Edge(vertical),
        )
        .unwrap_err();
    assert_eq!(err, FeatureError::Sketch(tr!("lost-relink-failed")));
    assert_eq!(r.d.edit_count(), edits, "Dokument unverändert");
    assert!(r.d.snapshot().same_content(&before));
    assert_eq!(r.d.feature(r.s).unwrap().lost_projections().len(), 1);
}
