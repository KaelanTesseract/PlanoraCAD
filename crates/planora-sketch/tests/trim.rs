//! Stutzen wie in Fusion: Stück zwischen den nächsten Schnittpunkten löschen, Kurve ohne
//! Schnitt ganz löschen; Abhängigkeiten nach Entscheidung 7 des Plans (nichts bewegt sich).

use planora_math::{DVec2, Frame, TAU, Tolerance};
use planora_sketch::SketchError;
use planora_sketch::constraint::{ConstraintKind as K, LineOrPoints};
use planora_sketch::model::{CurveId, CurveKind, PointId, Sketch};
use planora_sketch::solver::{Drag, SolveStatus};

fn sketch() -> Sketch {
    Sketch::new(Frame::WORLD, "S", Tolerance::default())
}

fn line(s: &mut Sketch, x0: f64, y0: f64, x1: f64, y1: f64) -> (CurveId, PointId, PointId) {
    let a = s.add_point(x0, y0).unwrap();
    let b = s.add_point(x1, y1).unwrap();
    (s.add_line(a, b).unwrap(), a, b)
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

fn xy(s: &Sketch, p: PointId) -> DVec2 {
    s.point_xy(p).unwrap()
}

/// Alle lebenden Punkte mit ihrer Lage.
fn snapshot(s: &Sketch) -> Vec<(PointId, DVec2)> {
    s.points().map(|(p, _)| (p, xy(s, p))).collect()
}

/// Alle Punkte aus `before`, die noch leben, liegen bitgleich an derselben Stelle.
fn assert_unmoved(s: &Sketch, before: &[(PointId, DVec2)]) {
    for &(p, v) in before {
        if let Ok(now) = s.point_xy(p) {
            assert_eq!(now, v, "Punkt {p:?} hat sich bewegt");
        }
    }
}

fn kinds(s: &Sketch) -> Vec<K> {
    s.constraints().map(|(_, d)| d.kind.clone()).collect()
}

fn has(s: &Sketch, k: &K) -> bool {
    kinds(s).contains(k)
}

fn close(a: DVec2, b: DVec2) -> bool {
    a.distance(b) < 1e-9
}

/// Lösen bewegt nichts und meldet „gelöst“.
fn assert_solve_keeps(s: &mut Sketch) {
    let before = snapshot(s);
    let r = s.solve();
    assert_eq!(r.status, SolveStatus::Solved, "{r:?}");
    assert_unmoved(s, &before);
}

#[test]
fn trimming_a_cross_arm_ends_the_line_at_the_crossing_with_point_on_curve() {
    let mut s = sketch();
    let (h, ha, hb) = line(&mut s, -10.0, 0.0, 10.0, 0.0);
    let (v, _, _) = line(&mut s, 0.0, -10.0, 0.0, 10.0);
    let before = snapshot(&s);
    assert_eq!(s.trim(h, DVec2::new(-5.0, 0.0)).unwrap(), vec![h]);
    let (p, b) = ends(&s, h);
    assert_eq!(b, hb);
    assert!(close(xy(&s, p), DVec2::ZERO), "{:?}", xy(&s, p));
    assert!(s.point_xy(ha).is_err(), "der alte Endpunkt ist weg");
    assert!(has(&s, &K::PointOnCurve(p, v)), "{:?}", kinds(&s));
    assert_unmoved(&s, &before);
    assert_solve_keeps(&mut s);
}

#[test]
fn trimming_at_a_t_junction_line_end_sets_coincident() {
    let mut s = sketch();
    let (h, _, hb) = line(&mut s, 0.0, 0.0, 10.0, 0.0);
    let (_, va, _) = line(&mut s, 5.0, 0.0, 5.0, 10.0);
    let before = snapshot(&s);
    assert_eq!(s.trim(h, DVec2::new(2.0, 0.0)).unwrap(), vec![h]);
    let (p, b) = ends(&s, h);
    assert_eq!(b, hb);
    assert_eq!(xy(&s, p), xy(&s, va));
    assert!(has(&s, &K::Coincident(p, va)), "{:?}", kinds(&s));
    assert!(
        !kinds(&s)
            .iter()
            .any(|k| matches!(k, K::PointOnCurve(q, _) if *q == p)),
        "{:?}",
        kinds(&s)
    );
    assert_unmoved(&s, &before);
    assert_solve_keeps(&mut s);
}

#[test]
fn the_middle_piece_leaves_two_horizontal_collinear_lines() {
    let mut s = sketch();
    let (h, ha, hb) = line(&mut s, 0.0, 0.0, 10.0, 0.0);
    s.add_constraint(K::Horizontal(LineOrPoints::Line(h)))
        .unwrap();
    let (v1, _, _) = line(&mut s, 3.0, -5.0, 3.0, 5.0);
    let (v2, _, _) = line(&mut s, 7.0, -5.0, 7.0, 5.0);
    let before = snapshot(&s);
    let out = s.trim(h, DVec2::new(5.0, 0.0)).unwrap();
    assert_eq!(out.len(), 2);
    assert_eq!(out[0], h);
    let n = out[1];
    let (a0, p0) = ends(&s, h);
    let (p1, b1) = ends(&s, n);
    assert_eq!((a0, b1), (ha, hb));
    assert!(close(xy(&s, p0), DVec2::new(3.0, 0.0)));
    assert!(close(xy(&s, p1), DVec2::new(7.0, 0.0)));
    assert!(has(&s, &K::Horizontal(LineOrPoints::Line(h))));
    assert!(has(&s, &K::Horizontal(LineOrPoints::Line(n))));
    assert!(has(&s, &K::PointOnCurve(p1, h)), "{:?}", kinds(&s));
    assert!(has(&s, &K::PointOnCurve(p0, v1)));
    assert!(has(&s, &K::PointOnCurve(p1, v2)));
    assert_unmoved(&s, &before);
    assert_solve_keeps(&mut s);
    // kollinear: nach dem Ziehen des freien Endes bleibt die zweite Linie auf der Geraden
    s.drag(&[Drag {
        point: hb,
        target: DVec2::new(12.0, 3.0),
    }])
    .unwrap();
    let (a, d) = (xy(&s, ha), xy(&s, p0) - xy(&s, ha));
    for q in [p1, hb] {
        let off = d.normalize().perp_dot(xy(&s, q) - a);
        assert!(off.abs() < 1e-6, "{q:?} liegt {off} neben der Geraden");
    }
}

#[test]
fn a_circle_with_two_cutting_lines_becomes_an_arc_keeping_its_radius() {
    let mut s = sketch();
    let m = s.add_point(0.0, 0.0).unwrap();
    let c = s.add_circle(m, 5.0).unwrap();
    s.add_constraint(K::Radius(c, 5.0)).unwrap();
    let (l1, _, _) = line(&mut s, 3.0, -10.0, 3.0, 10.0);
    line(&mut s, -3.0, -10.0, -3.0, 10.0);
    let before = snapshot(&s);
    let piece = s.trim_piece(c, DVec2::new(5.5, 0.0)).unwrap();
    assert!(!piece.whole);
    assert_eq!(s.trim(c, DVec2::new(5.5, 0.0)).unwrap(), vec![c]);
    let CurveKind::Arc { center, start, end } = s.curve(c).unwrap().kind else {
        panic!("kein Bogen: {:?}", s.curve(c).unwrap().kind)
    };
    assert_eq!(center, m);
    assert!(
        close(xy(&s, start), DVec2::new(3.0, 4.0)),
        "{:?}",
        xy(&s, start)
    );
    assert!(
        close(xy(&s, end), DVec2::new(3.0, -4.0)),
        "{:?}",
        xy(&s, end)
    );
    // die Vorschau zeigt genau das gelöschte Stück: von `end` nach `start`
    assert!(close(piece.polyline[0], xy(&s, end)));
    assert!(close(*piece.polyline.last().unwrap(), xy(&s, start)));
    assert!(has(&s, &K::Radius(c, 5.0)), "{:?}", kinds(&s));
    assert!(has(&s, &K::PointOnCurve(start, l1)));
    assert!(has(&s, &K::PointOnCurve(end, l1)));
    assert!((s.radius(c).unwrap() - 5.0).abs() < 1e-12);
    assert_unmoved(&s, &before);
    assert_solve_keeps(&mut s);
}

#[test]
fn a_length_dimension_of_a_trimmed_line_is_dropped_and_nothing_moves() {
    let mut s = sketch();
    let (h, a, b) = line(&mut s, 0.0, 0.0, 10.0, 0.0);
    s.add_constraint(K::Distance(a, b, 10.0)).unwrap();
    let mid = s.add_point(5.0, 0.0).unwrap();
    s.add_constraint(K::Midpoint(mid, h)).unwrap();
    line(&mut s, 4.0, -5.0, 4.0, 5.0);
    let before = snapshot(&s);
    assert_eq!(s.trim(h, DVec2::new(2.0, 0.0)).unwrap(), vec![h]);
    assert!(
        !kinds(&s).iter().any(|k| matches!(k, K::Distance(..))),
        "{:?}",
        kinds(&s)
    );
    assert!(
        !kinds(&s).iter().any(|k| matches!(k, K::Midpoint(..))),
        "Mittelpunkt an gekürzter Linie entfällt"
    );
    assert_unmoved(&s, &before);
    assert_solve_keeps(&mut s);
}

#[test]
fn an_ellipse_becomes_an_elliptic_arc_keeping_its_major_axis() {
    let mut s = sketch();
    let m = s.add_point(0.0, 0.0).unwrap();
    let e = s.add_ellipse(m, DVec2::new(10.0, 0.0), 5.0).unwrap();
    s.add_constraint(K::EllipseMajor(e, 10.0)).unwrap();
    let CurveKind::Ellipse { ax, ay, minor, .. } = s.curve(e).unwrap().kind else {
        unreachable!()
    };
    let (l, _, _) = line(&mut s, 0.0, -10.0, 0.0, 10.0);
    let before = snapshot(&s);
    assert_eq!(s.trim(e, DVec2::new(10.0, 0.0)).unwrap(), vec![e]);
    let CurveKind::EllipticArc {
        center,
        ax: ax2,
        ay: ay2,
        minor: mi2,
        start,
        end,
    } = s.curve(e).unwrap().kind
    else {
        panic!("kein Ellipsenbogen")
    };
    assert_eq!((center, ax2, ay2, mi2), (m, ax, ay, minor));
    assert!(
        close(xy(&s, start), DVec2::new(0.0, 5.0)),
        "{:?}",
        xy(&s, start)
    );
    assert!(
        close(xy(&s, end), DVec2::new(0.0, -5.0)),
        "{:?}",
        xy(&s, end)
    );
    assert!(has(&s, &K::EllipseMajor(e, 10.0)));
    assert!(has(&s, &K::PointOnCurve(start, l)));
    assert_unmoved(&s, &before);
    assert_solve_keeps(&mut s);
    // der Bogen liegt links (durch (−10, 0))
    let pl = s.curve_polyline(e, 64).unwrap();
    assert!(pl.iter().any(|p| close(*p, DVec2::new(-10.0, 0.0))));
}

#[test]
fn the_middle_of_an_arc_splits_into_two_equal_arcs_on_one_center() {
    let mut s = sketch();
    let m = s.add_point(0.0, 0.0).unwrap();
    let a = s.add_point(5.0, 0.0).unwrap();
    let b = s.add_point(-5.0, 0.0).unwrap();
    let c = s.add_arc(m, a, b).unwrap();
    line(&mut s, 2.0, -10.0, 2.0, 10.0);
    line(&mut s, -2.0, -10.0, -2.0, 10.0);
    let before = snapshot(&s);
    let out = s.trim(c, DVec2::new(0.0, 5.0)).unwrap();
    assert_eq!(out.len(), 2);
    assert_eq!(out[0], c);
    let n = out[1];
    let CurveKind::Arc { center: m2, .. } = s.curve(n).unwrap().kind else {
        panic!("kein Bogen")
    };
    assert_eq!(m2, m);
    assert_eq!(ends(&s, c).0, a);
    assert_eq!(ends(&s, n).1, b);
    assert!(has(&s, &K::Equal(c, n)), "{:?}", kinds(&s));
    assert_unmoved(&s, &before);
    assert_solve_keeps(&mut s);
}

#[test]
fn the_middle_of_an_elliptic_arc_splits_into_two_sharing_the_axes() {
    let mut s = sketch();
    let m = s.add_point(0.0, 0.0).unwrap();
    let a = s.add_point(10.0, 0.0).unwrap();
    let b = s.add_point(-10.0, 0.0).unwrap();
    let e = s
        .add_elliptic_arc(m, DVec2::new(10.0, 0.0), 5.0, a, b)
        .unwrap();
    line(&mut s, 3.0, -10.0, 3.0, 10.0);
    line(&mut s, -3.0, -10.0, -3.0, 10.0);
    let before = snapshot(&s);
    let out = s.trim(e, DVec2::new(0.0, 5.0)).unwrap();
    assert_eq!(out.len(), 2);
    let axes = |c: CurveId| match s.curve(c).unwrap().kind {
        CurveKind::EllipticArc {
            center,
            ax,
            ay,
            minor,
            ..
        } => (center, ax, ay, minor),
        ref k => panic!("{k:?}"),
    };
    assert_eq!(axes(out[0]), axes(out[1]));
    assert_eq!(ends(&s, out[0]).0, a);
    assert_eq!(ends(&s, out[1]).1, b);
    assert_unmoved(&s, &before);
    assert_solve_keeps(&mut s);
}

#[test]
fn a_trimmed_spline_ends_on_the_cutting_curve() {
    let mut s = sketch();
    let pts: Vec<PointId> = [(0.0, 0.0), (2.0, 1.0), (4.0, 0.0), (6.0, -1.0), (8.0, 0.0)]
        .iter()
        .map(|&(x, y)| s.add_point(x, y).unwrap())
        .collect();
    let sp = s.add_spline(&pts).unwrap();
    let (l, _, _) = line(&mut s, 5.0, -5.0, 5.0, 5.0);
    let at = s.curve_eval(sp, 3.5).unwrap();
    let before = snapshot(&s);
    assert_eq!(s.trim(sp, at).unwrap(), vec![sp]);
    let CurveKind::Spline { points } = &s.curve(sp).unwrap().kind else {
        panic!("kein Spline")
    };
    assert!(points.len() >= 3, "{points:?}");
    assert_eq!(points[0], pts[0]);
    let p = *points.last().unwrap();
    assert!((xy(&s, p).x - 5.0).abs() < 1e-6, "{:?}", xy(&s, p));
    assert!(has(&s, &K::PointOnCurve(p, l)));
    assert!(s.point_xy(pts[4]).is_err(), "Stützpunkt des Stücks ist weg");
    assert_unmoved(&s, &before);
    assert_solve_keeps(&mut s);
}

#[test]
fn a_spline_piece_without_inner_points_gets_its_midpoint() {
    let mut s = sketch();
    let pts: Vec<PointId> = [(0.0, 0.0), (4.0, 2.0), (8.0, 0.0)]
        .iter()
        .map(|&(x, y)| s.add_point(x, y).unwrap())
        .collect();
    let sp = s.add_spline(&pts).unwrap();
    line(&mut s, 1.0, -5.0, 1.0, 5.0);
    line(&mut s, 3.0, -5.0, 3.0, 5.0);
    // Stück rechts von x = 3: übrig bleibt (0, 0) … x = 1 … x = 3 ohne Stützpunkt dazwischen
    let at = s.curve_eval(sp, 1.5).unwrap();
    let out = s.trim(sp, at).unwrap();
    assert_eq!(out, vec![sp]);
    let CurveKind::Spline { points } = &s.curve(sp).unwrap().kind else {
        panic!("kein Spline")
    };
    assert!(points.len() >= 3, "{points:?}");
    let last = xy(&s, *points.last().unwrap());
    assert!((last.x - 3.0).abs() < 1e-6, "{last:?}");
}

#[test]
fn a_curve_without_intersections_is_deleted_with_its_free_points() {
    let mut s = sketch();
    let (l, a, b) = line(&mut s, 0.0, 0.0, 10.0, 0.0);
    let piece = s.trim_piece(l, DVec2::new(3.0, 1.0)).unwrap();
    assert!(piece.whole);
    assert_eq!((piece.t0, piece.t1), (0.0, 1.0));
    assert_eq!(
        s.trim(l, DVec2::new(3.0, 1.0)).unwrap(),
        Vec::<CurveId>::new()
    );
    assert!(s.curve(l).is_none());
    assert!(s.point_xy(a).is_err() && s.point_xy(b).is_err());

    let m = s.add_point(20.0, 0.0).unwrap();
    let c = s.add_circle(m, 2.0).unwrap();
    assert!(s.trim(c, DVec2::new(22.0, 0.0)).unwrap().is_empty());
    assert!(s.curve(c).is_none() && s.point_xy(m).is_err());

    // L-Ecke: Berührung am Ende ist kein Schnitt; der Eckpunkt bleibt für die andere Linie
    let corner = s.add_point(0.0, 0.0).unwrap();
    let p = s.add_point(10.0, 0.0).unwrap();
    let q = s.add_point(0.0, 10.0).unwrap();
    let l1 = s.add_line(corner, p).unwrap();
    let l2 = s.add_line(corner, q).unwrap();
    assert!(s.trim(l1, DVec2::new(5.0, 0.0)).unwrap().is_empty());
    assert!(s.point_xy(corner).is_ok() && s.point_xy(p).is_err());
    assert!(s.curve(l2).is_some());
}

#[test]
fn construction_lines_count_as_cutters() {
    let mut s = sketch();
    let (h, _, _) = line(&mut s, -10.0, 0.0, 10.0, 0.0);
    let (v, _, _) = line(&mut s, 0.0, -10.0, 0.0, 10.0);
    s.set_construction(v, true).unwrap();
    let piece = s.trim_piece(h, DVec2::new(-5.0, 0.0)).unwrap();
    assert!(!piece.whole);
    assert_eq!(s.trim(h, DVec2::new(-5.0, 0.0)).unwrap(), vec![h]);
    assert!(close(xy(&s, ends(&s, h).0), DVec2::ZERO));
}

#[test]
fn trim_piece_gives_the_bounds_that_trim_removes() {
    let mut s = sketch();
    let (h, _, _) = line(&mut s, -10.0, 0.0, 10.0, 0.0);
    line(&mut s, 0.0, -10.0, 0.0, 10.0);
    line(&mut s, 5.0, -10.0, 5.0, 10.0);
    let at = DVec2::new(2.0, 0.3);
    let piece = s.trim_piece(h, at).unwrap();
    assert_eq!(piece.curve, h);
    assert!(!piece.whole);
    assert!((piece.t0 - 0.5).abs() < 1e-12 && (piece.t1 - 0.75).abs() < 1e-12);
    let (p0, p1) = (piece.polyline[0], *piece.polyline.last().unwrap());
    assert_eq!(p0, s.curve_eval(h, piece.t0).unwrap());
    assert_eq!(p1, s.curve_eval(h, piece.t1).unwrap());
    let out = s.trim(h, at).unwrap();
    assert_eq!(out.len(), 2);
    assert_eq!(xy(&s, ends(&s, out[0]).1), p0);
    assert_eq!(xy(&s, ends(&s, out[1]).0), p1);

    // Kreis: periodisch über den Nullwinkel
    let mut s = sketch();
    let m = s.add_point(0.0, 0.0).unwrap();
    let c = s.add_circle(m, 5.0).unwrap();
    line(&mut s, 3.0, -10.0, 3.0, 10.0);
    let piece = s.trim_piece(c, DVec2::new(6.0, 0.0)).unwrap();
    assert!(piece.t0 < 0.0 && piece.t1 > 0.0 && piece.t1 - piece.t0 < TAU);
    assert!(close(piece.polyline[0], DVec2::new(3.0, -4.0)));
    assert!(close(*piece.polyline.last().unwrap(), DVec2::new(3.0, 4.0)));

    assert!(s.trim_piece(c, DVec2::new(f64::NAN, 0.0)).is_none());
}

#[test]
fn trimming_unknown_curves_fails_without_change() {
    let mut s = sketch();
    let (h, _, _) = line(&mut s, 0.0, 0.0, 1.0, 0.0);
    s.remove_curve(h).unwrap();
    let rev = s.revision();
    assert_eq!(s.trim(h, DVec2::ZERO), Err(SketchError::UnknownObject));
    assert_eq!(s.revision(), rev);
    assert!(s.trim_piece(h, DVec2::ZERO).is_none());
}

/// Fläche eines geschlossenen Linienzugs (gegen den Uhrzeigersinn positiv).
fn area(poly: &[DVec2]) -> f64 {
    let n = poly.len();
    (0..n)
        .map(|i| poly[i].perp_dot(poly[(i + 1) % n]))
        .sum::<f64>()
        / 2.0
}

#[test]
fn profiles_after_trimming_a_hash_are_one_rectangle() {
    let mut s = sketch();
    line(&mut s, -2.0, 0.0, 12.0, 0.0);
    line(&mut s, -2.0, 4.0, 12.0, 4.0);
    line(&mut s, 0.0, -2.0, 0.0, 6.0);
    line(&mut s, 10.0, -2.0, 10.0, 6.0);
    let ids: Vec<CurveId> = s.curves().map(|(c, _)| c).collect();
    let clicks = [
        (ids[0], -1.0, 0.0),
        (ids[0], 11.0, 0.0),
        (ids[1], -1.0, 4.0),
        (ids[1], 11.0, 4.0),
        (ids[2], 0.0, -1.0),
        (ids[2], 0.0, 5.0),
        (ids[3], 10.0, -1.0),
        (ids[3], 10.0, 5.0),
    ];
    let before = snapshot(&s);
    for (c, x, y) in clicks {
        assert_eq!(s.trim(c, DVec2::new(x, y)).unwrap(), vec![c]);
    }
    assert_unmoved(&s, &before);
    // spätere Schnitte an den schon gestutzten Enden: deckungsgleich statt Punkt auf Kurve
    assert!(
        kinds(&s).iter().any(|k| matches!(k, K::Coincident(..))),
        "{:?}",
        kinds(&s)
    );
    let e = s.profiles();
    assert!(e.rejected.is_empty(), "{:?}", e.rejected);
    assert_eq!(e.profiles.len(), 1);
    assert_eq!(e.curve_loops[0].len(), 4);
    assert!((area(&e.outlines[0][0]) - 40.0).abs() < 1e-9);
    assert_solve_keeps(&mut s);
}

#[test]
fn profiles_after_trimming_a_cross_in_a_rectangle() {
    let mut s = sketch();
    s.add_rectangle(DVec2::new(0.0, 0.0), DVec2::new(10.0, 10.0))
        .unwrap();
    let (h, _, _) = line(&mut s, 0.0, 5.0, 10.0, 5.0);
    let (v, _, _) = line(&mut s, 5.0, 0.0, 5.0, 10.0);
    assert_eq!(s.profiles().profiles.len(), 4);
    // linken Arm des Kreuzes stutzen: links oben und unten werden eine Fläche
    s.trim(h, DVec2::new(2.0, 5.0)).unwrap();
    let e = s.profiles();
    assert!(e.rejected.is_empty(), "{:?}", e.rejected);
    assert_eq!(e.profiles.len(), 3);
    s.trim(v, DVec2::new(5.0, 8.0)).unwrap();
    assert_eq!(s.profiles().profiles.len(), 2);
}

#[test]
fn an_elliptic_arc_cannot_start_or_end_at_its_center() {
    let mut s = sketch();
    let m = s.add_point(0.0, 0.0).unwrap();
    let a = s.add_point(10.0, 0.0).unwrap();
    let r = s.add_elliptic_arc(m, DVec2::new(10.0, 0.0), 5.0, m, a);
    assert!(matches!(r, Err(SketchError::Degenerate(_))), "{r:?}");
    let r = s.add_elliptic_arc(m, DVec2::new(10.0, 0.0), 5.0, a, m);
    assert!(matches!(r, Err(SketchError::Degenerate(_))), "{r:?}");
}
