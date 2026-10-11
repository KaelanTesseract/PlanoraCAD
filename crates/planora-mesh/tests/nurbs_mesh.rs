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
