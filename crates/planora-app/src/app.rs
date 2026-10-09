use crate::create_dialog::CreateDialog;
use crate::samples::{ExtrudeSample, RevolveSample};
use crate::state::AppState;
use crate::{browser, fonts, properties, theme, viewport};
use egui_tiles::{Container, SimplificationOptions, TileId, Tiles, Tree, UiResponse};
use planora_render::ViewFace;
use planora_render::gpu::GpuScene;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Pane {
    Browser,
    Viewport,
    Properties,
}

struct Behavior<'a> {
    state: &'a mut AppState,
}

impl egui_tiles::Behavior<Pane> for Behavior<'_> {
    fn tab_title_for_pane(&mut self, pane: &Pane) -> egui::WidgetText {
        match pane {
            Pane::Browser => "Browser",
            Pane::Viewport => "3D-Ansicht",
            Pane::Properties => "Eigenschaften",
        }
        .into()
    }

    fn pane_ui(&mut self, ui: &mut egui::Ui, _tile_id: TileId, pane: &mut Pane) -> UiResponse {
        match pane {
            // Der Viewport füllt den Platz selbst aus und braucht keine Rollbalken.
            Pane::Viewport => viewport::show(ui, self.state),
            Pane::Browser => {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| browser::show(ui, self.state));
            }
            Pane::Properties => {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| properties::show(ui, self.state));
            }
        }
        UiResponse::None
    }

    fn is_tab_closable(&self, _tiles: &Tiles<Pane>, _tile_id: TileId) -> bool {
        false
    }

    fn simplification_options(&self) -> SimplificationOptions {
        // Jeder Bereich behält seine eigene Titelleiste, auch wenn er allein steht.
        SimplificationOptions {
            all_panes_must_have_tabs: true,
            prune_single_child_tabs: false,
            ..Default::default()
        }
    }
}

/// Anfangsanordnung: Browser links (20 %), 3D-Ansicht in der Mitte, Eigenschaften rechts (22 %).
fn build_docking() -> Tree<Pane> {
    let mut tiles = Tiles::default();
    let mut tab_of = |pane: Pane| {
        let id = tiles.insert_pane(pane);
        tiles.insert_tab_tile(vec![id])
    };
    let browser = tab_of(Pane::Browser);
    let viewport = tab_of(Pane::Viewport);
    let properties = tab_of(Pane::Properties);
    let root = tiles.insert_horizontal_tile(vec![browser, viewport, properties]);
    if let Some(egui_tiles::Tile::Container(Container::Linear(linear))) = tiles.get_mut(root) {
        // Anteile; Summe 3 (= Anzahl der Kinder): 20 %, 58 %, 22 %.
        linear.shares.set_share(browser, 0.6);
        linear.shares.set_share(viewport, 1.74);
        linear.shares.set_share(properties, 0.66);
    }
    Tree::new("planora_docking", root, tiles)
}

pub struct PlanoraApp {
    state: AppState,
    dock: Tree<Pane>,
    dialog: Option<CreateDialog>,
    applied_dark: Option<bool>,
}

impl PlanoraApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let render_state = cc
            .wgpu_render_state
            .as_ref()
            .expect("PlanoraCAD benötigt den wgpu-Renderer");
        cc.egui_ctx.set_fonts(fonts::definitions());
        GpuScene::install(render_state);

        let mut state = AppState::new();
        state.frame_all();
        Self {
            state,
            dock: build_docking(),
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
            let mut behavior = Behavior {
                state: &mut self.state,
            };
            self.dock.ui(&mut behavior, ui);
        });

        if let Some(mut dialog) = self.dialog.take()
            && dialog.show(ui.ctx(), &mut self.state)
        {
            self.dialog = Some(dialog);
        }
        viewport::remember_keyboard_focus(ui.ctx());
    }
}
