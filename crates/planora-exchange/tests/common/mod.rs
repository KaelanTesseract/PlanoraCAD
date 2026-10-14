//! Mini-Leser für die Tests der Stufe 2 (unabhängig vom Schreiber, eigene CRC).
#![allow(dead_code)]

/// Ein Eintrag eines ZIP-Archivs.
pub struct ZipEntry {
    pub name: String,
    pub data: Vec<u8>,
}

fn u16_at(b: &[u8], at: usize) -> usize {
    usize::from(u16::from_le_bytes([b[at], b[at + 1]]))
}

fn u32_at(b: &[u8], at: usize) -> usize {
    u32::from_le_bytes(b[at..at + 4].try_into().unwrap()) as usize
}

/// CRC-32 Bit für Bit (langsam, aber unabhängig von der Tabelle des Schreibers).
pub fn crc32_slow(data: &[u8]) -> u32 {
    let mut c = u32::MAX;
    for &b in data {
        c ^= u32::from(b);
        for _ in 0..8 {
            c = if c & 1 == 1 {
                (c >> 1) ^ 0xEDB8_8320
            } else {
                c >> 1
            };
        }
    }
    !c
}

/// Liest ein ZIP-Archiv streng: Endsatz am Ende ohne Kommentar, Verzeichnis lückenlos direkt
/// vor dem Endsatz, jeder lokale Kopf passt zum Verzeichnis, Methode „stored“, CRC und Größen
/// stimmen, Einträge liegen lückenlos hintereinander.
pub fn read_zip(b: &[u8]) -> Result<Vec<ZipEntry>, String> {
    if b.len() < 22 {
        return Err("zu kurz".into());
    }
    let end = b.len() - 22;
    if u32_at(b, end) != 0x0605_4B50 {
        return Err("kein Endsatz".into());
    }
    let count = u16_at(b, end + 10);
    if u16_at(b, end + 8) != count || u16_at(b, end + 20) != 0 {
        return Err("Endsatz unstimmig".into());
    }
    let (dir_len, dir_start) = (u32_at(b, end + 12), u32_at(b, end + 16));
    if dir_start + dir_len != end {
        return Err("Verzeichnis liegt nicht direkt vor dem Endsatz".into());
    }
    let (mut at, mut next_local) = (dir_start, 0usize);
    let mut out = Vec::new();
    for _ in 0..count {
        if u32_at(b, at) != 0x0201_4B50 {
            return Err(format!("kein Verzeichniseintrag bei {at}"));
        }
        let (method, crc) = (u16_at(b, at + 10), u32_at(b, at + 16) as u32);
        let (packed, size) = (u32_at(b, at + 20), u32_at(b, at + 24));
        let (name_len, extra, comment) =
            (u16_at(b, at + 28), u16_at(b, at + 30), u16_at(b, at + 32));
        let local = u32_at(b, at + 42);
        let name = String::from_utf8(b[at + 46..at + 46 + name_len].to_vec())
            .map_err(|e| e.to_string())?;
        if method != 0 || packed != size {
            return Err(format!("{name}: nicht „stored“"));
        }
        if local != next_local {
            return Err(format!("{name}: Lücke vor dem lokalen Kopf"));
        }
        if u32_at(b, local) != 0x0403_4B50 {
            return Err(format!("{name}: kein lokaler Kopf"));
        }
        let (l_name, l_extra) = (u16_at(b, local + 26), u16_at(b, local + 28));
        let same = u16_at(b, local + 8) == 0
            && u32_at(b, local + 14) as u32 == crc
            && u32_at(b, local + 18) == size
            && u32_at(b, local + 22) == size
            && b[local + 30..local + 30 + l_name] == *name.as_bytes();
        if !same {
            return Err(format!("{name}: lokaler Kopf passt nicht zum Verzeichnis"));
        }
        let data_at = local + 30 + l_name + l_extra;
        let data = b[data_at..data_at + size].to_vec();
        if crc32_slow(&data) != crc {
            return Err(format!("{name}: CRC falsch"));
        }
        next_local = data_at + size;
        at += 46 + name_len + extra + comment;
        out.push(ZipEntry { name, data });
    }
    if next_local != dir_start || at != end {
        return Err("Einträge nicht lückenlos".into());
    }
    Ok(out)
}
