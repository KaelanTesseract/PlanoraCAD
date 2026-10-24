//! Feature-Baum: eine Zeile je Feature der Zeitleiste mit Symbol, Name und Status, darunter
//! eingerückt die sichtbaren Körper des Features; Rückrollmarker als Trennlinie; Kontextmenü
//! je Zeile. Die Zustandsfunktionen (Löschen mit Folgen, Unterdrücken, Zurückrollen) sind
//! ohne Oberfläche testbar.

use crate::dialog::{Dialog, DialogSize};
use crate::icons::{FeatureIcon, paint_body_icon, paint_feature_icon};
use crate::state::{AppState, BodyKind, SketchRequest};
use crate::theme;
use egui::{Color32, Sense, vec2};
use planora_feature::{FeatureId, FeatureKind, FeatureStatus, SketchPlane};

/// Zeilenhöhe (Graphit: 24).
const ROW_HEIGHT: f32 = theme::ROW_H;
/// Einrückung der Körperzeilen unter ihrem Feature.
const BODY_INDENT: f32 = 22.0;
const MARKER_HEIGHT: f32 = 20.0;
/// Größe der Zeilensymbole.
const ICON_SIZE: f32 = 16.0;
/// Rot für Fehler im dunklen Thema (Wert aus der Palette); die Oberfläche nutzt `error_color`.
pub const ERROR_COLOR: Color32 = theme::DARK.error;

/// Fehlerfarbe des Themas.
pub fn error_color(dark: bool) -> Color32 {
    theme::palette(dark).error
}
/// Text der Rückroll-Linie.
const MARKER_TEXT: &str = "Berechnung endet hier";
/// Kurze Statustexte rechts in den Zeilen.
const STATUS_WORDS: [&str; 4] = ["Fehler", "übersprungen", "unterdrückt", "nicht berechnet"];

// ---- Zustandsfunktionen -----------------------------------------------------------------------

/// Was „Löschen“ für ein Feature bedeutet: das Feature selbst, die abhängigen Features (in
/// der Reihenfolge der Zeitleiste) und ihre Anzeigenamen „„Name“ (Art)“.
#[derive(Clone, Debug, PartialEq)]
pub struct DeletePlan {
    pub target: FeatureId,
    pub dependents: Vec<FeatureId>,
    pub names: Vec<String>,
    /// Name des Features selbst.
    pub target_name: String,
    /// Hinweis zu Skizzen auf Flächen, die nicht mitgelöscht werden, aber danach woanders
    /// liegen können (siehe [`delete_plan`]).
    pub note: Option<String>,
}

impl DeletePlan {
    /// Text der Rückfrage.
    pub fn text(&self) -> String {
        let mut t = if self.dependents.is_empty() {
            format!("„{}“ wird gelöscht.", self.target_name)
        } else {
            format!(
                "„{}“ und {} davon abhängige Feature(s) werden gelöscht: {}",
                self.target_name,
                self.dependents.len(),
                self.names.join(", ")
            )
        };
        if let Some(note) = &self.note {
            t.push_str("\n\n");
            t.push_str(note);
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
    let label = |f: &planora_feature::Feature| format!("„{}“ ({})", f.name, f.kind.label());
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
            .map(|f| format!("„{}“", f.name))
            .collect();
        (!face_sketches.is_empty()).then(|| {
            format!(
                "Hinweis: Eine Skizze auf einer Fläche hängt nur von dem Feature ab, das die \
                 Fläche erzeugt hat (Grundkörper, Extrusion oder Rotation), nicht von späteren \
                 Operationen. {} bleibt deshalb erhalten und liegt danach auf der Fläche dieses \
                 Körpers.",
                face_sketches.join(", ")
            )
        })
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
        return state.fail("Das Feature gibt es nicht mehr".into());
    }
    if let Err(e) = state.end_sketch_before_delete(plan.target) {
        state.pending_delete = None;
        return Err(e);
    }
    let current = delete_plan(state, plan.target);
    if current.dependents != plan.dependents || current.note != plan.note {
        state.pending_delete = Some(current);
        return state.fail(
            "Die Folgen des Löschens haben sich geändert; bitte die neue Liste prüfen".into(),
        );
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
        return state.fail("Das Feature gibt es nicht mehr".into());
    };
    state.end_sketch_for(Some(id))?;
    if let Err(e) = state.document.set_suppressed(id, !suppressed) {
        return state.fail(e.to_string());
    }
    state.refresh();
    Ok(())
}

/// „Zurückrollen bis hier“: der Marker steht danach hinter `id`.
pub fn roll_back_to(state: &mut AppState, id: FeatureId) -> Result<(), String> {
    state.message = None;
    let Some(idx) = state.document.index_of(id) else {
        return state.fail("Das Feature gibt es nicht mehr".into());
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
            FeatureStatus::Error(e) => Some(format!("„{}“: {e}", f.name)),
            _ => None,
        })
}

// ---- Zeichnen ---------------------------------------------------------------------------------

/// Befehl aus einer Zeile (wird nach dem Zeichnen ausgeführt).
enum Action {
    Select(FeatureId),
    SelectBody(usize),
    Edit(FeatureId),
    Rename(FeatureId),
    ToggleSuppress(FeatureId),
    RollBack(FeatureId),
    ToEnd,
    Extrude(FeatureId),
    Revolve(FeatureId),
    Delete(FeatureId),
}

/// Angaben einer Feature-Zeile (aus dem Dokument kopiert, damit danach geändert werden darf).
struct RowData {
    id: FeatureId,
    name: String,
    icon: FeatureIcon,
    sketch: bool,
    suppressed: bool,
    status: FeatureStatus,
}

/// Kurzer Statustext rechts in der Zeile, seine Farbe (`None`: normal) und der Grund für den
/// Tooltip.
fn status_view(
    status: &FeatureStatus,
    weak: Color32,
    dark: bool,
) -> (Option<&'static str>, Option<Color32>, Option<String>) {
    match status {
        FeatureStatus::Ok => (None, None, None),
        FeatureStatus::Error(e) => (
            Some(STATUS_WORDS[0]),
            Some(error_color(dark)),
            Some(e.clone()),
        ),
        FeatureStatus::Skipped(why) => (Some(STATUS_WORDS[1]), Some(weak), Some(why.clone())),
        FeatureStatus::Suppressed => (
            Some(STATUS_WORDS[2]),
            Some(weak),
            Some("Unterdrückt: wird nicht berechnet (Rechtsklick, dann „Aktivieren“)".into()),
        ),
        FeatureStatus::NotComputed => (
            Some(STATUS_WORDS[3]),
            Some(weak),
            Some(
                "Steht hinter der Linie „Berechnung endet hier“ und wird nicht berechnet \
                 (Rechtsklick, dann „Zum Ende“)"
                    .into(),
            ),
        ),
    }
}

fn rows_id() -> egui::Id {
    egui::Id::new("planora-browser-rows")
}

/// Namen und Rechtecke (volle Zeilenbreite) der im letzten Bild gezeichneten Zeilen: Features,
/// Körper, Zeichnungen und Ansichten, von oben nach unten.
#[doc(hidden)]
pub fn row_rects(ctx: &egui::Context) -> Vec<(String, egui::Rect)> {
    ctx.data(|d| d.get_temp(rows_id())).unwrap_or_default()
}

/// Einrückung der Zeilen unter einem Eintrag.
pub(crate) const fn body_indent() -> f32 {
    BODY_INDENT
}

/// Eine Zeile mit Symbol, Name, optionalem Zusatz hinter dem Namen und optionalem Status
/// rechts; liefert die Antwort für Klicks.
#[allow(
    clippy::too_many_arguments,
    reason = "eine Zeile hat so viele Bestandteile"
)]
pub(crate) fn row(
    ui: &mut egui::Ui,
    indent: f32,
    status_width: f32,
    selected: bool,
    name: &str,
    color: Option<Color32>,
    status: Option<(&str, Color32)>,
    detail: Option<&str>,
    paint_icon: impl FnOnce(&egui::Painter, egui::Rect, Color32),
) -> egui::Response {
    let (full, response) =
        ui.allocate_exact_size(vec2(ui.available_width(), ROW_HEIGHT), Sense::click());
    ui.ctx().data_mut(|d| {
        d.get_temp_mut_or_default::<Vec<(String, egui::Rect)>>(rows_id())
            .push((name.to_string(), full));
    });
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
    let color = color.unwrap_or(crate::icons::stroke_color(dark, active));
    let icon_rect = egui::Rect::from_center_size(
        egui::pos2(
            rect.min.x + theme::SPACE_2 + ICON_SIZE / 2.0,
            rect.center().y,
        ),
        vec2(ICON_SIZE, ICON_SIZE),
    );
    paint_icon(ui.painter(), icon_rect, color);
    // Lange Namen werden vor der Statusspalte abgeschnitten.
    let name_clip = egui::Rect::from_min_max(
        rect.min,
        egui::pos2(
            (rect.max.x - status_width - 10.0).max(rect.min.x),
            rect.max.y,
        ),
    );
    let name_rect = ui.painter().with_clip_rect(name_clip).text(
        egui::pos2(icon_rect.right() + theme::SPACE_2, rect.center().y),
        egui::Align2::LEFT_CENTER,
        name,
        theme::label_font(),
        color,
    );
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
    if let Some((text, c)) = status {
        ui.painter().text(
            egui::pos2(rect.max.x - theme::SPACE_2, rect.center().y),
            egui::Align2::RIGHT_CENTER,
            text,
            theme::value_font(),
            c,
        );
    }
    response
}

/// Trennlinie des Rückrollmarkers.
fn marker_line(ui: &mut egui::Ui) {
    let marker = theme::palette(ui.visuals().dark_mode).marker;
    let (rect, response) =
        ui.allocate_exact_size(vec2(ui.available_width(), MARKER_HEIGHT), Sense::hover());
    let y = rect.center().y;
    let text_rect = ui.painter().text(
        egui::pos2(rect.min.x + 8.0, y),
        egui::Align2::LEFT_CENTER,
        MARKER_TEXT,
        theme::value_font(),
        marker,
    );
    let stroke = egui::Stroke::new(2.0, marker);
    ui.painter().line_segment(
        [egui::pos2(rect.min.x, y), egui::pos2(rect.min.x + 4.0, y)],
        stroke,
    );
    ui.painter().line_segment(
        [
            egui::pos2(text_rect.max.x + 6.0, y),
            egui::pos2(rect.max.x, y),
        ],
        stroke,
    );
    response.on_hover_text(
        "Features unter dieser Linie werden nicht berechnet. Rechtsklick auf ein Feature, dann \
         „Zum Ende“ berechnet wieder alles.",
    );
}

/// Feature-Baum mit Kontextmenü, Fenster „Umbenennen“ und Rückfrage beim Löschen, darunter
/// der Abschnitt „Zeichnungen“.
pub fn show(ui: &mut egui::Ui, state: &mut AppState) {
    show_with(ui, state, false);
}

/// Wie [`show`]; `read_only` (alte Version) sperrt den Feature-Baum und alle Änderungen an
/// Zeichnungen, lässt aber das Wählen und Öffnen von Zeichnungen zu.
pub fn show_with(ui: &mut egui::Ui, state: &mut AppState, read_only: bool) {
    ui.ctx()
        .data_mut(|d| d.insert_temp(rows_id(), Vec::<(String, egui::Rect)>::new()));
    ui.add_enabled_ui(!read_only, |ui| feature_tree(ui, state));
    ui.add_space(theme::SPACE_3);
    crate::drawing_browser::show(ui, state, read_only);
    crate::dimension_browser::show(ui, state, read_only);
    crate::balloon_browser::show(ui, state, read_only);
}

fn feature_tree(ui: &mut egui::Ui, state: &mut AppState) {
    theme::section_header(ui, "Feature-Baum", theme::HEADER_H);
    ui.add_space(theme::SPACE_1);
    let rows: Vec<RowData> = state
        .document
        .features()
        .iter()
        .map(|f| RowData {
            id: f.id,
            name: f.name.clone(),
            icon: FeatureIcon::of(&f.kind),
            sketch: matches!(f.kind, FeatureKind::Sketch(_)),
            suppressed: f.suppressed,
            status: f.status.clone(),
        })
        .collect();
    if rows.is_empty() {
        ui.weak(
            "Noch keine Features. Über „Erzeugen“ im Menü lässt sich ein Körper oder eine \
             Skizze anlegen.",
        );
    }
    let marker = state.document.marker();
    let open = state.open_sketch_id();
    let weak = ui.visuals().weak_text_color();
    // Breite der Statusspalte. Das Ausmessen legt die Zeichen der Statustexte und der
    // Rückroll-Linie zugleich schon im ersten Bild in den Schriftatlas, sodass das Erscheinen
    // eines Status später keine Texturänderung braucht.
    let status_width = STATUS_WORDS
        .iter()
        .chain([&MARKER_TEXT])
        .map(|t| {
            ui.painter()
                .layout_no_wrap(t.to_string(), theme::value_font(), weak)
                .size()
                .x
        })
        .fold(0.0, f32::max);
    let mut action = None;
    if marker == 0 && !rows.is_empty() {
        marker_line(ui);
    }
    for (i, r) in rows.iter().enumerate() {
        let id = r.id;
        let selected = state.selected_feature == Some(id) || open == Some(id);
        let name = if open == Some(id) {
            format!("{} (in Bearbeitung)", r.name)
        } else {
            r.name.clone()
        };
        let (status, color, reason) = status_view(&r.status, weak, ui.visuals().dark_mode);
        let icon = r.icon;
        let mut resp = row(
            ui,
            0.0,
            status_width,
            selected,
            &name,
            color,
            status.zip(color),
            None,
            |p, rect, c| paint_feature_icon(p, rect, icon, c),
        );
        resp = match reason {
            Some(reason) => resp.on_hover_text(reason),
            None => resp.on_hover_text(if r.sketch {
                "Doppelklick öffnet die Skizze zum Bearbeiten; Rechtsklick zeigt weitere Befehle."
            } else {
                "Doppelklick öffnet „Bearbeiten“; Rechtsklick zeigt weitere Befehle."
            }),
        };
        if resp.double_clicked() {
            action = Some(Action::Edit(id));
        } else if resp.clicked() {
            action = Some(Action::Select(id));
        }
        resp.context_menu(|ui| {
            let mut pick = |a: Action, ui: &mut egui::Ui| {
                action = Some(a);
                ui.close();
            };
            if ui.button("Bearbeiten").clicked() {
                pick(Action::Edit(id), ui);
            }
            if ui.button("Umbenennen").clicked() {
                pick(Action::Rename(id), ui);
            }
            let toggle = if r.suppressed {
                "Aktivieren"
            } else {
                "Unterdrücken"
            };
            if ui.button(toggle).clicked() {
                pick(Action::ToggleSuppress(id), ui);
            }
            ui.separator();
            if ui.button("Zurückrollen bis hier").clicked() {
                pick(Action::RollBack(id), ui);
            }
            if ui
                .add_enabled(marker < rows.len(), egui::Button::new("Zum Ende"))
                .on_disabled_hover_text("Es wird bereits alles berechnet.")
                .clicked()
            {
                pick(Action::ToEnd, ui);
            }
            if r.sketch {
                ui.separator();
                // Neue Körper aus der Skizze eines zurückgesetzten Körpers sind gesperrt.
                let lock = crate::history_bar::lock_reason(state, &[id]);
                for (label, act) in [
                    ("Extrudieren …", Action::Extrude(id)),
                    ("Drehen …", Action::Revolve(id)),
                ] {
                    if ui
                        .add_enabled(lock.is_none(), egui::Button::new(label))
                        .on_disabled_hover_text(lock.unwrap_or_default())
                        .clicked()
                    {
                        pick(act, ui);
                    }
                }
            }
            ui.separator();
            if ui.button("Löschen").clicked() {
                pick(Action::Delete(id), ui);
            }
        });
        // Die sichtbaren Körper des Features, eingerückt.
        let bodies: Vec<(usize, String, BodyKind)> = state
            .bodies
            .iter()
            .enumerate()
            .filter(|(_, b)| b.source.feature == id)
            .map(|(j, b)| (j, b.name.clone(), b.kind))
            .collect();
        for (j, body_name, kind) in bodies {
            let r = row(
                ui,
                BODY_INDENT,
                0.0,
                state.selected_body == Some(j),
                &body_name,
                None,
                None,
                None,
                |p, rect, c| paint_body_icon(p, rect, kind, c),
            )
            .on_hover_text("Körper; Klick wählt ihn für die Eigenschaften.");
            if r.clicked() {
                action = Some(Action::SelectBody(j));
            }
        }
        if i + 1 == marker && marker < rows.len() {
            marker_line(ui);
        }
    }
    if let Some(a) = action {
        run(state, a);
    }
    rename_window(ui.ctx(), state);
    delete_window(ui.ctx(), state);
}

/// Führt einen Befehl aus einer Zeile aus. Fehler legt `AppState` in `message` ab.
fn run(state: &mut AppState, action: Action) {
    let select = |state: &mut AppState, id: FeatureId| {
        // Nur eines ist hervorgehoben: Feature statt Körper bzw. 3D-Auswahl.
        state.selected_feature = Some(id);
        state.selected_body = None;
        state.drawing_ui.selected_drawing = None;
        state.set_selection(None);
    };
    match action {
        Action::Select(id) => select(state, id),
        Action::SelectBody(j) => {
            state.set_selection(None);
            state.selected_feature = None;
            state.drawing_ui.selected_drawing = None;
            state.selected_body = Some(j);
        }
        Action::Edit(id) => edit(state, id),
        Action::Rename(id) => {
            if let Some(f) = state.document.feature(id) {
                state.rename_draft = Some((id, f.name.clone()));
            }
        }
        Action::ToggleSuppress(id) => {
            let _ = toggle_suppress(state, id);
        }
        Action::RollBack(id) => {
            let _ = roll_back_to(state, id);
        }
        Action::ToEnd => {
            let _ = roll_to_end(state);
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
    Dialog::new("Umbenennen", DialogSize::Prompt)
        .open(&mut open)
        .show(ctx, |ui| {
            let r = theme::text_field(ui, egui::TextEdit::singleline(name));
            if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                apply = true;
            }
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui.button("Übernehmen").clicked() {
                    apply = true;
                }
                if ui.button("Abbrechen").clicked() {
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
    Dialog::new("Rückfrage", DialogSize::Prompt)
        .open(&mut open)
        .show(ctx, |ui| {
            ui.label(plan.text());
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui
                    .button(
                        egui::RichText::new("Löschen").color(error_color(ui.visuals().dark_mode)),
                    )
                    .clicked()
                {
                    confirm = true;
                }
                if ui.button("Abbrechen").clicked() {
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
