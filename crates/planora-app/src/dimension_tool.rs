//! Werkzeug „Maß“ (Teilprojekt 12, Aufgabe 12): Maße von Hand setzen.
//!
//! Ablauf: Untermodus wählen ([`ToolMode`]), Linien der Ansicht anklicken ([`DimensionTool::click`]);
//! jeder Klick wird sofort geprüft und abgelehnt, wenn er nicht passt (das Dokument bleibt dann
//! unverändert). Sind genug Bezüge gewählt, folgt die Maßlinie dem Zeiger
//! ([`DimensionTool::hover`]); der nächste Klick setzt das Maß ([`DimensionTool::finish`]).
//!
//! **Bezüge** kommen aus dem Ursprung der getroffenen Blattlinie (`SheetLine.origin`) und werden
//! genau wie beim Vorschlag der Grundmaße in Modellverweise umgewandelt (gleiche Schlüssel, der
//! Vorschlag setzt ein von Hand gesetztes Maß nicht noch einmal): Kante → `Edge`; bei linearen
//! Maßen, Kette und Bezugsmaßen ein Klick bis [`END_PX`] Pixel an einem Kantenende → `EdgeEnd`,
//! bis [`END_PX`] an der Mitte eines Kreises → `Center`; Umriss einer Fläche → `Face`.
//!
//! **Lage aus dem Zeiger** (dieselbe Regel wie im Layout und beim Ziehen): Lineare Maße nehmen
//! die Querachse `w` des Layouts (waagerechte Maße: oben positiv, senkrechte: rechts positiv) und
//! die äußersten Querlagen aller Bezüge (bei Kette und Bezugsmaßen der ganzen Gruppe). Liegt der
//! Zeiger auf der positiven Hälfte, ist `offset = Zeiger − größte Querlage` (mindestens
//! [`MIN_DRAG_OFFSET_MM`]), sonst `Zeiger − kleinste Querlage` (höchstens −1 mm). Ø/R mit
//! Führungslinie: Winkel und Abstand vom Kreis aus der Zeigerlage um die Mitte; an einem Bogen
//! wird der Winkel auf den Bogen geklemmt (`clamp_leader_deg`, wie beim Ziehen), damit die
//! Pfeilspitze den Bogen trifft. Winkel: Radius =
//! Abstand des Zeigers vom Scheitel.
//!
//! **Vorschau = Ergebnis:** die Vorschau entsteht mit derselben Messung und demselben Layout
//! ([`dimension_marks`]) wie die gesetzten Maße.
//!
//! **Kette:** Punkte A, B, C … nacheinander, Messrichtung (waagerecht oder senkrecht) aus den
//! ersten beiden; ein Klick ins Leere, ein Doppelklick oder Esc setzt die Maße AB, BC … mit
//! gemeinsamer Maßlinie. **Bezugsmaße:** erster Klick = Bezug, weitere = Ziele; die Maßlinien
//! stapeln sich um `DIM_STEP`. Beide bekommen eine neue Gruppennummer der Zeichnung.

use crate::dimension_state::{NO_DRAWING, NO_VIEW, VIEWS_PENDING, full_text, geo_ref, next_group};
use crate::drawing_view::{MIN_DRAG_OFFSET_MM, normalize_deg};
use crate::state::{AppState, BodyKind};
use planora_drawing::annotate::{
    DEFAULT_OFFSET, LARGE_DIAMETER, Mark, ResolvedDims, clamp_leader_deg, group_base,
};
use planora_drawing::dimension::{
    ChainMode, ChainSpec, DimError, DimId, DimKind, DimSpec, EndSel, Geo3, GeoRef,
    Tolerance as DimTolerance,
};
use planora_drawing::limits::MAX_DIMENSIONS;
use planora_drawing::sheet::{SheetTransform, dimension_marks, view_transform};
use planora_drawing::stroke::{StrokeOrigin, extent_of};
use planora_drawing::{
    Anchor, Basis, CandidateOrigin, DrawBody, DrawError, Drawing, DrawingId, Measured, ViewId,
    ViewKind, ViewLines, ViewSpec, measure,
};
use planora_feature::{RefCache, resolve_geo_cached};
use planora_kernel::{Curve, Edge, Id, Solid};
use planora_math::{DVec2, DVec3, Tolerance};
use std::collections::BTreeMap;

/// Abstand (Pixel), bis zu dem ein Klick als Kantenende bzw. Kreismitte gilt.
pub const END_PX: f64 = 8.0;

/// Ablehnung: der Bezug lässt sich nicht bemaßen.
pub const NOT_DIMENSIONABLE: &str = "Der Bezug lässt sich nicht bemaßen.";
/// Ablehnung in der Isometrie.
pub const NO_ISOMETRIC: &str = "In der Isometrie gibt es keine Maße.";
/// Ablehnung beim Winkelmaß mit einer gebogenen Kante.
pub const ANGLE_LINES: &str = "Ein Winkelmaß braucht zwei gerade Kanten.";
/// Hinweis nach dem Klick auf den Umriss eines Zylinders in einem linearen Maß.
pub const SILHOUETTE_NOTE: &str = "Gemessen wird bis zur Mitte des Zylinders.";
/// Erklärung, wenn die Zeichnung noch keine Ansicht hat.
pub const NO_VIEWS_TIP: &str = "Zuerst eine Ansicht anlegen.";
/// Hinweis bei einem Klick neben die Linien.
pub const MISSED: &str = "Bitte eine Linie der Ansicht anklicken.";
/// Beschriftung des Schalters.
pub const TOOL: &str = "Maß";
/// Tooltip des Schalters.
pub const TOOL_TIP: &str =
    "Maße von Hand setzen: Linien oder Ecken anklicken, dann die Maßlinie mit einem Klick ablegen.";
/// Erklärung, wenn der Schalter in einer alten Version gesperrt ist.
pub const READ_ONLY_TIP: &str = "In einer alten Version lassen sich keine Maße setzen.";
/// Erklärung, wenn die Zeichnung nur eine Isometrie (oder keine Ansicht) hat.
pub const ISOMETRIC_TIP: &str =
    "In der Isometrie gibt es keine Maße. Fügen Sie eine Vorder-, Seiten- oder Draufsicht hinzu.";

/// Messrichtung eines linearen Maßes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinearDir {
    Horizontal,
    Vertical,
    Aligned,
}

/// Untermodus des Werkzeugs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ToolMode {
    #[default]
    Off,
    Linear(LinearDir),
    Diameter,
    Radius,
    Angle,
    Chain,
    Baseline,
}

impl ToolMode {
    /// Die wählbaren Untermodi in der Reihenfolge der Auswahlliste.
    pub const CHOICES: [ToolMode; 8] = [
        ToolMode::Linear(LinearDir::Horizontal),
        ToolMode::Linear(LinearDir::Vertical),
        ToolMode::Linear(LinearDir::Aligned),
        ToolMode::Diameter,
        ToolMode::Radius,
        ToolMode::Angle,
        ToolMode::Chain,
        ToolMode::Baseline,
    ];

    /// Bezeichnung in der Oberfläche.
    pub fn label(self) -> &'static str {
        match self {
            ToolMode::Off => "Aus",
            ToolMode::Linear(LinearDir::Horizontal) => "Waagerecht",
            ToolMode::Linear(LinearDir::Vertical) => "Senkrecht",
            ToolMode::Linear(LinearDir::Aligned) => "Schräg",
            ToolMode::Diameter => "Durchmesser",
            ToolMode::Radius => "Radius",
            ToolMode::Angle => "Winkel",
            ToolMode::Chain => "Kette",
            ToolMode::Baseline => "Bezugsmaße",
        }
    }

    /// Zahl der Bezüge, ab der die Maßlinie gesetzt werden kann.
    fn needed(self) -> usize {
        match self {
            ToolMode::Off => usize::MAX,
            ToolMode::Diameter | ToolMode::Radius => 1,
            _ => 2,
        }
    }

    /// Kette und Bezugsmaße nehmen weitere Punkte an.
    fn collects(self) -> bool {
        matches!(self, ToolMode::Chain | ToolMode::Baseline)
    }

    /// Bezüge immer als ganze Kante (keine Enden, keine Mitte).
    fn whole_edges(self) -> bool {
        matches!(
            self,
            ToolMode::Diameter | ToolMode::Radius | ToolMode::Angle
        )
    }
}

/// Getroffene Blattlinie: Ursprung, Klickpunkt auf dem Blatt (mm) und Pixel je mm (für die
/// Schwelle [`END_PX`]).
#[derive(Clone, Debug, PartialEq)]
pub struct Hit {
    pub origin: StrokeOrigin,
    pub at: DVec2,
    pub px_per_mm: f64,
}

/// Ergebnis eines Klicks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ToolOutcome {
    /// Der Bezug ist gemerkt; es fehlt noch etwas (Hinweis als Satz).
    NeedMore(String),
    /// Genug Bezüge: die Maßlinie folgt dem Zeiger, der nächste Klick setzt das Maß.
    Ready,
    /// Abgelehnt (Grund als Satz); nichts wurde gemerkt oder geändert.
    Rejected(String),
}

/// Fehler beim Setzen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ToolError {
    /// Es fehlen noch Bezüge.
    NotReady,
    /// Abgelehnt (Grund als Satz); das Dokument ist unverändert.
    Rejected(String),
}

impl std::fmt::Display for ToolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ToolError::NotReady => f.write_str("Es sind noch nicht genug Linien gewählt."),
            ToolError::Rejected(t) => f.write_str(t),
        }
    }
}

impl std::error::Error for ToolError {}

/// Zustand des Werkzeugs „Maß“.
#[derive(Clone, Debug, Default)]
pub struct DimensionTool {
    /// Aktueller Untermodus (`Off`: Werkzeug aus).
    pub mode: ToolMode,
    /// Zuletzt gewählter Untermodus (zum Wiedereinschalten).
    choice: Option<ToolMode>,
    drawing: Option<DrawingId>,
    view: Option<ViewId>,
    pending: Vec<GeoRef>,
    /// Messrichtung einer Kette bzw. der Bezugsmaße (aus den ersten beiden Punkten).
    chain_kind: Option<DimKind>,
    /// Entwurf der letzten Vorschau.
    preview: Vec<DimSpec>,
    /// Gruppennummer der zuletzt gesetzten Kette bzw. Bezugsmaße.
    group: Option<u64>,
    pointer: Option<DVec2>,
    message: Option<String>,
    /// Hinweis zum zuletzt gewählten Bezug (keine Ablehnung).
    notice: Option<String>,
}

/// Alles, was zum Messen in einer Ansicht gebraucht wird.
struct Ctx<'a> {
    drawing: Drawing,
    view: ViewSpec,
    basis: Basis,
    xf: SheetTransform,
    solids: Vec<&'a Solid>,
    extent: f64,
    tol: Tolerance,
    results: BTreeMap<ViewId, std::sync::Arc<ViewLines>>,
    /// Prüfpunkte der Flächen für diesen Klick bzw. dieses Bild (jeder Bezug an derselben
    /// Fläche rechnet sie sonst neu).
    cache: RefCache<'a>,
}

impl<'a> Ctx<'a> {
    fn new(state: &'a AppState, drawing: DrawingId, view: ViewId) -> Result<Ctx<'a>, String> {
        let d = state.document.drawing(drawing).ok_or(NO_DRAWING)?;
        let v = d.view(view).ok_or(NO_VIEW)?.clone();
        if v.kind == ViewKind::Isometric {
            return Err(NO_ISOMETRIC.into());
        }
        if state.bodies.is_empty() {
            return Err(DrawError::NoBodies.to_string());
        }
        let model = state.document.model_edit_count();
        let solids: Vec<&Solid> = state
            .bodies
            .iter()
            .filter(|b| b.kind != BodyKind::Mesh)
            .map(|b| &b.solid)
            .collect();
        if solids.is_empty() {
            return Err(DrawError::MeshBodiesOnly.to_string());
        }
        // Eine Schnittansicht misst an ihren geschnittenen Körpern (Spec §9.3); die Herkunft
        // ihrer Linien zählt in diesen Körpern.
        let solids: Vec<&Solid> = if v.section.is_some() {
            let data = state
                .drawing_ui
                .section_ref(&v, model)
                .ok_or(VIEWS_PENDING)?;
            data.solids.iter().collect()
        } else {
            solids
        };
        let results = state.drawing_ui.results_for(d, model);
        let refs: BTreeMap<ViewId, &ViewLines> =
            results.iter().map(|(k, l)| (*k, l.as_ref())).collect();
        let xf = view_transform(d, view, &refs).ok_or(VIEWS_PENDING)?;
        let draw: Vec<DrawBody> = solids
            .iter()
            .map(|s| DrawBody {
                name: String::new(),
                solid: s,
            })
            .collect();
        let extent = extent_of(&draw);
        Ok(Ctx {
            drawing: d.clone(),
            basis: Basis::of(v.kind),
            view: v,
            xf,
            solids,
            extent,
            tol: Tolerance::default(),
            results,
            cache: RefCache::new(),
        })
    }

    /// Lage eines Raumpunkts auf dem Blatt.
    fn sheet(&self, p: DVec3) -> DVec2 {
        self.xf.apply(self.basis.project(p))
    }

    /// Misst ein Maß an den Bezügen; Fehler als Satz für die Oberfläche.
    fn measure(&self, kind: DimKind, refs: &[GeoRef]) -> Result<Measured, String> {
        let geos = refs
            .iter()
            .map(|r| resolve_geo_cached(&self.solids, r, &self.tol, &self.cache))
            .collect::<Result<Vec<Geo3>, _>>()
            .map_err(|_| NOT_DIMENSIONABLE.to_string())?;
        measure(kind, &geos, &self.basis, self.view.kind, self.extent).map_err(|e| dim_text(&e))
    }

    /// Bezug aus der getroffenen Linie (siehe Modulkommentar).
    fn reference(&self, hit: &Hit, whole: bool) -> Result<GeoRef, String> {
        let origin = match hit.origin {
            StrokeOrigin::Silhouette { body, face } => CandidateOrigin::Face { body, face },
            StrokeOrigin::Edge { body, edge } => {
                let solid = self.solids.get(body).ok_or(NOT_DIMENSIONABLE)?;
                let e = solid
                    .edges
                    .get(Id::<Edge>::from_index(edge))
                    .ok_or(NOT_DIMENSIONABLE)?;
                if whole {
                    CandidateOrigin::Edge { body, edge }
                } else {
                    self.edge_part(solid, e, body, edge, hit)
                }
            }
        };
        geo_ref(&self.solids, origin, &self.tol, &self.cache)
            .ok_or_else(|| NOT_DIMENSIONABLE.to_string())
    }

    /// Kante, Kantenende (bis [`END_PX`] an einem Ende) oder Kreismitte (bis [`END_PX`] an der
    /// Mitte).
    fn edge_part(
        &self,
        solid: &Solid,
        e: &Edge,
        body: usize,
        edge: usize,
        hit: &Hit,
    ) -> CandidateOrigin {
        let near = if hit.px_per_mm.is_finite() && hit.px_per_mm > 0.0 {
            END_PX / hit.px_per_mm
        } else {
            0.0
        };
        if let Curve::Circle { frame, .. } = &e.curve
            && hit.at.distance(self.sheet(frame.origin)) <= near
        {
            return CandidateOrigin::Center { body, edge };
        }
        if e.start != e.end {
            let at = |v| solid.vertices.get(v).map(|x| self.sheet(x.point));
            if let (Some(a), Some(b)) = (at(e.start), at(e.end)) {
                let (da, db) = (hit.at.distance(a), hit.at.distance(b));
                if da.min(db) <= near {
                    let end = if da <= db { EndSel::Start } else { EndSel::End };
                    return CandidateOrigin::EdgeEnd { body, edge, end };
                }
            }
        }
        CandidateOrigin::Edge { body, edge }
    }
}

/// Ein Text als ganzer Satz: endet er nicht mit einem Satzzeichen, kommt ein Punkt dazu.
pub fn sentence(text: &str) -> String {
    let t = text.trim_end();
    if t.is_empty() || t.ends_with(['.', '!', '?']) {
        t.to_string()
    } else {
        format!("{t}.")
    }
}

/// Meldung, wenn eine Kette oder Bezugsmaße nur zum Teil gesetzt werden konnten.
pub fn incomplete_text(set: usize, total: usize) -> String {
    format!("Die Maße sind unvollständig: nur {set} von {total} Maßen wurden gesetzt.")
}

/// Fehler einer Messung als Satz für die Oberfläche (ohne Hinweise auf die Herkunft).
fn dim_text(e: &DimError) -> String {
    match e {
        DimError::Invalid(t) => t.clone(),
        DimError::Lost(_) | DimError::Ambiguous => NOT_DIMENSIONABLE.into(),
        e => e.to_string(),
    }
}

fn linear_kind(dir: LinearDir) -> DimKind {
    match dir {
        LinearDir::Horizontal => DimKind::Horizontal,
        LinearDir::Vertical => DimKind::Vertical,
        LinearDir::Aligned => DimKind::Aligned,
    }
}

impl DimensionTool {
    /// Ist das Werkzeug eingeschaltet?
    pub fn is_active(&self) -> bool {
        self.mode != ToolMode::Off
    }

    /// Zuletzt gewählter Untermodus (Vorgabe: waagerecht).
    pub fn choice(&self) -> ToolMode {
        self.choice
            .unwrap_or(ToolMode::Linear(LinearDir::Horizontal))
    }

    /// Setzt den Untermodus (`Off` schaltet aus); gewählte Bezüge verfallen.
    pub fn set_mode(&mut self, mode: ToolMode) {
        self.cancel();
        self.mode = mode;
        if mode != ToolMode::Off {
            self.choice = Some(mode);
        }
    }

    /// Schaltet das Werkzeug ein (mit dem zuletzt gewählten Untermodus) oder aus.
    pub fn toggle(&mut self) {
        if self.is_active() {
            self.set_mode(ToolMode::Off);
        } else {
            self.set_mode(self.choice());
        }
    }

    /// Die bisher gewählten Bezüge.
    pub fn pending(&self) -> &[GeoRef] {
        &self.pending
    }

    /// Ansicht der gewählten Bezüge.
    pub fn view(&self) -> Option<ViewId> {
        self.view
    }

    /// Entwurf der letzten Vorschau.
    pub fn preview(&self) -> &[DimSpec] {
        &self.preview
    }

    /// Gruppennummer der zuletzt gesetzten Kette bzw. Bezugsmaße.
    pub fn last_group(&self) -> Option<u64> {
        self.group
    }

    /// Letzte Zeigerlage auf dem Blatt (aus [`DimensionTool::hover`]).
    pub fn last_pointer(&self) -> Option<DVec2> {
        self.pointer
    }

    /// Letzte Ablehnung (bis zum nächsten Klick).
    pub fn message(&self) -> Option<&str> {
        self.message.as_deref()
    }

    /// Setzt die Meldung für die Hinweiszeile (etwa bei einem Klick neben die Linien).
    pub fn set_message(&mut self, message: Option<String>) {
        self.message = message;
    }

    /// Genug Bezüge für das Setzen?
    pub fn is_ready(&self) -> bool {
        self.mode != ToolMode::Off && self.pending.len() >= self.mode.needed()
    }

    /// Nimmt der Untermodus weitere Punkte an (Kette, Bezugsmaße)?
    pub fn collects_more(&self) -> bool {
        self.mode.collects()
    }

    /// Bricht ab: gewählte Bezüge und Vorschau verfallen, das Dokument bleibt unverändert.
    pub fn cancel(&mut self) {
        self.pending.clear();
        self.chain_kind = None;
        self.preview.clear();
        self.view = None;
        self.drawing = None;
        self.message = None;
        self.notice = None;
    }

    /// Hinweis zum zuletzt gewählten Bezug (etwa: Zylinderumriss misst bis zur Mitte).
    pub fn notice(&self) -> Option<&str> {
        self.notice.as_deref()
    }

    /// Hinweis zum nächsten Schritt; `None`, wenn das Werkzeug aus ist.
    pub fn hint(&self) -> Option<String> {
        let n = self.pending.len();
        let text = match self.mode {
            ToolMode::Off => return None,
            ToolMode::Diameter | ToolMode::Radius if n == 0 => {
                "Kreis, Bogen oder Zylinderrand anklicken."
            }
            ToolMode::Angle if n == 0 => "Erste gerade Kante anklicken.",
            ToolMode::Angle if n == 1 => "Zweite gerade Kante anklicken.",
            ToolMode::Linear(_) if n == 0 => "Erste Kante oder Ecke anklicken.",
            ToolMode::Linear(_) if n == 1 => "Zweite Kante oder Ecke anklicken.",
            ToolMode::Chain if n == 0 => "Ersten Punkt der Kette anklicken.",
            ToolMode::Chain if n == 1 => "Nächsten Punkt der Kette anklicken.",
            ToolMode::Baseline if n == 0 => "Bezugskante anklicken.",
            ToolMode::Baseline if n == 1 => "Erstes Ziel anklicken.",
            ToolMode::Chain | ToolMode::Baseline => {
                "Weitere Punkte anklicken; Klick ins Leere, Doppelklick oder Esc setzt die Maße."
            }
            _ => "Klicken legt die Maßlinie ab. Esc bricht ab.",
        };
        Some(text.into())
    }

    /// Klick auf eine Linie der Ansicht `view`: merkt den Bezug oder lehnt ihn ab (das Dokument
    /// bleibt in jedem Fall unverändert). Ein Klick in eine andere Ansicht beginnt neu.
    pub fn click(
        &mut self,
        state: &AppState,
        drawing: DrawingId,
        view: ViewId,
        hit: &Hit,
    ) -> ToolOutcome {
        let out = self.click_inner(state, drawing, view, hit);
        let out = match out {
            ToolOutcome::Rejected(t) => ToolOutcome::Rejected(sentence(&t)),
            o => o,
        };
        self.message = match &out {
            ToolOutcome::Rejected(t) => Some(t.clone()),
            _ => None,
        };
        // Umriss eines Zylinders in einem linearen Maß: gemessen wird bis zur Achse.
        self.notice = match (&out, self.pending.last()) {
            (ToolOutcome::NeedMore(_) | ToolOutcome::Ready, Some(GeoRef::Face(_)))
                if !self.mode.whole_edges() =>
            {
                Some(SILHOUETTE_NOTE.into())
            }
            _ => None,
        };
        out
    }

    fn click_inner(
        &mut self,
        state: &AppState,
        drawing: DrawingId,
        view: ViewId,
        hit: &Hit,
    ) -> ToolOutcome {
        use ToolOutcome::{NeedMore, Ready, Rejected};
        if self.mode == ToolMode::Off {
            return Rejected("Bitte zuerst eine Maßart wählen.".into());
        }
        let ctx = match Ctx::new(state, drawing, view) {
            Ok(c) => c,
            Err(t) => return Rejected(t),
        };
        if self.drawing != Some(drawing) || self.view != Some(view) {
            let mode = self.mode;
            self.cancel();
            self.mode = mode;
            self.drawing = Some(drawing);
            self.view = Some(view);
        }
        if self.is_ready() && !self.mode.collects() {
            return Ready;
        }
        let r = match ctx.reference(hit, self.mode.whole_edges()) {
            Ok(r) => r,
            Err(t) => return Rejected(t),
        };
        let geo = match resolve_geo_cached(&ctx.solids, &r, &ctx.tol, &ctx.cache) {
            Ok(g) => g,
            Err(_) => return Rejected(NOT_DIMENSIONABLE.into()),
        };
        let first = self.pending.first().cloned();
        let last = self.pending.last().cloned();
        match self.mode {
            ToolMode::Off => Rejected("Bitte zuerst eine Maßart wählen.".into()),
            ToolMode::Diameter | ToolMode::Radius => {
                let kind = if self.mode == ToolMode::Diameter {
                    DimKind::Diameter
                } else {
                    DimKind::Radius
                };
                match ctx.measure(kind, std::slice::from_ref(&r)) {
                    Ok(_) => {
                        self.pending.push(r);
                        Ready
                    }
                    Err(t) => Rejected(t),
                }
            }
            ToolMode::Angle | ToolMode::Linear(_) => {
                if self.mode == ToolMode::Angle && !matches!(geo, Geo3::Line { .. }) {
                    return Rejected(ANGLE_LINES.into());
                }
                let Some(a) = first else {
                    self.pending.push(r);
                    return NeedMore(self.hint().unwrap_or_default());
                };
                let kind = match self.mode {
                    ToolMode::Linear(dir) => linear_kind(dir),
                    _ => DimKind::Angle,
                };
                match ctx.measure(kind, &[a, r.clone()]) {
                    Ok(_) => {
                        self.pending.push(r);
                        Ready
                    }
                    Err(t) => Rejected(t),
                }
            }
            ToolMode::Chain | ToolMode::Baseline => {
                let prev = if self.mode == ToolMode::Chain {
                    last
                } else {
                    first
                };
                let Some(prev) = prev else {
                    self.pending.push(r);
                    return NeedMore(self.hint().unwrap_or_default());
                };
                let pair = [prev, r.clone()];
                let kind = match self.chain_kind {
                    Some(k) => ctx.measure(k, &pair).map(|_| k),
                    None => chain_direction(&ctx, &pair),
                };
                match kind {
                    Ok(k) => {
                        self.chain_kind = Some(k);
                        self.pending.push(r);
                        Ready
                    }
                    Err(t) => Rejected(t),
                }
            }
        }
    }

    /// Vorschau der Maßlinie(n) am Zeiger `pointer` (Blatt-mm); `None`, solange Bezüge fehlen
    /// oder sich nichts messen lässt. Merkt die Zeigerlage (für Esc in Kette und Bezugsmaßen).
    pub fn hover(
        &mut self,
        state: &AppState,
        drawing: DrawingId,
        pointer: DVec2,
    ) -> Option<Vec<Mark>> {
        if !pointer.is_finite() {
            return None;
        }
        self.pointer = Some(pointer);
        let (ctx, drafts) = match self.drafts(state, drawing, pointer) {
            Ok(x) => x,
            Err(_) => {
                self.preview.clear();
                return None;
            }
        };
        self.preview = drafts.iter().map(|(s, _)| s.clone()).collect();
        Some(preview_marks(&ctx, drafts))
    }

    /// Setzt das Maß (bei Kette und Bezugsmaßen alle Maße der Gruppe) mit der Lage aus dem
    /// Zeiger `pointer` (Blatt-mm). Jedes Maß ist eine Änderung des Dokuments; bei einem Fehler
    /// bleibt das Dokument unverändert. Danach sind keine Bezüge mehr gewählt (der Untermodus
    /// bleibt).
    pub fn finish(
        &mut self,
        state: &mut AppState,
        drawing: DrawingId,
        pointer: DVec2,
    ) -> Result<Vec<DimId>, ToolError> {
        let r = match self.finish_inner(state, drawing, pointer) {
            Err(ToolError::Rejected(t)) => Err(ToolError::Rejected(sentence(&t))),
            r => r,
        };
        self.message = match &r {
            Err(ToolError::Rejected(t)) => Some(t.clone()),
            Ok((ids, total)) if ids.len() < *total => Some(incomplete_text(ids.len(), *total)),
            _ => None,
        };
        r.map(|(ids, _)| ids)
    }

    fn finish_inner(
        &mut self,
        state: &mut AppState,
        drawing: DrawingId,
        pointer: DVec2,
    ) -> Result<(Vec<DimId>, usize), ToolError> {
        if !pointer.is_finite() {
            return Err(ToolError::NotReady);
        }
        let specs: Vec<DimSpec> = {
            let (_, drafts) = self.drafts(state, drawing, pointer)?;
            drafts.into_iter().map(|(s, _)| s).collect()
        };
        let d = state
            .document
            .drawing(drawing)
            .ok_or_else(|| ToolError::Rejected(NO_DRAWING.into()))?;
        if d.dimensions.len() + specs.len() > MAX_DIMENSIONS {
            return Err(ToolError::Rejected(full_text()));
        }
        let group = if self.mode.collects() {
            Some(next_group(d).ok_or_else(|| {
                ToolError::Rejected("Die Zeichnung hat keine freie Gruppennummer mehr.".into())
            })?)
        } else {
            None
        };
        let total = specs.len();
        let mut ids = Vec::with_capacity(total);
        for mut s in specs {
            if let (Some(c), Some(g)) = (s.chain.as_mut(), group) {
                c.group = g;
            }
            match state.document.add_dimension(drawing, s) {
                Ok(id) => ids.push(id),
                Err(e) => {
                    // Nur das erste Maß kann scheitern, ohne etwas zu ändern; alle Maße einer
                    // Gruppe sind gleich gebaut.
                    if ids.is_empty() {
                        return Err(ToolError::Rejected(e.to_string()));
                    }
                    break;
                }
            }
        }
        self.group = group;
        let mode = self.mode;
        self.cancel();
        self.mode = mode;
        Ok((ids, total))
    }

    /// Die Maße, die ein Klick an `pointer` setzen würde, mit ihren Messungen.
    fn drafts<'a>(
        &self,
        state: &'a AppState,
        drawing: DrawingId,
        pointer: DVec2,
    ) -> Result<(Ctx<'a>, Vec<(DimSpec, Measured)>), ToolError> {
        let view = self.view.ok_or(ToolError::NotReady)?;
        if !self.is_ready() || self.drawing != Some(drawing) {
            return Err(ToolError::NotReady);
        }
        let ctx = Ctx::new(state, drawing, view).map_err(ToolError::Rejected)?;
        let p = &self.pending;
        let chain_kind = self.chain_kind.unwrap_or(DimKind::Horizontal);
        let pairs: Vec<(DimKind, Vec<GeoRef>)> = match self.mode {
            ToolMode::Off => return Err(ToolError::NotReady),
            ToolMode::Linear(dir) => vec![(linear_kind(dir), p[..2].to_vec())],
            ToolMode::Diameter => vec![(DimKind::Diameter, p[..1].to_vec())],
            ToolMode::Radius => vec![(DimKind::Radius, p[..1].to_vec())],
            ToolMode::Angle => vec![(DimKind::Angle, p[..2].to_vec())],
            ToolMode::Chain => p.windows(2).map(|w| (chain_kind, w.to_vec())).collect(),
            ToolMode::Baseline => p[1..]
                .iter()
                .map(|t| (chain_kind, vec![p[0].clone(), t.clone()]))
                .collect(),
        };
        let measured: Vec<(DimKind, Vec<GeoRef>, Measured)> = pairs
            .into_iter()
            .map(|(k, refs)| {
                ctx.measure(k, &refs)
                    .map(|m| (k, refs, m))
                    .map_err(ToolError::Rejected)
            })
            .collect::<Result<_, _>>()?;
        let (offset, angle) = placement(&ctx, &measured, pointer);
        let chain_mode = match self.mode {
            ToolMode::Chain => Some(ChainMode::Chain),
            ToolMode::Baseline => Some(ChainMode::Baseline),
            _ => None,
        };
        let drafts = measured
            .into_iter()
            .enumerate()
            .map(|(i, (kind, refs, m))| {
                let spec = DimSpec {
                    id: DimId(0),
                    view,
                    kind,
                    refs,
                    offset,
                    angle,
                    text_shift: (0.0, 0.0),
                    tolerance: DimTolerance::None,
                    decimals: 1,
                    text_override: None,
                    chain: chain_mode.map(|mode| ChainSpec {
                        group: 0,
                        mode,
                        order: i as u32,
                    }),
                    suggested: false,
                };
                (spec, m)
            })
            .collect();
        Ok((ctx, drafts))
    }
}

/// Messrichtung einer Kette aus den ersten beiden Punkten: waagerecht oder senkrecht, je
/// nachdem, was sich messen lässt; geht beides, die Richtung mit dem größeren Wert.
fn chain_direction(ctx: &Ctx, pair: &[GeoRef]) -> Result<DimKind, String> {
    let h = ctx.measure(DimKind::Horizontal, pair);
    let v = ctx.measure(DimKind::Vertical, pair);
    match (h, v) {
        (Ok(a), Ok(b)) => Ok(if b.value > a.value {
            DimKind::Vertical
        } else {
            DimKind::Horizontal
        }),
        (Ok(_), Err(_)) => Ok(DimKind::Horizontal),
        (Err(_), Ok(_)) => Ok(DimKind::Vertical),
        (Err(t), Err(_)) => Err(t),
    }
}

/// `offset` und `angle` aus der Zeigerlage (Regeln im Modulkommentar).
fn placement(
    ctx: &Ctx,
    measured: &[(DimKind, Vec<GeoRef>, Measured)],
    pointer: DVec2,
) -> (f64, f64) {
    const DEFAULT_ANGLE: f64 = 45.0;
    let Some((_, _, first)) = measured.first() else {
        return (DEFAULT_OFFSET, DEFAULT_ANGLE);
    };
    match &first.anchor {
        Anchor::Linear { .. } => {
            let ms: Vec<&Measured> = measured.iter().map(|(_, _, m)| m).collect();
            let Some(base) = group_base(&ms, &ctx.xf) else {
                return (DEFAULT_OFFSET, DEFAULT_ANGLE);
            };
            let c = pointer.dot(base.w);
            let offset = if c >= 0.5 * (base.min + base.max) {
                (c - base.max).max(MIN_DRAG_OFFSET_MM)
            } else {
                (c - base.min).min(-MIN_DRAG_OFFSET_MM)
            };
            (offset, DEFAULT_ANGLE)
        }
        Anchor::Round {
            center,
            radius,
            arc,
        } => {
            let c = ctx.xf.apply(*center);
            let v = pointer - c;
            let len = v.length();
            // Die Ø-Linie durch die Mitte (großer Ø) dreht frei; eine Führungslinie bleibt auf
            // dem Bogen.
            let line = first.prefix == Some('Ø') && 2.0 * radius * ctx.xf.scale >= LARGE_DIAMETER;
            let arc = if line { None } else { arc.as_ref() };
            if len.is_finite() && len > 1e-9 {
                (
                    len - radius * ctx.xf.scale,
                    normalize_deg(clamp_leader_deg(arc, v.y.atan2(v.x).to_degrees())),
                )
            } else {
                (DEFAULT_OFFSET, DEFAULT_ANGLE)
            }
        }
        Anchor::Angle { vertex, .. } => {
            let r = pointer.distance(ctx.xf.apply(*vertex));
            let r = if r.is_finite() { r } else { DEFAULT_OFFSET };
            (r.max(MIN_DRAG_OFFSET_MM), DEFAULT_ANGLE)
        }
    }
}

/// Markierungen der Entwürfe mit demselben Layout wie die gesetzten Maße.
fn preview_marks(ctx: &Ctx, drafts: Vec<(DimSpec, Measured)>) -> Vec<Mark> {
    let refs: BTreeMap<ViewId, &ViewLines> =
        ctx.results.iter().map(|(k, l)| (*k, l.as_ref())).collect();
    let mut rd = ResolvedDims::default();
    rd.by_view.insert(
        ctx.view.id,
        drafts.into_iter().map(|(s, m)| (s, Ok(m))).collect(),
    );
    dimension_marks(&ctx.drawing, &refs, &rd)
        .into_iter()
        .flat_map(|dm| dm.marks)
        .collect()
}

/// Führt `f` mit dem Werkzeug aus `state.dimension_tool` aus (herausgenommen, damit `f` auch
/// den Zustand ändern kann).
pub fn with_tool<R>(
    state: &mut AppState,
    f: impl FnOnce(&mut DimensionTool, &mut AppState) -> R,
) -> R {
    let mut tool = std::mem::take(&mut state.dimension_tool);
    let r = f(&mut tool, state);
    state.dimension_tool = tool;
    r
}
