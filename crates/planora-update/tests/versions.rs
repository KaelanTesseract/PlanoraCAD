use planora_update::{Channel, Pre, PreKind, Version};

fn v(s: &str) -> Version {
    Version::parse(s).unwrap()
}

#[test]
fn ordering_follows_semver_with_dev_and_beta() {
    assert!(v("0.2.0-dev.1") < v("0.2.0-dev.2"));
    assert!(v("0.2.0-dev.2") < v("0.2.0-beta.1"));
    assert!(v("0.2.0-beta.1") < v("0.2.0"));
    assert!(v("0.1.9") < v("0.2.0-dev.1"));
    assert!(v("0.2.0") < v("0.2.1-dev.1"));
    assert_eq!(v("0.2.0-dev.1"), v("v0.2.0-dev.1"));
    let mut list = [
        v("0.2.0"),
        v("0.2.0-beta.1"),
        v("0.2.0-dev.2"),
        v("0.2.0-dev.1"),
    ];
    list.sort();
    let shown: Vec<String> = list.iter().map(ToString::to_string).collect();
    assert_eq!(
        shown,
        ["0.2.0-dev.1", "0.2.0-dev.2", "0.2.0-beta.1", "0.2.0"]
    );
}

#[test]
fn numbers_compare_numerically() {
    assert!(v("0.10.0") > v("0.9.0"));
    assert!(v("1.0.0") > v("0.99.99"));
    assert!(v("0.2.10") > v("0.2.9"));
    assert!(v("0.2.0-dev.10") > v("0.2.0-dev.9"));
    assert!(v("0.2.0-beta.10") > v("0.2.0-beta.9"));
}

#[test]
fn parse_accepts_leading_v_and_rejects_garbage() {
    let parsed = v("v0.2.0-dev.1");
    assert_eq!(
        parsed,
        Version {
            major: 0,
            minor: 2,
            patch: 0,
            pre: Some(Pre {
                kind: PreKind::Dev,
                number: 1
            }),
        }
    );
    assert_eq!(
        v("0.9.0-beta.2").pre,
        Some(Pre {
            kind: PreKind::Beta,
            number: 2
        })
    );
    assert_eq!(v("1.0.0").pre, None);
    for bad in [
        "",
        "v",
        "0.2",
        "0.2.0-rc.1",
        "abc",
        "0.2.0-dev",
        "0.2.0-dev.",
        "0.2.0-dev.x",
        "0.2.0-dev.1.2",
        "0.2.0-DEV.1",
        "0.2.0+build",
        "0.2.0.1",
        " 0.2.0",
        "0.2.0 ",
        "vv0.2.0",
        "V0.2.0",
        "-1.2.3",
        "+1.2.3",
        "1..3",
        "01.2.3",
        "0.2.0-dev.01",
        "99999999999999999999999.0.0",
    ] {
        assert!(
            Version::parse(bad).is_err(),
            "sollte abgelehnt werden: {bad:?}"
        );
    }
    // Die Fehlermeldung nennt die Eingabe.
    let err = Version::parse("abc").unwrap_err();
    assert!(err.to_string().contains("abc"));
}

#[test]
fn display_roundtrips() {
    for s in [
        "0.2.0-dev.1",
        "0.9.0-beta.2",
        "1.0.0",
        "12.345.6789-dev.10",
        "0.0.0",
    ] {
        assert_eq!(v(s).to_string(), s);
        assert_eq!(v(&v(s).to_string()), v(s));
    }
    assert_eq!(v("v1.2.3").to_string(), "1.2.3");
    assert_eq!(Version::from_cargo("0.1.0"), v("0.1.0"));
}

#[test]
fn channel_of_version() {
    assert_eq!(v("0.2.0-dev.1").channel(), Channel::Dev);
    assert_eq!(v("0.9.0-beta.2").channel(), Channel::Beta);
    assert_eq!(v("1.0.0").channel(), Channel::Stable);
}

#[test]
fn channel_includes_rules() {
    let dev = v("0.2.0-dev.1");
    let beta = v("0.9.0-beta.2");
    let stable = v("1.0.0");
    assert!(Channel::Dev.includes(&dev));
    assert!(Channel::Dev.includes(&beta));
    assert!(Channel::Dev.includes(&stable));
    assert!(!Channel::Beta.includes(&dev));
    assert!(Channel::Beta.includes(&beta));
    assert!(Channel::Beta.includes(&stable));
    assert!(!Channel::Stable.includes(&dev));
    assert!(!Channel::Stable.includes(&beta));
    assert!(Channel::Stable.includes(&stable));
}

#[test]
fn channel_ids_roundtrip_and_unknown_is_none() {
    assert_eq!(Channel::ALL, [Channel::Dev, Channel::Beta, Channel::Stable]);
    for c in Channel::ALL {
        assert_eq!(Channel::from_id(c.id()), Some(c));
    }
    assert_eq!(Channel::Dev.id(), "dev");
    assert_eq!(Channel::Beta.id(), "beta");
    assert_eq!(Channel::Stable.id(), "stable");
    assert_eq!(Channel::Dev.label(), "Dev");
    assert_eq!(Channel::Beta.label(), "Beta");
    assert_eq!(Channel::Stable.label(), "Stabil");
    for unknown in ["", "xyz", "Dev", "STABLE", "stabil", " dev"] {
        assert_eq!(Channel::from_id(unknown), None, "{unknown:?}");
    }
}
