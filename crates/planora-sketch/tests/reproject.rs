use planora_kernel::{AnalyticKernel, Id, Kernel};
use planora_math::{DVec3, Frame, Tolerance};
use planora_sketch::SketchError;
use planora_sketch::model::{CurveKind, Sketch};
use planora_sketch::solver::SolveStatus;

fn k() -> AnalyticKernel {
    AnalyticKernel::default()
}

fn sketch() -> Sketch {
    Sketch::new(Frame::WORLD, "S", Tolerance::default())
}

#[test]
fn set_frame_replaces_the_plane_and_rejects_bad_frames() {
    let mut s = sketch();
    let rev = s.revision();
    let f = Frame::from_origin_normal(DVec3::new(0.0, 0.0, 5.0), DVec3::Z);
    s.set_frame(f).unwrap();
    assert_eq!(*s.frame(), f);
    assert_ne!(s.revision(), rev);
    let mut bad = f;
    bad.x = DVec3::new(f64::NAN, 0.0, 0.0);
    assert!(matches!(
        s.set_frame(bad),
        Err(SketchError::InvalidInput(_))
    ));
    assert_eq!(*s.frame(), f, "unverändert");
}

#[test]
fn a_projected_line_follows_when_the_body_changes() {
    let a = k().make_box(DVec3::new(10.0, 20.0, 5.0)).unwrap();
    let mut s = sketch();
    let mut copied = None;
    for (id, _) in a.edges.iter() {
        if let Ok(c) = s.add_projected_edge(&a, id)
            && (s.line_length(c).unwrap_or(0.0) - 10.0).abs() < 1e-9
        {
            copied = Some((c, id));
            break;
        }
    }
    let (curve, edge) = copied.expect("eine Kante der Länge 10");
    let b = k().make_box(DVec3::new(12.0, 20.0, 5.0)).unwrap();
    s.reproject_edge(curve, &b, edge).unwrap();
    assert!((s.line_length(curve).unwrap() - 12.0).abs() < 1e-9);
    let rep = s.solve();
    assert_eq!(rep.status, SolveStatus::Solved, "{rep:?}");
    assert!(
        (s.line_length(curve).unwrap() - 12.0).abs() < 1e-7,
        "die Fixwerte wurden mit aktualisiert"
    );
    assert!(s.curve(curve).unwrap().construction);
}

#[test]
fn a_projected_circle_follows_the_radius() {
    let a = k().make_cylinder(3.0, 8.0).unwrap();
    let mut s = sketch();
    let mut copied = None;
    for (id, _) in a.edges.iter() {
        if let Ok(c) = s.add_projected_edge(&a, id)
            && matches!(s.curve(c).unwrap().kind, CurveKind::Circle { .. })
        {
            copied = Some((c, id));
            break;
        }
    }
    let (curve, edge) = copied.expect("Kreis");
    let b = k().make_cylinder(5.0, 8.0).unwrap();
    s.reproject_edge(curve, &b, edge).unwrap();
    assert!((s.radius(curve).unwrap() - 5.0).abs() < 1e-9);
    assert_eq!(s.solve().status, SolveStatus::Solved);
    assert!((s.radius(curve).unwrap() - 5.0).abs() < 1e-7);
}

#[test]
fn a_change_of_kind_or_a_bad_id_is_an_error_and_leaves_the_sketch_alone() {
    let a = k().make_cylinder(3.0, 8.0).unwrap();
    let mut s = sketch();
    let mut line_curve = None;
    let mut circle_edge = None;
    for (id, _) in a.edges.iter() {
        if let Ok(c) = s.add_projected_edge(&a, id) {
            match s.curve(c).unwrap().kind {
                CurveKind::Circle { .. } => circle_edge = Some(id),
                _ => line_curve = Some(c),
            }
        }
    }
    // Der Zylinder hat eine senkrechte Naht (wird nicht kopiert) – also nur Kreise.
    let c0 = s.curves().next().unwrap().0;
    let before = s.variables().to_vec();
    let box_b = k().make_box(DVec3::splat(5.0)).unwrap();
    let line_edge = box_b
        .edges
        .iter()
        .find(|(_, e)| matches!(e.curve, planora_kernel::Curve::Line { .. }))
        .map(|(i, _)| i)
        .unwrap();
    let r = s.reproject_edge(c0, &box_b, line_edge);
    assert!(
        matches!(r, Err(SketchError::UnsupportedProjection(_))),
        "{r:?}"
    );
    assert_eq!(s.variables(), before.as_slice());
    assert!(matches!(
        s.reproject_edge(c0, &a, Id::from_index(999)),
        Err(SketchError::UnknownObject)
    ));
    assert!(matches!(
        s.reproject_edge(Id::from_index(999), &a, circle_edge.unwrap()),
        Err(SketchError::UnknownObject)
    ));
    let _ = line_curve;
}
