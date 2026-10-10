use planora_kernel::Id;
use planora_math::{DVec2, Frame, Tolerance};
use planora_sketch::SketchError;
use planora_sketch::model::{CurveKind, Sketch};

fn sketch() -> Sketch {
    Sketch::new(Frame::WORLD, "S", Tolerance::default())
}

#[test]
fn points_and_lines_share_variables() {
    let mut s = sketch();
    let a = s.add_point(0.0, 0.0).unwrap();
    let b = s.add_point(3.0, 4.0).unwrap();
    let c = s.add_point(3.0, 0.0).unwrap();
    let l1 = s.add_line(a, b).unwrap();
    let l2 = s.add_line(b, c).unwrap();
    assert_eq!(s.variable_count(), 6);
    assert!((s.line_length(l1).unwrap() - 5.0).abs() < 1e-12);
    assert!((s.line_length(l2).unwrap() - 4.0).abs() < 1e-12);
    let CurveKind::Line { b: end1, .. } = s.curve(l1).unwrap().kind else {
        panic!("keine Linie")
    };
    let CurveKind::Line { a: start2, .. } = s.curve(l2).unwrap().kind else {
        panic!("keine Linie")
    };
    assert_eq!(end1, start2, "gemeinsamer Punkt");
    s.set_point_xy(b, 6.0, 8.0).unwrap();
    assert_eq!(s.point_xy(b).unwrap(), DVec2::new(6.0, 8.0));
    assert!((s.line_length(l1).unwrap() - 10.0).abs() < 1e-12);
    assert_eq!(s.points().count(), 3);
    assert_eq!(s.curves().count(), 2);
}

#[test]
fn circles_and_arcs_report_their_radius() {
    let mut s = sketch();
    let c = s.add_point(1.0, 1.0).unwrap();
    let circle = s.add_circle(c, 2.5).unwrap();
    assert_eq!(s.radius(circle), Some(2.5));
    let a = s.add_point(4.0, 1.0).unwrap();
    let b = s.add_point(1.0, 4.0).unwrap();
    let arc = s.add_arc(c, a, b).unwrap();
    assert!((s.radius(arc).unwrap() - 3.0).abs() < 1e-12);
    let line = s.add_line(c, a).unwrap();
    assert_eq!(s.radius(line), None);
}

#[test]
fn invalid_input_is_rejected_without_panic() {
    let mut s = sketch();
    assert!(matches!(
        s.add_point(f64::NAN, 0.0),
        Err(SketchError::InvalidInput(_))
    ));
    assert!(s.add_point(0.0, f64::INFINITY).is_err());
    let a = s.add_point(0.0, 0.0).unwrap();
    let b = s.add_point(0.0, 0.0).unwrap();
    assert!(matches!(s.add_line(a, a), Err(SketchError::Degenerate(_))));
    assert!(
        matches!(s.add_line(a, b), Err(SketchError::Degenerate(_))),
        "zusammenfallende Punkte"
    );
    assert!(s.add_circle(a, 0.0).is_err());
    assert!(s.add_circle(a, -1.0).is_err());
    assert!(s.add_circle(a, f64::NAN).is_err());
    let c = s.add_point(5.0, 0.0).unwrap();
    assert!(s.add_arc(a, a, c).is_err(), "Start = Mitte");
    assert!(s.add_arc(a, c, a).is_err(), "Ende = Mitte");
    assert!(s.set_point_xy(a, f64::NAN, 1.0).is_err());
    assert_eq!(s.point_xy(a).unwrap(), DVec2::ZERO, "unverändert");
}

#[test]
fn foreign_ids_give_unknown_object_not_a_panic() {
    let mut s = sketch();
    let a = s.add_point(0.0, 0.0).unwrap();
    let bogus = Id::from_index(99);
    assert_eq!(s.add_line(a, bogus), Err(SketchError::UnknownObject));
    assert_eq!(s.point_xy(bogus), Err(SketchError::UnknownObject));
    assert_eq!(
        s.set_point_xy(bogus, 1.0, 1.0),
        Err(SketchError::UnknownObject)
    );
    assert!(s.curve(Id::from_index(0)).is_none());
    assert_eq!(s.line_length(Id::from_index(3)), None);
}

#[test]
fn construction_flag_can_be_toggled() {
    let mut s = sketch();
    let a = s.add_point(0.0, 0.0).unwrap();
    let b = s.add_point(1.0, 0.0).unwrap();
    let l = s.add_line(a, b).unwrap();
    assert!(!s.curve(l).unwrap().construction);
    s.set_construction(l, true).unwrap();
    assert!(s.curve(l).unwrap().construction);
}
