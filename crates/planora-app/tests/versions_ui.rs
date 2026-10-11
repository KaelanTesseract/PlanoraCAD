use planora_app::versions_panel::*;
use planora_app::workspace::*;
use planora_app::{fonts, testing::collect_texts};
use planora_math::DVec3;
use planora_project::config::Config;
use planora_project::fsutil::new_id;
use planora_project::library::Library;

fn ctx() -> egui::Context {
    let c = egui::Context::default();
    c.set_fonts(fonts::definitions());
    c
}

fn ws_with_doc(versions: usize) -> Workspace {
    let root = std::env::temp_dir().join(format!("planora-vp-{}", new_id()));
    let mut w = Workspace::new(Some(Library::open(&root).unwrap()), Config::default());
    let p = w.library.as_ref().unwrap().create_project("P").unwrap();
    w.save_as(&p, "Teil", "Anfang").unwrap();
    for i in 1..versions {
        w.active_tab_mut()
            .state
            .create_box(DVec3::splat(2.0 + i as f64))
            .unwrap();
        w.save_active(&format!("Stand {}", i + 1), None).unwrap();
    }
    w
}

fn frame(c: &egui::Context, w: &mut Workspace, st: &mut VersionsUi) -> String {
    let out = c.run_ui(egui::RawInput::default(), |ui| show(ui, w, st));
    let t = collect_texts(&out);
    out.drop_without_applying_deltas();
    t
}

#[test]
fn an_unsaved_document_says_so() {
    let c = ctx();
    let mut w = Workspace::new(None, Config::default());
    let t = frame(&c, &mut w, &mut VersionsUi::default());
    assert!(t.contains("noch nicht gespeichert"), "{t}");
}

#[test]
fn the_list_shows_versions_comments_the_current_one_and_pending_changes() {
    let c = ctx();
    let mut w = ws_with_doc(3);
    let t = frame(&c, &mut w, &mut VersionsUi::default());
    for must in ["v1", "v2", "v3", "Haupt", "Anfang", "Stand 3"] {
        assert!(t.contains(must), "„{must}“ fehlt:\n{t}");
    }
    assert!(!t.contains("Änderungen"), "{t}");
    w.active_tab_mut()
        .state
        .create_box(DVec3::splat(9.0))
        .unwrap();
    let t = frame(&c, &mut w, &mut VersionsUi::default());
    assert!(t.contains("Änderungen"), "{t}");
}

#[test]
fn rows_are_newest_first_and_indented_by_branch() {
    let mut w = ws_with_doc(2);
    let path = w.active_tab().path.clone().unwrap();
    open_version(&mut w, 1);
    branch_from(&mut w, 1);
    // jetzt eine Ansicht von v1, bearbeitbar? Nein: das Dokument ist im anderen Tab beschreibbar offen.
    assert!(matches!(w.prompt, Some(Prompt::Notice(_))));
    w.prompt = None;
    let i = w.tab_for_path(&path).unwrap();
    w.activate(i);
    let info = w.active_tab().version_info().unwrap();
    let rows = rows(&info);
    assert_eq!(rows.iter().map(|r| r.number).collect::<Vec<_>>(), [2, 1]);
    assert!(rows.iter().all(|r| r.depth == 0));
}

#[test]
fn restore_comment_and_delete_work_on_the_writable_tab() {
    let mut w = ws_with_doc(3);
    open_version(&mut w, 1);
    restore(&mut w, 1);
    assert!(w.prompt.is_none(), "{:?}", w.prompt);
    let t = w.tabs.iter().find(|t| !t.read_only).unwrap();
    assert_eq!(t.version_info().unwrap().versions.len(), 4);
    assert_eq!(
        t.state.bodies.len(),
        0,
        "Stand von v1 hat noch keinen Körper"
    );
    set_comment(&mut w, 4, "neu benannt");
    assert_eq!(
        w.tabs
            .iter()
            .find(|t| !t.read_only)
            .unwrap()
            .version_info()
            .unwrap()
            .version(4)
            .unwrap()
            .comment,
        "neu benannt"
    );
    delete_version(&mut w, 4);
    assert_eq!(
        w.tabs
            .iter()
            .find(|t| !t.read_only)
            .unwrap()
            .version_info()
            .unwrap()
            .versions
            .len(),
        3
    );
    delete_version(&mut w, 1);
    assert!(matches!(w.prompt, Some(Prompt::Notice(_))), "v1 hat Kinder");
}

#[test]
fn branching_from_a_closed_document_makes_an_editable_tab_and_saving_asks_for_a_branch() {
    let mut w = ws_with_doc(2);
    let path = w.active_tab().path.clone().unwrap();
    w.close_tab(w.tab_for_path(&path).unwrap());
    w.open_version(&path, 1).unwrap();
    branch_from(&mut w, 1);
    assert!(w.prompt.is_none(), "{:?}", w.prompt);
    assert!(!w.active_tab().read_only);
    w.active_tab_mut()
        .state
        .create_box(DVec3::splat(7.0))
        .unwrap();
    assert_eq!(w.save_active("", None).unwrap(), SaveOutcome::NeedsBranch);
    assert_eq!(
        w.save_active("", Some("Variante")).unwrap(),
        SaveOutcome::Saved(3)
    );
    delete_branch(&mut w, "Haupt");
    assert!(
        matches!(w.prompt, Some(Prompt::Notice(_))),
        "v1 hat Kinder in Variante"
    );
    w.prompt = None;
    switch_branch(&mut w, "Haupt");
    assert!(w.prompt.is_none(), "{:?}", w.prompt);
}

#[test]
fn a_view_shows_the_banner_and_the_tree_without_a_writable_tab() {
    let c = ctx();
    let mut w = ws_with_doc(2);
    let path = w.active_tab().path.clone().unwrap();
    w.close_tab(w.tab_for_path(&path).unwrap());
    w.open_version(&path, 1).unwrap();
    let mut st = VersionsUi::default();
    let t = frame(&c, &mut w, &mut st);
    for must in [
        "Du siehst eine alte Version.",
        "Wiederherstellen",
        "Abzweigen",
        "v2",
    ] {
        assert!(t.contains(must), "„{must}“ fehlt:\n{t}");
    }
    assert!(!t.contains("Änderungen"), "{t}");
    // Ohne beschreibbaren Tab ändert sich nichts: Hinweis statt Absturz.
    set_comment(&mut w, 1, "x");
    assert!(matches!(w.prompt, Some(Prompt::Notice(_))));
}

#[test]
fn switching_the_branch_opens_its_tip_as_a_view() {
    let mut w = ws_with_doc(2);
    switch_branch(&mut w, "Haupt");
    assert!(w.prompt.is_none(), "{:?}", w.prompt);
    assert!(w.active_tab().read_only);
    assert_eq!(w.active_tab().based_on, Some(2));
    switch_branch(&mut w, "Gibtsnicht");
    assert!(matches!(w.prompt, Some(Prompt::Notice(_))));
}
