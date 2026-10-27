//! Install-Modus aus der Registry und Start des Installers.
//!
//! Die Registry wird hier nie gelesen oder geschrieben: [`pick`] bekommt die Einträge als Werte.
//! Ein echter Installer startet nie; nur der Fehlerfall „Datei fehlt“ geht an Windows.

mod common;

use planora_app::install_mode::*;
use planora_app::update_install::{SETUP_RESTART_PARAM, setup_args, setup_verb};
use std::path::{Path, PathBuf};

fn entry(mode: &str, path: &Path) -> Option<RegEntry> {
    Some(RegEntry {
        mode: mode.into(),
        path: path.to_string_lossy().into_owned(),
    })
}

/// Zwei echte Ordner unter einem eigenen Temp-Ordner.
fn two_dirs(prefix: &str) -> (PathBuf, PathBuf) {
    let root = common::temp_root(prefix);
    let a = root.join("eins");
    let b = root.join("zwei");
    std::fs::create_dir_all(&a).unwrap();
    std::fs::create_dir_all(&b).unwrap();
    (a, b)
}

#[test]
fn user_entry_counts_only_for_its_own_folder() {
    let (inst, other) = two_dirs("install-mode-user");
    assert_eq!(
        pick(&inst, entry("user", &inst), None),
        Some(InstallMode::User)
    );
    assert_eq!(pick(&other, entry("user", &inst), None), None);
    assert_eq!(pick(&inst, entry("all", &inst), None), None);
}

#[test]
fn all_users_entry_counts_only_for_its_own_folder() {
    let (inst, other) = two_dirs("install-mode-all");
    assert_eq!(
        pick(&inst, None, entry("all", &inst)),
        Some(InstallMode::AllUsers)
    );
    assert_eq!(pick(&other, None, entry("all", &inst)), None);
    assert_eq!(pick(&inst, None, entry("user", &inst)), None);
}

#[test]
fn both_entries_pick_the_one_of_the_running_exe() {
    let (user_dir, all_dir) = two_dirs("install-mode-both");
    let build = user_dir.parent().unwrap().join("target");
    std::fs::create_dir_all(&build).unwrap();
    let hkcu = || entry("user", &user_dir);
    let hklm = || entry("all", &all_dir);
    assert_eq!(pick(&all_dir, hkcu(), hklm()), Some(InstallMode::AllUsers));
    assert_eq!(pick(&user_dir, hkcu(), hklm()), Some(InstallMode::User));
    assert_eq!(pick(&build, hkcu(), hklm()), None);
}

#[test]
fn folder_match_ignores_case_and_trailing_separator() {
    let (inst, other) = two_dirs("install-mode-case");
    let upper = PathBuf::from(inst.to_string_lossy().to_uppercase());
    let trailing = PathBuf::from(format!("{}{}", inst.display(), std::path::MAIN_SEPARATOR));
    assert!(same_dir(&inst, &trailing));
    assert!(!same_dir(&inst, &other));
    if cfg!(windows) {
        assert!(same_dir(&inst, &upper));
    }
    // Ordner, die es nicht gibt: nur Textvergleich.
    assert!(same_dir(
        Path::new(r"C:\Gibt\Es\Nicht\PlanoraCAD"),
        Path::new(r"c:\gibt\es\nicht\planoracad\")
    ));
    assert!(same_dir(
        Path::new(r"\\?\C:\Gibt\Es\Nicht\PlanoraCAD"),
        Path::new(r"c:\gibt\es\nicht\planoracad")
    ));
    assert!(!same_dir(
        Path::new(r"C:\Gibt\Es\Nicht\PlanoraCAD"),
        Path::new(r"C:\Gibt\Es\Nicht\PlanoraCAD2")
    ));
}

#[test]
fn no_entries_means_not_installed() {
    let (inst, _) = two_dirs("install-mode-none");
    assert_eq!(pick(&inst, None, None), None);
}

#[test]
fn setup_args_are_exact() {
    let base = [
        "/VERYSILENT",
        "/SUPPRESSMSGBOXES",
        "/NORESTART",
        "/CLOSEAPPLICATIONS",
        "/RESTARTAPPLICATIONS",
    ];
    let mut user = base.to_vec();
    user.extend(["/CURRENTUSER", "/NACHUPDATE=1"]);
    let mut all = base.to_vec();
    all.extend(["/ALLUSERS", "/NACHUPDATE=1"]);
    assert_eq!(setup_args(InstallMode::User), user);
    assert_eq!(setup_args(InstallMode::AllUsers), all);
    assert_eq!(
        *setup_args(InstallMode::User).last().unwrap(),
        SETUP_RESTART_PARAM
    );
}

#[test]
fn setup_verb_asks_windows_for_admin_only_for_all_users() {
    assert_eq!(setup_verb(InstallMode::User), "open");
    assert_eq!(setup_verb(InstallMode::AllUsers), "runas");
}

#[test]
fn installer_script_matches_the_app() {
    let iss = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../installer/planoracad.iss");
    let text = std::fs::read_to_string(&iss).unwrap();
    assert!(text.contains(r#"Subkey: "Software\PlanoraCAD""#));
    assert_eq!(REG_SUBKEY, r"Software\PlanoraCAD");
    assert!(text.contains(&format!(r#"ValueName: "{REG_MODE}""#)));
    assert!(text.contains(&format!(r#"ValueName: "{REG_PATH}""#)));
    assert!(text.contains(r#"ValueName: "InstallMode""#));
    assert!(text.contains(r#"ValueName: "InstallPath""#));
    assert!(text.contains(r#"ValueData: "user""#));
    assert!(text.contains(r#"ValueData: "all""#));
    assert!(text.contains("{param:NACHUPDATE|0}"));
    assert_eq!(SETUP_RESTART_PARAM, "/NACHUPDATE=1");
}

#[cfg(windows)]
#[test]
fn shell_launcher_reports_a_missing_file() {
    use planora_app::update_install::{LaunchError, SetupLauncher, ShellLauncher};
    let root = common::temp_root("install-mode-launch");
    std::fs::create_dir_all(&root).unwrap();
    let missing = root.join("fehlt.exe");
    match ShellLauncher.launch(&missing, InstallMode::User) {
        Err(LaunchError::Failed(msg)) => assert!(!msg.is_empty()),
        other => panic!("erwartet Failed, war {other:?}"),
    }
}

#[test]
fn no_launch_never_starts_anything() {
    use planora_app::update_install::{LaunchError, NoLaunch, SetupLauncher};
    assert!(matches!(
        NoLaunch.launch(Path::new("setup.exe"), InstallMode::AllUsers),
        Err(LaunchError::Failed(_))
    ));
}

#[cfg(not(windows))]
#[test]
fn detect_is_none_off_windows() {
    assert_eq!(
        detect(Some(Path::new("/opt/planoracad/PlanoraCAD.exe"))),
        None
    );
    assert_eq!(detect(None), None);
}
