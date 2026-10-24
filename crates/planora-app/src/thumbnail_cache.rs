//! Miniatur eines Dokuments und ihr Zwischenspeicher im Dokumentordner (Plan P6, P8):
//! `vorschau.png` (496 × 280 RGBA) und `vorschau.json` (eine Zeile, die „fertig“-Marke).
//!
//! Gebaut wird immer aus einer gespeicherten Version (nie aus einem offenen, geänderten Tab):
//! lesen, `Document::from_snapshot`, sichtbare Körper vernetzen, zeichnen. Grenzen: Summe der
//! Dreiecke, Bauzeit und geschätzter Zeichenaufwand; darüber gilt die Miniatur als „zu
//! aufwendig“ und wird so gespeichert. Lese- und Aufbaufehler, auch eine Panik, ergeben
//! `Failed`, das nie gespeichert wird. Geschrieben werden nur `vorschau.png` und `vorschau.json`
//! (über eigene Zwischendateien je Schreibvorgang), sonst nichts im Dokumentordner.

use crate::state::CHORD_TOLERANCE;
use crate::thumbnail::{
    Image, RENDER_COST_BUDGET, THUMB_H, THUMB_SCALE, THUMB_W, ThumbView, decode_png, encode_png,
    render, render_cost, thumb_colors,
};
use planora_feature::Document;
use planora_kernel::Solid;
use planora_mesh::{Mesh, TessellationOptions, tessellate};
use planora_project::docstore::read_version;
use planora_project::fsutil::atomic_write_unique;
use std::io::Read;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

pub const PREVIEW_PNG: &str = "vorschau.png";
pub const PREVIEW_JSON: &str = "vorschau.json";
pub const PREVIEW_FORMAT: u32 = 1;
pub const PROGRAM: &str = env!("CARGO_PKG_VERSION");
pub const MAX_TRIANGLES: usize = 200_000;
pub const MAX_BUILD: Duration = Duration::from_secs(3);
pub const THUMB_CHORD_MM: f64 = 0.1;

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

/// Wofür eine Miniatur gilt: Version, Zweig, Programmversion, Hell/Dunkel.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ThumbKey {
    pub version: u32,
    pub branch: String,
    pub program: String,
    pub dark: bool,
}

impl ThumbKey {
    /// Schlüssel für dieses Programm (`program` = [`PROGRAM`]).
    pub fn new(version: u32, branch: &str, dark: bool) -> ThumbKey {
        ThumbKey {
            version,
            branch: branch.to_string(),
            program: PROGRAM.to_string(),
            dark,
        }
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

/// Gültige Zusatzdatei (P6) → Some; alles andere (fehlt, kaputt, anderer Schlüssel) → None.
pub fn load_cached(doc_dir: &Path, key: &ThumbKey) -> Option<Thumb> {
    let bytes = read_capped(&doc_dir.join(PREVIEW_JSON), MAX_JSON_BYTES)?;
    let rec = parse_record(std::str::from_utf8(&bytes).ok()?)?;
    let valid = rec.v == PREVIEW_FORMAT
        && rec.version == key.version
        && rec.branch == key.branch
        && rec.program == key.program
        && rec.dark == key.dark
        && rec.w == PREVIEW_W
        && rec.h == PREVIEW_H;
    if !valid {
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
    let mut meshes: Vec<Mesh> = Vec::new();
    let mut triangles: usize = 0;
    let solids: Vec<&Solid> = doc.visible_bodies().into_iter().map(|v| v.solid).collect();
    for solid in solids {
        // Ein Körper, der sich nicht vernetzen lässt, fehlt nur selbst.
        let Ok(mesh) = tessellate(solid, &options) else {
            continue;
        };
        triangles = triangles.saturating_add(mesh.indices.len() / 3);
        if triangles > limits.max_triangles {
            return Thumb::TooComplex;
        }
        meshes.push(mesh);
        if over_time() {
            return Thumb::TooComplex;
        }
    }
    for v in doc.visible_meshes() {
        // Vor dem Platzieren zählen: ein riesiges Netz wird gar nicht erst kopiert.
        triangles = triangles.saturating_add(v.mesh.triangle_count());
        if triangles > limits.max_triangles {
            return Thumb::TooComplex;
        }
        meshes.push(v.mesh.placed(&v.transform));
        if over_time() {
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

/// PNG (nur bei Image), dann JSON; `Failed` wird nie gespeichert (Err).
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
        Thumb::Failed(_) => {
            return Err("Eine fehlgeschlagene Vorschau wird nicht gespeichert.".into());
        }
    };
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

/// Ergebnis von [`thumbnail_for`]: `stored` = die Miniatur liegt als Zusatzdatei vor (aus der
/// Datei gelesen oder eben gespeichert), `from_cache` = aus der Datei gelesen, nicht gebaut.
#[derive(Clone, Debug)]
pub struct ThumbOutcome {
    pub thumb: Thumb,
    pub stored: bool,
    pub from_cache: bool,
}

/// `load_cached` oder `build` + `store`.
pub fn thumbnail_for(doc_dir: &Path, key: &ThumbKey, limits: &BuildLimits) -> ThumbOutcome {
    if let Some(thumb) = load_cached(doc_dir, key) {
        return ThumbOutcome {
            thumb,
            stored: true,
            from_cache: true,
        };
    }
    let thumb = build(doc_dir, key, limits);
    let stored = store(doc_dir, key, &thumb).is_ok();
    ThumbOutcome {
        thumb,
        stored,
        from_cache: false,
    }
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
}

/// Eine Zeile JSON, Schlüssel in der Reihenfolge von P6.
fn record_json(key: &ThumbKey, art: &str) -> String {
    format!(
        "{{\"v\":{PREVIEW_FORMAT},\"version\":{},\"branch\":{},\"program\":{},\"dark\":{},\"w\":{PREVIEW_W},\"h\":{PREVIEW_H},\"art\":{}}}",
        key.version,
        json_string(&key.branch),
        json_string(&key.program),
        key.dark,
        json_string(art)
    )
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
/// Zahlen ganzzahlig ohne Vorzeichen und ohne führende Null, `true`/`false`, sonst nichts.
fn parse_record(text: &str) -> Option<Record> {
    let mut r = Reader {
        chars: text.chars().peekable(),
    };
    let (mut v, mut version, mut w, mut h) = (None, None, None, None);
    let (mut branch, mut program, mut art, mut dark) = (None, None, None, None);
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
            ("branch", Value::Text(t)) => branch.replace(t).is_none(),
            ("program", Value::Text(t)) => program.replace(t).is_none(),
            ("art", Value::Text(t)) => art.replace(t).is_none(),
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
    Some(Record {
        v: v?,
        version: version?,
        branch: branch?,
        program: program?,
        dark: dark?,
        w: w?,
        h: h?,
        art: art?,
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
