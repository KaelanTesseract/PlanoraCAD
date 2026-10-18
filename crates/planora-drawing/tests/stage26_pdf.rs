//! Stufe 26: PDF-Schreiber (`pdf.rs`) – Dateiaufbau, Seitengrößen, Dokumenteigenschaften,
//! Linien, Markierungen, Texte und Kurven, geprüft mit dem eigenen Leser `common::pdf_read`.

mod common;

use common::pdf_read::{
    self, Item, Op, Seg, Value, all_chars, mm, read, text_width_pt, utf16_text,
};
use common::svg_marks::manual_sheet;
use common::svg_read;
use planora_drawing::annotate::{Mark, MarkKind, TextAnchor, text_width};
use planora_drawing::balloon::PositionPen;
use planora_drawing::model::{Paper, Sheet};
use planora_drawing::out2d::Out2d;
use planora_drawing::pdf::{
    MAX_PAGES, NO_PAGES, PdfMeta, TOO_MANY_PAGES, fits_page_pt, pdf_char_message, pdf_date, pdf_num,
};
use planora_drawing::sheet::{SheetLine, SheetModel, SheetView};
use planora_drawing::sheet_frame::{SheetPen, sheet_scale};
use planora_drawing::sheet_marks::{SheetFrame, sheet_text_width};
use planora_drawing::stroke::StrokeOrigin;
use planora_drawing::{DrawError, ViewId, Visibility, pdf_bytes, svg_string, write_pdf};
use planora_math::{DVec2, PI, TAU};

// ---------------------------------------------------------------------------------------------
// Hilfen
// ---------------------------------------------------------------------------------------------

fn meta() -> PdfMeta {
    PdfMeta {
        title: "Blatt".into(),
        author: None,
        date: None,
    }
}

fn empty(paper: Paper, landscape: bool) -> SheetModel {
    SheetModel {
        sheet: Sheet { paper, landscape },
        frame: None,
        views: Vec::new(),
    }
}

/// A3 quer, eine Ansicht „V“ mit den Linien.
fn lines_sheet(lines: Vec<(Visibility, Out2d)>) -> SheetModel {
    SheetModel {
        sheet: Sheet {
            paper: Paper::A3,
            landscape: true,
        },
        frame: None,
        views: vec![SheetView {
            id: ViewId(1),
            name: "V".into(),
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

fn marks_sheet(paper: Paper, marks: Vec<Mark>) -> SheetModel {
    manual_sheet(paper, vec![("V", marks)])
}

fn bytes_of(models: &[&SheetModel], meta: &PdfMeta) -> Vec<u8> {
    pdf_bytes(models, meta).expect("PDF")
}

fn page_of(model: &SheetModel) -> pdf_read::Page {
    read(&bytes_of(&[model], &meta()))
        .expect("lesbar")
        .pages
        .remove(0)
}

fn text_of(bytes: &[u8]) -> String {
    String::from_utf8(bytes.to_vec()).expect("ASCII")
}

fn v(x: f64, y: f64) -> DVec2 {
    DVec2::new(x, y)
}

fn close(a: f64, b: f64, tol: f64) -> bool {
    (a - b).abs() <= tol
}

fn close_mm(p: pdf_read::P, q: (f64, f64), tol: f64) -> bool {
    close(mm(p.0), q.0, tol) && close(mm(p.1), q.1, tol)
}

struct StrokeRef<'a> {
    rgb: [f64; 3],
    width: f64,
    dash: &'a [f64],
    subpaths: &'a [Vec<Seg>],
}

fn strokes(page: &pdf_read::Page) -> Vec<StrokeRef<'_>> {
    page.items
        .iter()
        .filter_map(|i| match i {
            Item::Stroke {
                rgb,
                width,
                dash,
                subpaths,
            } => Some(StrokeRef {
                rgb: *rgb,
                width: *width,
                dash,
                subpaths,
            }),
            _ => None,
        })
        .collect()
}

struct TextRef<'a> {
    rgb: [f64; 3],
    size: f64,
    matrix: [f64; 6],
    tz: f64,
    text: &'a str,
}

fn texts(page: &pdf_read::Page) -> Vec<TextRef<'_>> {
    page.items
        .iter()
        .filter_map(|i| match i {
            Item::Text {
                rgb,
                size,
                matrix,
                tz,
                text,
            } => Some(TextRef {
                rgb: *rgb,
                size: *size,
                matrix: *matrix,
                tz: *tz,
                text,
            }),
            _ => None,
        })
        .collect()
}

fn op_names(ops: &[Op]) -> Vec<&str> {
    ops.iter().map(|o| o.op.as_str()).collect()
}

fn num_args(op: &Op) -> Vec<f64> {
    op.args.iter().map(|a| a.num().expect("Zahl")).collect()
}

fn all_segs(page: &pdf_read::Page) -> Vec<Seg> {
    strokes(page)
        .iter()
        .flat_map(|s| s.subpaths.iter().flatten().cloned())
        .collect()
}

fn text_mark(pos: DVec2, height: f64, angle: f64, anchor: TextAnchor, text: &str) -> Mark {
    Mark::Text {
        pos,
        height,
        angle,
        anchor,
        text: text.into(),
        kind: MarkKind::Dimension,
    }
}

// ---------------------------------------------------------------------------------------------
// (a) Dateiaufbau
// ---------------------------------------------------------------------------------------------

#[test]
fn file_structure_is_exact() {
    let one = lines_sheet(vec![(
        Visibility::Visible,
        Out2d::Line {
            a: v(10.0, 10.0),
            b: v(110.0, 10.0),
        },
    )]);
    let b = bytes_of(&[&one], &meta());
    assert!(b.starts_with(b"%PDF-1.4\n"));
    assert!(b.iter().all(|&x| x < 0x80), "nur ASCII");
    assert!(b.ends_with(b"%%EOF\n"));
    let pdf = read(&b).expect("lesbar");
    assert_eq!(
        pdf.objects.keys().copied().collect::<Vec<_>>(),
        [1, 2, 3, 4, 5, 6]
    );
    let s = text_of(&b);
    assert!(
        s.contains("1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n"),
        "{s}"
    );
    assert!(s.contains("2 0 obj\n<< /Type /Pages /Kids [5 0 R] /Count 1 >>\nendobj\n"));
    assert!(s.contains(
        "3 0 obj\n<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>\nendobj\n"
    ));
    assert!(s.contains(
        "5 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 1190.5512 841.8898] /Resources << /Font << /F1 3 0 R >> >> /Contents 6 0 R >>\nendobj\n"
    ));
    assert!(s.contains("6 0 obj\n<< /Length "));
    assert!(s.contains("\nxref\n0 7\n0000000000 65535 f \n"));
    assert!(s.contains("trailer\n<< /Size 7 /Root 1 0 R /Info 4 0 R >>\nstartxref\n"));
    // Der Inhaltsstrom beginnt mit runden Enden und Ecken.
    let ops = &pdf.pages[0].ops;
    assert_eq!(ops[0].op, "J");
    assert_eq!(num_args(&ops[0]), [1.0]);
    assert_eq!(ops[1].op, "j");
    assert_eq!(num_args(&ops[1]), [1.0]);

    // Drei Seiten in Eingabereihenfolge.
    let a4 = empty(Paper::A4, false);
    let a0 = empty(Paper::A0, false);
    let b3 = bytes_of(&[&a4, &one, &a0], &meta());
    let pdf3 = read(&b3).expect("lesbar");
    assert_eq!(
        pdf3.objects.keys().copied().collect::<Vec<_>>(),
        (1..=10).collect::<Vec<u32>>()
    );
    let pages = pdf3.objects[&2].dict().unwrap();
    assert_eq!(
        pages["Kids"],
        Value::Arr(vec![Value::Ref(5), Value::Ref(7), Value::Ref(9)])
    );
    assert_eq!(pages["Count"], Value::Num(3.0));
    for i in 0..3u32 {
        let page = pdf3.objects[&(5 + 2 * i)].dict().unwrap();
        assert_eq!(page["Contents"], Value::Ref(6 + 2 * i));
        assert!(matches!(pdf3.objects[&(6 + 2 * i)], Value::Stream(..)));
    }
    let widths: Vec<f64> = pdf3.pages.iter().map(|p| p.media_box[2]).collect();
    assert_eq!(widths, [595.2756, 1190.5512, 2383.937]);
    assert!(text_of(&b3).contains("/Count 3 >>"));
    // Leere Seite: nur die beiden Kopfanweisungen.
    assert_eq!(op_names(&pdf3.pages[0].ops), ["J", "j"]);
    assert!(pdf3.pages[0].items.is_empty());
    assert!(text_of(&b3).contains("<< /Length 7 >>\nstream\n1 J\n1 j\nendstream\n"));

    // Der Leser beißt: ein eingefügtes Byte vor Objekt 3 …
    let at = s.find("3 0 obj\n").unwrap();
    let mut shifted = b.clone();
    shifted.insert(at, b' ');
    assert!(read(&shifted).is_err());
    // … und /Length + 1 (gleiche Stellenzahl, damit nur die Länge falsch ist).
    let lp = s.find("/Length ").unwrap() + "/Length ".len();
    let le = lp + s[lp..].find(' ').unwrap();
    let len: usize = s[lp..le].parse().unwrap();
    let longer = format!("{}{}{}", &s[..lp], len + 1, &s[le..]);
    assert_eq!(
        longer.len(),
        s.len(),
        "Länge mit gleicher Stellenzahl erwartet"
    );
    assert!(read(longer.as_bytes()).is_err());
    // Gegenprobe: unverändert gelingt es.
    assert!(read(s.as_bytes()).is_ok());
}

// ---------------------------------------------------------------------------------------------
// (b) Seitengrößen
// ---------------------------------------------------------------------------------------------

#[test]
fn page_sizes_in_points() {
    let cases = [
        (Paper::A4, false, "[0 0 595.2756 841.8898]"),
        (Paper::A4, true, "[0 0 841.8898 595.2756]"),
        (Paper::A3, true, "[0 0 1190.5512 841.8898]"),
        (Paper::A0, false, "[0 0 2383.937 3370.3937]"),
    ];
    for (paper, landscape, want) in cases {
        let m = empty(paper, landscape);
        let b = bytes_of(&[&m], &meta());
        assert!(
            text_of(&b).contains(&format!("/MediaBox {want}")),
            "{paper:?} {landscape}"
        );
        let mb = read(&b).unwrap().pages[0].media_box;
        let nums: Vec<f64> = want
            .trim_matches(['[', ']'])
            .split(' ')
            .map(|x| x.parse().unwrap())
            .collect();
        assert_eq!(mb.to_vec(), nums);
    }
    for paper in [Paper::A4, Paper::A3, Paper::A2, Paper::A1, Paper::A0] {
        for landscape in [false, true] {
            let (w, h) = Sheet { paper, landscape }.size_mm();
            let k = 72.0 / 25.4;
            assert!(fits_page_pt(w * k, h * k));
        }
    }
    assert!(fits_page_pt(14400.0, 1.0));
    assert!(fits_page_pt(1.0, 14400.0));
    assert!(!fits_page_pt(14400.0001, 1.0));
    assert!(!fits_page_pt(1.0, 14400.0001));
    assert!(!fits_page_pt(f64::NAN, 1.0));
}

// ---------------------------------------------------------------------------------------------
// (c) Dokumenteigenschaften und Wiederholbarkeit
// ---------------------------------------------------------------------------------------------

#[test]
fn info_and_identity() {
    let m = lines_sheet(vec![(
        Visibility::Visible,
        Out2d::Line {
            a: v(10.0, 10.0),
            b: v(110.0, 10.0),
        },
    )]);
    let title = "Grundplatte ÄÖÜ ß Ø 😀";
    let full = PdfMeta {
        title: title.into(),
        author: Some("Prüfer".into()),
        date: Some("2026-10-18".into()),
    };
    let b = bytes_of(&[&m], &full);
    let pdf = read(&b).unwrap();
    assert_eq!(
        pdf.info.keys().map(String::as_str).collect::<Vec<_>>(),
        ["Author", "CreationDate", "Creator", "Producer", "Title"]
    );
    assert_eq!(utf16_text(&pdf.info["Title"]), title);
    assert_eq!(utf16_text(&pdf.info["Author"]), "Prüfer");
    assert_eq!(
        pdf.info["CreationDate"],
        Value::Str(b"D:20261018000000Z".to_vec())
    );
    assert_eq!(pdf.info["Producer"], Value::Str(b"PlanoraCAD".to_vec()));
    assert_eq!(pdf.info["Creator"], Value::Str(b"PlanoraCAD".to_vec()));
    let s = text_of(&b);
    assert!(s.contains("/CreationDate (D:20261018000000Z)"));
    // UTF-16BE mit BOM, das Emoji als Ersatzpaar.
    assert!(s.contains("/Title <FEFF0047"), "{s}");
    assert!(s.contains("D83DDE00>"), "{s}");
    assert!(s.contains("/Author <FEFF0050007200FC006600650072>"), "{s}");
    assert!(!s.contains("/ID"));
    assert!(!s.contains("/ModDate"));

    // Ohne Ersteller, ohne Titel, ohne gültiges Datum.
    for author in [None, Some(String::new())] {
        let b = bytes_of(
            &[&m],
            &PdfMeta {
                author,
                ..full.clone()
            },
        );
        assert!(!read(&b).unwrap().info.contains_key("Author"));
    }
    let b = bytes_of(
        &[&m],
        &PdfMeta {
            title: String::new(),
            ..full.clone()
        },
    );
    let info = read(&b).unwrap().info;
    assert!(!info.contains_key("Title"));
    assert!(info.contains_key("Author"));
    for bad in [
        "18.10.2026",
        "2026-13-01",
        "2026-10-32",
        "2026-00-10",
        "2026-10-00",
        "",
    ] {
        let b = bytes_of(
            &[&m],
            &PdfMeta {
                date: Some(bad.into()),
                ..full.clone()
            },
        );
        assert!(
            !read(&b).unwrap().info.contains_key("CreationDate"),
            "{bad}"
        );
        assert_eq!(pdf_date(bad), None, "{bad}");
    }
    let b = bytes_of(
        &[&m],
        &PdfMeta {
            date: None,
            ..full.clone()
        },
    );
    assert!(!read(&b).unwrap().info.contains_key("CreationDate"));
    assert_eq!(pdf_date("2026-10-18").as_deref(), Some("D:20261018000000Z"));
    assert_eq!(pdf_date("1999-01-31").as_deref(), Some("D:19990131000000Z"));
    for bad in [
        "2026-1-018",
        "2026/10/18",
        "2026-10-1a",
        "+026-10-18",
        "2026-10-18 ",
    ] {
        assert_eq!(pdf_date(bad), None, "{bad}");
    }

    // Byte-gleich bei gleicher Eingabe.
    assert_eq!(bytes_of(&[&m], &full), bytes_of(&[&m], &full));
    // Anderer Titel: nur Info (und damit die Versätze) anders.
    let other = bytes_of(
        &[&m],
        &PdfMeta {
            title: "Deckel".into(),
            ..full.clone()
        },
    );
    assert_ne!(other, b);
    let (p1, p2) = (
        read(&bytes_of(&[&m], &full)).unwrap(),
        read(&other).unwrap(),
    );
    for (k, val) in &p1.objects {
        if *k == 4 {
            assert_ne!(val, &p2.objects[k]);
        } else {
            assert_eq!(val, &p2.objects[k], "Objekt {k}");
        }
    }
}

// ---------------------------------------------------------------------------------------------
// (d) Linien der Ansichten
// ---------------------------------------------------------------------------------------------

#[test]
fn view_lines_widths_and_dashes() {
    let vis = (
        Visibility::Visible,
        Out2d::Line {
            a: v(10.0, 10.0),
            b: v(110.0, 10.0),
        },
    );
    let hid = (
        Visibility::Hidden,
        Out2d::Line {
            a: v(10.0, 20.0),
            b: v(110.0, 20.0),
        },
    );
    let m = lines_sheet(vec![hid.clone(), vis.clone()]);
    let b = bytes_of(&[&m], &meta());
    let page = read(&b).unwrap().pages.remove(0);
    let st = strokes(&page);
    assert_eq!(st.len(), 2);
    assert_eq!(page.items.len(), 2);
    assert_eq!(st[0].rgb, [0.0; 3]);
    assert_eq!(st[0].width, 1.4173);
    assert!(st[0].dash.is_empty());
    assert_eq!(
        st[0].subpaths,
        [vec![Seg::Line((28.3465, 28.3465), (311.811, 28.3465))]]
    );
    assert_eq!(st[1].rgb, [0.0; 3]);
    assert_eq!(st[1].width, 0.7087);
    assert_eq!(st[1].dash, [8.5039, 2.8346]);
    assert_eq!(
        st[1].subpaths,
        [vec![Seg::Line((28.3465, 56.6929), (311.811, 56.6929))]]
    );
    let s = text_of(&b);
    assert!(
        s.contains(
            "1 J\n1 j\n0 0 0 RG\n1.4173 w\n[] 0 d\n28.3465 28.3465 m\n311.811 28.3465 l\nS\n0 0 0 RG\n0.7087 w\n[8.5039 2.8346] 0 d\n28.3465 56.6929 m\n311.811 56.6929 l\nS\nendstream"
        ),
        "{s}"
    );

    // Ohne verdeckte Linien kein zweiter Strich; mehrere sichtbare Linien in einem Strich.
    let m = lines_sheet(vec![vis.clone(), vis]);
    let page = page_of(&m);
    let st = strokes(&page);
    assert_eq!(st.len(), 1);
    assert_eq!(st[0].subpaths.len(), 2);
    // Ohne sichtbare Linien nur der verdeckte Strich.
    let page = page_of(&lines_sheet(vec![hid]));
    let st = strokes(&page);
    assert_eq!(st.len(), 1);
    assert_eq!(st[0].width, 0.7087);
}

// ---------------------------------------------------------------------------------------------
// (e) Stifte der Markierungen
// ---------------------------------------------------------------------------------------------

fn frame_lines(scale: f64) -> SheetFrame {
    let line = |y: f64, pen: SheetPen| Mark::Line {
        a: v(20.0, y),
        b: v(200.0, y),
        kind: MarkKind::Sheet(pen),
    };
    SheetFrame {
        marks: vec![
            line(10.0, SheetPen::Heavy),
            line(20.0, SheetPen::Medium),
            line(30.0, SheetPen::Thin),
        ],
        scale,
    }
}

#[test]
fn mark_pens() {
    let line = |y: f64, kind: MarkKind| Mark::Line {
        a: v(50.0, y),
        b: v(150.0, y),
        kind,
    };
    // Absichtlich gemischt: die Ausgabe ordnet wie SVG (Maße, Mittellinien, Positionen).
    let marks = vec![
        line(100.0, MarkKind::Position(PositionPen::Leader)),
        line(110.0, MarkKind::CenterLine),
        Mark::Arc {
            center: v(200.0, 200.0),
            r: 4.0,
            start: 0.0,
            end: TAU,
            kind: MarkKind::Position(PositionPen::Circle),
        },
        line(120.0, MarkKind::Dimension),
        line(130.0, MarkKind::Lost),
    ];
    let mut m = marks_sheet(Paper::A3, marks.clone());
    m.frame = Some(frame_lines(1.0));
    let page = page_of(&m);
    let st = strokes(&page);
    let got: Vec<(f64, Vec<f64>, [f64; 3])> = st
        .iter()
        .map(|s| (s.width, s.dash.to_vec(), s.rgb))
        .collect();
    let black = [0.0; 3];
    assert_eq!(
        got,
        [
            (1.9843, vec![], black),
            (1.4173, vec![], black),
            (0.7087, vec![], black),
            (0.7087, vec![], black),
            (0.7087, vec![], [0.8, 0.0, 0.0]),
            (0.7087, vec![34.0157, 8.5039, 5.6693, 8.5039], black),
            (0.7087, vec![], black),
            (0.9921, vec![], black),
        ]
    );
    // Reihenfolge an den Lagen geprüft: Rahmen 10/20/30, dann 120 (Maß), 130 (verloren),
    // 110 (Mittellinie), 100 (Hinweislinie), Kreis.
    let ys: Vec<f64> = st[..7]
        .iter()
        .map(|s| mm(s.subpaths[0][0].start().1))
        .collect();
    for (y, want) in ys
        .iter()
        .zip([10.0, 20.0, 30.0, 120.0, 130.0, 110.0, 100.0])
    {
        assert!(close(*y, want, 1e-4), "{ys:?}");
    }

    // A4: Faktor 0,7071; dünne Linie auf die Mindeststärke 0,18 mm angehoben.
    let mut m4 = marks_sheet(Paper::A4, marks.clone());
    let s4 = sheet_scale(m4.sheet);
    assert!(close(s4, 297.0 / 420.0, 1e-12));
    m4.frame = Some(frame_lines(s4));
    let page = page_of(&m4);
    let w: Vec<f64> = strokes(&page).iter().map(|s| s.width).collect();
    assert_eq!(w[..3], [1.4031, 1.0022, 0.5102]);
    // Nummernkreis mit dem Faktor des Blatts: 0,35 · 0,7071 mm.
    assert_eq!(w[7], 0.7016);
    assert_eq!(w[6], 0.5102);

    // Rahmenmarken folgen `frame.scale`, Ansichtsmarken dem Blatt.
    let mut m2 = marks_sheet(Paper::A3, marks);
    m2.frame = Some(frame_lines(2.0));
    let page = page_of(&m2);
    let w: Vec<f64> = strokes(&page).iter().map(|s| s.width).collect();
    assert_eq!(w[..3], [3.9685, 2.8346, 1.4173]);
    assert_eq!(w[7], 0.9921);

    // Ohne Rahmen keine Rahmenstriche.
    let page = page_of(&marks_sheet(Paper::A3, Vec::new()));
    assert!(page.items.is_empty());
}

// ---------------------------------------------------------------------------------------------
// (f) Pfeile
// ---------------------------------------------------------------------------------------------

#[test]
fn arrows_are_filled_triangles() {
    let arrow = |kind| Mark::Arrow {
        tip: v(50.0, 50.0),
        dir: v(1.0, 0.0),
        kind,
    };
    let page = page_of(&marks_sheet(Paper::A3, vec![arrow(MarkKind::Dimension)]));
    assert_eq!(page.items.len(), 1);
    let Item::Fill { rgb, subpaths } = &page.items[0] else {
        panic!("Fläche erwartet: {:?}", page.items);
    };
    assert_eq!(*rgb, [0.0; 3]);
    assert_eq!(subpaths.len(), 1);
    let segs = &subpaths[0];
    assert_eq!(segs.len(), 3, "{segs:?}");
    let corners = [(50.0, 50.0), (53.5, 50.6), (53.5, 49.4)];
    for (s, c) in segs.iter().zip(corners) {
        assert!(matches!(s, Seg::Line(..)));
        assert!(close_mm(s.start(), c, 1e-4), "{s:?} {c:?}");
    }
    assert_eq!(segs[2].end(), segs[0].start());
    assert_eq!(
        op_names(&page.ops),
        ["J", "j", "rg", "m", "l", "l", "h", "f"]
    );
    // Verlorene Pfeile rot.
    let page = page_of(&marks_sheet(Paper::A3, vec![arrow(MarkKind::Lost)]));
    let Item::Fill { rgb, .. } = &page.items[0] else {
        panic!();
    };
    assert_eq!(*rgb, [0.8, 0.0, 0.0]);
}

// ---------------------------------------------------------------------------------------------
// (g) Texte: Anker und Drehung
// ---------------------------------------------------------------------------------------------

fn anchor_k(a: TextAnchor) -> f64 {
    match a {
        TextAnchor::Start => 0.0,
        TextAnchor::Middle => 0.5,
        TextAnchor::End => 1.0,
    }
}

#[test]
fn text_anchor_and_rotation() {
    // „Ä“ als Blatttext (Schätzung 0,72·h, breiter als Helvetica: nicht gestaucht), die übrigen
    // als Maßtexte; „WM@“ ist breiter als die Schätzung und wird gestaucht (Mitte nach der
    // Stauchung).
    let sheet = MarkKind::Sheet(SheetPen::Thin);
    let dim = MarkKind::Dimension;
    let cases = [
        (v(100.0, 100.0), 0.0, TextAnchor::Middle, "Ä", sheet),
        (v(50.0, 50.0), 0.5 * PI, TextAnchor::End, "10", dim),
        (v(20.0, 30.0), 0.0, TextAnchor::Start, "x", dim),
        (
            v(150.0, 120.0),
            30f64.to_radians(),
            TextAnchor::Middle,
            "Maß 12,5",
            dim,
        ),
        (
            v(220.0, 80.0),
            -45f64.to_radians(),
            TextAnchor::End,
            "Ø 40 H7",
            dim,
        ),
        (v(260.0, 160.0), PI, TextAnchor::Middle, "WM@", dim),
        (v(200.0, 40.0), 0.7, TextAnchor::End, "@@@", sheet),
    ];
    let marks: Vec<Mark> = cases
        .iter()
        .map(|&(pos, angle, anchor, t, kind)| Mark::Text {
            pos,
            height: 3.5,
            angle,
            anchor,
            text: t.into(),
            kind,
        })
        .collect();
    let b = bytes_of(&[&marks_sheet(Paper::A3, marks)], &meta());
    let page = read(&b).unwrap().pages.remove(0);
    let tx = texts(&page);
    assert_eq!(tx.len(), cases.len());
    assert_eq!(tx[0].size, 9.9213);
    assert_eq!(tx[0].text, "Ä");
    assert_eq!(tx[0].matrix, [1.0, 0.0, 0.0, 1.0, 280.1558, 283.4646]);
    assert_eq!(tx[1].matrix, [0.0, 1.0, -1.0, 0.0, 141.7323, 130.6998]);
    assert_eq!(tx[2].matrix[4], 56.6929);
    assert_eq!(tx[2].matrix[5], 85.0394);
    assert!(
        text_of(&b).contains(
            "0 0 0 rg\nBT\n/F1 9.9213 Tf\n1 0 0 1 280.1558 283.4646 Tm\n(\\304) Tj\nET\n"
        )
    );
    assert!(tx[5].tz < 100.0 && tx[6].tz < 100.0);
    for (t, &(p, angle, anc, s, _)) in tx.iter().zip(&cases) {
        assert_eq!(t.text, s);
        assert_eq!(t.rgb, [0.0; 3]);
        let [a, bb, c, d, e, f] = t.matrix;
        assert!(
            close(a, angle.cos(), 1e-4) && close(bb, angle.sin(), 1e-4),
            "{s}"
        );
        assert!(
            close(c, -angle.sin(), 1e-4) && close(d, angle.cos(), 1e-4),
            "{s}"
        );
        // Zurückgerechnet mit dem ungerundeten Winkel (die Matrix trägt cos/sin auf 4 Stellen).
        let w = text_width_pt(t.text, t.size, t.tz);
        let k = anchor_k(anc);
        let back = (e + k * w * angle.cos(), f + k * w * angle.sin());
        assert!(close_mm(back, (p.x, p.y), 1e-4), "{s}: {back:?}");
    }
}

// ---------------------------------------------------------------------------------------------
// (h) Stauchung: nie breiter als die Schätzung des Blattmodells
// ---------------------------------------------------------------------------------------------

fn tz_ops_around_tj(ops: &[Op]) -> Vec<Vec<String>> {
    // Je Textblock die Folge der Anweisungen mit Tz-Werten.
    let mut out = Vec::new();
    let mut cur: Option<Vec<String>> = None;
    for o in ops {
        match o.op.as_str() {
            "BT" => cur = Some(Vec::new()),
            "ET" => out.push(cur.take().unwrap()),
            "Tz" => cur.as_mut().unwrap().push(format!("Tz {}", num_args(o)[0])),
            "Tj" => cur.as_mut().unwrap().push("Tj".into()),
            _ => {}
        }
    }
    out
}

#[test]
fn text_compressed_never_wider() {
    let kind_text = |h: f64, s: &str, kind: MarkKind| Mark::Text {
        pos: v(30.0, 30.0),
        height: h,
        angle: 0.0,
        anchor: TextAnchor::Start,
        text: s.into(),
        kind,
    };
    let sheet_text = |h: f64, s: &str| kind_text(h, s, MarkKind::Sheet(SheetPen::Thin));
    let page = page_of(&marks_sheet(
        Paper::A3,
        vec![
            sheet_text(10.0, "@@@@"),
            sheet_text(10.0, "WWW"),
            text_mark(v(30.0, 30.0), 3.5, 0.0, TextAnchor::Start, "Ø20"),
        ],
    ));
    // Siehe Bericht: der Faktor wird auf vier Stellen abgerundet und mit der geschriebenen
    // (gerundeten) Schriftgröße bestimmt, damit die gerenderte Breite nie größer wird.
    assert_eq!(
        tz_ops_around_tj(&page.ops),
        [
            vec!["Tz 93.5959".to_string(), "Tj".into(), "Tz 100".into()],
            vec!["Tj".to_string()],
            vec!["Tz 95.2377".to_string(), "Tj".into(), "Tz 100".into()],
        ]
    );
    let tx = texts(&page);
    assert_eq!(tx[0].tz, 93.5959);
    assert_eq!(tx[1].tz, 100.0);
    // Die zuletzt gesetzte Stauchung gilt nicht für den nächsten Text.
    assert_eq!(tx[2].tz, 95.2377);

    // Jedes zulässige Zeichen einzeln und als Folge, in beiden Arten.
    let mut chars = all_chars();
    chars.push('\u{2212}');
    // Positionstexte stehen in der Ausgabe nach den Maßen (Gruppenfolge wie SVG).
    let mut marks = Vec::new();
    let mut want = Vec::new();
    let mut pos_marks = Vec::new();
    let mut pos_want = Vec::new();
    for &c in &chars {
        for s in [c.to_string(), c.to_string().repeat(7)] {
            marks.push(sheet_text(5.0, &s));
            want.push(sheet_text_width(&s, 5.0));
            marks.push(text_mark(v(30.0, 30.0), 5.0, 0.0, TextAnchor::Start, &s));
            want.push(text_width(&s, 5.0));
            pos_marks.push(kind_text(5.0, &s, MarkKind::Position(PositionPen::Circle)));
            pos_want.push(sheet_text_width(&s, 5.0));
        }
    }
    marks.extend(pos_marks);
    want.extend(pos_want);
    let page = page_of(&marks_sheet(Paper::A3, marks));
    let tx = texts(&page);
    assert_eq!(tx.len(), want.len());
    let mut squeezed = 0;
    for (t, w) in tx.iter().zip(&want) {
        let rendered = mm(text_width_pt(t.text, t.size, t.tz));
        assert!(
            rendered <= w + 1e-9,
            "{:?} (Größe {}, tz {}): {rendered} > {w}",
            t.text,
            t.size,
            t.tz
        );
        if t.tz < 100.0 {
            squeezed += 1;
            // Gestaucht wird nur so weit wie nötig (Abrundung auf vier Stellen).
            let full = mm(text_width_pt(t.text, t.size, 100.0));
            assert!(full > *w);
            assert!(rendered >= w - full * 2e-6, "{:?}", t.text);
        } else {
            assert_eq!(t.tz, 100.0);
        }
    }
    assert!(squeezed > 20, "{squeezed}");
}

// ---------------------------------------------------------------------------------------------
// (i) Bytes der Texte
// ---------------------------------------------------------------------------------------------

#[test]
fn text_bytes() {
    let marks = vec![
        text_mark(v(30.0, 30.0), 3.5, 0.0, TextAnchor::Start, "(a)\\"),
        text_mark(v(30.0, 40.0), 3.5, 0.0, TextAnchor::Start, "äÖß€…–"),
        text_mark(v(30.0, 50.0), 3.5, 0.0, TextAnchor::Start, "\u{2212}0,1"),
        text_mark(
            v(30.0, 60.0),
            3.5,
            0.0,
            TextAnchor::Start,
            "Ø 12 ± 0,1 20° µ",
        ),
    ];
    let b = bytes_of(&[&marks_sheet(Paper::A3, marks)], &meta());
    assert!(b.iter().all(|&x| x < 0x80));
    let s = text_of(&b);
    assert!(s.contains("(\\(a\\)\\\\) Tj\n"), "{s}");
    assert!(s.contains("(\\344\\326\\337\\200\\205\\226) Tj\n"), "{s}");
    assert!(s.contains("(-0,1) Tj\n"), "{s}");
    assert!(s.contains("(\\330 12 \\261 0,1 20\\260 \\265) Tj\n"), "{s}");
    let page = read(&b).unwrap().pages.remove(0);
    let got: Vec<&str> = texts(&page).iter().map(|t| t.text).collect();
    assert_eq!(got, ["(a)\\", "äÖß€…–", "-0,1", "Ø 12 ± 0,1 20° µ"]);
}

// ---------------------------------------------------------------------------------------------
// (j) Kurven als Bézier-Stücke
// ---------------------------------------------------------------------------------------------

fn arc(c: DVec2, r: f64, start: f64, end: f64) -> Out2d {
    Out2d::Arc { c, r, start, end }
}

fn spline(degree: usize, knots: Vec<f64>, ctrl: &[(f64, f64)], weights: Vec<f64>) -> Out2d {
    Out2d::Spline {
        degree,
        knots,
        ctrl: ctrl.iter().map(|&(x, y)| v(x, y)).collect(),
        weights,
    }
}

fn one_curve(curve: Out2d) -> pdf_read::Page {
    page_of(&lines_sheet(vec![(Visibility::Visible, curve)]))
}

fn cubic_points(s: &Seg) -> [(f64, f64); 4] {
    let Seg::Cubic(a, b, c, d) = s else {
        panic!("Kubik erwartet: {s:?}");
    };
    [a, b, c, d].map(|p| (mm(p.0), mm(p.1)))
}

/// Anfangs- und Endpunkt eines Pfadstücks (mm).
type Ends = ((f64, f64), (f64, f64));

/// Endpunkte der Pfadstücke der SVG-Ausgabe derselben Kurve (mm).
fn svg_ends(curve: Out2d) -> Vec<Ends> {
    let m = lines_sheet(vec![(Visibility::Visible, curve)]);
    let svg = svg_read::read(&svg_string(&m).unwrap());
    svg.paths_of("V")
        .flat_map(|p| p.segments.iter().map(|s| (s.start(), s.end())))
        .collect()
}

fn pdf_ends(page: &pdf_read::Page) -> Vec<Ends> {
    all_segs(page)
        .iter()
        .map(|s| {
            let (a, b) = (s.start(), s.end());
            ((mm(a.0), mm(a.1)), (mm(b.0), mm(b.1)))
        })
        .collect()
}

fn same_ends(a: &[Ends], b: &[Ends]) {
    assert_eq!(a.len(), b.len(), "{a:?} {b:?}");
    for (x, y) in a.iter().zip(b) {
        for (p, q) in [(x.0, y.0), (x.1, y.1)] {
            assert!(
                close(p.0, q.0, 1e-4) && close(p.1, q.1, 1e-4),
                "{a:?} {b:?}"
            );
        }
    }
}

#[test]
fn curves_as_beziers() {
    let c = v(100.0, 100.0);
    // Vollkreis: ein Teilpfad, vier Kubiken, Ende exakt am Anfang.
    let page = one_curve(arc(c, 10.0, 0.0, TAU));
    let st = strokes(&page);
    assert_eq!(st.len(), 1);
    assert_eq!(st[0].subpaths.len(), 1);
    let segs = &st[0].subpaths[0];
    assert_eq!(segs.len(), 4);
    assert!(segs.iter().all(|s| matches!(s, Seg::Cubic(..))));
    assert_eq!(segs[3].end(), segs[0].start());
    for w in segs.windows(2) {
        assert_eq!(w[0].end(), w[1].start());
    }

    // Viertelbogen 0°–90°: κ = 4/3·tan(π/8).
    let page = one_curve(arc(c, 10.0, 0.0, 0.5 * PI));
    let segs = all_segs(&page);
    assert_eq!(segs.len(), 1);
    let want = [
        (110.0, 100.0),
        (110.0, 105.5228),
        (105.5228, 110.0),
        (100.0, 110.0),
    ];
    for (p, q) in cubic_points(&segs[0]).iter().zip(want) {
        assert!(
            close(p.0, q.0, 1e-4) && close(p.1, q.1, 1e-4),
            "{p:?} {q:?}"
        );
    }
    // Jeder Punkt eines Kreisstücks liegt fast auf dem Kreis (Fehler der Näherung ≈ 2,7e-4·r).
    for seg in all_segs(&one_curve(arc(c, 10.0, 0.3, 0.3 + 1.5))) {
        for k in 0..=50 {
            let p = seg.point_at(f64::from(k) / 50.0);
            let d = (mm(p.0) - 100.0).hypot(mm(p.1) - 100.0);
            assert!(close(d, 10.0, 3e-4 * 10.0), "{d}");
        }
    }

    // 100° → zwei Stücke; −30°…30° → ein Stück, Endpunkte wie SVG.
    assert_eq!(
        all_segs(&one_curve(arc(c, 10.0, 0.0, 100f64.to_radians()))).len(),
        2
    );
    assert_eq!(
        all_segs(&one_curve(arc(c, 10.0, 0.0, 90.001f64.to_radians()))).len(),
        2
    );
    assert_eq!(
        all_segs(&one_curve(arc(c, 10.0, 0.0, 180f64.to_radians()))).len(),
        2
    );
    assert_eq!(
        all_segs(&one_curve(arc(c, 10.0, 0.0, 181f64.to_radians()))).len(),
        3
    );
    let over0 = arc(c, 10.0, -PI / 6.0, PI / 6.0);
    let page = one_curve(over0.clone());
    assert_eq!(all_segs(&page).len(), 1);
    same_ends(&pdf_ends(&page), &svg_ends(over0));
    // Negativer Radius: dieselben Endpunkte wie SVG.
    let neg = arc(c, -10.0, 0.2, 2.0);
    same_ends(&pdf_ends(&one_curve(neg.clone())), &svg_ends(neg));

    // Ellipse: Punkte auf der Ellipse.
    let ell = Out2d::Ellipse {
        c,
        major: v(20.0, 0.0),
        ratio: 0.5,
        start: 0.0,
        end: 0.5 * PI,
    };
    let page = one_curve(ell.clone());
    let segs = all_segs(&page);
    assert_eq!(segs.len(), 1);
    same_ends(&pdf_ends(&page), &svg_ends(ell));
    for k in 0..50 {
        let p = segs[0].point_at((f64::from(k) + 0.5) / 50.0);
        let (x, y) = (mm(p.0) - 100.0, mm(p.1) - 100.0);
        let t = (y / 10.0).atan2(x / 20.0);
        let d = (x - 20.0 * t.cos()).hypot(y - 10.0 * t.sin());
        assert!(d <= 3e-4 * 20.0, "{d}");
    }
    // Gedrehte Ellipse mit negativem Verhältnis (Umlauf im Uhrzeigersinn) wie SVG.
    let ell2 = Out2d::Ellipse {
        c,
        major: v(12.0, 9.0),
        ratio: -0.4,
        start: 1.0,
        end: 4.5,
    };
    same_ends(&pdf_ends(&one_curve(ell2.clone())), &svg_ends(ell2));

    // Winzige Spanne: ein Stück, nur endliche Zahlen (der Leser lässt nichts anderes zu).
    let page = one_curve(arc(c, 10.0, 0.0, 1e-9));
    let segs = all_segs(&page);
    assert_eq!(segs.len(), 1);
    assert!(matches!(segs[0], Seg::Cubic(..)));

    // Markierungsbogen: r 0 → nichts; Vollkreis → vier Stücke, geschlossen.
    let mk = |r: f64, start: f64, end: f64| Mark::Arc {
        center: c,
        r,
        start,
        end,
        kind: MarkKind::Dimension,
    };
    let page = page_of(&marks_sheet(Paper::A3, vec![mk(0.0, 0.0, 1.0)]));
    assert!(page.items.is_empty());
    let page = page_of(&marks_sheet(Paper::A3, vec![mk(5.0, 1.0, 1.0)]));
    assert!(page.items.is_empty());
    let page = page_of(&marks_sheet(Paper::A3, vec![mk(5.0, 0.0, TAU)]));
    let segs = all_segs(&page);
    assert_eq!(segs.len(), 4);
    assert_eq!(segs[3].end(), segs[0].start());
    let page = page_of(&marks_sheet(Paper::A3, vec![mk(10.0, 0.0, 0.5 * PI)]));
    for (p, q) in cubic_points(&all_segs(&page)[0]).iter().zip(want) {
        assert!(
            close(p.0, q.0, 1e-4) && close(p.1, q.1, 1e-4),
            "{p:?} {q:?}"
        );
    }

    // Spline Grad 1: nur Strecken, ein Teilpfad.
    let page = one_curve(spline(
        1,
        vec![0.0, 0.0, 1.0, 2.0, 2.0],
        &[(10.0, 10.0), (20.0, 10.0), (20.0, 20.0)],
        vec![1.0; 3],
    ));
    assert_eq!(
        op_names(&page.ops),
        ["J", "j", "RG", "w", "d", "m", "l", "l", "S"]
    );
    // Grad 2: Gradanhebung.
    let page = one_curve(spline(
        2,
        vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0],
        &[(10.0, 10.0), (20.0, 40.0), (40.0, 10.0)],
        vec![1.0; 3],
    ));
    let segs = all_segs(&page);
    assert_eq!(segs.len(), 1);
    let want2 = [
        (10.0, 10.0),
        (10.0 + 20.0 / 3.0, 30.0),
        (40.0 - 40.0 / 3.0, 30.0),
        (40.0, 10.0),
    ];
    for (p, q) in cubic_points(&segs[0]).iter().zip(want2) {
        assert!(
            close(p.0, q.0, 1e-4) && close(p.1, q.1, 1e-4),
            "{p:?} {q:?}"
        );
    }
    // Grad 3: dieselben Steuerpunkte.
    let ctrl3 = [(10.0, 10.0), (15.0, 40.0), (35.0, -5.0), (40.0, 20.0)];
    let page = one_curve(spline(
        3,
        vec![0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0],
        &ctrl3,
        vec![1.0; 4],
    ));
    let segs = all_segs(&page);
    assert_eq!(segs.len(), 1);
    for (p, q) in cubic_points(&segs[0]).iter().zip(ctrl3) {
        assert!(
            close(p.0, q.0, 1e-4) && close(p.1, q.1, 1e-4),
            "{p:?} {q:?}"
        );
    }
    // Rationaler Viertelkreis Grad 2 (r 10 um (100,100)): kubische Näherung. Schranke 1e-6 mm
    // für die Näherung plus die Rundung auf 4 Stellen in pt (≤ 0,5e-4·√2 pt ≈ 2,5e-5 mm).
    let h = std::f64::consts::FRAC_1_SQRT_2;
    let page = one_curve(spline(
        2,
        vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0],
        &[(110.0, 100.0), (110.0, 110.0), (100.0, 110.0)],
        vec![1.0, h, 1.0],
    ));
    let segs = all_segs(&page);
    assert!(segs.len() >= 2, "{}", segs.len());
    assert_eq!(st_subpaths(&page), 1);
    assert!(close_mm(segs[0].start(), (110.0, 100.0), 1e-4));
    assert!(close_mm(segs[segs.len() - 1].end(), (100.0, 110.0), 1e-4));
    let mut n = 0;
    for seg in &segs {
        for k in 0..50 {
            let p = seg.point_at((f64::from(k) + 0.5) / 50.0);
            let d = (mm(p.0) - 100.0).hypot(mm(p.1) - 100.0);
            assert!(close(d, 10.0, 1e-6 + 2.5e-5), "{d}");
            n += 1;
        }
    }
    assert!(n >= 50);
}

fn st_subpaths(page: &pdf_read::Page) -> usize {
    strokes(page).iter().map(|s| s.subpaths.len()).sum()
}

// ---------------------------------------------------------------------------------------------
// (k) Fehler schreiben nichts
// ---------------------------------------------------------------------------------------------

#[test]
fn bad_input_writes_nothing() {
    let try_write = |models: &[&SheetModel]| {
        let mut out = Vec::new();
        let r = write_pdf(models, &meta(), &mut out);
        (r, out)
    };
    // NaN in einer Strecke.
    let nan = lines_sheet(vec![(
        Visibility::Visible,
        Out2d::Line {
            a: v(f64::NAN, 10.0),
            b: v(110.0, 10.0),
        },
    )]);
    let (r, out) = try_write(&[&nan]);
    assert!(matches!(r, Err(DrawError::NonFinite)), "{r:?}");
    assert!(out.is_empty());
    // Auch auf der zweiten Seite, in einem Text und in einem Pfeil.
    let ok = empty(Paper::A4, false);
    let (r, out) = try_write(&[&ok, &nan]);
    assert!(matches!(r, Err(DrawError::NonFinite)));
    assert!(out.is_empty());
    for bad in [
        text_mark(v(f64::INFINITY, 1.0), 3.5, 0.0, TextAnchor::Start, "x"),
        text_mark(v(1.0, 1.0), 3.5, f64::NAN, TextAnchor::Start, "x"),
        Mark::Arrow {
            tip: v(1.0, 1.0),
            dir: v(f64::NAN, 0.0),
            kind: MarkKind::Dimension,
        },
        Mark::Arc {
            center: v(1.0, 1.0),
            r: f64::NAN,
            start: 0.0,
            end: 1.0,
            kind: MarkKind::Dimension,
        },
    ] {
        let m = marks_sheet(Paper::A3, vec![bad]);
        let (r, out) = try_write(&[&m]);
        assert!(matches!(r, Err(DrawError::NonFinite)), "{r:?}");
        assert!(out.is_empty());
    }
    // Zeichen ohne WinAnsi-Platz im Rahmentext und im Maßtext.
    for c in ['😀', 'Ā'] {
        let mut m = empty(Paper::A3, true);
        m.frame = Some(SheetFrame {
            marks: vec![Mark::Text {
                pos: v(30.0, 30.0),
                height: 3.5,
                angle: 0.0,
                anchor: TextAnchor::Start,
                text: format!("Teil {c}"),
                kind: MarkKind::Sheet(SheetPen::Thin),
            }],
            scale: 1.0,
        });
        let (r, out) = try_write(&[&m]);
        match r {
            Err(DrawError::Unsupported(msg)) => assert_eq!(msg, pdf_char_message(c)),
            other => panic!("{other:?}"),
        }
        assert!(out.is_empty());
        let m = marks_sheet(
            Paper::A3,
            vec![text_mark(
                v(1.0, 1.0),
                3.5,
                0.0,
                TextAnchor::Start,
                &c.to_string(),
            )],
        );
        assert!(matches!(try_write(&[&m]).0, Err(DrawError::Unsupported(_))));
    }
    assert_eq!(
        pdf_char_message('😀'),
        "Das Zeichen „😀“ lässt sich in einer PDF nicht ausgeben."
    );
    // Seitenzahl.
    let many: Vec<&SheetModel> = vec![&ok; MAX_PAGES + 1];
    let (r, out) = try_write(&many);
    match r {
        Err(DrawError::Unsupported(msg)) => assert_eq!(msg, TOO_MANY_PAGES),
        other => panic!("{other:?}"),
    }
    assert!(out.is_empty());
    let (r, out) = try_write(&many[..MAX_PAGES]);
    assert!(r.is_ok());
    assert_eq!(read(&out).unwrap().pages.len(), 200);
    let (r, out) = try_write(&[]);
    match r {
        Err(DrawError::Unsupported(msg)) => assert_eq!(msg, NO_PAGES),
        other => panic!("{other:?}"),
    }
    assert!(out.is_empty());
}

// ---------------------------------------------------------------------------------------------
// (l) Zahlen
// ---------------------------------------------------------------------------------------------

#[test]
fn numbers() {
    let n = |x: f64| pdf_num(x).unwrap();
    assert_eq!(n(-0.00001), "0");
    assert_eq!(n(1.23456), "1.2346");
    assert_eq!(n(2.0), "2");
    assert_eq!(n(1e-5), "0");
    assert_eq!(n(12345678.9), "12345678.9");
    assert_eq!(n(-3.5), "-3.5");
    assert_eq!(n(0.1), "0.1");
    assert_eq!(n(-0.0), "0");
    for x in [1e-20, 1e20, -1e17, 123.456789e10] {
        assert!(!n(x).contains(['e', 'E']), "{}", n(x));
    }
    assert_eq!(n(1e20), "100000000000000000000");
    for x in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert!(matches!(pdf_num(x), Err(DrawError::NonFinite)));
    }
}
