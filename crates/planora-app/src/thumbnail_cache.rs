//! Miniatur eines Dokuments und ihr Zwischenspeicher im Dokumentordner (Plan P6, P8):
//! `vorschau.png` (496 × 280 RGBA) und `vorschau.json` (eine Zeile, die „fertig“-Marke).
//!
//! Gebaut wird immer aus einer gespeicherten Version (nie aus einem offenen, geänderten Tab):
//! lesen, `Document::from_snapshot`, sichtbare Körper vernetzen, zeichnen. Grenzen: Summe der
//! Dreiecke, Bauzeit und geschätzter Zeichenaufwand; darüber gilt die Miniatur als „zu
//! aufwendig“ und wird so gespeichert. Lese- und Aufbaufehler, auch eine Panik, ergeben
//! `Failed`, das nie gespeichert wird. Geschrieben werden nur `vorschau.png` und `vorschau.json`
//! (über eigene Zwischendateien je Schreibvorgang), sonst nichts im Dokumentordner.
//!
//! Absturzschutz: Vor dem Aufbau steht in `vorschau.json` die Marke `"art":"baut"` (mit Prozess
//! und Beginn). Findet ein späterer Lauf eine alte Marke eines anderen Prozesses für denselben
//! Schlüssel, hat dieser Aufbau das Programm beendet (Speicher, Stapel – das fängt
//! `catch_unwind` nicht): dann wird `"art":"fehlgeschlagen"` gespeichert und diese Version nie
//! wieder automatisch gebaut. Eine frische fremde Marke heißt: eine andere Instanz baut gerade.
//!
//! Werte von `art`: `bild`, `leer`, `zu_aufwendig`, `fehlgeschlagen` (Miniatur nicht möglich),
//! `baut` (nur mit den Zusatzschlüsseln `pid` und `since`; nie eine gültige Miniatur).

use crate::state::CHORD_TOLERANCE;
use crate::thumbnail::{
    Image, RENDER_COST_BUDGET, THUMB_H, THUMB_SCALE, THUMB_W, ThumbView, decode_png, encode_png,
    render, render_cost, thumb_colors,
};
use planora_feature::Document;
use planora_kernel::Solid;
use planora_mesh::{Mesh, TessellationOptions, tessellate};
use planora_project::docstore::{peek, read_version};
use planora_project::fsutil::{atomic_write_unique, now_iso};
use std::io::Read;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

pub const PREVIEW_PNG: &str = "vorschau.png";
pub const PREVIEW_JSON: &str = "vorschau.json";
pub const PREVIEW_FORMAT: u32 = 1;
pub const PROGRAM: &str = env!("CARGO_PKG_VERSION");
pub const MAX_TRIANGLES: usize = 200_000;
pub const MAX_BUILD: Duration = Duration::from_secs(3);
pub const THUMB_CHORD_MM: f64 = 0.1;
/// So lange gilt die Marke „baut“ einer anderen Instanz als laufender Aufbau; danach als Absturz.
pub const MARKER_FRESH: Duration = Duration::from_secs(15 * 60);
/// Eigene Zwischendateien (`vorschau.*.<Prozess>-<Zähler>.tmp`), die älter sind, werden beim
/// Speichern entfernt (Reste eines Absturzes).
pub const STALE_TMP: Duration = Duration::from_secs(60 * 60);
/// Kacheltext für eine Miniatur, deren Aufbau das Programm beendet hat.
pub const IMPOSSIBLE_TEXT: &str = "Miniatur nicht möglich";

/// Bildgröße der Datei in Pixeln.
const PREVIEW_W: u32 = THUMB_W * THUMB_SCALE;
const PREVIEW_H: u32 = THUMB_H * THUMB_SCALE;
/// Größte Länge von `vorschau.json`; längere Dateien gelten als ungültig.
const MAX_JSON_BYTES: u64 = 4096;
/// Größte Länge von `vorschau.png` (ein unkomprimiertes Bild ist kleiner als 1 MB).
const MAX_PNG_BYTES: u64 = 8 * 1024 * 1024;
/// Werte von `art` in `vorschau.json`.
const ART_IMAGE: &str = "bild";
const ART_EMPTY: &str = "leer";
const ART_TOO_COMPLEX: &str = "zu_aufwendig";
const ART_IMPOSSIBLE: &str = "fehlgeschlagen";
const ART_BUILDING: &str = "baut";

/// Wofür eine Miniatur gilt: Dokument (Kennung aus `dokument.json`), Version, Zweig,
/// Programmversion, Hell/Dunkel. Die Kennung steht nicht in `vorschau.json`; sie wird vor jedem
/// Schreiben gegen den Ordner geprüft.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ThumbKey {
    pub doc_id: String,
    pub version: u32,
    pub branch: String,
    pub program: String,
    pub dark: bool,
}

impl ThumbKey {
    /// Schlüssel für dieses Programm (`program` = [`PROGRAM`]).
    pub fn new(doc_id: &str, version: u32, branch: &str, dark: bool) -> ThumbKey {
        ThumbKey {
            doc_id: doc_id.to_string(),
            version,
            branch: branch.to_string(),
            program: PROGRAM.to_string(),
            dark,
        }
    }

    /// Schlüssel für die Spitze des aktiven Zweigs des Dokuments in `doc_dir`.
    pub fn current(doc_dir: &Path, dark: bool) -> Result<ThumbKey, String> {
        let (info, _) = peek(doc_dir).map_err(|e| e.to_string())?;
        let version = info
            .tip(&info.active_branch)
            .ok_or_else(|| "Der aktive Zweig hat keine Version.".to_string())?;
        Ok(ThumbKey::new(&info.id, version, &info.active_branch, dark))
    }
}

/// Ergebnis einer Miniatur.
#[derive(Clone, Debug)]
pub enum Thumb {
    /// Bild 496 × 280.
    Image(Arc<Image>),
    /// Das Dokument hat keine sichtbaren Körper.
    NoBodies,
    /// Über einer Grenze (Dreiecke, Zeit, Zeichenaufwand).
    TooComplex,
    /// Ein früherer Aufbau dieser Version hat das Programm beendet; gespeichert, nie neu gebaut
    /// (Kacheltext [`IMPOSSIBLE_TEXT`]).
    Impossible,
    /// Eine andere Instanz baut diese Miniatur gerade (nichts gebaut, nichts gespeichert).
    Busy,
    /// Lesen oder Aufbauen ist gescheitert (nur im Speicher, nie gespeichert).
    Failed(String),
}

/// Grenzen beim Aufbau (P8).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BuildLimits {
    /// Höchstens so viele Dreiecke über alle Körper.
    pub max_triangles: usize,
    /// Höchstens so lange (geprüft nach dem Aufbau des Dokuments und nach jedem Körper).
    pub max_time: Duration,
}

impl Default for BuildLimits {
    fn default() -> Self {
        BuildLimits {
            max_triangles: MAX_TRIANGLES,
            max_time: MAX_BUILD,
        }
    }
}

/// Stimmt der Datensatz zum Schlüssel (ohne Kennung, die nicht in der Datei steht)?
fn same_key(rec: &Record, key: &ThumbKey) -> bool {
    rec.v == PREVIEW_FORMAT
        && rec.version == key.version
        && rec.branch == key.branch
        && rec.program == key.program
        && rec.dark == key.dark
        && rec.w == PREVIEW_W
        && rec.h == PREVIEW_H
}

fn read_record(doc_dir: &Path) -> Option<Record> {
    let bytes = read_capped(&doc_dir.join(PREVIEW_JSON), MAX_JSON_BYTES)?;
    parse_record(std::str::from_utf8(&bytes).ok()?)
}

/// Gültige Zusatzdatei (P6) → Some; alles andere (fehlt, kaputt, anderer Schlüssel, Marke
/// „baut“) → None.
pub fn load_cached(doc_dir: &Path, key: &ThumbKey) -> Option<Thumb> {
    let rec = read_record(doc_dir)?;
    if !same_key(&rec, key) {
        return None;
    }
    match rec.art.as_str() {
        ART_IMAGE => {
            let png = read_capped(&doc_dir.join(PREVIEW_PNG), MAX_PNG_BYTES)?;
            let img = decode_png(&png, PREVIEW_W, PREVIEW_H).ok()?;
            Some(Thumb::Image(Arc::new(img)))
        }
        ART_EMPTY => Some(Thumb::NoBodies),
        ART_TOO_COMPLEX => Some(Thumb::TooComplex),
        ART_IMPOSSIBLE => Some(Thumb::Impossible),
        _ => None,
    }
}

/// Liest Version `key.version`, baut, vernetzt, zeichnet; nie eine Panik nach außen.
pub fn build(doc_dir: &Path, key: &ThumbKey, limits: &BuildLimits) -> Thumb {
    build_with(doc_dir, key, limits, &mut || {})
}

/// Wie [`build`]; `after_step` läuft nach dem Aufbau des Dokuments und nach jedem Körper, jeweils
/// direkt vor der Zeitprüfung (Prüfhaken für Tests: Verzögerung oder Panik).
pub fn build_with(
    doc_dir: &Path,
    key: &ThumbKey,
    limits: &BuildLimits,
    after_step: &mut dyn FnMut(),
) -> Thumb {
    let run = catch_unwind(AssertUnwindSafe(|| {
        build_unguarded(doc_dir, key, limits, after_step)
    }));
    run.unwrap_or_else(|_| {
        Thumb::Failed("Beim Erstellen der Vorschau ist ein Fehler aufgetreten.".into())
    })
}

/// Zählt Dreiecke gegen die Grenze und prüft die Zeit.
struct Gate<'a> {
    triangles: usize,
    max_triangles: usize,
    over_time: &'a mut dyn FnMut() -> bool,
}

impl Gate<'_> {
    /// false, wenn mit `n` weiteren Dreiecken die Grenze überschritten ist.
    fn add(&mut self, n: usize) -> bool {
        self.triangles = self.triangles.saturating_add(n);
        self.triangles <= self.max_triangles
    }
}

/// Vernetzt die Körper mit `mesh_of`; ein Körper, dessen Netz scheitert, fehlt nur selbst.
/// None, wenn eine Grenze überschritten ist.
fn mesh_solids(
    solids: &[&Solid],
    mesh_of: &dyn Fn(&Solid) -> Option<Mesh>,
    gate: &mut Gate,
    out: &mut Vec<Mesh>,
) -> Option<()> {
    for solid in solids {
        let Some(mesh) = mesh_of(solid) else {
            continue;
        };
        if !gate.add(mesh.indices.len() / 3) {
            return None;
        }
        out.push(mesh);
        if (gate.over_time)() {
            return None;
        }
    }
    Some(())
}

fn build_unguarded(
    doc_dir: &Path,
    key: &ThumbKey,
    limits: &BuildLimits,
    after_step: &mut dyn FnMut(),
) -> Thumb {
    let started = Instant::now();
    let snapshot = match read_version(doc_dir, key.version) {
        Ok(s) => s,
        Err(e) => return failed(e.to_string()),
    };
    let doc = match Document::from_snapshot(snapshot, CHORD_TOLERANCE) {
        Ok(d) => d,
        Err(e) => return failed(e.to_string()),
    };
    let mut over_time = || {
        after_step();
        started.elapsed() > limits.max_time
    };
    if over_time() {
        return Thumb::TooComplex;
    }
    let options = TessellationOptions {
        chord_tolerance: THUMB_CHORD_MM,
    };
    let mut gate = Gate {
        triangles: 0,
        max_triangles: limits.max_triangles,
        over_time: &mut over_time,
    };
    let mut meshes: Vec<Mesh> = Vec::new();
    let solids: Vec<&Solid> = doc.visible_bodies().into_iter().map(|v| v.solid).collect();
    let mesh_of = |s: &Solid| tessellate(s, &options).ok();
    if mesh_solids(&solids, &mesh_of, &mut gate, &mut meshes).is_none() {
        return Thumb::TooComplex;
    }
    for v in doc.visible_meshes() {
        // Vor dem Platzieren zählen: ein riesiges Netz wird gar nicht erst kopiert.
        if !gate.add(v.mesh.triangle_count()) {
            return Thumb::TooComplex;
        }
        meshes.push(v.mesh.placed(&v.transform));
        if (gate.over_time)() {
            return Thumb::TooComplex;
        }
    }
    if meshes.is_empty() {
        return Thumb::NoBodies;
    }
    let refs: Vec<&Mesh> = meshes.iter().collect();
    let view = ThumbView::start();
    if render_cost(&refs, view, PREVIEW_W, PREVIEW_H) > RENDER_COST_BUDGET {
        return Thumb::TooComplex;
    }
    let img = render(&refs, view, &thumb_colors(key.dark), PREVIEW_W, PREVIEW_H);
    Thumb::Image(Arc::new(img))
}

fn failed(reason: String) -> Thumb {
    Thumb::Failed(format!("Die Vorschau ließ sich nicht erstellen: {reason}"))
}

/// Gehört der Ordner noch dem Dokument des Schlüssels, und ist die Version noch die Spitze
/// ihres Zweigs?
fn still_current(doc_dir: &Path, key: &ThumbKey) -> bool {
    match peek(doc_dir) {
        Ok((info, _)) => info.id == key.doc_id && info.tip(&key.branch) == Some(key.version),
        Err(_) => false,
    }
}

/// Entfernt eine Datei; fehlt sie, ist das kein Fehler.
fn remove_if_present(path: &Path) -> Result<(), String> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!(
            "„{}“ lässt sich nicht entfernen: {e}",
            path.display()
        )),
    }
}

/// PNG (nur bei Image), dann JSON; `Failed` und `Busy` werden nie gespeichert (Err). Vorher:
/// alte eigene Zwischendateien entfernen, prüfen, dass Ordner und Version noch zum Schlüssel
/// passen, und die alte `vorschau.json` löschen – lässt sie sich nicht löschen, wird auch kein
/// PNG geschrieben (sonst stünde eine alte JSON neben einem neuen Bild).
pub fn store(doc_dir: &Path, key: &ThumbKey, thumb: &Thumb) -> Result<(), String> {
    let (art, png) = match thumb {
        Thumb::Image(img) => {
            if (img.width, img.height) != (PREVIEW_W, PREVIEW_H) {
                return Err(format!(
                    "Die Vorschau hat {} × {} statt {PREVIEW_W} × {PREVIEW_H} Pixel.",
                    img.width, img.height
                ));
            }
            (ART_IMAGE, Some(encode_png(img)?))
        }
        Thumb::NoBodies => (ART_EMPTY, None),
        Thumb::TooComplex => (ART_TOO_COMPLEX, None),
        Thumb::Impossible => (ART_IMPOSSIBLE, None),
        Thumb::Busy | Thumb::Failed(_) => {
            return Err("Diese Vorschau wird nicht gespeichert.".into());
        }
    };
    remove_stale_tmp(doc_dir);
    if !still_current(doc_dir, key) {
        return Err("Das Dokument hat sich inzwischen geändert.".into());
    }
    remove_if_present(&doc_dir.join(PREVIEW_JSON))?;
    let json = record_json(key, art);
    if let Some(bytes) = &png {
        write_preview(&doc_dir.join(PREVIEW_PNG), bytes)?;
    }
    // Zuletzt die JSON: sie ist das Zeichen, dass alles fertig geschrieben ist.
    write_preview(&doc_dir.join(PREVIEW_JSON), json.as_bytes())
}

fn write_preview(path: &Path, bytes: &[u8]) -> Result<(), String> {
    atomic_write_unique(path, bytes).map_err(|e| e.to_string())
}

/// Entfernt eigene Zwischendateien `vorschau.png.<Zahl>-<Zahl>.tmp` bzw.
/// `vorschau.json.<Zahl>-<Zahl>.tmp`, die älter als [`STALE_TMP`] sind. Sonst nichts.
fn remove_stale_tmp(doc_dir: &Path) {
    let Ok(rd) = std::fs::read_dir(doc_dir) else {
        return;
    };
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if !is_own_tmp(&name) {
            continue;
        }
        let Ok(meta) = e.metadata() else { continue };
        let old = meta
            .modified()
            .ok()
            .and_then(|t| t.elapsed().ok())
            .is_some_and(|age| age > STALE_TMP);
        if meta.is_file() && old {
            let _ = std::fs::remove_file(e.path());
        }
    }
}

/// Name einer Zwischendatei dieses Moduls (`atomic_write_unique`)?
fn is_own_tmp(name: &str) -> bool {
    let rest = [PREVIEW_PNG, PREVIEW_JSON]
        .iter()
        .find_map(|p| name.strip_prefix(p)?.strip_prefix('.'));
    let Some(middle) = rest.and_then(|r| r.strip_suffix(".tmp")) else {
        return false;
    };
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    middle
        .split_once('-')
        .is_some_and(|(pid, n)| digits(pid) && digits(n))
}

/// Ergebnis von [`thumbnail_for`]: `stored` = die Miniatur liegt als Zusatzdatei vor (aus der
/// Datei gelesen oder eben gespeichert), `from_cache` = aus der Datei gelesen, nicht gebaut.
#[derive(Clone, Debug)]
pub struct ThumbOutcome {
    pub thumb: Thumb,
    pub stored: bool,
    pub from_cache: bool,
}

/// `load_cached` oder `build` + `store`, mit Absturzschutz (siehe Moduldokumentation).
pub fn thumbnail_for(doc_dir: &Path, key: &ThumbKey, limits: &BuildLimits) -> ThumbOutcome {
    thumbnail_for_with(doc_dir, key, limits, &mut || {})
}

/// Wie [`thumbnail_for`]; `after_step` wie bei [`build_with`].
pub fn thumbnail_for_with(
    doc_dir: &Path,
    key: &ThumbKey,
    limits: &BuildLimits,
    after_step: &mut dyn FnMut(),
) -> ThumbOutcome {
    let outcome = |thumb, stored, from_cache| ThumbOutcome {
        thumb,
        stored,
        from_cache,
    };
    if let Some(thumb) = load_cached(doc_dir, key) {
        return outcome(thumb, true, true);
    }
    if !still_current(doc_dir, key) {
        return outcome(
            failed("Das Dokument hat sich geändert.".into()),
            false,
            false,
        );
    }
    match marker_state(doc_dir, key) {
        Marker::Fresh => return outcome(Thumb::Busy, false, false),
        Marker::Stale => {
            let stored = store(doc_dir, key, &Thumb::Impossible).is_ok();
            return outcome(Thumb::Impossible, stored, false);
        }
        Marker::None => {}
    }
    let marked = begin_build(doc_dir, key);
    let thumb = build_with(doc_dir, key, limits, after_step);
    let stored = store(doc_dir, key, &thumb).is_ok();
    if !stored && marked {
        clear_own_marker(doc_dir, key);
    }
    outcome(thumb, stored, false)
}

/// Lage einer Marke „baut“ für den Schlüssel.
enum Marker {
    /// Keine (oder für einen anderen Schlüssel, oder eine eigene Altlast dieses Prozesses).
    None,
    /// Eine andere Instanz baut gerade.
    Fresh,
    /// Ein früherer Aufbau hat sein Programm nicht überlebt.
    Stale,
}

fn marker_state(doc_dir: &Path, key: &ThumbKey) -> Marker {
    let Some(rec) = read_record(doc_dir) else {
        return Marker::None;
    };
    if rec.art != ART_BUILDING || !same_key(&rec, key) {
        return Marker::None;
    }
    let (Some(pid), Some(since)) = (rec.pid, rec.since.as_deref()) else {
        return Marker::None;
    };
    if pid == std::process::id() {
        // Dieser Prozess lebt: Rest eines eigenen, nicht abgestürzten Laufs.
        return Marker::None;
    }
    let fresh = iso_seconds(since)
        .zip(unix_now())
        .is_some_and(|(t, now)| (now - t).abs() <= MARKER_FRESH.as_secs() as i64);
    if fresh { Marker::Fresh } else { Marker::Stale }
}

/// Löscht alte Zusatzdateien (erst JSON, dann PNG) und schreibt die Marke „baut“. false, wenn
/// die Marke nicht steht (dann bleibt eine nicht löschbare alte JSON samt PNG unberührt).
fn begin_build(doc_dir: &Path, key: &ThumbKey) -> bool {
    if remove_if_present(&doc_dir.join(PREVIEW_JSON)).is_err() {
        return false;
    }
    let _ = remove_if_present(&doc_dir.join(PREVIEW_PNG));
    let text = marker_json(key, std::process::id(), &now_iso());
    write_preview(&doc_dir.join(PREVIEW_JSON), text.as_bytes()).is_ok()
}

/// Entfernt die eigene Marke nach einem nicht gespeicherten Ergebnis (Fehlschlag im Speicher).
fn clear_own_marker(doc_dir: &Path, key: &ThumbKey) {
    let own = read_record(doc_dir).is_some_and(|r| {
        r.art == ART_BUILDING && same_key(&r, key) && r.pid == Some(std::process::id())
    });
    if own {
        let _ = std::fs::remove_file(doc_dir.join(PREVIEW_JSON));
    }
}

/// Sekunden seit 1970 für „JJJJ-MM-TTThh:mm:ssZ“.
fn iso_seconds(t: &str) -> Option<i64> {
    let b = t.as_bytes();
    let form_ok = b.len() == 20
        && b.iter().enumerate().all(|(i, c)| match i {
            4 | 7 => *c == b'-',
            10 => *c == b'T',
            13 | 16 => *c == b':',
            19 => *c == b'Z',
            _ => c.is_ascii_digit(),
        });
    if !form_ok {
        return None;
    }
    let num = |a: usize, z: usize| t[a..z].parse::<i64>().ok();
    let (y, m, d) = (num(0, 4)?, num(5, 7)?, num(8, 10)?);
    let (hh, mm, ss) = (num(11, 13)?, num(14, 16)?, num(17, 19)?);
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) || hh > 23 || mm > 59 || ss > 60 {
        return None;
    }
    // Tage seit 1970-01-01 (Gregorianischer Kalender, Verfahren nach H. Hinnant).
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Some(days * 86_400 + hh * 3600 + mm * 60 + ss)
}

fn unix_now() -> Option<i64> {
    let d = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .ok()?;
    i64::try_from(d.as_secs()).ok()
}

/// Liest eine Datei, wenn sie eine Datei ist und höchstens `cap` Bytes hat.
fn read_capped(path: &Path, cap: u64) -> Option<Vec<u8>> {
    let file = std::fs::File::open(path).ok()?;
    if !file.metadata().ok()?.is_file() {
        return None;
    }
    let mut out = Vec::new();
    file.take(cap + 1).read_to_end(&mut out).ok()?;
    (out.len() as u64 <= cap).then_some(out)
}

// ---- vorschau.json --------------------------------------------------------------------------

/// Inhalt von `vorschau.json`.
struct Record {
    v: u32,
    version: u32,
    branch: String,
    program: String,
    dark: bool,
    w: u32,
    h: u32,
    art: String,
    /// Nur bei `baut`: Prozess und Beginn (UTC, „JJJJ-MM-TTThh:mm:ssZ“).
    pid: Option<u32>,
    since: Option<String>,
}

/// Text von `vorschau.json` für den Schlüssel und die Art (eine Zeile, Schlüssel in der
/// Reihenfolge von P6).
pub fn record_json(key: &ThumbKey, art: &str) -> String {
    format!(
        "{{\"v\":{PREVIEW_FORMAT},\"version\":{},\"branch\":{},\"program\":{},\"dark\":{},\"w\":{PREVIEW_W},\"h\":{PREVIEW_H},\"art\":{}}}",
        key.version,
        json_string(&key.branch),
        json_string(&key.program),
        key.dark,
        json_string(art)
    )
}

/// Marke „baut“: wie [`record_json`] mit `pid` und `since` dahinter.
pub fn marker_json(key: &ThumbKey, pid: u32, since: &str) -> String {
    let base = record_json(key, ART_BUILDING);
    let head = base.strip_suffix('}').unwrap_or(&base);
    format!("{head},\"pid\":{pid},\"since\":{}}}", json_string(since))
}

/// Text als JSON-Zeichenkette: `"` und `\` maskiert, Steuerzeichen als `\uXXXX`.
fn json_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if c.is_control() => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

enum Value {
    Number(u32),
    Text(String),
    Bool(bool),
}

/// Strenger Leser: ein Objekt mit genau den acht Schlüsseln (je einmal, Reihenfolge beliebig),
/// bei `"art":"baut"` zusätzlich genau `pid` und `since` (gültige Zeitangabe), sonst nie;
/// Zahlen ganzzahlig ohne Vorzeichen und ohne führende Null, `true`/`false`, sonst nichts.
fn parse_record(text: &str) -> Option<Record> {
    let mut r = Reader {
        chars: text.chars().peekable(),
    };
    let (mut v, mut version, mut w, mut h, mut pid) = (None, None, None, None, None);
    let (mut branch, mut program, mut art, mut dark) = (None, None, None, None);
    let mut since = None;
    r.expect('{')?;
    loop {
        let name = r.string()?;
        r.expect(':')?;
        let value = r.value()?;
        let fresh = match (name.as_str(), value) {
            ("v", Value::Number(n)) => v.replace(n).is_none(),
            ("version", Value::Number(n)) => version.replace(n).is_none(),
            ("w", Value::Number(n)) => w.replace(n).is_none(),
            ("h", Value::Number(n)) => h.replace(n).is_none(),
            ("pid", Value::Number(n)) => pid.replace(n).is_none(),
            ("branch", Value::Text(t)) => branch.replace(t).is_none(),
            ("program", Value::Text(t)) => program.replace(t).is_none(),
            ("art", Value::Text(t)) => art.replace(t).is_none(),
            ("since", Value::Text(t)) => since.replace(t).is_none(),
            ("dark", Value::Bool(b)) => dark.replace(b).is_none(),
            _ => return None,
        };
        if !fresh {
            return None;
        }
        r.skip_space();
        match r.chars.next()? {
            ',' => continue,
            '}' => break,
            _ => return None,
        }
    }
    r.skip_space();
    if r.chars.next().is_some() {
        return None;
    }
    let art: String = art?;
    let building = art == ART_BUILDING;
    let extras_ok = if building {
        pid.is_some() && since.as_deref().and_then(iso_seconds).is_some()
    } else {
        pid.is_none() && since.is_none()
    };
    if !extras_ok {
        return None;
    }
    Some(Record {
        v: v?,
        version: version?,
        branch: branch?,
        program: program?,
        dark: dark?,
        w: w?,
        h: h?,
        art,
        pid,
        since,
    })
}

struct Reader<'a> {
    chars: std::iter::Peekable<std::str::Chars<'a>>,
}

impl Reader<'_> {
    fn skip_space(&mut self) {
        while matches!(self.chars.peek(), Some(' ' | '\t' | '\n' | '\r')) {
            self.chars.next();
        }
    }

    fn expect(&mut self, c: char) -> Option<()> {
        self.skip_space();
        (self.chars.next()? == c).then_some(())
    }

    fn value(&mut self) -> Option<Value> {
        self.skip_space();
        match *self.chars.peek()? {
            '"' => self.string().map(Value::Text),
            't' => self.word("true").map(|()| Value::Bool(true)),
            'f' => self.word("false").map(|()| Value::Bool(false)),
            '0'..='9' => self.number().map(Value::Number),
            _ => None,
        }
    }

    fn word(&mut self, w: &str) -> Option<()> {
        for c in w.chars() {
            (self.chars.next()? == c).then_some(())?;
        }
        Some(())
    }

    fn number(&mut self) -> Option<u32> {
        let mut n: u32 = 0;
        let mut digits = 0;
        while let Some(d) = self.chars.peek().and_then(|c| c.to_digit(10)) {
            if digits == 1 && n == 0 {
                return None; // führende Null
            }
            self.chars.next();
            n = n.checked_mul(10)?.checked_add(d)?;
            digits += 1;
        }
        (digits > 0).then_some(n)
    }

    fn hex4(&mut self) -> Option<u32> {
        let mut n = 0;
        for _ in 0..4 {
            n = n * 16 + self.chars.next()?.to_digit(16)?;
        }
        Some(n)
    }

    fn string(&mut self) -> Option<String> {
        self.expect('"')?;
        let mut out = String::new();
        loop {
            match self.chars.next()? {
                '"' => return Some(out),
                '\\' => {
                    let c = match self.chars.next()? {
                        '"' => '"',
                        '\\' => '\\',
                        '/' => '/',
                        'b' => '\u{8}',
                        'f' => '\u{c}',
                        'n' => '\n',
                        'r' => '\r',
                        't' => '\t',
                        'u' => self.escaped_char()?,
                        _ => return None,
                    };
                    out.push(c);
                }
                c if (c as u32) < 0x20 => return None,
                c => out.push(c),
            }
        }
    }

    /// Zeichen nach `\u` (auch als Ersatzpaar `😀`).
    fn escaped_char(&mut self) -> Option<char> {
        let hi = self.hex4()?;
        if (0xD800..0xDC00).contains(&hi) {
            self.word("\\u")?;
            let lo = self.hex4()?;
            if !(0xDC00..0xE000).contains(&lo) {
                return None;
            }
            char::from_u32(0x10000 + ((hi - 0xD800) << 10) + (lo - 0xDC00))
        } else {
            char::from_u32(hi)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use planora_kernel::{AnalyticKernel, Kernel};
    use planora_math::DVec3;

    fn mesh_with(n: usize) -> Mesh {
        let mut m = tessellate(
            &AnalyticKernel::default()
                .make_box(DVec3::splat(1.0))
                .unwrap(),
            &TessellationOptions {
                chord_tolerance: THUMB_CHORD_MM,
            },
        )
        .unwrap();
        m.indices = (0..n * 3).map(|i| (i % 3) as u32).collect();
        m
    }

    #[test]
    fn a_body_whose_mesh_fails_is_left_out_and_the_others_stay() {
        let k = AnalyticKernel::default();
        let a = k.make_box(DVec3::splat(1.0)).unwrap();
        let b = k.make_box(DVec3::splat(2.0)).unwrap();
        let c = k.make_box(DVec3::splat(3.0)).unwrap();
        let solids = [&a, &b, &c];
        // Der zweite Körper lässt sich nicht vernetzen.
        let mesh_of = |s: &Solid| (!std::ptr::eq(s, &b)).then(|| mesh_with(2));
        let mut steps = 0;
        let mut over = || {
            steps += 1;
            false
        };
        let mut gate = Gate {
            triangles: 0,
            max_triangles: 100,
            over_time: &mut over,
        };
        let mut out = Vec::new();
        assert!(mesh_solids(&solids, &mesh_of, &mut gate, &mut out).is_some());
        assert_eq!(out.len(), 2, "nur der fehlerhafte Körper fehlt");
        assert_eq!(gate.triangles, 4);
        assert_eq!(steps, 2, "Zeit nach jedem gezeichneten Körper geprüft");
    }

    #[test]
    fn the_triangle_limit_counts_exactly() {
        let k = AnalyticKernel::default();
        let a = k.make_box(DVec3::splat(1.0)).unwrap();
        let solids = [&a, &a];
        let mesh_of = |_: &Solid| Some(mesh_with(5));
        for (max, ok) in [(10, true), (9, false)] {
            let mut over = || false;
            let mut gate = Gate {
                triangles: 0,
                max_triangles: max,
                over_time: &mut over,
            };
            let mut out = Vec::new();
            let r = mesh_solids(&solids, &mesh_of, &mut gate, &mut out);
            assert_eq!(r.is_some(), ok, "Grenze {max}");
        }
    }

    #[test]
    fn iso_times_convert_to_unix_seconds() {
        assert_eq!(iso_seconds("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(iso_seconds("2000-03-01T00:00:00Z"), Some(951_868_800));
        assert_eq!(iso_seconds("2026-10-24T12:34:56Z"), Some(1_792_845_296));
        for bad in [
            "2026-10-24 12:34:56Z",
            "2026-13-01T00:00:00Z",
            "2026-10-24T24:00:00Z",
            "2026-10-24T12:34:56",
            "",
        ] {
            assert_eq!(iso_seconds(bad), None, "{bad}");
        }
    }

    #[test]
    fn own_temporary_names_are_recognised() {
        for ok in ["vorschau.png.123-4.tmp", "vorschau.json.9-0.tmp"] {
            assert!(is_own_tmp(ok), "{ok}");
        }
        for no in [
            "vorschau.png.tmp",
            "vorschau.json.tmp",
            "vorschau.png.abc-1.tmp",
            "vorschau.png.1-.tmp",
            "vorschau.png.-1.tmp",
            "vorschau.png.1-2",
            "dokument.json.1-2.tmp",
            "vorschau.pngx.1-2.tmp",
        ] {
            assert!(!is_own_tmp(no), "{no}");
        }
    }
}
