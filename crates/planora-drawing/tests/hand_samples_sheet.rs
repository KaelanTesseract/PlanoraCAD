//! Proben für den Handtest des Zeichnungsblatts (Teilprojekt 3c-1): SVG und DXF mit und ohne
//! Blattrahmen für A4 hoch, A4 quer, A3 quer, A2 hoch und A0 quer, Projektionsmethode 1 und 3,
//! je mit Standardangaben ("kurz") und mit Längstexten und Sonderzeichen ("lang").
//!
//! Der Test `write_sheet_samples_for_the_hand_test` ist ausgeschaltet und wird so gestartet:
//!
//! `cargo test --release -p planora-drawing --test hand_samples_sheet -- --ignored`
//!
//! Zielordner ist `$PLANORA_SHEET_SAMPLES_DIR`, sonst `D:\Testumgebung\zeichnung-proben\blatt`.

mod common;

use common::model::*;
use planora_drawing::annotate::ResolvedDims;
use planora_drawing::layout::{GAP_MM, align_dir, view_extent};
use planora_drawing::model::{Drawing, DrawingId, Paper, Placement, Projection, Scale, Sheet};
use planora_drawing::sheet::build_sheet_with;
use planora_drawing::sheet_frame::sheet_layout;
use planora_drawing::sheet_info::{
    FieldValue, NotesValue, ProjectInfo, Revision, SheetContext, SheetInfo, Stamp, main_scale,
    resolve_texts,
};
use planora_drawing::sheet_marks::{sheet_frame, sheet_frame_dxf};
use planora_drawing::{DrawBody, ViewKind, ViewLines, compute_view, dxf_string, svg_string};
use planora_exchange::Progress;
use planora_math::DVec3;
use std::collections::BTreeMap;
use std::path::PathBuf;

const SIZES: [(&str, Paper, bool); 5] = [
    ("A4hoch", Paper::A4, false),
    ("A4quer", Paper::A4, true),
    ("A3quer", Paper::A3, true),
    ("A2hoch", Paper::A2, false),
    ("A0quer", Paper::A0, true),
];

fn text(s: &str) -> FieldValue {
    FieldValue::Text(s.to_string())
}

/// Längstexte (bis an die Grenzen der Felder) mit Sonderzeichen.
fn long_info(stamp: &Stamp) -> (ProjectInfo, SheetInfo) {
    let mut p = ProjectInfo::new(stamp);
    p.owner = text("Müller & Söhne Präzisionsteile Größenwahn GmbH & Co. KG, Äußere Weg 12");
    p.creator = text("Dr. Ing. Jürgen Großmann-Weißenfels");
    p.checked = text("Prüfer Ø ° ± äöüß");
    p.approved = text("Freigeber Ä Ö Ü");
    p.material = text("S235JR (1.0038), verzinkt, 3 mm ± 0,1");
    let mut s = SheetInfo::new(stamp);
    s.number = text("WWWWWWWWWWWWWWWWWWWW-2026-10-17-0001");
    s.title = text("Haltewinkel für die Gehäuseabdeckung links oben, Ausführung groß");
    s.subtitle = text("Zusatztitel mit Umlauten: Übergröße, Öffnung, Maße ± 0,05 mm, Ø 12 °");
    s.doc_type = text("Fertigungszeichnung mit Prüfmerkmalen");
    s.status = text("zur Freigabe vorgelegt, nicht für die Fertigung");
    s.weight = text("12,345 kg ± 0,5 %");
    s.notes = NotesValue::Lines(
        (1..=6)
            .map(|i| {
                format!(
                    "Hinweis {i}: Oberflächen Ra 3,2 soweit nicht anders angegeben, Kanten gebrochen 0,3 mm, ä ö ü ß Ø ° ±"
                )
            })
            .collect(),
    );
    for i in 0..23 {
        s.revisions.push(Revision {
            letter: FieldValue::Auto,
            description: format!(
                "Änderung {} der Bohrung auf Ø 10 H7 und der Wanddicke auf 3 ± 0,1 mm, lange Beschreibung",
                i + 1
            ),
            date: "2026-10-17".into(),
            name: "Müller-Weißenfels".into(),
        });
    }
    (p, s)
}

struct Built {
    drawing: Drawing,
    views: BTreeMap<planora_drawing::model::ViewId, ViewLines>,
}

/// Drei Ansichten (Vorne, Oben, Links) als Block mittig im freien Bereich, größter Maßstab, bei
/// dem der Block hineinpasst.
fn build(paper: Paper, landscape: bool, projection: Projection, long: bool) -> Built {
    let sheet = Sheet { paper, landscape };
    let stamp = Stamp {
        date: "2026-10-17".into(),
        user: "Probe".into(),
    };
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
    let layout = notes_layout(sheet, long);
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
    if long {
        let (pi, si) = long_info(&stamp);
        let _ = pi;
        d.info = Some(si);
    } else {
        d.info = Some(SheetInfo::new(&stamp));
    }
    let mut views = BTreeMap::new();
    for (id, l) in [f, t, l].into_iter().zip(lines) {
        views.insert(id, l);
    }
    Built { drawing: d, views }
}

/// Blattgeometrie nach den Zeilenzahlen der Angaben (lang: 6 Änderungen sichtbar, 6 Hinweise).
fn notes_layout(sheet: Sheet, long: bool) -> planora_drawing::SheetLayout {
    if long {
        sheet_layout(sheet, 6, 6)
    } else {
        sheet_layout(sheet, 0, 3)
    }
}

fn write_all(dir: &PathBuf) {
    std::fs::create_dir_all(dir).unwrap();
    let stamp = Stamp {
        date: "2026-10-17".into(),
        user: "Probe".into(),
    };
    for (name, paper, landscape) in SIZES {
        for (mname, projection) in [("M1", Projection::First), ("M3", Projection::Third)] {
            for (vname, long) in [("kurz", false), ("lang", true)] {
                let b = build(paper, landscape, projection, long);
                let refs: BTreeMap<_, &ViewLines> = b.views.iter().map(|(k, v)| (*k, v)).collect();
                let mut model = build_sheet_with(&b.drawing, &refs, &ResolvedDims::default());
                assert_eq!(model.views.len(), 3);
                let project = if long {
                    long_info(&stamp).0
                } else {
                    ProjectInfo::new(&stamp)
                };
                let ctx = SheetContext {
                    doc_name: "Probe".into(),
                    index: 1,
                    count: 3,
                };
                let texts = resolve_texts(&b.drawing, Some(&project), &ctx, main_scale(&b.drawing));
                let layout = sheet_layout(
                    b.drawing.sheet,
                    texts.visible_revisions().len(),
                    texts.notes.len(),
                );
                // ohne Rahmen
                let base = format!("{name}_{mname}_{vname}");
                std::fs::write(
                    dir.join(format!("{base}_ohneRahmen.svg")),
                    svg_string(&model).unwrap(),
                )
                .unwrap();
                std::fs::write(
                    dir.join(format!("{base}_ohneRahmen.dxf")),
                    dxf_string(&model).unwrap(),
                )
                .unwrap();
                model.frame = Some(sheet_frame(&layout, &texts, b.drawing.projection));
                std::fs::write(
                    dir.join(format!("{base}_Rahmen.svg")),
                    svg_string(&model).unwrap(),
                )
                .unwrap();
                // DXF wie der Export: Texte mit dem Breitenzuschlag für CAD-Schriften.
                model.frame = Some(sheet_frame_dxf(&layout, &texts, b.drawing.projection));
                match dxf_string(&model) {
                    Ok(s) => std::fs::write(dir.join(format!("{base}_Rahmen.dxf")), s).unwrap(),
                    Err(e) => panic!("{base}: DXF mit Rahmen scheitert: {e}"),
                }
            }
        }
    }
}

fn samples_dir() -> PathBuf {
    match std::env::var_os("PLANORA_SHEET_SAMPLES_DIR") {
        Some(d) if !d.is_empty() => PathBuf::from(d),
        _ => PathBuf::from(r"D:\Testumgebung\zeichnung-proben\blatt"),
    }
}

#[test]
#[ignore = "schreibt Dateien für den Handtest: --release -- --ignored"]
fn write_sheet_samples_for_the_hand_test() {
    let dir = samples_dir();
    write_all(&dir);
    let n = std::fs::read_dir(&dir).unwrap().count();
    assert_eq!(n, 5 * 2 * 2 * 4);
    println!("Proben liegen in {}", dir.display());
}

/// Damit der Schreiber nicht verrottet: eine Größe in den Temp-Ordner.
#[test]
fn sheet_sample_builder_still_works() {
    for (paper, landscape) in [(Paper::A4, false), (Paper::A0, true)] {
        for long in [false, true] {
            let b = build(paper, landscape, Projection::Third, long);
            assert_eq!(b.drawing.views.len(), 3);
        }
    }
}
