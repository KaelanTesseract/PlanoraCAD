//! Plätze und ihre Schlüssel in der Konfiguration.

/// Ein Platz, der sich als eigenes Fenster abdocken lässt.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Place {
    Tree,
    Right,
    SketchSide,
    Dialog(&'static str),
}

/// Alle Dialogschlüssel (`Dialog::dock_key`): fest und sprachunabhängig, je Dialog bzw. je
/// Feature-Art einer. Der Test `tests/dialog_dock_keys.rs` hält die Liste und die Aufrufe
/// gleich.
pub const DIALOG_KEYS: &[&str] = &[
    "about",
    "boolean",
    // Erzeugen (`create_dialog`), je Art
    "box",
    "cylinder",
    "sphere",
    "cone",
    "extrude",
    "revolve",
    // Verrunden und Fase (`blend_dialog`)
    "fillet",
    "chamfer",
    // Bearbeiten eines Features (`feature_dialog`), je Art; eigene Plätze, weil das Fenster
    // „Bearbeiten“ neben dem Erzeugen-Fenster derselben Art offen sein kann
    "edit_box",
    "edit_cylinder",
    "edit_sphere",
    "edit_cone",
    "edit_extrude",
    "edit_revolve",
    "edit_boolean",
    "edit_move",
    "edit_fillet",
    "edit_chamfer",
    "edit_feature",
    // Konstruktionsebenen (`plane_dialog`), je Befehl
    "plane_offset",
    "plane_angle",
    "plane_tangent",
    "plane_mid",
    "plane_three_points",
    // Einfügen (Skizzen 9)
    "insert_dxf",
    "insert_svg",
    "canvas",
    // Auf Fläche projizieren und Schnittkurve (Skizzen 10)
    "sketch_project_to_surface",
    "sketch_intersection_curve",
    // Drehprofil (Skizzen 11b)
    "sketch_spun_profile",
    "sketch_isoparametric",
    // Krümmungsanzeige einrichten (Skizzen 11a)
    "curvature_display",
    // Verlorene Projektionen verwalten (Skizzen 11a)
    "lost_projections",
    // Skizze als DXF speichern (Skizzen 11b)
    "sketch_dxf",
    // Skizzengruppe umbenennen (Skizzen 11b)
    "tree_group_rename",
    // Baum und Zeichnung
    "tree_rename",
    "tree_delete",
    "drawing_rename",
    "drawing_delete",
    "dimension_delete",
    "dimension_edit",
    "drawing_new",
    "sheet_info",
    // Austausch
    "export",
    "mesh_import",
    "step_import",
    // Fenster
    "addins",
    "parameters",
    "projects",
    "settings",
    "shortcuts",
    "feedback",
    "update_done",
];

/// Die abdockbaren Felder (alle Plätze außer den Dialogen).
pub const FIELDS: [Place; 3] = [Place::Tree, Place::Right, Place::SketchSide];

impl Place {
    /// Ein Feld (Baum, rechte Spalte, Skizzenfeld), kein Dialog?
    pub fn is_field(self) -> bool {
        !matches!(self, Place::Dialog(_))
    }

    /// Schlüssel in `config.json` (`detached`).
    pub fn key(self) -> String {
        match self {
            Place::Tree => "tree".to_string(),
            Place::Right => "right".to_string(),
            Place::SketchSide => "sketch_side".to_string(),
            Place::Dialog(k) => format!("dialog:{k}"),
        }
    }
}
