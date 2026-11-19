use crate::history::{History, sketch_bytes};
use crate::msg::Msg;
use crate::sketch_state::SketchSession;
use glam::{Vec2, Vec3};
use planora_exchange::{ExchangeMesh, MeshReport, MeshTransform, check_mesh};
use planora_feature::planes::{PlaneBase, PlaneDef};
use planora_feature::{
    BlendArt, BodyRef, CopySource, CopyTarget, Document, EdgeCopy, FeatureError, FeatureId,
    FeatureKind, FeatureStatus, FieldKey, Formulas, MeshId, NewStepBody, ParamEdit, ParamId,
    ProfileSel, RestoreKind, SketchFeature, SketchPlane, StepShape, mesh_locked,
};
use planora_i18n::tr;
use planora_kernel::boolean::BoolOp;
use planora_kernel::{AnalyticKernel, Edge, Face, Id, Solid};
use planora_math::{DQuat, DVec2, DVec3, Frame};
use planora_mesh::{
    MassProperties, Mesh, TessellationOptions, is_watertight, mass_properties, tessellate,
};
use planora_render::{
    EdgeRef, FaceRef, Highlight, OrbitCamera, Palette, Pick, PlaneRect, Ray, SceneData,
    build_scene_marked, pick_edge_sliced, pick_face, pick_plane, pick_vertex_sliced, slice_cuts,
};
use planora_sketch::constraint::ConstraintKind;
use planora_sketch::helpers::Included;
use planora_sketch::model::{CurveId, PointId, Sketch};
use planora_sketch::{ExtractedProfiles, ProfileRef, plane_of_face};
use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::mpsc::Receiver;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BodyKind {
    Box,
    Cylinder,
    Sphere,
    Cone,
    Extrusion,
    Revolution,
    /// Ergebnis einer Booleschen Operation (oder Art unbekannt).
    Combined,
    /// Eingelesenes Netz (Netz-Körper, Teilprojekt 9).
    Mesh,
    /// Aus einer STEP-Datei übernommener exakter Körper (STEP-Körper); verschoben bleibt er es.
    Step,
}

/// Stand der Schriftliste für Skizzentext (Skizzen 8, Spec A1, G4). Die Liste entsteht einmal je
/// Programmlauf im Hintergrund ([`crate::fonts_library::start`]); bis sie da ist, wartet nichts
/// auf sie. Die App fragt den Kanal in jedem Bild ab ([`FontState::poll`]).
pub enum FontState {
    /// Die Schriftordner werden noch durchsucht; die Liste kommt über den Kanal.
    Loading(Receiver<planora_text::FontLibrary>),
    /// Die Liste ist da.
    Ready(Arc<planora_text::FontLibrary>),
}

impl FontState {
    /// Fragt den Kanal ab, ohne zu warten, und liefert die Liste, sobald sie da ist. Bricht das
    /// Durchsuchen ohne Liste ab (Kanal geschlossen), gelten die mitgelieferten Schriften, mit
    /// einer Zeile im Protokoll: Text bleibt so immer setzbar.
    pub fn poll(&mut self) -> Option<&Arc<planora_text::FontLibrary>> {
        if let FontState::Loading(rx) = self {
            match rx.try_recv() {
                Ok(lib) => *self = FontState::Ready(Arc::new(lib)),
                Err(std::sync::mpsc::TryRecvError::Empty) => return None,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    crate::diag::warn("Font scan ended without a font list; using bundled fonts");
                    *self = FontState::Ready(Arc::new(planora_text::FontLibrary::bundled_only()));
                }
            }
        }
        match self {
            FontState::Ready(lib) => Some(lib),
            FontState::Loading(_) => None,
        }
    }
}

/// Vorschau eines Einfügen-Fensters: Lage der Ebene und Linienzüge darin (Skizzenkoordinaten).
#[derive(Clone, Debug, PartialEq)]
pub struct InsertPreview {
    pub frame: Frame,
    pub lines: Vec<Vec<DVec2>>,
}

/// Griffe auf der Vorschau eines Einfügen-Fensters ([`crate::sketch_grips`] ohne Werkzeug):
/// Lage in Skizzenkoordinaten der Ebene `frame`, intern (Y nach oben, Winkel gegen den
/// Uhrzeigersinn). Die Mitte der Griffe ist `offset`, dort liegt die Mitte der Vorschau.
#[derive(Clone, Debug, PartialEq)]
pub struct InsertGrips {
    pub frame: Frame,
    pub offset: DVec2,
    /// Winkel in Radiant, gegen den Uhrzeigersinn.
    pub angle: f64,
    /// Maßstab als Faktor.
    pub factor: f64,
    /// Griff unter dem Zeiger.
    pub hot: Option<crate::sketch_grips::Grip>,
    /// Laufender Zug.
    pub drag: Option<crate::sketch_grips::GripDrag>,
    /// Der Zug hat die Lage geändert; das Fenster übernimmt sie in seine Felder.
    pub moved: bool,
    /// Esc hat in diesem Bild nur den Zug abgebrochen: Das Fenster schließt darauf nicht.
    pub esc_taken: bool,
}

impl InsertGrips {
    /// Nimmt die Ansicht den Zeiger (Griff unter dem Zeiger oder laufender Zug)?
    pub fn busy(&self) -> bool {
        self.hot.is_some() || self.drag.is_some()
    }
}

/// Grundebenen für neue Skizzen (gemeinsame Definition mit dem Feature-Modell).
pub use planora_feature::StandardPlane;

/// Größte Nummer „Bezeichnung N“, die beim Laden als Namenszähler fortgesetzt wird. Größere
/// Nummern (nur aus fremden oder bösartigen Dateien denkbar) werden ignoriert, damit der Zähler
/// nicht an `u64::MAX` stößt; neue Namen kollidieren mit ihnen erst nach so vielen Features.
const MAX_NAME_COUNTER: u64 = 1_000_000;

/// Sehnentoleranz (mm), mit der die App Dokumente aufbaut (`Document::from_snapshot`).
pub const CHORD_TOLERANCE: f64 = 0.05;

/// Meldung, wenn sich ein Schritt nicht zurückladen lässt (Rückgängig).
pub fn undo_failed() -> String {
    tr!("state-undo-failed")
}
/// Meldung, wenn sich ein Schritt nicht zurückladen lässt (Wiederholen).
pub fn redo_failed() -> String {
    tr!("state-redo-failed")
}
/// Sperre der Parameter, solange eine Skizze offen ist (ihr Entwurf hängt an den alten Werten).
pub fn sketch_open_lock() -> String {
    tr!("state-sketch-open-lock")
}

static NEXT_SCENE_VERSION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

/// Szenenstände sind über alle Tabs eindeutig, damit ein Tab-Wechsel die GPU-Szene sicher
/// neu hochlädt (siehe viewport.rs).
fn next_scene_version() -> u64 {
    NEXT_SCENE_VERSION.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

/// Sichtbarer Körper, abgeleitet aus dem Feature-Dokument (nie direkt verändert).
pub struct Body {
    pub name: String,
    pub kind: BodyKind,
    /// Verweis auf den Körper im Dokument.
    pub source: BodyRef,
    pub solid: Solid,
    pub mesh: Mesh,
    pub mass: MassProperties,
    /// Achsenparallele Hülle `(min, max)` in Millimetern.
    pub bounds: (DVec3, DVec3),
    /// Das Netz ist an einzelnen Stellen ungenau (`Mesh::inexact_faces`) oder nicht geschlossen;
    /// die Statuszeile zeigt dann [`mesh_note`]. Der Körper selbst ist davon nicht betroffen.
    pub mesh_inexact: bool,
    /// Prüfbericht des Netzes; nur bei Netz-Körpern (Spec §6).
    pub mesh_report: Option<MeshReport>,
    /// Die Ansicht zeigt nur einen Teil der Dreiecke (Netz-Körper über `display_triangles`).
    pub preview: bool,
}

/// Hinweis in der Statuszeile, wenn das Netz eines Körpers ungenau ist.
pub fn mesh_note() -> String {
    tr!("state-mesh-note")
}

/// Punkte näher als das gelten bei der Prüfung auf ein geschlossenes Netz als derselbe Punkt
/// (wie in den Netztests; benachbarte Flächen teilen ihre Kantenpunkte exakt).
const WATERTIGHT_EPS: f64 = 1e-6;

/// Wunsch aus dem Browser (Kontextmenü einer Skizze), einen Dialog zu öffnen; die App setzt ihn
/// im selben Bild in den passenden Dialog um.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SketchRequest {
    /// „Skizze extrudieren“ für dieses Skizzen-Feature.
    Extrude(FeatureId),
    /// „Skizze drehen“ für dieses Skizzen-Feature.
    Revolve(FeatureId),
}

/// Ein Wahlmodus ohne Fenster: Der nächste Klick in der 3D-Ansicht oder im Browser wählt etwas
/// für einen Befehl (Skizzen 6). Esc oder Rechtsklick beenden ihn ohne Änderung.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PendingPick {
    /// „Skizzenebene neu festlegen“ für diese Skizze: Grundebene, Konstruktionsebene oder
    /// ebene Fläche wählen.
    SketchPlane(FeatureId),
    /// „Skizze erstellen“ (wie Fusion „Create Sketch“): Grundebene, Konstruktionsebene oder
    /// ebene Fläche wählen; der Klick legt die Skizze an und öffnet sie.
    NewSketch,
}

/// Alles, was die Oberfläche anzeigt. Enthält keine egui-Typen und ist deshalb testbar.
pub struct AppState {
    /// Das Feature-Dokument; Quelle aller Körper.
    pub document: Document,
    /// Sichtbare Körper des Dokuments, abgeleitet mit [`AppState::refresh`].
    pub bodies: Vec<Body>,
    pub selected_body: Option<usize>,
    pub selection: Option<Pick>,
    pub hover: Option<Pick>,
    pub camera: OrbitCamera,
    pub scene: Arc<SceneData>,
    pub scene_version: u64,
    pub dark: bool,
    pub kernel: AnalyticKernel,
    pub chord_tolerance: f64,
    /// Letzte Fehlermeldung für die Statuszeile.
    pub message: Option<String>,
    /// Letzter Hinweis ohne Fehler für die Statuszeile (etwa „Rückgängig: Quader anlegen“),
    /// in der normalen Textfarbe. Eine Meldung in `message` hat Vorrang; der Hinweis endet mit
    /// dem nächsten Verlaufsschritt oder einer Meldung ([`crate::workspace::DocTab::observe_history`]).
    pub info: Option<String>,
    /// Der Hinweis `info` beschreibt die Änderung, aus der der nächste Verlaufsschritt wird (etwa
    /// die Zusammenfassung von „STEP importieren“): dieser eine Schritt beendet ihn nicht.
    pub info_outlives_step: bool,
    /// Ein Eintrag des Rechtsklickmenüs der Skizze (Kopieren, Ausschneiden, Einfügen) wurde
    /// gewählt; der Verteiler der App führt ihn mit der Zwischenablage des Arbeitsbereichs aus
    /// (Skizzen 11b).
    pub clip_request: Option<crate::keymap::Action>,
    /// Laufende Nummer je Bezeichnung für neue Features („Quader 1“, „Quader 2“, …); zählt
    /// nur hoch, damit Namen auch nach dem Löschen eindeutig bleiben.
    name_counters: BTreeMap<String, u64>,
    /// Im Browser gewähltes Feature (bis zum Feature-Baum: eine Skizze).
    pub selected_feature: Option<FeatureId>,
    /// Für die Verlaufsleiste gewählter Körper (Spitze oder ein Schritt seiner Kette), wie
    /// gewählt; gelesen wird er über [`AppState::history_body`].
    history_choice: Option<FeatureId>,
    /// Auf- und zugeklappte Ordner des Feature-Baums (je Tab).
    pub tree_folders: crate::browser::FolderState,
    /// Aufgeklappte Skizzengruppen im Ordner „Skizzen“ (Skizzen 11b; Ansicht, nicht Dokument).
    pub tree_groups_open: std::collections::BTreeSet<planora_feature::SketchGroupId>,
    /// Per Strg-Klick im Ordner „Skizzen“ gewählte Skizzen (Skizzen 11b); gilt nur, solange
    /// `selected_feature` darin steht (siehe [`AppState::browser_sketch_selection`]).
    pub browser_sketches: Vec<FeatureId>,
    /// Rechtsklickmenü auf einer fertigen Skizze in der 3D-Ansicht (Skizzen 12): Bildschirmort
    /// des Menüs. Gilt für die gewählte Skizzenkurve oder den gewählten Skizzenpunkt.
    pub sketch_pick_menu: Option<(f32, f32)>,
    /// Im Ordner „Skizzen“ gewählte Gruppe (Skizzen 11b); gilt nur ohne `selected_feature`.
    pub browser_group: Option<planora_feature::SketchGroupId>,
    /// Grundebene unter dem Zeiger (hell gezeichnet; gilt nur für das laufende Bild).
    pub origin_hover: Option<StandardPlane>,
    /// Gewählte Grundebene; gelesen über [`AppState::selected_origin_plane`].
    selected_origin: Option<StandardPlane>,
    /// Geöffneter Skizzen-Modus.
    pub sketch_session: Option<SketchSession>,
    /// Entwurf des geöffneten Skizzen-Features (Entscheidung F6): eine Kopie der Definition mit
    /// der berechneten Skizze. Der Skizzen-Modus arbeitet nur hierauf; erst
    /// [`AppState::close_sketch`] schreibt den Entwurf ins Dokument zurück.
    pub sketch_draft: Option<SketchFeature>,
    /// Verlauf der offenen Skizze (R10): Bilder des Entwurfs, angelegt beim Öffnen, verworfen,
    /// sobald die Sitzung endet (Beenden, Löschen, Zurückladen des Dokuments). Geändert heißt:
    /// der Entwurf ist ungleich dem aktuellen Bild (Stand der Skizze, Kopien), siehe
    /// [`AppState::observe_sketch`].
    sketch_history: Option<History<SketchFeature>>,
    /// Kamera vor dem Öffnen des Skizzen-Modus.
    saved_camera: Option<OrbitCamera>,
    /// Einpassen beim Öffnen einer Skizze, das die 3D-Ansicht mit dem freien Rechteck des
    /// Skizzenmodus nachholt ([`AppState::refit_sketch_view`]).
    sketch_fit: Option<SketchFit>,
    /// Die Ansicht wurde ohne bekanntes freies Rechteck eingepasst (neuer Tab, geladener Stand)
    /// und wird einmal nachgeholt, sobald die 3D-Ansicht das freie Rechteck kennt
    /// ([`crate::viewport::show_in`]).
    pub fit_pending: bool,
    /// Offener Dialogwunsch aus dem Browser (siehe [`SketchRequest`]).
    pub sketch_request: Option<SketchRequest>,
    /// Zuletzt berechnete Profile (für die Dialoge, die jedes Bild neu zeichnen).
    pub profile_cache: ProfileCache,
    /// Abgetastete Kurven und feste Mengen der fertigen Skizzen je Stand (Skizzen 12, Task 12b),
    /// damit nicht jedes Bild jede Kurve neu abtastet.
    pub finished_cache: crate::finished_cache::FinishedSketchCache,
    /// Skizzenraster zeigen (Skizzenfeld, wie Fusions Skizzenpalette); Vorgabe an. Ohne Raster
    /// kein Raster-Fang. Den Stand führt [`crate::workspace::Workspace::sync_sketch_grid`].
    pub sketch_show_grid: bool,
    /// Freie Punkte am sichtbaren Raster fangen (Skizzenfeld „Fang“); Vorgabe an.
    pub sketch_snap_grid: bool,
    /// Ein Schalter wurde betätigt: Der Arbeitsbereich speichert den Stand und gibt ihn allen Tabs.
    pub sketch_grid_changed: bool,
    /// Aufklappfeld „Anzeigeoptionen“ im Skizzenfeld offen (Skizzen 12). Der Zustand liegt hier,
    /// nicht im Popup-Speicher von egui: Der steht je Fenster, und das Skizzenfeld kann in einem
    /// eigenen Fenster liegen.
    pub sketch_display_menu_open: bool,
    /// „Ebenen anzeigen“ der Triade der 3D-Skizze (Rechtsklickmenü, Skizzen 10); Vorgabe an. Den
    /// Stand führt [`crate::workspace::Workspace::sync_triad_view`].
    pub triad_planes: bool,
    /// „Drehgriffe anzeigen“ der Triade; Vorgabe an.
    pub triad_handles: bool,
    /// Ein Haken des Menüs wurde betätigt: Der Arbeitsbereich speichert und gibt ihn allen Tabs.
    pub triad_view_changed: bool,
    /// Offene Profilwahl eines Dialogs „Extrudieren“/„Drehen“: Die 3D-Ansicht zeigt die Profile
    /// der Skizze und schaltet sie per Klick um (siehe [`ProfilePick`]).
    pub profile_pick: Option<ProfilePick>,
    /// Offene Rückfrage „Löschen mit Folgen“ aus dem Feature-Baum.
    pub pending_delete: Option<crate::browser::DeletePlan>,
    /// Offenes Fenster „Umbenennen“: Feature und Entwurf des Namens.
    pub rename_draft: Option<(FeatureId, String)>,
    /// Offenes Fenster „Umbenennen“ einer Skizzengruppe (Skizzen 11b): Gruppe und Entwurf.
    pub group_rename_draft: Option<(planora_feature::SketchGroupId, String)>,
    /// Offene Rückfrage „Zeichnung oder Ansicht löschen“ aus dem Abschnitt „Zeichnungen“.
    pub pending_delete_drawing: Option<crate::drawing_browser::DrawingDeletePlan>,
    /// Offenes Fenster „Umbenennen“ einer Zeichnung: Zeichnung und Entwurf des Namens.
    pub rename_drawing_draft: Option<(planora_drawing::DrawingId, String)>,
    /// Wunsch aus dem Feature-Baum, den Bearbeiten-Dialog für dieses Feature zu öffnen.
    pub edit_request: Option<FeatureId>,
    /// Markierte Kanten (Mehrfachauswahl im Fenster „Verrunden“/„Fase“), alle aus einem Körper.
    pub marked_edges: Vec<EdgeRef>,
    /// Klicks auf Kanten markieren statt zu wählen (solange das Fenster offen ist).
    pub edge_picking: bool,
    /// Ein Klick markiert die ganze Tangentenkette.
    pub chain_marking: bool,
    /// Offenes Fenster einer Konstruktionsebene (Skizzen 6): Klicks in der 3D-Ansicht und im
    /// Ordner „Ursprung“ füllen sein aktives Auswahlfeld statt zu wählen; dazu die Vorschau.
    pub plane_pick: Option<crate::plane_dialog::PlanePick>,
    /// Laufender Wahlmodus ohne Fenster ([`PendingPick`]); schließt sich mit `plane_pick` und
    /// dem Fenster „Verrunden“/„Fase“ gegenseitig aus.
    pub pending_pick: Option<PendingPick>,
    /// Laufendes „Kalibrieren“ einer Leinwand (Skizzen 9): ein Klickmodus in der Ansicht
    /// außerhalb der Skizze ([`crate::insert::calibrate::Calibrate`]).
    pub calibrate: Option<crate::insert::calibrate::Calibrate>,
    /// Name des nächsten Rückgängig-Schritts, wenn der Vergleich der Bilder ihn nicht
    /// erkennt (etwa „Kalibrieren“, das sonst wie „Leinwand bearbeiten“ aussähe). Gilt nur für
    /// genau den nächsten Schritt.
    pub undo_label_hint: Option<fn() -> String>,
    /// Die Parametertabelle hat sich geändert (Skizzen 11b, Parameter im Text): Der Tab setzt
    /// seine Texte im nächsten Bild neu ([`crate::workspace::DocTab::apply_fonts`]).
    pub(crate) texts_due: bool,
    /// Ist ein Fenster „DXF einfügen“ oder „SVG einfügen“ offen (Stand des letzten Bildes)? Dann
    /// gehört Esc dem Fenster: Die offene Skizze bricht damit nicht auch ihr Werkzeug ab
    /// (Gesamtprüfung M1).
    pub insert_dialog_open: bool,
    /// Vorschau eines offenen Einfügen-Fensters (Skizzen 9, „DXF einfügen“): Linienzüge in
    /// Skizzenkoordinaten auf der gewählten Ebene, als Linien über der Ansicht gezeichnet.
    pub insert_preview: Option<InsertPreview>,
    /// Griffe auf der Vorschau eines Einfügen-Fensters (Skizzen 9, „SVG einfügen“): das Fenster
    /// schreibt die Lage in jedem Bild, die Ansicht den Zug am Griff ([`InsertGrips::moved`]).
    pub insert_grips: Option<InsertGrips>,
    /// Vorschau des Fensters „Leinwand“ (Skizzen 9): das Bild mit der Lage aus den Feldern.
    pub canvas_preview: Option<crate::canvas::CanvasPreview>,
    /// Entschlüsselte Bilder der Leinwände, einmal je Bild ([`crate::canvas::TextureCache`]).
    pub canvas_textures: crate::canvas::TextureCache,
    /// Auswählbare Leinwand unter dem Zeiger (nur für dieses Bild, Rahmen in der Ansicht).
    pub canvas_hover: Option<FeatureId>,
    /// Verschieben, das der laufende Zug im Eigenschaftenfenster angelegt hat.
    drag_move: Option<FeatureId>,
    /// Prüfberichte der Netze, einmal je Netz berechnet (Plan P13).
    mesh_reports: BTreeMap<MeshId, MeshReport>,
    /// Ab so vielen Dreiecken zeigt die Ansicht Netz-Körper als Vorschau (Spec §11).
    pub display_triangles: usize,
    /// Zeichnungsbereich: geöffnete Zeichnung, Auswahl, Blattansicht und Hintergrundberechnung.
    pub drawing_ui: crate::drawing_state::DrawingUi,
    /// Aufgelöste Maße der geöffneten Zeichnung mit Zwischenspeicher und letzten Lagen.
    pub dimension_ui: crate::dimension_state::DimensionUi,
    /// Werkzeug „Maß“ des Zeichnungsbereichs (Untermodus, gewählte Bezüge, Vorschau).
    pub dimension_tool: crate::dimension_tool::DimensionTool,
    /// Werkzeug „Position“ im Zeichnungsbereich.
    pub balloon_tool: crate::balloon_tool::BalloonTool,
    /// Werkzeug „Schnitt“ im Zeichnungsbereich.
    pub section_tool: crate::section_tool::SectionTool,
    /// Offenes Fenster „Maß bearbeiten“.
    pub dimension_dialog: Option<crate::dimension_dialog::DimensionDialog>,
    /// Offenes Fenster „Blattangaben“.
    pub sheet_dialog: Option<crate::sheet_dialog::SheetInfoDialog>,
    /// Offener Dialog „Auf Fläche projizieren“ oder „Schnittkurve“ der offenen Skizze
    /// (Skizzen 10); er folgt der gewählten Art ([`crate::surface_project_dialog::sync`]).
    pub surface_dialog: Option<crate::surface_project_dialog::SurfaceDialog>,
    /// Offener Dialog „Krümmungsanzeige einrichten“ der offenen Skizze (Skizzen 11a); seine
    /// Vorschau zeichnet die Ansicht, die Skizze ändert erst OK.
    pub curvature_dialog: Option<crate::curvature_dialog::CurvatureDialog>,
    /// Offener Dialog „Verlorene Projektionen verwalten“ (Skizzen 11a); bleibt über das Öffnen
    /// der Skizze für „Neu verknüpfen“ offen.
    pub lost_dialog: Option<crate::lost_projections_dialog::LostProjectionsDialog>,
    /// Offener Dialog „Als DXF speichern“ einer Skizze (Skizzen 11b); ändert das Dokument nie.
    pub sketch_dxf_dialog: Option<crate::sketch_dxf_dialog::SketchDxfDialog>,
    /// Hervorhebungsliste der Ansicht (Task H3): was die Felder des offenen Dialogs
    /// [`AppState::surface_dialog`] gewählt haben. Flächen sind getönt, Kanten, Ecken, Achsen,
    /// Ebenen und Kurven fertiger Skizzen stehen in der Auswahlfarbe. Setzen nur über
    /// [`AppState::set_dialog_marks`] (baut die Szene neu).
    pub dialog_marks: Vec<IncludePick>,
    /// Offene Rückfrage „Maß löschen“ (Kettenglieder) aus dem Abschnitt „Maße“.
    pub pending_delete_dimension: Option<crate::dimension_browser::DimDeletePlan>,
    /// Name des Dokuments für die Zeichnungsnummer im Schriftfeld („Werkstück-01“); die App setzt
    /// ihn in jedem Bild aus dem Namen des Tabs.
    pub document_name: String,
    /// Kennung dieses Zustands (jedes Tab hat einen eigenen; eindeutig im Programm). Trennt die
    /// programmweiten Zwischenspeicher der Zeichnungsansicht nach Dokumenten.
    pub instance: u64,
    /// Die Schriftliste des Programms, sobald sie da ist ([`AppState::fonts`]); die App gibt sie
    /// jedem Tab ([`crate::workspace::Workspace::apply_fonts`]).
    fonts: Option<Arc<planora_text::FontLibrary>>,
    /// Die zuletzt benutzten Werte des Werkzeugs „Text“ (Spec C4): geteilt mit dem
    /// Arbeitsbereich und allen seinen Tabs, so gelten sie für den Programmlauf
    /// ([`AppState::share_text_memory`]). Jede Skizzensitzung bekommt denselben Speicher.
    text_memory: crate::sketch_text::TextMemoryShare,
}

/// Profilwahl eines offenen Dialogs „Extrudieren“/„Drehen“ (wie in Fusion: Klick in die Ansicht
/// schaltet ein Profil um). Der Dialog legt Skizze und Wahl in jedem Bild ab; die 3D-Ansicht
/// trägt die Profile unter dem Mauszeiger ein (`hover`: die ein Klick jetzt umschalten würde, bei
/// einem Textprofil ohne Strg also alle Profile des Textes, [`crate::profile_pick::click_set`])
/// und ein angeklicktes (`clicked`, mit den beim Klick gedrückten Tasten), das der Dialog im
/// selben Bild umschaltet und löscht ([`crate::profile_pick::take_click`]). Indizes in
/// `profiles()` der berechneten Skizze.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProfilePick {
    pub sketch: FeatureId,
    pub selected: Vec<usize>,
    pub hover: Vec<usize>,
    pub clicked: Option<(usize, egui::Modifiers)>,
}

/// Merkt sich die Profile der zuletzt gefragten Skizze zu ihrem Stand
/// ([`Sketch::revision`], über alle Skizzen eindeutig). Ändert sich die Skizze, ändert sich der
/// Stand, und die Profile werden neu berechnet.
#[derive(Debug, Default)]
pub struct ProfileCache {
    entry: RefCell<Option<(u64, Arc<ExtractedProfiles>)>>,
    computed: Cell<usize>,
    /// Gruppen der Profilwahl ([`crate::profile_pick::groups`]) zum selben Stand.
    groups: RefCell<Option<(u64, Arc<Vec<crate::profile_pick::ProfileGroup>>)>>,
    /// Zahl der Profile nach dem alten Algorithmus ([`Sketch::legacy_profiles`]) zum Stand.
    legacy: Cell<Option<(u64, usize)>>,
    legacy_computed: Cell<usize>,
}

impl ProfileCache {
    /// Die Profile von `sk`, aus dem Speicher, wenn sich die Skizze seither nicht geändert hat.
    pub fn get(&self, sk: &Sketch) -> Arc<ExtractedProfiles> {
        let mut entry = self.entry.borrow_mut();
        if let Some((rev, p)) = entry.as_ref()
            && *rev == sk.revision()
        {
            return Arc::clone(p);
        }
        let p = Arc::new(sk.profiles());
        self.computed.set(self.computed.get() + 1);
        *entry = Some((sk.revision(), Arc::clone(&p)));
        p
    }

    /// Die Gruppen der Profile von `sk` (je Text eine, sonst je Profil eine, Spec D2), aus dem
    /// Speicher, wenn sich die Skizze seither nicht geändert hat. So läuft die Zuordnung der
    /// Profile zu Texten einmal je Stand und nicht in jedem Bild.
    pub fn groups(&self, sk: &Sketch) -> Arc<Vec<crate::profile_pick::ProfileGroup>> {
        if let Some((rev, g)) = self.groups.borrow().as_ref()
            && *rev == sk.revision()
        {
            return Arc::clone(g);
        }
        let g = Arc::new(crate::profile_pick::groups(&self.get(sk), sk));
        *self.groups.borrow_mut() = Some((sk.revision(), Arc::clone(&g)));
        g
    }

    /// Wie oft bisher tatsächlich berechnet wurde (für Tests).
    pub fn computations(&self) -> usize {
        self.computed.get()
    }

    /// Zahl der Profile von `sk` nach dem alten Algorithmus (für alte Profilnummern, die keiner
    /// heutigen Fläche entsprechen); neu gezählt nur nach einer Änderung der Skizze.
    pub fn legacy_count(&self, sk: &Sketch) -> usize {
        if let Some((rev, n)) = self.legacy.get()
            && rev == sk.revision()
        {
            return n;
        }
        let n = sk.legacy_profiles().profiles.len();
        self.legacy_computed.set(self.legacy_computed.get() + 1);
        self.legacy.set(Some((sk.revision(), n)));
        n
    }

    /// Wie oft die alten Profile bisher gezählt wurden (für Tests).
    pub fn legacy_computations(&self) -> usize {
        self.legacy_computed.get()
    }
}

/// R10: Der Schritt bleibt offen, solange ein Werkzeug offene Punkte hat (`pending`) und seit
/// dem letzten Bild `before` **nur** Punkte hinzukamen (samt ihrer Bindung an eine Kurve beim
/// Fangen): Der erste Klick einer Linie, eines Kreises, Bogens, Splines gehört zum Element.
/// Alles Vorhandene muss dafür unverändert sein: Kurven (Art, Hilfslinie), Punkte und alle
/// übrigen Werte bitgleich, Abhängigkeiten (Art samt Maßwert, „anzeigend“, aktiv, Lage), Kopien.
/// Kam eine Kurve hinzu, fehlt oder änderte sich etwas (Maßwert, „anzeigend“, Hilfslinie), ist
/// es ein eigener Schritt (ein Linienzug: je Linie einer). Ebenso ein umgeschalteter Haken
/// „3D-Skizze“: Er ist ein eigener Schritt (Spec 4), auch bei offenem Punkt.
fn deferred_by_tool(se: &SketchSession, before: &SketchFeature, after: &SketchFeature) -> bool {
    if se.pending.is_empty()
        || before.copies != after.copies
        || before.sketch.three_d != after.sketch.three_d
    {
        return false;
    }
    let (b, a) = (&before.sketch, &after.sketch);
    let same_curves = a.curves().count() == b.curves().count()
        && b.curves().all(|(id, c)| {
            a.curve(id).is_some_and(|n| {
                n.kind == c.kind && n.construction == c.construction && n.centerline == c.centerline
            })
        });
    let bits = |v: DVec2| (v.x.to_bits(), v.y.to_bits());
    let points_kept = b.points().all(
        |(id, _)| matches!((b.point_xy(id), a.point_xy(id)), (Ok(p), Ok(q)) if bits(p) == bits(q)),
    );
    // Alle bisherigen Werte (Koordinaten, Radien, Kurvenparameter) bitgleich; neue Punkte
    // hängen ihre Werte hinten an.
    let (bv, av) = (b.variables(), a.variables());
    let values_kept =
        av.len() >= bv.len() && bv.iter().zip(av).all(|(x, y)| x.to_bits() == y.to_bits());
    let constraints_kept = b.constraints().all(|(id, c)| {
        a.constraint(id).is_some_and(|n| {
            n.kind == c.kind
                && n.driven == c.driven
                && n.enabled == c.enabled
                && n.branch() == c.branch()
        })
    });
    // Fangbindungen des offenen Punktes gehören zum Element: auf der Kurve, in der Linienmitte,
    // im Ursprung.
    let new_open = |p: &PointId| se.pending.contains(p) && b.point_xy(*p).is_err();
    // Ein beim Fang an einer Körperecke eingezogener Punkt (A7) gehört ebenso zum Element.
    let included =
        |p: &PointId| se.included_vertices.iter().any(|(q, _)| q == p) && b.point_xy(*p).is_err();
    let only_snaps = a
        .constraints()
        .filter(|(id, _)| b.constraint(*id).is_none())
        .all(|(_, c)| match &c.kind {
            ConstraintKind::PointOnCurve(..) => true,
            ConstraintKind::Fixed(p, ..) | ConstraintKind::Fixed3(p, ..) if included(p) => true,
            ConstraintKind::Coincident(p, q) => {
                (new_open(p) && included(q)) || (new_open(q) && included(p))
            }
            ConstraintKind::Midpoint(p, _) => new_open(p),
            ConstraintKind::Fixed(p, x, y) => new_open(p) && *x == 0.0 && *y == 0.0,
            ConstraintKind::Fixed3(p, x, y, z) => {
                new_open(p) && *x == 0.0 && *y == 0.0 && *z == 0.0
            }
            _ => false,
        });
    same_curves && points_kept && values_kept && constraints_kept && only_snaps
}

/// Verknüpft die eingezogenen Eckpunkte der Sitzung `se` (A7) im Entwurf `draft` mit ihren
/// Körperecken; tote Punkte und Ecken ohne Verweis entfallen (der Punkt bleibt dann fest stehen).
fn link_vertices(state: &AppState, se: &mut SketchSession, draft: &mut SketchFeature) {
    let tol = state.kernel.tolerance;
    let visible = state.document.visible_bodies_before(se.sketch);
    for (q, i) in std::mem::take(&mut se.included_vertices) {
        let target = planora_feature::CopyTarget::Point(q);
        if draft.sketch.point_xy(q).is_err() || draft.copies.iter().any(|c| c.target == target) {
            continue;
        }
        let Some(v) = se.body_vertices.get(i).copied() else {
            continue;
        };
        let source = state.bodies.get(v.body).map(|b| b.source);
        let Some(solid) = visible
            .iter()
            .find(|b| Some(b.body) == source)
            .map(|b| b.solid)
        else {
            continue;
        };
        let Ok(edge) = planora_feature::EdgeRef::from_edge(solid, v.edge, &tol) else {
            continue;
        };
        let source = CopySource::Include(planora_feature::IncludeRef::Vertex(
            planora_feature::VertexRef { edge, end: v.end },
        ));
        draft.copies.push(EdgeCopy {
            target,
            source,
            piece: 0,
            of: 1,
        });
    }
}

/// Länge des Geometrieschlüssels einer Kante (Anfangs- und Endpunkt, Parameterbereich).
const EDGE_KEY_LEN: usize = 8;

/// Slice beim Anklicken: Ab welcher Strahllänge zählt ein Treffer? 0, wenn der Strahl hinter der
/// Ebene beginnt; sonst dort, wo er die Toleranz [`planora_render::SLICE_EPS`] vor der Ebene
/// erreicht. `None`, wenn er die weggeschnittene Seite nie verlässt.
fn slice_ray_start(plane: [f32; 4], origin: DVec3, dir: DVec3) -> Option<f32> {
    let [nx, ny, nz, d] = plane.map(f64::from);
    let n = DVec3::new(nx, ny, nz);
    let side = n.dot(origin) + d - f64::from(planora_render::SLICE_EPS);
    if side <= 0.0 {
        return Some(0.0);
    }
    let towards = -n.dot(dir);
    (towards > 1e-12).then(|| (side / towards) as f32)
}

/// Geometrieschlüssel einer Kante (Bitmuster von Endpunkten und Parameterbereich); gleich nur,
/// wenn die Kante unverändert liegt. `None`, wenn es die Kante nicht gibt.
fn edge_key(solid: &Solid, edge: Id<Edge>) -> Option<[u64; EDGE_KEY_LEN]> {
    let e = solid.edges.get(edge)?;
    let (a, b) = (
        solid.vertices.get(e.start)?.point,
        solid.vertices.get(e.end)?.point,
    );
    Some([
        a.x.to_bits(),
        a.y.to_bits(),
        a.z.to_bits(),
        b.x.to_bits(),
        b.y.to_bits(),
        b.z.to_bits(),
        e.t0.to_bits(),
        e.t1.to_bits(),
    ])
}

/// Größter Betrag einer Koordinate oder eines Maßes in mm (wie
/// [`planora_feature::MAX_COORD_MM`], dort für jede Berechnung geprüft): Darüber ist der
/// Abstand benachbarter `f64`-Werte größer als die Toleranz.
pub const MAX_COORD_MM: f64 = planora_feature::MAX_COORD_MM;
/// Bezeichnung (und Namensstamm) von Skizzen-Features. Der Name wird mit dem Feature
/// gespeichert und bleibt in der Sprache, in der er entstand.
fn sketch_label() -> String {
    tr!("state-sketch-label")
}
const PICK_EDGE_PX: f32 = 6.0;
/// Fangweite für Eckpunkte bei der Punktwahl (Skizzen 6, „Ebene durch drei Punkte“).
const PICK_VERTEX_PX: f32 = 8.0;

/// Was [`AppState::pick_with`] finden darf (Skizzen 6). Ausgeschaltete Flächen verdecken
/// trotzdem, was hinter ihnen liegt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PickFilter {
    /// Konstruktionsebenen.
    pub planes: bool,
    /// Eckpunkte der Körper (nur bei der Punktwahl eines Dialogs).
    pub vertices: bool,
    /// Kanten der Körper.
    pub edges: bool,
    /// Flächen der Körper.
    pub faces: bool,
    /// Punkte fertiger, sichtbarer Skizzen (Skizzen 12); normale Auswahl und Punktfelder.
    pub sketch_points: bool,
    /// Kurven fertiger, sichtbarer Skizzen (Skizzen 12); normale Auswahl und Achsenfelder.
    pub sketch_curves: bool,
}

impl PickFilter {
    /// Normale Auswahl in der 3D-Ansicht: Flächen, Kanten, Ebenen und die fertigen Skizzen
    /// (Skizzen 12), keine Eckpunkte.
    pub const SELECT: PickFilter = PickFilter {
        planes: true,
        vertices: false,
        edges: true,
        faces: true,
        sketch_points: true,
        sketch_curves: true,
    };
    /// Nur Flächen und Kanten (Projizieren und Schneiden im Skizzen-Modus).
    pub const GEOMETRY: PickFilter = PickFilter {
        planes: false,
        vertices: false,
        edges: true,
        faces: true,
        sketch_points: false,
        sketch_curves: false,
    };
    /// Punktwahl: Eckpunkte vor allem anderen, dann Kanten und Flächen. Ebenen zählen nicht
    /// und verdecken keine Ecken (die Punkte liegen oft hinter einer schon gezeichneten Ebene).
    pub const POINTS: PickFilter = PickFilter {
        planes: false,
        vertices: true,
        edges: true,
        faces: true,
        sketch_points: false,
        sketch_curves: false,
    };
    /// „3D-Geometrie einbeziehen“ (Skizzen 10): Ecken, Kanten, Flächen und Ebenen.
    pub const INCLUDE: PickFilter = PickFilter {
        planes: true,
        vertices: true,
        edges: true,
        faces: true,
        sketch_points: false,
        sketch_curves: false,
    };
    /// Bezug eines Ebenen-Fensters: Flächen und Ebenen, keine Kanten.
    pub const BASES: PickFilter = PickFilter {
        planes: true,
        vertices: false,
        edges: false,
        faces: true,
        sketch_points: false,
        sketch_curves: false,
    };
    /// Nur Flächen (Tangentialebene).
    pub const FACES: PickFilter = PickFilter {
        planes: false,
        vertices: false,
        edges: false,
        faces: true,
        sketch_points: false,
        sketch_curves: false,
    };
    /// Nur Kanten (Achse der Ebene unter Winkel).
    pub const EDGES: PickFilter = PickFilter {
        planes: false,
        vertices: false,
        edges: true,
        faces: false,
        sketch_points: false,
        sketch_curves: false,
    };
    /// Nur Eckpunkte („Ebene durch drei Punkte“).
    pub const VERTICES: PickFilter = PickFilter {
        planes: false,
        vertices: true,
        edges: false,
        faces: false,
        sketch_points: false,
        sketch_curves: false,
    };
}
const BODY_GAP_MM: f64 = 10.0;
/// Mindestgröße des Bereichs, auf den die Ansicht beim Öffnen einer Skizze eingepasst wird.
const MIN_SKETCH_VIEW_MM: f64 = 10.0;
/// Halbe Kantenlänge des Bereichs um den Ursprung der Ebene, den eine leere Skizze ohne Fläche
/// und ohne Körper zeigt (wie `frame_all` ohne Körper: ±25 mm).
const EMPTY_SKETCH_HALF_MM: f64 = 25.0;
/// So viele Bilder in Folge mit unverändertem freiem Rechteck beendet das Nachholen des
/// Einpassens beim Öffnen einer Skizze.
const SKETCH_FIT_STABLE_FRAMES: u8 = 3;

/// Einpassen beim Öffnen einer Skizze: die Punkte, auf die eingepasst wird, und die Kamera nach
/// dem letzten Einpassen. Die 3D-Ansicht passt nach, solange sich das freie Rechteck ändert
/// (beim Öffnen gilt noch das des Modellmodus) und der Benutzer die Ansicht nicht bewegt hat.
#[derive(Clone, Debug)]
struct SketchFit {
    points: Vec<Vec3>,
    fitted: OrbitCamera,
    stable: u8,
}

/// Nächste Kennung für [`AppState::instance`].
static NEXT_INSTANCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

impl AppState {
    pub fn new() -> Self {
        let chord_tolerance = CHORD_TOLERANCE;
        let mut state = Self {
            document: Document::with_options(chord_tolerance),
            bodies: Vec::new(),
            selected_body: None,
            selection: None,
            hover: None,
            camera: OrbitCamera::default(),
            scene: Arc::new(SceneData::default()),
            scene_version: 0,
            dark: true,
            kernel: AnalyticKernel::default(),
            chord_tolerance,
            message: None,
            info: None,
            info_outlives_step: false,
            clip_request: None,
            name_counters: BTreeMap::new(),
            selected_feature: None,
            history_choice: None,
            tree_folders: crate::browser::FolderState::default(),
            tree_groups_open: std::collections::BTreeSet::new(),
            browser_sketches: Vec::new(),
            sketch_pick_menu: None,
            browser_group: None,
            origin_hover: None,
            selected_origin: None,
            sketch_session: None,
            sketch_draft: None,
            sketch_history: None,
            saved_camera: None,
            sketch_fit: None,
            fit_pending: false,
            sketch_request: None,
            profile_cache: ProfileCache::default(),
            finished_cache: Default::default(),
            sketch_show_grid: true,
            sketch_snap_grid: true,
            sketch_grid_changed: false,
            sketch_display_menu_open: false,
            triad_planes: true,
            triad_handles: true,
            triad_view_changed: false,
            profile_pick: None,
            pending_delete: None,
            rename_draft: None,
            group_rename_draft: None,
            pending_delete_drawing: None,
            rename_drawing_draft: None,
            edit_request: None,
            marked_edges: Vec::new(),
            edge_picking: false,
            chain_marking: true,
            plane_pick: None,
            pending_pick: None,
            calibrate: None,
            undo_label_hint: None,
            texts_due: false,
            insert_dialog_open: false,
            insert_preview: None,
            insert_grips: None,
            canvas_preview: None,
            canvas_textures: crate::canvas::TextureCache::default(),
            canvas_hover: None,
            drag_move: None,
            mesh_reports: BTreeMap::new(),
            display_triangles: crate::mesh_body::DISPLAY_TRIANGLES,
            drawing_ui: crate::drawing_state::DrawingUi::default(),
            dimension_ui: crate::dimension_state::DimensionUi::default(),
            dimension_tool: crate::dimension_tool::DimensionTool::default(),
            balloon_tool: crate::balloon_tool::BalloonTool::default(),
            section_tool: crate::section_tool::SectionTool::default(),
            dimension_dialog: None,
            sheet_dialog: None,
            surface_dialog: None,
            curvature_dialog: None,
            lost_dialog: None,
            sketch_dxf_dialog: None,
            dialog_marks: Vec::new(),
            pending_delete_dimension: None,
            document_name: tr!("state-unnamed"),
            instance: NEXT_INSTANCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            fonts: None,
            text_memory: Default::default(),
        };
        state.rebuild_scene();
        state
    }

    /// Teilt die gemerkten Werte des Werkzeugs „Text“ mit `share` (dem Speicher des
    /// Arbeitsbereichs, Spec C4: für den Programmlauf, über alle Tabs); eine offene Sitzung
    /// bekommt ihn auch.
    pub fn share_text_memory(&mut self, share: &crate::sketch_text::TextMemoryShare) {
        self.text_memory = share.clone();
        if let Some(se) = self.sketch_session.as_mut() {
            se.text_memory = share.clone();
        }
    }

    /// Der Speicher der gemerkten Werte des Werkzeugs „Text“.
    pub fn text_memory_share(&self) -> &crate::sketch_text::TextMemoryShare {
        &self.text_memory
    }

    /// Die Schriftliste; `None`, solange die Schriftordner noch durchsucht werden (Spec G4).
    pub fn fonts(&self) -> Option<&Arc<planora_text::FontLibrary>> {
        self.fonts.as_ref()
    }

    /// Gibt dem Zustand die Schriftliste (einmal, sobald sie da ist).
    pub(crate) fn set_fonts(&mut self, fonts: Arc<planora_text::FontLibrary>) {
        self.fonts = Some(fonts);
    }

    /// Setzt alle Texte des Dokuments mit der Schriftliste neu (Spec G4: nach dem Laden und nach
    /// jedem Zurückladen). `true`, wenn sich die Form geändert hat; dann ist das Dokument neu
    /// berechnet und die Körper sind neu aufgebaut, ohne die Ansicht neu einzupassen. Ohne Liste
    /// geschieht nichts. Ob das ein Schritt wird, entscheidet der Aufrufer (nie: siehe
    /// [`crate::workspace::DocTab`]).
    pub(crate) fn reshape_texts(&mut self) -> bool {
        let Some(lib) = self.fonts.clone() else {
            return false;
        };
        let changed = self
            .document
            .reshape_texts(&crate::fonts_library::Guarded(&lib));
        if changed {
            self.refresh_with(None, false);
        }
        changed
    }

    /// Farben der 3D-Szene im aktuellen Thema (Raster, Achsen, Auswahl aus dem Graphit-Theme).
    pub fn palette(&self) -> Palette {
        crate::viewport::render_palette(self.dark)
    }

    /// Palette der Szene: im Skizzen-Modus gedämpft.
    fn scene_palette(&self) -> Palette {
        if self.sketch_session.is_some() {
            self.palette().dimmed()
        } else {
            self.palette()
        }
    }

    /// Baut Körperliste, Netze, Maße und Szene aus den sichtbaren Körpern des Dokuments neu
    /// auf. Auswahl, Hervorhebung und gewählter Körper bleiben, solange ihr Körper sichtbar ist
    /// (Indizes werden auf die neue Liste abgebildet); sonst werden sie gelöscht. Die Ansicht
    /// wird nur neu eingepasst, wenn sich die Zahl der Körper geändert hat.
    pub fn refresh(&mut self) {
        self.refresh_following(None);
    }

    /// Wie [`AppState::refresh`]; Verweise auf den Körper `follow.0` gehen auf `follow.1` über
    /// (z. B. auf das Ergebnis eines neuen Verschiebens).
    fn refresh_following(&mut self, follow: Option<(BodyRef, BodyRef)>) {
        self.refresh_with(follow, true);
    }

    /// Wie [`AppState::refresh_following`]; mit `fit == false` wird die Ansicht nie neu
    /// eingepasst (Rückgängig/Wiederholen, Spec §3).
    fn refresh_with(&mut self, follow: Option<(BodyRef, BodyRef)>, fit: bool) {
        let old: Vec<BodyRef> = self.bodies.iter().map(|b| b.source).collect();
        // Geometrie der markierten Kanten vor der Neuberechnung: Ändert sich der Körper, können
        // Kanten-Ids umnummeriert sein; eine Markierung darf dann nicht still auf eine andere
        // Kante zeigen.
        let mark_keys: Vec<Option<[u64; EDGE_KEY_LEN]>> = self
            .marked_edges
            .iter()
            .map(|m| {
                self.bodies
                    .get(m.body)
                    .and_then(|b| edge_key(&b.solid, m.edge))
            })
            .collect();
        let count_before = self.bodies.len();
        let mut bodies = Vec::new();
        let mut failed = Vec::new();
        for v in self.document.visible_bodies() {
            let name = self
                .document
                .body_display_name(v.body)
                .unwrap_or_else(|| v.name.clone());
            let kind = body_kind(&self.document, v.body);
            match self.make_body(kind, name.clone(), v.body, v.solid.clone()) {
                Ok(b) => bodies.push(b),
                Err(e) => failed.push(tr!("tree-status-line", name = &name, error = e)),
            }
        }
        for v in self.document.visible_meshes() {
            let report = *self
                .mesh_reports
                .entry(v.mesh_id)
                .or_insert_with(|| check_mesh(v.mesh));
            bodies.push(crate::mesh_body::make_body(
                v.name.clone(),
                v.body,
                v.mesh,
                &v.transform,
                report,
                self.display_triangles,
            ));
        }
        self.mesh_reports
            .retain(|id, _| self.document.mesh(*id).is_some());
        self.bodies = bodies;
        let new: Vec<BodyRef> = self.bodies.iter().map(|b| b.source).collect();
        let map = |i: usize| -> Option<usize> {
            let mut r = *old.get(i)?;
            if let Some((from, to)) = follow
                && r == from
            {
                r = to;
            }
            new.iter().position(|n| *n == r)
        };
        self.selected_body = self.selected_body.and_then(map);
        self.selection = self.selection.and_then(|p| self.remap_pick(p, map));
        self.hover = self.hover.and_then(|p| self.remap_pick(p, map));
        // Markierungen bleiben nur, wenn jede Kante im selben Körper mit derselben Geometrie
        // wieder da ist; sonst ändert sich der Körper und die Markierung entfällt ganz.
        let marks: Option<Vec<EdgeRef>> = self
            .marked_edges
            .iter()
            .zip(&mark_keys)
            .map(|(m, key)| match self.remap_pick(Pick::Edge(*m), map) {
                Some(Pick::Edge(x)) => (edge_key(&self.bodies[x.body].solid, x.edge) == *key
                    && key.is_some())
                .then_some(x),
                _ => None,
            })
            .collect();
        self.marked_edges = marks.unwrap_or_default();
        if !failed.is_empty() {
            self.message = Some(failed.join("; "));
        }
        self.rebuild_scene();
        if fit && self.bodies.len() != count_before {
            self.frame_all();
        }
    }

    /// Bildet eine Auswahl auf den neuen Körperindex ab; gibt es die Fläche oder Kante im
    /// neuen Körper nicht mehr, entfällt sie.
    fn remap_pick(&self, pick: Pick, map: impl Fn(usize) -> Option<usize>) -> Option<Pick> {
        match pick {
            Pick::Face(FaceRef { body, face }) => {
                let body = map(body)?;
                // Ein Netz-Körper hat eine einzige Fläche ohne Kernel-Gegenstück.
                if self.bodies[body].kind != BodyKind::Mesh {
                    self.bodies[body].solid.faces.get(face)?;
                }
                Some(Pick::Face(FaceRef { body, face }))
            }
            Pick::Edge(EdgeRef { body, edge }) => {
                let body = map(body)?;
                self.bodies[body].solid.edges.get(edge)?;
                Some(Pick::Edge(EdgeRef { body, edge }))
            }
            Pick::Vertex { body, edge, end } => {
                let body = map(body)?;
                self.bodies[body].solid.edges.get(edge)?;
                Some(Pick::Vertex { body, edge, end })
            }
            // Eine Ebene hängt an keinem Körper; sie bleibt, solange sie gezeichnet wird.
            Pick::Plane(id) => self.plane_rect(id).map(|_| pick),
            // Eine Skizzenkurve oder ein Skizzenpunkt bleibt, solange es sie gibt.
            Pick::SketchCurve { sketch, curve } => self
                .document
                .sketch_of(sketch)
                .and_then(|sk| sk.curve(curve))
                .map(|_| pick),
            Pick::SketchPoint { sketch, point } => self
                .document
                .sketch_of(sketch)
                .and_then(|sk| sk.point_xyz(point).ok())
                .map(|_| pick),
        }
    }

    /// Wählt den Körper der Verlaufsleiste (`None`: Standard, der letzte Körper). Unabhängig
    /// von `selected_body` (Index in der Körperliste für die Eigenschaften).
    pub fn select_body(&mut self, tip: Option<FeatureId>) {
        self.history_choice = tip;
        // Eine andere Wahl verdrängt die Skizzengruppe (Skizzen 11b).
        if tip.is_some() {
            self.browser_group = None;
        }
    }

    /// Körper der Verlaufsleiste als Spitze ([`Document::body_tips`]): der gewählte, solange er
    /// eine Spitze ist; ist er in einem späteren Schritt aufgegangen (Verschieben, Vereinigen),
    /// dessen Spitze; gibt es ihn nicht mehr oder ist nichts gewählt, der letzte Körper.
    /// `None` ohne Körper. Jedes Tab hat einen eigenen Zustand und damit eine eigene Wahl.
    pub fn history_body(&self) -> Option<FeatureId> {
        let tips = self.document.body_tips();
        if let Some(c) = self.history_choice {
            if tips.contains(&c) {
                return Some(c);
            }
            if let Some(t) = tips.iter().find(|t| self.document.chain(**t).contains(&c)) {
                return Some(*t);
            }
        }
        tips.last().copied()
    }

    /// Ist der Körper `j` (Index in `bodies`) gerade ausgeblendet (selbst oder über den Ordner)?
    /// Unbekannte nicht.
    pub fn is_body_hidden(&self, j: usize) -> bool {
        self.bodies.get(j).is_some() && self.document.visibility().folders.bodies
            || self.own_body_hidden(j)
    }

    /// Steht das Auge des Körpers `j` selbst auf „aus“ (ohne den Ordner)?
    pub fn own_body_hidden(&self, j: usize) -> bool {
        self.bodies.get(j).is_some_and(|b| {
            self.document
                .visibility()
                .bodies
                .contains(&self.document.body_key(b.source))
        })
    }

    /// Blendet den Körper `j` aus oder ein (Auge im Feature-Baum). Das Modell bleibt: Ein
    /// ausgeblendeter Körper wird nicht gezeichnet und nicht gewählt; Auswahl und Hervorhebung auf
    /// ihm entfallen. Ein unbekannter Index ändert nichts.
    pub fn set_body_hidden(&mut self, j: usize, hidden: bool) {
        let Some(b) = self.bodies.get(j) else {
            return;
        };
        let key = self.document.body_key(b.source);
        let mut v = self.document.visibility().clone();
        if hidden {
            v.bodies.insert(key);
        } else {
            v.bodies.remove(&key);
        }
        self.apply_visibility(v);
    }

    /// Ist die Skizze oder Konstruktionsebene `id` gerade ausgeblendet (selbst oder über den
    /// Ordner)?
    pub fn is_feature_hidden(&self, id: FeatureId) -> bool {
        let v = self.document.visibility();
        // Je Bild für jede Skizze gerufen: das Feature nur einmal suchen, die Gruppen aus dem
        // Zwischenspeicher des Dokuments (Skizzen 12, Task 12).
        let (folder, sketch) = match self.document.feature(id).map(|f| &f.kind) {
            Some(FeatureKind::Sketch(_)) => (v.folders.sketches, true),
            Some(FeatureKind::Plane(_)) => (v.folders.planes, false),
            Some(FeatureKind::Canvas(_)) => (v.folders.canvases, false),
            _ => (false, false),
        };
        // Skizzen 11b: eine Skizze in einer ausgeblendeten Gruppe (ihr eigenes Auge bleibt).
        folder || v.features.contains(&id) || (sketch && self.document.sketch_hidden_by_group(id))
    }

    /// Die im Ordner „Skizzen“ gewählten Skizzen (Skizzen 11b): die Strg-Klick-Wahl, solange
    /// `selected_feature` darin steht, sonst die gewählte Skizze allein; nur vorhandene Skizzen.
    pub fn browser_sketch_selection(&self) -> Vec<FeatureId> {
        let is_sketch = |id: &FeatureId| {
            self.document
                .feature(*id)
                .is_some_and(|f| matches!(f.kind, FeatureKind::Sketch(_)))
        };
        let Some(sel) = self.selected_feature.filter(is_sketch) else {
            return Vec::new();
        };
        if self.browser_sketches.contains(&sel) {
            self.browser_sketches
                .iter()
                .copied()
                .filter(is_sketch)
                .collect()
        } else {
            vec![sel]
        }
    }

    /// Die im Ordner „Skizzen“ gewählte Gruppe (Skizzen 11b): nur, solange nichts anderes gewählt
    /// ist (Feature, Körper, Auswahl in der Ansicht, Zeichnung, Grundebene) und es sie gibt.
    pub fn selected_sketch_group(&self) -> Option<planora_feature::SketchGroupId> {
        self.browser_group.filter(|g| {
            self.selected_feature.is_none()
                && self.selected_body.is_none()
                && self.selection.is_none()
                && self.drawing_ui.selected_drawing.is_none()
                && self.selected_origin.is_none()
                && self.document.sketch_groups().group(*g).is_some()
        })
    }

    /// Fenster und Wahl einer Skizzengruppe, die es nicht mehr gibt, entfallen (Skizzen 11b).
    fn forget_gone_groups(&mut self) {
        let groups = self.document.sketch_groups();
        if self
            .group_rename_draft
            .as_ref()
            .is_some_and(|(g, _)| groups.group(*g).is_none())
        {
            self.group_rename_draft = None;
        }
        if self
            .browser_group
            .is_some_and(|g| groups.group(g).is_none())
        {
            self.browser_group = None;
        }
    }

    /// Strg-Klick auf die Skizzenzeile `id`: nimmt sie in die Wahl auf oder heraus.
    pub fn toggle_browser_sketch(&mut self, id: FeatureId) {
        let mut list = self.browser_sketch_selection();
        if let Some(i) = list.iter().position(|s| *s == id) {
            list.remove(i);
        } else {
            list.push(id);
        }
        self.selected_feature = list.last().copied();
        self.browser_sketches = list;
        self.browser_group = None;
        self.selected_body = None;
        self.drawing_ui.selected_drawing = None;
        self.set_selection(None);
    }

    /// Fusion „Gruppe erstellen“ (Skizzen 11b): die gewählten Skizzen und `row` (die Zeile des
    /// Menüs) kommen in eine neue Gruppe; sie wird aufgeklappt und gewählt. Ein Schritt.
    pub fn create_sketch_group_from_browser(
        &mut self,
        row: Option<FeatureId>,
    ) -> Result<planora_feature::SketchGroupId, String> {
        let mut list = self.browser_sketch_selection();
        if let Some(r) = row
            && !list.contains(&r)
        {
            list.push(r);
        }
        let items: Vec<planora_feature::GroupItem> = list
            .into_iter()
            .map(planora_feature::GroupItem::Sketch)
            .collect();
        match self.document.create_sketch_group(&items) {
            Ok(g) => {
                self.tree_groups_open.insert(g);
                self.message = None;
                Ok(g)
            }
            Err(e) => self.fail(e.to_string()),
        }
    }

    /// Fusion „Gruppe auflösen“ (auch „Löschen“ einer Gruppe): der Inhalt geht eine Ebene höher,
    /// keine Skizze wird gelöscht. Ein Schritt.
    pub fn ungroup_sketch_group(
        &mut self,
        id: planora_feature::SketchGroupId,
    ) -> Result<(), String> {
        let hidden = self
            .document
            .sketch_groups()
            .group(id)
            .is_some_and(|g| g.hidden);
        match self.document.ungroup_sketch_group(id) {
            Ok(()) => {
                self.tree_groups_open.remove(&id);
                if self.browser_group == Some(id) {
                    self.browser_group = None;
                }
                if self.group_rename_draft.as_ref().is_some_and(|d| d.0 == id) {
                    self.group_rename_draft = None;
                }
                if hidden {
                    self.sync_visibility();
                }
                Ok(())
            }
            Err(e) => self.fail(e.to_string()),
        }
    }

    /// Legt eine Skizze oder Gruppe in die Gruppe `target` (`None`: oben in den Ordner).
    pub fn move_to_sketch_group(
        &mut self,
        item: planora_feature::GroupItem,
        target: Option<planora_feature::SketchGroupId>,
    ) -> Result<(), String> {
        match self.document.move_to_group(item, target) {
            Ok(()) => {
                self.sync_visibility();
                Ok(())
            }
            Err(e) => self.fail(e.to_string()),
        }
    }

    /// Benennt die Skizzengruppe `id` um (leere Namen sind ungültig).
    pub fn rename_sketch_group(
        &mut self,
        id: planora_feature::SketchGroupId,
        name: &str,
    ) -> Result<(), String> {
        match self.document.rename_sketch_group(id, name) {
            Ok(()) => Ok(()),
            Err(e) => self.fail(e.to_string()),
        }
    }

    /// Auge der Skizzengruppe `id`: blendet alle Skizzen darin aus oder ein (ihr eigenes Auge
    /// bleibt).
    pub fn set_sketch_group_hidden(&mut self, id: planora_feature::SketchGroupId, hidden: bool) {
        let before = self.document.sketch_groups().group(id).map(|g| g.hidden);
        if before.is_none() || before == Some(hidden) {
            return;
        }
        self.document.set_sketch_group_hidden(id, hidden);
        self.sync_visibility();
    }

    /// Steht das Auge der Skizze oder Konstruktionsebene `id` selbst auf „aus“?
    pub fn own_feature_hidden(&self, id: FeatureId) -> bool {
        self.document.visibility().features.contains(&id)
    }

    /// Blendet die Skizze oder Konstruktionsebene `id` aus oder ein (Auge im Feature-Baum). Eine
    /// ausgeblendete Ebene wird nicht gezeichnet und nicht angeklickt, Auswahl und Hervorhebung
    /// auf ihr entfallen; eine ausgeblendete Skizze wird in 3D nicht gezeichnet. Andere Features
    /// und das Ausblenden der geöffneten Skizze (sie bleibt sichtbar) ändern nichts.
    pub fn set_feature_hidden(&mut self, id: FeatureId, hidden: bool) {
        let hideable = self.document.feature(id).is_some_and(|f| {
            matches!(
                f.kind,
                FeatureKind::Sketch(_) | FeatureKind::Plane(_) | FeatureKind::Canvas(_)
            )
        });
        if !hideable || (hidden && self.open_sketch_id() == Some(id)) {
            return;
        }
        let mut v = self.document.visibility().clone();
        if hidden {
            v.features.insert(id);
        } else {
            v.features.remove(&id);
        }
        self.apply_visibility(v);
    }

    /// Die Skizze der gewählten Skizzenkurve oder des gewählten Skizzenpunkts in der 3D-Ansicht
    /// (Skizzen 12).
    pub fn picked_sketch(&self) -> Option<FeatureId> {
        match self.selection {
            Some(Pick::SketchCurve { sketch, .. } | Pick::SketchPoint { sketch, .. }) => {
                Some(sketch)
            }
            _ => None,
        }
    }

    /// „Skizze bearbeiten“ im Rechtsklickmenü auf Skizzengeometrie: öffnet die Skizze der
    /// Auswahl wie ein Doppelklick im Baum. Ohne gewählte Skizzengeometrie nichts.
    pub fn edit_picked_sketch(&mut self) {
        self.sketch_pick_menu = None;
        if let Some(id) = self.picked_sketch() {
            crate::browser::edit(self, id);
        }
    }

    /// „Ausblenden“ im Rechtsklickmenü auf Skizzengeometrie: das Auge der Skizze geht aus
    /// (wie im Baum; die Auswahl entfällt dabei). Ohne gewählte Skizzengeometrie nichts.
    pub fn hide_picked_sketch(&mut self) {
        self.sketch_pick_menu = None;
        if let Some(id) = self.picked_sketch() {
            self.set_feature_hidden(id, true);
        }
    }

    /// Der Haken „Maße anzeigen“ im Rechtsklickmenü einer Skizze (Baum und 3D-Ansicht, Skizzen 12):
    /// `Some(an)` für eine fertige Skizze, `None` bei der gerade geöffneten (dort gilt der Haken
    /// im Skizzenfeld) und bei einer Kennung ohne Skizze.
    pub fn sketch_dimension_menu(&self, id: FeatureId) -> Option<bool> {
        self.document.sketch_of(id)?;
        (self.open_sketch_id() != Some(id))
            .then(|| self.document.sketch_display(id).dimensions_finished)
    }

    /// Schaltet „Maße anzeigen“ der fertigen Skizze `id` um (ein Schritt, Beschriftung
    /// „Skizzenanzeige ändern“). Bei der geöffneten Skizze und ohne Skizze geschieht nichts.
    pub fn toggle_finished_dimensions(&mut self, id: FeatureId) {
        let Some(on) = self.sketch_dimension_menu(id) else {
            return;
        };
        let mut d = self.document.sketch_display(id);
        d.dimensions_finished = !on;
        self.document.set_sketch_display(id, d);
    }

    /// Die Skizze für den belegbaren Befehl „Maße anzeigen“ der fertigen Skizze: die gewählte
    /// Skizzengeometrie in der 3D-Ansicht, sonst die im Baum gewählte Skizze.
    pub fn sketch_for_dimension_menu(&self) -> Option<FeatureId> {
        self.picked_sketch().or_else(|| {
            let id = self.selected_feature?;
            self.document.sketch_of(id).map(|_| id)
        })
    }

    /// Ist die Grundebene `p` eingeblendet (Auge im Ordner „Ursprung“)?
    pub fn is_origin_plane_shown(&self, p: StandardPlane) -> bool {
        self.document.visibility().origin_shown(p)
    }

    /// Blendet die Grundebene `p` in der 3D-Ansicht ein oder aus.
    pub fn set_origin_plane_shown(&mut self, p: StandardPlane, shown: bool) {
        let mut v = self.document.visibility().clone();
        v.set_origin_shown(p, shown);
        self.apply_visibility(v);
    }

    /// Zeigt das Auge des Ordners `f` „aus“? „Ursprung“: keine Grundebene ist eingeblendet;
    /// „Zeichnungen“ hat kein Auge.
    pub fn is_folder_hidden(&self, f: crate::browser::Folder) -> bool {
        use crate::browser::Folder;
        let v = self.document.visibility();
        match f {
            Folder::Bodies => v.folders.bodies,
            Folder::Sketches => v.folders.sketches,
            Folder::Construction => v.folders.planes,
            Folder::Canvases => v.folders.canvases,
            Folder::Origin => v.origin_planes == [false; 3],
            Folder::Drawings => false,
        }
    }

    /// Auge am Ordner `f`. Körper, Skizzen und Konstruktion: Alles darin geht aus oder an, die
    /// Augen der Zeilen bleiben stehen (wie in Fusion). „Ursprung“: Ist eine Grundebene an, gehen
    /// alle aus, sonst alle an.
    pub fn toggle_folder_hidden(&mut self, f: crate::browser::Folder) {
        use crate::browser::Folder;
        let mut v = self.document.visibility().clone();
        match f {
            Folder::Bodies => v.folders.bodies = !v.folders.bodies,
            Folder::Sketches => v.folders.sketches = !v.folders.sketches,
            Folder::Construction => v.folders.planes = !v.folders.planes,
            Folder::Canvases => v.folders.canvases = !v.folders.canvases,
            Folder::Origin => v.origin_planes = [v.origin_planes == [false; 3]; 3],
            Folder::Drawings => return,
        }
        self.apply_visibility(v);
    }

    /// Übernimmt `v` ins Dokument und lässt die Ansicht folgen.
    fn apply_visibility(&mut self, v: planora_feature::Visibility) {
        if v != *self.document.visibility() {
            self.document.set_visibility(v);
            self.sync_visibility();
        }
    }

    /// Nach einer geänderten Sichtbarkeit: Auswahl und Hervorhebung auf Ausgeblendetem entfallen,
    /// die Szene wird neu gebaut.
    fn sync_visibility(&mut self) {
        let hidden = |s: &Self, p: Option<Pick>| match p {
            Some(Pick::Plane(id)) => s.is_feature_hidden(id),
            Some(Pick::SketchCurve { sketch, .. } | Pick::SketchPoint { sketch, .. }) => {
                s.is_feature_hidden(sketch)
            }
            Some(p) => pick_body(p).is_some_and(|j| s.is_body_hidden(j)),
            None => false,
        };
        if hidden(self, self.selection) {
            self.selection = None;
        }
        if hidden(self, self.hover) {
            self.hover = None;
        }
        if self.selected_body.is_some_and(|j| self.is_body_hidden(j)) {
            self.selected_body = None;
        }
        let marks = std::mem::take(&mut self.marked_edges);
        self.marked_edges = marks
            .into_iter()
            .filter(|e| !self.is_body_hidden(e.body))
            .collect();
        self.rebuild_scene();
    }

    /// Wählt die Grundebene `p` (Klick in der Ansicht oder im Ordner „Ursprung“). Alles andere ist
    /// danach nicht mehr gewählt.
    pub fn select_origin_plane(&mut self, p: StandardPlane) {
        self.set_selection(None);
        self.selected_feature = None;
        self.selected_body = None;
        self.drawing_ui.selected_drawing = None;
        self.selected_origin = Some(p);
    }

    /// Die gewählte Grundebene; jede andere Wahl hat Vorrang.
    pub fn selected_origin_plane(&self) -> Option<StandardPlane> {
        self.selected_origin.filter(|_| {
            self.selection.is_none()
                && self.selected_feature.is_none()
                && self.selected_body.is_none()
        })
    }

    /// „Sichtbarkeit umschalten“ (Taste V): die gewählte Skizze, Konstruktions- oder Grundebene,
    /// sonst der gewählte Körper. `false`, wenn nichts davon gewählt ist.
    pub fn toggle_selected_visibility(&mut self) -> bool {
        let feature = self.selected_feature.filter(|id| {
            self.document.feature(*id).is_some_and(|f| {
                matches!(
                    f.kind,
                    FeatureKind::Sketch(_) | FeatureKind::Plane(_) | FeatureKind::Canvas(_)
                )
            })
        });
        if let Some(id) = feature {
            let hidden = self.own_feature_hidden(id);
            self.set_feature_hidden(id, !hidden);
            return true;
        }
        if let Some(p) = self.selected_origin_plane() {
            let shown = self.is_origin_plane_shown(p);
            self.set_origin_plane_shown(p, !shown);
            return true;
        }
        match self.chosen_body() {
            Some(j) => {
                let hidden = self.own_body_hidden(j);
                self.set_body_hidden(j, !hidden);
                true
            }
            None => false,
        }
    }

    /// Index des Körpers `r` in der Körperliste, falls er sichtbar ist.
    pub fn body_index(&self, r: BodyRef) -> Option<usize> {
        self.bodies.iter().position(|b| b.source == r)
    }

    /// Stand der Zeitleiste zum Speichern (eine offene Skizze wird vorher mit `try_close_sketch`
    /// ins Dokument geschrieben; das ist Sache des Aufrufers).
    pub fn snapshot(&self) -> planora_feature::Snapshot {
        self.document.snapshot()
    }

    /// Neuer Zustand aus einem geladenen Stand: Dokument aufbauen und berechnen, Namenszähler
    /// fortsetzen, Körper ableiten, Ansicht einpassen.
    pub fn from_snapshot(snapshot: planora_feature::Snapshot) -> Result<AppState, String> {
        let mut s = AppState::new();
        let dissolved = snapshot.dissolved_patterns;
        s.document =
            Document::from_snapshot(snapshot, s.chord_tolerance).map_err(|e| e.to_string())?;
        s.rebuild_name_counters();
        s.refresh();
        s.frame_all();
        // Die frische Kamera kennt das freie Rechteck noch nicht.
        s.fit_pending = true;
        // Skizzen 5: Muster in einer Schleife hat das Laden aufgelöst; die Meldung sagt es.
        if dissolved > 0 {
            s.message = Some(tr!("state-patterns-dissolved-on-load", n = dissolved));
        }
        Ok(s)
    }

    /// Ersetzt nur das Dokument durch das Bild `s` (Rückgängig/Wiederholen, Spec §6a.1, R1, R2,
    /// R9). Kamera, Hell/Dunkel, `instance` und `fit_pending` bleiben; die Ansicht wird nicht neu
    /// eingepasst.
    ///
    /// Bei Erfolg: Namenszähler aus den Namen neu; Körper neu ohne Einpassen (Weg
    /// [`RestoreKind::Full`]; bei [`RestoreKind::Annotations`] bleiben Körper und Szene, wie sie
    /// sind); auf dem vollen Weg entfallen Flächen-/Kantenwahl und Hervorhebung, gewählter Körper
    /// und gewähltes Feature bleiben, solange es sie gibt (der Körper auch über ein
    /// zurückgenommenes oder wiederholtes Verschieben, Verrunden oder eine Boolesche Operation
    /// hinweg, siehe `same_body_after`); auf dem Zeichnungsweg bleiben Auswahl,
    /// Hervorhebung und gewählter Körper unverändert; offene Rückfragen, Umbenennen-Felder, Dialogwünsche, Maß- und
    /// Blattangaben-Fenster, markierte Kanten, ein laufender Zug und die Werkzeuge des
    /// Zeichnungsbereichs werden geleert, `message` ebenso. Eine offene Skizze (kommt über die
    /// Oberfläche nicht vor, R10) wird verworfen wie beim Löschen ihres Features. Eine geöffnete
    /// Zeichnung, die es im Bild nicht gibt, schaltet auf das Modell zurück
    /// ([`crate::drawing_state::DrawingUi::forget_missing`]).
    ///
    /// Bei Fehler bleibt alles unverändert (auch `message`; die Meldung setzt der Aufrufer).
    ///
    /// Sache des Aufrufers (`DocTab`, Aufgabe 6): Die Fenster des Tabs (`feature_dialog`,
    /// `blend_dialog`, `boolean_dialog`, …) halten `FeatureId`s und Kanten des alten Dokuments
    /// und kennen diesen Zustand nicht; nach jedem gelungenen Aufruf muss
    /// `DocTab::close_dialogs()` sie schließen.
    pub fn restore_document(
        &mut self,
        s: planora_feature::Snapshot,
    ) -> Result<RestoreKind, String> {
        // Herkunft der Körper im alten Stand: Ein gewählter Körper bleibt gewählt, solange es
        // ihn aus Sicht des Nutzers noch gibt (nur ein Verschieben, Verrunden oder eine Boolesche
        // Operation an ihm wurde zurückgenommen oder wiederholt).
        let chosen = self
            .selected_body
            .and_then(|i| self.bodies.get(i))
            .map(|b| b.source);
        let old_inputs: Vec<(FeatureId, BodyRef)> = self
            .document
            .features()
            .iter()
            .filter_map(|f| carried_body(&f.kind).map(|b| (f.id, b)))
            .collect();
        let visibility_before = self.document.visibility().clone();
        let groups_before = self.document.sketch_groups().clone();
        let kind = self.document.restore(s).map_err(|e| e.to_string())?;
        self.forget_gone_groups();
        self.rebuild_name_counters();
        // Eine offene Skizze verwerfen: Ihr Entwurf (samt Verlauf) gehört zum alten Dokument.
        self.sketch_draft = None;
        self.sketch_history = None;
        let left_sketch = self.sketch_session.take().is_some();
        let had_marks = !self.marked_edges.is_empty() || !self.dialog_marks.is_empty();
        self.marked_edges.clear();
        self.edge_picking = false;
        self.plane_pick = None;
        self.pending_pick = None;
        self.calibrate = None;
        self.drag_move = None;
        self.pending_delete = None;
        self.pending_delete_drawing = None;
        self.pending_delete_dimension = None;
        self.rename_draft = None;
        self.group_rename_draft = None;
        self.rename_drawing_draft = None;
        self.edit_request = None;
        self.sketch_request = None;
        self.dimension_dialog = None;
        self.sheet_dialog = None;
        self.surface_dialog = None;
        self.curvature_dialog = None;
        self.lost_dialog = None;
        self.sketch_dxf_dialog = None;
        self.dialog_marks.clear();
        self.dimension_tool = crate::dimension_tool::DimensionTool::default();
        self.balloon_tool = crate::balloon_tool::BalloonTool::default();
        self.section_tool = crate::section_tool::SectionTool::default();
        self.message = None;
        if self
            .selected_feature
            .is_some_and(|id| self.document.feature(id).is_none())
        {
            self.selected_feature = None;
        }
        self.drawing_ui.forget_missing(&self.document);
        match kind {
            RestoreKind::Full => {
                // Ein zurückgeholter Körper kann seine Flächen und Kanten anders nummerieren:
                // Flächen-/Kantenwahl und Hervorhebung entfallen, damit sie nie auf einer gleich
                // nummerierten, aber anderen Fläche stehen. Der gewählte Körper bleibt (über
                // seinen Verweis abgebildet), solange es ihn gibt.
                self.selection = None;
                self.hover = None;
                let follow = chosen.map(|from| (from, self.same_body_after(from, &old_inputs)));
                self.refresh_with(follow, false);
            }
            // Gleiches Modell: Körper und Auswahl gelten weiter; die Szene nur neu, wenn sich
            // ihre Hervorhebung (markierte Kanten) geändert hat.
            RestoreKind::Annotations if had_marks && !left_sketch => self.rebuild_scene(),
            RestoreKind::Annotations => {}
        }
        if *self.document.visibility() != visibility_before
            || *self.document.sketch_groups() != groups_before
        {
            self.sync_visibility();
        }
        if left_sketch {
            // Wie beim Verwerfen einer Skizze: die Kamera von vor dem Skizzenmodus.
            self.restore_camera();
        }
        Ok(kind)
    }

    /// Derselbe Körper aus Sicht des Nutzers im zurückgeladenen Stand (der Verweis `from` stammt
    /// aus dem alten): Ist `from` nicht mehr sichtbar, geht es zurück zum Eingangskörper eines
    /// fehlenden Verschiebens/Verrundens/einer Booleschen Operation (`old_inputs`, alter Stand)
    /// bzw. vorwärts zum Ergebnis eines solchen Features im neuen Stand, das ihn fortführt. Ein
    /// Körper ohne solche Herkunft (etwa ein zurückgenommener Quader) bleibt `from` und
    /// entfällt dann bei der Abbildung.
    fn same_body_after(&self, from: BodyRef, old_inputs: &[(FeatureId, BodyRef)]) -> BodyRef {
        let visible: Vec<BodyRef> = self
            .document
            .visible_bodies()
            .iter()
            .map(|v| v.body)
            .chain(self.document.visible_meshes().iter().map(|v| v.body))
            .collect();
        let mut r = from;
        // Jeder Schritt führt zu einem anderen Feature; mehr Schritte als Features gibt es nicht.
        for _ in 0..=self.document.features().len() + old_inputs.len() {
            if visible.contains(&r) {
                break;
            }
            let forward = self
                .document
                .features()
                .iter()
                .find(|f| carried_body(&f.kind) == Some(r));
            if let Some(f) = forward {
                r = BodyRef {
                    feature: f.id,
                    index: 0,
                };
                continue;
            }
            let back = old_inputs
                .iter()
                .find(|(id, _)| r.index == 0 && *id == r.feature)
                .filter(|_| self.document.feature(r.feature).is_none());
            match back {
                Some((_, input)) => r = *input,
                None => break,
            }
        }
        r
    }

    /// Feature des laufenden Zugs im Eigenschaftenfenster (nur für Tests).
    #[doc(hidden)]
    pub fn drag_move(&self) -> Option<FeatureId> {
        self.drag_move
    }

    /// Setzt die Zähler je Bezeichnung aus vorhandenen Namen „Bezeichnung N“ fort, damit neue
    /// Namen nach dem Laden nicht doppelt vergeben werden.
    fn rebuild_name_counters(&mut self) {
        self.name_counters.clear();
        for f in self.document.features() {
            if let Some((label, num)) = f.name.rsplit_once(' ')
                && let Ok(n) = num.parse::<u64>()
                && n <= MAX_NAME_COUNTER
            {
                let c = self.name_counters.entry(label.to_string()).or_insert(0);
                *c = (*c).max(n);
            }
        }
    }

    /// Nächster Name `{Bezeichnung} {Nummer}` für `label`, ohne den Zähler zu erhöhen.
    fn next_name(&self, label: &str) -> String {
        let n = self
            .name_counters
            .get(label)
            .copied()
            .unwrap_or(0)
            .saturating_add(1);
        format!("{label} {n}")
    }

    /// Zählt den Namenszähler für `label` weiter (nur nach erfolgreichem Anlegen).
    fn count_name(&mut self, label: &str) {
        let c = self.name_counters.entry(label.to_string()).or_insert(0);
        *c = c.saturating_add(1);
    }

    /// Legt ein Feature `{Bezeichnung} {Nummer}` am Marker an (Bezeichnung `base_name` oder
    /// die der Feature-Art; der Zähler je Bezeichnung zählt nur hoch) und aktualisiert die
    /// Körper. Ein Feature mit dem Status `Error` wird trotzdem angelegt; seine Meldung steht
    /// dann in `message`.
    pub fn add_feature(
        &mut self,
        base_name: Option<&str>,
        kind: FeatureKind,
    ) -> Result<FeatureId, String> {
        self.add_feature_with(base_name, kind, Formulas::new())
    }

    /// Wie [`AppState::add_feature`], mit den Formeln der Zahlenfelder. Eine Formel, die sich
    /// nicht auswerten lässt, wird abgelehnt (Dokument unverändert); ein unzulässiges Ergebnis
    /// (etwa Breite 0) ergibt den Fehlerstatus mit Meldung.
    pub fn add_feature_with(
        &mut self,
        base_name: Option<&str>,
        kind: FeatureKind,
        formulas: Formulas,
    ) -> Result<FeatureId, String> {
        crate::history_bar::refuse_if_locked(self, &kind.all_dependencies())?;
        let label = base_name.map_or_else(|| kind.label(), str::to_string);
        let name = self.next_name(&label);
        match self
            .document
            .add_feature_with_formulas(name, kind, formulas)
        {
            Ok(id) => {
                self.count_name(&label);
                self.refresh();
                if let Some(FeatureStatus::Error(e)) = self.document.feature(id).map(|f| &f.status)
                {
                    self.message = Some(e.clone());
                }
                Ok(id)
            }
            Err(e) => self.fail(e.to_string()),
        }
    }

    /// Ursprung für einen neuen Grundkörper rechts neben den vorhandenen Körpern (10 mm
    /// Abstand). `extent_x` ist der Abstand vom Ursprung des Grundkörpers zu seiner linken
    /// Kante (Quader 0, sonst der Radius). Ohne Körper liegt er im Nullpunkt.
    pub fn free_origin(&self, extent_x: f64) -> DVec3 {
        match self.bodies.iter().map(|b| b.bounds.1.x).reduce(f64::max) {
            None => DVec3::ZERO,
            Some(max_x) => DVec3::new(max_x + BODY_GAP_MM + extent_x, 0.0, 0.0),
        }
    }

    /// Prüft die Maße eines Grundkörpers (endlich und größer als die Toleranz).
    fn check_sizes(&mut self, values: &[f64]) -> Result<(), String> {
        let min = self.kernel.tolerance.linear;
        if !values.iter().all(|v| v.is_finite() && *v > min) {
            return self.fail(tr!("state-create-sizes-positive"));
        }
        if values.iter().any(|v| *v > MAX_COORD_MM) {
            return self.fail(tr!(
                "state-create-sizes-max",
                max = planora_i18n::len_arg(MAX_COORD_MM, format!("{MAX_COORD_MM:e}")),
                unit = planora_i18n::unit_symbol()
            ));
        }
        Ok(())
    }

    /// Quader neben den vorhandenen Körpern.
    pub fn create_box(&mut self, size: DVec3) -> Result<FeatureId, String> {
        self.create_box_with(size, Formulas::new())
    }

    /// Wie [`AppState::create_box`], mit Formeln (`size` sind die Werte der Felder; die Formeln
    /// bestimmen ihre Felder beim Rechnen).
    pub fn create_box_with(
        &mut self,
        size: DVec3,
        formulas: Formulas,
    ) -> Result<FeatureId, String> {
        self.check_sizes(&size.to_array())?;
        let origin = self.free_origin(0.0);
        self.add_feature_with(None, FeatureKind::Box { size, origin }, formulas)
    }

    /// Zylinder neben den vorhandenen Körpern.
    pub fn create_cylinder(&mut self, radius: f64, height: f64) -> Result<FeatureId, String> {
        self.create_cylinder_with(radius, height, Formulas::new())
    }

    /// Wie [`AppState::create_cylinder`], mit Formeln.
    pub fn create_cylinder_with(
        &mut self,
        radius: f64,
        height: f64,
        formulas: Formulas,
    ) -> Result<FeatureId, String> {
        self.check_sizes(&[radius, height])?;
        let origin = self.free_origin(radius);
        self.add_feature_with(
            None,
            FeatureKind::Cylinder {
                radius,
                height,
                origin,
            },
            formulas,
        )
    }

    /// Kugel neben den vorhandenen Körpern (linke Kante am freien Rand).
    pub fn create_sphere(&mut self, radius: f64) -> Result<FeatureId, String> {
        self.create_sphere_with(radius, Formulas::new())
    }

    /// Wie [`AppState::create_sphere`], mit Formeln.
    pub fn create_sphere_with(
        &mut self,
        radius: f64,
        formulas: Formulas,
    ) -> Result<FeatureId, String> {
        self.check_sizes(&[radius])?;
        let origin = self.free_origin(radius);
        self.add_feature_with(None, FeatureKind::Sphere { radius, origin }, formulas)
    }

    /// Kegel neben den vorhandenen Körpern.
    pub fn create_cone(&mut self, radius: f64, height: f64) -> Result<FeatureId, String> {
        self.create_cone_with(radius, height, Formulas::new())
    }

    /// Wie [`AppState::create_cone`], mit Formeln.
    pub fn create_cone_with(
        &mut self,
        radius: f64,
        height: f64,
        formulas: Formulas,
    ) -> Result<FeatureId, String> {
        self.check_sizes(&[radius, height])?;
        let origin = self.free_origin(radius);
        self.add_feature_with(
            None,
            FeatureKind::Cone {
                radius,
                height,
                origin,
            },
            formulas,
        )
    }

    /// Legt eine Boolesche Operation `a op b` (Körperindizes) an. Das Ergebnis ersetzt beide
    /// Körper an der Stelle des vorderen und wird gewählt. Schlägt die Operation fehl, wird das
    /// Feature wieder entfernt: Dokument und Körper bleiben unverändert, die Meldung steht in
    /// `message`.
    pub fn add_boolean(&mut self, a: usize, b: usize, op: BoolOp) -> Result<FeatureId, String> {
        if a == b || a >= self.bodies.len() || b >= self.bodies.len() {
            return self.fail(tr!("state-pick-two-bodies"));
        }
        if self.is_mesh(a) || self.is_mesh(b) {
            return self.fail(mesh_locked());
        }
        let kind = FeatureKind::Boolean {
            target: self.bodies[a].source,
            tool: self.bodies[b].source,
            op,
        };
        // Name nach der Art („Boolesche Operation N“); die Operation zeigen Baum und
        // Eigenschaften.
        let id = self.add_checked(&kind.label(), kind)?;
        self.hover = None;
        self.selection = None;
        self.refresh();
        self.selected_body = self.body_index(BodyRef {
            feature: id,
            index: 0,
        });
        Ok(id)
    }

    /// Legt eine Rundung oder Fase an den Kanten `edges` des Körpers `body` an. Das Ergebnis
    /// ersetzt den Körper (Name bleibt) und wird gewählt. Schlägt sie fehl, wird das Feature
    /// wieder entfernt: Dokument und Körper bleiben unverändert, die Meldung steht in `message`.
    pub fn add_blend(
        &mut self,
        body: usize,
        edges: &[Id<Edge>],
        art: BlendArt,
        tangentenkette: bool,
    ) -> Result<FeatureId, String> {
        self.add_blend_first(body, edges, art, tangentenkette, None)
    }

    /// Wie [`AppState::add_blend`]; `first_face` ist die Rolle der Fläche 1 (bei Fasen mit zwei
    /// Abständen oder Winkel), `None`: die erste Fläche der Kante.
    pub fn add_blend_first(
        &mut self,
        body: usize,
        edges: &[Id<Edge>],
        art: BlendArt,
        tangentenkette: bool,
        first_face: Option<&str>,
    ) -> Result<FeatureId, String> {
        self.add_blend_with(
            body,
            edges,
            art,
            tangentenkette,
            first_face,
            Formulas::new(),
        )
    }

    /// Wie [`AppState::add_blend_first`], mit Formeln (Radius, Abstände, Winkel).
    pub fn add_blend_with(
        &mut self,
        body: usize,
        edges: &[Id<Edge>],
        art: BlendArt,
        tangentenkette: bool,
        first_face: Option<&str>,
        formulas: Formulas,
    ) -> Result<FeatureId, String> {
        if self.is_mesh(body) {
            return self.fail(mesh_locked());
        }
        let Some(b) = self.bodies.get(body) else {
            return self.fail(tr!("state-body-not-found"));
        };
        if edges.is_empty() {
            return self.fail(tr!("state-pick-edge-first"));
        }
        let tol = self.kernel.tolerance;
        let mut refs = Vec::with_capacity(edges.len());
        for &e in edges {
            match planora_feature::EdgeRef::from_edge(&b.solid, e, &tol) {
                Ok(r) => refs.push(r),
                Err(err) => return self.fail(err.to_string()),
            }
        }
        if let Some(role) = first_face {
            crate::blend_dialog::set_first_face(&mut refs, role);
        }
        let source = b.source;
        self.try_close_sketch()?;
        let kind = FeatureKind::Verrunden {
            body: source,
            edges: refs,
            art,
            tangentenkette,
        };
        let id = self.add_checked_with(&kind.label(), kind, formulas)?;
        self.hover = None;
        self.selection = None;
        self.refresh();
        self.selected_body = self.body_index(BodyRef {
            feature: id,
            index: 0,
        });
        Ok(id)
    }

    /// Legt ein Feature an und nimmt es wieder heraus, wenn es nicht berechnet werden konnte
    /// (die Meldung steht dann in `message`). Aktualisiert die Körper nicht.
    fn add_checked(&mut self, label: &str, kind: FeatureKind) -> Result<FeatureId, String> {
        self.add_checked_with(label, kind, Formulas::new())
    }

    /// Wie [`AppState::add_checked`], mit Formeln.
    fn add_checked_with(
        &mut self,
        label: &str,
        kind: FeatureKind,
        formulas: Formulas,
    ) -> Result<FeatureId, String> {
        let id = self.add_checked_named(self.next_name(label), kind, formulas)?;
        self.count_name(label);
        Ok(id)
    }

    /// Wie [`AppState::add_checked_with`], mit festem Namen (ohne Zähler).
    fn add_checked_named(
        &mut self,
        name: String,
        kind: FeatureKind,
        formulas: Formulas,
    ) -> Result<FeatureId, String> {
        crate::history_bar::refuse_if_locked(self, &kind.all_dependencies())?;
        let id = match self
            .document
            .add_feature_with_formulas(name, kind, formulas)
        {
            Ok(id) => id,
            Err(e) => return self.fail(e.to_string()),
        };
        match self.document.feature(id).map(|f| f.status.clone()) {
            Some(FeatureStatus::Ok) => Ok(id),
            status => {
                let _ = self.document.delete_with_dependents(id);
                self.fail(status_message(status))
            }
        }
    }

    /// Verschiebt den Körper `index` um `delta` (Millimeter) **zusätzlich** zur derzeitigen
    /// Lage (Entscheidung F9). Ist der Körper das Ergebnis eines Verschiebens und liest ihn
    /// danach kein anderes Feature (siehe [`AppState::read_later`]) und hat es keine Formeln
    /// (P11), wird dessen Verschiebung erhöht; sonst entsteht ein neues Verschieben am Marker.
    /// Auswahl
    /// und gewählter Körper gehen auf das Ergebnis über. Bei einem Fehler bleibt alles
    /// unverändert. Eine offene Skizze wird vorher beendet (ihr Entwurf übernommen), damit
    /// Dokument und Entwurf nicht auseinanderlaufen.
    pub fn move_body(&mut self, index: usize, delta: DVec3) -> Result<(), String> {
        self.drag_move = None;
        self.move_body_with(index, delta, false)
    }

    /// Wie [`AppState::move_body`] während eines Zugs (Ziehen der Position im
    /// Eigenschaftenfenster): Ein Verschieben, das derselbe Zug angelegt hat, wird erweitert,
    /// auch wenn ein späteres Feature den Körper liest. Der Zug wirkt damit wie eine einzige
    /// Verschiebung um die Gesamtstrecke, statt je Bild ein neues Verschieben anzulegen.
    /// [`AppState::end_drag`] beendet den Zug.
    pub fn move_body_dragging(&mut self, index: usize, delta: DVec3) -> Result<(), String> {
        self.move_body_with(index, delta, true)
    }

    /// Beendet einen Zug (siehe [`AppState::move_body_dragging`]).
    pub fn end_drag(&mut self) {
        self.drag_move = None;
    }

    fn move_body_with(&mut self, index: usize, delta: DVec3, dragging: bool) -> Result<(), String> {
        if !delta.is_finite() {
            return self.fail(tr!("state-invalid-move"));
        }
        let Some(r) = self.bodies.get(index).map(|b| b.source) else {
            return self.fail(tr!("state-body-not-found"));
        };
        if delta == DVec3::ZERO {
            return Ok(());
        }
        // Auch das Erweitern eines vorhandenen Verschiebens ist eine neue Aktion am Körper.
        crate::history_bar::refuse_if_locked(self, &[r.feature])?;
        // Netz-Körper verschieben sich über ihre eigene Lage (Plan P10), ohne Feature.
        if self.is_mesh(index) {
            return self.move_mesh_body(r, delta);
        }
        // Beim Übernehmen des Entwurfs kann sich die Körperliste ändern (etwa wenn eine
        // Extrusion der Skizze danach fehlschlägt): den Körper über seinen Verweis neu suchen.
        self.try_close_sketch()?;
        let Some((index, (lo, hi))) = self.body_index(r).map(|i| (i, self.bodies[i].bounds)) else {
            return self.fail(tr!("state-body-invisible"));
        };
        // Der Körper muss danach im darstellbaren Bereich liegen; das deckt auch Summen ab, die
        // beim Erweitern eines Verschiebens nicht mehr endlich wären.
        let inside = |p: DVec3| p.is_finite() && p.abs().max_element() <= MAX_COORD_MM;
        if !(inside(lo + delta) && inside(hi + delta)) {
            return self.fail(tr!(
                "state-move-rejected",
                max = planora_i18n::len_arg(MAX_COORD_MM, format!("{MAX_COORD_MM:e}")),
                unit = planora_i18n::unit_symbol()
            ));
        }
        // Ein Verschieben mit Formeln wird nie erweitert (P11): Die Formeln bleiben gültig, die
        // neue Strecke bekommt ein eigenes Verschieben.
        let extendable = r.index == 0
            && self.document.feature(r.feature).is_some_and(|f| {
                matches!(f.kind, FeatureKind::Move { .. }) && f.formulas().is_empty()
            })
            && (!self.read_later(index) || (dragging && self.drag_move == Some(r.feature)));
        let moved = if extendable {
            self.extend_move(r.feature, delta)?;
            r
        } else {
            let id = self.add_checked(
                &tr!("state-move-label"),
                FeatureKind::Move { body: r, delta },
            )?;
            if dragging {
                self.drag_move = Some(id);
            }
            BodyRef {
                feature: id,
                index: 0,
            }
        };
        self.refresh_following(Some((r, moved)));
        Ok(())
    }

    /// Liest ein Feature nach dem erzeugenden Feature des Körpers `index` (auch hinter dem
    /// Marker) diesen Körper? Gelesen wird über einen Verweis auf das erzeugende Feature selbst
    /// oder über die Flächenmarken des Körpers: Skizzen auf Flächen und Kantenkopien verweisen
    /// auf die Herkunft der Marke (etwa den Quader), nicht auf das Verschieben.
    fn read_later(&self, index: usize) -> bool {
        let Some(body) = self.bodies.get(index) else {
            return false;
        };
        let mut origins: Vec<FeatureId> = body
            .solid
            .faces
            .iter()
            .filter_map(|(_, f)| f.tag.as_ref().map(|t| FeatureId(t.origin)))
            .collect();
        origins.push(body.source.feature);
        let Some(start) = self.document.index_of(body.source.feature) else {
            return false;
        };
        // Auch Features hinter dem Marker zählen: Sie wanderten sonst beim Vorrücken des Markers
        // mit dem erweiterten Verschieben mit.
        self.document.features()[start + 1..].iter().any(|f| {
            f.kind
                .all_dependencies()
                .iter()
                .any(|d| origins.contains(d))
        })
    }

    /// Erhöht die Verschiebung des Features `id` um `delta`. Ist das Verschieben danach nicht
    /// berechenbar oder schlägt dadurch ein späteres, bisher fehlerfreies Feature fehl, wird
    /// die alte Verschiebung zurückgeschrieben: Das Dokument bleibt dann unverändert.
    fn extend_move(&mut self, id: FeatureId, delta: DVec3) -> Result<(), String> {
        let (old, old_formulas) = match self.document.feature(id) {
            Some(f) => match &f.kind {
                FeatureKind::Move { delta, .. } => (*delta, f.formulas().clone()),
                _ => return self.fail(tr!("state-move-gone")),
            },
            None => return self.fail(tr!("state-move-gone")),
        };
        let was_ok: Vec<FeatureId> = self
            .document
            .features()
            .iter()
            .filter(|f| f.status == FeatureStatus::Ok)
            .map(|f| f.id)
            .collect();
        let set = |k: &mut FeatureKind, to: DVec3| {
            if let FeatureKind::Move { delta, .. } = k {
                *delta = to;
            }
        };
        if let Err(e) = self.document.update(id, |k| set(k, old + delta)) {
            return self.fail(e.to_string());
        }
        let broken = self
            .document
            .features()
            .iter()
            .find(|f| was_ok.contains(&f.id) && f.status != FeatureStatus::Ok)
            .map(|f| (f.id, f.status.clone()));
        match broken {
            None => Ok(()),
            Some((broken_id, status)) => {
                // Zurückschreiben samt Formeln (nie still eine Formel verlieren).
                let _ = self
                    .document
                    .update_with_formulas(id, |k| set(k, old), old_formulas);
                let message = status_message(Some(status));
                if broken_id == id {
                    self.fail(message)
                } else {
                    let name = self
                        .document
                        .feature(broken_id)
                        .map_or_else(String::new, |f| f.name.clone());
                    self.fail(tr!("state-move-discarded", name = name, message = message))
                }
            }
        }
    }

    /// Anzeigename des Körpers `r` (auch wenn er gerade nicht sichtbar ist); ohne Feature
    /// „Feature {Id}“.
    pub fn body_ref_name(&self, r: BodyRef) -> String {
        self.document
            .body_display_name(r)
            .unwrap_or_else(|| tr!("state-feature-n", n = r.feature.0))
    }

    /// Das Feature, das den Namen des Körpers `index` trägt: das erzeugende Feature, bei
    /// verschobenen Körpern das Feature, das den Körper ursprünglich erzeugt hat.
    pub fn naming_feature(&self, index: usize) -> Option<FeatureId> {
        let r = self.bodies.get(index)?.source;
        Some(self.document.naming_origin(r).feature)
    }

    /// Benennt ein Feature um (leere Namen sind ungültig) und aktualisiert die Körpernamen.
    pub fn rename_feature(&mut self, id: FeatureId, name: &str) -> Result<(), String> {
        match self.document.rename(id, name) {
            Ok(()) => {
                for i in 0..self.bodies.len() {
                    if let Some(n) = self.document.body_display_name(self.bodies[i].source) {
                        self.bodies[i].name = n;
                    }
                }
                Ok(())
            }
            Err(e) => self.fail(e.to_string()),
        }
    }

    /// Vor einer Änderung am Dokument aus dem Feature-Baum (R12): Betrifft sie die geöffnete
    /// Skizze selbst (`target`), bleibt alles unverändert und die Meldung bittet, die Skizze
    /// zuerst zu beenden; sonst wird eine offene Skizze beendet und ihr Entwurf übernommen.
    pub fn end_sketch_for(&mut self, target: Option<FeatureId>) -> Result<(), String> {
        if target.is_some() && target == self.open_sketch_id() {
            return self.fail(tr!("state-end-sketch-first"));
        }
        self.try_close_sketch()
    }

    /// Vor dem Löschen von `id`: Eine offene Skizze, die dabei mitgelöscht wird, wird verworfen
    /// (siehe [`AppState::forget_deleted`]); jede andere wird beendet und übernommen.
    /// Bleibt die Skizze offen (Entwurf abgelehnt), liefert es den Fehler.
    pub(crate) fn end_sketch_before_delete(&mut self, id: FeatureId) -> Result<(), String> {
        if let Some(open) = self.open_sketch_id()
            && (open == id || self.document.dependents(id).contains(&open))
        {
            return Ok(());
        }
        self.try_close_sketch()
    }

    /// Löscht ein Feature, von dem nichts abhängt. Hängen andere Features davon ab, bleibt
    /// alles unverändert und der Fehlertext nennt die abhängigen Features. Eine offene Skizze
    /// wird vorher beendet, außer sie selbst wird gelöscht.
    pub fn delete_feature(&mut self, id: FeatureId) -> Result<(), String> {
        if self.document.feature(id).is_some() {
            self.end_sketch_before_delete(id)?;
        }
        match self.document.delete(id) {
            Ok(()) => {
                self.forget_deleted();
                self.refresh();
                Ok(())
            }
            Err(FeatureError::HasDependents(ids)) => {
                let name = self
                    .document
                    .feature(id)
                    .map_or_else(String::new, |f| f.name.clone());
                let names: Vec<String> = ids
                    .iter()
                    .filter_map(|d| self.document.feature(FeatureId(*d)))
                    .map(|f| tr!("tree-quoted-name", name = &f.name))
                    .collect();
                self.fail(tr!(
                    "state-delete-blocked",
                    name = name,
                    list = names.join(", ")
                ))
            }
            Err(e) => self.fail(e.to_string()),
        }
    }

    /// Löscht ein Feature samt allen abhängigen Features; liefert die entfernten Ids. Eine
    /// offene Skizze wird vorher beendet, außer sie wird mitgelöscht.
    pub fn delete_feature_with_dependents(
        &mut self,
        id: FeatureId,
    ) -> Result<Vec<FeatureId>, String> {
        if self.document.feature(id).is_some() {
            self.end_sketch_before_delete(id)?;
        }
        match self.document.delete_with_dependents(id) {
            Ok(removed) => {
                self.forget_deleted();
                self.refresh();
                Ok(removed)
            }
            Err(e) => self.fail(e.to_string()),
        }
    }

    /// Löscht das Feature, das den Körper erzeugt hat, samt seiner Verschiebungen. Bauen
    /// andere Features darauf auf, bleibt alles unverändert und die Meldung nennt sie (in der
    /// Oberfläche läuft Löschen über den Feature-Baum mit Rückfrage). Eine offene Skizze wird
    /// vorher beendet.
    pub fn delete_body(&mut self, index: usize) -> Result<(), String> {
        let Some(r) = self.bodies.get(index).map(|b| b.source) else {
            return self.fail(tr!("state-body-not-found"));
        };
        // Beim Übernehmen des Entwurfs kann sich die Körperliste ändern.
        self.try_close_sketch()?;
        let Some(id) = self.body_index(r).and_then(|i| self.naming_feature(i)) else {
            return self.fail(tr!("state-body-not-found"));
        };
        let only_moves = self.document.dependents(id).iter().all(|d| {
            self.document
                .feature(*d)
                .is_some_and(|f| matches!(f.kind, FeatureKind::Move { .. }))
        });
        if only_moves {
            self.delete_feature_with_dependents(id).map(|_| ())
        } else {
            self.delete_feature(id)
        }
    }

    /// Netz, Masse und Hülle eines sichtbaren Körpers berechnen.
    fn make_body(
        &self,
        kind: BodyKind,
        name: String,
        source: BodyRef,
        solid: Solid,
    ) -> Result<Body, String> {
        let mesh = self.mesh_of(&solid)?;
        let mass = mass_properties(&mesh);
        let bounds = bounds_of(&mesh);
        let mesh_inexact = !mesh.inexact_faces.is_empty() || !is_watertight(&mesh, WATERTIGHT_EPS);
        Ok(Body {
            name,
            kind,
            source,
            solid,
            mesh,
            mass,
            bounds,
            mesh_inexact,
            mesh_report: None,
            preview: false,
        })
    }

    fn mesh_of(&self, solid: &Solid) -> Result<Mesh, String> {
        tessellate(
            solid,
            &TessellationOptions {
                chord_tolerance: self.chord_tolerance,
            },
        )
        .map_err(|e| tr!("state-tessellation-failed", error = e.to_string()))
    }

    pub fn rebuild_scene(&mut self) {
        // Ausgeblendete Körper behalten ihren Platz (die Indizes der Auswahl bleiben gültig),
        // tragen aber kein Netz bei.
        let empty = Mesh::default();
        let meshes: Vec<&Mesh> = self
            .bodies
            .iter()
            .enumerate()
            .map(|(j, b)| {
                if self.is_body_hidden(j) {
                    &empty
                } else {
                    &b.mesh
                }
            })
            .collect();
        let highlight = Highlight {
            hover: self.hover,
            selected: self.selection,
        };
        let mut marked: Vec<Pick> = self.marked_edges.iter().map(|e| Pick::Edge(*e)).collect();
        // Flächen und Kanten, die ein offener Dialog gewählt hat (Task H3).
        marked.extend(self.dialog_marks.iter().filter_map(|m| match *m {
            IncludePick::Face(body, face) => Some(Pick::Face(FaceRef { body, face })),
            IncludePick::Edge(body, edge) => Some(Pick::Edge(EdgeRef { body, edge })),
            _ => None,
        }));
        self.scene = Arc::new(build_scene_marked(
            &meshes,
            &highlight,
            &marked,
            &self.scene_palette(),
        ));
        self.scene_version = next_scene_version();
    }

    /// Wie [`AppState::frame_all`], nur für die Körper `indices` (Indizes in `bodies`; unbekannte
    /// zählen nicht). Ohne einen solchen Körper bleibt die Kamera, wie sie ist.
    pub fn frame_bodies(&mut self, indices: &[usize]) {
        let mut iter = indices
            .iter()
            .filter_map(|&i| self.bodies.get(i))
            .map(|b| b.bounds);
        let Some(first) = iter.next() else {
            return;
        };
        let (min, max) = iter.fold(first, |(lo, hi), (a, b)| (lo.min(a), hi.max(b)));
        self.camera.frame_bounds(to_vec3(min), to_vec3(max));
    }

    /// Richtet die Kamera auf alle Körper aus (ohne Körper: Standardansicht auf den Ursprung).
    pub fn frame_all(&mut self) {
        let mut iter = self.bodies.iter().map(|b| b.bounds);
        let Some(first) = iter.next() else {
            self.camera
                .frame_bounds(Vec3::splat(-25.0), Vec3::splat(25.0));
            return;
        };
        let (min, max) = iter.fold(first, |(lo, hi), (a, b)| (lo.min(a), hi.max(b)));
        self.camera.frame_bounds(
            Vec3::new(min.x as f32, min.y as f32, min.z as f32),
            Vec3::new(max.x as f32, max.y as f32, max.z as f32),
        );
    }

    pub fn set_hover(&mut self, hover: Option<Pick>) {
        if self.hover != hover {
            self.hover = hover;
            self.rebuild_scene();
        }
    }

    pub fn set_selection(&mut self, selection: Option<Pick>) {
        self.selected_origin = None;
        // Jede Wahl in der Ansicht (auch ihr Aufheben) verdrängt die Skizzengruppe (Skizzen 11b);
        // die Gruppenzeile setzt ihre Wahl erst danach.
        self.browser_group = None;
        if self.selection != selection {
            self.selection = selection;
            // Eine Ebene wählt ihr Feature (wie ein Klick im Browser), alles andere den Körper.
            match selection {
                Some(Pick::Plane(id)) => {
                    self.selected_feature = Some(id);
                    self.selected_body = None;
                }
                Some(p) => {
                    self.selected_feature = None;
                    self.selected_body = pick_body(p);
                }
                None => {}
            }
            self.rebuild_scene();
        }
    }

    /// Markiert die Kante `e` (mit `chain_marking` ihre Tangentenkette). Ohne `extend` ersetzt
    /// die Gruppe die Markierung (dieselbe Gruppe noch einmal: leeren); mit `extend` kommt sie
    /// dazu oder wird entfernt, wenn sie schon ganz markiert ist. Kanten eines anderen Körpers
    /// ersetzen die Markierung.
    pub fn toggle_edge_mark(&mut self, e: EdgeRef, extend: bool) {
        // Unbekannter Körper oder unbekannte Kante: nichts ändern (bestehende Markierung bleibt).
        let Some(b) = self.bodies.get(e.body) else {
            return;
        };
        if b.solid.edges.get(e.edge).is_none() {
            return;
        }
        let group: Vec<EdgeRef> = if self.chain_marking {
            planora_kernel::blend::chain::tangent_chain(&b.solid, e.edge)
                .into_iter()
                .map(|edge| EdgeRef { body: e.body, edge })
                .collect()
        } else {
            vec![e]
        };
        let all_marked = group.iter().all(|g| self.marked_edges.contains(g));
        if !extend || self.marked_edges.iter().any(|m| m.body != e.body) {
            self.marked_edges = if all_marked && self.marked_edges.len() == group.len() {
                Vec::new()
            } else {
                group
            };
        } else if all_marked {
            self.marked_edges.retain(|m| !group.contains(m));
        } else {
            for g in group {
                if !self.marked_edges.contains(&g) {
                    self.marked_edges.push(g);
                }
            }
        }
        self.selected_body = Some(e.body);
        self.rebuild_scene();
    }

    /// Schaltet die Tangentenkette beim Markieren um. Wird sie eingeschaltet, werden die schon
    /// markierten Kanten sofort um ihre Ketten ergänzt (die Vorschau gleicht dem, was der Kernel
    /// rechnet); beim Ausschalten bleibt die Markierung, wie sie ist.
    pub fn set_chain_marking(&mut self, on: bool) {
        let turned_on = on && !self.chain_marking;
        self.chain_marking = on;
        if !turned_on || self.marked_edges.is_empty() {
            return;
        }
        let mut all = self.marked_edges.clone();
        for m in &self.marked_edges {
            let Some(b) = self.bodies.get(m.body) else {
                continue;
            };
            for edge in planora_kernel::blend::chain::tangent_chain(&b.solid, m.edge) {
                let r = EdgeRef { body: m.body, edge };
                if !all.contains(&r) {
                    all.push(r);
                }
            }
        }
        if all.len() != self.marked_edges.len() {
            self.marked_edges = all;
            self.rebuild_scene();
        }
    }

    /// Löscht die Markierung.
    pub fn clear_edge_marks(&mut self) {
        if !self.marked_edges.is_empty() {
            self.marked_edges.clear();
            self.rebuild_scene();
        }
    }

    /// Was liegt unter dem Mauszeiger? `cursor` in Pixeln (Ursprung oben links) im Viewport der Größe `viewport`.
    /// Kanten haben Vorrang, wenn sie nahe genug und nicht verdeckt sind. Konstruktionsebenen
    /// zählen mit, Eckpunkte nicht ([`AppState::pick_with`] mit [`PickFilter::SELECT`]).
    pub fn pick(&self, cursor: Vec2, viewport: Vec2) -> Option<Pick> {
        self.pick_with(cursor, viewport, PickFilter::SELECT)
    }

    /// Was ein Klick in der 3D-Ansicht trifft: beim Markieren von Kanten (Dialoge Abrunden und
    /// Fase, [`AppState::edge_picking`]) nur Flächen und Kanten, damit keine Ebene die Kanten
    /// dahinter verdeckt; sonst wie [`AppState::pick`].
    pub fn view_pick(&self, cursor: Vec2, viewport: Vec2) -> Option<Pick> {
        let filter = if self.edge_picking {
            PickFilter::GEOMETRY
        } else {
            PickFilter::SELECT
        };
        self.pick_with(cursor, viewport, filter)
    }

    /// Wird die Ebene `id` hervorgehoben gezeichnet? Gewählt in der Ansicht, im Browser
    /// (`selected_feature`) oder unter dem Mauszeiger.
    pub fn plane_highlighted(&self, id: FeatureId) -> bool {
        let this = Some(Pick::Plane(id));
        self.selection == this
            || self.hover == this
            || self.selected_feature == Some(id)
            || self.dialog_marked(&IncludePick::Plane(planora_feature::PlaneId::Feature(id)))
    }

    /// Hat ein Feld des offenen Dialogs `pick` gewählt ([`AppState::dialog_marks`])?
    pub fn dialog_marked(&self, pick: &IncludePick) -> bool {
        self.dialog_marks.contains(pick)
    }

    /// Setzt die Hervorhebungsliste der Ansicht ([`AppState::dialog_marks`]); baut die Szene nur
    /// neu, wenn sich etwas geändert hat.
    pub fn set_dialog_marks(&mut self, marks: Vec<IncludePick>) {
        if self.dialog_marks != marks {
            self.dialog_marks = marks;
            self.rebuild_scene();
        }
    }

    /// Wie [`AppState::pick`], mit Wahl, was zählt ([`PickFilter`]). Reihenfolge: Eckpunkt
    /// (nur mit `filter.vertices`), Kante (mit `filter.edges`), dann das Vordere aus Fläche und
    /// Ebene (eine ausgeschaltete Fläche ergibt nichts, verdeckt aber weiter). Ob eine Ebene
    /// Ecken und Kanten dahinter verdeckt, hängt vom Filter ab: bei `planes: true` verdeckt sie
    /// wie eine Fläche (liegt eine Fläche näher, gewinnt diese); mit `planes: false` (etwa
    /// `PickFilter::VERTICES`) zählen Ebenen nicht und verdecken nichts. Bei Slice
    /// ([`AppState::active_slice`]) zählt von den Körpern nur, was hinter der Skizzenebene liegt.
    pub fn pick_with(&self, cursor: Vec2, viewport: Vec2, filter: PickFilter) -> Option<Pick> {
        if viewport.x < 1.0 || viewport.y < 1.0 {
            return None;
        }
        let nearest_face = self.nearest_face_hit(cursor, viewport);
        // Slice: weggeschnittene Kanten(-teile) und Ecken zählen nicht und verdrängen keine
        // sichtbaren (die Flächen schon in `nearest_face_hit`).
        let slice = self.active_slice();
        let nearest_plane = if filter.planes {
            self.nearest_plane_hit(cursor, viewport)
        } else {
            None
        };
        let plane_distance = nearest_plane.map(|(_, d)| d as f32);
        let occluder = match (nearest_face.map(|(_, h)| h.distance), plane_distance) {
            (Some(f), Some(p)) => Some(f.min(p)),
            (f, p) => f.or(p),
        };

        let shown = || {
            self.bodies
                .iter()
                .enumerate()
                .filter(|(j, _)| !self.is_body_hidden(*j))
        };
        if filter.vertices {
            let meshes: Vec<(usize, &Mesh)> = shown().map(|(j, b)| (j, &b.mesh)).collect();
            if let Some((body, edge, end)) = pick_vertex_sliced(
                &meshes,
                &self.camera,
                viewport,
                cursor,
                PICK_VERTEX_PX,
                occluder,
                slice,
            ) {
                return Some(Pick::Vertex { body, edge, end });
            }
        }
        let mut best_edge: Option<(usize, planora_render::EdgeHit)> = None;
        for (body, b) in shown().filter(|_| filter.edges) {
            if let Some(hit) = pick_edge_sliced(
                &b.mesh,
                &self.camera,
                viewport,
                cursor,
                PICK_EDGE_PX,
                occluder,
                slice,
            ) && best_edge.is_none_or(|(_, h)| hit.pixel_distance < h.pixel_distance)
            {
                best_edge = Some((body, hit));
            }
        }
        if let Some((body, hit)) = best_edge {
            return Some(Pick::Edge(EdgeRef {
                body,
                edge: hit.edge,
            }));
        }
        // Skizzenpunkt, dann Skizzenkurve einer fertigen, sichtbaren Skizze (Skizzen 12); was ein
        // Körper verdeckt, zählt nicht.
        if filter.sketch_points || filter.sketch_curves {
            let map = crate::sketch_view::Mapper {
                camera: self.camera,
                frame: planora_math::Frame::WORLD,
                rect: egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(viewport.x, viewport.y),
                ),
            };
            if let Some(hit) = crate::sketch_view::finished_sketch_under(
                self,
                &map,
                egui::pos2(cursor.x, cursor.y),
                slice,
                filter.sketch_points,
                filter.sketch_curves,
            ) {
                return Some(hit);
            }
        }
        match (nearest_face, nearest_plane) {
            (Some((_, f)), Some((id, p))) if p < f.distance as f64 => Some(Pick::Plane(id)),
            (None, Some((id, _))) => Some(Pick::Plane(id)),
            (face, _) => face.filter(|_| filter.faces).map(|(body, hit)| {
                Pick::Face(FaceRef {
                    body,
                    face: hit.face,
                })
            }),
        }
    }

    /// Die vorderste gezeichnete Konstruktionsebene unter dem Zeiger und die Strahllänge bis zu
    /// ihr (gleiches Maß wie [`planora_render::FaceHit::distance`]).
    fn nearest_plane_hit(&self, cursor: Vec2, viewport: Vec2) -> Option<(FeatureId, f64)> {
        let ndc = Vec2::new(
            2.0 * cursor.x / viewport.x - 1.0,
            1.0 - 2.0 * cursor.y / viewport.y,
        );
        let (origin, dir) = self.camera.ray(ndc);
        let ray = Ray {
            origin: origin.as_dvec3(),
            dir: dir.as_dvec3(),
        };
        self.plane_rects()
            .into_iter()
            .filter_map(|(id, rect)| pick_plane(&ray, &rect).map(|t| (id, t)))
            .min_by(|a, b| a.1.total_cmp(&b.1))
    }

    /// Alle gezeichneten Konstruktionsebenen in der Reihenfolge des Verlaufs (siehe
    /// [`AppState::plane_rect`]); im Baum ausgeblendete fehlen.
    pub fn plane_rects(&self) -> Vec<(FeatureId, PlaneRect)> {
        self.document
            .features()
            .iter()
            .filter(|f| matches!(f.kind, FeatureKind::Plane(_)))
            .filter(|f| !self.is_feature_hidden(f.id))
            .filter_map(|f| self.plane_rect(f.id).map(|r| (f.id, r)))
            .collect()
    }

    /// Die eingeblendeten Grundebenen als Rechtecke im Ursprung, so groß wie
    /// Konstruktionsebenen ([`AppState::plane_rect_for`]). In einem Wahlmodus ohne Fenster
    /// ([`PendingPick`]: „Skizze erstellen“, „Skizzenebene neu festlegen“) alle drei, auch
    /// ausgeblendete, wie in Fusion; die Sichtbarkeit im Dokument bleibt dabei unverändert.
    pub fn origin_plane_rects(&self) -> Vec<(StandardPlane, PlaneRect)> {
        let picking = self.pending_pick.is_some();
        [StandardPlane::XY, StandardPlane::XZ, StandardPlane::YZ]
            .into_iter()
            .filter(|p| picking || self.is_origin_plane_shown(*p))
            .filter_map(|p| {
                let def = PlaneDef::Offset {
                    base: PlaneBase::Standard(p),
                    distance: 0.0,
                };
                self.plane_rect_for(&def, p.frame()).map(|r| (p, r))
            })
            .collect()
    }

    /// Die vorderste eingeblendete Grundebene unter dem Zeiger und die Strahllänge bis zu ihr
    /// (wie [`Self::nearest_plane_hit`]).
    fn nearest_origin_hit(&self, cursor: Vec2, viewport: Vec2) -> Option<(StandardPlane, f64)> {
        let ndc = Vec2::new(
            2.0 * cursor.x / viewport.x - 1.0,
            1.0 - 2.0 * cursor.y / viewport.y,
        );
        let (origin, dir) = self.camera.ray(ndc);
        let ray = Ray {
            origin: origin.as_dvec3(),
            dir: dir.as_dvec3(),
        };
        self.origin_plane_rects()
            .into_iter()
            .filter_map(|(p, rect)| pick_plane(&ray, &rect).map(|t| (p, t)))
            .min_by(|a, b| a.1.total_cmp(&b.1))
    }

    /// Was ein Klick in der normalen Auswahl trifft: wie [`Self::view_pick`], dazu die
    /// eingeblendeten Grundebenen ([`Self::plane_click_at`]; beim Markieren von Kanten nicht).
    pub fn view_click_at(
        &self,
        cursor: Vec2,
        viewport: Vec2,
    ) -> Option<crate::plane_dialog::PlaneClick> {
        let filter = if self.edge_picking {
            PickFilter::GEOMETRY
        } else {
            PickFilter::SELECT
        };
        self.plane_click_at(cursor, viewport, filter)
    }

    /// Was ein Klick in einem Wahlmodus (Ebenen-Fenster, „Skizzenebene neu festlegen“) trifft:
    /// wie [`Self::pick_with`], dazu bei `filter.planes` die eingeblendeten Grundebenen. Eine
    /// Grundebene gewinnt, wenn nichts davor liegt (Ecken und Kanten gehen immer vor).
    pub fn plane_click_at(
        &self,
        cursor: Vec2,
        viewport: Vec2,
        filter: PickFilter,
    ) -> Option<crate::plane_dialog::PlaneClick> {
        use crate::plane_dialog::PlaneClick;
        if viewport.x < 1.0 || viewport.y < 1.0 {
            return None;
        }
        let hit = self.pick_with(cursor, viewport, filter);
        let origin = filter
            .planes
            .then(|| self.nearest_origin_hit(cursor, viewport))
            .flatten();
        let Some((plane, t)) = origin else {
            return hit.map(PlaneClick::View);
        };
        // Eine nur für den Wahlmodus eingeblendete Grundebene (im Baum ausgeblendet) steht hinter
        // jedem anderen Treffer zurück: Sie geht durch das Modell und soll Flächen dahinter nicht
        // verdecken.
        if hit.is_some() && !self.is_origin_plane_shown(plane) {
            return hit.map(PlaneClick::View);
        }
        // Abstand des anderen Treffers; Ecken und Kanten liegen vorn.
        let front = match hit {
            None => None,
            Some(Pick::Face(_)) => self
                .nearest_face_hit(cursor, viewport)
                .map(|(_, f)| f.distance as f64),
            Some(Pick::Plane(_)) => self.nearest_plane_hit(cursor, viewport).map(|(_, d)| d),
            Some(_) => Some(f64::NEG_INFINITY),
        };
        match (hit, front) {
            (Some(h), Some(d)) if d <= t => Some(PlaneClick::View(h)),
            _ => Some(PlaneClick::Standard(plane)),
        }
    }

    /// Strahllänge bis zum Treffer `click` unter dem Zeiger (wie in [`Self::plane_click_at`]):
    /// Ecken und Kanten liegen ganz vorn (`-∞`), ohne Treffer `None`.
    pub(crate) fn click_distance(
        &self,
        cursor: Vec2,
        viewport: Vec2,
        click: Option<crate::plane_dialog::PlaneClick>,
    ) -> Option<f64> {
        use crate::plane_dialog::PlaneClick;
        match click? {
            PlaneClick::Standard(_) => self.nearest_origin_hit(cursor, viewport).map(|(_, t)| t),
            PlaneClick::View(Pick::Face(_)) => self
                .nearest_face_hit(cursor, viewport)
                .map(|(_, f)| f64::from(f.distance)),
            PlaneClick::View(Pick::Plane(_)) => {
                self.nearest_plane_hit(cursor, viewport).map(|(_, d)| d)
            }
            PlaneClick::View(_) => Some(f64::NEG_INFINITY),
        }
    }

    /// Rechteck, als das die Konstruktionsebene `id` gezeichnet und angeklickt wird (Skizzen 6);
    /// `None` für unterdrückte, fehlerhafte und zurückgerollte Ebenen und andere Features.
    /// Halbe Kante: das Größere aus 25 mm und 0,6 × Diagonale der Hüllquader aller sichtbaren
    /// Körper. Mitte: Mitte der Basisfläche (Versatz von einer Fläche, Tangentialebene) in die
    /// Ebene gelegt, sonst der Fußpunkt des Ursprungs.
    pub fn plane_rect(&self, id: FeatureId) -> Option<PlaneRect> {
        let Some(FeatureKind::Plane(pf)) = self.document.feature(id).map(|f| &f.kind) else {
            return None;
        };
        if !self.document.is_active(id) {
            return None;
        }
        let frame = self.document.plane_frame(id)?;
        self.plane_rect_for(&pf.def, frame)
    }

    /// Rechteck einer Ebene mit der Definition `def` und der Lage `frame` (wie
    /// [`AppState::plane_rect`]; auch für die Vorschau des Fensters, die noch kein Feature ist).
    pub fn plane_rect_for(&self, def: &PlaneDef, frame: Frame) -> Option<PlaneRect> {
        use planora_feature::plane_rect;
        // Dieselbe Regel wie „3D-Geometrie einbeziehen“ (Skizzen 10, `plane_rect`).
        let anchor = plane_rect::plane_base_face(def)
            .and_then(|r| self.face_outline(r))
            .and_then(|pts| plane_rect::outline_anchor(&pts));
        let center = plane_rect::plane_rect_center(&frame, anchor);
        let half = plane_rect::plane_half_for_bounds(
            self.bodies
                .iter()
                .enumerate()
                .filter(|(j, _)| !self.is_body_hidden(*j))
                .map(|(_, b)| b.bounds),
        );
        (center.is_finite() && half.is_finite()).then_some(PlaneRect {
            frame: Frame {
                origin: center,
                ..frame
            },
            half: DVec2::splat(half),
        })
    }

    /// Wie [`AppState::pick`], aber nur Flächen (Kanten zählen nicht): für Schneiden und
    /// Silhouette, die Flächen wählen.
    pub fn pick_face_under(&self, cursor: Vec2, viewport: Vec2) -> Option<FaceRef> {
        if viewport.x < 1.0 || viewport.y < 1.0 {
            return None;
        }
        self.nearest_face_hit(cursor, viewport)
            .map(|(body, hit)| FaceRef {
                body,
                face: hit.face,
            })
    }

    /// Wie [`AppState::pick_face_under`], dazu die getroffene Stelle in Weltkoordinaten (für den
    /// Punkt der Isokurve, Skizzen 11b).
    pub fn face_point_under(
        &self,
        cursor: Vec2,
        viewport: Vec2,
    ) -> Option<(usize, Id<Face>, DVec3)> {
        if viewport.x < 1.0 || viewport.y < 1.0 {
            return None;
        }
        let ndc = Vec2::new(
            2.0 * cursor.x / viewport.x - 1.0,
            1.0 - 2.0 * cursor.y / viewport.y,
        );
        let (origin, dir) = self.camera.ray(ndc);
        let (body, hit) = self.nearest_face_hit(cursor, viewport)?;
        let at = origin.as_dvec3() + dir.as_dvec3() * f64::from(hit.distance);
        Some((body, hit.face, at))
    }

    /// Die vorderste sichtbare Fläche unter dem Zeiger (Index des Körpers und Treffer).
    fn nearest_face_hit(
        &self,
        cursor: Vec2,
        viewport: Vec2,
    ) -> Option<(usize, planora_render::FaceHit)> {
        let ndc = Vec2::new(
            2.0 * cursor.x / viewport.x - 1.0,
            1.0 - 2.0 * cursor.y / viewport.y,
        );
        let (origin, dir) = self.camera.ray(ndc);
        self.nearest_face_along(origin, dir, self.active_slice())
    }

    /// Strahllänge bis zum ersten Körpertreffer des Strahls `origin` + t · `dir` (derselbe Treffer
    /// wie beim Anklicken von Flächen, auch bei Slice: Was vor der Skizzenebene liegt, zählt
    /// nicht; ausgeblendete Körper fehlen). `None`, wenn der Strahl keinen Körper trifft. Damit
    /// prüfen fertige Skizzen, ob eine Stelle verdeckt ist: Der Treffer liegt dann vor ihr.
    pub fn body_hit_distance(
        &self,
        origin: DVec3,
        dir: DVec3,
        slice: Option<[f32; 4]>,
    ) -> Option<f64> {
        self.nearest_face_along(to_vec3(origin), to_vec3(dir), slice)
            .map(|(_, h)| f64::from(h.distance))
    }

    /// Die vorderste Fläche, die der Strahl trifft, mit dem Index ihres Körpers.
    fn nearest_face_along(
        &self,
        origin: Vec3,
        dir: Vec3,
        slice: Option<[f32; 4]>,
    ) -> Option<(usize, planora_render::FaceHit)> {
        // Slice: Was vor der Skizzenebene liegt, ist weggeschnitten; der Strahl beginnt dann erst
        // an der Ebene (knapp davor, so dass Flächen in der Ebene zählen).
        let skip = match slice {
            Some(plane) => slice_ray_start(plane, origin.as_dvec3(), dir.as_dvec3())?,
            None => 0.0,
        };
        let start = origin + dir * skip;
        let mut nearest_face: Option<(usize, planora_render::FaceHit)> = None;
        for (body, b) in self.bodies.iter().enumerate() {
            if self.is_body_hidden(body) {
                continue;
            }
            if let Some(mut hit) = pick_face(&b.mesh, start, dir) {
                hit.distance += skip;
                let at = (origin + dir * hit.distance).as_dvec3();
                if slice.is_some_and(|plane| slice_cuts(plane, at)) {
                    continue;
                }
                if nearest_face.is_none_or(|(_, h)| hit.distance < h.distance) {
                    nearest_face = Some((body, hit));
                }
            }
        }
        nearest_face
    }
}

/// Was „Projizieren“ in die Skizze legt: eine Kante, eine Fläche (ihre Schleifen) oder einen
/// ganzen Körper; je mit dem Index des Körpers in [`AppState::bodies`]. Dazu (Skizzen 10,
/// Spec 3) eine Kurve einer fertigen Skizze, die im Verlauf vor der offenen steht: Projizieren
/// wirft sie in die Ebene, Schneiden liefert ihre Schnittpunkte mit der Ebene.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectTarget {
    Edge(usize, Id<Edge>),
    Face(usize, Id<Face>),
    Body(usize),
    SketchCurve(FeatureId, CurveId),
}

/// Was „3D-Geometrie einbeziehen“ (Skizzen 10, Spec 5) in die Skizze holt; Körper je mit ihrem
/// Index in [`AppState::bodies`].
#[derive(Clone, Debug, PartialEq)]
pub enum IncludePick {
    /// Eine Körperkante.
    Edge(usize, Id<Edge>),
    /// Alle Randkanten einer Fläche.
    Face(usize, Id<Face>),
    /// Eine Körperecke: Anfang (`end == false`) oder Ende der Kante `edge`.
    Vertex {
        body: usize,
        edge: Id<Edge>,
        end: bool,
    },
    /// Grund- oder Konstruktionsebene (vier Randlinien ihres Rechtecks).
    Plane(planora_feature::PlaneId),
    /// Eine Achse des Ursprungs.
    Axis(planora_feature::AxisId),
    /// Ein Konstruktionspunkt (gibt es noch nicht: Meldung).
    ConstructionPoint(FeatureId),
    /// Der Ursprung.
    Origin,
    /// Eine Kurve einer Skizze, die im Verlauf vor der offenen steht.
    SketchCurve { sketch: FeatureId, curve: CurveId },
}

/// Meldung zu einem Fehler des Ebenenschnitts (`section`) oder der Silhouette, mit dem Text des
/// Befehls statt dem des Rechners.
fn section_error(e: &planora_kernel::SectionError, section: bool) -> Msg {
    use planora_kernel::SectionError;
    match (e, section) {
        (SectionError::Unsupported { surface, .. }, true) => crate::msg!(
            "sketch-intersect-unsupported",
            surface = planora_kernel::boolean::surface_label(surface)
        ),
        (SectionError::Unsupported { surface, .. }, false) => crate::msg!(
            "sketch-silhouette-unsupported",
            surface = planora_kernel::boolean::surface_label(surface)
        ),
        (SectionError::Freeform(_), true) => crate::msg!("sketch-intersect-freeform"),
        (SectionError::Freeform(_), false) => crate::msg!("sketch-silhouette-freeform"),
    }
}

/// Kanten, die in einer Fläche zweimal vorkommen (Nähte von Zylinder, Kegel und Kugel): Sie
/// sind keine echten Ränder und werden nicht projiziert.
fn seam_edges(solid: &Solid) -> std::collections::HashSet<Id<Edge>> {
    let mut seams = std::collections::HashSet::new();
    for (_, face) in solid.faces.iter() {
        let mut seen = std::collections::HashSet::new();
        for he in std::iter::once(&face.outer)
            .chain(&face.inner)
            .flat_map(|l| &l.half_edges)
        {
            if !seen.insert(he.edge) {
                seams.insert(he.edge);
            }
        }
    }
    seams
}

/// Die Kanten der Schleifen von `face` (außen, dann die Löcher), ohne Nähte und Wiederholung;
/// `None`, wenn es die Fläche nicht gibt.
fn face_edges(solid: &Solid, face: Id<Face>) -> Option<Vec<Id<Edge>>> {
    let f = solid.faces.get(face)?;
    let seams = seam_edges(solid);
    let mut out: Vec<Id<Edge>> = Vec::new();
    for he in std::iter::once(&f.outer)
        .chain(&f.inner)
        .flat_map(|l| &l.half_edges)
    {
        if !seams.contains(&he.edge) && !out.contains(&he.edge) {
            out.push(he.edge);
        }
    }
    Some(out)
}

/// Alle Kanten des Körpers ohne Nähte.
fn body_edges(solid: &Solid) -> Vec<Id<Edge>> {
    let seams = seam_edges(solid);
    solid
        .edges
        .iter()
        .map(|(id, _)| id)
        .filter(|id| !seams.contains(id))
        .collect()
}

/// Index des Körpers, zu dem eine Fläche, Kante oder Ecke gehört (eine Ebene: `None`).
fn pick_body(p: Pick) -> Option<usize> {
    match p {
        Pick::Face(FaceRef { body, .. })
        | Pick::Edge(EdgeRef { body, .. })
        | Pick::Vertex { body, .. } => Some(body),
        Pick::Plane(_) | Pick::SketchCurve { .. } | Pick::SketchPoint { .. } => None,
    }
}

fn to_vec3(v: DVec3) -> Vec3 {
    Vec3::new(v.x as f32, v.y as f32, v.z as f32)
}

// ---- Skizzen ----------------------------------------------------------------------------------

impl AppState {
    /// Hinweis für die Statuszeile, wenn das Netz eines sichtbaren Körpers ungenau ist oder nur
    /// als Vorschau gezeigt wird: mit dem Namen des Körpers (des gewählten, wenn er betroffen
    /// ist, sonst des ersten). Ändert nichts.
    pub fn mesh_note(&self) -> Option<String> {
        let noted = |b: &&Body| b.mesh_inexact || b.preview;
        let chosen = self
            .selected_body
            .and_then(|i| self.bodies.get(i))
            .filter(noted);
        let body = chosen.or_else(|| self.bodies.iter().find(noted))?;
        let note = if body.preview {
            crate::mesh_body::preview_note()
        } else {
            mesh_note()
        };
        Some(tr!("state-body-note", name = &body.name, note = note))
    }

    /// Fehler als Meldung ablegen und zurückgeben.
    pub(crate) fn fail<T>(&mut self, e: String) -> Result<T, String> {
        self.message = Some(e.clone());
        Err(e)
    }

    /// Skizzen-Features der Zeitleiste in ihrer Reihenfolge, mit Namen.
    pub fn sketch_features(&self) -> Vec<(FeatureId, String)> {
        self.document
            .features()
            .iter()
            .filter(|f| matches!(f.kind, FeatureKind::Sketch(_)))
            .map(|f| (f.id, f.name.clone()))
            .collect()
    }

    /// Das geöffnete Skizzen-Feature.
    pub fn open_sketch_id(&self) -> Option<FeatureId> {
        self.sketch_session.as_ref().map(|s| s.sketch)
    }

    /// Schnittebene von Slice (Skizzen 12, [`planora_render::slice_plane`]): nur bei geöffneter
    /// Skizze mit Haken `SketchDisplay::slice`. Die Ebene ist die Skizzenebene (`frame`, auch bei
    /// 3D-Skizzen); die Seite folgt der Kamera dieses Augenblicks.
    pub fn active_slice(&self) -> Option<[f32; 4]> {
        let id = self.open_sketch_id()?;
        if !self.document.sketch_display(id).slice {
            return None;
        }
        let frame = *self.sketch_draft.as_ref()?.sketch.frame();
        Some(planora_render::slice_plane(
            frame.origin,
            frame.z,
            self.camera.eye().as_dvec3(),
        ))
    }

    /// Legt ein leeres Skizzen-Feature „Skizze N“ auf der Ebene `plane` am Marker an. Bei
    /// `OnFace` bestimmt die Berechnung die Lage aus der Fläche. Lässt sich die Skizze nicht
    /// berechnen (etwa weil es die Fläche nicht gibt), bleibt das Dokument unverändert.
    pub fn add_sketch(&mut self, plane: SketchPlane) -> Result<FeatureId, String> {
        let frame = match &plane {
            SketchPlane::Standard(p) => p.frame(),
            // Ohne bekannte Fläche vorläufig; die Berechnung setzt die Ebene der Fläche.
            // [`AppState::new_sketch_on_face`] gibt die echte Ebene mit.
            SketchPlane::OnFace(_) => Frame::WORLD,
            // Lage der Konstruktionsebene; nur ohne berechnete Ebene vorläufig die Weltebene.
            SketchPlane::OnPlane(id) => self.document.plane_frame(*id).unwrap_or(Frame::WORLD),
        };
        self.add_sketch_in(plane, frame)
    }

    /// Legt ein leeres Skizzen-Feature auf `plane` an; `frame` ist die Lage der Ebene, die in
    /// der Definition gespeichert wird.
    fn add_sketch_in(&mut self, plane: SketchPlane, frame: Frame) -> Result<FeatureId, String> {
        let label = sketch_label();
        let sketch = Sketch::new(frame, self.next_name(&label), self.kernel.tolerance);
        let kind = FeatureKind::Sketch(SketchFeature {
            sketch,
            plane,
            copies: Vec::new(),
            formulas: Default::default(),
            text_formulas: Default::default(),
        });
        self.add_checked(&label, kind)
    }

    /// Legt ein Skizzen-Feature auf `plane` (Lage `frame`) an, dessen Skizze `fill` vorher füllt
    /// (Einfügen, Skizzen 9). Ohne `name` heißt sie „Skizze N“. Scheitert `fill` oder lässt
    /// sich die Skizze nicht berechnen, bleibt das Dokument unverändert.
    pub(crate) fn add_filled_sketch<R>(
        &mut self,
        plane: SketchPlane,
        frame: Frame,
        name: Option<&str>,
        fill: impl FnOnce(&mut Sketch) -> Result<R, String>,
    ) -> Result<(FeatureId, R), String> {
        let label = sketch_label();
        let given = name.map(str::trim).filter(|n| !n.is_empty());
        let name = given.map_or_else(|| self.next_name(&label), str::to_string);
        let mut sketch = Sketch::new(frame, name.clone(), self.kernel.tolerance);
        let out = fill(&mut sketch)?;
        let kind = FeatureKind::Sketch(SketchFeature {
            sketch,
            plane,
            copies: Vec::new(),
            formulas: Default::default(),
            text_formulas: Default::default(),
        });
        let id = self.add_checked_named(name, kind, Formulas::new())?;
        // Nur ein Name aus dem Zähler zählt ihn weiter (ein Layername nicht).
        if given.is_none() {
            self.count_name(&label);
        }
        Ok((id, out))
    }

    /// Nimmt Features zurück, die ein Befehl im selben Bild angelegt hat (alles oder nichts).
    pub(crate) fn remove_features(&mut self, ids: &[FeatureId]) {
        for &id in ids.iter().rev() {
            let _ = self.document.delete_with_dependents(id);
        }
    }

    /// Legt ein Skizzen-Feature auf der ebenen Fläche `face` des sichtbaren Körpers `body` an.
    /// Die Skizze verweist über Marke und Hinweispunkt auf die Fläche und wandert mit ihr.
    pub fn new_sketch_on_face(&mut self, body: usize, face: Id<Face>) -> Result<FeatureId, String> {
        if self.is_mesh(body) {
            return self.fail(mesh_locked());
        }
        let Some(b) = self.bodies.get(body) else {
            return self.fail(tr!("state-body-not-found"));
        };
        if b.solid.faces.get(face).is_none() {
            return self.fail(tr!("state-face-not-found"));
        }
        let Some(frame) = plane_of_face(&b.solid, face) else {
            return self.fail(tr!("state-sketch-planar-only"));
        };
        let r = match planora_feature::FaceRef::from_face(&b.solid, face, &self.kernel.tolerance) {
            Ok(r) => r,
            Err(e) => {
                return self.fail(tr!("state-sketch-on-face-failed", error = e.to_string()));
            }
        };
        self.add_sketch_in(SketchPlane::OnFace(r), frame)
    }

    /// Legt eine Konstruktionsebene „Ebene N“ mit der Definition `def` am Marker an (Skizzen 6,
    /// Menü „Konstruieren“). Ein Schritt; lässt sich die Ebene nicht berechnen, bleibt sie mit
    /// Fehlerstatus stehen (wie andere Features) und die Meldung steht in `message`.
    pub fn add_plane(&mut self, def: PlaneDef) -> Result<FeatureId, String> {
        self.add_plane_with(def, Formulas::new())
    }

    /// Wie [`AppState::add_plane`], mit den Formeln der Felder „Abstand“ bzw. „Winkel“.
    pub fn add_plane_with(
        &mut self,
        def: PlaneDef,
        formulas: Formulas,
    ) -> Result<FeatureId, String> {
        self.try_close_sketch()?;
        let kind = FeatureKind::Plane(planora_feature::planes::PlaneFeature { def });
        self.add_feature_with(None, kind, formulas)
    }

    /// Ändert die Konstruktionsebene `id` auf `def` mit `formulas` (Fenster „Bearbeiten“): ein
    /// Schritt. Lässt sich die Ebene danach nicht berechnen, steht der Grund in `message` und im
    /// Baum; das gilt trotzdem als übernommen.
    pub fn update_plane(
        &mut self,
        id: FeatureId,
        def: PlaneDef,
        formulas: Formulas,
    ) -> Result<(), String> {
        if !self
            .document
            .feature(id)
            .is_some_and(|f| matches!(f.kind, FeatureKind::Plane(_)))
        {
            return self.fail(tr!("state-plane-not-found"));
        }
        self.end_sketch_for(None)?;
        let kind = FeatureKind::Plane(planora_feature::planes::PlaneFeature { def });
        if let Err(e) = self
            .document
            .update_with_formulas(id, |k| *k = kind, formulas)
        {
            return self.fail(tr!("edit-rejected", error = e.to_string()));
        }
        self.message = None;
        self.refresh();
        if let Some(f) = self.document.feature(id)
            && let FeatureStatus::Error(e) = &f.status
        {
            self.message = Some(tr!("tree-status-line", name = &f.name, error = e));
        }
        Ok(())
    }

    /// „Skizzenebene neu festlegen“ (Skizzen 6): setzt die Skizze `sketch` auf `plane`. Ein
    /// Schritt; eine offene Skizze wird vorher beendet. Lehnt das Dokument ab (etwa eine Fläche
    /// eines späteren Features), bleibt es unverändert und der Grund steht in `message`.
    pub fn redefine_sketch_plane(
        &mut self,
        sketch: FeatureId,
        plane: SketchPlane,
    ) -> Result<(), String> {
        if !self
            .document
            .feature(sketch)
            .is_some_and(|f| matches!(f.kind, FeatureKind::Sketch(_)))
        {
            return self.fail(tr!("state-sketch-not-found"));
        }
        self.try_close_sketch()?;
        if let Err(e) = self.document.set_sketch_plane(sketch, plane) {
            return self.fail(tr!("edit-rejected", error = e.to_string()));
        }
        self.message = None;
        self.refresh();
        if let Some(f) = self.document.feature(sketch)
            && let FeatureStatus::Error(e) = &f.status
        {
            self.message = Some(tr!("tree-status-line", name = &f.name, error = e));
        }
        Ok(())
    }

    /// Beginnt den Wahlmodus „Skizzenebene neu festlegen“ für `sketch` (Kontextmenü, Taste).
    /// Eine offene Skizze wird vorher beendet (wie „Extrudieren“ aus dem Kontextmenü); wird ihr
    /// Entwurf abgelehnt, beginnt der Modus nicht. Andere Wahlmodi enden.
    pub fn start_redefine_sketch_plane(&mut self, sketch: FeatureId) -> Result<(), String> {
        if !self
            .document
            .feature(sketch)
            .is_some_and(|f| matches!(f.kind, FeatureKind::Sketch(_)))
        {
            return self.fail(tr!("state-sketch-not-found"));
        }
        self.try_close_sketch()?;
        self.plane_pick = None;
        self.edge_picking = false;
        self.clear_edge_marks();
        self.selected_feature = Some(sketch);
        self.selected_body = None;
        self.set_selection(None);
        self.message = None;
        self.pending_pick = Some(PendingPick::SketchPlane(sketch));
        Ok(())
    }

    /// Beendet einen Wahlmodus ohne Änderung (Esc, Rechtsklick), auch „Kalibrieren“: Wer ein
    /// Fenster öffnet, das in der Ansicht wählt, beendet damit jeden Klickmodus.
    pub fn cancel_pending_pick(&mut self) {
        self.pending_pick = None;
        self.calibrate = None;
        self.set_hover(None);
    }

    /// Ein Klick im Wahlmodus „Skizzenebene neu festlegen“ (Ansicht oder Ordner „Ursprung“).
    /// Eine Grundebene, Konstruktionsebene oder ebene Fläche setzt die Skizze darauf und beendet
    /// den Modus, auch wenn das Dokument ablehnt. Eine gekrümmte Fläche (oder eine Fläche eines
    /// Netz-Körpers) gibt nur eine Meldung; der Modus bleibt.
    pub fn pick_sketch_plane(&mut self, click: crate::plane_dialog::PlaneClick) {
        use crate::plane_dialog::PlaneClick;
        let sketch = match self.pending_pick {
            Some(PendingPick::SketchPlane(sketch)) => sketch,
            Some(PendingPick::NewSketch) => return self.pick_new_sketch_plane(click),
            None => return,
        };
        let plane = match click {
            PlaneClick::Standard(p) => SketchPlane::Standard(p),
            PlaneClick::View(Pick::Plane(id)) => SketchPlane::OnPlane(id),
            PlaneClick::View(Pick::Face(f)) => match self.face_ref_for_sketch(f.body, f.face) {
                Ok(r) => SketchPlane::OnFace(r),
                Err(e) => {
                    self.message = Some(e);
                    return;
                }
            },
            // Kanten und Eckpunkte wählt der Filter nicht.
            PlaneClick::View(_) => return,
        };
        self.cancel_pending_pick();
        let _ = self.redefine_sketch_plane(sketch, plane);
    }

    /// „Skizze erstellen“ (wie Fusion „Create Sketch“, ein Rückgängig-Schritt). Ist eine
    /// Grundebene, eine Konstruktionsebene oder eine ebene Fläche gewählt, entsteht die Skizze
    /// sofort darauf und wird geöffnet (`Ok(Some(id))`). Sonst wird die Auswahl geleert und der
    /// Wahlmodus [`PendingPick::NewSketch`] beginnt (`Ok(None)`). Eine offene Skizze wird zuerst
    /// geschlossen; scheitert das oder ist die Vorauswahl gesperrt, kommt der Grund zurück (auch
    /// in `message`) und es gibt keinen Wahlmodus.
    pub fn start_create_sketch(&mut self) -> Result<Option<FeatureId>, String> {
        self.try_close_sketch()?;
        self.plane_pick = None;
        self.edge_picking = false;
        self.clear_edge_marks();
        self.cancel_pending_pick();
        self.message = None;
        let origin = self.selected_origin_plane();
        let plane = self.selected_plane();
        let face = match self.selection {
            Some(Pick::Face(FaceRef { body, face })) => self
                .bodies
                .get(body)
                .is_some_and(|b| plane_of_face(&b.solid, face).is_some())
                .then_some((body, face)),
            _ => None,
        };
        if origin.is_some() || plane.is_some() || face.is_some() {
            if let Some(why) = self.sketch_on_face_blocked() {
                return self.fail(why);
            }
            let made = match (origin, plane, face) {
                (Some(p), _, _) => self.add_sketch(SketchPlane::Standard(p)),
                (None, Some(id), _) => self.new_sketch_on_plane(id),
                (None, None, Some((body, face))) => self.new_sketch_on_face(body, face),
                (None, None, None) => unreachable!("oben geprüft"),
            }?;
            self.open_sketch(made)?;
            return Ok(Some(made));
        }
        self.selected_feature = None;
        self.selected_body = None;
        self.selected_origin = None;
        self.set_selection(None);
        self.pending_pick = Some(PendingPick::NewSketch);
        Ok(None)
    }

    /// Ein Klick im Wahlmodus „Skizze erstellen“: Grundebene, Konstruktionsebene oder ebene
    /// Fläche legt die Skizze an, öffnet sie und beendet den Modus (auch wenn das Dokument
    /// ablehnt, dann mit Meldung). Eine gekrümmte Fläche (oder eine Fläche eines Netz-Körpers)
    /// gibt nur eine Meldung; der Modus bleibt.
    fn pick_new_sketch_plane(&mut self, click: crate::plane_dialog::PlaneClick) {
        use crate::plane_dialog::PlaneClick;
        if let PlaneClick::View(Pick::Face(f)) = click
            && let Err(e) = self.face_ref_for_sketch(f.body, f.face)
        {
            self.message = Some(e);
            return;
        }
        let made = match click {
            PlaneClick::Standard(p) => {
                self.cancel_pending_pick();
                self.add_sketch(SketchPlane::Standard(p))
            }
            PlaneClick::View(Pick::Plane(id)) => {
                self.cancel_pending_pick();
                self.new_sketch_on_plane(id)
            }
            PlaneClick::View(Pick::Face(f)) => {
                self.cancel_pending_pick();
                self.new_sketch_on_face(f.body, f.face)
            }
            // Kanten und Eckpunkte wählt der Filter nicht.
            PlaneClick::View(_) => return,
        };
        if let Ok(id) = made {
            let _ = self.open_sketch(id);
        }
    }

    /// Verweis auf die ebene Fläche `face` des sichtbaren Körpers `body` für eine Skizze; sonst
    /// der Grund.
    pub(crate) fn face_ref_for_sketch(
        &self,
        body: usize,
        face: Id<Face>,
    ) -> Result<planora_feature::FaceRef, String> {
        if self.is_mesh(body) {
            return Err(mesh_locked());
        }
        let b = self
            .bodies
            .get(body)
            .ok_or_else(|| tr!("state-body-not-found"))?;
        if b.solid.faces.get(face).is_none() {
            return Err(tr!("state-face-not-found"));
        }
        if plane_of_face(&b.solid, face).is_none() {
            return Err(tr!("state-sketch-planar-only"));
        }
        planora_feature::FaceRef::from_face(&b.solid, face, &self.kernel.tolerance)
            .map_err(|e| tr!("state-sketch-on-face-failed", error = e.to_string()))
    }

    /// Legt ein leeres Skizzen-Feature auf der Konstruktionsebene `id` an (Kontextmenü
    /// „Skizze erstellen“, „Skizze auf gewählter Fläche“ mit gewählter Ebene). Die Skizze folgt
    /// der Ebene. Nur für berechnete Ebenen.
    pub fn new_sketch_on_plane(&mut self, id: FeatureId) -> Result<FeatureId, String> {
        if !self
            .document
            .feature(id)
            .is_some_and(|f| matches!(f.kind, FeatureKind::Plane(_)))
        {
            return self.fail(tr!("state-plane-not-found"));
        }
        let Some(frame) = self.document.plane_frame(id) else {
            return self.fail(tr!("state-plane-not-computed"));
        };
        if let Some(why) = crate::history_bar::lock_reason(self, &[id]) {
            return self.fail(why);
        }
        self.try_close_sketch()?;
        self.add_sketch_in(SketchPlane::OnPlane(id), frame)
    }

    /// Die gewählte, berechnete Konstruktionsebene (in der Ansicht angeklickt oder im Browser
    /// gewählt), sonst `None`.
    pub fn selected_plane(&self) -> Option<FeatureId> {
        let id = match self.selection {
            Some(Pick::Plane(id)) => id,
            _ => self.selected_feature?,
        };
        self.plane_rect(id).map(|_| id)
    }

    /// Öffnet den Skizzen-Modus für das Skizzen-Feature `id` auf einem Entwurf (Kopie der
    /// Definition mit der berechneten Skizze): Kamera senkrecht auf die Skizzenebene, übrige
    /// Szene gedämpft. Eine schon geöffnete Skizze wird vorher zurückgeschrieben; die gemerkte
    /// Kamera bleibt dabei erhalten.
    pub fn open_sketch(&mut self, id: FeatureId) -> Result<(), String> {
        if !self
            .document
            .feature(id)
            .is_some_and(|f| matches!(f.kind, FeatureKind::Sketch(_)))
        {
            return self.fail(tr!("state-sketch-not-found"));
        }
        // Im Skizzen-Modus wählt die Ansicht nichts für einen anderen Befehl.
        self.pending_pick = None;
        self.calibrate = None;
        let switching = self.sketch_session.is_some();
        if switching {
            // Bleibt die alte Skizze offen (Entwurf abgelehnt), wird nicht gewechselt.
            self.commit_draft()?;
        }
        let Some(f) = self.document.feature(id) else {
            return self.fail(tr!("state-sketch-not-found"));
        };
        let FeatureKind::Sketch(def) = &f.kind else {
            return self.fail(tr!("state-sketch-not-found"));
        };
        let mut draft = def.clone();
        // Die berechnete Skizze trägt die aufgelöste Ebene und die nachgeführten Kantenkopien;
        // ohne Ergebnis (etwa hinter dem Marker) dient die Definition.
        if let Some(sk) = self.document.sketch_of(id) {
            draft.sketch = sk.clone();
        }
        draft.sketch.name = f.name.clone();
        let frame = *draft.sketch.frame();
        let view_points = self.sketch_view_points(&draft.plane, &frame, draft.sketch.extent());
        let mut session = SketchSession::new(id);
        session.body_vertices = self.body_vertices(id, &frame);
        session.last_report = Some(draft.sketch.clone().solve());
        session.text_memory = self.text_memory.clone();
        // Parameter im Text (Skizzen 11b): Der Entwurf zeigt den ersetzten Inhalt.
        session.text_scope = self.document.param_scope();
        planora_feature::text_refs::apply_shown(&mut draft.sketch, &session.text_scope);
        if !switching {
            self.saved_camera = Some(self.camera);
        }
        self.sketch_session = Some(session);
        // Jede geöffnete Skizze beginnt mit einem eigenen, leeren Verlauf (R10).
        self.sketch_history = Some(History::new(draft.clone(), sketch_bytes(&draft)));
        self.sketch_draft = Some(draft);
        self.sync_sketch_display();
        self.selected_feature = Some(id);
        // Senkrecht auf die Ebene blicken, dann mittig in das freie Rechteck einpassen. Die
        // Kamera kennt hier noch das freie Rechteck des vorigen Bildes (Modellmodus); die
        // 3D-Ansicht passt mit dem des Skizzenmodus nach (`refit_sketch_view`).
        self.camera.look_along(to_vec3(frame.z), to_vec3(frame.y));
        self.camera.frame_points(&view_points);
        self.sketch_fit = Some(SketchFit {
            points: view_points,
            fitted: self.camera,
            stable: 0,
        });
        self.rebuild_scene();
        Ok(())
    }

    /// Fusion „Blick auf“ (Look At): Die Kamera schaut senkrecht auf die Ebene der offenen
    /// Skizze, oben ist die y-Achse der Skizze. Nur Richtung und Ziel ändern sich (das Ziel
    /// wandert in die Ebene), der Abstand bleibt; kein Rückgängig-Schritt. Das Einpassen beim
    /// Öffnen endet damit, es springt nichts mehr nach. Ohne offene Skizze geschieht nichts.
    pub fn look_at_sketch(&mut self) {
        let Some(frame) = self.sketch_draft.as_ref().map(|d| *d.sketch.frame()) else {
            return;
        };
        if self.sketch_session.is_none() {
            return;
        }
        let n = to_vec3(frame.z);
        let target = self.camera.target;
        let off = (target - to_vec3(frame.origin)).dot(n);
        self.camera.target = target - n * off;
        self.camera.look_along(n, to_vec3(frame.y));
        self.sketch_fit = None;
    }

    /// Punkte, auf die die Ansicht beim Öffnen einer Skizze eingepasst wird (alle in der
    /// Skizzenebene `frame`):
    /// - Skizze mit Geometrie: ihre Hülle (`extent`), mindestens `MIN_SKETCH_VIEW_MM` groß;
    /// - leere Skizze auf einer Fläche: der Umriss der Fläche, in die Ebene gelegt;
    /// - sonst (Grundebene, oder die Fläche ist nicht mehr zu finden): die Hüllen aller Körper,
    ///   in die Ebene gelegt; ohne Körper ±25 mm um den Ursprung der Ebene.
    fn sketch_view_points(
        &self,
        plane: &SketchPlane,
        frame: &Frame,
        extent: Option<(DVec2, DVec2)>,
    ) -> Vec<Vec3> {
        // Rechteck `lo..hi` der Ebene, sehr kleine (ein Punkt, eine winzige Kurve) mit
        // Mindestgröße um ihre Mitte.
        let rect = |lo: DVec2, hi: DVec2| -> Vec<Vec3> {
            let (lo, hi) = if (hi - lo).max_element() < MIN_SKETCH_VIEW_MM {
                let c = (lo + hi) * 0.5;
                let half = DVec2::splat(MIN_SKETCH_VIEW_MM * 0.5);
                (c - half, c + half)
            } else {
                (lo, hi)
            };
            [lo, DVec2::new(hi.x, lo.y), hi, DVec2::new(lo.x, hi.y)]
                .iter()
                .map(|&p| to_vec3(frame.point_2d(p)))
                .collect()
        };
        let in_plane = |p: DVec3| {
            let l = frame.to_local(p);
            DVec2::new(l.x, l.y)
        };
        let hull = |pts: &[DVec2]| {
            pts.iter()
                .fold((pts[0], pts[0]), |(a, b), &p| (a.min(p), b.max(p)))
        };
        if let Some((lo, hi)) = extent {
            return rect(lo, hi);
        }
        if let SketchPlane::OnFace(r) = plane
            && let Some(outline) = self.face_outline(r)
        {
            let pts: Vec<DVec2> = outline.into_iter().map(in_plane).collect();
            if !pts.is_empty() && pts.iter().all(|p| p.is_finite()) {
                let (lo, hi) = hull(&pts);
                if (hi - lo).max_element() < MIN_SKETCH_VIEW_MM {
                    return rect(lo, hi);
                }
                return pts
                    .into_iter()
                    .map(|p| to_vec3(frame.point_2d(p)))
                    .collect();
            }
        }
        let corners: Vec<DVec2> = self
            .bodies
            .iter()
            .flat_map(|b| {
                let (lo, hi) = b.bounds;
                (0..8).map(move |i| {
                    DVec3::new(
                        if i & 1 == 0 { lo.x } else { hi.x },
                        if i & 2 == 0 { lo.y } else { hi.y },
                        if i & 4 == 0 { lo.z } else { hi.z },
                    )
                })
            })
            .map(in_plane)
            .filter(|p| p.is_finite())
            .collect();
        if corners.is_empty() {
            let half = DVec2::splat(EMPTY_SKETCH_HALF_MM);
            return rect(-half, half);
        }
        let (lo, hi) = hull(&corners);
        rect(lo, hi)
    }

    /// Eckpunkte des Anzeigenetzes der Fläche, auf die `r` verweist (unter den sichtbaren
    /// Körpern); `None`, wenn sie sich nicht finden lässt.
    fn face_outline(&self, r: &planora_feature::FaceRef) -> Option<Vec<DVec3>> {
        let solids: Vec<&Solid> = self.bodies.iter().map(|b| &b.solid).collect();
        let found = planora_feature::resolve_face(&solids, r, &self.kernel.tolerance).ok()?;
        let mesh = &self.bodies.get(found.body)?.mesh;
        planora_feature::plane_rect::mesh_face_points(mesh, found.face)
    }

    /// Holt das Einpassen beim Öffnen einer Skizze mit dem freien Rechteck nach, das die
    /// 3D-Ansicht gerade gesetzt hat (jedes Bild im Skizzenmodus nach `set_visible`). Beim
    /// Öffnen aus einem Menü kennt die Kamera nur das freie Rechteck des Modellmodus; das des
    /// Skizzenmodus steht erst im nächsten Bild fest. Nachgepasst wird, solange sich das freie
    /// Rechteck ändert; es endet, sobald es `SKETCH_FIT_STABLE_FRAMES` Bilder gleich bleibt oder
    /// der Benutzer die Ansicht bewegt (Verschieben, Zoomen, Würfel).
    pub fn refit_sketch_view(&mut self) {
        if self.sketch_session.is_none() {
            self.sketch_fit = None;
            return;
        }
        let Some(fit) = self.sketch_fit.as_mut() else {
            return;
        };
        let (cam, was) = (self.camera, fit.fitted);
        if (cam.target, cam.distance, cam.yaw, cam.pitch)
            != (was.target, was.distance, was.yaw, was.pitch)
        {
            // Vom Benutzer bewegt: seine Ansicht bleibt.
            self.sketch_fit = None;
            return;
        }
        let rect = |c: &OrbitCamera| {
            (
                c.aspect,
                c.ndc_offset,
                c.visible_aspect,
                c.visible_height_fraction,
            )
        };
        if rect(&cam) == rect(&was) {
            fit.stable += 1;
            if fit.stable >= SKETCH_FIT_STABLE_FRAMES {
                self.sketch_fit = None;
            }
            return;
        }
        self.camera.frame_points(&fit.points);
        fit.fitted = self.camera;
        fit.stable = 0;
    }

    /// Beendet das Nachpassen der Ansicht beim Öffnen einer Skizze (die Ansicht wurde bewegt).
    pub(crate) fn cancel_sketch_fit(&mut self) {
        self.sketch_fit = None;
    }

    /// Skizze der offenen Sitzung (der Entwurf).
    pub fn session_sketch(&self) -> Option<&Sketch> {
        self.sketch_session.as_ref()?;
        self.sketch_draft.as_ref().map(|d| &d.sketch)
    }

    /// Übernimmt die Anzeigeschalter der geöffneten Skizze (Dokument) und die Kopien ihres
    /// Entwurfs in die Sitzung (Skizzen 12), damit Wählen, Rahmenauswahl und Fang auslassen, was
    /// die Schalter ausblenden. Je Bild vor den Eingaben ([`crate::sketch_view::show_overlay`])
    /// und beim Öffnen; ohne offene Skizze nichts.
    pub fn sync_sketch_display(&mut self) {
        let filter = crate::sketch_view::DisplayFilter::of_open(self);
        if let Some(se) = self.sketch_session.as_mut()
            && se.display != filter
        {
            se.display = filter;
        }
    }

    /// Sitzung und Entwurfsskizze gemeinsam (für die Werkzeuge des Skizzen-Modus).
    pub fn session_sketch_mut(&mut self) -> Option<(&mut SketchSession, &mut Sketch)> {
        let AppState {
            sketch_session,
            sketch_draft,
            ..
        } = self;
        Some((sketch_session.as_mut()?, &mut sketch_draft.as_mut()?.sketch))
    }

    /// Sitzung und ganzer Entwurf (Skizze, Kopien, Formeln der Maße) gemeinsam; der Entwurf
    /// selbst, keine Kopie.
    pub fn session_draft_mut(&mut self) -> Option<(&mut SketchSession, &mut SketchFeature)> {
        let AppState {
            sketch_session,
            sketch_draft,
            ..
        } = self;
        Some((sketch_session.as_mut()?, sketch_draft.as_mut()?))
    }

    /// Schreibt den Entwurf ins Dokument zurück (Skizze und Kopien aus dem Entwurf; die Ebene
    /// bleibt die der aktuellen Definition, im Skizzen-Modus lässt sie sich nicht ändern) und beendet
    /// die Sitzung, ohne die Kamera anzufassen. Ein unveränderter Entwurf löst keine
    /// Neuberechnung aus.
    ///
    /// Lehnt das Dokument den Entwurf ab, bleiben Sitzung und Entwurf offen; die Meldung steht
    /// in `message` und im Skizzenfenster.
    fn commit_draft(&mut self) -> Result<(), String> {
        // Der Skizzenverlauf endet mit der Sitzung; nur wenn das Dokument den Entwurf ablehnt
        // und die Skizze offen bleibt, bleibt er auch.
        let history = self.sketch_history.take();
        let (Some(mut se), Some(mut draft)) =
            (self.sketch_session.take(), self.sketch_draft.take())
        else {
            return Ok(());
        };
        // Ein Befehl ohne fertige Form hinterlässt nichts, auch beim Beenden der Skizze.
        se.drop_open_points(&mut draft.sketch);
        se.clear_pending();
        // Eingezogene Eckpunkte fertiger Elemente behalten ihre Verknüpfung (A7).
        link_vertices(self, &mut se, &mut draft);
        // Projektionen gelöschter Kurven werden nicht gespeichert (Skizzen 6; beim Laden ebenso).
        draft.prune_copies();
        let id = se.sketch;
        let Some(FeatureKind::Sketch(def)) = self.document.feature(id).map(|f| &f.kind) else {
            return Ok(());
        };
        draft.plane = def.plane.clone();
        // Auch eine reine Formeländerung (gleicher Wert, etwa „Breite“ → „breite“) wird
        // übernommen.
        let unchanged = draft.copies == def.copies
            && draft.formulas == def.formulas
            && self
                .document
                .sketch_of(id)
                .is_some_and(|sk| sk.revision() == draft.sketch.revision());
        if unchanged {
            return Ok(());
        }
        let kind = FeatureKind::Sketch(draft.clone());
        match self.document.update(id, |k| *k = kind) {
            Ok(()) => {
                self.refresh();
                if let Some(FeatureStatus::Error(e)) = self.document.feature(id).map(|f| &f.status)
                {
                    self.message = Some(e.clone());
                }
                Ok(())
            }
            Err(e) => {
                let message = tr!("state-sketch-commit-failed", error = e.to_string());
                se.message = Some(crate::msg!(
                    "state-sketch-commit-failed",
                    error = e.to_string()
                ));
                self.sketch_session = Some(se);
                self.sketch_draft = Some(draft);
                self.sketch_history = history;
                self.fail(message)
            }
        }
    }

    /// Beendet den Skizzen-Modus: schreibt den Entwurf zurück, berechnet neu und stellt die
    /// vorherige Kamera wieder her.
    /// Lässt sich der Entwurf nicht übernehmen, bleibt die Skizze offen (Meldung in
    /// `message`); siehe [`AppState::try_close_sketch`].
    pub fn close_sketch(&mut self) {
        let _ = self.try_close_sketch();
    }

    /// Wie [`AppState::close_sketch`], mit Ergebnis: Ein Fehler bedeutet, dass die Skizze
    /// offen geblieben ist.
    pub fn try_close_sketch(&mut self) -> Result<(), String> {
        if self.sketch_session.is_none() {
            self.sketch_draft = None;
            self.sketch_history = None;
            return Ok(());
        }
        // Parameter im Text (Skizzen 11b): Ein Text mit `{…}` aus der Sitzung (etwa ohne
        // Schriftliste eingefügt) wird nach dem Schließen mit den Werten des Dokuments neu
        // gesetzt; bei unveränderter Form ändert das nichts.
        let param_texts = self.sketch_draft.as_ref().is_some_and(|d| {
            d.sketch
                .texts()
                .any(|(_, t)| planora_feature::text_refs::has_param_ref(&t.content))
        });
        self.commit_draft()?;
        if param_texts {
            self.texts_due = true;
        }
        self.restore_camera();
        Ok(())
    }

    /// Die gewählten Kurven der offenen Skizze, die Projektionen sind (Ziel von „Verknüpfung
    /// aufheben“ an Knopf und Aktion); ohne offene Skizze leer.
    pub fn selected_projections(&self) -> Vec<CurveId> {
        match (self.sketch_session.as_ref(), self.sketch_draft.as_ref()) {
            (Some(se), Some(draft)) => se.selected_projections(draft),
            _ => Vec::new(),
        }
    }

    /// „Verknüpfung aufheben“ (Skizzen 6, Fusion „Break Link“), der einzige Weg für Knopf und
    /// Aktion: Die projizierten Kurven `curves` der offenen Skizze folgen ihrer Herkunft nicht
    /// mehr, Lage und Fixierung bleiben. Alle zusammen sind genau ein Skizzen-Rückgängig-Schritt
    /// („Verknüpfung aufheben“); Rückgängig stellt die Verknüpfungen wieder her. Kurven ohne
    /// Projektion werden übergangen. Liefert die Zahl der gelösten Kurven.
    /// Fehler (Meldung auch in `message`, nichts geändert): keine offene Skizze, oder keine der
    /// Kurven ist eine Projektion.
    pub fn break_projection_links(&mut self, curves: &[CurveId]) -> Result<usize, String> {
        if self.sketch_session.is_none() || self.sketch_draft.is_none() {
            return self.fail(tr!("state-no-open-sketch"));
        }
        // Offene Änderungen vorher als eigener Schritt, damit dieser nur das Lösen enthält.
        self.observe_sketch(false);
        let broken = self
            .sketch_draft
            .as_mut()
            .map_or(0, |d| curves.iter().filter(|c| d.break_link(**c)).count());
        if broken == 0 {
            return self.fail(tr!("state-curve-not-projected"));
        }
        if let Some(se) = self.sketch_session.as_mut() {
            se.edit_label = Some(crate::msg!("action-sketch-break-link"));
        }
        self.observe_sketch(false);
        Ok(broken)
    }

    /// [`break_projection_links`](Self::break_projection_links) für eine Kurve.
    pub fn break_projection_link(&mut self, curve: CurveId) -> Result<(), String> {
        self.break_projection_links(&[curve]).map(|_| ())
    }

    // ---- Verlorene Projektionen verwalten (Skizzen 11a, Fusion „Manage Lost Projections“) ----

    /// Öffnet den Dialog „Verlorene Projektionen verwalten“ für die Skizze `sketch`; nur, wenn
    /// sie verlorene Projektionen hat (sonst nichts, `false`).
    pub fn open_lost_projections(&mut self, sketch: FeatureId) -> bool {
        if !crate::lost_projections_dialog::offers(self, sketch) {
            return false;
        }
        self.selected_feature = Some(sketch);
        self.lost_dialog = Some(crate::lost_projections_dialog::LostProjectionsDialog::new(
            sketch,
        ));
        true
    }

    /// Die Ziele der Skizze `sketch` mit verlorenem Bezug, wie der Dialog sie zeigt: aus der
    /// letzten Berechnung, ohne die in der offenen Skizze schon gelösten, gelöschten oder neu
    /// verknüpften (dort zählt der Entwurf, das Dokument rechnet erst beim Beenden).
    pub fn lost_rows(&self, sketch: FeatureId) -> Vec<CopyTarget> {
        let Some(f) = self.document.feature(sketch) else {
            return Vec::new();
        };
        let FeatureKind::Sketch(def) = &f.kind else {
            return Vec::new();
        };
        let lost = f.lost_projections();
        let draft = self
            .sketch_draft
            .as_ref()
            .filter(|_| self.open_sketch_id() == Some(sketch));
        let Some(draft) = draft else {
            return lost.to_vec();
        };
        lost.iter()
            .copied()
            .filter(|t| {
                let old = def
                    .copies
                    .iter()
                    .find(|c| c.target == *t)
                    .map(|c| &c.source);
                draft
                    .copies
                    .iter()
                    .any(|c| c.target == *t && Some(&c.source) == old)
            })
            .collect()
    }

    /// „Verknüpfung aufheben“ im Dialog für die Ziele `targets` der Skizze `sketch`: Ist sie
    /// offen, am Entwurf als ein Skizzen-Schritt, sonst am Dokument als ein Schritt
    /// ([`Document::unlink_projections`]); eine andere offene Skizze wird vorher beendet.
    /// Liefert die Zahl der gelösten Kopien (0: nichts geändert, kein Schritt).
    pub fn unlink_lost_projections(
        &mut self,
        sketch: FeatureId,
        targets: &[CopyTarget],
    ) -> Result<usize, String> {
        if self.open_sketch_id() == Some(sketch) {
            self.observe_sketch(false);
            let n = self.sketch_draft.as_mut().map_or(0, |d| {
                targets.iter().filter(|t| d.break_link_target(**t)).count()
            });
            if n > 0 {
                if let Some(se) = self.sketch_session.as_mut() {
                    se.edit_label = Some(crate::msg!("lost-break-link"));
                }
                self.observe_sketch(false);
            }
            return Ok(n);
        }
        self.try_close_sketch()?;
        match self.document.unlink_projections(sketch, targets) {
            Ok(n) => {
                if n > 0 {
                    self.undo_label_hint = Some(|| tr!("lost-break-link"));
                    self.refresh();
                }
                Ok(n)
            }
            Err(e) => self.fail(e.to_string()),
        }
    }

    /// „Geometrie löschen“ im Dialog: wie [`AppState::unlink_lost_projections`], löscht aber
    /// die Kurven und Punkte samt Kopie ([`Document::delete_lost_geometry`]); in der offenen
    /// Skizze dieselbe Regel am Entwurf. Liefert die Zahl der gelöschten Ziele.
    pub fn delete_lost_geometry(
        &mut self,
        sketch: FeatureId,
        targets: &[CopyTarget],
    ) -> Result<usize, String> {
        if self.open_sketch_id() == Some(sketch) {
            self.observe_sketch(false);
            let Some(draft) = self.sketch_draft.as_mut() else {
                return Ok(0);
            };
            // Derselbe Weg wie am Dokument; an einer Kopie, damit ein Fehler nichts ändert.
            let mut trial = draft.clone();
            let n = match planora_feature::delete_lost_targets(&mut trial, targets) {
                Ok(n) => n,
                Err(e) => return self.fail(e.to_string()),
            };
            if n > 0 {
                *draft = trial;
                if let Some(se) = self.sketch_session.as_mut() {
                    se.forget_removed(&draft.sketch);
                    se.edit_label = Some(crate::msg!("lost-delete-geometry"));
                }
                self.observe_sketch(false);
            }
            return Ok(n);
        }
        self.try_close_sketch()?;
        match self.document.delete_lost_geometry(sketch, targets) {
            Ok(n) => {
                if n > 0 {
                    self.undo_label_hint = Some(|| tr!("lost-delete-geometry"));
                    self.refresh();
                }
                Ok(n)
            }
            Err(e) => self.fail(e.to_string()),
        }
    }

    /// „Neu verknüpfen“ im Dialog (Ruling F9): öffnet die Skizze `sketch` (falls zu) und setzt
    /// die Wahl der Sitzung ([`crate::sketch_state::RelinkPick`]); der nächste passende Klick in
    /// der Ansicht (dieselbe Auswahl wie „Projizieren“ bzw. bei eingezogener 3D-Geometrie wie
    /// „3D-Geometrie einbeziehen“) ruft [`AppState::relink_pick`] bzw.
    /// [`AppState::relink_include_pick`]. Esc nimmt
    /// die Wahl zurück. Fehler (Meldung auch in `message`): keine Kopie, Art nicht wählbar. Bei
    /// offener Skizze zählen die Kopien des Entwurfs.
    pub fn start_relink(&mut self, sketch: FeatureId, target: CopyTarget) -> Result<(), String> {
        let copies = match self.sketch_draft.as_ref() {
            Some(draft) if self.open_sketch_id() == Some(sketch) => Some(&draft.copies),
            _ => match self.document.feature(sketch).map(|f| &f.kind) {
                Some(FeatureKind::Sketch(def)) => Some(&def.copies),
                _ => None,
            },
        };
        let family = copies
            .and_then(|cs| cs.iter().find(|c| c.target == target))
            .map(|c| c.source.clone());
        let Some(family) = family else {
            return self.fail(tr!("feature-relink-no-copy"));
        };
        if !relink_pickable(&family) {
            return self.fail(tr!("lost-relink-unsupported"));
        }
        if self.open_sketch_id() != Some(sketch) {
            self.open_sketch(sketch)?;
        }
        let Some((se, sk)) = self.session_sketch_mut() else {
            return self.fail(tr!("state-no-open-sketch"));
        };
        se.set_tool(sk, crate::sketch_state::Tool::Select);
        se.relink = Some(crate::sketch_state::RelinkPick { target, family });
        se.message = Some(crate::msg!("lost-relink-pick"));
        Ok(())
    }

    /// Der Klick der Wahl „Neu verknüpfen“ auf `picked` (Kante, Fläche oder Kurve einer früheren
    /// Skizze, wie bei „Projizieren“): Passt er zur Familie, wird er die neue Herkunft
    /// ([`AppState::relink_projection`]); sonst bleibt die Wahl, und die Meldung der Sitzung nennt
    /// den Grund.
    pub fn relink_pick(&mut self, picked: ProjectTarget) -> Result<(), String> {
        let r = self.try_relink_pick(picked);
        self.report_to_session(r.map(|()| 1)).map(|_| ())
    }

    fn try_relink_pick(&mut self, picked: ProjectTarget) -> Result<(), Msg> {
        let pick = self
            .sketch_session
            .as_ref()
            .and_then(|se| se.relink.clone())
            .ok_or_else(|| crate::msg!("state-no-open-sketch"))?;
        let source = self.relink_source(&pick.family, picked)?;
        self.try_relink(pick.target, source)
    }

    /// Wie [`AppState::relink_pick`] für eingezogene 3D-Geometrie (Ruling T18-1): Der Klick
    /// `pick` (dieselbe Auswahl wie „3D-Geometrie einbeziehen“: Ecke, Kante, Fläche, Ebene, Achse,
    /// Ursprung, Kurve einer früheren Skizze) wird über [`AppState::include_ref`] die neue Herkunft.
    /// Eine andere Art als die verlorene lehnt es ab (die Wahl bleibt).
    pub fn relink_include_pick(&mut self, pick: IncludePick) -> Result<(), String> {
        let r = self.try_relink_include(pick);
        self.report_to_session(r.map(|()| 1)).map(|_| ())
    }

    fn try_relink_include(&mut self, pick: IncludePick) -> Result<(), Msg> {
        let relink = self
            .sketch_session
            .as_ref()
            .and_then(|se| se.relink.clone())
            .ok_or_else(|| crate::msg!("state-no-open-sketch"))?;
        let source = CopySource::Include(self.include_ref(pick)?);
        if !relink.family.same_family(&source) {
            return Err(crate::msg!("lost-relink-wrong-pick"));
        }
        self.try_relink(relink.target, source)
    }

    /// Die neue Herkunft der Familie `family` aus dem Klick `picked`; Schnitt und Silhouette
    /// behalten den Stückindex der alten Herkunft.
    fn relink_source(&self, family: &CopySource, picked: ProjectTarget) -> Result<CopySource, Msg> {
        use planora_feature::IncludeRef;
        let tol = self.kernel.tolerance;
        let edge = |b: usize, e: Id<Edge>| -> Result<planora_feature::EdgeRef, Msg> {
            planora_feature::EdgeRef::from_edge(&self.solid_before_sketch(b)?, e, &tol)
                .map_err(|err| crate::msg!("state-edge-copy-failed", error = err.to_string()))
        };
        let face = |b: usize, f: Id<Face>| -> Result<planora_feature::FaceRef, Msg> {
            planora_feature::FaceRef::from_face(&self.solid_before_sketch(b)?, f, &tol)
                .map_err(|e| crate::msg!("state-face-link-failed", error = e.to_string()))
        };
        Ok(match (family, picked) {
            (CopySource::Edge(_), ProjectTarget::Edge(b, e)) => CopySource::Edge(edge(b, e)?),
            (CopySource::Include(IncludeRef::Edge(_)), ProjectTarget::Edge(b, e)) => {
                CopySource::Include(IncludeRef::Edge(edge(b, e)?))
            }
            (CopySource::Section { index, .. }, ProjectTarget::Face(b, f)) => CopySource::Section {
                face: face(b, f)?,
                index: *index,
            },
            (CopySource::Silhouette { index, .. }, ProjectTarget::Face(b, f)) => {
                CopySource::Silhouette {
                    face: face(b, f)?,
                    index: *index,
                }
            }
            (CopySource::Include(IncludeRef::FaceBoundary(_)), ProjectTarget::Face(b, f)) => {
                CopySource::Include(IncludeRef::FaceBoundary(face(b, f)?))
            }
            // Isokurve (Skizzen 11b): neue Fläche, Richtung und Anteil bleiben.
            (
                CopySource::Isoparametric {
                    direction, value, ..
                },
                ProjectTarget::Face(b, f),
            ) => CopySource::Isoparametric {
                face: face(b, f)?,
                direction: *direction,
                value: *value,
            },
            (CopySource::SketchCurve { .. }, ProjectTarget::SketchCurve(sketch, curve)) => {
                self.check_source_sketch(sketch)?;
                CopySource::SketchCurve { sketch, curve }
            }
            (
                CopySource::Include(IncludeRef::SketchCurve { .. }),
                ProjectTarget::SketchCurve(sketch, curve),
            ) => {
                self.check_source_sketch(sketch)?;
                CopySource::Include(IncludeRef::SketchCurve { sketch, curve })
            }
            _ => return Err(crate::msg!("lost-relink-wrong-pick")),
        })
    }

    /// „Neu verknüpfen“ in der offenen Skizze: gibt der Kopie mit dem Ziel `target` (bei Gruppen
    /// der ganzen Gruppe, [`planora_feature::relink_copies`]) die Herkunft `source` und führt sie
    /// sofort nach, wie die Neuberechnung ([`Document::follow_draft_copies`]). Genau ein
    /// Skizzen-Schritt („Neu verknüpfen“); die Wahl endet. Andere Familie, keine Kopie oder eine
    /// Quelle, der die Kurve nicht folgen kann: Fehler, nichts geändert (die Wahl bleibt).
    pub fn relink_projection(
        &mut self,
        target: CopyTarget,
        source: CopySource,
    ) -> Result<(), String> {
        let r = self.try_relink(target, source);
        self.report_to_session(r.map(|()| 1)).map(|_| ())
    }

    fn try_relink(&mut self, target: CopyTarget, source: CopySource) -> Result<(), Msg> {
        let id = self
            .open_sketch_id()
            .ok_or_else(|| crate::msg!("state-no-open-sketch"))?;
        // Offene Änderungen vorher als eigener Schritt, damit dieser nur das Verknüpfen enthält.
        self.observe_sketch(false);
        let draft = self
            .sketch_draft
            .as_ref()
            .ok_or_else(|| crate::msg!("state-no-open-sketch"))?;
        let old = draft
            .copies
            .iter()
            .find(|c| c.target == target)
            .map(|c| c.source.clone())
            .ok_or_else(|| crate::msg!("feature-relink-no-copy"))?;
        if !old.same_family(&source) {
            return Err(crate::msg!("feature-relink-kind"));
        }
        if old == source {
            // Dieselbe Herkunft: nichts zu tun, die Wahl endet ohne Schritt.
            if let Some(se) = self.sketch_session.as_mut() {
                se.relink = None;
                se.message = None;
            }
            return Ok(());
        }
        let mut trial = draft.clone();
        planora_feature::relink_copies(&mut trial, target, &old, &source);
        let group: Vec<CopyTarget> = trial
            .copies
            .iter()
            .filter(|c| c.source == source)
            .map(|c| c.target)
            .collect();
        let lost = self
            .document
            .follow_draft_copies(id, &mut trial, &group)
            .map_err(|e| Msg::Raw(e.to_string()))?;
        if lost.contains(&target) {
            return Err(crate::msg!("lost-relink-failed"));
        }
        self.sketch_draft = Some(trial);
        if let Some(se) = self.sketch_session.as_mut() {
            se.relink = None;
            se.message = None;
            se.edit_label = Some(crate::msg!("lost-relink"));
        }
        self.observe_sketch(false);
        Ok(())
    }

    /// Hängt für jeden beim Fang an einer Körperecke eingezogenen Punkt der offenen Skizze, der
    /// noch lebt, seine Verknüpfung mit der Ecke an (`CopySource::Include(Vertex)`, A7) und leert
    /// die Liste der Sitzung.
    fn link_included_vertices(&mut self) {
        let (Some(mut se), Some(mut draft)) =
            (self.sketch_session.take(), self.sketch_draft.take())
        else {
            return;
        };
        link_vertices(self, &mut se, &mut draft);
        self.sketch_session = Some(se);
        self.sketch_draft = Some(draft);
    }

    /// Die Körperecken vor der Skizze `id` in Skizzenkoordinaten von `frame`, je Ecke einmal
    /// (Fang beim Zeichnen in 3D, A7). Ausgeblendete Körper fangen nicht.
    fn body_vertices(&self, id: FeatureId, frame: &Frame) -> Vec<crate::sketch_draw3::BodyVertex> {
        let visible = self.document.visible_bodies_before(id);
        let mut out = Vec::new();
        for (j, b) in self.bodies.iter().enumerate() {
            if self.is_body_hidden(j) {
                continue;
            }
            let Some(v) = visible.iter().find(|v| v.body == b.source) else {
                continue;
            };
            let solid = v.solid;
            let mut seen = std::collections::HashSet::new();
            for (e, edge) in solid.edges.iter() {
                for (vid, end) in [(edge.start, false), (edge.end, true)] {
                    if seen.insert(vid)
                        && let Some(vx) = solid.vertices.get(vid)
                    {
                        out.push(crate::sketch_draw3::BodyVertex {
                            at: frame.to_local(vx.point),
                            body: j,
                            edge: e,
                            end,
                        });
                    }
                }
            }
        }
        out
    }

    // ---- Rückgängig in der offenen Skizze (R10) ------------------------------------------

    /// Verlauf der offenen Skizze (lesen); `None` ohne offene Skizze (er lebt genau so lange
    /// wie die Sitzung).
    pub fn sketch_history(&self) -> Option<&History<SketchFeature>> {
        self.sketch_history.as_ref()
    }

    /// Eine noch nicht als Schritt erfasste Änderung am Entwurf (er gleicht nicht dem letzten
    /// Bild: anderer Stand der Skizze oder andere Kopien): Verlauf und Entwurf, dazu, ob sie
    /// aufgeschoben ist ([`deferred_by_tool`], offener erster Klick eines Werkzeugs).
    fn sketch_open_change(&self) -> Option<(&History<SketchFeature>, &SketchFeature, bool)> {
        let se = self.sketch_session.as_ref()?;
        let h = self.sketch_history.as_ref()?;
        let draft = self.sketch_draft.as_ref()?;
        if draft == h.current() {
            return None;
        }
        Some((h, draft, deferred_by_tool(se, h.current(), draft)))
    }

    /// Am Ende eines Bildes bei offener Skizze (R4, R10): Weicht der Entwurf vom letzten Bild ab
    /// und läuft keine Geste (Maustaste, Eingabefeld), wird er ein Schritt mit dem Namen aus
    /// dem Vergleich ([`describe_sketch_change`](crate::undo_labels::describe_sketch_change)).
    /// Ziehen eines Punktes ist so ein Schritt; der erste Klick eines Werkzeugs bleibt offen,
    /// bis das Element entsteht.
    /// Vorher entfallen im Entwurf die Formeln an Maßen, die keine haben können (gelöscht,
    /// anzeigend), damit kein Bild des Verlaufs sie trägt.
    pub fn observe_sketch(&mut self, gesture: bool) {
        if let Some(draft) = self.sketch_draft.as_mut() {
            draft.prune_formulas();
        }
        if gesture {
            return;
        }
        let Some((_, _, false)) = self.sketch_open_change() else {
            // Kein Schritt: ein Schrittname eines Werkzeugs gehört zu keiner Änderung mehr.
            if self.sketch_open_change().is_none()
                && let Some(se) = self.sketch_session.as_mut()
            {
                se.edit_label = None;
                se.included_vertices.clear();
                se.replace_step = false;
            }
            return;
        };
        // Die Verknüpfungen eingezogener Eckpunkte (A7) kommen mit dem Schritt dazu.
        self.link_included_vertices();
        let Some((h, draft, _)) = self.sketch_open_change() else {
            return;
        };
        // Bestätigtes Verschieben der eingefügten Kopie: kein zweiter Schritt, das Bild des
        // Schritts „Einfügen“ wird ersetzt (Skizzen 11b, Spec-Entscheidung 5). Der Name bleibt.
        let replace = self
            .sketch_session
            .as_ref()
            .is_some_and(|se| se.replace_step)
            && h.undo_len() > 0;
        if replace {
            let after = draft.clone();
            let bytes = sketch_bytes(&after);
            if let Some(h) = self.sketch_history.as_mut() {
                h.replace_current(after, bytes);
            }
            if let Some(se) = self.sketch_session.as_mut() {
                se.edit_label = None;
                se.replace_step = false;
            }
            return;
        }
        let label = self.sketch_label(h.current(), draft);
        crate::diag::step(&crate::diag::in_english(|| {
            self.sketch_label(h.current(), draft)
        }));
        let after = draft.clone();
        let bytes = sketch_bytes(&after);
        if let Some(h) = self.sketch_history.as_mut() {
            h.record(after, bytes, label);
        }
        if let Some(se) = self.sketch_session.as_mut() {
            se.edit_label = None;
        }
    }

    // ---- Zwischenablage der Skizze (Skizzen 11b, Task 3) ----------------------------------

    /// Die Auswahl der offenen Skizze als Zwischenablage. Ändert die Skizze nicht; die Meldung
    /// sagt, wie viele Elemente es sind. Fehler: keine Skizze offen, nichts gewählt, Text auf
    /// einem Pfad ohne seinen Pfad.
    pub fn copy_sketch_selection(&mut self) -> Result<planora_feature::FeatureClip, Msg> {
        let (Some(se), Some(sf)) = (self.sketch_session.as_mut(), self.sketch_draft.as_ref())
        else {
            return Err(crate::msg!("state-no-open-sketch"));
        };
        let set = clip_set_of(se, &sf.sketch);
        let n = set.curves.len() + set.points.len();
        if n == 0 {
            let m = crate::msg!("sketch-nothing-selected");
            se.message = Some(m.clone());
            return Err(m);
        }
        let frame = *sf.sketch.frame();
        match planora_feature::copy_selection(sf, &frame, &set) {
            Ok(clip) => {
                se.message = Some(crate::msg!("sketch-copy-done", n = n));
                Ok(clip)
            }
            Err(e) => {
                let m = crate::sketch_state::err(e);
                se.message = Some(m.clone());
                Err(m)
            }
        }
    }

    /// Ausschneiden: kopieren und die Auswahl wie mit Entf löschen (Löschregel der Skizze), ein
    /// Schritt „Ausschneiden“. Scheitert das Kopieren, bleibt die Skizze, wie sie war.
    pub fn cut_sketch_selection(&mut self) -> Result<planora_feature::FeatureClip, Msg> {
        // Dieselben Sperren wie Entf (`sketch_view`): mitten in einem Zug, in der offenen Anordnung
        // und im offenen Abschnitt „Text“ gehört die Auswahl dem Werkzeug; dann geschieht nichts
        // (auch keine Zwischenablage, keine Meldung).
        if self
            .sketch_session
            .as_ref()
            .is_some_and(|se| se.in_gesture() || se.runs_pattern() || se.runs_text())
        {
            return Err(Msg::Join(Vec::new()));
        }
        // Was davor offen war, ist ein eigener Schritt.
        self.observe_sketch(false);
        let clip = self.copy_sketch_selection()?;
        if let Some((se, sf)) = self.session_draft_mut() {
            se.delete_selection_in(sf);
            sf.prune_formulas();
            se.edit_label = Some(crate::msg!("undo-sketch-cut"));
        }
        self.observe_sketch(false);
        Ok(clip)
    }

    /// Einfügen: die Zwischenablage an die Lage der Quelle in die offene Skizze, ein Schritt
    /// „Einfügen“. Danach sind alle eingefügten Elemente gewählt und Verschieben/Kopieren ist
    /// damit offen (`TransformState::pasted`); ein bestätigtes Verschieben ersetzt das Bild dieses
    /// Schritts, ein Abbruch lässt die Kopie an der Quelllage. Enthält die Kopie einen festen
    /// Spline, bleibt sie gewählt, das Werkzeug geht nicht auf, und eine Meldung sagt es. Ein
    /// Fehler (etwa „sketch-move-fixed“ beim Einfügen von 3D-Inhalt in eine andere Ebene) ändert
    /// nichts.
    pub fn paste_sketch_clip(
        &mut self,
        clip: &planora_feature::FeatureClip,
    ) -> Result<planora_feature::PasteReport, Msg> {
        use crate::sketch_state::{Item, Tool};
        if self.sketch_session.is_none() || self.sketch_draft.is_none() {
            return Err(crate::msg!("state-no-open-sketch"));
        }
        // Was davor offen war, ist ein eigener Schritt; ein laufendes Werkzeug endet.
        self.observe_sketch(false);
        let scope = self.document.param_scope();
        let fonts = self.fonts.clone();
        let Some((se, sf)) = self.session_draft_mut() else {
            return Err(crate::msg!("state-no-open-sketch"));
        };
        se.set_tool(&mut sf.sketch, Tool::Select);
        let frame = *sf.sketch.frame();
        let before = sf.clone();
        let report = match planora_feature::paste_clip(sf, &frame, clip, &scope) {
            Ok(r) => r,
            Err(e) => {
                let m = crate::sketch_state::err(e);
                se.message = Some(m.clone());
                return Err(m);
            }
        };
        // Parameter im Text (Skizzen 11b): Ein eingefügter Text zeigt den Wert dieses Dokuments
        // (ein fehlender Parameter steht wörtlich da und warnt), im selben Schritt neu gesetzt.
        // Ohne Schriftliste setzt das Schließen der Skizze neu (`try_close_sketch`).
        planora_feature::text_refs::apply_shown(&mut sf.sketch, &scope);
        if let Some(lib) = &fonts {
            sf.sketch
                .reshape_texts(&crate::fonts_library::Guarded(lib.as_ref()));
        }
        if let Err(m) = se.solve_checked(&mut sf.sketch, &before.sketch) {
            *sf = before;
            se.message = Some(m.clone());
            return Err(m);
        }
        // Alle eingefügten Elemente sind gewählt: die Kurven und die freien Punkte. Ein neu
        // gesetzter Text kann andere Kurven haben als die Quelle: nur lebende, dazu alle
        // heutigen Kurven der eingefügten Texte.
        let mut curves: Vec<CurveId> = report
            .map
            .curves
            .values()
            .copied()
            .filter(|&c| sf.sketch.curve(c).is_some())
            .collect();
        for t in report.map.texts.values() {
            if let Some(d) = sf.sketch.text(*t) {
                for &c in &d.curves {
                    if !curves.contains(&c) {
                        curves.push(c);
                    }
                }
            }
        }
        let grown = sf.sketch.transform_set(&curves, &[]);
        let mut selection: Vec<Item> = curves.iter().map(|&c| Item::Curve(c)).collect();
        selection.extend(
            report
                .map
                .points
                .values()
                .filter(|p| !grown.points.contains(p) && sf.sketch.point_xy(**p).is_ok())
                .map(|&p| Item::Point(p)),
        );
        let points: Vec<PointId> = report
            .map
            .points
            .values()
            .copied()
            .filter(|&p| sf.sketch.point_xy(p).is_ok())
            .collect();
        se.selection = selection.clone();
        se.drop_curve_picks();
        se.edit_label = Some(crate::msg!("undo-sketch-paste"));
        let mut notes = Vec::new();
        if clip.made_free > 0 {
            notes.push(crate::msg!("sketch-paste-made-free", n = clip.made_free));
        }
        if report.formulas_dropped > 0 {
            notes.push(crate::msg!(
                "sketch-paste-formulas-dropped",
                n = report.formulas_dropped
            ));
        }
        if report.constraints_dropped > 0 {
            notes.push(crate::msg!(
                "sketch-move-dropped",
                n = report.constraints_dropped
            ));
        }
        // Ein fester Spline in der Kopie: Sie lässt sich nicht verschieben.
        let movable = !paste_is_fixed(&sf.sketch, &curves, &points);
        if !movable {
            notes.push(crate::msg!("sketch-paste-fixed-spline"));
        }
        se.message = match notes.len() {
            0 => None,
            1 => notes.pop(),
            _ => Some(Msg::Join(notes)),
        };
        // Die Kopie ist ein Schritt, bevor sie bewegt wird.
        self.observe_sketch(false);
        if movable && let Some((se, sf)) = self.session_draft_mut() {
            let message = se.message.take();
            se.set_tool(&mut sf.sketch, Tool::MoveCopy);
            se.transform.selection_before = selection;
            se.transform.pasted = true;
            se.message = message;
        }
        Ok(report)
    }

    /// Rückgängig in der offenen Skizze. Offene Änderungen werden vorher ein Schritt. Bleibt
    /// danach nur ein offener erster Klick eines Werkzeugs (aufgeschoben), nimmt Rückgängig nur
    /// diesen zurück (wie Esc, samt dem gesetzten Punkt) und lässt den Verlauf, wie er ist; sonst
    /// den letzten Schritt. Der Entwurf wird jeweils der Klon des Bildes (gleicher Stand);
    /// laufendes Werkzeug, Auswahl, Ziehen und Meldung entfallen, der Lösungsbericht wird neu
    /// bestimmt ([`SketchSession::after_history_jump`]). Das Dokument bleibt unberührt.
    /// `false`: keine Skizze offen oder nichts zurückzunehmen.
    pub fn undo_sketch(&mut self) -> bool {
        self.sketch_jump(true)
    }

    /// Gegenstück zu [`AppState::undo_sketch`]; bei einem offenen ersten Klick gibt es nichts zu
    /// wiederholen.
    pub fn redo_sketch(&mut self) -> bool {
        self.sketch_jump(false)
    }

    fn sketch_jump(&mut self, back: bool) -> bool {
        // Mitten im Griffzug nimmt Rückgängig nur den Zug zurück: Die Skizze hat er nicht berührt.
        if back
            && let Some(se) = self.sketch_session.as_mut()
            && se.grip_drag.is_some()
        {
            se.abort_grip();
            return true;
        }
        // Ebenso mitten im Griffzug an der Skizze: Er geht ganz zurück, auch ein Einschalten.
        if back
            && let Some((se, sk)) = self.session_sketch_mut()
            && se.abort_handle_drag(sk)
        {
            return true;
        }
        self.observe_sketch(false);
        let AppState {
            sketch_session: Some(se),
            sketch_draft: Some(draft),
            sketch_history: Some(h),
            ..
        } = self
        else {
            return false;
        };
        let target = if *draft != *h.current() {
            // Nur noch ein aufgeschobener erster Klick ist offen.
            if !back {
                return false;
            }
            h.current().clone()
        } else {
            let target = if back { h.peek_undo() } else { h.peek_redo() };
            let Some(target) = target.cloned() else {
                return false;
            };
            if back {
                h.commit_undo();
            } else {
                h.commit_redo();
            }
            target
        };
        *draft = target;
        se.after_history_jump(&draft.sketch);
        true
    }

    /// Name des Schritts, den Rückgängig in der offenen Skizze zurücknähme (eine noch offene
    /// Änderung zählt mit, auch ein offener erster Klick); `None`: nichts da.
    pub fn sketch_undo_label(&self) -> Option<String> {
        match self.sketch_open_change() {
            // Nur ein offener erster Klick: Rückgängig nimmt diesen Punkt zurück.
            Some((h, draft, true)) => Some(crate::undo_labels::describe_sketch_change(
                h.current(),
                draft,
            )),
            Some((h, draft, false)) => Some(self.sketch_label(h.current(), draft)),
            None => self.sketch_history()?.undo_label().map(str::to_string),
        }
    }

    /// Name eines Skizzenschritts; offene Punkte des laufenden Werkzeugs gehören zum nächsten
    /// Element und benennen ihn nicht.
    ///
    /// Hat ein Werkzeug den Namen gesetzt (Stutzen, Dehnen, Brechen: `edit_label`), gilt
    /// dieser; [`observe_sketch`](Self::observe_sketch) leert ihn mit dem Schritt.
    fn sketch_label(&self, before: &SketchFeature, after: &SketchFeature) -> String {
        if let Some(label) = self
            .sketch_session
            .as_ref()
            .and_then(|se| se.edit_label.as_ref())
        {
            return label.text();
        }
        let open = self
            .sketch_session
            .as_ref()
            .map_or(&[][..], |se| se.pending.as_slice());
        crate::undo_labels::describe_sketch_change_beside(before, after, open)
    }

    /// Name des Schritts, den Wiederholen in der offenen Skizze herstellte. Bei einer offenen
    /// Änderung (auch einem offenen ersten Klick) gibt es nichts zu wiederholen: `None`.
    pub fn sketch_redo_label(&self) -> Option<String> {
        if self.sketch_open_change().is_some() {
            return None;
        }
        self.sketch_history()?.redo_label().map(str::to_string)
    }

    /// Verwirft Sitzung und Entwurf (das Skizzen-Feature gibt es nicht mehr).
    fn discard_sketch(&mut self) {
        self.sketch_draft = None;
        self.sketch_history = None;
        if self.sketch_session.take().is_some() {
            self.restore_camera();
        }
    }

    fn restore_camera(&mut self) {
        self.sketch_fit = None;
        if let Some(mut cam) = self.saved_camera.take() {
            // Seitenverhältnis und freies Rechteck gehören zum aktuellen Fenster.
            cam.aspect = self.camera.aspect;
            cam.ndc_offset = self.camera.ndc_offset;
            cam.visible_aspect = self.camera.visible_aspect;
            cam.visible_height_fraction = self.camera.visible_height_fraction;
            self.camera = cam;
        }
        self.rebuild_scene();
    }

    /// Nach dem Löschen von Features: Auswahl, Dialogwünsche und eine Sitzung auf gelöschte
    /// Features aufgeben. Eine überlebende Skizze ist vor dem Löschen schon beendet worden
    /// (ihr Entwurf übernommen), Kantenkopien hängen also im Dokument und werden mitgelöscht.
    fn forget_deleted(&mut self) {
        if self
            .selected_feature
            .is_some_and(|id| self.document.feature(id).is_none())
        {
            self.selected_feature = None;
        }
        let gone = |doc: &Document, id: &FeatureId| doc.feature(*id).is_none();
        if self
            .edit_request
            .as_ref()
            .is_some_and(|id| gone(&self.document, id))
        {
            self.edit_request = None;
        }
        if self
            .rename_draft
            .as_ref()
            .is_some_and(|(id, _)| gone(&self.document, id))
        {
            self.rename_draft = None;
        }
        self.forget_gone_groups();
        if self
            .open_sketch_id()
            .is_some_and(|id| gone(&self.document, &id))
        {
            self.discard_sketch();
        }
    }

    /// Projiziert die Kante `edge` des sichtbaren Körpers `body` verknüpft in die geöffnete
    /// Skizze (Kurzform von [`AppState::project_into_sketch`]).
    pub fn project_edge_into_sketch(&mut self, body: usize, edge: Id<Edge>) -> Result<(), String> {
        self.project_into_sketch(ProjectTarget::Edge(body, edge), true)
            .map(|_| ())
    }

    /// Fusion „Projizieren“ (Skizzen 6): projiziert eine Kante, alle Kanten der Schleifen einer
    /// Fläche (außen und Löcher) oder alle Kanten eines Körpers in die geöffnete Skizze, als feste,
    /// normale Kurven. Kanten, die nur ein Punkt wären, und Nähte entfallen; deckungsgleiche
    /// Projektionen kommen nur einmal, auch gegen schon projizierte Kurven der Skizze. Mit `link`
    /// bekommt jede Kurve ihren Verweis auf die Kante und folgt dem Körper. Freiformkanten werden
    /// übersprungen und in der Meldung gezählt. Nur Körper, die vor dem Skizzen-Feature sichtbar
    /// sind, sind erlaubt. Liefert die Zahl neuer Kurven; alles ist ein Rückgängig-Schritt.
    /// Fehler stehen auch in der Meldung der Sitzung.
    pub fn project_into_sketch(
        &mut self,
        target: ProjectTarget,
        link: bool,
    ) -> Result<usize, String> {
        let r = self.try_project(target, link);
        self.report_to_session(r)
    }

    /// Fusion „Schneiden“ (Skizzen 6): schneidet die Skizzenebene mit einer Fläche oder allen
    /// Flächen eines Körpers und legt je Schnittstück eine feste, normale Kurve in die geöffnete
    /// Skizze. Jede Kurve ist mit ihrer Fläche verknüpft (`CopySource::Section`) und folgt dem
    /// Körper. Kann der Rechner eine getroffene Fläche nicht schneiden (Freiform, Lage ohne
    /// Rechenweg), entsteht nichts und die Meldung nennt den Grund. Kanten lassen sich nicht
    /// schneiden. Liefert die Zahl neuer Kurven; alles ist ein Rückgängig-Schritt. Fehler stehen
    /// auch in der Meldung der Sitzung.
    pub fn intersect_into_sketch(&mut self, target: ProjectTarget) -> Result<usize, String> {
        let r = self.try_intersect(target);
        self.report_to_session(r)
    }

    /// Fusion „Silhouette“ (Skizzen 6): legt den Umriss der Fläche `face` des sichtbaren Körpers
    /// `body` in Blickrichtung der Skizzenebene (ihre Normale) als feste, normale Kurven in die
    /// geöffnete Skizze, verknüpft über `CopySource::Silhouette`. Fehler des Rechners wie bei
    /// [`AppState::intersect_into_sketch`]. Ein Rückgängig-Schritt.
    pub fn silhouette_into_sketch(&mut self, body: usize, face: Id<Face>) -> Result<usize, String> {
        let r = self.try_silhouette(body, face);
        self.report_to_session(r)
    }

    /// Ein Fehler eines Befehls steht auch in der Meldung der Skizzensitzung.
    fn report_to_session(&mut self, r: Result<usize, Msg>) -> Result<usize, String> {
        r.map_err(|m| {
            let text = m.text();
            if let Some(se) = self.sketch_session.as_mut() {
                se.message = Some(m);
            }
            text
        })
    }

    /// Der Körper `body` (Index in [`AppState::bodies`]), wie er vor der geöffneten Skizze
    /// sichtbar ist; nur diese Körper lassen sich in die Skizze bringen.
    pub(crate) fn solid_before_sketch(&self, body: usize) -> Result<Solid, Msg> {
        let id = self
            .open_sketch_id()
            .ok_or_else(|| crate::msg!("state-no-open-sketch"))?;
        let source = self
            .bodies
            .get(body)
            .map(|b| b.source)
            .ok_or_else(|| crate::msg!("state-body-not-found"))?;
        self.document
            .visible_bodies_before(id)
            .into_iter()
            .find(|v| v.body == source)
            .map(|v| v.solid.clone())
            .ok_or_else(|| crate::msg!("state-edge-body-later"))
    }

    fn try_project(&mut self, target: ProjectTarget, link: bool) -> Result<usize, Msg> {
        let body = match target {
            ProjectTarget::Edge(b, _) | ProjectTarget::Face(b, _) | ProjectTarget::Body(b) => b,
            ProjectTarget::SketchCurve(sketch, curve) => {
                // Skizzen 10, Spec 3: dieselbe Rechnung wie die Neuberechnung
                // (`CopySource::SketchCurve`, Kurve in Raumlage, wie eine Kante projiziert).
                let piece = self.source_sketch_curve(sketch, curve)?;
                let source = link.then_some(CopySource::SketchCurve { sketch, curve });
                return self.place_projection(
                    &[piece],
                    &[source],
                    true,
                    0,
                    crate::msg!("sketch-project-nothing-new"),
                    crate::msg!("undo-edge-project"),
                );
            }
        };
        let solid = self.solid_before_sketch(body)?;
        let tol = self.kernel.tolerance;
        let single = matches!(target, ProjectTarget::Edge(..));
        let edges = match target {
            ProjectTarget::Edge(_, e) => vec![e],
            ProjectTarget::Face(_, f) => {
                face_edges(&solid, f).ok_or_else(|| crate::msg!("state-face-not-found"))?
            }
            ProjectTarget::Body(_) => body_edges(&solid),
            // Oben schon abgeschlossen.
            ProjectTarget::SketchCurve(..) => Vec::new(),
        };
        // Je Kante Kurvenstück und (mit Verknüpfung) Verweis; eine Kante ohne Verweis wird
        // übersprungen, bei einer einzelnen Kante ist das ein Fehler wie früher.
        let mut pieces = Vec::with_capacity(edges.len());
        let mut sources = Vec::with_capacity(edges.len());
        // Übersprungene Kanten ohne Verweis (Freiform zählt die Sitzung selbst).
        let mut skipped_other = 0;
        for e in edges {
            let edge_ref = if link {
                match planora_feature::EdgeRef::from_edge(&solid, e, &tol) {
                    Ok(r) => Some(CopySource::Edge(r)),
                    Err(err) if single => {
                        return Err(crate::msg!(
                            "state-edge-copy-failed",
                            error = err.to_string()
                        ));
                    }
                    Err(_) => {
                        skipped_other += 1;
                        continue;
                    }
                }
            } else {
                None
            };
            let Some(edge) = solid.edges.get(e) else {
                return Err(crate::msg!("state-edge-copy-failed", error = String::new()));
            };
            pieces.push((edge.curve.clone(), edge.t0, edge.t1));
            sources.push(edge_ref);
        }
        // Der Schritt heißt wie der Befehl (Fusion „Projizieren“), auch ohne Verknüpfung.
        self.place_projection(
            &pieces,
            &sources,
            single,
            skipped_other,
            crate::msg!("sketch-project-nothing-new"),
            crate::msg!("undo-edge-project"),
        )
    }

    fn try_intersect(&mut self, target: ProjectTarget) -> Result<usize, Msg> {
        let (body, faces) = match target {
            ProjectTarget::Edge(..) => return Err(crate::msg!("sketch-pick-intersect")),
            ProjectTarget::SketchCurve(sketch, curve) => {
                return self.try_intersect_curve(sketch, curve);
            }
            ProjectTarget::Face(b, f) => (b, Some(f)),
            ProjectTarget::Body(b) => (b, None),
        };
        let solid = self.solid_before_sketch(body)?;
        let frame = *self
            .sketch_draft
            .as_ref()
            .ok_or_else(|| crate::msg!("state-no-open-sketch"))?
            .sketch
            .frame();
        let faces: Vec<Id<Face>> = match faces {
            Some(f) if solid.faces.get(f).is_some() => vec![f],
            Some(_) => return Err(crate::msg!("state-face-not-found")),
            None => solid.faces.iter().map(|(f, _)| f).collect(),
        };
        let tol = self.kernel.tolerance;
        // Je Fläche einzeln geschnitten: Der Index eines Stücks zählt unter den Stücken seiner
        // Fläche, so wie `follow_copy` im Feature ihn beim Nachführen auflöst.
        self.place_face_pieces(
            &solid,
            &faces,
            |f| {
                planora_kernel::section_by_plane(&solid, &frame, Some(&[f]), &tol)
                    .map_err(|e| section_error(&e, true))
            },
            true,
        )
    }

    fn try_silhouette(&mut self, body: usize, face: Id<Face>) -> Result<usize, Msg> {
        let solid = self.solid_before_sketch(body)?;
        if solid.faces.get(face).is_none() {
            return Err(crate::msg!("state-face-not-found"));
        }
        let view = self
            .sketch_draft
            .as_ref()
            .ok_or_else(|| crate::msg!("state-no-open-sketch"))?
            .sketch
            .frame()
            .z;
        let tol = self.kernel.tolerance;
        self.place_face_pieces(
            &solid,
            &[face],
            |f| {
                planora_kernel::silhouette(&solid, f, view, &tol)
                    .map_err(|e| section_error(&e, false))
            },
            false,
        )
    }

    /// Schneiden und Silhouette: die Stücke der Flächen `faces` (je Fläche aus `pieces_of`),
    /// jedes verknüpft mit seiner Fläche und seinem Index unter den Stücken dieser Fläche
    /// (`section`: Schnitt, sonst Umriss). Ein Fehler des Rechners bricht alles ab, bevor etwas
    /// angelegt ist. Eine Fläche ohne Verweis zählt mit ihren Stücken als übersprungen; ist sie die
    /// einzige, ist das ein Fehler.
    fn place_face_pieces(
        &mut self,
        solid: &Solid,
        faces: &[Id<Face>],
        pieces_of: impl Fn(Id<Face>) -> Result<Vec<planora_kernel::SectionPiece>, Msg>,
        section: bool,
    ) -> Result<usize, Msg> {
        let tol = self.kernel.tolerance;
        let mut pieces = Vec::new();
        let mut sources = Vec::new();
        let mut skipped_other = 0;
        for &f in faces {
            let found = pieces_of(f)?;
            if found.is_empty() {
                continue;
            }
            let face = match planora_feature::FaceRef::from_face(solid, f, &tol) {
                Ok(r) => r,
                Err(e) if faces.len() == 1 => {
                    return Err(crate::msg!("state-face-link-failed", error = e.to_string()));
                }
                Err(_) => {
                    skipped_other += found.len();
                    continue;
                }
            };
            for (index, p) in found.into_iter().enumerate() {
                let face = face.clone();
                sources.push(Some(if section {
                    CopySource::Section { face, index }
                } else {
                    CopySource::Silhouette { face, index }
                }));
                pieces.push((p.curve, p.t0, p.t1));
            }
        }
        if pieces.is_empty() && skipped_other == 0 {
            return Err(if section {
                crate::msg!("sketch-intersect-nothing")
            } else {
                crate::msg!("sketch-silhouette-nothing")
            });
        }
        let (nothing, label) = if section {
            (
                crate::msg!("sketch-intersect-nothing-new"),
                crate::msg!("undo-sketch-intersect"),
            )
        } else {
            (
                crate::msg!("sketch-silhouette-nothing-new"),
                crate::msg!("undo-silhouette"),
            )
        };
        self.place_projection(&pieces, &sources, false, skipped_other, nothing, label)
    }

    /// Die Kurve `curve` der Skizze `sketch` in Raumlage als Quelle für die offene Skizze: Die
    /// Quelle muss im Verlauf davor stehen (Spec 10) und berechnet sein.
    fn source_sketch_curve(
        &self,
        sketch: FeatureId,
        curve: CurveId,
    ) -> Result<(planora_kernel::Curve, f64, f64), Msg> {
        self.check_source_sketch(sketch)?;
        self.document
            .sketch_of(sketch)
            .and_then(|sk| sk.curve_world(curve))
            .ok_or_else(|| crate::msg!("sketch-include-failed"))
    }

    /// Steht die Skizze `sketch` im Verlauf vor der offenen (Spec 10)? Sonst die Meldung.
    pub(crate) fn check_source_sketch(&self, sketch: FeatureId) -> Result<(), Msg> {
        let id = self
            .open_sketch_id()
            .ok_or_else(|| crate::msg!("state-no-open-sketch"))?;
        if planora_feature::sketch_source_allowed(&self.document, id, sketch) {
            Ok(())
        } else {
            Err(crate::msg!("sketch-source-sketch-later"))
        }
    }

    /// Stelle der offenen Skizze im Verlauf und ihre Ebene (Skizzenkoordinaten).
    fn open_sketch_place(&self) -> Result<(usize, Frame), Msg> {
        let no = || crate::msg!("state-no-open-sketch");
        let id = self.open_sketch_id().ok_or_else(no)?;
        let idx = self.document.index_of(id).ok_or_else(no)?;
        let frame = *self.sketch_draft.as_ref().ok_or_else(no)?.sketch.frame();
        Ok((idx, frame))
    }

    /// Schneiden mit einer Skizzenkurve (Skizzen 10, Spec 3): je Schnittpunkt mit der Ebene ein
    /// fester Punkt, alle in einer Gruppe mit der Herkunft `CopySource::SketchCurve` (dieselbe
    /// Rechnung wie die Neuberechnung). Ein Rückgängig-Schritt.
    fn try_intersect_curve(&mut self, sketch: FeatureId, curve: CurveId) -> Result<usize, Msg> {
        self.source_sketch_curve(sketch, curve)?;
        let (idx, frame) = self.open_sketch_place()?;
        let points =
            planora_feature::sketch_curve_points(&self.document, idx, &frame, sketch, curve)
                .ok_or_else(|| crate::msg!("sketch-intersect-curve-in-plane"))?;
        if points.is_empty() {
            return Err(crate::msg!("sketch-intersect-nothing"));
        }
        let pieces: Vec<Included> = points.into_iter().map(Included::Point).collect();
        self.place_included(
            &pieces,
            CopySource::SketchCurve { sketch, curve },
            crate::msg!("undo-sketch-intersect"),
        )
    }

    /// Fusion „3D-Geometrie einbeziehen“ (Skizzen 10, Spec 5, A4): holt Kante, Flächenrand,
    /// Ecke, Ebene, Achse, Ursprung oder eine Kurve einer früheren Skizze als feste 3D-Geometrie
    /// in Raumlage in die geöffnete Skizze, verknüpft (lila) mit ihrer Herkunft
    /// (`CopySource::Include`); mehrere Stücke bilden eine Gruppe. Gerechnet wird mit
    /// [`planora_feature::included_pieces`], also genau wie bei der Neuberechnung. Geht auch ohne
    /// den Haken „3D-Skizze“. Ein Rückgängig-Schritt; Fehler stehen auch in der Meldung der
    /// Sitzung, und die Skizze bleibt dann unverändert.
    pub fn include_into_sketch(&mut self, pick: IncludePick) -> Result<(), String> {
        let r = self.try_include(pick);
        self.report_to_session(r).map(|_| ())
    }

    fn try_include(&mut self, pick: IncludePick) -> Result<usize, Msg> {
        let r = self.include_ref(pick)?;
        let (idx, frame) = self.open_sketch_place()?;
        let pieces = planora_feature::included_pieces(&self.document, idx, &frame, &r)
            .filter(|p| !p.is_empty())
            .ok_or_else(|| crate::msg!("sketch-include-failed"))?;
        self.place_included(
            &pieces,
            CopySource::Include(r),
            crate::msg!("undo-include-3d"),
        )
    }

    /// Der dauerhafte Verweis zur Wahl `pick` (Körper vor der offenen Skizze, Ebenen und Skizzen
    /// davor).
    pub(crate) fn include_ref(
        &self,
        pick: IncludePick,
    ) -> Result<planora_feature::IncludeRef, Msg> {
        use planora_feature::{EdgeRef, FaceRef, IncludeRef, PlaneId, VertexRef};
        let (idx, _) = self.open_sketch_place()?;
        let tol = self.kernel.tolerance;
        let edge_ref = |solid: &Solid, e: Id<Edge>| {
            EdgeRef::from_edge(solid, e, &tol)
                .map_err(|err| crate::msg!("state-edge-copy-failed", error = err.to_string()))
        };
        Ok(match pick {
            IncludePick::Edge(b, e) => {
                IncludeRef::Edge(edge_ref(&self.solid_before_sketch(b)?, e)?)
            }
            IncludePick::Face(b, f) => {
                let solid = self.solid_before_sketch(b)?;
                IncludeRef::FaceBoundary(
                    FaceRef::from_face(&solid, f, &tol).map_err(|e| {
                        crate::msg!("state-face-link-failed", error = e.to_string())
                    })?,
                )
            }
            IncludePick::Vertex { body, edge, end } => IncludeRef::Vertex(VertexRef {
                edge: edge_ref(&self.solid_before_sketch(body)?, edge)?,
                end,
            }),
            IncludePick::Plane(p) => {
                if let PlaneId::Feature(f) = p
                    && !self.document.index_of(f).is_some_and(|i| i < idx)
                {
                    return Err(crate::msg!("sketch-include-plane-later"));
                }
                IncludeRef::Plane(p)
            }
            IncludePick::Axis(a) => IncludeRef::Axis(a),
            // Konstruktionspunkte als Features gibt es noch nicht (Platzhalter aus Task 15).
            IncludePick::ConstructionPoint(_) => {
                return Err(crate::msg!("sketch-include-construction-point"));
            }
            IncludePick::Origin => IncludeRef::Origin,
            IncludePick::SketchCurve { sketch, curve } => {
                self.check_source_sketch(sketch)?;
                IncludeRef::SketchCurve { sketch, curve }
            }
        })
    }

    /// Fusion „Auf Fläche projizieren“ (Skizzen 10, Spec 6) bzw. „Schnittkurve“ (Spec 7) mit der
    /// Herkunft `source` ([`CopySource::OnSurface`] oder [`CopySource::Intersection`]): rechnet
    /// genau wie die Neuberechnung (Toleranz des Dokuments, Zeitgrenze
    /// [`planora_feature::surface_project::surface_budget`]) und legt die Stücke als verknüpfte
    /// 3D-Geometrie in die offene Skizze, als eine Gruppe. Ein Rückgängig-Schritt; trifft nichts
    /// oder dauert es zu lange, bleibt die Skizze unverändert, und die Meldung steht auch in der
    /// Sitzung. Liefert die Zahl der Stücke.
    pub fn surface_curves_into_sketch(&mut self, source: CopySource) -> Result<usize, String> {
        let r = self.try_surface_curves(source);
        self.report_to_session(r)
    }

    fn try_surface_curves(&mut self, source: CopySource) -> Result<usize, Msg> {
        use planora_feature::surface_project::{ProjectError, surface_budget};
        use planora_feature::{DocView, SURFACE_PROJECT_BUDGET, intersection_curve};
        let (idx, frame) = self.open_sketch_place()?;
        let view = DocView::new(&self.document, idx, frame);
        let tol = self.document.kernel().tolerance;
        let (pieces, label) = match &source {
            CopySource::OnSurface(p) => (
                planora_feature::project_to_surface(&view, p, &tol, surface_budget()),
                crate::msg!("undo-project-to-surface"),
            ),
            CopySource::Intersection(p) => (
                intersection_curve(&view, p, &tol, surface_budget()),
                crate::msg!("undo-intersection-curve"),
            ),
            _ => return Err(crate::msg!("sketch-include-failed")),
        };
        let pieces = pieces.map_err(|e| match e {
            ProjectError::NoHit => crate::msg!("feature-surface-no-hit"),
            ProjectError::TooSlow => crate::msg!(
                "feature-surface-too-slow",
                seconds = i64::try_from(SURFACE_PROJECT_BUDGET.as_secs()).unwrap_or(i64::MAX)
            ),
            ProjectError::Lost => crate::msg!("feature-surface-lost"),
        })?;
        self.place_included(&pieces, source, label)
    }

    /// Legt die Stücke `pieces` einer Quelle als feste Geometrie in die Skizze
    /// ([`SketchSession::include_pieces`]) und hängt je Stück seine Kopie an: dieselbe Herkunft
    /// `source`, Stückindex in der Reihenfolge der Stücke, Stückzahl der Gruppe. Benennt den
    /// Schritt mit `label`; liefert die Zahl der Stücke.
    fn place_included(
        &mut self,
        pieces: &[Included],
        source: CopySource,
        label: Msg,
    ) -> Result<usize, Msg> {
        self.place_groups(vec![(pieces.to_vec(), source)], true, label)
    }

    /// Wie [`AppState::place_included`] für mehrere Gruppen auf einmal (je Gruppe Stücke und
    /// Herkunft), alles in einem Schritt: Schlägt eine Gruppe fehl, bleibt die Skizze unverändert.
    /// Mit `linked == false` entstehen freie Kurven ohne Fixierung und ohne Eintrag in `copies`.
    fn place_groups(
        &mut self,
        groups: Vec<(Vec<Included>, CopySource)>,
        linked: bool,
        label: Msg,
    ) -> Result<usize, Msg> {
        let (Some(se), Some(draft)) = (self.sketch_session.as_mut(), self.sketch_draft.as_mut())
        else {
            return Err(crate::msg!("state-no-open-sketch"));
        };
        let all: Vec<Included> = groups.iter().flat_map(|(p, _)| p.iter().cloned()).collect();
        let targets = match se.include_pieces_as(&mut draft.sketch, &all, linked) {
            Ok(t) => t,
            Err(e) => return Err(se.message.clone().unwrap_or(Msg::Raw(e))),
        };
        if linked {
            let mut from = 0;
            for (pieces, source) in &groups {
                let part = &targets[from..from + pieces.len()];
                from += pieces.len();
                let of = u32::try_from(part.len()).unwrap_or(u32::MAX);
                for (k, target) in part.iter().enumerate() {
                    draft.copies.push(EdgeCopy {
                        target: *target,
                        source: source.clone(),
                        piece: u32::try_from(k).unwrap_or(u32::MAX),
                        of,
                    });
                }
            }
        }
        se.edit_label = Some(label);
        Ok(targets.len())
    }

    /// Fusion „Drehprofil“ (Skizzen 11b, Spec 5): rechnet mit
    /// [`planora_feature::spun_profile`] (wie die Neuberechnung) und legt die Kurven in die
    /// offene Skizze. Mit `linked` sind sie violett verknüpft (`CopySource::SpunProfile`, eine
    /// Gruppe), sonst freie Kurven ohne Fixierung und ohne Eintrag in `copies`. Mit
    /// `project_axis` kommt die Achse dazu: eine Kante als `Include(Edge)`, eine Ursprungsachse
    /// als `Include(Axis)`, eine Linie einer früheren Skizze als `Include(SketchCurve)`; eine
    /// Linie der eigenen Skizze liegt schon da. Ein Rückgängig-Schritt („Drehprofil“); bei einem
    /// Fehler bleibt die Skizze unverändert, die Meldung steht auch in der Sitzung.
    pub fn add_spun_profile(
        &mut self,
        spec: planora_feature::SpunSpec,
        project_axis: bool,
        linked: bool,
    ) -> Result<usize, Msg> {
        let r = self.try_spun_profile(spec, project_axis, linked);
        if let Err(m) = &r
            && let Some(se) = self.sketch_session.as_mut()
        {
            se.message = Some(m.clone());
        }
        r
    }

    fn try_spun_profile(
        &mut self,
        spec: planora_feature::SpunSpec,
        project_axis: bool,
        linked: bool,
    ) -> Result<usize, Msg> {
        let (idx, frame) = self.open_sketch_place()?;
        let pieces = self.spun_pieces(&spec)?;
        let mut groups = vec![(pieces, CopySource::SpunProfile(spec.clone()))];
        if project_axis && let Some(r) = spun_axis_include(&spec.axis) {
            let axis = planora_feature::included_pieces(&self.document, idx, &frame, &r)
                .filter(|p| !p.is_empty())
                .ok_or_else(|| crate::msg!("sketch-include-failed"))?;
            groups.push((axis, CopySource::Include(r)));
        }
        self.place_groups(groups, linked, crate::msg!("undo-spun-profile"))
    }

    /// Die Kurvenstücke des Drehprofils `spec` in der offenen Skizze, gerechnet wie die
    /// Neuberechnung; die Vorschau des Dialogs nutzt dieselbe Rechnung. Ein Fehler der Rechnung
    /// ist die Meldung in der aktuellen Sprache.
    pub fn spun_pieces(&self, spec: &planora_feature::SpunSpec) -> Result<Vec<Included>, Msg> {
        use planora_feature::{DocView, SpunAxis, SpunError, spun_profile};
        let (idx, frame) = self.open_sketch_place()?;
        let own = match &spec.axis {
            SpunAxis::Own(c) => Some(
                self.session_sketch()
                    .and_then(|sk| crate::spun_profile_dialog::own_axis_of(sk, *c))
                    .ok_or_else(|| crate::msg!("sketch-spun-lost"))?,
            ),
            SpunAxis::Ref(_) => None,
        };
        let view = DocView::new(&self.document, idx, frame);
        let tol = self.document.kernel().tolerance;
        spun_profile(&view, spec, own, &tol).map_err(|e| match e {
            SpunError::AxisNotInPlane => crate::msg!("sketch-spun-axis-off-plane"),
            SpunError::NotRevolved => crate::msg!("sketch-spun-not-revolved"),
            SpunError::Lost => crate::msg!("sketch-spun-lost"),
            SpunError::Nothing => crate::msg!("sketch-spun-nothing"),
        })
    }

    /// Fusion „Isoparametrische Kurve“ (Skizzen 11b, Spec 6): je Paar (Richtung, Anteil) aus
    /// `pairs` die Kurven `u = const` bzw. `v = const` der Fläche `face`, gerechnet mit
    /// [`planora_feature::isoparametric::iso_curves`] (wie die Neuberechnung). Mit `linked` ist
    /// jedes Paar eine violette Gruppe (`CopySource::Isoparametric`), sonst freie Kurven (ohne
    /// Fixierung, nur feste Splines behalten ihre Enden, ohne Eintrag in `copies`). Die Skizze
    /// wird zur 3D-Skizze. Ein Rückgängig-Schritt („Isoparametrische Kurve“); ein Paar ohne Stück
    /// fällt weg, ergibt keines ein Stück, ist es ein Fehler. Bei einem Fehler bleibt die Skizze
    /// unverändert, die Meldung steht auch in der Sitzung.
    pub fn add_isoparametric(
        &mut self,
        face: planora_feature::FaceRef,
        pairs: &[(planora_feature::IsoDirection, f64)],
        linked: bool,
    ) -> Result<usize, Msg> {
        let r = self.try_isoparametric(face, pairs, linked);
        if let Err(m) = &r
            && let Some(se) = self.sketch_session.as_mut()
        {
            se.message = Some(m.clone());
        }
        r
    }

    fn try_isoparametric(
        &mut self,
        face: planora_feature::FaceRef,
        pairs: &[(planora_feature::IsoDirection, f64)],
        linked: bool,
    ) -> Result<usize, Msg> {
        let mut groups = Vec::new();
        for &(direction, value) in pairs {
            let pieces = self.iso_pieces(&face, direction, value)?;
            if !pieces.is_empty() {
                let source = CopySource::Isoparametric {
                    face: face.clone(),
                    direction,
                    value,
                };
                groups.push((pieces, source));
            }
        }
        if groups.is_empty() {
            return Err(crate::msg!("iso-no-hit"));
        }
        let n = self.place_groups(groups, linked, crate::msg!("undo-isoparametric"))?;
        // Wie „Auf Fläche projizieren“ liegen die Kurven im Raum: Die Skizze wird zur
        // 3D-Skizze, im selben Schritt.
        if let Some(draft) = self.sketch_draft.as_mut() {
            draft.sketch.set_three_d(true);
        }
        Ok(n)
    }

    /// Die Isokurven der Fläche `face` in der Richtung `dir` beim Anteil `value` in der offenen
    /// Skizze, gerechnet wie die Neuberechnung; die Vorschau des Dialogs nutzt dieselbe Rechnung.
    /// Trifft der Wert nichts, ist das Ergebnis leer; verlorene Fläche und Zeitgrenze sind
    /// Fehler (Meldung in der aktuellen Sprache).
    pub fn iso_pieces(
        &self,
        face: &planora_feature::FaceRef,
        dir: planora_feature::IsoDirection,
        value: f64,
    ) -> Result<Vec<Included>, Msg> {
        use planora_feature::surface_project::{ProjectError, surface_budget};
        use planora_feature::{DocView, SURFACE_PROJECT_BUDGET};
        let (idx, frame) = self.open_sketch_place()?;
        let view = DocView::new(&self.document, idx, frame);
        let tol = self.document.kernel().tolerance;
        match planora_feature::isoparametric::iso_curves(
            &view,
            face,
            dir,
            value,
            &tol,
            surface_budget(),
        ) {
            Ok(p) => Ok(p),
            Err(ProjectError::NoHit) => Ok(Vec::new()),
            Err(ProjectError::Lost) => Err(crate::msg!("iso-lost")),
            Err(ProjectError::TooSlow) => Err(crate::msg!(
                "feature-surface-too-slow",
                seconds = i64::try_from(SURFACE_PROJECT_BUDGET.as_secs()).unwrap_or(i64::MAX)
            )),
        }
    }

    /// Gemeinsamer Abschluss von Projizieren, Schneiden und Silhouette: legt die Kurvenstücke
    /// `pieces` über [`SketchSession::project_pieces`] in die Skizze (ohne Punkte und Doppelte),
    /// hängt je neue Kurve ihre Herkunft aus `sources` an (gleicher Index; `None`: ohne
    /// Verknüpfung) und benennt den Schritt mit `label`. Übersprungene Stücke stehen als Hinweis
    /// in der Meldung (`skipped_other` kommt zu den anderen der Sitzung dazu); ohne neue Kurve
    /// ist das Ergebnis ein Fehler (`nothing`, wenn nichts übersprungen ist).
    fn place_projection(
        &mut self,
        pieces: &[(planora_kernel::Curve, f64, f64)],
        sources: &[Option<CopySource>],
        single: bool,
        mut skipped_other: usize,
        nothing: Msg,
        label: Msg,
    ) -> Result<usize, Msg> {
        let (Some(se), Some(draft)) = (self.sketch_session.as_mut(), self.sketch_draft.as_mut())
        else {
            return Err(crate::msg!("state-no-open-sketch"));
        };
        let known: Vec<CurveId> = draft.copies.iter().filter_map(|c| c.curve()).collect();
        // Fehler der Sitzung stehen schon als Grund in ihrer Meldung.
        let outcome = match se.project_pieces(&mut draft.sketch, pieces, &known, single) {
            Ok(o) => o,
            Err(e) => return Err(se.message.clone().unwrap_or(Msg::Raw(e))),
        };
        skipped_other += outcome.skipped_other;
        let skipped_freeform = outcome.skipped_freeform;
        let mut notes = Vec::new();
        if skipped_freeform > 0 {
            notes.push(crate::msg!(
                "sketch-project-skipped",
                count = skipped_freeform
            ));
        }
        if skipped_other > 0 {
            notes.push(crate::msg!(
                "sketch-project-skipped-other",
                count = skipped_other
            ));
        }
        if outcome.added.is_empty() {
            return Err(match notes.len() {
                0 => nothing,
                1 => notes.remove(0),
                _ => Msg::Join(notes),
            });
        }
        for &(i, curve) in &outcome.added {
            if let Some(source) = sources[i].clone() {
                draft.copies.push(EdgeCopy::new(curve, source));
            }
        }
        for note in notes {
            se.add_note(note);
        }
        se.edit_label = Some(label);
        Ok(outcome.added.len())
    }

    /// Wunsch aus dem Kontextmenü des Browsers, „Skizze extrudieren/drehen“ zu öffnen: beendet
    /// die offene Skizze, wählt die Skizze im Browser und hebt die Auswahl von Körpern und in
    /// der 3D-Ansicht auf (wie ein einfacher Klick auf die Skizze).
    pub fn request_sketch_dialog(&mut self, request: SketchRequest) {
        let (SketchRequest::Extrude(id) | SketchRequest::Revolve(id)) = request;
        // Wird der Entwurf abgelehnt, bleibt die Skizze offen und es öffnet sich kein Dialog.
        if self.try_close_sketch().is_err() {
            return;
        }
        self.selected_feature = Some(id);
        self.selected_body = None;
        self.set_selection(None);
        self.sketch_request = Some(request);
    }

    /// Prüft, dass `sketch` eine berechnete Skizze ist und `sel` (Indizes in `profiles()`)
    /// mindestens ein vorhandenes Profil wählt; liefert den Namen des Skizzen-Features und die
    /// Verweise der gewählten Profile (ohne Wiederholung).
    fn check_profiles(
        &self,
        sketch: FeatureId,
        sel: &[usize],
    ) -> Result<(String, Vec<ProfileRef>), String> {
        let f = self
            .document
            .feature(sketch)
            .filter(|f| matches!(f.kind, FeatureKind::Sketch(_)))
            .ok_or_else(|| tr!("state-sketch-not-found"))?;
        let sk = self.document.sketch_of(sketch).ok_or_else(|| {
            tr!(
                "state-sketch-not-computed",
                name = &f.name,
                reason = status_message(Some(f.status.clone()))
            )
        })?;
        if sel.is_empty() {
            return Err(tr!("create-pick-profile"));
        }
        let extracted = self.profile_cache.get(sk);
        let mut refs = Vec::with_capacity(sel.len());
        for (k, &i) in sel.iter().enumerate() {
            let r = extracted
                .refs
                .get(i)
                .ok_or_else(|| tr!("state-profile-not-found"))?;
            if !sel[..k].contains(&i) {
                refs.push(r.clone());
            }
        }
        Ok((f.name.clone(), refs))
    }

    /// Auswahl der Profile `sel` (Indizes in `profiles()`) der berechneten Skizze `sketch` für ein
    /// Feature: Verweise auf die Flächen, damit das Feature bei Skizzenänderungen an denselben
    /// Flächen bleibt (gespeichert als alte Profilnummer, wenn das geht). Unbekannte Indizes
    /// entfallen; ohne berechnete Skizze ist die Wahl leer.
    pub(crate) fn profiles_selection(&self, sketch: FeatureId, sel: &[usize]) -> ProfileSel {
        let refs = self
            .document
            .sketch_of(sketch)
            .map(|sk| {
                let extracted = self.profile_cache.get(sk);
                sel.iter()
                    .filter_map(|&i| extracted.refs.get(i).cloned())
                    .collect()
            })
            .unwrap_or_default();
        ProfileSel::Refs(refs)
    }

    /// Vorgabe der Profilwahl für die Skizze `sketch` wie in Fusion: Hat sie genau ein Profil,
    /// ist es gewählt, sonst keins.
    pub fn default_profiles(&self, sketch: FeatureId) -> Vec<usize> {
        match self.document.sketch_of(sketch) {
            Some(sk) if self.profile_cache.get(sk).profiles.len() == 1 => vec![0],
            _ => Vec::new(),
        }
    }

    /// Indizes (in `profiles()` der berechneten Skizze `sketch`) der Profile, die `sel` heute
    /// wählt; leer, wenn die Skizze nicht berechnet ist oder kein Profil (mehr) passt.
    pub(crate) fn selected_profile_indices(
        &self,
        sketch: FeatureId,
        sel: &ProfileSel,
    ) -> Vec<usize> {
        let Some(sk) = self.document.sketch_of(sketch) else {
            return Vec::new();
        };
        let refs = match sel {
            ProfileSel::Refs(r) => r.clone(),
            ProfileSel::Index(n) => match sk.migrate_profile_index(*n) {
                Some(r) => r,
                None => return Vec::new(),
            },
        };
        self.profile_cache
            .get(sk)
            .resolve(&refs)
            .unwrap_or_default()
    }

    /// Legt ein Körper-Feature aus einer Skizze an (Name ohne Zähler). Lässt es sich nicht
    /// berechnen, wird es wieder entfernt; sonst wird die Körperliste aktualisiert.
    fn add_body_from_sketch(
        &mut self,
        name: String,
        kind: FeatureKind,
        formulas: Formulas,
    ) -> Result<FeatureId, String> {
        let id = self.add_checked_named(name, kind, formulas)?;
        self.refresh();
        Ok(id)
    }

    /// Extrudiert Profil `profile` des Skizzen-Features `sketch` um `height` entlang der
    /// Skizzennormalen (Kurzform von [`AppState::extrude_profiles`] für ein Profil).
    pub fn extrude_sketch(
        &mut self,
        sketch: FeatureId,
        profile: usize,
        height: f64,
    ) -> Result<FeatureId, String> {
        self.extrude_profiles(sketch, &[profile], height, Formulas::new())
    }

    /// Wie [`AppState::extrude_sketch`], mit Formeln (Höhe).
    pub fn extrude_sketch_with(
        &mut self,
        sketch: FeatureId,
        profile: usize,
        height: f64,
        formulas: Formulas,
    ) -> Result<FeatureId, String> {
        self.extrude_profiles(sketch, &[profile], height, formulas)
    }

    /// Extrudiert die Profile `sel` (Indizes in `profiles()`) des Skizzen-Features `sketch` um
    /// `height` entlang der Skizzennormalen; der Körper entsteht an Ort und Stelle.
    /// Nebeneinanderliegende Profile werden ein Körper, getrennte mehrere (wie Fusion).
    /// `formulas` sind die Formeln der Felder (Höhe). Eine offene Skizze wird vorher beendet.
    /// Bei einem Fehler bleibt das Dokument unverändert.
    pub fn extrude_profiles(
        &mut self,
        sketch: FeatureId,
        sel: &[usize],
        height: f64,
        formulas: Formulas,
    ) -> Result<FeatureId, String> {
        self.try_close_sketch()?;
        if !(height.is_finite() && height > self.kernel.tolerance.linear) {
            return self.fail(tr!("state-height-positive"));
        }
        let (name, refs) = match self.check_profiles(sketch, sel) {
            Ok(r) => r,
            Err(e) => return self.fail(e),
        };
        self.add_body_from_sketch(
            tr!("state-extrusion-name", name = name),
            FeatureKind::Extrude {
                sketch,
                profiles: ProfileSel::Refs(refs),
                height,
            },
            formulas,
        )
    }

    /// Dreht Profil `profile` des Skizzen-Features `sketch` um die Linie `axis` (Kurzform von
    /// [`AppState::revolve_profiles`] für ein Profil).
    pub fn revolve_sketch(
        &mut self,
        sketch: FeatureId,
        profile: usize,
        axis: CurveId,
    ) -> Result<FeatureId, String> {
        self.revolve_profiles(sketch, &[profile], axis)
    }

    /// Dreht die Profile `sel` (Indizes in `profiles()`) des Skizzen-Features `sketch` um die
    /// Linie `axis` (360°); der Körper entsteht an Ort und Stelle. Eine offene Skizze wird
    /// vorher beendet. Bei einem Fehler bleibt das Dokument unverändert.
    pub fn revolve_profiles(
        &mut self,
        sketch: FeatureId,
        sel: &[usize],
        axis: CurveId,
    ) -> Result<FeatureId, String> {
        self.try_close_sketch()?;
        let (name, refs) = match self.check_profiles(sketch, sel) {
            Ok(r) => r,
            Err(e) => return self.fail(e),
        };
        self.add_body_from_sketch(
            tr!("state-revolve-name", name = name),
            FeatureKind::Revolve {
                sketch,
                profiles: ProfileSel::Refs(refs),
                axis,
            },
            Formulas::new(),
        )
    }
}

fn bounds_of(mesh: &Mesh) -> (DVec3, DVec3) {
    mesh.positions.iter().fold(
        (DVec3::splat(f64::MAX), DVec3::splat(f64::MIN)),
        |(lo, hi), p| (lo.min(*p), hi.max(*p)),
    )
}

/// Meldung zu einem Feature, das nicht berechnet werden konnte.
fn status_message(status: Option<FeatureStatus>) -> String {
    match status {
        Some(FeatureStatus::Error(e) | FeatureStatus::Skipped(e)) => e,
        _ => tr!("state-feature-failed"),
    }
}

/// Art eines Körpers aus dem Feature, das ihn erzeugt hat; Verschieben übernimmt die Art des
/// verschobenen Körpers.
fn body_kind(doc: &Document, r: BodyRef) -> BodyKind {
    match doc.feature(doc.naming_origin(r).feature).map(|f| &f.kind) {
        Some(FeatureKind::Box { .. }) => BodyKind::Box,
        Some(FeatureKind::Cylinder { .. }) => BodyKind::Cylinder,
        Some(FeatureKind::Sphere { .. }) => BodyKind::Sphere,
        Some(FeatureKind::Cone { .. }) => BodyKind::Cone,
        Some(FeatureKind::Extrude { .. }) => BodyKind::Extrusion,
        Some(FeatureKind::Revolve { .. }) => BodyKind::Revolution,
        Some(FeatureKind::MeshBody { .. }) => BodyKind::Mesh,
        Some(FeatureKind::StepBody { .. }) => BodyKind::Step,
        _ => BodyKind::Combined,
    }
}

// ---- Netz-Körper ------------------------------------------------------------------------------

/// Bezeichnung (und Namensstamm) eingelesener Netze. Der Name wird mit dem Feature gespeichert
/// und bleibt in der Sprache, in der er entstand.
fn mesh_label() -> String {
    tr!("state-mesh-label")
}

/// Hinweis an ausgegrauten Menüpunkten bei gewähltem Netz-Körper (Spec §6).
pub fn mesh_locked_hint() -> String {
    tr!("state-mesh-locked")
}

/// Der Verweis, mit dem „Achse projizieren“ (Drehprofil) die Achse in die Skizze einzieht; `None`
/// bei einer Linie der eigenen Skizze (sie liegt schon da).
fn spun_axis_include(axis: &planora_feature::SpunAxis) -> Option<planora_feature::IncludeRef> {
    use planora_feature::planes::AxisRef;
    use planora_feature::{IncludeRef, SpunAxis};
    match axis {
        SpunAxis::Own(_) => None,
        SpunAxis::Ref(AxisRef::Standard(a)) => Some(IncludeRef::Axis(*a)),
        SpunAxis::Ref(AxisRef::Edge(e)) => Some(IncludeRef::Edge(e.clone())),
        SpunAxis::Ref(AxisRef::SketchLine { sketch, curve }) => Some(IncludeRef::SketchCurve {
            sketch: *sketch,
            curve: *curve,
        }),
    }
}

/// Lässt sich die Herkunft `source` mit „Neu verknüpfen“ in der Ansicht neu wählen? Kante,
/// Schnitt, Silhouette und Skizzenkurve über die Auswahl von „Projizieren“ (Ruling F9),
/// eingezogene 3D-Geometrie (Kante, Flächenrand, Ecke, Ebene, Achse, Ursprung, Skizzenkurve)
/// über die Auswahl von „3D-Geometrie einbeziehen“ (Ruling T18-1). Nicht: Konstruktionspunkte
/// (gibt es noch nicht als Features), „Auf Fläche projizieren“, die Schnittkurve und das
/// Drehprofil (mehrteilige Eingaben ihres Dialogs; Abweichung in offene-punkte). Die Isokurve
/// (Skizzen 11b) wählt ihre Fläche neu, Richtung und Anteil bleiben.
pub fn relink_pickable(source: &CopySource) -> bool {
    use planora_feature::IncludeRef;
    match source {
        CopySource::Edge(_)
        | CopySource::Section { .. }
        | CopySource::Silhouette { .. }
        | CopySource::SketchCurve { .. }
        | CopySource::Isoparametric { .. } => true,
        CopySource::Include(r) => !matches!(r, IncludeRef::ConstructionPoint(_)),
        CopySource::OnSurface(_) | CopySource::Intersection(_) | CopySource::SpunProfile(_) => {
            false
        }
    }
}

/// Wählt „Neu verknüpfen“ für die Herkunft `family` über die Auswahl von „3D-Geometrie
/// einbeziehen“ (Ecken, Ebenen, Achsen, Ursprung; Ruling T18-1)? Sonst über die von
/// „Projizieren“.
pub fn relink_picks_like_include(family: &CopySource) -> bool {
    matches!(family, CopySource::Include(_))
}

/// Der Körper, den ein Feature fortführt (sein Ergebnis ist für den Nutzer „derselbe Körper“):
/// Verschieben, Verrunden/Fase, bei Booleschen Operationen das Ziel.
fn carried_body(kind: &FeatureKind) -> Option<BodyRef> {
    match kind {
        FeatureKind::Move { body, .. } | FeatureKind::Verrunden { body, .. } => Some(*body),
        FeatureKind::Boolean { target, .. } => Some(*target),
        _ => None,
    }
}

/// Schreibt die Lage `t` in einen Netz-Körper.
fn set_transform(kind: &mut FeatureKind, t: MeshTransform) {
    if let FeatureKind::MeshBody {
        offset,
        rotation,
        scale,
        ..
    } = kind
    {
        (*offset, *rotation, *scale) = (t.offset, t.rotation, t.scale);
    }
}

impl AppState {
    /// Ist der Körper `index` ein Netz-Körper?
    pub fn is_mesh(&self, index: usize) -> bool {
        self.bodies
            .get(index)
            .is_some_and(|b| b.kind == BodyKind::Mesh)
    }

    /// Ist der gewählte Körper ein Netz-Körper?
    pub fn selected_is_mesh(&self) -> bool {
        self.selected_body.is_some_and(|i| self.is_mesh(i))
    }

    /// Ist der sichtbare Körper `index` zurückgesetzt? Dann [`crate::history_bar::lock_hint`]
    /// (neue Aktionen an ihm sind gesperrt, siehe [`crate::history_bar::lock_reason`]).
    pub fn body_lock(&self, index: usize) -> Option<String> {
        let b = self.bodies.get(index)?;
        crate::history_bar::lock_reason(self, &[b.source.feature])
    }

    /// Sperre des Körpers, um den es bei Menü und Befehlsfeld geht: der gewählte, sonst der
    /// Körper der gewählten Fläche oder Kante.
    pub(crate) fn chosen_lock(&self) -> Option<String> {
        self.chosen_body().and_then(|i| self.body_lock(i))
    }

    /// Der Körper, um den es bei Menü, Befehlsfeld und Tasten geht (Index in `bodies`): der
    /// gewählte, sonst der Körper der gewählten Fläche oder Kante.
    pub fn chosen_body(&self) -> Option<usize> {
        self.selected_body.or(self.selection.and_then(pick_body))
    }

    /// Warum „Vereinigen“, „Abziehen“ und „Schneiden“ gerade nicht gehen (`None`: sie gehen).
    pub fn boolean_blocked(&self) -> Option<String> {
        if self.selected_is_mesh() {
            Some(mesh_locked_hint())
        } else if self.solid_body_count() < 2 {
            Some(tr!("state-need-two-bodies"))
        } else {
            self.chosen_lock()
        }
    }

    /// Warum „Verrunden“ und „Fase“ gerade nicht gehen (`None`: sie gehen).
    pub fn blend_blocked(&self) -> Option<String> {
        if self.selected_is_mesh() {
            Some(mesh_locked_hint())
        } else if self.solid_body_count() == 0 {
            Some(tr!("state-need-one-body"))
        } else {
            self.chosen_lock()
        }
    }

    /// Warum „Skizze auf gewählter Fläche“ gerade nicht geht (`None`: es geht). Eine gewählte
    /// Konstruktionsebene zählt wie eine ebene Fläche (Skizzen 6).
    pub fn sketch_on_face_blocked(&self) -> Option<String> {
        if self.selected_origin_plane().is_some() {
            return None;
        }
        if let Some(id) = self.selected_plane() {
            return crate::history_bar::lock_reason(self, &[id]);
        }
        if self.selected_is_mesh() {
            return Some(mesh_locked_hint());
        }
        let planar = match self.selection {
            Some(Pick::Face(FaceRef { body, face })) => self
                .bodies
                .get(body)
                .is_some_and(|b| plane_of_face(&b.solid, face).is_some())
                .then_some(body),
            _ => None,
        };
        if let Some(body) = planar {
            self.body_lock(body)
        } else {
            Some(tr!("state-pick-planar-face"))
        }
    }

    /// Anzahl der sichtbaren Körper, die keine Netz-Körper sind.
    pub fn solid_body_count(&self) -> usize {
        self.bodies
            .iter()
            .filter(|b| b.kind != BodyKind::Mesh)
            .count()
    }

    /// Lage des Netz-Körpers `index`; `None` für andere Körper.
    pub fn mesh_transform(&self, index: usize) -> Option<MeshTransform> {
        let b = self
            .bodies
            .get(index)
            .filter(|b| b.kind == BodyKind::Mesh)?;
        self.document.mesh_body(b.source.feature).map(|(_, t)| t)
    }

    /// Legt ein eingelesenes Netz als Netz-Körper an (Lage: Ursprung, Maßstab `unit_scale` =
    /// Millimeter je Einheit der Datei) und wählt ihn. Ein leerer Name ergibt „Netz-Körper N“.
    /// Schlägt es fehl, bleibt das Dokument unverändert (Spec §5).
    pub fn import_mesh(
        &mut self,
        name: &str,
        mesh: Arc<ExchangeMesh>,
        unit_scale: f64,
    ) -> Result<FeatureId, String> {
        self.try_close_sketch()?;
        let trimmed = name.trim();
        let label = mesh_label();
        let (name, counted) = if trimmed.is_empty() {
            (self.next_name(&label), true)
        } else {
            (trimmed.to_string(), false)
        };
        let t = MeshTransform {
            offset: DVec3::ZERO,
            rotation: DQuat::IDENTITY,
            scale: unit_scale,
        };
        match self.document.add_mesh_body(name, mesh, t) {
            Ok(id) => {
                if counted {
                    self.count_name(&label);
                }
                self.hover = None;
                self.selection = None;
                self.selected_feature = None;
                self.refresh();
                self.selected_body = self.body_index(BodyRef {
                    feature: id,
                    index: 0,
                });
                Ok(id)
            }
            Err(e) => self.fail(e.to_string()),
        }
    }

    /// Setzt die Lage des Netz-Körpers `index`. Lässt er sich damit nicht berechnen, wird die
    /// alte Lage zurückgeschrieben und die Meldung steht in `message`.
    pub fn set_mesh_transform(&mut self, index: usize, t: MeshTransform) -> Result<(), String> {
        let Some(r) = self
            .bodies
            .get(index)
            .filter(|b| b.kind == BodyKind::Mesh)
            .map(|b| b.source)
        else {
            return self.fail(tr!("state-not-mesh"));
        };
        self.set_mesh_transform_of(r, t)
    }

    fn move_mesh_body(&mut self, r: BodyRef, delta: DVec3) -> Result<(), String> {
        let Some(mut t) = self.document.mesh_body(r.feature).map(|(_, t)| t) else {
            return self.fail(tr!("state-body-not-found"));
        };
        t.offset += delta;
        self.set_mesh_transform_of(r, t)
    }

    /// Setzt die Lage `t` (P2: ändert sich ein Feld mit Formel, entfällt nur diese Formel).
    fn set_mesh_transform_of(&mut self, r: BodyRef, t: MeshTransform) -> Result<(), String> {
        self.try_close_sketch()?;
        let Some(old) = self.document.mesh_body(r.feature).map(|(_, t)| t) else {
            return self.fail(tr!("state-body-not-found"));
        };
        if old == t {
            return Ok(());
        }
        self.update_mesh_body(r, |k| set_transform(k, t), None)
    }

    /// Formelfeld des Netz-Körpers `index` (Position x/y/z, Maßstab; P10): Eine Zahl setzt den
    /// Wert, die Formel dieses Feldes entfällt; sonst wird der Text die Formel des Feldes und
    /// bestimmt den Wert. Eine kaputte Formel oder ein Wert, mit dem sich der Körper nicht
    /// berechnen lässt, wird abgelehnt; Lage und Formeln bleiben dann, wie sie waren.
    pub fn set_mesh_formula(
        &mut self,
        index: usize,
        key: FieldKey,
        text: &str,
    ) -> Result<(), String> {
        let Some(r) = self
            .bodies
            .get(index)
            .filter(|b| b.kind == BodyKind::Mesh)
            .map(|b| b.source)
        else {
            return self.fail(tr!("state-not-mesh"));
        };
        let text = text.trim();
        if text.is_empty() {
            return self.fail(crate::formula_field::empty());
        }
        self.try_close_sketch()?;
        let Some(f) = self.document.feature(r.feature) else {
            return self.fail(tr!("state-body-not-found"));
        };
        if !planora_feature::field_keys(&f.kind).contains(&key) {
            return self
                .fail(FeatureError::InvalidInput(tr!("state-field-no-formula")).to_string());
        }
        let mut formulas = f.formulas().clone();
        let number = planora_feature::expr::plain_number(text);
        match number {
            Some(v) => {
                let same = planora_feature::field_value(&f.kind, key).map(f64::to_bits)
                    == Some(v.to_bits());
                if same && !formulas.contains_key(&key) {
                    return Ok(());
                }
                formulas.remove(&key);
            }
            None => {
                if formulas.get(&key).map(String::as_str) == Some(text) {
                    return Ok(());
                }
                formulas.insert(key, text.to_string());
            }
        }
        self.update_mesh_body(
            r,
            |k| {
                if let Some(v) = number {
                    planora_feature::set_field_value(k, key, v);
                }
            },
            Some(formulas),
        )
    }

    /// Ändert den Netz-Körper `r` (mit `formulas`: ganze Formeltabelle, sonst P2). Lässt er sich
    /// danach nicht berechnen, werden Lage **und Formeln** zurückgeschrieben (nie still eine
    /// Formel verlieren) und die Meldung steht in `message`.
    fn update_mesh_body(
        &mut self,
        r: BodyRef,
        f: impl FnOnce(&mut FeatureKind),
        formulas: Option<Formulas>,
    ) -> Result<(), String> {
        let Some((old_kind, old_formulas)) = self
            .document
            .feature(r.feature)
            .map(|f| (f.kind.clone(), f.formulas().clone()))
        else {
            return self.fail(tr!("state-body-not-found"));
        };
        let changed = match formulas {
            Some(t) => self
                .document
                .update_with_formulas(r.feature, f, t)
                .map(|_| ()),
            None => self.document.update(r.feature, f),
        };
        if let Err(e) = changed {
            return self.fail(e.to_string());
        }
        match self.document.feature(r.feature).map(|f| f.status.clone()) {
            Some(FeatureStatus::Ok) => {
                self.refresh();
                Ok(())
            }
            status => {
                let _ =
                    self.document
                        .update_with_formulas(r.feature, |k| *k = old_kind, old_formulas);
                self.refresh();
                self.fail(status_message(status))
            }
        }
    }
}

// ---- STEP-Körper ------------------------------------------------------------------------------

/// Bezeichnung (und Namensstamm) der aus STEP-Dateien übernommenen Körper, gleich
/// `FeatureKind::StepBody.label()`. Nur in der Crate sichtbar: `export_dialog::STEP_LABEL` (Name
/// des Exportformats) trägt denselben Namen, und Tests binden beide Module mit `*` ein.
pub(crate) fn step_label() -> String {
    planora_step::unnamed_body()
}

/// Nummer `n`, wenn `name` genau „STEP-Körper n“ ist (n höchstens `MAX_NAME_COUNTER`).
fn step_number(name: &str) -> Option<u64> {
    let (label, num) = name.trim().rsplit_once(' ')?;
    let n = num.parse::<u64>().ok()?;
    (label == step_label() && n <= MAX_NAME_COUNTER).then_some(n)
}

impl AppState {
    /// Die Namen, die [`AppState::import_step`] den Körpern mit den Namen `given` (in dieser
    /// Reihenfolge) geben würde: ein Name wird getrimmt übernommen, ein leerer wird
    /// „STEP-Körper n“. Die Nummern liegen über allen „STEP-Körper k“ im Dokument (Features und
    /// sichtbare Körper, auch umbenannte), über allen schon vergebenen und über denen in `given`
    /// selbst, damit kein Name doppelt vorkommt. Das Fenster „STEP importieren“ benutzt sie für
    /// die Lage-Meldung (`ImportedBody::warnings_named`), damit Meldung und Baum übereinstimmen.
    pub fn step_body_names(&self, given: &[String]) -> Vec<String> {
        self.plan_step_names(given).0
    }

    /// Wie [`AppState::step_body_names`]; dazu die letzte vergebene Nummer (keine: `None`).
    fn plan_step_names(&self, given: &[String]) -> (Vec<String>, Option<u64>) {
        let base = self
            .document
            .features()
            .iter()
            .map(|f| f.name.as_str())
            .chain(self.bodies.iter().map(|b| b.name.as_str()))
            .chain(given.iter().map(String::as_str))
            .filter_map(step_number)
            .chain(self.name_counters.get(&step_label()).copied())
            .max()
            .unwrap_or(0);
        let mut next = base;
        let mut last = None;
        let names = given
            .iter()
            .map(|name| {
                let trimmed = name.trim();
                if trimmed.is_empty() {
                    next = next.saturating_add(1);
                    last = Some(next);
                    format!("{} {next}", step_label())
                } else {
                    trimmed.to_string()
                }
            })
            .collect();
        (names, last)
    }

    /// Legt die Körper `bodies` (Name, Form aus dem Import) als STEP-Körper an, alle in einem
    /// Schritt am Marker (ganz oder gar nicht). Ein leerer Name ergibt „STEP-Körper n“ (Zähler wie
    /// bei „Netz-Körper n“; er zählt nur nach gelungenem Anlegen weiter). Danach ist der erste
    /// neue Körper gewählt und die Kamera auf die neuen Körper ausgerichtet. Eine offene Skizze
    /// wird vorher beendet. Schlägt es fehl, bleibt das Dokument unverändert und die Meldung
    /// steht in `message` (etwa die Summengrenze der STEP-Formen). Eine leere Liste ändert nichts.
    pub fn import_step(
        &mut self,
        bodies: Vec<(String, Arc<StepShape>)>,
    ) -> Result<Vec<FeatureId>, String> {
        self.try_close_sketch()?;
        if bodies.is_empty() {
            return Ok(Vec::new());
        }
        let given: Vec<String> = bodies.iter().map(|(n, _)| n.clone()).collect();
        let (names, last) = self.plan_step_names(&given);
        let list: Vec<NewStepBody> = bodies
            .into_iter()
            .zip(names)
            .map(|((_, shape), name)| NewStepBody { name, shape })
            .collect();
        match self.document.add_step_bodies(list) {
            Ok(ids) => {
                if let Some(n) = last {
                    self.name_counters.insert(step_label(), n);
                }
                self.hover = None;
                self.selection = None;
                self.selected_feature = None;
                self.refresh();
                let shown: Vec<usize> = ids
                    .iter()
                    .filter_map(|id| {
                        self.body_index(BodyRef {
                            feature: *id,
                            index: 0,
                        })
                    })
                    .collect();
                self.selected_body = shown.first().copied();
                self.frame_bodies(&shown);
                Ok(ids)
            }
            Err(e) => self.fail(e.to_string()),
        }
    }
}

// ---- Parameter --------------------------------------------------------------------------------

impl AppState {
    /// Neue Zeile mit Vorgaben (`ParameterN`, Formel „10“, Länge; Plan P13).
    pub fn add_parameter(&mut self) -> Result<ParamId, String> {
        self.parameter_action(Document::add_parameter_default)
    }

    /// Ändert Name, Formel, Art und Kommentar eines Parameters; die Formeln, die ihn nennen,
    /// rechnen mit, ein neuer Name wird in allen Formeln ersetzt.
    pub fn edit_parameter(&mut self, id: ParamId, edit: ParamEdit) -> Result<(), String> {
        self.parameter_action(|d| d.set_parameter(id, edit).map(|_| ()))
    }

    /// Löscht einen Parameter, den keine Formel nennt (sonst Meldung mit den Benutzern).
    pub fn delete_parameter(&mut self, id: ParamId) -> Result<(), String> {
        self.parameter_action(|d| d.remove_parameter(id))
    }

    /// „Durch den Wert ersetzen“: Formeln, die den Parameter nennen, werden durch ihren Wert
    /// ersetzt, dann wird er gelöscht (ein Schritt, kein Wert ändert sich; Plan P7).
    pub fn replace_parameter(&mut self, id: ParamId) -> Result<(), String> {
        self.parameter_action(|d| d.replace_parameter_by_value(id))
    }

    /// Gemeinsamer Ablauf der Parameter-Aktionen: Bei offener Skizze gesperrt
    /// ([`sketch_open_lock`], Dokument unverändert). Sonst die Änderung am Dokument (abgelehnt:
    /// nichts ändert sich, Meldung in `message`), danach die Körper neu; Auswahl und gewählter
    /// Körper folgen den Regeln von [`AppState::refresh`]. Scheitert dadurch ein Feature, das
    /// vorher keinen Fehler hatte, nennt `message` das erste davon („„Quader 1“: …“).
    fn parameter_action<T>(
        &mut self,
        act: impl FnOnce(&mut Document) -> Result<T, FeatureError>,
    ) -> Result<T, String> {
        if self.sketch_session.is_some() {
            return self.fail(sketch_open_lock());
        }
        let failed_before: Vec<FeatureId> = self
            .document
            .features()
            .iter()
            .filter(|f| matches!(f.status, FeatureStatus::Error(_)))
            .map(|f| f.id)
            .collect();
        let count = self.document.edit_count();
        let constraints_before = self.sketch_constraint_counts();
        let out = match act(&mut self.document) {
            Ok(out) => out,
            Err(e) => return self.fail(e.to_string()),
        };
        if self.document.edit_count() == count {
            return Ok(out);
        }
        self.texts_due = true;
        self.refresh();
        // Skizzen 5: Senkt eine Formel die Anzahl eines Musters, entfallen die Abhängigkeiten an
        // den wegfallenden Exemplaren; die Meldung nennt ihre Zahl wie im Werkzeug.
        let after = self.sketch_constraint_counts();
        let dropped: usize = constraints_before
            .iter()
            .filter_map(|(id, n)| {
                let now = after.iter().find(|(f, _)| f == id)?.1;
                Some(n.saturating_sub(now))
            })
            .sum();
        if dropped > 0 {
            self.message = Some(tr!("sketch-pattern-dropped", n = dropped));
        }
        let newly_failed = self
            .document
            .features()
            .iter()
            .find_map(|f| match &f.status {
                FeatureStatus::Error(e) if !failed_before.contains(&f.id) => {
                    Some(tr!("tree-status-line", name = &f.name, error = e))
                }
                _ => None,
            });
        if let Some(m) = newly_failed {
            self.message = Some(match self.message.take() {
                Some(note) if dropped > 0 => format!("{m} {note}"),
                _ => m,
            });
        }
        Ok(out)
    }

    /// Zahl der Abhängigkeiten (ohne interne) je Skizzen-Feature, für die Meldung nach einer
    /// Parameteränderung. Aus der Definition, nicht aus dem Ergebnis: Eine Skizze mit Fehler hat
    /// kein Ergebnis, ihre Abhängigkeiten zählen trotzdem (die Anzahl eines Musters schreibt der
    /// Formel-Durchgang in die Definition).
    fn sketch_constraint_counts(&self) -> Vec<(FeatureId, usize)> {
        self.document
            .features()
            .iter()
            .filter_map(|f| match &f.kind {
                FeatureKind::Sketch(sf) => Some((f.id, sf.sketch.constraints().count())),
                _ => None,
            })
            .collect()
    }
}

/// Die Auswahl der Sitzung als Menge für die Zwischenablage: gewählte Kurven und Punkte (was nicht
/// mehr lebt, fällt weg).
fn clip_set_of(se: &SketchSession, sk: &Sketch) -> planora_sketch::clipboard::ClipSet {
    use crate::sketch_state::Item;
    let mut set = planora_sketch::clipboard::ClipSet::default();
    for item in &se.selection {
        match *item {
            Item::Curve(c) if sk.curve(c).is_some() => set.curves.push(c),
            Item::Point(p) if sk.point_xy(p).is_ok() => set.points.push(p),
            _ => {}
        }
    }
    set
}

/// Hält etwas in der eingefügten Menge fest (fester Spline: seine Enden bleiben `Fixed`)? Dann
/// geht Verschieben/Kopieren dafür nicht.
fn paste_is_fixed(sk: &Sketch, curves: &[CurveId], points: &[PointId]) -> bool {
    use planora_sketch::transform::{Similarity, TransformBlock};
    let set = sk.transform_set(curves, points);
    let base = set
        .points
        .first()
        .and_then(|&p| sk.point_xy(p).ok())
        .unwrap_or(DVec2::ZERO);
    let probe = Similarity {
        base,
        offset: DVec2::new(1.0, 0.0),
        angle: 0.0,
        factor: 1.0,
    };
    matches!(
        sk.check_transform(&set, &probe),
        Err(TransformBlock::FixedInSet(_) | TransformBlock::BoundToFixed(_))
    )
}
