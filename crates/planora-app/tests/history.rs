//! Reiner Verlauf für Rückgängig und Wiederholen (Teilprojekt „Rückgängig und Wiederholen“,
//! Aufgabe 1): Schritte, Nummern, Grenzen, Gespeichert-Marke, Gesten-Zähler, Größenschätzung.

use planora_app::history::*;
use planora_drawing::{Paper, Placement, Projection, Scale, Sheet, ViewKind};
use planora_exchange::ExchangeMesh;
use planora_feature::{Document, FeatureKind, MeshId, Snapshot};
use planora_math::DVec3;
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
