use crate::sketch_view::{format_length, format_number};
use crate::state::AppState;
use planora_math::DVec3;
use planora_sketch::model::{CurveId, CurveKind, Sketch};

/// Voreingestellte Höhe für „Skizze extrudieren“ in Millimetern.
pub const DEFAULT_SKETCH_HEIGHT: f64 = 10.0;

/// Offener Erzeugen-Dialog samt Eingabewerten.
#[derive(Clone, Debug, PartialEq)]
pub enum CreateDialog {
    Box {
        x: f64,
        y: f64,
        z: f64,
    },
    Cylinder {
        radius: f64,
        height: f64,
    },
    Sphere {
        radius: f64,
    },
    Cone {
        radius: f64,
        height: f64,
    },
    /// Profil `profile` (Index in `Sketch::profiles()`) der Skizze `sketch` um `height` entlang
    /// der Skizzennormalen extrudieren.
    ExtrudeSketch {
        sketch: usize,
        profile: usize,
        height: f64,
    },
    /// Profil `profile` der Skizze `sketch` um die Linie `axis` der Skizze drehen (360°).
    RevolveSketch {
        sketch: usize,
        profile: usize,
        axis: Option<CurveId>,
    },
}

/// Linien der Skizze, die als Drehachse taugen: Konstruktionslinien zuerst, dann die übrigen,
/// jeweils in der Reihenfolge der Kurven. Dazu der Name für die Auswahlliste.
pub fn axis_candidates(sk: &Sketch) -> Vec<(CurveId, String)> {
    let mut lines: Vec<(CurveId, bool)> = sk
        .curves()
        .filter(|(_, c)| matches!(c.kind, CurveKind::Line { .. }))
        .map(|(id, c)| (id, c.construction))
        .collect();
    // stabil: innerhalb der Gruppen bleibt die Kurvenreihenfolge
    lines.sort_by_key(|&(_, construction)| !construction);
    let (mut n_construction, mut n_line) = (0, 0);
    lines
        .into_iter()
        .map(|(id, construction)| {
            let length = sk.line_length(id).map(format_length).unwrap_or_default();
            let name = if construction {
                n_construction += 1;
                format!("Konstruktionslinie {n_construction} ({length})")
            } else {
                n_line += 1;
                format!("Linie {n_line} ({length})")
            };
            (id, name)
        })
        .collect()
}

impl CreateDialog {
    /// Dialog „Skizze extrudieren“ für die Skizze `sketch` mit voreingestellten Werten.
    pub fn extrude_sketch(sketch: usize) -> Self {
        CreateDialog::ExtrudeSketch {
            sketch,
            profile: 0,
            height: DEFAULT_SKETCH_HEIGHT,
        }
    }

    /// Dialog „Skizze drehen“ für die Skizze `sketch`; als Achse ist die erste Konstruktionslinie
    /// (sonst die erste Linie) vorgewählt.
    pub fn revolve_sketch(state: &AppState, sketch: usize) -> Self {
        let axis = state
            .sketches
            .get(sketch)
            .and_then(|sk| axis_candidates(sk).first().map(|(id, _)| *id));
        CreateDialog::RevolveSketch {
            sketch,
            profile: 0,
            axis,
        }
    }

    pub fn title(&self) -> &'static str {
        match self {
            CreateDialog::Box { .. } => "Quader erzeugen",
            CreateDialog::Cylinder { .. } => "Zylinder erzeugen",
            CreateDialog::Sphere { .. } => "Kugel erzeugen",
            CreateDialog::Cone { .. } => "Kegel erzeugen",
            CreateDialog::ExtrudeSketch { .. } => "Skizze extrudieren",
            CreateDialog::RevolveSketch { .. } => "Skizze drehen",
        }
    }

    /// Körper aus einer Skizze: Bei einem Fehler bleibt der Dialog offen.
    fn is_sketch_dialog(&self) -> bool {
        matches!(
            self,
            CreateDialog::ExtrudeSketch { .. } | CreateDialog::RevolveSketch { .. }
        )
    }

    /// Auswahl der Skizze; liefert `true`, wenn sie gewechselt wurde.
    fn sketch_choice(ui: &mut egui::Ui, state: &AppState, sketch: &mut usize) -> bool {
        let before = *sketch;
        let current = state
            .sketches
            .get(*sketch)
            .map(|s| s.name.clone())
            .unwrap_or_else(|| "(keine)".into());
        egui::ComboBox::from_label("Skizze")
            .selected_text(current)
            .show_ui(ui, |ui| {
                for (i, sk) in state.sketches.iter().enumerate() {
                    ui.selectable_value(sketch, i, &sk.name);
                }
            });
        *sketch != before
    }

    /// Liste der Profile der Skizze zum Auswählen, darunter die nicht verwendbaren Schleifen
    /// mit Grund.
    fn profile_list(ui: &mut egui::Ui, state: &AppState, sketch: usize, profile: &mut usize) {
        let Some(sk) = state.sketches.get(sketch) else {
            ui.weak("Skizze nicht gefunden.");
            return;
        };
        // Zwischengespeichert je Stand der Skizze: neu berechnet nur nach einer Änderung.
        let extracted = state.profile_cache.get(sk);
        ui.label("Profil");
        if extracted.profiles.is_empty() {
            ui.weak("Die Skizze enthält kein geschlossenes Profil.");
        }
        for (i, p) in extracted.profiles.iter().enumerate() {
            let area = p.outer.signed_area().abs()
                - p.holes.iter().map(|h| h.signed_area().abs()).sum::<f64>();
            let holes = match p.holes.len() {
                0 => String::new(),
                1 => ", 1 Loch".into(),
                n => format!(", {n} Löcher"),
            };
            let label = format!("Profil {}: {} mm²{holes}", i + 1, format_number(area, 2));
            ui.selectable_value(profile, i, label);
        }
        for r in &extracted.rejected {
            ui.weak(format!("Nicht verwendbar: {}", r.reason));
        }
    }

    /// Auswahl der Drehachse (Linien der Skizze, Konstruktionslinien zuerst).
    fn axis_choice(ui: &mut egui::Ui, state: &AppState, sketch: usize, axis: &mut Option<CurveId>) {
        let candidates = state
            .sketches
            .get(sketch)
            .map(axis_candidates)
            .unwrap_or_default();
        if candidates.is_empty() {
            ui.weak("Die Skizze enthält keine Linie, die als Drehachse dienen kann.");
            return;
        }
        let current = candidates
            .iter()
            .find(|(id, _)| Some(*id) == *axis)
            .map(|(_, n)| n.clone())
            .unwrap_or_else(|| "(bitte wählen)".into());
        egui::ComboBox::from_label("Drehachse")
            .selected_text(current)
            .show_ui(ui, |ui| {
                for (id, name) in &candidates {
                    ui.selectable_value(axis, Some(*id), name);
                }
            });
    }

    fn drag(ui: &mut egui::Ui, label: &str, value: &mut f64) {
        ui.horizontal(|ui| {
            ui.label(label);
            ui.add(
                egui::DragValue::new(value)
                    .speed(0.5)
                    .range(0.001..=100_000.0)
                    .suffix(" mm"),
            );
        });
    }

    /// Zeigt den Dialog. Gibt `true` zurück, solange er offen bleiben soll.
    pub fn show(&mut self, ctx: &egui::Context, state: &mut AppState) -> bool {
        let mut open = true;
        let mut create = false;
        let mut cancel = false;
        egui::Window::new(self.title())
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .show(ctx, |ui| {
                match self {
                    CreateDialog::Box { x, y, z } => {
                        Self::drag(ui, "Breite (x)", x);
                        Self::drag(ui, "Tiefe (y)", y);
                        Self::drag(ui, "Höhe (z)", z);
                    }
                    CreateDialog::Cylinder { radius, height } => {
                        Self::drag(ui, "Radius", radius);
                        Self::drag(ui, "Höhe", height);
                    }
                    CreateDialog::Sphere { radius } => Self::drag(ui, "Radius", radius),
                    CreateDialog::Cone { radius, height } => {
                        Self::drag(ui, "Radius", radius);
                        Self::drag(ui, "Höhe", height);
                    }
                    CreateDialog::ExtrudeSketch {
                        sketch,
                        profile,
                        height,
                    } => {
                        if Self::sketch_choice(ui, state, sketch) {
                            *profile = 0;
                        }
                        Self::profile_list(ui, state, *sketch, profile);
                        ui.add_space(4.0);
                        Self::drag(ui, "Höhe", height);
                    }
                    CreateDialog::RevolveSketch {
                        sketch,
                        profile,
                        axis,
                    } => {
                        if Self::sketch_choice(ui, state, sketch) {
                            *profile = 0;
                            *axis = state
                                .sketches
                                .get(*sketch)
                                .and_then(|sk| axis_candidates(sk).first().map(|(id, _)| *id));
                        }
                        Self::profile_list(ui, state, *sketch, profile);
                        ui.add_space(4.0);
                        Self::axis_choice(ui, state, *sketch, axis);
                    }
                }
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui.button("Erzeugen").clicked() {
                        create = true;
                    }
                    if ui.button("Abbrechen").clicked() {
                        cancel = true;
                    }
                });
            });
        if create {
            let ok = self.create(state);
            // Körper aus Skizzen: bei einem Fehler (Meldung in der Statuszeile) offen bleiben.
            return self.is_sketch_dialog() && !ok;
        }
        open && !cancel
    }

    /// Erzeugt den Körper mit den aktuellen Werten als Feature in `state`. Liefert `true` bei
    /// Erfolg; sonst steht der Fehler in `state.message`.
    pub fn create(&self, state: &mut AppState) -> bool {
        state.message = None;
        let result = match self {
            CreateDialog::Box { x, y, z } => state.create_box(DVec3::new(*x, *y, *z)),
            CreateDialog::Cylinder { radius, height } => state.create_cylinder(*radius, *height),
            CreateDialog::Sphere { radius } => state.create_sphere(*radius),
            CreateDialog::Cone { radius, height } => state.create_cone(*radius, *height),
            // Körper aus Skizzen entstehen an Ort und Stelle; Fehler legt `AppState` ab.
            CreateDialog::ExtrudeSketch {
                sketch,
                profile,
                height,
            } => return state.extrude_sketch(*sketch, *profile, *height).is_ok(),
            CreateDialog::RevolveSketch {
                sketch,
                profile,
                axis,
            } => {
                let Some(axis) = axis else {
                    state.message =
                        Some("Bitte eine Drehachse wählen (eine Linie der Skizze).".into());
                    return false;
                };
                // Die Achse kann veraltet sein (Linie oder Skizze inzwischen gelöscht, Index
                // verschoben): nur Linien aus der Auswahlliste der aktuellen Skizze gelten.
                let valid = state
                    .sketches
                    .get(*sketch)
                    .is_some_and(|sk| axis_candidates(sk).iter().any(|(id, _)| id == axis));
                if !valid {
                    state.message = Some(
                        "Die gewählte Drehachse gibt es in dieser Skizze nicht mehr; bitte neu wählen."
                            .into(),
                    );
                    return false;
                }
                return state.revolve_sketch(*sketch, *profile, *axis).is_ok();
            }
        };
        // Ein Feature mit Fehler gilt nicht als Erfolg (die Meldung steht in `message`).
        result.is_ok() && state.message.is_none()
    }
}
