//! DXF-Schreiber für das Blattmodell (ASCII-DXF, Version AC1015 = AutoCAD 2000/R2000).
//!
//! Blattkoordinaten sind Millimeter, y nach oben, Ursprung unten links; das ist auch der
//! Modellraum einer DXF-Zeichnung, es gibt keine Umrechnung.
//!
//! # Aufbau (nach der offenen DXF-Referenz für R2000)
//!
//! * Zeilenende `\r\n`, jedes Paar aus Gruppencode (rechtsbündig in Breite 3) und Wert; die Datei
//!   enthält nur ASCII (Zeichen darüber hinaus stehen als `\U+XXXX`, siehe Namen).
//! * Abschnitte in der Reihenfolge `HEADER`, `CLASSES` (zwei Klassen der Plotstil-Kette), `TABLES`, `BLOCKS`, `ENTITIES`,
//!   `OBJECTS` (Wurzelverzeichnis mit `ACAD_GROUP` und `ACAD_PLOTSTYLENAME`), Ende `0 / EOF`.
//! * `HEADER`: `$ACADVER = AC1015`, `$DWGCODEPAGE`, `$HANDSEED` (größer als alle Handles),
//!   `$INSBASE`, `$EXTMIN`/`$EXTMAX` (Blatt), `$LIMMIN`/`$LIMMAX`, `$LTSCALE = 1`,
//!   `$MEASUREMENT = 1` (metrisch), `$INSUNITS = 4` (Millimeter).
//! * `TABLES` in der Reihenfolge `VPORT, LTYPE, LAYER, STYLE, VIEW, UCS, APPID, DIMSTYLE,
//!   BLOCK_RECORD`. Jede Tabelle trägt Handle (5), Besitzer `330 0`, `100 AcDbSymbolTable` und den
//!   Zähler (70); jeder Eintrag Handle (5, bei `DIMSTYLE` 105), Besitzer (330 = Tabelle),
//!   `100 AcDbSymbolTableRecord` und die Untertypmarke (`AcDbViewportTableRecord`,
//!   `AcDbLinetypeTableRecord`, `AcDbLayerTableRecord`, `AcDbTextStyleTableRecord`,
//!   `AcDbRegAppTableRecord`, `AcDbDimStyleTableRecord`, `AcDbBlockTableRecord`).
//!   - `VPORT`: `*ACTIVE` (Ansicht zeigt das ganze Blatt).
//!   - `LTYPE`: `ByBlock`, `ByLayer`, `CONTINUOUS`, `HIDDEN` (Strich 3 mm, Lücke 1 mm: 73 = 2,
//!     40 = 4, 49 = 3 / −1, 72 = 65).
//!   - `LAYER`: `0`, `SICHTBAR` (Farbe 7, `CONTINUOUS`, Linienstärke 50 = 0,5 mm), `VERDECKT`
//!     (Farbe 7, `HIDDEN`, Linienstärke 25 = 0,25 mm). Objekte tragen weder eigene Farbe noch
//!     eigene Linienart (von der Ebene bestimmt).
//!   - `STYLE`: `STANDARD`; `VIEW`, `UCS`: leer; `APPID`: `ACAD`; `DIMSTYLE`: `Standard`.
//!   - `BLOCK_RECORD`: `*Model_Space`, `*Paper_Space` und ein Eintrag je Ansicht.
//! * `BLOCKS`: zuerst die Pflichtblöcke `*Model_Space` und `*Paper_Space` (letzterer mit `67 1`),
//!   dann je Ansicht ein Block. `BLOCK` und `ENDBLK` tragen Handle, Besitzer (der Blockeintrag),
//!   `100 AcDbEntity`, Ebene `0`, `100 AcDbBlockBegin` bzw. `AcDbBlockEnd`.
//! * `ENTITIES`: Innenrahmen als geschlossene `LWPOLYLINE` auf `SICHTBAR`, danach je Ansicht ein
//!   `INSERT` bei (0, 0, 0) (Ebene `0`). Jedes Objekt: Handle (5), Besitzer (330 = Blockeintrag
//!   des Blocks, in dem es steht; im Modellraum `*Model_Space`), `100 AcDbEntity`, Ebene (8),
//!   Untertypmarke (`AcDbLine`, `AcDbCircle`, `AcDbCircle` + `AcDbArc`, `AcDbEllipse`,
//!   `AcDbSpline`, `AcDbPolyline`, `AcDbBlockReference`).
//!
//! # Objekte
//!
//! * `Line` → `LINE`. `Arc` → `ARC` (Mittelpunkt, Radius, Winkel in **Grad** gegen den
//!   Uhrzeigersinn, Anfang und Ende je in `[0, 360)`; läuft der Bogen über 0° hinweg, ist
//!   Anfang größer als Ende: so schreibt DXF den Umlauf) oder bei vollem Umlauf `CIRCLE`.
//!   Ein negativer Radius entspricht dem um 180° gedrehten Bogen mit dem Betrag.
//! * `Ellipse` → `ELLIPSE`: Mittelpunkt, Hauptachsenendpunkt relativ zur Mitte (11/21/31),
//!   Normale `0, 0, 1` (210/220/230), Verhältnis Nebenachse/Hauptachse (40, höchstens 1),
//!   Anfangs- und Endparameter (41/42) in Radiant. Der Parameter der DXF-Ellipse ist wie der von
//!   `Out2d` derselbe Parameter `t` in `c + major·cos t + minor·sin t` (nicht der Polarwinkel),
//!   gegen den Uhrzeigersinn, `minor = ratio · (−major.y, major.x)`. Es ist also keine
//!   Umrechnung nötig, nur das Hereinholen nach `[0, 2π]` (voller Umlauf `0 … 2π`); ein Verhältnis
//!   über 1 oder unter 0 (von Hand gebaut) wird in die zulässige Form umgerechnet.
//! * `Spline` → `SPLINE`: Flags 8 (eben), +4 bei rational (70), Grad (71), Zahl der Knoten (72),
//!   der Kontrollpunkte (73), der Anpassungspunkte 0 (74), Toleranzen (42/43), Knoten (40),
//!   Gewichte (41, nur rational), Kontrollpunkte (10/20/30).
//!
//! # Zahlen und Namen
//!
//! Zahlen mit höchstens neun Nachkommastellen, ohne Exponent und ohne nachgestellte Nullen,
//! `-0` wird `0`; nicht endliche Zahlen sind ein Fehler (`NonFinite`, es wird nichts
//! geschrieben). Blocknamen: die Zeichen `< > / \ " : ; ? * | =`, der Gravis und das Komma sowie
//! Steuerzeichen werden zu `_`, Zeichen außerhalb der Grundebene des Unicodes ebenfalls; die
//! übrigen Zeichen außerhalb von ASCII stehen als `\U+XXXX` in der Datei; höchstens 255 Zeichen; doppelte Namen (ohne Rücksicht auf
//! Groß-/Kleinschreibung, wie in DXF) bekommen ` 2`, ` 3`, …; leere Namen heißen `Ansicht`.

use crate::Visibility;
use crate::error::DrawError;
use crate::out2d::Out2d;
use crate::sheet::SheetModel;
use planora_math::{DVec2, PI, TAU};
use std::collections::BTreeSet;
use std::io::Write;

const LAYER_VISIBLE: &str = "SICHTBAR";
const LAYER_HIDDEN: &str = "VERDECKT";
/// Nachkommastellen aller Zahlen der Datei.
const DECIMALS: usize = 9;
/// Höchstlänge eines Namens in der Datei (Zeichen, nach der Verschlüsselung).
const MAX_NAME_LEN: usize = 255;
/// Kleinstes Verhältnis einer Ellipse (DXF-Referenz: 0,000001).
const MIN_RATIO: f64 = 1e-6;

/// Schreibt das Blattmodell als DXF. Bei einem Fehler wird nichts geschrieben.
pub fn write_dxf(model: &SheetModel, out: &mut dyn Write) -> Result<(), DrawError> {
    let text = dxf_string(model)?;
    out.write_all(text.as_bytes())?;
    Ok(())
}

/// Das Blattmodell als DXF-Text.
pub fn dxf_string(model: &SheetModel) -> Result<String, DrawError> {
    let (w, h) = model.sheet.size_mm();
    let (lo, hi) = model.frame;
    if !(w.is_finite() && h.is_finite() && lo.is_finite() && hi.is_finite()) {
        return Err(DrawError::NonFinite);
    }
    // Blocknamen, eindeutig (ohne Rücksicht auf Groß-/Kleinschreibung).
    let mut used: BTreeSet<String> = ["*MODEL_SPACE".to_string(), "*PAPER_SPACE".to_string()]
        .into_iter()
        .collect();
    let names: Vec<String> = model
        .views
        .iter()
        .map(|v| unique_name(&mut used, &v.name))
        .collect();

    // Handles der Tabellen und Blockeinträge im Voraus; alles Weitere beim Schreiben.
    let mut hs = Handles(0);
    let mut t = TableHandles {
        vport: hs.next(),
        ltype: hs.next(),
        layer: hs.next(),
        style: hs.next(),
        view: hs.next(),
        ucs: hs.next(),
        appid: hs.next(),
        dimstyle: hs.next(),
        block_record: hs.next(),
        plot_placeholder: 0,
    };
    let br_model = hs.next();
    let br_paper = hs.next();
    let br_views: Vec<u64> = names.iter().map(|_| hs.next()).collect();
    let (dict_root, dict_group) = (hs.next(), hs.next());
    let (dict_plot, placeholder) = (hs.next(), hs.next());
    t.plot_placeholder = placeholder;

    let mut tables = Out::new();
    write_tables(
        &mut tables,
        &mut hs,
        &t,
        (w, h),
        &names,
        [br_model, br_paper],
        &br_views,
    )?;

    let mut blocks = Out::new();
    write_blocks(
        &mut blocks,
        &mut hs,
        model,
        &names,
        br_model,
        br_paper,
        &br_views,
    )?;

    let mut entities = Out::new();
    write_border(&mut entities, &mut hs, br_model, lo, hi)?;
    for name in &names {
        entities.entity_head("INSERT", hs.next(), br_model, "0");
        entities.text(100, "AcDbBlockReference");
        entities.text(2, &encode(name));
        entities.point(10, DVec2::ZERO)?;
    }

    let mut s = String::new();
    s.push_str(&header(w, h, hs.0 + 1)?);
    s.push_str("  0\r\nSECTION\r\n  2\r\nCLASSES\r\n");
    for (name, cpp) in [
        ("ACDBDICTIONARYWDFLT", "AcDbDictionaryWithDefault"),
        ("ACDBPLACEHOLDER", "AcDbPlaceHolder"),
    ] {
        let mut o = Out::new();
        o.text(0, "CLASS");
        o.text(1, name);
        o.text(2, cpp);
        o.text(3, "ObjectDBX Classes");
        o.int(90, 0);
        o.int(280, 0);
        o.int(281, 0);
        s.push_str(&o.s);
    }
    s.push_str("  0\r\nENDSEC\r\n");
    s.push_str("  0\r\nSECTION\r\n  2\r\nTABLES\r\n");
    s.push_str(&tables.s);
    s.push_str("  0\r\nENDSEC\r\n");
    s.push_str("  0\r\nSECTION\r\n  2\r\nBLOCKS\r\n");
    s.push_str(&blocks.s);
    s.push_str("  0\r\nENDSEC\r\n");
    s.push_str("  0\r\nSECTION\r\n  2\r\nENTITIES\r\n");
    s.push_str(&entities.s);
    s.push_str("  0\r\nENDSEC\r\n");
    // OBJECTS: Wurzelverzeichnis mit dem Eintrag ACAD_GROUP (ohne Layouts).
    let mut objects = Out::new();
    objects.text(0, "DICTIONARY");
    objects.text(5, &hex(dict_root));
    objects.text(330, "0");
    objects.text(100, "AcDbDictionary");
    objects.int(281, 1);
    objects.text(3, "ACAD_GROUP");
    objects.text(350, &hex(dict_group));
    objects.text(3, "ACAD_PLOTSTYLENAME");
    objects.text(350, &hex(dict_plot));
    objects.text(0, "DICTIONARY");
    objects.text(5, &hex(dict_group));
    objects.text(330, &hex(dict_root));
    objects.text(100, "AcDbDictionary");
    objects.int(281, 1);
    // Plotstil-Kette: Verzeichnis mit Vorgabe „Normal“ -> Platzhalter; jede Ebene verweist mit 390.
    objects.text(0, "ACDBDICTIONARYWDFLT");
    objects.text(5, &hex(dict_plot));
    objects.text(330, &hex(dict_root));
    objects.text(100, "AcDbDictionary");
    objects.int(281, 1);
    objects.text(3, "Normal");
    objects.text(350, &hex(placeholder));
    objects.text(100, "AcDbDictionaryWithDefault");
    objects.text(340, &hex(placeholder));
    objects.text(0, "ACDBPLACEHOLDER");
    objects.text(5, &hex(placeholder));
    objects.text(330, &hex(dict_plot));
    s.push_str("  0\r\nSECTION\r\n  2\r\nOBJECTS\r\n");
    s.push_str(&objects.s);
    s.push_str("  0\r\nENDSEC\r\n");
    s.push_str("  0\r\nEOF\r\n");
    Ok(s)
}

// ---------------------------------------------------------------------------------------------
// Ausgabe von Gruppen, Zahlen, Namen
// ---------------------------------------------------------------------------------------------

/// Fortlaufende Handles ab 1.
struct Handles(u64);

impl Handles {
    fn next(&mut self) -> u64 {
        self.0 += 1;
        self.0
    }
}

fn hex(h: u64) -> String {
    format!("{h:X}")
}

/// Zahl mit höchstens neun Nachkommastellen, ohne Exponent und ohne nachgestellte Nullen;
/// `-0` wird `0`. Nicht endliche Zahlen sind ein Fehler.
fn num(x: f64) -> Result<String, DrawError> {
    if !x.is_finite() {
        return Err(DrawError::NonFinite);
    }
    let t = format!("{x:.prec$}", prec = DECIMALS);
    let t = t.trim_end_matches('0').trim_end_matches('.');
    Ok(if t == "-0" || t.is_empty() {
        "0".to_string()
    } else {
        t.to_string()
    })
}

/// Winkel in `[0, 2π)`.
fn wrap(a: f64) -> f64 {
    let r = a.rem_euclid(TAU);
    if r >= TAU { 0.0 } else { r }
}

/// Winkel in Grad aus einem Winkel im Bogenmaß; nach dem Runden nie 360.
fn degrees(rad: f64) -> Result<String, DrawError> {
    let t = num(wrap(rad).to_degrees())?;
    Ok(if t.parse::<f64>().is_ok_and(|v| v >= 360.0) {
        "0".to_string()
    } else {
        t
    })
}

/// Parameter im Bogenmaß in `[0, 2π]` (nach dem Runden nie über 2π).
fn radians(rad: f64) -> Result<String, DrawError> {
    num(wrap(rad))
}

/// Ein Zeichen, das in einem Blocknamen nicht vorkommen darf.
fn illegal(c: char) -> bool {
    matches!(
        c,
        '<' | '>' | '/' | '\\' | '"' | ':' | ';' | '?' | '*' | '|' | '=' | '`' | ','
    ) || c.is_control()
        || (c as u32) > 0xFFFF
        || c == '\u{FFFE}'
        || c == '\u{FFFF}'
}

/// Länge eines Namens in der Datei (ASCII: 1, sonst `\U+XXXX` = 7).
fn encoded_len(c: char) -> usize {
    if c.is_ascii() { 1 } else { 7 }
}

/// Namen in der Schreibweise der Datei: ASCII, alles andere als `\U+XXXX`.
fn encode(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        if c.is_ascii() {
            o.push(c);
        } else {
            o.push_str(&format!("\\U+{:04X}", c as u32));
        }
    }
    o
}

/// `base` so gekürzt, dass `base + suffix` in der Datei höchstens `MAX_NAME_LEN` lang ist.
fn fit_name(base: &str, suffix: &str) -> String {
    let room = MAX_NAME_LEN.saturating_sub(suffix.len());
    let mut o = String::new();
    let mut len = 0;
    for c in base.chars() {
        let l = encoded_len(c);
        if len + l > room {
            break;
        }
        o.push(c);
        len += l;
    }
    o.push_str(suffix);
    o
}

/// Eindeutiger, zulässiger Blockname: Zeichen ersetzen, kürzen, bei Wiederholung ` 2`, ` 3`, …
/// anhängen; leer wird zu „Ansicht“. Der Vergleich ignoriert Groß-/Kleinschreibung.
fn unique_name(used: &mut BTreeSet<String>, raw: &str) -> String {
    let cleaned: String = raw
        .chars()
        .map(|c| if illegal(c) { '_' } else { c })
        .collect();
    let cleaned = cleaned.trim();
    let base = if cleaned.is_empty() {
        "Ansicht"
    } else {
        cleaned
    };
    let mut n = 1;
    loop {
        let suffix = if n == 1 {
            String::new()
        } else {
            format!(" {n}")
        };
        let name = fit_name(base, &suffix);
        if used.insert(name.to_uppercase()) {
            return name;
        }
        n += 1;
    }
}

struct Out {
    s: String,
}

impl Out {
    fn new() -> Out {
        Out { s: String::new() }
    }

    fn text(&mut self, code: i32, v: &str) {
        self.s.push_str(&format!("{code:>3}\r\n{v}\r\n"));
    }

    fn int(&mut self, code: i32, v: i64) {
        self.text(code, &v.to_string());
    }

    fn num(&mut self, code: i32, v: f64) -> Result<(), DrawError> {
        let t = num(v)?;
        self.text(code, &t);
        Ok(())
    }

    /// Punkt in der Ebene als Gruppen `base`, `base + 10`, `base + 20` (z = 0).
    fn point(&mut self, base: i32, p: DVec2) -> Result<(), DrawError> {
        self.num(base, p.x)?;
        self.num(base + 10, p.y)?;
        self.num(base + 20, 0.0)
    }

    /// Normale `0, 0, 1`.
    fn normal(&mut self) {
        self.text(210, "0");
        self.text(220, "0");
        self.text(230, "1");
    }

    /// Anfang eines Objekts: Art, Handle, Besitzer, `AcDbEntity`, Ebene.
    fn entity_head(&mut self, kind: &str, handle: u64, owner: u64, layer: &str) {
        self.text(0, kind);
        self.text(5, &hex(handle));
        self.text(330, &hex(owner));
        self.text(100, "AcDbEntity");
        self.text(8, layer);
    }

    /// Anfang einer Tabelle.
    fn table_head(&mut self, name: &str, handle: u64, count: usize) {
        self.text(0, "TABLE");
        self.text(2, name);
        self.text(5, &hex(handle));
        self.text(330, "0");
        self.text(100, "AcDbSymbolTable");
        self.int(70, count as i64);
        if name == "DIMSTYLE" {
            self.text(100, "AcDbDimStyleTable");
            self.int(71, count as i64);
        }
    }

    /// Anfang eines Tabelleneintrags.
    fn record_head(&mut self, kind: &str, handle: u64, table: u64, marker: &str) {
        self.text(0, kind);
        self.text(if kind == "DIMSTYLE" { 105 } else { 5 }, &hex(handle));
        self.text(330, &hex(table));
        self.text(100, "AcDbSymbolTableRecord");
        self.text(100, marker);
    }

    fn end_table(&mut self) {
        self.text(0, "ENDTAB");
    }
}

// ---------------------------------------------------------------------------------------------
// Kopf und Tabellen
// ---------------------------------------------------------------------------------------------

fn header(w: f64, h: f64, handseed: u64) -> Result<String, DrawError> {
    let mut o = Out::new();
    o.text(0, "SECTION");
    o.text(2, "HEADER");
    o.text(9, "$ACADVER");
    o.text(1, "AC1015");
    o.text(9, "$DWGCODEPAGE");
    o.text(3, "ANSI_1252");
    o.text(9, "$HANDSEED");
    o.text(5, &hex(handseed));
    o.text(9, "$INSBASE");
    o.point(10, DVec2::ZERO)?;
    o.text(9, "$EXTMIN");
    o.point(10, DVec2::ZERO)?;
    o.text(9, "$EXTMAX");
    o.point(10, DVec2::new(w, h))?;
    o.text(9, "$LIMMIN");
    o.num(10, 0.0)?;
    o.num(20, 0.0)?;
    o.text(9, "$LIMMAX");
    o.num(10, w)?;
    o.num(20, h)?;
    o.text(9, "$LTSCALE");
    o.num(40, 1.0)?;
    o.text(9, "$MEASUREMENT");
    o.int(70, 1);
    o.text(9, "$INSUNITS");
    o.int(70, 4);
    o.text(0, "ENDSEC");
    Ok(o.s)
}

struct TableHandles {
    vport: u64,
    ltype: u64,
    layer: u64,
    style: u64,
    view: u64,
    ucs: u64,
    appid: u64,
    dimstyle: u64,
    block_record: u64,
    /// Platzhalter des Plotstils `Normal`, auf den jede Ebene mit 390 verweist.
    plot_placeholder: u64,
}

fn write_tables(
    o: &mut Out,
    hs: &mut Handles,
    t: &TableHandles,
    (w, h): (f64, f64),
    names: &[String],
    [br_model, br_paper]: [u64; 2],
    br_views: &[u64],
) -> Result<(), DrawError> {
    // VPORT: eine aktive Ansicht, die das ganze Blatt zeigt.
    o.table_head("VPORT", t.vport, 1);
    o.record_head("VPORT", hs.next(), t.vport, "AcDbViewportTableRecord");
    o.text(2, "*ACTIVE");
    o.int(70, 0);
    o.num(10, 0.0)?;
    o.num(20, 0.0)?;
    o.num(11, 1.0)?;
    o.num(21, 1.0)?;
    o.num(12, 0.5 * w)?;
    o.num(22, 0.5 * h)?;
    o.num(13, 0.0)?;
    o.num(23, 0.0)?;
    o.num(14, 1.0)?;
    o.num(24, 1.0)?;
    o.num(15, 1.0)?;
    o.num(25, 1.0)?;
    o.num(16, 0.0)?;
    o.num(26, 0.0)?;
    o.num(36, 1.0)?;
    o.num(17, 0.0)?;
    o.num(27, 0.0)?;
    o.num(37, 0.0)?;
    o.num(40, 1.1 * h)?;
    o.num(41, w / h)?;
    o.num(42, 50.0)?;
    o.num(43, 0.0)?;
    o.num(44, 0.0)?;
    o.num(50, 0.0)?;
    o.num(51, 0.0)?;
    for (code, v) in [
        (71, 0),
        (72, 100),
        (73, 1),
        (74, 3),
        (75, 0),
        (76, 0),
        (77, 0),
        (78, 0),
    ] {
        o.int(code, v);
    }
    o.end_table();

    // LTYPE: ByBlock, ByLayer, CONTINUOUS, HIDDEN (Strich 3 mm, Lücke 1 mm).
    o.table_head("LTYPE", t.ltype, 4);
    for (name, desc) in [
        ("ByBlock", ""),
        ("ByLayer", ""),
        ("CONTINUOUS", "Solid line"),
    ] {
        o.record_head("LTYPE", hs.next(), t.ltype, "AcDbLinetypeTableRecord");
        o.text(2, name);
        o.int(70, 0);
        o.text(3, desc);
        o.int(72, 65);
        o.int(73, 0);
        o.num(40, 0.0)?;
    }
    o.record_head("LTYPE", hs.next(), t.ltype, "AcDbLinetypeTableRecord");
    o.text(2, "HIDDEN");
    o.int(70, 0);
    o.text(3, "Hidden __ __ __ __ __ __ __ __");
    o.int(72, 65);
    o.int(73, 2);
    o.num(40, 4.0)?;
    o.num(49, 3.0)?;
    o.int(74, 0);
    o.num(49, -1.0)?;
    o.int(74, 0);
    o.end_table();

    // LAYER: 0, SICHTBAR (0,5 mm), VERDECKT (0,25 mm, gestrichelt).
    o.table_head("LAYER", t.layer, 3);
    for (name, ltype, weight) in [
        ("0", "CONTINUOUS", -3),
        (LAYER_VISIBLE, "CONTINUOUS", 50),
        (LAYER_HIDDEN, "HIDDEN", 25),
    ] {
        o.record_head("LAYER", hs.next(), t.layer, "AcDbLayerTableRecord");
        o.text(2, name);
        o.int(70, 0);
        o.int(62, 7);
        o.text(6, ltype);
        o.int(370, weight);
        o.text(390, &hex(t.plot_placeholder));
    }
    o.end_table();

    // STYLE: STANDARD.
    o.table_head("STYLE", t.style, 1);
    let style_handle = hs.next();
    o.record_head("STYLE", style_handle, t.style, "AcDbTextStyleTableRecord");
    o.text(2, "STANDARD");
    o.int(70, 0);
    o.num(40, 0.0)?;
    o.num(41, 1.0)?;
    o.num(50, 0.0)?;
    o.int(71, 0);
    o.num(42, 2.5)?;
    o.text(3, "txt");
    o.text(4, "");
    o.end_table();

    // VIEW, UCS: leer.
    o.table_head("VIEW", t.view, 0);
    o.end_table();
    o.table_head("UCS", t.ucs, 0);
    o.end_table();

    // APPID: ACAD.
    o.table_head("APPID", t.appid, 1);
    o.record_head("APPID", hs.next(), t.appid, "AcDbRegAppTableRecord");
    o.text(2, "ACAD");
    o.int(70, 0);
    o.end_table();

    // DIMSTYLE: Standard (Handle in Gruppe 105).
    o.table_head("DIMSTYLE", t.dimstyle, 1);
    o.record_head("DIMSTYLE", hs.next(), t.dimstyle, "AcDbDimStyleTableRecord");
    o.text(2, "Standard");
    o.int(70, 0);
    for (code, v) in [
        (40, 1.0),
        (41, 2.5),
        (42, 0.625),
        (43, 3.75),
        (44, 1.25),
        (140, 2.5),
        (141, 2.5),
        (147, 0.625),
    ] {
        o.num(code, v)?;
    }
    o.text(340, &hex(style_handle));
    o.end_table();

    // BLOCK_RECORD: die zwei Räume und ein Eintrag je Ansicht.
    o.table_head("BLOCK_RECORD", t.block_record, 2 + names.len());
    let all = [("*Model_Space", br_model), ("*Paper_Space", br_paper)]
        .into_iter()
        .map(|(n, h)| (n.to_string(), h))
        .chain(
            names
                .iter()
                .map(|n| encode(n))
                .zip(br_views.iter().copied()),
        );
    for (name, handle) in all {
        o.record_head(
            "BLOCK_RECORD",
            handle,
            t.block_record,
            "AcDbBlockTableRecord",
        );
        o.text(2, &name);
        o.int(70, 0);
        o.int(280, 1);
        o.int(281, 0);
    }
    o.end_table();
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// Blöcke und Objekte
// ---------------------------------------------------------------------------------------------

fn write_blocks(
    o: &mut Out,
    hs: &mut Handles,
    model: &SheetModel,
    names: &[String],
    br_model: u64,
    br_paper: u64,
    br_views: &[u64],
) -> Result<(), DrawError> {
    block_begin(o, hs.next(), br_model, "*Model_Space", false)?;
    block_end(o, hs.next(), br_model, false);
    block_begin(o, hs.next(), br_paper, "*Paper_Space", true)?;
    block_end(o, hs.next(), br_paper, true);
    for ((view, name), &br) in model.views.iter().zip(names).zip(br_views) {
        block_begin(o, hs.next(), br, &encode(name), false)?;
        for line in &view.lines {
            let layer = match line.vis {
                Visibility::Visible => LAYER_VISIBLE,
                Visibility::Hidden => LAYER_HIDDEN,
            };
            write_curve(o, hs, br, layer, &line.curve)?;
        }
        block_end(o, hs.next(), br, false);
    }
    Ok(())
}

fn block_begin(
    o: &mut Out,
    handle: u64,
    record: u64,
    name: &str,
    paper: bool,
) -> Result<(), DrawError> {
    o.text(0, "BLOCK");
    o.text(5, &hex(handle));
    o.text(330, &hex(record));
    o.text(100, "AcDbEntity");
    if paper {
        o.int(67, 1);
    }
    o.text(8, "0");
    o.text(100, "AcDbBlockBegin");
    o.text(2, name);
    o.int(70, 0);
    o.point(10, DVec2::ZERO)?;
    o.text(3, name);
    o.text(1, "");
    Ok(())
}

fn block_end(o: &mut Out, handle: u64, record: u64, paper: bool) {
    o.text(0, "ENDBLK");
    o.text(5, &hex(handle));
    o.text(330, &hex(record));
    o.text(100, "AcDbEntity");
    if paper {
        o.int(67, 1);
    }
    o.text(8, "0");
    o.text(100, "AcDbBlockEnd");
}

/// Innenrahmen als geschlossene Polylinie auf `SICHTBAR` im Modellraum.
fn write_border(
    o: &mut Out,
    hs: &mut Handles,
    owner: u64,
    lo: DVec2,
    hi: DVec2,
) -> Result<(), DrawError> {
    o.entity_head("LWPOLYLINE", hs.next(), owner, LAYER_VISIBLE);
    o.text(100, "AcDbPolyline");
    o.int(90, 4);
    o.int(70, 1);
    o.num(43, 0.0)?;
    for p in [
        DVec2::new(lo.x, lo.y),
        DVec2::new(hi.x, lo.y),
        DVec2::new(hi.x, hi.y),
        DVec2::new(lo.x, hi.y),
    ] {
        o.num(10, p.x)?;
        o.num(20, p.y)?;
    }
    Ok(())
}

fn write_curve(
    o: &mut Out,
    hs: &mut Handles,
    owner: u64,
    layer: &str,
    curve: &Out2d,
) -> Result<(), DrawError> {
    if !curve.is_finite() {
        return Err(DrawError::NonFinite);
    }
    match curve {
        Out2d::Line { a, b } => {
            o.entity_head("LINE", hs.next(), owner, layer);
            o.text(100, "AcDbLine");
            o.point(10, *a)?;
            o.point(11, *b)
        }
        Out2d::Arc { c, r, .. } => {
            let Some((s0, span)) = curve.normalized_range() else {
                return Err(DrawError::NonFinite);
            };
            if span <= 0.0 || *r == 0.0 {
                return Ok(());
            }
            // Ein negativer Radius liegt auf der Gegenseite des Mittelpunkts.
            let (radius, shift) = if *r < 0.0 { (-*r, PI) } else { (*r, 0.0) };
            let a0 = wrap(s0 + shift);
            let (start, end) = (degrees(a0)?, degrees(a0 + span)?);
            // Fast voller Bogen, dessen Winkel nach dem Runden zusammenfallen: voller Kreis.
            if span >= TAU - 1e-12 || (start == end && span > PI) {
                o.entity_head("CIRCLE", hs.next(), owner, layer);
                o.text(100, "AcDbCircle");
                o.point(10, *c)?;
                return o.num(40, radius);
            }
            if start == end {
                // Öffnung unter der Auflösung der Datei: nichts zu zeichnen.
                return Ok(());
            }
            o.entity_head("ARC", hs.next(), owner, layer);
            o.text(100, "AcDbCircle");
            o.point(10, *c)?;
            o.num(40, radius)?;
            o.text(100, "AcDbArc");
            o.text(50, &start);
            o.text(51, &end);
            Ok(())
        }
        Out2d::Ellipse {
            c,
            major,
            ratio,
            start,
            end,
        } => {
            let (mut major, mut ratio, mut start, mut end) = (*major, *ratio, *start, *end);
            // Negatives Verhältnis kehrt die Umlaufrichtung um: P(s) mit −ratio = P(−s) mit ratio.
            if ratio < 0.0 {
                ratio = -ratio;
                (start, end) = (-end, -start);
            }
            // Verhältnis über 1: Achsen vertauschen (neue Hauptachse = alte Nebenachse,
            // Parameter um π/2 versetzt).
            if ratio > 1.0 {
                major = DVec2::new(-major.y, major.x) * ratio;
                ratio = 1.0 / ratio;
                start -= 0.5 * PI;
                end -= 0.5 * PI;
            }
            if !(major.is_finite() && ratio.is_finite() && start.is_finite() && end.is_finite()) {
                return Err(DrawError::NonFinite);
            }
            if major.length() == 0.0 {
                return Ok(());
            }
            let ratio = ratio.max(MIN_RATIO);
            let tmp = Out2d::Ellipse {
                c: *c,
                major,
                ratio,
                start,
                end,
            };
            let Some((s0, span)) = tmp.normalized_range() else {
                return Err(DrawError::NonFinite);
            };
            if span <= 0.0 {
                return Ok(());
            }
            let (a, b) = (radians(s0)?, radians(s0 + span)?);
            // Fast voller Umlauf, dessen Parameter nach dem Runden zusammenfallen: ganze Ellipse.
            let (p0, p1) = if span >= TAU - 1e-12 || (a == b && span > PI) {
                ("0".to_string(), num(TAU)?)
            } else if a == b {
                return Ok(());
            } else {
                (a, b)
            };
            o.entity_head("ELLIPSE", hs.next(), owner, layer);
            o.text(100, "AcDbEllipse");
            o.point(10, *c)?;
            o.point(11, major)?;
            o.normal();
            o.num(40, ratio)?;
            o.text(41, &p0);
            o.text(42, &p1);
            Ok(())
        }
        Out2d::Spline {
            degree,
            knots,
            ctrl,
            weights,
        } => {
            let rational = weights.iter().any(|&w| w != 1.0);
            o.entity_head("SPLINE", hs.next(), owner, layer);
            o.text(100, "AcDbSpline");
            o.normal();
            o.int(70, if rational { 12 } else { 8 });
            o.int(71, *degree as i64);
            o.int(72, knots.len() as i64);
            o.int(73, ctrl.len() as i64);
            o.int(74, 0);
            o.num(42, 1e-7)?;
            o.num(43, 1e-7)?;
            for &k in knots {
                o.num(40, k)?;
            }
            if rational {
                for &w in weights {
                    o.num(41, w)?;
                }
            }
            for &p in ctrl {
                o.point(10, p)?;
            }
            Ok(())
        }
    }
}
