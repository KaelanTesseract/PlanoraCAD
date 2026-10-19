//! Proben für den Handtest der PDF-Ausgabe (Teilprojekt 3c-3). Zu jeder PDF liegt die SVG daneben
//! (bei mehrseitigen PDFs je Seite eine SVG).
//!
//! - `Blatt_<Größe>_<Rahmen|ohneRahmen>`: Quader mit Bohrung in Vorne, Oben, Rechts und
//!   Isometrie mit Mittellinien und Maßen („Ø 10 ± 0,1“, „20 −0,1“), A4 hoch, A4 quer, A3 quer,
//!   A0 quer.
//! - `Stueckliste_<Größe>_<kurz|lang>_<Rahmen|ohneRahmen>`: dieselben Ansichten mit Stückliste auf
//!   dem Blatt (3 Zeilen, 12 Zeilen mit Längstexten und Sonderzeichen, davon 10 auf dem Blatt) und
//!   den Nummernkreisen 1, 12 und 200.
//! - `Stuecklistenblatt_<Rahmen|ohneRahmen>`: Stücklistenblatt A4 hoch, 45 Zeilen auf zwei Seiten.
//! - `alle-blaetter`: sechs Seiten gemischter Formate in einer PDF.
//!
//! Der Test `write_pdf_samples_for_the_hand_test` ist ausgeschaltet und wird so gestartet:
//!
//! `cargo test --release -p planora-drawing --test hand_samples_pdf -- --ignored`
//!
//! Zielordner ist `$PLANORA_PDF_SAMPLES_DIR`, sonst `D:\Testumgebung\zeichnung-proben\pdf`.

mod common;

use common::model::*;
use common::pdf_read;
use planora_drawing::annotate::{Mark, MarkKind, ResolvedDims, TextAnchor};
use planora_drawing::balloon::{add_balloon_marks, balloon_marks};
use planora_drawing::layout::{align_dir, view_extent};
use planora_drawing::model::{
    Balloon, BalloonId, Drawing, DrawingId, DrawingKind, Paper, Placement, Projection, Scale,
    Sheet, ViewId,
};
use planora_drawing::parts::{page_rows, parts_sheet_name};
use planora_drawing::sheet::{SheetModel, build_sheet_with};
use planora_drawing::sheet_frame::{
    PartsSpace, parts_rows_per_page, sheet_layout_with, sheet_scale,
};
use planora_drawing::sheet_info::{
    FieldValue, NotesValue, SheetContext, SheetInfo, Stamp, main_scale, resolve_texts,
};
use planora_drawing::sheet_marks::{SheetFrame, parts_table_marks, sheet_frame};
use planora_drawing::{
    BodyKey, DrawBody, PartsCells, PdfMeta, ViewKind, ViewLines, compute_view, pdf_bytes,
    svg_string,
};
use planora_exchange::Progress;
use planora_math::{DVec2, DVec3};
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
        date: "2026-10-19".into(),
        user: "Probe".into(),
    }
}

fn meta(title: &str) -> PdfMeta {
    PdfMeta {
        title: title.into(),
        author: None,
        date: Some("2026-10-19".into()),
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
                    "S235JR (1.0038), verzinkt, 3 mm ± 0,1 µ € WWWWWWWWWWWWWWWWWWWWWW",
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

fn text_mark(pos: DVec2, h: f64, deg: f64, anchor: TextAnchor, s: &str) -> Mark {
    Mark::Text {
        pos,
        height: h,
        angle: deg.to_radians(),
        anchor,
        text: s.into(),
        kind: MarkKind::Dimension,
        fit_width: None,
    }
}

fn line_mark(a: DVec2, b: DVec2) -> Mark {
    Mark::Line {
        a,
        b,
        kind: MarkKind::Dimension,
    }
}

/// Maße der Vorderansicht (Mitte `c`, Breite `w`, Höhe `h`, Schriftgröße `th`): Breite 40 unten,
/// Höhe „20 −0,1“ rechts (Text gedreht), Bohrung „Ø 10 ± 0,1“ mit Hinweislinie.
fn dim_marks(c: DVec2, w: f64, h: f64, k: f64) -> Vec<Mark> {
    let th = 3.5 * k;
    let mut m = Vec::new();
    let y = c.y - h / 2.0 - 8.0 * k;
    let (x0, x1) = (c.x - w / 2.0, c.x + w / 2.0);
    m.push(line_mark(
        DVec2::new(x0, c.y - h / 2.0 - 1.0 * k),
        DVec2::new(x0, y - 2.0 * k),
    ));
    m.push(line_mark(
        DVec2::new(x1, c.y - h / 2.0 - 1.0 * k),
        DVec2::new(x1, y - 2.0 * k),
    ));
    m.push(line_mark(DVec2::new(x0, y), DVec2::new(x1, y)));
    m.push(Mark::Arrow {
        tip: DVec2::new(x0, y),
        dir: DVec2::new(1.0, 0.0),
        kind: MarkKind::Dimension,
    });
    m.push(Mark::Arrow {
        tip: DVec2::new(x1, y),
        dir: DVec2::new(-1.0, 0.0),
        kind: MarkKind::Dimension,
    });
    m.push(text_mark(
        DVec2::new(c.x, y + 1.2 * k),
        th,
        0.0,
        TextAnchor::Middle,
        "40",
    ));
    // Höhe rechts, Text gedreht
    let x = c.x + w / 2.0 + 8.0 * k;
    let (y0, y1) = (c.y - h / 2.0, c.y + h / 2.0);
    m.push(line_mark(
        DVec2::new(x1 + 1.0 * k, y0),
        DVec2::new(x + 2.0 * k, y0),
    ));
    m.push(line_mark(
        DVec2::new(x1 + 1.0 * k, y1),
        DVec2::new(x + 2.0 * k, y1),
    ));
    m.push(line_mark(DVec2::new(x, y0), DVec2::new(x, y1)));
    m.push(Mark::Arrow {
        tip: DVec2::new(x, y0),
        dir: DVec2::new(0.0, 1.0),
        kind: MarkKind::Dimension,
    });
    m.push(Mark::Arrow {
        tip: DVec2::new(x, y1),
        dir: DVec2::new(0.0, -1.0),
        kind: MarkKind::Dimension,
    });
    m.push(text_mark(
        DVec2::new(x - 1.2 * k, c.y),
        th,
        90.0,
        TextAnchor::Middle,
        "20 \u{2212}0,1",
    ));
    // Bohrung (Mittelpunkt c, Radius 5 mal Maßstab nicht bekannt: Hinweis an die Mitte)
    let tip = DVec2::new(c.x + 2.0 * k, c.y + 2.0 * k);
    let knee = DVec2::new(c.x + w / 2.0 + 4.0 * k, c.y + h / 2.0 + 8.0 * k);
    m.push(line_mark(tip, knee));
    m.push(line_mark(knee, DVec2::new(knee.x + 4.0 * k, knee.y)));
    m.push(Mark::Arrow {
        tip,
        dir: (knee - tip).normalize(),
        kind: MarkKind::Dimension,
    });
    m.push(text_mark(
        DVec2::new(knee.x + 5.0 * k, knee.y - 1.2 * k),
        th,
        0.0,
        TextAnchor::Start,
        "\u{00D8} 10 \u{00B1} 0,1",
    ));
    m
}

struct Built {
    drawing: Drawing,
    views: BTreeMap<ViewId, ViewLines>,
    /// Mitte, Breite und Höhe der Vorderansicht auf dem Blatt.
    front: (DVec2, f64, f64),
}

/// Vorne, Oben, Rechts und Isometrie in vier Zellen des freien Bereichs, größter Maßstab, bei dem
/// jede Ansicht in ihre Zelle passt.
fn build(paper: Paper, landscape: bool, parts_rows: Option<usize>, long: bool) -> Built {
    let sheet = Sheet { paper, landscape };
    let projection = Projection::First;
    let solid = box_with_hole(40.0, 30.0, 20.0, 5.0, DVec3::Y);
    let bodies = [DrawBody {
        name: "Q".into(),
        solid: &solid,
    }];
    let p = Progress::new();
    let kinds = [
        ViewKind::Front,
        ViewKind::Top,
        ViewKind::Right,
        ViewKind::Isometric,
    ];
    let lines: Vec<ViewLines> = kinds
        .iter()
        .map(|&k| compute_view(&bodies, k, &p).unwrap())
        .collect();
    let space = match parts_rows {
        Some(rows) => PartsSpace::OnSheet { rows },
        None => PartsSpace::None,
    };
    let layout = sheet_layout_with(sheet, 0, if long { 6 } else { 3 }, space);
    let (lo, hi) = layout.free;
    let (free_w, free_h) = (hi.x - lo.x, hi.y - lo.y);
    let (cw, ch) = (free_w / 2.0, free_h / 2.0);
    let mut order = Scale::SERIES.to_vec();
    order.sort_by(|a, b| b.factor().partial_cmp(&a.factor()).unwrap());
    let scale = order
        .into_iter()
        .find(|&s| {
            kinds.iter().enumerate().all(|(i, _)| {
                let (w, h) = view_extent(lines[i].bbox, s);
                w <= cw - 24.0 && h <= ch - 24.0
            })
        })
        .expect("kein Maßstab passt");
    let (dxr, _) = align_dir(projection, ViewKind::Front, ViewKind::Right).unwrap();
    let (_, dyt) = align_dir(projection, ViewKind::Front, ViewKind::Top).unwrap();
    let fx = lo.x + free_w / 2.0 - dxr * cw / 2.0;
    let fy = lo.y + free_h / 2.0 - dyt * ch / 2.0;
    let mut d = Drawing::new(DrawingId(1), "Haltewinkel".into(), sheet, projection);
    let f = d.add_view(
        ViewKind::Front,
        scale,
        Placement::Free { x: fx, y: fy },
        None,
    );
    let t = d.add_view(
        ViewKind::Top,
        scale,
        Placement::Aligned { distance: ch },
        Some(f),
    );
    let r = d.add_view(
        ViewKind::Right,
        scale,
        Placement::Aligned { distance: cw },
        Some(f),
    );
    let i = d.add_view(
        ViewKind::Isometric,
        scale,
        Placement::Free {
            x: fx + dxr * cw,
            y: fy + dyt * ch,
        },
        None,
    );
    for v in &mut d.views {
        v.centerlines = true;
    }
    let mut info = SheetInfo::new(&stamp());
    if parts_rows.is_some() {
        info.parts_list = true;
    }
    if long {
        info.notes = NotesValue::Lines(
            (1..=6)
                .map(|n| format!("Hinweis {n}: Kanten gebrochen 0,3 mm, ä ö ü ß Ø ° ± µ €"))
                .collect(),
        );
        info.title = FieldValue::Text("Haltewinkel für die Gehäuseabdeckung links oben".into());
    }
    d.info = Some(info);
    if parts_rows.is_some() {
        let k = sheet_scale(sheet);
        for (view, anchor, offset, body) in [
            (f, (0.3, 0.7), (10.0, 10.0), 0u64),
            (t, (0.5, 0.5), (-12.0, -9.0), 1),
            (r, (0.8, 0.2), (12.0, 14.0), 2),
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
    }
    let (fw, fh) = view_extent(lines[0].bbox, scale);
    let mut views = BTreeMap::new();
    for (id, l) in [f, t, r, i].into_iter().zip(lines) {
        views.insert(id, l);
    }
    Built {
        drawing: d,
        views,
        front: (DVec2::new(fx, fy), fw, fh),
    }
}

/// Ein Blatt mit und ohne Rahmen: (ohne, mit).
fn sheet_pair(
    paper: Paper,
    landscape: bool,
    parts: Option<Vec<PartsCells>>,
    long: bool,
) -> [SheetModel; 2] {
    let rows_on_sheet = parts.as_ref().map(|r| r.len().min(10));
    let b = build(paper, landscape, rows_on_sheet, long);
    let refs: BTreeMap<_, &ViewLines> = b.views.iter().map(|(k, v)| (*k, v)).collect();
    let mut model = build_sheet_with(&b.drawing, &refs, &ResolvedDims::default());
    assert_eq!(model.views.len(), 4);
    let k = sheet_scale(b.drawing.sheet);
    let front = model.views.iter_mut().find(|v| v.name == "Vorne").unwrap();
    front
        .marks
        .extend(dim_marks(b.front.0, b.front.1, b.front.2, k));
    if parts.is_some() {
        let mut numbers = BTreeMap::new();
        for (bl, n) in b.drawing.balloons.iter().zip([1u32, 12, 200]) {
            numbers.insert(bl.id, Some(n));
        }
        add_balloon_marks(&mut model, &balloon_marks(&b.drawing, &refs, &numbers));
    }
    let ctx = SheetContext {
        doc_name: "Probe".into(),
        index: 1,
        count: 1,
    };
    let mut texts = resolve_texts(&b.drawing, None, &ctx, main_scale(&b.drawing));
    if let Some(rows) = parts {
        texts.parts = rows.into_iter().take(10).collect();
    }
    let layout = sheet_layout_with(
        b.drawing.sheet,
        texts.visible_revisions().len(),
        texts.notes.len(),
        match rows_on_sheet {
            Some(_) => PartsSpace::OnSheet {
                rows: texts.parts.len(),
            },
            None => PartsSpace::None,
        },
    );
    let bare = model.clone();
    model.frame = Some(sheet_frame(&layout, &texts, b.drawing.projection));
    [bare, model]
}

/// Stücklistenblatt (A4 hoch, 45 Zeilen: 44 auf Seite 1, 1 auf Seite 2): [Seite][ohne, mit].
fn parts_pages(page_count: usize) -> Vec<[SheetModel; 2]> {
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
    (1..=2u32)
        .map(|page| {
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
                count: page_count,
            };
            let mut texts = resolve_texts(&d, None, &ctx, None);
            texts.parts = page_rows(&all, page, per_page).to_vec();
            let layout = sheet_layout_with(
                sheet,
                texts.visible_revisions().len(),
                0,
                PartsSpace::ListPage,
            );
            model.frame = Some(SheetFrame {
                marks: parts_table_marks(&layout, &texts.parts, 1.0),
                scale: layout.scale,
            });
            let bare = model.clone();
            model.frame = Some(sheet_frame(&layout, &texts, d.projection));
            [bare, model]
        })
        .collect()
}

/// Schreibt eine PDF (alle Seiten) und je Seite eine SVG; prüft die PDF mit dem eigenen Leser.
fn put(dir: &Path, base: &str, pages: &[&SheetModel], written: &mut Vec<PathBuf>) {
    let bytes = pdf_bytes(pages, &meta(base)).unwrap_or_else(|e| panic!("{base}: {e}"));
    let pdf = pdf_read::read(&bytes).unwrap_or_else(|e| panic!("{base}: {e}"));
    assert_eq!(pdf.pages.len(), pages.len(), "{base}");
    let path = dir.join(format!("{base}.pdf"));
    std::fs::write(&path, bytes).unwrap();
    written.push(path);
    for (i, m) in pages.iter().enumerate() {
        let name = if pages.len() == 1 {
            format!("{base}.svg")
        } else {
            format!("{base}_Seite{}.svg", i + 1)
        };
        let path = dir.join(name);
        std::fs::write(&path, svg_string(m).unwrap()).unwrap();
        written.push(path);
    }
}

fn write_all(dir: &Path) -> Vec<PathBuf> {
    std::fs::create_dir_all(dir).unwrap();
    let mut written = Vec::new();
    // Quader mit Maßen, Mittellinien und Isometrie
    for (name, paper, landscape) in SIZES {
        let [bare, framed] = sheet_pair(paper, landscape, None, false);
        put(
            dir,
            &format!("Blatt_{name}_ohneRahmen"),
            &[&bare],
            &mut written,
        );
        put(
            dir,
            &format!("Blatt_{name}_Rahmen"),
            &[&framed],
            &mut written,
        );
    }
    // mit Stückliste und Nummernkreisen 1, 12, 200
    for (name, paper, landscape) in SIZES {
        for (vname, long) in [("kurz", false), ("lang", true)] {
            let rows = if long { long_rows() } else { short_rows() };
            let [bare, framed] = sheet_pair(paper, landscape, Some(rows), long);
            put(
                dir,
                &format!("Stueckliste_{name}_{vname}_ohneRahmen"),
                &[&bare],
                &mut written,
            );
            put(
                dir,
                &format!("Stueckliste_{name}_{vname}_Rahmen"),
                &[&framed],
                &mut written,
            );
        }
    }
    // Stücklistenblatt mit zwei Seiten
    let parts = parts_pages(2);
    put(
        dir,
        "Stuecklistenblatt_ohneRahmen",
        &[&parts[0][0], &parts[1][0]],
        &mut written,
    );
    put(
        dir,
        "Stuecklistenblatt_Rahmen",
        &[&parts[0][1], &parts[1][1]],
        &mut written,
    );
    // eine PDF mit sechs Seiten gemischter Formate
    let a4_hoch = sheet_pair(Paper::A4, false, None, false);
    let a3_lang = sheet_pair(Paper::A3, true, Some(long_rows()), true);
    let a0_kurz = sheet_pair(Paper::A0, true, Some(short_rows()), false);
    let a4_quer = sheet_pair(Paper::A4, true, None, false);
    let all: Vec<&SheetModel> = vec![
        &a4_hoch[1],
        &a3_lang[1],
        &parts[0][1],
        &parts[1][1],
        &a0_kurz[1],
        &a4_quer[1],
    ];
    put(dir, "alle-blaetter", &all, &mut written);
    std::fs::write(
        dir.join("ERWARTUNG.txt"),
        "Proben der PDF-Ausgabe. Zu jeder PDF liegt die SVG daneben (mehrseitig: je Seite eine).\n\
         Zu prüfen im PDF-Betrachter: Seitengröße und Ausrichtung, Texte in den Zellen des\n\
         Schriftfelds und der Stückliste, Umlaute, Ø ° ± µ €, Minus in \"20 -0,1\", Linienstärken\n\
         wie in der SVG, verdeckte Kanten gestrichelt, Mittellinien Strich-Punkt, Nummernkreise\n\
         1, 12 und 200. alle-blaetter.pdf hat 6 Seiten (A4 hoch, A3 quer, Stücklistenblatt 2 Seiten,\n\
         A0 quer, A4 quer).\n",
    )
    .unwrap();
    written
}

fn samples_dir() -> PathBuf {
    match std::env::var_os("PLANORA_PDF_SAMPLES_DIR") {
        Some(d) if !d.is_empty() => PathBuf::from(d),
        _ => PathBuf::from(r"D:\Testumgebung\zeichnung-proben\pdf"),
    }
}

#[test]
#[ignore = "schreibt Dateien für den Handtest: --release -- --ignored"]
fn write_pdf_samples_for_the_hand_test() {
    let dir = samples_dir();
    let written = write_all(&dir);
    let pdfs = written
        .iter()
        .filter(|p| p.extension().is_some_and(|e| e == "pdf"))
        .count();
    let svgs = written
        .iter()
        .filter(|p| p.extension().is_some_and(|e| e == "svg"))
        .count();
    // 8 Blätter + 16 mit Stückliste + 2 Stücklistenblätter + 1 mehrseitige PDF
    assert_eq!(pdfs, 8 + 16 + 2 + 1);
    // je PDF eine SVG; die Stücklistenblätter zweiseitig, alle-blaetter sechsseitig
    assert_eq!(svgs, 8 + 16 + 4 + 6);
    println!("Proben liegen in {}", dir.display());
}

/// Damit der Schreiber nicht verrottet: Aufbau und PDF ohne Dateien.
#[test]
fn pdf_sample_builder_still_works() {
    for (paper, landscape) in [(Paper::A4, false), (Paper::A0, true)] {
        for long in [false, true] {
            let rows = if long { long_rows() } else { short_rows() };
            let [bare, framed] = sheet_pair(paper, landscape, Some(rows), long);
            let bytes = pdf_bytes(&[&bare, &framed], &meta("Probe")).unwrap();
            assert_eq!(pdf_read::read(&bytes).unwrap().pages.len(), 2);
        }
    }
    let pages = parts_pages(2);
    assert_eq!(pages.len(), 2);
    assert_eq!(long_rows().len(), 12);
}
