//! Feindliche Dateien mit riesigen Maß- und Bezugslisten werden abgelehnt, ohne dafür Speicher
//! in der Größe der Liste anzulegen (Teilprojekt 12, Aufgabe 9). Eigene Testdatei mit einem
//! einzigen Test, damit der zählende Speicherverwalter nur diesen Test misst (wie
//! `drawings_huge_list.rs`).

use planora_drawing::dimension::{DimId, DimKind, DimSpec, GeoRef, GeoRefData, TagData, Tolerance};
use planora_drawing::{Paper, Placement, Projection, Scale, Sheet, ViewId, ViewKind};
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

/// Gültige Datei in Format 5 mit einem Maß (Durchmesser an einer Fläche) in Ansicht 1.
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
    d.add_dimension(
        z,
        DimSpec {
            id: DimId(0),
            view: ViewId(1),
            kind: DimKind::Diameter,
            refs: vec![GeoRef::Face(GeoRefData::Face(TagData {
                origin: 1,
                role: "M".into(),
                hint: DVec3::ZERO,
            }))],
            offset: 10.0,
            angle: 45.0,
            text_shift: (0.0, 0.0),
            tolerance: Tolerance::None,
            decimals: 1,
            text_override: None,
            chain: None,
            suggested: false,
        },
    )
    .unwrap();
    let text = encode_version(&d.snapshot()).unwrap();
    assert!(text.contains("\"format\": 5"));
    text
}

/// Ersetzt `path` durch eine Liste aus `n` Kopien von `entry` (als Text, ohne `Value`).
fn with_list(
    good: &str,
    path: impl FnOnce(&mut Value) -> &mut Value,
    entry: &str,
    n: usize,
) -> String {
    let mut v: Value = serde_json::from_str(good).unwrap();
    *path(&mut v) = json!("@@LISTE@@");
    v["zeichnungen"][0]["naechstes_mass"] = json!(u64::MAX - 1);
    let marked = serde_json::to_string_pretty(&v).unwrap();
    let mut list = String::with_capacity(n * (entry.len() + 1) + 2);
    list.push('[');
    for i in 0..n {
        if i > 0 {
            list.push(',');
        }
        list.push_str(entry);
    }
    list.push(']');
    marked.replacen("\"@@LISTE@@\"", &list, 1)
}

/// Liest `text` und gibt Meldung, zusätzlich belegte Bytes (Höchststand) und Dauer zurück.
fn measure(text: &str) -> (String, usize, Duration) {
    let base = NOW.load(Ordering::Relaxed);
    PEAK.store(base, Ordering::Relaxed);
    let start = Instant::now();
    let e = decode_version(text, "x.json").unwrap_err();
    let took = start.elapsed();
    let extra = PEAK.load(Ordering::Relaxed) - base;
    let full = e.to_string();
    assert!(
        full.starts_with("Die Datei „x.json“ ist beschädigt: "),
        "{full}"
    );
    match e {
        ProjectError::Corrupt { msg, .. } => (msg, extra, took),
        other => panic!("{other:?}"),
    }
}

#[test]
fn huge_dimension_and_reference_lists_are_refused_without_allocating_them() {
    let good = good();
    let bezug = r#"{"typ":"flaeche","marke":{"herkunft":1,"rolle":"M","hinweis":[0.0,0.0,0.0]}}"#;

    // 400 000 Maße: über 100 MB Text, die Liste allein hätte über 400 000 Einträge zu je
    // mehr als 200 Bytes (> 80 MB). Gelesen werden höchstens 2000 + 1.
    let mass = format!(
        r#"{{"id":1,"ansicht":1,"art":"durchmesser","bezuege":[{bezug}],"abstand":10.0,"winkel":45.0,"textversatz":[0.0,0.0],"toleranz":{{"typ":"keine"}},"stellen":1,"text":null,"kette":null,"vorgeschlagen":false}}"#
    );
    const N: usize = 400_000;
    let text = with_list(&good, |v| &mut v["zeichnungen"][0]["masse"], &mass, N);
    assert!(text.len() > 100_000_000);
    let (msg, extra, took) = measure(&text);
    drop(text);
    assert!(
        msg.contains("Maße: zu viele Einträge (höchstens 2000)"),
        "{msg}"
    );
    assert!(extra < 4_000_000, "{extra} Bytes zusätzlich belegt");
    assert!(took < Duration::from_secs(30), "{took:?}");

    // Ein Maß mit 2 000 000 Bezügen: abgelehnt nach dem fünften.
    const M: usize = 2_000_000;
    let text = with_list(
        &good,
        |v| &mut v["zeichnungen"][0]["masse"][0]["bezuege"],
        bezug,
        M,
    );
    assert!(text.len() > 100_000_000);
    let (msg, extra, took) = measure(&text);
    assert!(
        msg.contains("Bezüge: zu viele Einträge (höchstens 4)"),
        "{msg}"
    );
    assert!(extra < 1_000_000, "{extra} Bytes zusätzlich belegt");
    assert!(took < Duration::from_secs(30), "{took:?}");
}
