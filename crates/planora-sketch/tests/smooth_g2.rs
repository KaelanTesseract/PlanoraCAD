//! Glatt G2 (Skizzen 7, Task 9; Spec C): die Krümmungszeile, der Befehl `prepare_smooth` und welche
//! Kurven zusammengehen. Die Krümmung wird nur über die Lage der Kurve gemessen.

use planora_math::{DVec2, Frame, Tolerance};
use planora_sketch::constraint::ConstraintKind as K;
use planora_sketch::model::{CurveId, CurveKind, PointId, Sketch};
use planora_sketch::solver::{Drag, SolveStatus};
use planora_sketch::{ConstraintId, HandleCurvature};

fn sketch() -> Sketch {
    Sketch::new(Frame::WORLD, "S", Tolerance::default())
}

fn points(s: &mut Sketch, xy: &[(f64, f64)]) -> Vec<PointId> {
    xy.iter()
        .map(|&(x, y)| s.add_point(x, y).unwrap())
        .collect()
}

fn pos(s: &Sketch, p: PointId) -> DVec2 {
    s.point_xy(p).unwrap()
}

/// Sechs-Punkt-Formeln an einem Kurvenende; `side` = +1 vom Anfang des Bereichs aus, sonst −1.
fn ends_of(s: &Sketch, c: CurveId, side: f64) -> (DVec2, DVec2) {
    let (lo, hi) = s.curve_domain(c).unwrap();
    let h = (hi - lo) * 2e-3;
    let t = if side > 0.0 { lo } else { hi };
    let f: [DVec2; 6] = std::array::from_fn(|k| s.curve_eval(c, t + side * k as f64 * h).unwrap());
    let d = (f[0] * (-137.0 / 60.0) + f[1] * 5.0 - f[2] * 5.0 + f[3] * (10.0 / 3.0) - f[4] * 1.25
        + f[5] * 0.2)
        / h
        * side;
    let a = (f[0] * (15.0 / 4.0) - f[1] * (77.0 / 6.0) + f[2] * (107.0 / 6.0) - f[3] * 13.0
        + f[4] * (61.0 / 12.0)
        - f[5] * (5.0 / 6.0))
        / (h * h);
    (d, a)
}

/// Liegt `p` am Anfang des Bereichs von `c`?
fn at_start(s: &Sketch, c: CurveId, p: PointId) -> bool {
    let (lo, hi) = s.curve_domain(c).unwrap();
    let q = pos(s, p);
    s.curve_eval(c, lo).unwrap().distance(q) < s.curve_eval(c, hi).unwrap().distance(q)
}

/// Krümmung am Ende `p` in Richtung in die Kurve hinein.
fn kappa_into(s: &Sketch, c: CurveId, p: PointId) -> f64 {
    let first = at_start(s, c, p);
    let (d, a) = ends_of(s, c, if first { 1.0 } else { -1.0 });
    let k = d.perp_dot(a) / d.length().powi(3);
    if first { k } else { -k }
}

/// Einheitsrichtung am Ende `p` in die Kurve hinein.
fn dir_into(s: &Sketch, c: CurveId, p: PointId) -> DVec2 {
    let first = at_start(s, c, p);
    let (d, _) = ends_of(s, c, if first { 1.0 } else { -1.0 });
    if first { d.normalize() } else { -d.normalize() }
}

/// Die Fuge ist G2: gleiche Lage, gegenläufige Richtung, Krümmungssprung unter 1e-6.
fn assert_g2(s: &Sketch, a: CurveId, pa: PointId, b: CurveId, pb: PointId, what: &str) {
    assert!(pos(s, pa).distance(pos(s, pb)) < 1e-6, "{what}: Lage");
    let turn = dir_into(s, a, pa).dot(dir_into(s, b, pb));
    assert!((turn + 1.0).abs() < 1e-8, "{what}: Richtung {turn}");
    let (ka, kb) = (kappa_into(s, a, pa), kappa_into(s, b, pb));
    assert!((ka + kb).abs() < 1e-6, "{what}: κ {ka} und {kb}");
}

fn solved(s: &mut Sketch, what: &str) {
    let r = s.solve();
    assert_eq!(r.status, SolveStatus::Solved, "{what}: {r:?}");
}

/// Der Befehl aus C3 ohne Oberfläche.
fn smooth(s: &mut Sketch, a: CurveId, pa: PointId, b: CurveId, pb: PointId) -> ConstraintId {
    s.prepare_smooth(a, pa, b, pb).unwrap();
    s.add_constraint(K::Smooth(a, pa, b, pb)).unwrap()
}

fn dense(s: &Sketch, c: CurveId) -> Vec<DVec2> {
    let (lo, hi) = s.curve_domain(c).unwrap();
    (0..=200)
        .map(|k| {
            s.curve_eval(c, lo + (hi - lo) * f64::from(k) / 200.0)
                .unwrap()
        })
        .collect()
}

fn near(a: &[DVec2], b: &[DVec2], limit: f64) {
    for (i, (p, q)) in a.iter().zip(b).enumerate() {
        assert!(p.distance(*q) <= limit, "Stelle {i}: {p} / {q}");
    }
}

fn rows(s: &Sketch, id: ConstraintId) -> usize {
    s.evaluate(id, s.variables()).unwrap().residuals.len()
}

fn curvature_of(s: &Sketch, c: CurveId, at: PointId) -> Option<HandleCurvature> {
    let CurveKind::Spline { handles, .. } = &s.curve(c).unwrap().kind else {
        panic!("kein Spline");
    };
    handles.iter().find(|h| h.at == at).map(|h| h.curvature)
}

fn tip_of(s: &Sketch, c: CurveId, at: PointId) -> PointId {
    let CurveKind::Spline { handles, .. } = &s.curve(c).unwrap().kind else {
        panic!("kein Spline");
    };
    handles.iter().find(|h| h.at == at).unwrap().tip
}

/// Bogen um den Ursprung mit Radius `r` von (r, 0) nach (0, r).
fn quarter(s: &mut Sketch, r: f64) -> (CurveId, PointId, PointId, PointId) {
    let p = points(s, &[(0.0, 0.0), (r, 0.0), (0.0, r)]);
    (s.add_arc(p[0], p[1], p[2]).unwrap(), p[0], p[1], p[2])
}

// ---- Spline durch Stützpunkte am Bogen ---------------------------------------------------------

#[test]
fn spline_arc_is_g2() {
    let mut s = sketch();
    let (arc, _, _, en) = quarter(&mut s, 3.0);
    let q = points(&mut s, &[(-3.0, 5.0), (-7.0, 6.0), (-10.0, 3.0)]);
    let sp = s.add_spline(&[en, q[0], q[1], q[2]]).unwrap();
    let radius = s.add_constraint(K::Radius(arc, 3.0)).unwrap();
    let id = smooth(&mut s, arc, en, sp, en);
    assert_eq!(rows(&s, id), 2, "Winkel und Krümmung");
    solved(&mut s, "angelegt");
    assert_g2(&s, arc, en, sp, en, "r = 3");
    assert!(matches!(
        curvature_of(&s, sp, en),
        Some(HandleCurvature::Arc { .. })
    ));
    // Das Radiusmaß zieht die Krümmung mit.
    s.set_constraint_value(radius, 7.0).unwrap();
    solved(&mut s, "r = 7");
    assert_g2(&s, arc, en, sp, en, "r = 7");
    assert!((kappa_into(&s, arc, en).abs() - 1.0 / 7.0).abs() < 1e-6);
}

/// Beide Enden des Splines, beide Enden des Bogens, der Spline biegt nach beiden Seiten: Der Griff
/// muss in jedem Fall auf die richtige Seite kommen, auch wenn sein Bogen schon auf der anderen lag.
#[test]
fn every_end_and_side_is_g2() {
    for spline_first in [true, false] {
        for arc_start in [true, false] {
            for bend in [1.0, -1.0] {
                for radius in [0.6, 3.0, 40.0] {
                    let what = format!("{spline_first} {arc_start} {bend} {radius}");
                    let mut s = sketch();
                    let (arc, _, st, en) = quarter(&mut s, radius);
                    let joint = if arc_start { st } else { en };
                    let from = pos(&s, joint);
                    let mut xy: Vec<(f64, f64)> = [(4.0, 3.0), (9.0, -2.0), (14.0, 2.0)]
                        .iter()
                        .map(|&(x, y)| (from.x + x, from.y + bend * y))
                        .collect();
                    if !spline_first {
                        xy.reverse();
                    }
                    let q = points(&mut s, &xy);
                    let list = if spline_first {
                        vec![joint, q[0], q[1], q[2]]
                    } else {
                        vec![q[0], q[1], q[2], joint]
                    };
                    let sp = s.add_spline(&list).unwrap();
                    smooth(&mut s, arc, joint, sp, joint);
                    solved(&mut s, &what);
                    assert_g2(&s, arc, joint, sp, joint, &what);
                }
            }
        }
    }
}

/// Die Griff-Lage kommt vor dem Lösen: Auch ein Radius weit über der Grifflänge kommt an, wo
/// das reine Lösen von der aktuellen Krümmung aus nicht hinreicht.
#[test]
fn large_and_small_partner_radii_arrive() {
    for r in [0.05, 3000.0, 20000.0] {
        let mut s = sketch();
        let (arc, _, _, en) = quarter(&mut s, r);
        let from = pos(&s, en);
        let q = points(
            &mut s,
            &[
                (from.x - 2.0, from.y + 1.0),
                (from.x - 4.0, from.y - 1.0),
                (from.x - 6.0, from.y),
            ],
        );
        let sp = s.add_spline(&[en, q[0], q[1], q[2]]).unwrap();
        smooth(&mut s, arc, en, sp, en);
        solved(&mut s, &format!("{r}"));
        assert_g2(&s, arc, en, sp, en, &format!("{r}"));
    }
}

#[test]
fn prepare_keeps_the_handle_length_at_a_third_of_the_derivative() {
    let mut s = sketch();
    let (arc, _, _, en) = quarter(&mut s, 3.0);
    let q = points(&mut s, &[(-3.0, 5.0), (-7.0, 6.0), (-10.0, 3.0)]);
    let sp = s.add_spline(&[en, q[0], q[1], q[2]]).unwrap();
    let (d, _) = ends_of(&s, sp, 1.0);
    let moved: Vec<DVec2> = q.iter().map(|&p| pos(&s, p)).collect();
    s.prepare_smooth(arc, en, sp, en).unwrap();
    let tip = tip_of(&s, sp, en);
    let reach = pos(&s, tip) - pos(&s, en);
    assert!((reach.length() - d.length() / 3.0).abs() < 1e-3, "{reach}");
    // Schon vor dem Lösen auf den Werten des Partners.
    assert!(dir_into(&s, sp, en).dot(dir_into(&s, arc, en)) < -1.0 + 1e-8);
    assert!((kappa_into(&s, sp, en) + kappa_into(&s, arc, en)).abs() < 1e-6);
    // Vorbereiten zieht keine Stützpunkte.
    for (&p, m) in q.iter().zip(&moved) {
        assert_eq!(pos(&s, p), *m);
    }
}

#[test]
fn a_refused_prepare_changes_nothing() {
    let mut s = sketch();
    let (arc, _, _, en) = quarter(&mut s, 3.0);
    let q = points(&mut s, &[(-3.0, 5.0), (-7.0, 6.0)]);
    let sp = s.add_spline(&[en, q[0], q[1]]).unwrap();
    let before = s.to_data();
    // Kein Ende.
    assert!(s.prepare_smooth(arc, en, sp, q[0]).is_err());
    assert_eq!(s.to_data(), before);
    // Dieselbe Kurve.
    assert!(s.prepare_smooth(sp, en, sp, q[1]).is_err());
    assert_eq!(s.to_data(), before);
}

// ---- Kontrollpunkt-Spline und Kegelschnitt -----------------------------------------------------

fn control(s: &mut Sketch, first: PointId, xy: &[(f64, f64)], degree: u8) -> CurveId {
    let mut pts = vec![first];
    pts.extend(points(s, xy));
    s.add_control_spline(&pts, degree).unwrap()
}

const CONTROL: [(f64, f64); 5] = [
    (-2.0, 5.0),
    (-5.0, 8.0),
    (-9.0, 8.0),
    (-12.0, 5.0),
    (-14.0, 1.0),
];

#[test]
fn control_spline_arc_is_g2() {
    for degree in [3, 5] {
        let mut s = sketch();
        let (arc, _, _, en) = quarter(&mut s, 3.0);
        let c = control(&mut s, en, &CONTROL, degree);
        let curves = s.curves().count();
        let id = smooth(&mut s, arc, en, c, en);
        assert_eq!(rows(&s, id), 2);
        assert_eq!(
            s.curves().count(),
            curves,
            "kein Griff am Kontrollpunkt-Spline"
        );
        solved(&mut s, "Grad");
        assert_g2(&s, arc, en, c, en, "Kontrollpunkt-Spline am Bogen");
    }
}

#[test]
fn conic_control_spline_is_g2() {
    let mut s = sketch();
    let p = points(&mut s, &[(0.0, 0.0), (8.0, 0.0), (4.0, 6.0)]);
    let conic = s.add_conic(p[0], p[1], p[2], 0.4).unwrap();
    let c = control(
        &mut s,
        p[1],
        &[(11.0, -3.0), (15.0, -5.0), (20.0, -4.0), (24.0, -1.0)],
        3,
    );
    let id = smooth(&mut s, conic, p[1], c, p[1]);
    assert_eq!(rows(&s, id), 2);
    solved(&mut s, "Kegelschnitt");
    assert_g2(
        &s,
        conic,
        p[1],
        c,
        p[1],
        "Kegelschnitt am Kontrollpunkt-Spline",
    );
}

#[test]
fn fit_spline_follows_a_conic_and_a_control_spline() {
    let mut s = sketch();
    let p = points(&mut s, &[(0.0, 0.0), (8.0, 0.0), (4.0, 6.0)]);
    let conic = s.add_conic(p[0], p[1], p[2], 0.7).unwrap();
    let q = points(&mut s, &[(12.0, -2.0), (16.0, 1.0)]);
    let sp = s.add_spline(&[p[1], q[0], q[1]]).unwrap();
    smooth(&mut s, conic, p[1], sp, p[1]);
    solved(&mut s, "Kegelschnitt");
    assert_g2(&s, conic, p[1], sp, p[1], "Spline am Kegelschnitt");
    assert!(matches!(
        curvature_of(&s, sp, p[1]),
        Some(HandleCurvature::Arc { .. })
    ));

    let mut s = sketch();
    let (_, _, _, en) = quarter(&mut s, 3.0);
    let c = control(&mut s, en, &CONTROL, 5);
    let CurveKind::ControlSpline { points: cp, .. } = s.curve(c).unwrap().kind.clone() else {
        panic!("falsche Art");
    };
    let last = *cp.last().unwrap();
    let q = points(&mut s, &[(-16.0, -3.0), (-20.0, -5.0)]);
    let sp = s.add_spline(&[last, q[0], q[1]]).unwrap();
    smooth(&mut s, c, last, sp, last);
    solved(&mut s, "Kontrollpunkte");
    assert_g2(&s, c, last, sp, last, "Spline am Kontrollpunkt-Spline");
    assert!(matches!(
        curvature_of(&s, sp, last),
        Some(HandleCurvature::Arc { .. })
    ));
}

#[test]
fn a_nearly_straight_control_spline_gets_a_straight_partner_and_adapts() {
    let mut s = sketch();
    let pts = points(
        &mut s,
        &[
            (0.0, 0.0),
            (4.0, 0.0),
            (8.0, 0.0),
            (12.0, 2.0),
            (16.0, 2.0),
            (20.0, 0.0),
        ],
    );
    let c = s.add_control_spline(&pts, 3).unwrap();
    let q = points(&mut s, &[(-4.0, 3.0), (-8.0, 1.0)]);
    let sp = s.add_spline(&[pts[0], q[0], q[1]]).unwrap();
    s.activate_tangent_handle(sp, pts[0]).unwrap();
    let id = smooth(&mut s, c, pts[0], sp, pts[0]);
    assert_eq!(
        curvature_of(&s, sp, pts[0]),
        Some(HandleCurvature::Straight)
    );
    assert_eq!(rows(&s, id), 2);
    solved(&mut s, "fast gerade");
    assert_g2(&s, c, pts[0], sp, pts[0], "fast gerade");
    assert!(kappa_into(&s, c, pts[0]).abs() < 1e-6);
}

#[test]
fn control_spline_against_a_line_adapts_its_points() {
    let mut s = sketch();
    let p = points(&mut s, &[(-10.0, 0.0), (0.0, 0.0)]);
    let l = s.add_line(p[0], p[1]).unwrap();
    let c = control(
        &mut s,
        p[1],
        &[(3.0, 2.0), (7.0, 4.0), (11.0, 3.0), (14.0, 0.0)],
        3,
    );
    s.add_constraint(K::Fixed(p[0], -10.0, 0.0)).unwrap();
    s.add_constraint(K::Fixed(p[1], 0.0, 0.0)).unwrap();
    assert!(kappa_into(&s, c, p[1]).abs() > 1e-3);
    let id = smooth(&mut s, l, p[1], c, p[1]);
    assert_eq!(rows(&s, id), 2);
    solved(&mut s, "Linie");
    assert_g2(&s, l, p[1], c, p[1], "Linie");
    assert!(kappa_into(&s, c, p[1]).abs() < 1e-6);
}

// ---- Zwei Splines durch Stützpunkte ------------------------------------------------------------

/// Zwei Splines am gemeinsamen Punkt, an dem beide einen Griff haben können.
fn two_splines(s: &mut Sketch) -> (CurveId, CurveId, PointId) {
    let joint = s.add_point(0.0, 0.0).unwrap();
    let q = points(s, &[(-5.0, 4.0), (-10.0, 1.0), (-15.0, 3.0)]);
    let a = s.add_spline(&[joint, q[0], q[1], q[2]]).unwrap();
    let r = points(s, &[(6.0, 3.0), (11.0, -1.0), (16.0, 2.0)]);
    let b = s.add_spline(&[joint, r[0], r[1], r[2]]).unwrap();
    (a, b, joint)
}

/// Tangentengriffe an beiden Splines, die Spitzen gedreht und verschieden lang.
fn turned_tangents(s: &mut Sketch, a: CurveId, b: CurveId, joint: PointId) {
    s.activate_tangent_handle(a, joint).unwrap();
    s.activate_tangent_handle(b, joint).unwrap();
    let (ta, tb) = (tip_of(s, a, joint), tip_of(s, b, joint));
    s.set_point_xy(ta, -1.0, 2.5).unwrap();
    s.set_point_xy(tb, 3.0, -0.5).unwrap();
}

#[test]
fn spline_spline_with_handles_is_g2() {
    // Beide nur mit Tangentengriff, die Spitzen gedreht: Am natürlichen Ende wäre die Krümmung sonst 0.
    let mut s = sketch();
    let (a, b, joint) = two_splines(&mut s);
    turned_tangents(&mut s, a, b, joint);
    let mean = 0.5 * (kappa_into(&s, a, joint) - kappa_into(&s, b, joint));
    let id = smooth(&mut s, a, joint, b, joint);
    assert_eq!(rows(&s, id), 2);
    assert!(matches!(
        curvature_of(&s, a, joint),
        Some(HandleCurvature::Arc { .. })
    ));
    assert!(matches!(
        curvature_of(&s, b, joint),
        Some(HandleCurvature::Arc { .. })
    ));
    // Der Mittelwert, schon vor dem Lösen.
    assert!(mean.abs() > 1e-3, "{mean}");
    assert!((kappa_into(&s, a, joint) - mean).abs() < 1e-6);
    assert!((kappa_into(&s, b, joint) + mean).abs() < 1e-6);
    solved(&mut s, "beide Griffe");
    assert_g2(&s, a, joint, b, joint, "beide Griffe");

    // Beide mit Krümmungsgriff.
    let mut s = sketch();
    let (a, b, joint) = two_splines(&mut s);
    s.activate_curvature_handle(a, joint).unwrap();
    s.activate_curvature_handle(b, joint).unwrap();
    smooth(&mut s, a, joint, b, joint);
    solved(&mut s, "beide Krümmungsgriffe");
    assert_g2(&s, a, joint, b, joint, "beide Krümmungsgriffe");

    // Krümmungsgriff trifft Tangentengriff, in beiden Reihenfolgen.
    for arc_first in [true, false] {
        let mut s = sketch();
        let (a, b, joint) = two_splines(&mut s);
        let (with_arc, plain) = if arc_first { (a, b) } else { (b, a) };
        s.activate_curvature_handle(with_arc, joint).unwrap();
        s.activate_tangent_handle(plain, joint).unwrap();
        smooth(&mut s, a, joint, b, joint);
        solved(&mut s, "gemischt");
        assert_g2(&s, a, joint, b, joint, "gemischt");
    }

    // Krümmungsgriff trifft Spline ohne Griff: Der andere bekommt einen.
    for arc_first in [true, false] {
        let mut s = sketch();
        let (a, b, joint) = two_splines(&mut s);
        let (with_arc, plain) = if arc_first { (a, b) } else { (b, a) };
        s.activate_curvature_handle(with_arc, joint).unwrap();
        smooth(&mut s, a, joint, b, joint);
        assert!(matches!(
            curvature_of(&s, plain, joint),
            Some(HandleCurvature::Arc { .. })
        ));
        solved(&mut s, "ohne Griff");
        assert_g2(&s, a, joint, b, joint, "ohne Griff");
    }
}

#[test]
fn a_tangent_handle_against_a_natural_end_goes_straight() {
    for handle_first in [true, false] {
        let mut s = sketch();
        let (a, b, joint) = two_splines(&mut s);
        let (handled, plain) = if handle_first { (a, b) } else { (b, a) };
        s.activate_tangent_handle(handled, joint).unwrap();
        let id = smooth(&mut s, a, joint, b, joint);
        assert_eq!(
            curvature_of(&s, handled, joint),
            Some(HandleCurvature::Straight)
        );
        assert_eq!(curvature_of(&s, plain, joint), None);
        assert_eq!(rows(&s, id), 1, "beide von selbst 0");
        solved(&mut s, "gerade");
        assert_g2(&s, a, joint, b, joint, "gerade");
    }
}

// ---- Linie -------------------------------------------------------------------------------------

/// Linie von links, Spline ab dem gemeinsamen Punkt.
fn line_and_spline(s: &mut Sketch) -> (CurveId, CurveId, PointId) {
    let (start, joint) = (
        s.add_point(-10.0, 0.0).unwrap(),
        s.add_point(0.0, 0.0).unwrap(),
    );
    let l = s.add_line(start, joint).unwrap();
    let q = points(s, &[(5.0, 4.0), (10.0, -1.0), (15.0, 2.0)]);
    let sp = s.add_spline(&[joint, q[0], q[1], q[2]]).unwrap();
    (l, sp, joint)
}

#[test]
fn spline_line_goes_straight() {
    // Ohne Griff bleibt er natürlich: nichts kommt hinzu.
    let mut s = sketch();
    let (l, sp, joint) = line_and_spline(&mut s);
    let curves = s.curves().count();
    let id = smooth(&mut s, l, joint, sp, joint);
    assert_eq!(s.curves().count(), curves);
    assert_eq!(curvature_of(&s, sp, joint), None);
    assert_eq!(rows(&s, id), 1);
    solved(&mut s, "natürlich");
    assert_g2(&s, l, joint, sp, joint, "natürlich");

    // Mit Tangentengriff wird der zu `Straight`.
    let mut s = sketch();
    let (l, sp, joint) = line_and_spline(&mut s);
    s.activate_tangent_handle(sp, joint).unwrap();
    let id = smooth(&mut s, l, joint, sp, joint);
    assert_eq!(curvature_of(&s, sp, joint), Some(HandleCurvature::Straight));
    assert_eq!(rows(&s, id), 1);
    solved(&mut s, "Griff");
    assert_g2(&s, l, joint, sp, joint, "Griff");

    // Mit Krümmungsgriff entfällt der Bogen samt Zeile und Punkten.
    let mut s = sketch();
    let (l, sp, joint) = line_and_spline(&mut s);
    let bow = s.activate_curvature_handle(sp, joint).unwrap();
    let id = smooth(&mut s, l, joint, sp, joint);
    assert_eq!(curvature_of(&s, sp, joint), Some(HandleCurvature::Straight));
    assert!(s.curve(bow).is_none(), "der Bogen entfällt");
    assert!(
        s.constraints()
            .all(|(_, c)| !matches!(c.kind, K::SplineCurvatureHandle(..))),
        "die interne Zeile entfällt"
    );
    assert_eq!(rows(&s, id), 1);
    solved(&mut s, "Bogen");
    assert_g2(&s, l, joint, sp, joint, "Bogen");
    // Die Spitze zeigt schon vor dem Lösen die Linie entlang.
    let tip = tip_of(&s, sp, joint);
    assert!((pos(&s, tip) - pos(&s, joint)).y.abs() < 1e-6);
}

#[test]
fn straight_handle_stays_at_zero_when_tip_is_dragged() {
    let mut s = sketch();
    let (l, sp, joint) = line_and_spline(&mut s);
    s.activate_tangent_handle(sp, joint).unwrap();
    smooth(&mut s, l, joint, sp, joint);
    solved(&mut s, "angelegt");
    let tip = tip_of(&s, sp, joint);
    for (x, y) in [(3.0, 1.0), (9.0, 2.0), (1.0, 4.0), (4.0, -2.0), (7.0, 0.0)] {
        let r = s
            .drag(&[Drag {
                point: tip,
                target: DVec2::new(x, y).extend(0.0),
                plane: None,
            }])
            .unwrap();
        assert_eq!(r.status, SolveStatus::Solved, "Zug {x} {y}: {r:?}");
        assert_g2(&s, l, joint, sp, joint, &format!("Zug {x} {y}"));
        assert!(kappa_into(&s, sp, joint).abs() < 1e-6);
    }
}

// ---- Zeilen ------------------------------------------------------------------------------------

#[test]
fn natural_ends_add_no_row() {
    // Spline ohne Griff an einer Linie.
    let mut s = sketch();
    let (l, sp, joint) = line_and_spline(&mut s);
    let before = s.clone().solve().dof;
    let id = s.add_constraint(K::Smooth(l, joint, sp, joint)).unwrap();
    assert_eq!(rows(&s, id), 1);
    assert_eq!(s.clone().solve().dof, before - 1, "nur der Winkel");
    // Der Griff an einem anderen Stützpunkt ändert das Ende nicht.
    let CurveKind::Spline { points: list, .. } = s.curve(sp).unwrap().kind.clone() else {
        panic!("falsche Art");
    };
    s.activate_tangent_handle(sp, list[2]).unwrap();
    assert_eq!(rows(&s, id), 1, "Griff anderswo");

    // Zwei natürliche Enden.
    let mut s = sketch();
    let (a, b, joint) = two_splines(&mut s);
    let before = s.clone().solve().dof;
    let id = s.add_constraint(K::Smooth(a, joint, b, joint)).unwrap();
    assert_eq!(rows(&s, id), 1);
    assert_eq!(s.clone().solve().dof, before - 1);

    // Natürlich gegen „mit Wert“ ohne den Befehl: der alte G1-Fall (C4).
    let mut s = sketch();
    let (arc, _, _, en) = quarter(&mut s, 3.0);
    let q = points(&mut s, &[(-3.0, 5.0), (-7.0, 6.0)]);
    let sp = s.add_spline(&[en, q[0], q[1]]).unwrap();
    let before = s.clone().solve().dof;
    let id = s.add_constraint(K::Smooth(arc, en, sp, en)).unwrap();
    assert_eq!(rows(&s, id), 1);
    assert_eq!(s.clone().solve().dof, before - 1);

    // `Straight` und Linie: von selbst G2.
    let mut s = sketch();
    let (l, sp, joint) = line_and_spline(&mut s);
    s.activate_tangent_handle(sp, joint).unwrap();
    s.prepare_smooth(l, joint, sp, joint).unwrap();
    let before = s.clone().solve().dof;
    let id = s.add_constraint(K::Smooth(l, joint, sp, joint)).unwrap();
    assert_eq!(rows(&s, id), 1);
    assert_eq!(s.clone().solve().dof, before - 1);
}

#[test]
fn values_get_a_second_row() {
    // Griff `Form` hat nie eine Zeile (C1), auch nicht gegen eine Linie oder einen Bogen.
    let mut s = sketch();
    let (l, sp, joint) = line_and_spline(&mut s);
    s.activate_tangent_handle(sp, joint).unwrap();
    let id = s.add_constraint(K::Smooth(l, joint, sp, joint)).unwrap();
    assert_eq!(rows(&s, id), 1);
    // Griff `Arc`.
    let mut s = sketch();
    let (arc, _, _, en) = quarter(&mut s, 3.0);
    let q = points(&mut s, &[(-3.0, 5.0), (-7.0, 6.0)]);
    let sp = s.add_spline(&[en, q[0], q[1]]).unwrap();
    s.activate_curvature_handle(sp, en).unwrap();
    let id = s.add_constraint(K::Smooth(arc, en, sp, en)).unwrap();
    assert_eq!(rows(&s, id), 2);
}

/// C1: Ein Tangentengriff an einem alten G1 bringt keine Zeile; die Form bleibt, auch am festen Bogen.
#[test]
fn a_tangent_handle_on_an_old_smooth_adds_no_row_and_keeps_the_shape() {
    for pinned in [false, true] {
        let mut s = sketch();
        let (arc, m, st, en) = quarter(&mut s, 12.0);
        if pinned {
            for p in [m, st, en] {
                let at = pos(&s, p);
                s.add_constraint(K::Fixed(p, at.x, at.y)).unwrap();
            }
        }
        let q = points(&mut s, &[(-4.0, 13.5), (-8.0, 12.0), (-12.0, 9.0)]);
        let sp = s.add_spline(&[en, q[0], q[1], q[2]]).unwrap();
        let id = s.add_constraint(K::Smooth(arc, en, sp, en)).unwrap();
        let old = s.solve();
        assert_eq!(old.status, SolveStatus::Solved, "{old:?}");
        let (shape_arc, shape_spline) = (dense(&s, arc), dense(&s, sp));
        s.activate_tangent_handle(sp, en).unwrap();
        assert_eq!(rows(&s, id), 1);
        assert_eq!(curvature_of(&s, sp, en), Some(HandleCurvature::Form));
        let r = s.solve();
        assert_eq!(r.status, SolveStatus::Solved, "{pinned}: {r:?}");
        // Drei feste Punkte am Bogen sind von sich aus überzählig; der Griff bringt nichts dazu.
        assert_eq!(r.redundant, old.redundant, "{pinned}");
        near(&dense(&s, arc), &shape_arc, 1e-9);
        near(&dense(&s, sp), &shape_spline, 1e-9);
    }
}

/// Linie fest, Tangentengriff: Nichts wird überzählig, auch keine fremde Abhängigkeit.
#[test]
fn a_tangent_handle_at_a_pinned_line_makes_nothing_redundant() {
    let mut s = sketch();
    let (l, sp, joint) = line_and_spline(&mut s);
    let CurveKind::Line { a, b } = s.curve(l).unwrap().kind else {
        panic!("keine Linie");
    };
    for p in [a, b] {
        let at = pos(&s, p);
        s.add_constraint(K::Fixed(p, at.x, at.y)).unwrap();
    }
    s.add_constraint(K::Smooth(l, joint, sp, joint)).unwrap();
    solved(&mut s, "alt");
    s.activate_tangent_handle(sp, joint).unwrap();
    let check = |s: &mut Sketch, what: &str| {
        let r = s.solve();
        assert_eq!(r.status, SolveStatus::Solved, "{what}: {r:?}");
        assert!(r.redundant.is_empty(), "{what}: {r:?}");
        assert!(r.conflicting.is_empty(), "{what}: {r:?}");
    };
    check(&mut s, "Linie fest");
    // Auch die Stützpunkte fest.
    let CurveKind::Spline { points: list, .. } = s.curve(sp).unwrap().kind.clone() else {
        panic!("kein Spline");
    };
    for &p in &list[1..] {
        let at = pos(&s, p);
        s.add_constraint(K::Fixed(p, at.x, at.y)).unwrap();
    }
    check(&mut s, "alles fest");
}

/// C1: Löschen des Krümmungsbogens fällt auf G1 zurück, in jeder Lage lösbar.
#[test]
fn deleting_the_bow_falls_back_to_g1_and_stays_solvable() {
    for spline_first in [true, false] {
        for arc_start in [true, false] {
            for bend in [1.0, -1.0] {
                for pinned in [false, true] {
                    for radius in [1.0, 6.0] {
                        let what = format!("{spline_first} {arc_start} {bend} {pinned} {radius}");
                        let mut s = sketch();
                        let (arc, m, st, en) = quarter(&mut s, radius);
                        let joint = if arc_start { st } else { en };
                        if pinned {
                            for p in [m, st, en] {
                                let at = pos(&s, p);
                                s.add_constraint(K::Fixed(p, at.x, at.y)).unwrap();
                            }
                        }
                        let from = pos(&s, joint);
                        let mut xy: Vec<(f64, f64)> = [(4.0, 3.0), (9.0, -2.0), (14.0, 2.0)]
                            .iter()
                            .map(|&(x, y)| (from.x + x, from.y + bend * y))
                            .collect();
                        if !spline_first {
                            xy.reverse();
                        }
                        let q = points(&mut s, &xy);
                        let list = if spline_first {
                            vec![joint, q[0], q[1], q[2]]
                        } else {
                            vec![q[0], q[1], q[2], joint]
                        };
                        let sp = s.add_spline(&list).unwrap();
                        let id = smooth(&mut s, arc, joint, sp, joint);
                        solved(&mut s, &what);
                        let Some(HandleCurvature::Arc { arc: bow, .. }) =
                            curvature_of(&s, sp, joint)
                        else {
                            panic!("{what}: kein Bogen");
                        };
                        s.remove_curve(bow).unwrap();
                        assert_eq!(curvature_of(&s, sp, joint), Some(HandleCurvature::Form));
                        assert_eq!(rows(&s, id), 1, "{what}");
                        solved(&mut s, &what);
                        let turn = dir_into(&s, arc, joint).dot(dir_into(&s, sp, joint));
                        assert!((turn + 1.0).abs() < 1e-8, "{what}: {turn}");
                    }
                }
            }
        }
    }
}

// ---- Krümmungsgriff an einem Ende mit Glatt (C3) ------------------------------------------------

#[test]
fn a_curvature_handle_on_an_old_smooth_takes_the_partners_values() {
    for pinned in [false, true] {
        let mut s = sketch();
        let (arc, m, st, en) = quarter(&mut s, 12.0);
        if pinned {
            for p in [m, st, en] {
                let at = pos(&s, p);
                s.add_constraint(K::Fixed(p, at.x, at.y)).unwrap();
            }
        }
        let q = points(&mut s, &[(-4.0, 13.5), (-8.0, 12.0), (-12.0, 9.0)]);
        let sp = s.add_spline(&[en, q[0], q[1], q[2]]).unwrap();
        let id = s.add_constraint(K::Smooth(arc, en, sp, en)).unwrap();
        solved(&mut s, "alt");
        s.activate_curvature_handle(sp, en).unwrap();
        assert_eq!(rows(&s, id), 2);
        assert_g2(&s, arc, en, sp, en, "gleich gelöst");
        assert_eq!(s.clone().solve().status, SolveStatus::Solved);
    }
}

#[test]
fn a_curvature_handle_is_refused_against_curvature_zero() {
    // Linie.
    let mut s = sketch();
    let (l, sp, joint) = line_and_spline(&mut s);
    s.add_constraint(K::Smooth(l, joint, sp, joint)).unwrap();
    solved(&mut s, "Linie");
    let before = s.to_data();
    assert!(s.activate_curvature_handle(sp, joint).is_err());
    assert_eq!(s.to_data(), before, "Linie: nichts bleibt");
    // Griff `Straight` an der Linie.
    s.activate_tangent_handle(sp, joint).unwrap();
    s.prepare_smooth(l, joint, sp, joint).unwrap();
    solved(&mut s, "gerade");
    assert_eq!(curvature_of(&s, sp, joint), Some(HandleCurvature::Straight));
    let before = s.to_data();
    assert!(s.activate_curvature_handle(sp, joint).is_err());
    assert_eq!(s.to_data(), before, "gerade: nichts bleibt");
    // Natürliches Ende als Partner.
    let mut s = sketch();
    let (a, b, joint) = two_splines(&mut s);
    s.add_constraint(K::Smooth(a, joint, b, joint)).unwrap();
    solved(&mut s, "natürlich");
    let before = s.to_data();
    assert!(s.activate_curvature_handle(a, joint).is_err());
    assert_eq!(s.to_data(), before, "natürlich: nichts bleibt");
}

/// Zwei feste Bögen mit verschiedenen Radien am selben Spline-Ende: G1 zu beiden geht, die Krümmung
/// beider nie. Der Griff wird abgelehnt, und nichts bleibt.
#[test]
fn a_curvature_handle_that_cannot_solve_is_refused_whole() {
    let mut s = sketch();
    let (first, m, st, en) = quarter(&mut s, 5.0);
    let p = points(&mut s, &[(0.0, -5.0), (10.0, -5.0)]);
    let second = s.add_arc(p[0], p[1], en).unwrap();
    for q in [m, st, en, p[0]] {
        let at = pos(&s, q);
        s.add_constraint(K::Fixed(q, at.x, at.y)).unwrap();
    }
    let q = points(&mut s, &[(-4.0, 6.5), (-8.0, 5.0), (-12.0, 2.0)]);
    let sp = s.add_spline(&[en, q[0], q[1], q[2]]).unwrap();
    s.add_constraint(K::Smooth(first, en, sp, en)).unwrap();
    s.add_constraint(K::Smooth(second, en, sp, en)).unwrap();
    solved(&mut s, "beide tangential");
    let before = s.to_data();
    let e = s.activate_curvature_handle(sp, en).unwrap_err();
    assert!(
        e.to_string()
            .contains(&planora_i18n::tr!("sketch-err-handle-smooth-unsolved")),
        "{e}"
    );
    assert_eq!(s.to_data(), before, "nichts bleibt");
}

#[test]
fn both_selection_orders_are_bit_identical() {
    fn bits(s: &Sketch) -> Vec<u64> {
        s.variables().iter().map(|v| v.to_bits()).collect()
    }
    type Setup = fn(&mut Sketch, CurveId, CurveId, PointId);
    let tangents: Setup = turned_tangents;
    let bows: Setup = |s, a, b, j| {
        turned_tangents(s, a, b, j);
        s.activate_curvature_handle(a, j).unwrap();
        s.activate_curvature_handle(b, j).unwrap();
    };
    for setup in [tangents, bows] {
        let run = |swap: bool| {
            let mut s = sketch();
            let (a, b, j) = two_splines(&mut s);
            setup(&mut s, a, b, j);
            if swap {
                smooth(&mut s, b, j, a, j);
            } else {
                smooth(&mut s, a, j, b, j);
            }
            let prepared = bits(&s);
            solved(&mut s, "Reihenfolge");
            assert_g2(&s, a, j, b, j, "Reihenfolge");
            (prepared, bits(&s), s.curves().count())
        };
        assert_eq!(run(false), run(true));
    }
}

#[test]
fn a_mean_near_zero_makes_both_straight() {
    // Spiegelbildlich an der Fuge: Die Krümmungen in die Kurven sind gleich, das Mittel ist 0.
    let mut s = sketch();
    let joint = s.add_point(0.0, 0.0).unwrap();
    let q = points(&mut s, &[(5.0, 3.0), (10.0, 0.0)]);
    let r = points(&mut s, &[(-5.0, -3.0), (-10.0, 0.0)]);
    let a = s.add_spline(&[joint, q[0], q[1]]).unwrap();
    let b = s.add_spline(&[joint, r[0], r[1]]).unwrap();
    s.activate_tangent_handle(a, joint).unwrap();
    s.activate_tangent_handle(b, joint).unwrap();
    let CurveKind::Spline { handles, .. } = s.curve(a).unwrap().kind.clone() else {
        panic!("kein Spline");
    };
    s.set_point_xy(handles[0].tip, 2.0, 2.0).unwrap();
    let CurveKind::Spline { handles, .. } = s.curve(b).unwrap().kind.clone() else {
        panic!("kein Spline");
    };
    s.set_point_xy(handles[0].tip, -2.0, -2.0).unwrap();
    let id = smooth(&mut s, a, joint, b, joint);
    assert_eq!(curvature_of(&s, a, joint), Some(HandleCurvature::Straight));
    assert_eq!(curvature_of(&s, b, joint), Some(HandleCurvature::Straight));
    assert_eq!(rows(&s, id), 1);
    solved(&mut s, "Mittel 0");
    assert_g2(&s, a, joint, b, joint, "Mittel 0");
}

#[test]
fn a_radius_on_the_bow_moves_the_partner_too() {
    let mut s = sketch();
    let (arc, _, _, en) = quarter(&mut s, 4.0);
    let q = points(&mut s, &[(-3.0, 6.0), (-7.0, 7.0), (-10.0, 4.0)]);
    let sp = s.add_spline(&[en, q[0], q[1], q[2]]).unwrap();
    smooth(&mut s, arc, en, sp, en);
    solved(&mut s, "angelegt");
    let Some(HandleCurvature::Arc { arc: bow, .. }) = curvature_of(&s, sp, en) else {
        panic!("kein Bogen");
    };
    let id = s.add_constraint(K::Radius(bow, 4.0)).unwrap();
    solved(&mut s, "Maß");
    for r in [6.0, 9.0, 3.0] {
        s.set_constraint_value(id, r).unwrap();
        solved(&mut s, &format!("r = {r}"));
        assert_g2(&s, arc, en, sp, en, &format!("r = {r}"));
        assert!(
            (kappa_into(&s, arc, en).abs() - 1.0 / r).abs() < 1e-6,
            "{r}"
        );
    }
}

// ---- Partner -----------------------------------------------------------------------------------

/// Seit Skizzen 11a geht auch der Ellipsenbogen als Partner eines Splines (Task 4); ohne Spline auf
/// einer Seite geht es nach wie vor nicht.
#[test]
fn elliptic_arc_is_a_partner_of_splines() {
    let mut s = sketch();
    let p = points(&mut s, &[(0.0, 0.0), (6.0, 0.0), (0.0, 3.0)]);
    let e = s
        .add_elliptic_arc(p[0], DVec2::new(6.0, 0.0), 3.0, p[1], p[2])
        .unwrap();
    let q = points(&mut s, &[(-3.0, 5.0), (-7.0, 6.0)]);
    let sp = s.add_spline(&[p[2], q[0], q[1]]).unwrap();
    assert!(s.smooth_kinds_fit(e, sp));
    assert!(s.smooth_kinds_fit(sp, e));
    assert!(s.prepare_smooth(e, p[2], sp, p[2]).is_ok());
    assert!(s.add_constraint(K::Smooth(e, p[2], sp, p[2])).is_ok());
}

#[test]
fn partners_follow_c5() {
    let mut s = sketch();
    let (arc, _, _, en) = quarter(&mut s, 3.0);
    let c = control(&mut s, en, &CONTROL, 3);
    let l0 = points(&mut s, &[(50.0, 0.0), (60.0, 0.0)]);
    let line = s.add_line(l0[0], l0[1]).unwrap();
    let k = points(&mut s, &[(70.0, 0.0), (80.0, 0.0), (75.0, 5.0)]);
    let conic = s.add_conic(k[0], k[1], k[2], 0.5).unwrap();
    let f = points(&mut s, &[(90.0, 0.0), (95.0, 5.0)]);
    let fit = s.add_spline(&f).unwrap();
    let k2 = points(&mut s, &[(70.0, 20.0), (80.0, 20.0), (75.0, 25.0)]);
    let conic2 = s.add_conic(k2[0], k2[1], k2[2], 0.5).unwrap();
    let circle_at = s.add_point(0.0, 80.0).unwrap();
    let circle = s.add_circle(circle_at, 2.0).unwrap();
    for spline in [fit, c] {
        for other in [line, arc, conic] {
            assert!(s.smooth_kinds_fit(spline, other));
            assert!(s.smooth_kinds_fit(other, spline));
        }
        assert!(!s.smooth_kinds_fit(spline, spline));
        assert!(!s.smooth_kinds_fit(spline, circle));
    }
    assert!(s.smooth_kinds_fit(fit, c));
    // Ohne Spline auf einer Seite geht es nicht.
    assert!(!s.smooth_kinds_fit(conic, conic2));
    assert!(!s.smooth_kinds_fit(conic, line));
    assert!(!s.smooth_kinds_fit(conic, arc));
    assert!(!s.smooth_kinds_fit(line, arc));
}

// ---- Bewegen und vorhandene Griffe -------------------------------------------------------------

/// Ein vorhandener Griff-Bogen auf der falschen Seite: Der Befehl behält den Bogen und tauscht die
/// Seite. Das Lösen allein täte das nie (B9).
#[test]
fn an_existing_arc_changes_sides_through_the_command_only() {
    for bend in [1.0, -1.0] {
        let mut s = sketch();
        let (arc, _, _, en) = quarter(&mut s, 3.0);
        let q = points(
            &mut s,
            &[(-4.0, 5.0 + bend), (-8.0, 5.0 - bend), (-12.0, 5.0 + bend)],
        );
        let sp = s.add_spline(&[en, q[0], q[1], q[2]]).unwrap();
        let bow = s.activate_curvature_handle(sp, en).unwrap();
        smooth(&mut s, arc, en, sp, en);
        assert!(s.curve(bow).is_some(), "derselbe Bogen");
        assert!(matches!(
            curvature_of(&s, sp, en),
            Some(HandleCurvature::Arc { arc, .. }) if arc == bow
        ));
        solved(&mut s, "Seite");
        assert_g2(&s, arc, en, sp, en, "Seite");
    }
}

/// G2 hält auch unter Zügen an beiden Seiten und unter weiten Radiusmaßen in Schritten.
#[test]
fn g2_holds_while_dragging_and_through_big_radii() {
    let mut s = sketch();
    let (arc, m, _, en) = quarter(&mut s, 5.0);
    let q = points(&mut s, &[(-3.0, 7.0), (-8.0, 8.0), (-12.0, 5.0)]);
    let sp = s.add_spline(&[en, q[0], q[1], q[2]]).unwrap();
    smooth(&mut s, arc, en, sp, en);
    solved(&mut s, "angelegt");
    for (p, x, y) in [
        (q[1], -8.5, 8.5),
        (q[1], -7.0, 9.5),
        (q[2], -13.0, 4.0),
        (m, 0.3, -0.2),
        (m, -0.2, 0.4),
        (en, 0.2, 5.3),
    ] {
        let r = s
            .drag(&[Drag {
                point: p,
                target: DVec2::new(x, y).extend(0.0),
                plane: None,
            }])
            .unwrap();
        assert_eq!(r.status, SolveStatus::Solved, "Zug {x} {y}: {r:?}");
        assert_g2(&s, arc, en, sp, en, &format!("Zug {x} {y}"));
    }
    let id = s.add_constraint(K::Radius(arc, 5.0)).unwrap();
    solved(&mut s, "Maß");
    for r in [7.0, 12.0, 30.0, 2.0] {
        s.set_constraint_value(id, r).unwrap();
        let report = s.solve();
        assert_eq!(report.status, SolveStatus::Solved, "r = {r}: {report:?}");
        assert_g2(&s, arc, en, sp, en, &format!("r = {r}"));
    }
}

// ---- Ellipsenbogen (Skizzen 11a, Task 4) --------------------------------------------------------

/// Ellipsenbogen um den Ursprung (a = 8, b = 4) von (8, 0) nach (0, 4), mit festen Maßen, damit
/// sich der Spline anpasst und nicht der Bogen.
fn fixed_elliptic_quarter(s: &mut Sketch) -> (CurveId, PointId) {
    let p = points(s, &[(0.0, 0.0), (8.0, 0.0), (0.0, 4.0)]);
    let e = s
        .add_elliptic_arc(p[0], DVec2::new(8.0, 0.0), 4.0, p[1], p[2])
        .unwrap();
    s.add_constraint(K::Fixed(p[0], 0.0, 0.0)).unwrap();
    s.add_constraint(K::EllipseMajor(e, 8.0)).unwrap();
    s.add_constraint(K::EllipseMinor(e, 4.0)).unwrap();
    s.add_constraint(K::EllipseAngle(e, 0.0)).unwrap();
    (e, p[2])
}

#[test]
fn smooth_on_elliptic_arc_matches_curvature() {
    let mut s = sketch();
    let (e, en) = fixed_elliptic_quarter(&mut s);
    let sp = control(&mut s, en, &CONTROL, 5);
    let id = smooth(&mut s, e, en, sp, en);
    assert_eq!(rows(&s, id), 2, "Winkel und Krümmung");
    solved(&mut s, "Ellipsenbogen");
    assert_g2(&s, e, en, sp, en, "Ellipsenbogen");
    // Das Ende gleitet auf der Ellipse; dort gilt κ = a·b / (a² sin²t + b² cos²t)^{3/2} mit dem
    // Winkel t des Punkts (a = 8, b = 4, große Halbachse auf x).
    let at = pos(&s, en);
    let t = (at.y / 4.0).atan2(at.x / 8.0);
    let exact = 32.0 / (64.0 * t.sin().powi(2) + 16.0 * t.cos().powi(2)).powf(1.5);
    assert!((kappa_into(&s, e, en).abs() - exact).abs() < 1e-6);
}

#[test]
fn smooth_elliptic_arc_needs_spline_leader() {
    let mut s = sketch();
    let (e, en) = fixed_elliptic_quarter(&mut s);
    let q = points(&mut s, &[(-4.0, 6.0)]);
    let line = s.add_line(en, q[0]).unwrap();
    let before = s.to_data();
    assert!(s.prepare_smooth(e, en, line, en).is_err());
    assert!(s.add_constraint(K::Smooth(e, en, line, en)).is_err());
    assert_eq!(s.to_data(), before, "nichts angelegt");
    assert!(!s.smooth_kinds_fit(e, line));
}
