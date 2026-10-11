//! PlanoraCAD – Anwendungslogik und Oberfläche.

pub mod app;
pub mod boolean_dialog;
pub mod browser;
pub mod create_dialog;
pub mod dock;
pub mod feature_dialog;
pub mod fonts;
pub mod icons;
pub mod projects_panel;
pub mod prompts;
pub mod properties;
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
