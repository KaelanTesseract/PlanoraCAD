use crate::boolean_dialog::BooleanDialog;
use crate::create_dialog::CreateDialog;
use crate::samples::{ExtrudeSample, RevolveSample};
use crate::state::{AppState, SketchRequest, StandardPlane};
use crate::{browser, fonts, properties, sketch_panel, theme, viewport};
use egui_tiles::{Container, SimplificationOptions, TileId, Tiles, Tree, UiResponse};
use planora_kernel::BoolOp;
use planora_render::gpu::GpuScene;
use planora_render::{FaceRef, Pick, ViewFace};
use planora_sketch::plane_of_face;

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
                    .show(ui, |ui| {
                        egui::Frame::NONE
                            .inner_margin(egui::Margin::same(8))
                            .show(ui, |ui| browser::show(ui, self.state));
                    });
            }
            Pane::Properties => {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        egui::Frame::NONE
                            .inner_margin(egui::Margin::same(8))
                            .show(ui, |ui| properties::show(ui, self.state));
                    });
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
    boolean_dialog: Option<BooleanDialog>,
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
            boolean_dialog: None,
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
                // Körper entstehen außerhalb des Skizzen-Modus: eine offene Skizze wird vorher
                // beendet.
                let mut open = |dialog: CreateDialog, ui: &mut egui::Ui| {
                    self.state.close_sketch();
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
                ui.separator();
                self.sketch_menu(ui);
                ui.separator();
                // Körper aus Skizzen: die gewählte (sonst die geöffnete, sonst die letzte) Skizze.
                let target = self
                    .state
                    .selected_sketch
                    .or(self.state.sketch_session.as_ref().map(|s| s.sketch))
                    .filter(|&i| i < self.state.sketches.len())
                    .or(self.state.sketches.len().checked_sub(1));
                for (label, revolve) in [("Skizze extrudieren …", false), ("Skizze drehen …", true)]
                {
                    if ui
                        .add_enabled(target.is_some(), egui::Button::new(label))
                        .on_disabled_hover_text("Zuerst eine Skizze anlegen.")
                        .clicked()
                        && let Some(i) = target
                    {
                        self.state.close_sketch();
                        self.dialog = Some(if revolve {
                            CreateDialog::revolve_sketch(&self.state, i)
                        } else {
                            CreateDialog::extrude_sketch(i)
                        });
                        ui.close();
                    }
                }
            });
            ui.menu_button("Bearbeiten", |ui| {
                // Boolesche Operationen brauchen mindestens zwei Körper.
                let enabled = self.state.bodies.len() >= 2;
                for (label, op) in [
                    ("Vereinigen …", BoolOp::Union),
                    ("Abziehen …", BoolOp::Difference),
                    ("Schneiden …", BoolOp::Intersection),
                ] {
                    if ui
                        .add_enabled(enabled, egui::Button::new(label))
                        .on_disabled_hover_text("Dafür werden mindestens zwei Körper benötigt.")
                        .clicked()
                    {
                        self.state.close_sketch();
                        self.boolean_dialog = Some(BooleanDialog::with_op(&self.state, op));
                        ui.close();
                    }
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

impl PlanoraApp {
    /// Einträge „Skizze auf …“ im Menü „Erzeugen“ (ohne Untermenü).
    fn sketch_menu(&mut self, ui: &mut egui::Ui) {
        for (label, plane) in [
            ("Skizze auf XY-Ebene", StandardPlane::XY),
            ("Skizze auf XZ-Ebene", StandardPlane::XZ),
            ("Skizze auf YZ-Ebene", StandardPlane::YZ),
        ] {
            if ui.button(label).clicked() {
                let i = self.state.new_sketch(AppState::standard_frame(plane));
                self.state.selected_sketch = Some(i);
                let _ = self.state.open_sketch(i);
                ui.close();
            }
        }
        let face = match self.state.selection {
            Some(Pick::Face(FaceRef { body, face })) => self
                .state
                .bodies
                .get(body)
                .and_then(|b| plane_of_face(&b.solid, face))
                .map(|_| (body, face)),
            _ => None,
        };
        if ui
            .add_enabled(
                face.is_some(),
                egui::Button::new("Skizze auf gewählter Fläche"),
            )
            .on_disabled_hover_text("Dafür zuerst eine ebene Fläche eines Körpers anklicken.")
            .clicked()
            && let Some((body, face)) = face
        {
            if let Ok(i) = self.state.new_sketch_on_face(body, face) {
                self.state.selected_sketch = Some(i);
                let _ = self.state.open_sketch(i);
            }
            ui.close();
        }
    }
}

/// Körper- und Boolesche Dialoge gibt es nur außerhalb des Skizzen-Modus: Wird eine Skizze
/// geöffnet (Menü, Doppelklick im Browser), werden offene Dialoge verworfen.
pub fn drop_dialogs_in_sketch_mode(
    state: &AppState,
    create: &mut Option<CreateDialog>,
    boolean: &mut Option<BooleanDialog>,
) {
    if state.sketch_session.is_some() {
        *create = None;
        *boolean = None;
    }
}

/// Setzt einen Dialogwunsch aus dem Browser (Kontextmenü einer Skizze) in den passenden Dialog um
/// und löscht ihn. Der Browser hat die offene Skizze dann bereits beendet.
pub fn dialog_for_request(state: &mut AppState) -> Option<CreateDialog> {
    match state.sketch_request.take()? {
        SketchRequest::Extrude(i) => Some(CreateDialog::extrude_sketch(i)),
        SketchRequest::Revolve(i) => Some(CreateDialog::revolve_sketch(state, i)),
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
                let sketch = self
                    .state
                    .sketch_session
                    .as_ref()
                    .and_then(|s| self.state.sketches.get(s.sketch));
                match (&self.state.message, sketch) {
                    (Some(m), _) => ui.colored_label(egui::Color32::from_rgb(230, 90, 90), m),
                    (None, Some(sk)) => ui.weak(format!(
                        "Skizzen-Modus: {} – Esc bricht ab, Eingabe beendet, Entf löscht, mittlere Maustaste oder Umschalt + Ziehen verschiebt",
                        sk.name
                    )),
                    (None, None) => ui.weak(format!("{} Körper", self.state.bodies.len())),
                };
            });
        });
        egui::CentralPanel::default().show(ui, |ui| {
            let mut behavior = Behavior {
                state: &mut self.state,
            };
            self.dock.ui(&mut behavior, ui);
        });

        if let Some(dialog) = dialog_for_request(&mut self.state) {
            self.dialog = Some(dialog);
        }
        drop_dialogs_in_sketch_mode(&self.state, &mut self.dialog, &mut self.boolean_dialog);
        sketch_panel::show(ui.ctx(), &mut self.state);
        if let Some(mut dialog) = self.dialog.take()
            && dialog.show(ui.ctx(), &mut self.state)
        {
            self.dialog = Some(dialog);
        }
        if let Some(mut dialog) = self.boolean_dialog.take()
            && dialog.show(ui.ctx(), &mut self.state)
        {
            self.boolean_dialog = Some(dialog);
        }
        viewport::remember_keyboard_focus(ui.ctx());
    }
}
