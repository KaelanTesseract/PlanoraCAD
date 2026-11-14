//! Prüft als Text, dass das Bauskript erst den Inno-Motor und dann die Graphit-Hülle baut,
//! dass der Motor nie nach `dist` gelangt und dass der Release-Ablauf Inno 6.7 sicherstellt.

use std::path::PathBuf;

fn lese(teile: &[&str]) -> String {
    let mut path: PathBuf = [env!("CARGO_MANIFEST_DIR"), "..", ".."].iter().collect();
    for t in teile {
        path.push(t);
    }
    std::fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("{} nicht lesbar: {err}", path.display()))
}

fn bauskript() -> String {
    lese(&["tools", "build-installer.ps1"])
}

#[test]
fn build_script_checks_inno_version() {
    let s = bauskript();
    assert!(s.contains("ProductVersion"));
    assert!(s.contains("6.7"));
    assert!(s.contains("Inno Setup 6.7 oder neuer"));
}

#[test]
fn build_script_builds_engine_then_shell() {
    let s = bauskript();
    let motor = s
        .find("/F\"planoracad-engine-x64\"")
        .expect("Motor-Name fehlt");
    let huelle = s.find("-p planora-setup").expect("Hüllen-Build fehlt");
    assert!(s.contains("planoracad-setup-x64.exe"), "Ausgabename fehlt");
    // Die Hülle wird erst nach ihrem Bau nach <OutDir>\<Ausgabename> kopiert.
    let kopie = s
        .find("Copy-Item -LiteralPath $shell -Destination $setup")
        .expect("Kopie fehlt");
    assert!(motor < huelle, "erst Motor, dann Hülle");
    assert!(huelle < kopie, "erst Hülle, dann Kopie nach <OutDir>");
    assert!(s.contains("PLANORA_SETUP_ENGINE"));
}

#[test]
fn engine_never_lands_in_dist() {
    let s = bauskript();
    assert!(s.contains(r"target\installer-engine"));
    // Der Motor wird nur unter target\installer-engine abgelegt, nie nach <OutDir>.
    assert!(
        !s.contains("/O$OutDir"),
        "ISCC darf nicht nach OutDir schreiben"
    );
    assert!(!s.contains("\"/O$OutDir\""));
}

#[test]
fn release_ensures_inno_67() {
    let s = lese(&[".github", "workflows", "release.yml"]);
    assert!(s.contains("6.7.3"));
    assert!(s.contains("ProductVersion"));
    assert!(s.contains("6.7"));
    let app = s.find("-p planora-app").expect("App-Build fehlt");
    let skript = s.find("tools/build-installer.ps1 ").expect("Aufruf fehlt");
    assert!(app < skript, "erst App bauen, dann Installer");
}

#[test]
fn build_rs_fails_when_engine_file_missing() {
    let s = lese(&["crates", "planora-setup", "build.rs"]);
    assert!(
        s.contains("panic!"),
        "build.rs muss bei gesetzter, aber fehlender Motor-Datei abbrechen"
    );
    assert!(s.contains("PLANORA_SETUP_ENGINE"));
}
