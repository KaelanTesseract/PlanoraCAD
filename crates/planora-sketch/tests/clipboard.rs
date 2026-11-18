//! Zwischenablage im Skizzenmodell (Skizzen 11b, Task 1): `extract` legt die Auswahl als eigene
//! Skizze mit dichten Ids ab, `paste_from` legt sie in einer Skizze neu an.

use planora_i18n::tr;
use planora_kernel::Curve;
use planora_math::nurbs::NurbsCurve;
use planora_math::{DVec2, DVec3, Frame, Tolerance};
use planora_sketch::clipboard::ClipSet;
use planora_sketch::comb::CombSettings;
use planora_sketch::constraint::{ConstraintKind as K, LineOrPoints};
use planora_sketch::helpers::ProjectStyle;
use planora_sketch::model::{CurveId, CurveKind, LineStyle, PointId, Sketch};
use planora_sketch::text::{HAlign, TextId, TextParams, TextPlacement, VAlign};
use planora_sketch::text_shape::{
    Contour, Glyph, Seg, ShapedLine, TextShape, TextShapeError, TextShaper, TextStyle,
};
use planora_sketch::{CopyMap, SketchError};
use std::collections::HashSet;
use std::sync::Arc;

fn sketch() -> Sketch {
    Sketch::new(Frame::WORLD, "S", Tolerance::default())
}

fn v(x: f64, y: f64) -> DVec2 {
    DVec2::new(x, y)
}

fn pt(s: &mut Sketch, x: f64, y: f64) -> PointId {
    s.add_point(x, y).unwrap()
}

fn line(s: &mut Sketch, a: DVec2, b: DVec2) -> CurveId {
    let (pa, pb) = (pt(s, a.x, a.y), pt(s, b.x, b.y));
    s.add_line(pa, pb).unwrap()
}

fn ends(s: &Sketch, l: CurveId) -> (PointId, PointId) {
    match s.curve(l).unwrap().kind {
        CurveKind::Line { a, b } => (a, b),
        ref k => panic!("keine Linie: {k:?}"),
    }
}

/// Rechteck aus vier Linien mit eigenen Endpunkten: 4 Coincident, 2 Horizontal, 2 Vertikal,
/// Breite und Höhe als Maß.
struct Rect {
    lines: [CurveId; 4],
    width: planora_sketch::ConstraintId,
    height: planora_sketch::ConstraintId,
}

fn rect(s: &mut Sketch) -> Rect {
    let c = [v(0.0, 0.0), v(30.0, 0.0), v(30.0, 20.0), v(0.0, 20.0)];
    let mut lines = Vec::new();
    for i in 0..4 {
        lines.push(line(s, c[i], c[(i + 1) % 4]));
    }
    for i in 0..4 {
        let (_, b) = ends(s, lines[i]);
        let (a, _) = ends(s, lines[(i + 1) % 4]);
        s.add_constraint(K::Coincident(b, a)).unwrap();
    }
    for (i, &l) in lines.iter().enumerate() {
        let k = if i % 2 == 0 {
            K::Horizontal(LineOrPoints::Line(l))
        } else {
            K::Vertical(LineOrPoints::Line(l))
        };
        s.add_constraint(k).unwrap();
    }
    let (a0, b0) = ends(s, lines[0]);
    let (a1, b1) = ends(s, lines[1]);
    let width = s.add_constraint(K::Distance(a0, b0, 30.0)).unwrap();
    let height = s.add_constraint(K::Distance(a1, b1, 20.0)).unwrap();
    Rect {
        lines: [lines[0], lines[1], lines[2], lines[3]],
        width,
        height,
    }
}

fn curves_only(curves: &[CurveId]) -> ClipSet {
    ClipSet {
        curves: curves.to_vec(),
        ..ClipSet::default()
    }
}

fn count(s: &Sketch, f: impl Fn(&K) -> bool) -> usize {
    s.constraints().filter(|(_, d)| f(&d.kind)).count()
}

fn all_curves(s: &Sketch) -> Vec<CurveId> {
    s.curves().map(|(c, _)| c).collect()
}

// ---- Text ohne echte Schrift: ein Quadrat je Zeichen -------------------------------------------

struct SquareShaper;

impl TextShaper for SquareShaper {
    fn shape(&self, content: &str, _style: &TextStyle) -> Result<TextShape, TextShapeError> {
        if content.trim().is_empty() {
            return Err(TextShapeError::Empty);
        }
        let glyphs: Vec<Glyph> = content
            .chars()
            .enumerate()
            .map(|(i, _)| Glyph {
                origin: v(i as f64 * 0.8, 0.0),
                advance: 0.8,
                contours: vec![Contour {
                    start: v(0.0, 0.0),
                    segs: vec![
                        Seg::Line(v(0.6, 0.0)),
                        Seg::Line(v(0.6, 0.7)),
                        Seg::Line(v(0.0, 0.7)),
                        Seg::Line(v(0.0, 0.0)),
                    ],
                }],
            })
            .collect();
        let width = glyphs.len() as f64 * 0.8;
        Ok(TextShape {
            lines: vec![ShapedLine { glyphs, width }],
            ascender: 0.9,
            descender: -0.2,
            line_height: 1.2,
            substituted: None,
            synthetic: false,
            missing_chars: false,
        })
    }
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

fn shape_of(p: &TextParams) -> Arc<TextShape> {
    Arc::new(SquareShaper.shape(&p.content, &p.style).unwrap())
}

fn text_box(s: &mut Sketch, content: &str) -> TextId {
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

// ---- Extrahieren -------------------------------------------------------------------------------

#[test]
fn extract_keeps_inner_constraints_drops_outer() {
    let mut s = sketch();
    let r = rect(&mut s);
    let outside = line(&mut s, v(0.0, 40.0), v(30.0, 45.0));
    s.add_constraint(K::Parallel(outside, r.lines[0])).unwrap();
    let (corner, _) = ends(&s, r.lines[0]);
    s.add_constraint(K::Fixed(corner, 0.0, 0.0)).unwrap();

    let out = s.extract(&curves_only(&r.lines)).unwrap();
    let clip = &out.clip;
    assert_eq!(clip.curves().count(), 4);
    assert_eq!(clip.points().count(), 8);
    assert_eq!(count(clip, |k| matches!(k, K::Distance(..))), 2);
    assert_eq!(count(clip, |k| matches!(k, K::Coincident(..))), 4);
    assert_eq!(count(clip, |k| matches!(k, K::Horizontal(_))), 2);
    assert_eq!(count(clip, |k| matches!(k, K::Vertical(_))), 2);
    assert_eq!(count(clip, |k| matches!(k, K::Parallel(..))), 0);
    assert_eq!(
        count(clip, |k| matches!(k, K::Fixed(..) | K::Fixed3(..))),
        0
    );
    // Die Abbildung führt von der Quelle in den Clip, die Lage ist dieselbe.
    assert_eq!(out.map.curves.len(), 4);
    assert!(!out.map.curves.contains_key(&outside));
    for (&from, &to) in &out.map.points {
        assert_eq!(s.point_xy(from).unwrap(), clip.point_xy(to).unwrap());
    }
    let w = out.map.constraints[&r.width];
    assert_eq!(
        clip.constraint(w).unwrap().kind,
        K::Distance(
            out.map.points[&ends(&s, r.lines[0]).0],
            out.map.points[&ends(&s, r.lines[0]).1],
            30.0
        )
    );
    assert!(!clip.three_d);
}

#[test]
fn extract_has_own_dense_ids() {
    let mut s = sketch();
    for i in 0..50 {
        let l = line(&mut s, v(i as f64, 100.0), v(i as f64, 110.0));
        s.remove_curve(l).unwrap();
    }
    let r = rect(&mut s);
    assert!(r.lines[0].index() >= 50);
    let out = s.extract(&curves_only(&r.lines)).unwrap();
    let clip = &out.clip;
    let mut curves: Vec<usize> = clip.curves().map(|(c, _)| c.index()).collect();
    curves.sort_unstable();
    assert_eq!(curves, (0..4).collect::<Vec<_>>());
    let mut points: Vec<usize> = clip.points().map(|(p, _)| p.index()).collect();
    points.sort_unstable();
    assert_eq!(points, (0..8).collect::<Vec<_>>());
    // Keine gelöschten Einträge, keine fremden Werte.
    let d = clip.to_data();
    assert_eq!(d.curves.len(), 4);
    assert_eq!(d.points.len(), 8);
    assert!(d.constraints.iter().all(|c| c.alive));
    assert!(d.vars.len() <= 16, "{} Werte", d.vars.len());
    assert_eq!(clip.name, "");
}

#[test]
fn extract_keeps_style_and_fixed_spline() {
    let mut s = sketch();
    let construction = line(&mut s, v(0.0, 0.0), v(10.0, 0.0));
    s.set_construction(construction, true).unwrap();
    let (fixed_corner, _) = ends(&s, construction);
    s.add_constraint(K::Fixed(fixed_corner, 0.0, 0.0)).unwrap();
    let centerline = line(&mut s, v(0.0, 5.0), v(10.0, 5.0));
    s.set_centerline(centerline, true).unwrap();
    let m = pt(&mut s, 30.0, 0.0);
    let circle = s.add_circle(m, 4.0).unwrap();
    let comb = CombSettings {
        density: 33,
        scale: 2.0,
    };
    s.set_comb(circle, Some(comb)).unwrap();
    let wave = NurbsCurve::new(
        3,
        vec![0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0],
        vec![
            DVec3::new(0.0, 20.0, 0.0),
            DVec3::new(3.0, 25.0, 0.0),
            DVec3::new(6.0, 18.0, 0.0),
            DVec3::new(9.0, 22.0, 0.0),
        ],
        vec![1.0; 4],
    )
    .unwrap();
    let edge = Curve::Nurbs(Arc::new(wave));
    let fixed = s
        .add_projected_curve(
            &edge,
            0.0,
            1.0,
            ProjectStyle {
                construction: false,
            },
        )
        .unwrap();
    let before = s.revision();

    let out = s
        .extract(&curves_only(&[construction, centerline, circle, fixed]))
        .unwrap();
    assert_eq!(s.revision(), before, "die Quelle bleibt unverändert");
    let clip = &out.clip;
    let style = |c: CurveId| clip.curve(out.map.curves[&c]).unwrap().style();
    assert_eq!(style(construction), LineStyle::Construction);
    assert_eq!(style(centerline), LineStyle::Centerline);
    assert_eq!(style(circle), s.curve(circle).unwrap().style());
    assert_eq!(
        clip.curve(out.map.curves[&circle]).unwrap().comb,
        Some(comb)
    );
    assert_eq!(
        clip.curve(out.map.curves[&fixed]).unwrap().kind.clone(),
        match &s.curve(fixed).unwrap().kind {
            CurveKind::FixedSpline { start, end, curve } => CurveKind::FixedSpline {
                start: out.map.points[start],
                end: out.map.points[end],
                curve: curve.clone(),
            },
            k => panic!("kein fester Spline: {k:?}"),
        }
    );
    // Ein fester Spline braucht feste Enden (sonst wäre er keiner): Nur diese beiden bleiben fest,
    // ein `Fixed` an einem anderen Punkt nicht.
    let CurveKind::FixedSpline { start, end, .. } =
        clip.curve(out.map.curves[&fixed]).unwrap().kind
    else {
        unreachable!()
    };
    let held: Vec<PointId> = clip
        .constraints()
        .filter_map(|(_, d)| d.kind.fixed_point())
        .collect();
    assert_eq!(held.len(), 2);
    assert!(held.contains(&start) && held.contains(&end));
    let mut target = sketch();
    let map = target.paste_from(clip).unwrap();
    assert!(matches!(
        target
            .curve(map.curves[&out.map.curves[&fixed]])
            .unwrap()
            .kind,
        CurveKind::FixedSpline { .. }
    ));
}

#[test]
fn extract_text_with_box() {
    let mut s = sketch();
    let t = text_box(&mut s, "AB");
    let source = s.text(t).unwrap().clone();
    let out = s
        .extract(&ClipSet {
            texts: vec![t],
            ..ClipSet::default()
        })
        .unwrap();
    let clip = &out.clip;
    let texts: Vec<TextId> = clip.texts().map(|(t, _)| t).collect();
    assert_eq!(texts.len(), 1);
    let copy = clip.text(texts[0]).unwrap();
    assert_eq!(out.map.texts[&t], texts[0]);
    assert_eq!(copy.content, "AB");
    assert_eq!(copy.height, source.height);
    assert_eq!(copy.curves.len(), source.curves.len());
    let TextPlacement::Box { lines, .. } = copy.placement else {
        panic!("kein Rahmen");
    };
    for l in lines {
        let d = clip.curve(l).expect("Rahmenlinie im Clip");
        assert!(d.construction);
    }
    assert_eq!(clip.curves().count(), 4 + source.curves.len());
}

#[test]
fn extract_text_on_path_without_path_refused() {
    let mut s = sketch();
    let path = line(&mut s, v(0.0, 0.0), v(50.0, 0.0));
    let p = params("AB");
    let shape = shape_of(&p);
    let t = s
        .add_text_path(path, p, true, false, HAlign::Left, shape)
        .unwrap();
    let before = s.revision();
    let letters = s.text(t).unwrap().curves.clone();
    let err = s
        .extract(&ClipSet {
            texts: vec![t],
            ..ClipSet::default()
        })
        .unwrap_err();
    assert_eq!(
        err,
        SketchError::InvalidInput(tr!("sketch-text-path-follows"))
    );
    assert_eq!(s.revision(), before);
    // Über die Buchstaben gewählt ebenso; mit dem Pfad geht es.
    assert!(s.extract(&curves_only(&letters)).is_err());
    let mut with_path = letters.clone();
    with_path.push(path);
    let out = s.extract(&curves_only(&with_path)).unwrap();
    assert_eq!(out.clip.texts().count(), 1);
    // Leere Menge.
    assert_eq!(
        s.extract(&ClipSet::default()).unwrap_err(),
        SketchError::InvalidInput(tr!("sketch-nothing-selected"))
    );
    assert_eq!(s.revision(), before);
}

// ---- Einfügen ----------------------------------------------------------------------------------

/// Kurven der Kopie aus `map` (Clip → Ziel).
fn copied_curves(map: &CopyMap) -> HashSet<CurveId> {
    map.curves.values().copied().collect()
}

#[test]
fn paste_twice_gives_two_independent_copies() {
    let mut src = sketch();
    let r = rect(&mut src);
    let clip = src.extract(&curves_only(&r.lines)).unwrap().clip;

    let mut target = sketch();
    let own = line(&mut target, v(-5.0, -5.0), v(-1.0, -9.0));
    let own_ends = ends(&target, own);
    let own_xy = (
        target.point_xy(own_ends.0).unwrap(),
        target.point_xy(own_ends.1).unwrap(),
    );
    let before = target.revision();
    let first = target.paste_from(&clip).unwrap();
    assert!(target.revision() != before);
    let second = target.paste_from(&clip).unwrap();

    assert_eq!(target.curves().count(), 1 + 8);
    assert_eq!(count(&target, |k| matches!(k, K::Distance(..))), 4);
    assert_eq!(count(&target, |k| matches!(k, K::Coincident(..))), 8);
    // Die alte Linie bleibt, wie sie war.
    assert_eq!(ends(&target, own), own_ends);
    assert_eq!(target.point_xy(own_ends.0).unwrap(), own_xy.0);
    assert_eq!(target.point_xy(own_ends.1).unwrap(), own_xy.1);
    // Keine Abhängigkeit verbindet die beiden Kopien oder die alte Linie.
    let (a, b) = (copied_curves(&first), copied_curves(&second));
    assert!(a.is_disjoint(&b));
    let pa: HashSet<PointId> = first.points.values().copied().collect();
    let pb: HashSet<PointId> = second.points.values().copied().collect();
    assert!(pa.is_disjoint(&pb));
    for (_, d) in target.constraints() {
        let in_a = d.kind.curves().iter().all(|c| a.contains(c))
            && d.kind.points().iter().all(|p| pa.contains(p));
        let in_b = d.kind.curves().iter().all(|c| b.contains(c))
            && d.kind.points().iter().all(|p| pb.contains(p));
        assert!(in_a || in_b, "{:?} hängt an beiden", d.kind);
    }
    // Die Abbildung führt vom Clip ins Ziel.
    for (&from, &to) in &first.points {
        assert_eq!(clip.point_xy(from).unwrap(), target.point_xy(to).unwrap());
    }
}

#[test]
fn paste_keeps_driven_and_disabled() {
    let mut src = sketch();
    let l = line(&mut src, v(0.0, 0.0), v(10.0, 1.0));
    let (a, b) = ends(&src, l);
    let shown = src
        .add_constraint(K::Distance(a, b, 10.0_f64.hypot(1.0)))
        .unwrap();
    src.set_driven(shown, true).unwrap();
    let off = src
        .add_constraint(K::Horizontal(LineOrPoints::Line(l)))
        .unwrap();
    src.set_enabled(off, false).unwrap();

    let ex = src.extract(&curves_only(&[l])).unwrap();
    let mut target = sketch();
    let map = target.paste_from(&ex.clip).unwrap();
    let shown_t = map.constraints[&ex.map.constraints[&shown]];
    let off_t = map.constraints[&ex.map.constraints[&off]];
    let d = target.constraint(shown_t).unwrap();
    assert!(d.driven && d.enabled);
    let d = target.constraint(off_t).unwrap();
    assert!(!d.enabled && !d.driven);
}

#[test]
fn paste_3d_clip_into_2d_sketch_errors() {
    let mut src = sketch();
    src.set_three_d(true);
    let a = src.add_point3(DVec3::new(0.0, 0.0, 0.0)).unwrap();
    let b = src.add_point3(DVec3::new(10.0, 0.0, 5.0)).unwrap();
    let l = src.add_line(a, b).unwrap();
    let clip = src.extract(&curves_only(&[l])).unwrap().clip;
    assert!(clip.three_d);

    let mut target = sketch();
    let own = line(&mut target, v(0.0, 0.0), v(1.0, 1.0));
    let before = target.revision();
    assert_eq!(
        target.paste_from(&clip).unwrap_err(),
        SketchError::InvalidInput(tr!("sketch-err-tool-not-in-3d"))
    );
    assert_eq!(target.revision(), before);
    assert_eq!(all_curves(&target), vec![own]);

    // Nach dem Umschalten (das macht der Aufrufer) geht es, die Lage im Raum bleibt.
    target.set_three_d(true);
    let map = target.paste_from(&clip).unwrap();
    for (&from, &to) in &map.points {
        assert_eq!(clip.point_xyz(from).unwrap(), target.point_xyz(to).unwrap());
    }
    // 2D-Inhalt aus einer 3D-Skizze bleibt 2D.
    let flat = src.add_point(3.0, 3.0).unwrap();
    let flat_end = src.add_point(4.0, 7.0).unwrap();
    let f = src.add_line(flat, flat_end).unwrap();
    assert!(!src.extract(&curves_only(&[f])).unwrap().clip.three_d);
}

#[test]
fn paste_solves_like_source() {
    let mut src = sketch();
    let r = rect(&mut src);
    let m = pt(&mut src, 15.0, 10.0);
    let circle = src.add_circle(m, 3.0).unwrap();
    src.add_constraint(K::Radius(circle, 3.0)).unwrap();
    src.solve();
    let mut set = r.lines.to_vec();
    set.push(circle);
    let ex = src.extract(&curves_only(&set)).unwrap();

    let mut target = Sketch::new(
        Frame::from_origin_normal(DVec3::new(0.0, 0.0, 7.0), DVec3::Z),
        "Ziel",
        Tolerance::default(),
    );
    line(&mut target, v(100.0, 0.0), v(120.0, 0.0));
    let map = target.paste_from(&ex.clip).unwrap();
    target.solve();
    for (&from, &mid) in &ex.map.points {
        let to = map.points[&mid];
        let d = src
            .point_xy(from)
            .unwrap()
            .distance(target.point_xy(to).unwrap());
        assert!(d < 1e-9, "{d}");
    }
    let _ = r.height;
}

#[test]
fn spline_handles_and_pattern_instances_come_along() {
    use planora_sketch::pattern::{PatternDir, PatternKind, PatternPlan};
    let mut s = sketch();
    let pts: Vec<PointId> = [(0.0, 0.0), (5.0, 4.0), (10.0, -2.0), (15.0, 3.0)]
        .iter()
        .map(|&(x, y)| pt(&mut s, x, y))
        .collect();
    let spline = s.add_spline(&pts).unwrap();
    s.activate_curvature_handle(spline, pts[1]).unwrap();
    let source = line(&mut s, v(0.0, 30.0), v(5.0, 30.0));
    let pattern = s
        .add_pattern(&PatternPlan {
            kind: PatternKind::Rect {
                dirs: [PatternDir::SketchX, PatternDir::SketchY],
                extent: [false, false],
                symmetric: [false, false],
            },
            sources: vec![source],
            counts: [3, 1],
            values: [10.0, 10.0],
            suppressed: Vec::new(),
        })
        .unwrap();
    let instance = s.pattern(pattern).unwrap().instances[0].curves[0];

    // Der Spline zieht seine Griffe mit; ein Exemplar wird gewöhnliche Geometrie.
    let out = s.extract(&curves_only(&[spline, instance])).unwrap();
    let clip = &out.clip;
    let CurveKind::Spline { handles, .. } = &clip.curve(out.map.curves[&spline]).unwrap().kind
    else {
        panic!("kein Spline");
    };
    assert_eq!(handles.len(), 1);
    assert!(clip.curve(handles[0].line).is_some());
    assert_eq!(clip.patterns().count(), 0);
    assert!(clip.curve(out.map.curves[&instance]).is_some());
    let mut target = sketch();
    let map = target.paste_from(clip).unwrap();
    assert_eq!(target.curves().count(), clip.curves().count());
    assert_eq!(map.curves.len(), clip.curves().count());
}
