//! Feature-Baum als Körper-Übersicht: oben das Dokument, darunter die Ordner „Körper“ (je
//! Körper eine Zeile mit Auge und einem Quadrat in der Körperfarbe der Verlaufsleiste),
//! „Skizzen“, „Zeichnungen“ und „Ursprung“; Kontextmenü je Zeile. Die einzelnen Schritte stehen
//! in der Verlaufsleiste ([`crate::history_bar`]). Die Zustandsfunktionen (Löschen mit Folgen,
//! Unterdrücken, globaler Marker) sind ohne Oberfläche testbar.

use crate::dialog::{Dialog, DialogSize};
use crate::icons::{FeatureIcon, Icon, IconSource, paint_feature_icon, paint_source};
use crate::state::{AppState, SketchRequest};
use crate::theme;
use egui::{Color32, Sense, vec2};
use planora_feature::{BodyRef, FeatureId, FeatureKind, FeatureStatus, SketchPlane};
use planora_i18n::tr;

/// Zeilenhöhe (Graphit: 24).
const ROW_HEIGHT: f32 = theme::ROW_H;
/// Einrückung der Zeilen unter einem Eintrag (Ansichten unter ihrer Zeichnung).
const BODY_INDENT: f32 = 22.0;
/// Einrückung des Inhalts eines Ordners.
const FOLDER_INDENT: f32 = 14.0;
/// Abstand zwischen Name und Status in einer Zeile.
const STATUS_GAP: f32 = 10.0;
/// Größe der Zeilensymbole.
const ICON_SIZE: f32 = 16.0;
/// Größe des Pfeils vor einem Ordner.
const CARET: f32 = 8.0;
/// Breite des Auges vor einer Körperzeile und Größe seines Symbols.
const EYE_W: f32 = 20.0;
const EYE_ICON: f32 = 14.0;
/// Kantenlänge des Farbquadrats einer Körperzeile.
const SWATCH: f32 = 10.0;
/// Rot für Fehler im dunklen Thema (Wert aus der Palette); die Oberfläche nutzt `error_color`.
pub const ERROR_COLOR: Color32 = theme::DARK.error;

/// Fehlerfarbe des Themas.
pub fn error_color(dark: bool) -> Color32 {
    theme::palette(dark).error
}

fn tree_ink_id() -> egui::Id {
    egui::Id::new("planora-browser-tree-ink")
}

/// Schriftfarben des durchsichtigen Baums, solange [`show_in_panel`] zeichnet, sonst `None`.
fn tree_ink(ui: &egui::Ui) -> Option<theme::TreeInk> {
    ui.ctx().data(|d| d.get_temp(tree_ink_id()))
}

/// Schrift- und Symbolfarbe einer Zeile: aktiv (gewählt, überfahren, Ordner) oder in Ruhe. Sonst
/// wie [`crate::icons::stroke_color`], im durchsichtigen Baum aus [`theme::tree_ink`].
pub(crate) fn row_ink(ui: &egui::Ui, active: bool) -> Color32 {
    match tree_ink(ui) {
        Some(i) if active => i.active,
        Some(i) => i.rest,
        None => crate::icons::stroke_color(ui.visuals().dark_mode, active),
    }
}

/// Fehlerfarbe einer Zeile: sonst [`error_color`], im durchsichtigen Baum aus
/// [`theme::tree_ink`].
pub(crate) fn row_error(ui: &egui::Ui) -> Color32 {
    tree_ink(ui).map_or_else(|| error_color(ui.visuals().dark_mode), |i| i.error)
}
/// Kurze Statustexte rechts in den Zeilen.
fn status_words() -> [String; 4] {
    [
        tr!("tree-status-error"),
        tr!("tree-status-skipped"),
        tr!("tree-status-suppressed"),
        tr!("tree-status-not-computed"),
    ]
}

// ---- Zustandsfunktionen -----------------------------------------------------------------------

/// Was „Löschen“ für ein Feature bedeutet: das Feature selbst, die abhängigen Features (in
/// der Reihenfolge der Zeitleiste) und ihre Anzeigenamen „„Name“ (Art)“.
#[derive(Clone, Debug, PartialEq)]
pub struct DeletePlan {
    pub target: FeatureId,
    pub dependents: Vec<FeatureId>,
    pub names: Vec<(String, FeatureKind)>,
    /// Name des Features selbst.
    pub target_name: String,
    /// Hinweis zu Skizzen auf Flächen, die nicht mitgelöscht werden, aber danach woanders
    /// liegen können (siehe [`delete_plan`]).
    pub note: Option<Vec<String>>,
}

impl DeletePlan {
    /// Text der Rückfrage.
    pub fn text(&self) -> String {
        let mut t = if self.dependents.is_empty() {
            tr!("tree-delete-single", name = self.target_name.as_str())
        } else {
            let list: Vec<String> = self
                .names
                .iter()
                .map(|(name, kind)| {
                    tr!("tree-plan-item", name = name.as_str(), kind = kind.label())
                })
                .collect();
            tr!(
                "tree-delete-with-dependents",
                name = self.target_name.as_str(),
                count = self.dependents.len(),
                list = list.join(", ")
            )
        };
        if let Some(sketches) = &self.note {
            let quoted: Vec<String> = sketches
                .iter()
                .map(|n| tr!("tree-quoted-name", name = n.as_str()))
                .collect();
            t.push_str("\n\n");
            t.push_str(&tr!("tree-face-note", names = quoted.join(", ")));
        }
        t
    }

    /// Ist eine Rückfrage nötig? Ja, wenn mehr als das Feature selbst gelöscht wird oder ein
    /// Hinweis zu Skizzen auf Flächen besteht.
    pub fn needs_confirmation(&self) -> bool {
        !self.dependents.is_empty() || self.note.is_some()
    }
}

/// Plant das Löschen von `id` (ändert nichts). Eine Skizze auf einer Fläche hängt nur von dem
/// Feature ab, das die Fläche erzeugt hat (Marke der Fläche), nicht von späteren Operationen
/// wie Vereinigen oder Verschieben: Wird eine solche Operation gelöscht, bleibt die Skizze und
/// liegt danach auf der Fläche des ursprünglichen Körpers. Das nennt der Hinweis.
pub fn delete_plan(state: &AppState, id: FeatureId) -> DeletePlan {
    let doc = &state.document;
    let dependents = doc.dependents(id);
    let label = |f: &planora_feature::Feature| (f.name.clone(), f.kind.clone());
    let names = dependents
        .iter()
        .filter_map(|d| doc.feature(*d))
        .map(label)
        .collect();
    let target_name = doc.feature(id).map_or_else(String::new, |f| f.name.clone());
    let operation = doc.feature(id).is_some_and(|f| {
        matches!(
            f.kind,
            FeatureKind::Boolean { .. } | FeatureKind::Move { .. } | FeatureKind::Verrunden { .. }
        )
    });
    let note = if operation {
        let start = doc.index_of(id).unwrap_or(0);
        let face_sketches: Vec<String> = doc.features()[start..]
            .iter()
            .filter(|f| !dependents.contains(&f.id))
            .filter(|f| {
                matches!(&f.kind, FeatureKind::Sketch(s) if matches!(s.plane, SketchPlane::OnFace(_)))
            })
            .map(|f| f.name.clone())
            .collect();
        (!face_sketches.is_empty()).then_some(face_sketches)
    } else {
        None
    };
    DeletePlan {
        target: id,
        dependents,
        names,
        target_name,
        note,
    }
}

/// Führt einen Löschplan aus (das Feature samt Folgen) und schließt die Rückfrage.
///
/// Die Rückfrage kann veraltet sein (etwa weil inzwischen eine Skizze eine Kante des Features
/// kopiert hat). Deshalb wird eine offene Skizze erst beendet und der Plan neu berechnet:
/// Weichen die Folgen vom gezeigten Plan ab, wird nichts gelöscht und die Rückfrage zeigt die
/// neue Liste.
pub fn apply_delete(state: &mut AppState, plan: &DeletePlan) -> Result<(), String> {
    state.message = None;
    if state.document.feature(plan.target).is_none() {
        state.pending_delete = None;
        return state.fail(tr!("tree-feature-gone"));
    }
    if let Err(e) = state.end_sketch_before_delete(plan.target) {
        state.pending_delete = None;
        return Err(e);
    }
    let current = delete_plan(state, plan.target);
    if current.dependents != plan.dependents || current.note != plan.note {
        state.pending_delete = Some(current);
        return state.fail(tr!("tree-plan-changed"));
    }
    state.pending_delete = None;
    state
        .delete_feature_with_dependents(plan.target)
        .map(|_| ())
}

/// „Löschen“ im Kontextmenü: beendet eine offene Skizze, die nicht mitgelöscht wird (damit
/// Kantenkopien aus ihrem Entwurf als Folgen erscheinen), plant das Löschen und löscht sofort,
/// wenn es keine Folgen gibt; sonst steht die Rückfrage in `pending_delete`.
pub fn request_delete(state: &mut AppState, id: FeatureId) {
    if state.document.feature(id).is_none() {
        return;
    }
    state.message = None;
    if state.end_sketch_before_delete(id).is_err() {
        return;
    }
    let plan = delete_plan(state, id);
    if plan.needs_confirmation() {
        state.pending_delete = Some(plan);
    } else {
        let _ = apply_delete(state, &plan);
    }
}

/// „Unterdrücken“/„Aktivieren“. Betrifft es die geöffnete Skizze, bleibt alles unverändert
/// („Zuerst die Skizze beenden“); eine andere offene Skizze wird vorher beendet.
pub fn toggle_suppress(state: &mut AppState, id: FeatureId) -> Result<(), String> {
    state.message = None;
    let Some(suppressed) = state.document.feature(id).map(|f| f.suppressed) else {
        return state.fail(tr!("tree-feature-gone"));
    };
    state.end_sketch_for(Some(id))?;
    if let Err(e) = state.document.set_suppressed(id, !suppressed) {
        return state.fail(e.to_string());
    }
    state.refresh();
    Ok(())
}

/// Globaler Marker hinter `id` (ohne eigene Bedienung im Baum; die Verlaufsleiste setzt
/// Rückrollstellen je Körper).
pub fn roll_back_to(state: &mut AppState, id: FeatureId) -> Result<(), String> {
    state.message = None;
    let Some(idx) = state.document.index_of(id) else {
        return state.fail(tr!("tree-feature-gone"));
    };
    state.end_sketch_for(Some(id))?;
    set_marker(state, idx + 1)
}

/// „Zum Ende“: alle Features werden wieder berechnet.
pub fn roll_to_end(state: &mut AppState) -> Result<(), String> {
    state.message = None;
    state.end_sketch_for(None)?;
    set_marker(state, state.document.features().len())
}

fn set_marker(state: &mut AppState, n: usize) -> Result<(), String> {
    if let Err(e) = state.document.set_marker(n) {
        return state.fail(e.to_string());
    }
    state.refresh();
    Ok(())
}

/// Übernimmt den Namen aus dem Fenster „Umbenennen“. Bei einem ungültigen Namen bleibt das
/// Fenster offen (die Meldung steht in `message`).
pub fn apply_rename(state: &mut AppState) -> Result<(), String> {
    let Some((id, name)) = state.rename_draft.take() else {
        return Ok(());
    };
    match state.rename_feature(id, name.trim()) {
        Ok(()) => {
            state.message = None;
            Ok(())
        }
        Err(e) => {
            state.rename_draft = Some((id, name));
            Err(e)
        }
    }
}

/// Text für die Statuszeile: der Fehler des ersten Features mit Status `Error`.
pub fn status_text(state: &AppState) -> Option<String> {
    state
        .document
        .features()
        .iter()
        .find_map(|f| match &f.status {
            FeatureStatus::Error(e) => Some(tr!(
                "tree-status-line",
                name = f.name.as_str(),
                error = e.as_str()
            )),
            _ => None,
        })
}

// ---- Ordner ----------------------------------------------------------------------------------

/// Ordner des Feature-Baums, von oben nach unten.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Folder {
    /// Die Körper des Dokuments (je Körper eine Zeile, nicht je Schritt).
    Bodies,
    /// Die Skizzen.
    Sketches,
    /// Die Zeichnungen mit ihren Ansichten.
    Drawings,
    /// Die drei Grundebenen.
    Origin,
}

impl Folder {
    /// Alle Ordner in der Reihenfolge des Baums.
    pub const ALL: [Folder; 4] = [
        Folder::Bodies,
        Folder::Sketches,
        Folder::Drawings,
        Folder::Origin,
    ];

    /// Beschriftung im Baum.
    pub fn label(self) -> String {
        match self {
            Folder::Bodies => tr!("tree-folder-bodies"),
            Folder::Sketches => tr!("tree-folder-sketches"),
            Folder::Drawings => tr!("tree-folder-drawings"),
            Folder::Origin => tr!("tree-folder-origin"),
        }
    }

    fn index(self) -> usize {
        self as usize
    }
}

/// Auf- und zugeklappte Ordner eines Tabs (steht im Zustand des Tabs, jedes Tab hat seinen
/// eigenen). Anfangs ist alles offen außer „Ursprung“.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FolderState {
    open: [bool; 4],
}

impl Default for FolderState {
    fn default() -> Self {
        FolderState {
            open: [true, true, true, false],
        }
    }
}

impl FolderState {
    /// Ist der Ordner aufgeklappt?
    pub fn is_open(&self, f: Folder) -> bool {
        self.open[f.index()]
    }

    /// Klappt den Ordner auf (`true`) oder zu.
    pub fn set_open(&mut self, f: Folder, open: bool) {
        self.open[f.index()] = open;
    }

    /// Klappt den Ordner um.
    pub fn toggle(&mut self, f: Folder) {
        self.open[f.index()] ^= true;
    }
}

// ---- Zeichnen ---------------------------------------------------------------------------------

/// Text im Ordner „Körper“, solange es keinen Körper gibt.
pub fn empty_text() -> String {
    tr!("tree-empty")
}

/// Text im Ordner „Skizzen“, solange es keine Skizze gibt.
pub fn no_sketches() -> String {
    tr!("tree-no-sketches")
}

/// Kurzer Statustext eines Körpers, dessen Verlauf zurückgesetzt ist.
pub fn rolled_back() -> String {
    tr!("tree-rolled-back")
}

/// Hinweis zu einem zurückgesetzten Körper.
fn rolled_back_hint() -> String {
    tr!("tree-rolled-back-hint")
}

/// Die drei Grundebenen im Ordner „Ursprung“.
fn planes() -> [String; 3] {
    [
        tr!("tree-plane-xy"),
        tr!("tree-plane-xz"),
        tr!("tree-plane-yz"),
    ]
}

/// Rahmen des Feldes, in dem der Baum steht (die einzige Stelle, an der er festgelegt wird):
/// das Panel, zu `percent` Prozent durchsichtig (Einstellungen → Darstellung).
pub fn tree_frame(dark: bool, percent: u8) -> egui::Frame {
    theme::panel_frame_translucent(dark, percent)
}

/// Wie [`show_with`] im durchsichtigen Panel: hinter alle Zeilen (vom Kopf bis zur letzten
/// Zeile) kommt die Mindestfläche aus [`theme::row_backing`], damit die Schrift über jedem
/// 3D-Hintergrund lesbar bleibt. Der freie Platz darunter und die Ränder bleiben durchsichtig.
pub fn show_in_panel(ui: &mut egui::Ui, state: &mut AppState, read_only: bool, percent: u8) {
    let dark = ui.visuals().dark_mode;
    let backing = theme::row_backing(dark, percent);
    // Platz in der Zeichenliste vor den Zeilen; gefüllt wird, wenn ihre Höhe feststeht.
    let slot = ui.painter().add(egui::Shape::Noop);
    let top = ui.cursor().min;
    // Kräftigere Schrift im durchsichtigen Baum: Zeilen holen sie über [`row_ink`] und
    // [`row_error`], schwacher Text (auch `ui.weak`) über den Stil. Danach gilt wieder alles wie
    // beim Aufrufer.
    let tree = theme::tree_ink(dark);
    let saved = ui.style().clone();
    ui.style_mut().visuals.weak_text_color = Some(tree.weak);
    ui.ctx().data_mut(|d| d.insert_temp(tree_ink_id(), tree));
    show_with(ui, state, read_only);
    ui.ctx()
        .data_mut(|d| d.remove::<theme::TreeInk>(tree_ink_id()));
    ui.set_style(saved);
    if backing != Color32::TRANSPARENT {
        let rect = egui::Rect::from_min_max(
            top,
            egui::pos2(ui.max_rect().right(), ui.min_rect().bottom()),
        );
        ui.painter().set(
            slot,
            egui::epaint::RectShape::filled(rect, theme::RADIUS_SM, backing),
        );
    }
}

/// Befehl aus einer Zeile (wird nach dem Zeichnen ausgeführt).
enum Action {
    Select(FeatureId),
    /// Körperzeile: Spitze für die Verlaufsleiste und, falls sichtbar, Index in `bodies`.
    SelectBody(FeatureId, Option<usize>),
    ToggleHidden(usize),
    Fold(Folder),
    Edit(FeatureId),
    Rename(FeatureId),
    ToggleSuppress(FeatureId),
    Extrude(FeatureId),
    Revolve(FeatureId),
    Delete(FeatureId),
}

/// Angaben einer Skizzenzeile (aus dem Dokument kopiert, damit danach geändert werden darf).
struct SketchRow {
    id: FeatureId,
    name: String,
    suppressed: bool,
    status: FeatureStatus,
}

/// Angaben einer Körperzeile: ein sichtbarer Körper oder, wenn von einem Körper gerade nichts
/// zu sehen ist (zurückgesetzt, unterdrückt, Fehler), seine Spitze.
struct BodyRow {
    /// Spitze des Körpers ([`planora_feature::Document::body_tips`]).
    tip: FeatureId,
    /// Ursprung des Körpers (das Feature, das ihn erzeugt hat): Ziel von Bearbeiten,
    /// Umbenennen, Unterdrücken und Löschen.
    target: FeatureId,
    /// Index in `bodies`, wenn der Körper sichtbar ist.
    index: Option<usize>,
    name: String,
    icon: FeatureIcon,
    color: Color32,
    locked: bool,
    hidden: bool,
    suppressed: bool,
    status: FeatureStatus,
}

/// Die Körperzeilen in der Reihenfolge des Entstehens (erster Körperschritt, wie die Farbe): je
/// sichtbarem Körper eine Zeile (nach einem zurückgesetzten Vereinigen also beide
/// Eingangskörper), sonst eine Zeile für die Spitze. Ein Körper, der in einem Vereinigen
/// aufgegangen ist, erscheint nicht mehr einzeln.
fn body_rows(state: &AppState, dark: bool) -> Vec<BodyRow> {
    let doc = &state.document;
    let chains: Vec<(FeatureId, Vec<FeatureId>)> = doc
        .body_tips()
        .into_iter()
        .map(|t| (t, doc.chain(t)))
        .collect();
    let owner = |f: FeatureId| {
        chains
            .iter()
            .find(|(_, c)| c.contains(&f))
            .map_or(f, |(t, _)| *t)
    };
    let owners: Vec<FeatureId> = state
        .bodies
        .iter()
        .map(|b| owner(b.source.feature))
        .collect();
    // Spitzen, dazu (nur zur Sicherheit) Besitzer ohne Spitze; feste Reihenfolge nach dem ersten
    // Körperschritt (wie die Farbe), damit Verschieben oder Bearbeiten nicht umsortiert.
    let mut tips: Vec<FeatureId> = chains.iter().map(|(t, _)| *t).collect();
    for o in &owners {
        if !tips.contains(o) {
            tips.push(*o);
        }
    }
    tips.sort_by_key(|t| (crate::history_bar::body_root(state, *t).unwrap_or(*t), *t));
    let mut rows = Vec::new();
    for tip in tips {
        let Some(tip_feature) = doc.feature(tip) else {
            continue;
        };
        let color = crate::history_bar::body_color(state, tip).get(dark);
        let locked = crate::history_bar::is_locked(state, tip);
        let mut push = |r: BodyRef, index: Option<usize>, name: String, icon: FeatureIcon| {
            let target = doc.naming_origin(r).feature;
            let suppressed = doc.feature(target).is_some_and(|f| f.suppressed);
            let status = if suppressed {
                FeatureStatus::Suppressed
            } else {
                tip_feature.status.clone()
            };
            rows.push(BodyRow {
                tip,
                target,
                index,
                name,
                icon,
                color,
                locked,
                hidden: index.is_some_and(|j| state.is_body_hidden(j)),
                suppressed,
                status,
            });
        };
        let mut shown = false;
        for (j, b) in state.bodies.iter().enumerate() {
            if owners[j] == tip {
                shown = true;
                push(b.source, Some(j), b.name.clone(), FeatureIcon::Body(b.kind));
            }
        }
        if !shown {
            let r = BodyRef {
                feature: tip,
                index: 0,
            };
            let origin = doc.naming_origin(r).feature;
            let name = doc
                .body_display_name(r)
                .unwrap_or_else(|| tip_feature.name.clone());
            let icon = doc
                .feature(origin)
                .map_or(FeatureIcon::of(&tip_feature.kind), |f| {
                    FeatureIcon::of(&f.kind)
                });
            push(r, None, name, icon);
        }
    }
    rows
}

/// Kurzer Statustext rechts in der Zeile, seine Farbe (`None`: normal) und der Grund für den
/// Tooltip.
fn status_view(
    status: &FeatureStatus,
    weak: Color32,
    error: Color32,
) -> (Option<String>, Option<Color32>, Option<String>) {
    let [error_word, skipped_word, suppressed_word, not_computed_word] = status_words();
    match status {
        FeatureStatus::Ok => (None, None, None),
        FeatureStatus::Error(e) => (Some(error_word), Some(error), Some(e.clone())),
        FeatureStatus::Skipped(why) => (Some(skipped_word), Some(weak), Some(why.clone())),
        FeatureStatus::Suppressed => (
            Some(suppressed_word),
            Some(weak),
            Some(tr!("tree-suppressed-hint")),
        ),
        FeatureStatus::NotComputed => (
            Some(not_computed_word),
            Some(weak),
            Some(tr!("tree-not-computed-hint")),
        ),
    }
}

fn rows_id() -> egui::Id {
    egui::Id::new("planora-browser-rows")
}

fn folders_id() -> egui::Id {
    egui::Id::new("planora-browser-folders")
}

fn eyes_id() -> egui::Id {
    egui::Id::new("planora-browser-eyes")
}

/// Namen und Rechtecke (volle Zeilenbreite) der im letzten Bild gezeichneten Zeilen: Körper,
/// Skizzen, Zeichnungen und Ansichten, Ebenen, Maße und Positionen, von oben nach unten (ohne
/// Dokument und Ordner, siehe [`folder_rects`]).
#[doc(hidden)]
pub fn row_rects(ctx: &egui::Context) -> Vec<(String, egui::Rect)> {
    ctx.data(|d| d.get_temp(rows_id())).unwrap_or_default()
}

/// Die im letzten Bild gezeichneten Ordnerköpfe mit ihren Rechtecken.
#[doc(hidden)]
pub fn folder_rects(ctx: &egui::Context) -> Vec<(Folder, egui::Rect)> {
    ctx.data(|d| d.get_temp(folders_id())).unwrap_or_default()
}

/// Die im letzten Bild gezeichneten Augen (Name der Körperzeile, Rechteck).
#[doc(hidden)]
pub fn eye_rects(ctx: &egui::Context) -> Vec<(String, egui::Rect)> {
    ctx.data(|d| d.get_temp(eyes_id())).unwrap_or_default()
}

fn remember<T: Clone + Send + Sync + 'static>(ui: &egui::Ui, id: egui::Id, item: T) {
    ui.ctx()
        .data_mut(|d| d.get_temp_mut_or_default::<Vec<T>>(id).push(item));
}

/// Einrückung der Zeilen unter einem Eintrag.
pub(crate) const fn body_indent() -> f32 {
    BODY_INDENT
}

/// Eine Zeile mit Symbol, Name, optionalem Zusatz hinter dem Namen und optionalem Status
/// rechts (nur wenn er neben den ganzen Namen passt, sonst im Hinweis); liefert die Antwort für
/// Klicks.
#[allow(
    clippy::too_many_arguments,
    reason = "eine Zeile hat so viele Bestandteile"
)]
pub(crate) fn row(
    ui: &mut egui::Ui,
    indent: f32,
    selected: bool,
    name: &str,
    color: Option<Color32>,
    status: Option<(&str, Color32)>,
    detail: Option<&str>,
    paint_icon: impl FnOnce(&egui::Painter, egui::Rect, Color32),
) -> egui::Response {
    row_lead(
        ui, indent, 0.0, selected, name, color, status, detail, paint_icon,
    )
}

/// Wie [`row`], mit `lead` Pixeln Platz vor dem Symbol (für Auge und Farbquadrat, die der
/// Aufrufer in die Zeile zeichnet).
#[allow(
    clippy::too_many_arguments,
    reason = "eine Zeile hat so viele Bestandteile"
)]
fn row_lead(
    ui: &mut egui::Ui,
    indent: f32,
    lead: f32,
    selected: bool,
    name: &str,
    color: Option<Color32>,
    status: Option<(&str, Color32)>,
    detail: Option<&str>,
    paint_icon: impl FnOnce(&egui::Painter, egui::Rect, Color32),
) -> egui::Response {
    let (full, response) =
        ui.allocate_exact_size(vec2(ui.available_width(), ROW_HEIGHT), Sense::click());
    remember(ui, rows_id(), (name.to_string(), full));
    let rect = egui::Rect::from_min_max(full.min + vec2(indent, 0.0), full.max);
    // Graphit: in Ruhe ohne Fläche und in text_secondary; überfahren bg_field, gewählt bg_active
    // (neutral, kein Akzent), beide mit text_primary.
    let dark = ui.visuals().dark_mode;
    let p = theme::palette(dark);
    let hovered = response.hovered() && ui.is_enabled();
    if selected {
        ui.painter()
            .rect_filled(rect, theme::RADIUS_SM, p.bg_active);
    } else if hovered {
        ui.painter().rect_filled(rect, theme::RADIUS_SM, p.bg_field);
    }
    let active = selected || hovered;
    // Text und gezeichnetes Symbol in derselben Farbe (Symbolfarbe wie bei allen Handsymbolen).
    let color = color.unwrap_or(row_ink(ui, active));
    let icon_rect = egui::Rect::from_center_size(
        egui::pos2(
            rect.min.x + theme::SPACE_2 + lead + ICON_SIZE / 2.0,
            rect.center().y,
        ),
        vec2(ICON_SIZE, ICON_SIZE),
    );
    paint_icon(ui.painter(), icon_rect, color);
    // Der Name bekommt die ganze Breite. Der Status rechts braucht nur seinen eigenen Platz;
    // passen beide nicht nebeneinander, wird der Name mit „…“ gekürzt und der Status steht nur
    // im Hinweis.
    let name_x = icon_rect.right() + theme::SPACE_2;
    let right = rect.max.x - theme::SPACE_2;
    let font = theme::label_font();
    let name_w = ui
        .painter()
        .layout_no_wrap(name.to_string(), font.clone(), color)
        .size()
        .x;
    let status_w = status.map(|(t, c)| {
        ui.painter()
            .layout_no_wrap(t.to_string(), theme::value_font(), c)
            .size()
            .x
    });
    let show_status = status_w.is_some_and(|w| name_x + name_w + STATUS_GAP + w <= right);
    let name_end = match status_w {
        Some(w) if show_status => right - w - STATUS_GAP,
        _ => right,
    };
    let name_clip =
        egui::Rect::from_min_max(rect.min, egui::pos2(name_end.max(rect.min.x), rect.max.y));
    let name_rect = if name_x + name_w <= name_end {
        ui.painter().text(
            egui::pos2(name_x, rect.center().y),
            egui::Align2::LEFT_CENTER,
            name,
            font,
            color,
        )
    } else {
        let mut job = egui::text::LayoutJob::simple_singleline(name.to_string(), font, color);
        job.wrap = egui::text::TextWrapping {
            max_width: (name_end - name_x).max(0.0),
            max_rows: 1,
            break_anywhere: true,
            overflow_character: Some('…'),
        };
        let galley = ui.painter().layout_job(job);
        let pos = egui::pos2(name_x, rect.center().y - galley.size().y / 2.0);
        let r = egui::Rect::from_min_size(pos, galley.size());
        ui.painter()
            .with_clip_rect(name_clip)
            .galley(pos, galley, color);
        r
    };
    // Kleiner, schwacher Zusatz hinter dem Namen (Blatt und Projektion, Maßstab).
    if let Some(detail) = detail {
        ui.painter().with_clip_rect(name_clip).text(
            egui::pos2(name_rect.right() + 8.0, rect.center().y),
            egui::Align2::LEFT_CENTER,
            detail,
            theme::value_font(),
            ui.visuals().weak_text_color(),
        );
    }
    match status {
        Some((text, c)) if show_status => {
            ui.painter().text(
                egui::pos2(right, rect.center().y),
                egui::Align2::RIGHT_CENTER,
                text,
                theme::value_font(),
                c,
            );
        }
        Some((text, _)) => return response.on_hover_text(text),
        None => {}
    }
    response
}

/// Kopf eines Ordners: Pfeil (zu: nach rechts, offen: nach unten), Ordnersymbol, Name. Ein
/// Klick klappt ihn um.
fn folder_header(ui: &mut egui::Ui, folder: Folder, open: bool) -> egui::Response {
    let (rect, response) =
        ui.allocate_exact_size(vec2(ui.available_width(), ROW_HEIGHT), Sense::click());
    remember(ui, folders_id(), (folder, rect));
    let enabled = ui.is_enabled();
    response.widget_info(|| {
        egui::WidgetInfo::selected(
            egui::WidgetType::CollapsingHeader,
            enabled,
            open,
            folder.label(),
        )
    });
    let dark = ui.visuals().dark_mode;
    let p = theme::palette(dark);
    let hovered = response.hovered() && enabled;
    if hovered {
        ui.painter().rect_filled(rect, theme::RADIUS_SM, p.bg_field);
    }
    let color = row_ink(ui, true);
    let caret_c = egui::pos2(rect.min.x + theme::SPACE_1 + CARET / 2.0, rect.center().y);
    let h = CARET / 2.0;
    let points = if open {
        vec![
            caret_c + vec2(-h, -h / 2.0),
            caret_c + vec2(h, -h / 2.0),
            caret_c + vec2(0.0, h / 2.0 + 1.0),
        ]
    } else {
        vec![
            caret_c + vec2(-h / 2.0, -h),
            caret_c + vec2(h / 2.0 + 1.0, 0.0),
            caret_c + vec2(-h / 2.0, h),
        ]
    };
    ui.painter().add(egui::Shape::convex_polygon(
        points,
        row_ink(ui, hovered),
        egui::Stroke::NONE,
    ));
    let icon_rect = egui::Rect::from_center_size(
        egui::pos2(
            rect.min.x + theme::SPACE_1 + CARET + theme::SPACE_1 + ICON_SIZE / 2.0,
            rect.center().y,
        ),
        vec2(ICON_SIZE, ICON_SIZE),
    );
    paint_source(
        ui.painter(),
        icon_rect,
        IconSource::Font(Icon::Folder),
        color,
    );
    ui.painter().text(
        egui::pos2(icon_rect.right() + theme::SPACE_2, rect.center().y),
        egui::Align2::LEFT_CENTER,
        folder.label(),
        theme::label_font(),
        color,
    );
    response.on_hover_text(if open {
        tr!("tree-fold-close")
    } else {
        tr!("tree-fold-open")
    })
}

/// Zeile des Dokuments ganz oben (Name des Tabs).
fn document_row(ui: &mut egui::Ui, name: &str) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), ROW_HEIGHT), Sense::hover());
    let color = row_ink(ui, true);
    let icon_rect = egui::Rect::from_center_size(
        egui::pos2(
            rect.min.x + theme::SPACE_2 + ICON_SIZE / 2.0,
            rect.center().y,
        ),
        vec2(ICON_SIZE, ICON_SIZE),
    );
    paint_source(ui.painter(), icon_rect, IconSource::Font(Icon::File), color);
    ui.painter().text(
        egui::pos2(icon_rect.right() + theme::SPACE_2, rect.center().y),
        egui::Align2::LEFT_CENTER,
        name,
        theme::label_font(),
        color,
    );
}

/// Inhalt eines Ordners, um [`FOLDER_INDENT`] eingerückt.
fn indented<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let mut rect = ui.available_rect_before_wrap();
    rect.min.x += FOLDER_INDENT;
    ui.scope_builder(
        egui::UiBuilder::new().max_rect(rect).layout(*ui.layout()),
        add,
    )
    .inner
}

/// Feature-Baum mit Kontextmenü, Fenster „Umbenennen“ und Rückfrage beim Löschen: das Dokument,
/// darunter die Ordner „Körper“, „Skizzen“, „Zeichnungen“ (mit den Maßen und Positionen der
/// geöffneten Zeichnung) und „Ursprung“.
pub fn show(ui: &mut egui::Ui, state: &mut AppState) {
    show_with(ui, state, false);
}

/// Wie [`show`]; `read_only` (alte Version) sperrt alle Änderungen an Körpern, Skizzen und
/// Zeichnungen, lässt aber das Auf- und Zuklappen der Ordner und das Wählen und Öffnen von
/// Zeichnungen zu.
pub fn show_with(ui: &mut egui::Ui, state: &mut AppState, read_only: bool) {
    ui.ctx().data_mut(|d| {
        d.insert_temp(rows_id(), Vec::<(String, egui::Rect)>::new());
        d.insert_temp(folders_id(), Vec::<(Folder, egui::Rect)>::new());
        d.insert_temp(eyes_id(), Vec::<(String, egui::Rect)>::new());
    });
    theme::section_header(ui, &tr!("tree-header"), theme::HEADER_H);
    ui.add_space(theme::SPACE_1);
    let name = state.document_name.clone();
    document_row(ui, &name);
    let mut action = None;
    indented(ui, |ui| {
        for folder in Folder::ALL {
            let open = state.tree_folders.is_open(folder);
            if folder_header(ui, folder, open).clicked() {
                action = Some(Action::Fold(folder));
            }
            if !open {
                continue;
            }
            indented(ui, |ui| match folder {
                Folder::Bodies => {
                    ui.add_enabled_ui(!read_only, |ui| bodies(ui, state, &mut action));
                }
                Folder::Sketches => {
                    ui.add_enabled_ui(!read_only, |ui| sketches(ui, state, &mut action));
                }
                Folder::Drawings => {
                    crate::drawing_browser::show(ui, state, read_only);
                    // Maße und Positionen der geöffneten Zeichnung gehören zu den Zeichnungen.
                    crate::dimension_browser::show(ui, state, read_only);
                    crate::balloon_browser::show(ui, state, read_only);
                }
                Folder::Origin => origin(ui),
            });
        }
    });
    if let Some(a) = action {
        run(state, a);
    }
    rename_window(ui.ctx(), state);
    delete_window(ui.ctx(), state);
    crate::drawing_browser::windows(ui.ctx(), state);
    crate::dimension_browser::windows(ui.ctx(), state);
}

/// Misst die Statustexte einmal aus: Das legt ihre Zeichen schon im ersten Bild in den
/// Schriftatlas, sodass das Erscheinen eines Status später keine Texturänderung braucht.
fn prime_status_words(ui: &egui::Ui) {
    let weak = ui.visuals().weak_text_color();
    for t in status_words().into_iter().chain([rolled_back()]) {
        let _ = ui.painter().layout_no_wrap(t, theme::value_font(), weak);
    }
}

/// Ordner „Körper“: je Körper Auge, Farbquadrat, Symbol, Name und Status.
fn bodies(ui: &mut egui::Ui, state: &AppState, action: &mut Option<Action>) {
    let dark = ui.visuals().dark_mode;
    let weak = ui.visuals().weak_text_color();
    prime_status_words(ui);
    let rows = body_rows(state, dark);
    if rows.is_empty() {
        ui.weak(empty_text());
    }
    for r in rows {
        let selected = r.index.is_some_and(|j| state.selected_body == Some(j))
            || state
                .selected_feature
                .is_some_and(|f| f == r.target || f == r.tip);
        let (status, mut color, reason) = if r.locked {
            (Some(rolled_back()), Some(weak), Some(rolled_back_hint()))
        } else {
            status_view(&r.status, weak, row_error(ui))
        };
        if r.hidden && color.is_none() {
            color = Some(weak);
        }
        let icon = r.icon;
        let resp = row_lead(
            ui,
            0.0,
            EYE_W + SWATCH + theme::SPACE_1,
            selected,
            &r.name,
            color,
            status.as_deref().zip(color),
            None,
            |p, rect, c| paint_feature_icon(p, rect, icon, c),
        );
        let full = resp.rect;
        let resp = resp.on_hover_text(reason.unwrap_or_else(|| tr!("tree-body-hint")));
        if resp.double_clicked() {
            *action = Some(Action::Edit(r.target));
        } else if resp.clicked() {
            *action = Some(Action::SelectBody(r.tip, r.index));
        }
        let target = r.target;
        let suppressed = r.suppressed;
        resp.context_menu(|ui| {
            feature_menu(ui, action, target, suppressed, None);
        });
        // Farbquadrat in der Körperfarbe (wie die Schritte in der Leiste).
        let lead_x = full.min.x + theme::SPACE_2;
        let swatch = egui::Rect::from_center_size(
            egui::pos2(lead_x + EYE_W + SWATCH / 2.0, full.center().y),
            vec2(SWATCH, SWATCH),
        );
        let ink = if r.locked || r.hidden {
            r.color.gamma_multiply(theme::HISTORY_DIM_ALPHA)
        } else {
            r.color
        };
        ui.painter()
            .rect_filled(swatch, egui::CornerRadius::same(2), ink);
        // Auge: nur für sichtbare Körper (sonst gibt es nichts aus- oder einzublenden).
        if let Some(j) = r.index {
            let eye = egui::Rect::from_min_size(
                egui::pos2(lead_x - theme::SPACE_1, full.min.y),
                vec2(EYE_W, ROW_HEIGHT),
            );
            let id = ui.id().with(("eye", r.tip, j));
            let e = ui.interact(eye, id, Sense::click());
            remember(ui, eyes_id(), (r.name.clone(), eye));
            let hovered = e.hovered() && ui.is_enabled();
            let glyph = if r.hidden {
                Icon::Hidden
            } else {
                Icon::Visible
            };
            let c = row_ink(ui, hovered);
            paint_source(
                ui.painter(),
                egui::Rect::from_center_size(eye.center(), vec2(EYE_ICON, EYE_ICON)),
                IconSource::Font(glyph),
                if r.hidden { weak } else { c },
            );
            let e = e.on_hover_ui(|ui| {
                let km = crate::keymap::mouse::current(ui.ctx());
                let text = if r.hidden {
                    tr!("tree-show-body")
                } else {
                    tr!("tree-hide-body")
                };
                ui.label(crate::keymap::with_hint(
                    &km,
                    crate::keymap::Action::ToggleVisibility,
                    &text,
                ));
            });
            if e.clicked() {
                *action = Some(Action::ToggleHidden(j));
            }
        }
    }
}

/// Ordner „Skizzen“: je Skizze eine Zeile mit den bisherigen Befehlen.
fn sketches(ui: &mut egui::Ui, state: &AppState, action: &mut Option<Action>) {
    let weak = ui.visuals().weak_text_color();
    prime_status_words(ui);
    let open = state.open_sketch_id();
    let rows: Vec<SketchRow> = state
        .document
        .features()
        .iter()
        .filter(|f| matches!(f.kind, FeatureKind::Sketch(_)))
        .map(|f| SketchRow {
            id: f.id,
            name: f.name.clone(),
            suppressed: f.suppressed,
            status: f.status.clone(),
        })
        .collect();
    if rows.is_empty() {
        ui.weak(no_sketches());
    }
    for r in rows {
        let id = r.id;
        let selected = state.selected_feature == Some(id) || open == Some(id);
        let name = if open == Some(id) {
            tr!("tree-in-progress", name = r.name.as_str())
        } else {
            r.name.clone()
        };
        let (status, color, reason) = status_view(&r.status, weak, row_error(ui));
        let resp = row(
            ui,
            0.0,
            selected,
            &name,
            color,
            status.as_deref().zip(color),
            None,
            |p, rect, c| paint_feature_icon(p, rect, FeatureIcon::Sketch, c),
        )
        .on_hover_text(reason.unwrap_or_else(|| tr!("tree-sketch-hint")));
        if resp.double_clicked() {
            *action = Some(Action::Edit(id));
        } else if resp.clicked() {
            *action = Some(Action::Select(id));
        }
        // Neue Körper aus der Skizze eines zurückgesetzten Körpers sind gesperrt.
        let lock = crate::history_bar::lock_reason(state, &[id]);
        resp.context_menu(|ui| {
            feature_menu(ui, action, id, r.suppressed, Some(lock));
        });
    }
}

/// Kontextmenü einer Körper- oder Skizzenzeile für das Feature `id`. `sketch` (nur bei
/// Skizzen): die Sperre für neue Körper aus ihr.
fn feature_menu(
    ui: &mut egui::Ui,
    action: &mut Option<Action>,
    id: FeatureId,
    suppressed: bool,
    sketch: Option<Option<String>>,
) {
    let mut pick = |a: Action, ui: &mut egui::Ui| {
        *action = Some(a);
        ui.close();
    };
    // Die Tasten der Menüeinträge stehen rechts, wie sie in den Einstellungen belegt sind.
    let km = crate::keymap::mouse::current(ui.ctx());
    let key_button = |label: &str, a: crate::keymap::Action| {
        let keys = crate::keymap::shortcut_hint(&km, a);
        let b = egui::Button::new(label);
        if keys.is_empty() {
            b
        } else {
            b.shortcut_text(egui::RichText::new(keys).font(theme::kbd_font()))
        }
    };
    if ui.button(tr!("tree-edit")).clicked() {
        pick(Action::Edit(id), ui);
    }
    if ui.button(tr!("tree-rename")).clicked() {
        pick(Action::Rename(id), ui);
    }
    let toggle = if suppressed {
        tr!("tree-activate")
    } else {
        tr!("tree-suppress")
    };
    if ui.button(toggle).clicked() {
        pick(Action::ToggleSuppress(id), ui);
    }
    if let Some(lock) = sketch {
        ui.separator();
        for (label, act, key) in [
            (
                tr!("tree-extrude"),
                Action::Extrude(id),
                Some(crate::keymap::Action::ExtrudeSketch),
            ),
            (tr!("tree-revolve"), Action::Revolve(id), None),
        ] {
            let button = match key {
                Some(k) => key_button(&label, k),
                None => egui::Button::new(label),
            };
            if ui
                .add_enabled(lock.is_none(), button)
                .on_disabled_hover_text(lock.clone().unwrap_or_default())
                .clicked()
            {
                pick(act, ui);
            }
        }
    }
    ui.separator();
    if ui
        .add(key_button(
            &tr!("tree-delete"),
            crate::keymap::Action::DeleteSelection,
        ))
        .clicked()
    {
        pick(Action::Delete(id), ui);
    }
}

/// Ordner „Ursprung“: die drei Grundebenen (nur zur Anzeige).
fn origin(ui: &mut egui::Ui) {
    for plane in planes() {
        row(ui, 0.0, false, &plane, None, None, None, |p, rect, c| {
            paint_source(p, rect, IconSource::Font(Icon::SketchOnPlane), c)
        })
        .on_hover_text(tr!("tree-plane-hint"));
    }
}

/// Führt einen Befehl aus einer Zeile aus. Fehler legt `AppState` in `message` ab.
fn run(state: &mut AppState, action: Action) {
    match action {
        Action::Select(id) => {
            // Nur eines ist hervorgehoben: Feature statt Körper bzw. 3D-Auswahl.
            state.selected_feature = Some(id);
            state.selected_body = None;
            state.drawing_ui.selected_drawing = None;
            state.set_selection(None);
        }
        Action::SelectBody(tip, index) => {
            state.set_selection(None);
            state.selected_feature = None;
            state.drawing_ui.selected_drawing = None;
            state.selected_body = index;
            state.select_body(Some(tip));
        }
        Action::ToggleHidden(j) => {
            let hidden = state.is_body_hidden(j);
            state.set_body_hidden(j, !hidden);
        }
        Action::Fold(folder) => state.tree_folders.toggle(folder),
        Action::Edit(id) => edit(state, id),
        Action::Rename(id) => {
            if let Some(f) = state.document.feature(id) {
                state.rename_draft = Some((id, f.name.clone()));
            }
        }
        Action::ToggleSuppress(id) => {
            let _ = toggle_suppress(state, id);
        }
        // Körper entstehen außerhalb des Skizzen-Modus: die offene Skizze wird vorher beendet.
        Action::Extrude(id) => state.request_sketch_dialog(SketchRequest::Extrude(id)),
        Action::Revolve(id) => state.request_sketch_dialog(SketchRequest::Revolve(id)),
        Action::Delete(id) => request_delete(state, id),
    }
}

/// „Bearbeiten“ (Doppelklick im Baum oder in der Verlaufsleiste): wählt das Feature `id`; eine
/// Skizze wird zum Bearbeiten geöffnet, sonst wird der Dialog angefordert (`edit_request`, eine
/// offene Skizze wird vorher beendet). Nicht gesperrt, auch nicht bei zurückgesetzten Körpern.
pub(crate) fn edit(state: &mut AppState, id: FeatureId) {
    state.selected_feature = Some(id);
    state.selected_body = None;
    state.drawing_ui.selected_drawing = None;
    state.set_selection(None);
    let sketch = state
        .document
        .feature(id)
        .is_some_and(|f| matches!(f.kind, FeatureKind::Sketch(_)));
    if sketch {
        if state.open_sketch_id() != Some(id) {
            let _ = state.open_sketch(id);
        }
    } else if state.try_close_sketch().is_ok() {
        // Bearbeiten ändert das Dokument: eine offene Skizze wird vorher beendet.
        state.edit_request = Some(id);
    }
}

/// Kleines Fenster „Umbenennen“ mit Textfeld.
fn rename_window(ctx: &egui::Context, state: &mut AppState) {
    let Some((_, name)) = state.rename_draft.as_mut() else {
        return;
    };
    let (mut apply, mut cancel) = (false, false);
    let mut open = true;
    Dialog::new(tr!("tree-rename"), DialogSize::Prompt)
        .open(&mut open)
        .show(ctx, |ui| {
            let r = theme::text_field(ui, egui::TextEdit::singleline(name));
            if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                apply = true;
            }
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui.button(tr!("tree-apply")).clicked() {
                    apply = true;
                }
                if ui.button(tr!("common-cancel")).clicked() {
                    cancel = true;
                }
            });
        });
    if apply {
        let _ = apply_rename(state);
    } else if cancel || !open {
        state.rename_draft = None;
    }
}

/// Rückfrage „Löschen mit Folgen“.
fn delete_window(ctx: &egui::Context, state: &mut AppState) {
    let Some(plan) = state.pending_delete.clone() else {
        return;
    };
    let (mut confirm, mut cancel) = (false, false);
    let mut open = true;
    Dialog::new(tr!("tree-confirm-title"), DialogSize::Prompt)
        .open(&mut open)
        .show(ctx, |ui| {
            ui.label(plan.text());
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui
                    .button(
                        egui::RichText::new(tr!("tree-delete"))
                            .color(error_color(ui.visuals().dark_mode)),
                    )
                    .clicked()
                {
                    confirm = true;
                }
                if ui.button(tr!("common-cancel")).clicked() {
                    cancel = true;
                }
            });
        });
    if confirm {
        let _ = apply_delete(state, &plan);
    } else if cancel || !open {
        state.pending_delete = None;
    }
}
