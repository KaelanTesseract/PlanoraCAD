//! Wächter: keine deutschen Zeichenkettenliterale im Programmcode.
//!
//! Durchsucht `crates/*/src/**/*.rs` nach Zeichenkettenliteralen mit `äöüßÄÖÜ„“`. Kommentare,
//! `#[cfg(test)]`-Teile (Module, Funktionen, per `#[cfg(test)] mod x;` eingebundene Dateien) und
//! die Ordner `tests/` zählen nicht. Oberflächentexte gehören in die Sprachdateien
//! (`languages/*.ftl`), Protokolle und Absturzmeldungen sind englisch. Was bewusst deutsch bleibt,
//! steht mit Begründung in [`ALLOWED`].
//!
//! Grenze: Erkannt wird Deutsch nur an Umlauten, `ß` und den Anführungszeichen „“. Ein deutsches
//! Literal ohne diese Zeichen (etwa „Datei nicht gefunden“ ohne Umlaut) rutscht durch.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

const GERMAN: [char; 9] = ['ä', 'ö', 'ü', 'ß', 'Ä', 'Ö', 'Ü', '„', '“'];

/// Erlaubte Literale: (Datei relativ zum Arbeitsbereich mit `/`, genauer Inhalt des Literals,
/// Begründung). Verglichen wird genau, damit eine Ausnahme keine anderen Texte durchlässt.
/// Jede Ausnahme muss noch treffen, sonst schlägt der Test fehl (Liste bleibt kurz).
const ALLOWED: &[(&str, &str, &str)] = &[
    (
        "crates/planora-app/src/feedback.rs",
        "Tastenkürzel",
        "Bereichs-Schlüssel der Rückmeldung (deutsch gespeichert, angezeigt per tr!)",
    ),
    (
        "crates/planora-app/src/keymap/text.rs",
        "Rücktaste",
        "gespeicherter Tastenname in der Konfiguration, ändert sich nie",
    ),
    (
        "crates/planora-app/src/keymap/text.rs",
        "Einfügen",
        "gespeicherter Tastenname in der Konfiguration, ändert sich nie",
    ),
    (
        "crates/planora-app/src/keymap/text.rs",
        "Schrägstrich",
        "gespeicherter Tastenname in der Konfiguration, ändert sich nie",
    ),
    (
        "crates/planora-app/src/keymap/text.rs",
        "Browser zurück",
        "gespeicherter Tastenname in der Konfiguration, ändert sich nie",
    ),
    (
        "crates/planora-app/src/shell_panels.rs",
        "Fuß",
        "Name eines Felds nur für Tests (field_rects), nie angezeigt",
    ),
    (
        "crates/planora-kernel/src/blend/topo.rs",
        "Fläche",
        "gespeicherte Rolle der Flächenmarken (Dateiformat), angezeigt per tr!",
    ),
    (
        "crates/planora-kernel/src/blend/topo.rs",
        "STEP-Fläche",
        "gespeicherte Rolle der Flächenmarken (Dateiformat), angezeigt per tr!",
    ),
    (
        "crates/planora-kernel/src/revolve.rs",
        "Fläche {}",
        "gespeicherte Rolle der Flächenmarke (Dateiformat), angezeigt per tr!",
    ),
    (
        "crates/planora-step/src/read/topo.rs",
        "STEP-Fläche",
        "gespeicherte Rolle der Flächenmarke (Dateiformat), angezeigt per tr!",
    ),
    (
        "crates/planora-kernel/src/boolean/surfsurf.rs",
        "Freiformfläche",
        "interne Kennung der Flächenart, angezeigt per surface_label (tr!)",
    ),
    (
        "crates/planora-kernel/src/boolean/error.rs",
        "Freiformfläche",
        "dieselbe interne Kennung wie surface_name",
    ),
];

/// Ein Zeichenkettenliteral mit Zeile (ab 1) und Inhalt (roh, ohne Anführungszeichen).
#[derive(Debug, Clone, PartialEq, Eq)]
struct Literal {
    line: usize,
    text: String,
}

/// Ergebnis der Durchsicht einer Datei.
#[derive(Debug, Default)]
struct Scan {
    /// Literale außerhalb von Kommentaren und `#[cfg(test)]`-Teilen.
    literals: Vec<Literal>,
    /// Per `#[cfg(test)] mod name;` eingebundene Dateien (relativ zum Ordner der Datei).
    test_files: Vec<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Tok {
    Str(Literal),
    /// Bezeichner oder Schlüsselwort.
    Word(String),
    /// Einzelnes Satzzeichen.
    Punct(char),
}

/// Zerlegt Rust-Quelltext grob in Token; Kommentare fallen weg, Zeichenliterale und
/// Lebenszeiten auch.
fn lex(src: &str) -> Vec<Tok> {
    let chars: Vec<char> = src.chars().collect();
    let mut toks = Vec::new();
    let mut i = 0;
    let mut line = 1;
    let n = chars.len();
    let at = |i: usize| chars.get(i).copied().unwrap_or('\0');
    while i < n {
        let c = chars[i];
        if c == '\n' {
            line += 1;
            i += 1;
        } else if c.is_whitespace() {
            i += 1;
        } else if c == '/' && at(i + 1) == '/' {
            while i < n && chars[i] != '\n' {
                i += 1;
            }
        } else if c == '/' && at(i + 1) == '*' {
            let mut depth = 0;
            while i < n {
                if chars[i] == '/' && at(i + 1) == '*' {
                    depth += 1;
                    i += 2;
                } else if chars[i] == '*' && at(i + 1) == '/' {
                    depth -= 1;
                    i += 2;
                    if depth == 0 {
                        break;
                    }
                } else {
                    if chars[i] == '\n' {
                        line += 1;
                    }
                    i += 1;
                }
            }
        } else if c == '"' {
            // Gewöhnliches Literal mit Escapes.
            let start = line;
            let mut text = String::new();
            i += 1;
            while i < n && chars[i] != '"' {
                if chars[i] == '\\' {
                    text.push(chars[i]);
                    i += 1;
                }
                if i < n {
                    if chars[i] == '\n' {
                        line += 1;
                    }
                    text.push(chars[i]);
                    i += 1;
                }
            }
            i += 1;
            toks.push(Tok::Str(Literal { line: start, text }));
        } else if c == 'r' && (at(i + 1) == '"' || (at(i + 1) == '#' && raw_start(&chars, i + 1))) {
            // Rohes Literal r"…", r#"…"#.
            i += 1;
            let mut hashes = 0;
            while at(i) == '#' {
                hashes += 1;
                i += 1;
            }
            i += 1; // "
            let start = line;
            let mut text = String::new();
            loop {
                if i >= n {
                    break;
                }
                if chars[i] == '"' && (1..=hashes).all(|k| at(i + k) == '#') {
                    i += 1 + hashes;
                    break;
                }
                if chars[i] == '\n' {
                    line += 1;
                }
                text.push(chars[i]);
                i += 1;
            }
            toks.push(Tok::Str(Literal { line: start, text }));
        } else if (c == 'b' || c == 'c')
            && (at(i + 1) == '"' || (at(i + 1) == 'r' && (at(i + 2) == '"' || at(i + 2) == '#')))
            && !prev_is_word(&chars, i)
        {
            // b"…", br"…", c"…": Präfix überspringen, Literal im nächsten Durchlauf.
            i += 1;
        } else if c == '\'' {
            // Zeichenliteral oder Lebenszeit.
            if at(i + 1) == '\\' {
                i += 2;
                while i < n && chars[i] != '\'' {
                    i += 1;
                }
                i += 1;
            } else if at(i + 2) == '\'' {
                i += 3;
            } else {
                i += 1;
                while i < n && (chars[i].is_alphanumeric() || chars[i] == '_') {
                    i += 1;
                }
            }
        } else if c.is_alphanumeric() || c == '_' {
            let start = i;
            while i < n && (chars[i].is_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            let word: String = chars[start..i].iter().collect();
            // Präfixe von Literalen (b"…", r"…") stehen direkt vor dem Anführungszeichen.
            if matches!(word.as_str(), "b" | "c" | "br" | "r") && at(i) == '"' {
                continue;
            }
            toks.push(Tok::Word(word));
        } else {
            toks.push(Tok::Punct(c));
            i += 1;
        }
    }
    toks
}

/// `#…#"` ab `i`: Beginn eines rohen Literals.
fn raw_start(chars: &[char], mut i: usize) -> bool {
    while chars.get(i) == Some(&'#') {
        i += 1;
    }
    chars.get(i) == Some(&'"')
}

fn prev_is_word(chars: &[char], i: usize) -> bool {
    i > 0 && (chars[i - 1].is_alphanumeric() || chars[i - 1] == '_')
}

fn is_word(tok: Option<&Tok>, w: &str) -> bool {
    matches!(tok, Some(Tok::Word(x)) if x == w)
}

fn is_punct(tok: Option<&Tok>, p: char) -> bool {
    matches!(tok, Some(Tok::Punct(x)) if *x == p)
}

/// Ende (exklusiv) eines Attributs `#[…]` ab `i` (zeigt auf `#`).
fn attribute_end(toks: &[Tok], i: usize) -> Option<usize> {
    if !is_punct(toks.get(i), '#') || !is_punct(toks.get(i + 1), '[') {
        return None;
    }
    let mut depth = 0;
    for (k, tok) in toks.iter().enumerate().skip(i + 1) {
        match tok {
            Tok::Punct('[') => depth += 1,
            Tok::Punct(']') => {
                depth -= 1;
                if depth == 0 {
                    return Some(k + 1);
                }
            }
            _ => {}
        }
    }
    None
}

/// `#[cfg(test)]` genau ab `i`.
fn is_cfg_test(toks: &[Tok], i: usize) -> bool {
    is_punct(toks.get(i), '#')
        && is_punct(toks.get(i + 1), '[')
        && is_word(toks.get(i + 2), "cfg")
        && is_punct(toks.get(i + 3), '(')
        && is_word(toks.get(i + 4), "test")
        && is_punct(toks.get(i + 5), ')')
        && is_punct(toks.get(i + 6), ']')
}

fn scan(src: &str) -> Scan {
    let toks = lex(src);
    let mut out = Scan::default();
    let mut i = 0;
    while i < toks.len() {
        if is_cfg_test(&toks, i) {
            // Weitere Attribute überspringen, `#[path = "…"]` merken.
            let mut k = i + 7;
            let mut path = None;
            while let Some(end) = attribute_end(&toks, k) {
                if is_word(toks.get(k + 2), "path")
                    && let Some(Tok::Str(lit)) = toks.get(k + 4)
                {
                    path = Some(PathBuf::from(&lit.text));
                }
                k = end;
            }
            // Das Element bis `;` oder bis zur passenden `}` überspringen.
            let item_start = k;
            let mut depth = 0;
            while k < toks.len() {
                match &toks[k] {
                    Tok::Punct('{') => depth += 1,
                    Tok::Punct('}') => {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                    }
                    Tok::Punct(';') if depth == 0 => {
                        // `mod name;` → eingebundene Datei ist Testcode.
                        let words: Vec<&str> = toks[item_start..k]
                            .iter()
                            .filter_map(|t| match t {
                                Tok::Word(w) => Some(w.as_str()),
                                _ => None,
                            })
                            .collect();
                        if let Some(pos) = words.iter().position(|w| *w == "mod")
                            && let Some(name) = words.get(pos + 1)
                        {
                            out.test_files.push(
                                path.take()
                                    .unwrap_or_else(|| PathBuf::from(format!("{name}.rs"))),
                            );
                        }
                        break;
                    }
                    _ => {}
                }
                k += 1;
            }
            i = k + 1;
            continue;
        }
        if let Tok::Str(lit) = &toks[i] {
            out.literals.push(lit.clone());
        }
        i += 1;
    }
    out
}

fn has_german(s: &str) -> bool {
    s.chars().any(|c| GERMAN.contains(&c))
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if path
                .file_name()
                .is_some_and(|n| n == "target" || n == "tests")
            {
                continue;
            }
            rust_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

#[test]
fn scanner_skips_comments_tests_and_char_literals() {
    let sample = r####"
// "Kommentar mit Ü"
/* "Block /* verschachtelt "ö" */ noch Kommentar" */
/// "Doku ä"
fn f<'a>(x: &'a str) -> char {
    let _ = "drin ä";
    let _ = r#"roh "ü" drin"#;
    let _ = b"bytes";
    let _ = "escape \" ß";
    'ä'
}
#[cfg(test)]
mod tests {
    fn g() { let _ = "Test ä"; }
}
#[cfg(test)]
#[path = "x_tests.rs"]
mod x;
#[cfg(test)]
mod y;
let _ = "danach „ok“";
"####;
    let scan = scan(sample);
    let german: Vec<(usize, String)> = scan
        .literals
        .iter()
        .filter(|l| has_german(&l.text))
        .map(|l| (l.line, l.text.clone()))
        .collect();
    assert_eq!(
        german,
        vec![
            (6, "drin ä".to_string()),
            (7, "roh \"ü\" drin".to_string()),
            (9, "escape \\\" ß".to_string()),
            (21, "danach „ok“".to_string()),
        ]
    );
    assert_eq!(
        scan.test_files,
        vec![PathBuf::from("x_tests.rs"), PathBuf::from("y.rs")]
    );
}

#[test]
fn no_german_literals_in_code() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("Arbeitsbereich liegt zwei Ebenen über dem Crate");
    let mut files = Vec::new();
    for entry in std::fs::read_dir(root.join("crates"))
        .expect("Ordner crates lesbar")
        .flatten()
    {
        rust_files(&entry.path().join("src"), &mut files);
    }
    files.sort();
    assert!(
        !files.is_empty(),
        "keine .rs-Dateien unter crates/*/src gefunden"
    );

    // Erst alle Dateien lesen, damit per `#[cfg(test)] mod x;` eingebundene Dateien bekannt sind.
    let mut scans = Vec::new();
    let mut test_only = BTreeSet::new();
    for file in &files {
        let text = std::fs::read_to_string(file)
            .unwrap_or_else(|e| panic!("{} nicht lesbar: {e}", file.display()));
        let scan = scan(&text);
        let dir = file.parent().unwrap_or(root);
        let stem = file.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        for rel in &scan.test_files {
            test_only.insert(dir.join(rel));
            // `mod x;` in `foo.rs` liegt auch unter `foo/x.rs`.
            if !matches!(stem, "lib" | "main" | "mod") {
                test_only.insert(dir.join(stem).join(rel));
            }
        }
        scans.push((file, scan));
    }

    let mut used = vec![false; ALLOWED.len()];
    let mut problems = Vec::new();
    for (file, scan) in &scans {
        if test_only.contains(*file) {
            continue;
        }
        let shown = file
            .strip_prefix(root)
            .unwrap_or(file)
            .to_string_lossy()
            .replace('\\', "/");
        for lit in scan.literals.iter().filter(|l| has_german(&l.text)) {
            let allowed = ALLOWED
                .iter()
                .position(|(f, text, _)| *f == shown && lit.text == *text);
            match allowed {
                Some(k) => used[k] = true,
                None => problems.push(format!("  {shown}:{}: \"{}\"", lit.line, lit.text)),
            }
        }
    }
    for (k, (f, text, _)) in ALLOWED.iter().enumerate() {
        if !used[k] {
            problems.push(format!(
                "  Ausnahme ohne Treffer (bitte streichen): {f}: \"{text}\""
            ));
        }
    }
    assert!(
        problems.is_empty(),
        "Deutsche Zeichenkettenliterale im Code ({} Stellen). Oberflächentexte gehören in \
         languages/*.ftl (tr!), Protokolle und Absturzmeldungen sind englisch; bewusste \
         Ausnahmen mit Begründung in ALLOWED:\n{}",
        problems.len(),
        problems.join("\n")
    );
}
