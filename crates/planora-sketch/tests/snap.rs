//! Fang-Index: Arten, Reihenfolge, Lage, Ausrichtung.

use planora_math::{DVec2, Frame, Tolerance};
use planora_sketch::{CurveId, PointId, Sketch, Snap, SnapIndex};

fn sketch() -> Sketch {
    Sketch::new(Frame::WORLD, "S", Tolerance::default())
}

fn pt(s: &mut Sketch, x: f64, y: f64) -> PointId {
    s.add_point(x, y).unwrap()
}

fn line(s: &mut Sketch, a: (f64, f64), b: (f64, f64)) -> (CurveId, PointId, PointId) {
    let a = pt(s, a.0, a.1);
    let b = pt(s, b.0, b.1);
    (s.add_line(a, b).unwrap(), a, b)
}

fn v(x: f64, y: f64) -> DVec2 {
    DVec2::new(x, y)
}

fn close(a: DVec2, b: DVec2) -> bool {
    a.distance(b) < 1e-9
}

#[test]
fn line_midpoint_is_found_at_exact_position() {
    let mut s = sketch();
    let (l, _, _) = line(&mut s, (10.0, 10.0), (30.0, 10.0));
    let hit = SnapIndex::build(&s).find(v(20.2, 10.3), 1.0).unwrap();
    assert_eq!(hit.snap, Snap::Midpoint(l));
    assert!(close(hit.at, v(20.0, 10.0)), "{:?}", hit.at);
}

#[test]
fn arc_midpoint_lies_on_the_circle_for_wrapping_arc() {
    // Start unten (a0 = -pi/2), Ende links: 270 Grad gegen den Uhrzeigersinn über rechts und oben.
    let mut s = sketch();
    let c = pt(&mut s, 50.0, 50.0);
    let a = pt(&mut s, 50.0, 40.0);
    let e = pt(&mut s, 40.0, 50.0);
    let arc = s.add_arc(c, a, e).unwrap();
    let r = 10.0;
    // Mitte der 270 Grad: -pi/2 + 3pi/4 = pi/4
    let expect = v(50.0 + r * 0.5f64.sqrt(), 50.0 + r * 0.5f64.sqrt());
    let hit = SnapIndex::build(&s)
        .find(expect + v(0.1, 0.1), 1.0)
        .unwrap();
    assert_eq!(hit.snap, Snap::Midpoint(arc));
    assert!(close(hit.at, expect), "{:?} {:?}", hit.at, expect);
    assert!((hit.at.distance(v(50.0, 50.0)) - r).abs() < 1e-9);
}

#[test]
fn arc_starting_at_three_half_pi_has_midpoint_on_the_circle() {
    // Start bei 3pi/2 (unten), Ende bei 0 (rechts): Viertelbogen, Mitte bei 7pi/4.
    let mut s = sketch();
    let c = pt(&mut s, 0.0, 100.0);
    let a = pt(&mut s, 0.0, 90.0);
    let e = pt(&mut s, 10.0, 100.0);
    let arc = s.add_arc(c, a, e).unwrap();
    let expect = v(10.0 * 0.5f64.sqrt(), 100.0 - 10.0 * 0.5f64.sqrt());
    let hit = SnapIndex::build(&s).find(expect, 0.5).unwrap();
    assert_eq!(hit.snap, Snap::Midpoint(arc));
    assert!(close(hit.at, expect), "{:?}", hit.at);
}

#[test]
fn midpoint_beats_curve_at_same_spot() {
    let mut s = sketch();
    let (l, _, _) = line(&mut s, (10.0, 10.0), (30.0, 10.0));
    let hit = SnapIndex::build(&s).find(v(20.5, 10.0), 1.0).unwrap();
    assert_eq!(hit.snap, Snap::Midpoint(l));
}

#[test]
fn curve_is_found_and_position_is_refined() {
    let mut s = sketch();
    let (l, _, _) = line(&mut s, (10.0, 10.0), (30.0, 10.0));
    let hit = SnapIndex::build(&s).find(v(14.0, 10.6), 1.0).unwrap();
    assert_eq!(hit.snap, Snap::Curve(l));
    assert!(close(hit.at, v(14.0, 10.0)), "{:?}", hit.at);
}

#[test]
fn circle_rim_is_found_on_the_circle() {
    let mut s = sketch();
    let c = pt(&mut s, 50.0, 50.0);
    let k = s.add_circle(c, 10.0).unwrap();
    let hit = SnapIndex::build(&s).find(v(60.4, 50.0), 1.0).unwrap();
    assert_eq!(hit.snap, Snap::Curve(k));
    assert!(close(hit.at, v(60.0, 50.0)), "{:?}", hit.at);
}

#[test]
fn intersection_found_with_exact_position() {
    let mut s = sketch();
    let (a, _, _) = line(&mut s, (0.0, 0.0), (100.0, 0.0));
    let (b, _, _) = line(&mut s, (30.0, -50.0), (30.0, 80.0));
    let hit = SnapIndex::build(&s).find(v(30.4, 0.3), 1.0).unwrap();
    assert_eq!(hit.snap, Snap::Intersection(a, b));
    assert!(close(hit.at, v(30.0, 0.0)), "{:?}", hit.at);
}

#[test]
fn intersection_includes_construction_curves() {
    let mut s = sketch();
    line(&mut s, (0.0, 0.0), (100.0, 0.0));
    let (b, _, _) = line(&mut s, (30.0, -50.0), (30.0, 80.0));
    s.set_construction(b, true).unwrap();
    let hit = SnapIndex::build(&s).find(v(30.0, 0.2), 1.0).unwrap();
    assert!(matches!(hit.snap, Snap::Intersection(..)), "{hit:?}");
}

#[test]
fn endpoint_in_radius_beats_nearer_intersection() {
    // Klasse vor Abstand: Der Punkt liegt im Radius, der Schnittpunkt ist näher am Zeiger.
    let mut s = sketch();
    let (_, _, end) = line(&mut s, (0.0, 0.0), (30.0, 0.0));
    line(&mut s, (29.5, -50.0), (29.5, 80.0));
    let hit = SnapIndex::build(&s).find(v(29.5, 0.0), 1.0).unwrap();
    assert_eq!(hit.snap, Snap::Point(end));
    assert!(close(hit.at, v(30.0, 0.0)));
}

#[test]
fn shared_endpoint_is_point_not_intersection() {
    let mut s = sketch();
    let a = pt(&mut s, 0.0, 0.0);
    let m = pt(&mut s, 40.0, 40.0);
    let b = pt(&mut s, 80.0, 0.0);
    s.add_line(a, m).unwrap();
    s.add_line(m, b).unwrap();
    let hit = SnapIndex::build(&s).find(v(40.2, 40.1), 1.0).unwrap();
    assert_eq!(hit.snap, Snap::Point(m));
    assert!(close(hit.at, v(40.0, 40.0)));
}

#[test]
fn circle_center_is_center() {
    let mut s = sketch();
    let c = pt(&mut s, 50.0, 50.0);
    s.add_circle(c, 10.0).unwrap();
    let hit = SnapIndex::build(&s).find(v(50.2, 50.0), 1.0).unwrap();
    assert_eq!(hit.snap, Snap::Center(c));
}

#[test]
fn ellipse_center_is_center() {
    let mut s = sketch();
    let c = pt(&mut s, 50.0, 50.0);
    s.add_ellipse(c, v(20.0, 0.0), 10.0).unwrap();
    let hit = SnapIndex::build(&s).find(v(50.0, 50.2), 1.0).unwrap();
    assert_eq!(hit.snap, Snap::Center(c));
}

#[test]
fn center_that_is_also_endpoint_is_point() {
    let mut s = sketch();
    let c = pt(&mut s, 50.0, 50.0);
    let far = pt(&mut s, 90.0, 50.0);
    s.add_circle(c, 10.0).unwrap();
    s.add_line(c, far).unwrap();
    let hit = SnapIndex::build(&s).find(v(50.0, 50.0), 1.0).unwrap();
    assert_eq!(hit.snap, Snap::Point(c));
}

#[test]
fn free_origin_snaps_and_point_at_origin_wins() {
    let mut s = sketch();
    line(&mut s, (50.0, 50.0), (80.0, 50.0));
    let hit = SnapIndex::build(&s).find(v(0.3, -0.2), 1.0).unwrap();
    assert_eq!(hit.snap, Snap::Origin);
    assert!(close(hit.at, DVec2::ZERO));

    let mut s = sketch();
    let (_, a, _) = line(&mut s, (0.0, 0.0), (80.0, 50.0));
    let hit = SnapIndex::build(&s).find(v(0.3, -0.2), 1.0).unwrap();
    assert_eq!(hit.snap, Snap::Point(a));
}

#[test]
fn nothing_in_radius_is_none() {
    let mut s = sketch();
    line(&mut s, (50.0, 50.0), (80.0, 50.0));
    assert!(SnapIndex::build(&s).find(v(200.0, 200.0), 1.0).is_none());
    assert!(SnapIndex::build(&s).find(v(60.0, 52.0), 1.0).is_none());
}

#[test]
fn revision_matches_sketch() {
    let mut s = sketch();
    line(&mut s, (50.0, 50.0), (80.0, 50.0));
    let idx = SnapIndex::build(&s);
    assert_eq!(idx.revision(), s.revision());
    pt(&mut s, 1.0, 1.0);
    assert_ne!(idx.revision(), s.revision());
}

#[test]
fn align_x_and_y_from_two_points() {
    let mut s = sketch();
    let p = pt(&mut s, 30.0, 70.0);
    let q = pt(&mut s, 90.0, 20.0);
    let idx = SnapIndex::build(&s);
    let h = idx.align(v(30.2, 20.3), 1.0, &[]);
    assert_eq!(h.x, Some((p, 30.0)));
    assert_eq!(h.y, Some((q, 20.0)));
}

#[test]
fn align_skip_and_radius() {
    let mut s = sketch();
    let p = pt(&mut s, 30.0, 70.0);
    let q = pt(&mut s, 90.0, 20.0);
    let idx = SnapIndex::build(&s);
    let h = idx.align(v(30.2, 20.3), 1.0, &[p]);
    assert_eq!(h.x, None);
    assert_eq!(h.y, Some((q, 20.0)));
    let h = idx.align(v(32.0, 25.0), 1.0, &[]);
    assert_eq!((h.x, h.y), (None, None));
}

#[test]
fn tiny_curve_and_zero_radius_never_give_nan() {
    let mut s = sketch();
    let a = pt(&mut s, 5.0, 5.0);
    let b = pt(&mut s, 5.0 + 1e-9, 5.0);
    let _ = s.add_line(a, b);
    let c = pt(&mut s, 7.0, 7.0);
    let _ = s.add_circle(c, 1e-9);
    let idx = SnapIndex::build(&s);
    for r in [0.0, 1e-12, 1.0] {
        for at in [v(5.0, 5.0), v(7.0, 7.0), v(0.0, 0.0), v(6.0, 6.0)] {
            if let Some(h) = idx.find(at, r) {
                assert!(h.at.is_finite(), "{h:?}");
            }
            let al = idx.align(at, r, &[]);
            assert!(al.x.is_none_or(|(_, x)| x.is_finite()));
        }
    }
}
