//! Echter Abruf der Release-Liste (nur Windows), Downloads und Anfragen für Plug-ins (Windows
//! und Unix) über HTTPS.
//!
//! Client ist `ureq` mit der TLS-Schicht des Systems über `native-tls`: unter Windows Schannel
//! mit dem Zertifikatsspeicher von Windows, unter Linux das OpenSSL des Systems mit dessen
//! Stammzertifikaten. Keine Anmeldung, keine Nutzerdaten: gesendet wird nur die Anfrage selbst
//! mit der Kennung `PlanoraCAD/<Version>`. Die Release-Liste dient nur dem Update des Programms,
//! und das gibt es nur unter Windows (Installer); außerhalb von Windows liefert [`HttpSource`]
//! darum immer [`NetError::Unsupported`], ohne ins Netz zu gehen. [`HttpDownloader`],
//! [`get_limited`] und [`get_once`] gehen auch unter Unix ins Netz.

use std::time::Duration;

use planora_i18n::tr;

use crate::check::{NetError, ReleaseSource};
use crate::plugin_source::Hop;
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
            agent: tls::agent(&user_agent, crate::install::local_test_url_ok(&url)),
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
        tls::fetch(&self.agent, &self.url)
    }

    #[cfg(not(windows))]
    fn fetch_releases(&self) -> Result<String, NetError> {
        Err(NetError::Unsupported)
    }
}

/// Zeitgrenze je Abschnitt eines Downloads (Namensauflösung, Verbindung, Antwortkopf).
#[cfg(any(windows, unix))]
const DOWNLOAD_STEP_TIMEOUT: Duration = crate::install::STALL_TIMEOUT;

/// Obergrenze für einen ganzen Download samt Weiterleitungen; beendet auch einen hängenden
/// Lese-Thread, dessen Download schon wegen Stillstand oder Abbruch aufgegeben wurde.
#[cfg(any(windows, unix))]
const DOWNLOAD_TOTAL_TIMEOUT: Duration = Duration::from_secs(60 * 60);

/// Download einer Datei eines Releases über HTTPS (Windows und Unix, sonst Fehler „nur Windows“).
///
/// Nur Adressen aus dem Release ([`crate::install::release_url_ok`]); Weiterleitungen folgt der
/// Client nicht selbst, sondern jede wird geprüft ([`crate::install::redirect_target`]: nur
/// HTTPS auf `github.com` und `*.githubusercontent.com`, höchstens
/// [`crate::install::MAX_REDIRECTS`]). Gelesen wird über [`crate::install::copy_limited`] mit
/// Größen- und Stillstandsgrenze. Wird nur im Programm angelegt, nie in Tests benutzt.
pub struct HttpDownloader {
    #[cfg(any(windows, unix))]
    agent: ureq::Agent,
    /// Client ohne HTTPS-Zwang, nur für die lokale Test-Quelle.
    #[cfg(all(any(windows, unix), feature = "update-test-source"))]
    local_agent: ureq::Agent,
}

impl HttpDownloader {
    /// Client mit der Kennung `PlanoraCAD/<running>`; verbindet noch nicht.
    pub fn new(running: &Version) -> Self {
        #[cfg(not(any(windows, unix)))]
        let _ = running;
        HttpDownloader {
            #[cfg(any(windows, unix))]
            agent: tls::download_agent(&format!("PlanoraCAD/{running}"), false),
            #[cfg(all(any(windows, unix), feature = "update-test-source"))]
            local_agent: tls::download_agent(&format!("PlanoraCAD/{running}"), true),
        }
    }

    /// Der Client für `url`: für die lokale Test-Quelle der ohne HTTPS-Zwang, sonst der normale.
    #[cfg(any(windows, unix))]
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
            return Err(InstallError::Network(tr!("update-detail-foreign-url")));
        }
        #[cfg(any(windows, unix))]
        {
            tls::download(&|u| self.agent_for(u), url, dest, max_bytes, progress)
        }
        #[cfg(not(any(windows, unix)))]
        {
            let _ = (dest, max_bytes, progress);
            Err(InstallError::Network(tr!("update-detail-only-windows")))
        }
    }
}

/// Kennung der Anfragen von Plug-ins (wie die des Programms, ohne weitere Angaben).
const PLUGIN_USER_AGENT: &str = concat!("PlanoraCAD/", env!("CARGO_PKG_VERSION"));

/// Ein GET für ein Plug-in (Windows und Unix, sonst Fehler „nur Windows“): genau die Adresse `url`,
/// nur HTTPS, ohne Weiterleitungen (eine 3xx-Antwort kommt als Antwort zurück), höchstens `max`
/// Bytes Inhalt, `timeout` für die ganze Anfrage. Jeder Statuscode ist eine Antwort (Status,
/// Inhalt); ein Fehler ist nur Netz, Zeit oder Größe. Ob die Adresse erlaubt ist, prüft der
/// Aufrufer (für Plug-ins der Host gegen das Manifest). Wartet: nur in einem Arbeitsthread rufen.
pub fn get_limited(url: &str, max: u64, timeout: Duration) -> Result<(u16, Vec<u8>), String> {
    #[cfg(any(windows, unix))]
    {
        tls::get_limited(url, max, timeout)
    }
    #[cfg(not(any(windows, unix)))]
    {
        let _ = (url, max, timeout);
        Err(tr!("update-detail-only-windows"))
    }
}

/// Eine einzelne Anfrage ohne Weiterleitung für Downloads von Plug-ins (Windows und Unix, sonst
/// Fehler „nur Windows“): wie [`get_limited`], dazu die Kopfzeile `Location` einer 3xx-Antwort
/// (deren Inhalt wird nicht gelesen). Weiterleitungen prüft und verfolgt
/// [`crate::plugin_source::get_following`]. Wartet: nur in einem Arbeitsthread rufen.
pub fn get_once(url: &str, max: u64, timeout: Duration) -> Result<Hop, String> {
    #[cfg(any(windows, unix))]
    {
        tls::get_once(url, max, timeout)
    }
    #[cfg(not(any(windows, unix)))]
    {
        let _ = (url, max, timeout);
        Err(tr!("update-detail-only-windows"))
    }
}

/// Fehler zu einem Statuscode, `None` bei Erfolg (2xx). GitHub meldet das Rate-Limit mit 429
/// oder mit 403 und `x-ratelimit-remaining: 0`.
#[cfg(any(windows, unix))]
fn status_error(status: u16, ratelimit_remaining: Option<&str>) -> Option<NetError> {
    match status {
        200..=299 => None,
        429 => Some(NetError::RateLimited),
        403 if ratelimit_remaining.map(str::trim) == Some("0") => Some(NetError::RateLimited),
        code => Some(NetError::Http(code)),
    }
}

/// Netzfehler zu einem Ein-/Ausgabefehler.
#[cfg(any(windows, unix))]
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

#[cfg(any(windows, unix))]
mod tls {
    use super::*;
    use ureq::tls::{RootCerts, TlsConfig, TlsProvider};

    /// Client: TLS des Systems (Schannel bzw. OpenSSL) mit dessen Stammzertifikaten, nur HTTPS,
    /// Statuscodes selbst auswerten (für das Rate-Limit braucht es die Kopfzeilen). `local`: für
    /// die lokale Test-Quelle ohne HTTPS-Zwang (nur mit dem Schalter `update-test-source` je
    /// `true`). Nur unter Windows (die Release-Liste dient nur dem Update des Programms).
    #[cfg(windows)]
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

    #[cfg(windows)]
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
                    .ok_or_else(|| {
                        InstallError::Network(tr!("update-detail-redirect-no-target"))
                    })?;
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
        Err(InstallError::Network(tr!(
            "update-detail-too-many-redirects"
        )))
    }

    /// Client für Plug-ins: nur HTTPS, keine Weiterleitungen, Statuscodes als Antwort, eine
    /// Zeitgrenze für die ganze Anfrage.
    pub(super) fn plugin_agent(timeout: Duration) -> ureq::Agent {
        let tls = TlsConfig::builder()
            .provider(TlsProvider::NativeTls)
            .root_certs(RootCerts::PlatformVerifier)
            .build();
        ureq::Agent::config_builder()
            .tls_config(tls)
            .https_only(true)
            .http_status_as_error(false)
            .max_redirects(0)
            .user_agent(PLUGIN_USER_AGENT)
            .timeout_global(Some(timeout))
            .build()
            .new_agent()
    }

    pub(super) fn get_limited(
        url: &str,
        max: u64,
        timeout: Duration,
    ) -> Result<(u16, Vec<u8>), String> {
        let text = |e: ureq::Error| net_error(e).to_string();
        let mut response = plugin_agent(timeout).get(url).call().map_err(text)?;
        let status = response.status().as_u16();
        let body = response
            .body_mut()
            .with_config()
            .limit(max)
            .read_to_vec()
            .map_err(text)?;
        Ok((status, body))
    }

    pub(super) fn get_once(url: &str, max: u64, timeout: Duration) -> Result<Hop, String> {
        let text = |e: ureq::Error| net_error(e).to_string();
        let mut response = plugin_agent(timeout).get(url).call().map_err(text)?;
        let status = response.status().as_u16();
        if (300..400).contains(&status) {
            let location = response
                .headers()
                .get("location")
                .and_then(|v| v.to_str().ok())
                .map(str::to_string);
            return Ok(Hop {
                status,
                location,
                body: Vec::new(),
            });
        }
        let body = response
            .body_mut()
            .with_config()
            .limit(max)
            .read_to_vec()
            .map_err(text)?;
        Ok(Hop {
            status,
            location: None,
            body,
        })
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
            ureq::Error::BodyExceedsLimit(_) => NetError::Other(tr!("update-net-too-big")),
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

    #[cfg(any(windows, unix))]
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

    #[cfg(any(windows, unix))]
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

    #[cfg(any(windows, unix))]
    #[test]
    fn ureq_errors() {
        use super::tls::net_error;
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

    #[cfg(any(windows, unix))]
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

    #[cfg(any(windows, unix))]
    #[test]
    fn plugin_agent_follows_no_redirect_and_keeps_limits() {
        // Ein Plug-in darf nur zu Rechnern aus seinem Manifest; eine Weiterleitung ginge woandershin.
        let a = tls::plugin_agent(Duration::from_secs(30));
        let config = a.config();
        assert_eq!(config.max_redirects(), 0);
        assert!(config.https_only());
        assert!(!config.http_status_as_error());
        assert_eq!(config.timeouts().global, Some(Duration::from_secs(30)));
        assert_eq!(
            config.tls_config().provider(),
            ureq::tls::TlsProvider::NativeTls
        );
    }

    #[test]
    fn get_limited_refuses_plain_http() {
        // Nur HTTPS (unter Windows lehnt der Client ab, sonst gibt es gar kein Netz).
        assert!(get_limited("http://127.0.0.1:9/", 16, Duration::from_secs(1)).is_err());
        assert!(get_once("http://127.0.0.1:9/", 16, Duration::from_secs(1)).is_err());
    }
}
