//! Text im Rahmen (Skizzen 8, Task 3): Rahmen aus vier Konstruktionslinien, erzeugte Kurven mit
//! festen Ids, Lage im Rahmen, Neusetzen im Löser, Löschen nur als Ganzes, Konstruktion für den
//! ganzen Text, Zustand „noch nicht gesetzt“ nach dem Laden.
//!
//! Der Test-Setzer [`BoxShaper`] liefert für jedes Zeichen ein Quadrat mit Loch, so prüft die
//! Skizze ohne `planora-text` und ohne Schriften.

use planora_i18n::tr;
use planora_math::{DVec2, Frame, Tolerance};
use planora_sketch::SketchError;
use planora_sketch::constraint::{ConstraintKind as K, LineOrPoints};
use planora_sketch::model::{CurveId, CurveKind, PointId, Sketch};
use planora_sketch::solver::{Drag, SolveReport, SolveStatus};
use planora_sketch::text::{HAlign, TextId, TextParams, TextPlacement, VAlign};
use planora_sketch::text_shape::{
    Contour, Glyph, MAX_TEXT_CHARS, MAX_TEXT_CURVES, Seg, ShapedLine, TextShape, TextShapeError,
    TextShaper, TextStyle,
};
use std::collections::HashSet;
use std::sync::Arc;

/// Vorschub je Zeichen des Test-Setzers (Einheiten der Höhe).
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

fn style() -> TextStyle {
    TextStyle {
        font: "Test".into(),
        bold: false,
        italic: false,
        spacing: 0.0,
    }
}

fn params(content: &str) -> TextParams {
    TextParams {
        content: content.into(),
        style: style(),
        height: 5.0,
        flip_h: false,
        flip_v: false,
    }
}

fn shape_of(p: &TextParams) -> Arc<TextShape> {
    Arc::new(BoxShaper.shape(&p.content, &p.style).unwrap())
}

/// Text im Rahmen (0, 0)–(40, 10).
fn text_in(s: &mut Sketch, p: TextParams, h: HAlign, va: VAlign) -> TextId {
    let shape = shape_of(&p);
    s.add_text_box(v(0.0, 0.0), v(40.0, 10.0), p, h, va, shape)
        .unwrap()
}

fn text(s: &mut Sketch, content: &str) -> TextId {
    text_in(s, params(content), HAlign::Left, VAlign::Bottom)
}

fn frame(s: &Sketch, t: TextId) -> ([CurveId; 4], [PointId; 4]) {
    match s.text(t).unwrap().placement {
        TextPlacement::Box { lines, corners, .. } => (lines, corners),
        TextPlacement::Path { .. } => panic!("kein Rahmen"),
    }
}

fn at(s: &Sketch, p: PointId) -> DVec2 {
    s.point_xy(p).unwrap()
}

fn glyph_curves(s: &Sketch, t: TextId) -> Vec<CurveId> {
    s.text(t).unwrap().curves.clone()
}

fn glyph_points(s: &Sketch, t: TextId) -> Vec<PointId> {
    s.text(t).unwrap().points.clone()
}

fn positions(s: &Sketch, t: TextId) -> Vec<DVec2> {
    glyph_points(s, t).iter().map(|&p| at(s, p)).collect()
}

fn kinds(s: &Sketch, t: TextId) -> Vec<CurveKind> {
    glyph_curves(s, t)
        .iter()
        .map(|&c| s.curve(c).unwrap().kind.clone())
        .collect()
}

/// Hüllquader der Glyphpunkte (die Kontrollpunkte liegen beim Test-Setzer innen).
fn bbox(points: &[DVec2]) -> (DVec2, DVec2) {
    let mut min = DVec2::splat(f64::INFINITY);
    let mut max = DVec2::splat(f64::NEG_INFINITY);
    for p in points {
        min = min.min(*p);
        max = max.max(*p);
    }
    (min, max)
}

fn close(a: DVec2, b: DVec2) -> bool {
    a.distance(b) <= 1e-9
}

fn solved(s: &mut Sketch) -> SolveReport {
    let r = s.solve();
    assert_eq!(r.status, SolveStatus::Solved, "{r:?}");
    r
}

fn says<T: std::fmt::Debug>(r: Result<T, SketchError>, msg: String, what: &str) {
    let e = r.expect_err(what).to_string();
    assert!(e.contains(&msg), "{what}: {e}");
}

#[test]
fn box_text_makes_curves_and_frame() {
    let mut s = sketch();
    let t = text(&mut s, "ab");
    let (lines, corners) = frame(&s, t);
    let want = [v(0.0, 0.0), v(40.0, 0.0), v(40.0, 10.0), v(0.0, 10.0)];
    for (k, &p) in corners.iter().enumerate() {
        assert!(close(at(&s, p), want[k]), "Ecke {k}: {:?}", at(&s, p));
    }
    for (k, &l) in lines.iter().enumerate() {
        let d = s.curve(l).unwrap();
        assert!(d.construction, "Rahmenlinie {k} ist Konstruktion");
        assert_eq!(
            d.kind,
            CurveKind::Line {
                a: corners[k],
                b: corners[(k + 1) % 4]
            }
        );
        assert_eq!(s.text_of_frame(l), Some(t));
        assert_eq!(s.text_of_curve(l), None);
    }
    let curves = glyph_curves(&s, t);
    assert_eq!(curves.len(), 16, "2 Glyphen zu je 8 Kurven");
    assert_eq!(glyph_points(&s, t).len(), 22, "2 Glyphen zu je 11 Punkten");
    let (mut lines_n, mut conics, mut splines) = (0, 0, 0);
    for &c in &curves {
        let d = s.curve(c).unwrap();
        assert!(!d.construction, "erzeugte Kurven bilden Profile");
        assert_eq!(s.text_of_curve(c), Some(t));
        assert_eq!(s.text_of_frame(c), None);
        match &d.kind {
            CurveKind::Line { .. } => lines_n += 1,
            CurveKind::Conic { rho, .. } => {
                assert_eq!(*rho, 0.5);
                conics += 1;
            }
            CurveKind::ControlSpline {
                points,
                degree,
                knots,
            } => {
                assert_eq!(points.len(), 4);
                assert_eq!(*degree, 3);
                assert_eq!(knots, &vec![0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0]);
                splines += 1;
            }
            other => panic!("unerwartete Art {other:?}"),
        }
    }
    assert_eq!((lines_n, conics, splines), (12, 2, 2));
    // Eine Kontur teilt die Endpunkte, der letzte ist der erste.
    let first = s.curve(curves[0]).unwrap().kind.clone();
    let fourth = s.curve(curves[3]).unwrap().kind.clone();
    let (CurveKind::Line { a: a0, b: b0 }, CurveKind::Line { a: a3, b: b3 }) = (first, fourth)
    else {
        panic!("Außenkontur aus Strecken");
    };
    assert_eq!(b3, a0, "die Kontur schließt am ersten Punkt");
    let CurveKind::Line { a: a1, .. } = s.curve(curves[1]).unwrap().kind else {
        panic!("Strecke");
    };
    assert_eq!(a1, b0, "Nachbarn teilen den Punkt");
    assert_ne!(a3, a0);
    // Die Glyphpunkte sind keine Freiheitsgrade, der Rahmen hat 5 minus `Horizontal`.
    assert_eq!(s.constraints().count(), 0, "nur interne Abhängigkeiten");
    assert_eq!(s.internal_constraints().count(), 4);
    let r = solved(&mut s);
    assert_eq!(r.dof, 4, "{r:?}");
    assert!(
        r.free_points
            .iter()
            .all(|p| !glyph_points(&s, t).contains(p)),
        "Glyphpunkte sind nicht frei"
    );
    assert_eq!(s.texts().count(), 1);
    assert!(s.is_text_shaped(t));
}

#[test]
fn box_text_profiles_have_holes() {
    let mut s = sketch();
    text(&mut s, "ab");
    let e = s.profiles();
    assert!(e.rejected.is_empty(), "{:?}", e.rejected);
    let with_holes: Vec<_> = e.profiles.iter().filter(|p| !p.holes.is_empty()).collect();
    assert_eq!(with_holes.len(), 2, "je Buchstabe ein Profil mit Loch");
    assert!(with_holes.iter().all(|p| p.holes.len() == 1));
    // Dazu die Lochflächen selbst, wie Ring und Kreisfläche (Skizzen 1).
    assert_eq!(e.profiles.len(), 4);
}

#[test]
fn moving_frame_moves_glyphs_keeps_ids() {
    let mut s = sketch();
    let t = text(&mut s, "ab");
    solved(&mut s);
    let (_, corners) = frame(&s, t);
    let (curves, points, kinds_before, before) = (
        glyph_curves(&s, t),
        glyph_points(&s, t),
        kinds(&s, t),
        positions(&s, t),
    );
    let c0 = at(&s, corners[0]);
    s.drag(&[Drag {
        point: corners[0],
        target: v(-5.0, -3.0),
    }])
    .unwrap();
    solved(&mut s);
    let delta = at(&s, corners[0]) - c0;
    assert!(delta.length() > 1.0, "die Ecke hat sich bewegt: {delta:?}");
    assert_eq!(glyph_curves(&s, t), curves);
    assert_eq!(glyph_points(&s, t), points);
    assert_eq!(kinds(&s, t), kinds_before);
    for (old, new) in before.iter().zip(positions(&s, t)) {
        assert!(close(new, *old + delta), "{old:?} + {delta:?} ≠ {new:?}");
    }
}

#[test]
fn height_change_keeps_ids() {
    let mut s = sketch();
    let t = text(&mut s, "ab");
    let (curves, points, before) = (glyph_curves(&s, t), glyph_points(&s, t), positions(&s, t));
    let mut p = params("ab");
    p.height = 8.0;
    let shape = shape_of(&p);
    s.edit_text_box(t, p, HAlign::Left, VAlign::Bottom, shape)
        .unwrap();
    assert_eq!(glyph_curves(&s, t), curves);
    assert_eq!(glyph_points(&s, t), points);
    assert_eq!(s.text(t).unwrap().height, 8.0);
    // Links unten wächst der Text um Ecke 0.
    for (old, new) in before.iter().zip(positions(&s, t)) {
        assert!(close(new, *old * (8.0 / 5.0)), "{old:?} → {new:?}");
    }
}

#[test]
fn alignment_change_keeps_ids() {
    let mut s = sketch();
    let t = text(&mut s, "ab");
    let (curves, points, before) = (glyph_curves(&s, t), glyph_points(&s, t), positions(&s, t));
    let shape = shape_of(&params("ab"));
    s.edit_text_box(t, params("ab"), HAlign::Right, VAlign::Top, shape)
        .unwrap();
    assert_eq!(glyph_curves(&s, t), curves);
    assert_eq!(glyph_points(&s, t), points);
    assert_ne!(positions(&s, t), before);
    let TextPlacement::Box { h, v: va, .. } = s.text(t).unwrap().placement else {
        panic!("Rahmen");
    };
    assert_eq!((h, va), (HAlign::Right, VAlign::Top));
}

#[test]
fn content_change_reuses_ids_in_order() {
    let mut s = sketch();
    let t = text(&mut s, "ab");
    let (curves, points, before) = (glyph_curves(&s, t), glyph_points(&s, t), positions(&s, t));
    let p = params("abc");
    let shape = shape_of(&p);
    s.edit_text_box(t, p, HAlign::Left, VAlign::Bottom, shape)
        .unwrap();
    let (now_curves, now_points) = (glyph_curves(&s, t), glyph_points(&s, t));
    assert_eq!(now_curves.len(), 24);
    assert_eq!(now_points.len(), 33);
    assert_eq!(&now_curves[..16], &curves[..], "die ersten Ids bleiben");
    assert_eq!(&now_points[..22], &points[..]);
    let old: HashSet<CurveId> = curves.iter().copied().collect();
    assert!(
        now_curves[16..].iter().all(|c| !old.contains(c)),
        "neue Ids kommen dazu"
    );
    // Links bündig: die ersten beiden Glyphen stehen, wo sie standen.
    assert_eq!(&positions(&s, t)[..22], &before[..]);
    assert_eq!(s.text(t).unwrap().content, "abc");
    // Und zurück: Überzählige werden gelöscht.
    let p = params("ab");
    let shape = shape_of(&p);
    s.edit_text_box(t, p, HAlign::Left, VAlign::Bottom, shape)
        .unwrap();
    assert_eq!(glyph_curves(&s, t), curves);
    assert_eq!(glyph_points(&s, t), points);
    for &c in &now_curves[16..] {
        assert!(s.curve(c).is_none(), "überzählige Kurve gelöscht");
    }
    for &p in &now_points[22..] {
        assert!(s.point_xy(p).is_err(), "überzähliger Punkt gelöscht");
    }
}

#[test]
fn kind_change_replaces_only_that_curve() {
    let mut s = sketch();
    let t = text(&mut s, "a");
    let curves = glyph_curves(&s, t);
    // Dieselbe Glyphe, aber die quadratische Kurve des Lochs ist eine Strecke.
    let mut shape = (*shape_of(&params("a"))).clone();
    let hole = &mut shape.lines[0].glyphs[0].contours[1];
    hole.segs[1] = Seg::Line(v(0.4, 0.5));
    s.edit_text_box(
        t,
        params("a"),
        HAlign::Left,
        VAlign::Bottom,
        Arc::new(shape),
    )
    .unwrap();
    let now = glyph_curves(&s, t);
    assert_eq!(now.len(), 8);
    for k in (0..8).filter(|&k| k != 5) {
        assert_eq!(now[k], curves[k], "Kurve {k} behält ihre Id");
    }
    assert_ne!(now[5], curves[5], "die Art passt nicht: neue Kurve");
    assert!(s.curve(curves[5]).is_none());
    assert!(matches!(
        s.curve(now[5]).unwrap().kind,
        CurveKind::Line { .. }
    ));
    assert_eq!(glyph_points(&s, t).len(), 10);
}

#[test]
fn glyph_points_are_solver_constants() {
    let mut s = sketch();
    let t = text(&mut s, "ab");
    solved(&mut s);
    let (_, corners) = frame(&s, t);
    let (curves, points) = (glyph_curves(&s, t), glyph_points(&s, t));
    let p = points[0];
    let rev = s.revision();
    let key = || tr!("sketch-text-no-dimension");
    let xy = at(&s, p);
    says(
        s.add_constraint(K::Fixed(p, xy.x, xy.y)),
        key(),
        "Fixed an der Glyphe",
    );
    says(
        s.add_constraint(K::Distance(points[0], points[1], 3.0)),
        key(),
        "Maß zwischen Glyphpunkten",
    );
    says(
        s.add_constraint(K::Coincident(corners[0], p)),
        key(),
        "Ecke auf Glyphpunkt",
    );
    says(
        s.add_constraint(K::PointOnCurve(corners[2], curves[0])),
        key(),
        "Ecke auf Textkurve",
    );
    says(
        s.add_constraint(K::Horizontal(LineOrPoints::Line(curves[0]))),
        key(),
        "Waagerecht an der Textkurve",
    );
    assert_eq!(s.revision(), rev, "Ablehnungen hinterlassen nichts");
    // Rahmenlinien und Ecken nehmen Abhängigkeiten wie Linien und Punkte.
    s.add_constraint(K::Fixed(corners[0], 0.0, 0.0)).unwrap();
    s.add_constraint(K::Distance(corners[0], corners[1], 50.0))
        .unwrap();
    let r = solved(&mut s);
    assert_eq!(r.dof, 1, "nur die Höhe des Rahmens ist frei: {r:?}");
    // Der Löser bewegt Glyphpunkte nie selbst; sie folgen nur dem Rahmen.
    assert!(close(at(&s, corners[1]), v(50.0, 0.0)));
    let left = bbox(&positions(&s, t)).0;
    assert!(close(left, v(0.0, -DESC * 5.0)), "{left:?}");
}

#[test]
fn align_right_top() {
    let mut s = sketch();
    let t = text_in(&mut s, params("ab"), HAlign::Right, VAlign::Top);
    let h = 5.0;
    let (min, max) = bbox(&positions(&s, t));
    // Die Zeile (Vorschub bis hinter die letzte Glyphe) endet an Ecke 1 (x = 40); die Glyphe des
    // Test-Setzers hat rechts `ADV − INK_RIGHT` Luft.
    assert!(
        (max.x - (40.0 - (ADV - INK_RIGHT) * h)).abs() < 1e-9,
        "{max:?}"
    );
    assert!((min.x - (40.0 - 2.0 * ADV * h)).abs() < 1e-9, "{min:?}");
    // Die Grundlinie liegt `ascender · h` unter der Gegenlinie (y = 10).
    let baseline = 10.0 - ASC * h;
    assert!((min.y - baseline).abs() < 1e-9, "{min:?}");
    assert!((max.y - (baseline + INK_TOP * h)).abs() < 1e-9, "{max:?}");
}

#[test]
fn align_center_middle_and_lines() {
    let mut s = sketch();
    let t = text_in(&mut s, params("ab\nab"), HAlign::Center, VAlign::Middle);
    let h = 5.0;
    let (min, max) = bbox(&positions(&s, t));
    // Waagerecht mittig: die Zeile 2 · ADV breit um x = 20.
    assert!((min.x - (20.0 - ADV * h)).abs() < 1e-9, "{min:?}");
    // Senkrecht: die Mitte zwischen Oberlänge der ersten und Unterlänge der letzten Zeile liegt
    // auf der halben Höhe des Rahmens.
    let top_rel = ASC * h;
    let bottom_rel = -LINE_HEIGHT * h + DESC * h;
    let first_baseline = 5.0 - 0.5 * (top_rel + bottom_rel);
    assert!(
        (max.y - (first_baseline + INK_TOP * h)).abs() < 1e-9,
        "{max:?}"
    );
    assert!(
        (min.y - (first_baseline - LINE_HEIGHT * h)).abs() < 1e-9,
        "{min:?}"
    );
    // Links unten: Unterlänge der letzten Zeile auf der Grundlinie.
    let mut s = sketch();
    let t = text(&mut s, "a\na");
    let (min, _) = bbox(&positions(&s, t));
    assert!(close(min, v(0.0, -DESC * h)), "{min:?}");
    let first = positions(&s, t)[0];
    assert!(
        close(first, v(0.0, -DESC * h + LINE_HEIGHT * h)),
        "{first:?}"
    );
}

#[test]
fn flip_h_mirrors_about_frame_axis() {
    let mut s = sketch();
    let t = text(&mut s, "ab");
    let mut flipped = sketch();
    let mut p = params("ab");
    p.flip_h = true;
    let f = text_in(&mut flipped, p, HAlign::Left, VAlign::Bottom);
    for (a, b) in positions(&s, t).iter().zip(positions(&flipped, f)) {
        assert!(close(b, v(40.0 - a.x, a.y)), "{a:?} ↔ {b:?}");
    }
    let mut flipped = sketch();
    let mut p = params("ab");
    p.flip_v = true;
    let f = text_in(&mut flipped, p, HAlign::Left, VAlign::Bottom);
    for (a, b) in positions(&s, t).iter().zip(positions(&flipped, f)) {
        assert!(close(b, v(a.x, 10.0 - a.y)), "{a:?} ↔ {b:?}");
    }
}

#[test]
fn flip_follows_a_rotated_frame() {
    // Umkehren spiegelt an der Mittelachse des Rahmens, auch wenn er gedreht ist.
    let mut s = sketch();
    let t = text(&mut s, "ab");
    let (_, corners) = frame(&s, t);
    // Das interne Waagerecht hält die Grundlinie; ein gedrehter Rahmen entsteht hier über die
    // Ecken (wie nach dem Laden einer Datei mit gedrehtem Rahmen) und einen Lauf ohne Löser.
    let angle: f64 = 0.5;
    let (sin, cos) = angle.sin_cos();
    let rot = |p: DVec2| v(p.x * cos - p.y * sin, p.x * sin + p.y * cos);
    let want = [v(0.0, 0.0), v(40.0, 0.0), v(40.0, 10.0), v(0.0, 10.0)];
    for (k, &c) in corners.iter().enumerate() {
        let p = rot(want[k]);
        s.set_point_xy(c, p.x, p.y).unwrap();
    }
    let shape = shape_of(&params("ab"));
    s.edit_text_box(t, params("ab"), HAlign::Left, VAlign::Bottom, shape)
        .unwrap();
    let straight: Vec<DVec2> = {
        let mut u = sketch();
        let w = text(&mut u, "ab");
        positions(&u, w)
    };
    for (a, b) in straight.iter().zip(positions(&s, t)) {
        assert!(close(b, rot(*a)), "{a:?} gedreht ≠ {b:?}");
    }
    let mut p = params("ab");
    p.flip_h = true;
    let shape = shape_of(&p);
    s.edit_text_box(t, p, HAlign::Left, VAlign::Bottom, shape)
        .unwrap();
    for (a, b) in straight.iter().zip(positions(&s, t)) {
        assert!(close(b, rot(v(40.0 - a.x, a.y))), "{a:?} ↔ {b:?}");
    }
}

#[test]
fn remove_frame_line_rejected() {
    let mut s = sketch();
    let t = text(&mut s, "ab");
    let (lines, corners) = frame(&s, t);
    let rev = s.revision();
    for l in lines {
        says(s.remove_curve(l), tr!("sketch-text-part"), "Rahmenlinie");
    }
    for c in corners {
        says(s.remove_point(c), tr!("sketch-text-part"), "Ecke");
    }
    assert_eq!(s.revision(), rev);
    assert!(s.text(t).is_some());
    assert!(lines.iter().all(|&l| s.curve(l).is_some()));
}

#[test]
fn remove_glyph_rejected() {
    let mut s = sketch();
    let t = text(&mut s, "ab");
    let rev = s.revision();
    for &c in &glyph_curves(&s, t) {
        says(s.remove_curve(c), tr!("sketch-text-part"), "Textkurve");
    }
    for &p in &glyph_points(&s, t) {
        says(s.remove_point(p), tr!("sketch-text-part"), "Glyphpunkt");
    }
    assert_eq!(s.revision(), rev);
    assert_eq!(s.profiles().profiles.len(), 4);
}

#[test]
fn remove_text_removes_all() {
    let mut s = sketch();
    let t = text(&mut s, "ab");
    let (lines, corners) = frame(&s, t);
    let q = s.add_point(60.0, 30.0).unwrap();
    let r = s.add_point(70.0, 30.0).unwrap();
    let other = s.add_line(q, r).unwrap();
    s.add_constraint(K::Fixed(corners[0], 0.0, 0.0)).unwrap();
    s.add_constraint(K::Distance(corners[2], q, 30.0)).unwrap();
    s.add_constraint(K::Parallel(lines[2], other)).unwrap();
    let (curves, points) = (glyph_curves(&s, t), glyph_points(&s, t));
    s.remove_text(t).unwrap();
    assert!(s.text(t).is_none());
    assert_eq!(s.texts().count(), 0);
    for c in curves.iter().chain(&lines) {
        assert!(s.curve(*c).is_none(), "Kurve {c:?} lebt noch");
    }
    for p in points.iter().chain(&corners) {
        assert!(s.point_xy(*p).is_err(), "Punkt {p:?} lebt noch");
    }
    let gone: HashSet<CurveId> = curves.iter().chain(&lines).copied().collect();
    let dead: HashSet<PointId> = points.iter().chain(&corners).copied().collect();
    for (_, k) in s.constraints().chain(s.internal_constraints()) {
        assert!(
            k.kind.curves().iter().all(|c| !gone.contains(c))
                && k.kind.points().iter().all(|p| !dead.contains(p)),
            "Abhängigkeit am Text lebt noch: {:?}",
            k.kind
        );
    }
    assert_eq!(s.constraints().count(), 0);
    assert_eq!(s.internal_constraints().count(), 0);
    assert!(s.curve(other).is_some(), "anderes bleibt");
    assert!(s.point_xy(q).is_ok());
    assert!(s.profiles().profiles.is_empty());
    assert_eq!(s.remove_text(t), Err(SketchError::UnknownObject));
}

#[test]
fn construction_toggle_whole_text() {
    let mut s = sketch();
    let t = text(&mut s, "ab");
    let (lines, _) = frame(&s, t);
    s.set_text_construction(t, true);
    assert!(
        glyph_curves(&s, t)
            .iter()
            .all(|&c| s.curve(c).unwrap().construction)
    );
    assert!(s.profiles().profiles.is_empty());
    s.set_text_construction(t, false);
    assert_eq!(s.profiles().profiles.len(), 4);
    assert!(lines.iter().all(|&l| s.curve(l).unwrap().construction));
    // Umschalten an einer einzelnen Textkurve gilt für den ganzen Text (B10).
    let one = glyph_curves(&s, t)[3];
    s.set_construction(one, true).unwrap();
    assert!(
        glyph_curves(&s, t)
            .iter()
            .all(|&c| s.curve(c).unwrap().construction)
    );
    assert!(s.profiles().profiles.is_empty());
    // Neue Kurven beim Neusetzen nehmen den Stil des Textes.
    let p = params("abc");
    let shape = shape_of(&p);
    s.edit_text_box(t, p, HAlign::Left, VAlign::Bottom, shape)
        .unwrap();
    assert!(
        glyph_curves(&s, t)
            .iter()
            .all(|&c| s.curve(c).unwrap().construction)
    );
    // Mittellinie wird abgelehnt.
    let line = glyph_curves(&s, t)[0];
    let rev = s.revision();
    says(
        s.set_centerline(line, true),
        tr!("sketch-text-no-centerline"),
        "Mittellinie an Textkurve",
    );
    assert_eq!(s.revision(), rev);
}

#[test]
fn unshaped_text_keeps_curves() {
    let mut s = sketch();
    let t = text(&mut s, "ab");
    solved(&mut s);
    let mut loaded = Sketch::from_data(s.to_data()).unwrap();
    assert!(s.is_text_shaped(t));
    assert!(
        !loaded.is_text_shaped(t),
        "nach dem Laden noch nicht gesetzt"
    );
    assert_eq!(glyph_curves(&loaded, t), glyph_curves(&s, t));
    let before = positions(&loaded, t);
    let (_, corners) = frame(&loaded, t);
    loaded
        .drag(&[Drag {
            point: corners[0],
            target: v(-5.0, -3.0),
        }])
        .unwrap();
    let r = solved(&mut loaded);
    assert_eq!(
        r.dof, 4,
        "auch ungesetzt sind Glyphpunkte keine Freiheitsgrade"
    );
    let delta = at(&loaded, corners[0]);
    assert!(delta.length() > 1.0);
    let after = positions(&loaded, t);
    for (a, b) in before.iter().zip(&after) {
        assert_eq!(
            a.to_array().map(f64::to_bits),
            b.to_array().map(f64::to_bits)
        );
    }
    // Erst das Neusetzen bringt die Kurven an den Rahmen, mit gleichen Ids.
    let (curves, points) = (glyph_curves(&loaded, t), glyph_points(&loaded, t));
    let report = loaded.reshape_texts(&BoxShaper);
    assert!(report.changed);
    assert!(loaded.is_text_shaped(t));
    assert_eq!(glyph_curves(&loaded, t), curves);
    assert_eq!(glyph_points(&loaded, t), points);
    for (a, b) in before.iter().zip(positions(&loaded, t)) {
        assert!(close(b, *a + delta), "{a:?} + {delta:?} ≠ {b:?}");
    }
}

#[test]
fn reshape_same_shape_reports_unchanged() {
    let mut s = sketch();
    let t = text(&mut s, "ab");
    solved(&mut s);
    let rev = s.revision();
    let report = s.reshape_texts(&BoxShaper);
    assert!(!report.changed, "{report:?}");
    assert_eq!(
        s.revision(),
        rev,
        "kein neuer Stand, kein Rückgängig-Schritt"
    );
    assert!(report.substituted.is_empty() && report.synthetic.is_empty());
    assert!(report.missing_chars.is_empty());
    // Ebenso nach dem Laden: Das erste Setzen liefert dieselben Kurven.
    let mut loaded = Sketch::from_data(s.to_data()).unwrap();
    let rev = loaded.revision();
    let report = loaded.reshape_texts(&BoxShaper);
    assert!(!report.changed, "{report:?}");
    assert_eq!(loaded.revision(), rev);
    assert!(loaded.is_text_shaped(t));
    // Lösen danach ändert auch nichts.
    solved(&mut loaded);
    assert_eq!(loaded.revision(), rev);
}

#[test]
fn reshape_reports_substitute_synthetic_and_changes() {
    let mut s = sketch();
    let mut p = params("ab");
    p.style.font = "Fehlt".into();
    p.style.bold = true;
    let t = text_in(&mut s, p, HAlign::Left, VAlign::Bottom);
    let plain = text(&mut s, "x");
    let report = s.reshape_texts(&BoxShaper);
    assert_eq!(report.substituted, vec![(t, "Fehlt".to_string())]);
    assert_eq!(report.synthetic, vec![t]);
    assert!(report.missing_chars.is_empty());
    assert!(!report.changed);
    // Ein anderer Setzer mit anderer Breite: neue Lage, gleiche Ids.
    struct Wide;
    impl TextShaper for Wide {
        fn shape(&self, content: &str, style: &TextStyle) -> Result<TextShape, TextShapeError> {
            let mut shape = BoxShaper.shape(content, style)?;
            for line in &mut shape.lines {
                for (i, g) in line.glyphs.iter_mut().enumerate() {
                    g.origin.x = i as f64 * 1.5;
                }
            }
            Ok(shape)
        }
    }
    let curves = glyph_curves(&s, plain);
    let before = positions(&s, t);
    let rev = s.revision();
    let report = s.reshape_texts(&Wide);
    assert!(report.changed);
    assert_ne!(s.revision(), rev);
    assert_ne!(positions(&s, t), before);
    assert_eq!(glyph_curves(&s, plain), curves);
}

#[test]
fn text_data_eq_ignores_cache() {
    let mut s = sketch();
    let t = text(&mut s, "ab");
    let loaded = Sketch::from_data(s.to_data()).unwrap();
    assert_ne!(s.is_text_shaped(t), loaded.is_text_shaped(t));
    assert_eq!(s.text(t).unwrap(), loaded.text(t).unwrap());
    assert_eq!(s.to_data(), loaded.to_data());
}

#[test]
fn solving_twice_keeps_the_revision() {
    let mut s = sketch();
    text(&mut s, "ab");
    solved(&mut s);
    let rev = s.revision();
    solved(&mut s);
    assert_eq!(
        s.revision(),
        rev,
        "Neusetzen ohne Änderung ist keine Änderung"
    );
}

#[test]
fn add_text_box_rejects_bad_input_and_leaves_nothing() {
    let mut s = sketch();
    let rev = s.revision();
    let a = v(0.0, 0.0);
    let b = v(40.0, 10.0);
    let ok_shape = shape_of(&params("a"));
    let try_add = |s: &mut Sketch, p: TextParams, shape: Arc<TextShape>| {
        s.add_text_box(a, b, p, HAlign::Left, VAlign::Bottom, shape)
    };
    says(
        try_add(&mut s, params("  \n\t"), ok_shape.clone()),
        tr!("sketch-text-empty"),
        "nur Leerraum",
    );
    // Ein Text, der keine Kurven ergibt (etwa nur Zeichen ohne Umriss).
    let blank = Arc::new(TextShape {
        lines: vec![ShapedLine {
            glyphs: vec![Glyph {
                origin: v(0.0, 0.0),
                advance: ADV,
                contours: Vec::new(),
            }],
            width: ADV,
        }],
        ascender: ASC,
        descender: DESC,
        line_height: LINE_HEIGHT,
        substituted: None,
        synthetic: false,
        missing_chars: true,
    });
    says(
        try_add(&mut s, params("a"), blank),
        tr!("sketch-text-empty"),
        "ohne Kurven",
    );
    let long = "a".repeat(MAX_TEXT_CHARS + 1);
    let e = try_add(&mut s, params(&long), ok_shape.clone())
        .expect_err("zu lang")
        .to_string();
    assert!(
        e.contains(&tr!(
            "sketch-text-too-long",
            max = MAX_TEXT_CHARS.to_string()
        )),
        "{e}"
    );
    let many = {
        let mut shape = (*shape_of(&params("a"))).clone();
        let glyph = shape.lines[0].glyphs[0].clone();
        shape.lines[0].glyphs = vec![glyph; MAX_TEXT_CURVES / 8 + 1];
        Arc::new(shape)
    };
    let e = try_add(&mut s, params("a"), many)
        .expect_err("zu viele Kurven")
        .to_string();
    assert!(
        e.contains(&tr!(
            "sketch-text-too-many-curves",
            max = MAX_TEXT_CURVES.to_string()
        )),
        "{e}"
    );
    for height in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        let mut p = params("a");
        p.height = height;
        says(
            try_add(&mut s, p, ok_shape.clone()),
            tr!("sketch-text-height"),
            "Höhe",
        );
    }
    for spacing in [-1.0, 1000.5, f64::NAN] {
        let mut p = params("a");
        p.style.spacing = spacing;
        says(
            try_add(&mut s, p, ok_shape.clone()),
            tr!("sketch-text-spacing", max = "1000".to_string()),
            "Abstand",
        );
    }
    let flat = s.add_text_box(
        a,
        v(40.0, 0.0),
        params("a"),
        HAlign::Left,
        VAlign::Bottom,
        ok_shape.clone(),
    );
    says(flat, tr!("sketch-err-rect-size"), "Rahmen ohne Höhe");
    let mut bad = (*ok_shape).clone();
    bad.lines[0].glyphs[0].contours[0].start = v(f64::NAN, 0.0);
    says(
        try_add(&mut s, params("a"), Arc::new(bad)),
        tr!("sketch-text-out-of-range"),
        "NaN im Umriss",
    );
    assert_eq!(s.revision(), rev, "nichts angelegt");
    assert_eq!(s.texts().count(), 0);
    assert_eq!(s.curves().count(), 0);
    assert_eq!(s.points().count(), 0);
    // Zwei Ecken in beliebiger Lage ergeben denselben Rahmen (Ecke 0 unten links).
    let t = s
        .add_text_box(b, a, params("a"), HAlign::Left, VAlign::Bottom, ok_shape)
        .unwrap();
    let (_, corners) = frame(&s, t);
    assert!(close(at(&s, corners[0]), a));
    assert!(close(at(&s, corners[2]), b));
}

#[test]
fn edit_text_box_rejects_and_keeps_the_text() {
    let mut s = sketch();
    let t = text(&mut s, "ab");
    let rev = s.revision();
    let (curves, before) = (glyph_curves(&s, t), positions(&s, t));
    let mut p = params("ab");
    p.height = 0.0;
    let shape = shape_of(&params("ab"));
    says(
        s.edit_text_box(t, p, HAlign::Right, VAlign::Top, shape.clone()),
        tr!("sketch-text-height"),
        "Höhe 0",
    );
    says(
        s.edit_text_box(t, params(" "), HAlign::Right, VAlign::Top, shape.clone()),
        tr!("sketch-text-empty"),
        "leer",
    );
    assert_eq!(s.revision(), rev);
    assert_eq!(glyph_curves(&s, t), curves);
    assert_eq!(positions(&s, t), before);
    s.remove_text(t).unwrap();
    assert_eq!(
        s.edit_text_box(t, params("ab"), HAlign::Left, VAlign::Bottom, shape),
        Err(SketchError::UnknownObject)
    );
}
