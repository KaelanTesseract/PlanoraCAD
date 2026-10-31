//! Maße der App (Teilprojekt 12, Aufgabe 10): Auflösen der Maße einer Zeichnung gegen die
//! exakten Körper mit Zwischenspeicher, letzte bekannte Lage verlorener Maße, Hinweise
//! (verloren, außerhalb des Blatts) und der Vorschlag der Grundmaße.
//!
//! Der Zwischenspeicher hängt am Stand `(Zeichnung, Document::model_edit_count(),
//! Document::edit_count())`. Ändert sich nur die Zeichnung (Lage eines Maßes, Text, Ansicht
//! verschoben) und bleiben Modell, Ansichtsarten und alle Bezüge gleich, werden die Messungen
//! übernommen statt neu gerechnet. Die Linien der Ansichten rechnet hier nichts: die kommen aus
//! [`crate::drawing_state::DrawingUi`].

use crate::drawing_state::{ViewResult, draw_error_msg};
use crate::msg::Msg;
use crate::state::{AppState, Body, BodyKind};
use planora_drawing::annotate::{
    ARROW_HALF_W, ARROW_LEN, DEFAULT_OFFSET, DIM_STEP, Mark, ResolvedDims, TextAnchor,
    default_decimals, text_width,
};
use planora_drawing::dimension::{
    ChainSpec, DimId, DimKind, DimSpec, Geo3, GeoRef, Tolerance as DimTolerance,
};
use planora_drawing::limits::MAX_DIMENSIONS;
use planora_drawing::sheet::{DimMarks, dimension_marks};
use planora_drawing::stroke::extent_of;
use planora_drawing::{
    Anchor, Basis, Candidate, CandidateOrigin, DrawBody, DrawError, Drawing, DrawingId,
    SectionData, ViewId, ViewKind, ViewLines, ViewSpec, dimension_key, measure, suggest,
};
use planora_feature::{
    RefCache, geo_ref_for_edge_cached, geo_ref_for_face_cached, resolve_drawing_dimensions_views,
    resolve_geo_cached,
};
use planora_kernel::{Edge, Face, Id, Solid};
use planora_math::{DVec2, Tolerance};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::convert::Infallible;
use std::sync::Arc;

/// Meldung, solange die Ansichten für den Vorschlag noch nicht fertig sind.
pub fn views_pending() -> Msg {
    crate::msg!("dim-views-pending")
}
/// Meldung, wenn es die Zeichnung nicht (mehr) gibt.
pub fn no_drawing() -> Msg {
    crate::msg!("dim-no-drawing")
}
/// Meldung, wenn es die gewählte Ansicht nicht (mehr) gibt.
pub fn no_view() -> Msg {
    crate::msg!("dim-no-view")
}
/// Meldung für die isometrische Ansicht.
pub fn no_isometric() -> Msg {
    crate::msg!("dim-no-isometric")
}
/// Meldung, wenn die Zeichnung keine Ansicht hat, in der sich Maße vorschlagen lassen.
pub fn no_normal_view() -> Msg {
    crate::msg!("dim-no-normal-view")
}
/// Meldung, wenn ein vorgeschlagenes Maß nicht sicher am Modell festzumachen ist.
pub fn not_bound() -> Msg {
    crate::msg!("dim-not-bound")
}
/// Toleranz (Blatt-mm), um die eine Markierung über den Blattrand ragen darf, ohne als
/// „außerhalb“ zu gelten (Rundung).
const SHEET_EPS: f64 = 1e-9;
/// Stützpunkte eines Bogens für die Prüfung „außerhalb des Blatts“.
const ARC_SAMPLES: usize = 32;

/// Meldung, wenn die Zeichnung schon die Höchstzahl an Maßen hat.
pub fn full_text() -> Msg {
    crate::msg!("dim-full", max = MAX_DIMENSIONS)
}

/// Hinweise zu den Maßen einer Zeichnung.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DimNotes {
    /// Maße, deren Bezug verloren ist (rot gezeichnet).
    pub lost: usize,
    /// Maße, die ganz oder teilweise außerhalb des Blatts liegen (trotzdem gezeichnet).
    pub outside: usize,
    /// Die Hinweise als Sätze (leer, wenn alles in Ordnung ist); übersetzt erst beim Zeichnen.
    pub texts: Vec<Msg>,
}

/// Ergebnis von [`suggest_for`].
#[derive(Clone, Debug, PartialEq)]
pub enum SuggestOutcome {
    /// So viele Maße wurden hinzugefügt (alle Vorschläge).
    Added(usize),
    /// Ein Teil wurde hinzugefügt, der Rest ausgelassen; je ausgelassenem Grund ein Satz.
    Partly { added: usize, skipped: Vec<Msg> },
    /// Es gibt nichts Neues vorzuschlagen.
    NothingNew,
    /// Kein Vorschlag möglich; der Grund als Satz.
    Unavailable(Msg),
}

impl SuggestOutcome {
    /// Meldung für die Oberfläche.
    pub fn message(&self) -> Msg {
        match self {
            SuggestOutcome::Added(n) => crate::msg!("dim-added", n = *n),
            SuggestOutcome::Partly { added, skipped } => {
                let mut list = vec![crate::msg!("dim-added", n = *added)];
                list.extend(skipped.iter().cloned());
                Msg::Join(list)
            }
            SuggestOutcome::NothingNew => crate::msg!("dim-none-new"),
            SuggestOutcome::Unavailable(t) => t.clone(),
        }
    }
}

/// Zwischenspeicher eines Auflösungsstands.
struct Cached {
    drawing: DrawingId,
    model: u64,
    edit: u64,
    /// Ansichten und ihre Arten zu diesem Stand (eine andere Art misst anders).
    views: Vec<(ViewId, ViewKind)>,
    /// Die fertigen Schnitte, mit denen aufgelöst wurde (festgehalten, damit der Vergleich über
    /// die Adresse eindeutig bleibt): Wird ein Schnitt fertig oder neu gerechnet, wird neu
    /// aufgelöst.
    sections: Sections,
    dims: Arc<ResolvedDims>,
}

/// Fertige Schnitte je Schnittansicht.
type Sections = Vec<(ViewId, Arc<SectionData>)>;

/// Dieselben fertigen Schnitte (gleiche Ansichten, dieselben Rechenergebnisse)?
fn same_sections(a: &[(ViewId, Arc<SectionData>)], b: &[(ViewId, Arc<SectionData>)]) -> bool {
    a.len() == b.len()
        && a.iter()
            .zip(b)
            .all(|((va, da), (vb, db))| va == vb && Arc::ptr_eq(da, db))
}

/// Zwischenspeicher der Markierungen je Maß und der Hinweise: gilt für genau ein
/// `Arc<ResolvedDims>` (festgehalten, damit die Adresse eindeutig bleibt) und dieselben fertigen
/// Ansichtsergebnisse (Adressen der `Arc<ViewLines>`).
struct MarksCached {
    drawing: DrawingId,
    dims: Arc<ResolvedDims>,
    results: Vec<(ViewId, usize)>,
    marks: Arc<Vec<DimMarks>>,
    notes: DimNotes,
}

/// Aufgelöste Maße mit Zwischenspeicher und den letzten bekannten Lagen.
#[derive(Default)]
pub struct DimensionUi {
    cache: Option<Cached>,
    marks_cache: Option<MarksCached>,
    /// Zahl der Neuberechnungen der Markierungen je Maß (für Tests).
    mark_builds: u64,
    /// Letzte erfolgreiche Lage je Maß und Zeichnung; verlorene Maße erscheinen dort rot.
    last_known: BTreeMap<DrawingId, BTreeMap<DimId, Anchor>>,
    /// Letzter geometrischer Schlüssel (`dimension_key`) je Maß und Zeichnung, solange sich die
    /// Bezüge noch auflösen ließen. Verliert ein Maß seinen Bezug, erkennt der Vorschlag es
    /// darüber weiter und schlägt dasselbe Maß nicht noch einmal vor.
    last_keys: BTreeMap<DrawingId, BTreeMap<DimId, String>>,
    /// Zahl der echten Auflösungen gegen die Körper (für Tests).
    resolutions: u64,
}

impl std::fmt::Debug for DimensionUi {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DimensionUi")
            .field(
                "cached",
                &self.cache.as_ref().map(|c| (c.drawing, c.model, c.edit)),
            )
            .field("resolutions", &self.resolutions)
            .finish()
    }
}

/// Exakte Körper in der Reihenfolge, in der auch die Ansichten berechnet werden.
fn exact_bodies(bodies: &[Body]) -> Vec<&Body> {
    bodies.iter().filter(|b| b.kind != BodyKind::Mesh).collect()
}

/// Übernimmt die Messungen von `old` für die Maße von `d`, wenn kein Maß neu ist und sich bei
/// keinem Ansicht, Art oder Bezüge geändert haben; sonst `None`.
fn reuse(old: &ResolvedDims, d: &Drawing) -> Option<ResolvedDims> {
    let by_id: HashMap<DimId, &(DimSpec, _)> = old
        .by_view
        .values()
        .flatten()
        .map(|e| (e.0.id, e))
        .collect();
    let mut out = ResolvedDims::default();
    for m in &d.dimensions {
        let (s, r) = by_id.get(&m.id)?;
        if s.view != m.view || s.kind != m.kind || s.refs != m.refs {
            return None;
        }
        out.by_view
            .entry(m.view)
            .or_default()
            .push((m.clone(), r.clone()));
    }
    Some(out)
}

impl DimensionUi {
    /// Zahl der Auflösungen gegen die Körper bisher (für Tests).
    pub fn resolutions(&self) -> u64 {
        self.resolutions
    }

    /// Löst die Maße der Zeichnung auf (nur wenn Modell- oder Dokumentstand oder die fertigen
    /// Schnitte sich geändert haben); bei unverändertem Stand dasselbe `Arc`. Gerechnet wird mit
    /// den exakten Körpern von `state.bodies` (Netz-Körper nicht), Maße einer Schnittansicht mit
    /// deren geschnittenen Körpern ([`crate::drawing_state::DrawingUi::view_solids`]); solange ein
    /// Schnitt rechnet, fehlen seine Maße im Ergebnis (sie gelten nicht als verloren). Die letzte
    /// erfolgreiche Lage jedes Maßes wird gemerkt und steht in `last_known` (für verlorene
    /// Maße); gelöschte Maße fallen dort weg. Gibt es die Zeichnung nicht, ist das Ergebnis leer.
    pub fn resolve(&mut self, state: &AppState, drawing: DrawingId) -> Arc<ResolvedDims> {
        let model = state.document.model_edit_count();
        let edit = state.document.edit_count();
        let sections: Sections = state
            .document
            .drawing(drawing)
            .map(|d| {
                state
                    .drawing_ui
                    .sections_for(d, model)
                    .into_iter()
                    .collect()
            })
            .unwrap_or_default();
        if let Some(c) = self
            .cache
            .as_ref()
            .filter(|c| same_sections(&c.sections, &sections))
            .and_then(|_| self.cached(drawing, model, edit))
        {
            return c;
        }
        let Some(d) = state.document.drawing(drawing) else {
            self.last_known.remove(&drawing);
            self.last_keys.remove(&drawing);
            return self.store(
                drawing,
                model,
                edit,
                (Vec::new(), Vec::new()),
                ResolvedDims::default(),
            );
        };
        let views: Vec<(ViewId, ViewKind)> = d.views.iter().map(|v| (v.id, v.kind)).collect();
        let reused = self
            .cache
            .as_ref()
            .filter(|c| {
                c.drawing == drawing
                    && c.model == model
                    && c.views == views
                    && same_sections(&c.sections, &sections)
            })
            .and_then(|c| reuse(&c.dims, d));
        let ids: HashSet<DimId> = d.dimensions.iter().map(|m| m.id).collect();
        let keys = self.last_keys.entry(drawing).or_default();
        keys.retain(|id, _| ids.contains(id));
        let mut dims = match reused {
            Some(r) => r,
            None => {
                self.resolutions += 1;
                let solids: Vec<&Solid> = exact_bodies(&state.bodies)
                    .into_iter()
                    .map(|b| &b.solid)
                    .collect();
                let view_solids: BTreeMap<ViewId, Vec<&Solid>> = sections
                    .iter()
                    .map(|(v, data)| (*v, data.solids.iter().collect()))
                    .collect();
                match resolve_drawing_dimensions_views(d, &solids, &view_solids, || {
                    Ok::<(), Infallible>(())
                }) {
                    Ok((dims, new_keys)) => {
                        keys.extend(new_keys);
                        dims
                    }
                    Err(never) => match never {},
                }
            }
        };
        let known = self.last_known.entry(drawing).or_default();
        known.retain(|id, _| ids.contains(id));
        for (s, r) in dims.by_view.values().flatten() {
            if let Ok(m) = r {
                known.insert(s.id, m.anchor.clone());
            }
        }
        dims.last_known = known.clone();
        self.store(drawing, model, edit, (views, sections), dims)
    }

    fn store(
        &mut self,
        drawing: DrawingId,
        model: u64,
        edit: u64,
        (views, sections): (Vec<(ViewId, ViewKind)>, Sections),
        dims: ResolvedDims,
    ) -> Arc<ResolvedDims> {
        let dims = Arc::new(dims);
        self.cache = Some(Cached {
            drawing,
            model,
            edit,
            views,
            sections,
            dims: Arc::clone(&dims),
        });
        dims
    }

    /// Die zuletzt aufgelösten Maße, aber nur für genau diesen Stand (`model` =
    /// `Document::model_edit_count`, `doc_edit` = `Document::edit_count`); sonst `None`. So
    /// erscheinen alte Werte nie als aktuell.
    pub fn cached(
        &self,
        drawing: DrawingId,
        model: u64,
        doc_edit: u64,
    ) -> Option<Arc<ResolvedDims>> {
        self.cache
            .as_ref()
            .filter(|c| c.drawing == drawing && c.model == model && c.edit == doc_edit)
            .map(|c| Arc::clone(&c.dims))
    }

    /// Letzter bekannter geometrischer Schlüssel des Maßes (aus der letzten Auflösung, in der
    /// sich seine Bezüge auflösen ließen); `None`, wenn es keinen gab.
    pub fn last_key(&self, drawing: DrawingId, dim: DimId) -> Option<&str> {
        self.last_keys.get(&drawing)?.get(&dim).map(String::as_str)
    }

    fn remember_key(&mut self, drawing: DrawingId, dim: DimId, key: String) {
        self.last_keys.entry(drawing).or_default().insert(dim, key);
    }

    /// Hinweise zu den Maßen `resolved` der Zeichnung: Zahl der verlorenen Maße und der Maße,
    /// die ganz oder teilweise außerhalb des Blatts liegen (Lage wie im Blattmodell mit den
    /// fertigen Ansichten aus `state.drawing_ui`), dazu die Sätze für die Hinweiszeile.
    pub fn notes(&self, state: &AppState, drawing: DrawingId, resolved: &ResolvedDims) -> DimNotes {
        let (marks, size) = build_marks(state, drawing, resolved);
        notes_of(resolved, &marks, size)
    }

    /// Zahl der Neuberechnungen in [`DimensionUi::marks`] bisher (für Tests).
    pub fn mark_builds(&self) -> u64 {
        self.mark_builds
    }

    /// Die Markierungen jedes Maßes ([`dimension_marks`]) zu `dims` mit den fertigen Ansichten
    /// aus `state.drawing_ui`, zwischengespeichert: solange `dims` dasselbe `Arc` ist und sich
    /// die Ergebnisse der Ansichten nicht geändert haben, wird nichts neu gerechnet (auch die
    /// Hinweise aus [`DimensionUi::cached_notes`] nicht).
    pub fn marks(
        &mut self,
        state: &AppState,
        drawing: DrawingId,
        dims: &Arc<ResolvedDims>,
    ) -> Arc<Vec<DimMarks>> {
        self.ensure_marks(state, drawing, dims).marks.clone()
    }

    /// Wie [`DimensionUi::notes`], aber aus demselben Zwischenspeicher wie
    /// [`DimensionUi::marks`] (für jedes Bild der Oberfläche).
    pub fn cached_notes(
        &mut self,
        state: &AppState,
        drawing: DrawingId,
        dims: &Arc<ResolvedDims>,
    ) -> DimNotes {
        self.ensure_marks(state, drawing, dims).notes.clone()
    }

    fn ensure_marks(
        &mut self,
        state: &AppState,
        drawing: DrawingId,
        dims: &Arc<ResolvedDims>,
    ) -> &MarksCached {
        let results: Vec<(ViewId, usize)> =
            state.document.drawing(drawing).map_or_else(Vec::new, |d| {
                state
                    .drawing_ui
                    .results_for(d, state.document.model_edit_count())
                    .iter()
                    .map(|(k, v)| (*k, Arc::as_ptr(v) as usize))
                    .collect()
            });
        let hit = self.marks_cache.as_ref().is_some_and(|c| {
            c.drawing == drawing && Arc::ptr_eq(&c.dims, dims) && c.results == results
        });
        if !hit {
            self.mark_builds += 1;
            let (marks, size) = build_marks(state, drawing, dims);
            let notes = notes_of(dims, &marks, size);
            self.marks_cache = Some(MarksCached {
                drawing,
                dims: Arc::clone(dims),
                results,
                marks: Arc::new(marks),
                notes,
            });
        }
        self.marks_cache.as_ref().expect("eben gesetzt")
    }
}

/// Markierungen je Maß und Blattgröße der Zeichnung (leer ohne Zeichnung).
fn build_marks(
    state: &AppState,
    drawing: DrawingId,
    resolved: &ResolvedDims,
) -> (Vec<DimMarks>, (f64, f64)) {
    let Some(d) = state.document.drawing(drawing) else {
        return (Vec::new(), (0.0, 0.0));
    };
    let owned = state
        .drawing_ui
        .results_for(d, state.document.model_edit_count());
    let results: BTreeMap<ViewId, &ViewLines> =
        owned.iter().map(|(k, v)| (*k, v.as_ref())).collect();
    (dimension_marks(d, &results, resolved), d.sheet.size_mm())
}

/// Hinweise aus den Messergebnissen und den Markierungen je Maß.
fn notes_of(resolved: &ResolvedDims, marks: &[DimMarks], size: (f64, f64)) -> DimNotes {
    let lost = resolved
        .by_view
        .values()
        .flatten()
        .filter(|(_, r)| r.is_err())
        .count();
    let outside = count_outside(marks, size);
    let mut texts = Vec::new();
    match lost {
        0 => {}
        n => texts.push(crate::msg!("dim-note-lost", n = n)),
    }
    match outside {
        0 => {}
        n => texts.push(crate::msg!("dim-note-outside", n = n)),
    }
    DimNotes {
        lost,
        outside,
        texts,
    }
}

/// Zahl der Maße, von deren Markierungen ein Punkt außerhalb des Blatts `(0, 0)…size` liegt
/// (Texte mit geschätzter Breite und Höhe; nicht endliche Punkte zählen als außerhalb).
pub fn count_outside(marks: &[DimMarks], size: (f64, f64)) -> usize {
    marks
        .iter()
        .filter(|dm| dm.marks.iter().any(|m| mark_outside(m, size)))
        .count()
}

/// Löst die Maße der Zeichnung mit dem Zwischenspeicher `state.dimension_ui` auf (siehe
/// [`DimensionUi::resolve`]).
pub fn resolve_in(state: &mut AppState, drawing: DrawingId) -> Arc<ResolvedDims> {
    let mut ui = std::mem::take(&mut state.dimension_ui);
    let dims = ui.resolve(state, drawing);
    state.dimension_ui = ui;
    dims
}

/// [`DimensionUi::marks`] mit dem Zwischenspeicher `state.dimension_ui`.
pub fn marks_in(
    state: &mut AppState,
    drawing: DrawingId,
    dims: &Arc<ResolvedDims>,
) -> Arc<Vec<DimMarks>> {
    let mut ui = std::mem::take(&mut state.dimension_ui);
    let marks = ui.marks(state, drawing, dims);
    state.dimension_ui = ui;
    marks
}

/// [`DimensionUi::cached_notes`] mit dem Zwischenspeicher `state.dimension_ui`.
pub fn notes_in(state: &mut AppState, drawing: DrawingId, dims: &Arc<ResolvedDims>) -> DimNotes {
    let mut ui = std::mem::take(&mut state.dimension_ui);
    let notes = ui.cached_notes(state, drawing, dims);
    state.dimension_ui = ui;
    notes
}

/// Liegt ein Punkt der Markierung außerhalb des Blatts `(0, 0)…size` (oder ist er nicht
/// endlich)? Texte zählen mit ihrer geschätzten Breite und Höhe.
fn mark_outside(m: &Mark, size: (f64, f64)) -> bool {
    let out = |p: DVec2| {
        !p.is_finite()
            || p.x < -SHEET_EPS
            || p.y < -SHEET_EPS
            || p.x > size.0 + SHEET_EPS
            || p.y > size.1 + SHEET_EPS
    };
    match m {
        Mark::Line { a, b, .. } => out(*a) || out(*b),
        Mark::Arrow { tip, dir, .. } => {
            let base = *tip + *dir * ARROW_LEN;
            let side = dir.perp() * ARROW_HALF_W;
            [*tip, base + side, base - side].into_iter().any(out)
        }
        Mark::Arc {
            center,
            r,
            start,
            end,
            ..
        } => (0..=ARC_SAMPLES).any(|i| {
            let t = start + (end - start) * i as f64 / ARC_SAMPLES as f64;
            out(*center + DVec2::new(t.cos(), t.sin()) * *r)
        }),
        Mark::Text {
            pos,
            height,
            angle,
            anchor,
            text,
            ..
        } => {
            let w = text_width(text, *height);
            let s = match anchor {
                TextAnchor::Start => 0.0,
                TextAnchor::Middle => -0.5 * w,
                TextAnchor::End => -w,
            };
            let u = DVec2::new(angle.cos(), angle.sin());
            let v = u.perp() * *height;
            let p0 = *pos + u * s;
            let p1 = *pos + u * (s + w);
            [p0, p1, p0 + v, p1 + v].into_iter().any(out)
        }
    }
}

/// Nächste freie Gruppennummer der Zeichnung (größte vorhandene + 1, erste: 1); `None`, wenn
/// keine mehr frei ist. Vorschlag und Werkzeug „Maß“ vergeben Gruppen so.
pub fn next_group(d: &Drawing) -> Option<u64> {
    d.dimensions
        .iter()
        .filter_map(|m| m.chain.map(|c| c.group))
        .max()
        .unwrap_or(0)
        .checked_add(1)
}

/// Bezug aus der Herkunft eines Vorschlags bzw. einer angeklickten Linie (Körperindex in den
/// exakten Körpern). Vorschlag und Werkzeug „Maß“ bilden Bezüge nur hiermit, damit gleiche
/// Maße gleiche Schlüssel haben. `cache` hält die Prüfpunkte der Flächen für den ganzen
/// Durchgang (ein Vorschlag, ein Klick): sonst kostete jeder Bezug an einer großen Fläche mit
/// vielen Bohrungen Zehntelsekunden.
pub(crate) fn geo_ref<'a>(
    solids: &[&'a Solid],
    o: CandidateOrigin,
    tol: &Tolerance,
    cache: &RefCache<'a>,
) -> Option<GeoRef> {
    let edge = |body: usize, edge: usize| {
        let s: &'a Solid = solids.get(body)?;
        let id = Id::<Edge>::from_index(edge);
        s.edges.get(id)?;
        geo_ref_for_edge_cached(s, id, tol, cache).ok()
    };
    Some(match o {
        CandidateOrigin::Edge { body, edge: e } => GeoRef::Edge(edge(body, e)?),
        CandidateOrigin::EdgeEnd { body, edge: e, end } => GeoRef::EdgeEnd {
            edge: edge(body, e)?,
            end,
        },
        CandidateOrigin::Center { body, edge: e } => GeoRef::Center(edge(body, e)?),
        CandidateOrigin::Face { body, face } => {
            let s: &'a Solid = solids.get(body)?;
            let id = Id::<Face>::from_index(face);
            s.faces.get(id)?;
            GeoRef::Face(geo_ref_for_face_cached(s, id, tol, cache).ok()?)
        }
    })
}

/// Richtung eines linearen Maßes auf dem Blatt (für das Stapeln nach P3): `Some(true)`
/// waagerecht, `Some(false)` senkrecht, `None` ohne Maßlinie (Ø/R mit Führungslinie, Winkel).
fn direction(anchor: &Anchor) -> Option<bool> {
    match anchor {
        Anchor::Linear { dir, .. } => Some(dir.x.abs() >= dir.y.abs()),
        _ => None,
    }
}

/// Richtung eines vorhandenen Maßes ohne Messung (verlorener Bezug) nach seiner Art.
fn direction_of_kind(kind: DimKind) -> Option<bool> {
    match kind {
        DimKind::Horizontal => Some(true),
        DimKind::Vertical => Some(false),
        _ => None,
    }
}

/// Schlägt Grundmaße für die Zeichnung vor (nur die Ansicht `only_view`, wenn angegeben) und
/// fügt sie dem Dokument hinzu.
///
/// Grundlage sind die **berechneten** Ansichten (`DrawingUi::result`); rechnet eine noch, gibt
/// es nichts ([`views_pending`]). Vorhandene Maße werden über ihren Schlüssel
/// (`dimension_key`) erkannt und nicht noch einmal vorgeschlagen; für Maße mit verlorenem Bezug
/// gilt ihr zuletzt bekannter Schlüssel ([`DimensionUi::last_key`]). Nur ein Maß, das in dieser
/// Sitzung nie aufgelöst wurde (etwa schon verloren aus einer Datei geladen), hat keinen und kann
/// erneut vorgeschlagen werden. Jeder Vorschlag wird
/// in Bezüge des Modells umgewandelt und nur gesetzt, wenn diese Bezüge wieder genau zu ihm
/// führen. Abstand der Maßlinie nach P3: `DEFAULT_OFFSET + n·DIM_STEP`, `n` = Zahl der Maße
/// derselben Ansicht und Richtung (Bezugsmaße einer Gruppe teilen den Abstand ihrer ersten
/// Stufe und zählen je Stufe), dazu der Überstand des Umrisses über die Bezüge
/// (`Candidate::clear` mal Maßstab; eine Gruppe nimmt ihn wie den Abstand von ihrem ersten
/// Glied – die Bohrungsmitten einer Gruppe hängen alle an derselben Bezugskante): Die Maßlinie
/// liegt so ab dem Umriss der Ansicht, nicht ab dem Ende einer Bezugskante, die vor einer
/// Rundung endet. Bezugsmaß-Gruppen bekommen in der Zeichnung neue Nummern. Jedes
/// Maß wird einzeln über `Document::add_dimension` geprüft; scheitert eines, wird es
/// ausgelassen und sein Grund gemeldet ([`SuggestOutcome::Partly`]).
///
/// Schnittansichten (Spec §9.3) übernehmen keine Maße ihrer Quelle: Ihre Vorschläge entstehen
/// aus ihren eigenen Linien und den geschnittenen Körpern, Bezüge auf eine Schnittfläche tragen
/// deren Schnittmarke. Rechnet ein gewählter Schnitt noch, gibt es nichts ([`views_pending`]).
pub fn suggest_for(
    state: &mut AppState,
    drawing: DrawingId,
    only_view: Option<ViewId>,
) -> SuggestOutcome {
    use SuggestOutcome::Unavailable;
    let Some(d) = state.document.drawing(drawing).cloned() else {
        return Unavailable(no_drawing());
    };
    if state.bodies.is_empty() {
        return Unavailable(draw_error_msg(&DrawError::NoBodies));
    }
    let exact = exact_bodies(&state.bodies);
    if exact.is_empty() {
        return Unavailable(draw_error_msg(&DrawError::MeshBodiesOnly));
    }
    if let Some(o) = only_view {
        match d.view(o) {
            None => return Unavailable(no_view()),
            Some(v) if !v.kind.is_normal() => return Unavailable(no_isometric()),
            Some(_) => {}
        }
    }
    let wanted: Vec<&ViewSpec> = d
        .views
        .iter()
        .filter(|v| v.kind.is_normal() && only_view.is_none_or(|o| v.id == o))
        .collect();
    if wanted.is_empty() {
        return Unavailable(no_normal_view());
    }
    if d.dimensions.len() >= MAX_DIMENSIONS {
        return Unavailable(full_text());
    }
    let model = state.document.model_edit_count();
    let mut ready: Vec<(ViewSpec, Arc<ViewLines>)> = Vec::new();
    let mut failed: Option<Msg> = None;
    for v in &wanted {
        match state.drawing_ui.result_of(v, model) {
            ViewResult::Pending { .. } => return Unavailable(views_pending()),
            ViewResult::Failed(m) => {
                failed.get_or_insert(m);
            }
            ViewResult::Ready(lines) => ready.push(((*v).clone(), lines)),
        }
    }
    if ready.is_empty() {
        return Unavailable(failed.unwrap_or_else(views_pending));
    }
    // Die geschnittenen Körper aller fertigen Schnittansichten (auch nicht gewählter: deren
    // vorhandene Maße zählen gegen Doppelungen und für die Stufen).
    let sections = state.drawing_ui.sections_for(&d, model);
    if ready
        .iter()
        .any(|(v, _)| v.section.is_some() && !sections.contains_key(&v.id))
    {
        return Unavailable(views_pending());
    }

    let solids: Vec<&Solid> = exact.iter().map(|b| &b.solid).collect();
    // Körper je Ansicht (Spec §9.3): Normansichten die exakten Körper, Schnittansichten ihre
    // geschnittenen Körper; dazu die Ausdehnung wie in der Ansichtsrechnung.
    let plain = ViewBodies::new(solids);
    let cut: BTreeMap<ViewId, ViewBodies> = sections
        .iter()
        .map(|(v, data)| (*v, ViewBodies::new(data.solids.iter().collect())))
        .collect();
    let bodies_of = |v: &ViewSpec| -> Option<&ViewBodies> {
        if v.section.is_some() {
            cut.get(&v.id)
        } else {
            Some(&plain)
        }
    };
    let tol = Tolerance::default();
    // Ein Speicher für den ganzen Vorschlag (Prüfpunkte, Kantenverwendungen, Achsbereiche; die
    // Schlüssel sind die Körper, also gilt er für alle Ansichten).
    let cache = RefCache::new();
    let geos_in = |b: &ViewBodies, refs: &[GeoRef]| -> Option<Vec<Geo3>> {
        refs.iter()
            .map(|r| resolve_geo_cached(&b.solids, r, &tol, &cache).ok())
            .collect()
    };

    // Vorhandene Maße: Schlüssel (gegen Doppelungen) und Zahl je Ansicht und Richtung (P3).
    let mut existing: Vec<(ViewId, DimKind, String)> = Vec::new();
    let mut remember: Vec<(DimId, String)> = Vec::new();
    let mut stack: HashMap<(ViewId, bool), usize> = HashMap::new();
    for m in &d.dimensions {
        let Some(v) = d.view(m.view) else {
            continue;
        };
        let basis = Basis::of(v.kind);
        // Schnitt noch nicht fertig: wie ein Maß ohne Messung.
        let vb = bodies_of(v);
        let geos = vb.and_then(|b| geos_in(b, &m.refs));
        let extent = vb.map_or(0.0, |b| b.extent);
        let measured = geos
            .as_ref()
            .and_then(|g| measure(m.kind, g, &basis, v.kind, extent).ok());
        let dir = match &measured {
            Some(x) => direction(&x.anchor),
            None => direction_of_kind(m.kind),
        };
        if let Some(h) = dir {
            *stack.entry((m.view, h)).or_insert(0) += 1;
        }
        match geos.and_then(|g| dimension_key(m.kind, &g, &basis, v.kind, extent)) {
            Some(key) => {
                remember.push((m.id, key.clone()));
                existing.push((m.view, m.kind, key));
            }
            // Bezug verloren: der zuletzt bekannte Schlüssel verhindert, dass dasselbe Maß
            // noch einmal vorgeschlagen wird (das rote bleibt, bis es gelöscht wird).
            None => {
                if let Some(key) = state.dimension_ui.last_key(drawing, m.id) {
                    existing.push((m.view, m.kind, key.to_string()));
                }
            }
        }
    }

    for (id, key) in remember {
        state.dimension_ui.remember_key(drawing, id, key);
    }

    // Erst alle Normansichten mit den exakten Körpern, dann jede Schnittansicht für sich mit
    // ihren geschnittenen Körpern (die Herkunft eines Vorschlags zählt in den Körpern seines
    // Durchgangs). Übernommen wird nichts: jede Ansicht bekommt Vorschläge aus ihren Linien.
    let mut passes: Vec<(&ViewBodies, Vec<(ViewSpec, &ViewLines)>)> = Vec::new();
    let normal: Vec<(ViewSpec, &ViewLines)> = ready
        .iter()
        .filter(|(s, _)| s.section.is_none())
        .map(|(s, l)| (s.clone(), l.as_ref()))
        .collect();
    if !normal.is_empty() {
        passes.push((&plain, normal));
    }
    for (s, l) in ready.iter().filter(|(s, _)| s.section.is_some()) {
        if let Some(b) = cut.get(&s.id) {
            passes.push((b, vec![(s.clone(), l.as_ref())]));
        }
    }

    let mut next_group = next_group(&d);
    let mut count = d.dimensions.len();
    let mut added = 0usize;
    let mut proposed = 0usize;
    let mut skipped: Vec<Msg> = Vec::new();
    let mut unbound = false;
    'passes: for (vb, pairs) in passes {
        let candidates = suggest(&vb.draw(), &pairs, &existing);
        proposed += candidates.len();
        // Gruppe des Vorschlags → (Nummer in der Zeichnung, Abstand, nächste Stufe).
        let mut groups: HashMap<u64, (u64, f64, u32)> = HashMap::new();
        let geos_of = |refs: &[GeoRef]| geos_in(vb, refs);
        for (i, c) in candidates.iter().enumerate() {
            if count >= MAX_DIMENSIONS {
                let rest = candidates.len() - i;
                skipped.push(Msg::Join(vec![
                    full_text(),
                    crate::msg!("dim-rest-not-set", n = rest),
                ]));
                break 'passes;
            }
            // Stufen und Gruppen zählen erst weiter, wenn das Maß wirklich gesetzt ist.
            let (mut st, mut gr, mut ng) = (stack.clone(), groups.clone(), next_group);
            let Some(spec) = candidate_spec(
                c, &d, &vb.solids, &tol, &cache, vb.extent, &geos_of, &mut st, &mut gr, &mut ng,
            ) else {
                unbound = true;
                continue;
            };
            match state.document.add_dimension(drawing, spec) {
                Ok(id) => {
                    state.dimension_ui.remember_key(drawing, id, c.key.clone());
                    existing.push((c.view, c.kind, c.key.clone()));
                    added += 1;
                    count += 1;
                    (stack, groups, next_group) = (st, gr, ng);
                }
                Err(e) => skipped.push(crate::msg!("dim-skipped", reason = e.to_string())),
            }
        }
    }
    if proposed == 0 {
        return SuggestOutcome::NothingNew;
    }
    if unbound {
        skipped.insert(0, not_bound());
    }
    if skipped.is_empty() {
        SuggestOutcome::Added(added)
    } else {
        SuggestOutcome::Partly { added, skipped }
    }
}

/// Die Körper, auf denen die Maße einer Ansicht arbeiten, mit ihrer Ausdehnung.
struct ViewBodies<'a> {
    solids: Vec<&'a Solid>,
    extent: f64,
}

impl<'a> ViewBodies<'a> {
    fn new(solids: Vec<&'a Solid>) -> ViewBodies<'a> {
        let mut b = ViewBodies {
            solids,
            extent: 0.0,
        };
        b.extent = extent_of(&b.draw());
        b
    }

    /// Die Körper als Eingabe der Ansichtsrechnung (ohne Namen).
    fn draw(&self) -> Vec<DrawBody<'a>> {
        self.solids
            .iter()
            .map(|s| DrawBody {
                name: String::new(),
                solid: s,
            })
            .collect()
    }
}

/// Maß aus einem Vorschlag: Bezüge aus der Herkunft (nur, wenn sie wieder zum selben Schlüssel
/// führen), Abstand nach P3 zuzüglich `c.clear` (Modell-mm) mal Maßstab der Ansicht, Gruppe mit
/// Nummer der Zeichnung. Zählt Stufen und Gruppen weiter.
#[allow(clippy::too_many_arguments)]
fn candidate_spec<'a>(
    c: &Candidate,
    d: &Drawing,
    solids: &[&'a Solid],
    tol: &Tolerance,
    cache: &RefCache<'a>,
    extent: f64,
    geos_of: &dyn Fn(&[GeoRef]) -> Option<Vec<Geo3>>,
    stack: &mut HashMap<(ViewId, bool), usize>,
    groups: &mut HashMap<u64, (u64, f64, u32)>,
    next_group: &mut Option<u64>,
) -> Option<DimSpec> {
    let view = d.view(c.view)?;
    let basis = Basis::of(view.kind);
    let refs: Vec<GeoRef> = c
        .origins
        .iter()
        .map(|o| geo_ref(solids, *o, tol, cache))
        .collect::<Option<_>>()?;
    let geos = geos_of(&refs)?;
    if dimension_key(c.kind, &geos, &basis, view.kind, extent).as_deref() != Some(c.key.as_str()) {
        return None;
    }
    let measured = measure(c.kind, &geos, &basis, view.kind, extent).ok()?;
    let dir = direction(&measured.anchor);
    let out = if c.clear.is_finite() {
        c.clear.max(0.0) * view.scale.factor()
    } else {
        0.0
    };
    let level = |stack: &HashMap<(ViewId, bool), usize>, h: bool| {
        DEFAULT_OFFSET + DIM_STEP * stack.get(&(c.view, h)).copied().unwrap_or(0) as f64 + out
    };
    let (offset, chain) = match (c.chain, dir) {
        (Some(ch), Some(h)) => {
            let entry = match groups.get(&ch.group) {
                Some(e) => Some(*e),
                None => next_group.map(|g| {
                    *next_group = g.checked_add(1);
                    let e = (g, level(stack, h), 0);
                    groups.insert(ch.group, e);
                    e
                }),
            };
            match entry {
                Some((g, off, order)) => {
                    groups.insert(ch.group, (g, off, order + 1));
                    (
                        off,
                        Some(ChainSpec {
                            group: g,
                            mode: ch.mode,
                            order,
                        }),
                    )
                }
                // Keine freie Gruppennummer mehr: als einzelnes Maß.
                None => (level(stack, h), None),
            }
        }
        (_, Some(h)) => (level(stack, h), None),
        (_, None) => (DEFAULT_OFFSET, None),
    };
    if let Some(h) = dir {
        *stack.entry((c.view, h)).or_insert(0) += 1;
    }
    Some(DimSpec {
        id: DimId(0),
        view: c.view,
        kind: c.kind,
        refs,
        offset,
        angle: 45.0,
        text_shift: (0.0, 0.0),
        tolerance: DimTolerance::None,
        decimals: default_decimals(c.kind, d.unit),
        text_override: None,
        chain,
        suggested: true,
    })
}
