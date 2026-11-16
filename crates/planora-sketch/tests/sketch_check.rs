//! Skizzenstatus, Liste der unterbestimmten Elemente und Schloss (Skizzen 11a, Task 15, A4).

use planora_kernel::Curve;
use planora_math::{DVec2, DVec3, Frame, Tolerance};
use planora_sketch::check::{CheckItem, SketchStatus};
use planora_sketch::constraint::{ConstraintKind as K, LineOrPoints};
use planora_sketch::helpers::ProjectStyle;
use planora_sketch::model::Sketch;
use planora_sketch::text::{HAlign, TextParams, VAlign};
use planora_sketch::text_shape::{
    Contour, Glyph, MAX_TEXT_CHARS, MAX_TEXT_CURVES, Seg, ShapedLine, TextShape, TextShapeError,
    TextShaper, TextStyle,
};
use std::sync::Arc;

const ADV: f64 = 0.8;
/// Rechter und oberer Rand der Glyphe des Test-Setzers.
const INK_RIGHT: f64 = 0.6;
const INK_TOP: f64 = 0.7;
const ASC: f64 = 0.9;
const DESC: f64 = -0.2;
const LINE_HEIGHT: f64 = 1.2;

fn v(x: f64, y: f64) -> DVec2 {
    DVec2::new(x, y)
}

/// Die Glyphe des Test-Setzers: außen ein Quadrat aus vier Strecken, innen ein Loch aus zwei
/// Strecken, einer quadratischen und einer kubischen Bézierkurve. 8 Kurven, 11 Punkte.
fn box_glyph() -> Vec<Contour> {
    vec![
        Contour {
            start: v(0.0, 0.0),
            segs: vec![
                Seg::Line(v(INK_RIGHT, 0.0)),
                Seg::Line(v(INK_RIGHT, INK_TOP)),
                Seg::Line(v(0.0, INK_TOP)),
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
    ]
}

/// Test-Setzer: je Zeichen ein Quadrat mit Loch, Leerzeichen ohne Umriss, `\n` bricht die Zeile.
/// Fett gilt als nachgebildet, die Schrift „Fehlt“ als ersetzt.
struct BoxShaper;

impl TextShaper for BoxShaper {
    fn shape(&self, content: &str, style: &TextStyle) -> Result<TextShape, TextShapeError> {
        if content.chars().count() > MAX_TEXT_CHARS {
            return Err(TextShapeError::TooLong);
        }
        if content.trim().is_empty() {
            return Err(TextShapeError::Empty);
        }
        let mut lines = Vec::new();
        for text in content.split('\n') {
            let chars: Vec<char> = text.chars().collect();
            let mut glyphs = Vec::new();
            let mut pen = 0.0;
            for (i, ch) in chars.iter().enumerate() {
                let contours = if ch.is_whitespace() {
                    Vec::new()
                } else {
                    box_glyph()
                };
                glyphs.push(Glyph {
                    origin: v(pen, 0.0),
                    advance: ADV,
                    contours,
                });
                pen += ADV;
                if i + 1 < chars.len() {
                    pen += ADV * style.spacing / 100.0;
                }
            }
            lines.push(ShapedLine { glyphs, width: pen });
        }
        let shape = TextShape {
            lines,
            ascender: ASC,
            descender: DESC,
            line_height: LINE_HEIGHT,
            substituted: (style.font == "Fehlt").then(|| style.font.clone()),
            synthetic: style.bold,
            missing_chars: false,
        };
        if shape.segment_count() > MAX_TEXT_CURVES {
            return Err(TextShapeError::TooManyCurves);
        }
        Ok(shape)
    }
}

fn sketch() -> Sketch {
    Sketch::new(Frame::WORLD, "S", Tolerance::default())
}

#[test]
fn free_line_is_under() {
    let mut s = sketch();
    let a = s.add_point(0.0, 0.0).unwrap();
    let b = s.add_point(5.0, 1.0).unwrap();
    let l = s.add_line(a, b).unwrap();
    let r = s.solve();
    assert_eq!(s.status(&r), SketchStatus::Under);
    assert_eq!(s.under_items(&r), vec![CheckItem::Curve(l)]);
    assert!(!s.is_locked(&r));
}

#[test]
fn line_ends_count_via_curve() {
    let mut s = sketch();
    let a = s.add_point(0.0, 0.0).unwrap();
    let b = s.add_point(5.0, 1.0).unwrap();
    s.add_line(a, b).unwrap();
    let r = s.solve();
    assert!(r.free_points.contains(&a));
    assert!(
        !s.under_items(&r)
            .iter()
            .any(|i| matches!(i, CheckItem::Point(_)))
    );
}

#[test]
fn lone_point_listed() {
    let mut s = sketch();
    let p = s.add_point(3.0, 4.0).unwrap();
    let r = s.solve();
    assert_eq!(s.under_items(&r), vec![CheckItem::Point(p)]);
    assert_eq!(s.status(&r), SketchStatus::Under);
}

#[test]
fn fixed_rect_is_determined_and_locked() {
    let mut s = sketch();
    let p = [
        s.add_point(0.0, 0.0).unwrap(),
        s.add_point(40.0, 0.0).unwrap(),
        s.add_point(40.0, 30.0).unwrap(),
        s.add_point(0.0, 30.0).unwrap(),
    ];
    let l: Vec<_> = (0..4)
        .map(|i| s.add_line(p[i], p[(i + 1) % 4]).unwrap())
        .collect();
    s.add_constraint(K::Horizontal(LineOrPoints::Line(l[0])))
        .unwrap();
    s.add_constraint(K::Vertical(LineOrPoints::Line(l[1])))
        .unwrap();
    s.add_constraint(K::Horizontal(LineOrPoints::Line(l[2])))
        .unwrap();
    s.add_constraint(K::Vertical(LineOrPoints::Line(l[3])))
        .unwrap();
    s.add_constraint(K::Distance(p[0], p[1], 40.0)).unwrap();
    s.add_constraint(K::Distance(p[1], p[2], 30.0)).unwrap();
    s.add_constraint(K::Fixed(p[0], 0.0, 0.0)).unwrap();
    let r = s.solve();
    assert_eq!(s.status(&r), SketchStatus::Determined, "{r:?}");
    assert!(s.under_items(&r).is_empty());
    assert!(s.is_locked(&r));
}

#[test]
fn empty_sketch_not_locked() {
    let mut s = sketch();
    let r = s.solve();
    assert_eq!(s.status(&r), SketchStatus::Determined);
    assert!(!s.is_locked(&r));
}

#[test]
fn projected_curve_never_under() {
    let mut s = sketch();
    let line = Curve::Line {
        origin: DVec3::new(1.0, 2.0, 0.0),
        dir: DVec3::X,
    };
    s.add_projected_curve(
        &line,
        0.0,
        10.0,
        ProjectStyle {
            construction: false,
        },
    )
    .unwrap();
    let r = s.solve();
    assert_eq!(s.status(&r), SketchStatus::Determined, "{r:?}");
    assert!(s.under_items(&r).is_empty());
}

#[test]
fn conflict_status_and_only_solved_free() {
    let mut s = sketch();
    let a = s.add_point(0.0, 0.0).unwrap();
    let b = s.add_point(10.0, 0.0).unwrap();
    s.add_line(a, b).unwrap();
    s.add_constraint(K::Distance(a, b, 10.0)).unwrap();
    s.add_constraint(K::Distance(a, b, 20.0)).unwrap();
    let c = s.add_point(50.0, 50.0).unwrap();
    let d = s.add_point(60.0, 55.0).unwrap();
    let free = s.add_line(c, d).unwrap();
    let r = s.solve();
    assert_eq!(s.status(&r), SketchStatus::Conflict, "{r:?}");
    assert_eq!(s.under_items(&r), vec![CheckItem::Curve(free)]);
    assert!(!s.is_locked(&r));
}

#[test]
fn not_converged_status() {
    let mut s = sketch();
    let a = s.add_point(0.0, 0.0).unwrap();
    let b = s.add_point(5.0, 0.0).unwrap();
    let c = s.add_point(9.0, 3.0).unwrap();
    s.add_constraint(K::Fixed(a, 0.0, 0.0)).unwrap();
    s.add_constraint(K::Fixed(b, 5.0, 0.0)).unwrap();
    s.add_line(b, c).unwrap();
    s.set_point_xy(c, 5.0, 0.0).unwrap();
    let r = s.solve();
    assert_eq!(s.status(&r), SketchStatus::NotSolved, "{r:?}");
    assert!(!s.is_locked(&r));
}

#[test]
fn text_counts_as_one_item() {
    let mut s = sketch();
    let p = TextParams {
        content: "Ab".into(),
        style: TextStyle {
            font: "Test".into(),
            bold: false,
            italic: false,
            spacing: 0.0,
        },
        height: 5.0,
        flip_h: false,
        flip_v: false,
    };
    let shape = Arc::new(BoxShaper.shape(&p.content, &p.style).unwrap());
    let t = s
        .add_text_box(
            DVec2::new(0.0, 0.0),
            DVec2::new(40.0, 10.0),
            p,
            HAlign::Left,
            VAlign::Bottom,
            shape,
        )
        .unwrap();
    let r = s.solve();
    assert_eq!(s.under_items(&r), vec![CheckItem::Text(t)], "{r:?}");
    assert_eq!(s.status(&r), SketchStatus::Under);
}

#[test]
fn ordinal_counts_per_kind() {
    let mut s = sketch();
    let mut lines = Vec::new();
    for i in 0..3 {
        let a = s.add_point(0.0, f64::from(i)).unwrap();
        let b = s.add_point(5.0, f64::from(i)).unwrap();
        lines.push(s.add_line(a, b).unwrap());
    }
    assert_eq!(s.item_ordinal(CheckItem::Curve(lines[2])), 3);
    s.remove_curve(lines[1]).unwrap();
    assert_eq!(s.item_ordinal(CheckItem::Curve(lines[0])), 1);
    assert_eq!(s.item_ordinal(CheckItem::Curve(lines[2])), 2);
}

#[test]
fn second_solve_comes_from_the_cache() {
    // compute_sketch löst mit Formeln ein zweites Mal, nur um den Bericht zu bekommen.
    let mut s = sketch();
    let a = s.add_point(0.0, 0.0).unwrap();
    let b = s.add_point(10.0, 0.0).unwrap();
    s.add_line(a, b).unwrap();
    s.add_constraint(K::Distance(a, b, 10.0)).unwrap();
    let first = s.solve();
    assert!(first.group_solves > 0, "{first:?}");
    let second = s.solve();
    assert_eq!(second.group_solves, 0, "{second:?}");
    assert_eq!(s.status(&first), s.status(&second));
}

#[test]
fn ordinal_counts_only_lone_points() {
    // Ruling T16-1: Kurvenenden zählen bei den Punkten nicht mit.
    let mut s = sketch();
    let a = s.add_point(0.0, 0.0).unwrap();
    let b = s.add_point(5.0, 0.0).unwrap();
    s.add_line(a, b).unwrap();
    let c = s.add_point(0.0, 3.0).unwrap();
    let d = s.add_point(5.0, 3.0).unwrap();
    s.add_line(c, d).unwrap();
    let lone = s.add_point(9.0, 9.0).unwrap();
    let lone2 = s.add_point(8.0, 9.0).unwrap();
    assert_eq!(s.item_ordinal(CheckItem::Point(lone)), 1);
    assert_eq!(s.item_ordinal(CheckItem::Point(lone2)), 2);
}

fn two_lines() -> Sketch {
    let mut s = sketch();
    let a = s.add_point(0.0, 0.0).unwrap();
    let b = s.add_point(10.0, 1.0).unwrap();
    let c = s.add_point(0.0, 5.0).unwrap();
    s.add_line(a, b).unwrap();
    s.add_line(b, c).unwrap();
    s.add_constraint(K::Fixed(a, 0.0, 0.0)).unwrap();
    s.add_constraint(K::Distance(a, b, 10.0)).unwrap();
    s
}

#[test]
fn recompute_all_equals_fresh_solve() {
    let mut s = two_lines();
    let _ = s.solve();
    let mut fresh = two_lines();
    let want = fresh.solve();
    let got = s.recompute_all();
    assert_eq!(got.status, want.status);
    assert_eq!(got.dof, want.dof);
    assert_eq!(got.free_points, want.free_points);
    assert_eq!(got.free_curves, want.free_curves);
    assert_eq!(got.redundant, want.redundant);
    assert_eq!(got.conflicting, want.conflicting);
    // Der Cache war leer: es wurde wirklich gerechnet.
    assert!(got.group_solves > 0, "{got:?}");
}

#[test]
fn recompute_all_unchanged_values_bitwise() {
    let mut s = two_lines();
    let _ = s.solve();
    let before: Vec<(f64, f64)> = s
        .points()
        .map(|(p, _)| s.point_xy(p).map(|q| (q.x, q.y)).unwrap())
        .collect();
    let _ = s.recompute_all();
    let after: Vec<(f64, f64)> = s
        .points()
        .map(|(p, _)| s.point_xy(p).map(|q| (q.x, q.y)).unwrap())
        .collect();
    assert_eq!(before.len(), after.len());
    for (b, a) in before.iter().zip(&after) {
        assert_eq!(b.0.to_bits(), a.0.to_bits());
        assert_eq!(b.1.to_bits(), a.1.to_bits());
    }
}

#[test]
fn item_ordinals_equal_the_single_ones() {
    let mut s = sketch();
    let a = s.add_point(0.0, 0.0).unwrap();
    let b = s.add_point(5.0, 1.0).unwrap();
    let l1 = s.add_line(a, b).unwrap();
    let m = s.add_point(0.0, 9.0).unwrap();
    let c = s.add_point(3.0, 9.0).unwrap();
    let d = s.add_point(0.0, 12.0).unwrap();
    let arc = s.add_arc(m, c, d).unwrap();
    let e = s.add_point(7.0, 7.0).unwrap();
    let f = s.add_point(9.0, 7.0).unwrap();
    let l2 = s.add_line(e, f).unwrap();
    let lone = s.add_point(20.0, 20.0).unwrap();
    let items = [
        CheckItem::Curve(l2),
        CheckItem::Curve(arc),
        CheckItem::Curve(l1),
        CheckItem::Point(lone),
    ];
    let want: Vec<usize> = items.iter().map(|&i| s.item_ordinal(i)).collect();
    assert_eq!(s.item_ordinals(&items), want);
    assert_eq!(want, [2, 1, 1, 1]);
}
