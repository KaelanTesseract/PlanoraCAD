//! Prüfungen der eingebauten Sprachdateien: gleiche Schlüssel, gleiche Platzhalter, alle im Code
//! benutzten Ids vorhanden, kein Deutsch im Englischen.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use fluent_syntax::ast::{
    CallArguments, Entry, Expression, InlineExpression, Pattern, PatternElement,
};
use fluent_syntax::parser;

const DE: &str = include_str!("../languages/de.ftl");
const EN: &str = include_str!("../languages/en.ftl");

/// Schlüssel (`id` oder `id.attribut`) -> Menge der Variablen `$name` im Text.
type Keys = BTreeMap<String, BTreeSet<String>>;

fn vars_in_pattern(p: &Pattern<&str>, out: &mut BTreeSet<String>) {
    for el in &p.elements {
        if let PatternElement::Placeable { expression } = el {
            vars_in_expression(expression, out);
        }
    }
}

fn vars_in_expression(e: &Expression<&str>, out: &mut BTreeSet<String>) {
    match e {
        Expression::Inline(i) => vars_in_inline(i, out),
        Expression::Select { selector, variants } => {
            vars_in_inline(selector, out);
            for v in variants {
                vars_in_pattern(&v.value, out);
            }
        }
    }
}

fn vars_in_args(a: &CallArguments<&str>, out: &mut BTreeSet<String>) {
    for p in &a.positional {
        vars_in_inline(p, out);
    }
    for n in &a.named {
        vars_in_inline(&n.value, out);
    }
}

fn vars_in_inline(i: &InlineExpression<&str>, out: &mut BTreeSet<String>) {
    match i {
        InlineExpression::VariableReference { id } => {
            out.insert(id.name.to_string());
        }
        InlineExpression::FunctionReference { arguments, .. } => vars_in_args(arguments, out),
        InlineExpression::TermReference {
            arguments: Some(a), ..
        } => vars_in_args(a, out),
        InlineExpression::Placeable { expression } => vars_in_expression(expression, out),
        _ => {}
    }
}

fn vars_of(p: &Pattern<&str>) -> BTreeSet<String> {
    let mut vars = BTreeSet::new();
    vars_in_pattern(p, &mut vars);
    vars
}

fn parse(name: &str, source: &'static str) -> Keys {
    parse_with_duplicates(name, source).0
}

/// Wie [`parse`], dazu die mehrfach definierten Schlüssel (sonst überschriebe der letzte still).
fn parse_with_duplicates(name: &str, source: &'static str) -> (Keys, Vec<String>) {
    let resource = parser::parse(source)
        .unwrap_or_else(|(_, errors)| panic!("{name} hat Syntaxfehler: {errors:?}"));
    let mut keys = Keys::new();
    let mut duplicates = Vec::new();
    let mut add = |key: String, vars: BTreeSet<String>| {
        if keys.insert(key.clone(), vars).is_some() {
            duplicates.push(key);
        }
    };
    for entry in &resource.body {
        match entry {
            Entry::Message(m) => {
                add(
                    m.id.name.to_string(),
                    m.value.as_ref().map(vars_of).unwrap_or_default(),
                );
                for a in &m.attributes {
                    add(format!("{}.{}", m.id.name, a.id.name), vars_of(&a.value));
                }
            }
            Entry::Term(t) => {
                add(format!("-{}", t.id.name), vars_of(&t.value));
                for a in &t.attributes {
                    add(format!("-{}.{}", t.id.name, a.id.name), vars_of(&a.value));
                }
            }
            _ => {}
        }
    }
    (keys, duplicates)
}

#[test]
fn german_and_english_have_the_same_keys() {
    let (de, de_dups) = parse_with_duplicates("de.ftl", DE);
    let (en, en_dups) = parse_with_duplicates("en.ftl", EN);
    let mut problems = Vec::new();
    for k in de_dups {
        problems.push(format!("  mehrfach in de.ftl: {k}"));
    }
    for k in en_dups {
        problems.push(format!("  mehrfach in en.ftl: {k}"));
    }
    for k in de.keys().filter(|k| !en.contains_key(*k)) {
        problems.push(format!("  fehlt in en.ftl: {k}"));
    }
    for k in en.keys().filter(|k| !de.contains_key(*k)) {
        problems.push(format!("  fehlt in de.ftl: {k}"));
    }
    assert!(
        problems.is_empty(),
        "de.ftl und en.ftl haben verschiedene Schlüssel:\n{}",
        problems.join("\n")
    );
}

#[test]
fn placeholders_match() {
    let de = parse("de.ftl", DE);
    let en = parse("en.ftl", EN);
    let mut problems = Vec::new();
    for (key, de_vars) in &de {
        if let Some(en_vars) = en.get(key)
            && de_vars != en_vars
        {
            problems.push(format!("  {key}: de {de_vars:?} <-> en {en_vars:?}"));
        }
    }
    assert!(
        problems.is_empty(),
        "Platzhalter ($name) stimmen zwischen de.ftl und en.ftl nicht überein:\n{}",
        problems.join("\n")
    );
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if path.file_name().is_some_and(|n| n == "target") {
                continue;
            }
            rust_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

/// Form `[a-z0-9]+(-[a-z0-9]+)*`.
fn is_id(s: &str) -> bool {
    !s.is_empty()
        && s.split('-').all(|part| {
            !part.is_empty()
                && part
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        })
}

/// Ids aus `tr!("…"`, `msg!("…"`, `t("…"` und `t_args("…"` im ganzen Text, mit Zeilennummer (ab 1).
/// Zwischen `(` und `"` darf Leerraum samt Zeilenumbruch stehen (so bricht rustfmt lange Aufrufe um).
/// Wortgrenze davor; Treffer in Zeilenkommentaren zählen nicht.
fn ids_in_text(text: &str) -> Vec<(usize, String)> {
    let mut found = Vec::new();
    for prefix in ["tr!(", "t(", "t_args(", "msg!("] {
        let mut from = 0;
        while let Some(pos) = text[from..].find(prefix) {
            let start = from + pos;
            from = start + prefix.len();
            let boundary = text[..start]
                .chars()
                .next_back()
                .is_none_or(|c| !(c.is_alphanumeric() || c == '_'));
            if !boundary {
                continue;
            }
            let line_start = text[..start].rfind('\n').map_or(0, |i| i + 1);
            if text[line_start..start].contains("//") {
                continue;
            }
            let rest = text[from..].trim_start();
            let Some(after_quote) = rest.strip_prefix('"') else {
                continue;
            };
            if let Some(len) = after_quote.find('"') {
                let id = &after_quote[..len];
                if is_id(id) {
                    let line = text[..start].matches('\n').count() + 1;
                    found.push((line, id.to_string()));
                }
            }
        }
    }
    found.sort();
    found
}

#[test]
fn scanner_finds_ids_in_all_call_forms() {
    let sample = r#"
let a = tr!("one-id");
let b = tr!(
    "wrapped-id",
    name = value
);
let c = t_args(
    "args-id",
    &[],
);
let d = t("plain-id");
let e = tr!("with-args", n = 1);
print("not-an-id");
let f = format("nope-x");
let g = my_t("boundary-x");
// tr!("commented-id")
let h = x; // t("trailing-comment-id")
let i = tr!("Not_An_Id");
"#;
    let ids: Vec<String> = ids_in_text(sample).into_iter().map(|(_, id)| id).collect();
    assert_eq!(
        ids,
        vec!["one-id", "wrapped-id", "args-id", "plain-id", "with-args"]
    );
    let wrapped = ids_in_text(sample)
        .into_iter()
        .find(|(_, id)| id == "wrapped-id")
        .unwrap();
    assert_eq!(wrapped.0, 3, "Zeile des umbrochenen tr!(");
}

#[test]
fn every_key_used_in_code_exists() {
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

    let de = parse("de.ftl", DE);
    let en = parse("en.ftl", EN);
    let mut problems = Vec::new();
    let mut checked = 0usize;
    for file in &files {
        let text = std::fs::read_to_string(file)
            .unwrap_or_else(|e| panic!("{} nicht lesbar: {e}", file.display()));
        let shown = file.strip_prefix(root).unwrap_or(file).display();
        for (line, id) in ids_in_text(&text) {
            checked += 1;
            if !de.contains_key(&id) {
                problems.push(format!("  {shown}:{line}: \"{id}\" fehlt in de.ftl"));
            }
            if !en.contains_key(&id) {
                problems.push(format!("  {shown}:{line}: \"{id}\" fehlt in en.ftl"));
            }
        }
    }
    assert!(
        problems.is_empty(),
        "Im Code benutzte Ids fehlen in den Sprachdateien ({checked} Stellen geprüft):\n{}",
        problems.join("\n")
    );
}

/// Alle Textstücke eines Musters, auch in Auswahl-Varianten.
fn collect_text(p: &Pattern<&str>, out: &mut Vec<String>) {
    for el in &p.elements {
        match el {
            PatternElement::TextElement { value } => out.push(value.to_string()),
            PatternElement::Placeable { expression } => collect_text_expr(expression, out),
        }
    }
}

fn collect_text_expr(e: &Expression<&str>, out: &mut Vec<String>) {
    match e {
        Expression::Inline(InlineExpression::StringLiteral { value }) => {
            out.push(value.to_string())
        }
        Expression::Inline(InlineExpression::Placeable { expression }) => {
            collect_text_expr(expression, out)
        }
        Expression::Inline(_) => {}
        Expression::Select { variants, .. } => {
            for v in variants {
                collect_text(&v.value, out);
            }
        }
    }
}

#[test]
fn english_has_no_german_letters() {
    const GERMAN: [char; 8] = ['ä', 'ö', 'ü', 'ß', 'Ä', 'Ö', 'Ü', '„'];
    let resource = parser::parse(EN).expect("en.ftl ist gültig");
    let mut problems = Vec::new();
    let mut check = |key: String, p: &Pattern<&str>| {
        let mut texts = Vec::new();
        collect_text(p, &mut texts);
        for t in texts {
            if let Some(c) = t.chars().find(|c| GERMAN.contains(c)) {
                problems.push(format!("  {key}: enthält '{c}' in \"{t}\""));
            }
        }
    };
    for entry in &resource.body {
        match entry {
            Entry::Message(m) => {
                if let Some(v) = &m.value {
                    check(m.id.name.to_string(), v);
                }
                for a in &m.attributes {
                    check(format!("{}.{}", m.id.name, a.id.name), &a.value);
                }
            }
            Entry::Term(t) => {
                check(format!("-{}", t.id.name), &t.value);
                for a in &t.attributes {
                    check(format!("-{}.{}", t.id.name, a.id.name), &a.value);
                }
            }
            _ => {}
        }
    }
    assert!(
        problems.is_empty(),
        "en.ftl enthält deutsche Buchstaben:\n{}",
        problems.join("\n")
    );
}
