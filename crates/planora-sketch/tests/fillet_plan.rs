//! Geometrie von Rundung und Fase (Skizzen 4, Task 1): reine Pläne, die Skizze wird nur gelesen.
//! Geprüft wird über dichte Wertefolgen, nicht an Stichproben.

use planora_math::{DVec2, Frame, PI, Tolerance};
use planora_sketch::fillet::{ChamferKind, CurvePick, EndMove, FilletPlan};
use planora_sketch::model::{CurveId, CurveKind, PointId, Sketch};

fn sketch() -> Sketch {
    Sketch::new(Frame::WORLD, "S", Tolerance::default())
}

fn pt(s: &mut Sketch, x: f64, y: f64) -> PointId {
    s.add_point(x, y).unwrap()
}

fn line_pts(s: &mut Sketch, a: PointId, b: PointId) -> CurveId {
    s.add_line(a, b).unwrap()
}

fn line(s: &mut Sketch, x0: f64, y0: f64, x1: f64, y1: f64) -> CurveId {
    let a = pt(s, x0, y0);
    let b = pt(s, x1, y1);
    line_pts(s, a, b)
}

fn pick(curve: CurveId, x: f64, y: f64) -> CurvePick {
    CurvePick {
        curve,
        at: DVec2::new(x, y),
    }
}

fn ends(s: &Sketch, c: CurveId) -> (PointId, PointId) {
    match s.curve(c).unwrap().kind {
        CurveKind::Line { a, b } => (a, b),
        CurveKind::Arc { start, end, .. } | CurveKind::EllipticArc { start, end, .. } => {
            (start, end)
        }
        ref k => panic!("keine offene Kurve: {k:?}"),
    }
}

fn close(a: DVec2, b: DVec2) -> bool {
    a.distance(b) < 1e-9
}

/// Abstand von `p` zur (unendlichen) Geraden durch `a` mit Richtung `d` (Einheitsvektor).
fn dist_line(p: DVec2, a: DVec2, d: DVec2) -> f64 {
    d.perp_dot(p - a).abs()
}

/// Zwei Linien mit gemeinsamer Ecke im Ursprung, Öffnungswinkel `phi`, 40 lang. `turn` ist der
/// Drehsinn der zweiten Linie gegenüber der ersten.
fn corner(phi: f64, turn: f64) -> (Sketch, CurveId, CurveId, DVec2, DVec2) {
    let mut s = sketch();
    let p = pt(&mut s, 0.0, 0.0);
    let d1 = DVec2::new(-1.0, 0.0);
    let d2 = DVec2::from_angle(PI + turn * phi);
    let a = pt(&mut s, 40.0 * d1.x, 40.0 * d1.y);
    let b = pt(&mut s, 40.0 * d2.x, 40.0 * d2.y);
    let l1 = line_pts(&mut s, a, p);
    let l2 = line_pts(&mut s, p, b);
    (s, l1, l2, d1, d2)
}

fn assert_plan_finite(p: &FilletPlan) {
    let vals = [
        p.arc.center,
        p.arc.start,
        p.arc.end,
        p.touch[0],
        p.touch[1],
        p.corner_at,
        p.picks[0].at,
        p.picks[1].at,
    ];
    for v in vals {
        assert!(v.is_finite(), "{p:?}");
    }
    assert!(p.radius.is_finite());
    assert!(p.arc.sweep().is_finite());
}

#[test]
fn right_angle_lines_get_the_quarter_arc() {
    let mut s = sketch();
    let a = pt(&mut s, -40.0, 0.0);
    let p = pt(&mut s, 0.0, 0.0);
    let b = pt(&mut s, 0.0, 40.0);
    let l1 = line_pts(&mut s, a, p);
    let l2 = line_pts(&mut s, p, b);
    let rev = s.revision();
    let plan = s
        .fillet_plan(pick(l1, -20.0, 0.0), pick(l2, 0.0, 20.0), 5.0)
        .unwrap();
    assert!(close(plan.touch[0], DVec2::new(-5.0, 0.0)), "{plan:?}");
    assert!(close(plan.touch[1], DVec2::new(0.0, 5.0)), "{plan:?}");
    assert!(close(plan.arc.center, DVec2::new(-5.0, 5.0)), "{plan:?}");
    assert!((plan.arc.sweep() - PI / 2.0).abs() < 1e-9);
    assert!((plan.arc.radius() - 5.0).abs() < 1e-9);
    assert_eq!(plan.moves, [EndMove::Replace(p), EndMove::Replace(p)]);
    assert_eq!(plan.corner, Some(p));
    assert!(close(plan.corner_at, DVec2::ZERO));
    assert_eq!(plan.radius, 5.0);
    assert_eq!(plan.picks[0].curve, l1);
    assert_eq!(plan.picks[1].curve, l2);
    // gleiche Eingabe, bitgleich; die Skizze bleibt unberührt
    let again = s
        .fillet_plan(pick(l1, -20.0, 0.0), pick(l2, 0.0, 20.0), 5.0)
        .unwrap();
    assert_eq!(plan, again);
    assert_eq!(s.revision(), rev);
    assert_eq!(
        s.fillet_corner(pick(l1, -20.0, 0.0), pick(l2, 0.0, 20.0)),
        Some(DVec2::ZERO)
    );
}

#[test]
fn lines_dense_in_angle_and_radius_never_flip() {
    for deg in (5..=175).step_by(5) {
        let phi = f64::from(deg).to_radians();
        for turn in [1.0, -1.0] {
            let (s, l1, l2, d1, d2) = corner(phi, turn);
            let rev = s.revision();
            let (c1, c2) = (d1 * 20.0, d2 * 20.0);
            let half = phi / 2.0;
            // Grenze: die Berührpunkte erreichen das ferne Ende (40 vom Eck).
            let r_max = 40.0 * half.tan();
            let mut prev: Option<(f64, DVec2)> = None;
            for k in 1..=200 {
                let r = r_max * f64::from(k) / 201.0;
                let plan = s
                    .fillet_plan(pick(l1, c1.x, c1.y), pick(l2, c2.x, c2.y), r)
                    .unwrap_or_else(|| panic!("None bei {deg}° r {r}"));
                assert_plan_finite(&plan);
                assert!(plan.exact);
                let m = plan.arc.center;
                // Berührpunkte auf dem behaltenen Stück: von der Ecke zum fernen Ende
                for (i, d) in [d1, d2].into_iter().enumerate() {
                    let t = plan.touch[i];
                    let along = (t - DVec2::ZERO).dot(d);
                    assert!(along > 0.0 && along < 40.0, "{deg}° r {r}: {along}");
                    assert!(dist_line(t, DVec2::ZERO, d) < 1e-9);
                    assert!(
                        (dist_line(m, DVec2::ZERO, d) - r).abs() < 1e-9,
                        "{deg}° {r}"
                    );
                }
                assert!((plan.arc.radius() - r).abs() < 1e-9 * (1.0 + r));
                // Mitte auf der Winkelhalbierenden, im Winkel
                let bis = (d1 + d2).normalize();
                assert!(
                    (m.dot(bis) - r / half.sin()).abs() < 1e-8 * (1.0 + r),
                    "{deg}°"
                );
                if let Some((pr, pm)) = prev {
                    let dr = r - pr;
                    assert!(
                        m.distance(pm) <= dr / half.sin() * 1.001,
                        "{deg}° turn {turn}: Sprung bei r {r}"
                    );
                }
                prev = Some((r, m));
            }
            assert_eq!(s.revision(), rev);
        }
    }
}

#[test]
fn line_and_arc_with_the_decision_one_values() {
    let mut s = sketch();
    let p = pt(&mut s, 0.0, 0.0);
    let a = pt(&mut s, -30.0, 0.0);
    let l = line_pts(&mut s, a, p);
    let center = pt(&mut s, 10.0, 10.0);
    let e = pt(&mut s, 20.0, 0.0);
    let arc = s.add_arc(center, p, e).unwrap();
    let big = 200.0f64.sqrt();
    // Klick auf dem Bogen: 22,5° nach dem Start
    let ang = 225.0f64.to_radians() + 22.5f64.to_radians();
    let on_arc = DVec2::new(10.0, 10.0) + DVec2::from_angle(ang) * big;
    let rev = s.revision();
    let mut prev: Option<(f64, DVec2)> = None;
    for k in 0..120 {
        let r = 0.2 + 11.8 * f64::from(k) / 119.0;
        let plan = s
            .fillet_plan(pick(l, -15.0, 0.0), pick(arc, on_arc.x, on_arc.y), r)
            .unwrap_or_else(|| panic!("None bei r {r}"));
        assert_plan_finite(&plan);
        let m = plan.arc.center;
        assert!(
            (m.distance(DVec2::new(10.0, 10.0)) - (big + r)).abs() < 1e-9,
            "r {r}"
        );
        assert!((plan.touch[1].distance(DVec2::new(10.0, 10.0)) - big).abs() < 1e-9);
        assert!(plan.touch[0].y.abs() < 1e-9 && plan.touch[0].x < 0.0 && plan.touch[0].x > -30.0);
        assert!((m.y + r).abs() < 1e-9, "Mitte unter der Linie im Abstand r");
        assert_eq!(plan.moves, [EndMove::Replace(p), EndMove::Replace(p)]);
        assert_eq!(plan.corner, Some(p));
        if let Some((pr, pm)) = prev {
            assert!(m.distance(pm) <= 2.0 * (r - pr) + 1e-9, "Sprung bei r {r}");
        }
        prev = Some((r, m));
    }
    assert_eq!(s.revision(), rev);
}

#[test]
fn arc_and_arc_notch_with_the_decision_one_values() {
    let mut s = sketch();
    let p = pt(&mut s, 0.0, 15.0);
    let ca = pt(&mut s, -20.0, 0.0);
    let cb = pt(&mut s, 20.0, 0.0);
    // A: ab P gegen den Uhrzeigersinn nach links oben; B: bis P, die Kerbe liegt darüber
    let ea = {
        let a = 100.0f64.to_radians();
        pt(&mut s, -20.0 + 25.0 * a.cos(), 25.0 * a.sin())
    };
    let sb = {
        let a = 83.13f64.to_radians();
        pt(&mut s, 20.0 + 25.0 * a.cos(), 25.0 * a.sin())
    };
    let arc_a = s.add_arc(ca, p, ea).unwrap();
    let arc_b = s.add_arc(cb, sb, p).unwrap();
    let click_a = DVec2::new(-20.0, 0.0) + DVec2::from_angle(60.0f64.to_radians()) * 25.0;
    let click_b = DVec2::new(20.0, 0.0) + DVec2::from_angle(110.0f64.to_radians()) * 25.0;
    let mut prev: Option<(f64, DVec2)> = None;
    for k in 0..120 {
        let r = 0.2 + 14.8 * f64::from(k) / 119.0;
        let plan = s
            .fillet_plan(
                pick(arc_a, click_a.x, click_a.y),
                pick(arc_b, click_b.x, click_b.y),
                r,
            )
            .unwrap_or_else(|| panic!("None bei r {r}"));
        assert_plan_finite(&plan);
        let m = plan.arc.center;
        assert!(
            (m.distance(DVec2::new(-20.0, 0.0)) - (25.0 + r)).abs() < 1e-9,
            "r {r}"
        );
        assert!(
            (m.distance(DVec2::new(20.0, 0.0)) - (25.0 + r)).abs() < 1e-9,
            "r {r}"
        );
        assert!((plan.touch[0].distance(DVec2::new(-20.0, 0.0)) - 25.0).abs() < 1e-9);
        assert!((plan.touch[1].distance(DVec2::new(20.0, 0.0)) - 25.0).abs() < 1e-9);
        assert_eq!(plan.corner, Some(p));
        assert_eq!(plan.moves, [EndMove::Replace(p), EndMove::Replace(p)]);
        if let Some((pr, pm)) = prev {
            assert!(m.distance(pm) <= 3.0 * (r - pr) + 1e-9, "Sprung bei r {r}");
        }
        prev = Some((r, m));
    }
}

#[test]
fn line_and_circle_keep_the_circle_whole() {
    let mut s = sketch();
    let l = line(&mut s, -30.0, 0.0, 0.0, 0.0);
    let c = pt(&mut s, 0.0, 12.0);
    let circle = s.add_circle(c, 10.0).unwrap();
    let (a, b) = ends(&s, l);
    let mut prev: Option<(f64, DVec2)> = None;
    for k in 0..120 {
        let r = 2.0 + 8.0 * f64::from(k) / 119.0;
        let plan = s
            .fillet_plan(pick(l, -15.0, 0.0), pick(circle, -6.0, 4.0), r)
            .unwrap_or_else(|| panic!("None bei r {r}"));
        assert_plan_finite(&plan);
        let m = plan.arc.center;
        assert!(
            (m.distance(DVec2::new(0.0, 12.0)) - (10.0 + r)).abs() < 1e-9,
            "r {r}"
        );
        assert!((plan.touch[1].distance(DVec2::new(0.0, 12.0)) - 10.0).abs() < 1e-9);
        assert!(plan.touch[0].y.abs() < 1e-9 && plan.touch[0].x < 0.0);
        assert_eq!(plan.moves, [EndMove::Replace(b), EndMove::Whole], "a {a:?}");
        assert_eq!(plan.corner, None);
        if let Some((pr, pm)) = prev {
            assert!(m.distance(pm) <= 5.0 * (r - pr) + 1e-9, "Sprung bei r {r}");
        }
        prev = Some((r, m));
    }
}

#[test]
fn crossing_lines_pick_the_clicked_quadrant() {
    let mut s = sketch();
    let h = line(&mut s, -30.0, 0.0, 30.0, 0.0);
    let v = line(&mut s, 0.0, -30.0, 0.0, 30.0);
    let (ha, hb) = ends(&s, h);
    let (va, vb) = ends(&s, v);
    let p = s
        .fillet_plan(pick(h, -10.0, 0.0), pick(v, 0.0, 10.0), 4.0)
        .unwrap();
    assert!(close(p.arc.center, DVec2::new(-4.0, 4.0)), "{p:?}");
    assert!(close(p.touch[0], DVec2::new(-4.0, 0.0)));
    assert!(close(p.touch[1], DVec2::new(0.0, 4.0)));
    // Fernes Ende bleibt, das nahe wird ersetzt
    assert_eq!(p.moves, [EndMove::Replace(hb), EndMove::Replace(va)]);
    assert_eq!(p.corner, None, "kein gemeinsamer Punkt");
    assert!(close(p.corner_at, DVec2::ZERO));
    let q = s
        .fillet_plan(pick(h, 10.0, 0.0), pick(v, 0.0, -10.0), 4.0)
        .unwrap();
    assert!(close(q.arc.center, DVec2::new(4.0, -4.0)), "{q:?}");
    assert_eq!(q.moves, [EndMove::Replace(ha), EndMove::Replace(vb)]);
}

#[test]
fn lines_meeting_only_when_extended_are_lengthened() {
    let mut s = sketch();
    let l1 = line(&mut s, -40.0, 0.0, -10.0, 0.0);
    let l2 = line(&mut s, 0.0, 10.0, 0.0, 40.0);
    let (_, b1) = ends(&s, l1);
    let (a2, _) = ends(&s, l2);
    let plan = s
        .fillet_plan(pick(l1, -25.0, 0.0), pick(l2, 0.0, 25.0), 5.0)
        .unwrap();
    assert!(close(plan.touch[0], DVec2::new(-5.0, 0.0)), "{plan:?}");
    assert!(close(plan.touch[1], DVec2::new(0.0, 5.0)));
    assert_eq!(plan.moves, [EndMove::Replace(b1), EndMove::Replace(a2)]);
    assert!(close(plan.corner_at, DVec2::ZERO));
    // Berührpunkt jenseits des fernen Endes: nichts
    assert!(
        s.fillet_plan(pick(l1, -25.0, 0.0), pick(l2, 0.0, 25.0), 39.0)
            .is_some()
    );
    assert!(
        s.fillet_plan(pick(l1, -25.0, 0.0), pick(l2, 0.0, 25.0), 41.0)
            .is_none()
    );
}

#[test]
fn ellipse_and_spline_are_numeric_and_stay_whole() {
    let mut s = sketch();
    let l = line(&mut s, -40.0, 0.0, 40.0, 0.0);
    let c = pt(&mut s, 0.0, 20.0);
    let e = s.add_ellipse(c, DVec2::new(15.0, 0.0), 8.0).unwrap();
    let plan = s
        .fillet_plan(pick(l, -20.0, 0.0), pick(e, -10.0, 14.0), 8.0)
        .unwrap();
    assert!(!plan.exact);
    assert_plan_finite(&plan);
    assert_eq!(plan.moves[1], EndMove::Whole);
    assert!(plan.touch[0].y.abs() < 1e-3);
    assert!((plan.arc.center.y - 8.0).abs() < 1e-3, "{plan:?}");
    // Berührpunkt auf der Ellipse: die Abstandsformel nahe der Ellipse
    let t = plan.touch[1] - DVec2::new(0.0, 20.0);
    let f = (t.x / 15.0).powi(2) + (t.y / 8.0).powi(2);
    assert!((f.sqrt() - 1.0).abs() < 1e-3, "{f}");
    // Tangente: Abstand Mitte–Ellipse ist r
    assert!(((plan.arc.center - plan.touch[1]).length() - 8.0).abs() < 1e-3);

    // Spline
    let pts: Vec<PointId> = [(-30.0, 10.0), (-10.0, 18.0), (10.0, 18.0), (30.0, 10.0)]
        .into_iter()
        .map(|(x, y)| pt(&mut s, x, y))
        .collect();
    let sp = s.add_spline(&pts).unwrap();
    let plan = s
        .fillet_plan(pick(l, -20.0, 0.0), pick(sp, -20.0, 14.0), 6.0)
        .unwrap();
    assert!(!plan.exact);
    assert_plan_finite(&plan);
    assert_eq!(plan.moves[1], EndMove::Whole);
    let d = (plan.arc.center - plan.touch[1]).length();
    assert!((d - 6.0).abs() < 1e-3, "{d}");
}

#[test]
fn elliptic_arc_is_shortened_at_its_end() {
    let mut s = sketch();
    let l = line(&mut s, -40.0, 0.0, 40.0, 0.0);
    let c = pt(&mut s, 0.0, 20.0);
    let a = pt(&mut s, -15.0, 20.0);
    let b = pt(&mut s, 15.0, 20.0);
    // untere Halbellipse: von (-15,20) gegen den Uhrzeigersinn nach (15,20)
    let arc = s
        .add_elliptic_arc(c, DVec2::new(15.0, 0.0), 8.0, a, b)
        .unwrap();
    let plan = s
        .fillet_plan(pick(l, -20.0, 0.0), pick(arc, -5.0, 13.0), 8.0)
        .unwrap();
    assert!(!plan.exact);
    assert_plan_finite(&plan);
    assert!(matches!(plan.moves[1], EndMove::Replace(_)), "{plan:?}");
    assert!(((plan.arc.center - plan.touch[1]).length() - 8.0).abs() < 1e-3);
}

#[test]
fn impossible_input_yields_none() {
    let (s, l1, l2, _, _) = corner(PI / 2.0, 1.0);
    let (p1, p2) = (pick(l1, -20.0, 0.0), pick(l2, 0.0, -20.0));
    assert!(s.fillet_plan(p1, p2, 5.0).is_some());
    for r in [0.0, -1.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY, 1e-12] {
        assert!(s.fillet_plan(p1, p2, r).is_none(), "r {r}");
    }
    // größer als die Linie: bei 90° ist die Grenze r = 40
    assert!(s.fillet_plan(p1, p2, 39.9).is_some());
    assert!(s.fillet_plan(p1, p2, 40.0).is_none());
    assert!(s.fillet_plan(p1, p2, 1e9).is_none());
    // dieselbe Kurve zweimal
    assert!(s.fillet_plan(p1, pick(l1, -10.0, 0.0), 5.0).is_none());
    // Klick mit NaN
    assert!(s.fillet_plan(p1, pick(l2, f64::NAN, 0.0), 5.0).is_none());
    // unbekannte Kurve
    let gone = CurveId::from_index(999);
    assert!(s.fillet_plan(p1, pick(gone, 0.0, 0.0), 5.0).is_none());

    // parallel
    let mut s = sketch();
    let a = line(&mut s, 0.0, 0.0, 30.0, 0.0);
    let b = line(&mut s, 0.0, 10.0, 30.0, 10.0);
    assert!(
        s.fillet_plan(pick(a, 15.0, 0.0), pick(b, 15.0, 10.0), 2.0)
            .is_none()
    );
    // gegenläufig und gleiche Gerade
    let c = line(&mut s, 5.0, 0.0, 40.0, 0.0);
    assert!(
        s.fillet_plan(pick(a, 5.0, 0.0), pick(c, 35.0, 0.0), 2.0)
            .is_none()
    );

    // tangential am gemeinsamen Ende: Linie und Bogen, der die Linie fortsetzt
    let mut s = sketch();
    let l = line(&mut s, -40.0, 0.0, 0.0, 0.0);
    let (_, p) = ends(&s, l);
    let center = pt(&mut s, 0.0, 10.0);
    let end = pt(&mut s, 10.0, 10.0);
    let arc = s.add_arc(center, p, end).unwrap();
    assert!(
        s.fillet_plan(pick(l, -20.0, 0.0), pick(arc, 7.0, 3.0), 2.0)
            .is_none()
    );
}

#[test]
fn never_nan_in_some_over_a_wild_sweep() {
    let (s, l1, l2, _, _) = corner(1.0, 1.0);
    let wild = [
        1e-300, 1e-9, 1e-6, 0.5, 3.0, 17.0, 1e3, 1e6, 1e12, 1e100, 1e300,
    ];
    for r in wild {
        for (x, y) in [(-1e-9, 0.0), (-20.0, 0.0), (-1e6, 0.0)] {
            if let Some(p) = s.fillet_plan(pick(l1, x, y), pick(l2, 5.0, 5.0), r) {
                assert_plan_finite(&p);
            }
        }
    }
}

#[test]
fn curves_at_point_lists_open_curves_ending_there() {
    let mut s = sketch();
    let o = pt(&mut s, 0.0, 0.0);
    let a = pt(&mut s, 10.0, 0.0);
    let b = pt(&mut s, 0.0, 10.0);
    let l1 = line_pts(&mut s, o, a);
    let l2 = line_pts(&mut s, b, o);
    let arc = s.add_arc(o, a, b).unwrap();
    let circle = s.add_circle(o, 3.0).unwrap();
    let mut at_o = s.curves_at_point(o);
    at_o.sort_by_key(|c| c.index());
    assert_eq!(
        at_o,
        vec![l1, l2],
        "der Kreis und der Bogen hängen nur mit der Mitte dort"
    );
    let mut at_a = s.curves_at_point(a);
    at_a.sort_by_key(|c| c.index());
    assert_eq!(at_a, vec![l1, arc]);
    assert!(!at_o.contains(&circle));
    // Ellipsenbogen
    let c = pt(&mut s, 50.0, 0.0);
    let e0 = pt(&mut s, 60.0, 0.0);
    let e1 = pt(&mut s, 50.0, 5.0);
    let ea = s
        .add_elliptic_arc(c, DVec2::new(10.0, 0.0), 5.0, e0, e1)
        .unwrap();
    assert_eq!(s.curves_at_point(e1), vec![ea]);
    assert!(s.curves_at_point(c).is_empty());
}

fn right_angle() -> (Sketch, CurveId, CurveId, PointId) {
    let mut s = sketch();
    let a = pt(&mut s, -40.0, 0.0);
    let p = pt(&mut s, 0.0, 0.0);
    let b = pt(&mut s, 0.0, 40.0);
    let l1 = line_pts(&mut s, a, p);
    let l2 = line_pts(&mut s, p, b);
    (s, l1, l2, p)
}

#[test]
fn chamfer_kinds_place_the_touch_points() {
    let (s, l1, l2, p) = right_angle();
    let (p1, p2) = (pick(l1, -20.0, 0.0), pick(l2, 0.0, 20.0));
    let rev = s.revision();

    let eq = s.chamfer_plan(p1, p2, ChamferKind::Equal(6.0)).unwrap();
    assert!(close(eq.touch[0], DVec2::new(-6.0, 0.0)), "{eq:?}");
    assert!(close(eq.touch[1], DVec2::new(0.0, 6.0)));
    assert_eq!(eq.moves, [p, p]);
    assert_eq!(eq.corner, Some(p));
    assert!(close(eq.corner_at, DVec2::ZERO));
    assert_eq!(eq.kind, ChamferKind::Equal(6.0));

    let two = s
        .chamfer_plan(p1, p2, ChamferKind::TwoDistances(3.0, 8.0))
        .unwrap();
    assert!(close(two.touch[0], DVec2::new(-3.0, 0.0)), "{two:?}");
    assert!(close(two.touch[1], DVec2::new(0.0, 8.0)));

    let da = s
        .chamfer_plan(p1, p2, ChamferKind::DistanceAngle(6.0, PI / 4.0))
        .unwrap();
    assert!(close(da.touch[0], DVec2::new(-6.0, 0.0)), "{da:?}");
    assert!(close(da.touch[1], DVec2::new(0.0, 6.0)));
    let da = s
        .chamfer_plan(p1, p2, ChamferKind::DistanceAngle(6.0, PI / 6.0))
        .unwrap();
    assert!(close(da.touch[0], DVec2::new(-6.0, 0.0)));
    assert!(
        close(da.touch[1], DVec2::new(0.0, 6.0 * (PI / 6.0).tan())),
        "{da:?}"
    );
    assert_eq!(s.revision(), rev);
}

#[test]
fn chamfer_distance_angle_matches_the_triangle_over_a_dense_sweep() {
    for deg in (10..=170).step_by(10) {
        let phi = f64::from(deg).to_radians();
        let (s, l1, l2, d1, d2) = corner(phi, 1.0);
        let (c1, c2) = (d1 * 20.0, d2 * 20.0);
        for k in 1..=120 {
            let theta = (PI - phi) * f64::from(k) / 121.0;
            let plan = s.chamfer_plan(
                pick(l1, c1.x, c1.y),
                pick(l2, c2.x, c2.y),
                ChamferKind::DistanceAngle(5.0, theta),
            );
            // zweiter Abstand nach dem Sinussatz; ab 40 reicht die Linie nicht mehr
            let len2 = 5.0 * theta.sin() / (PI - phi - theta).sin();
            if len2 >= 40.0 {
                assert!(plan.is_none(), "{deg}° θ {theta}: d2 {len2}");
                continue;
            }
            let plan = plan.unwrap_or_else(|| panic!("None bei {deg}° θ {theta}"));
            assert!(close(plan.touch[1], d2 * len2), "{deg}°");
            assert!(close(plan.touch[0], d1 * 5.0));
            // Winkel bei T1 zwischen T1→X und T1→T2
            let v = (DVec2::ZERO - plan.touch[0]).normalize();
            let w = (plan.touch[1] - plan.touch[0]).normalize();
            let got = v.dot(w).clamp(-1.0, 1.0).acos();
            assert!((got - theta).abs() < 1e-9, "{deg}° θ {theta}: {got}");
        }
        // Summe der Winkel erreicht π: der Nenner ist nicht mehr positiv
        assert!(
            s.chamfer_plan(
                pick(l1, c1.x, c1.y),
                pick(l2, c2.x, c2.y),
                ChamferKind::DistanceAngle(5.0, PI - phi)
            )
            .is_none()
        );
        assert!(
            s.chamfer_plan(
                pick(l1, c1.x, c1.y),
                pick(l2, c2.x, c2.y),
                ChamferKind::DistanceAngle(5.0, PI - phi + 0.1)
            )
            .is_none()
        );
    }
}

#[test]
fn chamfer_impossible_input_yields_none() {
    let (s, l1, l2, _) = right_angle();
    let (p1, p2) = (pick(l1, -20.0, 0.0), pick(l2, 0.0, 20.0));
    // länger als die Linie (40)
    assert!(s.chamfer_plan(p1, p2, ChamferKind::Equal(40.0)).is_none());
    assert!(s.chamfer_plan(p1, p2, ChamferKind::Equal(39.9)).is_some());
    assert!(
        s.chamfer_plan(p1, p2, ChamferKind::TwoDistances(3.0, 41.0))
            .is_none()
    );
    assert!(
        s.chamfer_plan(p1, p2, ChamferKind::DistanceAngle(41.0, 0.5))
            .is_none()
    );
    // θ so groß, dass der Nenner nicht positiv ist (bei 90° ab π/2)
    assert!(
        s.chamfer_plan(p1, p2, ChamferKind::DistanceAngle(5.0, PI / 2.0))
            .is_none()
    );
    assert!(
        s.chamfer_plan(p1, p2, ChamferKind::DistanceAngle(5.0, 3.0))
            .is_none()
    );
    // Müll
    for k in [
        ChamferKind::Equal(0.0),
        ChamferKind::Equal(-1.0),
        ChamferKind::Equal(f64::NAN),
        ChamferKind::TwoDistances(0.0, 1.0),
        ChamferKind::TwoDistances(1.0, f64::INFINITY),
        ChamferKind::DistanceAngle(1.0, 0.0),
        ChamferKind::DistanceAngle(1.0, -0.2),
        ChamferKind::DistanceAngle(1.0, f64::NAN),
        ChamferKind::DistanceAngle(0.0, 0.5),
    ] {
        assert!(s.chamfer_plan(p1, p2, k).is_none(), "{k:?}");
    }
    // dieselbe Kurve, parallele Linien
    assert!(
        s.chamfer_plan(p1, pick(l1, -10.0, 0.0), ChamferKind::Equal(2.0))
            .is_none()
    );
    let mut t = sketch();
    let a = line(&mut t, 0.0, 0.0, 30.0, 0.0);
    let b = line(&mut t, 0.0, 10.0, 30.0, 10.0);
    assert!(
        t.chamfer_plan(
            pick(a, 15.0, 0.0),
            pick(b, 15.0, 10.0),
            ChamferKind::Equal(2.0)
        )
        .is_none()
    );
    // Bogen als Kurve
    let mut t = sketch();
    let l = line(&mut t, -40.0, 0.0, 0.0, 0.0);
    let (_, p) = ends(&t, l);
    let c = pt(&mut t, 0.0, 10.0);
    let e = pt(&mut t, 10.0, 10.0);
    let arc = t.add_arc(c, p, e).unwrap();
    assert!(
        t.chamfer_plan(
            pick(l, -20.0, 0.0),
            pick(arc, 7.0, 3.0),
            ChamferKind::Equal(2.0)
        )
        .is_none()
    );
}

#[test]
fn chamfer_on_a_crossing_picks_the_clicked_side() {
    let mut s = sketch();
    let h = line(&mut s, -30.0, 0.0, 30.0, 0.0);
    let v = line(&mut s, 0.0, -30.0, 0.0, 30.0);
    let (_, hb) = ends(&s, h);
    let (va, _) = ends(&s, v);
    let plan = s
        .chamfer_plan(
            pick(h, -10.0, 0.0),
            pick(v, 0.0, 10.0),
            ChamferKind::Equal(4.0),
        )
        .unwrap();
    assert!(close(plan.touch[0], DVec2::new(-4.0, 0.0)));
    assert!(close(plan.touch[1], DVec2::new(0.0, 4.0)));
    assert_eq!(plan.moves, [hb, va]);
    assert_eq!(plan.corner, None);
}

#[test]
fn a_fixed_end_keeps_the_line_whole_and_blocks_the_chamfer() {
    let (mut s, l1, l2, p) = right_angle();
    s.add_constraint(planora_sketch::ConstraintKind::Fixed(p, 0.0, 0.0))
        .unwrap();
    let (p1, p2) = (pick(l1, -20.0, 0.0), pick(l2, 0.0, 20.0));
    let plan = s.fillet_plan(p1, p2, 5.0).unwrap();
    assert_eq!(plan.moves, [EndMove::Whole, EndMove::Whole]);
    assert_eq!(plan.corner, Some(p));
    assert!(close(plan.touch[0], DVec2::new(-5.0, 0.0)));
    assert!(s.chamfer_plan(p1, p2, ChamferKind::Equal(6.0)).is_none());
}
