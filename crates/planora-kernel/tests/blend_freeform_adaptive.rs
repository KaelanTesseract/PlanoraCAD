//! Rundungen und Fasen an Freiformkanten, Nachbesserung Task 15 (Review I-1, I-2): Startwert
//! jedes Querschnitts nach der Ebenenformel (kein ferner Kugelort aus der Nachbarstation),
//! Stationen an den Knoten der Kante und angepasste Verfeinerung statt gleichmäßiger
//! Verdopplung. Orakel: geschlossene Formeln (Steiner bzw. Band entlang der Kurve) gegen die
//! Volumenschätzung des Kernels, die bei geraden Querschnittsrändern exakt ist (Simpson über
//! exakte Querschnitte), dazu `validate` und punktweise Lage der Hautfläche.

use planora_kernel::blend::topo::{edge_uses, face_name};
use planora_kernel::*;
use planora_math::nurbs::NurbsCurve;
use planora_math::{DMat4, DVec2, DVec3, Frame, PI, TAU};
use std::sync::Arc;

/// Relative Schranke der Schätzung gegen die Formel (gemessen ≤ 1e-6 in Task 15).
const ESTIMATE_REL: f64 = 1e-4;
/// Querschnittsfläche der Rundung mit Radius 1 an einer rechtwinkligen Kante.
const A: f64 = 1.0 - PI / 4.0;

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
        .unwrap_or_else(|| panic!("keine Kante {a}/{b}"))
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

/// Extrudierte Ellipse mit Halbachsen `a`, `b`, Anfangswinkel `start` der Kurve, Höhe `h`.
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

/// Simpson über `[a, b]` mit `n` (gerade) Abschnitten.
fn simpson(a: f64, b: f64, n: usize, f: impl Fn(f64) -> f64) -> f64 {
    let h = (b - a) / n as f64;
    let mut sum = f(a) + f(b);
    for i in 1..n {
        sum += f(a + h * i as f64) * if i % 2 == 1 { 4.0 } else { 2.0 };
    }
    sum * h / 3.0
}

/// Umfang der Ellipse (Simpson, glatt und periodisch).
fn ellipse_perimeter(a: f64, b: f64) -> f64 {
    simpson(0.0, TAU, 40_000, |t| {
        (a * a * t.sin().powi(2) + b * b * t.cos().powi(2)).sqrt()
    })
}

/// Steiner-Formel: abgetragenes Volumen einer Rundung `r` am Außenrand eines konvexen Prismas
/// (Umfang `p`), einer Fase `d` ebenso, und einer Rundung am Rand eines konvexen Durchbruchs
/// (dort wächst das Loch: Fläche `(r − ρ) P + π (r − ρ)²` je Höhe).
fn rim_fillet(p: f64, r: f64) -> f64 {
    (p - TAU * r) * r * r * A + PI * r * r * r / 3.0
}
fn rim_chamfer(p: f64, d: f64) -> f64 {
    p * d * d / 2.0 - PI * d * d * d / 3.0
}
fn hole_fillet(p: f64, r: f64) -> f64 {
    p * r * r * A + PI * r * r * r * (5.0 / 3.0 - PI / 2.0)
}

/// Ok, gültig, Schätzung nach Formel (`want` > 0: abgetragen).
fn check(s: &Solid, e: Id<Edge>, kind: BlendKind, want: f64, what: &str) -> Blended {
    let b = k()
        .blend(s, &req(e, kind))
        .unwrap_or_else(|err| panic!("{what}: {err}"));
    validate(&b.solid).unwrap_or_else(|err| panic!("{what}: {err}"));
    assert!(
        (b.volume_change + want).abs() <= ESTIMATE_REL * want,
        "{what}: Schätzung {} statt {}",
        b.volume_change,
        -want
    );
    b
}

/// Abstand von `p` zur ebenen Kurve `c(t)` (t periodisch in [0, 2π)) in der Höhe `z`: dichte
/// Abtastung und Goldener Schnitt um die beste Stelle.
fn distance_to_curve(p: DVec3, z: f64, c: impl Fn(f64) -> DVec2) -> f64 {
    const N: usize = 4000;
    let d = |t: f64| (c(t) - p.truncate()).length().hypot(p.z - z);
    let h = TAU / N as f64;
    let best = (0..N)
        .map(|i| i as f64 * h)
        .min_by(|a, b| d(*a).total_cmp(&d(*b)))
        .unwrap();
    let (mut lo, mut hi) = (best - h, best + h);
    let g = 0.5 * (5f64.sqrt() - 1.0);
    for _ in 0..100 {
        let (x1, x2) = (hi - g * (hi - lo), lo + g * (hi - lo));
        if d(x1) < d(x2) {
            hi = x2;
        } else {
            lo = x1;
        }
    }
    d(0.5 * (lo + hi))
}

/// Größte Abweichung der Rundungsfläche „Rundung 1“ von der Kugelbahn: jeder Punkt eines
/// Rasters 61 × 11 hat vom Mittelpunkt (Kurve `center` in der Höhe `z`) den Abstand `r`.
fn skin_deviation(s: &Solid, z: f64, r: f64, center: impl Fn(f64) -> DVec2) -> f64 {
    let (_, f) = s
        .faces
        .iter()
        .find(|(id, f)| matches!(f.surface, Surface::Nurbs(_)) && face_name(s, *id) == "Rundung 1")
        .unwrap();
    let patch = f.patch.unwrap();
    let mut worst: f64 = 0.0;
    for i in 0..=60 {
        for j in 0..=10 {
            let u = patch.u0 + (patch.u1 - patch.u0) * i as f64 / 60.0;
            let v = patch.v0 + (patch.v1 - patch.v0) * j as f64 / 10.0;
            let p = f.surface.point(u, v);
            worst = worst.max((distance_to_curve(p, z, &center) - r).abs());
        }
    }
    worst
}

/// Erlaubte Abweichung der Rundungsfläche von der unabhängig gerechneten Kugelbahn.
const SKIN_CHECK: f64 = 1e-5;

#[test]
fn elongated_ellipse_rims_no_longer_lose_the_ball() {
    // Review-Wiederholung 1: Ellipse 4 × 2 ab Winkel 0,3, Höhe 3, r = 0,3 (ρ_min = b²/a = 1).
    let s = ellipse_prism(4.0, 2.0, 0.3, 3.0);
    let e = edge_between(&s, "Wand 1", "Deckel");
    let p = ellipse_perimeter(4.0, 2.0);
    check(
        &s,
        e,
        BlendKind::Fillet { radius: 0.3 },
        rim_fillet(p, 0.3),
        "4×2 r 0,3",
    );
    // Wiederholung 2: Ellipse 4 × 1 (ρ_min = 0,25), alle r von 0,01 bis 0,2.
    let s = ellipse_prism(4.0, 1.0, 0.0, 3.0);
    let e = edge_between(&s, "Wand 1", "Deckel");
    let p = ellipse_perimeter(4.0, 1.0);
    for r in [0.01, 0.05, 0.1, 0.15, 0.2] {
        let b = check(
            &s,
            e,
            BlendKind::Fillet { radius: r },
            rim_fillet(p, r),
            &format!("4×1 r {r}"),
        );
        if r == 0.2 {
            // Kugelmittelpunkt auf der inneren Parallelkurve im Abstand r, Höhe 3 − r.
            let center = |t: f64| {
                let n = DVec2::new(1.0 * t.cos(), 4.0 * t.sin()).normalize();
                DVec2::new(4.0 * t.cos(), t.sin()) - n * r
            };
            let worst = skin_deviation(&b.solid, 3.0 - r, r, center);
            assert!(worst < SKIN_CHECK, "4×1 r 0,2: {worst}");
        }
    }
}

/// Ellipsen 6 × 6/k (k Seitenverhältnis) ab Winkel 0, Höhe 2, Rundungen 0,3 ρ und 0,7 ρ
/// (ρ = b²/a kleinster Krümmungsradius) nach der Steiner-Formel.
fn aspects(list: &[f64]) {
    for &aspect in list {
        let (a, b) = (6.0, 6.0 / aspect);
        let rho = b * b / a;
        let s = ellipse_prism(a, b, 0.0, 2.0);
        let e = edge_between(&s, "Wand 1", "Deckel");
        let p = ellipse_perimeter(a, b);
        for f in [0.3, 0.7] {
            let r = f * rho;
            check(
                &s,
                e,
                BlendKind::Fillet { radius: r },
                rim_fillet(p, r),
                &format!("Seitenverhältnis {aspect}, r {r}"),
            );
        }
    }
}

#[test]
fn ellipse_aspect_six_rounds_at_three_and_seven_tenths_of_the_curvature_radius() {
    // Seitenverhältnis 2 deckt `elongated_ellipse_rims_no_longer_lose_the_ball` (4 × 2) ab.
    aspects(&[6.0]);
}

#[test]
#[ignore = "ganze Reihe Seitenverhältnis 2 bis 6, nur mit --release --ignored (etwa 7 s)"]
fn ellipse_aspects_two_to_six_round_at_three_and_seven_tenths_of_the_curvature_radius() {
    aspects(&[2.0, 3.0, 4.0, 5.0, 6.0]);
}

#[test]
fn chamfers_on_an_eight_by_two_ellipse() {
    let s = ellipse_prism(8.0, 2.0, 0.0, 2.0);
    let e = edge_between(&s, "Wand 1", "Deckel");
    let p = ellipse_perimeter(8.0, 2.0);
    for d in [0.15, 0.4] {
        check(
            &s,
            e,
            BlendKind::ChamferEqual { distance: d },
            rim_chamfer(p, d),
            &format!("8×2 Fase {d}"),
        );
    }
}

/// Quader 10 × 10 × 5 minus ein durchgehender elliptischer Zylinder (Mitte (5, 5)).
fn box_minus_ellipse(a: f64, b: f64) -> Solid {
    let block = k().make_box(DVec3::new(10.0, 10.0, 5.0)).unwrap();
    let tool = k()
        .transform(
            &ellipse_prism(a, b, 0.0, 7.0),
            &DMat4::from_translation(DVec3::new(5.0, 5.0, -1.0)),
        )
        .unwrap();
    k().boolean(&block, &tool, BoolOp::Difference).unwrap()
}

/// Obere Randkante des Durchbruchs (Freiformkante bei z = 5).
fn hole_rim(s: &Solid) -> Id<Edge> {
    s.edges
        .iter()
        .map(|(id, _)| id)
        .find(|&id| {
            let e = &s.edges[id];
            matches!(e.curve, Curve::Nurbs(_)) && (e.curve.point(e.t0).z - 5.0).abs() < 1e-6
        })
        .expect("Randkante des Durchbruchs")
}

#[test]
fn hole_rims_of_boolean_freeform_holes_round() {
    // Review I-2 (a): Die Randkante ist eine angepasste kubische NURBS-Kurve mit vielen Knoten.
    for (a, b) in [(2.0, 2.0), (2.0, 1.5)] {
        let s = box_minus_ellipse(a, b);
        let e = hole_rim(&s);
        let p = ellipse_perimeter(a, b);
        for r in [0.2, 0.5] {
            let bl = check(
                &s,
                e,
                BlendKind::Fillet { radius: r },
                hole_fillet(p, r),
                &format!("Loch {a}×{b} r {r}"),
            );
            // Kugelmittelpunkt auf der äußeren Parallelkurve (im Material), Höhe 5 − r.
            let center = |t: f64| {
                let n = DVec2::new(b * t.cos(), a * t.sin()).normalize();
                DVec2::new(5.0 + a * t.cos(), 5.0 + b * t.sin()) + n * r
            };
            let worst = skin_deviation(&bl.solid, 5.0 - r, r, center);
            assert!(worst < SKIN_CHECK, "Loch {a}×{b} r {r}: {worst}");
        }
    }
    // Analytischer Zwilling (Kreisloch r = 2): gleiche Schätzung bis auf die Formelgenauigkeit.
    let block = k().make_box(DVec3::new(10.0, 10.0, 5.0)).unwrap();
    let tool = k()
        .transform(
            &k().make_cylinder(2.0, 7.0).unwrap(),
            &DMat4::from_translation(DVec3::new(5.0, 5.0, -1.0)),
        )
        .unwrap();
    let twin = k().boolean(&block, &tool, BoolOp::Difference).unwrap();
    let et = twin
        .edges
        .iter()
        .map(|(id, _)| id)
        .find(|&id| {
            let e = &twin.edges[id];
            matches!(e.curve, Curve::Circle { .. }) && (e.curve.point(e.t0).z - 5.0).abs() < 1e-6
        })
        .unwrap();
    let bt = k()
        .blend(&twin, &req(et, BlendKind::Fillet { radius: 0.2 }))
        .unwrap();
    let want = hole_fillet(TAU * 2.0, 0.2);
    assert!(
        (bt.volume_change + want).abs() <= 1e-3 * want,
        "Zwilling {} / {}",
        bt.volume_change,
        -want
    );
}

/// Kubische B-Spline-Wand mit inneren Knoten von (0, 0) nach (10, 0), waagerechte Enden (die
/// Nachbarwände x = 0 und x = 10 stehen senkrecht auf der Randkante), Wellen der Höhe `amp`.
fn wavy_wall(amp: f64) -> NurbsCurve {
    let ys = [0.0, 0.0, amp, -amp, amp, -0.5 * amp, amp, 0.0, 0.0];
    let n = ys.len();
    let control: Vec<DVec3> = ys
        .iter()
        .enumerate()
        .map(|(i, y)| DVec3::new(10.0 * i as f64 / (n - 1) as f64, *y, 0.0))
        .collect();
    // Geklemmt, ungleichmäßige innere Knoten.
    let mut knots = vec![0.0; 4];
    knots.extend([0.15, 0.3, 0.42, 0.6, 0.85]);
    knots.extend([1.0; 4]);
    NurbsCurve::new(3, knots, control, vec![1.0; n]).unwrap()
}

fn wavy_prism(amp: f64) -> Solid {
    let profile = Profile {
        outer: ProfileLoop {
            start: DVec2::ZERO,
            segments: vec![
                Segment::Nurbs {
                    curve: Arc::new(wavy_wall(amp)),
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
    k().extrude(&profile, &Frame::WORLD, 3.0).unwrap()
}

/// Kleinster Krümmungsradius der Kurve (dichte Abtastung).
fn min_curvature_radius(c: &NurbsCurve) -> f64 {
    let (t0, t1) = c.domain();
    (0..=4000)
        .map(|i| {
            let d = c.derivs(t0 + (t1 - t0) * i as f64 / 4000.0, 2);
            d[1].length().powi(3) / d[1].cross(d[2]).length().max(1e-300)
        })
        .fold(f64::INFINITY, f64::min)
}

#[test]
fn rims_of_wavy_spline_walls_with_interior_knots_round() {
    // Band entlang der Kurve: abgetragene Fläche je Höhe w L − (w²/2) ∫κ ds; mit waagerechten
    // Enden ist ∫κ ds = 0, also ΔV = L r² (1 − π/4) bzw. L d²/2 (L Länge der Kurve, Simpson).
    for amp in [0.4, 0.8] {
        let s = wavy_prism(amp);
        let e = edge_between(&s, "Wand 1", "Deckel");
        let c = wavy_wall(amp);
        let (t0, t1) = c.domain();
        let len = simpson(t0, t1, 40_000, |t| c.tangent(t).length());
        let rho = min_curvature_radius(&c);
        for f in [0.2, 0.6] {
            let r = f * rho;
            check(
                &s,
                e,
                BlendKind::Fillet { radius: r },
                len * r * r * A,
                &format!("Welle {amp} r {r}"),
            );
        }
        let d = 0.5 * rho;
        check(
            &s,
            e,
            BlendKind::ChamferEqual { distance: d },
            len * d * d / 2.0,
            &format!("Welle {amp} Fase {d}"),
        );
    }
}

#[test]
fn a_lens_edge_between_two_freeform_walls_is_named_clearly() {
    // Review M1: Linsen aus zwei NURBS-Kreisbögen (Mittelpunkte (±3, 0) bzw. (±4,5, 0), Radius 5), senkrechte
    // Spitzenkante zwischen zwei Freiformwänden. Ergebnis gültig oder klar benannt (kein
    // technischer Text über Öffnungswinkel).
    for off in [3.0, 4.5, 4.95] {
        lens_case(off);
    }
}

/// Linse aus Bögen um (±`off`, 0) mit Radius 5.
fn lens_case(off: f64) {
    let arc = |c: f64, from: f64, sweep: f64| Segment::Nurbs {
        curve: Arc::new(
            NurbsCurve::elliptic_arc(
                DVec3::new(c, 0.0, 0.0),
                DVec3::new(5.0 * from.cos(), 5.0 * from.sin(), 0.0),
                DVec3::new(-5.0 * from.sin(), 5.0 * from.cos(), 0.0),
                0.0,
                sweep,
            )
            .unwrap(),
        ),
    };
    let half = (off / 5.0).acos();
    let tip = DVec2::new(5.0 * half.cos() - off, -5.0 * half.sin());
    let profile = Profile {
        outer: ProfileLoop {
            start: tip,
            segments: vec![
                arc(-off, -half, 2.0 * half),
                arc(off, PI - half, 2.0 * half),
            ],
        },
        holes: vec![],
    };
    let lens = k().extrude(&profile, &Frame::WORLD, 2.0).unwrap();
    let e = edge_between(&lens, "Wand 1", "Wand 2");
    match k().blend(&lens, &req(e, BlendKind::Fillet { radius: 1.0 })) {
        Ok(b) => {
            validate(&b.solid).unwrap();
        }
        Err(err) => {
            let m = err.to_string();
            assert!(
                !m.contains("Öffnungswinkel") && !m.contains("Ungültige Eingabe"),
                "{m}"
            );
        }
    }
}
