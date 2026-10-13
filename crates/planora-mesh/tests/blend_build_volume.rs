//! Umbau der Nachbarflächen (Teilprojekt 8, Task 4): Das vernähte Ergebnis hat das richtige
//! Volumen (geschlossene Formeln) und ein dichtes Netz. Liegt hier, weil der Kernel nicht
//! vernetzen kann.

use planora_kernel::blend::analytic::*;
use planora_kernel::blend::build::*;
use planora_kernel::blend::section::ChamferSpec;
use planora_kernel::blend::topo::*;
use planora_kernel::*;
use planora_math::{DVec2, DVec3, Frame, PI, TAU, Tolerance};
use planora_mesh::{Discretizer, is_watertight, mass_properties, tessellate_with};

/// Sehnentoleranz der Volumenprobe.
const CHORD: f64 = 1e-4;
/// Erlaubte Abweichung der Volumenänderung (mm³): Sehnenfehler der gekrümmten Flächen.
const VOLUME_SLACK: f64 = 0.02;

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
        .unwrap_or_else(|| panic!("keine Kante zwischen {a} und {b}"))
}

/// Volumenänderung (Ergebnis − Eingang) bei gemeinsamer Teilung.
fn delta_volume(s: &Solid, a: &str, b: &str, kind: SectionKind) -> f64 {
    delta_volume_at(s, edge_between(s, a, b), kind)
}

fn delta_volume_at(s: &Solid, e: Id<Edge>, kind: SectionKind) -> f64 {
    let tol = Tolerance::default();
    let sides = edge_sides(s, e, None).unwrap();
    let angle = edge_angle(s, e, &sides).unwrap();
    let g = analytic_geom(s, e, &sides, angle.convex, kind, &tol)
        .unwrap()
        .unwrap();
    let geoms = vec![g];
    let plan = end_cuts(s, &geoms, &tol).unwrap();
    let r = assemble(s, &geoms, &plan, &["Rundung 1".to_string()], &tol).unwrap();
    let d = Discretizer::for_solids(&[s, &r], CHORD);
    let (m0, m1) = (
        tessellate_with(s, &d).unwrap(),
        tessellate_with(&r, &d).unwrap(),
    );
    assert!(is_watertight(&m1, 1e-9));
    mass_properties(&m1).volume - mass_properties(&m0).volume
}

fn assert_close(got: f64, want: f64) {
    assert!((got - want).abs() <= VOLUME_SLACK, "{got} statt {want}");
}

#[test]
fn rebuilt_bodies_have_the_expected_volume() {
    let cube = k().make_box(DVec3::splat(10.0)).unwrap();
    // Rundung r = 2 an einer Würfelkante: (4 − π) · 10 weg.
    let dv = delta_volume(
        &cube,
        "Deckel",
        "Wand 1",
        SectionKind::Fillet { radius: 2.0 },
    );
    assert_close(dv, -(4.0 - PI) * 10.0);
    // Fase 1 × 2: Dreieck 1 · 10 weg.
    let kind = SectionKind::Chamfer(ChamferSpec::Distances { a: 1.0, b: 2.0 });
    assert_close(delta_volume(&cube, "Deckel", "Wand 1", kind), -10.0);

    // Zylinder r = 5: Deckelkante mit r = 1 gerundet (Pappus), mit 1 × 1 gefast.
    let cyl = k().make_cylinder(5.0, 8.0).unwrap();
    let area = 1.0 - PI / 4.0;
    let centroid = 5.0 - (10.0 - 3.0 * PI) / (3.0 * (4.0 - PI));
    let dv = delta_volume(
        &cyl,
        "Wand 1",
        "Deckel",
        SectionKind::Fillet { radius: 1.0 },
    );
    assert_close(dv, -area * TAU * centroid);
    let kind = SectionKind::Chamfer(ChamferSpec::Distances { a: 1.0, b: 1.0 });
    let dv = delta_volume(&cyl, "Wand 1", "Deckel", kind);
    assert_close(dv, -0.5 * TAU * (5.0 - 1.0 / 3.0));

    // Konkave Kante des L-Profils: (1 − π/4) · 5 kommt hinzu.
    let profile = Profile {
        outer: ProfileLoop::polygon(&[
            DVec2::new(0.0, 0.0),
            DVec2::new(10.0, 0.0),
            DVec2::new(10.0, 4.0),
            DVec2::new(4.0, 4.0),
            DVec2::new(4.0, 10.0),
            DVec2::new(0.0, 10.0),
        ])
        .unwrap(),
        holes: vec![],
    };
    let ell = k().extrude(&profile, &Frame::WORLD, 5.0).unwrap();
    let dv = delta_volume(
        &ell,
        "Wand 3",
        "Wand 4",
        SectionKind::Fillet { radius: 1.0 },
    );
    assert_close(dv, area * 5.0);
}

/// Kante mit den Endpunkten `p0` und `p1` (in beliebiger Richtung).
fn edge_at(s: &Solid, p0: DVec3, p1: DVec3) -> Id<Edge> {
    let near = |v: Id<Vertex>, p: DVec3| s.vertices[v].point.distance(p) < 1e-9;
    s.edges
        .iter()
        .find(|(_, e)| {
            (near(e.start, p0) && near(e.end, p1)) || (near(e.start, p1) && near(e.end, p0))
        })
        .map(|(id, _)| id)
        .unwrap_or_else(|| panic!("keine Kante {p0} – {p1}"))
}

fn holed_box(holes: &[DVec2]) -> Solid {
    let profile = Profile {
        outer: ProfileLoop::rectangle(DVec2::ZERO, DVec2::splat(10.0)),
        holes: holes.iter().map(|c| ProfileLoop::circle(*c, 0.5)).collect(),
    };
    k().extrude(&profile, &Frame::WORLD, 10.0).unwrap()
}

#[test]
fn bodies_with_holes_outside_the_rounded_area_keep_them() {
    // Regression (Review Task 4): Löcher 1,5 mm vor den Wänden; r = 0,5 bleibt vor dem Loch.
    let s = holed_box(&[
        DVec2::new(1.5, 5.0),
        DVec2::new(8.5, 5.0),
        DVec2::new(5.0, 1.5),
        DVec2::new(5.0, 8.5),
    ]);
    let e = edge_at(&s, DVec3::new(0.0, 0.0, 10.0), DVec3::new(0.0, 10.0, 10.0));
    let fillet = SectionKind::Fillet { radius: 0.5 };
    assert_close(
        delta_volume_at(&s, e, fillet),
        -(1.0 - PI / 4.0) * 0.25 * 10.0,
    );
    // Loch nahe der Ecke einer Abschlussfläche, innerhalb der Abschlusskurve (r = 3).
    let s = holed_box(&[DVec2::new(1.5, 1.5)]);
    let e = edge_at(&s, DVec3::ZERO, DVec3::new(0.0, 0.0, 10.0));
    let fillet = SectionKind::Fillet { radius: 3.0 };
    assert_close(
        delta_volume_at(&s, e, fillet),
        -(1.0 - PI / 4.0) * 9.0 * 10.0,
    );
}
