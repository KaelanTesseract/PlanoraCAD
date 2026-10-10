use crate::browser::ERROR_COLOR;
use crate::sketch_view::format_number;
use crate::state::AppState;
use planora_feature::{FeatureId, FeatureKind, FeatureStatus, SketchPlane, StandardPlane};
use planora_kernel::{BoolOp, Kernel};
use planora_math::DVec3;
use planora_render::Pick;

/// Eigenschaften: für einen gewählten Körper Maße, Fläche/Kante und Position; für ein
/// gewähltes Feature Art, Name, Status und Parameter.
pub fn show(ui: &mut egui::Ui, state: &mut AppState) {
    ui.heading("Eigenschaften");
    ui.add_space(4.0);
    if let Some(index) = state.selected_body.filter(|&i| i < state.bodies.len()) {
        body(ui, state, index);
    } else if let Some(id) = state
        .selected_feature
        .filter(|id| state.document.feature(*id).is_some())
    {
        feature(ui, state, id);
    } else {
        ui.weak("Nichts ausgewählt.");
    }
}

/// Namensfeld eines Features. Während der Eingabe steht der Entwurf im Speicher von egui,
/// damit sich das Feld auch kurz leeren lässt (leere Namen sind ungültig und werden nicht
/// übernommen). Liefert einen neuen, gültigen Namen.
fn name_field(ui: &mut egui::Ui, id: Option<FeatureId>, current: &str) -> Option<String> {
    let key = egui::Id::new(("feature_name", id));
    let mut name = ui
        .data(|d| d.get_temp::<String>(key))
        .unwrap_or_else(|| current.to_string());
    let response = ui.text_edit_singleline(&mut name);
    let mut renamed = None;
    if response.changed() {
        if !name.trim().is_empty() && name != current {
            renamed = Some(name.clone());
        }
        ui.data_mut(|d| d.insert_temp(key, name));
    } else if !response.has_focus() {
        ui.data_mut(|d| d.remove::<String>(key));
    }
    renamed
}

fn mm(v: f64) -> String {
    format!("{} mm", format_number(v, 2))
}

fn triple(v: DVec3) -> String {
    format!(
        "{} / {} / {} mm",
        format_number(v.x, 2),
        format_number(v.y, 2),
        format_number(v.z, 2)
    )
}

/// Parameter eines Features als Text (Bezeichnung, Wert).
pub fn parameter_rows(state: &AppState, kind: &FeatureKind) -> Vec<(&'static str, String)> {
    let doc = &state.document;
    let feature_name = |id: FeatureId| {
        doc.feature(id)
            .map_or_else(|| format!("Feature {}", id.0), |f| f.name.clone())
    };
    match kind {
        FeatureKind::Sketch(s) => {
            let plane = match &s.plane {
                SketchPlane::Standard(StandardPlane::XY) => "XY-Ebene".to_string(),
                SketchPlane::Standard(StandardPlane::XZ) => "XZ-Ebene".to_string(),
                SketchPlane::Standard(StandardPlane::YZ) => "YZ-Ebene".to_string(),
                SketchPlane::OnFace(r) => format!(
                    "Fläche „{}“ von „{}“",
                    r.role,
                    feature_name(FeatureId(r.origin))
                ),
            };
            vec![
                ("Ebene", plane),
                ("Kurven", s.sketch.curves().count().to_string()),
                ("Kantenkopien", s.copies.len().to_string()),
            ]
        }
        FeatureKind::Box { size, origin } => vec![
            (
                "Größe",
                format!(
                    "{} × {} × {} mm",
                    format_number(size.x, 2),
                    format_number(size.y, 2),
                    format_number(size.z, 2)
                ),
            ),
            ("Position", triple(*origin)),
        ],
        FeatureKind::Cylinder {
            radius,
            height,
            origin,
        }
        | FeatureKind::Cone {
            radius,
            height,
            origin,
        } => vec![
            ("Radius", mm(*radius)),
            ("Höhe", mm(*height)),
            ("Position", triple(*origin)),
        ],
        FeatureKind::Sphere { radius, origin } => {
            vec![("Radius", mm(*radius)), ("Position", triple(*origin))]
        }
        FeatureKind::Extrude {
            sketch,
            profile,
            height,
        } => vec![
            ("Skizze", feature_name(*sketch)),
            ("Profil", (profile + 1).to_string()),
            ("Höhe", mm(*height)),
        ],
        FeatureKind::Revolve {
            sketch,
            profile,
            axis,
        } => vec![
            ("Skizze", feature_name(*sketch)),
            ("Profil", (profile + 1).to_string()),
            ("Drehachse", format!("Kurve {}", axis.index() + 1)),
        ],
        FeatureKind::Boolean { target, tool, op } => vec![
            ("Ziel", state.body_ref_name(*target)),
            ("Werkzeug", state.body_ref_name(*tool)),
            (
                "Operation",
                match op {
                    BoolOp::Union => "Vereinigen",
                    BoolOp::Difference => "Abziehen",
                    BoolOp::Intersection => "Schneiden",
                }
                .to_string(),
            ),
        ],
        FeatureKind::Move { body, delta } => vec![
            ("Körper", state.body_ref_name(*body)),
            ("Verschiebung", triple(*delta)),
        ],
    }
}

/// Eigenschaften eines Features der Zeitleiste.
fn feature(ui: &mut egui::Ui, state: &mut AppState, id: FeatureId) {
    let Some(f) = state.document.feature(id) else {
        return;
    };
    let (name, label, status) = (f.name.clone(), f.kind.label(), f.status.clone());
    let rows = parameter_rows(state, &f.kind);
    let sketch = matches!(f.kind, FeatureKind::Sketch(_));
    ui.label("Name");
    let renamed = name_field(ui, Some(id), &name);
    ui.add_space(6.0);
    egui::Grid::new("feature_props")
        .num_columns(2)
        .show(ui, |ui| {
            ui.label("Art");
            ui.label(label);
            ui.end_row();
            ui.label("Status");
            match &status {
                FeatureStatus::Ok => ui.label("berechnet"),
                FeatureStatus::Error(e) => ui.colored_label(ERROR_COLOR, format!("Fehler: {e}")),
                FeatureStatus::Skipped(why) => ui.weak(format!("übersprungen: {why}")),
                FeatureStatus::Suppressed => ui.weak("unterdrückt"),
                FeatureStatus::NotComputed => {
                    ui.weak("nicht berechnet (hinter dem Rückrollmarker)")
                }
            };
            ui.end_row();
            for (k, v) in &rows {
                ui.label(*k);
                ui.label(v);
                ui.end_row();
            }
        });
    ui.add_space(8.0);
    let edit = ui
        .button(if sketch {
            "Skizze öffnen"
        } else {
            "Bearbeiten …"
        })
        .clicked();
    if let Some(name) = renamed {
        let _ = state.rename_feature(id, &name);
    }
    if edit {
        if sketch {
            if state.open_sketch_id() != Some(id) {
                let _ = state.open_sketch(id);
            }
        } else {
            state.close_sketch();
            state.edit_request = Some(id);
        }
    }
}

/// Maße des gewählten Körpers sowie Angaben zur gewählten Fläche oder Kante. Gelöscht wird
/// über das Feature im Feature-Baum.
fn body(ui: &mut egui::Ui, state: &mut AppState, index: usize) {
    let mut moved: Option<DVec3> = None;
    let renamed;
    {
        let body = &state.bodies[index];
        ui.label("Name");
        // Der Name gehört dem erzeugenden Feature.
        renamed = name_field(ui, state.naming_feature(index), &body.name);
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

    // Erst nach dem Zeichnen anwenden, wenn der Körper nicht mehr geliehen ist. Fehler legt
    // `AppState` in `message` ab.
    if let (Some(name), Some(id)) = (renamed, state.naming_feature(index)) {
        let _ = state.rename_feature(id, &name);
    }
    if let Some(delta) = moved {
        let _ = state.move_body(index, delta);
    }
}
