use crate::create_dialog::CreateDialog;
use crate::samples::{ExtrudeSample, RevolveSample};
use crate::state::AppState;
use crate::{browser, properties, theme, viewport};
use egui_dock::{DockArea, DockState, NodeIndex, TabViewer};
use planora_render::ViewFace;
use planora_render::gpu::GpuScene;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Tab {
    Browser,
    Viewport,
    Properties,
}

struct Viewer<'a> {
    state: &'a mut AppState,
}

impl TabViewer for Viewer<'_> {
    type Tab = Tab;

    fn id(&mut self, tab: &mut Tab) -> egui::Id {
        egui::Id::new(*tab)
    }

    fn title(&mut self, tab: &mut Tab) -> egui::WidgetText {
        match tab {
            Tab::Browser => "Browser",
            Tab::Viewport => "3D-Ansicht",
            Tab::Properties => "Eigenschaften",
        }
        .into()
    }

    fn ui(&mut self, ui: &mut egui::Ui, tab: &mut Tab) {
        match tab {
            Tab::Browser => browser::show(ui, self.state),
            Tab::Viewport => viewport::show(ui, self.state),
            Tab::Properties => properties::show(ui, self.state),
        }
    }

    fn is_closeable(&self, _tab: &Tab) -> bool {
        false
    }

    fn scroll_bars(&self, tab: &Tab) -> [bool; 2] {
        // Der Viewport füllt den Platz selbst aus und braucht keine Rollbalken.
        if *tab == Tab::Viewport {
            [false, false]
        } else {
            [false, true]
        }
    }
}

pub struct PlanoraApp {
    state: AppState,
    dock: DockState<Tab>,
    dialog: Option<CreateDialog>,
    applied_dark: Option<bool>,
}

impl PlanoraApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let render_state = cc
            .wgpu_render_state
            .as_ref()
            .expect("PlanoraCAD benötigt den wgpu-Renderer");
        GpuScene::install(render_state);

        let mut dock = DockState::new(vec![Tab::Viewport]);
        let tree = dock.main_surface_mut();
        let [center, _left] = tree.split_left(NodeIndex::root(), 0.2, vec![Tab::Browser]);
        let [_center, _right] = tree.split_right(center, 0.75, vec![Tab::Properties]);

        let mut state = AppState::new();
        state.frame_all();
        Self {
            state,
            dock,
            dialog: None,
            applied_dark: None,
        }
    }

    fn menu(&mut self, ui: &mut egui::Ui) {
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button("Datei", |ui| {
                if ui.button("Beenden").clicked() {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });
            ui.menu_button("Erzeugen", |ui| {
                let mut open = |dialog: CreateDialog, ui: &mut egui::Ui| {
                    self.dialog = Some(dialog);
                    ui.close();
                };
                if ui.button("Quader …").clicked() {
                    open(
                        CreateDialog::Box {
                            x: 40.0,
                            y: 30.0,
                            z: 20.0,
                        },
                        ui,
                    );
                }
                if ui.button("Zylinder …").clicked() {
                    open(
                        CreateDialog::Cylinder {
                            radius: 15.0,
                            height: 30.0,
                        },
                        ui,
                    );
                }
                if ui.button("Kugel …").clicked() {
                    open(CreateDialog::Sphere { radius: 20.0 }, ui);
                }
                if ui.button("Kegel …").clicked() {
                    open(
                        CreateDialog::Cone {
                            radius: 20.0,
                            height: 40.0,
                        },
                        ui,
                    );
                }
                ui.separator();
                if ui.button("Extrusion …").clicked() {
                    let sample = ExtrudeSample::PlateWithHole;
                    open(
                        CreateDialog::Extrude {
                            sample,
                            height: sample.default_height(),
                        },
                        ui,
                    );
                }
                if ui.button("Rotation …").clicked() {
                    open(
                        CreateDialog::Revolve {
                            sample: RevolveSample::Vase,
                        },
                        ui,
                    );
                }
            });
            ui.menu_button("Ansicht", |ui| {
                for face in ViewFace::ALL {
                    if ui.button(face.label()).clicked() {
                        self.state.camera.set_view(face);
                        ui.close();
                    }
                }
                if ui.button("Isometrisch").clicked() {
                    self.state.camera.set_iso_view();
                    ui.close();
                }
                ui.separator();
                if ui.button("Alles einpassen").clicked() {
                    self.state.frame_all();
                    ui.close();
                }
                ui.separator();
                let label = if self.state.dark {
                    "Helles Design"
                } else {
                    "Dunkles Design"
                };
                if ui.button(label).clicked() {
                    self.state.dark = !self.state.dark;
                    self.state.rebuild_scene();
                    ui.close();
                }
            });
        });
    }
}

impl eframe::App for PlanoraApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        if self.applied_dark != Some(self.state.dark) {
            theme::apply(ui.ctx(), self.state.dark);
            self.applied_dark = Some(self.state.dark);
        }

        egui::Panel::top("menu").show(ui, |ui| self.menu(ui));
        egui::Panel::bottom("status").show(ui, |ui| {
            ui.horizontal(|ui| {
                match &self.state.message {
                    Some(m) => ui.colored_label(egui::Color32::from_rgb(230, 90, 90), m),
                    None => ui.weak(format!("{} Körper", self.state.bodies.len())),
                };
            });
        });
        egui::CentralPanel::default().show(ui, |ui| {
            let mut viewer = Viewer {
                state: &mut self.state,
            };
            DockArea::new(&mut self.dock).show_inside(ui, &mut viewer);
        });

        if let Some(mut dialog) = self.dialog.take()
            && dialog.show(ui.ctx(), &mut self.state)
        {
            self.dialog = Some(dialog);
        }
        viewport::remember_keyboard_focus(ui.ctx());
    }
}
