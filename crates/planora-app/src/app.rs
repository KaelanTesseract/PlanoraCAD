use crate::blend_dialog::BlendDialog;
use crate::boolean_dialog::BooleanDialog;
use crate::create_dialog::CreateDialog;
use crate::drawing_dialog::drawing_menu;
use crate::drawing_state::WorkArea;
use crate::export_dialog::ExportDialog;
use crate::feature_dialog::FeatureDialog;
use crate::file_chooser::default_dir;
use crate::icons::{self, Icon};
use crate::import_dialog::ImportDialog;
use crate::projects_panel::ProjectsUi;
use crate::shell::ShellState;
use crate::state::{AppState, BodyKind, SketchRequest, StandardPlane};
use crate::versions_panel::VersionsUi;
use crate::workspace::{Prompt, Workspace};
use crate::{
    about_dialog, browser, drawing_view, fonts, parameter_dialog, prompts, shell_panels,
    sketch_panel, theme, viewport,
};
use planora_feature::SketchPlane;
use planora_kernel::BoolOp;
use planora_render::gpu::GpuScene;
use planora_render::{FaceRef, Pick, ViewFace};
use planora_sketch::plane_of_face;
use std::time::{Duration, Instant};

/// Wie oft die App ohne Eingaben neu zeichnet, damit die Autosicherung auch im Leerlauf läuft.
const AUTOSAVE_REPAINT: Duration = Duration::from_secs(30);

pub struct PlanoraApp {
    pub(crate) ws: Workspace,
    /// Sichtbarkeit und Reiter der schwebenden Felder (`shell_panels`).
    pub(crate) shell: ShellState,
    pub(crate) projects_ui: ProjectsUi,
    pub(crate) versions_ui: VersionsUi,
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
        Self::setup_context(&cc.egui_ctx);
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
        Self::with_workspace(ws)
    }

    /// Einstellungen des egui-Kontexts, die nicht vom Aussehen abhängen. Kein Tastatur-Zoom
    /// (Strg und Plus oder Minus): Die Felder sind in Punkten auf die Mindestgröße 800 × 600
    /// ausgelegt, ein Zoom ließe sie dort nicht mehr hineinpassen.
    #[doc(hidden)]
    pub fn setup_context(ctx: &egui::Context) {
        ctx.options_mut(|o| o.zoom_with_keyboard = false);
    }

    fn with_workspace(ws: Workspace) -> Self {
        Self {
            ws,
            shell: ShellState::default(),
            projects_ui: ProjectsUi::default(),
            versions_ui: VersionsUi::default(),
            applied_dark: None,
            close_sent: false,
        }
    }

    /// App ohne Fenster und Renderer für die Oberflächentests.
    #[doc(hidden)]
    pub fn for_tests(ws: Workspace) -> Self {
        Self::with_workspace(ws)
    }

    /// Arbeitsbereich (für Tests).
    #[doc(hidden)]
    pub fn ws_mut(&mut self) -> &mut Workspace {
        &mut self.ws
    }

    /// Arbeitsbereich und die Zustände der Felder Projekte und Versionen (für Tests).
    #[doc(hidden)]
    pub fn panels_mut(
        &mut self,
    ) -> (
        &mut Workspace,
        &mut crate::projects_panel::ProjectsUi,
        &mut VersionsUi,
    ) {
        (&mut self.ws, &mut self.projects_ui, &mut self.versions_ui)
    }

    /// Zustand der schwebenden Felder (für Tests).
    #[doc(hidden)]
    pub fn shell_mut(&mut self) -> &mut ShellState {
        &mut self.shell
    }

    /// Öffnen (Strg+O): blendet die Projekte im linken Feld ein; dort wird das Dokument gewählt.
    /// Der Baum wird dabei neu gelesen, damit er aktuell ist.
    pub(crate) fn show_projects(&mut self) {
        self.shell.show_projects();
        self.projects_ui.invalidate();
    }

    /// Tastenkürzel Strg+N/O/S/W, Strg+Umschalt+E, Strg+Z (Rückgängig) sowie Strg+Umschalt+Z und
    /// Strg+Y (Wiederholen). Nur ohne aktives Eingabefeld (dort gehören Strg+Z und Strg+Y dem
    /// Feld), ohne offene Rückfrage (die sonst überschrieben würde) und nur im ersten Durchgang
    /// eines Bildes (ein verworfener Durchgang würde das Kürzel sonst zweimal ausführen).
    /// Rückgängig und Wiederholen sind zusätzlich gesperrt, solange ein Fenster offen ist und
    /// solange eine Maustaste gedrückt ist ([`PlanoraApp::undo_keys`]).
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
            begin_save(&mut self.ws, false);
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
            open_export(&mut self.ws);
        }
        self.undo_keys(ctx);
    }

    /// Strg+Z, Strg+Umschalt+Z und Strg+Y (R8). Gesperrt (die Tasten bleiben dann
    /// unverbraucht) bei einem offenen Fenster des Tabs
    /// ([`crate::workspace::DocTab::dialog_open`], auch Export), bei „Über“, bei einem Fenster der
    /// Felder Projekte und Versionen und solange eine Maustaste gedrückt ist: Mitten im Ziehen
    /// würde das halbe Ziehen sonst ein Schritt und gleich wieder zurückgenommen.
    /// Strg+Umschalt+Z wird vor Strg+Z abgefragt, weil das Kürzel mit Strg allein auch bei
    /// gedrückter Umschalttaste passt. Gehaltene Tasten: jedes Bild mit (Wiederholungs-)Ereignis
    /// ist ein Schritt, mehrere Ereignisse desselben Kürzels in einem Bild zusammen nur einer
    /// (`consume_shortcut` verbraucht sie alle).
    fn undo_keys(&mut self, ctx: &egui::Context) {
        if self.ws.about_open
            || self.ws.active_tab().dialog_open()
            || crate::projects_panel::dialog_open(&self.projects_ui)
            || crate::versions_panel::has_pending(&self.versions_ui)
            || ctx.input(|i| i.pointer.any_down())
        {
            return;
        }
        let consume = |modifiers: egui::Modifiers, key: egui::Key| {
            ctx.input_mut(|i| i.consume_shortcut(&egui::KeyboardShortcut::new(modifiers, key)))
        };
        let shift_z = consume(
            egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
            egui::Key::Z,
        );
        let z = consume(egui::Modifiers::COMMAND, egui::Key::Z);
        let y = consume(egui::Modifiers::COMMAND, egui::Key::Y);
        if z {
            undo_active(&mut self.ws);
        }
        if shift_z || y {
            redo_active(&mut self.ws);
        }
    }
}

/// Rückgängig im aktiven Tab (bei offener Skizze in der Skizze). Die Statuszeile nennt danach
/// den Schritt als Hinweis in normaler Farbe (`state.info`, „Rückgängig: Quader anlegen“) oder
/// sagt als Meldung, warum es nicht geht (etwa „Es gibt nichts rückgängig zu machen.“).
/// Schlägt das Zurückladen fehl, steht dort die Meldung von [`crate::workspace::DocTab::undo`];
/// hat der erreichte Stand ein fehlerhaftes Feature, dessen Fehler.
pub(crate) fn undo_active(ws: &mut Workspace) {
    history_step(ws, true);
}

/// Wiederholen im aktiven Tab; Meldung wie bei [`undo_active`].
pub(crate) fn redo_active(ws: &mut Workspace) {
    history_step(ws, false);
}

fn history_step(ws: &mut Workspace, undo: bool) {
    let tab = ws.active_tab_mut();
    let blocked = if undo {
        tab.undo_blocked()
    } else {
        tab.redo_blocked()
    };
    if let Some(why) = blocked {
        tab.state.message = Some(why.into());
        tab.state.info = None;
        return;
    }
    // Alte Meldungen gehören zum alten Stand; was danach in `message` steht, kommt vom Sprung
    // (etwa „Rückgängig ist nicht möglich.“) und bleibt.
    tab.state.message = None;
    tab.state.info = None;
    let (label, done, word) = if undo {
        (tab.undo_label(), tab.undo(), "Rückgängig")
    } else {
        (tab.redo_label(), tab.redo(), "Wiederholen")
    };
    // Landet der Sprung auf einem Stand mit fehlerhaftem Feature, zeigt die Statuszeile dessen
    // Fehler (Fehlerfarbe) statt des Hinweises; in der offenen Skizze steht der Hinweis hinter
    // der Zeile „Skizzen-Modus: …“, die dort vor Feature-Fehlern kommt.
    let shows_error = tab.state.message.is_some()
        || (tab.state.sketch_session.is_none()
            && crate::browser::status_text(&tab.state).is_some());
    if done.is_ok() && !shows_error {
        tab.state.info = Some(match label {
            Some(l) => format!("{word}: {l}"),
            None => word.into(),
        });
    }
}

/// Strg+S / Speichern: fragt nach Kommentar (Dialog), Ort oder Zweig.
fn begin_save(ws: &mut Workspace, as_new: bool) {
    let tab = ws.active_tab();
    if tab.read_only {
        return;
    }
    let (id, has_store, name) = (tab.id, tab.store.is_some(), tab.name.clone());
    ws.prompt = Some(if as_new || !has_store {
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
fn open_export(ws: &mut Workspace) {
    let tab = ws.active_tab_mut();
    if tab.state.bodies.is_empty() || tab.state.try_close_sketch().is_err() {
        return;
    }
    tab.export_dialog = Some(ExportDialog::new(&tab.state));
}

/// Datei → Netz importieren …: öffnet das Fenster im zuletzt benutzten Ordner; eine offene
/// Skizze wird vorher beendet. Nicht in schreibgeschützten Ansichten.
fn open_import(ws: &mut Workspace) {
    let dir = ws.exchange_dir.clone().unwrap_or_else(default_dir);
    let tab = ws.active_tab_mut();
    if tab.read_only || tab.state.try_close_sketch().is_err() {
        return;
    }
    tab.import_dialog = Some(ImportDialog::new(dir));
}

/// Menüeintrag (gesperrt, wenn nicht `enabled`) mit sichtbarem Tastaturfokus: die Einträge haben
/// keinen Rand, der Fokus zeigt sich als 2-px-Ring in Akzentfarbe.
fn entry(ui: &mut egui::Ui, enabled: bool, widget: impl egui::Widget) -> egui::Response {
    theme::entry(ui, enabled, widget)
}

/// Menüeintrag: Symbol, Beschriftung und rechtsbündig das Tastenkürzel (`kbd`-Schrift, schwach),
/// jedes ein eigener Text; die Beschriftung wird nicht mit Leerzeichen aufgefüllt.
fn shortcut_button(
    ui: &mut egui::Ui,
    enabled: bool,
    icon: Icon,
    label: &str,
    shortcut: &str,
) -> egui::Response {
    entry(
        ui,
        enabled,
        icons::button(icon, label)
            .shortcut_text(egui::RichText::new(shortcut).font(theme::kbd_font())),
    )
}

/// Menü „Datei“. Liefert `true`, wenn „Öffnen …“ gewählt wurde (der Aufrufer blendet dann die
/// Projekte im linken Feld ein).
#[doc(hidden)]
pub fn file_menu(ui: &mut egui::Ui, ws: &mut Workspace) -> bool {
    let read_only = ws.active_tab().read_only;
    let mut open_projects = false;
    if shortcut_button(ui, true, Icon::New, "Neu", "Strg+N").clicked() {
        ws.new_tab();
        ui.close();
    }
    if shortcut_button(ui, true, Icon::Open, "Öffnen …", "Strg+O").clicked() {
        open_projects = true;
        ui.close();
    }
    ui.separator();
    if shortcut_button(ui, !read_only, Icon::Save, "Speichern", "Strg+S").clicked() {
        begin_save(ws, false);
        ui.close();
    }
    if entry(
        ui,
        !read_only,
        icons::button(Icon::SaveAs, "Speichern unter …"),
    )
    .clicked()
    {
        begin_save(ws, true);
        ui.close();
    }
    ui.separator();
    let any = !ws.active_tab().state.bodies.is_empty();
    if shortcut_button(ui, any, Icon::Export, "Exportieren …", "Strg+Umschalt+E")
        .on_disabled_hover_text("Es gibt noch keinen Körper.")
        .clicked()
    {
        open_export(ws);
        ui.close();
    }
    if entry(
        ui,
        !read_only,
        icons::button(Icon::Import, "Netz importieren …"),
    )
    .on_disabled_hover_text("In einer alten Version lässt sich nichts hinzufügen.")
    .clicked()
    {
        open_import(ws);
        ui.close();
    }
    ui.separator();
    if shortcut_button(ui, true, Icon::Close, "Schließen", "Strg+W").clicked() {
        let i = ws.active;
        ws.request_close(i);
        ui.close();
    }
    if entry(ui, true, icons::button(Icon::About, about_dialog::TITLE)).clicked() {
        ws.about_open = true;
        ui.close();
    }
    if entry(ui, true, icons::button(Icon::Quit, "Beenden")).clicked() {
        // Ist Beenden freigegeben, schickt `handle_close_request` im nächsten Bild `Close`.
        ws.request_quit();
        ui.close();
    }
    open_projects
}

/// Menüleiste im oberen Feld; „Ansicht“ blendet Projekte und Versionen über den [`ShellState`]
/// ein und aus. Liefert `true`, wenn „Datei → Öffnen …“ gewählt wurde.
#[doc(hidden)]
pub fn menu_bar(ui: &mut egui::Ui, ws: &mut Workspace, shell: &mut ShellState) -> bool {
    menu_bar_measured(ui, ws, shell).0
}

/// Wie [`menu_bar`], dazu die Breite, die die Menüknöpfe selbst brauchen (die Leiste füllt sonst
/// die ganze verfügbare Breite).
pub(crate) fn menu_bar_measured(
    ui: &mut egui::Ui,
    ws: &mut Workspace,
    shell: &mut ShellState,
) -> (bool, f32) {
    let read_only = ws.active_tab().read_only;
    let mut open_projects = false;
    let mut width = 0.0;
    egui::MenuBar::new()
        .config(egui::containers::menu::MenuConfig::new().style(theme::menu_style))
        .ui(ui, |ui| {
            let start = ui.cursor().left();
            ui.menu_button("Datei", |ui| open_projects = file_menu(ui, ws));
            ui.add_enabled_ui(!read_only, |ui| {
                ui.menu_button("Erzeugen", |ui| create_menu(ui, ws));
            });
            // Zeichnungen lassen sich auch in alten Versionen öffnen, aber nicht anlegen.
            ui.menu_button("Zeichnung", |ui| {
                let tab = ws.active_tab_mut();
                drawing_menu(ui, &mut tab.state, &mut tab.drawing_dialog, read_only);
            });
            ui.add_enabled_ui(!read_only, |ui| {
                ui.menu_button("Bearbeiten", |ui| edit_menu(ui, ws));
            });
            ui.menu_button("Ansicht", |ui| view_menu(ui, ws, shell));
            // Der Zeiger steht hinter dem letzten Knopf und einem Abstand.
            width = ui.cursor().left() - start - ui.spacing().item_spacing.x;
        });
    (open_projects, width)
}

/// „Ansicht“: Projekte und Versionen in den schwebenden Feldern.
fn shell_toggles(ui: &mut egui::Ui, shell: &mut ShellState) {
    let mut projects = shell.projects_visible;
    if ui
        .checkbox(
            &mut projects,
            (icons::atom(Icon::PanelProjects), "Projekte"),
        )
        .changed()
    {
        shell.projects_visible = projects;
    }
    let mut versions = shell.versions_visible;
    if ui
        .checkbox(
            &mut versions,
            (icons::atom(Icon::PanelVersions), "Versionen"),
        )
        .changed()
    {
        shell.set_versions_visible(versions);
    }
}

/// Körper, die sich über „Erzeugen“ (Menü und Befehlsfeld) anlegen lassen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CreateKind {
    Box,
    Cylinder,
    Sphere,
    Cone,
    Extrusion,
    Revolution,
}

impl CreateKind {
    pub const ALL: [CreateKind; 6] = [
        CreateKind::Box,
        CreateKind::Cylinder,
        CreateKind::Sphere,
        CreateKind::Cone,
        CreateKind::Extrusion,
        CreateKind::Revolution,
    ];

    /// Beschriftung im Menü (und Hinweistext im Befehlsfeld).
    pub fn label(self) -> &'static str {
        match self {
            CreateKind::Box => "Quader …",
            CreateKind::Cylinder => "Zylinder …",
            CreateKind::Sphere => "Kugel …",
            CreateKind::Cone => "Kegel …",
            CreateKind::Extrusion => "Skizze extrudieren …",
            CreateKind::Revolution => "Skizze drehen …",
        }
    }

    /// Symbol (wie im Menü).
    pub fn icon(self) -> icons::IconSource {
        icons::body_icon(match self {
            CreateKind::Box => BodyKind::Box,
            CreateKind::Cylinder => BodyKind::Cylinder,
            CreateKind::Sphere => BodyKind::Sphere,
            CreateKind::Cone => BodyKind::Cone,
            CreateKind::Extrusion => BodyKind::Extrusion,
            CreateKind::Revolution => BodyKind::Revolution,
        })
    }
}

/// Die Skizze für „Extrudieren“ und „Drehen“: die gewählte, sonst die geöffnete, sonst die letzte.
fn sketch_target(state: &AppState) -> Option<planora_feature::FeatureId> {
    let sketches = state.sketch_features();
    state
        .selected_feature
        .or(state.open_sketch_id())
        .filter(|id| sketches.iter().any(|(s, _)| s == id))
        .or(sketches.last().map(|(id, _)| *id))
}

/// Grund, warum `kind` gerade nicht geht (Menü und Befehlsfeld); `None`: erlaubt.
pub fn create_blocked(ws: &Workspace, kind: CreateKind) -> Option<&'static str> {
    match kind {
        CreateKind::Extrusion | CreateKind::Revolution
            if sketch_target(&ws.active_tab().state).is_none() =>
        {
            Some("Zuerst eine Skizze anlegen.")
        }
        _ => None,
    }
}

/// Öffnet den Erzeugen-Dialog zu `kind` im aktiven Tab. Körper entstehen außerhalb des
/// Skizzen-Modus: eine offene Skizze wird vorher beendet. Wird der Entwurf abgelehnt, bleibt die
/// Skizze offen und der Dialog entfällt.
pub fn do_create(ws: &mut Workspace, kind: CreateKind) {
    let tab = ws.active_tab_mut();
    let dialog = match kind {
        CreateKind::Box => CreateDialog::Box {
            x: 40.0,
            y: 30.0,
            z: 20.0,
        },
        CreateKind::Cylinder => CreateDialog::Cylinder {
            radius: 15.0,
            height: 30.0,
        },
        CreateKind::Sphere => CreateDialog::Sphere { radius: 20.0 },
        CreateKind::Cone => CreateDialog::Cone {
            radius: 20.0,
            height: 40.0,
        },
        CreateKind::Extrusion | CreateKind::Revolution => {
            let Some(i) = sketch_target(&tab.state) else {
                return;
            };
            if tab.state.try_close_sketch().is_err() {
                return;
            }
            tab.dialog = Some(if kind == CreateKind::Revolution {
                CreateDialog::revolve_sketch(&tab.state, i)
            } else {
                CreateDialog::extrude_sketch(i)
            });
            return;
        }
    };
    if tab.state.try_close_sketch().is_ok() {
        tab.dialog = Some(dialog);
    }
}

/// Öffnet den Dialog „Boolesche Operation“ mit vorgewählter Operation (Menü und Chips).
pub fn open_boolean(ws: &mut Workspace, op: BoolOp) {
    let tab = ws.active_tab_mut();
    if tab.state.try_close_sketch().is_ok() {
        tab.boolean_dialog = Some(BooleanDialog::with_op(&tab.state, op));
    }
}

/// Öffnet den Dialog Verrunden (`chamfer == false`) oder Fase.
pub fn open_blend(ws: &mut Workspace, chamfer: bool) {
    let tab = ws.active_tab_mut();
    if tab.state.try_close_sketch().is_ok() {
        tab.blend_dialog = Some(if chamfer {
            BlendDialog::chamfer()
        } else {
            BlendDialog::fillet()
        });
    }
}

fn create_menu(ui: &mut egui::Ui, ws: &mut Workspace) {
    for kind in &CreateKind::ALL[..4] {
        if icons::source_entry(ui, kind.icon(), kind.label(), true).clicked() {
            do_create(ws, *kind);
            ui.close();
        }
    }
    ui.separator();
    sketch_menu(ui, ws);
    ui.separator();
    // Körper aus Skizzen: die gewählte (sonst die geöffnete, sonst die letzte) Skizze.
    for kind in &CreateKind::ALL[4..] {
        let blocked = create_blocked(ws, *kind);
        if icons::source_entry(ui, kind.icon(), kind.label(), blocked.is_none())
            .on_disabled_hover_text(blocked.unwrap_or_default())
            .clicked()
        {
            do_create(ws, *kind);
            ui.close();
        }
    }
}

/// Text des Menüeintrags „Rückgängig“: mit Aktionsnamen „Rückgängig: Quader anlegen“, ohne
/// Schritt nur „Rückgängig“.
pub fn undo_menu_text(label: Option<&str>) -> String {
    match label {
        Some(l) => format!("Rückgängig: {l}"),
        None => "Rückgängig".into(),
    }
}

/// Text des Menüeintrags „Wiederholen“ (wie [`undo_menu_text`]).
pub fn redo_menu_text(label: Option<&str>) -> String {
    match label {
        Some(l) => format!("Wiederholen: {l}"),
        None => "Wiederholen".into(),
    }
}

/// Menüeintrag „Bearbeiten → Parameter …“.
pub const PARAMETERS_ENTRY: &str = "Parameter …";

fn edit_menu(ui: &mut egui::Ui, ws: &mut Workspace) {
    // Rückgängig und Wiederholen: gesperrt mit Hilfetext, wenn es nichts zu tun gibt, in alten
    // Versionen und während des Exports. Ein offenes Fenster sperrt nicht, es wird geschlossen.
    let tab = ws.active_tab();
    let (undo_block, redo_block) = (tab.undo_blocked(), tab.redo_blocked());
    let undo_text = undo_menu_text(tab.undo_label());
    let redo_text = redo_menu_text(tab.redo_label());
    if shortcut_button(ui, undo_block.is_none(), Icon::Undo, &undo_text, "Strg+Z")
        .on_disabled_hover_text(undo_block.unwrap_or_default())
        .clicked()
    {
        undo_active(ws);
        ui.close();
    }
    if shortcut_button(ui, redo_block.is_none(), Icon::Redo, &redo_text, "Strg+Y")
        .on_disabled_hover_text(redo_block.unwrap_or_default())
        .clicked()
    {
        redo_active(ws);
        ui.close();
    }
    ui.separator();
    // Parameter: gesperrt, solange eine Skizze offen ist (ihr Entwurf hängt an den alten Werten).
    let tab = ws.active_tab_mut();
    let sketch_open = tab.state.sketch_session.is_some();
    if entry(
        ui,
        !sketch_open,
        icons::button(Icon::Parameters, PARAMETERS_ENTRY),
    )
    .on_disabled_hover_text(crate::state::SKETCH_OPEN_LOCK)
    .clicked()
    {
        parameter_dialog::open(tab);
        ui.close();
    }
    ui.separator();
    // Boolesche Operationen brauchen zwei gewöhnliche Körper; Netz-Körper sind gesperrt.
    let boolean = ws.active_tab().state.boolean_blocked();
    for (icon, label, op) in [
        (Icon::Union, "Vereinigen …", BoolOp::Union),
        (Icon::Subtract, "Abziehen …", BoolOp::Difference),
        (Icon::Intersect, "Schneiden …", BoolOp::Intersection),
    ] {
        if entry(ui, boolean.is_none(), icons::button(icon, label))
            .on_disabled_hover_text(boolean.unwrap_or_default())
            .clicked()
        {
            open_boolean(ws, op);
            ui.close();
        }
    }
    ui.separator();
    let blend = ws.active_tab().state.blend_blocked();
    for (label, source, chamfer) in icons::blend_entries() {
        if icons::source_entry(ui, source, label, blend.is_none())
            .on_disabled_hover_text(blend.unwrap_or_default())
            .clicked()
        {
            open_blend(ws, chamfer);
            ui.close();
        }
    }
    ui.separator();
    let tab = ws.active_tab_mut();
    let rolled_back = tab.state.document.marker() < tab.state.document.features().len();
    if entry(
        ui,
        rolled_back,
        icons::button(Icon::ToTimelineEnd, "Zum Ende der Zeitleiste"),
    )
    .on_disabled_hover_text("Es wird bereits die ganze Zeitleiste berechnet.")
    .clicked()
    {
        let _ = browser::roll_to_end(&mut tab.state);
        ui.close();
    }
}

fn view_menu(ui: &mut egui::Ui, ws: &mut Workspace, shell: &mut ShellState) {
    let state = &mut ws.active_tab_mut().state;
    for face in ViewFace::ALL {
        if entry(ui, true, icons::button(Icon::ViewFace, face.label())).clicked() {
            state.camera.set_view(face);
            ui.close();
        }
    }
    if entry(ui, true, icons::button(Icon::ViewIso, "Isometrisch")).clicked() {
        state.camera.set_iso_view();
        ui.close();
    }
    ui.separator();
    if entry(ui, true, icons::button(Icon::FitAll, "Alles einpassen")).clicked() {
        state.frame_all();
        ui.close();
    }
    ui.separator();
    let dark = state.dark;
    let (icon, label) = if dark {
        (Icon::ThemeLight, "Helles Design")
    } else {
        (Icon::ThemeDark, "Dunkles Design")
    };
    if entry(ui, true, icons::button(icon, label)).clicked() {
        ws.set_dark(!dark);
        ui.close();
    }
    ui.separator();
    shell_toggles(ui, shell);
}

/// Einträge „Skizze auf …“ im Menü „Erzeugen“ (ohne Untermenü).
fn sketch_menu(ui: &mut egui::Ui, ws: &mut Workspace) {
    let state = &mut ws.active_tab_mut().state;
    for (label, plane) in [
        ("Skizze auf XY-Ebene", StandardPlane::XY),
        ("Skizze auf XZ-Ebene", StandardPlane::XZ),
        ("Skizze auf YZ-Ebene", StandardPlane::YZ),
    ] {
        if entry(ui, true, icons::button(Icon::SketchOnPlane, label)).clicked() {
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
    if entry(
        ui,
        blocked.is_none(),
        icons::button(Icon::SketchOnFace, "Skizze auf gewählter Fläche"),
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
        // Das Fenster „Parameter“ vor allen übrigen Bereichen: Übernimmt es eine Zeile, weil ein
        // Klick das Feld verlassen hat, wird die Änderung ein eigener Schritt, bevor die Ansicht
        // im selben Bild mit einem Ziehen beginnt (sonst ein gemeinsamer Schritt). Ein Fenster
        // schwebt ohnehin über den Feldern.
        parameter_dialog::show_in_tab(&ctx, self.ws.active_tab_mut());
        // Im Skizzen-Modus gibt es keinen Zeichnungsbereich: die Skizze braucht die 3D-Ansicht.
        let tab = self.ws.active_tab_mut();
        if tab.state.document_name != tab.name {
            tab.state.document_name.clone_from(&tab.name);
        }
        let state = &mut tab.state;
        if state.sketch_session.is_some() {
            state.drawing_ui.area = WorkArea::Model;
        }

        // Zuerst die schwebenden Felder (Menü und Tabs, Baum, Eigenschaften | Versionen, Befehle,
        // Status, Hinweis); sie liefern die freie Fläche zwischen sich. Der Modus folgt aus dem
        // Tab (`shell_panels::mode_of`): mit offener Skizze ohne Baum, Eigenschaften und Befehle,
        // deren Stelle die Skizzenfelder von `sketch_panel::show` (weiter unten) einnehmen. Menü und Tabs sind bei
        // offener Rückfrage gesperrt, Baum und Eigenschaften in alten Versionen.
        let free = shell_panels::show_fields(&ctx, self);
        // Darunter der Arbeitsbereich über das ganze Fenster: das Modell mit der Bildmitte in der
        // freien Fläche, ebenso das Blatt der Zeichnung (über das ganze Fenster gemalt, in die
        // freie Fläche eingepasst; Werkzeug-, Ansichts- und Hinweisfeld zeichnet `show_with`).
        let enabled = !self.ws.active_tab().read_only;
        egui::CentralPanel::no_frame().show(ui, |ui| {
            let state = &mut self.ws.active_tab_mut().state;
            match state.drawing_ui.area {
                WorkArea::Model => viewport::show_in(ui, state, Some(free)),
                WorkArea::Drawing(_) => drawing_view::show_with(ui, state, enabled),
            }
        });

        // Das Feld Projekte kann den aktiven Tab gewechselt haben (Dokument geöffnet). Der
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
        // In einer alten Version lässt sich nichts ändern: das Fenster „Blattangaben“ (mit dem
        // Knopf für das Stücklistenblatt) gibt es dort nicht.
        if tab.read_only {
            tab.state.sheet_dialog = None;
        }
        if tab.state.sketch_session.is_some() {
            tab.feature_dialog = None;
            tab.blend_dialog = None;
            // Beim Schreiben einer STEP-Datei oder Zeichnung bleibt das Fenster, damit nichts ohne
            // Meldung endet.
            if tab
                .export_dialog
                .as_ref()
                .is_some_and(|d| !d.is_writing_file())
            {
                tab.export_dialog = None;
            }
            tab.import_dialog = None;
            tab.drawing_dialog = None;
            tab.state.dimension_dialog = None;
            tab.state.sheet_dialog = None;
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
        if let Some(mut dialog) = tab.drawing_dialog.take()
            && dialog.show(&ctx, &mut tab.state)
        {
            tab.drawing_dialog = Some(dialog);
        }
        if let Some(mut dialog) = tab.state.dimension_dialog.take()
            && dialog.show(&ctx, &mut tab.state)
            && tab.state.dimension_dialog.is_none()
        {
            tab.state.dimension_dialog = Some(dialog);
        }
        if let Some(mut dialog) = tab.state.sheet_dialog.take()
            && dialog.show(&ctx, &mut tab.state)
            && tab.state.sheet_dialog.is_none()
        {
            tab.state.sheet_dialog = Some(dialog);
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
        about_dialog::show(&ctx, &mut self.ws.about_open);
        prompts::show(&ctx, &mut self.ws);

        // Änderungen am Dokument werden Schritte für Rückgängig (R3, R4): erst wenn keine
        // Maustaste mehr gedrückt ist und kein Eingabefeld die Tastatur hat, damit Ziehen und
        // Tippen je ein Schritt sind. Vor `remember_keyboard_focus` zählt dabei auch das vorige
        // Bild; das verschiebt den Schritt nach Enter nur um ein Bild (beides ein Schritt).
        let gesture = ctx.input(|i| i.pointer.any_down()) || viewport::typing(&ctx);
        self.ws.observe_history(gesture);
        viewport::remember_keyboard_focus(&ctx);
        if self.ws.quit && !self.close_sent {
            // Beenden ist freigegeben (Menü oder letzte Rückfrage): `logic` des nächsten Bildes
            // schickt `Close`.
            ctx.request_repaint();
        }
    }
}
