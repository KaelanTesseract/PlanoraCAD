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
        .find(" /Fplanoracad-engine-x64 ")
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

/// Kein ISCC-Argument darf Anführungszeichen im Inneren tragen: PowerShell ab 7.3 maskiert sie
/// (`\"`), und ISCC scheitert dann (Release läuft mit `shell: pwsh`).
#[test]
fn iscc_args_have_no_embedded_quotes() {
    let s = bauskript();
    let aufruf = s
        .lines()
        .find(|l| l.trim_start().starts_with("& $isccPath /Qp"))
        .expect("ISCC-Aufruf fehlt");
    assert!(
        !aufruf.contains('\''),
        "keine Argumente in einfachen Anführungszeichen: {aufruf}"
    );
    for arg in aufruf.split_whitespace().skip(2) {
        let innen = arg.trim_matches('"');
        assert!(!innen.contains('"'), "Anführungszeichen im Argument {arg}");
    }
    assert!(aufruf.contains("\"/O$engineDir\""));
}

/// Die Hülle wird dort gesucht, wo cargo sie ablegt (`CARGO_TARGET_DIR`, sonst `target`), und
/// eine alte Hülle wird vor dem Bau gelöscht.
#[test]
fn shell_path_follows_cargo_target_dir_and_old_shell_is_removed() {
    let s = bauskript();
    let ziel = s
        .find("$env:CARGO_TARGET_DIR")
        .expect("CARGO_TARGET_DIR fehlt");
    let pfad = s
        .find(r"$shell = Join-Path $targetDir 'release\planoracad-setup.exe'")
        .expect("Hüllen-Pfad fehlt");
    let loeschen = s
        .find("Remove-Item -LiteralPath $shell -Force")
        .expect("alte Hülle wird nicht gelöscht");
    let bau = s.find("-p planora-setup").expect("Hüllen-Build fehlt");
    assert!(ziel < pfad && pfad < loeschen && loeschen < bau);
    assert!(!s.contains(r"Join-Path $root 'target\release\planoracad-setup.exe'"));
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
