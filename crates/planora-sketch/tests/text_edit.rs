//! Text ändern und Winkel (Skizzen 8, Task 6, Spec C5 und C8): Ändern mit gleichen Ids, Wechsel
//! Rahmen ↔ Pfad, Drehen um Ecke 0, Ablehnung bei gehaltenem Winkel, jede Ablehnung lässt die
//! Skizze unverändert.
//!
//! Der Test-Setzer [`BoxShaper`] liefert für jedes Zeichen ein Quadrat mit Loch, so prüft die
//! Skizze ohne `planora-text` und ohne Schriften.

use planora_i18n::tr;
use planora_math::{DVec2, Frame, Tolerance};
use planora_sketch::constraint::{ConstraintKind as K, LineOrPoints};
use planora_sketch::model::{CurveId, PointId, Sketch};
use planora_sketch::solver::SolveStatus;
use planora_sketch::text::{
    HAlign, TextId, TextKindEdit, TextParams, TextPlacement, VAlign, path_content,
};
use planora_sketch::text_shape::{
    Contour, Glyph, MAX_TEXT_CHARS, MAX_TEXT_CURVES, Seg, ShapedLine, TextShape, TextShapeError,
    TextShaper, TextStyle,
};
use planora_sketch::{SketchData, SketchError};
use std::f64::consts::PI;
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

/// Gesetzt für den Rahmen.
fn shape_of(p: &TextParams) -> Arc<TextShape> {
    Arc::new(BoxShaper.shape(&p.content, &p.style).unwrap())
}

/// Gesetzt für einen Pfad: Zeilen mit Leerzeichen verbunden.
fn path_shape_of(p: &TextParams) -> Arc<TextShape> {
    Arc::new(
        BoxShaper
            .shape(&path_content(&p.content), &p.style)
            .unwrap(),
    )
}

/// Text im Rahmen (0, 0)–(40, 10), links unten.
fn text(s: &mut Sketch, content: &str) -> TextId {
    let p = params(content);
    let shape = shape_of(&p);
    s.add_text_box(
        v(0.0, 0.0),
        v(40.0, 10.0),
        p,
        HAlign::Left,
        VAlign::Bottom,
        shape,
    )
    .unwrap()
}

fn line(s: &mut Sketch, a: DVec2, b: DVec2) -> CurveId {
    let a = s.add_point(a.x, a.y).unwrap();
    let b = s.add_point(b.x, b.y).unwrap();
    s.add_line(a, b).unwrap()
}

fn on_path(path: CurveId) -> TextKindEdit {
    TextKindEdit::Path {
        path,
        above: true,
        fit: false,
        h: HAlign::Left,
    }
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

/// Drehung um `c` um `angle`.
fn rot_about(c: DVec2, angle: f64) -> impl Fn(DVec2) -> DVec2 {
    let (sin, cos) = angle.sin_cos();
    move |p: DVec2| {
        let r = p - c;
        c + v(r.x * cos - r.y * sin, r.x * sin + r.y * cos)
    }
}

/// Das interne `Horizontal` auf der Grundlinie `l`.
fn has_internal_horizontal(s: &Sketch, l: CurveId) -> bool {
    s.internal_constraints()
        .any(|(_, c)| c.kind == K::Horizontal(LineOrPoints::Line(l)))
}

/// Lebt noch eine Abhängigkeit (auch intern) an einer der Kurven oder Punkte?
fn any_constraint_on(s: &Sketch, curves: &[CurveId], points: &[PointId]) -> bool {
    s.constraints()
        .chain(s.internal_constraints())
        .any(|(_, c)| {
            c.kind.curves().iter().any(|x| curves.contains(x))
                || c.kind.points().iter().any(|p| points.contains(p))
        })
}

fn says<T: std::fmt::Debug>(r: Result<T, SketchError>, msg: String, what: &str) {
    let e = r.expect_err(what).to_string();
    assert!(e.contains(&msg), "{what}: {e}");
}

/// Nach einer Ablehnung: Die Skizze ist wie `before`, beim Stand `rev`.
fn unchanged(s: &Sketch, before: &SketchData, rev: u64, what: &str) {
    assert_eq!(&s.to_data(), before, "{what}: Skizze geändert");
    assert_eq!(s.revision(), rev, "{what}: neuer Stand");
}

#[test]
fn edit_keeps_ids_when_only_style_changes() {
    let mut s = sketch();
    let t = text(&mut s, "ab");
    let (curves, points, before) = (glyph_curves(&s, t), glyph_points(&s, t), positions(&s, t));
    let frame_before = frame(&s, t);
    let mut p = params("ab");
    p.height = 7.0;
    p.style.bold = true;
    p.style.italic = true;
    p.style.spacing = 50.0;
    p.flip_h = true;
    let centered = TextKindEdit::Box {
        h: HAlign::Center,
        v: VAlign::Middle,
    };
    s.edit_text(t, p.clone(), centered, shape_of(&p)).unwrap();
    assert_eq!(glyph_curves(&s, t), curves, "gleiche Kurven");
    assert_eq!(glyph_points(&s, t), points, "gleiche Punkte");
    assert_ne!(positions(&s, t), before, "andere Lage");
    assert_eq!(frame(&s, t), frame_before, "der Rahmen bleibt");
    let d = s.text(t).unwrap();
    assert_eq!(
        (d.height, d.style.clone(), d.flip_h),
        (7.0, p.style.clone(), true)
    );
    assert!(matches!(
        d.placement,
        TextPlacement::Box {
            h: HAlign::Center,
            v: VAlign::Middle,
            ..
        }
    ));
    // Unverändert noch einmal: kein neuer Stand.
    let rev = s.revision();
    s.edit_text(t, p.clone(), centered, shape_of(&p)).unwrap();
    assert_eq!(s.revision(), rev);

    // Auf Pfad: Wechsel nach unterhalb, Anpassen und Höhe behalten die Ids ebenso.
    let path = line(&mut s, v(0.0, 50.0), v(100.0, 50.0));
    let p = params("ab");
    let u = s
        .add_text_path(
            path,
            p.clone(),
            true,
            false,
            HAlign::Left,
            path_shape_of(&p),
        )
        .unwrap();
    let (curves, before) = (glyph_curves(&s, u), positions(&s, u));
    let mut q = p;
    q.height = 3.0;
    let kind = TextKindEdit::Path {
        path,
        above: false,
        fit: true,
        h: HAlign::Center,
    };
    s.edit_text(u, q.clone(), kind, path_shape_of(&q)).unwrap();
    assert_eq!(glyph_curves(&s, u), curves);
    assert_ne!(positions(&s, u), before);
    assert_eq!(
        s.text(u).unwrap().placement,
        TextPlacement::Path {
            path,
            above: false,
            fit: true,
            h: HAlign::Center
        }
    );
    let rev = s.revision();
    s.edit_text(u, q.clone(), kind, path_shape_of(&q)).unwrap();
    assert_eq!(s.revision(), rev, "unverändert auf Pfad: kein neuer Stand");
}

#[test]
fn box_to_path_removes_frame() {
    let mut s = sketch();
    let t = text(&mut s, "a\nb");
    let (lines, corners) = frame(&s, t);
    let curves = glyph_curves(&s, t);
    // Eine Linie mit Fang an Ecke 2 benutzt den Eckpunkt selbst; sie und die Ecke bleiben.
    let q = s.add_point(60.0, 30.0).unwrap();
    let keep_line = s.add_line(corners[2], q).unwrap();
    let r = s.add_point(-20.0, 0.0).unwrap();
    s.add_constraint(K::Distance(corners[0], r, 20.0)).unwrap();
    s.add_constraint(K::Parallel(lines[1], keep_line)).unwrap();
    let gap = at(&s, corners[2]).distance(v(60.0, 30.0));
    let keep = s.add_constraint(K::Distance(corners[2], q, gap)).unwrap();
    let path = line(&mut s, v(0.0, 50.0), v(100.0, 50.0));
    let mut p = params("a\nb");
    p.height = 4.0;
    s.edit_text(t, p.clone(), on_path(path), path_shape_of(&p))
        .unwrap();

    assert_eq!(
        s.text(t).unwrap().placement,
        TextPlacement::Path {
            path,
            above: true,
            fit: false,
            h: HAlign::Left
        }
    );
    assert_eq!(
        s.text(t).unwrap().content,
        "a\nb",
        "der Inhalt bleibt mit Zeilen"
    );
    assert!(
        lines.iter().all(|&l| s.curve(l).is_none()),
        "Rahmenlinien weg"
    );
    for k in [0, 1, 3] {
        assert!(s.point_xy(corners[k]).is_err(), "Ecke {k} weg");
    }
    assert!(s.point_xy(corners[2]).is_ok(), "benutzte Ecke bleibt");
    assert!(s.curve(keep_line).is_some());
    assert!(!any_constraint_on(
        &s,
        &lines,
        &[corners[0], corners[1], corners[3]]
    ));
    assert!(s.constraint(keep).is_some(), "ohne Rahmenlinie bleibt sie");
    assert_eq!(s.constraints().count(), 1);
    assert_eq!(
        s.internal_constraints().count(),
        0,
        "keine Rahmen-Abhängigkeit"
    );
    assert_eq!(glyph_curves(&s, t), curves, "gleiche Glyphen, gleiche Ids");
    // Die Lage wie bei einem neu angelegten Text auf demselben Pfad.
    let mut u = sketch();
    let path_u = line(&mut u, v(0.0, 50.0), v(100.0, 50.0));
    let w = u
        .add_text_path(
            path_u,
            p.clone(),
            true,
            false,
            HAlign::Left,
            path_shape_of(&p),
        )
        .unwrap();
    for (a, b) in positions(&s, t).iter().zip(positions(&u, w)) {
        assert!(close(*a, b), "{a:?} ≠ {b:?}");
    }
    assert_eq!(s.solve().status, SolveStatus::Solved);
    let loaded = Sketch::from_data(s.to_data()).unwrap();
    assert!(matches!(
        loaded.text(t).unwrap().placement,
        TextPlacement::Path { .. }
    ));
}

/// Mitte des Hüllquaders der Punkte.
fn center(points: &[DVec2]) -> DVec2 {
    let (min, max) = bbox(points);
    0.5 * (min + max)
}

#[test]
fn path_to_box_wraps_current_place() {
    // Waagerechter Pfad: Der Rahmen umfasst den Text nach den Maßen der Schrift (Breite der
    // Zeile, Ober- und Unterlänge), der Text bleibt bei jeder Ausrichtung genau an seiner Stelle.
    let mut s = sketch();
    let path = line(&mut s, v(0.0, 50.0), v(100.0, 50.0));
    let p = params("ab");
    let t = s
        .add_text_path(
            path,
            p.clone(),
            true,
            false,
            HAlign::Left,
            path_shape_of(&p),
        )
        .unwrap();
    let (curves, before) = (glyph_curves(&s, t), positions(&s, t));
    // Breite 2 × 0,8 × 5 = 8, Grundlinie auf dem Pfad, Unterlänge −0,2 × 5, Oberlänge 0,9 × 5.
    let want = [v(0.0, 49.0), v(8.0, 49.0), v(8.0, 54.5), v(0.0, 54.5)];
    for (h, va) in [
        (HAlign::Left, VAlign::Bottom),
        (HAlign::Right, VAlign::Top),
        (HAlign::Center, VAlign::Middle),
    ] {
        let mut s = s.clone();
        let kind = TextKindEdit::Box { h, v: va };
        s.edit_text(t, p.clone(), kind, shape_of(&p)).unwrap();
        let (lines, corners) = frame(&s, t);
        for (k, &c) in corners.iter().enumerate() {
            assert!(
                close(at(&s, c), want[k]),
                "{h:?}/{va:?} Ecke {k}: {:?} ≠ {:?}",
                at(&s, c),
                want[k]
            );
        }
        for (a, b) in before.iter().zip(positions(&s, t)) {
            assert!(close(*a, b), "{h:?}/{va:?}: {a:?} ≠ {b:?}");
        }
        for (k, &l) in lines.iter().enumerate() {
            let d = s.curve(l).unwrap();
            assert!(d.construction, "Rahmenlinie {k} ist Konstruktion");
            assert_eq!(s.text_of_frame(l), Some(t));
        }
        assert!(s.text_angle(t).unwrap().abs() <= 1e-12, "Winkel 0");
        assert!(has_internal_horizontal(&s, lines[0]));
        assert_eq!(
            s.internal_constraints().count(),
            4,
            "Rechteck und Waagerecht"
        );
        assert_eq!(glyph_curves(&s, t), curves, "gleiche Ids");
        assert!(s.curve(path).is_some(), "der Pfad bleibt");
        // Der Pfad ist jetzt frei: Er lässt sich löschen, der Text bleibt an seiner Stelle.
        assert_eq!(s.solve().status, SolveStatus::Solved);
        s.remove_curve(path).unwrap();
        assert!(s.text(t).is_some());
        for (a, b) in before.iter().zip(positions(&s, t)) {
            assert!(close(*a, b), "nach dem Lösen {h:?}/{va:?}: {a:?} ≠ {b:?}");
        }
    }

    // Schräger Pfad, auch mit zwei Zeilen: Der Rahmen steht mit Winkel 0, nach den Maßen der
    // Schrift groß, um die Mitte des Textes an seiner bisherigen Stelle.
    for (content, size) in [("ab", v(8.0, 5.5)), ("a\nb", v(4.0, 11.5))] {
        let mut s = sketch();
        let path = line(&mut s, v(0.0, 0.0), v(50.0, 20.0));
        let p = params(content);
        let t = s
            .add_text_path(
                path,
                p.clone(),
                true,
                false,
                HAlign::Left,
                path_shape_of(&p),
            )
            .unwrap();
        let mid = center(&positions(&s, t));
        for va in [VAlign::Bottom, VAlign::Top] {
            let mut s = s.clone();
            let kind = TextKindEdit::Box {
                h: HAlign::Left,
                v: va,
            };
            s.edit_text(t, p.clone(), kind, shape_of(&p)).unwrap();
            let (_, corners) = frame(&s, t);
            let c: Vec<DVec2> = corners.iter().map(|&k| at(&s, k)).collect();
            assert!(s.text_angle(t).unwrap().abs() <= 1e-12, "Winkel 0");
            assert!(
                (c[1] - c[0] - v(size.x, 0.0)).length() <= 1e-9,
                "{content:?}: {c:?}"
            );
            assert!(
                (c[3] - c[0] - v(0.0, size.y)).length() <= 1e-9,
                "{content:?}: {c:?}"
            );
            assert!(
                close(center(&positions(&s, t)), mid),
                "{content:?}/{va:?}: Mitte {:?} ≠ {mid:?}",
                center(&positions(&s, t))
            );
        }
    }
}

#[test]
fn set_angle_rotates_about_corner0() {
    let mut s = sketch();
    let t = text(&mut s, "ab");
    let (lines, corners) = frame(&s, t);
    assert!(s.text_angle(t).unwrap().abs() <= 1e-12);
    assert!(has_internal_horizontal(&s, lines[0]));
    let (curves, straight) = (glyph_curves(&s, t), positions(&s, t));
    let c0 = at(&s, corners[0]);
    let start: Vec<DVec2> = corners.iter().map(|&c| at(&s, c)).collect();
    let rev = s.revision();
    s.set_text_angle(t, 0.5).unwrap();
    assert_ne!(s.revision(), rev);
    let rot = rot_about(c0, 0.5);
    assert_eq!(at(&s, corners[0]), c0, "Ecke 0 steht");
    for (k, &c) in corners.iter().enumerate() {
        assert!(close(at(&s, c), rot(start[k])), "Ecke {k}");
    }
    assert!((s.text_angle(t).unwrap() - 0.5).abs() <= 1e-12);
    assert!(
        !has_internal_horizontal(&s, lines[0]),
        "Waagerecht entfällt"
    );
    assert_eq!(s.internal_constraints().count(), 3);
    assert_eq!(glyph_curves(&s, t), curves, "gleiche Ids");
    for (a, b) in straight.iter().zip(positions(&s, t)) {
        assert!(close(rot(*a), b), "{a:?} gedreht ≠ {b:?}");
    }
    // Der Löser lässt den gedrehten Rahmen stehen.
    assert_eq!(s.solve().status, SolveStatus::Solved);
    assert!((s.text_angle(t).unwrap() - 0.5).abs() <= 1e-9);
    // Noch einmal drehen, auch über ±π hinaus.
    s.set_text_angle(t, -2.5).unwrap();
    assert!((s.text_angle(t).unwrap() + 2.5).abs() <= 1e-12);
    s.set_text_angle(t, 3.0 * PI / 2.0).unwrap();
    assert!((s.text_angle(t).unwrap() + PI / 2.0).abs() <= 1e-12);
    assert_eq!(at(&s, corners[0]), c0);
    // Gleicher Winkel: nichts geschieht.
    let rev = s.revision();
    s.set_text_angle(t, -PI / 2.0).unwrap();
    assert_eq!(s.revision(), rev);
    // Gedreht gespeichert und geladen bleibt gedreht.
    let mut loaded = Sketch::from_data(s.to_data()).unwrap();
    assert_eq!(loaded.solve().status, SolveStatus::Solved);
    assert!((loaded.text_angle(t).unwrap() + PI / 2.0).abs() <= 1e-9);

    // Eine Abhängigkeit, die den Winkel nicht hält (Ecke 0 fest), lässt das Drehen zu.
    let mut s = sketch();
    let t = text(&mut s, "ab");
    let (_, corners) = frame(&s, t);
    s.add_constraint(K::Fixed(corners[0], 0.0, 0.0)).unwrap();
    s.set_text_angle(t, 0.25).unwrap();
    assert!((s.text_angle(t).unwrap() - 0.25).abs() <= 1e-12);
    assert_eq!(s.solve().status, SolveStatus::Solved);

    // Text auf Pfad hat keinen Winkel.
    let mut s = sketch();
    let path = line(&mut s, v(0.0, 0.0), v(100.0, 0.0));
    let p = params("ab");
    let u = s
        .add_text_path(
            path,
            p.clone(),
            true,
            false,
            HAlign::Left,
            path_shape_of(&p),
        )
        .unwrap();
    assert_eq!(s.text_angle(u), None);
    let (before, rev) = (s.to_data(), s.revision());
    says(s.set_text_angle(u, 0.5), tr!("sketch-text-not-box"), "Pfad");
    unchanged(&s, &before, rev, "Pfad");
}

#[test]
fn angle_held_by_user_constraint_rejected_unchanged() {
    // Waagerecht des Benutzers auf der Grundlinie, nachdem das interne gefallen ist.
    let mut s = sketch();
    let t = text(&mut s, "ab");
    let (lines, _) = frame(&s, t);
    s.set_text_angle(t, 0.3).unwrap();
    s.add_constraint(K::Horizontal(LineOrPoints::Line(lines[0])))
        .unwrap();
    assert_eq!(s.solve().status, SolveStatus::Solved);
    let (before, rev) = (s.to_data(), s.revision());
    says(
        s.set_text_angle(t, 0.7),
        tr!("sketch-text-angle-held"),
        "Waagerecht",
    );
    unchanged(&s, &before, rev, "Waagerecht");

    // Zwei feste Ecken.
    let mut s = sketch();
    let t = text(&mut s, "ab");
    let (_, corners) = frame(&s, t);
    s.add_constraint(K::Fixed(corners[0], 0.0, 0.0)).unwrap();
    s.add_constraint(K::Fixed(corners[1], 40.0, 0.0)).unwrap();
    let (before, rev) = (s.to_data(), s.revision());
    says(
        s.set_text_angle(t, 1.0),
        tr!("sketch-text-angle-held"),
        "feste Ecken",
    );
    unchanged(&s, &before, rev, "feste Ecken");

    // Senkrecht auf der Seitenlinie.
    let mut s = sketch();
    let t = text(&mut s, "ab");
    let (lines, _) = frame(&s, t);
    s.set_text_angle(t, 0.4).unwrap();
    s.add_constraint(K::Vertical(LineOrPoints::Line(lines[1])))
        .unwrap();
    assert_eq!(s.solve().status, SolveStatus::Solved);
    let (before, rev) = (s.to_data(), s.revision());
    says(
        s.set_text_angle(t, -0.4),
        tr!("sketch-text-angle-held"),
        "Senkrecht",
    );
    unchanged(&s, &before, rev, "Senkrecht");

    // Kein endlicher Winkel.
    assert!(matches!(
        s.set_text_angle(t, f64::NAN),
        Err(SketchError::InvalidInput(_))
    ));
    unchanged(&s, &before, rev, "NaN");
}

#[test]
fn edit_failure_leaves_sketch_unchanged() {
    let mut s = sketch();
    let t = text(&mut s, "ab");
    let other = text(&mut s, "c");
    let (lines, corners) = frame(&s, t);
    let (other_lines, _) = frame(&s, other);
    let r = s.add_point(-20.0, 0.0).unwrap();
    s.add_constraint(K::Distance(corners[0], r, 20.0)).unwrap();
    let path = line(&mut s, v(0.0, 50.0), v(100.0, 50.0));
    let p = params("ab");
    let shape = path_shape_of(&p);
    let (before, rev) = (s.to_data(), s.revision());

    // Kein Pfad: Rahmenlinien, eigene und fremde, und Buchstaben.
    for (c, what) in [
        (lines[0], "eigene Rahmenlinie"),
        (other_lines[2], "fremde Rahmenlinie"),
        (glyph_curves(&s, other)[0], "Buchstabe"),
        (glyph_curves(&s, t)[0], "eigener Buchstabe"),
    ] {
        says(
            s.edit_text(t, p.clone(), on_path(c), shape.clone()),
            tr!("sketch-text-bad-path"),
            what,
        );
        unchanged(&s, &before, rev, what);
    }
    // Falsche Werte, beim Wechsel und ohne.
    let mut bad = p.clone();
    bad.height = 0.0;
    says(
        s.edit_text(t, bad.clone(), on_path(path), shape.clone()),
        tr!("sketch-text-height"),
        "Höhe 0 auf Pfad",
    );
    unchanged(&s, &before, rev, "Höhe 0 auf Pfad");
    let box_kind = TextKindEdit::Box {
        h: HAlign::Right,
        v: VAlign::Top,
    };
    says(
        s.edit_text(t, bad, box_kind, shape.clone()),
        tr!("sketch-text-height"),
        "Höhe 0 im Rahmen",
    );
    unchanged(&s, &before, rev, "Höhe 0 im Rahmen");
    // Erst nach dem Abbau des Rahmens zeigt sich: außerhalb des Rechenbereichs. Alles zurück.
    let mut huge = p.clone();
    huge.height = 1e300;
    says(
        s.edit_text(t, huge.clone(), on_path(path), shape.clone()),
        tr!("sketch-text-out-of-range"),
        "riesig auf Pfad",
    );
    unchanged(&s, &before, rev, "riesig auf Pfad");
    // Unbekannter Text.
    s.remove_text(other).unwrap();
    let (before, rev) = (s.to_data(), s.revision());
    assert_eq!(
        s.edit_text(other, p.clone(), on_path(path), shape.clone()),
        Err(SketchError::UnknownObject)
    );
    assert_eq!(
        s.set_text_angle(other, 0.5),
        Err(SketchError::UnknownObject)
    );
    assert_eq!(s.text_angle(other), None);
    unchanged(&s, &before, rev, "unbekannt");

    // Vom Pfad in den Rahmen: zu groß für den Rechenbereich, und leer.
    s.edit_text(t, p.clone(), on_path(path), shape.clone())
        .unwrap();
    let (before, rev) = (s.to_data(), s.revision());
    let box_shape = shape_of(&p);
    says(
        s.edit_text(t, huge, box_kind, box_shape.clone()),
        tr!("sketch-text-out-of-range"),
        "riesig im Rahmen",
    );
    unchanged(&s, &before, rev, "riesig im Rahmen");
    says(
        s.edit_text(t, params(" \n "), box_kind, box_shape),
        tr!("sketch-text-empty"),
        "leer",
    );
    unchanged(&s, &before, rev, "leer");
}

/// Zwei sich widersprechende Maße an einer Linie anderswo: Diese Gruppe ist unlösbar.
fn unrelated_conflict(s: &mut Sketch) {
    let a = s.add_point(100.0, 100.0).unwrap();
    let b = s.add_point(105.0, 100.0).unwrap();
    s.add_line(a, b).unwrap();
    s.add_constraint(K::Distance(a, b, 10.0)).unwrap();
    s.add_constraint(K::Distance(a, b, 20.0)).unwrap();
    assert_eq!(s.solve().status, SolveStatus::Conflicting);
}

#[test]
fn unrelated_conflict_does_not_hold_angle() {
    let mut s = sketch();
    let t = text(&mut s, "ab");
    let (lines, corners) = frame(&s, t);
    unrelated_conflict(&mut s);
    let c0 = at(&s, corners[0]);
    s.set_text_angle(t, 0.5).unwrap();
    assert!((s.text_angle(t).unwrap() - 0.5).abs() <= 1e-12);
    assert_eq!(at(&s, corners[0]), c0);
    // Hält der Benutzer den Winkel, bleibt es bei der Ablehnung, auch neben dem Widerspruch.
    s.add_constraint(K::Vertical(LineOrPoints::Line(lines[1])))
        .unwrap();
    s.solve();
    let (before, rev) = (s.to_data(), s.revision());
    says(
        s.set_text_angle(t, 1.0),
        tr!("sketch-text-angle-held"),
        "Senkrecht neben Widerspruch",
    );
    unchanged(&s, &before, rev, "Senkrecht neben Widerspruch");
}

#[test]
fn angle_with_length_and_outside_distance_follows() {
    let mut s = sketch();
    let t = text(&mut s, "ab");
    let (_, corners) = frame(&s, t);
    s.add_constraint(K::Distance(corners[0], corners[1], 40.0))
        .unwrap();
    let q = s.add_point(60.0, 10.0).unwrap();
    let gap = at(&s, corners[2]).distance(v(60.0, 10.0));
    s.add_constraint(K::Distance(corners[2], q, gap)).unwrap();
    assert_eq!(s.solve().status, SolveStatus::Solved);
    s.set_text_angle(t, 0.5).unwrap();
    assert!((s.text_angle(t).unwrap() - 0.5).abs() <= 1e-12);
    assert_ne!(at(&s, q), v(60.0, 10.0), "der freie Punkt folgt");
    assert!((at(&s, corners[2]).distance(at(&s, q)) - gap).abs() <= 1e-9);
    assert_eq!(s.solve().status, SolveStatus::Solved);
    assert!((s.text_angle(t).unwrap() - 0.5).abs() <= 1e-9);
    assert!((at(&s, corners[0]).distance(at(&s, corners[1])) - 40.0).abs() <= 1e-9);
}
