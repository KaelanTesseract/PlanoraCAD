//! Prüft das Inno-Setup-Skript `installer/planoracad.iss` auf die festen Werte, die Updates,
//! Deinstallation und App-Erkennung voraussetzen (AppId, Registry-Namen, Parameter, Dateinamen).

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

/// Alle Zeilen ohne Kommentare (`;` am Anfang), getrimmt, ohne Leerzeilen.
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

fn has_section(name: &str) -> bool {
    let header = format!("[{name}]");
    code_lines().iter().any(|l| l.eq_ignore_ascii_case(&header))
}

fn find<'a>(lines: &'a [String], needles: &[&str]) -> &'a String {
    lines
        .iter()
        .find(|l| needles.iter().all(|n| l.contains(n)))
        .unwrap_or_else(|| panic!("keine Zeile mit {needles:?} in {lines:#?}"))
}

#[test]
fn setup_section_has_the_fixed_values() {
    let setup = section("Setup");
    for expected in [
        "AppId={{D42C66D0-6AB0-4D5C-A693-9616105847B8}",
        "AppName=PlanoraCAD",
        "AppVersion={#AppVersion}",
        "VersionInfoVersion={#AppVersionNumeric}",
        "AppPublisher=Dennis Guse",
        r"DefaultDirName={autopf}\PlanoraCAD",
        "PrivilegesRequired=lowest",
        "PrivilegesRequiredOverridesAllowed=commandline dialog",
        "ArchitecturesAllowed=x64compatible",
        "ArchitecturesInstallIn64BitMode=x64compatible",
        r"LicenseFile=..\LICENSE",
        r"SetupIconFile=..\assets\logo\planoracad.ico",
        r"UninstallDisplayIcon={app}\PlanoraCAD.exe",
        "OutputBaseFilename=planoracad-setup-x64",
        "CloseApplications=yes",
        // Neustart nur über `[Run]` mit /NACHUPDATE=1, nicht zusätzlich über den Neustart-Manager.
        "RestartApplications=no",
        "ShowLanguageDialog=no",
        "DisableProgramGroupPage=yes",
    ] {
        assert!(
            setup.iter().any(|l| l == expected),
            "Zeile fehlt in [Setup]: {expected}"
        );
    }
}

#[test]
fn version_is_never_hard_coded() {
    let lines = code_lines();
    for line in &lines {
        if let Some(rest) = line.strip_prefix("AppVersion=") {
            assert!(
                !rest.chars().any(|c| c.is_ascii_digit()),
                "Version fest eingetragen: {line}"
            );
        }
    }
    let ifndef = lines
        .iter()
        .position(|l| l == "#ifndef AppVersion")
        .expect("#ifndef AppVersion fehlt");
    assert!(
        lines[ifndef + 1].starts_with("#error"),
        "nach #ifndef AppVersion muss #error folgen"
    );
}

#[test]
fn languages_are_english_and_german() {
    let langs = section("Languages");
    assert_eq!(langs.len(), 2, "{langs:#?}");
    assert!(
        langs[0].contains(r#"Name: "en""#)
            && langs[0].contains(r#"MessagesFile: "compiler:Default.isl""#)
    );
    assert!(
        langs[1].contains(r#"Name: "de""#)
            && langs[1].contains(r#"MessagesFile: "compiler:Languages\German.isl""#)
    );
}

#[test]
fn desktop_icon_task_is_on_by_default() {
    let tasks = section("Tasks");
    let task = find(&tasks, &[r#"Name: "desktopicon""#]);
    assert!(!task.contains("unchecked"), "{task}");
    let icons = section("Icons");
    find(&icons, &[r"{autoprograms}\PlanoraCAD"]);
    let desk = find(&icons, &[r"{autodesktop}\PlanoraCAD"]);
    assert!(desk.contains("Tasks: desktopicon"), "{desk}");
}

#[test]
fn files_are_exe_and_licenses() {
    let files = section("Files");
    find(
        &files,
        &[
            r#"Source: "{#SourceExe}""#,
            r#"DestDir: "{app}""#,
            r#"DestName: "PlanoraCAD.exe""#,
        ],
    );
    find(
        &files,
        &[
            r#"Source: "{#LicensesFile}""#,
            r#"DestName: "THIRD-PARTY-LICENSES.txt""#,
        ],
    );
    find(
        &files,
        &[r#"Source: "..\LICENSE""#, r#"DestName: "LICENSE.txt""#],
    );
}

#[test]
fn registry_remembers_mode_and_path() {
    let reg = section("Registry");
    for l in &reg {
        assert!(l.contains("Root: HKA"), "{l}");
        assert!(l.contains(r#"Subkey: "Software\PlanoraCAD""#), "{l}");
        assert!(l.contains("uninsdeletekey"), "{l}");
    }
    let user = find(
        &reg,
        &[r#"ValueName: "InstallMode""#, r#"ValueData: "user""#],
    );
    assert!(user.contains("Check: not IsAdminInstallMode"), "{user}");
    let all = find(
        &reg,
        &[r#"ValueName: "InstallMode""#, r#"ValueData: "all""#],
    );
    assert!(
        all.contains("Check: IsAdminInstallMode") && !all.contains("not IsAdminInstallMode"),
        "{all}"
    );
    find(
        &reg,
        &[r#"ValueName: "InstallPath""#, r#"ValueData: "{app}""#],
    );
}

#[test]
fn user_data_is_never_deleted() {
    assert!(!has_section("UninstallDelete"));
    assert!(!has_section("InstallDelete"));
    for line in code_lines() {
        for forbidden in ["{userappdata}", "{userdocs}", r"{localappdata}\PlanoraCAD"] {
            assert!(!line.contains(forbidden), "{forbidden} in: {line}");
        }
    }
}

#[test]
fn restart_after_update_only_with_the_parameter() {
    let run = section("Run");
    let a = find(
        &run,
        &[
            r#"Filename: "{app}\PlanoraCAD.exe""#,
            "postinstall",
            "skipifsilent",
        ],
    );
    assert!(!a.contains("IsUpdateRun"), "{a}");
    let b = find(
        &run,
        &[
            r#"Filename: "{app}\PlanoraCAD.exe""#,
            "Check: IsUpdateRun and not IsAdminInstallMode",
        ],
    );
    assert!(b.contains("nowait"), "{b}");
    let c = find(
        &run,
        &[
            r#"Filename: "{win}\explorer.exe""#,
            r#"Parameters: """{app}\PlanoraCAD.exe""""#,
            "Check: IsUpdateRun and IsAdminInstallMode",
        ],
    );
    assert!(c.contains("nowait"), "{c}");
    assert!(
        section("Code")
            .iter()
            .any(|l| l.contains("{param:NACHUPDATE|0}")),
        "[Code] muss {{param:NACHUPDATE|0}} lesen"
    );
}
