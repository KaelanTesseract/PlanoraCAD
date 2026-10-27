use planora_update::{
    Channel, EXE_NAME, LICENSES_NAME, MAX_NOTES_CHARS, MAX_NOTES_RELEASES, REPO, Release,
    SHA256_NAME, Version, parse_releases, pick_update,
};
use serde_json::{Value, json};

fn v(s: &str) -> Version {
    Version::parse(s).unwrap()
}

fn url(tag: &str, name: &str) -> String {
    format!("https://github.com/{REPO}/releases/download/{tag}/{name}")
}

fn asset(tag: &str, name: &str, size: u64) -> Value {
    json!({ "name": name, "browser_download_url": url(tag, name), "size": size })
}

/// Ein vollständiges Release im GitHub-Format.
fn release_json(tag: &str, body: &str) -> Value {
    json!({
        "tag_name": tag,
        "draft": false,
        "prerelease": tag.contains('-'),
        "body": body,
        "assets": [
            asset(tag, EXE_NAME, 12_345_678),
            asset(tag, SHA256_NAME, 64),
            asset(tag, LICENSES_NAME, 999),
        ],
    })
}

fn parse(list: &[Value]) -> Vec<Release> {
    parse_releases(&Value::Array(list.to_vec()).to_string()).unwrap()
}

/// Releases zu den Versionen, Notiz jeweils „Notiz <Version>“.
fn releases(tags: &[&str]) -> Vec<Release> {
    let list: Vec<Value> = tags
        .iter()
        .map(|t| release_json(t, &format!("Notiz {t}")))
        .collect();
    parse(&list)
}

#[test]
fn parses_a_github_list() {
    let mut extra = release_json("v0.2.0-dev.1", "## Neu\n- Etwas");
    // Unbekannte Felder und fremde Zusatzdateien stören nicht.
    extra["html_url"] =
        json!("https://github.com/KaelanTesseract/PlanoraCAD/releases/tag/v0.2.0-dev.1");
    extra["assets"]
        .as_array_mut()
        .unwrap()
        .push(asset("v0.2.0-dev.1", "readme.txt", 3));
    let mut no_licenses = release_json("v0.2.0-dev.2", "");
    no_licenses["assets"].as_array_mut().unwrap().pop();
    no_licenses["body"] = Value::Null;
    let list = parse(&[extra, no_licenses]);
    assert_eq!(list.len(), 2);

    let first = &list[0];
    assert_eq!(first.version, v("0.2.0-dev.1"));
    assert_eq!(first.tag, "v0.2.0-dev.1");
    assert_eq!(first.notes, "## Neu\n- Etwas");
    assert_eq!(first.exe.name, EXE_NAME);
    assert_eq!(first.exe.url, url("v0.2.0-dev.1", EXE_NAME));
    assert_eq!(first.exe.size, 12_345_678);
    assert_eq!(first.sha256.name, SHA256_NAME);
    assert_eq!(first.sha256.size, 64);
    let licenses = first.licenses.as_ref().unwrap();
    assert_eq!(licenses.name, LICENSES_NAME);
    assert_eq!(licenses.url, url("v0.2.0-dev.1", LICENSES_NAME));

    let second = &list[1];
    assert_eq!(second.version, v("0.2.0-dev.2"));
    assert_eq!(second.notes, "");
    assert!(second.licenses.is_none());
}

#[test]
fn skips_drafts_missing_files_unknown_versions_and_foreign_urls() {
    let good = release_json("v0.2.0-dev.1", "gut");

    let mut draft = release_json("v0.2.0-dev.2", "Entwurf");
    draft["draft"] = json!(true);

    let mut no_exe = release_json("v0.2.0-dev.3", "");
    no_exe["assets"].as_array_mut().unwrap().remove(0);

    let mut no_sha = release_json("v0.2.0-dev.4", "");
    no_sha["assets"].as_array_mut().unwrap().remove(1);

    let rc = release_json("v0.2.0-rc.1", "unbekannte Endung");
    let no_tag = json!({ "draft": false, "assets": [] });

    let mut foreign = release_json("v0.2.0-dev.5", "");
    foreign["assets"][0]["browser_download_url"] = json!(format!(
        "https://example.com/download/v0.2.0-dev.5/{EXE_NAME}"
    ));

    let mut other_repo = release_json("v0.2.0-dev.6", "");
    other_repo["assets"][1]["browser_download_url"] = json!(format!(
        "https://github.com/Someone/PlanoraCAD/releases/download/v0.2.0-dev.6/{SHA256_NAME}"
    ));

    let mut http = release_json("v0.2.0-dev.7", "");
    http["assets"][0]["browser_download_url"] = json!(format!(
        "http://github.com/{REPO}/releases/download/v0.2.0-dev.7/{EXE_NAME}"
    ));

    let mut other_tag = release_json("v0.2.0-dev.8", "");
    other_tag["assets"][0]["browser_download_url"] = json!(url("v0.1.0", EXE_NAME));

    let mut wrong_case = release_json("v0.2.0-dev.9", "");
    wrong_case["assets"][0]["name"] = json!("PlanoraCAD-windows-x64.exe");

    let mut no_size = release_json("v0.2.0-dev.10", "");
    no_size["assets"][0].as_object_mut().unwrap().remove("size");

    let mut bad_type = release_json("v0.2.0-dev.11", "");
    bad_type["tag_name"] = json!(42);

    let not_an_object = json!("Text statt Release");

    // Lizenzdatei mit fremder Adresse: das Release bleibt, nur die Lizenzdatei fehlt.
    let mut foreign_licenses = release_json("v0.2.0-dev.12", "");
    foreign_licenses["assets"][2]["browser_download_url"] =
        json!("https://example.com/THIRD-PARTY-LICENSES.txt");

    let list = parse(&[
        good,
        draft,
        no_exe,
        no_sha,
        rc,
        no_tag,
        foreign,
        other_repo,
        http,
        other_tag,
        wrong_case,
        no_size,
        bad_type,
        not_an_object,
        foreign_licenses,
    ]);
    let versions: Vec<String> = list.iter().map(|r| r.version.to_string()).collect();
    assert_eq!(versions, ["0.2.0-dev.1", "0.2.0-dev.12"]);
    assert!(list[1].licenses.is_none());
}

#[test]
fn two_exe_assets_with_the_exact_name_are_ambiguous_and_skipped() {
    let mut two_exe = release_json("v0.2.0-dev.1", "");
    two_exe["assets"]
        .as_array_mut()
        .unwrap()
        .push(asset("v0.2.0-dev.1", EXE_NAME, 1));
    let mut two_sha = release_json("v0.2.0-dev.2", "");
    two_sha["assets"]
        .as_array_mut()
        .unwrap()
        .push(asset("v0.2.0-dev.2", SHA256_NAME, 1));
    let mut two_licenses = release_json("v0.2.0-dev.3", "");
    two_licenses["assets"]
        .as_array_mut()
        .unwrap()
        .push(asset("v0.2.0-dev.3", LICENSES_NAME, 1));

    let list = parse(&[two_exe, two_sha, two_licenses]);
    // Doppelte Programm- oder Prüfsummendatei: ausgelassen; doppelte Lizenzdatei: nur diese fehlt.
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].version, v("0.2.0-dev.3"));
    assert!(list[0].licenses.is_none());
}

#[test]
fn broken_json_is_an_error() {
    for broken in ["", "[", "{\"tag_name\":", "nicht json", "[{]"] {
        assert!(parse_releases(broken).is_err(), "{broken:?}");
    }
    // Rate-Limit-Antwort von GitHub: lesbares JSON, aber keine Liste.
    let rate_limit =
        r#"{"message":"API rate limit exceeded","documentation_url":"https://docs.github.com"}"#;
    let err = parse_releases(rate_limit).unwrap_err();
    assert!(!err.to_string().is_empty());
}

#[test]
fn empty_list_gives_none() {
    let list = parse_releases("[]").unwrap();
    assert!(list.is_empty());
    assert!(pick_update(&list, &v("0.1.0"), Channel::Dev, None).is_none());
}

#[test]
fn pick_offers_only_higher_versions_in_channel() {
    let list = releases(&["v0.2.0-dev.1", "v0.2.0-dev.2", "v0.2.0-beta.1", "v0.1.5"]);

    let update = pick_update(&list, &v("0.2.0-dev.1"), Channel::Dev, None).unwrap();
    assert_eq!(update.latest.version, v("0.2.0-beta.1"));

    // Laufende Version gleich der neuesten oder neuer: nichts.
    assert!(pick_update(&list, &v("0.2.0-beta.1"), Channel::Dev, None).is_none());
    assert!(pick_update(&list, &v("0.3.0"), Channel::Dev, None).is_none());

    // Beta sieht keine Dev-Versionen, Stabil nur Versionen ohne Endung.
    let update = pick_update(&list, &v("0.1.0"), Channel::Beta, None).unwrap();
    assert_eq!(update.latest.version, v("0.2.0-beta.1"));
    let update = pick_update(&list, &v("0.1.0"), Channel::Stable, None).unwrap();
    assert_eq!(update.latest.version, v("0.1.5"));
    assert!(pick_update(&list, &v("0.1.5"), Channel::Stable, None).is_none());

    // Kanal Dev mit nur Dev-Versionen über der laufenden.
    let dev_only = releases(&["v0.2.0-dev.1", "v0.2.0-dev.3", "v0.2.0-dev.2"]);
    let update = pick_update(&dev_only, &v("0.2.0-dev.1"), Channel::Dev, None).unwrap();
    assert_eq!(update.latest.version, v("0.2.0-dev.3"));
}

#[test]
fn pick_respects_skipped_version_only_for_that_version() {
    let list = releases(&["v0.2.0-dev.1", "v0.2.0-dev.2", "v0.2.0-dev.3"]);
    let running = v("0.2.0-dev.1");

    // Die neueste ist übersprungen: die nächstniedrigere wird angeboten.
    let update = pick_update(&list, &running, Channel::Dev, Some(&v("0.2.0-dev.3"))).unwrap();
    assert_eq!(update.latest.version, v("0.2.0-dev.2"));

    // Eine ältere übersprungene Version hält die neueste nicht auf.
    let update = pick_update(&list, &running, Channel::Dev, Some(&v("0.2.0-dev.2"))).unwrap();
    assert_eq!(update.latest.version, v("0.2.0-dev.3"));

    // Erscheint nach der übersprungenen eine neue Version, wird sie angeboten.
    let only_skipped = releases(&["v0.2.0-dev.1", "v0.2.0-dev.2"]);
    assert!(
        pick_update(
            &only_skipped,
            &running,
            Channel::Dev,
            Some(&v("0.2.0-dev.2"))
        )
        .is_none()
    );
}

#[test]
fn notes_between_lists_all_skipped_versions_newest_first() {
    let list = releases(&[
        "v0.2.0-dev.2",
        "v0.2.0-dev.5",
        "v0.2.0-dev.1",
        "v0.2.0-dev.4",
        "v0.2.0-dev.3",
    ]);
    let update = pick_update(&list, &v("0.2.0-dev.2"), Channel::Dev, None).unwrap();
    assert_eq!(update.latest.version, v("0.2.0-dev.5"));
    assert!(!update.truncated);
    let got: Vec<(String, String)> = update
        .notes_between
        .iter()
        .map(|(ver, text)| (ver.to_string(), text.clone()))
        .collect();
    assert_eq!(
        got,
        [
            ("0.2.0-dev.5".to_string(), "Notiz v0.2.0-dev.5".to_string()),
            ("0.2.0-dev.4".to_string(), "Notiz v0.2.0-dev.4".to_string()),
            ("0.2.0-dev.3".to_string(), "Notiz v0.2.0-dev.3".to_string()),
        ]
    );

    // Neueste übersprungen: Notizen reichen nur bis zur angebotenen Version.
    let update = pick_update(
        &list,
        &v("0.2.0-dev.2"),
        Channel::Dev,
        Some(&v("0.2.0-dev.5")),
    )
    .unwrap();
    let versions: Vec<String> = update
        .notes_between
        .iter()
        .map(|(v, _)| v.to_string())
        .collect();
    assert_eq!(versions, ["0.2.0-dev.4", "0.2.0-dev.3"]);

    // Versionen außerhalb des Kanals erscheinen nicht in den Notizen.
    let mixed = releases(&["v0.2.0-dev.1", "v0.2.0-beta.1", "v0.2.0-dev.9", "v0.2.0"]);
    let update = pick_update(&mixed, &v("0.1.0"), Channel::Beta, None).unwrap();
    let versions: Vec<String> = update
        .notes_between
        .iter()
        .map(|(v, _)| v.to_string())
        .collect();
    assert_eq!(versions, ["0.2.0", "0.2.0-beta.1"]);
}

#[test]
fn notes_are_capped_at_20_releases_and_20000_chars_with_truncated_flag() {
    assert_eq!(MAX_NOTES_RELEASES, 20);
    assert_eq!(MAX_NOTES_CHARS, 20_000);

    // 30 kleine Releases: nur die neuesten 20.
    let tags: Vec<String> = (1..=30).map(|n| format!("v0.2.0-dev.{n}")).collect();
    let tag_refs: Vec<&str> = tags.iter().map(String::as_str).collect();
    let list = releases(&tag_refs);
    let update = pick_update(&list, &v("0.1.0"), Channel::Dev, None).unwrap();
    assert_eq!(update.notes_between.len(), 20);
    assert!(update.truncated);
    assert_eq!(update.notes_between[0].0, v("0.2.0-dev.30"));
    assert_eq!(update.notes_between[19].0, v("0.2.0-dev.11"));

    // Genau 20 Releases: nicht gekürzt.
    let update = pick_update(&list, &v("0.2.0-dev.10"), Channel::Dev, None).unwrap();
    assert_eq!(update.notes_between.len(), 20);
    assert!(!update.truncated);

    // Lange Notizen (mit Umlauten, mehrere Bytes je Zeichen): höchstens 20 000 Zeichen insgesamt.
    let long = "ä".repeat(8_000);
    let list = parse(&[
        release_json("v0.2.0-dev.1", &long),
        release_json("v0.2.0-dev.2", &long),
        release_json("v0.2.0-dev.3", &long),
        release_json("v0.2.0-dev.4", &long),
    ]);
    let update = pick_update(&list, &v("0.1.0"), Channel::Dev, None).unwrap();
    assert!(update.truncated);
    let total: usize = update
        .notes_between
        .iter()
        .map(|(_, t)| t.chars().count())
        .sum();
    assert_eq!(total, MAX_NOTES_CHARS);
    let versions: Vec<String> = update
        .notes_between
        .iter()
        .map(|(v, _)| v.to_string())
        .collect();
    assert_eq!(versions, ["0.2.0-dev.4", "0.2.0-dev.3", "0.2.0-dev.2"]);
    assert_eq!(update.notes_between[2].1.chars().count(), 4_000);

    // Eine einzelne riesige Notiz wird ebenfalls gekürzt.
    let huge = "x".repeat(50_000);
    let list = parse(&[release_json("v0.2.0-dev.1", &huge)]);
    let update = pick_update(&list, &v("0.1.0"), Channel::Dev, None).unwrap();
    assert!(update.truncated);
    assert_eq!(update.notes_between[0].1.len(), MAX_NOTES_CHARS);
    // Das Release selbst behält die vollständige Notiz.
    assert_eq!(update.latest.notes.len(), 50_000);
}

/// Die Symboldatei (`.pdb`, für `tools/symbolize.ps1`) liegt als weitere Datei im Release. Die
/// Update-Funktion wählt nur nach exaktem Namen und lädt sie nie; ein Name, der nur ähnlich ist,
/// zählt nicht als exe.
#[test]
fn pdb_asset_does_not_disturb_the_choice() {
    let tag = "v0.2.0-dev.3";
    let mut r = release_json(tag, "");
    let assets = r["assets"].as_array_mut().unwrap();
    assets.insert(0, asset(tag, "planoracad-windows-x64.pdb", 95_000_000));
    assets.push(asset(tag, "planoracad-windows-x64.exe.pdb", 95_000_000));
    let list = parse(&[r]);
    assert_eq!(list.len(), 1);
    let rel = &list[0];
    assert_eq!(rel.exe.name, EXE_NAME);
    assert_eq!(rel.exe.url, url(tag, EXE_NAME));
    assert_eq!(rel.exe.size, 12_345_678);
    assert_eq!(rel.sha256.name, SHA256_NAME);
    assert_eq!(
        rel.licenses.as_ref().map(|l| l.name.as_str()),
        Some(LICENSES_NAME)
    );
}
