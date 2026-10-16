//! Fenster „Exportieren“ (Spec §8 des Netz-Austauschs, Spec §6 des STEP-Exports): Format,
//! Qualität und Umfang als gleich hohe Karten über die ganze Breite; das Netz bzw. die Prüfung
//! der Körper entsteht im Arbeitsthread (Fortschritt, Abbrechen); danach Zahl der Dreiecke,
//! geschätzte Größe und Prüfbericht (Netz) bzw. Zahl der Körper und Flächen (STEP); gespeichert
//! wird über die eigene Dateiauswahl (Plan P14) und eine Hilfsdatei.
//!
//! Zeichnungen (Teilprojekt 11, Aufgabe 17) als sechstes und siebtes Format: Die Ansichten der
//! gewählten Zeichnung werden im eigenen Arbeitsthread des Fensters berechnet (nicht über den
//! Zwischenspeicher des Zeichnungsbereichs, Festlegung F11); geschrieben wird nur, wenn alle
//! Ansichten berechnet sind.
//!
//! Maße und Mittellinien (Teilprojekt 12, Aufgabe 14): Nach den Ansichten löst derselbe
//! Arbeitsthread die Maße der Zeichnung gegen seine Kopien der exakten Körper auf (nicht über den
//! Zwischenspeicher der Oberfläche) und baut das Blatt mit `build_sheet_with`. Hat ein Maß seinen
//! Bezug verloren, wird nichts geschrieben (Festlegung P13); Maße außerhalb des Blatts werden
//! geschrieben und in der Karte gezählt (P12).

use crate::browser::ERROR_COLOR;
use crate::cards::card_row;
use crate::dimension_dialog::kind_label;
use crate::dimension_state::count_outside;
use crate::drawing_dialog::paper_label;
use crate::drawing_state::{ComputeFn, WorkArea};
use crate::drawing_view::view_names;
use crate::file_chooser::{ChooserAction, FileChooser, default_dir};
use crate::jobs::{Job, Poll};
use crate::mesh_body::{WARNING_COLOR, report_text};
use crate::state::{AppState, BodyKind};
use planora_drawing::annotate::{Mark, ResolvedDims};
use planora_drawing::dimension::{DimError, DimSpec};
use planora_drawing::dxf::dxf_text_fits;
use planora_drawing::sheet::{SheetModel, build_sheet_with, dimension_marks};
use planora_drawing::{
    DrawBody, DrawError, Drawing, DrawingFormat, DrawingId, Sheet, ViewId, ViewKind, ViewLines,
    Visibility, compute_view, export_drawing_file,
};
use planora_exchange::{
    ExchangeError, ExchangeMesh, ExportBody, Format, MeshReport, MeshTransform, PlacedSource,
    Progress, SolidSource, check_mesh, collect_body, export_file,
};
use planora_feature::{BodyRef, resolve_drawing_dimensions};
use planora_kernel::Solid;
use planora_mesh::{TessellationOptions, tessellate};
use planora_step::{
    MAX_NAME_CHARS, StepBody, StepError, StepOptions, check_body, display_name, export_step_file,
};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

/// Abstand des Fensters zum linken und rechten Rand; sonst reicht es über die ganze Breite.
pub const DIALOG_MARGIN: f32 = 16.0;
/// Abstand des Fensters zum oberen Rand (unter Menü und Tab-Leiste).
pub const DIALOG_TOP: f32 = 60.0;
/// Kleinste Breite bei sehr schmalen Fenstern.
pub(crate) const MIN_WIDTH: f32 = 320.0;
/// Breite des Fortschrittsbalkens.
pub(crate) const PROGRESS_WIDTH: f32 = 280.0;
/// So oft wird neu gezeichnet, solange ein Arbeitsthread läuft.
pub(crate) const REPAINT: Duration = Duration::from_millis(100);
/// Bytes je Megabyte (dezimal, wie die Dateiverwaltung).
const BYTES_PER_MB: f64 = 1e6;
/// Darunter zeigt das Fenster „unter 0,1 MB“.
const MIN_SHOWN_MB: f64 = 0.1;
/// Meldung, wenn ein Arbeitsthread ohne Ergebnis endet.
pub const LOST: &str = "Die Berechnung wurde unerwartet beendet.";
/// Warnung bei nicht geschlossenem Netz (Spec E6).
pub const NOT_CLOSED_WARNING: &str = "Die Datei wird trotzdem geschrieben. Manche Programme für den 3D-Druck melden solche Netze als fehlerhaft.";
/// Eintrag des fünften Formats (STEP-Spec §6).
pub const STEP_LABEL: &str = "STEP – für andere CAD-Programme (exakt)";
/// Steht bei STEP an der Stelle der Karte „Qualität“ (STEP-Spec §6).
pub const STEP_EXACT: &str = "Rundungen und Bohrungen bleiben exakt.";
/// Hinweis, wenn das Fenster beim Schreiben einer STEP-Datei nicht geschlossen werden darf.
pub const WRITING_HINT: &str = "Das Schreiben läuft noch. Bitte warten oder erst abbrechen.";
/// So viele Zeichen eines Körpernamens zeigt das Fenster in Meldungen und Listen höchstens.
const MAX_SHOWN_NAME: usize = 40;
/// Meldung, wenn bei STEP kein Körper übrig bleibt.
pub const STEP_NOTHING: &str = "Es gibt keinen Körper, der sich als STEP-Datei ausgeben lässt.";
/// Hinweis bei den Zeichnungsformaten, wenn das Dokument keine Zeichnung hat.
pub const NO_DRAWING: &str =
    "Es gibt noch keine Zeichnung. Über „Zeichnung“ im Menü lässt sich eine anlegen.";
/// So oft schaut der Arbeitsthread nach, ob die Berechnung einer Ansicht abgebrochen werden soll.
const WATCH: Duration = Duration::from_millis(5);

/// Fenster über die ganze Breite (Rand DIALOG_MARGIN), unter Menü und Tab-Leiste.
pub(crate) fn wide_window(title: &'static str, ctx: &egui::Context) -> egui::Window<'static> {
    let width = (ctx.content_rect().width() - 2.0 * DIALOG_MARGIN).max(MIN_WIDTH);
    egui::Window::new(title)
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_TOP, [0.0, DIALOG_TOP])
        .min_width(width)
        .max_width(width)
}

/// Sehnentoleranz „Grob“ in Millimetern (Spec E5).
const CHORD_COARSE: f64 = 0.1;
/// Sehnentoleranz „Normal“ in Millimetern (Spec E5).
const CHORD_NORMAL: f64 = 0.01;
/// Sehnentoleranz „Fein“ in Millimetern (Spec E5).
const CHORD_FINE: f64 = 0.001;
/// Meldung, wenn sich Auswahl, Qualität oder Modell seit der Berechnung geändert haben.
pub const STALE: &str = "Die Auswahl hat sich geändert, bitte noch einmal speichern.";
/// Meldung, wenn es noch kein fertiges Netz gibt.
pub const NOT_COMPUTED: &str = "Es ist noch nichts berechnet. Bitte warten oder neu berechnen.";

/// Qualität des Netzes (Spec E5).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Quality {
    Coarse,
    Normal,
    Fine,
}

impl Quality {
    pub const ALL: [Quality; 3] = [Quality::Coarse, Quality::Normal, Quality::Fine];

    /// Sehnentoleranz in Millimetern.
    pub fn chord_tolerance(self) -> f64 {
        match self {
            Quality::Coarse => CHORD_COARSE,
            Quality::Normal => CHORD_NORMAL,
            Quality::Fine => CHORD_FINE,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Quality::Coarse => "Grob (bis 0,1 mm Abweichung)",
            Quality::Normal => "Normal (bis 0,01 mm)",
            Quality::Fine => "Fein (bis 0,001 mm)",
        }
    }
}

/// Umfang (Spec E4).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    Selected,
    Visible,
}

/// Format im Fenster: eines der vier Netzformate, STEP (exakte Körper) oder eine Zeichnung.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ExportFormat {
    Mesh(Format),
    Step,
    Drawing(DrawingFormat),
}

impl ExportFormat {
    pub const ALL: [ExportFormat; 7] = [
        ExportFormat::Mesh(Format::Stl),
        ExportFormat::Mesh(Format::Obj),
        ExportFormat::Mesh(Format::ThreeMf),
        ExportFormat::Mesh(Format::Glb),
        ExportFormat::Step,
        ExportFormat::Drawing(DrawingFormat::Svg),
        ExportFormat::Drawing(DrawingFormat::Dxf),
    ];

    /// Dateiendung ohne Punkt.
    pub fn extension(self) -> &'static str {
        match self {
            ExportFormat::Mesh(f) => f.extension(),
            ExportFormat::Step => "step",
            ExportFormat::Drawing(f) => f.extension(),
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            ExportFormat::Mesh(f) => format_label(f),
            ExportFormat::Step => STEP_LABEL,
            ExportFormat::Drawing(f) => f.label(),
        }
    }

    pub fn is_step(self) -> bool {
        self == ExportFormat::Step
    }

    pub fn is_drawing(self) -> bool {
        self.drawing_format().is_some()
    }

    /// Das Zeichnungsformat, wenn es eines ist.
    pub fn drawing_format(self) -> Option<DrawingFormat> {
        match self {
            ExportFormat::Drawing(f) => Some(f),
            _ => None,
        }
    }
}

/// Ein Körper für den Arbeitsthread (Kopie, damit das Dokument frei bleibt).
#[derive(Clone, Debug)]
pub enum ExportSource {
    Solid(Solid),
    Mesh(Arc<ExchangeMesh>, MeshTransform),
}

#[derive(Clone, Debug)]
pub struct ExportItem {
    pub name: String,
    pub source: ExportSource,
}

/// Fertig vernetzte Körper mit gemeinsamem Bericht.
#[derive(Debug)]
pub struct Prepared {
    pub bodies: Vec<ExportBody>,
    pub report: MeshReport,
}

impl Prepared {
    pub fn triangles(&self) -> u64 {
        self.bodies
            .iter()
            .map(|b| b.mesh.triangle_count() as u64)
            .sum()
    }

    pub fn vertices(&self) -> u64 {
        self.bodies
            .iter()
            .map(|b| b.mesh.positions.len() as u64)
            .sum()
    }
}

/// Für STEP geprüfte Körper (STEP-Spec §4, §6, E3).
#[derive(Debug)]
pub struct StepPrepared {
    /// Körper, die die Prüfung bestanden haben: Name und exakter Körper.
    pub bodies: Vec<(String, Solid)>,
    /// Flächen aller geprüften Körper.
    pub faces: usize,
    /// Abgelehnte Körper mit Grund („„Name“ wird nicht ausgegeben: …“).
    pub rejected: Vec<String>,
    /// Netz-Körper, die STEP nicht enthält (E3).
    pub skipped_meshes: Vec<String>,
    /// Körper, deren Name in der Datei auf `MAX_NAME_CHARS` Zeichen gekürzt wird (kurz gezeigt).
    pub shortened: Vec<String>,
}

/// Was das Fenster zum Ergebnis zeigt.
#[derive(Debug, PartialEq)]
pub enum ExportStatus {
    Computing(f32),
    Ready {
        triangles: u64,
        bytes: u64,
        report: MeshReport,
    },
    /// STEP: Zahl der Körper und Flächen, abgelehnte Körper, ausgelassene Netz-Körper.
    StepReady {
        bodies: usize,
        faces: usize,
        rejected: Vec<String>,
        skipped_meshes: Vec<String>,
    },
    /// Zeichnung: alle Ansichten berechnet; Zahl der Ansichten, Blatt, Linien (verdeckte nur bei
    /// Ansichten, die sie zeigen), ausgelassene Netz-Körper, Zahl der Maße und der Maße außerhalb
    /// des Blatts, Mittellinien.
    DrawingReady {
        views: usize,
        sheet: String,
        visible: usize,
        hidden: usize,
        skipped_meshes: Vec<String>,
        dimensions: usize,
        outside: usize,
        centerlines: CenterlineUse,
    },
    Failed(String),
    /// Es gibt nichts zu berechnen (keine Zeichnung, keine exakten Körper, keine Ansicht).
    Unavailable(String),
}

/// Zeichnung mit allen berechneten Ansichten, fertig zum Schreiben.
#[derive(Debug)]
pub struct DrawingPrepared {
    pub model: Arc<SheetModel>,
    pub views: usize,
    /// Blatt, z. B. „A3 quer“.
    pub sheet: String,
    pub visible: usize,
    pub hidden: usize,
    /// Netz-Körper, die die Zeichnung nicht enthält.
    pub skipped_meshes: Vec<String>,
    /// Zahl der Maße auf dem Blatt.
    pub dimensions: usize,
    /// Zahl der Maße, die ganz oder teilweise außerhalb des Blatts liegen (P12).
    pub outside: usize,
    pub centerlines: CenterlineUse,
}

/// Mittellinien der Normansichten einer Zeichnung (die isometrische Ansicht hat nie welche).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CenterlineUse {
    /// Alle Normansichten zeigen Mittellinien.
    Yes,
    /// Keine Ansicht zeigt Mittellinien.
    No,
    /// Manche Normansichten zeigen sie, andere nicht.
    Partly,
}

impl CenterlineUse {
    /// „ja“, „nein“ oder „teilweise“.
    pub fn label(self) -> &'static str {
        match self {
            CenterlineUse::Yes => "ja",
            CenterlineUse::No => "nein",
            CenterlineUse::Partly => "teilweise",
        }
    }
}

/// Zeigen die Normansichten der Zeichnung Mittellinien (Schalter der Ansicht)?
pub fn centerline_use(d: &Drawing) -> CenterlineUse {
    let flags: Vec<bool> = d
        .views
        .iter()
        .filter(|v| v.kind.is_normal())
        .map(|v| v.centerlines)
        .collect();
    if !flags.is_empty() && flags.iter().all(|c| *c) {
        CenterlineUse::Yes
    } else if flags.iter().any(|c| *c) {
        CenterlineUse::Partly
    } else {
        CenterlineUse::No
    }
}

/// Das Auflösen der Maße einer Zeichnung im Arbeitsthread (austauschbar nur für Tests).
pub type ResolveFn = fn(&Drawing, &[&Solid], &Progress) -> Result<ResolvedDims, DrawError>;

/// Löst die Maße der Zeichnung gegen die Körper `solids` auf (Kopien im Arbeitsthread); bricht
/// zwischen den Maßen ab, wenn gewünscht.
pub fn resolve_dimensions_in_worker(
    drawing: &Drawing,
    solids: &[&Solid],
    progress: &Progress,
) -> Result<ResolvedDims, DrawError> {
    resolve_drawing_dimensions(drawing, solids, || {
        progress.check().map_err(|_| DrawError::Cancelled)
    })
}

/// Name eines Maßes in Meldungen: der eigene Text (der Platzhalter für den Wert `<>` als `?`,
/// gekürzt), sonst die Art („Waagerecht“, „Durchmesser“ …).
pub fn dimension_label(spec: &DimSpec) -> String {
    match spec.text_override.as_deref().map(str::trim) {
        Some(t) if !t.is_empty() => shorten_for_display(&t.replace("<>", "?")),
        _ => kind_label(spec.kind).to_string(),
    }
}

/// Meldung bei einem Maß ohne Messwert (Festlegung P13): Ist der Bezug verloren
/// ([`DimError::Lost`]), „… hat seinen Bezug verloren: …“; sonst (Bezug vorhanden, aber z. B.
/// nicht mehr quer zum Maß) „… hat sich verändert und lässt sich nicht mehr messen: …“.
///
/// Der Grund ist bei `Lost` der Satz aus der Auflösung selbst (ohne den Vorsatz „Der Bezug
/// wurde nicht gefunden“). Am Ende steht, was hilft: Das Fenster „Maß bearbeiten“ kann den
/// Bezug nicht neu verknüpfen, also bleibt nur Löschen und neu Setzen ([`LOST_ADVICE`]).
pub fn lost_dimension_text(label: &str, view: &str, error: &DimError) -> String {
    let text = match error {
        DimError::Lost(inner) => inner.clone(),
        e => e.to_string(),
    };
    let reason = text.trim_end().trim_end_matches('.');
    let what = match error {
        DimError::Lost(_) => "hat seinen Bezug verloren",
        _ => "hat sich verändert und lässt sich nicht mehr messen",
    };
    format!("Das Maß „{label}“ in der Ansicht „{view}“ {what}: {reason}. {LOST_ADVICE}")
}

/// Schlusssatz der Meldung bei einem Maß ohne Messwert (Festlegung P13).
pub const LOST_ADVICE: &str = "Bitte das Maß löschen und neu setzen.";

/// Das erste Maß (Ansichten in der Reihenfolge der Zeichnung) ohne Messwert als Meldung.
fn first_lost(
    drawing: &Drawing,
    dims: &ResolvedDims,
    names: &BTreeMap<ViewId, String>,
) -> Option<String> {
    drawing.views.iter().find_map(|v| {
        dims.by_view.get(&v.id)?.iter().find_map(|(spec, r)| {
            let e = r.as_ref().err()?;
            let name = names
                .get(&v.id)
                .cloned()
                .unwrap_or_else(|| v.kind.label().to_string());
            Some(lost_dimension_text(&dimension_label(spec), &name, e))
        })
    })
}

/// Hinweis auf Maße außerhalb des Blatts (sie werden trotzdem geschrieben).
pub fn outside_text(n: usize) -> String {
    if n == 1 {
        "1 Maß liegt außerhalb des Blatts und wird trotzdem geschrieben.".into()
    } else {
        format!("{n} Maße liegen außerhalb des Blatts und werden trotzdem geschrieben.")
    }
}

/// Passen alle Texte des Blatts in ein DXF-`TEXT`? Sonst die Meldung (statt eines Fehlers erst
/// beim Schreiben).
pub fn dxf_texts_fit(model: &SheetModel) -> Result<(), String> {
    for v in &model.views {
        for m in &v.marks {
            if let Mark::Text { text, .. } = m
                && !dxf_text_fits(text)
            {
                return Err(format!(
                    "Ein Maßtext in der Ansicht „{}“ ist für DXF zu lang: „{}“. Bitte den eigenen Text kürzen.",
                    v.name,
                    shorten_for_display(text)
                ));
            }
        }
    }
    Ok(())
}

/// Blatt für die Anzeige, z. B. „A3 quer“.
pub fn sheet_label(sheet: Sheet) -> String {
    format!(
        "{} {}",
        paper_label(sheet.paper),
        if sheet.landscape { "quer" } else { "hoch" }
    )
}

/// Berechnet eine Ansichtsart in einem eigenen Thread mit eigenem Fortschritt; ein Abbruch von
/// `outer` wird weitergereicht. `None`, wenn die Rechnung abstürzt.
fn compute_watched(
    solids: &[(String, Solid)],
    kind: ViewKind,
    compute: ComputeFn,
    outer: &Progress,
) -> Option<Result<ViewLines, DrawError>> {
    let inner = Progress::new();
    std::thread::scope(|scope| {
        let handle = scope.spawn(|| {
            let refs: Vec<DrawBody> = solids
                .iter()
                .map(|(name, solid)| DrawBody {
                    name: name.clone(),
                    solid,
                })
                .collect();
            compute(&refs, kind, &inner)
        });
        while !handle.is_finished() {
            if outer.is_cancelled() {
                inner.cancel();
            }
            std::thread::sleep(WATCH);
        }
        handle.join().ok()
    })
}

/// Berechnet die Ansichten der Zeichnung (jede Ansichtsart einmal, Fortschritt je Art), löst die
/// Maße auf und baut das Blatt (läuft im Arbeitsthread). Scheitert eine Ansicht, gibt es kein
/// Blatt, sondern die Meldung „Die Ansicht „Name“ lässt sich nicht berechnen: …“ (Name wie im
/// Browser); siehe [`prepare_drawing_with`].
pub fn prepare_drawing(
    drawing: &Drawing,
    solids: &[(String, Solid)],
    compute: ComputeFn,
    progress: &Progress,
) -> Result<DrawingPrepared, String> {
    prepare_drawing_with(
        drawing,
        solids,
        compute,
        resolve_dimensions_in_worker,
        progress,
    )
}

/// Wie [`prepare_drawing`] mit eigener Auflösung der Maße `resolve`. Nach den Ansichten werden
/// die Maße gegen `solids` (die Kopien des Arbeitsthreads) aufgelöst; hat eines seinen Bezug
/// verloren, gibt es kein Blatt, sondern die Meldung „Das Maß „…“ in der Ansicht „…“ hat seinen
/// Bezug verloren: …“ (P13). Das Blatt entsteht mit `build_sheet_with`; ohne Maße ist es
/// dasselbe wie mit `build_sheet`.
pub fn prepare_drawing_with(
    drawing: &Drawing,
    solids: &[(String, Solid)],
    compute: ComputeFn,
    resolve: ResolveFn,
    progress: &Progress,
) -> Result<DrawingPrepared, String> {
    let cancelled = || ExchangeError::Cancelled.to_string();
    let mut kinds: Vec<ViewKind> = Vec::new();
    for v in &drawing.views {
        if !kinds.contains(&v.kind) {
            kinds.push(v.kind);
        }
    }
    let with_dims = !drawing.dimensions.is_empty();
    progress.set_total(kinds.len() as u64 + u64::from(with_dims));
    let names = view_names(drawing);
    let mut lines: Vec<(ViewKind, ViewLines)> = Vec::with_capacity(kinds.len());
    for kind in kinds {
        progress.check().map_err(|_| cancelled())?;
        match compute_watched(solids, kind, compute, progress) {
            None => return Err(LOST.to_string()),
            Some(Ok(l)) => lines.push((kind, l)),
            Some(Err(DrawError::Cancelled)) => return Err(cancelled()),
            Some(Err(e)) => {
                let name = drawing
                    .views
                    .iter()
                    .find(|v| v.kind == kind)
                    .and_then(|v| names.get(&v.id))
                    .cloned()
                    .unwrap_or_else(|| kind.label().to_string());
                return Err(format!(
                    "Die Ansicht „{name}“ lässt sich nicht berechnen: {e}"
                ));
            }
        }
        progress.advance(1);
    }
    let map: BTreeMap<ViewId, &ViewLines> = drawing
        .views
        .iter()
        .filter_map(|v| {
            lines
                .iter()
                .find(|(k, _)| *k == v.kind)
                .map(|(_, l)| (v.id, l))
        })
        .collect();
    let dims = if with_dims {
        progress.check().map_err(|_| cancelled())?;
        let refs: Vec<&Solid> = solids.iter().map(|(_, s)| s).collect();
        let dims = match resolve(drawing, &refs, progress) {
            Ok(d) => d,
            Err(DrawError::Cancelled) => return Err(cancelled()),
            Err(e) => return Err(e.to_string()),
        };
        if let Some(m) = first_lost(drawing, &dims, &names) {
            return Err(m);
        }
        progress.advance(1);
        dims
    } else {
        ResolvedDims::default()
    };
    let model = build_sheet_with(drawing, &map, &dims);
    // `build_sheet` lässt Ansichten weg, die sich nicht anordnen lassen; das Dokument prüft Lage,
    // Maßstab und Eltern, also kommt das nur bei einer beschädigten Zeichnung vor. Nie still
    // weniger Ansichten schreiben.
    if model.views.len() < drawing.views.len() {
        return Err(VIEWS_NOT_PLACEABLE.to_string());
    }
    let count = |vis| {
        model
            .views
            .iter()
            .flat_map(|v| &v.lines)
            .filter(|l| l.vis == vis)
            .count()
    };
    let outside = count_outside(
        &dimension_marks(drawing, &map, &dims),
        drawing.sheet.size_mm(),
    );
    Ok(DrawingPrepared {
        views: model.views.len(),
        sheet: sheet_label(drawing.sheet),
        visible: count(Visibility::Visible),
        hidden: count(Visibility::Hidden),
        skipped_meshes: Vec::new(),
        dimensions: dims.by_view.values().map(Vec::len).sum(),
        outside,
        centerlines: centerline_use(drawing),
        model: Arc::new(model),
    })
}

/// Meldung, wenn sich nicht alle Ansichten auf dem Blatt anordnen lassen.
pub const VIEWS_NOT_PLACEABLE: &str =
    "Die Zeichnung enthält Ansichten, die sich nicht auf dem Blatt anordnen lassen.";

/// Hinweis auf Netz-Körper, die eine Zeichnung nicht enthält.
pub fn drawing_skipped_text(names: &[String]) -> String {
    let list: Vec<String> = names.iter().map(|n| format!("„{n}“")).collect();
    format!(
        "Netz-Körper sind in der Zeichnung nicht enthalten: {}.",
        list.join(", ")
    )
}

/// Meldung zu einem Fehler beim Schreiben einer Zeichnung (Abbrechen wie bei den anderen
/// Formaten).
fn draw_write_text(e: DrawError) -> String {
    match e {
        DrawError::Cancelled => ExchangeError::Cancelled.to_string(),
        e => e.to_string(),
    }
}

/// Vorgabe der Zeichnung: die geöffnete, sonst die erste.
fn default_drawing(state: &AppState) -> Option<DrawingId> {
    match state.drawing_ui.area {
        WorkArea::Drawing(id) if state.document.drawing(id).is_some() => Some(id),
        _ => state.document.drawings().first().map(|d| d.id),
    }
}

/// Exakte Körper als eigene Kopien für den Arbeitsthread und die Namen der Netz-Körper.
fn drawing_bodies(state: &AppState) -> (Vec<(String, Solid)>, Vec<String>) {
    let mut solids = Vec::new();
    let mut meshes = Vec::new();
    for (i, b) in state.bodies.iter().enumerate() {
        let name = display_name(&b.name, i);
        if b.kind == BodyKind::Mesh {
            meshes.push(shorten_for_display(&name));
        } else {
            solids.push((name, b.solid.clone()));
        }
    }
    (solids, meshes)
}

/// Stand des Schreibens der Datei.
#[derive(Debug, PartialEq)]
pub enum WriteState {
    Idle,
    Writing,
    Done(PathBuf, u64),
    Failed(String),
}

/// Vernetzt die Körper (läuft im Arbeitsthread); bricht zwischen den Körpern ab, wenn
/// gewünscht (Plan P15).
pub fn prepare(
    items: &[ExportItem],
    quality: Quality,
    progress: &Progress,
) -> Result<Prepared, String> {
    progress.set_total(items.len() as u64);
    let mut bodies = Vec::with_capacity(items.len());
    for item in items {
        progress.check().map_err(|e| e.to_string())?;
        let body = match &item.source {
            ExportSource::Solid(s) => {
                let options = TessellationOptions {
                    chord_tolerance: quality.chord_tolerance(),
                };
                let mesh = tessellate(s, &options)
                    .map_err(|e| format!("„{}“ lässt sich nicht vernetzen: {e}", item.name))?;
                collect_body(&SolidSource {
                    name: &item.name,
                    mesh: &mesh,
                })
            }
            ExportSource::Mesh(m, t) => collect_body(&PlacedSource {
                name: &item.name,
                mesh: m,
                transform: *t,
            }),
        }
        .map_err(|e| format!("„{}“: {e}", item.name))?;
        bodies.push(body);
        progress.advance(1);
    }
    let reports: Vec<MeshReport> = bodies.iter().map(|b| check_mesh(&b.mesh)).collect();
    Ok(Prepared {
        report: MeshReport::combine(&reports),
        bodies,
    })
}

/// Prüft die Körper für STEP (läuft im Arbeitsthread): Netz-Körper werden ausgelassen und
/// genannt, jeder exakte Körper durchläuft `check_body`; abgelehnte stehen mit Grund in der
/// Liste. Bricht zwischen den Körpern und in großen Körpern ab, wenn gewünscht.
pub fn prepare_step(items: &[ExportItem], progress: &Progress) -> Result<StepPrepared, String> {
    progress.set_total(items.len() as u64);
    let mut out = StepPrepared {
        bodies: Vec::new(),
        faces: 0,
        rejected: Vec::new(),
        skipped_meshes: Vec::new(),
        shortened: Vec::new(),
    };
    for (index, item) in items.iter().enumerate() {
        progress.check().map_err(|e| e.to_string())?;
        // Leere Namen werden „Körper n“, damit Meldungen nie „„“ lauten; lange Namen erscheinen
        // in Meldungen und Listen gekürzt.
        let name = display_name(&item.name, index);
        let shown = shorten_for_display(&name);
        match &item.source {
            ExportSource::Mesh(..) => out.skipped_meshes.push(shown),
            ExportSource::Solid(s) => {
                if name.chars().count() > MAX_NAME_CHARS {
                    out.shortened.push(shown.clone());
                }
                let body = StepBody {
                    name: shown,
                    solid: s,
                };
                match check_body(&body, &StepOptions::new("x.step"), progress) {
                    Ok(stats) => {
                        out.faces += stats.faces;
                        out.bodies.push((name, s.clone()));
                    }
                    Err(e @ StepError::InvalidBody { .. }) => out.rejected.push(e.to_string()),
                    Err(e) => return Err(e.to_string()),
                }
            }
        }
        progress.advance(1);
    }
    Ok(out)
}

/// Kürzt einen Namen für Meldungen auf `MAX_SHOWN_NAME` Zeichen (mit „…“).
pub fn shorten_for_display(name: &str) -> String {
    if name.chars().count() > MAX_SHOWN_NAME {
        let head: String = name.chars().take(MAX_SHOWN_NAME).collect();
        format!("{head}…")
    } else {
        name.to_string()
    }
}

/// Hinweis auf Namen, die in der Datei gekürzt wurden.
pub fn shortened_text(names: &[String]) -> String {
    let list: Vec<String> = names.iter().map(|n| format!("„{n}“")).collect();
    format!(
        "Name gekürzt (höchstens {MAX_NAME_CHARS} Zeichen in der Datei): {}",
        list.join(", ")
    )
}

/// Endung `.stp` wird bei STEP zu `.step` (statt `x.stp.step`). Bei Zeichnungen wird die Endung
/// `.svg` bzw. `.dxf` erzwungen: in beliebiger Schreibweise klein geschrieben, sonst angehängt.
pub fn normalize_target(format: ExportFormat, path: PathBuf) -> PathBuf {
    if let Some(f) = format.drawing_format() {
        let ext = f.extension();
        return match path.extension() {
            Some(e) if e.eq_ignore_ascii_case(ext) => path.with_extension(ext),
            _ => {
                let mut s = path.into_os_string();
                s.push(".");
                s.push(ext);
                PathBuf::from(s)
            }
        };
    }
    let is_stp = path
        .extension()
        .is_some_and(|e| e.to_string_lossy().eq_ignore_ascii_case("stp"));
    if format.is_step() && is_stp {
        path.with_extension(ExportFormat::Step.extension())
    } else {
        path
    }
}

/// Hinweis am gesperrten Knopf „Exportieren …“: bei STEP ohne Körper und bei einer Zeichnung,
/// die sich nicht berechnen lässt, der Grund, sonst Warten.
pub fn disabled_hint(status: &ExportStatus) -> String {
    match status {
        ExportStatus::StepReady { bodies: 0, .. } => STEP_NOTHING.to_string(),
        ExportStatus::Unavailable(m) => m.clone(),
        _ => "Erst möglich, wenn die Berechnung fertig ist.".to_string(),
    }
}

/// Schreibt geprüfte Körper als STEP-Datei nach `path` (Hilfsdatei, Fortschritt, Abbrechen);
/// liefert die Dateigröße.
pub fn write_step_bodies(
    path: &Path,
    prepared: &StepPrepared,
    progress: &Progress,
) -> Result<u64, StepError> {
    let bodies: Vec<StepBody> = prepared
        .bodies
        .iter()
        .map(|(name, solid)| StepBody {
            name: name.clone(),
            solid,
        })
        .collect();
    let opts = StepOptions::new(&file_name(path));
    export_step_file(path, &bodies, &opts, progress).map(|(_, bytes)| bytes)
}

/// Kurzbeschreibung eines Netzformats für die Auswahl.
pub fn format_label(f: Format) -> &'static str {
    match f {
        Format::Stl => "STL – für den 3D-Druck",
        Format::Obj => "OBJ – für viele 3D-Programme",
        Format::ThreeMf => "3MF – für den 3D-Druck, mit Namen",
        Format::Glb => "glTF (.glb) – zum Ansehen und Zeigen",
    }
}

/// Hinweis auf ausgelassene Netz-Körper (STEP-Spec E3).
pub fn skipped_text(names: &[String]) -> String {
    let list: Vec<String> = names.iter().map(|n| format!("„{n}“")).collect();
    format!(
        "Netz-Körper werden als STEP nicht ausgegeben: {}. Für sie bitte STL, OBJ, 3MF oder glTF wählen.",
        list.join(", ")
    )
}

/// Zahl mit Leerzeichen als Tausendertrenner („12 345“).
pub fn group_digits(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(' ');
        }
        out.push(c);
    }
    out
}

/// Größe in Megabyte mit Komma; unter 0,1 MB „unter 0,1 MB“.
pub fn mb_text(bytes: u64) -> String {
    let mb = bytes as f64 / BYTES_PER_MB;
    if mb < MIN_SHOWN_MB {
        "unter 0,1 MB".into()
    } else {
        format!("{mb:.1} MB").replace('.', ",")
    }
}

/// Geschätzte Größe: „ca. 1,2 MB“ (sehr kleine Dateien „unter 0,1 MB“).
pub fn estimate_text(bytes: u64) -> String {
    let t = mb_text(bytes);
    if t.starts_with("unter") {
        t
    } else {
        format!("ca. {t}")
    }
}

/// Dateiname ohne Ordner.
pub(crate) fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Stand, zu dem ein Ergebnis gehört: Qualität (`None` bei STEP), Umfang, Änderungszahl des
/// Dokuments, gewählter Körper.
type Key = (Option<Quality>, Scope, u64, Option<BodyRef>);
type PrepJob = Job<Result<Prepared, String>>;
type PrepResult = Result<Arc<Prepared>, String>;
type StepJob = Job<Result<StepPrepared, String>>;
type StepResult = Result<Arc<StepPrepared>, String>;
type WriteJob = Job<Result<u64, ExchangeError>>;
type StepWriteJob = Job<Result<u64, StepError>>;
/// Stand, zu dem ein Zeichnungsergebnis gehört: Zeichnung, Änderungszahl des Dokuments (alle
/// Änderungen, auch an der Zeichnung), Format.
type DrawKey = (DrawingId, u64, DrawingFormat);
type DrawJob = Job<Result<DrawingPrepared, String>>;
type DrawResult = Result<Arc<DrawingPrepared>, String>;
type DrawWriteJob = Job<Result<u64, String>>;
/// Das Schreiben einer Zeichnung (austauschbar nur für Tests).
pub type WriteFn = fn(&Path, &SheetModel, DrawingFormat, &Progress) -> Result<u64, DrawError>;

/// Stand zur Zeit des Öffnens der Dateiauswahl.
#[derive(Clone, Copy, Debug, PartialEq)]
enum ChooserKey {
    Bodies(Key),
    Drawing(Option<DrawKey>),
}

pub struct ExportDialog {
    pub format: ExportFormat,
    pub quality: Quality,
    pub scope: Scope,
    /// Gewählte Zeichnung (Zeichnungsformate); Vorgabe die geöffnete, sonst die erste.
    pub drawing: Option<DrawingId>,
    prepared: Option<(Key, PrepResult)>,
    job: Option<(Key, PrepJob)>,
    step_prepared: Option<(Key, StepResult)>,
    step_job: Option<(Key, StepJob)>,
    drawing_prepared: Option<(DrawKey, DrawResult)>,
    drawing_job: Option<(DrawKey, DrawJob)>,
    chooser: Option<FileChooser>,
    /// Schlüssel und Format zur Zeit des Öffnens der Dateiauswahl; ändern sie sich, schließt sie.
    chooser_key: Option<ChooserKey>,
    writing: Option<(PathBuf, WriteJob)>,
    step_writing: Option<(PathBuf, StepWriteJob)>,
    drawing_writing: Option<(PathBuf, DrawWriteJob)>,
    compute: ComputeFn,
    resolve: ResolveFn,
    writer: WriteFn,
    written: Option<Result<(PathBuf, u64), String>>,
    /// Format, mit dem zuletzt geschrieben wurde; die Meldung gehört nur zu diesem Format.
    write_format: ExportFormat,
    written_format: Option<ExportFormat>,
    /// Schließen wurde abgewiesen, weil eine STEP-Datei geschrieben wird (zeigt den Hinweis).
    close_denied: bool,
    /// Rechteck des Fensters im letzten Bild (Tests der vollen Breite).
    pub last_rect: Option<egui::Rect>,
    /// Rechtecke der Karten im letzten Bild (Tests der gleichen Höhe).
    pub last_cards: Vec<egui::Rect>,
}

impl ExportDialog {
    /// Voreinstellung: STL, Normal; Umfang „gewählter Körper“, wenn einer gewählt ist.
    pub fn new(state: &AppState) -> ExportDialog {
        let selected = state.selected_body.is_some_and(|i| i < state.bodies.len());
        ExportDialog {
            format: ExportFormat::Mesh(Format::Stl),
            quality: Quality::Normal,
            scope: if selected {
                Scope::Selected
            } else {
                Scope::Visible
            },
            drawing: default_drawing(state),
            prepared: None,
            job: None,
            step_prepared: None,
            step_job: None,
            drawing_prepared: None,
            drawing_job: None,
            chooser: None,
            chooser_key: None,
            writing: None,
            step_writing: None,
            drawing_writing: None,
            compute: compute_view,
            resolve: resolve_dimensions_in_worker,
            writer: export_drawing_file,
            written: None,
            write_format: ExportFormat::Mesh(Format::Stl),
            written_format: None,
            close_denied: false,
            last_rect: None,
            last_cards: Vec::new(),
        }
    }

    fn has_selection(state: &AppState) -> bool {
        state.selected_body.is_some_and(|i| i < state.bodies.len())
    }

    /// Indizes der Körper im Umfang (Spec E4: der gewählte, sonst alle sichtbaren).
    fn chosen(&self, state: &AppState) -> Vec<usize> {
        match (self.scope, state.selected_body) {
            (Scope::Selected, Some(i)) if i < state.bodies.len() => vec![i],
            _ => (0..state.bodies.len()).collect(),
        }
    }

    /// Kopien der Körper im Umfang für den Arbeitsthread.
    pub fn items(&self, state: &AppState) -> Vec<ExportItem> {
        self.chosen(state)
            .into_iter()
            .filter_map(|i| {
                let b = &state.bodies[i];
                let source = if b.kind == BodyKind::Mesh {
                    let (mesh, t) = state.document.mesh_body(b.source.feature)?;
                    ExportSource::Mesh(Arc::clone(mesh), t)
                } else {
                    ExportSource::Solid(b.solid.clone())
                };
                Some(ExportItem {
                    name: b.name.clone(),
                    source,
                })
            })
            .collect()
    }

    fn key(&self, state: &AppState) -> Key {
        let selected = match self.scope {
            Scope::Selected => state
                .selected_body
                .and_then(|i| state.bodies.get(i))
                .map(|b| b.source),
            Scope::Visible => None,
        };
        let quality = (!self.format.is_step()).then_some(self.quality);
        (quality, self.scope, state.document.edit_count(), selected)
    }

    /// Ersetzt die Rechnung einer Ansicht (nur für Tests: Fehler, lange Rechnungen).
    #[doc(hidden)]
    pub fn set_compute(&mut self, f: ComputeFn) {
        self.compute = f;
    }

    /// Ersetzt das Auflösen der Maße im Arbeitsthread (nur für Tests: Zähler, lange Rechnungen).
    #[doc(hidden)]
    pub fn set_resolve(&mut self, f: ResolveFn) {
        self.resolve = f;
    }

    /// Ersetzt das Schreiben einer Zeichnung (nur für Tests: langes Schreiben).
    #[doc(hidden)]
    pub fn set_writer(&mut self, f: WriteFn) {
        self.writer = f;
    }

    /// Die gewählte Zeichnung; gibt es sie nicht (mehr), die Vorgabe.
    fn current_drawing<'a>(&self, state: &'a AppState) -> Option<&'a Drawing> {
        self.drawing
            .and_then(|id| state.document.drawing(id))
            .or_else(|| default_drawing(state).and_then(|id| state.document.drawing(id)))
    }

    /// Schlüssel des Zeichnungsergebnisses; `None` bei anderen Formaten oder ohne Zeichnung.
    fn drawing_key(&self, state: &AppState) -> Option<DrawKey> {
        let f = self.format.drawing_format()?;
        let d = self.current_drawing(state)?;
        Some((d.id, state.document.edit_count(), f))
    }

    /// Warum sich keine Zeichnung berechnen lässt: keine Zeichnung, keine exakten Körper (wie
    /// im Zeichnungsbereich) oder keine Ansicht.
    fn drawing_block(&self, state: &AppState) -> Option<String> {
        let Some(d) = self.current_drawing(state) else {
            return Some(NO_DRAWING.to_string());
        };
        if state.bodies.is_empty() {
            return Some(DrawError::NoBodies.to_string());
        }
        if state.bodies.iter().all(|b| b.kind == BodyKind::Mesh) {
            return Some(DrawError::MeshBodiesOnly.to_string());
        }
        if d.views.is_empty() {
            return Some(format!(
                "Die Zeichnung „{}“ hat noch keine Ansicht.",
                shorten_for_display(&d.name)
            ));
        }
        None
    }

    fn chooser_key_now(&self, state: &AppState) -> ChooserKey {
        if self.format.is_drawing() {
            ChooserKey::Drawing(self.drawing_key(state))
        } else {
            ChooserKey::Bodies(self.key(state))
        }
    }

    /// Startet die Berechnung der Zeichnung, wenn sich Zeichnung, Dokument oder Format geändert
    /// haben, und holt ein fertiges Ergebnis ab.
    fn tick_drawing(&mut self, state: &AppState, block: bool) {
        if self
            .drawing
            .is_none_or(|id| state.document.drawing(id).is_none())
        {
            self.drawing = default_drawing(state);
        }
        if let (Some(key), None) = (self.drawing_key(state), self.drawing_block(state)) {
            let fresh = self
                .drawing_prepared
                .as_ref()
                .is_some_and(|(k, _)| *k == key);
            let running = self.drawing_job.as_ref().is_some_and(|(k, _)| *k == key);
            if !fresh
                && !running
                && let Some(drawing) = self.current_drawing(state).cloned()
            {
                let (solids, meshes) = drawing_bodies(state);
                let (compute, resolve, format) = (self.compute, self.resolve, key.2);
                // Ein alter Auftrag wird dabei fallengelassen und bricht ab.
                self.drawing_job = Some((
                    key,
                    Job::spawn(move |p| {
                        let mut r = prepare_drawing_with(&drawing, &solids, compute, resolve, p)?;
                        if format == DrawingFormat::Dxf {
                            dxf_texts_fit(&r.model)?;
                        }
                        r.skipped_meshes = meshes;
                        Ok(r)
                    }),
                ));
            }
        }
        let finished = match &self.drawing_job {
            Some((k, job)) => match job.fetch(block) {
                Poll::Pending => None,
                Poll::Ready(r) => Some((*k, r.map(Arc::new))),
                Poll::Lost => Some((*k, Err(LOST.to_string()))),
            },
            None => None,
        };
        if let Some(done) = finished {
            self.drawing_prepared = Some(done);
            self.drawing_job = None;
        }
    }

    fn drawing_status(&self, state: &AppState) -> ExportStatus {
        if let Some(m) = self.drawing_block(state) {
            return ExportStatus::Unavailable(m);
        }
        let key = self.drawing_key(state);
        if let Some((k, job)) = &self.drawing_job
            && Some(*k) == key
        {
            return ExportStatus::Computing(job.fraction());
        }
        match &self.drawing_prepared {
            Some((k, Ok(p))) if Some(*k) == key => ExportStatus::DrawingReady {
                views: p.views,
                sheet: p.sheet.clone(),
                visible: p.visible,
                hidden: p.hidden,
                skipped_meshes: p.skipped_meshes.clone(),
                dimensions: p.dimensions,
                outside: p.outside,
                centerlines: p.centerlines,
            },
            Some((k, Err(m))) if Some(*k) == key => ExportStatus::Failed(m.clone()),
            _ => ExportStatus::Computing(0.0),
        }
    }

    /// Schreibt die fertige Zeichnung im Arbeitsthread nach `path` (über eine Hilfsdatei); nur
    /// wenn alle Ansichten berechnet sind und das Ergebnis zum Stand von `state` passt.
    fn write_drawing(&mut self, path: PathBuf, format: DrawingFormat, state: &AppState) {
        if let Some(m) = self.drawing_block(state) {
            self.set_written(Err(m));
            return;
        }
        let key = self.drawing_key(state);
        let prepared = match &self.drawing_prepared {
            Some((k, _)) if Some(*k) != key => {
                self.set_written(Err(STALE.to_string()));
                return;
            }
            Some((_, Ok(p))) => Arc::clone(p),
            Some((_, Err(m))) => {
                let m = m.clone();
                self.set_written(Err(m));
                return;
            }
            None => {
                self.set_written(Err(NOT_COMPUTED.to_string()));
                return;
            }
        };
        let (writer, target) = (self.writer, path.clone());
        self.written = None;
        self.drawing_writing = Some((
            path,
            Job::spawn(move |p| {
                writer(&target, &prepared.model, format, p).map_err(draw_write_text)
            }),
        ));
    }

    /// Startet das Vernetzen bzw. die STEP-Prüfung, wenn sich Format, Qualität, Umfang oder
    /// Dokument geändert haben, und holt fertige Ergebnisse ab. Mit `block` wird gewartet
    /// (Tests).
    pub fn tick(&mut self, state: &AppState, block: bool) {
        // Ohne Auswahl gibt es „Gewählter Körper“ nicht mehr (Anzeige und Export stimmen überein).
        if self.scope == Scope::Selected && !Self::has_selection(state) {
            self.scope = Scope::Visible;
        }
        let key = self.key(state);
        if self.format.is_drawing() {
            self.tick_drawing(state, block);
        } else if self.format.is_step() {
            let fresh = self.step_prepared.as_ref().is_some_and(|(k, _)| *k == key);
            let running = self.step_job.as_ref().is_some_and(|(k, _)| *k == key);
            if !fresh && !running {
                let items = self.items(state);
                self.step_job = Some((key, Job::spawn(move |p| prepare_step(&items, p))));
            }
            let finished = match &self.step_job {
                Some((k, job)) => match job.fetch(block) {
                    Poll::Pending => None,
                    Poll::Ready(r) => Some((*k, r.map(Arc::new))),
                    Poll::Lost => Some((*k, Err(LOST.to_string()))),
                },
                None => None,
            };
            if let Some(done) = finished {
                self.step_prepared = Some(done);
                self.step_job = None;
            }
        } else {
            let fresh = self.prepared.as_ref().is_some_and(|(k, _)| *k == key);
            let running = self.job.as_ref().is_some_and(|(k, _)| *k == key);
            if !fresh && !running {
                let items = self.items(state);
                let quality = self.quality;
                // Ein alter Auftrag wird dabei fallengelassen und bricht ab.
                self.job = Some((key, Job::spawn(move |p| prepare(&items, quality, p))));
            }
            let finished = match &self.job {
                Some((k, job)) => match job.fetch(block) {
                    Poll::Pending => None,
                    Poll::Ready(r) => Some((*k, r.map(Arc::new))),
                    Poll::Lost => Some((*k, Err(LOST.to_string()))),
                },
                None => None,
            };
            if let Some(done) = finished {
                self.prepared = Some(done);
                self.job = None;
            }
        }
        let wrote = match &self.writing {
            Some((path, job)) => match job.fetch(block) {
                Poll::Pending => None,
                Poll::Ready(r) => Some(r.map(|n| (path.clone(), n)).map_err(|e| e.to_string())),
                Poll::Lost => Some(Err(LOST.to_string())),
            },
            None => None,
        };
        if let Some(w) = wrote {
            self.set_written(w);
            self.writing = None;
        }
        let wrote = match &self.step_writing {
            Some((path, job)) => match job.fetch(block) {
                Poll::Pending => None,
                Poll::Ready(r) => Some(r.map(|n| (path.clone(), n)).map_err(|e| e.to_string())),
                Poll::Lost => Some(Err(LOST.to_string())),
            },
            None => None,
        };
        if let Some(w) = wrote {
            self.set_written(w);
            self.step_writing = None;
        }
        let wrote = match &self.drawing_writing {
            Some((path, job)) => match job.fetch(block) {
                Poll::Pending => None,
                Poll::Ready(r) => Some(r.map(|n| (path.clone(), n))),
                Poll::Lost => Some(Err(LOST.to_string())),
            },
            None => None,
        };
        if let Some(w) = wrote {
            self.set_written(w);
            self.drawing_writing = None;
        }
        if !self.is_writing_file() {
            self.close_denied = false;
        }
    }

    /// Läuft gerade das Schreiben einer STEP-Datei?
    pub fn is_writing_step(&self) -> bool {
        self.step_writing.is_some()
    }

    /// Läuft gerade das Schreiben einer STEP-Datei oder einer Zeichnung? Dann darf das Fenster
    /// nicht verworfen werden: Der Arbeitsthread bräche ab und die alte Datei bliebe ohne
    /// Meldung liegen.
    pub fn is_writing_file(&self) -> bool {
        self.step_writing.is_some() || self.drawing_writing.is_some()
    }

    /// Fragt, ob das Fenster geschlossen werden darf; beim Schreiben einer STEP-Datei oder
    /// Zeichnung nicht (dann steht im Fenster der Hinweis `WRITING_HINT`).
    pub fn request_close(&mut self) -> bool {
        if self.is_writing_file() {
            self.close_denied = true;
            false
        } else {
            true
        }
    }

    pub fn status(&self, state: &AppState) -> ExportStatus {
        if self.format.is_drawing() {
            return self.drawing_status(state);
        }
        let key = self.key(state);
        if self.format.is_step() {
            if let Some((k, job)) = &self.step_job
                && *k == key
            {
                return ExportStatus::Computing(job.fraction());
            }
            return match &self.step_prepared {
                Some((k, Ok(p))) if *k == key => ExportStatus::StepReady {
                    bodies: p.bodies.len(),
                    faces: p.faces,
                    rejected: p.rejected.clone(),
                    skipped_meshes: p.skipped_meshes.clone(),
                },
                Some((k, Err(m))) if *k == key => ExportStatus::Failed(m.clone()),
                _ => ExportStatus::Computing(0.0),
            };
        }
        if let Some((k, job)) = &self.job
            && *k == key
        {
            return ExportStatus::Computing(job.fraction());
        }
        match (&self.prepared, self.format) {
            (Some((k, Ok(p))), ExportFormat::Mesh(f)) if *k == key => ExportStatus::Ready {
                triangles: p.triangles(),
                bytes: f.estimate_bytes(p.triangles(), p.vertices()),
                report: p.report,
            },
            (Some((k, Err(m))), _) if *k == key => ExportStatus::Failed(m.clone()),
            _ => ExportStatus::Computing(0.0),
        }
    }

    /// Merkt sich das Ergebnis des Schreibens samt Format, mit dem geschrieben wurde.
    fn set_written(&mut self, w: Result<(PathBuf, u64), String>) {
        self.written = Some(w);
        self.written_format = Some(self.write_format);
    }

    /// Namen, die in der geprüften STEP-Datei gekürzt werden (leer ohne gültiges Ergebnis).
    pub fn shortened_names(&self, state: &AppState) -> Vec<String> {
        match &self.step_prepared {
            Some((k, Ok(p))) if *k == self.key(state) => p.shortened.clone(),
            _ => Vec::new(),
        }
    }

    pub fn write_state(&self) -> WriteState {
        if self.writing.is_some() || self.is_writing_file() {
            return WriteState::Writing;
        }
        if self.written_format != Some(self.format) {
            return WriteState::Idle;
        }
        match &self.written {
            None => WriteState::Idle,
            Some(Ok((p, n))) => WriteState::Done(p.clone(), *n),
            Some(Err(m)) => WriteState::Failed(m.clone()),
        }
    }

    /// Bricht das Vernetzen bzw. die Prüfung ab; das Fenster zeigt „Abgebrochen“, bis neu
    /// berechnet wird.
    pub fn cancel(&mut self) {
        // Nur der Auftrag des gewählten Formats; der andere läuft im Hintergrund weiter.
        if self.format.is_drawing() {
            if let Some((key, _)) = self.drawing_job.take() {
                self.drawing_prepared = Some((key, Err(ExchangeError::Cancelled.to_string())));
            }
        } else if self.format.is_step() {
            if let Some((key, _)) = self.step_job.take() {
                self.step_prepared = Some((key, Err(StepError::Cancelled.to_string())));
            }
        } else if let Some((key, _)) = self.job.take() {
            self.prepared = Some((key, Err(ExchangeError::Cancelled.to_string())));
        }
    }

    /// Erledigter Anteil beim Schreiben einer STEP-Datei oder Zeichnung (Netzformate: 0).
    pub fn write_progress(&self) -> f32 {
        if let Some((_, job)) = &self.drawing_writing {
            return job.fraction();
        }
        self.step_writing
            .as_ref()
            .map_or(0.0, |(_, job)| job.fraction())
    }

    /// Bricht das Schreiben einer STEP-Datei oder Zeichnung ab; die Zieldatei bleibt
    /// unverändert.
    pub fn cancel_write(&mut self) {
        if let Some((_, job)) = &self.step_writing {
            job.cancel();
        }
        if let Some((_, job)) = &self.drawing_writing {
            job.cancel();
        }
    }

    /// Verwirft das Ergebnis des gewählten Formats; das nächste `tick` rechnet neu. Das Ergebnis
    /// des anderen Formats bleibt (Wechsel zurück rechnet nicht neu).
    pub fn recompute(&mut self) {
        if self.format.is_drawing() {
            self.drawing_prepared = None;
        } else if self.format.is_step() {
            self.step_prepared = None;
        } else {
            self.prepared = None;
        }
    }

    /// Öffnet die Dateiauswahl im Ordner `dir` mit dem Namen des Körpers (bei mehreren
    /// „Körper“) bzw. der Zeichnung; Zeichen, die in Dateinamen nicht erlaubt sind, werden „_“.
    pub fn begin_save(&mut self, dir: PathBuf, state: &AppState) {
        let stem = if self.format.is_drawing() {
            self.current_drawing(state)
                .map_or_else(|| "Zeichnung".to_string(), |d| d.name.clone())
        } else {
            let names: Vec<&str> = self
                .chosen(state)
                .into_iter()
                .filter_map(|i| state.bodies.get(i).map(|b| b.name.as_str()))
                .collect();
            match names.as_slice() {
                [one] => (*one).to_string(),
                _ => "Körper".to_string(),
            }
        };
        let stem: String = stem
            .chars()
            .map(|c| {
                if c.is_control() || "\\/:*?\"<>|".contains(c) {
                    '_'
                } else {
                    c
                }
            })
            .collect();
        self.chooser_key = Some(self.chooser_key_now(state));
        self.chooser = Some(FileChooser::save(dir, stem, self.format.extension()));
    }

    pub fn chooser(&self) -> Option<&FileChooser> {
        self.chooser.as_ref()
    }

    /// Schreibt das fertige Ergebnis im Arbeitsthread nach `path` (über eine Hilfsdatei).
    /// Schreibt nur, wenn das Ergebnis noch zum Stand von `state` passt; sonst gibt es eine
    /// Meldung.
    pub fn write_to(&mut self, path: PathBuf, state: &AppState) {
        let key = self.key(state);
        self.write_format = self.format;
        let path = normalize_target(self.format, path);
        match self.format {
            ExportFormat::Drawing(format) => self.write_drawing(path, format, state),
            ExportFormat::Step => {
                let Some((k, Ok(prepared))) = &self.step_prepared else {
                    self.set_written(Err(NOT_COMPUTED.to_string()));
                    return;
                };
                if *k != key {
                    self.set_written(Err(STALE.to_string()));
                    return;
                }
                if prepared.bodies.is_empty() {
                    self.set_written(Err(StepError::NothingToExport.to_string()));
                    return;
                }
                let (prepared, target) = (Arc::clone(prepared), path.clone());
                self.written = None;
                self.step_writing = Some((
                    path,
                    Job::spawn(move |p| write_step_bodies(&target, &prepared, p)),
                ));
            }
            ExportFormat::Mesh(format) => {
                let Some((k, Ok(prepared))) = &self.prepared else {
                    self.set_written(Err(NOT_COMPUTED.to_string()));
                    return;
                };
                if *k != key {
                    self.set_written(Err(STALE.to_string()));
                    return;
                }
                let (prepared, target) = (Arc::clone(prepared), path.clone());
                self.written = None;
                self.writing = Some((
                    path,
                    Job::spawn(move |_| export_file(&target, format, &prepared.bodies)),
                ));
            }
        }
    }

    /// Übernimmt die in der Dateiauswahl gewählte Datei. Wurde bei STEP `.stp` eingegeben und
    /// gibt es die Datei mit `.step` schon, fragt die Auswahl noch einmal nach dem Ersetzen.
    pub fn accept_chosen(&mut self, path: PathBuf, state: &AppState) {
        let target = normalize_target(self.format, path.clone());
        if target != path
            && target.exists()
            && let Some(c) = self.chooser.as_mut()
        {
            c.select(&file_name(&target));
            c.confirm();
            return;
        }
        self.chooser = None;
        self.chooser_key = None;
        self.write_to(target, state);
    }

    /// Zeichnet das Fenster; `true`, solange es offen bleiben soll. `last_dir` ist der zuletzt
    /// benutzte Ordner der Dateiauswahl.
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        state: &mut AppState,
        last_dir: &mut Option<PathBuf>,
    ) -> bool {
        self.tick(state, false);
        let status = self.status(state);
        let write_state = self.write_state();
        let write_progress = self.write_progress();
        let denied = self.close_denied;
        let shortened = if matches!(write_state, WriteState::Done(..)) && self.format.is_step() {
            self.shortened_names(state)
        } else {
            Vec::new()
        };
        let has_selection = Self::has_selection(state);
        let before = self.format;
        let (mut format, mut quality, mut scope) = (self.format, self.quality, self.scope);
        // Karte „Zeichnung“: Auswahl und Angaben (Linien erst, wenn alles berechnet ist).
        let drawing_list: Vec<(DrawingId, String)> = state
            .document
            .drawings()
            .iter()
            .map(|d| (d.id, shorten_for_display(&d.name)))
            .collect();
        let current = self.current_drawing(state);
        let mut picked = current.map(|d| d.id);
        let picked_before = picked;
        let mut drawing_info: Vec<String> = Vec::new();
        if let Some(d) = current {
            let views = match &status {
                ExportStatus::DrawingReady { views, .. } => *views,
                _ => d.views.len(),
            };
            drawing_info.push(format!("Ansichten: {views}"));
            drawing_info.push(format!("Blatt: {}", sheet_label(d.sheet)));
            drawing_info.push(match &status {
                ExportStatus::DrawingReady {
                    visible, hidden, ..
                } => format!(
                    "Linien: {} sichtbar, {} verdeckt",
                    group_digits(*visible as u64),
                    group_digits(*hidden as u64)
                ),
                _ => "Linien: …".to_string(),
            });
            // Maße und Mittellinien: aus der Zeichnung, nach der Berechnung aus dem Ergebnis.
            let (dims, centerlines) = match &status {
                ExportStatus::DrawingReady {
                    dimensions,
                    centerlines,
                    ..
                } => (*dimensions, *centerlines),
                _ => (d.dimensions.len(), centerline_use(d)),
            };
            drawing_info.push(format!("Maße: {}", group_digits(dims as u64)));
            if let ExportStatus::DrawingReady { outside, .. } = &status
                && *outside > 0
            {
                drawing_info.push(format!(
                    "Maße außerhalb des Blatts: {}",
                    group_digits(*outside as u64)
                ));
            }
            drawing_info.push(format!("Mittellinien: {}", centerlines.label()));
        }
        let writing_file = self.is_writing_file();
        let (mut open, mut close, mut cancel, mut recompute, mut begin, mut drop_chooser) =
            (true, false, false, false, false, false);
        let mut cancel_write = false;
        let mut chosen: Option<PathBuf> = None;
        let mut cards = Vec::new();
        let chooser = &mut self.chooser;
        let response = wide_window("Exportieren", ctx)
            .open(&mut open)
            .show(ctx, |ui| {
                let mut format_card = |ui: &mut egui::Ui| {
                    ui.strong("Format");
                    for f in ExportFormat::ALL {
                        ui.radio_value(&mut format, f, f.label());
                    }
                };
                let mut quality_card = |ui: &mut egui::Ui| {
                    if before.is_step() {
                        ui.label(STEP_EXACT);
                    } else {
                        ui.strong("Qualität");
                        for q in Quality::ALL {
                            ui.radio_value(&mut quality, q, q.label());
                        }
                        ui.weak("Feiner heißt mehr Dreiecke und größere Dateien.");
                    }
                };
                let mut scope_card = |ui: &mut egui::Ui| {
                    ui.strong("Umfang");
                    let selected = ui
                        .add_enabled(
                            has_selection,
                            egui::RadioButton::new(scope == Scope::Selected, "Gewählter Körper"),
                        )
                        .on_disabled_hover_text("Dafür zuerst einen Körper wählen.");
                    if selected.clicked() {
                        scope = Scope::Selected;
                    }
                    ui.radio_value(&mut scope, Scope::Visible, "Alle sichtbaren Körper");
                    ui.weak("Unterdrückte Schritte zählen nicht.");
                };
                let mut drawing_card = |ui: &mut egui::Ui| {
                    ui.strong("Zeichnung");
                    if drawing_list.is_empty() {
                        ui.weak("Keine Zeichnung vorhanden.");
                    } else {
                        let shown = drawing_list
                            .iter()
                            .find(|(id, _)| Some(*id) == picked)
                            .map_or("", |(_, n)| n.as_str());
                        egui::ComboBox::from_id_salt("export-drawing")
                            .selected_text(shown)
                            .width(ui.available_width())
                            .show_ui(ui, |ui| {
                                for (id, name) in &drawing_list {
                                    ui.selectable_value(&mut picked, Some(*id), name.as_str());
                                }
                            });
                    }
                    for line in &drawing_info {
                        ui.label(line);
                    }
                };
                cards = if before.is_drawing() {
                    let row: &mut [&mut dyn FnMut(&mut egui::Ui)] =
                        &mut [&mut format_card, &mut drawing_card];
                    card_row(ui, "export-drawing", row)
                } else {
                    let row: &mut [&mut dyn FnMut(&mut egui::Ui)] =
                        &mut [&mut format_card, &mut quality_card, &mut scope_card];
                    card_row(ui, "export", row)
                };
                ui.add_space(8.0);
                match &status {
                    ExportStatus::Computing(f) => {
                        let text = if before.is_drawing() {
                            "Die Ansichten werden berechnet …"
                        } else if before.is_step() {
                            "Die Körper werden geprüft …"
                        } else {
                            "Das Netz wird berechnet …"
                        };
                        ui.horizontal(|ui| {
                            ui.add(
                                egui::ProgressBar::new(*f)
                                    .desired_width(PROGRESS_WIDTH)
                                    .text(text),
                            );
                            if ui.button("Abbrechen").clicked() {
                                cancel = true;
                            }
                        });
                    }
                    ExportStatus::Failed(m) => {
                        ui.colored_label(ERROR_COLOR, m);
                        if ui.button("Neu berechnen").clicked() {
                            recompute = true;
                        }
                    }
                    ExportStatus::Ready {
                        triangles,
                        bytes,
                        report,
                    } => {
                        ui.label(format!(
                            "{} Dreiecke, {}",
                            group_digits(*triangles),
                            estimate_text(*bytes)
                        ));
                        if report.closed {
                            ui.label(report_text(report));
                        } else {
                            ui.colored_label(WARNING_COLOR, report_text(report));
                            ui.colored_label(WARNING_COLOR, NOT_CLOSED_WARNING);
                        }
                    }
                    ExportStatus::StepReady {
                        bodies,
                        faces,
                        rejected,
                        skipped_meshes,
                    } => {
                        if *bodies == 0 {
                            ui.colored_label(WARNING_COLOR, STEP_NOTHING);
                        } else {
                            ui.label(format!(
                                "{} Körper, {} Flächen",
                                group_digits(*bodies as u64),
                                group_digits(*faces as u64)
                            ));
                        }
                        for r in rejected {
                            ui.colored_label(WARNING_COLOR, r);
                        }
                        if !skipped_meshes.is_empty() {
                            ui.colored_label(WARNING_COLOR, skipped_text(skipped_meshes));
                        }
                    }
                    ExportStatus::DrawingReady {
                        skipped_meshes,
                        outside,
                        ..
                    } => {
                        ui.label("Alle Ansichten sind berechnet.");
                        if *outside > 0 {
                            ui.colored_label(WARNING_COLOR, outside_text(*outside));
                        }
                        if !skipped_meshes.is_empty() {
                            ui.colored_label(WARNING_COLOR, drawing_skipped_text(skipped_meshes));
                        }
                    }
                    ExportStatus::Unavailable(m) => {
                        ui.colored_label(WARNING_COLOR, m);
                    }
                }
                match &write_state {
                    WriteState::Idle => {}
                    WriteState::Writing => {
                        ui.horizontal(|ui| {
                            ui.add(
                                egui::ProgressBar::new(write_progress)
                                    .desired_width(PROGRESS_WIDTH)
                                    .text("Die Datei wird geschrieben …"),
                            );
                            if writing_file && ui.button("Abbrechen").clicked() {
                                cancel_write = true;
                            }
                        });
                        if denied {
                            ui.colored_label(WARNING_COLOR, WRITING_HINT);
                        }
                    }
                    WriteState::Done(path, n) => {
                        ui.label(format!(
                            "Fertig: „{}“ geschrieben ({}).",
                            file_name(path),
                            mb_text(*n)
                        ));
                        if !shortened.is_empty() {
                            ui.colored_label(WARNING_COLOR, shortened_text(&shortened));
                        }
                    }
                    WriteState::Failed(m) => {
                        ui.colored_label(ERROR_COLOR, format!("Nicht geschrieben: {m}"));
                    }
                }
                ui.separator();
                match chooser.as_mut() {
                    Some(c) => {
                        ui.strong("Speichern unter");
                        match c.show(ui) {
                            ChooserAction::Chosen(p) => chosen = Some(p),
                            ChooserAction::Cancel => drop_chooser = true,
                            ChooserAction::None => {}
                        }
                    }
                    None => {
                        let ready = match &status {
                            ExportStatus::Ready { .. } => true,
                            ExportStatus::StepReady { bodies, .. } => *bodies > 0,
                            ExportStatus::DrawingReady { .. } => true,
                            _ => false,
                        } && write_state != WriteState::Writing;
                        ui.horizontal(|ui| {
                            if ui
                                .add_enabled(ready, egui::Button::new("Exportieren …"))
                                .on_disabled_hover_text(disabled_hint(&status))
                                .clicked()
                            {
                                begin = true;
                            }
                            if ui.button("Schließen").clicked() {
                                close = true;
                            }
                        });
                    }
                }
            });
        let want_close = !open || close;
        let alive = !want_close || !self.request_close();
        self.last_rect = response.map(|r| r.response.rect);
        self.last_cards = cards;
        (self.format, self.quality, self.scope) = (format, quality, scope);
        let other_drawing = picked != picked_before;
        if other_drawing {
            self.drawing = picked;
        }
        let stale_chooser = self
            .chooser_key
            .is_some_and(|k| k != self.chooser_key_now(state));
        if self.format != before || drop_chooser || stale_chooser {
            self.chooser = None;
            self.chooser_key = None;
        }
        if self.format != before || stale_chooser || other_drawing {
            // Die Meldung „Fertig“ gehörte zum alten Format oder Stand.
            self.written = None;
        }
        let busy = matches!(status, ExportStatus::Computing(_))
            || self.write_state() == WriteState::Writing;
        if busy {
            ctx.request_repaint_after(REPAINT);
        }
        if cancel {
            self.cancel();
        }
        if cancel_write {
            self.cancel_write();
        }
        if recompute {
            self.recompute();
        }
        if begin {
            let dir = last_dir.clone().unwrap_or_else(default_dir);
            self.begin_save(dir, state);
        }
        if let Some(path) = chosen {
            *last_dir = path.parent().map(Path::to_path_buf);
            self.accept_chosen(path, state);
        }
        alive
    }
}
