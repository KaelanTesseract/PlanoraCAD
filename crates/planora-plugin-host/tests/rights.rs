//! Rechte und Anfragen mit Kennung (Plug-ins Etappe 1, Task 3).

use planora_plugin_host::manifest::{Manifest, Right};
use planora_plugin_host::rights::{
    Answer, Grant, MAX_URL_LEN, Pending, Requests, Rights, Source, check_net_url,
};
use std::collections::BTreeSet;

const MANIFEST: &str = r#"
id = "planora.beispiel-hallo"
name = "Hallo"
version = "0.1.0"
api = "0.1"
author = "Dennis Guse"
license = "GPL-3.0-only"
description = "Sagt Hallo."
rights = ["net"]
net-hosts = ["example.org"]
"#;

fn manifest() -> Manifest {
    Manifest::parse(MANIFEST).unwrap()
}

#[test]
fn once_is_not_recorded() {
    // „Nur diesmal“ erledigt der Host je Anfrage; in den Rechten bleibt nichts davon.
    let mut r = Rights::default();
    r.answer(Right::Net, Answer::Once);
    assert_eq!(r.state(Right::Net), None);
    r.answer(Right::Net, Answer::Deny);
    r.answer(Right::Net, Answer::Once);
    assert_eq!(r.state(Right::Net), Some(Grant::Denied));
}

#[test]
fn allow_is_saved() {
    let mut r = Rights::default();
    r.answer(Right::Net, Answer::Allow);
    assert_eq!(r.state(Right::Net), Some(Grant::Allowed));
}

#[test]
fn deny_saved() {
    let mut r = Rights::default();
    r.answer(Right::ModelWrite, Answer::Deny);
    assert_eq!(r.state(Right::ModelWrite), Some(Grant::Denied));
}

#[test]
fn revoke_returns_to_ask() {
    let mut r = Rights::default();
    r.answer(Right::Net, Answer::Allow);
    r.revoke(Right::Net);
    assert_eq!(r.state(Right::Net), None);
}

#[test]
fn unavailable_rights_denied_without_asking() {
    // `fs-folder`, `model-read`, `model-write` setzt Etappe 1 noch nicht um: abgelehnt, auch
    // wenn deklariert und gespeichert erlaubt.
    let r = Rights::default();
    let all: BTreeSet<Right> = [
        Right::Net,
        Right::FsFolder,
        Right::ModelRead,
        Right::ModelWrite,
    ]
    .into();
    for right in [Right::FsFolder, Right::ModelRead, Right::ModelWrite] {
        assert!(!right.available());
        assert_eq!(r.decide(&all, right), Some(Grant::Denied));
        let mut saved = Rights::default();
        saved.answer(right, Answer::Allow);
        assert_eq!(saved.decide(&all, right), Some(Grant::Denied));
    }
    assert!(Right::Net.available());
    assert_eq!(r.decide(&all, Right::Net), None);
}

#[test]
fn carry_over_needs_same_source() {
    let m = manifest();
    let mut r = Rights::default();
    r.answer(Right::Net, Answer::Allow);
    let link = Source::link("https://github.com/A/hallo");
    let same =
        |old: &Source, new: &Source| r.carry_over(Some((&m, old)), &m, new).state(Right::Net);
    assert_eq!(
        same(&Source::Bundled, &Source::Bundled),
        Some(Grant::Allowed)
    );
    assert_eq!(same(&Source::File, &Source::File), Some(Grant::Allowed));
    assert_eq!(
        same(&link, &Source::link("https://github.com/a/HALLO")),
        Some(Grant::Allowed)
    );
    // Andere Herkunft: die Rechte fallen.
    assert_eq!(same(&Source::File, &link), None);
    assert_eq!(
        same(&link, &Source::link("https://github.com/b/hallo")),
        None
    );
    assert_eq!(same(&Source::Bundled, &Source::File), None);
    // Aus der Konfiguration: ohne gespeicherte Herkunft fällt alles.
    let from = |source: Option<&Source>| {
        r.carry_over_from(Some("Dennis Guse"), &m.net_hosts, source, &m, &Source::File)
            .state(Right::Net)
    };
    assert_eq!(from(Some(&Source::File)), Some(Grant::Allowed));
    assert_eq!(from(None), None);
    // Die Form in der Konfiguration.
    for s in [Source::Bundled, Source::File, link.clone()] {
        assert_eq!(Source::from_config(&s.to_config()), s);
    }
}

#[test]
fn undeclared_right_always_denied() {
    let mut r = Rights::default();
    let declared: BTreeSet<Right> = manifest().rights.clone();
    // Nicht deklariert: sofort abgelehnt, auch wenn gespeichert erlaubt.
    r.answer(Right::ModelWrite, Answer::Allow);
    assert_eq!(r.decide(&declared, Right::ModelWrite), Some(Grant::Denied));
    // Deklariert und unbeantwortet: nachfragen.
    assert_eq!(r.decide(&declared, Right::Net), None);
    r.answer(Right::Net, Answer::Allow);
    assert_eq!(r.decide(&declared, Right::Net), Some(Grant::Allowed));
}

#[test]
fn requests_count_from_one() {
    let mut q = Requests::default();
    assert_eq!(q.next(), 1);
    assert_eq!(q.next(), 2);
    assert_eq!(Requests::default().next(), 1);
    let p = Pending::AskRight {
        request: 1,
        right: Right::Net,
    };
    assert_eq!(p.clone(), p);
}

#[test]
fn net_url_rules() {
    let m = manifest();
    assert!(check_net_url(&m, "https://example.org/a?b").is_ok());
    assert!(check_net_url(&m, "HTTPS://Example.ORG").is_ok());
    assert!(check_net_url(&m, "https://example.org:443/x").is_ok());
    assert!(check_net_url(&m, "https://example.org./x").is_ok());
    for bad in [
        "http://example.org",
        "https://evil.org",
        "https://example.org.evil.org",
        "https://u:p@example.org",
        "https://u@example.org",
        "https://example.org:8443",
        "https://example.org:/x",
        "https://example.org@evil.org",
        "https://evil.org#@example.org",
        "https://evil.org?@example.org",
        "https://[::1]/",
        "https://",
        "example.org",
        "ftp://example.org",
        "https://example.org\\@evil.org",
        "https://example.org:443:443",
    ] {
        assert!(check_net_url(&m, bad).is_err(), "{bad}");
    }
}

#[test]
fn long_urls_rejected() {
    let m = manifest();
    let url = |len: usize| {
        let base = "https://example.org/";
        format!("{base}{}", "a".repeat(len - base.len()))
    };
    assert!(check_net_url(&m, &url(MAX_URL_LEN)).is_ok());
    let err = check_net_url(&m, &url(MAX_URL_LEN + 1)).unwrap_err();
    assert!(err.contains("8192"), "{err}");
}
