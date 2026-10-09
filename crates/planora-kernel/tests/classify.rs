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

// --- Nachbesserung 1 ---

/// Erste feste Strahlrichtung von `solid_contains` (siehe `classify.rs`).
fn first_dir() -> DVec3 {
    DVec3::new(0.7321, 0.4123, 0.5471).normalize()
}

fn line(a: DVec3, b: DVec3) -> Seg {
    Seg::new(
        Curve::Line {
            origin: a,
            dir: (b - a).normalize(),
        },
        0.0,
        a.distance(b),
    )
}

fn face_at(faces: &[FaceGeom], p: DVec3, pred: impl Fn(&FaceGeom) -> bool) -> &FaceGeom {
    faces
        .iter()
        .find(|g| g.surface.implicit(p).abs() < 1e-9 && pred(g))
        .expect("Fläche nicht gefunden")
}

#[test]
fn ray_through_cap_inside_the_chord_sag_of_its_rim_is_not_lost() {
    // Deckel z = 8 eines Zylinders r = 10: zwischen Sehne und Bogen des Randkreises liegt ein
    // schmaler Streifen, den das grobe Sehnenpolygon von `point_in_face` als außen ansieht.
    let r = 10.0;
    let cyl = k().make_cylinder(r, 8.0).unwrap();
    let faces = solid_faces(&cyl);
    let top = DVec3::new(0.0, 0.0, 8.0);
    let cap = face_at(&faces, top, |g| matches!(g.surface, Surface::Plane { .. }));
    let pts = loop_points(&cap.loops[0]);
    let d = first_dir();
    let want = d.y.atan2(d.x);
    // Sehnenmitte, deren Richtung der Strahlrichtung (in xy) am nächsten liegt
    let m = (0..pts.len())
        .map(|i| (pts[i] + pts[(i + 1) % pts.len()]) / 2.0)
        .min_by(|a, b| {
            let da = (a.y.atan2(a.x) - want).abs();
            let db = (b.y.atan2(b.x) - want).abs();
            da.total_cmp(&db)
        })
        .unwrap();
    let radial = DVec3::new(m.x, m.y, 0.0);
    let sag = r - radial.length();
    assert!(sag > 1e-3, "Sehnenpfeil {sag}");
    let hit = top + radial.normalize() * (r - 0.5 * sag);
    assert_eq!(
        point_in_face(cap, hit, &tol()),
        Containment::Outside,
        "Voraussetzung: Polygon sieht den Treffer außen"
    );
    let p = hit - d * 4.0;
    assert!(p.truncate().length() < r - 1.0 && p.z > 1.0 && p.z < 7.0);
    match solid_contains(&faces, p, &tol()) {
        Ok(loc) => assert_eq!(loc, Location::Inside),
        Err(e) => assert_eq!(e, BooleanError::AmbiguousClassification),
    }
}

#[test]
fn ray_hitting_a_seam_is_retried() {
    let cyl = k().make_cylinder(3.0, 8.0).unwrap();
    let faces = solid_faces(&cyl);
    let side = faces
        .iter()
        .find(|g| matches!(g.surface, Surface::Cylinder { .. }))
        .unwrap();
    let seam = side.loops[0]
        .iter()
        .find(|s| matches!(s.curve, Curve::Line { .. }))
        .expect("Naht");
    let s = seam.at(0.5);
    let p = s - first_dir() * 2.0;
    assert!(p.truncate().length() < 3.0, "Startpunkt im Zylinder");
    assert_eq!(solid_contains(&faces, p, &tol()).unwrap(), Location::Inside);
}

#[test]
fn region_with_a_hole_uses_a_point_off_the_hole() {
    let a = k().make_box(DVec3::splat(10.0)).unwrap();
    let fa = solid_faces(&a);
    let g = face_at(&fa, DVec3::new(5.0, 5.0, 10.0), |g| {
        matches!(g.surface, Surface::Plane { .. })
    });
    // Loch [2, 8]² läuft um die Außennormale +z im Uhrzeigersinn.
    let c = |x: f64, y: f64| DVec3::new(x, y, 10.0);
    let hole = vec![
        line(c(2.0, 2.0), c(2.0, 8.0)),
        line(c(2.0, 8.0), c(8.0, 8.0)),
        line(c(8.0, 8.0), c(8.0, 2.0)),
        line(c(8.0, 2.0), c(2.0, 2.0)),
    ];
    let other = moved(
        &k().make_box(DVec3::new(4.0, 4.0, 10.0)).unwrap(),
        DVec3::new(3.0, 3.0, 5.0),
    );
    let fo = solid_faces(&other);
    let whole = Region {
        outer: g.loops[0].clone(),
        holes: vec![],
    };
    assert_eq!(
        classify_region(g, &whole, &fo, &tol()).unwrap(),
        Class::Inside,
        "Mitte liegt im anderen Körper"
    );
    let ring = Region {
        outer: g.loops[0].clone(),
        holes: vec![hole],
    };
    assert_eq!(
        classify_region(g, &ring, &fo, &tol()).unwrap(),
        Class::Outside
    );
}

#[test]
fn hole_on_a_cylinder_unrolled_one_period_away_is_recognised() {
    let cyl = k().make_cylinder(3.0, 8.0).unwrap();
    let fc = solid_faces(&cyl);
    let side = fc
        .iter()
        .find(|g| matches!(g.surface, Surface::Cylinder { .. }))
        .unwrap();
    let outer_uv = points_to_uv(&side.surface, side.same_sense, &loop_points(&side.loops[0]));
    let (umin, umax) = outer_uv
        .iter()
        .fold((f64::MAX, f64::MIN), |(a, b), p| (a.min(p.x), b.max(p.x)));
    // Großes Loch: alles außer einem Streifen um die Naht (Winkel c ± 150° um die Gegenseite),
    // z ∈ [0.5, 7.5], im (u, v)-Raum im Uhrzeigersinn. Ohne Loch läge der Innenpunkt darin.
    let seam = side.loops[0]
        .iter()
        .find(|s| matches!(s.curve, Curve::Line { .. }))
        .expect("Naht")
        .at(0.5);
    let c = seam.y.atan2(seam.x).to_degrees() + 180.0;
    let at = |deg: f64, z: f64| {
        DVec3::new(
            3.0 * deg.to_radians().cos(),
            3.0 * deg.to_radians().sin(),
            z,
        )
    };
    let arc = |z: f64, from: f64, to: f64| {
        Seg::new(
            Curve::Circle {
                frame: Frame {
                    origin: DVec3::new(0.0, 0.0, z),
                    ..Frame::WORLD
                },
                radius: 3.0,
            },
            from.to_radians(),
            to.to_radians(),
        )
    };
    let base = [
        line(at(c - 150.0, 0.5), at(c - 150.0, 7.5)),
        arc(7.5, c - 150.0, c + 150.0),
        line(at(c + 150.0, 7.5), at(c + 150.0, 0.5)),
        arc(0.5, c + 150.0, c - 150.0),
    ];
    // Beginn so wählen, dass das Loch beim Aufrollen außerhalb des u-Bereichs der Außenschleife liegt.
    let hole = (0..4)
        .map(|k| {
            base[k..]
                .iter()
                .chain(base[..k].iter())
                .cloned()
                .collect::<Vec<_>>()
        })
        .find(|h| {
            let uv = points_to_uv(&side.surface, side.same_sense, &loop_points(h));
            let mid = uv.iter().map(|p| p.x).sum::<f64>() / uv.len() as f64;
            mid < umin || mid > umax
        })
        .expect("ein um eine Periode versetztes Aufrollen des Lochs");
    let other = moved(
        &k().make_box(DVec3::new(6.0, 8.0, 10.0)).unwrap(),
        DVec3::new(-4.0, -4.0, -1.0),
    );
    // Der andere Körper enthält alles mit x < 2, also nicht den Streifen um die Naht bei x = 3.
    assert!(seam.x > 2.9 && seam.y.abs() < 1e-9, "Naht bei +x");
    let fo = solid_faces(&other);
    let whole = Region {
        outer: side.loops[0].clone(),
        holes: vec![],
    };
    assert_eq!(
        classify_region(side, &whole, &fo, &tol()).unwrap(),
        Class::Inside,
        "ohne Loch liegt der Innenpunkt im anderen Körper"
    );
    let ring = Region {
        outer: side.loops[0].clone(),
        holes: vec![hole],
    };
    assert_eq!(
        classify_region(side, &ring, &fo, &tol()).unwrap(),
        Class::Outside
    );
}

#[test]
fn faces_with_reversed_sense_are_classified() {
    let m = DMat4::from_scale(DVec3::new(1.0, 1.0, -1.0));
    let a = k()
        .transform(&k().make_box(DVec3::splat(10.0)).unwrap(), &m)
        .unwrap();
    assert!(
        solid_faces(&a).iter().any(|g| !g.same_sense),
        "Spiegelung kehrt den Flächensinn um"
    );
    let big = moved(
        &k().make_box(DVec3::splat(20.0)).unwrap(),
        DVec3::new(-5.0, -5.0, -15.0),
    );
    let b = moved(&a, DVec3::new(10.0, 0.0, 0.0));
    for i in 0..6 {
        assert_eq!(whole_face_class(&a, &a, i), Class::OnSame, "Fläche {i}");
        assert_eq!(whole_face_class(&a, &big, i), Class::Inside, "Fläche {i}");
    }
    let on = (0..6)
        .filter(|&i| whole_face_class(&a, &b, i) == Class::OnOpposite)
        .count();
    assert_eq!(on, 1);
}

#[test]
fn curved_whole_faces_are_classified() {
    let cyl = k().make_cylinder(3.0, 8.0).unwrap();
    let big = moved(
        &k().make_box(DVec3::splat(20.0)).unwrap(),
        DVec3::new(-10.0, -10.0, -5.0),
    );
    let far = moved(&cyl, DVec3::new(30.0, 0.0, 0.0));
    let mirrored = k()
        .transform(&cyl, &DMat4::from_scale(DVec3::new(1.0, 1.0, -1.0)))
        .unwrap();
    let n = solid_faces(&cyl).len();
    for i in 0..n {
        assert_eq!(whole_face_class(&cyl, &cyl, i), Class::OnSame, "Fläche {i}");
        assert_eq!(whole_face_class(&cyl, &big, i), Class::Inside, "Fläche {i}");
        assert_eq!(
            whole_face_class(&cyl, &far, i),
            Class::Outside,
            "Fläche {i}"
        );
    }
    for i in 0..solid_faces(&mirrored).len() {
        assert_eq!(
            whole_face_class(&mirrored, &mirrored, i),
            Class::OnSame,
            "gespiegelt {i}"
        );
    }
    let sph = k().make_sphere(4.0).unwrap();
    for i in 0..solid_faces(&sph).len() {
        assert_eq!(whole_face_class(&sph, &sph, i), Class::OnSame, "Kugel {i}");
        assert_eq!(whole_face_class(&sph, &big, i), Class::Inside, "Kugel {i}");
    }
}
