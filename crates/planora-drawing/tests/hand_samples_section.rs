//! Proben für den Handtest der Schnittansichten (Teilprojekt 3d). Zu jeder Probe gibt es SVG, DXF
//! und PDF, je ohne und mit Blattrahmen.
//!
//! - `RohrQuer_A4quer`: Rohr, Schnitt quer zur Achse (Ring mit freier Bohrung).
//! - `RohrLaengs_A4quer`: dasselbe Rohr längs durch die Achse (zwei Wandstreifen).
//! - `Quader_A3`: Quader mit zwei Bohrungen und einer Nut, A3, Maßstab 1:2, Schnitt A (durch die
//!   Bohrungen) mit Maßen und Schnitt B (durch die Nut).
//! - `PlatteBolzen_A4quer`: Platte mit Bolzen in der Bohrung, zwei Körper (45° und 135°), 2:1.
//! - `Stufe_A4quer`: Stufenkörper, Schnitt A genau auf der Stufenfläche (Ebene wird um 0,001 mm
//!   verschoben), Schnitt B quer dazu.
//! - `DreiSchnitte_A0quer`: A0 mit den Schnitten A, B und C.
//!
//! Der Test `write_section_samples_for_the_hand_test` ist ausgeschaltet und wird so gestartet:
//!
//! `cargo test --release -p planora-drawing --test hand_samples_section -- --ignored`
//!
//! Zielordner ist `$PLANORA_SECTION_SAMPLES_DIR`, sonst `D:\Testumgebung\zeichnung-proben\schnitt`.

mod common;

use common::model::{axis_cylinder, box_at, box_solid, box_with_hole, cylinder_solid, translated};
use common::{pdf_read, svg_read};
use planora_drawing::annotate::{Mark, MarkKind, ResolvedDims, TextAnchor};
use planora_drawing::layout::{align_dir, view_extent};
use planora_drawing::section::{SectionPlane, compute_section};
use planora_drawing::section_marks::{add_section_marks, section_marks};
use planora_drawing::sheet::{SheetModel, build_sheet_with, view_transform};
use planora_drawing::sheet_frame::{PartsSpace, sheet_layout_with, sheet_scale};
use planora_drawing::sheet_info::{SheetContext, SheetInfo, Stamp, main_scale, resolve_texts};
use planora_drawing::sheet_marks::sheet_frame;
use planora_drawing::{
    DrawBody, Drawing, DrawingId, Paper, PdfMeta, Placement, Projection, Scale, Sheet, ViewId,
    ViewKind, ViewLines, compute_view, dxf_string, pdf_bytes, svg_string,
};
use planora_exchange::Progress;
use planora_kernel::{AnalyticKernel, BoolOp, Kernel, Solid};
use planora_math::{DVec2, DVec3};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

fn k() -> AnalyticKernel {
    AnalyticKernel::default()
}

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

fn minus(a: &Solid, b: &Solid) -> Solid {
    k().boolean(a, b, BoolOp::Difference).unwrap()
}

/// Rohr: außen r 20, innen r 8, Höhe 40, Achse z durch den Ursprung.
fn tube() -> Solid {
    let outer = cylinder_solid(20.0, 40.0);
    let inner = translated(&cylinder_solid(8.0, 44.0), DVec3::new(0.0, 0.0, -2.0));
    minus(&outer, &inner)
}

/// Quader 100 × 60 × 40, zwei Bohrungen (r 6) längs y bei x = 25 und 75 auf Höhe 20, Nut
/// 10 breit und 5 tief oben bei x = 45 bis 55, durchgehend längs y.
fn block() -> Solid {
    let mut s = box_solid(100.0, 60.0, 40.0);
    for x in [25.0, 75.0] {
        let drill = axis_cylinder(6.0, 40.0, DVec3::Y, DVec3::new(x, 30.0, 20.0));
        s = minus(&s, &drill);
    }
    minus(&s, &box_at((45.0, -1.0, 35.0), (55.0, 61.0, 41.0)))
}

/// Zwei Quader übereinander: unten 80 × 40 × 10, oben 40 × 40 (z 10 bis 30).
fn steps() -> Solid {
    let low = box_solid(80.0, 40.0, 10.0);
    let high = box_at((0.0, 0.0, 10.0), (40.0, 40.0, 30.0));
    k().boolean(&low, &high, BoolOp::Union).unwrap()
}

struct Cut {
    kind: ViewKind,
    offset: f64,
}

struct Sample {
    name: &'static str,
    paper: Paper,
    scale: Scale,
    bodies: Vec<(&'static str, Solid)>,
    cuts: Vec<Cut>,
    /// Maße um die Umrisse der Schnittansichten legen.
    dims: bool,
}

struct Built {
    bare: SheetModel,
    framed: SheetModel,
    nudged: Vec<bool>,
}

fn line_mark(a: DVec2, b: DVec2) -> Mark {
    Mark::Line {
        a,
        b,
        kind: MarkKind::Dimension,
    }
}

fn arrow(tip: DVec2, dir: DVec2) -> Mark {
    Mark::Arrow {
        tip,
        dir,
        kind: MarkKind::Dimension,
    }
}

fn text(pos: DVec2, h: f64, deg: f64, s: &str) -> Mark {
    Mark::Text {
        pos,
        height: h,
        angle: deg.to_radians(),
        anchor: TextAnchor::Middle,
        text: s.into(),
        kind: MarkKind::Dimension,
        fit_width: None,
    }
}

/// Breite oben, Höhe rechts um den Umriss `lo`..`hi` (Blattkoordinaten).
fn outline_dims(lo: DVec2, hi: DVec2, f: f64, w: &str, h: &str) -> Vec<Mark> {
    let th = 3.5 * f;
    let mut m = Vec::new();
    let y = hi.y + 8.0 * f;
    m.push(line_mark(
        DVec2::new(lo.x, hi.y + 1.0 * f),
        DVec2::new(lo.x, y + 2.0 * f),
    ));
    m.push(line_mark(
        DVec2::new(hi.x, hi.y + 1.0 * f),
        DVec2::new(hi.x, y + 2.0 * f),
    ));
    m.push(line_mark(DVec2::new(lo.x, y), DVec2::new(hi.x, y)));
    m.push(arrow(DVec2::new(lo.x, y), DVec2::new(1.0, 0.0)));
    m.push(arrow(DVec2::new(hi.x, y), DVec2::new(-1.0, 0.0)));
    m.push(text(
        DVec2::new((lo.x + hi.x) / 2.0, y + 1.2 * f),
        th,
        0.0,
        w,
    ));
    let x = hi.x + 8.0 * f;
    m.push(line_mark(
        DVec2::new(hi.x + 1.0 * f, lo.y),
        DVec2::new(x + 2.0 * f, lo.y),
    ));
    m.push(line_mark(
        DVec2::new(hi.x + 1.0 * f, hi.y),
        DVec2::new(x + 2.0 * f, hi.y),
    ));
    m.push(line_mark(DVec2::new(x, lo.y), DVec2::new(x, hi.y)));
    m.push(arrow(DVec2::new(x, lo.y), DVec2::new(0.0, 1.0)));
    m.push(arrow(DVec2::new(x, hi.y), DVec2::new(0.0, -1.0)));
    m.push(text(
        DVec2::new(x - 1.2 * f, (lo.y + hi.y) / 2.0),
        th,
        90.0,
        h,
    ));
    m
}

fn num(v: f64) -> String {
    format!("{v:.1}").replace('.', ",")
}

fn build(s: &Sample) -> Built {
    let bodies: Vec<DrawBody> = s
        .bodies
        .iter()
        .map(|(n, so)| DrawBody {
            name: (*n).into(),
            solid: so,
        })
        .collect();
    let sheet = Sheet {
        paper: s.paper,
        landscape: true,
    };
    let projection = Projection::First;
    let f = sheet_scale(sheet);
    let p = Progress::new();
    let front_lines = compute_view(&bodies, ViewKind::Front, &p).unwrap();
    let sections: Vec<_> = s
        .cuts
        .iter()
        .map(|c| {
            let plane = SectionPlane {
                kind: c.kind,
                offset: c.offset,
            };
            compute_section(&bodies, &plane, compute_view, &p)
                .unwrap_or_else(|e| panic!("{}: Schnitt {:?}: {e}", s.name, c.kind))
        })
        .collect();

    // Lage: Abstände wie die App (Mitte zu Mitte, weitere Schnitte dahinter), Gruppe mittig.
    let layout = sheet_layout_with(sheet, 0, 3, PartsSpace::None);
    let (lo, hi) = layout.free;
    let (fw, fh) = view_extent(front_lines.bbox, s.scale);
    let gap = 18.0 * f;
    let mut far: BTreeMap<(i32, i32), f64> = BTreeMap::new();
    let mut rects = vec![(DVec2::ZERO, fw, fh)];
    let mut distances = Vec::new();
    for (c, sec) in s.cuts.iter().zip(&sections) {
        let (dx, dy) = align_dir(projection, ViewKind::Front, c.kind).unwrap();
        let key = (dx as i32, dy as i32);
        let (w, h) = view_extent(sec.lines.bbox, s.scale);
        let (half_self, half_child) = if dx != 0.0 {
            (fw / 2.0, w / 2.0)
        } else {
            (fh / 2.0, h / 2.0)
        };
        let edge = *far.get(&key).unwrap_or(&half_self);
        let dist = edge + gap + half_child;
        far.insert(key, dist + half_child);
        distances.push(dist);
        rects.push((DVec2::new(dx * dist, dy * dist), w, h));
    }
    let (mut gx0, mut gy0, mut gx1, mut gy1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for (c, w, h) in &rects {
        gx0 = gx0.min(c.x - w / 2.0);
        gx1 = gx1.max(c.x + w / 2.0);
        gy0 = gy0.min(c.y - h / 2.0);
        gy1 = gy1.max(c.y + h / 2.0);
    }
    let fx = (lo.x + hi.x) / 2.0 - (gx0 + gx1) / 2.0;
    let fy = (lo.y + hi.y) / 2.0 - (gy0 + gy1) / 2.0;

    let mut d = Drawing::new(DrawingId(1), s.name.into(), sheet, projection);
    let front = d.add_view(
        ViewKind::Front,
        s.scale,
        Placement::Free { x: fx, y: fy },
        None,
    );
    let mut ids: Vec<ViewId> = Vec::new();
    for (c, dist) in s.cuts.iter().zip(&distances) {
        ids.push(d.add_section(front, c.kind, c.offset, *dist).unwrap());
    }
    for v in &mut d.views {
        v.centerlines = true;
    }
    d.info = Some(SheetInfo::new(&stamp()));

    let mut results: BTreeMap<ViewId, &ViewLines> = BTreeMap::from([(front, &front_lines)]);
    let mut data = BTreeMap::new();
    for (id, sec) in ids.iter().zip(&sections) {
        results.insert(*id, &sec.lines);
        data.insert(*id, &sec.data);
    }
    let mut model = build_sheet_with(&d, &results, &ResolvedDims::default());
    let (list, errors) = section_marks(&d, &results, &data);
    assert!(errors.is_empty(), "{}: {errors:?}", s.name);
    add_section_marks(&mut model, &list);
    for sec in &sections {
        assert!(
            !sec.data.regions.is_empty(),
            "{}: eine Schnittansicht ohne Schnittfläche",
            s.name
        );
    }

    if s.dims {
        for (id, sec) in ids.iter().zip(&sections).take(1) {
            let xf = view_transform(&d, *id, &results).unwrap();
            let ((x0, y0), (x1, y1)) = sec.lines.bbox;
            let a = xf.apply(DVec2::new(x0, y0));
            let b = xf.apply(DVec2::new(x1, y1));
            let marks = outline_dims(a.min(b), a.max(b), f, &num(x1 - x0), &num(y1 - y0));
            let view = model.views.iter_mut().find(|v| v.id == *id).unwrap();
            view.marks.extend(marks);
        }
    }

    let ctx = SheetContext {
        doc_name: "Probe".into(),
        index: 1,
        count: 1,
    };
    let texts = resolve_texts(&d, None, &ctx, main_scale(&d));
    let layout = sheet_layout_with(
        sheet,
        texts.visible_revisions().len(),
        texts.notes.len(),
        PartsSpace::None,
    );
    let bare = model.clone();
    model.frame = Some(sheet_frame(&layout, &texts, projection));
    Built {
        bare,
        framed: model,
        nudged: sections.iter().map(|x| x.data.nudged).collect(),
    }
}

fn samples() -> Vec<Sample> {
    let one = Scale { num: 1, den: 1 };
    let half = Scale { num: 1, den: 2 };
    let two = Scale { num: 2, den: 1 };
    let plate = box_with_hole(60.0, 60.0, 10.0, 8.0, DVec3::Z);
    let bolt = axis_cylinder(7.5, 15.0, DVec3::Z, DVec3::new(30.0, 30.0, 5.0));
    vec![
        Sample {
            name: "RohrQuer_A4quer",
            paper: Paper::A4,
            scale: one,
            bodies: vec![("Rohr", tube())],
            cuts: vec![Cut {
                kind: ViewKind::Top,
                offset: 20.0,
            }],
            dims: false,
        },
        Sample {
            name: "RohrLaengs_A4quer",
            paper: Paper::A4,
            scale: one,
            bodies: vec![("Rohr", tube())],
            cuts: vec![Cut {
                kind: ViewKind::Left,
                offset: 0.0,
            }],
            dims: false,
        },
        Sample {
            name: "Quader_A3quer",
            paper: Paper::A3,
            scale: half,
            bodies: vec![("Quader", block())],
            cuts: vec![
                Cut {
                    kind: ViewKind::Top,
                    offset: 20.0,
                },
                Cut {
                    kind: ViewKind::Left,
                    offset: 50.0,
                },
            ],
            dims: true,
        },
        Sample {
            name: "PlatteBolzen_A4quer",
            paper: Paper::A4,
            scale: two,
            bodies: vec![("Platte", plate), ("Bolzen", bolt)],
            cuts: vec![Cut {
                kind: ViewKind::Left,
                offset: 30.0,
            }],
            dims: false,
        },
        Sample {
            name: "Stufe_A4quer",
            paper: Paper::A4,
            scale: one,
            bodies: vec![("Stufe", steps())],
            cuts: vec![
                Cut {
                    kind: ViewKind::Top,
                    offset: 10.0,
                },
                Cut {
                    kind: ViewKind::Left,
                    offset: 20.0,
                },
            ],
            dims: false,
        },
        Sample {
            name: "DreiSchnitte_A0quer",
            paper: Paper::A0,
            scale: half,
            bodies: vec![("Block", box_with_hole(400.0, 300.0, 200.0, 40.0, DVec3::Y))],
            cuts: vec![
                Cut {
                    kind: ViewKind::Top,
                    offset: 105.0,
                },
                Cut {
                    kind: ViewKind::Left,
                    offset: 200.0,
                },
                Cut {
                    kind: ViewKind::Top,
                    offset: 125.0,
                },
            ],
            dims: false,
        },
    ]
}

fn put(dir: &Path, base: &str, model: &SheetModel, written: &mut Vec<PathBuf>) {
    let svg = svg_string(model).unwrap_or_else(|e| panic!("{base}: {e}"));
    let parsed = svg_read::read(&svg);
    assert!(
        parsed.group_ids.iter().any(|g| g.ends_with("Schraffur")),
        "{base}: keine Schraffurgruppe"
    );
    let dxf = dxf_string(model).unwrap_or_else(|e| panic!("{base}: {e}"));
    assert!(dxf.contains("SCHRAFFUR"), "{base}: keine Schraffurebene");
    assert!(dxf.contains("SCHNITTLINIE"), "{base}: keine Schnittlinie");
    let pdf = pdf_bytes(&[model], &meta(base)).unwrap_or_else(|e| panic!("{base}: {e}"));
    assert_eq!(pdf_read::read(&pdf).unwrap().pages.len(), 1, "{base}");
    for (ext, bytes) in [
        ("svg", svg.into_bytes()),
        ("dxf", dxf.into_bytes()),
        ("pdf", pdf),
    ] {
        let path = dir.join(format!("{base}.{ext}"));
        std::fs::write(&path, bytes).unwrap();
        written.push(path);
    }
}

fn write_all(dir: &Path) -> Vec<PathBuf> {
    std::fs::create_dir_all(dir).unwrap();
    let mut written = Vec::new();
    for s in samples() {
        let b = build(&s);
        println!("{}: Ebene verschoben je Schnitt: {:?}", s.name, b.nudged);
        put(
            dir,
            &format!("Schnitt_{}_ohneRahmen", s.name),
            &b.bare,
            &mut written,
        );
        put(
            dir,
            &format!("Schnitt_{}_Rahmen", s.name),
            &b.framed,
            &mut written,
        );
    }
    std::fs::write(
        dir.join("ERWARTUNG.txt"),
        "Proben der Schnittansichten (SVG, DXF, PDF je ohne und mit Rahmen).\n\
         Zu prüfen: Schraffur dünn, 45 Grad (zweiter Körper 135 Grad), Löcher und Spalte bleiben\n\
         frei; Kontur der Schnittfläche dick; Schnittlinie Strich-Punkt mit dicken Enden, Pfeile\n\
         in Blickrichtung, Buchstaben an den Pfeilen; Titel \"A–A (1:2)\" unter der Schnittansicht;\n\
         Quader_A3quer: Maße um Schnitt A; Stufe_A4quer Schnitt A liegt auf der Stufenfläche.\n",
    )
    .unwrap();
    written
}

fn samples_dir() -> PathBuf {
    match std::env::var_os("PLANORA_SECTION_SAMPLES_DIR") {
        Some(d) if !d.is_empty() => PathBuf::from(d),
        _ => PathBuf::from(r"D:\Testumgebung\zeichnung-proben\schnitt"),
    }
}

#[test]
#[ignore = "schreibt Dateien für den Handtest: --release -- --ignored"]
fn write_section_samples_for_the_hand_test() {
    let dir = samples_dir();
    let written = write_all(&dir);
    // 6 Proben × (ohne, mit Rahmen) × (SVG, DXF, PDF)
    assert_eq!(written.len(), 6 * 2 * 3);
    println!("Proben liegen in {}", dir.display());
}

/// Damit der Aufbau nicht verrottet: eine Probe ohne Dateien.
#[test]
fn section_sample_builder_still_works() {
    let s = samples().into_iter().next().unwrap();
    let b = build(&s);
    assert!(b.framed.frame.is_some() && b.bare.frame.is_none());
    assert_eq!(b.nudged, vec![false]);
}
