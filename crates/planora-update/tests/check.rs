//! Prüfung auf neue Versionen ohne Netz: eine Nachbildung der Quelle liefert die Antwort oder
//! einen Netzfehler; dazu der 24-Stunden-Takt. Die echte Quelle wird nur angelegt, nie benutzt.

use std::sync::Arc;

use planora_update::check::{
    CHECK_INTERVAL_SECS, CheckOutcome, Checker, HttpSource, MAX_RESPONSE_BYTES, NetError,
    RELEASES_URL, ReleaseSource, TIMEOUT, due,
};
use planora_update::{Channel, EXE_NAME, REPO, SHA256_NAME, Version};
use serde_json::{Value, json};

fn v(s: &str) -> Version {
    Version::parse(s).unwrap()
}

fn release_json(tag: &str) -> Value {
    let asset = |name: &str| {
        json!({
            "name": name,
            "browser_download_url": format!("https://github.com/{REPO}/releases/download/{tag}/{name}"),
            "size": 10,
        })
    };
    json!({
        "tag_name": tag,
        "draft": false,
        "body": format!("Notiz {tag}"),
        "assets": [asset(EXE_NAME), asset(SHA256_NAME)],
    })
}

fn list(tags: &[&str]) -> String {
    Value::Array(tags.iter().map(|t| release_json(t)).collect()).to_string()
}

/// Nachbildung der Quelle: liefert immer dieselbe Antwort.
struct Fake(Result<String, NetError>);

impl ReleaseSource for Fake {
    fn fetch_releases(&self) -> Result<String, NetError> {
        self.0.clone()
    }
}

fn checker(answer: Result<String, NetError>) -> Checker<Fake> {
    Checker::new(Fake(answer))
}

#[test]
fn up_to_date_when_running_is_latest() {
    let c = checker(Ok(list(&["v0.2.0-dev.2", "v0.2.0-dev.1"])));
    assert_eq!(
        c.check(&v("0.2.0-dev.2"), Channel::Dev, None),
        CheckOutcome::UpToDate
    );
    // Eine neuere laufende Version (etwa selbst gebaut) ist ebenfalls aktuell.
    assert_eq!(
        c.check(&v("0.2.0-dev.5"), Channel::Dev, None),
        CheckOutcome::UpToDate
    );
    // Die einzige höhere Version ist übersprungen.
    assert_eq!(
        c.check(&v("0.2.0-dev.1"), Channel::Dev, Some(&v("0.2.0-dev.2"))),
        CheckOutcome::UpToDate
    );
}

#[test]
fn available_when_higher_in_channel() {
    let c = checker(Ok(list(&["v0.2.0-dev.3", "v0.2.0-dev.2", "v0.2.0-dev.1"])));
    let CheckOutcome::Available(update) = c.check(&v("0.2.0-dev.1"), Channel::Dev, None) else {
        panic!("Update erwartet");
    };
    assert_eq!(update.latest.version, v("0.2.0-dev.3"));
    assert_eq!(
        update
            .notes_between
            .iter()
            .map(|(v, _)| v.to_string())
            .collect::<Vec<_>>(),
        ["0.2.0-dev.3", "0.2.0-dev.2"]
    );
    // Kanal Stabil: Dev-Versionen zählen nicht.
    assert_eq!(
        c.check(&v("0.1.0"), Channel::Stable, None),
        CheckOutcome::UpToDate
    );
    // Über die Nachbildung hinter `Arc` (so benutzt es die App).
    let shared: Arc<dyn ReleaseSource + Send + Sync> = Arc::new(Fake(Ok(list(&["v0.2.0-dev.3"]))));
    let c = Checker::new(shared);
    assert!(matches!(
        c.check(&v("0.2.0-dev.1"), Channel::Dev, None),
        CheckOutcome::Available(_)
    ));
}

#[test]
fn network_errors_become_failed_not_panics() {
    for err in [
        NetError::Offline,
        NetError::Timeout,
        NetError::RateLimited,
        NetError::Http(500),
        NetError::Http(404),
        NetError::Other("kaputt".into()),
    ] {
        let c = checker(Err(err.clone()));
        assert_eq!(
            c.check(&v("0.2.0-dev.1"), Channel::Dev, None),
            CheckOutcome::Failed(err.clone())
        );
        // Jede Fehlerart hat einen lesbaren deutschen Text.
        assert!(!err.to_string().is_empty());
    }
}

#[test]
fn broken_json_is_failed() {
    for answer in [
        "{kaputt",
        "",
        // Rate-Limit-Meldung von GitHub mit Status 200 wäre keine Liste.
        r#"{"message":"API rate limit exceeded"}"#,
    ] {
        let c = checker(Ok(answer.into()));
        assert!(
            matches!(
                c.check(&v("0.2.0-dev.1"), Channel::Dev, None),
                CheckOutcome::Failed(NetError::Other(_))
            ),
            "{answer}"
        );
    }
}

#[test]
fn empty_list_is_up_to_date() {
    let c = checker(Ok("[]".into()));
    assert_eq!(
        c.check(&v("0.2.0-dev.1"), Channel::Dev, None),
        CheckOutcome::UpToDate
    );
    // Nur unbrauchbare Einträge (ohne Programmdatei): ebenso.
    let c = checker(Ok(
        json!([{ "tag_name": "v9.0.0", "assets": [] }]).to_string()
    ));
    assert_eq!(
        c.check(&v("0.2.0-dev.1"), Channel::Dev, None),
        CheckOutcome::UpToDate
    );
}

#[test]
fn due_logic() {
    const H: u64 = 3600;
    let now = 1_761_400_000u64;
    assert_eq!(CHECK_INTERVAL_SECS, 86_400);
    // Nie geprüft.
    assert!(due(now, None, true, false));
    // Vor 23 Stunden: noch nicht.
    assert!(!due(now, Some(now - 23 * H), true, false));
    // Gerade eben: nicht.
    assert!(!due(now, Some(now), true, false));
    // Genau 24 Stunden: fällig.
    assert!(due(now, Some(now - 24 * H), true, false));
    // Vor 25 Stunden: fällig.
    assert!(due(now, Some(now - 25 * H), true, false));
    // Manuell: immer, auch gerade eben und mit Schalter aus.
    assert!(due(now, Some(now), true, true));
    assert!(due(now, Some(now), false, true));
    assert!(due(now, None, false, true));
    // Schalter aus: nie von allein.
    assert!(!due(now, None, false, false));
    assert!(!due(now, Some(now - 25 * H), false, false));
    // Zeitpunkt in der Zukunft (Uhr verstellt): fällig.
    assert!(due(now, Some(now + H), true, false));
    // Unsinnige Werte stürzen nicht ab.
    assert!(due(0, Some(u64::MAX), true, false));
    assert!(due(u64::MAX, Some(0), true, false));
}

#[test]
fn http_source_is_built_without_network() {
    // Nur anlegen: die Werte stimmen, ein Abruf findet nicht statt.
    let s = HttpSource::new(&v("0.2.0-dev.1"));
    assert_eq!(
        RELEASES_URL,
        "https://api.github.com/repos/KaelanTesseract/PlanoraCAD/releases"
    );
    assert_eq!(s.url(), RELEASES_URL);
    assert_eq!(s.user_agent(), "PlanoraCAD/0.2.0-dev.1");
    assert_eq!(TIMEOUT, std::time::Duration::from_secs(10));
    assert_eq!(MAX_RESPONSE_BYTES, 2 * 1024 * 1024);
    // Passt hinter die Schnittstelle der App.
    let _shared: Arc<dyn ReleaseSource + Send + Sync> = Arc::new(s);
}

/// Außerhalb von Windows fragt die Quelle nie im Netz, sondern meldet, dass es hier keine
/// Aktualisierung gibt (nicht „kein Netz“).
#[cfg(not(windows))]
#[test]
fn http_source_outside_windows_reports_unsupported() {
    let s = HttpSource::new(&v("0.2.0-dev.1"));
    assert_eq!(s.fetch_releases(), Err(NetError::Unsupported));
}
