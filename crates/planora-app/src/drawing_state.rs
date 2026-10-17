//! Zeichnungsstand der App (Teilprojekt 11, Aufgabe 14): geöffneter Arbeitsbereich, gewählte
//! Ansicht, Zoom und Verschiebung des Blatts sowie die Berechnung der Ansichtslinien im
//! Hintergrund.
//!
//! Ein Ergebnis hängt nur am Modellstand und an der Ansichtsart (Festlegung F8): Schlüssel
//! `(Document::model_edit_count(), ViewKind)`. Maßstab, Lage und andere Zeichnungsänderungen
//! rechnen nichts neu. Jeder Arbeitsthread bekommt eigene Kopien der exakten Körper; es gibt
//! keinen geteilten veränderlichen Zustand (Festlegung F11).

use crate::export_dialog::shorten_for_display;
use crate::jobs::{Job, Poll};
use crate::state::{Body, BodyKind};
use planora_drawing::dimension::DimId;
use planora_drawing::sheet_frame::sheet_layout;
use planora_drawing::{
    Basis, DrawBody, DrawError, Drawing, DrawingId, Projection, Scale, Sheet, ViewId, ViewKind,
    ViewLines, compute_view,
};
use planora_drawing::{GAP_MM, align_dir};
use planora_exchange::Progress;
use planora_feature::Document;
use planora_kernel::Solid;
use planora_math::DVec3;
use planora_step::display_name;
use std::collections::BTreeMap;
use std::sync::Arc;

/// Höchstzahl der Einträge im Zwischenspeicher: einer je Ansichtsart.
pub const MAX_CACHED: usize = 7;
const _: () = assert!(MAX_CACHED >= ViewKind::ALL.len());

/// Meldung, wenn ein Arbeitsthread ohne Ergebnis endet (Absturz).
pub const LOST_TEXT: &str = "Die Berechnung wurde unerwartet beendet.";

/// Geöffneter Arbeitsbereich.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum WorkArea {
    #[default]
    Model,
    Drawing(DrawingId),
}

/// Stand der Linien einer Ansichtsart.
#[derive(Clone, Debug)]
pub enum ViewResult {
    /// Wird berechnet (oder ist noch nicht angefangen: `fraction` 0).
    Pending {
        fraction: f32,
    },
    Ready(Arc<ViewLines>),
    /// Verständliche Meldung, warum es keine Linien gibt.
    Failed(String),
}

impl PartialEq for ViewResult {
    /// Fertige Ergebnisse sind gleich, wenn es dieselbe Berechnung ist.
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (ViewResult::Pending { fraction: a }, ViewResult::Pending { fraction: b }) => a == b,
            (ViewResult::Ready(a), ViewResult::Ready(b)) => Arc::ptr_eq(a, b),
            (ViewResult::Failed(a), ViewResult::Failed(b)) => a == b,
            _ => false,
        }
    }
}

/// Die Rechnung einer Ansicht (austauschbar nur für Tests).
pub type ComputeFn = fn(&[DrawBody], ViewKind, &Progress) -> Result<ViewLines, DrawError>;

type ViewJob = Job<Result<ViewLines, DrawError>>;

enum EntryState {
    Running {
        job: ViewJob,
        progress: Arc<Progress>,
    },
    Ready(Arc<ViewLines>),
    Failed(String),
}

struct Entry {
    /// Modellstand, zu dem das Ergebnis gehört.
    model: u64,
    kind: ViewKind,
    state: EntryState,
}

/// Zustand des Zeichnungsbereichs.
pub struct DrawingUi {
    pub area: WorkArea,
    pub selected_view: Option<ViewId>,
    /// Im Zeichnungsbereich gewähltes Maß (schließt eine gewählte Ansicht aus).
    pub selected_dim: Option<DimId>,
    /// Letzte Meldung zu den Maßen (etwa nach „Maße vorschlagen“) für die Hinweiszeile.
    pub dim_message: Option<String>,
    /// Stand zur Meldung (Änderungszähler, gewählte Ansicht, gewähltes Maß); weicht der
    /// aktuelle Stand ab, verfällt die Meldung.
    pub dim_message_key: Option<(u64, Option<ViewId>, Option<DimId>)>,
    /// Im Browser gewählte Zeichnung (unabhängig davon, ob sie geöffnet ist).
    pub selected_drawing: Option<DrawingId>,
    pub zoom: f32,
    pub pan: egui::Vec2,
    /// Höchstens ein Eintrag je Ansichtsart.
    cache: Vec<Entry>,
    /// Zeichnung, die beim letzten `tick` (oder `open`) offen war; ein Wechsel setzt Auswahl,
    /// Zoom und Verschiebung zurück (Ansichtskennungen zählen je Zeichnung).
    last_drawing: Option<DrawingId>,
    spawned: u64,
    compute: ComputeFn,
}

impl Default for DrawingUi {
    fn default() -> Self {
        DrawingUi {
            area: WorkArea::Model,
            selected_view: None,
            selected_dim: None,
            dim_message: None,
            dim_message_key: None,
            selected_drawing: None,
            zoom: 1.0,
            pan: egui::Vec2::ZERO,
            cache: Vec::new(),
            last_drawing: None,
            spawned: 0,
            compute: compute_view,
        }
    }
}

impl std::fmt::Debug for DrawingUi {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DrawingUi")
            .field("area", &self.area)
            .field("selected_view", &self.selected_view)
            .field("zoom", &self.zoom)
            .field("pan", &self.pan)
            .field("cached", &self.cache.len())
            .field("spawned", &self.spawned)
            .finish()
    }
}

/// Exakte Körper als eigene Kopien für einen Arbeitsthread (Netz-Körper ausgelassen).
fn exact_copies(bodies: &[Body]) -> Vec<(String, Solid)> {
    bodies
        .iter()
        .enumerate()
        .filter(|(_, b)| b.kind != BodyKind::Mesh)
        .map(|(i, b)| (display_name(&b.name, i), b.solid.clone()))
        .collect()
}

impl DrawingUi {
    /// Zahl gestarteter Berechnungen (für Tests).
    pub fn spawned(&self) -> u64 {
        self.spawned
    }

    /// Öffnet die Zeichnung `id`: Bereich setzen, Auswahl leeren, Zoom und Verschiebung auf
    /// die Vorgaben.
    pub fn open(&mut self, id: DrawingId) {
        self.area = WorkArea::Drawing(id);
        self.last_drawing = Some(id);
        self.reset_sheet_view();
    }

    /// Zeigt die Zeichnung `id` im Arbeitsbereich. War es die zuletzt offene, bleiben Auswahl,
    /// Zoom und Verschiebung erhalten (etwa nach dem Wechsel zum 3D-Modell und zurück);
    /// sonst beginnt die Zeichnung frisch wie bei [`DrawingUi::open`].
    pub fn show_drawing(&mut self, id: DrawingId) {
        if self.last_drawing == Some(id) {
            self.area = WorkArea::Drawing(id);
        } else {
            self.open(id);
        }
    }

    fn reset_sheet_view(&mut self) {
        self.selected_view = None;
        self.selected_dim = None;
        self.dim_message = None;
        self.dim_message_key = None;
        self.zoom = 1.0;
        self.pan = egui::Vec2::ZERO;
    }

    /// Ersetzt die Rechnung (nur für Tests: Abstürze, Fehler, lange Rechnungen).
    #[doc(hidden)]
    pub fn set_compute(&mut self, f: ComputeFn) {
        self.compute = f;
    }

    /// Fortschritt der laufenden Berechnung einer Art (nur für Tests).
    #[doc(hidden)]
    pub fn progress_of(&self, kind: ViewKind) -> Option<Arc<Progress>> {
        self.cache.iter().find_map(|e| match &e.state {
            EntryState::Running { progress, .. } if e.kind == kind => Some(Arc::clone(progress)),
            _ => None,
        })
    }

    /// Startet fehlende oder veraltete Berechnungen für die Ansichtsarten der aktuellen
    /// Zeichnung; holt fertige ab (mit `block` wartend).
    ///
    /// Ohne geöffnete Zeichnung geschieht nichts. Gibt es die geöffnete Zeichnung nicht (mehr),
    /// geht der Bereich zurück zum Modell. Einträge eines älteren Modellstands werden verworfen
    /// (laufende Berechnungen dadurch abgebrochen); ebenso laufende Berechnungen für Arten, die
    /// die geöffnete Zeichnung nicht braucht. Fertige Ergebnisse des aktuellen Stands bleiben
    /// (höchstens eines je Art, [`MAX_CACHED`]).
    ///
    /// `state_bodies` müssen zum aktuellen Modellstand von `document` gehören (nach
    /// `AppState::refresh`); sonst gehörte ein Ergebnis zum falschen Stand.
    ///
    /// Wurde der Bereich direkt (ohne [`DrawingUi::open`]) auf eine andere Zeichnung gesetzt,
    /// werden Auswahl, Zoom und Verschiebung hier zurückgesetzt.
    pub fn tick(&mut self, state_bodies: &[Body], document: &Document, block: bool) {
        let WorkArea::Drawing(id) = self.area else {
            return;
        };
        let Some(drawing) = document.drawing(id) else {
            self.area = WorkArea::Model;
            self.selected_view = None;
            self.selected_dim = None;
            // Die Zeichnung gibt es nicht mehr: eine neue mit derselben Kennung (anderes
            // Dokument) beginnt frisch.
            self.last_drawing = None;
            return;
        };
        if self.last_drawing != Some(id) {
            self.last_drawing = Some(id);
            self.reset_sheet_view();
        }
        if self
            .selected_view
            .is_some_and(|v| drawing.view(v).is_none())
        {
            self.selected_view = None;
        }
        if self
            .selected_dim
            .is_some_and(|m| drawing.dimension(m).is_none())
        {
            self.selected_dim = None;
        }
        let mut needed: Vec<ViewKind> = Vec::new();
        for v in &drawing.views {
            if !needed.contains(&v.kind) {
                needed.push(v.kind);
            }
        }
        let model = document.model_edit_count();
        // Veraltete Einträge und nicht gebrauchte laufende Berechnungen verwerfen; `Drop` des
        // Auftrags bricht ab.
        self.cache.retain(|e| {
            e.model == model
                && (needed.contains(&e.kind) || !matches!(e.state, EntryState::Running { .. }))
        });
        // Erst die vorhandenen abholen, dann neue starten: Ein frisch gestarteter Auftrag wird
        // ohne `block` erst im nächsten Bild gefragt.
        for e in &mut self.cache {
            Self::fetch(e, false);
        }
        for kind in needed {
            if self.cache.iter().any(|e| e.kind == kind) {
                continue;
            }
            let state = self.start(state_bodies, kind);
            self.cache.push(Entry { model, kind, state });
        }
        if block {
            for e in &mut self.cache {
                Self::fetch(e, true);
            }
        }
    }

    /// Startet die Berechnung einer Art; ohne exakte Körper steht die Meldung sofort fest.
    fn start(&mut self, bodies: &[Body], kind: ViewKind) -> EntryState {
        let solids = exact_copies(bodies);
        if solids.is_empty() {
            let e = if bodies.is_empty() {
                DrawError::NoBodies
            } else {
                DrawError::MeshBodiesOnly
            };
            return EntryState::Failed(e.to_string());
        }
        let compute = self.compute;
        let progress = Arc::new(Progress::new());
        let job = Job::spawn_with(Arc::clone(&progress), move |p| {
            let refs: Vec<DrawBody> = solids
                .iter()
                .map(|(name, solid)| DrawBody {
                    name: name.clone(),
                    solid,
                })
                .collect();
            compute(&refs, kind, p)
        });
        self.spawned += 1;
        EntryState::Running { job, progress }
    }

    fn fetch(e: &mut Entry, block: bool) {
        let EntryState::Running { job, .. } = &e.state else {
            return;
        };
        e.state = match job.fetch(block) {
            Poll::Pending => return,
            Poll::Ready(Ok(lines)) => EntryState::Ready(Arc::new(lines)),
            Poll::Ready(Err(err)) => EntryState::Failed(err.to_string()),
            Poll::Lost => EntryState::Failed(LOST_TEXT.into()),
        };
    }

    /// Stand der Linien einer Art zum Modellstand `model` (`Document::model_edit_count`).
    /// Ein Eintrag eines anderen Stands zählt als fehlend (`Pending` mit 0): alte Linien
    /// erscheinen nie als aktuell, auch nicht vor dem nächsten `tick`.
    pub fn result(&self, kind: ViewKind, model: u64) -> ViewResult {
        let entry = self
            .cache
            .iter()
            .find(|e| e.kind == kind && e.model == model);
        match entry.map(|e| &e.state) {
            None => ViewResult::Pending { fraction: 0.0 },
            Some(EntryState::Running { job, .. }) => ViewResult::Pending {
                fraction: job.fraction(),
            },
            Some(EntryState::Ready(lines)) => ViewResult::Ready(Arc::clone(lines)),
            Some(EntryState::Failed(m)) => ViewResult::Failed(m.clone()),
        }
    }

    /// Fertige Linien je Ansicht der Zeichnung zum Modellstand `model` (Ansichten ohne
    /// fertiges Ergebnis dieses Stands fehlen).
    pub fn results_for(&self, drawing: &Drawing, model: u64) -> BTreeMap<ViewId, Arc<ViewLines>> {
        drawing
            .views
            .iter()
            .filter_map(|v| match self.result(v.kind, model) {
                ViewResult::Ready(lines) => Some((v.id, lines)),
                _ => None,
            })
            .collect()
    }

    /// Hinweis zu den Körpern: keine Körper, nur Netz-Körper, oder welche Netz-Körper nicht
    /// gezeichnet werden. `None`, wenn alle Körper exakt sind.
    pub fn note(&self, bodies: &[Body]) -> Option<String> {
        if bodies.is_empty() {
            return Some(DrawError::NoBodies.to_string());
        }
        let meshes: Vec<String> = bodies
            .iter()
            .enumerate()
            .filter(|(_, b)| b.kind == BodyKind::Mesh)
            .map(|(i, b)| shorten_for_display(&display_name(&b.name, i)))
            .collect();
        if meshes.is_empty() {
            None
        } else if meshes.len() == bodies.len() {
            Some(DrawError::MeshBodiesOnly.to_string())
        } else {
            Some(format!(
                "Netz-Körper werden nicht gezeichnet: {}.",
                meshes.join(", ")
            ))
        }
    }
}

/// Gemeinsame Hüllbox aller exakten Körper; `None` ohne exakte Körper.
fn exact_bounds(bodies: &[Body]) -> Option<(DVec3, DVec3)> {
    bodies
        .iter()
        .filter(|b| b.kind != BodyKind::Mesh)
        .map(|b| b.bounds)
        .reduce(|(a0, a1), (b0, b1)| (a0.min(b0), a1.max(b1)))
}

/// Ausdehnung der gemeinsamen Hüllbox entlang `dir` (8 Ecken projiziert).
fn extent_along(bounds: (DVec3, DVec3), dir: DVec3) -> f64 {
    let (lo, hi) = bounds;
    let mut min = f64::INFINITY;
    let mut max = f64::NEG_INFINITY;
    for i in 0..8 {
        let c = DVec3::new(
            if i & 1 == 0 { lo.x } else { hi.x },
            if i & 2 == 0 { lo.y } else { hi.y },
            if i & 4 == 0 { lo.z } else { hi.z },
        );
        let t = c.dot(dir);
        min = min.min(t);
        max = max.max(t);
    }
    max - min
}

/// Geschätzte Breite und Höhe einer Ansicht (ungeskaliert, mm) aus der Hüllbox der exakten
/// Körper; ohne exakte Körper `(0, 0)`.
pub fn estimate_view_size(bodies: &[Body], kind: ViewKind) -> (f64, f64) {
    let Some(b) = exact_bounds(bodies) else {
        return (0.0, 0.0);
    };
    let basis = Basis::of(kind);
    (extent_along(b, basis.ex), extent_along(b, basis.ey))
}

/// Größte Stufe der Maßstabsreihe, bei der Grundansicht, Seitenansicht und Draufsicht samt
/// [`GAP_MM`] dazwischen in den freien Bereich des Blatts (`free`: Breite und Höhe in mm) passen.
/// Die Seitenansicht liegt waagerecht neben der Grundansicht (Breite = Tiefe der Grundansicht),
/// die Draufsicht senkrecht (Höhe = Tiefe); für Erst- und Drittwinkel ergeben sich dieselben
/// Summen. Passt keine Stufe: die kleinste (1:100). Ohne exakte Körper gibt es nichts zu
/// messen: 1:1.
pub fn suggest_scale_in(bodies: &[Body], free: (f64, f64), base: ViewKind) -> Scale {
    let (free_w, free_h) = free;
    let basis = Basis::of(base);
    let (ex, ey, ed) = match exact_bounds(bodies) {
        Some(b) => (
            extent_along(b, basis.ex),
            extent_along(b, basis.ey),
            extent_along(b, basis.d),
        ),
        None => return Scale { num: 1, den: 1 },
    };
    let fits = |s: Scale| {
        let f = s.factor();
        f * (ex + ed) + GAP_MM <= free_w && f * (ey + ed) + GAP_MM <= free_h
    };
    let smallest = Scale::SERIES
        .into_iter()
        .min_by(|a, b| a.factor().total_cmp(&b.factor()))
        .unwrap_or(Scale { num: 1, den: 100 });
    Scale::SERIES
        .into_iter()
        .filter(|s| fits(*s))
        .max_by(|a, b| a.factor().total_cmp(&b.factor()))
        .unwrap_or(smallest)
}

/// Wie [`suggest_scale_in`] für das Blatt einer neuen Zeichnung (ohne Änderungen, mit den drei
/// Hinweiszeilen der Vorgabe).
pub fn suggest_scale(
    bodies: &[Body],
    sheet: Sheet,
    projection: Projection,
    base: ViewKind,
) -> Scale {
    // Erst- und Drittwinkel vertauschen nur die Seiten, nicht die Summen.
    let _ = projection;
    let (lo, hi) = sheet_layout(sheet, 0, 3).free;
    suggest_scale_in(bodies, (hi.x - lo.x, hi.y - lo.y), base)
}

/// Vorgabeabstand der Mitten einer abgeleiteten Ansicht `child` von ihrer Ausgangsansicht
/// `parent`: halbe Ausdehnung der Ausgangsansicht entlang der Ausrichtungsachse (mit ihrem
/// Maßstab), [`GAP_MM`] und halbe Ausdehnung der neuen Ansicht (mit ihrem Maßstab).
pub fn default_distance(
    bodies: &[Body],
    parent: ViewKind,
    child: ViewKind,
    parent_scale: Scale,
    child_scale: Scale,
) -> f64 {
    // Die Achse (waagerecht oder senkrecht) hängt nicht von der Projektionsart ab.
    let horizontal = align_dir(Projection::First, parent, child).is_none_or(|(x, _)| x != 0.0);
    let (pw, ph) = estimate_view_size(bodies, parent);
    let (cw, ch) = estimate_view_size(bodies, child);
    let (p, c) = if horizontal { (pw, cw) } else { (ph, ch) };
    p * parent_scale.factor() / 2.0 + GAP_MM + c * child_scale.factor() / 2.0
}
