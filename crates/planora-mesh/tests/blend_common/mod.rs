//! Gemeinsame Hilfen der Tests zu Rundungen und Fasen (`blend_stage1.rs`, `blend_stage2.rs`,
//! `blend_stage3.rs`).

#![allow(dead_code)]

use planora_kernel::blend::topo::{edge_uses, face_name};
use planora_kernel::*;
use planora_math::{DVec2, DVec3, Frame, PI, Tolerance};
use planora_mesh::{
    Discretizer, blend_volume_plausible, check_face_boundaries, curved_area, is_watertight,
    mass_properties, tessellate_with,
};

pub fn k() -> AnalyticKernel {
    AnalyticKernel::default()
}

pub fn tol() -> Tolerance {
    Tolerance::default()
}

/// Kante zwischen den Flächen mit den Rollen `a` und `b` (Reihenfolge egal).
pub fn edge_between(s: &Solid, a: &str, b: &str) -> Id<Edge> {
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
        .unwrap_or_else(|| panic!("keine Kante zwischen {a} und {b}"))
}

pub fn blend_edges(
    s: &Solid,
    edges: &[Id<Edge>],
    kind: BlendKind,
    chain: bool,
) -> Result<Blended, BlendError> {
    k().blend(
        s,
        &BlendRequest {
            edges: edges
                .iter()
                .map(|&edge| BlendEdge {
                    edge,
                    first_face: None,
                })
                .collect(),
            kind,
            tangent_chain: chain,
        },
    )
}

/// Netzvolumen nachher minus vorher mit gemeinsamer Teilung. Prüft das Ergebnis: `validate`,
/// Rand jeder Fläche aus ihren Kantenpunkten, geschlossenes Netz.
pub fn delta_volume(before: &Solid, after: &Solid, chord: f64) -> f64 {
    validate(after).unwrap_or_else(|e| panic!("validate: {e}"));
    let disc = Discretizer::for_solids(&[before, after], chord);
    let mb = tessellate_with(before, &disc).unwrap();
    let ma = tessellate_with(after, &disc).unwrap_or_else(|e| panic!("Netz: {e}"));
    check_face_boundaries(after, &ma).unwrap_or_else(|e| panic!("{e}"));
    assert!(is_watertight(&ma, 1e-6), "Netz nicht geschlossen");
    mass_properties(&ma).volume - mass_properties(&mb).volume
}

/// Volumenprobe wie im Feature (P2) mit Sehnentoleranz `chord`.
pub fn plausible(before: &Solid, b: &Blended, chord: f64) -> bool {
    let disc = Discretizer::for_solids(&[before, &b.solid], chord);
    let mb = tessellate_with(before, &disc).unwrap();
    let ma = tessellate_with(&b.solid, &disc).unwrap();
    let slack =
        2.0 * disc.chord_tolerance() * (curved_area(before, &mb) + curved_area(&b.solid, &ma));
    blend_volume_plausible(
        mass_properties(&mb).volume,
        mass_properties(&ma).volume,
        b.volume_change,
        0.5,
        slack,
    )
}

pub fn assert_rel(got: f64, want: f64, rel: f64, what: &str) {
    assert!(
        (got - want).abs() <= rel * want.abs(),
        "{what}: {got} statt {want} (relativ {})",
        ((got - want) / want).abs()
    );
}

pub fn extrude_polygon(points: &[DVec2], h: f64) -> Solid {
    let profile = Profile {
        outer: ProfileLoop::polygon(points).unwrap(),
        holes: vec![],
    };
    k().extrude(&profile, &Frame::WORLD, h).unwrap()
}

/// Halbkugel über der xy-Ebene: „Fläche 1“ Boden (Kreisscheibe), „Fläche 2“ Kugelfläche.
pub fn hemisphere(r: f64) -> Solid {
    k().revolve(
        &ProfileLoop {
            start: DVec2::ZERO,
            segments: vec![
                Segment::Line {
                    to: DVec2::new(r, 0.0),
                },
                Segment::Arc {
                    center: DVec2::ZERO,
                    sweep: PI / 2.0,
                },
            ],
        },
        &Frame::WORLD,
    )
    .unwrap()
}

/// Querschnitt zwischen einer Kante mit Innenwinkel `theta` (zwei Ebenen) und der Rundung mit
/// Radius `r`: (Fläche, Abstand des Schwerpunkts von der Kante längs der Winkelhalbierenden).
/// Drachen (Kante, zwei Berührpunkte, Mittelpunkt) minus Kreissektor mit Öffnung `π − theta`.
pub fn spandrel(theta: f64, r: f64) -> (f64, f64) {
    let half = theta / 2.0;
    let t = r / half.tan();
    let d = r / half.sin();
    let kite = r * t;
    let kite_c = (t * half.cos() + d) / 3.0;
    let alpha = PI - theta;
    let sector = 0.5 * r * r * alpha;
    let sector_c = d - 4.0 * r * (alpha / 2.0).sin() / (3.0 * alpha);
    let area = kite - sector;
    (area, (kite * kite_c - sector * sector_c) / area)
}

/// Fläche und Schwerpunkt (in der Ebene (ρ, h)) des Gebiets zwischen einer Ebene `h = 0`
/// (Material darüber), einem Kreis um den Ursprung mit Radius `big` (Material innen) und der
/// Rundung mit Radius `r`, die beide innen berührt; fein abgetastet (Schnürsenkelformel).
pub fn plane_circle_spandrel(big: f64, r: f64) -> (f64, DVec2) {
    const N: usize = 4000;
    let c = DVec2::new(((big - r) * (big - r) - r * r).sqrt(), r);
    let pp = DVec2::new(c.x, 0.0);
    let ps_angle = c.y.atan2(c.x);
    let mut pts = vec![pp];
    for i in 0..=N {
        let a = ps_angle * i as f64 / N as f64;
        pts.push(DVec2::new(a.cos(), a.sin()) * big);
    }
    // Rundungsbogen von der Kugel (Richtung c) zurück zur Ebene (Richtung −y) über die Ecke.
    let (b0, b1) = (ps_angle, -PI / 2.0);
    for i in 1..N {
        let a = b0 + (b1 - b0) * i as f64 / N as f64;
        pts.push(c + DVec2::new(a.cos(), a.sin()) * r);
    }
    let n = pts.len();
    let (mut area, mut cx, mut cy) = (0.0, 0.0, 0.0);
    for i in 0..n {
        let (p, q) = (pts[i], pts[(i + 1) % n]);
        let cr = p.x * q.y - q.x * p.y;
        area += cr;
        cx += (p.x + q.x) * cr;
        cy += (p.y + q.y) * cr;
    }
    area *= 0.5;
    (area.abs(), DVec2::new(cx / (6.0 * area), cy / (6.0 * area)))
}

/// Kleiner Zufallsgenerator mit festem Startwert (keine Abhängigkeit).
pub struct Lcg(pub u64);

impl Lcg {
    pub fn next(&mut self) -> f64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (self.0 >> 11) as f64 / (1u64 << 53) as f64
    }

    pub fn range(&mut self, a: f64, b: f64) -> f64 {
        a + (b - a) * self.next()
    }
}

/// Punkt im Abstand `d` von `p` in Richtung `dir` (für Lageprüfungen an robusten Punkten).
pub fn offset(p: DVec3, dir: DVec3, d: f64) -> DVec3 {
    p + dir.normalize() * d
}
