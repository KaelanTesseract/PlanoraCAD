//! PlanoraCAD-Setup: Graphit-Oberfläche, die den Inno-Setup-Motor unsichtbar ansteuert.
//!
//! Mit irgendeinem `/`-Argument (etwa vom stillen Update der App) zeigt die Hülle kein Fenster:
//! Sie packt den Motor aus, startet ihn mit genau demselben Argumenttext, wartet und endet mit
//! seinem Exit-Code – wie ein direkter Aufruf des Motors.

// Immer Fenster-Subsystem: Auch der Debug-Build darf bei der Weitergabe keine Konsole öffnen.
#![windows_subsystem = "windows"]

mod args;
mod engine;
#[expect(dead_code, reason = "die Oberfläche (Task 6) nutzt das")]
mod existing;
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "die Oberfläche (Task 6) nutzt das")
)]
mod progress;
#[expect(dead_code, reason = "die Oberfläche (Task 6) nutzt das")]
mod system;
#[expect(dead_code, reason = "die Oberfläche (Task 6) nutzt das")]
mod wizard;

fn main() {
    let args = args::raw_args();
    if args::is_passthrough(&args) {
        std::process::exit(passthrough(&args) as i32);
    }
    // Das Fenster kommt in einem späteren Schritt; bis dahin endet die Hülle ohne Arbeit.
}

/// Stille Weitergabe an den Motor; Exit-Code des Motors, oder 1, wenn er gar nicht lief
/// (kein Motor eingebettet, Auspacken oder Start gescheitert). Keine Meldung, kein Fenster.
fn passthrough(args: &[String]) -> u32 {
    let Ok(unpacked) = engine::Unpacked::unpack() else {
        return 1;
    };
    let code = match engine::spawn(&unpacked.exe, args, false) {
        Ok(process) => process.wait(),
        Err(_) => 1,
    };
    unpacked.cleanup();
    code
}
