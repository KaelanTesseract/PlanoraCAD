//! Wächter: Farben der Oberfläche stehen nur in `theme.rs`. Alle anderen Quelldateien holen sie
//! aus `theme::palette(dark)` oder aus den Funktionen des Themes.

use std::fs;
use std::path::Path;

/// Aufrufe, die eine feste Farbe erzeugen.
const CONSTRUCTORS: [&str; 9] = [
    "Color32::from_rgb(",
    "Color32::from_rgba_unmultiplied(",
    "Color32::from_rgba_premultiplied(",
    "Color32::from_gray(",
    "Color32::from_black_alpha(",
    "Color32::from_white_alpha(",
    "Color32::from_hex(",
    "hex_color!(",
    "Color32::new(",
];

/// Erlaubte Ausnahmen: (Datei, Aufruf, Begründung). Bewusst leer; alle Farben liegen im Theme.
const ALLOWED: [(&str, &str, &str); 0] = [];

/// Namen von `Color32`-Konstanten, die keine feste Farbe sind.
const NEUTRAL_NAMES: [&str; 2] = ["PLACEHOLDER", "TRANSPARENT"];

/// Trefferliste `datei:zeile: text` für den Quelltext einer Datei (ohne Kommentarzeilen).
fn hits_in(file: &str, text: &str) -> Vec<String> {
    let mut hits = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let code = line.trim_start();
        if code.starts_with("//") {
            continue;
        }
        let mut found = CONSTRUCTORS.iter().any(|c| code.contains(c));
        // Benannte Farben wie `Color32::WHITE` oder `Color32::RED`.
        let mut rest = code;
        while let Some(at) = rest.find("Color32::") {
            rest = &rest[at + "Color32::".len()..];
            let ident: String = rest
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                .collect();
            let is_const = ident.chars().next().is_some_and(|c| c.is_ascii_uppercase())
                && ident
                    .chars()
                    .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_');
            if is_const && !NEUTRAL_NAMES.contains(&ident.as_str()) {
                found = true;
            }
        }
        if found
            && !ALLOWED
                .iter()
                .any(|(f, c, _)| *f == file && code.contains(c))
        {
            hits.push(format!("{file}:{}: {code}", i + 1));
        }
    }
    hits
}

fn source_files() -> Vec<(String, String)> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    for entry in fs::read_dir(&dir).expect("src lesen") {
        let path = entry.expect("Eintrag").path();
        if path.extension().is_some_and(|e| e == "rs") {
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            let text = fs::read_to_string(&path).expect("Datei lesen");
            files.push((name, text));
        }
    }
    files.sort();
    files
}

#[test]
fn no_color32_literals_outside_the_theme() {
    let mut hits = Vec::new();
    let files = source_files();
    assert!(
        files.iter().any(|(n, _)| n == "theme.rs"),
        "src/theme.rs fehlt"
    );
    for (name, text) in &files {
        if name == "theme.rs" {
            continue;
        }
        hits.extend(hits_in(name, text));
    }
    assert!(
        hits.is_empty(),
        "feste Farben außerhalb von theme.rs ({}):\n{}",
        hits.len(),
        hits.join("\n")
    );
}

#[test]
fn the_scanner_finds_each_kind_of_literal() {
    for code in [
        "let c = Color32::from_rgb(1, 2, 3);",
        "let c = egui::Color32::from_gray(9);",
        "let c = Color32::from_rgba_unmultiplied(1, 2, 3, 4);",
        "let c = Color32::from_black_alpha(9);",
        "let c = Color32::WHITE;",
        "let c = egui::Color32::RED;",
    ] {
        assert_eq!(hits_in("x.rs", code).len(), 1, "{code}");
    }
    for code in [
        "// Color32::from_rgb(1, 2, 3)",
        "let c = Color32::PLACEHOLDER;",
        "let c = Color32::TRANSPARENT;",
        "let c = theme::palette(dark).error;",
    ] {
        assert!(hits_in("x.rs", code).is_empty(), "{code}");
    }
}
