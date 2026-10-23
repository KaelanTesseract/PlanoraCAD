//! STEP-Körper im Dokument (Teilprojekt STEP-Import, Aufgabe 11): Formtabelle, Anlegen ganz oder
//! gar nicht, Weiterverwenden wie jeder exakte Körper, Verlauf, Snapshot-Regeln, Zeichnung,
//! Stückliste und STEP-Export.

use planora_drawing::section::{SectionPlane, loop_polygon, polygon_area};
use planora_drawing::sheet_info::Stamp;
use planora_drawing::{DrawBody, Paper, Placement, Projection, Scale, Sheet, ViewKind};
use planora_exchange::Progress;
use planora_feature::*;
use planora_kernel::{AnalyticKernel, BoolOp, Kernel, Solid};
use planora_math::{DVec2, DVec3, Tolerance};
use planora_mesh::{TessellationOptions, mass_properties, tessellate};
use planora_sketch::Sketch;
use planora_step::{StepBody, StepOptions, normalize, write_step};
use std::sync::Arc;

const A4: Sheet = Sheet {
    paper: Paper::A4,
    landscape: false,
};
const S11: Scale = Scale { num: 1, den: 1 };

fn kernel() -> AnalyticKernel {
    AnalyticKernel::default()
}

/// Kernel-Quader 10 × 20 × 30 am Ursprung.
fn box_solid() -> Solid {
    kernel().make_box(DVec3::new(10.0, 20.0, 30.0)).unwrap()
}

/// Kernel-Kugel r 10 um (50, 0, 0).
fn sphere_solid() -> Solid {
    let k = kernel();
    let s = k.make_sphere(10.0).unwrap();
    k.transform(
        &s,
        &planora_math::DMat4::from_translation(DVec3::new(50.0, 0.0, 0.0)),
    )
    .unwrap()
}

/// Form aus dem Import: normalisierter Text samt schon gebautem Körper.
fn imported(s: &Solid, name: &str) -> Arc<StepShape> {
    let n = normalize(s, name).unwrap();
    Arc::new(StepShape::with_solid(n.text, 1e-6, n.solid))
}

/// Form wie nach dem Laden: nur der Text, gebaut wird beim Rechnen.
fn loaded(s: &Solid) -> Arc<StepShape> {
    let n = normalize(s, "Q").unwrap();
    Arc::new(StepShape::new(n.text, 1e-6).unwrap())
}

/// Text mit gültigem Kopf und Ende, aber ohne Körper.
fn broken_text() -> String {
    let text = normalize(&box_solid(), "Q").unwrap().text;
    let head = &text[..text.find("DATA;").unwrap()];
    format!("{head}DATA;\nENDSEC;\nEND-ISO-10303-21;\n")
}

fn body(name: &str, shape: Arc<StepShape>) -> NewStepBody {
    NewStepBody {
        name: name.into(),
        shape,
    }
}

fn status(d: &Document, id: FeatureId) -> FeatureStatus {
    d.feature(id).unwrap().status.clone()
}

fn first(id: FeatureId) -> BodyRef {
    BodyRef {
        feature: id,
        index: 0,
    }
}

fn volume(s: &Solid) -> f64 {
    let mesh = tessellate(
        s,
        &TessellationOptions {
            chord_tolerance: 1e-3,
        },
    )
    .unwrap();
    mass_properties(&mesh).volume
}

fn bounds(s: &Solid) -> (DVec3, DVec3) {
    let mut lo = DVec3::splat(f64::INFINITY);
    let mut hi = DVec3::splat(f64::NEG_INFINITY);
    for (_, v) in s.vertices.iter() {
        lo = lo.min(v.point);
        hi = hi.max(v.point);
    }
    (lo, hi)
}

/// Marken (Herkunft, Rolle) aller Flächen, sortiert.
fn tags(s: &Solid) -> Vec<(u64, String)> {
    let mut t: Vec<(u64, String)> = s
        .faces
        .iter()
        .map(|(_, f)| {
            let tag = f.tag.as_ref().expect("Fläche ohne Marke");
            (tag.origin, tag.role.clone())
        })
        .collect();
    t.sort();
    t
}

fn face_ref(s: &Solid, role: &str) -> FaceRef {
    let (id, _) = s
        .faces
        .iter()
        .find(|(_, f)| f.tag.as_ref().is_some_and(|t| t.role == role))
        .unwrap_or_else(|| panic!("Fläche „{role}“ fehlt"));
    FaceRef::from_face(s, id, &Tolerance::default()).unwrap()
}

/// Verweis auf die Kante zwischen den Flächen `a` und `b`.
fn edge_ref(s: &Solid, a: &str, b: &str) -> EdgeRef {
    let tol = Tolerance::default();
    s.edges
        .iter()
        .filter_map(|(e, _)| EdgeRef::from_edge(s, e, &tol).ok())
        .find(|r| (r.a.role == a && r.b.role == b) || (r.a.role == b && r.b.role == a))
        .unwrap_or_else(|| {
            let all: Vec<(String, String)> = s
                .edges
                .iter()
                .filter_map(|(e, _)| EdgeRef::from_edge(s, e, &tol).ok())
                .map(|r| (r.a.role, r.b.role))
                .collect();
            panic!("keine Kante zwischen „{a}“ und „{b}“: {all:?}")
        })
}

fn step_shape_of(d: &Document, id: FeatureId) -> StepShapeId {
    match &d.feature(id).unwrap().kind {
        FeatureKind::StepBody { shape } => *shape,
        other => panic!("kein STEP-Körper: {other:?}"),
    }
}

#[test]
fn step_body_is_one_exact_body() {
    let mut d = Document::new();
    let edits = d.edit_count();
    let ids = d
        .add_step_bodies(vec![body("Q", imported(&box_solid(), "Q"))])
        .unwrap();
    assert_eq!(ids.len(), 1);
    let id = ids[0];
    assert_eq!(d.edit_count(), edits + 1);
    let f = d.feature(id).unwrap();
    assert_eq!(f.name, "Q");
    assert_eq!(f.status, FeatureStatus::Ok);
    assert_eq!(f.kind.label(), "STEP-Körper");
    assert!(f.kind.dependencies().is_empty());
    assert!(field_keys(&f.kind).is_empty());
    let v = d.visible_bodies();
    assert_eq!(v.len(), 1);
    assert_eq!(v[0].body, first(id));
    assert_eq!(v[0].solid.faces.len(), 6);
    let expected: Vec<(u64, String)> = (1..=6)
        .map(|n| (id.0, format!("STEP-Fläche {n}")))
        .collect();
    assert_eq!(tags(v[0].solid), expected);
    assert!(d.visible_meshes().is_empty());
    let shape = step_shape_of(&d, id);
    assert_eq!(shape, StepShapeId(1));
    assert!(Arc::ptr_eq(
        d.step_body(id).unwrap(),
        d.step_shape(shape).unwrap()
    ));
    d.check_invariants().unwrap();

    // Mehrere Körper in einem Aufruf: je Körper eine Änderung, Formen fortlaufend.
    let edits = d.edit_count();
    let more = d
        .add_step_bodies(vec![
            body("K", imported(&sphere_solid(), "K")),
            body("Q2", imported(&box_solid(), "Q2")),
        ])
        .unwrap();
    assert_eq!(more.len(), 2);
    assert_eq!(d.edit_count(), edits + 2);
    assert_eq!(step_shape_of(&d, more[0]), StepShapeId(2));
    assert_eq!(step_shape_of(&d, more[1]), StepShapeId(3));
    for (feature, shape) in [(more[0], 2), (more[1], 3)] {
        assert!(Arc::ptr_eq(
            d.step_body(feature).unwrap(),
            d.step_shape(StepShapeId(shape)).unwrap()
        ));
    }
    assert_eq!(d.visible_bodies().len(), 3);
    assert_eq!(d.features()[1].id, more[0], "in der Reihenfolge am Marker");
    assert_eq!(d.marker(), 3);
    assert!(d.step_body(FeatureId(999)).is_none());
    // Ein leerer Aufruf ändert nichts.
    let edits = d.edit_count();
    assert_eq!(d.add_step_bodies(vec![]).unwrap(), Vec::<FeatureId>::new());
    assert_eq!(d.edit_count(), edits);
    d.check_invariants().unwrap();
}

#[test]
fn all_or_nothing() {
    let mut d = Document::new();
    let q = d
        .add_feature(
            "Quader",
            FeatureKind::Box {
                size: DVec3::splat(5.0),
                origin: DVec3::new(100.0, 0.0, 0.0),
            },
        )
        .unwrap();
    let before = d.snapshot();
    let (edits, model, next_id) = (d.edit_count(), d.model_edit_count(), before.next_id);
    let broken = Arc::new(StepShape::new(broken_text(), 1e-6).unwrap());
    let e = d
        .add_step_bodies(vec![
            body("Q", imported(&box_solid(), "Q")),
            body("Kaputt", broken),
        ])
        .unwrap_err();
    let msg = e.to_string();
    assert!(
        msg.starts_with("„Kaputt“: Die gespeicherte STEP-Form lässt sich nicht lesen: "),
        "{msg}"
    );
    assert!(d.snapshot().same_content(&before));
    assert_eq!(d.edit_count(), edits);
    assert_eq!(d.model_edit_count(), model);
    assert_eq!(d.snapshot().next_id, next_id);
    assert!(d.snapshot().step_shapes.is_empty());
    assert!(d.step_shape(StepShapeId(1)).is_none());
    assert!(d.step_shape(StepShapeId(2)).is_none());
    assert_eq!(d.marker(), 1);
    assert_eq!(status(&d, q), FeatureStatus::Ok);
    d.check_invariants().unwrap();
    // Ein leerer Name wird abgelehnt, ohne etwas zu ändern.
    assert!(matches!(
        d.add_step_bodies(vec![body(" ", imported(&box_solid(), "Q"))]),
        Err(FeatureError::InvalidInput(_))
    ));
    assert!(d.snapshot().same_content(&before));
    // Kennungen werden nach der Rücknahme wieder ab 1 vergeben.
    let ids = d
        .add_step_bodies(vec![body("Q", imported(&box_solid(), "Q"))])
        .unwrap();
    assert_eq!(ids, vec![FeatureId(next_id)]);
    assert_eq!(step_shape_of(&d, ids[0]), StepShapeId(1));
    d.check_invariants().unwrap();
}

/// Die Rücknahme nimmt auch ein Folgeblatt der Stückliste zurück, das die neuen Körper
/// ausgelöst haben: Zeichnungen, Zeichnungskennung und Änderungszahl wie vorher.
#[test]
fn rollback_restores_parts_sheets() {
    let a4_quer = Sheet {
        paper: Paper::A4,
        landscape: true,
    };
    let mut d = Document::new();
    // A4 quer: 26 Zeilen je Blatt; 26 Quader füllen Seite 1.
    for i in 1..=26 {
        d.add_feature(
            format!("Teil {i}"),
            FeatureKind::Box {
                size: DVec3::splat(5.0),
                origin: DVec3::new(20.0 * i as f64, 100.0, 0.0),
            },
        )
        .unwrap();
    }
    let stamp = Stamp {
        date: "2026-10-23".into(),
        user: "dennis".into(),
    };
    d.add_parts_sheets(a4_quer, &stamp).unwrap();
    assert_eq!(d.parts_sheets().len(), 1);
    let before = d.snapshot();
    let (edits, model) = (d.edit_count(), d.model_edit_count());
    let broken = Arc::new(StepShape::new(broken_text(), 1e-6).unwrap());
    let e = d
        .add_step_bodies(vec![
            body("Q", imported(&box_solid(), "Q")),
            body("Kaputt", broken),
        ])
        .unwrap_err();
    assert!(e.to_string().starts_with("„Kaputt“: "), "{e}");
    assert_eq!(d.parts_sheets().len(), 1);
    assert!(d.snapshot().same_content(&before));
    assert_eq!(d.snapshot().next_drawing, before.next_drawing);
    assert_eq!(d.edit_count(), edits);
    assert_eq!(d.model_edit_count(), model);
    d.check_invariants().unwrap();
    // Gelingt der Import, entsteht das Folgeblatt.
    d.add_step_bodies(vec![body("Q", imported(&box_solid(), "Q"))])
        .unwrap();
    assert_eq!(d.parts_sheets().len(), 2);
}

/// Ein Körper außerhalb des darstellbaren Bereichs wird wie jeder Körper abgelehnt; der Import
/// wird ganz zurückgenommen. Der Leser lässt solche Texte gar nicht erst zu („weiter als
/// 1 000 km“); geprüft wird hier die Bereichsprüfung des Dokuments selbst über eine Form, deren
/// mitgegebener Körper weit weg liegt.
#[test]
fn far_away_body_is_refused() {
    let k = kernel();
    let far = k
        .transform(
            &box_solid(),
            &planora_math::DMat4::from_translation(DVec3::new(MAX_COORD_MM - 5.0, 0.0, 0.0)),
        )
        .unwrap();
    let text = normalize(&box_solid(), "Fern").unwrap().text;
    let far = Arc::new(StepShape::with_solid(text, 1e-6, far));
    let mut d = Document::new();
    let before = d.snapshot();
    let e = d
        .add_step_bodies(vec![
            body("Q", imported(&box_solid(), "Q")),
            body("Fern", far),
        ])
        .unwrap_err();
    assert_eq!(
        e.to_string(),
        format!("„Fern“: {}", FeatureError::OutOfRange("Der Körper".into()))
    );
    assert!(d.snapshot().same_content(&before));
    assert!(d.step_shape(StepShapeId(1)).is_none());
}

#[test]
fn works_like_any_exact_body() {
    let mut d = Document::new();
    let q = d
        .add_step_bodies(vec![body("Q", imported(&box_solid(), "Q"))])
        .unwrap()[0];
    let z = d
        .add_feature(
            "Zylinder",
            FeatureKind::Cylinder {
                radius: 2.0,
                height: 40.0,
                origin: DVec3::new(5.0, 10.0, -5.0),
            },
        )
        .unwrap();
    let b = d
        .add_feature(
            "Bohrung",
            FeatureKind::Boolean {
                target: first(q),
                tool: first(z),
                op: BoolOp::Difference,
            },
        )
        .unwrap();
    assert_eq!(status(&d, b), FeatureStatus::Ok, "{:?}", status(&d, b));
    let cut = d.body(first(b)).unwrap().clone();
    let expected = 6000.0 - std::f64::consts::PI * 4.0 * 30.0;
    let v = volume(&cut);
    assert!(
        ((v - expected) / expected).abs() <= 1e-3,
        "{v} statt {expected}"
    );

    // Beim Quader aus dem Kernel liegen „STEP-Fläche 1“ und „STEP-Fläche 3“ einander gegenüber
    // (keine gemeinsame Kante); verrundet wird die Kante zwischen 1 und 2.
    let edge = edge_ref(&cut, "STEP-Fläche 1", "STEP-Fläche 2");
    let r = d
        .add_feature(
            "Rundung",
            FeatureKind::Verrunden {
                body: first(b),
                edges: vec![edge],
                art: BlendArt::Rundung { radius: 1.0 },
                tangentenkette: false,
            },
        )
        .unwrap();
    assert_eq!(status(&d, r), FeatureStatus::Ok, "{:?}", status(&d, r));
    let rounded = d.body(first(r)).unwrap().clone();
    assert!(rounded.faces.len() > cut.faces.len());

    let mut sk = Sketch::new(planora_math::Frame::WORLD, "S", Tolerance::default());
    sk.add_rectangle(DVec2::ZERO, DVec2::new(2.0, 3.0)).unwrap();
    let s = d
        .add_feature(
            "Skizze",
            FeatureKind::Sketch(SketchFeature {
                sketch: sk,
                plane: SketchPlane::OnFace(face_ref(&rounded, "STEP-Fläche 2")),
                copies: vec![],
                formulas: Default::default(),
            }),
        )
        .unwrap();
    assert_eq!(status(&d, s), FeatureStatus::Ok, "{:?}", status(&d, s));
    let e = d
        .add_feature(
            "Extrusion",
            FeatureKind::Extrude {
                sketch: s,
                profile: 0,
                height: 5.0,
            },
        )
        .unwrap();
    assert_eq!(status(&d, e), FeatureStatus::Ok, "{:?}", status(&d, e));
    let m = d
        .add_feature(
            "Verschieben",
            FeatureKind::Move {
                body: first(r),
                delta: DVec3::X,
            },
        )
        .unwrap();
    assert_eq!(status(&d, m), FeatureStatus::Ok);
    let (lo0, hi0) = bounds(&rounded);
    let (lo1, hi1) = bounds(d.body(first(m)).unwrap());
    assert!((lo1 - lo0 - DVec3::X).abs().max_element() < 1e-9);
    assert!((hi1 - hi0 - DVec3::X).abs().max_element() < 1e-9);
    d.check_invariants().unwrap();
}

#[test]
fn restore_reuses_the_built_shape() {
    let mut d = Document::new();
    let empty = d.snapshot();
    let shape = loaded(&box_solid());
    let builds = step_builds();
    let q = d
        .add_step_bodies(vec![body("Q", Arc::clone(&shape))])
        .unwrap()[0];
    assert_eq!(step_builds(), builds + 1, "einmal gebaut beim Rechnen");
    let s1 = d.snapshot();
    assert_eq!(s1.step_shapes.len(), 1);
    let tags1 = tags(d.body(first(q)).unwrap());
    d.add_feature(
        "Verschieben",
        FeatureKind::Move {
            body: first(q),
            delta: DVec3::X,
        },
    )
    .unwrap();
    assert_eq!(d.restore(s1.clone()).unwrap(), RestoreKind::Full);
    assert_eq!(step_builds(), builds + 1, "das gebaute Arc baut nicht neu");
    assert!(Arc::ptr_eq(d.step_body(q).unwrap(), &shape));
    assert_eq!(d.features().len(), 1);

    // Rückgängig des Imports: keine STEP-Körper, die Form ist verworfen.
    assert_eq!(d.restore(empty.clone()).unwrap(), RestoreKind::Full);
    assert!(d.features().is_empty());
    assert!(d.step_shape(StepShapeId(1)).is_none());
    assert!(d.snapshot().step_shapes.is_empty());
    // Ein neuer Import danach bekommt eine neue Kennung (Kennungen nie wiederverwendet).
    let other = d
        .add_step_bodies(vec![body("Neu", imported(&box_solid(), "Neu"))])
        .unwrap();
    assert_eq!(step_shape_of(&d, other[0]), StepShapeId(2));
    // Wiederholen: gleiche Marken, wieder ohne Bauen.
    assert_eq!(d.restore(s1).unwrap(), RestoreKind::Full);
    assert_eq!(status(&d, q), FeatureStatus::Ok);
    assert_eq!(tags(d.body(first(q)).unwrap()), tags1);
    assert_eq!(step_builds(), builds + 1);
    // Kennungen laufen weiter (nie wiederverwendet nach dem Wiederholen).
    let more = d
        .add_step_bodies(vec![body("Q2", imported(&box_solid(), "Q2"))])
        .unwrap();
    assert_eq!(step_shape_of(&d, more[0]), StepShapeId(3));
    // Löschen verwirft die Form.
    d.delete(more[0]).unwrap();
    assert!(d.step_shape(StepShapeId(3)).is_none());
    assert!(d.step_shape(StepShapeId(1)).is_some());
    d.check_invariants().unwrap();
}

#[test]
fn snapshot_rules() {
    let mut d = Document::new();
    let q = d
        .add_step_bodies(vec![body("Q", imported(&box_solid(), "Q"))])
        .unwrap()[0];
    let s = d.snapshot();
    assert!(s.same_model(&s.clone()));
    let id = step_shape_of(&d, q);
    let same_text = Arc::new(StepShape::new(s.step_shapes[&id].text().to_string(), 1e-6).unwrap());
    let mut other = s.clone();
    other.step_shapes.insert(id, same_text);
    assert!(!s.same_model(&other), "gleicher Text in anderem Arc");
    assert!(!s.same_content(&other));

    // Unlesbarer Text: das Dokument lädt, nur der STEP-Körper hat den Fehlerstatus (P14).
    let mut bad = s.clone();
    bad.step_shapes
        .insert(id, Arc::new(StepShape::new(broken_text(), 1e-6).unwrap()));
    let loaded = Document::from_snapshot(bad, 0.05).unwrap();
    match status(&loaded, q) {
        FeatureStatus::Error(m) => assert!(
            m.starts_with("Die gespeicherte STEP-Form lässt sich nicht lesen: "),
            "{m}"
        ),
        other => panic!("Fehlerstatus erwartet: {other:?}"),
    }
    assert!(loaded.visible_bodies().is_empty());
    loaded.check_invariants().unwrap();

    // Fehlende Form: das Bild ist kaputt.
    let mut missing = s.clone();
    missing.step_shapes.clear();
    assert_eq!(
        Document::from_snapshot(missing, 0.05).unwrap_err(),
        FeatureError::StepShapeMissing(id.0)
    );
    assert_eq!(
        FeatureError::StepShapeMissing(7).to_string(),
        "Die STEP-Form 7 fehlt im Dokument"
    );
    // Kennung 0.
    let mut zero = s.clone();
    let shape = zero.step_shapes.remove(&id).unwrap();
    zero.step_shapes.insert(StepShapeId(0), shape);
    assert!(matches!(
        Document::from_snapshot(zero, 0.05),
        Err(FeatureError::InvalidInput(_))
    ));
    // Unbenutzte Formen stehen nicht im Bild; nach dem Laden laufen die Kennungen weiter.
    let mut extra = s.clone();
    extra
        .step_shapes
        .insert(StepShapeId(5), imported(&box_solid(), "X"));
    let x = Document::from_snapshot(extra, 0.05).unwrap();
    assert_eq!(
        x.snapshot().step_shapes.keys().copied().collect::<Vec<_>>(),
        vec![id]
    );
    assert!(x.snapshot().same_model(&s));
    let mut e = Document::from_snapshot(s, 0.05).unwrap();
    assert_eq!(e.edit_count(), 0);
    assert_eq!(status(&e, q), FeatureStatus::Ok);
    let more = e
        .add_step_bodies(vec![body("Q2", imported(&box_solid(), "Q2"))])
        .unwrap();
    assert_eq!(step_shape_of(&e, more[0]), StepShapeId(2));
    // Ein Feature, das auf eine fehlende Form zeigt, lässt sich nicht anlegen.
    assert_eq!(
        e.add_feature(
            "X",
            FeatureKind::StepBody {
                shape: StepShapeId(99)
            }
        )
        .unwrap_err(),
        FeatureError::StepShapeMissing(99)
    );
    // Netz-Sperre trifft STEP-Körper nicht; der Körper lässt sich zudem nicht „bearbeiten“.
    assert!(
        e.update(q, |k| {
            if let FeatureKind::StepBody { shape } = k {
                *shape = StepShapeId(99);
            }
        })
        .is_err()
    );
    e.check_invariants().unwrap();
}

#[test]
fn coarse_flag() {
    // Formen werden zwischen Fäden geteilt (Hintergrundarbeit der App).
    fn send_sync<T: Send + Sync>() {}
    send_sync::<StepShape>();
    send_sync::<NewStepBody>();
    let text = normalize(&box_solid(), "Q").unwrap().text;
    let coarse = StepShape::new(text.clone(), 1e-3).unwrap();
    assert!(coarse.is_coarse());
    assert_eq!(coarse.tolerance_mm(), 1e-3);
    assert_eq!(coarse.text(), text);
    let fine = StepShape::new(text.clone(), 1e-6).unwrap();
    assert!(!fine.is_coarse());
    assert!(!StepShape::new(text.clone(), 1e-5).unwrap().is_coarse());
    assert!(StepShape::new(text.clone(), 1.1e-5).unwrap().is_coarse());
    for bad in [2e-3, 9e-7, 0.0, -1e-4, f64::NAN, f64::INFINITY] {
        assert!(StepShape::new(text.clone(), bad).is_err(), "{bad}");
    }
    // Kopfprüfung (ohne zu bauen).
    let builds = step_builds();
    assert!(StepShape::new("hallo".into(), 1e-6).is_err());
    let unread = StepShape::new(broken_text(), 1e-6).unwrap();
    assert_eq!(step_builds(), builds, "new baut nicht");
    let dbg = format!("{unread:?}");
    assert!(dbg.contains("StepShape"), "{dbg}");
    assert!(!dbg.contains("ISO-10303"), "Debug ohne den Text: {dbg}");
    // Bauen genau einmal; ein Fehler bleibt.
    assert!(unread.solid().is_err());
    assert!(unread.solid().is_err());
    assert_eq!(step_builds(), builds + 1);
    let built = StepShape::new(text, 1e-6).unwrap();
    assert_eq!(built.solid().unwrap().faces.len(), 6);
    assert_eq!(built.solid().unwrap().faces.len(), 6);
    assert_eq!(step_builds(), builds + 2);
}

#[test]
fn drawings_and_parts_list_see_it() {
    let mut d = Document::new();
    let q = d
        .add_step_bodies(vec![body("Teil aus STEP", imported(&box_solid(), "Q"))])
        .unwrap()[0];
    let id = d.add_drawing("Blatt", A4, Projection::First).unwrap();
    let top = d
        .add_view(
            id,
            ViewKind::Top,
            S11,
            Placement::Free { x: 100.0, y: 200.0 },
            None,
        )
        .unwrap();
    d.add_view(
        id,
        ViewKind::Front,
        S11,
        Placement::Free { x: 100.0, y: 80.0 },
        None,
    )
    .unwrap();
    d.update_drawing(id, |dr| {
        dr.add_section(top, ViewKind::Front, 10.0, 40.0).unwrap();
    })
    .unwrap();
    assert_eq!(d.drawing(id).unwrap().views.len(), 3);
    let visible = d.visible_bodies();
    assert_eq!(visible.len(), 1);
    let bodies: Vec<DrawBody> = visible
        .iter()
        .map(|v| DrawBody {
            name: v.name.clone(),
            solid: v.solid,
        })
        .collect();
    let view = planora_drawing::compute_view(&bodies, ViewKind::Front, &Progress::new()).unwrap();
    assert!(!view.pieces.is_empty(), "Kanten in der Ansicht");
    let plane = SectionPlane {
        kind: ViewKind::Front,
        offset: 10.0,
    };
    let sec = planora_drawing::compute_section(
        &bodies,
        &plane,
        planora_drawing::compute_view,
        &Progress::new(),
    )
    .unwrap();
    assert!(!sec.lines.pieces.is_empty());
    let area: f64 = sec
        .data
        .regions
        .iter()
        .flat_map(|r| r.loops.iter())
        .map(|l| polygon_area(&loop_polygon(l, &sec.lines.basis, 1e-3)))
        .sum::<f64>()
        .abs();
    assert!(area > 0.0, "Schnittfläche {area}");
    assert!((area - 300.0).abs() < 1.0, "Schnittfläche {area}");

    let rows = d.parts_rows();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].name, "Teil aus STEP");
    assert_eq!(rows[0].keys[0].feature, q.0);

    let step: Vec<StepBody> = visible
        .iter()
        .map(|v| StepBody {
            name: v.name.clone(),
            solid: v.solid,
        })
        .collect();
    let mut out = Vec::new();
    let report = write_step(
        &step,
        &StepOptions::new("x.step"),
        &mut out,
        &Progress::new(),
    )
    .unwrap();
    assert_eq!(report.bodies, 1);
    assert_eq!(report.faces, 6);
    assert!(report.rejected.is_empty());
}

/// Summengrenze des Dateiformats schon beim Import (Aufgabe 12, Fix-Runde 1): mehr STEP-Text,
/// als sich speichern ließe, wird ganz abgelehnt; dieselbe Form zählt einmal.
#[test]
fn import_respects_the_step_text_sum_limit() {
    assert_eq!(MAX_STEP_TEXT_SUM_BYTES, 200 * 1024 * 1024);
    assert_eq!(step_text_sum_limit(), MAX_STEP_TEXT_SUM_BYTES);
    assert_eq!(step_text_megabytes(MAX_STEP_TEXT_SUM_BYTES), "200");
    assert_eq!(step_text_megabytes(MAX_STEP_TEXT_SUM_BYTES + 1), "200,1");
    assert_eq!(step_text_megabytes(1024 * 1024 / 2), "0,5");
    assert_eq!(step_text_megabytes(0), "0");

    let a = loaded(&box_solid());
    let b = loaded(&sphere_solid());
    let (la, lb) = (a.text().len(), b.text().len());
    assert_eq!(
        step_text_sum([&a, &a, &b]),
        la + lb,
        "dasselbe Arc zählt einmal"
    );
    {
        let _limit = limit_step_text_sum_for_tests(la + lb - 1);
        assert_eq!(step_text_sum_limit(), la + lb - 1);
        let mut d = Document::new();
        d.add_step_bodies(vec![body("A", Arc::clone(&a))]).unwrap();
        let (before, edits) = (d.snapshot(), d.edit_count());
        let builds = step_builds();
        let e = d
            .add_step_bodies(vec![body("B", Arc::clone(&b))])
            .unwrap_err();
        assert!(matches!(e, FeatureError::StepImport(_)), "{e:?}");
        assert_eq!(
            e.to_string(),
            format!(
                "Die STEP-Formen im Projekt sind zusammen zu groß ({} MB, erlaubt {} MB). \
                 Bitte weniger oder kleinere Körper importieren.",
                step_text_megabytes(la + lb),
                step_text_megabytes(la + lb - 1)
            )
        );
        assert_eq!(step_builds(), builds, "abgelehnt, bevor gebaut wird");
        assert!(d.snapshot().same_content(&before));
        assert_eq!(d.edit_count(), edits);
        assert_eq!(d.snapshot().step_shapes.len(), 1);
        // Dieselbe Form noch einmal: die Summe wächst nicht.
        d.add_step_bodies(vec![body("A2", Arc::clone(&a))]).unwrap();
        d.check_invariants().unwrap();
    }
    assert_eq!(
        step_text_sum_limit(),
        MAX_STEP_TEXT_SUM_BYTES,
        "Wächter stellt zurück"
    );
}

/// Schlussprüfung M2: Gründe aus dem gespeicherten Text nennen keine Zeilen unseres eigenen
/// Texts (dort sucht niemand), wie schon beim Import.
#[test]
fn damaged_saved_text_names_no_line_of_the_internal_text() {
    let text = normalize(&box_solid(), "Q").unwrap().text;
    let from = "DIRECTION('',(0.,0.,1.))";
    assert!(text.contains(from), "Richtung im Text");
    let broken = text.replacen(from, "DIRECTION('',(0.,0.,0.))", 1);
    let shape = StepShape::new(broken, 1e-6).unwrap();
    let e = shape.solid().expect_err("kaputte Richtung");
    assert!(!e.contains("Zeile"), "{e}");
    assert!(!e.is_empty());
}

/// Schlussprüfung M1: Eine Form ohne Genauigkeit der Datei ist grob und sagt das.
#[test]
fn shapes_without_stated_accuracy_are_coarse() {
    let n = normalize(&box_solid(), "Q").unwrap();
    let known = StepShape::with_solid(n.text.clone(), 1e-6, n.solid.clone());
    assert!(known.tolerance_known() && !known.is_coarse());
    let unknown = StepShape::new(n.text, 1e-3).unwrap().accuracy_unknown();
    assert!(!unknown.tolerance_known());
    assert!(unknown.is_coarse());
    assert_eq!(unknown.tolerance_mm(), 1e-3);
}
