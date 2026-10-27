//! Download und Prüfung des Installers ohne Oberfläche: herunterladen ([`Downloader`],
//! Kopierschleife [`copy_limited`] mit Größen-, Stillstands- und Abbruchgrenze) und die
//! Prüfsumme prüfen ([`verify_sha256`], [`file_matches_sha256`]).
//!
//! Installer und Prüfsummen-Datei liegen in einem eigenen Ordner ([`DownloadPaths`], angelegt mit
//! [`prepare_dir`], verworfen mit [`discard_download`]). Ausgetauscht wird hier nichts: die
//! Installation übernimmt der Installer, den der Aufrufer erst startet, wenn die Prüfsumme
//! stimmt. Nur [`HttpDownloader`] geht ins Netz; Tests benutzen Nachbildungen und Temp-Ordner.

use std::fs::File;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::{Duration, Instant};

use crate::release::{REPO, SETUP_NAME, SHA256_NAME};
use crate::sha256::Sha256;

pub use crate::net::HttpDownloader;

/// Kommen so lange keine Daten, bricht der Download ab.
pub const STALL_TIMEOUT: Duration = Duration::from_secs(30);

/// Größte Datei, die je geladen wird (200 MB), unabhängig von der Angabe im Release.
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
    /// Ordner anlegen, Lesen oder Schreiben gescheitert.
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
                "Die heruntergeladene Datei ist größer als erwartet. Es wurde nichts installiert."
                    .into()
            }
            InstallError::Checksum => {
                "Die heruntergeladene Datei ist beschädigt (die Prüfsumme stimmt nicht). Es wurde nichts installiert."
                    .into()
            }
            InstallError::Io(detail) => format!(
                "Das Update ließ sich nicht vorbereiten ({detail}). Das Programm läuft in der bisherigen Version weiter."
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
    if local_test_url_ok(url) {
        return true;
    }
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
    #[cfg(feature = "update-test-source")]
    if let Some(target) = local_redirect(current, location) {
        return Ok(target);
    }
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

/// Anfang einer Adresse der lokalen Test-Quelle (nur mit dem Schalter `update-test-source`).
#[cfg(feature = "update-test-source")]
pub const LOCAL_TEST_PREFIX: &str = "http://127.0.0.1:";

/// Ist `url` eine Adresse der lokalen Test-Quelle: `http://127.0.0.1:<Port>/<Pfad>` ohne
/// Anmeldedaten, Anfrageteil und `..`? Ohne den Schalter `update-test-source` nie.
pub fn local_test_url_ok(url: &str) -> bool {
    #[cfg(feature = "update-test-source")]
    {
        url.strip_prefix(LOCAL_TEST_PREFIX)
            .and_then(|rest| rest.split_once('/'))
            .is_some_and(|(port, path)| {
                !port.is_empty()
                    && port.bytes().all(|b| b.is_ascii_digit())
                    && !path.is_empty()
                    && !path.contains("..")
                    && !path.contains(['?', '#', '\\', '@'])
            })
    }
    #[cfg(not(feature = "update-test-source"))]
    {
        let _ = url;
        false
    }
}

/// Weiterleitung innerhalb der lokalen Test-Quelle (absolut oder `/pfad` von dort aus).
#[cfg(feature = "update-test-source")]
fn local_redirect(current: &str, location: &str) -> Option<String> {
    if local_test_url_ok(location) {
        return Some(location.to_string());
    }
    if location.starts_with('/') && !location.starts_with("//") && local_test_url_ok(current) {
        let rest = current.strip_prefix(LOCAL_TEST_PREFIX)?;
        let port = rest.split_once('/')?.0;
        let target = format!("{LOCAL_TEST_PREFIX}{port}{location}");
        return local_test_url_ok(&target).then_some(target);
    }
    None
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
    let expected = checksum_hex(checksum_file_contents, expected_name)?;
    file_matches_sha256(file, &expected)
}

/// Stimmt die Prüfsumme von `file` mit `expected_hex` überein (Groß-/Kleinschreibung egal)?
/// Sonst [`InstallError::Checksum`].
pub fn file_matches_sha256(file: &Path, expected_hex: &str) -> Result<(), InstallError> {
    let mut f = File::open(file).map_err(|e| io_err("Datei lesen", &e))?;
    reader_matches_sha256(&mut f, expected_hex)
}

/// Die erwartete Prüfsumme (64 Hexziffern, klein) aus dem Inhalt der Prüfsummen-Datei, nach
/// denselben Regeln wie [`verify_sha256`].
pub fn checksum_hex(contents: &str, expected_name: &str) -> Result<String, InstallError> {
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

/// Stimmt die Prüfsumme dessen, was `reader` bis zum Ende liefert, mit `expected_hex` überein
/// (Groß-/Kleinschreibung egal)? Sonst [`InstallError::Checksum`]. Für die zweite Prüfung über
/// einen gehaltenen, gesperrten Griff auf den Installer.
pub fn reader_matches_sha256(
    reader: &mut dyn Read,
    expected_hex: &str,
) -> Result<(), InstallError> {
    let actual = reader_sha256(reader)?;
    if actual.eq_ignore_ascii_case(expected_hex) {
        Ok(())
    } else {
        Err(InstallError::Checksum)
    }
}

fn reader_sha256(f: &mut dyn Read) -> Result<String, InstallError> {
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

/// Dateien eines Downloads in einem eigenen Ordner.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadPaths {
    /// Der Ordner des Downloads.
    pub dir: PathBuf,
    /// Der Installer: `dir/`[`SETUP_NAME`].
    pub setup: PathBuf,
    /// Die Prüfsummen-Datei: `dir/`[`SHA256_NAME`].
    pub checksum: PathBuf,
}

impl DownloadPaths {
    /// Die Pfade im Ordner `dir`.
    pub fn in_dir(dir: &Path) -> DownloadPaths {
        DownloadPaths {
            dir: dir.to_path_buf(),
            setup: dir.join(SETUP_NAME),
            checksum: dir.join(SHA256_NAME),
        }
    }
}

/// Legt den Ordner des Downloads an (auch übergeordnete Ordner).
pub fn prepare_dir(paths: &DownloadPaths) -> Result<(), InstallError> {
    std::fs::create_dir_all(&paths.dir).map_err(|e| io_err("Ordner anlegen", &e))
}

/// Verwirft einen Download: Installer und Prüfsummen-Datei, danach den Ordner, aber nur, wenn
/// er leer ist. Fehler werden übergangen.
pub fn discard_download(paths: &DownloadPaths) {
    let _ = std::fs::remove_file(&paths.setup);
    let _ = std::fs::remove_file(&paths.checksum);
    // `remove_dir` löscht nur leere Ordner; fremde Dateien bleiben samt Ordner.
    let _ = std::fs::remove_dir(&paths.dir);
}
