//! Reine Geometrie der Zeichenvarianten (Skizzen 3, Task 1).

use planora_math::{DVec2, PI};
use planora_sketch::shapes::*;

const TOL: f64 = 1e-6;

fn v(x: f64, y: f64) -> DVec2 {
    DVec2::new(x, y)
}

fn near(a: DVec2, b: DVec2) -> bool {
    (a - b).length() < 1e-9
}

#[track_caller]
fn assert_near(a: DVec2, b: DVec2) {
    assert!(near(a, b), "{a:?} != {b:?}");
}

#[test]
fn circle_through_three_points() {
    let (c, r) = circle_through(v(0.0, 0.0), v(10.0, 0.0), v(0.0, 10.0), TOL).unwrap();
    assert_near(c, v(5.0, 5.0));
    assert!((r - 50f64.sqrt()).abs() < 1e-9);
}

#[test]
fn circle_through_degenerate_is_none() {
    assert!(circle_through(v(0.0, 0.0), v(5.0, 0.0), v(10.0, 0.0), TOL).is_none());
    assert!(circle_through(v(0.0, 0.0), v(0.0, 0.0), v(10.0, 3.0), TOL).is_none());
    assert!(circle_through(v(0.0, 0.0), v(4.0, 4.0), v(4.0, 4.0), TOL).is_none());
}

#[test]
fn arc_through_counter_clockwise_and_clockwise() {
    let s = 10.0 / 2f64.sqrt(); // exakt auf dem Kreis (der Entwurf nennt gerundet 7,07)
    let a = arc_through(v(10.0, 0.0), v(0.0, 10.0), v(s, s), TOL).unwrap();
    assert_near(a.center, v(0.0, 0.0));
    assert_near(a.start, v(10.0, 0.0));
    assert_near(a.end, v(0.0, 10.0));
    assert!((a.sweep() - PI / 2.0).abs() < 1e-9);
    assert!((a.radius() - 10.0).abs() < 1e-9);

    let b = arc_through(v(10.0, 0.0), v(0.0, 10.0), v(-s, -s), TOL).unwrap();
    assert_near(b.center, v(0.0, 0.0));
    assert_near(b.start, v(0.0, 10.0));
    assert_near(b.end, v(10.0, 0.0));
    assert!((b.sweep() - 1.5 * PI).abs() < 1e-9);
}

#[test]
fn tangent_arc_both_sides_and_degenerate() {
    let x = v(1.0, 0.0);
    let a = tangent_arc(v(10.0, 0.0), x, v(15.0, 5.0), TOL).unwrap();
    assert_near(a.center, v(10.0, 5.0));
    assert_near(a.start, v(10.0, 0.0));
    assert_near(a.end, v(15.0, 5.0));

    let b = tangent_arc(v(10.0, 0.0), x, v(15.0, -5.0), TOL).unwrap();
    assert_near(b.center, v(10.0, -5.0));
    assert_near(b.start, v(15.0, -5.0));
    assert_near(b.end, v(10.0, 0.0));

    assert!(tangent_arc(v(10.0, 0.0), x, v(20.0, 0.0), TOL).is_none());
    assert!(tangent_arc(v(10.0, 0.0), x, v(0.0, 0.0), TOL).is_none());
    assert!(tangent_arc(v(10.0, 0.0), x, v(10.0, 0.0), TOL).is_none());
}

#[test]
fn tangent_arc_locked_variants() {
    let x = v(1.0, 0.0);
    let s = v(10.0, 0.0);
    let toward = v(15.0, 5.0);
    let end = tangent_arc_locked(s, x, Some(5.0), Some(PI / 2.0), toward, TOL).unwrap();
    assert_near(end, v(15.0, 5.0));
    // Nur Radius: Ende auf dem Kreis in Richtung des Zeigers
    let end = tangent_arc_locked(s, x, Some(5.0), None, v(10.0, 20.0), TOL).unwrap();
    assert_near(end, v(10.0, 10.0));
    // Nur Winkel: Radius aus dem Zeiger, Ende liegt dann auf der Sehne
    let end = tangent_arc_locked(s, x, None, Some(PI / 2.0), toward, TOL).unwrap();
    assert_near(end, v(15.0, 5.0));
    // Nichts gesperrt: der Zeiger
    assert_eq!(
        tangent_arc_locked(s, x, None, None, toward, TOL),
        Some(toward)
    );
    // Untere Seite spiegelt
    let end = tangent_arc_locked(s, x, Some(5.0), Some(PI / 2.0), v(15.0, -5.0), TOL).unwrap();
    assert_near(end, v(15.0, -5.0));
    // Zeiger auf der Tangente: keine Seite
    assert!(tangent_arc_locked(s, x, Some(5.0), None, v(20.0, 0.0), TOL).is_none());
}

#[test]
fn rectangles() {
    let r = rect_three_point(v(0.0, 0.0), v(10.0, 0.0), v(3.0, 4.0), TOL).unwrap();
    for (got, want) in r
        .iter()
        .zip([v(0.0, 0.0), v(10.0, 0.0), v(10.0, 4.0), v(0.0, 4.0)])
    {
        assert_near(*got, want);
    }
    let r = rect_three_point(v(0.0, 0.0), v(10.0, 0.0), v(3.0, -4.0), TOL).unwrap();
    assert_near(r[2], v(10.0, -4.0));
    assert_near(r[3], v(0.0, -4.0));
    assert!(rect_three_point(v(0.0, 0.0), v(10.0, 0.0), v(3.0, 0.0), TOL).is_none());
    assert!(rect_three_point(v(1.0, 1.0), v(1.0, 1.0), v(3.0, 0.0), TOL).is_none());

    let c = rect_center(v(0.0, 0.0), v(3.0, 2.0), TOL).unwrap();
    for (got, want) in c
        .iter()
        .zip([v(-3.0, -2.0), v(3.0, -2.0), v(3.0, 2.0), v(-3.0, 2.0)])
    {
        assert_near(*got, want);
    }
    assert!(rect_center(v(0.0, 0.0), v(3.0, 0.0), TOL).is_none());
}

#[test]
fn polygons_are_regular() {
    for n in [3usize, 4, 6, 64] {
        let p = polygon_inscribed(v(2.0, -1.0), v(12.0, -1.0), n, TOL).unwrap();
        assert_eq!(p.corners.len(), n);
        assert_eq!(p.corners[0], v(12.0, -1.0));
        let side = (p.corners[1] - p.corners[0]).length();
        for i in 0..n {
            let a = p.corners[i];
            let b = p.corners[(i + 1) % n];
            assert!(((b - a).length() - side).abs() < 1e-9);
            assert!(((a - p.center).length() - 10.0).abs() < 1e-9);
        }
    }
}

#[test]
fn polygon_circumscribed_side_mid_is_exact() {
    let p = polygon_circumscribed(v(5.0, -3.0), v(15.0, -3.0), 6, TOL).unwrap();
    assert_eq!(p.corners.len(), 6);
    assert_near((p.corners[0] + p.corners[1]) * 0.5, v(15.0, -3.0));
    let side = (p.corners[1] - p.corners[0]).length();
    for i in 0..6 {
        let a = p.corners[i];
        let b = p.corners[(i + 1) % 6];
        assert!(((b - a).length() - side).abs() < 1e-9);
    }
}

#[test]
fn polygon_edge_sides_and_limits() {
    let p = polygon_edge(v(0.0, 0.0), v(10.0, 0.0), v(5.0, 5.0), 4, TOL).unwrap();
    let want = [v(0.0, 0.0), v(10.0, 0.0), v(10.0, 10.0), v(0.0, 10.0)];
    for (got, w) in p.corners.iter().zip(want) {
        assert_near(*got, w);
    }
    assert_near(p.center, v(5.0, 5.0));

    let p = polygon_edge(v(0.0, 0.0), v(10.0, 0.0), v(5.0, -5.0), 4, TOL).unwrap();
    assert_near(p.center, v(5.0, -5.0));
    assert_eq!(p.corners[0], v(0.0, 0.0));
    assert_eq!(p.corners[1], v(10.0, 0.0));
    assert_near(p.corners[2], v(10.0, -10.0));

    for bad in [2usize, 65, 0] {
        assert!(polygon_edge(v(0.0, 0.0), v(10.0, 0.0), v(5.0, 5.0), bad, TOL).is_none());
        assert!(polygon_inscribed(v(0.0, 0.0), v(10.0, 0.0), bad, TOL).is_none());
        assert!(polygon_circumscribed(v(0.0, 0.0), v(10.0, 0.0), bad, TOL).is_none());
    }
}

#[test]
fn slots() {
    let s = slot_center_to_center(v(0.0, 0.0), v(40.0, 0.0), v(20.0, 6.0), TOL).unwrap();
    assert!((s.radius - 6.0).abs() < 1e-9);
    assert_near(s.a, v(0.0, 0.0));
    assert_near(s.b, v(40.0, 0.0));

    let s = slot_overall(v(0.0, 0.0), v(40.0, 0.0), v(20.0, 6.0), TOL).unwrap();
    assert_near(s.a, v(6.0, 0.0));
    assert_near(s.b, v(34.0, 0.0));
    assert!((s.radius - 6.0).abs() < 1e-9);
    // Breite >= Länge
    assert!(slot_overall(v(0.0, 0.0), v(40.0, 0.0), v(20.0, 20.0), TOL).is_none());
    assert!(slot_overall(v(0.0, 0.0), v(40.0, 0.0), v(20.0, 25.0), TOL).is_none());

    let s = slot_center_point(v(0.0, 0.0), v(20.0, 0.0), v(5.0, 6.0), TOL).unwrap();
    assert_near(s.a, v(-20.0, 0.0));
    assert_near(s.b, v(20.0, 0.0));
    assert!((s.radius - 6.0).abs() < 1e-9);
}

fn arc_plan(radius: f64, sweep_deg: f64) -> ArcPlan {
    let sweep = sweep_deg.to_radians();
    ArcPlan {
        center: v(0.0, 0.0),
        start: v(radius, 0.0),
        end: v(radius * sweep.cos(), radius * sweep.sin()),
    }
}

#[test]
fn arc_slot_width_and_limits() {
    let axis = arc_plan(50.0, 90.0);
    for at in [v(55.0, 0.0), v(45.0, 0.0)] {
        let s = arc_slot(axis, at, TOL).unwrap();
        assert!((s.half_width - 5.0).abs() < 1e-9);
    }
    // h >= R
    assert!(arc_slot(axis, v(100.0, 0.0), TOL).is_none());
    assert!(arc_slot(axis, v(0.0, 0.0), TOL).is_none());
    // h = 0
    assert!(arc_slot(axis, v(50.0, 0.0), TOL).is_none());
    // Enden treffen sich auf der offenen Seite
    assert!(arc_slot(arc_plan(50.0, 350.0), v(55.0, 0.0), TOL).is_none());
    assert!(arc_slot(arc_plan(50.0, 300.0), v(55.0, 0.0), TOL).is_some());
}

fn assert_closed(poly: &[DVec2]) {
    assert!(poly.len() > 3);
    assert_near(poly[0], *poly.last().unwrap());
}

#[test]
fn slot_outline_is_closed_without_jumps() {
    let plan = ShapePlan::Slot(SlotPlan {
        a: v(0.0, 0.0),
        b: v(40.0, 0.0),
        radius: 6.0,
    });
    let outline = plan.outline(64);
    assert_eq!(outline.len(), 1);
    let poly = &outline[0];
    assert_closed(poly);
    // Stöße: kein Sprung größer als eine Teilstrecke; Linien sind 40 lang
    for w in poly.windows(2) {
        assert!((w[1] - w[0]).length() <= 40.0 + 1e-9);
    }
    // alle Punkte liegen auf dem Rand: Abstand zur Achse = 6 oder auf Endkreis
    for p in poly {
        let on_side = (p.y.abs() - 6.0).abs() < 1e-9 && p.x >= -1e-9 && p.x <= 40.0 + 1e-9;
        let on_cap = ((*p - v(0.0, 0.0)).length() - 6.0).abs() < 1e-9
            || ((*p - v(40.0, 0.0)).length() - 6.0).abs() < 1e-9;
        assert!(on_side || on_cap, "{p:?}");
    }
}

#[test]
fn other_outlines() {
    let quad = ShapePlan::Quad([v(0.0, 0.0), v(1.0, 0.0), v(1.0, 1.0), v(0.0, 1.0)]);
    assert_closed(&quad.outline(32)[0]);
    let circle = ShapePlan::Circle(v(3.0, 3.0), 2.0);
    let o = &circle.outline(32)[0];
    assert_closed(o);
    for p in o {
        assert!(((*p - v(3.0, 3.0)).length() - 2.0).abs() < 1e-9);
    }
    let poly = ShapePlan::Polygon(polygon_inscribed(v(0.0, 0.0), v(5.0, 0.0), 5, TOL).unwrap());
    assert_eq!(poly.outline(32)[0].len(), 6);
    let arc = ShapePlan::Arc(arc_plan(10.0, 90.0));
    let o = &arc.outline(64)[0];
    assert_near(o[0], v(10.0, 0.0));
    assert_near(*o.last().unwrap(), arc_plan(10.0, 90.0).end);

    let aslot = ShapePlan::ArcSlot(arc_slot(arc_plan(50.0, 120.0), v(55.0, 0.0), TOL).unwrap());
    let o = &aslot.outline(64)[0];
    assert_closed(o);
    for p in o {
        let d = p.length();
        let end = arc_plan(50.0, 120.0).end;
        let ok = (d - 55.0).abs() < 1e-9
            || (d - 45.0).abs() < 1e-9
            || ((*p - v(50.0, 0.0)).length() - 5.0).abs() < 1e-9
            || ((*p - end).length() - 5.0).abs() < 1e-9;
        assert!(ok, "{p:?}");
    }
}

#[test]
fn tiny_inputs_never_give_nan() {
    let e = 1e-12;
    let a = v(0.0, 0.0);
    let b = v(e, 0.0);
    let c = v(0.0, e);
    assert!(circle_through(a, b, c, TOL).is_none());
    assert!(arc_through(a, b, c, TOL).is_none());
    assert!(tangent_arc(a, v(1.0, 0.0), c, TOL).is_none());
    assert!(tangent_arc(a, v(0.0, 0.0), v(1.0, 1.0), TOL).is_none());
    assert!(rect_three_point(a, b, c, TOL).is_none());
    assert!(rect_center(a, c, TOL).is_none());
    assert!(polygon_inscribed(a, b, 5, TOL).is_none());
    assert!(polygon_circumscribed(a, b, 5, TOL).is_none());
    assert!(polygon_edge(a, b, c, 5, TOL).is_none());
    assert!(slot_center_to_center(a, b, c, TOL).is_none());
    assert!(slot_overall(a, b, c, TOL).is_none());
    assert!(slot_center_point(a, b, c, TOL).is_none());
    assert!(tangent_arc_locked(a, v(1.0, 0.0), Some(1.0), None, c, TOL).is_none());
    assert!(
        arc_slot(
            ArcPlan {
                center: a,
                start: b,
                end: c
            },
            c,
            TOL
        )
        .is_none()
    );
    let nan = v(f64::NAN, 0.0);
    assert!(circle_through(a, nan, c, TOL).is_none());
}
