//! Wächter: Kein Oberflächentext nennt das Projektverzeichnis „Wurzel“ oder verweist auf das
//! entfallene Feld „Projekte“ bzw. den alten Menüeintrag „Öffnen …“. Geprüft werden die
//! Zeichenkettenliterale in allen Quelltexten unter `src` (auch Unterordner); Kommentare zählen
//! nicht (dort bleibt „Wurzel“ als Fachwort erlaubt, ebenso der Konfigurationsschlüssel `root`).

use std::path::Path;

const FORBIDDEN: [&str; 4] = ["Wurzel", "Feld Projekte", "Feld „Projekte“", "Öffnen …"];

/// Zeichenkettenliterale `(Zeile, Inhalt)` im Quelltext, ohne Kommentare (Zeilen- und
/// Blockkommentare). Erkennt gewöhnliche (`"…"`, `b"…"`) und rohe Literale (`r"…"`, `r#"…"#`),
/// Zeichenliterale (`'"'`) und Escapes (`\"`).
fn literals(source: &str) -> Vec<(usize, String)> {
    let s: Vec<char> = source.chars().collect();
    let mut out = Vec::new();
    let (mut i, mut line) = (0, 1);
    let ident = |c: char| c.is_alphanumeric() || c == '_';
    while i < s.len() {
        let c = s[i];
        let next = s.get(i + 1).copied();
        if c == '\n' {
            line += 1;
            i += 1;
        } else if c == '/' && next == Some('/') {
            while i < s.len() && s[i] != '\n' {
                i += 1;
            }
        } else if c == '/' && next == Some('*') {
            let mut depth = 0;
            while i < s.len() {
                if s[i] == '/' && s.get(i + 1) == Some(&'*') {
                    depth += 1;
                    i += 2;
                } else if s[i] == '*' && s.get(i + 1) == Some(&'/') {
                    depth -= 1;
                    i += 2;
                    if depth == 0 {
                        break;
                    }
                } else {
                    if s[i] == '\n' {
                        line += 1;
                    }
                    i += 1;
                }
            }
        } else if c == '\'' {
            // Zeichenliteral ('x', '\n', '\u{..}') oder Lebensdauer ('a).
            if next == Some('\\') {
                // nach dem Rückstrich das maskierte Zeichen überspringen ('\'')
                let mut j = i + 3;
                while j < s.len() && s[j] != '\'' && s[j] != '\n' {
                    j += 1;
                }
                i = j + 1;
            } else if s.get(i + 2) == Some(&'\'') {
                i += 3;
            } else {
                i += 1;
            }
        } else if c == 'r' && (i == 0 || !ident(s[i - 1])) && matches!(next, Some('"') | Some('#'))
        {
            let mut j = i + 1;
            let mut hashes = 0;
            while s.get(j) == Some(&'#') {
                hashes += 1;
                j += 1;
            }
            if s.get(j) != Some(&'"') {
                i += 1;
                continue;
            }
            let start_line = line;
            let mut text = String::new();
            j += 1;
            while j < s.len() {
                if s[j] == '"' && (1..=hashes).all(|k| s.get(j + k) == Some(&'#')) {
                    j += 1 + hashes;
                    break;
                }
                if s[j] == '\n' {
                    line += 1;
                }
                text.push(s[j]);
                j += 1;
            }
            out.push((start_line, text));
            i = j;
        } else if c == '"' {
            let start_line = line;
            let mut text = String::new();
            let mut j = i + 1;
            while j < s.len() && s[j] != '"' {
                if s[j] == '\\' && j + 1 < s.len() {
                    text.push(s[j]);
                    text.push(s[j + 1]);
                    if s[j + 1] == '\n' {
                        line += 1;
                    }
                    j += 2;
                    continue;
                }
                if s[j] == '\n' {
                    line += 1;
                }
                text.push(s[j]);
                j += 1;
            }
            out.push((start_line, text));
            i = j + 1;
        } else {
            i += 1;
        }
    }
    out
}

/// Fundstellen `(Zeile, Wort)`: Literale, die ein verbotenes Wort enthalten.
fn forbidden_words(source: &str) -> Vec<(usize, &'static str)> {
    let mut found = Vec::new();
    for (line, text) in literals(source) {
        for word in FORBIDDEN {
            if text.contains(word) {
                found.push((line, word));
            }
        }
    }
    found
}

fn rust_files(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

#[test]
fn no_ui_text_says_wurzel_or_feld_projekte() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    rust_files(&src, &mut files);
    assert!(files.len() > 20, "zu wenige Quelltexte: {}", files.len());
    let mut bad = Vec::new();
    for path in files {
        let source = std::fs::read_to_string(&path).unwrap();
        for (line, word) in forbidden_words(&source) {
            bad.push(format!("{}:{line}: {word}", path.display()));
        }
    }
    assert!(bad.is_empty(), "verbotene Oberflächentexte: {bad:?}");
}

#[test]
fn the_scanner_finds_literals_and_skips_comments() {
    let text = "// \"Wurzel\" steht im Kommentar\n\
                let a = \"Wurzel wählen …\"; // \"Feld Projekte\" im Zeilenende\n\
                /* \"Öffnen …\" im Blockkommentar */ let b = '\"';\n\
                let c = r#\"Zuerst im Feld Projekte\"#;\n\
                let d = \"mit \\\" Escape und Öffnen …\";\n\
                let e = \"Feld „Projekte“\"; fn f<'a>(x: &'a str) {}\n\
                let ok = \"Fenster Projekte\";\n";
    assert_eq!(
        forbidden_words(text),
        [
            (2, "Wurzel"),
            (4, "Feld Projekte"),
            (5, "Öffnen …"),
            (6, "Feld „Projekte“"),
        ]
    );
    // Literale werden ganz gelesen, auch über Zeilen und mit Escapes.
    let lits = literals("let x = \"a\\\"b\";\nlet y = \"c\nd\"; let z = r\"e\";\n");
    assert_eq!(
        lits,
        [
            (1, "a\\\"b".to_string()),
            (2, "c\nd".to_string()),
            (3, "e".to_string())
        ]
    );
}
