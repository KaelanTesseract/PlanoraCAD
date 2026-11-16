//! Übergangskurve (Skizzen 11a, Task 9, Spec A2): Kontrollpunkt-Spline zwischen zwei Kurvenenden,
//! G1 mit „Tangente am Ende“, G2 mit „glatt“; Ablehnungen lassen die Skizze bitgleich.

use planora_i18n::tr;
use planora_math::{DVec2, DVec3, Frame, Tolerance};
use planora_sketch::HandleCurvature;
use planora_sketch::SketchError;
use planora_sketch::blend::{BlendEnd, BlendKind, BlendRefusal};
use planora_sketch::constraint::ConstraintKind as K;
use planora_sketch::model::{CurveId, CurveKind, PointId, Sketch};
use planora_sketch::solver::SolveStatus;
use planora_sketch::sparse::SolverPath;
use planora_sketch::text::{HAlign, TextParams, VAlign};
use planora_sketch::text_shape::{Contour, Glyph, Seg, ShapedLine, TextShape, TextStyle};
use std::sync::Arc;

fn sketch() -> Sketch {
    Sketch::new(Frame::WORLD, "S", Tolerance::default())
}

fn pt(s: &mut Sketch, x: f64, y: f64) -> PointId {
    s.add_point(x, y).unwrap()
}

fn pos(s: &Sketch, p: PointId) -> DVec2 {
    s.point_xy(p).unwrap()
}

fn end(curve: CurveId, point: PointId) -> BlendEnd {
    BlendEnd { curve, point }
}

/// Kontrollpunkt-Ids, Grad des Splines `c`.
fn control(s: &Sketch, c: CurveId) -> (Vec<PointId>, u8) {
    match &s.curve(c).unwrap().kind {
        CurveKind::ControlSpline { points, degree, .. } => (points.clone(), *degree),
        k => panic!("kein Kontrollpunkt-Spline: {k:?}"),
    }
}

/// Einheitsrichtung am Anfang und Krümmungsvektor des Bézier-Stücks (Grad n = Punkte − 1) vom
/// Ende `from_start` aus, exakt aus den Kontrollpunkten.
fn bezier_end(q: &[DVec2], from_start: bool) -> (DVec2, DVec2) {
    let mut q = q.to_vec();
    if !from_start {
        q.reverse();
    }
    let n = (q.len() - 1) as f64;
    let (d1, d2) = (q[1] - q[0], q[2] - q[1]);
    let kappa = (n - 1.0) / n * d1.perp_dot(d2) / d1.length().powi(3);
    let t = d1.normalize();
    (t, kappa * t.perp())
}

fn positions(s: &Sketch, ids: &[PointId]) -> Vec<DVec2> {
    ids.iter().map(|&p| pos(s, p)).collect()
}

fn count(s: &Sketch, f: impl Fn(&K) -> bool) -> usize {
    s.constraints().filter(|(_, d)| f(&d.kind)).count()
}

fn smooths(s: &Sketch) -> usize {
    count(s, |k| matches!(k, K::Smooth(..)))
}

fn end_tangents(s: &Sketch) -> usize {
    count(s, |k| matches!(k, K::EndTangent(..)))
}

/// Zwei Linien (0,0)-(10,0) und (20,5)-(30,5); die Enden (10,0) und (20,5).
fn two_lines(s: &mut Sketch) -> (BlendEnd, BlendEnd) {
    let (a0, a1) = (pt(s, 0.0, 0.0), pt(s, 10.0, 0.0));
    let (b0, b1) = (pt(s, 20.0, 5.0), pt(s, 30.0, 5.0));
    let la = s.add_line(a0, a1).unwrap();
    let lb = s.add_line(b0, b1).unwrap();
    for (p, x, y) in [
        (a0, 0.0, 0.0),
        (a1, 10.0, 0.0),
        (b0, 20.0, 5.0),
        (b1, 30.0, 5.0),
    ] {
        s.add_constraint(K::Fixed(p, x, y)).unwrap();
    }
    (end(la, a1), end(lb, b0))
}

/// Erwartet die Ablehnung mit `message` und eine unveränderte Skizze.
fn refused_unchanged(s: &mut Sketch, a: BlendEnd, b: BlendEnd, kind: BlendKind, message: &str) {
    // Ruling F12: Weg ausdrücklich, damit der Bitgleich-Vergleich nicht an der Umgebung hängt.
    s.set_solver_path(SolverPath::Dense);
    let before = s.to_data();
    let curves = s.curves().count();
    let err = s.add_blend_curve(a, b, kind).unwrap_err();
    assert_eq!(err, SketchError::InvalidInput(message.to_string()));
    assert_eq!(s.to_data(), before, "Skizze nicht bitgleich");
    assert_eq!(s.curves().count(), curves);
}

#[test]
fn g2_blend_two_lines() {
    let mut s = sketch();
    let (a, b) = two_lines(&mut s);
    let c = s.add_blend_curve(a, b, BlendKind::G2).unwrap();
    let (ids, degree) = control(&s, c);
    assert_eq!((degree, ids.len()), (5, 6));
    assert_eq!(smooths(&s), 2);
    assert_eq!(end_tangents(&s), 0);
    assert_eq!(s.solve().status, SolveStatus::Solved);
    let q = positions(&s, &ids);
    // Von a weg nach +x, von b weg nach −x; die Linien sind gerade, die Krümmung 0.
    let (ta, ka) = bezier_end(&q, true);
    let (tb, kb) = bezier_end(&q, false);
    assert!(ta.perp_dot(DVec2::X).abs() <= 1e-9, "{ta}");
    assert!(tb.perp_dot(DVec2::X).abs() <= 1e-9, "{tb}");
    assert!(ka.length() <= 1e-9 && kb.length() <= 1e-9, "{ka} {kb}");
}

#[test]
fn g1_blend_line_arc() {
    let mut s = sketch();
    let (l0, l1) = (pt(&mut s, 0.0, 0.0), pt(&mut s, 10.0, 0.0));
    let line = s.add_line(l0, l1).unwrap();
    // Viertelbogen um (30, 0), Radius 5, von (35, 0) nach (30, 5).
    let (m, st, en) = (
        pt(&mut s, 30.0, 0.0),
        pt(&mut s, 35.0, 0.0),
        pt(&mut s, 30.0, 5.0),
    );
    let arc = s.add_arc(m, st, en).unwrap();
    for (p, x, y) in [
        (l0, 0.0, 0.0),
        (l1, 10.0, 0.0),
        (m, 30.0, 0.0),
        (st, 35.0, 0.0),
    ] {
        s.add_constraint(K::Fixed(p, x, y)).unwrap();
    }
    let c = s
        .add_blend_curve(end(line, l1), end(arc, en), BlendKind::G1)
        .unwrap();
    let (ids, degree) = control(&s, c);
    assert_eq!((degree, ids.len()), (3, 4));
    assert_eq!(end_tangents(&s), 2);
    assert_eq!(smooths(&s), 0);
    let q = positions(&s, &ids);
    let (ta, _) = bezier_end(&q, true);
    let (tb, _) = bezier_end(&q, false);
    assert!(ta.perp_dot(DVec2::X).abs() <= 1e-9, "{ta}");
    // Am Bogenende steht die Tangente senkrecht auf dem Radius.
    let radius = (pos(&s, en) - pos(&s, m)).normalize();
    assert!(tb.dot(radius).abs() <= 1e-9, "{tb} {radius}");
}

#[test]
fn blend_shares_end_points() {
    let mut s = sketch();
    let (a, b) = two_lines(&mut s);
    let points = s.points().count();
    let c = s.add_blend_curve(a, b, BlendKind::G2).unwrap();
    let (ids, _) = control(&s, c);
    assert_eq!(ids.first(), Some(&a.point));
    assert_eq!(ids.last(), Some(&b.point));
    // Nur die vier inneren Punkte sind neu, kein Deckungsgleich.
    assert_eq!(s.points().count(), points + 4);
    assert_eq!(count(&s, |k| matches!(k, K::Coincident(..))), 0);
}

#[test]
fn blend_elliptic_arc_g2() {
    let mut s = sketch();
    // Ellipsenbogen um (0, 0), a = 8, b = 4, von (8, 0) nach (0, 4). Am Start ist κ = a / b².
    let (m, st, en) = (
        pt(&mut s, 0.0, 0.0),
        pt(&mut s, 8.0, 0.0),
        pt(&mut s, 0.0, 4.0),
    );
    let arc = s
        .add_elliptic_arc(m, DVec2::new(8.0, 0.0), 4.0, st, en)
        .unwrap();
    let (l0, l1) = (pt(&mut s, 16.0, -10.0), pt(&mut s, 16.0, -20.0));
    let line = s.add_line(l0, l1).unwrap();
    for (p, x, y) in [
        (m, 0.0, 0.0),
        (st, 8.0, 0.0),
        (l0, 16.0, -10.0),
        (l1, 16.0, -20.0),
    ] {
        s.add_constraint(K::Fixed(p, x, y)).unwrap();
    }
    let c = s
        .add_blend_curve(end(arc, st), end(line, l0), BlendKind::G2)
        .unwrap();
    assert_eq!(smooths(&s), 2);
    assert_eq!(s.solve().status, SolveStatus::Solved);
    assert!(pos(&s, st).distance(DVec2::new(8.0, 0.0)) <= 1e-9);
    let (ids, _) = control(&s, c);
    let q = positions(&s, &ids);
    let (ta, ka) = bezier_end(&q, true);
    // Vom Start des Bogens weg nach −y; Krümmungsvektor zur Mitte hin, |κ| = 8 / 16.
    assert!(ta.distance(-DVec2::Y) <= 1e-9, "{ta}");
    assert!(ka.distance(DVec2::new(-0.5, 0.0)) <= 1e-6, "{ka}");
    let (_, kb) = bezier_end(&q, false);
    assert!(kb.length() <= 1e-6, "{kb}");
}

#[test]
fn blend_two_ends_of_same_curve() {
    let mut s = sketch();
    // Bogen 270° um (0, 0), r = 10, von (10, 0) nach (0, −10); die Lücke ist das vierte Viertel.
    let (m, st, en) = (
        pt(&mut s, 0.0, 0.0),
        pt(&mut s, 10.0, 0.0),
        pt(&mut s, 0.0, -10.0),
    );
    let arc = s.add_arc(m, st, en).unwrap();
    for (p, x, y) in [(m, 0.0, 0.0), (st, 10.0, 0.0), (en, 0.0, -10.0)] {
        s.add_constraint(K::Fixed(p, x, y)).unwrap();
    }
    let c = s
        .add_blend_curve(end(arc, st), end(arc, en), BlendKind::G2)
        .unwrap();
    assert_eq!(smooths(&s), 2);
    assert_eq!(s.solve().status, SolveStatus::Solved);
    let (ids, _) = control(&s, c);
    let q = positions(&s, &ids);
    // An beiden Fugen Krümmung 1/r zur Mitte hin.
    let (_, ka) = bezier_end(&q, true);
    let (_, kb) = bezier_end(&q, false);
    assert!(ka.distance(DVec2::new(-0.1, 0.0)) <= 1e-9, "{ka}");
    assert!(kb.distance(DVec2::new(0.0, 0.1)) <= 1e-9, "{kb}");
}

#[test]
fn blend_same_end_refused() {
    let mut s = sketch();
    let (a, _) = two_lines(&mut s);
    refused_unchanged(&mut s, a, a, BlendKind::G2, &tr!("sketch-blend-same-end"));
}

#[test]
fn blend_between_joined_ends_refused() {
    let mut s = sketch();
    // Prüf-Schwerpunkt 1: zwei Linien mit gemeinsamer Ecke, beide Enden dort gewählt.
    let (p, q, r) = (
        pt(&mut s, 0.0, 0.0),
        pt(&mut s, 10.0, 0.0),
        pt(&mut s, 10.0, 10.0),
    );
    let l1 = s.add_line(p, q).unwrap();
    let l2 = s.add_line(q, r).unwrap();
    let message = tr!("sketch-blend-ends-touch");
    assert!(message.contains("aufeinander"), "{message}");
    for kind in [BlendKind::G1, BlendKind::G2] {
        refused_unchanged(&mut s, end(l1, q), end(l2, q), kind, &message);
    }
    // Getrennte Punkte an derselben Stelle (deckungsgleich) liegen ebenso aufeinander.
    let (u, w) = (pt(&mut s, 50.0, 0.0), pt(&mut s, 60.0, 0.0));
    let (x, y) = (pt(&mut s, 60.0, 0.0), pt(&mut s, 60.0, 10.0));
    let l3 = s.add_line(u, w).unwrap();
    let l4 = s.add_line(x, y).unwrap();
    s.add_constraint(K::Coincident(w, x)).unwrap();
    refused_unchanged(&mut s, end(l3, w), end(l4, x), BlendKind::G2, &message);
}

#[test]
fn blend_from_3d_curve_refused() {
    let mut s = sketch();
    let a = s.add_point3(DVec3::new(0.0, 0.0, 0.0)).unwrap();
    let b = s.add_point3(DVec3::new(10.0, 0.0, 5.0)).unwrap();
    let line = s.add_line(a, b).unwrap();
    assert_eq!(s.blend_source(line), Err(BlendRefusal::ThreeD));
    assert_eq!(s.blend_end_near(line, DVec2::new(10.0, 0.0)), None);
}

/// Handtest 11a, Ruling H1-1: In einer 3D-Skizze ist jede Kurve eine 3D-Kurve, auch eine Linie
/// auf z = 0 (Fusion bietet die Übergangskurve nur in 2D-Skizzen an). Fällt der Haken, ist sie
/// wieder eine Quelle.
#[test]
fn blend_in_3d_sketch_refused_even_on_z0() {
    let mut s = sketch();
    let (p, q) = (pt(&mut s, 0.0, 0.0), pt(&mut s, 10.0, 0.0));
    let line = s.add_line(p, q).unwrap();
    s.set_three_d(true);
    assert_eq!(s.blend_source(line), Err(BlendRefusal::ThreeD));
    assert_eq!(s.blend_end_near(line, DVec2::new(10.0, 0.0)), None);
    s.set_three_d(false);
    assert_eq!(s.blend_source(line), Ok(()));
}

#[test]
fn blend_circle_refused() {
    let mut s = sketch();
    let m = pt(&mut s, 0.0, 0.0);
    let circle = s.add_circle(m, 5.0).unwrap();
    assert_eq!(s.blend_source(circle), Err(BlendRefusal::Closed));
    assert_eq!(s.blend_end_near(circle, DVec2::new(5.0, 0.0)), None);
    // Eine gelöschte Kurve ist keine Kurve.
    let (p, q) = (pt(&mut s, 0.0, 10.0), pt(&mut s, 10.0, 10.0));
    let line = s.add_line(p, q).unwrap();
    assert_eq!(s.blend_source(line), Ok(()));
    s.remove_curve(line).unwrap();
    assert_eq!(s.blend_source(line), Err(BlendRefusal::NotCurve));
}

/// Ruling F13: Ein nachweislich unlösbarer Fall. Quelle `a` ist ein Kontrollpunkt-Spline, dessen
/// Punkte alle fest sind; die ersten beiden sollen auf derselben Stelle liegen, stehen aber vor dem
/// Lösen noch getrennt (die Skizze ist lösbar, nur noch nicht gelöst). Die Anfangslage folgt der
/// jetzigen Richtung; beim Lösen fällt sie weg, und am Ende gibt es keine Richtung mehr, an die die
/// Übergangskurve anschließen könnte. Die Zeilen von „glatt“ und „Tangente am Ende“ lassen sich
/// dort nicht erfüllen.
fn unsolvable_sources(s: &mut Sketch) -> (BlendEnd, BlendEnd) {
    let q: Vec<PointId> = [(0.0, 0.0), (-5.0, 0.0), (-10.0, 5.0), (-15.0, 5.0)]
        .iter()
        .map(|&(x, y)| pt(s, x, y))
        .collect();
    let spline = s.add_control_spline(&q, 3).unwrap();
    for (p, (x, y)) in q
        .iter()
        .zip([(0.0, 0.0), (0.0, 0.0), (-10.0, 5.0), (-15.0, 5.0)])
    {
        s.add_constraint(K::Fixed(*p, x, y)).unwrap();
    }
    let (l0, l1) = (pt(s, 20.0, 5.0), pt(s, 30.0, 5.0));
    let line = s.add_line(l0, l1).unwrap();
    s.add_constraint(K::Fixed(l0, 20.0, 5.0)).unwrap();
    s.add_constraint(K::Fixed(l1, 30.0, 5.0)).unwrap();
    (end(spline, q[0]), end(line, l0))
}

/// Was `add_blend_curve` anlegt, hier ohne Rücknahme: Ist das Ergebnis schlechter als vorher?
fn worse_without_rollback(s: &Sketch, a: BlendEnd, b: BlendEnd, kind: BlendKind) -> bool {
    let before = s.clone().solve();
    let mut t = s.clone();
    let q = t.blend_control_points(a, b, kind).unwrap();
    let mut ids = vec![a.point];
    for p in &q[1..q.len() - 1] {
        ids.push(t.add_point(p.x, p.y).unwrap());
    }
    ids.push(b.point);
    let c = t.add_control_spline(&ids, kind.degree()).unwrap();
    for e in [a, b] {
        t.add_constraint(match kind {
            BlendKind::G1 => K::EndTangent(c, e.point, e.curve, e.point),
            BlendKind::G2 => K::Smooth(c, e.point, e.curve, e.point),
        })
        .unwrap();
    }
    t.solve().is_worse_than(&before)
}

#[test]
fn unsolvable_blend_leaves_nothing() {
    for kind in [BlendKind::G2, BlendKind::G1] {
        let mut s = sketch();
        let (a, b) = unsolvable_sources(&mut s);
        assert_eq!(
            s.clone().solve().status,
            SolveStatus::Solved,
            "Quellen lösbar"
        );
        // Nachweis: Ohne Rücknahme bliebe eine schlechter gelöste Skizze.
        assert!(
            worse_without_rollback(&s, a, b, kind),
            "{kind:?} wäre lösbar"
        );
        refused_unchanged(&mut s, a, b, kind, &tr!("sketch-blend-unsolvable"));
    }
}

#[test]
fn end_near_picks_closer_end() {
    let mut s = sketch();
    let (p, q) = (pt(&mut s, 0.0, 0.0), pt(&mut s, 10.0, 0.0));
    let line = s.add_line(p, q).unwrap();
    assert_eq!(
        s.blend_end_near(line, DVec2::new(8.0, 1.0)),
        Some(end(line, q))
    );
    assert_eq!(
        s.blend_end_near(line, DVec2::new(1.0, -2.0)),
        Some(end(line, p))
    );
}

#[test]
fn initial_inner_points_follow_tangents() {
    let mut s = sketch();
    let (a, b) = two_lines(&mut s);
    let (p0, p3) = (DVec2::new(10.0, 0.0), DVec2::new(20.0, 5.0));
    let h = p0.distance(p3) / 3.0;
    let q = s.blend_control_points(a, b, BlendKind::G1).unwrap();
    assert_eq!(q.len(), 4);
    assert_eq!((q[0], q[3]), (p0, p3));
    assert!(q[1].distance(p0 + h * DVec2::X) <= 1e-12, "{}", q[1]);
    assert!(q[2].distance(p3 - h * DVec2::X) <= 1e-12, "{}", q[2]);
    let preview = s.blend_preview(a, b, BlendKind::G1).unwrap();
    assert!(preview.len() > 2);
    assert_eq!(preview.first(), Some(&p0));
    assert!(preview.last().unwrap().distance(p3) <= 1e-12);
    // G2: dieselben Hebel, bei Linien liegt der zweite Punkt auf der Tangente.
    let q = s.blend_control_points(a, b, BlendKind::G2).unwrap();
    assert_eq!(q.len(), 6);
    assert!(q[1].distance(p0 + h * DVec2::X) <= 1e-12);
    assert!(q[2].distance(p0 + 2.0 * h * DVec2::X) <= 1e-12);
    assert!(q[3].distance(p3 - 2.0 * h * DVec2::X) <= 1e-12);
    assert!(q[4].distance(p3 - h * DVec2::X) <= 1e-12);
}

/// Sechs-Punkt-Formeln an einem Kurvenende (wie in `smooth_g2.rs`); `side` = +1 vom Anfang des
/// Bereichs aus, sonst −1.
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

/// Krümmung am Ende `p` in Richtung in die Kurve hinein.
fn kappa_into(s: &Sketch, c: CurveId, p: PointId) -> f64 {
    let (lo, hi) = s.curve_domain(c).unwrap();
    let q = pos(s, p);
    let first = s.curve_eval(c, lo).unwrap().distance(q) < s.curve_eval(c, hi).unwrap().distance(q);
    let (d, a) = ends_of(s, c, if first { 1.0 } else { -1.0 });
    let k = d.perp_dot(a) / d.length().powi(3);
    if first { k } else { -k }
}

fn handle_curvature(s: &Sketch, c: CurveId, at: PointId) -> Option<HandleCurvature> {
    let CurveKind::Spline { handles, .. } = &s.curve(c).unwrap().kind else {
        panic!("kein Spline");
    };
    handles.iter().find(|h| h.at == at).map(|h| h.curvature)
}

/// Prüfung I1: G2 an einem Spline mit Stützpunkten, Ende ohne Griff (`Natural`) und mit Griff
/// `Form`. Die Fuge bleibt G2, auch nachdem ein Stützpunkt der Quelle verschoben und neu gelöst
/// wurde (die Krümmungszeile muss also bestehen, nicht nur die Anfangslage stimmen).
#[test]
fn g2_blend_keeps_curvature_at_fit_spline_ends() {
    for form in [false, true] {
        let mut s = sketch();
        let q: Vec<PointId> = [(0.0, 0.0), (10.0, 6.0), (20.0, 0.0)]
            .iter()
            .map(|&(x, y)| pt(&mut s, x, y))
            .collect();
        let spline = s.add_spline(&q).unwrap();
        s.add_constraint(K::Fixed(q[0], 0.0, 0.0)).unwrap();
        if form {
            s.activate_tangent_handle(spline, q[2]).unwrap();
            assert_eq!(
                handle_curvature(&s, spline, q[2]),
                Some(HandleCurvature::Form)
            );
        }
        let (l0, l1) = (pt(&mut s, 40.0, -5.0), pt(&mut s, 50.0, -5.0));
        let line = s.add_line(l0, l1).unwrap();
        s.add_constraint(K::Fixed(l0, 40.0, -5.0)).unwrap();
        s.add_constraint(K::Fixed(l1, 50.0, -5.0)).unwrap();
        let c = s
            .add_blend_curve(end(spline, q[2]), end(line, l0), BlendKind::G2)
            .unwrap();
        let joint = |s: &Sketch| kappa_into(s, c, q[2]) + kappa_into(s, spline, q[2]);
        assert!(joint(&s).abs() <= 1e-6, "{form}: Anlage {}", joint(&s));
        // Stützpunkt der Quelle verschieben und neu lösen.
        s.set_point_xy(q[1], 9.0, 11.0).unwrap();
        let r = s.solve();
        assert_eq!(r.status, SolveStatus::Solved, "{form}: {r:?}");
        let (kb, ks) = (kappa_into(&s, c, q[2]), kappa_into(&s, spline, q[2]));
        assert!((kb + ks).abs() <= 1e-6, "{form}: κ {kb} und {ks}");
    }
}

/// Minor M2: Kurven eines Textes sind keine Quelle, auch nicht im Raum geprüft (Text vor 3D).
#[test]
fn blend_from_text_curve_refused() {
    let mut s = sketch();
    let p = |x: f64, y: f64| DVec2::new(x, y);
    let shape = TextShape {
        lines: vec![ShapedLine {
            glyphs: vec![Glyph {
                origin: p(0.0, 0.0),
                advance: 0.8,
                contours: vec![Contour {
                    start: p(0.0, 0.0),
                    segs: vec![
                        Seg::Line(p(0.6, 0.0)),
                        Seg::Line(p(0.6, 0.7)),
                        Seg::Line(p(0.0, 0.7)),
                        Seg::Line(p(0.0, 0.0)),
                    ],
                }],
            }],
            width: 0.8,
        }],
        ascender: 0.9,
        descender: -0.2,
        line_height: 1.2,
        substituted: None,
        synthetic: false,
        missing_chars: false,
    };
    let params = TextParams {
        content: "A".into(),
        style: TextStyle {
            font: "Test".into(),
            bold: false,
            italic: false,
            spacing: 0.0,
        },
        height: 5.0,
        flip_h: false,
        flip_v: false,
    };
    let t = s
        .add_text_box(
            p(0.0, 0.0),
            p(40.0, 10.0),
            params,
            HAlign::Left,
            VAlign::Bottom,
            Arc::new(shape),
        )
        .unwrap();
    let curves = s.text(t).unwrap().curves.clone();
    assert!(!curves.is_empty());
    for c in curves {
        assert_eq!(s.blend_source(c), Err(BlendRefusal::Text));
        assert_eq!(s.blend_end_near(c, DVec2::ZERO), None);
    }
}
