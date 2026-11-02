//! Messtests des Versatzes im Release-Build:
//! `cargo test --release -p planora-sketch --test offset_perf -- --ignored --nocapture`

use planora_math::{DVec2, Frame, TAU, Tolerance};
use planora_sketch::model::{CurveId, Sketch};
use planora_sketch::offset::{OffsetProblem, OrderedChain};
use std::time::Instant;

/// Kette aus `2·n` Kurven, Bogen und Linie im Wechsel: `n`-Eck mit runden Ecken.
fn rounded_polygon(n: usize, big: f64, r: f64) -> (Sketch, Vec<CurveId>) {
    let pts: Vec<(f64, f64)> = (0..n)
        .map(|k| {
            let a = TAU * k as f64 / n as f64;
            (big * a.cos(), big * a.sin())
        })
        .collect();
    shape(&pts, r)
}

fn median_ms(s: &Sketch, curves: &[CurveId], d: f64) -> (f64, usize) {
    let mut times = Vec::new();
    let mut pieces = 0;
    for _ in 0..101 {
        let t = Instant::now();
        let chains = s.ordered_chains(curves).unwrap();
        let plan = s.offset_plan(&chains, d).unwrap();
        let outline = plan.outline(64);
        times.push(t.elapsed().as_secs_f64() * 1e3);
        assert!(plan.is_closed_like_source() && !outline.is_empty());
        pieces = plan.piece_count();
    }
    times.sort_by(f64::total_cmp);
    (times[times.len() / 2], pieces)
}

#[test]
#[ignore = "Messtest, nur im Release-Build sinnvoll"]
fn offset_of_200_curves_under_16_ms() {
    let (s, curves) = rounded_polygon(100, 500.0, 5.0);
    assert_eq!(curves.len(), 200);
    for (label, d, want) in [
        ("außen", -3.0, 200),
        ("innen", 3.0, 200),
        ("innen, Bögen entfallen", 8.0, 100),
    ] {
        let (ms, pieces) = median_ms(&s, &curves, d);
        println!("{label} d={d}: Median {ms:.2} ms, {pieces} Stücke");
        assert_eq!(pieces, want, "{label}");
        assert!(ms < 16.0, "{label}: Median {ms:.2} ms");
    }
}

// ---------------------------------------------------------------------------------------------
// Messung des Trimmens über dichte Abstandsfolgen
// ---------------------------------------------------------------------------------------------

/// Vieleck, die Ecken mit Radius `rr[k % rr.len()]` gerundet (0 überall: scharf). Gegen den
/// Uhrzeigersinn angegeben, konkave Ecken bekommen einen gegenläufig durchlaufenen Bogen.
fn rounded(pts: &[(f64, f64)], rr: &[f64]) -> (Sketch, Vec<CurveId>) {
    let mut s = Sketch::new(Frame::WORLD, "S", Tolerance::default());
    let n = pts.len();
    let at = |k: usize| DVec2::new(pts[k % n].0, pts[k % n].1);
    if rr.iter().all(|r| *r == 0.0) {
        let ids: Vec<_> = pts.iter().map(|p| s.add_point(p.0, p.1).unwrap()).collect();
        let curves = (0..n)
            .map(|i| s.add_line(ids[i], ids[(i + 1) % n]).unwrap())
            .collect();
        return (s, curves);
    }
    let mut curves = Vec::new();
    let mut first = None;
    let mut last = None;
    for k in 0..n {
        let r = rr[k % rr.len()];
        let (p0, p1, p2) = (at(k + n - 1), at(k), at(k + 1));
        let (u, w) = ((p1 - p0).normalize(), (p2 - p1).normalize());
        let turn = u.perp_dot(w).atan2(u.dot(w));
        let inset = r * (turn.abs() / 2.0).tan();
        let (a, b) = (p1 - u * inset, p1 + w * inset);
        let left = turn > 0.0;
        let m = a + u.perp() * if left { r } else { -r };
        let (ia, ib, ic) = (
            s.add_point(a.x, a.y).unwrap(),
            s.add_point(b.x, b.y).unwrap(),
            s.add_point(m.x, m.y).unwrap(),
        );
        if let Some(prev) = last {
            curves.push(s.add_line(prev, ia).unwrap());
        } else {
            first = Some(ia);
        }
        curves.push(if left {
            s.add_arc(ic, ia, ib).unwrap()
        } else {
            s.add_arc(ic, ib, ia).unwrap()
        });
        last = Some(ib);
    }
    curves.push(s.add_line(last.unwrap(), first.unwrap()).unwrap());
    (s, curves)
}

fn shape(pts: &[(f64, f64)], r: f64) -> (Sketch, Vec<CurveId>) {
    rounded(pts, &[r])
}

fn sketch() -> Sketch {
    Sketch::new(Frame::WORLD, "S", Tolerance::default())
}

fn add_at(s: &mut Sketch, p: DVec2) -> planora_sketch::model::PointId {
    s.add_point(p.x, p.y).unwrap()
}

// Die Formen der Nachprüfung: Bögen, die sich berühren, schneiden oder einschnüren.

fn stadium(w: f64, r: f64) -> (Sketch, Vec<CurveId>) {
    let mut s = sketch();
    let (a, b, c, d) = (
        s.add_point(0., -r).unwrap(),
        s.add_point(w, -r).unwrap(),
        s.add_point(w, r).unwrap(),
        s.add_point(0., r).unwrap(),
    );
    let (m1, m2) = (s.add_point(w, 0.).unwrap(), s.add_point(0., 0.).unwrap());
    let cs = vec![
        s.add_line(a, b).unwrap(),
        s.add_arc(m1, b, c).unwrap(),
        s.add_line(c, d).unwrap(),
        s.add_arc(m2, d, a).unwrap(),
    ];
    (s, cs)
}
fn oval() -> (Sketch, Vec<CurveId>) {
    let mut s = sketch();
    let p: Vec<_> = [(39., -12.), (39., 12.), (-39., 12.), (-39., -12.)]
        .iter()
        .map(|q| s.add_point(q.0, q.1).unwrap())
        .collect();
    let c: Vec<_> = [(30., 0.), (0., -40.), (-30., 0.), (0., 40.)]
        .iter()
        .map(|q| s.add_point(q.0, q.1).unwrap())
        .collect();
    let cs = (0..4)
        .map(|k| s.add_arc(c[k], p[k], p[(k + 1) % 4]).unwrap())
        .collect();
    (s, cs)
}
fn peanut() -> (Sketch, Vec<CurveId>) {
    let mut s = sketch();
    let (cx, big, small): (f64, f64, f64) = (35.0, 25.0, 20.0);
    let h = ((big + small) * (big + small) - cx * cx).sqrt();
    let (cr, cl, ct, cb) = (
        DVec2::new(cx, 0.),
        DVec2::new(-cx, 0.),
        DVec2::new(0., h),
        DVec2::new(0., -h),
    );
    let tp = |a: DVec2, b: DVec2| a + (b - a).normalize() * big;
    let (tr, tl, bl, br) = (tp(cr, ct), tp(cl, ct), tp(cl, cb), tp(cr, cb));
    let (ptr, ptl, pbl, pbr) = (
        add_at(&mut s, tr),
        add_at(&mut s, tl),
        add_at(&mut s, bl),
        add_at(&mut s, br),
    );
    let (mr, ml, mt, mb) = (
        add_at(&mut s, cr),
        add_at(&mut s, cl),
        add_at(&mut s, ct),
        add_at(&mut s, cb),
    );
    let cs = vec![
        s.add_arc(mr, pbr, ptr).unwrap(),
        s.add_arc(mt, ptl, ptr).unwrap(),
        s.add_arc(ml, ptl, pbl).unwrap(),
        s.add_arc(mb, pbr, pbl).unwrap(),
    ];
    (s, cs)
}
fn cone() -> (Sketch, Vec<CurveId>) {
    let mut s = sketch();
    let phi = (20.0f64 / 60.0).acos();
    let (t1, t2) = (
        DVec2::from_angle(phi) * 20.0,
        DVec2::from_angle(-phi) * 20.0,
    );
    let (c, a, b, ap) = (
        s.add_point(0., 0.).unwrap(),
        add_at(&mut s, t1),
        add_at(&mut s, t2),
        s.add_point(60., 0.).unwrap(),
    );
    let cs = vec![
        s.add_arc(c, a, b).unwrap(),
        s.add_line(b, ap).unwrap(),
        s.add_line(ap, a).unwrap(),
    ];
    (s, cs)
}
fn keyhole() -> (Sketch, Vec<CurveId>) {
    let mut s = sketch();
    let x = 30.0 - (225.0f64 - 16.0).sqrt();
    let c = s.add_point(30., 0.).unwrap();
    let p: Vec<_> = [
        (-60., -30.),
        (-10., -30.),
        (-10., -4.),
        (x, -4.),
        (x, 4.),
        (-10., 4.),
        (-10., 30.),
        (-60., 30.),
    ]
    .iter()
    .map(|q| s.add_point(q.0, q.1).unwrap())
    .collect();
    let mut cs = vec![];
    for k in 0..3 {
        cs.push(s.add_line(p[k], p[k + 1]).unwrap());
    }
    cs.push(s.add_arc(c, p[3], p[4]).unwrap());
    for k in 4..8 {
        cs.push(s.add_line(p[k], p[(k + 1) % 8]).unwrap());
    }
    (s, cs)
}
/// Arc of the given sweep, closed with its chord.
fn segment(sweep: f64) -> (Sketch, Vec<CurveId>) {
    let mut s = sketch();
    let a0 = -sweep / 2.0 - TAU / 4.0 + TAU / 2.0;
    let (st, en) = (
        DVec2::from_angle(a0) * 30.0,
        DVec2::from_angle(a0 + sweep) * 30.0,
    );
    let (c, a, b) = (
        s.add_point(0., 0.).unwrap(),
        add_at(&mut s, st),
        add_at(&mut s, en),
    );
    let cs = vec![s.add_arc(c, a, b).unwrap(), s.add_line(b, a).unwrap()];
    (s, cs)
}
fn lens() -> (Sketch, Vec<CurveId>) {
    let mut s = sketch();
    let (a, b) = (
        s.add_point(40., 0.).unwrap(),
        s.add_point(-40., 0.).unwrap(),
    );
    let (c1, c2) = (
        s.add_point(0., -30.).unwrap(),
        s.add_point(0., 30.).unwrap(),
    );
    let cs = vec![s.add_arc(c1, a, b).unwrap(), s.add_arc(c2, b, a).unwrap()];
    (s, cs)
}
fn astroid() -> (Sketch, Vec<CurveId>) {
    let mut s = sketch();
    let p: Vec<_> = [(40., 0.), (0., 40.), (-40., 0.), (0., -40.)]
        .iter()
        .map(|q| s.add_point(q.0, q.1).unwrap())
        .collect();
    let c: Vec<_> = [(50., 50.), (-50., 50.), (-50., -50.), (50., -50.)]
        .iter()
        .map(|q| s.add_point(q.0, q.1).unwrap())
        .collect();
    let cs = (0..4)
        .map(|k| s.add_arc(c[k], p[(k + 1) % 4], p[k]).unwrap())
        .collect();
    (s, cs)
}
fn scallop(n: usize, bulge: f64) -> (Sketch, Vec<CurveId>) {
    let mut s = sketch();
    let pos: Vec<DVec2> = (0..n)
        .map(|k| DVec2::from_angle(TAU * k as f64 / n as f64) * 60.0)
        .collect();
    let ids: Vec<_> = pos.iter().map(|p| add_at(&mut s, *p)).collect();
    let mut cs = vec![];
    for k in 0..n {
        let j = (k + 1) % n;
        if k % 2 == 1 {
            cs.push(s.add_line(ids[k], ids[j]).unwrap());
            continue;
        }
        let (a, b) = (pos[k], pos[j]);
        let (ch, u) = (a.distance(b), (b - a).normalize());
        let b0 = bulge.abs();
        let r = (ch * ch / 4.0 + b0 * b0) / (2.0 * b0);
        if bulge > 0.0 {
            let m = (a + b) * 0.5 + u.perp() * (r - b0);
            let c = add_at(&mut s, m);
            cs.push(s.add_arc(c, ids[k], ids[j]).unwrap());
        } else {
            let m = (a + b) * 0.5 - u.perp() * (r - b0);
            let c = add_at(&mut s, m);
            cs.push(s.add_arc(c, ids[j], ids[k]).unwrap());
        }
    }
    (s, cs)
}

fn rng_star(seed: u64, n: usize) -> Vec<(f64, f64)> {
    let mut x = seed;
    (0..n)
        .map(|k| {
            x = x
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            let r = 30.0 + 70.0 * ((x >> 33) as f64 / (1u64 << 31) as f64);
            let a = TAU * k as f64 / n as f64;
            (r * a.cos(), r * a.sin())
        })
        .collect()
}

/// Sockel 92×15, sieben Zinken 8 breit und 40 hoch, Lücke 6.
fn comb() -> Vec<(f64, f64)> {
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
    pts
}

/// Band aus drei Windungen, 6 breit, Abstand der Windungen 16.
fn spiral_band() -> Vec<(f64, f64)> {
    let path: Vec<DVec2> = (0..60)
        .map(|k| {
            let a = TAU * k as f64 / 20.0;
            DVec2::from_angle(a) * (15.0 + 16.0 * a / TAU)
        })
        .collect();
    let mut left = Vec::new();
    let mut right = Vec::new();
    for k in 0..path.len() {
        let dir = (path[(k + 1).min(path.len() - 1)] - path[k.saturating_sub(1)]).normalize();
        left.push(path[k] + dir.perp() * 3.0);
        right.push(path[k] - dir.perp() * 3.0);
    }
    right.extend(left.into_iter().rev());
    right.into_iter().map(|p| (p.x, p.y)).collect()
}

/// Hohlraum mit Öffnung nach rechts und einer Spitze, die in ihn hineinragt: Außen laufen die
/// Gehrungen in die gegenüberliegenden Wände.
const WITH_SPIKE: [(f64, f64); 15] = [
    (0.0, 0.0),
    (100.0, 0.0),
    (100.0, 40.0),
    (80.0, 40.0),
    (80.0, 20.0),
    (20.0, 20.0),
    (20.0, 45.0),
    (60.0, 50.0),
    (20.0, 55.0),
    (20.0, 80.0),
    (80.0, 80.0),
    (80.0, 60.0),
    (100.0, 60.0),
    (100.0, 100.0),
    (0.0, 100.0),
];
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
const ARROW: [(f64, f64); 7] = [
    (0.0, 8.0),
    (60.0, 8.0),
    (60.0, 0.0),
    (100.0, 15.0),
    (60.0, 30.0),
    (60.0, 22.0),
    (0.0, 22.0),
];
const ZIG: [(f64, f64); 9] = [
    (0.0, 0.0),
    (120.0, 0.0),
    (120.0, 30.0),
    (100.0, 12.0),
    (80.0, 30.0),
    (60.0, 12.0),
    (40.0, 30.0),
    (20.0, 12.0),
    (0.0, 30.0),
];

/// Abstand Sehne–Kreis je Radius bei 128 Abschnitten je Vollkreis: So weit weicht der Linienzug
/// eines Bogens von ihm ab, das zählt nicht als Fehler.
const SAG: f64 = 3.1e-4;
/// Knicke im Linienzug der Vorlage ab diesem Winkel sind Ecken (Bögen: 256 Abschnitte je Vollkreis).
const CORNER: f64 = 0.05;

#[derive(Default)]
struct Tally {
    steps: usize,
    ok: usize,
    vanished: usize,
    self_crossing: usize,
    no_join: usize,
    cusp: usize,
    open: usize,
    crossing: usize,
    too_close: usize,
    /// Geliefert, aber ein Punkt der Vorlage fehlt: innen mit Abstand weit über d nicht im
    /// Ergebnis, außen in der Vorlage oder näher als d und nicht im Ergebnis.
    missing: usize,
    /// Geliefert, aber etwas zu viel: innen auf der falschen Seite oder zu nah, außen weit weg.
    excess: usize,
    area_jump: usize,
    false_vanish: usize,
    /// Vorwärts und rückwärts verschieden.
    asymmetric: usize,
    worst_step_ms: f64,
    first_problem: Option<f64>,
}

fn point_in(polys: &[Vec<DVec2>], p: DVec2) -> bool {
    let mut inside = false;
    for pts in polys {
        for w in pts.windows(2) {
            let (a, b) = (w[0], w[1]);
            if (a.y > p.y) != (b.y > p.y) && p.x < a.x + (p.y - a.y) / (b.y - a.y) * (b.x - a.x) {
                inside = !inside;
            }
        }
    }
    inside
}

fn segment_distance(p: DVec2, a: DVec2, b: DVec2) -> f64 {
    let d = b - a;
    let t = ((p - a).dot(d) / d.length_squared()).clamp(0.0, 1.0);
    p.distance(a + d * t)
}

fn proper_cross(p: DVec2, q: DVec2, r: DVec2, s: DVec2) -> bool {
    let side = |a: DVec2, b: DVec2, c: DVec2| (b - a).perp_dot(c - a);
    let (d1, d2, d3, d4) = (side(r, s, p), side(r, s, q), side(p, q, r), side(p, q, s));
    const E: f64 = 1e-6;
    ((d1 > E && d2 < -E) || (d1 < -E && d2 > E)) && ((d3 > E && d4 < -E) || (d3 < -E && d4 > E))
}

fn outline_area(outline: &[Vec<DVec2>]) -> f64 {
    outline
        .iter()
        .map(|pl| {
            pl.windows(2)
                .map(|w| w[0].perp_dot(w[1]))
                .sum::<f64>()
                .abs()
                / 2.0
        })
        .sum()
}

/// Größte Gehrung je Abstand einer Seite: Weiter als `d·k` von der Vorlage reicht keine Ecke.
fn mitre_reach(source: &[DVec2], inward_turn_sign: f64) -> f64 {
    let n = source.len() - 1;
    (0..n)
        .map(|k| {
            let (a, b, c) = (source[(k + n - 1) % n], source[k], source[(k + 1) % n]);
            let (u, w) = ((b - a).normalize_or_zero(), (c - b).normalize_or_zero());
            let turn = u.perp_dot(w).atan2(u.dot(w));
            // Gehrung auf der Seite, von der die Kette wegdreht.
            if turn * inward_turn_sign < -CORNER {
                1.0 / (turn.abs() / 2.0).cos()
            } else {
                1.0
            }
        })
        .fold(1.0, f64::max)
}

/// Misst eine geschlossene Form über `steps` Abstände, innen und außen, in beiden Richtungen der
/// Kette. Die Prüfungen rechnen nur auf Linienzügen des Ergebnisses und der Vorlage, nicht mit den
/// Funktionen des Plans.
fn measure(s: &Sketch, curves: &[CurveId], steps: usize) -> [Tally; 2] {
    let chains = s.ordered_chains(curves).unwrap();
    assert_eq!(chains.len(), 1);
    let back = OrderedChain {
        items: chains[0]
            .items
            .iter()
            .rev()
            .map(|(c, r)| (*c, !*r))
            .collect(),
        closed: chains[0].closed,
    };
    let mut source: Vec<DVec2> = Vec::new();
    for (c, rev) in &chains[0].items {
        let mut pl = s.curve_polyline(*c, 256).unwrap();
        if *rev {
            pl.reverse();
        }
        if source.last() == pl.first() {
            pl.remove(0);
        }
        source.extend(pl);
    }
    if source.first() != source.last() {
        source.push(source[0]);
    }
    let signed: f64 = source.windows(2).map(|w| w[0].perp_dot(w[1])).sum();
    // Die Kette kann auch im Uhrzeigersinn laufen: „innen“ ist dann rechts.
    let inward = if signed > 0.0 { 1.0 } else { -1.0 };
    let reach_in = mitre_reach(&source, inward);
    let reach_out = mitre_reach(&source, -inward);
    let (lo, hi) = source
        .iter()
        .fold((DVec2::INFINITY, DVec2::NEG_INFINITY), |(l, h), p| {
            (l.min(*p), h.max(*p))
        });
    let size = (hi - lo).length();
    // Gitter mit Abstand und Seite zur Vorlage: einmal gerechnet, gilt für jedes d.
    const GRID: usize = 40;
    let margin = size * 0.6;
    let grid: Vec<(DVec2, f64, bool)> = (0..GRID * GRID)
        .map(|k| {
            let f = DVec2::new(
                ((k % GRID) as f64 + 0.37) / GRID as f64,
                ((k / GRID) as f64 + 0.61) / GRID as f64,
            );
            let p = lo - DVec2::splat(margin) + (hi - lo + DVec2::splat(2.0 * margin)) * f;
            let dist = source
                .windows(2)
                .map(|w| segment_distance(p, w[0], w[1]))
                .fold(f64::INFINITY, f64::min);
            (p, dist, point_in(std::slice::from_ref(&source), p))
        })
        .collect();
    let mut out = [Tally::default(), Tally::default()];
    for (side, tally) in out.iter_mut().enumerate() {
        let outward = side == 1;
        let limit = if outward { size } else { size * 0.5 };
        let mut last_area: Option<f64> = None;
        for k in 0..steps {
            let d = 0.05 + (limit - 0.05) * k as f64 / (steps - 1) as f64;
            let signed_d = if outward { -inward * d } else { inward * d };
            let slack = 0.05 + size * 5e-4 + (d + size) * SAG;
            let far = if outward { reach_out } else { reach_in } * d + slack + 0.5;
            tally.steps += 1;
            let t = Instant::now();
            let result = s.offset_plan(&chains, signed_d);
            tally.worst_step_ms = tally.worst_step_ms.max(t.elapsed().as_secs_f64() * 1e3);
            let reverse = s.offset_plan(std::slice::from_ref(&back), -signed_d);
            let same = match (&result, &reverse) {
                (Ok(a), Ok(b)) => {
                    let (x, y) = (outline_area(&a.outline(128)), outline_area(&b.outline(128)));
                    (x - y).abs() <= 0.05 + x * 1e-3
                }
                (Err(a), Err(b)) => a == b,
                _ => false,
            };
            if !same {
                tally.asymmetric += 1;
                tally.first_problem.get_or_insert(d);
            }
            let plan = match result {
                Ok(p) => p,
                Err(e) => {
                    match e {
                        OffsetProblem::Vanished => {
                            tally.vanished += 1;
                            let room = grid.iter().any(|g| g.2 && g.1 > far);
                            if outward || room {
                                tally.false_vanish += 1;
                                tally.first_problem.get_or_insert(d);
                            }
                            last_area = Some(0.0);
                        }
                        OffsetProblem::SelfCrossing => {
                            tally.self_crossing += 1;
                            tally.first_problem.get_or_insert(d);
                        }
                        OffsetProblem::NoJoin => {
                            tally.no_join += 1;
                            tally.first_problem.get_or_insert(d);
                        }
                        OffsetProblem::Cusp => tally.cusp += 1,
                        other => panic!("{other:?}"),
                    }
                    continue;
                }
            };
            tally.ok += 1;
            let mut bad = false;
            if !plan.is_closed_like_source() || plan.loops.iter().any(|l| !l.closed) {
                tally.open += 1;
                bad = true;
            }
            let outline = plan.outline(128);
            // Kreuzungen der fein abgetasteten Schleifen untereinander und mit sich selbst
            let segs: Vec<(DVec2, DVec2)> = outline
                .iter()
                .flat_map(|pl| pl.windows(2).map(|w| (w[0], w[1])))
                .collect();
            let crossed = (0..segs.len()).any(|i| {
                (i + 1..segs.len())
                    .any(|j| proper_cross(segs[i].0, segs[i].1, segs[j].0, segs[j].1))
            });
            if crossed {
                tally.crossing += 1;
                bad = true;
            }
            // Punkte des Ergebnisses dürfen nicht näher an der Vorlage liegen.
            let close = segs.iter().any(|sg| {
                let m = (sg.0 + sg.1) * 0.5;
                source
                    .windows(2)
                    .map(|w| segment_distance(m, w[0], w[1]))
                    .fold(f64::INFINITY, f64::min)
                    < d - 0.02 - size * 2e-4 - (d + size) * SAG
            });
            if close {
                tally.too_close += 1;
                bad = true;
            }
            let mut missing = false;
            let mut excess = false;
            for &(p, dist, inside) in &grid {
                let in_result = point_in(&outline, p);
                if outward {
                    missing |= (inside || dist < d - slack) && !in_result;
                    // Nur ohne Ecken: Über entfallene Stücke hinweg reicht eine Gehrung weiter.
                    excess |= reach_out == 1.0 && in_result && !inside && dist > far;
                } else {
                    missing |= inside && dist > far && !in_result;
                    excess |= in_result && (!inside || dist < d - slack);
                }
            }
            if missing {
                tally.missing += 1;
                bad = true;
            }
            if excess {
                tally.excess += 1;
                bad = true;
            }
            let area = outline_area(&outline);
            if let Some(prev) = last_area {
                let jump = if outward {
                    area < prev * 0.999
                } else {
                    area > prev * 1.001 + 0.1
                };
                if jump {
                    tally.area_jump += 1;
                    bad = true;
                }
            }
            last_area = Some(area);
            if bad {
                tally.first_problem.get_or_insert(d);
            }
        }
    }
    out
}

/// Zeilen mit Abweichungen: `(Name mit Seite, Meldungen, zu nah, frühester erster Befund)`. Die
/// Meldungen sind `SelfCrossing` und `NoJoin` zusammen. Alle übrigen Zeilen haben nichts davon.
const ROW_LIMITS: &[(&str, usize, usize, f64)] = &[
    // Näherung: Der Spline weicht bei großem Abstand mehr ab, als die Prüfung zulässt.
    ("Ellipse außen", 0, 267, 29.0),
    ("C mit Spitze innen", 3, 0, 10.0),
    ("Spiralband außen", 109, 0, 121.0),
    ("Zufallsstern 3 innen", 7, 0, 30.0),
    ("Zufallsstern 6 innen", 28, 0, 17.0),
    // Spitzen zwischen hohlen Bögen ohne Schnitt: Der Plan kennt keinen Verbindungsbogen.
    ("Astroide außen", 396, 0, 1.0),
    ("Bögen außen 16 innen", 1, 0, 18.9),
    ("Bögen innen 16 außen", 11, 0, 14.5),
    ("Bögen innen 8 außen", 38, 0, 15.3),
];

/// Obergrenze für einen einzelnen Zug.
const WORST_STEP_MS: f64 = 16.0;

#[test]
#[ignore = "Messung, nur im Release-Build sinnvoll"]
fn trimming_sweep_report() {
    let circle = {
        let mut s = Sketch::new(Frame::WORLD, "S", Tolerance::default());
        let c = s.add_point(0.0, 0.0).unwrap();
        let k = s.add_circle(c, 40.0).unwrap();
        (s, vec![k])
    };
    let ell = {
        let mut s = Sketch::new(Frame::WORLD, "S", Tolerance::default());
        let c = s.add_point(0.0, 0.0).unwrap();
        let k = s.add_ellipse(c, DVec2::new(40.0, 0.0), 20.0).unwrap();
        (s, vec![k])
    };
    let scallop_ring = |n: usize, bulge: f64| scallop(n, bulge);
    let rect = [(0., 0.), (100., 0.), (100., 60.), (0., 60.)];
    let l = [
        (0., 0.),
        (60., 0.),
        (60., 20.),
        (20., 20.),
        (20., 60.),
        (0., 60.),
    ];
    let u = [
        (0., 0.),
        (100., 0.),
        (100., 80.),
        (60., 80.),
        (60., 30.),
        (40., 30.),
        (40., 80.),
        (0., 80.),
    ];
    let star: Vec<(f64, f64)> = (0..10)
        .map(|k| {
            let r = if k % 2 == 0 { 50. } else { 20. };
            let a = TAU * k as f64 / 10.;
            (r * f64::cos(a), r * f64::sin(a))
        })
        .collect();
    let hex: Vec<(f64, f64)> = (0..6)
        .map(|k| {
            let a = TAU * k as f64 / 6.;
            (50. * f64::cos(a), 50. * f64::sin(a))
        })
        .collect();
    let mut shapes: Vec<(String, (Sketch, Vec<CurveId>))> = vec![
        ("Kreis".into(), circle),
        ("Ellipse".into(), ell),
        ("Hantel".into(), shape(&DUMBBELL, 0.0)),
        ("Hantel rund 2".into(), shape(&DUMBBELL, 2.0)),
        ("Kamm".into(), shape(&comb(), 0.0)),
        ("Kamm rund 2".into(), shape(&comb(), 2.0)),
        ("C mit Spitze".into(), shape(&WITH_SPIKE, 0.0)),
        ("C mit Spitze rund 1".into(), shape(&WITH_SPIKE, 1.0)),
        ("Pfeil".into(), shape(&ARROW, 0.0)),
        ("Zickzack".into(), shape(&ZIG, 0.0)),
        ("Zickzack rund 2".into(), shape(&ZIG, 2.0)),
        ("Spiralband".into(), shape(&spiral_band(), 0.0)),
    ];
    for seed in 1..=6u64 {
        shapes.push((
            format!("Zufallsstern {seed}"),
            shape(&rng_star(seed * 7919, 8 + 4 * seed as usize), 0.0),
        ));
    }
    for seed in 1..=3u64 {
        shapes.push((
            format!("Zufallsstern rund {seed}"),
            shape(&rng_star(seed * 104729, 12), 1.5),
        ));
    }
    // Die 23 Formen der Nachprüfung; „Bögen außen 16“ ist zugleich die 22. der ersten Messung.
    shapes.extend([
        ("Stadion".to_string(), stadium(100., 15.)),
        ("Oval".to_string(), oval()),
        ("Erdnuss".to_string(), peanut()),
        (
            "Rechteck rund gemischt".to_string(),
            rounded(&rect, &[5., 20., 10., 25.]),
        ),
        ("Rechteck rund 0,5".to_string(), rounded(&rect, &[0.5])),
        ("L rund 3".to_string(), rounded(&l, &[3.])),
        ("L rund 8".to_string(), rounded(&l, &[8.])),
        ("U rund 4".to_string(), rounded(&u, &[4.])),
        ("U rund 9".to_string(), rounded(&u, &[9.])),
        ("Hantel rund 2 b".to_string(), rounded(&DUMBBELL, &[2.])),
        ("Hantel rund 5".to_string(), rounded(&DUMBBELL, &[5.])),
        ("Stern rund 3".to_string(), rounded(&star, &[3.])),
        ("Sechseck rund 30/3".to_string(), rounded(&hex, &[30., 3.])),
        ("Kegel".to_string(), cone()),
        ("Schlüsselloch".to_string(), keyhole()),
        ("Segment 180".to_string(), segment(TAU / 2.)),
        ("Segment 270".to_string(), segment(TAU * 0.75)),
        ("Segment 330".to_string(), segment(TAU * 0.92)),
        ("Linse".to_string(), lens()),
        ("Astroide".to_string(), astroid()),
        ("Bögen außen 16".to_string(), scallop_ring(16, 4.)),
        ("Bögen innen 16".to_string(), scallop_ring(16, -4.)),
        ("Bögen innen 8".to_string(), scallop_ring(8, -10.)),
    ]);
    assert_eq!(shapes.len(), 44);
    println!(
        "{:<30} {:>5} | {:>5} {:>5} {:>5} {:>5} {:>4} | {:>4} {:>4} {:>4} {:>4} {:>4} {:>4} {:>4} {:>4} | {:>7} {:>6}",
        "Form, Seite",
        "Züge",
        "ok",
        "weg",
        "kreuz",
        "noJ",
        "cusp",
        "offn",
        "krz",
        "nah",
        "fehl",
        "viel",
        "flä",
        "fweg",
        "asym",
        "erster",
        "ms max"
    );
    let mut total = Tally::default();
    let mut limits_hit: Vec<String> = Vec::new();
    for (name, (s, curves)) in &shapes {
        let [inner, outer] = measure(s, curves, 400);
        for (label, t) in [("innen", &inner), ("außen", &outer)] {
            let row = format!("{name} {label}");
            println!(
                "{:<30} {:>5} | {:>5} {:>5} {:>5} {:>5} {:>4} | {:>4} {:>4} {:>4} {:>4} {:>4} {:>4} {:>4} {:>4} | {:>7} {:>6.2}",
                row,
                t.steps,
                t.ok,
                t.vanished,
                t.self_crossing,
                t.no_join,
                t.cusp,
                t.open,
                t.crossing,
                t.too_close,
                t.missing,
                t.excess,
                t.area_jump,
                t.false_vanish,
                t.asymmetric,
                t.first_problem
                    .map_or("-".to_string(), |d| format!("{d:.2}")),
                t.worst_step_ms
            );
            let (refusals, close_limit, first_limit) = ROW_LIMITS
                .iter()
                .find(|r| r.0 == row)
                .map_or((0, 0, f64::INFINITY), |r| (r.1, r.2, r.3));
            let mut check = |ok: bool, what: &str| {
                if !ok {
                    limits_hit.push(format!("{row}: {what}"));
                }
            };
            check(t.steps == 400, "Züge");
            check(
                t.ok + t.vanished + t.self_crossing + t.no_join + t.cusp == t.steps,
                "Summe der Ausgänge",
            );
            check(t.ok >= 1, "liefert nichts");
            check(t.self_crossing + t.no_join <= refusals, "Meldungen");
            check(t.too_close <= close_limit, "zu nah");
            check(t.open == 0, "offen");
            check(t.crossing == 0, "Kreuzung");
            check(t.missing == 0, "unvollständig");
            check(t.excess == 0, "zu viel");
            check(t.area_jump == 0, "Fläche");
            check(t.false_vanish == 0, "falsch verschwunden");
            check(t.asymmetric == 0, "Richtungen verschieden");
            check(
                t.cusp == 0 || (row == "Ellipse innen" && t.cusp <= 311),
                "Spitze",
            );
            check(t.vanished == 0 || label == "innen", "außen verschwunden");
            check(
                t.first_problem.is_none_or(|d| d >= first_limit),
                "erster Befund zu früh",
            );
            check(t.worst_step_ms < WORST_STEP_MS, "Zeit je Zug");
            total.steps += t.steps;
            total.ok += t.ok;
            total.vanished += t.vanished;
            total.self_crossing += t.self_crossing;
            total.no_join += t.no_join;
            total.cusp += t.cusp;
            total.open += t.open;
            total.crossing += t.crossing;
            total.too_close += t.too_close;
            total.missing += t.missing;
            total.excess += t.excess;
            total.area_jump += t.area_jump;
            total.false_vanish += t.false_vanish;
            total.asymmetric += t.asymmetric;
        }
    }
    println!(
        "Summe: {} Züge, {} ok, weg {}, Spitze {}, SelfCrossing {}, NoJoin {}, offen {}, Kreuzung {}, zu nah {}, fehlt {}, zu viel {}, Fläche {}, falsch verschwunden {}, asym {}",
        total.steps,
        total.ok,
        total.vanished,
        total.cusp,
        total.self_crossing,
        total.no_join,
        total.open,
        total.crossing,
        total.too_close,
        total.missing,
        total.excess,
        total.area_jump,
        total.false_vanish,
        total.asymmetric
    );
    assert!(limits_hit.is_empty(), "{limits_hit:#?}");
    assert_eq!(total.steps, 35200);
    assert!(total.ok >= 23585, "{}", total.ok);
    assert!(total.vanished <= 10711, "{}", total.vanished);
    assert!(total.cusp <= 311, "{}", total.cusp);
    assert!(total.self_crossing <= 147, "{}", total.self_crossing);
    assert!(total.no_join <= 446, "{}", total.no_join);
    assert!(total.too_close <= 267, "{}", total.too_close);
    assert_eq!(total.open, 0);
    assert_eq!(total.crossing, 0);
    assert_eq!(total.missing, 0);
    assert_eq!(total.excess, 0);
    assert_eq!(total.area_jump, 0);
    assert_eq!(total.false_vanish, 0);
    assert_eq!(total.asymmetric, 0);
}
