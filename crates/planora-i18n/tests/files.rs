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
    let resource = parser::parse(source)
        .unwrap_or_else(|(_, errors)| panic!("{name} hat Syntaxfehler: {errors:?}"));
    let mut keys = Keys::new();
    for entry in &resource.body {
        match entry {
            Entry::Message(m) => {
                let vars = m.value.as_ref().map(vars_of).unwrap_or_default();
                keys.insert(m.id.name.to_string(), vars);
                for a in &m.attributes {
                    keys.insert(format!("{}.{}", m.id.name, a.id.name), vars_of(&a.value));
                }
            }
            Entry::Term(t) => {
                keys.insert(format!("-{}", t.id.name), vars_of(&t.value));
                for a in &t.attributes {
                    keys.insert(format!("-{}.{}", t.id.name, a.id.name), vars_of(&a.value));
                }
            }
            _ => {}
        }
    }
    keys
}

#[test]
fn german_and_english_have_the_same_keys() {
    let de = parse("de.ftl", DE);
    let en = parse("en.ftl", EN);
    let mut problems = Vec::new();
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

/// Ids aus `tr!("…"`, `t("…"` und `t_args("…"` (mit Wortgrenze davor) in einer Zeile.
fn ids_in_line(line: &str) -> Vec<String> {
    let mut found = Vec::new();
    for prefix in ["tr!(\"", "t(\"", "t_args(\""] {
        let mut from = 0;
        while let Some(pos) = line[from..].find(prefix) {
            let start = from + pos;
            from = start + prefix.len();
            let boundary = line[..start]
                .chars()
                .next_back()
                .is_none_or(|c| !(c.is_alphanumeric() || c == '_'));
            if !boundary {
                continue;
            }
            if let Some(len) = line[from..].find('"') {
                let id = &line[from..from + len];
                if is_id(id) {
                    found.push(id.to_string());
                }
            }
        }
    }
    found
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
        let Ok(text) = std::fs::read_to_string(file) else {
            continue;
        };
        for (n, line) in text.lines().enumerate() {
            if line.trim_start().starts_with("//") {
                continue;
            }
            for id in ids_in_line(line) {
                checked += 1;
                let shown = file.strip_prefix(root).unwrap_or(file).display();
                if !de.contains_key(&id) {
                    problems.push(format!("  {shown}:{}: \"{id}\" fehlt in de.ftl", n + 1));
                }
                if !en.contains_key(&id) {
                    problems.push(format!("  {shown}:{}: \"{id}\" fehlt in en.ftl", n + 1));
                }
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
