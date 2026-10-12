//! Gemeinsame Hilfsfunktionen der Tests mit Booleschen Operationen auf Freiformflächen
//! (`nurbs_boolean.rs`, `nurbs_boolean_full.rs`, `nurbs_boolean_robust.rs`).

#![allow(dead_code)]

use planora_kernel::{
    AnalyticKernel, Curve, Kernel, Profile, ProfileLoop, Solid, Surface, UvRect, validate,
};
use planora_math::nurbs::{NurbsCurve, NurbsSurface};
use planora_math::{DMat4, DVec2, DVec3, Frame, TAU, Tolerance};
use planora_mesh::{TessellationOptions, is_watertight, mass_properties, tessellate};

pub fn k() -> AnalyticKernel {
    AnalyticKernel::default()
}

pub fn tol() -> Tolerance {
    Tolerance::default()
}

pub fn opts(c: f64) -> TessellationOptions {
    TessellationOptions { chord_tolerance: c }
}

/// Zylinder um die z-Achse (Boden bei z = 0), dessen Mantel eine NURBS-Drehfläche ist
/// (wie in `nurbs_mesh.rs`, Task 5).
pub fn nurbs_cylinder_solid(r: f64, h: f64) -> Solid {
    nurbs_cylinder_solid_from(r, h, false)
}

/// Wie [`nurbs_cylinder_solid`]; mit `at_seam_edge` beginnt die Drehfläche (u = u0) genau an der
/// Nahtkante des extrudierten Kreises, sonst bei (r, 0).
pub fn nurbs_cylinder_solid_from(r: f64, h: f64, at_seam_edge: bool) -> Solid {
    let profile = Profile {
        outer: ProfileLoop::circle(DVec2::ZERO, r),
        holes: vec![],
    };
    let mut solid = k()
        .extrude(
            &profile,
            &Frame::from_origin_normal(DVec3::ZERO, DVec3::Z),
            h,
        )
        .unwrap();
    let foot = if at_seam_edge {
        let seams: Vec<DVec3> = solid
            .edges
            .iter()
            .filter(|(_, e)| matches!(e.curve, Curve::Line { .. }))
            .map(|(_, e)| e.curve.point(e.t0.min(e.t1)))
            .collect();
        assert_eq!(seams.len(), 1, "eine Nahtkante");
        DVec3::new(seams[0].x, seams[0].y, 0.0)
    } else {
        DVec3::new(r, 0.0, 0.0)
    };
    let line = NurbsCurve::new(
        1,
        vec![0.0, 0.0, 1.0, 1.0],
        vec![foot, foot + DVec3::Z * h],
        vec![1.0; 2],
    )
    .unwrap();
    let nurbs = Surface::nurbs(
        NurbsSurface::revolve(&line, DVec3::ZERO, DVec3::Z, 0.0, TAU).unwrap(),
        true,
        false,
    )
    .unwrap();
    let ids: Vec<_> = solid
        .faces
        .iter()
        .filter(|(_, f)| matches!(f.surface, Surface::Cylinder { .. }))
        .map(|(id, _)| id)
        .collect();
    assert_eq!(ids.len(), 1);
    solid.faces[ids[0]].surface = nurbs;
    solid.faces[ids[0]].patch = Some(UvRect {
        u0: 0.0,
        u1: TAU,
        v0: 0.0,
        v1: 1.0,
    });
    solid
}

/// Analytischer Quader der Größe `size` mit der Ecke `origin`.
pub fn box_solid(size: DVec3, origin: DVec3) -> Solid {
    k().transform(
        &k().make_box(size).unwrap(),
        &DMat4::from_translation(origin),
    )
    .unwrap()
}

pub fn volume(s: &Solid) -> f64 {
    mass_properties(&tessellate(s, &opts(0.002)).unwrap()).volume
}

/// Quader 20 × 20 × 10 um den Ursprung (z ∈ [−5, 5]).
pub fn centered_box() -> Solid {
    box_solid(DVec3::new(20.0, 20.0, 10.0), DVec3::new(-10.0, -10.0, -5.0))
}

/// NURBS-Zylinder r = 3, h = 20 ab z = −10, um `angle` um die z-Achse gedreht und um `dx` in x
/// verschoben.
pub fn placed_cylinder(angle: f64, dx: f64) -> Solid {
    k().transform(
        &nurbs_cylinder_solid(3.0, 20.0),
        &(DMat4::from_translation(DVec3::new(dx, 0.0, -10.0)) * DMat4::from_rotation_z(angle)),
    )
    .unwrap()
}

pub fn is_nurbs(s: &Surface) -> bool {
    matches!(s, Surface::Nurbs(_))
}

/// Prüft ein Ergebnis: `validate` ok, Netz wasserdicht; liefert das Netzvolumen.
pub fn checked_volume(s: &Solid) -> f64 {
    validate(s).unwrap_or_else(|e| panic!("validate: {e}"));
    let m = tessellate(s, &opts(0.002)).unwrap();
    assert!(is_watertight(&m, 1e-6), "Netz nicht geschlossen");
    mass_properties(&m).volume
}

pub fn rel(a: f64, b: f64) -> f64 {
    ((a - b) / b).abs()
}

pub fn count_nurbs_faces(s: &Solid) -> usize {
    s.faces.iter().filter(|(_, f)| is_nurbs(&f.surface)).count()
}

/// Kugel r um den Ursprung, deren Fläche eine NURBS-Drehfläche (Halbkreis) ist; Pole auf der
/// z-Achse (wie in `planora-kernel/tests/nurbs_ray.rs`).
pub fn nurbs_sphere_solid(r: f64) -> Solid {
    let mut solid = k().make_sphere(r).unwrap();
    let half = NurbsCurve::elliptic_arc(
        DVec3::ZERO,
        DVec3::new(0.0, 0.0, -r),
        DVec3::new(r, 0.0, 0.0),
        0.0,
        std::f64::consts::PI,
    )
    .unwrap();
    let ns = NurbsSurface::revolve(&half, DVec3::ZERO, DVec3::Z, 0.0, TAU).unwrap();
    let ((u0, u1), (v0, v1)) = ns.domain();
    let surface = Surface::nurbs(ns, true, false).unwrap();
    let (id, f) = solid.faces.iter().next().unwrap();
    let p = DVec3::new(r * 0.6, r * 0.8, 0.0);
    let (ou, ov) = f.surface.project(p);
    let outward = f.surface.normal(ou, ov) * if f.same_sense { 1.0 } else { -1.0 };
    let (nu, nv) = surface.project(p);
    let same_sense = surface.normal(nu, nv).dot(outward) > 0.0;
    let f = &mut solid.faces[id];
    f.surface = surface;
    f.same_sense = same_sense;
    f.patch = Some(UvRect { u0, u1, v0, v1 });
    solid
}
