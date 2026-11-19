//! Kleiner ZIP-Leser für Pakete und Downloads: liest das Inhaltsverzeichnis am Ende der Datei
//! und entpackt gewählte Einträge (gespeichert oder deflate, über `miniz_oxide`). Kein ZIP64,
//! keine Verschlüsselung. Vorhaben 2 stellt `planora-eda-lib` auf diesen Leser um.
//!
//! Schutz vor präparierten Dateien: höchstens [`ZipReadLimits::max_entries`] Einträge, jeder
//! Eintrag höchstens [`ZipReadLimits::max_entry`] Bytes, alle zusammen höchstens
//! [`ZipReadLimits::max_total`]; jeder Eintrag wird höchstens bis zu seiner angegebenen Größe
//! entpackt (eine Dekompressionsbombe, die mehr liefert, als sie angibt, bricht ab) und über
//! seine CRC32 geprüft. Namen mit `..`, absolute Pfade und Laufwerksbuchstaben werden abgelehnt.

use crate::zip::crc32;
use planora_i18n::tr;
use std::fmt;

/// Grenzen beim Lesen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ZipReadLimits {
    /// Höchstzahl der Einträge.
    pub max_entries: usize,
    /// Größte einzelne Datei (entpackt).
    pub max_entry: u64,
    /// Höchstens so viele Bytes werden insgesamt entpackt.
    pub max_total: u64,
}

/// Warum eine ZIP-Datei nicht gelesen wurde (Meldungen in der Sprache des Programms).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ZipReadError {
    BadZip,
    Zip64,
    TooManyEntries { max: usize },
    TooLarge { max_mib: u64 },
    UnsafePath(String),
    Encrypted(String),
    Method { name: String, method: u16 },
}

impl fmt::Display for ZipReadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = match self {
            ZipReadError::BadZip => tr!("zip-read-bad"),
            ZipReadError::Zip64 => tr!("zip-read-zip64"),
            ZipReadError::TooManyEntries { max } => {
                tr!("zip-read-too-many", max = max.to_string())
            }
            ZipReadError::TooLarge { max_mib } => {
                tr!("zip-read-too-large", max_mib = max_mib.to_string())
            }
            ZipReadError::UnsafePath(name) => tr!("zip-read-unsafe-path", name = name),
            ZipReadError::Encrypted(name) => tr!("zip-read-encrypted", name = name),
            ZipReadError::Method { name, method } => {
                tr!("zip-read-method", name = name, method = method.to_string())
            }
        };
        f.write_str(&text)
    }
}

impl std::error::Error for ZipReadError {}

const EOCD_SIG: u32 = 0x0605_4b50;
const CENTRAL_SIG: u32 = 0x0201_4b50;
const LOCAL_SIG: u32 = 0x0403_4b50;
const EOCD_LEN: usize = 22;
const CENTRAL_LEN: usize = 46;
const LOCAL_LEN: usize = 30;

/// Ein Eintrag des Inhaltsverzeichnisses.
#[derive(Clone, Debug)]
struct ZipEntry {
    /// Name mit `/` als Trenner.
    name: String,
    method: u16,
    flags: u16,
    crc: u32,
    packed: u64,
    size: u64,
    local: u64,
}

/// Eine geöffnete ZIP-Datei (ganz im Speicher; die Dateigröße ist vorher begrenzt).
pub struct ZipRead<'a> {
    data: &'a [u8],
    entries: Vec<ZipEntry>,
    /// Bisher entpackte Bytes (für die Gesamtgrenze).
    unpacked: u64,
    /// Grenzen (Einzel- und Gesamtgröße).
    max_entry: u64,
    max_total: u64,
}

fn u16_at(d: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(d.get(at..at + 2)?.try_into().ok()?))
}

fn u32_at(d: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(d.get(at..at + 4)?.try_into().ok()?))
}

/// Ist der Name ein sicherer, relativer Pfad (kein `..`, nicht absolut, kein Laufwerk)?
fn safe_name(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('/')
        && !name.contains(':')
        && !name.contains('\0')
        && name.split('/').all(|part| part != "..")
}

impl<'a> ZipRead<'a> {
    /// Liest das Inhaltsverzeichnis und prüft Zahl, Namen und angegebene Größen.
    pub fn open(data: &'a [u8], limits: &ZipReadLimits) -> Result<ZipRead<'a>, ZipReadError> {
        let bad = || ZipReadError::BadZip;
        if data.len() < EOCD_LEN {
            return Err(bad());
        }
        // Ende des Inhaltsverzeichnisses: höchstens 64 KiB Kommentar dahinter.
        let lowest = data.len().saturating_sub(EOCD_LEN + u16::MAX as usize);
        let eocd = (lowest..=data.len() - EOCD_LEN)
            .rev()
            .find(|&i| u32_at(data, i) == Some(EOCD_SIG))
            .ok_or_else(bad)?;
        let count = u16_at(data, eocd + 10).ok_or_else(bad)? as usize;
        let dir_len = u32_at(data, eocd + 12).ok_or_else(bad)? as usize;
        let dir_at = u32_at(data, eocd + 16).ok_or_else(bad)? as usize;
        if count == u16::MAX as usize || dir_at == u32::MAX as usize {
            // ZIP64: für Bauteil-Downloads nie nötig.
            return Err(ZipReadError::Zip64);
        }
        if count > limits.max_entries {
            return Err(ZipReadError::TooManyEntries {
                max: limits.max_entries,
            });
        }
        let dir = data
            .get(dir_at..dir_at.checked_add(dir_len).ok_or_else(bad)?)
            .ok_or_else(bad)?;
        let mut entries = Vec::with_capacity(count);
        let mut pos = 0usize;
        let mut declared = 0u64;
        for _ in 0..count {
            if u32_at(dir, pos) != Some(CENTRAL_SIG) {
                return Err(bad());
            }
            let field16 = |off: usize| u16_at(dir, pos + off).ok_or_else(bad);
            let field32 = |off: usize| u32_at(dir, pos + off).ok_or_else(bad);
            let flags = field16(8)?;
            let method = field16(10)?;
            let crc = field32(16)?;
            let packed = field32(20)? as u64;
            let size = field32(24)? as u64;
            let name_len = field16(28)? as usize;
            let extra_len = field16(30)? as usize;
            let comment_len = field16(32)? as usize;
            let local = field32(42)? as u64;
            let start = pos + CENTRAL_LEN;
            let raw = dir.get(start..start + name_len).ok_or_else(bad)?;
            let name = String::from_utf8_lossy(raw).replace('\\', "/");
            if !safe_name(&name) {
                return Err(ZipReadError::UnsafePath(name));
            }
            if packed == u32::MAX as u64 || size == u32::MAX as u64 || local == u32::MAX as u64 {
                return Err(ZipReadError::Zip64);
            }
            declared = declared.saturating_add(size);
            if size > limits.max_entry {
                return Err(ZipReadError::TooLarge {
                    max_mib: limits.max_entry >> 20,
                });
            }
            if declared > limits.max_total {
                return Err(ZipReadError::TooLarge {
                    max_mib: limits.max_total >> 20,
                });
            }
            entries.push(ZipEntry {
                name,
                method,
                flags,
                crc,
                packed,
                size,
                local,
            });
            pos = start + name_len + extra_len + comment_len;
        }
        Ok(ZipRead {
            data,
            entries,
            unpacked: 0,
            max_entry: limits.max_entry,
            max_total: limits.max_total,
        })
    }

    /// Zahl der Einträge.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Ob das Inhaltsverzeichnis leer ist.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Name des Eintrags `i` (mit `/` als Trenner); Verzeichnisse enden auf `/`.
    ///
    /// # Panics
    /// Wenn `i >= len()`.
    pub fn name(&self, i: usize) -> &str {
        &self.entries[i].name
    }

    /// Bisher entpackte Bytes.
    pub fn unpacked(&self) -> u64 {
        self.unpacked
    }

    /// Entpackt den Eintrag `i`: höchstens bis zur angegebenen Größe und innerhalb der
    /// Gesamtgrenze; Größe und CRC32 müssen stimmen.
    pub fn read(&mut self, i: usize) -> Result<Vec<u8>, ZipReadError> {
        let bad = || ZipReadError::BadZip;
        let e = self.entries.get(i).ok_or_else(bad)?.clone();
        if e.flags & 1 != 0 {
            return Err(ZipReadError::Encrypted(e.name));
        }
        let at = usize::try_from(e.local).map_err(|_| bad())?;
        if u32_at(self.data, at) != Some(LOCAL_SIG) {
            return Err(bad());
        }
        let name_len = u16_at(self.data, at + 26).ok_or_else(bad)? as usize;
        let extra_len = u16_at(self.data, at + 28).ok_or_else(bad)? as usize;
        let start = at + LOCAL_LEN + name_len + extra_len;
        let end = start
            .checked_add(usize::try_from(e.packed).map_err(|_| bad())?)
            .ok_or_else(bad)?;
        let packed = self.data.get(start..end).ok_or_else(bad)?;
        let budget = self.max_total.saturating_sub(self.unpacked);
        let too_large = || ZipReadError::TooLarge {
            max_mib: self.max_total >> 20,
        };
        if e.size > self.max_entry || e.size > budget {
            return Err(too_large());
        }
        let out = match e.method {
            0 => packed.to_vec(),
            8 => {
                // Ein Byte mehr als angegeben erlaubt: kommt es, lügt die Angabe (Bombe).
                let limit = usize::try_from(e.size)
                    .map_err(|_| bad())?
                    .saturating_add(1);
                miniz_oxide::inflate::decompress_to_vec_with_limit(packed, limit).map_err(
                    |err| match err.status {
                        miniz_oxide::inflate::TINFLStatus::HasMoreOutput => too_large(),
                        _ => bad(),
                    },
                )?
            }
            m => {
                return Err(ZipReadError::Method {
                    name: e.name,
                    method: m,
                });
            }
        };
        if out.len() as u64 != e.size {
            return Err(if out.len() as u64 > e.size {
                too_large()
            } else {
                bad()
            });
        }
        if crc32(&out) != e.crc {
            return Err(bad());
        }
        self.unpacked += out.len() as u64;
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::safe_name;

    #[test]
    fn unsafe_names_are_rejected() {
        assert!(safe_name("languages/de.ftl"));
        assert!(safe_name("a..b/c.svg"));
        assert!(!safe_name("../evil"));
        assert!(!safe_name("a/../../evil"));
        assert!(!safe_name("/etc/passwd"));
        assert!(!safe_name("C:/Windows/x"));
        assert!(!safe_name(""));
    }
}
