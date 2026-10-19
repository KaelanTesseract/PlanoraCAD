//! Reiner Verlauf für Rückgängig und Wiederholen (Teilprojekt „Rückgängig und Wiederholen“,
//! Aufgabe 1): Schritte, Nummern, Grenzen, Gespeichert-Marke, Gesten-Zähler, Größenschätzung.

use planora_app::history::*;
use planora_drawing::dimension::{DimId, DimKind, DimSpec, Tolerance as DimTol};
use planora_drawing::model::Balloon;
use planora_drawing::{
    BalloonId, BodyKey, Paper, Placement, Projection, RowOverride, Scale, Sheet, ViewId, ViewKind,
};
use planora_exchange::ExchangeMesh;
use planora_feature::{
    Document, EdgeCopy, EdgeRef, FaceRef, FeatureKind, MeshId, SketchFeature, SketchPlane,
    Snapshot, SnapshotFeature, StandardPlane,
};
use planora_math::DVec3;
use planora_math::{Frame, Tolerance};
use planora_sketch::model::Sketch;
use std::sync::Arc;

const A4_QUER: Sheet = Sheet {
    paper: Paper::A4,
    landscape: true,
};

/// Verlauf 0 → 1 → … → n mit je 10 Bytes.
fn chain(n: u32) -> History<u32> {
    let mut h = History::new(0u32, 10);
    for i in 1..=n {
        h.record(i, 10, "X");
    }
    h
}

/// Geht mit Rückgängig bis zum Anfang und gibt den ältesten erreichbaren Zustand zurück.
fn oldest(h: &mut History<u32>) -> u32 {
    while h.commit_undo().is_some() {}
    *h.current()
}

#[test]
fn record_undo_redo_roundtrip() {
    let mut h = History::new(0u32, 10);
    assert_eq!(h.current_id(), 0);
    h.record(1, 10, "A");
    h.record(2, 10, "B");
    assert_eq!(h.current_id(), 2);
    assert_eq!(h.undo_label(), Some("B"));
    assert_eq!(h.peek_undo(), Some(&1));
    assert_eq!(h.commit_undo(), Some("B"));
    assert_eq!(*h.current(), 1);
    assert_eq!(h.current_id(), 1);
    assert_eq!(h.redo_label(), Some("B"));
    assert_eq!(h.peek_redo(), Some(&2));
    assert_eq!(h.undo_label(), Some("A"));
    assert_eq!(h.commit_undo(), Some("A"));
    assert_eq!(*h.current(), 0);
    assert_eq!(h.current_id(), 0);
    assert_eq!(h.redo_label(), Some("A"));
    assert_eq!(h.commit_redo(), Some("A"));
    assert_eq!(*h.current(), 1);
    assert_eq!(h.current_id(), 1);
    assert_eq!(h.commit_redo(), Some("B"));
    assert_eq!(*h.current(), 2);
    assert_eq!(h.current_id(), 2);
    assert_eq!(h.undo_len(), 2);
    assert_eq!(h.redo_len(), 0);
}

#[test]
fn empty_history() {
    let mut h = History::new(7u32, 10);
    assert_eq!(h.undo_label(), None);
    assert_eq!(h.redo_label(), None);
    assert_eq!(h.peek_undo(), None);
    assert_eq!(h.peek_redo(), None);
    assert_eq!(h.commit_undo(), None);
    assert_eq!(h.commit_redo(), None);
    assert_eq!(*h.current(), 7);
    assert_eq!(h.current_id(), 0);
    h.discard_undo();
    h.discard_redo();
    assert_eq!(*h.current(), 7);
    assert_eq!(h.undo_len(), 0);
    assert_eq!(h.redo_len(), 0);
    assert_eq!(h.bytes(), 10);
}

#[test]
fn new_action_clears_redo() {
    let mut h = chain(2);
    h.commit_undo();
    assert_eq!(h.redo_len(), 1);
    h.record(3, 10, "C");
    assert_eq!(h.redo_len(), 0);
    assert_eq!(h.redo_label(), None);
    assert_eq!(h.current_id(), 3, "neue Nummer, nicht die alte 2");
    assert_eq!(h.commit_undo(), Some("C"));
    assert_eq!(*h.current(), 1);
    assert_eq!(h.current_id(), 1);
}

#[test]
fn limit_100_steps() {
    assert_eq!(MAX_STEPS, 100);
    let mut h = History::new(0u32, 10);
    for i in 1..=150 {
        h.record(i, 10, "X");
    }
    assert_eq!(h.undo_len(), 100);
    assert_eq!(*h.current(), 150);
    assert_eq!(oldest(&mut h), 50);
}

#[test]
fn limit_bytes() {
    assert_eq!(MAX_BYTES, 209_715_200);
    let mut h = History::with_limits(0u32, 30, 1000, 100);
    for i in 1..=10 {
        h.record(i, 30, "X");
        assert!(h.bytes() <= 100, "Bytes {} nach Schritt {i}", h.bytes());
    }
    assert_eq!(h.undo_len(), 2);
    assert_eq!(h.bytes(), 90);
    assert_eq!(oldest(&mut h), 8, "der älteste fällt zuerst");

    let mut g = History::with_limits(0u32, 30, 1000, 100);
    g.record(1, 30, "X");
    g.record(99, 500, "Groß");
    assert_eq!(g.undo_len(), 0);
    assert_eq!(*g.current(), 99);
    assert_eq!(g.commit_undo(), None);
}

#[test]
fn saved_marker() {
    let mut h = History::new(0u32, 10);
    assert!(h.is_saved());
    h.record(1, 10, "A");
    assert!(!h.is_saved());
    h.mark_saved();
    assert!(h.is_saved());
    h.commit_undo();
    assert!(!h.is_saved());
    h.commit_redo();
    assert!(h.is_saved());
    h.commit_undo();
    h.record(5, 10, "Neu");
    assert!(!h.is_saved());
    h.commit_undo();
    h.commit_redo();
    assert!(!h.is_saved(), "nie wieder gespeichert ohne mark_saved");
    h.mark_saved();
    assert!(h.is_saved());
    h.forget_saved();
    assert!(!h.is_saved());
    let mut z = History::new(0u32, 10);
    z.forget_saved();
    assert!(!z.is_saved(), "auch bei Nummer 0");
}

#[test]
fn tracker_coalesces_gestures() {
    let mut t = Tracker::new(5);
    assert!(!t.is_pending(5));
    assert!(!t.observe(5, false));
    assert!(!t.observe(6, true));
    assert!(t.is_pending(6));
    assert!(!t.observe(9, true));
    assert!(t.is_pending(9));
    assert!(t.observe(9, false));
    assert!(!t.is_pending(9));
    assert!(!t.observe(9, false));
    assert!(t.observe(4, false), "nicht monoton");
    assert!(t.is_pending(5), "ungesehene Änderung");
    t.observe(30, true);
    t.reset(20);
    assert!(!t.is_pending(20));
    assert!(!t.observe(20, false));
}

#[test]
fn discard_keeps_rest() {
    let mut h = chain(3);
    h.discard_undo();
    assert_eq!(*h.current(), 3);
    assert_eq!(h.undo_len(), 2);
    assert_eq!(h.peek_undo(), Some(&1));
    assert_eq!(h.redo_len(), 0);

    let mut r = chain(3);
    r.commit_undo();
    r.commit_undo();
    assert_eq!(r.redo_len(), 2);
    r.discard_redo();
    assert_eq!(r.redo_len(), 1);
    assert_eq!(*r.current(), 1);
    assert_eq!(r.peek_redo(), Some(&3));
    assert_eq!(r.undo_len(), 1);
}

fn mesh_with_triangles(n: usize) -> ExchangeMesh {
    ExchangeMesh {
        positions: vec![[0.0, 0.0, 0.0]; 3],
        indices: vec![0u32; n * 3],
    }
}

#[test]
fn snapshot_bytes_counts_without_meshes() {
    let mut doc = Document::new();
    let empty = doc.snapshot();
    let b0 = snapshot_bytes(&empty);
    assert!(b0 > 0);

    doc.add_feature(
        "Quader 1",
        FeatureKind::Box {
            size: DVec3::new(10.0, 10.0, 10.0),
            origin: DVec3::ZERO,
        },
    )
    .unwrap();
    let with_box = doc.snapshot();
    let b1 = snapshot_bytes(&with_box);
    assert!(b1 > b0);

    // Ein riesiges Netz im Snapshot zählt nicht.
    let mut with_mesh = empty.clone();
    with_mesh
        .meshes
        .insert(MeshId(1), Arc::new(mesh_with_triangles(1_000_000)));
    assert_eq!(snapshot_bytes(&with_mesh), b0);

    // Zeichnung: mehr Ansichten, mehr Bytes.
    let d = doc
        .add_drawing("Blatt 1", A4_QUER, Projection::First)
        .unwrap();
    doc.add_view(
        d,
        ViewKind::Front,
        Scale { num: 1, den: 1 },
        Placement::Free { x: 50.0, y: 50.0 },
        None,
    )
    .unwrap();
    let one: Snapshot = doc.snapshot();
    for x in [100.0, 150.0] {
        doc.add_view(
            d,
            ViewKind::Top,
            Scale { num: 1, den: 1 },
            Placement::Free { x, y: 120.0 },
            None,
        )
        .unwrap();
    }
    let three = doc.snapshot();
    assert!(snapshot_bytes(&three) > snapshot_bytes(&one));
    assert!(snapshot_bytes(&one) > b1);
}

#[test]
fn limit_bytes_boundary_and_redo_counted() {
    // Genau an der Grenze bleibt alles erhalten (≤, nicht <).
    let mut h = History::with_limits(0u32, 50, 1000, 100);
    h.record(1, 50, "X");
    assert_eq!(h.bytes(), 100);
    assert_eq!(h.undo_len(), 1);
    // Die Wiederholen-Liste zählt mit.
    h.commit_undo();
    assert_eq!(h.undo_len(), 0);
    assert_eq!(h.redo_len(), 1);
    assert_eq!(h.bytes(), 100);
}

#[test]
fn snapshot_bytes_counts_names() {
    let make = |name: &str| {
        let mut doc = Document::new();
        doc.add_feature(
            name,
            FeatureKind::Box {
                size: DVec3::new(1.0, 1.0, 1.0),
                origin: DVec3::ZERO,
            },
        )
        .unwrap();
        snapshot_bytes(&doc.snapshot())
    };
    assert_eq!(make("Langer Name") - make("N"), "Langer Name".len() - 1);
}

#[test]
fn unequal_sizes_keep_bytes_constant() {
    let mut h = History::new(0u32, 5);
    h.record(1, 7, "A");
    h.record(2, 9, "B");
    assert_eq!(h.bytes(), 5 + 7 + 9);
    h.commit_undo();
    assert_eq!(h.bytes(), 21);
    h.commit_undo();
    assert_eq!(h.bytes(), 21);
    h.commit_redo();
    assert_eq!(h.bytes(), 21);
    h.commit_redo();
    assert_eq!(h.bytes(), 21);
    // Über die Grenze: nur die Größen der wirklich entfernten Einträge fallen weg.
    let mut g = History::with_limits(0u32, 5, 1000, 20);
    g.record(1, 7, "A");
    g.record(2, 9, "B");
    assert_eq!(g.bytes(), 16, "5 fällt heraus, 7 + 9 bleiben");
    assert_eq!(g.undo_len(), 1);
}

#[test]
fn redo_order_after_three_undos() {
    let mut h = chain(3);
    for _ in 0..3 {
        h.commit_undo();
    }
    assert_eq!(h.redo_len(), 3);
    assert_eq!(h.peek_redo(), Some(&1));
    assert_eq!(h.commit_redo(), Some("X"));
    assert_eq!(*h.current(), 1);
    assert_eq!(h.peek_redo(), Some(&2));
    h.commit_redo();
    assert_eq!(h.peek_redo(), Some(&3));
    assert_eq!(h.redo_label(), Some("X"));
    h.commit_redo();
    assert_eq!(h.redo_label(), None);
    // Beschriftungen bleiben je Schritt erhalten.
    let mut l = History::new(0u32, 1);
    l.record(1, 1, "A");
    l.record(2, 1, "B");
    l.record(3, 1, "C");
    for _ in 0..3 {
        l.commit_undo();
    }
    assert_eq!(l.redo_label(), Some("A"));
    l.commit_redo();
    assert_eq!(l.redo_label(), Some("B"));
    assert_eq!(l.undo_label(), Some("A"));
}

#[test]
fn evicted_saved_stays_dirty() {
    let mut h = History::with_limits(0u32, 10, 2, MAX_BYTES);
    assert!(h.is_saved());
    for i in 1..=3 {
        h.record(i, 10, "X");
    }
    assert_eq!(oldest(&mut h), 1, "Stand 0 ist verdrängt");
    assert!(!h.is_saved());
    while h.commit_redo().is_some() {
        assert!(!h.is_saved());
    }
    h.mark_saved();
    assert!(h.is_saved());
}

fn base_snapshot() -> Snapshot {
    Document::new().snapshot()
}

fn sketch_feature() -> SketchFeature {
    SketchFeature {
        sketch: Sketch::new(Frame::WORLD, "Skizze", Tolerance::default()),
        plane: SketchPlane::Standard(StandardPlane::XY),
        copies: vec![],
    }
}

fn with_sketch(sf: SketchFeature) -> Snapshot {
    let mut s = base_snapshot();
    s.features.push(SnapshotFeature {
        id: planora_feature::FeatureId(1),
        name: "S".into(),
        suppressed: false,
        kind: FeatureKind::Sketch(sf),
    });
    s
}

#[test]
fn snapshot_bytes_grows_with_sketch_content() {
    let base = snapshot_bytes(&with_sketch(sketch_feature()));

    let mut sf = sketch_feature();
    let a = sf.sketch.add_point(0.0, 0.0).unwrap();
    let with_point = snapshot_bytes(&with_sketch(sf.clone()));
    // Punkt: mindestens 64 für den Punkt plus 16 für zwei Variablen.
    assert_eq!(with_point - base, 64 + 2 * 8);

    let b = sf.sketch.add_point(10.0, 0.0).unwrap();
    let before_line = snapshot_bytes(&with_sketch(sf.clone()));
    sf.sketch.add_line(a, b).unwrap();
    let with_line = snapshot_bytes(&with_sketch(sf.clone()));
    assert_eq!(with_line - before_line, 64, "Kurve zählt 64");

    sf.sketch
        .add_constraint(planora_sketch::constraint::ConstraintKind::Fixed(
            a, 0.0, 0.0,
        ))
        .unwrap();
    let with_constraint = snapshot_bytes(&with_sketch(sf.clone()));
    assert!(with_constraint >= with_line + 64, "Abhängigkeit zählt 64");

    let curve = sf.sketch.curves().next().unwrap().0;
    sf.copies.push(EdgeCopy {
        curve,
        edge: EdgeRef {
            a: FaceRef {
                origin: 1,
                role: String::new(),
                hint: DVec3::ZERO,
            },
            b: FaceRef {
                origin: 1,
                role: String::new(),
                hint: DVec3::ZERO,
            },
            hint: DVec3::ZERO,
        },
    });
    let with_copy = snapshot_bytes(&with_sketch(sf));
    assert_eq!(with_copy - with_constraint, 64, "Kopie zählt 64");
}

#[test]
fn snapshot_bytes_counts_drawing_and_parts_content() {
    let mut doc = Document::new();
    let d = doc.add_drawing("A", A4_QUER, Projection::First).unwrap();
    let base_snap = doc.snapshot();
    let base = snapshot_bytes(&base_snap);

    // Zeichnungsname
    let mut s = base_snap.clone();
    s.drawings[0].name = "AAAAAAAAAA".into();
    assert_eq!(snapshot_bytes(&s), base + 9);

    // Grundwert 1024 für Blattangaben: ein zweites, gleich benanntes Blatt kostet genau
    // size_of::<Drawing>() + Name + 1024.
    let mut s = base_snap.clone();
    let copy = s.drawings[0].clone();
    s.drawings.push(copy);
    assert_eq!(
        snapshot_bytes(&s),
        base + std::mem::size_of::<planora_drawing::Drawing>() + 1 + 1024
    );

    // Maße je 256
    let mut s = base_snap.clone();
    s.drawings[0].dimensions.push(DimSpec {
        id: DimId(1),
        view: ViewId(1),
        kind: DimKind::Horizontal,
        refs: vec![],
        offset: 0.0,
        angle: 0.0,
        text_shift: (0.0, 0.0),
        tolerance: DimTol::None,
        decimals: 1,
        text_override: None,
        chain: None,
        suggested: false,
    });
    assert_eq!(snapshot_bytes(&s), base + 256);

    // Positionsnummern je size_of::<Balloon>()
    let mut s = base_snap.clone();
    s.drawings[0].balloons.push(Balloon {
        id: BalloonId(1),
        view: ViewId(1),
        body: BodyKey {
            feature: 1,
            index: 0,
        },
        anchor: (0.0, 0.0),
        offset: (0.0, 0.0),
    });
    assert_eq!(snapshot_bytes(&s), base + std::mem::size_of::<Balloon>());

    // Stücklistentexte je 256
    let mut s = base_snap.clone();
    s.parts_overrides.insert(
        BodyKey {
            feature: 1,
            index: 0,
        },
        RowOverride::default(),
    );
    assert_eq!(snapshot_bytes(&s), base + 256);
    let _ = d;
}
