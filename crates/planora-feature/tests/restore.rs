//! Zurückladen eines Bildes ins Dokument (Rückgängig und Wiederholen, Aufgabe 3):
//! Inhaltsgleichheit, voller Weg und Zeichnungsweg, steigende Zähler, Fehler ohne Folgen,
//! keine erneute Prüfung bekannter Netze, Gleichheit von Skizzen nach ihrem Stand.

use planora_drawing::dimension::{
    DimId, DimKind, DimSpec, GeoRef, GeoRefData, TagData, Tolerance as DimTolerance,
};
use planora_drawing::sheet_info::{FieldValue, NotesValue, ProjectInfo};
use planora_drawing::{
    BodyKey, DrawingId, Paper, Placement, Projection, RowOverride, Scale, Sheet, ViewId, ViewKind,
};
use planora_exchange::{ExchangeMesh, MeshTransform};
use planora_feature::*;
use planora_kernel::Solid;
use planora_math::{DVec3, Tolerance};
use planora_sketch::Sketch;
use std::sync::Arc;

const A4: Sheet = Sheet {
    paper: Paper::A4,
    landscape: false,
};
const S11: Scale = Scale { num: 1, den: 1 };
const FREE: Placement = Placement::Free { x: 0.0, y: 0.0 };

fn bx(size: f64) -> FeatureKind {
    FeatureKind::Box {
        size: DVec3::splat(size),
        origin: DVec3::ZERO,
    }
}

fn sketch_feature() -> SketchFeature {
    let mut sk = Sketch::new(StandardPlane::XY.frame(), "Skizze", Tolerance::default());
    let a = sk.add_point(0.0, 0.0).unwrap();
    let b = sk.add_point(10.0, 0.0).unwrap();
    sk.add_line(a, b).unwrap();
    SketchFeature {
        sketch: sk,
        plane: SketchPlane::Standard(StandardPlane::XY),
        copies: vec![],
    }
}

fn tetra() -> Arc<ExchangeMesh> {
    Arc::new(ExchangeMesh {
        positions: vec![
            [0.0, 0.0, 0.0],
            [10.0, 0.0, 0.0],
            [0.0, 10.0, 0.0],
            [0.0, 0.0, 10.0],
        ],
        indices: vec![0, 2, 1, 0, 1, 3, 0, 3, 2, 1, 2, 3],
    })
}

fn project(login: &str) -> ProjectInfo {
    ProjectInfo {
        login: login.into(),
        owner: FieldValue::Auto,
        creator: FieldValue::Auto,
        checked: FieldValue::Auto,
        approved: FieldValue::Auto,
        material: FieldValue::Auto,
        tolerance: FieldValue::Auto,
        notes: NotesValue::Auto,
    }
}

fn name_override(t: &str) -> RowOverride {
    RowOverride {
        name: FieldValue::Text(t.into()),
        ..RowOverride::default()
    }
}

fn horizontal(view: ViewId) -> DimSpec {
    let tag = |role: &str| TagData {
        origin: 1,
        role: role.into(),
        hint: DVec3::ZERO,
    };
    let edge = |a: &str, b: &str| {
        GeoRef::Edge(GeoRefData::Edge {
            a: tag(a),
            b: tag(b),
            hint: DVec3::new(1.0, 2.0, 3.0),
        })
    };
    DimSpec {
        id: DimId(0),
        view,
        kind: DimKind::Horizontal,
        refs: vec![edge("Wand 1", "Wand 2"), edge("Wand 1", "Wand 4")],
        offset: 10.0,
        angle: 45.0,
        text_shift: (0.0, 0.0),
        tolerance: DimTolerance::None,
        decimals: 1,
        text_override: None,
        chain: None,
        suggested: false,
    }
}

/// Quader, Skizze, Netz-Körper, Zeichnung mit Ansicht, Projektangaben und Stücklistentext.
struct Rich {
    doc: Document,
    quader: FeatureId,
    drawing: DrawingId,
    view: ViewId,
}

fn rich() -> Rich {
    let mut doc = Document::new();
    let quader = doc.add_feature("Quader 1", bx(10.0)).unwrap();
    doc.add_feature("Skizze 1", FeatureKind::Sketch(sketch_feature()))
        .unwrap();
    doc.add_mesh_body("Netz 1", tetra(), MeshTransform::IDENTITY)
        .unwrap();
    let drawing = doc.add_drawing("Blatt", A4, Projection::First).unwrap();
    let view = doc
        .add_view(drawing, ViewKind::Front, S11, FREE, None)
        .unwrap();
    doc.set_project_info(project("dennis")).unwrap();
    let key = doc.body_key(BodyRef {
        feature: quader,
        index: 0,
    });
    doc.set_parts_override(key, name_override("Grundplatte"))
        .unwrap();
    Rich {
        doc,
        quader,
        drawing,
        view,
    }
}

fn kind_of(s: &mut Snapshot, id: FeatureId) -> &mut FeatureKind {
    &mut s.features.iter_mut().find(|f| f.id == id).unwrap().kind
}

fn sketch_id(s: &Snapshot) -> FeatureId {
    s.features
        .iter()
        .find(|f| matches!(f.kind, FeatureKind::Sketch(_)))
        .unwrap()
        .id
}

/// Hüllbox aller Ecken eines Körpers.
fn hull(s: &Solid) -> (DVec3, DVec3) {
    let mut lo = DVec3::splat(f64::INFINITY);
    let mut hi = DVec3::splat(f64::NEG_INFINITY);
    for (_, v) in s.vertices.iter() {
        lo = lo.min(v.point);
        hi = hi.max(v.point);
    }
    (lo, hi)
}

fn hulls(d: &Document) -> Vec<(BodyRef, (DVec3, DVec3))> {
    d.visible_bodies()
        .iter()
        .map(|b| (b.body, hull(b.solid)))
        .collect()
}

fn counters(d: &Document) -> (u64, u64) {
    (d.edit_count(), d.model_edit_count())
}

#[test]
fn same_content_detects_each_field() {
    let r = rich();
    let a = r.doc.snapshot();
    assert!(a.same_content(&a));
    assert!(a.same_model(&a));
    assert!(a.same_content(&r.doc.snapshot()), "zweites Bild ist gleich");
    assert!(a.same_content(&a.clone()));

    // (Name, Änderung, betrifft das Modell)
    type Change = Box<dyn Fn(&mut Snapshot)>;
    let quader = r.quader;
    let changes: Vec<(&str, Change, bool)> = vec![
        (
            "Name",
            Box::new(|s: &mut Snapshot| s.features[0].name = "Quader 9".into()),
            true,
        ),
        (
            "unterdrückt",
            Box::new(|s: &mut Snapshot| s.features[0].suppressed = true),
            true,
        ),
        (
            "Maß des Quaders",
            Box::new(move |s: &mut Snapshot| {
                if let FeatureKind::Box { size, .. } = kind_of(s, quader) {
                    size.x += 1.0;
                }
            }),
            true,
        ),
        (
            "Skizzenpunkt verschoben",
            Box::new(|s: &mut Snapshot| {
                let id = sketch_id(s);
                if let FeatureKind::Sketch(sf) = kind_of(s, id) {
                    let p = sf.sketch.points().next().map(|(id, _)| id).unwrap();
                    sf.sketch.set_point_xy(p, 1.0, 1.0).unwrap();
                }
            }),
            true,
        ),
        ("Marker", Box::new(|s: &mut Snapshot| s.marker -= 1), true),
        ("next_id", Box::new(|s: &mut Snapshot| s.next_id += 1), true),
        (
            "anderes Netz-Arc mit gleichem Inhalt",
            Box::new(|s: &mut Snapshot| {
                let m = s.meshes.values_mut().next().unwrap();
                *m = Arc::new((**m).clone());
            }),
            true,
        ),
        (
            "Ansicht verschoben",
            Box::new(|s: &mut Snapshot| {
                s.drawings[0].views[0].placement = Placement::Free { x: 5.0, y: 0.0 }
            }),
            false,
        ),
        (
            "Zeichnungszähler",
            Box::new(|s: &mut Snapshot| s.next_drawing += 1),
            false,
        ),
        (
            "Projektangabe",
            Box::new(|s: &mut Snapshot| {
                s.project_info.as_mut().unwrap().material = FieldValue::Text("Stahl".into())
            }),
            false,
        ),
        (
            "Stücklistentext",
            Box::new(|s: &mut Snapshot| {
                *s.parts_overrides.values_mut().next().unwrap() = name_override("Sockel")
            }),
            false,
        ),
    ];
    for (name, change, model) in changes {
        let mut b = a.clone();
        change(&mut b);
        assert!(
            !a.same_content(&b),
            "{name}: same_content muss ungleich sein"
        );
        assert!(
            !b.same_content(&a),
            "{name}: same_content muss ungleich sein (andersrum)"
        );
        assert_eq!(
            !a.same_model(&b),
            model,
            "{name}: same_model ungleich genau bei Modellfeldern"
        );
    }
    // ohne Netz: Schlüssel fehlt
    let mut b = a.clone();
    b.meshes.clear();
    assert!(!a.same_model(&b) && !b.same_model(&a));
}

#[test]
fn restore_full_roundtrip() {
    let mut r = rich();
    let a = r.doc.snapshot();
    let before = hulls(&r.doc);
    let names: Vec<_> = r.doc.features().iter().map(|f| f.name.clone()).collect();
    r.doc
        .add_feature(
            "Zylinder 1",
            FeatureKind::Cylinder {
                radius: 2.0,
                height: 5.0,
                origin: DVec3::new(30.0, 0.0, 0.0),
            },
        )
        .unwrap();
    r.doc.delete(r.quader).unwrap();
    assert!(!r.doc.snapshot().same_content(&a));
    assert_eq!(r.doc.restore(a.clone()), Ok(RestoreKind::Full));
    assert!(r.doc.snapshot().same_content(&a));
    r.doc.check_invariants().unwrap();
    let names_after: Vec<_> = r.doc.features().iter().map(|f| f.name.clone()).collect();
    assert_eq!(names, names_after);
    let after = hulls(&r.doc);
    assert_eq!(before.len(), after.len());
    for ((ba, (la, ha)), (bb, (lb, hb))) in before.iter().zip(&after) {
        assert_eq!(ba, bb);
        assert!(la.abs_diff_eq(*lb, 1e-9) && ha.abs_diff_eq(*hb, 1e-9));
    }
    assert_eq!(r.doc.visible_meshes().len(), 1);
    // Nach dem Zurückladen geht es mit neuen Kennungen weiter, nie mit alten.
    let next = r
        .doc
        .add_feature("Quader 2", bx(3.0))
        .expect("Anlegen nach dem Zurückladen");
    assert_eq!(next.0, a.next_id);
}

#[test]
fn counters_keep_rising() {
    let mut r = rich();
    let a = r.doc.snapshot();
    r.doc.add_feature("Quader 2", bx(4.0)).unwrap();
    let b = r.doc.snapshot();
    r.doc.add_feature("Quader 3", bx(5.0)).unwrap();

    let c0 = counters(&r.doc);
    assert_eq!(r.doc.restore(b.clone()), Ok(RestoreKind::Full));
    let c1 = counters(&r.doc);
    assert!(c1.0 > c0.0 && c1.1 > c0.1, "{c0:?} → {c1:?}");
    // R1: voller Weg erhöht beide Teilzähler um 1, `edit_count` also um 2.
    assert_eq!(c1, (c0.0 + 2, c0.1 + 1), "{c0:?} → {c1:?}");
    assert_eq!(r.doc.restore(a.clone()), Ok(RestoreKind::Full));
    let c2 = counters(&r.doc);
    assert!(c2.0 > c1.0 && c2.1 > c1.1, "{c1:?} → {c2:?}");
    assert!(c0 != c1 && c1 != c2 && c0 != c2);
    // Feature-Versionen werden nicht wiederverwendet (Dialoge erkennen veraltete Kopien).
    let v_before: Vec<u64> = r.doc.features().iter().map(|f| f.version()).collect();
    assert_eq!(r.doc.restore(b.clone()), Ok(RestoreKind::Full));
    assert!(
        r.doc
            .features()
            .iter()
            .all(|f| v_before.iter().all(|v| f.version() > *v))
    );

    // Gegenprobe: aus dem Bild neu gebaut wären beide Zähler 0.
    let fresh = Document::from_snapshot(a, 0.05).unwrap();
    assert_eq!(counters(&fresh), (0, 0));
}

#[test]
fn restore_annotations_only() {
    let mut r = rich();
    let a = r.doc.snapshot();
    r.doc
        .update_drawing(r.drawing, |d| {
            d.views[0].placement = Placement::Free { x: 20.0, y: -5.0 }
        })
        .unwrap();
    r.doc.add_dimension(r.drawing, horizontal(r.view)).unwrap();
    assert!(!r.doc.snapshot().same_content(&a));

    let (edit, model) = counters(&r.doc);
    let bodies: Vec<*const Solid> = r
        .doc
        .visible_bodies()
        .iter()
        .map(|b| b.solid as *const Solid)
        .collect();
    let versions: Vec<u64> = r.doc.features().iter().map(|f| f.version()).collect();

    assert_eq!(r.doc.restore(a.clone()), Ok(RestoreKind::Annotations));
    assert_eq!(r.doc.model_edit_count(), model);
    assert_eq!(r.doc.edit_count(), edit + 1);
    let bodies_after: Vec<*const Solid> = r
        .doc
        .visible_bodies()
        .iter()
        .map(|b| b.solid as *const Solid)
        .collect();
    assert_eq!(bodies, bodies_after, "Körper nicht neu berechnet");
    let versions_after: Vec<u64> = r.doc.features().iter().map(|f| f.version()).collect();
    assert_eq!(versions, versions_after);
    assert_eq!(r.doc.drawings(), a.drawings.as_slice());
    assert!(r.doc.snapshot().same_content(&a));
    r.doc.check_invariants().unwrap();

    // Projektangaben und Stücklistentexte gehen auf demselben Weg zurück.
    let key = *a.parts_overrides.keys().next().unwrap();
    r.doc
        .set_parts_override(key, name_override("Sockel"))
        .unwrap();
    r.doc.set_project_info(project("anna")).unwrap();
    r.doc.clear_project_info();
    let (edit, model) = counters(&r.doc);
    assert_eq!(r.doc.restore(a.clone()), Ok(RestoreKind::Annotations));
    assert!(r.doc.snapshot().same_content(&a));
    assert_eq!(counters(&r.doc), (edit + 1, model));
}

#[test]
fn restore_failure_leaves_document() {
    let mut r = rich();
    r.doc.add_feature("Quader 2", bx(4.0)).unwrap();
    let good = r.doc.snapshot();
    let check = |doc: &mut Document, bad: Snapshot, what: &str| {
        let before = doc.snapshot();
        let c = counters(doc);
        let bodies = hulls(doc);
        assert!(doc.restore(bad).is_err(), "{what}: Fehler erwartet");
        assert!(
            doc.snapshot().same_content(&before),
            "{what}: Dokument geändert"
        );
        assert_eq!(counters(doc), c, "{what}: Zähler geändert");
        assert_eq!(hulls(doc), bodies, "{what}: Körper geändert");
        doc.check_invariants().unwrap();
    };

    // doppelte Feature-Kennung (voller Weg)
    let mut dup = good.clone();
    dup.features[1].id = dup.features[0].id;
    check(&mut r.doc, dup, "doppelte Kennung");

    // Marker hinter dem Ende
    let mut marker = good.clone();
    marker.marker = marker.features.len() + 1;
    check(&mut r.doc, marker, "Marker");

    // ungültige Projektangaben auf dem Zeichnungsweg
    let mut info = good.clone();
    info.project_info = Some(project(&"x".repeat(10_000)));
    assert!(info.same_model(&r.doc.snapshot()), "Zeichnungsweg");
    check(&mut r.doc, info, "Projektangaben");

    // ungültiger Stücklistentext auf dem Zeichnungsweg
    let mut parts = good.clone();
    parts.parts_overrides.insert(
        BodyKey {
            feature: 1,
            index: 0,
        },
        name_override(&"y".repeat(10_000)),
    );
    check(&mut r.doc, parts, "Stücklistentext");

    // ungültige Zeichnung auf dem Zeichnungsweg, zusammen mit gültigen Projektangaben
    let mut drawing = good.clone();
    drawing.project_info = Some(project("anna"));
    drawing.next_drawing = 0;
    check(&mut r.doc, drawing, "Zeichnungszähler");

    // Fehler spät im vollen Weg (Zeichnung ungültig, Modell anders)
    let mut late = good.clone();
    late.features.pop();
    late.drawings[0].views[0].placement = Placement::Free {
        x: f64::NAN,
        y: 0.0,
    };
    check(&mut r.doc, late, "Zeichnung im vollen Weg");

    // danach geht es normal weiter
    assert_eq!(r.doc.restore(good.clone()), Ok(RestoreKind::Annotations));
}

#[test]
fn known_meshes_not_revalidated() {
    let mut d = Document::new();
    d.add_mesh_body("Netz 1", tetra(), MeshTransform::IDENTITY)
        .unwrap();
    let a = d.snapshot();
    d.add_feature("Quader 1", bx(5.0)).unwrap();

    let n0 = mesh_validations();
    assert_eq!(d.restore(a.clone()), Ok(RestoreKind::Full));
    assert_eq!(
        mesh_validations(),
        n0,
        "derselbe Arc wird nicht erneut geprüft"
    );

    let mut fresh = a.clone();
    let m = fresh.meshes.values_mut().next().unwrap();
    *m = Arc::new((**m).clone());
    assert_eq!(d.restore(fresh), Ok(RestoreKind::Full));
    assert_eq!(mesh_validations(), n0 + 1, "neuer Arc wird geprüft");

    let n1 = mesh_validations();
    Document::from_snapshot(a.clone(), 0.05).unwrap();
    assert_eq!(mesh_validations(), n1 + 1, "Laden prüft wie bisher");

    // Ein neuer, kaputter Arc wird abgewiesen.
    let mut broken = a.clone();
    *broken.meshes.values_mut().next().unwrap() = Arc::new(ExchangeMesh {
        positions: vec![[f32::NAN; 3]; 3],
        indices: vec![0, 1, 2],
    });
    d.add_feature("Quader 2", bx(5.0)).unwrap();
    let before = d.snapshot();
    assert!(d.restore(broken).is_err());
    assert!(d.snapshot().same_content(&before));
}

#[test]
fn mesh_ids_are_not_reused_after_restore() {
    let mut d = Document::new();
    d.add_feature("Quader 1", bx(5.0)).unwrap();
    let a = d.snapshot();
    let n = d
        .add_mesh_body("Netz 1", tetra(), MeshTransform::IDENTITY)
        .unwrap();
    let first = match d.feature(n).unwrap().kind {
        FeatureKind::MeshBody { mesh, .. } => mesh,
        _ => unreachable!(),
    };
    assert_eq!(d.restore(a), Ok(RestoreKind::Full));
    let m = d.add_mesh(tetra()).unwrap();
    assert!(m.0 > first.0, "Netz-Kennung {m:?} nach {first:?}");
}

#[test]
fn sketch_equality_by_revision() {
    let a = sketch_feature();
    let b = a.clone();
    assert_eq!(a, b);
    assert_eq!(
        FeatureKind::Sketch(a.clone()),
        FeatureKind::Sketch(b.clone())
    );
    let mut c = a.clone();
    c.sketch.add_point(5.0, 5.0).unwrap();
    assert_ne!(a, c);
    let separate = sketch_feature();
    assert_ne!(a, separate, "getrennt gebaute Skizzen sind ungleich (R5)");
    // gleiche Skizze, andere Ebene bzw. andere Kopien
    let mut plane = a.clone();
    plane.plane = SketchPlane::Standard(StandardPlane::XZ);
    assert_ne!(a, plane);
    // gleicher Stand, anderer Skizzenname
    let mut named = a.clone();
    named.sketch.name = "Umbenannt".into();
    assert_eq!(named.sketch.revision(), a.sketch.revision());
    assert_ne!(a, named);
    let mut copies = a.clone();
    copies.copies.push(EdgeCopy {
        curve: a.sketch.curves().next().map(|(id, _)| id).unwrap(),
        edge: EdgeRef {
            a: FaceRef {
                origin: 1,
                role: "Wand 1".into(),
                hint: DVec3::ZERO,
            },
            b: FaceRef {
                origin: 1,
                role: "Wand 2".into(),
                hint: DVec3::ZERO,
            },
            hint: DVec3::ZERO,
        },
    });
    assert_ne!(a, copies);
}

#[test]
fn renamed_sketch_with_same_revision_is_restored() {
    let mut r = rich();
    let current = r.doc.snapshot();
    let mut renamed = current.clone();
    let id = sketch_id(&renamed);
    let FeatureKind::Sketch(sf) = kind_of(&mut renamed, id) else {
        unreachable!()
    };
    let revision = sf.sketch.revision();
    sf.sketch.name = "Umbenannt".into();
    assert!(!renamed.same_content(&current));
    assert!(!renamed.same_model(&current));
    assert_eq!(r.doc.restore(renamed.clone()), Ok(RestoreKind::Full));
    let FeatureKind::Sketch(sf) = &r.doc.feature(id).unwrap().kind else {
        unreachable!()
    };
    assert_eq!(sf.sketch.name, "Umbenannt");
    assert_eq!(sf.sketch.revision(), revision);
    assert!(r.doc.snapshot().same_content(&renamed));
}
