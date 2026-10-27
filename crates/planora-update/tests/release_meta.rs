//! Prüft die Veröffentlichungs-Angaben des Repositorys: die Versionsnummer in `Cargo.toml`
//! und die Abschnitte in `CHANGELOG.md`. Beides muss zusammenpassen, sonst bricht
//! `release.yml` ab (siehe `docs/funktionen.md`, Abschnitt „Updates und Versionen“).

use planora_update::Version;
use std::path::PathBuf;

fn repo_file(name: &str) -> String {
    let path: PathBuf = [env!("CARGO_MANIFEST_DIR"), "..", "..", name]
        .iter()
        .collect();
    std::fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("{} nicht lesbar: {err}", path.display()))
}

/// `version = "…"` aus dem Abschnitt `[workspace.package]` der Wurzel-`Cargo.toml`.
fn workspace_version() -> String {
    let toml = repo_file("Cargo.toml");
    let mut in_section = false;
    for line in toml.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_section = line == "[workspace.package]";
            continue;
        }
        if in_section && let Some(rest) = line.strip_prefix("version") {
            let value = rest.trim_start().strip_prefix('=').expect("version = …");
            return value.trim().trim_matches('"').to_string();
        }
    }
    panic!("workspace.package.version fehlt in Cargo.toml");
}

/// Die Überschriften `## <Version>` in der Reihenfolge der Datei.
fn changelog_versions() -> Vec<String> {
    repo_file("CHANGELOG.md")
        .lines()
        .filter_map(|l| l.strip_prefix("## "))
        .map(|l| l.trim().to_string())
        .collect()
}

#[test]
fn cargo_version_parses_with_planora_update() {
    let text = workspace_version();
    let parsed = Version::parse(&text).unwrap_or_else(|err| panic!("{err}"));
    assert_eq!(parsed.to_string(), text);
}

#[test]
fn changelog_has_a_section_for_the_current_version() {
    let current = workspace_version();
    assert!(
        changelog_versions().contains(&current),
        "CHANGELOG.md hat keinen Abschnitt „## {current}“"
    );
}

#[test]
fn changelog_sections_are_newest_first_and_versions_parse() {
    let versions = changelog_versions();
    assert!(!versions.is_empty(), "CHANGELOG.md ohne Abschnitt");
    let parsed: Vec<Version> = versions
        .iter()
        .map(|v| Version::parse(v).unwrap_or_else(|err| panic!("Überschrift „{v}“: {err}")))
        .collect();
    for pair in parsed.windows(2) {
        assert!(pair[0] > pair[1], "{} steht vor {}", pair[0], pair[1]);
    }
    // Die neueste Überschrift ist die aktuelle Version (oder eine künftige, nie eine ältere).
    let current = Version::parse(&workspace_version()).unwrap();
    assert!(parsed[0] >= current);
}

#[test]
fn every_changelog_section_has_content() {
    // Jeder Abschnitt „## Version“ und jeder Unterabschnitt „### …“ darin braucht mindestens
    // eine Aufzählung; eine Überschrift ohne Inhalt fällt auf.
    let text = repo_file("CHANGELOG.md");
    let mut heading = String::new();
    let mut bullets = 0usize;
    let mut sub: Option<String> = None;
    let mut sub_bullets = 0usize;
    let mut sections = 0usize;
    fn finish_sub(sub: &mut Option<String>, n: usize, version: &str) {
        if let Some(name) = sub.take() {
            assert!(n > 0, "Unterabschnitt „{name}“ in „{version}“ ist leer");
        }
    }
    for line in text.lines() {
        if let Some(v) = line.strip_prefix("## ") {
            finish_sub(&mut sub, sub_bullets, &heading);
            if sections > 0 {
                assert!(bullets > 0, "Abschnitt „{heading}“ ohne Aufzählung");
            }
            heading = v.trim().to_string();
            sections += 1;
            bullets = 0;
            sub_bullets = 0;
        } else if let Some(name) = line.strip_prefix("### ") {
            finish_sub(&mut sub, sub_bullets, &heading);
            sub = Some(name.trim().to_string());
            sub_bullets = 0;
        } else if line.trim_start().starts_with("- ") {
            bullets += 1;
            sub_bullets += 1;
        }
    }
    finish_sub(&mut sub, sub_bullets, &heading);
    assert!(sections > 0, "CHANGELOG.md ohne Abschnitt");
    assert!(bullets > 0, "Abschnitt „{heading}“ ohne Aufzählung");
}

/// Das Release enthält die Symboldatei (für `tools/symbolize.ps1`); die Update-Funktion kennt
/// sie nicht und lädt sie nie.
#[test]
fn release_ships_the_pdb_but_update_never_loads_it() {
    let yml = repo_file(".github/workflows/release.yml");
    assert!(
        yml.contains("target/release/planora_app.pdb")
            && yml.contains("'dist/planoracad-windows-x64.pdb'"),
        "release.yml legt die .pdb nicht ab"
    );
    let src: PathBuf = [env!("CARGO_MANIFEST_DIR"), "src"].iter().collect();
    for e in std::fs::read_dir(&src).expect("src lesbar").flatten() {
        let text = std::fs::read_to_string(e.path()).unwrap_or_default();
        assert!(
            !text.to_ascii_lowercase().contains(".pdb"),
            "{} erwähnt .pdb",
            e.path().display()
        );
    }
}
