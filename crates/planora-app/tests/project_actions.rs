use planora_app::project_actions::*;
use planora_app::workspace::*;
use planora_project::config::Config;
use planora_project::fsutil::new_id;
use planora_project::library::Library;

fn ws() -> Workspace {
    let root = std::env::temp_dir().join(format!("planora-pp-{}", new_id()));
    Workspace::new(Some(Library::open(&root).unwrap()), Config::default())
}

fn notice(w: &Workspace) -> String {
    match &w.prompt {
        Some(Prompt::Notice(m)) => m.clone(),
        other => panic!("Hinweis erwartet, war {other:?}"),
    }
}

#[test]
fn a_panel_action_with_a_bad_name_shows_a_notice_and_changes_nothing() {
    let mut w = ws();
    let mut st = ProjectsUi::default();
    assert!(create_project(&mut w, &mut st, "..").is_none());
    assert!(notice(&w).contains("Name"));
    w.prompt = None;
    let p = create_project(&mut w, &mut st, "P").unwrap();
    assert!(create_project(&mut w, &mut st, "P").is_none(), "doppelt");
    assert!(create_folder(&mut w, &mut st, &vec![], "X").is_none());
    assert_eq!(w.library.as_ref().unwrap().tree().unwrap().len(), 1);
    let _ = p;
}

#[test]
fn opening_a_document_creates_a_tab_and_a_locked_one_asks_to_take_over() {
    let mut w = ws();
    let mut st = ProjectsUi::default();
    let p = create_project(&mut w, &mut st, "P").unwrap();
    let doc = w.save_as(&p, "Teil", "").unwrap();
    w.close_tab(0);
    open_node(&mut w, &doc);
    assert_eq!(w.tabs.len(), 1);
    assert_eq!(w.active_tab().title(), "Teil");
    // zweites Programm
    let mut other = Workspace::new(
        Some(Library::open(w.library.as_ref().unwrap().root().path()).unwrap()),
        Config::default(),
    );
    open_node(&mut other, &doc);
    assert!(
        matches!(other.prompt, Some(Prompt::TakeOver { .. })),
        "{:?}",
        other.prompt
    );
    assert_eq!(other.tabs.len(), 1);
}

#[test]
fn renaming_moving_and_deleting_documents_in_open_tabs_is_refused() {
    let mut w = ws();
    let mut st = ProjectsUi::default();
    let p = create_project(&mut w, &mut st, "P").unwrap();
    let f = create_folder(&mut w, &mut st, &p, "F").unwrap();
    let doc = w.save_as(&p, "Teil", "").unwrap();
    assert!(rename_node(&mut w, &mut st, &doc, "Neu").is_none());
    assert!(notice(&w).contains("schließen"));
    w.prompt = None;
    assert!(move_node(&mut w, &mut st, &doc, &f).is_none());
    w.prompt = None;
    assert!(
        !delete_node(&mut w, &mut st, &p),
        "Projekt enthält ein offenes Dokument"
    );
    w.prompt = None;
    w.close_tab(0);
    let moved = move_node(&mut w, &mut st, &doc, &f).unwrap();
    assert_eq!(moved.len(), 3);
    let renamed = rename_node(&mut w, &mut st, &moved, "Neu").unwrap();
    assert_eq!(renamed.last().unwrap(), "Neu.pcad");
    assert!(delete_node(&mut w, &mut st, &renamed));
    assert!(
        w.library.as_ref().unwrap().tree().unwrap()[0].children[0]
            .children
            .is_empty()
    );
}

#[test]
fn notes_are_not_lost_when_an_autosave_is_offered() {
    use planora_project::docstore::LockMode;
    use std::time::{Duration, Instant};
    let mut w = ws();
    let mut st = ProjectsUi::default();
    let p = create_project(&mut w, &mut st, "P").unwrap();
    let doc = w.save_as(&p, "Teil", "").unwrap();
    // „Absturz“ mit Autosicherung
    w.autosave_interval = Duration::ZERO;
    w.active_tab_mut()
        .state
        .create_box(planora_math::DVec3::splat(5.0))
        .unwrap();
    w.autosave_tick(Instant::now());
    w.close_tab(w.tab_for_path(&doc).unwrap());
    // verwaiste Versionsdatei
    let dir = w.library.as_ref().unwrap().document_dir(&doc).unwrap();
    std::fs::write(dir.join("versionen").join("0099.json"), "{}").unwrap();
    open_node(&mut w, &doc);
    assert!(matches!(w.prompt, Some(Prompt::AutosaveOffer { .. })));
    let msg = w.active_tab().state.message.clone().unwrap_or_default();
    assert!(
        msg.contains("Reste eines abgebrochenen Speicherns"),
        "{msg}"
    );
    let _ = LockMode::Acquire;
}
