//! Die letzten Schritte (Namen von Aktionen und Rückgängig-Schritten, nie Projektinhalt).

mod common;

use planora_app::commands;
use planora_app::diag::{self, Diag, STEPS, Step};
use planora_app::keymap::Action;
use planora_app::versions_panel;
use planora_app::workspace::Workspace;
use planora_math::DVec3;
use planora_project::config::Config;
use planora_project::library::Library;

fn ws() -> Workspace {
    let root = common::temp_root("planora-steps");
    Workspace::new(Some(Library::open(&root).unwrap()), Config::default())
}

fn has(label: &str) -> bool {
    diag::recent_steps().iter().any(|s| s.label.contains(label))
}

#[test]
fn steps_keep_the_last_50() {
    let mut d = Diag::in_memory();
    assert_eq!(STEPS, 50);
    for i in 0..60 {
        d.step(&format!("Schritt {i}"));
    }
    let all = d.recent_steps();
    assert_eq!(all.len(), 50);
    assert_eq!(all[0].label, "Schritt 10");
    assert_eq!(all[49].label, "Schritt 59");
}

#[test]
fn repeated_steps_are_counted_not_repeated() {
    let mut d = Diag::in_memory();
    d.step("Rückgängig");
    d.step("Rückgängig");
    d.step("Rückgängig");
    let s = d.recent_steps();
    assert_eq!(s.len(), 1);
    assert_eq!(s[0].label, "Rückgängig ×3");
    d.step("Wiederholen");
    d.step("Rückgängig");
    let s = d.recent_steps();
    assert_eq!(s.len(), 3);
    assert_eq!(s[2].label, "Rückgängig");
}

#[test]
fn step_names_are_cleaned_like_log_lines() {
    let mut d = Diag::in_memory();
    d.step("Zeile\neins\r\nzwei");
    let s = d.recent_steps();
    assert!(!s[0].label.contains('\n') && !s[0].label.contains('\r'));
    let text = diag::steps_text(&s);
    assert_eq!(text.lines().count(), 1);
}

#[test]
fn run_action_records_the_action_label() {
    let mut w = ws();
    commands::run_action(&mut w, Action::New);
    let last = diag::recent_steps().into_iter().last().unwrap();
    assert!(
        has("Neues Dokument"),
        "letzter Schritt war {:?}",
        last.label
    );
}

#[test]
fn blocked_model_action_in_old_version_records_nothing() {
    let mut w = ws();
    let p = w.library.as_ref().unwrap().create_project("P").unwrap();
    w.save_as(&p, "Teil", "Anfang").unwrap();
    for _ in 0..2 {
        w.active_tab_mut()
            .state
            .create_box(DVec3::splat(3.0))
            .unwrap();
    }
    w.save_active("Stand 2", None).unwrap();
    versions_panel::open_version(&mut w, 1);
    assert!(w.active_tab().read_only);
    // Nur dieser Test löst „Skizze extrudieren“ aus; die Liste darf den Namen nie enthalten.
    commands::run_action(&mut w, Action::ExtrudeSketch);
    assert!(!has(Action::ExtrudeSketch.label()));
}

#[test]
fn undo_label_becomes_a_step() {
    let mut w = ws();
    w.active_tab_mut()
        .state
        .create_box(DVec3::splat(100.0))
        .unwrap();
    w.observe_history(false);
    let label = w.active_tab().history().undo_label().unwrap();
    assert!(has(label), "{label} fehlt in {:?}", diag::recent_steps());
}

#[test]
fn steps_text_format() {
    let steps = vec![
        Step {
            time: "14:03:22".into(),
            label: "Skizze extrudieren".into(),
        },
        Step {
            time: "14:03:25".into(),
            label: "Rückgängig ×3".into(),
        },
    ];
    assert_eq!(
        diag::steps_text(&steps),
        "14:03:22 Skizze extrudieren\n14:03:25 Rückgängig ×3"
    );
    assert_eq!(diag::steps_text(&[]), "");
}
