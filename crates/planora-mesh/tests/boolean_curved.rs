use planora_kernel::boolean::*;
use planora_kernel::*;
use planora_math::*;
use planora_mesh::*;

fn k() -> AnalyticKernel {
    AnalyticKernel::default()
}

fn moved(s: &Solid, v: DVec3) -> Solid {
    k().transform(s, &DMat4::from_translation(v)).unwrap()
}

fn rel(a: f64, b: f64) -> f64 {
    ((a - b) / b).abs()
}

fn vol(s: &Solid) -> f64 {
    validate(s).unwrap();
    let m = tessellate(
        s,
        &TessellationOptions {
            chord_tolerance: 0.001,
        },
    )
    .unwrap();
    assert!(is_watertight(&m, 1e-6), "Netz nicht geschlossen");
    mass_properties(&m).volume
}

/// Die Zuordnung Polygonpunkt → Eingabepunkt bleibt auch dann richtig, wenn die Schleife an einem
/// Pol beginnt und deshalb intern gedreht wird.
#[test]
fn uv_indices_survive_rotation_at_a_pole() {
    let surface = Surface::Sphere {
        frame: Frame::WORLD,
        radius: 2.0,
    };
    // Großkreis durch beide Pole (Ebene y = 0), Beginn am Nordpol
    let n = 24;
    let pts: Vec<DVec3> = (0..n)
        .map(|i| {
            let t = PI / 2.0 + TAU * i as f64 / n as f64;
            DVec3::new(2.0 * t.cos(), 0.0, 2.0 * t.sin())
        })
        .collect();
    let out = points_to_uv_indexed(&surface, true, &pts);
    let mut seen = vec![0; n];
    for (uv, src) in &out {
        match src {
            Some(i) => {
                seen[*i] += 1;
                let (u, v) = surface.project(pts[*i]);
                assert!((v - uv.y).abs() < 1e-12, "v von Punkt {i}");
                let du = (uv.x - u) / TAU;
                assert!((du - du.round()).abs() < 1e-12, "u von Punkt {i}");
                assert!(surface.point(uv.x, uv.y).distance(pts[*i]) < 1e-9);
            }
            None => assert!((uv.y.abs() - PI / 2.0).abs() < 1e-12, "Polpunkt"),
        }
    }
    // Polpunkte (Index 0 = Nordpol, n/2 = Südpol) werden ersetzt, alle anderen genau einmal
    for (i, c) in seen.iter().enumerate() {
        let expect = if i == 0 || i == n / 2 { 0 } else { 1 };
        assert_eq!(*c, expect, "Punkt {i}");
    }
    assert_eq!(out.iter().filter(|(_, s)| s.is_none()).count(), 4);
}

#[test]
fn cylinder_with_a_side_notch() {
    let cyl = k().make_cylinder(3.0, 10.0).unwrap();
    let boxx = moved(
        &k().make_box(DVec3::new(10.0, 20.0, 6.0)).unwrap(),
        DVec3::new(1.0, -10.0, 2.0),
    );
    let d = k().boolean(&cyl, &boxx, BoolOp::Difference).unwrap();
    let segment = 9.0 * (1.0_f64 / 3.0).acos() - 8.0_f64.sqrt();
    let expect = PI * 9.0 * 10.0 - segment * 6.0;
    let v = vol(&d);
    assert!(rel(v, expect) < 0.005, "{v} vs {expect}");
}

/// Wie `half_sphere_by_a_box` in `trimmed.rs`, aber ohne Berührung der Kugel mit den Quaderkanten:
/// Die Schnittebene geht durch beide Pole.
#[test]
fn half_sphere_by_a_larger_box() {
    let sph = moved(&k().make_sphere(5.0).unwrap(), DVec3::new(5.0, 5.0, 5.0));
    let boxx = moved(
        &k().make_box(DVec3::new(15.0, 20.0, 20.0)).unwrap(),
        DVec3::new(5.0, -5.0, -5.0),
    );
    let i = k().boolean(&sph, &boxx, BoolOp::Intersection).unwrap();
    let d = k().boolean(&sph, &boxx, BoolOp::Difference).unwrap();
    let half = 2.0 / 3.0 * PI * 125.0;
    for v in [vol(&i), vol(&d)] {
        assert!(rel(v, half) < 0.01, "{v} vs {half}");
    }
}

#[test]
fn sphere_minus_an_octant() {
    let sph = k().make_sphere(4.0).unwrap();
    let boxx = moved(&k().make_box(DVec3::splat(10.0)).unwrap(), DVec3::ZERO);
    let d = k().boolean(&sph, &boxx, BoolOp::Difference).unwrap();
    let expect = 7.0 / 8.0 * 4.0 / 3.0 * PI * 64.0;
    let v = vol(&d);
    assert!(rel(v, expect) < 0.01, "{v} vs {expect}");
}
