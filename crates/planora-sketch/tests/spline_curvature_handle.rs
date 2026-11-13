//! Krümmungsgriff am Spline durch Stützpunkte (Skizzen 7, Task 7; Spec B2, B4, B5, B8, B9).

use planora_math::{DVec2, Frame, Tolerance};
use planora_sketch::constraint::ConstraintKind as K;
use planora_sketch::model::{CurveId, CurveKind, PointId, Sketch};
use planora_sketch::solver::{Drag, SolveStatus};
use planora_sketch::transform::Similarity;
use planora_sketch::{HandleCurvature, SketchError};

const MAX_OPENING: f64 = 2.5;

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

fn spline(s: &mut Sketch, xy: &[(f64, f64)]) -> (CurveId, Vec<PointId>) {
    let pts: Vec<PointId> = xy
        .iter()
        .map(|&(x, y)| s.add_point(x, y).unwrap())
        .collect();
    (s.add_spline(&pts).unwrap(), pts)
}

/// Die Teile eines Griffs am Stützpunkt `at`.
struct Parts {
    line: CurveId,
    tip: PointId,
    arc: CurveId,
    center: PointId,
    other: PointId,
}

fn parts(s: &Sketch, c: CurveId, at: PointId) -> Parts {
    let CurveKind::Spline { handles, .. } = &s.curve(c).unwrap().kind else {
        panic!("kein Spline");
    };
    let h = handles.iter().find(|h| h.at == at).expect("kein Griff");
    let HandleCurvature::Arc { arc, center, other } = h.curvature else {
        panic!("kein Krümmungsgriff");
    };
    Parts {
        line: h.line,
        tip: h.tip,
        arc,
        center,
        other,
    }
}

fn pos(s: &Sketch, p: PointId) -> DVec2 {
    s.point_xy(p).unwrap()
}

fn dense(s: &Sketch, c: CurveId, per_segment: usize) -> Vec<DVec2> {
    let (lo, hi) = s.curve_domain(c).unwrap();
    let steps = (hi - lo) as usize * per_segment;
    (0..=steps)
        .map(|k| {
            s.curve_eval(c, lo + (hi - lo) * k as f64 / steps as f64)
                .unwrap()
        })
        .collect()
}

fn same_shape(a: &[DVec2], b: &[DVec2], limit: f64) {
    assert_eq!(a.len(), b.len());
    for (i, (p, q)) in a.iter().zip(b).enumerate() {
        assert!(p.distance(*q) <= limit, "Stelle {i}: {p} / {q}");
    }
}

/// Schrittweite der einseitigen Formeln. Sie sind für Grad 5 exakt, darum darf sie groß sein.
const STEP: f64 = 1e-2;

fn six(s: &Sketch, c: CurveId, t: f64, side: f64) -> [DVec2; 6] {
    std::array::from_fn(|k| s.curve_eval(c, t + side * k as f64 * STEP).unwrap())
}

/// Einseitige Krümmung bei `t`, gemessen nur über die Lage. Die Richtung ist immer `t` aufwärts.
fn kappa_side(s: &Sketch, c: CurveId, t: f64, side: f64) -> f64 {
    let f = six(s, c, t, side);
    let d = (f[0] * (-137.0 / 60.0) + f[1] * 5.0 - f[2] * 5.0 + f[3] * (10.0 / 3.0) - f[4] * 1.25
        + f[5] * 0.2)
        / STEP
        * side;
    let a = (f[0] * (15.0 / 4.0) - f[1] * (77.0 / 6.0) + f[2] * (107.0 / 6.0) - f[3] * 13.0
        + f[4] * (61.0 / 12.0)
        - f[5] * (5.0 / 6.0))
        / (STEP * STEP);
    d.perp_dot(a) / d.length().powi(3)
}

/// Prüft die beiden Zeilen aus B2 und den Radiusbezug an den aktuellen Werten. `limit` gilt für die
/// Winkel in Radiant und für den Abstand des Bogenendes vom Kreis, relativ zum Radius.
fn assert_rows_hold(s: &Sketch, at: PointId, h: &Parts, limit: f64) {
    let (a, tip, m, o) = (pos(s, at), pos(s, h.tip), pos(s, h.center), pos(s, h.other));
    let f = tip - a;
    let (l, r) = (f.length(), (m - a).length());
    assert!(
        (m - a).dot(f).abs() / (l * r) <= limit,
        "Mitte nicht auf der Normalen"
    );
    assert!(
        ((o - m).length() - r).abs() <= limit * r.max(1.0),
        "Ende nicht auf dem Kreis"
    );
    let open = (a - m).perp_dot(o - m).atan2((a - m).dot(o - m)).abs();
    let want = (l / r).min(MAX_OPENING);
    assert!((open - want).abs() <= limit, "Öffnung {open}, soll {want}");
    // Das freie Ende liegt vor dem Griff, die Mitte auf der Seite des Bogens.
    let CurveKind::Arc { start, .. } = s.curve(h.arc).unwrap().kind else {
        panic!("kein Bogen");
    };
    let left = f.perp_dot(m - a) > 0.0;
    assert_eq!(left, start == at, "Seite passt nicht zum Bogen");
    assert!((o - a).dot(f) > 0.0, "Bogenende hinter dem Griff");
}

#[test]
fn activating_curvature_keeps_the_shape() {
    // Ohne Griff, mit Griff und an einem zweiten Punkt: Die Form bleibt bis auf Rechenrauschen.
    let mut s = sketch();
    let (c, pts) = spline(&mut s, &WAVE);
    let before = dense(&s, c, 40);
    s.activate_curvature_handle(c, pts[2]).unwrap();
    same_shape(&dense(&s, c, 40), &before, 1e-9);
    assert_eq!(s.solve().status, SolveStatus::Solved);
    same_shape(&dense(&s, c, 40), &before, 1e-9);
    s.activate_tangent_handle(c, pts[3]).unwrap();
    s.activate_curvature_handle(c, pts[3]).unwrap();
    same_shape(&dense(&s, c, 40), &before, 1e-9);
    assert_eq!(s.solve().status, SolveStatus::Solved);
    same_shape(&dense(&s, c, 40), &before, 1e-9);

    // Das Beispiel aus der Spec: Hier hat `M̄` einen Tangentialanteil von 1,5. Würde der Griff die
    // ganze zweite Ableitung vorgeben, wiche die Form ab.
    let mut s = sketch();
    let (c, pts) = spline(&mut s, &[(0.0, 0.0), (10.0, 0.0), (0.0, 1.0)]);
    let before = dense(&s, c, 200);
    s.activate_curvature_handle(c, pts[1]).unwrap();
    same_shape(&dense(&s, c, 200), &before, 1e-9);
    assert_eq!(s.solve().status, SolveStatus::Solved);
    same_shape(&dense(&s, c, 200), &before, 1e-9);

    // Auch am Spline-Ende, dessen Griff gedreht wurde, die Krümmung also nicht null ist.
    let mut s = sketch();
    let (c, pts) = spline(&mut s, &WAVE);
    s.activate_tangent_handle(c, pts[0]).unwrap();
    let CurveKind::Spline { handles, .. } = &s.curve(c).unwrap().kind else {
        panic!();
    };
    let tip = handles[0].tip;
    s.set_point_xy(tip, 0.0, 9.0).unwrap();
    let before = dense(&s, c, 40);
    s.activate_curvature_handle(c, pts[0]).unwrap();
    same_shape(&dense(&s, c, 40), &before, 1e-9);
}

#[test]
fn activating_sets_the_arc_from_the_current_curvature() {
    let mut s = sketch();
    let (c, pts) = spline(&mut s, &WAVE);
    let want = kappa_side(&s, c, 2.0, 1.0);
    let arc = s.activate_curvature_handle(c, pts[2]).unwrap();
    let h = parts(&s, c, pts[2]);
    assert_eq!(arc, h.arc);
    assert!(s.curve(arc).unwrap().construction);
    let r = pos(&s, h.center).distance(pos(&s, pts[2]));
    assert!(
        (r - 1.0 / want.abs()).abs() < 1e-9 * r,
        "r = {r}, κ = {want}"
    );
    assert_rows_hold(&s, pts[2], &h, 1e-9);
}

#[test]
fn activation_is_one_atomic_step() {
    let mut s = sketch();
    let (c, pts) = spline(&mut s, &WAVE);
    // Der Tangentengriff kommt mit.
    s.activate_curvature_handle(c, pts[2]).unwrap();
    let h = parts(&s, c, pts[2]);
    assert!(s.curve(h.line).unwrap().construction);

    // Abgelehnt: Die Skizze bleibt, wie sie ist.
    let same = |s: &mut Sketch, r: Result<CurveId, SketchError>| {
        assert!(r.is_err());
        s.to_data()
    };
    let before = s.to_data();
    let r = s.activate_curvature_handle(c, pts[2]); // schon an
    assert_eq!(same(&mut s, r), before);
    let r = s.activate_curvature_handle(c, h.tip); // kein Stützpunkt
    assert_eq!(same(&mut s, r), before);
    let r = s.activate_curvature_handle(h.line, pts[2]); // kein Spline
    assert_eq!(same(&mut s, r), before);

    // Der Spline steht still: Auch der Tangentengriff bleibt aus.
    let mut s = sketch();
    let (c, pts) = spline(&mut s, &[(0.0, 0.0), (10.0, 0.0), (1e-6, 0.0)]);
    let before = s.to_data();
    let r = s.activate_curvature_handle(c, pts[1]);
    assert!(matches!(r, Err(SketchError::Degenerate(_))), "{r:?}");
    assert_eq!(s.to_data(), before);
}

#[test]
fn radius_sets_the_curvature() {
    let mut s = sketch();
    let (c, pts) = spline(&mut s, &WAVE);
    let before = dense(&s, c, 10);
    s.activate_curvature_handle(c, pts[2]).unwrap();
    let h = parts(&s, c, pts[2]);
    let k0 = kappa_side(&s, c, 2.0, 1.0);
    let side = k0.signum();
    let a = pos(&s, pts[2]);
    let dir = (pos(&s, h.center) - a).normalize();
    for r in [1.5, 4.0, 11.0, 37.0, 150.0] {
        // Zug an der Mitte auf der Normalen.
        let target = a + dir * r;
        let rep = s
            .drag(&[Drag {
                point: h.center,
                target: target.extend(0.0),
                plane: None,
            }])
            .unwrap();
        assert!(!rep.drag_failed);
        let now = pos(&s, h.center).distance(pos(&s, pts[2]));
        let k = kappa_side(&s, c, 2.0, 1.0);
        assert!((k - side / now).abs() < 1e-6, "r = {now}, κ = {k}");
        assert_rows_hold(&s, pts[2], &h, 1e-6);
    }
    // Die Form hat sich wirklich geändert.
    assert!(
        dense(&s, c, 10)
            .iter()
            .zip(&before)
            .any(|(p, q)| p.distance(*q) > 1e-2)
    );
}

#[test]
fn radius_dimension_drives_it() {
    for fix in [false, true] {
        let mut s = sketch();
        let (c, pts) = spline(&mut s, &WAVE);
        s.activate_curvature_handle(c, pts[2]).unwrap();
        let h = parts(&s, c, pts[2]);
        let side = kappa_side(&s, c, 2.0, 1.0).signum();
        if fix {
            for &p in pts.iter().chain([&h.tip]) {
                let q = pos(&s, p);
                s.add_constraint(K::Fixed(p, q.x, q.y)).unwrap();
            }
        }
        let dim = s.add_constraint(K::Radius(h.arc, 7.0)).unwrap();
        assert_eq!(s.solve().status, SolveStatus::Solved);
        for r in [7.0, 3.0, 19.0, 60.0, 2.0] {
            s.set_constraint_value(dim, r).unwrap();
            assert_eq!(s.solve().status, SolveStatus::Solved, "r = {r}");
            assert!((pos(&s, h.center).distance(pos(&s, pts[2])) - r).abs() < 1e-7);
            let k = kappa_side(&s, c, 2.0, 1.0);
            assert!((k - side / r).abs() < 1e-6, "r = {r}, κ = {k}");
            assert_rows_hold(&s, pts[2], &h, 1e-6);
        }
    }
}

#[test]
fn small_radius_stays_solvable() {
    let mut s = sketch();
    let (c, pts) = spline(&mut s, &WAVE);
    s.activate_curvature_handle(c, pts[2]).unwrap();
    let h = parts(&s, c, pts[2]);
    // Griff halten: `L` bleibt, die Öffnung hängt dann nur am Radius.
    for p in [pts[2], h.tip] {
        let q = pos(&s, p);
        s.add_constraint(K::Fixed(p, q.x, q.y)).unwrap();
    }
    let dim = s.add_constraint(K::Radius(h.arc, 40.0)).unwrap();
    assert_eq!(s.solve().status, SolveStatus::Solved);
    let l = pos(&s, h.tip).distance(pos(&s, pts[2]));
    for f in [0.39, 0.3, 0.2, 0.1, 0.05] {
        let r = f * l;
        s.set_constraint_value(dim, r).unwrap();
        assert_eq!(s.solve().status, SolveStatus::Solved, "r = {r}");
        assert_rows_hold(&s, pts[2], &h, 1e-6);
        let (m, o, a) = (pos(&s, h.center), pos(&s, h.other), pos(&s, pts[2]));
        let open = (a - m).perp_dot(o - m).atan2((a - m).dot(o - m)).abs();
        assert!((open - MAX_OPENING).abs() < 1e-6, "Öffnung {open}");
    }
}

#[test]
fn opening_follows_the_rule_through_the_switch() {
    // Das Radiusmaß fährt in feinen Schritten über die Stelle L / r = 2,5 und zurück.
    let mut s = sketch();
    let (c, pts) = spline(&mut s, &WAVE);
    s.activate_curvature_handle(c, pts[2]).unwrap();
    let h = parts(&s, c, pts[2]);
    let q = pos(&s, h.tip);
    s.add_constraint(K::Fixed(h.tip, q.x, q.y)).unwrap();
    let l = q.distance(pos(&s, pts[2]));
    let switch = l / MAX_OPENING;
    let dim = s.add_constraint(K::Radius(h.arc, 1.6 * switch)).unwrap();
    assert_eq!(s.solve().status, SolveStatus::Solved);
    let side = kappa_side(&s, c, 2.0, 1.0).signum();
    let down = (0..=120).map(|k| switch * (1.6 - 1.2 * k as f64 / 120.0));
    let up = (0..=120).map(|k| switch * (0.4 + 1.2 * k as f64 / 120.0));
    for r in down.chain(up).chain([switch]) {
        s.set_constraint_value(dim, r).unwrap();
        assert_eq!(s.solve().status, SolveStatus::Solved, "r = {r}");
        assert_rows_hold(&s, pts[2], &h, 1e-6);
        let k = kappa_side(&s, c, 2.0, 1.0);
        assert!((k - side / r).abs() < 1e-6, "r = {r}, κ = {k}");
    }
}

#[test]
fn dragging_the_free_end_passes_the_switch() {
    // Der Zug am freien Bogenende ändert den Radius; auch über den Knick von `min` hinweg, und auch
    // dann, wenn die nächste erreichbare Lage genau auf dem Knick liegt. In mehreren Maßstäben.
    for scale in [0.05, 1.0, 40.0] {
        drag_through_the_switch(scale);
    }
}

#[test]
fn the_switch_also_holds_when_the_handle_moves() {
    // Griff frei: Das Ziehen der Spitze verschiebt den Knick, der Bogen bleibt auf ihm stehen.
    let mut s = sketch();
    let (c, pts) = spline(&mut s, &WAVE);
    s.activate_curvature_handle(c, pts[2]).unwrap();
    let h = parts(&s, c, pts[2]);
    let a = pos(&s, pts[2]);
    s.add_constraint(K::Fixed(pts[2], a.x, a.y)).unwrap();
    let dim = s.add_constraint(K::Radius(h.arc, 4.0)).unwrap();
    assert_eq!(s.solve().status, SolveStatus::Solved);
    let f = (pos(&s, h.tip) - a).normalize();
    // Der Griff wächst und schrumpft über L = 2,5·r = 10.
    let mut lengths = Vec::new();
    for k in 0..=400 {
        let l = 10.0 + 6.0 * (k as f64 * 0.04).sin();
        let target = a + f * l;
        let rep = s
            .drag(&[Drag {
                point: h.tip,
                target: target.extend(0.0),
                plane: None,
            }])
            .unwrap();
        assert!(!rep.drag_failed, "L = {l}: Ziehen gescheitert");
        assert_rows_hold(&s, pts[2], &h, 1e-6);
        lengths.push(pos(&s, h.tip).distance(a));
    }
    // Das Maß hat gehalten, und der Griff hat beide Seiten des Knicks gesehen.
    assert_eq!(s.constraint(dim).unwrap().kind, K::Radius(h.arc, 4.0));
    assert!((pos(&s, h.center).distance(a) - 4.0).abs() < 1e-6);
    assert!(lengths.iter().any(|&l| l > 11.0), "{lengths:?}");
    assert!(lengths.iter().any(|&l| l < 9.0), "{lengths:?}");
    // Und ohne jedes Halten: Das freie Ende zieht alles mit.
    let mut s = sketch();
    let (c, pts) = spline(&mut s, &WAVE);
    s.activate_curvature_handle(c, pts[2]).unwrap();
    let h = parts(&s, c, pts[2]);
    for k in 0..=400 {
        let w = k as f64 * 0.05;
        let o = pos(&s, h.other);
        let m = pos(&s, h.center);
        let target = o + (m - o).normalize() * (0.2 * w.sin()) + DVec2::new(w.cos(), 0.0) * 0.05;
        let rep = s
            .drag(&[Drag {
                point: h.other,
                target: target.extend(0.0),
                plane: None,
            }])
            .unwrap();
        assert!(!rep.drag_failed, "Schritt {k}: Ziehen gescheitert");
        assert_rows_hold(&s, pts[2], &h, 1e-6);
    }
}

#[test]
fn the_center_cannot_change_sides() {
    // B9: Die Seite wechselt nicht durch Lösen, auch nicht beim großen Zug durch den Stützpunkt.
    for (xy, left) in [
        ([(0.0, 0.0), (10.0, -6.0), (20.0, 0.0)], true),
        ([(0.0, 0.0), (10.0, 6.0), (20.0, 0.0)], false),
    ] {
        let mut s = sketch();
        let (c, pts) = spline(&mut s, &xy);
        s.activate_curvature_handle(c, pts[1]).unwrap();
        let h = parts(&s, c, pts[1]);
        let is_left = |s: &Sketch| {
            let a = pos(s, pts[1]);
            (pos(s, h.tip) - a).perp_dot(pos(s, h.center) - a) > 0.0
        };
        assert_eq!(is_left(&s), left);
        let sign = if left { 1.0 } else { -1.0 };
        for far in [1.0, 5.0, 50.0, 500.0] {
            let a = pos(&s, pts[1]);
            let target = a - (pos(&s, h.center) - a).normalize() * far;
            s.drag(&[Drag {
                point: h.center,
                target: target.extend(0.0),
                plane: None,
            }])
            .unwrap();
            assert_eq!(is_left(&s), left, "Zug über {far}");
            assert_rows_hold(&s, pts[1], &h, 1e-6);
            assert!(kappa_side(&s, c, 1.0, 1.0) * sign > 0.0, "Zug über {far}");
        }
        // Auch die Spitze, die in feinen Schritten ganz um den Stützpunkt läuft, kehrt die Seite
        // nicht um, und sie kommt wirklich herum.
        let a = pos(&s, pts[1]);
        s.add_constraint(K::Fixed(pts[1], a.x, a.y)).unwrap();
        let start = pos(&s, h.tip) - a;
        for k in 1..=72 {
            let (sin, cos) = (k as f64 * 5.0_f64.to_radians()).sin_cos();
            let d = DVec2::new(start.x * cos - start.y * sin, start.x * sin + start.y * cos);
            let target = pos(&s, pts[1]) + d;
            let rep = s
                .drag(&[Drag {
                    point: h.tip,
                    target: target.extend(0.0),
                    plane: None,
                }])
                .unwrap();
            assert!(!rep.drag_failed, "Spitze, Schritt {k}");
            assert_eq!(is_left(&s), left, "Spitze, Schritt {k}");
            assert_rows_hold(&s, pts[1], &h, 1e-6);
            let now = pos(&s, h.tip) - pos(&s, pts[1]);
            let off = now.normalize().dot(d.normalize());
            assert!(off > 0.9, "Spitze folgt nicht, Schritt {k}: {off}");
        }
        // Eine harte Vorgabe auf der anderen Seite wäre lösbar, wird aber nicht erfüllt: Die
        // Skizze meldet das und bleibt auf ihrer Seite.
        let a = pos(&s, pts[1]);
        let m = pos(&s, h.center);
        let mirror = a - (m - a);
        let before = kappa_side(&s, c, 1.0, 1.0);
        let q = pos(&s, h.tip);
        s.add_constraint(K::Fixed(h.tip, q.x, q.y)).unwrap();
        assert_eq!(s.solve().status, SolveStatus::Solved);
        let fixed = s
            .add_constraint(K::Fixed(h.center, mirror.x, mirror.y))
            .unwrap();
        assert_ne!(s.solve().status, SolveStatus::Solved);
        assert_eq!(is_left(&s), left);
        assert!((kappa_side(&s, c, 1.0, 1.0) - before).abs() < 1e-6);
        s.remove_constraint(fixed).unwrap();
        assert_eq!(s.solve().status, SolveStatus::Solved);
    }
}

fn drag_through_the_switch(scale: f64) {
    let mut s = sketch();
    let wave = WAVE.map(|(x, y)| (x * scale, y * scale));
    let (c, pts) = spline(&mut s, &wave);
    s.activate_curvature_handle(c, pts[2]).unwrap();
    let h = parts(&s, c, pts[2]);
    for &p in pts.iter().chain([&h.tip]) {
        let q = pos(&s, p);
        s.add_constraint(K::Fixed(p, q.x, q.y)).unwrap();
    }
    assert_eq!(s.solve().status, SolveStatus::Solved);
    let a = pos(&s, pts[2]);
    let l = pos(&s, h.tip).distance(a);
    let f = (pos(&s, h.tip) - a) / l;
    let n = (pos(&s, h.center) - a).normalize();
    let switch = l / MAX_OPENING;
    // Lage des freien Endes beim Radius `r`, mit der Mitte auf der Seite `n`. Bei `r = switch` liegt
    // sie genau am Knick: Die Bahn des Endes hat dort eine Ecke.
    let end_at = |r: f64| {
        let w = (l / r).min(MAX_OPENING);
        a + n * r + (f * w.sin() - n * w.cos()) * r
    };
    let knick = end_at(switch);
    let mut radii = Vec::new();
    let down = (0..=200).map(|k| switch * (3.0 - 2.5 * k as f64 / 200.0));
    let up = (0..=200).map(|k| switch * (0.5 + 2.5 * k as f64 / 200.0));
    for r in down.chain(up) {
        let rep = s
            .drag(&[Drag {
                point: h.other,
                target: end_at(r).extend(0.0),
                plane: None,
            }])
            .unwrap();
        assert!(!rep.drag_failed, "r = {r}: Ziehen gescheitert");
        assert_rows_hold(&s, pts[2], &h, 1e-6 * scale.max(1.0));
        radii.push(pos(&s, h.center).distance(a));
    }
    // Es hat wirklich beide Seiten des Knicks gesehen.
    assert!(radii.iter().any(|&r| r > 1.5 * switch), "{radii:?}");
    assert!(radii.iter().any(|&r| r < 0.7 * switch), "{radii:?}");

    // Zug in die Ecke hinein, aus vielen Richtungen: Der Löser bleibt stabil.
    for k in 0..240 {
        let w = k as f64 / 48.0 * std::f64::consts::TAU;
        let target = knick + DVec2::new(w.cos(), w.sin()) * (0.1 * scale * (1 + k / 48) as f64);
        let rep = s
            .drag(&[Drag {
                point: h.other,
                target: target.extend(0.0),
                plane: None,
            }])
            .unwrap();
        assert!(!rep.drag_failed, "Richtung {k}: Ziehen gescheitert");
        assert_rows_hold(&s, pts[2], &h, 1e-6 * scale.max(1.0));
    }
}

#[test]
fn arc_on_the_right_side_has_negative_kappa() {
    // Der Bogen über der Spitze: Die Kurve dreht in Laufrichtung nach rechts.
    let mut s = sketch();
    let (c, pts) = spline(&mut s, &[(0.0, 0.0), (10.0, 6.0), (20.0, 0.0)]);
    let before = kappa_side(&s, c, 1.0, 1.0);
    assert!(before < -1e-3);
    let arc = s.activate_curvature_handle(c, pts[1]).unwrap();
    let h = parts(&s, c, pts[1]);
    let CurveKind::Arc { center, start, end } = s.curve(arc).unwrap().kind else {
        panic!("kein Bogen");
    };
    assert_eq!((center, start, end), (h.center, h.other, pts[1]));
    let a = pos(&s, pts[1]);
    let forward = pos(&s, h.tip) - a;
    assert!(
        forward.perp_dot(pos(&s, h.center) - a) < 0.0,
        "Mitte rechts"
    );
    // Das freie Ende liegt vorn, nicht hinter dem Griff.
    assert!((pos(&s, h.other) - a).dot(forward) > 0.0);
    assert_rows_hold(&s, pts[1], &h, 1e-9);
    assert!((kappa_side(&s, c, 1.0, 1.0) - before).abs() < 1e-7);

    // Und das Radiusmaß behält das Vorzeichen.
    s.add_constraint(K::Radius(arc, 4.0)).unwrap();
    assert_eq!(s.solve().status, SolveStatus::Solved);
    assert!((kappa_side(&s, c, 1.0, 1.0) + 0.25).abs() < 1e-6);

    // Linke Seite: Der Griff beginnt am Stützpunkt.
    let mut s = sketch();
    let (c, pts) = spline(&mut s, &[(0.0, 0.0), (10.0, -6.0), (20.0, 0.0)]);
    let arc = s.activate_curvature_handle(c, pts[1]).unwrap();
    let h = parts(&s, c, pts[1]);
    let CurveKind::Arc { start, end, .. } = s.curve(arc).unwrap().kind else {
        panic!("kein Bogen");
    };
    assert_eq!((start, end), (pts[1], h.other));
    assert!(kappa_side(&s, c, 1.0, 1.0) > 1e-3);
    let forward = pos(&s, h.tip) - pos(&s, pts[1]);
    assert!((pos(&s, h.other) - pos(&s, pts[1])).dot(forward) > 0.0);
}

#[test]
fn near_zero_curvature_starts_at_1000_l() {
    let mut s = sketch();
    let (c, pts) = spline(&mut s, &[(0.0, 0.0), (10.0, 0.0), (20.0, 0.0), (30.0, 0.0)]);
    let before = dense(&s, c, 20);
    s.activate_curvature_handle(c, pts[1]).unwrap();
    let h = parts(&s, c, pts[1]);
    let a = pos(&s, pts[1]);
    let l = pos(&s, h.tip).distance(a);
    let m = pos(&s, h.center) - a;
    assert!((m.length() - 1000.0 * l).abs() < 1e-6 * l, "{m}, L = {l}");
    let forward = pos(&s, h.tip) - a;
    assert!(forward.perp_dot(m) > 0.0, "die linke Seite");
    assert_rows_hold(&s, pts[1], &h, 1e-9);
    assert!((kappa_side(&s, c, 1.0, 1.0) - 1.0 / m.length()).abs() < 1e-7);
    // Die einzige erlaubte Formänderung ist klein.
    same_shape(&dense(&s, c, 20), &before, 1e-2);
    assert_eq!(s.solve().status, SolveStatus::Solved);
    // Von dort führt ein Radiusmaß in einem Schritt auf einen kleinen Radius, die Seite bleibt.
    let dim = s.add_constraint(K::Radius(h.arc, 5.0)).unwrap();
    assert_eq!(s.solve().status, SolveStatus::Solved);
    assert!((kappa_side(&s, c, 1.0, 1.0) - 0.2).abs() < 1e-6);
    let accept =
        |_: &Sketch, r: &planora_sketch::solver::SolveReport| r.status == SolveStatus::Solved;
    let (done, _) = s.solve_in_steps(&[(dim, 5.0, 1000.0 * l)], accept).unwrap();
    assert_rows_hold(&done, pts[1], &h, 1e-6);

    // Knapp darunter und darüber: Die Grenze ist |κ|·L = 1e-3.
    let bent = |y: f64| {
        let mut s = sketch();
        let (c, pts) = spline(&mut s, &[(0.0, 0.0), (10.0, y), (20.0, 0.0)]);
        let k = kappa_side(&s, c, 1.0, 1.0);
        s.activate_curvature_handle(c, pts[1]).unwrap();
        let h = parts(&s, c, pts[1]);
        let a = pos(&s, pts[1]);
        let l = pos(&s, h.tip).distance(a);
        (
            k.abs() * l,
            pos(&s, h.center).distance(a) / l,
            (pos(&s, h.tip) - a).perp_dot(pos(&s, h.center) - a) > 0.0,
        )
    };
    let (small, r_small, left) = bent(1e-5);
    assert!(small < 1e-3, "{small}");
    assert!((r_small - 1000.0).abs() < 1e-6 && left);
    let (large, r_large, _) = bent(0.2);
    assert!(large > 1e-3, "{large}");
    assert!((r_large - 1.0 / large).abs() < 1e-6, "{r_large}");
}

#[test]
fn handle_leaves_one_dof_for_the_radius() {
    let mut s = sketch();
    let (c, pts) = spline(&mut s, &WAVE);
    s.activate_curvature_handle(c, pts[2]).unwrap();
    let h = parts(&s, c, pts[2]);
    for &p in pts.iter().chain([&h.tip]) {
        let q = pos(&s, p);
        s.add_constraint(K::Fixed(p, q.x, q.y)).unwrap();
    }
    let rep = s.solve();
    assert_eq!(rep.status, SolveStatus::Solved);
    assert_eq!(rep.dof, 1, "nur der Radius bleibt");

    // Der Zug am freien Ende ändert den Radius, Spline-Punkte und Spitze bleiben.
    let (a, tip) = (pos(&s, pts[2]), pos(&s, h.tip));
    let r0 = pos(&s, h.center).distance(a);
    let target = pos(&s, h.other) + (pos(&s, h.center) - a).normalize() * (0.3 * r0);
    assert!(
        !s.drag(&[Drag {
            point: h.other,
            target: target.extend(0.0),
            plane: None
        }])
        .unwrap()
        .drag_failed
    );
    assert!((pos(&s, h.center).distance(a) - r0).abs() > 1e-3);
    assert!(pos(&s, pts[2]).distance(a) < 1e-7 && pos(&s, h.tip).distance(tip) < 1e-7);

    // Auch am Knick, bei kleinem und bei großem Radius; mit dem Maß ist alles bestimmt.
    let l = tip.distance(a);
    let dim = s.add_constraint(K::Radius(h.arc, r0)).unwrap();
    let rep = s.solve();
    assert_eq!((rep.status, rep.dof), (SolveStatus::Solved, 0));
    s.remove_constraint(dim).unwrap();
    let dim = s.add_constraint(K::Radius(h.arc, l / MAX_OPENING)).unwrap();
    assert_eq!(s.solve().status, SolveStatus::Solved);
    s.remove_constraint(dim).unwrap();
    assert_eq!(s.solve().dof, 1, "am Knick");
    let dim = s.add_constraint(K::Radius(h.arc, 0.1 * l)).unwrap();
    assert_eq!(s.solve().status, SolveStatus::Solved);
    s.remove_constraint(dim).unwrap();
    assert_eq!(s.solve().dof, 1, "kleiner Radius");
    let dim = s.add_constraint(K::Radius(h.arc, 10.0 * l)).unwrap();
    assert_eq!(s.solve().status, SolveStatus::Solved);
    s.set_constraint_value(dim, 1000.0 * l).unwrap();
    assert_eq!(s.solve().status, SolveStatus::Solved);
    s.remove_constraint(dim).unwrap();
    assert_eq!(s.solve().dof, 1, "großer Radius");
}

#[test]
fn deleting_the_arc_keeps_the_tangent_handle() {
    let mut s = sketch();
    let (c, pts) = spline(&mut s, &WAVE);
    let line = s.activate_tangent_handle(c, pts[2]).unwrap();
    // Stützpunkte und Spitze halten: Sonst bewegt das Radiusmaß auch sie, und die Form danach wäre
    // nicht mehr die alte.
    let tip = {
        let CurveKind::Spline { handles, .. } = &s.curve(c).unwrap().kind else {
            panic!();
        };
        handles[0].tip
    };
    for &p in pts.iter().chain([&tip]) {
        let q = pos(&s, p);
        s.add_constraint(K::Fixed(p, q.x, q.y)).unwrap();
    }
    let form = dense(&s, c, 20);
    let rows = s.internal_constraints().count();
    let curves = s.curves().count();
    let points = s.points().count();

    for how in 0..4 {
        let arc = s.activate_curvature_handle(c, pts[2]).unwrap();
        let h = parts(&s, c, pts[2]);
        assert_eq!(h.line, line);
        let radius = s.add_constraint(K::Radius(arc, 5.0)).unwrap();
        assert_eq!(s.solve().status, SolveStatus::Solved);
        assert!(
            dense(&s, c, 20)
                .iter()
                .zip(&form)
                .any(|(p, q)| p.distance(*q) > 1e-3)
        );
        match how {
            0 => s.remove_curve(arc).unwrap(),
            1 => s.deactivate_curvature_handle(c, pts[2]).unwrap(),
            2 => s.remove_point(h.center).unwrap(),
            _ => s.remove_point(h.other).unwrap(),
        }
        let CurveKind::Spline { handles, .. } = &s.curve(c).unwrap().kind else {
            panic!();
        };
        assert_eq!(handles.len(), 1, "der Tangentengriff bleibt");
        assert_eq!(handles[0].curvature, HandleCurvature::Form);
        assert_eq!(s.internal_constraints().count(), rows, "Weg {how}");
        assert_eq!(s.curves().count(), curves);
        assert_eq!(s.points().count(), points);
        assert!(s.constraint(radius).is_none(), "das Maß geht mit");
        assert_eq!(s.solve().status, SolveStatus::Solved);
        same_shape(&dense(&s, c, 20), &form, 1e-3);
    }

    // Ausschalten ohne Krümmungsgriff: abgelehnt, nichts ändert sich.
    let before = s.to_data();
    assert!(s.deactivate_curvature_handle(c, pts[2]).is_err());
    assert!(s.deactivate_curvature_handle(c, pts[3]).is_err());
    assert_eq!(s.to_data(), before);

    // Griff-Linie löschen nimmt den Krümmungsgriff mit, ebenso die Spitze und der Spline.
    let natural = {
        let mut t = sketch();
        let (c, _) = spline(&mut t, &WAVE);
        dense(&t, c, 20)
    };
    for how in 0..3 {
        let mut s = sketch();
        let (c, pts) = spline(&mut s, &WAVE);
        let base = s.internal_constraints().count();
        s.activate_curvature_handle(c, pts[2]).unwrap();
        let h = parts(&s, c, pts[2]);
        match how {
            0 => s.remove_curve(h.line).unwrap(),
            1 => s.remove_point(h.tip).unwrap(),
            _ => s.remove_curve(c).unwrap(),
        }
        assert_eq!(s.internal_constraints().count(), base, "Weg {how}");
        assert!(s.curve(h.arc).is_none() && s.curve(h.line).is_none());
        assert!(s.point_xy(h.center).is_err() && s.point_xy(h.other).is_err());
        assert_eq!(s.solve().status, SolveStatus::Solved);
        if how < 2 {
            same_shape(&dense(&s, c, 20), &natural, 1e-9);
        }
    }
}

#[test]
fn curve_stays_g2_while_dragging() {
    let mut s = sketch();
    let (c, pts) = spline(&mut s, &WAVE);
    s.activate_curvature_handle(c, pts[2]).unwrap();
    s.activate_curvature_handle(c, pts[3]).unwrap();
    let h = parts(&s, c, pts[2]);
    let a = pos(&s, pts[2]);
    let dir0 = pos(&s, h.tip) - a;
    let side0 = kappa_side(&s, c, 2.0, 1.0).signum();
    let mut kappas = Vec::new();
    let check = |s: &Sketch, kappas: &mut Vec<f64>, what: &str| {
        // Über jede innere Fuge: Tangente und Krümmung springen nicht.
        for t in [1.0, 2.0, 3.0, 4.0] {
            let (k0, k1) = (kappa_side(s, c, t, -1.0), kappa_side(s, c, t, 1.0));
            assert!((k0 - k1).abs() < 1e-6, "{what}, Fuge {t}: {k0} / {k1}");
        }
        // Am Griff: der Griffwert.
        let now = parts(s, c, pts[2]);
        let r = pos(s, now.center).distance(pos(s, pts[2]));
        let k = kappa_side(s, c, 2.0, 1.0);
        assert!((k - side0 / r).abs() < 1e-6, "{what}: κ {k}, r {r}");
        assert_rows_hold(s, pts[2], &now, 1e-6);
        kappas.push(k);
    };
    // Griffspitze fein im Bogen führen.
    for k in 0..=120 {
        let w = 0.8 * (k as f64 * 0.05).sin();
        let (sin, cos) = w.sin_cos();
        let d = DVec2::new(dir0.x * cos - dir0.y * sin, dir0.x * sin + dir0.y * cos);
        let target = a + d * (1.0 + 0.4 * (k as f64 * 0.11).sin());
        assert!(
            !s.drag(&[Drag {
                point: h.tip,
                target: target.extend(0.0),
                plane: None
            }])
            .unwrap()
            .drag_failed
        );
        check(&s, &mut kappas, "Spitze");
    }
    // Das freie Bogenende fein führen.
    for k in 0..=120 {
        let m = pos(&s, h.center);
        let o = pos(&s, h.other);
        let target = o + (o - m).normalize() * (0.15 * (k as f64 * 0.2).cos());
        assert!(
            !s.drag(&[Drag {
                point: h.other,
                target: target.extend(0.0),
                plane: None
            }])
            .unwrap()
            .drag_failed
        );
        check(&s, &mut kappas, "Bogenende");
    }
    // Die Mitte fein führen, nie durch den Stützpunkt.
    for k in 0..=120 {
        let p = pos(&s, pts[2]);
        let m = pos(&s, h.center);
        let r = m.distance(p);
        let target = p + (m - p) / r * (r * (1.0 + 0.04 * (k as f64 * 0.3).sin()));
        assert!(
            !s.drag(&[Drag {
                point: h.center,
                target: target.extend(0.0),
                plane: None
            }])
            .unwrap()
            .drag_failed
        );
        check(&s, &mut kappas, "Mitte");
    }
    // Es hat sich wirklich etwas bewegt.
    let (lo, hi) = kappas
        .iter()
        .fold((f64::MAX, f64::MIN), |(l, h), k| (l.min(*k), h.max(*k)));
    assert!(hi - lo > 1e-3, "{lo} .. {hi}");
}

#[test]
fn scaling_keeps_the_rows_and_scales_the_curvature() {
    let mut s = sketch();
    let (c, pts) = spline(&mut s, &WAVE);
    s.activate_curvature_handle(c, pts[2]).unwrap();
    let k = kappa_side(&s, c, 2.0, 1.0);
    let before = dense(&s, c, 20);
    let set = s.transform_set(&[c], &[]);
    let h = parts(&s, c, pts[2]);
    for p in [h.tip, h.center, h.other] {
        assert!(set.points.contains(&p));
    }
    let map = Similarity {
        base: DVec2::ZERO,
        offset: DVec2::new(3.0, -2.0),
        angle: 0.7,
        factor: 2.0,
    };
    s.apply_transform(&set, &map).unwrap();
    assert_eq!(s.solve().status, SolveStatus::Solved);
    assert_rows_hold(&s, pts[2], &h, 1e-7);
    assert!((kappa_side(&s, c, 2.0, 1.0) - 0.5 * k).abs() < 1e-6);
    let (sin, cos) = 0.7_f64.sin_cos();
    for (p, q) in dense(&s, c, 20).iter().zip(&before) {
        let want =
            DVec2::new(q.x * cos - q.y * sin, q.x * sin + q.y * cos) * 2.0 + DVec2::new(3.0, -2.0);
        assert!(p.distance(want) < 1e-6, "{p} / {want}");
    }
}

#[test]
fn the_row_is_internal_and_cannot_be_added_by_hand() {
    let mut s = sketch();
    let (c, pts) = spline(&mut s, &WAVE);
    assert!(
        s.add_constraint(K::SplineCurvatureHandle(c, pts[2]))
            .is_err()
    );
    let base = s.internal_constraints().count();
    let users = s.constraints().count();
    s.activate_curvature_handle(c, pts[2]).unwrap();
    // Der Griff-Bogen bringt `ArcRadius` mit, dazu die neue Zeile.
    assert_eq!(s.internal_constraints().count(), base + 2);
    assert_eq!(s.constraints().count(), users);
    assert!(
        s.internal_constraints()
            .any(|(_, k)| k.kind == K::SplineCurvatureHandle(c, pts[2]))
    );
}

#[test]
fn data_with_a_curvature_handle_is_checked_on_loading() {
    let mut s = sketch();
    let (c, pts) = spline(&mut s, &WAVE);
    s.activate_curvature_handle(c, pts[2]).unwrap();
    let h = parts(&s, c, pts[2]);
    let d = s.to_data();
    let back = Sketch::from_data(d.clone()).unwrap();
    assert_eq!(back.to_data(), d);
    same_shape(&dense(&back, c, 20), &dense(&s, c, 20), 0.0);

    let row = d
        .constraints
        .iter()
        .position(|k| k.kind == K::SplineCurvatureHandle(c, pts[2]))
        .unwrap();
    let refused = |d: planora_sketch::data::SketchData| {
        let e = Sketch::from_data(d).map(|_| ()).unwrap_err();
        assert!(matches!(e, SketchError::InvalidInput(_)), "{e:?}");
    };
    // Zeile gelöscht oder fehlt: Der Bogen wäre sonst frei.
    let mut bad = d.clone();
    bad.constraints[row].alive = false;
    refused(bad);
    // Zeile nicht intern, abgeschaltet oder nur anzeigend.
    let mut bad = d.clone();
    bad.constraints[row].internal = false;
    refused(bad);
    let mut bad = d.clone();
    bad.constraints[row].enabled = false;
    refused(bad);
    let mut bad = d.clone();
    bad.constraints[row].driven = true;
    refused(bad);
    // Zeile an einem Stützpunkt ohne Krümmungsgriff.
    let mut bad = d.clone();
    bad.constraints[row].kind = K::SplineCurvatureHandle(c, pts[3]);
    refused(bad);
    // Zeile doppelt.
    let mut bad = d.clone();
    let copy = bad.constraints[row].clone();
    bad.constraints.push(copy);
    refused(bad);
    // Zeile an etwas, das kein Spline ist.
    let mut bad = d.clone();
    bad.constraints[row].kind = K::SplineCurvatureHandle(h.line, pts[2]);
    assert!(Sketch::from_data(bad).is_err());
    // Griff ohne Bogen, aber mit Zeile.
    let mut bad = d.clone();
    let CurveKind::Spline { handles, .. } = &mut bad.curves[c.index()].kind else {
        panic!();
    };
    handles[0].curvature = HandleCurvature::Form;
    refused(bad);

    // Nach dem Löschen bleibt die Zeile als gelöschter Eintrag; die Datei lädt.
    s.remove_curve(h.arc).unwrap();
    let d = s.to_data();
    assert!(d.constraints[row].kind == K::SplineCurvatureHandle(c, pts[2]));
    assert!(!d.constraints[row].alive);
    Sketch::from_data(d).unwrap();
}

/// Griff am Stützpunkt `pts[2]`, Spitze soll durch `pin` auf den um `degrees` gedrehten Ort.
fn turn_the_tip(degrees: f64, hold_fit_points: bool, pin: usize) -> Option<(f64, f64)> {
    let mut s = sketch();
    let (c, pts) = spline(&mut s, &WAVE);
    s.activate_curvature_handle(c, pts[2]).unwrap();
    let h = parts(&s, c, pts[2]);
    if hold_fit_points {
        for &p in &pts {
            let q = pos(&s, p);
            s.add_constraint(K::Fixed(p, q.x, q.y)).unwrap();
        }
        assert_eq!(s.solve().status, SolveStatus::Solved);
    }
    let a = pos(&s, pts[2]);
    let side = |s: &Sketch| {
        let a = pos(s, pts[2]);
        (pos(s, h.tip) - a).perp_dot(pos(s, h.center) - a) > 0.0
    };
    let before = side(&s);
    let r0 = pos(&s, h.center).distance(a);
    let d = pos(&s, h.tip) - a;
    let (sin, cos) = degrees.to_radians().sin_cos();
    let q = a + DVec2::new(d.x * cos - d.y * sin, d.x * sin + d.y * cos);
    if pin == 0 {
        let free = s.add_point(q.x, q.y).unwrap();
        s.add_constraint(K::Fixed(free, q.x, q.y)).unwrap();
        s.add_constraint(K::Coincident(h.tip, free)).unwrap();
    } else {
        s.add_constraint(K::Fixed(h.tip, q.x, q.y)).unwrap();
    }
    let rep = s.solve();
    if rep.status != SolveStatus::Solved {
        return None;
    }
    assert_eq!(side(&s), before, "{degrees}°: Seite gewechselt");
    assert!(
        pos(&s, h.tip).distance(q) < 1e-6,
        "{degrees}°: Spitze nicht am Ziel"
    );
    assert_rows_hold(&s, pts[2], &h, 1e-6);
    // Die Krümmung folgt dem Griff mit demselben Vorzeichen.
    let now = pos(&s, h.center).distance(pos(&s, pts[2]));
    let sign = if before { 1.0 } else { -1.0 };
    let k = kappa_side(&s, c, 2.0, 1.0);
    assert!((k - sign / now).abs() < 1e-6, "{degrees}°: κ {k}, r {now}");
    assert!(
        now > 0.2 * r0,
        "{degrees}°: Radius eingebrochen, {r0} auf {now}"
    );
    Some((now, r0))
}

#[test]
fn turning_the_tip_by_a_constraint_is_solvable_on_the_same_side() {
    // In beide Richtungen, frei und mit festen Stützpunkten, über Deckungsgleich und über Fest.
    // Die Seite bleibt, es gibt also keinen falschen Widerspruch.
    for hold in [false, true] {
        for pin in [0, 1] {
            for k in -35..=35 {
                let degrees = 5.0 * k as f64;
                assert!(
                    turn_the_tip(degrees, hold, pin).is_some(),
                    "{degrees}°, feste Stützpunkte {hold}, Art {pin}"
                );
            }
        }
    }
}

#[test]
fn a_tip_held_against_a_held_center_is_not_solved_by_flipping() {
    // Mitte fest, Griff soll herumdrehen: Das ginge nur auf der anderen Seite. Das ist kein Fall
    // für einen Seitenwechsel (B9): Die Skizze meldet es und bleibt stehen.
    let mut s = sketch();
    let (c, pts) = spline(&mut s, &WAVE);
    s.activate_curvature_handle(c, pts[2]).unwrap();
    let h = parts(&s, c, pts[2]);
    for &p in pts.iter().chain([&h.center]) {
        let q = pos(&s, p);
        s.add_constraint(K::Fixed(p, q.x, q.y)).unwrap();
    }
    assert_eq!(s.solve().status, SolveStatus::Solved);
    let a = pos(&s, pts[2]);
    let d = pos(&s, h.tip) - a;
    let (tip0, center0) = (pos(&s, h.tip), pos(&s, h.center));
    let q = a - d;
    s.add_constraint(K::Fixed(h.tip, q.x, q.y)).unwrap();
    assert_ne!(s.solve().status, SolveStatus::Solved);
    assert!(pos(&s, h.tip).distance(tip0) < 1e-9);
    assert!(pos(&s, h.center).distance(center0) < 1e-9);
}

#[test]
fn big_radius_jumps_arrive_in_steps() {
    // Der direkte Weg kann scheitern; die Maßeingabe der Oberfläche geht über `solve_in_steps`.
    for (from, to) in [(1.0, 3000.0), (0.4, 1000.0), (0.1, 1000.0), (3000.0, 0.1)] {
        let mut s = sketch();
        let (c, pts) = spline(&mut s, &WAVE);
        s.activate_curvature_handle(c, pts[2]).unwrap();
        let h = parts(&s, c, pts[2]);
        for &p in pts.iter().chain([&h.tip]) {
            let q = pos(&s, p);
            s.add_constraint(K::Fixed(p, q.x, q.y)).unwrap();
        }
        let l = pos(&s, h.tip).distance(pos(&s, pts[2]));
        let dim = s.add_constraint(K::Radius(h.arc, l)).unwrap();
        assert_eq!(s.solve().status, SolveStatus::Solved);
        // Über 10·L zum Anfangswert, das geht direkt.
        s.set_constraint_value(dim, 10.0 * l).unwrap();
        assert_eq!(s.solve().status, SolveStatus::Solved);
        s.set_constraint_value(dim, from * l).unwrap();
        assert_eq!(s.solve().status, SolveStatus::Solved, "Anfang {from}");
        let accept =
            |_: &Sketch, r: &planora_sketch::solver::SolveReport| r.status == SolveStatus::Solved;
        let (done, _) = s
            .solve_in_steps(&[(dim, from * l, to * l)], accept)
            .unwrap_or_else(|| panic!("{from} -> {to}: nicht angekommen"));
        let r = pos(&done, h.center).distance(pos(&done, pts[2]));
        assert!(
            (r - to * l).abs() < 1e-6 * (to * l).max(1.0),
            "{from} -> {to}: r = {r}"
        );
        assert_rows_hold(&done, pts[2], &h, 1e-5);
    }
}

/// Handtest Skizzen 7: Mit einem Krümmungsgriff am Ende liest dessen Zeile den Spline nur am Ende. Die
/// Spitze eines inneren Griffs und ein innerer Stützpunkt liegen in keiner Gruppe des Lösers, gehören
/// aber zur Kurve, an der die Zeile hängt. Sie folgen dem Zug trotzdem, auf beiden Seiten des Griffs.
#[test]
fn inner_handle_tip_and_fit_point_follow_a_drag_beside_a_curvature_handle() {
    let xy = [(10.66, -16.14), (17.0, -5.0), (26.0, -15.0), (36.0, -7.0)];
    for k in [1, 2] {
        for pull in [DVec2::new(2.0, 3.0), DVec2::new(-6.0, 1.5)] {
            let mut s = sketch();
            let (c, pts) = spline(&mut s, &xy);
            s.activate_curvature_handle(c, pts[3]).unwrap();
            s.activate_tangent_handle(c, pts[k]).unwrap();
            assert_eq!(s.solve().status, SolveStatus::Solved);
            let CurveKind::Spline { handles, .. } = &s.curve(c).unwrap().kind else {
                panic!("kein Spline");
            };
            let tip = handles.iter().find(|h| h.at == pts[k]).unwrap().tip;
            let before = dense(&s, c, 8);
            let to = pos(&s, tip) + pull;
            let r = s
                .drag_holding(
                    &[Drag {
                        point: tip,
                        target: to.extend(0.0),
                        plane: None,
                    }],
                    &[pts[k]],
                )
                .unwrap();
            assert_eq!(r.status, SolveStatus::Solved, "{k} {pull}");
            assert!(pos(&s, tip).distance(to) < 1e-9, "{k} {pull}: Spitze");
            assert!(
                pos(&s, pts[k]).distance(DVec2::new(xy[k].0, xy[k].1)) < 1e-12,
                "Stützpunkt bleibt"
            );
            let moved = dense(&s, c, 8)
                .iter()
                .zip(&before)
                .map(|(p, q)| p.distance(*q))
                .fold(0.0, f64::max);
            assert!(moved > 0.1, "{k} {pull}: die Kurve folgt ({moved})");
            // Der Stützpunkt selbst zieht ebenso.
            let to = DVec2::new(xy[k].0, xy[k].1) + pull;
            s.drag(&[Drag {
                point: pts[k],
                target: to.extend(0.0),
                plane: None,
            }])
            .unwrap();
            assert!(
                pos(&s, pts[k]).distance(to) < 1e-9,
                "{k} {pull}: Stützpunkt"
            );
        }
    }
}

/// Zweiter Handtest Skizzen 7: Ein Natur-Ende hat Krümmung fast 0, der Krümmungsgriff beginnt bei
/// r = 1000·L (B5). Von dort kommt der Zug am freien Bogenende allein nicht an: Der Löser bleibt beim
/// großen Radius. `aim_curvature_arc` legt den Bogen vorher auf den Radius, dessen Ende dem Ziel am
/// nächsten liegt, auf derselben Seite (B9); danach hält der Zug wie sonst Stützpunkt und Spitze.
#[test]
fn aiming_the_curvature_arc_reaches_the_nearest_end_from_a_flat_start() {
    let xy = [(10.0, -16.0), (17.0, -5.0), (26.0, -15.0), (36.0, -7.0)];
    for end in [0usize, 3] {
        for reach in [1.5, 3.0, 6.0] {
            let mut s = sketch();
            let (c, pts) = spline(&mut s, &xy);
            s.activate_curvature_handle(c, pts[end]).unwrap();
            let h = parts(&s, c, pts[end]);
            let (a, t, m0, o0) = (
                pos(&s, pts[end]),
                pos(&s, h.tip),
                pos(&s, h.center),
                pos(&s, h.other),
            );
            let side = (t - a).perp_dot(m0 - a).signum();
            let target = o0 + (m0 - o0).normalize() * reach;
            // Das nächste erreichbare Ende auf dieser Seite, grob abgetastet.
            let l = t.distance(a);
            let best = (0..4000)
                .map(|k| {
                    let r = l * 1e-2 * (1e5f64).powf(f64::from(k) / 4000.0);
                    let n = (t - a).perp().normalize() * side;
                    let m = a + n * r;
                    let turn = DVec2::from_angle(side * (l / r).min(MAX_OPENING));
                    m + turn.rotate(a - m)
                })
                .map(|q| q.distance(target))
                .fold(f64::INFINITY, f64::min);
            let what = format!("Ende {end}, Weg {reach}");
            s.aim_curvature_arc(c, pts[end], target).unwrap();
            s.drag_holding(
                &[Drag {
                    point: h.other,
                    target: target.extend(0.0),
                    plane: None,
                }],
                &[pts[end], h.tip],
            )
            .unwrap();
            let got = pos(&s, h.other).distance(target);
            assert!(got < best + 1e-3, "{what}: {got} statt {best}");
            assert!(pos(&s, pts[end]).distance(a) < 1e-9, "{what}");
            assert!(pos(&s, h.tip).distance(t) < 1e-9, "{what}");
            let m = pos(&s, h.center);
            assert_eq!((t - a).perp_dot(m - a).signum(), side, "{what}: Seite");
            assert_eq!(s.solve().status, SolveStatus::Solved, "{what}");
        }
    }
}

/// Nachprüfung zu 9741876: Bei kleinem Radius und gekappter Öffnung hat das erreichbare Bogenende zwei
/// Äste. Pendelt das Ziel um die Stelle, an der beide gleich nah sind, darf das Ende nicht bei jedem
/// Bild zwischen ihnen springen; höchstens einmal.
#[test]
fn aiming_does_not_flip_between_two_branches_when_the_target_wobbles() {
    let xy = [(0.0, 0.0), (10.0, 6.0), (21.0, -3.0), (30.0, 8.0)];
    let mut s = sketch();
    let (c, pts) = spline(&mut s, &xy);
    s.activate_curvature_handle(c, pts[3]).unwrap();
    let h = parts(&s, c, pts[3]);
    let (a, t) = (pos(&s, pts[3]), pos(&s, h.tip));
    let l = a.distance(t);
    let tu = (t - a) / l;
    let side = tu.perp_dot(pos(&s, h.center) - a).signum();
    let n = tu.perp() * side;
    // Alle erreichbaren Enden auf dieser Seite, fein abgetastet.
    let reach: Vec<DVec2> = (0..20000)
        .map(|k| {
            let r = l * 1e-2 * (1e5f64).powf(f64::from(k) / 20000.0);
            let m = a + n * r;
            m + DVec2::from_angle(side * (l / r).min(MAX_OPENING)).rotate(a - m)
        })
        .collect();
    let nearest = |q: DVec2| {
        *reach
            .iter()
            .min_by(|x, y| x.distance(q).total_cmp(&y.distance(q)))
            .unwrap()
    };
    // Eine Linie quer vor der Spitze; dort, wo der nächste Punkt springt, liegt die Mitte der Äste.
    let (p0, p1) = (a + n * (0.5 * l), a + tu * (0.9 * l) + n * (0.5 * l));
    let at_frac = |f: f64| p0 + (p1 - p0) * f;
    let steps = 400;
    let switch = (1..=steps)
        .find(|&i| {
            let (u, v) = (
                f64::from(i - 1) / f64::from(steps),
                f64::from(i) / f64::from(steps),
            );
            nearest(at_frac(u)).distance(nearest(at_frac(v))) > 0.2 * l
        })
        .expect("die Linie kreuzt die Mitte zwischen den Ästen");
    let drag_to = |s: &mut Sketch, q: DVec2| {
        s.aim_curvature_arc(c, pts[3], q).unwrap();
        s.drag_holding(
            &[Drag {
                point: h.other,
                target: q.extend(0.0),
                plane: None,
            }],
            &[pts[3], h.tip],
        )
        .unwrap();
        pos(s, h.other)
    };
    // Hin bis kurz vor die Stelle, dann pendeln.
    let mut last = pos(&s, h.other);
    for i in 0..switch {
        last = drag_to(&mut s, at_frac(f64::from(i) / f64::from(steps)));
    }
    let mut jumps = 0;
    for k in 0..40 {
        let i = if k % 2 == 0 { switch } else { switch - 1 };
        let now = drag_to(&mut s, at_frac(f64::from(i) / f64::from(steps)));
        if now.distance(last) > 0.2 * l {
            jumps += 1;
        }
        last = now;
    }
    assert!(jumps <= 1, "{jumps} Sprünge beim Pendeln");
}
