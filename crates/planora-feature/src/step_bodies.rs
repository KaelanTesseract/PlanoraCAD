//! STEP-Körper (Teilprojekt STEP-Import, Plan P4, P6, P7, P14): Formtabelle des Dokuments mit
//! dem normalisierten STEP-Text je Form, einmal gebautem Körper und Anlegen beim Import (ganz
//! oder gar nicht).
//!
//! Ein STEP-Körper ist ein gewöhnlicher exakter Körper: Er läuft über die Fertigstellung
//! (`finish`: Prüfung und Marken „STEP-Fläche n“ mit der Feature-Kennung als Herkunft) und lässt
//! sich verrechnen, verrunden, verschieben und als Skizzenebene benutzen. Er hat keine
//! Zahlenfelder; Verschieben geht über das Verschieben-Feature.

use crate::error::FeatureError;
use crate::model::{Document, FeatureId, FeatureKind, FeatureStatus};
use planora_kernel::Solid;
use planora_step::read::units::{MAX_WELD_TOL_MM, MIN_WELD_TOL_MM};
use planora_step::{COARSE_TOL_MM, check_normal_header, read_normal_body};
use std::cell::Cell;
use std::collections::{BTreeSet, HashSet};
use std::sync::{Arc, OnceLock};

thread_local! {
    static STEP_BUILDS: Cell<u64> = const { Cell::new(0) };
    static STEP_TEXT_SUM_LIMIT: Cell<usize> = const { Cell::new(MAX_STEP_TEXT_SUM_BYTES) };
}

/// Größte Summe der STEP-Texte eines Dokuments in Bytes (jede Form einmal gezählt). Bis hierhin
/// lässt sich das Dokument speichern (Dateiformat 10); der Import lehnt mehr ab, damit ein
/// Dokument nie mehr hält, als es speichern kann.
pub const MAX_STEP_TEXT_SUM_BYTES: usize = 200 * 1024 * 1024;

/// Die geltende Summengrenze: [`MAX_STEP_TEXT_SUM_BYTES`], außer ein Test hat sie in diesem
/// Thread mit [`limit_step_text_sum_for_tests`] herabgesetzt.
#[doc(hidden)]
pub fn step_text_sum_limit() -> usize {
    STEP_TEXT_SUM_LIMIT.with(Cell::get)
}

/// Setzt die Summengrenze im aufrufenden Thread, bis der Wächter fällt (nur für Tests, damit
/// die Grenze ohne Hunderte MiB Text prüfbar ist).
#[doc(hidden)]
#[must_use]
pub fn limit_step_text_sum_for_tests(limit: usize) -> StepTextSumLimitGuard {
    let previous = STEP_TEXT_SUM_LIMIT.with(|c| c.replace(limit));
    StepTextSumLimitGuard { previous }
}

/// Stellt beim Fallen die vorige Summengrenze wieder her.
#[doc(hidden)]
pub struct StepTextSumLimitGuard {
    previous: usize,
}

impl Drop for StepTextSumLimitGuard {
    fn drop(&mut self) {
        STEP_TEXT_SUM_LIMIT.with(|c| c.set(self.previous));
    }
}

/// Summe der Textlängen der Formen in Bytes; dieselbe Form (dasselbe `Arc`) zählt einmal, wie
/// sie auch nur einmal in der Datei steht.
pub fn step_text_sum<'a>(shapes: impl IntoIterator<Item = &'a Arc<StepShape>>) -> usize {
    let mut seen: HashSet<*const StepShape> = HashSet::new();
    let mut sum = 0usize;
    for s in shapes {
        if seen.insert(Arc::as_ptr(s)) {
            sum = sum.saturating_add(s.text().len());
        }
    }
    sum
}

/// Bytes als Megabyte (1 MB = 1024 × 1024 Bytes) mit höchstens einer Nachkommastelle, nach
/// oben gerundet, mit deutschem Komma: knapp über der Grenze ist nie „gleich“ der Grenze.
pub fn step_text_megabytes(bytes: usize) -> String {
    let tenths = (bytes as u128 * 10).div_ceil(1024 * 1024);
    if tenths.is_multiple_of(10) {
        format!("{}", tenths / 10)
    } else {
        format!("{},{}", tenths / 10, tenths % 10)
    }
}

/// Zahl der Körperaufbauten aus einem gespeicherten Text ([`StepShape::solid`]) in diesem
/// Thread seit Programmstart. Nur für Tests, die prüfen, dass eine schon gebaute Form nicht
/// erneut gebaut wird (zählt im aufrufenden Thread, deshalb unabhängig von parallel laufenden
/// Tests).
#[doc(hidden)]
pub fn step_builds() -> u64 {
    STEP_BUILDS.with(Cell::get)
}

/// Kennung einer Form in der Formtabelle des Dokuments (ab 1); wird nie wiederverwendet, außer
/// nach einem zurückgenommenen Import.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct StepShapeId(pub u64);

/// Form eines STEP-Körpers: der normalisierte STEP-Text (P5), die Genauigkeit der Datei in mm
/// (P4, für die Warnung „grob“) und der daraus gebaute Körper. Gebaut wird höchstens einmal je
/// Form; mehrere Dokumente (Verlauf, Rückgängig) teilen sich die Form über `Arc`.
pub struct StepShape {
    text: String,
    tolerance_mm: f64,
    /// Nennt die Ursprungsdatei ihre Genauigkeit? Sonst ist `tolerance_mm` angenommen (P4).
    tolerance_known: bool,
    solid: OnceLock<Result<Solid, String>>,
}

impl StepShape {
    /// Form aus einem gespeicherten Text. Prüft nur den Kopf ([`check_normal_header`], P14) und
    /// die Genauigkeit (1e-6 … 1e-3 mm); gebaut wird erst beim Rechnen ([`StepShape::solid`]).
    pub fn new(text: String, tolerance_mm: f64) -> Result<StepShape, String> {
        check_normal_header(&text)?;
        if !(MIN_WELD_TOL_MM..=MAX_WELD_TOL_MM).contains(&tolerance_mm) {
            return Err("Die Genauigkeit muss zwischen 0,000001 und 0,001 mm liegen.".into());
        }
        Ok(StepShape {
            text,
            tolerance_mm,
            tolerance_known: true,
            solid: OnceLock::new(),
        })
    }

    /// Form aus dem Import: Text und der daraus schon gebaute Körper (Sitzung = Neuladen, P5).
    pub fn with_solid(text: String, tolerance_mm: f64, solid: Solid) -> StepShape {
        StepShape {
            text,
            tolerance_mm,
            tolerance_known: true,
            solid: OnceLock::from(Ok(solid)),
        }
    }

    /// Dieselbe Form, deren Ursprungsdatei keine Genauigkeit nennt: `tolerance_mm` ist dann der
    /// angenommene Wert (P4: „Genauigkeit unbekannt“), die Form gilt als grob.
    pub fn accuracy_unknown(mut self) -> StepShape {
        self.tolerance_known = false;
        self
    }

    /// Nennt die Ursprungsdatei ihre Genauigkeit?
    pub fn tolerance_known(&self) -> bool {
        self.tolerance_known
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    /// Genauigkeit der Datei in mm.
    pub fn tolerance_mm(&self) -> f64 {
        self.tolerance_mm
    }

    /// Grobe Genauigkeit (gröber als [`COARSE_TOL_MM`] oder unbekannt): Rechnen mit dem Körper
    /// kann scheitern.
    pub fn is_coarse(&self) -> bool {
        !self.tolerance_known || self.tolerance_mm > COARSE_TOL_MM
    }

    /// Der Körper der Form; beim ersten Aufruf aus dem Text gebaut ([`read_normal_body`]), danach
    /// (auch ein Fehler) nur noch geliefert.
    pub fn solid(&self) -> Result<&Solid, &str> {
        self.solid
            .get_or_init(|| {
                STEP_BUILDS.with(|c| c.set(c.get() + 1));
                read_normal_body(&self.text)
            })
            .as_ref()
            .map_err(String::as_str)
    }
}

impl std::fmt::Debug for StepShape {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StepShape")
            .field("text_bytes", &self.text.len())
            .field("tolerance_mm", &self.tolerance_mm)
            .field("tolerance_known", &self.tolerance_known)
            .field("built", &self.solid.get().map(Result::is_ok))
            .finish()
    }
}

/// Ein neuer STEP-Körper beim Import: Name (nicht leer) und Form.
#[derive(Clone, Debug)]
pub struct NewStepBody {
    pub name: String,
    pub shape: Arc<StepShape>,
}

impl Document {
    pub fn step_shape(&self, id: StepShapeId) -> Option<&Arc<StepShape>> {
        self.step_shapes.get(&id)
    }

    /// Form des STEP-Körpers `feature` (auch wenn er unterdrückt oder fehlerhaft ist); `None`
    /// für andere Arten.
    pub fn step_body(&self, feature: FeatureId) -> Option<&Arc<StepShape>> {
        match &self.feature(feature)?.kind {
            FeatureKind::StepBody { shape } => self.step_shapes.get(shape),
            _ => None,
        }
    }

    /// Legt die Formen mit fortlaufenden Kennungen und je Körper ein Feature am Marker an (in
    /// der gegebenen Reihenfolge, je Körper eine Änderung). Ist danach ein neuer Körper nicht
    /// `Ok`, wird alles zurückgenommen (Features, Formen, Kennungszähler, Änderungszahl, Marker,
    /// dabei entstandene Folgeblätter der Stückliste samt Zeichnungszähler)
    /// und die Meldung des ersten geliefert („„{Name}“: {Meldung}“). Ein leerer Aufruf ändert
    /// nichts. Vorher abgelehnt (nichts geändert): leere Namen und STEP-Text, der zusammen mit
    /// den Formen des Dokuments die Summengrenze ([`MAX_STEP_TEXT_SUM_BYTES`]) überschreitet.
    pub fn add_step_bodies(
        &mut self,
        bodies: Vec<NewStepBody>,
    ) -> Result<Vec<FeatureId>, FeatureError> {
        if bodies.is_empty() {
            return Ok(Vec::new());
        }
        if bodies.iter().any(|b| b.name.trim().is_empty()) {
            return Err(FeatureError::InvalidInput(
                "Der Name darf nicht leer sein".into(),
            ));
        }
        // Nie mehr STEP-Text halten, als sich speichern lässt (Summengrenze des Dateiformats).
        let total = step_text_sum(
            self.step_shapes
                .values()
                .chain(bodies.iter().map(|b| &b.shape)),
        );
        let limit = step_text_sum_limit();
        if total > limit {
            return Err(FeatureError::StepImport(format!(
                "Die STEP-Formen im Projekt sind zusammen zu groß ({} MB, erlaubt {} MB). Bitte weniger oder kleinere Körper importieren.",
                step_text_megabytes(total),
                step_text_megabytes(limit)
            )));
        }
        let saved = (self.next_id, self.next_step_shape, self.edits, self.marker);
        // Neue Körper können ein Folgeblatt der Stückliste auslösen (`sync_parts_sheets`); die
        // Rücknahme stellt deshalb auch Zeichnungen, Zeichnungskennung und -änderungszahl her.
        let saved_drawings = (self.drawings.clone(), self.next_drawing, self.drawing_edits);
        let mut shapes = Vec::with_capacity(bodies.len());
        let mut ids = Vec::with_capacity(bodies.len());
        let added = self.insert_step_bodies(bodies, &mut shapes, &mut ids);
        let refused = match added {
            Err(e) => Some(e),
            Ok(()) => ids.iter().find_map(|id| {
                let f = self.feature(*id)?;
                let message = match &f.status {
                    FeatureStatus::Ok => return None,
                    FeatureStatus::Error(m) | FeatureStatus::Skipped(m) => m.clone(),
                    other => format!("{other:?}"),
                };
                Some(FeatureError::StepImport(format!("„{}“: {message}", f.name)))
            }),
        };
        let Some(e) = refused else {
            return Ok(ids);
        };
        self.features.retain(|f| !ids.contains(&f.id));
        for s in &shapes {
            self.step_shapes.remove(s);
        }
        (self.next_id, self.next_step_shape, self.edits, self.marker) = saved;
        self.recompute();
        (self.drawings, self.next_drawing, self.drawing_edits) = saved_drawings;
        Err(e)
    }

    /// Formen und Features anlegen; merkt sich alles Angelegte für die Rücknahme.
    fn insert_step_bodies(
        &mut self,
        bodies: Vec<NewStepBody>,
        shapes: &mut Vec<StepShapeId>,
        ids: &mut Vec<FeatureId>,
    ) -> Result<(), FeatureError> {
        let mut named = Vec::with_capacity(bodies.len());
        for b in bodies {
            let id = StepShapeId(self.next_step_shape);
            self.next_step_shape = self.next_step_shape.checked_add(1).ok_or_else(|| {
                FeatureError::InvalidInput("Die STEP-Form-Kennungen sind aufgebraucht".into())
            })?;
            self.step_shapes.insert(id, b.shape);
            shapes.push(id);
            named.push((b.name, id));
        }
        for (name, shape) in named {
            ids.push(self.add_feature(name, FeatureKind::StepBody { shape })?);
        }
        Ok(())
    }

    /// Formen, auf die ein Feature verweist.
    pub(crate) fn used_step_shapes(&self) -> BTreeSet<StepShapeId> {
        self.features
            .iter()
            .filter_map(|f| match &f.kind {
                FeatureKind::StepBody { shape } => Some(*shape),
                _ => None,
            })
            .collect()
    }

    /// Entfernt Formen, auf die kein Feature mehr verweist.
    pub(crate) fn prune_step_shapes(&mut self) {
        let used = self.used_step_shapes();
        self.step_shapes.retain(|id, _| used.contains(id));
    }
}
