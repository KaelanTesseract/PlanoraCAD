//! Geometrie aus DXF und SVG in die Skizze einfügen (Skizzen 9, Task 6, Spec B).
//!
//! Der Test-Setzer [`SquareShaper`] liefert je Zeichen ein Quadrat; die Schrift „Fehlt“ gilt als
//! ersetzt. So prüft die Skizze Texte ohne `planora-text` und ohne Schriften.

use planora_exchange::import2d::{Element2d, HAlign2d, Import2d, Layer2d, Text2d, VAlign2d};
use planora_exchange::read_dxf;
use planora_math::nurbs::NurbsCurve;
use planora_math::{DVec2, DVec3, Frame, Tolerance};
use planora_sketch::constraint::ConstraintKind as K;
use planora_sketch::import::{InsertOptions, InsertReport, Placement, preview_polylines};
use planora_sketch::model::{CurveId, CurveKind, PointId, Sketch};
use planora_sketch::text_shape::{
    Contour, Glyph, Seg, ShapedLine, TextShape, TextShapeError, TextShaper, TextStyle,
};
use std::collections::HashSet;
use std::f64::consts::{FRAC_PI_2, PI, TAU};

fn v(x: f64, y: f64) -> DVec2 {
    DVec2::new(x, y)
}

/// Test-Setzer: je Zeichen ein Quadrat 0,6 × 0,7, Vorschub 0,8; Leerzeichen ohne Umriss.
struct SquareShaper;

impl TextShaper for SquareShaper {
    fn shape(&self, content: &str, style: &TextStyle) -> Result<TextShape, TextShapeError> {
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
                    vec![Contour {
                        start: v(0.0, 0.0),
                        segs: vec![
                            Seg::Line(v(0.6, 0.0)),
                            Seg::Line(v(0.6, 0.7)),
                            Seg::Line(v(0.0, 0.7)),
                            Seg::Line(v(0.0, 0.0)),
                        ],
                    }]
                };
                glyphs.push(Glyph {
                    origin: v(pen, 0.0),
                    advance: 0.8,
                    contours,
                });
                pen += 0.8;
            }
            lines.push(ShapedLine { glyphs, width: pen });
        }
        Ok(TextShape {
            lines,
            ascender: 0.9,
            descender: -0.2,
            line_height: 1.2,
            substituted: (style.font == "Fehlt").then(|| style.font.clone()),
            synthetic: false,
            missing_chars: false,
        })
    }
}

fn sketch() -> Sketch {
    Sketch::new(Frame::WORLD, "S", Tolerance::default())
}

fn one_layer(elements: Vec<Element2d>) -> Import2d {
    Import2d {
        layers: vec![Layer2d {
            name: "0".into(),
            elements,
        }],
        ..Default::default()
    }
}

fn free() -> InsertOptions {
    InsertOptions::default()
}

fn fixed() -> InsertOptions {
    InsertOptions {
        fixed: true,
        ..Default::default()
    }
}

fn insert(s: &mut Sketch, imp: &Import2d, opt: &InsertOptions) -> InsertReport {
    s.insert_geometry(imp, &Placement::default(), opt, &SquareShaper)
        .expect("einfügen")
}

fn only_curve(s: &Sketch) -> (CurveId, CurveKind) {
    let all: Vec<_> = s.curves().map(|(c, d)| (c, d.kind.clone())).collect();
    assert_eq!(all.len(), 1, "{all:?}");
    all[0].clone()
}

fn xy(s: &Sketch, p: PointId) -> DVec2 {
    s.point_xy(p).unwrap()
}

fn near(a: DVec2, b: DVec2) -> bool {
    a.distance(b) < 1e-9
}

fn square() -> Vec<Element2d> {
    vec![
        Element2d::Line(v(0.0, 0.0), v(10.0, 0.0)),
        Element2d::Line(v(10.0, 0.0), v(10.0, 10.0)),
        Element2d::Line(v(10.0, 10.0), v(0.0, 10.0)),
        Element2d::Line(v(0.0, 10.0), v(0.0, 0.0)),
    ]
}

fn fixed_points(s: &Sketch) -> HashSet<PointId> {
    s.constraints()
        .filter_map(|(_, d)| match d.kind {
            K::Fixed(p, ..) => Some(p),
            _ => None,
        })
        .collect()
}

/// Größter Abstand der Proben von `a` zur dicht abgetasteten Kurve `b` (Polygon).
fn max_distance(a: &[DVec2], b: &[DVec2]) -> f64 {
    let seg_dist = |p: DVec2, q0: DVec2, q1: DVec2| {
        let d = q1 - q0;
        let t = if d.length_squared() > 0.0 {
            ((p - q0).dot(d) / d.length_squared()).clamp(0.0, 1.0)
        } else {
            0.0
        };
        p.distance(q0 + d * t)
    };
    a.iter()
        .map(|&p| {
            b.windows(2)
                .map(|w| seg_dist(p, w[0], w[1]))
                .fold(f64::INFINITY, f64::min)
        })
        .fold(0.0, f64::max)
}

fn sample(c: &NurbsCurve, n: usize) -> Vec<DVec2> {
    let (a, b) = c.domain();
    (0..=n)
        .map(|i| c.point(a + (b - a) * i as f64 / n as f64).truncate())
        .collect()
}

fn nurbs2(degree: usize, knots: &[f64], points: &[DVec2], weights: Option<&[f64]>) -> NurbsCurve {
    NurbsCurve::new(
        degree,
        knots.to_vec(),
        points.iter().map(|p| p.extend(0.0)).collect(),
        weights.map_or_else(|| vec![1.0; points.len()], <[f64]>::to_vec),
    )
    .unwrap()
}

#[test]
fn square_of_four_lines_shares_points_and_is_profile() {
    let mut s = sketch();
    let r = insert(&mut s, &one_layer(square()), &free());
    assert_eq!(s.points().count(), 4);
    assert_eq!(r.points, 4);
    assert_eq!(r.curves, 4);
    assert_eq!(r.new_curves.len(), 4);
    assert_eq!(s.profiles().profiles.len(), 1);
}

#[test]
fn nearly_touching_ends_merge_within_tolerance() {
    // Diagonale ≈ 14,1: Toleranz ≈ 1,4e-5. Ein Fehler von 1e-7 wird zusammengeführt, 1e-3 nicht.
    let mut els = square();
    els[1] = Element2d::Line(v(10.0 + 1e-7, 0.0), v(10.0, 10.0));
    let mut s = sketch();
    insert(&mut s, &one_layer(els.clone()), &free());
    assert_eq!(s.points().count(), 4);
    els[1] = Element2d::Line(v(10.0 + 1e-3, 0.0), v(10.0, 10.0));
    let mut s = sketch();
    insert(&mut s, &one_layer(els), &free());
    assert_eq!(s.points().count(), 5);
}

#[test]
fn quad_becomes_conic_rho_half() {
    let mut s = sketch();
    let q = [v(0.0, 0.0), v(1.0, 2.0), v(2.0, 0.0)];
    insert(&mut s, &one_layer(vec![Element2d::Quad(q)]), &free());
    let (_, kind) = only_curve(&s);
    let CurveKind::Conic {
        start,
        end,
        apex,
        rho,
    } = kind
    else {
        panic!("kein Kegelschnitt: {kind:?}");
    };
    assert_eq!(rho, 0.5);
    assert!(near(xy(&s, start), q[0]) && near(xy(&s, apex), q[1]) && near(xy(&s, end), q[2]));
}

#[test]
fn cubic_becomes_control_spline_3() {
    let mut s = sketch();
    let q = [v(0.0, 0.0), v(1.0, 2.0), v(3.0, 2.0), v(4.0, 0.0)];
    insert(&mut s, &one_layer(vec![Element2d::Cubic(q)]), &free());
    let (_, kind) = only_curve(&s);
    let CurveKind::ControlSpline {
        points,
        degree,
        knots,
    } = kind
    else {
        panic!("kein Spline: {kind:?}");
    };
    assert_eq!(degree, 3);
    assert_eq!(knots, vec![0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0]);
    let at: Vec<DVec2> = points.iter().map(|&p| xy(&s, p)).collect();
    assert!(at.iter().zip(q).all(|(a, b)| near(*a, b)), "{at:?}");
}

#[test]
fn closed_cubic_is_split_in_two() {
    let q = [v(0.0, 0.0), v(4.0, 4.0), v(-4.0, 4.0), v(0.0, 0.0)];
    let mut s = sketch();
    let r = insert(&mut s, &one_layer(vec![Element2d::Cubic(q)]), &free());
    assert_eq!(r.curves, 2);
    let want = sample(&nurbs2(3, &[0., 0., 0., 0., 1., 1., 1., 1.], &q, None), 50);
    let mut got = Vec::new();
    for c in &r.new_curves {
        got.extend(sample(&s.nurbs_of(*c).unwrap(), 400));
    }
    assert!(max_distance(&want, &got) < 1e-6);
    assert_eq!(s.profiles().profiles.len(), 1);
}

#[test]
fn nurbs_degree_3_keeps_knots() {
    let knots = [0.0, 0.0, 0.0, 0.0, 0.3, 1.0, 1.0, 1.0, 1.0];
    let pts = vec![
        v(0.0, 0.0),
        v(1.0, 1.0),
        v(2.0, -1.0),
        v(3.0, 1.0),
        v(4.0, 0.0),
    ];
    let mut s = sketch();
    let imp = one_layer(vec![Element2d::Nurbs {
        degree: 3,
        knots: knots.to_vec(),
        points: pts.clone(),
        weights: None,
    }]);
    insert(&mut s, &imp, &free());
    let (_, kind) = only_curve(&s);
    let CurveKind::ControlSpline {
        points,
        degree,
        knots: k,
    } = kind
    else {
        panic!("kein Spline: {kind:?}");
    };
    assert_eq!(degree, 3);
    assert_eq!(k, knots.to_vec());
    assert!(points.iter().zip(&pts).all(|(&p, &q)| near(xy(&s, p), q)));
}

#[test]
fn nurbs_degree_2_elevated_exact() {
    let knots = [0.0, 0.0, 0.0, 0.5, 1.0, 1.0, 1.0];
    let pts = [v(0.0, 0.0), v(1.0, 2.0), v(3.0, 2.0), v(4.0, 0.0)];
    let orig = nurbs2(2, &knots, &pts, None);
    let mut s = sketch();
    let imp = one_layer(vec![Element2d::Nurbs {
        degree: 2,
        knots: knots.to_vec(),
        points: pts.to_vec(),
        weights: None,
    }]);
    insert(&mut s, &imp, &free());
    let (c, kind) = only_curve(&s);
    assert!(
        matches!(kind, CurveKind::ControlSpline { degree: 3, .. }),
        "{kind:?}"
    );
    let got = s.nurbs_of(c).unwrap();
    assert_eq!(got.domain(), orig.domain());
    for i in 0..10 {
        let u = i as f64 / 9.0;
        let d = got.point(u).distance(orig.point(u));
        assert!(d < 1e-9, "u = {u}: {d}");
    }
}

#[test]
fn nurbs_degree_4_elevated_to_5() {
    let knots = [0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0];
    let pts = [
        v(0.0, 0.0),
        v(1.0, 2.0),
        v(2.0, -2.0),
        v(3.0, 2.0),
        v(4.0, 0.0),
    ];
    let orig = nurbs2(4, &knots, &pts, None);
    let mut s = sketch();
    let imp = one_layer(vec![Element2d::Nurbs {
        degree: 4,
        knots: knots.to_vec(),
        points: pts.to_vec(),
        weights: None,
    }]);
    insert(&mut s, &imp, &free());
    let (c, kind) = only_curve(&s);
    assert!(
        matches!(kind, CurveKind::ControlSpline { degree: 5, .. }),
        "{kind:?}"
    );
    let got = s.nurbs_of(c).unwrap();
    for i in 0..10 {
        let u = i as f64 / 9.0;
        assert!(got.point(u).distance(orig.point(u)) < 1e-9);
    }
}

#[test]
fn nurbs_degree_7_split_into_cubics_within_tolerance() {
    let knots: Vec<f64> = [0.0; 8].into_iter().chain([1.0; 8]).collect();
    let pts: Vec<DVec2> = (0..8)
        .map(|i| v(i as f64, if i % 2 == 0 { 0.0 } else { 3.0 }))
        .collect();
    let orig = nurbs2(7, &knots, &pts, None);
    let mut s = sketch();
    let imp = one_layer(vec![Element2d::Nurbs {
        degree: 7,
        knots,
        points: pts,
        weights: None,
    }]);
    let r = insert(&mut s, &imp, &free());
    assert!(r.curves >= 2, "{r:?}");
    let mut got = Vec::new();
    for c in &r.new_curves {
        let d = s.curve(*c).unwrap();
        assert!(
            matches!(d.kind, CurveKind::ControlSpline { degree: 3, .. }),
            "{:?}",
            d.kind
        );
        got.extend(sample(&s.nurbs_of(*c).unwrap(), 2000));
    }
    let (lo, hi) = imp.bounds().unwrap();
    let tol = 1e-6 * lo.distance(hi);
    let dev = max_distance(&sample(&orig, 300), &got);
    assert!(dev < tol, "Abweichung {dev} ≥ {tol}");
    // Zusammenhängend: ein Punkt je Stoß.
    assert_eq!(s.points().count(), 3 * r.curves + 1);
}

#[test]
fn long_nurbs_is_split_exactly_into_control_splines() {
    // 100 Kontrollpunkte, Grad 3: mehr als die 64 eines Splines.
    let n = 100;
    let pts: Vec<DVec2> = (0..n)
        .map(|i| v(i as f64, (i as f64 * 0.7).sin() * 5.0))
        .collect();
    let mut knots = vec![0.0; 4];
    knots.extend((1..n - 3).map(|i| i as f64));
    knots.extend([(n - 3) as f64; 4]);
    let orig = nurbs2(3, &knots, &pts, None);
    let mut s = sketch();
    let imp = one_layer(vec![Element2d::Nurbs {
        degree: 3,
        knots,
        points: pts,
        weights: None,
    }]);
    let r = insert(&mut s, &imp, &free());
    assert!(r.curves >= 2);
    let mut total = 0;
    for c in &r.new_curves {
        let CurveKind::ControlSpline { points, .. } = &s.curve(*c).unwrap().kind else {
            panic!("kein Spline");
        };
        assert!(points.len() <= 64);
        total += points.len();
        let piece = s.nurbs_of(*c).unwrap();
        let (a, b) = piece.domain();
        for i in 0..=20 {
            let u = a + (b - a) * i as f64 / 20.0;
            assert!(piece.point(u).distance(orig.point(u)) < 1e-9);
        }
    }
    // Die Stücke teilen sich ihre Stoßpunkte.
    assert_eq!(s.points().count(), total - (r.curves - 1));
}

#[test]
fn closed_nurbs_is_split() {
    // Geschlossener kubischer Spline: Anfang = Ende.
    let pts = [
        v(0.0, 0.0),
        v(5.0, 0.0),
        v(5.0, 5.0),
        v(0.0, 5.0),
        v(0.0, 0.0),
    ];
    let knots = [0.0, 0.0, 0.0, 0.0, 0.5, 1.0, 1.0, 1.0, 1.0];
    let mut s = sketch();
    let imp = one_layer(vec![Element2d::Nurbs {
        degree: 3,
        knots: knots.to_vec(),
        points: pts.to_vec(),
        weights: None,
    }]);
    let r = insert(&mut s, &imp, &free());
    assert_eq!(r.curves, 2);
    assert_eq!(s.profiles().profiles.len(), 1);
}

#[test]
fn rational_nurbs_becomes_fixed_spline() {
    let w = std::f64::consts::FRAC_1_SQRT_2;
    let mut s = sketch();
    let imp = one_layer(vec![Element2d::Nurbs {
        degree: 2,
        knots: vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0],
        points: vec![v(1.0, 0.0), v(1.0, 1.0), v(0.0, 1.0)],
        weights: Some(vec![1.0, w, 1.0]),
    }]);
    insert(&mut s, &imp, &free());
    let (_, kind) = only_curve(&s);
    let CurveKind::FixedSpline { start, end, curve } = kind else {
        panic!("kein fester Spline: {kind:?}");
    };
    // Ein Viertelkreis, exakt.
    let mid = curve.point(0.5).truncate();
    assert!((mid.length() - 1.0).abs() < 1e-12);
    assert!(near(xy(&s, start), v(1.0, 0.0)) && near(xy(&s, end), v(0.0, 1.0)));
    assert_eq!(fixed_points(&s), HashSet::from([start, end]));
    // Speichern und Laden geht (Enden liegen auf der Kurve und sind fest).
    Sketch::from_data(s.to_data()).expect("laden");
}

#[test]
fn fit_spline_becomes_spline() {
    let pts = vec![v(0.0, 0.0), v(1.0, 1.0), v(2.0, 0.0), v(3.0, 1.0)];
    let mut s = sketch();
    insert(
        &mut s,
        &one_layer(vec![Element2d::FitSpline(pts.clone())]),
        &free(),
    );
    let (_, kind) = only_curve(&s);
    let CurveKind::Spline { points, handles } = kind else {
        panic!("kein Spline: {kind:?}");
    };
    assert!(handles.is_empty());
    assert_eq!(points.len(), 4);
    assert!(points.iter().zip(&pts).all(|(&p, &q)| near(xy(&s, p), q)));
}

#[test]
fn closed_or_long_fit_spline_becomes_fixed_spline() {
    let closed = vec![v(0.0, 0.0), v(4.0, 0.0), v(2.0, 3.0), v(0.0, 0.0)];
    let mut s = sketch();
    insert(
        &mut s,
        &one_layer(vec![Element2d::FitSpline(closed)]),
        &free(),
    );
    let (_, kind) = only_curve(&s);
    let CurveKind::FixedSpline { start, end, curve } = kind else {
        panic!("kein fester Spline: {kind:?}");
    };
    assert_eq!(start, end);
    // Durch die Stützpunkte bei t = 0, 1, 2, 3.
    assert!(curve.point(1.0).truncate().distance(v(4.0, 0.0)) < 1e-9);
    assert!(curve.point(2.0).truncate().distance(v(2.0, 3.0)) < 1e-9);
    assert_eq!(s.profiles().profiles.len(), 1);
    Sketch::from_data(s.to_data()).expect("laden");

    let long: Vec<DVec2> = (0..80).map(|i| v(i as f64, (i % 3) as f64)).collect();
    let mut s = sketch();
    insert(
        &mut s,
        &one_layer(vec![Element2d::FitSpline(long)]),
        &free(),
    );
    let (_, kind) = only_curve(&s);
    assert!(matches!(kind, CurveKind::FixedSpline { .. }), "{kind:?}");
}

#[test]
fn svg_fixed_true_fixes_all_points() {
    let mut els = square();
    els.push(Element2d::Cubic([
        v(0.0, 0.0),
        v(-3.0, 3.0),
        v(-3.0, 7.0),
        v(0.0, 10.0),
    ]));
    let mut s = sketch();
    insert(&mut s, &one_layer(els), &fixed());
    let all: HashSet<PointId> = s.points().map(|(p, _)| p).collect();
    assert_eq!(all.len(), 6);
    assert_eq!(fixed_points(&s), all);
}

#[test]
fn dxf_fixed_false_leaves_free() {
    let mut s = sketch();
    insert(&mut s, &one_layer(square()), &free());
    assert!(fixed_points(&s).is_empty());
}

#[test]
fn merge_only_within_import() {
    let mut s = sketch();
    let a = s.add_point(-5.0, 0.0).unwrap();
    let b = s.add_point(0.0, 0.0).unwrap();
    s.add_line(a, b).unwrap();
    let r = insert(
        &mut s,
        &one_layer(vec![Element2d::Line(v(0.0, 0.0), v(5.0, 0.0))]),
        &free(),
    );
    assert_eq!(r.points, 2);
    assert_eq!(s.points().count(), 4);
}

#[test]
fn mirror_placement_keeps_arcs_correct() {
    let arc = Element2d::Arc {
        c: v(0.0, 0.0),
        r: 1.0,
        start: 0.0,
        end: FRAC_PI_2,
    };
    let place = Placement {
        scale: v(-2.0, 2.0),
        ..Placement::default()
    };
    assert!(place.mirrors());
    let mut s = sketch();
    s.insert_geometry(&one_layer(vec![arc]), &place, &free(), &SquareShaper)
        .unwrap();
    let (_, kind) = only_curve(&s);
    let CurveKind::Arc { center, start, end } = kind else {
        panic!("kein Bogen: {kind:?}");
    };
    // Bild: Viertelkreis im zweiten Quadranten, gegen den Uhrzeigersinn von (0, 2) nach (−2, 0).
    assert!(near(xy(&s, center), v(0.0, 0.0)));
    assert!(near(xy(&s, start), v(0.0, 2.0)), "{:?}", xy(&s, start));
    assert!(near(xy(&s, end), v(-2.0, 0.0)), "{:?}", xy(&s, end));
}

#[test]
fn placement_order_pivot_scale_rotate_offset() {
    let place = Placement {
        scale: v(2.0, 2.0),
        angle: FRAC_PI_2,
        offset: v(10.0, 0.0),
        pivot: v(1.0, 1.0),
    };
    // (2, 1) − Mitte = (1, 0), × 2 = (2, 0), gedreht (0, 2), verschoben (10, 2).
    assert!(near(place.apply(v(2.0, 1.0)), v(10.0, 2.0)));
    assert!(!place.mirrors());
    assert!(near(Placement::default().apply(v(3.0, 4.0)), v(3.0, 4.0)));
}

#[test]
fn uneven_scale_turns_circle_and_arc_into_ellipses() {
    let place = Placement {
        scale: v(2.0, 1.0),
        ..Placement::default()
    };
    let imp = one_layer(vec![
        Element2d::Circle {
            c: v(0.0, 0.0),
            r: 1.0,
        },
        Element2d::Arc {
            c: v(10.0, 0.0),
            r: 1.0,
            start: 0.0,
            end: PI,
        },
    ]);
    let mut s = sketch();
    let r = s
        .insert_geometry(&imp, &place, &free(), &SquareShaper)
        .unwrap();
    let kinds: Vec<_> = r
        .new_curves
        .iter()
        .map(|c| s.curve(*c).unwrap().kind.clone())
        .collect();
    assert!(matches!(kinds[0], CurveKind::Ellipse { .. }), "{kinds:?}");
    let CurveKind::EllipticArc { start, end, .. } = kinds[1] else {
        panic!("kein Ellipsenbogen: {kinds:?}");
    };
    assert!(near(xy(&s, start), v(22.0, 0.0)) && near(xy(&s, end), v(18.0, 0.0)));
    // Oberer Scheitel (20, 1) liegt auf dem Bogen: Parameter zwischen Start und Ende.
    let ts = s.ellipse_param_of(r.new_curves[1], v(22.0, 0.0)).unwrap();
    let top = s.ellipse_point(r.new_curves[1], ts + FRAC_PI_2).unwrap();
    assert!(near(top, v(20.0, 1.0)), "{top:?}");
}

/// Eine selbst geschriebene Text-DXF aus Entitäten („Code Wert“ je Zeile).
fn dxf(entities: &str) -> Vec<u8> {
    let mut out = String::from("  0\nSECTION\n  2\nENTITIES\n");
    for line in entities.lines().map(str::trim).filter(|l| !l.is_empty()) {
        let (code, value) = line.split_once(' ').unwrap_or((line, ""));
        out.push_str(&format!("{code:>3}\n{}\n", value.trim()));
    }
    out.push_str("  0\nENDSEC\n  0\nEOF\n");
    out.into_bytes()
}

#[test]
fn closed_bulge_polyline_gives_one_profile() {
    // Langloch: zwei Strecken und zwei Halbkreise (Bulge 1) als geschlossene LWPOLYLINE.
    let ent = "0 LWPOLYLINE\n8 0\n90 4\n70 1\n\
               10 0\n20 0\n\
               10 10\n20 0\n42 1\n\
               10 10\n20 4\n\
               10 0\n20 4\n42 1";
    let imp = read_dxf(&dxf(ent), &|| false).expect("lesen");
    assert_eq!(imp.element_count(), 4, "{imp:?}");
    let mut s = sketch();
    let r = insert(&mut s, &imp, &free());
    assert_eq!(r.curves, 4);
    let prof = s.profiles();
    assert_eq!(prof.profiles.len(), 1, "{:?}", prof.rejected);
    // Bögen bringen je einen Mittelpunkt mit.
    assert_eq!(s.points().count(), 6);
}

#[test]
fn layer_filter() {
    let imp = Import2d {
        layers: vec![
            Layer2d {
                name: "A".into(),
                elements: square(),
            },
            Layer2d {
                name: "B".into(),
                elements: vec![Element2d::Circle {
                    c: v(5.0, 5.0),
                    r: 2.0,
                }],
            },
        ],
        ..Default::default()
    };
    let mut s = sketch();
    let opt = InsertOptions {
        layers: vec![false, true],
        ..Default::default()
    };
    let r = insert(&mut s, &imp, &opt);
    assert_eq!(r.curves, 1);
    assert!(matches!(only_curve(&s).1, CurveKind::Circle { .. }));
    // Leere Liste: alle Layer.
    let mut s = sketch();
    assert_eq!(insert(&mut s, &imp, &free()).curves, 5);
}

fn text(content: &str, font: &str, height: f64) -> Element2d {
    Element2d::Text(Text2d {
        text: content.into(),
        font: font.into(),
        height,
        pos: v(1.0, 2.0),
        angle: 0.0,
        halign: HAlign2d::Left,
        valign: VAlign2d::Bottom,
    })
}

#[test]
fn dxf_text_becomes_text_box_and_reports_missing_font() {
    let mut els = square();
    els.push(text("AB", "Fehlt", 2.0));
    els.push(text("C", "Da", 1.0));
    let mut s = sketch();
    let r = insert(&mut s, &one_layer(els), &free());
    assert_eq!(r.texts, 2);
    assert_eq!(r.missing_fonts, vec!["Fehlt".to_string()]);
    assert_eq!(r.curves, 4, "Textkurven zählen nicht");
    let (t, d) = s.texts().next().unwrap();
    assert_eq!(d.content, "AB");
    assert_eq!(d.height, 2.0);
    assert_eq!(d.style.font, "Fehlt");
    // Grundlinie links bei (1, 2): die erste Glyphe beginnt dort.
    let first = d.points[0];
    assert!(near(xy(&s, first), v(1.0, 2.0)), "{:?}", xy(&s, first));
    assert!(s.text_angle(t).unwrap().abs() < 1e-12);
}

#[test]
fn rotated_text_keeps_angle() {
    let mut el = text("A", "Da", 1.0);
    if let Element2d::Text(t) = &mut el {
        t.angle = FRAC_PI_2;
    }
    let mut s = sketch();
    insert(&mut s, &one_layer(vec![el]), &free());
    let (t, d) = s.texts().next().unwrap();
    assert!((s.text_angle(t).unwrap() - FRAC_PI_2).abs() < 1e-9);
    assert!(
        near(xy(&s, d.points[0]), v(1.0, 2.0)),
        "{:?}",
        xy(&s, d.points[0])
    );
}

#[test]
fn failure_leaves_sketch_unchanged() {
    let mut s = sketch();
    let a = s.add_point(-5.0, 0.0).unwrap();
    let b = s.add_point(0.0, 3.0).unwrap();
    s.add_line(a, b).unwrap();
    let before = s.clone();
    let mut els = square();
    els.insert(2, text("X", "Da", 0.0));
    let res = s.insert_geometry(
        &one_layer(els),
        &Placement::default(),
        &free(),
        &SquareShaper,
    );
    assert!(res.is_err(), "{res:?}");
    assert_eq!(s.to_data(), before.to_data());
    assert_eq!(s.revision(), before.revision());
}

#[test]
fn invalid_placement_is_error() {
    let mut s = sketch();
    let place = Placement {
        scale: v(0.0, 1.0),
        ..Placement::default()
    };
    let before = s.to_data();
    assert!(
        s.insert_geometry(&one_layer(square()), &place, &free(), &SquareShaper)
            .is_err()
    );
    assert_eq!(s.to_data(), before);
}

#[test]
fn collapsed_pieces_count_as_degenerate() {
    let imp = one_layer(vec![
        Element2d::Line(v(0.0, 0.0), v(10.0, 0.0)),
        Element2d::Line(v(5.0, 5.0), v(5.0, 5.0 + 1e-9)),
    ]);
    let mut s = sketch();
    let r = insert(&mut s, &imp, &free());
    assert_eq!(r.curves, 1);
    assert_eq!(r.skipped.degenerate, 1);
}

fn mixed() -> Import2d {
    one_layer(vec![
        Element2d::Line(v(0.0, 0.0), v(10.0, 0.0)),
        Element2d::Arc {
            c: v(10.0, 5.0),
            r: 5.0,
            start: -FRAC_PI_2,
            end: FRAC_PI_2,
        },
        Element2d::Quad([v(10.0, 10.0), v(5.0, 14.0), v(0.0, 10.0)]),
        Element2d::Cubic([v(0.0, 10.0), v(-2.0, 7.0), v(-2.0, 3.0), v(0.0, 0.0)]),
        Element2d::Circle {
            c: v(5.0, 5.0),
            r: 1.0,
        },
        Element2d::Ellipse {
            c: v(20.0, 5.0),
            major: v(3.0, 1.0),
            ratio: 0.5,
            start: 0.3,
            end: 2.5,
        },
        Element2d::Ellipse {
            c: v(30.0, 5.0),
            major: v(3.0, 1.0),
            ratio: 0.5,
            start: 0.0,
            end: TAU,
        },
        Element2d::FitSpline(vec![v(40.0, 0.0), v(41.0, 2.0), v(43.0, 1.0)]),
        Element2d::Point(v(50.0, 50.0)),
    ])
}

#[test]
fn solve_after_insert_changes_nothing() {
    for opt in [free(), fixed()] {
        let mut s = sketch();
        let r = insert(&mut s, &mixed(), &opt);
        assert_eq!(r.curves, 8);
        let before: Vec<DVec2> = s.points().map(|(p, _)| xy(&s, p)).collect();
        let rep = s.solve();
        assert!(
            matches!(rep.status, planora_sketch::SolveStatus::Solved),
            "{:?}",
            rep.status
        );
        let after: Vec<DVec2> = s.points().map(|(p, _)| xy(&s, p)).collect();
        for (a, b) in before.iter().zip(&after) {
            assert!(a.distance(*b) < 1e-12, "{a:?} → {b:?}");
        }
        assert_eq!(s.profiles().profiles.len(), 3, "Kontur, Kreis, Ellipse");
    }
}

#[test]
fn show_frame_marks_splines_only() {
    let mut s = sketch();
    let opt = InsertOptions {
        show_frame: true,
        ..Default::default()
    };
    let r = insert(&mut s, &mixed(), &opt);
    for c in &r.new_curves {
        let d = s.curve(*c).unwrap();
        let want = matches!(
            d.kind,
            CurveKind::ControlSpline { .. }
                | CurveKind::Conic { .. }
                | CurveKind::FixedSpline { .. }
        );
        assert_eq!(d.show_frame, want, "{:?}", d.kind);
    }
    let mut s = sketch();
    let r = insert(&mut s, &mixed(), &free());
    assert!(
        r.new_curves
            .iter()
            .all(|c| !s.curve(*c).unwrap().show_frame)
    );
}

#[test]
fn preview_polylines_follow_placement_and_layers() {
    let place = Placement {
        offset: v(100.0, 0.0),
        ..Placement::default()
    };
    let lines = preview_polylines(&mixed(), &place, &[], 1e-3);
    assert_eq!(lines.len(), 8, "ohne den Punkt");
    assert!(near(lines[0][0], v(100.0, 0.0)) && near(lines[0][1], v(110.0, 0.0)));
    // Der Kreis: geschlossen, alle Proben im Abstand 1 um (105, 5).
    let circle = &lines[4];
    assert!(circle.len() > 8);
    assert!(near(circle[0], *circle.last().unwrap()));
    assert!(
        circle
            .iter()
            .all(|p| (p.distance(v(105.0, 5.0)) - 1.0).abs() < 1e-9)
    );
    assert!(preview_polylines(&mixed(), &place, &[false], 1e-3).is_empty());
    let _ = DVec3::ZERO;
}

#[test]
fn show_frame_survives_data_roundtrip_and_is_checked() {
    let mut s = sketch();
    let opt = InsertOptions {
        show_frame: true,
        ..Default::default()
    };
    insert(&mut s, &mixed(), &opt);
    let data = s.to_data();
    let back = Sketch::from_data(data.clone()).expect("laden");
    let flags = |s: &Sketch| s.curves().map(|(_, c)| c.show_frame).collect::<Vec<_>>();
    assert_eq!(flags(&back), flags(&s));
    assert!(back.has_frame_entry());
    // Ein Rahmen an einer Linie (ohne Kontrollpolygon) ist kein gültiger Stand.
    let mut bad = data;
    let line = bad
        .curves
        .iter_mut()
        .find(|c| matches!(c.kind, CurveKind::Line { .. }))
        .expect("Linie");
    line.show_frame = true;
    let e = Sketch::from_data(bad).unwrap_err();
    assert!(
        e.to_string()
            .contains(&planora_i18n::tr!("sketch-err-data-show-frame")),
        "{e}"
    );
    // Ohne Rahmen: kein Eintrag.
    let mut s = sketch();
    insert(&mut s, &mixed(), &free());
    assert!(!s.has_frame_entry());
}
