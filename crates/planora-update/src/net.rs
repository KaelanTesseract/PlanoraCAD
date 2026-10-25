//! Echter Abruf der Release-Liste über HTTPS (nur Windows).
//!
//! Client ist `ureq` mit der TLS-Schicht von Windows (Schannel über `native-tls`); geprüft wird
//! gegen den Zertifikatsspeicher von Windows. Keine Anmeldung, keine Nutzerdaten: gesendet wird
//! nur die Anfrage selbst mit der Kennung `PlanoraCAD/<Version>`. Auf anderen Systemen liefert
//! [`HttpSource`] immer [`NetError::Unsupported`], ohne ins Netz zu gehen.

use std::time::Duration;

use crate::check::{NetError, ReleaseSource};
#[cfg(test)]
use crate::release::REPO;
use crate::version::Version;

/// Adresse der Release-Liste.
pub const RELEASES_URL: &str = "https://api.github.com/repos/KaelanTesseract/PlanoraCAD/releases";

/// Zeitüberschreitung je Abschnitt (Namensauflösung, Verbindung, Antwort, Inhalt).
pub const TIMEOUT: Duration = Duration::from_secs(10);

/// Obergrenze für die ganze Anfrage.
#[cfg(windows)]
const TOTAL_TIMEOUT: Duration = Duration::from_secs(30);

/// Größte angenommene Antwort (Bytes); eine größere zählt als Fehler.
pub const MAX_RESPONSE_BYTES: u64 = 2 * 1024 * 1024;

/// Umgebungsvariable mit der Adresse einer lokalen Release-Liste (nur mit dem Schalter
/// `update-test-source`; sonst ohne Wirkung).
#[cfg(feature = "update-test-source")]
pub const TEST_API_ENV: &str = "PLANORA_UPDATE_TEST_API";

/// Abruf bei GitHub über HTTPS. Wird nur im Programm angelegt, nie in Tests benutzt.
pub struct HttpSource {
    user_agent: String,
    url: String,
    #[cfg(windows)]
    agent: ureq::Agent,
}

impl HttpSource {
    /// Quelle mit der Kennung `PlanoraCAD/<running>`; baut den Client, verbindet aber noch nicht.
    /// Mit dem Schalter `update-test-source` und gesetztem [`TEST_API_ENV`] (eine Adresse
    /// `http://127.0.0.1:<Port>/…`) wird statt GitHub diese Adresse gefragt.
    pub fn new(running: &Version) -> Self {
        #[cfg(feature = "update-test-source")]
        if let Some(url) = std::env::var(TEST_API_ENV)
            .ok()
            .filter(|u| crate::install::local_test_url_ok(u))
        {
            return Self::with_url(running, url);
        }
        Self::build(running, RELEASES_URL.to_string())
    }

    /// Quelle für die lokale Test-Quelle `url` (`http://127.0.0.1:<Port>/…`; nur mit dem
    /// Schalter `update-test-source`).
    #[cfg(feature = "update-test-source")]
    pub fn with_url(running: &Version, url: String) -> Self {
        Self::build(running, url)
    }

    fn build(running: &Version, url: String) -> Self {
        let user_agent = format!("PlanoraCAD/{running}");
        HttpSource {
            #[cfg(windows)]
            agent: windows::agent(&user_agent, crate::install::local_test_url_ok(&url)),
            user_agent,
            url,
        }
    }

    /// Abgefragte Adresse ([`RELEASES_URL`], mit dem Testschalter auch die lokale).
    pub fn url(&self) -> &str {
        &self.url
    }

    /// Gesendete Kennung des Programms.
    pub fn user_agent(&self) -> &str {
        &self.user_agent
    }
}

impl ReleaseSource for HttpSource {
    #[cfg(windows)]
    fn fetch_releases(&self) -> Result<String, NetError> {
        windows::fetch(&self.agent, &self.url)
    }

    #[cfg(not(windows))]
    fn fetch_releases(&self) -> Result<String, NetError> {
        Err(NetError::Unsupported)
    }
}

/// Zeitgrenze je Abschnitt eines Downloads (Namensauflösung, Verbindung, Antwortkopf).
#[cfg(windows)]
const DOWNLOAD_STEP_TIMEOUT: Duration = crate::install::STALL_TIMEOUT;

/// Obergrenze für einen ganzen Download samt Weiterleitungen; beendet auch einen hängenden
/// Lese-Thread, dessen Download schon wegen Stillstand oder Abbruch aufgegeben wurde.
#[cfg(windows)]
const DOWNLOAD_TOTAL_TIMEOUT: Duration = Duration::from_secs(60 * 60);

/// Download einer Datei eines Releases über HTTPS (nur Windows, sonst Fehler „nur Windows“).
///
/// Nur Adressen aus dem Release ([`crate::install::release_url_ok`]); Weiterleitungen folgt der
/// Client nicht selbst, sondern jede wird geprüft ([`crate::install::redirect_target`]: nur
/// HTTPS auf `github.com` und `*.githubusercontent.com`, höchstens
/// [`crate::install::MAX_REDIRECTS`]). Gelesen wird über [`crate::install::copy_limited`] mit
/// Größen- und Stillstandsgrenze. Wird nur im Programm angelegt, nie in Tests benutzt.
pub struct HttpDownloader {
    #[cfg(windows)]
    agent: ureq::Agent,
    /// Client ohne HTTPS-Zwang, nur für die lokale Test-Quelle.
    #[cfg(all(windows, feature = "update-test-source"))]
    local_agent: ureq::Agent,
}

impl HttpDownloader {
    /// Client mit der Kennung `PlanoraCAD/<running>`; verbindet noch nicht.
    pub fn new(running: &Version) -> Self {
        #[cfg(not(windows))]
        let _ = running;
        HttpDownloader {
            #[cfg(windows)]
            agent: windows::download_agent(&format!("PlanoraCAD/{running}"), false),
            #[cfg(all(windows, feature = "update-test-source"))]
            local_agent: windows::download_agent(&format!("PlanoraCAD/{running}"), true),
        }
    }

    /// Der Client für `url`: für die lokale Test-Quelle der ohne HTTPS-Zwang, sonst der normale.
    #[cfg(windows)]
    fn agent_for(&self, url: &str) -> &ureq::Agent {
        #[cfg(feature = "update-test-source")]
        if crate::install::local_test_url_ok(url) {
            return &self.local_agent;
        }
        let _ = url;
        &self.agent
    }
}

impl crate::install::Downloader for HttpDownloader {
    fn download(
        &self,
        url: &str,
        dest: &std::path::Path,
        max_bytes: u64,
        progress: &mut dyn FnMut(u64, u64) -> bool,
    ) -> Result<(), crate::install::InstallError> {
        use crate::install::{InstallError, release_url_ok};
        if !release_url_ok(url) {
            return Err(InstallError::Network(
                "Adresse gehört nicht zum Release".into(),
            ));
        }
        #[cfg(windows)]
        {
            windows::download(&|u| self.agent_for(u), url, dest, max_bytes, progress)
        }
        #[cfg(not(windows))]
        {
            let _ = (dest, max_bytes, progress);
            Err(InstallError::Network("nur Windows".into()))
        }
    }
}

/// Fehler zu einem Statuscode, `None` bei Erfolg (2xx). GitHub meldet das Rate-Limit mit 429
/// oder mit 403 und `x-ratelimit-remaining: 0`.
#[cfg(windows)]
fn status_error(status: u16, ratelimit_remaining: Option<&str>) -> Option<NetError> {
    match status {
        200..=299 => None,
        429 => Some(NetError::RateLimited),
        403 if ratelimit_remaining.map(str::trim) == Some("0") => Some(NetError::RateLimited),
        code => Some(NetError::Http(code)),
    }
}

/// Netzfehler zu einem Ein-/Ausgabefehler.
#[cfg(windows)]
fn io_error(e: &std::io::Error) -> NetError {
    use std::io::ErrorKind as K;
    match e.kind() {
        K::TimedOut | K::WouldBlock => NetError::Timeout,
        K::ConnectionRefused
        | K::ConnectionReset
        | K::ConnectionAborted
        | K::NotConnected
        | K::AddrNotAvailable
        | K::BrokenPipe
        | K::UnexpectedEof
        | K::NetworkUnreachable
        | K::HostUnreachable
        | K::NetworkDown => NetError::Offline,
        _ => NetError::Other(e.to_string()),
    }
}

#[cfg(windows)]
mod windows {
    use super::*;
    use ureq::tls::{RootCerts, TlsConfig, TlsProvider};

    /// Client: Schannel mit dem Zertifikatsspeicher von Windows, nur HTTPS, Statuscodes selbst
    /// auswerten (für das Rate-Limit braucht es die Kopfzeilen). `local`: für die lokale
    /// Test-Quelle ohne HTTPS-Zwang (nur mit dem Schalter `update-test-source` je `true`).
    pub(super) fn agent(user_agent: &str, local: bool) -> ureq::Agent {
        let tls = TlsConfig::builder()
            .provider(TlsProvider::NativeTls)
            .root_certs(RootCerts::PlatformVerifier)
            .build();
        ureq::Agent::config_builder()
            .tls_config(tls)
            .https_only(!local)
            .http_status_as_error(false)
            .user_agent(user_agent)
            .timeout_global(Some(TOTAL_TIMEOUT))
            .timeout_resolve(Some(TIMEOUT))
            .timeout_connect(Some(TIMEOUT))
            .timeout_recv_response(Some(TIMEOUT))
            .timeout_recv_body(Some(TIMEOUT))
            .build()
            .new_agent()
    }

    pub(super) fn fetch(agent: &ureq::Agent, url: &str) -> Result<String, NetError> {
        let mut response = agent
            .get(url)
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .call()
            .map_err(net_error)?;
        let remaining = response
            .headers()
            .get("x-ratelimit-remaining")
            .and_then(|v| v.to_str().ok());
        if let Some(e) = status_error(response.status().as_u16(), remaining) {
            return Err(e);
        }
        response
            .body_mut()
            .with_config()
            .limit(MAX_RESPONSE_BYTES)
            .read_to_string()
            .map_err(net_error)
    }

    /// Client für Downloads: wie [`agent`], aber ohne eigene Weiterleitungen (jede wird in
    /// [`download`] geprüft) und mit Zeitgrenzen für große Dateien. `local` wie bei [`agent`].
    pub(super) fn download_agent(user_agent: &str, local: bool) -> ureq::Agent {
        let tls = TlsConfig::builder()
            .provider(TlsProvider::NativeTls)
            .root_certs(RootCerts::PlatformVerifier)
            .build();
        ureq::Agent::config_builder()
            .tls_config(tls)
            .https_only(!local)
            .http_status_as_error(false)
            .max_redirects(0)
            .user_agent(user_agent)
            .timeout_global(Some(DOWNLOAD_TOTAL_TIMEOUT))
            .timeout_resolve(Some(DOWNLOAD_STEP_TIMEOUT))
            .timeout_connect(Some(DOWNLOAD_STEP_TIMEOUT))
            .timeout_recv_response(Some(DOWNLOAD_STEP_TIMEOUT))
            .build()
            .new_agent()
    }

    pub(super) fn download<'a>(
        agent_for: &dyn Fn(&str) -> &'a ureq::Agent,
        url: &str,
        dest: &std::path::Path,
        max_bytes: u64,
        progress: &mut dyn FnMut(u64, u64) -> bool,
    ) -> Result<(), crate::install::InstallError> {
        use crate::install::{
            InstallError, MAX_REDIRECTS, STALL_TIMEOUT, copy_limited, redirect_target,
        };
        let net = |e: ureq::Error| InstallError::Network(net_error(e).to_string());
        let mut current = url.to_string();
        for _ in 0..=MAX_REDIRECTS {
            if !progress(0, 0) {
                return Err(InstallError::Cancelled);
            }
            let response = agent_for(&current).get(&current).call().map_err(net)?;
            let status = response.status().as_u16();
            if (300..400).contains(&status) {
                let location = response
                    .headers()
                    .get("location")
                    .and_then(|v| v.to_str().ok())
                    .ok_or_else(|| InstallError::Network("Weiterleitung ohne Ziel".into()))?;
                current = redirect_target(&current, location)?;
                continue;
            }
            if let Some(e) = status_error(status, None) {
                return Err(InstallError::Network(e.to_string()));
            }
            let length = response
                .headers()
                .get("content-length")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.trim().parse::<u64>().ok());
            if length.is_some_and(|l| l > max_bytes) {
                return Err(InstallError::TooLarge);
            }
            let reader = response
                .into_body()
                .into_with_config()
                .limit(max_bytes.saturating_add(1))
                .reader();
            return copy_limited(
                reader,
                length.unwrap_or(0),
                dest,
                max_bytes,
                STALL_TIMEOUT,
                progress,
            );
        }
        Err(InstallError::Network("zu viele Weiterleitungen".into()))
    }

    /// Netzfehler zu einem Fehler von `ureq`.
    pub(super) fn net_error(e: ureq::Error) -> NetError {
        match e {
            ureq::Error::Timeout(_) => NetError::Timeout,
            ureq::Error::HostNotFound | ureq::Error::ConnectionFailed => NetError::Offline,
            ureq::Error::StatusCode(code) => {
                status_error(code, None).unwrap_or(NetError::Http(code))
            }
            ureq::Error::Io(io) => io_error(&io),
            ureq::Error::BodyExceedsLimit(_) => NetError::Other("Antwort zu groß".into()),
            other => NetError::Other(other.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_belongs_to_the_fixed_repository() {
        assert_eq!(
            RELEASES_URL,
            format!("https://api.github.com/repos/{REPO}/releases")
        );
    }

    #[cfg(windows)]
    #[test]
    fn status_codes() {
        assert_eq!(status_error(200, None), None);
        assert_eq!(status_error(204, Some("0")), None);
        assert_eq!(status_error(429, None), Some(NetError::RateLimited));
        assert_eq!(status_error(403, Some("0")), Some(NetError::RateLimited));
        assert_eq!(status_error(403, Some(" 0 ")), Some(NetError::RateLimited));
        assert_eq!(status_error(403, Some("12")), Some(NetError::Http(403)));
        assert_eq!(status_error(403, None), Some(NetError::Http(403)));
        assert_eq!(status_error(404, None), Some(NetError::Http(404)));
        assert_eq!(status_error(500, None), Some(NetError::Http(500)));
    }

    #[cfg(windows)]
    #[test]
    fn io_errors() {
        use std::io::{Error, ErrorKind as K};
        assert_eq!(io_error(&Error::from(K::TimedOut)), NetError::Timeout);
        for k in [
            K::ConnectionRefused,
            K::ConnectionReset,
            K::NetworkUnreachable,
        ] {
            assert_eq!(io_error(&Error::from(k)), NetError::Offline, "{k:?}");
        }
        assert!(matches!(
            io_error(&Error::from(K::PermissionDenied)),
            NetError::Other(_)
        ));
    }

    #[cfg(windows)]
    #[test]
    fn ureq_errors() {
        use super::windows::net_error;
        assert_eq!(
            net_error(ureq::Error::Timeout(ureq::Timeout::Connect)),
            NetError::Timeout
        );
        assert_eq!(net_error(ureq::Error::HostNotFound), NetError::Offline);
        assert_eq!(net_error(ureq::Error::ConnectionFailed), NetError::Offline);
        assert_eq!(
            net_error(ureq::Error::StatusCode(429)),
            NetError::RateLimited
        );
        assert_eq!(net_error(ureq::Error::StatusCode(502)), NetError::Http(502));
        assert_eq!(
            net_error(ureq::Error::BodyExceedsLimit(MAX_RESPONSE_BYTES)),
            NetError::Other("Antwort zu groß".into())
        );
    }

    #[cfg(windows)]
    #[test]
    fn agent_uses_windows_tls_and_system_roots() {
        // Ohne diese Einstellung bräche `ureq` beim ersten Abruf ab (keine Mozilla-Wurzeln
        // eingebaut) bzw. nähme rustls, das nicht mitgebaut ist.
        let s = HttpSource::new(&Version::parse("0.2.0-dev.1").unwrap());
        let config = s.agent.config();
        assert_eq!(
            config.tls_config().provider(),
            ureq::tls::TlsProvider::NativeTls
        );
        assert!(matches!(
            config.tls_config().root_certs(),
            ureq::tls::RootCerts::PlatformVerifier
        ));
        assert!(config.https_only());
        assert!(!config.http_status_as_error());
    }

    #[cfg(windows)]
    #[test]
    fn download_agent_follows_no_redirect_by_itself() {
        // Jede Weiterleitung prüft `install::redirect_target`; folgte der Client selbst, ginge
        // er auch zu fremden Adressen.
        let d = HttpDownloader::new(&Version::parse("0.2.0-dev.1").unwrap());
        let config = d.agent.config();
        assert_eq!(config.max_redirects(), 0);
        assert!(config.https_only());
        assert!(!config.http_status_as_error());
        assert_eq!(
            config.tls_config().provider(),
            ureq::tls::TlsProvider::NativeTls
        );
        assert!(matches!(
            config.tls_config().root_certs(),
            ureq::tls::RootCerts::PlatformVerifier
        ));
    }
}
