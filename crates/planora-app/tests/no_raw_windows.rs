//! Wächter: Fenster entstehen in `src` nur über `dialog::Dialog` (Datei `dialog.rs`). Ein
//! `egui::Window`, das alte `theme::window(` oder `wide_window` an anderer Stelle lässt den Test
//! scheitern. Kommentarzeilen und Zeilenenden nach `//` zählen nicht.

use std::path::Path;

const FORBIDDEN: [&str; 3] = ["egui::Window", "theme::window(", "wide_window"];

/// Fundstellen `(Zeile, Wort)` im Quelltext, ohne Kommentare.
fn raw_windows(source: &str) -> Vec<(usize, &'static str)> {
    let mut found = Vec::new();
    for (i, line) in source.lines().enumerate() {
        let code = line.split("//").next().unwrap_or("");
        for word in FORBIDDEN {
            if code.contains(word) {
                found.push((i + 1, word));
            }
        }
    }
    found
}

#[test]
fn windows_only_through_dialog() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut bad = Vec::new();
    for entry in std::fs::read_dir(&src).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        if path.extension().is_none_or(|e| e != "rs") || name == "dialog.rs" {
            continue;
        }
        let source = std::fs::read_to_string(&path).unwrap();
        for (line, word) in raw_windows(&source) {
            bad.push(format!("{name}:{line}: {word}"));
        }
    }
    assert!(bad.is_empty(), "Fenster am Dialog vorbei: {bad:?}");
}

#[test]
fn the_scanner_finds_raw_windows_and_skips_comments() {
    let text = "// egui::Window steht im Kommentar\n\
                let w = egui::Window::new(\"x\"); // Hinweis\n\
                let v = theme::window(ctx, \"y\");\n\
                wide_window(ctx);\n\
                let ok = Dialog::new(\"z\", DialogSize::Normal);\n";
    assert_eq!(
        raw_windows(text),
        [
            (2, "egui::Window"),
            (3, "theme::window("),
            (4, "wide_window")
        ]
    );
}
