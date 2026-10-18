//! Proben für den Handtest der Stückliste und der Positionsnummern (Teilprojekt 3c-2): SVG und
//! DXF mit und ohne Blattrahmen für A4 hoch, A4 quer, A3 quer und A0 quer, je mit einer Tabelle
//! von 3 Zeilen ("kurz") und von 12 Zeilen mit Längstexten und Sonderzeichen ("lang", davon 10 auf
//! dem Blatt), dazu Nummernkreise (1, 12 und 200), und ein Stücklistenblatt mit zwei Seiten
//! (A4 hoch, 45 Zeilen).
//!
//! Der Test `write_parts_samples_for_the_hand_test` ist ausgeschaltet und wird so gestartet:
//!
//! `cargo test --release -p planora-drawing --test hand_samples_parts -- --ignored`
//!
//! Zielordner ist `$PLANORA_PARTS_SAMPLES_DIR`, sonst `D:\Testumgebung\zeichnung-proben\stueckliste`.

mod common;

use common::model::*;
use planora_drawing::annotate::ResolvedDims;
use planora_drawing::balloon::{add_balloon_marks, balloon_marks};
use planora_drawing::layout::{GAP_MM, align_dir, view_extent};
use planora_drawing::model::{
    Balloon, BalloonId, Drawing, DrawingId, DrawingKind, Paper, Placement, Projection, Scale,
    Sheet, ViewId,
};
use planora_drawing::parts::{page_rows, parts_sheet_name};
use planora_drawing::sheet::build_sheet_with;
use planora_drawing::sheet_frame::{
    PartsSpace, parts_rows_per_page, sheet_layout_with, sheet_scale,
};
use planora_drawing::sheet_info::{
    FieldValue, NotesValue, SheetContext, SheetInfo, Stamp, main_scale, resolve_texts,
};
use planora_drawing::sheet_marks::{
    DXF_TEXT_WIDTH_FACTOR, SheetFrame, parts_table_marks, sheet_frame, sheet_frame_dxf,
};
use planora_drawing::{
    BodyKey, DrawBody, PartsCells, ViewKind, ViewLines, compute_view, dxf_string, svg_string,
};
use planora_exchange::Progress;
use planora_math::DVec3;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const SIZES: [(&str, Paper, bool); 4] = [
    ("A4hoch", Paper::A4, false),
    ("A4quer", Paper::A4, true),
    ("A3quer", Paper::A3, true),
    ("A0quer", Paper::A0, true),
];

fn stamp() -> Stamp {
    Stamp {
        date: "2026-10-18".into(),
        user: "Probe".into(),
    }
}

fn cells(pos: usize, name: &str, qty: &str, material: &str, note: &str) -> PartsCells {
    [
        pos.to_string(),
        name.to_string(),
        qty.to_string(),
        material.to_string(),
        note.to_string(),
    ]
}

fn take(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

/// Drei Zeilen mit Vorgabetexten.
fn short_rows() -> Vec<PartsCells> {
    vec![
        cells(1, "Haltewinkel", "1", "S235JR", ""),
        cells(2, "Bolzen", "2", "S235JR", ""),
        cells(3, "Scheibe", "4", "–", ""),
    ]
}

/// Zwölf Zeilen mit Längstexten (bis an die Grenzen) und Sonderzeichen.
fn long_rows() -> Vec<PartsCells> {
    (1..=12)
        .map(|i| {
            cells(
                i,
                &take(
                    &format!("Gehäuseabdeckung Ø 12 ± 0,1 & Söhne <Nr. {i}> äöüß WWWWWWWWWWWWWWW"),
                    60,
                ),
                &format!("{}", i * 111),
                &take(
                    "S235JR (1.0038), verzinkt, 3 mm ± 0,1 WWWWWWWWWWWWWWWWWWWWWW",
                    60,
                ),
                &take(
                    "Bemerkung mit Umlauten: Übergröße, Öffnung, Maße ± 0,05 mm, Ø 12 °, a & b < c",
                    80,
                ),
            )
        })
        .collect()
}

struct Built {
    drawing: Drawing,
    views: BTreeMap<ViewId, ViewLines>,
}

/// Drei Ansichten (Vorne, Oben, Links) als Block mittig im freien Bereich neben der Tabelle
/// (Platz für 10 Zeilen), größter Maßstab, bei dem der Block hineinpasst; drei Nummernkreise.
fn build(paper: Paper, landscape: bool, long: bool) -> Built {
    let sheet = Sheet { paper, landscape };
    let projection = Projection::First;
    let solid = box_with_hole(40.0, 30.0, 20.0, 5.0, DVec3::Y);
    let bodies = [DrawBody {
        name: "Q".into(),
        solid: &solid,
    }];
    let p = Progress::new();
    let kinds = [ViewKind::Front, ViewKind::Top, ViewKind::Left];
    let lines: Vec<ViewLines> = kinds
        .iter()
        .map(|&k| compute_view(&bodies, k, &p).unwrap())
        .collect();
    let space = PartsSpace::OnSheet { rows: 10 };
    let layout = if long {
        sheet_layout_with(sheet, 0, 6, space)
    } else {
        sheet_layout_with(sheet, 0, 3, space)
    };
    let (lo, hi) = layout.free;
    let (free_w, free_h) = (hi.x - lo.x, hi.y - lo.y);
    let mut order = Scale::SERIES.to_vec();
    order.sort_by(|a, b| b.factor().partial_cmp(&a.factor()).unwrap());
    let mut chosen = None;
    for scale in order {
        let ext = |i: usize| view_extent(lines[i].bbox, scale);
        let (fw, fh) = ext(0);
        let mut rects = vec![(-fw / 2.0, -fh / 2.0, fw / 2.0, fh / 2.0)];
        let mut dists = Vec::new();
        for i in [1usize, 2] {
            let (dx, dy) = align_dir(projection, ViewKind::Front, kinds[i]).unwrap();
            let (cw, ch) = ext(i);
            let dist = if dx != 0.0 {
                (fw + cw) / 2.0 + GAP_MM
            } else {
                (fh + ch) / 2.0 + GAP_MM
            };
            dists.push(dist);
            rects.push((
                dx * dist - cw / 2.0,
                dy * dist - ch / 2.0,
                dx * dist + cw / 2.0,
                dy * dist + ch / 2.0,
            ));
        }
        let u = rects
            .iter()
            .fold((f64::MAX, f64::MAX, f64::MIN, f64::MIN), |u, r| {
                (u.0.min(r.0), u.1.min(r.1), u.2.max(r.2), u.3.max(r.3))
            });
        if u.2 - u.0 <= free_w - 10.0 && u.3 - u.1 <= free_h - 10.0 {
            let cx = lo.x + free_w / 2.0 - (u.0 + u.2) / 2.0;
            let cy = lo.y + free_h / 2.0 - (u.1 + u.3) / 2.0;
            chosen = Some((scale, cx, cy, dists));
            break;
        }
    }
    let (scale, cx, cy, dists) = chosen.expect("kein Maßstab passt");
    let mut d = Drawing::new(DrawingId(1), "Haltewinkel".into(), sheet, projection);
    let f = d.add_view(
        ViewKind::Front,
        scale,
        Placement::Free { x: cx, y: cy },
        None,
    );
    let t = d.add_view(
        ViewKind::Top,
        scale,
        Placement::Aligned { distance: dists[0] },
        Some(f),
    );
    let l = d.add_view(
        ViewKind::Left,
        scale,
        Placement::Aligned { distance: dists[1] },
        Some(f),
    );
    let mut info = SheetInfo::new(&stamp());
    info.parts_list = true;
    if long {
        info.notes = NotesValue::Lines(
            (1..=6)
                .map(|i| format!("Hinweis {i}: Kanten gebrochen 0,3 mm, ä ö ü ß Ø ° ±"))
                .collect(),
        );
        info.title = FieldValue::Text("Haltewinkel für die Gehäuseabdeckung links oben".into());
    }
    d.info = Some(info);
    // Nummernkreise an den drei Ansichten (Nummern 1, 12 und 200 setzt der Schreiber).
    let k = sheet_scale(sheet);
    for (view, anchor, offset, body) in [
        (f, (0.3, 0.7), (10.0, 10.0), 0u64),
        (t, (0.5, 0.5), (-12.0, -9.0), 1),
        (l, (0.8, 0.2), (12.0, 14.0), 2),
    ] {
        d.add_balloon(Balloon {
            id: BalloonId(0),
            view,
            body: BodyKey {
                feature: body,
                index: 0,
            },
            anchor,
            offset: (offset.0 * k, offset.1 * k),
        });
    }
    let mut views = BTreeMap::new();
    for (id, l) in [f, t, l].into_iter().zip(lines) {
        views.insert(id, l);
    }
    Built { drawing: d, views }
}

fn write(dir: &Path, base: &str, ext: &str, content: String) {
    std::fs::write(dir.join(format!("{base}.{ext}")), content).unwrap();
}

fn write_sheet_files(dir: &Path) {
    for (name, paper, landscape) in SIZES {
        for (vname, long) in [("kurz", false), ("lang", true)] {
            let b = build(paper, landscape, long);
            let rows = if long { long_rows() } else { short_rows() };
            let refs: BTreeMap<_, &ViewLines> = b.views.iter().map(|(k, v)| (*k, v)).collect();
            let mut model = build_sheet_with(&b.drawing, &refs, &ResolvedDims::default());
            assert_eq!(model.views.len(), 3);
            let mut numbers = BTreeMap::new();
            for (bl, n) in b.drawing.balloons.iter().zip([1u32, 12, 200]) {
                numbers.insert(bl.id, Some(n));
            }
            add_balloon_marks(&mut model, &balloon_marks(&b.drawing, &refs, &numbers));
            let ctx = SheetContext {
                doc_name: "Probe".into(),
                index: 1,
                count: 1,
            };
            let mut texts = resolve_texts(&b.drawing, None, &ctx, main_scale(&b.drawing));
            texts.parts = rows.iter().take(10).cloned().collect();
            let layout = sheet_layout_with(
                b.drawing.sheet,
                texts.visible_revisions().len(),
                texts.notes.len(),
                PartsSpace::OnSheet {
                    rows: texts.parts.len(),
                },
            );
            let base = format!("{name}_{vname}");
            write(
                dir,
                &format!("{base}_ohneRahmen"),
                "svg",
                svg_string(&model).unwrap(),
            );
            write(
                dir,
                &format!("{base}_ohneRahmen"),
                "dxf",
                dxf_string(&model).unwrap(),
            );
            model.frame = Some(sheet_frame(&layout, &texts, b.drawing.projection));
            write(
                dir,
                &format!("{base}_Rahmen"),
                "svg",
                svg_string(&model).unwrap(),
            );
            model.frame = Some(sheet_frame_dxf(&layout, &texts, b.drawing.projection));
            write(
                dir,
                &format!("{base}_Rahmen"),
                "dxf",
                dxf_string(&model).expect("DXF mit Rahmen"),
            );
        }
    }
}

/// Stücklistenblatt (A4 hoch, 45 Zeilen: 44 auf Blatt 1, 1 auf Blatt 2).
fn write_parts_sheets(dir: &Path) {
    let sheet = Sheet {
        paper: Paper::A4,
        landscape: false,
    };
    let all: Vec<PartsCells> = (1..=45)
        .map(|i| {
            cells(
                i,
                &format!("Teil Nr. {i} Ø 8 ± 0,1 äöü & <x>"),
                &format!("{}", 1 + i % 7),
                "S235JR",
                if i % 5 == 0 { "nach Zeichnung" } else { "" },
            )
        })
        .collect();
    let per_page = parts_rows_per_page(sheet);
    assert_eq!(per_page, 44);
    for page in 1..=2u32 {
        let mut d = Drawing::new(
            DrawingId(u64::from(page)),
            parts_sheet_name(page),
            sheet,
            Projection::First,
        );
        d.kind = DrawingKind::PartsList { page };
        d.info = Some(SheetInfo::new(&stamp()));
        let mut model = build_sheet_with(&d, &BTreeMap::new(), &ResolvedDims::default());
        let ctx = SheetContext {
            doc_name: "Probe".into(),
            index: page as usize,
            count: 2,
        };
        let mut texts = resolve_texts(&d, None, &ctx, None);
        texts.parts = page_rows(&all, page, per_page).to_vec();
        let layout = sheet_layout_with(
            sheet,
            texts.visible_revisions().len(),
            0,
            PartsSpace::ListPage,
        );
        let base = format!("Stuecklistenblatt_Seite{page}");
        // ohne Rahmen: nur die Tabelle
        model.frame = Some(SheetFrame {
            marks: parts_table_marks(&layout, &texts.parts, 1.0),
            scale: layout.scale,
        });
        write(
            dir,
            &format!("{base}_ohneRahmen"),
            "svg",
            svg_string(&model).unwrap(),
        );
        model.frame = Some(SheetFrame {
            marks: parts_table_marks(&layout, &texts.parts, DXF_TEXT_WIDTH_FACTOR),
            scale: layout.scale,
        });
        write(
            dir,
            &format!("{base}_ohneRahmen"),
            "dxf",
            dxf_string(&model).unwrap(),
        );
        // mit Rahmen
        model.frame = Some(sheet_frame(&layout, &texts, d.projection));
        write(
            dir,
            &format!("{base}_Rahmen"),
            "svg",
            svg_string(&model).unwrap(),
        );
        model.frame = Some(sheet_frame_dxf(&layout, &texts, d.projection));
        write(
            dir,
            &format!("{base}_Rahmen"),
            "dxf",
            dxf_string(&model).unwrap(),
        );
    }
}

fn samples_dir() -> PathBuf {
    match std::env::var_os("PLANORA_PARTS_SAMPLES_DIR") {
        Some(d) if !d.is_empty() => PathBuf::from(d),
        _ => PathBuf::from(r"D:\Testumgebung\zeichnung-proben\stueckliste"),
    }
}

#[test]
#[ignore = "schreibt Dateien für den Handtest: --release -- --ignored"]
fn write_parts_samples_for_the_hand_test() {
    let dir = samples_dir();
    std::fs::create_dir_all(&dir).unwrap();
    write_sheet_files(&dir);
    write_parts_sheets(&dir);
    let n = std::fs::read_dir(&dir).unwrap().count();
    assert_eq!(n, 4 * 2 * 4 + 2 * 4);
    println!("Proben liegen in {}", dir.display());
}

/// Damit der Schreiber nicht verrottet: Aufbau ohne Dateien.
#[test]
fn parts_sample_builder_still_works() {
    for (paper, landscape) in [(Paper::A4, false), (Paper::A0, true)] {
        for long in [false, true] {
            let b = build(paper, landscape, long);
            assert_eq!(b.drawing.views.len(), 3);
            assert_eq!(b.drawing.balloons.len(), 3);
        }
    }
    assert_eq!(long_rows().len(), 12);
    assert!(long_rows().iter().all(|r| r[1].chars().count() <= 60
        && r[3].chars().count() <= 60
        && r[4].chars().count() <= 80));
}
