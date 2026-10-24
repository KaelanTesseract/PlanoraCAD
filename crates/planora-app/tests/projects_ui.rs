use planora_app::project_actions::{create_folder, create_project};
use planora_app::projects_panel::*;
use planora_app::workspace::*;
use planora_app::{fonts, testing::collect_texts};
use planora_project::config::Config;
use planora_project::fsutil::new_id;
use planora_project::library::Library;

fn ctx() -> egui::Context {
    let c = egui::Context::default();
    c.set_fonts(fonts::definitions());
    c
}

fn ws() -> Workspace {
    let root = std::env::temp_dir().join(format!("planora-pp-{}", new_id()));
    Workspace::new(Some(Library::open(&root).unwrap()), Config::default())
}

fn frame(c: &egui::Context, ws: &mut Workspace, st: &mut ProjectsUi) -> String {
    let out = c.run_ui(egui::RawInput::default(), |ui| show(ui, ws, st));
    let t = collect_texts(&out);
    out.drop_without_applying_deltas();
    t
}

#[test]
fn the_panel_shows_the_empty_state_and_then_the_tree() {
    let c = ctx();
    let mut w = ws();
    let mut st = ProjectsUi::default();
    let t = frame(&c, &mut w, &mut st);
    assert!(t.contains("Noch keine Projekte"), "{t}");
    let p = create_project(&mut w, &mut st, "Gehäuse").unwrap();
    let f = create_folder(&mut w, &mut st, &p, "Deckel").unwrap();
    w.save_as(&f, "Oben", "").unwrap();
    let t = frame(&c, &mut w, &mut st);
    for must in ["Gehäuse", "Deckel", "Oben"] {
        assert!(t.contains(must), "„{must}“ fehlt:\n{t}");
    }
}

#[test]
fn the_tree_is_not_reread_every_frame() {
    let c = ctx();
    let mut w = ws();
    let mut st = ProjectsUi::default();
    create_project(&mut w, &mut st, "A").unwrap();
    frame(&c, &mut w, &mut st);
    // direkt auf der Platte angelegt: erscheint erst nach „Aktualisieren“ (invalidate)
    w.library.as_ref().unwrap().create_project("B").unwrap();
    let t = frame(&c, &mut w, &mut st);
    assert!(!t.contains('B') || t.contains("Aktualisieren"), "{t}");
    st.invalidate();
    let t = frame(&c, &mut w, &mut st);
    assert!(t.contains("B"), "{t}");
}

#[test]
fn without_a_library_the_panel_asks_for_a_root() {
    let c = ctx();
    let mut w = Workspace::new(None, Config::default());
    let mut st = ProjectsUi::default();
    let t = frame(&c, &mut w, &mut st);
    assert!(t.contains("Projektordner"), "{t}");
}

#[test]
fn a_save_as_through_the_workspace_shows_up_without_refresh() {
    let c = ctx();
    let mut w = ws();
    let mut st = ProjectsUi::default();
    let p = create_project(&mut w, &mut st, "Gehäuse").unwrap();
    let t = frame(&c, &mut w, &mut st);
    assert!(!t.contains("Neuteil"), "{t}");
    w.save_as(&p, "Neuteil", "").unwrap();
    let t = frame(&c, &mut w, &mut st);
    assert!(t.contains("Neuteil"), "{t}");
}
