//! Stufentest 3 (Teilprojekt 10, Task 12): Körper aus Rundungen, Fasen und Booleschen
//! Operationen (auch mit Freiform-Schnittkurven), gespiegelt und gedreht; Zufallsreihe mit
//! festem Startwert (kurz im Debug-Build, lang nur mit `--release --ignored`).

mod common;

use common::check::measure;
use common::{Lcg, checked, export};
use planora_kernel::boolean::BoolOp;
use planora_kernel::{
    AnalyticKernel, BlendEdge, BlendKind, BlendRequest, Edge, Id, Kernel, Solid, Surface, UvRect,
};
use planora_math::nurbs::{NurbsCurve, NurbsSurface};
use planora_math::{DMat4, DVec3, EulerRot, TAU};
use planora_mesh::{TessellationOptions, mass_properties, tessellate};
use std::fs;
use std::path::PathBuf;

/// Sehnentoleranz der Netze.
const CHORD: f64 = 0.01;
/// Wie in `stage2.rs`: Volumenfehler höchstens `SAG_FACTOR · CHORD · Oberfläche`.
const SAG_FACTOR: f64 = 2.0;
/// Fälle der kurzen Zufallsreihe (Debug-Build, unter 30 s).
const SHORT_SERIES: usize = 12;
/// Fälle der langen Zufallsreihe (nur Release).
const LONG_SERIES: usize = 300;
/// Sehnentoleranz (mm) für die Volumenangaben der Beispieldateien.
const SAMPLE_CHORD: f64 = 0.001;

fn k() -> AnalyticKernel {
    AnalyticKernel::default()
}

fn moved(s: &Solid, m: DMat4) -> Solid {
    k().transform(s, &m).unwrap()
}

/// Kante, deren beide Endpunkte `test` erfüllen (die erste, nach Id).
fn edge_where(s: &Solid, test: impl Fn(DVec3) -> bool) -> Id<Edge> {
    s.edges
        .iter()
        .find(|(_, e)| test(s.vertices[e.start].point) && test(s.vertices[e.end].point))
        .map(|(id, _)| id)
        .expect("Kante gefunden")
}

fn blend(s: &Solid, edges: &[Id<Edge>], kind: BlendKind) -> Solid {
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
            tangent_chain: false,
        },
    )
    .unwrap_or_else(|e| panic!("Rundung: {e}"))
    .solid
}

fn cube() -> Solid {
    k().make_box(DVec3::splat(10.0)).unwrap()
}

/// Obere vordere Kante (y = 0, z = 10) und die beiden anderen Kanten an der Ecke (10, 0, 10).
fn corner_edges(s: &Solid) -> Vec<Id<Edge>> {
    let near = |a: f64, b: f64| (a - b).abs() < 1e-9;
    vec![
        edge_where(s, |p| near(p.y, 0.0) && near(p.z, 10.0)),
        edge_where(s, |p| near(p.x, 10.0) && near(p.z, 10.0)),
        edge_where(s, |p| near(p.x, 10.0) && near(p.y, 0.0)),
    ]
}

/// Quader 20 × 20 × 10 um den Ursprung (z ∈ [−5, 5]).
fn centered_box() -> Solid {
    moved(
        &k().make_box(DVec3::new(20.0, 20.0, 10.0)).unwrap(),
        DMat4::from_translation(DVec3::new(-10.0, -10.0, -5.0)),
    )
}

/// Zylinder r = 3, h = 20 ab z = −10, dessen Mantel eine NURBS-Drehfläche ist (wie in
/// `planora-mesh/tests/nurbs_common`), um `angle` um z gedreht und um `dx` in x verschoben.
fn nurbs_cylinder(angle: f64, dx: f64) -> Solid {
    let (r, h) = (3.0, 20.0);
    let mut solid = k().make_cylinder(r, h).unwrap();
    let foot = DVec3::new(r, 0.0, 0.0);
    let line = NurbsCurve::new(
        1,
        vec![0.0, 0.0, 1.0, 1.0],
        vec![foot, foot + DVec3::Z * h],
        vec![1.0; 2],
    )
    .unwrap();
    let surface = Surface::nurbs(
        NurbsSurface::revolve(&line, DVec3::ZERO, DVec3::Z, 0.0, TAU).unwrap(),
        true,
        false,
    )
    .unwrap();
    let id = solid
        .faces
        .iter()
        .find(|(_, f)| matches!(f.surface, Surface::Cylinder { .. }))
        .map(|(id, _)| id)
        .unwrap();
    solid.faces[id].surface = surface;
    solid.faces[id].patch = Some(UvRect {
        u0: 0.0,
        u1: TAU,
        v0: 0.0,
        v1: 1.0,
    });
    moved(
        &solid,
        DMat4::from_translation(DVec3::new(dx, 0.0, -10.0)) * DMat4::from_rotation_z(angle),
    )
}

/// (Name, Körper) aus Teilprojekt 8, 7 und 3.
fn bodies() -> Vec<(&'static str, Solid)> {
    let c = cube();
    let edges = corner_edges(&c);
    let crossed = k()
        .boolean(
            &centered_box(),
            &nurbs_cylinder(0.3, 2.0),
            BoolOp::Difference,
        )
        .unwrap();
    let dome = k()
        .boolean(
            &c,
            &moved(
                &k().make_sphere(4.0).unwrap(),
                DMat4::from_translation(DVec3::new(5.0, 5.0, 10.0)),
            ),
            BoolOp::Union,
        )
        .unwrap();
    vec![
        (
            "Rundung",
            blend(&c, &edges[..1], BlendKind::Fillet { radius: 2.0 }),
        ),
        ("Ecke", blend(&c, &edges, BlendKind::Fillet { radius: 2.0 })),
        (
            "Fase",
            blend(&c, &edges[..1], BlendKind::ChamferEqual { distance: 1.5 }),
        ),
        (
            "Variabel",
            blend(
                &c,
                &edges[..1],
                BlendKind::VariableFillet {
                    points: vec![(0.0, 1.0), (1.0, 2.5)],
                },
            ),
        ),
        ("Kreuz", crossed),
        ("Kuppel", dome),
    ]
}

/// Prüft einen Körper vollständig und vergleicht das Volumen mit dem Netz des Kernels.
fn full(name: &str, s: &Solid) {
    let (text, _) = export(&[(name, s)]);
    let (m, _) = checked(&text, &[s]);
    let r = measure(&m, &[s], CHORD).unwrap_or_else(|e| panic!("{name}: {e}"));
    assert_eq!(r.flipped, 0, "{name}");
    assert!(
        (r.volume - r.kernel_volume).abs() <= SAG_FACTOR * CHORD * r.area,
        "{name}: {} gegen {}",
        r.volume,
        r.kernel_volume
    );
}

#[test]
fn rounded_chamfered_and_combined_bodies_pass_all_checks() {
    for (name, s) in bodies() {
        for (pose, m) in [
            ("wie gebaut", DMat4::IDENTITY),
            (
                "gespiegelt und gedreht",
                DMat4::from_rotation_z(0.6) * DMat4::from_scale(DVec3::new(1.0, 1.0, -1.0)),
            ),
        ] {
            let s = moved(&s, m);
            let label = format!("{name}, {pose}");
            full(&label, &s);
        }
    }
}

#[test]
fn the_blends_produce_the_expected_kinds_of_faces() {
    let kinds = |s: &Solid| {
        let (text, _) = export(&[("x", s)]);
        (
            text.contains("CYLINDRICAL_SURFACE("),
            text.contains("SPHERICAL_SURFACE("),
            text.contains("B_SPLINE_SURFACE"),
        )
    };
    let all = bodies();
    let get = |n: &str| &all.iter().find(|(name, _)| *name == n).unwrap().1;
    assert!(kinds(get("Rundung")).0);
    assert!(kinds(get("Ecke")).1, "Kugelecke");
    assert!(kinds(get("Variabel")).2, "variabler Radius ist Freiform");
    let (text, _) = export(&[("x", get("Kreuz"))]);
    assert!(
        text.contains("B_SPLINE_CURVE"),
        "Schnittkurve mit der Freiformfläche"
    );
}

/// Zufällige Lage: verschoben um bis zu ±2 und beliebig gedreht.
fn random_pose(rng: &mut Lcg) -> DMat4 {
    DMat4::from_translation(DVec3::new(
        rng.range(-2.0, 2.0),
        rng.range(-2.0, 2.0),
        rng.range(-2.0, 2.0),
    )) * DMat4::from_euler(
        EulerRot::XYZ,
        rng.range(0.0, 3.0),
        rng.range(0.0, 3.0),
        rng.range(0.0, 3.0),
    )
}

/// Zufälliger Körper: ein gedrehter Quader, verrechnet mit einem gedrehten Quader, einem Zylinder
/// oder einer Kugel (Paare, die der Kernel kann; Zylinder mit Zylinder oder Kugel schräg zur
/// Achse rechnet er noch nicht). `None`, wenn der Kernel die Operation benannt ablehnt.
fn random_body(rng: &mut Lcg) -> Option<Solid> {
    let size = |rng: &mut Lcg| {
        DVec3::new(
            rng.range(2.0, 8.0),
            rng.range(2.0, 8.0),
            rng.range(2.0, 8.0),
        )
    };
    let a = moved(&k().make_box(size(rng)).unwrap(), random_pose(rng));
    let b = match (rng.uniform() * 3.0) as usize {
        0 => k().make_box(size(rng)).unwrap(),
        1 => k()
            .make_cylinder(rng.range(1.0, 4.0), rng.range(2.0, 8.0))
            .unwrap(),
        _ => k().make_sphere(rng.range(1.0, 4.0)).unwrap(),
    };
    let b = moved(&b, random_pose(rng));
    let op =
        [BoolOp::Union, BoolOp::Difference, BoolOp::Intersection][(rng.uniform() * 3.0) as usize];
    k().boolean(&a, &b, op).ok()
}

fn series(cases: usize) -> usize {
    let mut rng = Lcg(20_261_014);
    let mut done = 0;
    for i in 0..cases {
        if let Some(s) = random_body(&mut rng) {
            full(&format!("Zufall {i}"), &s);
            done += 1;
        }
    }
    done
}

#[test]
fn a_short_random_series_passes_all_checks() {
    assert!(
        series(SHORT_SERIES) >= SHORT_SERIES / 2,
        "zu viele Ablehnungen"
    );
}

#[test]
#[ignore = "lange Reihe, nur mit --release --ignored"]
fn a_long_random_series_passes_all_checks() {
    assert!(
        series(LONG_SERIES) >= LONG_SERIES / 2,
        "zu viele Ablehnungen"
    );
}

/// Schreibt Beispieldateien für den Handtest in Fusion 360 (Task 15) in den Ordner aus der
/// Umgebungsvariable `PLANORA_STEP_SAMPLES` (sonst `planora-step-proben` im Ordner für
/// vorübergehende Dateien) und nennt Flächenzahl und Volumen (Netz, Sehne 0,001 mm) je Körper.
#[test]
#[ignore = "nur für den Handtest (Task 15): schreibt Beispieldateien"]
fn write_samples_for_the_hand_test() {
    let dir = std::env::var_os("PLANORA_STEP_SAMPLES")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("planora-step-proben"));
    fs::create_dir_all(&dir).unwrap();
    let hollow = k()
        .boolean(
            &cube(),
            &moved(
                &k().make_box(DVec3::splat(4.0)).unwrap(),
                DMat4::from_translation(DVec3::splat(3.0)),
            ),
            BoolOp::Difference,
        )
        .unwrap();
    let mut all = bodies();
    all.push(("Zylinder", k().make_cylinder(5.0, 12.0).unwrap()));
    all.push(("Kegel", k().make_cone(6.0, 9.0).unwrap()));
    all.push(("Kugel", k().make_sphere(7.0).unwrap()));
    all.push(("Hohl", hollow));
    for (name, s) in &all {
        let (text, report) = export(&[(name, s)]);
        fs::write(dir.join(format!("{name}.step")), &text).unwrap();
        let mesh = tessellate(
            s,
            &TessellationOptions {
                chord_tolerance: SAMPLE_CHORD,
            },
        )
        .unwrap();
        println!(
            "{name}.step: {} Flächen, Volumen {:.3} mm³",
            report.faces,
            mass_properties(&mesh).volume
        );
    }
    let list: Vec<(&str, &Solid)> = all.iter().map(|(n, s)| (*n, s)).collect();
    let (text, report) = export(&list);
    fs::write(dir.join("Alle.step"), &text).unwrap();
    println!(
        "Alle.step: {} Teile, Ordner {}",
        report.bodies,
        dir.display()
    );
}
