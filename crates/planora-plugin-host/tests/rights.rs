//! Rechte und Anfragen mit Kennung (Plug-ins Etappe 1, Task 3).

use planora_plugin_host::manifest::{Manifest, Right};
use planora_plugin_host::rights::{Answer, Grant, Pending, Requests, Rights, check_net_url};
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
fn once_lasts_until_reset() {
    let mut r = Rights::default();
    assert_eq!(r.state(Right::Net), None);
    r.answer(Right::Net, Answer::Once);
    assert_eq!(r.state(Right::Net), Some(Grant::Allowed));
    r.reset_once();
    assert_eq!(r.state(Right::Net), None);
}

#[test]
fn allow_is_saved() {
    let mut r = Rights::default();
    r.answer(Right::Net, Answer::Allow);
    r.reset_once();
    assert_eq!(r.state(Right::Net), Some(Grant::Allowed));
}

#[test]
fn deny_saved() {
    let mut r = Rights::default();
    r.answer(Right::ModelWrite, Answer::Deny);
    r.reset_once();
    assert_eq!(r.state(Right::ModelWrite), Some(Grant::Denied));
}

#[test]
fn once_overrides_saved_deny_only_until_reset() {
    let mut r = Rights::default();
    r.answer(Right::Net, Answer::Deny);
    r.answer(Right::Net, Answer::Once);
    assert_eq!(r.state(Right::Net), Some(Grant::Allowed));
    r.reset_once();
    assert_eq!(r.state(Right::Net), Some(Grant::Denied));
}

#[test]
fn revoke_returns_to_ask() {
    let mut r = Rights::default();
    r.answer(Right::Net, Answer::Allow);
    r.answer(Right::Net, Answer::Once);
    r.revoke(Right::Net);
    assert_eq!(r.state(Right::Net), None);
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
