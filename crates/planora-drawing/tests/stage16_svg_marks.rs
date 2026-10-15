//! Stufe 16 (Teilprojekt 12): SVG-Ausgabe von Maßen und Mittellinien. Gelesen wird mit dem
//! eigenen Leser (`common/svg_read.rs`); die Sollwerte sind von Hand aus den Eingabepunkten
//! gerechnet (Rechenweg im Kommentar). Die goldenen Dateien in `fixtures/` hat der Schreiber vor
//! der Änderung erzeugt.

mod common;

use common::mark_check::v;
use common::svg_marks::*;
use common::svg_read::{self, P, Seg};
use planora_drawing::annotate::{Mark, MarkKind, ResolvedDims, TextAnchor, layout_dimension};
use planora_drawing::centerline::{CenterLine, CenterSource};
use planora_drawing::dimension::{Anchor, DimError, DimId, DimKind, DimSpec, Measured, Tolerance};
use planora_drawing::model::{Paper, Placement, Scale};
use planora_drawing::sheet::{SheetModel, SheetTransform, build_sheet_with};
use planora_drawing::stroke::{Stroke, StrokeOrigin};
use planora_drawing::{
    Basis, DrawError, Piece, ViewId, ViewKind, ViewLines, Visibility, svg_string, write_svg,
};
use planora_kernel::Curve;
use planora_math::{DVec2, DVec3, PI};
use std::collections::BTreeMap;

const TOL: f64 = 1e-6;
/// √2/2.
const H2: f64 = std::f64::consts::FRAC_1_SQRT_2;

fn id_xf() -> SheetTransform {
    SheetTransform {
        scale: 1.0,
        tx: 0.0,
        ty: 0.0,
        bbox_center: DVec2::ZERO,
    }
}

fn spec(kind: DimKind, offset: f64) -> DimSpec {
    DimSpec {
        id: DimId(1),
        view: ViewId(1),
        kind,
        refs: Vec::new(),
        offset,
        angle: 45.0,
        text_shift: (0.0, 0.0),
        tolerance: Tolerance::None,
        decimals: 1,
        text_override: None,
        chain: None,
        suggested: false,
    }
}

fn lin(a: DVec2, b: DVec2, dir: DVec2) -> Anchor {
    Anchor::Linear {
        a,
        b,
        dir,
        a_span: (a, a),
        b_span: (b, b),
    }
}

fn meas(value: f64, anchor: Anchor) -> Measured {
    Measured {
        value,
        anchor,
        prefix: None,
    }
}

fn svg_of(sheet: &SheetModel) -> String {
    svg_string(sheet).expect("SVG")
}

fn text_mark(pos: DVec2, angle: f64, anchor: TextAnchor, text: &str, kind: MarkKind) -> Mark {
    Mark::Text {
        pos,
        height: 3.5,
        angle,
        anchor,
        text: text.to_string(),
        kind,
    }
}

fn line_mark(kind: MarkKind) -> Mark {
    Mark::Line {
        a: v(0.0, 0.0),
        b: v(1.0, 1.0),
        kind,
    }
}

// ---------------------------------------------------------------------------------------------
// (a) Alte Blätter bleiben Byte für Byte gleich

// Die Fixturen hat der Schreiber vor der Änderung erzeugt (Quader mit Bohrung mit Vorne/Oben/
// Rechts, Zylinder mit Vorne/Isometrie). Ohne Maße und mit ausgeschalteten Mittellinien müssen
// beide Blätter genau dieselben Bytes ergeben (kein zusätzlicher Zeilenumbruch, keine leere
// Gruppe).
#[test]
fn old_sheets_stay_byte_for_byte() {
    let golden_box = include_str!("fixtures/svg_without_marks_box.svg");
    let golden_cyl = include_str!("fixtures/svg_without_marks_cylinder.svg");
    assert!(!golden_box.contains('\r') && !golden_cyl.contains('\r'));
    let b = box_sheet(false);
    let c = cylinder_sheet(false);
    assert!(b.views.iter().all(|v| v.marks.is_empty()));
    assert!(c.views.iter().all(|v| v.marks.is_empty()));
    assert_eq!(svg_string(&b).unwrap(), golden_box);
    assert_eq!(svg_string(&c).unwrap(), golden_cyl);
    let mut out = Vec::new();
    write_svg(&b, &mut out).unwrap();
    assert_eq!(out, golden_box.as_bytes());
    // Die Fixturen zeigen den Umfang des Altbestands: verdeckte Linien, mehrere Ansichten.
    assert!(golden_box.contains("stroke-dasharray=\"3 1\""));
    assert_eq!(golden_box.matches("<g id=").count(), 3);
    assert_eq!(golden_cyl.matches("<g id=").count(), 2);
    assert!(!golden_box.contains("Mittellinien") && !golden_cyl.contains("Maße"));
}

// Nur die Ansicht mit Marken bekommt Gruppen; alle anderen Zeilen bleiben die der Fixtur.
#[test]
fn only_marked_views_add_lines() {
    let golden = include_str!("fixtures/svg_without_marks_box.svg");
    let mut sheet = box_sheet(false);
    sheet.views[1].marks.push(Mark::Line {
        a: v(1.0, 2.0),
        b: v(3.0, 4.0),
        kind: MarkKind::Dimension,
    });
    let text = svg_of(&sheet);
    let added = [
        "<g id=\"Oben Maße\">",
        "<path d=\"M 1 2 L 3 4\" stroke=\"#000\" stroke-width=\"0.25\" fill=\"none\" stroke-linecap=\"round\"/>",
        "</g>",
    ];
    // Die neuen Zeilen stehen hinter der Liniengruppe „Oben“ (zweite `</g>`-Zeile von innen).
    let mut expected: Vec<&str> = golden.lines().collect();
    let at = expected
        .iter()
        .position(|l| *l == "<g id=\"Rechts\">")
        .unwrap();
    for (i, l) in added.iter().enumerate() {
        expected.insert(at + i, l);
    }
    let got: Vec<&str> = text.lines().collect();
    assert_eq!(got, expected);
}

// ---------------------------------------------------------------------------------------------
// (b) Mittellinien

// Eine Mittellinie (100,87)–(100,113) steht in der Gruppe „Vorne Mittellinien“ gestrichelt mit
// dem Strichpunktmuster 12 3 2 3, Breite 0,25.
#[test]
fn centerlines_are_written_dashed_in_their_own_group() {
    let sheet = manual_sheet(
        Paper::A3,
        vec![(
            "Vorne",
            vec![
                Mark::Line {
                    a: v(100.0, 87.0),
                    b: v(100.0, 113.0),
                    kind: MarkKind::CenterLine,
                },
                Mark::Line {
                    a: v(87.0, 100.0),
                    b: v(113.0, 100.0),
                    kind: MarkKind::CenterLine,
                },
            ],
        )],
    );
    let text = svg_of(&sheet);
    assert!(text.contains(
        "<path d=\"M 100 87 L 100 113\" stroke=\"#000\" stroke-width=\"0.25\" stroke-dasharray=\"12 3 2 3\" fill=\"none\" stroke-linecap=\"round\"/>"
    ));
    let r = svg_read::read(&text);
    assert_eq!(r.group_ids, vec!["Vorne", "Vorne Mittellinien"]);
    assert!(r.paths_of("Vorne").next().is_none());
    let paths: Vec<_> = r.paths_of("Vorne Mittellinien").collect();
    assert_eq!(paths.len(), 2);
    for p in &paths {
        assert_eq!(p.dasharray.as_deref(), Some("12 3 2 3"));
        assert_eq!(p.width, 0.25);
        assert_eq!(p.stroke, "#000");
        assert_eq!(p.fill, "none");
        assert_eq!(p.linecap, "round");
    }
    let l = lines_in(&r, "Vorne Mittellinien");
    assert!(has_seg(&l, (100.0, 87.0), (100.0, 113.0), TOL));
    assert!(has_seg(&l, (87.0, 100.0), (113.0, 100.0), TOL));
    // Keine Gruppe „Maße“, solange es nur Mittellinien gibt.
    assert!(!text.contains("Maße"));
}

// Aus dem Modell: alle Mittellinien des Quaders mit Bohrung stehen als Strecken in der Gruppe
// ihrer Ansicht, hinter deren Liniengruppe; Zahl und Lage stimmen mit den Marken des Blatts
// überein, und die Datei ist ohne Mittellinien-Schalter kürzer (kein Rest in der Ausgabe).
#[test]
fn centerlines_from_the_model_roundtrip() {
    let sheet = box_sheet(true);
    let r = svg_read::read(&svg_of(&sheet));
    let mut total = 0;
    for view in &sheet.views {
        let marks: Vec<(DVec2, DVec2)> = view
            .marks
            .iter()
            .filter_map(|m| match m {
                Mark::Line {
                    a,
                    b,
                    kind: MarkKind::CenterLine,
                } => Some((*a, *b)),
                _ => None,
            })
            .collect();
        assert_eq!(marks.len(), view.marks.len());
        let group = format!("{} Mittellinien", view.name);
        let got = lines_in(&r, &group);
        assert_eq!(got.len(), marks.len(), "{}", view.name);
        for (a, b) in &marks {
            assert!(has_seg(&got, (a.x, a.y), (b.x, b.y), TOL), "{a} {b}");
        }
        total += marks.len();
        if !marks.is_empty() {
            let i = r.group_ids.iter().position(|g| *g == view.name).unwrap();
            assert_eq!(r.group_ids[i + 1], group);
        }
    }
    assert!(total >= 2, "der Quader mit Bohrung hat Mittellinien");
}

// ---------------------------------------------------------------------------------------------
// Blatt von Hand (wie Stufe 15): Vorne bei (100,100) 1:1, Rechteck 40×20 → Blatt = p + (80,90).

fn v3(x: f64, y: f64, z: f64) -> DVec3 {
    DVec3::new(x, y, z)
}

fn rect_front(w: f64, h: f64) -> ViewLines {
    let corners = [
        v3(0.0, 0.0, 0.0),
        v3(w, 0.0, 0.0),
        v3(w, 0.0, h),
        v3(0.0, 0.0, h),
    ];
    let strokes: Vec<Stroke> = (0..4)
        .map(|i| {
            let (a, b) = (corners[i], corners[(i + 1) % 4]);
            let d = b - a;
            Stroke {
                curve: Curve::Line {
                    origin: a,
                    dir: d.normalize(),
                },
                t0: 0.0,
                t1: d.length(),
                origin: StrokeOrigin::Edge { body: 0, edge: i },
            }
        })
        .collect();
    let pieces = strokes
        .iter()
        .enumerate()
        .map(|(i, s)| Piece {
            stroke: i,
            t0: s.t0,
            t1: s.t1,
            vis: Visibility::Visible,
        })
        .collect();
    ViewLines {
        kind: ViewKind::Front,
        basis: Basis::of(ViewKind::Front),
        extent: 100.0,
        strokes,
        pieces,
        bbox: ((0.0, 0.0), (w, h)),
        centerlines: vec![CenterLine {
            a: v(20.0, 0.0),
            b: v(20.0, 20.0),
            source: CenterSource::Axis { body: 0, face: 3 },
        }],
        dropped_splits: 0,
    }
}

fn front_sheet(dims: &ResolvedDims) -> SheetModel {
    let mut d = drawing(Paper::A3);
    let f = d.add_view(
        ViewKind::Front,
        Scale { num: 1, den: 1 },
        Placement::Free { x: 100.0, y: 100.0 },
        None,
    );
    assert_eq!(f, ViewId(1));
    let vf = rect_front(40.0, 20.0);
    let res: BTreeMap<ViewId, &ViewLines> = BTreeMap::from([(f, &vf)]);
    build_sheet_with(&d, &res, dims)
}

fn h_dims(offset: f64) -> ResolvedDims {
    let mut dims = ResolvedDims::default();
    let s = spec(DimKind::Horizontal, offset);
    // Maß (0,20)–(40,20) im Bild → Blatt (80,110)–(120,110).
    dims.by_view.insert(
        ViewId(1),
        vec![(
            s,
            Ok(meas(40.0, lin(v(0.0, 20.0), v(40.0, 20.0), v(1.0, 0.0)))),
        )],
    );
    dims
}

// (c) Waagerechtes Maß über den ganzen Weg Bild → Blatt → SVG → Leser. Blatt: Maßlinie
// y = 110 + 10 = 120; Hilfslinien von 110 + 1 bis 120 + 2 bei x = 80 und 120; Pfeile innen
// (Spitzen auf den Hilfslinien) bei (80,120) nach rechts und (120,120) nach links, Fuß 3,5
// weiter innen; Text „40,0“ mittig bei (100, 121), Höhe 3,5. y nach oben: alles liegt über der
// Oberkante des Rechtecks (y = 110).
#[test]
fn dimension_roundtrip_geometry() {
    let sheet = front_sheet(&h_dims(10.0));
    let text = svg_of(&sheet);
    // Blattkoordinaten stehen unverändert in der Datei, die Wurzelgruppe spiegelt.
    assert!(text.contains("<path d=\"M 80 120 L 120 120\""));
    let r = svg_read::read(&text);
    assert_eq!(
        r.group_ids,
        vec!["Vorne", "Vorne Maße", "Vorne Mittellinien"]
    );
    let lines = lines_in(&r, "Vorne Maße");
    assert_eq!(lines.len(), 3, "{lines:?}");
    assert!(has_seg(&lines, (80.0, 120.0), (120.0, 120.0), TOL));
    assert!(has_seg(&lines, (80.0, 111.0), (80.0, 122.0), TOL));
    assert!(has_seg(&lines, (120.0, 111.0), (120.0, 122.0), TOL));
    // y-Orientierung: die Maßlinie liegt über den Umrisslinien der Ansicht (Oberkante 110).
    let top_of_part = lines_in(&r, "Vorne")
        .iter()
        .flat_map(|&(a, b)| [a.1, b.1])
        .fold(f64::MIN, f64::max);
    assert!((top_of_part - 110.0).abs() < TOL);
    assert!(lines.iter().all(|&(a, b)| a.1 > 110.0 && b.1 > 110.0));
    // Pfeile: zwei gefüllte Dreiecke, Spitze auf der Hilfslinie, Körper zur Mitte.
    let tris = triangles_in(&r, "Vorne Maße");
    assert_eq!(tris.len(), 2);
    for (tip, base_x) in [((80.0, 120.0), 83.5), ((120.0, 120.0), 116.5)] {
        let t = tris
            .iter()
            .find(|t| near(t[0], tip, TOL))
            .unwrap_or_else(|| panic!("Pfeil {tip:?}"));
        assert!((t[1].0 - base_x).abs() < TOL && (t[2].0 - base_x).abs() < TOL);
        assert!((t[1].1 - t[2].1).abs() - 1.2 < TOL);
    }
    // Text: aufrecht, Textachse entlang der Blatt-x-Achse, Anker Mitte.
    let ts: Vec<_> = r.texts_of("Vorne Maße").collect();
    assert_eq!(ts.len(), 1);
    let t = ts[0];
    assert_eq!(t.text, "40,0");
    assert!(near(t.pos, (100.0, 121.0), TOL), "{:?}", t.pos);
    assert_eq!(t.anchor, "middle");
    assert_eq!(t.font_size, 3.5);
    assert_eq!(t.font_family, "sans-serif");
    assert_eq!(t.fill, "#000");
    assert!(t.upright, "Schrift gespiegelt: {t:?}");
    assert!(near(t.axis, (1.0, 0.0), 1e-9), "{:?}", t.axis);
    assert!(near(t.up, (0.0, 1.0), 1e-9), "{:?}", t.up);
    assert!(t.angle_deg.abs() < 1e-9);
    // Mittellinie der Ansicht (Achse (20,0)–(20,20) → (100,90)–(100,110), Überstand 3).
    assert!(has_seg(
        &lines_in(&r, "Vorne Mittellinien"),
        (100.0, 87.0),
        (100.0, 113.0),
        TOL
    ));
}

// (d) Senkrechtes Maß (40,30)–(40,50), Abstand 10: Maßlinie x = 50, Text links davon bei
// (49, 40), Winkel 90°: die Textachse zeigt nach oben (0,1), „oben“ der Buchstaben nach links
// (−1,0); das ist eine Drehung, keine Spiegelung. Dazu andere Winkel: Achse (cos, sin), oben
// (−sin, cos).
#[test]
fn rotated_text_for_vertical_dimension() {
    let marks = layout_dimension(
        &spec(DimKind::Vertical, 10.0),
        &meas(20.0, lin(v(40.0, 30.0), v(40.0, 50.0), v(0.0, 1.0))),
        &id_xf(),
        0,
    );
    let sheet = manual_sheet(Paper::A3, vec![("V", marks)]);
    let text = svg_of(&sheet);
    assert!(text.contains("rotate(-90)"), "{text}");
    let r = svg_read::read(&text);
    let ts: Vec<_> = r.texts_of("V Maße").collect();
    assert_eq!(ts.len(), 1);
    let t = ts[0];
    assert_eq!(t.text, "20,0");
    assert!(near(t.pos, (49.0, 40.0), 1e-9), "{:?}", t.pos);
    assert!(t.upright);
    assert!(near(t.axis, (0.0, 1.0), 1e-9), "{:?}", t.axis);
    assert!(near(t.up, (-1.0, 0.0), 1e-9), "{:?}", t.up);
    assert!((t.angle_deg - 90.0).abs() < 1e-9);
    for deg in [-135.0f64, -45.0, 0.0, 30.0, 45.0, 90.0, 180.0] {
        let sheet = manual_sheet(
            Paper::A3,
            vec![(
                "R",
                vec![text_mark(
                    v(60.0, 70.0),
                    deg.to_radians(),
                    TextAnchor::Start,
                    "x",
                    MarkKind::Dimension,
                )],
            )],
        );
        let r = svg_read::read(&svg_of(&sheet));
        let t = &r.texts[0];
        let (s, c) = deg.to_radians().sin_cos();
        assert!(near(t.axis, (c, s), 1e-9), "{deg}: {:?}", t.axis);
        assert!(near(t.up, (-s, c), 1e-9), "{deg}: {:?}", t.up);
        assert!(t.upright);
        assert!(near(t.pos, (60.0, 70.0), 1e-9));
        assert_eq!(t.anchor, "start");
    }
}

// (e) Sonderzeichen im Text: maskiert in der Datei (`&amp;`, `&lt;`, `&gt;`, `&quot;`,
// `&apos;`), Ø ° ± − als Unicode; der Leser dekodiert den Text gleich.
#[test]
fn special_characters() {
    let samples = [
        "Ø10 H7 ±0,1 − & <x> \"q\" 'a' 45,0°",
        "a&b",
        "<<<>>>",
        "&amp;",
        "−0,5",
    ];
    for s in samples {
        let sheet = manual_sheet(
            Paper::A3,
            vec![(
                "T",
                vec![
                    text_mark(v(10.0, 10.0), 0.0, TextAnchor::End, s, MarkKind::Dimension),
                    text_mark(v(10.0, 20.0), 0.0, TextAnchor::Start, "x", MarkKind::Lost),
                ],
            )],
        );
        let r = svg_read::read(&svg_of(&sheet));
        assert_eq!(r.texts[0].text, s);
        assert_eq!(r.texts[0].anchor, "end");
    }
    let sheet = manual_sheet(
        Paper::A3,
        vec![(
            "T",
            vec![text_mark(
                v(0.0, 0.0),
                0.0,
                TextAnchor::Start,
                "Ø10 H7 ±0,1 − & <x> \"q\" 'a'",
                MarkKind::Dimension,
            )],
        )],
    );
    let text = svg_of(&sheet);
    assert!(
        text.contains(">Ø10 H7 ±0,1 − &amp; &lt;x&gt; &quot;q&quot; &apos;a&apos;</text>"),
        "{text}"
    );
    assert!(!text.contains("<x>"));
    // Steuerzeichen, die XML nicht erlaubt, werden zu Leerzeichen: die Datei bleibt lesbar.
    let sheet = manual_sheet(
        Paper::A3,
        vec![(
            "T",
            vec![text_mark(
                v(0.0, 0.0),
                0.0,
                TextAnchor::Start,
                "a\u{1}b\u{0}c",
                MarkKind::Dimension,
            )],
        )],
    );
    let r = svg_read::read(&svg_of(&sheet));
    assert_eq!(r.texts[0].text, "a b c");
}

// (f) Verlorene Maße sind rot (Linien, Pfeile, Text), gewöhnliche Maße derselben Gruppe
// schwarz. Verloren: letzte Lage (0,0)–(40,0) → Blatt y = 90, Abstand −10 → Linie y = 80.
#[test]
fn lost_marks_are_red() {
    let mut dims = h_dims(10.0);
    let mut s = spec(DimKind::Horizontal, -10.0);
    s.id = DimId(2);
    dims.by_view
        .get_mut(&ViewId(1))
        .unwrap()
        .push((s, Err(DimError::Lost("Kante 3".into()))));
    dims.last_known
        .insert(DimId(2), lin(v(0.0, 0.0), v(40.0, 0.0), v(1.0, 0.0)));
    let sheet = front_sheet(&dims);
    let r = svg_read::read(&svg_of(&sheet));
    let stroked = |color: &str| {
        r.paths_of("Vorne Maße")
            .filter(|p| p.stroke == color && p.fill == "none")
            .count()
    };
    assert_eq!(stroked("#c00"), 3, "Maßlinie und zwei Hilfslinien");
    assert_eq!(stroked("#000"), 3);
    let red: Vec<(P, P)> = r
        .paths_of("Vorne Maße")
        .filter(|p| p.stroke == "#c00")
        .flat_map(|p| p.segments.iter().map(|s| (s.start(), s.end())))
        .collect();
    assert!(has_seg(&red, (80.0, 80.0), (120.0, 80.0), TOL));
    let filled = |color: &str| r.paths_of("Vorne Maße").filter(|p| p.fill == color).count();
    assert_eq!((filled("#c00"), filled("#000")), (2, 2));
    let texts: Vec<_> = r.texts_of("Vorne Maße").collect();
    assert_eq!(texts.len(), 2);
    let lost: Vec<_> = texts.iter().filter(|t| t.fill == "#c00").collect();
    assert_eq!(lost.len(), 1);
    assert!(lost[0].text.starts_with("? "), "{}", lost[0].text);
    assert_eq!(texts.iter().filter(|t| t.fill == "#000").count(), 1);
}

// (g) Winkelmaß: Scheitel (10,10), Abstand 15, 90° zwischen +x und +y. Bogen Radius 15 von 0 bis
// π/2 (gegen den Uhrzeigersinn) um den Scheitel: Anfang (25,10), Ende (10,25), Mitte bei 45°
// (10 + 15·√2/2, 10 + 15·√2/2) – im ersten Viertel, nicht der Gegenbogen. Pfeile (25,10) und
// (10,25) mit Fußmitte (25;13,5) bzw. (13,5;25); Text „90,0°“ bei (10 + 16·√2/2, …), −45°.
#[test]
fn arc_and_arrow_roundtrip() {
    let marks = layout_dimension(
        &spec(DimKind::Angle, 15.0),
        &meas(
            90.0,
            Anchor::Angle {
                vertex: v(10.0, 10.0),
                d1: v(1.0, 0.0),
                d2: v(0.0, 1.0),
            },
        ),
        &id_xf(),
        0,
    );
    let sheet = manual_sheet(Paper::A3, vec![("W", marks)]);
    let r = svg_read::read(&svg_of(&sheet));
    let arcs: Vec<_> = r
        .paths_of("W Maße")
        .filter(|p| p.segments.iter().any(|s| matches!(s, Seg::Arc { .. })))
        .collect();
    assert_eq!(arcs.len(), 1);
    let arc = arcs[0];
    assert_eq!(arc.fill, "none");
    assert_eq!(arc.stroke, "#000");
    let first = arc.segments.first().unwrap();
    let last = arc.segments.last().unwrap();
    assert!(
        near(first.start(), (25.0, 10.0), TOL),
        "{:?}",
        first.start()
    );
    assert!(near(last.end(), (10.0, 25.0), TOL), "{:?}", last.end());
    for seg in &arc.segments {
        for k in 0..=10 {
            let p = seg.point_at(k as f64 / 10.0);
            let rad = ((p.0 - 10.0).powi(2) + (p.1 - 10.0).powi(2)).sqrt();
            assert!((rad - 15.0).abs() < 1e-5, "Radius {rad}");
            assert!(p.0 >= 10.0 - 1e-5 && p.1 >= 10.0 - 1e-5, "Gegenbogen {p:?}");
        }
    }
    assert!(arc.distance_to((10.0 + 15.0 * H2, 10.0 + 15.0 * H2)) < 1e-5);
    let tris = triangles_in(&r, "W Maße");
    assert_eq!(tris.len(), 2);
    for (tip, base) in [((25.0, 10.0), (25.0, 13.5)), ((10.0, 25.0), (13.5, 25.0))] {
        let t = tris.iter().find(|t| near(t[0], tip, TOL)).expect("Pfeil");
        let mid = ((t[1].0 + t[2].0) / 2.0, (t[1].1 + t[2].1) / 2.0);
        assert!(near(mid, base, TOL), "{mid:?} {base:?}");
    }
    let t = &r.texts[0];
    assert_eq!(t.text, "90,0°");
    assert!((t.angle_deg + 45.0).abs() < 1e-9, "{}", t.angle_deg);
    assert!(near(t.pos, (10.0 + 16.0 * H2, 10.0 + 16.0 * H2), 1e-5));
    assert!(t.upright);
    // Bogen über 90° und ganzer Kreis: 30° bis 280° (250° gegen den Uhrzeigersinn) geht durch
    // 180°, nicht durch 0°; der Kreis besteht aus vier Stücken und schließt sich.
    let big = Mark::Arc {
        center: v(100.0, 100.0),
        r: 20.0,
        start: PI / 6.0,
        end: PI / 6.0 + 250f64.to_radians(),
        kind: MarkKind::Dimension,
    };
    let full = Mark::Arc {
        center: v(200.0, 100.0),
        r: 5.0,
        start: 0.0,
        end: 2.0 * PI,
        kind: MarkKind::Dimension,
    };
    let sheet = manual_sheet(Paper::A3, vec![("B", vec![big, full])]);
    let r = svg_read::read(&svg_of(&sheet));
    let p: Vec<_> = r.paths_of("B Maße").collect();
    assert_eq!(p.len(), 2);
    assert!(p[0].distance_to((80.0, 100.0)) < 1e-4, "geht durch 180°");
    assert!(p[0].distance_to((120.0, 100.0)) > 1.0, "nicht durch 0°");
    assert!(near(
        p[0].segments[0].start(),
        (100.0 + 20.0 * 3f64.sqrt() / 2.0, 110.0),
        1e-5
    ));
    assert_eq!(p[1].segments.len(), 4);
    for q in [
        (205.0, 100.0),
        (200.0, 105.0),
        (195.0, 100.0),
        (200.0, 95.0),
    ] {
        assert!(p[1].distance_to(q) < 1e-4, "{q:?}");
    }
}

// (h) Pfeil: gefülltes Dreieck. Spitze (50,60), Richtung (−1,0) → Fuß (46,5; 60), Ecken
// (46,5; 60 ± 0,6): Länge 3,5, Breite 1,2. Schräge Richtung (0,6; 0,8): Fußmitte = Spitze +
// 3,5·Richtung, die beiden Ecken liegen 1,2 auseinander, senkrecht zur Richtung.
#[test]
fn fill_arrows_are_triangles_with_expected_size() {
    let sheet = manual_sheet(
        Paper::A3,
        vec![(
            "P",
            vec![
                Mark::Arrow {
                    tip: v(50.0, 60.0),
                    dir: v(-1.0, 0.0),
                    kind: MarkKind::Dimension,
                },
                Mark::Arrow {
                    tip: v(100.0, 100.0),
                    dir: v(0.6, 0.8),
                    kind: MarkKind::Dimension,
                },
            ],
        )],
    );
    let text = svg_of(&sheet);
    assert!(
        text.contains(
            "<path d=\"M 50 60 L 46.5 59.4 L 46.5 60.6 Z\" fill=\"#000\" stroke=\"none\"/>"
        )
    );
    let r = svg_read::read(&text);
    let tris = triangles_in(&r, "P Maße");
    assert_eq!(tris.len(), 2);
    for p in r.paths_of("P Maße") {
        assert_eq!(p.fill, "#000");
        assert_eq!(p.stroke, "none");
    }
    let t = tris[0];
    assert!(near(t[0], (50.0, 60.0), TOL));
    assert!((t[1].0 - 46.5).abs() < TOL && (t[2].0 - 46.5).abs() < TOL);
    assert!(((t[1].1 - t[2].1).abs() - 1.2).abs() < TOL, "Breite");
    let t = tris[1];
    let base = (100.0 + 3.5 * 0.6, 100.0 + 3.5 * 0.8);
    let mid = ((t[1].0 + t[2].0) / 2.0, (t[1].1 + t[2].1) / 2.0);
    assert!(near(mid, base, TOL));
    let (wx, wy) = (t[1].0 - t[2].0, t[1].1 - t[2].1);
    assert!((wx.hypot(wy) - 1.2).abs() < TOL);
    assert!(
        (wx * 0.6 + wy * 0.8).abs() < TOL,
        "Breite senkrecht zur Richtung"
    );
    let (lx, ly) = (t[0].0 - mid.0, t[0].1 - mid.1);
    assert!((lx.hypot(ly) - 3.5).abs() < TOL, "Länge");
}

// (i) Eindeutige gültige Kennungen auch bei gleichen und seltsamen Ansichtsnamen.
#[test]
fn groups_have_unique_valid_ids_with_odd_view_names() {
    let both = || {
        vec![
            line_mark(MarkKind::Dimension),
            line_mark(MarkKind::CenterLine),
        ]
    };
    // „A Maße“ kollidiert mit der Gruppe der ersten Ansicht.
    let sheet = manual_sheet(
        Paper::A3,
        vec![("A", both()), ("A", both()), ("A Maße", both())],
    );
    let r = svg_read::read(&svg_of(&sheet));
    assert_eq!(
        r.group_ids,
        vec![
            "A",
            "A Maße",
            "A Mittellinien",
            "A 2",
            "A 2 Maße",
            "A 2 Mittellinien",
            "A Maße 2",
            "A Maße 2 Maße",
            "A Maße 2 Mittellinien",
        ]
    );
    // Namen mit Sonderzeichen und leer; der Leser erzwingt Eindeutigkeit und gültige
    // Anführungszeichen. Ansichten ohne Name heißen „Ansicht“.
    let names = ["<&\"'>", "", "", "Ä ö ü ß", "a\tb\nc", "x\u{1}y"];
    let sheet = manual_sheet(Paper::A3, names.iter().map(|n| (*n, both())).collect());
    let r = svg_read::read(&svg_of(&sheet));
    assert_eq!(r.group_ids.len(), names.len() * 3);
    let mut ids = r.group_ids.clone();
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), r.group_ids.len(), "Kennungen eindeutig");
    assert_eq!(r.group_ids[0], "<&\"'>");
    assert_eq!(r.group_ids[1], "<&\"'> Maße");
    assert_eq!(r.group_ids[3], "Ansicht");
    assert_eq!(r.group_ids[6], "Ansicht 2");
    assert_eq!(r.group_ids[7], "Ansicht 2 Maße");
    assert_eq!(r.group_ids[9], "Ä ö ü ß");
    assert_eq!(r.group_ids[12], "a\tb\nc");
    assert_eq!(r.group_ids[15], "x y");
    for g in &r.group_ids {
        if g.ends_with(" Maße") || g.ends_with(" Mittellinien") {
            assert_eq!(r.paths_of(g).count(), 1, "{g}");
        }
    }
}

// (j) Nicht endliche Zahlen in Markierungen: benannter Fehler, nichts wird geschrieben.
#[test]
fn non_finite_marks_refuse_with_nonfinite_error() {
    let bad = [
        Mark::Line {
            a: v(f64::NAN, 0.0),
            b: v(1.0, 1.0),
            kind: MarkKind::Dimension,
        },
        Mark::Line {
            a: v(0.0, 0.0),
            b: v(1.0, f64::INFINITY),
            kind: MarkKind::CenterLine,
        },
        Mark::Arrow {
            tip: v(0.0, f64::NAN),
            dir: v(1.0, 0.0),
            kind: MarkKind::Dimension,
        },
        Mark::Arrow {
            tip: v(0.0, 0.0),
            dir: v(f64::INFINITY, 0.0),
            kind: MarkKind::Lost,
        },
        Mark::Arc {
            center: v(0.0, 0.0),
            r: f64::NAN,
            start: 0.0,
            end: 1.0,
            kind: MarkKind::Dimension,
        },
        Mark::Arc {
            center: v(0.0, 0.0),
            r: 1.0,
            start: 0.0,
            end: f64::NEG_INFINITY,
            kind: MarkKind::Dimension,
        },
        text_mark(
            v(f64::NAN, 0.0),
            0.0,
            TextAnchor::Start,
            "x",
            MarkKind::Dimension,
        ),
        text_mark(
            v(0.0, 0.0),
            f64::INFINITY,
            TextAnchor::Start,
            "x",
            MarkKind::Dimension,
        ),
        Mark::Text {
            pos: v(0.0, 0.0),
            height: f64::NAN,
            angle: 0.0,
            anchor: TextAnchor::Start,
            text: "x".into(),
            kind: MarkKind::Dimension,
        },
    ];
    for m in bad {
        // Eine gute Ansicht davor: „nichts geschrieben“ schließt auch Teilausgabe aus.
        let sheet = manual_sheet(
            Paper::A3,
            vec![
                ("Gut", vec![line_mark(MarkKind::Dimension)]),
                ("Schlecht", vec![m.clone()]),
            ],
        );
        assert!(
            matches!(svg_string(&sheet), Err(DrawError::NonFinite)),
            "{m:?}"
        );
        let mut out = Vec::new();
        assert!(matches!(
            write_svg(&sheet, &mut out),
            Err(DrawError::NonFinite)
        ));
        assert!(out.is_empty(), "bei einem Fehler wird nichts geschrieben");
    }
}

// (k) A0-Blatt (1189 × 841) und große Zahlen: Zahlen ohne Exponent und ohne nachgestellte
// Nullen, höchstens sechs Nachkommastellen; Werte bis 99 999,5 kommen unverändert zurück.
#[test]
fn a0_sheet_and_large_numbers() {
    let sheet = manual_sheet(
        Paper::A0,
        vec![(
            "Groß",
            vec![
                Mark::Line {
                    a: v(1100.5, 800.25),
                    b: v(99999.5, -99999.5),
                    kind: MarkKind::Dimension,
                },
                Mark::Line {
                    a: v(1e-9, 0.1 + 0.2),
                    b: v(-1e-9, 1.23456),
                    kind: MarkKind::CenterLine,
                },
                text_mark(
                    v(1180.0, 830.0),
                    0.0,
                    TextAnchor::End,
                    "99999,5",
                    MarkKind::Dimension,
                ),
                Mark::Arrow {
                    tip: v(1189.0, 841.0),
                    dir: v(0.0, -1.0),
                    kind: MarkKind::Dimension,
                },
            ],
        )],
    );
    let text = svg_of(&sheet);
    assert!(text.contains("viewBox=\"0 0 1189 841\""));
    assert!(text.contains("<path d=\"M 1100.5 800.25 L 99999.5 -99999.5\""));
    // 0,1 + 0,2 wird auf sechs Stellen gerundet: 0.3; ±1e-9 wird 0 (nicht „-0“).
    assert!(text.contains("M 0 0.3 L 0 1.23456\""), "{text}");
    for line in text.lines().filter(|l| l.contains(" d=\"")) {
        let d = line
            .split("d=\"")
            .nth(1)
            .unwrap()
            .split('"')
            .next()
            .unwrap();
        assert!(!d.contains(['e', 'E', 'N']) && !d.contains("-0 "), "{d}");
    }
    let r = svg_read::read(&text);
    let l = lines_in(&r, "Groß Maße");
    assert!(has_seg(&l, (1100.5, 800.25), (99999.5, -99999.5), 1e-9));
    let t = &r.texts[0];
    assert!(near(t.pos, (1180.0, 830.0), 1e-9));
    assert!(t.upright && t.text == "99999,5");
    let tri = triangles_in(&r, "Groß Maße");
    assert!(near(tri[0][0], (1189.0, 841.0), 1e-9));
    assert!((tri[0][1].1 - 837.5).abs() < 1e-9);
    // Gerundet, nicht abgeschnitten.
    let sheet = manual_sheet(
        Paper::A0,
        vec![(
            "R",
            vec![Mark::Line {
                a: v(0.1234564, 0.1234566),
                b: v(1.0, 2.0),
                kind: MarkKind::Dimension,
            }],
        )],
    );
    assert!(svg_of(&sheet).contains("M 0.123456 0.123457 L 1 2"));
}
