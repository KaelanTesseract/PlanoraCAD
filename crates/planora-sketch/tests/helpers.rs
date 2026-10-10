use planora_kernel::{AnalyticKernel, Kernel};
use planora_math::{DVec2, DVec3, Frame, PI, Tolerance};
use planora_sketch::constraint::ConstraintKind as K;
use planora_sketch::model::{CurveKind, Sketch};
use planora_sketch::solver::{Drag, SolveStatus};
use planora_sketch::{SketchError, plane_of_face};

fn sketch() -> Sketch {
    Sketch::new(Frame::WORLD, "S", Tolerance::default())
}

#[test]
fn rectangle_stays_axis_parallel_after_solving_and_dragging_a_corner() {
    let mut s = sketch();
    let r = s
        .add_rectangle(DVec2::new(5.0, 2.0), DVec2::new(1.0, 7.0))
        .unwrap();
    assert_eq!(
        s.point_xy(r.points[0]).unwrap(),
        DVec2::new(1.0, 2.0),
        "unten links"
    );
    assert_eq!(
        s.point_xy(r.points[2]).unwrap(),
        DVec2::new(5.0, 7.0),
        "oben rechts"
    );
    assert_eq!(s.constraints().count(), 4);
    let rep = s.solve();
    assert_eq!(rep.status, SolveStatus::Solved);
    assert_eq!(rep.dof, 4, "x, y, Breite, Höhe");
    let rep = s
        .drag(&[planora_sketch::solver::Drag {
            point: r.points[2],
            target: DVec2::new(9.0, 10.0),
        }])
        .unwrap();
    assert_eq!(rep.status, SolveStatus::Solved);
    let (c0, c2, c1, c3) = (
        s.point_xy(r.points[0]).unwrap(),
        s.point_xy(r.points[2]).unwrap(),
        s.point_xy(r.points[1]).unwrap(),
        s.point_xy(r.points[3]).unwrap(),
    );
    assert!((c1.y - c0.y).abs() < 1e-7 && (c3.y - c2.y).abs() < 1e-7);
    assert!((c1.x - c2.x).abs() < 1e-7 && (c3.x - c0.x).abs() < 1e-7);
    assert!(s.add_rectangle(DVec2::ZERO, DVec2::new(0.0, 5.0)).is_err());
}

#[test]
fn mirrored_copy_is_symmetric_and_arcs_keep_their_shape() {
    let mut s = sketch();
    let axis_a = s.add_point(0.0, -10.0).unwrap();
    let axis_b = s.add_point(0.0, 10.0).unwrap();
    let axis = s.add_line(axis_a, axis_b).unwrap();
    s.add_constraint(K::Fixed(axis_a, 0.0, -10.0)).unwrap();
    s.add_constraint(K::Fixed(axis_b, 0.0, 10.0)).unwrap();
    let c = s.add_point(3.0, 0.0).unwrap();
    let a0 = s.add_point(5.0, 0.0).unwrap();
    let a1 = s.add_point(3.0, 2.0).unwrap();
    let arc = s.add_arc(c, a0, a1).unwrap();
    let circle_center = s.add_point(6.0, 5.0).unwrap();
    let circle = s.add_circle(circle_center, 1.0).unwrap();
    let copies = s.add_mirrored(&[arc, circle], axis).unwrap();
    assert_eq!(copies.len(), 2);
    let rep = s.solve();
    assert_eq!(rep.status, SolveStatus::Solved, "{rep:?}");
    let CurveKind::Arc { center, start, end } = s.curve(copies[0]).unwrap().kind else {
        panic!("Bogen erwartet")
    };
    assert!((s.point_xy(center).unwrap() - DVec2::new(-3.0, 0.0)).length() < 1e-7);
    // Spiegelung vertauscht Start und Ende: der gespiegelte Start ist das Bild von a1
    assert!((s.point_xy(start).unwrap() - DVec2::new(-3.0, 2.0)).length() < 1e-7);
    assert!((s.point_xy(end).unwrap() - DVec2::new(-5.0, 0.0)).length() < 1e-7);
    // dasselbe Gebiet: gespiegelter Bogen hat denselben Radius und Öffnungssinn gegen den Uhrzeigersinn
    assert!((s.radius(copies[0]).unwrap() - 2.0).abs() < 1e-7);
    let mirrored_circle = copies[1];
    assert!((s.radius(mirrored_circle).unwrap() - 1.0).abs() < 1e-7);
    // Achse nicht in der Liste
    assert!(s.add_mirrored(&[axis], axis).is_err());
}

#[test]
fn linear_pattern_places_translated_copies() {
    let mut s = sketch();
    let a = s.add_point(0.0, 0.0).unwrap();
    let b = s.add_point(2.0, 0.0).unwrap();
    let l = s.add_line(a, b).unwrap();
    let c = s.add_point(1.0, 3.0).unwrap();
    let circle = s.add_circle(c, 0.5).unwrap();
    s.add_constraint(K::Fixed(a, 0.0, 0.0)).unwrap();
    s.add_constraint(K::Fixed(b, 2.0, 0.0)).unwrap();
    s.add_constraint(K::Fixed(c, 1.0, 3.0)).unwrap();
    s.add_constraint(K::Radius(circle, 0.5)).unwrap();
    let copies = s
        .add_linear_pattern(&[l, circle], DVec2::new(5.0, 0.0), 2)
        .unwrap();
    assert_eq!(copies.len(), 2);
    assert_eq!(s.curves().count(), 2 + 4);
    let rep = s.solve();
    assert_eq!(rep.status, SolveStatus::Solved, "{rep:?}");
    for (k, group) in copies.iter().enumerate() {
        let CurveKind::Line { a: la, .. } = s.curve(group[0]).unwrap().kind else {
            panic!("Linie erwartet")
        };
        let moved = s.point_xy(la).unwrap();
        let want = DVec2::new(5.0 * (k as f64 + 1.0), 0.0);
        assert!(moved.distance(want) < 1e-6, "Kopie {k}: {moved:?}");
        assert!((s.line_length(group[0]).unwrap() - 2.0).abs() < 1e-7);
    }
    assert!(s.add_linear_pattern(&[l], DVec2::X, 0).is_err());
    assert!(s.add_linear_pattern(&[l], DVec2::X, 101).is_err());
}

#[test]
fn body_edges_are_copied_as_fixed_construction_geometry() {
    let k = AnalyticKernel::default();
    let bx = k.make_box(DVec3::new(10.0, 20.0, 5.0)).unwrap();
    let mut s = sketch();
    let mut copied = 0;
    for (id, _) in bx.edges.iter() {
        match s.add_projected_edge(&bx, id) {
            Ok(c) => {
                assert!(s.curve(c).unwrap().construction);
                copied += 1;
            }
            Err(SketchError::UnsupportedProjection(_)) => {}
            Err(e) => panic!("{e}"),
        }
    }
    // 8 Kanten liegen in z-Ebenen (4 unten, 4 oben, je projiziert deckungsgleich), die 4
    // senkrechten Kanten stehen senkrecht zur Skizzenebene und werden abgelehnt.
    assert_eq!(copied, 8);
    assert!(s.points().all(|(_, p)| p.construction));
    assert_eq!(s.solve().status, SolveStatus::Solved);
    assert!(
        s.profiles().profiles.is_empty(),
        "Konstruktionsgeometrie bildet keine Profile"
    );

    let cyl = k.make_cylinder(3.0, 8.0).unwrap();
    let mut s = sketch();
    let mut circles = 0;
    for (id, _) in cyl.edges.iter() {
        if let Ok(c) = s.add_projected_edge(&cyl, id)
            && matches!(s.curve(c).unwrap().kind, CurveKind::Circle { .. })
        {
            circles += 1;
            assert!((s.radius(c).unwrap() - 3.0).abs() < 1e-9);
        }
    }
    assert_eq!(circles, 2, "unterer und oberer Kreis");
}

#[test]
fn plane_of_a_face_points_outwards() {
    let k = AnalyticKernel::default();
    let bx = k.make_box(DVec3::new(10.0, 20.0, 5.0)).unwrap();
    let mut tops = 0;
    let mut bottoms = 0;
    for (id, _) in bx.faces.iter() {
        let f = plane_of_face(&bx, id).expect("alle Quaderflächen sind eben");
        assert!(f.is_orthonormal(1e-9));
        if (f.z - DVec3::Z).length() < 1e-9 {
            tops += 1;
            assert!((f.origin.z - 5.0).abs() < 1e-9);
        }
        if (f.z + DVec3::Z).length() < 1e-9 {
            bottoms += 1;
            assert!(f.origin.z.abs() < 1e-9);
        }
    }
    assert_eq!((tops, bottoms), (1, 1));
    let cyl = k.make_cylinder(3.0, 8.0).unwrap();
    let planar = cyl
        .faces
        .iter()
        .filter(|(id, _)| plane_of_face(&cyl, *id).is_some())
        .count();
    assert_eq!(planar, 2, "die Mantelfläche ist nicht eben");
    assert!(plane_of_face(&bx, planora_kernel::Id::from_index(99)).is_none());
}

// ---- Zusatztests (über den Plan hinaus) ---------------------------------------------------

#[test]
fn failing_helpers_leave_the_sketch_unchanged() {
    let mut s = sketch();
    let a = s.add_point(0.0, -1.0).unwrap();
    let b = s.add_point(0.0, 1.0).unwrap();
    let axis = s.add_line(a, b).unwrap();
    let c = s.add_point(2.0, 0.0).unwrap();
    let d = s.add_point(3.0, 0.0).unwrap();
    let l = s.add_line(c, d).unwrap();
    let before = (
        s.points().count(),
        s.curves().count(),
        s.constraints().count(),
        s.variable_count(),
    );
    let after = |s: &Sketch| {
        (
            s.points().count(),
            s.curves().count(),
            s.constraints().count(),
            s.variable_count(),
        )
    };
    // gültige Kurve vor der Achse: darf trotzdem nichts anlegen
    assert!(s.add_mirrored(&[l, axis], axis).is_err());
    assert_eq!(after(&s), before);
    assert!(matches!(
        s.add_mirrored(&[l], l),
        Err(SketchError::InvalidInput(_))
    ));
    assert!(s.add_mirrored(&[l, l], axis).is_err());
    assert!(s.add_linear_pattern(&[l], DVec2::ZERO, 3).is_err());
    assert!(
        s.add_rectangle(DVec2::ZERO, DVec2::new(f64::NAN, 1.0))
            .is_err()
    );
    assert_eq!(after(&s), before);
}

#[test]
fn axis_parallel_pattern_adds_horizontal_or_vertical() {
    let mut s = sketch();
    let a = s.add_point(0.0, 0.0).unwrap();
    let b = s.add_point(1.0, 1.0).unwrap();
    let l = s.add_line(a, b).unwrap();
    s.add_linear_pattern(&[l], DVec2::new(0.0, 4.0), 2).unwrap();
    let kinds: Vec<_> = s.constraints().map(|(_, c)| c.kind.clone()).collect();
    assert_eq!(
        kinds.iter().filter(|k| matches!(k, K::Vertical(_))).count(),
        2
    );
    assert_eq!(
        kinds
            .iter()
            .filter(|k| matches!(k, K::Horizontal(_)))
            .count(),
        0
    );
    assert!(kinds.contains(&K::Distance(a, planora_kernel::Id::from_index(4), 8.0)));
    let mut s = sketch();
    let a = s.add_point(0.0, 0.0).unwrap();
    let b = s.add_point(1.0, 1.0).unwrap();
    let l = s.add_line(a, b).unwrap();
    s.add_linear_pattern(&[l], DVec2::new(3.0, 4.0), 1).unwrap();
    assert!(
        s.constraints()
            .all(|(_, c)| !matches!(c.kind, K::Horizontal(_) | K::Vertical(_)))
    );
    assert_eq!(s.solve().status, SolveStatus::Solved);
}

#[test]
fn projected_arcs_keep_their_sense_and_tilted_circles_are_rejected() {
    use planora_kernel::{Curve, Solid};
    let mut solid = Solid::new();
    let v0 = solid.add_vertex(DVec3::new(2.0, 0.0, 3.0));
    let v1 = solid.add_vertex(DVec3::new(0.0, 2.0, 3.0));
    // Viertelkreis gegen den Uhrzeigersinn, aber mit nach unten zeigender Kreisnormale
    // und fallendem Parameter: Drehsinn in der Skizze bleibt gegen den Uhrzeigersinn.
    let down = Frame {
        origin: DVec3::new(0.0, 0.0, 3.0),
        x: DVec3::X,
        y: -DVec3::Y,
        z: -DVec3::Z,
    };
    let e_down = solid.add_edge(
        Curve::Circle {
            frame: down,
            radius: 2.0,
        },
        0.0,
        -std::f64::consts::FRAC_PI_2,
        v0,
        v1,
    );
    let up = Frame {
        origin: DVec3::new(0.0, 0.0, 3.0),
        ..Frame::WORLD
    };
    let e_up = solid.add_edge(
        Curve::Circle {
            frame: up,
            radius: 2.0,
        },
        std::f64::consts::FRAC_PI_2,
        0.0,
        v1,
        v0,
    );
    // Seit Task 9 werden ganze schräge Kreise und ganze Ellipsen zu Ellipsen; abgelehnt
    // werden nur noch Teilstücke davon.
    let tilted = Frame::from_origin_normal(DVec3::ZERO, DVec3::new(1.0, 0.0, 1.0));
    let e_tilted = solid.add_edge(
        Curve::Circle {
            frame: tilted,
            radius: 1.0,
        },
        0.0,
        PI,
        v0,
        v1,
    );
    let ellipse = solid.add_edge(
        Curve::Ellipse {
            frame: Frame::WORLD,
            major: 2.0,
            minor: 1.0,
        },
        0.0,
        PI,
        v0,
        v1,
    );
    let mut s = sketch();
    for e in [e_down, e_up] {
        let c = s.add_projected_edge(&solid, e).unwrap();
        let CurveKind::Arc { start, end, .. } = s.curve(c).unwrap().kind else {
            panic!("Bogen erwartet")
        };
        assert!((s.point_xy(start).unwrap() - DVec2::new(2.0, 0.0)).length() < 1e-9);
        assert!((s.point_xy(end).unwrap() - DVec2::new(0.0, 2.0)).length() < 1e-9);
    }
    let before = s.points().count();
    assert!(matches!(
        s.add_projected_edge(&solid, e_tilted),
        Err(SketchError::UnsupportedProjection(_))
    ));
    assert!(matches!(
        s.add_projected_edge(&solid, ellipse),
        Err(SketchError::UnsupportedProjection(_))
    ));
    assert!(matches!(
        s.add_projected_edge(&solid, planora_kernel::Id::from_index(99)),
        Err(SketchError::UnknownObject)
    ));
    assert_eq!(s.points().count(), before);
    let rep = s.solve();
    assert_eq!(rep.status, SolveStatus::Solved);
    assert!(
        rep.redundant.is_empty(),
        "projizierte Bögen sind nicht überbestimmt: {rep:?}"
    );
}

/// Muster eines Rechtecks (Abschlusswelle I4): keine überzähligen Abhängigkeiten, Freiheitsgrade
/// nur die des Originals, und die Kopien behalten beim Ziehen ihre Form.
#[test]
fn a_patterned_rectangle_has_no_redundant_constraints() {
    for n in [3usize, 10] {
        let mut s = Sketch::new(Frame::WORLD, "S", Tolerance::default());
        let r = s.add_rectangle(DVec2::ZERO, DVec2::new(10.0, 5.0)).unwrap();
        let copies = s
            .add_linear_pattern(&r.lines, DVec2::new(15.0, 0.0), n)
            .unwrap();
        let rep = s.solve();
        assert_eq!(rep.status, SolveStatus::Solved, "{rep:?}");
        assert!(rep.redundant.is_empty(), "n={n}: {:?}", rep.redundant);
        // Rechteck: 8 Variablen − 4 (waagerecht/senkrecht); Kopien sind voll bestimmt.
        assert_eq!(rep.dof, 4, "n={n}");

        let dragged = s
            .drag(&[Drag {
                point: r.points[2],
                target: DVec2::new(12.0, 7.0),
            }])
            .unwrap();
        assert_eq!(dragged.status, SolveStatus::Solved, "{dragged:?}");
        let corner = |c: planora_sketch::CurveId, end: usize| {
            let CurveKind::Line { a, b } = s.curve(c).unwrap().kind.clone() else {
                unreachable!()
            };
            s.point_xy(if end == 0 { a } else { b }).unwrap()
        };
        let orig: Vec<DVec2> = r.points.iter().map(|&p| s.point_xy(p).unwrap()).collect();
        assert!((orig[2] - orig[0]).length() > 12.0, "Zug wirkt: {orig:?}");
        for (k, copy) in copies.iter().enumerate() {
            let shift = DVec2::new(15.0 * (k + 1) as f64, 0.0);
            for (i, &line) in copy.iter().enumerate() {
                let want = orig[i] + shift;
                assert!(
                    (corner(line, 0) - want).length() < 1e-6,
                    "Kopie {k} Ecke {i}: {:?} statt {want:?}",
                    corner(line, 0)
                );
            }
        }
    }
}
