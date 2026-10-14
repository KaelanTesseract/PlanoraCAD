//! Nachbesserung Task 15, Runde 2: Naht geschlossener angepasster Freiformkanten mit variablem
//! Radius. Boolesche Schnittkanten (Quader minus elliptischer Zylinder) schließen mit einem
//! Tangentenknick bis einige 1e-3 rad; jedes `Ok` muss dem exakten Modell folgen (Querschnitte
//! senkrecht zur wahren Kante, Kugelmittelpunkt im Abstand `r(s)` von beiden Flächen, `r` an der
//! Bogenlänge der Stelle), geprüft auf einem dichten Raster unabhängig vom Kernel; sonst eine
//! benannte Ablehnung.

use planora_kernel::blend::topo::face_name;
use planora_kernel::*;
use planora_math::nurbs::NurbsCurve;
use planora_math::{DMat4, DVec2, DVec3, Frame, RadiusLaw, TAU};

/// Erlaubte Abweichung von der exakten Rundung (`NURBS_FIT_TOL / 2`, wie die Nachprüfung der
/// Bahn).
const SKIN_TOL: f64 = 5e-6;
/// Höhe des Quaders.
const H: f64 = 5.0;

fn k() -> AnalyticKernel {
    AnalyticKernel::default()
}

fn ellipse_prism(a: f64, b: f64) -> Solid {
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
        7.0,
    )
    .unwrap()
}

/// Quader 10 × 10 × 5 minus elliptischer Durchbruch (Mitte (5, 5)) und seine obere Randkante.
fn hole(a: f64, b: f64) -> (Solid, Id<Edge>) {
    let block = k().make_box(DVec3::new(10.0, 10.0, H)).unwrap();
    let tool = k()
        .transform(
            &ellipse_prism(a, b),
            &DMat4::from_translation(DVec3::new(5.0, 5.0, -1.0)),
        )
        .unwrap();
    let s = k().boolean(&block, &tool, BoolOp::Difference).unwrap();
    let e = s
        .edges
        .iter()
        .map(|(id, _)| id)
        .find(|&id| {
            let e = &s.edges[id];
            matches!(e.curve, Curve::Nurbs(_)) && (e.curve.point(e.t0).z - H).abs() < 1e-6
        })
        .unwrap();
    (s, e)
}

/// Bogenlänge der Ellipse von `t0` bis `t1` (Simpson mit `n` Abschnitten, gerade).
fn arc(a: f64, b: f64, t0: f64, t1: f64, n: usize) -> f64 {
    let f = |x: f64| (a * a * x.sin().powi(2) + b * b * x.cos().powi(2)).sqrt();
    let h = (t1 - t0) / n as f64;
    let mut sum = f(t0) + f(t1);
    for i in 1..n {
        sum += f(t0 + h * i as f64) * if i % 2 == 1 { 4.0 } else { 2.0 };
    }
    sum * h / 3.0
}

/// Bogenlängentabelle (Stützstellen je `TAU / TABLE`), dazwischen Simpson mit 8 Abschnitten.
const TABLE: usize = 4096;
fn arc_table(a: f64, b: f64) -> Vec<f64> {
    let h = TAU / TABLE as f64;
    let mut acc = vec![0.0];
    for i in 0..TABLE {
        let last = acc[i];
        acc.push(last + arc(a, b, h * i as f64, h * (i + 1) as f64, 16));
    }
    acc
}
fn arc_length(table: &[f64], a: f64, b: f64, t: f64) -> f64 {
    let h = TAU / TABLE as f64;
    let i = ((t / h) as usize).min(TABLE - 1);
    table[i] + arc(a, b, h * i as f64, t, 8)
}

/// Größte Abweichung der Rundungsfläche vom exakten Modell: Jeder Rasterpunkt p liegt in der
/// senkrechten Ebene durch seinen Fußpunkt e(t) auf der Ellipse (nächster Punkt, dichte Suche und
/// Goldener Schnitt); dort ist der Kugelmittelpunkt e(t) + r(f) · n_außen in der Höhe H − r(f),
/// f = Bogenlänge ab der Naht (t = 0) durch den Umfang. Das Gesetz ist symmetrisch, die
/// Laufrichtung der Kante spielt keine Rolle.
fn deviation(s: &Solid, a: f64, b: f64, law: &RadiusLaw) -> f64 {
    let (_, face) = s
        .faces
        .iter()
        .find(|(id, f)| matches!(f.surface, Surface::Nurbs(_)) && face_name(s, *id) == "Rundung 1")
        .unwrap();
    let patch = face.patch.unwrap();
    let table = arc_table(a, b);
    let per = table[TABLE];
    let e = |t: f64| DVec2::new(5.0 + a * t.cos(), 5.0 + b * t.sin());
    let mut worst: f64 = 0.0;
    for i in 0..=300 {
        for j in 0..=8 {
            let u = patch.u0 + (patch.u1 - patch.u0) * i as f64 / 300.0;
            let v = patch.v0 + (patch.v1 - patch.v0) * j as f64 / 8.0;
            let p = face.surface.point(u, v);
            let q = p.truncate();
            let d = |t: f64| (e(t) - q).length();
            const N: usize = 720;
            let step = TAU / N as f64;
            let best = (0..N)
                .map(|k| k as f64 * step)
                .min_by(|x, y| d(*x).total_cmp(&d(*y)))
                .unwrap();
            let (mut lo, mut hi) = (best - step, best + step);
            let g = 0.5 * (5f64.sqrt() - 1.0);
            for _ in 0..100 {
                let (x1, x2) = (hi - g * (hi - lo), lo + g * (hi - lo));
                if d(x1) < d(x2) {
                    hi = x2;
                } else {
                    lo = x1;
                }
            }
            let t = (0.5 * (lo + hi)).rem_euclid(TAU);
            let r = law.radius(arc_length(&table, a, b, t) / per);
            let n = DVec2::new(b * t.cos(), a * t.sin()).normalize();
            let c = (e(t) + n * r).extend(H - r);
            worst = worst.max((p.distance(c) - r).abs());
        }
    }
    worst
}

/// Ergebnis einer Anfrage: `Ok(Abweichung vom exakten Modell)` oder die Meldung der Ablehnung.
fn run(a: f64, b: f64, points: &[(f64, f64)]) -> Result<f64, String> {
    run_kind(
        a,
        b,
        points,
        BlendKind::VariableFillet {
            points: points.to_vec(),
        },
    )
}

/// Wie [`run`] mit beliebiger Art (`points` beschreibt den Radius für das Orakel).
fn run_kind(a: f64, b: f64, points: &[(f64, f64)], kind: BlendKind) -> Result<f64, String> {
    let (s, e) = hole(a, b);
    let law = RadiusLaw::new(points).unwrap();
    let request = BlendRequest {
        edges: vec![BlendEdge {
            edge: e,
            first_face: None,
        }],
        kind,
        tangent_chain: false,
    };
    match k().blend(&s, &request) {
        Ok(blended) => {
            validate(&blended.solid).unwrap();
            Ok(deviation(&blended.solid, a, b, &law))
        }
        Err(err) => Err(err.to_string()),
    }
}

/// `Ok` innerhalb `SKIN_TOL` oder benannt abgelehnt.
fn exact_or_named(a: f64, b: f64, points: &[(f64, f64)]) -> Result<f64, String> {
    let r = run(a, b, points);
    eprintln!("Loch {a}×{b} {points:?}: {r:?}");
    if let Ok(dev) = r {
        assert!(dev <= SKIN_TOL, "Loch {a}×{b} {points:?}: Abweichung {dev}");
    }
    r
}

#[test]
fn the_review_construction_is_exact_or_named() {
    // Review-Konstruktion (Runde 2): Nahtknick ~3e-3 rad, steiles Gesetz an der Naht; mit der
    // geneigten Naht (d46ac7b) war das Ergebnis Ok, aber 3e-4 mm neben dem exakten Modell.
    exact_or_named(
        1.4726,
        0.2267,
        &[(0.0, 0.2), (0.03, 0.8), (0.97, 0.8), (1.0, 0.2)],
    )
    .ok();
}

#[test]
#[ignore = "weitere Gesetze und Löcher, nur mit --release --ignored (etwa 10 s)"]
fn further_steep_and_mild_laws_at_kinked_seams() {
    exact_or_named(
        1.4726,
        0.2267,
        &[(0.0, 0.1), (0.015, 0.6), (0.985, 0.6), (1.0, 0.1)],
    )
    .ok();
    exact_or_named(
        3.0,
        0.5,
        &[(0.0, 0.2), (0.03, 0.8), (0.97, 0.8), (1.0, 0.2)],
    )
    .ok();
    // Milde Gesetze bleiben Ok und exakt.
    let law = [(0.0, 0.1), (0.1, 0.5), (0.9, 0.5), (1.0, 0.1)];
    for (a, b) in [(2.0, 0.4), (3.0, 0.5), (1.4726, 0.2267)] {
        let r = exact_or_named(a, b, &law);
        assert!(r.is_ok(), "Loch {a}×{b}: {r:?}");
    }
}

#[test]
fn a_tiny_constant_radius_on_a_fitted_hole_rim_is_exact() {
    // r = 0,002 mm (Mindestmaß) am gestreckten Durchbruch 2 × 0,4: Die Kantenkurve liegt nur bis
    // ~1e-5 auf den Flächen, die Kugel berührt aber die Flächen selbst.
    let r = 0.002;
    let dev = run_kind(
        2.0,
        0.4,
        &[(0.0, r), (1.0, r)],
        BlendKind::Fillet { radius: r },
    )
    .unwrap();
    assert!(dev <= SKIN_TOL, "{dev}");
}
