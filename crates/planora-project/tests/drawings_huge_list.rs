//! Eine feindliche Datei mit 1 000 001 Ansichten wird abgelehnt, ohne dafür Speicher in der
//! Größe der Liste anzulegen (Teilprojekt 11, Task 13). Eigene Testdatei mit einem einzigen
//! Test, damit der zählende Speicherverwalter nur diesen Test misst.

use planora_drawing::{Paper, Placement, Projection, Scale, Sheet, ViewKind};
use planora_feature::{Document, FeatureKind};
use planora_math::DVec3;
use planora_project::{ProjectError, decode_version, encode_version};
use serde_json::{Value, json};
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

/// Zählt die belegten Bytes und merkt sich den Höchststand.
struct Counting;

static NOW: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: Weitergabe an den Systemverwalter mit demselben Layout.
        let p = unsafe { System.alloc(layout) };
        if !p.is_null() {
            let now = NOW.fetch_add(layout.size(), Ordering::Relaxed) + layout.size();
            PEAK.fetch_max(now, Ordering::Relaxed);
        }
        p
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: `ptr` stammt aus `alloc` mit demselben Layout.
        unsafe { System.dealloc(ptr, layout) };
        NOW.fetch_sub(layout.size(), Ordering::Relaxed);
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: Weitergabe an den Systemverwalter; Verträge wie bei `GlobalAlloc::realloc`.
        let p = unsafe { System.realloc(ptr, layout, new_size) };
        if !p.is_null() {
            NOW.fetch_sub(layout.size(), Ordering::Relaxed);
            let now = NOW.fetch_add(new_size, Ordering::Relaxed) + new_size;
            PEAK.fetch_max(now, Ordering::Relaxed);
        }
        p
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

fn good() -> String {
    let mut d = Document::new();
    d.add_feature(
        "Quader 1",
        FeatureKind::Box {
            size: DVec3::splat(10.0),
            origin: DVec3::ZERO,
        },
    )
    .unwrap();
    let z = d
        .add_drawing(
            "Blatt 1",
            Sheet {
                paper: Paper::A4,
                landscape: false,
            },
            Projection::First,
        )
        .unwrap();
    d.add_view(
        z,
        ViewKind::Front,
        Scale { num: 1, den: 1 },
        Placement::Free { x: 0.0, y: 0.0 },
        None,
    )
    .unwrap();
    encode_version(&d.snapshot()).unwrap()
}

#[test]
fn a_huge_view_list_is_refused_without_allocating_it() {
    let mut v: Value = serde_json::from_str(&good()).unwrap();
    v["zeichnungen"][0]["ansichten"] = json!("@@ANSICHTEN@@");
    v["zeichnungen"][0]["naechste_ansicht"] = json!(u64::MAX - 1);
    let marked = serde_json::to_string_pretty(&v).unwrap();
    const N: usize = 1_000_001;
    let entry = r#"{"id":1,"art":"vorne","massstab":[1,1],"lage":{"typ":"frei","x":0.0,"y":0.0},"verdeckt_zeigen":false,"eltern":null}"#;
    let mut list = String::with_capacity(N * (entry.len() + 1) + 2);
    list.push('[');
    for i in 0..N {
        if i > 0 {
            list.push(',');
        }
        list.push_str(entry);
    }
    list.push(']');
    let text = marked.replacen("\"@@ANSICHTEN@@\"", &list, 1);
    drop(list);
    assert!(text.len() > 100_000_000);

    let base = NOW.load(Ordering::Relaxed);
    PEAK.store(base, Ordering::Relaxed);
    let start = Instant::now();
    let e = decode_version(&text, "x.json").unwrap_err();
    let took = start.elapsed();
    let extra = PEAK.load(Ordering::Relaxed) - base;

    match e {
        ProjectError::Corrupt { msg, .. } => {
            assert!(msg.contains("Ansichten: zu viele Einträge"), "{msg}")
        }
        other => panic!("{other:?}"),
    }
    // Die Liste allein hätte über 1 000 000 Einträge zu je mehr als 64 Bytes (> 64 MB).
    assert!(extra < 1_000_000, "{extra} Bytes zusätzlich belegt");
    // Großzügige Schranke (Debug-Bau, parallel laufende Tests): der Text wird einmal gelesen.
    assert!(took < Duration::from_secs(30), "{took:?}");
}
