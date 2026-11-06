//! Reine Wahl des Maßes aus Auswahl und Zeiger (Skizzen 5, Task 6, Entscheidung 13): Zeigerregel
//! für waagerecht, senkrecht und ausgerichtet über ein dichtes Gitter, Kurven allein, Punkt und
//! Linie, Mittellinie mit Umschalter Ø/R, zwei Linien, zwei Kreise; Vorschau = Ergebnis und
//! keine Änderung an der Skizze.

use planora_math::{DVec2, Frame, Tolerance};
use planora_sketch::constraint::ConstraintKind as K;
use planora_sketch::dimension::{DimMode, DimPick, DimPlan};
use planora_sketch::model::{CurveId, PointId, Sketch};

fn sketch() -> Sketch {
    Sketch::new(Frame::WORLD, "S", Tolerance::default())
}

fn v(x: f64, y: f64) -> DVec2 {
    DVec2::new(x, y)
}

fn pt(s: &mut Sketch, x: f64, y: f64) -> PointId {
    s.add_point(x, y).unwrap()
}

fn line(s: &mut Sketch, a: DVec2, b: DVec2) -> (CurveId, PointId, PointId) {
    let (pa, pb) = (pt(s, a.x, a.y), pt(s, b.x, b.y));
    (s.add_line(pa, pb).unwrap(), pa, pb)
}

fn value(k: &K) -> f64 {
    match *k {
        K::Distance(_, _, x)
        | K::DistanceX(_, _, x)
        | K::DistanceY(_, _, x)
        | K::DistancePointLine(_, _, x)
        | K::CenterlineDiameter(_, _, x)
        | K::Radius(_, x)
        | K::Diameter(_, x)
        | K::Angle(_, _, x)
        | K::EllipseMajor(_, x)
        | K::EllipseMinor(_, x) => x,
        ref other => panic!("kein Maß: {other:?}"),
    }
}

/// Der Plan: Wert gemessen, nie 0, nie NaN, Anker am Zeiger, Skizze unberührt, bitgleich wiederholt.
fn plan(s: &Sketch, picks: &[DimPick], pointer: DVec2) -> DimPlan {
    let rev = s.revision();
    let p = s
        .dimension_plan(picks, pointer)
        .unwrap_or_else(|| panic!("kein Plan für {picks:?} bei {pointer:?}"));
    assert_eq!(s.revision(), rev);
    assert_eq!(s.dimension_plan(picks, pointer), Some(p.clone()));
    assert_eq!(p.anchor, pointer);
    let x = value(&p.kind);
    assert!(x.is_finite() && x > 0.0, "{p:?}");
    let m = s.measure(&p.kind).unwrap();
    assert!((m - x).abs() < 1e-12, "{p:?}: gemessen {m}");
    p
}

/// Vorschau = Ergebnis: Anlegen und Lösen bewegt nichts.
fn commits_still(s: &Sketch, p: &DimPlan) {
    let mut c = s.clone();
    let before = c.variables().to_vec();
    c.add_constraint(p.kind.clone()).unwrap();
    let r = c.solve();
    assert_eq!(r.iterations, 0, "{p:?}: {r:?}");
    assert!(r.conflicting.is_empty(), "{p:?}: {r:?}");
    assert_eq!(c.variables(), &before[..], "{p:?}");
}

#[test]
fn two_points_follow_the_pointer() {
    let mut s = sketch();
    let (a, b) = (pt(&mut s, 0.0, 0.0), pt(&mut s, 40.0, 30.0));
    let picks = [DimPick::Point(a), DimPick::Point(b)];
    let h = plan(&s, &picks, v(20.0, 50.0));
    assert_eq!(h.mode, DimMode::Horizontal);
    assert_eq!(h.kind, K::DistanceX(a, b, 40.0));
    let vt = plan(&s, &picks, v(60.0, 15.0));
    assert_eq!(vt.mode, DimMode::Vertical);
    assert_eq!(vt.kind, K::DistanceY(a, b, 30.0));
    let al = plan(&s, &picks, v(60.0, 50.0));
    assert_eq!(al.mode, DimMode::Aligned);
    assert!(matches!(al.kind, K::Distance(p, q, _) if p == a && q == b));
    assert!((value(&al.kind) - 50.0).abs() < 1e-12);
    for p in [h, vt, al] {
        commits_still(&s, &p);
        assert_eq!(p.toggled(&s), None);
    }
}

/// Dichtes Gitter 200×200 um die Punkte: Die Art wechselt nur dort, wo eine Kante der x- oder
/// y-Spanne zwischen zwei benachbarten Zeigerlagen liegt.
#[test]
fn dense_pointer_grid_switches_only_at_span_edges() {
    let mut s = sketch();
    let (a, b) = (pt(&mut s, 0.0, 0.0), pt(&mut s, 40.0, 30.0));
    let picks = [DimPick::Point(a), DimPick::Point(b)];
    let n = 200;
    let coord = |i: usize, lo: f64, hi: f64| lo + (hi - lo) * i as f64 / (n - 1) as f64;
    let grid: Vec<Vec<DimMode>> = (0..n)
        .map(|i| {
            (0..n)
                .map(|j| plan(&s, &picks, v(coord(i, -40.0, 80.0), coord(j, -30.0, 60.0))).mode)
                .collect()
        })
        .collect();
    let crosses = |u: f64, w: f64, edges: [f64; 2]| {
        edges
            .iter()
            .any(|&e| (u <= e && e <= w) || (w <= e && e <= u))
    };
    let mut seen = std::collections::HashSet::new();
    for i in 0..n {
        for j in 0..n {
            seen.insert(format!("{:?}", grid[i][j]));
            if i + 1 < n && grid[i][j] != grid[i + 1][j] {
                let (x0, x1) = (coord(i, -40.0, 80.0), coord(i + 1, -40.0, 80.0));
                assert!(crosses(x0, x1, [0.0, 40.0]), "Sprung bei x {x0}..{x1}");
            }
            if j + 1 < n && grid[i][j] != grid[i][j + 1] {
                let (y0, y1) = (coord(j, -30.0, 60.0), coord(j + 1, -30.0, 60.0));
                assert!(crosses(y0, y1, [0.0, 30.0]), "Sprung bei y {y0}..{y1}");
            }
        }
    }
    assert_eq!(seen.len(), 3, "{seen:?}");
}

#[test]
fn points_above_each_other_are_never_horizontal() {
    let mut s = sketch();
    let (a, b) = (pt(&mut s, 5.0, 0.0), pt(&mut s, 5.0, 30.0));
    let picks = [DimPick::Point(a), DimPick::Point(b)];
    let n = 200;
    for i in 0..n {
        for j in 0..n {
            let x = -20.0 + 50.0 * i as f64 / (n - 1) as f64;
            let y = -30.0 + 90.0 * j as f64 / (n - 1) as f64;
            let p = plan(&s, &picks, v(x, y));
            assert_ne!(p.mode, DimMode::Horizontal, "{p:?}");
        }
    }
    // Genau über der Spanne (x = 5): ausgerichtet, nicht waagerecht.
    assert_eq!(plan(&s, &picks, v(5.0, 50.0)).mode, DimMode::Aligned);
    // Dieselbe Stelle zweimal: kein Maß.
    assert_eq!(
        s.dimension_plan(&[DimPick::Point(a), DimPick::Point(a)], v(0.0, 0.0)),
        None
    );
}

#[test]
fn single_curves() {
    let mut s = sketch();
    let (l, _, _) = line(&mut s, v(0.0, 0.0), v(30.0, 40.0));
    let p = plan(&s, &[DimPick::Curve(l, v(15.0, 20.0))], v(0.0, 30.0));
    assert_eq!(p.mode, DimMode::Aligned);
    assert!((value(&p.kind) - 50.0).abs() < 1e-12);
    commits_still(&s, &p);

    let m = pt(&mut s, 100.0, 0.0);
    let circle = s.add_circle(m, 7.0).unwrap();
    let p = plan(&s, &[DimPick::Curve(circle, v(107.0, 0.0))], v(120.0, 5.0));
    assert_eq!(p.mode, DimMode::Diameter);
    assert_eq!(p.kind, K::Diameter(circle, 14.0));
    commits_still(&s, &p);
    let r = p.toggled(&s).unwrap();
    assert_eq!(
        (r.mode, r.kind.clone()),
        (DimMode::Radius, K::Radius(circle, 7.0))
    );
    assert_eq!(r.toggled(&s), Some(p));

    let (c, st, en) = (
        pt(&mut s, 0.0, 100.0),
        pt(&mut s, 5.0, 100.0),
        pt(&mut s, 0.0, 105.0),
    );
    let arc = s.add_arc(c, st, en).unwrap();
    let p = plan(&s, &[DimPick::Curve(arc, v(3.5, 103.5))], v(10.0, 110.0));
    assert_eq!(p.mode, DimMode::Radius);
    assert!(matches!(p.kind, K::Radius(x, _) if x == arc));
    assert!((value(&p.kind) - 5.0).abs() < 1e-12);
    commits_still(&s, &p);
    let d = p.toggled(&s).unwrap();
    assert_eq!(d.mode, DimMode::Diameter);
    assert_eq!(d.toggled(&s), Some(p));

    // Spline allein, Punkt allein: kein Maß.
    let sp: Vec<PointId> = (0..3)
        .map(|i| pt(&mut s, 200.0 + 10.0 * i as f64, (i % 2) as f64))
        .collect();
    let spline = s.add_spline(&sp).unwrap();
    assert_eq!(
        s.dimension_plan(&[DimPick::Curve(spline, v(200.0, 0.0))], v(0.0, 0.0)),
        None
    );
    assert_eq!(
        s.dimension_plan(&[DimPick::Point(sp[0])], v(0.0, 0.0)),
        None
    );
    assert_eq!(s.dimension_plan(&[], v(0.0, 0.0)), None);
}

#[test]
fn ellipse_axis_follows_the_pointer() {
    let mut s = sketch();
    let m = pt(&mut s, 0.0, 0.0);
    let e = s.add_ellipse(m, v(20.0, 0.0), 8.0).unwrap();
    let pick = [DimPick::Curve(e, v(20.0, 0.0))];
    // Nahe der großen Achse (x-Achse).
    let p = plan(&s, &pick, v(30.0, 2.0));
    assert_eq!(p.mode, DimMode::Major);
    assert_eq!(p.kind, K::EllipseMajor(e, 20.0));
    commits_still(&s, &p);
    // Nahe der kleinen Achse (y-Achse).
    let p = plan(&s, &pick, v(1.0, 15.0));
    assert_eq!(p.mode, DimMode::Minor);
    assert_eq!(p.kind, K::EllipseMinor(e, 8.0));
    commits_still(&s, &p);
    assert_eq!(p.toggled(&s), None);
}

#[test]
fn point_and_line() {
    let mut s = sketch();
    let (l, _, _) = line(&mut s, v(0.0, 0.0), v(50.0, 0.0));
    let p = pt(&mut s, 10.0, 12.0);
    for picks in [
        [DimPick::Point(p), DimPick::Curve(l, v(5.0, 0.0))],
        [DimPick::Curve(l, v(5.0, 0.0)), DimPick::Point(p)],
    ] {
        let d = plan(&s, &picks, v(20.0, 20.0));
        assert_eq!(d.mode, DimMode::PointLine);
        assert_eq!(d.kind, K::DistancePointLine(p, l, 12.0));
        commits_still(&s, &d);
        assert_eq!(d.toggled(&s), None);
    }
    // Ein Ende der Linie selbst: Abstand 0, kein Maß.
    let end = s.points().map(|(id, _)| id).next().unwrap();
    assert_eq!(
        s.dimension_plan(
            &[DimPick::Point(end), DimPick::Curve(l, v(5.0, 0.0))],
            v(0.0, 0.0)
        ),
        None
    );
}

#[test]
fn point_and_centerline_is_a_diameter() {
    let mut s = sketch();
    let (axis, _, _) = line(&mut s, v(0.0, 0.0), v(0.0, 80.0));
    s.set_centerline(axis, true).unwrap();
    let p = pt(&mut s, 15.0, 30.0);
    let d = plan(
        &s,
        &[DimPick::Point(p), DimPick::Curve(axis, v(0.0, 10.0))],
        v(-10.0, 30.0),
    );
    assert_eq!(d.mode, DimMode::CenterlineDiameter);
    assert_eq!(d.kind, K::CenterlineDiameter(p, axis, 30.0));
    commits_still(&s, &d);
    let r = d.toggled(&s).unwrap();
    assert_eq!(r.mode, DimMode::CenterlineRadius);
    assert_eq!(r.kind, K::DistancePointLine(p, axis, 15.0));
    commits_still(&s, &r);
    assert_eq!(r.toggled(&s), Some(d));

    // Kreis gegen Mittellinie: über die Mitte.
    let m = pt(&mut s, -20.0, 50.0);
    let c = s.add_circle(m, 4.0).unwrap();
    let d = plan(
        &s,
        &[
            DimPick::Curve(c, v(-16.0, 50.0)),
            DimPick::Curve(axis, v(0.0, 50.0)),
        ],
        v(-10.0, 60.0),
    );
    assert_eq!(d.mode, DimMode::CenterlineDiameter);
    assert_eq!(d.kind, K::CenterlineDiameter(m, axis, 40.0));
    commits_still(&s, &d);
}

#[test]
fn two_lines() {
    let mut s = sketch();
    let (l1, _, _) = line(&mut s, v(0.0, 0.0), v(50.0, 0.0));
    let (l2, a2, _) = line(&mut s, v(10.0, 20.0), v(60.0, 20.0));
    let d = plan(
        &s,
        &[
            DimPick::Curve(l1, v(5.0, 0.0)),
            DimPick::Curve(l2, v(30.0, 20.0)),
        ],
        v(30.0, 10.0),
    );
    assert_eq!(d.mode, DimMode::PointLine);
    assert_eq!(d.kind, K::DistancePointLine(a2, l1, 20.0));
    commits_still(&s, &d);

    // Parallel mit Mittellinie: Durchmesser vom Punkt der anderen Linie.
    let mut c = s.clone();
    c.set_centerline(l1, true).unwrap();
    for picks in [
        [
            DimPick::Curve(l1, v(5.0, 0.0)),
            DimPick::Curve(l2, v(30.0, 20.0)),
        ],
        [
            DimPick::Curve(l2, v(30.0, 20.0)),
            DimPick::Curve(l1, v(5.0, 0.0)),
        ],
    ] {
        let d = plan(&c, &picks, v(30.0, 10.0));
        assert_eq!(d.kind, K::CenterlineDiameter(a2, l1, 40.0));
        assert_eq!(d.mode, DimMode::CenterlineDiameter);
        commits_still(&c, &d);
    }

    // Nicht parallel: Winkel, positiv, in beiden Reihenfolgen.
    let (l3, _, _) = line(&mut s, v(0.0, 0.0), v(-30.0, 30.0));
    for picks in [
        [
            DimPick::Curve(l1, v(5.0, 0.0)),
            DimPick::Curve(l3, v(-5.0, 5.0)),
        ],
        [
            DimPick::Curve(l3, v(-5.0, 5.0)),
            DimPick::Curve(l1, v(5.0, 0.0)),
        ],
        [
            DimPick::Curve(l2, v(30.0, 20.0)),
            DimPick::Curve(l3, v(-5.0, 5.0)),
        ],
    ] {
        let d = plan(&s, &picks, v(10.0, 10.0));
        assert_eq!(d.mode, DimMode::Angle);
        assert!(
            (value(&d.kind) - 0.75 * std::f64::consts::PI).abs() < 1e-12,
            "{d:?}"
        );
        commits_still(&s, &d);
    }
    // Dieselbe Linie zweimal: kein Maß.
    assert_eq!(
        s.dimension_plan(
            &[
                DimPick::Curve(l1, v(5.0, 0.0)),
                DimPick::Curve(l1, v(6.0, 0.0))
            ],
            v(0.0, 0.0)
        ),
        None
    );
}

#[test]
fn two_circles_measure_between_centres() {
    let mut s = sketch();
    let (m1, m2) = (pt(&mut s, 0.0, 0.0), pt(&mut s, 40.0, 30.0));
    let c1 = s.add_circle(m1, 5.0).unwrap();
    let c2 = s.add_circle(m2, 3.0).unwrap();
    let picks = [
        DimPick::Curve(c1, v(5.0, 0.0)),
        DimPick::Curve(c2, v(43.0, 30.0)),
    ];
    let h = plan(&s, &picks, v(20.0, 50.0));
    assert_eq!(
        (h.mode, h.kind.clone()),
        (DimMode::Horizontal, K::DistanceX(m1, m2, 40.0))
    );
    let vt = plan(&s, &picks, v(60.0, 15.0));
    assert_eq!(
        (vt.mode, vt.kind.clone()),
        (DimMode::Vertical, K::DistanceY(m1, m2, 30.0))
    );
    let al = plan(&s, &picks, v(60.0, 50.0));
    assert_eq!(al.mode, DimMode::Aligned);
    // Punkt und Kreismitte ebenso.
    let p = pt(&mut s, 40.0, 0.0);
    let d = plan(
        &s,
        &[DimPick::Point(p), DimPick::Curve(c1, v(5.0, 0.0))],
        v(20.0, -10.0),
    );
    assert_eq!(
        (d.mode, d.kind.clone()),
        (DimMode::Horizontal, K::DistanceX(p, m1, 40.0))
    );
    for p in [h, vt, al, d] {
        commits_still(&s, &p);
    }
}

/// Eine abgeschlossene Skizze bewegt sich durch das Anlegen nicht und ändert ihren Stand nur
/// beim Anlegen, nie durch den Plan.
#[test]
fn plan_does_not_touch_the_sketch() {
    let mut s = sketch();
    let (a, b) = (pt(&mut s, 0.0, 0.0), pt(&mut s, 40.0, 30.0));
    s.solve();
    let rev = s.revision();
    let n = s.points().count();
    let picks = [DimPick::Point(a), DimPick::Point(b)];
    for i in 0..50 {
        let _ = s.dimension_plan(&picks, v(i as f64, 50.0 - i as f64));
    }
    assert_eq!(s.revision(), rev);
    assert_eq!(s.points().count(), n);
}
