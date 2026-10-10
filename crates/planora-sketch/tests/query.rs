use planora_math::{DVec2, Frame, Tolerance};
use planora_sketch::model::Sketch;

fn sketch() -> Sketch {
    Sketch::new(Frame::WORLD, "S", Tolerance::default())
}

#[test]
fn polylines_hit_the_exact_end_points() {
    let mut s = sketch();
    let c = s.add_point(0.0, 0.0).unwrap();
    let a = s.add_point(2.0, 0.0).unwrap();
    let b = s.add_point(0.0, 2.0).unwrap();
    let arc = s.add_arc(c, a, b).unwrap();
    let pl = s.curve_polyline(arc, 64).unwrap();
    assert_eq!(*pl.first().unwrap(), DVec2::new(2.0, 0.0));
    assert!((*pl.last().unwrap() - DVec2::new(0.0, 2.0)).length() < 1e-12);
    assert!(pl.iter().all(|p| (p.length() - 2.0).abs() < 1e-9));
    assert!(pl.len() >= 16);
    let circle = s.add_circle(c, 1.0).unwrap();
    let cl = s.curve_polyline(circle, 32).unwrap();
    assert!(
        (cl[0] - *cl.last().unwrap()).length() < 1e-12,
        "Kreis ist geschlossen"
    );
    let line = s.add_line(a, b).unwrap();
    assert_eq!(s.curve_polyline(line, 64).unwrap().len(), 2);
    assert!(
        s.curve_polyline(planora_kernel::Id::from_index(77), 8)
            .is_none()
    );
}

#[test]
fn picking_prefers_points_and_respects_the_radius() {
    let mut s = sketch();
    let a = s.add_point(0.0, 0.0).unwrap();
    let b = s.add_point(10.0, 0.0).unwrap();
    let l = s.add_line(a, b).unwrap();
    assert_eq!(s.pick_point(DVec2::new(0.3, 0.2), 0.5), Some(a));
    assert_eq!(s.pick_point(DVec2::new(5.0, 0.0), 0.5), None);
    assert_eq!(s.pick_curve(DVec2::new(5.0, 0.4), 0.5), Some(l));
    assert_eq!(s.pick_curve(DVec2::new(5.0, 2.0), 0.5), None);
    assert!((s.distance_to_curve(l, DVec2::new(5.0, 3.0)).unwrap() - 3.0).abs() < 1e-12);
    let (lo, hi) = s.bounds().unwrap();
    assert_eq!((lo, hi), (DVec2::ZERO, DVec2::new(10.0, 0.0)));
    assert!(sketch().bounds().is_none());
}

#[test]
fn extent_includes_curves() {
    let mut s = sketch();
    let c = s.add_point(1.0, 2.0).unwrap();
    s.add_circle(c, 5.0).unwrap();
    assert_eq!(
        s.bounds().unwrap(),
        (DVec2::new(1.0, 2.0), DVec2::new(1.0, 2.0))
    );
    let (lo, hi) = s.extent().unwrap();
    assert!((lo - DVec2::new(-4.0, -3.0)).length() < 1e-9, "{lo:?}");
    assert!((hi - DVec2::new(6.0, 7.0)).length() < 1e-9, "{hi:?}");
    assert!(sketch().extent().is_none());
}
