//! Strenge Prüfung der Texte in den Rohdaten (Skizzen 8, Task 4, Spec G5): Zahlen endlich, Höhe
//! größer als null, Zeichenabstand im Bereich, Rahmen aus vier Linien zwischen vier Ecken, Pfad
//! vorhanden und keine Textkurve, Kurven und Punkte vorhanden und keinem zweiten Text zugeordnet,
//! Inhalt höchstens 500 Zeichen. Eine kaputte Skizze wird abgelehnt, nicht still repariert.

use planora_i18n::tr;
use planora_math::{DVec2, Frame, Tolerance};
use planora_sketch::model::{CurveId, Sketch};
use planora_sketch::text::{HAlign, TextParams, TextPlacement, VAlign};
use planora_sketch::text_shape::{
    Contour, Glyph, MAX_TEXT_CHARS, MAX_TEXT_CURVES, Seg, ShapedLine, TextShape, TextStyle,
};
use planora_sketch::{SketchData, TextRecord};
use std::sync::Arc;

fn v(x: f64, y: f64) -> DVec2 {
    DVec2::new(x, y)
}

/// Je Zeichen ein Quadrat aus vier Strecken und ein Loch mit einer quadratischen und einer
/// kubischen Bézierkurve.
fn shape(content: &str) -> Arc<TextShape> {
    let glyphs: Vec<Glyph> = content
        .chars()
        .enumerate()
        .map(|(i, _)| Glyph {
            origin: v(i as f64 * 0.8, 0.0),
            advance: 0.8,
            contours: vec![
                Contour {
                    start: v(0.0, 0.0),
                    segs: vec![
                        Seg::Line(v(0.6, 0.0)),
                        Seg::Line(v(0.6, 0.7)),
                        Seg::Line(v(0.0, 0.7)),
                        Seg::Line(v(0.0, 0.0)),
                    ],
                },
                Contour {
                    start: v(0.2, 0.2),
                    segs: vec![
                        Seg::Line(v(0.4, 0.2)),
                        Seg::Quad(v(0.5, 0.35), v(0.4, 0.5)),
                        Seg::Cubic(v(0.35, 0.55), v(0.25, 0.55), v(0.2, 0.5)),
                        Seg::Line(v(0.2, 0.2)),
                    ],
                },
            ],
        })
        .collect();
    let width = content.chars().count() as f64 * 0.8;
    Arc::new(TextShape {
        lines: vec![ShapedLine { glyphs, width }],
        ascender: 0.9,
        descender: -0.2,
        line_height: 1.2,
        substituted: None,
        synthetic: false,
        missing_chars: false,
    })
}

fn params(content: &str) -> TextParams {
    TextParams {
        content: content.into(),
        style: TextStyle {
            font: "Test".into(),
            bold: false,
            italic: false,
            spacing: 0.0,
        },
        height: 5.0,
        flip_h: false,
        flip_v: false,
    }
}

/// Zwei Texte im Rahmen und eine freie Linie (für einen Pfad).
fn data() -> (SketchData, CurveId) {
    let mut s = Sketch::new(Frame::WORLD, "S", Tolerance::default());
    s.add_text_box(
        v(0.0, 0.0),
        v(40.0, 10.0),
        params("ab"),
        HAlign::Left,
        VAlign::Bottom,
        shape("ab"),
    )
    .unwrap();
    s.add_text_box(
        v(0.0, 20.0),
        v(40.0, 30.0),
        params("c"),
        HAlign::Center,
        VAlign::Top,
        shape("c"),
    )
    .unwrap();
    let a = s.add_point(0.0, -10.0).unwrap();
    let b = s.add_point(30.0, -10.0).unwrap();
    let line = s.add_line(a, b).unwrap();
    (s.to_data(), line)
}

fn reason(key: &str) -> String {
    tr!("sketch-err-text-data", reason = planora_i18n::t(key))
}

/// `f` verdirbt die Rohdaten; das Laden muss mit dem Grund `key` scheitern.
fn refused(what: &str, key: &str, f: impl FnOnce(&mut SketchData)) {
    let (mut d, _) = data();
    f(&mut d);
    let e = Sketch::from_data(d).expect_err(what).to_string();
    assert!(e.contains(&reason(key)), "{what}: {e}");
}

fn frame(t: &TextRecord) -> ([CurveId; 4], [planora_sketch::PointId; 4]) {
    match t.placement {
        TextPlacement::Box { lines, corners, .. } => (lines, corners),
        TextPlacement::Path { .. } => panic!("kein Rahmen"),
    }
}

#[test]
fn valid_data_loads() {
    let (d, _) = data();
    let s = Sketch::from_data(d.clone()).unwrap();
    assert_eq!(s.to_data(), d);
}

#[test]
fn path_text_loads() {
    let (mut d, line) = data();
    d.texts[1].placement = TextPlacement::Path {
        path: line,
        above: true,
        fit: false,
        h: HAlign::Right,
    };
    let s = Sketch::from_data(d.clone()).unwrap();
    assert_eq!(s.to_data(), d);
}

#[test]
fn from_data_rejects() {
    let h = "sketch-err-text-data-height";
    refused("Höhe 0", h, |d| d.texts[0].height = 0.0);
    refused("Höhe negativ", h, |d| d.texts[0].height = -1.0);
    refused("Höhe NaN", h, |d| d.texts[0].height = f64::NAN);
    refused("Höhe unendlich", h, |d| d.texts[0].height = f64::INFINITY);
    let sp = "sketch-err-text-data-spacing";
    refused("Abstand 1001", sp, |d| d.texts[0].style.spacing = 1001.0);
    refused("Abstand negativ", sp, |d| d.texts[0].style.spacing = -0.5);
    refused("Abstand NaN", sp, |d| d.texts[0].style.spacing = f64::NAN);
    let c = "sketch-err-text-data-content";
    refused("501 Zeichen", c, |d| {
        d.texts[0].content = "x".repeat(MAX_TEXT_CHARS + 1)
    });
    refused("leerer Inhalt", c, |d| d.texts[0].content = " \n ".into());
    let f = "sketch-err-text-data-frame";
    refused("Rahmen mit 3 Linien", f, |d| {
        let TextPlacement::Box { lines, .. } = &mut d.texts[0].placement else {
            unreachable!()
        };
        lines[3] = lines[2];
    });
    refused("Rahmen mit 3 Ecken", f, |d| {
        let TextPlacement::Box { corners, .. } = &mut d.texts[0].placement else {
            unreachable!()
        };
        corners[3] = corners[2];
    });
    refused("Linien in falscher Reihenfolge", f, |d| {
        let TextPlacement::Box { lines, .. } = &mut d.texts[0].placement else {
            unreachable!()
        };
        lines.swap(0, 1);
    });
    refused("Rahmenlinie ist keine Linie", f, |d| {
        // Eine erzeugte Kurve (Kegelschnitt) statt der Gegenlinie.
        let conic = d.texts[0].curves[5];
        let TextPlacement::Box { lines, .. } = &mut d.texts[0].placement else {
            unreachable!()
        };
        lines[2] = conic;
    });
    refused(
        "Pfad gleich einer Textkurve",
        "sketch-err-text-data-path",
        |d| {
            let glyph = d.texts[0].curves[0];
            d.texts[1].placement = TextPlacement::Path {
                path: glyph,
                above: false,
                fit: false,
                h: HAlign::Left,
            };
        },
    );
    refused(
        "Pfad gleich einer eigenen Textkurve",
        "sketch-err-text-data-path",
        |d| {
            let glyph = d.texts[1].curves[0];
            d.texts[1].placement = TextPlacement::Path {
                path: glyph,
                above: false,
                fit: false,
                h: HAlign::Left,
            };
        },
    );
    let sh = "sketch-err-text-data-shared";
    refused("eine Kurve in zwei Texten", sh, |d| {
        let c = d.texts[0].curves[0];
        d.texts[1].curves.push(c);
    });
    refused("eine Kurve zweimal im selben Text", sh, |d| {
        let c = d.texts[0].curves[0];
        d.texts[0].curves.push(c);
    });
    refused("ein Punkt in zwei Texten", sh, |d| {
        let p = d.texts[0].points[0];
        d.texts[1].points.push(p);
    });
    refused("Rahmenlinie als erzeugte Kurve", sh, |d| {
        let (lines, _) = frame(&d.texts[0]);
        d.texts[1].curves.push(lines[0]);
    });
    refused("Ecke als erzeugter Punkt", sh, |d| {
        let (_, corners) = frame(&d.texts[0]);
        d.texts[0].points.push(corners[0]);
    });
    refused("zu viele Kurven", "sketch-err-text-data-curves", |d| {
        let c = d.texts[0].curves[0];
        d.texts[0].curves = vec![c; MAX_TEXT_CURVES + 1];
    });
    let del = "sketch-err-text-data-deleted";
    refused("gelöschte erzeugte Kurve", del, |d| {
        let c = d.texts[0].curves[0];
        d.curves[c.index()].alive = false;
    });
    // Ein gelöschter erzeugter Punkt fällt schon an seinen lebenden Kurven auf.
    let (mut d, _) = data();
    let p = d.texts[0].points[0];
    d.points[p.index()].alive = false;
    assert!(Sketch::from_data(d).is_err(), "gelöschter erzeugter Punkt");
    refused("gelöschter Pfad", del, |d| {
        let line = CurveId::from_index(d.curves.len() - 1);
        d.curves[line.index()].alive = false;
        d.texts[1].placement = TextPlacement::Path {
            path: line,
            above: false,
            fit: false,
            h: HAlign::Left,
        };
    });
    let obj = "sketch-err-text-data-object";
    refused("fehlende Kurve", obj, |d| {
        let n = d.curves.len();
        d.texts[0].curves.push(CurveId::from_index(n));
    });
    refused("fehlender Punkt", obj, |d| {
        let n = d.points.len();
        d.texts[1]
            .points
            .push(planora_sketch::PointId::from_index(n + 7));
    });
    refused("fehlender Pfad", obj, |d| {
        let n = d.curves.len();
        d.texts[1].placement = TextPlacement::Path {
            path: CurveId::from_index(n),
            above: false,
            fit: false,
            h: HAlign::Left,
        };
    });
}

#[test]
fn deleted_text_is_checked_only_for_values_and_indices() {
    // Ein gelöschter Text darf auf gelöschte Teile zeigen (so lässt ihn `remove_text` zurück)
    // und seine Kurven einem lebenden Text überlassen haben (Text auflösen, Spec F).
    let (mut d, _) = data();
    d.texts[0].alive = false;
    let c = d.texts[0].curves[0];
    d.curves[c.index()].alive = false;
    let other = d.texts[1].curves[0];
    d.texts[0].curves.push(other);
    Sketch::from_data(d.clone()).unwrap();
    // Werte und Indizes zählen auch dann.
    let mut bad = d.clone();
    bad.texts[0].height = f64::NAN;
    assert!(Sketch::from_data(bad).is_err());
    let mut bad = d;
    let n = bad.curves.len();
    bad.texts[0].curves.push(CurveId::from_index(n));
    assert!(Sketch::from_data(bad).is_err());
}
