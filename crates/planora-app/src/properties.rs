use crate::state::AppState;
use planora_kernel::{Curve, Surface};
use planora_render::Pick;

fn surface_name(s: &Surface) -> &'static str {
    match s {
        Surface::Plane { .. } => "Ebene",
        Surface::Cylinder { .. } => "Zylinderfläche",
        Surface::Cone { .. } => "Kegelfläche",
        Surface::Sphere { .. } => "Kugelfläche",
        Surface::Torus { .. } => "Torusfläche",
    }
}

fn curve_name(c: &Curve) -> &'static str {
    match c {
        Curve::Line { .. } => "Gerade",
        Curve::Circle { .. } => "Kreis",
        Curve::Ellipse { .. } => "Ellipse",
    }
}

/// Maße des gewählten Körpers sowie Angaben zur gewählten Fläche oder Kante.
pub fn show(ui: &mut egui::Ui, state: &mut AppState) {
    ui.heading("Eigenschaften");
    ui.add_space(4.0);
    let Some(index) = state.selected_body.filter(|&i| i < state.bodies.len()) else {
        ui.weak("Nichts ausgewählt.");
        return;
    };

    let mut delete = false;
    {
        let body = &mut state.bodies[index];
        ui.label("Name");
        ui.text_edit_singleline(&mut body.name);
        ui.add_space(6.0);
        let (min, max) = body.bounds;
        let size = max - min;
        egui::Grid::new("body_props").num_columns(2).show(ui, |ui| {
            ui.label("Volumen");
            ui.label(format!("≈ {:.2} mm³", body.mass.volume));
            ui.end_row();
            ui.label("Oberfläche");
            ui.label(format!("≈ {:.2} mm²", body.mass.surface_area));
            ui.end_row();
            ui.label("Schwerpunkt");
            ui.label(format!(
                "{:.2} / {:.2} / {:.2} mm",
                body.mass.centroid.x, body.mass.centroid.y, body.mass.centroid.z
            ));
            ui.end_row();
            ui.label("Abmessungen");
            ui.label(format!("{:.2} × {:.2} × {:.2} mm", size.x, size.y, size.z));
            ui.end_row();
            ui.label("Flächen / Kanten");
            ui.label(format!(
                "{} / {}",
                body.solid.faces.len(),
                body.solid.edges.len()
            ));
            ui.end_row();
        });
        ui.add_space(8.0);
        if ui.button("Körper löschen").clicked() {
            delete = true;
        }
    }

    ui.separator();
    match state.selection {
        Some(Pick::Face(f)) if f.body == index => {
            let face = &state.bodies[index].solid.faces[f.face];
            ui.label(format!(
                "Ausgewählte Fläche: {}",
                surface_name(&face.surface)
            ));
        }
        Some(Pick::Edge(e)) if e.body == index => {
            let body = &state.bodies[index];
            let edge = &body.solid.edges[e.edge];
            let length: f64 = body.mesh.edge_lines[e.edge.index()]
                .points
                .windows(2)
                .map(|w| w[0].distance(w[1]))
                .sum();
            ui.label(format!(
                "Ausgewählte Kante: {}, Länge ≈ {:.2} mm",
                curve_name(&edge.curve),
                length
            ));
        }
        _ => {
            ui.weak("Klicke im Viewport eine Fläche oder Kante an.");
        }
    }

    if delete {
        state.remove_body(index);
    }
}
