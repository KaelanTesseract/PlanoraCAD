//! Stufe 29: Randschleifen der Schnittflächen als Vielecke (`section.rs`, Teil 2).

use planora_drawing::section::*;
use planora_drawing::stroke::{Stroke, StrokeOrigin};
use planora_drawing::{Basis, ViewKind};
use planora_kernel::{AnalyticKernel, BoolOp, Curve, Kernel, Solid};
use planora_math::nurbs::NurbsCurve;
use planora_math::{DMat4, DVec2, DVec3, Tolerance};
use std::f64::consts::PI;
use std::sync::Arc;

fn k() -> AnalyticKernel {
    AnalyticKernel::default()
}

fn moved(s: &Solid, x: f64, y: f64, z: f64) -> Solid {
    s.transformed(&DMat4::from_translation(DVec3::new(x, y, z)))
        .unwrap()
}

fn diff(a: &Solid, b: &Solid) -> Solid {
    let mut s = k().boolean(a, b, BoolOp::Difference).unwrap();
    s.assign_origin(7);
    s
}

fn boxs(a: f64, b: f64, c: f64) -> Solid {
    let mut s = k().make_box(DVec3::new(a, b, c)).unwrap();
    s.assign_origin(7);
    s
}

fn tube() -> Solid {
    let outer = k().make_cylinder(10.0, 20.0).unwrap();
    let inner = moved(&k().make_cylinder(6.0, 24.0).unwrap(), 0.0, 0.0, -2.0);
    diff(&outer, &inner)
}

fn cut_at(s: &Solid, body: usize, kind: ViewKind, offset: f64) -> Solid {
    match cut_body(
        s,
        body,
        &SectionPlane { kind, offset },
        &Tolerance::default(),
    )
    .unwrap()
    {
        CutOutcome::Cut { solid, .. } => solid,
        o => panic!("Cut erwartet, bekommen {o:?}"),
    }
}

fn polys(r: &SectionRegion, b: &Basis, tol: f64) -> Vec<Vec<DVec2>> {
    r.loops.iter().map(|l| loop_polygon(l, b, tol)).collect()
}

#[test]
fn box_region_is_ccw_in_view() {
    let cut = cut_at(&boxs(20.0, 10.0, 5.0), 0, ViewKind::Front, 4.0);
    let regions = section_regions(&cut, 0);
    assert_eq!(regions.len(), 1);
    assert_eq!(regions[0].loops.len(), 1);
    let p = loop_polygon(&regions[0].loops[0], &Basis::of(ViewKind::Front), 1e-4);
    assert_eq!(p.len(), 4);
    let a = polygon_area(&p);
    assert!(a > 0.0 && (a - 100.0).abs() < 1e-9, "{a}");
}

#[test]
fn tube_region_has_hole() {
    let cut = cut_at(&tube(), 0, ViewKind::Top, 10.0);
    let regions = section_regions(&cut, 0);
    assert_eq!(regions.len(), 1);
    assert_eq!(regions[0].loops.len(), 2);
    let tol = 1e-4;
    let ps = polys(&regions[0], &Basis::of(ViewKind::Top), tol);
    let (a0, a1) = (polygon_area(&ps[0]), polygon_area(&ps[1]));
    assert!((a0 - 100.0 * PI).abs() < 0.01, "{a0}");
    assert!((a1 + 36.0 * PI).abs() < 0.01, "{a1}");
    assert!((a0 + a1 - 64.0 * PI).abs() < 0.01);
    for (p, r) in ps.iter().zip([10.0, 6.0]) {
        for q in p {
            assert!((q.length() - r).abs() < 1e-12, "{q:?}");
        }
        // Größter Sehnenabstand: Radius minus Abstand der Sehne vom Mittelpunkt.
        let n = p.len();
        let worst = (0..n)
            .map(|i| r - ((p[i] + p[(i + 1) % n]) * 0.5).length())
            .fold(0.0, f64::max);
        assert!(worst <= tol, "{worst}");
        assert!(worst > 0.5 * tol, "Vieleck unnötig fein: {worst}");
    }
}

#[test]
fn u_profile_loop() {
    let plate = boxs(40.0, 20.0, 10.0);
    let groove = moved(
        &k().make_box(DVec3::new(8.0, 22.0, 7.0)).unwrap(),
        16.0,
        -1.0,
        5.0,
    );
    let cut = cut_at(&diff(&plate, &groove), 0, ViewKind::Front, 10.0);
    let regions = section_regions(&cut, 0);
    assert_eq!(regions.len(), 1);
    assert_eq!(regions[0].loops.len(), 1);
    let p = loop_polygon(&regions[0].loops[0], &Basis::of(ViewKind::Front), 1e-4);
    assert_eq!(p.len(), 8);
    assert!((polygon_area(&p) - 360.0).abs() < 1e-9);
}

#[test]
fn two_pieces_two_regions() {
    let cut = cut_at(&tube(), 0, ViewKind::Front, 0.0);
    let regions = section_regions(&cut, 0);
    assert_eq!(regions.len(), 2);
    for r in &regions {
        assert_eq!(r.loops.len(), 1);
        let a = polygon_area(&loop_polygon(
            &r.loops[0],
            &Basis::of(ViewKind::Front),
            1e-4,
        ));
        assert!((a - 80.0).abs() < 1e-5, "{a}");
    }
}

#[test]
fn body_index_kept() {
    let cut = cut_at(&tube(), 4, ViewKind::Front, 0.0);
    let regions = section_regions(&cut, 4);
    assert!(!regions.is_empty());
    for r in &regions {
        assert_eq!(r.body, 4);
        for lp in &r.loops {
            let mut seen: Vec<usize> = lp
                .iter()
                .map(|s| match &s.origin {
                    StrokeOrigin::Edge { edge, .. } => *edge,
                    o => panic!("{o:?}"),
                })
                .collect();
            seen.sort_unstable();
            seen.dedup();
            assert_eq!(seen.len(), lp.len(), "Kantenindizes einer Schleife");
            // Der Index nennt wirklich die Kante, aus der das Stück stammt.
            for s in lp {
                let StrokeOrigin::Edge { edge, .. } = &s.origin else {
                    unreachable!()
                };
                let (_, e) = cut.edges.iter().nth(*edge).unwrap();
                let (a, b) = (e.curve.point(e.t0), e.curve.point(e.t1));
                let (p, q) = (s.point(s.t0), s.point(s.t1));
                assert!(
                    ((p - a).length() < 1e-9 && (q - b).length() < 1e-9)
                        || ((p - b).length() < 1e-9 && (q - a).length() < 1e-9)
                );
            }
        }
        for s in r.loops.iter().flatten() {
            match &s.origin {
                StrokeOrigin::Edge { body, edge } => {
                    assert_eq!(*body, 4);
                    assert!(*edge < cut.edges.len());
                }
                o => panic!("{o:?}"),
            }
        }
    }
}

#[test]
fn tolerance_controls_points() {
    let cut = cut_at(&tube(), 0, ViewKind::Top, 10.0);
    let r = &section_regions(&cut, 0)[0];
    let b = Basis::of(ViewKind::Top);
    let count = |tol: f64| loop_polygon(&r.loops[0], &b, tol).len();
    let (coarse, fine, fallback) = (count(1e-2), count(1e-4), count(1e-6));
    assert!(
        coarse < fine && fine < fallback,
        "{coarse} {fine} {fallback}"
    );
    for bad in [0.0, -1.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert_eq!(count(bad), fallback, "{bad}");
    }
    // Sehnenabstand r(1 - cos(pi/n)) bei n Stücken: 1e-2 → n >= 71 für r = 10.
    assert!((70..=75).contains(&coarse), "{coarse}");
}

#[test]
fn backward_edges_are_reversed() {
    let cut = cut_at(&boxs(20.0, 10.0, 5.0), 0, ViewKind::Front, 4.0);
    let r = &section_regions(&cut, 0)[0];
    let b = Basis::of(ViewKind::Front);
    let lp = &r.loops[0];
    // Jede Kante beginnt dort, wo die vorige endet.
    for i in 0..lp.len() {
        let (s, n) = (&lp[i], &lp[(i + 1) % lp.len()]);
        let end = s.point2(&b, s.t1);
        let start = n.point2(&b, n.t0);
        assert!((end - start).length() < 1e-9, "{i}");
    }
}

#[test]
fn freeform_is_refined() {
    let ctrl = [
        (0.0, 1.0),
        (2.0, 4.0),
        (5.0, -2.0),
        (7.0, 3.0),
        (9.0, 0.0),
        (12.0, 5.0),
    ];
    let pts: Vec<DVec3> = ctrl.iter().map(|c| DVec3::new(c.0, c.1, 0.0)).collect();
    let knots = vec![0.0, 0.0, 0.0, 0.0, 0.3, 0.6, 1.0, 1.0, 1.0, 1.0];
    let c = Arc::new(NurbsCurve::new(3, knots, pts, vec![1.0; 6]).unwrap());
    let s = Stroke {
        curve: Curve::Nurbs(c),
        t0: 0.0,
        t1: 1.0,
        origin: StrokeOrigin::Edge { body: 0, edge: 0 },
    };
    let b = Basis::of(ViewKind::Top);
    let tol = 1e-4;
    let mut poly = loop_polygon(std::slice::from_ref(&s), &b, tol);
    assert!(poly.len() > 64 && poly.len() <= 4096, "{}", poly.len());
    // Der Schlusspunkt gehört der nächsten Kante; hier den Linienzug damit schließen.
    poly.push(s.point2(&b, 1.0));
    // Dichte Kurvenpunkte liegen höchstens tol (plus Rundung) vom Linienzug entfernt.
    let dist = |q: DVec2| {
        poly.windows(2)
            .map(|w| {
                let ab = w[1] - w[0];
                let u = ((q - w[0]).dot(ab) / ab.length_squared()).clamp(0.0, 1.0);
                (q - (w[0] + ab * u)).length()
            })
            .fold(f64::INFINITY, f64::min)
    };
    for i in 0..=2000 {
        let q = s.point2(&b, i as f64 / 2000.0);
        assert!(dist(q) <= tol * 1.05, "{i}: {}", dist(q));
    }
    // Mit grober Toleranz weniger Punkte.
    assert!(loop_polygon(std::slice::from_ref(&s), &b, 0.5).len() < poly.len());
}

#[test]
fn polygon_area_is_signed() {
    let sq = [
        DVec2::new(0.0, 0.0),
        DVec2::new(2.0, 0.0),
        DVec2::new(2.0, 3.0),
        DVec2::new(0.0, 3.0),
    ];
    assert_eq!(polygon_area(&sq), 6.0);
    let rev: Vec<DVec2> = sq.iter().rev().copied().collect();
    assert_eq!(polygon_area(&rev), -6.0);
    assert_eq!(polygon_area(&[]), 0.0);
}

fn ellipse_stroke(a: f64, b: f64) -> Stroke {
    Stroke {
        curve: Curve::Ellipse {
            frame: planora_math::Frame::WORLD,
            major: a,
            minor: b,
        },
        t0: 0.0,
        t1: 2.0 * PI,
        origin: StrokeOrigin::Edge { body: 0, edge: 0 },
    }
}

#[test]
fn ellipse_uses_largest_radius_and_area() {
    let (a, b, tol) = (10.0, 4.0, 1e-4);
    let s = ellipse_stroke(a, b);
    let bas = Basis::of(ViewKind::Top);
    let poly = loop_polygon(std::slice::from_ref(&s), &bas, tol);
    // Stückzahl nach dem größten Radius (a), nicht nach b.
    let want = (2.0 * PI / (2.0 * (1.0 - tol / a).acos())).ceil() as usize;
    assert_eq!(poly.len(), want);
    // Alle Punkte liegen auf der Ellipse.
    for q in &poly {
        let e = (q.x / a).powi(2) + (q.y / b).powi(2);
        assert!((e - 1.0).abs() < 1e-12, "{q:?}");
    }
    // Dichte Kurvenpunkte liegen höchstens tol vom Vieleck.
    let n = poly.len();
    for i in 0..4000 {
        let t = 2.0 * PI * i as f64 / 4000.0;
        let q = DVec2::new(a * t.cos(), b * t.sin());
        let d = (0..n)
            .map(|j| {
                let (p0, p1) = (poly[j], poly[(j + 1) % n]);
                let ab = p1 - p0;
                let u = ((q - p0).dot(ab) / ab.length_squared()).clamp(0.0, 1.0);
                (q - (p0 + ab * u)).length()
            })
            .fold(f64::INFINITY, f64::min);
        assert!(d <= tol, "{i}: {d}");
    }
    let area = polygon_area(&poly);
    assert!(area > 0.0 && (area - PI * a * b).abs() < 0.01, "{area}");
}

#[test]
fn plate_with_two_bores_has_two_holes() {
    let plate = boxs(40.0, 20.0, 10.0);
    let d1 = moved(&k().make_cylinder(3.0, 14.0).unwrap(), 10.0, 10.0, -2.0);
    let d2 = moved(&k().make_cylinder(2.0, 14.0).unwrap(), 28.0, 10.0, -2.0);
    let s = diff(&diff(&plate, &d1), &d2);
    let cut = cut_at(&s, 0, ViewKind::Top, 5.0);
    let regions = section_regions(&cut, 0);
    assert_eq!(regions.len(), 1);
    assert_eq!(regions[0].loops.len(), 3);
    let ps = polys(&regions[0], &Basis::of(ViewKind::Top), 1e-4);
    let areas: Vec<f64> = ps.iter().map(|p| polygon_area(p)).collect();
    assert!(
        areas[0] > 0.0 && areas[1] < 0.0 && areas[2] < 0.0,
        "{areas:?}"
    );
    assert!((areas[0] - 800.0).abs() < 1e-9);
    // Jedes Loch ist ein Kreis um seine Mitte; die Reihenfolge bleibt die der Flächenschleifen
    // (Bohrung bei x = 10 zuerst).
    let mut found = Vec::new();
    for p in &ps[1..] {
        let c = p.iter().fold(DVec2::ZERO, |s, q| s + *q) / p.len() as f64;
        let r = (p[0] - c).length();
        assert!(p.iter().all(|q| ((*q - c).length() - r).abs() < 1e-9));
        found.push((c.x.round(), (r * 1e6).round() / 1e6));
    }
    assert_eq!(found, vec![(10.0, 3.0), (28.0, 2.0)]);
    let again = polys(
        &section_regions(&cut, 0)[0],
        &Basis::of(ViewKind::Top),
        1e-4,
    );
    assert_eq!(again, ps, "Reihenfolge ist stabil");
}

#[test]
fn other_bases_mirror_axes() {
    // Quader 20 x 10 x 5; Sollecken von Hand aus Basis::of (ex = d x ey).
    let b = boxs(20.0, 10.0, 5.0);
    let cases = [
        // (Art, Versatz, Ecken in Ansichtskoordinaten)
        (
            ViewKind::Back,
            4.0,
            [(-20.0, 0.0), (0.0, 0.0), (0.0, 5.0), (-20.0, 5.0)],
        ),
        (
            ViewKind::Left,
            8.0,
            [(-10.0, 0.0), (0.0, 0.0), (0.0, 5.0), (-10.0, 5.0)],
        ),
        (
            ViewKind::Right,
            8.0,
            [(0.0, 0.0), (10.0, 0.0), (10.0, 5.0), (0.0, 5.0)],
        ),
        (
            ViewKind::Bottom,
            2.0,
            [(0.0, -10.0), (20.0, -10.0), (20.0, 0.0), (0.0, 0.0)],
        ),
    ];
    for (kind, off, want) in cases {
        let cut = cut_at(&b, 0, kind, off);
        let regions = section_regions(&cut, 0);
        assert_eq!(regions.len(), 1, "{kind:?}");
        let p = loop_polygon(&regions[0].loops[0], &Basis::of(kind), 1e-4);
        assert_eq!(p.len(), 4, "{kind:?}");
        assert!(polygon_area(&p) > 0.0, "{kind:?}");
        for w in want {
            assert!(
                p.iter()
                    .any(|q| (q.x - w.0).abs() < 1e-9 && (q.y - w.1).abs() < 1e-9),
                "{kind:?} fehlt {w:?} in {p:?}"
            );
        }
    }
}
