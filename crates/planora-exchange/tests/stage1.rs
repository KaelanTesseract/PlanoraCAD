//! Stufentest 1: Körper aus dem Kernel als STL und OBJ schreiben, lesen und mit Formeln
//! vergleichen; Zufallsreihe gegen die Masseneigenschaften (Teilprojekt 9, Task 5).

use planora_exchange::*;
use planora_kernel::blend::topo::{edge_uses, face_name};
use planora_kernel::{AnalyticKernel, BlendEdge, BlendKind, BlendRequest, Kernel, Solid};
use planora_math::{DMat4, DVec3, PI};
use planora_mesh::{TessellationOptions, mass_properties, tessellate};

/// Rundung durch `f32` in der Datei, relativ zum Volumen (Koordinaten bis 30 mm).
const F32_REL: f64 = 1e-5;
/// Wie [`F32_REL`] für die Zufallsreihe mit Lagen bis 500 mm und Körpern ab 0,5 mm.
const RANDOM_REL: f64 = 1e-3;
/// Die Mitte eines Dreiecks liegt bis zu zwei Sehnenhöhen (in Breite und Länge) unter der gekrümmten Fläche.
const SAG_FACTOR: f64 = 2.0;

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

fn export_body(name: &str, solid: &Solid, chord: f64) -> (ExportBody, f64) {
    let mesh = tessellate(
        solid,
        &TessellationOptions {
            chord_tolerance: chord,
        },
    )
    .unwrap();
    let body = collect_body(&SolidSource { name, mesh: &mesh }).unwrap();
    (body, mass_properties(&mesh).volume)
}

/// Schreibt STL und OBJ, liest beide zurück.
fn round_trip(body: &ExportBody) -> [ExchangeMesh; 2] {
    let mut stl = Vec::new();
    write_stl(&mut stl, std::slice::from_ref(body)).unwrap();
    let mut obj = Vec::new();
    write_obj(&mut obj, std::slice::from_ref(body)).unwrap();
    let p = Progress::new();
    [
        read_stl(&stl[..], stl.len() as u64, &p).unwrap(),
        read_obj(&obj[..], obj.len() as u64, &p).unwrap(),
    ]
}

/// Würfel 10 mm mit einer gerundeten Kante (Deckel/Wand 1).
fn filleted_box(r: f64) -> Solid {
    let k = AnalyticKernel::default();
    let s = k.make_box(DVec3::splat(10.0)).unwrap();
    let e = s
        .edges
        .iter()
        .map(|(id, _)| id)
        .find(|&id| {
            let n: Vec<String> = edge_uses(&s, id)
                .iter()
                .map(|u| face_name(&s, u.face))
                .collect();
            n.contains(&"Deckel".to_string()) && n.contains(&"Wand 1".to_string())
        })
        .unwrap();
    let request = BlendRequest {
        edges: vec![BlendEdge {
            edge: e,
            first_face: None,
        }],
        kind: BlendKind::Fillet { radius: r },
        tangent_chain: false,
    };
    k.blend(&s, &request).unwrap().solid
}

#[test]
fn closed_formulas_survive_the_files() {
    let k = AnalyticKernel::default();
    let r = 1.5;
    let cases: [(&str, Solid, f64); 4] = [
        (
            "Quader",
            k.make_box(DVec3::new(10.0, 20.0, 5.0)).unwrap(),
            1000.0,
        ),
        (
            "Zylinder",
            k.make_cylinder(4.0, 10.0).unwrap(),
            PI * 16.0 * 10.0,
        ),
        ("Kugel", k.make_sphere(5.0).unwrap(), 4.0 / 3.0 * PI * 125.0),
        (
            "Gerundeter Quader",
            filleted_box(r),
            1000.0 - 10.0 * r * r * (1.0 - PI / 4.0),
        ),
    ];
    // Grob, Normal, Fein (Spec E5)
    for chord in [0.1, 0.01, 0.001] {
        for (name, solid, volume) in &cases {
            let (body, v_mesh) = export_body(name, solid, chord);
            let area = body.mesh.mass(&MeshTransform::IDENTITY).surface_area;
            let closed = check_mesh(&body.mesh).closed;
            assert!(closed, "{name} {chord}: Netz nicht geschlossen");
            for back in round_trip(&body) {
                assert_eq!(back.triangle_count(), body.mesh.triangle_count(), "{name}");
                let m = back.mass(&MeshTransform::IDENTITY);
                assert!(
                    (m.volume - v_mesh).abs() <= F32_REL * volume,
                    "{name} {chord}: Datei {} / Netz {v_mesh}",
                    m.volume
                );
                assert!(
                    (m.surface_area - area).abs() <= F32_REL * area,
                    "{name} {chord}: Fläche {} / {area}",
                    m.surface_area
                );
                // Das Netz liegt innerhalb der Sehnentoleranz an der Fläche.
                assert!(
                    (m.volume - volume).abs() <= SAG_FACTOR * chord * area + F32_REL * volume,
                    "{name} {chord}: Datei {} / Formel {volume}",
                    m.volume
                );
                assert_eq!(check_mesh(&back).closed, closed, "{name} {chord}");
            }
        }
    }
}

fn random_series(n: usize, seed: u64) {
    let k = AnalyticKernel::default();
    let mut rng = Lcg(seed);
    for i in 0..n {
        let solid = match i % 4 {
            0 => k
                .make_box(DVec3::new(
                    rng.range(0.5, 30.0),
                    rng.range(0.5, 30.0),
                    rng.range(0.5, 30.0),
                ))
                .unwrap(),
            1 => k
                .make_cylinder(rng.range(0.5, 15.0), rng.range(0.5, 30.0))
                .unwrap(),
            2 => k.make_sphere(rng.range(0.5, 15.0)).unwrap(),
            _ => k
                .make_cone(rng.range(0.5, 15.0), rng.range(0.5, 30.0))
                .unwrap(),
        };
        let offset = DVec3::new(
            rng.range(-500.0, 500.0),
            rng.range(-500.0, 500.0),
            rng.range(-500.0, 500.0),
        );
        let solid = k
            .transform(&solid, &DMat4::from_translation(offset))
            .unwrap();
        let (body, v_mesh) = export_body("Zufall", &solid, 0.05);
        for back in round_trip(&body) {
            let v = back.mass(&MeshTransform::IDENTITY).volume;
            assert!(
                (v - v_mesh).abs() <= RANDOM_REL * v_mesh.abs(),
                "Fall {i}: {v} / {v_mesh}"
            );
        }
    }
}

#[test]
fn a_fixed_random_series_keeps_the_volume() {
    random_series(24, 0x5EED_0009);
}

#[test]
#[ignore = "lange Zufallsreihe; nur mit --release --ignored (etwa 0,5 s)"]
fn a_long_random_series_keeps_the_volume() {
    random_series(2000, 0x5EED_0019);
}
