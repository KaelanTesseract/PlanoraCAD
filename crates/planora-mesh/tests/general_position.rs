//! C15 (2026-11-23): Boolesche Operationen mit Flächenpaaren in allgemeiner Lage (schräge
//! Zylinder, Kegel und Torus, Zylinder neben der Kugelmitte). Vorher: „Der Schnitt von … mit …
//! wird noch nicht unterstützt“. Jedes Ergebnis ist gültig und geschlossen, und die Volumen
//! erfüllen Einschluss–Ausschluss: V(A∪B) + V(A∩B) = V(A) + V(B), V(A∖B) = V(A) − V(A∩B).

mod nurbs_common;

use nurbs_common::*;
use planora_kernel::boolean::{BoolOp, boolean};
use planora_kernel::{Kernel, ProfileLoop, Segment, Solid};
use planora_math::{DMat4, DVec2, DVec3, Frame, PI};

/// Spielraum der Volumenprobe relativ zu V(A) + V(B) (Netz mit Sehne 0,002 mm).
const REL: f64 = 3e-3;

fn moved(s: &Solid, m: DMat4) -> Solid {
    k().transform(s, &m).unwrap()
}

/// Zylinder Radius `r`, Länge `h`, mittig um den Ursprung entlang z.
fn centered_cylinder(r: f64, h: f64) -> Solid {
    moved(
        &k().make_cylinder(r, h).unwrap(),
        DMat4::from_translation(DVec3::new(0.0, 0.0, -h / 2.0)),
    )
}

/// Drehkörper um z mit Torusfläche außen: Scheibe der Dicke `2·minor` bis zum Radius `major`,
/// außen ein halber Torus (großer Radius `major`, Rohrradius `minor`). Ein Drehprofil muss die
/// Achse berühren, ein voller Torus geht so nicht.
fn torus(major: f64, minor: f64) -> Solid {
    let chain = ProfileLoop {
        start: DVec2::new(0.0, -minor),
        segments: vec![
            Segment::Line {
                to: DVec2::new(major, -minor),
            },
            Segment::Arc {
                center: DVec2::new(major, 0.0),
                sweep: PI,
            },
            Segment::Line {
                to: DVec2::new(0.0, minor),
            },
        ],
    };
    k().revolve(&chain, &Frame::WORLD).unwrap()
}

/// Alle drei Operationen: gültig, geschlossen, Volumen nach Einschluss–Ausschluss.
fn check_all(a: &Solid, b: &Solid, what: &str) {
    let (va, vb) = (checked_volume(a), checked_volume(b));
    let run = |op: BoolOp| {
        let r = boolean(a, b, op, &tol()).unwrap_or_else(|e| panic!("{what} {op:?}: {e}"));
        checked_volume(&r)
    };
    let (vu, vi, vd) = (
        run(BoolOp::Union),
        run(BoolOp::Intersection),
        run(BoolOp::Difference),
    );
    let scale = va + vb;
    assert!(vi > 1e-3 * scale, "{what}: Schnitt leer ({vi})");
    assert!(
        ((vu + vi) - scale).abs() < REL * scale,
        "{what}: {vu} + {vi} statt {scale}"
    );
    assert!(
        (vd - (va - vi)).abs() < REL * scale,
        "{what}: Differenz {vd} statt {}",
        va - vi
    );
}

#[test]
fn skew_cylinders() {
    let a = centered_cylinder(5.0, 30.0);
    let b = moved(
        &centered_cylinder(2.0, 40.0),
        DMat4::from_translation(DVec3::new(1.0, 0.0, 2.0)) * DMat4::from_rotation_x(PI / 3.0),
    );
    check_all(&a, &b, "schräge Zylinder");
}

#[test]
fn oblique_hole_through_a_tube() {
    let tube = boolean(
        &centered_cylinder(10.0, 40.0),
        &centered_cylinder(8.0, 50.0),
        BoolOp::Difference,
        &tol(),
    )
    .unwrap();
    let drill = moved(
        &centered_cylinder(3.0, 60.0),
        DMat4::from_rotation_y(PI / 3.0),
    );
    check_all(&tube, &drill, "schräge Bohrung im Rohr");
}

#[test]
fn tilted_cylinder_through_a_torus() {
    let t = torus(10.0, 3.0);
    let rod = moved(
        &centered_cylinder(1.5, 30.0),
        DMat4::from_translation(DVec3::new(11.5, 0.5, 0.0)) * DMat4::from_rotation_x(PI / 6.0),
    );
    check_all(&t, &rod, "Zylinder schräg durch Torus");
}

#[test]
fn tilted_box_through_a_torus() {
    let t = torus(10.0, 3.0);
    let slab = moved(
        &box_solid(
            DVec3::new(40.0, 40.0, 40.0),
            DVec3::new(-20.0, -20.0, -40.0),
        ),
        DMat4::from_translation(DVec3::new(0.0, 0.0, 1.0))
            * DMat4::from_rotation_x(0.35)
            * DMat4::from_rotation_y(0.2),
    );
    check_all(&t, &slab, "schräge Ebene durch Torus");
}

#[test]
fn tilted_box_through_a_cone() {
    let cone = k().make_cone(6.0, 12.0).unwrap();
    let slab = moved(
        &box_solid(
            DVec3::new(40.0, 40.0, 40.0),
            DVec3::new(-20.0, -20.0, -40.0),
        ),
        DMat4::from_translation(DVec3::new(0.0, 0.0, 5.0))
            * DMat4::from_rotation_x(0.4)
            * DMat4::from_rotation_y(0.25),
    );
    check_all(&cone, &slab, "schräge Ebene durch Kegel");
}

#[test]
fn off_center_hole_through_a_sphere() {
    let ball = k().make_sphere(8.0).unwrap();
    let drill = moved(
        &centered_cylinder(2.0, 30.0),
        DMat4::from_translation(DVec3::new(3.0, 1.0, 0.0)) * DMat4::from_rotation_x(0.3),
    );
    check_all(&ball, &drill, "Bohrung neben der Kugelmitte");
}

#[test]
fn cone_against_a_skew_cylinder() {
    let cone = k().make_cone(6.0, 12.0).unwrap();
    let rod = moved(
        &centered_cylinder(1.5, 40.0),
        DMat4::from_translation(DVec3::new(0.5, 0.0, 4.0)) * DMat4::from_rotation_y(PI / 2.5),
    );
    check_all(&cone, &rod, "Kegel gegen schrägen Zylinder");
}
