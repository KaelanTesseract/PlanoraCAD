//! Fenster „Exportieren“ (Spec §8 des Netz-Austauschs, Spec §6 des STEP-Exports): Format,
//! Qualität und Umfang als gleich hohe Karten über die ganze Breite; das Netz bzw. die Prüfung
//! der Körper entsteht im Arbeitsthread (Fortschritt, Abbrechen); danach Zahl der Dreiecke,
//! geschätzte Größe und Prüfbericht (Netz) bzw. Zahl der Körper und Flächen (STEP); gespeichert
//! wird über die eigene Dateiauswahl (Plan P14) und eine Hilfsdatei.
//!
//! Zeichnungen (Teilprojekt 11, Aufgabe 17) als sechstes und siebtes Format: Die Ansichten der
//! gewählten Zeichnung werden im eigenen Arbeitsthread des Fensters berechnet (nicht über den
//! Zwischenspeicher des Zeichnungsbereichs, Festlegung F11); geschrieben wird nur, wenn alle
//! Ansichten berechnet sind. Als achtes Format kommt „Zeichnung als PDF“ dazu (Teilprojekt 3c-3):
//! gleiche Berechnung wie bei SVG, vor dem Schreiben prüft der Auftrag, ob alle Zeichen in der
//! PDF darstellbar sind (P13), und die Datei bekommt Dokumenteigenschaften ([`pdf_meta`], P11).
//! Bei PDF lässt sich statt des gewählten Blatts „Alle Blätter“ wählen: Ein Auftrag rechnet alle
//! Zeichnungen in Dokumentreihenfolge ([`prepare_all_sheets`]); kann ein Blatt nicht ausgegeben
//! werden, ist die ganze PDF gesperrt und die Meldung nennt das erste solche Blatt (F2).
//!
//! Maße und Mittellinien (Teilprojekt 12, Aufgabe 14): Nach den Ansichten löst derselbe
//! Arbeitsthread die Maße der Zeichnung gegen seine Kopien der exakten Körper auf (nicht über den
//! Zwischenspeicher der Oberfläche) und baut das Blatt mit `build_sheet_with`. Hat ein Maß seinen
//! Bezug verloren, wird nichts geschrieben (Festlegung P13); Maße außerhalb des Blatts werden
//! geschrieben und in der Karte gezählt (P12).

use crate::browser::error_color;
use crate::cards::card_row;
use crate::dialog::{Dialog, DialogSize};
use crate::dimension_dialog::kind_label;
use crate::dimension_state::count_outside;
use crate::drawing_dialog::paper_label;
use crate::drawing_state::{ComputeFn, WorkArea};
use crate::drawing_view::{context_or_single, parts_on, parts_space_for, view_names};
use crate::file_chooser::{ChooserAction, FileChooser, default_dir};
use crate::jobs::{Job, Poll};
use crate::mesh_body::{report_text, warning_color};
use crate::places::Places;
use crate::sheet_dialog::title_field_name;
use crate::state::{AppState, BodyKind};
use crate::theme;
use planora_drawing::annotate::{Mark, ResolvedDims};
use planora_drawing::balloon::{add_balloon_marks, balloon_marks};
use planora_drawing::dimension::{DimError, DimSpec};
use planora_drawing::dxf::{dxf_char_ok, dxf_text_fits};
use planora_drawing::parts::PARTS_HEAD;
use planora_drawing::pdf::{MAX_PAGES, PdfMeta, pdf_date};
use planora_drawing::pdf_font::pdf_char_ok;
use planora_drawing::section_marks::{add_section_marks, section_marks};
use planora_drawing::sheet::{SheetModel, build_sheet_with, dimension_marks};
use planora_drawing::sheet_frame::{PartsSpace, sheet_layout_with};
use planora_drawing::sheet_info::{
    FieldValue, ProjectInfo, SheetContext, Stamp, TitleTexts, main_scale, resolve_texts,
};
use planora_drawing::sheet_marks::{
    DXF_TEXT_WIDTH_FACTOR, SheetFrame, parts_table_marks, sheet_frame, sheet_frame_dxf,
};
use planora_drawing::{
    BalloonId, DrawBody, DrawError, Drawing, DrawingFormat, DrawingId, DrawingKind, PartsCells,
    SectionData, Sheet, ViewId, ViewKey, ViewLines, Visibility, compute_section, compute_view,
    export_drawing_file, export_pdf_file,
};
use planora_exchange::{
    ExchangeError, ExchangeMesh, ExportBody, Format, MeshReport, MeshTransform, PlacedSource,
    Progress, SolidSource, check_mesh, collect_body, export_file,
};
use planora_feature::{BodyRef, resolve_drawing_dimensions_views};
use planora_kernel::Solid;
use planora_mesh::{TessellationOptions, tessellate};
use planora_step::{
    MAX_NAME_CHARS, StepBody, StepError, StepOptions, check_body, display_name, export_step_file,
};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering as AtomicOrdering};
use std::time::Duration;

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
/// Schalter in der Karte „Zeichnung“: Rahmen, Schriftfeld und Tabellen des Blatts mit ausgeben.
pub const FRAME_SWITCH: &str = "Blattrahmen mit ausgeben";
/// Hinweis bei den Zeichnungsformaten, wenn das Dokument keine Zeichnung hat.
pub const NO_DRAWING: &str =
    "Es gibt noch keine Zeichnung. Über „Zeichnung“ im Menü lässt sich eine anlegen.";
/// So oft schaut der Arbeitsthread nach, ob die Berechnung einer Ansicht abgebrochen werden soll.
const WATCH: Duration = Duration::from_millis(5);

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
    pub const ALL: [ExportFormat; 8] = [
        ExportFormat::Mesh(Format::Stl),
        ExportFormat::Mesh(Format::Obj),
        ExportFormat::Mesh(Format::ThreeMf),
        ExportFormat::Mesh(Format::Glb),
        ExportFormat::Step,
        ExportFormat::Drawing(DrawingFormat::Svg),
        ExportFormat::Drawing(DrawingFormat::Dxf),
        ExportFormat::Drawing(DrawingFormat::Pdf),
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
    /// Die Texte des Blattrahmens (nur mit Blattrahmen).
    pub texts: Option<TitleTexts>,
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
    centerline_use_all(std::slice::from_ref(d))
}

/// Wie [`centerline_use`] über die Normansichten aller Zeichnungen `drawings` (Blätter ohne
/// Normansicht zählen nicht).
pub fn centerline_use_all(drawings: &[Drawing]) -> CenterlineUse {
    let flags: Vec<bool> = drawings
        .iter()
        .flat_map(|d| &d.views)
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

/// Die geschnittenen Körper je Schnittansicht (Spec §9.3: Maße einer Schnittansicht hängen an
/// ihnen).
pub type ViewSolids<'a> = BTreeMap<ViewId, Vec<&'a Solid>>;

/// Das Auflösen der Maße einer Zeichnung im Arbeitsthread (austauschbar nur für Tests).
pub type ResolveFn =
    fn(&Drawing, &[&Solid], &ViewSolids<'_>, &Progress) -> Result<ResolvedDims, DrawError>;

/// Löst die Maße der Zeichnung gegen die Körper `solids` auf (Kopien im Arbeitsthread), Maße
/// einer Schnittansicht gegen deren geschnittene Körper aus `view_solids`; bricht zwischen den
/// Maßen ab, wenn gewünscht.
pub fn resolve_dimensions_in_worker(
    drawing: &Drawing,
    solids: &[&Solid],
    view_solids: &ViewSolids<'_>,
    progress: &Progress,
) -> Result<ResolvedDims, DrawError> {
    resolve_drawing_dimensions_views(drawing, solids, view_solids, || {
        progress.check().map_err(|_| DrawError::Cancelled)
    })
    .map(|(dims, _)| dims)
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
    if let Some(frame) = &model.frame {
        for m in &frame.marks {
            if let Mark::Text { text, .. } = m
                && !dxf_text_fits(text)
            {
                return Err(
                    "Ein Text im Blattrahmen ist für DXF zu lang. Bitte die Angaben kürzen."
                        .to_string(),
                );
            }
        }
    }
    Ok(())
}

/// Lassen sich alle Texte des Blattrahmens als DXF ausgeben? Sonst die Meldung mit dem Feld
/// (nichts wird still gekürzt oder ersetzt): zuerst Zeichen, die die Datei nicht darstellt (als
/// Zeichennummer „U+1F600“, weil die Schrift ein solches Zeichen oft nur als leeres Kästchen
/// zeigt), dann zu lange Texte.
pub fn dxf_sheet_texts_ok(texts: &TitleTexts) -> Result<(), String> {
    check_sheet_cells(texts, &|name, text| {
        if let Some(c) = text.chars().find(|c| !dxf_char_ok(*c)) {
            return Err(format!(
                "Das Zeichen U+{:04X} im Feld „{name}“ lässt sich als DXF nicht ausgeben. Bitte ändern.",
                c as u32
            ));
        }
        if !dxf_text_fits(text) {
            return Err(format!(
                "Der Text im Feld „{name}“ ist für DXF zu lang. Bitte kürzen."
            ));
        }
        Ok(())
    })
}

/// Geht alle Zellen des Blattrahmens mit ihrem Feldnamen durch (Felder des Schriftfelds, Zeilen
/// der Änderungstabelle, Hinweise, Zellen der Stückliste mit Position und Spalte); `check(Name,
/// Text)` meldet das erste Problem. Gemeinsam für DXF und PDF, damit beide dieselben Feldnamen
/// nennen.
fn check_sheet_cells(
    texts: &TitleTexts,
    check: &dyn Fn(&str, &str) -> Result<(), String>,
) -> Result<(), String> {
    for (field, text) in &texts.fields {
        check(title_field_name(*field), text)?;
    }
    for row in texts.visible_revisions() {
        let name = format!("Änderung {}", row[0]);
        for cell in row {
            check(&name, cell)?;
        }
    }
    for (i, note) in texts.notes.iter().enumerate() {
        check(&format!("Hinweis {}", i + 1), note)?;
    }
    for row in &texts.parts {
        for (cell, head) in row.iter().zip(PARTS_HEAD).skip(1) {
            check(&format!("Stückliste, Pos. {}, {head}", row[0]), cell)?;
        }
    }
    Ok(())
}

/// Meldung zu einem Zeichen im Feld `field`, das sich in einer PDF nicht ausgeben lässt (Spec §4).
pub fn pdf_char_text(c: char, field: &str) -> String {
    format!(
        "Das Zeichen {} im Feld „{field}“ lässt sich in einer PDF nicht ausgeben.",
        planora_drawing::pdf::pdf_char_label(c)
    )
}

/// Lassen sich alle Texte des Blattrahmens in einer PDF ausgeben (P13)? Sonst die Meldung mit
/// dem Feld, genau wie beim DXF benannt; nichts wird gekürzt oder ersetzt. Die Länge ist bei PDF
/// kein Hindernis (die Texte werden beim Bau des Blatts eingepasst).
pub fn pdf_sheet_texts_ok(texts: &TitleTexts) -> Result<(), String> {
    check_sheet_cells(
        texts,
        &|name, text| match text.chars().find(|c| !pdf_char_ok(*c)) {
            Some(c) => Err(pdf_char_text(c, name)),
            None => Ok(()),
        },
    )
}

/// Lassen sich alle Texte der Ansichten (Maßtexte, Positionsnummern) in einer PDF ausgeben (P13)?
/// Sonst die Meldung mit dem Zeichen und der Ansicht.
pub fn pdf_model_texts_ok(model: &SheetModel) -> Result<(), String> {
    for v in &model.views {
        for m in &v.marks {
            if let Mark::Text { text, .. } = m
                && let Some(c) = text.chars().find(|c| !pdf_char_ok(*c))
            {
                return Err(format!(
                    "Das Zeichen {} in einem Maßtext der Ansicht „{}“ lässt sich in einer PDF nicht ausgeben.",
                    planora_drawing::pdf::pdf_char_label(c),
                    v.name
                ));
            }
        }
    }
    Ok(())
}

/// Welche Blätter in die PDF kommen.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum PdfPages {
    /// Nur das gewählte Blatt.
    #[default]
    Selected,
    /// Alle Blätter des Dokuments in Dokumentreihenfolge.
    All,
}

/// Dokumenteigenschaften der PDF (P10, P11): Titel = Name des gewählten Blatts bzw. des Dokuments
/// (alle Blätter); Ersteller nur, wenn in den Projektangaben ein eigener Text steht (nicht der
/// Anmeldename); Datum des ersten Blatts der PDF = eigenes Ausgabedatum, wenn gültig, sonst das
/// Erstellungsdatum des Blatts, wenn gültig, sonst `today`.
pub fn pdf_meta(
    state: &AppState,
    pages: PdfPages,
    drawing: Option<DrawingId>,
    today: &str,
) -> PdfMeta {
    let doc = &state.document;
    let first = match pages {
        PdfPages::Selected => drawing
            .and_then(|id| doc.drawing(id))
            .or_else(|| default_drawing(state).and_then(|id| doc.drawing(id))),
        PdfPages::All => doc.drawings().first(),
    };
    let title = match pages {
        PdfPages::Selected => first.map(|d| d.name.clone()).unwrap_or_default(),
        PdfPages::All => state.document_name.clone(),
    };
    let author = doc.project_info().and_then(|p| match &p.creator {
        FieldValue::Text(t) if !t.trim().is_empty() => Some(t.trim().to_string()),
        _ => None,
    });
    let valid = |s: &str| pdf_date(s).is_some();
    let info = first.and_then(|d| d.info.as_ref());
    let date = info
        .and_then(|i| match &i.issued {
            FieldValue::Text(t) if valid(t) => Some(t.clone()),
            _ => None,
        })
        .or_else(|| {
            info.filter(|i| valid(&i.created))
                .map(|i| i.created.clone())
        })
        .unwrap_or_else(|| today.to_string());
    PdfMeta {
        title,
        author,
        date: Some(date),
    }
}

/// Wahl in der Karte „Zeichnung“ bei PDF: nur das gewählte Blatt.
pub const PDF_SELECTED: &str = "Gewähltes Blatt";
/// Wahl in der Karte „Zeichnung“ bei PDF: alle Blätter des Dokuments.
pub const PDF_ALL: &str = "Alle Blätter";
/// Hinweis an der gesperrten Zeichnungsauswahl bei „Alle Blätter“.
pub const PDF_ALL_HINT: &str =
    "Bei „Alle Blätter“ kommen alle Zeichnungen der Reihe nach in die PDF.";

/// Text im Fortschrittsbalken, solange bei „Alle Blätter“ das Blatt `n` von `total` berechnet
/// wird.
pub fn sheet_progress_text(n: usize, total: usize) -> String {
    format!("Blatt {n} von {total} wird berechnet …")
}

/// Meldung `msg` zu dem Blatt `sheet` (bei „Alle Blätter“).
pub fn in_sheet(sheet: &str, msg: &str) -> String {
    format!("Im Blatt „{sheet}“: {msg}")
}

/// Meldung, wenn das Dokument mehr Blätter hat, als eine PDF enthalten kann.
pub fn too_many_pages_text(n: usize) -> String {
    format!(
        "Eine PDF kann höchstens {MAX_PAGES} Blätter enthalten, das Dokument hat {n}. Bitte einzelne Blätter ausgeben."
    )
}

/// „1 Blatt“ bzw. „{n} Blätter“.
pub fn pages_label(n: usize) -> String {
    if n == 1 {
        "1 Blatt".to_string()
    } else {
        format!("{n} Blätter")
    }
}

/// Ein Blatt für die Berechnung aller Blätter: Kopie der Zeichnung, ihr Blattrahmen (mit der
/// Stellung im Dokument) und ihre Stückliste.
#[derive(Clone, Debug)]
pub struct SheetJob {
    pub drawing: Drawing,
    pub frame: Option<FrameInput>,
    pub parts: PartsInput,
}

/// So fein ist der Fortschritt eines Blatts bei „Alle Blätter“ unterteilt.
const SHEET_STEPS: u64 = 1000;

/// Prüft ein fertiges Blatt auf Zeichen, die sich in einer PDF nicht ausgeben lassen (P13):
/// erst die Felder des Rahmens, dann die Texte der Ansichten.
fn pdf_texts_ok(r: &DrawingPrepared) -> Result<(), String> {
    if let Some(texts) = &r.texts {
        pdf_sheet_texts_ok(texts)?;
    }
    pdf_model_texts_ok(&r.model)
}

/// Berechnet alle Blätter für eine PDF nacheinander in der gegebenen Reihenfolge (P12, F2; läuft
/// im Arbeitsthread). Vor jedem Blatt steht dessen Nummer (ab 1) in `counter`; der Fortschritt
/// `progress` wächst über alle Blätter (das laufende Blatt anteilig). Jedes Blatt wird wie beim
/// gewählten Blatt berechnet und auf Zeichen geprüft. Der erste Fehler in Blattreihenfolge
/// beendet alles mit „Im Blatt „Name“: …“; nie wird ein Blatt ausgelassen. Abbrechen ergibt
/// „Abgebrochen“.
///
/// Grenze: Jedes Blatt wird von Grund auf neu gerechnet (kein Zwischenspeicher zwischen den
/// Blättern und zum Zeichnungsbereich); dieselbe Ansichtsart derselben Körper auf mehreren
/// Blättern kostet also jedes Mal die volle Rechenzeit, und jede Änderung am Dokument rechnet
/// alle Blätter neu.
pub fn prepare_all_sheets(
    sheets: &[SheetJob],
    solids: &[(String, Solid)],
    compute: ComputeFn,
    resolve: ResolveFn,
    counter: &AtomicUsize,
    progress: &Progress,
) -> Result<Vec<DrawingPrepared>, String> {
    prepare_all_counted(sheets, solids, compute, resolve, counter, progress).map_err(|f| f.message)
}

/// Wie [`prepare_all_sheets`]; scheitert erst das letzte Blatt an einer späteren Prüfung, nachdem
/// seine Linien berechnet sind, trägt der Fehler die Summe der Linien aller Blätter.
fn prepare_all_counted(
    sheets: &[SheetJob],
    solids: &[(String, Solid)],
    compute: ComputeFn,
    resolve: ResolveFn,
    counter: &AtomicUsize,
    progress: &Progress,
) -> Result<Vec<DrawingPrepared>, DrawFailed> {
    let cancelled = ExchangeError::Cancelled.to_string();
    progress.set_total(sheets.len().max(1) as u64 * SHEET_STEPS);
    let mut shown = 0u64;
    let mut out = Vec::with_capacity(sheets.len());
    for (i, sheet) in sheets.iter().enumerate() {
        progress
            .check()
            .map_err(|_| DrawFailed::plain(cancelled.clone()))?;
        counter.store(i + 1, AtomicOrdering::SeqCst);
        // Eigener Fortschritt je Blatt; Abbrechen wird weitergereicht, der Anteil fließt in den
        // Fortschritt aller Blätter ein.
        let inner = Progress::new();
        let result = std::thread::scope(|scope| {
            let handle = scope.spawn(|| -> Result<DrawingPrepared, DrawFailed> {
                let mut lines = None;
                let fail = |m: String, lines| DrawFailed { message: m, lines };
                let r = prepare_counted(
                    &sheet.drawing,
                    solids,
                    compute,
                    resolve,
                    sheet.frame.as_ref(),
                    &sheet.parts,
                    &inner,
                    &mut lines,
                )
                .map_err(|m| fail(m, lines))?;
                pdf_texts_ok(&r).map_err(|m| fail(m, lines))?;
                Ok(r)
            });
            while !handle.is_finished() {
                if progress.is_cancelled() {
                    inner.cancel();
                }
                let at = ((i as f64 + f64::from(inner.fraction())) * SHEET_STEPS as f64) as u64;
                if at > shown {
                    progress.advance(at - shown);
                    shown = at;
                }
                std::thread::sleep(WATCH);
            }
            handle.join().ok()
        });
        let name = shorten_for_display(&sheet.drawing.name);
        match result {
            Some(Ok(r)) => out.push(r),
            _ if progress.is_cancelled() => return Err(DrawFailed::plain(cancelled)),
            Some(Err(f)) if f.message == cancelled => return Err(DrawFailed::plain(cancelled)),
            Some(Err(f)) => {
                // Linien gibt es nur, wenn alle Blätter berechnet sind: hier also nur, wenn das
                // gescheiterte Blatt das letzte ist.
                let last = i + 1 == sheets.len();
                let lines = f.lines.filter(|_| last).map(|(v, h)| {
                    out.iter()
                        .fold((v, h), |(v, h), p| (v + p.visible, h + p.hidden))
                });
                return Err(DrawFailed {
                    message: in_sheet(&name, &f.message),
                    lines,
                });
            }
            None => return Err(DrawFailed::plain(in_sheet(&name, LOST))),
        }
        let at = (i as u64 + 1) * SHEET_STEPS;
        if at > shown {
            progress.advance(at - shown);
            shown = at;
        }
    }
    Ok(out)
}

/// Blatt für die Anzeige, z. B. „A3 quer“.
pub fn sheet_label(sheet: Sheet) -> String {
    format!(
        "{} {}",
        paper_label(sheet.paper),
        if sheet.landscape { "quer" } else { "hoch" }
    )
}

/// Linien einer Ansicht und bei einer Schnittansicht die Daten ihrer Schnittflächen.
type KeyedLines = (ViewLines, Option<SectionData>);

/// Berechnet eine Ansicht (Art oder Schnittebene) in einem eigenen Thread mit eigenem Fortschritt;
/// ein Abbruch von `outer` wird weitergereicht. `None`, wenn die Rechnung abstürzt.
fn compute_watched(
    solids: &[(String, Solid)],
    key: ViewKey,
    compute: ComputeFn,
    outer: &Progress,
) -> Option<Result<KeyedLines, DrawError>> {
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
            match key {
                ViewKey::Plain(kind) => compute(&refs, kind, &inner).map(|l| (l, None)),
                ViewKey::Section(plane) => {
                    compute_section(&refs, &plane, compute, &inner).map(|s| (s.lines, Some(s.data)))
                }
            }
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
    prepare_drawing_framed(drawing, solids, compute, resolve, None, progress)
}

/// Angaben für den Blattrahmen (Rahmen, Schriftfeld, Tabellen) einer ausgegebenen Zeichnung.
#[derive(Clone, Debug)]
pub struct FrameInput {
    /// Projektangaben des Dokuments, falls schon vorhanden.
    pub project: Option<ProjectInfo>,
    /// Stellung des Blatts im Dokument und Name des Dokuments (für die automatischen Werte).
    pub ctx: SheetContext,
    /// Für DXF: die Texte mit dem Breitenzuschlag für CAD-Schriften einpassen
    /// ([`sheet_frame_dxf`]).
    pub dxf: bool,
}

/// Wie [`prepare_drawing_with`]; mit `frame` entsteht zu dem Blatt auch der Blattrahmen: die
/// Texte aus den Blattangaben und dem Maßstab der Hauptansicht, die Geometrie nach deren
/// Zeilenzahlen. Ohne `frame` bleibt `model.frame` leer.
///
/// Diese Funktion (und [`prepare_drawing`], [`prepare_drawing_with`]) kennt die Nummern der
/// Positionskreise nicht: Eine Zeichnung mit Kreisen würde als „Kreis verloren“ abgewiesen, in
/// Debug-Läufen bricht eine Prüfung vorher ab. Dafür gibt es [`prepare_drawing_parts`] mit
/// [`parts_input`].
pub fn prepare_drawing_framed(
    drawing: &Drawing,
    solids: &[(String, Solid)],
    compute: ComputeFn,
    resolve: ResolveFn,
    frame: Option<&FrameInput>,
    progress: &Progress,
) -> Result<DrawingPrepared, String> {
    debug_assert!(
        drawing.balloons.is_empty(),
        "Zeichnungen mit Positionsnummern gehören zu prepare_drawing_parts (mit den Nummern)"
    );
    prepare_drawing_parts(
        drawing,
        solids,
        compute,
        resolve,
        frame,
        &PartsInput::default(),
        progress,
    )
}

/// Was die Stückliste zur Ausgabe einer Zeichnung beiträgt (Festlegungen G12, G13).
#[derive(Clone, Debug, Default)]
pub struct PartsInput {
    /// Die Zeilen der Stückliste, die auf dieses Blatt gehören (Zeichnungsblatt: die ersten
    /// zehn, wenn der Schalter des Blatts an ist; Stücklistenblatt: die Zeilen seiner Seite).
    pub cells: Vec<PartsCells>,
    /// Nummer je Kreis der Zeichnung; `None` oder fehlend heißt verloren.
    pub balloons: BTreeMap<BalloonId, Option<u32>>,
    /// Die Ausgabe ist DXF. Gilt nur für ein Stücklistenblatt ohne Rahmen (Breitenzuschlag der
    /// Tabellentexte); mit Rahmen entscheidet [`FrameInput::dxf`].
    pub dxf: bool,
}

/// Die Stückliste und die Nummern der Kreise für die Ausgabe von `drawing` aus dem Stand von
/// `state` (`dxf` bleibt `false`).
pub fn parts_input(state: &AppState, drawing: &Drawing) -> PartsInput {
    let cells = if parts_space_for(drawing, 0, true) == PartsSpace::None {
        Vec::new()
    } else {
        parts_on(drawing, &state.document.parts_cells())
    };
    let balloons = if drawing.balloons.is_empty() {
        BTreeMap::new()
    } else {
        state.document.balloon_numbers(drawing.id)
    };
    PartsInput {
        cells,
        balloons,
        dxf: false,
    }
}

/// Meldung, wenn ein Nummernkreis seinen Körper verloren hat (Festlegung G13): es wird nichts
/// geschrieben.
pub fn lost_balloon_text(view: &str) -> String {
    format!(
        "Die Positionsnummer in der Ansicht „{view}“ gehört zu einem Teil, das es nicht mehr gibt. Bitte die Positionsnummer löschen und neu setzen."
    )
}

/// Der erste Nummernkreis (in der Reihenfolge der Zeichnung) ohne Nummer als Meldung.
fn first_lost_balloon(
    drawing: &Drawing,
    numbers: &BTreeMap<BalloonId, Option<u32>>,
    names: &BTreeMap<ViewId, String>,
) -> Option<String> {
    drawing
        .balloons
        .iter()
        .find(|b| numbers.get(&b.id).copied().flatten().is_none())
        .map(|b| lost_balloon_text(&names.get(&b.view).cloned().unwrap_or_default()))
}

/// Wie [`prepare_drawing_framed`] mit der Stückliste `parts`. Die Nummernkreise stehen immer im
/// Blatt, auch ohne Rahmen. Ein Zeichnungsblatt bekommt die Tabelle nur mit Rahmen (Schalter
/// „Blattrahmen mit ausgeben“); ein Stücklistenblatt gibt seine Tabelle immer aus, ohne Rahmen
/// allein, ohne Rahmen und Schriftfeld. Hat ein Kreis seinen Körper verloren, gibt es kein
/// Blatt, sondern [`lost_balloon_text`]; ohne Nummern in `parts` gilt jeder Kreis als verloren.
pub fn prepare_drawing_parts(
    drawing: &Drawing,
    solids: &[(String, Solid)],
    compute: ComputeFn,
    resolve: ResolveFn,
    frame: Option<&FrameInput>,
    parts: &PartsInput,
    progress: &Progress,
) -> Result<DrawingPrepared, String> {
    let mut lines = None;
    prepare_counted(
        drawing, solids, compute, resolve, frame, parts, progress, &mut lines,
    )
}

/// Wie [`prepare_drawing_parts`]; sobald alle Ansichten mit ihren Linien auf dem Blatt stehen,
/// steht deren Zahl in `lines_out`, auch wenn eine spätere Prüfung die Ausgabe noch sperrt.
#[allow(clippy::too_many_arguments)]
fn prepare_counted(
    drawing: &Drawing,
    solids: &[(String, Solid)],
    compute: ComputeFn,
    resolve: ResolveFn,
    frame: Option<&FrameInput>,
    parts: &PartsInput,
    progress: &Progress,
    lines_out: &mut Option<LineCounts>,
) -> Result<DrawingPrepared, String> {
    let cancelled = || ExchangeError::Cancelled.to_string();
    // Erst die Normarten wie bisher, dann je Schnittansicht ihre Ebene.
    let mut keys: Vec<ViewKey> = Vec::new();
    for v in drawing.views.iter().filter(|v| v.section.is_none()) {
        let key = ViewKey::of(v);
        if !keys.contains(&key) {
            keys.push(key);
        }
    }
    for v in drawing.views.iter().filter(|v| v.section.is_some()) {
        let key = ViewKey::of(v);
        if !keys.contains(&key) {
            keys.push(key);
        }
    }
    let with_dims = !drawing.dimensions.is_empty();
    let kinds_empty = keys.is_empty();
    progress.set_total((keys.len() as u64 + u64::from(with_dims)).max(1));
    let names = view_names(drawing);
    if let Some(m) = first_lost_balloon(drawing, &parts.balloons, &names) {
        return Err(m);
    }
    let mut lines: Vec<(ViewKey, KeyedLines)> = Vec::with_capacity(keys.len());
    for key in keys {
        progress.check().map_err(|_| cancelled())?;
        match compute_watched(solids, key, compute, progress) {
            None => return Err(LOST.to_string()),
            Some(Ok(l)) => lines.push((key, l)),
            Some(Err(DrawError::Cancelled)) => return Err(cancelled()),
            Some(Err(e)) => {
                let name = drawing
                    .views
                    .iter()
                    .find(|v| ViewKey::of(v) == key)
                    .and_then(|v| names.get(&v.id))
                    .cloned()
                    .unwrap_or_else(|| match key {
                        ViewKey::Plain(kind) => kind.label().to_string(),
                        ViewKey::Section(plane) => plane.kind.label().to_string(),
                    });
                return Err(format!(
                    "Die Ansicht „{name}“ lässt sich nicht berechnen: {e}"
                ));
            }
        }
        progress.advance(1);
    }
    let found = |v: &planora_drawing::ViewSpec| {
        let key = ViewKey::of(v);
        lines.iter().find(|(k, _)| *k == key).map(|(_, l)| l)
    };
    let map: BTreeMap<ViewId, &ViewLines> = drawing
        .views
        .iter()
        .filter_map(|v| found(v).map(|(l, _)| (v.id, l)))
        .collect();
    let section_data: BTreeMap<ViewId, &SectionData> = drawing
        .views
        .iter()
        .filter_map(|v| found(v).and_then(|(_, d)| d.as_ref()).map(|d| (v.id, d)))
        .collect();
    let dims = if with_dims {
        progress.check().map_err(|_| cancelled())?;
        let refs: Vec<&Solid> = solids.iter().map(|(_, s)| s).collect();
        // Jede Schnittansicht mit den Körpern ihres eigenen Schnitts (hier berechnet, also immer
        // vorhanden; ein gescheiterter Schnitt hat die Ausgabe schon gesperrt).
        let cut: ViewSolids = section_data
            .iter()
            .map(|(v, data)| (*v, data.solids.iter().collect()))
            .collect();
        // Scheitern die Maße, sind die Linien doch berechnet: ihre Zahl aus dem Blatt ohne Maße
        // (Maße ändern keine Linie).
        let mut no_dims = || {
            let plain = build_sheet_with(drawing, &map, &ResolvedDims::default());
            *lines_out = sheet_line_counts(drawing, &plain);
        };
        let dims = match resolve(drawing, &refs, &cut, progress) {
            Ok(d) => d,
            Err(DrawError::Cancelled) => return Err(cancelled()),
            Err(e) => {
                no_dims();
                return Err(e.to_string());
            }
        };
        if let Some(m) = first_lost(drawing, &dims, &names) {
            no_dims();
            return Err(m);
        }
        progress.advance(1);
        dims
    } else {
        ResolvedDims::default()
    };
    let mut model = build_sheet_with(drawing, &map, &dims);
    add_balloon_marks(&mut model, &balloon_marks(drawing, &map, &parts.balloons));
    // Die Linien stehen fest (Schnittmarken und Rahmen ändern sie nicht).
    *lines_out = sheet_line_counts(drawing, &model);
    // Schraffur, Schnittlinie und Titel; ein Fehler (zu feine Schraffur) sperrt die Ausgabe.
    let (marks, section_errors) = section_marks(drawing, &map, &section_data);
    if let Some(m) = section_errors.into_iter().next() {
        return Err(m);
    }
    add_section_marks(&mut model, &marks);
    // `build_sheet` lässt Ansichten weg, die sich nicht anordnen lassen; das Dokument prüft Lage,
    // Maßstab und Eltern, also kommt das nur bei einer beschädigten Zeichnung vor. Nie still
    // weniger Ansichten schreiben.
    if model.views.len() < drawing.views.len() {
        return Err(VIEWS_NOT_PLACEABLE.to_string());
    }
    if kinds_empty && !with_dims {
        progress.advance(1);
    }
    let space = parts_space_for(drawing, parts.cells.len(), false);
    let texts = match frame {
        Some(f) => {
            let mut texts = resolve_texts(drawing, f.project.as_ref(), &f.ctx, main_scale(drawing));
            texts.parts = parts.cells.clone();
            let layout = sheet_layout_with(
                drawing.sheet,
                texts.visible_revisions().len(),
                texts.notes.len(),
                space,
            );
            model.frame = Some(if f.dxf {
                sheet_frame_dxf(&layout, &texts, drawing.projection)
            } else {
                sheet_frame(&layout, &texts, drawing.projection)
            });
            Some(texts)
        }
        // Ein Stücklistenblatt ohne Rahmen gibt nur seine Tabelle aus.
        None if matches!(drawing.kind, DrawingKind::PartsList { .. }) => {
            let layout = sheet_layout_with(drawing.sheet, 0, 0, space);
            let widen = if parts.dxf {
                DXF_TEXT_WIDTH_FACTOR
            } else {
                1.0
            };
            model.frame = Some(SheetFrame {
                marks: parts_table_marks(&layout, &parts.cells, widen),
                scale: layout.scale,
            });
            Some(TitleTexts {
                fields: BTreeMap::new(),
                revisions: Vec::new(),
                notes: Vec::new(),
                parts: parts.cells.clone(),
            })
        }
        None => None,
    };
    let outside = count_outside(
        &dimension_marks(drawing, &map, &dims),
        drawing.sheet.size_mm(),
    );
    Ok(DrawingPrepared {
        views: model.views.len(),
        sheet: sheet_label(drawing.sheet),
        visible: line_count(&model, Visibility::Visible),
        hidden: line_count(&model, Visibility::Hidden),
        skipped_meshes: Vec::new(),
        dimensions: dims.by_view.values().map(Vec::len).sum(),
        outside,
        centerlines: centerline_use(drawing),
        texts,
        model: Arc::new(model),
    })
}

/// Zahl der Linien des Blatts `model` mit der Sichtbarkeit `vis`.
fn line_count(model: &SheetModel, vis: Visibility) -> usize {
    model
        .views
        .iter()
        .flat_map(|v| &v.lines)
        .filter(|l| l.vis == vis)
        .count()
}

/// Sichtbare und verdeckte Linien des Blatts `model`; nur wenn jede Ansicht der Zeichnung
/// darauf steht (sonst wäre die Zahl unvollständig).
fn sheet_line_counts(drawing: &Drawing, model: &SheetModel) -> Option<LineCounts> {
    (model.views.len() >= drawing.views.len()).then(|| {
        (
            line_count(model, Visibility::Visible),
            line_count(model, Visibility::Hidden),
        )
    })
}

/// Meldung, wenn sich nicht alle Ansichten auf dem Blatt anordnen lassen.
pub const VIEWS_NOT_PLACEABLE: &str =
    "Die Zeichnung enthält Ansichten, die sich nicht auf dem Blatt anordnen lassen.";

/// Zeile „Linien“, solange die Ansichten berechnet werden.
pub const LINES_COMPUTING: &str = "Linien: werden berechnet …";
/// Zeile „Linien“, wenn es keine berechneten Linien gibt (etwa eine Ansicht scheiterte).
pub const LINES_NOT_COMPUTED: &str = "Linien: nicht berechnet";

/// Zeile „Linien“ mit der Zahl der sichtbaren und verdeckten Linien.
pub fn lines_counts_text(visible: usize, hidden: usize) -> String {
    format!(
        "Linien: {} sichtbar, {} verdeckt",
        group_digits(visible as u64),
        group_digits(hidden as u64)
    )
}

/// Text im Fortschrittsbalken, solange das Ergebnis des Formats `format` berechnet wird;
/// `is_list` für ein Stücklistenblatt.
pub fn computing_text(format: ExportFormat, is_list: bool) -> &'static str {
    if format.is_drawing() && is_list {
        "Die Stückliste wird berechnet …"
    } else if format.is_drawing() {
        "Die Ansichten werden berechnet …"
    } else if format.is_step() {
        "Die Körper werden geprüft …"
    } else {
        "Das Netz wird berechnet …"
    }
}

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
/// Was ein Zeichnungsergebnis enthält: eine Zeichnung oder (nur PDF) alle Blätter.
#[derive(Clone, Copy, Debug, PartialEq)]
enum DrawTarget {
    One(DrawingId),
    All,
}

/// Stand, zu dem ein Zeichnungsergebnis gehört: Zeichnung bzw. alle Blätter, Änderungszahl des
/// Dokuments (alle Änderungen, auch an jeder Zeichnung, an Projekt- und Blattangaben), Format,
/// Schalter „Blattrahmen“ und Prüfsumme des Dokumentnamens (er steckt in der Zeichnungsnummer
/// und im Titel der PDF aller Blätter).
#[derive(Clone, Copy, Debug, PartialEq)]
struct DrawKey {
    target: DrawTarget,
    edits: u64,
    format: DrawingFormat,
    frame: bool,
    doc_name: u64,
}

/// Ergebnis: die Blätter in Dokumentreihenfolge (beim gewählten Blatt genau eines).
type DrawJob = Job<Result<Vec<DrawingPrepared>, DrawFailed>>;
type DrawResult = Result<Arc<Vec<DrawingPrepared>>, DrawFailed>;

/// Linien einer Zeichnung: sichtbar und verdeckt.
type LineCounts = (usize, usize);

/// Gescheiterte Berechnung einer Zeichnung: die Meldung und, wenn die Linien aller Blätter
/// schon berechnet waren (gesperrt hat erst eine spätere Prüfung, etwa ein Zeichen, das eine
/// PDF nicht ausgeben kann, oder eine zu feine Schraffur), deren Zahl für die Angaben.
#[derive(Debug)]
struct DrawFailed {
    message: String,
    lines: Option<LineCounts>,
}

impl DrawFailed {
    /// Fehler ohne berechnete Linien.
    fn plain(message: impl Into<String>) -> Self {
        DrawFailed {
            message: message.into(),
            lines: None,
        }
    }
}
/// Laufende Berechnung: Stand, Auftrag und bei „Alle Blätter“ Zähler des Blatts und Zahl der
/// Blätter.
type DrawRun = (DrawKey, DrawJob, Option<(Arc<AtomicUsize>, usize)>);
type DrawWriteJob = Job<Result<u64, String>>;
/// Das Schreiben einer Zeichnung (austauschbar nur für Tests).
pub type WriteFn = fn(&Path, &SheetModel, DrawingFormat, &Progress) -> Result<u64, DrawError>;
/// Das Schreiben einer PDF-Datei aus Blättern mit Dokumenteigenschaften (austauschbar nur für
/// Tests).
pub type PdfWriteFn = fn(&Path, &[&SheetModel], &PdfMeta, &Progress) -> Result<u64, DrawError>;
/// Das Schreiben einer STEP-Datei (austauschbar nur für Tests).
pub type StepWriteFn = fn(&Path, &StepPrepared, &Progress) -> Result<u64, StepError>;

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
    /// Zeichnungsformate: Blattrahmen (Rahmen, Schriftfeld, Tabellen) mit ausgeben; Vorgabe an.
    pub with_frame: bool,
    /// PDF: das gewählte Blatt oder alle Blätter; Vorgabe das gewählte.
    pub pdf_pages: PdfPages,
    prepared: Option<(Key, PrepResult)>,
    job: Option<(Key, PrepJob)>,
    step_prepared: Option<(Key, StepResult)>,
    step_job: Option<(Key, StepJob)>,
    /// Ergebnisse der Zeichnungsformate, höchstens eines je Format (ein Wechsel des Formats
    /// lässt das Ergebnis des anderen stehen).
    drawing_prepared: Vec<(DrawKey, DrawResult)>,
    drawing_job: Option<DrawRun>,
    chooser: Option<FileChooser>,
    /// Schlüssel und Format zur Zeit des Öffnens der Dateiauswahl; ändern sie sich, schließt sie.
    chooser_key: Option<ChooserKey>,
    writing: Option<(PathBuf, WriteJob)>,
    step_writing: Option<(PathBuf, StepWriteJob)>,
    drawing_writing: Option<(PathBuf, DrawWriteJob)>,
    compute: ComputeFn,
    resolve: ResolveFn,
    writer: WriteFn,
    pdf_writer: PdfWriteFn,
    step_writer: StepWriteFn,
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
            with_frame: true,
            pdf_pages: PdfPages::default(),
            prepared: None,
            job: None,
            step_prepared: None,
            step_job: None,
            drawing_prepared: Vec::new(),
            drawing_job: None,
            chooser: None,
            chooser_key: None,
            writing: None,
            step_writing: None,
            drawing_writing: None,
            compute: compute_view,
            resolve: resolve_dimensions_in_worker,
            writer: export_drawing_file,
            pdf_writer: export_pdf_file,
            step_writer: write_step_bodies,
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

    /// Ersetzt das Schreiben einer PDF-Datei (nur für Tests: langes Schreiben).
    #[doc(hidden)]
    pub fn set_pdf_writer(&mut self, f: PdfWriteFn) {
        self.pdf_writer = f;
    }

    /// Ersetzt das Schreiben einer STEP-Datei (nur für Tests: Schreiben, das bis zum Abbrechen
    /// läuft).
    #[doc(hidden)]
    pub fn set_step_writer(&mut self, f: StepWriteFn) {
        self.step_writer = f;
    }

    /// Die gewählte Zeichnung; gibt es sie nicht (mehr), die Vorgabe.
    fn current_drawing<'a>(&self, state: &'a AppState) -> Option<&'a Drawing> {
        self.drawing
            .and_then(|id| state.document.drawing(id))
            .or_else(|| default_drawing(state).and_then(|id| state.document.drawing(id)))
    }

    /// Gilt die Wahl „Alle Blätter“ (nur bei PDF)?
    fn all_sheets(&self) -> bool {
        self.format == ExportFormat::Drawing(DrawingFormat::Pdf) && self.pdf_pages == PdfPages::All
    }

    /// Schlüssel des Zeichnungsergebnisses; `None` bei anderen Formaten oder ohne Zeichnung. Bei
    /// „Alle Blätter“ zählt die gewählte Zeichnung nicht (eine andere Wahl rechnet nicht neu).
    fn drawing_key(&self, state: &AppState) -> Option<DrawKey> {
        let format = self.format.drawing_format()?;
        let d = self.current_drawing(state)?;
        let mut hasher = std::hash::DefaultHasher::new();
        std::hash::Hash::hash(&state.document_name, &mut hasher);
        Some(DrawKey {
            target: if self.all_sheets() {
                DrawTarget::All
            } else {
                DrawTarget::One(d.id)
            },
            edits: state.document.edit_count(),
            format,
            frame: self.with_frame,
            doc_name: std::hash::Hasher::finish(&hasher),
        })
    }

    /// Warum sich keine Zeichnung berechnen lässt: keine Zeichnung, keine exakten Körper (wie
    /// im Zeichnungsbereich) oder keine Ansicht. Bei „Alle Blätter“ auch zu viele Blätter
    /// ([`too_many_pages_text`]) und die erste Ansichtenzeichnung ohne Ansicht.
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
        let no_view = |d: &Drawing| {
            (d.views.is_empty() && matches!(d.kind, DrawingKind::Views)).then(|| {
                format!(
                    "Die Zeichnung „{}“ hat noch keine Ansicht.",
                    shorten_for_display(&d.name)
                )
            })
        };
        if self.all_sheets() {
            let all = state.document.drawings();
            if all.len() > MAX_PAGES {
                return Some(too_many_pages_text(all.len()));
            }
            return all.iter().find_map(no_view);
        }
        no_view(d)
    }

    fn chooser_key_now(&self, state: &AppState) -> ChooserKey {
        if self.format.is_drawing() {
            ChooserKey::Drawing(self.drawing_key(state))
        } else {
            ChooserKey::Bodies(self.key(state))
        }
    }

    /// Auftrag für die gewählte Zeichnung (SVG, DXF, PDF mit gewähltem Blatt).
    fn spawn_one(&self, state: &AppState, key: DrawKey) -> Option<DrawRun> {
        let drawing = self.current_drawing(state)?.clone();
        let (solids, meshes) = drawing_bodies(state);
        let (compute, resolve, format) = (self.compute, self.resolve, key.format);
        let frame = self.with_frame.then(|| FrameInput {
            project: state.document.project_info().cloned(),
            ctx: context_or_single(state, drawing.id),
            dxf: format == DrawingFormat::Dxf,
        });
        let parts = PartsInput {
            dxf: format == DrawingFormat::Dxf,
            ..parts_input(state, &drawing)
        };
        let is_list = matches!(drawing.kind, DrawingKind::PartsList { .. });
        let job = Job::spawn(move |p| {
            let mut lines = None;
            let mut r = prepare_counted(
                &drawing,
                &solids,
                compute,
                resolve,
                frame.as_ref(),
                &parts,
                p,
                &mut lines,
            )
            .map_err(|message| DrawFailed { message, lines })?;
            // Die Prüfungen der Texte sperren die Ausgabe, die Linien sind aber berechnet.
            let checked =
                |c: Result<(), String>| c.map_err(|message| DrawFailed { message, lines });
            if format == DrawingFormat::Dxf {
                if let Some(texts) = &r.texts {
                    checked(dxf_sheet_texts_ok(texts))?;
                }
                checked(dxf_texts_fit(&r.model))?;
            }
            if format == DrawingFormat::Pdf {
                checked(pdf_texts_ok(&r))?;
            }
            // Ein Stücklistenblatt hat keine Ansichten, die Netz-Körper auslassen.
            if !is_list {
                r.skipped_meshes = meshes;
            }
            Ok(vec![r])
        });
        Some((key, job, None))
    }

    /// Auftrag für alle Blätter des Dokuments in Dokumentreihenfolge (P12): je Blatt eigener
    /// Blattrahmen mit seiner Stellung im Dokument und eigene Stückliste, beides ohne
    /// DXF-Zuschlag. Der Hinweis auf Netz-Körper steht einmal (am ersten Ansichtenblatt).
    fn spawn_all(&self, state: &AppState, key: DrawKey) -> DrawRun {
        let (solids, meshes) = drawing_bodies(state);
        let project = state.document.project_info().cloned();
        let sheets: Vec<SheetJob> = state
            .document
            .drawings()
            .iter()
            .map(|d| SheetJob {
                drawing: d.clone(),
                frame: self.with_frame.then(|| FrameInput {
                    project: project.clone(),
                    ctx: context_or_single(state, d.id),
                    dxf: false,
                }),
                parts: parts_input(state, d),
            })
            .collect();
        let total = sheets.len();
        let counter = Arc::new(AtomicUsize::new(0));
        let shared = Arc::clone(&counter);
        let (compute, resolve) = (self.compute, self.resolve);
        let job = Job::spawn(move |p| {
            let mut r = prepare_all_counted(&sheets, &solids, compute, resolve, &shared, p)?;
            if let Some(first) = sheets
                .iter()
                .zip(r.iter_mut())
                .find(|(s, _)| !matches!(s.drawing.kind, DrawingKind::PartsList { .. }))
            {
                first.1.skipped_meshes = meshes;
            }
            Ok(r)
        });
        (key, job, Some((counter, total)))
    }

    /// Startet die Berechnung der Zeichnung (bzw. aller Blätter), wenn sich Zeichnung, Dokument,
    /// Format oder Wahl geändert haben, und holt ein fertiges Ergebnis ab.
    fn tick_drawing(&mut self, state: &AppState, block: bool) {
        if self
            .drawing
            .is_none_or(|id| state.document.drawing(id).is_none())
        {
            self.drawing = default_drawing(state);
        }
        if let (Some(key), None) = (self.drawing_key(state), self.drawing_block(state)) {
            let fresh = self
                .prepared_for(key.format)
                .is_some_and(|(k, _)| *k == key);
            let running = self.drawing_job.as_ref().is_some_and(|(k, ..)| *k == key);
            if !fresh && !running {
                // Ein alter Auftrag wird dabei fallengelassen und bricht ab.
                let run = match key.target {
                    DrawTarget::All => Some(self.spawn_all(state, key)),
                    DrawTarget::One(_) => self.spawn_one(state, key),
                };
                if run.is_some() {
                    self.drawing_job = run;
                }
            }
        }
        let finished = match &self.drawing_job {
            Some((k, job, _)) => match job.fetch(block) {
                Poll::Pending => None,
                Poll::Ready(r) => Some((*k, r.map(Arc::new))),
                Poll::Lost => Some((*k, Err(DrawFailed::plain(LOST)))),
            },
            None => None,
        };
        if let Some(done) = finished {
            self.store_prepared(done);
            self.drawing_job = None;
        }
    }

    /// Bei „Alle Blätter“, solange gerechnet wird: das laufende Blatt (ab 1) und die Zahl der
    /// Blätter.
    pub fn computing_sheet(&self) -> Option<(usize, usize)> {
        let (_, _, Some((counter, total))) = self.drawing_job.as_ref()? else {
            return None;
        };
        let n = counter
            .load(AtomicOrdering::SeqCst)
            .clamp(1, (*total).max(1));
        Some((n, *total))
    }

    fn drawing_status(&self, state: &AppState) -> ExportStatus {
        if let Some(m) = self.drawing_block(state) {
            return ExportStatus::Unavailable(m);
        }
        let key = self.drawing_key(state);
        if let Some((k, job, _)) = &self.drawing_job
            && Some(*k) == key
        {
            return ExportStatus::Computing(job.fraction());
        }
        match key.and_then(|k| self.prepared_for(k.format)) {
            Some((k, Ok(list))) if Some(*k) == key => {
                let all = k.target == DrawTarget::All;
                let sum = |f: fn(&DrawingPrepared) -> usize| list.iter().map(f).sum::<usize>();
                ExportStatus::DrawingReady {
                    views: sum(|p| p.views),
                    sheet: if all {
                        pages_label(list.len())
                    } else {
                        list.first().map(|p| p.sheet.clone()).unwrap_or_default()
                    },
                    visible: sum(|p| p.visible),
                    hidden: sum(|p| p.hidden),
                    skipped_meshes: list
                        .iter()
                        .flat_map(|p| p.skipped_meshes.iter().cloned())
                        .collect(),
                    dimensions: sum(|p| p.dimensions),
                    outside: sum(|p| p.outside),
                    centerlines: if all {
                        centerline_use_all(state.document.drawings())
                    } else {
                        list.first().map_or(CenterlineUse::No, |p| p.centerlines)
                    },
                }
            }
            Some((k, Err(f))) if Some(*k) == key => ExportStatus::Failed(f.message.clone()),
            _ => ExportStatus::Computing(0.0),
        }
    }

    /// Zeile „Linien“ in den Angaben zur Zeichnung (für alle Zeichnungsformate gleich): die Zahl
    /// der Linien, sobald sie berechnet ist, auch wenn eine spätere Prüfung die Ausgabe sperrt;
    /// sonst [`LINES_COMPUTING`] während der Berechnung und [`LINES_NOT_COMPUTED`], wenn es keine
    /// Linien gibt (gescheiterte Ansicht, Abbruch, nichts zu berechnen).
    fn lines_text(&self, state: &AppState, status: &ExportStatus) -> String {
        let counts = match status {
            ExportStatus::DrawingReady {
                visible, hidden, ..
            } => Some((*visible, *hidden)),
            ExportStatus::Computing(_) => return LINES_COMPUTING.to_string(),
            ExportStatus::Failed(_) => {
                let key = self.drawing_key(state);
                match key.and_then(|k| self.prepared_for(k.format)) {
                    Some((k, Err(f))) if Some(*k) == key => f.lines,
                    _ => None,
                }
            }
            _ => None,
        };
        match counts {
            Some((visible, hidden)) => lines_counts_text(visible, hidden),
            None => LINES_NOT_COMPUTED.to_string(),
        }
    }

    /// Das Ergebnis des Zeichnungsformats `format`, falls es eines gibt.
    fn prepared_for(&self, format: DrawingFormat) -> Option<&(DrawKey, DrawResult)> {
        self.drawing_prepared
            .iter()
            .find(|(k, _)| k.format == format)
    }

    /// Merkt sich ein Ergebnis und ersetzt das alte desselben Formats.
    fn store_prepared(&mut self, entry: (DrawKey, DrawResult)) {
        self.drawing_prepared
            .retain(|(k, _)| k.format != entry.0.format);
        self.drawing_prepared.push(entry);
    }

    /// Schreibt die fertige Zeichnung (bzw. bei „Alle Blätter“ alle Blätter in eine PDF) im
    /// Arbeitsthread nach `path` (über eine Hilfsdatei); nur wenn alles berechnet ist und das
    /// Ergebnis zum Stand von `state` passt.
    fn write_drawing(&mut self, path: PathBuf, format: DrawingFormat, state: &AppState) {
        if let Some(m) = self.drawing_block(state) {
            self.set_written(Err(m));
            return;
        }
        let key = self.drawing_key(state);
        let prepared = match self.prepared_for(format) {
            Some((k, _)) if Some(*k) != key => {
                self.set_written(Err(STALE.to_string()));
                return;
            }
            Some((_, Ok(p))) => Arc::clone(p),
            Some((_, Err(f))) => {
                let m = f.message.clone();
                self.set_written(Err(m));
                return;
            }
            None => {
                self.set_written(Err(NOT_COMPUTED.to_string()));
                return;
            }
        };
        let target = path.clone();
        self.written = None;
        let job: DrawWriteJob = if format == DrawingFormat::Pdf {
            let pages = if key.is_some_and(|k| k.target == DrawTarget::All) {
                PdfPages::All
            } else {
                PdfPages::Selected
            };
            let meta = pdf_meta(
                state,
                pages,
                self.current_drawing(state).map(|d| d.id),
                &Stamp::now().date,
            );
            let pdf_writer = self.pdf_writer;
            Job::spawn(move |p| {
                let models: Vec<&SheetModel> = prepared.iter().map(|d| &*d.model).collect();
                pdf_writer(&target, &models, &meta, p).map_err(draw_write_text)
            })
        } else {
            let writer = self.writer;
            Job::spawn(move |p| match prepared.first() {
                Some(d) => writer(&target, &d.model, format, p).map_err(draw_write_text),
                None => Err(NOT_COMPUTED.to_string()),
            })
        };
        self.drawing_writing = Some((path, job));
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
            if let Some((key, ..)) = self.drawing_job.take() {
                self.store_prepared((
                    key,
                    Err(DrawFailed::plain(ExchangeError::Cancelled.to_string())),
                ));
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
            if let ExportFormat::Drawing(f) = self.format {
                self.drawing_prepared.retain(|(k, _)| k.format != f);
            }
        } else if self.format.is_step() {
            self.step_prepared = None;
        } else {
            self.prepared = None;
        }
    }

    /// Öffnet die Dateiauswahl im Ordner `dir` mit dem Namen des Körpers (bei mehreren
    /// „Körper“), der Zeichnung bzw. bei „Alle Blätter“ des Dokuments (P10); Zeichen, die in
    /// Dateinamen nicht erlaubt sind, werden „_“.
    pub fn begin_save(&mut self, dir: PathBuf, state: &AppState) {
        let stem = if self.all_sheets() && !state.document_name.trim().is_empty() {
            state.document_name.clone()
        } else if self.format.is_drawing() {
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
                let writer = self.step_writer;
                self.written = None;
                self.step_writing =
                    Some((path, Job::spawn(move |p| writer(&target, &prepared, p))));
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
    ///
    /// `places` sind die gemeinsamen Schnellzugriffe der Dateiauswahl (Favoriten,
    /// Projektordner); geänderte Favoriten stehen danach darin.
    pub fn show_in(
        &mut self,
        ctx: &egui::Context,
        state: &mut AppState,
        last_dir: &mut Option<PathBuf>,
        places: &mut Places,
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
        let mut with_frame = self.with_frame;
        // Karte „Zeichnung“: Auswahl und Angaben (Linien erst, wenn alles berechnet ist).
        let drawing_list: Vec<(DrawingId, String)> = state
            .document
            .drawings()
            .iter()
            .map(|d| (d.id, shorten_for_display(&d.name)))
            .collect();
        let current = self.current_drawing(state);
        let is_list = current.is_some_and(|d| matches!(d.kind, DrawingKind::PartsList { .. }));
        let mut picked = current.map(|d| d.id);
        let picked_before = picked;
        let is_pdf = before == ExportFormat::Drawing(DrawingFormat::Pdf);
        let all = self.all_sheets();
        let mut pdf_pages = self.pdf_pages;
        let sheet_no = self.computing_sheet();
        let lines_line = self.lines_text(state, &status);
        let mut drawing_info: Vec<String> = Vec::new();
        if all {
            // Alle Blätter: Zahl der Blätter und Summen über alle (P9).
            let drawings = state.document.drawings();
            drawing_info.push(format!("Blätter: {}", drawings.len()));
            let (views, dims, centerlines) = match &status {
                ExportStatus::DrawingReady {
                    views,
                    dimensions,
                    centerlines,
                    ..
                } => (*views, *dimensions, *centerlines),
                _ => (
                    drawings.iter().map(|d| d.views.len()).sum(),
                    drawings.iter().map(|d| d.dimensions.len()).sum(),
                    centerline_use_all(drawings),
                ),
            };
            drawing_info.push(format!("Ansichten: {}", group_digits(views as u64)));
            drawing_info.push(lines_line);
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
        } else if let Some(d) = current {
            let views = match &status {
                ExportStatus::DrawingReady { views, .. } => *views,
                _ => d.views.len(),
            };
            drawing_info.push(format!("Ansichten: {views}"));
            drawing_info.push(format!("Blatt: {}", sheet_label(d.sheet)));
            drawing_info.push(lines_line);
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
            if is_list {
                // Ein Stücklistenblatt hat weder Ansichten noch Maße.
                drawing_info.retain(|l| l.starts_with("Blatt:"));
            }
        }
        let writing_file = self.is_writing_file();
        let (mut open, mut close, mut cancel, mut recompute, mut begin, mut drop_chooser) =
            (true, false, false, false, false, false);
        let mut cancel_write = false;
        let mut chosen: Option<PathBuf> = None;
        let mut cards = Vec::new();
        // Die Dateiauswahl zeichnet oben (rollt) und in der Fußzeile (Speichern, Abbrechen).
        let chooser = RefCell::new(&mut self.chooser);
        let (mut body_action, mut footer_action) = (ChooserAction::None, ChooserAction::None);
        let response = Dialog::new("Exportieren", DialogSize::Wide)
            .open(&mut open)
            .show_with_footer(
                ctx,
                |ui| {
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
                                egui::RadioButton::new(
                                    scope == Scope::Selected,
                                    "Gewählter Körper",
                                ),
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
                        // Nur bei PDF: das gewählte Blatt oder alle Blätter (P9).
                        if is_pdf {
                            ui.radio_value(&mut pdf_pages, PdfPages::Selected, PDF_SELECTED);
                            ui.radio_value(&mut pdf_pages, PdfPages::All, PDF_ALL);
                        }
                        if drawing_list.is_empty() {
                            ui.weak("Keine Zeichnung vorhanden.");
                        } else {
                            let shown = drawing_list
                                .iter()
                                .find(|(id, _)| Some(*id) == picked)
                                .map_or("", |(_, n)| n.as_str());
                            ui.add_enabled_ui(!all, |ui| {
                                egui::ComboBox::from_id_salt("export-drawing")
                                    .selected_text(shown)
                                    .width(ui.available_width())
                                    .show_ui(ui, |ui| {
                                        for (id, name) in &drawing_list {
                                            ui.selectable_value(
                                                &mut picked,
                                                Some(*id),
                                                name.as_str(),
                                            );
                                        }
                                    });
                            })
                            .response
                            .on_disabled_hover_text(PDF_ALL_HINT);
                            if all {
                                ui.weak(PDF_ALL_HINT);
                            }
                        }
                        for line in &drawing_info {
                            ui.label(line);
                        }
                        ui.add_space(4.0);
                        ui.add(theme::Toggle::new(&mut with_frame, FRAME_SWITCH));
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
                            let text = match sheet_no {
                                Some((n, total)) if all => sheet_progress_text(n, total),
                                _ => computing_text(before, is_list).to_string(),
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
                            ui.colored_label(error_color(ui.visuals().dark_mode), m);
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
                                ui.colored_label(
                                    warning_color(ui.visuals().dark_mode),
                                    report_text(report),
                                );
                                ui.colored_label(
                                    warning_color(ui.visuals().dark_mode),
                                    NOT_CLOSED_WARNING,
                                );
                            }
                        }
                        ExportStatus::StepReady {
                            bodies,
                            faces,
                            rejected,
                            skipped_meshes,
                        } => {
                            if *bodies == 0 {
                                ui.colored_label(
                                    warning_color(ui.visuals().dark_mode),
                                    STEP_NOTHING,
                                );
                            } else {
                                ui.label(format!(
                                    "{} Körper, {} Flächen",
                                    group_digits(*bodies as u64),
                                    group_digits(*faces as u64)
                                ));
                            }
                            for r in rejected {
                                ui.colored_label(warning_color(ui.visuals().dark_mode), r);
                            }
                            if !skipped_meshes.is_empty() {
                                ui.colored_label(
                                    warning_color(ui.visuals().dark_mode),
                                    skipped_text(skipped_meshes),
                                );
                            }
                        }
                        ExportStatus::DrawingReady {
                            skipped_meshes,
                            outside,
                            ..
                        } => {
                            ui.label(if all {
                                "Alle Blätter sind berechnet."
                            } else if is_list {
                                "Die Stückliste ist berechnet."
                            } else {
                                "Alle Ansichten sind berechnet."
                            });
                            if *outside > 0 {
                                ui.colored_label(
                                    warning_color(ui.visuals().dark_mode),
                                    outside_text(*outside),
                                );
                            }
                            if !skipped_meshes.is_empty() {
                                ui.colored_label(
                                    warning_color(ui.visuals().dark_mode),
                                    drawing_skipped_text(skipped_meshes),
                                );
                            }
                        }
                        ExportStatus::Unavailable(m) => {
                            ui.colored_label(warning_color(ui.visuals().dark_mode), m);
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
                                ui.colored_label(
                                    warning_color(ui.visuals().dark_mode),
                                    WRITING_HINT,
                                );
                            }
                        }
                        WriteState::Done(path, n) => {
                            ui.label(format!(
                                "Fertig: „{}“ geschrieben ({}).",
                                file_name(path),
                                mb_text(*n)
                            ));
                            if !shortened.is_empty() {
                                ui.colored_label(
                                    warning_color(ui.visuals().dark_mode),
                                    shortened_text(&shortened),
                                );
                            }
                        }
                        WriteState::Failed(m) => {
                            ui.colored_label(
                                error_color(ui.visuals().dark_mode),
                                format!("Nicht geschrieben: {m}"),
                            );
                        }
                    }
                    ui.separator();
                    match chooser.borrow_mut().as_mut() {
                        Some(c) => {
                            ui.strong("Speichern unter");
                            body_action = c.show_body(ui, places);
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
                },
                |ui| {
                    // Fußzeile: „Speichern“ und „Abbrechen“ bleiben stehen, auch wenn der Inhalt
                    // darüber rollt.
                    if let Some(c) = chooser.borrow_mut().as_mut() {
                        footer_action = c.show_footer(ui);
                    }
                },
            );
        for action in [body_action, footer_action] {
            match action {
                ChooserAction::Chosen(p) => chosen = Some(p),
                ChooserAction::Cancel => drop_chooser = true,
                ChooserAction::None => {}
            }
        }
        let want_close = !open || close;
        let alive = !want_close || !self.request_close();
        self.last_rect = response.map(|r| r.response.rect);
        self.last_cards = cards;
        (self.format, self.quality, self.scope) = (format, quality, scope);
        let toggled = with_frame != self.with_frame;
        self.with_frame = with_frame;
        let other_drawing = picked != picked_before;
        if other_drawing {
            self.drawing = picked;
        }
        let other_pages = pdf_pages != self.pdf_pages;
        self.pdf_pages = pdf_pages;
        let stale_chooser = self
            .chooser_key
            .is_some_and(|k| k != self.chooser_key_now(state));
        if self.format != before || drop_chooser || stale_chooser {
            self.chooser = None;
            self.chooser_key = None;
        }
        if self.format != before || stale_chooser || other_drawing || toggled || other_pages {
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

#[cfg(test)]
mod line_count_tests {
    use super::*;
    use planora_drawing::sheet::build_sheet;
    use planora_drawing::{Paper, Placement, Projection, Scale, ViewKind};
    use planora_math::DVec3;

    /// Fehlt eine Ansicht auf dem Blatt, ist die Zahl der Linien unvollständig: keine Zahl.
    #[test]
    fn counts_only_when_every_view_is_on_the_sheet() {
        let mut s = AppState::new();
        s.create_box(DVec3::new(100.0, 60.0, 40.0)).unwrap();
        let sheet = Sheet {
            paper: Paper::A3,
            landscape: true,
        };
        let d = s
            .document
            .add_drawing("Blatt", sheet, Projection::First)
            .unwrap();
        let one = Scale { num: 1, den: 1 };
        for (kind, x) in [(ViewKind::Front, 120.0), (ViewKind::Right, 300.0)] {
            s.document
                .add_view(d, kind, one, Placement::Free { x, y: 200.0 }, None)
                .unwrap();
        }
        let drawing = s.document.drawing(d).unwrap();
        let bodies: Vec<DrawBody> = s
            .bodies
            .iter()
            .map(|b| DrawBody {
                name: b.name.clone(),
                solid: &b.solid,
            })
            .collect();
        let lines: Vec<(ViewId, ViewLines)> = drawing
            .views
            .iter()
            .map(|v| {
                (
                    v.id,
                    compute_view(&bodies, v.kind, &Progress::new()).unwrap(),
                )
            })
            .collect();
        let all: BTreeMap<ViewId, &ViewLines> = lines.iter().map(|(i, l)| (*i, l)).collect();
        let full = build_sheet(drawing, &all);
        assert_eq!(full.views.len(), 2);
        let counts = sheet_line_counts(drawing, &full).expect("alle Ansichten auf dem Blatt");
        assert!(counts.0 > 0, "{counts:?}");
        // Nur die erste Ansicht berechnet: das Blatt hat eine Ansicht weniger.
        let first: BTreeMap<ViewId, &ViewLines> =
            all.iter().take(1).map(|(i, l)| (*i, *l)).collect();
        let part = build_sheet(drawing, &first);
        assert_eq!(part.views.len(), 1, "Testvoraussetzung");
        assert!(
            line_count(&part, Visibility::Visible) > 0,
            "Testvoraussetzung"
        );
        assert_eq!(sheet_line_counts(drawing, &part), None);
    }
}
