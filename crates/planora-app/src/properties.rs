use crate::state::AppState;
use planora_kernel::Kernel;
use planora_math::DVec3;
use planora_render::Pick;

/// Maße des gewählten Körpers sowie Angaben zur gewählten Fläche oder Kante.
pub fn show(ui: &mut egui::Ui, state: &mut AppState) {
    ui.heading("Eigenschaften");
    ui.add_space(4.0);
    let Some(index) = state.selected_body.filter(|&i| i < state.bodies.len()) else {
        ui.weak("Nichts ausgewählt.");
        return;
    };

    let mut delete = false;
    let mut moved: Option<DVec3> = None;
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
            ui.label("Position (x/y/z)");
            // Lage der unteren Ecke der Hülle; Änderungen verschieben den Körper.
            let mut position = min;
            ui.horizontal(|ui| {
                for value in [&mut position.x, &mut position.y, &mut position.z] {
                    ui.add(egui::DragValue::new(value).speed(0.5).suffix(" mm"));
                }
            });
            if position != min {
                moved = Some(position - min);
            }
            ui.end_row();
            ui.label("Flächen / Kanten");
            let (faces, edges) = state.kernel.counts(&body.solid);
            ui.label(format!("{faces} / {edges}"));
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
            match state.kernel.face_kind(&state.bodies[index].solid, f.face) {
                Some(kind) => {
                    ui.label(format!("Ausgewählte Fläche: {}", kind.label()));
                }
                None => {
                    ui.weak("Klicke im Viewport eine Fläche oder Kante an.");
                }
            }
        }
        Some(Pick::Edge(e)) if e.body == index => {
            let body = &state.bodies[index];
            match state.kernel.edge_kind(&body.solid, e.edge) {
                Some(kind) => {
                    let length: f64 = body
                        .mesh
                        .edge_lines
                        .get(e.edge.index())
                        .map_or(0.0, |line| {
                            line.points.windows(2).map(|w| w[0].distance(w[1])).sum()
                        });
                    ui.label(format!(
                        "Ausgewählte Kante: {}, Länge ≈ {:.2} mm",
                        kind.label(),
                        length
                    ));
                }
                None => {
                    ui.weak("Klicke im Viewport eine Fläche oder Kante an.");
                }
            }
        }
        _ => {
            ui.weak("Klicke im Viewport eine Fläche oder Kante an.");
        }
    }

    // Erst nach dem Zeichnen anwenden, wenn der Körper nicht mehr geliehen ist.
    if let Some(delta) = moved
        && !delete
        && let Err(e) = state.translate_body(index, delta)
    {
        state.message = Some(e);
    }
    if delete {
        state.remove_body(index);
    }
}
