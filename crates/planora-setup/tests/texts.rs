//! Texte und Bilder der Oberfläche: Alle `setup-*`-Schlüssel gibt es auf Deutsch und Englisch,
//! und die eingebetteten Screenshots bleiben klein.

use planora_i18n::{english, german, t, with_lang};
use std::path::PathBuf;

/// Alle Schlüssel, die die Oberfläche des Installers benutzt.
const KEYS: &[&str] = &[
    "setup-window-title",
    "setup-steps-title",
    "setup-step-welcome",
    "setup-step-license",
    "setup-step-folder",
    "setup-step-install",
    "setup-step-done",
    "setup-welcome-text",
    "setup-welcome-update",
    "setup-license-accept",
    "setup-mode-user",
    "setup-mode-user-hint",
    "setup-mode-all",
    "setup-mode-all-hint",
    "setup-folder-browse",
    "setup-desktop-icon",
    "setup-hint-running",
    "setup-hint-recheck",
    "setup-hint-uac",
    "setup-hint-folder",
    "setup-installing",
    "setup-progress-prepare",
    "setup-progress-install",
    "setup-progress-postinstall",
    "setup-progress-done",
    "setup-progress-line",
    "setup-teaser-sketch",
    "setup-teaser-extrude",
    "setup-teaser-fillet",
    "setup-done-text",
    "setup-done-where",
    "setup-done-where-desktop",
    "setup-start-app",
    "setup-failed-title",
    "setup-failed-text",
    "setup-failed-text-nolog",
    "setup-open-log",
    "setup-no-engine",
    "setup-next",
    "setup-back",
    "setup-cancel",
    "setup-install",
    "setup-finish",
    "setup-close",
];

#[test]
fn all_setup_keys_exist_in_de_and_en() {
    for (name, lang) in [("de", german()), ("en", english())] {
        for key in KEYS {
            let text = with_lang(&lang, || t(key));
            assert_ne!(text, *key, "{key} fehlt in {name}");
            assert!(!text.trim().is_empty(), "{key} ist leer in {name}");
        }
    }
}

/// Breite aus dem Kopf (IHDR) einer PNG-Datei.
fn png_width(bytes: &[u8]) -> u32 {
    assert!(
        bytes.len() > 24 && bytes.starts_with(b"\x89PNG\r\n\x1a\n") && &bytes[12..16] == b"IHDR",
        "keine PNG-Datei"
    );
    u32::from_be_bytes([bytes[16], bytes[17], bytes[18], bytes[19]])
}

#[test]
fn setup_assets_are_small() {
    let dir: PathBuf = [env!("CARGO_MANIFEST_DIR"), "assets"].iter().collect();
    let mut count = 0;
    // Startseite 1280 breit, die drei Karten der Installieren-Seite 960 (16:9).
    for (name, width) in [
        ("startseite.png", 1280),
        ("skizze.png", 960),
        ("platte.png", 960),
        ("verrunden.png", 960),
    ] {
        let path = dir.join(name);
        let bytes = std::fs::read(&path)
            .unwrap_or_else(|err| panic!("{} nicht lesbar: {err}", path.display()));
        assert!(bytes.len() < 600 * 1024, "{name}: {} Bytes", bytes.len());
        assert_eq!(png_width(&bytes), width, "{name}");
        count += 1;
    }
    // Keine weiteren, ungeprüften Dateien im Ordner.
    let all = std::fs::read_dir(&dir)
        .expect("Ordner assets lesbar")
        .filter_map(Result::ok)
        .filter(|e| e.path().is_file())
        .count();
    assert_eq!(all, count, "unbekannte Dateien in {}", dir.display());
}
