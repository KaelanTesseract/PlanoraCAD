use planora_kernel::boolean::*;
use planora_kernel::*;
use planora_math::*;

fn tol() -> Tolerance {
    Tolerance::default()
}

fn k() -> AnalyticKernel {
    AnalyticKernel::default()
}

fn face_where(s: &Solid, f: impl Fn(&Face) -> bool) -> Id<Face> {
    s.faces
        .iter()
        .find(|(_, face)| f(face))
        .map(|(id, _)| id)
        .expect("Fläche nicht gefunden")
}

fn plane_face_at_z(s: &Solid, z: f64, up: bool) -> Id<Face> {
    face_where(s, |f| match &f.surface {
        Surface::Plane { frame } => {
            let n = if f.same_sense { frame.z } else { -frame.z };
            (frame.origin.z - z).abs() < 1e-9 && (n.z > 0.5) == up && n.z.abs() > 0.5
        }
        _ => false,
    })
}

#[test]
fn oriented_area_of_a_planar_face_is_positive() {
    let s = k().make_box(DVec3::new(10.0, 6.0, 4.0)).unwrap();
    for (id, f) in s.faces.iter() {
        let g = face_geom(&s, id);
        let outer = points_to_uv(&g.surface, g.same_sense, &loop_points(&g.loops[0]));
        assert!(oriented_area(&outer, g.same_sense) > 1.0, "{:?}", f.surface);
    }
}

#[test]
fn point_in_planar_face_with_hole() {
    let profile = Profile {
        outer: ProfileLoop::rectangle(DVec2::ZERO, DVec2::new(20.0, 20.0)),
        holes: vec![ProfileLoop::circle(DVec2::new(10.0, 10.0), 3.0)],
    };
    let s = k().extrude(&profile, &Frame::WORLD, 5.0).unwrap();
    let g = face_geom(&s, plane_face_at_z(&s, 5.0, true));
    assert_eq!(g.loops.len(), 2);
    assert_eq!(
        point_in_face(&g, DVec3::new(2.0, 2.0, 5.0), &tol()),
        Containment::Inside
    );
    assert_eq!(
        point_in_face(&g, DVec3::new(10.0, 10.0, 5.0), &tol()),
        Containment::Outside
    );
    assert_eq!(
        point_in_face(&g, DVec3::new(25.0, 10.0, 5.0), &tol()),
        Containment::Outside
    );
    assert_eq!(
        point_in_face(&g, DVec3::new(0.0, 7.0, 5.0), &tol()),
        Containment::Boundary
    );
    assert_eq!(
        point_in_face(&g, DVec3::new(13.0, 10.0, 5.0), &tol()),
        Containment::Boundary
    );
}

#[test]
fn point_in_a_cylinder_wall_across_the_seam() {
    let s = k().make_cylinder(5.0, 10.0).unwrap();
    let id = face_where(&s, |f| matches!(f.surface, Surface::Cylinder { .. }));
    let g = face_geom(&s, id);
    for angle in [0.1, 1.0, 3.0, 5.0, 6.2] {
        let p = DVec3::new(5.0 * f64::cos(angle), 5.0 * f64::sin(angle), 4.0);
        assert_eq!(
            point_in_face(&g, p, &tol()),
            Containment::Inside,
            "u = {angle}"
        );
    }
    // oberhalb der Höhe: außerhalb der Wand (nicht auf der Fläche), Punkt auf dem Rand
    assert_eq!(
        point_in_face(&g, DVec3::new(5.0, 0.0, 4.0), &tol()),
        Containment::Boundary
    );
    assert_eq!(
        point_in_face(&g, DVec3::new(0.0, 5.0, 11.0), &tol()),
        Containment::Outside
    );
}

#[test]
fn point_in_a_sphere_face_with_poles() {
    let s = k().make_sphere(5.0).unwrap();
    let g = face_geom(&s, Id::<Face>::from_index(0));
    let area = {
        let poly = points_to_uv(&g.surface, g.same_sense, &loop_points(&g.loops[0]));
        oriented_area(&poly, g.same_sense)
    };
    assert!(
        (area - TAU * PI).abs() / (TAU * PI) < 0.01,
        "UV-Fläche der ganzen Kugel: {area}"
    );
    for (u, v) in [(0.3, 0.2), (2.0, -1.2), (5.5, 1.4), (3.1, 0.0)] {
        let p = g.surface.point(u, v);
        assert_eq!(
            point_in_face(&g, p, &tol()),
            Containment::Inside,
            "u={u} v={v}"
        );
    }
}

#[test]
fn point_in_cone_and_torus_faces() {
    let cone = k().make_cone(3.0, 6.0).unwrap();
    let cid = face_where(&cone, |f| matches!(f.surface, Surface::Cone { .. }));
    let g = face_geom(&cone, cid);
    for angle in [0.4, 2.0, 4.5, 6.0] {
        let p = DVec3::new(1.5 * f64::cos(angle), 1.5 * f64::sin(angle), 3.0);
        assert_eq!(
            point_in_face(&g, p, &tol()),
            Containment::Inside,
            "u = {angle}"
        );
    }
    let puck = ProfileLoop {
        start: DVec2::ZERO,
        segments: vec![
            Segment::Line {
                to: DVec2::new(3.0, 0.0),
            },
            Segment::Arc {
                center: DVec2::new(3.0, 1.0),
                sweep: PI / 2.0,
            },
            Segment::Line {
                to: DVec2::new(0.0, 1.0),
            },
        ],
    };
    let solid = k().revolve(&puck, &Frame::WORLD).unwrap();
    let tid = face_where(&solid, |f| matches!(f.surface, Surface::Torus { .. }));
    let gt = face_geom(&solid, tid);
    let p = gt.surface.point(2.5, -PI / 4.0);
    assert_eq!(point_in_face(&gt, p, &tol()), Containment::Inside);
    // Punkt der Kegelfläche, der nicht auf dem Torusstück liegt
    let q = gt.surface.point(1.0, 1.5);
    assert_ne!(point_in_face(&gt, q, &tol()), Containment::Inside);
}

#[test]
fn mirrored_faces_work_too() {
    let m = DMat4::from_scale(DVec3::new(1.0, 1.0, -1.0));
    for solid in [
        k().make_cylinder(5.0, 10.0).unwrap(),
        k().make_sphere(4.0).unwrap(),
    ] {
        let mirrored = k().transform(&solid, &m).unwrap();
        for (id, f) in mirrored.faces.iter() {
            if matches!(f.surface, Surface::Plane { .. }) {
                continue;
            }
            let g = face_geom(&mirrored, id);
            let patch = f.patch.unwrap();
            let p = g.surface.point(
                0.5 * (patch.u0 + patch.u1) + 0.1,
                0.5 * (patch.v0 + patch.v1),
            );
            assert_eq!(
                point_in_face(&g, p, &tol()),
                Containment::Inside,
                "{:?}",
                f.surface
            );
        }
    }
}

#[test]
fn winding_counts_correctly() {
    let sq = [
        DVec2::new(0.0, 0.0),
        DVec2::new(2.0, 0.0),
        DVec2::new(2.0, 2.0),
        DVec2::new(0.0, 2.0),
    ];
    assert_eq!(winding(&sq, DVec2::new(1.0, 1.0)), 1);
    assert_eq!(winding(&sq, DVec2::new(3.0, 1.0)), 0);
    let rev: Vec<DVec2> = sq.iter().rev().cloned().collect();
    assert_eq!(winding(&rev, DVec2::new(1.0, 1.0)), -1);
}

/// Zusatztest: Torusstück, dessen `v`-Bereich über ±π hinausreicht (Hohlkehle auf der Innenseite
/// des Rohrs). Die Projektion liefert `v` in (−π, π]; die Schleife muss trotzdem stetig bleiben.
#[test]
fn torus_face_across_the_v_seam() {
    let groove = ProfileLoop {
        start: DVec2::ZERO,
        segments: vec![
            Segment::Line {
                to: DVec2::new(4.0, 0.0),
            },
            Segment::Line {
                to: DVec2::new(4.0, 1.0),
            },
            Segment::Arc {
                center: DVec2::new(5.0, 2.0),
                sweep: -PI / 2.0,
            },
            Segment::Line {
                to: DVec2::new(0.0, 3.0),
            },
        ],
    };
    let solid = k().revolve(&groove, &Frame::WORLD).unwrap();
    let tid = face_where(&solid, |f| matches!(f.surface, Surface::Torus { .. }));
    let g = face_geom(&solid, tid);
    let poly = points_to_uv(&g.surface, g.same_sense, &loop_points(&g.loops[0]));
    let area = oriented_area(&poly, g.same_sense);
    assert!(
        (area - TAU * PI / 2.0).abs() / (TAU * PI / 2.0) < 0.01,
        "UV-Fläche: {area}"
    );
    for (u, v) in [
        (1.0, -PI),
        (4.0, -1.1 * PI),
        (0.2, -0.8 * PI),
        (6.0, 0.9 * PI),
    ] {
        let p = g.surface.point(u, v);
        assert_eq!(
            point_in_face(&g, p, &tol()),
            Containment::Inside,
            "u={u} v={v}"
        );
    }
    for (u, v) in [(1.0, 0.0), (3.0, -PI / 2.0)] {
        let p = g.surface.point(u, v);
        assert_eq!(
            point_in_face(&g, p, &tol()),
            Containment::Outside,
            "u={u} v={v}"
        );
    }
}
