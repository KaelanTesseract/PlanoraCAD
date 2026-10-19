//! Stufe 34: Schraffur, Schnittlinie und Titel in SVG, DXF und PDF; drei goldene Dateien.
//!
//! Golden-Dateien neu erzeugen (nur bei einer gewollten Änderung der Schreiber, im Commit
//! begründen): `PLANORA_UPDATE_SECTION_GOLDEN=1 cargo test -p planora-drawing --test
//! stage34_section_output golden_files`. Der Test schreibt dann die Dateien und schlägt
//! trotzdem fehl; danach ohne die Variable erneut laufen lassen.

mod common;

use common::pdf_read::{self, Item, mm};
use common::svg_read::{self, ReadSvg};
use common::{dxf_read, svg_read::ReadPath};
use planora_drawing::annotate::{Mark, MarkKind, SectionPen};
use planora_drawing::section::{SectionPlane, compute_section};
use planora_drawing::section_marks::{add_section_marks, section_marks};
use planora_drawing::sheet::{SheetModel, build_sheet};
use planora_drawing::sheet_frame::sheet_scale;
use planora_drawing::{
    DrawBody, Drawing, DrawingId, Paper, PdfMeta, Placement, Projection, Scale, Sheet, ViewId,
    ViewKind, ViewLines, compute_view, dxf_string, pdf_bytes, svg_string,
};
use planora_exchange::Progress;
use planora_kernel::{AnalyticKernel, BoolOp, Kernel, Solid};
use planora_math::{DMat4, DVec3};
use std::collections::BTreeMap;
use std::path::PathBuf;

const TITLE: &str = "A\u{2013}A (1:1)";

fn k() -> AnalyticKernel {
    AnalyticKernel::default()
}

/// Rohr: außen r 20, innen r 8, Höhe 40, Achse z.
fn tube() -> Solid {
    let outer = k().make_cylinder(20.0, 40.0).unwrap();
    let inner = k()
        .make_cylinder(8.0, 44.0)
        .unwrap()
        .transformed(&DMat4::from_translation(DVec3::new(0.0, 0.0, -2.0)))
        .unwrap();
    let mut s = k().boolean(&outer, &inner, BoolOp::Difference).unwrap();
    s.assign_origin(7);
    s
}

/// Blatt mit Vorne (frei bei (100, 120), 1:1) und, wenn `section`, Schnitt A–A (Oben, Versatz
/// 20, abhängig); Mittellinien an, ohne Rahmen. Ohne Schnitt kommt eine gewöhnliche Ansicht
/// „Oben“ an dieselbe Stelle.
fn sheet_on(paper: Paper, section: bool, hatch: bool) -> SheetModel {
    let solid = tube();
    let bodies = [DrawBody {
        name: "Rohr".into(),
        solid: &solid,
    }];
    let mut d = Drawing::new(
        DrawingId(1),
        "T".into(),
        Sheet {
            paper,
            landscape: true,
        },
        Projection::First,
    );
    let one = Scale { num: 1, den: 1 };
    let front = d.add_view(
        ViewKind::Front,
        one,
        Placement::Free { x: 100.0, y: 120.0 },
        None,
    );
    let p = Progress::new();
    let front_lines = compute_view(&bodies, ViewKind::Front, &p).unwrap();
    let mut results: BTreeMap<ViewId, &ViewLines> = BTreeMap::from([(front, &front_lines)]);
    let plane = SectionPlane {
        kind: ViewKind::Top,
        offset: 20.0,
    };
    let sec_lines;
    let top_lines;
    let mut sec_id = None;
    if section {
        let sec = d.add_section(front, ViewKind::Top, 20.0, 70.0).unwrap();
        d.views
            .iter_mut()
            .find(|v| v.id == sec)
            .unwrap()
            .section
            .as_mut()
            .unwrap()
            .hatch
            .on = hatch;
        sec_lines = compute_section(&bodies, &plane, compute_view, &p).unwrap();
        results.insert(sec, &sec_lines.lines);
        sec_id = Some((sec, &sec_lines.data));
    } else {
        let top = d.add_view(
            ViewKind::Top,
            one,
            Placement::Aligned { distance: 70.0 },
            Some(front),
        );
        top_lines = compute_view(&bodies, ViewKind::Top, &p).unwrap();
        results.insert(top, &top_lines);
    }
    let mut model = build_sheet(&d, &results);
    if let Some((sec, data)) = sec_id {
        let sections = BTreeMap::from([(sec, data)]);
        let (list, errs) = section_marks(&d, &results, &sections);
        assert!(errs.is_empty(), "{errs:?}");
        add_section_marks(&mut model, &list);
    }
    model
}

fn marks_of(m: &SheetModel, name: &str) -> Vec<Mark> {
    m.views
        .iter()
        .find(|v| v.name == name)
        .unwrap_or_else(|| panic!("Ansicht {name}"))
        .marks
        .clone()
}

fn kind_of(m: &Mark) -> MarkKind {
    match m {
        Mark::Line { kind, .. }
        | Mark::Arrow { kind, .. }
        | Mark::Arc { kind, .. }
        | Mark::Text { kind, .. } => *kind,
    }
}

fn count_marks(marks: &[Mark], f: impl Fn(&Mark) -> bool) -> usize {
    marks.iter().filter(|m| f(m)).count()
}

fn is_line(m: &Mark, kind: MarkKind) -> bool {
    matches!(m, Mark::Line { kind: k, .. } if *k == kind)
}

fn group_index(svg: &ReadSvg, id: &str) -> usize {
    svg.group_ids
        .iter()
        .position(|g| g == id)
        .unwrap_or_else(|| panic!("Gruppe {id:?} fehlt in {:?}", svg.group_ids))
}

/// Länge und Breite eines Pfeildreiecks (Spitze, Mitte des Fußes).
fn triangle(p: &ReadPath) -> (f64, f64) {
    let tip = p.segments[0].start();
    let a = p.segments[0].end();
    let b = p.segments[1].end();
    let mid = ((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0);
    (svg_read::dist(tip, mid), svg_read::dist(a, b))
}

// ---------------------------------------------------------------------------------------------
// (a) SVG
// ---------------------------------------------------------------------------------------------

#[test]
fn svg_groups_and_pens() {
    let model = sheet_on(Paper::A3, true, true);
    assert!((sheet_scale(model.sheet) - 1.0).abs() < 1e-12);
    let svg = svg_read::read(&svg_string(&model).unwrap());
    let hatch_id = "Schnitt A\u{2013}A Schraffur";
    let section_id = "Vorne Schnitt";
    let title_id = "Schnitt A\u{2013}A Schnitt";
    // Reihenfolge je Ansicht: Linien, Maße/Mittellinien, dann Schraffur, dann Schnitt.
    let vorne = group_index(&svg, "Vorne");
    assert!(group_index(&svg, "Vorne Mittellinien") > vorne);
    assert!(group_index(&svg, section_id) > group_index(&svg, "Vorne Mittellinien"));
    let sec_view = group_index(&svg, "Schnitt A\u{2013}A");
    assert!(group_index(&svg, section_id) < sec_view);
    assert!(group_index(&svg, "Schnitt A\u{2013}A Mittellinien") > sec_view);
    let h = group_index(&svg, hatch_id);
    assert!(h > group_index(&svg, "Schnitt A\u{2013}A Mittellinien"));
    assert_eq!(group_index(&svg, title_id), h + 1);

    // Schraffur: dünn, ohne Muster, so viele Striche wie Markierungen.
    let hatch: Vec<&ReadPath> = svg.paths_of(hatch_id).collect();
    let want = count_marks(&marks_of(&model, "Schnitt A\u{2013}A"), |m| {
        is_line(m, MarkKind::Hatch)
    });
    assert!(want > 10);
    assert_eq!(hatch.len(), want);
    for p in &hatch {
        assert_eq!(p.width, 0.25);
        assert!(p.dasharray.is_none());
        assert_eq!(p.stroke, "#000");
    }

    // Schnittlinie in der Quellansicht.
    let chain_marks = count_marks(&marks_of(&model, "Vorne"), |m| {
        is_line(m, MarkKind::SectionLine(SectionPen::Chain))
    });
    assert_eq!(chain_marks, 3, "Kette und zwei Schäfte");
    let mut chain = 0;
    let mut ends = 0;
    let mut tris = Vec::new();
    for p in svg.paths_of(section_id) {
        if p.fill != "none" {
            tris.push(triangle(p));
        } else if p.width == 0.5 {
            assert!(p.dasharray.is_none());
            ends += 1;
        } else {
            assert_eq!(p.width, 0.25);
            assert_eq!(p.dasharray.as_deref(), Some("12 3 2 3"));
            chain += 1;
        }
    }
    assert_eq!((chain, ends), (3, 2));
    assert_eq!(tris.len(), 2);
    for (len, width) in tris {
        assert!((len - 5.0).abs() < 1e-5 && (width - 2.0).abs() < 1e-5);
    }
    let letters: Vec<&str> = svg.texts_of(section_id).map(|t| t.text.as_str()).collect();
    assert_eq!(letters, ["A", "A"]);
    for t in svg.texts_of(section_id) {
        assert_eq!(t.font_size, 3.5);
    }
    let titles: Vec<_> = svg.texts_of(title_id).collect();
    assert_eq!(titles.len(), 1);
    assert_eq!(titles[0].text, TITLE);
    assert_eq!(titles[0].font_size, 5.0);

    // Kleineres und größeres Blatt: die Pfeile folgen dem Blattfaktor.
    for paper in [Paper::A4, Paper::A0] {
        let m = sheet_on(paper, true, true);
        let f = sheet_scale(m.sheet);
        let svg = svg_read::read(&svg_string(&m).unwrap());
        let tris: Vec<_> = svg
            .paths_of(section_id)
            .filter(|p| p.fill != "none")
            .map(triangle)
            .collect();
        assert_eq!(tris.len(), 2);
        for (len, width) in tris {
            assert!((len - 5.0 * f).abs() < 1e-5 && (width - 2.0 * f).abs() < 1e-5);
        }
    }

    // Ohne Schnitt keine dieser Gruppen.
    let plain = svg_read::read(&svg_string(&sheet_on(Paper::A3, false, true)).unwrap());
    for g in &plain.group_ids {
        assert!(!g.contains("Schraffur"), "{g}");
        assert!(!g.ends_with(" Schnitt"), "{g}");
    }
    // Schraffur aus: nur die Gruppe der Schraffur fehlt.
    let off = svg_read::read(&svg_string(&sheet_on(Paper::A3, true, false)).unwrap());
    assert!(!off.group_ids.iter().any(|g| g.contains("Schraffur")));
    assert!(off.group_ids.iter().any(|g| g == section_id));
}

// ---------------------------------------------------------------------------------------------
// (b) DXF
// ---------------------------------------------------------------------------------------------

fn layer_row(dxf: &dxf_read::Dxf, name: &str) -> Option<(String, String, String)> {
    dxf.table("LAYER")?
        .records
        .iter()
        .find(|r| r.name() == name)
        .map(|r| {
            (
                r.get(62).unwrap().to_string(),
                r.get(6).unwrap().to_string(),
                r.get(370).unwrap().to_string(),
            )
        })
}

fn without_centerlines(mut m: SheetModel) -> SheetModel {
    for v in &mut m.views {
        v.marks.retain(|x| kind_of(x) != MarkKind::CenterLine);
    }
    m
}

fn center_ltypes(dxf: &dxf_read::Dxf) -> usize {
    dxf.table("LTYPE")
        .unwrap()
        .records
        .iter()
        .filter(|r| r.name() == "CENTER")
        .count()
}

#[test]
fn dxf_layers() {
    let model = sheet_on(Paper::A3, true, true);
    let dxf = dxf_read::read(&dxf_string(&model).unwrap()).unwrap();
    let s = |a: &str, b: &str, c: &str| Some((a.to_string(), b.to_string(), c.to_string()));
    assert_eq!(layer_row(&dxf, "SCHRAFFUR"), s("7", "CONTINUOUS", "25"));
    assert_eq!(layer_row(&dxf, "SCHNITTLINIE"), s("7", "CENTER", "25"));
    assert_eq!(
        layer_row(&dxf, "SCHNITTLINIE_DICK"),
        s("7", "CONTINUOUS", "50")
    );
    // Tabellenzähler stimmt (Prüfung im Leser) und CENTER genau einmal.
    assert_eq!(center_ltypes(&dxf), 1);

    // Objekte auf den Ebenen.
    let hatch_marks = count_marks(&marks_of(&model, "Schnitt A\u{2013}A"), |m| {
        is_line(m, MarkKind::Hatch)
    });
    let on = |block: &str, layer: &str, kind: &str| {
        dxf.block(block)
            .unwrap()
            .entities
            .iter()
            .filter(|e| e.layer == layer && e.kind == kind)
            .count()
    };
    assert_eq!(on("Schnitt A\u{2013}A", "SCHRAFFUR", "LINE"), hatch_marks);
    assert_eq!(on("Vorne", "SCHNITTLINIE", "LINE"), 3);
    assert_eq!(on("Vorne", "SCHNITTLINIE_DICK", "LINE"), 2);
    assert_eq!(on("Vorne", "SCHNITTLINIE", "SOLID"), 2);
    assert_eq!(on("Vorne", "SCHNITTLINIE", "TEXT"), 2);
    assert_eq!(on("Schnitt A\u{2013}A", "SCHNITTLINIE", "TEXT"), 1);
    // Pfeile: 5 lang, 2 breit.
    for e in dxf
        .block("Vorne")
        .unwrap()
        .entities
        .iter()
        .filter(|e| e.kind == "SOLID")
    {
        let dxf_read::Geom::Solid { pts } = e.geom().unwrap() else {
            panic!("SOLID");
        };
        let mid = ((pts[1].0 + pts[2].0) / 2.0, (pts[1].1 + pts[2].1) / 2.0);
        assert!((dxf_read::dist(pts[0], mid) - 5.0).abs() < 1e-6);
        assert!((dxf_read::dist(pts[1], pts[2]) - 2.0).abs() < 1e-6);
    }
    // Der Titel: Halbgeviertstrich als \U+2013, nach dem Lesen wieder „A–A (1:1)“.
    let title = dxf
        .block("Schnitt A\u{2013}A")
        .unwrap()
        .entities
        .iter()
        .find(|e| e.kind == "TEXT")
        .unwrap();
    let dxf_read::Geom::Text { text, raw, .. } = title.geom().unwrap() else {
        panic!("TEXT");
    };
    assert_eq!(raw, "A\\U+2013A (1:1)");
    assert_eq!(text, TITLE);

    // Ohne Schnitte keine der Ebenen.
    let plain = dxf_read::read(&dxf_string(&sheet_on(Paper::A3, false, true)).unwrap()).unwrap();
    for name in ["SCHRAFFUR", "SCHNITTLINIE", "SCHNITTLINIE_DICK"] {
        assert!(layer_row(&plain, name).is_none(), "{name}");
    }
    assert_eq!(center_ltypes(&plain), 1);
    // Schraffur aus: nur diese Ebene fehlt.
    let off = dxf_read::read(&dxf_string(&sheet_on(Paper::A3, true, false)).unwrap()).unwrap();
    assert!(layer_row(&off, "SCHRAFFUR").is_none());
    assert!(layer_row(&off, "SCHNITTLINIE").is_some());
    // Ohne Mittellinien, aber mit Schnittlinie: CENTER genau einmal.
    let nc = without_centerlines(sheet_on(Paper::A3, true, true));
    let nc_dxf = dxf_read::read(&dxf_string(&nc).unwrap()).unwrap();
    assert!(layer_row(&nc_dxf, "MITTEL").is_none());
    assert_eq!(center_ltypes(&nc_dxf), 1);
    // Ohne Mittellinien und ohne Schnitt: kein CENTER.
    let nothing = dxf_read::read(
        &dxf_string(&without_centerlines(sheet_on(Paper::A3, false, true))).unwrap(),
    )
    .unwrap();
    assert_eq!(center_ltypes(&nothing), 0);
    // A4: Pfeil 5·f lang.
    let a4 = sheet_on(Paper::A4, true, true);
    let f = sheet_scale(a4.sheet);
    let d4 = dxf_read::read(&dxf_string(&a4).unwrap()).unwrap();
    for e in d4
        .block("Vorne")
        .unwrap()
        .entities
        .iter()
        .filter(|e| e.kind == "SOLID")
    {
        let dxf_read::Geom::Solid { pts } = e.geom().unwrap() else {
            panic!("SOLID");
        };
        let mid = ((pts[1].0 + pts[2].0) / 2.0, (pts[1].1 + pts[2].1) / 2.0);
        assert!((dxf_read::dist(pts[0], mid) - 5.0 * f).abs() < 1e-6);
    }
}

// ---------------------------------------------------------------------------------------------
// (c) PDF wie SVG
// ---------------------------------------------------------------------------------------------

fn svg_rgb(c: &str) -> [f64; 3] {
    assert_eq!(c, "#000");
    [0.0; 3]
}

fn pdf_same_as_svg_for(model: &SheetModel) {
    let svg_text = svg_string(model).unwrap();
    let svg = svg_read::read(&svg_text);
    let pdf = pdf_read::read(&pdf_bytes(&[model], &PdfMeta::default()).unwrap()).unwrap();
    let items = &pdf.pages[0].items;
    // Reihenfolge der Elemente in der SVG-Datei (Pfade und Texte).
    let mut order = String::new();
    let mut rest = svg_text.as_str();
    let mut paths = svg.paths.iter();
    loop {
        let p = rest.find("<path ");
        let t = rest.find("<text ");
        let (at, is_path) = match (p, t) {
            (Some(p), Some(t)) => (p.min(t), p < t),
            (Some(p), None) => (p, true),
            (None, Some(t)) => (t, false),
            (None, None) => break,
        };
        order.push(if is_path {
            if paths.next().unwrap().fill != "none" {
                'F'
            } else {
                'S'
            }
        } else {
            'T'
        });
        rest = &rest[at + 1..];
    }
    let pdf_order: String = items
        .iter()
        .map(|i| match i {
            Item::Stroke { .. } => 'S',
            Item::Fill { .. } => 'F',
            Item::Text { .. } => 'T',
        })
        .collect();
    assert_eq!(pdf_order, order, "Reihenfolge der Elemente");
    assert!(order.contains('F') && order.contains('T'));

    let (mut paths, mut texts) = (svg.paths.iter(), svg.texts.iter());
    let near =
        |a: (f64, f64), b: (f64, f64)| (a.0 - b.0).abs() <= 1e-3 && (a.1 - b.1).abs() <= 1e-3;
    for (i, item) in items.iter().enumerate() {
        match item {
            Item::Stroke {
                rgb,
                width,
                dash,
                subpaths,
            } => {
                let s = paths.next().unwrap();
                assert_eq!(svg_rgb(&s.stroke), *rgb, "Element {i}");
                assert!((mm(*width) - s.width).abs() <= 1e-4, "Element {i}: Breite");
                let want: Vec<f64> = s
                    .dasharray
                    .as_deref()
                    .map(|d| d.split_whitespace().map(|x| x.parse().unwrap()).collect())
                    .unwrap_or_default();
                assert_eq!(dash.len(), want.len(), "Element {i}: Muster");
                for (a, b) in dash.iter().zip(&want) {
                    assert!((mm(*a) - b).abs() <= 1e-4, "Element {i}: Muster");
                }
                let flat: Vec<_> = subpaths.iter().flatten().collect();
                assert_eq!(flat.len(), s.segments.len(), "Element {i}");
                for (a, b) in s.segments.iter().zip(flat) {
                    let (bs, be) = (b.start(), b.end());
                    assert!(near(a.start(), (mm(bs.0), mm(bs.1))), "Element {i}");
                    assert!(near(a.end(), (mm(be.0), mm(be.1))), "Element {i}");
                }
            }
            Item::Fill { rgb, subpaths } => {
                let s = paths.next().unwrap();
                assert_eq!(svg_rgb(&s.fill), *rgb);
                let flat: Vec<_> = subpaths.iter().flatten().collect();
                assert!(flat.len() >= 2, "Pfeil {i}");
                for (a, b) in s.segments.iter().zip(flat) {
                    let bs = b.start();
                    assert!(near(a.start(), (mm(bs.0), mm(bs.1))), "Pfeil {i}");
                }
            }
            Item::Text { size, text, .. } => {
                let s = texts.next().unwrap();
                assert_eq!(text, &s.text);
                assert!((mm(*size) - s.font_size).abs() <= 1e-4);
            }
        }
    }
    assert!(paths.next().is_none() && texts.next().is_none());
    // Die Schnittelemente stehen in der Datei: Titel, zwei Buchstaben, Muster der Kette.
    assert!(
        items
            .iter()
            .any(|i| matches!(i, Item::Text { text, .. } if text == TITLE))
    );
    assert_eq!(
        items
            .iter()
            .filter(|i| matches!(i, Item::Text { text, .. } if text == "A"))
            .count(),
        2
    );
    let pattern = |w: f64, n: usize| {
        items
            .iter()
            .filter(|i| {
                matches!(i, Item::Stroke { width, dash, .. }
                if (mm(*width) - w).abs() < 1e-4 && dash.len() == n)
            })
            .count()
    };
    assert!(pattern(0.25, 4) >= 3);
    assert!(pattern(0.5, 0) >= 2);
}

#[test]
fn pdf_same_as_svg() {
    for paper in [Paper::A3, Paper::A4, Paper::A0] {
        pdf_same_as_svg_for(&sheet_on(paper, true, true));
    }
    pdf_same_as_svg_for(&sheet_on(Paper::A3, true, false));
}

// ---------------------------------------------------------------------------------------------
// (d) Goldene Dateien
// ---------------------------------------------------------------------------------------------

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

fn first_diff(a: &[u8], b: &[u8]) -> Option<usize> {
    a.iter()
        .zip(b)
        .position(|(x, y)| x != y)
        .or((a.len() != b.len()).then_some(a.len().min(b.len())))
}

#[test]
fn golden_files() {
    let update = std::env::var("PLANORA_UPDATE_SECTION_GOLDEN").is_ok_and(|v| v == "1");
    let model = sheet_on(Paper::A4, true, true);
    let meta = PdfMeta {
        title: "Golden".into(),
        author: Some("Prüfer Ä".into()),
        date: Some("2026-10-19".into()),
    };
    let svg = svg_string(&model).unwrap();
    let dxf = dxf_string(&model).unwrap();
    let pdf = pdf_bytes(&[&model], &meta).unwrap();
    assert_eq!(pdf, pdf_bytes(&[&model], &meta).unwrap());
    let cases: [(&str, Vec<u8>); 3] = [
        ("svg_section_tube.svg", svg.clone().into_bytes()),
        ("dxf_section_tube.dxf", dxf.clone().into_bytes()),
        ("pdf_section_tube.pdf", pdf.clone()),
    ];
    for (name, bytes) in &cases {
        if update {
            std::fs::write(fixture(name), bytes).unwrap();
        }
        let stored = std::fs::read(fixture(name)).unwrap_or_else(|e| panic!("{name}: {e}"));
        if let Some(i) = first_diff(&stored, bytes) {
            panic!(
                "{name}: Bytes weichen ab Stelle {i} ab (Fixtur {} Bytes, Schreiber {} Bytes)",
                stored.len(),
                bytes.len()
            );
        }
    }
    // Die Fixturen bestehen die Leser und zeigen den Schnitt.
    let svg_r = svg_read::read(&String::from_utf8(cases[0].1.clone()).unwrap());
    assert!(svg_r.group_ids.iter().any(|g| g == "Vorne Schnitt"));
    assert!(
        svg_r
            .group_ids
            .iter()
            .any(|g| g == "Schnitt A\u{2013}A Schraffur")
    );
    assert!(svg_r.texts.iter().any(|t| t.text == TITLE));
    let dxf_r = dxf_read::read(&String::from_utf8(cases[1].1.clone()).unwrap()).unwrap();
    assert!(layer_row(&dxf_r, "SCHRAFFUR").is_some());
    assert_eq!(center_ltypes(&dxf_r), 1);
    let pdf_r = pdf_read::read(&cases[2].1).unwrap();
    assert_eq!(pdf_r.pages.len(), 1);
    assert_eq!(pdf_r.pages[0].media_box[2..], [841.8898, 595.2756]);
    assert_eq!(pdf_read::utf16_text(&pdf_r.info["Title"]), "Golden");
    assert_eq!(pdf_read::utf16_text(&pdf_r.info["Author"]), "Prüfer Ä");
    assert_eq!(
        pdf_r.info["CreationDate"],
        pdf_read::Value::Str(b"D:20261019000000Z".to_vec())
    );
    assert!(
        !update,
        "Golden-Dateien neu geschrieben: ohne PLANORA_UPDATE_SECTION_GOLDEN=1 erneut prüfen"
    );
}

// ---------------------------------------------------------------------------------------------
// (e) Alte Golden-Dateien unverändert
// ---------------------------------------------------------------------------------------------

fn fnv(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325u64, |h, b| {
        (h ^ u64::from(*b)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

#[test]
fn old_goldens_unchanged() {
    // (Datei, Länge, FNV-1a-64); die Schreiber dieser Dateien prüfen ihre Stufen weiter.
    let pins: [(&str, usize, u64); 9] = [
        ("dxf_without_marks_box.dxf", 8216, 0xf996ae72ab250213),
        ("dxf_without_marks_cylinder.dxf", 6859, 0xe18b8ea90a437eeb),
        ("pdf_box_frame_a3_portrait.pdf", 14300, 0xdd1b18ea2949adaa),
        ("pdf_cylinder_a4_landscape.pdf", 2027, 0xda2863a0c98a7110),
        ("stage15_sheet_lines.txt", 3048, 0x14e41c67856ab6a3),
        ("svg_balloons_a4_a0.svg", 5147, 0x4f5b2cc391f7ba3b),
        ("svg_splines.svg", 10727, 0x44a6fdf6a974fb7d),
        ("svg_without_marks_box.svg", 1088, 0x81001732d989d3f6),
        ("svg_without_marks_cylinder.svg", 811, 0x0ca24303fd757df8),
    ];
    for (name, len, hash) in pins {
        let mut bytes = std::fs::read(fixture(name)).unwrap();
        if name.ends_with(".txt") {
            bytes.retain(|b| *b != b'\r');
        }
        assert_eq!(bytes.len(), len, "{name}: Länge");
        assert_eq!(fnv(&bytes), hash, "{name}: Inhalt");
    }
}
