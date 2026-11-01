//! Dehnen und Brechen wie in Fusion: Dehnen verlängert ein offenes Ende bis zum nächsten
//! Schnitt mit einer anderen Kurve; Brechen teilt eine Kurve an den beiden Schnittstellen um
//! die Klickstelle, die Teile hängen deckungsgleich zusammen (Entscheidungen 7–9 des Plans).

use planora_math::{DVec2, Frame, PI, Tolerance};
use planora_sketch::constraint::{ConstraintKind as K, LineOrPoints};
use planora_sketch::model::{CurveId, CurveKind, PointId, Sketch};
use planora_sketch::solver::SolveStatus;

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

fn snapshot(s: &Sketch) -> Vec<(PointId, DVec2)> {
    s.points().map(|(p, _)| (p, xy(s, p))).collect()
}

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

/// „deckungsgleich“ zwischen `p` und `q` in einer der beiden Reihenfolgen.
fn coincident(s: &Sketch, p: PointId, q: PointId) -> bool {
    has(s, &K::Coincident(p, q)) || has(s, &K::Coincident(q, p))
}

fn close(a: DVec2, b: DVec2) -> bool {
    a.distance(b) < 1e-9
}

/// Lösen bewegt nichts, meldet „gelöst“ und keine überzähligen Abhängigkeiten.
fn assert_solve_keeps(s: &mut Sketch) {
    let before = snapshot(s);
    let r = s.solve();
    assert_eq!(r.status, SolveStatus::Solved, "{r:?}");
    assert!(r.redundant.is_empty(), "{r:?}");
    assert_unmoved(s, &before);
}

fn area(poly: &[DVec2]) -> f64 {
    let n = poly.len();
    (0..n)
        .map(|i| poly[i].perp_dot(poly[(i + 1) % n]))
        .sum::<f64>()
        / 2.0
}

/// Flächeninhalte aller Profile (äußerer Umriss minus Löcher), sortiert.
fn profile_areas(s: &Sketch) -> Vec<f64> {
    let e = s.profiles();
    assert!(e.rejected.is_empty(), "{:?}", e.rejected);
    let mut v: Vec<f64> = e
        .outlines
        .iter()
        .map(|loops| loops.iter().map(|l| area(l)).sum())
        .collect();
    v.sort_by(f64::total_cmp);
    v
}

fn assert_same_areas(a: &[f64], b: &[f64]) {
    assert_eq!(a.len(), b.len(), "{a:?} / {b:?}");
    for (x, y) in a.iter().zip(b) {
        assert!((x - y).abs() < 1e-6, "{a:?} / {b:?}");
    }
}

// ---------------------------------------------------------------- Dehnen

#[test]
fn extending_a_line_moves_its_open_end_onto_the_next_curve() {
    let mut s = sketch();
    let (l, a, b) = line(&mut s, 0.0, 0.0, 5.0, 0.0);
    let (v, _, _) = line(&mut s, 10.0, -5.0, 10.0, 5.0);
    assert!(s.extend(l, DVec2::new(4.5, 0.2)).unwrap());
    assert_eq!(ends(&s, l), (a, b), "der Endpunkt behält seine Id");
    assert!(close(xy(&s, b), DVec2::new(10.0, 0.0)), "{:?}", xy(&s, b));
    assert_eq!(xy(&s, a), DVec2::ZERO);
    assert!(has(&s, &K::PointOnCurve(b, v)), "{:?}", kinds(&s));
    assert_solve_keeps(&mut s);
}

#[test]
fn extending_without_a_target_changes_nothing() {
    let mut s = sketch();
    let (l, _, _) = line(&mut s, 0.0, 0.0, 5.0, 0.0);
    line(&mut s, 10.0, -5.0, 10.0, 5.0);
    let rev = s.revision();
    assert_eq!(s.extend_preview(l, DVec2::new(0.5, 0.0)), None);
    assert!(!s.extend(l, DVec2::new(0.5, 0.0)).unwrap());
    assert_eq!(s.revision(), rev, "Stand unverändert");
}

#[test]
fn the_preview_runs_from_the_old_to_the_new_end() {
    let mut s = sketch();
    let (l, _, _) = line(&mut s, 0.0, 0.0, 5.0, 0.0);
    line(&mut s, 10.0, -5.0, 10.0, 5.0);
    let rev = s.revision();
    let p = s.extend_preview(l, DVec2::new(4.0, 0.0)).unwrap();
    assert_eq!(p.len(), 2, "{p:?}");
    assert!(close(p[0], DVec2::new(5.0, 0.0)), "{p:?}");
    assert!(close(p[1], DVec2::new(10.0, 0.0)), "{p:?}");
    assert_eq!(s.revision(), rev);
}

#[test]
fn extending_an_arc_follows_its_circle_to_the_line() {
    let mut s = sketch();
    let m = s.add_point(0.0, 0.0).unwrap();
    let st = s.add_point(5.0, 0.0).unwrap();
    let en = s.add_point(0.0, 5.0).unwrap();
    let arc = s.add_arc(m, st, en).unwrap();
    let (v, _, _) = line(&mut s, -3.0, -10.0, -3.0, 10.0);
    // Vorschau folgt dem Kreis: alle Punkte im Abstand 5 von der Mitte
    let p = s.extend_preview(arc, DVec2::new(0.3, 5.0)).unwrap();
    assert!(p.len() > 2, "{p:?}");
    assert!(close(p[0], DVec2::new(0.0, 5.0)), "{p:?}");
    assert!(close(*p.last().unwrap(), DVec2::new(-3.0, 4.0)), "{p:?}");
    assert!(p.iter().all(|q| (q.length() - 5.0).abs() < 1e-9));
    // Ende gegen den Uhrzeigersinn bis (-3, 4)
    assert!(s.extend(arc, DVec2::new(0.3, 5.0)).unwrap());
    assert!(close(xy(&s, en), DVec2::new(-3.0, 4.0)), "{:?}", xy(&s, en));
    assert!(has(&s, &K::PointOnCurve(en, v)), "{:?}", kinds(&s));
    // Anfang im Uhrzeigersinn bis (-3, -4)
    assert!(s.extend(arc, DVec2::new(5.0, -0.3)).unwrap());
    assert!(
        close(xy(&s, st), DVec2::new(-3.0, -4.0)),
        "{:?}",
        xy(&s, st)
    );
    assert!(has(&s, &K::PointOnCurve(st, v)), "{:?}", kinds(&s));
    assert_eq!(ends(&s, arc), (st, en));
    assert_solve_keeps(&mut s);
}

#[test]
fn an_arc_stops_short_of_its_own_start() {
    // Die einzige weitere Kurve schneidet den Kreis nur innerhalb des Bogens.
    let mut s = sketch();
    let m = s.add_point(0.0, 0.0).unwrap();
    let st = s.add_point(5.0, 0.0).unwrap();
    let en = s.add_point(-5.0, 0.0).unwrap();
    let arc = s.add_arc(m, st, en).unwrap();
    line(&mut s, 0.0, 0.0, 0.0, 10.0);
    let rev = s.revision();
    assert!(!s.extend(arc, DVec2::new(-5.0, 0.3)).unwrap());
    assert_eq!(s.revision(), rev);
}

#[test]
fn extending_an_elliptic_arc_follows_its_ellipse() {
    let mut s = sketch();
    let m = s.add_point(0.0, 0.0).unwrap();
    let st = s.add_point(10.0, 0.0).unwrap();
    let en = s.add_point(0.0, 5.0).unwrap();
    let e = s
        .add_elliptic_arc(m, DVec2::new(10.0, 0.0), 5.0, st, en)
        .unwrap();
    let (v, _, _) = line(&mut s, -6.0, -10.0, -6.0, 10.0);
    assert!(s.extend(e, DVec2::new(0.5, 5.0)).unwrap());
    assert!(close(xy(&s, en), DVec2::new(-6.0, 4.0)), "{:?}", xy(&s, en));
    assert!(has(&s, &K::PointOnCurve(en, v)), "{:?}", kinds(&s));
    assert_solve_keeps(&mut s);
}

#[test]
fn construction_curves_are_targets_too() {
    let mut s = sketch();
    let (l, _, b) = line(&mut s, 0.0, 0.0, 5.0, 0.0);
    let (v, _, _) = line(&mut s, 10.0, -5.0, 10.0, 5.0);
    s.set_construction(v, true).unwrap();
    assert!(s.extend(l, DVec2::new(5.0, 0.0)).unwrap());
    assert!(close(xy(&s, b), DVec2::new(10.0, 0.0)));
}

#[test]
fn the_nearest_target_wins_and_lands_on_its_point() {
    let mut s = sketch();
    let (l, _, b) = line(&mut s, 0.0, 0.0, 5.0, 0.0);
    line(&mut s, 20.0, -5.0, 20.0, 5.0);
    // Ziel endet genau auf der Verlängerung: deckungsgleich mit seinem Endpunkt
    let (_, q, _) = line(&mut s, 8.0, 0.0, 8.0, 5.0);
    assert!(s.extend(l, DVec2::new(5.0, 0.0)).unwrap());
    assert!(close(xy(&s, b), DVec2::new(8.0, 0.0)), "{:?}", xy(&s, b));
    assert!(coincident(&s, b, q), "{:?}", kinds(&s));
    assert_solve_keeps(&mut s);
}

#[test]
fn a_shared_rectangle_corner_is_not_an_open_end() {
    let mut s = sketch();
    let r = s
        .add_rectangle(DVec2::new(0.0, 0.0), DVec2::new(10.0, 10.0))
        .unwrap();
    line(&mut s, 20.0, -50.0, 20.0, 50.0);
    line(&mut s, -50.0, 20.0, 50.0, 20.0);
    line(&mut s, -20.0, -50.0, -20.0, 50.0);
    line(&mut s, -50.0, -20.0, 50.0, -20.0);
    let rev = s.revision();
    for (k, &c) in r.lines.iter().enumerate() {
        let (a, b) = ends(&s, c);
        for p in [a, b] {
            let at = xy(&s, p);
            assert_eq!(s.extend_preview(c, at), None, "Seite {k}");
            assert!(!s.extend(c, at).unwrap(), "Seite {k}");
        }
    }
    assert_eq!(s.revision(), rev);
}

#[test]
fn an_end_with_a_coincident_constraint_is_not_open() {
    let mut s = sketch();
    let (l, _, b) = line(&mut s, 0.0, 0.0, 5.0, 0.0);
    let (_, c, _) = line(&mut s, 5.0, 0.0, 5.0, -3.0);
    s.add_constraint(K::Coincident(b, c)).unwrap();
    line(&mut s, 10.0, -5.0, 10.0, 5.0);
    let rev = s.revision();
    assert!(!s.extend(l, DVec2::new(5.0, 0.0)).unwrap());
    assert_eq!(s.revision(), rev);
}

#[test]
fn extending_keeps_satisfied_constraints_and_drops_the_length() {
    let mut s = sketch();
    let (l, a, b) = line(&mut s, 0.0, 0.0, 5.0, 0.0);
    line(&mut s, 10.0, -5.0, 10.0, 5.0);
    s.add_constraint(K::Horizontal(LineOrPoints::Line(l)))
        .unwrap();
    s.add_constraint(K::Distance(a, b, 5.0)).unwrap();
    assert!(s.extend(l, DVec2::new(5.0, 0.0)).unwrap());
    assert!(has(&s, &K::Horizontal(LineOrPoints::Line(l))));
    assert!(
        !kinds(&s).iter().any(|k| matches!(k, K::Distance(..))),
        "{:?}",
        kinds(&s)
    );
    assert_solve_keeps(&mut s);
}

#[test]
fn circles_and_splines_do_not_extend() {
    let mut s = sketch();
    let m = s.add_point(0.0, 0.0).unwrap();
    let c = s.add_circle(m, 5.0).unwrap();
    line(&mut s, 10.0, -5.0, 10.0, 5.0);
    let rev = s.revision();
    assert!(!s.extend(c, DVec2::new(5.0, 0.0)).unwrap());
    assert_eq!(s.revision(), rev);
}

// ---------------------------------------------------------------- Brechen

#[test]
fn breaking_a_line_between_two_crossings_gives_three_coincident_lines() {
    let mut s = sketch();
    let (h, a, b) = line(&mut s, 0.0, 0.0, 10.0, 0.0);
    let (v1, _, _) = line(&mut s, 3.0, -5.0, 3.0, 5.0);
    let (v2, _, _) = line(&mut s, 7.0, -5.0, 7.0, 5.0);
    let before = snapshot(&s);
    let out = s.break_curve(h, DVec2::new(5.0, 0.2)).unwrap();
    assert_eq!(out.len(), 3, "{out:?}");
    assert_eq!(out[0], h);
    let e: Vec<(PointId, PointId)> = out.iter().map(|&c| ends(&s, c)).collect();
    assert_eq!(e[0].0, a);
    assert_eq!(e[2].1, b);
    let want = [(0.0, 3.0), (3.0, 7.0), (7.0, 10.0)];
    for (k, &(x0, x1)) in want.iter().enumerate() {
        assert!(close(xy(&s, e[k].0), DVec2::new(x0, 0.0)), "Teil {k}");
        assert!(close(xy(&s, e[k].1), DVec2::new(x1, 0.0)), "Teil {k}");
    }
    // getrennte Punkte an den Trennstellen, deckungsgleich verbunden
    assert_ne!(e[0].1, e[1].0);
    assert_ne!(e[1].1, e[2].0);
    assert!(coincident(&s, e[0].1, e[1].0), "{:?}", kinds(&s));
    assert!(coincident(&s, e[1].1, e[2].0), "{:?}", kinds(&s));
    // Regel 7 für die schneidenden Kurven
    let on = |p: PointId, q: PointId, k: CurveId| {
        has(&s, &K::PointOnCurve(p, k)) || has(&s, &K::PointOnCurve(q, k))
    };
    assert!(on(e[0].1, e[1].0, v1), "{:?}", kinds(&s));
    assert!(on(e[1].1, e[2].0, v2), "{:?}", kinds(&s));
    assert_unmoved(&s, &before);
    assert_solve_keeps(&mut s);
}

#[test]
fn breaking_near_the_end_splits_at_the_one_crossing() {
    let mut s = sketch();
    let (h, _, _) = line(&mut s, 0.0, 0.0, 10.0, 0.0);
    line(&mut s, 3.0, -5.0, 3.0, 5.0);
    let out = s.break_curve(h, DVec2::new(1.0, 0.0)).unwrap();
    assert_eq!(out.len(), 2);
    assert!(close(xy(&s, ends(&s, out[0]).1), DVec2::new(3.0, 0.0)));
    assert!(close(xy(&s, ends(&s, out[1]).0), DVec2::new(3.0, 0.0)));
    assert_solve_keeps(&mut s);
}

#[test]
fn breaking_without_a_crossing_changes_nothing() {
    let mut s = sketch();
    let (h, _, _) = line(&mut s, 0.0, 0.0, 10.0, 0.0);
    let rev = s.revision();
    assert_eq!(s.break_curve(h, DVec2::new(5.0, 0.0)).unwrap(), vec![h]);
    assert_eq!(s.revision(), rev);
}

#[test]
fn a_circle_with_two_cuts_breaks_into_two_arcs_with_the_same_center() {
    let mut s = sketch();
    let m = s.add_point(0.0, 0.0).unwrap();
    let c = s.add_circle(m, 5.0).unwrap();
    line(&mut s, -10.0, 0.0, 10.0, 0.0);
    let areas = profile_areas(&s);
    assert_eq!(areas.len(), 2);
    let before = snapshot(&s);
    let out = s.break_curve(c, DVec2::new(0.0, 5.0)).unwrap();
    assert_eq!(out.len(), 2, "{out:?}");
    assert_eq!(out[0], c);
    let mut joints = Vec::new();
    for &k in &out {
        let CurveKind::Arc { center, start, end } = s.curve(k).unwrap().kind else {
            panic!("kein Bogen: {:?}", s.curve(k).unwrap().kind);
        };
        assert_eq!(center, m);
        joints.push((start, end));
    }
    // Ende des einen Bogens deckungsgleich mit dem Anfang des anderen, an beiden Stellen
    assert!(coincident(&s, joints[0].1, joints[1].0), "{:?}", kinds(&s));
    assert!(coincident(&s, joints[1].1, joints[0].0), "{:?}", kinds(&s));
    assert_unmoved(&s, &before);
    assert_same_areas(&profile_areas(&s), &areas);
    assert!((areas[0] - 12.5 * PI).abs() < 0.1, "{areas:?}");
    assert_solve_keeps(&mut s);
}

#[test]
fn a_circle_with_one_cut_stays_whole() {
    let mut s = sketch();
    let m = s.add_point(0.0, 0.0).unwrap();
    let c = s.add_circle(m, 5.0).unwrap();
    line(&mut s, 0.0, 0.0, 10.0, 0.0);
    let rev = s.revision();
    assert_eq!(s.break_curve(c, DVec2::new(0.0, 5.0)).unwrap(), vec![c]);
    assert_eq!(s.revision(), rev);
}

#[test]
fn an_ellipse_breaks_into_two_elliptic_arcs_sharing_the_axes() {
    let mut s = sketch();
    let m = s.add_point(0.0, 0.0).unwrap();
    let e = s.add_ellipse(m, DVec2::new(10.0, 0.0), 5.0).unwrap();
    line(&mut s, -20.0, 0.0, 20.0, 0.0);
    let areas = profile_areas(&s);
    let out = s.break_curve(e, DVec2::new(0.0, 5.0)).unwrap();
    assert_eq!(out.len(), 2, "{out:?}");
    let axes = |k: CurveId| match s.curve(k).unwrap().kind {
        CurveKind::EllipticArc {
            center,
            ax,
            ay,
            minor,
            ..
        } => (center, ax, ay, minor),
        ref k => panic!("kein Ellipsenbogen: {k:?}"),
    };
    assert_eq!(axes(out[0]), axes(out[1]));
    assert_same_areas(&profile_areas(&s), &areas);
    assert_solve_keeps(&mut s);
}

#[test]
fn breaking_an_arc_keeps_the_profiles() {
    let mut s = sketch();
    // Halbkreis über einer Linie, von einer senkrechten Linie geteilt
    let m = s.add_point(0.0, 0.0).unwrap();
    let st = s.add_point(5.0, 0.0).unwrap();
    let en = s.add_point(-5.0, 0.0).unwrap();
    let arc = s.add_arc(m, st, en).unwrap();
    let base = s.add_line(en, st).unwrap();
    line(&mut s, 0.0, 0.0, 0.0, 10.0);
    let areas = profile_areas(&s);
    assert_eq!(areas.len(), 2);
    let out = s.break_curve(arc, DVec2::new(3.0, 4.0)).unwrap();
    assert_eq!(out.len(), 2, "{out:?}");
    assert_same_areas(&profile_areas(&s), &areas);
    // dann die Linie am Mittelpunkt brechen: die Teile hängen am Bogenende deckungsgleich
    let parts = s.break_curve(base, DVec2::new(-1.0, 0.0)).unwrap();
    assert_eq!(parts.len(), 2);
    assert_same_areas(&profile_areas(&s), &areas);
    assert_solve_keeps(&mut s);
}

#[test]
fn breaking_a_spline_keeps_its_knots_in_the_pieces() {
    let mut s = sketch();
    let pts: Vec<PointId> = [(0.0, 0.0), (2.0, 1.5), (5.0, 2.0), (8.0, 1.5), (10.0, 0.0)]
        .iter()
        .map(|&(x, y)| s.add_point(x, y).unwrap())
        .collect();
    let sp = s.add_spline(&pts).unwrap();
    line(&mut s, 3.0, -5.0, 3.0, 5.0);
    line(&mut s, 7.0, -5.0, 7.0, 5.0);
    let out = s.break_curve(sp, DVec2::new(5.0, 2.0)).unwrap();
    assert_eq!(out.len(), 3, "{out:?}");
    let knots = |k: CurveId| match &s.curve(k).unwrap().kind {
        CurveKind::Spline { points } => points.clone(),
        k => panic!("kein Spline: {k:?}"),
    };
    let (a, m, b) = (knots(out[0]), knots(out[1]), knots(out[2]));
    assert_eq!(a[0], pts[0]);
    assert!(m.contains(&pts[2]), "{m:?}");
    assert_eq!(*b.last().unwrap(), pts[4]);
    assert!(coincident(&s, *a.last().unwrap(), m[0]), "{:?}", kinds(&s));
    assert!(coincident(&s, *m.last().unwrap(), b[0]), "{:?}", kinds(&s));
    assert!((xy(&s, m[0]).x - 3.0).abs() < 1e-9);
    assert!((xy(&s, b[0]).x - 7.0).abs() < 1e-9);
}
