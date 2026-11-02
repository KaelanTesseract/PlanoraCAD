//! Messtests des Versatzes im Release-Build:
//! `cargo test --release -p planora-sketch --test offset_perf -- --ignored --nocapture`

use planora_math::{DVec2, Frame, TAU, Tolerance};
use planora_sketch::model::{CurveId, Sketch};
use planora_sketch::offset::OffsetProblem;
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

/// Vieleck, die Ecken mit Radius `r` gerundet (0: scharf). Gegen den Uhrzeigersinn angegeben,
/// konkave Ecken bekommen einen gegenläufig durchlaufenen Bogen.
fn shape(pts: &[(f64, f64)], r: f64) -> (Sketch, Vec<CurveId>) {
    let mut s = Sketch::new(Frame::WORLD, "S", Tolerance::default());
    let n = pts.len();
    let at = |k: usize| DVec2::new(pts[k % n].0, pts[k % n].1);
    if r == 0.0 {
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

/// Bögen und Linien im Wechsel mit scharfen Fugen: Jede zweite Kante ist ein Bogen mit Stich
/// `bulge`, der nach außen wölbt.
fn scallop(pts: &[(f64, f64)], bulge: f64) -> (Sketch, Vec<CurveId>) {
    let mut s = Sketch::new(Frame::WORLD, "S", Tolerance::default());
    let n = pts.len();
    let ids: Vec<_> = pts.iter().map(|p| s.add_point(p.0, p.1).unwrap()).collect();
    let mut curves = Vec::new();
    for k in 0..n {
        let (a, b) = (
            DVec2::new(pts[k].0, pts[k].1),
            DVec2::new(pts[(k + 1) % n].0, pts[(k + 1) % n].1),
        );
        if k % 2 == 1 {
            curves.push(s.add_line(ids[k], ids[(k + 1) % n]).unwrap());
            continue;
        }
        let (chord, u) = (a.distance(b), (b - a).normalize());
        let radius = (chord * chord / 4.0 + bulge * bulge) / (2.0 * bulge);
        let m = (a + b) * 0.5 + u.perp() * (radius - bulge);
        let c = s.add_point(m.x, m.y).unwrap();
        curves.push(s.add_arc(c, ids[k], ids[(k + 1) % n]).unwrap());
    }
    (s, curves)
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
    wrong_inside: usize,
    area_jump: usize,
    false_vanish: usize,
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

/// Misst eine geschlossene Form über `steps` Abstände, innen und außen. Die Prüfungen rechnen nur
/// auf Linienzügen des Ergebnisses und der Vorlage, nicht mit den Funktionen des Plans.
fn measure(s: &Sketch, curves: &[CurveId], steps: usize) -> [Tally; 2] {
    let chains = s.ordered_chains(curves).unwrap();
    assert_eq!(chains.len(), 1);
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
            tally.steps += 1;
            let t = Instant::now();
            let result = s.offset_plan(&chains, signed_d);
            tally.worst_step_ms = tally.worst_step_ms.max(t.elapsed().as_secs_f64() * 1e3);
            let plan = match result {
                Ok(p) => p,
                Err(e) => {
                    match e {
                        OffsetProblem::Vanished => {
                            tally.vanished += 1;
                            if !outward && grid.iter().any(|g| g.2 && g.1 > d * 1.5 + 0.5) {
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
            // Gitter: Innen liegt das Ergebnis auf der richtigen Seite und mindestens d weit weg;
            // außen enthält es die Vorlage und alles, was näher als d an ihr liegt.
            let slack = 0.05 + size * 5e-4 + (d + size) * SAG;
            let wrong = grid.iter().any(|&(p, dist, inside)| {
                let inside_result = point_in(&outline, p);
                if outward {
                    (inside || dist < d - slack) && !inside_result
                } else {
                    inside_result && (!inside || dist < d - slack)
                }
            });
            if wrong {
                tally.wrong_inside += 1;
                bad = true;
            }
            let area: f64 = outline
                .iter()
                .map(|pl| {
                    pl.windows(2)
                        .map(|w| w[0].perp_dot(w[1]))
                        .sum::<f64>()
                        .abs()
                        / 2.0
                })
                .sum();
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
        (
            "Bögen scharf 16-Eck".into(),
            scallop(
                &(0..16)
                    .map(|k| {
                        let a = TAU * k as f64 / 16.0;
                        (60.0 * a.cos(), 60.0 * a.sin())
                    })
                    .collect::<Vec<_>>(),
                4.0,
            ),
        ),
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
    println!(
        "{:<28} {:>5} | {:>5} {:>5} {:>5} {:>5} {:>4} | {:>4} {:>4} {:>4} {:>4} {:>4} {:>4} | {:>7} {:>8}",
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
        "gitt",
        "flä",
        "fweg",
        "erster",
        "ms max"
    );
    let mut total = Tally::default();
    for (name, (s, curves)) in &shapes {
        let [inner, outer] = measure(s, curves, 400);
        for (label, t) in [("innen", &inner), ("außen", &outer)] {
            println!(
                "{:<28} {:>5} | {:>5} {:>5} {:>5} {:>5} {:>4} | {:>4} {:>4} {:>4} {:>4} {:>4} {:>4} | {:>7} {:>8.2}",
                format!("{name} {label}"),
                t.steps,
                t.ok,
                t.vanished,
                t.self_crossing,
                t.no_join,
                t.cusp,
                t.open,
                t.crossing,
                t.too_close,
                t.wrong_inside,
                t.area_jump,
                t.false_vanish,
                t.first_problem
                    .map_or("-".to_string(), |d| format!("{d:.2}")),
                t.worst_step_ms
            );
            total.steps += t.steps;
            total.ok += t.ok;
            total.self_crossing += t.self_crossing;
            total.no_join += t.no_join;
            total.open += t.open;
            total.crossing += t.crossing;
            total.too_close += t.too_close;
            total.wrong_inside += t.wrong_inside;
            total.area_jump += t.area_jump;
            total.false_vanish += t.false_vanish;
        }
    }
    println!(
        "Summe: {} Züge, {} ok, SelfCrossing {}, NoJoin {}, offen {}, Kreuzung {}, zu nah {}, Gitter {}, Fläche {}, falsch verschwunden {}",
        total.steps,
        total.ok,
        total.self_crossing,
        total.no_join,
        total.open,
        total.crossing,
        total.too_close,
        total.wrong_inside,
        total.area_jump,
        total.false_vanish
    );
    // Die Anti-Ziele: Was geliefert wird, ist geschlossen und kreuzt sich nicht, die Fläche
    // springt nicht, und nichts verschwindet, solange noch Platz ist.
    assert_eq!(total.open, 0);
    assert_eq!(total.crossing, 0);
    assert_eq!(total.area_jump, 0);
    assert_eq!(total.false_vanish, 0);
}
