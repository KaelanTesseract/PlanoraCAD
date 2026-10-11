//! Schließen des Fensters über `app::handle_close_request` (läuft in `App::logic`, auch bei
//! minimiertem Fenster).

use egui::{ViewportCommand, ViewportEvent, ViewportId, ViewportInfo};
use planora_app::app::handle_close_request;
use planora_app::workspace::*;
use planora_math::DVec3;
use planora_project::config::Config;
use planora_project::fsutil::new_id;
use planora_project::library::Library;

fn ws() -> Workspace {
    let root = std::env::temp_dir().join(format!("planora-close-{}", new_id()));
    Workspace::new(Some(Library::open(&root).unwrap()), Config::default())
}

fn input(close: bool, minimized: bool) -> egui::RawInput {
    let mut raw = egui::RawInput::default();
    raw.viewports.insert(
        ViewportId::ROOT,
        ViewportInfo {
            events: if close {
                vec![ViewportEvent::Close]
            } else {
                vec![]
            },
            minimized: Some(minimized),
            ..Default::default()
        },
    );
    raw
}

/// Ein sichtbares Bild: wie eframe `logic` innerhalb von `run_ui` aufruft.
fn visible_frame(
    c: &egui::Context,
    close: bool,
    w: &mut Workspace,
    sent: &mut bool,
) -> Vec<ViewportCommand> {
    let out = c.run_ui(input(close, false), |ui| {
        handle_close_request(ui.ctx(), w, sent)
    });
    let cmds = out
        .viewport_output
        .get(&ViewportId::ROOT)
        .map(|v| v.commands.clone())
        .unwrap_or_default();
    out.drop_without_applying_deltas();
    cmds
}

/// Minimiertes Fenster: eframe ruft nur `logic` über `run_logic` auf, ohne Oberfläche.
fn minimized_frame(
    c: &egui::Context,
    close: bool,
    w: &mut Workspace,
    sent: &mut bool,
) -> Vec<ViewportCommand> {
    let out = c.run_logic(&input(close, true), |ctx| {
        handle_close_request(ctx, w, sent)
    });
    out.viewport_commands
        .get(&ViewportId::ROOT)
        .cloned()
        .unwrap_or_default()
}

fn dirty(w: &mut Workspace) {
    w.active_tab_mut()
        .state
        .create_box(DVec3::splat(4.0))
        .unwrap();
}

#[test]
fn closing_with_a_changed_tab_is_cancelled_and_asks() {
    let c = egui::Context::default();
    let mut w = ws();
    dirty(&mut w);
    let mut sent = false;
    let cmds = visible_frame(&c, true, &mut w, &mut sent);
    assert!(cmds.contains(&ViewportCommand::CancelClose), "{cmds:?}");
    assert!(matches!(
        w.prompt,
        Some(Prompt::SaveChanges {
            then: After::Quit,
            ..
        })
    ));
    assert!(!w.quit);
    assert!(!cmds.contains(&ViewportCommand::Close));
}

#[test]
fn closing_a_minimized_window_with_changes_is_cancelled_and_shows_the_window() {
    let c = egui::Context::default();
    let mut w = ws();
    dirty(&mut w);
    let mut sent = false;
    let cmds = minimized_frame(&c, true, &mut w, &mut sent);
    assert!(cmds.contains(&ViewportCommand::CancelClose), "{cmds:?}");
    assert!(
        cmds.contains(&ViewportCommand::Minimized(false)),
        "{cmds:?}"
    );
    assert!(cmds.contains(&ViewportCommand::Focus), "{cmds:?}");
    assert!(matches!(w.prompt, Some(Prompt::SaveChanges { .. })));
}

#[test]
fn closing_without_changes_is_not_cancelled() {
    let c = egui::Context::default();
    let mut w = ws();
    let mut sent = false;
    let cmds = visible_frame(&c, true, &mut w, &mut sent);
    assert!(!cmds.contains(&ViewportCommand::CancelClose), "{cmds:?}");
    assert!(w.quit);
    assert!(w.prompt.is_none());
    // auch minimiert
    let mut w = ws();
    let cmds = minimized_frame(&c, true, &mut w, &mut sent);
    assert!(!cmds.contains(&ViewportCommand::CancelClose), "{cmds:?}");
    assert!(w.quit);
}

#[test]
fn closing_with_an_open_autosave_offer_is_cancelled() {
    let c = egui::Context::default();
    let mut w = ws();
    let id = w.active_tab().id;
    w.prompt = Some(Prompt::AutosaveOffer { tab: id });
    let mut sent = false;
    let cmds = visible_frame(&c, true, &mut w, &mut sent);
    assert!(cmds.contains(&ViewportCommand::CancelClose), "{cmds:?}");
    assert!(matches!(w.prompt, Some(Prompt::AutosaveOffer { .. })));
    assert!(!w.quit);
}

#[test]
fn after_discarding_the_last_change_the_app_closes_exactly_once() {
    let c = egui::Context::default();
    let mut w = ws();
    dirty(&mut w);
    let mut sent = false;
    visible_frame(&c, true, &mut w, &mut sent);
    w.discard_and_continue();
    assert!(w.quit);
    let cmds = visible_frame(&c, false, &mut w, &mut sent);
    assert_eq!(
        cmds.iter()
            .filter(|c| **c == ViewportCommand::Close)
            .count(),
        1,
        "{cmds:?}"
    );
    let cmds = visible_frame(&c, false, &mut w, &mut sent);
    assert!(!cmds.contains(&ViewportCommand::Close), "{cmds:?}");
    // die daraus folgende Schließanfrage wird nicht mehr abgebrochen
    let cmds = visible_frame(&c, true, &mut w, &mut sent);
    assert!(!cmds.contains(&ViewportCommand::CancelClose), "{cmds:?}");
}
