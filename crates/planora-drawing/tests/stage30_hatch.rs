//! Stufe 30: Schraffur-Zuschnitt (`hatch.rs`).

use planora_drawing::hatch::*;
use planora_math::DVec2;
use std::f64::consts::{PI, SQRT_2};

fn v(x: f64, y: f64) -> DVec2 {
    DVec2::new(x, y)
}
fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Vec<DVec2> {
    vec![v(x0, y0), v(x1, y0), v(x1, y1), v(x0, y1)]
}
fn o() -> DVec2 {
    v(0.0, 0.5)
}
fn len(p: &(DVec2, DVec2)) -> f64 {
    (p.1 - p.0).length()
}
fn rev(mut p: Vec<DVec2>) -> Vec<DVec2> {
    p.reverse();
    p
}
fn run(polys: &[Vec<DVec2>], a: f64, s: f64, org: DVec2) -> Vec<(DVec2, DVec2)> {
    hatch_lines(polys, a, s, org, MAX_HATCH_LINES).unwrap()
}

#[test]
fn square() {
    let r = run(&[rect(0.0, 0.0, 10.0, 10.0)], 0.0, 3.0, o());
    assert_eq!(r.len(), 4);
    for (i, p) in r.iter().enumerate() {
        let y = 0.5 + 3.0 * i as f64;
        assert_eq!(p.0, v(0.0, y));
        assert_eq!(p.1, v(10.0, y));
    }
}

#[test]
fn square_45() {
    let r = run(&[rect(0.0, 0.0, 10.0, 10.0)], PI / 4.0, 2.0, v(0.0, 0.0));
    assert_eq!(r.len(), 7);
    let total: f64 = r.iter().map(len).sum();
    let want: f64 = (-3i32..=3)
        .map(|k| (10.0 - 2.0 * SQRT_2 * k.abs() as f64) * SQRT_2)
        .sum();
    assert!((total - want).abs() < 1e-9, "{total} {want}");
    for p in &r {
        for q in [p.0, p.1] {
            let on =
                q.x.abs()
                    .min((q.x - 10.0).abs())
                    .min(q.y.abs())
                    .min((q.y - 10.0).abs());
            assert!(on < 1e-12, "{q:?}");
        }
    }
}

#[test]
fn concave_l() {
    let l = vec![
        v(0., 0.),
        v(10., 0.),
        v(10., 4.),
        v(4., 4.),
        v(4., 10.),
        v(0., 10.),
    ];
    let r = run(&[l], 0.0, 3.0, o());
    let ls: Vec<f64> = r.iter().map(len).collect();
    assert_eq!(ls.len(), 4);
    for (a, b) in ls.iter().zip([10.0, 10.0, 4.0, 4.0]) {
        assert!((a - b).abs() < 1e-12);
    }
}

fn spans(r: &[(DVec2, DVec2)], y: f64) -> Vec<(f64, f64)> {
    r.iter()
        .filter(|p| (p.0.y - y).abs() < 1e-12)
        .map(|p| (p.0.x, p.1.x))
        .collect()
}

#[test]
fn square_with_hole() {
    for hole in [rect(3., 3., 7., 7.), rev(rect(3., 3., 7., 7.))] {
        let r = run(&[rect(0., 0., 10., 10.), hole], 0.0, 3.0, o());
        assert_eq!(r.len(), 6);
        assert_eq!(spans(&r, 0.5), vec![(0.0, 10.0)]);
        assert_eq!(spans(&r, 3.5), vec![(0.0, 3.0), (7.0, 10.0)]);
        assert_eq!(spans(&r, 6.5), vec![(0.0, 3.0), (7.0, 10.0)]);
        assert_eq!(spans(&r, 9.5), vec![(0.0, 10.0)]);
    }
}

#[test]
fn two_disjoint() {
    let r = run(
        &[rect(0., 0., 4., 4.), rect(6., 0., 10., 4.)],
        0.0,
        3.0,
        o(),
    );
    assert_eq!(r.len(), 4);
    assert_eq!(spans(&r, 0.5), vec![(0.0, 4.0), (6.0, 10.0)]);
}

#[test]
fn two_disjoint_two_rows() {
    // Abstand 3, Ursprung y 0,5: Zeilen 0,5 und 3,5 liegen beide in [0,4]
    let r = run(
        &[rect(0., 0., 4., 4.), rect(6., 0., 10., 4.)],
        0.0,
        3.0,
        o(),
    );
    assert_eq!(spans(&r, 3.5), vec![(0.0, 4.0), (6.0, 10.0)]);
}

#[test]
fn through_a_vertex() {
    let d = vec![v(5., 0.), v(10., 5.), v(5., 10.), v(0., 5.)];
    let r = run(&[d], 0.0, 5.0, v(0.0, 5.0));
    assert_eq!(r, vec![(v(0.0, 5.0), v(10.0, 5.0))]);
}

fn circle(r: f64, n: usize) -> Vec<DVec2> {
    (0..n)
        .map(|i| {
            let a = 2.0 * PI * i as f64 / n as f64;
            v(r * a.cos(), r * a.sin())
        })
        .collect()
}

#[test]
fn circle_with_hole_rows() {
    // Sehnenabstand r(1-cos(pi/n)) <= 1e-6
    let r = run(
        &[circle(10.0, 8000), circle(4.0, 5000)],
        0.0,
        1.0,
        v(0.0, 0.5),
    );
    for k in -10..10 {
        let y = 0.5 + k as f64;
        let ps: Vec<_> = r.iter().filter(|p| (p.0.y - y).abs() < 1e-12).collect();
        let outer = 2.0 * (100.0 - y * y).sqrt();
        let want = if y.abs() < 4.0 {
            outer - 2.0 * (16.0 - y * y).sqrt()
        } else {
            outer
        };
        let got: f64 = ps.iter().map(|p| len(p)).sum();
        assert!((got - want).abs() < 1e-5, "y={y} {got} {want}");
        assert_eq!(ps.len(), if y.abs() < 4.0 { 2 } else { 1 });
        if y.abs() < 4.0 {
            let inner = (16.0 - y * y).sqrt();
            for p in &ps {
                // Stückpunkte nie im Loch
                for x in [p.0.x, p.1.x] {
                    assert!(x.abs() >= inner - 1e-5, "{x} {y}");
                }
            }
        }
    }
}

#[test]
fn limits() {
    let sq = vec![rect(0., 0., 1., 1.)];
    for s in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        assert_eq!(
            hatch_lines(&sq, 0.0, s, o(), MAX_HATCH_LINES),
            Err(HatchError::BadInput)
        );
    }
    assert_eq!(
        hatch_lines(&sq, f64::NAN, 1.0, o(), MAX_HATCH_LINES),
        Err(HatchError::BadInput)
    );
    assert_eq!(
        hatch_lines(&sq, 0.0, 1.0, v(f64::NAN, 0.0), MAX_HATCH_LINES),
        Err(HatchError::BadInput)
    );
    match hatch_lines(
        &[rect(0., 0., 1000., 1000.)],
        0.0,
        0.01,
        o(),
        MAX_HATCH_LINES,
    ) {
        Err(HatchError::TooMany(n)) => assert!(n > MAX_HATCH_LINES),
        other => panic!("{other:?}"),
    }
    // Grenze: genau max Linien erlaubt, eine weniger nicht
    let big = vec![rect(0., 0., 10., 9.9)];
    assert_eq!(
        hatch_lines(&big, 0.0, 1.0, v(0., 0.), 10).unwrap().len(),
        10
    );
    assert_eq!(
        hatch_lines(&big, 0.0, 1.0, v(0., 0.), 9),
        Err(HatchError::TooMany(10))
    );
    // Gitter nicht auf der Unterkante: Zeilen y = 1..=10
    let off = vec![rect(0., 0.5, 10., 10.4)];
    assert_eq!(
        hatch_lines(&off, 0.0, 1.0, v(0., 0.), 10).unwrap().len(),
        10
    );
    assert_eq!(hatch_lines(&[], 0.0, 1.0, o(), 10).unwrap(), vec![]);
    let two = vec![v(0., 0.), v(5., 5.)];
    assert_eq!(hatch_lines(&[two], 0.0, 1.0, o(), 10).unwrap(), vec![]);
}

#[test]
fn origin_shifts_rows() {
    let r = run(&[rect(0., 0., 10., 10.)], 0.0, 4.0, v(7.0, 1.0));
    let ys: Vec<f64> = r.iter().map(|p| p.0.y).collect();
    assert_eq!(ys, vec![1.0, 5.0, 9.0]);
}

#[test]
fn piece_cap() {
    // Linienzahl passt, Stückzahl nicht: Kamm mit vielen Zähnen
    let mut comb = Vec::new();
    for i in 0..10 {
        let x = i as f64 * 2.0;
        comb.extend([v(x, 0.0), v(x, 5.0), v(x + 1.0, 5.0), v(x + 1.0, 0.0)]);
    }
    comb.push(v(19., 0.0));
    comb.push(v(19., -1.0));
    comb.push(v(0., -1.0));
    let r = hatch_lines(&[comb], 0.0, 1.0, v(0., 0.5), 12);
    assert!(matches!(r, Err(HatchError::TooMany(n)) if n > 12), "{r:?}");
}
