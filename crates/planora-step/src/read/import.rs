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
//! Dass ein zweites Normalisieren denselben Text gibt, ist in den Tests so (auch weit vom
//! Ursprung), aber nicht allgemein bewiesen; siehe [`normalize`]. Scheitert das Normalisieren
//! eines gelesenen Körpers, steht der Grund ohne „Zeile n“ da (die Zeile gehörte zu unserem
//! eigenen Text).
//!
//! **Freie Volumen-Einträge** (in keiner Darstellung) haben keinen eigenen Kontext. Sie bekommen
//! die Einheiten der Datei: Jeder Kontext mit Längeneinheit wird zuerst auf die gewählte Einheit
//! umgestellt, dann müssen alle in Länge und Winkel übereinstimmen (Genauigkeit: die gröbste);
//! nennt kein Kontext eine Längeneinheit, gelten Millimeter ([`UnitState::Missing`]);
//! widersprechen sich die Kontexte, ist die Einheit unklar ([`UnitState::Unclear`]) und der
//! Körper wird ausgelassen – nie still Millimeter in einer Zoll-Datei.
//!
//! **Lagen:** Lage-Meldungen aus Aufgabe 8 und, wo die Richtung einer Beziehung ohne Vorkommen
//! angenommen wurde, [`guessed_note`]; bei leerem Namen mit „STEP-Körper“, die App bildet sie mit
//! [`ImportedBody::warnings_named`] neu.
//!
//! **Fortschritt (P15):** Gesamtzahl = Summe der Flächen aller Körper innerhalb der
//! Flächengrenze (vorab über die Schalen gezählt, [`count_faces`]); je Fläche ein Schritt, auch
//! für Körper, die scheitern; Körper über der Grenze zählen nicht. Am Ende steht der Anteil auf 1.
//! `Progress` hat keinen Rücksetzer: Der Aufrufer gibt jedem Aufruf einen **frischen**
//! `Progress` (sonst zählt der Anteil die alten Schritte mit). Abbruch wird beim Lesen je Fläche,
//! zwischen den Stufen des Aufbaus und zwischen den Runden des Normalisierens geprüft.

use crate::read::assembly::{
    FoundBody, find_bodies, guessed_note, placement_note, unsupported_kinds,
};
use crate::read::error::{ReadLimits, StepReadError};
use crate::read::geom::Skip;
use crate::read::index::{StepFile, StepHeader};
use crate::read::topo::{BuildOptions, build_solid, count_faces, probe_chord};
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
    /// Die Lage folgt aus einer angenommenen Richtung der Baugruppe (Meldung
    /// [`guessed_note`] zuletzt in `warnings`).
    pub placement_guessed: bool,
    /// Der normalisierte Körper oder der Grund des Auslassens.
    pub outcome: Result<NormalBody, String>,
}

impl ImportedBody {
    /// Warnungen mit `shown` als Name in der Lage-Meldung; für Körper ohne Namen gibt die App
    /// hier den Namen, den sie vergibt („STEP-Körper n“).
    pub fn warnings_named(&self, shown: &str) -> Vec<String> {
        let mut w = self.warnings.clone();
        if let Some(last) = w.last_mut() {
            if let Some(reason) = &self.placement_reason {
                *last = placement_note(shown, reason);
            } else if self.placement_guessed {
                *last = guessed_note(shown);
            }
        }
        w
    }
}

/// Was die Datei über ihre Längeneinheit sagt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnitState {
    /// Die Datei nennt eine Längeneinheit.
    Named,
    /// Die Datei nennt keine (angenommen: Millimeter).
    Missing,
    /// Die Datei nennt verschiedene, die nicht zueinander passen.
    Unclear,
}

/// Ergebnis der Prüfung einer Datei.
#[derive(Clone, Debug)]
pub struct StepImport {
    pub header: StepHeader,
    /// Längeneinheit, mit der gerechnet wurde (die gewählte oder die der Datei).
    pub unit: LengthUnit,
    /// Nennt die Datei eine Längeneinheit eindeutig? (`unit_state == Named`, auch wenn eine
    /// Einheit gewählt ist.)
    pub unit_known: bool,
    /// Was die Datei über ihre Längeneinheit sagt (für den Hinweis im Fenster: keine oder
    /// widersprüchliche).
    pub unit_state: UnitState,
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
            None => free_units(self.file, self.unit_override),
        }
    }
}

/// Einheiten für Einträge ohne Kontext (schon mit der gewählten Längeneinheit `chosen`): Jeder
/// Kontext, der eine Längeneinheit nennt, wird zuerst auf die gewählte Einheit umgestellt (seine
/// Genauigkeit wird dabei mit umgerechnet, P11); danach müssen alle in Länge und Winkel
/// übereinstimmen, sonst ist die Einheit unklar. Genauigkeit: die gröbste (in mm, also
/// unabhängig von der Reihenfolge); fehlt sie in einem, gilt sie als unbekannt (grob). Nennt
/// keiner eine Längeneinheit: Millimeter, unbekannt (bzw. die gewählte Einheit).
fn free_units(file: &StepFile, chosen: Option<LengthUnit>) -> Result<Units, StepReadError> {
    let apply = |u: Units| match chosen {
        Some(l) => u.with_length(l),
        None => u,
    };
    let mut found: Option<Units> = None;
    for &c in file.ids_of("GEOMETRIC_REPRESENTATION_CONTEXT") {
        let u = context_units(file, c)?;
        if !u.length_known {
            continue;
        }
        let u = apply(u);
        found = Some(match found {
            None => u,
            Some(f) => {
                if f.angle_rad != u.angle_rad || f.length != u.length {
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
    Ok(found.unwrap_or_else(|| {
        apply(Units {
            length_known: false,
            ..Units::MM
        })
    }))
}

/// Liest und prüft alle Körper der Datei (Spec §4b.3: unlesbare oder ungültige Körper werden
/// mit Grund ausgelassen, die übrigen übernommen). Fehler der ganzen Datei (keine Körper, mehr
/// als 50, unlesbare Baugruppe) und Abbrechen sind `Err`. `progress` muss frisch sein (je
/// Aufruf ein neuer `Progress`; `set_total` setzt die schon gezählten Schritte nicht zurück).
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
    let (unit, unit_state) = file_unit(&units, &found);
    Ok(StepImport {
        header: file.header().clone(),
        unit: opts.unit_override.unwrap_or(unit),
        unit_known: unit_state == UnitState::Named,
        unit_state,
        bodies,
        messages: unsupported_kinds(file),
    })
}

/// Längeneinheit der Datei (ohne die Wahl) und was die Datei darüber sagt: die des ersten
/// Körpers, dessen Einheiten sich bestimmen lassen; lassen sie sich für keinen bestimmen
/// (verschiedene Einheiten, unlesbarer Kontext), ist sie unklar.
fn file_unit(units: &FileUnits, found: &[FoundBody]) -> (LengthUnit, UnitState) {
    let plain = FileUnits {
        file: units.file,
        unit_override: None,
    };
    match found.iter().find_map(|b| plain.of(b.context).ok()) {
        Some(u) if u.length_known => (u.length, UnitState::Named),
        Some(u) => (u.length, UnitState::Missing),
        None => (LengthUnit::Millimeter, UnitState::Unclear),
    }
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
        placement_guessed: b.placement_guessed,
        outcome: Err(String::new()),
    };
    let note = match (&placement_reason, &b.placement_note) {
        (Some(reason), _) => Some(placement_note(shown, reason)),
        (None, Some(note)) => Some(note.clone()),
        (None, None) => b.placement_guessed.then(|| guessed_note(shown)),
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
                Ok(placed) => {
                    match normalize_until(&placed, &b.name, &|| progress.is_cancelled()) {
                        Ok(n) => Ok(n),
                        Err(Stop::Cancelled) => return Err(StepReadError::Cancelled),
                        Err(Stop::Failed(m)) => Err(normal_failure(&m)),
                    }
                }
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

/// Hülle (mm): Ecken und ein Netz mit Sehne [`CLASSIFY_CHORD`], bei großen Körpern
/// [`probe_chord`] (darum bei gekrümmten Flächen höchstens so viel zu klein: 0,05 mm, bei 10 m
/// Größe 5 mm); lässt sich kein Netz bilden, nur die Ecken.
fn bounds_of(solid: &Solid) -> (DVec3, DVec3) {
    let mut lo = DVec3::splat(f64::INFINITY);
    let mut hi = DVec3::splat(f64::NEG_INFINITY);
    for (_, v) in solid.vertices.iter() {
        lo = lo.min(v.point);
        hi = hi.max(v.point);
    }
    // Große Körper relativ zur Größe (wie die Seitenprobe), damit die Zeit nicht mit der Größe
    // wächst; bis 100 mm Diagonale wie bisher 0,05 mm.
    let diag = if lo.is_finite() && hi.is_finite() {
        lo.distance(hi)
    } else {
        0.0
    };
    let chord = probe_chord(diag).max(CLASSIFY_CHORD);
    let mut add = |p: DVec3| {
        lo = lo.min(p);
        hi = hi.max(p);
    };
    if let Ok(mesh) = tessellate(
        solid,
        &TessellationOptions {
            chord_tolerance: chord,
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

/// Warum das Normalisieren aufhörte.
#[derive(Debug, PartialEq)]
enum Stop {
    Cancelled,
    Failed(String),
}

/// Grund für einen Körper, der sich lesen, aber nicht normalisieren ließ: ohne „Zeile n:“
/// (die Zeile gehörte zu unserem eigenen Text, nicht zur Datei des Nutzers).
fn normal_failure(reason: &str) -> String {
    format!(
        "Der Körper lässt sich nicht in die Form des Programms umschreiben: {}",
        without_line(reason)
    )
}

/// `reason` ohne vorangestelltes „Zeile n: “.
fn without_line(reason: &str) -> &str {
    reason
        .strip_prefix("Zeile ")
        .and_then(|rest| rest.split_once(": "))
        .filter(|(n, _)| !n.is_empty() && n.bytes().all(|c| c.is_ascii_digit()))
        .map_or(reason, |(_, tail)| tail)
}

/// Schreibt `solid` erneut, bis der Text gleich bleibt (höchstens [`MAX_NORMAL_ROUNDS`]
/// Lesungen insgesamt, die erste macht der Aufrufer). Scheitert ein späteres Schreiben oder
/// Lesen, bleibt das letzte zusammengehörige Paar. `cancelled` wird vor jedem Schreiben und
/// Lesen gefragt.
fn settle<S>(
    mut text: String,
    mut solid: S,
    write: impl Fn(&S) -> Result<String, String>,
    read: impl Fn(&str) -> Result<S, String>,
    cancelled: &dyn Fn() -> bool,
) -> Result<(String, S), Stop> {
    for _ in 1..MAX_NORMAL_ROUNDS {
        if cancelled() {
            return Err(Stop::Cancelled);
        }
        let Ok(again) = write(&solid) else {
            break;
        };
        if again == text {
            break;
        }
        if cancelled() {
            return Err(Stop::Cancelled);
        }
        let Ok(next) = read(&again) else {
            break;
        };
        text = again;
        solid = next;
    }
    Ok((text, solid))
}

/// [`normalize`] mit Abbruchfrage zwischen den Schritten.
fn normalize_until(
    solid: &Solid,
    name: &str,
    cancelled: &dyn Fn() -> bool,
) -> Result<NormalBody, Stop> {
    if cancelled() {
        return Err(Stop::Cancelled);
    }
    let text = write_normal(solid, name).map_err(Stop::Failed)?;
    if cancelled() {
        return Err(Stop::Cancelled);
    }
    let first = read_normal_body(&text).map_err(Stop::Failed)?;
    let (text, solid) = settle(
        text,
        first,
        |s| write_normal(s, name),
        read_normal_body,
        cancelled,
    )?;
    Ok(NormalBody { text, solid })
}

/// Normalisiert einen Körper (P5): schreibt ihn mit festem Zeitstempel und Dateinamen, liest
/// den Text wieder ([`read_normal_body`]) und liefert Text und gelesenen Körper; Text und
/// Körper gehören immer zusammen (`read_normal_body(&n.text)` ergibt `n.solid`, Sitzung =
/// Neuladen). Die erste Lesung kann den Körper in den letzten Stellen ändern (etwa Ecken, die
/// der Leser auf eine Kegelspitze oder einen Pol zieht); darum wird der gelesene Körper noch
/// einmal geschrieben und nur dann noch einmal gelesen, wenn der Text sich dabei ändert
/// (höchstens [`MAX_NORMAL_ROUNDS`] Lesungen). Ein gelesener Körper ist für den Schreiber ein
/// Fixpunkt (Kegel und Zylinder behalten ihren Bezugsort), so dass in allen Tests nach
/// höchstens zwei Lesungen `normalize(&n.solid).text == n.text` gilt; garantiert ist das nicht,
/// die Schleife bleibt als Sicherheitsnetz. Lehnt der Schreiber ab, ist sein Grund der Fehler
/// (bei Körpern aus unserem Leser bisher nicht beobachtet).
pub fn normalize(solid: &Solid, name: &str) -> Result<NormalBody, String> {
    normalize_until(solid, name, &|| false).map_err(|stop| match stop {
        Stop::Failed(m) => m,
        Stop::Cancelled => "Abgebrochen".into(),
    })
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
        None => free_units(&file, None),
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    /// Lauf von [`settle`] mit Zahlen als „Körper“; Abbruch nach `cancel_after` Fragen. Liefert
    /// das Ergebnis und die Zahl der Fragen.
    fn run(
        start: i32,
        write: impl Fn(&i32) -> Result<String, String>,
        read: impl Fn(&str) -> Result<i32, String>,
        cancel_after: usize,
    ) -> (Result<(String, i32), Stop>, usize) {
        let asked = Cell::new(0);
        let cancelled = || {
            asked.set(asked.get() + 1);
            asked.get() > cancel_after
        };
        let r = settle(start.to_string(), start, write, read, &cancelled);
        (r, asked.get())
    }

    fn parse(t: &str) -> Result<i32, String> {
        t.parse().map_err(|_| String::from("keine Zahl"))
    }

    #[test]
    fn settle_stops_at_the_fixed_point() {
        let reads = Cell::new(0);
        // Schreiben zieht jede Zahl um eins hoch, höchstens auf 3: Fixpunkt nach 2 Lesungen.
        let (r, _) = run(
            1,
            |s| Ok((s + 1).min(3).to_string()),
            |t| {
                reads.set(reads.get() + 1);
                parse(t)
            },
            usize::MAX,
        );
        assert_eq!(r, Ok(("3".to_string(), 3)));
        assert_eq!(reads.get(), 2);
    }

    #[test]
    fn settle_gives_up_after_the_last_round() {
        let reads = Cell::new(0);
        let (r, _) = run(
            0,
            |s| Ok((s + 1).to_string()),
            |t| {
                reads.set(reads.get() + 1);
                parse(t)
            },
            usize::MAX,
        );
        // Insgesamt höchstens MAX_NORMAL_ROUNDS Lesungen (die erste macht der Aufrufer).
        assert_eq!(reads.get(), MAX_NORMAL_ROUNDS - 1);
        let (text, solid) = r.expect("Paar");
        assert_eq!(text, solid.to_string(), "Text und Körper gehören zusammen");
    }

    #[test]
    fn settle_keeps_the_last_pair_when_a_round_fails() {
        // Lesen scheitert ab der zweiten Runde.
        let (r, _) = run(
            0,
            |s| Ok((s + 1).to_string()),
            |t| match t {
                "1" => Ok(1),
                _ => Err("kaputt".into()),
            },
            usize::MAX,
        );
        assert_eq!(r, Ok(("1".to_string(), 1)));
        // Schreiben scheitert gleich.
        let (r, _) = run(5, |_| Err("nein".into()), |_| Ok(9), usize::MAX);
        assert_eq!(r, Ok(("5".to_string(), 5)));
    }

    #[test]
    fn settle_asks_for_cancel_before_writing_and_reading() {
        let (r, asked) = run(0, |s| Ok((s + 1).to_string()), parse, 0);
        assert_eq!(r, Err(Stop::Cancelled));
        assert_eq!(asked, 1, "vor dem Schreiben gefragt");
        let (r, asked) = run(0, |s| Ok((s + 1).to_string()), parse, 1);
        assert_eq!(r, Err(Stop::Cancelled));
        assert_eq!(asked, 2, "vor dem Lesen gefragt");
    }

    #[test]
    fn failures_of_our_own_text_show_no_line() {
        assert_eq!(
            normal_failure(
                "Zeile 121: Der Parameterbereich einer Fläche reicht über mehr als einen Umlauf."
            ),
            "Der Körper lässt sich nicht in die Form des Programms umschreiben: Der \
             Parameterbereich einer Fläche reicht über mehr als einen Umlauf."
        );
        assert_eq!(without_line("Zeile x: a"), "Zeile x: a");
        assert_eq!(without_line("Zeile : a"), "Zeile : a");
        assert_eq!(
            without_line("Der Körper ist ungültig: x"),
            "Der Körper ist ungültig: x"
        );
    }

    #[test]
    fn writer_reasons_end_with_a_full_stop() {
        let e = StepError::InvalidBody {
            name: "K".into(),
            reason: "Eine Kegelfläche liegt jenseits ihrer Spitze".into(),
        };
        assert_eq!(
            writer_reason(e),
            "Eine Kegelfläche liegt jenseits ihrer Spitze."
        );
        assert_eq!(
            writer_reason(StepError::NotFinite),
            "Eine Zahl ist zu groß oder keine gültige Zahl."
        );
        assert_eq!(sentence("Schon."), "Schon.");
    }
}
