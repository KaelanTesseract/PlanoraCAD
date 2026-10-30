//! „Speichern“ wird erst ein Schritt (Letzte Schritte im Absturzbericht), wenn wirklich
//! gespeichert wurde; ein abgebrochenes „Speichern unter“ legt keinen ab. Eigene Testdatei,
//! weil die Schritte global sind.

mod common;

use planora_app::commands;
use planora_app::diag;
use planora_app::keymap::Action;
use planora_app::workspace::Workspace;
use planora_project::config::Config;
use planora_project::library::Library;

fn saves() -> usize {
    diag::recent_steps()
        .iter()
        .filter(|s| s.label.starts_with("Save"))
        .count()
}

#[test]
fn cancelled_save_as_records_no_step_and_real_save_does() {
    let root = common::temp_root("planora-speichern-schritt");
    let mut w = Workspace::new(Some(Library::open(&root).unwrap()), Config::default());
    // Neues Dokument: „Speichern“ fragt nach dem Ort (Speichern unter) …
    commands::run_action(&mut w, Action::Save);
    assert!(w.prompt.is_some(), "Speichern unter steht an");
    // … und wird abgebrochen.
    w.prompt = None;
    assert_eq!(saves(), 0, "{:?}", diag::recent_steps());

    // Wirklich gespeichert: jetzt ein Schritt.
    let p = w.library.as_ref().unwrap().create_project("P").unwrap();
    w.save_as(&p, "Teil", "").unwrap();
    assert_eq!(saves(), 1, "{:?}", diag::recent_steps());
    w.save_active("zwei", None).unwrap();
    let last = diag::recent_steps().into_iter().last().unwrap();
    assert!(last.label.starts_with("Save"), "{last:?}");
}
