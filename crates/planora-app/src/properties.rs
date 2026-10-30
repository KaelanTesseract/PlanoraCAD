use crate::browser::error_color;
use crate::formula_field::FormulaField;
use crate::mesh_body::{euler_degrees, report_text, rotation_from_degrees, warning_color};
use crate::sketch_view::format_number;
use crate::state::{AppState, BodyKind, step_label};
use crate::theme;
use planora_exchange::MeshTransform;
use planora_feature::expr::number_text;
use planora_feature::{
    BlendArt, FeatureId, FeatureKind, FeatureStatus, FieldKey, ParamUnit, SketchPlane,
    StandardPlane, StepShape, field_unit, field_value,
};
use planora_i18n::{fmt_num, localize_decimal, tr};
use planora_kernel::{BoolOp, Kernel};
use planora_math::DVec3;
use planora_render::Pick;

/// Beschriftung der Zeile „Geschlossen“: Der Bericht stammt vom Einlesen; das Export-Fenster
/// prüft nach der Lage noch einmal und kann deshalb ein anderes Ergebnis zeigen.
fn closed_at_import_label() -> String {
    tr!("props-closed-at-import")
}

/// Bezeichnung der Zahlenfelder für die Lage (die Einheit steht hier, nicht in jedem Feld, damit
/// die Zahlen in ein Drittel der Feldbreite passen).
pub fn position_label() -> String {
    tr!("props-position-label")
}

/// Bezeichnung der Zahlenfelder für die Drehung eines Netz-Körpers.
pub fn rotation_label() -> String {
    tr!("props-rotation-label")
}

/// Warnung unter den Zeilen: der Bericht von damals, als solcher gekennzeichnet.
fn import_warning(r: &planora_exchange::MeshReport) -> String {
    tr!("props-at-import", report = report_text(r))
}

/// Hinweis unter den Netz-Körpern (ein Satz, ohne doppelte Leerzeichen).
fn mesh_body_hint() -> String {
    tr!("props-mesh-body-hint")
}

/// Warnung bei STEP-Körpern mit grober Genauigkeit der Datei (gröber als 1e-5 mm), in Warnfarbe.
pub fn step_coarse_hint() -> String {
    tr!("props-step-coarse-hint")
}

/// Genauigkeit in mm für die Anzeige: höchstens vier gültige Stellen, Dezimaltrenner der Sprache,
/// ohne überflüssige Nullen („0,001 mm“, „0,00000254 mm“), wie in den Meldungen des STEP-Lesers.
fn tolerance_text(t: f64) -> String {
    if !t.is_finite() || t <= 0.0 {
        return "0 mm".into();
    }
    let digits = (3 - t.log10().floor() as i32).max(0) as usize;
    let s = format!("{t:.digits$}");
    let s = if s.contains('.') {
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    } else {
        s
    };
    format!("{} mm", planora_i18n::localize_decimal(&s))
}

/// Genauigkeit der Datei für die Anzeige; nennt die Datei keine, „unbekannt“ mit dem
/// angenommenen Wert (P4).
fn accuracy_text(shape: &StepShape) -> String {
    let t = tolerance_text(shape.tolerance_mm());
    if shape.tolerance_known() {
        t
    } else {
        tr!("props-accuracy-unknown", assumed = t)
    }
}

/// Zeilen „Flächen“ und „Genauigkeit der Datei“ einer STEP-Form (Flächen der Form, wie sie aus
/// der Datei kam; „–“, wenn sie sich nicht lesen lässt). `faces_label` ist die Bezeichnung der
/// ersten Zeile.
fn step_rows(state: &AppState, shape: &StepShape, faces_label: String) -> Vec<(String, String)> {
    let faces = shape.solid().map_or_else(
        |_| "–".to_string(),
        |s| state.kernel.counts(s).0.to_string(),
    );
    vec![
        (faces_label, faces),
        (tr!("props-accuracy"), accuracy_text(shape)),
    ]
}

/// Warnung unter den Zeilen eines groben STEP-Körpers.
fn step_warning(ui: &mut egui::Ui, shape: &StepShape) {
    if shape.is_coarse() {
        ui.colored_label(warning_color(ui.visuals().dark_mode), step_coarse_hint());
    }
}

/// Eigenschaften: für einen gewählten Körper Maße, Fläche/Kante und Position; für ein
/// gewähltes Feature Art, Name, Status und Parameter.
pub fn show(ui: &mut egui::Ui, state: &mut AppState) {
    theme::section_header(ui, &tr!("tab-properties"), theme::HEADER_H);
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
        ui.weak(tr!("props-nothing-selected"));
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

/// Zeile „Bezeichnung – Wert“ in der Breite des Felds. Passt beides nebeneinander, steht der Wert
/// rechtsbündig neben der Bezeichnung; sonst steht er darunter und bricht um. So wird kein Wert
/// abgeschnitten, auch nicht bei langen Zahlen oder Namen (Hand-Prüfung Haltepunkt 2).
fn row(ui: &mut egui::Ui, label: &str, value: impl Into<egui::WidgetText>) {
    let value: egui::WidgetText = value.into();
    let fits = {
        let width = |text: egui::WidgetText| {
            text.into_galley(
                ui,
                Some(egui::TextWrapMode::Extend),
                f32::INFINITY,
                egui::TextStyle::Body,
            )
            .size()
            .x
        };
        width(label.into()) + theme::SPACE_2 + width(value.clone()) <= ui.available_width()
    };
    if fits {
        ui.horizontal(|ui| {
            ui.label(label);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(value);
            });
        });
    } else {
        ui.label(label);
        ui.add(egui::Label::new(value).wrap());
        ui.add_space(theme::SPACE_1);
    }
}

/// Drei Zahlenfelder x, y, z unter der Bezeichnung `label`, nebeneinander in der Breite des
/// Felds. Passt eine Zahl nicht in ihr Drittel, stehen die drei Felder im nächsten Bild
/// untereinander (mit x, y, z davor), damit keine Zahl abgeschnitten wird; während des Ziehens
/// bleibt die Anordnung. `key` unterscheidet die Gruppen. Liefert, ob gerade gezogen wird.
fn xyz_fields(
    ui: &mut egui::Ui,
    key: &'static str,
    label: &str,
    values: [&mut f64; 3],
    speed: f64,
) -> bool {
    ui.label(label);
    let stacked_key = egui::Id::new(("planora-xyz-stacked", key));
    let stacked = ui
        .data(|d| d.get_temp::<bool>(stacked_key))
        .unwrap_or(false);
    let gap = theme::SPACE_1;
    let w = ((ui.available_width() - 2.0 * gap) / 3.0).floor();
    let mut dragging = false;
    let mut too_wide = false;
    // Feste Id je Achse: der Zustand eines Felds (Ziehen, Eingabe) hängt nicht an der Anordnung.
    let mut one = |ui: &mut egui::Ui, axis: usize, v: &mut f64| {
        let r = ui
            .scope_builder(
                egui::UiBuilder::new().id(egui::Id::new(("planora-xyz", key, axis))),
                |ui| {
                    ui.spacing_mut().interact_size.x = w;
                    ui.spacing_mut().button_padding.x = theme::SPACE_1;
                    theme::field(ui, egui::DragValue::new(v).speed(speed))
                },
            )
            .inner;
        dragging |= r.dragged();
        too_wide |= r.rect.width() > w + 0.5;
    };
    if stacked {
        for (axis, (name, v)) in ["x", "y", "z"].into_iter().zip(values).enumerate() {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = gap;
                ui.weak(name);
                one(ui, axis, v);
            });
        }
    } else {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = gap;
            for (axis, v) in values.into_iter().enumerate() {
                one(ui, axis, v);
            }
        });
    }
    if !dragging && too_wide != stacked {
        ui.data_mut(|d| d.insert_temp(stacked_key, too_wide));
        crate::shell_panels::discard(ui.ctx(), "Eigenschaften: Zahlenfelder neu angeordnet");
    }
    dragging
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
pub fn parameter_rows(state: &AppState, kind: &FeatureKind) -> Vec<(String, String)> {
    let doc = &state.document;
    let feature_name = |id: FeatureId| {
        doc.feature(id)
            .map_or_else(|| tr!("state-feature-n", n = id.0), |f| f.name.clone())
    };
    match kind {
        FeatureKind::Sketch(s) => {
            let plane = match &s.plane {
                SketchPlane::Standard(StandardPlane::XY) => tr!("tree-plane-xy"),
                SketchPlane::Standard(StandardPlane::XZ) => tr!("tree-plane-xz"),
                SketchPlane::Standard(StandardPlane::YZ) => tr!("tree-plane-yz"),
                SketchPlane::OnFace(r) => tr!(
                    "props-on-face",
                    role = planora_kernel::blend::topo::role_label(&r.role),
                    name = feature_name(FeatureId(r.origin))
                ),
            };
            vec![
                (tr!("props-plane"), plane),
                (tr!("props-curves"), s.sketch.curves().count().to_string()),
                (tr!("props-edge-copies"), s.copies.len().to_string()),
            ]
        }
        FeatureKind::Box { size, origin } => vec![
            (
                tr!("props-size"),
                format!(
                    "{} × {} × {} mm",
                    format_number(size.x, 2),
                    format_number(size.y, 2),
                    format_number(size.z, 2)
                ),
            ),
            (tr!("props-position"), triple(*origin)),
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
            (tr!("props-radius"), mm(*radius)),
            (tr!("props-height"), mm(*height)),
            (tr!("props-position"), triple(*origin)),
        ],
        FeatureKind::Sphere { radius, origin } => {
            vec![
                (tr!("props-radius"), mm(*radius)),
                (tr!("props-position"), triple(*origin)),
            ]
        }
        FeatureKind::Extrude {
            sketch,
            profile,
            height,
        } => vec![
            (tr!("props-sketch"), feature_name(*sketch)),
            (tr!("props-profile"), (profile + 1).to_string()),
            (tr!("props-height"), mm(*height)),
        ],
        FeatureKind::Revolve {
            sketch,
            profile,
            axis,
        } => vec![
            (tr!("props-sketch"), feature_name(*sketch)),
            (tr!("props-profile"), (profile + 1).to_string()),
            (
                tr!("props-axis"),
                tr!("props-curve-n", n = axis.index() + 1),
            ),
        ],
        FeatureKind::Boolean { target, tool, op } => vec![
            (tr!("edit-target"), state.body_ref_name(*target)),
            (tr!("edit-tool"), state.body_ref_name(*tool)),
            (
                tr!("props-operation"),
                match op {
                    BoolOp::Union => tr!("chip-union"),
                    BoolOp::Difference => tr!("chip-subtract"),
                    BoolOp::Intersection => tr!("chip-intersect"),
                },
            ),
        ],
        FeatureKind::Move { body, delta } => vec![
            (tr!("props-body"), state.body_ref_name(*body)),
            (tr!("props-displacement"), triple(*delta)),
        ],
        FeatureKind::Verrunden {
            body,
            edges,
            art,
            tangentenkette,
        } => {
            let mut rows = vec![
                (tr!("props-body"), state.body_ref_name(*body)),
                (tr!("props-edges"), edges.len().to_string()),
                (tr!("props-kind"), art.kind_label().to_string()),
            ];
            match art {
                BlendArt::Rundung { radius } => rows.push((tr!("props-radius"), mm(*radius))),
                BlendArt::RundungVariabel { punkte } => {
                    rows.push((tr!("props-points"), punkte.len().to_string()))
                }
                BlendArt::FaseGleich { abstand } => {
                    rows.push((tr!("blend-field-distance"), mm(*abstand)))
                }
                BlendArt::FaseZwei { abstand1, abstand2 } => {
                    rows.push((tr!("blend-field-distance-1"), mm(*abstand1)));
                    rows.push((tr!("blend-field-distance-2"), mm(*abstand2)));
                }
                BlendArt::FaseWinkel { abstand, winkel } => {
                    rows.push((tr!("blend-field-distance"), mm(*abstand)));
                    rows.push((
                        tr!("blend-field-angle"),
                        format!("{} °", format_number(*winkel, 2)),
                    ));
                }
            }
            rows.push((
                tr!("props-tangent-chain"),
                if *tangentenkette {
                    tr!("props-on")
                } else {
                    tr!("props-off")
                },
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
                (tr!("props-mesh"), tr!("props-triangles", n = triangles)),
                (tr!("props-position"), triple(*offset)),
                (
                    tr!("props-rotation"),
                    format!(
                        "{} / {} / {} °",
                        format_number(d.x, 2),
                        format_number(d.y, 2),
                        format_number(d.z, 2)
                    ),
                ),
                (tr!("props-scale"), format_number(*scale, 4)),
            ]
        }
        // Keine Zahlenfelder (P6): nur Angaben zur Form aus der Datei.
        FeatureKind::StepBody { shape } => doc
            .step_shape(*shape)
            .map_or_else(Vec::new, |s| step_rows(state, s, tr!("props-faces"))),
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
    ui.label(tr!("props-name"));
    let renamed = name_field(ui, Some(id), &name);
    ui.add_space(6.0);
    row(ui, &tr!("props-kind"), theme::value(label));
    let weak = |text: String| egui::RichText::new(text).weak();
    let status_text = match &status {
        FeatureStatus::Ok => theme::value(tr!("props-status-ok")),
        FeatureStatus::Error(e) => egui::RichText::new(tr!("props-status-error", error = e))
            .color(error_color(ui.visuals().dark_mode)),
        FeatureStatus::Skipped(why) => weak(tr!("props-status-skipped", why = why)),
        FeatureStatus::Suppressed => weak(tr!("tree-status-suppressed")),
        FeatureStatus::NotComputed => weak(tr!("props-status-not-computed")),
    };
    row(ui, &tr!("props-status"), status_text);
    for (k, v) in &rows {
        row(ui, k, theme::value(v));
    }
    if let Some(shape) = state.document.step_body(id).cloned() {
        step_warning(ui, &shape);
    }
    ui.add_space(8.0);
    let edit = ui
        .button(if sketch {
            tr!("props-open-sketch")
        } else {
            tr!("props-edit")
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

/// Zeilen „Volumen“ und „Oberfläche“.
fn mass_rows(ui: &mut egui::Ui, mass: &planora_mesh::MassProperties) {
    row(
        ui,
        &tr!("props-volume"),
        theme::value(format!("≈ {:.2} mm³", mass.volume)),
    );
    row(
        ui,
        &tr!("props-surface"),
        theme::value(format!("≈ {:.2} mm²", mass.surface_area)),
    );
}

/// Maße des gewählten Körpers sowie Angaben zur gewählten Fläche oder Kante. Gelöscht wird
/// über das Feature im Feature-Baum.
fn body(ui: &mut egui::Ui, state: &mut AppState, index: usize) {
    if state.is_mesh(index) {
        mesh_body(ui, state, index);
        return;
    }
    let mut moved: Option<DVec3> = None;
    let dragging;
    let renamed;
    {
        let body = &state.bodies[index];
        ui.label(tr!("props-name"));
        // Der Name gehört dem erzeugenden Feature.
        renamed = name_field(ui, state.naming_feature(index), &body.name);
        ui.add_space(6.0);
        // STEP-Körper (auch verschoben): Art, Angaben zur Form und bei grober Datei die Warnung.
        let step = (body.kind == BodyKind::Step)
            .then(|| state.naming_feature(index))
            .flatten()
            .and_then(|f| state.document.step_body(f));
        if let Some(shape) = step {
            row(ui, &tr!("props-kind"), theme::value(step_label()));
            // Der Körper kann seit dem Import gerundet oder verrechnet sein: Die Zahl der Form
            // aus der Datei heißt hier ausdrücklich so, „Flächen / Kanten“ unten zählt den
            // Körper, wie er jetzt ist.
            for (k, v) in step_rows(state, shape, tr!("props-faces-in-file")) {
                row(ui, &k, theme::value(v));
            }
            step_warning(ui, shape);
        }
        let (min, max) = body.bounds;
        let size = max - min;
        mass_rows(ui, &body.mass);
        row(
            ui,
            &tr!("props-centroid"),
            theme::value(format!(
                "{:.2} / {:.2} / {:.2} mm",
                body.mass.centroid.x, body.mass.centroid.y, body.mass.centroid.z
            )),
        );
        row(
            ui,
            &tr!("props-dimensions"),
            theme::value(format!("{:.2} × {:.2} × {:.2} mm", size.x, size.y, size.z)),
        );
        let (faces, edges) = state.kernel.counts(&body.solid);
        row(
            ui,
            &tr!("props-faces-edges"),
            theme::value(format!("{faces} / {edges}")),
        );
        ui.add_space(theme::SPACE_1);
        // Lage der unteren Ecke der Hülle; Änderungen verschieben den Körper.
        let mut position = min;
        // Verschieben ist eine neue Aktion: bei einem zurückgesetzten Körper gesperrt.
        let lock = state.body_lock(index);
        let fields = ui.add_enabled_ui(lock.is_none(), |ui| {
            xyz_fields(
                ui,
                "body-position",
                &position_label(),
                [&mut position.x, &mut position.y, &mut position.z],
                0.5,
            )
        });
        if let Some(why) = lock {
            fields.response.on_disabled_hover_text(why);
        }
        dragging = fields.inner;
        if position != min {
            moved = Some(position - min);
        }
    }

    ui.separator();
    match state.selection {
        Some(Pick::Face(f)) if f.body == index => {
            match state.kernel.face_kind(&state.bodies[index].solid, f.face) {
                Some(kind) => {
                    ui.label(theme::value(tr!(
                        "props-selected-face",
                        kind = kind.label()
                    )));
                }
                None => {
                    ui.weak(tr!("props-click-face-or-edge"));
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
                    ui.label(theme::value(tr!(
                        "props-selected-edge",
                        kind = kind.label(),
                        length = format!("{length:.2}")
                    )));
                }
                None => {
                    ui.weak(tr!("props-click-face-or-edge"));
                }
            }
        }
        _ => {
            ui.weak(tr!("props-click-face-or-edge"));
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
    let mut degrees = shown_degrees;
    let Some(fid) = state.bodies.get(index).map(|b| b.source.feature) else {
        return;
    };
    let renamed;
    let mut commit: Option<(FieldKey, String)>;
    {
        let body = &state.bodies[index];
        ui.label(tr!("props-name"));
        renamed = name_field(ui, state.naming_feature(index), &body.name);
        ui.add_space(6.0);
        let size = body.bounds.1 - body.bounds.0;
        row(ui, &tr!("props-kind"), theme::value(tr!("props-mesh-body")));
        mass_rows(ui, &body.mass);
        row(
            ui,
            &tr!("props-dimensions"),
            theme::value(format!("{:.2} × {:.2} × {:.2} mm", size.x, size.y, size.z)),
        );
        if let Some(r) = body.mesh_report {
            row(
                ui,
                &tr!("props-triangles-label"),
                theme::value(r.triangles.to_string()),
            );
            row(
                ui,
                &closed_at_import_label(),
                theme::value(if r.closed {
                    tr!("props-yes")
                } else {
                    tr!("props-no")
                }),
            );
        }
        ui.add_space(theme::SPACE_1);
        // Position und Maßstab sind Formelfelder (P10, Tippen statt Ziehen), die Drehung bleibt
        // ein Ziehfeld.
        let fields = mesh_fields(state, index);
        commit = mesh_formula_rows(ui, state, ("mesh-position", fid), &fields[..3]);
        xyz_fields(
            ui,
            "mesh-rotation",
            &rotation_label(),
            [&mut degrees.x, &mut degrees.y, &mut degrees.z],
            1.0,
        );
        commit = commit.or(mesh_formula_rows(
            ui,
            state,
            ("mesh-scale", fid),
            &fields[3..],
        ));
        if let Some(r) = body.mesh_report
            && !r.closed
        {
            ui.colored_label(warning_color(ui.visuals().dark_mode), import_warning(&r));
        }
        ui.add_space(4.0);
        ui.weak(mesh_body_hint());
    }
    if let (Some(name), Some(id)) = (renamed, state.naming_feature(index)) {
        let _ = state.rename_feature(id, &name);
    }
    // Die Drehung wird nur neu gebildet, wenn ein Winkel geändert wurde (sonst bliebe sie durch
    // das Hin- und Zurückrechnen nicht bitgleich).
    if degrees != shown_degrees {
        let new = MeshTransform {
            rotation: rotation_from_degrees(degrees),
            ..t
        };
        let _ = state.set_mesh_transform(index, new);
    }
    // Ein Formelfeld wirkt bei Enter oder beim Verlassen (P9): eine Zahl setzt den Wert (die
    // Formel dieses Feldes entfällt), sonst wird der Text die Formel des Feldes. Abgelehnt:
    // Text und Meldung bleiben im Feld.
    if let Some((key, text)) = commit {
        let id = mesh_draft_id(fid, key);
        match state.set_mesh_formula(index, key, &text) {
            Ok(()) => ui.data_mut(|d| d.remove::<MeshDraft>(id)),
            Err(e) => {
                let draft = MeshDraft {
                    stand: stand(state),
                    text,
                    error: Some(e),
                };
                ui.data_mut(|d| d.insert_temp(id, draft));
            }
        }
    }
}

/// Entwurf eines Formelfelds des Netz-Körpers im egui-Speicher: der getippte Text, solange er
/// nicht übernommen ist, und die Meldung einer abgelehnten Übernahme. Ändert sich das Dokument
/// (Parameter, Rückgängig, andere Änderung), gilt der Entwurf nicht mehr: Das Feld zeigt wieder
/// Formel bzw. Wert aus dem Dokument (wie die Zeilen des Fensters „Parameter“).
#[derive(Clone, Debug)]
struct MeshDraft {
    stand: (u64, u64),
    text: String,
    error: Option<String>,
}

/// Stand des Dokuments, zu dem ein Entwurf gehört.
fn stand(state: &AppState) -> (u64, u64) {
    (state.instance, state.document.edit_count())
}

fn mesh_draft_id(fid: FeatureId, key: FieldKey) -> egui::Id {
    egui::Id::new(("mesh-formula", fid, key))
}

/// Ein Formelfeld des Netz-Körpers: Feld, Bezeichnung, Einheit und gespeicherter Text (Formel,
/// sonst der Wert).
struct MeshField {
    key: FieldKey,
    label: String,
    unit: ParamUnit,
    saved: String,
}

/// Die vier Formelfelder des Netz-Körpers `index` (Position x/y/z, Maßstab).
fn mesh_fields(state: &AppState, index: usize) -> Vec<MeshField> {
    let f = state
        .bodies
        .get(index)
        .and_then(|b| state.document.feature(b.source.feature));
    [
        (FieldKey::X, tr!("props-position-x")),
        (FieldKey::Y, tr!("props-position-y")),
        (FieldKey::Z, tr!("props-position-z")),
        (FieldKey::Massstab, tr!("props-scale")),
    ]
    .into_iter()
    .map(|(key, label)| {
        let saved = f
            .and_then(|f| {
                f.formulas()
                    .get(&key)
                    .cloned()
                    .or_else(|| field_value(&f.kind, key).map(field_text))
            })
            .unwrap_or_default();
        MeshField {
            key,
            label,
            unit: f.map_or(ParamUnit::Length, |f| field_unit(&f.kind, key)),
            saved,
        }
    })
    .collect()
}

/// Zahl für ein Formelfeld, das aus einem Zahlenwert vorbelegt wird: verlustfrei, solange sie
/// kurz ist, sonst auf vier Stellen gerundet (Nullen am Ende weg), mit dem Dezimaltrenner der
/// Sprache (wie die Vorgaben der Skizzenfelder; gelesen werden Komma und Punkt). Eine
/// gespeicherte Formel zeigt das Feld dagegen, wie sie getippt wurde. Winzige negative Werte
/// ergeben „0“, nie „-0“. Bleibt der Text unverändert, wird nichts übernommen.
fn field_text(v: f64) -> String {
    let exact = localize_decimal(&number_text(v).replace(',', "."));
    if exact.chars().count() <= 12 {
        return exact;
    }
    // `fmt_num` schreibt kein „-0,0000“. Die Nullen am Ende fallen weg, mit dem Trenner.
    let rounded = fmt_num(v, 4);
    let sep = planora_i18n::current().decimal();
    let rounded = rounded.trim_end_matches('0').trim_end_matches(sep);
    if rounded.is_empty() || rounded == "-" {
        "0".into()
    } else {
        rounded.to_string()
    }
}

/// Zeilen „Bezeichnung – Formelfeld“; liefert eine Übernahme (Enter oder Verlassen nach einer
/// Änderung) mit dem Text, wenn er gültig ist und vom gespeicherten Text abweicht.
fn mesh_formula_rows(
    ui: &mut egui::Ui,
    state: &AppState,
    grid: (&'static str, FeatureId),
    fields: &[MeshField],
) -> Option<(FieldKey, String)> {
    let scope = state.document.param_scope();
    let now = stand(state);
    let fid = grid.1;
    let mut commit = None;
    egui::Grid::new(grid).num_columns(2).show(ui, |ui| {
        for f in fields {
            let id = mesh_draft_id(fid, f.key);
            let draft = ui
                .data(|d| d.get_temp::<MeshDraft>(id))
                .filter(|d| d.stand == now);
            let mut text = draft
                .as_ref()
                .map_or_else(|| f.saved.clone(), |d| d.text.clone());
            let mut error = draft.and_then(|d| d.error);
            ui.label(&f.label);
            let out = ui
                .vertical(|ui| {
                    let out = FormulaField::new(("mesh-field", fid, f.key), &mut text, f.unit)
                        .width(80.0)
                        .label(&f.label)
                        .saved(&f.saved)
                        .show(ui, &scope);
                    if let Some(e) = &error
                        && out.error.is_none()
                    {
                        ui.colored_label(theme::palette(ui.visuals().dark_mode).error, e);
                    }
                    out
                })
                .inner;
            if out.response.changed() {
                error = None;
            }
            if out.committed && out.error.is_none() && text.trim() != f.saved {
                commit = Some((f.key, text.clone()));
            }
            if text == f.saved && error.is_none() {
                ui.data_mut(|d| d.remove::<MeshDraft>(id));
            } else {
                let draft = MeshDraft {
                    stand: now,
                    text,
                    error,
                };
                ui.data_mut(|d| d.insert_temp(id, draft));
            }
            ui.end_row();
        }
    });
    commit
}

#[cfg(test)]
mod tests {
    use super::{closed_at_import_label, import_warning, mesh_body_hint};
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
        assert!(closed_at_import_label().contains("beim Einlesen"));
    }

    #[test]
    fn the_mesh_body_hint_has_no_runs_of_spaces() {
        let hint = mesh_body_hint();
        assert!(!hint.contains("  "), "{hint:?}");
        assert!(hint.contains("exportieren, aber nicht"));
    }
}
