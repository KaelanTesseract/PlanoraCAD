//! Querschnitt der Rundung und der Fase (Teilprojekt 8, Task 2).

use planora_kernel::Surface;
use planora_kernel::blend::BlendError;
use planora_kernel::blend::section::*;
use planora_math::{DVec3, Frame};

fn plane(origin: DVec3, normal: DVec3) -> Surface {
    Surface::Plane {
        frame: Frame::from_origin_normal(origin, normal),
    }
}

fn side(s: &Surface) -> Side<'_> {
    Side {
        surface: s,
        same_sense: true,
    }
}

#[test]
fn two_perpendicular_planes_give_the_textbook_ball() {
    // Deckel z = 10 (außen +z), Vorderwand y = 0 (außen −y), Kante längs x.
    let top = plane(DVec3::new(0.0, 0.0, 10.0), DVec3::Z);
    let front = plane(DVec3::ZERO, -DVec3::Y);
    let o = DVec3::new(3.0, 0.0, 10.0);
    let s = fillet_section(side(&top), side(&front), o, DVec3::X, true, 2.0, None).unwrap();
    assert!(
        s.center.distance(DVec3::new(3.0, 2.0, 8.0)) < 1e-12,
        "{:?}",
        s.center
    );
    assert!(
        s.pa.distance(DVec3::new(3.0, 2.0, 10.0)) < 1e-12,
        "{:?}",
        s.pa
    );
    assert!(
        s.pb.distance(DVec3::new(3.0, 0.0, 8.0)) < 1e-12,
        "{:?}",
        s.pb
    );
    // Konkav: Kugel außerhalb des Materials.
    let c = fillet_section(side(&top), side(&front), o, DVec3::X, false, 2.0, None).unwrap();
    assert!(
        c.center.distance(DVec3::new(3.0, -2.0, 12.0)) < 1e-12,
        "{:?}",
        c.center
    );
}

#[test]
fn plane_and_cylinder_and_sphere_converge_to_the_exact_center() {
    // Zylinder r = 5 (Achse z), Deckel z = 8: Kugel r = 1 bei ρ = 4, z = 7.
    let cyl = Surface::Cylinder {
        frame: Frame::WORLD,
        radius: 5.0,
    };
    let top = plane(DVec3::new(0.0, 0.0, 8.0), DVec3::Z);
    let o = DVec3::new(5.0, 0.0, 8.0);
    let s = fillet_section(side(&cyl), side(&top), o, DVec3::Y, true, 1.0, None).unwrap();
    assert!(
        s.center.distance(DVec3::new(4.0, 0.0, 7.0)) < 1e-10,
        "{:?}",
        s.center
    );
    // Halbkugel R = 5 auf der Ebene z = 0 (außen −z): Kugel r = 1 berührt die Kugel innen.
    let sphere = Surface::Sphere {
        frame: Frame::WORLD,
        radius: 5.0,
    };
    let base = plane(DVec3::ZERO, -DVec3::Z);
    let o = DVec3::new(5.0, 0.0, 0.0);
    let s = fillet_section(side(&sphere), side(&base), o, DVec3::Y, true, 1.0, None).unwrap();
    let want = DVec3::new((16.0f64 - 1.0).sqrt(), 0.0, 1.0);
    assert!(s.center.distance(want) < 1e-10, "{:?}", s.center);
    assert!((s.pa.length() - 5.0).abs() < 1e-10);
    assert!(s.pb.z.abs() < 1e-10);
}

#[test]
fn chamfer_points_by_distance_and_by_angle() {
    let top = plane(DVec3::new(0.0, 0.0, 10.0), DVec3::Z);
    let front = plane(DVec3::ZERO, -DVec3::Y);
    let o = DVec3::new(3.0, 0.0, 10.0);
    let (wa, wb) = (DVec3::Y, -DVec3::Z);
    let s = chamfer_section(
        side(&top),
        side(&front),
        o,
        DVec3::X,
        true,
        wa,
        wb,
        ChamferSpec::Distances { a: 1.0, b: 2.5 },
    )
    .unwrap();
    assert!(s.pa.distance(o + DVec3::Y) < 1e-12);
    assert!(s.pb.distance(o - DVec3::Z * 2.5) < 1e-12);
    assert_eq!(s.center, o);
    let angle = 30f64.to_radians();
    let s = chamfer_section(
        side(&top),
        side(&front),
        o,
        DVec3::X,
        true,
        wa,
        wb,
        ChamferSpec::DistanceAngle {
            distance: 1.0,
            angle,
        },
    )
    .unwrap();
    assert!(
        s.pb.distance(o - DVec3::Z * angle.tan()) < 1e-10,
        "{:?}",
        s.pb
    );
    // Ein Winkel, dessen Gerade die zweite Fläche nicht trifft, wird benannt abgelehnt.
    let r = chamfer_section(
        side(&top),
        side(&front),
        o,
        DVec3::X,
        true,
        wa,
        wb,
        ChamferSpec::DistanceAngle {
            distance: 1.0,
            angle: 170f64.to_radians(),
        },
    );
    assert!(
        matches!(&r, Err(BlendError::InvalidParameter(m)) if m.contains("Winkel")),
        "{r:?}"
    );
}

#[test]
fn opposite_normals_lose_the_spine_instead_of_dividing_by_zero() {
    let a = plane(DVec3::ZERO, DVec3::Z);
    let b = plane(DVec3::ZERO, -DVec3::Z);
    let r = fillet_section(side(&a), side(&b), DVec3::ZERO, DVec3::X, true, 1.0, None);
    assert_eq!(r, Err(BlendError::SpineLost));
}

#[test]
fn a_ball_larger_than_the_curvature_is_named() {
    // Zylinder r = 1, Kugel im Material (konvexe Kante, Mantel nach außen): r = 1,5 passt nicht.
    let cyl = Surface::Cylinder {
        frame: Frame::WORLD,
        radius: 1.0,
    };
    let r = check_curvature(side(&cyl), DVec3::new(1.0, 0.0, 0.0), true, 1.5, "Wand 1");
    assert_eq!(
        r,
        Err(BlendError::CurvatureTooSmall {
            face: "Wand 1".into()
        })
    );
    assert!(check_curvature(side(&cyl), DVec3::new(1.0, 0.0, 0.0), true, 0.5, "Wand 1").is_ok());
    // Kugel außerhalb des Zylinders (konkave Kante an einem Zapfen): keine Grenze.
    assert!(check_curvature(side(&cyl), DVec3::new(1.0, 0.0, 0.0), false, 5.0, "Wand 1").is_ok());
    // Bohrung (Mantel nach innen, same_sense falsch) und konkave Kante: Kugel liegt in der Bohrung.
    let hole = Side {
        surface: &cyl,
        same_sense: false,
    };
    assert!(check_curvature(hole, DVec3::new(1.0, 0.0, 0.0), false, 1.5, "Loch").is_err());
}
