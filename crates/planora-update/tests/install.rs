//! Herunterladen, Prüfen, Austauschen und Rücksetzen ohne Netz und ohne die echte Programmdatei:
//! alles in eigenen Temp-Ordnern, der Download über Nachbildungen von [`Downloader`] auf der
//! gemeinsamen Kopierschleife [`copy_limited`].

use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use planora_update::install::*;
use planora_update::sha256::{Sha256, hex_digest};
use planora_update::{EXE_NAME, REPO};

/// Eigener Temp-Ordner je Test; wird am Ende gelöscht.
struct TempDir(PathBuf);

impl TempDir {
    fn new(prefix: &str) -> TempDir {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "planora-install-{prefix}-{}-{nanos}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        TempDir(dir)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn read(p: &Path) -> String {
    std::fs::read_to_string(p).unwrap()
}

/// Ordner mit laufender „exe“ (Inhalt `alt`) und heruntergeladener `.new` (Inhalt `neu`).
fn setup(prefix: &str) -> (TempDir, Paths) {
    let dir = TempDir::new(prefix);
    let paths = Paths::for_exe(&dir.path().join("planoracad.exe"));
    std::fs::write(&paths.running_exe, "alt").unwrap();
    std::fs::write(&paths.new_exe, "neu").unwrap();
    (dir, paths)
}

#[test]
fn paths_sit_next_to_the_exe() {
    let p = Paths::for_exe(Path::new(r"C:\Programm\planoracad.exe"));
    assert_eq!(p.running_exe, Path::new(r"C:\Programm\planoracad.exe"));
    assert_eq!(p.new_exe, Path::new(r"C:\Programm\planoracad.exe.new"));
    assert_eq!(p.old_exe, Path::new(r"C:\Programm\planoracad.exe.old"));
    assert_eq!(p.ok_marker, Path::new(r"C:\Programm\planoracad.update-ok"));
    assert_eq!(OK_MARKER_NAME, "planoracad.update-ok");
    assert_eq!(
        p.checksum_file(),
        Path::new(r"C:\Programm\planoracad.exe.new.sha256")
    );
}

#[test]
fn sha256_matches_fips_180_4_vectors() {
    assert_eq!(
        hex_digest(b""),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
    assert_eq!(
        hex_digest(b"abc"),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    assert_eq!(
        hex_digest(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"),
        "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
    );
    assert_eq!(
        hex_digest(
            b"abcdefghbcdefghicdefghijdefghijkefghijklfghijklmghijklmnhijklmnoijklmnopjklmnopqklmnopqrlmnopqrsmnopqrstnopqrstu"
        ),
        "cf5b16a778af8380036ce59e7b0492370b249b11e8f07a51afac45037afee9d1"
    );
    // Eine Million „a“ in ungleichen Stücken: prüft das Zusammensetzen über Blockgrenzen.
    let mut h = Sha256::new();
    let chunk = [b'a'; 997];
    let mut left = 1_000_000usize;
    while left > 0 {
        let n = left.min(chunk.len());
        h.update(&chunk[..n]);
        left -= n;
    }
    assert_eq!(
        h.hex(),
        "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0"
    );
}

#[test]
fn verify_accepts_matching_checksum_in_both_formats() {
    let dir = TempDir::new("pruef-gut");
    let file = dir.path().join("x.new");
    std::fs::write(&file, b"abc").unwrap();
    let hex = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
    for contents in [
        format!("{hex}  {EXE_NAME}\n"),
        format!("{hex} *{EXE_NAME}"),
        format!("{}  {EXE_NAME}\r\n", hex.to_uppercase()),
        format!("{hex}\n"),
        format!("  {}  ", hex.to_uppercase()),
    ] {
        assert_eq!(
            verify_sha256(&file, &contents, EXE_NAME),
            Ok(()),
            "{contents:?}"
        );
    }
}

#[test]
fn verify_rejects_wrong_hash_wrong_name_and_garbage() {
    let dir = TempDir::new("pruef-schlecht");
    let file = dir.path().join("x.new");
    std::fs::write(&file, b"abc").unwrap();
    let good = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
    let wrong = "ca7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
    for contents in [
        format!("{wrong}  {EXE_NAME}"),
        format!("{good}  andere.exe"),
        format!("{good}  {EXE_NAME} mehr"),
        format!("{good}  {EXE_NAME}\n{good}  {EXE_NAME}"),
        good[..63].to_string(),
        format!("{good}0"),
        format!("{}g", &good[..63]),
        String::new(),
        "Not Found".to_string(),
        "<html>Fehler</html>".to_string(),
    ] {
        assert_eq!(
            verify_sha256(&file, &contents, EXE_NAME),
            Err(InstallError::Checksum),
            "{contents:?}"
        );
    }
    // Fehlt die Datei, ist das kein Prüfsummen-, sondern ein Dateifehler.
    assert!(matches!(
        verify_sha256(&dir.path().join("fehlt"), good, EXE_NAME),
        Err(InstallError::Io(_))
    ));
}

#[test]
fn swap_moves_old_aside_and_new_into_place() {
    let (_dir, paths) = setup("tausch");
    // Ein `.old` aus einem früheren Update stört nicht.
    std::fs::write(&paths.old_exe, "uralt").unwrap();
    let state = swap(&paths).unwrap();
    assert!(state.stale_old_removed);
    assert_eq!(read(&paths.running_exe), "neu");
    assert_eq!(read(&paths.old_exe), "alt");
    assert!(!paths.new_exe.exists());
    let (_dir, paths) = setup("tausch-ohne-alt");
    assert!(!swap(&paths).unwrap().stale_old_removed);
}

#[test]
fn swap_rolls_back_when_second_step_fails() {
    // Die neue Datei fehlt: Schritt 2 scheitert, die laufende Datei kommt zurück.
    let (_dir, paths) = setup("tausch-zurueck");
    std::fs::remove_file(&paths.new_exe).unwrap();
    assert!(matches!(swap(&paths), Err(InstallError::Io(_))));
    assert_eq!(read(&paths.running_exe), "alt");
    assert!(!paths.old_exe.exists());

    // Die neue Datei ist von einem anderen Vorgang ohne Freigabe geöffnet (Windows: kein
    // Umbenennen möglich).
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        let (_dir, paths) = setup("tausch-gesperrt");
        let lock = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(&paths.new_exe)
            .unwrap();
        assert!(matches!(swap(&paths), Err(InstallError::Io(_))));
        drop(lock);
        assert_eq!(read(&paths.running_exe), "alt");
        assert_eq!(read(&paths.new_exe), "neu");
        assert!(!paths.old_exe.exists());
    }
}

#[test]
fn swap_without_running_exe_changes_nothing() {
    let (_dir, paths) = setup("tausch-ohne-exe");
    std::fs::remove_file(&paths.running_exe).unwrap();
    assert!(swap(&paths).is_err());
    assert_eq!(read(&paths.new_exe), "neu");
    assert!(!paths.running_exe.exists());
}

#[test]
fn rollback_restores_old() {
    let (_dir, paths) = setup("zurueck");
    swap(&paths).unwrap();
    rollback(&paths).unwrap();
    assert_eq!(read(&paths.running_exe), "alt");
    assert!(!paths.old_exe.exists());
    // Die zurückgenommene neue Datei ist weg (oder liegt höchstens als `.new` herum).
    assert!(!paths.new_exe.exists() || read(&paths.new_exe) == "neu");
    // Ohne Sicherung gibt es nichts zurückzulegen: Fehler, nichts verändert.
    assert!(matches!(rollback(&paths), Err(InstallError::Io(_))));
    assert_eq!(read(&paths.running_exe), "alt");
}

#[test]
fn cleanup_old_ignores_errors() {
    let (_dir, paths) = setup("aufraeumen");
    std::fs::write(&paths.old_exe, "alt").unwrap();
    cleanup_old(&paths);
    assert!(!paths.old_exe.exists());
    // Nichts da: kein Fehler, kein Absturz.
    cleanup_old(&paths);
    // Nicht löschbar (ein Ordner statt einer Datei): still übergangen.
    std::fs::create_dir(&paths.old_exe).unwrap();
    std::fs::write(paths.old_exe.join("drin"), "x").unwrap();
    cleanup_old(&paths);
    assert!(paths.old_exe.exists());
    // Die laufende Datei bleibt immer.
    assert_eq!(read(&paths.running_exe), "alt");
}

#[test]
fn discard_download_removes_new_and_checksum_files() {
    let (_dir, paths) = setup("verwerfen");
    std::fs::write(paths.checksum_file(), "x").unwrap();
    discard_download(&paths);
    assert!(!paths.new_exe.exists());
    assert!(!paths.checksum_file().exists());
    assert_eq!(read(&paths.running_exe), "alt");
    discard_download(&paths);
}

#[test]
fn not_writable_dir_is_reported() {
    let dir = TempDir::new("schreibbar");
    assert!(exe_dir_writable(dir.path()));
    // Keine Spur der Probe bleibt zurück.
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    assert!(!exe_dir_writable(&dir.path().join("gibt-es-nicht")));
    let file = dir.path().join("datei");
    std::fs::write(&file, "x").unwrap();
    assert!(!exe_dir_writable(&file));
    assert_eq!(
        InstallError::NotWritable.message(),
        "Das Programm liegt in einem geschützten Ordner. Lade die neue Version von GitHub herunter."
    );
}

#[test]
fn errors_have_plain_german_messages() {
    for e in [
        InstallError::Cancelled,
        InstallError::Network("keine Verbindung".into()),
        InstallError::TooLarge,
        InstallError::Checksum,
        InstallError::NotWritable,
        InstallError::Io("Zugriff verweigert".into()),
    ] {
        let m = e.message();
        assert!(!m.is_empty() && m.ends_with('.'), "{m}");
    }
    assert!(
        InstallError::Checksum
            .message()
            .contains("nichts ausgetauscht")
    );
    assert!(
        InstallError::Network("keine Verbindung".into())
            .message()
            .contains("keine Verbindung")
    );
}

#[test]
fn download_limit_follows_the_asset_size_with_a_hard_cap() {
    assert_eq!(download_limit(1000), Ok(1000 + SIZE_SLACK));
    assert_eq!(download_limit(0), Ok(SIZE_SLACK));
    assert_eq!(MAX_DOWNLOAD_BYTES, 200 * 1024 * 1024);
    assert_eq!(download_limit(MAX_DOWNLOAD_BYTES), Ok(MAX_DOWNLOAD_BYTES));
    assert_eq!(
        download_limit(MAX_DOWNLOAD_BYTES + 1),
        Err(InstallError::TooLarge)
    );
    assert_eq!(STALL_TIMEOUT, Duration::from_secs(30));
}

#[test]
fn only_release_urls_and_github_redirects_are_allowed() {
    let base = format!("https://github.com/{REPO}/releases/download/v0.2.0-dev.1/{EXE_NAME}");
    assert!(release_url_ok(&base));
    for bad in [
        format!("http://github.com/{REPO}/releases/download/v1/{EXE_NAME}"),
        format!("https://github.com/andere/repo/releases/download/v1/{EXE_NAME}"),
        format!("https://example.com/{REPO}/releases/download/v1/{EXE_NAME}"),
        format!("https://github.com/{REPO}/releases/download/../../x/{EXE_NAME}"),
        format!("https://github.com/{REPO}/releases/download/v1/{EXE_NAME}?x=1"),
        format!("https://github.com.evil.com/{REPO}/releases/download/v1/{EXE_NAME}"),
    ] {
        assert!(!release_url_ok(&bad), "{bad}");
    }

    let ok = |loc: &str| redirect_target(&base, loc);
    assert_eq!(
        ok("https://objects.githubusercontent.com/github-production-release-asset/1?a=b"),
        Ok("https://objects.githubusercontent.com/github-production-release-asset/1?a=b".into())
    );
    assert!(ok("https://release-assets.githubusercontent.com/x").is_ok());
    assert!(ok("https://github.com/x").is_ok());
    assert!(ok("https://GitHub.com:443/x").is_ok());
    assert_eq!(ok("/x/y"), Ok("https://github.com/x/y".into()));
    assert_eq!(
        ok("//objects.githubusercontent.com/z"),
        Ok("https://objects.githubusercontent.com/z".into())
    );
    for bad in [
        "http://objects.githubusercontent.com/x",
        "https://evil.com/x",
        "https://githubusercontent.com.evil.com/x",
        "https://evilgithubusercontent.com/x",
        "https://.githubusercontent.com/x",
        "https://github.com@evil.com/x",
        "https://user@github.com/x",
        "https://github.com:8443/x",
        "ftp://github.com/x",
        "relativ/pfad",
        "",
    ] {
        assert!(
            matches!(ok(bad), Err(InstallError::Network(_))),
            "{bad}: {:?}",
            ok(bad)
        );
    }
    assert_eq!(MAX_REDIRECTS, 5);
}

/// Leser, der `data` in kleinen Stücken liefert.
struct Slow {
    data: Vec<u8>,
    at: usize,
}

impl Read for Slow {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = buf.len().min(1000).min(self.data.len() - self.at);
        buf[..n].copy_from_slice(&self.data[self.at..self.at + n]);
        self.at += n;
        Ok(n)
    }
}

/// Leser, der nach `first` Bytes stehen bleibt, bis der Sender wegfällt.
struct Stuck {
    first: usize,
    hold: mpsc::Receiver<()>,
}

impl Read for Stuck {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.first > 0 {
            let n = buf.len().min(self.first);
            buf[..n].fill(7);
            self.first -= n;
            return Ok(n);
        }
        let _ = self.hold.recv();
        Err(io::Error::from(io::ErrorKind::ConnectionReset))
    }
}

/// Nachbildung des Downloads: liest aus einem Leser über die gemeinsame Kopierschleife, mit
/// kurzer Stillstandsgrenze.
struct FakeDownloader<F: Fn() -> Box<dyn Read + Send> + Sync> {
    reader: F,
    stall: Duration,
}

impl<F: Fn() -> Box<dyn Read + Send> + Sync> Downloader for FakeDownloader<F> {
    fn download(
        &self,
        url: &str,
        dest: &Path,
        max_bytes: u64,
        progress: &mut dyn FnMut(u64, u64) -> bool,
    ) -> Result<(), InstallError> {
        assert!(release_url_ok(url));
        copy_limited((self.reader)(), 0, dest, max_bytes, self.stall, progress)
    }
}

fn url() -> String {
    format!("https://github.com/{REPO}/releases/download/v9.0.0/{EXE_NAME}")
}

#[test]
fn download_writes_the_file_and_reports_progress() {
    let dir = TempDir::new("laden");
    let dest = dir.path().join("x.new");
    let d = FakeDownloader {
        reader: || {
            Box::new(Slow {
                data: vec![1; 10_000],
                at: 0,
            })
        },
        stall: Duration::from_secs(5),
    };
    let mut seen = Vec::new();
    d.download(&url(), &dest, 10_000, &mut |done, _| {
        seen.push(done);
        true
    })
    .unwrap();
    assert_eq!(std::fs::read(&dest).unwrap().len(), 10_000);
    assert_eq!(seen.first(), Some(&0));
    assert_eq!(seen.last(), Some(&10_000));
    assert!(seen.windows(2).all(|w| w[0] <= w[1]));
}

#[test]
fn download_is_cancelled_by_progress_callback_and_leaves_no_file() {
    let dir = TempDir::new("abbruch");
    let dest = dir.path().join("x.new");
    let d = FakeDownloader {
        reader: || {
            Box::new(Slow {
                data: vec![1; 50_000],
                at: 0,
            })
        },
        stall: Duration::from_secs(5),
    };
    let r = d.download(&url(), &dest, 100_000, &mut |done, _| done < 5_000);
    assert_eq!(r, Err(InstallError::Cancelled));
    assert!(!dest.exists());

    // Auch während der Leser steht, wirkt der Abbruch rasch.
    let (_keep, hold) = mpsc::channel();
    let hold = std::sync::Mutex::new(Some(hold));
    let d = FakeDownloader {
        reader: || {
            Box::new(Stuck {
                first: 0,
                hold: hold.lock().unwrap().take().unwrap(),
            })
        },
        stall: Duration::from_secs(30),
    };
    let start = Instant::now();
    let r = d.download(&url(), &dest, 100, &mut |_, _| {
        start.elapsed() < Duration::from_millis(300)
    });
    assert_eq!(r, Err(InstallError::Cancelled));
    assert!(start.elapsed() < Duration::from_secs(5));
    assert!(!dest.exists());
}

#[test]
fn download_over_max_bytes_is_rejected() {
    let dir = TempDir::new("zu-gross");
    let dest = dir.path().join("x.new");
    let d = FakeDownloader {
        reader: || {
            Box::new(Slow {
                data: vec![1; 10_001],
                at: 0,
            })
        },
        stall: Duration::from_secs(5),
    };
    let r = d.download(&url(), &dest, 10_000, &mut |_, _| true);
    assert_eq!(r, Err(InstallError::TooLarge));
    assert!(!dest.exists());
}

#[test]
fn stalled_download_times_out() {
    let dir = TempDir::new("stillstand");
    let dest = dir.path().join("x.new");
    let (_keep, hold) = mpsc::channel();
    let hold = std::sync::Mutex::new(Some(hold));
    let d = FakeDownloader {
        reader: || {
            Box::new(Stuck {
                first: 3000,
                hold: hold.lock().unwrap().take().unwrap(),
            })
        },
        stall: Duration::from_millis(300),
    };
    let start = Instant::now();
    let r = d.download(&url(), &dest, 100_000, &mut |_, _| true);
    assert!(matches!(r, Err(InstallError::Network(_))), "{r:?}");
    assert!(start.elapsed() >= Duration::from_millis(300));
    assert!(start.elapsed() < Duration::from_secs(10));
    assert!(!dest.exists());
}

#[test]
fn broken_connection_is_a_network_error_and_leaves_no_file() {
    let dir = TempDir::new("abgerissen");
    let dest = dir.path().join("x.new");
    // Der Sender fällt sofort weg: der Leser meldet nach 2000 Bytes einen Verbindungsabbruch.
    let d = FakeDownloader {
        reader: || {
            let (_, hold) = mpsc::channel();
            Box::new(Stuck { first: 2000, hold })
        },
        stall: Duration::from_secs(5),
    };
    let r = d.download(&url(), &dest, 100_000, &mut |_, _| true);
    assert!(matches!(r, Err(InstallError::Network(_))), "{r:?}");
    assert!(!dest.exists());
}

#[test]
fn http_downloader_is_built_but_never_used() {
    let d = HttpDownloader::new(&planora_update::Version::parse("0.2.0-dev.1").unwrap());
    // Eine fremde Adresse wird abgelehnt, ohne ins Netz zu gehen.
    let dir = TempDir::new("http");
    let r = d.download(
        "https://example.com/x.exe",
        &dir.path().join("x"),
        10,
        &mut |_, _| true,
    );
    assert!(matches!(r, Err(InstallError::Network(_))));
}
