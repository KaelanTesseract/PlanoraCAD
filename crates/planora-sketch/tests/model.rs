use planora_kernel::Id;
use planora_math::{DVec2, Frame, Tolerance};
use planora_sketch::SketchError;
use planora_sketch::model::{CurveKind, LineStyle, Sketch};

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

// Skizzen 3: Mittellinie als dritter Linienstil.
#[test]
fn centerline_is_a_construction_line_with_its_own_style() {
    let mut s = sketch();
    let a = s.add_point(0.0, 0.0).unwrap();
    let b = s.add_point(1.0, 0.0).unwrap();
    let l = s.add_line(a, b).unwrap();
    assert_eq!(s.curve(l).unwrap().style(), LineStyle::Normal);
    assert!(!s.has_centerline_entry());
    s.set_centerline(l, true).unwrap();
    let c = s.curve(l).unwrap();
    assert!(c.construction && c.centerline);
    assert_eq!(c.style(), LineStyle::Centerline);
    assert!(s.has_centerline_entry());
    s.set_centerline(l, false).unwrap();
    let c = s.curve(l).unwrap();
    assert!(!c.construction && !c.centerline);
    assert_eq!(c.style(), LineStyle::Normal);
    s.set_centerline(l, true).unwrap();
    s.set_construction(l, true).unwrap();
    assert_eq!(s.curve(l).unwrap().style(), LineStyle::Construction);
    s.set_construction(l, false).unwrap();
    assert_eq!(s.curve(l).unwrap().style(), LineStyle::Normal);
}

#[test]
fn a_deleted_centerline_still_counts_as_an_entry() {
    let mut s = sketch();
    let a = s.add_point(0.0, 0.0).unwrap();
    let b = s.add_point(1.0, 0.0).unwrap();
    let l = s.add_line(a, b).unwrap();
    s.set_centerline(l, true).unwrap();
    s.remove_curve(l).unwrap();
    assert!(s.has_centerline_entry());
}

#[test]
fn a_centerline_needs_a_line() {
    let mut s = sketch();
    let m = s.add_point(0.0, 0.0).unwrap();
    let c = s.add_circle(m, 2.0).unwrap();
    assert!(matches!(
        s.set_centerline(c, true),
        Err(SketchError::WrongType(_))
    ));
    assert!(!s.has_centerline_entry());
    assert_eq!(s.curve(c).unwrap().style(), LineStyle::Normal);
}

#[test]
fn a_centerline_in_a_rectangle_splits_no_face_and_bounds_no_profile() {
    for legacy in [false, true] {
        let mut s = sketch();
        let r = s
            .add_rectangle(DVec2::new(0.0, 0.0), DVec2::new(4.0, 4.0))
            .unwrap();
        let diag = s.add_line(r.points[0], r.points[2]).unwrap();
        s.set_centerline(diag, true).unwrap();
        let e = if legacy {
            s.legacy_profiles()
        } else {
            s.profiles()
        };
        assert!(e.rejected.is_empty(), "{:?}", e.rejected);
        assert_eq!(e.profiles.len(), 1, "legacy = {legacy}");
    }
}

#[test]
fn a_centerline_only_cuts_when_construction_is_asked_for() {
    let mut s = sketch();
    let a = s.add_point(-5.0, 0.0).unwrap();
    let b = s.add_point(5.0, 0.0).unwrap();
    let h = s.add_line(a, b).unwrap();
    let c = s.add_point(0.0, -5.0).unwrap();
    let d = s.add_point(0.0, 5.0).unwrap();
    let v = s.add_line(c, d).unwrap();
    s.set_centerline(v, true).unwrap();
    assert!(s.split_params(h, false).is_empty());
    assert_eq!(s.split_params(h, true).len(), 1);
}

#[test]
fn mirroring_and_patterns_keep_the_line_style() {
    let mut s = sketch();
    let a = s.add_point(1.0, 1.0).unwrap();
    let b = s.add_point(3.0, 1.0).unwrap();
    let l = s.add_line(a, b).unwrap();
    let k = s.add_point(1.0, 4.0).unwrap();
    let m = s.add_point(3.0, 4.0).unwrap();
    let con = s.add_line(k, m).unwrap();
    s.set_centerline(l, true).unwrap();
    s.set_construction(con, true).unwrap();
    let p = s.add_point(0.0, -5.0).unwrap();
    let q = s.add_point(0.0, 5.0).unwrap();
    let axis = s.add_line(p, q).unwrap();
    let out = s.add_mirrored(&[l, con], axis).unwrap();
    assert_eq!(s.curve(out[0]).unwrap().style(), LineStyle::Centerline);
    assert_eq!(s.curve(out[1]).unwrap().style(), LineStyle::Construction);
    let copies = s
        .add_linear_pattern(&[l], DVec2::new(0.0, -2.0), 2)
        .unwrap();
    for c in copies.iter().flatten() {
        assert_eq!(s.curve(*c).unwrap().style(), LineStyle::Centerline);
    }
}
