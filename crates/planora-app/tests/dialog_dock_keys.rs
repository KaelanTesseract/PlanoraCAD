//! Wächter: Jeder `Dialog::new(` in `src` hat im selben Ausdruck (bis zum nächsten `.show`,
//! `;` oder Funktionsende) genau eines von `.dock_key(…)`, `.fixed()` oder
//! `.bottom_right_in(`; nur der Absturzhinweis (`crash_dialog.rs`) hat keins. Die Schlüssel am
//! Aufruf stehen in `detach::DIALOG_KEYS`, jeder höchstens einmal; die Liste hat keine Doppelten
//! und keinen Schlüssel, den der Quelltext nicht nennt.

use planora_app::detach::DIALOG_KEYS;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Wie ein `Dialog::new(` gekennzeichnet ist.
#[derive(Debug, PartialEq, Eq)]
enum Mark {
    /// `.dock_key("…")` mit Schlüssel als Text, `None` bei einem Ausdruck (Schlüssel je Art).
    Key(Option<String>),
    Fixed,
    BottomRight,
}

/// Ein Aufruf `Dialog::new(` mit Zeile und allen Kennzeichen im Ausdruck.
#[derive(Debug)]
struct Call {
    line: usize,
    marks: Vec<Mark>,
}

/// Quelltext ohne Kommentare (alles ab `//` je Zeile), Zeilen bleiben erhalten.
fn strip_comments(source: &str) -> String {
    source
        .lines()
        .map(|l| l.split("//").next().unwrap_or(""))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Alle Aufrufe `Dialog::new(` (nicht `ExportDialog::new(` o. Ä.) mit ihren Kennzeichen.
fn calls(source: &str) -> Vec<Call> {
    let code = strip_comments(source);
    let needle = "Dialog::new(";
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(pos) = code[from..].find(needle).map(|p| p + from) {
        from = pos + needle.len();
        let before = code[..pos].chars().next_back();
        if before.is_some_and(|c| c.is_alphanumeric() || c == '_') {
            continue;
        }
        let rest = &code[from..];
        let end = [".show", ";", "\n}"]
            .iter()
            .filter_map(|e| rest.find(e))
            .min()
            .unwrap_or(rest.len());
        let expr = &rest[..end];
        let mut marks = Vec::new();
        let mut at = 0;
        while let Some(p) = expr[at..].find(".dock_key(").map(|p| p + at) {
            at = p + ".dock_key(".len();
            // Bis zur schließenden Klammer: ein Ausdruck wie `self.kind.dock_key()` zählt nicht
            // als zweites Kennzeichen.
            let mut depth = 1;
            let len = expr[at..]
                .char_indices()
                .find(|&(_, c)| {
                    depth += match c {
                        '(' => 1,
                        ')' => -1,
                        _ => 0,
                    };
                    depth == 0
                })
                .map_or(expr.len() - at, |(i, _)| i);
            let arg = expr[at..at + len].trim();
            let key = arg
                .strip_prefix('"')
                .and_then(|a| a.strip_suffix('"'))
                .map(str::to_string);
            marks.push(Mark::Key(key));
            at += len;
        }
        for _ in expr.matches(".fixed()") {
            marks.push(Mark::Fixed);
        }
        for _ in expr.matches(".bottom_right_in(") {
            marks.push(Mark::BottomRight);
        }
        out.push(Call {
            line: code[..pos].matches('\n').count() + 1,
            marks,
        });
    }
    out
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

fn sources() -> Vec<(String, String)> {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    rust_files(&src, &mut files);
    files.sort();
    files
        .into_iter()
        .map(|p| {
            let name = p
                .strip_prefix(&src)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            (name, std::fs::read_to_string(&p).unwrap())
        })
        .collect()
}

#[test]
fn every_dialog_has_exactly_one_mark() {
    let mut bad = Vec::new();
    let mut count = 0;
    for (name, source) in sources() {
        if name == "dialog.rs" {
            continue;
        }
        for call in calls(&source) {
            count += 1;
            let ok = if name == "crash_dialog.rs" {
                call.marks.is_empty()
            } else {
                call.marks.len() == 1
            };
            if !ok {
                bad.push(format!("{name}:{}: {:?}", call.line, call.marks));
            }
        }
    }
    assert!(count >= 30, "zu wenige Aufrufe gefunden ({count})");
    assert!(
        bad.is_empty(),
        "Dialoge ohne bzw. mit mehreren Kennzeichen: {bad:#?}"
    );
}

#[test]
fn literal_keys_are_listed_and_unique() {
    let mut seen: BTreeMap<String, String> = BTreeMap::new();
    let mut bad = Vec::new();
    for (name, source) in sources() {
        for call in calls(&source) {
            for mark in &call.marks {
                if let Mark::Key(Some(key)) = mark {
                    let at = format!("{name}:{}", call.line);
                    if !DIALOG_KEYS.contains(&key.as_str()) {
                        bad.push(format!("{at}: „{key}“ fehlt in DIALOG_KEYS"));
                    }
                    if let Some(first) = seen.insert(key.clone(), at.clone()) {
                        bad.push(format!("{at}: „{key}“ schon bei {first}"));
                    }
                }
            }
        }
    }
    assert!(bad.is_empty(), "{bad:#?}");
}

#[test]
fn dialog_keys_have_no_duplicates_and_all_are_used() {
    let mut sorted = DIALOG_KEYS.to_vec();
    sorted.sort_unstable();
    let before = sorted.len();
    sorted.dedup();
    assert_eq!(before, sorted.len(), "Doppelte in DIALOG_KEYS");
    // Jeder Schlüssel steht außerhalb von `detach.rs` als Text im Quelltext (am Aufruf oder in
    // der Zuordnung je Art); sonst ist er veraltet.
    let all: String = sources()
        .into_iter()
        .filter(|(n, _)| n != "detach.rs")
        .map(|(_, s)| strip_comments(&s))
        .collect();
    let unused: Vec<_> = DIALOG_KEYS
        .iter()
        .filter(|k| !all.contains(&format!("\"{k}\"")))
        .collect();
    assert!(unused.is_empty(), "nicht benutzt: {unused:?}");
    for k in DIALOG_KEYS {
        assert!(
            !k.is_empty()
                && k.chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'),
            "Schlüssel „{k}“: nur a-z, 0-9 und _"
        );
    }
}

#[test]
fn the_scanner_reads_marks() {
    let text = "let a = Dialog::new(t, S).dock_key(\"blend\").open(&mut o).show(ctx, f);\n\
                let b = ExportDialog::new(x);\n\
                // Dialog::new(im Kommentar)\n\
                fn p() -> Dialog { Dialog::new(t, S).id(i).fixed()\n}\n\
                Dialog::new(h, S).bottom_right_in(area).show(ctx, f);\n\
                Dialog::new(h, S).dock_key(self.kind.dock_key()).show(ctx, f);\n\
                Dialog::new(h, S).open(&mut o).show(ctx, |ui| ui.label(\".fixed()\"));\n";
    let marks: Vec<_> = calls(text).into_iter().map(|c| (c.line, c.marks)).collect();
    assert_eq!(
        marks,
        vec![
            (1, vec![Mark::Key(Some("blend".into()))]),
            (4, vec![Mark::Fixed]),
            (6, vec![Mark::BottomRight]),
            (7, vec![Mark::Key(None)]),
            (8, vec![]),
        ]
    );
}
