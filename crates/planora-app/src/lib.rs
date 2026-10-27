//! PlanoraCAD – Anwendungslogik und Oberfläche.

pub mod about_dialog;
pub mod app;
pub mod balloon_browser;
pub mod balloon_tool;
pub mod blend_dialog;
pub mod boolean_dialog;
pub mod browser;
pub mod build_info;
pub mod cards;
pub mod commands;
pub mod crash;
pub mod crash_addr;
pub mod crash_dialog;
pub mod create_dialog;
pub mod diag;
pub mod dialog;
pub mod dimension_browser;
pub mod dimension_dialog;
pub mod dimension_state;
pub mod dimension_tool;
pub mod drawing_browser;
pub mod drawing_dialog;
pub mod drawing_state;
pub mod drawing_view;
pub mod export_dialog;
pub mod feature_dialog;
pub mod feedback;
pub mod feedback_dialog;
pub mod feedback_shot;
pub mod file_chooser;
pub mod fonts;
pub mod formula_field;
pub mod history;
pub mod history_bar;
pub mod icons;
pub mod import_dialog;
pub mod install_mode;
pub mod jobs;
pub mod keymap;
pub mod local_time;
pub mod logo;
pub mod mesh_body;
pub mod parameter_dialog;
pub mod parts_section;
pub mod places;
pub mod project_actions;
pub mod project_tiles;
pub mod projects_window;
pub mod prompts;
pub mod properties;
pub mod section_tool;
pub mod settings_window;
pub mod sheet_dialog;
pub mod shell;
pub mod shell_panels;
pub mod sketch_panel;
pub mod sketch_state;
pub mod sketch_view;
pub mod state;
pub mod step_import_dialog;
pub mod system_language;
pub mod tabs;
pub mod theme;
pub mod thumbnail;
pub mod thumbnail_cache;
pub mod thumbnail_worker;
pub mod undo_labels;
pub mod update_check;
pub mod update_dialog;
pub mod update_install;
pub mod versions_panel;
pub mod viewport;
pub mod workspace;
pub mod zip_store;

/// Laufende Version des Programms (aus `Cargo.toml`).
pub const RUNNING_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Laufende Version für Updates und Anzeige. Eine ungeeignete Versionsnummer in `Cargo.toml`
/// lässt das abbrechen; ein Test deckt das schon beim Bauen auf.
pub fn running_version() -> planora_update::Version {
    planora_update::Version::from_cargo(RUNNING_VERSION)
}

/// Hilfen für die fensterlosen Oberflächentests.
#[doc(hidden)]
pub mod testing {
    /// Alle Texte der gezeichneten Formen eines Bildes (auch verschachtelte), je Zeile einer.
    pub fn collect_texts(out: &egui::FullOutput) -> String {
        fn walk(shape: &egui::epaint::Shape, seen: &mut String) {
            match shape {
                egui::epaint::Shape::Text(t) => {
                    seen.push_str(t.galley.text());
                    seen.push('\n');
                }
                egui::epaint::Shape::Vec(v) => v.iter().for_each(|s| walk(s, seen)),
                _ => {}
            }
        }
        let mut seen = String::new();
        for clipped in &out.shapes {
            walk(&clipped.shape, &mut seen);
        }
        seen
    }
}
