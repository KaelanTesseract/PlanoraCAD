use planora_math::{DVec2, Frame, TAU, Tolerance};
use planora_sketch::constraint::ConstraintKind as K;
use planora_sketch::model::Sketch;
use planora_sketch::solver::SolveStatus;
use proptest::prelude::*;
use proptest::test_runner::{Config, RngAlgorithm, RngSeed, TestRng, TestRunner};
use std::cell::Cell;

fn runner() -> TestRunner {
    TestRunner::new_with_rng(
        Config {
            cases: 40,
            failure_persistence: None,
            rng_seed: RngSeed::Fixed(7),
            ..Config::default()
        },
        TestRng::deterministic_rng(RngAlgorithm::ChaCha),
    )
}

/// Alle Abhängigkeiten sind erfüllt (Residuen unter der Schwelle).
fn all_satisfied(s: &Sketch) -> bool {
    s.constraints().all(|(id, c)| {
        c.driven
            || s.evaluate(id, s.variables())
                .is_some_and(|ev| ev.residuals.iter().all(|r| r.v.abs() < 1e-6))
    })
}

#[test]
fn polygons_with_known_dimensions_are_found_from_a_disturbed_start() {
    let checked = Cell::new(0usize);
    let strategy = (
        3usize..7,
        5.0f64..30.0,
        proptest::collection::vec((-1.0f64..1.0, -1.0f64..1.0), 6),
    );
    runner()
        .run(&strategy, |(n, size, noise)| {
            // konvexes n-Eck auf einem Kreis (die Lösung), dazu eine gestörte Startlage
            let truth: Vec<DVec2> = (0..n)
                .map(|i| DVec2::from_angle(TAU * i as f64 / n as f64) * size)
                .collect();
            let mut s = Sketch::new(Frame::WORLD, "S", Tolerance::default());
            let pts: Vec<_> = truth
                .iter()
                .zip(&noise)
                .map(|(p, (dx, dy))| {
                    s.add_point(p.x + dx * 0.05 * size, p.y + dy * 0.05 * size)
                        .unwrap()
                })
                .collect();
            let lines: Vec<_> = (0..n)
                .map(|i| s.add_line(pts[i], pts[(i + 1) % n]).unwrap())
                .collect();
            s.add_constraint(K::Fixed(pts[0], truth[0].x, truth[0].y))
                .unwrap();
            for i in 0..n {
                let d = truth[i].distance(truth[(i + 1) % n]);
                s.add_constraint(K::Distance(pts[i], pts[(i + 1) % n], d))
                    .unwrap();
            }
            for i in 0..n {
                let (a, b) = (
                    truth[(i + 1) % n] - truth[i],
                    truth[(i + 2) % n] - truth[(i + 1) % n],
                );
                let angle = a.perp_dot(b).atan2(a.dot(b));
                s.add_constraint(K::Angle(lines[i], lines[(i + 1) % n], angle))
                    .unwrap();
            }
            let rep = s.solve();
            prop_assert_eq!(rep.status, SolveStatus::Solved, "{:?}", rep);
            prop_assert!(all_satisfied(&s));
            checked.set(checked.get() + 1);
            Ok(())
        })
        .unwrap();
    assert_eq!(checked.get(), 40, "alle Fälle wurden wirklich geprüft");
}

#[test]
fn circles_with_known_radii_and_tangent_lines_are_found() {
    let checked = Cell::new(0usize);
    let strategy = (
        2.0f64..10.0,
        2.0f64..10.0,
        15.0f64..40.0,
        (-1.0f64..1.0, -1.0f64..1.0),
    );
    runner()
        .run(&strategy, |(r1, r2, dist, (nx, ny))| {
            let mut s = Sketch::new(Frame::WORLD, "S", Tolerance::default());
            let c1 = s.add_point(0.0, 0.0).unwrap();
            let c2 = s.add_point(dist + nx * 0.3, ny * 0.3).unwrap();
            let k1 = s.add_circle(c1, r1 * 1.05).unwrap();
            let k2 = s.add_circle(c2, r2 * 0.95).unwrap();
            // äußere Tangentenlinie zwischen den Kreisen oben: Endpunkte liegen auf den Kreisen
            let p1 = s.add_point(0.0, r1).unwrap();
            let p2 = s.add_point(dist, r2).unwrap();
            let line = s.add_line(p1, p2).unwrap();
            s.add_constraint(K::Fixed(c1, 0.0, 0.0)).unwrap();
            s.add_constraint(K::Distance(c1, c2, dist)).unwrap();
            s.add_constraint(K::Horizontal(
                planora_sketch::constraint::LineOrPoints::Points(c1, c2),
            ))
            .unwrap();
            s.add_constraint(K::Radius(k1, r1)).unwrap();
            s.add_constraint(K::Radius(k2, r2)).unwrap();
            s.add_constraint(K::Tangent(line, k1)).unwrap();
            s.add_constraint(K::Tangent(line, k2)).unwrap();
            s.add_constraint(K::PointOnCurve(p1, k1)).unwrap();
            s.add_constraint(K::PointOnCurve(p2, k2)).unwrap();
            let rep = s.solve();
            prop_assert_eq!(rep.status, SolveStatus::Solved, "{:?}", rep);
            prop_assert!(all_satisfied(&s));
            checked.set(checked.get() + 1);
            Ok(())
        })
        .unwrap();
    assert_eq!(checked.get(), 40);
}
