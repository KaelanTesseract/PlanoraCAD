//! Der eingebettete Inno-Setup-Motor (`planoracad-engine-x64.exe`): auspacken, starten, warten,
//! aufräumen.

use std::fs::File;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Dateiname des ausgepackten Motors.
const ENGINE_FILE: &str = "planoracad-engine-x64.exe";

/// Der Motor als Bytes, wenn er beim Bauen über `PLANORA_SETUP_ENGINE` eingebettet wurde.
#[cfg(has_engine)]
pub fn engine_bytes() -> Option<&'static [u8]> {
    Some(include_bytes!(env!("PLANORA_SETUP_ENGINE_PATH")))
}

/// Ohne `PLANORA_SETUP_ENGINE` beim Bauen gibt es keinen Motor (Entwicklungs-Build).
#[cfg(not(has_engine))]
pub fn engine_bytes() -> Option<&'static [u8]> {
    None
}

/// Neuer, eigener Ordner für den Motor: `%TEMP%\planoracad-setup-<pid>-<ms>` (nur der Pfad; angelegt
/// wird er erst beim Auspacken). So kennt die Oberfläche Fortschritts- und Protokolldatei schon,
/// bevor der Motor-Faden auspackt.
pub fn fresh_dir() -> PathBuf {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    std::env::temp_dir().join(format!("planoracad-setup-{}-{millis}", std::process::id()))
}

/// Der ausgepackte Motor in seinem eigenen Ordner. Bis zum Start hält er die Datei gesperrt
/// (siehe [`lock_verified`]): Niemand kann sie zwischen Prüfung und Start umschreiben, umbenennen
/// oder löschen.
pub struct Unpacked {
    dir: PathBuf,
    exe: PathBuf,
    /// Griff mit Freigabe nur zum Lesen, über den die Bytes geprüft wurden; `None` nach dem Start.
    lock: Option<File>,
}

impl Unpacked {
    /// Packt den Motor in einen eigenen Ordner `%TEMP%\planoracad-setup-<pid>-<ms>` aus.
    /// Ohne eingebetteten Motor: Fehler `NotFound`.
    pub fn unpack() -> io::Result<Unpacked> {
        Unpacked::unpack_into(fresh_dir())
    }

    /// Packt den Motor in den neuen Ordner `dir` aus. Gibt es `dir` schon, scheitert es
    /// (`AlreadyExists`), damit niemand einen vorbereiteten Ordner unterschieben kann. Ohne
    /// eingebetteten Motor: Fehler `NotFound`; scheitert Schreiben oder Prüfung, wird der Ordner
    /// wieder entfernt.
    pub fn unpack_into(dir: PathBuf) -> io::Result<Unpacked> {
        let bytes = engine_bytes()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "kein Motor eingebettet"))?;
        Unpacked::unpack_bytes(dir, bytes)
    }

    /// Schreibt `bytes` als Motor in den neuen Ordner `dir` und sperrt die Datei geprüft.
    fn unpack_bytes(dir: PathBuf, bytes: &[u8]) -> io::Result<Unpacked> {
        // Nur anlegen, nie einen vorhandenen Ordner übernehmen.
        std::fs::create_dir(&dir)?;
        let exe = dir.join(ENGINE_FILE);
        let written = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&exe)
            .and_then(|mut f| f.write_all(bytes));
        match written.and_then(|()| lock_verified(&exe, bytes)) {
            Ok(lock) => Ok(Unpacked {
                dir,
                exe,
                lock: Some(lock),
            }),
            Err(err) => {
                let _ = std::fs::remove_dir_all(&dir);
                Err(err)
            }
        }
    }

    /// Startet den Motor (siehe [`spawn`]) und gibt die Sperre erst danach frei, auch wenn der
    /// Start scheitert.
    pub fn spawn(&mut self, args: &[String], elevate: bool) -> Result<ProcessHandle, RunError> {
        let result = spawn(&self.exe, args, elevate);
        self.lock = None;
        result
    }

    /// Löscht den Ordner (vorher fällt die Sperre). Der Motor kann die Datei nach dem Ende noch
    /// kurz halten, daher einige Versuche mit kurzer Pause; bleibt er danach liegen, wird der
    /// Fehler ignoriert.
    pub fn cleanup(self) {
        let Unpacked { dir, lock, .. } = self;
        drop(lock);
        for _ in 0..10 {
            if std::fs::remove_dir_all(&dir).is_ok() || !dir.exists() {
                return;
            }
            std::thread::sleep(Duration::from_millis(200));
        }
    }
}

/// Öffnet `file` mit Freigabe nur zum Lesen (`FILE_SHARE_READ`) und prüft über genau diesen
/// Griff, dass die Datei aus `expected` besteht (wie `lock_verified` in der Update-Funktion der
/// App). Solange der Griff offen ist, kann niemand die Datei schreiben, umbenennen oder löschen;
/// lesen und starten geht weiter (Windows öffnet ein Programm zum Start nur zum Lesen und
/// Ausführen). Hält schon jemand die Datei zum Schreiben offen, scheitert das Öffnen. Anderswo
/// ist es nur eine offene Datei.
fn lock_verified(file: &Path, expected: &[u8]) -> io::Result<File> {
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.share_mode(windows_sys::Win32::Storage::FileSystem::FILE_SHARE_READ);
    }
    let mut f = options.open(file)?;
    if !reader_matches(&mut f, expected)? {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "unpacked engine differs",
        ));
    }
    Ok(f)
}

/// Liest `reader` bis zum Ende und vergleicht mit `expected` (Länge und Inhalt).
fn reader_matches(reader: &mut impl Read, expected: &[u8]) -> io::Result<bool> {
    let mut actual = Vec::with_capacity(expected.len());
    reader.read_to_end(&mut actual)?;
    Ok(actual == expected)
}

/// Warum der Motor nicht gestartet ist.
#[derive(Debug)]
pub enum RunError {
    /// Die Admin-Abfrage wurde abgelehnt (`ERROR_CANCELLED`).
    #[cfg_attr(
        not(any(windows, test)),
        expect(dead_code, reason = "Admin-Abfrage gibt es nur unter Windows")
    )]
    Declined,
    /// Sonstiger Fehler beim Start.
    Io(
        #[expect(
            dead_code,
            reason = "nur zur Fehlersuche (Debug); die Seite zeigt das Protokoll"
        )]
        io::Error,
    ),
}

/// Startet `exe` mit `args` (Rohtext je Argument, durch einzelne Leerzeichen verbunden).
/// `elevate` fragt über das Verb `runas` nach Admin-Rechten; sonst das Standardverb, wie bei
/// der stillen Weitergabe, bei der der Motor sich für `/ALLUSERS` selbst erhöht.
#[cfg(windows)]
pub fn spawn(exe: &Path, args: &[String], elevate: bool) -> Result<ProcessHandle, RunError> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Foundation::{ERROR_CANCELLED, GetLastError};
    use windows_sys::Win32::UI::Shell::{
        SEE_MASK_FLAG_NO_UI, SEE_MASK_NOASYNC, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW,
        ShellExecuteExW,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    fn wide(s: &std::ffi::OsStr) -> Vec<u16> {
        s.encode_wide().chain(std::iter::once(0)).collect()
    }
    let verb = wide("runas".as_ref());
    let file = wide(exe.as_os_str());
    let params = wide(args.join(" ").as_ref());
    let dir = wide(exe.parent().unwrap_or(Path::new("")).as_os_str());

    // SAFETY: SHELLEXECUTEINFOW ist eine reine C-Struktur; alles Null ist ein gültiger
    // Anfangszustand (keine Fenster, keine Griffe).
    let mut info: SHELLEXECUTEINFOW = unsafe { std::mem::zeroed() };
    info.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
    info.fMask = SEE_MASK_NOCLOSEPROCESS | SEE_MASK_NOASYNC | SEE_MASK_FLAG_NO_UI;
    // Null-Verb = Standardverb, genau wie ein direkter Aufruf des Motors.
    info.lpVerb = if elevate {
        verb.as_ptr()
    } else {
        std::ptr::null()
    };
    info.lpFile = file.as_ptr();
    info.lpParameters = params.as_ptr();
    info.lpDirectory = dir.as_ptr();
    info.nShow = SW_SHOWNORMAL;
    // SAFETY: `info` ist vollständig gesetzt; alle Zeichenketten sind nullterminiert und leben
    // bis nach dem Aufruf.
    if unsafe { ShellExecuteExW(&mut info) } == 0 {
        // SAFETY: Liest nur den Fehlercode des eigenen Fadens.
        let code = unsafe { GetLastError() };
        return Err(if code == ERROR_CANCELLED {
            RunError::Declined
        } else {
            RunError::Io(io::Error::from_raw_os_error(code as i32))
        });
    }
    if info.hProcess.is_null() {
        // Ohne Prozessgriff (z. B. über DDE weitergereicht) lässt sich nicht warten.
        return Err(RunError::Io(io::Error::other(
            "kein Prozessgriff vom Start",
        )));
    }
    Ok(ProcessHandle {
        handle: info.hProcess,
    })
}

/// Laufender Motor; der Griff wird beim Verwerfen geschlossen.
#[cfg(windows)]
pub struct ProcessHandle {
    handle: windows_sys::Win32::Foundation::HANDLE,
}

#[cfg(windows)]
impl ProcessHandle {
    /// Exit-Code, falls der Motor schon fertig ist (ohne Warten).
    #[expect(
        dead_code,
        reason = "die Seite wartet im eigenen Faden mit wait; Abfrage ohne Warten bleibt bereit"
    )]
    pub fn try_wait(&self) -> Option<u32> {
        self.wait_ms(0)
    }

    /// Wartet bis zum Ende des Motors und liefert seinen Exit-Code.
    pub fn wait(self) -> u32 {
        use windows_sys::Win32::System::Threading::INFINITE;
        // Bei einem Fehler des Wartens selbst gilt der Lauf als gescheitert.
        self.wait_ms(INFINITE).unwrap_or(1)
    }

    fn wait_ms(&self, ms: u32) -> Option<u32> {
        use windows_sys::Win32::Foundation::{STILL_ACTIVE, WAIT_OBJECT_0};
        use windows_sys::Win32::System::Threading::{GetExitCodeProcess, WaitForSingleObject};
        // SAFETY: `handle` ist ein gültiger Prozessgriff aus ShellExecuteExW, bis Drop.
        if unsafe { WaitForSingleObject(self.handle, ms) } != WAIT_OBJECT_0 {
            return None;
        }
        let mut code = 0u32;
        // SAFETY: wie oben; `code` ist ein gültiger Zeiger auf u32.
        if unsafe { GetExitCodeProcess(self.handle, &mut code) } == 0 || code == STILL_ACTIVE as u32
        {
            return None;
        }
        Some(code)
    }
}

#[cfg(windows)]
impl Drop for ProcessHandle {
    fn drop(&mut self) {
        // SAFETY: Der Griff gehört nur dieser Struktur und wird genau einmal geschlossen.
        unsafe {
            windows_sys::Win32::Foundation::CloseHandle(self.handle);
        }
    }
}

/// Außerhalb von Windows: Start über `std::process::Command` (nur damit es baut und testet).
#[cfg(not(windows))]
pub fn spawn(exe: &Path, args: &[String], _elevate: bool) -> Result<ProcessHandle, RunError> {
    std::process::Command::new(exe)
        .args(args)
        .spawn()
        .map(|child| ProcessHandle {
            child: std::cell::RefCell::new(child),
        })
        .map_err(RunError::Io)
}

/// Laufender Motor (Nicht-Windows).
#[cfg(not(windows))]
pub struct ProcessHandle {
    child: std::cell::RefCell<std::process::Child>,
}

#[cfg(not(windows))]
impl ProcessHandle {
    /// Exit-Code, falls der Motor schon fertig ist (ohne Warten).
    #[expect(
        dead_code,
        reason = "die Seite wartet im eigenen Faden mit wait; Abfrage ohne Warten bleibt bereit"
    )]
    pub fn try_wait(&self) -> Option<u32> {
        match self.child.borrow_mut().try_wait() {
            Ok(Some(status)) => Some(status.code().unwrap_or(1) as u32),
            _ => None,
        }
    }

    /// Wartet bis zum Ende des Motors und liefert seinen Exit-Code.
    pub fn wait(self) -> u32 {
        self.child
            .borrow_mut()
            .wait()
            .map(|s| s.code().unwrap_or(1) as u32)
            .unwrap_or(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Eigener, noch nicht vorhandener Testordner unter `%TEMP%`.
    fn test_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("planora-setup-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn unpack_refuses_existing_dir() {
        let dir = test_dir("vorhanden");
        let first = Unpacked::unpack_bytes(dir.clone(), b"motor").expect("erstes Auspacken");
        let err = match Unpacked::unpack_bytes(dir.clone(), b"anders") {
            Ok(_) => panic!("vorhandener Ordner darf nicht übernommen werden"),
            Err(err) => err,
        };
        assert_eq!(err.kind(), io::ErrorKind::AlreadyExists);
        // Der erste Ordner bleibt unberührt.
        assert_eq!(std::fs::read(dir.join(ENGINE_FILE)).unwrap(), b"motor");
        first.cleanup();
        assert!(!dir.exists());
    }

    #[test]
    fn reader_matches_compares_length_and_content() {
        assert!(reader_matches(&mut &b"abc"[..], b"abc").unwrap());
        assert!(!reader_matches(&mut &b"abd"[..], b"abc").unwrap());
        assert!(!reader_matches(&mut &b"abcd"[..], b"abc").unwrap());
        assert!(!reader_matches(&mut &b"ab"[..], b"abc").unwrap());
    }

    /// Unter Windows ist der ausgepackte Motor bis zum Start gesperrt: nicht zum Schreiben zu
    /// öffnen, nicht zu löschen, aber zu lesen. Nach der Freigabe geht beides wieder.
    #[cfg(windows)]
    #[test]
    fn unpacked_engine_is_locked_until_released() {
        let dir = test_dir("sperre");
        let mut unpacked = Unpacked::unpack_bytes(dir.clone(), b"motor").expect("auspacken");
        let exe = dir.join(ENGINE_FILE);
        assert!(
            std::fs::OpenOptions::new().write(true).open(&exe).is_err(),
            "gesperrt: kein Schreiben"
        );
        assert!(
            std::fs::remove_file(&exe).is_err(),
            "gesperrt: kein Löschen"
        );
        assert_eq!(std::fs::read(&exe).unwrap(), b"motor", "lesen geht");
        unpacked.lock = None;
        assert!(std::fs::OpenOptions::new().write(true).open(&exe).is_ok());
        unpacked.cleanup();
        assert!(!dir.exists());
    }
}
