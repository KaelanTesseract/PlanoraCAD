//! Dateisystem-Hilfen: Namen prüfen, atomar schreiben, Wurzel mit Pfadprüfung, Papierkorb, Sperre.

use crate::error::{ProjectError, Result};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Längster erlaubter Name eines Projekts, Ordners oder Dokuments (in Zeichen).
pub const MAX_NAME_CHARS: usize = 80;
/// Versuche beim Umbenennen (Virenscanner und Indexdienste halten Dateien kurz offen).
const RENAME_TRIES: u32 = 5;
const RENAME_WAIT: Duration = Duration::from_millis(40);
/// Name des Papierkorbs unter der Wurzel.
pub const TRASH: &str = ".papierkorb";
const LOCK_FILE: &str = ".sperre";
/// Endung eines Dokumentordners; gehört beim Prüfen nicht zum Namen.
const DOC_SUFFIX: &str = ".pcad";
/// Teil der Id (Zeichen 8 bis 24: Zeit und Prozess unten, Zähler oben), der in den
/// Papierkorbnamen eingeht und ihn je Aufruf eindeutig macht.
const TRASH_ID_RANGE: std::ops::Range<usize> = 8..24;

/// Von Windows reservierte Gerätenamen (auch mit Endung und mit hochgestellten Ziffern).
const RESERVED: [&str; 32] = [
    "CON",
    "CONIN$",
    "CONOUT$",
    "PRN",
    "AUX",
    "NUL",
    "COM1",
    "COM2",
    "COM3",
    "COM4",
    "COM5",
    "COM6",
    "COM7",
    "COM8",
    "COM9",
    "COM0",
    "COM\u{b9}",
    "COM\u{b2}",
    "COM\u{b3}",
    "LPT1",
    "LPT2",
    "LPT3",
    "LPT4",
    "LPT5",
    "LPT6",
    "LPT7",
    "LPT8",
    "LPT9",
    "LPT0",
    "LPT\u{b9}",
    "LPT\u{b2}",
    "LPT\u{b3}",
];

fn invalid(msg: &str) -> ProjectError {
    ProjectError::InvalidName(msg.to_string())
}

/// Prüft einen Namen und gibt ihn getrimmt zurück.
pub fn validate_name(name: &str) -> Result<String> {
    let n = name.trim();
    if n.is_empty() {
        return Err(invalid("Der Name darf nicht leer sein."));
    }
    if n.chars().count() > MAX_NAME_CHARS {
        return Err(invalid("Der Name ist zu lang (höchstens 80 Zeichen)."));
    }
    if n.starts_with('.') || n.ends_with('.') {
        return Err(invalid(
            "Der Name darf nicht mit einem Punkt beginnen oder enden.",
        ));
    }
    if n.chars()
        .any(|c| c.is_control() || "/\\:*?\"<>|".contains(c))
    {
        return Err(invalid(
            "Der Name enthält ein unzulässiges Zeichen (/ \\ : * ? \" < > |).",
        ));
    }
    // Windows ignoriert Leerzeichen und Punkte am Ende des Namensteils vor der Endung.
    let stem = n.split('.').next().unwrap_or(n).trim_end().to_uppercase();
    if RESERVED.contains(&stem.as_str()) {
        return Err(invalid("Dieser Name ist vom Betriebssystem reserviert."));
    }
    Ok(n.to_string())
}

/// Schreibt `bytes` atomar: erst in `<Name>.tmp` im selben Ordner, `sync_all`, dann umbenennen.
pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    write_via(&PathBuf::from(tmp), path, bytes)
}

/// Wie [`atomic_write`], aber mit eigener Zwischendatei je Schreibvorgang
/// (`<Name>.<Prozess>-<Zähler>.tmp`): Schreiben zwei Programme gleichzeitig dieselbe Datei
/// (Konfiguration), vermischen sich ihre Zwischendateien nicht.
pub fn atomic_write_unique(path: &Path, bytes: &[u8]) -> Result<()> {
    write_via(&unique_tmp(path), path, bytes)
}

/// Eigener Name der Zwischendatei je Aufruf: `<Name>.<Prozess>-<Zähler>.tmp`.
fn unique_tmp(path: &Path) -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(format!(".{}-{n}.tmp", std::process::id()));
    PathBuf::from(tmp)
}

fn write_via(tmp: &Path, path: &Path, bytes: &[u8]) -> Result<()> {
    let tmp = tmp.to_path_buf();
    let written = (|| -> std::io::Result<()> {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()
    })();
    if let Err(e) = written {
        let _ = fs::remove_file(&tmp);
        return Err(ProjectError::io(&tmp, &e));
    }
    let renamed = rename_with_retries(&tmp, path);
    if renamed.is_err() {
        // Eine halb fertige Zwischendatei bleibt nicht liegen.
        let _ = fs::remove_file(&tmp);
    }
    renamed
}

/// Benennt um (ersetzt eine vorhandene Datei) und versucht es kurz mehrfach.
fn rename_with_retries(from: &Path, to: &Path) -> Result<()> {
    let mut last = None;
    for _ in 0..RENAME_TRIES {
        match fs::rename(from, to) {
            Ok(()) => return Ok(()),
            Err(e) => {
                last = Some(e);
                std::thread::sleep(RENAME_WAIT);
            }
        }
    }
    let err = last.unwrap_or_else(|| std::io::Error::other("Umbenennen fehlgeschlagen"));
    Err(ProjectError::io(to, &err))
}

static COUNTER: AtomicU64 = AtomicU64::new(0);

/// 32 Hexzeichen aus Zeit, Prozess-Id, Zähler und Adresse eines Stapelwerts (keine Sicherheits-
/// funktion, nur Eindeutigkeit).
pub fn new_id() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let marker = 0u8;
    let addr = &marker as *const u8 as usize as u64;
    let hi = (nanos as u64) ^ (u64::from(std::process::id()) << 32);
    let lo = (n << 32) ^ (addr & 0xffff_ffff) ^ ((nanos >> 64) as u64);
    format!("{hi:016x}{lo:016x}")
}

/// Aktuelle Zeit (UTC) als „JJJJ-MM-TTThh:mm:ssZ“.
pub fn now_iso() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0) as i64;
    iso_from_unix(secs)
}

/// Bürgerliches Datum aus Tagen seit 1970-01-01 (Algorithmus von Howard Hinnant).
fn iso_from_unix(secs: i64) -> String {
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

/// Die Wurzel aller Projekte. Jeder Pfad, der gelesen oder geschrieben wird, geht durch
/// [`Root::resolve`].
#[derive(Clone, Debug)]
pub struct Root {
    path: PathBuf,
}

impl Root {
    /// Öffnet die Wurzel (legt sie an, falls sie fehlt) und merkt ihren kanonischen Pfad.
    pub fn open(path: &Path) -> Result<Root> {
        fs::create_dir_all(path).map_err(|e| ProjectError::io(path, &e))?;
        let path = fs::canonicalize(path).map_err(|e| ProjectError::io(path, &e))?;
        Ok(Root { path })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Pfad zu `parts` (je Teil ein gültiger Name). Verweist ein vorhandener Teil des Pfades
    /// (auch über Symlink oder Junction) aus der Wurzel hinaus, ist das ein Fehler.
    pub fn resolve(&self, parts: &[String]) -> Result<PathBuf> {
        let mut p = self.path.clone();
        for part in parts {
            // Für Dokumente ist die Endung „.pcad“ Teil des Namens; geprüft wird der Teil ohne
            // diese Endung. Der Teil selbst muss unverändert (ungetrimmt) gültig sein, sonst
            // würde der Dateiname vom geprüften Namen abweichen.
            let name = part.strip_suffix(DOC_SUFFIX).unwrap_or(part);
            if validate_name(name)? != name {
                return Err(invalid(
                    "Der Name darf nicht mit einem Leerzeichen beginnen oder enden.",
                ));
            }
            p.push(part);
        }
        // Der längste vorhandene Vorfahre (auch ein kaputter Link zählt als vorhanden und
        // scheitert dann beim Auflösen) muss innerhalb der Wurzel liegen.
        let mut probe = p.clone();
        while fs::symlink_metadata(&probe).is_err() {
            if !probe.pop() {
                return Err(ProjectError::OutsideRoot);
            }
        }
        let real = fs::canonicalize(&probe).map_err(|e| ProjectError::io(&probe, &e))?;
        if !real.starts_with(&self.path) {
            return Err(ProjectError::OutsideRoot);
        }
        Ok(p)
    }

    /// Verschiebt `parts` in den Papierkorb unter der Wurzel (Name mit Zeitstempel und Id, damit
    /// nichts kollidiert). Der Papierkorb selbst lässt sich nicht löschen.
    pub fn to_trash(&self, parts: &[String]) -> Result<()> {
        let Some(last) = parts.last() else {
            return Err(ProjectError::OutsideRoot);
        };
        if parts[0] == TRASH {
            return Err(ProjectError::OutsideRoot);
        }
        let from = self.resolve(parts)?;
        if fs::symlink_metadata(&from).is_err() {
            return Err(ProjectError::NotFound(parts.join("/")));
        }
        let trash = self.path.join(TRASH);
        fs::create_dir_all(&trash).map_err(|e| ProjectError::io(&trash, &e))?;
        let stamp = now_iso().replace(':', "-");
        let name = format!("{stamp}-{}-{last}", &new_id()[TRASH_ID_RANGE]);
        rename_with_retries(&from, &trash.join(name))
    }
}

/// Sperre eines Dokumentordners; wird beim Drop freigegeben.
#[derive(Debug)]
pub struct DocLock {
    file: PathBuf,
    token: String,
}

impl DocLock {
    pub fn acquire(dir: &Path) -> Result<DocLock> {
        let file = dir.join(LOCK_FILE);
        let token = new_id();
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&file)
        {
            Ok(mut f) => {
                let _ = writeln!(f, "{token}\n{}\n{}", now_iso(), std::process::id());
                Ok(DocLock { file, token })
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Err(ProjectError::Locked {
                since: lock_age(dir).unwrap_or_else(|| "unbekannt".into()),
            }),
            Err(e) => Err(ProjectError::io(&file, &e)),
        }
    }

    /// Übernimmt eine vorhandene Sperre (nach Rückfrage des Nutzers).
    pub fn take_over(dir: &Path) -> Result<DocLock> {
        let file = dir.join(LOCK_FILE);
        let _ = fs::remove_file(&file);
        DocLock::acquire(dir)
    }

    /// Gehört die Sperre noch uns? Falsch, wenn ein anderes Programm sie übernommen hat (oder
    /// die Sperrdatei fehlt oder unlesbar ist).
    pub fn still_held(&self) -> bool {
        fs::read_to_string(&self.file)
            .map(|text| text.lines().next() == Some(self.token.as_str()))
            .unwrap_or(false)
    }
}

impl Drop for DocLock {
    fn drop(&mut self) {
        // Nur löschen, was uns gehört (eine übernommene Sperre trägt ein anderes Zeichen).
        if let Ok(text) = fs::read_to_string(&self.file)
            && text.lines().next() == Some(self.token.as_str())
        {
            let _ = fs::remove_file(&self.file);
        }
    }
}

/// Zeitstempel der vorhandenen Sperre (zweite Zeile der Sperrdatei).
pub fn lock_age(dir: &Path) -> Option<String> {
    let text = fs::read_to_string(dir.join(LOCK_FILE)).ok()?;
    text.lines().nth(1).map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_failed_rename_leaves_the_source_untouched() {
        let d = std::env::temp_dir().join(format!("planora-fsutil-{}", new_id()));
        fs::create_dir_all(d.join("ziel")).unwrap();
        fs::write(d.join("ziel").join("x"), b"x").unwrap();
        let src = d.join("quelle.txt");
        fs::write(&src, b"daten").unwrap();
        // Eine Datei lässt sich nicht über einen nicht leeren Ordner umbenennen.
        assert!(rename_with_retries(&src, &d.join("ziel")).is_err());
        assert_eq!(fs::read(&src).unwrap(), b"daten");
        // atomic_write räumt seine eigene Zwischendatei dagegen weg.
        assert!(atomic_write(&d.join("ziel"), b"neu").is_err());
        assert!(!d.join("ziel.tmp").exists());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn unique_writes_use_their_own_temp_file() {
        let p = Path::new("config.json");
        let (a, b) = (unique_tmp(p), unique_tmp(p));
        assert_ne!(a, b, "jeder Schreibvorgang eine eigene Zwischendatei");
        for t in [&a, &b] {
            let name = t.to_string_lossy();
            assert!(
                name.starts_with("config.json.") && name.ends_with(".tmp"),
                "{name}"
            );
        }
        let d = std::env::temp_dir().join(format!("planora-fsutil-u-{}", new_id()));
        fs::create_dir_all(&d).unwrap();
        atomic_write_unique(&d.join("c.json"), b"1").unwrap();
        atomic_write_unique(&d.join("c.json"), b"2").unwrap();
        assert_eq!(fs::read(d.join("c.json")).unwrap(), b"2");
        assert_eq!(fs::read_dir(&d).unwrap().count(), 1, "keine Reste");
        let _ = fs::remove_dir_all(&d);
    }
}
