use planora_kernel::*;
use planora_math::*;
use planora_mesh::*;

fn k() -> AnalyticKernel {
    AnalyticKernel::default()
}

fn opts() -> TessellationOptions {
    TessellationOptions {
        chord_tolerance: 0.001,
    }
}

fn mesh_of(s: &Solid) -> Mesh {
    tessellate(s, &opts()).unwrap()
}

fn rel(a: f64, b: f64) -> f64 {
    ((a - b) / b).abs()
}

fn puck() -> ProfileLoop {
    ProfileLoop {
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
    }
}

#[test]
fn box_has_exact_volume_area_and_centroid() {
    let s = k().make_box(DVec3::new(2.0, 3.0, 4.0)).unwrap();
    let m = mesh_of(&s);
    let p = mass_properties(&m);
    assert!(rel(p.volume, 24.0) < 1e-12);
    assert!(rel(p.surface_area, 52.0) < 1e-12);
    assert!(p.centroid.distance(DVec3::new(1.0, 1.5, 2.0)) < 1e-9);
    assert!(is_watertight(&m, 1e-6));
}

#[test]
fn cylinder_volume_matches_formula() {
    let s = k().make_cylinder(5.0, 10.0).unwrap();
    let m = mesh_of(&s);
    let p = mass_properties(&m);
    assert!(rel(p.volume, PI * 25.0 * 10.0) < 0.005, "{}", p.volume);
    assert!(p.centroid.distance(DVec3::new(0.0, 0.0, 5.0)) < 1e-6);
    assert!(is_watertight(&m, 1e-6));
}

#[test]
fn sphere_volume_matches_formula() {
    let s = k().make_sphere(5.0).unwrap();
    let m = mesh_of(&s);
    let p = mass_properties(&m);
    assert!(
        rel(p.volume, 4.0 / 3.0 * PI * 125.0) < 0.005,
        "{}",
        p.volume
    );
    assert!(is_watertight(&m, 1e-6));
}

#[test]
fn cone_volume_matches_formula() {
    let s = k().make_cone(3.0, 6.0).unwrap();
    let m = mesh_of(&s);
    let p = mass_properties(&m);
    assert!(rel(p.volume, PI * 9.0 * 6.0 / 3.0) < 0.005, "{}", p.volume);
    assert!(is_watertight(&m, 1e-6));
}

#[test]
fn rounded_puck_volume_matches_pappus() {
    let s = k().revolve(&puck(), &Frame::WORLD).unwrap();
    let m = mesh_of(&s);
    let p = mass_properties(&m);
    // V = 2π ∫ x dA = 2π (4,5 + 3π/4 + 1/3)
    let expected = 2.0 * PI * (4.5 + 3.0 * PI / 4.0 + 1.0 / 3.0);
    assert!(
        rel(p.volume, expected) < 0.005,
        "{} vs {expected}",
        p.volume
    );
    assert!(is_watertight(&m, 1e-6));
}

#[test]
fn plate_with_hole_is_watertight_with_correct_volume() {
    let profile = Profile {
        outer: ProfileLoop::rectangle(DVec2::ZERO, DVec2::new(20.0, 20.0)),
        holes: vec![ProfileLoop::circle(DVec2::new(10.0, 10.0), 3.0)],
    };
    let s = k().extrude(&profile, &Frame::WORLD, 5.0).unwrap();
    let m = mesh_of(&s);
    let p = mass_properties(&m);
    assert!(
        rel(p.volume, 2000.0 - PI * 9.0 * 5.0) < 0.005,
        "{}",
        p.volume
    );
    assert!(is_watertight(&m, 1e-6));
}

#[test]
fn notched_bar_volume_matches_formula() {
    let outer = ProfileLoop {
        start: DVec2::new(0.0, 0.0),
        segments: vec![
            Segment::Line {
                to: DVec2::new(4.0, 0.0),
            },
            Segment::Line {
                to: DVec2::new(4.0, 2.0),
            },
            Segment::Line {
                to: DVec2::new(3.0, 2.0),
            },
            Segment::Arc {
                center: DVec2::new(2.0, 2.0),
                sweep: -PI,
            },
            Segment::Line {
                to: DVec2::new(0.0, 2.0),
            },
            Segment::Line {
                to: DVec2::new(0.0, 0.0),
            },
        ],
    };
    let s = k()
        .extrude(
            &Profile {
                outer,
                holes: vec![],
            },
            &Frame::WORLD,
            3.0,
        )
        .unwrap();
    let m = mesh_of(&s);
    let p = mass_properties(&m);
    assert!(
        rel(p.volume, 3.0 * (8.0 - PI / 2.0)) < 0.005,
        "{}",
        p.volume
    );
    assert!(is_watertight(&m, 1e-6));
}

#[test]
fn concave_l_shape_is_watertight() {
    let p = Profile {
        outer: ProfileLoop::polygon(&[
            DVec2::new(0.0, 0.0),
            DVec2::new(4.0, 0.0),
            DVec2::new(4.0, 1.0),
            DVec2::new(1.0, 1.0),
            DVec2::new(1.0, 3.0),
            DVec2::new(0.0, 3.0),
        ])
        .unwrap(),
        holes: vec![],
    };
    let s = k().extrude(&p, &Frame::WORLD, 2.0).unwrap();
    let m = mesh_of(&s);
    assert!(rel(mass_properties(&m).volume, 2.0 * (4.0 + 2.0)) < 1e-12);
    assert!(is_watertight(&m, 1e-6));
}

#[test]
fn face_ranges_cover_every_face_and_normals_are_unit() {
    let s = k().make_cylinder(2.0, 3.0).unwrap();
    let m = mesh_of(&s);
    assert_eq!(m.face_ranges.len(), s.faces.len());
    assert_eq!(m.indices.len() % 3, 0);
    let total: u32 = m.face_ranges.iter().map(|r| r.index_count).sum();
    assert_eq!(total as usize, m.indices.len());
    assert_eq!(m.positions.len(), m.normals.len());
    for n in &m.normals {
        assert!((n.length() - 1.0).abs() < 1e-9);
    }
    assert_eq!(m.edge_lines.len(), s.edges.len());
}

#[test]
fn huge_and_tiny_bodies_keep_reasonable_triangle_counts() {
    let big = k().make_cylinder(1.0e5, 10.0).unwrap();
    let m = tessellate(
        &big,
        &TessellationOptions {
            chord_tolerance: 0.01,
        },
    )
    .unwrap();
    assert!(m.indices.len() < 100_000, "{}", m.indices.len());

    let tiny = k().make_cylinder(0.01, 0.02).unwrap();
    let m = tessellate(
        &tiny,
        &TessellationOptions {
            chord_tolerance: 0.01,
        },
    )
    .unwrap();
    // Mantel: mindestens 16 Segmente × 2 Dreiecke × 3 Indizes
    let side = m.face_ranges.iter().map(|r| r.index_count).max().unwrap();
    assert!(side >= 16 * 6, "{side}");
}

/// Die App tessiert mit 0,05 mm; der Dialog erlaubt Radien bis 100 000 mm. Ohne Obergrenze der
/// Unterteilung entstünden zig Millionen Dreiecke (Einfrieren, GPU-Puffer zu groß).
#[test]
fn huge_sphere_keeps_a_bounded_mesh() {
    let r = 1.0e5;
    let s = k().make_sphere(r).unwrap();
    let m = tessellate(
        &s,
        &TessellationOptions {
            chord_tolerance: 0.05,
        },
    )
    .unwrap();
    assert!(m.indices.len() < 3_000_000, "{}", m.indices.len());
    assert!(m.positions.len() < 1_000_000, "{}", m.positions.len());
    assert!(is_watertight(&m, 1e-3));
    let p = mass_properties(&m);
    let expected = 4.0 / 3.0 * PI * r * r * r;
    assert!(p.volume > 0.0);
    assert!(rel(p.volume, expected) < 0.02, "{} vs {expected}", p.volume);
}

#[test]
fn huge_rounded_puck_keeps_a_bounded_mesh() {
    let scale = 1.0e4;
    let s = k().revolve(&puck(), &Frame::WORLD).unwrap();
    let s = k()
        .transform(&s, &DMat4::from_scale(DVec3::splat(scale)))
        .unwrap();
    let m = tessellate(
        &s,
        &TessellationOptions {
            chord_tolerance: 0.05,
        },
    )
    .unwrap();
    assert!(m.indices.len() < 3_000_000, "{}", m.indices.len());
    assert!(is_watertight(&m, 1e-3));
    let expected = 2.0 * PI * (4.5 + 3.0 * PI / 4.0 + 1.0 / 3.0) * scale.powi(3);
    let p = mass_properties(&m);
    assert!(rel(p.volume, expected) < 0.02, "{} vs {expected}", p.volume);
}
