//! Prüft die Veröffentlichungs-Angaben des Repositorys: die Versionsnummer in `Cargo.toml`
//! und die Abschnitte in `CHANGELOG.md`. Beides muss zusammenpassen, sonst bricht
//! `release.yml` ab (siehe README, Abschnitt „Updates und Versionen“).

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
    let text = repo_file("CHANGELOG.md");
    let mut sections = text.split("\n## ").skip(1);
    for section in sections.by_ref() {
        let heading = section.lines().next().unwrap_or("");
        assert!(
            section.lines().any(|l| l.trim_start().starts_with("- ")),
            "Abschnitt „{heading}“ ohne Aufzählung"
        );
    }
}
