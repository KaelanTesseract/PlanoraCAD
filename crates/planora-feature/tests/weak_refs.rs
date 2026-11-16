//! Skizzen 11a, Task 17: Projektionen als schwache Bezüge (Spec A5, Abschnitt „Verlorene
//! Projektionen“). Die Herkunft einer Kopie hält die Skizze nicht fest: Löschen, Unterdrücken,
//! ein Fehler oder ein Verschieben der Quelle hinter die Skizze ergibt einen verlorenen Bezug
//! (Warnung, die Kurve bleibt an ihrer letzten Lage), keinen Fehler. Die Skizzenebene bleibt ein
//! harter Bezug.

use planora_feature::*;
use planora_i18n::tr;
use planora_kernel::{Face, Id, Solid, section_by_plane, silhouette};
use planora_math::{DVec2, DVec3, Frame, Tolerance};
use planora_sketch::helpers::ProjectStyle;
use planora_sketch::model::{CurveId, CurveKind, Sketch};

const LINKED: ProjectStyle = ProjectStyle {
    construction: false,
};

fn tol() -> Tolerance {
    Tolerance::default()
}

fn body(d: &Document, feature: FeatureId) -> Solid {
    d.body(BodyRef { feature, index: 0 }).unwrap().clone()
}

fn add_box(d: &mut Document, name: &str, size: DVec3, origin: DVec3) -> FeatureId {
    d.add_feature(name, FeatureKind::Box { size, origin })
        .unwrap()
}

fn sketch_kind(sketch: Sketch, plane: SketchPlane, copies: Vec<EdgeCopy>) -> FeatureKind {
    FeatureKind::Sketch(SketchFeature {
        sketch,
        plane,
        copies,
        formulas: Default::default(),
    })
}

const XY: SketchPlane = SketchPlane::Standard(StandardPlane::XY);
const XZ: SketchPlane = SketchPlane::Standard(StandardPlane::XZ);

fn line_ends(sk: &Sketch, c: CurveId) -> (DVec2, DVec2) {
    match sk.curve(c).unwrap().kind {
        CurveKind::Line { a, b } => (sk.point_xy(a).unwrap(), sk.point_xy(b).unwrap()),
        ref k => panic!("keine Linie: {k:?}"),
    }
}

fn close(a: DVec2, b: DVec2) -> bool {
    (a - b).length() < 1e-7
}

fn curve_copy(curve: CurveId, source: CopySource) -> EdgeCopy {
    EdgeCopy {
        target: CopyTarget::Curve(curve),
        piece: 0,
        of: 1,
        source,
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

fn find_face(solid: &Solid, pick: impl Fn(Id<Face>) -> bool) -> Id<Face> {
    solid
        .faces
        .iter()
        .map(|(f, _)| f)
        .find(|f| pick(*f))
        .expect("passende Fläche")
}

/// Die Deckfläche (Normale +z, oben) des Körpers.
fn top_face(solid: &Solid) -> FaceRef {
    let hi = solid
        .vertices
        .iter()
        .map(|(_, v)| v.point.z)
        .fold(f64::MIN, f64::max);
    let id = find_face(solid, |f| {
        planora_sketch::plane_of_face(solid, f)
            .is_some_and(|fr| fr.z.z > 0.5 && (fr.origin.z - hi).abs() < 1e-9)
    });
    FaceRef::from_face(solid, id, &tol()).unwrap()
}

/// Quader (Länge 10 in x) und eine Skizze auf XY mit der verknüpft projizierten Kante der Länge
/// 10 und einem eigenen Rechteck abseits (Profil 0 für eine Extrusion).
fn box_doc() -> (Document, FeatureId, FeatureId, CurveId) {
    let mut d = Document::new();
    let b = add_box(&mut d, "Quader", DVec3::new(10.0, 20.0, 5.0), DVec3::ZERO);
    let solid = body(&d, b);
    let mut sk = Sketch::new(Frame::WORLD, "Skizze", tol());
    let mut found = None;
    for (id, e) in solid.edges.iter() {
        let Ok(r) = EdgeRef::from_edge(&solid, id, &tol()) else {
            continue;
        };
        let mut trial = sk.clone();
        let Ok(c) = trial.add_projected_curve(&e.curve, e.t0, e.t1, LINKED) else {
            continue;
        };
        if matches!(trial.curve(c).unwrap().kind, CurveKind::Line { .. })
            && (trial.line_length(c).unwrap() - 10.0).abs() < 1e-9
        {
            sk = trial;
            found = Some((c, r));
            break;
        }
    }
    let (curve, edge) = found.expect("Kante der Länge 10");
    sk.add_rectangle(DVec2::new(50.0, 50.0), DVec2::new(55.0, 55.0))
        .unwrap();
    let s = d
        .add_feature(
            "Skizze",
            sketch_kind(sk, XY, vec![curve_copy(curve, CopySource::Edge(edge))]),
        )
        .unwrap();
    ok_without_warning(&d, s);
    (d, b, s, curve)
}

/// Quader und eine Skizze auf seiner Deckfläche (harter Bezug).
fn face_doc() -> (Document, FeatureId, FeatureId) {
    let mut d = Document::new();
    let b = add_box(&mut d, "Quader", DVec3::new(10.0, 20.0, 5.0), DVec3::ZERO);
    let face = top_face(&body(&d, b));
    let sk = Sketch::new(Frame::WORLD, "Skizze", tol());
    let s = d
        .add_feature("Skizze", sketch_kind(sk, SketchPlane::OnFace(face), vec![]))
        .unwrap();
    ok_without_warning(&d, s);
    (d, b, s)
}

/// Quader quer durch XZ, Skizze auf XZ mit dem Schnitt der Deckfläche.
fn section_doc() -> (Document, FeatureId, FeatureId) {
    let mut d = Document::new();
    let b = add_box(
        &mut d,
        "Quader",
        DVec3::new(20.0, 20.0, 10.0),
        DVec3::new(-10.0, -10.0, 0.0),
    );
    let solid = body(&d, b);
    let frame = StandardPlane::XZ.frame();
    let face = top_face(&solid);
    let top = find_face(&solid, |f| {
        FaceRef::from_face(&solid, f, &tol()).is_ok_and(|r| r == face)
    });
    let pieces = section_by_plane(&solid, &frame, Some(&[top]), &tol()).unwrap();
    let mut sk = Sketch::new(frame, "Skizze", tol());
    let p = &pieces[0];
    let curve = sk
        .add_projected_curve(&p.curve, p.t0, p.t1, LINKED)
        .unwrap();
    let s = d
        .add_feature(
            "Skizze",
            sketch_kind(
                sk,
                XZ,
                vec![curve_copy(curve, CopySource::Section { face, index: 0 })],
            ),
        )
        .unwrap();
    ok_without_warning(&d, s);
    (d, b, s)
}

/// Zylinder um z, Skizze auf XZ mit einer Mantellinie als Umriss.
fn silhouette_doc() -> (Document, FeatureId, FeatureId) {
    let mut d = Document::new();
    let c = d
        .add_feature(
            "Zylinder",
            FeatureKind::Cylinder {
                radius: 5.0,
                height: 20.0,
                origin: DVec3::ZERO,
            },
        )
        .unwrap();
    let solid = body(&d, c);
    let frame = StandardPlane::XZ.frame();
    let face = find_face(&solid, |f| {
        silhouette(&solid, f, frame.z, &tol()).is_ok_and(|p| !p.is_empty())
    });
    let pieces = silhouette(&solid, face, frame.z, &tol()).unwrap();
    let mut sk = Sketch::new(frame, "Skizze", tol());
    let p = &pieces[0];
    let curve = sk
        .add_projected_curve(&p.curve, p.t0, p.t1, LINKED)
        .unwrap();
    let face = FaceRef::from_face(&solid, face, &tol()).unwrap();
    let s = d
        .add_feature(
            "Skizze",
            sketch_kind(
                sk,
                XZ,
                vec![curve_copy(curve, CopySource::Silhouette { face, index: 0 })],
            ),
        )
        .unwrap();
    ok_without_warning(&d, s);
    (d, c, s)
}

/// Quader und eine Skizze auf XY mit der einbezogenen senkrechten Kante (3D-Geometrie).
fn include_doc() -> (Document, FeatureId, FeatureId) {
    let mut d = Document::new();
    let b = add_box(&mut d, "Quader", DVec3::new(10.0, 20.0, 5.0), DVec3::ZERO);
    let solid = body(&d, b);
    let mut edge = None;
    for (id, e) in solid.edges.iter() {
        let p0 = solid.vertices[e.start].point;
        let p1 = solid.vertices[e.end].point;
        let at = |p: DVec3| (p.x - 10.0).abs() < 1e-9 && p.y.abs() < 1e-9;
        if at(p0) && at(p1) {
            edge = Some(EdgeRef::from_edge(&solid, id, &tol()).unwrap());
        }
    }
    let r = IncludeRef::Edge(edge.expect("senkrechte Kante"));
    let pieces = included_pieces(&d, d.features().len(), &Frame::WORLD, &r).unwrap();
    let mut sk = Sketch::new(Frame::WORLD, "Skizze", tol());
    let ids = sk.add_included(&pieces[0]).unwrap();
    let s = d
        .add_feature(
            "Skizze",
            sketch_kind(
                sk,
                XY,
                vec![curve_copy(ids.curves[0], CopySource::Include(r))],
            ),
        )
        .unwrap();
    ok_without_warning(&d, s);
    (d, b, s)
}

/// Quader 20 × 20 × 5, Quellskizze mit einer Linie, Zielskizze mit „Auf Fläche projizieren“ und
/// „Schnittkurve“ der Linie auf der Deckfläche. Liefert Dokument, Quader, Quellskizze, Ziel.
fn surface_doc() -> (Document, FeatureId, FeatureId, FeatureId) {
    let mut d = Document::new();
    let b = add_box(&mut d, "Quader", DVec3::new(20.0, 20.0, 5.0), DVec3::ZERO);
    let mut src = Sketch::new(Frame::WORLD, "Quelle", tol());
    let p = src.add_point(1.0, 10.0).unwrap();
    let q = src.add_point(19.0, 10.0).unwrap();
    let line = src.add_line(p, q).unwrap();
    let q_id = d
        .add_feature("Quelle", sketch_kind(src, XY, vec![]))
        .unwrap();
    let face = top_face(&body(&d, b));
    let curve = CurveSource::SketchCurve {
        sketch: q_id,
        curve: line,
    };
    let on = CopySource::OnSurface(SurfaceProjection {
        sources: vec![curve.clone()],
        faces: vec![FaceTarget::Face(face.clone())],
        kind: ProjectType::AlongVector(DVec3::Z),
    });
    let cut = CopySource::Intersection(IntersectionSpec {
        curves: vec![curve],
        targets: vec![IntersectTarget::Face(face)],
    });
    let view = DocView::new(&d, d.features().len(), Frame::WORLD);
    let doc_tol = d.kernel().tolerance;
    let mut sk = Sketch::new(Frame::WORLD, "Ziel", tol());
    let mut copies = Vec::new();
    for source in [on, cut] {
        let pieces = match &source {
            CopySource::OnSurface(p) => {
                project_to_surface(&view, p, &doc_tol, SURFACE_PROJECT_BUDGET).unwrap()
            }
            CopySource::Intersection(p) => {
                intersection_curve(&view, p, &doc_tol, SURFACE_PROJECT_BUDGET).unwrap()
            }
            other => panic!("{other:?}"),
        };
        for (k, g) in pieces.iter().enumerate() {
            let ids = sk.add_included(g).unwrap();
            copies.push(EdgeCopy {
                target: CopyTarget::Curve(ids.curves[0]),
                source: source.clone(),
                piece: k as u32,
                of: pieces.len() as u32,
            });
        }
    }
    let t = d.add_feature("Ziel", sketch_kind(sk, XY, copies)).unwrap();
    ok_without_warning(&d, t);
    (d, b, q_id, t)
}

fn extrude(d: &mut Document, s: FeatureId) -> FeatureId {
    let e = d
        .add_feature(
            "Extrusion",
            FeatureKind::Extrude {
                sketch: s,
                profiles: ProfileSel::Index(0),
                height: 2.0,
            },
        )
        .unwrap();
    assert_eq!(d.feature(e).unwrap().status, FeatureStatus::Ok);
    e
}

fn ok_without_warning(d: &Document, s: FeatureId) {
    let f = d.feature(s).unwrap();
    assert_eq!(f.status, FeatureStatus::Ok, "{:?}", f.status);
    assert!(f.warning().is_none(), "{:?}", f.warning());
}

/// Die Testdokumente für den Vergleich der Bezüge (fester Erwartungswert, gemessen vor der
/// Aufgabe auf 244cfdd).
fn test_documents() -> Vec<Document> {
    let (mut a, _, s, _) = box_doc();
    extrude(&mut a, s);
    vec![
        a,
        face_doc().0,
        section_doc().0,
        silhouette_doc().0,
        include_doc().0,
        surface_doc().0,
    ]
}

fn lost_text() -> String {
    tr!("feature-sketch-projection-lost", count = 1)
}

/// Status `Ok`, Warnung mit einer verlorenen Projektion, das Ziel `curve` als verloren.
fn lost_one(d: &Document, s: FeatureId, curve: CurveId) {
    let f = d.feature(s).unwrap();
    assert_eq!(f.status, FeatureStatus::Ok, "{:?}", f.status);
    let w = f.warning().unwrap_or_default();
    assert!(w.contains(&lost_text()), "{w}");
    assert_eq!(f.lost_projections(), &[CopyTarget::Curve(curve)]);
}

fn not_lost(d: &Document, s: FeatureId) {
    ok_without_warning(d, s);
    assert!(d.feature(s).unwrap().lost_projections().is_empty());
}

fn length(d: &Document, s: FeatureId, c: CurveId) -> f64 {
    d.sketch_of(s).unwrap().line_length(c).unwrap()
}

/// Das Dokument mit vertauschten Features `i` und `j` (Verschieben in der Zeitleiste).
fn swapped(d: &Document, i: usize, j: usize) -> Result<Document, FeatureError> {
    let mut snap = d.snapshot();
    snap.features.swap(i, j);
    let mut out = d.clone();
    out.restore(snap)?;
    Ok(out)
}

#[test]
fn delete_body_keeps_projecting_sketch() {
    let (mut d, b, s, curve) = box_doc();
    set_box_x(&mut d, b, 12.0);
    let before = line_ends(d.sketch_of(s).unwrap(), curve);
    assert!(d.dependents(b).is_empty(), "{:?}", d.dependents(b));
    d.delete(b).unwrap();
    assert!(d.feature(s).is_some(), "Skizze bleibt");
    lost_one(&d, s, curve);
    let after = line_ends(d.sketch_of(s).unwrap(), curve);
    assert!(
        close(before.0, after.0) && close(before.1, after.1),
        "{before:?} → {after:?}"
    );
    d.check_invariants().unwrap();
}

#[test]
fn plane_on_face_stays_hard() {
    let (mut d, b, s) = face_doc();
    assert_eq!(d.dependents(b), vec![s]);
    assert!(matches!(d.delete(b), Err(FeatureError::HasDependents(_))));
    let kind = &d.feature(s).unwrap().kind;
    assert_eq!(kind.dependencies(), vec![b]);
    assert!(kind.weak_dependencies().is_empty());
}

#[test]
fn undo_restores_link() {
    let (mut d, b, s, curve) = box_doc();
    let snap = d.snapshot();
    d.delete(b).unwrap();
    lost_one(&d, s, curve);
    d.restore(snap).unwrap();
    not_lost(&d, s);
    set_box_x(&mut d, b, 13.0);
    not_lost(&d, s);
    assert!((length(&d, s, curve) - 13.0).abs() < 1e-7);
    d.check_invariants().unwrap();
}

/// Prüf-Schwerpunkt 2: Eine Skizze darf vor ihre Projektionsquelle geschoben werden; der Bezug
/// ist dann verloren (die sichtbaren Körper stammen nur von Features davor). Zurückgeschoben
/// folgt die Kurve wieder der Kante.
#[test]
fn move_sketch_before_source_and_back() {
    // Ohne vorherige Änderung des Quaders: Das Verschieben baut das Dokument neu (wie Laden),
    // die verlorene Kurve steht dann an der Lage der Definition.
    let (d, b, s, curve) = box_doc();
    let before = line_ends(d.sketch_of(s).unwrap(), curve);
    let front = swapped(&d, 0, 1).expect("kein ForwardReference");
    assert_eq!(front.index_of(s), Some(0));
    lost_one(&front, s, curve);
    let after = line_ends(front.sketch_of(s).unwrap(), curve);
    assert!(close(before.0, after.0) && close(before.1, after.1));
    assert!(front.present_weak_dependencies(0).is_empty());
    assert_eq!(front.feature(s).unwrap().kind.weak_dependencies(), vec![b]);
    front.check_invariants().unwrap();

    let mut back = swapped(&front, 0, 1).unwrap();
    assert_eq!(back.index_of(s), Some(1));
    assert_eq!(back.present_weak_dependencies(1), vec![b]);
    not_lost(&back, s);
    set_box_x(&mut back, b, 14.0);
    not_lost(&back, s);
    assert!((length(&back, s, curve) - 14.0).abs() < 1e-7);
    back.check_invariants().unwrap();
}

#[test]
fn suppressed_source_is_lost_not_skipped() {
    let (mut d, b, s, curve) = box_doc();
    let e = extrude(&mut d, s);
    d.set_suppressed(b, true).unwrap();
    lost_one(&d, s, curve);
    assert_eq!(d.feature(e).unwrap().status, FeatureStatus::Ok);
    d.check_invariants().unwrap();
}

#[test]
fn failing_source_is_lost_not_skipped() {
    let (mut d, b, s, curve) = box_doc();
    let e = extrude(&mut d, s);
    set_box_x(&mut d, b, 0.0);
    assert!(matches!(
        d.feature(b).unwrap().status,
        FeatureStatus::Error(_)
    ));
    lost_one(&d, s, curve);
    assert_eq!(d.feature(e).unwrap().status, FeatureStatus::Ok);
    d.check_invariants().unwrap();
}

#[test]
fn cache_key_follows_present_source() {
    let (mut d, b, s, curve) = box_doc();
    let e = extrude(&mut d, s);
    let r = d
        .update_with_report(b, |k| {
            if let FeatureKind::Box { size, .. } = k {
                size.x = 12.0;
            }
        })
        .unwrap();
    assert!(r.computed.contains(&s), "{r:?}");
    assert!((length(&d, s, curve) - 12.0).abs() < 1e-7);

    // Vor den Quader geschoben: Eine Änderung des Quaders rechnet die Skizze nicht neu.
    let mut front = swapped(&d, 0, 1).unwrap();
    let r = front
        .update_with_report(b, |k| {
            if let FeatureKind::Box { size, .. } = k {
                size.x = 15.0;
            }
        })
        .unwrap();
    assert!(r.reused.contains(&s), "{r:?}");
    lost_one(&front, s, curve);

    // Gelöscht: Eine Änderung anderer Features rechnet die Skizze nicht neu.
    d.delete(b).unwrap();
    let r = d
        .update_with_report(e, |k| {
            if let FeatureKind::Extrude { height, .. } = k {
                *height = 3.0;
            }
        })
        .unwrap();
    assert!(r.reused.contains(&s), "{r:?}");
    assert!(!r.computed.contains(&s), "{r:?}");
    lost_one(&d, s, curve);
}

#[test]
fn rollback_chain_includes_present_source() {
    let (mut d, b, s, _) = box_doc();
    let e = extrude(&mut d, s);
    assert!(d.chain(e).contains(&b), "{:?}", d.chain(e));
    assert!(d.chain(s).contains(&b), "{:?}", d.chain(s));
    // Rückrollstelle am Quader: Die Skizze dahinter mit Bezug auf ihn ist erfasst (wie bisher).
    let mut rolled = d.clone();
    rolled.set_rollback(b, b).unwrap();
    assert!(!rolled.is_active(s));
    rolled.check_invariants().unwrap();

    // Vor den Quader geschoben: nicht mehr in der Kette, nicht erfasst.
    let mut front = swapped(&d, 0, 1).unwrap();
    assert!(!front.chain(s).contains(&b), "{:?}", front.chain(s));
    front.set_rollback(b, b).unwrap();
    assert!(front.is_active(s));
    front.check_invariants().unwrap();

    d.delete(b).unwrap();
    assert_eq!(d.chain(s), vec![s]);
    assert!(!d.chain(e).contains(&b));
}

#[test]
fn all_kinds_of_copy_source_are_weak() {
    let (d, b, s, _) = box_doc();
    let mut docs = vec![("Kante", d, b, s)];
    let (d, b, s) = section_doc();
    docs.push(("Schnitt", d, b, s));
    let (d, b, s) = silhouette_doc();
    docs.push(("Silhouette", d, b, s));
    let (d, b, s) = include_doc();
    docs.push(("Einbeziehen", d, b, s));
    let (d, b, _, s) = surface_doc();
    docs.push(("Auf Fläche und Schnittkurve", d, b, s));
    for (name, mut d, b, s) in docs {
        let kind = &d.feature(s).unwrap().kind;
        assert!(kind.dependencies().is_empty(), "{name}");
        assert_eq!(kind.weak_dependencies(), vec![b], "{name}");
        assert!(d.dependents(b).is_empty(), "{name}");
        d.delete(b).unwrap_or_else(|e| panic!("{name}: {e}"));
        let f = d.feature(s).unwrap();
        assert_eq!(f.status, FeatureStatus::Ok, "{name}: {:?}", f.status);
        let w = f.warning().unwrap_or_default();
        assert!(!w.is_empty(), "{name}: Warnung fehlt");
        let n = match &f.kind {
            FeatureKind::Sketch(sf) => sf.copies.len(),
            _ => unreachable!(),
        };
        assert_eq!(f.lost_projections().len(), n, "{name}: {w}");
        d.check_invariants()
            .unwrap_or_else(|e| panic!("{name}: {e}"));
    }
}

/// Fester Erwartungswert: die Bezüge (`dependencies()`) jedes Features der Testdokumente,
/// gemessen vor der Aufgabe auf 244cfdd (Ruling F14).
#[test]
fn all_dependencies_equals_old_dependencies() {
    let old: Vec<Vec<Vec<u64>>> = vec![
        vec![vec![], vec![1], vec![2]],
        vec![vec![], vec![1]],
        vec![vec![], vec![1]],
        vec![vec![], vec![1]],
        vec![vec![], vec![1]],
        vec![vec![], vec![], vec![1]],
    ];
    let now: Vec<Vec<Vec<u64>>> = test_documents()
        .iter()
        .map(|d| {
            d.features()
                .iter()
                .map(|f| f.kind.all_dependencies().into_iter().map(|i| i.0).collect())
                .collect()
        })
        .collect();
    assert_eq!(now, old);
}
