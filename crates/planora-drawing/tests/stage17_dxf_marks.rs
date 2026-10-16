//! Stufe 17 (Teilprojekt 12): DXF-Ausgabe von Maßen und Mittellinien. Gelesen und geprüft wird
//! mit dem eigenen Leser (`common/dxf_read.rs`, prüft auch den Aufbau der Datei); die Sollwerte
//! sind von Hand aus den Eingabepunkten gerechnet (Rechenweg im Kommentar). Die goldenen Dateien
//! in `fixtures/` hat der Schreiber vor der Änderung erzeugt.

mod common;

use common::dxf_read::{self, Dxf, Geom, P, ReadEntity};
use common::mark_check::v;
use common::svg_marks::{box_sheet, cylinder_sheet, drawing, manual_sheet};
use planora_drawing::annotate::{Mark, MarkKind, ResolvedDims, TextAnchor, layout_dimension};
use planora_drawing::centerline::{CenterLine, CenterSource};
use planora_drawing::dimension::{Anchor, DimError, DimId, DimKind, DimSpec, Measured, Tolerance};
use planora_drawing::model::{Paper, Placement, Scale};
use planora_drawing::sheet::{SheetModel, SheetTransform, build_sheet_with};
use planora_drawing::stroke::{Stroke, StrokeOrigin};
use planora_drawing::{
    Basis, DrawError, Piece, ViewId, ViewKind, ViewLines, Visibility, dxf_string, write_dxf,
};
use planora_kernel::Curve;
use planora_math::{DVec2, DVec3, PI};
use std::collections::BTreeMap;

const TOL: f64 = 1e-9;
/// √2/2.
const H2: f64 = std::f64::consts::FRAC_1_SQRT_2;

// ---------------------------------------------------------------------------------------------
// Hilfen
// ---------------------------------------------------------------------------------------------

fn id_xf() -> SheetTransform {
    SheetTransform {
        scale: 1.0,
        tx: 0.0,
        ty: 0.0,
        bbox_center: DVec2::ZERO,
    }
}

fn spec(kind: DimKind, offset: f64) -> DimSpec {
    DimSpec {
        id: DimId(1),
        view: ViewId(1),
        kind,
        refs: Vec::new(),
        offset,
        angle: 45.0,
        text_shift: (0.0, 0.0),
        tolerance: Tolerance::None,
        decimals: 1,
        text_override: None,
        chain: None,
        suggested: false,
    }
}

fn lin(a: DVec2, b: DVec2, dir: DVec2) -> Anchor {
    Anchor::Linear {
        a,
        b,
        dir,
        a_span: (a, a),
        b_span: (b, b),
    }
}

fn meas(value: f64, anchor: Anchor) -> Measured {
    Measured {
        value,
        anchor,
        prefix: None,
    }
}

fn text_mark(pos: DVec2, angle: f64, anchor: TextAnchor, text: &str, kind: MarkKind) -> Mark {
    Mark::Text {
        pos,
        height: 3.5,
        angle,
        anchor,
        text: text.to_string(),
        kind,
    }
}

fn line_mark(kind: MarkKind) -> Mark {
    Mark::Line {
        a: v(0.0, 0.0),
        b: v(1.0, 1.0),
        kind,
    }
}

fn dxf_of(sheet: &SheetModel) -> String {
    dxf_string(sheet).expect("DXF")
}

fn read(sheet: &SheetModel) -> Dxf {
    let text = dxf_of(sheet);
    dxf_read::read(&text).unwrap_or_else(|e| panic!("Aufbau: {e}"))
}

fn near(a: P, b: P, tol: f64) -> bool {
    (a.0 - b.0).abs() <= tol && (a.1 - b.1).abs() <= tol
}

/// Objekte eines Blocks auf einer Ebene, nach Art.
fn ents<'a>(d: &'a Dxf, block: &str, layer: &str, kind: &str) -> Vec<&'a ReadEntity> {
    d.block(block)
        .unwrap_or_else(|| panic!("Block {block}"))
        .entities
        .iter()
        .filter(|e| e.layer == layer && e.kind == kind)
        .collect()
}

fn lines_of(d: &Dxf, block: &str, layer: &str) -> Vec<(P, P)> {
    ents(d, block, layer, "LINE")
        .iter()
        .map(|e| match e.geom().unwrap() {
            Geom::Line { a, b } => (a, b),
            g => panic!("{g:?}"),
        })
        .collect()
}

fn has_seg(list: &[(P, P)], a: P, b: P, tol: f64) -> bool {
    list.iter()
        .any(|&(x, y)| (near(x, a, tol) && near(y, b, tol)) || (near(x, b, tol) && near(y, a, tol)))
}

fn solids_of(d: &Dxf, block: &str, layer: &str) -> Vec<[P; 4]> {
    ents(d, block, layer, "SOLID")
        .iter()
        .map(|e| match e.geom().unwrap() {
            Geom::Solid { pts } => pts,
            g => panic!("{g:?}"),
        })
        .collect()
}

struct ReadText {
    pos: P,
    align_pt: P,
    height: f64,
    rotation: f64,
    halign: i64,
    text: String,
    raw: String,
    codes: Vec<i32>,
    style: String,
}

fn texts_of(d: &Dxf, block: &str, layer: &str) -> Vec<ReadText> {
    ents(d, block, layer, "TEXT")
        .iter()
        .map(|e| match e.geom().unwrap() {
            Geom::Text {
                pos,
                align_pt,
                height,
                rotation,
                halign,
                text,
                raw,
            } => ReadText {
                pos,
                align_pt,
                height,
                rotation,
                halign,
                text,
                raw,
                codes: e.codes(),
                style: e.get(7).unwrap().to_string(),
            },
            g => panic!("{g:?}"),
        })
        .collect()
}

fn table_names(d: &Dxf, table: &str) -> Vec<String> {
    d.table(table)
        .unwrap()
        .records
        .iter()
        .map(|r| r.name())
        .collect()
}

fn layer<'a>(d: &'a Dxf, name: &str) -> &'a dxf_read::Record {
    d.table("LAYER")
        .unwrap()
        .records
        .iter()
        .find(|r| r.name() == name)
        .unwrap_or_else(|| panic!("Ebene {name}"))
}

/// Die Gruppen eines Tabelleneintrags ab der Gruppe `from` (einschließlich) als (Code, Wert).
fn groups_from(r: &dxf_read::Record, from: i32) -> Vec<(i32, String)> {
    let i = r.groups.iter().position(|g| g.code == from).unwrap();
    r.groups[i..]
        .iter()
        .map(|g| (g.code, g.value.clone()))
        .collect()
}

// Blatt von Hand (wie Stufe 15/16): Vorne bei (100,100) 1:1, Rechteck 40×20 → Blatt = p + (80,90),
// eine Mittellinie (20,0)–(20,20) → Blatt (100,87)–(100,113) mit Überstand 3.

fn rect_front(w: f64, h: f64) -> ViewLines {
    let corners = [
        DVec3::new(0.0, 0.0, 0.0),
        DVec3::new(w, 0.0, 0.0),
        DVec3::new(w, 0.0, h),
        DVec3::new(0.0, 0.0, h),
    ];
    let strokes: Vec<Stroke> = (0..4)
        .map(|i| {
            let (a, b) = (corners[i], corners[(i + 1) % 4]);
            let d = b - a;
            Stroke {
                curve: Curve::Line {
                    origin: a,
                    dir: d.normalize(),
                },
                t0: 0.0,
                t1: d.length(),
                origin: StrokeOrigin::Edge { body: 0, edge: i },
            }
        })
        .collect();
    let pieces = strokes
        .iter()
        .enumerate()
        .map(|(i, s)| Piece {
            stroke: i,
            t0: s.t0,
            t1: s.t1,
            vis: Visibility::Visible,
        })
        .collect();
    ViewLines {
        kind: ViewKind::Front,
        basis: Basis::of(ViewKind::Front),
        extent: 100.0,
        strokes,
        pieces,
        bbox: ((0.0, 0.0), (w, h)),
        centerlines: vec![CenterLine {
            a: v(20.0, 0.0),
            b: v(20.0, 20.0),
            source: CenterSource::Axis { body: 0, face: 3 },
        }],
        dropped_splits: 0,
    }
}

fn front_sheet(dims: &ResolvedDims) -> SheetModel {
    let mut d = drawing(Paper::A3);
    let f = d.add_view(
        ViewKind::Front,
        Scale { num: 1, den: 1 },
        Placement::Free { x: 100.0, y: 100.0 },
        None,
    );
    assert_eq!(f, ViewId(1));
    let vf = rect_front(40.0, 20.0);
    let res: BTreeMap<ViewId, &ViewLines> = BTreeMap::from([(f, &vf)]);
    build_sheet_with(&d, &res, dims)
}

fn h_dims(offset: f64) -> ResolvedDims {
    let mut dims = ResolvedDims::default();
    // Maß (0,20)–(40,20) im Bild → Blatt (80,110)–(120,110).
    dims.by_view.insert(
        ViewId(1),
        vec![(
            spec(DimKind::Horizontal, offset),
            Ok(meas(40.0, lin(v(0.0, 20.0), v(40.0, 20.0), v(1.0, 0.0)))),
        )],
    );
    dims
}

/// Zerlegt eine Datei in (Code, Wert)-Paare und setzt sie wieder zusammen (für Verfälschungen).
fn pairs(text: &str) -> Vec<(String, String)> {
    let lines: Vec<&str> = text.trim_end_matches("\r\n").split("\r\n").collect();
    lines
        .chunks(2)
        .map(|c| (c[0].to_string(), c[1].to_string()))
        .collect()
}

fn join(p: &[(String, String)]) -> String {
    p.iter().map(|(c, v)| format!("{c}\r\n{v}\r\n")).collect()
}

fn code(p: &(String, String)) -> i32 {
    p.0.trim().parse().unwrap()
}

/// Index des ersten Paars `(0, kind)`.
fn first_obj(p: &[(String, String)], kind: &str) -> usize {
    p.iter()
        .position(|x| code(x) == 0 && x.1 == kind)
        .unwrap_or_else(|| panic!("{kind} fehlt"))
}

/// Ende (exklusiv) des Objekts, das bei `i` beginnt.
fn obj_end(p: &[(String, String)], i: usize) -> usize {
    i + 1 + p[i + 1..].iter().position(|x| code(x) == 0).unwrap()
}

// ---------------------------------------------------------------------------------------------
// (a) Alte Blätter bleiben Byte für Byte gleich
// ---------------------------------------------------------------------------------------------

// Die Fixturen hat der Schreiber vor der Änderung erzeugt (Quader mit Bohrung mit Vorne/Oben/
// Rechts, Zylinder mit Vorne/Isometrie). Ohne Maße und mit ausgeschalteten Mittellinien müssen
// beide Blätter genau dieselben Bytes ergeben: Tabellenzähler, Handles, `$HANDSEED` unverändert,
// keine neue Ebene, keine neue Linienart.
#[test]
fn old_sheets_stay_byte_for_byte() {
    let golden_box = include_bytes!("fixtures/dxf_without_marks_box.dxf");
    let golden_cyl = include_bytes!("fixtures/dxf_without_marks_cylinder.dxf");
    // Die Fixturen sind binär eingecheckt: `\r\n` bleibt erhalten.
    assert!(golden_box.windows(2).any(|w| w == b"\r\n"));
    assert!(!String::from_utf8_lossy(golden_box).contains("\n\n"));
    let b = box_sheet(false);
    let c = cylinder_sheet(false);
    assert!(b.views.iter().all(|v| v.marks.is_empty()));
    assert!(c.views.iter().all(|v| v.marks.is_empty()));
    assert_eq!(dxf_of(&b).as_bytes(), golden_box);
    assert_eq!(dxf_of(&c).as_bytes(), golden_cyl);
    let mut out = Vec::new();
    write_dxf(&b, &mut out).unwrap();
    assert_eq!(out, golden_box);
    // Umfang des Altbestands: drei bzw. zwei Ansichten, verdeckte Linien, alte Tabellen.
    let d = dxf_read::read(std::str::from_utf8(golden_box).unwrap()).unwrap();
    assert_eq!(d.view_block_names(), vec!["Vorne", "Oben", "Rechts"]);
    assert_eq!(
        table_names(&d, "LTYPE"),
        ["ByBlock", "ByLayer", "CONTINUOUS", "HIDDEN"]
    );
    assert_eq!(table_names(&d, "LAYER"), ["0", "SICHTBAR", "VERDECKT"]);
    assert!(
        d.blocks
            .iter()
            .flat_map(|b| &b.entities)
            .any(|e| e.layer == "VERDECKT")
    );
    for golden in [&golden_box[..], &golden_cyl[..]] {
        let t = std::str::from_utf8(golden).unwrap();
        for word in ["MASSE", "MITTEL", "CENTER", "TEXT", "SOLID"] {
            assert!(!t.contains(word), "{word}");
        }
    }
}

// Eine Ansicht mit nur einem Maß: nur die Ebene MASSE kommt hinzu (Zähler 4), keine Linienart
// CENTER (Zähler bleibt 4), keine Ebenen MITTEL/MASSE_FEHLT.
#[test]
fn only_used_mark_layers_are_added() {
    let mut sheet = box_sheet(false);
    sheet.views[1].marks.push(Mark::Line {
        a: v(1.0, 2.0),
        b: v(3.0, 4.0),
        kind: MarkKind::Dimension,
    });
    let d = read(&sheet);
    assert_eq!(
        table_names(&d, "LTYPE"),
        ["ByBlock", "ByLayer", "CONTINUOUS", "HIDDEN"]
    );
    assert_eq!(d.table("LTYPE").unwrap().count, 4);
    assert_eq!(
        table_names(&d, "LAYER"),
        ["0", "SICHTBAR", "VERDECKT", "MASSE"]
    );
    assert_eq!(d.table("LAYER").unwrap().count, 4);
    let l = layer(&d, "MASSE");
    assert_eq!(l.get(62), Some("7"));
    assert_eq!(l.get(6), Some("CONTINUOUS"));
    assert_eq!(l.get(370), Some("25"));
    // Gleiche Plotstil-Kette wie die alten Ebenen.
    assert_eq!(l.get(390), layer(&d, "SICHTBAR").get(390));
    let lines = lines_of(&d, "Oben", "MASSE");
    assert_eq!(lines, vec![((1.0, 2.0), (3.0, 4.0))]);
    assert!(lines_of(&d, "Vorne", "MASSE").is_empty());
    // Nur verlorene Maße: nur MASSE_FEHLT.
    let sheet = manual_sheet(Paper::A3, vec![("L", vec![line_mark(MarkKind::Lost)])]);
    let d = read(&sheet);
    assert_eq!(
        table_names(&d, "LAYER"),
        ["0", "SICHTBAR", "VERDECKT", "MASSE_FEHLT"]
    );
    assert_eq!(d.table("LTYPE").unwrap().count, 4);
}

// ---------------------------------------------------------------------------------------------
// (b) Mittellinien: Ebene MITTEL und Linienart CENTER nur, wenn sie vorkommen
// ---------------------------------------------------------------------------------------------

// Muster CENTER (Strich 12, Lücke 3, Strich 2, Lücke 3; Gesamtlänge 20 = 12 + 3 + 2 + 3).
#[test]
fn centerline_layer_and_linetype_present_only_when_used() {
    let sheet = box_sheet(true);
    let d = read(&sheet);
    let lt = d.table("LTYPE").unwrap();
    assert_eq!(lt.count, 5);
    assert_eq!(
        table_names(&d, "LTYPE"),
        ["ByBlock", "ByLayer", "CONTINUOUS", "HIDDEN", "CENTER"]
    );
    let center = &lt.records[4];
    assert_eq!(center.owner, lt.handle);
    let want: Vec<(i32, String)> = [
        (72, "65"),
        (73, "4"),
        (40, "20"),
        (49, "12"),
        (74, "0"),
        (49, "-3"),
        (74, "0"),
        (49, "2"),
        (74, "0"),
        (49, "-3"),
        (74, "0"),
    ]
    .iter()
    .map(|(c, v)| (*c, v.to_string()))
    .collect();
    assert_eq!(groups_from(center, 72), want);
    assert_eq!(d.table("LAYER").unwrap().count, 4);
    assert_eq!(
        table_names(&d, "LAYER"),
        ["0", "SICHTBAR", "VERDECKT", "MITTEL"]
    );
    let l = layer(&d, "MITTEL");
    assert_eq!(l.get(62), Some("7"));
    assert_eq!(l.get(6), Some("CENTER"));
    assert_eq!(l.get(370), Some("25"));
    assert_eq!(l.get(390), layer(&d, "SICHTBAR").get(390));
    // Jede Mittellinien-Marke steht als LINE auf MITTEL im Block ihrer Ansicht, ohne eigene
    // Farbe oder Linienart.
    let mut total = 0;
    for view in &sheet.views {
        let marks: Vec<(DVec2, DVec2)> = view
            .marks
            .iter()
            .filter_map(|m| match m {
                Mark::Line {
                    a,
                    b,
                    kind: MarkKind::CenterLine,
                } => Some((*a, *b)),
                _ => None,
            })
            .collect();
        assert_eq!(marks.len(), view.marks.len());
        let got = lines_of(&d, &view.name, "MITTEL");
        assert_eq!(got.len(), marks.len(), "{}", view.name);
        for (a, b) in &marks {
            assert!(has_seg(&got, (a.x, a.y), (b.x, b.y), 1e-8), "{a} {b}");
        }
        for e in ents(&d, &view.name, "MITTEL", "LINE") {
            assert!(e.get(62).is_none() && e.get(6).is_none());
        }
        total += marks.len();
    }
    assert!(total >= 2, "der Quader mit Bohrung hat Mittellinien");
    // Ohne Mittellinien: weder Linienart noch Ebene.
    let d = read(&box_sheet(false));
    assert_eq!(d.table("LTYPE").unwrap().count, 4);
    assert!(!table_names(&d, "LTYPE").contains(&"CENTER".to_string()));
    assert!(!table_names(&d, "LAYER").contains(&"MITTEL".to_string()));
    // Von Hand: eine senkrechte Mittellinie.
    let d = read(&front_sheet(&ResolvedDims::default()));
    assert_eq!(
        lines_of(&d, "Vorne", "MITTEL"),
        vec![((100.0, 87.0), (100.0, 113.0))]
    );
    assert_eq!(lines_of(&d, "Vorne", "SICHTBAR").len(), 4);
}

// ---------------------------------------------------------------------------------------------
// (c) Maße: LINE, SOLID, TEXT, ARC an den erwarteten Stellen
// ---------------------------------------------------------------------------------------------

// Waagerechtes Maß über den ganzen Weg Bild → Blatt → DXF → Leser. Blatt: Maßlinie
// y = 110 + 10 = 120; Hilfslinien von 110 + 1 bis 120 + 2 bei x = 80 und 120; Pfeile innen bei
// (80,120) und (120,120), Fuß 3,5 weiter innen; Text „40,0“ mittig bei (100, 121), Höhe 3,5,
// Drehung 0, Ausrichtung 1 (Mitte) mit Ausrichtepunkt = Einfügepunkt.
#[test]
fn dimension_entities_roundtrip() {
    let sheet = front_sheet(&h_dims(10.0));
    let d = read(&sheet);
    assert_eq!(
        table_names(&d, "LAYER"),
        ["0", "SICHTBAR", "VERDECKT", "MASSE", "MITTEL"]
    );
    let lines = lines_of(&d, "Vorne", "MASSE");
    assert_eq!(lines.len(), 3, "{lines:?}");
    assert!(has_seg(&lines, (80.0, 120.0), (120.0, 120.0), TOL));
    assert!(has_seg(&lines, (80.0, 111.0), (80.0, 122.0), TOL));
    assert!(has_seg(&lines, (120.0, 111.0), (120.0, 122.0), TOL));
    let solids = solids_of(&d, "Vorne", "MASSE");
    assert_eq!(solids.len(), 2);
    for (tip, base_x) in [((80.0, 120.0), 83.5), ((120.0, 120.0), 116.5)] {
        let s = solids
            .iter()
            .find(|s| near(s[0], tip, TOL))
            .unwrap_or_else(|| panic!("Pfeil {tip:?}"));
        for p in &s[1..] {
            assert!((p.0 - base_x).abs() < TOL, "{p:?}");
            assert!((p.1 - 120.0).abs() - 0.6 < TOL);
        }
    }
    for e in ents(&d, "Vorne", "MASSE", "SOLID") {
        assert_eq!(
            e.codes(),
            [
                5, 330, 100, 8, 100, 10, 20, 30, 11, 21, 31, 12, 22, 32, 13, 23, 33, 39
            ]
        );
        assert_eq!(e.get(100), Some("AcDbEntity"));
        assert_eq!(e.get(39), Some("0"));
    }
    let ts = texts_of(&d, "Vorne", "MASSE");
    assert_eq!(ts.len(), 1);
    let t = &ts[0];
    assert_eq!(t.text, "40,0");
    assert_eq!(t.raw, "40,0");
    assert!(near(t.pos, (100.0, 121.0), TOL), "{:?}", t.pos);
    assert!(near(t.align_pt, (100.0, 121.0), TOL));
    assert_eq!(t.height, 3.5);
    assert_eq!(t.rotation, 0.0);
    assert_eq!(t.halign, 1);
    assert_eq!(t.style, "STANDARD");
    assert_eq!(
        t.codes,
        [
            5, 330, 100, 8, 100, 10, 20, 30, 40, 1, 50, 7, 72, 11, 21, 31, 100, 73
        ]
    );
    // Die Maße stehen im Block der Ansicht, nicht im Modellraum.
    assert!(d.entities.iter().all(|e| e.layer != "MASSE"));
    assert_eq!(lines_of(&d, "Vorne", "MITTEL").len(), 1);

    // Senkrechtes Maß (40,30)–(40,50), Abstand 10: Text „20,0“ bei (49, 40), Drehung 90°.
    let marks = layout_dimension(
        &spec(DimKind::Vertical, 10.0),
        &meas(20.0, lin(v(40.0, 30.0), v(40.0, 50.0), v(0.0, 1.0))),
        &id_xf(),
        0,
    );
    let d = read(&manual_sheet(Paper::A3, vec![("V", marks)]));
    let t = &texts_of(&d, "V", "MASSE")[0];
    assert_eq!(t.text, "20,0");
    assert!(near(t.pos, (49.0, 40.0), TOL), "{:?}", t.pos);
    assert!((t.rotation - 90.0).abs() < TOL);
    assert!(has_seg(
        &lines_of(&d, "V", "MASSE"),
        (50.0, 30.0),
        (50.0, 50.0),
        TOL
    ));

    // Winkelmaß: Scheitel (10,10), Abstand 15, 90° zwischen +x und +y → ARC um (10,10), Radius 15,
    // von 0° bis 90° (gegen den Uhrzeigersinn); Pfeile (25,10) und (10,25); Text „90,0°“ bei
    // (10 + 16·√2/2, …) mit Drehung −45° = 315°.
    let marks = layout_dimension(
        &spec(DimKind::Angle, 15.0),
        &meas(
            90.0,
            Anchor::Angle {
                vertex: v(10.0, 10.0),
                d1: v(1.0, 0.0),
                d2: v(0.0, 1.0),
            },
        ),
        &id_xf(),
        0,
    );
    let d = read(&manual_sheet(Paper::A3, vec![("W", marks)]));
    let arcs = ents(&d, "W", "MASSE", "ARC");
    assert_eq!(arcs.len(), 1);
    match arcs[0].geom().unwrap() {
        Geom::Arc { c, r, start, end } => {
            assert!(near(c, (10.0, 10.0), TOL));
            assert!((r - 15.0).abs() < TOL);
            assert!(start.abs() < TOL, "{start}");
            assert!((end - 90.0).abs() < TOL, "{end}");
        }
        g => panic!("{g:?}"),
    }
    let solids = solids_of(&d, "W", "MASSE");
    assert_eq!(solids.len(), 2);
    for (tip, base) in [((25.0, 10.0), (25.0, 13.5)), ((10.0, 25.0), (13.5, 25.0))] {
        let s = solids.iter().find(|s| near(s[0], tip, TOL)).expect("Pfeil");
        let mid = ((s[1].0 + s[2].0) / 2.0, (s[1].1 + s[2].1) / 2.0);
        assert!(near(mid, base, TOL), "{mid:?} {base:?}");
    }
    let t = &texts_of(&d, "W", "MASSE")[0];
    assert_eq!(t.text, "90,0°");
    assert_eq!(t.raw, "90,0%%d");
    assert!((t.rotation - 315.0).abs() < TOL, "{}", t.rotation);
    assert!(near(t.pos, (10.0 + 16.0 * H2, 10.0 + 16.0 * H2), 1e-8));

    // Ausrichtung je Anker: Start 0, Mitte 1, Ende 2; Drehung im Bereich [0, 360).
    let sheet = manual_sheet(
        Paper::A3,
        vec![(
            "A",
            vec![
                text_mark(
                    v(10.0, 20.0),
                    -PI / 2.0,
                    TextAnchor::Start,
                    "s",
                    MarkKind::Dimension,
                ),
                text_mark(
                    v(30.0, 40.0),
                    PI,
                    TextAnchor::Middle,
                    "m",
                    MarkKind::Dimension,
                ),
                text_mark(
                    v(50.0, 60.0),
                    2.0 * PI,
                    TextAnchor::End,
                    "e",
                    MarkKind::Dimension,
                ),
            ],
        )],
    );
    let d = read(&sheet);
    let ts = texts_of(&d, "A", "MASSE");
    let got: Vec<(String, i64, f64, P, P)> = ts
        .iter()
        .map(|t| (t.text.clone(), t.halign, t.rotation, t.pos, t.align_pt))
        .collect();
    assert_eq!(
        got,
        vec![
            ("s".to_string(), 0, 270.0, (10.0, 20.0), (10.0, 20.0)),
            ("m".to_string(), 1, 180.0, (30.0, 40.0), (30.0, 40.0)),
            ("e".to_string(), 2, 0.0, (50.0, 60.0), (50.0, 60.0)),
        ]
    );
    // Bogen über 90° und ganzer Kreis: 30° bis 280° bleibt ein ARC (30 → 280), der ganze Kreis
    // wird wie bei den Linien ein CIRCLE.
    let sheet = manual_sheet(
        Paper::A3,
        vec![(
            "B",
            vec![
                Mark::Arc {
                    center: v(100.0, 100.0),
                    r: 20.0,
                    start: PI / 6.0,
                    end: PI / 6.0 + 250f64.to_radians(),
                    kind: MarkKind::Dimension,
                },
                Mark::Arc {
                    center: v(200.0, 100.0),
                    r: 5.0,
                    start: 0.0,
                    end: 2.0 * PI,
                    kind: MarkKind::Dimension,
                },
            ],
        )],
    );
    let d = read(&sheet);
    match ents(&d, "B", "MASSE", "ARC")[0].geom().unwrap() {
        Geom::Arc { start, end, .. } => {
            assert!((start - 30.0).abs() < 1e-8 && (end - 280.0).abs() < 1e-8);
        }
        g => panic!("{g:?}"),
    }
    match ents(&d, "B", "MASSE", "CIRCLE")[0].geom().unwrap() {
        Geom::Circle { c, r } => assert!(near(c, (200.0, 100.0), TOL) && r == 5.0),
        g => panic!("{g:?}"),
    }
}

// ---------------------------------------------------------------------------------------------
// (d) Reihenfolge der SOLID-Ecken
// ---------------------------------------------------------------------------------------------

// DXF zeichnet ein SOLID über die Ecken 1, 2, 4, 3. Ein Dreieck hat deshalb gleiche dritte und
// vierte Ecke. Pfeil Spitze (50,60), Richtung (−1,0): Fuß (46,5; 60), Ecken (46,5; 60 ± 0,6);
// Fläche ½ · 1,2 · 3,5 = 2,1. Schräg (0,6; 0,8): Fußmitte = Spitze + 3,5 · Richtung.
#[test]
fn solid_triangle_vertex_order() {
    let sheet = manual_sheet(
        Paper::A3,
        vec![(
            "P",
            vec![
                Mark::Arrow {
                    tip: v(50.0, 60.0),
                    dir: v(-1.0, 0.0),
                    kind: MarkKind::Dimension,
                },
                Mark::Arrow {
                    tip: v(100.0, 100.0),
                    dir: v(0.6, 0.8),
                    kind: MarkKind::Dimension,
                },
            ],
        )],
    );
    let d = read(&sheet);
    let solids = solids_of(&d, "P", "MASSE");
    assert_eq!(solids.len(), 2);
    let area = |o: [P; 4]| {
        let mut s = 0.0;
        for i in 0..4 {
            let (a, b) = (o[i], o[(i + 1) % 4]);
            s += a.0 * b.1 - b.0 * a.1;
        }
        0.5 * s.abs()
    };
    for (s, tip, dir) in [
        (solids[0], (50.0, 60.0), (-1.0, 0.0)),
        (solids[1], (100.0, 100.0), (0.6, 0.8)),
    ] {
        assert!(near(s[0], tip, TOL), "Ecke 1 ist die Spitze: {s:?}");
        assert_eq!(s[2], s[3], "dritte und vierte Ecke gleich: {s:?}");
        let base = (tip.0 + 3.5 * dir.0, tip.1 + 3.5 * dir.1);
        let mid = ((s[1].0 + s[2].0) / 2.0, (s[1].1 + s[2].1) / 2.0);
        assert!(near(mid, base, TOL), "{mid:?}");
        let (wx, wy) = (s[1].0 - s[2].0, s[1].1 - s[2].1);
        assert!((wx.hypot(wy) - 1.2).abs() < TOL, "Breite");
        assert!((wx * dir.0 + wy * dir.1).abs() < TOL);
        // Umriss in Zeichenfolge 1, 2, 4, 3 = Dreieck mit der vollen Fläche.
        let outline = [s[0], s[1], s[3], s[2]];
        assert!((area(outline) - 2.1).abs() < 1e-9, "{}", area(outline));
    }
    let s = solids[0];
    assert!(has_seg(&[(s[1], s[2])], (46.5, 59.4), (46.5, 60.6), TOL));
}

// ---------------------------------------------------------------------------------------------
// (e) Zeichenkürzel im Text
// ---------------------------------------------------------------------------------------------

#[test]
fn text_code_conversion() {
    let cases = [
        ("Ø10,0", "%%c10,0", "Ø10,0"),
        ("90,0°", "90,0%%d", "90,0°"),
        ("12,5 ±0,1", "12,5 %%p0,1", "12,5 ±0,1"),
        ("−0,5", "-0,5", "-0,5"),
        ("R5,0", "R5,0", "R5,0"),
        (
            "Ø10 H7 ±0,05 45°",
            "%%c10 H7 %%p0,05 45%%d",
            "Ø10 H7 ±0,05 45°",
        ),
        // Übrige Zeichen außerhalb von ASCII als \U+XXXX (Datei bleibt ASCII).
        ("Maß ä", "Ma\\U+00DF \\U+00E4", "Maß ä"),
        ("≈ 5 €", "\\U+2248 5 \\U+20AC", "≈ 5 €"),
        // Steuerzeichen werden zu Leerzeichen, das Dach als `^ ` (sonst Steuerzeichen-Kürzel).
        ("a\tb\nc\u{1}", "a b c ", "a b c "),
        ("x^J", "x^ J", "x^J"),
        // Außerhalb der Grundebene: Fragezeichen.
        ("a\u{1F600}b", "a?b", "a?b"),
    ];
    for (input, raw, decoded) in cases {
        let sheet = manual_sheet(
            Paper::A3,
            vec![(
                "T",
                vec![text_mark(
                    v(10.0, 10.0),
                    0.0,
                    TextAnchor::Start,
                    input,
                    MarkKind::Dimension,
                )],
            )],
        );
        let text = dxf_of(&sheet);
        assert!(text.is_ascii());
        let d = dxf_read::read(&text).unwrap();
        let t = &texts_of(&d, "T", "MASSE")[0];
        assert_eq!(t.raw, raw, "{input:?}");
        assert_eq!(t.text, decoded, "{input:?}");
    }
}

/// Eigene Texte mit `%` und `\` bilden keine Kürzel: „50%%c“ bleibt „50%%c“ (kein Ø), „\U+00D8“
/// bleibt die Zeichenfolge selbst. `%` steht als `%%%` in der Datei (nicht als `\U+0025`: das
/// lösen CAD-Programme schon beim Laden auf, die `%%`-Kürzel erst beim Anzeigen; der Leser
/// entschlüsselt in dieser Reihenfolge), `\` als `\U+005C`. Texte ohne diese Zeichen bleiben wie
/// vorher (siehe `text_code_conversion` und die goldenen Dateien).
#[test]
fn percent_and_backslash_in_own_text_are_masked() {
    let cases = [
        ("50%", "50%%%", "50%"),
        ("50%%c", "50%%%%%%c", "50%%c"),
        ("5%Ø", "5%%%%%c", "5%Ø"),
        ("%%d%%p", "%%%%%%d%%%%%%p", "%%d%%p"),
        ("%%%", "%%%%%%%%%", "%%%"),
        ("a\\U+00D8", "a\\U+005CU+00D8", "a\\U+00D8"),
        ("C:\\x", "C:\\U+005Cx", "C:\\x"),
        ("Ø5 %", "%%c5 %%%", "Ø5 %"),
        ("\\%%c", "\\U+005C%%%%%%c", "\\%%c"),
    ];
    for (input, raw, decoded) in cases {
        let sheet = manual_sheet(
            Paper::A3,
            vec![(
                "T",
                vec![text_mark(
                    v(10.0, 10.0),
                    0.0,
                    TextAnchor::Start,
                    input,
                    MarkKind::Dimension,
                )],
            )],
        );
        let text = dxf_of(&sheet);
        assert!(text.is_ascii());
        let d = dxf_read::read(&text).unwrap();
        let t = &texts_of(&d, "T", "MASSE")[0];
        assert_eq!(t.raw, raw, "{input:?}");
        assert_eq!(t.text, decoded, "{input:?}");
    }
    // Die Länge zählt in der Schreibweise der Datei: 85 · 3 = 255 passt, 86 · 3 = 258 nicht;
    // `\` braucht 7 Zeichen: 36 · 7 = 252 passt, 37 · 7 = 259 nicht.
    assert!(planora_drawing::dxf::dxf_text_fits(&"%".repeat(85)));
    assert!(!planora_drawing::dxf::dxf_text_fits(&"%".repeat(86)));
    assert!(planora_drawing::dxf::dxf_text_fits(&"\\".repeat(36)));
    assert!(!planora_drawing::dxf::dxf_text_fits(&"\\".repeat(37)));
    assert!(planora_drawing::dxf::dxf_text_fits(&"a".repeat(255)));
    assert!(!planora_drawing::dxf::dxf_text_fits(&"a".repeat(256)));
}

// ---------------------------------------------------------------------------------------------
// (f) Verlorene Maße auf einer roten Ebene
// ---------------------------------------------------------------------------------------------

// Verloren: letzte Lage (0,0)–(40,0) → Blatt y = 90, Abstand −10 → Linie y = 80, rot auf
// MASSE_FEHLT (Farbe 1, durchgezogen); das gute Maß bleibt auf MASSE.
#[test]
fn lost_marks_go_to_a_red_layer() {
    let mut dims = h_dims(10.0);
    let mut s = spec(DimKind::Horizontal, -10.0);
    s.id = DimId(2);
    dims.by_view
        .get_mut(&ViewId(1))
        .unwrap()
        .push((s, Err(DimError::Lost("Kante 3".into()))));
    dims.last_known
        .insert(DimId(2), lin(v(0.0, 0.0), v(40.0, 0.0), v(1.0, 0.0)));
    let d = read(&front_sheet(&dims));
    assert_eq!(
        table_names(&d, "LAYER"),
        [
            "0",
            "SICHTBAR",
            "VERDECKT",
            "MASSE",
            "MITTEL",
            "MASSE_FEHLT"
        ]
    );
    let red = layer(&d, "MASSE_FEHLT");
    assert_eq!(red.get(62), Some("1"));
    assert_eq!(red.get(6), Some("CONTINUOUS"));
    assert_eq!(red.get(370), Some("25"));
    assert_eq!(red.get(390), layer(&d, "SICHTBAR").get(390));
    let lost = lines_of(&d, "Vorne", "MASSE_FEHLT");
    assert_eq!(lost.len(), 3);
    assert!(has_seg(&lost, (80.0, 80.0), (120.0, 80.0), TOL));
    assert_eq!(solids_of(&d, "Vorne", "MASSE_FEHLT").len(), 2);
    let ts = texts_of(&d, "Vorne", "MASSE_FEHLT");
    assert_eq!(ts.len(), 1);
    assert!(ts[0].text.starts_with("? "), "{}", ts[0].text);
    assert_eq!(lines_of(&d, "Vorne", "MASSE").len(), 3);
    assert_eq!(solids_of(&d, "Vorne", "MASSE").len(), 2);
    assert_eq!(texts_of(&d, "Vorne", "MASSE").len(), 1);
    // Objekte tragen keine eigene Farbe; rot kommt von der Ebene.
    for e in &d.block("Vorne").unwrap().entities {
        assert!(e.get(62).is_none(), "{e:?}");
    }
}

// ---------------------------------------------------------------------------------------------
// (g) Der Prüfer erkennt kaputte Strukturen
// ---------------------------------------------------------------------------------------------

#[test]
fn validator_rejects_corrupted_mark_structures() {
    let sheet = manual_sheet(
        Paper::A3,
        vec![(
            "K",
            vec![
                Mark::Line {
                    a: v(10.0, 10.0),
                    b: v(50.0, 10.0),
                    kind: MarkKind::CenterLine,
                },
                Mark::Arrow {
                    tip: v(10.0, 20.0),
                    dir: v(1.0, 0.0),
                    kind: MarkKind::Dimension,
                },
                text_mark(
                    v(30.0, 21.0),
                    0.0,
                    TextAnchor::Middle,
                    "40,0",
                    MarkKind::Dimension,
                ),
            ],
        )],
    );
    let good = dxf_of(&sheet);
    dxf_read::read(&good).expect("unverändert gültig");
    let p = pairs(&good);
    assert_eq!(join(&p), good);
    let expect_err = |p: &[(String, String)], what: &str, needle: &str| {
        let e = dxf_read::read(&join(p)).expect_err(what);
        assert!(e.contains(needle), "{what}: {e}");
    };

    // TEXT ohne Ebene.
    let mut q = p.clone();
    let t = first_obj(&q, "TEXT");
    let i = t + q[t..].iter().position(|x| code(x) == 8).unwrap();
    q.remove(i);
    expect_err(&q, "TEXT ohne Ebene", "Ebene");

    // TEXT mit unbekanntem Stil.
    let mut q = p.clone();
    let t = first_obj(&q, "TEXT");
    let i = t + q[t..].iter().position(|x| code(x) == 7).unwrap();
    q[i].1 = "FREMD".into();
    expect_err(&q, "TEXT mit fremdem Stil", "Stil");

    // SOLID mit drei Ecken (13/23/33 fehlen).
    let mut q = p.clone();
    let s = first_obj(&q, "SOLID");
    let end = obj_end(&q, s);
    let removed: Vec<usize> = (s..end)
        .filter(|&i| [13, 23, 33].contains(&code(&q[i])))
        .collect();
    assert_eq!(removed.len(), 3);
    for i in removed.into_iter().rev() {
        q.remove(i);
    }
    expect_err(&q, "SOLID mit drei Ecken", "kommt 0-mal vor");

    // CENTER ohne Muster (73 = 0, Längen und Arten entfernt, Gesamtlänge 0).
    let mut q = p.clone();
    let c = q
        .iter()
        .position(|x| code(x) == 2 && x.1 == "CENTER")
        .unwrap();
    let end = obj_end(&q, c);
    let mut i = c;
    let mut stop = end;
    while i < stop {
        match code(&q[i]) {
            49 | 74 => {
                q.remove(i);
                stop -= 1;
            }
            73 => {
                q[i].1 = "0".into();
                i += 1;
            }
            40 => {
                q[i].1 = "0".into();
                i += 1;
            }
            _ => i += 1,
        }
    }
    expect_err(&q, "CENTER ohne Muster", "CENTER ohne Muster");

    // CENTER mit einer fehlenden Länge.
    let mut q = p.clone();
    let c = q
        .iter()
        .position(|x| code(x) == 2 && x.1 == "CENTER")
        .unwrap();
    let i = c + q[c..].iter().position(|x| code(x) == 49).unwrap();
    q.remove(i);
    expect_err(&q, "CENTER mit drei Längen", "Elemente");

    // Linienart CENTER fehlt, die Ebene MITTEL nennt sie trotzdem.
    let mut q = p.clone();
    let c = q
        .iter()
        .position(|x| code(x) == 2 && x.1 == "CENTER")
        .unwrap();
    q[c].1 = "CENTRE".into();
    expect_err(&q, "Ebene mit fehlender Linienart", "Linienart CENTER");

    // Doppelte Handles: TEXT bekommt das Handle des SOLID.
    let mut q = p.clone();
    let s = first_obj(&q, "SOLID");
    let t = first_obj(&q, "TEXT");
    let hs = q[s + 1].clone();
    assert_eq!(code(&hs), 5);
    assert_eq!(code(&q[t + 1]), 5);
    q[t + 1].1 = hs.1;
    expect_err(&q, "doppelte Handles", "doppelt");

    // TEXT mit nur einer Marke AcDbText.
    let mut q = p.clone();
    let t = first_obj(&q, "TEXT");
    let end = obj_end(&q, t);
    let last = (t..end)
        .rev()
        .find(|&i| code(&q[i]) == 100 && q[i].1 == "AcDbText")
        .unwrap();
    q.remove(last);
    expect_err(&q, "TEXT ohne zweite Marke", "Marken");
}

// ---------------------------------------------------------------------------------------------
// (h) Zu lange Texte und nicht endliche Zahlen
// ---------------------------------------------------------------------------------------------

#[test]
fn very_long_text_is_refused_by_name() {
    let one = |s: String| {
        manual_sheet(
            Paper::A3,
            vec![
                ("Gut", vec![line_mark(MarkKind::Dimension)]),
                (
                    "T",
                    vec![text_mark(
                        v(0.0, 0.0),
                        0.0,
                        TextAnchor::Start,
                        &s,
                        MarkKind::Dimension,
                    )],
                ),
            ],
        )
    };
    // 255 Zeichen gehen, 256 nicht; gezählt wird in der Datei (Ø = „%%c“ = 3 Zeichen).
    for (s, ok) in [
        ("a".repeat(255), true),
        ("a".repeat(256), false),
        ("Ø".repeat(85), true),
        ("Ø".repeat(86), false),
        ("ä".repeat(36), true),
        ("ä".repeat(37), false),
    ] {
        let sheet = one(s.clone());
        let r = dxf_string(&sheet);
        if ok {
            let d = dxf_read::read(&r.unwrap()).unwrap();
            assert_eq!(texts_of(&d, "T", "MASSE")[0].text, s);
        } else {
            match r {
                Err(DrawError::Unsupported(m)) => assert_eq!(m, "Der Maßtext ist zu lang."),
                other => panic!("{other:?}"),
            }
            let mut out = Vec::new();
            assert!(write_dxf(&sheet, &mut out).is_err());
            assert!(out.is_empty(), "bei einem Fehler wird nichts geschrieben");
        }
    }
    // Die Meldung ist deutsch und benennt den Grund.
    let e = dxf_string(&one("x".repeat(300))).unwrap_err();
    assert!(e.to_string().contains("Der Maßtext ist zu lang."), "{e}");
}

#[test]
fn non_finite_marks_refuse_with_nonfinite_error() {
    let bad = [
        Mark::Line {
            a: v(f64::NAN, 0.0),
            b: v(1.0, 1.0),
            kind: MarkKind::Dimension,
        },
        Mark::Line {
            a: v(0.0, 0.0),
            b: v(1.0, f64::INFINITY),
            kind: MarkKind::CenterLine,
        },
        Mark::Arrow {
            tip: v(0.0, f64::NAN),
            dir: v(1.0, 0.0),
            kind: MarkKind::Dimension,
        },
        Mark::Arrow {
            tip: v(0.0, 0.0),
            dir: v(f64::INFINITY, 0.0),
            kind: MarkKind::Lost,
        },
        Mark::Arc {
            center: v(0.0, 0.0),
            r: f64::NAN,
            start: 0.0,
            end: 1.0,
            kind: MarkKind::Dimension,
        },
        Mark::Arc {
            center: v(0.0, 0.0),
            r: 1.0,
            start: 0.0,
            end: f64::NEG_INFINITY,
            kind: MarkKind::Dimension,
        },
        text_mark(
            v(f64::NAN, 0.0),
            0.0,
            TextAnchor::Start,
            "x",
            MarkKind::Dimension,
        ),
        text_mark(
            v(0.0, 0.0),
            f64::INFINITY,
            TextAnchor::Start,
            "x",
            MarkKind::Dimension,
        ),
        Mark::Text {
            pos: v(0.0, 0.0),
            height: f64::NAN,
            angle: 0.0,
            anchor: TextAnchor::Start,
            text: "x".into(),
            kind: MarkKind::Dimension,
        },
    ];
    for m in bad {
        let sheet = manual_sheet(
            Paper::A3,
            vec![
                ("Gut", vec![line_mark(MarkKind::Dimension)]),
                ("Schlecht", vec![m.clone()]),
            ],
        );
        assert!(
            matches!(dxf_string(&sheet), Err(DrawError::NonFinite)),
            "{m:?}"
        );
        let mut out = Vec::new();
        assert!(matches!(
            write_dxf(&sheet, &mut out),
            Err(DrawError::NonFinite)
        ));
        assert!(out.is_empty());
    }
}

// ---------------------------------------------------------------------------------------------
// (i) A0-Blatt und weit entfernte Koordinaten
// ---------------------------------------------------------------------------------------------

// Zahlen ohne Exponent, höchstens neun Nachkommastellen; Werte bis ±1e5 kommen unverändert
// zurück, auch weit außerhalb des Blatts.
#[test]
fn a0_and_far_coordinates() {
    let sheet = manual_sheet(
        Paper::A0,
        vec![(
            "Groß",
            vec![
                Mark::Line {
                    a: v(1100.5, 800.25),
                    b: v(99999.5, -99999.5),
                    kind: MarkKind::Dimension,
                },
                Mark::Line {
                    a: v(1e-12, 0.1 + 0.2),
                    b: v(-1e-12, 1.23456),
                    kind: MarkKind::CenterLine,
                },
                text_mark(
                    v(-99999.123456789, 99999.0),
                    0.0,
                    TextAnchor::End,
                    "99999,5",
                    MarkKind::Dimension,
                ),
                Mark::Arrow {
                    tip: v(1189.0, 841.0),
                    dir: v(0.0, -1.0),
                    kind: MarkKind::Lost,
                },
                Mark::Arc {
                    center: v(100000.0, -100000.0),
                    r: 0.5,
                    start: 0.0,
                    end: 1.0,
                    kind: MarkKind::Dimension,
                },
            ],
        )],
    );
    let text = dxf_of(&sheet);
    // Kein Wert, der als Zahl lesbar ist, hat einen Exponenten (Wörter wie „Model_Space“ zählen
    // nicht).
    for l in text.split("\r\n") {
        if l.trim().parse::<f64>().is_ok() {
            assert!(!l.contains(['e', 'E', 'n', 'N']), "{l}");
        }
    }
    let d = dxf_read::read(&text).unwrap();
    let ext = d.header_var("$EXTMAX").unwrap();
    assert_eq!(ext[0].value, "1189");
    assert_eq!(ext[1].value, "841");
    let l = lines_of(&d, "Groß", "MASSE");
    assert_eq!(l, vec![((1100.5, 800.25), (99999.5, -99999.5))]);
    // 0,1 + 0,2 auf neun Stellen gerundet: 0.3; ±1e-12 wird 0 (nicht „-0“).
    assert_eq!(
        lines_of(&d, "Groß", "MITTEL"),
        vec![((0.0, 0.3), (0.0, 1.23456))]
    );
    let t = &texts_of(&d, "Groß", "MASSE")[0];
    assert_eq!(t.pos, (-99999.123456789, 99999.0));
    assert_eq!(t.halign, 2);
    let s = solids_of(&d, "Groß", "MASSE_FEHLT")[0];
    assert_eq!(s[0], (1189.0, 841.0));
    assert!((s[1].1 - 837.5).abs() < TOL && s[2] == s[3]);
    match ents(&d, "Groß", "MASSE", "ARC")[0].geom().unwrap() {
        Geom::Arc { c, r, start, end } => {
            assert_eq!(c, (100000.0, -100000.0));
            assert_eq!(r, 0.5);
            assert_eq!(start, 0.0);
            assert!((end - 1f64.to_degrees()).abs() < 1e-9);
        }
        g => panic!("{g:?}"),
    }
}

// ---------------------------------------------------------------------------------------------
// (j) Doku-Kommentar nennt den Abschnitt zu Maßen und Mittellinien
// ---------------------------------------------------------------------------------------------

// Leichtes Wartungsnetz: der Modulkommentar von `dxf.rs` beschreibt die neuen Objekte, Ebenen
// und die Linienart (die Liste für den Handtest in Fusion 360 steht dort).
#[test]
fn hand_check_list_documented() {
    let src = include_str!("../src/dxf.rs");
    let doc: String = src
        .lines()
        .filter(|l| l.starts_with("//!"))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(doc.contains("# Maße und Mittellinien"), "Abschnitt fehlt");
    for word in [
        "`TEXT`",
        "`SOLID`",
        "`CENTER`",
        "`MASSE`",
        "`MITTEL`",
        "`MASSE_FEHLT`",
        "%%c",
        "%%d",
        "%%p",
        "AcDbTrace",
        "Der Maßtext ist zu lang.",
        "Handtest",
    ] {
        assert!(doc.contains(word), "{word} fehlt im Modulkommentar");
    }
}
