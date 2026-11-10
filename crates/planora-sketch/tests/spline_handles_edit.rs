//! Griffe in Stutzen, Brechen, Kopieren, Spiegeln, Muster, Fang und Versatz (Skizzen 7, Task 8; Spec,
//! Tabelle E, Spalte „Spline mit Griffen“).

use planora_math::{DVec2, Frame, Tolerance};
use planora_sketch::constraint::ConstraintKind as K;
use planora_sketch::fillet::CurvePick;
use planora_sketch::model::{CurveId, CurveKind, PointId, Sketch};
use planora_sketch::pattern::{CircularMode, PatternDir, PatternId, PatternKind, PatternPlan};
use planora_sketch::snap::{Snap, SnapIndex};
use planora_sketch::solver::SolveStatus;
use planora_sketch::transform::Similarity;
use planora_sketch::{HandleCurvature, SplineHandle};

const WAVE: [(f64, f64); 6] = [
    (0.0, 0.0),
    (10.0, 6.0),
    (21.0, -3.0),
    (30.0, 8.0),
    (42.0, 5.0),
    (50.0, -7.0),
];

fn sketch() -> Sketch {
    Sketch::new(Frame::WORLD, "S", Tolerance::default())
}

fn v(x: f64, y: f64) -> DVec2 {
    DVec2::new(x, y)
}

fn spline(s: &mut Sketch, xy: &[(f64, f64)]) -> (CurveId, Vec<PointId>) {
    let pts: Vec<PointId> = xy
        .iter()
        .map(|&(x, y)| s.add_point(x, y).unwrap())
        .collect();
    (s.add_spline(&pts).unwrap(), pts)
}

fn line(s: &mut Sketch, a: (f64, f64), b: (f64, f64)) -> CurveId {
    let p = s.add_point(a.0, a.1).unwrap();
    let q = s.add_point(b.0, b.1).unwrap();
    s.add_line(p, q).unwrap()
}

fn handles(s: &Sketch, c: CurveId) -> Vec<SplineHandle> {
    let CurveKind::Spline { handles, .. } = &s.curve(c).unwrap().kind else {
        panic!("kein Spline");
    };
    handles.clone()
}

fn handle_at(s: &Sketch, c: CurveId, at: PointId) -> SplineHandle {
    handles(s, c)
        .into_iter()
        .find(|h| h.at == at)
        .expect("kein Griff")
}

fn fit_points(s: &Sketch, c: CurveId) -> Vec<PointId> {
    let CurveKind::Spline { points, .. } = &s.curve(c).unwrap().kind else {
        panic!("kein Spline");
    };
    points.clone()
}

fn arc_parts(h: &SplineHandle) -> (CurveId, PointId, PointId) {
    let HandleCurvature::Arc { arc, center, other } = h.curvature else {
        panic!("kein Krümmungsgriff");
    };
    (arc, center, other)
}

fn pos(s: &Sketch, p: PointId) -> DVec2 {
    s.point_xy(p).unwrap()
}

fn rows(s: &Sketch) -> Vec<(CurveId, PointId)> {
    s.internal_constraints()
        .filter_map(|(_, d)| match d.kind {
            K::SplineCurvatureHandle(c, at) => Some((c, at)),
            _ => None,
        })
        .collect()
}

/// Die Skizze lädt: `from_data` prüft Griffe und Zeilen.
fn loads(s: &Sketch) {
    Sketch::from_data(s.to_data()).expect("Griffe und Zeilen passen nicht zusammen");
}

fn dense(s: &Sketch, c: CurveId) -> Vec<DVec2> {
    let (lo, hi) = s.curve_domain(c).unwrap();
    (0..=400)
        .map(|k| {
            s.curve_eval(c, lo + (hi - lo) * f64::from(k) / 400.0)
                .unwrap()
        })
        .collect()
}

fn assert_mapped(s: &Sketch, a: CurveId, b: CurveId, map: impl Fn(DVec2) -> DVec2, tol: f64) {
    for (p, q) in dense(s, a).into_iter().zip(dense(s, b)) {
        assert!(map(p).distance(q) < tol, "{p} -> {q}");
    }
}

fn radius(s: &Sketch, arc: CurveId) -> f64 {
    let CurveKind::Arc { center, start, .. } = s.curve(arc).unwrap().kind else {
        panic!("kein Bogen");
    };
    pos(s, center).distance(pos(s, start))
}

fn rect_plan(sources: Vec<CurveId>, n: u32, spacing: f64) -> PatternPlan {
    PatternPlan {
        kind: PatternKind::Rect {
            dirs: [PatternDir::SketchX, PatternDir::SketchY],
            extent: [false, false],
            symmetric: [false, false],
        },
        sources,
        counts: [n, 1],
        values: [spacing, 10.0],
        suppressed: vec![],
    }
}

/// Griffe: Tangente an `pts[1]`, Krümmung an `pts[4]`.
fn handled_wave(s: &mut Sketch) -> (CurveId, Vec<PointId>) {
    let (c, pts) = spline(s, &WAVE);
    s.activate_tangent_handle(c, pts[1]).unwrap();
    s.activate_curvature_handle(c, pts[4]).unwrap();
    (c, pts)
}

// ---- Stutzen und Brechen --------------------------------------------------------------------

#[test]
fn trim_keeps_handles_on_remaining_points() {
    let mut s = sketch();
    let (c, pts) = spline(&mut s, &WAVE);
    // Griffe mitten im Stück, das wegfällt: Sie schneiden nicht.
    for k in [1, 2, 3, 4] {
        s.activate_tangent_handle(c, pts[k]).unwrap();
    }
    s.activate_curvature_handle(c, pts[4]).unwrap();
    let (kept, moved) = (handle_at(&s, c, pts[1]), handle_at(&s, c, pts[4]));
    let (tip1, tip4) = (pos(&s, kept.tip), pos(&s, moved.tip));
    let (arc, center, other) = arc_parts(&moved);
    let (center_at, other_at) = (pos(&s, center), pos(&s, other));
    line(&mut s, (15.0, -40.0), (15.0, 40.0));
    line(&mut s, (36.0, -40.0), (36.0, 40.0));

    let out = s.trim(c, v(25.5, 3.0)).unwrap();
    assert_eq!(out.len(), 2, "{out:?}");
    assert_eq!(out[0], c);
    let n = out[1];
    assert_eq!(fit_points(&s, c)[..2], [pts[0], pts[1]]);
    assert_eq!(fit_points(&s, n)[1..], [pts[4], pts[5]]);
    // Geschnitten wurde an den Linien, nicht an den Griffen dazwischen.
    let (lo, hi) = s.curve_domain(c).unwrap();
    assert!(
        (s.curve_eval(c, hi).unwrap().x - 15.0).abs() < 1e-6,
        "{lo} {hi}"
    );
    let (lo, _) = s.curve_domain(n).unwrap();
    assert!((s.curve_eval(n, lo).unwrap().x - 36.0).abs() < 1e-6);

    // Griffe bleiben samt Lage und Kurve; die Zeile zieht mit ihrem Griff um.
    assert_eq!(handles(&s, c), vec![kept]);
    assert_eq!(handles(&s, n), vec![moved]);
    assert_eq!(pos(&s, handle_at(&s, c, pts[1]).tip), tip1);
    assert_eq!(pos(&s, handle_at(&s, n, pts[4]).tip), tip4);
    assert_eq!(pos(&s, center), center_at);
    assert_eq!(pos(&s, other), other_at);
    assert!(s.curve(arc).is_some());
    assert_eq!(rows(&s), vec![(n, pts[4])]);
    loads(&s);
    let rep = s.solve();
    assert_eq!(rep.status, SolveStatus::Solved, "{rep:?}");
    assert!(rep.redundant.is_empty(), "{rep:?}");
}

#[test]
fn trim_drops_handles_on_removed_points() {
    let mut s = sketch();
    let (c, pts) = handled_wave(&mut s);
    let gone = handle_at(&s, c, pts[4]);
    let (arc, center, other) = arc_parts(&gone);
    let dim = s
        .add_constraint(K::Distance(pts[4], gone.tip, 3.0))
        .unwrap();
    let cutter = line(&mut s, (25.5, -40.0), (25.5, 40.0));

    let out = s.trim(c, v(45.0, 3.0)).unwrap();
    assert_eq!(out, vec![c]);
    assert_eq!(handles(&s, c).len(), 1);
    assert_eq!(handles(&s, c)[0].at, pts[1]);
    assert!(s.curve(gone.line).is_none());
    assert!(s.curve(arc).is_none());
    for p in [pts[4], gone.tip, center, other] {
        assert!(s.point_xy(p).is_err());
    }
    assert!(s.constraint(dim).is_none());
    assert!(rows(&s).is_empty());
    assert!(s.curve(cutter).is_some());
    loads(&s);
    assert_eq!(s.solve().status, SolveStatus::Solved);
}

#[test]
fn a_new_end_is_natural_and_a_kept_end_keeps_its_handle() {
    let mut s = sketch();
    let (c, pts) = spline(&mut s, &WAVE);
    s.activate_tangent_handle(c, pts[0]).unwrap();
    let tip = handle_at(&s, c, pts[0]).tip;
    line(&mut s, (25.5, -40.0), (25.5, 40.0));
    s.trim(c, v(45.0, 3.0)).unwrap();

    let (lo, hi) = s.curve_domain(c).unwrap();
    // Am alten Ende gilt der Griff, am neuen Ende ist der Spline natürlich: zweite Ableitung null.
    let h = 1e-3;
    let d0 = (s.curve_eval(c, lo + h).unwrap() - s.curve_eval(c, lo).unwrap()) / h;
    let want = (pos(&s, tip) - pos(&s, pts[0])) * 3.0;
    assert!(d0.distance(want) < 0.05, "{d0} {want}");
    let f = |t: f64| s.curve_eval(c, t).unwrap();
    let second = (f(hi) - f(hi - h) * 2.0 + f(hi - 2.0 * h)) / (h * h);
    assert!(second.length() < 0.2, "{second}");
}

#[test]
fn break_moves_handles_and_rows_with_their_piece() {
    let mut s = sketch();
    let (c, pts) = handled_wave(&mut s);
    let moved = handle_at(&s, c, pts[4]);
    line(&mut s, (25.5, -40.0), (25.5, 40.0));

    let out = s.break_curve(c, v(10.0, 6.0)).unwrap();
    assert_eq!(out.len(), 2, "{out:?}");
    let n = out[1];
    assert_eq!(handles(&s, c).len(), 1);
    assert_eq!(handles(&s, c)[0].at, pts[1]);
    assert_eq!(handles(&s, n), vec![moved]);
    assert_eq!(rows(&s), vec![(n, pts[4])]);
    loads(&s);
    let rep = s.solve();
    assert_eq!(rep.status, SolveStatus::Solved, "{rep:?}");
    // Der Griff wirkt am neuen Stück: Eine andere Spitze ändert dessen Form, nicht die des ersten.
    let (first, second) = (dense(&s, c), dense(&s, n));
    let tip = pos(&s, moved.tip);
    s.add_constraint(K::Fixed(moved.tip, tip.x + 1.0, tip.y + 2.0))
        .unwrap();
    assert_eq!(s.solve().status, SolveStatus::Solved);
    assert_ne!(dense(&s, n), second);
    assert_eq!(dense(&s, c), first);
}

#[test]
fn a_handled_fit_point_on_the_cut_loses_its_handle() {
    let mut s = sketch();
    let (c, pts) = handled_wave(&mut s);
    // Die Linie trifft den Stützpunkt mit Griff genau: Er fällt weg wie ohne Griff.
    let (x, y) = WAVE[4];
    line(&mut s, (x, y - 40.0), (x, y + 40.0));
    let gone = handle_at(&s, c, pts[4]);
    let out = s.break_curve(c, v(10.0, 6.0)).unwrap();
    assert_eq!(out.len(), 2);
    assert!(s.curve(gone.line).is_none());
    assert!(rows(&s).is_empty());
    loads(&s);
}

#[test]
fn trimming_a_handle_switches_it_off_and_breaking_it_does_nothing() {
    let mut s = sketch();
    let (c, pts) = handled_wave(&mut s);
    let h = handle_at(&s, c, pts[1]);
    // Eine fremde Linie quer durch den Griff zerteilt ihn nicht.
    let (a, b) = (pos(&s, pts[1]), pos(&s, h.tip));
    let mid = (a + b) * 0.5;
    let across = (b - a).perp().normalize() * 5.0;
    line(
        &mut s,
        (mid.x - across.x, mid.y - across.y),
        (mid.x + across.x, mid.y + across.y),
    );
    let before = s.to_data();
    assert_eq!(s.break_curve(h.line, mid).unwrap(), vec![h.line]);
    assert_eq!(s.to_data(), before);
    assert_eq!(s.trim(h.line, mid).unwrap(), Vec::<CurveId>::new());
    assert!(handles(&s, c).iter().all(|k| k.at != pts[1]));
    assert!(s.point_xy(h.tip).is_err());
    assert!(s.curve(c).is_some());
    loads(&s);
}

#[test]
fn a_foreign_line_is_not_cut_by_a_handle() {
    let mut s = sketch();
    let (c, pts) = handled_wave(&mut s);
    let h = handle_at(&s, c, pts[1]);
    let (a, b) = (pos(&s, pts[1]), pos(&s, h.tip));
    let mid = (a + b) * 0.5;
    // Kurz quer zum Griff trifft die Linie nur ihn, nicht den Spline: Stutzen löscht sie ganz.
    let n = (b - a).perp().normalize() * 0.01;
    assert!(s.distance_to_curve(c, mid).unwrap() > 0.05);
    let l = line(
        &mut s,
        (mid.x - n.x, mid.y - n.y),
        (mid.x + n.x, mid.y + n.y),
    );
    let piece = s.trim_piece(l, mid).unwrap();
    assert!(piece.whole, "der Griff schneidet nicht");
}

// ---- Dehnen -----------------------------------------------------------------------------------

#[test]
fn extend_handled_spline_still_refused() {
    let mut s = sketch();
    let (c, pts) = handled_wave(&mut s);
    s.activate_tangent_handle(c, pts[5]).unwrap();
    line(&mut s, (70.0, -40.0), (70.0, 40.0));
    let h = handle_at(&s, c, pts[5]);
    let before = s.to_data();
    assert!(!s.extend(c, v(50.0, -7.0)).unwrap());
    assert!(s.extend_preview(c, v(50.0, -7.0)).is_none());
    assert_eq!(s.to_data(), before);
    // Auch die Griff-Linie dehnt sich nicht.
    assert!(!s.extend(h.line, pos(&s, h.tip)).unwrap());
    assert_eq!(s.to_data(), before);
}

// ---- Kopieren und Skalieren -------------------------------------------------------------------

#[test]
fn copy_carries_handles() {
    let mut s = sketch();
    let (c, pts) = handled_wave(&mut s);
    let source = handles(&s, c);
    let held = s
        .add_constraint(K::Radius(
            arc_parts(&source[1]).0,
            radius(&s, arc_parts(&source[1]).0),
        ))
        .unwrap();
    let mut t = Similarity::identity(DVec2::ZERO);
    t.offset = v(100.0, 40.0);
    let set = s.transform_set(&[c], &[]);
    let map = s.copy_transformed(&set, &t).unwrap();
    let copy = map.curves[&c];

    assert_mapped(&s, c, copy, |p| p + t.offset, 1e-9);
    let copied = handles(&s, copy);
    assert_eq!(copied.len(), 2);
    for (from, to) in source.iter().zip(&copied) {
        assert_eq!(to.at, map.points[&from.at]);
        assert_eq!(to.tip, map.points[&from.tip]);
        assert_ne!(to.tip, from.tip);
        assert_eq!(to.line, map.curves[&from.line]);
        assert_ne!(to.line, from.line);
        assert!(s.curve(to.line).unwrap().construction);
    }
    let (arc, center, other) = arc_parts(&source[1]);
    assert_eq!(
        arc_parts(&copied[1]),
        (map.curves[&arc], map.points[&center], map.points[&other])
    );
    assert!(s.curve(map.curves[&arc]).unwrap().construction);
    assert_eq!(rows(&s), vec![(c, pts[4]), (copy, map.points[&pts[4]])]);
    // Abhängigkeiten am Griff kommen mit.
    assert!(map.constraints.contains_key(&held));
    assert!(s.is_handle_curve(map.curves[&arc]));
    loads(&s);
    let rep = s.solve();
    assert_eq!(rep.status, SolveStatus::Solved, "{rep:?}");
    assert!(rep.redundant.is_empty(), "{rep:?}");
    assert_mapped(&s, c, copy, |p| p + t.offset, 1e-7);

    // Die Griffe der Kopie sind ihre eigenen.
    let before = dense(&s, c);
    let tip = pos(&s, copied[0].tip);
    s.add_constraint(K::Fixed(copied[0].tip, tip.x, tip.y + 3.0))
        .unwrap();
    assert_eq!(s.solve().status, SolveStatus::Solved);
    assert_eq!(dense(&s, c), before);
}

#[test]
fn copy_of_a_handle_alone_takes_its_spline_along() {
    let mut s = sketch();
    let (c, pts) = handled_wave(&mut s);
    let h = handle_at(&s, c, pts[1]);
    let mut t = Similarity::identity(DVec2::ZERO);
    t.offset = v(0.0, 50.0);
    let set = s.transform_set(&[h.line], &[]);
    let map = s.copy_transformed(&set, &t).unwrap();
    assert!(map.curves.contains_key(&c));
    assert_eq!(handles(&s, map.curves[&c]).len(), 2);
    loads(&s);
}

#[test]
fn scale_scales_handles() {
    let base = v(5.0, 5.0);
    let t = Similarity {
        base,
        offset: v(3.0, -2.0),
        angle: 0.4,
        factor: 2.5,
    };
    // Bewegen und Kopieren bilden alle Griffpunkte gleich ab.
    let mut s = sketch();
    let (c, pts) = handled_wave(&mut s);
    let h = handle_at(&s, c, pts[4]);
    let (arc, center, other) = arc_parts(&h);
    let r = radius(&s, arc);
    let reference = s.clone();
    let set = s.transform_set(&[c], &[]);
    let map = s.copy_transformed(&set, &t).unwrap();
    for p in [h.tip, center, other, pts[4]] {
        let want = t.apply(pos(&s, p));
        assert!(pos(&s, map.points[&p]).distance(want) < 1e-9);
    }
    assert!((radius(&s, map.curves[&arc]) - r * 2.5).abs() < 1e-9);
    assert_mapped(&s, c, map.curves[&c], |p| t.apply(p), 1e-8);
    loads(&s);

    let mut moved = reference.clone();
    let set = moved.transform_set(&[c], &[]);
    moved.apply_transform(&set, &t).unwrap();
    for p in [h.tip, center, other, pts[4]] {
        let want = t.apply(pos(&reference, p));
        assert!(pos(&moved, p).distance(want) < 1e-9);
    }
    let (old, new) = (dense(&reference, c), dense(&moved, c));
    for (p, q) in old.into_iter().zip(new) {
        assert!(t.apply(p).distance(q) < 1e-8);
    }
    loads(&moved);
}

// ---- Spiegeln ---------------------------------------------------------------------------------

fn mirror_x(x0: f64) -> impl Fn(DVec2) -> DVec2 {
    move |p| v(2.0 * x0 - p.x, p.y)
}

#[test]
fn mirror_adds_symmetric_on_at_and_tip_and_equal_on_arcs() {
    let mut s = sketch();
    let (c, pts) = handled_wave(&mut s);
    let axis = line(&mut s, (-10.0, -30.0), (-10.0, 30.0));
    let source = handles(&s, c);
    let out = s.add_mirrored(&[c], axis).unwrap();
    assert_eq!(out.len(), 1, "je gewählter Kurve eine Kopie");
    let copy = out[0];

    let symmetric: Vec<(PointId, PointId)> = s
        .constraints()
        .filter_map(|(_, d)| match d.kind {
            K::Symmetric(p, q, x) if x == axis => Some((p, q)),
            _ => None,
        })
        .collect();
    let on = |p: PointId| symmetric.iter().find(|x| x.0 == p).map(|x| x.1);
    assert_eq!(
        symmetric.len(),
        pts.len() + 2,
        "Stützpunkte und zwei Spitzen"
    );
    let copied = handles(&s, copy);
    assert_eq!(copied.len(), 2);
    for (from, to) in source.iter().zip(&copied) {
        assert_eq!(on(from.at), Some(to.at));
        assert_eq!(on(from.tip), Some(to.tip));
    }
    // Mitte und freies Ende bleiben ohne Zeile, sonst wären sie überzählig.
    let (arc, center, other) = arc_parts(&source[1]);
    let (arc2, center2, other2) = arc_parts(&copied[1]);
    assert!(on(center).is_none());
    assert!(on(other).is_none());
    let equal: Vec<_> = s
        .constraints()
        .filter(|(_, d)| matches!(d.kind, K::Equal(..)))
        .map(|(_, d)| d.kind.clone())
        .collect();
    assert_eq!(equal, vec![K::Equal(arc, arc2)]);
    // Der gespiegelte Bogen läuft andersherum.
    let (
        CurveKind::Arc { start, end, .. },
        CurveKind::Arc {
            start: s2, end: e2, ..
        },
    ) = (&s.curve(arc).unwrap().kind, &s.curve(arc2).unwrap().kind)
    else {
        panic!("kein Bogen");
    };
    let mirrored_at = on(pts[4]).unwrap();
    assert!(*start == pts[4] || *end == pts[4]);
    assert_eq!(*start == pts[4], *e2 == mirrored_at, "{s2:?}");
    assert_eq!(*end == pts[4], *s2 == mirrored_at, "{e2:?}");
    let m = mirror_x(-10.0);
    assert!(pos(&s, center2).distance(m(pos(&s, center))) < 1e-9);
    assert!(pos(&s, other2).distance(m(pos(&s, other))) < 1e-9);
    assert_eq!(rows(&s), vec![(c, pts[4]), (copy, on(pts[4]).unwrap())]);
    loads(&s);

    let rep = s.solve();
    assert_eq!(rep.status, SolveStatus::Solved, "{rep:?}");
    assert!(rep.redundant.is_empty(), "{rep:?}");
    assert!(rep.conflicting.is_empty(), "{rep:?}");
    assert_mapped(&s, c, copy, &m, 1e-6);

    // Die Bindungen halten: Radius und Griffe folgen der Vorlage.
    // Die Achse steht fest, sonst dürfte der Löser auch sie kippen.
    let CurveKind::Line { a, b } = s.curve(axis).unwrap().kind else {
        panic!("keine Linie");
    };
    for p in [a, b] {
        let q = pos(&s, p);
        s.add_constraint(K::Fixed(p, q.x, q.y)).unwrap();
    }
    s.add_constraint(K::Radius(arc, radius(&s, arc) * 1.3))
        .unwrap();
    let tip = pos(&s, source[0].tip);
    s.add_constraint(K::Fixed(source[0].tip, tip.x + 1.0, tip.y - 1.0))
        .unwrap();
    let rep = s.solve();
    assert_eq!(rep.status, SolveStatus::Solved, "{rep:?}");
    assert!(rep.redundant.is_empty(), "{rep:?}");
    assert!((radius(&s, arc) - radius(&s, arc2)).abs() < 1e-6);
    assert_mapped(&s, c, copy, &m, 1e-5);
}

#[test]
fn mirror_with_a_handle_on_the_axis() {
    // Stützpunkt auf der Achse, Griff quer dazu: Der Stützpunkt wird mitbenutzt, Spitze gespiegelt,
    // die Mitte liegt auf der Achse und bekommt trotzdem einen eigenen Punkt.
    let mut s = sketch();
    let (c, pts) = spline(&mut s, &[(0.0, 0.0), (10.0, 6.0), (20.0, 0.0)]);
    s.activate_curvature_handle(c, pts[1]).unwrap();
    let axis = line(&mut s, (10.0, -30.0), (10.0, 30.0));
    let h = handle_at(&s, c, pts[1]);
    let (arc, center, _) = arc_parts(&h);
    let copy = s.add_mirrored(&[c], axis).unwrap()[0];
    let m = handle_at(&s, copy, pts[1]);
    assert_ne!(m.tip, h.tip);
    let (arc2, center2, _) = arc_parts(&m);
    assert_ne!(arc2, arc);
    assert_ne!(center2, center);
    loads(&s);
    let rep = s.solve();
    assert_eq!(rep.status, SolveStatus::Solved, "{rep:?}");
    assert!(rep.redundant.is_empty(), "{rep:?}");
    assert!(rep.conflicting.is_empty(), "{rep:?}");
    assert_mapped(&s, c, copy, mirror_x(10.0), 1e-6);
}

#[test]
fn mirror_with_a_handle_along_the_axis_shares_the_tip() {
    let mut s = sketch();
    let (c, pts) = spline(&mut s, &[(4.0, -10.0), (10.0, 0.0), (4.0, 10.0)]);
    s.activate_tangent_handle(c, pts[1]).unwrap();
    let axis = line(&mut s, (10.0, -30.0), (10.0, 30.0));
    let h = handle_at(&s, c, pts[1]);
    assert!((pos(&s, h.tip).x - 10.0).abs() < 1e-9);
    let copy = s.add_mirrored(&[c], axis).unwrap()[0];
    let m = handle_at(&s, copy, pts[1]);
    assert_eq!(m.tip, h.tip);
    assert_ne!(m.line, h.line);
    loads(&s);
    let rep = s.solve();
    assert_eq!(rep.status, SolveStatus::Solved, "{rep:?}");
    assert!(rep.redundant.is_empty(), "{rep:?}");
    assert_mapped(&s, c, copy, mirror_x(10.0), 1e-6);
    // Griff der Vorlage löschen: Die Spitze bleibt für die Kopie stehen.
    s.deactivate_handle(c, pts[1]).unwrap();
    assert!(s.point_xy(m.tip).is_ok());
    loads(&s);
}

#[test]
fn mirror_refuses_its_own_handle_as_axis_and_takes_the_family_of_a_handle() {
    let mut s = sketch();
    let (c, pts) = handled_wave(&mut s);
    let h = handle_at(&s, c, pts[1]);
    let before = s.to_data();
    assert!(s.add_mirrored(&[c], h.line).is_err());
    assert_eq!(s.to_data(), before);
    let axis = line(&mut s, (-10.0, -30.0), (-10.0, 30.0));
    // Nur die Griff-Linie gewählt: Der Spline kommt mit, geliefert wird die gewählte Kurve.
    let out = s.add_mirrored(&[h.line], axis).unwrap();
    assert_eq!(out.len(), 1);
    assert!(s.is_handle_curve(out[0]));
    loads(&s);
    assert!(s.solve().redundant.is_empty());
}

// ---- Lineares Muster aus `add_linear_pattern` -------------------------------------------------

#[test]
fn linear_pattern_copies_handles_without_extra_bindings() {
    let mut s = sketch();
    let (c, pts) = handled_wave(&mut s);
    let groups = s.add_linear_pattern(&[c], v(0.0, 30.0), 2).unwrap();
    assert_eq!(groups.len(), 2);
    for (k, group) in groups.iter().enumerate() {
        assert_eq!(group.len(), 1, "je gewählter Kurve eine Kopie");
        let copy = group[0];
        assert_eq!(handles(&s, copy).len(), 2);
        assert_mapped(&s, c, copy, |p| p + v(0.0, 30.0 * (k + 1) as f64), 1e-6);
    }
    assert_eq!(rows(&s).len(), 3);
    assert!(rows(&s).contains(&(c, pts[4])));
    loads(&s);
    let rep = s.solve();
    assert_eq!(rep.status, SolveStatus::Solved, "{rep:?}");
    assert!(rep.redundant.is_empty(), "{rep:?}");
}

// ---- Muster -----------------------------------------------------------------------------------

/// Das Exemplar an der Lage `at`: Kopie des Splines und Punkt zu `p`.
fn instance_spline(s: &Sketch, id: PatternId, c: CurveId, at: [i32; 2]) -> CurveId {
    let d = s.pattern(id).unwrap();
    let k = d.sources.iter().position(|&x| x == c).unwrap();
    d.instances.iter().find(|i| i.at == at).unwrap().curves[k]
}

fn instance_point(s: &Sketch, id: PatternId, p: PointId, at: [i32; 2]) -> PointId {
    let d = s.pattern(id).unwrap();
    let k = d.source_points.iter().position(|&x| x == p).unwrap();
    d.instances.iter().find(|i| i.at == at).unwrap().points[k]
}

#[test]
fn pattern_instance_follows_source_handle() {
    let mut s = sketch();
    let (c, pts) = handled_wave(&mut s);
    let id = s.add_pattern(&rect_plan(vec![c], 3, 70.0)).unwrap();
    // Griff-Kurven gehören zum Exemplar, sind aber keine Vorlagen, die der Nutzer sieht.
    assert_eq!(s.pattern_plan(id).unwrap().sources, vec![c]);

    let source = handles(&s, c);
    for at in [[1, 0], [2, 0]] {
        let copy = instance_spline(&s, id, c, at);
        let copied = handles(&s, copy);
        assert_eq!(copied.len(), 2);
        assert_ne!(copied[0].line, source[0].line);
        assert_ne!(copied[0].tip, source[0].tip);
        assert_eq!(copied[0].tip, instance_point(&s, id, source[0].tip, at));
        assert_eq!(copied[1].at, instance_point(&s, id, pts[4], at));
        let (arc, center, other) = arc_parts(&source[1]);
        let (arc2, center2, other2) = arc_parts(&copied[1]);
        assert_eq!(arc2, instance_spline(&s, id, arc, at));
        assert_eq!(center2, instance_point(&s, id, center, at));
        assert_eq!(other2, instance_point(&s, id, other, at));
        assert!(rows(&s).contains(&(copy, copied[1].at)));
        let shift = v(70.0 * f64::from(at[0]), 0.0);
        assert_mapped(&s, c, copy, |p| p + shift, 1e-9);
    }
    assert_eq!(rows(&s).len(), 3);
    loads(&s);
    let rep = s.solve();
    assert_eq!(rep.status, SolveStatus::Solved, "{rep:?}");
    assert!(rep.redundant.is_empty(), "{rep:?}");

    // Die Spitze der Vorlage wandert, das Exemplar folgt.
    let tip = pos(&s, source[0].tip);
    s.add_constraint(K::Fixed(source[0].tip, tip.x + 1.0, tip.y + 4.0))
        .unwrap();
    let rep = s.solve();
    assert_eq!(rep.status, SolveStatus::Solved, "{rep:?}");
    let copy = instance_spline(&s, id, c, [2, 0]);
    let moved = pos(
        &s,
        handle_at(&s, copy, instance_point(&s, id, pts[1], [2, 0])).tip,
    );
    assert!(
        moved.distance(v(tip.x + 141.0, tip.y + 4.0)) < 1e-9,
        "{moved}"
    );
    assert_mapped(&s, c, copy, |p| p + v(140.0, 0.0), 1e-9);
    loads(&s);
}

#[test]
fn circular_pattern_turns_handles_with_the_instance() {
    let mut s = sketch();
    let (c, _) = handled_wave(&mut s);
    let center = s.add_point(25.0, -40.0).unwrap();
    let plan = PatternPlan {
        kind: PatternKind::Circular {
            center,
            mode: CircularMode::Full,
        },
        sources: vec![c],
        counts: [4, 1],
        values: [0.0, 0.0],
        suppressed: vec![],
    };
    let id = s.add_pattern(&plan).unwrap();
    let copy = instance_spline(&s, id, c, [1, 0]);
    let turn = |p: DVec2| {
        let m = v(25.0, -40.0);
        let r = p - m;
        m + v(-r.y, r.x)
    };
    assert_mapped(&s, c, copy, turn, 1e-9);
    loads(&s);
    let rep = s.solve();
    assert_eq!(rep.status, SolveStatus::Solved, "{rep:?}");
    assert!(rep.redundant.is_empty(), "{rep:?}");
    assert_mapped(&s, c, copy, turn, 1e-9);
}

#[test]
fn instance_handles_are_no_sources_and_a_source_handle_means_its_spline() {
    let mut s = sketch();
    let (c, pts) = handled_wave(&mut s);
    let h = handle_at(&s, c, pts[1]);
    // Nur die Griff-Linie gewählt: Das Muster nimmt den Spline.
    let id = s.add_pattern(&rect_plan(vec![h.line], 2, 70.0)).unwrap();
    let plan = s.pattern_plan(id).unwrap();
    assert_eq!(plan.sources, vec![c]);
    let copy = instance_spline(&s, id, c, [1, 0]);
    let line = handles(&s, copy)[0].line;
    assert!(s.add_pattern(&rect_plan(vec![line], 2, 70.0)).is_err());
    assert!(s.add_pattern(&rect_plan(vec![copy], 2, 70.0)).is_err());
    // Der Plan aus dem Muster gibt beim Bearbeiten keine neuen Exemplare.
    let edit = s.edit_pattern(id, &plan).unwrap();
    assert_eq!(edit.added + edit.removed, 0);
    let keep = instance_spline(&s, id, c, [1, 0]);
    assert_eq!(keep, copy);
    // Vorschau: nur die Form, keine Griffe.
    let preview = s.pattern_preview(&rect_plan(vec![c], 2, 70.0), 64).unwrap();
    assert_eq!(preview.len(), 1);
    assert_eq!(preview[0].1.len(), 1);
}

#[test]
fn editing_and_suppressing_a_pattern_keeps_handles_consistent() {
    let mut s = sketch();
    let (c, _) = handled_wave(&mut s);
    let id = s.add_pattern(&rect_plan(vec![c], 3, 70.0)).unwrap();
    s.edit_pattern(id, &rect_plan(vec![c], 4, 70.0)).unwrap();
    assert_eq!(rows(&s).len(), 4);
    loads(&s);
    s.edit_pattern(id, &rect_plan(vec![c], 2, 70.0)).unwrap();
    assert_eq!(rows(&s).len(), 2);
    loads(&s);
    s.suppress_instance(id, [1, 0], true).unwrap();
    assert_eq!(rows(&s).len(), 1);
    loads(&s);
    s.suppress_instance(id, [1, 0], false).unwrap();
    assert_eq!(rows(&s).len(), 2);
    let copy = instance_spline(&s, id, c, [1, 0]);
    assert_eq!(handles(&s, copy).len(), 2);
    loads(&s);
    assert_eq!(s.solve().status, SolveStatus::Solved);
}

#[test]
fn changing_handles_dissolves_the_pattern_and_stays_loadable() {
    // An der Vorlage und am Exemplar: Das Muster löst sich auf, die Geometrie bleibt ladbar.
    for on_instance in [false, true] {
        let mut s = sketch();
        let (c, pts) = handled_wave(&mut s);
        let id = s.add_pattern(&rect_plan(vec![c], 2, 70.0)).unwrap();
        let copy = instance_spline(&s, id, c, [1, 0]);
        let at = instance_point(&s, id, pts[4], [1, 0]);
        if on_instance {
            s.deactivate_handle(copy, at).unwrap();
            assert!(handles(&s, copy).iter().all(|h| h.at != at));
            assert_eq!(handles(&s, c).len(), 2);
        } else {
            s.deactivate_handle(c, pts[1]).unwrap();
            assert_eq!(handles(&s, copy).len(), 2);
        }
        assert!(s.pattern(id).is_none());
        loads(&s);
        assert_eq!(s.solve().status, SolveStatus::Solved);
    }
    // Stutzen löst das Muster ebenso auf.
    let mut s = sketch();
    let (c, _) = handled_wave(&mut s);
    let id = s.add_pattern(&rect_plan(vec![c], 2, 70.0)).unwrap();
    let copy = instance_spline(&s, id, c, [1, 0]);
    line(&mut s, (95.5, -40.0), (95.5, 40.0));
    s.trim(copy, v(115.0, 3.0)).unwrap();
    assert!(s.pattern(id).is_none());
    assert_eq!(handles(&s, copy).len(), 1);
    loads(&s);
}

// ---- Fang -------------------------------------------------------------------------------------

#[test]
fn hidden_handle_does_not_snap() {
    let mut s = sketch();
    let (c, pts) = handled_wave(&mut s);
    let h = handle_at(&s, c, pts[1]);
    let (_, center, other) = arc_parts(&handle_at(&s, c, pts[4]));
    let index = SnapIndex::build(&s);
    let tip = pos(&s, h.tip);
    let near = tip + v(0.01, 0.01);

    // Unsichtbar: kein Punkt, auch kein Griff-Linie als Kurve oder ihre Mitte.
    let hidden = index.find(near, 0.05, &[]);
    assert!(hidden.is_none(), "{hidden:?}");
    let mid = (pos(&s, h.at) + tip) * 0.5;
    assert!(index.find(mid, 0.05, &[]).is_none());
    // Das freie Ende liegt dicht am Spline: Es darf die Kurve fangen, aber nicht sich selbst.
    for p in [center, other] {
        let hit = index.find(pos(&s, p), 0.05, &[]);
        assert!(
            hit.is_none_or(|h| matches!(h.snap, Snap::Curve(k) if k == c)),
            "{hit:?}"
        );
    }
    let al = index.align(v(tip.x + 0.01, 500.0), 0.05, &[], &[]);
    assert!(al.x.is_none(), "{al:?}");

    // Sichtbar: die Spitze als Punkt; ihre Linie bleibt draußen.
    let hit = index.find(near, 0.05, &[h.tip]).unwrap();
    assert_eq!(hit.snap, Snap::Point(h.tip));
    assert_eq!(hit.at, tip);
    assert!(index.find(mid, 0.05, &[h.tip]).is_none());
    let al = index.align(v(tip.x + 0.01, 500.0), 0.05, &[], &[h.tip]);
    assert_eq!(al.x, Some((h.tip, tip.x)));

    // Der Stützpunkt und die Kurve fangen weiter.
    let on = pos(&s, pts[1]) + v(0.01, 0.01);
    assert_eq!(index.find(on, 0.05, &[]).unwrap().snap, Snap::Point(pts[1]));
}

#[test]
fn a_handle_point_another_curve_uses_snaps_and_hidden_points_do_not_block_the_origin() {
    let mut s = sketch();
    let (c, pts) = handled_wave(&mut s);
    let h = handle_at(&s, c, pts[1]);
    let q = s.add_point(90.0, 90.0).unwrap();
    s.add_line(h.tip, q).unwrap();
    let index = SnapIndex::build(&s);
    let hit = index
        .find(pos(&s, h.tip) + v(0.01, 0.0), 0.05, &[])
        .unwrap();
    assert_eq!(hit.snap, Snap::Point(h.tip));

    // Eine versteckte Spitze im Ursprung lässt den Ursprung frei.
    let mut s = sketch();
    let (c, pts) = spline(&mut s, &[(-3.0, 0.0), (-3.0, 3.0), (3.0, 6.0)]);
    s.activate_tangent_handle(c, pts[0]).unwrap();
    let tip = handle_at(&s, c, pts[0]).tip;
    let d = DVec2::ZERO - pos(&s, tip);
    let mut t = Similarity::identity(DVec2::ZERO);
    t.offset = d;
    let set = s.transform_set(&[c], &[]);
    s.apply_transform(&set, &t).unwrap();
    assert!(pos(&s, tip).length() < 1e-9);
    let hit = SnapIndex::build(&s).find(v(0.01, 0.0), 0.05, &[]).unwrap();
    assert_eq!(hit.snap, Snap::Origin);
}

#[test]
fn sketches_without_handles_snap_as_before_with_an_empty_list() {
    let mut s = sketch();
    let (c, pts) = spline(&mut s, &WAVE);
    let index = SnapIndex::build(&s);
    let hit = index.find(v(10.01, 6.0), 0.05, &[]).unwrap();
    assert_eq!(hit.snap, Snap::Point(pts[1]));
    let on = s.curve_eval(c, 2.4).unwrap();
    assert_eq!(
        index.find(on + v(0.001, 0.0), 0.02, &[]).unwrap().snap,
        Snap::Curve(c)
    );
}

// ---- Versatz und Abrunden ---------------------------------------------------------------------

#[test]
fn offset_of_handled_spline() {
    let mut s = sketch();
    let (c, pts) = spline(&mut s, &WAVE);
    let natural = s.clone();
    s.activate_tangent_handle(c, pts[3]).unwrap();
    let h = handle_at(&s, c, pts[3]);
    // Der Griff dreht die Kurve spürbar weg vom natürlichen Spline.
    let tip = pos(&s, pts[3]) + v(2.0, 9.0);
    s.add_constraint(K::Fixed(h.tip, tip.x, tip.y)).unwrap();
    assert_eq!(s.solve().status, SolveStatus::Solved);
    let before = s.to_data();

    let chains = s.ordered_chains(&[c]).unwrap();
    let d = 0.5;
    let plan = s.offset_plan(&chains, d).unwrap();
    let ids = s.add_offset(&plan).unwrap();
    assert_eq!(ids.curves.len(), 1);
    let copy = ids.curves[0];
    assert!(
        handles(&s, copy).is_empty(),
        "das Ergebnis ist ein Spline ohne Griffe"
    );
    // Abstand zur wahren Form, nicht zum natürlichen Kubik.
    let mut far_from_natural: f64 = 0.0;
    for q in dense(&s, copy) {
        let to_true = s.distance_to_curve(c, q).unwrap();
        assert!((to_true - d).abs() < 0.05, "{q}: {to_true}");
        far_from_natural = far_from_natural.max(natural.distance_to_curve(c, q).unwrap());
    }
    assert!(far_from_natural > 1.0, "{far_from_natural}");
    // Die Vorlage bleibt samt Griff.
    assert_eq!(
        handles(&s, c),
        handles(&Sketch::from_data(before).unwrap(), c)
    );
    loads(&s);
}

#[test]
fn offset_chain_ignores_handle_parts_among_the_picks() {
    let mut s = sketch();
    let (c, pts) = handled_wave(&mut s);
    let h = handle_at(&s, c, pts[1]);
    let (arc, ..) = arc_parts(&handle_at(&s, c, pts[4]));
    let alone = s.ordered_chains(&[c]).unwrap();
    let all = s.ordered_chains(&[c, h.line, arc]).unwrap();
    assert_eq!(all, alone);
}

#[test]
fn fillet_beside_a_handled_spline_leaves_handles_alone() {
    let mut s = sketch();
    let (c, pts) = handled_wave(&mut s);
    let l = line(&mut s, (15.0, 30.0), (15.0, -30.0));
    let on = s.curve_eval(c, 2.0).unwrap();
    let plan = s
        .fillet_plan(
            CurvePick {
                curve: l,
                at: v(15.0, 20.0),
            },
            CurvePick { curve: c, at: on },
            1.0,
        )
        .expect("Plan");
    let before = handles(&s, c);
    let shape = dense(&s, c);
    s.add_fillet(&plan, None).unwrap();
    assert_eq!(handles(&s, c), before);
    assert_eq!(dense(&s, c), shape);
    assert_eq!(rows(&s), vec![(c, pts[4])]);
    loads(&s);
}
