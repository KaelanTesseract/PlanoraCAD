//! Kleine ZIP-Schreibfunktion für die Projektdatei in der Rückmeldung: Einträge nur
//! gespeichert (ohne Kompression), Namen in UTF-8 (Merker gesetzt), ohne ZIP64. Eine eigene
//! Funktion statt einer weiteren Bibliothek; die Prüfsumme rechnet `crc32fast`.
//!
//! Ein `.pcad`-Projekt ist ein Ordner, den Datei-Felder im Browser meist nicht annehmen. Als
//! `<Name>.pcad.zip` geht er als eine Datei mit; die Autosicherung (`.autosave.json`, ein
//! ungespeicherter Zwischenstand) bleibt draußen.

use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;

use planora_project::docstore::AUTOSAVE_FILE;

/// Merker „Namen in UTF-8“ (Bit 11) im Feld „general purpose flags“.
const FLAG_UTF8: u16 = 0x0800;
/// Version 2.0: nötig für Ordner-Einträge.
const VERSION: u16 = 20;
/// Höchstzahl Einträge ohne ZIP64.
const MAX_ENTRIES: usize = 0xFFFF;

/// Ein Eintrag fürs Inhaltsverzeichnis.
struct Entry {
    name: Vec<u8>,
    crc: u32,
    size: u32,
    offset: u32,
    dir: bool,
    time: u16,
    date: u16,
}

/// Schreibt den Projektordner `project` (samt Unterordnern, ohne `.autosave.json`) als ZIP
/// nach `out`; im Archiv liegt alles unter dem Ordnernamen (`Teil.pcad/…`). Misslingt es,
/// bleibt keine halbe Datei liegen. Gedacht für Projekte bis einige hundert MB (die Grenze
/// zum Mitschicken prüft der Aufrufer); über 4 GB oder 65535 Einträge gibt es einen Fehler.
pub fn write_project_zip(project: &Path, out: &Path) -> std::io::Result<()> {
    let r = write_inner(project, out);
    if r.is_err() {
        let _ = std::fs::remove_file(out);
    }
    r
}

fn write_inner(project: &Path, out: &Path) -> std::io::Result<()> {
    let root = project
        .file_name()
        .ok_or_else(|| std::io::Error::other("kein Ordnername"))?
        .to_string_lossy()
        .into_owned();
    if !project.is_dir() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "kein Projektordner",
        ));
    }
    let mut w = Writer {
        out: BufWriter::new(File::create(out)?),
        pos: 0,
        entries: Vec::new(),
    };
    w.add_dir(project, &root)?;
    w.finish()
}

struct Writer {
    out: BufWriter<File>,
    pos: u64,
    entries: Vec<Entry>,
}

fn too_big() -> std::io::Error {
    std::io::Error::other("zu groß für ZIP ohne ZIP64")
}

impl Writer {
    fn write(&mut self, b: &[u8]) -> std::io::Result<()> {
        self.out.write_all(b)?;
        self.pos += b.len() as u64;
        Ok(())
    }

    /// Ordner `dir` als `name/` und alles darin (Einträge sortiert, damit das Archiv gleich
    /// bleibt).
    fn add_dir(&mut self, dir: &Path, name: &str) -> std::io::Result<()> {
        let modified = std::fs::metadata(dir).and_then(|m| m.modified()).ok();
        self.add(&format!("{name}/"), &[], modified, true)?;
        let mut items: Vec<_> = std::fs::read_dir(dir)?.collect::<Result<_, _>>()?;
        items.sort_by_key(|e| e.file_name());
        for e in items {
            let file_name = e.file_name().to_string_lossy().into_owned();
            let kind = e.file_type()?;
            let child = format!("{name}/{file_name}");
            if kind.is_dir() {
                self.add_dir(&e.path(), &child)?;
            } else if kind.is_file() && file_name != AUTOSAVE_FILE {
                let data = std::fs::read(e.path())?;
                let modified = e.metadata().and_then(|m| m.modified()).ok();
                self.add(&child, &data, modified, false)?;
            }
        }
        Ok(())
    }

    /// Lokaler Kopf und Inhalt eines Eintrags.
    fn add(
        &mut self,
        name: &str,
        data: &[u8],
        modified: Option<std::time::SystemTime>,
        dir: bool,
    ) -> std::io::Result<()> {
        if self.entries.len() >= MAX_ENTRIES {
            return Err(too_big());
        }
        let name = name.as_bytes().to_vec();
        let name_len = u16::try_from(name.len()).map_err(|_| too_big())?;
        let size = u32::try_from(data.len()).map_err(|_| too_big())?;
        let offset = u32::try_from(self.pos).map_err(|_| too_big())?;
        let crc = crc32fast::hash(data);
        let (time, date) = dos_time(modified);
        let mut h = Vec::with_capacity(30 + name.len());
        h.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
        h.extend_from_slice(&VERSION.to_le_bytes());
        h.extend_from_slice(&FLAG_UTF8.to_le_bytes());
        h.extend_from_slice(&0u16.to_le_bytes()); // gespeichert
        h.extend_from_slice(&time.to_le_bytes());
        h.extend_from_slice(&date.to_le_bytes());
        h.extend_from_slice(&crc.to_le_bytes());
        h.extend_from_slice(&size.to_le_bytes());
        h.extend_from_slice(&size.to_le_bytes());
        h.extend_from_slice(&name_len.to_le_bytes());
        h.extend_from_slice(&0u16.to_le_bytes());
        h.extend_from_slice(&name);
        self.write(&h)?;
        self.write(data)?;
        u32::try_from(self.pos).map_err(|_| too_big())?;
        self.entries.push(Entry {
            name,
            crc,
            size,
            offset,
            dir,
            time,
            date,
        });
        Ok(())
    }

    /// Inhaltsverzeichnis und Abschluss.
    fn finish(mut self) -> std::io::Result<()> {
        let start = u32::try_from(self.pos).map_err(|_| too_big())?;
        let entries = std::mem::take(&mut self.entries);
        for e in &entries {
            let mut c = Vec::with_capacity(46 + e.name.len());
            c.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
            c.extend_from_slice(&VERSION.to_le_bytes()); // erstellt mit (MS-DOS-Attribute)
            c.extend_from_slice(&VERSION.to_le_bytes()); // nötig zum Entpacken
            c.extend_from_slice(&FLAG_UTF8.to_le_bytes());
            c.extend_from_slice(&0u16.to_le_bytes());
            c.extend_from_slice(&e.time.to_le_bytes());
            c.extend_from_slice(&e.date.to_le_bytes());
            c.extend_from_slice(&e.crc.to_le_bytes());
            c.extend_from_slice(&e.size.to_le_bytes());
            c.extend_from_slice(&e.size.to_le_bytes());
            c.extend_from_slice(&(e.name.len() as u16).to_le_bytes());
            c.extend_from_slice(&0u16.to_le_bytes()); // Zusatzfeld
            c.extend_from_slice(&0u16.to_le_bytes()); // Kommentar
            c.extend_from_slice(&0u16.to_le_bytes()); // Datenträger
            c.extend_from_slice(&0u16.to_le_bytes()); // interne Attribute
            let attr: u32 = if e.dir { 0x10 } else { 0x20 };
            c.extend_from_slice(&attr.to_le_bytes());
            c.extend_from_slice(&e.offset.to_le_bytes());
            c.extend_from_slice(&e.name);
            self.write(&c)?;
        }
        let size = u32::try_from(self.pos - u64::from(start)).map_err(|_| too_big())?;
        let count = entries.len() as u16;
        let mut end = Vec::with_capacity(22);
        end.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
        end.extend_from_slice(&0u16.to_le_bytes());
        end.extend_from_slice(&0u16.to_le_bytes());
        end.extend_from_slice(&count.to_le_bytes());
        end.extend_from_slice(&count.to_le_bytes());
        end.extend_from_slice(&size.to_le_bytes());
        end.extend_from_slice(&start.to_le_bytes());
        end.extend_from_slice(&0u16.to_le_bytes());
        self.write(&end)?;
        self.out.flush()?;
        self.out.get_ref().sync_all()
    }
}

/// Änderungszeit als MS-DOS-Zeit und -Datum (Ortszeit, 2-Sekunden-Schritte); unbekannt oder
/// vor 1980: 1.1.1980.
fn dos_time(t: Option<std::time::SystemTime>) -> (u16, u16) {
    use chrono::{Datelike, Timelike};
    let Some(t) = t else {
        return (0, (1 << 5) | 1);
    };
    let local: chrono::DateTime<chrono::Local> = t.into();
    let year = local.year();
    if !(1980..=2107).contains(&year) {
        return (0, (1 << 5) | 1);
    }
    let time = ((local.hour() as u16) << 11)
        | ((local.minute() as u16) << 5)
        | ((local.second() as u16) / 2);
    let date = (((year - 1980) as u16) << 9) | ((local.month() as u16) << 5) | local.day() as u16;
    (time, date)
}

/// Liegt im Projektordner mindestens eine Datei (ohne `.autosave.json`, auch in Unterordnern)?
/// Sonst bekäme die ZIP nur leere Ordner-Einträge, und die Seite lehnt ein leeres Projekt ab.
pub fn has_files(project: &Path) -> bool {
    let Ok(entries) = std::fs::read_dir(project) else {
        return false;
    };
    entries.flatten().any(|e| match e.file_type() {
        Ok(k) if k.is_dir() => has_files(&e.path()),
        Ok(k) if k.is_file() => e.file_name().to_string_lossy() != AUTOSAVE_FILE,
        _ => false,
    })
}
