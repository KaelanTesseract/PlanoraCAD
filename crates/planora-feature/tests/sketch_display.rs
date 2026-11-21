//! Anzeigeschalter je Skizze (Skizzen 12, Task 1): Vorgabe, Zähler, Snapshot und Rückgängig.

use planora_feature::*;
use planora_math::{Frame, Tolerance};
use planora_sketch::Sketch;

fn sketch(d: &mut Document, n: usize) -> FeatureId {
    let name = format!("Skizze{n}");
    d.add_feature(
        &name,
        FeatureKind::Sketch(SketchFeature {
            sketch: Sketch::new(Frame::WORLD, &name, Tolerance::default()),
            plane: SketchPlane::Standard(StandardPlane::XY),
            copies: vec![],
            formulas: Default::default(),
            text_formulas: TextFormulas::new(),
        }),
    )
    .unwrap()
}

fn shown_slice() -> SketchDisplay {
    SketchDisplay {
        slice: true,
        ..SketchDisplay::default()
    }
}

#[test]
fn default_display_values() {
    let d = SketchDisplay::default();
    assert!(!d.slice);
    assert!(!d.dimensions_finished);
    assert!(d.profiles && d.points && d.dimensions && d.constraints);
    assert!(d.construction && d.projected);
    assert!(d.is_default());
    assert!(!shown_slice().is_default());
    let mut doc = Document::new();
    let id = sketch(&mut doc, 1);
    assert_eq!(doc.sketch_display(id), SketchDisplay::default());
}

#[test]
fn set_display_counts_as_view_edit() {
    let mut doc = Document::new();
    let id = sketch(&mut doc, 1);
    let (e, m) = (doc.edit_count(), doc.model_edit_count());
    doc.set_sketch_display(id, shown_slice());
    assert_eq!(doc.sketch_display(id), shown_slice());
    assert_eq!(doc.edit_count(), e + 1);
    assert_eq!(doc.model_edit_count(), m);
    // Gleicher Wert: keine Änderung.
    doc.set_sketch_display(id, shown_slice());
    assert_eq!(doc.edit_count(), e + 1);
}

#[test]
fn setting_default_removes_entry() {
    let mut doc = Document::new();
    let id = sketch(&mut doc, 1);
    doc.set_sketch_display(id, shown_slice());
    assert_eq!(doc.snapshot().sketch_display.len(), 1);
    doc.set_sketch_display(id, SketchDisplay::default());
    assert!(doc.snapshot().sketch_display.is_empty());
}

#[test]
fn display_on_non_sketch_is_ignored() {
    let mut doc = Document::new();
    let p = doc
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
    let e = doc.edit_count();
    doc.set_sketch_display(p, shown_slice());
    doc.set_sketch_display(FeatureId(999), shown_slice());
    assert_eq!(doc.edit_count(), e);
    assert!(doc.snapshot().sketch_display.is_empty());
}

#[test]
fn display_entry_follows_sketch_delete_and_undo() {
    let mut doc = Document::new();
    let a = sketch(&mut doc, 1);
    let b = sketch(&mut doc, 2);
    doc.set_sketch_display(a, shown_slice());
    doc.set_sketch_display(b, shown_slice());
    let before = doc.snapshot();
    doc.delete(a).unwrap();
    let after = doc.snapshot();
    assert_eq!(after.sketch_display.len(), 1);
    assert!(after.sketch_display.contains_key(&b));
    assert_eq!(doc.sketch_display(a), SketchDisplay::default());
    doc.restore(before).unwrap();
    assert_eq!(doc.sketch_display(a), shown_slice());
    assert_eq!(doc.sketch_display(b), shown_slice());
}
