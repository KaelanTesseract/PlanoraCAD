use crate::samples::{ExtrudeSample, RevolveSample};
use crate::state::{AppState, BodyKind};
use planora_kernel::Kernel;
use planora_math::{DVec3, Frame};

/// Offener Erzeugen-Dialog samt Eingabewerten.
#[derive(Clone, Debug, PartialEq)]
pub enum CreateDialog {
    Box { x: f64, y: f64, z: f64 },
    Cylinder { radius: f64, height: f64 },
    Sphere { radius: f64 },
    Cone { radius: f64, height: f64 },
    Extrude { sample: ExtrudeSample, height: f64 },
    Revolve { sample: RevolveSample },
}

impl CreateDialog {
    pub fn title(&self) -> &'static str {
        match self {
            CreateDialog::Box { .. } => "Quader erzeugen",
            CreateDialog::Cylinder { .. } => "Zylinder erzeugen",
            CreateDialog::Sphere { .. } => "Kugel erzeugen",
            CreateDialog::Cone { .. } => "Kegel erzeugen",
            CreateDialog::Extrude { .. } => "Extrusion erzeugen",
            CreateDialog::Revolve { .. } => "Rotation erzeugen",
        }
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
                    CreateDialog::Extrude { sample, height } => {
                        ui.label("Beispielprofil");
                        for s in ExtrudeSample::ALL {
                            ui.selectable_value(sample, s, s.label());
                        }
                        Self::drag(ui, "Höhe", height);
                    }
                    CreateDialog::Revolve { sample } => {
                        ui.label("Beispielprofil");
                        for s in RevolveSample::ALL {
                            ui.selectable_value(sample, s, s.label());
                        }
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
            self.create(state);
            return false;
        }
        open && !cancel
    }

    fn create(&self, state: &mut AppState) {
        let k = state.kernel;
        let (kind, name, solid) = match self {
            CreateDialog::Box { x, y, z } => (
                BodyKind::Box,
                "Quader".to_string(),
                k.make_box(DVec3::new(*x, *y, *z)),
            ),
            CreateDialog::Cylinder { radius, height } => (
                BodyKind::Cylinder,
                "Zylinder".to_string(),
                k.make_cylinder(*radius, *height),
            ),
            CreateDialog::Sphere { radius } => (
                BodyKind::Sphere,
                "Kugel".to_string(),
                k.make_sphere(*radius),
            ),
            CreateDialog::Cone { radius, height } => (
                BodyKind::Cone,
                "Kegel".to_string(),
                k.make_cone(*radius, *height),
            ),
            CreateDialog::Extrude { sample, height } => (
                BodyKind::Extrusion,
                format!("Extrusion: {}", sample.label()),
                k.extrude(&sample.profile(), &Frame::WORLD, *height),
            ),
            CreateDialog::Revolve { sample } => (
                BodyKind::Revolution,
                format!("Rotation: {}", sample.label()),
                k.revolve(&sample.chain(), &Frame::WORLD),
            ),
        };
        state.message = None;
        match solid {
            Ok(solid) => {
                let number = state.bodies.len() + 1;
                if let Err(e) = state.add_body(kind, format!("{name} {number}"), solid) {
                    state.message = Some(e);
                }
            }
            Err(e) => state.message = Some(format!("Erzeugen fehlgeschlagen: {e}")),
        }
    }
}
