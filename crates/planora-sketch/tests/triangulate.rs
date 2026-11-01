//! Füllung der Profile („Profil anzeigen“): Die Dreiecke decken genau die Fläche des Umrisses.

use planora_math::{DVec2, Frame, Tolerance};
use planora_sketch::ExtractedProfiles;
use planora_sketch::model::{PointId, Sketch};
use planora_sketch::triangulate::triangulate;

fn sketch() -> Sketch {
    Sketch::new(Frame::WORLD, "S", Tolerance::default())
}

/// Fläche des Umrisses: äußerer Rand minus Löcher (Gaußsche Trapezformel).
fn outline_area(outline: &[Vec<DVec2>]) -> f64 {
    let shoelace = |pts: &[DVec2]| {
        let n = pts.len();
        ((0..n)
            .map(|i| pts[i].perp_dot(pts[(i + 1) % n]))
            .sum::<f64>()
            / 2.0)
            .abs()
    };
    shoelace(&outline[0]) - outline[1..].iter().map(|h| shoelace(h)).sum::<f64>()
}

fn triangle_area(t: &[DVec2; 3]) -> f64 {
    (t[1] - t[0]).perp_dot(t[2] - t[0]) / 2.0
}

/// Jedes Profil: Dreiecke gegen den Uhrzeigersinn, ihre Summe = Fläche (±1e-6 relativ).
fn assert_fills(e: &ExtractedProfiles, what: &str) {
    assert!(!e.profiles.is_empty(), "{what}: kein Profil");
    for (i, outline) in e.outlines.iter().enumerate() {
        let tris = e.fill(i);
        assert!(!tris.is_empty(), "{what} {i}: keine Dreiecke");
        assert!(
            tris.iter().all(|t| triangle_area(t) > 0.0),
            "{what} {i}: Dreieck falsch herum"
        );
        let sum: f64 = tris.iter().map(triangle_area).sum();
        let want = outline_area(outline);
        assert!(
            ((sum - want) / want).abs() <= 1e-6,
            "{what} {i}: {sum} statt {want}"
        );
        // Zweiter Aufruf: dieselben Dreiecke aus dem Speicher.
        assert!(std::ptr::eq(tris, e.fill(i)), "{what} {i}: neu berechnet");
    }
}

#[test]
fn a_rectangle_is_filled() {
    let mut s = sketch();
    s.add_rectangle(DVec2::ZERO, DVec2::new(20.0, 10.0))
        .unwrap();
    let e = s.profiles();
    assert_fills(&e, "Rechteck");
    let sum: f64 = e.fill(0).iter().map(triangle_area).sum();
    assert!((sum - 200.0).abs() < 1e-9, "{sum}");
}

#[test]
fn a_ring_is_filled_without_its_hole() {
    let mut s = sketch();
    s.add_rectangle(DVec2::ZERO, DVec2::new(20.0, 10.0))
        .unwrap();
    let c = s.add_point(6.0, 5.0).unwrap();
    s.add_circle(c, 3.0).unwrap();
    let e = s.profiles();
    assert_eq!(e.profiles.len(), 2);
    assert_fills(&e, "Ring");
    let ring = (0..2).find(|&i| e.outlines[i].len() == 2).unwrap();
    // Kein Dreieck reicht in das Loch.
    for t in e.fill(ring) {
        let m = (t[0] + t[1] + t[2]) / 3.0;
        assert!(m.distance(DVec2::new(6.0, 5.0)) > 2.9, "{m:?} im Loch");
    }
}

#[test]
fn a_ring_with_two_holes_is_filled() {
    let mut s = sketch();
    s.add_rectangle(DVec2::ZERO, DVec2::new(30.0, 10.0))
        .unwrap();
    for x in [8.0, 22.0] {
        let c = s.add_point(x, 5.0).unwrap();
        s.add_circle(c, 3.0).unwrap();
    }
    assert_fills(&s.profiles(), "zwei Löcher");
}

#[test]
fn a_half_circle_is_filled() {
    let mut s = sketch();
    let c = s.add_point(0.0, 0.0).unwrap();
    let a = s.add_point(5.0, 0.0).unwrap();
    let b = s.add_point(-5.0, 0.0).unwrap();
    s.add_arc(c, a, b).unwrap();
    s.add_line(b, a).unwrap();
    assert_fills(&s.profiles(), "Halbkreis");
}

#[test]
fn half_of_a_figure_eight_is_filled() {
    let mut s = sketch();
    let p: Vec<PointId> = [
        (0.0, 0.0),
        (2.0, 1.5),
        (4.0, 0.3),
        (6.0, -1.5),
        (8.0, 0.0),
        (6.0, 1.5),
        (4.0, -0.3),
        (2.0, -1.5),
        (0.0, 0.0),
    ]
    .iter()
    .map(|&(x, y)| s.add_point(x, y).unwrap())
    .collect();
    s.add_spline(&p).unwrap();
    let e = s.profiles();
    assert_eq!(e.profiles.len(), 2, "beide Hälften");
    assert_fills(&e, "Achterschleife");
}

#[test]
fn a_concave_outline_is_filled() {
    // L-Form mit einspringender Ecke
    let pts = vec![
        DVec2::new(0.0, 0.0),
        DVec2::new(10.0, 0.0),
        DVec2::new(10.0, 3.0),
        DVec2::new(3.0, 3.0),
        DVec2::new(3.0, 10.0),
        DVec2::new(0.0, 10.0),
    ];
    let tris = triangulate(&[pts]);
    let sum: f64 = tris.iter().map(triangle_area).sum();
    assert!((sum - 51.0).abs() < 1e-9, "{sum}");
}

#[test]
fn nothing_to_fill_gives_no_triangles() {
    assert!(triangulate(&[]).is_empty());
    assert!(triangulate(&[vec![DVec2::ZERO, DVec2::X]]).is_empty());
    let e = sketch().profiles();
    assert!(e.fill(0).is_empty());
}

/// Prüft die Füllung des Rings `outer` mit dem Rechteckloch `hole` auf einem Raster (Punkte
/// neben allen Kanten): kein Punkt im Loch bedeckt, jeder Punkt der Ringfläche genau einmal.
fn assert_ring_cover(outer: (f64, f64, f64, f64), hole: (f64, f64, f64, f64)) {
    let mut s = sketch();
    s.add_rectangle(DVec2::new(outer.0, outer.1), DVec2::new(outer.2, outer.3))
        .unwrap();
    s.add_rectangle(DVec2::new(hole.0, hole.1), DVec2::new(hole.2, hole.3))
        .unwrap();
    let e = s.profiles();
    let what = format!("{outer:?} / {hole:?}");
    assert_eq!(e.profiles.len(), 2, "{what}");
    assert_fills(&e, &what);
    let ring = (0..2).find(|&i| e.outlines[i].len() == 2).unwrap();
    let tris = e.fill(ring);
    let want = (outer.2 - outer.0) * (outer.3 - outer.1) - (hole.2 - hole.0) * (hole.3 - hole.1);
    let sum: f64 = tris.iter().map(triangle_area).sum();
    assert!((sum - want).abs() < 1e-9, "{what}: {sum} statt {want}");
    let inside = |t: &[DVec2; 3], p: DVec2| {
        let o = |a: DVec2, b: DVec2| (b - a).perp_dot(p - a) > 0.0;
        o(t[0], t[1]) && o(t[1], t[2]) && o(t[2], t[0])
    };
    let (mut in_hole, mut twice, mut missing) = (0, 0, 0);
    let step = 0.25;
    let mut y = outer.1 + 0.1309;
    while y < outer.3 {
        let mut x = outer.0 + 0.1171;
        while x < outer.2 {
            let p = DVec2::new(x, y);
            let n = tris.iter().filter(|t| inside(t, p)).count();
            let hole_p = x > hole.0 && x < hole.2 && y > hole.1 && y < hole.3;
            match (hole_p, n) {
                (true, 0) | (false, 1) => {}
                (true, _) => in_hole += 1,
                (false, 0) => missing += 1,
                (false, _) => twice += 1,
            }
            x += step;
        }
        y += step;
    }
    assert_eq!(
        (in_hole, twice, missing),
        (0, 0, 0),
        "{what}: im Loch, doppelt, fehlend"
    );
}

#[test]
fn a_square_hole_is_left_open_wherever_it_sits() {
    assert_ring_cover((0.0, 0.0, 20.0, 20.0), (2.0, 2.0, 8.0, 8.0));
    assert_ring_cover((0.0, 0.0, 20.0, 20.0), (5.0, 5.0, 10.0, 10.0));
    assert_ring_cover((0.0, 0.0, 30.0, 30.0), (5.0, 5.0, 10.0, 10.0));
    assert_ring_cover((0.0, 0.0, 20.0, 20.0), (12.0, 12.0, 18.0, 18.0));
    assert_ring_cover((0.0, 0.0, 20.0, 20.0), (7.0, 7.0, 13.0, 13.0));
}
