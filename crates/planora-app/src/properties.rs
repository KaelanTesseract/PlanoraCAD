use crate::browser::error_color;
use crate::mesh_body::{euler_degrees, report_text, rotation_from_degrees, warning_color};
use crate::sketch_view::format_number;
use crate::state::AppState;
use crate::theme;
use planora_exchange::MeshTransform;
use planora_feature::{
    BlendArt, FeatureId, FeatureKind, FeatureStatus, MAX_MESH_SCALE, MIN_MESH_SCALE, SketchPlane,
    StandardPlane,
};
use planora_kernel::{BoolOp, Kernel};
use planora_math::DVec3;
use planora_render::Pick;

/// Beschriftung der Zeile „Geschlossen“: Der Bericht stammt vom Einlesen; das Export-Fenster
/// prüft nach der Lage noch einmal und kann deshalb ein anderes Ergebnis zeigen.
const CLOSED_AT_IMPORT_LABEL: &str = "Geschlossen beim Einlesen";

/// Vorsatz der Warnung aus dem Bericht beim Einlesen.
const AT_IMPORT_PREFIX: &str = "Beim Einlesen: ";

/// Warnung unter den Zeilen: der Bericht von damals, als solcher gekennzeichnet.
fn import_warning(r: &planora_exchange::MeshReport) -> String {
    format!("{AT_IMPORT_PREFIX}{}", report_text(r))
}

/// Hinweis unter den Netz-Körpern (ein Satz, ohne doppelte Leerzeichen).
const MESH_BODY_HINT: &str = concat!(
    "Netz-Körper lassen sich verschieben, drehen, vergrößern, messen und exportieren, ",
    "aber nicht mit anderen Körpern verrechnen, runden oder als Fläche für eine Skizze nutzen."
);

/// Eigenschaften: für einen gewählten Körper Maße, Fläche/Kante und Position; für ein
/// gewähltes Feature Art, Name, Status und Parameter.
pub fn show(ui: &mut egui::Ui, state: &mut AppState) {
    theme::section_header(ui, "Eigenschaften", theme::HEADER_H);
    ui.add_space(theme::SPACE_1);
    show_content(ui, state);
}

/// Wie [`show`], ohne den Abschnittskopf (den zeigt dann der Aufrufer, z. B. als Umschalter).
pub fn show_content(ui: &mut egui::Ui, state: &mut AppState) {
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
    let response = theme::text_field(ui, egui::TextEdit::singleline(&mut name));
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
        FeatureKind::Verrunden {
            body,
            edges,
            art,
            tangentenkette,
        } => {
            let mut rows = vec![
                ("Körper", state.body_ref_name(*body)),
                ("Kanten", edges.len().to_string()),
                ("Art", art.kind_label().to_string()),
            ];
            match art {
                BlendArt::Rundung { radius } => rows.push(("Radius", mm(*radius))),
                BlendArt::RundungVariabel { punkte } => {
                    rows.push(("Punkte", punkte.len().to_string()))
                }
                BlendArt::FaseGleich { abstand } => rows.push(("Abstand", mm(*abstand))),
                BlendArt::FaseZwei { abstand1, abstand2 } => {
                    rows.push(("Abstand 1", mm(*abstand1)));
                    rows.push(("Abstand 2", mm(*abstand2)));
                }
                BlendArt::FaseWinkel { abstand, winkel } => {
                    rows.push(("Abstand", mm(*abstand)));
                    rows.push(("Winkel", format!("{} °", format_number(*winkel, 2))));
                }
            }
            rows.push((
                "Tangentenkette",
                if *tangentenkette { "an" } else { "aus" }.to_string(),
            ));
            rows
        }
        FeatureKind::MeshBody {
            mesh,
            offset,
            rotation,
            scale,
        } => {
            let triangles = doc.mesh(*mesh).map_or(0, |m| m.triangle_count());
            let d = euler_degrees(*rotation);
            vec![
                ("Netz", format!("{triangles} Dreiecke")),
                ("Position", triple(*offset)),
                (
                    "Drehung",
                    format!(
                        "{} / {} / {} °",
                        format_number(d.x, 2),
                        format_number(d.y, 2),
                        format_number(d.z, 2)
                    ),
                ),
                ("Maßstab", format_number(*scale, 4)),
            ]
        }
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
            ui.label(theme::value(label));
            ui.end_row();
            ui.label("Status");
            match &status {
                FeatureStatus::Ok => ui.label(theme::value("berechnet")),
                FeatureStatus::Error(e) => {
                    ui.colored_label(error_color(ui.visuals().dark_mode), format!("Fehler: {e}"))
                }
                FeatureStatus::Skipped(why) => ui.weak(format!("übersprungen: {why}")),
                FeatureStatus::Suppressed => ui.weak("unterdrückt"),
                FeatureStatus::NotComputed => {
                    ui.weak("nicht berechnet (hinter dem Rückrollmarker)")
                }
            };
            ui.end_row();
            for (k, v) in &rows {
                ui.label(*k);
                ui.label(theme::value(v));
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
            if state.try_close_sketch().is_ok() {
                state.edit_request = Some(id);
            }
        }
    }
}

/// Zeilen „Volumen“ und „Oberfläche“ des Eigenschaften-Rasters.
fn mass_rows(ui: &mut egui::Ui, mass: &planora_mesh::MassProperties) {
    ui.label("Volumen");
    ui.label(theme::value(format!("≈ {:.2} mm³", mass.volume)));
    ui.end_row();
    ui.label("Oberfläche");
    ui.label(theme::value(format!("≈ {:.2} mm²", mass.surface_area)));
    ui.end_row();
}

/// Maße des gewählten Körpers sowie Angaben zur gewählten Fläche oder Kante. Gelöscht wird
/// über das Feature im Feature-Baum.
fn body(ui: &mut egui::Ui, state: &mut AppState, index: usize) {
    if state.is_mesh(index) {
        mesh_body(ui, state, index);
        return;
    }
    let mut moved: Option<DVec3> = None;
    let mut dragging = false;
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
            mass_rows(ui, &body.mass);
            ui.label("Schwerpunkt");
            ui.label(theme::value(format!(
                "{:.2} / {:.2} / {:.2} mm",
                body.mass.centroid.x, body.mass.centroid.y, body.mass.centroid.z
            )));
            ui.end_row();
            ui.label("Abmessungen");
            ui.label(theme::value(format!(
                "{:.2} × {:.2} × {:.2} mm",
                size.x, size.y, size.z
            )));
            ui.end_row();
            ui.label("Position (x/y/z)");
            // Lage der unteren Ecke der Hülle; Änderungen verschieben den Körper.
            let mut position = min;
            ui.horizontal(|ui| {
                for value in [&mut position.x, &mut position.y, &mut position.z] {
                    let r = theme::field(ui, egui::DragValue::new(value).speed(0.5).suffix(" mm"));
                    dragging |= r.dragged();
                }
            });
            if position != min {
                moved = Some(position - min);
            }
            ui.end_row();
            ui.label("Flächen / Kanten");
            let (faces, edges) = state.kernel.counts(&body.solid);
            ui.label(theme::value(format!("{faces} / {edges}")));
            ui.end_row();
        });
    }

    ui.separator();
    match state.selection {
        Some(Pick::Face(f)) if f.body == index => {
            match state.kernel.face_kind(&state.bodies[index].solid, f.face) {
                Some(kind) => {
                    ui.label(theme::value(format!(
                        "Ausgewählte Fläche: {}",
                        kind.label()
                    )));
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
                    ui.label(theme::value(format!(
                        "Ausgewählte Kante: {}, Länge ≈ {:.2} mm",
                        kind.label(),
                        length
                    )));
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
    // Während des Ziehens erweitert jedes Bild das Verschieben desselben Zugs.
    match (moved, dragging) {
        (Some(delta), true) => {
            let _ = state.move_body_dragging(index, delta);
        }
        (Some(delta), false) => {
            let _ = state.move_body(index, delta);
        }
        (None, true) => {}
        (None, false) => state.end_drag(),
    }
}

/// Eigenschaften eines Netz-Körpers (Spec §6): Name, Maße, Lage, Drehung in Grad, Maßstab und
/// Prüfbericht. Änderungen wirken direkt auf den Netz-Körper (kein neues Feature).
fn mesh_body(ui: &mut egui::Ui, state: &mut AppState, index: usize) {
    let Some(t) = state.mesh_transform(index) else {
        return;
    };
    let shown_degrees = euler_degrees(t.rotation);
    let (mut offset, mut degrees, mut scale) = (t.offset, shown_degrees, t.scale);
    let renamed;
    {
        let body = &state.bodies[index];
        ui.label("Name");
        renamed = name_field(ui, state.naming_feature(index), &body.name);
        ui.add_space(6.0);
        let size = body.bounds.1 - body.bounds.0;
        egui::Grid::new("mesh_props").num_columns(2).show(ui, |ui| {
            ui.label("Art");
            ui.label(theme::value("Netz-Körper"));
            ui.end_row();
            mass_rows(ui, &body.mass);
            ui.label("Abmessungen");
            ui.label(theme::value(format!(
                "{:.2} × {:.2} × {:.2} mm",
                size.x, size.y, size.z
            )));
            ui.end_row();
            ui.label("Position (x/y/z)");
            ui.horizontal(|ui| {
                for v in [&mut offset.x, &mut offset.y, &mut offset.z] {
                    theme::field(ui, egui::DragValue::new(v).speed(0.5).suffix(" mm"));
                }
            });
            ui.end_row();
            ui.label("Drehung (x/y/z)");
            ui.horizontal(|ui| {
                for v in [&mut degrees.x, &mut degrees.y, &mut degrees.z] {
                    theme::field(ui, egui::DragValue::new(v).speed(1.0).suffix(" °"));
                }
            });
            ui.end_row();
            ui.label("Maßstab");
            theme::field(
                ui,
                egui::DragValue::new(&mut scale)
                    .speed(0.01)
                    .range(MIN_MESH_SCALE..=MAX_MESH_SCALE),
            );
            ui.end_row();
            if let Some(r) = body.mesh_report {
                ui.label("Dreiecke");
                ui.label(theme::value(r.triangles.to_string()));
                ui.end_row();
                ui.label(CLOSED_AT_IMPORT_LABEL);
                ui.label(theme::value(if r.closed { "ja" } else { "nein" }));
                ui.end_row();
            }
        });
        if let Some(r) = body.mesh_report
            && !r.closed
        {
            ui.colored_label(warning_color(ui.visuals().dark_mode), import_warning(&r));
        }
        ui.add_space(4.0);
        ui.weak(MESH_BODY_HINT);
    }
    if let (Some(name), Some(id)) = (renamed, state.naming_feature(index)) {
        let _ = state.rename_feature(id, &name);
    }
    // Die Drehung wird nur neu gebildet, wenn ein Winkel geändert wurde (sonst bliebe sie durch
    // das Hin- und Zurückrechnen nicht bitgleich).
    let rotation = if degrees == shown_degrees {
        t.rotation
    } else {
        rotation_from_degrees(degrees)
    };
    let new = MeshTransform {
        offset,
        rotation,
        scale,
    };
    if new != t {
        let _ = state.set_mesh_transform(index, new);
    }
}

#[cfg(test)]
mod tests {
    use super::{CLOSED_AT_IMPORT_LABEL, MESH_BODY_HINT, import_warning};
    use planora_exchange::MeshReport;

    #[test]
    fn the_report_is_labelled_as_taken_when_the_file_was_read() {
        let open = MeshReport {
            open_edges: 3,
            ..MeshReport::default()
        };
        let text = import_warning(&open);
        assert!(text.starts_with("Beim Einlesen: "), "{text}");
        assert!(text.contains("3 offene Kanten"), "{text}");
        assert!(CLOSED_AT_IMPORT_LABEL.contains("beim Einlesen"));
    }

    #[test]
    fn the_mesh_body_hint_has_no_runs_of_spaces() {
        assert!(!MESH_BODY_HINT.contains("  "), "{MESH_BODY_HINT:?}");
        assert!(MESH_BODY_HINT.contains("exportieren, aber nicht"));
    }
}
