//! Konvexe und konkave Kanten in einer Verrundung (Review Stufe 2, I1, Runde 2): Zufallsreihe
//! richtiger Körper (Prismen mit einspringenden Ecken, Quader mit gerundeter Tasche, Platte mit
//! gerundetem Zapfen; Gehrungen und Ketten über glatte Übergänge) misst die größte Abweichung
//! der Volumenänderung von der Schätzung relativ zur Summe der Beträge und prüft, dass die
//! Volumenprobe sie alle annimmt (Grundlage für `BLEND_REL_MIXED`).

mod blend_common;

use blend_common::*;
use planora_kernel::*;
use planora_math::{DVec2, DVec3, Frame, PI};
use planora_mesh::{
    BLEND_REL, BLEND_REL_MIXED, BLEND_SLACK_FACTOR, Discretizer, blend_volume_plausible,
    curved_area, is_watertight, mass_properties, tessellate_with,
};

/// Sehne der Reihe (wie die Feature-Vorgabe der Größenordnung nach).
const CHORD: f64 = 1e-3;

/// Ergebnis eines Falls: (gemischt?, |ΔV − Schätzung| ohne Netzspielraum relativ zur Summe der
/// Beträge, dasselbe mit Netzspielraum, Probe bestanden?).
fn measure(s: &Solid, b: &Blended) -> (bool, f64, f64, bool) {
    validate(&b.solid).unwrap_or_else(|e| panic!("validate: {e}"));
    let disc = Discretizer::for_solids(&[s, &b.solid], CHORD);
    let (m0, m1) = (
        tessellate_with(s, &disc).unwrap(),
        tessellate_with(&b.solid, &disc).unwrap(),
    );
    assert!(is_watertight(&m1, 1e-6), "Netz nicht dicht");
    let slack = BLEND_SLACK_FACTOR
        * disc.chord_tolerance()
        * (curved_area(s, &m0) + curved_area(&b.solid, &m1));
    let (v0, v1) = (mass_properties(&m0).volume, mass_properties(&m1).volume);
    let d = v1 - v0;
    let mixed = b.volume_change.abs() < b.volume_scale * (1.0 - 1e-9);
    let raw = (d - b.volume_change).abs() / b.volume_scale;
    let excess = ((d - b.volume_change).abs() - slack).max(0.0) / b.volume_scale;
    let ok = blend_volume_plausible(v0, v1, b.volume_change, b.volume_scale, BLEND_REL, slack);
    (mixed, raw, excess, ok)
}

fn request(edges: &[Id<Edge>], kind: BlendKind, chain: bool) -> BlendRequest {
    BlendRequest {
        edges: edges
            .iter()
            .map(|&edge| BlendEdge {
                edge,
                first_face: None,
            })
            .collect(),
        kind,
        tangent_chain: chain,
    }
}

fn kind(rng: &mut Lcg, size: f64) -> BlendKind {
    let pick = rng.next();
    if pick < 0.5 {
        BlendKind::Fillet { radius: size }
    } else if pick < 0.75 {
        BlendKind::ChamferEqual { distance: size }
    } else {
        BlendKind::ChamferTwo {
            distance1: size,
            distance2: size * rng.range(0.5, 1.5),
        }
    }
}

/// Endpunkte einer Kante.
fn ends(s: &Solid, e: Id<Edge>) -> (DVec3, DVec3) {
    let x = &s.edges[e];
    (s.vertices[x.start].point, s.vertices[x.end].point)
}

/// Prismen mit einspringenden Ecken (L, T, Stufe): senkrechte Kanten gemischt, dazu manchmal
/// zwei Deckelkanten an einer Ecke (Gehrung).
fn prism_case(rng: &mut Lcg) -> (Solid, Vec<Id<Edge>>, BlendKind, bool) {
    let (w, d) = (rng.range(8.0, 14.0), rng.range(8.0, 14.0));
    let (a, b) = (rng.range(3.0, w - 3.0), rng.range(3.0, d - 3.0));
    let h = rng.range(3.0, 8.0);
    let shape = (rng.next() * 3.0) as usize;
    let pts: Vec<DVec2> = match shape {
        0 => vec![(0.0, 0.0), (w, 0.0), (w, b), (a, b), (a, d), (0.0, d)],
        1 => {
            let c = rng.range(1.5, a - 1.0).min(a - 1.0);
            vec![
                (0.0, 0.0),
                (w, 0.0),
                (w, b),
                (a, b),
                (a, d),
                (c, d),
                (c, b),
                (0.0, b),
            ]
        }
        _ => vec![
            (0.0, 0.0),
            (w, 0.0),
            (w, b),
            (a, b),
            (a, d),
            (0.0, d),
            (0.0, b * 0.5),
        ],
    }
    .into_iter()
    .map(|(x, y)| DVec2::new(x, y))
    .collect();
    let s = extrude_polygon(&pts, h);
    let verticals: Vec<Id<Edge>> = s
        .edges
        .iter()
        .map(|(id, _)| id)
        .filter(|&id| {
            let (p, q) = ends(&s, id);
            (p.x - q.x).abs() < 1e-9 && (p.y - q.y).abs() < 1e-9
        })
        .collect();
    let mut chosen: Vec<Id<Edge>> = verticals
        .iter()
        .copied()
        .filter(|_| rng.next() < 0.6)
        .collect();
    if chosen.len() < 2 {
        chosen = verticals.clone();
    }
    if rng.next() < 0.4 {
        // Zwei Deckelkanten an der Ecke (w, 0, h): Gehrung, wenn die senkrechte dort frei ist.
        let top = |p: DVec3, q: DVec3| {
            s.edges
                .iter()
                .map(|(id, _)| id)
                .find(|&id| {
                    let (x, y) = ends(&s, id);
                    (x.distance(p) < 1e-9 && y.distance(q) < 1e-9)
                        || (x.distance(q) < 1e-9 && y.distance(p) < 1e-9)
                })
                .unwrap()
        };
        chosen.push(top(DVec3::new(0.0, 0.0, h), DVec3::new(w, 0.0, h)));
        chosen.push(top(DVec3::new(w, 0.0, h), DVec3::new(w, b, h)));
    }
    let limit = a.min(b).min(w - a).min(d - b).min(h) * 0.3;
    let size = rng.range(0.2, 1.0) * limit.min(1.5);
    let k = kind(rng, size);
    (s, chosen, k, rng.next() < 0.5)
}

/// Gerundetes Rechteck (Ecken r = `rho`) als Profil um `origin`.
fn rounded_rect(origin: DVec2, w: f64, d: f64, rho: f64) -> ProfileLoop {
    let p = |x: f64, y: f64| origin + DVec2::new(x, y);
    ProfileLoop {
        start: p(rho, 0.0),
        segments: vec![
            Segment::Line {
                to: p(w - rho, 0.0),
            },
            Segment::Arc {
                center: p(w - rho, rho),
                sweep: PI / 2.0,
            },
            Segment::Line { to: p(w, d - rho) },
            Segment::Arc {
                center: p(w - rho, d - rho),
                sweep: PI / 2.0,
            },
            Segment::Line { to: p(rho, d) },
            Segment::Arc {
                center: p(rho, d - rho),
                sweep: PI / 2.0,
            },
            Segment::Line { to: p(0.0, rho) },
            Segment::Arc {
                center: p(rho, rho),
                sweep: PI / 2.0,
            },
        ],
    }
}

/// Erste Kante, deren beide Enden `test` erfüllen und die eine Gerade ist.
fn line_edge(s: &Solid, test: impl Fn(DVec3) -> bool) -> Option<Id<Edge>> {
    s.edges.iter().map(|(id, _)| id).find(|&id| {
        let (p, q) = ends(s, id);
        matches!(s.edges[id].curve, Curve::Line { .. }) && test(p) && test(q)
    })
}

/// Quader mit gerundeter Tasche (konkave Bodenkette) oder mit gerundetem Zapfen (konkave
/// Fußkette, konvexe Kopfkette); dazu ein oder zwei Außenkanten des Deckels (zwei: Gehrung).
fn pocket_or_boss_case(rng: &mut Lcg) -> (Solid, Vec<Id<Edge>>, BlendKind, bool) {
    let (w, d, h) = (
        rng.range(12.0, 18.0),
        rng.range(12.0, 18.0),
        rng.range(4.0, 7.0),
    );
    let plate = k().make_box(DVec3::new(w, d, h)).unwrap();
    let (pw, pd) = (rng.range(4.0, w - 6.0), rng.range(4.0, d - 6.0));
    let rho = rng.range(1.0, 1.8);
    let origin = DVec2::new(rng.range(3.0, w - 3.0 - pw), rng.range(3.0, d - 3.0 - pd));
    let pocket = rng.next() < 0.5;
    let depth = rng.range(1.5, h - 1.5);
    let profile = Profile {
        outer: rounded_rect(origin, pw, pd, rho),
        holes: vec![],
    };
    let (z0, height, floor_z) = if pocket {
        (h - depth, depth + 1.0, h - depth)
    } else {
        (h - 1.0, 1.0 + depth, h)
    };
    let tool = k()
        .extrude(
            &profile,
            &Frame {
                origin: DVec3::new(0.0, 0.0, z0),
                ..Frame::WORLD
            },
            height,
        )
        .unwrap();
    let op = if pocket {
        BoolOp::Difference
    } else {
        BoolOp::Union
    };
    let s = k().boolean(&plate, &tool, op).unwrap();
    let inner = |p: DVec3| {
        (p.z - floor_z).abs() < 1e-9 && p.x > 1e-6 && p.x < w - 1e-6 && p.y > 1e-6 && p.y < d - 1e-6
    };
    let mut chosen = vec![line_edge(&s, inner).expect("Kante der Tasche bzw. des Zapfens")];
    if !pocket && rng.next() < 0.5 {
        let top = h + depth;
        chosen.push(line_edge(&s, |p| (p.z - top).abs() < 1e-9).expect("Kopfkante"));
    }
    let front = line_edge(&s, |p| p.y.abs() < 1e-9 && (p.z - h).abs() < 1e-9).unwrap();
    chosen.push(front);
    if rng.next() < 0.5 {
        chosen.push(line_edge(&s, |p| (p.x - w).abs() < 1e-9 && (p.z - h).abs() < 1e-9).unwrap());
    }
    let size = rng.range(0.2, 0.9) * (rho * 0.45).min(depth * 0.4).min(1.0);
    let k = kind(rng, size);
    (s, chosen, k, true)
}

/// Läuft `cases` Fälle; gibt (gemischte Ok-Fälle, größte Überschreitung) zurück.
fn mixed_series(cases: usize, seed: u64) -> (usize, f64) {
    let mut rng = Lcg(seed);
    let (mut mixed_ok, mut worst, mut worst_raw) = (0, 0.0f64, 0.0f64);
    let mut refused: std::collections::BTreeMap<String, usize> = Default::default();
    for case in 0..cases {
        let (s, edges, kind, chain) = if case % 2 == 0 {
            prism_case(&mut rng)
        } else {
            pocket_or_boss_case(&mut rng)
        };
        match k().blend(&s, &request(&edges, kind.clone(), chain)) {
            Ok(b) => {
                let (mixed, raw, excess, ok) = measure(&s, &b);
                assert!(
                    ok,
                    "Fall {case} ({kind:?}): richtiger Körper abgelehnt, {excess}"
                );
                if mixed {
                    mixed_ok += 1;
                    worst = worst.max(excess);
                    worst_raw = worst_raw.max(raw);
                }
            }
            Err(e) => {
                let m = e.to_string();
                *refused.entry(m.chars().take(60).collect()).or_default() += 1;
            }
        }
    }
    eprintln!(
        "gemischt Ok {mixed_ok}, größte Abweichung {worst_raw:e}, über dem Netzspielraum {worst:e}"
    );
    for (m, n) in &refused {
        eprintln!("abgelehnt {n}: {m}");
    }
    (mixed_ok, worst)
}

#[test]
fn mixed_blends_pass_the_volume_probe_small() {
    let (n, worst) = mixed_series(12, 0x0071_0001);
    assert!(n >= 4, "zu wenige gemischte Fälle: {n}");
    assert!(worst < BLEND_REL_MIXED, "{worst}");
}

#[test]
#[ignore = "Zufallsreihe gemischter Kanten, nur mit --release --ignored (etwa 60 s)"]
fn mixed_blends_pass_the_volume_probe_series() {
    let (n, worst) = mixed_series(600, 0x0071_0002);
    assert!(n >= 200, "zu wenige gemischte Fälle: {n}");
    assert!(worst < BLEND_REL_MIXED, "{worst}");
}

/// Gehrungen an kurzen Kanten mit großem Maß neben einer konkaven Kante: die Schätzung des
/// Kernels für Gehrungen ist hier am ungenauesten (Grundlage für `BLEND_REL_MIXED`).
fn miter_case(rng: &mut Lcg) -> (Solid, Vec<Id<Edge>>, BlendKind, bool) {
    let (w, d) = (rng.range(4.0, 9.0), rng.range(4.0, 9.0));
    let (a, b) = (rng.range(1.5, w - 1.5), rng.range(1.5, d - 1.5));
    let h = rng.range(2.0, 6.0);
    let pts: Vec<DVec2> = [(0.0, 0.0), (w, 0.0), (w, b), (a, b), (a, d), (0.0, d)]
        .into_iter()
        .map(|(x, y)| DVec2::new(x, y))
        .collect();
    let s = extrude_polygon(&pts, h);
    let find = |p: DVec3, q: DVec3| {
        s.edges
            .iter()
            .map(|(id, _)| id)
            .find(|&id| {
                let (x, y) = ends(&s, id);
                (x.distance(p) < 1e-9 && y.distance(q) < 1e-9)
                    || (x.distance(q) < 1e-9 && y.distance(p) < 1e-9)
            })
            .unwrap()
    };
    let chosen = vec![
        find(DVec3::new(0.0, 0.0, h), DVec3::new(w, 0.0, h)),
        find(DVec3::new(w, 0.0, h), DVec3::new(w, b, h)),
        find(DVec3::new(a, b, 0.0), DVec3::new(a, b, h)),
    ];
    let limit = b.min(w - a).min(h).min(a).min(d - b);
    let size = rng.range(0.3, 0.95) * 0.5 * limit;
    let k = kind(rng, size);
    (s, chosen, k, false)
}

#[test]
#[ignore = "Gehrungen mit großem Maß, nur mit --release --ignored (etwa 20 s)"]
fn miter_heavy_mixed_blends_pass_the_volume_probe() {
    let mut rng = Lcg(0x0071_0003);
    let (mut n, mut worst, mut worst_raw) = (0, 0.0f64, 0.0f64);
    for case in 0..300 {
        let (s, edges, kind, chain) = miter_case(&mut rng);
        if let Ok(b) = k().blend(&s, &request(&edges, kind.clone(), chain)) {
            let (mixed, raw, excess, ok) = measure(&s, &b);
            assert!(
                ok,
                "Fall {case} ({kind:?}): richtiger Körper abgelehnt, {excess}"
            );
            if mixed {
                n += 1;
                worst = worst.max(excess);
                worst_raw = worst_raw.max(raw);
            }
        }
    }
    eprintln!(
        "Gehrung gemischt Ok {n}, größte Abweichung {worst_raw:e}, über dem Netzspielraum {worst:e}"
    );
    assert!(n >= 100 && worst < BLEND_REL_MIXED, "{n} {worst}");
}
