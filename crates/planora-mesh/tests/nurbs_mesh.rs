use planora_kernel::{AnalyticKernel, Kernel, Profile, ProfileLoop, Solid, Surface, UvRect};
use planora_math::nurbs::{NurbsCurve, NurbsSurface};
use planora_math::{DVec2, DVec3, Frame, PI, TAU};
use planora_mesh::{TessellationOptions, is_watertight, mass_properties, tessellate};

fn world() -> Frame {
    Frame::from_origin_normal(DVec3::ZERO, DVec3::Z)
}

fn opts(c: f64) -> TessellationOptions {
    TessellationOptions { chord_tolerance: c }
}

/// Zylinder, dessen Mantel eine NURBS-Drehfläche (u = Winkel, v = Kurvenparameter 0..1) ist.
fn nurbs_cylinder_solid(r: f64, h: f64) -> Solid {
    let kernel = AnalyticKernel::default();
    let profile = Profile {
        outer: ProfileLoop::circle(DVec2::ZERO, r),
        holes: vec![],
    };
    let mut solid = kernel.extrude(&profile, &world(), h).unwrap();
    let line = NurbsCurve::new(
        1,
        vec![0.0, 0.0, 1.0, 1.0],
        vec![DVec3::new(r, 0.0, 0.0), DVec3::new(r, 0.0, h)],
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

#[test]
fn the_nurbs_cylinder_has_the_analytic_volume_and_area() {
    let (r, h) = (3.0, 5.0);
    let solid = nurbs_cylinder_solid(r, h);
    let mesh = tessellate(&solid, &opts(0.01)).unwrap();
    let mp = mass_properties(&mesh);
    assert!(
        (mp.volume - PI * r * r * h).abs() / (PI * r * r * h) < 2e-3,
        "{}",
        mp.volume
    );
    let area = TAU * r * h + 2.0 * PI * r * r;
    assert!(
        (mp.surface_area - area).abs() / area < 2e-3,
        "{}",
        mp.surface_area
    );
    assert!(is_watertight(&mesh, 1e-9));
}

#[test]
fn a_finer_chord_tolerance_gives_more_triangles_and_a_smaller_error() {
    let solid = nurbs_cylinder_solid(3.0, 5.0);
    let exact = PI * 9.0 * 5.0;
    let mut last = (0usize, f64::MAX);
    for c in [0.1, 0.01, 0.001] {
        let mesh = tessellate(&solid, &opts(c)).unwrap();
        let err = (mass_properties(&mesh).volume - exact).abs();
        assert!(mesh.indices.len() >= last.0, "Dreieckszahl wächst");
        assert!(err <= last.1 + 1e-9, "Fehler sinkt: {err} nach {}", last.1);
        last = (mesh.indices.len(), err);
    }
}

#[test]
fn the_normals_of_a_nurbs_face_point_outward() {
    let solid = nurbs_cylinder_solid(3.0, 5.0);
    let mesh = tessellate(&solid, &opts(0.01)).unwrap();
    let range = mesh
        .face_ranges
        .iter()
        .find(|r| matches!(solid.faces[r.face].surface, Surface::Nurbs(_)))
        .unwrap();
    for t in mesh.indices
        [range.first_index as usize..(range.first_index + range.index_count) as usize]
        .chunks(3)
    {
        let p = mesh.positions[t[0] as usize];
        let n = (mesh.positions[t[1] as usize] - p).cross(mesh.positions[t[2] as usize] - p);
        assert!(
            n.dot(DVec3::new(p.x, p.y, 0.0)) > 0.0,
            "Dreieck zeigt nach innen"
        );
    }
}

#[test]
fn a_nurbs_sphere_with_two_poles_has_the_exact_volume_and_no_nan() {
    // Halbkreis von unten nach oben (Start π, Überstreichung −π), damit die Normale der Drehfläche nach außen zeigt.
    let half = NurbsCurve::elliptic_arc(
        DVec3::ZERO,
        DVec3::new(0.0, 0.0, 2.0),
        DVec3::new(2.0, 0.0, 0.0),
        PI,
        -PI,
    )
    .unwrap();
    let surf = Surface::nurbs(
        NurbsSurface::revolve(&half, DVec3::ZERO, DVec3::Z, 0.0, TAU).unwrap(),
        true,
        false,
    )
    .unwrap();
    // Körper aus einer einzigen Kugelfläche: der analytische Kugelkörper, die Fläche ersetzt (v-Bereich der Halbkreiskurve ist 0..1)
    let kernel = AnalyticKernel::default();
    let mut solid = kernel.make_sphere(2.0).unwrap();
    let id = solid.faces.iter().map(|(id, _)| id).next().unwrap();
    solid.faces[id].surface = surf;
    solid.faces[id].patch = Some(UvRect {
        u0: 0.0,
        u1: TAU,
        v0: 0.0,
        v1: 1.0,
    });
    let mesh = tessellate(&solid, &opts(0.005)).unwrap();
    assert!(
        mesh.normals
            .iter()
            .all(|n| n.is_finite() && (n.length() - 1.0).abs() < 1e-6)
    );
    let exact = 4.0 / 3.0 * PI * 8.0;
    assert!((mass_properties(&mesh).volume - exact).abs() / exact < 3e-3);
    assert!(is_watertight(&mesh, 1e-9));
}

/// Zylinder (r = 3, h = 5) geschnitten mit einem großen, um die x-Achse gekippten Quader: Der
/// Mantel wird oben und unten von schrägen Ebenen beschnitten (Ellipsenkanten). Danach wird der
/// Zylindermantel durch dieselbe Fläche als NURBS-Drehfläche ersetzt; Patch und Randschleifen
/// bleiben. Das Netz muss geschlossen sein und dasselbe Volumen haben wie die analytische Version.
#[test]
fn a_trimmed_nurbs_face_gives_a_closed_mesh_with_the_right_volume() {
    use planora_kernel::boolean::{BoolOp, boolean};
    use planora_math::{DMat4, Tolerance};
    let (r, h) = (3.0, 5.0);
    let kernel = AnalyticKernel::default();
    let profile = Profile {
        outer: ProfileLoop::circle(DVec2::ZERO, r),
        holes: vec![],
    };
    let cylinder = kernel.extrude(&profile, &world(), h).unwrap();
    // Quader 20 × 20 × 3, mittig um den Ursprung, um 15° um x gekippt, nach z = 2,5 geschoben.
    let block = kernel.make_box(DVec3::new(20.0, 20.0, 3.0)).unwrap();
    let m = DMat4::from_translation(DVec3::new(0.0, 0.0, 2.5))
        * DMat4::from_rotation_x(15f64.to_radians())
        * DMat4::from_translation(DVec3::new(-10.0, -10.0, -1.5));
    let block = kernel.transform(&block, &m).unwrap();
    let tol = Tolerance::default();
    let analytic = boolean(&cylinder, &block, BoolOp::Intersection, &tol).unwrap();
    planora_kernel::validate(&analytic).unwrap();

    let analytic_mesh = tessellate(&analytic, &opts(0.001)).unwrap();
    assert!(is_watertight(&analytic_mesh, 1e-6));
    let expected = mass_properties(&analytic_mesh).volume;
    // Plausibilität: Zylinder der Höhe 3/cos 15° (schräge Scheibe durch den ganzen Zylinder).
    let slab = PI * r * r * 3.0 / 15f64.to_radians().cos();
    assert!(
        (expected - slab).abs() / slab < 1e-3,
        "{expected} statt {slab}"
    );

    // Mantel ersetzen: Drehfläche einer Linie bei Radius r, länger als der Zylinder hoch ist.
    let line = NurbsCurve::new(
        1,
        vec![0.0, 0.0, 1.0, 1.0],
        vec![DVec3::new(r, 0.0, -1.0), DVec3::new(r, 0.0, h + 1.0)],
        vec![1.0; 2],
    )
    .unwrap();
    let nurbs = Surface::nurbs(
        NurbsSurface::revolve(&line, DVec3::ZERO, DVec3::Z, 0.0, TAU).unwrap(),
        true,
        false,
    )
    .unwrap();
    let mut solid = analytic.clone();
    let ids: Vec<_> = solid
        .faces
        .iter()
        .filter(|(_, f)| matches!(f.surface, Surface::Cylinder { .. }))
        .map(|(id, _)| id)
        .collect();
    assert_eq!(ids.len(), 1, "genau eine Zylinderfläche");
    solid.faces[ids[0]].surface = nurbs;
    planora_kernel::validate(&solid).unwrap();

    let mesh = tessellate(&solid, &opts(0.001)).unwrap();
    assert!(is_watertight(&mesh, 1e-6), "Netz nicht geschlossen");
    let v = mass_properties(&mesh).volume;
    assert!(
        (v - expected).abs() / expected < 1e-3,
        "{v} statt {expected}"
    );
    // Außennormalen auch auf der beschnittenen Freiformfläche.
    let range = mesh
        .face_ranges
        .iter()
        .find(|fr| fr.face == ids[0])
        .unwrap();
    assert!(range.index_count > 0);
    let first = range.first_index as usize;
    for t in mesh.indices[first..first + range.index_count as usize].chunks(3) {
        let p = mesh.positions[t[0] as usize];
        let n = (mesh.positions[t[1] as usize] - p).cross(mesh.positions[t[2] as usize] - p);
        assert!(
            n.dot(DVec3::new(p.x, p.y, 0.0)) >= 0.0,
            "Dreieck zeigt nach innen"
        );
    }
}

/// NURBS-Zylinder, dessen Kreiskanten zusätzlich durch NURBS-Kreise (Parameter 0..1) ersetzt
/// sind (gleiche Lage und Richtung wie die analytischen Kanten).
fn nurbs_cylinder_with_nurbs_edges(r: f64, h: f64) -> Solid {
    use planora_kernel::Curve;
    let mut solid = nurbs_cylinder_solid(r, h);
    let ids: Vec<_> = solid
        .edges
        .iter()
        .filter(|(_, e)| matches!(e.curve, Curve::Circle { .. }))
        .map(|(id, _)| id)
        .collect();
    assert_eq!(ids.len(), 2);
    for id in ids {
        let Curve::Circle { frame, radius } = solid.edges[id].curve.clone() else {
            unreachable!()
        };
        let c =
            NurbsCurve::elliptic_arc(frame.origin, frame.x * radius, frame.y * radius, 0.0, TAU)
                .unwrap();
        let (a, b) = c.domain();
        let e = &mut solid.edges[id];
        e.curve = Curve::nurbs(c);
        (e.t0, e.t1) = (a, b);
    }
    solid
}

#[test]
fn nurbs_edges_are_divided_by_their_shape_not_by_their_parameter_span() {
    let (r, h) = (3.0, 5.0);
    let solid = nurbs_cylinder_with_nurbs_edges(r, h);
    planora_kernel::validate(&solid).unwrap();
    let tol = 0.01;
    let mesh = tessellate(&solid, &opts(tol)).unwrap();
    assert!(is_watertight(&mesh, 1e-9));
    let exact = PI * r * r * h;
    let v = mass_properties(&mesh).volume;
    assert!((v - exact).abs() / exact < 2e-3, "{v}");
    for line in &mesh.edge_lines {
        if !matches!(
            solid.edges[line.edge].curve,
            planora_kernel::Curve::Nurbs(_)
        ) {
            continue;
        }
        // Parameterbereich 0..1: früher nur etwa 12 Punkte, jetzt nach Krümmung.
        assert!(line.points.len() > 40, "{} Punkte", line.points.len());
        for w in line.points.windows(2) {
            // Sehnenmitte gegen den Kreis (Radius r um die z-Achse).
            let m = 0.5 * (w[0] + w[1]);
            let sag = r - DVec3::new(m.x, m.y, 0.0).length();
            assert!(sag <= tol, "Sehnenabweichung {sag}");
        }
    }
}

#[test]
fn a_closed_nurbs_mantle_with_period_one_and_a_shifted_seam_meshes_closed() {
    // Extrudierter NURBS-Kreis: u-Bereich 0..1 (Periode 1), Naht der Fläche bei (3, 0, z),
    // während die Naht-Kante des Körpers bei (0, 3, z) liegt.
    let (r, h) = (3.0, 5.0);
    let mut solid = nurbs_cylinder_solid(r, h);
    let circle =
        NurbsCurve::elliptic_arc(DVec3::ZERO, DVec3::X * r, DVec3::Y * r, 0.0, TAU).unwrap();
    let surface = Surface::nurbs(
        NurbsSurface::extrude(&circle, DVec3::Z * h).unwrap(),
        true,
        false,
    )
    .unwrap();
    let id = solid
        .faces
        .iter()
        .find(|(_, f)| matches!(f.surface, Surface::Nurbs(_)))
        .map(|(id, _)| id)
        .unwrap();
    solid.faces[id].surface = surface;
    solid.faces[id].patch = Some(UvRect {
        u0: 0.0,
        u1: 1.0,
        v0: 0.0,
        v1: 1.0,
    });
    planora_kernel::validate(&solid).unwrap();
    let mesh = tessellate(&solid, &opts(0.01)).unwrap();
    assert!(is_watertight(&mesh, 1e-9));
    let exact = PI * r * r * h;
    let mp = mass_properties(&mesh);
    assert!((mp.volume - exact).abs() / exact < 2e-3, "{}", mp.volume);
    let area = TAU * r * h + 2.0 * PI * r * r;
    assert!(
        (mp.surface_area - area).abs() / area < 2e-3,
        "{}",
        mp.surface_area
    );
}

#[test]
fn the_nurbs_grid_follows_bezier_pieces_curvature_and_chord_tolerance() {
    use planora_mesh::{Discretizer, MAX_NURBS_DIVISIONS};
    let solid = nurbs_cylinder_solid(3.0, 5.0);
    let Surface::Nurbs(d) = &solid
        .faces
        .iter()
        .find(|(_, f)| matches!(f.surface, Surface::Nurbs(_)))
        .unwrap()
        .1
        .surface
    else {
        unreachable!()
    };
    let s = d.surface();
    let full = UvRect {
        u0: 0.0,
        u1: TAU,
        v0: 0.0,
        v1: 1.0,
    };
    let grid = |tol: f64| Discretizer::new(&solid, tol).nurbs_grid(s, &full);
    let (nu, nv) = grid(0.01);
    // 4 Bézier-Abschnitte vom Grad 2 in u, ein Abschnitt vom Grad 1 in v (mindestens 2).
    assert!(nu >= 8 && nv == 2, "{nu} × {nv}");
    // Sehnen des Rasters in u höchstens um die Toleranz von der Fläche entfernt.
    for i in 0..nu {
        let (a, b) = (TAU * i as f64 / nu as f64, TAU * (i + 1) as f64 / nu as f64);
        let m = 0.5 * (s.point(a, 0.5) + s.point(b, 0.5));
        let sag = 3.0 - DVec3::new(m.x, m.y, 0.0).length();
        assert!(sag <= 0.01, "Sehne {i}: {sag}");
    }
    let (fine, _) = grid(0.0001);
    assert!(fine > nu, "{fine} nach {nu}");
    // Grenzen: winzige Toleranz höchstens MAX_NURBS_DIVISIONS, unbrauchbares Rechteck = ganzer Bereich.
    let (cap_u, cap_v) = grid(1e-12);
    assert!(cap_u <= MAX_NURBS_DIVISIONS && cap_v <= MAX_NURBS_DIVISIONS);
    let nan = UvRect {
        u0: f64::NAN,
        u1: f64::INFINITY,
        v0: 2.0,
        v1: 2.0,
    };
    assert_eq!(Discretizer::new(&solid, 0.01).nurbs_grid(s, &nan), (nu, nv));
    // Halber Bereich in u: etwa halb so viele Abschnitte.
    let half = UvRect {
        u0: 0.0,
        u1: PI,
        v0: 0.0,
        v1: 1.0,
    };
    let (hu, hv) = Discretizer::new(&solid, 0.01).nurbs_grid(s, &half);
    assert!(hu >= 4 && hu < nu && hv == 2, "{hu} × {hv}");
}

/// Ersetzt die einzige Zylinderfläche (Achse z, Radius `r`) durch die NURBS-Drehfläche einer
/// Linie bei Radius `r` von `z0` bis `z1`; Patch und Randschleifen bleiben.
fn with_nurbs_mantle(
    analytic: &Solid,
    r: f64,
    z0: f64,
    z1: f64,
) -> (Solid, planora_kernel::Id<planora_kernel::Face>) {
    let line = NurbsCurve::new(
        1,
        vec![0.0, 0.0, 1.0, 1.0],
        vec![DVec3::new(r, 0.0, z0), DVec3::new(r, 0.0, z1)],
        vec![1.0; 2],
    )
    .unwrap();
    let nurbs = Surface::nurbs(
        NurbsSurface::revolve(&line, DVec3::ZERO, DVec3::Z, 0.0, TAU).unwrap(),
        true,
        false,
    )
    .unwrap();
    let mut solid = analytic.clone();
    let ids: Vec<_> = solid
        .faces
        .iter()
        .filter(|(_, f)| matches!(f.surface, Surface::Cylinder { .. }))
        .map(|(id, _)| id)
        .collect();
    assert_eq!(ids.len(), 1, "genau eine Zylinderfläche");
    solid.faces[ids[0]].surface = nurbs;
    (solid, ids[0])
}

/// Größte Abweichung der Dreiecke einer Fläche vom Zylinder (Achse z, Radius `r`), baryzentrisch
/// abgetastet (Ecken, Kantenpunkte und Innenpunkte).
fn mantle_deviation(
    mesh: &planora_mesh::Mesh,
    face: planora_kernel::Id<planora_kernel::Face>,
    r: f64,
) -> f64 {
    face_deviation(mesh, face, |p| {
        (DVec3::new(p.x, p.y, 0.0).length() - r).abs()
    })
}

/// Größte Abweichung der Dreiecke einer Fläche, gemessen mit dem Abstand `dist` zur exakten
/// Fläche, baryzentrisch abgetastet.
fn face_deviation(
    mesh: &planora_mesh::Mesh,
    face: planora_kernel::Id<planora_kernel::Face>,
    dist: impl Fn(DVec3) -> f64,
) -> f64 {
    const N: usize = 8;
    let range = mesh.face_ranges.iter().find(|fr| fr.face == face).unwrap();
    let first = range.first_index as usize;
    let mut worst = 0.0_f64;
    for t in mesh.indices[first..first + range.index_count as usize].chunks(3) {
        let [a, b, c] = [0, 1, 2].map(|k| mesh.positions[t[k] as usize]);
        for i in 0..=N {
            for j in 0..=N - i {
                let (wa, wb) = (i as f64 / N as f64, j as f64 / N as f64);
                let p = a * wa + b * wb + c * (1.0 - wa - wb);
                worst = worst.max(dist(p));
            }
        }
    }
    worst
}

/// Prüft den Körper und seine NURBS-Version: beide gültig, Netz dicht, Volumen wie beim fein
/// vernetzten analytischen Körper,
/// Abweichung des NURBS-Mantels höchstens die Sehnentoleranz (plus 5 % für Rundung/Abtastung).
fn check_trimmed_mantle(analytic: &Solid, r: f64, h: f64, tol: f64) {
    planora_kernel::validate(analytic).unwrap();
    // Bezugsvolumen: analytischer Körper, sehr fein vernetzt.
    let expected = mass_properties(&tessellate(analytic, &opts(3e-4)).unwrap()).volume;
    // Volumenfehler wächst mit der Sehnentoleranz (Kreiskanten der Deckel und Löcher).
    let limit = if tol <= 0.001 { 1e-3 } else { 3e-3 };
    let (solid, face) = with_nurbs_mantle(analytic, r, -1.0, h + 1.0);
    planora_kernel::validate(&solid).unwrap();
    let mesh = tessellate(&solid, &opts(tol)).unwrap();
    assert!(
        is_watertight(&mesh, 1e-6),
        "Netz nicht geschlossen (tol {tol})"
    );
    let v = mass_properties(&mesh).volume;
    assert!(
        (v - expected).abs() / expected < limit,
        "{v} statt {expected}"
    );
    let dev = mantle_deviation(&mesh, face, r);
    assert!(dev <= tol * 1.05, "Abweichung {dev} bei Toleranz {tol}");
    assert!(mesh.inexact_faces.is_empty(), "Toleranz überall erreicht");
}

fn cylinder_r3_h5() -> Solid {
    let kernel = AnalyticKernel::default();
    let profile = Profile {
        outer: ProfileLoop::circle(DVec2::ZERO, 3.0),
        holes: vec![],
    };
    kernel.extrude(&profile, &world(), 5.0).unwrap()
}

#[test]
fn trimmed_nurbs_faces_keep_the_chord_tolerance() {
    use planora_kernel::boolean::{BoolOp, boolean};
    use planora_math::{DMat4, Tolerance};
    let kernel = AnalyticKernel::default();
    let tol = Tolerance::default();
    let cylinder = cylinder_r3_h5();
    // Quader 10 × 1 × 1 entlang x durch den Mantel (zwei rechteckige Löcher).
    let bar = kernel
        .transform(
            &kernel.make_box(DVec3::new(10.0, 1.0, 1.0)).unwrap(),
            &DMat4::from_translation(DVec3::new(-5.0, -0.5, 2.0)),
        )
        .unwrap();
    let holed = boolean(&cylinder, &bar, BoolOp::Difference, &tol).unwrap();
    for c in [0.01, 0.001] {
        check_trimmed_mantle(&holed, 3.0, 5.0, c);
    }
    // Derselbe Stab entlang y (Löcher bei 90° und 270°, eines liegt auf der Naht der Kante).
    let bar_y = kernel
        .transform(
            &kernel.make_box(DVec3::new(1.0, 10.0, 1.0)).unwrap(),
            &DMat4::from_translation(DVec3::new(-0.5, -5.0, 2.0)),
        )
        .unwrap();
    let holed_y = boolean(&cylinder, &bar_y, BoolOp::Difference, &tol).unwrap();
    for c in [0.01, 0.001] {
        check_trimmed_mantle(&holed_y, 3.0, 5.0, c);
    }
    // Schräg gekippte Scheibe (15° um x): Mantel oben und unten schräg beschnitten.
    let slab = kernel
        .transform(
            &kernel.make_box(DVec3::new(20.0, 20.0, 3.0)).unwrap(),
            &(DMat4::from_translation(DVec3::new(0.0, 0.0, 2.5))
                * DMat4::from_rotation_x(15f64.to_radians())
                * DMat4::from_translation(DVec3::new(-10.0, -10.0, -1.5))),
        )
        .unwrap();
    let cut = boolean(&cylinder, &slab, BoolOp::Intersection, &tol).unwrap();
    check_trimmed_mantle(&cut, 3.0, 5.0, 0.001);
    // Diagonaler Stab (45° um z, 20° um x): schräge Löcher im Mantel.
    let diagonal = kernel
        .transform(
            &kernel.make_box(DVec3::new(10.0, 1.5, 1.5)).unwrap(),
            &(DMat4::from_translation(DVec3::new(0.0, 0.0, 2.5))
                * DMat4::from_rotation_x(20f64.to_radians())
                * DMat4::from_rotation_z(45f64.to_radians())
                * DMat4::from_translation(DVec3::new(-5.0, -0.75, -0.75))),
        )
        .unwrap();
    let crossed = boolean(&cylinder, &diagonal, BoolOp::Difference, &tol).unwrap();
    for c in [0.01, 0.003, 0.001] {
        check_trimmed_mantle(&crossed, 3.0, 5.0, c);
    }
}

/// NURBS-Mantel (r = 3, h = 5) aus einem extrudierten Kreis mit dem u-Bereich
/// `offset … offset + width`.
fn shifted_mantle_solid(offset: f64, width: f64) -> Solid {
    let mut solid = nurbs_cylinder_solid(3.0, 5.0);
    let c =
        NurbsCurve::elliptic_arc(DVec3::ZERO, DVec3::Y * 3.0, DVec3::X * -3.0, 0.0, TAU).unwrap();
    let knots = c
        .knots()
        .knots()
        .iter()
        .map(|k| offset + width * k)
        .collect();
    let c = NurbsCurve::new(2, knots, c.control_points().to_vec(), c.weights().to_vec()).unwrap();
    let s = Surface::nurbs(
        NurbsSurface::extrude(&c, DVec3::Z * 5.0).unwrap(),
        true,
        false,
    )
    .unwrap();
    let id = solid
        .faces
        .iter()
        .find(|(_, f)| matches!(f.surface, Surface::Nurbs(_)))
        .map(|(id, _)| id)
        .unwrap();
    solid.faces[id].surface = s;
    solid.faces[id].patch = Some(UvRect {
        u0: offset,
        u1: offset + width,
        v0: 0.0,
        v1: 1.0,
    });
    solid
}

#[test]
fn nurbs_faces_with_parameters_far_from_zero_mesh_closed() {
    for (offset, width) in [(1e5, 1.0), (-100.0, 0.01)] {
        let solid = shifted_mantle_solid(offset, width);
        planora_kernel::validate(&solid).unwrap();
        let mesh = tessellate(&solid, &opts(0.01)).unwrap();
        assert!(is_watertight(&mesh, 1e-9), "Bereich ab {offset}");
        let exact = PI * 9.0 * 5.0;
        let v = mass_properties(&mesh).volume;
        assert!((v - exact).abs() / exact < 2e-3, "{v}");
    }
}

#[test]
fn boundary_points_off_the_nurbs_face_are_an_error_not_an_open_mesh() {
    // Mantel mit Radius 3,1 an Kreiskanten mit Radius 3: Die Randpunkte liegen 0,1 mm neben der
    // Fläche, die Rückrechnung führt nicht auf sie zurück.
    let mut solid = nurbs_cylinder_solid(3.0, 5.0);
    let (wrong, _) = with_nurbs_mantle(&cylinder_r3_h5(), 3.1, 0.0, 5.0);
    let id = solid
        .faces
        .iter()
        .find(|(_, f)| matches!(f.surface, Surface::Nurbs(_)))
        .map(|(id, _)| id)
        .unwrap();
    let mantle = wrong
        .faces
        .iter()
        .find(|(_, f)| matches!(f.surface, Surface::Nurbs(_)))
        .unwrap()
        .1
        .surface
        .clone();
    solid.faces[id].surface = mantle;
    assert_eq!(
        tessellate(&solid, &opts(0.01)).unwrap_err(),
        planora_mesh::MeshError::OffSurface(id.index())
    );
}

#[test]
fn a_cut_nurbs_sphere_keeps_the_chord_tolerance() {
    use planora_kernel::boolean::{BoolOp, boolean};
    use planora_math::{DMat4, Tolerance};
    let kernel = AnalyticKernel::default();
    let sphere = kernel.make_sphere(2.0).unwrap();
    // Würfel schneidet eine schräge Kappe ab (gekippt um 25° um y).
    let cube = kernel
        .transform(
            &kernel.make_box(DVec3::new(6.0, 6.0, 6.0)).unwrap(),
            &(DMat4::from_rotation_y(25f64.to_radians())
                * DMat4::from_translation(DVec3::new(0.8, -3.0, -3.0))),
        )
        .unwrap();
    let cut = boolean(&sphere, &cube, BoolOp::Difference, &Tolerance::default()).unwrap();
    planora_kernel::validate(&cut).unwrap();
    let expected = mass_properties(&tessellate(&cut, &opts(3e-4)).unwrap()).volume;
    let half = NurbsCurve::elliptic_arc(
        DVec3::ZERO,
        DVec3::new(0.0, 0.0, 2.0),
        DVec3::new(2.0, 0.0, 0.0),
        PI,
        -PI,
    )
    .unwrap();
    let surf = Surface::nurbs(
        NurbsSurface::revolve(&half, DVec3::ZERO, DVec3::Z, 0.0, TAU).unwrap(),
        true,
        false,
    )
    .unwrap();
    let mut solid = cut.clone();
    let face = solid
        .faces
        .iter()
        .find(|(_, f)| matches!(f.surface, Surface::Sphere { .. }))
        .map(|(id, _)| id)
        .unwrap();
    solid.faces[face].surface = surf;
    planora_kernel::validate(&solid).unwrap();
    for tol in [0.01, 0.001] {
        let mesh = tessellate(&solid, &opts(tol)).unwrap();
        assert!(is_watertight(&mesh, 1e-6));
        let v = mass_properties(&mesh).volume;
        // Ein einbeschriebenes Netz mit Tiefe ≤ tol verliert höchstens etwa 3 · tol / r Volumen.
        let limit = 3.0 * tol / 2.0;
        assert!(
            (v - expected).abs() / expected < limit,
            "{v} statt {expected}"
        );
        let dev = face_deviation(&mesh, face, |p| (p.length() - 2.0).abs());
        assert!(dev <= tol * 1.05, "Abweichung {dev} bei Toleranz {tol}");
        assert!(mesh.inexact_faces.is_empty());
    }
}

#[test]
fn untrimmed_nurbs_tubes_keep_their_mesh_size() {
    // Obergrenzen: das Dreifache der Dreieckszahlen vor der isotropen Zellgrenze (Commit
    // 6739179: 1320, 426 und 1332 Dreiecke für den ganzen Körper).
    for (r, h, tol, before) in [
        (3.0, 5.0, 0.001, 1320),
        (1.0, 10.0, 0.01, 426),
        (1.0, 100.0, 0.01, 1332),
    ] {
        let solid = nurbs_cylinder_solid(r, h);
        let mesh = tessellate(&solid, &opts(tol)).unwrap();
        let tris = mesh.indices.len() / 3;
        assert!(
            tris <= 3 * before,
            "r {r}, h {h}, tol {tol}: {tris} Dreiecke"
        );
        assert!(is_watertight(&mesh, 1e-9));
        let face = solid
            .faces
            .iter()
            .find(|(_, f)| matches!(f.surface, Surface::Nurbs(_)))
            .map(|(id, _)| id)
            .unwrap();
        let dev = mantle_deviation(&mesh, face, r);
        assert!(dev <= tol * 1.05, "Abweichung {dev} bei Toleranz {tol}");
        assert!(mesh.inexact_faces.is_empty());
    }
}

#[test]
fn a_face_whose_boundary_is_too_coarse_is_reported_as_inexact() {
    use planora_kernel::boolean::{BoolOp, boolean};
    use planora_math::{DMat4, Tolerance};
    // Zylinder r = 50, h = 1 minus schräger Stab, Sehnentoleranz 1e-4: Die Kreiskanten sind auf
    // 720 Abschnitte begrenzt (Sehne ≈ 4,8e-4), der Mantel kann die Toleranz nicht überall halten.
    let kernel = AnalyticKernel::default();
    let profile = Profile {
        outer: ProfileLoop::circle(DVec2::ZERO, 50.0),
        holes: vec![],
    };
    let cylinder = kernel.extrude(&profile, &world(), 1.0).unwrap();
    let bar = kernel
        .transform(
            &kernel.make_box(DVec3::new(200.0, 5.0, 0.5)).unwrap(),
            &(DMat4::from_rotation_x(20f64.to_radians())
                * DMat4::from_rotation_z(30f64.to_radians())
                * DMat4::from_translation(DVec3::new(-100.0, -2.5, 0.2))),
        )
        .unwrap();
    let holed = boolean(&cylinder, &bar, BoolOp::Difference, &Tolerance::default()).unwrap();
    let (solid, face) = with_nurbs_mantle(&holed, 50.0, -1.0, 2.0);
    let tol = 1e-4;
    let mesh = tessellate(&solid, &opts(tol)).unwrap();
    assert!(is_watertight(&mesh, 1e-9), "geschlossen trotz Grenze");
    assert_eq!(
        mesh.inexact_faces,
        vec![face],
        "Fläche als ungenau gemeldet"
    );
    // Die Abweichung bleibt bei der des Randes (Sehne der Kreiskante), nicht darüber hinaus.
    let rim = 50.0 * (1.0 - (PI / 720.0).cos());
    let dev = mantle_deviation(&mesh, face, 50.0);
    assert!(
        dev > tol && dev <= rim * 1.05,
        "Abweichung {dev}, Rand {rim}"
    );
}
