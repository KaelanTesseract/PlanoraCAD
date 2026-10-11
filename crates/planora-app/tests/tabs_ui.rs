use planora_app::workspace::*;
use planora_app::{fonts, prompts, tabs, testing::collect_texts};
use planora_math::DVec3;
use planora_project::config::Config;
use planora_project::fsutil::new_id;
use planora_project::library::Library;

fn ctx() -> egui::Context {
    let c = egui::Context::default();
    c.set_fonts(fonts::definitions());
    c
}

fn ws() -> Workspace {
    let root = std::env::temp_dir().join(format!("planora-tabs-{}", new_id()));
    Workspace::new(Some(Library::open(&root).unwrap()), Config::default())
}

fn frame(c: &egui::Context, f: impl FnMut(&mut egui::Ui)) -> String {
    let out = c.run_ui(egui::RawInput::default(), f);
    let t = collect_texts(&out);
    out.drop_without_applying_deltas();
    t
}

#[test]
fn the_tab_bar_lists_tabs_and_marks_changes() {
    let c = ctx();
    let mut w = ws();
    let t = frame(&c, |ui| tabs::show(ui, &mut w));
    assert!(t.contains("Unbenannt 1"), "{t}");
    assert!(!t.contains('●'));
    w.active_tab_mut()
        .state
        .create_box(DVec3::splat(4.0))
        .unwrap();
    w.new_tab();
    let t = frame(&c, |ui| tabs::show(ui, &mut w));
    assert!(t.contains("Unbenannt 1 ●"), "{t}");
    assert!(t.contains("Unbenannt 2"), "{t}");
}

#[test]
fn a_read_only_tab_shows_its_version_in_the_title() {
    let c = ctx();
    let mut w = ws();
    let p = w.library.as_ref().unwrap().create_project("P").unwrap();
    w.save_as(&p, "Teil", "").unwrap();
    let path = w.active_tab().path.clone().unwrap();
    w.open_version(&path, 1).unwrap();
    let t = frame(&c, |ui| tabs::show(ui, &mut w));
    assert!(t.contains("Teil · v1"), "{t}");
}

#[test]
fn the_save_changes_prompt_names_the_tab_and_offers_three_choices() {
    let c = ctx();
    let mut w = ws();
    w.active_tab_mut()
        .state
        .create_box(DVec3::splat(4.0))
        .unwrap();
    assert!(!w.request_close(0));
    let t = frame(&c, |ui| prompts::show(ui.ctx(), &mut w));
    for must in [
        "Änderungen speichern",
        "Unbenannt 1",
        "Speichern",
        "Verwerfen",
        "Abbrechen",
    ] {
        assert!(t.contains(must), "„{must}“ fehlt:\n{t}");
    }
}

#[test]
fn the_other_prompts_render_their_fields() {
    let c = ctx();
    let mut w = ws();
    let id = w.active_tab().id;
    w.prompt = Some(Prompt::SaveComment {
        tab: id,
        then: None,
        comment: String::new(),
        branch: Some(String::new()),
    });
    let t = frame(&c, |ui| prompts::show(ui.ctx(), &mut w));
    assert!(t.contains("Zweig") && t.contains("Kommentar"), "{t}");
    w.prompt = Some(Prompt::SaveAs {
        tab: id,
        then: None,
        parent: vec![],
        name: "X".into(),
        comment: String::new(),
    });
    let t = frame(&c, |ui| prompts::show(ui.ctx(), &mut w));
    assert!(t.contains("Speichern unter") && t.contains("Name"), "{t}");
    w.prompt = Some(Prompt::Notice("Etwas ist schiefgegangen".into()));
    let t = frame(&c, |ui| prompts::show(ui.ctx(), &mut w));
    assert!(t.contains("Etwas ist schiefgegangen"), "{t}");
    w.prompt = Some(Prompt::TakeOver {
        path: vec!["P".into(), "T.pcad".into()],
        version: None,
    });
    let t = frame(&c, |ui| prompts::show(ui.ctx(), &mut w));
    assert!(t.contains("gesperrt") || t.contains("benutzt"), "{t}");
    w.prompt = Some(Prompt::AutosaveOffer { tab: id });
    let t = frame(&c, |ui| prompts::show(ui.ctx(), &mut w));
    assert!(
        t.contains("Wiederherstellen") && t.contains("Verwerfen"),
        "{t}"
    );
}
