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

use crate::dimension_state::{no_drawing, no_view, views_pending};
use crate::dimension_tool::sentence;
use crate::drawing_dialog::sketch_blocked;
use crate::drawing_state::WorkArea;
use crate::drawing_view::PICK_PX;
use crate::msg::Msg;
use crate::state::{AppState, BodyKind};
use egui::Pos2;
use planora_drawing::balloon::{
    BalloonMarks, DEFAULT_BALLOON_OFFSET, anchor_of, balloon_marks, layout_balloon,
};
use planora_drawing::parts::MAX_BALLOONS;
use planora_drawing::sheet::view_transform;
use planora_drawing::sheet_frame::sheet_scale;
use planora_drawing::{BalloonId, BodyKey, Drawing, DrawingId, DrawingKind, ViewId, ViewLines};
use planora_feature::FeatureError;
use planora_i18n::tr;
use planora_math::DVec2;
use std::collections::BTreeMap;
use std::sync::Arc;

/// Beschriftung des Schalters.
pub fn tool() -> String {
    tr!("balloon-tool")
}
/// Tooltip des Schalters.
pub fn tool_tip() -> String {
    tr!("balloon-tool-tip")
}
/// Hinweis bei eingeschaltetem Werkzeug.
pub fn hint() -> String {
    tr!("balloon-hint")
}
/// Meldung bei einem Klick neben die Linien.
pub fn missed() -> Msg {
    crate::msg!("balloon-missed")
}
/// Erklärung, wenn der Schalter in einer alten Version gesperrt ist.
pub fn read_only_tip() -> String {
    tr!("balloon-read-only-tip")
}
/// Erklärung am gesperrten Schalter eines Stücklistenblatts.
pub fn parts_sheet_tip() -> String {
    tr!("balloon-parts-sheet-tip")
}
/// Tooltip eines verlorenen Kreises.
pub fn lost_tip() -> String {
    tr!("balloon-lost-tip")
}
/// Meldung, wenn die Zeichnung schon die Höchstzahl an Positionsnummern hat.
pub fn too_many() -> Msg {
    crate::msg!("balloon-too-many", max = MAX_BALLOONS)
}

/// Hinweis zu verlorenen Kreisen: „1 Positionsnummer hat ihr Teil verloren.“ bzw.
/// „{n} Positionsnummern haben ihr Teil verloren.“
pub fn lost_note(n: usize) -> Msg {
    crate::msg!("balloon-lost-note", n = n)
}

/// Zustand des Werkzeugs „Position“.
#[derive(Debug, Default)]
pub struct BalloonTool {
    active: bool,
    message: Option<Msg>,
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

    /// Letzte Ablehnung (rot in der Hinweiszeile), bis zum nächsten Klick, in der aktuellen
    /// Sprache.
    pub fn message(&self) -> Option<String> {
        self.message.as_ref().map(Msg::text)
    }

    /// Setzt die Meldung für die Hinweiszeile.
    pub fn set_message(&mut self, message: Option<Msg>) {
        self.message = message;
    }
}

/// Kann das Werkzeug benutzt werden? Sonst die Erklärung (alte Version, Stücklistenblatt, keine
/// Ansicht). Isometrische Ansichten sind erlaubt.
pub fn blocked(drawing: &Drawing, editable: bool) -> Option<String> {
    blocked_msg(drawing, editable).map(|m| m.text())
}

/// Wie [`blocked`], als Meldung (übersetzt erst beim Zeichnen).
pub fn blocked_msg(drawing: &Drawing, editable: bool) -> Option<Msg> {
    if !editable {
        Some(crate::msg!("balloon-read-only-tip"))
    } else if matches!(drawing.kind, DrawingKind::PartsList { .. }) {
        Some(crate::msg!("balloon-parts-sheet-tip"))
    } else if drawing.views.is_empty() {
        Some(crate::msg!("dim-no-views-tip"))
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

fn feature_text(e: FeatureError) -> Msg {
    match e {
        FeatureError::InvalidInput(m) => Msg::Raw(sentence(&m)),
        e => Msg::Raw(sentence(&e.to_string())),
    }
}

/// Setzt einen Nummernkreis an den Körper `body` (Index unter den exakten Körpern, in einer
/// Schnittansicht unter ihren geschnittenen Körpern) in der Ansicht `view` mit dem Anker am
/// Blattpunkt `at`. Eine Änderung des Dokuments; bei einem Fehler bleibt
/// es unverändert, und der Grund kommt als Satz zurück.
pub fn place(
    state: &mut AppState,
    drawing: DrawingId,
    view: ViewId,
    body: usize,
    at: DVec2,
) -> Result<BalloonId, Msg> {
    if state.sketch_session.is_some() {
        return Err(sketch_blocked());
    }
    let d = state.document.drawing(drawing).ok_or_else(no_drawing)?;
    if let Some(why) = blocked_msg(d, true) {
        return Err(why);
    }
    let spec = d.view(view).ok_or_else(no_view)?;
    if d.balloons.len() >= MAX_BALLOONS {
        return Err(too_many());
    }
    if !at.is_finite() {
        return Err(missed());
    }
    // In einer Schnittansicht zählt die Herkunft der Linie in den geschnittenen Körpern; der
    // Kreis gehört zum ursprünglichen Körper (`SectionData::source`).
    let body = if spec.section.is_some() {
        let data = state
            .drawing_ui
            .section_ref(spec, state.document.model_edit_count())
            .ok_or_else(views_pending)?;
        *data.source.get(body).ok_or_else(missed)?
    } else {
        body
    };
    let key = body_key_of_stroke(state, body).ok_or_else(missed)?;
    // Nur Körper, die eine Zeile der Stückliste haben.
    if state.document.row_of(key).is_none() {
        return Err(missed());
    }
    let owned = state
        .drawing_ui
        .results_for(d, state.document.model_edit_count());
    let refs: BTreeMap<ViewId, &ViewLines> = owned.iter().map(|(k, l)| (*k, l.as_ref())).collect();
    let (Some(xf), Some(lines)) = (view_transform(d, view, &refs), refs.get(&view)) else {
        return Err(views_pending());
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
        state.message = Some(feature_text(e).text());
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

/// Nummern der Kreise je Zeichnung, je Stand des Dokuments einmal gerechnet (die Stückliste
/// hängt an Körpern, Namen und eigenen Texten; jede Änderung erhöht den Änderungszähler).
#[derive(Clone)]
struct NumbersCache {
    instance: u64,
    edit: u64,
    drawing: DrawingId,
    numbers: Arc<Numbers>,
}

/// Nummer je Kreis (`None`: verloren).
pub type Numbers = BTreeMap<BalloonId, Option<u32>>;

/// Wie [`planora_feature::Document::balloon_numbers`], aus dem Zwischenspeicher von `ctx`.
pub fn numbers_cached(ctx: &egui::Context, state: &AppState, drawing: DrawingId) -> Arc<Numbers> {
    let id = egui::Id::new("planora-balloon-numbers");
    let edit = state.document.edit_count();
    if let Some(c) = ctx
        .data(|d| d.get_temp::<NumbersCache>(id))
        .filter(|c| c.instance == state.instance && c.edit == edit && c.drawing == drawing)
    {
        return c.numbers;
    }
    let has = state
        .document
        .drawing(drawing)
        .is_some_and(|d| !d.balloons.is_empty());
    let numbers = Arc::new(if has {
        state.document.balloon_numbers(drawing)
    } else {
        Numbers::new()
    });
    ctx.data_mut(|d| {
        d.insert_temp(
            id,
            NumbersCache {
                instance: state.instance,
                edit,
                drawing,
                numbers: Arc::clone(&numbers),
            },
        )
    });
    numbers
}

/// Die Kreise der Zeichnung auf dem Blatt mit den Nummern `numbers`.
pub fn balloon_list(
    drawing: &Drawing,
    results: &BTreeMap<ViewId, &ViewLines>,
    numbers: &Numbers,
) -> Vec<BalloonMarks> {
    if drawing.balloons.is_empty() {
        return Vec::new();
    }
    balloon_marks(drawing, results, numbers)
}

/// `list` mit dem Kreis `id` am Versatz `offset` (beim Ziehen, bevor das Dokument geändert wird);
/// der Anker bleibt, die übrigen Kreise auch.
pub fn with_moved(
    mut list: Vec<BalloonMarks>,
    drawing: &Drawing,
    numbers: &Numbers,
    (id, offset): (BalloonId, (f64, f64)),
) -> Vec<BalloonMarks> {
    let (Some(bm), Some(b)) = (list.iter_mut().find(|x| x.id == id), drawing.balloon(id)) else {
        return list;
    };
    let at = bm.centre - DVec2::new(b.offset.0, b.offset.1);
    let number = numbers.get(&id).copied().flatten();
    let marks = layout_balloon(at, offset, number, sheet_scale(drawing.sheet));
    if !marks.is_empty() {
        bm.centre = at + DVec2::new(offset.0, offset.1);
        bm.marks = marks;
    }
    list
}

/// Zahl der verlorenen Kreise (Teil gibt es nicht mehr).
pub fn lost_count(numbers: &Numbers) -> usize {
    numbers.values().filter(|n| n.is_none()).count()
}
