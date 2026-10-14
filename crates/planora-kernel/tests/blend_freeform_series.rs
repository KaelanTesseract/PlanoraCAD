//! Zufallsreihe zur Nachbesserung Task 15 (Review I-1, I-2): Anteil der `Ok` an Ellipsenrändern
//! (Seitenverhältnis 1 bis 6, beliebiger Anfangswinkel), Spline-Wänden mit inneren Knoten und
//! Booleschen Ellipsen-Durchbrüchen, jeweils unter dem kleinsten Krümmungsradius. Jedes `Ok` muss
//! gültig sein und die Schätzung des Kernels der geschlossenen Formel folgen (Steiner bzw. Band,
//! Querschnitte von geraden Flächenstücken begrenzt, also exakt); Ablehnungen werden gezählt und
//! nach Meldung ausgegeben. Fester Startwert, nur Release.

use planora_kernel::blend::topo::{edge_uses, face_name};
use planora_kernel::*;
use planora_math::nurbs::NurbsCurve;
use planora_math::{DMat4, DVec2, DVec3, Frame, PI, TAU};
use std::collections::BTreeMap;
use std::sync::Arc;

/// Relative Schranke der Schätzung gegen die Formel.
const ESTIMATE_REL: f64 = 1e-4;
/// Anpassungsgenauigkeit Boolescher Schnittkanten (`NURBS_FIT_TOL`).
const FIT_TOL: f64 = 1e-5;
const A: f64 = 1.0 - PI / 4.0;

struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> f64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (self.0 >> 11) as f64 / (1u64 << 53) as f64
    }
    fn range(&mut self, a: f64, b: f64) -> f64 {
        a + (b - a) * self.next()
    }
}

fn k() -> AnalyticKernel {
    AnalyticKernel::default()
}

fn edge_between(s: &Solid, a: &str, b: &str) -> Id<Edge> {
    s.edges
        .iter()
        .map(|(id, _)| id)
        .find(|&id| {
            let n: Vec<String> = edge_uses(s, id)
                .iter()
                .map(|u| face_name(s, u.face))
                .collect();
            n.len() == 2 && ((n[0] == a && n[1] == b) || (n[0] == b && n[1] == a))
        })
        .unwrap()
}

fn req(e: Id<Edge>, kind: BlendKind) -> BlendRequest {
    BlendRequest {
        edges: vec![BlendEdge {
            edge: e,
            first_face: None,
        }],
        kind,
        tangent_chain: false,
    }
}

fn simpson(a: f64, b: f64, n: usize, f: impl Fn(f64) -> f64) -> f64 {
    let h = (b - a) / n as f64;
    let mut sum = f(a) + f(b);
    for i in 1..n {
        sum += f(a + h * i as f64) * if i % 2 == 1 { 4.0 } else { 2.0 };
    }
    sum * h / 3.0
}

fn perimeter(a: f64, b: f64) -> f64 {
    simpson(0.0, TAU, 40_000, |t| {
        (a * a * t.sin().powi(2) + b * b * t.cos().powi(2)).sqrt()
    })
}

fn ellipse_prism(a: f64, b: f64, start: f64, h: f64) -> Solid {
    let c = NurbsCurve::elliptic_arc(
        DVec3::ZERO,
        DVec3::new(a, 0.0, 0.0),
        DVec3::new(0.0, b, 0.0),
        start,
        TAU,
    )
    .unwrap();
    k().extrude(
        &Profile {
            outer: ProfileLoop::nurbs_loop(c).unwrap(),
            holes: vec![],
        },
        &Frame::WORLD,
        h,
    )
    .unwrap()
}

#[derive(Default)]
struct Tally {
    total: usize,
    ok: usize,
    wrong: usize,
    refusals: BTreeMap<String, usize>,
}

impl Tally {
    /// `fitted`: Kantenkurve eine angepasste Boolesche Schnittkante mit Umfang `fitted`; die
    /// Schätzung nimmt den Kantenpunkt als Ecke des Querschnitts, der bis `FIT_TOL` neben der
    /// wahren Ecke liegt (Spielraum `Umfang · FIT_TOL · 2 · Maß`, bei kleinsten Maßen maßgebend;
    /// die Lage der Fläche selbst prüft `blend_freeform_seam.rs` punktweise).
    fn run(&mut self, s: &Solid, e: Id<Edge>, kind: BlendKind, want: f64, what: &str) {
        self.run_fitted(s, e, kind, want, 0.0, what);
    }

    fn run_fitted(
        &mut self,
        s: &Solid,
        e: Id<Edge>,
        kind: BlendKind,
        want: f64,
        fitted: f64,
        what: &str,
    ) {
        let size = match &kind {
            BlendKind::Fillet { radius } => *radius,
            BlendKind::ChamferEqual { distance } => *distance,
            _ => 0.0,
        };
        self.total += 1;
        match k().blend(s, &req(e, kind)) {
            Ok(b) => {
                self.ok += 1;
                let valid = validate(&b.solid).is_ok();
                let exact = (b.volume_change + want).abs()
                    <= ESTIMATE_REL * want + fitted * FIT_TOL * 2.0 * size;
                if !(valid && exact) {
                    self.wrong += 1;
                    eprintln!(
                        "FALSCH {what}: gültig {valid}, Schätzung {} statt {}",
                        b.volume_change, -want
                    );
                }
            }
            Err(err) => {
                let m = err.to_string();
                eprintln!("  abgelehnt {what}: {m}");
                let key: String = m.chars().take(60).collect();
                *self.refusals.entry(key).or_default() += 1;
            }
        }
    }

    /// Ohne Formel (variabler Radius): nur `validate`.
    fn run_valid(&mut self, s: &Solid, e: Id<Edge>, kind: BlendKind, what: &str) {
        self.total += 1;
        match k().blend(s, &req(e, kind)) {
            Ok(b) => {
                self.ok += 1;
                if validate(&b.solid).is_err() {
                    self.wrong += 1;
                    eprintln!("FALSCH {what}: ungültig");
                }
            }
            Err(err) => {
                let m = err.to_string();
                eprintln!("  abgelehnt {what}: {m}");
                let key: String = m.chars().take(60).collect();
                *self.refusals.entry(key).or_default() += 1;
            }
        }
    }

    fn report(&self, name: &str) {
        eprintln!(
            "{name}: Ok {}/{} , falsch {}",
            self.ok, self.total, self.wrong
        );
        for (m, n) in &self.refusals {
            eprintln!("    {n} × {m}");
        }
    }
}

fn wavy(rng: &mut Lcg) -> NurbsCurve {
    let amp = rng.range(0.2, 1.0);
    let n = 9;
    let mut ys = vec![0.0, 0.0];
    for _ in 0..n - 4 {
        ys.push(rng.range(-amp, amp));
    }
    ys.extend([0.0, 0.0]);
    let control: Vec<DVec3> = ys
        .iter()
        .enumerate()
        .map(|(i, y)| DVec3::new(10.0 * i as f64 / (n - 1) as f64, *y, 0.0))
        .collect();
    let mut inner: Vec<f64> = (0..n - 4).map(|_| rng.range(0.05, 0.95)).collect();
    inner.sort_by(f64::total_cmp);
    let mut knots = vec![0.0; 4];
    knots.extend(inner);
    knots.extend([1.0; 4]);
    NurbsCurve::new(3, knots, control, vec![1.0; n]).unwrap()
}

fn min_rho(c: &NurbsCurve) -> f64 {
    let (t0, t1) = c.domain();
    (0..=4000)
        .map(|i| {
            let d = c.derivs(t0 + (t1 - t0) * i as f64 / 4000.0, 2);
            d[1].length().powi(3) / d[1].cross(d[2]).length().max(1e-300)
        })
        .fold(f64::INFINITY, f64::min)
}

#[test]
#[ignore = "Zufallsreihe, nur mit --release --ignored (etwa 5 min)"]
fn freeform_ok_rates() {
    let mut rng = Lcg(20261013);
    // Ellipsenränder.
    let mut t = Tally::default();
    for i in 0..60 {
        let aspect = 1.0 + 5.0 * rng.next();
        let a = rng.range(3.0, 6.0);
        let b = a / aspect;
        let rho = b * b / a;
        let h = rng.range(1.0, 4.0);
        let s = ellipse_prism(a, b, rng.range(0.0, TAU), h);
        let e = edge_between(&s, "Wand 1", "Deckel");
        let p = perimeter(a, b);
        let size = rng.range(0.05, 0.9) * rho.min(0.9 * h);
        if i % 3 == 2 {
            t.run(
                &s,
                e,
                BlendKind::ChamferEqual { distance: size },
                p * size * size / 2.0 - PI * size.powi(3) / 3.0,
                &format!("Ellipse {a:.3}×{b:.3} Fase {size:.4}"),
            );
        } else {
            t.run(
                &s,
                e,
                BlendKind::Fillet { radius: size },
                (p - TAU * size) * size * size * A + PI * size.powi(3) / 3.0,
                &format!("Ellipse {a:.3}×{b:.3} r {size:.4}"),
            );
        }
    }
    t.report("Ellipsenränder");
    // Spline-Wände (Rand Wand 1 / Deckel, waagerechte Enden: ΔV = L r²(1 − π/4) bzw. L d²/2).
    let mut t = Tally::default();
    for i in 0..40 {
        let c = wavy(&mut rng);
        let (t0, t1) = c.domain();
        let len = simpson(t0, t1, 40_000, |x| c.tangent(x).length());
        let rho = min_rho(&c);
        let profile = Profile {
            outer: ProfileLoop {
                start: DVec2::ZERO,
                segments: vec![
                    Segment::Nurbs { curve: Arc::new(c) },
                    Segment::Line {
                        to: DVec2::new(10.0, 6.0),
                    },
                    Segment::Line {
                        to: DVec2::new(0.0, 6.0),
                    },
                    Segment::Line { to: DVec2::ZERO },
                ],
            },
            holes: vec![],
        };
        let Ok(s) = k().extrude(&profile, &Frame::WORLD, 3.0) else {
            continue;
        };
        let e = edge_between(&s, "Wand 1", "Deckel");
        let size = rng.range(0.05, 0.9) * rho.min(1.0);
        if i % 3 == 2 {
            t.run(
                &s,
                e,
                BlendKind::ChamferEqual { distance: size },
                len * size * size / 2.0,
                &format!("Spline Fase {size:.4}"),
            );
        } else {
            t.run(
                &s,
                e,
                BlendKind::Fillet { radius: size },
                len * size * size * A,
                &format!("Spline r {size:.4}"),
            );
        }
    }
    t.report("Spline-Ränder");
    // Quader minus Ellipse (Durchbruch), oberer Lochrand: Loch wächst,
    // ΔV = P r²(1 − π/4) + π r³ (5/3 − π/2).
    let mut t = Tally::default();
    for _ in 0..30 {
        let a = rng.range(1.0, 3.0);
        let b = a / (1.0 + 2.0 * rng.next());
        let block = k().make_box(DVec3::new(10.0, 10.0, 5.0)).unwrap();
        let tool = k()
            .transform(
                &ellipse_prism(a, b, 0.0, 7.0),
                &DMat4::from_translation(DVec3::new(5.0, 5.0, -1.0)),
            )
            .unwrap();
        let Ok(s) = k().boolean(&block, &tool, BoolOp::Difference) else {
            continue;
        };
        let e = s
            .edges
            .iter()
            .map(|(id, _)| id)
            .find(|&id| {
                let ed = &s.edges[id];
                matches!(ed.curve, Curve::Nurbs(_)) && (ed.curve.point(ed.t0).z - 5.0).abs() < 1e-6
            })
            .unwrap();
        let r = rng.range(0.05, 0.6);
        let p = perimeter(a, b);
        t.run_fitted(
            &s,
            e,
            BlendKind::Fillet { radius: r },
            p * r * r * A + PI * r.powi(3) * (5.0 / 3.0 - PI / 2.0),
            p,
            &format!("Loch {a:.3}×{b:.3} r {r:.4}"),
        );
    }
    t.report("Quader minus Ellipse");
    // Weitere Lochränder: Fasen (Loch wächst um τ: ΔV = P d²/2 + π d³/3), variable Radien
    // (geschlossen, gleiche Endradien; nur `validate`) und sehr kleine Maße.
    let mut chamfers = Tally::default();
    let mut variable = Tally::default();
    let mut tiny = Tally::default();
    for i in 0..60 {
        let a = rng.range(1.0, 3.0);
        let b = a / (1.0 + 4.0 * rng.next());
        let block = k().make_box(DVec3::new(10.0, 10.0, 5.0)).unwrap();
        let tool = k()
            .transform(
                &ellipse_prism(a, b, 0.0, 7.0),
                &DMat4::from_translation(DVec3::new(5.0, 5.0, -1.0)),
            )
            .unwrap();
        let Ok(s) = k().boolean(&block, &tool, BoolOp::Difference) else {
            continue;
        };
        let e = s
            .edges
            .iter()
            .map(|(id, _)| id)
            .find(|&id| {
                let ed = &s.edges[id];
                matches!(ed.curve, Curve::Nurbs(_)) && (ed.curve.point(ed.t0).z - 5.0).abs() < 1e-6
            })
            .unwrap();
        let p = perimeter(a, b);
        match i % 3 {
            0 => {
                let d = rng.range(0.05, 0.6);
                chamfers.run_fitted(
                    &s,
                    e,
                    BlendKind::ChamferEqual { distance: d },
                    p * d * d / 2.0 + PI * d.powi(3) / 3.0,
                    p,
                    &format!("Loch {a:.3}×{b:.3} Fase {d:.4}"),
                );
            }
            1 => {
                let (r0, r1) = (rng.range(0.05, 0.3), rng.range(0.1, 0.6));
                let f = rng.range(0.1, 0.4);
                variable.run_valid(
                    &s,
                    e,
                    BlendKind::VariableFillet {
                        points: vec![(0.0, r0), (f, r1), (1.0 - f, r1), (1.0, r0)],
                    },
                    &format!("Loch {a:.3}×{b:.3} variabel {r0:.3}/{r1:.3}/{f:.3}"),
                );
            }
            _ => {
                let r = rng.range(0.002, 0.015);
                tiny.run_fitted(
                    &s,
                    e,
                    BlendKind::Fillet { radius: r },
                    p * r * r * A + PI * r.powi(3) * (5.0 / 3.0 - PI / 2.0),
                    p,
                    &format!("Loch {a:.3}×{b:.3} klein r {r:.4}"),
                );
            }
        }
    }
    chamfers.report("Lochrand-Fasen");
    variable.report("Lochrand variabel");
    tiny.report("Lochrand kleine Radien");
}

/// Quader 10 × 10 × 5 minus elliptischer Zylinder ab der Höhe `z0` (Durchbruch bei z0 < 0,
/// sonst Sackloch) und seine obere Randkante.
fn holed(a: f64, b: f64, z0: f64) -> (Solid, Id<Edge>) {
    let block = k().make_box(DVec3::new(10.0, 10.0, 5.0)).unwrap();
    let tool = k()
        .transform(
            &ellipse_prism(a, b, 0.0, 7.0),
            &DMat4::from_translation(DVec3::new(5.0, 5.0, z0)),
        )
        .unwrap();
    let s = k().boolean(&block, &tool, BoolOp::Difference).unwrap();
    let e = s
        .edges
        .iter()
        .map(|(id, _)| id)
        .find(|&id| {
            let ed = &s.edges[id];
            matches!(ed.curve, Curve::Nurbs(_)) && (ed.curve.point(ed.t0).z - 5.0).abs() < 1e-6
        })
        .unwrap();
    (s, e)
}

#[test]
#[ignore = "Konstruktionen des Reviews (Runde 2), nur mit --release --ignored (etwa 70 s)"]
fn review_round_two_constructions() {
    let mut t = Tally::default();
    // Vernähen scheiterte im ganzen Bereich r 0,356 … 0,435 (Loch 2,589 × 1,128).
    let (s, e) = holed(2.589, 1.128, -1.0);
    let p = perimeter(2.589, 1.128);
    for r in [0.356, 0.38, 0.3954, 0.41, 0.435] {
        t.run_fitted(
            &s,
            e,
            BlendKind::Fillet { radius: r },
            p * r * r * A + PI * r.powi(3) * (5.0 / 3.0 - PI / 2.0),
            p,
            &format!("Loch 2,589×1,128 r {r}"),
        );
    }
    // Fasen an gestreckten Durchbrüchen und an Sacklöchern (Tiefe 2).
    for (a, b, z0) in [
        (2.0, 0.4, -1.0),
        (3.0, 0.5, -1.0),
        (1.4726, 0.2267, -1.0),
        (2.0, 0.4, 3.0),
        (3.0, 1.0, 3.0),
    ] {
        let (s, e) = holed(a, b, z0);
        let p = perimeter(a, b);
        for d in [0.1, 0.3] {
            t.run_fitted(
                &s,
                e,
                BlendKind::ChamferEqual { distance: d },
                p * d * d / 2.0 + PI * d.powi(3) / 3.0,
                p,
                &format!("Loch {a}×{b} ab {z0} Fase {d}"),
            );
        }
        // Sehr kleine Radien (vorher teils „zu groß für Deckel“).
        for r in [0.002, 0.005, 0.015] {
            t.run_fitted(
                &s,
                e,
                BlendKind::Fillet { radius: r },
                p * r * r * A + PI * r.powi(3) * (5.0 / 3.0 - PI / 2.0),
                p,
                &format!("Loch {a}×{b} ab {z0} r {r}"),
            );
        }
        // Variable Radien bis 0,6 (vorher teils „Wand 1 liegt im Bereich der Rundung“).
        t.run_valid(
            &s,
            e,
            BlendKind::VariableFillet {
                points: vec![(0.0, 0.25), (0.3, 0.6), (0.7, 0.6), (1.0, 0.25)],
            },
            &format!("Loch {a}×{b} ab {z0} variabel"),
        );
    }
    t.report("Review Runde 2");
}
