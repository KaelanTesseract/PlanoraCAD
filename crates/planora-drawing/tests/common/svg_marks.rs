//! Prüfwerkzeuge für die SVG-Ausgabe der Markierungen (Stufe 16): Blätter für die goldenen
//! Dateien, von Hand gebaute Blattmodelle mit Markierungen und Auszüge aus dem eigenen Leser.
//! Unabhängig von `svg.rs`.

use super::model::{box_with_hole, cylinder_solid};
use super::svg_read::{P, ReadSvg, Seg};
use planora_drawing::annotate::{Mark, ResolvedDims};
use planora_drawing::model::{Drawing, DrawingId, Paper, Placement, Projection, Scale, Sheet};
use planora_drawing::sheet::{SheetModel, SheetView, build_sheet_with};
use planora_drawing::{DrawBody, ViewId, ViewKind, ViewLines, compute_view};
use planora_exchange::Progress;
use planora_math::DVec3;
use std::collections::BTreeMap;

/// Nummer bei A3 quer.
pub fn drawing(paper: Paper) -> Drawing {
    Drawing::new(
        DrawingId(1),
        "Z".into(),
        Sheet {
            paper,
            landscape: true,
        },
        Projection::First,
    )
}

const ONE: Scale = Scale { num: 1, den: 1 };

/// Blatt 1 der goldenen Dateien: Quader mit Bohrung, drei Ansichten (Vorne frei, Oben und Rechts
/// abgeleitet). Dieselbe Zusammenstellung hat die Fixtur `svg_without_marks_box.svg` erzeugt.
pub fn box_sheet(centerlines: bool) -> SheetModel {
    let solid = box_with_hole(40.0, 30.0, 20.0, 5.0, DVec3::Y);
    let bodies = [DrawBody {
        name: "Q".into(),
        solid: &solid,
    }];
    let mut d = drawing(Paper::A3);
    let f = d.add_view(
        ViewKind::Front,
        ONE,
        Placement::Free { x: 120.0, y: 200.0 },
        None,
    );
    let t = d.add_view(
        ViewKind::Top,
        ONE,
        Placement::Aligned { distance: 60.0 },
        Some(f),
    );
    let r = d.add_view(
        ViewKind::Right,
        ONE,
        Placement::Aligned { distance: 70.0 },
        Some(f),
    );
    for v in &mut d.views {
        v.centerlines = centerlines;
    }
    let p = Progress::new();
    let vf = compute_view(&bodies, ViewKind::Front, &p).unwrap();
    let vt = compute_view(&bodies, ViewKind::Top, &p).unwrap();
    let vr = compute_view(&bodies, ViewKind::Right, &p).unwrap();
    let res: BTreeMap<ViewId, &ViewLines> = BTreeMap::from([(f, &vf), (t, &vt), (r, &vr)]);
    build_sheet_with(&d, &res, &ResolvedDims::default())
}

/// Blatt 2 der goldenen Dateien: Zylinder, Vorne und Isometrie (Maßstab 2:1).
pub fn cylinder_sheet(centerlines: bool) -> SheetModel {
    let solid = cylinder_solid(8.0, 12.0);
    let bodies = [DrawBody {
        name: "Z".into(),
        solid: &solid,
    }];
    let mut d = drawing(Paper::A3);
    let f = d.add_view(
        ViewKind::Front,
        ONE,
        Placement::Free { x: 100.0, y: 200.0 },
        None,
    );
    let i = d.add_view(
        ViewKind::Isometric,
        Scale { num: 2, den: 1 },
        Placement::Free { x: 300.0, y: 150.0 },
        None,
    );
    for v in &mut d.views {
        v.centerlines = centerlines;
    }
    let p = Progress::new();
    let vf = compute_view(&bodies, ViewKind::Front, &p).unwrap();
    let vi = compute_view(&bodies, ViewKind::Isometric, &p).unwrap();
    let res: BTreeMap<ViewId, &ViewLines> = BTreeMap::from([(f, &vf), (i, &vi)]);
    build_sheet_with(&d, &res, &ResolvedDims::default())
}

/// Blattmodell von Hand: je Ansicht (Name, Markierungen), ohne Linien.
pub fn manual_sheet(paper: Paper, views: Vec<(&str, Vec<Mark>)>) -> SheetModel {
    let sheet = Sheet {
        paper,
        landscape: true,
    };
    SheetModel {
        sheet,
        frame: None,
        views: views
            .into_iter()
            .enumerate()
            .map(|(i, (name, marks))| SheetView {
                id: ViewId(i as u64 + 1),
                name: name.to_string(),
                lines: Vec::new(),
                extent_mm: (0.0, 0.0),
                marks,
            })
            .collect(),
    }
}

pub fn near(a: P, b: P, tol: f64) -> bool {
    (a.0 - b.0).abs() <= tol && (a.1 - b.1).abs() <= tol
}

/// Alle geraden Strecken (`M … L …`) ungefüllter Pfade der Gruppe, als Endpunktpaare.
pub fn lines_in(svg: &ReadSvg, group: &str) -> Vec<(P, P)> {
    let mut out = Vec::new();
    for p in svg.paths_of(group) {
        if p.fill != "none" {
            continue;
        }
        for s in &p.segments {
            if let Seg::Line { a, b } = s {
                out.push((*a, *b));
            }
        }
    }
    out
}

/// Die Ecken aller gefüllten Pfade der Gruppe (Dreiecke: Spitze, Ecke, Ecke).
pub fn triangles_in(svg: &ReadSvg, group: &str) -> Vec<[P; 3]> {
    let mut out = Vec::new();
    for p in svg.paths_of(group) {
        if p.fill == "none" || p.fill.is_empty() {
            continue;
        }
        let pts: Vec<P> = p.segments.iter().map(Seg::start).collect();
        assert_eq!(pts.len(), 3, "Dreieck erwartet: {p:?}");
        // Das `Z` schließt mit der dritten Seite zum Anfang.
        assert!(near(p.segments[2].end(), pts[0], 1e-9), "nicht geschlossen");
        out.push([pts[0], pts[1], pts[2]]);
    }
    out
}

pub fn has_seg(list: &[(P, P)], a: P, b: P, tol: f64) -> bool {
    list.iter()
        .any(|&(x, y)| (near(x, a, tol) && near(y, b, tol)) || (near(x, b, tol) && near(y, a, tol)))
}
