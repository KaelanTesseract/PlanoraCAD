//! Stufe 27: Die PDF-Ausgabe zeigt genau den Inhalt der SVG-Ausgabe desselben Blatts (gleiche
//! Elemente in gleicher Reihenfolge, gleiche Punkte, Strichbreiten, Muster und Texte), die
//! Bézier-Stücke folgen den exakten Bögen, zwei goldene PDF-Dateien halten die Bytes fest, und
//! die goldenen SVG/DXF-Dateien bleiben unverändert.
//!
//! Golden-Dateien neu erzeugen (nur bei einer gewollten Änderung des Schreibers, im Commit
//! begründen): `PLANORA_UPDATE_PDF_GOLDEN=1 cargo test -p planora-drawing --test stage27_pdf_svg`.
//! Der Test schreibt dann die Dateien und schlägt trotzdem fehl, damit das nie unbemerkt
//! geschieht; danach ohne die Variable erneut laufen lassen.

mod common;

use common::model::cylinder_solid;
use common::pdf_read::{self, Item, mm, read, text_width_pt};
use common::svg_marks::{box_sheet, cylinder_sheet, drawing, manual_sheet};
use common::svg_read::{self, ReadPath, ReadText};
use planora_drawing::annotate::{Mark, MarkKind, ResolvedDims, TextAnchor};
use planora_drawing::balloon::{PositionPen, layout_balloon};
use planora_drawing::model::{Paper, Placement, Projection, Scale, Sheet};
use planora_drawing::out2d::Out2d;
use planora_drawing::sheet::{SheetLine, SheetModel, SheetView, build_sheet_with};
use planora_drawing::sheet_frame::{PartsSpace, TitleField, sheet_layout_with, sheet_scale};
use planora_drawing::sheet_info::TitleTexts;
use planora_drawing::sheet_marks::sheet_frame;
use planora_drawing::stroke::StrokeOrigin;
use planora_drawing::{
    DrawBody, PdfMeta, ViewId, ViewKind, ViewLines, Visibility, compute_view, dxf_string,
    pdf_bytes, svg_string,
};
use planora_exchange::Progress;
use planora_math::{DVec2, PI, TAU};
use std::collections::BTreeMap;
use std::path::PathBuf;

// ---------------------------------------------------------------------------------------------
// Blätter
// ---------------------------------------------------------------------------------------------

fn v(x: f64, y: f64) -> DVec2 {
    DVec2::new(x, y)
}

fn sheet(paper: Paper, landscape: bool) -> Sheet {
    Sheet { paper, landscape }
}

/// Dokumenteigenschaften der goldenen Dateien.
fn golden_meta() -> PdfMeta {
    PdfMeta {
        title: "Golden".into(),
        author: Some("Prüfer Ä".into()),
        date: Some("2026-10-18".into()),
    }
}

/// Goldenes Blatt 1: Zylinder (r 8, h 12) auf A4 quer, ohne Rahmen, mit Mittellinien: Vorne,
/// Oben (Kreise) und Isometrie 2:1 (Ellipsen), alle Ansichten ganz auf dem Blatt.
fn golden_cylinder() -> SheetModel {
    let solid = cylinder_solid(8.0, 12.0);
    let bodies = [DrawBody {
        name: "Z".into(),
        solid: &solid,
    }];
    let mut d = drawing(Paper::A4);
    let one = Scale { num: 1, den: 1 };
    let f = d.add_view(
        ViewKind::Front,
        one,
        Placement::Free { x: 70.0, y: 140.0 },
        None,
    );
    let t = d.add_view(
        ViewKind::Top,
        one,
        Placement::Aligned { distance: 40.0 },
        Some(f),
    );
    let i = d.add_view(
        ViewKind::Isometric,
        Scale { num: 2, den: 1 },
        Placement::Free { x: 200.0, y: 105.0 },
        None,
    );
    for w in &mut d.views {
        w.centerlines = true;
    }
    let p = Progress::new();
    let vf = compute_view(&bodies, ViewKind::Front, &p).unwrap();
    let vt = compute_view(&bodies, ViewKind::Top, &p).unwrap();
    let vi = compute_view(&bodies, ViewKind::Isometric, &p).unwrap();
    let res: BTreeMap<ViewId, &ViewLines> = BTreeMap::from([(f, &vf), (t, &vt), (i, &vi)]);
    let m = build_sheet_with(&d, &res, &ResolvedDims::default());
    assert_eq!(m.sheet, sheet(Paper::A4, true));
    assert!(m.frame.is_none());
    m
}

/// Texte des Rahmens mit Sonderzeichen, drei Stücklistenzeilen.
fn special_texts() -> TitleTexts {
    let s = "Ø 12 ä ß µ ± ° € – −0,1";
    TitleTexts {
        fields: [
            (TitleField::Title, format!("Halter {s}")),
            (TitleField::Number, "Z-0815".to_string()),
            (TitleField::Material, s.to_string()),
            (TitleField::SheetOf, "1 von 1".to_string()),
            (TitleField::Scale, "1:1".to_string()),
        ]
        .into_iter()
        .collect(),
        revisions: vec![
            [
                "A".into(),
                format!("Bohrung {s}"),
                "2026-10-01".into(),
                "dg".into(),
            ],
            ["B".into(), "Fase".into(), "2026-10-18".into(), "dg".into()],
        ],
        notes: vec![
            format!("Toleranz {s}"),
            "Kanten gebrochen".into(),
            "Maße in mm".into(),
        ],
        parts: vec![
            [
                "1".into(),
                format!("Grundplatte {s}"),
                "1".into(),
                "S235".into(),
                "".into(),
            ],
            [
                "2".into(),
                "Welle Ø 12".into(),
                "2".into(),
                "C45 ± 0,1".into(),
                "gehärtet".into(),
            ],
            [
                "3".into(),
                "Scheibe".into(),
                "4".into(),
                "µ-Stahl".into(),
                "€ 0,10".into(),
            ],
        ],
        units_note: None,
    }
}

/// Goldenes Blatt 2 (auf `paper`): `box_sheet(true)` mit vollem Rahmen aus
/// `sheet_layout_with(…, 2, 3, OnSheet { rows: 3 })` und einem Nummernkreis (Nummer 2) an
/// „Vorne“.
fn golden_box_on(s: Sheet) -> SheetModel {
    let mut m = box_sheet(true);
    m.sheet = s;
    let layout = sheet_layout_with(s, 2, 3, PartsSpace::OnSheet { rows: 3 });
    m.frame = Some(sheet_frame(&layout, &special_texts(), Projection::First));
    let front = m
        .views
        .iter_mut()
        .find(|w| w.name == "Vorne")
        .expect("Vorne");
    front.marks.extend(layout_balloon(
        v(120.0, 200.0),
        (25.0, 20.0),
        Some(2),
        sheet_scale(s),
    ));
    m
}

fn golden_box() -> SheetModel {
    golden_box_on(sheet(Paper::A3, false))
}

fn text(pos: DVec2, deg: f64, anchor: TextAnchor, s: &str, kind: MarkKind) -> Mark {
    Mark::Text {
        pos,
        height: 3.5,
        angle: deg.to_radians(),
        anchor,
        text: s.into(),
        kind,
        fit_width: None,
    }
}

/// Von Hand: Maßpfeile, gedrehte Texte (0°, 90°, 30°), ein verlorenes Maß, Mittellinien vor den
/// Maßen eingetragen, ein Nummernkreis, Bögen.
fn marks_sheet() -> SheetModel {
    let d = MarkKind::Dimension;
    let first = vec![
        Mark::Line {
            a: v(10.0, 150.0),
            b: v(90.0, 150.0),
            kind: MarkKind::CenterLine,
        },
        Mark::Line {
            a: v(20.0, 40.0),
            b: v(120.0, 40.0),
            kind: d,
        },
        Mark::Arrow {
            tip: v(20.0, 40.0),
            dir: v(1.0, 0.0),
            kind: d,
        },
        Mark::Arrow {
            tip: v(120.0, 40.0),
            dir: v(-1.0, 0.0),
            kind: d,
        },
        text(v(70.0, 41.0), 0.0, TextAnchor::Middle, "100 −0,1", d),
        Mark::Line {
            a: v(150.0, 20.0),
            b: v(150.0, 90.0),
            kind: d,
        },
        Mark::Arrow {
            tip: v(150.0, 20.0),
            dir: v(0.0, 1.0),
            kind: d,
        },
        text(v(149.0, 55.0), 90.0, TextAnchor::Middle, "Ø 70", d),
        Mark::Line {
            a: v(200.0, 50.0),
            b: v(260.0, 84.641),
            kind: d,
        },
        Mark::Arrow {
            tip: v(200.0, 50.0),
            dir: v(0.866_025_403_784_438_6, 0.5),
            kind: d,
        },
        text(v(230.0, 68.0), 30.0, TextAnchor::Start, "R 5 (x) \\ ä", d),
        text(v(300.0, 30.0), 0.0, TextAnchor::End, "Ende ±0,2°", d),
        Mark::Arc {
            center: v(300.0, 120.0),
            r: 15.0,
            start: 0.2,
            end: 0.2 + 1.9 * PI,
            kind: d,
        },
        Mark::Arc {
            center: v(340.0, 120.0),
            r: 8.0,
            start: -0.4,
            end: 1.1,
            kind: MarkKind::CenterLine,
        },
        // Verlorenes Maß: rot, mit Pfeil und Text.
        Mark::Line {
            a: v(30.0, 200.0),
            b: v(110.0, 200.0),
            kind: MarkKind::Lost,
        },
        Mark::Arrow {
            tip: v(30.0, 200.0),
            dir: v(1.0, 0.0),
            kind: MarkKind::Lost,
        },
        text(
            v(70.0, 201.0),
            0.0,
            TextAnchor::Middle,
            "80?",
            MarkKind::Lost,
        ),
        Mark::Line {
            a: v(50.0, 140.0),
            b: v(50.0, 160.0),
            kind: MarkKind::CenterLine,
        },
    ];
    let mut second = layout_balloon(v(200.0, 200.0), (20.0, 15.0), Some(17), 1.0);
    second.push(Mark::Line {
        a: v(180.0, 180.0),
        b: v(220.0, 180.0),
        kind: MarkKind::CenterLine,
    });
    second.extend(layout_balloon(v(250.0, 220.0), (-15.0, 10.0), None, 1.0));
    second.push(Mark::Arc {
        center: v(260.0, 250.0),
        r: 4.0,
        start: 0.0,
        end: TAU,
        kind: MarkKind::Position(PositionPen::Leader),
    });
    manual_sheet(Paper::A3, vec![("Vorne", first), ("Oben", second)])
}

fn spline(degree: usize, knots: Vec<f64>, ctrl: &[(f64, f64)], weights: Vec<f64>) -> Out2d {
    Out2d::Spline {
        degree,
        knots,
        ctrl: ctrl.iter().map(|&(x, y)| v(x, y)).collect(),
        weights,
    }
}

/// Splines Grad 1, 2, 3, 5 und rational, sichtbar und verdeckt, dazu Bögen und Ellipsen.
fn spline_sheet() -> SheetModel {
    let h = std::f64::consts::FRAC_1_SQRT_2;
    let lines = vec![
        (
            Visibility::Visible,
            spline(
                1,
                vec![0.0, 0.0, 1.0, 2.0, 3.0, 3.0],
                &[(10.0, 10.0), (40.0, 12.0), (42.0, 40.0), (70.0, 45.0)],
                vec![1.0; 4],
            ),
        ),
        (
            Visibility::Visible,
            spline(
                2,
                vec![0.0, 0.0, 0.0, 0.5, 1.0, 1.0, 1.0],
                &[(80.0, 10.0), (90.0, 50.0), (120.0, 40.0), (130.0, 10.0)],
                vec![1.0; 4],
            ),
        ),
        (
            Visibility::Hidden,
            spline(
                3,
                vec![0.0, 0.0, 0.0, 0.0, 0.4, 1.0, 1.0, 1.0, 1.0],
                &[
                    (150.0, 10.0),
                    (160.0, 60.0),
                    (180.0, -10.0),
                    (200.0, 40.0),
                    (220.0, 20.0),
                ],
                vec![1.0; 5],
            ),
        ),
        (
            Visibility::Visible,
            spline(
                5,
                vec![
                    0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.5, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0,
                ],
                &[
                    (10.0, 100.0),
                    (20.0, 140.0),
                    (40.0, 90.0),
                    (60.0, 150.0),
                    (80.0, 95.0),
                    (100.0, 130.0),
                    (120.0, 100.0),
                ],
                vec![1.0; 7],
            ),
        ),
        (
            Visibility::Visible,
            spline(
                2,
                vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0],
                &[(170.0, 100.0), (170.0, 120.0), (150.0, 120.0)],
                vec![1.0, h, 1.0],
            ),
        ),
        (
            Visibility::Hidden,
            spline(
                3,
                vec![0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0],
                &[
                    (200.0, 100.0),
                    (210.0, 140.0),
                    (240.0, 90.0),
                    (250.0, 130.0),
                ],
                vec![1.0, 2.0, 0.5, 1.0],
            ),
        ),
        (
            Visibility::Visible,
            Out2d::Arc {
                c: v(300.0, 60.0),
                r: 20.0,
                start: -1.0,
                end: 3.5,
            },
        ),
        (
            Visibility::Hidden,
            Out2d::Arc {
                c: v(300.0, 60.0),
                r: 10.0,
                start: 0.0,
                end: TAU,
            },
        ),
        (
            Visibility::Visible,
            Out2d::Ellipse {
                c: v(300.0, 200.0),
                major: v(30.0, 12.0),
                ratio: -0.45,
                start: 0.5,
                end: 5.0,
            },
        ),
        (
            Visibility::Visible,
            Out2d::Line {
                a: v(260.0, 150.0),
                b: v(380.0, 150.0),
            },
        ),
    ];
    SheetModel {
        sheet: sheet(Paper::A3, true),
        frame: None,
        views: vec![SheetView {
            id: ViewId(1),
            name: "Kurven".into(),
            lines: lines
                .into_iter()
                .map(|(vis, curve)| SheetLine {
                    vis,
                    curve,
                    origin: StrokeOrigin::Edge { body: 0, edge: 0 },
                })
                .collect(),
            extent_mm: (0.0, 0.0),
            marks: Vec::new(),
        }],
    }
}

// ---------------------------------------------------------------------------------------------
// Vergleich SVG ↔ PDF
// ---------------------------------------------------------------------------------------------

/// Reihenfolge der Elemente in der SVG-Datei: `S` Strich, `F` Fläche, `T` Text.
fn svg_kinds(svg: &str, read: &svg_read::ReadSvg) -> String {
    let mut out = String::new();
    let mut paths = read.paths.iter();
    let mut rest = svg;
    loop {
        let p = rest.find("<path ");
        let t = rest.find("<text ");
        let (at, is_path) = match (p, t) {
            (Some(p), Some(t)) => (p.min(t), p < t),
            (Some(p), None) => (p, true),
            (None, Some(t)) => (t, false),
            (None, None) => break,
        };
        if is_path {
            let path = paths.next().expect("Pfad");
            out.push(if path.fill != "none" { 'F' } else { 'S' });
        } else {
            out.push('T');
        }
        rest = &rest[at + 1..];
    }
    assert!(paths.next().is_none());
    out
}

fn pdf_kinds(page: &pdf_read::Page) -> String {
    page.items
        .iter()
        .map(|i| match i {
            Item::Stroke { .. } => 'S',
            Item::Fill { .. } => 'F',
            Item::Text { .. } => 'T',
        })
        .collect()
}

fn svg_rgb(c: &str) -> [f64; 3] {
    match c {
        "#000" => [0.0, 0.0, 0.0],
        "#c00" => [0.8, 0.0, 0.0],
        o => panic!("Farbe {o}"),
    }
}

fn mm_pt(p: pdf_read::P) -> (f64, f64) {
    (mm(p.0), mm(p.1))
}

fn near(a: (f64, f64), b: (f64, f64), tol: f64) -> bool {
    (a.0 - b.0).abs() <= tol && (a.1 - b.1).abs() <= tol
}

/// Ein Paar gleicher Stücke: SVG (mm) und PDF (pt).
type SegPair = (svg_read::Seg, pdf_read::Seg);

fn compare_stroke(
    what: &str,
    i: usize,
    s: &ReadPath,
    rgb: [f64; 3],
    width: Option<(f64, &[f64])>,
    subpaths: &[Vec<pdf_read::Seg>],
    pairs: &mut Vec<SegPair>,
) {
    let at = format!("{what}, Element {i}");
    if let Some((w, dash)) = width {
        assert_eq!(svg_rgb(&s.stroke), rgb, "{at}: Farbe");
        assert!(
            (mm(w) - s.width).abs() <= 1e-4,
            "{at}: Breite {} {}",
            mm(w),
            s.width
        );
        let want: Vec<f64> = s
            .dasharray
            .as_deref()
            .map(|d| d.split_whitespace().map(|x| x.parse().unwrap()).collect())
            .unwrap_or_default();
        assert_eq!(dash.len(), want.len(), "{at}: Muster {dash:?} {want:?}");
        for (a, b) in dash.iter().zip(&want) {
            assert!((mm(*a) - b).abs() <= 1e-4, "{at}: Muster {dash:?} {want:?}");
        }
    } else {
        assert_eq!(svg_rgb(&s.fill), rgb, "{at}: Füllfarbe");
    }
    assert_eq!(subpaths.len(), s.subpaths, "{at}: Teilpfade");
    let flat: Vec<&pdf_read::Seg> = subpaths.iter().flatten().collect();
    assert_eq!(flat.len(), s.segments.len(), "{at}: Stücke");
    for (k, (a, b)) in s.segments.iter().zip(&flat).enumerate() {
        assert!(
            near(a.start(), mm_pt(b.start()), 1e-3) && near(a.end(), mm_pt(b.end()), 1e-3),
            "{at}, Stück {k}: SVG {:?}→{:?}, PDF {:?}→{:?}",
            a.start(),
            a.end(),
            mm_pt(b.start()),
            mm_pt(b.end())
        );
        pairs.push((a.clone(), (*b).clone()));
    }
}

fn anchor_k(a: &str) -> f64 {
    match a {
        "start" => 0.0,
        "middle" => 0.5,
        "end" => 1.0,
        o => panic!("Anker {o}"),
    }
}

fn compare_text(what: &str, i: usize, s: &ReadText, item: &Item) {
    let &Item::Text {
        rgb,
        size,
        matrix: m,
        tz,
        ref text,
    } = item
    else {
        panic!("{what}, Element {i}: Text erwartet");
    };
    let t = text.as_str();
    let at = format!("{what}, Text {i} {:?}", s.text);
    assert_eq!(t, s.text.replace('−', "-"), "{at}: Inhalt");
    assert_eq!(svg_rgb(&s.fill), rgb, "{at}: Farbe");
    assert!((mm(size) - s.font_size).abs() <= 1e-4, "{at}: Höhe");
    assert!(s.upright, "{at}");
    // Winkel: die Matrix ist eine Drehung um den Winkel der SVG-Schreibrichtung.
    let (ax, ay) = s.axis;
    for (got, want) in [(m[0], ax), (m[1], ay), (m[2], -ay), (m[3], ax)] {
        assert!(
            (got - want).abs() <= 1e-4,
            "{at}: Matrix {m:?}, Achse {:?}",
            s.axis
        );
    }
    // Ankerpunkt: Ursprung + k · gezeichnete Breite in Schreibrichtung.
    let w = text_width_pt(t, size, tz);
    let k = anchor_k(&s.anchor);
    let anchor = (mm(m[4] + k * w * ax), mm(m[5] + k * w * ay));
    assert!(
        near(anchor, s.pos, 1e-3),
        "{at}: Anker {anchor:?} statt {:?}",
        s.pos
    );
}

/// Prüft, dass die PDF genau den Inhalt der SVG zeigt; liefert alle Strichstücke paarweise.
fn same_content(what: &str, model: &SheetModel) -> Vec<SegPair> {
    let svg_text = svg_string(model).unwrap();
    let svg = svg_read::read(&svg_text);
    let pdf = read(&pdf_bytes(&[model], &PdfMeta::default()).unwrap()).expect("lesbar");
    assert_eq!(pdf.pages.len(), 1);
    let page = &pdf.pages[0];
    assert!(
        (mm(page.media_box[2]) - svg.viewbox[2]).abs() <= 1e-3,
        "{what}"
    );
    assert!(
        (mm(page.media_box[3]) - svg.viewbox[3]).abs() <= 1e-3,
        "{what}"
    );
    let kinds = svg_kinds(&svg_text, &svg);
    assert_eq!(pdf_kinds(page), kinds, "{what}: Reihenfolge der Elemente");
    let strokes = svg.paths.iter().filter(|p| p.stroke != "none").count();
    let fills = svg.paths.iter().filter(|p| p.fill != "none").count();
    assert_eq!(kinds.matches('S').count(), strokes, "{what}");
    assert_eq!(kinds.matches('F').count(), fills, "{what}");
    assert_eq!(kinds.matches('T').count(), svg.texts.len(), "{what}");
    assert!(!kinds.is_empty(), "{what}: leer");

    let mut pairs = Vec::new();
    let (mut paths, mut texts) = (svg.paths.iter(), svg.texts.iter());
    for (i, item) in page.items.iter().enumerate() {
        match item {
            Item::Stroke {
                rgb,
                width,
                dash,
                subpaths,
            } => {
                let s = paths.next().unwrap();
                assert_eq!(s.fill, "none", "{what}, Element {i}");
                assert_eq!(s.linecap, "round", "{what}, Element {i}");
                compare_stroke(what, i, s, *rgb, Some((*width, dash)), subpaths, &mut pairs);
            }
            Item::Fill { rgb, subpaths } => {
                let s = paths.next().unwrap();
                assert_eq!(s.stroke, "none", "{what}, Element {i}");
                compare_stroke(what, i, s, *rgb, None, subpaths, &mut Vec::new());
            }
            Item::Text { .. } => {
                let s = texts.next().unwrap();
                compare_text(what, i, s, item);
            }
        }
    }
    pairs
}

// (a) Gleicher Inhalt wie SVG.
#[test]
fn same_content_as_svg() {
    let pairs = same_content("Quader", &box_sheet(true));
    assert!(pairs.iter().any(|(s, _)| s.kind() == 'A'));
    let pairs = same_content("Zylinder", &cylinder_sheet(true));
    assert!(pairs.iter().any(|(s, _)| s.kind() == 'A'));
    same_content("Golden 1", &golden_cylinder());
    let g2 = golden_box();
    let pairs = same_content("Golden 2", &g2);
    assert!(pairs.len() > 100);
    // Dasselbe Blatt auf A4 quer: andere (skalierte) Stifte, gleiches Ergebnis.
    let a4 = golden_box_on(sheet(Paper::A4, true));
    assert!(sheet_scale(a4.sheet) < 1.0);
    same_content("Golden 2 auf A4 quer", &a4);
    // Von Hand: Pfeile, gedrehte Texte, verlorenes Maß, Nummernkreise.
    let marks = marks_sheet();
    same_content("Markierungen", &marks);
    let svg = svg_read::read(&svg_string(&marks).unwrap());
    let angles: Vec<i64> = svg
        .texts
        .iter()
        .map(|t| t.angle_deg.round() as i64)
        .collect();
    for a in [0, 90, 30] {
        assert!(angles.contains(&a), "{angles:?}");
    }
    assert!(svg.paths.iter().any(|p| p.stroke == "#c00"));
    assert!(svg.texts.iter().any(|t| t.fill == "#c00"));
    assert!(svg.group_ids.iter().any(|g| g == "Oben Positionen"));
    // Splines aller Arten.
    let pairs = same_content("Splines", &spline_sheet());
    let kinds: String = pairs.iter().map(|(s, _)| s.kind()).collect();
    for k in ['L', 'Q', 'C', 'A'] {
        assert!(kinds.contains(k), "{kinds}");
    }
    // Grad 2 ohne Gewichte: SVG `Q`, PDF eine Kubik mit den angehobenen Steuerpunkten.
    for (s, p) in &pairs {
        if let (svg_read::Seg::Quad { a, c, b }, pdf_read::Seg::Cubic(_, c1, c2, _)) = (s, p) {
            let want1 = (a.0 + 2.0 / 3.0 * (c.0 - a.0), a.1 + 2.0 / 3.0 * (c.1 - a.1));
            let want2 = (b.0 + 2.0 / 3.0 * (c.0 - b.0), b.1 + 2.0 / 3.0 * (c.1 - b.1));
            assert!(near(mm_pt(*c1), want1, 1e-4) && near(mm_pt(*c2), want2, 1e-4));
        }
        if let svg_read::Seg::Cubic { c1, c2, .. } = s {
            let pdf_read::Seg::Cubic(_, d1, d2, _) = p else {
                panic!("Kubik erwartet: {p:?}");
            };
            assert!(near(mm_pt(*d1), *c1, 1e-4) && near(mm_pt(*d2), *c2, 1e-4));
        }
        match s.kind() {
            'L' => assert!(matches!(p, pdf_read::Seg::Line(..)), "{p:?}"),
            _ => assert!(matches!(p, pdf_read::Seg::Cubic(..)), "{p:?}"),
        }
    }
}

// ---------------------------------------------------------------------------------------------
// (b) Bézier-Stücke folgen den exakten Bögen
// ---------------------------------------------------------------------------------------------

/// Exakter Kegelschnitt des Modells: Mitte, Hauptachse, Nebenachse (Länge).
#[derive(Clone, Copy, Debug)]
struct Conic {
    c: DVec2,
    major: DVec2,
    minor_len: f64,
}

impl Conic {
    fn r(&self) -> f64 {
        self.major.length()
    }

    /// Abstand eines Punkts (mm) zum ganzen Kreis bzw. zur ganzen Ellipse.
    fn distance(&self, p: (f64, f64)) -> f64 {
        let q = v(p.0, p.1) - self.c;
        let a = self.major.length();
        let b = self.minor_len;
        if (a - b).abs() <= 1e-12 * a {
            return (q.length() - a).abs();
        }
        let u = self.major / a;
        let (x, y) = (q.dot(u), q.dot(v(-u.y, u.x)));
        let f = |t: f64| (x - a * t.cos()).hypot(y - b * t.sin());
        const N: usize = 1440;
        let (mut best, mut bi) = (f64::INFINITY, 0);
        for i in 0..N {
            let d = f(TAU * i as f64 / N as f64);
            if d < best {
                (best, bi) = (d, i);
            }
        }
        let (mut lo, mut hi) = (
            TAU * (bi as f64 - 1.0) / N as f64,
            TAU * (bi as f64 + 1.0) / N as f64,
        );
        let g = 0.5 * (5f64.sqrt() - 1.0);
        for _ in 0..100 {
            let m1 = hi - g * (hi - lo);
            let m2 = lo + g * (hi - lo);
            if f(m1) < f(m2) {
                hi = m2;
            } else {
                lo = m1;
            }
        }
        best.min(f(0.5 * (lo + hi)))
    }
}

/// Alle Kreise und Ellipsen des Modells (Linien, Markierungen der Ansichten und des Rahmens).
fn conics(model: &SheetModel) -> Vec<Conic> {
    let mut out = Vec::new();
    let mark = |out: &mut Vec<Conic>, m: &Mark| {
        if let Mark::Arc { center, r, .. } = m {
            out.push(Conic {
                c: *center,
                major: v(r.abs(), 0.0),
                minor_len: r.abs(),
            });
        }
    };
    if let Some(f) = &model.frame {
        for m in &f.marks {
            mark(&mut out, m);
        }
    }
    for view in &model.views {
        for l in &view.lines {
            match &l.curve {
                Out2d::Arc { c, r, .. } => out.push(Conic {
                    c: *c,
                    major: v(r.abs(), 0.0),
                    minor_len: r.abs(),
                }),
                Out2d::Ellipse {
                    c, major, ratio, ..
                } => out.push(Conic {
                    c: *c,
                    major: *major,
                    minor_len: ratio.abs() * major.length(),
                }),
                _ => {}
            }
        }
        for m in &view.marks {
            mark(&mut out, m);
        }
    }
    out
}

/// Prüft alle Bogenstücke; liefert ihre Zahl.
fn arcs_follow(what: &str, model: &SheetModel) -> usize {
    let all = conics(model);
    let mut n = 0;
    for (s, p) in same_content(what, model) {
        if s.kind() != 'A' {
            continue;
        }
        let pdf_read::Seg::Cubic(..) = p else {
            panic!("{what}: Bogen nicht als Kubik: {p:?}");
        };
        // Der exakte Bogen: der Kegelschnitt des Modells durch Anfang, Mitte und Ende des
        // SVG-Bogens.
        let probe = [s.start(), s.point_at(0.5), s.end()];
        let conic = all
            .iter()
            .map(|c| {
                let d = probe.iter().map(|&q| c.distance(q)).fold(0.0, f64::max);
                (d, *c)
            })
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .filter(|(d, _)| *d <= 1e-5)
            .unwrap_or_else(|| panic!("{what}: kein Bogen im Modell für {probe:?}"))
            .1;
        let tol = 3e-4 * conic.r() + 1e-6;
        for k in 0..=50 {
            let q = mm_pt(p.point_at(f64::from(k) / 50.0));
            let d = conic.distance(q);
            assert!(d <= tol, "{what}: Abweichung {d} > {tol} (r {})", conic.r());
        }
        n += 1;
    }
    n
}

#[test]
fn beziers_follow_the_arcs() {
    // Die Ellipsen der Isometrie: 6 Bogenstücke.
    assert_eq!(arcs_follow("Zylinder", &cylinder_sheet(true)), 6);
    // Kreis der Draufsicht (4 Stücke) und Ellipsen der Isometrie (6 Stücke).
    assert_eq!(arcs_follow("Golden 1", &golden_cylinder()), 10);
    assert_eq!(arcs_follow("Golden 2", &golden_box()), 20);
    // Ellipsen und Bögen über 0° hinweg.
    assert_eq!(arcs_follow("Splines", &spline_sheet()), 10);
}

// ---------------------------------------------------------------------------------------------
// (c) Goldene PDF-Dateien
// ---------------------------------------------------------------------------------------------

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

/// Erste Stelle, an der sich zwei Byte-Folgen unterscheiden.
fn first_diff(a: &[u8], b: &[u8]) -> Option<usize> {
    a.iter()
        .zip(b)
        .position(|(x, y)| x != y)
        .or((a.len() != b.len()).then_some(a.len().min(b.len())))
}

#[test]
fn golden_files() {
    let update = std::env::var("PLANORA_UPDATE_PDF_GOLDEN").is_ok_and(|v| v == "1");
    // Ohne diese Zeile in `.gitattributes` würde `core.autocrlf` die Dateien verderben.
    let attrs = std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.gitattributes"),
    )
    .expect(".gitattributes");
    assert!(
        attrs
            .lines()
            .any(|l| l.trim() == "crates/planora-drawing/tests/fixtures/*.pdf -text"),
        ".gitattributes ohne PDF-Zeile"
    );
    let meta = golden_meta();
    let cases = [
        (
            "pdf_cylinder_a4_landscape.pdf",
            golden_cylinder(),
            [841.8898, 595.2756],
        ),
        (
            "pdf_box_frame_a3_portrait.pdf",
            golden_box(),
            [841.8898, 1190.5512],
        ),
    ];
    for (name, model, size) in cases {
        let bytes = pdf_bytes(&[&model], &meta).unwrap();
        // Gleiche Eingabe, gleiche Bytes.
        assert_eq!(bytes, pdf_bytes(&[&model], &meta).unwrap());
        let path = fixture(name);
        if update {
            std::fs::write(&path, &bytes).unwrap();
        }
        let stored = std::fs::read(&path).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert!(
            !stored.contains(&b'\r'),
            "{name}: Wagenrücklauf in der Fixtur"
        );
        if let Some(i) = first_diff(&stored, &bytes) {
            panic!(
                "{name}: Bytes weichen ab Stelle {i} ab (Fixtur {} Bytes, Schreiber {} Bytes)",
                stored.len(),
                bytes.len()
            );
        }
        let pdf = read(&stored).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(pdf.pages.len(), 1);
        assert_eq!(pdf.pages[0].media_box, [0.0, 0.0, size[0], size[1]]);
        // Nichts ragt über den Blattrand (alle Punkte und Steuerpunkte auf der Seite).
        for item in &pdf.pages[0].items {
            let (Item::Stroke { subpaths, .. } | Item::Fill { subpaths, .. }) = item else {
                continue;
            };
            for seg in subpaths.iter().flatten() {
                let pts = match seg {
                    pdf_read::Seg::Line(a, b) => vec![*a, *b],
                    pdf_read::Seg::Cubic(a, c1, c2, b) => vec![*a, *c1, *c2, *b],
                };
                for (x, y) in pts {
                    assert!(
                        (0.0..=size[0]).contains(&x) && (0.0..=size[1]).contains(&y),
                        "{name}: Punkt ({x}, {y}) außerhalb der Seite"
                    );
                }
            }
        }
        assert_eq!(pdf_read::utf16_text(&pdf.info["Title"]), "Golden");
        assert_eq!(pdf_read::utf16_text(&pdf.info["Author"]), "Prüfer Ä");
        assert_eq!(
            pdf.info["CreationDate"],
            pdf_read::Value::Str(b"D:20261018000000Z".to_vec())
        );
    }
    // Inhalt der Fixturen: Kreise und Ellipsen (Blatt 1), Rahmen mit Sonderzeichen, Stückliste
    // und Nummernkreis (Blatt 2).
    let one = read(&std::fs::read(fixture("pdf_cylinder_a4_landscape.pdf")).unwrap()).unwrap();
    let texts1 = one.pages[0]
        .items
        .iter()
        .filter(|i| matches!(i, Item::Text { .. }))
        .count();
    assert_eq!(texts1, 0);
    let two = read(&std::fs::read(fixture("pdf_box_frame_a3_portrait.pdf")).unwrap()).unwrap();
    let texts2: Vec<&str> = two.pages[0]
        .items
        .iter()
        .filter_map(|i| match i {
            Item::Text { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    for want in [
        "Halter Ø 12 ä ß µ ± ° € – -0,1",
        "Grundplatte Ø 12 ä ß µ ± ° € – -0,1",
        "Welle Ø 12",
        "Scheibe",
        "2",
    ] {
        assert!(
            texts2.iter().any(|t| t.starts_with(want)),
            "{want} fehlt: {texts2:?}"
        );
    }
    assert!(
        !update,
        "Golden-Dateien neu geschrieben: ohne PLANORA_UPDATE_PDF_GOLDEN=1 erneut prüfen"
    );
}

// ---------------------------------------------------------------------------------------------
// (d) Goldene SVG- und DXF-Dateien unverändert; `svg_splines.svg` (mit dem Stand vor jeder
// Änderung an `fitted_cubics` erzeugt) enthält rationale und höhergradige Splines und schützt so
// die gemeinsame kubische Näherung.
// ---------------------------------------------------------------------------------------------

#[test]
fn golden_svgs_unchanged() {
    let splines = svg_string(&spline_sheet()).unwrap();
    let golden = include_str!("fixtures/svg_splines.svg");
    assert!(!golden.contains('\r'));
    // Die Näherung erzeugt viele Kubiken (Grad 5, rational Grad 2 und 3).
    assert!(golden.matches(" C ").count() > 100);
    assert!(splines == golden, "SVG der Splines hat sich geändert");
    let b = box_sheet(false);
    let c = cylinder_sheet(false);
    assert_eq!(
        svg_string(&b).unwrap(),
        include_str!("fixtures/svg_without_marks_box.svg")
    );
    assert_eq!(
        svg_string(&c).unwrap(),
        include_str!("fixtures/svg_without_marks_cylinder.svg")
    );
    assert_eq!(
        dxf_string(&b).unwrap().as_bytes(),
        include_bytes!("fixtures/dxf_without_marks_box.dxf")
    );
    assert_eq!(
        dxf_string(&c).unwrap().as_bytes(),
        include_bytes!("fixtures/dxf_without_marks_cylinder.dxf")
    );
}
