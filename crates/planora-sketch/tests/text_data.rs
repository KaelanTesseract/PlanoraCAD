//! Strenge Prüfung der Texte in den Rohdaten (Skizzen 8, Task 4, Spec G5): Zahlen endlich, Höhe
//! größer als null, Zeichenabstand im Bereich, Rahmen aus vier Linien zwischen vier Ecken, Pfad
//! vorhanden und keine Textkurve, Kurven und Punkte vorhanden und keinem zweiten Text zugeordnet,
//! Inhalt höchstens 500 Zeichen. Eine kaputte Skizze wird abgelehnt, nicht still repariert.

use planora_i18n::tr;
use planora_math::{DVec2, Frame, Tolerance};
use planora_sketch::model::{CurveId, CurveKind, Sketch};
use planora_sketch::pattern::{CircularMode, PatternDir, PatternKind, PatternPlan};
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

/// Wie [`data`], dazu eine freie Linie mit rechteckigem Muster entlang einer freien
/// Richtungslinie, ein kreisförmiges Muster um einen freien Punkt und ein Spline mit Griff.
/// Liefert die Rohdaten und die Kurve des Griffs.
fn data_with_parts() -> (SketchData, CurveId) {
    let (d, _) = data();
    let mut s = Sketch::from_data(d).unwrap();
    let a = s.add_point(60.0, 0.0).unwrap();
    let b = s.add_point(70.0, 0.0).unwrap();
    let free = s.add_line(a, b).unwrap();
    let c = s.add_point(60.0, 10.0).unwrap();
    let e = s.add_point(70.0, 15.0).unwrap();
    let dir = s.add_line(c, e).unwrap();
    s.add_pattern(&PatternPlan {
        kind: PatternKind::Rect {
            dirs: [PatternDir::Line(dir), PatternDir::SketchY],
            extent: [false, false],
            symmetric: [false, false],
        },
        sources: vec![free],
        counts: [2, 1],
        values: [20.0, 10.0],
        suppressed: Vec::new(),
    })
    .unwrap();
    let f = s.add_point(100.0, 0.0).unwrap();
    let g = s.add_point(110.0, 0.0).unwrap();
    let other = s.add_line(f, g).unwrap();
    let center = s.add_point(100.0, -10.0).unwrap();
    s.add_pattern(&PatternPlan {
        kind: PatternKind::Circular {
            center,
            mode: CircularMode::Full,
        },
        sources: vec![other],
        counts: [3, 1],
        values: [0.0, 0.0],
        suppressed: Vec::new(),
    })
    .unwrap();
    let p: Vec<_> = [(0.0, 50.0), (10.0, 55.0), (20.0, 50.0)]
        .iter()
        .map(|&(x, y)| s.add_point(x, y).unwrap())
        .collect();
    let spline = s.add_spline(&p).unwrap();
    let handle = s.activate_tangent_handle(spline, p[1]).unwrap();
    (s.to_data(), handle)
}

/// Wie [`refused`] auf den Rohdaten aus [`data_with_parts`].
fn refused_with_parts(what: &str, key: &str, f: impl FnOnce(&mut SketchData, CurveId)) {
    let (mut d, handle) = data_with_parts();
    f(&mut d, handle);
    let e = Sketch::from_data(d).expect_err(what).to_string();
    assert!(e.contains(&reason(key)), "{what}: {e}");
}

/// Die erste erzeugte Kurve des Textes `t` mit `pick`.
fn glyph_where(d: &SketchData, t: usize, pick: impl Fn(&CurveKind) -> bool) -> CurveId {
    *d.texts[t]
        .curves
        .iter()
        .find(|c| pick(&d.curves[c.index()].kind))
        .expect("Kurve dieser Art")
}

#[test]
fn from_data_rejects_foreign_glyph_parts() {
    // Ohne Eingriff laden die Rohdaten.
    let (d, _) = data_with_parts();
    let s = Sketch::from_data(d.clone()).unwrap();
    assert_eq!(s.to_data(), d);

    let kind = "sketch-err-text-data-kind";
    refused("Kegelschnitt mit Rho 0,7", kind, |d| {
        let c = glyph_where(d, 0, |k| matches!(k, CurveKind::Conic { .. }));
        let CurveKind::Conic { rho, .. } = &mut d.curves[c.index()].kind else {
            unreachable!()
        };
        *rho = 0.7;
    });
    refused("Spline vom Grad 2", kind, |d| {
        let c = glyph_where(d, 0, |k| matches!(k, CurveKind::ControlSpline { .. }));
        let CurveKind::ControlSpline { degree, knots, .. } = &mut d.curves[c.index()].kind else {
            unreachable!()
        };
        *degree = 2;
        *knots = vec![0.0, 0.0, 0.0, 0.5, 1.0, 1.0, 1.0];
    });
    refused("Kreis statt Strecke", kind, |d| {
        let c = glyph_where(d, 0, |k| matches!(k, CurveKind::Line { .. }));
        let CurveKind::Line { a, .. } = d.curves[c.index()].kind else {
            unreachable!()
        };
        d.vars.push(1.0);
        d.curves[c.index()].kind = CurveKind::Circle {
            center: a,
            radius: d.vars.len() - 1,
        };
    });

    let pattern = "sketch-err-text-data-pattern";
    refused_with_parts("Richtungslinie eines Musters", pattern, |d, _| {
        let glyph = glyph_where(d, 0, |k| matches!(k, CurveKind::Line { .. }));
        let m = d
            .patterns
            .iter_mut()
            .find(|m| matches!(m.kind, PatternKind::Rect { .. }))
            .unwrap();
        let PatternKind::Rect { dirs, .. } = &mut m.kind else {
            unreachable!()
        };
        dirs[0] = PatternDir::Line(glyph);
    });
    refused_with_parts("Mitte eines Musters", pattern, |d, _| {
        let p = d.texts[0].points[0];
        let m = d
            .patterns
            .iter_mut()
            .find(|m| matches!(m.kind, PatternKind::Circular { .. }))
            .unwrap();
        let PatternKind::Circular { center, .. } = &mut m.kind else {
            unreachable!()
        };
        *center = p;
    });
    refused_with_parts("Vorlage eines Musters", pattern, |d, _| {
        let m = &d.patterns[0];
        let (source, points) = (m.sources[0], m.source_points.clone());
        d.texts[0].curves.push(source);
        d.texts[0].points.extend(points);
    });
    refused_with_parts("Exemplar eines Musters", pattern, |d, _| {
        let inst = d.patterns[0].instances[0].clone();
        d.texts[0].curves.extend(inst.curves);
        d.texts[0].points.extend(inst.points);
    });

    refused_with_parts(
        "Griff eines Splines",
        "sketch-err-text-data-handle",
        |d, handle| {
            d.texts[0].curves.push(handle);
        },
    );

    refused(
        "Rahmenlinie ohne Konstruktion",
        "sketch-err-text-data-construction",
        |d| {
            let (lines, _) = frame(&d.texts[1]);
            d.curves[lines[2].index()].construction = false;
        },
    );
}
