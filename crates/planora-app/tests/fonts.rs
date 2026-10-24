use planora_app::fonts;

fn families() -> [egui::FontFamily; 5] {
    [
        egui::FontFamily::Proportional,
        egui::FontFamily::Monospace,
        fonts::medium(),
        fonts::semibold(),
        fonts::icons(),
    ]
}

#[test]
fn all_families_have_fonts() {
    let defs = fonts::definitions();
    for family in families() {
        let names = defs.families.get(&family).expect("Familie vorhanden");
        assert!(!names.is_empty(), "{family:?} ohne Schrift");
        for name in names {
            assert!(!defs.font_data[name].font.is_empty());
        }
    }
}

#[test]
fn proportional_starts_with_montserrat_and_falls_back_to_roboto() {
    let defs = fonts::definitions();
    let names = |family: egui::FontFamily| defs.families[&family].clone();
    assert_eq!(
        names(egui::FontFamily::Proportional),
        ["Montserrat-Regular", "Roboto"]
    );
    assert_eq!(names(fonts::medium()), ["Montserrat-Medium", "Roboto"]);
    assert_eq!(names(fonts::semibold()), ["Montserrat-SemiBold", "Roboto"]);
    assert_eq!(names(fonts::icons()), ["Phosphor-Light"]);
    assert_eq!(names(egui::FontFamily::Monospace), ["Roboto"]);
}

/// Zeichen außerhalb von ASCII, die in Oberflächentexten vorkommen dürfen. Der Test unten prüft,
/// dass jedes davon in Montserrat oder in der Ersatzschrift Roboto vorhanden ist; alles andere
/// erscheint als Kästchen (z. B. ●, ▸, →) und ist verboten.
const ALLOWED: &str = "ÄÖÜäöüß„“–−·×°±²³…ØΔ≈";

/// Hat die Schrift in ihrer Zeichentabelle einen Eintrag für `c`? (Die egui-Abfrage `has_glyph`
/// taugt hier nicht: Sie meldet Zeichen der ersten Schrift der Familie fälschlich als fehlend,
/// sobald diese Schrift auch das Ersatzzeichen „?“ liefert.)
fn font_has_char(bytes: &[u8], c: char) -> bool {
    use skrifa::MetadataProvider;
    skrifa::FontRef::new(bytes)
        .expect("gültige Schriftdatei")
        .charmap()
        .map(c)
        .is_some()
}

#[test]
fn every_allowed_character_has_a_glyph_in_proportional() {
    let defs = fonts::definitions();
    for family in [
        egui::FontFamily::Proportional,
        fonts::medium(),
        fonts::semibold(),
    ] {
        let chain = &defs.families[&family];
        let missing: Vec<char> = ALLOWED
            .chars()
            .filter(|c| {
                !chain
                    .iter()
                    .any(|n| font_has_char(&defs.font_data[n].font, *c))
            })
            .collect();
        assert!(missing.is_empty(), "{family:?}: kein Glyph für {missing:?}");
    }
}

/// Jedes Nicht-ASCII-Zeichen in den Quelltexten der Oberfläche (außerhalb von Kommentaren)
/// muss in der Erlaubnisliste stehen. Ausgenommen sind Zeilen mit ASCII-Ersatz in der
/// Skizzenansicht und der gezeichnete Tab-Punkt (`DIRTY_MARK`).
#[test]
fn every_non_ascii_character_in_the_ui_sources_is_known_to_exist_in_the_font() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut bad = std::collections::BTreeSet::new();
    // Quelltexte samt Modulordnern (etwa `history_bar/`).
    let mut files = Vec::new();
    let mut dirs = vec![dir];
    while let Some(d) = dirs.pop() {
        for e in std::fs::read_dir(d).unwrap().flatten() {
            let path = e.path();
            if path.is_dir() {
                dirs.push(path);
            } else {
                files.push(path);
            }
        }
    }
    for path in files {
        let text = std::fs::read_to_string(path).unwrap();
        for line in text.lines() {
            if line.trim_start().starts_with("//")
                || line.contains("symbol(ctx")
                || line.contains("DIRTY_MARK: char")
            {
                continue;
            }
            bad.extend(
                line.chars()
                    .filter(|c| !c.is_ascii() && !ALLOWED.contains(*c)),
            );
        }
    }
    assert!(
        bad.is_empty(),
        "Zeichen ohne Glyph in Montserrat/Roboto: {bad:?}"
    );
}
