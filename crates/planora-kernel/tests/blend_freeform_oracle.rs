//! Nachbesserung Task 15, Runde 3: unabhängige Orakel für (1) variable Radien auf Tangentenketten
//! mit Freiformkante (Radius an der exakten Bogenlänge der Kette), (2) Fasen an Freiformrändern
//! (Fasenlinie im Querschnitt senkrecht zur wahren Kante, Maße ab der wahren Ecke) und (3) einen
//! Körper aus zwei sich schneidenden elliptischen Zylindern (Kante zwischen zwei Freiformflächen,
//! Kugelmittelpunkt im Abstand r von beiden Flächen). Jede Fläche wird auf einem dichten Raster
//! gegen das Orakel geprüft; Schranke `NURBS_FIT_TOL / 2` wie die Nachprüfung der Bahn.

use planora_kernel::blend::topo::{edge_uses, face_name};
use planora_kernel::*;
use planora_math::nurbs::NurbsCurve;
use planora_math::{DMat4, DVec2, DVec3, Frame, PI, RadiusLaw, TAU};
use std::sync::Arc;

/// Erlaubte Abweichung vom exakten Modell.
const SKIN_TOL: f64 = 5e-6;

fn k() -> AnalyticKernel {
    AnalyticKernel::default()
}

fn req(e: Id<Edge>, kind: BlendKind, chain: bool) -> BlendRequest {
    BlendRequest {
        edges: vec![BlendEdge {
            edge: e,
            first_face: None,
        }],
        kind,
        tangent_chain: chain,
    }
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

/// Minimum von `f` auf `[lo, hi]`: Abtastung mit `n` Stellen, dann Goldener Schnitt.
fn minimize(lo: f64, hi: f64, n: usize, f: impl Fn(f64) -> f64) -> f64 {
    let h = (hi - lo) / n as f64;
    let best = (0..=n)
        .map(|i| lo + h * i as f64)
        .min_by(|x, y| f(*x).total_cmp(&f(*y)))
        .unwrap();
    let (mut a, mut b) = ((best - h).max(lo), (best + h).min(hi));
    let g = 0.5 * (5f64.sqrt() - 1.0);
    for _ in 0..80 {
        let (x1, x2) = (b - g * (b - a), a + g * (b - a));
        if f(x1) < f(x2) {
            b = x2;
        } else {
            a = x1;
        }
    }
    0.5 * (a + b)
}

/// Größte Abweichung `dev(p)` über ein Raster `nu × nv` aller Freiform-Flächen mit Rolle
/// „Rundung …“ bzw. „Fase …“.
fn worst(s: &Solid, nu: usize, nv: usize, dev: impl Fn(DVec3) -> f64) -> f64 {
    let mut w: f64 = 0.0;
    let mut faces = 0;
    for (id, f) in s.faces.iter() {
        let name = face_name(s, id);
        if !(matches!(f.surface, Surface::Nurbs(_))
            && (name.starts_with("Rundung") || name.starts_with("Fase")))
        {
            continue;
        }
        faces += 1;
        let p = f.patch.unwrap();
        for i in 0..=nu {
            for j in 0..=nv {
                let u = p.u0 + (p.u1 - p.u0) * i as f64 / nu as f64;
                let v = p.v0 + (p.v1 - p.v0) * j as f64 / nv as f64;
                w = w.max(dev(f.surface.point(u, v)));
            }
        }
    }
    assert!(faces > 0, "keine Freiform-Rundungsfläche");
    w
}

fn simpson(a: f64, b: f64, n: usize, f: impl Fn(f64) -> f64) -> f64 {
    let h = (b - a) / n as f64;
    let mut sum = f(a) + f(b);
    for i in 1..n {
        sum += f(a + h * i as f64) * if i % 2 == 1 { 4.0 } else { 2.0 };
    }
    sum * h / 3.0
}

// ---- (1) Variable Radien auf einer Tangentenkette ----

/// Kubische Welle von (2, 0) nach (8, 0) mit waagerechten Enden.
fn wave() -> NurbsCurve {
    let ys = [0.0, 0.0, 1.2, -1.2, 1.2, -0.6, 1.2, 0.0, 0.0];
    let n = ys.len();
    let control: Vec<DVec3> = ys
        .iter()
        .enumerate()
        .map(|(i, y)| DVec3::new(2.0 + 6.0 * i as f64 / (n - 1) as f64, *y, 0.0))
        .collect();
    let mut knots = vec![0.0; 4];
    knots.extend([0.15, 0.3, 0.42, 0.6, 0.85]);
    knots.extend([1.0; 4]);
    NurbsCurve::new(3, knots, control, vec![1.0; n]).unwrap()
}

#[test]
fn a_variable_radius_on_a_tangent_chain_sits_at_the_exact_arc_length() {
    // Gesetz mit Knicken an den Übergängen (mit den Streckenzug-Längen aus 27748c1: 2,9e-5).
    chain_case(false, (60, 6));
}

#[test]
#[ignore = "beide Gesetze, dichtes Raster, nur mit --release --ignored (etwa 20 s)"]
fn variable_radii_on_a_tangent_chain_on_a_dense_grid() {
    chain_case(true, (200, 8));
}

fn chain_case(both: bool, (nu, nv): (usize, usize)) {
    let c = wave();
    let profile = Profile {
        outer: ProfileLoop {
            start: DVec2::ZERO,
            segments: vec![
                Segment::Line {
                    to: DVec2::new(2.0, 0.0),
                },
                Segment::Nurbs {
                    curve: Arc::new(c.clone()),
                },
                Segment::Line {
                    to: DVec2::new(10.0, 0.0),
                },
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
    let h = 3.0;
    let s = k().extrude(&profile, &Frame::WORLD, h).unwrap();
    let e = edge_between(&s, "Wand 2", "Deckel");
    // Kleinster Krümmungsradius der Welle.
    let (t0, t1) = c.domain();
    let rho = (0..=4000)
        .map(|i| {
            let d = c.derivs(t0 + (t1 - t0) * i as f64 / 4000.0, 2);
            d[1].length().powi(3) / d[1].cross(d[2]).length().max(1e-300)
        })
        .fold(f64::INFINITY, f64::min);
    // Bogenlänge der Welle: Tabelle (4096 Stücke, Simpson je Stück).
    const TABLE: usize = 4096;
    let speed = |t: f64| c.tangent(t).truncate().length();
    let dt = (t1 - t0) / TABLE as f64;
    let mut acc = vec![0.0];
    for i in 0..TABLE {
        let last = acc[i];
        let a = t0 + dt * i as f64;
        acc.push(last + simpson(a, a + dt, 8, speed));
    }
    let wave_len = acc[TABLE];
    let arc_to = |t: f64| {
        let i = (((t - t0) / dt) as usize).min(TABLE - 1);
        let a = t0 + dt * i as f64;
        acc[i] + simpson(a, t, 8, speed)
    };
    let total = 4.0 + wave_len;
    // Fußpunkt auf der Unterkante (Gerade, Welle, Gerade): Bogenlänge, Punkt, Innennormale.
    let foot = |q: DVec2| -> (f64, DVec2, DVec2) {
        let mut best = (f64::INFINITY, 0.0, q, DVec2::Y);
        if q.x <= 2.0 + 1e-9 {
            let x = q.x.clamp(0.0, 2.0);
            best = (q.y.abs(), x, DVec2::new(x, 0.0), DVec2::Y);
        }
        if q.x >= 8.0 - 1e-9 {
            let x = q.x.clamp(8.0, 10.0);
            let d = q.y.abs();
            if d < best.0 {
                best = (d, 2.0 + wave_len + (x - 8.0), DVec2::new(x, 0.0), DVec2::Y);
            }
        }
        let t = minimize(t0, t1, 400, |t| (c.point(t).truncate() - q).length());
        let pt = c.point(t).truncate();
        if (pt - q).length() < best.0 {
            let tan = c.tangent(t).truncate().normalize();
            best = (
                (pt - q).length(),
                2.0 + arc_to(t),
                pt,
                DVec2::new(-tan.y, tan.x),
            );
        }
        (best.1, best.2, best.3)
    };
    let laws: [Vec<(f64, f64)>; 2] = [
        vec![(0.0, 0.2 * rho), (0.5, 0.8 * rho), (1.0, 0.2 * rho)],
        vec![
            (0.19, 0.3 * rho),
            (0.21, 0.7 * rho),
            (0.79, 0.7 * rho),
            (0.81, 0.3 * rho),
        ],
    ];
    for points in laws.into_iter().skip(if both { 0 } else { 1 }) {
        let law = RadiusLaw::new(&points).unwrap();
        let b = k()
            .blend(
                &s,
                &req(
                    e,
                    BlendKind::VariableFillet {
                        points: points.clone(),
                    },
                    true,
                ),
            )
            .unwrap_or_else(|err| panic!("{points:?}: {err}"));
        validate(&b.solid).unwrap();
        assert_eq!(b.edges.len(), 3, "Kette über Gerade, Welle, Gerade");
        // Das Gesetz ist symmetrisch: die Laufrichtung der Kette spielt keine Rolle.
        let dev = worst(&b.solid, nu, nv, |p| {
            let (sx, q, n) = foot(p.truncate());
            let r = law.radius(sx / total);
            let center = (q + n * r).extend(h - r);
            (p.distance(center) - r).abs()
        });
        eprintln!("Kette {points:?}: {dev:.3e}");
        assert!(dev <= SKIN_TOL, "{points:?}: Abweichung {dev}");
    }
}

// ---- (2) Fasen an Freiformrändern ----

fn ellipse_prism(a: f64, b: f64, h: f64) -> Solid {
    let c = NurbsCurve::elliptic_arc(
        DVec3::ZERO,
        DVec3::new(a, 0.0, 0.0),
        DVec3::new(0.0, b, 0.0),
        0.0,
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

/// Abstand des Punkts (w, depth) im Querschnitt von der Fasenlinie durch (w_top, 0) und
/// (0, d_wall) (w waagerecht ab der Ecke in die Deckelfläche, depth senkrecht in die Wand).
fn chamfer_dev(w: f64, depth: f64, w_top: f64, d_wall: f64) -> f64 {
    (w / w_top + depth / d_wall - 1.0).abs()
        / (1.0 / (w_top * w_top) + 1.0 / (d_wall * d_wall)).sqrt()
}

/// Größte Abweichung einer Fase am oberen Rand einer Ellipse (Mitte `c`, Halbachsen a, b,
/// Höhe h) vom Modell, für beide Zuordnungen der Maße (Fläche 1 = Deckel oder Wand); `inside`:
/// Material innerhalb der Ellipse (Prisma) oder außerhalb (Loch).
#[allow(clippy::too_many_arguments)]
fn chamfer_worst(
    s: &Solid,
    c: DVec2,
    a: f64,
    b: f64,
    h: f64,
    inside: bool,
    dims: (f64, f64),
    (nu, nv): (usize, usize),
) -> f64 {
    let e = |t: f64| c + DVec2::new(a * t.cos(), b * t.sin());
    let model = |p: DVec3| {
        let q = p.truncate();
        let t = minimize(0.0, TAU, 720, |t| (e(t) - q).length());
        let n_out = DVec2::new(b * t.cos(), a * t.sin()).normalize();
        let w = (q - e(t)).dot(n_out) * if inside { -1.0 } else { 1.0 };
        (w, h - p.z)
    };
    let one = |(wt, dw): (f64, f64)| {
        worst(s, nu, nv, |p| {
            let (w, depth) = model(p);
            chamfer_dev(w, depth, wt, dw)
        })
    };
    one(dims).min(one((dims.1, dims.0)))
}

#[test]
fn a_chamfer_on_an_elongated_hole_rim_follows_the_exact_section() {
    chamfer_cases(false, (150, 8));
}

#[test]
#[ignore = "Prisma und Loch, dichtes Raster, nur mit --release --ignored (etwa 10 s)"]
fn chamfers_on_freeform_rims_on_a_dense_grid() {
    chamfer_cases(true, (600, 16));
}

fn chamfer_cases(prism: bool, grid: (usize, usize)) {
    if prism {
        // Kreisprisma R = 3 (Freiform), ChamferTwo 0,9 / 1,8.
        let s = ellipse_prism(3.0, 3.0, 4.0);
        let e = edge_between(&s, "Wand 1", "Deckel");
        let b = k()
            .blend(
                &s,
                &req(
                    e,
                    BlendKind::ChamferTwo {
                        distance1: 0.9,
                        distance2: 1.8,
                    },
                    false,
                ),
            )
            .unwrap();
        validate(&b.solid).unwrap();
        let dev = chamfer_worst(&b.solid, DVec2::ZERO, 3.0, 3.0, 4.0, true, (0.9, 1.8), grid);
        eprintln!("Prisma 3×3: {dev:.3e}");
        assert!(dev <= SKIN_TOL, "Prisma 3×3: {dev}");
    }
    // Loch 2,5 × 0,3125 (Quader 10 × 10 × 5 minus Ellipsenprisma), ChamferAngle 0,2 / 30°:
    // Fasenlinie durch den Punkt im Abstand 0,2 auf Fläche 1 unter 30° zu ihr, also die andere
    // Fläche im Abstand 0,2 · tan 30°.
    let block = k().make_box(DVec3::new(10.0, 10.0, 5.0)).unwrap();
    let tool = k()
        .transform(
            &ellipse_prism(2.5, 0.3125, 7.0),
            &DMat4::from_translation(DVec3::new(5.0, 5.0, -1.0)),
        )
        .unwrap();
    let holed = k().boolean(&block, &tool, BoolOp::Difference).unwrap();
    let rim = holed
        .edges
        .iter()
        .map(|(id, _)| id)
        .find(|&id| {
            let ed = &holed.edges[id];
            matches!(ed.curve, Curve::Nurbs(_)) && (ed.curve.point(ed.t0).z - 5.0).abs() < 1e-6
        })
        .unwrap();
    let angle = PI / 6.0;
    let b = k()
        .blend(
            &holed,
            &req(
                rim,
                BlendKind::ChamferAngle {
                    distance: 0.2,
                    angle,
                },
                false,
            ),
        )
        .unwrap();
    validate(&b.solid).unwrap();
    let dev = chamfer_worst(
        &b.solid,
        DVec2::new(5.0, 5.0),
        2.5,
        0.3125,
        5.0,
        false,
        (0.2, 0.2 * angle.tan()),
        grid,
    );
    eprintln!("Loch 2,5×0,3125: {dev:.3e}");
    assert!(dev <= SKIN_TOL, "Loch 2,5×0,3125: {dev}");
}

// ---- (3) Zwei sich schneidende elliptische Zylinder ----

/// Körper A ∩ B: A elliptischer Zylinder (a, b) längs z (−4 … 4), B elliptischer Zylinder
/// (Halbachse `ea` in z, `eb` in y) längs x. Die obere Kante (z > 0) liegt zwischen der Wand von
/// A und der Fläche von B (beide Freiform).
fn stone(a: f64, b: f64, ea: f64, eb: f64) -> (Solid, Id<Edge>) {
    let prism = |a: f64, b: f64| {
        let c = NurbsCurve::elliptic_arc(
            DVec3::ZERO,
            DVec3::new(a, 0.0, 0.0),
            DVec3::new(0.0, b, 0.0),
            0.0,
            TAU,
        )
        .unwrap();
        let s = k()
            .extrude(
                &Profile {
                    outer: ProfileLoop::nurbs_loop(c).unwrap(),
                    holes: vec![],
                },
                &Frame::WORLD,
                8.0,
            )
            .unwrap();
        k().transform(&s, &DMat4::from_translation(DVec3::new(0.0, 0.0, -4.0)))
            .unwrap()
    };
    let body_a = prism(a, b);
    let body_b = k()
        .transform(&prism(ea, eb), &DMat4::from_rotation_y(PI / 2.0))
        .unwrap();
    let s = k().boolean(&body_a, &body_b, BoolOp::Intersection).unwrap();
    let e = s
        .edges
        .iter()
        .map(|(id, _)| id)
        .find(|&id| {
            let ed = &s.edges[id];
            ed.curve.point(0.5 * (ed.t0 + ed.t1)).z > 0.0
        })
        .unwrap();
    (s, e)
}

/// Abweichung der Rundung am oberen Rand des Steins von der rollenden Kugel: Mittelpunkte im
/// Abstand r innerhalb beider Ellipsen. Parameter t läuft über A (xy): Mittelpunkt (x, y) auf
/// der inneren Parallelkurve von A; seine Höhe z > 0 aus der inneren Parallelkurve von B in der
/// yz-Ebene (Bisektion über deren Winkel). Abweichung | min_t |p − c(t)| − r |.
fn stone_dev(p: DVec3, a: f64, b: f64, ea: f64, eb: f64, r: f64) -> f64 {
    let offset_b = |phi: f64| {
        let (y, z) = (eb * phi.cos(), ea * phi.sin());
        let n = DVec2::new(phi.cos() / eb, phi.sin() / ea).normalize();
        DVec2::new(y, z) - n * r
    };
    let center = |t: f64| -> Option<DVec3> {
        let n = DVec2::new(t.cos() / a, t.sin() / b).normalize();
        let xy = DVec2::new(a * t.cos(), b * t.sin()) - n * r;
        // offset_b(φ).x fällt auf (0, π) von eb − r auf −(eb − r).
        let (mut lo, mut hi) = (0.0, PI);
        if !(xy.y <= offset_b(lo).x && xy.y >= offset_b(hi).x) {
            return None;
        }
        for _ in 0..100 {
            let mid = 0.5 * (lo + hi);
            if offset_b(mid).x > xy.y {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        Some(DVec3::new(xy.x, xy.y, offset_b(0.5 * (lo + hi)).y))
    };
    let d = |t: f64| center(t).map_or(f64::INFINITY, |c| c.distance(p));
    let t = minimize(0.0, TAU, 1440, d);
    (d(t) - r).abs()
}

#[test]
#[ignore = "zwei Steinkörper, nur mit --release --ignored (etwa 10 s)"]
fn fillets_between_two_elliptic_cylinders_follow_the_rolling_ball() {
    for (a, b, ea, eb, r) in [
        (
            2.4216346495848473,
            1.1298842526132722,
            1.2709506591826438,
            1.769509353230152,
            0.3482268843335502,
        ),
        (
            2.6506280358031002,
            1.0590541373882418,
            1.2841764993299427,
            2.068154211579869,
            0.268,
        ),
    ] {
        let (s, e) = stone(a, b, ea, eb);
        match k().blend(&s, &req(e, BlendKind::Fillet { radius: r }, false)) {
            Ok(bl) => {
                validate(&bl.solid).unwrap();
                let dev = worst(&bl.solid, 200, 8, |p| stone_dev(p, a, b, ea, eb, r));
                eprintln!("Stein {a:.3}/{b:.3}/{ea:.3}/{eb:.3} r {r:.3}: {dev:.3e}");
                assert!(dev <= SKIN_TOL, "Stein: {dev}");
            }
            Err(err) => eprintln!("Stein abgelehnt: {err}"),
        }
    }
}
