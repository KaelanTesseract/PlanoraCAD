//! Arbeitsgrenze der Booleschen Operationen mit Freiformflächen (Review, I2): Ein Drehkörper aus
//! einem Skizzen-Spline mit vielen Punkten gegen einen Zylinder darf nicht beliebig lange rechnen.
//! Die Grenze zählt NURBS-Auswertungen, nicht Zeit; das Ergebnis ist bei jedem Lauf gleich.

use planora_kernel::boolean::{
    BooleanError, MAX_FREEFORM_EVALUATIONS, TOO_EXPENSIVE, boolean, boolean_limited,
};
use planora_kernel::{AnalyticKernel, BoolOp, Kernel, ProfileLoop, Segment, Solid, revolve};
use planora_math::nurbs::{NurbsCurve, evaluations};
use planora_math::{DMat4, DVec2, DVec3, Frame, PI, Tolerance};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Pseudozufallszahl in [0, 1) (lineare Kongruenz, fester Startwert).
fn lcg(s: &mut u64) -> f64 {
    *s = s
        .wrapping_mul(6_364_136_223_846_793_005)
        .wrapping_add(1_442_695_040_888_963_407);
    ((*s >> 11) as f64) / ((1u64 << 53) as f64)
}

/// Drehkörper um die y-Achse der Skizze: Spline durch `n` Punkte mit Radius 3 … 8 (zufällig)
/// und Höhe 0 … 10 (gleichmäßig steigend), unten und oben über Linien zur Achse geschlossen.
fn spline_vase(seed: u64, n: usize) -> Solid {
    let mut s = seed;
    let pts: Vec<DVec3> = (0..n)
        .map(|i| {
            DVec3::new(
                3.0 + 5.0 * lcg(&mut s),
                10.0 * i as f64 / (n - 1) as f64,
                0.0,
            )
        })
        .collect();
    let params: Vec<f64> = (0..n).map(|i| i as f64).collect();
    let spline = NurbsCurve::natural_cubic(&pts, &params).unwrap();
    let chain = ProfileLoop {
        start: DVec2::ZERO,
        segments: vec![
            Segment::Line {
                to: DVec2::new(pts[0].x, 0.0),
            },
            Segment::Nurbs {
                curve: Arc::new(spline),
            },
            Segment::Line {
                to: DVec2::new(0.0, 10.0),
            },
        ],
    };
    revolve(
        &chain,
        &Frame::from_origin_normal(DVec3::ZERO, DVec3::Z),
        &Tolerance::default(),
    )
    .unwrap()
}

/// Zylinder r = 2 quer durch die Vase (Achse x, auf halber Höhe).
fn crossing_cylinder() -> Solid {
    let k = AnalyticKernel::default();
    k.transform(
        &k.make_cylinder(2.0, 30.0).unwrap(),
        &(DMat4::from_translation(DVec3::new(-15.0, 5.0, 0.0)) * DMat4::from_rotation_y(PI / 2.0)),
    )
    .unwrap()
}

#[test]
fn a_spline_with_39_points_stops_at_a_small_work_limit_with_a_named_error() {
    // Kleine Grenze, damit der Test im Debug-Lauf kurz bleibt; das Verhalten an der Grenze ist
    // dasselbe wie bei der echten Grenze.
    const SMALL: u64 = 300_000;
    let (a, b) = (spline_vase(7, 39), crossing_cylinder());
    let run = || {
        let start = evaluations();
        let r = boolean_limited(&a, &b, BoolOp::Union, &Tolerance::default(), SMALL);
        (r, evaluations() - start)
    };
    let t = Instant::now();
    let (first, used) = run();
    assert_eq!(
        first.as_ref().map(|_| ()),
        Err(&BooleanError::Unsupported(TOO_EXPENSIVE.into())),
        "{first:?}"
    );
    assert!(TOO_EXPENSIVE.contains("zu aufwendig"));
    // Die Grenze greift an der nächsten Prüfstelle; der Überhang ist eine Stufe der Pipeline.
    assert!(used > SMALL && used < 20 * SMALL, "{used}");
    assert!(t.elapsed() < Duration::from_secs(25), "{:?}", t.elapsed());
    // Deterministisch: derselbe Lauf endet gleich und an derselben Stelle.
    let (second, used2) = run();
    assert_eq!(format!("{first:?}"), format!("{second:?}"));
    assert_eq!(used, used2);
}

#[test]
fn analytic_booleans_have_no_work_limit() {
    // Ohne Freiformfläche gilt keine Grenze, auch nicht mit Grenze 0.
    let k = AnalyticKernel::default();
    let a = k.make_box(DVec3::splat(4.0)).unwrap();
    let b = k
        .transform(
            &k.make_cylinder(1.0, 10.0).unwrap(),
            &DMat4::from_translation(DVec3::new(2.0, 2.0, -3.0)),
        )
        .unwrap();
    let limited = boolean_limited(&a, &b, BoolOp::Difference, &Tolerance::default(), 0);
    let normal = boolean(&a, &b, BoolOp::Difference, &Tolerance::default());
    assert!(limited.is_ok());
    assert_eq!(format!("{limited:?}"), format!("{normal:?}"));
}

/// Mit der echten Grenze endet der 39-Punkte-Fall mit einem Körper oder einer benannten Meldung,
/// in wenigen Sekunden (Release; im Debug-Lauf zu langsam).
#[test]
#[ignore = "nur mit --release --ignored (etwa 5 s je Operation)"]
fn a_spline_with_39_points_ends_within_the_real_limit() {
    let b = crossing_cylinder();
    for seed in [1, 3, 7] {
        let a = spline_vase(seed, 39);
        for op in [BoolOp::Union, BoolOp::Difference, BoolOp::Intersection] {
            let t = Instant::now();
            let start = evaluations();
            let r = boolean(&a, &b, op, &Tolerance::default());
            let used = evaluations() - start;
            assert!(
                used <= MAX_FREEFORM_EVALUATIONS + MAX_FREEFORM_EVALUATIONS / 2,
                "{seed} {op:?}: {used}"
            );
            assert!(
                t.elapsed() < Duration::from_secs(20),
                "{seed} {op:?}: {:?}",
                t.elapsed()
            );
            if let Err(e) = r {
                assert!(
                    matches!(
                        e,
                        BooleanError::Unsupported(_)
                            | BooleanError::AmbiguousContact
                            | BooleanError::AmbiguousClassification
                            | BooleanError::InvalidInput(_)
                    ),
                    "{seed} {op:?}: {e:?}"
                );
            }
        }
    }
}
