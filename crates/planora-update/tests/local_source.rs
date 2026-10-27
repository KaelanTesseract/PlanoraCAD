//! Echter Netzweg gegen eine lokale Test-Quelle (nur mit dem Schalter `update-test-source` und
//! nur unter Windows, wo `ureq` mitgebaut wird): ein kleiner HTTP-Server auf 127.0.0.1 im Test
//! liefert Release-Liste, Installer und Prüfsumme; [`HttpSource`] und [`HttpDownloader`]
//! laufen dagegen wie im Programm. Ausführen mit
//! `cargo test -p planora-update --features update-test-source`.

#![cfg(all(feature = "update-test-source", windows))]

use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::path::PathBuf;

use planora_update::check::{HttpSource, ReleaseSource};
use planora_update::install::{
    Downloader, HttpDownloader, InstallError, download_limit, local_test_url_ok, release_url_ok,
    verify_sha256,
};
use planora_update::sha256::hex_digest;
use planora_update::{SETUP_NAME, SHA256_NAME, Version, parse_releases};

const SETUP: &[u8] = b"lokale Testversion des Installers";
const TAG: &str = "v9.0.0-dev.1";

/// Antworten nach Pfad: `(Status, Kopfzeilen, Inhalt)`.
fn answer(path: &str, port: u16) -> (u16, Vec<(String, String)>, Vec<u8>) {
    let base = format!("http://127.0.0.1:{port}/dl/{TAG}");
    match path {
        "/releases" => {
            let asset = |name: &str, size: usize| {
                serde_json::json!({
                    "name": name,
                    "browser_download_url": format!("{base}/{name}"),
                    "size": size,
                })
            };
            let body = serde_json::json!([{
                "tag_name": TAG,
                "body": "- Lokal",
                "assets": [asset(SETUP_NAME, SETUP.len()), asset(SHA256_NAME, 100)],
            }])
            .to_string();
            (200, vec![], body.into_bytes())
        }
        p if p == format!("/dl/{TAG}/{SETUP_NAME}") => (200, vec![], SETUP.to_vec()),
        p if p == format!("/dl/{TAG}/{SHA256_NAME}") => (
            200,
            vec![],
            format!("{}  {SETUP_NAME}\n", hex_digest(SETUP)).into_bytes(),
        ),
        "/umleitung" => (
            302,
            vec![("Location".into(), format!("/dl/{TAG}/{SETUP_NAME}"))],
            vec![],
        ),
        "/fremd" => (
            302,
            vec![("Location".into(), "https://example.com/x.exe".into())],
            vec![],
        ),
        _ => (404, vec![], b"nicht da".to_vec()),
    }
}

/// Startet den Server; er beantwortet beliebig viele Anfragen, bis der Test endet.
fn server() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new();
            if reader.read_line(&mut line).is_err() {
                continue;
            }
            let path = line.split_whitespace().nth(1).unwrap_or("/").to_string();
            // Kopfzeilen der Anfrage überlesen.
            loop {
                let mut h = String::new();
                if reader.read_line(&mut h).is_err() || h.trim().is_empty() {
                    break;
                }
            }
            let (status, headers, body) = answer(&path, port);
            let mut out = format!(
                "HTTP/1.1 {status} X\r\nContent-Length: {}\r\nConnection: close\r\n",
                body.len()
            );
            for (k, v) in headers {
                out.push_str(&format!("{k}: {v}\r\n"));
            }
            out.push_str("\r\n");
            let _ = stream.write_all(out.as_bytes());
            let _ = stream.write_all(&body);
            let _ = stream.flush();
        }
    });
    port
}

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "planora-lokal-{name}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn list_download_and_checksum_through_the_real_http_code() {
    let port = server();
    let running = Version::parse("9.0.0-dev.0").unwrap();
    let url = format!("http://127.0.0.1:{port}/releases");
    assert!(local_test_url_ok(&url));
    let source = HttpSource::with_url(&running, url.clone());
    assert_eq!(source.url(), url);
    let json = source.fetch_releases().unwrap();
    let releases = parse_releases(&json).unwrap();
    assert_eq!(releases.len(), 1, "lokale Adressen werden angenommen");
    let release = &releases[0];
    assert!(release_url_ok(&release.setup.url));
    assert!(release_url_ok(&release.sha256.url));

    let dir = temp("laden");
    let d = HttpDownloader::new(&running);
    let sha = dir.join("x.sha256");
    d.download(&release.sha256.url, &sha, 4096, &mut |_, _| true)
        .unwrap();
    let new = dir.join(SETUP_NAME);
    let mut last = 0;
    d.download(
        &release.setup.url,
        &new,
        download_limit(release.setup.size).unwrap(),
        &mut |done, _| {
            last = done;
            true
        },
    )
    .unwrap();
    assert_eq!(last, SETUP.len() as u64);
    assert_eq!(std::fs::read(&new).unwrap(), SETUP);
    let text = std::fs::read_to_string(&sha).unwrap();
    assert_eq!(verify_sha256(&new, &text, SETUP_NAME), Ok(()));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn local_redirects_stay_local() {
    let port = server();
    let running = Version::parse("9.0.0-dev.0").unwrap();
    let d = HttpDownloader::new(&running);
    let dir = temp("umleitung");
    // Innerhalb der lokalen Quelle: erlaubt.
    let dest = dir.join("a");
    d.download(
        &format!("http://127.0.0.1:{port}/umleitung"),
        &dest,
        1000,
        &mut |_, _| true,
    )
    .unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), SETUP);
    // Zu einer fremden Adresse: abgelehnt, bevor dorthin verbunden wird.
    let dest = dir.join("b");
    let r = d.download(
        &format!("http://127.0.0.1:{port}/fremd"),
        &dest,
        1000,
        &mut |_, _| true,
    );
    assert!(matches!(r, Err(InstallError::Network(_))), "{r:?}");
    assert!(!dest.exists());
    // Andere Hosts über HTTP bleiben verboten.
    assert!(!local_test_url_ok("http://localhost:8080/x"));
    assert!(!local_test_url_ok("http://127.0.0.2:8080/x"));
    assert!(!local_test_url_ok("http://127.0.0.1/x"));
    assert!(!local_test_url_ok("http://127.0.0.1:8080/../x"));
    assert!(!release_url_ok("http://example.com/x"));
    let _ = std::fs::remove_dir_all(&dir);
}
