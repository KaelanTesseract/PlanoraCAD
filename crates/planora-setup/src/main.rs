//! PlanoraCAD-Setup: Graphit-Oberfläche, die den Inno-Setup-Motor unsichtbar ansteuert.
//!
//! Mit irgendeinem `/`-Argument (etwa vom stillen Update der App) zeigt die Hülle kein Fenster:
//! Sie packt den Motor aus, startet ihn mit genau demselben Argumenttext, wartet und endet mit
//! seinem Exit-Code – wie ein direkter Aufruf des Motors. Lässt sich die Oberfläche nicht öffnen,
//! startet der normale Inno-Assistent; ohne eingebetteten Motor erscheint nur eine Meldung.

// Immer Fenster-Subsystem: Auch der Debug-Build darf bei der Weitergabe keine Konsole öffnen.
#![windows_subsystem = "windows"]

mod args;
mod engine;
mod existing;
mod progress;
mod system;
mod ui;
mod wizard;

/// Was die Hülle beim Start tut.
#[derive(Debug, PartialEq, Eq)]
enum Action {
    /// Stille Weitergabe an den Motor (irgendein `/`-Argument).
    Passthrough,
    /// Graphit-Oberfläche.
    Gui,
    /// Die Oberfläche ließ sich nicht öffnen (etwa ohne wgpu-Grafik): normaler Inno-Assistent.
    FallbackInno,
    /// Kein Motor eingebettet (Entwicklungs-Build): nur eine Meldung.
    NoEngine,
}

/// Entscheidet, was die Hülle tut. Die Weitergabe hat Vorrang vor allem; ohne Motor öffnet sich
/// nie die Oberfläche; scheitert die Oberfläche (`gui`), übernimmt der Inno-Assistent.
fn decide(passthrough: bool, has_engine: bool, gui: Result<(), String>) -> Action {
    if passthrough {
        Action::Passthrough
    } else if !has_engine {
        Action::NoEngine
    } else if gui.is_err() {
        Action::FallbackInno
    } else {
        Action::Gui
    }
}

fn main() {
    let args = args::raw_args();
    let passthrough_args = args::is_passthrough(&args);
    let has_engine = engine::engine_bytes().is_some();
    // Vor dem Öffnen der Oberfläche zählt deren Ergebnis noch nicht (Ok = noch nicht versucht).
    match decide(passthrough_args, has_engine, Ok(())) {
        // Ohne Motor endet die Weitergabe still mit 1, nie mit einer Meldung.
        Action::Passthrough => std::process::exit(passthrough(&args) as i32),
        Action::NoEngine => {
            set_language();
            show_no_engine();
            std::process::exit(1);
        }
        Action::Gui | Action::FallbackInno => {}
    }
    set_language();
    let gui = ui::run(wizard::Wizard::new(existing::detect())).map_err(|err| err.to_string());
    if decide(false, has_engine, gui) == Action::FallbackInno {
        std::process::exit(fallback_inno() as i32);
    }
}

/// Sprache der Oberfläche nach Windows: Deutsch, sonst Englisch.
fn set_language() {
    planora_i18n::set_global(
        planora_i18n::find(system::ui_lang()).unwrap_or_else(planora_i18n::english),
    );
}

/// Meldung „kein Installer in dieser Fassung“ (Windows-Meldungsfenster).
#[cfg(windows)]
fn show_no_engine() {
    use planora_i18n::tr;
    use windows_sys::Win32::UI::WindowsAndMessaging::{MB_ICONINFORMATION, MB_OK, MessageBoxW};
    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }
    let text = wide(&tr!("setup-no-engine"));
    let caption = wide(&tr!("setup-window-title"));
    // SAFETY: Beide Zeichenketten sind nullterminiert und leben bis nach dem Aufruf; ohne
    // Besitzerfenster (Null-Griff) ist das Meldungsfenster eigenständig.
    unsafe {
        MessageBoxW(
            std::ptr::null_mut(),
            text.as_ptr(),
            caption.as_ptr(),
            MB_OK | MB_ICONINFORMATION,
        );
    }
}

/// Außerhalb von Windows gibt es kein Meldungsfenster; der Text geht auf die Fehlerausgabe.
#[cfg(not(windows))]
fn show_no_engine() {
    eprintln!("{}", planora_i18n::tr!("setup-no-engine"));
}

/// Rückfall ohne Graphit-Oberfläche: Motor ohne Parameter starten (normaler Inno-Assistent),
/// warten und seinen Exit-Code liefern; 1, wenn er gar nicht lief.
fn fallback_inno() -> u32 {
    passthrough(&[])
}

/// Motor mit `args` starten (still bei der Weitergabe, ohne Argumente als Assistent), warten,
/// aufräumen. Exit-Code des Motors, oder 1, wenn er gar nicht lief (kein Motor eingebettet,
/// Auspacken oder Start gescheitert). Keine Meldung, kein eigenes Fenster.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passthrough_wins() {
        for has_engine in [true, false] {
            for gui in [Ok(()), Err("wgpu".to_string())] {
                assert_eq!(decide(true, has_engine, gui), Action::Passthrough);
            }
        }
    }

    #[test]
    fn missing_engine_never_opens_gui() {
        assert_eq!(decide(false, false, Ok(())), Action::NoEngine);
        assert_eq!(
            decide(false, false, Err("wgpu".to_string())),
            Action::NoEngine
        );
    }

    #[test]
    fn gui_failure_falls_back_to_inno() {
        assert_eq!(
            decide(false, true, Err("kein Adapter".to_string())),
            Action::FallbackInno
        );
        assert_eq!(decide(false, true, Ok(())), Action::Gui);
    }
}
