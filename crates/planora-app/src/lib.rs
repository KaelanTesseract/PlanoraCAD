//! PlanoraCAD – Anwendungslogik und Oberfläche.

pub mod about_dialog;
pub mod app;
pub mod balloon_browser;
pub mod balloon_tool;
pub mod blend_dialog;
pub mod boolean_dialog;
pub mod browser;
pub mod build_info;
pub mod canvas;
pub mod cards;
pub mod commands;
pub mod crash;
pub mod crash_addr;
pub mod crash_dialog;
pub mod create_dialog;
pub mod curvature_dialog;
pub mod detach;
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
pub mod fonts_library;
pub mod formula_field;
pub mod history;
pub mod history_bar;
pub mod icons;
pub mod import_dialog;
pub mod insert;
pub mod install_mode;
pub mod isoparametric_dialog;
pub mod jobs;
pub mod keymap;
pub mod local_time;
pub mod logo;
pub mod lost_projections_dialog;
pub mod mesh_body;
pub mod msg;
pub mod parameter_dialog;
pub mod parts_section;
pub mod places;
pub mod plane_dialog;
pub mod profile_pick;
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
pub mod sketch_blend;
pub mod sketch_cursor;
pub mod sketch_dimension;
pub mod sketch_drag3;
pub mod sketch_draw3;
pub mod sketch_dxf_dialog;
pub mod sketch_grips;
pub mod sketch_handles;
pub mod sketch_inline;
pub mod sketch_input;
pub mod sketch_kinds;
pub mod sketch_panel;
pub mod sketch_pattern;
pub mod sketch_project;
pub mod sketch_state;
pub mod sketch_text;
pub mod sketch_tree;
pub mod sketch_triad;
pub mod sketch_under;
pub mod sketch_view;
pub mod spun_profile_dialog;
pub mod state;
pub mod step_import_dialog;
pub mod surface_project_dialog;
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

    use planora_render::StrokeVertex;

    /// Ist das Dreieck eine Fläche (alle Ecken mit `edge` 0) statt eines Linienstücks?
    fn is_fill(t: &[StrokeVertex]) -> bool {
        t.iter().all(|v| v.edge == 0.0)
    }

    /// Zahl der Flächendreiecke in der Farbe `color`.
    pub fn fill_triangles(v: &[StrokeVertex], color: egui::Color32) -> usize {
        v.chunks(3)
            .filter(|t| is_fill(t) && t[0].color == crate::sketch_view::stroke_rgba(color))
            .count()
    }

    /// Zahl der Liniendreiecke in der Farbe `color`.
    pub fn line_triangles(v: &[StrokeVertex], color: egui::Color32) -> usize {
        v.chunks(3)
            .filter(|t| !is_fill(t) && t[0].color == crate::sketch_view::stroke_rgba(color))
            .count()
    }

    /// Farben der Linienstücke als GPU-Farben (jede einmal, in der Reihenfolge des ersten
    /// Auftretens); vergleichen mit [`crate::sketch_view::stroke_rgba`].
    pub fn line_colors(v: &[StrokeVertex]) -> Vec<[f32; 4]> {
        let mut out = Vec::new();
        for t in v.chunks(3).filter(|t| !is_fill(t)) {
            let c = t[0].color;
            if !out.contains(&c) {
                out.push(c);
            }
        }
        out
    }

    /// GPU-Linien und -Flächen der fertigen Skizzen im Viewport `rect` bei 1 Pixel je Punkt.
    pub fn finished_strokes(state: &crate::state::AppState, rect: egui::Rect) -> Vec<StrokeVertex> {
        let mut out = Vec::new();
        let view = crate::viewport::stroke_view(state, rect, 1.0);
        crate::sketch_view::finished_sketch_strokes(state, &view, &mut out);
        out
    }

    /// GPU-Flächen und -Ränder der Konstruktionsebenen im Viewport `rect` bei 1 Pixel je Punkt.
    pub fn plane_strokes(state: &crate::state::AppState, rect: egui::Rect) -> Vec<StrokeVertex> {
        let mut out = Vec::new();
        let view = crate::viewport::stroke_view(state, rect, 1.0);
        crate::sketch_view::plane_strokes(state, &view, &mut out);
        out
    }

    /// Zahl der gezeichneten Konstruktionsebenen (je Rechteck zwei Flächendreiecke).
    pub fn plane_count(state: &crate::state::AppState, rect: egui::Rect) -> usize {
        plane_strokes(state, rect)
            .chunks(3)
            .filter(|t| is_fill(t))
            .count()
            / 2
    }
}
