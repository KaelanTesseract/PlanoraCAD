//! Kleinster ZIP-Schreiber für 3MF (Spec E7, Plan P7): Methode „stored“ ohne Kompression,
//! CRC-32, feste Zeitangabe (1.1.1980), ohne ZIP64. Jeder Eintrag wird zweimal erzeugt: erst
//! Größe und CRC messen, dann schreiben; so liegt nichts ganz im Speicher.

use crate::error::ExchangeError;
use planora_i18n::tr;
use std::io::{self, Write};

/// Erzeugerpolynom der CRC-32 nach IEEE 802.3 (wie ZIP), bitweise gespiegelt.
const CRC_POLY: u32 = 0xEDB8_8320;
const CRC_TABLE: [u32; 256] = crc_table();
const LOCAL_SIG: u32 = 0x0403_4B50;
const CENTRAL_SIG: u32 = 0x0201_4B50;
const END_SIG: u32 = 0x0605_4B50;
/// ZIP-Version 2.0 („stored“ und Ordner im Namen).
const VERSION: u16 = 20;
/// 00:00 Uhr im DOS-Format.
const DOS_TIME: u16 = 0;
/// 1.1.1980 im DOS-Format (Jahr 0 ab 1980, Monat 1, Tag 1).
const DOS_DATE: u16 = (1 << 5) | 1;
/// Feste Länge eines lokalen Kopfs ohne Namen.
const LOCAL_HEADER: usize = 30;

const fn crc_table() -> [u32; 256] {
    let mut table = [0u32; 256];
    let mut i = 0;
    while i < 256 {
        let mut c = i as u32;
        let mut k = 0;
        while k < 8 {
            c = if c & 1 != 0 {
                CRC_POLY ^ (c >> 1)
            } else {
                c >> 1
            };
            k += 1;
        }
        table[i] = c;
        i += 1;
    }
    table
}

/// Laufende CRC-32.
#[derive(Clone, Copy, Debug)]
pub struct Crc32(u32);

impl Default for Crc32 {
    fn default() -> Self {
        Crc32(u32::MAX)
    }
}

impl Crc32 {
    pub fn new() -> Crc32 {
        Crc32::default()
    }

    pub fn update(&mut self, bytes: &[u8]) {
        let mut c = self.0;
        for &b in bytes {
            c = CRC_TABLE[((c ^ u32::from(b)) & 0xFF) as usize] ^ (c >> 8);
        }
        self.0 = c;
    }

    pub fn finish(self) -> u32 {
        !self.0
    }
}

/// CRC-32 eines Bytefelds.
pub fn crc32(bytes: &[u8]) -> u32 {
    let mut c = Crc32::new();
    c.update(bytes);
    c.finish()
}

/// Inhalt eines Eintrags: wird zweimal aufgerufen und muss beide Male dieselben Bytes liefern.
pub type Content<'a> = &'a dyn Fn(&mut dyn Write) -> io::Result<()>;

/// Zählt Bytes und rechnet die CRC, ohne etwas zu speichern.
struct Measure {
    crc: Crc32,
    len: u64,
}

impl Measure {
    fn new() -> Measure {
        Measure {
            crc: Crc32::new(),
            len: 0,
        }
    }
}

impl Write for Measure {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.crc.update(buf);
        self.len += buf.len() as u64;
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// Schreibt ins Ziel und misst dabei mit.
struct Tee<'a, W> {
    out: &'a mut W,
    measure: Measure,
}

impl<W: Write> Write for Tee<'_, W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let n = self.out.write(buf)?;
        self.measure.crc.update(&buf[..n]);
        self.measure.len += n as u64;
        Ok(n)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.out.flush()
    }
}

struct Entry {
    name: String,
    crc: u32,
    size: u32,
    offset: u32,
}

/// ZIP-Archiv im Entstehen.
pub struct ZipWriter<W: Write> {
    out: W,
    offset: u64,
    entries: Vec<Entry>,
}

fn put16(b: &mut Vec<u8>, v: u16) {
    b.extend_from_slice(&v.to_le_bytes());
}

fn put32(b: &mut Vec<u8>, v: u32) {
    b.extend_from_slice(&v.to_le_bytes());
}

/// Größen und Lagen müssen ohne ZIP64 in 32 Bit passen (bis knapp 4 GiB).
fn small(n: u64) -> Result<u32, ExchangeError> {
    u32::try_from(n)
        .ok()
        .filter(|&v| v != u32::MAX)
        .ok_or(ExchangeError::TooLargeForFormat { format: "3MF" })
}

impl<W: Write> ZipWriter<W> {
    pub fn new(out: W) -> Self {
        ZipWriter {
            out,
            offset: 0,
            entries: Vec::new(),
        }
    }

    /// Hängt einen Eintrag an (Name in ASCII, Ordner mit „/“).
    pub fn add_entry(&mut self, name: &str, content: Content<'_>) -> Result<(), ExchangeError> {
        let name_len = u16::try_from(name.len())
            .map_err(|_| ExchangeError::Corrupt(tr!("exchange-zip-name-long")))?;
        let mut measure = Measure::new();
        content(&mut measure)?;
        let (crc, len) = (measure.crc.finish(), measure.len);
        let size = small(len)?;
        let offset = small(self.offset)?;
        let mut header = Vec::with_capacity(LOCAL_HEADER + name.len());
        put32(&mut header, LOCAL_SIG);
        put16(&mut header, VERSION);
        put16(&mut header, 0); // Kennzeichen
        put16(&mut header, 0); // Methode „stored“
        put16(&mut header, DOS_TIME);
        put16(&mut header, DOS_DATE);
        put32(&mut header, crc);
        put32(&mut header, size); // gepackt
        put32(&mut header, size); // ungepackt
        put16(&mut header, name_len);
        put16(&mut header, 0); // Zusatzfeld
        header.extend_from_slice(name.as_bytes());
        self.out.write_all(&header)?;
        let mut tee = Tee {
            out: &mut self.out,
            measure: Measure::new(),
        };
        content(&mut tee)?;
        if tee.measure.crc.finish() != crc || tee.measure.len != len {
            return Err(ExchangeError::Corrupt(tr!("exchange-zip-content-changed")));
        }
        self.offset += header.len() as u64 + len;
        self.entries.push(Entry {
            name: name.to_string(),
            crc,
            size,
            offset,
        });
        Ok(())
    }

    /// Schreibt Verzeichnis und Endsatz und gibt das Ziel zurück.
    pub fn finish(mut self) -> Result<W, ExchangeError> {
        let start = small(self.offset)?;
        let mut dir = Vec::new();
        for e in &self.entries {
            put32(&mut dir, CENTRAL_SIG);
            put16(&mut dir, VERSION); // erstellt mit
            put16(&mut dir, VERSION); // benötigt
            put16(&mut dir, 0); // Kennzeichen
            put16(&mut dir, 0); // Methode „stored“
            put16(&mut dir, DOS_TIME);
            put16(&mut dir, DOS_DATE);
            put32(&mut dir, e.crc);
            put32(&mut dir, e.size);
            put32(&mut dir, e.size);
            put16(&mut dir, e.name.len() as u16);
            put16(&mut dir, 0); // Zusatzfeld
            put16(&mut dir, 0); // Kommentar
            put16(&mut dir, 0); // Datenträger
            put16(&mut dir, 0); // interne Attribute
            put32(&mut dir, 0); // externe Attribute
            put32(&mut dir, e.offset);
            dir.extend_from_slice(e.name.as_bytes());
        }
        let dir_len = small(dir.len() as u64)?;
        small(self.offset + dir.len() as u64)?;
        let count = u16::try_from(self.entries.len())
            .map_err(|_| ExchangeError::TooLargeForFormat { format: "3MF" })?;
        put32(&mut dir, END_SIG);
        put16(&mut dir, 0); // dieser Datenträger
        put16(&mut dir, 0); // Datenträger des Verzeichnisses
        put16(&mut dir, count);
        put16(&mut dir, count);
        put32(&mut dir, dir_len);
        put32(&mut dir, start);
        put16(&mut dir, 0); // Kommentar
        self.out.write_all(&dir)?;
        self.out.flush()?;
        Ok(self.out)
    }
}
