//! 3D-Punkte im Skizzenmodell (Skizzen 10, Task 1).

use planora_math::{DVec2, DVec3, Frame, Tolerance};
use planora_sketch::SketchError;
use planora_sketch::model::{MAX_COORD, Sketch};

fn sketch() -> Sketch {
    Sketch::new(Frame::WORLD, "S", Tolerance::default())
}

#[test]
fn point3_roundtrips_xyz() {
    let mut s = sketch();
    let p = s.add_point3(DVec3::new(1.0, 2.0, 3.0)).unwrap();
    assert!(s.is_3d_point(p));
    assert_eq!(s.point_xyz(p).unwrap(), DVec3::new(1.0, 2.0, 3.0));
    // Die Draufsicht liefert (x, y).
    assert_eq!(s.point_xy(p).unwrap(), DVec2::new(1.0, 2.0));
    assert_eq!(s.variable_count(), 3);
    s.set_point_xyz(p, DVec3::new(-4.0, 5.0, -6.0)).unwrap();
    assert_eq!(s.point_xyz(p).unwrap(), DVec3::new(-4.0, 5.0, -6.0));
}

#[test]
fn point3_at_z0_stays_3d() {
    let mut s = sketch();
    let p = s.add_point3(DVec3::new(1.0, 2.0, 0.0)).unwrap();
    assert!(s.is_3d_point(p));
    assert_eq!(s.variable_count(), 3);
    s.set_point_xyz(p, DVec3::new(1.0, 2.0, 7.0)).unwrap();
    s.set_point_xyz(p, DVec3::new(1.0, 2.0, 0.0)).unwrap();
    assert!(
        s.is_3d_point(p),
        "z = 0 macht einen 3D-Punkt nicht wieder 2D"
    );
    // Daten tragen z mit und laden ihn wieder.
    let d = s.to_data();
    assert!(d.points[p.index()].z.is_some());
    let back = Sketch::from_data(d.clone()).unwrap();
    assert!(back.is_3d_point(p));
    assert_eq!(back.to_data(), d);
}

#[test]
fn plain_point_has_no_z_var() {
    let mut s = sketch();
    let before = s.variable_count();
    let p = s.add_point(1.0, 2.0).unwrap();
    assert_eq!(s.variable_count(), before + 2);
    assert!(!s.is_3d_point(p));
    assert_eq!(s.point_xyz(p).unwrap(), DVec3::new(1.0, 2.0, 0.0));
    assert_eq!(s.to_data().points[p.index()].z, None);
    // Setzen mit z = 0 bleibt 2D und erlaubt.
    s.set_point_xyz(p, DVec3::new(3.0, 4.0, 0.0)).unwrap();
    assert!(!s.is_3d_point(p));
    assert_eq!(s.variable_count(), before + 2);
    assert_eq!(s.point_xy(p).unwrap(), DVec2::new(3.0, 4.0));
}

#[test]
fn z_over_max_coord_is_rejected() {
    let mut s = sketch();
    let n = s.variable_count();
    for z in [2.0 * MAX_COORD, -2.0 * MAX_COORD, f64::NAN, f64::INFINITY] {
        assert!(matches!(
            s.add_point3(DVec3::new(0.0, 0.0, z)),
            Err(SketchError::InvalidInput(_))
        ));
    }
    assert_eq!(s.variable_count(), n, "abgelehnt hinterlässt nichts");
    let p = s.add_point3(DVec3::new(0.0, 0.0, 1.0)).unwrap();
    assert!(matches!(
        s.set_point_xyz(p, DVec3::new(0.0, 0.0, 2.0 * MAX_COORD)),
        Err(SketchError::InvalidInput(_))
    ));
    assert_eq!(s.point_xyz(p).unwrap(), DVec3::new(0.0, 0.0, 1.0));
}

#[test]
fn set_z_on_2d_point_is_rejected() {
    let mut s = sketch();
    let p = s.add_point(1.0, 2.0).unwrap();
    assert!(matches!(
        s.set_point_xyz(p, DVec3::new(5.0, 6.0, 1.0)),
        Err(SketchError::InvalidInput(_))
    ));
    assert!(!s.is_3d_point(p));
    assert_eq!(s.point_xy(p).unwrap(), DVec2::new(1.0, 2.0), "unverändert");
}

#[test]
fn three_d_flag_is_stored_and_carried_in_data() {
    let mut s = sketch();
    assert!(!s.three_d);
    assert!(!s.to_data().three_d);
    s.set_three_d(true);
    assert!(s.three_d);
    let d = s.to_data();
    assert!(d.three_d);
    assert!(Sketch::from_data(d).unwrap().three_d);
    assert!(s.clone().three_d);
    s.set_three_d(false);
    assert!(!s.three_d);
}
