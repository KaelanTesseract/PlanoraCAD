//! Update einspielen ohne Oberfläche: herunterladen ([`Downloader`], Kopierschleife
//! [`copy_limited`] mit Größen-, Stillstands- und Abbruchgrenze), die Prüfsumme prüfen
//! ([`verify_sha256`]), die Programmdatei austauschen ([`swap`]) und zurücklegen ([`rollback`]).
//!
//! Alle Dateien liegen neben der laufenden exe ([`Paths`]); der Download geht nur nach
//! `<exe>.new`. Ausgetauscht wird erst, wenn die Prüfsumme stimmt (das prüft der Aufrufer mit
//! [`verify_sha256`] vor [`swap`]). Nur [`HttpDownloader`] geht ins Netz; Tests benutzen
//! Nachbildungen und Temp-Ordner.

use std::fs::File;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::{Duration, Instant};

use crate::release::REPO;
use crate::sha256::Sha256;

pub use crate::net::HttpDownloader;

/// Name der Datei, die eine neu gestartete Version neben die exe legt, sobald sie läuft.
pub const OK_MARKER_NAME: &str = "planoracad.update-ok";

/// Kommen so lange keine Daten, bricht der Download ab.
pub const STALL_TIMEOUT: Duration = Duration::from_secs(30);

/// Größte Programmdatei, die je geladen wird (200 MB), unabhängig von der Angabe im Release.
pub const MAX_DOWNLOAD_BYTES: u64 = 200 * 1024 * 1024;

/// So viel darf eine Datei größer sein als im Release angegeben.
pub const SIZE_SLACK: u64 = 64 * 1024;

/// Größte angenommene Prüfsummen-Datei.
pub const MAX_CHECKSUM_BYTES: u64 = 4096;

/// Höchstens so viele Weiterleitungen folgt ein Download (GitHub leitet einmal um).
pub const MAX_REDIRECTS: usize = 5;

/// So oft meldet sich die Kopierschleife mindestens beim Rückruf, auch ohne neue Daten (damit
/// ein Abbruch rasch wirkt).
const TICK: Duration = Duration::from_millis(100);

/// Fehler beim Einspielen eines Updates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstallError {
    /// Vom Nutzer abgebrochen.
    Cancelled,
    /// Download gescheitert (kein Netz, Zeitüberschreitung, Stillstand, Statuscode, Adresse).
    Network(String),
    /// Die Datei ist größer als erlaubt.
    TooLarge,
    /// Die Prüfsumme passt nicht oder die Prüfsummen-Datei ist unlesbar.
    Checksum,
    /// Der Ordner der exe ist nicht beschreibbar.
    NotWritable,
    /// Lesen, Schreiben, Umbenennen gescheitert.
    Io(String),
}

impl InstallError {
    /// Meldung für den Nutzer in einfacher Sprache.
    pub fn message(&self) -> String {
        match self {
            InstallError::Cancelled => "Das Update wurde abgebrochen.".into(),
            InstallError::Network(detail) => format!(
                "Die neue Version ließ sich nicht herunterladen ({detail}). Bitte später noch einmal versuchen."
            ),
            InstallError::TooLarge => {
                "Die heruntergeladene Datei ist größer als erwartet. Es wurde nichts ausgetauscht."
                    .into()
            }
            InstallError::Checksum => {
                "Die heruntergeladene Datei ist beschädigt (die Prüfsumme stimmt nicht). Es wurde nichts ausgetauscht."
                    .into()
            }
            InstallError::NotWritable => {
                "Das Programm liegt in einem geschützten Ordner. Lade die neue Version von GitHub herunter."
                    .into()
            }
            InstallError::Io(detail) => format!(
                "Das Update ließ sich nicht einspielen ({detail}). Das Programm läuft in der bisherigen Version weiter."
            ),
        }
    }
}

impl std::fmt::Display for InstallError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message())
    }
}

impl std::error::Error for InstallError {}

fn io_err(what: &str, e: &io::Error) -> InstallError {
    InstallError::Io(format!("{what}: {e}"))
}

/// Lädt eine Datei des Releases. `progress(geladen, gesamt)` kommt mindestens alle 100 ms
/// (`gesamt` 0 = unbekannt); liefert er `false`, bricht der Download ab
/// ([`InstallError::Cancelled`]). Mehr als `max_bytes` → [`InstallError::TooLarge`]. Bei jedem
/// Fehler bleibt keine Datei `dest` zurück.
pub trait Downloader {
    fn download(
        &self,
        url: &str,
        dest: &Path,
        max_bytes: u64,
        progress: &mut dyn FnMut(u64, u64) -> bool,
    ) -> Result<(), InstallError>;
}

/// Obergrenze für den Download einer Datei mit `asset_size` Bytes laut Release: die Größe plus
/// [`SIZE_SLACK`], höchstens [`MAX_DOWNLOAD_BYTES`]; ist schon die Angabe größer, wird gar
/// nicht geladen.
pub fn download_limit(asset_size: u64) -> Result<u64, InstallError> {
    if asset_size > MAX_DOWNLOAD_BYTES {
        return Err(InstallError::TooLarge);
    }
    Ok(asset_size
        .saturating_add(SIZE_SLACK)
        .min(MAX_DOWNLOAD_BYTES))
}

/// Ist `url` eine Download-Adresse eines Releases von [`REPO`] (wie sie
/// [`crate::parse_releases`] liefert)? Ohne Anfrageteil, ohne `..`.
pub fn release_url_ok(url: &str) -> bool {
    let prefix = format!("https://github.com/{REPO}/releases/download/");
    url.strip_prefix(&prefix).is_some_and(|rest| {
        !rest.is_empty()
            && !rest.contains("..")
            && !rest.contains(['?', '#', '\\', '@'])
            && rest.split('/').count() == 2
    })
}

/// Ziel einer Weiterleitung von `current` nach `location` (Kopfzeile `Location`). Erlaubt sind
/// nur HTTPS-Adressen auf `github.com` und `*.githubusercontent.com` (dort liegen die Dateien
/// der Releases), ohne Anmeldedaten und ohne anderen Port als 443; relative Angaben (`/pfad`,
/// `//host/pfad`) werden gegen `current` aufgelöst. Alles andere ist ein Netzfehler.
pub fn redirect_target(current: &str, location: &str) -> Result<String, InstallError> {
    let location = location.trim();
    let target = if location.starts_with("https://") {
        location.to_string()
    } else if let Some(rest) = location.strip_prefix("//") {
        format!("https://{rest}")
    } else if location.starts_with('/') {
        let (origin, _) = split_origin(current)
            .ok_or_else(|| InstallError::Network("Weiterleitung nicht erlaubt".into()))?;
        format!("{origin}{location}")
    } else {
        return Err(InstallError::Network("Weiterleitung nicht erlaubt".into()));
    };
    match split_origin(&target) {
        Some((_, host)) if host_allowed(&host) => Ok(target),
        _ => Err(InstallError::Network(
            "Weiterleitung zu einer fremden Adresse".into(),
        )),
    }
}

/// `("https://authority", host)` einer HTTPS-Adresse; `None` mit Anmeldedaten oder anderem
/// Port als 443.
fn split_origin(url: &str) -> Option<(String, String)> {
    let rest = url.strip_prefix("https://")?;
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let authority = &rest[..end];
    if authority.contains('@') {
        return None;
    }
    let host = match authority.split_once(':') {
        Some((host, "443")) => host,
        Some(_) => return None,
        None => authority,
    };
    Some((format!("https://{authority}"), host.to_ascii_lowercase()))
}

fn host_allowed(host: &str) -> bool {
    const SUFFIX: &str = ".githubusercontent.com";
    let valid = !host.is_empty()
        && host
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'.' || b == b'-');
    let sub_ok = host
        .strip_suffix(SUFFIX)
        .is_some_and(|sub| !sub.is_empty() && !sub.starts_with('.') && !sub.ends_with('.'));
    valid && (host == "github.com" || sub_ok)
}

/// Kopiert `reader` nach `dest` (neu angelegt): höchstens `max_bytes`, Abbruch über `progress`
/// (Rückruf mindestens alle 100 ms, `total` wird durchgereicht), Fehler, wenn `stall` lang keine
/// Daten kommen. Gelesen wird in einem eigenen Thread, damit auch ein hängender Leser Abbruch
/// und Stillstand nicht aufhält (er endet dann mit seiner eigenen Zeitgrenze). Bei jedem Fehler
/// wird `dest` gelöscht.
pub fn copy_limited(
    reader: impl Read + Send + 'static,
    total: u64,
    dest: &Path,
    max_bytes: u64,
    stall: Duration,
    progress: &mut dyn FnMut(u64, u64) -> bool,
) -> Result<(), InstallError> {
    let mut file = File::create(dest).map_err(|e| io_err("Datei anlegen", &e))?;
    let result = copy_into(reader, total, &mut file, max_bytes, stall, progress);
    drop(file);
    if result.is_err() {
        let _ = std::fs::remove_file(dest);
    }
    result
}

fn copy_into(
    reader: impl Read + Send + 'static,
    total: u64,
    file: &mut File,
    max_bytes: u64,
    stall: Duration,
    progress: &mut dyn FnMut(u64, u64) -> bool,
) -> Result<(), InstallError> {
    let (tx, rx) = mpsc::sync_channel::<io::Result<Vec<u8>>>(8);
    std::thread::Builder::new()
        .name("planora-update-lesen".into())
        .spawn(move || {
            let mut reader = reader;
            let mut buf = vec![0u8; 64 * 1024];
            loop {
                let msg = match reader.read(&mut buf) {
                    Ok(n) => Ok(buf[..n].to_vec()),
                    Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                    Err(e) => Err(e),
                };
                let end = !matches!(&msg, Ok(chunk) if !chunk.is_empty());
                if tx.send(msg).is_err() || end {
                    return;
                }
            }
        })
        .map_err(|e| io_err("Thread", &e))?;

    let mut done = 0u64;
    let mut last_data = Instant::now();
    if !progress(done, total) {
        return Err(InstallError::Cancelled);
    }
    loop {
        match rx.recv_timeout(TICK) {
            Ok(Ok(chunk)) if chunk.is_empty() => break,
            Ok(Ok(chunk)) => {
                done += chunk.len() as u64;
                if done > max_bytes {
                    return Err(InstallError::TooLarge);
                }
                file.write_all(&chunk)
                    .map_err(|e| io_err("Datei schreiben", &e))?;
                last_data = Instant::now();
            }
            Ok(Err(e)) => return Err(InstallError::Network(e.to_string())),
            Err(RecvTimeoutError::Timeout) => {
                if last_data.elapsed() >= stall {
                    return Err(InstallError::Network(format!(
                        "seit {} Sekunden kommen keine Daten",
                        stall.as_secs().max(1)
                    )));
                }
            }
            Err(RecvTimeoutError::Disconnected) => {
                return Err(InstallError::Network("Verbindung abgebrochen".into()));
            }
        }
        if !progress(done, total) {
            return Err(InstallError::Cancelled);
        }
    }
    file.sync_all().map_err(|e| io_err("Datei schreiben", &e))?;
    Ok(())
}

/// Prüft `file` gegen den Inhalt der Prüfsummen-Datei: genau eine Zeile `<hex>  <name>`,
/// `<hex> *<name>` oder nur `<hex>` (64 Hexziffern, Groß-/Kleinschreibung egal); steht ein Name
/// dabei, muss er `expected_name` sein. Alles andere ist [`InstallError::Checksum`].
pub fn verify_sha256(
    file: &Path,
    checksum_file_contents: &str,
    expected_name: &str,
) -> Result<(), InstallError> {
    let expected = parse_checksum(checksum_file_contents, expected_name)?;
    let actual = file_sha256(file)?;
    if actual.eq_ignore_ascii_case(&expected) {
        Ok(())
    } else {
        Err(InstallError::Checksum)
    }
}

fn parse_checksum(contents: &str, expected_name: &str) -> Result<String, InstallError> {
    let mut lines = contents.lines().map(str::trim).filter(|l| !l.is_empty());
    let line = lines.next().ok_or(InstallError::Checksum)?;
    if lines.next().is_some() {
        return Err(InstallError::Checksum);
    }
    let (hex, name) = match line.split_once(char::is_whitespace) {
        Some((hex, rest)) => {
            let rest = rest.trim_start();
            (hex, Some(rest.strip_prefix('*').unwrap_or(rest)))
        }
        None => (line, None),
    };
    if hex.len() != 64 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(InstallError::Checksum);
    }
    if name.is_some_and(|n| n != expected_name) {
        return Err(InstallError::Checksum);
    }
    Ok(hex.to_ascii_lowercase())
}

fn file_sha256(file: &Path) -> Result<String, InstallError> {
    let mut f = File::open(file).map_err(|e| io_err("Datei lesen", &e))?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        match f.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => h.update(&buf[..n]),
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(io_err("Datei lesen", &e)),
        }
    }
    Ok(h.hex())
}

/// Dateien eines Updates neben der laufenden exe.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Paths {
    /// Die laufende Programmdatei.
    pub running_exe: PathBuf,
    /// Download: `<exe>.new`.
    pub new_exe: PathBuf,
    /// Sicherung der bisherigen Datei: `<exe>.old`.
    pub old_exe: PathBuf,
    /// [`OK_MARKER_NAME`] im Ordner der exe.
    pub ok_marker: PathBuf,
}

impl Paths {
    /// Die Pfade zur Programmdatei `exe`.
    pub fn for_exe(exe: &Path) -> Paths {
        let with = |ext: &str| {
            let mut s = exe.as_os_str().to_owned();
            s.push(ext);
            PathBuf::from(s)
        };
        let dir = exe.parent().unwrap_or(Path::new("."));
        Paths {
            running_exe: exe.to_path_buf(),
            new_exe: with(".new"),
            old_exe: with(".old"),
            ok_marker: dir.join(OK_MARKER_NAME),
        }
    }

    /// Geladene Prüfsummen-Datei: `<exe>.new.sha256` (nur kurz, bis sie gelesen ist).
    pub fn checksum_file(&self) -> PathBuf {
        let mut s = self.new_exe.as_os_str().to_owned();
        s.push(".sha256");
        PathBuf::from(s)
    }

    /// Ordner der exe.
    pub fn dir(&self) -> &Path {
        self.running_exe.parent().unwrap_or(Path::new("."))
    }
}

/// Ergebnis eines gelungenen Austauschs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SwapState {
    /// Ein `.old` eines früheren Updates lag noch da und wurde vorher gelöscht.
    pub stale_old_removed: bool,
}

/// Tauscht aus: die laufende exe wird zu `.old` (Windows erlaubt das Umbenennen einer laufenden
/// Datei), dann `.new` an ihren Platz. Scheitert der zweite Schritt, wird `.old` zurückbenannt;
/// danach ist alles wie vorher. Ein `.old` aus einem früheren Update wird vorher gelöscht; geht
/// das nicht, wird nichts verändert.
pub fn swap(paths: &Paths) -> Result<SwapState, InstallError> {
    let stale_old_removed = paths.old_exe.exists();
    if stale_old_removed {
        std::fs::remove_file(&paths.old_exe).map_err(|e| io_err("alte Sicherung löschen", &e))?;
    }
    std::fs::rename(&paths.running_exe, &paths.old_exe)
        .map_err(|e| io_err("Programmdatei beiseitelegen", &e))?;
    if let Err(e) = std::fs::rename(&paths.new_exe, &paths.running_exe) {
        let detail = match std::fs::rename(&paths.old_exe, &paths.running_exe) {
            Ok(()) => format!("neue Programmdatei einsetzen: {e}"),
            Err(back) => format!(
                "neue Programmdatei einsetzen: {e}; Zurücklegen gescheitert: {back} (die bisherige Datei liegt als {})",
                paths.old_exe.display()
            ),
        };
        return Err(InstallError::Io(detail));
    }
    Ok(SwapState { stale_old_removed })
}

/// Legt den alten Stand zurück: die neue exe wird wieder zu `.new` (und dann, wenn möglich,
/// gelöscht), `.old` kommt an ihren Platz. Ohne `.old` geschieht nichts (Fehler).
pub fn rollback(paths: &Paths) -> Result<(), InstallError> {
    if !paths.old_exe.is_file() {
        return Err(InstallError::Io(
            "keine Sicherung der bisherigen Version".into(),
        ));
    }
    if paths.running_exe.exists() {
        if paths.new_exe.exists() {
            std::fs::remove_file(&paths.new_exe).map_err(|e| io_err("Download löschen", &e))?;
        }
        std::fs::rename(&paths.running_exe, &paths.new_exe)
            .map_err(|e| io_err("neue Programmdatei beiseitelegen", &e))?;
    }
    if let Err(e) = std::fs::rename(&paths.old_exe, &paths.running_exe) {
        // Die neue Datei wieder einsetzen, damit es überhaupt eine Programmdatei gibt.
        let _ = std::fs::rename(&paths.new_exe, &paths.running_exe);
        return Err(io_err("bisherige Programmdatei zurücklegen", &e));
    }
    // Läuft die neue Version noch, lässt sie sich nicht löschen; dann bleibt sie als `.new`.
    let _ = std::fs::remove_file(&paths.new_exe);
    Ok(())
}

/// Löscht `.old` eines früheren Updates; Fehler (etwa weil die alte Version noch läuft) werden
/// übergangen, beim nächsten Start geht es wieder.
pub fn cleanup_old(paths: &Paths) {
    let _ = std::fs::remove_file(&paths.old_exe);
}

/// Verwirft einen Download (`.new` und die Prüfsummen-Datei); Fehler werden übergangen.
pub fn discard_download(paths: &Paths) {
    let _ = std::fs::remove_file(&paths.new_exe);
    let _ = std::fs::remove_file(paths.checksum_file());
}

/// Lassen sich im Ordner `dir` Dateien anlegen und löschen? Probiert es mit einer kurzen
/// Probedatei aus (ein Ordner unter „Programme“ ist ohne Administratorrechte nicht beschreibbar).
pub fn exe_dir_writable(dir: &Path) -> bool {
    if !dir.is_dir() {
        return false;
    }
    let probe = dir.join(format!(".planoracad-probe-{}", std::process::id()));
    let created = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&probe)
        .and_then(|mut f| f.write_all(b"x"));
    let removed = std::fs::remove_file(&probe);
    created.is_ok() && removed.is_ok()
}
