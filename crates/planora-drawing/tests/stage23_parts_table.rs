//! Stufe 23 (Teilprojekt 3c-2): Stücklisten-Bausteine – Zelltexte aus Zeilen und
//! Überschreibungen, Lage der Tabelle auf dem Blatt und dem Stücklistenblatt, Seitenrechnung und
//! die Tabelle als Markierungen.

use planora_drawing::annotate::{Mark, MarkKind};
use planora_drawing::model::DrawingKind;
use planora_drawing::parts::*;
use planora_drawing::sheet_frame::*;
use planora_drawing::sheet_info::{
    FieldValue, ProjectInfo, SheetContext, SheetInfo, Stamp, resolve_texts,
};
use planora_drawing::sheet_marks::{parts_cell_is_cut, parts_table_marks, sheet_text_width};
use planora_drawing::{Drawing, DrawingId, Paper, Projection, Sheet, SheetPen, TitleField};
use planora_math::DVec2;
use std::collections::BTreeMap;

const PAPERS: [Paper; 5] = [Paper::A4, Paper::A3, Paper::A2, Paper::A1, Paper::A0];

fn sh(paper: Paper, landscape: bool) -> Sheet {
    Sheet { paper, landscape }
}

fn bx(x0: f64, y0: f64, x1: f64, y1: f64) -> Box2 {
    (DVec2::new(x0, y0), DVec2::new(x1, y1))
}

fn near_box(a: Box2, b: Box2, eps: f64) -> bool {
    (a.0.x - b.0.x).abs() <= eps
        && (a.0.y - b.0.y).abs() <= eps
        && (a.1.x - b.1.x).abs() <= eps
        && (a.1.y - b.1.y).abs() <= eps
}

#[track_caller]
fn assert_box(a: Box2, b: Box2, eps: f64) {
    assert!(near_box(a, b, eps), "{a:?} != {b:?}");
}

fn k(feature: u64, index: u32) -> BodyKey {
    BodyKey { feature, index }
}

fn cells(row: [&str; 5]) -> PartsCells {
    row.map(String::from)
}

fn sample_rows() -> Vec<PartsRow> {
    vec![
        PartsRow {
            pos: 1,
            name: "Bolzen".into(),
            qty: 2,
            keys: vec![k(1, 0), k(3, 0)],
        },
        PartsRow {
            pos: 2,
            name: "Platte".into(),
            qty: 1,
            keys: vec![k(2, 0)],
        },
    ]
}

#[test]
fn resolve_defaults_and_overrides() {
    let rows = sample_rows();
    let none = BTreeMap::new();
    assert_eq!(
        resolve_parts(&rows, &none, "–"),
        vec![
            cells(["1", "Bolzen", "2", "–", ""]),
            cells(["2", "Platte", "1", "–", ""]),
        ]
    );

    let mut ov = BTreeMap::new();
    ov.insert(
        k(1, 0),
        RowOverride {
            name: FieldValue::Text("Zylinderstift".into()),
            note: FieldValue::Text("gehärtet".into()),
            ..RowOverride::default()
        },
    );
    let got = resolve_parts(&rows, &ov, "–");
    assert_eq!(got[0], cells(["1", "Zylinderstift", "2", "–", "gehärtet"]));
    assert_eq!(got[1], cells(["2", "Platte", "1", "–", ""]));

    // Unter einem Schlüssel, der nicht der erste der Zeile ist: wirkungslos.
    let mut ov3 = BTreeMap::new();
    ov3.insert(
        k(3, 0),
        RowOverride {
            name: FieldValue::Text("Anders".into()),
            ..RowOverride::default()
        },
    );
    assert_eq!(
        resolve_parts(&rows, &ov3, "–"),
        resolve_parts(&rows, &none, "–")
    );

    assert!(RowOverride::default().is_auto());
    assert!(!ov[&k(1, 0)].is_auto());

    let name = |s: String| RowOverride {
        name: FieldValue::Text(s),
        ..RowOverride::default()
    };
    assert!(validate_override(&name("Ä".repeat(61))).is_err());
    assert!(validate_override(&name("Ä".repeat(60))).is_ok());
    let note = |s: String| RowOverride {
        note: FieldValue::Text(s),
        ..RowOverride::default()
    };
    assert!(validate_override(&note("x".repeat(81))).is_err());
    assert!(validate_override(&note("x".repeat(80))).is_ok());
    assert!(validate_override(&name("a\nb".into())).is_err());
    let qty = RowOverride {
        qty: FieldValue::Text("x".repeat(61)),
        ..RowOverride::default()
    };
    assert!(validate_override(&qty).is_err());
    let mat = RowOverride {
        material: FieldValue::Text("x".repeat(61)),
        ..RowOverride::default()
    };
    assert!(validate_override(&mat).is_err());
}

#[test]
fn layout_on_sheet_examples() {
    let on10 = PartsSpace::OnSheet { rows: 10 };
    // A3 quer.
    let l = sheet_layout_with(sh(Paper::A3, true), 3, 3, on10);
    assert_box(l.parts.unwrap(), bx(230.0, 66.0, 410.0, 143.0), 1e-9);
    assert_box(l.notes.unwrap(), bx(110.0, 10.0, 220.0, 32.0), 1e-9);
    assert_box(l.free, bx(20.0, 143.0, 410.0, 259.0), 1e-9);
    assert_eq!(l.parts_space, on10);
    // A3 hoch.
    let l = sheet_layout_with(sh(Paper::A3, false), 3, 3, on10);
    assert_box(l.parts.unwrap(), bx(107.0, 66.0, 287.0, 143.0), 1e-9);
    assert_box(l.notes.unwrap(), bx(107.0, 143.0, 287.0, 165.0), 1e-9);
    assert_box(l.free, bx(20.0, 165.0, 287.0, 382.0), 1e-9);
    // A4 hoch.
    let l = sheet_layout_with(sh(Paper::A4, false), 3, 3, on10);
    assert_box(l.parts.unwrap(), bx(75.643, 46.671, 202.929, 101.121), 1e-3);
    let n = l.notes.unwrap();
    assert!((n.0.y - 101.121).abs() < 1e-3 && (n.1.y - 116.679).abs() < 1e-3);
    assert_box(l.free, bx(14.143, 116.679, 202.929, 270.129), 1e-3);
    // Weniger Zeilen: kleinere Tabelle; mehr als 10: wie 10.
    let l = sheet_layout_with(sh(Paper::A3, true), 3, 3, PartsSpace::OnSheet { rows: 2 });
    assert_box(l.parts.unwrap(), bx(230.0, 66.0, 410.0, 87.0), 1e-9);
    let a = sheet_layout_with(sh(Paper::A3, true), 3, 3, PartsSpace::OnSheet { rows: 12 });
    let b = sheet_layout_with(sh(Paper::A3, true), 3, 3, on10);
    assert_eq!(a.parts, b.parts);
    assert_eq!(a.notes, b.notes);
    assert_eq!(a.free, b.free);

    // Ohne Stückliste genau wie bisher.
    for paper in PAPERS {
        for landscape in [false, true] {
            for rev in 0..=3 {
                for notes in 0..=6 {
                    let s = sh(paper, landscape);
                    let a = sheet_layout_with(s, rev, notes, PartsSpace::None);
                    let b = sheet_layout(s, rev, notes);
                    assert_eq!(a.sheet, b.sheet);
                    assert_eq!(a.frame, b.frame);
                    assert_eq!(a.centre_marks, b.centre_marks);
                    assert_eq!(a.grid_ticks, b.grid_ticks);
                    assert_eq!(a.grid_labels, b.grid_labels);
                    assert_eq!(a.title, b.title);
                    assert_eq!(a.revisions, b.revisions);
                    assert_eq!(a.notes, b.notes);
                    assert_eq!(a.free, b.free);
                    assert_eq!(a.scale, b.scale);
                    assert_eq!(a.parts, None);
                    assert_eq!(b.parts, None);
                    assert_eq!(a.parts_space, PartsSpace::None);
                    assert_eq!(b.parts_space, PartsSpace::None);
                }
            }
        }
    }
}

fn overlaps(a: Box2, b: Box2) -> bool {
    const E: f64 = 1e-9;
    a.0.x < b.1.x - E && b.0.x < a.1.x - E && a.0.y < b.1.y - E && b.0.y < a.1.y - E
}

fn inside(a: Box2, frame: Box2) -> bool {
    const E: f64 = 1e-9;
    a.0.x >= frame.0.x - E
        && a.0.y >= frame.0.y - E
        && a.1.x <= frame.1.x + E
        && a.1.y <= frame.1.y + E
}

#[test]
fn nothing_overlaps_and_free_never_empty() {
    for paper in PAPERS {
        for landscape in [false, true] {
            for rows in 0..=12 {
                for rev in 0..=3 {
                    for notes in 0..=6 {
                        let s = sh(paper, landscape);
                        let l = sheet_layout_with(s, rev, notes, PartsSpace::OnSheet { rows });
                        let ctx = format!(
                            "{paper:?} quer={landscape} rows={rows} rev={rev} notes={notes}"
                        );
                        let mut boxes = vec![l.parts.expect("Tabelle"), l.title, l.revisions];
                        if let Some(n) = l.notes {
                            boxes.push(n);
                        }
                        for (i, a) in boxes.iter().enumerate() {
                            assert!(inside(*a, l.frame), "{ctx}: {a:?} außerhalb");
                            for b in &boxes[i + 1..] {
                                assert!(!overlaps(*a, *b), "{ctx}: {a:?} / {b:?}");
                            }
                            assert!(!overlaps(*a, l.free), "{ctx}: frei / {a:?}");
                        }
                        assert!(
                            l.free.1.x - l.free.0.x > 0.0 && l.free.1.y - l.free.0.y > 0.0,
                            "{ctx}: frei leer {:?}",
                            l.free
                        );
                        assert!(inside(l.free, l.frame), "{ctx}");
                    }
                }
            }
        }
    }
}

/// Texte der Markierungen: (Grundlinienanfang, Höhe, Text).
fn texts(marks: &[Mark]) -> Vec<(DVec2, f64, String)> {
    marks
        .iter()
        .filter_map(|m| match m {
            Mark::Text {
                pos, height, text, ..
            } => Some((*pos, *height, text.clone())),
            _ => None,
        })
        .collect()
}

fn lines(marks: &[Mark]) -> Vec<(DVec2, DVec2, MarkKind)> {
    marks
        .iter()
        .filter_map(|m| match m {
            Mark::Line { a, b, kind } => Some((*a, *b, *kind)),
            _ => None,
        })
        .collect()
}

#[test]
fn longest_texts_stay_in_cells() {
    let long60 = "W".repeat(60);
    let long80 = "W".repeat(80);
    let special = "ÄÖÜß Ø°±".to_string();
    let rows: Vec<PartsCells> = vec![
        [
            "200".into(),
            long60.clone(),
            long60.clone(),
            long60.clone(),
            long80.clone(),
        ],
        [
            "2".into(),
            special.clone(),
            special.clone(),
            special.clone(),
            special.clone(),
        ],
        cells(["3", "Platte", "1", "–", ""]),
    ];
    let cases = [
        (sh(Paper::A4, false), PartsSpace::OnSheet { rows: 3 }),
        (sh(Paper::A3, true), PartsSpace::OnSheet { rows: 3 }),
        (sh(Paper::A0, false), PartsSpace::OnSheet { rows: 3 }),
        (sh(Paper::A4, false), PartsSpace::ListPage),
        (sh(Paper::A3, true), PartsSpace::ListPage),
        (sh(Paper::A0, false), PartsSpace::ListPage),
    ];
    for (s, space) in cases {
        let l = sheet_layout_with(s, 3, 3, space);
        let (lo, hi) = l.parts.unwrap();
        let cols = l.parts_columns();
        assert!((cols.iter().sum::<f64>() - (hi.x - lo.x)).abs() < 1e-9);
        let mut xs = vec![lo.x];
        for w in cols {
            xs.push(xs[xs.len() - 1] + w);
        }
        let row_h = l.mm(PARTS_ROW_H);
        let marks = parts_table_marks(&l, &rows, 1.0);
        let ts = texts(&marks);
        assert_eq!(ts.len(), 5 + 3 * 5 - 1, "{s:?} {space:?}"); // leere Bemerkung fehlt
        for (pos, h, t) in ts {
            let w = sheet_text_width(&t, h);
            let col = (0..5)
                .find(|&c| pos.x >= xs[c] - 1e-9 && pos.x < xs[c + 1])
                .expect("Spalte");
            assert!(pos.x + w <= xs[col + 1] + 1e-9, "{s:?} {space:?} {t}");
            // Zeile: die Zelle, in deren Höhe die Grundlinie liegt (gezählt von der Kopfseite).
            let (y0, y1) = if space == PartsSpace::ListPage {
                let r = ((hi.y - pos.y) / row_h).floor();
                (hi.y - (r + 1.0) * row_h, hi.y - r * row_h)
            } else {
                let r = ((pos.y - lo.y) / row_h).floor();
                (lo.y + r * row_h, lo.y + (r + 1.0) * row_h)
            };
            assert!(pos.y >= y0 - 1e-9 && pos.y + h <= y1 + 1e-9, "{s:?} {t}");
        }
    }
    let l = sheet_layout_with(sh(Paper::A3, true), 0, 0, PartsSpace::OnSheet { rows: 3 });
    assert!(parts_cell_is_cut(1, &long60, &l));
    assert!(!parts_cell_is_cut(1, "Platte", &l));
}

#[test]
fn pages() {
    for paper in [Paper::A4, Paper::A3, Paper::A0] {
        assert_eq!(parts_rows_per_page(sh(paper, false)), 44, "{paper:?}");
    }
    for paper in [Paper::A4, Paper::A3, Paper::A1] {
        assert_eq!(parts_rows_per_page(sh(paper, true)), 26, "{paper:?}");
    }
    assert_eq!(pages_needed(0, 26), 1);
    assert_eq!(pages_needed(26, 26), 1);
    assert_eq!(pages_needed(27, 26), 2);
    assert_eq!(pages_needed(44, 44), 1);
    assert_eq!(pages_needed(45, 44), 2);
    assert_eq!(pages_needed(201, 26), 8);
    let all: Vec<PartsCells> = (1..=201)
        .map(|i| {
            [
                i.to_string(),
                "Teil".into(),
                "1".into(),
                "–".into(),
                String::new(),
            ]
        })
        .collect();
    assert_eq!(page_rows(&all, 8, 26).len(), 18);
    assert_eq!(page_rows(&all, 2, 26)[0][0], "27");
    assert_eq!(page_rows(&all, 1, 26).len(), 26);
    assert!(page_rows(&all, 9, 26).is_empty());
}

#[test]
fn list_page_layout() {
    let l = sheet_layout_with(sh(Paper::A3, false), 0, 6, PartsSpace::ListPage);
    assert_box(l.parts.unwrap(), bx(20.0, 66.0, 287.0, 382.0), 1e-9);
    assert_eq!(l.notes, None);
    let cols = l.parts_columns();
    for (c, a3) in cols.iter().zip(PARTS_COLUMNS) {
        assert!((c - a3 * 267.0 / 180.0).abs() < 1e-9);
    }
    assert!((cols[0] - 17.8).abs() < 1e-9);
    // Änderungstabelle immer mit drei Zeilen reserviert.
    assert_box(l.revisions, bx(111.0, 382.0, 287.0, 410.0), 1e-9);
}

#[test]
fn table_marks() {
    let rows = vec![
        cells(["1", "Bolzen", "2", "–", ""]),
        cells(["2", "Platte", "1", "–", ""]),
        cells(["3", "Mutter", "4", "–", ""]),
    ];
    let l = sheet_layout_with(sh(Paper::A3, true), 0, 0, PartsSpace::OnSheet { rows: 3 });
    let marks = parts_table_marks(&l, &rows, 1.0);
    let ts = texts(&marks);
    let at = |t: &str| ts.iter().find(|x| x.2 == t).map(|x| x.0).expect(t);
    for h in PARTS_HEAD {
        assert!((at(h).y - 67.75).abs() < 1e-9, "{h}");
    }
    assert!((at("Pos.").x - 231.0).abs() < 1e-9);
    assert!((at("1").y - 74.75).abs() < 1e-9);
    assert!((at("3").y - 88.75).abs() < 1e-9);
    let ls = lines(&marks);
    assert!(ls.iter().all(|l| l.2 == MarkKind::Sheet(SheetPen::Thin)));
    for x in [242.0, 320.0, 335.0, 380.0] {
        assert!(
            ls.iter().any(|(a, b, _)| (a.x - x).abs() < 1e-9
                && (b.x - x).abs() < 1e-9
                && (a.y.min(b.y) - 66.0).abs() < 1e-9
                && (a.y.max(b.y) - 94.0).abs() < 1e-9),
            "Trennlinie bei x {x}"
        );
    }

    let l = sheet_layout_with(sh(Paper::A3, false), 0, 0, PartsSpace::ListPage);
    let marks = parts_table_marks(&l, &rows, 1.0);
    let ts = texts(&marks);
    let at = |t: &str| ts.iter().find(|x| x.2 == t).map(|x| x.0).expect(t);
    assert!((at("Pos.").y - 376.75).abs() < 1e-9);
    assert!((at("1").y - 369.75).abs() < 1e-9);

    let marks = parts_table_marks(&l, &[], 1.0);
    let ts = texts(&marks);
    assert_eq!(
        ts.iter().map(|t| t.2.as_str()).collect::<Vec<_>>(),
        PARTS_HEAD.to_vec()
    );
    // Ohne Tabelle nichts.
    let l = sheet_layout(sh(Paper::A3, true), 0, 0);
    assert!(parts_table_marks(&l, &rows, 1.0).is_empty());
}

#[test]
fn doc_type_of_parts_sheet() {
    let stamp = Stamp {
        date: "2026-10-18".into(),
        user: "dg".into(),
    };
    let mut d = Drawing::new(
        DrawingId(5),
        parts_sheet_name(2),
        sh(Paper::A4, false),
        Projection::First,
    );
    assert_eq!(d.kind, DrawingKind::Views);
    d.kind = DrawingKind::PartsList { page: 2 };
    d.info = Some(SheetInfo::new(&stamp));
    assert!(!d.info.as_ref().unwrap().parts_list);
    let ctx = SheetContext {
        doc_name: "Projekt".into(),
        index: 2,
        count: 3,
    };
    let p = ProjectInfo::new(&stamp);
    let t = resolve_texts(&d, Some(&p), &ctx, None);
    assert_eq!(t.get(TitleField::DocType), PARTS_DOC_TYPE);
    assert_eq!(t.get(TitleField::DocType), "Stückliste");
    assert_eq!(t.get(TitleField::Title), "Stückliste (2)");
    assert!(t.parts.is_empty());
    d.kind = DrawingKind::Views;
    let t = resolve_texts(&d, Some(&p), &ctx, None);
    assert_eq!(t.get(TitleField::DocType), "Fertigungszeichnung");
    assert_eq!(parts_sheet_name(1), "Stückliste");
    assert_eq!(parts_sheet_name(3), "Stückliste (3)");
}
