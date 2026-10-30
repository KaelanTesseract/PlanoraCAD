//! Stufe 23 (Teilprojekt 3c-2): Stücklisten-Bausteine – Zelltexte aus Zeilen und
//! Überschreibungen, Lage der Tabelle auf dem Blatt und dem Stücklistenblatt, Seitenrechnung und
//! die Tabelle als Markierungen.

use planora_drawing::annotate::{Mark, MarkKind};
use planora_drawing::model::DrawingKind;
use planora_drawing::parts::*;
use planora_drawing::sheet_frame::*;
use planora_drawing::sheet_info::{
    FieldValue, ProjectInfo, SheetContext, SheetInfo, Stamp, TitleTexts, resolve_texts,
};
use planora_drawing::sheet_marks::{
    parts_cell_is_cut, parts_table_marks, sheet_frame, sheet_text_width,
};
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

    // Jede Spalte mit eigener Grenze: genau die Grenze geht, eins mehr nicht (in Zeichen,
    // nicht Bytes); Steuerzeichen nie.
    let one = |col: usize, t: String| {
        let mut o = RowOverride::default();
        let v = FieldValue::Text(t);
        match col {
            0 => o.name = v,
            1 => o.qty = v,
            2 => o.material = v,
            _ => o.note = v,
        }
        o
    };
    let limits = [
        MAX_PARTS_NAME_CHARS,
        MAX_PARTS_QTY_CHARS,
        MAX_PARTS_MATERIAL_CHARS,
        MAX_PARTS_NOTE_CHARS,
    ];
    assert_eq!(limits, [60, 60, 60, 80]);
    for (col, max) in limits.into_iter().enumerate() {
        assert!(
            validate_override(&one(col, "Ä".repeat(max))).is_ok(),
            "{col}"
        );
        assert!(
            validate_override(&one(col, "Ä".repeat(max + 1))).is_err(),
            "{col}"
        );
        for ctrl in ["a\nb", "a\tb", "\u{7}"] {
            assert!(
                validate_override(&one(col, ctrl.into())).is_err(),
                "{col} {ctrl:?}"
            );
        }
    }
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
            // Stücklistenblatt: kein Hinweisfeld, Änderungstabelle immer reserviert, Tabelle
            // zwischen Schriftfeld und Änderungstabelle über die volle Breite; der freie
            // Bereich ist die Tabelle selbst (keine Ansichten auf diesem Blatt).
            for rev in 0..=3 {
                for notes in 0..=6 {
                    let s = sh(paper, landscape);
                    let l = sheet_layout_with(s, rev, notes, PartsSpace::ListPage);
                    let ctx = format!("Liste {paper:?} quer={landscape} rev={rev} notes={notes}");
                    let t = l.parts.expect("Tabelle");
                    assert_eq!(l.notes, None, "{ctx}");
                    assert_eq!(
                        l.revisions,
                        sheet_layout(s, REV_VISIBLE, 0).revisions,
                        "{ctx}"
                    );
                    assert_eq!(l.free, t, "{ctx}");
                    assert!(t.1.x - t.0.x > 0.0 && t.1.y - t.0.y > 0.0, "{ctx}");
                    assert!((t.0.x - l.frame.0.x).abs() < 1e-9, "{ctx}");
                    assert!((t.1.x - l.frame.1.x).abs() < 1e-9, "{ctx}");
                    let boxes = [t, l.title, l.revisions];
                    for (i, a) in boxes.iter().enumerate() {
                        assert!(inside(*a, l.frame), "{ctx}: {a:?} außerhalb");
                        for b in &boxes[i + 1..] {
                            assert!(!overlaps(*a, *b), "{ctx}: {a:?} / {b:?}");
                        }
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
    for h in &parts_head() {
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
        parts_head().to_vec()
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
    assert_eq!(t.get(TitleField::DocType), parts_doc_type());
    assert_eq!(t.get(TitleField::DocType), "Stückliste");
    assert_eq!(t.get(TitleField::Title), "Stückliste (2)");
    assert!(t.parts.is_empty());
    d.kind = DrawingKind::Views;
    let t = resolve_texts(&d, Some(&p), &ctx, None);
    assert_eq!(t.get(TitleField::DocType), "Fertigungszeichnung");
    assert_eq!(parts_sheet_name(1), "Stückliste");
    assert_eq!(parts_sheet_name(3), "Stückliste (3)");
}

#[test]
fn table_marks_scale_with_paper() {
    let rows = vec![
        cells(["1", "Bolzen", "2", "–", ""]),
        cells(["2", "Platte", "1", "–", ""]),
        cells(["3", "Mutter", "4", "–", ""]),
    ];
    // (Blatt, f, x „Pos.“, Grundlinie Kopf, Höhe, Grundlinie Pos. 1, x „Benennung“)
    let cases = [
        (
            sh(Paper::A4, false),
            0.707_142_857_142_857_2,
            76.35,
            47.908_928_571_428_57,
            2.475,
            52.858_928_571_428_57,
            84.835_714_285_714_26,
        ),
        (
            sh(Paper::A0, true),
            2.830_952_380_952_381,
            653.95,
            191.797_023_809_523_84,
            9.908_333_333_333_333,
            211.613_690_476_190_5,
            687.921_428_571_428_5,
        ),
    ];
    for (s, f, x_pos, base_head, h, base_1, x_name) in cases {
        let l = sheet_layout_with(s, 0, 0, PartsSpace::OnSheet { rows: 3 });
        assert!((l.scale - f).abs() < 1e-12);
        let ts = texts(&parts_table_marks(&l, &rows, 1.0));
        let get = |t: &str| ts.iter().find(|x| x.2 == t).cloned().expect(t);
        let (p, ht, _) = get("Pos.");
        assert!((p.x - x_pos).abs() < 1e-9, "{s:?} x {}", p.x);
        assert!((p.y - base_head).abs() < 1e-9, "{s:?} y {}", p.y);
        assert!((ht - h).abs() < 1e-9, "{s:?} h {ht}");
        let (p, ht, _) = get("1");
        assert!((p.x - x_pos).abs() < 1e-9);
        assert!((p.y - base_1).abs() < 1e-9, "{s:?} y1 {}", p.y);
        assert!((ht - h).abs() < 1e-9);
        let (p, _, _) = get("Benennung");
        assert!((p.x - x_name).abs() < 1e-9, "{s:?} xn {}", p.x);
        assert!((p.y - base_head).abs() < 1e-9);
    }
}

fn numbered(n: usize) -> Vec<PartsCells> {
    (1..=n)
        .map(|i| {
            [
                i.to_string(),
                "Teil".into(),
                "1".into(),
                "–".into(),
                String::new(),
            ]
        })
        .collect()
}

#[track_caller]
fn assert_marks_inside(marks: &[Mark], bounds: Box2) {
    for m in marks {
        match m {
            Mark::Line { a, b, .. } => {
                assert!(
                    inside((*a, *a), bounds) && inside((*b, *b), bounds),
                    "{a:?} {b:?} / {bounds:?}"
                )
            }
            Mark::Text {
                pos, height, text, ..
            } => {
                let hi = DVec2::new(pos.x + sheet_text_width(text, *height), pos.y + height);
                assert!(inside((*pos, hi), bounds), "{text} / {bounds:?}");
            }
            _ => panic!("unerwartete Markierung {m:?}"),
        }
    }
}

#[test]
fn rows_beyond_capacity_are_not_drawn() {
    // Zeichnungsblatt: höchstens 10 Zeilen.
    let l = sheet_layout_with(sh(Paper::A3, true), 0, 0, PartsSpace::OnSheet { rows: 10 });
    let marks = parts_table_marks(&l, &numbered(12), 1.0);
    let ts = texts(&marks);
    let pos: Vec<&str> = ts
        .iter()
        .filter(|t| (t.0.x - 231.0).abs() < 1e-9)
        .map(|t| t.2.as_str())
        .collect();
    let want: Vec<String> = std::iter::once("Pos.".to_string())
        .chain((1..=10).map(|i| i.to_string()))
        .collect();
    assert_eq!(pos, want);
    assert_marks_inside(&marks, l.parts.unwrap());
    // Stücklistenblatt: höchstens so viele, wie hineinpassen.
    for s in [sh(Paper::A3, false), sh(Paper::A4, true)] {
        let l = sheet_layout_with(s, 0, 0, PartsSpace::ListPage);
        let cap = parts_rows_per_page(s);
        let marks = parts_table_marks(&l, &numbered(cap + 6), 1.0);
        let x_pos = l.parts.unwrap().0.x + l.mm(1.0);
        let n_pos = texts(&marks)
            .iter()
            .filter(|t| (t.0.x - x_pos).abs() < 1e-9)
            .count();
        assert_eq!(n_pos, cap + 1, "{s:?}");
        assert_marks_inside(&marks, l.parts.unwrap());
    }
}

/// Waagerechte Linien (y, x0, x1) und senkrechte (x, y0, y1), sortiert.
#[allow(clippy::type_complexity)]
fn split_lines(marks: &[Mark]) -> (Vec<(f64, f64, f64)>, Vec<(f64, f64, f64)>) {
    let mut hz = Vec::new();
    let mut vt = Vec::new();
    for (a, b, _) in lines(marks) {
        if (a.y - b.y).abs() < 1e-12 {
            hz.push((a.y, a.x.min(b.x), a.x.max(b.x)));
        } else {
            assert!((a.x - b.x).abs() < 1e-12);
            vt.push((a.x, a.y.min(b.y), a.y.max(b.y)));
        }
    }
    hz.sort_by(|a, b| a.partial_cmp(b).unwrap());
    vt.sort_by(|a, b| a.partial_cmp(b).unwrap());
    (hz, vt)
}

/// Prüft genau: waagerechte Linien bei `ys` (aufsteigend) über die volle Breite, senkrechte bei
/// `xs` vom untersten bis zum obersten `ys`.
#[track_caller]
fn assert_grid(marks: &[Mark], x0: f64, x1: f64, ys: &[f64], xs: &[f64]) {
    let (hz, vt) = split_lines(marks);
    assert_eq!(hz.len(), ys.len(), "{hz:?}");
    for (h, y) in hz.iter().zip(ys) {
        assert!(
            (h.0 - y).abs() < 1e-9 && (h.1 - x0).abs() < 1e-9 && (h.2 - x1).abs() < 1e-9,
            "{h:?} / {y}"
        );
    }
    let (y0, y1) = (ys[0], ys[ys.len() - 1]);
    assert_eq!(vt.len(), xs.len(), "{vt:?}");
    for (v, x) in vt.iter().zip(xs) {
        assert!(
            (v.0 - x).abs() < 1e-9 && (v.1 - y0).abs() < 1e-9 && (v.2 - y1).abs() < 1e-9,
            "{v:?} / {x}"
        );
    }
}

#[test]
fn only_header_and_filled_rows_are_drawn() {
    // Stücklistenblatt A3 hoch: Tabelle (20,66)–(287,382), Kopf oben.
    let l = sheet_layout_with(sh(Paper::A3, false), 0, 0, PartsSpace::ListPage);
    let mut xs = vec![20.0];
    for w in l.parts_columns() {
        xs.push(xs[xs.len() - 1] + w);
    }
    for n in [0usize, 3, 44] {
        let marks = parts_table_marks(&l, &numbered(n), 1.0);
        let ys: Vec<f64> = (0..=n + 1).rev().map(|k| 382.0 - 7.0 * k as f64).collect();
        assert_grid(&marks, 20.0, 287.0, &ys, &xs);
    }
    // Zeichnungsblatt mit Platz für 10, aber 3 Zeilen: Umriss nur um Kopf und 3 Zeilen.
    let l = sheet_layout_with(sh(Paper::A3, true), 0, 0, PartsSpace::OnSheet { rows: 10 });
    let xs = [230.0, 242.0, 320.0, 335.0, 380.0, 410.0];
    let marks = parts_table_marks(&l, &numbered(3), 1.0);
    assert_grid(&marks, 230.0, 410.0, &[66.0, 73.0, 80.0, 87.0, 94.0], &xs);
    let marks = parts_table_marks(&l, &[], 1.0);
    assert_grid(&marks, 230.0, 410.0, &[66.0, 73.0], &xs);
}

#[test]
fn sheet_frame_contains_the_table() {
    let with_parts = TitleTexts {
        fields: BTreeMap::new(),
        revisions: Vec::new(),
        notes: Vec::new(),
        parts: numbered(3),
    };
    let l = sheet_layout_with(sh(Paper::A3, true), 0, 0, PartsSpace::OnSheet { rows: 3 });
    let frame = sheet_frame(&l, &with_parts, Projection::First);
    let table = parts_table_marks(&l, &with_parts.parts, 1.0);
    assert!(!table.is_empty());
    for m in &table {
        assert!(frame.marks.contains(m), "{m:?}");
    }
    let l = sheet_layout(sh(Paper::A3, true), 0, 0);
    let frame = sheet_frame(&l, &with_parts, Projection::First);
    assert!(
        !frame
            .marks
            .iter()
            // „Werkstoff“ steht auch im Schriftfeld; nur Texte, die allein die Tabelle hat.
            .any(|m| matches!(m, Mark::Text { text, .. }
                if ["Pos.", "Benennung", "Menge", "Bemerkung", "Teil"].contains(&text.as_str())))
    );
}

#[test]
fn page_count_is_capped_at_200_rows() {
    assert_eq!(pages_needed(200, 25), 8);
    assert_eq!(pages_needed(201, 25), 8);
    assert_eq!(pages_needed(1000, 25), 8);
    assert_eq!(pages_needed(200, 1), 200);
    assert_eq!(pages_needed(201, 1), 200);
}

#[test]
fn cut_check_keeps_the_cell_margin() {
    // Spalte Pos. auf A3: 12 breit, Text darf 10 (12 − 2·1). 6 × „W“ in kleinster Höhe 2,1:
    // 11,97 – passt in 12, nicht in 10.
    let l = sheet_layout_with(sh(Paper::A3, true), 0, 0, PartsSpace::OnSheet { rows: 3 });
    assert!(parts_cell_is_cut(0, "WWWWWW", &l));
    assert!(!parts_cell_is_cut(0, "WWWWW", &l));
}

#[test]
fn list_page_free_area_is_the_table() {
    let l = sheet_layout_with(sh(Paper::A3, false), 2, 4, PartsSpace::ListPage);
    assert_box(l.free, bx(20.0, 66.0, 287.0, 382.0), 1e-9);
    assert_eq!(Some(l.free), l.parts);
}
