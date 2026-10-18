//! Werkzeug „Position“ (Teilprojekt 3c-2, Aufgabe 7): Positionsnummern setzen, wählen, ziehen
//! und löschen.
//!
//! **Setzen:** ein Klick auf eine Linie eines Teils (dieselben Linienzüge wie beim Werkzeug
//! „Maß“) setzt einen Nummernkreis. Der Körper kommt aus der Herkunft der getroffenen Linie
//! (`StrokeOrigin`, Index unter den exakten Körpern) und wird über seine stabile Kennung
//! (`Document::body_key`) gemerkt, nicht über den Index. Der Anker ist der Klickpunkt relativ zur
//! Hüllbox der Ansicht ([`anchor_of`]), der Versatz zur Kreismitte die Vorgabe
//! `(10, 10) · f` ([`DEFAULT_BALLOON_OFFSET`], `f` = Faktor des Blatts). Die Kreismitte bleibt auf
//! dem Papier ([`clamp_offset`]).
//!
//! **Wählen und Ziehen** gehen auch ohne Werkzeug: getroffen wird ein Kreis, wenn der Zeiger
//! höchstens [`PICK_PX`] außerhalb seines Rands liegt ([`balloon_hit`]). Ziehen verschiebt nur den
//! Kreis (der Anker bleibt am Teil) und ist am Ende eine einzige Änderung des Dokuments.

use crate::dimension_state::{NO_DRAWING, NO_VIEW, VIEWS_PENDING};
use crate::dimension_tool::{NO_VIEWS_TIP, sentence};
use crate::drawing_dialog::SKETCH_BLOCKED;
use crate::drawing_state::WorkArea;
use crate::drawing_view::PICK_PX;
use crate::state::{AppState, BodyKind};
use egui::Pos2;
use planora_drawing::balloon::{BalloonMarks, DEFAULT_BALLOON_OFFSET, anchor_of, balloon_marks};
use planora_drawing::parts::MAX_BALLOONS;
use planora_drawing::sheet::view_transform;
use planora_drawing::sheet_frame::sheet_scale;
use planora_drawing::{BalloonId, BodyKey, Drawing, DrawingId, DrawingKind, ViewId, ViewLines};
use planora_feature::{FeatureError, TOO_MANY_BALLOONS};
use planora_math::DVec2;
use std::collections::BTreeMap;

/// Beschriftung des Schalters.
pub const TOOL: &str = "Position";
/// Tooltip des Schalters.
pub const TOOL_TIP: &str = "Ein Klick auf eine Linie eines Teils setzt eine Positionsnummer. Den Kreis kann man danach verschieben.";
/// Hinweis bei eingeschaltetem Werkzeug.
pub const HINT: &str = "Auf eine Linie eines Teils klicken. Esc beendet.";
/// Meldung bei einem Klick neben die Linien.
pub const MISSED: &str = "Bitte eine Linie eines Teils anklicken.";
/// Erklärung, wenn der Schalter in einer alten Version gesperrt ist.
pub const READ_ONLY_TIP: &str = "In einer alten Version lassen sich keine Positionsnummern setzen.";
/// Erklärung am gesperrten Schalter eines Stücklistenblatts.
pub const PARTS_SHEET_TIP: &str = "Auf dem Stücklistenblatt gibt es keine Positionsnummern.";
/// Tooltip eines verlorenen Kreises.
pub const LOST_TIP: &str = "Das Teil zu dieser Positionsnummer gibt es nicht mehr.";

/// Hinweis zu verlorenen Kreisen: „1 Positionsnummer hat ihr Teil verloren.“ bzw.
/// „{n} Positionsnummern haben ihr Teil verloren.“
pub fn lost_note(n: usize) -> String {
    if n == 1 {
        "1 Positionsnummer hat ihr Teil verloren.".into()
    } else {
        format!("{n} Positionsnummern haben ihr Teil verloren.")
    }
}

/// Zustand des Werkzeugs „Position“.
#[derive(Debug, Default)]
pub struct BalloonTool {
    active: bool,
    message: Option<String>,
}

impl BalloonTool {
    /// Ist das Werkzeug eingeschaltet?
    pub fn is_active(&self) -> bool {
        self.active
    }

    /// Schaltet das Werkzeug ein oder aus; eine Meldung verfällt dabei.
    pub fn set_active(&mut self, on: bool) {
        self.active = on;
        self.message = None;
    }

    /// Letzte Ablehnung (rot in der Hinweiszeile), bis zum nächsten Klick.
    pub fn message(&self) -> Option<&str> {
        self.message.as_deref()
    }

    /// Setzt die Meldung für die Hinweiszeile.
    pub fn set_message(&mut self, message: Option<String>) {
        self.message = message;
    }
}

/// Kann das Werkzeug benutzt werden? Sonst die Erklärung (alte Version, Stücklistenblatt, keine
/// Ansicht). Isometrische Ansichten sind erlaubt.
pub fn blocked(drawing: &Drawing, editable: bool) -> Option<&'static str> {
    if !editable {
        Some(READ_ONLY_TIP)
    } else if matches!(drawing.kind, DrawingKind::PartsList { .. }) {
        Some(PARTS_SHEET_TIP)
    } else if drawing.views.is_empty() {
        Some(NO_VIEWS_TIP)
    } else {
        None
    }
}

/// Stabile Kennung des Körpers `body` (Index unter den exakten Körpern, wie in den Ansichten:
/// Netz-Körper zählen nicht mit).
pub fn body_key_of_stroke(state: &AppState, body: usize) -> Option<BodyKey> {
    state
        .bodies
        .iter()
        .filter(|b| b.kind != BodyKind::Mesh)
        .nth(body)
        .map(|b| state.document.body_key(b.source))
}

/// Versatz `offset` so begrenzt, dass die Kreismitte `at + offset` auf dem Papier `size` liegt.
pub fn clamp_offset(at: DVec2, offset: (f64, f64), size: (f64, f64)) -> (f64, f64) {
    let cx = (at.x + offset.0).clamp(0.0, size.0);
    let cy = (at.y + offset.1).clamp(0.0, size.1);
    (cx - at.x, cy - at.y)
}

fn feature_text(e: FeatureError) -> String {
    match e {
        FeatureError::InvalidInput(m) => sentence(&m),
        e => sentence(&e.to_string()),
    }
}

/// Setzt einen Nummernkreis an den Körper `body` (Index unter den exakten Körpern) in der Ansicht
/// `view` mit dem Anker am Blattpunkt `at`. Eine Änderung des Dokuments; bei einem Fehler bleibt
/// es unverändert, und der Grund kommt als Satz zurück.
pub fn place(
    state: &mut AppState,
    drawing: DrawingId,
    view: ViewId,
    body: usize,
    at: DVec2,
) -> Result<BalloonId, String> {
    if state.sketch_session.is_some() {
        return Err(SKETCH_BLOCKED.into());
    }
    let d = state.document.drawing(drawing).ok_or(NO_DRAWING)?;
    if let Some(why) = blocked(d, true) {
        return Err(why.into());
    }
    if d.view(view).is_none() {
        return Err(NO_VIEW.into());
    }
    if d.balloons.len() >= MAX_BALLOONS {
        return Err(TOO_MANY_BALLOONS.into());
    }
    if !at.is_finite() {
        return Err(MISSED.into());
    }
    let key = body_key_of_stroke(state, body).ok_or(MISSED)?;
    // Nur Körper, die eine Zeile der Stückliste haben.
    if state.document.row_of(key).is_none() {
        return Err(MISSED.into());
    }
    let owned = state
        .drawing_ui
        .results_for(d, state.document.model_edit_count());
    let refs: BTreeMap<ViewId, &ViewLines> = owned.iter().map(|(k, l)| (*k, l.as_ref())).collect();
    let (Some(xf), Some(lines)) = (view_transform(d, view, &refs), refs.get(&view)) else {
        return Err(VIEWS_PENDING.into());
    };
    let anchor = anchor_of(lines.bbox, &xf, at);
    let point = planora_drawing::balloon::anchor_point(lines.bbox, &xf, anchor);
    let f = sheet_scale(d.sheet);
    let offset = clamp_offset(
        point,
        (DEFAULT_BALLOON_OFFSET.0 * f, DEFAULT_BALLOON_OFFSET.1 * f),
        d.sheet.size_mm(),
    );
    state
        .document
        .add_balloon(drawing, view, key, anchor, offset)
        .map_err(feature_text)
}

/// Löscht den gewählten Kreis der geöffneten Zeichnung (eine Änderung); `false`, wenn keiner
/// gewählt ist oder es ihn nicht mehr gibt.
pub fn delete_selected(state: &mut AppState) -> bool {
    let (WorkArea::Drawing(id), Some(b)) =
        (state.drawing_ui.area, state.drawing_ui.selected_balloon)
    else {
        return false;
    };
    delete(state, id, b)
}

/// Löscht den Kreis `b` der Zeichnung `drawing` (eine Änderung); die Auswahl verfällt.
pub fn delete(state: &mut AppState, drawing: DrawingId, b: BalloonId) -> bool {
    let mut removed = false;
    let r = state.document.update_drawing(drawing, |d| {
        removed = d.remove_balloon(b);
    });
    if let Err(e) = r {
        state.message = Some(feature_text(e));
        return false;
    }
    if state.drawing_ui.selected_balloon == Some(b) {
        state.drawing_ui.selected_balloon = None;
    }
    removed
}

/// Kreis unter `pos`: der Zeiger liegt höchstens [`PICK_PX`] außerhalb des Kreisrands; von
/// mehreren der, dessen Rand am nächsten (bzw. am tiefsten innen) liegt.
pub fn balloon_hit(
    list: &[BalloonMarks],
    to_screen: &dyn Fn(DVec2) -> Pos2,
    pos: Pos2,
) -> Option<BalloonId> {
    let mut best: Option<(f32, BalloonId)> = None;
    for b in list {
        let c = to_screen(b.centre);
        let edge = to_screen(b.centre + DVec2::new(b.radius, 0.0));
        let r = (edge - c).length();
        let d = (pos - c).length() - r;
        if d.is_finite() && d <= PICK_PX && best.is_none_or(|(x, _)| d < x) {
            best = Some((d, b.id));
        }
    }
    best.map(|(_, id)| id)
}

/// Die Kreise der Zeichnung auf dem Blatt mit den Nummern aus der Stückliste; `moved` ersetzt
/// den Versatz eines Kreises (beim Ziehen, bevor das Dokument geändert wird).
pub fn balloon_list(
    state: &AppState,
    drawing: &Drawing,
    results: &BTreeMap<ViewId, &ViewLines>,
    moved: Option<(BalloonId, (f64, f64))>,
) -> Vec<BalloonMarks> {
    if drawing.balloons.is_empty() {
        return Vec::new();
    }
    let numbers = state.document.balloon_numbers(drawing.id);
    match moved {
        Some((id, offset)) => {
            let mut d = drawing.clone();
            if let Some(b) = d.balloons.iter_mut().find(|b| b.id == id) {
                b.offset = offset;
            }
            balloon_marks(&d, results, &numbers)
        }
        None => balloon_marks(drawing, results, &numbers),
    }
}

/// Zahl der verlorenen Kreise der Zeichnung (Teil gibt es nicht mehr).
pub fn lost_count(state: &AppState, drawing: &Drawing) -> usize {
    if drawing.balloons.is_empty() {
        return 0;
    }
    state
        .document
        .balloon_numbers(drawing.id)
        .values()
        .filter(|n| n.is_none())
        .count()
}
