//! Körper mit mehreren Schalen: Hohlräume als `BREP_WITH_VOIDS`, getrennte Stücke als eigene
//! `MANIFOLD_SOLID_BREP` im selben Teil (Teilprojekt 10, Task 11).

mod common;

use common::check::measure;
use common::{checked, export};
use planora_kernel::boolean::BoolOp;
use planora_kernel::{AnalyticKernel, Kernel, Solid};
use planora_math::{DMat4, DVec3, PI};

/// Sehnentoleranz der Netze.
const CHORD: f64 = 0.002;
/// Wie in `stage2.rs`: Volumenfehler höchstens `SAG_FACTOR · CHORD · Oberfläche`.
const SAG_FACTOR: f64 = 2.0;

fn k() -> AnalyticKernel {
    AnalyticKernel::default()
}

fn moved(s: &Solid, v: DVec3) -> Solid {
    k().transform(s, &DMat4::from_translation(v)).unwrap()
}

fn cube(size: f64) -> Solid {
    k().make_box(DVec3::splat(size)).unwrap()
}

/// Würfel 10 mit würfelförmigem Hohlraum 2 in der Mitte.
fn hollow_cube() -> Solid {
    let r = k()
        .boolean(
            &cube(10.0),
            &moved(&cube(2.0), DVec3::splat(4.0)),
            BoolOp::Difference,
        )
        .unwrap();
    assert_eq!(r.shells.len(), 2);
    r
}

/// Würfel 10 mit kugelförmigem Hohlraum r = 2 in der Mitte.
fn hollow_ball() -> Solid {
    let ball = moved(&k().make_sphere(2.0).unwrap(), DVec3::splat(5.0));
    let r = k().boolean(&cube(10.0), &ball, BoolOp::Difference).unwrap();
    assert_eq!(r.shells.len(), 2);
    r
}

/// Zwei getrennte Würfel (Vereinigung ohne Berührung).
fn two_cubes() -> Solid {
    let r = k()
        .boolean(
            &cube(4.0),
            &moved(&cube(3.0), DVec3::X * 10.0),
            BoolOp::Union,
        )
        .unwrap();
    assert_eq!(r.shells.len(), 2);
    r
}

/// Würfel 12 mit Hohlraum 8, darin ein freier Würfel 2: drei Schalen.
fn box_in_box() -> Solid {
    let shell = k()
        .boolean(
            &cube(12.0),
            &moved(&cube(8.0), DVec3::splat(2.0)),
            BoolOp::Difference,
        )
        .unwrap();
    let r = k()
        .boolean(&shell, &moved(&cube(2.0), DVec3::splat(5.0)), BoolOp::Union)
        .unwrap();
    assert_eq!(r.shells.len(), 3);
    r
}

#[test]
fn a_cavity_becomes_a_brep_with_voids_with_a_reversed_shell() {
    let s = hollow_cube();
    let (text, _) = export(&[("Hohl", &s)]);
    assert_eq!(text.matches("BREP_WITH_VOIDS(").count(), 1);
    assert_eq!(text.matches("ORIENTED_CLOSED_SHELL('',*,#").count(), 1);
    assert!(!text.contains("MANIFOLD_SOLID_BREP("));
    let (m, stats) = checked(&text, &[&s]);
    assert_eq!(stats.len(), 2);
    let solid = &m.products[0].solids[0];
    assert_eq!(solid.name, "Hohl");
    assert_eq!(solid.voids.len(), 1);
    assert!(!solid.voids[0].orientation);
    let r = measure(&m, &[&s], CHORD).unwrap();
    assert_eq!(r.flipped, 0);
    assert!((r.volume - 992.0).abs() < 1e-9, "{}", r.volume);
}

#[test]
fn a_spherical_cavity_keeps_the_volume() {
    let s = hollow_ball();
    for (pose, m) in [
        ("wie gebaut", DMat4::IDENTITY),
        ("gespiegelt", DMat4::from_scale(DVec3::new(1.0, -1.0, 1.0))),
    ] {
        let s = k().transform(&s, &m).unwrap();
        let (text, _) = export(&[("Kugelhohl", &s)]);
        let (model, _) = checked(&text, &[&s]);
        let r = measure(&model, &[&s], CHORD).unwrap();
        assert_eq!(r.flipped, 0, "{pose}");
        let want = 1000.0 - 4.0 / 3.0 * PI * 8.0;
        assert!(
            (r.volume - want).abs() <= SAG_FACTOR * CHORD * 4.0 * PI * 4.0,
            "{pose}: {} statt {want}",
            r.volume
        );
    }
}

#[test]
fn separate_pieces_are_several_solids_of_one_part() {
    let s = two_cubes();
    let (text, report) = export(&[("Zwei", &s)]);
    assert_eq!(report.bodies, 1);
    assert_eq!(text.matches("MANIFOLD_SOLID_BREP(").count(), 2);
    let (m, _) = checked(&text, &[&s]);
    assert_eq!(m.products.len(), 1);
    assert_eq!(m.products[0].solids.len(), 2);
    let r = measure(&m, &[&s], CHORD).unwrap();
    assert!((r.volume - 91.0).abs() < 1e-9, "{}", r.volume);
}

/// Würfel 12 mit Hohlraum 8, darin ein Würfel 4 mit eigenem Hohlraum 1: vier Schalen, der kleine
/// Hohlraum liegt in beiden äußeren Schalen und gehört zur kleineren.
#[test]
fn a_cavity_belongs_to_the_smallest_outer_shell_around_it() {
    let big = k()
        .boolean(
            &cube(12.0),
            &moved(&cube(8.0), DVec3::splat(2.0)),
            BoolOp::Difference,
        )
        .unwrap();
    let small = k()
        .boolean(
            &moved(&cube(4.0), DVec3::splat(4.0)),
            &moved(&cube(1.0), DVec3::splat(5.5)),
            BoolOp::Difference,
        )
        .unwrap();
    let s = k().boolean(&big, &small, BoolOp::Union).unwrap();
    assert_eq!(s.shells.len(), 4);
    let (text, _) = export(&[("Verschachtelt", &s)]);
    let (m, _) = checked(&text, &[&s]);
    let solids = &m.products[0].solids;
    assert_eq!(solids.len(), 2);
    assert!(solids.iter().all(|x| x.voids.len() == 1));
    let r = measure(&m, &[&s], CHORD).unwrap();
    assert_eq!(r.flipped, 0);
    let want = 1728.0 - 512.0 + 64.0 - 1.0;
    assert!((r.volume - want).abs() < 1e-9, "{}", r.volume);
}

#[test]
fn a_piece_inside_a_cavity_is_its_own_solid_and_the_cavity_belongs_to_the_big_box() {
    let s = box_in_box();
    let (text, _) = export(&[("Schachtel", &s)]);
    let (m, stats) = checked(&text, &[&s]);
    assert_eq!(stats.len(), 3);
    let solids = &m.products[0].solids;
    assert_eq!(solids.len(), 2);
    let with_void = solids.iter().filter(|x| x.voids.len() == 1).count();
    assert_eq!(with_void, 1);
    let r = measure(&m, &[&s], CHORD).unwrap();
    assert_eq!(r.flipped, 0);
    assert!(
        (r.volume - (1728.0 - 512.0 + 8.0)).abs() < 1e-9,
        "{}",
        r.volume
    );
}
