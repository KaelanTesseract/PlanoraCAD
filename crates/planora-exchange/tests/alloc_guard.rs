//! Feindliche Dateien fordern keinen großen Speicher an (Spec §5, §9): Ein eigener Allocator
//! merkt sich die größte einzelne Anforderung. Nur ein Test in dieser Datei, damit nichts
//! anderes gleichzeitig anfordert (Teilprojekt 9, Task 5).

use planora_exchange::*;
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

struct Counting;

static LARGEST: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        LARGEST.fetch_max(layout.size(), Ordering::Relaxed);
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        LARGEST.fetch_max(layout.size(), Ordering::Relaxed);
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        LARGEST.fetch_max(new_size, Ordering::Relaxed);
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

/// Größte erlaubte einzelne Anforderung beim Lesen einer feindlichen Datei (16 MiB; der
/// Lesepuffer hat 64 KiB, eine Zeile höchstens 1 MiB, die erste Reservierung 65 536 Dreiecke).
const LIMIT: usize = 16 * 1024 * 1024;

/// Führt `f` aus und liefert die größte einzelne Anforderung dabei.
fn largest_during(f: impl FnOnce()) -> usize {
    LARGEST.store(0, Ordering::Relaxed);
    f();
    LARGEST.load(Ordering::Relaxed)
}

#[test]
fn hostile_files_never_ask_for_much_memory() {
    let p = Progress::new();
    // Binärkopf verspricht 50 Millionen Dreiecke.
    let mut claim = b"Planora".to_vec();
    claim.resize(80, 0);
    claim.extend_from_slice(&(MAX_TRIANGLES as u32).to_le_bytes());
    let mut cases: Vec<(&str, Vec<u8>, u64, ExchangeError)> = vec![
        (
            "8-GB-Behauptung ohne Inhalt",
            claim.clone(),
            MAX_FILE_BYTES,
            ExchangeError::Truncated,
        ),
        (
            "Kopfzahl größer als die Datei",
            claim.clone(),
            claim.len() as u64,
            ExchangeError::CountTooLarge {
                claimed: MAX_TRIANGLES as u64,
                possible: 0,
            },
        ),
        (
            "Datei über 8 GB",
            claim.clone(),
            MAX_FILE_BYTES + 1,
            ExchangeError::FileTooLarge {
                bytes: MAX_FILE_BYTES + 1,
            },
        ),
    ];
    let mut endless = b"solid x\n".to_vec();
    endless.extend(std::iter::repeat_n(b'1', 10 * MAX_LINE_BYTES));
    let len = endless.len() as u64;
    cases.push((
        "Zeile ohne Ende",
        endless,
        len,
        ExchangeError::Syntax {
            line: 2,
            msg: "Die Zeile ist zu lang".to_string(),
        },
    ));
    for (what, bytes, len, expected) in cases {
        let mut result = None;
        let used = largest_during(|| result = Some(read_stl(&bytes[..], len, &p)));
        assert_eq!(result, Some(Err(expected)), "{what}");
        assert!(used <= LIMIT, "{what}: {used} Bytes");
    }
    // OBJ: riesige Nummer, riesige Zahl, Verweis ohne Punkte
    for (text, expected) in [
        (
            "v 0 0 0\nf 1 1 4294967296000\n",
            ExchangeError::IndexOutOfRange { line: 2 },
        ),
        ("v 1e400 0 0\n", ExchangeError::NotFinite { line: Some(1) }),
        ("f 1 2 3\n", ExchangeError::IndexOutOfRange { line: 1 }),
    ] {
        let mut result = None;
        let used = largest_during(|| {
            result = Some(read_obj(text.as_bytes(), MAX_FILE_BYTES, &p));
        });
        assert_eq!(result, Some(Err(expected)), "{text:?}");
        assert!(used <= LIMIT, "{text:?}: {used} Bytes");
    }
}
