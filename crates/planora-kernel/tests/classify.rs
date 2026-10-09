use planora_kernel::boolean::*;
use planora_kernel::*;
use planora_math::*;

fn tol() -> Tolerance {
    Tolerance::default()
}

fn k() -> AnalyticKernel {
    AnalyticKernel::default()
}

fn moved(s: &Solid, v: DVec3) -> Solid {
    k().transform(s, &DMat4::from_translation(v)).unwrap()
}

fn contains(s: &Solid, p: DVec3) -> Location {
    solid_contains(&solid_faces(s), p, &tol()).unwrap()
}

#[test]
fn box_contains_points() {
    let b = k().make_box(DVec3::splat(10.0)).unwrap();
    assert_eq!(contains(&b, DVec3::splat(5.0)), Location::Inside);
    assert_eq!(contains(&b, DVec3::new(15.0, 5.0, 5.0)), Location::Outside);
    assert_eq!(
        contains(&b, DVec3::new(10.0, 5.0, 5.0)),
        Location::OnBoundary
    );
    assert_eq!(
        contains(&b, DVec3::new(-3.0, -3.0, -3.0)),
        Location::Outside
    );
}

#[test]
fn curved_solids_contain_points() {
    let cyl = k().make_cylinder(3.0, 8.0).unwrap();
    assert_eq!(contains(&cyl, DVec3::new(1.0, 1.0, 4.0)), Location::Inside);
    assert_eq!(contains(&cyl, DVec3::new(4.0, 0.0, 4.0)), Location::Outside);
    assert_eq!(contains(&cyl, DVec3::new(0.0, 0.0, 9.0)), Location::Outside);
    let sph = k().make_sphere(4.0).unwrap();
    assert_eq!(contains(&sph, DVec3::new(1.0, -1.0, 2.0)), Location::Inside);
    assert_eq!(contains(&sph, DVec3::new(4.0, 4.0, 0.0)), Location::Outside);
    assert_eq!(
        contains(&sph, DVec3::new(0.0, 0.0, 4.0)),
        Location::OnBoundary
    );
    let cone = k().make_cone(3.0, 6.0).unwrap();
    assert_eq!(contains(&cone, DVec3::new(0.5, 0.0, 1.0)), Location::Inside);
    assert_eq!(
        contains(&cone, DVec3::new(2.5, 0.0, 3.0)),
        Location::Outside
    );
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
    let t = k().revolve(&puck, &Frame::WORLD).unwrap();
    assert_eq!(contains(&t, DVec3::new(2.0, 0.0, 0.5)), Location::Inside);
    assert_eq!(contains(&t, DVec3::new(3.9, 0.0, 0.05)), Location::Outside); // wegen der Rundung außerhalb
    assert_eq!(contains(&t, DVec3::new(6.0, 0.0, 0.5)), Location::Outside);
}

#[test]
fn mirrored_and_rotated_solids_work_too() {
    let m = DMat4::from_scale(DVec3::new(1.0, 1.0, -1.0));
    let cyl = k()
        .transform(&k().make_cylinder(3.0, 8.0).unwrap(), &m)
        .unwrap();
    assert_eq!(contains(&cyl, DVec3::new(1.0, 1.0, -4.0)), Location::Inside);
    assert_eq!(contains(&cyl, DVec3::new(1.0, 1.0, 4.0)), Location::Outside);
    let rot = DMat4::from_axis_angle(DVec3::new(1.0, 2.0, 3.0).normalize(), 0.7)
        * DMat4::from_translation(DVec3::new(1.0, 1.0, 1.0));
    let b = k()
        .transform(&k().make_box(DVec3::splat(4.0)).unwrap(), &rot)
        .unwrap();
    let c = rot.transform_point3(DVec3::splat(2.0));
    assert_eq!(contains(&b, c), Location::Inside);
}

fn whole_face_class(a: &Solid, b: &Solid, face_index: usize) -> Class {
    let fa = solid_faces(a);
    let fb = solid_faces(b);
    let g = &fa[face_index];
    let region = Region {
        outer: g.loops[0].clone(),
        holes: g.loops[1..].to_vec(),
    };
    classify_region(g, &region, &fb, &tol()).unwrap()
}

#[test]
fn whole_faces_are_classified_against_the_other_solid() {
    let a = k().make_box(DVec3::splat(10.0)).unwrap();
    let far = moved(&a, DVec3::new(30.0, 0.0, 0.0));
    for i in 0..6 {
        assert_eq!(whole_face_class(&a, &far, i), Class::Outside, "Fläche {i}");
    }
    let big = moved(
        &k().make_box(DVec3::splat(20.0)).unwrap(),
        DVec3::splat(-5.0),
    );
    for i in 0..6 {
        assert_eq!(whole_face_class(&a, &big, i), Class::Inside, "Fläche {i}");
    }
    for i in 0..6 {
        assert_eq!(whole_face_class(&a, &a, i), Class::OnSame, "Fläche {i}");
    }
}

#[test]
fn touching_boxes_have_opposite_coincident_faces() {
    let a = k().make_box(DVec3::splat(10.0)).unwrap();
    let b = moved(&a, DVec3::new(10.0, 0.0, 0.0));
    let fa = solid_faces(&a);
    let on: Vec<usize> = (0..6)
        .filter(|&i| whole_face_class(&a, &b, i) == Class::OnOpposite)
        .collect();
    assert_eq!(on.len(), 1, "genau die Fläche x = 10");
    let g = &fa[on[0]];
    assert!(matches!(&g.surface, Surface::Plane { frame } if (frame.origin.x - 10.0).abs() < 1e-9));
}
