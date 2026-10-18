//! PlanoraCAD – Anwendungslogik und Oberfläche.

pub mod app;
pub mod blend_dialog;
pub mod boolean_dialog;
pub mod browser;
pub mod cards;
pub mod create_dialog;
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
pub mod file_chooser;
pub mod fonts;
pub mod icons;
pub mod import_dialog;
pub mod jobs;
pub mod mesh_body;
pub mod parts_section;
pub mod projects_panel;
pub mod prompts;
pub mod properties;
pub mod sheet_dialog;
pub mod shell;
pub mod shell_panels;
pub mod sketch_panel;
pub mod sketch_state;
pub mod sketch_view;
pub mod state;
pub mod tabs;
pub mod theme;
pub mod versions_panel;
pub mod viewport;
pub mod workspace;

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
