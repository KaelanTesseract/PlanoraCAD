//! Reine Wahl des Maßes aus Auswahl und Zeiger (Skizzen 5, Task 6, Entscheidung 13): Zeigerregel
//! für waagerecht, senkrecht und ausgerichtet über ein dichtes Gitter, Kurven allein, Punkt und
//! Linie, Mittellinie mit Umschalter Ø/R, zwei Linien, zwei Kreise; Vorschau = Ergebnis und
//! keine Änderung an der Skizze.

use planora_math::{DVec2, Frame, Tolerance};
use planora_sketch::constraint::ConstraintKind as K;
use planora_sketch::dimension::{DimMode, DimPick, DimPlan};
use planora_sketch::model::{CurveId, PointId, Sketch};
use planora_sketch::pattern::{PatternDir, PatternKind, PatternPlan};

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

/// Dichtes Gitter `n`×`n` über dem Bereich `lo..hi`: Die Art wechselt nur dort, wo eine Kante
/// der x- oder y-Spanne von `a` und `b` zwischen zwei benachbarten Zeigerlagen liegt. Liefert die
/// gesehenen Arten.
fn edge_grid(
    s: &Sketch,
    picks: &[DimPick],
    (a, b): (DVec2, DVec2),
    (lo, hi): (DVec2, DVec2),
    n: usize,
) -> std::collections::HashSet<String> {
    let coord = |i: usize, l: f64, h: f64| l + (h - l) * i as f64 / (n - 1) as f64;
    let grid: Vec<Vec<DimMode>> = (0..n)
        .map(|i| {
            (0..n)
                .map(|j| plan(s, picks, v(coord(i, lo.x, hi.x), coord(j, lo.y, hi.y))).mode)
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
                let (x0, x1) = (coord(i, lo.x, hi.x), coord(i + 1, lo.x, hi.x));
                assert!(crosses(x0, x1, [a.x, b.x]), "Sprung bei x {x0}..{x1}");
            }
            if j + 1 < n && grid[i][j] != grid[i][j + 1] {
                let (y0, y1) = (coord(j, lo.y, hi.y), coord(j + 1, lo.y, hi.y));
                assert!(crosses(y0, y1, [a.y, b.y]), "Sprung bei y {y0}..{y1}");
            }
        }
    }
    seen
}

/// Gitter `n`×`n` über `lo..hi`, jeder Plan wird angelegt und gelöst: nichts bewegt sich.
fn commit_grid(s: &Sketch, picks: &[DimPick], (lo, hi): (DVec2, DVec2), n: usize) {
    for i in 0..n {
        for j in 0..n {
            let x = lo.x + (hi.x - lo.x) * i as f64 / (n - 1) as f64;
            let y = lo.y + (hi.y - lo.y) * j as f64 / (n - 1) as f64;
            commits_still(s, &plan(s, picks, v(x, y)));
        }
    }
}

/// Dichtes Gitter 200×200 um die Punkte, dazu 50×50 angelegt.
#[test]
fn dense_pointer_grid_switches_only_at_span_edges() {
    let mut s = sketch();
    let (a, b) = (pt(&mut s, 0.0, 0.0), pt(&mut s, 40.0, 30.0));
    let picks = [DimPick::Point(a), DimPick::Point(b)];
    let area = (v(-40.0, -30.0), v(80.0, 60.0));
    let seen = edge_grid(&s, &picks, (v(0.0, 0.0), v(40.0, 30.0)), area, 200);
    assert_eq!(seen.len(), 3, "{seen:?}");
    commit_grid(&s, &picks, area, 50);
}

/// Eine Linie allein folgt der Zeigerregel ihrer Enden wie zwei Punkte (Korrektur 1, A).
#[test]
fn single_line_follows_the_pointer() {
    let mut s = sketch();
    let (l, a, b) = line(&mut s, v(0.0, 0.0), v(40.0, 30.0));
    let pick = [DimPick::Curve(l, v(20.0, 15.0))];
    let h = plan(&s, &pick, v(20.0, 50.0));
    assert_eq!(
        (h.mode, h.kind.clone()),
        (DimMode::Horizontal, K::DistanceX(a, b, 40.0))
    );
    let vt = plan(&s, &pick, v(60.0, 15.0));
    assert_eq!(
        (vt.mode, vt.kind.clone()),
        (DimMode::Vertical, K::DistanceY(a, b, 30.0))
    );
    let al = plan(&s, &pick, v(60.0, 50.0));
    assert_eq!(al.mode, DimMode::Aligned);
    assert!(matches!(al.kind, K::Distance(p, q, _) if p == a && q == b));
    let area = (v(-40.0, -30.0), v(80.0, 60.0));
    let seen = edge_grid(&s, &pick, (v(0.0, 0.0), v(40.0, 30.0)), area, 200);
    assert_eq!(seen.len(), 3, "{seen:?}");
    commit_grid(&s, &pick, area, 50);

    // Waagerechte Linie: nie senkrecht; senkrechte Linie: nie waagerecht.
    let (hl, ..) = line(&mut s, v(0.0, 100.0), v(30.0, 100.0));
    let (vl, ..) = line(&mut s, v(100.0, 0.0), v(100.0, 30.0));
    let n = 50;
    for i in 0..n {
        for j in 0..n {
            let x = -20.0 + 70.0 * i as f64 / (n - 1) as f64;
            let y = -20.0 + 70.0 * j as f64 / (n - 1) as f64;
            let p = plan(&s, &[DimPick::Curve(hl, v(10.0, 100.0))], v(x, 80.0 + y));
            assert_ne!(p.mode, DimMode::Vertical, "{p:?}");
            commits_still(&s, &p);
            let p = plan(&s, &[DimPick::Curve(vl, v(100.0, 10.0))], v(80.0 + x, y));
            assert_ne!(p.mode, DimMode::Horizontal, "{p:?}");
            commits_still(&s, &p);
        }
    }
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

/// Winkel zweier Linien bei Zeigerlagen rundum: immer derselbe positive Wert in (0, π), und das
/// Anlegen bewegt nichts. Den Ergänzungswinkel wählt der Zeiger nicht (Korrektur 1, B:
/// `Angle` kann `π − θ` nicht mit passendem Messwert ausdrücken).
#[test]
fn angle_quadrant_sweep_commits_still() {
    let mut s = sketch();
    let (l1, _, _) = line(&mut s, v(0.0, 0.0), v(50.0, 0.0));
    let (l2, _, _) = line(&mut s, v(0.0, 0.0), v(20.0, 30.0));
    let theta = 30.0f64.atan2(20.0);
    for picks in [
        [
            DimPick::Curve(l1, v(5.0, 0.0)),
            DimPick::Curve(l2, v(2.0, 3.0)),
        ],
        [
            DimPick::Curve(l2, v(2.0, 3.0)),
            DimPick::Curve(l1, v(5.0, 0.0)),
        ],
    ] {
        for k in 0..360 {
            let a = (k as f64).to_radians();
            let p = plan(&s, &picks, v(25.0 * a.cos(), 25.0 * a.sin()));
            assert_eq!(p.mode, DimMode::Angle);
            let x = value(&p.kind);
            assert!(x > 0.0 && x < std::f64::consts::PI, "{p:?}");
            assert!((x - theta).abs() < 1e-12, "{p:?}");
            if k % 10 == 0 {
                commits_still(&s, &p);
            }
        }
    }
}

#[test]
fn elliptic_arc_axis_follows_the_pointer() {
    let mut s = sketch();
    let m = pt(&mut s, 0.0, 0.0);
    // Große Halbachse (12, 16), Länge 20, kleine 8; Viertelbogen von t = 0 bis t = π/2.
    let a = v(12.0, 16.0);
    let b = v(-16.0, 12.0) / 20.0 * 8.0;
    let (st, en) = (pt(&mut s, a.x, a.y), pt(&mut s, b.x, b.y));
    let e = s.add_elliptic_arc(m, a, 8.0, st, en).unwrap();
    let pick = [DimPick::Curve(e, a)];
    let p = plan(&s, &pick, a * 1.5 + b * 0.1);
    assert_eq!(p.mode, DimMode::Major);
    assert!(matches!(p.kind, K::EllipseMajor(c, _) if c == e));
    assert!((value(&p.kind) - 20.0).abs() < 1e-12);
    commits_still(&s, &p);
    let p = plan(&s, &pick, b * 2.0 + a * 0.05);
    assert_eq!(p.mode, DimMode::Minor);
    assert!(matches!(p.kind, K::EllipseMinor(c, _) if c == e));
    assert!((value(&p.kind) - 8.0).abs() < 1e-12);
    commits_still(&s, &p);
}

/// Was `add_constraint` als Schleife zwischen Vorlage und eigenem Exemplar ablehnt, plant das
/// Bemaßen nicht (Korrektur 1, I1); zwischen Exemplaren und von freien Punkten geht es.
#[test]
fn no_plan_that_closes_a_pattern_cycle() {
    let mut s = sketch();
    let r = s.add_rectangle(v(0.0, 0.0), v(10.0, 6.0)).unwrap();
    let id = s
        .add_pattern(&PatternPlan {
            kind: PatternKind::Rect {
                dirs: [PatternDir::SketchX, PatternDir::SketchY],
                extent: [false, false],
                symmetric: [false, false],
            },
            sources: r.lines.to_vec(),
            counts: [3, 1],
            values: [20.0, 10.0],
            suppressed: vec![],
        })
        .unwrap();
    s.solve();
    let d = s.pattern(id).unwrap().clone();
    let src = r.points[0];
    let k = d.source_points.iter().position(|&x| x == src).unwrap();
    let inst = |at: [i32; 2]| d.instances.iter().find(|i| i.at == at).unwrap();
    let (i1, i2) = (inst([1, 0]).points[k], inst([2, 0]).points[k]);
    let pointer = v(20.0, 30.0);

    // Vorlagenecke und eigenes Exemplar: add_constraint lehnt ab, also kein Plan.
    let picks = [DimPick::Point(src), DimPick::Point(i1)];
    assert!(
        s.clone()
            .add_constraint(K::Distance(src, i1, 20.0))
            .is_err()
    );
    assert_eq!(s.dimension_plan(&picks, pointer), None);
    // Vorlagenpunkt und Exemplarlinie.
    let line_of_instance = inst([1, 0]).curves[0];
    let picks = [
        DimPick::Point(src),
        DimPick::Curve(line_of_instance, v(25.0, 0.0)),
    ];
    assert_eq!(s.dimension_plan(&picks, pointer), None);

    // Exemplar zu Exemplar, freier Punkt zu Exemplar: geplant und angelegt.
    let free = pt(&mut s, -20.0, 30.0);
    s.solve();
    for picks in [
        [DimPick::Point(i1), DimPick::Point(i2)],
        [DimPick::Point(free), DimPick::Point(i1)],
    ] {
        for pointer in [v(30.0, 40.0), v(60.0, 15.0), v(-40.0, -40.0)] {
            let p = plan(&s, &picks, pointer);
            commits_still(&s, &p);
            // Schlusskorrektur I1: Nur Exemplare ⇒ anzeigend; Vorschau = Ergebnis.
            let only_instances = matches!(picks[0], DimPick::Point(x) if x == i1);
            assert_eq!(p.driven, only_instances, "{p:?}");
            let mut c = s.clone();
            let id = c.add_constraint(p.kind.clone()).unwrap();
            let made = c.constraint(id).unwrap();
            assert_eq!(made.driven, p.driven, "{p:?}");
            assert_eq!(made.kind, p.kind, "{p:?}");
        }
    }
}
