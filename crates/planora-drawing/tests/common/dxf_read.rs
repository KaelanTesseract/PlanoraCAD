//! Eigener DXF-Leser der Tests, unabhängig von `dxf.rs` und `out2d.rs` geschrieben.
//!
//! Er liest die Datei streng als Folge von Gruppencode/Wert-Paaren (zwei Zeilen je Paar, Zeilenende
//! `\r\n`, nur ASCII), prüft jede Zahl nach der Art ihres Gruppencodes, zerlegt sie in Abschnitte,
//! Tabellen, Blöcke und Objekte und prüft dann den Aufbau einer R2000-Datei (`validate`):
//!
//! * Abschnitte `HEADER, TABLES, BLOCKS, ENTITIES`, danach `EOF` und nichts mehr;
//! * Tabellen in der Reihenfolge `VPORT, LTYPE, LAYER, STYLE, VIEW, UCS, APPID, DIMSTYLE,
//!   BLOCK_RECORD`, Zähler stimmt, jeder Eintrag mit Handle (bei `DIMSTYLE` Gruppe 105), Besitzer
//!   (330 = Tabelle) und Untertyp-Markierungen (100);
//! * jedes Handle der Datei einmalig, `$HANDSEED` größer als alle, jeder Besitzer und jeder Verweis
//!   (330/340) zeigt auf ein vorhandenes Handle;
//! * jede Ebene und jede Linienart, die ein Objekt oder eine Ebene nennt, steht in den Tabellen,
//!   jedes `INSERT` nennt einen vorhandenen Block, jeder Block hat seinen Blockeintrag;
//! * jedes Objekt hat die Gruppen, die sein Typ braucht.
//!
//! Dazu die Auswertung der Objekte (Punkte, Längen, Abstand zu einem Punkt); Splines mit dem
//! Verfahren von de Boor in homogenen Koordinaten.
#![allow(dead_code)]

use std::collections::{BTreeMap, BTreeSet};
use std::f64::consts::TAU;

pub type P = (f64, f64);

// ---------------------------------------------------------------------------------------------
// Gruppen
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct Group {
    pub code: i32,
    pub value: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Text,
    Float,
    Int,
    Handle,
}

/// Art des Werts nach dem Gruppencode (Tabelle der DXF-Referenz, Bereiche für R2000).
pub fn group_kind(code: i32) -> Kind {
    match code {
        5 | 105 | 320..=369 | 390..=399 | 480..=481 => Kind::Handle,
        10..=59 | 110..=149 | 210..=239 | 460..=469 | 1010..=1059 => Kind::Float,
        60..=79
        | 90..=99
        | 160..=179
        | 270..=289
        | 370..=389
        | 400..=409
        | 420..=429
        | 440..=459
        | 1060..=1071 => Kind::Int,
        _ => Kind::Text,
    }
}

impl Group {
    pub fn num(&self) -> f64 {
        self.value.trim().parse::<f64>().unwrap()
    }
}

/// Zerlegt die Datei in Paare. Streng: jede Zeile endet mit `\r\n`, nur ASCII, gerade Zeilenzahl,
/// Codes sind ganze Zahlen, Zahlenwerte sind endlich, ohne Exponent und ohne `-0`.
pub fn parse_groups(text: &str) -> Result<Vec<Group>, String> {
    if !text.is_ascii() {
        return Err("Datei enthält Zeichen außerhalb von ASCII".into());
    }
    if !text.ends_with("\r\n") {
        return Err("Datei endet nicht mit \\r\\n".into());
    }
    let body = &text[..text.len() - 2];
    let lines: Vec<&str> = body.split("\r\n").collect();
    if lines.iter().any(|l| l.contains('\n') || l.contains('\r')) {
        return Err("Zeilenende nicht durchgehend \\r\\n".into());
    }
    if !lines.len().is_multiple_of(2) {
        return Err(format!("ungerade Zeilenzahl {}", lines.len()));
    }
    let mut out = Vec::with_capacity(lines.len() / 2);
    for (i, pair) in lines.chunks(2).enumerate() {
        let code: i32 = pair[0]
            .trim()
            .parse()
            .map_err(|_| format!("Paar {i}: Gruppencode {:?} keine Zahl", pair[0]))?;
        let value = pair[1].to_string();
        match group_kind(code) {
            Kind::Float => {
                let t = value.trim();
                let ok_chars = !t.is_empty()
                    && t.chars().all(|c| c.is_ascii_digit() || "+-.".contains(c))
                    && t.matches('.').count() <= 1;
                let v = t
                    .parse::<f64>()
                    .map_err(|_| format!("Paar {i}: Gruppe {code} Wert {value:?} keine Zahl"))?;
                if !ok_chars || !v.is_finite() {
                    return Err(format!(
                        "Paar {i}: Gruppe {code} Wert {value:?} nicht schlicht/endlich"
                    ));
                }
                if t == "-0" || t.starts_with("-0.") && t[3..].chars().all(|c| c == '0') {
                    return Err(format!("Paar {i}: Gruppe {code} Wert {value:?} ist -0"));
                }
                if let Some((_, frac)) = t.split_once('.')
                    && frac.len() > 9
                {
                    return Err(format!(
                        "Paar {i}: Gruppe {code} Wert {value:?} hat mehr als 9 Nachkommastellen"
                    ));
                }
            }
            Kind::Int => {
                value.trim().parse::<i64>().map_err(|_| {
                    format!("Paar {i}: Gruppe {code} Wert {value:?} keine ganze Zahl")
                })?;
            }
            Kind::Handle => {
                let t = value.as_str();
                if t.is_empty()
                    || !t
                        .chars()
                        .all(|c| c.is_ascii_digit() || ('A'..='F').contains(&c))
                {
                    return Err(format!(
                        "Paar {i}: Gruppe {code} Handle {value:?} nicht hex"
                    ));
                }
            }
            Kind::Text => {}
        }
        out.push(Group { code, value });
    }
    Ok(out)
}

// ---------------------------------------------------------------------------------------------
// Aufbau
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct Record {
    pub kind: String,
    pub handle: String,
    pub owner: String,
    pub groups: Vec<Group>,
}

impl Record {
    pub fn get(&self, code: i32) -> Option<&str> {
        self.groups
            .iter()
            .find(|g| g.code == code)
            .map(|g| g.value.as_str())
    }

    pub fn name(&self) -> String {
        decode_text(self.get(2).unwrap_or(""))
    }

    pub fn num(&self, code: i32) -> Option<f64> {
        self.get(code).map(|v| v.trim().parse().unwrap())
    }
}

#[derive(Clone, Debug)]
pub struct Table {
    pub name: String,
    pub handle: String,
    pub owner: String,
    pub count: i64,
    pub groups: Vec<Group>,
    pub records: Vec<Record>,
}

#[derive(Clone, Debug)]
pub struct Block {
    pub name: String,
    pub begin: Record,
    pub end: Record,
    pub entities: Vec<ReadEntity>,
}

/// Ein gelesenes Objekt mit seinen Rohgruppen.
#[derive(Clone, Debug)]
pub struct ReadEntity {
    pub kind: String,
    pub handle: String,
    pub owner: String,
    pub layer: String,
    pub groups: Vec<Group>,
}

#[derive(Clone, Debug)]
pub struct Dxf {
    pub sections: Vec<String>,
    pub header: Vec<(String, Vec<Group>)>,
    pub tables: Vec<Table>,
    pub blocks: Vec<Block>,
    pub entities: Vec<ReadEntity>,
}

struct Cur {
    g: Vec<Group>,
    i: usize,
}

impl Cur {
    fn peek(&self) -> Option<&Group> {
        self.g.get(self.i)
    }
    fn next(&mut self) -> Result<Group, String> {
        let g = self.g.get(self.i).cloned().ok_or("Datei zu Ende")?;
        self.i += 1;
        Ok(g)
    }
    fn expect(&mut self, code: i32, value: &str) -> Result<(), String> {
        let g = self.next()?;
        if g.code != code || g.value != value {
            return Err(format!(
                "erwartet ({code}, {value:?}), gefunden ({}, {:?}) bei Paar {}",
                g.code,
                g.value,
                self.i - 1
            ));
        }
        Ok(())
    }
    /// Gruppen bis zur nächsten mit Code 0 (nicht verbraucht).
    fn until_zero(&mut self) -> Vec<Group> {
        let mut v = Vec::new();
        while let Some(g) = self.peek() {
            if g.code == 0 {
                break;
            }
            v.push(g.clone());
            self.i += 1;
        }
        v
    }
}

fn find(gs: &[Group], code: i32) -> Option<&Group> {
    gs.iter().find(|g| g.code == code)
}

fn entity_from(kind: String, groups: Vec<Group>) -> ReadEntity {
    let get = |c: i32| {
        find(&groups, c)
            .map(|g| g.value.clone())
            .unwrap_or_default()
    };
    ReadEntity {
        kind,
        handle: get(5),
        owner: get(330),
        layer: get(8),
        groups,
    }
}

fn record_from(kind: String, groups: Vec<Group>, handle_code: i32) -> Record {
    let get = |c: i32| {
        find(&groups, c)
            .map(|g| g.value.clone())
            .unwrap_or_default()
    };
    Record {
        kind,
        handle: get(handle_code),
        owner: get(330),
        groups,
    }
}

/// Liest die Datei und prüft den Aufbau (`validate`). Fehler als Text.
pub fn read(text: &str) -> Result<Dxf, String> {
    let d = parse(text)?;
    validate(&d)?;
    Ok(d)
}

/// Liest nur die Gruppen und die Abschnitte, ohne die R2000-Regeln zu prüfen.
pub fn parse(text: &str) -> Result<Dxf, String> {
    let mut c = Cur {
        g: parse_groups(text)?,
        i: 0,
    };
    let mut d = Dxf {
        sections: Vec::new(),
        header: Vec::new(),
        tables: Vec::new(),
        blocks: Vec::new(),
        entities: Vec::new(),
    };
    loop {
        let g = c.next()?;
        if g.code == 0 && g.value == "EOF" {
            if c.i != c.g.len() {
                return Err("Gruppen nach EOF".into());
            }
            break;
        }
        if !(g.code == 0 && g.value == "SECTION") {
            return Err(format!(
                "SECTION erwartet, gefunden ({}, {:?})",
                g.code, g.value
            ));
        }
        let name = c.next()?;
        if name.code != 2 {
            return Err("Abschnittsname fehlt".into());
        }
        d.sections.push(name.value.clone());
        match name.value.as_str() {
            "HEADER" => {
                let mut cur: Option<(String, Vec<Group>)> = None;
                loop {
                    let g = c.next()?;
                    if g.code == 0 && g.value == "ENDSEC" {
                        break;
                    }
                    if g.code == 9 {
                        if let Some(v) = cur.take() {
                            d.header.push(v);
                        }
                        cur = Some((g.value, Vec::new()));
                    } else if let Some((_, v)) = cur.as_mut() {
                        v.push(g);
                    } else {
                        return Err("Gruppe vor der ersten Kopfvariablen".into());
                    }
                }
                if let Some(v) = cur.take() {
                    d.header.push(v);
                }
            }
            "TABLES" => loop {
                let g = c.next()?;
                if g.code == 0 && g.value == "ENDSEC" {
                    break;
                }
                if !(g.code == 0 && g.value == "TABLE") {
                    return Err(format!("TABLE erwartet, gefunden {:?}", g.value));
                }
                let tn = c.next()?;
                if tn.code != 2 {
                    return Err("Tabellenname fehlt".into());
                }
                let groups = c.until_zero();
                let handle = find(&groups, 5)
                    .map(|g| g.value.clone())
                    .unwrap_or_default();
                let owner = find(&groups, 330)
                    .map(|g| g.value.clone())
                    .unwrap_or_default();
                let count = find(&groups, 70)
                    .map(|g| g.value.trim().parse::<i64>().unwrap())
                    .ok_or("Tabelle ohne Zähler (70)")?;
                let mut records = Vec::new();
                loop {
                    let r = c.next()?;
                    if r.code != 0 {
                        return Err("Eintrag erwartet".into());
                    }
                    if r.value == "ENDTAB" {
                        break;
                    }
                    let gs = c.until_zero();
                    let hc = if tn.value == "DIMSTYLE" { 105 } else { 5 };
                    records.push(record_from(r.value, gs, hc));
                }
                d.tables.push(Table {
                    name: tn.value,
                    handle,
                    owner,
                    count,
                    groups,
                    records,
                });
            },
            "BLOCKS" => loop {
                let g = c.next()?;
                if g.code == 0 && g.value == "ENDSEC" {
                    break;
                }
                if !(g.code == 0 && g.value == "BLOCK") {
                    return Err(format!("BLOCK erwartet, gefunden {:?}", g.value));
                }
                let bg = c.until_zero();
                let begin = record_from("BLOCK".into(), bg, 5);
                let mut ents = Vec::new();
                let end;
                loop {
                    let e = c.next()?;
                    if e.code != 0 {
                        return Err("Objekt erwartet".into());
                    }
                    let gs = c.until_zero();
                    if e.value == "ENDBLK" {
                        end = record_from("ENDBLK".into(), gs, 5);
                        break;
                    }
                    ents.push(entity_from(e.value, gs));
                }
                d.blocks.push(Block {
                    name: begin.name(),
                    begin,
                    end,
                    entities: ents,
                });
            },
            "ENTITIES" => loop {
                let e = c.next()?;
                if e.code == 0 && e.value == "ENDSEC" {
                    break;
                }
                if e.code != 0 {
                    return Err("Objekt erwartet".into());
                }
                let gs = c.until_zero();
                d.entities.push(entity_from(e.value, gs));
            },
            other => return Err(format!("unbekannter Abschnitt {other}")),
        }
    }
    Ok(d)
}

/// Entschlüsselt `\U+XXXX` (Unicode-Schreibweise der DXF-Zeichenketten).
pub fn decode_text(s: &str) -> String {
    let mut out = String::new();
    let b: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < b.len() {
        if b[i] == '\\'
            && i + 6 < b.len()
            && b[i + 1] == 'U'
            && b[i + 2] == '+'
            && let Ok(v) = u32::from_str_radix(&b[i + 3..i + 7].iter().collect::<String>(), 16)
            && let Some(ch) = char::from_u32(v)
        {
            out.push(ch);
            i += 7;
            continue;
        }
        out.push(b[i]);
        i += 1;
    }
    out
}

// ---------------------------------------------------------------------------------------------
// Prüfung des Aufbaus
// ---------------------------------------------------------------------------------------------

pub const TABLE_ORDER: [&str; 9] = [
    "VPORT",
    "LTYPE",
    "LAYER",
    "STYLE",
    "VIEW",
    "UCS",
    "APPID",
    "DIMSTYLE",
    "BLOCK_RECORD",
];

fn table_marker(name: &str) -> &'static str {
    match name {
        "VPORT" => "AcDbViewportTableRecord",
        "LTYPE" => "AcDbLinetypeTableRecord",
        "LAYER" => "AcDbLayerTableRecord",
        "STYLE" => "AcDbTextStyleTableRecord",
        "VIEW" => "AcDbViewTableRecord",
        "UCS" => "AcDbUCSTableRecord",
        "APPID" => "AcDbRegAppTableRecord",
        "DIMSTYLE" => "AcDbDimStyleTableRecord",
        _ => "AcDbBlockTableRecord",
    }
}

fn entity_marker(kind: &str) -> Option<&'static str> {
    Some(match kind {
        "LINE" => "AcDbLine",
        "CIRCLE" | "ARC" => "AcDbCircle",
        "ELLIPSE" => "AcDbEllipse",
        "SPLINE" => "AcDbSpline",
        "LWPOLYLINE" => "AcDbPolyline",
        "INSERT" => "AcDbBlockReference",
        _ => return None,
    })
}

fn markers(gs: &[Group]) -> Vec<&str> {
    gs.iter()
        .filter(|g| g.code == 100)
        .map(|g| g.value.as_str())
        .collect()
}

impl Dxf {
    pub fn table(&self, name: &str) -> Option<&Table> {
        self.tables.iter().find(|t| t.name == name)
    }

    pub fn header_var(&self, name: &str) -> Option<&Vec<Group>> {
        self.header.iter().find(|(n, _)| n == name).map(|(_, v)| v)
    }

    pub fn block(&self, name: &str) -> Option<&Block> {
        self.blocks.iter().find(|b| b.name == name)
    }

    /// Namen aller Blöcke außer den beiden Raumblöcken, in Dateireihenfolge.
    pub fn view_block_names(&self) -> Vec<String> {
        self.blocks
            .iter()
            .filter(|b| !b.name.starts_with('*'))
            .map(|b| b.name.clone())
            .collect()
    }

    /// Alle Handles der Datei in Dateireihenfolge.
    pub fn all_handles(&self) -> Vec<String> {
        let mut v = Vec::new();
        for t in &self.tables {
            v.push(t.handle.clone());
            v.extend(t.records.iter().map(|r| r.handle.clone()));
        }
        for b in &self.blocks {
            v.push(b.begin.handle.clone());
            v.extend(b.entities.iter().map(|e| e.handle.clone()));
            v.push(b.end.handle.clone());
        }
        v.extend(self.entities.iter().map(|e| e.handle.clone()));
        v
    }
}

fn hex(s: &str) -> u64 {
    u64::from_str_radix(s, 16).unwrap()
}

/// Prüft den Aufbau der Datei (siehe Kopf der Datei). Der erste Fehler als Text.
pub fn validate(d: &Dxf) -> Result<(), String> {
    if d.sections != ["HEADER", "TABLES", "BLOCKS", "ENTITIES"] {
        return Err(format!("Abschnittsfolge {:?}", d.sections));
    }
    // Kopf.
    let acad = d.header_var("$ACADVER").ok_or("$ACADVER fehlt")?;
    if acad.len() != 1 || acad[0].code != 1 || acad[0].value != "AC1015" {
        return Err(format!("$ACADVER {acad:?}"));
    }
    for v in [
        "$HANDSEED",
        "$INSUNITS",
        "$MEASUREMENT",
        "$EXTMIN",
        "$EXTMAX",
    ] {
        d.header_var(v).ok_or(format!("{v} fehlt"))?;
    }
    let units = d.header_var("$INSUNITS").unwrap();
    if units.len() != 1 || units[0].code != 70 || units[0].value.trim() != "4" {
        return Err(format!("$INSUNITS {units:?}"));
    }
    let seed = d.header_var("$HANDSEED").unwrap();
    if seed.len() != 1 || seed[0].code != 5 {
        return Err(format!("$HANDSEED {seed:?}"));
    }
    // Tabellen: Reihenfolge, Zähler, Marken.
    let names: Vec<&str> = d.tables.iter().map(|t| t.name.as_str()).collect();
    if names != TABLE_ORDER {
        return Err(format!("Tabellenfolge {names:?}"));
    }
    let mut handle_set: BTreeSet<String> = BTreeSet::new();
    let mut add = |h: &str, what: &str| -> Result<(), String> {
        if h.is_empty() || h == "0" {
            return Err(format!("{what}: Handle fehlt oder 0"));
        }
        if !handle_set.insert(h.to_string()) {
            return Err(format!("{what}: Handle {h} doppelt"));
        }
        Ok(())
    };
    for t in &d.tables {
        add(&t.handle, &format!("Tabelle {}", t.name))?;
        if t.owner != "0" {
            return Err(format!(
                "Tabelle {}: Besitzer {:?} statt 0",
                t.name, t.owner
            ));
        }
        if t.count != t.records.len() as i64 {
            return Err(format!(
                "Tabelle {}: Zähler {} statt {}",
                t.name,
                t.count,
                t.records.len()
            ));
        }
        let m = markers(&t.groups);
        let want: &[&str] = if t.name == "DIMSTYLE" {
            &["AcDbSymbolTable", "AcDbDimStyleTable"]
        } else {
            &["AcDbSymbolTable"]
        };
        if m != want {
            return Err(format!("Tabelle {}: Marken {m:?}", t.name));
        }
        let mut seen_names = BTreeSet::new();
        for r in &t.records {
            if r.kind != t.name {
                return Err(format!("Tabelle {}: Eintrag {}", t.name, r.kind));
            }
            add(&r.handle, &format!("{}-Eintrag", t.name))?;
            if r.owner != t.handle {
                return Err(format!(
                    "{}-Eintrag {}: Besitzer {}",
                    t.name, r.handle, r.owner
                ));
            }
            let m = markers(&r.groups);
            if m != ["AcDbSymbolTableRecord", table_marker(&t.name)] {
                return Err(format!("{}-Eintrag {}: Marken {m:?}", t.name, r.handle));
            }
            let name = r.name().to_uppercase();
            if name.is_empty() && t.name != "VPORT" {
                return Err(format!("{}-Eintrag ohne Namen", t.name));
            }
            if t.name != "VPORT" && !seen_names.insert(name.clone()) {
                return Err(format!("{}: Name {name} doppelt", t.name));
            }
        }
    }
    let layers: BTreeSet<String> = d
        .table("LAYER")
        .unwrap()
        .records
        .iter()
        .map(|r| r.name().to_uppercase())
        .collect();
    let ltypes: BTreeSet<String> = d
        .table("LTYPE")
        .unwrap()
        .records
        .iter()
        .map(|r| r.name().to_uppercase())
        .collect();
    if !layers.contains("0") {
        return Err("Ebene 0 fehlt".into());
    }
    for need in ["BYBLOCK", "BYLAYER", "CONTINUOUS"] {
        if !ltypes.contains(need) {
            return Err(format!("Linienart {need} fehlt"));
        }
    }
    if !d
        .table("STYLE")
        .unwrap()
        .records
        .iter()
        .any(|r| r.name().eq_ignore_ascii_case("STANDARD"))
    {
        return Err("Schriftart STANDARD fehlt".into());
    }
    if !d
        .table("APPID")
        .unwrap()
        .records
        .iter()
        .any(|r| r.name().eq_ignore_ascii_case("ACAD"))
    {
        return Err("Anwendung ACAD fehlt".into());
    }
    for r in &d.table("LAYER").unwrap().records {
        let lt = r
            .get(6)
            .ok_or(format!("Ebene {}: Linienart fehlt", r.name()))?;
        if !ltypes.contains(&lt.to_uppercase()) {
            return Err(format!("Ebene {}: Linienart {lt} unbekannt", r.name()));
        }
        if r.get(62).is_none() {
            return Err(format!("Ebene {}: Farbe fehlt", r.name()));
        }
    }
    // Blockeinträge und Blöcke.
    let br_table = d.table("BLOCK_RECORD").unwrap();
    let mut br_by_name: BTreeMap<String, String> = BTreeMap::new();
    for r in &br_table.records {
        br_by_name.insert(r.name().to_uppercase(), r.handle.clone());
    }
    for need in ["*MODEL_SPACE", "*PAPER_SPACE"] {
        if !br_by_name.contains_key(need) {
            return Err(format!("Blockeintrag {need} fehlt"));
        }
    }
    let first_two: Vec<String> = d
        .blocks
        .iter()
        .take(2)
        .map(|b| b.name.to_uppercase())
        .collect();
    if first_two != ["*MODEL_SPACE", "*PAPER_SPACE"] {
        return Err(format!("erste Blöcke {first_two:?}"));
    }
    if d.blocks.len() != br_table.records.len() {
        return Err(format!(
            "{} Blöcke, aber {} Blockeinträge",
            d.blocks.len(),
            br_table.records.len()
        ));
    }
    let mut block_names = BTreeSet::new();
    for b in &d.blocks {
        let key = b.name.to_uppercase();
        if !block_names.insert(key.clone()) {
            return Err(format!("Block {} doppelt", b.name));
        }
        let br = br_by_name
            .get(&key)
            .ok_or(format!("Block {} ohne Blockeintrag", b.name))?;
        add(&b.begin.handle, &format!("BLOCK {}", b.name))?;
        add(&b.end.handle, &format!("ENDBLK {}", b.name))?;
        if &b.begin.owner != br || &b.end.owner != br {
            return Err(format!(
                "Block {}: Besitzer {}/{} statt {br}",
                b.name, b.begin.owner, b.end.owner
            ));
        }
        let m = markers(&b.begin.groups);
        if m != ["AcDbEntity", "AcDbBlockBegin"] {
            return Err(format!("BLOCK {}: Marken {m:?}", b.name));
        }
        let m = markers(&b.end.groups);
        if m != ["AcDbEntity", "AcDbBlockEnd"] {
            return Err(format!("ENDBLK {}: Marken {m:?}", b.name));
        }
        // Namensgruppen 2 und 3 gleich.
        if b.begin.get(2) != b.begin.get(3) {
            return Err(format!("BLOCK {}: Gruppen 2 und 3 verschieden", b.name));
        }
        for e in &b.entities {
            add(&e.handle, &format!("Objekt in {}", b.name))?;
            if &e.owner != br {
                return Err(format!(
                    "Objekt {} in {}: Besitzer {}",
                    e.handle, b.name, e.owner
                ));
            }
        }
    }
    let model_br = br_by_name["*MODEL_SPACE"].clone();
    for e in &d.entities {
        add(&e.handle, "Objekt in ENTITIES")?;
        if e.owner != model_br {
            return Err(format!(
                "Objekt {}: Besitzer {} statt Modellraum",
                e.handle, e.owner
            ));
        }
    }
    // Handleschranke.
    let max = handle_set.iter().map(|h| hex(h)).max().unwrap_or(0);
    let seed_v = hex(&seed[0].value);
    if seed_v <= max {
        return Err(format!(
            "$HANDSEED {seed_v:X} nicht über dem größten Handle {max:X}"
        ));
    }
    // Verweise (340) und Besitzer (330) in allen Gruppen.
    let mut all_groups: Vec<&Group> = Vec::new();
    for t in &d.tables {
        all_groups.extend(t.groups.iter());
        for r in &t.records {
            all_groups.extend(r.groups.iter());
        }
    }
    for b in &d.blocks {
        all_groups.extend(b.begin.groups.iter());
        all_groups.extend(b.end.groups.iter());
        for e in &b.entities {
            all_groups.extend(e.groups.iter());
        }
    }
    for e in &d.entities {
        all_groups.extend(e.groups.iter());
    }
    for g in all_groups {
        if matches!(g.code, 330 | 340 | 360 | 390)
            && g.value != "0"
            && !handle_set.contains(&g.value)
        {
            return Err(format!("Verweis ({}, {}) zeigt ins Leere", g.code, g.value));
        }
    }
    // Objekte.
    let check_entity = |e: &ReadEntity, ctx: &str| -> Result<(), String> {
        let m = markers(&e.groups);
        let spec = entity_marker(&e.kind).ok_or(format!("{ctx}: Objektart {}", e.kind))?;
        let want: Vec<&str> = if e.kind == "ARC" {
            vec!["AcDbEntity", "AcDbCircle", "AcDbArc"]
        } else {
            vec!["AcDbEntity", spec]
        };
        if m != want {
            return Err(format!("{ctx} {}: Marken {m:?}", e.kind));
        }
        if !layers.contains(&e.layer.to_uppercase()) {
            return Err(format!("{ctx} {}: Ebene {:?} unbekannt", e.kind, e.layer));
        }
        if let Some(lt) = find(&e.groups, 6)
            && !ltypes.contains(&lt.value.to_uppercase())
        {
            return Err(format!(
                "{ctx} {}: Linienart {} unbekannt",
                e.kind, lt.value
            ));
        }
        if e.kind == "INSERT" {
            let n = decode_text(&find(&e.groups, 2).ok_or("INSERT ohne Namen")?.value);
            if !block_names.contains(&n.to_uppercase()) || n.starts_with('*') {
                return Err(format!("{ctx}: INSERT auf unbekannten Block {n}"));
            }
        }
        e.geom().map(|_| ())
    };
    for b in &d.blocks {
        for e in &b.entities {
            check_entity(e, &format!("Block {}", b.name))?;
        }
    }
    for e in &d.entities {
        check_entity(e, "ENTITIES")?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// Geometrie der Objekte
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub enum Geom {
    Line {
        a: P,
        b: P,
    },
    Circle {
        c: P,
        r: f64,
    },
    /// Winkel in Grad, gegen den Uhrzeigersinn von `start` nach `end`.
    Arc {
        c: P,
        r: f64,
        start: f64,
        end: f64,
    },
    /// `major` relativ zum Mittelpunkt; Parameter in Radiant (Ellipse: `c + major·cos + minor·sin`,
    /// `minor = ratio·(−major.y, major.x)`).
    Ellipse {
        c: P,
        major: P,
        ratio: f64,
        start: f64,
        end: f64,
    },
    Spline {
        flags: i64,
        degree: usize,
        knots: Vec<f64>,
        ctrl: Vec<P>,
        weights: Vec<f64>,
        counts: (i64, i64, i64),
    },
    Poly {
        closed: bool,
        pts: Vec<P>,
    },
    Insert {
        name: String,
        at: P,
    },
}

pub fn dist(a: P, b: P) -> f64 {
    ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt()
}

impl ReadEntity {
    fn nums(&self, code: i32) -> Vec<f64> {
        self.groups
            .iter()
            .filter(|g| g.code == code)
            .map(|g| g.num())
            .collect()
    }

    fn one(&self, code: i32) -> Result<f64, String> {
        let v = self.nums(code);
        if v.len() != 1 {
            return Err(format!(
                "{} {}: Gruppe {code} kommt {}-mal vor",
                self.kind,
                self.handle,
                v.len()
            ));
        }
        Ok(v[0])
    }

    /// Der Wert einer Gruppe, `None` wenn sie fehlt.
    pub fn get(&self, code: i32) -> Option<&str> {
        find(&self.groups, code).map(|g| g.value.as_str())
    }

    /// Reihenfolge der Gruppencodes (ohne die Besitzer-/Untertypmarken).
    pub fn codes(&self) -> Vec<i32> {
        self.groups.iter().map(|g| g.code).collect()
    }

    pub fn geom(&self) -> Result<Geom, String> {
        let pt = |x: i32, y: i32, z: i32| -> Result<P, String> {
            let z = self.one(z)?;
            if z != 0.0 {
                return Err(format!("{} {}: z = {z}", self.kind, self.handle));
            }
            Ok((self.one(x)?, self.one(y)?))
        };
        Ok(match self.kind.as_str() {
            "LINE" => Geom::Line {
                a: pt(10, 20, 30)?,
                b: pt(11, 21, 31)?,
            },
            "CIRCLE" => Geom::Circle {
                c: pt(10, 20, 30)?,
                r: self.one(40)?,
            },
            "ARC" => Geom::Arc {
                c: pt(10, 20, 30)?,
                r: self.one(40)?,
                start: self.one(50)?,
                end: self.one(51)?,
            },
            "ELLIPSE" => {
                let ext = (self.one(210)?, self.one(220)?, self.one(230)?);
                if ext != (0.0, 0.0, 1.0) {
                    return Err(format!("ELLIPSE {}: Normale {ext:?}", self.handle));
                }
                let major = (self.one(11)?, self.one(21)?);
                if self.one(31)? != 0.0 {
                    return Err("ELLIPSE: z der Hauptachse".into());
                }
                Geom::Ellipse {
                    c: pt(10, 20, 30)?,
                    major,
                    ratio: self.one(40)?,
                    start: self.one(41)?,
                    end: self.one(42)?,
                }
            }
            "SPLINE" => {
                let flags = self.one(70)? as i64;
                let degree = self.one(71)? as usize;
                let counts = (
                    self.one(72)? as i64,
                    self.one(73)? as i64,
                    self.one(74)? as i64,
                );
                let knots = self.nums(40);
                let xs = self.nums(10);
                let ys = self.nums(20);
                let zs = self.nums(30);
                if xs.len() != ys.len() || xs.len() != zs.len() || zs.iter().any(|&z| z != 0.0) {
                    return Err("SPLINE: Kontrollpunkte uneinheitlich".into());
                }
                let ctrl: Vec<P> = xs.iter().copied().zip(ys.iter().copied()).collect();
                let rational = flags & 4 != 0;
                let weights = if rational {
                    self.nums(41)
                } else {
                    if !self.nums(41).is_empty() {
                        return Err("SPLINE: Gewichte ohne Rational-Flag".into());
                    }
                    vec![1.0; ctrl.len()]
                };
                if flags & 8 == 0 {
                    return Err(format!("SPLINE: Flag planar fehlt ({flags})"));
                }
                if flags & !(8 | 4) != 0 {
                    return Err(format!("SPLINE: unerwartete Flags {flags}"));
                }
                if counts.0 != knots.len() as i64
                    || counts.1 != ctrl.len() as i64
                    || counts.2 != 0
                    || self.nums(11).len() + self.nums(12).len() + self.nums(13).len() != 0
                {
                    return Err(format!("SPLINE: Zähler {counts:?} passen nicht"));
                }
                if weights.len() != ctrl.len() {
                    return Err("SPLINE: Gewichtezahl".into());
                }
                if knots.len() != ctrl.len() + degree + 1 {
                    return Err("SPLINE: Knotenzahl passt nicht zu Grad und Kontrollpunkten".into());
                }
                if degree < 1 || ctrl.len() <= degree {
                    return Err("SPLINE: Grad".into());
                }
                if knots.windows(2).any(|w| w[0] > w[1]) {
                    return Err("SPLINE: Knoten fallen".into());
                }
                if weights.iter().any(|&w| w <= 0.0) {
                    return Err("SPLINE: Gewicht <= 0".into());
                }
                let ext = (self.one(210)?, self.one(220)?, self.one(230)?);
                if ext != (0.0, 0.0, 1.0) {
                    return Err("SPLINE: Normale".into());
                }
                Geom::Spline {
                    flags,
                    degree,
                    knots,
                    ctrl,
                    weights,
                    counts,
                }
            }
            "LWPOLYLINE" => {
                let n = self.one(90)? as usize;
                let xs = self.nums(10);
                let ys = self.nums(20);
                if xs.len() != n || ys.len() != n {
                    return Err(format!(
                        "LWPOLYLINE: {n} Ecken, aber {}/{} Werte",
                        xs.len(),
                        ys.len()
                    ));
                }
                Geom::Poly {
                    closed: self.one(70)? as i64 & 1 == 1,
                    pts: xs.into_iter().zip(ys).collect(),
                }
            }
            "INSERT" => Geom::Insert {
                name: decode_text(self.get(2).ok_or("INSERT ohne Namen")?),
                at: pt(10, 20, 30)?,
            },
            other => return Err(format!("Objektart {other}")),
        })
    }
}

/// Öffnungswinkel `end − start` im Bereich `(0, full]` (gleich oder kleiner null: `+ full`).
fn sweep(start: f64, end: f64, full: f64) -> f64 {
    let mut s = (end - start) % full;
    if s <= 0.0 {
        s += full;
    }
    s
}

/// Punkt der Ellipse bei Parameter `t` (Eigenform des Lesers).
fn ellipse_at(c: P, major: P, ratio: f64, t: f64) -> P {
    let (ct, st) = (t.cos(), t.sin());
    (
        c.0 + major.0 * ct - ratio * major.1 * st,
        c.1 + major.1 * ct + ratio * major.0 * st,
    )
}

/// de Boor in homogenen Koordinaten, iterativ.
pub fn spline_at(p: usize, knots: &[f64], ctrl: &[P], w: &[f64], u: f64) -> P {
    let n = ctrl.len();
    let (a, b) = (knots[p], knots[n]);
    let u = u.clamp(a, b);
    let mut k = p;
    for i in p..n {
        if knots[i] < knots[i + 1] {
            k = i;
            if u < knots[i + 1] {
                break;
            }
        }
    }
    let mut d: Vec<[f64; 3]> = (0..=p)
        .map(|j| {
            let i = k - p + j;
            [ctrl[i].0 * w[i], ctrl[i].1 * w[i], w[i]]
        })
        .collect();
    for r in 1..=p {
        for j in (r..=p).rev() {
            let i = k - p + j;
            let den = knots[i + p + 1 - r] - knots[i];
            let al = if den > 0.0 { (u - knots[i]) / den } else { 0.0 };
            let prev = d[j - 1];
            for (x, pv) in d[j].iter_mut().zip(prev) {
                *x = (1.0 - al) * pv + al * *x;
            }
        }
    }
    (d[p][0] / d[p][2], d[p][1] / d[p][2])
}

/// Abstand eines Punkts zu einer Kurve `f` über `[t0, t1]` (grob abtasten, dann goldener Schnitt).
fn dist_param(p: P, f: &dyn Fn(f64) -> P, t0: f64, t1: f64, n: usize) -> f64 {
    let at = |i: usize| t0 + (t1 - t0) * i as f64 / n as f64;
    let ds: Vec<f64> = (0..=n).map(|i| dist(p, f(at(i)))).collect();
    let mut best = f64::INFINITY;
    let g = 0.5 * (5f64.sqrt() - 1.0);
    for i in 0..=n {
        let left = if i == 0 { ds[0] } else { ds[i - 1] };
        let right = if i == n { ds[n] } else { ds[i + 1] };
        if ds[i] <= left && ds[i] <= right {
            let (mut lo, mut hi) = (at(i.saturating_sub(1)), at((i + 1).min(n)));
            for _ in 0..70 {
                let (m1, m2) = (hi - g * (hi - lo), lo + g * (hi - lo));
                if dist(p, f(m1)) < dist(p, f(m2)) {
                    hi = m2;
                } else {
                    lo = m1;
                }
            }
            best = best.min(dist(p, f(0.5 * (lo + hi))));
        }
    }
    best.min(ds.iter().copied().fold(f64::INFINITY, f64::min))
}

impl Geom {
    /// Anfangs- und Endparameter der Auswertung: Strecke `[0, 1]`, Kreis `[0, TAU]`, Bogen
    /// Winkel in Grad (`start`, `start + Öffnung`), Ellipse Parameter, Spline Knotenbereich.
    pub fn range(&self) -> (f64, f64) {
        match self {
            Geom::Line { .. } => (0.0, 1.0),
            Geom::Circle { .. } => (0.0, TAU),
            Geom::Arc { start, end, .. } => (*start, *start + sweep(*start, *end, 360.0)),
            Geom::Ellipse { start, end, .. } => (*start, *start + sweep(*start, *end, TAU)),
            Geom::Spline {
                degree,
                knots,
                ctrl,
                ..
            } => (knots[*degree], knots[ctrl.len()]),
            Geom::Poly { pts, .. } => (0.0, (pts.len() - 1) as f64),
            Geom::Insert { .. } => (0.0, 0.0),
        }
    }

    /// Punkt bei Parameter `t` im Bereich von [`Geom::range`].
    pub fn at(&self, t: f64) -> P {
        match self {
            Geom::Line { a, b } => (a.0 + t * (b.0 - a.0), a.1 + t * (b.1 - a.1)),
            Geom::Circle { c, r } => (c.0 + r * t.cos(), c.1 + r * t.sin()),
            Geom::Arc { c, r, .. } => {
                let a = t.to_radians();
                (c.0 + r * a.cos(), c.1 + r * a.sin())
            }
            Geom::Ellipse {
                c, major, ratio, ..
            } => ellipse_at(*c, *major, *ratio, t),
            Geom::Spline {
                degree,
                knots,
                ctrl,
                weights,
                ..
            } => spline_at(*degree, knots, ctrl, weights, t),
            Geom::Poly { pts, closed } => {
                let i = (t.floor() as usize).min(pts.len() - 1);
                let j = if i + 1 < pts.len() {
                    i + 1
                } else if *closed {
                    0
                } else {
                    i
                };
                let f = t - i as f64;
                (
                    pts[i].0 + f * (pts[j].0 - pts[i].0),
                    pts[i].1 + f * (pts[j].1 - pts[i].1),
                )
            }
            Geom::Insert { at, .. } => *at,
        }
    }

    /// `n + 1` gleichverteilte Punkte über den Parameterbereich.
    pub fn points(&self, n: usize) -> Vec<P> {
        let (a, b) = self.range();
        (0..=n)
            .map(|i| self.at(a + (b - a) * i as f64 / n as f64))
            .collect()
    }

    /// Länge der Kurve in mm.
    pub fn length(&self) -> f64 {
        match self {
            Geom::Line { a, b } => dist(*a, *b),
            Geom::Circle { r, .. } => TAU * r,
            Geom::Arc { r, .. } => {
                let (a, b) = self.range();
                r * (b - a).to_radians()
            }
            Geom::Ellipse { major, ratio, .. } => {
                let (a, b) = self.range();
                let n = 20_000;
                let speed = |t: f64| {
                    let (s, c) = t.sin_cos();
                    let vx = -major.0 * s - ratio * major.1 * c;
                    let vy = -major.1 * s + ratio * major.0 * c;
                    (vx * vx + vy * vy).sqrt()
                };
                let h = (b - a) / n as f64;
                let mut sum = speed(a) + speed(b);
                for i in 1..n {
                    sum += speed(a + h * i as f64) * if i % 2 == 1 { 4.0 } else { 2.0 };
                }
                sum * h / 3.0
            }
            Geom::Spline { .. } => {
                let pts = self.points(40_000);
                pts.windows(2).map(|w| dist(w[0], w[1])).sum()
            }
            Geom::Poly { pts, closed } => {
                let mut l: f64 = pts.windows(2).map(|w| dist(w[0], w[1])).sum();
                if *closed {
                    l += dist(pts[pts.len() - 1], pts[0]);
                }
                l
            }
            Geom::Insert { .. } => 0.0,
        }
    }

    /// Abstand eines Punkts zur Kurve.
    pub fn distance_to(&self, p: P) -> f64 {
        match self {
            Geom::Line { a, b } => {
                let (dx, dy) = (b.0 - a.0, b.1 - a.1);
                let l2 = dx * dx + dy * dy;
                let t = if l2 > 0.0 {
                    (((p.0 - a.0) * dx + (p.1 - a.1) * dy) / l2).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                dist(p, (a.0 + t * dx, a.1 + t * dy))
            }
            Geom::Circle { c, r } => (dist(p, *c) - r).abs(),
            Geom::Arc { c, r, .. } => {
                let (a, b) = self.range();
                // Winkel von p auf den Bereich [a, a + 360) bringen und prüfen, ob er im Bogen liegt.
                let ang = (p.1 - c.1).atan2(p.0 - c.0).to_degrees();
                let rel = (ang - a).rem_euclid(360.0);
                if rel <= b - a {
                    (dist(p, *c) - r).abs()
                } else {
                    dist(p, self.at(a)).min(dist(p, self.at(b)))
                }
            }
            Geom::Ellipse { .. } | Geom::Spline { .. } => {
                let (a, b) = self.range();
                let n = if matches!(self, Geom::Spline { .. }) {
                    600
                } else {
                    400
                };
                dist_param(p, &|t| self.at(t), a, b, n)
            }
            Geom::Poly { pts, closed } => {
                let mut best = f64::INFINITY;
                let m = pts.len();
                let edges = if *closed { m } else { m - 1 };
                for i in 0..edges {
                    let l = Geom::Line {
                        a: pts[i],
                        b: pts[(i + 1) % m],
                    };
                    best = best.min(l.distance_to(p));
                }
                best
            }
            Geom::Insert { at, .. } => dist(p, *at),
        }
    }
}
