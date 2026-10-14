use planora_drawing::{basis::Basis, layout::*, model::*};
use planora_math::DVec3;

#[test]
fn bases_are_orthonormal_right_handed() {
    for k in ViewKind::ALL {
        let b = Basis::of(k);
        for (a, c) in [(b.d, b.ex), (b.d, b.ey), (b.ex, b.ey)] {
            assert!(a.dot(c).abs() < 1e-14, "{k:?} nicht senkrecht");
        }
        for v in [b.d, b.ex, b.ey] {
            assert!((v.length() - 1.0).abs() < 1e-14);
        }
        assert!(
            (b.ex.cross(b.ey) + b.d).length() < 1e-14,
            "{k:?} kein Rechtssystem"
        );
    }
}

#[test]
fn front_view_projects_x_right_z_up() {
    let b = Basis::of(ViewKind::Front);
    let p = b.project(DVec3::new(3.0, 5.0, 7.0));
    assert_eq!((p.x, p.y), (3.0, 7.0));
    assert_eq!(b.depth(DVec3::new(3.0, 5.0, 7.0)), 5.0);
}

#[test]
fn first_angle_places_top_below_and_left_right_of_front() {
    let top = align_dir(Projection::First, ViewKind::Front, ViewKind::Top).unwrap();
    let left = align_dir(Projection::First, ViewKind::Front, ViewKind::Left).unwrap();
    assert_eq!(top, (0.0, -1.0));
    assert_eq!(left, (1.0, 0.0));
}

#[test]
fn third_angle_mirrors_first_angle() {
    let top = align_dir(Projection::Third, ViewKind::Front, ViewKind::Top).unwrap();
    let right = align_dir(Projection::Third, ViewKind::Front, ViewKind::Right).unwrap();
    assert_eq!(top, (0.0, 1.0));
    assert_eq!(right, (1.0, 0.0));
}

#[test]
fn back_view_sits_right_of_base_in_both_projections() {
    assert_eq!(
        align_dir(Projection::First, ViewKind::Front, ViewKind::Back),
        Some((1.0, 0.0))
    );
    assert_eq!(
        align_dir(Projection::Third, ViewKind::Front, ViewKind::Back),
        Some((1.0, 0.0))
    );
}

#[test]
fn isometric_cannot_be_derived() {
    assert_eq!(
        align_dir(Projection::First, ViewKind::Front, ViewKind::Isometric),
        None
    );
}

#[test]
fn paper_sizes_and_orientation() {
    assert_eq!(Paper::A4.size_mm(), (210.0, 297.0));
    let s = Sheet {
        paper: Paper::A3,
        landscape: true,
    };
    assert_eq!(s.size_mm(), (420.0, 297.0));
}

#[test]
fn scale_series_and_labels() {
    assert_eq!(Scale::SERIES.len(), 10);
    assert_eq!(Scale { num: 1, den: 2 }.label(), "1:2");
    assert_eq!(Scale { num: 5, den: 1 }.label(), "5:1");
    assert!((Scale { num: 1, den: 4 }.factor() - 0.25).abs() < 1e-15);
    assert!(!Scale { num: 0, den: 1 }.is_valid());
}

#[test]
fn removing_a_view_removes_its_children() {
    let mut d = Drawing::new(
        DrawingId(1),
        "Z".into(),
        Sheet {
            paper: Paper::A4,
            landscape: true,
        },
        Projection::First,
    );
    let f = d.add_view(
        ViewKind::Front,
        Scale { num: 1, den: 1 },
        Placement::Free { x: 100.0, y: 100.0 },
        None,
    );
    let t = d.add_view(
        ViewKind::Top,
        Scale { num: 1, den: 1 },
        Placement::Aligned { distance: 80.0 },
        Some(f),
    );
    let iso = d.add_view(
        ViewKind::Isometric,
        Scale { num: 1, den: 1 },
        Placement::Free { x: 200.0, y: 50.0 },
        None,
    );
    let gone = d.remove_view(f);
    assert_eq!(gone, vec![f, t]);
    assert!(d.view(iso).is_some() && d.view(f).is_none() && d.view(t).is_none());
}
