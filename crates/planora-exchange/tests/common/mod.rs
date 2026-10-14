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

/// Element einer XML-Datei: Name und Attribute (Werte schon entmaskiert).
pub struct XmlElement {
    pub name: String,
    pub attrs: Vec<(String, String)>,
}

/// Prüft XML auf Wohlgeformtheit (Kopfzeile, genau ein Wurzelelement, Tags paarweise
/// geschlossen, Attribute in Anführungszeichen und nicht doppelt, nur die fünf
/// Standard-Entitäten, kein Text außerhalb der Wurzel) und liefert alle Elemente in Reihenfolge.
pub fn parse_xml(text: &str) -> Result<Vec<XmlElement>, String> {
    let mut rest = text.strip_prefix("<?xml ").ok_or("keine XML-Kopfzeile")?;
    let close = rest.find("?>").ok_or("Kopfzeile nicht geschlossen")?;
    rest = &rest[close + 2..];
    let mut stack: Vec<String> = Vec::new();
    let mut out = Vec::new();
    let mut seen_root = false;
    while let Some(lt) = rest.find('<') {
        let between = &rest[..lt];
        check_text(between)?;
        if stack.is_empty() && !between.trim().is_empty() {
            return Err("Text außerhalb des Wurzelelements".into());
        }
        rest = &rest[lt + 1..];
        let gt = rest.find('>').ok_or("Tag nicht geschlossen")?;
        let tag = &rest[..gt];
        rest = &rest[gt + 1..];
        if let Some(name) = tag.strip_prefix('/') {
            if stack.pop().as_deref() != Some(name.trim()) {
                return Err(format!("</{name}> passt nicht"));
            }
            continue;
        }
        let (body, self_closing) = match tag.strip_suffix('/') {
            Some(b) => (b, true),
            None => (tag, false),
        };
        let (name, attrs) = parse_tag(body)?;
        if stack.is_empty() {
            if seen_root {
                return Err("zweites Wurzelelement".into());
            }
            seen_root = true;
        }
        if !self_closing {
            stack.push(name.clone());
        }
        out.push(XmlElement { name, attrs });
    }
    if !rest.trim().is_empty() {
        return Err("Text nach dem Wurzelelement".into());
    }
    if !stack.is_empty() {
        return Err(format!("nicht geschlossen: {stack:?}"));
    }
    if !seen_root {
        return Err("kein Element".into());
    }
    Ok(out)
}

fn parse_tag(body: &str) -> Result<(String, Vec<(String, String)>), String> {
    let body = body.trim();
    let end = body.find(char::is_whitespace).unwrap_or(body.len());
    let name = body[..end].to_string();
    if name.is_empty()
        || !name
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == ':')
    {
        return Err(format!("ungültiger Name „{name}“"));
    }
    let mut rest = body[end..].trim_start();
    let mut attrs: Vec<(String, String)> = Vec::new();
    while !rest.is_empty() {
        let eq = rest
            .find('=')
            .ok_or_else(|| format!("Attribut ohne Wert in <{name}>"))?;
        let key = rest[..eq].trim().to_string();
        let after = rest[eq + 1..].trim_start();
        let quote = after.chars().next().ok_or("Wert fehlt")?;
        if quote != '"' && quote != '\'' {
            return Err(format!("Wert von {key} ohne Anführungszeichen"));
        }
        let close = after[1..]
            .find(quote)
            .ok_or_else(|| format!("Wert von {key} nicht geschlossen"))?;
        let value = &after[1..1 + close];
        check_text(value)?;
        if attrs.iter().any(|(k, _)| *k == key) {
            return Err(format!("Attribut {key} doppelt"));
        }
        attrs.push((key, unescape(value)));
        rest = after[close + 2..].trim_start();
    }
    Ok((name, attrs))
}

fn check_text(t: &str) -> Result<(), String> {
    if t.contains('<') {
        return Err("„<“ im Text".into());
    }
    let mut rest = t;
    while let Some(i) = rest.find('&') {
        let tail = &rest[i..];
        let known = ["&amp;", "&lt;", "&gt;", "&quot;", "&apos;"]
            .iter()
            .any(|e| tail.starts_with(e));
        if !known {
            let start: String = tail.chars().take(8).collect();
            return Err(format!("unbekannte Entität bei „{start}“"));
        }
        rest = &tail[1..];
    }
    Ok(())
}

fn unescape(v: &str) -> String {
    v.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}
