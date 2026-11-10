//! Verlängern, Gradwechsel und gleichmäßige Knoten für NURBS-Kurven.

use planora_math::DVec3;
use planora_math::nurbs::*;

/// Kubische, nicht rationale Kurve mit doppeltem inneren Knoten.
fn cubic() -> NurbsCurve {
    let pts: Vec<DVec3> = (0..8)
        .map(|i| {
            let t = i as f64;
            DVec3::new(t * 1.3, (t * 0.9).sin() * 2.0, (t * 0.4).cos())
        })
        .collect();
    // 8 Kontrollpunkte, Grad 3 => 12 Knoten
    let knots = vec![0.0, 0.0, 0.0, 0.0, 0.2, 0.4, 0.4, 0.7, 1.0, 1.0, 1.0, 1.0];
    NurbsCurve::new(3, knots, pts, vec![1.0; 8]).unwrap()
}

fn rational() -> NurbsCurve {
    let pts = vec![
        DVec3::new(0.0, 0.0, 0.0),
        DVec3::new(1.0, 2.0, 0.0),
        DVec3::new(2.5, 2.0, 1.0),
        DVec3::new(4.0, 0.0, 0.5),
        DVec3::new(5.0, 1.0, 0.0),
    ];
    NurbsCurve::new(
        3,
        vec![0.0, 0.0, 0.0, 0.0, 0.5, 1.0, 1.0, 1.0, 1.0],
        pts,
        vec![1.0, 2.0, 0.7, 1.5, 1.0],
    )
    .unwrap()
}

#[test]
fn uniform_knots_shape() {
    assert_eq!(
        uniform_clamped_knots(3, 5),
        vec![0.0, 0.0, 0.0, 0.0, 1.0, 2.0, 2.0, 2.0, 2.0]
    );
    assert_eq!(uniform_clamped_knots(1, 2), vec![0.0, 0.0, 1.0, 1.0]);
}

#[test]
fn unclamped_matches_inside() {
    let c = cubic();
    for i in 0..=50 {
        let u = i as f64 / 50.0;
        assert!((c.point_unclamped(u) - c.point(u)).length() < 1e-12);
        let (a, b) = (c.derivs_unclamped(u, 2), c.derivs(u, 2));
        for k in 0..3 {
            assert!((a[k] - b[k]).length() < 1e-12);
        }
    }
    // Außerhalb: begrenzt bleibt am Ende stehen, unbegrenzt läuft weiter.
    assert_eq!(c.point(2.0), c.point(1.0));
    assert!((c.point_unclamped(2.0) - c.point(1.0)).length() > 1e-3);
    assert!((c.point_unclamped(-1.0) - c.point(0.0)).length() > 1e-3);
}

#[test]
fn extended_keeps_the_old_part() {
    let c = cubic();
    let e = c.extended(-0.5, 1.7).unwrap();
    assert_eq!(e.domain(), (-0.5, 1.7));
    assert_eq!(e.degree(), 3);
    for i in 0..=400 {
        let u = i as f64 / 400.0;
        assert!((e.point(u) - c.point(u)).length() < 1e-9, "u={u}");
    }
    assert!((e.point(-0.5) - c.point_unclamped(-0.5)).length() < 1e-9);
    assert!((e.point(1.7) - c.point_unclamped(1.7)).length() < 1e-9);
    for i in 0..=40 {
        let u = -0.5 + 2.2 * i as f64 / 40.0;
        let want = c.point_unclamped(u);
        // Innerhalb des neuen Bereichs gilt die Fortsetzung der Randstücke.
        if !(0.0..=1.0).contains(&u) {
            assert!((e.point(u) - want).length() < 1e-9, "u={u}");
        }
    }
    // Nur eine Seite verlängern.
    let one = c.extended(0.0, 2.0).unwrap();
    assert!((one.point(2.0) - c.point_unclamped(2.0)).length() < 1e-9);
}

#[test]
fn extended_rejects_rational() {
    assert!(matches!(
        rational().extended(-1.0, 2.0),
        Err(NurbsError::Input(_))
    ));
    // Zu kleiner Bereich ebenfalls.
    assert!(matches!(
        cubic().extended(0.5, 2.0),
        Err(NurbsError::Input(_))
    ));
}

#[test]
fn elevated_is_the_same_curve() {
    let pts: Vec<DVec3> = (0..5)
        .map(|i| DVec3::new(i as f64, ((i * i) % 5) as f64, 0.5 * i as f64))
        .collect();
    let c = NurbsCurve::new(
        3,
        vec![0.0, 0.0, 0.0, 0.0, 0.5, 1.0, 1.0, 1.0, 1.0],
        pts,
        vec![1.0; 5],
    )
    .unwrap();
    let e = c.elevated(5).unwrap();
    assert_eq!(e.degree(), 5);
    for i in 0..=400 {
        let u = i as f64 / 400.0;
        assert!((e.point(u) - c.point(u)).length() < 1e-9, "u={u}");
    }
    // Mit doppeltem Knoten und rational.
    for c in [cubic(), rational()] {
        let e = c.elevated(4).unwrap();
        assert_eq!(e.degree(), 4);
        for i in 0..=200 {
            let u = i as f64 / 200.0;
            assert!((e.point(u) - c.point(u)).length() < 1e-9, "u={u}");
        }
    }
    assert!(c.elevated(2).is_err());
    assert!(c.elevated(MAX_DEGREE + 1).is_err());
    assert_eq!(c.elevated(3).unwrap(), c);
}

#[test]
fn reduced_keeps_the_ends() {
    let c = cubic();
    let r = c.reduced(2, 6).unwrap();
    assert_eq!(r.degree(), 2);
    assert_eq!(r.control_points().len(), 6);
    assert_eq!(r.domain(), c.domain());
    assert!((r.point(0.0) - c.point(0.0)).length() < 1e-12);
    assert!((r.point(1.0) - c.point(1.0)).length() < 1e-12);
    assert!(c.reduced(0, 6).is_err());
    assert!(c.reduced(3, 3).is_err());
}

#[test]
fn reduced_reproduces_a_curve_in_the_space() {
    // Eine Kurve auf gleichmäßigen Knoten bleibt bei gleicher Größe erhalten.
    let pts: Vec<DVec3> = (0..7)
        .map(|i| DVec3::new(i as f64, ((i * 3) % 4) as f64, (i as f64).sin()))
        .collect();
    let k: Vec<f64> = uniform_clamped_knots(3, 7)
        .iter()
        .map(|v| v / 4.0)
        .collect();
    let c = NurbsCurve::new(3, k, pts, vec![1.0; 7]).unwrap();
    let r = c.reduced(3, 7).unwrap();
    for i in 0..=100 {
        let u = i as f64 / 100.0;
        assert!((r.point(u) - c.point(u)).length() < 1e-8, "u={u}");
    }
}
