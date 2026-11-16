//! Krümmungskamm: Modell und Rechnung (Skizzen 11a, Task 5).

mod common;

use planora_math::{DVec2, DVec3, Frame, Tolerance};
use planora_sketch::SketchError;
use planora_sketch::comb::{
    COMB_DENSITY, COMB_MAX_FRACTION, COMB_SCALE_MAX, COMB_SCALE_MIN, CombSettings, comb_teeth,
};
use planora_sketch::constraint::ConstraintKind as K;
use planora_sketch::model::{CurveId, Sketch};
use planora_sketch::sparse::SolverPath;
use planora_sketch::transform::Similarity;

fn sketch() -> Sketch {
    Sketch::new(Frame::WORLD, "S", Tolerance::default())
}

fn circle(s: &mut Sketch, r: f64) -> CurveId {
    let m = s.add_point(0.0, 0.0).unwrap();
    s.add_circle(m, r).unwrap()
}

/// Offener Kegelschnitt von (0,0) nach (10,0) mit dem Scheitel (5,5).
fn conic(s: &mut Sketch) -> CurveId {
    let a = s.add_point(0.0, 0.0).unwrap();
    let b = s.add_point(10.0, 0.0).unwrap();
    let apex = s.add_point(5.0, 5.0).unwrap();
    s.add_conic(a, b, apex, 0.5).unwrap()
}

fn longest(teeth: &[(DVec3, DVec3)]) -> f64 {
    teeth
        .iter()
        .map(|(f, t)| f.distance(*t))
        .fold(0.0, f64::max)
}

#[test]
fn circle_comb_constant_and_outward() {
    let mut s = sketch();
    let c = circle(&mut s, 5.0);
    let samples = s.comb_samples(c, 8).unwrap();
    assert_eq!(samples.len(), 8);
    for k in &samples {
        assert!((k.kappa - 0.2).abs() < 1e-12, "{k:?}");
        // vom Mittelpunkt weg, also in Richtung des Ortes
        assert!((k.out - k.at / 5.0).length() < 1e-12, "{k:?}");
    }
    let first = samples[0];
    assert!((first.at - DVec3::new(5.0, 0.0, 0.0)).length() < 1e-12);
    assert!((first.out - DVec3::X).length() < 1e-12);
}

#[test]
fn ellipse_curvature_at_vertices() {
    let mut s = sketch();
    let m = s.add_point(0.0, 0.0).unwrap();
    let e = s.add_ellipse(m, DVec2::new(4.0, 0.0), 2.0).unwrap();
    let samples = s.comb_samples(e, 8).unwrap();
    // Stelle 0: Hauptscheitel (4,0), Stelle 2: Nebenscheitel (0,2)
    assert!((samples[0].kappa - 1.0).abs() < 1e-9, "{:?}", samples[0]);
    assert!((samples[2].kappa - 0.125).abs() < 1e-9, "{:?}", samples[2]);
    assert!((samples[2].at - DVec3::new(0.0, 2.0, 0.0)).length() < 1e-9);
}

#[test]
fn density_gives_tooth_count() {
    let mut s = sketch();
    let m = s.add_point(0.0, 0.0).unwrap();
    let a = s.add_point(5.0, 0.0).unwrap();
    let b = s.add_point(0.0, 5.0).unwrap();
    let arc = s.add_arc(m, a, b).unwrap();
    let samples = s.comb_samples(arc, 20).unwrap();
    assert_eq!(samples.len(), 20);
    assert!((samples[0].at - DVec3::new(5.0, 0.0, 0.0)).length() < 1e-9);
    assert!((samples[19].at - DVec3::new(0.0, 5.0, 0.0)).length() < 1e-9);
    // geschlossene Kurve ohne doppelten Endpunkt
    let c = circle(&mut s, 3.0);
    let ring = s.comb_samples(c, 20).unwrap();
    assert_eq!(ring.len(), 20);
    assert!((ring[0].at - ring[19].at).length() > 0.1);
}

#[test]
fn longest_tooth_is_15_percent() {
    let mut s = sketch();
    let c = conic(&mut s);
    let len = s.curve_length(c).unwrap();
    assert!(len > 10.0 && len < 15.0, "{len}");
    let samples = s.comb_samples(c, 30).unwrap();
    let teeth = comb_teeth(&samples, len, 1.0, 0.0);
    let l1 = longest(&teeth);
    assert!((l1 - COMB_MAX_FRACTION * len).abs() < 1e-9, "{l1}");
    // der Maßstab wirkt linear
    let l2 = longest(&comb_teeth(&samples, len, 2.0, 0.0));
    assert!((l2 - 2.0 * l1).abs() < 1e-9);
}

#[test]
fn min_world_lifts_short_curves() {
    let mut s = sketch();
    let m = s.add_point(0.0, 0.0).unwrap();
    // Viertelkreis mit Länge 1: Radius 2/π
    let r = 2.0 / std::f64::consts::PI;
    let a = s.add_point(r, 0.0).unwrap();
    let b = s.add_point(0.0, r).unwrap();
    let arc = s.add_arc(m, a, b).unwrap();
    let len = s.curve_length(arc).unwrap();
    assert!((len - 1.0).abs() < 1e-9, "{len}");
    let samples = s.comb_samples(arc, 10).unwrap();
    let l = longest(&comb_teeth(&samples, len, 1.0, 2.0));
    assert!((l - 2.0).abs() < 1e-9, "{l}");
}

#[test]
fn comb_on_straight_spline_has_zero_teeth() {
    let mut s = sketch();
    let pts: Vec<_> = (0..5)
        .map(|i| s.add_point(f64::from(i) * 3.0, f64::from(i) * 2.0).unwrap())
        .collect();
    let c = s.add_control_spline(&pts, 3).unwrap();
    let len = s.curve_length(c).unwrap();
    let samples = s.comb_samples(c, 20).unwrap();
    let teeth = comb_teeth(&samples, len, 1.0, 0.0);
    assert_eq!(teeth.len(), 20);
    for (foot, tip) in teeth {
        assert!(foot.is_finite() && tip.is_finite());
        assert_eq!(foot, tip);
    }
}

#[test]
fn line_has_no_comb() {
    let mut s = sketch();
    let a = s.add_point(0.0, 0.0).unwrap();
    let b = s.add_point(3.0, 0.0).unwrap();
    let l = s.add_line(a, b).unwrap();
    assert!(!s.comb_capable(l));
    assert!(matches!(
        s.set_comb(l, Some(CombSettings::default())),
        Err(SketchError::InvalidInput(_))
    ));
    assert!(s.comb(l).is_none());
    assert!(s.comb_samples(l, 20).is_none());
}

#[test]
fn comb_out_of_range_rejected() {
    let mut s = sketch();
    let c = circle(&mut s, 5.0);
    assert!(s.comb_capable(c));
    assert_eq!((*COMB_DENSITY.start(), *COMB_DENSITY.end()), (2, 100));
    assert_eq!((COMB_SCALE_MIN, COMB_SCALE_MAX), (0.01, 100.0));
    for (density, scale) in [(1, 1.0), (101, 1.0), (20, 0.0), (20, 101.0), (20, f64::NAN)] {
        let r = s.set_comb(c, Some(CombSettings { density, scale }));
        assert!(
            matches!(r, Err(SketchError::InvalidInput(_))),
            "{density} {scale}"
        );
    }
    assert!(s.comb(c).is_none());
    // die Grenzen selbst sind erlaubt
    for (density, scale) in [(2, 0.01), (100, 100.0)] {
        s.set_comb(c, Some(CombSettings { density, scale }))
            .unwrap();
        assert_eq!(s.comb(c), Some(CombSettings { density, scale }));
    }
    s.set_comb(c, None).unwrap();
    assert!(s.comb(c).is_none());
    assert_eq!(
        CombSettings::default(),
        CombSettings {
            density: 20,
            scale: 1.0
        }
    );
}

#[test]
fn set_comb_keeps_values_and_cache() {
    let mut s = sketch();
    s.set_solver_path(SolverPath::Dense);
    let a = s.add_point(0.0, 0.0).unwrap();
    let b = s.add_point(4.0, 0.0).unwrap();
    s.add_line(a, b).unwrap();
    let c = s.add_circle(b, 2.0).unwrap();
    s.add_constraint(K::Fixed(a, 0.0, 0.0)).unwrap();
    s.add_constraint(K::Distance(a, b, 4.0)).unwrap();
    let first = s.solve();
    assert!(first.group_solves >= 1, "{first:?}");
    let before = common::values_bits(&s);
    let set = CombSettings {
        density: 33,
        scale: 2.5,
    };
    s.set_comb(c, Some(set)).unwrap();
    assert_eq!(common::values_bits(&s), before);
    let second = s.solve();
    assert_eq!(second.group_solves, 0, "{second:?}");
    assert_eq!(common::values_bits(&s), before);
    assert_eq!(s.comb(c), Some(set));
}

#[test]
fn comb_3d_arc_in_its_plane() {
    let mut s = sketch();
    let m = s.add_point3(DVec3::ZERO).unwrap();
    let a = s.add_point3(DVec3::new(0.0, 3.0, 0.0)).unwrap();
    let b = s.add_point3(DVec3::new(0.0, 0.0, 3.0)).unwrap();
    let arc = s.add_arc3(m, a, b, DVec3::X).unwrap();
    let samples = s.comb_samples(arc, 12).unwrap();
    assert_eq!(samples.len(), 12);
    for k in &samples {
        assert!(k.out.dot(DVec3::X).abs() < 1e-9, "{k:?}");
        assert!((k.out.length() - 1.0).abs() < 1e-9);
        assert!((k.kappa - 1.0 / 3.0).abs() < 1e-9);
        // vom Mittelpunkt weg
        assert!(k.out.dot(k.at) > 0.0, "{k:?}");
    }
}

#[test]
fn comb_travels_with_copies_and_break_pieces() {
    let mut s = sketch();
    let m = s.add_point(0.0, 0.0).unwrap();
    let a = s.add_point(5.0, 0.0).unwrap();
    let b = s.add_point(-5.0, 0.0).unwrap();
    let arc = s.add_arc(m, a, b).unwrap();
    let set = CombSettings {
        density: 40,
        scale: 3.0,
    };
    s.set_comb(arc, Some(set)).unwrap();
    // Kopie
    let tset = s.transform_set(&[arc], &[]);
    let shift = Similarity {
        offset: DVec2::new(0.0, -20.0),
        ..Similarity::identity(DVec2::ZERO)
    };
    let map = s.copy_transformed(&tset, &shift).unwrap();
    assert_eq!(s.comb(map.curves[&arc]), Some(set));
    // Brechen: beide Teile
    let (p, q) = (
        s.add_point(0.0, -10.0).unwrap(),
        s.add_point(0.0, 10.0).unwrap(),
    );
    s.add_line(p, q).unwrap();
    let pieces = s.break_curve(arc, DVec2::new(0.0, 5.0)).unwrap();
    assert_eq!(pieces.len(), 2);
    for p in pieces {
        assert_eq!(s.comb(p), Some(set));
    }
}
