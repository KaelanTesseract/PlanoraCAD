//! Was am Text hängt (Skizzen 8, Task 7, Spec E): Maße, Bewegen, Kopieren, Skalieren, Spiegeln,
//! Muster, Versatz, Stutzen, Dehnen, Brechen, Abrunden, Fase und Fang am Text.
//!
//! Jede Ablehnung lässt die Skizze gleich (`to_data`) und ihren Stand (`revision`) unverändert.
//! Der Test-Setzer [`BoxShaper`] liefert je Zeichen ein Quadrat mit Loch wie in `text_box.rs`.

use planora_i18n::tr;
use planora_math::{DVec2, Frame, Tolerance};
use planora_sketch::SketchError;
use planora_sketch::constraint::{ConstraintKind as K, LineOrPoints};
use planora_sketch::dimension::DimPick;
use planora_sketch::fillet::{ChamferKind, CurvePick};
use planora_sketch::model::{CurveId, CurveKind, PointId, Sketch};
use planora_sketch::offset::OffsetProblem;
use planora_sketch::pattern::{PatternDir, PatternKind, PatternPlan, PatternProblem};
use planora_sketch::snap::{Snap, SnapIndex};
use planora_sketch::solver::SolveStatus;
use planora_sketch::text::{HAlign, TextId, TextParams, TextPlacement, VAlign};
use planora_sketch::text_shape::{
    Contour, Glyph, Seg, ShapedLine, TextShape, TextShapeError, TextShaper, TextStyle,
};
use planora_sketch::transform::{Similarity, TransformBlock, TransformSet};
use std::collections::HashSet;
use std::f64::consts::FRAC_PI_6;
use std::sync::Arc;

const ADV: f64 = 0.8;

fn v(x: f64, y: f64) -> DVec2 {
    DVec2::new(x, y)
}

/// Quadrat (0, 0)–(0,6, 0,7) mit Loch aus Strecken, quadratischer und kubischer Bézierkurve.
fn box_glyph() -> Vec<Contour> {
    vec![
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
    ]
}

struct BoxShaper;

impl TextShaper for BoxShaper {
    fn shape(&self, content: &str, _style: &TextStyle) -> Result<TextShape, TextShapeError> {
        if content.trim().is_empty() {
            return Err(TextShapeError::Empty);
        }
        let mut lines = Vec::new();
        for text in content.split('\n') {
            let mut glyphs = Vec::new();
            let mut pen = 0.0;
            for ch in text.chars() {
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
            }
            lines.push(ShapedLine { glyphs, width: pen });
        }
        Ok(TextShape {
            lines,
            ascender: 0.9,
            descender: -0.2,
            line_height: 1.2,
            substituted: None,
            synthetic: false,
            missing_chars: false,
        })
    }
}

fn sketch() -> Sketch {
    Sketch::new(Frame::WORLD, "S", Tolerance::default())
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

fn shape_of(content: &str) -> Arc<TextShape> {
    Arc::new(BoxShaper.shape(content, &params(content).style).unwrap())
}

/// Text „a“ mittig im Rahmen (0, 0)–(40, 10): Die Glyphe liegt bei x = 18 … 21.
fn box_text(s: &mut Sketch) -> TextId {
    s.add_text_box(
        v(0.0, 0.0),
        v(40.0, 10.0),
        params("a"),
        HAlign::Center,
        VAlign::Bottom,
        shape_of("a"),
    )
    .unwrap()
}

fn line(s: &mut Sketch, a: DVec2, b: DVec2) -> CurveId {
    let p = s.add_point(a.x, a.y).unwrap();
    let q = s.add_point(b.x, b.y).unwrap();
    s.add_line(p, q).unwrap()
}

/// Text „ab“ auf der Linie (0, 0)–(50, 0).
fn path_text(s: &mut Sketch) -> (TextId, CurveId) {
    let path = line(s, v(0.0, 0.0), v(50.0, 0.0));
    let t = s
        .add_text_path(
            path,
            params("ab"),
            true,
            false,
            HAlign::Left,
            shape_of("ab"),
        )
        .unwrap();
    (t, path)
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

fn glyphs(s: &Sketch, t: TextId) -> Vec<CurveId> {
    s.text(t).unwrap().curves.clone()
}

fn positions(s: &Sketch, t: TextId) -> Vec<DVec2> {
    s.text(t)
        .unwrap()
        .points
        .iter()
        .map(|&p| at(s, p))
        .collect()
}

fn glyph_line(s: &Sketch, t: TextId) -> CurveId {
    glyphs(s, t)
        .into_iter()
        .find(|&c| matches!(s.curve(c).unwrap().kind, CurveKind::Line { .. }))
        .unwrap()
}

fn bbox(points: &[DVec2]) -> (DVec2, DVec2) {
    points.iter().fold(
        (DVec2::splat(f64::INFINITY), DVec2::splat(f64::NEG_INFINITY)),
        |(lo, hi), p| (lo.min(*p), hi.max(*p)),
    )
}

fn close(a: DVec2, b: DVec2) -> bool {
    a.distance(b) <= 1e-9
}

fn all_close(a: &[DVec2], b: &[DVec2], what: &str) {
    assert_eq!(a.len(), b.len(), "{what}: Anzahl");
    for (i, (x, y)) in a.iter().zip(b).enumerate() {
        assert!(close(*x, *y), "{what}: Punkt {i}: {x} statt {y}");
    }
}

fn solved(s: &mut Sketch) {
    let r = s.solve();
    assert_eq!(r.status, SolveStatus::Solved, "{r:?}");
}

fn says<T: std::fmt::Debug>(r: Result<T, SketchError>, msg: String, what: &str) {
    let e = r.expect_err(what).to_string();
    assert!(e.contains(&msg), "{what}: {e}");
}

/// Abgelehnt, und nichts hat sich geändert, auch nicht der Stand.
fn unchanged(s: &Sketch, before: &Sketch, what: &str) {
    assert!(s.to_data() == before.to_data(), "{what}: Skizze verändert");
    assert_eq!(s.revision(), before.revision(), "{what}: Stand verändert");
}

fn shift(x: f64, y: f64) -> Similarity {
    Similarity {
        base: DVec2::ZERO,
        offset: v(x, y),
        angle: 0.0,
        factor: 1.0,
    }
}

/// Keine Abhängigkeit des Benutzers an erzeugten Kurven oder Punkten.
fn no_constraints_at_glyphs(s: &Sketch, t: TextId) {
    let d = s.text(t).unwrap();
    for (_, k) in s.constraints() {
        assert!(
            !k.kind.curves().iter().any(|c| d.curves.contains(c))
                && !k.kind.points().iter().any(|p| d.points.contains(p)),
            "Abhängigkeit an Buchstaben: {:?}",
            k.kind
        );
    }
}

// ---- Maße und Abhängigkeiten -------------------------------------------------------------------

#[test]
fn dimension_on_glyph_rejected() {
    let mut s = box_text_sketch();
    let t = s.texts().next().unwrap().0;
    let g = glyph_line(&s, t);
    let gp = s.text(t).unwrap().points[0];
    let other = s.add_point(-5.0, -5.0).unwrap();
    let pointer = v(19.0, -3.0);
    // Die Planung bietet kein Maß an Buchstaben an.
    assert_eq!(
        s.dimension_plan(&[DimPick::Curve(g, pointer)], pointer),
        None
    );
    assert_eq!(
        s.dimension_plan(&[DimPick::Point(gp), DimPick::Point(other)], pointer),
        None
    );
    let (lines, _) = frame(&s, t);
    assert_eq!(
        s.dimension_plan(
            &[
                DimPick::Curve(g, pointer),
                DimPick::Curve(lines[1], pointer)
            ],
            pointer
        ),
        None
    );
    // Anlegen mit derselben Meldung wie jede Abhängigkeit an Buchstaben.
    let before = s.clone();
    says(
        s.add_constraint(K::Distance(gp, other, 3.0)),
        tr!("sketch-text-no-dimension"),
        "Abstand an Buchstabenpunkt",
    );
    unchanged(&s, &before, "Abstand an Buchstabenpunkt");
    says(
        s.add_constraint(K::Horizontal(LineOrPoints::Line(g))),
        tr!("sketch-text-no-dimension"),
        "Waagerecht an Buchstabe",
    );
    unchanged(&s, &before, "Waagerecht an Buchstabe");
}

fn box_text_sketch() -> Sketch {
    let mut s = sketch();
    box_text(&mut s);
    s
}

#[test]
fn dimension_on_frame_ok() {
    let mut s = sketch();
    let t = box_text(&mut s);
    let (lines, corners) = frame(&s, t);
    let pointer = v(20.0, -3.0);
    let plan = s
        .dimension_plan(&[DimPick::Curve(lines[0], pointer)], pointer)
        .expect("Maß am Rahmen");
    s.add_constraint(plan.kind).unwrap();
    let other = s.add_point(-5.0, -5.0).unwrap();
    let plan = s
        .dimension_plan(
            &[DimPick::Point(corners[0]), DimPick::Point(other)],
            pointer,
        )
        .expect("Maß an der Ecke");
    s.add_constraint(plan.kind).unwrap();
    solved(&mut s);
}

// ---- Bewegen, Kopieren, Drehen, Skalieren ------------------------------------------------------

#[test]
fn move_box_text() {
    let mut s = sketch();
    let t = box_text(&mut s);
    let (lines, corners) = frame(&s, t);
    let ids = glyphs(&s, t);
    let before = positions(&s, t);
    let corners_before: Vec<DVec2> = corners.iter().map(|&p| at(&s, p)).collect();
    // Eine Buchstabenkurve gewählt: Der ganze Text samt Rahmen geht mit.
    let set = s.transform_set(&[ids[0]], &[]);
    for c in lines.iter().chain(&ids) {
        assert!(set.curves.contains(c), "{c:?} fehlt in der Menge");
    }
    s.apply_transform(&set, &shift(5.0, 3.0)).unwrap();
    let want: Vec<DVec2> = before.iter().map(|p| *p + v(5.0, 3.0)).collect();
    all_close(&positions(&s, t), &want, "Buchstaben");
    for (p, b) in corners.iter().zip(&corners_before) {
        assert!(close(at(&s, *p), *b + v(5.0, 3.0)), "Ecke");
    }
    assert_eq!(glyphs(&s, t), ids, "Ids bleiben");
    assert_eq!(s.texts().count(), 1);
    solved(&mut s);
    all_close(&positions(&s, t), &want, "nach dem Lösen");

    // Nur eine Rahmenlinie gewählt: ebenso der ganze Text.
    let set = s.transform_set(&[lines[2]], &[]);
    for c in lines.iter().chain(&ids) {
        assert!(set.curves.contains(c), "{c:?} fehlt (Rahmenlinie)");
    }
    s.apply_transform(&set, &shift(-5.0, -3.0)).unwrap();
    all_close(&positions(&s, t), &before, "zurück");
    solved(&mut s);
    all_close(&positions(&s, t), &before, "zurück, gelöst");
}

#[test]
fn rotate_box_text() {
    let mut s = sketch();
    let t = box_text(&mut s);
    let ids = glyphs(&s, t);
    let before = positions(&s, t);
    let turn = Similarity {
        base: DVec2::ZERO,
        offset: DVec2::ZERO,
        angle: FRAC_PI_6,
        factor: 1.0,
    };
    let set = s.transform_set(&[ids[3]], &[]);
    s.apply_transform(&set, &turn).unwrap();
    let want: Vec<DVec2> = before.iter().map(|p| turn.apply(*p)).collect();
    all_close(&positions(&s, t), &want, "gedreht");
    assert!((s.text_angle(t).unwrap() - FRAC_PI_6).abs() <= 1e-12);
    // Das interne Waagerecht der Grundlinie hielte den Rahmen sonst beim Lösen zurück.
    solved(&mut s);
    all_close(&positions(&s, t), &want, "gedreht, gelöst");
    assert!((s.text_angle(t).unwrap() - FRAC_PI_6).abs() <= 1e-9);
}

#[test]
fn move_box_text_with_fixed_corner_rejected() {
    let mut s = sketch();
    let t = box_text(&mut s);
    let (_, corners) = frame(&s, t);
    let c0 = at(&s, corners[0]);
    s.add_constraint(K::Fixed(corners[0], c0.x, c0.y)).unwrap();
    solved(&mut s);
    let set = s.transform_set(&[glyphs(&s, t)[0]], &[]);
    let before = s.clone();
    assert!(matches!(
        s.apply_transform(&set, &shift(5.0, 0.0)),
        Err(TransformBlock::FixedInSet(_))
    ));
    unchanged(&s, &before, "feste Ecke");
}

#[test]
fn move_path_text_without_path_rejected() {
    let mut s = sketch();
    let (t, _) = path_text(&mut s);
    let set = s.transform_set(&[glyphs(&s, t)[0]], &[]);
    let before = s.clone();
    assert_eq!(
        s.check_transform(&set, &shift(0.0, 5.0)),
        Err(TransformBlock::TextOnPath)
    );
    assert_eq!(
        s.apply_transform(&set, &shift(0.0, 5.0)),
        Err(TransformBlock::TextOnPath)
    );
    unchanged(&s, &before, "Verschieben ohne Pfad");
    says(
        s.copy_transformed(&set, &shift(0.0, 5.0)),
        tr!("sketch-text-path-follows"),
        "Kopieren ohne Pfad",
    );
    unchanged(&s, &before, "Kopieren ohne Pfad");
}

#[test]
fn move_path_text_with_path_ok() {
    let mut s = sketch();
    let (t, path) = path_text(&mut s);
    let ids = glyphs(&s, t);
    let before = positions(&s, t);
    let set = s.transform_set(&[ids[0], path], &[]);
    s.apply_transform(&set, &shift(0.0, 7.0)).unwrap();
    let want: Vec<DVec2> = before.iter().map(|p| *p + v(0.0, 7.0)).collect();
    all_close(&positions(&s, t), &want, "mit Pfad");
    solved(&mut s);
    all_close(&positions(&s, t), &want, "mit Pfad, gelöst");

    // Nur der Pfad: Der Text folgt ihm.
    let set = s.transform_set(&[path], &[]);
    s.apply_transform(&set, &shift(0.0, -7.0)).unwrap();
    all_close(&positions(&s, t), &before, "Pfad allein");
    assert_eq!(glyphs(&s, t), ids);
}

#[test]
fn copy_box_text_makes_second_text() {
    let mut s = sketch();
    let t = box_text(&mut s);
    let (_, corners) = frame(&s, t);
    let before = positions(&s, t);
    let set = s.transform_set(&[glyphs(&s, t)[0]], &[]);
    let map = s.copy_transformed(&set, &shift(0.0, 20.0)).unwrap();
    let texts: Vec<TextId> = s.texts().map(|(id, _)| id).collect();
    assert_eq!(texts.len(), 2, "zweiter Text");
    let copy = texts.into_iter().find(|&id| id != t).unwrap();
    let (a, b) = (s.text(t).unwrap(), s.text(copy).unwrap());
    assert_eq!(
        (&a.content, &a.style, a.height, a.flip_h, a.flip_v),
        (&b.content, &b.style, b.height, b.flip_h, b.flip_v),
        "gleiche Werte"
    );
    assert!(s.is_text_shaped(copy), "sofort gesetzt");
    let (copy_lines, copy_corners) = frame(&s, copy);
    for (i, &p) in corners.iter().enumerate() {
        assert_eq!(map.points[&p], copy_corners[i], "Ecke {i} abgebildet");
        assert!(close(at(&s, copy_corners[i]), at(&s, p) + v(0.0, 20.0)));
    }
    for &l in &copy_lines {
        assert!(
            s.curve(l).unwrap().construction,
            "Rahmen bleibt Konstruktion"
        );
    }
    let own: HashSet<CurveId> = glyphs(&s, t).into_iter().collect();
    assert!(
        glyphs(&s, copy).iter().all(|c| !own.contains(c)),
        "eigene Kurven"
    );
    let want: Vec<DVec2> = before.iter().map(|p| *p + v(0.0, 20.0)).collect();
    all_close(&positions(&s, copy), &want, "Kopie");
    all_close(&positions(&s, t), &before, "Original");
    solved(&mut s);
    all_close(&positions(&s, copy), &want, "Kopie, gelöst");
    all_close(&positions(&s, t), &before, "Original, gelöst");
    // Der kopierte Rahmen hat die internen Abhängigkeiten eines neuen Rahmens (Rechteck und
    // Waagerecht der Grundlinie), nicht solche des Benutzers.
    let at_copy = |k: &K| k.curves().iter().any(|c| copy_lines.contains(c));
    assert_eq!(
        s.internal_constraints()
            .filter(|(_, d)| at_copy(&d.kind))
            .count(),
        4
    );
    assert_eq!(s.constraints().filter(|(_, d)| at_copy(&d.kind)).count(), 0);
    assert_eq!(s.text_angle(copy), Some(0.0));
}

#[test]
fn copy_path_text_with_path_copies_both() {
    let mut s = sketch();
    let (t, path) = path_text(&mut s);
    let before = positions(&s, t);
    let set = s.transform_set(&[glyphs(&s, t)[0], path], &[]);
    let map = s.copy_transformed(&set, &shift(0.0, 20.0)).unwrap();
    let copy = s.texts().map(|(id, _)| id).find(|&id| id != t).unwrap();
    match s.text(copy).unwrap().placement {
        TextPlacement::Path { path: p, .. } => assert_eq!(p, map.curves[&path]),
        TextPlacement::Box { .. } => panic!("auf Pfad erwartet"),
    }
    let want: Vec<DVec2> = before.iter().map(|p| *p + v(0.0, 20.0)).collect();
    all_close(&positions(&s, copy), &want, "Kopie auf Pfad");
    solved(&mut s);
    all_close(&positions(&s, copy), &want, "Kopie auf Pfad, gelöst");
}

#[test]
fn scale_box_text_scales_height() {
    let mut s = sketch();
    let t = box_text(&mut s);
    let before = positions(&s, t);
    let grow = Similarity {
        base: DVec2::ZERO,
        offset: DVec2::ZERO,
        angle: 0.0,
        factor: 2.0,
    };
    let set = s.transform_set(&[glyphs(&s, t)[0]], &[]);
    s.apply_transform(&set, &grow).unwrap();
    assert_eq!(s.text(t).unwrap().height, 10.0);
    let want: Vec<DVec2> = before.iter().map(|p| *p * 2.0).collect();
    all_close(&positions(&s, t), &want, "skaliert");
    solved(&mut s);
    all_close(&positions(&s, t), &want, "skaliert, gelöst");

    // Skaliert kopiert: Die Kopie hat die halbe Höhe.
    let set = s.transform_set(&[glyphs(&s, t)[0]], &[]);
    let half = Similarity {
        factor: 0.5,
        ..grow
    };
    s.copy_transformed(&set, &half).unwrap();
    let copy = s.texts().map(|(id, _)| id).find(|&id| id != t).unwrap();
    assert_eq!(s.text(copy).unwrap().height, 5.0);
    all_close(&positions(&s, copy), &before, "halb kopiert");
}

// ---- Spiegeln, Muster, Versatz ---------------------------------------------------------------

#[test]
fn mirror_text_rejected() {
    let mut s = sketch();
    let t = box_text(&mut s);
    let axis = line(&mut s, v(-10.0, -10.0), v(-10.0, 10.0));
    let before = s.clone();
    says(
        s.add_mirrored(&[glyphs(&s, t)[0]], axis),
        tr!("sketch-text-no-mirror"),
        "Buchstabe spiegeln",
    );
    unchanged(&s, &before, "Buchstabe spiegeln");
    let other = line(&mut s, v(-30.0, 0.0), v(-20.0, 0.0));
    let before = s.clone();
    says(
        s.add_mirrored(&[other], glyph_line(&s, t)),
        tr!("sketch-text-no-mirror"),
        "an Buchstabe spiegeln",
    );
    unchanged(&s, &before, "an Buchstabe spiegeln");
}

#[test]
fn mirror_frame_lines_only_copies_lines() {
    let mut s = sketch();
    let t = box_text(&mut s);
    let (lines, _) = frame(&s, t);
    let axis = line(&mut s, v(-10.0, -10.0), v(-10.0, 10.0));
    let n = s.curves().count();
    let copies = s.add_mirrored(&lines, axis).unwrap();
    assert_eq!(copies.len(), 4);
    assert_eq!(s.curves().count(), n + 4, "nur die vier Linien");
    assert_eq!(s.texts().count(), 1, "kein neuer Text");
    for c in copies {
        assert!(matches!(s.curve(c).unwrap().kind, CurveKind::Line { .. }));
        assert_eq!(s.text_of_curve(c), None);
        assert_eq!(s.text_of_frame(c), None);
    }
    solved(&mut s);
}

fn rect_plan(sources: Vec<CurveId>) -> PatternPlan {
    PatternPlan {
        kind: PatternKind::Rect {
            dirs: [PatternDir::SketchX, PatternDir::SketchY],
            extent: [false, false],
            symmetric: [false, false],
        },
        sources,
        counts: [2, 1],
        values: [60.0, 10.0],
        suppressed: Vec::new(),
    }
}

#[test]
fn pattern_text_rejected() {
    let mut s = sketch();
    let t = box_text(&mut s);
    let g = glyphs(&s, t)[0];
    let before = s.clone();
    assert_eq!(
        s.pattern_transforms(&rect_plan(vec![g])).err(),
        Some(PatternProblem::Text)
    );
    says(
        s.add_pattern(&rect_plan(vec![g])),
        tr!("sketch-text-no-pattern"),
        "Muster aus Buchstaben",
    );
    unchanged(&s, &before, "Muster aus Buchstaben");
    says(
        s.add_linear_pattern(&[g], v(60.0, 0.0), 1),
        tr!("sketch-text-no-pattern"),
        "lineares Muster aus Buchstaben",
    );
    unchanged(&s, &before, "lineares Muster aus Buchstaben");
    // Nur Rahmenlinien: Sie werden wie Linien vervielfältigt, ein Text entsteht nicht.
    let (lines, _) = frame(&s, t);
    s.add_pattern(&rect_plan(lines.to_vec())).unwrap();
    assert_eq!(s.texts().count(), 1);
}

#[test]
fn offset_text_rejected() {
    let mut s = sketch();
    let t = box_text(&mut s);
    let before = s.clone();
    assert_eq!(
        s.ordered_chains(&glyphs(&s, t)).err(),
        Some(OffsetProblem::Text)
    );
    assert_eq!(
        s.ordered_chains(&[glyphs(&s, t)[0]]).err(),
        Some(OffsetProblem::Text)
    );
    unchanged(&s, &before, "Versatz");
    // Rahmenlinien sind Linien: Ihr Versatz geht.
    let (lines, _) = frame(&s, t);
    assert!(s.ordered_chains(&lines).is_ok());
}

// ---- Stutzen, Dehnen, Brechen, Abrunden, Fase --------------------------------------------------

#[test]
fn trim_glyph_rejected() {
    let mut s = sketch();
    let t = box_text(&mut s);
    let g = glyph_line(&s, t);
    let (lines, _) = frame(&s, t);
    let on = |s: &Sketch, c: CurveId| match s.curve(c).unwrap().kind {
        CurveKind::Line { a, b } => (at(s, a) + at(s, b)) * 0.5,
        _ => unreachable!(),
    };
    let before = s.clone();
    let here = on(&s, g);
    says(s.trim(g, here), tr!("sketch-text-no-trim"), "Stutzen");
    unchanged(&s, &before, "Stutzen");
    says(s.extend(g, here), tr!("sketch-text-no-extend"), "Dehnen");
    unchanged(&s, &before, "Dehnen");
    says(
        s.break_curve(g, here),
        tr!("sketch-text-no-break"),
        "Brechen",
    );
    unchanged(&s, &before, "Brechen");
    assert_eq!(s.trim_piece(g, here), None, "keine Vorschau");
    // Eine Rahmenlinie gehört zum Text: Stutzen zerrisse den Rahmen.
    let here = on(&s, lines[0]);
    says(
        s.trim(lines[0], here),
        tr!("sketch-text-no-trim"),
        "Rahmen stutzen",
    );
    unchanged(&s, &before, "Rahmen stutzen");
    says(
        s.break_curve(lines[0], here),
        tr!("sketch-text-no-break"),
        "Rahmen brechen",
    );
    unchanged(&s, &before, "Rahmen brechen");
}

#[test]
fn trim_other_curve_at_glyph_ok() {
    let mut s = sketch();
    let t = box_text(&mut s);
    let (lo, _) = bbox(&positions(&s, t));
    let y = lo.y + 0.5;
    let l = line(&mut s, v(10.0, y), v(30.0, y));
    let ids = glyphs(&s, t);
    let before = positions(&s, t);
    s.trim(l, v(12.0, y)).unwrap();
    let CurveKind::Line { a, b } = s.curve(l).unwrap().kind else {
        panic!("Linie erwartet");
    };
    let xs = [at(&s, a).x, at(&s, b).x];
    assert!(
        xs.iter().any(|x| (x - lo.x).abs() <= 1e-9),
        "an der Buchstabenkante gestutzt: {xs:?}"
    );
    assert!(xs.iter().all(|&x| x >= lo.x - 1e-9), "{xs:?}");
    assert_eq!(glyphs(&s, t), ids);
    all_close(&positions(&s, t), &before, "Buchstaben bleiben");
    no_constraints_at_glyphs(&s, t);
    solved(&mut s);
}

#[test]
fn extend_to_glyph_ok() {
    let mut s = sketch();
    let t = box_text(&mut s);
    let (lo, _) = bbox(&positions(&s, t));
    let y = lo.y + 0.5;
    let l = line(&mut s, v(10.0, y), v(15.0, y));
    assert!(s.extend(l, v(15.0, y)).unwrap());
    let CurveKind::Line { b, .. } = s.curve(l).unwrap().kind else {
        panic!("Linie erwartet");
    };
    assert!(
        close(at(&s, b), v(lo.x, y)),
        "bis zur Buchstabenkante: {}",
        at(&s, b)
    );
    no_constraints_at_glyphs(&s, t);
    solved(&mut s);
}

#[test]
fn fillet_glyph_rejected() {
    let mut s = sketch();
    let t = box_text(&mut s);
    let ids = glyphs(&s, t);
    // Zwei Stücke des Außenquadrats teilen eine Ecke.
    let (g0, g1) = (ids[0], ids[1]);
    let near = |s: &Sketch, c: CurveId| match s.curve(c).unwrap().kind {
        CurveKind::Line { a, b } => at(s, a) * 0.75 + at(s, b) * 0.25,
        _ => unreachable!(),
    };
    let a = CurvePick {
        curve: g0,
        at: near(&s, g0),
    };
    let b = CurvePick {
        curve: g1,
        at: near(&s, g1),
    };
    assert_eq!(s.fillet_plan(a, b, 0.2), None);
    assert_eq!(s.fillet_corner(a, b), None);
    assert_eq!(s.chamfer_plan(a, b, ChamferKind::Equal(0.2)), None);

    // Ein Plan aus zwei gewöhnlichen Linien, umgehängt auf Buchstaben, wird beim Anlegen
    // abgelehnt.
    let p = s.add_point(-20.0, 0.0).unwrap();
    let q = s.add_point(-10.0, 0.0).unwrap();
    let r = s.add_point(-10.0, 10.0).unwrap();
    let l1 = s.add_line(p, q).unwrap();
    let l2 = s.add_line(q, r).unwrap();
    let pa = CurvePick {
        curve: l1,
        at: v(-15.0, 0.0),
    };
    let pb = CurvePick {
        curve: l2,
        at: v(-10.0, 5.0),
    };
    let mut plan = s.fillet_plan(pa, pb, 1.0).unwrap();
    plan.picks = [a, b];
    let before = s.clone();
    says(
        s.add_fillet(&plan, None),
        tr!("sketch-text-no-fillet"),
        "Rundung",
    );
    unchanged(&s, &before, "Rundung");
    let mut plan = s.chamfer_plan(pa, pb, ChamferKind::Equal(1.0)).unwrap();
    plan.picks = [a, b];
    says(s.add_chamfer(&plan), tr!("sketch-text-no-chamfer"), "Fase");
    unchanged(&s, &before, "Fase");
}

// ---- Fang ------------------------------------------------------------------------------------

#[test]
fn no_snap_points_on_glyphs() {
    let mut s = sketch();
    let t = box_text(&mut s);
    // Eine Linie quer durch den Buchstaben: Schnitte mit Buchstaben fangen nicht.
    let (lo, _) = bbox(&positions(&s, t));
    let y = lo.y + 0.5;
    line(&mut s, v(10.0, y), v(30.0, y));
    let index = SnapIndex::build(&s);
    let d = s.text(t).unwrap();
    let own_points: HashSet<PointId> = d.points.iter().copied().collect();
    let own_curves: HashSet<CurveId> = d.curves.iter().copied().collect();
    let glyph_snap = |snap: Snap| match snap {
        Snap::Point(p) | Snap::Center(p) => own_points.contains(&p),
        Snap::Midpoint(c) => own_curves.contains(&c),
        Snap::Intersection(a, b) => own_curves.contains(&a) || own_curves.contains(&b),
        _ => false,
    };
    for &p in &d.points {
        let hit = index.find(at(&s, p), 0.05, &[]);
        assert!(
            hit.is_none_or(|h| !glyph_snap(h.snap)),
            "Fangpunkt am Buchstaben: {hit:?}"
        );
        let align = index.align(at(&s, p) + v(0.0, 100.0), 1e-6, &[], &[]);
        assert!(
            align.x.is_none_or(|(q, _)| !own_points.contains(&q)),
            "Ausrichtung an Buchstabenpunkt"
        );
    }
    for &c in &d.curves {
        if let CurveKind::Line { a, b } = s.curve(c).unwrap().kind {
            let mid = (at(&s, a) + at(&s, b)) * 0.5;
            let hit = index.find(mid, 0.05, &[]);
            assert!(hit.is_none_or(|h| !glyph_snap(h.snap)), "Mitte: {hit:?}");
        }
    }
    let hit = index.find(v(lo.x, y), 0.05, &[]);
    assert!(hit.is_none_or(|h| !glyph_snap(h.snap)), "Schnitt: {hit:?}");
    // Der Rahmen fängt wie Linien.
    let (lines, corners) = frame(&s, t);
    let hit = index.find(at(&s, corners[2]), 0.05, &[]).unwrap();
    assert_eq!(hit.snap, Snap::Point(corners[2]));
    let hit = index.find(v(20.0, 0.0), 0.05, &[]).unwrap();
    assert_eq!(hit.snap, Snap::Midpoint(lines[0]));
}

/// Das ganze Bewegen der Skizze nimmt Text auf Pfad nur mit, wenn sein Pfad mitgeht.
#[test]
fn whole_sketch_move_keeps_text_on_held_path() {
    let mut s = sketch();
    let (t, path) = path_text(&mut s);
    let CurveKind::Line { a, b } = s.curve(path).unwrap().kind else {
        unreachable!()
    };
    for p in [a, b] {
        let xy = at(&s, p);
        s.add_constraint(K::Fixed(p, xy.x, xy.y)).unwrap();
    }
    box_text(&mut s);
    solved(&mut s);
    let before = positions(&s, t);
    let set: TransformSet = s.whole_sketch_set();
    assert!(!set.curves.contains(&path));
    assert!(glyphs(&s, t).iter().all(|c| !set.curves.contains(c)));
    s.apply_transform(&set, &shift(0.0, 30.0)).unwrap();
    all_close(&positions(&s, t), &before, "Text am festen Pfad");
}

/// Kopieren und Bewegen gehen auch, solange ein geladener Text noch nicht gesetzt ist: Die
/// Buchstaben werden mit bewegt, das spätere Neusetzen ändert an ihrer Lage nichts.
#[test]
fn copy_and_move_unshaped_text() {
    let mut s = sketch();
    let t = box_text(&mut s);
    solved(&mut s);
    let mut loaded = Sketch::from_data(s.to_data()).unwrap();
    assert!(!loaded.is_text_shaped(t));
    let before = positions(&loaded, t);
    let set = loaded.transform_set(&[glyphs(&loaded, t)[0]], &[]);
    let map = loaded.copy_transformed(&set, &shift(0.0, 20.0)).unwrap();
    let copy = map.texts[&t];
    assert!(!loaded.is_text_shaped(copy));
    let want: Vec<DVec2> = before.iter().map(|p| *p + v(0.0, 20.0)).collect();
    all_close(&positions(&loaded, copy), &want, "ungesetzt kopiert");
    let set = loaded.transform_set(&[glyphs(&loaded, t)[0]], &[]);
    loaded.apply_transform(&set, &shift(5.0, 0.0)).unwrap();
    let moved: Vec<DVec2> = before.iter().map(|p| *p + v(5.0, 0.0)).collect();
    all_close(&positions(&loaded, t), &moved, "ungesetzt bewegt");
    loaded.reshape_texts(&BoxShaper);
    assert!(loaded.is_text_shaped(copy));
    all_close(&positions(&loaded, copy), &want, "Kopie nach dem Neusetzen");
    all_close(
        &positions(&loaded, t),
        &moved,
        "Original nach dem Neusetzen",
    );
}

/// Gedreht kopiert: Die Kopie bekommt kein Waagerecht und bleibt beim Lösen gedreht.
#[test]
fn copy_rotated_keeps_angle() {
    let mut s = sketch();
    let t = box_text(&mut s);
    let turn = Similarity {
        base: DVec2::ZERO,
        offset: v(0.0, 30.0),
        angle: FRAC_PI_6,
        factor: 1.0,
    };
    let set = s.transform_set(&[glyphs(&s, t)[0]], &[]);
    let map = s.copy_transformed(&set, &turn).unwrap();
    let copy = map.texts[&t];
    let want: Vec<DVec2> = positions(&s, t).iter().map(|p| turn.apply(*p)).collect();
    solved(&mut s);
    assert!((s.text_angle(copy).unwrap() - FRAC_PI_6).abs() <= 1e-9);
    assert_eq!(s.text_angle(t), Some(0.0));
    all_close(&positions(&s, copy), &want, "gedrehte Kopie");
}

// ---- Buchstaben selbst ändern (Korrekturrunde 1) -----------------------------------------------

fn glyph_of(s: &Sketch, t: TextId, want: fn(&CurveKind) -> bool) -> CurveId {
    glyphs(s, t)
        .into_iter()
        .find(|&c| want(&s.curve(c).unwrap().kind))
        .unwrap()
}

#[test]
fn conic_rho_on_glyph_rejected() {
    let mut s = sketch();
    let t = box_text(&mut s);
    let conic = glyph_of(&s, t, |k| matches!(k, CurveKind::Conic { .. }));
    let before = s.clone();
    says(
        s.set_conic_rho(conic, 0.7),
        tr!("sketch-text-no-edit"),
        "Rho am Buchstaben",
    );
    unchanged(&s, &before, "Rho am Buchstaben");
}

#[test]
fn spline_degree_on_glyph_rejected() {
    let mut s = sketch();
    let t = box_text(&mut s);
    let spline = glyph_of(&s, t, |k| matches!(k, CurveKind::ControlSpline { .. }));
    let before = s.clone();
    says(
        s.set_control_spline_degree(spline, 5),
        tr!("sketch-text-no-edit"),
        "Grad am Buchstaben",
    );
    unchanged(&s, &before, "Grad am Buchstaben");
}

#[test]
fn insert_control_point_on_glyph_rejected() {
    let mut s = sketch();
    let t = box_text(&mut s);
    let spline = glyph_of(&s, t, |k| matches!(k, CurveKind::ControlSpline { .. }));
    let CurveKind::ControlSpline { ref points, .. } = s.curve(spline).unwrap().kind else {
        unreachable!()
    };
    let near = (at(&s, points[0]) + at(&s, points[3])) * 0.5;
    let before = s.clone();
    says(
        s.insert_control_point(spline, near),
        tr!("sketch-text-no-edit"),
        "Kontrollpunkt am Buchstaben",
    );
    unchanged(&s, &before, "Kontrollpunkt am Buchstaben");
}

#[test]
fn set_glyph_point_rejected() {
    let mut s = sketch();
    let t = box_text(&mut s);
    let p = s.text(t).unwrap().points[0];
    let before = s.clone();
    says(
        s.set_point_xy(p, 1.0, 1.0),
        tr!("sketch-text-no-edit"),
        "Lage eines Buchstabenpunkts",
    );
    unchanged(&s, &before, "Lage eines Buchstabenpunkts");
    // Ecken des Rahmens sind gewöhnliche Punkte.
    let (_, corners) = frame(&s, t);
    s.set_point_xy(corners[2], 41.0, 11.0).unwrap();
}

#[test]
fn pattern_direction_on_glyph_rejected() {
    let mut s = sketch();
    let t = box_text(&mut s);
    let other = line(&mut s, v(-30.0, 0.0), v(-20.0, 0.0));
    let mut plan = rect_plan(vec![other]);
    plan.kind = PatternKind::Rect {
        dirs: [PatternDir::Line(glyph_line(&s, t)), PatternDir::SketchY],
        extent: [false, false],
        symmetric: [false, false],
    };
    let before = s.clone();
    assert_eq!(
        s.pattern_transforms(&plan).err(),
        Some(PatternProblem::Text)
    );
    says(
        s.add_pattern(&plan),
        tr!("sketch-text-no-pattern"),
        "Richtung am Buchstaben",
    );
    unchanged(&s, &before, "Richtung am Buchstaben");
}
