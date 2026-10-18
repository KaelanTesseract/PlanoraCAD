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
//! * `ENTITIES`: der Blattrahmen (`SheetModel.frame`, falls vorhanden; siehe unten), danach je
//!   Ansicht ein `INSERT` bei (0, 0, 0) (Ebene `0`). Jedes Objekt: Handle (5), Besitzer (330 = Blockeintrag
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
//!   Anfangs- und Endparameter (41/42) in Radiant, in voller `f64`-Genauigkeit (ein auf neun Stellen
//!   gerundetes 2π liegt knapp darunter, und Fusion 360 lässt eine so geschriebene volle Ellipse aus). Der Parameter der DXF-Ellipse ist wie der von
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
//!
//! # Maße und Mittellinien
//!
//! Die Markierungen einer Ansicht (`SheetView.marks`) stehen hinter ihren Linien im Block der
//! Ansicht. Ebenen und Linienart kommen **nur** hinzu, wenn Markierungen dieser Art vorkommen;
//! ein Blatt ohne Markierungen ergibt Byte für Byte die Datei von vorher (gleiche Zähler,
//! gleiche Handles). Neue Einträge hängen hinten an ihre Tabelle an und bekommen Handles in
//! Schreibreihenfolge wie alle anderen.
//!
//! * Ebenen (je Farbe, Linienart, Linienstärke, Verweis 390 auf den Plotstil-Platzhalter wie
//!   die alten Ebenen): `MASSE` (Maße; 7, `CONTINUOUS`, 25), `MITTEL` (Mittellinien; 7,
//!   `CENTER`, 25), `MASSE_FEHLT` (Maße und Nummernkreise mit verlorenem Bezug; 1 = rot,
//!   `CONTINUOUS`, 25), `POSITIONEN` (Kreis und Zahl der Nummernkreise; 7, `CONTINUOUS`, auf A3
//!   35) und `POSITIONEN_LINIE` (Hinweislinie und Punkt; 7, `CONTINUOUS`, auf A3 25). Die
//!   Stärken der beiden Positionsebenen sind wie beim Blattrahmen die mit dem Faktor des Blatts
//!   skalierten (nie unter 0,18 mm), gerundet nach [`dxf_lineweight`] (A4: 25 und 18).
//! * Linienart `CENTER` (nur mit `MITTEL`): Strich 12, Lücke 3, Strich 2, Lücke 3 mm
//!   (`72 65`, `73 4`, `40 20`, je Element `49` Länge und `74 0`).
//! * `Mark::Line` → `LINE`. `Mark::Arc` → `ARC` wie die Bögen der Linien (Grad, normalisiert,
//!   voller Umlauf als `CIRCLE`; Radius ≤ 0 wird ausgelassen).
//! * `Mark::Arrow` → `SOLID` (`100 AcDbTrace`): DXF füllt die Ecken in der Reihenfolge 1, 2, 4,
//!   3; das Dreieck hat deshalb Ecke 1 = Spitze, Ecken 2 und 3 am Fuß, Ecke 4 = Ecke 3. Dicke
//!   `39 0`.
//! * `Mark::Text` → `TEXT`: `100 AcDbText`, Einfügepunkt 10/20/30, Höhe 40, Text 1, Drehung 50
//!   in Grad (`[0, 360)`), Stil `7 STANDARD`, waagerechte Ausrichtung 72 (0 Anfang, 1 Mitte,
//!   2 Ende) mit Ausrichtepunkt 11/21/31 = Einfügepunkt, dann `100 AcDbText` und `73 0`
//!   (Grundlinie). Zeichen: `Ø` → `%%c`, `°` → `%%d`, `±` → `%%p`, `−` (U+2212) → `-`, `^` →
//!   `^ ` (sonst ein Steuerzeichen-Kürzel), `%` → `%%%` und `\` → `\U+005C` (eigene Texte
//!   bilden so keine Kürzel wie `%%c` oder `\U+…`; `\U+0025` hilft bei `%` nicht, weil
//!   CAD-Programme `\U+…` beim Laden, `%%…` erst beim Anzeigen auflösen), Steuerzeichen →
//!   Leerzeichen, übrige Zeichen außerhalb
//!   von ASCII → `\U+XXXX`, außerhalb der Grundebene → `?`. Höchstens 255 Zeichen in der Datei;
//!   ein längerer Text ist der Fehler `Unsupported("Der Maßtext ist zu lang.")`, und es wird
//!   nichts geschrieben. Nicht endliche Zahlen sind wie überall `NonFinite`.
//!
//! # Blattrahmen
//!
//! Die Markierungen des Blattrahmens stehen direkt im Modellraum vor den `INSERT`s, mit den
//! Ebenen nach Strichstärke: `BLATTRAHMEN` (7, `CONTINUOUS`, auf A3 70: Rahmen und Außenlinie des
//! Schriftfelds), `BLATT_MITTE` (auf A3 50: Zentriermarken, Projektionssymbol) und `BLATT` (auf A3
//! 25: übrige Linien und alle Texte). Auf anderen Formaten ist es die mit `SheetFrame.scale`
//! skalierte Stärke (nie unter 0,18 mm), gerundet auf die nächste Stärke, die DXF kennt
//! ([`dxf_lineweight`]; A4: 50/35/18). Die Ebenen werden nur angelegt, wenn es einen Rahmen
//! gibt; ohne Rahmen enthält die Datei weder diese Ebenen noch eine `LWPOLYLINE`. Zu lange Texte
//! (mehr als 255 Zeichen in der Datei) werden wie Maßtexte abgelehnt, nie still gekürzt; vorher
//! prüft [`dxf_text_fits`]. Die Texte des Rahmens für DXF passt
//! [`sheet_frame_dxf`](crate::sheet_marks::sheet_frame_dxf) mit einem Breitenzuschlag ein.
//!
//! Handtest in Fusion 360 (nach jeder Änderung am Aufbau): Datei mit Maßen, verlorenem Maß und
//! Mittellinien öffnen; prüfen, dass die Datei ohne Meldung geladen wird, die Ebenen `MASSE`,
//! `MITTEL`, `MASSE_FEHLT` erscheinen, Texte (auch `%%c`, `%%d`, `%%p`) richtig und an der
//! richtigen Stelle stehen, Pfeile gefüllte Dreiecke sind und die Mittellinien strichpunktiert.

use crate::Visibility;
use crate::annotate::{ARROW_HALF_W, ARROW_LEN, CENTER_PATTERN, Mark, MarkKind, TextAnchor};
use crate::balloon::PositionPen;
use crate::error::DrawError;
use crate::out2d::Out2d;
use crate::sheet::SheetModel;
use crate::sheet_frame::{SheetPen, sheet_scale};
use planora_math::{DVec2, PI, TAU};
use std::collections::BTreeSet;
use std::io::Write;

const LAYER_VISIBLE: &str = "SICHTBAR";
const LAYER_HIDDEN: &str = "VERDECKT";
const LAYER_DIM: &str = "MASSE";
const LAYER_CENTER: &str = "MITTEL";
const LAYER_LOST: &str = "MASSE_FEHLT";
const LAYER_FRAME: &str = "BLATTRAHMEN";
const LAYER_CENTRE: &str = "BLATT_MITTE";
const LAYER_SHEET: &str = "BLATT";
const LAYER_POSITION: &str = "POSITIONEN";
const LAYER_POSITION_LINE: &str = "POSITIONEN_LINIE";
/// Höchstlänge eines Maßtexts in der Datei (Zeichen, nach der Verschlüsselung).
const MAX_TEXT_LEN: usize = 255;
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
    if !(w.is_finite() && h.is_finite()) {
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

    let used = UsedKinds::of(model);
    let mut tables = Out::new();
    write_tables(
        &mut tables,
        &mut hs,
        &t,
        (w, h),
        &names,
        [br_model, br_paper],
        &br_views,
        used,
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
    if let Some(frame) = &model.frame {
        for m in &frame.marks {
            write_mark(&mut entities, &mut hs, br_model, m)?;
        }
    }
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

/// Zahl in kürzester Schreibweise, die denselben `f64` ergibt (volle Genauigkeit, nie mit
/// Exponent); `-0` wird `0`. Für die Parameter der Ellipse: ein auf neun Stellen gerundetes 2π
/// liegt knapp unter 2π, und Fusion 360 zeichnet eine so geschriebene volle Ellipse nicht.
fn exact(x: f64) -> Result<String, DrawError> {
    if !x.is_finite() {
        return Err(DrawError::NonFinite);
    }
    let t = format!("{x}");
    Ok(if t == "-0" { "0".to_string() } else { t })
}

/// Öffnung (Radiant), ab der ein Bogen als voller Kreis gilt: nicht weniger als ein Milliardstel
/// Grad unter einem Umlauf (feiner löst die Datei mit neun Nachkommastellen in Grad nicht auf).
const FULL_ARC_SPAN: f64 = TAU - 1e-9 * (PI / 180.0);
/// Öffnung (Radiant), ab der eine Ellipse als voll gilt (Fehler höchstens 1e-9 · Halbachse).
const FULL_ELLIPSE_SPAN: f64 = TAU - 1e-9;

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

/// Welche Arten von Markierungen auf dem Blatt vorkommen (bestimmt Ebenen und Linienart).
#[derive(Clone, Copy, Debug, Default)]
struct UsedKinds {
    dim: bool,
    center: bool,
    lost: bool,
    /// Blattrahmen (die drei Ebenen `BLATTRAHMEN`, `BLATT_MITTE`, `BLATT`).
    sheet: bool,
    /// Faktor des Blattzubehörs (bestimmt die Linienstärken der drei Blattebenen).
    sheet_scale: f64,
    /// Nummernkreise: Kreis und Zahl (`POSITIONEN`), Hinweislinie und Punkt
    /// (`POSITIONEN_LINIE`).
    position: bool,
    position_line: bool,
    /// Faktor des Blatts (bestimmt die Linienstärken der Ebenen der Nummernkreise).
    paper_scale: f64,
}

impl UsedKinds {
    fn of(model: &SheetModel) -> UsedKinds {
        let mut u = UsedKinds {
            sheet: model.frame.is_some(),
            sheet_scale: model.frame.as_ref().map_or(1.0, |f| f.scale),
            paper_scale: sheet_scale(model.sheet),
            ..UsedKinds::default()
        };
        for m in model.views.iter().flat_map(|v| &v.marks) {
            match mark_kind(m) {
                MarkKind::Dimension => u.dim = true,
                MarkKind::CenterLine => u.center = true,
                MarkKind::Lost => u.lost = true,
                MarkKind::Sheet(_) => u.sheet = true,
                MarkKind::Position(PositionPen::Circle) => u.position = true,
                MarkKind::Position(PositionPen::Leader) => u.position_line = true,
            }
        }
        u
    }
}

/// Die Linienstärken, die DXF kennt (Gruppe 370, Hundertstel Millimeter).
const DXF_LINEWEIGHTS: [i64; 24] = [
    0, 5, 9, 13, 15, 18, 20, 25, 30, 35, 40, 50, 53, 60, 70, 80, 90, 100, 106, 120, 140, 158, 200,
    211,
];

/// Nächste DXF-Linienstärke zu `mm` (bei Gleichstand die dünnere).
pub fn dxf_lineweight(mm: f64) -> i64 {
    let want = mm * 100.0;
    DXF_LINEWEIGHTS
        .iter()
        .copied()
        .min_by(|a, b| {
            ((*a as f64) - want)
                .abs()
                .total_cmp(&((*b as f64) - want).abs())
        })
        .unwrap_or(25)
}

/// Linienstärke einer Blattebene beim Faktor des Blattrahmens.
fn sheet_weight(pen: SheetPen, used: UsedKinds) -> i64 {
    dxf_lineweight(pen.width_at(used.sheet_scale))
}

fn mark_kind(m: &Mark) -> MarkKind {
    match m {
        Mark::Line { kind, .. }
        | Mark::Arrow { kind, .. }
        | Mark::Arc { kind, .. }
        | Mark::Text { kind, .. } => *kind,
    }
}

fn mark_layer(kind: MarkKind) -> &'static str {
    match kind {
        MarkKind::Dimension => LAYER_DIM,
        MarkKind::CenterLine => LAYER_CENTER,
        MarkKind::Lost => LAYER_LOST,
        MarkKind::Sheet(SheetPen::Heavy) => LAYER_FRAME,
        MarkKind::Sheet(SheetPen::Medium) => LAYER_CENTRE,
        MarkKind::Sheet(SheetPen::Thin) => LAYER_SHEET,
        MarkKind::Position(PositionPen::Circle) => LAYER_POSITION,
        MarkKind::Position(PositionPen::Leader) => LAYER_POSITION_LINE,
    }
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

#[allow(clippy::too_many_arguments)]
fn write_tables(
    o: &mut Out,
    hs: &mut Handles,
    t: &TableHandles,
    (w, h): (f64, f64),
    names: &[String],
    [br_model, br_paper]: [u64; 2],
    br_views: &[u64],
    used: UsedKinds,
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

    // LTYPE: ByBlock, ByLayer, CONTINUOUS, HIDDEN (Strich 3 mm, Lücke 1 mm), CENTER nur mit
    // Mittellinien.
    o.table_head("LTYPE", t.ltype, 4 + usize::from(used.center));
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
    if used.center {
        // Strichpunkt: Strich, Lücke, kurzer Strich, Lücke (Lücken negativ).
        o.record_head("LTYPE", hs.next(), t.ltype, "AcDbLinetypeTableRecord");
        o.text(2, "CENTER");
        o.int(70, 0);
        o.text(3, "Center ____ _ ____ _ ____ _ ____");
        o.int(72, 65);
        o.int(73, CENTER_PATTERN.len() as i64);
        o.num(40, CENTER_PATTERN.iter().sum())?;
        for (i, len) in CENTER_PATTERN.iter().enumerate() {
            o.num(49, if i % 2 == 0 { *len } else { -*len })?;
            o.int(74, 0);
        }
    }
    o.end_table();

    // LAYER: 0, SICHTBAR (0,5 mm), VERDECKT (0,25 mm, gestrichelt); dahinter die Ebenen der
    // Markierungen, die vorkommen.
    let mut layers = vec![
        ("0", "CONTINUOUS", 7, -3),
        (LAYER_VISIBLE, "CONTINUOUS", 7, 50),
        (LAYER_HIDDEN, "HIDDEN", 7, 25),
    ];
    for (on, layer) in [
        (used.dim, (LAYER_DIM, "CONTINUOUS", 7, 25)),
        (used.center, (LAYER_CENTER, "CENTER", 7, 25)),
        (used.lost, (LAYER_LOST, "CONTINUOUS", 1, 25)),
        (
            used.sheet,
            (
                LAYER_FRAME,
                "CONTINUOUS",
                7,
                sheet_weight(SheetPen::Heavy, used),
            ),
        ),
        (
            used.sheet,
            (
                LAYER_CENTRE,
                "CONTINUOUS",
                7,
                sheet_weight(SheetPen::Medium, used),
            ),
        ),
        (
            used.sheet,
            (
                LAYER_SHEET,
                "CONTINUOUS",
                7,
                sheet_weight(SheetPen::Thin, used),
            ),
        ),
        (
            used.position,
            (
                LAYER_POSITION,
                "CONTINUOUS",
                7,
                dxf_lineweight(PositionPen::Circle.width_at(used.paper_scale)),
            ),
        ),
        (
            used.position_line,
            (
                LAYER_POSITION_LINE,
                "CONTINUOUS",
                7,
                dxf_lineweight(PositionPen::Leader.width_at(used.paper_scale)),
            ),
        ),
    ] {
        if on {
            layers.push(layer);
        }
    }
    o.table_head("LAYER", t.layer, layers.len());
    for (name, ltype, color, weight) in layers {
        o.record_head("LAYER", hs.next(), t.layer, "AcDbLayerTableRecord");
        o.text(2, name);
        o.int(70, 0);
        o.int(62, color);
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
        for m in &view.marks {
            write_mark(o, hs, br, m)?;
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
            if span >= FULL_ARC_SPAN || (start == end && span > PI) {
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
            // Parameter in voller Genauigkeit; das Ende liegt in (0, 2π] (ein Ende bei 2π bleibt
            // 2π, nicht 0); eine volle Ellipse läuft von 0 bis genau 2π.
            let mut e = s0 + span;
            if e > TAU {
                e -= TAU;
            }
            let (p0, p1) = if span >= FULL_ELLIPSE_SPAN {
                ("0".to_string(), exact(TAU)?)
            } else {
                (exact(s0)?, exact(e)?)
            };
            if p0 == p1 && span <= PI {
                // Öffnung null (kleiner als die Genauigkeit von f64).
                return Ok(());
            }
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

// ---------------------------------------------------------------------------------------------
// Markierungen (Maße, Mittellinien)
// ---------------------------------------------------------------------------------------------

/// Eine Markierung als Objekt im Block `owner` (siehe Modulkommentar).
fn write_mark(o: &mut Out, hs: &mut Handles, owner: u64, m: &Mark) -> Result<(), DrawError> {
    let layer = mark_layer(mark_kind(m));
    match m {
        Mark::Line { a, b, .. } => write_curve(o, hs, owner, layer, &Out2d::Line { a: *a, b: *b }),
        Mark::Arc {
            center,
            r,
            start,
            end,
            ..
        } => {
            let arc = Out2d::Arc {
                c: *center,
                r: *r,
                start: *start,
                end: *end,
            };
            if !arc.is_finite() {
                return Err(DrawError::NonFinite);
            }
            if *r <= 0.0 {
                return Ok(());
            }
            write_curve(o, hs, owner, layer, &arc)
        }
        Mark::Arrow { tip, dir, .. } => {
            if !(tip.is_finite() && dir.is_finite()) {
                return Err(DrawError::NonFinite);
            }
            let base = *tip + *dir * ARROW_LEN;
            let perp = DVec2::new(-dir.y, dir.x) * ARROW_HALF_W;
            let (p, q) = (base + perp, base - perp);
            // DXF füllt 1, 2, 4, 3: Dreieck mit gleicher dritter und vierter Ecke.
            o.entity_head("SOLID", hs.next(), owner, layer);
            o.text(100, "AcDbTrace");
            o.point(10, *tip)?;
            o.point(11, p)?;
            o.point(12, q)?;
            o.point(13, q)?;
            o.num(39, 0.0)
        }
        Mark::Text {
            pos,
            height,
            angle,
            anchor,
            text,
            ..
        } => {
            if !(pos.is_finite() && height.is_finite() && angle.is_finite()) {
                return Err(DrawError::NonFinite);
            }
            let encoded = encode_text(text);
            if encoded.len() > MAX_TEXT_LEN {
                return Err(DrawError::Unsupported("Der Maßtext ist zu lang.".into()));
            }
            let align = match anchor {
                TextAnchor::Start => 0,
                TextAnchor::Middle => 1,
                TextAnchor::End => 2,
            };
            o.entity_head("TEXT", hs.next(), owner, layer);
            o.text(100, "AcDbText");
            o.point(10, *pos)?;
            o.num(40, *height)?;
            o.text(1, &encoded);
            o.text(50, &degrees(*angle)?);
            o.text(7, "STANDARD");
            o.int(72, align);
            o.point(11, *pos)?;
            o.text(100, "AcDbText");
            o.int(73, 0);
            Ok(())
        }
    }
}

/// Passt der Text eines Maßes in ein `TEXT` der Datei (höchstens 255 Zeichen in der
/// Schreibweise der Datei)? Sonst scheitert das Schreiben mit
/// `Unsupported("Der Maßtext ist zu lang.")`; so lässt es sich vorher prüfen.
pub fn dxf_text_fits(text: &str) -> bool {
    encode_text(text).len() <= MAX_TEXT_LEN
}

/// Inhalt eines `TEXT` in der Schreibweise der Datei (nur ASCII, siehe Modulkommentar).
fn encode_text(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\u{d8}' => o.push_str("%%c"),
            '\u{b0}' => o.push_str("%%d"),
            '\u{b1}' => o.push_str("%%p"),
            '\u{2212}' => o.push('-'),
            '^' => o.push_str("^ "),
            // Eigene Texte dürfen keine Kürzel bilden. `%` steht als `%%%` (das Kürzel für ein
            // einzelnes Prozentzeichen): CAD-Programme lösen `\U+…` schon beim Laden auf, `%%…`
            // erst beim Anzeigen; ein `\U+0025` würde also vor einem `%%c` wieder zu `%` und mit
            // ihm ein Kürzel bilden. `\` steht als `\U+005C` (sonst wie in `\U+…`).
            '%' => o.push_str("%%%"),
            '\\' => o.push_str("\\U+005C"),
            c if c.is_control() => o.push(' '),
            c if c.is_ascii() => o.push(c),
            c if (c as u32) <= 0xFFFF => o.push_str(&format!("\\U+{:04X}", c as u32)),
            _ => o.push('?'),
        }
    }
    o
}

/// Kann das Zeichen in einem Text der DXF-Datei stehen: in der Grundebene des Unicodes und kein
/// Steuerzeichen? (Andere Zeichen werden in der Datei durch `?` bzw. ein Leerzeichen ersetzt.)
pub fn dxf_char_ok(c: char) -> bool {
    (c as u32) <= 0xFFFF && !c.is_control()
}
