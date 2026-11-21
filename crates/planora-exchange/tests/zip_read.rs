//! ZIP-Leser (Plug-ins Etappe 1, Task 1): gespeicherte und deflate-Einträge, Schutz vor
//! präparierten Dateien.

use planora_exchange::ZipWriter;
use planora_exchange::crc32;
use planora_exchange::zip_read::{ZipRead, ZipReadError, ZipReadLimits};

fn limits() -> ZipReadLimits {
    ZipReadLimits {
        max_entries: 1000,
        max_entry: 32 << 20,
        max_total: 64 << 20,
    }
}

/// Ein Eintrag für [`zip_file`]: Name, Inhalt, mit deflate packen, angegebene Größe (sonst die
/// echte).
struct ZipItem<'a> {
    name: &'a str,
    data: &'a [u8],
    deflate: bool,
    claimed_size: Option<u32>,
}

fn item<'a>(name: &'a str, data: &'a [u8]) -> ZipItem<'a> {
    ZipItem {
        name,
        data,
        deflate: true,
        claimed_size: None,
    }
}

/// Schreibt eine ZIP-Datei (lokale Köpfe, Inhaltsverzeichnis, Ende).
fn zip_file(items: &[ZipItem]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut central = Vec::new();
    for it in items {
        let packed = if it.deflate {
            miniz_oxide::deflate::compress_to_vec(it.data, 6)
        } else {
            it.data.to_vec()
        };
        let method: u16 = if it.deflate { 8 } else { 0 };
        let crc = crc32(it.data);
        let size = it.claimed_size.unwrap_or(it.data.len() as u32);
        let offset = out.len() as u32;
        let head = |sig: u32, central: bool| {
            let mut h = Vec::new();
            h.extend_from_slice(&sig.to_le_bytes());
            if central {
                h.extend_from_slice(&20u16.to_le_bytes());
            }
            h.extend_from_slice(&20u16.to_le_bytes());
            h.extend_from_slice(&0u16.to_le_bytes());
            h.extend_from_slice(&method.to_le_bytes());
            h.extend_from_slice(&0u32.to_le_bytes());
            h.extend_from_slice(&crc.to_le_bytes());
            h.extend_from_slice(&(packed.len() as u32).to_le_bytes());
            h.extend_from_slice(&size.to_le_bytes());
            h.extend_from_slice(&(it.name.len() as u16).to_le_bytes());
            h.extend_from_slice(&0u16.to_le_bytes());
            if central {
                h.extend_from_slice(&[0u8; 6]);
                h.extend_from_slice(&0u32.to_le_bytes());
                h.extend_from_slice(&offset.to_le_bytes());
            }
            h.extend_from_slice(it.name.as_bytes());
            h
        };
        out.extend(head(0x0403_4b50, false));
        out.extend(&packed);
        central.extend(head(0x0201_4b50, true));
    }
    let dir_at = out.len() as u32;
    out.extend(&central);
    out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
    out.extend_from_slice(&[0u8; 4]);
    out.extend_from_slice(&(items.len() as u16).to_le_bytes());
    out.extend_from_slice(&(items.len() as u16).to_le_bytes());
    out.extend_from_slice(&(central.len() as u32).to_le_bytes());
    out.extend_from_slice(&dir_at.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out
}

#[test]
fn stored_entries_from_zip_writer_roundtrip() {
    let mut w = ZipWriter::new(Vec::new());
    w.add_entry("a.txt", &|o| o.write_all(b"Hallo")).unwrap();
    w.add_entry("ordner/b.bin", &|o| o.write_all(&[1, 2, 3]))
        .unwrap();
    let data = w.finish().unwrap();
    let mut z = ZipRead::open(&data, &limits()).unwrap();
    assert_eq!(z.len(), 2);
    assert_eq!(z.name(0), "a.txt");
    assert_eq!(z.name(1), "ordner/b.bin");
    assert_eq!(z.read(0).unwrap(), b"Hallo");
    assert_eq!(z.read(1).unwrap(), [1, 2, 3]);
}

#[test]
fn deflate_and_stored_roundtrip() {
    let text = "Planora ".repeat(500);
    let data = zip_file(&[
        item("gepackt.txt", text.as_bytes()),
        ZipItem {
            deflate: false,
            ..item("roh.txt", b"roh")
        },
    ]);
    let mut z = ZipRead::open(&data, &limits()).unwrap();
    assert_eq!(z.read(0).unwrap(), text.as_bytes());
    assert_eq!(z.read(1).unwrap(), b"roh");
}

#[test]
fn unsafe_paths_rejected() {
    for name in [
        "../x",
        "a/../../x",
        "..\\x",
        "/tmp/x",
        "C:/x",
        "C:\\x",
        "a\\..\\b",
    ] {
        let data = zip_file(&[item(name, b"x")]);
        let err = ZipRead::open(&data, &limits()).err().unwrap();
        assert!(
            matches!(err, ZipReadError::UnsafePath(_)),
            "{name}: {err:?}"
        );
    }
}

#[test]
fn too_many_entries_rejected() {
    let names: Vec<String> = (0..5).map(|i| format!("f{i}.txt")).collect();
    let items: Vec<ZipItem> = names.iter().map(|n| item(n, b"x")).collect();
    let data = zip_file(&items);
    let l = ZipReadLimits {
        max_entries: 4,
        ..limits()
    };
    assert!(matches!(
        ZipRead::open(&data, &l).err(),
        Some(ZipReadError::TooManyEntries { max: 4 })
    ));
}

#[test]
fn zip_bomb_and_limits_rejected() {
    let zeros = vec![0u8; 4 << 20];
    let l = ZipReadLimits {
        max_entries: 10,
        max_entry: 1 << 20,
        max_total: 1 << 20,
    };
    // Ehrliche Angabe über der Grenze: abgelehnt, bevor entpackt wird.
    let data = zip_file(&[item("bombe", &zeros)]);
    assert!(matches!(
        ZipRead::open(&data, &l).err(),
        Some(ZipReadError::TooLarge { .. })
    ));
    // Gelogene Angabe (1000 angegeben, 4 MiB drin): Entpacken hört bei der Angabe auf.
    let liar = ZipItem {
        claimed_size: Some(1000),
        ..item("bombe", &zeros)
    };
    let data = zip_file(&[liar]);
    let mut z = ZipRead::open(&data, &l).unwrap();
    assert!(matches!(z.read(0), Err(ZipReadError::TooLarge { .. })));
    // Viele kleine Einträge, einzeln unter der Grenze, zusammen darüber.
    let part = vec![0u8; 300 << 10];
    let names: Vec<String> = (0..4).map(|i| format!("p{i}")).collect();
    let items: Vec<ZipItem> = names.iter().map(|n| item(n, &part)).collect();
    let data = zip_file(&items);
    assert!(matches!(
        ZipRead::open(&data, &l).err(),
        Some(ZipReadError::TooLarge { .. })
    ));
}

#[test]
fn broken_and_zip64_rejected() {
    assert_eq!(
        ZipRead::open(b"PK nicht wirklich", &limits()).err(),
        Some(ZipReadError::BadZip)
    );
    let mut z64 = zip_file(&[item("a", b"x")]);
    let n = z64.len();
    z64[n - 12..n - 10].copy_from_slice(&0xFFFFu16.to_le_bytes());
    assert_eq!(
        ZipRead::open(&z64, &limits()).err(),
        Some(ZipReadError::Zip64)
    );
}

#[test]
fn wrong_crc_rejected() {
    let mut data = zip_file(&[ZipItem {
        deflate: false,
        ..item("a", b"hallo")
    }]);
    // Ein Nutzdaten-Byte nach dem lokalen Kopf (30 + Name) ändern.
    data[31] ^= 0xFF;
    let mut z = ZipRead::open(&data, &limits()).unwrap();
    assert_eq!(z.read(0).err(), Some(ZipReadError::BadZip));
}
