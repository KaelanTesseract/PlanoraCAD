//! Import-Pipeline und Normalisieren (STEP-Import, Aufgabe 9).
//!
//! **Ablauf je Körper** ([`import_bodies`]): Einheiten seines Kontexts (P11: die gewählte
//! Einheit ersetzt die Längeneinheit aller Darstellungen, auch der Lagen), Aufbau
//! ([`build_solid`]), Lage anwenden, **dann** normalisieren. Ein Körper, der sich nicht lesen
//! oder bauen lässt, wird mit seinem Grund ausgelassen; die übrigen bleiben. Abbrechen gibt
//! `Err(Cancelled)` für den ganzen Aufruf (nie ein halbes Ergebnis). Geschrieben wird nur in den
//! Speicher, nie in Dateien.
//!
//! **Normalisieren (P5):** [`normalize`] schreibt den Körper mit unserem Schreiber bei festem
//! Zeitstempel (1970-01-01T00:00:00) und festem Dateinamen [`NORMAL_FILE_NAME`], liest den Text
//! mit [`read_normal_body`] wieder und liefert Text **und** diesen gelesenen Körper. Weil die
//! Sitzung genau den Körper benutzt, den auch das Laden der Projektdatei aus dem Text baut, ist
//! Sitzung = Neuladen (gleiche Flächen, Marken „STEP-Fläche n“, Ecken und Kantenparameter).
//!
//! **Freie Volumen-Einträge** (in keiner Darstellung) haben keinen eigenen Kontext. Sie bekommen
//! die Einheiten der Datei, wenn alle Kontexte mit Längeneinheit in Länge und Winkel
//! übereinstimmen; nennt kein Kontext eine Längeneinheit, gelten Millimeter (als unbekannt
//! gemeldet); widersprechen sich die Kontexte, ist die Einheit unklar und der Körper wird
//! ausgelassen – nie still Millimeter in einer Zoll-Datei.
//!
//! **Fortschritt (P15):** Gesamtzahl = Summe der Flächen aller Körper innerhalb der
//! Flächengrenze (vorab über die Schalen gezählt, [`count_faces`]); je Fläche ein Schritt, auch
//! für Körper, die scheitern; Körper über der Grenze zählen nicht. Am Ende steht der Anteil auf 1.

use crate::read::assembly::{FoundBody, find_bodies, placement_note, unsupported_kinds};
use crate::read::error::{ReadLimits, StepReadError};
use crate::read::geom::Skip;
use crate::read::index::{StepFile, StepHeader};
use crate::read::topo::{BuildOptions, build_solid, count_faces};
use crate::read::units::{DEFAULT_FILE_TOL_MM, LengthUnit, Units, context_units};
use crate::topology::CLASSIFY_CHORD;
use crate::{StepBody, StepError, StepOptions, check_body, write_step};
use planora_exchange::Progress;
use planora_kernel::Solid;
use planora_math::{DVec3, Tolerance};
use planora_mesh::{TessellationOptions, tessellate};
use std::time::UNIX_EPOCH;

/// Dateiname im Kopf jedes normalisierten Textes.
pub const NORMAL_FILE_NAME: &str = "planora-step-koerper.step";

/// Höchstens so lang (Bytes) ist ein normalisierter Text (Prüfung beim Laden).
pub const MAX_STEP_TEXT_BYTES: usize = 64 * 1024 * 1024;

/// Name in Meldungen für Körper ohne Namen (die App vergibt später „STEP-Körper n“, siehe
/// [`ImportedBody::warnings_named`]).
pub const UNNAMED_BODY: &str = "STEP-Körper";

const NORMAL_START: &str = "ISO-10303-21;";
const NORMAL_END: &str = "END-ISO-10303-21;";

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ImportOptions {
    /// Gewählte Längeneinheit (P11) statt der aus der Datei.
    pub unit_override: Option<LengthUnit>,
}

/// Normalisierter Körper: Text unseres Schreibers und der daraus gelesene Körper.
#[derive(Clone, Debug)]
pub struct NormalBody {
    pub text: String,
    pub solid: Solid,
}

/// Ein Körper der Datei mit Prüfergebnis.
#[derive(Clone, Debug)]
pub struct ImportedBody {
    /// Name nach P10; leer, wenn die Datei keinen nennt.
    pub name: String,
    /// Flächen des fertigen Körpers; bei ausgelassenen die Zahl in der Datei (0, wenn unlesbar).
    pub faces: usize,
    /// Hülle in mm nach der Lage (Ecken und ein Netz mit Sehne 0,05 mm); `None` bei
    /// ausgelassenen Körpern.
    pub bounds: Option<(DVec3, DVec3)>,
    /// Genauigkeit der Datei (`weld_tol`, P4) in mm.
    pub tolerance_mm: f64,
    /// Warnungen: grobe Genauigkeit, dann die Lage-Meldung (immer zuletzt).
    pub warnings: Vec<String>,
    /// Grund der Lage-Meldung (für [`ImportedBody::warnings_named`]).
    pub placement_reason: Option<String>,
    /// Der normalisierte Körper oder der Grund des Auslassens.
    pub outcome: Result<NormalBody, String>,
}

impl ImportedBody {
    /// Warnungen mit `shown` als Name in der Lage-Meldung; für Körper ohne Namen gibt die App
    /// hier den Namen, den sie vergibt („STEP-Körper n“).
    pub fn warnings_named(&self, shown: &str) -> Vec<String> {
        let mut w = self.warnings.clone();
        if let (Some(reason), Some(last)) = (&self.placement_reason, w.last_mut()) {
            *last = placement_note(shown, reason);
        }
        w
    }
}

/// Ergebnis der Prüfung einer Datei.
#[derive(Clone, Debug)]
pub struct StepImport {
    pub header: StepHeader,
    /// Längeneinheit, mit der gerechnet wurde (die gewählte oder die der Datei).
    pub unit: LengthUnit,
    /// Nennt die Datei eine Längeneinheit? (`false`: angenommen werden Millimeter.)
    pub unit_known: bool,
    pub bodies: Vec<ImportedBody>,
    /// Meldungen zur ganzen Datei: Körperarten, die nicht übernommen werden.
    pub messages: Vec<String>,
}

/// Einheiten der Datei je Kontext, mit der gewählten Längeneinheit.
struct FileUnits<'f> {
    file: &'f StepFile,
    unit_override: Option<LengthUnit>,
}

impl FileUnits<'_> {
    fn apply(&self, u: Units) -> Units {
        match self.unit_override {
            Some(l) => u.with_length(l),
            None => u,
        }
    }

    /// Einheiten des Kontexts `context`; ohne Kontext die der ganzen Datei ([`free_units`]).
    fn of(&self, context: Option<u64>) -> Result<Units, StepReadError> {
        match context {
            Some(c) => context_units(self.file, c).map(|u| self.apply(u)),
            None => free_units(self.file, self.unit_override.is_some()).map(|u| self.apply(u)),
        }
    }
}

/// Einheiten für Einträge ohne Kontext: Alle Kontexte, die eine Längeneinheit nennen, müssen in
/// Länge und Winkel übereinstimmen (`length_free`: die Länge wird ohnehin ersetzt, nur der
/// Winkel zählt). Genauigkeit: die gröbste der Kontexte (fehlt sie in einem, gilt sie als
/// unbekannt). Nennt keiner eine Längeneinheit: Millimeter, unbekannt.
fn free_units(file: &StepFile, length_free: bool) -> Result<Units, StepReadError> {
    let mut found: Option<Units> = None;
    for &c in file.ids_of("GEOMETRIC_REPRESENTATION_CONTEXT") {
        let u = context_units(file, c)?;
        if !u.length_known {
            continue;
        }
        found = Some(match found {
            None => u,
            Some(f) => {
                if f.angle_rad != u.angle_rad || (!length_free && f.length != u.length) {
                    return Err(StepReadError::UnclearUnits);
                }
                let uncertainty_mm = match (f.uncertainty_mm, u.uncertainty_mm) {
                    (Some(a), Some(b)) => Some(a.max(b)),
                    _ => None,
                };
                Units {
                    uncertainty_mm,
                    uncertainty_follows_length: f.uncertainty_follows_length
                        && u.uncertainty_follows_length,
                    ..f
                }
            }
        });
    }
    Ok(found.unwrap_or(Units {
        length_known: false,
        ..Units::MM
    }))
}

/// Liest und prüft alle Körper der Datei (Spec §4b.3: unlesbare oder ungültige Körper werden
/// mit Grund ausgelassen, die übrigen übernommen). Fehler der ganzen Datei (keine Körper, mehr
/// als 50, unlesbare Baugruppe) und Abbrechen sind `Err`.
pub fn import_bodies(
    file: &StepFile,
    opts: &ImportOptions,
    progress: &Progress,
) -> Result<StepImport, StepReadError> {
    let cancelled = || -> Result<(), StepReadError> {
        if progress.is_cancelled() {
            Err(StepReadError::Cancelled)
        } else {
            Ok(())
        }
    };
    cancelled()?;
    let units = FileUnits {
        file,
        unit_override: opts.unit_override,
    };
    let found = find_bodies(file, &|c| units.of(c))?;
    cancelled()?;
    let limits: &ReadLimits = file.limits();
    // Flächen je Körper (wie `read_raw` sie zählt) und ob sie im Fortschritt zählen.
    let counts: Vec<(usize, u64)> = found
        .iter()
        .map(|b| match count_faces(file, &b.items) {
            Ok(n) if n <= limits.max_faces => (n, n as u64),
            Ok(n) => (n, 0),
            Err(_) => (0, 0),
        })
        .collect();
    let total: u64 = counts.iter().map(|c| c.1).sum();
    progress.set_total(total.max(1));
    let mut bodies = Vec::with_capacity(found.len());
    for (b, &(faces, steps)) in found.iter().zip(&counts) {
        cancelled()?;
        bodies.push(import_one(file, &units, b, faces, steps, progress)?);
    }
    cancelled()?;
    if total == 0 {
        progress.advance(1);
    }
    let (unit, unit_known) = file_unit(&units, &found);
    Ok(StepImport {
        header: file.header().clone(),
        unit: opts.unit_override.unwrap_or(unit),
        unit_known,
        bodies,
        messages: unsupported_kinds(file),
    })
}

/// Längeneinheit der Datei (ohne die Wahl): die des ersten lesbaren Kontexts eines Körpers,
/// sonst die der ganzen Datei, sonst Millimeter (unbekannt).
fn file_unit(units: &FileUnits, found: &[FoundBody]) -> (LengthUnit, bool) {
    let plain = FileUnits {
        file: units.file,
        unit_override: None,
    };
    found
        .iter()
        .find_map(|b| plain.of(b.context).ok())
        .map_or((LengthUnit::Millimeter, false), |u| {
            (u.length, u.length_known)
        })
}

/// Ein Körper: Einheiten, Aufbau, Lage, Normalisieren. `Err` nur beim Abbrechen.
fn import_one(
    file: &StepFile,
    units: &FileUnits,
    b: &FoundBody,
    faces: usize,
    steps: u64,
    progress: &Progress,
) -> Result<ImportedBody, StepReadError> {
    let shown = if b.name.is_empty() {
        UNNAMED_BODY
    } else {
        &b.name
    };
    let placement_reason = b
        .placement_note
        .as_deref()
        .and_then(|note| reason_of(&b.name, note));
    let mut body = ImportedBody {
        name: b.name.clone(),
        faces,
        bounds: None,
        tolerance_mm: DEFAULT_FILE_TOL_MM,
        warnings: Vec::new(),
        placement_reason: placement_reason.clone(),
        outcome: Err(String::new()),
    };
    let note = match (&placement_reason, &b.placement_note) {
        (Some(reason), _) => Some(placement_note(shown, reason)),
        (None, note) => note.clone(),
    };
    let u = match units.of(b.context) {
        Ok(u) => u,
        Err(e) => {
            // Nicht gebaut: Die Flächen zählen trotzdem.
            progress.advance(steps);
            body.warnings.extend(note);
            body.outcome = Err(e.to_string());
            return Ok(body);
        }
    };
    body.tolerance_mm = u.weld_tol_mm();
    let built = build_solid(
        file,
        u,
        &b.items,
        &BuildOptions {
            weld_tol_mm: body.tolerance_mm,
        },
        progress,
    );
    if progress.is_cancelled() || matches!(&built, Err(Skip(m)) if m == "Abgebrochen") {
        return Err(StepReadError::Cancelled);
    }
    body.outcome = match built {
        Ok(built) => {
            body.warnings.extend(built.warnings);
            match built.solid.transformed(&b.placement) {
                Ok(placed) => normalize(&placed, &b.name),
                Err(_) => Err("Der Körper lässt sich nicht an seine Lage bringen.".into()),
            }
        }
        Err(Skip(m)) => Err(m),
    };
    body.warnings.extend(note);
    if let Ok(n) = &body.outcome {
        body.faces = n.solid.faces.len();
        body.bounds = Some(bounds_of(&n.solid));
    }
    Ok(body)
}

/// Grund aus einer Meldung von [`placement_note`] mit dem Namen `name`.
fn reason_of(name: &str, note: &str) -> Option<String> {
    let probe = placement_note(name, "\u{0}");
    let (head, tail) = probe.split_once('\u{0}')?;
    note.strip_prefix(head)?
        .strip_suffix(tail)
        .map(str::to_string)
}

/// Hülle (mm): Ecken und ein Netz mit Sehne [`CLASSIFY_CHORD`] (darum bei gekrümmten Flächen
/// höchstens so viel zu klein); lässt sich kein Netz bilden, nur die Ecken.
fn bounds_of(solid: &Solid) -> (DVec3, DVec3) {
    let mut lo = DVec3::splat(f64::INFINITY);
    let mut hi = DVec3::splat(f64::NEG_INFINITY);
    let mut add = |p: DVec3| {
        lo = lo.min(p);
        hi = hi.max(p);
    };
    for (_, v) in solid.vertices.iter() {
        add(v.point);
    }
    if let Ok(mesh) = tessellate(
        solid,
        &TessellationOptions {
            chord_tolerance: CLASSIFY_CHORD,
        },
    ) {
        for &p in &mesh.positions {
            add(p);
        }
    }
    (lo, hi)
}

/// Satz mit Schlusspunkt (Gründe des Schreibers enden ohne).
fn sentence(s: &str) -> String {
    if s.ends_with(['.', '!', '?']) {
        s.to_string()
    } else {
        format!("{s}.")
    }
}

/// Grund aus einem Fehler des Schreibers.
fn writer_reason(e: StepError) -> String {
    match e {
        StepError::InvalidBody { reason, .. } => sentence(&reason),
        other => sentence(&other.to_string()),
    }
}

/// Höchstens so oft wird beim Normalisieren geschrieben und wieder gelesen, bis der Text sich
/// nicht mehr ändert.
const MAX_NORMAL_ROUNDS: usize = 4;

/// Schreibt einen Körper mit festem Zeitstempel und Dateinamen.
fn write_normal(solid: &Solid, name: &str) -> Result<String, String> {
    let opts = StepOptions {
        file_name: NORMAL_FILE_NAME.to_string(),
        time: UNIX_EPOCH,
        tolerance: Tolerance::default(),
    };
    let body = StepBody {
        name: name.to_string(),
        solid,
    };
    let quiet = Progress::new();
    // Erst prüfen: Bei nur einem Körper meldete `write_step` sonst nur „nichts auszugeben“.
    check_body(&body, &opts, &quiet).map_err(writer_reason)?;
    let mut out = Vec::new();
    write_step(&[body], &opts, &mut out, &quiet).map_err(writer_reason)?;
    String::from_utf8(out).map_err(|_| "Der Schreiber lieferte keinen Text.".to_string())
}

/// Normalisiert einen Körper (P5): schreibt ihn mit festem Zeitstempel und Dateinamen, liest
/// den Text wieder ([`read_normal_body`]) und liefert Text und gelesenen Körper. Weil Lesen und
/// Schreiben in den letzten Stellen runden können (etwa der Bezugsort eines Kegels in der Mitte
/// seines Bereichs), wird der gelesene Körper noch einmal geschrieben, bis der Text gleich
/// bleibt (höchstens [`MAX_NORMAL_ROUNDS`] Lesungen); meist kostet das nur ein weiteres
/// Schreiben. Damit ist Normalisieren ruhend: `normalize(&n.solid).text == n.text`. Text und
/// Körper gehören immer zusammen (`read_normal_body(&n.text)` ergibt `n.solid`). Lehnt der
/// Schreiber ab (etwa ein Torus mit kleinem Radius ≥ großem), ist sein Grund der Fehler.
pub fn normalize(solid: &Solid, name: &str) -> Result<NormalBody, String> {
    let mut text = write_normal(solid, name)?;
    let mut solid = read_normal_body(&text)?;
    for _ in 1..MAX_NORMAL_ROUNDS {
        let Ok(again) = write_normal(&solid, name) else {
            break;
        };
        if again == text {
            break;
        }
        let Ok(next) = read_normal_body(&again) else {
            break;
        };
        text = again;
        solid = next;
    }
    Ok(NormalBody { text, solid })
}

/// Billige Prüfung eines gespeicherten Textes (P14): nur ASCII, beginnt mit „ISO-10303-21;“,
/// endet mit „END-ISO-10303-21;“ (danach nur Leerraum), höchstens [`MAX_STEP_TEXT_BYTES`]. Der
/// Inhalt wird nicht gelesen.
pub fn check_normal_header(text: &str) -> Result<(), String> {
    if text.len() > MAX_STEP_TEXT_BYTES {
        return Err("Der Text ist größer als 64 MB.".into());
    }
    if !text.is_ascii() {
        return Err("Der Text enthält Zeichen außerhalb von ASCII.".into());
    }
    if !text.starts_with(NORMAL_START) || !text.trim_end().ends_with(NORMAL_END) {
        return Err("Der Text hat nicht Anfang und Ende einer STEP-Datei.".into());
    }
    Ok(())
}

/// Liest einen normalisierten Text: Kopf wie [`check_normal_header`], genau ein Körper,
/// Millimeter, Einheitslage; Marken „STEP-Fläche n“ (P7). Sonst `Err` mit Grund.
pub fn read_normal_body(text: &str) -> Result<Solid, String> {
    check_normal_header(text)?;
    let quiet = Progress::new();
    let file = StepFile::parse(text.as_bytes().to_vec(), &ReadLimits::DEFAULT, &quiet)
        .map_err(|e| e.to_string())?;
    let units_of = |c: Option<u64>| match c {
        Some(c) => context_units(&file, c),
        None => free_units(&file, false),
    };
    let found = find_bodies(&file, &units_of).map_err(|e| e.to_string())?;
    let [body] = found.as_slice() else {
        return Err(format!(
            "Der Text enthält {} Körper statt einem.",
            found.len()
        ));
    };
    let units = units_of(body.context).map_err(|e| e.to_string())?;
    if units.length != LengthUnit::Millimeter || !units.length_known {
        return Err("Der Text ist nicht in Millimeter geschrieben.".into());
    }
    if body.placement_note.is_some() || body.placement != planora_math::DMat4::IDENTITY {
        return Err("Der Körper im Text liegt nicht an der Einheitslage.".into());
    }
    let built = build_solid(
        &file,
        units,
        &body.items,
        &BuildOptions {
            weld_tol_mm: units.weld_tol_mm(),
        },
        &quiet,
    )
    .map_err(|Skip(m)| m)?;
    Ok(built.solid)
}
