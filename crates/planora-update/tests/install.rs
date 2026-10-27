//! Herunterladen und Prüfen des Installers ohne Netz:
//! alles in eigenen Temp-Ordnern, der Download über Nachbildungen von [`Downloader`] auf der
//! gemeinsamen Kopierschleife [`copy_limited`].

use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use planora_update::install::*;
use planora_update::sha256::{Sha256, hex_digest};
use planora_update::{REPO, SETUP_NAME, SHA256_NAME};

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

#[test]
fn download_paths_sit_in_their_own_folder() {
    // Pfade aus Teilen, damit der Test auf jedem System gilt.
    let dir = Path::new("Temp").join("PlanoraCAD-Update");
    let p = DownloadPaths::in_dir(&dir);
    assert_eq!(p.dir, dir);
    assert_eq!(p.setup, dir.join("planoracad-setup-x64.exe"));
    assert_eq!(p.checksum, dir.join("planoracad-setup-x64.sha256"));
    assert_eq!(p.setup, dir.join(SETUP_NAME));
    assert_eq!(p.checksum, dir.join(SHA256_NAME));
}

#[test]
fn prepare_dir_creates_the_folder() {
    let dir = TempDir::new("vorbereiten");
    let paths = DownloadPaths::in_dir(&dir.path().join("a").join("PlanoraCAD-Update"));
    assert_eq!(prepare_dir(&paths), Ok(()));
    assert!(paths.dir.is_dir());
    // Schon da: kein Fehler.
    assert_eq!(prepare_dir(&paths), Ok(()));
    // An der Stelle liegt eine Datei: Dateifehler.
    let file = dir.path().join("datei");
    std::fs::write(&file, "x").unwrap();
    assert!(matches!(
        prepare_dir(&DownloadPaths::in_dir(&file)),
        Err(InstallError::Io(_))
    ));
}

#[test]
fn discard_download_removes_files_and_the_empty_folder() {
    let dir = TempDir::new("verwerfen");
    let paths = DownloadPaths::in_dir(&dir.path().join("PlanoraCAD-Update"));
    prepare_dir(&paths).unwrap();
    std::fs::write(&paths.setup, "neu").unwrap();
    std::fs::write(&paths.checksum, "x").unwrap();
    discard_download(&paths);
    assert!(!paths.setup.exists());
    assert!(!paths.checksum.exists());
    assert!(!paths.dir.exists());
    // Nichts mehr da: kein Fehler, kein Absturz.
    discard_download(&paths);
}

#[test]
fn discard_download_keeps_a_folder_with_other_files() {
    let dir = TempDir::new("verwerfen-fremd");
    let paths = DownloadPaths::in_dir(&dir.path().join("PlanoraCAD-Update"));
    prepare_dir(&paths).unwrap();
    std::fs::write(&paths.setup, "neu").unwrap();
    std::fs::write(paths.dir.join("fremd.txt"), "bleibt").unwrap();
    discard_download(&paths);
    assert!(!paths.setup.exists());
    assert_eq!(read(&paths.dir.join("fremd.txt")), "bleibt");
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
        format!("{hex}  {SETUP_NAME}\n"),
        format!("{hex} *{SETUP_NAME}"),
        format!("{}  {SETUP_NAME}\r\n", hex.to_uppercase()),
        format!("{hex}\n"),
        format!("  {}  ", hex.to_uppercase()),
    ] {
        assert_eq!(
            verify_sha256(&file, &contents, SETUP_NAME),
            Ok(()),
            "{contents:?}"
        );
    }
}

#[test]
fn expected_hex_is_kept_and_rechecked() {
    let dir = TempDir::new("pruef-zweimal");
    let file = dir.path().join("x.new");
    std::fs::write(&file, b"abc").unwrap();
    let hex = "BA7816BF8F01CFEA414140DE5DAE2223B00361A396177A9CB410FF61F20015AD";
    let kept = checksum_hex(&format!("{hex}  {SETUP_NAME}"), SETUP_NAME).unwrap();
    assert_eq!(kept, hex.to_ascii_lowercase());
    assert_eq!(file_matches_sha256(&file, &kept), Ok(()));
    // Danach verändert: die zweite Prüfung schlägt an.
    std::fs::write(&file, b"abd").unwrap();
    assert_eq!(
        file_matches_sha256(&file, &kept),
        Err(InstallError::Checksum)
    );
    assert_eq!(
        checksum_hex("Müll", SETUP_NAME),
        Err(InstallError::Checksum)
    );
}

#[test]
fn verify_rejects_wrong_hash_wrong_name_and_garbage() {
    let dir = TempDir::new("pruef-schlecht");
    let file = dir.path().join("x.new");
    std::fs::write(&file, b"abc").unwrap();
    let good = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
    let wrong = "ca7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
    for contents in [
        format!("{wrong}  {SETUP_NAME}"),
        format!("{good}  andere.exe"),
        format!("{good}  {SETUP_NAME} mehr"),
        format!("{good}  {SETUP_NAME}\n{good}  {SETUP_NAME}"),
        good[..63].to_string(),
        format!("{good}0"),
        format!("{}g", &good[..63]),
        String::new(),
        "Not Found".to_string(),
        "<html>Fehler</html>".to_string(),
    ] {
        assert_eq!(
            verify_sha256(&file, &contents, SETUP_NAME),
            Err(InstallError::Checksum),
            "{contents:?}"
        );
    }
    // Fehlt die Datei, ist das kein Prüfsummen-, sondern ein Dateifehler.
    assert!(matches!(
        verify_sha256(&dir.path().join("fehlt"), good, SETUP_NAME),
        Err(InstallError::Io(_))
    ));
}

#[test]
fn errors_have_plain_german_messages() {
    for e in [
        InstallError::Cancelled,
        InstallError::Network("keine Verbindung".into()),
        InstallError::TooLarge,
        InstallError::Checksum,
        InstallError::Io("Zugriff verweigert".into()),
    ] {
        let m = e.message();
        assert!(!m.is_empty() && m.ends_with('.'), "{m}");
    }
    assert_eq!(
        InstallError::Checksum.message(),
        "Die heruntergeladene Datei ist beschädigt (die Prüfsumme stimmt nicht). Es wurde nichts installiert."
    );
    assert_eq!(
        InstallError::TooLarge.message(),
        "Die heruntergeladene Datei ist größer als erwartet. Es wurde nichts installiert."
    );
    assert_eq!(
        InstallError::Io("Zugriff verweigert".into()).message(),
        "Das Update ließ sich nicht vorbereiten (Zugriff verweigert). Das Programm läuft in der bisherigen Version weiter."
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
    let base = format!("https://github.com/{REPO}/releases/download/v0.2.0-dev.1/{SETUP_NAME}");
    assert!(release_url_ok(&base));
    for bad in [
        format!("http://github.com/{REPO}/releases/download/v1/{SETUP_NAME}"),
        format!("https://github.com/andere/repo/releases/download/v1/{SETUP_NAME}"),
        format!("https://example.com/{REPO}/releases/download/v1/{SETUP_NAME}"),
        format!("https://github.com/{REPO}/releases/download/../../x/{SETUP_NAME}"),
        format!("https://github.com/{REPO}/releases/download/v1/{SETUP_NAME}?x=1"),
        format!("https://github.com.evil.com/{REPO}/releases/download/v1/{SETUP_NAME}"),
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
    format!("https://github.com/{REPO}/releases/download/v9.0.0/{SETUP_NAME}")
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

/// Ohne den Testschalter `update-test-source` (so wird jedes Release gebaut) gibt es keine lokale
/// Quelle: keine `http://`-Adressen, keine Weiterleitung dorthin, keine andere Release-Liste.
#[cfg(not(feature = "update-test-source"))]
#[test]
fn default_build_accepts_no_local_test_source() {
    let local = format!("http://127.0.0.1:8080/dl/v9.0.0/{SETUP_NAME}");
    assert!(!local_test_url_ok(&local));
    assert!(!release_url_ok(&local));
    assert!(
        redirect_target(
            &format!("https://github.com/{REPO}/releases/download/v9.0.0/{SETUP_NAME}"),
            &local
        )
        .is_err()
    );
    let json = serde_json::json!([{
        "tag_name": "v9.0.0",
        "assets": [
            {"name": SETUP_NAME, "browser_download_url": local, "size": 10},
            {"name": planora_update::SHA256_NAME,
             "browser_download_url": format!("http://127.0.0.1:8080/dl/v9.0.0/{}", planora_update::SHA256_NAME),
             "size": 10},
        ],
    }])
    .to_string();
    assert!(planora_update::parse_releases(&json).unwrap().is_empty());
    let source =
        planora_update::check::HttpSource::new(&planora_update::Version::parse("0.2.0").unwrap());
    assert_eq!(source.url(), planora_update::check::RELEASES_URL);
}
