//! Geometrie des Versatzes (Skizzen 4, Task 3): reiner Plan, die Skizze wird nur gelesen.
//! Geprüft wird über dichte Wertefolgen. Die Kreuzungsprüfung ist bewusst eine eigene Rechnung
//! auf feinen Linienzügen, damit sie nicht dieselben Schnittfunktionen benutzt wie der Plan.

use planora_math::{DVec2, Frame, TAU, Tolerance};
use planora_sketch::model::{CurveId, Sketch};
use planora_sketch::offset::{
    OffsetGeom, OffsetLoop, OffsetPiece, OffsetPlan, OffsetProblem, OrderedChain,
};

fn sketch() -> Sketch {
    Sketch::new(Frame::WORLD, "S", Tolerance::default())
}

fn v(x: f64, y: f64) -> DVec2 {
    DVec2::new(x, y)
}

fn line(s: &mut Sketch, x0: f64, y0: f64, x1: f64, y1: f64) -> CurveId {
    let a = s.add_point(x0, y0).unwrap();
    let b = s.add_point(x1, y1).unwrap();
    s.add_line(a, b).unwrap()
}

/// Geschlossener Linienzug mit gemeinsamen Punkten.
fn polygon(s: &mut Sketch, pts: &[(f64, f64)]) -> Vec<CurveId> {
    let ids: Vec<_> = pts.iter().map(|p| s.add_point(p.0, p.1).unwrap()).collect();
    (0..ids.len())
        .map(|i| s.add_line(ids[i], ids[(i + 1) % ids.len()]).unwrap())
        .collect()
}

fn open_polyline(s: &mut Sketch, pts: &[(f64, f64)]) -> Vec<CurveId> {
    let ids: Vec<_> = pts.iter().map(|p| s.add_point(p.0, p.1).unwrap()).collect();
    (0..ids.len() - 1)
        .map(|i| s.add_line(ids[i], ids[i + 1]).unwrap())
        .collect()
}

const SQUARE: [(f64, f64); 4] = [(0.0, 0.0), (100.0, 0.0), (100.0, 100.0), (0.0, 100.0)];
/// Schenkel 20 breit.
const L_FORM: [(f64, f64); 6] = [
    (0.0, 0.0),
    (60.0, 0.0),
    (60.0, 20.0),
    (20.0, 20.0),
    (20.0, 60.0),
    (0.0, 60.0),
];
/// Zwei Quadrate 40×40, Steg 20 lang und 6 hoch.
const DUMBBELL: [(f64, f64); 12] = [
    (0.0, 0.0),
    (40.0, 0.0),
    (40.0, 17.0),
    (60.0, 17.0),
    (60.0, 0.0),
    (100.0, 0.0),
    (100.0, 40.0),
    (60.0, 40.0),
    (60.0, 23.0),
    (40.0, 23.0),
    (40.0, 40.0),
    (0.0, 40.0),
];
/// Schlitz 20 breit, Schenkel 40 breit, Boden 30 hoch.
const U_FORM: [(f64, f64); 8] = [
    (0.0, 0.0),
    (100.0, 0.0),
    (100.0, 80.0),
    (60.0, 80.0),
    (60.0, 30.0),
    (40.0, 30.0),
    (40.0, 80.0),
    (0.0, 80.0),
];
/// Schlitz 20 breit, Schenkel nur 10 breit, Boden 40 hoch.
const U_THIN: [(f64, f64); 8] = [
    (0.0, 0.0),
    (40.0, 0.0),
    (40.0, 80.0),
    (30.0, 80.0),
    (30.0, 40.0),
    (10.0, 40.0),
    (10.0, 80.0),
    (0.0, 80.0),
];

/// Stern mit fünf Spitzen, gegen den Uhrzeigersinn.
fn star_points() -> Vec<(f64, f64)> {
    (0..10)
        .map(|k| {
            let r = if k % 2 == 0 { 50.0 } else { 20.0 };
            let a = TAU * k as f64 / 10.0;
            (r * a.cos(), r * a.sin())
        })
        .collect()
}

/// Abgerundetes Rechteck gegen den Uhrzeigersinn: Linie, Bogen, Linie, Bogen …
fn rounded_rect(s: &mut Sketch, w: f64, h: f64, r: f64) -> Vec<CurveId> {
    let pts = [
        (r, 0.0),
        (w - r, 0.0),
        (w, r),
        (w, h - r),
        (w - r, h),
        (r, h),
        (0.0, h - r),
        (0.0, r),
    ];
    let ids: Vec<_> = pts.iter().map(|p| s.add_point(p.0, p.1).unwrap()).collect();
    let centers = [(w - r, r), (w - r, h - r), (r, h - r), (r, r)];
    let mut out = Vec::new();
    for k in 0..4 {
        out.push(s.add_line(ids[2 * k], ids[2 * k + 1]).unwrap());
        let c = s.add_point(centers[k].0, centers[k].1).unwrap();
        out.push(s.add_arc(c, ids[2 * k + 1], ids[(2 * k + 2) % 8]).unwrap());
    }
    out
}

fn chain_of(s: &Sketch, curves: &[CurveId]) -> Vec<OrderedChain> {
    s.ordered_chains(curves).unwrap()
}

fn plan_of(s: &Sketch, curves: &[CurveId], d: f64) -> Result<OffsetPlan, OffsetProblem> {
    s.offset_plan(&chain_of(s, curves), d)
}

/// Fein abgetasteter Linienzug eines Stücks.
fn polyline(p: &OffsetPiece) -> Vec<DVec2> {
    match &p.geom {
        OffsetGeom::Line { a, b } => vec![*a, *b],
        OffsetGeom::Arc(arc) => {
            let (r, a0, sweep) = (
                arc.radius(),
                (arc.start - arc.center).to_angle(),
                arc.sweep(),
            );
            (0..=48)
                .map(|k| arc.center + DVec2::from_angle(a0 + sweep * k as f64 / 48.0) * r)
                .collect()
        }
        OffsetGeom::Circle { center, radius } => (0..=96)
            .map(|k| *center + DVec2::from_angle(TAU * k as f64 / 96.0) * *radius)
            .collect(),
        OffsetGeom::Approx { points } => points.clone(),
    }
}

/// Mittelpunkt eines Stücks; bei `Approx` eine exakt gerechnete Stützstelle.
fn midpoint(p: &OffsetPiece) -> DVec2 {
    match &p.geom {
        OffsetGeom::Line { a, b } => (*a + *b) * 0.5,
        OffsetGeom::Arc(arc) => {
            arc.center
                + DVec2::from_angle((arc.start - arc.center).to_angle() + arc.sweep() * 0.5)
                    * arc.radius()
        }
        OffsetGeom::Circle { center, radius } => *center + v(*radius, 0.0),
        OffsetGeom::Approx { points } => points[points.len() / 2],
    }
}

fn segments_cross(p: DVec2, q: DVec2, r: DVec2, s: DVec2) -> bool {
    let side = |a: DVec2, b: DVec2, c: DVec2| (b - a).perp_dot(c - a);
    let (d1, d2, d3, d4) = (side(r, s, p), side(r, s, q), side(p, q, r), side(p, q, s));
    const E: f64 = 1e-7;
    ((d1 > E && d2 < -E) || (d1 < -E && d2 > E)) && ((d3 > E && d4 < -E) || (d3 < -E && d4 > E))
}

/// Zahl der echten Kreuzungen zwischen allen Stücken (gemeinsame Enden zählen nicht).
fn crossings(plan: &OffsetPlan) -> usize {
    let polys: Vec<Vec<DVec2>> = plan
        .loops
        .iter()
        .flat_map(|l| l.pieces.iter().map(polyline))
        .collect();
    let mut n = 0;
    for i in 0..polys.len() {
        for j in i + 1..polys.len() {
            for a in polys[i].windows(2) {
                for b in polys[j].windows(2) {
                    if segments_cross(a[0], a[1], b[0], b[1]) {
                        n += 1;
                    }
                }
            }
        }
    }
    n
}

/// Fläche aller Schleifen (Betrag), aus den Linienzügen des Plans.
fn area(plan: &OffsetPlan) -> f64 {
    plan.outline(256)
        .iter()
        .map(|pts| {
            pts.windows(2)
                .map(|w| w[0].perp_dot(w[1]))
                .sum::<f64>()
                .abs()
                / 2.0
        })
        .sum()
}

/// Genauer Abstand zur Kurve in ihrem Bereich: Abtasten, dann Goldener Schnitt
/// (`distance_to_curve` misst nur auf dem Linienzug).
fn exact_distance(s: &Sketch, c: CurveId, p: DVec2) -> f64 {
    const R: f64 = 0.618_033_988_749_894_8;
    const STEPS: usize = 512;
    let (lo, hi) = s.curve_domain(c).unwrap();
    let at = |t: f64| s.curve_eval(c, t).unwrap().distance(p);
    let step = (hi - lo) / STEPS as f64;
    let best = (0..=STEPS)
        .min_by(|i, j| at(lo + step * *i as f64).total_cmp(&at(lo + step * *j as f64)))
        .unwrap();
    let mid = lo + step * best as f64;
    let (mut a, mut b) = ((mid - step).max(lo), (mid + step).min(hi));
    for _ in 0..60 {
        let (x, y) = (b - R * (b - a), a + R * (b - a));
        if at(x) <= at(y) {
            b = y;
        } else {
            a = x;
        }
    }
    at(0.5 * (a + b)).min(at(mid))
}

fn distance_to_all(s: &Sketch, curves: &[CurveId], p: DVec2) -> f64 {
    curves
        .iter()
        .map(|c| exact_distance(s, *c, p))
        .fold(f64::INFINITY, f64::min)
}

fn end_points(p: &OffsetPiece) -> [DVec2; 2] {
    match &p.geom {
        OffsetGeom::Line { a, b } => [*a, *b],
        OffsetGeom::Arc(arc) => [arc.start, arc.end],
        OffsetGeom::Circle { center, radius } => {
            let q = *center + v(*radius, 0.0);
            [q, q]
        }
        OffsetGeom::Approx { points } => [points[0], points[points.len() - 1]],
    }
}

/// Jede Fuge einer geschlossenen Schleife: gemeinsamer Punkt, bitgleich.
fn assert_joints_bit_equal(l: &OffsetLoop, label: &str) {
    let n = l.pieces.len();
    if n < 2 {
        return;
    }
    let shares = |x: &[DVec2; 2], y: &[DVec2; 2]| x.iter().any(|p| y.iter().any(|q| p == q));
    for i in 0..n {
        let (a, b) = (end_points(&l.pieces[i]), end_points(&l.pieces[(i + 1) % n]));
        assert!(shares(&a, &b), "{label}: Fuge {i} {a:?} {b:?}");
    }
}

/// Geschlossen, bitgleiche Fugen, keine Kreuzung, und jedes Stück liegt auf dem Träger seiner
/// Vorlage im Abstand `|d|`, nirgends näher an irgendeiner Vorlagekurve.
fn assert_clean_closed(s: &Sketch, curves: &[CurveId], plan: &OffsetPlan, d: f64, label: &str) {
    assert!(plan.is_closed_like_source(), "{label} d={d}");
    assert!(!plan.loops.is_empty(), "{label} d={d}");
    for l in &plan.loops {
        assert!(l.closed, "{label} d={d}: offen");
        assert_joints_bit_equal(l, label);
    }
    assert_eq!(crossings(plan), 0, "{label} d={d}: Kreuzung {plan:?}");
    for l in &plan.loops {
        for p in &l.pieces {
            let m = midpoint(p);
            assert!(m.is_finite(), "{label} d={d}: NaN");
            let on_carrier = s.nearest_on_curve(p.source, m).unwrap().distance(m);
            assert!(
                (on_carrier - d.abs()).abs() < 1e-6,
                "{label} d={d}: Mitte {m}, Träger {on_carrier}"
            );
            let dist = distance_to_all(s, curves, m);
            assert!(dist > d.abs() - 1e-6, "{label} d={d}: Mitte {m} bei {dist}");
        }
    }
}

/// Dazu: Die Stückmitte hat genau `|d|` Abstand zur Vorlage. Das gilt nur, solange sie über der
/// Kante liegt, die Gehrung schiebt sie sonst über das Ende hinaus (siehe `sweep`).
fn assert_good_closed(s: &Sketch, curves: &[CurveId], plan: &OffsetPlan, d: f64, label: &str) {
    assert_clean_closed(s, curves, plan, d, label);
    for l in &plan.loops {
        for p in &l.pieces {
            let m = midpoint(p);
            let dist = distance_to_all(s, curves, m);
            assert!(
                (dist - d.abs()).abs() < 1e-6,
                "{label} d={d}: Mitte {m} hat Abstand {dist}"
            );
        }
    }
}

/// Dichte Folge: 200 Schritte von 0,1 bis `limit`. Die Vorlagen laufen gegen den Uhrzeigersinn,
/// außen ist also rechts, negativ. Bis `exact_to` muss die Stückmitte genau `|d|` von der Vorlage
/// liegen; dahinter liegt sie über der Gehrung einer Ecke, also weiter weg, und nur der Träger
/// und „nirgends näher“ gelten.
fn sweep(s: &Sketch, curves: &[CurveId], limit: f64, exact_to: f64, outward: bool, label: &str) {
    let sign = if outward { -1.0 } else { 1.0 };
    let mut last: Option<f64> = None;
    let base = area_of_source(s, curves);
    for k in 0..200 {
        let d = 0.1 + (limit - 0.1) * k as f64 / 199.0;
        let plan = plan_of(s, curves, sign * d)
            .unwrap_or_else(|e| panic!("{label} außen={outward} d={d}: {e:?}"));
        if d <= exact_to {
            assert_good_closed(s, curves, &plan, sign * d, label);
        } else {
            assert_clean_closed(s, curves, &plan, sign * d, label);
        }
        let a = area(&plan);
        if outward {
            assert!(
                a > base,
                "{label} d={d}: Fläche {a} nicht größer als {base}"
            );
        } else {
            assert!(
                a < base,
                "{label} d={d}: Fläche {a} nicht kleiner als {base}"
            );
        }
        if let Some(prev) = last {
            if outward {
                assert!(a > prev, "{label} d={d}: Fläche {a} nach {prev}");
            } else {
                assert!(a < prev, "{label} d={d}: Fläche {a} nach {prev}");
            }
        }
        last = Some(a);
    }
}

fn area_of_source(s: &Sketch, curves: &[CurveId]) -> f64 {
    let chains = chain_of(s, curves);
    let mut pts: Vec<DVec2> = Vec::new();
    for (c, rev) in &chains[0].items {
        let mut pl = s.curve_polyline(*c, 256).unwrap();
        if *rev {
            pl.reverse();
        }
        pts.extend(pl);
    }
    let n = pts.len();
    (0..n)
        .map(|i| pts[i].perp_dot(pts[(i + 1) % n]))
        .sum::<f64>()
        .abs()
        / 2.0
}

// ---------------------------------------------------------------------------------------------
// Einzelne Kurven
// ---------------------------------------------------------------------------------------------

#[test]
fn a_line_moves_to_the_left() {
    let mut s = sketch();
    let l = line(&mut s, 0.0, 0.0, 30.0, 0.0);
    let plan = plan_of(&s, &[l], 5.0).unwrap();
    assert_eq!(plan.piece_count(), 1);
    assert_eq!(plan.distance, 5.0);
    assert_eq!(plan.dropped, 0);
    assert!(!plan.loops[0].closed);
    assert!(plan.is_closed_like_source());
    assert_eq!(
        plan.loops[0].pieces[0],
        OffsetPiece {
            source: l,
            geom: OffsetGeom::Line {
                a: v(0.0, 5.0),
                b: v(30.0, 5.0)
            }
        }
    );
}

#[test]
fn an_arc_shrinks_on_its_left_and_grows_on_its_right() {
    let mut s = sketch();
    let c = s.add_point(0.0, 0.0).unwrap();
    let a = s.add_point(20.0, 0.0).unwrap();
    let b = s.add_point(0.0, 20.0).unwrap();
    let arc = s.add_arc(c, a, b).unwrap();
    let radius = |d: f64| match &plan_of(&s, &[arc], d).unwrap().loops[0].pieces[0].geom {
        OffsetGeom::Arc(p) => {
            assert!(p.center.distance(DVec2::ZERO) < 1e-12);
            assert!((p.sweep() - TAU / 4.0).abs() < 1e-12);
            assert!((p.start - p.center).distance(v(p.radius(), 0.0)) < 1e-9);
            p.radius()
        }
        g => panic!("{g:?}"),
    };
    assert!((radius(5.0) - 15.0).abs() < 1e-12);
    assert!((radius(-5.0) - 25.0).abs() < 1e-12);
    // gegen die Kurvenrichtung gelaufen: links ist außen
    let back = OrderedChain {
        items: vec![(arc, true)],
        closed: false,
    };
    let plan = s.offset_plan(&[back], 5.0).unwrap();
    match &plan.loops[0].pieces[0].geom {
        OffsetGeom::Arc(p) => assert!((p.radius() - 25.0).abs() < 1e-12),
        g => panic!("{g:?}"),
    }
    // so weit nach innen, dass der Bogen verschwindet
    assert_eq!(plan_of(&s, &[arc], 20.0), Err(OffsetProblem::Vanished));
    assert_eq!(plan_of(&s, &[arc], 25.0), Err(OffsetProblem::Vanished));
}

#[test]
fn a_circle_has_its_inside_on_the_left() {
    let mut s = sketch();
    let c = s.add_point(3.0, 4.0).unwrap();
    let k = s.add_circle(c, 20.0).unwrap();
    let chains = chain_of(&s, &[k]);
    assert!(chains[0].closed);
    let plan = s.offset_plan(&chains, 5.0).unwrap();
    assert_eq!(
        plan.loops[0].pieces[0].geom,
        OffsetGeom::Circle {
            center: v(3.0, 4.0),
            radius: 15.0
        }
    );
    assert!(plan.loops[0].closed);
    let plan = s.offset_plan(&chains, -5.0).unwrap();
    assert_eq!(
        plan.loops[0].pieces[0].geom,
        OffsetGeom::Circle {
            center: v(3.0, 4.0),
            radius: 25.0
        }
    );
    assert_eq!(s.offset_plan(&chains, 20.0), Err(OffsetProblem::Vanished));
    for d in [0.0, f64::NAN, f64::INFINITY] {
        assert_eq!(s.offset_plan(&chains, d), Err(OffsetProblem::Degenerate));
    }
    assert_eq!(s.offset_plan(&[], 1.0), Err(OffsetProblem::Degenerate));
}

#[test]
fn a_square_outward_keeps_four_lines_with_mitred_corners() {
    let mut s = sketch();
    let q = polygon(&mut s, &SQUARE);
    let plan = plan_of(&s, &q, -10.0).unwrap();
    assert_eq!(plan.loops.len(), 1);
    assert!(plan.loops[0].closed);
    assert_eq!(plan.piece_count(), 4);
    let want = [
        (v(-10.0, -10.0), v(110.0, -10.0)),
        (v(110.0, -10.0), v(110.0, 110.0)),
        (v(110.0, 110.0), v(-10.0, 110.0)),
        (v(-10.0, 110.0), v(-10.0, -10.0)),
    ];
    for (i, p) in plan.loops[0].pieces.iter().enumerate() {
        assert_eq!(p.source, q[i]);
        match p.geom {
            OffsetGeom::Line { a, b } => {
                assert!(a.distance(want[i].0) < 1e-9 && b.distance(want[i].1) < 1e-9);
                assert!((a.distance(b) - 120.0).abs() < 1e-9);
            }
            ref g => panic!("{g:?}"),
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Tangentiale Kette
// ---------------------------------------------------------------------------------------------

#[test]
fn a_rounded_rectangle_keeps_its_arcs_with_exact_joints() {
    let mut s = sketch();
    let r = rounded_rect(&mut s, 100.0, 60.0, 10.0);
    for (d, want) in [(-5.0, 15.0), (5.0, 5.0)] {
        let plan = plan_of(&s, &r, d).unwrap();
        assert_eq!(plan.piece_count(), 8);
        assert_eq!(plan.dropped, 0);
        let l = &plan.loops[0];
        assert!(l.closed);
        assert_joints_bit_equal(l, "Rundrechteck");
        for p in &l.pieces {
            if let OffsetGeom::Arc(a) = &p.geom {
                assert!((a.radius() - want).abs() < 1e-9);
                assert!(((a.end - a.center).length() - want).abs() < 1e-9);
                assert!((a.sweep() - TAU / 4.0).abs() < 1e-9);
            }
        }
    }
}

#[test]
fn inward_past_the_radius_the_arcs_drop_and_the_lines_meet_in_corners() {
    let mut s = sketch();
    let r = rounded_rect(&mut s, 100.0, 60.0, 10.0);
    for d in [10.0, 12.0, 25.0] {
        let plan = plan_of(&s, &r, d).unwrap();
        assert_eq!(plan.dropped, 4, "d={d}");
        assert_eq!(plan.piece_count(), 4, "d={d}");
        assert!(plan.loops[0].closed);
        assert_eq!(crossings(&plan), 0);
        let (w, h) = (100.0 - 2.0 * d, 60.0 - 2.0 * d);
        assert!((area(&plan) - w * h).abs() < 1e-6, "d={d}");
        for p in &plan.loops[0].pieces {
            assert!(matches!(p.geom, OffsetGeom::Line { .. }));
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Dichte Folgen
// ---------------------------------------------------------------------------------------------

#[test]
fn dense_sweep_square() {
    let mut s = sketch();
    let q = polygon(&mut s, &SQUARE);
    sweep(&s, &q, 49.9, 49.9, false, "Quadrat");
    sweep(&s, &q, 200.0, 200.0, true, "Quadrat");
}

#[test]
fn dense_sweep_l_form() {
    let mut s = sketch();
    let q = polygon(&mut s, &L_FORM);
    sweep(&s, &q, 9.9, 9.9, false, "L-Form");
    sweep(&s, &q, 60.0, 19.0, true, "L-Form");
}

#[test]
fn dense_sweep_star() {
    let mut s = sketch();
    let pts = star_points();
    let q = polygon(&mut s, &pts);
    // Innen bis kurz vor den Inkreis, den alle Kanten gleichzeitig erreichen.
    let (a, b) = (v(pts[0].0, pts[0].1), v(pts[1].0, pts[1].1));
    let inradius = (b - a).normalize().perp_dot(-a).abs();
    sweep(&s, &q, inradius * 0.99, 10.0, false, "Stern");
    // Außen wächst die Gehrung der Spitzen schneller als die Kante.
    sweep(&s, &q, 50.0, 8.0, true, "Stern");
}

#[test]
fn dense_sweep_rounded_rectangle() {
    let mut s = sketch();
    let r = rounded_rect(&mut s, 100.0, 60.0, 10.0);
    sweep(&s, &r, 29.9, 29.9, false, "Rundrechteck");
    sweep(&s, &r, 100.0, 100.0, true, "Rundrechteck");
}

// ---------------------------------------------------------------------------------------------
// Trimmen
// ---------------------------------------------------------------------------------------------

#[test]
fn the_square_vanishes_at_half_its_width() {
    let mut s = sketch();
    let q = polygon(&mut s, &SQUARE);
    assert_eq!(plan_of(&s, &q, 50.0), Err(OffsetProblem::Vanished));
    assert_eq!(plan_of(&s, &q, 80.0), Err(OffsetProblem::Vanished));
    assert_eq!(plan_of(&s, &q, 500.0), Err(OffsetProblem::Vanished));
}

#[test]
fn an_l_form_past_its_legs_vanishes_with_nothing_left_over() {
    let mut s = sketch();
    let q = polygon(&mut s, &L_FORM);
    for k in 0..200 {
        let d = 10.0 + 40.0 * k as f64 / 199.0;
        assert_eq!(plan_of(&s, &q, d), Err(OffsetProblem::Vanished), "d={d}");
    }
    // Ein kurzes, schmales Bein auf breitem Körper: Der Überstand entfällt, der Körper bleibt.
    let mut s = sketch();
    let q = polygon(
        &mut s,
        &[
            (0.0, 0.0),
            (60.0, 0.0),
            (60.0, 40.0),
            (20.0, 40.0),
            (20.0, 50.0),
            (0.0, 50.0),
        ],
    );
    let plan = plan_of(&s, &q, 12.0).unwrap();
    assert_good_closed(&s, &q, &plan, 12.0, "L mit Bein");
    assert_eq!(plan.loops.len(), 1);
    assert!(plan.dropped >= 1, "{plan:?}");
    assert!((area(&plan) - 36.0 * 16.0).abs() < 1e-6, "{}", area(&plan));
}

#[test]
fn a_dumbbell_splits_into_two_loops_when_the_bridge_vanishes() {
    let mut s = sketch();
    let q = polygon(&mut s, &DUMBBELL);
    let loops_at = |d: f64| plan_of(&s, &q, d).unwrap().loops.len();
    assert_eq!(loops_at(2.9), 1);
    assert_eq!(loops_at(3.0), 2, "Steg genau weg");
    assert_eq!(loops_at(5.0), 2);
    assert_eq!(loops_at(19.0), 2);
    let plan = plan_of(&s, &q, 5.0).unwrap();
    assert_good_closed(&s, &q, &plan, 5.0, "Hantel");
    assert!(
        (area(&plan) - 2.0 * 30.0 * 30.0).abs() < 1e-6,
        "{}",
        area(&plan)
    );
    sweep(&s, &q, 19.9, 8.5, false, "Hantel");
    // Außen wachsen die Quadrate zusammen: Die Lücken über und unter dem Steg schließen sich.
    sweep(&s, &q, 60.0, 8.0, true, "Hantel");
}

#[test]
fn a_u_form_stays_closed_past_half_the_slot() {
    let mut s = sketch();
    let q = polygon(&mut s, &U_FORM);
    // innen: Der Schlitz wird breiter, die Schenkel bleiben bis 15 verbunden
    sweep(&s, &q, 14.9, 14.9, false, "U innen");
    for d in [10.0, 12.0, 14.0] {
        assert_eq!(plan_of(&s, &q, d).unwrap().loops.len(), 1, "d={d}");
    }
    // der Boden verschwindet: zwei Schenkel
    let plan = plan_of(&s, &q, 17.0).unwrap();
    assert_eq!(plan.loops.len(), 2);
    assert_good_closed(&s, &q, &plan, 17.0, "U Boden weg");
    // außen: Der Schlitz schließt sich bei 10, die Oberkante läuft dann durch
    sweep(&s, &q, 60.0, 30.0, true, "U außen");
    let plan = plan_of(&s, &q, -12.0).unwrap();
    assert_eq!(plan.loops.len(), 1);
    assert!(
        (area(&plan) - 124.0 * 104.0).abs() < 1e-6,
        "{}",
        area(&plan)
    );
}

#[test]
fn a_u_form_with_thin_legs_loses_them_and_stays_closed() {
    let mut s = sketch();
    let q = polygon(&mut s, &U_THIN);
    // Schenkel weg bei 5, über die halbe Schlitzbreite (10) bleibt der Boden
    sweep(&s, &q, 4.9, 4.9, false, "U dünn");
    for k in 0..200 {
        let d = 5.0 + 14.9 * k as f64 / 199.0;
        let plan = plan_of(&s, &q, d).unwrap_or_else(|e| panic!("d={d}: {e:?}"));
        assert_eq!(plan.loops.len(), 1, "d={d}");
        assert_good_closed(&s, &q, &plan, d, "U dünn, Schenkel weg");
        let want = (40.0 - 2.0 * d) * (40.0 - 2.0 * d);
        assert!((area(&plan) - want).abs() < 1e-6, "d={d}: {}", area(&plan));
    }
}

#[test]
fn a_notch_closes_from_outside_and_the_wall_runs_through() {
    let mut s = sketch();
    let q = polygon(
        &mut s,
        &[
            (0.0, 0.0),
            (100.0, 0.0),
            (100.0, 100.0),
            (0.0, 100.0),
            (0.0, 55.0),
            (40.0, 50.0),
            (0.0, 45.0),
        ],
    );
    // Die Kerbe ist von außen gesehen bei 12 geschlossen: Die linke Wand läuft durch.
    let plan = plan_of(&s, &q, -12.0).unwrap();
    assert_clean_closed(&s, &q, &plan, -12.0, "Kerbe");
    assert_eq!(plan.dropped, 2, "{plan:?}");
    assert!(
        (area(&plan) - 124.0 * 124.0).abs() < 1e-6,
        "{}",
        area(&plan)
    );
}

#[test]
fn a_comb_loses_its_teeth_and_then_vanishes() {
    // Sockel 92×15, Zinken 8 breit
    let mut pts = vec![(0.0, 0.0), (92.0, 0.0)];
    for k in (0..7).rev() {
        let x = k as f64 * 14.0;
        if k < 6 {
            pts.push((x + 8.0, 15.0));
        }
        pts.push((x + 8.0, 55.0));
        pts.push((x, 55.0));
        if k > 0 {
            pts.push((x, 15.0));
        }
    }
    let mut s = sketch();
    let q = polygon(&mut s, &pts);
    sweep(&s, &q, 3.9, 3.9, false, "Kamm");
    let plan = plan_of(&s, &q, 5.0).unwrap();
    assert_clean_closed(&s, &q, &plan, 5.0, "Kamm ohne Zinken");
    assert_eq!(plan.loops.len(), 1);
    assert!((area(&plan) - 82.0 * 5.0).abs() < 1e-6, "{}", area(&plan));
    // Genau bei 7,5 berühren sich die Ränder im Sockel: gemeldet, nie ein falsches Profil.
    assert!(plan_of(&s, &q, 7.5).is_err());
    for k in 0..200 {
        let d = 7.51 + 80.0 * k as f64 / 199.0;
        assert_eq!(plan_of(&s, &q, d), Err(OffsetProblem::Vanished), "d={d}");
    }
}

/// L-Form, die innere Ecke mit Radius 6 gerundet: Der Bogen läuft gegen seine Richtung.
fn rounded_inner_l(s: &mut Sketch) -> Vec<CurveId> {
    let p: Vec<_> = [
        (0.0, 0.0),
        (60.0, 0.0),
        (60.0, 20.0),
        (26.0, 20.0),
        (20.0, 26.0),
        (20.0, 60.0),
        (0.0, 60.0),
    ]
    .iter()
    .map(|q| s.add_point(q.0, q.1).unwrap())
    .collect();
    let c = s.add_point(26.0, 26.0).unwrap();
    vec![
        s.add_line(p[0], p[1]).unwrap(),
        s.add_line(p[1], p[2]).unwrap(),
        s.add_line(p[2], p[3]).unwrap(),
        s.add_arc(c, p[4], p[3]).unwrap(),
        s.add_line(p[4], p[5]).unwrap(),
        s.add_line(p[5], p[6]).unwrap(),
        s.add_line(p[6], p[0]).unwrap(),
    ]
}

#[test]
fn a_concave_fillet_grows_inward_and_vanishes_outward() {
    let mut s = sketch();
    let q = rounded_inner_l(&mut s);
    let chain = chain_of(&s, &q).remove(0);
    assert!(chain.items.iter().any(|(c, rev)| *c == q[3] && *rev));
    sweep(&s, &q, 9.9, 9.9, false, "L rund");
    sweep(&s, &q, 40.0, 19.0, true, "L rund");
    let radius_at = |d: f64| {
        let plan = s.offset_plan(std::slice::from_ref(&chain), d).unwrap();
        plan.loops[0].pieces.iter().find_map(|p| match &p.geom {
            OffsetGeom::Arc(a) => Some(a.radius()),
            _ => None,
        })
    };
    assert!((radius_at(5.0).unwrap() - 11.0).abs() < 1e-9);
    assert!((radius_at(-5.0).unwrap() - 1.0).abs() < 1e-9);
    assert_eq!(radius_at(-6.0), None);
}

#[test]
fn a_chain_run_backwards_gives_the_same_shape_with_swapped_signs() {
    for build in [
        (|s: &mut Sketch| rounded_rect(s, 100.0, 60.0, 10.0)) as fn(&mut Sketch) -> Vec<CurveId>,
        rounded_inner_l,
    ] {
        let mut s = sketch();
        let q = build(&mut s);
        let chain = chain_of(&s, &q).remove(0);
        let back = OrderedChain {
            items: chain.items.iter().rev().map(|(c, r)| (*c, !*r)).collect(),
            closed: true,
        };
        for k in 0..200 {
            let d = 0.1 + 9.7 * k as f64 / 199.0;
            for side in [d, -d] {
                let there = s
                    .offset_plan(std::slice::from_ref(&chain), side)
                    .unwrap_or_else(|e| panic!("{side}: {e:?}"));
                let here = s
                    .offset_plan(std::slice::from_ref(&back), -side)
                    .unwrap_or_else(|e| panic!("{side}: {e:?}"));
                assert!(here.is_closed_like_source());
                assert_eq!(here.piece_count(), there.piece_count(), "{side}");
                // Die Abtastung der Bögen darf um einen Abschnitt verschieden sein.
                assert!((area(&here) - area(&there)).abs() < 1e-2, "{side}");
                assert_eq!(crossings(&here), 0, "{side}");
            }
        }
    }
}

#[test]
fn an_arc_its_neighbour_no_longer_reaches_drops() {
    // Linie und nach außen gewölbter Bogen im Wechsel mit scharfen Fugen
    let mut s = sketch();
    let n = 16;
    let pts: Vec<_> = (0..n)
        .map(|k| {
            let a = TAU * k as f64 / n as f64;
            s.add_point(60.0 * a.cos(), 60.0 * a.sin()).unwrap()
        })
        .collect();
    let mut q = Vec::new();
    for k in 0..n {
        let (a, b) = (
            s.point_xy(pts[k]).unwrap(),
            s.point_xy(pts[(k + 1) % n]).unwrap(),
        );
        if k % 2 == 1 {
            q.push(s.add_line(pts[k], pts[(k + 1) % n]).unwrap());
        } else {
            let chord = a.distance(b);
            let bulge = 4.0;
            let r = (chord * chord / 4.0 + bulge * bulge) / (2.0 * bulge);
            let m = (a + b) * 0.5 + (b - a).normalize().perp() * (r - bulge);
            let c = s.add_point(m.x, m.y).unwrap();
            q.push(s.add_arc(c, pts[k], pts[(k + 1) % n]).unwrap());
        }
    }
    let mut seen_drop = false;
    let mut last = f64::INFINITY;
    for k in 0..400 {
        let d = 0.1 + 70.0 * k as f64 / 399.0;
        let a = match plan_of(&s, &q, d) {
            Ok(plan) => {
                assert_clean_closed(&s, &q, &plan, d, "Bögen");
                seen_drop |= plan.dropped > 0;
                area(&plan)
            }
            Err(OffsetProblem::Vanished) => 0.0,
            Err(e) => panic!("d={d}: {e:?}"),
        };
        assert!(a <= last + 1e-6, "d={d}: {a} nach {last}");
        last = a;
    }
    assert!(seen_drop);
    assert_eq!(last, 0.0);
}

#[test]
fn several_chains_are_trimmed_together() {
    let mut s = sketch();
    let mut q = polygon(
        &mut s,
        &[(0.0, 0.0), (40.0, 0.0), (40.0, 40.0), (0.0, 40.0)],
    );
    q.extend(polygon(
        &mut s,
        &[(50.0, 0.0), (90.0, 0.0), (90.0, 40.0), (50.0, 40.0)],
    ));
    let chains = chain_of(&s, &q);
    assert_eq!(chains.len(), 2);
    // Noch getrennt
    let plan = s.offset_plan(&chains, -4.0).unwrap();
    assert_eq!(plan.loops.len(), 2);
    assert!(plan.is_closed_like_source());
    // Die Lücke von 10 ist zu: eine Schleife, gemeinsamer Rand
    let plan = s.offset_plan(&chains, -7.0).unwrap();
    assert_eq!(plan.loops.len(), 1);
    assert!(plan.is_closed_like_source());
    assert!((area(&plan) - 104.0 * 54.0).abs() < 1e-6, "{}", area(&plan));
}

// ---------------------------------------------------------------------------------------------
// Offene Ketten
// ---------------------------------------------------------------------------------------------

#[test]
fn an_open_chain_ends_on_the_normals_of_its_ends() {
    let mut s = sketch();
    let q = open_polyline(
        &mut s,
        &[(0.0, 0.0), (50.0, 0.0), (50.0, 30.0), (90.0, 30.0)],
    );
    let chains = chain_of(&s, &q);
    assert!(!chains[0].closed);
    assert_eq!(chains[0].items.len(), 3);
    let plan = s.offset_plan(&chains, 4.0).unwrap();
    assert_eq!(plan.piece_count(), 3);
    assert!(!plan.loops[0].closed);
    assert!(plan.is_closed_like_source());
    let pts: Vec<[DVec2; 2]> = plan.loops[0].pieces.iter().map(end_points).collect();
    assert_eq!(pts[0][0], v(0.0, 4.0));
    assert_eq!(pts[2][1], v(90.0, 34.0));
    assert_eq!(pts[0][1], pts[1][0]);
    assert_eq!(pts[1][1], pts[2][0]);
    assert!(pts[0][1].distance(v(46.0, 4.0)) < 1e-9);
    assert!(pts[1][1].distance(v(46.0, 34.0)) < 1e-9);
}

#[test]
fn an_open_chain_loses_what_overhangs() {
    let mut s = sketch();
    // Langes Stück, kurzer Haken nach oben: Links davon liegt der Haken im Weg.
    let q = open_polyline(&mut s, &[(0.0, 0.0), (50.0, 0.0), (50.0, 3.0)]);
    for k in 0..200 {
        let d = 3.5 + 20.0 * k as f64 / 199.0;
        let plan = plan_of(&s, &q, d).unwrap_or_else(|e| panic!("d={d}: {e:?}"));
        assert_eq!(plan.piece_count(), 1, "d={d}: {plan:?}");
        assert_eq!(plan.dropped, 1, "d={d}");
        let [a, b] = end_points(&plan.loops[0].pieces[0]);
        assert_eq!(a, v(0.0, d));
        // Das Ende liegt, wo der Abstand zum Hakenende genau d ist.
        assert!((b.distance(v(50.0, 3.0)) - d).abs() < 1e-6, "d={d}: {b}");
        assert_eq!(crossings(&plan), 0);
    }
}

#[test]
fn ordered_chains_refuses_branches_and_ignores_construction_at_the_rim() {
    let mut s = sketch();
    let a = s.add_point(0.0, 0.0).unwrap();
    let b = s.add_point(10.0, 0.0).unwrap();
    let c = s.add_point(20.0, 0.0).unwrap();
    let e = s.add_point(10.0, 10.0).unwrap();
    let l1 = s.add_line(a, b).unwrap();
    let l2 = s.add_line(b, c).unwrap();
    let t = s.add_line(b, e).unwrap();
    assert_eq!(
        s.ordered_chains(&[l1, l2, t]),
        Err(OffsetProblem::Branching)
    );
    s.set_construction(t, true).unwrap();
    let chains = s.ordered_chains(&[l1, l2, t]).unwrap();
    assert_eq!(chains.len(), 1);
    assert_eq!(chains[0].items, vec![(l1, false), (l2, false)]);
    // Eine Kette nur aus Konstruktionskurven gilt.
    let chains = s.ordered_chains(&[t]).unwrap();
    assert_eq!(chains[0].items, vec![(t, false)]);
    // Gegenläufig gezeichnete Kurve wird rückwärts durchlaufen.
    let mut s = sketch();
    let a = s.add_point(0.0, 0.0).unwrap();
    let b = s.add_point(10.0, 0.0).unwrap();
    let c = s.add_point(20.0, 5.0).unwrap();
    let l1 = s.add_line(a, b).unwrap();
    let l2 = s.add_line(c, b).unwrap();
    let chains = s.ordered_chains(&[l2, l1]).unwrap();
    assert_eq!(chains[0].items, vec![(l1, false), (l2, true)]);
    assert!(!chains[0].closed);
    // Zwei getrennte Ketten; die geschlossene beginnt an der kleinsten Kurve, in deren Richtung.
    let mut s = sketch();
    let q = polygon(&mut s, &SQUARE);
    let lone = line(&mut s, 200.0, 0.0, 210.0, 0.0);
    let mut all = q.clone();
    all.push(lone);
    all.reverse();
    let chains = s.ordered_chains(&all).unwrap();
    assert_eq!(chains.len(), 2);
    assert!(chains[0].closed);
    assert_eq!(chains[0].items[0], (q[0], false));
    assert_eq!(chains[0].items[3], (q[3], false));
    assert_eq!(chains[1].items, vec![(lone, false)]);
}

// ---------------------------------------------------------------------------------------------
// Näherung
// ---------------------------------------------------------------------------------------------

/// Ellipse mit den Halbachsen 40 und 20 (kleinster Krümmungsradius 10).
fn ellipse(s: &mut Sketch) -> CurveId {
    let c = s.add_point(0.0, 0.0).unwrap();
    s.add_ellipse(c, v(40.0, 0.0), 20.0).unwrap()
}

/// Größte Abweichung des Splines durch die Stützpunkte vom Abstand `want` zur Vorlage.
fn spline_deviation(curve: (&Sketch, CurveId), points: &[DVec2], want: f64) -> f64 {
    let mut t = sketch();
    let ids: Vec<_> = points
        .iter()
        .map(|p| t.add_point(p.x, p.y).unwrap())
        .collect();
    let sp = t.add_spline(&ids).unwrap();
    let n = (points.len() - 1) * 40;
    (0..=n)
        .map(|k| {
            let q = t
                .curve_eval(sp, (points.len() - 1) as f64 * k as f64 / n as f64)
                .unwrap();
            (exact_distance(curve.0, curve.1, q) - want).abs()
        })
        .fold(0.0, f64::max)
}

#[test]
fn an_ellipse_becomes_two_splines_through_exact_points() {
    let mut s = sketch();
    let e = ellipse(&mut s);
    // gegen die Kurvenrichtung gelaufen: links ist außen, so gelten d = 5 und d = −11 wörtlich
    let back = OrderedChain {
        items: vec![(e, true)],
        closed: true,
    };
    let forward = chain_of(&s, &[e]);
    assert_eq!(forward[0].items, vec![(e, false)]);
    assert!(forward[0].closed);
    let check = |plan: &OffsetPlan, d: f64, limit: f64| {
        assert_eq!(plan.piece_count(), 2);
        let l = &plan.loops[0];
        assert!(l.closed);
        let OffsetGeom::Approx { points: p0 } = &l.pieces[0].geom else {
            panic!()
        };
        let OffsetGeom::Approx { points: p1 } = &l.pieces[1].geom else {
            panic!()
        };
        assert_eq!(p0.len(), 33);
        assert_eq!(p1.len(), 33);
        assert_eq!(p0[32], p1[0]);
        assert_eq!(p1[32], p0[0]);
        for p in p0.iter().chain(p1) {
            let dist = exact_distance(&s, e, *p);
            assert!((dist - d).abs() < 1e-9, "{p}: {dist}");
        }
        for pts in [p0, p1] {
            let dev = spline_deviation((&s, e), pts, d);
            assert!(dev < limit, "Abweichung {dev}");
        }
    };
    let plan = s.offset_plan(std::slice::from_ref(&back), 5.0).unwrap();
    check(&plan, 5.0, 0.03);
    // Vorwärts sind die Seiten getauscht: links innen, rechts außen.
    let plan = s.offset_plan(&forward, 5.0).unwrap();
    check(&plan, 5.0, 0.03);
    let plan = s.offset_plan(&forward, -5.0).unwrap();
    check(&plan, 5.0, 0.03);
    // Innen über dem kleinsten Krümmungsradius (10) entstünden Spitzen.
    assert_eq!(s.offset_plan(&[back], -11.0), Err(OffsetProblem::Cusp));
    assert_eq!(s.offset_plan(&forward, 11.0), Err(OffsetProblem::Cusp));
    assert_eq!(s.offset_plan(&forward, 10.0), Err(OffsetProblem::Cusp));
    assert!(s.offset_plan(&forward, 9.9).is_ok());
    assert!(s.offset_plan(&forward, -11.0).is_ok());
}

#[test]
fn a_spline_gets_four_sections_per_span_and_at_most_64_points() {
    let mut s = sketch();
    let pts: Vec<_> = (0..4)
        .map(|k| s.add_point(k as f64 * 20.0, (k % 2) as f64 * 8.0).unwrap())
        .collect();
    let sp = s.add_spline(&pts).unwrap();
    let plan = plan_of(&s, &[sp], 2.0).unwrap();
    let OffsetGeom::Approx { points } = &plan.loops[0].pieces[0].geom else {
        panic!()
    };
    assert_eq!(points.len(), 13);
    for p in points {
        assert!((exact_distance(&s, sp, *p) - 2.0).abs() < 1e-9);
    }
    let pts: Vec<_> = (0..30)
        .map(|k| s.add_point(k as f64 * 20.0, (k % 2) as f64 * 2.0).unwrap())
        .collect();
    let sp = s.add_spline(&pts).unwrap();
    let plan = plan_of(&s, &[sp], 1.0).unwrap();
    let OffsetGeom::Approx { points } = &plan.loops[0].pieces[0].geom else {
        panic!()
    };
    assert_eq!(points.len(), 64);
}

#[test]
fn a_line_with_an_elliptic_arc_shares_the_joint() {
    let mut s = sketch();
    let c = s.add_point(0.0, 0.0).unwrap();
    let e = s.add_point(40.0, 0.0).unwrap();
    let t = s.add_point(0.0, 20.0).unwrap();
    let arc = s.add_elliptic_arc(c, v(40.0, 0.0), 20.0, e, t).unwrap();
    // Senkrecht ankommende Linie von unten: tangential am Anfang des Ellipsenbogens
    let a = s.add_point(40.0, -30.0).unwrap();
    let l = s.add_line(a, e).unwrap();
    for d in [-3.0, 3.0] {
        let chain = OrderedChain {
            items: vec![(l, false), (arc, false)],
            closed: false,
        };
        let plan = s.offset_plan(&[chain], d).unwrap();
        assert_eq!(plan.piece_count(), 2, "d={d}");
        let lp = &plan.loops[0].pieces;
        let (pl, pa) = (end_points(&lp[0]), end_points(&lp[1]));
        assert_eq!(pl[1], pa[0], "d={d}");
        assert!(pl[1].distance(v(40.0 - d, 0.0)) < 1e-6, "d={d}: {}", pl[1]);
        assert!(matches!(lp[1].geom, OffsetGeom::Approx { .. }));
    }
    // Scharfe Fuge: Die Linie kommt schräg an; gemeinsamer Punkt, innen wie außen.
    let b = s.add_point(90.0, -30.0).unwrap();
    let k = s.add_line(b, e).unwrap();
    for d in [-3.0, 3.0] {
        let chain = OrderedChain {
            items: vec![(k, false), (arc, false)],
            closed: false,
        };
        let plan = s.offset_plan(&[chain], d).unwrap();
        assert_eq!(plan.piece_count(), 2, "d={d}");
        let lp = &plan.loops[0].pieces;
        let (pl, pa) = (end_points(&lp[0]), end_points(&lp[1]));
        assert_eq!(pl[1], pa[0], "d={d}");
    }
}

// ---------------------------------------------------------------------------------------------
// Seite
// ---------------------------------------------------------------------------------------------

#[test]
fn offset_side_is_positive_on_the_left_of_the_chain() {
    let mut s = sketch();
    let q = polygon(&mut s, &SQUARE);
    let chain = chain_of(&s, &q).remove(0);
    let side = |x: f64, y: f64| s.offset_side(&chain, v(x, y)).unwrap();
    // gegen den Uhrzeigersinn: innen links
    assert!((side(50.0, 10.0) - 10.0).abs() < 1e-12);
    assert!((side(50.0, -7.0) + 7.0).abs() < 1e-12);
    assert!((side(90.0, 50.0) - 10.0).abs() < 1e-12);
    // außen an der Ecke zählt der Abstand zur Ecke
    assert!((side(-3.0, -4.0) + 5.0).abs() < 1e-12);
    assert!((side(103.0, 104.0) + 5.0).abs() < 1e-12);
    // Dicht rundum: nie ein Seitenwechsel ohne Überqueren
    for k in 0..=400 {
        let a = TAU * k as f64 / 400.0;
        let dir = DVec2::from_angle(a);
        let inside = s.offset_side(&chain, v(50.0, 50.0) + dir * 20.0).unwrap();
        let outside = s.offset_side(&chain, v(50.0, 50.0) + dir * 90.0).unwrap();
        assert!(inside > 0.0 && outside < 0.0, "{a}");
    }
    // Auf der Kette: keine Seite
    assert_eq!(s.offset_side(&chain, v(50.0, 0.0)), None);
    // Rückwärts tauscht das Vorzeichen.
    let back = OrderedChain {
        items: chain.items.iter().rev().map(|(c, r)| (*c, !*r)).collect(),
        closed: true,
    };
    assert!((s.offset_side(&back, v(50.0, 10.0)).unwrap() + 10.0).abs() < 1e-12);
    // Der Zeigerabstand als Maß trifft den Versatz.
    let d = side(50.0, 90.0);
    let plan = s.offset_plan(std::slice::from_ref(&chain), d).unwrap();
    assert!((area(&plan) - 80.0 * 80.0).abs() < 1e-6);
}

#[test]
fn the_plan_is_pure_and_reproducible() {
    let mut s = sketch();
    let r = rounded_rect(&mut s, 100.0, 60.0, 10.0);
    let rev = s.revision();
    let chains = s.ordered_chains(&r).unwrap();
    let a = s.offset_plan(&chains, 12.5).unwrap();
    let b = s.offset_plan(&chains, 12.5).unwrap();
    assert_eq!(a, b);
    assert_eq!(a.outline(64), b.outline(64));
    let _ = s.offset_side(&chains[0], v(1.0, 1.0));
    assert_eq!(s.revision(), rev);
}

#[test]
fn outline_runs_along_the_loop_and_repeats_the_first_point_when_closed() {
    let mut s = sketch();
    let r = rounded_rect(&mut s, 100.0, 60.0, 10.0);
    for d in [-5.0, 5.0] {
        let plan = plan_of(&s, &r, d).unwrap();
        let o = plan.outline(64);
        assert_eq!(o.len(), 1);
        assert_eq!(o[0].first(), o[0].last());
        // läuft in Richtung der Kette: positive Fläche
        let a: f64 = o[0].windows(2).map(|w| w[0].perp_dot(w[1])).sum();
        assert!(a > 0.0);
    }
    let mut s = sketch();
    let q = open_polyline(&mut s, &[(0.0, 0.0), (10.0, 0.0), (10.0, 10.0)]);
    let plan = plan_of(&s, &q, 1.0).unwrap();
    let o = plan.outline(64);
    assert_eq!(o[0].len(), 3);
    assert_ne!(o[0].first(), o[0].last());
}

/// Zwei Halbkreise als Kette: Die Schleife hält, und die Seiten stimmen.
#[test]
fn two_half_circles_keep_the_loop_together() {
    let mut s = sketch();
    let c = s.add_point(0.0, 0.0).unwrap();
    let a = s.add_point(10.0, 0.0).unwrap();
    let b = s.add_point(-10.0, 0.0).unwrap();
    let up = s.add_arc(c, a, b).unwrap();
    let down = s.add_arc(c, b, a).unwrap();
    let plan = plan_of(&s, &[up, down], 3.0).unwrap();
    assert!(plan.loops[0].closed);
    assert_eq!(plan.piece_count(), 2);
    assert_joints_bit_equal(&plan.loops[0], "Halbkreise");
    assert!((area(&plan) - std::f64::consts::PI * 49.0).abs() < 0.1);
    let plan = plan_of(&s, &[up, down], -3.0).unwrap();
    assert!((area(&plan) - std::f64::consts::PI * 169.0).abs() < 0.2);
    assert_eq!(plan_of(&s, &[up, down], 10.0), Err(OffsetProblem::Vanished));
}
