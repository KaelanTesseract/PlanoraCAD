//! Schräge Ecken: gleichseitiges Prisma mit allen neun Kanten gerundet gegen Steiner
//! (Teilprojekt 8, Task 17).

mod blend_common;

use blend_common::*;
use planora_kernel::*;
use planora_math::{DMat3, DMat4, DVec2, DVec3, PI};

/// Abstand der Kernel-Schätzung `volume_change` von Steiner bei voll gerundeten Polyedern
/// (Querschnittsfläche im Kernel als Vieleck genähert; gemessen ≤ 3e-4).
const ESTIMATE_REL: f64 = 1e-3;

#[test]
fn a_fully_rounded_triangular_prism_follows_steiner() {
    let (side, h, r) = (10.0, 6.0, 0.8);
    let height = side * 3f64.sqrt() / 2.0;
    let s = extrude_polygon(
        &[
            DVec2::new(0.0, 0.0),
            DVec2::new(side, 0.0),
            DVec2::new(side / 2.0, height),
        ],
        h,
    );
    let edges: Vec<Id<Edge>> = s.edges.iter().map(|(id, _)| id).collect();
    let b = blend_edges(&s, &edges, BlendKind::Fillet { radius: r }, false).unwrap();
    // Inneres Prisma: Dreieck um r nach innen versetzt (Seite − 2√3·r), Höhe − 2r.
    let a1 = side - 2.0 * 3f64.sqrt() * r;
    let h1 = h - 2.0 * r;
    let area = 3f64.sqrt() / 4.0 * a1 * a1;
    let v = area * h1;
    let surface = 2.0 * area + 3.0 * a1 * h1;
    // Kanten: 3 senkrechte (Innenwinkel 60°, Außenwinkel 2π/3), 6 waagerechte (90°, π/2).
    let edge_term = 0.5 * (3.0 * h1 * (2.0 * PI / 3.0) + 6.0 * a1 * (PI / 2.0));
    let rounded = v + r * surface + r * r * edge_term + 4.0 / 3.0 * PI * r * r * r;
    let original = 3f64.sqrt() / 4.0 * side * side * h;
    assert_rel(
        delta_volume(&s, &b.solid, 1e-3),
        rounded - original,
        5e-3,
        "Steiner-Prisma",
    );
    assert!(plausible(&s, &b, 0.05));
    assert_rel(
        b.volume_change,
        rounded - original,
        ESTIMATE_REL,
        "Schätzung Steiner-Prisma",
    );
}

/// Konvexes Polyeder als Schnitt der Halbräume `n · x ≤ d` (`n` Einheitsvektor).
type Planes = Vec<(DVec3, f64)>;

/// Volumen, Oberfläche und Σ Kantenlänge · Außenwinkel des Polyeders `planes`, unabhängig vom
/// Kernel aus den Ebenengleichungen (Ecken als Schnitt dreier Ebenen).
fn polytope(planes: &Planes) -> (f64, f64, f64) {
    const ON: f64 = 1e-9;
    let m = planes.len();
    let mut verts: Vec<DVec3> = Vec::new();
    for i in 0..m {
        for j in i + 1..m {
            for k in j + 1..m {
                let (a, b, c) = (planes[i].0, planes[j].0, planes[k].0);
                let det = a.dot(b.cross(c));
                if det.abs() < 1e-9 {
                    continue;
                }
                let p = (b.cross(c) * planes[i].1
                    + c.cross(a) * planes[j].1
                    + a.cross(b) * planes[k].1)
                    / det;
                if planes.iter().all(|(n, d)| n.dot(p) <= d + ON)
                    && !verts.iter().any(|q| q.distance(p) < 1e-7)
                {
                    verts.push(p);
                }
            }
        }
    }
    let on = |i: usize| -> Vec<DVec3> {
        verts
            .iter()
            .copied()
            .filter(|p| (planes[i].0.dot(*p) - planes[i].1).abs() < 1e-7)
            .collect()
    };
    let o = verts.iter().copied().sum::<DVec3>() / verts.len() as f64;
    let (mut volume, mut area, mut edges) = (0.0, 0.0, 0.0);
    for i in 0..m {
        let pts = on(i);
        if pts.len() < 3 {
            continue;
        }
        let n = planes[i].0;
        let c = pts.iter().copied().sum::<DVec3>() / pts.len() as f64;
        let u = (pts[0] - c).normalize();
        let w = n.cross(u);
        let mut sorted = pts.clone();
        sorted.sort_by(|p, q| {
            let ap = (*p - c).dot(w).atan2((*p - c).dot(u));
            let aq = (*q - c).dot(w).atan2((*q - c).dot(u));
            ap.total_cmp(&aq)
        });
        let a: f64 = (0..sorted.len())
            .map(|k| {
                (sorted[k] - c)
                    .cross(sorted[(k + 1) % sorted.len()] - c)
                    .dot(n)
                    / 2.0
            })
            .sum();
        area += a;
        volume += (planes[i].1 - n.dot(o)) * a / 3.0;
        for (nj, dj) in &planes[i + 1..] {
            let shared: Vec<DVec3> = pts
                .iter()
                .copied()
                .filter(|p| (nj.dot(*p) - dj).abs() < 1e-7)
                .collect();
            if shared.len() == 2 {
                edges += shared[0].distance(shared[1]) * n.dot(*nj).clamp(-1.0, 1.0).acos();
            }
        }
    }
    (volume, area, edges)
}

/// Steiner: Volumen des voll gerundeten Polyeders = innerer Körper (Ebenen um r versetzt)
/// ⊕ Kugel vom Radius r. Liefert (Volumen gerundet, Volumen vorher).
fn steiner(planes: &Planes, r: f64) -> (f64, f64) {
    let inner: Planes = planes.iter().map(|(n, d)| (*n, d - r)).collect();
    let (v, s, e) = polytope(&inner);
    let rounded = v + r * s + 0.5 * r * r * e + 4.0 / 3.0 * PI * r * r * r;
    (rounded, polytope(planes).0)
}

/// Halbräume eines senkrechten Prismas über dem gegen den Uhrzeigersinn umlaufenden Dreieck.
fn prism_planes(tri: &[DVec2; 3], h: f64) -> Planes {
    let mut planes: Planes = vec![(DVec3::NEG_Z, 0.0), (DVec3::Z, h)];
    for k in 0..3 {
        let (a, b) = (tri[k], tri[(k + 1) % 3]);
        let n = DVec3::new(b.y - a.y, a.x - b.x, 0.0).normalize();
        planes.push((n, n.dot(a.extend(0.0))));
    }
    planes
}

fn round_all_and_check(s: &Solid, planes: &Planes, r: f64, what: &str) {
    let edges: Vec<Id<Edge>> = s.edges.iter().map(|(id, _)| id).collect();
    let b = blend_edges(s, &edges, BlendKind::Fillet { radius: r }, false)
        .unwrap_or_else(|e| panic!("{what}: {e}"));
    let corners = b
        .solid
        .faces
        .iter()
        .filter(|(id, _)| planora_kernel::blend::topo::face_name(&b.solid, *id).starts_with("Ecke"))
        .count();
    assert_eq!(corners, s.vertices.len(), "{what}: ein Kugelstück je Ecke");
    let (rounded, original) = steiner(planes, r);
    // Gegenprobe des Orakels am Eingang: Netzvolumen des ungerundeten Körpers.
    let disc = planora_mesh::Discretizer::for_solids(&[s], 1e-3);
    let m = planora_mesh::tessellate_with(s, &disc).unwrap();
    assert_rel(
        planora_mesh::mass_properties(&m).volume,
        original,
        1e-9,
        what,
    );
    // Der Netzfehler fällt linear mit der Sehne (gemessen am ungleichseitigen Prisma: 5,4e-3
    // bei Sehne 1e-3, 5,6e-4 bei 1e-4); das Ergebnis läuft auf das Orakel zu.
    assert_rel(
        delta_volume(s, &b.solid, 1e-4),
        rounded - original,
        1.5e-3,
        what,
    );
    assert!(plausible(s, &b, 0.05), "{what}: Volumenprobe");
    // Voll gerundet laufen alle Kanten zwischen Kugelecken: Kantenprismen plus Eckzellen sind
    // dann Steiner bis auf die Vieleck-Näherung der Querschnittsfläche im Kernel
    // (`polygon_area_centroid`, auch am Würfel ~3e-4 relativ).
    assert_rel(b.volume_change, rounded - original, ESTIMATE_REL, what);
}

#[test]
fn a_scalene_prism_with_a_slanted_top_follows_steiner() {
    let r = 0.5;
    let tri = [
        DVec2::new(0.0, 0.0),
        DVec2::new(10.0, 0.0),
        DVec2::new(3.0, 7.0),
    ];
    let h = 8.0;
    let prism = extrude_polygon(&tri, h);
    // Senkrechtes ungleichseitiges Prisma: Ecken mit zwei rechten und einem schiefen Winkel.
    round_all_and_check(&prism, &prism_planes(&tri, h), r, "ungleichseitiges Prisma");
    // Schräger Deckel: an den oberen Ecken sind alle drei Flächenpaare schief zueinander.
    let n = DVec3::new(0.15, 0.25, 1.0).normalize();
    let t = DVec3::new(4.0, 3.0, 6.0);
    let x = DVec3::Y.cross(n).normalize();
    let rot = DMat3::from_cols(x, n.cross(x), n);
    let tool = k()
        .transform(
            &k().make_box(DVec3::splat(40.0)).unwrap(),
            &(DMat4::from_translation(t)
                * DMat4::from_mat3(rot)
                * DMat4::from_translation(DVec3::new(-20.0, -20.0, 0.0))),
        )
        .unwrap();
    let cut = k().boolean(&prism, &tool, BoolOp::Difference).unwrap();
    validate(&cut).unwrap();
    let mut planes = prism_planes(&tri, h);
    planes[1] = (n, n.dot(t));
    round_all_and_check(&cut, &planes, r, "schräger Deckel");
}

/// Zufälliges ungleichseitiges Prisma mit schrägem Deckel; alle Kanten mit Radius r gerundet.
/// Jedes Ok muss Steiner folgen (Netzfehler-Schranke aus Sehne und gekrümmter Fläche des
/// Orakels), jede Ablehnung ist benannt. Liefert (Ok, abgelehnt).
fn seeded_slanted_prisms(seed: u64, cases: usize) -> (usize, usize) {
    const CHORD: f64 = 1e-3;
    let mut rng = Lcg(seed);
    let (mut ok, mut refused) = (0, 0);
    let mut tried = 0;
    while tried < cases {
        let tri = [
            DVec2::new(rng.range(-1.0, 1.0), rng.range(-1.0, 1.0)),
            DVec2::new(rng.range(8.0, 12.0), rng.range(-1.0, 1.0)),
            DVec2::new(rng.range(1.0, 9.0), rng.range(5.0, 10.0)),
        ];
        let h = rng.range(4.0, 10.0);
        let n = DVec3::new(rng.range(-0.5, 0.5), rng.range(-0.5, 0.5), 1.0).normalize();
        let c = (tri[0] + tri[1] + tri[2]) / 3.0;
        let t = DVec3::new(c.x, c.y, h * rng.range(0.5, 0.8));
        let r = rng.range(0.1, 1.5);
        // Deckel muss über allen Bodenecken mindestens 1 mm Abstand und unter h bleiben.
        let top = |p: DVec2| t.z - (n.x * (p.x - t.x) + n.y * (p.y - t.y)) / n.z;
        if tri.iter().any(|p| !(1.0..h - 0.2).contains(&top(*p))) {
            continue;
        }
        tried += 1;
        let prism = extrude_polygon(&tri, h);
        let x = DVec3::Y.cross(n).normalize();
        let rot = DMat3::from_cols(x, n.cross(x), n);
        let tool = k()
            .transform(
                &k().make_box(DVec3::splat(60.0)).unwrap(),
                &(DMat4::from_translation(t)
                    * DMat4::from_mat3(rot)
                    * DMat4::from_translation(DVec3::new(-30.0, -30.0, 0.0))),
            )
            .unwrap();
        let Ok(s) = k().boolean(&prism, &tool, BoolOp::Difference) else {
            continue;
        };
        let edges: Vec<Id<Edge>> = s.edges.iter().map(|(id, _)| id).collect();
        let b = match blend_edges(&s, &edges, BlendKind::Fillet { radius: r }, false) {
            Ok(b) => b,
            Err(e) => {
                assert!(!e.to_string().is_empty());
                refused += 1;
                continue;
            }
        };
        let mut planes = prism_planes(&tri, h);
        planes[1] = (n, n.dot(t));
        let (rounded, original) = steiner(&planes, r);
        let inner: Planes = planes.iter().map(|(n, d)| (*n, d - r)).collect();
        let (_, _, e) = polytope(&inner);
        let curved = r * e + 4.0 * PI * r * r;
        let got = delta_volume(&s, &b.solid, CHORD);
        let want = rounded - original;
        assert!(
            (got - want).abs() <= 2.0 * CHORD * curved,
            "Fall {tried} (seed {seed}): ΔV {got} statt {want}, r {r}, Dreieck {tri:?}, h {h}, n {n}"
        );
        assert!(plausible(&s, &b, 0.05), "Fall {tried}: Volumenprobe");
        assert_rel(
            b.volume_change,
            want,
            ESTIMATE_REL,
            "Schätzung gegen Steiner",
        );
        ok += 1;
    }
    (ok, refused)
}

#[test]
fn seeded_slanted_prisms_follow_steiner() {
    let (ok, refused) = seeded_slanted_prisms(17, 12);
    eprintln!("schräge Prismen: {ok} Ok, {refused} abgelehnt");
    assert!(ok >= 9, "zu wenige Ok: {ok}");
}

/// Schwere Serie: `cargo test --release -p planora-mesh --test blend_corners_general -- --ignored`.
#[test]
#[ignore]
fn seeded_slanted_prisms_follow_steiner_long() {
    let (ok, refused) = seeded_slanted_prisms(1717, 300);
    eprintln!("schräge Prismen: {ok} Ok, {refused} abgelehnt");
    assert!(ok >= 250, "zu wenige Ok: {ok}");
}

/// Fünfeck-Prisma (Höhe 5) mit stumpfer Spitze (Innenwinkel `deg`) oben; nur die drei Kanten
/// an der oberen Spitze gerundet (Kugelecke an schiefer Ecke, Korrekturrunde 1 von Task 17).
/// Vorher schätzte die Spat-Formel das Eckvolumen um ein Vielfaches zu groß (P2 lehnte korrekte
/// Körper ab, etwa 157°/r = 3: gemessen −19,9 statt geschätzt −73,6).
fn pentagon_tip(deg: f64, r: f64) {
    let hgt = 10.0 * ((180.0 - deg) / 2.0).to_radians().tan();
    let pts = [
        DVec2::new(0.0, 0.0),
        DVec2::new(20.0, 0.0),
        DVec2::new(20.0, 10.0),
        DVec2::new(10.0, 10.0 + hgt),
        DVec2::new(0.0, 10.0),
    ];
    let s = extrude_polygon(&pts, 5.0);
    let tip = DVec3::new(10.0, 10.0 + hgt, 5.0);
    let (top, _) = s
        .vertices
        .iter()
        .find(|(_, v)| v.point.distance(tip) < 1e-9)
        .unwrap();
    let edges: Vec<Id<Edge>> = s
        .edges
        .iter()
        .filter(|(_, e)| e.start == top || e.end == top)
        .map(|(id, _)| id)
        .collect();
    assert_eq!(edges.len(), 3);
    let b = blend_edges(&s, &edges, BlendKind::Fillet { radius: r }, false).unwrap();
    let got = delta_volume(&s, &b.solid, 2e-4);
    assert!(plausible(&s, &b, 0.05), "{deg}°, r {r}: Volumenprobe");
    // Rest: Netzfehler und die Schätzung an den schrägen fernen Kantenenden (Task 9, M1).
    assert_rel(b.volume_change, got, 0.05, "Schätzung Fünfeckspitze");
}

#[test]
fn obtuse_pentagon_tips_pass_the_volume_check() {
    pentagon_tip(157.0, 3.0);
    pentagon_tip(165.0, 2.0);
    pentagon_tip(140.0, 3.0);
}
