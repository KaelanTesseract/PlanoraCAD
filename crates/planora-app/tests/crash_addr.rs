//! Rücksprungadressen im Absturzbericht: Bau-Kennung aus dem PE-Kopf, Abschnittstext.

use planora_app::crash_addr::{self, Addresses, BuildId, Frame};

const GUID: [u8; 16] = [
    0x78, 0x56, 0x34, 0x12, 0xbc, 0x9a, 0xf0, 0xde, 0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef,
];

fn put(buf: &mut [u8], at: usize, bytes: &[u8]) {
    buf[at..at + bytes.len()].copy_from_slice(bytes);
}

/// Ein geladenes (gemapptes) PE32+-Abbild: MZ-Kopf, PE-Kopf, Debugverzeichnis mit CodeView.
fn image(with_codeview: bool) -> Vec<u8> {
    let mut b = vec![0u8; 0x3000];
    put(&mut b, 0, b"MZ");
    put(&mut b, 0x3c, &0x80u32.to_le_bytes());
    let pe = 0x80;
    put(&mut b, pe, b"PE\0\0");
    put(&mut b, pe + 4, &0x8664u16.to_le_bytes());
    put(&mut b, pe + 8, &0x6543_2100u32.to_le_bytes());
    put(&mut b, pe + 20, &240u16.to_le_bytes());
    let opt = pe + 24;
    put(&mut b, opt, &0x20bu16.to_le_bytes());
    put(&mut b, opt + 56, &0x3000u32.to_le_bytes());
    put(&mut b, opt + 108, &16u32.to_le_bytes());
    let debug_dir = opt + 112 + 6 * 8;
    put(&mut b, debug_dir, &0x1000u32.to_le_bytes());
    put(&mut b, debug_dir + 4, &28u32.to_le_bytes());
    // Ein Debugeintrag bei 0x1000, CodeView-Daten bei 0x2000.
    let typ: u32 = if with_codeview { 2 } else { 12 };
    put(&mut b, 0x1000 + 12, &typ.to_le_bytes());
    put(&mut b, 0x1000 + 16, &40u32.to_le_bytes());
    put(&mut b, 0x1000 + 20, &0x2000u32.to_le_bytes());
    put(&mut b, 0x2000, b"RSDS");
    put(&mut b, 0x2004, &GUID);
    put(&mut b, 0x2014, &3u32.to_le_bytes());
    put(&mut b, 0x2018, b"planora_app.pdb\0");
    b
}

#[test]
fn bau_kennung_aus_codeview() {
    let id = crash_addr::pe_build_id(&image(true)).expect("Kennung");
    assert_eq!(id.time_stamp, 0x6543_2100);
    assert_eq!(
        id.pdb_guid.as_deref(),
        Some("12345678-9ABC-DEF0-0123-456789ABCDEF")
    );
    assert_eq!(id.pdb_age, Some(3));
    assert_eq!(crash_addr::size_of_image(&image(true)), Some(0x3000));
}

#[test]
fn ohne_codeview_nur_zeitstempel() {
    let id = crash_addr::pe_build_id(&image(false)).expect("Kennung");
    assert_eq!(id.time_stamp, 0x6543_2100);
    assert_eq!(id.pdb_guid, None);
    assert_eq!(id.pdb_age, None);
}

#[test]
fn kaputtes_abbild_panikt_nie() {
    let full = image(true);
    for n in 0..full.len() {
        let _ = crash_addr::pe_build_id(&full[..n]);
        let _ = crash_addr::size_of_image(&full[..n]);
    }
    let mut weird = full.clone();
    // Riesige Werte überall, wo Längen und Versätze stehen.
    for at in [
        0x3c,
        0x80 + 20,
        0x80 + 24 + 108,
        0x80 + 24 + 160,
        0x80 + 24 + 164,
        0x1014,
    ] {
        put(&mut weird, at, &u32::MAX.to_le_bytes());
        let _ = crash_addr::pe_build_id(&weird);
    }
    assert_eq!(crash_addr::pe_build_id(b"kein PE"), None);
}

fn sample() -> Addresses {
    Addresses {
        module: "planora-app.exe".into(),
        build: Some(BuildId {
            time_stamp: 0x6543_2100,
            pdb_guid: Some("12345678-9ABC-DEF0-0123-456789ABCDEF".into()),
            pdb_age: Some(3),
        }),
        frames: vec![
            Frame {
                module: Some("planora-app.exe".into()),
                offset: 0x1a2b3c,
            },
            Frame {
                module: Some("ntdll.dll".into()),
                offset: 0x10,
            },
            Frame {
                module: None,
                offset: 0x7ff0_0000_1234,
            },
        ],
    }
}

#[test]
fn abschnitt_mit_rahmen() {
    let t = crash_addr::section_text(Some(&sample()));
    let want = "Adressen (für Dennis):\n\
                Modul: planora-app.exe\n\
                PDB-GUID: 12345678-9ABC-DEF0-0123-456789ABCDEF\n\
                PDB-Age: 3\n\
                PE-Zeit: 0x65432100\n\
                \x20 0: planora-app.exe+0x1a2b3c\n\
                \x20 1: ntdll.dll+0x10\n\
                \x20 2: unbekannt 0x7ff000001234\n";
    assert_eq!(t, want);
}

#[test]
fn abschnitt_ohne_angaben() {
    let t = crash_addr::section_text(None);
    assert_eq!(t, "Adressen (für Dennis):\nnicht verfügbar\n");
    let mut a = sample();
    a.build = None;
    a.frames.clear();
    let t = crash_addr::section_text(Some(&a));
    assert!(t.contains("PDB-GUID: nicht verfügbar\n"), "{t}");
    assert!(t.contains("keine Rahmen\n"), "{t}");
}

#[test]
fn hoechstens_max_rahmen() {
    let mut a = sample();
    a.frames = (0..200)
        .map(|i| Frame {
            module: Some("planora-app.exe".into()),
            offset: i,
        })
        .collect();
    let t = crash_addr::section_text(Some(&a));
    assert_eq!(
        t.matches("planora-app.exe+0x").count(),
        crash_addr::MAX_FRAMES
    );
}

#[cfg(windows)]
#[test]
fn eigene_adressen_unter_windows() {
    let a = crash_addr::capture().expect("Adressen");
    assert!(
        a.module.to_ascii_lowercase().ends_with(".exe"),
        "{}",
        a.module
    );
    let id = a.build.as_ref().expect("Bau-Kennung");
    assert!(id.pdb_guid.is_some(), "CodeView fehlt");
    assert!(!a.frames.is_empty() && a.frames.len() <= crash_addr::MAX_FRAMES);
    assert!(
        a.frames
            .iter()
            .any(|f| f.module.as_deref() == Some(a.module.as_str())),
        "kein Rahmen im eigenen Modul"
    );
}

#[cfg(not(windows))]
#[test]
fn ausserhalb_von_windows_nicht_verfuegbar() {
    assert!(crash_addr::capture().is_none());
}
