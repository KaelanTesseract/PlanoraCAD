//! Wächter: Einzeilige Textfelder gehen in `src` nur über `theme::text_field` bzw.
//! `theme::text_field_enabled` (26 px hoch, vertiefte Fläche, Fokusring, Wert in
//! `text_primary`). Ein Aufruf von `TextEdit::singleline(` oder `text_edit_singleline(` an anderer
//! Stelle lässt den Test scheitern. Mehrzeilige Textfelder und Zahlenfelder (`DragValue`) gehen
//! über `theme::field` (sonst stünde der eingegebene Wert grau statt in `text_primary`).

use std::path::Path;

/// Ausnahmen (Datei, Grund). Leer: es gibt keine.
const ALLOWED: &[(&str, &str)] = &[];

/// Fundstellen `(Zeile, Text)` von Textfeldern, die nicht über die Theme-Helfer laufen.
fn raw_text_edits(source: &str) -> Vec<(usize, String)> {
    let mut found = Vec::new();
    for needle in [
        "TextEdit::singleline(",
        "text_edit_singleline(",
        "TextEdit::multiline(",
        "text_edit_multiline(",
        "DragValue::new(",
    ] {
        let mut from = 0;
        while let Some(i) = source[from..].find(needle) {
            let at = from + i;
            from = at + needle.len();
            // Was davor steht, ohne Leerraum: muss mit `text_field(ui,` bzw.
            // `text_field_enabled(ui,<Ausdruck>,` enden (optional `egui::`).
            let before: String = source[..at]
                .chars()
                .filter(|c| !c.is_whitespace())
                .collect();
            let before = before.strip_suffix("egui::").unwrap_or(&before);
            let single = needle.contains("singleline");
            let helper = (!single && before.ends_with("theme::field(ui,"))
                || before.ends_with("theme::text_field(ui,")
                || before
                    .rfind("theme::text_field_enabled(ui,")
                    .is_some_and(|j| {
                        let arg = &before[j + "theme::text_field_enabled(ui,".len()..];
                        arg.ends_with(',') && !arg[..arg.len() - 1].contains([',', ';', '{', '}'])
                    });
            if !helper {
                let line = source[..at].lines().count();
                let text = source.lines().nth(line.saturating_sub(1)).unwrap_or("");
                found.push((line, text.trim().to_string()));
            }
        }
    }
    found
}

/// Alle `.rs`-Dateien unter `dir`, auch in Unterordnern (z. B. `keymap/`).
fn rust_files(dir: &Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            out.extend(rust_files(&path));
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
    out
}

#[test]
fn text_fields_go_through_the_theme() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut bad = Vec::new();
    for path in rust_files(&src) {
        let name = path
            .strip_prefix(&src)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        if name == "theme.rs" || ALLOWED.iter().any(|(f, _)| *f == name) {
            continue;
        }
        let source = std::fs::read_to_string(&path).unwrap();
        for (line, text) in raw_text_edits(&source) {
            bad.push(format!("{name}:{line}: {text}"));
        }
    }
    assert!(
        bad.is_empty(),
        "Text- oder Zahlenfelder ohne theme::text_field/theme::field:\n{}",
        bad.join("\n")
    );
}

#[test]
fn the_scanner_finds_raw_text_edits_and_accepts_the_helpers() {
    let ok = "let r = theme::text_field(\n    ui,\n    egui::TextEdit::singleline(&mut a),\n);\n\
              theme::text_field_enabled(ui, !read_only, egui::TextEdit::singleline(&mut b));";
    assert!(raw_text_edits(ok).is_empty(), "{:?}", raw_text_edits(ok));
    let bad = "ui.add(egui::TextEdit::singleline(&mut a));\nui.text_edit_singleline(&mut b);\n\
               ui.add_enabled(x, TextEdit::singleline(&mut c));";
    let found = raw_text_edits(bad);
    assert_eq!(found.len(), 3, "{found:?}");
    let ok = "theme::field(ui, egui::DragValue::new(v).speed(1.0));
              theme::field(
    ui,
    egui::TextEdit::multiline(&mut d),
);";
    assert!(raw_text_edits(ok).is_empty(), "{:?}", raw_text_edits(ok));
    let bad = "ui.add(egui::DragValue::new(v));
ui.text_edit_multiline(&mut d);
               theme::field(ui, egui::TextEdit::singleline(&mut a));";
    let found = raw_text_edits(bad);
    assert_eq!(found.len(), 3, "{found:?}");
}
