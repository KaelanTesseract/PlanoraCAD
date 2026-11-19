//! Plug-ins Etappe 1, Task 8: Repo-Link eines Plug-ins lesen, dessen Releases zerlegen, das
//! passende wählen und die Weiterleitungen beim Laden prüfen. Ohne Netz: die Anfragen gehen an
//! eine Nachbildung.

use planora_update::Channel;
use planora_update::plugin_source::{
    Hop, PluginRelease, RepoLink, get_following, parse_plugin_releases, parse_repo_link,
    pick_plugin_release, redirect_target,
};
use semver::Version;

fn link(owner: &str, repo: &str) -> RepoLink {
    RepoLink {
        owner: owner.to_string(),
        repo: repo.to_string(),
    }
}

#[test]
fn link_forms() {
    for text in [
        "https://github.com/a/b",
        "github.com/a/b",
        "https://github.com/a/b/",
        "https://github.com/a/b.git",
        "https://GitHub.COM/a/b",
        "  https://github.com/a/b  ",
    ] {
        assert_eq!(parse_repo_link(text), Ok(link("a", "b")), "{text}");
    }
    let l = parse_repo_link("https://github.com/Besitzer-1/mein_plugin.x").unwrap();
    assert_eq!(l, link("Besitzer-1", "mein_plugin.x"));
    assert_eq!(
        l.releases_url(),
        "https://api.github.com/repos/Besitzer-1/mein_plugin.x/releases"
    );
    assert_eq!(l.web_url(), "https://github.com/Besitzer-1/mein_plugin.x");
}

#[test]
fn link_rejects() {
    for text in [
        "https://gitlab.com/a/b",
        "https://github.com/a",
        "https://github.com/a/b/tree/main",
        "https://github.com/a/b?x=1",
        "https://github.com/a/b#readme",
        "https://u@github.com/a/b",
        "https://github.com:8443/a/b",
        "http://github.com/a/b",
        "ftp://github.com/a/b",
        "https://github.com/../b",
        "https://github.com/a/..",
        "https://github.com/a/x..y",
        "https://github.com//b",
        "https://github.com/a/.git",
        "",
        "   ",
        "https://github.com/a b/c",
        "https://github.com/a/b c",
        "https://github.com/a/b\\c",
        "https://evilgithub.com/a/b",
        "https://github.com.evil.org/a/b",
    ] {
        let e = parse_repo_link(text).expect_err(text);
        assert_eq!(
            e, "Das ist kein GitHub-Repo-Link (https://github.com/besitzer/repo).",
            "{text}"
        );
    }
}

/// Ein Release im Format der GitHub-Antwort.
fn release(tag: &str, draft: bool, pre: bool, assets: &[(&str, &str)]) -> String {
    let assets: Vec<String> = assets
        .iter()
        .map(|(name, url)| {
            format!(r#"{{"name":"{name}","browser_download_url":"{url}","size":1234}}"#)
        })
        .collect();
    format!(
        r#"{{"tag_name":"{tag}","draft":{draft},"prerelease":{pre},"body":"Notiz {tag}","assets":[{}]}}"#,
        assets.join(",")
    )
}

fn url(tag: &str, name: &str) -> String {
    format!("https://github.com/a/b/releases/download/{tag}/{name}")
}

#[test]
fn releases_parse() {
    let ok_pkg = url("v0.3.0", "hallo.pcplugin");
    let ok_sum = url("v0.3.0", "hallo.pcplugin.sha256");
    let list = [
        // Entwurf: übergangen.
        release(
            "v0.9.0",
            true,
            false,
            &[("hallo.pcplugin", &url("v0.9.0", "hallo.pcplugin"))],
        ),
        // Vorabversion: bleibt (der Kanal entscheidet erst beim Wählen).
        release(
            "v0.4.0-beta.1",
            false,
            true,
            &[("hallo.pcplugin", &url("v0.4.0-beta.1", "hallo.pcplugin"))],
        ),
        // Gültig mit Prüfsumme.
        release(
            "v0.3.0",
            false,
            false,
            &[
                ("hallo.pcplugin", &ok_pkg),
                ("hallo.pcplugin.sha256", &ok_sum),
            ],
        ),
        // Ohne Paket.
        release(
            "v0.2.5",
            false,
            false,
            &[("liesmich.txt", &url("v0.2.5", "liesmich.txt"))],
        ),
        // Zwei Pakete.
        release(
            "v0.2.4",
            false,
            false,
            &[
                ("a.pcplugin", &url("v0.2.4", "a.pcplugin")),
                ("b.pcplugin", &url("v0.2.4", "b.pcplugin")),
            ],
        ),
        // Fremde Adresse des Pakets.
        release(
            "v0.2.3",
            false,
            false,
            &[(
                "hallo.pcplugin",
                "https://example.org/a/b/releases/download/v0.2.3/hallo.pcplugin",
            )],
        ),
        // Adresse eines anderen Repos.
        release(
            "v0.2.2",
            false,
            false,
            &[(
                "hallo.pcplugin",
                "https://github.com/x/b/releases/download/v0.2.2/hallo.pcplugin",
            )],
        ),
        // Tag ohne `v`, ohne Prüfsumme.
        release(
            "0.2.0",
            false,
            false,
            &[("hallo.pcplugin", &url("0.2.0", "hallo.pcplugin"))],
        ),
        // Tag ohne Versionsmuster.
        release(
            "neu",
            false,
            false,
            &[("hallo.pcplugin", &url("neu", "hallo.pcplugin"))],
        ),
    ];
    let json = format!("[{}]", list.join(","));
    let found = parse_plugin_releases(&json, &link("a", "b")).unwrap();
    let tags: Vec<&str> = found.iter().map(|r| r.tag.as_str()).collect();
    assert_eq!(tags, ["v0.4.0-beta.1", "v0.3.0", "0.2.0"]);
    let r = &found[1];
    assert_eq!(r.version, Version::parse("0.3.0").unwrap());
    assert_eq!(r.notes, "Notiz v0.3.0");
    assert_eq!(r.package.name, "hallo.pcplugin");
    assert_eq!(r.package.url, ok_pkg);
    assert_eq!(r.package.size, 1234);
    assert_eq!(
        r.sha256.as_ref().map(|s| s.url.as_str()),
        Some(ok_sum.as_str())
    );
    assert!(found[2].sha256.is_none());
    // Groß- und Kleinschreibung des Links zählt nicht (GitHub liefert die eigene Schreibweise).
    let upper = parse_plugin_releases(&json, &link("A", "B")).unwrap();
    assert_eq!(upper.len(), 3);
    // Kein JSON oder keine Liste: ein Fehler.
    assert!(parse_plugin_releases("kein json", &link("a", "b")).is_err());
    assert!(parse_plugin_releases(r#"{"message":"rate"}"#, &link("a", "b")).is_err());
}

fn rel(version: &str) -> PluginRelease {
    let tag = format!("v{version}");
    let json = format!(
        "[{}]",
        release(
            &tag,
            false,
            false,
            &[("p.pcplugin", &url(&tag, "p.pcplugin"))]
        )
    );
    parse_plugin_releases(&json, &link("a", "b"))
        .unwrap()
        .remove(0)
}

#[test]
fn pick_respects_channel_and_installed() {
    let list = vec![
        rel("0.1.0"),
        rel("0.2.0-beta.1"),
        rel("0.2.0-dev.3"),
        rel("0.1.5"),
    ];
    let pick = |installed: Option<&str>, channel| {
        let installed = installed.map(|v| Version::parse(v).unwrap());
        pick_plugin_release(&list, installed.as_ref(), channel).map(|r| r.tag.clone())
    };
    assert_eq!(pick(None, Channel::Stable).as_deref(), Some("v0.1.5"));
    assert_eq!(pick(None, Channel::Beta).as_deref(), Some("v0.2.0-beta.1"));
    assert_eq!(pick(None, Channel::Dev).as_deref(), Some("v0.2.0-dev.3"));
    assert_eq!(
        pick(Some("0.1.0"), Channel::Stable).as_deref(),
        Some("v0.1.5")
    );
    assert_eq!(pick(Some("0.1.5"), Channel::Stable), None);
    assert_eq!(pick(Some("0.3.0"), Channel::Dev), None);
    let newest = vec![rel("0.1.0"), rel("0.2.0")];
    let installed = Version::parse("0.2.0").unwrap();
    assert!(pick_plugin_release(&newest, Some(&installed), Channel::Stable).is_none());
}

#[test]
fn redirect_rule_for_plugin_assets() {
    let asset = url("v0.3.0", "hallo.pcplugin");
    assert_eq!(
        redirect_target(&asset, "https://objects.githubusercontent.com/x/y?sig=1"),
        Ok("https://objects.githubusercontent.com/x/y?sig=1".to_string())
    );
    assert_eq!(
        redirect_target(
            "https://api.github.com/repos/a/b/releases",
            "https://api.github.com/repositories/42/releases"
        ),
        Ok("https://api.github.com/repositories/42/releases".to_string())
    );
    assert_eq!(
        redirect_target(&asset, "/a/b/releases/download/x"),
        Ok("https://github.com/a/b/releases/download/x".to_string())
    );
    for bad in [
        "https://example.org/x",
        "http://objects.githubusercontent.com/x",
        "https://u@objects.githubusercontent.com/x",
        "https://objects.githubusercontent.com:8443/x",
        "ftp://github.com/x",
        "relativ",
    ] {
        assert!(redirect_target(&asset, bad).is_err(), "{bad}");
    }
}

/// Nachbildung: je Adresse eine Antwort; merkt sich die gefragten Adressen.
fn hops(table: Vec<(&'static str, Hop)>) -> impl FnMut(&str, u64) -> Result<Hop, String> {
    move |url: &str, _max: u64| {
        table
            .iter()
            .find(|(u, _)| *u == url)
            .map(|(_, h)| h.clone())
            .ok_or_else(|| format!("unerwartet: {url}"))
    }
}

fn redirect(to: &str) -> Hop {
    Hop {
        status: 302,
        location: Some(to.to_string()),
        body: Vec::new(),
    }
}

fn body(data: &[u8]) -> Hop {
    Hop {
        status: 200,
        location: None,
        body: data.to_vec(),
    }
}

#[test]
fn downloads_follow_checked_redirects() {
    const START: &str = "https://github.com/a/b/releases/download/v1/p.pcplugin";
    const CDN: &str = "https://objects.githubusercontent.com/p";
    let mut net = hops(vec![(START, redirect(CDN)), (CDN, body(b"paket"))]);
    assert_eq!(
        get_following(START, 100, &mut net),
        Ok((200, b"paket".to_vec()))
    );
    // Weiterleitung zu einem fremden Rechner: Fehler, der wird nicht gefragt.
    let mut net = hops(vec![(START, redirect("https://example.org/p"))]);
    assert!(get_following(START, 100, &mut net).is_err());
    // Zu viele Weiterleitungen (mehr als fünf).
    let mut n = 0;
    let mut endless = |_: &str, _: u64| {
        n += 1;
        Ok(redirect(&format!(
            "https://objects.githubusercontent.com/{n}"
        )))
    };
    assert!(get_following(START, 100, &mut endless).is_err());
    assert_eq!(n, 6);
    // 3xx ohne Ziel, fremde Startadresse, zu großer Inhalt.
    let mut net = hops(vec![(
        START,
        Hop {
            status: 302,
            location: None,
            body: Vec::new(),
        },
    )]);
    assert!(get_following(START, 100, &mut net).is_err());
    let mut never = |u: &str, _: u64| -> Result<Hop, String> { panic!("gefragt: {u}") };
    assert!(get_following("http://github.com/a", 100, &mut never).is_err());
    assert!(get_following("https://example.org/a", 100, &mut never).is_err());
    let mut net = hops(vec![(START, body(b"zu lang"))]);
    assert!(get_following(START, 3, &mut net).is_err());
    // Andere Statuscodes kommen als Antwort zurück.
    let mut net = hops(vec![(
        START,
        Hop {
            status: 404,
            location: None,
            body: b"weg".to_vec(),
        },
    )]);
    assert_eq!(
        get_following(START, 100, &mut net),
        Ok((404, b"weg".to_vec()))
    );
}
