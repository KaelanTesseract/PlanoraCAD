//! Task 13: Die Meldungen von `planora-update` (Netzfehler, Release-Liste, Versionsnummer,
//! Kanal-Namen, Download-Fehler samt Einzelheiten) erscheinen unter `with_lang(&english())` auf
//! Englisch; unter Deutsch bleiben sie wie bisher.

use std::io::{self, Read};
use std::time::Duration;

use planora_i18n::{english, german, with_lang};
use planora_update::check::NetError;
use planora_update::install::*;
use planora_update::{Channel, Version, parse_releases};

fn assert_english(what: &str, text: &str) {
    assert!(!text.is_empty(), "{what}: leer");
    assert!(
        !text.chars().any(|c| "äöüßÄÖÜ„“".contains(c)),
        "{what}: {text}"
    );
    let looks_like_key = !text.contains(' ')
        && text.contains('-')
        && text
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
    assert!(!looks_like_key, "{what}: Schlüssel statt Text: {text}");
}

/// Ein Leser, der lange keine Daten liefert (bis der Stillstand greift).
struct Silent;
impl Read for Silent {
    fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
        std::thread::sleep(Duration::from_secs(2));
        Ok(0)
    }
}

fn details() -> Vec<(&'static str, InstallError)> {
    let dir = std::env::temp_dir().join(format!("planora-upd-en-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut out = vec![
        (
            "redirect foreign",
            redirect_target("https://github.com/x/y", "https://evil.example.com/a").unwrap_err(),
        ),
        (
            "redirect relative without origin",
            redirect_target("not a url", "/a").unwrap_err(),
        ),
        (
            "redirect scheme",
            redirect_target("https://github.com/x/y", "ftp://github.com/a").unwrap_err(),
        ),
        (
            "missing file",
            file_matches_sha256(&dir.join("missing"), "00").unwrap_err(),
        ),
        (
            "missing folder",
            copy_limited(
                io::empty(),
                0,
                &dir.join("no").join("such").join("file"),
                10,
                Duration::from_secs(1),
                &mut |_, _| true,
            )
            .unwrap_err(),
        ),
        (
            "stalled",
            copy_limited(
                Silent,
                0,
                &dir.join("a"),
                10,
                Duration::from_millis(1),
                &mut |_, _| true,
            )
            .unwrap_err(),
        ),
        (
            "cancelled",
            copy_limited(
                io::empty(),
                0,
                &dir.join("b"),
                10,
                Duration::from_secs(1),
                &mut |_, _| false,
            )
            .unwrap_err(),
        ),
    ];
    out.push(("folder is a file", {
        let file = dir.join("plain");
        std::fs::write(&file, b"x").unwrap();
        prepare_dir(&DownloadPaths::in_dir(&file.join("sub"))).unwrap_err()
    }));
    let _ = std::fs::remove_dir_all(&dir);
    out
}

#[test]
fn install_errors_speak_english() {
    with_lang(&english(), || {
        for (what, e) in details() {
            assert_english(what, &e.message());
            assert_english(what, &e.to_string());
            let detail = match &e {
                InstallError::Network(d) | InstallError::Io(d) => d.clone(),
                _ => String::new(),
            };
            // Die Einzelheit kommt vom Betriebssystem (englisch) oder aus unseren Texten.
            if !detail.is_empty() {
                assert_english(what, &detail);
            }
        }
        for e in [
            InstallError::Cancelled,
            InstallError::TooLarge,
            InstallError::Checksum,
            InstallError::Io("disk full".into()),
            InstallError::Network("offline".into()),
        ] {
            assert_english(&format!("{e:?}"), &e.message());
        }
        assert_eq!(
            InstallError::Checksum.message(),
            "The downloaded file is damaged (the checksum does not match). Nothing was installed."
        );
        assert_eq!(
            redirect_target("https://github.com/x/y", "https://evil.example.com/a").unwrap_err(),
            InstallError::Network("Redirect to a foreign address".into())
        );
    });
    with_lang(&german(), || {
        assert_eq!(
            redirect_target("https://github.com/x/y", "https://evil.example.com/a").unwrap_err(),
            InstallError::Network("Weiterleitung zu einer fremden Adresse".into())
        );
    });
}

#[test]
fn net_release_and_version_errors_speak_english() {
    let nets = [
        NetError::Offline,
        NetError::Timeout,
        NetError::RateLimited,
        NetError::Http(502),
        NetError::Unsupported,
    ];
    with_lang(&english(), || {
        for e in &nets {
            assert_english(&format!("{e:?}"), &e.to_string());
        }
        assert_eq!(NetError::Offline.to_string(), "no connection");
        assert_eq!(NetError::Http(502).to_string(), "response with status 502");
        let e = parse_releases("not json").unwrap_err();
        assert_english("release json", &e.to_string());
        assert!(
            e.to_string()
                .starts_with("The response from GitHub could not be read:")
        );
        let e = parse_releases("{}").unwrap_err();
        assert_eq!(
            e.to_string(),
            "The response from GitHub is not a release list"
        );
        let e = Version::parse("banana").unwrap_err();
        assert_eq!(e.to_string(), "not a valid version number: \"banana\"");
        assert_eq!(Channel::Dev.label(), "Dev");
        assert_eq!(Channel::Beta.label(), "Beta");
        assert_eq!(Channel::Stable.label(), "Stable");
    });
    with_lang(&german(), || {
        assert_eq!(NetError::Offline.to_string(), "keine Verbindung");
        assert_eq!(NetError::Http(502).to_string(), "Antwort mit Status 502");
        assert_eq!(Channel::Stable.label(), "Stabil");
        assert_eq!(
            Version::parse("banana").unwrap_err().to_string(),
            "keine gültige Versionsnummer: \"banana\""
        );
    });
}
