//! Prüft den Inno-Setup-Motor `installer/planoracad.iss`: Fortschrittsdatei (nur mit Parameter),
//! dunkle Graphit-Farben und den unveränderten Update-Vertrag.

use std::path::PathBuf;

fn script() -> String {
    let path: PathBuf = [
        env!("CARGO_MANIFEST_DIR"),
        "..",
        "..",
        "installer",
        "planoracad.iss",
    ]
    .iter()
    .collect();
    std::fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("{} nicht lesbar: {err}", path.display()))
}

/// Alle Zeilen ohne Kommentare (`;` oder `//` am Anfang), getrimmt, ohne Leerzeilen.
fn code_lines() -> Vec<String> {
    script()
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty() && !l.starts_with(';') && !l.starts_with("//"))
        .collect()
}

/// Zeilen eines Abschnitts (`[Setup]` usw.), ohne Kommentare.
fn section(name: &str) -> Vec<String> {
    let header = format!("[{name}]");
    let mut inside = false;
    let mut out = Vec::new();
    for line in code_lines() {
        if line.starts_with('[') && line.ends_with(']') {
            inside = line.eq_ignore_ascii_case(&header);
            continue;
        }
        if inside {
            out.push(line);
        }
    }
    out
}

#[test]
fn progress_file_is_written_only_with_param() {
    let code = section("Code");
    let joined = code.join("\n");
    assert!(
        joined.contains("{param:PROGRESSFILE|}"),
        "[Code] muss {{param:PROGRESSFILE|}} lesen"
    );
    assert!(joined.contains("procedure CurInstallProgressChanged"));
    assert!(joined.contains("procedure CurStepChanged"));

    // Jede Schreibstelle muss hinter einer Pruefung auf leeren Pfad stehen.
    let writes: Vec<usize> = code
        .iter()
        .enumerate()
        .filter(|(_, l)| l.contains("SaveStringToFile"))
        .map(|(i, _)| i)
        .collect();
    assert!(!writes.is_empty(), "keine Schreibstelle gefunden");
    for i in writes {
        let guarded = code[..i]
            .iter()
            .rev()
            .take(4)
            .any(|l| l.contains("<> ''") || l.contains("= ''") && l.contains("Exit"));
        assert!(
            guarded,
            "Schreibstelle ohne Pruefung auf leeren Pfad: {}",
            code[i]
        );
    }
    assert!(joined.contains("percent="));
    assert!(joined.contains("step="));
    for step in ["prepare", "install", "postinstall", "done"] {
        assert!(joined.contains(step), "Schritt fehlt: {step}");
    }
}

#[test]
fn wizard_is_dark_graphit() {
    let setup = section("Setup");
    for expected in [
        "WizardStyle=modern dynamic",
        "WizardBackColor=#ffffff",
        "WizardBackColorDynamicDark=#181818",
    ] {
        assert!(
            setup.iter().any(|l| l == expected),
            "Zeile fehlt in [Setup]: {expected}"
        );
    }
}

#[test]
fn update_contract_unchanged() {
    let lines = code_lines();
    let all = lines.join("\n");
    assert!(all.contains("AppId={{D42C66D0-6AB0-4D5C-A693-9616105847B8}"));
    assert!(all.contains("function IsUpdateRun: Boolean"));
    assert!(all.contains("{param:NACHUPDATE|0}"));
    assert!(
        section("Setup")
            .iter()
            .any(|l| l == "RestartApplications=no")
    );
}
