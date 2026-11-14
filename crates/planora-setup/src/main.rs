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

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// Was die Hülle beim Start tut.
#[derive(Debug, PartialEq, Eq)]
enum Action {
    /// Stille Weitergabe an den Motor (irgendein `/`-Argument).
    Passthrough,
    /// Graphit-Oberfläche.
    Gui,
    /// Die Oberfläche ließ sich nicht öffnen (etwa ohne wgpu-Grafik): normaler Inno-Assistent.
    FallbackInno,
    /// Die Oberfläche war schon offen und ist dann gescheitert: Ende mit Code 1, ohne den
    /// Inno-Assistenten (sonst liefe womöglich eine zweite Installation).
    GuiFailed,
    /// Kein Motor eingebettet (Entwicklungs-Build): nur eine Meldung.
    NoEngine,
}

/// Entscheidet, was die Hülle tut. Die Weitergabe hat Vorrang vor allem; ohne Motor öffnet sich
/// nie die Oberfläche; scheitert die Oberfläche (`gui`, auch nach einer Panik), übernimmt der
/// Inno-Assistent, aber nur, wenn sie nie zu sehen war (`gui_shown`).
fn decide(passthrough: bool, has_engine: bool, gui: Result<(), String>, gui_shown: bool) -> Action {
    if passthrough {
        Action::Passthrough
    } else if !has_engine {
        Action::NoEngine
    } else if gui.is_err() {
        if gui_shown {
            Action::GuiFailed
        } else {
            Action::FallbackInno
        }
    } else {
        Action::Gui
    }
}

fn main() {
    let args = args::raw_args();
    let passthrough_args = args::is_passthrough(&args);
    let has_engine = engine::engine_bytes().is_some();
    // Vor dem Öffnen der Oberfläche zählt deren Ergebnis noch nicht (Ok = noch nicht versucht).
    match decide(passthrough_args, has_engine, Ok(()), false) {
        // Ohne Motor endet die Weitergabe still mit 1, nie mit einer Meldung.
        Action::Passthrough => std::process::exit(passthrough(&args) as i32),
        Action::NoEngine => {
            set_language();
            show_no_engine();
            std::process::exit(1);
        }
        Action::Gui | Action::FallbackInno | Action::GuiFailed => {}
    }
    set_language();
    // Wird gesetzt, sobald das Fenster steht (siehe ui::run).
    let shown = Arc::new(AtomicBool::new(false));
    let wizard = wizard::Wizard::new(existing::detect());
    let run_shown = Arc::clone(&shown);
    // Auch eine Panik (etwa in wgpu) führt zum Rückfall, solange noch kein Fenster zu sehen war.
    let gui = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
        ui::run(wizard, run_shown)
    })) {
        Ok(result) => result.map_err(|err| err.to_string()),
        Err(_) => Err("gui panicked".to_string()),
    };
    match decide(false, has_engine, gui, shown.load(Ordering::SeqCst)) {
        Action::FallbackInno => std::process::exit(fallback_inno() as i32),
        Action::GuiFailed => std::process::exit(1),
        Action::Gui | Action::Passthrough | Action::NoEngine => {}
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
    let Ok(mut unpacked) = engine::Unpacked::unpack() else {
        return 1;
    };
    let code = match unpacked.spawn(args, false) {
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
                for shown in [true, false] {
                    assert_eq!(
                        decide(true, has_engine, gui.clone(), shown),
                        Action::Passthrough
                    );
                }
            }
        }
    }

    #[test]
    fn missing_engine_never_opens_gui() {
        assert_eq!(decide(false, false, Ok(()), false), Action::NoEngine);
        assert_eq!(
            decide(false, false, Err("wgpu".to_string()), false),
            Action::NoEngine
        );
    }

    #[test]
    fn gui_failure_falls_back_to_inno() {
        assert_eq!(
            decide(false, true, Err("kein Adapter".to_string()), false),
            Action::FallbackInno
        );
        assert_eq!(decide(false, true, Ok(()), false), Action::Gui);
        assert_eq!(decide(false, true, Ok(()), true), Action::Gui);
    }

    #[test]
    fn error_after_gui_shown_does_not_fall_back() {
        assert_eq!(
            decide(false, true, Err("wgpu verloren".to_string()), true),
            Action::GuiFailed
        );
    }

    /// Eine Panik vor dem ersten Fenster wird wie ein Fehler behandelt: Rückfall auf Inno.
    #[test]
    fn panic_before_gui_falls_back() {
        let shown = AtomicBool::new(false);
        let gui = match std::panic::catch_unwind(|| -> Result<(), String> {
            panic!("wgpu-Panik");
        }) {
            Ok(result) => result,
            Err(_) => Err("gui panicked".to_string()),
        };
        assert_eq!(
            decide(false, true, gui, shown.load(Ordering::SeqCst)),
            Action::FallbackInno
        );
    }
}
