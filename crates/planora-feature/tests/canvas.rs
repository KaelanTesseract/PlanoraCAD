//! Leinwand im Modell (Skizzen 9, Task 8): Lage auf dem Träger, Folgen und verlorene Fläche,
//! Bildtabelle im Snapshot und Ordner-Schalter.

use planora_exchange::ImageFormat;
use planora_feature::*;
use planora_kernel::Solid;
use planora_math::{DVec2, DVec3, Frame, Tolerance};
use planora_sketch::Sketch;
use std::sync::Arc;

const EPS: f64 = 1e-9;

fn image(width: u32, height: u32, bytes: &[u8], hash: u64) -> CanvasImage {
    CanvasImage {
        bytes: Arc::from(bytes),
        format: ImageFormat::Png,
        width,
        height,
        hash,
    }
}

fn canvas(image: ImageId, base: CanvasBase) -> CanvasFeature {
    CanvasFeature::new(image, base)
}

fn add_canvas(d: &mut Document, f: CanvasFeature) -> FeatureId {
    let id = d.add_feature("Leinwand1", FeatureKind::Canvas(f)).unwrap();
    let feat = d.feature(id).unwrap();
    assert_eq!(feat.status, FeatureStatus::Ok, "{:?}", feat.status);
    id
}

fn corners_of(d: &Document, id: FeatureId) -> [DVec3; 4] {
    let FeatureKind::Canvas(c) = &d.feature(id).unwrap().kind else {
        panic!("keine Leinwand");
    };
    let frame = d.canvas_frame(id).expect("kein Frame");
    let img = d.image(c.image).expect("kein Bild");
    c.corners(&frame, img)
}

fn assert_near(a: DVec3, b: DVec3) {
    assert!(a.distance(b) < EPS, "{a} ≠ {b}");
}

fn assert_corners(got: [DVec3; 4], want: [DVec3; 4]) {
    for (g, w) in got.into_iter().zip(want) {
        assert_near(g, w);
    }
}

fn rect_sketch(w: f64, h: f64) -> SketchFeature {
    let mut sk = Sketch::new(Frame::WORLD, "Skizze", Tolerance::default());
    sk.add_rectangle(DVec2::ZERO, DVec2::new(w, h)).unwrap();
    SketchFeature {
        sketch: sk,
        plane: SketchPlane::Standard(StandardPlane::XY),
        copies: vec![],
        formulas: Default::default(),
    }
}

fn solid(d: &Document, f: FeatureId) -> &Solid {
    d.body(BodyRef {
        feature: f,
        index: 0,
    })
    .unwrap()
}

/// Ebene Fläche mit der äußeren Normale `n`.
fn face_with_normal(s: &Solid, n: DVec3) -> FaceRef {
    let (id, _) = s
        .faces
        .iter()
        .find(|(f, _)| planora_sketch::plane_of_face(s, *f).is_some_and(|fr| fr.z.dot(n) > 0.999))
        .expect("Fläche fehlt");
    FaceRef::from_face(s, id, &Tolerance::default()).unwrap()
}

/// Skizze 40×30 auf XY und Extrusion mit Höhe `h`; liefert die Extrusion.
fn extruded_block(d: &mut Document, h: f64) -> FeatureId {
    let s = d
        .add_feature("Skizze1", FeatureKind::Sketch(rect_sketch(40.0, 30.0)))
        .unwrap();
    d.add_feature(
        "Extrusion1",
        FeatureKind::Extrude {
            sketch: s,
            profiles: ProfileSel::Index(0),
            height: h,
        },
    )
    .unwrap()
}

fn set_height(d: &mut Document, e: FeatureId, h: f64) {
    d.update(e, |k| {
        if let FeatureKind::Extrude { height, .. } = k {
            *height = h;
        }
    })
    .unwrap();
}

#[test]
fn canvas_on_xy_corners_1px_is_1mm() {
    let mut d = Document::new();
    let img = d.add_image(image(200, 100, b"bild", 7));
    let c = canvas(img, CanvasBase::Standard(StandardPlane::XY));
    // Vorgaben wie Fusion (Spec, Global Constraints).
    assert_eq!(c.opacity, 50);
    assert!(c.display_through && c.selectable);
    assert_eq!(c.scale, DVec2::ONE);
    let id = add_canvas(&mut d, c);
    assert_corners(
        corners_of(&d, id),
        [
            DVec3::new(-100.0, 50.0, 0.0),
            DVec3::new(100.0, 50.0, 0.0),
            DVec3::new(100.0, -50.0, 0.0),
            DVec3::new(-100.0, -50.0, 0.0),
        ],
    );
    let f = d.canvas_frame(id).unwrap();
    assert_near(f.origin, DVec3::ZERO);
    assert_near(f.z, DVec3::Z);
    assert!(
        d.visible_bodies().is_empty(),
        "eine Leinwand ist kein Körper"
    );
    assert_eq!(d.feature(id).unwrap().kind.label(), "Leinwand");
    assert!(d.feature(id).unwrap().warning().is_none());
    d.check_invariants().unwrap();
}

#[test]
fn flip_and_angle() {
    let mut d = Document::new();
    let img = d.add_image(image(200, 100, b"bild", 7));
    let mut c = canvas(img, CanvasBase::Standard(StandardPlane::XY));
    c.offset = DVec2::new(10.0, 20.0);
    c.angle = std::f64::consts::FRAC_PI_2;
    c.scale = DVec2::new(2.0, 1.0);
    c.flip_h = true;
    let id = add_canvas(&mut d, c);
    // Bildmitte auf dem Versatz; x des Bildes nach 90° auf +Y, gespiegelt auf −Y, Länge 2 mm/px.
    let f = d.canvas_frame(id).unwrap();
    assert_near(f.origin, DVec3::new(10.0, 20.0, 0.0));
    assert_near(f.x, DVec3::new(0.0, -2.0, 0.0));
    assert_near(f.y, DVec3::new(-1.0, 0.0, 0.0));
    let o = DVec3::new(10.0, 20.0, 0.0);
    assert_corners(
        corners_of(&d, id),
        [
            o + DVec3::new(-50.0, 200.0, 0.0),
            o + DVec3::new(-50.0, -200.0, 0.0),
            o + DVec3::new(50.0, -200.0, 0.0),
            o + DVec3::new(50.0, 200.0, 0.0),
        ],
    );
    // Senkrecht gespiegelt statt waagrecht, ohne Drehung: oben und unten tauschen.
    d.update(id, |k| {
        if let FeatureKind::Canvas(c) = k {
            c.angle = 0.0;
            c.scale = DVec2::ONE;
            c.flip_h = false;
            c.flip_v = true;
        }
    })
    .unwrap();
    assert_corners(
        corners_of(&d, id),
        [
            o + DVec3::new(-100.0, -50.0, 0.0),
            o + DVec3::new(100.0, -50.0, 0.0),
            o + DVec3::new(100.0, 50.0, 0.0),
            o + DVec3::new(-100.0, 50.0, 0.0),
        ],
    );
    // Ungültige Werte sind ein Fehler am Feature, kein Panic.
    for bad in [
        |c: &mut CanvasFeature| c.opacity = 101,
        |c: &mut CanvasFeature| c.scale = DVec2::new(0.0, 1.0),
        |c: &mut CanvasFeature| c.angle = f64::NAN,
        |c: &mut CanvasFeature| c.offset = DVec2::new(1e12, 0.0),
    ] {
        let mut probe = canvas(img, CanvasBase::Standard(StandardPlane::XY));
        bad(&mut probe);
        let p = d.add_feature("Kaputt", FeatureKind::Canvas(probe)).unwrap();
        assert!(matches!(
            d.feature(p).unwrap().status,
            FeatureStatus::Error(_)
        ));
        assert!(d.canvas_frame(p).is_none());
        d.delete(p).unwrap();
    }
}

#[test]
fn canvas_on_face_follows_extrude_height() {
    let mut d = Document::new();
    let e = extruded_block(&mut d, 10.0);
    let top = face_with_normal(solid(&d, e), DVec3::Z);
    let img = d.add_image(image(20, 10, b"bild", 1));
    let id = add_canvas(&mut d, canvas(img, CanvasBase::Face(top)));
    let z0 = d.canvas_frame(id).unwrap();
    assert!((z0.origin.z - 10.0).abs() < EPS, "{}", z0.origin);
    assert!(corners_of(&d, id).iter().all(|c| (c.z - 10.0).abs() < EPS));
    set_height(&mut d, e, 25.0);
    let f = d.canvas_frame(id).unwrap();
    assert!((f.origin.z - 25.0).abs() < EPS, "{}", f.origin);
    assert_near(f.z, DVec3::Z);
    assert!(d.feature(id).unwrap().warning().is_none());
    // Auf einer Konstruktionsebene folgt sie ebenso.
    let p = d
        .add_feature(
            "Ebene1",
            FeatureKind::Plane(planes::PlaneFeature {
                def: planes::PlaneDef::Offset {
                    base: planes::PlaneBase::Standard(StandardPlane::XY),
                    distance: 7.0,
                },
            }),
        )
        .unwrap();
    let on_plane = add_canvas(&mut d, canvas(img, CanvasBase::Plane(p)));
    assert!((d.canvas_frame(on_plane).unwrap().origin.z - 7.0).abs() < EPS);
    assert_eq!(d.feature(on_plane).unwrap().kind.dependencies(), vec![p]);
    assert_eq!(d.feature(id).unwrap().kind.dependencies(), vec![e]);
    d.check_invariants().unwrap();
}

#[test]
fn lost_face_is_warning_keeps_last_frame() {
    let mut d = Document::new();
    let e = extruded_block(&mut d, 10.0);
    let top = face_with_normal(solid(&d, e), DVec3::Z);
    let img = d.add_image(image(20, 10, b"bild", 1));
    let id = add_canvas(&mut d, canvas(img, CanvasBase::Face(top)));
    let before = d.canvas_frame(id).unwrap();
    d.set_suppressed(e, true).unwrap();
    let f = d.feature(id).unwrap();
    assert_eq!(f.status, FeatureStatus::Ok, "Warnung statt Fehler");
    assert_eq!(
        f.warning(),
        Some(planora_i18n::tr!("feature-canvas-face-lost").as_str())
    );
    assert_eq!(
        d.canvas_frame(id),
        Some(before),
        "bleibt an der letzten Lage"
    );
    // Auch eine geänderte Einstellung behält die letzte Lage.
    d.update(id, |k| {
        if let FeatureKind::Canvas(c) = k {
            c.opacity = 80;
        }
    })
    .unwrap();
    assert_eq!(d.canvas_frame(id), Some(before));
    assert!(d.feature(id).unwrap().warning().is_some());
    // Kommt die Fläche zurück, verschwindet die Warnung.
    d.set_suppressed(e, false).unwrap();
    assert!(d.feature(id).unwrap().warning().is_none());
    assert_eq!(d.canvas_frame(id), Some(before));
}

#[test]
fn undo_snapshot_shares_image_arc() {
    let mut d = Document::new();
    let img = d.add_image(image(200, 100, &[1, 2, 3, 4], 9));
    let id = add_canvas(&mut d, canvas(img, CanvasBase::Standard(StandardPlane::XY)));
    let a = d.snapshot();
    d.update(id, |k| {
        if let FeatureKind::Canvas(c) = k {
            c.opacity = 20;
        }
    })
    .unwrap();
    let b = d.snapshot();
    assert_eq!(a.images.len(), 1);
    assert!(
        Arc::ptr_eq(&a.images[&img], &b.images[&img]),
        "Rückgängig hält nur Verweise"
    );
    assert!(Arc::ptr_eq(&a.images[&img].bytes, &b.images[&img].bytes));
    assert!(!a.same_model(&b));
    // Zurück zu `a` und wieder vor: dasselbe Bild, kein neues.
    d.restore(a.clone()).unwrap();
    assert!(Arc::ptr_eq(&d.snapshot().images[&img], &a.images[&img]));
    assert!(d.snapshot().same_model(&a));
    d.restore(b).unwrap();
    let FeatureKind::Canvas(c) = &d.feature(id).unwrap().kind else {
        panic!()
    };
    assert_eq!(c.opacity, 20);
    // Eine Leinwand ohne ihr Bild in der Tabelle: Das Bild ist kaputt und wird abgewiesen.
    let mut broken = d.snapshot();
    broken.images.clear();
    assert!(Document::from_snapshot(broken, 0.05).is_err());
    let rebuilt = Document::from_snapshot(d.snapshot(), 0.05).unwrap();
    assert!(rebuilt.canvas_frame(id).is_some());
}

#[test]
fn same_image_twice_shares_id() {
    let mut d = Document::new();
    let a = d.add_image(image(2, 2, b"gleich", 42));
    let b = d.add_image(image(2, 2, b"gleich", 42));
    assert_eq!(a, b);
    // Gleiche Prüfsumme, andere Bytes: eigenes Bild.
    let c = d.add_image(image(2, 2, b"anders", 42));
    assert_ne!(a, c);
    let x = add_canvas(&mut d, canvas(a, CanvasBase::Standard(StandardPlane::XY)));
    let y = add_canvas(&mut d, canvas(b, CanvasBase::Standard(StandardPlane::XZ)));
    let s = d.snapshot();
    assert_eq!(s.images.len(), 1, "zwei Leinwände teilen sich die Bytes");
    let FeatureKind::Canvas(cx) = &d.feature(x).unwrap().kind else {
        panic!()
    };
    let FeatureKind::Canvas(cy) = &d.feature(y).unwrap().kind else {
        panic!()
    };
    assert_eq!(cx.image, cy.image);
}

#[test]
fn unused_image_dropped_from_snapshot() {
    let mut d = Document::new();
    let used = d.add_image(image(2, 2, b"benutzt", 1));
    let unused = d.add_image(image(2, 2, b"frei", 2));
    add_canvas(
        &mut d,
        canvas(used, CanvasBase::Standard(StandardPlane::XY)),
    );
    let s = d.snapshot();
    assert!(s.images.contains_key(&used));
    assert!(!s.images.contains_key(&unused));
    // Ohne Leinwand ist die Tabelle leer.
    assert!(Document::new().snapshot().images.is_empty());
    // Ein Verweis auf ein unbekanntes Bild wird abgewiesen (wie ein fehlendes Netz).
    let edits = d.edit_count();
    let ghost = canvas(ImageId(999), CanvasBase::Standard(StandardPlane::XY));
    assert!(d.add_feature("Geist", FeatureKind::Canvas(ghost)).is_err());
    assert_eq!(d.edit_count(), edits);
}

#[test]
fn hidden_folder_canvases() {
    let mut d = Document::new();
    let img = d.add_image(image(2, 2, b"bild", 1));
    let id = add_canvas(&mut d, canvas(img, CanvasBase::Standard(StandardPlane::XY)));
    let mut v = d.visibility().clone();
    assert!(!v.folders.canvases);
    v.folders.canvases = true;
    // Eine einzelne Leinwand lässt sich ebenfalls ausblenden.
    v.features.insert(id);
    let before = d.model_edit_count();
    d.set_visibility(v);
    assert!(d.visibility().folders.canvases);
    assert!(d.visibility().features.contains(&id));
    assert!(!d.visibility().is_default());
    assert_eq!(d.model_edit_count(), before, "nur Ansicht");
    let s = d.snapshot();
    assert!(s.visibility.folders.canvases);
    assert!(s.visibility.features.contains(&id));
}
