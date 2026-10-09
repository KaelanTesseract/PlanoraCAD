use planora_kernel::boolean::*;
use planora_kernel::*;
use planora_math::*;

fn tol() -> Tolerance {
    Tolerance::default()
}

fn k() -> AnalyticKernel {
    AnalyticKernel::default()
}

fn find_face(s: &Solid, f: impl Fn(&Face) -> bool) -> FaceGeom {
    let (id, _) = s.faces.iter().find(|(_, face)| f(face)).expect("Fläche");
    face_geom(s, id)
}

fn area(g: &FaceGeom, lp: &[Seg]) -> f64 {
    let poly = points_to_uv(&g.surface, g.same_sense, &loop_points(lp));
    oriented_area(&poly, g.same_sense)
}

fn region_area(g: &FaceGeom, r: &Region) -> f64 {
    area(g, &r.outer) + r.holes.iter().map(|h| area(g, h)).sum::<f64>()
}

fn line(o: DVec3, d: DVec3, len: f64) -> Seg {
    Seg::new(
        Curve::Line {
            origin: o,
            dir: d.normalize(),
        },
        0.0,
        len,
    )
}

fn top_face(s: &Solid, z: f64) -> FaceGeom {
    find_face(s, |f| match &f.surface {
        Surface::Plane { frame } => {
            (frame.origin.z - z).abs() < 1e-9
                && frame.z.z.abs() > 0.5
                && f.same_sense == (frame.z.z > 0.0)
        }
        _ => false,
    })
}

fn sorted_areas(g: &FaceGeom, regions: &[Region]) -> Vec<f64> {
    let mut a: Vec<f64> = regions.iter().map(|r| region_area(g, r)).collect();
    a.sort_by(|x, y| x.total_cmp(y));
    a
}

fn close(a: f64, b: f64, rel: f64) {
    assert!((a - b).abs() <= rel * b.abs().max(1e-9), "{a} != {b}");
}

#[test]
fn untouched_face_gives_itself() {
    let s = k().make_box(DVec3::new(10.0, 10.0, 10.0)).unwrap();
    let g = top_face(&s, 10.0);
    let r = split_face(&g, &[], &tol()).unwrap();
    assert_eq!(r.len(), 1);
    assert!(r[0].holes.is_empty());
    close(region_area(&g, &r[0]), 100.0, 1e-9);
}

#[test]
fn existing_hole_stays_a_hole() {
    let profile = Profile {
        outer: ProfileLoop::rectangle(DVec2::ZERO, DVec2::new(20.0, 20.0)),
        holes: vec![ProfileLoop::circle(DVec2::new(10.0, 10.0), 3.0)],
    };
    let s = k().extrude(&profile, &Frame::WORLD, 5.0).unwrap();
    let g = top_face(&s, 5.0);
    let r = split_face(&g, &[], &tol()).unwrap();
    assert_eq!(r.len(), 1);
    assert_eq!(r[0].holes.len(), 1);
}

#[test]
fn a_line_splits_a_planar_face_in_two() {
    let s = k().make_box(DVec3::new(10.0, 10.0, 10.0)).unwrap();
    let g = top_face(&s, 10.0);
    let cut = line(DVec3::new(4.0, 0.0, 10.0), DVec3::Y, 10.0);
    let r = split_face(&g, &[cut], &tol()).unwrap();
    assert_eq!(r.len(), 2);
    let a = sorted_areas(&g, &r);
    close(a[0], 40.0, 1e-6);
    close(a[1], 60.0, 1e-6);
}

#[test]
fn a_closed_circle_makes_a_disk_and_a_face_with_a_hole() {
    let s = k().make_box(DVec3::new(10.0, 10.0, 10.0)).unwrap();
    let g = top_face(&s, 10.0);
    let f = Frame::from_origin_normal(DVec3::new(5.0, 5.0, 10.0), DVec3::Z);
    let circle = Seg::new(
        Curve::Circle {
            frame: f,
            radius: 2.0,
        },
        0.0,
        TAU,
    );
    let r = split_face(&g, &[circle], &tol()).unwrap();
    assert_eq!(r.len(), 2);
    let with_hole = r
        .iter()
        .find(|x| x.holes.len() == 1)
        .expect("Region mit Loch");
    let disk = r.iter().find(|x| x.holes.is_empty()).expect("Scheibe");
    close(region_area(&g, with_hole), 100.0 - PI * 4.0, 0.01);
    close(region_area(&g, disk), PI * 4.0, 0.01);
}

#[test]
fn two_crossing_cuts_make_four_pieces() {
    let s = k().make_box(DVec3::new(10.0, 10.0, 10.0)).unwrap();
    let g = top_face(&s, 10.0);
    // beide Schnitte enden am Rand und treffen sich in (5,5): jeweils in zwei Hälften
    let secs = [
        line(DVec3::new(5.0, 0.0, 10.0), DVec3::Y, 5.0),
        line(DVec3::new(5.0, 5.0, 10.0), DVec3::Y, 5.0),
        line(DVec3::new(0.0, 5.0, 10.0), DVec3::X, 5.0),
        line(DVec3::new(5.0, 5.0, 10.0), DVec3::X, 5.0),
    ];
    let r = split_face(&g, &secs, &tol()).unwrap();
    assert_eq!(r.len(), 4);
    for x in sorted_areas(&g, &r) {
        close(x, 25.0, 1e-6);
    }
}

#[test]
fn a_dangling_cut_is_an_error() {
    let s = k().make_box(DVec3::new(10.0, 10.0, 10.0)).unwrap();
    let g = top_face(&s, 10.0);
    let dangling = line(DVec3::new(5.0, 0.0, 10.0), DVec3::Y, 4.0); // endet frei im Inneren
    assert!(split_face(&g, &[dangling], &tol()).is_err());
}

#[test]
fn a_cylinder_wall_is_split_by_a_circle_across_the_seam() {
    let s = k().make_cylinder(5.0, 10.0).unwrap();
    let g = find_face(&s, |f| matches!(f.surface, Surface::Cylinder { .. }));
    let f = Frame::from_origin_normal(DVec3::new(0.0, 0.0, 4.0), DVec3::Z);
    let circle = Seg::new(
        Curve::Circle {
            frame: f,
            radius: 5.0,
        },
        0.0,
        TAU,
    );
    let r = split_face(&g, &[circle], &tol()).unwrap();
    assert_eq!(r.len(), 2);
    let a = sorted_areas(&g, &r);
    close(a[0], TAU * 4.0, 0.01);
    close(a[1], TAU * 6.0, 0.01);
}

#[test]
fn a_cylinder_wall_is_split_by_two_axial_lines_with_the_seam_inside_one_piece() {
    let s = k().make_cylinder(5.0, 10.0).unwrap();
    let g = find_face(&s, |f| matches!(f.surface, Surface::Cylinder { .. }));
    let l1 = line(DVec3::new(0.0, 5.0, 0.0), DVec3::Z, 10.0); // u = π/2
    let l2 = line(DVec3::new(-5.0, 0.0, 0.0), DVec3::Z, 10.0); // u = π
    let r = split_face(&g, &[l1, l2], &tol()).unwrap();
    assert_eq!(r.len(), 2);
    let a = sorted_areas(&g, &r);
    close(a[0], (PI / 2.0) * 10.0, 0.01);
    close(a[1], (3.0 * PI / 2.0) * 10.0, 0.01);
}

#[test]
fn a_sphere_is_split_by_a_latitude_circle() {
    let s = k().make_sphere(5.0).unwrap();
    let g = face_geom(&s, Id::<Face>::from_index(0));
    let f = Frame::from_origin_normal(DVec3::new(0.0, 0.0, 3.0), DVec3::Z);
    let circle = Seg::new(
        Curve::Circle {
            frame: f,
            radius: 4.0,
        },
        0.0,
        TAU,
    );
    let r = split_face(&g, &[circle], &tol()).unwrap();
    assert_eq!(r.len(), 2);
    let a = sorted_areas(&g, &r);
    let lat = (3.0_f64 / 5.0).asin();
    close(a[0], TAU * (PI / 2.0 - lat), 0.02);
    close(a[1], TAU * (PI / 2.0 + lat), 0.02);
}

#[test]
fn a_small_closed_curve_on_a_sphere_makes_a_hole_and_a_cap() {
    let s = k().make_sphere(5.0).unwrap();
    let g = face_geom(&s, Id::<Face>::from_index(0));
    // kleiner Kreis um (0, 5, 0): Ebene y = 4.5, Radius sqrt(25 - 20.25)
    let f = Frame::from_origin_normal(DVec3::new(0.0, 4.5, 0.0), DVec3::Y);
    let circle = Seg::new(
        Curve::Circle {
            frame: f,
            radius: (25.0f64 - 20.25).sqrt(),
        },
        0.0,
        TAU,
    );
    let r = split_face(&g, &[circle], &tol()).unwrap();
    assert_eq!(r.len(), 2);
    assert!(r.iter().any(|x| x.holes.len() == 1));
}

// --- Zusatztests aus dem Review (Ruling R5) ---

/// Ein Ring um den Zylinder, dessen Kurvenanfang nicht auf der Naht liegt: Der Schnitt muss an der
/// Naht geteilt werden, sonst entstünde ein in u nicht geschlossener Umlauf.
#[test]
fn a_cylinder_ring_starting_off_the_seam_is_split_at_the_seam() {
    let s = k().make_cylinder(5.0, 10.0).unwrap();
    let g = find_face(&s, |f| matches!(f.surface, Surface::Cylinder { .. }));
    let f = Frame {
        origin: DVec3::new(0.0, 0.0, 4.0),
        x: DVec3::Y,
        y: -DVec3::X,
        z: DVec3::Z,
    };
    let circle = Seg::new(
        Curve::Circle {
            frame: f,
            radius: 5.0,
        },
        0.0,
        TAU,
    );
    let r = split_face(&g, &[circle], &tol()).unwrap();
    assert_eq!(r.len(), 2);
    let a = sorted_areas(&g, &r);
    close(a[0], TAU * 4.0, 0.01);
    close(a[1], TAU * 6.0, 0.01);
    // jede Region behält beide Seiten der (geteilten) Naht
    for reg in &r {
        let seam = reg
            .outer
            .iter()
            .filter(|s| matches!(s.curve, Curve::Line { .. }))
            .count();
        assert_eq!(seam, 2, "Nahtstücke der Region");
    }
}

/// Großkreis durch beide Pole: An den Polen entstehen Knoten, die Naht liegt im Inneren einer
/// Halbkugel. Ergebnis: zwei Halbkugeln mit je π² Fläche im Parameterraum.
#[test]
fn a_great_circle_through_both_poles_splits_the_sphere_in_two() {
    let s = k().make_sphere(5.0).unwrap();
    let g = face_geom(&s, Id::<Face>::from_index(0));
    let f = Frame::from_origin_normal(DVec3::ZERO, DVec3::X);
    let circle = Seg::new(
        Curve::Circle {
            frame: f,
            radius: 5.0,
        },
        0.0,
        TAU,
    );
    let r = split_face(&g, &[circle], &tol()).unwrap();
    assert_eq!(r.len(), 2);
    for a in sorted_areas(&g, &r) {
        close(a, PI * PI, 0.02);
    }
}

/// Ebene durch die Kegelachse: zwei Mantellinien von der Grundkante zur Spitze teilen den
/// Kegelmantel in zwei Hälften (Knoten an der Spitze).
#[test]
fn a_plane_through_the_cone_axis_splits_the_cone_in_two() {
    let s = k().make_cone(3.0, 6.0).unwrap();
    let g = find_face(&s, |f| matches!(f.surface, Surface::Cone { .. }));
    let apex = DVec3::new(0.0, 0.0, 6.0);
    let generator = |p: DVec3| line(p, apex - p, (apex - p).length());
    let secs = [
        generator(DVec3::new(0.0, 3.0, 0.0)),
        generator(DVec3::new(0.0, -3.0, 0.0)),
    ];
    let r = split_face(&g, &secs, &tol()).unwrap();
    assert_eq!(r.len(), 2);
    for a in sorted_areas(&g, &r) {
        close(a, PI * 6.0, 0.01);
    }
}

// --- Nachbesserung Runde 1: Schnitte auf der Naht, gegenläufige Flächen, Kreuzungen ---

fn cylinder_wall() -> FaceGeom {
    let s = k().make_cylinder(5.0, 10.0).unwrap();
    find_face(&s, |f| matches!(f.surface, Surface::Cylinder { .. }))
}

/// Ebene durch die Achse, die die Naht enthält: Die Mantellinie bei u = 0 fällt mit der Naht
/// zusammen und ist ein echter Schnitt. Zwei Hälften mit je π·10.
#[test]
fn a_cut_on_the_cylinder_seam_is_a_real_cut() {
    let g = cylinder_wall();
    let secs = [
        line(DVec3::new(5.0, 0.0, 0.0), DVec3::Z, 10.0),
        line(DVec3::new(-5.0, 0.0, 0.0), DVec3::Z, 10.0),
    ];
    let r = split_face(&g, &secs, &tol()).unwrap();
    assert_eq!(r.len(), 2);
    for a in sorted_areas(&g, &r) {
        close(a, PI * 10.0, 0.01);
    }
}

#[test]
fn a_cut_on_the_cone_seam_is_a_real_cut() {
    let s = k().make_cone(3.0, 6.0).unwrap();
    let g = find_face(&s, |f| matches!(f.surface, Surface::Cone { .. }));
    let apex = DVec3::new(0.0, 0.0, 6.0);
    let generator = |p: DVec3| line(p, apex - p, (apex - p).length());
    let secs = [
        generator(DVec3::new(3.0, 0.0, 0.0)),
        generator(DVec3::new(-3.0, 0.0, 0.0)),
    ];
    let r = split_face(&g, &secs, &tol()).unwrap();
    assert_eq!(r.len(), 2);
    for a in sorted_areas(&g, &r) {
        close(a, PI * 6.0, 0.01);
    }
}

/// Großkreis in der xz-Ebene: eine Hälfte liegt auf der Nahtmeridiane.
#[test]
fn a_great_circle_containing_the_sphere_seam_splits_the_sphere_in_two() {
    let s = k().make_sphere(5.0).unwrap();
    let g = face_geom(&s, Id::<Face>::from_index(0));
    let f = Frame::from_origin_normal(DVec3::ZERO, DVec3::Y);
    let circle = Seg::new(
        Curve::Circle {
            frame: f,
            radius: 5.0,
        },
        0.0,
        TAU,
    );
    let r = split_face(&g, &[circle], &tol()).unwrap();
    assert_eq!(r.len(), 2);
    for a in sorted_areas(&g, &r) {
        close(a, PI * PI, 0.02);
    }
}

/// Mantel einer Zylinderbohrung (`same_sense == false`), geteilt durch einen Kreis und eine am
/// Kreis vorgeteilte Mantellinie: positive Flächen 2π·3·2 bzw. 2π·3·3 im Parameterraum (u-Breite 2π).
#[test]
fn a_hole_wall_with_opposite_sense_is_split_with_positive_areas() {
    let profile = Profile {
        outer: ProfileLoop::rectangle(DVec2::ZERO, DVec2::new(20.0, 20.0)),
        holes: vec![ProfileLoop::circle(DVec2::new(10.0, 10.0), 3.0)],
    };
    let s = k().extrude(&profile, &Frame::WORLD, 5.0).unwrap();
    let g = find_face(&s, |f| matches!(f.surface, Surface::Cylinder { .. }));
    assert!(!g.same_sense);
    let f = Frame::from_origin_normal(DVec3::new(10.0, 10.0, 2.0), DVec3::Z);
    let secs = [
        Seg::new(
            Curve::Circle {
                frame: f,
                radius: 3.0,
            },
            0.0,
            TAU,
        ),
        line(DVec3::new(7.0, 10.0, 0.0), DVec3::Z, 2.0),
        line(DVec3::new(7.0, 10.0, 2.0), DVec3::Z, 3.0),
    ];
    let r = split_face(&g, &secs, &tol()).unwrap();
    assert_eq!(r.len(), 2);
    let a = sorted_areas(&g, &r);
    assert!(a.iter().all(|x| *x > 0.0));
    close(a[0], TAU * 2.0, 0.01);
    close(a[1], TAU * 3.0, 0.01);
}

/// Kreis bei z = 4 plus Mantellinie bei u = π (am Kreis vorgeteilt): Jedes Band wird über die Naht
/// zu einem Stück zusammengeführt; die Schleife enthält die Mantellinie zweimal, kein Nahtstück.
#[test]
fn bands_cut_by_an_axial_line_are_merged_across_the_seam() {
    let g = cylinder_wall();
    let f = Frame::from_origin_normal(DVec3::new(0.0, 0.0, 4.0), DVec3::Z);
    let secs = [
        Seg::new(
            Curve::Circle {
                frame: f,
                radius: 5.0,
            },
            0.0,
            TAU,
        ),
        line(DVec3::new(-5.0, 0.0, 0.0), DVec3::Z, 4.0),
        line(DVec3::new(-5.0, 0.0, 4.0), DVec3::Z, 6.0),
    ];
    let r = split_face(&g, &secs, &tol()).unwrap();
    assert_eq!(r.len(), 2);
    let a = sorted_areas(&g, &r);
    close(a[0], TAU * 4.0, 0.01);
    close(a[1], TAU * 6.0, 0.01);
    for reg in &r {
        assert!(reg.holes.is_empty());
        let lines: Vec<DVec3> = reg
            .outer
            .iter()
            .filter(|s| matches!(s.curve, Curve::Line { .. }))
            .map(|s| s.at(0.5))
            .collect();
        assert_eq!(
            lines.iter().filter(|p| p.x < -4.0).count(),
            2,
            "Mantellinie zweimal"
        );
        assert_eq!(
            lines.iter().filter(|p| p.x > 4.0).count(),
            0,
            "kein Nahtstück"
        );
        let poly = points_to_uv(&g.surface, g.same_sense, &loop_points(&reg.outer));
        let (first, last) = (poly[0], *poly.last().unwrap());
        assert!((first.x - last.x).abs() < 0.2, "Polygon geschlossen");
    }
}

/// Ein Schnitt auf einem gewöhnlichen Randstück einer ebenen Fläche entfällt wie bisher.
#[test]
fn a_section_on_an_ordinary_boundary_edge_is_dropped() {
    let s = k().make_box(DVec3::new(10.0, 10.0, 10.0)).unwrap();
    let g = top_face(&s, 10.0);
    let on_edge = line(DVec3::new(0.0, 0.0, 10.0), DVec3::X, 10.0);
    let r = split_face(&g, std::slice::from_ref(&on_edge), &tol()).unwrap();
    assert_eq!(r.len(), 1);
    close(region_area(&g, &r[0]), 100.0, 1e-9);
    let cut = line(DVec3::new(4.0, 0.0, 10.0), DVec3::Y, 10.0);
    let r = split_face(&g, &[on_edge, cut], &tol()).unwrap();
    assert_eq!(r.len(), 2);
    let a = sorted_areas(&g, &r);
    close(a[0], 40.0, 1e-6);
    close(a[1], 60.0, 1e-6);
}

/// Schnitte, die sich im Inneren kreuzen (nicht vorgeteilt), sind ein Fehler.
#[test]
fn sections_crossing_in_their_interiors_are_an_error() {
    let s = k().make_box(DVec3::new(10.0, 10.0, 10.0)).unwrap();
    let g = top_face(&s, 10.0);
    let secs = [
        line(DVec3::new(5.0, 0.0, 10.0), DVec3::Y, 10.0),
        line(DVec3::new(0.0, 5.0, 10.0), DVec3::X, 10.0),
    ];
    assert!(split_face(&g, &secs, &tol()).is_err());
}
