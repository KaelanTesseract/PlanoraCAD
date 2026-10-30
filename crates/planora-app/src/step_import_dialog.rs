//! Fenster „STEP importieren“ (STEP-Import, Spec §2, §3, Plan Aufgabe 14). Ablauf wie beim Netz-
//! Import: Datei wählen (`.step`, `.stp`), dann zwei Aufträge im Arbeitsthread nacheinander,
//! beide mit Fortschritt und Abbrechen (P15): „Lesen“ ([`read_step_file`]) und „Prüfen“
//! ([`import_bodies`], je Auftrag ein frischer `Progress`). Danach zeigt das Fenster zwei Karten:
//! „Datei“ (Name, Format, Einheit der Datei mit Auswahl) und „Körper“ (Tabelle mit Häkchen und
//! Prüfergebnis je Körper). Eine andere Einheit rechnet im Hintergrund auf der schon gelesenen
//! Datei neu (P11); die Häkchen bleiben je Zeile. „Importieren“ legt die angehakten Körper in
//! **einem** Schritt an ([`AppState::import_step`]).
//!
//! Wie beim Netz-Import (P13) gilt ein Ergebnis nur für den Dokumentstand bei der Dateiwahl:
//! hat sich das Dokument geändert, wird es verworfen ([`stale`]). Das Fenster zählt zu
//! `DocTab::dialog_open` (Rückgängig wirkt nicht, solange es offen ist). Schließen oder Verwerfen
//! des Fensters bricht einen laufenden Auftrag ab (`Job` bricht beim Fallenlassen ab), danach
//! kommt kein Ergebnis mehr an. Eine offene Skizze beendet schon das Öffnen des Fensters
//! (`app.rs`), damit Rückgängig des Imports nur den Import zurücknimmt.
//!
//! Die Größe in der Liste ist die Hülle des geprüften Körpers auf 0,1 mm gerundet. Bei Körpern
//! mit gekrümmten Flächen stammt sie aus einem Netz und ist bis zu 0,05 mm je Seite zu klein;
//! dort steht „etwa“ davor (nur Übersicht, nie als genaues Maß).

use crate::browser::error_color;
use crate::cards::card_row;
use crate::dialog::{Dialog, DialogSize};
use crate::export_dialog::{PROGRESS_WIDTH, REPAINT, file_name};
use crate::file_chooser::{ChooserAction, FileChooser};
use crate::import_dialog::{Unit, lost, stale};
use crate::jobs::{Job, Poll};
use crate::mesh_body::warning_color;
use crate::msg::Msg;
use crate::places::Places;
use crate::state::AppState;
use crate::theme;
use planora_exchange::Progress;
use planora_feature::{FeatureId, StepShape};
use planora_i18n::tr;
use planora_kernel::Surface;
use planora_math::DVec3;
use planora_step::read::assembly::{find_bodies, unsupported_kinds};
use planora_step::read::units::context_units;
use planora_step::{
    ImportOptions, LengthUnit, ReadLimits, StepFile, StepHeader, StepImport, StepReadError,
    UnitState, import_bodies, read_step_file, unnamed_body,
};
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Fenstertitel (und Name des Rückgängig-Schritts).
pub fn title() -> String {
    tr!("stepimport-title")
}
/// Eintrag im Menü „Datei“ (direkt unter „Netz importieren …“).
pub fn menu_label() -> String {
    tr!("menu-file-import-step")
}
/// Phase des ersten Auftrags.
pub fn reading() -> String {
    tr!("import-reading")
}
/// Phase des zweiten Auftrags.
pub fn checking() -> String {
    tr!("stepimport-checking")
}
/// Hinweis, wenn die Datei keine Längeneinheit nennt (Millimeter angenommen).
pub fn no_unit_hint() -> String {
    tr!("stepimport-no-unit")
}
/// Hinweis, wenn die Datei widersprüchliche Längeneinheiten nennt (betroffene Körper werden
/// ausgelassen, bis eine Einheit gewählt ist).
pub fn unclear_unit_hint() -> String {
    tr!("stepimport-unclear-unit")
}
/// Hinweis, wenn sich die Einheit der Datei nicht lesen lässt (kein Widerspruch).
pub fn unreadable_unit_hint() -> String {
    tr!("stepimport-unreadable-unit")
}
/// Hinweis bei einer Datei, deren Körper in verschiedenen Einheiten stehen (jeder Körper wird in
/// seiner eigenen gelesen).
pub fn mixed_unit_hint() -> String {
    tr!("stepimport-mixed-unit-hint")
}
/// Hinweis, wenn ein Körper ohne eigene Einheit wegen widersprüchlicher Einheiten ausgelassen ist.
pub fn free_skipped_hint() -> String {
    tr!("stepimport-free-skipped")
}
/// Text der Spalte „Einheit“, wenn sich die Einheit eines Körpers nicht sicher zeigen lässt.
pub fn mixed_unit() -> String {
    tr!("stepimport-mixed-unit")
}
/// Höchstens so viele ausgelassene Körper nennt [`summary`] mit Grund.
pub const MAX_SUMMARY_SKIPPED: usize = 3;
/// Hinweis am gesperrten Knopf „Importieren“.
pub fn nothing_checked() -> String {
    tr!("stepimport-nothing-checked")
}
/// Hinweis an der gerundeten Größe.
pub fn size_hint() -> String {
    tr!("stepimport-size-hint")
}

/// Einheit der Oberfläche ↔ Längeneinheit des Lesers (gleiche Namen).
pub fn length_of(unit: Unit) -> LengthUnit {
    match unit {
        Unit::Millimeter => LengthUnit::Millimeter,
        Unit::Zentimeter => LengthUnit::Zentimeter,
        Unit::Meter => LengthUnit::Meter,
        Unit::Zoll => LengthUnit::Zoll,
    }
}

fn unit_of(length: LengthUnit) -> Option<Unit> {
    match length {
        LengthUnit::Millimeter => Some(Unit::Millimeter),
        LengthUnit::Zentimeter => Some(Unit::Zentimeter),
        LengthUnit::Meter => Some(Unit::Meter),
        LengthUnit::Zoll => Some(Unit::Zoll),
        LengthUnit::Other { .. } => None,
    }
}

/// Vorgewählter Radioknopf: die gewählte Einheit, sonst die der Datei. Nennt die Datei
/// widersprüchliche Einheiten und ist keine gewählt, ist nichts vorgewählt (die angenommenen
/// Millimeter wären sonst schon „gewählt“). Eine andere Einheit der Datei
/// ([`LengthUnit::Other`]) hat keinen Knopf.
pub fn selected_unit(import: &StepImport, chosen: Option<LengthUnit>) -> Option<Unit> {
    match chosen {
        Some(l) => unit_of(l),
        None if import.unit_state == UnitState::Unclear => None,
        None => unit_of(import.unit),
    }
}

/// Was die Datei über ihre Längeneinheiten sagt (einmal je Prüfung bestimmt).
#[derive(Clone, Debug, PartialEq)]
pub struct UnitInfo {
    pub state: UnitState,
    /// Längeneinheit der Datei ohne Wahl (bei [`UnitState::Missing`] die angenommenen
    /// Millimeter); aus der ersten Prüfung, bleibt bei jeder Neuberechnung.
    pub own: LengthUnit,
    /// Die verschiedenen Längeneinheiten, die die Kontexte der Datei nennen.
    pub contexts: Vec<LengthUnit>,
    /// Ein Kontext der Datei lässt sich nicht lesen.
    pub unreadable: bool,
    /// Ein Körper ohne eigenen Kontext ist wegen widersprüchlicher Einheiten ausgelassen.
    pub free_skipped: bool,
}

impl UnitInfo {
    /// Nennen die Kontexte der Datei verschiedene Längeneinheiten?
    pub fn mixed(&self) -> bool {
        self.contexts.len() > 1
    }
}

/// Hinweis unter der Einheitenwahl. Ohne Wahl: keine Längeneinheit (Millimeter angenommen),
/// widersprüchliche oder unlesbare Einheiten, Körper in verschiedenen Einheiten, ein freier
/// Körper wegen unklarer Einheit ausgelassen. Mit Wahl nennt er die gewählte Einheit, bei einer
/// Datei mit eigener Einheit auch diese (die Wahl ersetzt dann die Einheit der Datei).
pub fn unit_hint(info: &UnitInfo, chosen: Option<LengthUnit>) -> Option<String> {
    let label = length_label;
    match (info.state, chosen) {
        (UnitState::Named, None) if info.free_skipped => Some(free_skipped_hint()),
        (UnitState::Named, None) if info.mixed() => Some(mixed_unit_hint()),
        (UnitState::Named, None) => None,
        (UnitState::Named, Some(u)) if info.mixed() => {
            Some(tr!("stepimport-hint-mixed-chosen-all", unit = label(u)))
        }
        (UnitState::Named, Some(u)) => Some(tr!(
            "stepimport-hint-named-chosen",
            own = label(info.own),
            unit = label(u)
        )),
        (UnitState::Missing, None) => Some(no_unit_hint()),
        (UnitState::Missing, Some(u)) => {
            Some(tr!("stepimport-hint-missing-chosen", unit = label(u)))
        }
        (UnitState::Unclear, None) if info.mixed() => Some(unclear_unit_hint()),
        (UnitState::Unclear, None) => Some(unreadable_unit_hint()),
        (UnitState::Unclear, Some(u)) if info.mixed() => {
            Some(tr!("stepimport-hint-mixed-chosen", unit = label(u)))
        }
        (UnitState::Unclear, Some(u)) => {
            Some(tr!("stepimport-hint-unreadable-chosen", unit = label(u)))
        }
    }
}

/// Name einer Längeneinheit: die bekannten in der Sprache der Oberfläche, jede andere wie
/// vom Leser benannt.
fn length_label(u: LengthUnit) -> String {
    unit_of(u).map_or_else(|| u.label(), Unit::label)
}

/// Längeneinheiten der Kontexte der Datei (verschiedene, in Reihenfolge) und ob sich einer nicht
/// lesen lässt.
fn context_survey(file: &StepFile) -> (Vec<LengthUnit>, bool) {
    let mut units = Vec::new();
    let mut unreadable = false;
    for &c in file.ids_of("GEOMETRIC_REPRESENTATION_CONTEXT") {
        match context_units(file, c) {
            Ok(u) if u.length_known => {
                if !units.contains(&u.length) {
                    units.push(u.length);
                }
            }
            Ok(_) => {}
            Err(_) => unreadable = true,
        }
    }
    (units, unreadable)
}

/// Einheit in einer Zeile der Tabelle (als Wert; der Text entsteht beim Zeichnen).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RowUnit {
    /// Die Einheit, mit der gerechnet wurde.
    Unit(LengthUnit),
    /// Die Einheit lässt sich nicht sicher zeigen ([`mixed_unit`]).
    Mixed,
    /// Ausgelassener Körper: keine Einheit, mit der gerechnet wurde.
    Skipped,
}

impl RowUnit {
    /// Text in der aktuellen Sprache.
    pub fn text(self) -> String {
        match self {
            RowUnit::Unit(u) => unit_short(u),
            RowUnit::Mixed => mixed_unit(),
            RowUnit::Skipped => "–".into(),
        }
    }
}

/// Einheit je Zeile: mit Wahl die gewählte, sonst bei einer Datei mit einer Einheit diese. Bei
/// Kontexten in verschiedenen Einheiten wird die Einheit jedes Körpers aus seinem eigenen
/// Kontext bestimmt (gleiche Suche wie der Import); lässt sie sich so nicht sicher zuordnen,
/// steht [`RowUnit::Mixed`] da statt einer falschen Einheit. Ausgelassene Körper:
/// [`RowUnit::Skipped`].
fn row_units(
    file: &StepFile,
    import: &StepImport,
    chosen: Option<LengthUnit>,
    info: &UnitInfo,
) -> Vec<RowUnit> {
    let per_body: Vec<RowUnit> = match chosen {
        Some(u) => vec![RowUnit::Unit(u); import.bodies.len()],
        None if !info.mixed() => vec![RowUnit::Unit(import.unit); import.bodies.len()],
        None => {
            // Ohne Wahl rechnet der Import freie Körper mit den Einheiten der ganzen Datei; die
            // widersprechen sich hier, also „unklar“ (wie im Leser).
            let units = |c: Option<u64>| match c {
                Some(c) => context_units(file, c),
                None => Err(StepReadError::UnclearUnits),
            };
            match find_bodies(file, &units) {
                Ok(found)
                    if found.len() == import.bodies.len()
                        && found
                            .iter()
                            .zip(&import.bodies)
                            .all(|(f, b)| f.name == b.name) =>
                {
                    found
                        .iter()
                        .map(|f| match f.context.map(|c| context_units(file, c)) {
                            Some(Ok(u)) if u.length_known => RowUnit::Unit(u.length),
                            _ => RowUnit::Mixed,
                        })
                        .collect()
                }
                _ => vec![RowUnit::Mixed; import.bodies.len()],
            }
        }
    };
    per_body
        .into_iter()
        .zip(&import.bodies)
        .map(|(u, b)| {
            if b.outcome.is_ok() {
                u
            } else {
                RowUnit::Skipped
            }
        })
        .collect()
}

/// Kurzform der Einheit in der Tabelle.
fn unit_short(unit: LengthUnit) -> String {
    match unit {
        LengthUnit::Millimeter => "mm".into(),
        LengthUnit::Zentimeter => "cm".into(),
        LengthUnit::Meter => "m".into(),
        LengthUnit::Zoll => tr!("import-unit-inch-short"),
        other => other.label(),
    }
}

/// Zahl mit höchstens einer Nachkommastelle und dem Dezimaltrenner der Sprache, ohne „,0“.
fn tenth(v: f64) -> String {
    let r = (v * 10.0).round() / 10.0;
    let r = if r == 0.0 { 0.0 } else { r };
    let s = format!("{r:.1}");
    planora_i18n::localize_decimal(s.strip_suffix(".0").unwrap_or(&s))
}

/// Maße der Hülle, auf 0,1 mm gerundet: „25,4 × 50,8 × 76,2 mm“; ohne Hülle „–“.
pub fn size_text(bounds: Option<(DVec3, DVec3)>) -> String {
    match bounds {
        Some((lo, hi)) => {
            let d = (hi - lo).max(DVec3::ZERO);
            format!("{} × {} × {} mm", tenth(d.x), tenth(d.y), tenth(d.z))
        }
        None => "–".into(),
    }
}

/// Größe einer Zeile der Tabelle als Wert (der Text entsteht beim Zeichnen).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RowSize {
    /// Hülle des geprüften Körpers.
    pub bounds: Option<(DVec3, DVec3)>,
    /// Körper mit gekrümmten Flächen: Die Hülle stammt aus einem Netz und ist bis zu 0,05 mm je
    /// Seite zu klein, darum steht „etwa“ davor.
    pub curved: bool,
}

impl RowSize {
    /// [`size_text`], bei gekrümmten Flächen mit „etwa“ davor.
    pub fn text(&self) -> String {
        let size = size_text(self.bounds);
        if self.curved {
            tr!("stepimport-about", size = size)
        } else {
            size
        }
    }
}

/// Größe in der Tabelle (siehe [`RowSize`]).
fn row_size(body: &planora_step::ImportedBody) -> RowSize {
    let curved = body.outcome.as_ref().is_ok_and(|n| {
        n.solid
            .faces
            .iter()
            .any(|(_, f)| !matches!(f.surface, Surface::Plane { .. }))
    });
    RowSize {
        bounds: body.bounds,
        curved,
    }
}

/// Meldung nach dem Import: „3 Körper importiert“ bzw. „3 Körper importiert, 1 ausgelassen:
/// Name (Grund)“ (mehrere mit „; “).
///
/// Damit die Meldung in der Statuszeile lesbar bleibt, stehen höchstens
/// [`MAX_SUMMARY_SKIPPED`] Körper mit Grund da, danach „und n weitere“.
pub fn summary(imported: usize, skipped: &[(String, String)]) -> String {
    let head = tr!("stepimport-summary", n = imported);
    if skipped.is_empty() {
        return head;
    }
    let mut list: Vec<String> = skipped
        .iter()
        .take(MAX_SUMMARY_SKIPPED)
        .map(|(name, why)| format!("{name} ({why})"))
        .collect();
    if skipped.len() > MAX_SUMMARY_SKIPPED {
        list.push(tr!(
            "stepimport-and-more",
            n = skipped.len() - MAX_SUMMARY_SKIPPED
        ));
    }
    tr!(
        "stepimport-summary-skipped",
        head = head,
        n = skipped.len(),
        list = list.join("; ")
    )
}

/// Format der Datei für die Karte „Datei“ („STEP AP214“), aus dem ersten Schema.
fn format_text(header: &StepHeader) -> String {
    let Some(schema) = header.schemas.first() else {
        return "STEP".into();
    };
    let upper = schema.trim().to_ascii_uppercase();
    let ap = if upper.starts_with("AUTOMOTIVE_DESIGN") || upper.starts_with("AP214") {
        "AP214"
    } else if upper.starts_with("CONFIG_CONTROL_DESIGN") || upper.starts_with("AP203") {
        "AP203"
    } else if upper.starts_with("AP242") {
        "AP242"
    } else {
        return "STEP".into();
    };
    format!("STEP {ap}")
}

/// Häkchen nach einer Prüfung: übernehmbare Körper an, ausgelassene aus. Nach einer
/// Neuberechnung derselben Datei (`before`) behält jede Zeile ihr Häkchen, solange der Körper
/// übernehmbar bleibt; ein neu übernehmbarer ist an.
fn checks_after(before: Option<(&StepImport, &[bool])>, import: &StepImport) -> Vec<bool> {
    import
        .bodies
        .iter()
        .enumerate()
        .map(|(i, b)| {
            if b.outcome.is_err() {
                return false;
            }
            match before {
                Some((old, checked)) if old.bodies.get(i).is_some_and(|o| o.outcome.is_ok()) => {
                    checked.get(i).copied().unwrap_or(true)
                }
                _ => true,
            }
        })
        .collect()
}

/// Namen der angehakten, übernehmbaren Körper (in Reihenfolge) – so, wie sie an
/// [`AppState::import_step`] gehen.
fn given_names(import: &StepImport, checked: &[bool]) -> Vec<(usize, String)> {
    import
        .bodies
        .iter()
        .enumerate()
        .filter(|(i, b)| b.outcome.is_ok() && checked.get(*i).copied().unwrap_or(false))
        .map(|(i, b)| (i, b.name.clone()))
        .collect()
}

/// Angezeigte Namen je Zeile: angehakte Körper heißen, wie der Import sie nennen wird
/// ([`AppState::step_body_names`], also „STEP-Körper n“ für unbenannte), die übrigen wie in der
/// Datei („STEP-Körper“ ohne Nummer, wenn sie keinen Namen nennt).
fn shown_names(import: &StepImport, checked: &[bool], state: &AppState) -> Vec<String> {
    let given = given_names(import, checked);
    let planned = state.step_body_names(&given.iter().map(|g| g.1.clone()).collect::<Vec<_>>());
    let mut names: Vec<String> = import
        .bodies
        .iter()
        .map(|b| {
            if b.name.trim().is_empty() {
                unnamed_body().to_string()
            } else {
                b.name.clone()
            }
        })
        .collect();
    for ((i, _), name) in given.iter().zip(planned) {
        names[*i] = name;
    }
    names
}

/// Ton einer Statuszeile.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Tone {
    Plain,
    Warning,
    Error,
}

/// Status eines Körpers: „wird übernommen“, „wird übernommen – {Warnung}“ oder
/// „wird ausgelassen: {Grund}“; die Lage-Meldung nennt den angezeigten Namen.
fn status(body: &planora_step::ImportedBody, shown: &str) -> (String, Tone) {
    match &body.outcome {
        Err(why) => (tr!("stepimport-status-skipped", why = why), Tone::Error),
        Ok(_) => {
            let warnings = body.warnings_named(shown);
            if warnings.is_empty() {
                (tr!("stepimport-status-ok"), Tone::Plain)
            } else {
                (
                    tr!(
                        "stepimport-status-ok-warning",
                        warnings = warnings.join(" ")
                    ),
                    Tone::Warning,
                )
            }
        }
    }
}

/// Gelesene und geprüfte Datei.
pub struct StepLoaded {
    pub path: PathBuf,
    pub file: Arc<StepFile>,
    pub import: StepImport,
    /// Änderungszähler des Dokuments bei der Dateiwahl; `None` ohne Bindung an ein Dokument.
    pub edits: Option<u64>,
    /// Einheiten der Datei.
    pub units: UnitInfo,
    /// Größe je Zeile (einmal je Prüfung bestimmt, nicht je Bild; der Text entsteht beim
    /// Zeichnen).
    pub sizes: Vec<RowSize>,
    /// Einheit je Zeile.
    pub row_units: Vec<RowUnit>,
}

/// Ergebnis einer Prüfung im Arbeitsthread.
struct Checked {
    import: StepImport,
    units: UnitInfo,
    sizes: Vec<RowSize>,
    row_units: Vec<RowUnit>,
}

/// Abgelehnte Datei: Meldung und Hinweise (etwa die Körperarten, die sie stattdessen enthält).
struct Refused {
    message: String,
    notes: Vec<String>,
}

type ReadJob = Job<Result<StepFile, StepReadError>>;
type CheckJob = Job<Result<Checked, Refused>>;

/// Laufender Auftrag.
enum Work {
    Read {
        path: PathBuf,
        edits: Option<u64>,
        job: ReadJob,
    },
    Check {
        path: PathBuf,
        edits: Option<u64>,
        file: Arc<StepFile>,
        unit: Option<LengthUnit>,
        job: CheckJob,
    },
}

impl Work {
    fn cancel(&self) {
        match self {
            Work::Read { job, .. } => job.cancel(),
            Work::Check { job, .. } => job.cancel(),
        }
    }

    fn fraction(&self) -> f32 {
        match self {
            Work::Read { job, .. } => job.fraction(),
            Work::Check { job, .. } => job.fraction(),
        }
    }
}

/// Startet das Prüfen der gelesenen Datei mit frischem Fortschritt; `own` ist die Einheit der
/// Datei aus der ersten Prüfung (`None` bei der ersten).
fn check_job(file: &Arc<StepFile>, unit: Option<LengthUnit>, own: Option<LengthUnit>) -> CheckJob {
    let file = Arc::clone(file);
    Job::spawn(move |progress| {
        let opts = ImportOptions {
            unit_override: unit,
        };
        let import = import_bodies(&file, &opts, progress).map_err(|e| Refused {
            // Ohne Volumenkörper: sagen, was die Datei stattdessen enthält.
            notes: if e == StepReadError::NoBodies {
                unsupported_kinds(&file)
            } else {
                Vec::new()
            },
            message: e.to_string(),
        })?;
        let (contexts, unreadable) = context_survey(&file);
        let unclear = StepReadError::UnclearUnits.to_string();
        let units = UnitInfo {
            state: import.unit_state,
            own: own.unwrap_or(import.unit),
            contexts,
            unreadable,
            free_skipped: import
                .bodies
                .iter()
                .any(|b| b.outcome.as_ref().err() == Some(&unclear)),
        };
        let sizes = import.bodies.iter().map(row_size).collect();
        let row_units = row_units(&file, &import, unit, &units);
        Ok(Checked {
            import,
            units,
            sizes,
            row_units,
        })
    })
}

pub struct StepImportDialog {
    chooser: FileChooser,
    /// Die Dateiwahl ist zu sehen (ohne gelesene Datei immer; danach über „Andere Datei
    /// wählen …“).
    choosing: bool,
    work: Option<Work>,
    pub loaded: Option<StepLoaded>,
    /// Häkchen je Körper (gleiche Reihenfolge wie `loaded.import.bodies`).
    pub checked: Vec<bool>,
    /// Meldung im Fenster; wird beim Zeichnen übersetzt.
    pub error: Option<Msg>,
    /// Hinweise zur Meldung `error` (etwa Körperarten der Datei, die nicht übernommen werden).
    pub notes: Vec<String>,
    /// Die im Fenster gewählte Einheit, mit der `loaded` gerechnet ist (`None`: die der Datei).
    chosen: Option<LengthUnit>,
    /// Rechteck des Fensters im letzten Bild (Tests der vollen Breite).
    pub last_rect: Option<egui::Rect>,
    /// Rechtecke der Karten im letzten Bild (Tests der gleichen Höhe).
    pub last_cards: Vec<egui::Rect>,
    /// Kennung und Rechteck des Häkchens je Zeile im letzten Bild (Tests).
    pub last_rows: Vec<(egui::Id, egui::Rect)>,
}

impl StepImportDialog {
    pub fn new(dir: PathBuf) -> StepImportDialog {
        StepImportDialog {
            chooser: FileChooser::open(dir, vec!["step", "stp"]),
            choosing: true,
            work: None,
            loaded: None,
            checked: Vec::new(),
            error: None,
            notes: Vec::new(),
            chosen: None,
            last_rect: None,
            last_cards: Vec::new(),
            last_rows: Vec::new(),
        }
    }

    /// Liest und prüft `path` im Arbeitsthread (ohne Bindung an ein Dokument).
    pub fn load(&mut self, path: PathBuf) {
        self.load_with(path, None, Arc::new(Progress::new()));
    }

    /// Wie [`StepImportDialog::load`], das Ergebnis gilt aber nur für den Dokumentstand `edits`.
    pub fn load_at(&mut self, path: PathBuf, edits: u64) {
        self.load_with(path, Some(edits), Arc::new(Progress::new()));
    }

    /// Wie [`StepImportDialog::load_at`] mit vorgegebenem Fortschritt für das Lesen (Tests).
    #[doc(hidden)]
    pub fn load_with(&mut self, path: PathBuf, edits: Option<u64>, progress: Arc<Progress>) {
        self.error = None;
        self.notes.clear();
        self.loaded = None;
        self.checked.clear();
        self.chosen = None;
        self.choosing = false;
        let source = path.clone();
        // Ein noch laufender Auftrag wird hier fallengelassen und damit abgebrochen.
        self.work = Some(Work::Read {
            path,
            edits,
            job: Job::spawn_with(progress, move |p| {
                read_step_file(&source, &ReadLimits::DEFAULT, p)
            }),
        });
    }

    /// Rechnet die gelesene Datei mit der Einheit `unit` neu (P11, im Arbeitsthread). Die Liste
    /// erscheint wieder, wenn die Prüfung fertig ist; die Häkchen bleiben je Zeile. Ist `unit`
    /// die Einheit der Datei selbst, wird ohne Wahl gerechnet (jeder Kontext in seiner eigenen
    /// Einheit) – eine Wahl stellte sonst alle Kontexte um, auch die in anderen Einheiten.
    pub fn set_unit(&mut self, unit: LengthUnit) {
        let Some(l) = &self.loaded else { return };
        let over = if l.units.state != UnitState::Unclear && unit == l.units.own {
            None
        } else {
            Some(unit)
        };
        self.error = None;
        self.notes.clear();
        self.work = Some(Work::Check {
            path: l.path.clone(),
            edits: l.edits,
            file: Arc::clone(&l.file),
            unit: over,
            job: check_job(&l.file, over, Some(l.units.own)),
        });
    }

    /// Die im Fenster gewählte Einheit der angezeigten Liste (`None`: die der Datei).
    pub fn chosen_unit(&self) -> Option<LengthUnit> {
        self.chosen
    }

    pub fn is_loading(&self) -> bool {
        self.work.is_some()
    }

    pub fn progress(&self) -> f32 {
        self.work.as_ref().map_or(0.0, Work::fraction)
    }

    /// Text des laufenden Auftrags („Die Datei wird gelesen …“ bzw. „Die Körper werden
    /// geprüft …“).
    pub fn phase(&self) -> Option<String> {
        match self.work.as_ref()? {
            Work::Read { .. } => Some(reading()),
            Work::Check { .. } => Some(checking()),
        }
    }

    /// Die Meldung im Fenster in der aktuellen Sprache.
    pub fn error_text(&self) -> Option<String> {
        self.error.as_ref().map(Msg::text)
    }

    /// Bricht den laufenden Auftrag ab. Eine schon gezeigte Liste (Neuberechnung mit anderer
    /// Einheit) bleibt mit ihrer Einheit stehen.
    pub fn cancel(&mut self) {
        if let Some(work) = self.work.take() {
            work.cancel();
            self.error = Some(crate::msg!("projects-cancelled"));
        }
    }

    /// Holt Ergebnisse ab und startet nach dem Lesen das Prüfen; mit `block` wird gewartet, bis
    /// kein Auftrag mehr läuft (Tests).
    pub fn tick(&mut self, block: bool) {
        loop {
            let Some(work) = self.work.take() else { return };
            match work {
                Work::Read { path, edits, job } => match job.fetch(block) {
                    Poll::Pending => {
                        self.work = Some(Work::Read { path, edits, job });
                        return;
                    }
                    Poll::Ready(Ok(file)) => {
                        let file = Arc::new(file);
                        self.work = Some(Work::Check {
                            path,
                            edits,
                            job: check_job(&file, None, None),
                            file,
                            unit: None,
                        });
                    }
                    Poll::Ready(Err(e)) => self.error = Some(Msg::Raw(e.to_string())),
                    Poll::Lost => self.error = Some(lost()),
                },
                Work::Check {
                    path,
                    edits,
                    file,
                    unit,
                    job,
                } => match job.fetch(block) {
                    Poll::Pending => {
                        self.work = Some(Work::Check {
                            path,
                            edits,
                            file,
                            unit,
                            job,
                        });
                        return;
                    }
                    Poll::Ready(Ok(c)) => {
                        let before = self.loaded.as_ref().map(|l| (&l.import, &self.checked[..]));
                        self.checked = checks_after(before, &c.import);
                        self.chosen = unit;
                        self.loaded = Some(StepLoaded {
                            path,
                            file,
                            import: c.import,
                            edits,
                            units: c.units,
                            sizes: c.sizes,
                            row_units: c.row_units,
                        });
                    }
                    Poll::Ready(Err(r)) => {
                        self.error = Some(Msg::Raw(r.message));
                        self.notes = r.notes;
                    }
                    Poll::Lost => self.error = Some(lost()),
                },
            }
        }
    }

    fn is_stale(&self, state: &AppState) -> bool {
        self.loaded
            .as_ref()
            .is_some_and(|l| l.edits.is_some_and(|e| e != state.document.edit_count()))
    }

    /// Gehört die Liste noch zum Dokument? Sonst wird sie samt Neuberechnung verworfen.
    fn drop_if_stale(&mut self, state: &AppState) {
        if self.is_stale(state) {
            self.loaded = None;
            self.checked.clear();
            self.chosen = None;
            if matches!(self.work, Some(Work::Check { .. })) {
                self.work = None;
            }
            self.notes.clear();
            self.error = Some(stale());
            self.choosing = true;
        }
    }

    /// Legt die angehakten Körper in einem Schritt an. Bei Erfolg steht die Zusammenfassung als
    /// Hinweis in `state.info` (neutrale Farbe; sie überlebt den Verlaufsschritt des Imports),
    /// eine alte Meldung verschwindet. Jede Ablehnung (keine Datei, Prüfung läuft, veraltet,
    /// nichts angehakt, Import abgelehnt) steht in `state.message`. Das Dokument ändert sich
    /// nur bei Erfolg.
    pub fn import(&mut self, state: &mut AppState) -> Result<Vec<FeatureId>, String> {
        self.import_msg(state).map_err(|m| m.text())
    }

    /// Wie [`StepImportDialog::import`], der Fehler ist aber der Grund (das Fenster behält ihn
    /// und übersetzt ihn beim Zeichnen).
    fn import_msg(&mut self, state: &mut AppState) -> Result<Vec<FeatureId>, Msg> {
        match self.try_import(state) {
            Ok((ids, text)) => {
                state.message = None;
                state.info = Some(text);
                state.info_outlives_step = true;
                Ok(ids)
            }
            Err(e) => {
                state.message = Some(e.text());
                Err(e)
            }
        }
    }

    /// Wie [`StepImportDialog::import`], liefert die Zusammenfassung statt sie zu setzen.
    fn try_import(&self, state: &mut AppState) -> Result<(Vec<FeatureId>, String), Msg> {
        let Some(l) = &self.loaded else {
            return Err(crate::msg!("import-choose-file"));
        };
        if self.work.is_some() {
            return Err(crate::msg!("stepimport-wait"));
        }
        if self.is_stale(state) {
            return Err(stale());
        }
        let given = given_names(&l.import, &self.checked);
        if given.is_empty() {
            return Err(crate::msg!("stepimport-nothing-checked"));
        }
        let bodies: Vec<(String, Arc<StepShape>)> = given
            .iter()
            .filter_map(|(i, name)| {
                let b = &l.import.bodies[*i];
                let n = b.outcome.as_ref().ok()?;
                let shape = StepShape::with_solid(n.text.clone(), b.tolerance_mm, n.solid.clone());
                let shape = if b.tolerance_known {
                    shape
                } else {
                    shape.accuracy_unknown()
                };
                Some((name.clone(), Arc::new(shape)))
            })
            .collect();
        let skipped: Vec<(String, String)> = l
            .import
            .bodies
            .iter()
            .filter_map(|b| {
                let why = b.outcome.as_ref().err()?;
                let name = if b.name.trim().is_empty() {
                    unnamed_body().to_string()
                } else {
                    b.name.clone()
                };
                Some((name, why.clone()))
            })
            .collect();
        let ids = state.import_step(bodies).map_err(Msg::Raw)?;
        let text = summary(ids.len(), &skipped);
        Ok((ids, text))
    }

    /// Zeichnet das Fenster; `true`, solange es offen bleiben soll.
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
        self.tick(false);
        self.drop_if_stale(state);
        let mut close_footer = false;
        let (mut open, mut close, mut cancel, mut import, mut other, mut back) =
            (true, false, false, false, false, false);
        let mut chosen = None;
        let mut pick_unit = None;
        let mut cards = Vec::new();
        let mut rows = Vec::new();
        let loading = self.phase().map(|p| (p, self.progress()));
        let error = self.error_text();
        let names = self
            .loaded
            .as_ref()
            .map(|l| shown_names(&l.import, &self.checked, state))
            .unwrap_or_default();
        let mut checked = self.checked.clone();
        let choosing = self.choosing || self.loaded.is_none();
        let selected = self
            .loaded
            .as_ref()
            .and_then(|l| selected_unit(&l.import, self.chosen));
        let hint = self
            .loaded
            .as_ref()
            .and_then(|l| unit_hint(&l.units, self.chosen));
        let has_list = self.loaded.is_some() && loading.is_none();
        let any = checked.iter().any(|c| *c);
        let has_loaded = self.loaded.is_some();
        // Die Dateiauswahl zeichnet oben (rollt) und in der Fußzeile (Öffnen, Abbrechen).
        let chooser = RefCell::new(&mut self.chooser);
        let (mut body_action, mut footer_action) = (ChooserAction::None, ChooserAction::None);
        let response = Dialog::new(title(), DialogSize::File)
            .center_on_open()
            .open(&mut open)
            .show_with_footer(
                ctx,
                |ui| {
                    let dark = ui.visuals().dark_mode;
                    if choosing {
                        ui.strong(tr!("stepimport-choose"));
                        body_action = chooser.borrow_mut().show_body(ui, places);
                    }
                    if let Some((phase, f)) = &loading {
                        ui.horizontal(|ui| {
                            ui.add(
                                egui::ProgressBar::new(*f)
                                    .desired_width(PROGRESS_WIDTH)
                                    .text(phase),
                            );
                            if ui.button(tr!("common-cancel")).clicked() {
                                cancel = true;
                            }
                        });
                    }
                    if let Some(e) = &error {
                        ui.colored_label(error_color(dark), e);
                    }
                    for n in &self.notes {
                        ui.colored_label(warning_color(dark), n);
                    }
                    if let (Some(l), None) = (&self.loaded, &loading) {
                        ui.separator();
                        // „Datei“ und „Körper“ je über die ganze Breite untereinander (Entscheid
                        // des Controllers, Fix-Runde 1): die Tabelle braucht die Breite.
                        let mut file_card = |ui: &mut egui::Ui| {
                            file_card(ui, l, selected, hint.as_deref(), &mut pick_unit);
                        };
                        cards.extend(card_row(ui, "step-datei", &mut [&mut file_card]));
                        ui.add_space(theme::SPACE_2);
                        let mut body_card = |ui: &mut egui::Ui| {
                            rows = body_card(ui, l, &names, &mut checked);
                        };
                        cards.extend(card_row(ui, "step-koerper", &mut [&mut body_card]));
                    }
                },
                |ui| {
                    // Fußzeile: bleibt stehen, auch wenn eine lange Liste rollt. Bei der
                    // Dateiauswahl nur deren „Öffnen“ und „Abbrechen“ (kein „Schließen“).
                    if choosing {
                        footer_action = chooser.borrow_mut().show_footer(ui);
                        return;
                    }
                    ui.horizontal(|ui| {
                        if has_list {
                            if ui
                                .add_enabled(any, egui::Button::new(tr!("import-import")))
                                .on_disabled_hover_text(nothing_checked())
                                .clicked()
                            {
                                import = true;
                            }
                            if ui.button(tr!("stepimport-other-file")).clicked() {
                                other = true;
                            }
                        }
                        if ui.button(crate::dialog::close_label()).clicked() {
                            close_footer = true;
                        }
                    });
                },
            );
        for action in [body_action, footer_action] {
            match action {
                ChooserAction::Chosen(p) => chosen = Some(p),
                // Mit gelesener Liste zurück zur Liste, sonst Fenster schließen.
                ChooserAction::Cancel if has_loaded => back = true,
                ChooserAction::Cancel => close = true,
                ChooserAction::None => {}
            }
        }
        self.last_rect = response.map(|r| r.response.rect);
        self.last_cards = cards;
        self.last_rows = rows;
        if self.loaded.is_some() && loading.is_none() {
            self.checked = checked;
        }
        if loading.is_some() {
            ctx.request_repaint_after(REPAINT);
        }
        if cancel {
            self.cancel();
        }
        if other {
            self.choosing = true;
        }
        if back {
            self.choosing = false;
        }
        if let Some(u) = pick_unit {
            self.set_unit(u);
        }
        if let Some(p) = chosen {
            *last_dir = p.parent().map(Path::to_path_buf);
            self.load_at(p, state.document.edit_count());
        }
        if import {
            match self.import_msg(state) {
                Ok(_) => return false,
                Err(e) => self.error = Some(e),
            }
        }
        open && !close && !close_footer
    }
}

/// Karte „Datei“: Name, Format, Einheit der Datei mit Auswahl, Hinweise.
fn file_card(
    ui: &mut egui::Ui,
    l: &StepLoaded,
    selected: Option<Unit>,
    hint: Option<&str>,
    pick: &mut Option<LengthUnit>,
) {
    let dark = ui.visuals().dark_mode;
    ui.strong(tr!("stepimport-file"));
    ui.add(egui::Label::new(file_name(&l.path)).wrap());
    ui.label(format_text(&l.import.header));
    ui.add_space(4.0);
    ui.strong(tr!("import-unit-title"));
    ui.horizontal_wrapped(|ui| {
        for u in Unit::ALL {
            if ui.radio(selected == Some(u), u.label()).clicked() && selected != Some(u) {
                *pick = Some(length_of(u));
            }
        }
        if let LengthUnit::Other { .. } = l.import.unit {
            // Die Einheit der Datei ohne eigenen Knopf: sichtbar, aber nicht wählbar.
            ui.add_enabled(
                false,
                egui::RadioButton::new(selected.is_none(), l.import.unit.label()),
            );
        }
    });
    if let Some(h) = hint {
        ui.add(egui::Label::new(egui::RichText::new(h).color(warning_color(dark))).wrap());
    }
    for m in &l.import.messages {
        ui.add(egui::Label::new(egui::RichText::new(m).color(warning_color(dark))).wrap());
    }
}

/// Abstand zwischen den Spalten der Tabelle.
const COLUMN_GAP: f32 = 4.0;
/// Anteile der Spalten Name, Flächen, Größe, Einheit an der Breite ohne Häkchen.
const COLUMNS: [f32; 4] = [0.38, 0.12, 0.36, 0.14];

/// Leere Spalte des Häkchens (wie eine Zelle, damit die Spalten genau untereinander stehen).
fn indent(ui: &mut egui::Ui, width: f32) {
    ui.allocate_exact_size(egui::vec2(width, 0.0), egui::Sense::hover());
}

/// Eine Zelle fester Breite; der Text bricht innerhalb um.
fn cell(ui: &mut egui::Ui, width: f32, text: egui::RichText) -> egui::Response {
    ui.allocate_ui_with_layout(
        egui::vec2(width, 0.0),
        egui::Layout::top_down(egui::Align::Min),
        |ui| {
            ui.set_width(width);
            ui.add(egui::Label::new(text).wrap())
        },
    )
    .inner
}

/// Karte „Körper“: Tabelle (Name, Flächen, Größe, Einheit; darunter je Zeile der Status) mit
/// einem Häkchen je Zeile. Liefert Kennung und Rechteck der Häkchen.
fn body_card(
    ui: &mut egui::Ui,
    l: &StepLoaded,
    names: &[String],
    checked: &mut [bool],
) -> Vec<(egui::Id, egui::Rect)> {
    let import = &l.import;
    let p = theme::palette(ui.visuals().dark_mode);
    ui.strong(tr!("tree-folder-bodies"));
    ui.spacing_mut().item_spacing.x = COLUMN_GAP;
    // Spalte des Häkchens: so breit, wie egui ein Häkchen mindestens macht.
    let check_w = ui.spacing().interact_size.x.max(ui.spacing().icon_width);
    let rest = (ui.available_width() - check_w - COLUMN_GAP * 4.0).max(40.0);
    let widths = COLUMNS.map(|f| f * rest);
    let head = |t: &str| {
        egui::RichText::new(t)
            .font(theme::value_font())
            .color(p.text_secondary)
    };
    ui.horizontal_top(|ui| {
        indent(ui, check_w);
        let heads = [
            tr!("props-name"),
            tr!("props-faces"),
            tr!("props-size"),
            tr!("stepimport-col-unit"),
        ];
        for (w, t) in widths.iter().zip(&heads) {
            cell(ui, *w, head(t));
        }
    });
    ui.horizontal_top(|ui| {
        indent(ui, check_w);
        cell(ui, rest, head(&tr!("props-status")));
    });
    let mut rows = Vec::with_capacity(import.bodies.len());
    for (i, b) in import.bodies.iter().enumerate() {
        ui.separator();
        // Kennung je Zeile, nie aus dem Namen (Plan: Feldkennungen hängen nicht vom Text ab).
        ui.push_id(("step-row", i), |ui| {
            let ok = b.outcome.is_ok();
            let name = names.get(i).map_or(b.name.as_str(), String::as_str);
            ui.horizontal_top(|ui| {
                let r = ui
                    .allocate_ui_with_layout(
                        egui::vec2(check_w, 0.0),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            ui.set_width(check_w);
                            let mut on = ok && checked.get(i).copied().unwrap_or(false);
                            let r = ui.add_enabled(ok, egui::Checkbox::without_text(&mut on));
                            if ok && let Some(c) = checked.get_mut(i) {
                                *c = on;
                            }
                            r
                        },
                    )
                    .inner;
                rows.push((r.id, r.rect));
                cell(ui, widths[0], egui::RichText::new(name));
                cell(ui, widths[1], egui::RichText::new(b.faces.to_string()));
                let size = l
                    .sizes
                    .get(i)
                    .map_or_else(|| "–".to_string(), RowSize::text);
                cell(ui, widths[2], egui::RichText::new(size)).on_hover_text(size_hint());
                // Ausgelassene Körper: „–“ (keine Einheit, mit der gerechnet wurde).
                let unit = l
                    .row_units
                    .get(i)
                    .map_or_else(|| "–".to_string(), |u| u.text());
                cell(ui, widths[3], egui::RichText::new(unit));
            });
            let (text, tone) = status(b, name);
            let text = match tone {
                Tone::Plain => egui::RichText::new(text),
                Tone::Warning => egui::RichText::new(text).color(p.warning),
                Tone::Error => egui::RichText::new(text).color(p.error),
            };
            ui.horizontal_top(|ui| {
                indent(ui, check_w);
                cell(ui, rest, text);
            });
        });
    }
    let usable = import.bodies.iter().filter(|b| b.outcome.is_ok()).count();
    ui.add_space(4.0);
    ui.label(
        egui::RichText::new(tr!(
            "stepimport-usable",
            usable = usable,
            total = import.bodies.len()
        ))
        .color(p.text_secondary),
    );
    rows
}
