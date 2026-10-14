use crate::blend_dialog::BlendDialog;
use crate::boolean_dialog::BooleanDialog;
use crate::create_dialog::CreateDialog;
use crate::dock::{Dock, Pane};
use crate::export_dialog::ExportDialog;
use crate::feature_dialog::FeatureDialog;
use crate::file_chooser::default_dir;
use crate::import_dialog::ImportDialog;
use crate::projects_panel::ProjectsUi;
use crate::state::{AppState, SketchRequest, StandardPlane};
use crate::versions_panel::VersionsUi;
use crate::workspace::{Prompt, Workspace};
use crate::{
    browser, fonts, projects_panel, prompts, properties, sketch_panel, tabs, theme, versions_panel,
    viewport,
};
use egui_tiles::{SimplificationOptions, TileId, Tiles, UiResponse};
use planora_feature::SketchPlane;
use planora_kernel::BoolOp;
use planora_render::gpu::GpuScene;
use planora_render::{FaceRef, Pick, ViewFace};
use planora_sketch::plane_of_face;
use std::time::{Duration, Instant};

/// Wie oft die App ohne Eingaben neu zeichnet, damit die Autosicherung auch im Leerlauf läuft.
const AUTOSAVE_REPAINT: Duration = Duration::from_secs(30);

struct Behavior<'a> {
    ws: &'a mut Workspace,
    projects_ui: &'a mut ProjectsUi,
    versions_ui: &'a mut VersionsUi,
}

/// Rollbarer Bereich mit Innenabstand für die Panels.
fn scrolled(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui)) {
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            egui::Frame::NONE
                .inner_margin(egui::Margin::same(8))
                .show(ui, add_contents);
        });
}

impl egui_tiles::Behavior<Pane> for Behavior<'_> {
    fn tab_title_for_pane(&mut self, pane: &Pane) -> egui::WidgetText {
        match pane {
            Pane::Projects => "Projekte",
            Pane::Browser => "Browser",
            Pane::Viewport => "3D-Ansicht",
            Pane::Properties => "Eigenschaften",
            Pane::Versions => "Versionen",
        }
        .into()
    }

    fn pane_ui(&mut self, ui: &mut egui::Ui, _tile_id: TileId, pane: &mut Pane) -> UiResponse {
        // Schreibgeschützte Ansicht einer alten Version: Browser und Eigenschaften sind gesperrt.
        let enabled = !self.ws.active_tab().read_only;
        match pane {
            // Der Viewport füllt den Platz selbst aus und braucht keine Rollbalken.
            Pane::Viewport => viewport::show(ui, &mut self.ws.active_tab_mut().state),
            Pane::Browser => scrolled(ui, |ui| {
                let state = &mut self.ws.active_tab_mut().state;
                ui.add_enabled_ui(enabled, |ui| browser::show(ui, state));
            }),
            Pane::Properties => scrolled(ui, |ui| {
                let state = &mut self.ws.active_tab_mut().state;
                ui.add_enabled_ui(enabled, |ui| properties::show(ui, state));
            }),
            Pane::Projects => scrolled(ui, |ui| {
                projects_panel::show(ui, self.ws, self.projects_ui);
            }),
            Pane::Versions => scrolled(ui, |ui| {
                versions_panel::show(ui, self.ws, self.versions_ui);
            }),
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

pub struct PlanoraApp {
    ws: Workspace,
    dock: Dock,
    projects_ui: ProjectsUi,
    versions_ui: VersionsUi,
    applied_dark: Option<bool>,
    /// `ViewportCommand::Close` wurde nach freigegebenem Beenden schon geschickt.
    close_sent: bool,
}

impl PlanoraApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let render_state = cc
            .wgpu_render_state
            .as_ref()
            .expect("PlanoraCAD benötigt den wgpu-Renderer");
        cc.egui_ctx.set_fonts(fonts::definitions());
        GpuScene::install(render_state);

        let config = planora_project::config::Config::load();
        let root = config
            .root
            .clone()
            .unwrap_or_else(planora_project::config::Config::default_root);
        // Ist die Wurzel nicht nutzbar, startet die App ohne Bibliothek; Speichern fragt dann
        // zuerst nach einem Projektordner.
        let library = planora_project::library::Library::open(&root).ok();
        let ws = Workspace::new(library, config);
        Self {
            ws,
            dock: Dock::new(),
            projects_ui: ProjectsUi::default(),
            versions_ui: VersionsUi::default(),
            applied_dark: None,
            close_sent: false,
        }
    }

    /// Öffnen (Strg+O): blendet das Projekt-Panel ein; dort wird das Dokument gewählt. Der Baum
    /// wird dabei neu gelesen, damit er aktuell ist.
    fn show_projects(&mut self) {
        self.dock.set_visible(Pane::Projects, true);
        self.projects_ui.invalidate();
    }

    /// Strg+S / Speichern: fragt nach Kommentar (Dialog), Ort oder Zweig.
    fn begin_save(&mut self, as_new: bool) {
        let tab = self.ws.active_tab();
        if tab.read_only {
            return;
        }
        let (id, has_store, name) = (tab.id, tab.store.is_some(), tab.name.clone());
        self.ws.prompt = Some(if as_new || !has_store {
            Prompt::SaveAs {
                tab: id,
                then: None,
                parent: Vec::new(),
                name,
                comment: String::new(),
            }
        } else {
            Prompt::SaveComment {
                tab: id,
                then: None,
                comment: String::new(),
                branch: None,
            }
        });
    }

    /// Datei → Exportieren … (Strg+Umschalt+E): öffnet das Fenster, wenn es Körper gibt; eine
    /// offene Skizze wird vorher beendet. Auch in schreibgeschützten Ansichten möglich.
    fn open_export(&mut self) {
        let tab = self.ws.active_tab_mut();
        if tab.state.bodies.is_empty() || tab.state.try_close_sketch().is_err() {
            return;
        }
        tab.export_dialog = Some(ExportDialog::new(&tab.state));
    }

    /// Datei → Netz importieren …: öffnet das Fenster im zuletzt benutzten Ordner; eine offene
    /// Skizze wird vorher beendet. Nicht in schreibgeschützten Ansichten.
    fn open_import(&mut self) {
        let dir = self.ws.exchange_dir.clone().unwrap_or_else(default_dir);
        let tab = self.ws.active_tab_mut();
        if tab.read_only || tab.state.try_close_sketch().is_err() {
            return;
        }
        tab.import_dialog = Some(ImportDialog::new(dir));
    }

    fn file_menu(&mut self, ui: &mut egui::Ui) {
        let read_only = self.ws.active_tab().read_only;
        if ui.button("Neu              Strg+N").clicked() {
            self.ws.new_tab();
            ui.close();
        }
        if ui.button("Öffnen …         Strg+O").clicked() {
            self.show_projects();
            ui.close();
        }
        ui.separator();
        if ui
            .add_enabled(!read_only, egui::Button::new("Speichern       Strg+S"))
            .clicked()
        {
            self.begin_save(false);
            ui.close();
        }
        if ui
            .add_enabled(!read_only, egui::Button::new("Speichern unter …"))
            .clicked()
        {
            self.begin_save(true);
            ui.close();
        }
        ui.separator();
        let any = !self.ws.active_tab().state.bodies.is_empty();
        if ui
            .add_enabled(any, egui::Button::new("Exportieren …   Strg+Umschalt+E"))
            .on_disabled_hover_text("Es gibt noch keinen Körper.")
            .clicked()
        {
            self.open_export();
            ui.close();
        }
        if ui
            .add_enabled(!read_only, egui::Button::new("Netz importieren …"))
            .on_disabled_hover_text("In einer alten Version lässt sich nichts hinzufügen.")
            .clicked()
        {
            self.open_import();
            ui.close();
        }
        ui.separator();
        if ui.button("Schließen        Strg+W").clicked() {
            let i = self.ws.active;
            self.ws.request_close(i);
            ui.close();
        }
        if ui.button("Beenden").clicked() {
            // Ist Beenden freigegeben, schickt `handle_close_request` im nächsten Bild `Close`.
            self.ws.request_quit();
            ui.close();
        }
    }

    fn menu(&mut self, ui: &mut egui::Ui) {
        let read_only = self.ws.active_tab().read_only;
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button("Datei", |ui| self.file_menu(ui));
            ui.add_enabled_ui(!read_only, |ui| {
                ui.menu_button("Erzeugen", |ui| self.create_menu(ui));
                ui.menu_button("Bearbeiten", |ui| self.edit_menu(ui));
            });
            ui.menu_button("Ansicht", |ui| self.view_menu(ui));
        });
    }

    fn create_menu(&mut self, ui: &mut egui::Ui) {
        // Körper entstehen außerhalb des Skizzen-Modus: eine offene Skizze wird vorher beendet.
        // Wird der Entwurf abgelehnt, bleibt die Skizze offen und der Dialog entfällt.
        let tab = self.ws.active_tab_mut();
        let mut open = |dialog: CreateDialog, ui: &mut egui::Ui| {
            if tab.state.try_close_sketch().is_ok() {
                tab.dialog = Some(dialog);
            }
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
        self.sketch_menu(ui);
        ui.separator();
        // Körper aus Skizzen: die gewählte (sonst die geöffnete, sonst die letzte) Skizze.
        let tab = self.ws.active_tab_mut();
        let sketches = tab.state.sketch_features();
        let target = tab
            .state
            .selected_feature
            .or(tab.state.open_sketch_id())
            .filter(|id| sketches.iter().any(|(s, _)| s == id))
            .or(sketches.last().map(|(id, _)| *id));
        for (label, revolve) in [("Skizze extrudieren …", false), ("Skizze drehen …", true)] {
            if ui
                .add_enabled(target.is_some(), egui::Button::new(label))
                .on_disabled_hover_text("Zuerst eine Skizze anlegen.")
                .clicked()
                && let Some(i) = target
            {
                if tab.state.try_close_sketch().is_ok() {
                    tab.dialog = Some(if revolve {
                        CreateDialog::revolve_sketch(&tab.state, i)
                    } else {
                        CreateDialog::extrude_sketch(i)
                    });
                }
                ui.close();
            }
        }
    }

    fn edit_menu(&mut self, ui: &mut egui::Ui) {
        let tab = self.ws.active_tab_mut();
        // Boolesche Operationen brauchen zwei gewöhnliche Körper; Netz-Körper sind gesperrt.
        let boolean = tab.state.boolean_blocked();
        for (label, op) in [
            ("Vereinigen …", BoolOp::Union),
            ("Abziehen …", BoolOp::Difference),
            ("Schneiden …", BoolOp::Intersection),
        ] {
            if ui
                .add_enabled(boolean.is_none(), egui::Button::new(label))
                .on_disabled_hover_text(boolean.unwrap_or_default())
                .clicked()
            {
                if tab.state.try_close_sketch().is_ok() {
                    tab.boolean_dialog = Some(BooleanDialog::with_op(&tab.state, op));
                }
                ui.close();
            }
        }
        ui.separator();
        let blend = tab.state.blend_blocked();
        for (label, dialog) in [
            ("Verrunden …", BlendDialog::fillet()),
            ("Fase …", BlendDialog::chamfer()),
        ] {
            if ui
                .add_enabled(blend.is_none(), egui::Button::new(label))
                .on_disabled_hover_text(blend.unwrap_or_default())
                .clicked()
            {
                if tab.state.try_close_sketch().is_ok() {
                    tab.blend_dialog = Some(dialog);
                }
                ui.close();
            }
        }
        ui.separator();
        let rolled_back = tab.state.document.marker() < tab.state.document.features().len();
        if ui
            .add_enabled(rolled_back, egui::Button::new("Zum Ende der Zeitleiste"))
            .on_disabled_hover_text("Es wird bereits die ganze Zeitleiste berechnet.")
            .clicked()
        {
            let _ = browser::roll_to_end(&mut tab.state);
            ui.close();
        }
    }

    fn view_menu(&mut self, ui: &mut egui::Ui) {
        let state = &mut self.ws.active_tab_mut().state;
        for face in ViewFace::ALL {
            if ui.button(face.label()).clicked() {
                state.camera.set_view(face);
                ui.close();
            }
        }
        if ui.button("Isometrisch").clicked() {
            state.camera.set_iso_view();
            ui.close();
        }
        ui.separator();
        if ui.button("Alles einpassen").clicked() {
            state.frame_all();
            ui.close();
        }
        ui.separator();
        let dark = state.dark;
        let label = if dark {
            "Helles Design"
        } else {
            "Dunkles Design"
        };
        if ui.button(label).clicked() {
            self.ws.set_dark(!dark);
            ui.close();
        }
        ui.separator();
        for (label, pane) in [("Projekte", Pane::Projects), ("Versionen", Pane::Versions)] {
            let mut visible = self.dock.is_visible(pane);
            if ui.checkbox(&mut visible, label).changed() {
                self.dock.set_visible(pane, visible);
            }
        }
    }

    /// Einträge „Skizze auf …“ im Menü „Erzeugen“ (ohne Untermenü).
    fn sketch_menu(&mut self, ui: &mut egui::Ui) {
        let state = &mut self.ws.active_tab_mut().state;
        for (label, plane) in [
            ("Skizze auf XY-Ebene", StandardPlane::XY),
            ("Skizze auf XZ-Ebene", StandardPlane::XZ),
            ("Skizze auf YZ-Ebene", StandardPlane::YZ),
        ] {
            if ui.button(label).clicked() {
                if let Ok(id) = state.add_sketch(SketchPlane::Standard(plane)) {
                    let _ = state.open_sketch(id);
                }
                ui.close();
            }
        }
        let face = match state.selection {
            Some(Pick::Face(FaceRef { body, face })) => state
                .bodies
                .get(body)
                .and_then(|b| plane_of_face(&b.solid, face))
                .map(|_| (body, face)),
            _ => None,
        };
        let blocked = state.sketch_on_face_blocked();
        if ui
            .add_enabled(
                blocked.is_none(),
                egui::Button::new("Skizze auf gewählter Fläche"),
            )
            .on_disabled_hover_text(blocked.unwrap_or_default())
            .clicked()
            && let Some((body, face)) = face
        {
            if let Ok(id) = state.new_sketch_on_face(body, face) {
                let _ = state.open_sketch(id);
            }
            ui.close();
        }
    }

    /// Tastenkürzel Strg+N/O/S/W und Strg+Umschalt+E. Nur ohne aktives Eingabefeld, ohne offene
    /// Rückfrage (die sonst überschrieben würde) und nur im ersten Durchgang eines Bildes (ein verworfener
    /// Durchgang würde das Kürzel sonst zweimal ausführen).
    fn shortcuts(&mut self, ctx: &egui::Context) {
        if viewport::typing(ctx) || self.ws.prompt.is_some() || ctx.current_pass_index() != 0 {
            return;
        }
        let pressed = |key: egui::Key| {
            ctx.input_mut(|i| {
                i.consume_shortcut(&egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, key))
            })
        };
        if pressed(egui::Key::N) {
            self.ws.new_tab();
        }
        if pressed(egui::Key::O) {
            self.show_projects();
        }
        if pressed(egui::Key::S) {
            self.begin_save(false);
        }
        if pressed(egui::Key::W) {
            let i = self.ws.active;
            self.ws.request_close(i);
        }
        let export = ctx.input_mut(|i| {
            i.consume_shortcut(&egui::KeyboardShortcut::new(
                egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
                egui::Key::E,
            ))
        });
        if export {
            self.open_export();
        }
    }
}

/// Schließen des Fensters (Titelleiste, Alt+F4, Taskleiste – auch minimiert): erst nach
/// Rückfrage bei ungespeicherten Änderungen oder offenem Autosicherungs-Angebot. Dann wird das
/// Schließen abgebrochen und das Fenster wieder hervorgeholt, damit die Rückfrage sichtbar ist.
/// Ist das Beenden freigegeben (`ws.quit`, z. B. nach „Verwerfen“ in der letzten Rückfrage),
/// schickt die Funktion selbst einmal `Close` (`close_sent` merkt sich das).
///
/// Läuft in `App::logic`, also vor jedem Bild und auch bei minimiertem Fenster. Mehrfaches
/// Aufrufen mit derselben Schließanfrage (mehrere Durchgänge eines Bildes, oder erst minimiert,
/// dann sichtbar) ist harmlos: `request_quit` setzt dieselbe Rückfrage noch einmal.
pub fn handle_close_request(ctx: &egui::Context, ws: &mut Workspace, close_sent: &mut bool) {
    if ctx.input(|i| i.viewport().close_requested()) {
        if !ws.quit && !ws.request_quit() {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            if ctx.input(|i| i.viewport().minimized == Some(true)) {
                ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
            }
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        }
    } else if ws.quit && !*close_sent {
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        *close_sent = true;
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
    /// Läuft vor jedem `ui` und – anders als `ui` – auch bei minimiertem oder verdecktem Fenster
    /// (eframe plant dann gedrosselte Wiederholungen ein). Deshalb stehen hier das Schließen
    /// und die Autosicherung.
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        handle_close_request(ctx, &mut self.ws, &mut self.close_sent);
        self.ws.autosave_tick(Instant::now());
        ctx.request_repaint_after(AUTOSAVE_REPAINT);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let dark = self.ws.active_tab().state.dark;
        if self.applied_dark != Some(dark) {
            theme::apply(&ctx, dark);
            self.applied_dark = Some(dark);
        }

        self.shortcuts(&ctx);

        // Solange eine Rückfrage offen ist, sind Menü und Tab-Leiste gesperrt (sonst könnte
        // z. B. „Speichern“ die Rückfrage beim Beenden ersetzen).
        let no_prompt = self.ws.prompt.is_none();
        egui::Panel::top("menu").show(ui, |ui| {
            ui.add_enabled_ui(no_prompt, |ui| self.menu(ui));
        });
        egui::Panel::top("tabs").show(ui, |ui| {
            ui.add_enabled_ui(no_prompt, |ui| tabs::show(ui, &mut self.ws));
            let tab = self.ws.active_tab();
            if tab.read_only {
                let n = tab.based_on.unwrap_or_default();
                ui.weak(format!(
                    "Alte Version v{n} – schreibgeschützt. Im Bereich „Versionen“ lässt sie sich wiederherstellen oder abzweigen."
                ));
            }
        });
        let tab = self.ws.active_tab();
        egui::Panel::bottom("status").show(ui, |ui| {
            ui.horizontal(|ui| {
                let state = &tab.state;
                let sketch = state.session_sketch();
                // Vorrang: letzte Meldung, Skizzen-Modus, erstes Feature mit Fehler, Körperzahl (mit
                // Hinweis bei ungenauem Netz).
                match (&state.message, sketch, browser::status_text(state)) {
                    (Some(m), _, _) => ui.colored_label(browser::ERROR_COLOR, m),
                    (None, Some(sk), _) => ui.weak(format!(
                        "Skizzen-Modus: {} – Esc bricht ab, Eingabe beendet, Entf löscht, mittlere Maustaste oder Umschalt + Ziehen verschiebt",
                        sk.name
                    )),
                    (None, None, Some(e)) => ui.colored_label(browser::ERROR_COLOR, e),
                    (None, None, None) => match state.mesh_note() {
                        Some(note) => ui.weak(format!("{} Körper – {note}", state.bodies.len())),
                        None => ui.weak(format!("{} Körper", state.bodies.len())),
                    },
                };
            });
        });
        egui::CentralPanel::default().show(ui, |ui| {
            let mut behavior = Behavior {
                ws: &mut self.ws,
                projects_ui: &mut self.projects_ui,
                versions_ui: &mut self.versions_ui,
            };
            self.dock.tree.ui(&mut behavior, ui);
        });

        // Das Projekt-Panel kann den aktiven Tab gewechselt haben (Dokument geöffnet). Der
        // Ordner der Dateiauswahl wird für die Fenster kurz herausgenommen.
        let mut exchange_dir = self.ws.exchange_dir.take();
        let tab = self.ws.active_tab_mut();
        if let Some(dialog) = dialog_for_request(&mut tab.state) {
            tab.dialog = Some(dialog);
        }
        if let Some(id) = tab.state.edit_request.take() {
            tab.feature_dialog = FeatureDialog::new(&tab.state, id);
        }
        drop_dialogs_in_sketch_mode(&tab.state, &mut tab.dialog, &mut tab.boolean_dialog);
        if tab.state.sketch_session.is_some() {
            tab.feature_dialog = None;
            tab.blend_dialog = None;
            // Beim Schreiben einer STEP-Datei bleibt das Fenster, damit nichts ohne Meldung endet.
            if tab
                .export_dialog
                .as_ref()
                .is_some_and(|d| !d.is_writing_step())
            {
                tab.export_dialog = None;
            }
            tab.import_dialog = None;
        }
        sketch_panel::show(&ctx, &mut tab.state);
        if let Some(mut dialog) = tab.dialog.take()
            && dialog.show(&ctx, &mut tab.state)
        {
            tab.dialog = Some(dialog);
        }
        if let Some(mut dialog) = tab.feature_dialog.take()
            && dialog.show(&ctx, &mut tab.state)
        {
            tab.feature_dialog = Some(dialog);
        }
        if let Some(mut dialog) = tab.boolean_dialog.take()
            && dialog.show(&ctx, &mut tab.state)
        {
            tab.boolean_dialog = Some(dialog);
        }
        if let Some(mut dialog) = tab.blend_dialog.take()
            && dialog.show(&ctx, &mut tab.state)
        {
            tab.blend_dialog = Some(dialog);
        }
        if let Some(mut dialog) = tab.export_dialog.take()
            && dialog.show(&ctx, &mut tab.state, &mut exchange_dir)
        {
            tab.export_dialog = Some(dialog);
        }
        if let Some(mut dialog) = tab.import_dialog.take()
            && dialog.show(&ctx, &mut tab.state, &mut exchange_dir)
        {
            tab.import_dialog = Some(dialog);
        }
        if tab.blend_dialog.is_none() && tab.state.edge_picking {
            tab.state.edge_picking = false;
            tab.state.clear_edge_marks();
        }

        self.ws.exchange_dir = exchange_dir;
        self.ws.refresh_prompts();
        prompts::show(&ctx, &mut self.ws);

        viewport::remember_keyboard_focus(&ctx);
        if self.ws.quit && !self.close_sent {
            // Beenden ist freigegeben (Menü oder letzte Rückfrage): `logic` des nächsten Bildes
            // schickt `Close`.
            ctx.request_repaint();
        }
    }
}
