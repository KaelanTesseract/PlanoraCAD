use planora_app::workspace::*;
use planora_math::DVec3;
use planora_project::config::Config;
use planora_project::docstore::LockMode;
use planora_project::fsutil::new_id;
use planora_project::library::{Library, NodePath};
use std::time::{Duration, Instant};

fn root() -> std::path::PathBuf {
    std::env::temp_dir().join(format!("planora-ws-{}", new_id()))
}

fn ws() -> Workspace {
    Workspace::new(Some(Library::open(&root()).unwrap()), Config::default())
}

fn add_box(ws: &mut Workspace) {
    ws.active_tab_mut()
        .state
        .create_box(DVec3::splat(5.0))
        .unwrap();
}

/// Speichert den aktiven Tab als Dokument `name` im Projekt „P“ mit `boxes` Quadern.
fn saved_document(w: &mut Workspace, name: &str, boxes: usize) -> NodePath {
    let p = w
        .library
        .as_ref()
        .unwrap()
        .create_project("P")
        .unwrap_or_else(|_| vec!["P".to_string()]);
    for _ in 0..boxes {
        add_box(w);
    }
    w.save_as(&p, name, "").unwrap()
}

#[test]
fn a_new_workspace_has_one_clean_untitled_tab() {
    let w = ws();
    assert_eq!(w.tabs.len(), 1);
    assert_eq!(w.active_tab().title(), "Unbenannt 1");
    assert!(!w.active_tab().is_dirty());
}

#[test]
fn tabs_have_independent_states_and_unique_scene_versions() {
    let mut w = ws();
    add_box(&mut w);
    let second = w.new_tab();
    assert_eq!(w.active, second);
    assert_eq!(w.active_tab().title(), "Unbenannt 2");
    assert_eq!(w.active_tab().state.bodies.len(), 0);
    assert_ne!(w.tabs[0].state.scene_version, w.tabs[1].state.scene_version);
    w.activate(0);
    assert_eq!(w.active_tab().state.bodies.len(), 1);
    w.activate(99);
    assert_eq!(w.active, 0);
    w.set_dark(false);
    assert!(w.tabs.iter().all(|t| !t.state.dark));
}

#[test]
fn dirty_follows_edits() {
    let mut w = ws();
    assert!(!w.active_tab().is_dirty());
    add_box(&mut w);
    assert!(w.active_tab().is_dirty());
}

#[test]
fn closing_a_clean_tab_works_and_the_last_tab_is_replaced_by_a_fresh_one() {
    let mut w = ws();
    assert!(w.request_close(0));
    assert_eq!(w.tabs.len(), 1);
    assert!(w.active_tab().title().starts_with("Unbenannt"));
    assert!(!w.active_tab().is_dirty());
}

#[test]
fn closing_a_dirty_tab_asks_first_and_discard_closes_it() {
    let mut w = ws();
    add_box(&mut w);
    w.new_tab();
    assert!(!w.request_close(0));
    assert!(matches!(
        w.prompt,
        Some(Prompt::SaveChanges {
            then: After::CloseTab,
            ..
        })
    ));
    assert_eq!(w.tabs.len(), 2);
    w.discard_and_continue();
    assert!(w.prompt.is_none());
    assert_eq!(w.tabs.len(), 1);
    assert!(w.tabs.iter().all(|t| t.state.bodies.is_empty()));
}

#[test]
fn quitting_asks_for_each_dirty_tab_in_turn() {
    let mut w = ws();
    add_box(&mut w);
    w.new_tab();
    add_box(&mut w);
    assert!(!w.request_quit());
    assert!(matches!(
        w.prompt,
        Some(Prompt::SaveChanges {
            then: After::Quit,
            ..
        })
    ));
    w.discard_and_continue();
    assert!(
        matches!(
            w.prompt,
            Some(Prompt::SaveChanges {
                then: After::Quit,
                ..
            })
        ),
        "zweiter Tab fragt"
    );
    w.discard_and_continue();
    assert!(w.quit && w.prompt.is_none());
    let mut clean = ws();
    assert!(clean.request_quit() && clean.quit);
}

#[test]
fn an_untitled_tab_needs_a_location_then_saves_and_is_clean() {
    let mut w = ws();
    add_box(&mut w);
    assert_eq!(w.save_active("", None).unwrap(), SaveOutcome::NeedsLocation);
    let project = w.library.as_ref().unwrap().create_project("P").unwrap();
    let path = w.save_as(&project, "Teil", "Erster Stand").unwrap();
    assert_eq!(path, vec!["P".to_string(), "Teil.pcad".to_string()]);
    assert_eq!(w.active_tab().title(), "Teil");
    assert!(!w.active_tab().is_dirty());
    assert_eq!(w.active_tab().based_on, Some(1));
    add_box(&mut w);
    assert!(w.active_tab().is_dirty());
    assert_eq!(
        w.save_active("zweiter Stand", None).unwrap(),
        SaveOutcome::Saved(2)
    );
    assert!(!w.active_tab().is_dirty());
    let info = w.active_tab().version_info().unwrap();
    assert_eq!(info.versions.len(), 2);
    assert_eq!(info.version(2).unwrap().comment, "zweiter Stand");
}

#[test]
fn saving_takes_an_open_sketch_along() {
    let mut w = ws();
    let project = w.library.as_ref().unwrap().create_project("P").unwrap();
    w.save_as(&project, "S", "").unwrap();
    {
        let s = &mut w.active_tab_mut().state;
        let id = s
            .add_sketch(planora_feature::SketchPlane::Standard(
                planora_app::state::StandardPlane::XY,
            ))
            .unwrap();
        s.open_sketch(id).unwrap();
    }
    assert!(
        w.active_tab().is_dirty(),
        "offene Skizze zählt als geändert"
    );
    assert_eq!(w.save_active("", None).unwrap(), SaveOutcome::Saved(2));
    assert!(w.active_tab().state.sketch_session.is_none());
    assert!(!w.active_tab().is_dirty());
}

#[test]
fn save_as_rejects_bad_places_and_names_without_changing_the_tab() {
    let mut w = ws();
    add_box(&mut w);
    assert!(w.save_as(&vec![], "X", "").is_err(), "kein Projekt");
    let p = w.library.as_ref().unwrap().create_project("P").unwrap();
    assert!(w.save_as(&p, "a/b", "").is_err());
    assert!(w.save_as(&p, "..", "").is_err());
    assert!(w.active_tab().store.is_none());
    w.save_as(&p, "Teil", "").unwrap();
    // zweites Dokument gleichen Namens
    w.new_tab();
    add_box(&mut w);
    assert!(w.save_as(&p, "Teil", "").is_err());
    assert!(w.active_tab().store.is_none());
}

#[test]
fn the_save_prompt_saves_closes_and_asks_for_missing_places() {
    let mut w = ws();
    let project = w.library.as_ref().unwrap().create_project("P").unwrap();
    w.save_as(&project, "T", "").unwrap();
    add_box(&mut w);
    assert!(!w.request_close(0));
    w.save_for_prompt();
    assert!(w.prompt.is_none());
    assert!(w.tabs.iter().all(|t| t.store.is_none() || !t.is_dirty()));
    // unbenannt: Prompt wechselt zu SaveAs
    let mut w2 = ws();
    add_box(&mut w2);
    w2.request_close(0);
    w2.save_for_prompt();
    assert!(matches!(w2.prompt, Some(Prompt::SaveAs { .. })));
}

#[test]
fn opening_a_document_loads_the_active_branch_and_reuses_an_open_tab() {
    let mut w = ws();
    let path = saved_document(&mut w, "Teil", 2);
    w.close_tab(0);
    assert_eq!(w.tabs.len(), 1);
    let r = w.open_document(&path, LockMode::Acquire).unwrap();
    assert_eq!(w.tabs.len(), 1, "der leere Tab wird ersetzt");
    assert_eq!(w.tabs[r.tab].state.bodies.len(), 2);
    assert!(!w.tabs[r.tab].is_dirty());
    let again = w.open_document(&path, LockMode::Acquire).unwrap();
    assert_eq!(again.tab, r.tab);
    assert_eq!(w.tabs.len(), 1);
}

#[test]
fn a_locked_document_reports_it_and_can_be_taken_over() {
    let mut w = ws();
    let path = saved_document(&mut w, "Teil", 1);
    // zweiter Arbeitsbereich (anderes Programm) auf demselben Ordner
    let mut other = Workspace::new(
        Some(Library::open(w.library.as_ref().unwrap().root().path()).unwrap()),
        Config::default(),
    );
    assert!(matches!(
        other.open_document(&path, LockMode::Acquire),
        Err(planora_project::ProjectError::Locked { .. })
    ));
    assert!(other.open_document(&path, LockMode::TakeOver).is_ok());
}

#[test]
fn old_versions_open_read_only_and_can_be_restored() {
    let mut w = ws();
    let path = saved_document(&mut w, "Teil", 1);
    add_box(&mut w);
    w.save_active("zwei", None).unwrap();
    add_box(&mut w);
    w.save_active("drei", None).unwrap();
    let v = w.open_version(&path, 1).unwrap();
    assert!(w.tabs[v].read_only);
    assert_eq!(w.tabs[v].title(), "Teil · v1");
    assert_eq!(w.tabs[v].state.bodies.len(), 1);
    assert!(!w.tabs[v].is_dirty());
    assert!(
        w.save_active("", None)
            .unwrap_err()
            .contains("schreibgeschützt")
    );
    // dieselbe Version nicht doppelt öffnen
    assert_eq!(w.open_version(&path, 1).unwrap(), v);
    // Wiederherstellen: neue Version 4 mit dem Stand von v1 im beschreibbaren Tab
    w.restore_version(v, 1).unwrap();
    let t = &w.tabs[w.tab_for_path(&path).unwrap()];
    assert_eq!(t.state.bodies.len(), 1);
    assert_eq!(t.based_on, Some(4));
    assert_eq!(t.version_info().unwrap().versions.len(), 4);
}

#[test]
fn restoring_is_refused_while_the_writable_tab_has_unsaved_changes() {
    let mut w = ws();
    let path = saved_document(&mut w, "Teil", 1);
    let v = w.open_version(&path, 1).unwrap();
    let writable = w.tab_for_path(&path).unwrap();
    w.activate(writable);
    add_box(&mut w);
    assert!(
        w.restore_version(v, 1)
            .unwrap_err()
            .contains("ungespeicherte")
    );
}

#[test]
fn branching_from_an_old_version_requires_a_branch_name_on_save() {
    let mut w = ws();
    let path = saved_document(&mut w, "Teil", 1);
    add_box(&mut w);
    w.save_active("zwei", None).unwrap();
    w.close_tab(w.tab_for_path(&path).unwrap());
    let v = w.open_version(&path, 1).unwrap();
    w.make_editable(v).unwrap();
    assert!(!w.tabs[v].read_only);
    w.activate(v);
    add_box(&mut w);
    assert_eq!(
        w.save_active("Variante", None).unwrap(),
        SaveOutcome::NeedsBranch
    );
    assert_eq!(
        w.save_active("Variante", Some("Variante A")).unwrap(),
        SaveOutcome::Saved(3)
    );
    let info = w.active_tab().version_info().unwrap();
    assert_eq!(info.version(3).unwrap().branch, "Variante A");
    assert!(!w.active_tab().is_dirty());
}

#[test]
fn making_a_view_editable_fails_while_another_tab_holds_the_document() {
    let mut w = ws();
    let path = saved_document(&mut w, "Teil", 1);
    let v = w.open_version(&path, 1).unwrap();
    assert!(w.make_editable(v).is_err());
    assert!(w.tabs[v].read_only);
}

#[test]
fn an_autosave_is_offered_after_a_crash_and_can_be_applied_or_discarded() {
    let mut w = ws();
    let path = saved_document(&mut w, "Teil", 1);
    w.autosave_interval = Duration::ZERO;
    add_box(&mut w);
    w.autosave_tick(Instant::now());
    // „Absturz“: Tab ohne Speichern schließen
    w.close_tab(w.tab_for_path(&path).unwrap());
    let r = w.open_document(&path, LockMode::Acquire).unwrap();
    assert!(r.autosave_offered);
    let id = w.tabs[r.tab].id;
    assert_eq!(
        w.tabs[r.tab].state.bodies.len(),
        1,
        "zuerst der gespeicherte Stand"
    );
    w.apply_autosave(id);
    assert_eq!(w.tabs[r.tab].state.bodies.len(), 2);
    assert!(
        w.tabs[r.tab].is_dirty(),
        "übernommene Sicherung gilt als ungespeichert"
    );
    w.close_tab(r.tab);
    let r2 = w.open_document(&path, LockMode::Acquire).unwrap();
    assert!(r2.autosave_offered);
    let id2 = w.tabs[r2.tab].id;
    w.discard_autosave(id2);
    w.close_tab(r2.tab);
    assert!(
        !w.open_document(&path, LockMode::Acquire)
            .unwrap()
            .autosave_offered
    );
}

#[test]
fn saving_removes_the_autosave() {
    let mut w = ws();
    let path = saved_document(&mut w, "Teil", 1);
    w.autosave_interval = Duration::ZERO;
    add_box(&mut w);
    w.autosave_tick(Instant::now());
    let dir = w.library.as_ref().unwrap().document_dir(&path).unwrap();
    assert!(dir.join(".autosave.json").exists());
    w.save_active("", None).unwrap();
    assert!(!dir.join(".autosave.json").exists());
}

#[test]
fn a_damaged_document_does_not_open_and_changes_nothing() {
    let mut w = ws();
    let path = saved_document(&mut w, "Teil", 1);
    let dir = w.library.as_ref().unwrap().document_dir(&path).unwrap();
    w.close_tab(0);
    std::fs::write(dir.join("dokument.json"), "{").unwrap();
    let before = w.tabs.len();
    assert!(w.open_document(&path, LockMode::Acquire).is_err());
    assert_eq!(w.tabs.len(), before);
}

#[test]
fn choosing_a_root_creates_it_and_remembers_it() {
    let mut w = Workspace::new(None, Config::default());
    assert!(w.library.is_none());
    let r = root();
    w.choose_root(&r).unwrap();
    assert!(w.library.is_some());
    assert_eq!(w.config.root.as_deref(), Some(r.as_path()));
}

// ---- Nachbesserung 1 ---------------------------------------------------------------

/// Schreibt für das Dokument `name` (aktiver Tab) eine Autosicherung mit einem Quader mehr
/// und schließt den Tab („Absturz“).
fn crash_with_autosave(w: &mut Workspace, name: &str) -> NodePath {
    let path = saved_document(w, name, 1);
    w.autosave_interval = Duration::ZERO;
    add_box(w);
    w.autosave_tick(Instant::now());
    w.close_tab(w.tab_for_path(&path).unwrap());
    path
}

fn autosave_bytes(w: &Workspace, path: &NodePath) -> Option<Vec<u8>> {
    let dir = w.library.as_ref().unwrap().document_dir(path).unwrap();
    std::fs::read(dir.join(".autosave.json")).ok()
}

#[test]
fn an_unreadable_autosave_is_kept_reported_and_never_overwritten() {
    let mut w = ws();
    let path = saved_document(&mut w, "Teil", 1);
    let dir = w.library.as_ref().unwrap().document_dir(&path).unwrap();
    w.close_tab(0);
    let foreign = r#"{"format":99,"basiert_auf":1,"zeit":"9999-01-01T00:00:00Z","inhalt":""}"#;
    std::fs::write(dir.join(".autosave.json"), foreign).unwrap();
    let r = w.open_document(&path, LockMode::Acquire).unwrap();
    assert!(!r.autosave_offered);
    assert!(
        r.notes.iter().any(|n| n.contains("Autosicherung")),
        "{:?}",
        r.notes
    );
    assert!(w.prompt.is_none());
    assert_eq!(autosave_bytes(&w, &path).unwrap(), foreign.as_bytes());
    // weder Autosicherung noch Speichern überschreiben oder löschen die Datei
    w.autosave_interval = Duration::ZERO;
    add_box(&mut w);
    w.autosave_tick(Instant::now());
    assert_eq!(autosave_bytes(&w, &path).unwrap(), foreign.as_bytes());
    w.save_active("", None).unwrap();
    assert_eq!(autosave_bytes(&w, &path).unwrap(), foreign.as_bytes());
}

#[test]
fn two_autosave_offers_in_a_row_are_both_reachable_and_not_overwritten() {
    let mut w = ws();
    let a = crash_with_autosave(&mut w, "A");
    let b = crash_with_autosave(&mut w, "B");
    let a_before = autosave_bytes(&w, &a).unwrap();
    let ra = w.open_document(&a, LockMode::Acquire).unwrap();
    assert!(ra.autosave_offered);
    let id_a = w.tabs[ra.tab].id;
    assert!(matches!(w.prompt, Some(Prompt::AutosaveOffer { tab }) if tab == id_a));
    // Bearbeiten vor der Antwort überschreibt die Absturzsicherung nicht
    add_box(&mut w);
    w.autosave_tick(Instant::now());
    assert_eq!(autosave_bytes(&w, &a).unwrap(), a_before);
    let rb = w.open_document(&b, LockMode::Acquire).unwrap();
    assert!(rb.autosave_offered);
    let id_b = w.tabs[rb.tab].id;
    assert!(
        matches!(w.prompt, Some(Prompt::AutosaveOffer { tab }) if tab == id_a),
        "erstes Angebot bleibt"
    );
    // Schließen und Beenden warten auf die Antwort
    assert!(!w.request_quit());
    assert!(!w.quit);
    assert!(!w.request_close(rb.tab));
    assert_eq!(w.tabs.len(), 2);
    assert!(matches!(w.prompt, Some(Prompt::AutosaveOffer { tab }) if tab == id_a));
    // ein anderer Prompt verdrängt das Angebot nur vorübergehend
    w.prompt = Some(Prompt::Notice("x".into()));
    w.refresh_prompts();
    assert!(matches!(w.prompt, Some(Prompt::Notice(_))));
    w.prompt = None;
    w.refresh_prompts();
    assert!(matches!(w.prompt, Some(Prompt::AutosaveOffer { tab }) if tab == id_a));
    w.apply_autosave(id_a);
    assert!(
        matches!(w.prompt, Some(Prompt::AutosaveOffer { tab }) if tab == id_b),
        "danach das zweite"
    );
    w.discard_autosave(id_b);
    assert!(w.prompt.is_none());
    assert!(autosave_bytes(&w, &b).is_none());
    assert!(autosave_bytes(&w, &a).is_some());
}

#[test]
fn a_failed_autosave_is_not_retried_before_the_interval() {
    let mut w = ws();
    let path = saved_document(&mut w, "Teil", 1);
    let dir = w.library.as_ref().unwrap().document_dir(&path).unwrap();
    // ein Ordner an der Stelle der Datei: das Schreiben schlägt fehl
    std::fs::create_dir(dir.join(".autosave.json")).unwrap();
    let base = Instant::now();
    add_box(&mut w);
    w.autosave_tick(base + AUTOSAVE_INTERVAL + Duration::from_secs(1));
    std::fs::remove_dir(dir.join(".autosave.json")).unwrap();
    w.autosave_tick(base + AUTOSAVE_INTERVAL + Duration::from_secs(2));
    assert!(
        !dir.join(".autosave.json").exists(),
        "kein neuer Versuch in jedem Bild"
    );
    w.autosave_tick(base + AUTOSAVE_INTERVAL * 2 + Duration::from_secs(2));
    assert!(dir.join(".autosave.json").is_file());
}

#[test]
fn a_failed_restore_turns_a_freshly_editable_view_back_into_a_view() {
    let mut w = ws();
    let path = saved_document(&mut w, "Teil", 1);
    w.close_tab(w.tab_for_path(&path).unwrap());
    let v = w.open_version(&path, 1).unwrap();
    assert!(w.restore_version(v, 99).is_err());
    assert!(w.tabs[v].read_only);
    assert!(w.tabs[v].store.is_none() && w.tabs[v].source_dir.is_some());
    assert!(w.tab_for_path(&path).is_none());
    // die Sperre ist wieder frei
    assert!(w.open_document(&path, LockMode::Acquire).is_ok());
}

#[test]
fn save_as_onto_a_taken_name_leaves_an_open_sketch_open() {
    let mut w = ws();
    let p = w.library.as_ref().unwrap().create_project("P").unwrap();
    w.save_as(&p, "Teil", "").unwrap();
    w.new_tab();
    {
        let s = &mut w.active_tab_mut().state;
        let id = s
            .add_sketch(planora_feature::SketchPlane::Standard(
                planora_app::state::StandardPlane::XY,
            ))
            .unwrap();
        s.open_sketch(id).unwrap();
    }
    assert!(w.save_as(&p, "Teil", "").is_err());
    assert!(w.active_tab().state.sketch_session.is_some());
    assert!(w.active_tab().store.is_none());
}
