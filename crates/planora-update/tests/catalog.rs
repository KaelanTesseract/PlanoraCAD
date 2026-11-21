//! Plug-in-Katalog Task 1: Katalog-Release und `katalog.json` lesen und prüfen. Ohne Netz.

use planora_update::catalog::{
    CATALOG_FORMAT, CacheIndex, CatalogError, MAX_ENTRIES, latest_release_url, parse_catalog,
    parse_latest_release,
};
use serde_json::json;

const EXAMPLE: &str = r#"{
  "format": 1,
  "eintraege": [
    {
      "id": "planora.elektronik",
      "name": { "de": "Elektronik", "en": "Electronics" },
      "beschreibung": { "de": "Bauteilbibliothek.", "en": "Part library." },
      "kategorie": "elektronik",
      "repo": "https://github.com/besitzer/plugin-repo",
      "symbol": "symbol.png"
    }
  ]
}"#;

fn entry(id: &str, repo: &str) -> serde_json::Value {
    json!({ "id": id, "name": { "en": "X" }, "repo": repo })
}

fn catalog(entries: Vec<serde_json::Value>) -> String {
    json!({ "format": CATALOG_FORMAT, "eintraege": entries }).to_string()
}

#[test]
fn parses_example() {
    let (entries, skipped) = parse_catalog(EXAMPLE).unwrap();
    assert!(skipped.is_empty());
    assert_eq!(entries.len(), 1);
    let e = &entries[0];
    assert_eq!(e.id, "planora.elektronik");
    assert_eq!(e.repo.owner, "besitzer");
    assert_eq!(e.repo.repo, "plugin-repo");
    assert_eq!(e.category, "elektronik");
    assert_eq!(e.symbol.as_deref(), Some("symbol.png"));
    assert_eq!(e.name_in("de"), "Elektronik");
    assert_eq!(e.name_in("fr"), "Electronics");
    assert_eq!(e.description_in("de"), "Bauteilbibliothek.");
    assert_eq!(e.description_in("fr"), "Part library.");
}

#[test]
fn language_fallback_first_then_empty() {
    let json = catalog(vec![json!({
        "id": "a.b", "name": { "ja": "J" }, "repo": "https://github.com/a/b"
    })]);
    let (entries, _) = parse_catalog(&json).unwrap();
    assert_eq!(entries[0].name_in("de"), "J");
    assert_eq!(entries[0].description_in("de"), "");
}

#[test]
fn format_too_new() {
    assert_eq!(
        parse_catalog(r#"{"format": 2, "eintraege": []}"#).unwrap_err(),
        CatalogError::TooNew(2)
    );
    for bad in [r#"{"eintraege": []}"#, "[]", "nope", r#"{"format": "1"}"#] {
        assert!(
            matches!(parse_catalog(bad), Err(CatalogError::Invalid(_))),
            "{bad}"
        );
    }
}

#[test]
fn bad_entries_skipped() {
    let ok = entry("planora.gut", "https://github.com/a/b");
    let json = catalog(vec![
        ok.clone(),
        entry("x.y", "https://gitlab.com/a/b"),
        entry("x.y", "https://github.com/a/../b"),
        entry("x.y", "https://github.com/a/b c"),
        json!({ "name": { "en": "X" }, "repo": "https://github.com/a/b" }),
        entry("Planora.Gross", "https://github.com/a/b"),
        entry("keinpunkt", "https://github.com/a/b"),
        entry("a..b", "https://github.com/a/b"),
        entry("planora.gut", "https://github.com/c/d"),
        json!("kein objekt"),
    ]);
    let (entries, skipped) = parse_catalog(&json).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].repo.owner, "a");
    assert_eq!(skipped.len(), 9);
}

#[test]
fn symbol_names_checked() {
    for (symbol, want) in [
        ("../x.png", None),
        ("a/b.png", None),
        (r"a\b.png", None),
        ("x.gif", None),
        ("", None),
        // SVG kann das Programm nicht zeichnen (kein Rasterer): nur PNG.
        ("logo.svg", None),
        ("LOGO.PNG", Some("LOGO.PNG")),
        ("logo.png", Some("logo.png")),
    ] {
        let mut e = entry("a.b", "https://github.com/a/b");
        e["symbol"] = json!(symbol);
        let (entries, _) = parse_catalog(&catalog(vec![e])).unwrap();
        assert_eq!(entries[0].symbol.as_deref(), want, "{symbol:?}");
    }
}

#[test]
fn too_many_entries() {
    let list: Vec<_> = (0..=MAX_ENTRIES)
        .map(|i| entry(&format!("a.p{i}"), "https://github.com/a/b"))
        .collect();
    let (entries, skipped) = parse_catalog(&catalog(list)).unwrap();
    assert_eq!(entries.len(), MAX_ENTRIES);
    assert_eq!(skipped.len(), 1);
}

#[test]
fn unknown_fields_ignored() {
    let json = r#"{"format": 1, "neu": true, "eintraege": [
        {"id": "a.b", "name": {"en": "X"}, "repo": "https://github.com/a/b", "zukunft": [1, 2]}]}"#;
    let (entries, skipped) = parse_catalog(json).unwrap();
    assert_eq!(entries.len(), 1);
    assert!(skipped.is_empty());
}

fn asset(name: &str, size: u64, tag: &str) -> serde_json::Value {
    json!({
        "name": name, "size": size,
        "browser_download_url": format!(
            "https://github.com/KaelanTesseract/PlanoraCAD-Plugins/releases/download/{tag}/{name}")
    })
}

#[test]
fn symbol_files_are_png() {
    let file = CacheIndex::symbol_file("logo.svg");
    assert!(file.ends_with(".png"), "{file}");
    assert!(CacheIndex::is_symbol_file(&file));
    assert!(!CacheIndex::is_symbol_file(&file.replace(".png", ".svg")));
}

#[test]
fn release_url() {
    assert_eq!(
        latest_release_url(),
        "https://api.github.com/repos/KaelanTesseract/PlanoraCAD-Plugins/releases/latest"
    );
}

#[test]
fn release_parse() {
    let mut foreign = asset("fremd.png", 10, "katalog-1");
    foreign["browser_download_url"] =
        json!("https://github.com/anderer/repo/releases/download/katalog-1/fremd.png");
    let release = json!({
        "tag_name": "katalog-1", "draft": false,
        "assets": [
            asset("katalog.json", 500, "katalog-1"),
            asset("logo.png", 1000, "katalog-1"),
            asset("logo.svg", 1000, "katalog-1"),
            asset("riesig.png", 300 << 10, "katalog-1"),
            foreign,
            asset("notiz.txt", 5, "katalog-1"),
        ]
    })
    .to_string();
    let r = parse_latest_release(&release).unwrap();
    assert_eq!(r.tag, "katalog-1");
    assert_eq!(r.catalog.name, "katalog.json");
    let names: Vec<_> = r.symbols.iter().map(|a| a.name.as_str()).collect();
    assert_eq!(names, ["logo.png"]);

    let without = json!({ "tag_name": "katalog-1", "assets": [asset("logo.png", 1, "katalog-1")] });
    assert!(parse_latest_release(&without.to_string()).is_err());
    let draft = json!({ "tag_name": "katalog-1", "draft": true,
        "assets": [asset("katalog.json", 1, "katalog-1")] });
    assert!(parse_latest_release(&draft.to_string()).is_err());
    assert!(parse_latest_release("kaputt").is_err());
}
