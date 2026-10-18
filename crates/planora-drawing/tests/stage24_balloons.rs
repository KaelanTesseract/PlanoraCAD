//! Stufe 24 (Teilprojekt 3c-2): Positionsnummern (Nummernkreise) im Modell, als Markierungen
//! der Ansicht und in SVG/DXF. Sollwerte von Hand gerechnet (Rechenweg im Kommentar); gelesen
//! wird mit den eigenen Lesern in `common/`.

mod common;

use common::dxf_read::{self, Dxf, Geom, ReadEntity};
use common::mark_check::v;
use common::model::box_solid;
use common::svg_marks::{box_sheet, manual_sheet};
use common::svg_read;
use planora_drawing::annotate::{Mark, MarkKind, ResolvedDims, TextAnchor};
use planora_drawing::balloon::{
    BALLOON_D, BALLOON_DOT_R, BALLOON_TEXT_H, BalloonMarks, DEFAULT_BALLOON_OFFSET, PositionPen,
    add_balloon_marks, anchor_of, anchor_point, balloon_marks, layout_balloon,
};
use planora_drawing::model::{Balloon, BalloonId, DrawingId};
use planora_drawing::sheet::{SheetModel, SheetTransform, build_sheet_with};
use planora_drawing::sheet_frame::sheet_scale;
use planora_drawing::sheet_marks::sheet_text_width;
use planora_drawing::{
    BodyKey, DrawBody, Drawing, Paper, Placement, Projection, Scale, Sheet, ViewId, ViewKind,
    ViewLines, compute_view, dxf_string, svg_string,
};
use planora_exchange::Progress;
use planora_math::{DVec2, TAU};
use std::collections::BTreeMap;

const TOL: f64 = 1e-9;
/// Faktor des A4-Blatts (297/420 ≈ 0,7071).
const F4: f64 = 297.0 / 420.0;
/// 1/√2.
const H2: f64 = std::f64::consts::FRAC_1_SQRT_2;

fn close(a: DVec2, b: DVec2, tol: f64) -> bool {
    (a.x - b.x).abs() <= tol && (a.y - b.y).abs() <= tol
}

/// Volle Kreise `(Mitte, Radius, Art)`.
fn circles(marks: &[Mark]) -> Vec<(DVec2, f64, MarkKind)> {
    marks
        .iter()
        .filter_map(|m| match m {
            Mark::Arc {
                center,
                r,
                start,
                end,
                kind,
            } if (end - start - TAU).abs() < 1e-12 => Some((*center, *r, *kind)),
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

fn texts(marks: &[Mark]) -> Vec<(DVec2, f64, f64, TextAnchor, String, MarkKind)> {
    marks
        .iter()
        .filter_map(|m| match m {
            Mark::Text {
                pos,
                height,
                angle,
                anchor,
                text,
                kind,
            } => Some((*pos, *height, *angle, *anchor, text.clone(), *kind)),
            _ => None,
        })
        .collect()
}

fn kind_of(m: &Mark) -> MarkKind {
    match m {
        Mark::Line { kind, .. }
        | Mark::Arrow { kind, .. }
        | Mark::Arc { kind, .. }
        | Mark::Text { kind, .. } => *kind,
    }
}

const CIRCLE: MarkKind = MarkKind::Position(PositionPen::Circle);
const LEADER: MarkKind = MarkKind::Position(PositionPen::Leader);

fn key(f: u64) -> BodyKey {
    BodyKey {
        feature: f,
        index: 0,
    }
}

fn balloon(view: ViewId, anchor: (f64, f64), offset: (f64, f64)) -> Balloon {
    Balloon {
        id: BalloonId(0),
        view,
        body: key(1),
        anchor,
        offset,
    }
}

fn a3() -> Sheet {
    Sheet {
        paper: Paper::A3,
        landscape: true,
    }
}

/// Zeichnung mit einer freien Ansicht „Vorne“ eines Quaders 40 × 30 × 20 bei (200, 150).
struct BoxCase {
    drawing: Drawing,
    view: ViewId,
    lines: ViewLines,
}

impl BoxCase {
    fn new(sheet: Sheet, scale: Scale) -> BoxCase {
        let solid = box_solid(40.0, 30.0, 20.0);
        let bodies = [DrawBody {
            name: "Q".into(),
            solid: &solid,
        }];
        let lines = compute_view(&bodies, ViewKind::Front, &Progress::new()).unwrap();
        let mut drawing = Drawing::new(DrawingId(1), "Z".into(), sheet, Projection::First);
        let view = drawing.add_view(
            ViewKind::Front,
            scale,
            Placement::Free { x: 200.0, y: 150.0 },
            None,
        );
        // Ohne Mittellinien: die Markierungen der Ansicht sind nur die der Kreise.
        drawing.views[0].centerlines = false;
        BoxCase {
            drawing,
            view,
            lines,
        }
    }

    fn results(&self) -> BTreeMap<ViewId, &ViewLines> {
        BTreeMap::from([(self.view, &self.lines)])
    }

    /// Halbe Breite und Höhe der Ansicht in Ansichtskoordinaten.
    fn half(&self) -> DVec2 {
        let ((x0, y0), (x1, y1)) = self.lines.bbox;
        v(0.5 * (x1 - x0), 0.5 * (y1 - y0))
    }

    fn sheet(&self, numbers: &BTreeMap<BalloonId, Option<u32>>) -> SheetModel {
        let res = self.results();
        let mut m = build_sheet_with(&self.drawing, &res, &ResolvedDims::default());
        add_balloon_marks(&mut m, &balloon_marks(&self.drawing, &res, numbers));
        m
    }
}

// ---------------------------------------------------------------------------------------------
// (a) Geometrie auf A3
// ---------------------------------------------------------------------------------------------

#[test]
fn balloon_geometry_a3() {
    assert_eq!((BALLOON_D, BALLOON_TEXT_H, BALLOON_DOT_R), (8.0, 3.5, 0.5));
    assert_eq!(DEFAULT_BALLOON_OFFSET, (10.0, 10.0));
    let marks = layout_balloon(v(100.0, 100.0), (10.0, 10.0), Some(3), 1.0);
    // Kreis: Mitte = Anker + Versatz = (110, 110), r = 8/2 = 4.
    let c = circles(&marks);
    assert_eq!(c.len(), 2, "{marks:?}");
    assert!(
        c.iter().any(|(m, r, k)| close(*m, v(110.0, 110.0), TOL)
            && (r - 4.0).abs() < TOL
            && *k == CIRCLE)
    );
    // Punkt am Teil: r 0,5 um den Anker, dünne Linie.
    assert!(
        c.iter().any(|(m, r, k)| close(*m, v(100.0, 100.0), TOL)
            && (r - 0.5).abs() < TOL
            && *k == LEADER)
    );
    // Hinweislinie vom Anker bis zum Kreisrand: Mitte − 4·(1, 1)/√2.
    let l = lines(&marks);
    assert_eq!(l.len(), 1);
    let edge = v(110.0 - 4.0 * H2, 110.0 - 4.0 * H2);
    let (a, b, k) = l[0];
    assert_eq!(k, LEADER);
    assert!(
        (close(a, v(100.0, 100.0), TOL) && close(b, edge, TOL))
            || (close(b, v(100.0, 100.0), TOL) && close(a, edge, TOL)),
        "{a:?} {b:?}"
    );
    // Zahl: mittig, Grundlinie = Mitte.y − 3,5/2 = 108,25, waagerecht.
    let t = texts(&marks);
    assert_eq!(t.len(), 1);
    let (pos, h, angle, anchor, text, _) = &t[0];
    assert!(close(*pos, v(110.0, 108.25), TOL), "{pos:?}");
    assert!((h - 3.5).abs() < TOL);
    assert_eq!(
        (*angle, *anchor, text.as_str()),
        (0.0, TextAnchor::Middle, "3")
    );
    // Nur diese vier Markierungen, alle in der Art „Position“.
    assert_eq!(marks.len(), 4);
    assert!(
        marks
            .iter()
            .all(|m| matches!(kind_of(m), MarkKind::Position(_)))
    );
    // Die Linie endet auf dem Rand, nicht in der Mitte: Abstand zur Mitte genau der Radius.
    let end = if close(a, v(100.0, 100.0), TOL) { b } else { a };
    assert!((end.distance(v(110.0, 110.0)) - 4.0).abs() < TOL);
    // Andere Richtung (links unten): Linie endet wieder auf dem Rand, auf der Seite des Ankers.
    let marks = layout_balloon(v(50.0, 50.0), (-30.0, 0.0), Some(1), 1.0);
    let l = lines(&marks);
    assert_eq!(l.len(), 1);
    let ends = [l[0].0, l[0].1];
    assert!(ends.iter().any(|p| close(*p, v(50.0, 50.0), TOL)));
    assert!(
        ends.iter().any(|p| close(*p, v(24.0, 50.0), TOL)),
        "{ends:?}"
    );
}

// ---------------------------------------------------------------------------------------------
// (b) Skalierung mit dem Papier
// ---------------------------------------------------------------------------------------------

#[test]
fn scales_with_paper() {
    let f0 = sheet_scale(Sheet {
        paper: Paper::A0,
        landscape: false,
    });
    assert!((f0 - 2.8310).abs() < 1e-3);
    let marks = layout_balloon(v(300.0, 300.0), (30.0, 40.0), Some(7), f0);
    let c = circles(&marks);
    let big = c.iter().find(|(_, _, k)| *k == CIRCLE).unwrap();
    assert!((big.1 - 4.0 * 2.8310).abs() < 1e-3, "{}", big.1);
    assert!(close(big.0, v(330.0, 340.0), TOL));
    let dot = c.iter().find(|(_, _, k)| *k == LEADER).unwrap();
    assert!((dot.1 - 0.5 * f0).abs() < TOL);
    let t = texts(&marks);
    assert!((t[0].1 - 3.5 * f0).abs() < TOL);
    assert!(close(t[0].0, v(330.0, 340.0 - 1.75 * f0), TOL));
    // Linie bis zum Rand: Länge 50 − 4·f.
    let l = lines(&marks);
    assert!((l[0].0.distance(l[0].1) - (50.0 - 4.0 * f0)).abs() < TOL);

    // Strichbreiten: A3-Werte mal Faktor, nie unter 0,18.
    assert_eq!(PositionPen::Circle.width_mm(), 0.35);
    assert_eq!(PositionPen::Leader.width_mm(), 0.25);
    assert!((PositionPen::Circle.width_at(F4) - 0.2475).abs() < 1e-4);
    assert!((PositionPen::Leader.width_at(F4) - 0.18).abs() < 1e-12);
    assert!((PositionPen::Circle.width_at(1.0) - 0.35).abs() < 1e-12);
    assert!((PositionPen::Leader.width_at(2.0) - 0.5).abs() < 1e-12);
    assert!((PositionPen::Circle.width_at(0.1) - 0.18).abs() < 1e-12);

    // Dreistellige Zahl passt in 0,8 · Ø (A3 und A4).
    for f in [1.0, F4] {
        let marks = layout_balloon(v(0.0, 0.0), (20.0, 0.0), Some(200), f);
        let t = texts(&marks);
        assert_eq!(t[0].4, "200");
        assert!(t[0].1 < 3.5 * f, "verkleinert");
        assert!(sheet_text_width("200", t[0].1) <= 0.8 * 8.0 * f + 1e-9);
        // Grundlinie folgt der eingepassten Höhe: Text bleibt in der Kreismitte.
        assert!(close(t[0].0, v(20.0, -t[0].1 / 2.0), TOL));
    }
    // Eine kurze Zahl bleibt in voller Höhe.
    let t = texts(&layout_balloon(v(0.0, 0.0), (20.0, 0.0), Some(12), 1.0));
    assert!((t[0].1 - 3.5).abs() < TOL);
}

// ---------------------------------------------------------------------------------------------
// (c) Anker im Kreis: keine Hinweislinie
// ---------------------------------------------------------------------------------------------

#[test]
fn short_offset_has_no_leader() {
    let marks = layout_balloon(v(100.0, 100.0), (1.0, 1.0), Some(2), 1.0);
    assert!(lines(&marks).is_empty());
    let c = circles(&marks);
    assert_eq!(c.len(), 2);
    assert!(
        c.iter()
            .any(|(m, _, k)| close(*m, v(101.0, 101.0), TOL) && *k == CIRCLE)
    );
    assert!(
        c.iter()
            .any(|(m, _, k)| close(*m, v(100.0, 100.0), TOL) && *k == LEADER)
    );
    // Grenze: Versatz genau ein Radius → keine Linie; knapp darüber → Linie.
    assert!(lines(&layout_balloon(v(0.0, 0.0), (4.0, 0.0), Some(1), 1.0)).is_empty());
    assert_eq!(
        lines(&layout_balloon(v(0.0, 0.0), (4.01, 0.0), Some(1), 1.0)).len(),
        1
    );
    // Auf A4 ist der Kreis kleiner: 3,5 mm Versatz liegt dort außerhalb (r = 2,83).
    assert_eq!(
        lines(&layout_balloon(v(0.0, 0.0), (3.5, 0.0), Some(1), F4)).len(),
        1
    );
}

// ---------------------------------------------------------------------------------------------
// (d) Verlorener Kreis
// ---------------------------------------------------------------------------------------------

#[test]
fn lost_balloon() {
    let ok = layout_balloon(v(100.0, 100.0), (10.0, 10.0), Some(3), 1.0);
    let lost = layout_balloon(v(100.0, 100.0), (10.0, 10.0), None, 1.0);
    assert_eq!(lost.len(), ok.len());
    assert!(lost.iter().all(|m| kind_of(m) == MarkKind::Lost));
    let t = texts(&lost);
    assert_eq!(t.len(), 1);
    assert_eq!(t[0].4, "?");
    // Dieselbe Geometrie.
    assert_eq!(
        circles(&lost)
            .iter()
            .map(|c| (c.0, c.1))
            .collect::<Vec<_>>(),
        circles(&ok).iter().map(|c| (c.0, c.1)).collect::<Vec<_>>()
    );
    assert_eq!(
        lines(&lost).iter().map(|l| (l.0, l.1)).collect::<Vec<_>>(),
        lines(&ok).iter().map(|l| (l.0, l.1)).collect::<Vec<_>>()
    );
    // Über `balloon_marks`: fehlt die Nummer in der Tabelle, ist der Kreis verloren.
    let mut case = BoxCase::new(a3(), Scale { num: 1, den: 1 });
    let a = case
        .drawing
        .add_balloon(balloon(case.view, (0.5, 0.5), (10.0, 10.0)));
    let b = case
        .drawing
        .add_balloon(balloon(case.view, (0.0, 0.0), (-10.0, 10.0)));
    let res = case.results();
    let numbers = BTreeMap::from([(a, Some(4)), (b, None)]);
    let list = balloon_marks(&case.drawing, &res, &numbers);
    assert_eq!(list.len(), 2);
    let ma = list.iter().find(|m| m.id == a).unwrap();
    let mb = list.iter().find(|m| m.id == b).unwrap();
    assert!(ma.marks.iter().all(|m| kind_of(m) != MarkKind::Lost));
    assert_eq!(texts(&ma.marks)[0].4, "4");
    assert!(mb.marks.iter().all(|m| kind_of(m) == MarkKind::Lost));
    assert_eq!(texts(&mb.marks)[0].4, "?");
    // Gar nicht in der Tabelle: ebenfalls verloren.
    let list = balloon_marks(&case.drawing, &res, &BTreeMap::new());
    assert!(
        list.iter()
            .all(|bm| bm.marks.iter().all(|m| kind_of(m) == MarkKind::Lost))
    );
}

// ---------------------------------------------------------------------------------------------
// (e) Anker relativ zur Hüllbox
// ---------------------------------------------------------------------------------------------

#[test]
fn anchor_round_trip() {
    let bbox = ((-10.0, -5.0), (30.0, 15.0));
    let xf = SheetTransform {
        scale: 0.5,
        tx: 150.0,
        ty: 100.0,
        bbox_center: v(10.0, 5.0),
    };
    for uv in [(0.0, 0.0), (0.25, 0.8), (1.0, 1.0)] {
        let p = anchor_point(bbox, &xf, uv);
        let back = anchor_of(bbox, &xf, p);
        assert!(
            (back.0 - uv.0).abs() < 1e-12 && (back.1 - uv.1).abs() < 1e-12,
            "{uv:?} {back:?}"
        );
    }
    // Ecken von Hand: Hüllbox 40 × 20, halb so groß auf dem Blatt um (150, 100).
    assert!(close(
        anchor_point(bbox, &xf, (0.5, 0.5)),
        v(150.0, 100.0),
        TOL
    ));
    assert!(close(
        anchor_point(bbox, &xf, (0.0, 0.0)),
        v(140.0, 95.0),
        TOL
    ));
    assert!(close(
        anchor_point(bbox, &xf, (1.0, 1.0)),
        v(160.0, 105.0),
        TOL
    ));
    assert!(close(
        anchor_point(bbox, &xf, (0.25, 0.8)),
        v(145.0, 103.0),
        TOL
    ));
    // Außerhalb: auf [0, 1] begrenzt, je Richtung für sich.
    assert_eq!(anchor_of(bbox, &xf, v(200.0, 100.0)), (1.0, 0.5));
    assert_eq!(anchor_of(bbox, &xf, v(150.0, 0.0)), (0.5, 0.0));
    assert_eq!(anchor_of(bbox, &xf, v(0.0, 500.0)), (0.0, 1.0));
    // Nicht endlich: nie NaN.
    let n = anchor_of(bbox, &xf, v(f64::NAN, 100.0));
    assert!(n.0.is_finite() && (0.0..=1.0).contains(&n.0));
    // Flache Hüllbox (Ansicht einer Linie): Mitte statt Division durch null.
    let flat = ((0.0, 2.0), (10.0, 2.0));
    let xf1 = SheetTransform {
        scale: 1.0,
        tx: 0.0,
        ty: 0.0,
        bbox_center: v(5.0, 2.0),
    };
    let a = anchor_of(flat, &xf1, v(2.5, 7.0));
    assert!((a.0 - 0.75).abs() < 1e-12 && a.1 == 0.5, "{a:?}");
}

// ---------------------------------------------------------------------------------------------
// (f) Der Kreis folgt dem Maßstab der Ansicht
// ---------------------------------------------------------------------------------------------

#[test]
fn follows_view_scale() {
    let mut case = BoxCase::new(a3(), Scale { num: 1, den: 1 });
    let id = case
        .drawing
        .add_balloon(balloon(case.view, (1.0, 1.0), (10.0, 10.0)));
    let numbers = BTreeMap::from([(id, Some(1))]);
    let half = case.half();
    assert!(half.x > 1.0 && half.y > 1.0);
    for (scale, s) in [
        (Scale { num: 1, den: 1 }, 1.0),
        (Scale { num: 1, den: 2 }, 0.5),
        (Scale { num: 2, den: 1 }, 2.0),
    ] {
        case.drawing.views[0].scale = scale;
        let list = balloon_marks(&case.drawing, &case.results(), &numbers);
        assert_eq!(list.len(), 1);
        // Ecke rechts oben der Ansicht: Mitte (200, 150) + halbe Ausdehnung mal Maßstab.
        let corner = v(200.0, 150.0) + half * s;
        let dot = circles(&list[0].marks)
            .into_iter()
            .find(|(_, _, k)| *k == LEADER)
            .unwrap();
        assert!(close(dot.0, corner, 1e-9), "{s}: {:?} {corner:?}", dot.0);
        // Versatz unverändert (Blatt-mm, nicht mit dem Maßstab der Ansicht).
        assert!(close(list[0].centre, corner + v(10.0, 10.0), 1e-9));
        assert!((list[0].radius - 4.0).abs() < TOL);
    }
    // Ansicht verschieben: der Kreis wandert mit.
    case.drawing.views[0].scale = Scale { num: 1, den: 1 };
    case.drawing.views[0].placement = Placement::Free { x: 100.0, y: 80.0 };
    let list = balloon_marks(&case.drawing, &case.results(), &numbers);
    assert!(close(
        list[0].centre,
        v(100.0, 80.0) + half + v(10.0, 10.0),
        1e-9
    ));
    // Linke untere Ecke.
    case.drawing.balloons[0].anchor = (0.0, 0.0);
    let list = balloon_marks(&case.drawing, &case.results(), &numbers);
    assert!(close(
        list[0].centre,
        v(100.0, 80.0) - half + v(10.0, 10.0),
        1e-9
    ));
    // Kreise auf A4 sind kleiner (Faktor des Blatts).
    case.drawing.sheet = Sheet {
        paper: Paper::A4,
        landscape: true,
    };
    let list = balloon_marks(&case.drawing, &case.results(), &numbers);
    assert!((list[0].radius - 4.0 * 297.0 / 420.0).abs() < 1e-12);
    // Mitte und Radius in `BalloonMarks` sind die des gezeichneten Kreises; der Versatz ist
    // Blatt-mm und wird nicht mit dem Faktor des Papiers multipliziert.
    let expected = v(100.0, 80.0) - half + v(10.0, 10.0);
    assert!(
        close(list[0].centre, expected, 1e-9),
        "{:?}",
        list[0].centre
    );
    let drawn: Vec<_> = circles(&list[0].marks)
        .into_iter()
        .filter(|(_, _, k)| *k == CIRCLE)
        .collect();
    assert_eq!(drawn.len(), 1);
    assert!(close(drawn[0].0, list[0].centre, 1e-12));
    assert!((drawn[0].1 - list[0].radius).abs() < 1e-12);
    // Ebenso auf A0 mit schraegem Versatz.
    case.drawing.sheet = Sheet {
        paper: Paper::A0,
        landscape: false,
    };
    case.drawing.balloons[0].offset = (-25.0, 7.5);
    let list = balloon_marks(&case.drawing, &case.results(), &numbers);
    let drawn: Vec<_> = circles(&list[0].marks)
        .into_iter()
        .filter(|(_, _, k)| *k == CIRCLE)
        .collect();
    assert!(close(
        list[0].centre,
        v(100.0, 80.0) - half + v(-25.0, 7.5),
        1e-9
    ));
    assert!(close(drawn[0].0, list[0].centre, 1e-12));
    assert!((drawn[0].1 - list[0].radius).abs() < 1e-12);
    assert!((list[0].radius - 4.0 * 1189.0 / 420.0).abs() < 1e-12);
}

// ---------------------------------------------------------------------------------------------
// (g) SVG und DXF
// ---------------------------------------------------------------------------------------------

fn layer<'a>(d: &'a Dxf, name: &str) -> Option<&'a dxf_read::Record> {
    d.table("LAYER")
        .unwrap()
        .records
        .iter()
        .find(|r| r.name() == name)
}

fn block_entities<'a>(d: &'a Dxf, name: &str) -> &'a [ReadEntity] {
    &d.block(name).unwrap().entities
}

#[test]
fn svg_and_dxf_output() {
    let mut case = BoxCase::new(a3(), Scale { num: 1, den: 1 });
    // Anker in der Mitte der Ansicht: (200, 150); Kreis bei (210, 160).
    let id = case
        .drawing
        .add_balloon(balloon(case.view, (0.5, 0.5), (10.0, 10.0)));
    let sheet = case.sheet(&BTreeMap::from([(id, Some(3))]));
    assert_eq!(sheet.views.len(), 1);

    // SVG: eigene Gruppe der Ansicht, Kreis 0,35, Linie und Punkt 0,25, Zahl „3“.
    let svg = svg_string(&sheet).unwrap();
    let r = svg_read::read(&svg);
    assert!(r.group_ids.iter().any(|g| g == "Vorne"));
    let group = "Vorne Positionen";
    assert!(r.group_ids.iter().any(|g| g == group), "{:?}", r.group_ids);
    let paths: Vec<_> = r.paths_of(group).collect();
    assert_eq!(paths.len(), 3, "{paths:?}");
    let circle = paths
        .iter()
        .find(|p| p.distance_to((214.0, 160.0)) < 1e-5 && p.distance_to((210.0, 164.0)) < 1e-5)
        .expect("Kreis");
    assert!((circle.width - 0.35).abs() < 1e-9);
    assert!(circle.kinds().contains('A'));
    let leader = paths
        .iter()
        .find(|p| p.kinds().contains('L'))
        .expect("Linie");
    assert!((leader.width - 0.25).abs() < 1e-9);
    assert!(leader.distance_to((200.0, 150.0)) < 1e-5);
    assert!(leader.distance_to((210.0 - 4.0 * H2, 160.0 - 4.0 * H2)) < 1e-5);
    let dot = paths
        .iter()
        .find(|p| p.distance_to((200.5, 150.0)) < 1e-5 && !p.kinds().contains('L'))
        .expect("Punkt");
    assert!((dot.width - 0.25).abs() < 1e-9);
    let t: Vec<_> = r.texts_of(group).collect();
    assert_eq!(t.len(), 1);
    assert_eq!(t[0].text, "3");
    assert_eq!(t[0].anchor, "middle");
    assert!((t[0].pos.0 - 210.0).abs() < 1e-6 && (t[0].pos.1 - 158.25).abs() < 1e-6);
    assert_eq!(t[0].fill, "#000");
    // Nichts davon in der Gruppe der Maße.
    assert!(!r.group_ids.iter().any(|g| g == "Vorne Maße"));

    // DXF: Ebenen POSITIONEN (35) und POSITIONEN_LINIE (25), Farbe 7, durchgezogen.
    let text = dxf_string(&sheet).unwrap();
    let d = dxf_read::read(&text).unwrap_or_else(|e| panic!("Aufbau: {e}"));
    let lp = layer(&d, "POSITIONEN").expect("Ebene POSITIONEN");
    assert_eq!(lp.get(370).map(str::trim), Some("35"));
    assert_eq!(lp.get(62).map(str::trim), Some("7"));
    assert_eq!(lp.get(6), Some("CONTINUOUS"));
    let ll = layer(&d, "POSITIONEN_LINIE").expect("Ebene POSITIONEN_LINIE");
    assert_eq!(ll.get(370).map(str::trim), Some("25"));
    assert_eq!(ll.get(62).map(str::trim), Some("7"));
    let ents = block_entities(&d, "Vorne");
    let on = |l: &'static str| {
        ents.iter()
            .filter(move |e| e.layer == l)
            .collect::<Vec<_>>()
    };
    let pos = on("POSITIONEN");
    let lin = on("POSITIONEN_LINIE");
    assert_eq!(pos.len(), 2, "Kreis und Text");
    assert_eq!(lin.len(), 2, "Linie und Punkt");
    let c = pos.iter().find(|e| e.kind == "CIRCLE").expect("CIRCLE");
    match c.geom().unwrap() {
        Geom::Circle { c, r } => {
            assert!(
                (c.0 - 210.0).abs() < 1e-9 && (c.1 - 160.0).abs() < 1e-9 && (r - 4.0).abs() < 1e-9
            )
        }
        g => panic!("{g:?}"),
    }
    let tx = pos.iter().find(|e| e.kind == "TEXT").expect("TEXT");
    assert_eq!(tx.get(1), Some("3"));
    let l = lin.iter().find(|e| e.kind == "LINE").expect("LINE");
    match l.geom().unwrap() {
        Geom::Line { a, b } => {
            let e = (210.0 - 4.0 * H2, 160.0 - 4.0 * H2);
            let near =
                |p: (f64, f64), q: (f64, f64)| (p.0 - q.0).abs() < 1e-8 && (p.1 - q.1).abs() < 1e-8;
            assert!(
                (near(a, (200.0, 150.0)) && near(b, e)) || (near(b, (200.0, 150.0)) && near(a, e))
            );
        }
        g => panic!("{g:?}"),
    }
    let p = lin.iter().find(|e| e.kind == "CIRCLE").expect("Punkt");
    match p.geom().unwrap() {
        Geom::Circle { r, .. } => assert!((r - 0.5).abs() < 1e-9),
        g => panic!("{g:?}"),
    }
    // Keine anderen Ebenen der Markierungen.
    assert!(layer(&d, "MASSE").is_none() && layer(&d, "MASSE_FEHLT").is_none());

    // Verlorener Kreis: rot in der Ebene der fehlenden Maße, nicht auf POSITIONEN.
    let lost = case.sheet(&BTreeMap::from([(id, None)]));
    let d = dxf_read::read(&dxf_string(&lost).unwrap()).unwrap();
    assert!(layer(&d, "POSITIONEN").is_none() && layer(&d, "POSITIONEN_LINIE").is_none());
    assert!(layer(&d, "MASSE_FEHLT").is_some());
    assert!(svg_string(&lost).unwrap().contains("#c00"));

    // Ohne Kreise: weder Ebenen noch Gruppe; die goldenen Dateien bleiben gleich.
    let none = case.sheet(&BTreeMap::new());
    let mut no_balloons = case.drawing.clone();
    no_balloons.balloons.clear();
    let res = case.results();
    let mut m = build_sheet_with(&no_balloons, &res, &ResolvedDims::default());
    add_balloon_marks(&mut m, &balloon_marks(&no_balloons, &res, &BTreeMap::new()));
    let d = dxf_read::read(&dxf_string(&m).unwrap()).unwrap();
    assert!(layer(&d, "POSITIONEN").is_none() && layer(&d, "POSITIONEN_LINIE").is_none());
    assert!(!svg_string(&m).unwrap().contains("Positionen"));
    assert!(
        none.views[0]
            .marks
            .iter()
            .all(|m| kind_of(m) == MarkKind::Lost)
    );
    let mut golden = box_sheet(false);
    add_balloon_marks(&mut golden, &[]);
    assert_eq!(
        svg_string(&golden).unwrap(),
        include_str!("fixtures/svg_without_marks_box.svg")
    );
    assert_eq!(
        dxf_string(&golden).unwrap().as_bytes(),
        include_bytes!("fixtures/dxf_without_marks_box.dxf")
    );
}

#[test]
fn line_weights_follow_the_paper() {
    // A4: Kreis 0,35 · 0,707 = 0,2475 (DXF 25), Linie 0,25 · 0,707 = 0,177 → 0,18 (DXF 18).
    let mut case = BoxCase::new(
        Sheet {
            paper: Paper::A4,
            landscape: true,
        },
        Scale { num: 1, den: 1 },
    );
    case.drawing.views[0].placement = Placement::Free { x: 120.0, y: 100.0 };
    let id = case
        .drawing
        .add_balloon(balloon(case.view, (0.5, 0.5), (10.0, 10.0)));
    let sheet = case.sheet(&BTreeMap::from([(id, Some(1))]));
    let r = svg_read::read(&svg_string(&sheet).unwrap());
    let widths: Vec<f64> = r.paths_of("Vorne Positionen").map(|p| p.width).collect();
    assert_eq!(widths.len(), 3);
    assert!(
        widths.iter().any(|w| (w - 0.2475).abs() < 1e-6),
        "{widths:?}"
    );
    assert_eq!(
        widths.iter().filter(|w| (*w - 0.18).abs() < 1e-9).count(),
        2
    );
    let d = dxf_read::read(&dxf_string(&sheet).unwrap()).unwrap();
    assert_eq!(
        layer(&d, "POSITIONEN").unwrap().get(370).map(str::trim),
        Some("25")
    );
    assert_eq!(
        layer(&d, "POSITIONEN_LINIE")
            .unwrap()
            .get(370)
            .map(str::trim),
        Some("18")
    );
}

/// Alle Papiere, hoch und quer.
fn all_sheets() -> Vec<Sheet> {
    let mut out = Vec::new();
    for paper in [Paper::A4, Paper::A3, Paper::A2, Paper::A1, Paper::A0] {
        for landscape in [false, true] {
            out.push(Sheet { paper, landscape });
        }
    }
    out
}

/// Blatt mit je einem Nummernkreis je Nummer (Mitte bei (80 + 30·i, 100)).
fn numbers_sheet(sheet: Sheet, numbers: &[u32]) -> SheetModel {
    let f = sheet_scale(sheet);
    let marks: Vec<Mark> = numbers
        .iter()
        .enumerate()
        .flat_map(|(i, n)| {
            layout_balloon(v(60.0 + 30.0 * i as f64, 100.0), (20.0, 0.0), Some(*n), f)
        })
        .collect();
    let mut m = manual_sheet(sheet.paper, vec![("Vorne", marks)]);
    m.sheet = sheet;
    m
}

// Die Zahl in DXF: CAD-Schriften laufen bis 1,45-mal breiter; die Zahl muss auch dann in
// 0,8 · Ø passen, ohne gekürzt zu werden, und bleibt senkrecht mittig.
#[test]
fn dxf_numbers_fit_the_circle() {
    let numbers = [7, 88, 200, 888, 111];
    for sheet in all_sheets() {
        let f = sheet_scale(sheet);
        let model = numbers_sheet(sheet, &numbers);
        let d = dxf_read::read(&dxf_string(&model).unwrap()).unwrap();
        let texts: Vec<_> = block_entities(&d, "Vorne")
            .iter()
            .filter(|e| e.kind == "TEXT")
            .collect();
        assert_eq!(texts.len(), numbers.len());
        for (i, (e, n)) in texts.iter().zip(numbers).enumerate() {
            let text = e.get(1).unwrap();
            assert_eq!(text, n.to_string(), "nie gekürzt");
            let h: f64 = e.get(40).unwrap().trim().parse().unwrap();
            let w = sheet_text_width(text, h) * 1.45;
            assert!(
                w <= 0.8 * 8.0 * f + 1e-8,
                "{sheet:?} {n}: {w} > {}",
                0.8 * 8.0 * f
            );
            assert!(h <= 3.5 * f + 1e-9);
            // Grundlinie: Kreismitte − h/2.
            let y: f64 = e.get(20).unwrap().trim().parse().unwrap();
            assert!((y + h / 2.0 - 100.0).abs() < 1e-8, "{sheet:?} {n}: {y} {h}");
            let x: f64 = e.get(10).unwrap().trim().parse().unwrap();
            assert!((x - (80.0 + 30.0 * i as f64)).abs() < 1e-8);
        }
        // Eine Ziffer passt ohne Verkleinerung (0,72 · 3,5 · 1,45 = 3,65 ≤ 6,4).
        let h0: f64 = texts[0].get(40).unwrap().trim().parse().unwrap();
        assert!((h0 - 3.5 * f).abs() < 1e-8);
    }
    // „200“ auf A4 und A0 von Hand: Höhe = 0,8 · 8 · f / (3 · 0,72 · 1,45).
    for (paper, f) in [(Paper::A4, 297.0 / 420.0), (Paper::A0, 1189.0 / 420.0)] {
        let sheet = Sheet {
            paper,
            landscape: true,
        };
        let model = numbers_sheet(sheet, &[200]);
        let d = dxf_read::read(&dxf_string(&model).unwrap()).unwrap();
        let t = block_entities(&d, "Vorne")
            .iter()
            .find(|e| e.kind == "TEXT")
            .unwrap();
        let h: f64 = t.get(40).unwrap().trim().parse().unwrap();
        let want = 6.4 * f / (3.0 * 0.72 * 1.45);
        assert!((h - want).abs() < 1e-8, "{paper:?} {h} {want}");
        // SVG behält die Schätzung ohne Zuschlag: 6,4 · f / (3 · 0,72), größer als in DXF.
        let r = svg_read::read(&svg_string(&model).unwrap());
        let s = r.texts_of("Vorne Positionen").next().unwrap();
        assert!(
            (s.font_size - 6.4 * f / (3.0 * 0.72)).abs() < 1e-5,
            "{}",
            s.font_size
        );
    }
}

// Die SVG-Ausgabe der Nummernkreise ist dieselbe wie vor der DXF-Einpassung (Fixtur mit dem
// Stand 1a97731 erzeugt).
#[test]
fn svg_numbers_unchanged() {
    let mut all = String::new();
    for sheet in [
        Sheet {
            paper: Paper::A4,
            landscape: true,
        },
        Sheet {
            paper: Paper::A0,
            landscape: false,
        },
    ] {
        all.push_str(&svg_string(&numbers_sheet(sheet, &[7, 88, 200])).unwrap());
    }
    assert_eq!(all, include_str!("fixtures/svg_balloons_a4_a0.svg"));
}

#[test]
fn svg_names_are_escaped() {
    let marks = layout_balloon(v(50.0, 50.0), (10.0, 10.0), Some(5), 1.0);
    let sheet = manual_sheet(Paper::A3, vec![("A<&\"'>", marks)]);
    let svg = svg_string(&sheet).unwrap();
    assert!(svg.contains("<g id=\"A&lt;&amp;&quot;&apos;&gt; Positionen\">"));
    assert!(!svg.contains("A<&"));
    let r = svg_read::read(&svg);
    assert!(
        r.group_ids.iter().any(|g| g == "A<&\"'> Positionen"),
        "{:?}",
        r.group_ids
    );
    assert_eq!(r.texts_of("A<&\"'> Positionen").count(), 1);
    // Zwei Ansichten mit gleichem Namen: eindeutige Gruppen.
    let m1 = layout_balloon(v(50.0, 50.0), (10.0, 10.0), Some(1), 1.0);
    let m2 = layout_balloon(v(80.0, 50.0), (10.0, 10.0), Some(2), 1.0);
    let sheet = manual_sheet(Paper::A3, vec![("Vorne", m1), ("Vorne", m2)]);
    let r = svg_read::read(&svg_string(&sheet).unwrap());
    let unique: std::collections::BTreeSet<_> = r.group_ids.iter().collect();
    assert_eq!(unique.len(), r.group_ids.len(), "{:?}", r.group_ids);
}

// ---------------------------------------------------------------------------------------------
// (h) Ansicht löschen löscht ihre Kreise
// ---------------------------------------------------------------------------------------------

#[test]
fn remove_view_drops_balloons() {
    let mut d = Drawing::new(DrawingId(1), "Z".into(), a3(), Projection::First);
    assert!(d.balloons.is_empty());
    assert_eq!(d.next_balloon, 1);
    let one = Scale { num: 1, den: 1 };
    let f = d.add_view(
        ViewKind::Front,
        one,
        Placement::Free { x: 100.0, y: 100.0 },
        None,
    );
    let t = d.add_view(
        ViewKind::Top,
        one,
        Placement::Aligned { distance: 60.0 },
        Some(f),
    );
    let i = d.add_view(
        ViewKind::Isometric,
        one,
        Placement::Free { x: 300.0, y: 100.0 },
        None,
    );
    let bf = d.add_balloon(balloon(f, (0.5, 0.5), (10.0, 10.0)));
    let bt = d.add_balloon(balloon(t, (0.5, 0.5), (10.0, 10.0)));
    let bi = d.add_balloon(balloon(i, (0.5, 0.5), (10.0, 10.0)));
    assert_eq!((bf, bt, bi), (BalloonId(1), BalloonId(2), BalloonId(3)));
    assert_eq!(d.next_balloon, 4);
    assert_eq!(d.balloon(bt).unwrap().view, t);
    assert_eq!(d.balloon(bt).unwrap().id, bt);
    assert!(d.balloon(BalloonId(9)).is_none());
    // Vorne samt abgeleitetem „Oben“ entfernen: deren Kreise sind weg, der isometrische bleibt.
    d.remove_view(f);
    assert_eq!(
        d.balloons.iter().map(|b| b.id).collect::<Vec<_>>(),
        vec![bi]
    );
    assert_eq!(d.next_balloon, 4, "Kennungen werden nicht wieder vergeben");
    // Unbekannte Ansicht entfernen: nichts passiert.
    d.remove_view(ViewId(77));
    assert_eq!(d.balloons.len(), 1);
    // Einzeln löschen.
    assert!(!d.remove_balloon(bf));
    assert!(d.remove_balloon(bi));
    assert!(d.balloons.is_empty());
    assert_eq!(
        d.add_balloon(balloon(i, (0.0, 0.0), (1.0, 1.0))),
        BalloonId(4)
    );
}

// ---------------------------------------------------------------------------------------------
// (i) Nicht endliche Zahlen
// ---------------------------------------------------------------------------------------------

#[test]
fn non_finite_is_dropped() {
    let nan = f64::NAN;
    assert!(layout_balloon(v(0.0, 0.0), (nan, 1.0), Some(1), 1.0).is_empty());
    assert!(layout_balloon(v(0.0, 0.0), (1.0, f64::INFINITY), Some(1), 1.0).is_empty());
    assert!(layout_balloon(v(nan, 0.0), (10.0, 10.0), Some(1), 1.0).is_empty());
    assert!(layout_balloon(v(0.0, 0.0), (10.0, 10.0), Some(1), nan).is_empty());
    assert!(layout_balloon(v(0.0, 0.0), (10.0, 10.0), None, 0.0).is_empty());
    let mut case = BoxCase::new(a3(), Scale { num: 1, den: 1 });
    let bad = case
        .drawing
        .add_balloon(balloon(case.view, (0.5, 0.5), (nan, 10.0)));
    let good = case
        .drawing
        .add_balloon(balloon(case.view, (0.5, 0.5), (10.0, 10.0)));
    let numbers = BTreeMap::from([(bad, Some(1)), (good, Some(2))]);
    let list: Vec<BalloonMarks> = balloon_marks(&case.drawing, &case.results(), &numbers);
    assert_eq!(list.iter().map(|b| b.id).collect::<Vec<_>>(), vec![good]);
    let sheet = case.sheet(&numbers);
    assert!(svg_string(&sheet).is_ok());
    assert!(dxf_string(&sheet).is_ok());
    // Kreise einer Ansicht, die nicht gezeichnet wird (kein Ergebnis), entfallen.
    let other = case.drawing.add_view(
        ViewKind::Top,
        Scale { num: 1, den: 1 },
        Placement::Free { x: 50.0, y: 50.0 },
        None,
    );
    let lone = case
        .drawing
        .add_balloon(balloon(other, (0.5, 0.5), (10.0, 10.0)));
    let list = balloon_marks(&case.drawing, &case.results(), &numbers);
    assert!(list.iter().all(|b| b.id != lone));
    // Markierungen einer Ansicht, die im Blattmodell fehlt, gehen nirgends hin.
    let mut m = build_sheet_with(&case.drawing, &case.results(), &ResolvedDims::default());
    let before: usize = m.views.iter().map(|v| v.marks.len()).sum();
    add_balloon_marks(
        &mut m,
        &[BalloonMarks {
            view: other,
            id: lone,
            centre: v(0.0, 0.0),
            radius: 4.0,
            marks: layout_balloon(v(0.0, 0.0), (10.0, 10.0), Some(1), 1.0),
        }],
    );
    assert_eq!(m.views.iter().map(|v| v.marks.len()).sum::<usize>(), before);
}
