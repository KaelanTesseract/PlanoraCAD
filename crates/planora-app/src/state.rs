use crate::history::{History, sketch_bytes};
use crate::msg::Msg;
use crate::sketch_state::SketchSession;
use glam::{Vec2, Vec3};
use planora_exchange::{ExchangeMesh, MeshReport, MeshTransform, check_mesh};
use planora_feature::planes::{PlaneBase, PlaneDef};
use planora_feature::{
    BlendArt, BodyRef, CopySource, Document, EdgeCopy, FeatureError, FeatureId, FeatureKind,
    FeatureStatus, FieldKey, Formulas, MeshId, NewStepBody, ParamEdit, ParamId, ProfileSel,
    RestoreKind, SketchFeature, SketchPlane, StepShape, mesh_locked,
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
    build_scene_marked, pick_edge, pick_face, pick_plane, pick_vertex,
};
use planora_sketch::constraint::ConstraintKind;
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
    /// Profile im Skizzen-Modus als helle Flächen zeigen (Skizzenfeld „Profil anzeigen“, wie
    /// Fusions Skizzenpalette „Profil“); Vorgabe an.
    pub show_sketch_profiles: bool,
    /// Skizzenraster zeigen (Skizzenfeld, wie Fusions Skizzenpalette); Vorgabe an. Ohne Raster
    /// kein Raster-Fang. Den Stand führt [`crate::workspace::Workspace::sync_sketch_grid`].
    pub sketch_show_grid: bool,
    /// Freie Punkte am sichtbaren Raster fangen (Skizzenfeld „Fang“); Vorgabe an.
    pub sketch_snap_grid: bool,
    /// Ein Schalter wurde betätigt: Der Arbeitsbereich speichert den Stand und gibt ihn allen Tabs.
    pub sketch_grid_changed: bool,
    /// Offene Profilwahl eines Dialogs „Extrudieren“/„Drehen“: Die 3D-Ansicht zeigt die Profile
    /// der Skizze und schaltet sie per Klick um (siehe [`ProfilePick`]).
    pub profile_pick: Option<ProfilePick>,
    /// Offene Rückfrage „Löschen mit Folgen“ aus dem Feature-Baum.
    pub pending_delete: Option<crate::browser::DeletePlan>,
    /// Offenes Fenster „Umbenennen“: Feature und Entwurf des Namens.
    pub rename_draft: Option<(FeatureId, String)>,
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
/// trägt das Profil unter dem Mauszeiger (`hover`) und ein angeklicktes (`clicked`) ein, das der
/// Dialog im selben Bild umschaltet und löscht. Indizes in `profiles()` der berechneten Skizze.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProfilePick {
    pub sketch: FeatureId,
    pub selected: Vec<usize>,
    pub hover: Option<usize>,
    pub clicked: Option<usize>,
}

/// Merkt sich die Profile der zuletzt gefragten Skizze zu ihrem Stand
/// ([`Sketch::revision`], über alle Skizzen eindeutig). Ändert sich die Skizze, ändert sich der
/// Stand, und die Profile werden neu berechnet.
#[derive(Debug, Default)]
pub struct ProfileCache {
    entry: RefCell<Option<(u64, Arc<ExtractedProfiles>)>>,
    computed: Cell<usize>,
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
/// es ein eigener Schritt (ein Linienzug: je Linie einer).
fn deferred_by_tool(se: &SketchSession, before: &SketchFeature, after: &SketchFeature) -> bool {
    if se.pending.is_empty() || before.copies != after.copies {
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
    let only_snaps = a
        .constraints()
        .filter(|(id, _)| b.constraint(*id).is_none())
        .all(|(_, c)| match &c.kind {
            ConstraintKind::PointOnCurve(..) => true,
            ConstraintKind::Midpoint(p, _) => new_open(p),
            ConstraintKind::Fixed(p, x, y) => new_open(p) && *x == 0.0 && *y == 0.0,
            _ => false,
        });
    same_curves && points_kept && values_kept && constraints_kept && only_snaps
}

/// Länge des Geometrieschlüssels einer Kante (Anfangs- und Endpunkt, Parameterbereich).
const EDGE_KEY_LEN: usize = 8;

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
/// Kleinste halbe Kantenlänge einer gezeichneten Konstruktionsebene (Skizzen 6).
const PLANE_MIN_HALF_MM: f64 = 25.0;
/// Halbe Kantenlänge einer Konstruktionsebene als Anteil der Diagonale aller sichtbaren Körper.
const PLANE_HALF_PER_DIAGONAL: f64 = 0.6;

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
}

impl PickFilter {
    /// Normale Auswahl in der 3D-Ansicht: Flächen, Kanten und Ebenen, keine Eckpunkte.
    pub const SELECT: PickFilter = PickFilter {
        planes: true,
        vertices: false,
        edges: true,
        faces: true,
    };
    /// Nur Flächen und Kanten (Projizieren und Schneiden im Skizzen-Modus).
    pub const GEOMETRY: PickFilter = PickFilter {
        planes: false,
        vertices: false,
        edges: true,
        faces: true,
    };
    /// Punktwahl: Eckpunkte vor allem anderen, dann Kanten und Flächen. Ebenen zählen nicht
    /// und verdecken keine Ecken (die Punkte liegen oft hinter einer schon gezeichneten Ebene).
    pub const POINTS: PickFilter = PickFilter {
        planes: false,
        vertices: true,
        edges: true,
        faces: true,
    };
    /// Bezug eines Ebenen-Fensters: Flächen und Ebenen, keine Kanten.
    pub const BASES: PickFilter = PickFilter {
        planes: true,
        vertices: false,
        edges: false,
        faces: true,
    };
    /// Nur Flächen (Tangentialebene).
    pub const FACES: PickFilter = PickFilter {
        planes: false,
        vertices: false,
        edges: false,
        faces: true,
    };
    /// Nur Kanten (Achse der Ebene unter Winkel).
    pub const EDGES: PickFilter = PickFilter {
        planes: false,
        vertices: false,
        edges: true,
        faces: false,
    };
    /// Nur Eckpunkte („Ebene durch drei Punkte“).
    pub const VERTICES: PickFilter = PickFilter {
        planes: false,
        vertices: true,
        edges: false,
        faces: false,
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
            name_counters: BTreeMap::new(),
            selected_feature: None,
            history_choice: None,
            tree_folders: crate::browser::FolderState::default(),
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
            show_sketch_profiles: true,
            sketch_show_grid: true,
            sketch_snap_grid: true,
            sketch_grid_changed: false,
            profile_pick: None,
            pending_delete: None,
            rename_draft: None,
            pending_delete_drawing: None,
            rename_drawing_draft: None,
            edit_request: None,
            marked_edges: Vec::new(),
            edge_picking: false,
            chain_marking: true,
            plane_pick: None,
            pending_pick: None,
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
        }
    }

    /// Wählt den Körper der Verlaufsleiste (`None`: Standard, der letzte Körper). Unabhängig
    /// von `selected_body` (Index in der Körperliste für die Eigenschaften).
    pub fn select_body(&mut self, tip: Option<FeatureId>) {
        self.history_choice = tip;
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
        let folder = match self.document.feature(id).map(|f| &f.kind) {
            Some(FeatureKind::Sketch(_)) => v.folders.sketches,
            Some(FeatureKind::Plane(_)) => v.folders.planes,
            _ => false,
        };
        folder || v.features.contains(&id)
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
        let hideable = self
            .document
            .feature(id)
            .is_some_and(|f| matches!(f.kind, FeatureKind::Sketch(_) | FeatureKind::Plane(_)));
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
            self.document
                .feature(*id)
                .is_some_and(|f| matches!(f.kind, FeatureKind::Sketch(_) | FeatureKind::Plane(_)))
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
        let kind = self.document.restore(s).map_err(|e| e.to_string())?;
        self.rebuild_name_counters();
        // Eine offene Skizze verwerfen: Ihr Entwurf (samt Verlauf) gehört zum alten Dokument.
        self.sketch_draft = None;
        self.sketch_history = None;
        let left_sketch = self.sketch_session.take().is_some();
        let had_marks = !self.marked_edges.is_empty();
        self.marked_edges.clear();
        self.edge_picking = false;
        self.plane_pick = None;
        self.pending_pick = None;
        self.drag_move = None;
        self.pending_delete = None;
        self.pending_delete_drawing = None;
        self.pending_delete_dimension = None;
        self.rename_draft = None;
        self.rename_drawing_draft = None;
        self.edit_request = None;
        self.sketch_request = None;
        self.dimension_dialog = None;
        self.sheet_dialog = None;
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
        if *self.document.visibility() != visibility_before {
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
        crate::history_bar::refuse_if_locked(self, &kind.dependencies())?;
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
        crate::history_bar::refuse_if_locked(self, &kind.dependencies())?;
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
        self.document.features()[start + 1..]
            .iter()
            .any(|f| f.kind.dependencies().iter().any(|d| origins.contains(d)))
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
        let marked: Vec<Pick> = self.marked_edges.iter().map(|e| Pick::Edge(*e)).collect();
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
        self.selection == this || self.hover == this || self.selected_feature == Some(id)
    }

    /// Wie [`AppState::pick`], mit Wahl, was zählt ([`PickFilter`]). Reihenfolge: Eckpunkt
    /// (nur mit `filter.vertices`), Kante (mit `filter.edges`), dann das Vordere aus Fläche und
    /// Ebene (eine ausgeschaltete Fläche ergibt nichts, verdeckt aber weiter). Ob eine Ebene
    /// Ecken und Kanten dahinter verdeckt, hängt vom Filter ab: bei `planes: true` verdeckt sie
    /// wie eine Fläche (liegt eine Fläche näher, gewinnt diese); mit `planes: false` (etwa
    /// `PickFilter::VERTICES`) zählen Ebenen nicht und verdecken nichts.
    pub fn pick_with(&self, cursor: Vec2, viewport: Vec2, filter: PickFilter) -> Option<Pick> {
        if viewport.x < 1.0 || viewport.y < 1.0 {
            return None;
        }
        let nearest_face = self.nearest_face_hit(cursor, viewport);
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
            if let Some((body, edge, end)) = pick_vertex(
                &meshes,
                &self.camera,
                viewport,
                cursor,
                PICK_VERTEX_PX,
                occluder,
            ) {
                return Some(Pick::Vertex { body, edge, end });
            }
        }
        let mut best_edge: Option<(usize, planora_render::EdgeHit)> = None;
        for (body, b) in shown().filter(|_| filter.edges) {
            if let Some(hit) = pick_edge(
                &b.mesh,
                &self.camera,
                viewport,
                cursor,
                PICK_EDGE_PX,
                occluder,
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
    /// Konstruktionsebenen ([`AppState::plane_rect_for`]).
    pub fn origin_plane_rects(&self) -> Vec<(StandardPlane, PlaneRect)> {
        [StandardPlane::XY, StandardPlane::XZ, StandardPlane::YZ]
            .into_iter()
            .filter(|p| self.is_origin_plane_shown(*p))
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
        let base_face = match def {
            PlaneDef::Offset {
                base: PlaneBase::Face(r),
                ..
            }
            | PlaneDef::Tangent { face: r, .. } => Some(r),
            _ => None,
        };
        let anchor = base_face
            .and_then(|r| self.face_outline(r))
            .and_then(|pts| {
                let first = *pts.first()?;
                let (lo, hi) = pts
                    .iter()
                    .fold((first, first), |(lo, hi), p| (lo.min(*p), hi.max(*p)));
                Some((lo + hi) * 0.5)
            })
            .unwrap_or(DVec3::ZERO);
        // `anchor` senkrecht in die Ebene legen.
        let center = anchor - frame.z * (anchor - frame.origin).dot(frame.z);
        let diagonal = self
            .bodies
            .iter()
            .enumerate()
            .filter(|(j, _)| !self.is_body_hidden(*j))
            .map(|(_, b)| b.bounds)
            .reduce(|(lo, hi), (a, b)| (lo.min(a), hi.max(b)))
            .map_or(0.0, |(lo, hi)| (hi - lo).length());
        let half = (PLANE_HALF_PER_DIAGONAL * diagonal).max(PLANE_MIN_HALF_MM);
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
        let mut nearest_face: Option<(usize, planora_render::FaceHit)> = None;
        for (body, b) in self.bodies.iter().enumerate() {
            if self.is_body_hidden(body) {
                continue;
            }
            if let Some(hit) = pick_face(&b.mesh, origin, dir)
                && nearest_face.is_none_or(|(_, h)| hit.distance < h.distance)
            {
                nearest_face = Some((body, hit));
            }
        }
        nearest_face
    }
}

/// Was „Projizieren“ in die Skizze legt: eine Kante, eine Fläche (ihre Schleifen) oder einen
/// ganzen Körper; je mit dem Index des Körpers in [`AppState::bodies`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectTarget {
    Edge(usize, Id<Edge>),
    Face(usize, Id<Face>),
    Body(usize),
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
        Pick::Plane(_) => None,
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
        });
        self.add_checked(&label, kind)
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

    /// Beendet einen Wahlmodus ohne Änderung (Esc, Rechtsklick).
    pub fn cancel_pending_pick(&mut self) {
        self.pending_pick = None;
        self.set_hover(None);
    }

    /// Ein Klick im Wahlmodus „Skizzenebene neu festlegen“ (Ansicht oder Ordner „Ursprung“).
    /// Eine Grundebene, Konstruktionsebene oder ebene Fläche setzt die Skizze darauf und beendet
    /// den Modus, auch wenn das Dokument ablehnt. Eine gekrümmte Fläche (oder eine Fläche eines
    /// Netz-Körpers) gibt nur eine Meldung; der Modus bleibt.
    pub fn pick_sketch_plane(&mut self, click: crate::plane_dialog::PlaneClick) {
        use crate::plane_dialog::PlaneClick;
        let Some(PendingPick::SketchPlane(sketch)) = self.pending_pick else {
            return;
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

    /// Verweis auf die ebene Fläche `face` des sichtbaren Körpers `body` für eine Skizze; sonst
    /// der Grund.
    fn face_ref_for_sketch(
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
        session.last_report = Some(draft.sketch.clone().solve());
        session.text_memory = self.text_memory.clone();
        if !switching {
            self.saved_camera = Some(self.camera);
        }
        self.sketch_session = Some(session);
        // Jede geöffnete Skizze beginnt mit einem eigenen, leeren Verlauf (R10).
        self.sketch_history = Some(History::new(draft.clone(), sketch_bytes(&draft)));
        self.sketch_draft = Some(draft);
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
        let range = mesh.face_ranges.iter().find(|fr| fr.face == found.face)?;
        let first = range.first_index as usize;
        let indices = mesh
            .indices
            .get(first..first + range.index_count as usize)?;
        let pts: Vec<DVec3> = indices
            .iter()
            .filter_map(|&i| mesh.positions.get(i as usize).copied())
            .collect();
        (!pts.is_empty()).then_some(pts)
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

    /// Skizze der offenen Sitzung (der Entwurf).
    pub fn session_sketch(&self) -> Option<&Sketch> {
        self.sketch_session.as_ref()?;
        self.sketch_draft.as_ref().map(|d| &d.sketch)
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
        self.commit_draft()?;
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
        let Some((h, draft, false)) = self.sketch_open_change() else {
            // Kein Schritt: ein Schrittname eines Werkzeugs gehört zu keiner Änderung mehr.
            if self.sketch_open_change().is_none()
                && let Some(se) = self.sketch_session.as_mut()
            {
                se.edit_label = None;
            }
            return;
        };
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
    fn solid_before_sketch(&self, body: usize) -> Result<Solid, Msg> {
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
        let known: Vec<CurveId> = draft.copies.iter().map(|c| c.curve).collect();
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
                draft.copies.push(EdgeCopy { curve, source });
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
