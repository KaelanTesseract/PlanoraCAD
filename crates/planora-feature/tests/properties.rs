use planora_feature::*;
use planora_kernel::BoolOp;
use planora_math::DVec3;
use proptest::prelude::*;
use proptest::test_runner::{Config, RngAlgorithm, RngSeed, TestRng, TestRunner};
use std::cell::Cell;

#[derive(Clone, Debug)]
enum Op {
    AddBox(f64),
    AddCylinder(f64, f64),
    AddBoolean(usize, usize, u8),
    AddMove(usize, f64),
    Resize(usize, f64),
    Rename(usize),
    Suppress(usize),
    Marker(usize),
    Delete(usize),
}

fn op() -> impl Strategy<Value = Op> {
    prop_oneof![
        (1.0f64..10.0).prop_map(Op::AddBox),
        (1.0f64..5.0, 1.0f64..10.0).prop_map(|(r, h)| Op::AddCylinder(r, h)),
        (0usize..8, 0usize..8, 0u8..3).prop_map(|(a, b, o)| Op::AddBoolean(a, b, o)),
        (0usize..8, -5.0f64..5.0).prop_map(|(i, x)| Op::AddMove(i, x)),
        (0usize..8, 1.0f64..10.0).prop_map(|(i, s)| Op::Resize(i, s)),
        (0usize..8).prop_map(Op::Rename),
        (0usize..8).prop_map(Op::Suppress),
        (0usize..9).prop_map(Op::Marker),
        (0usize..8).prop_map(Op::Delete),
    ]
}

fn apply(d: &mut Document, op: &Op) {
    let nth = |d: &Document, i: usize| {
        d.features()
            .get(i % d.features().len().max(1))
            .map(|f| f.id)
    };
    match op {
        Op::AddBox(s) => {
            let _ = d.add_feature(
                "Quader",
                FeatureKind::Box {
                    size: DVec3::splat(*s),
                    origin: DVec3::ZERO,
                },
            );
        }
        Op::AddCylinder(r, h) => {
            let _ = d.add_feature(
                "Zylinder",
                FeatureKind::Cylinder {
                    radius: *r,
                    height: *h,
                    origin: DVec3::new(2.0, 2.0, 0.0),
                },
            );
        }
        Op::AddBoolean(a, b, o) => {
            let vis: Vec<BodyRef> = d.visible_bodies().iter().map(|v| v.body).collect();
            if vis.len() >= 2 {
                let op = [BoolOp::Union, BoolOp::Difference, BoolOp::Intersection][*o as usize];
                let _ = d.add_feature(
                    "Boolesch",
                    FeatureKind::Boolean {
                        target: vis[a % vis.len()],
                        tool: vis[(b + 1) % vis.len()],
                        op,
                    },
                );
            }
        }
        Op::AddMove(i, x) => {
            let vis: Vec<BodyRef> = d.visible_bodies().iter().map(|v| v.body).collect();
            if !vis.is_empty() {
                let _ = d.add_feature(
                    "Verschieben",
                    FeatureKind::Move {
                        body: vis[i % vis.len()],
                        delta: DVec3::new(*x, 0.0, 0.0),
                    },
                );
            }
        }
        Op::Resize(i, s) => {
            if let Some(id) = nth(d, *i) {
                let _ = d.update(id, |k| {
                    if let FeatureKind::Box { size, .. } = k {
                        *size = DVec3::splat(*s);
                    }
                });
            }
        }
        Op::Rename(i) => {
            if let Some(id) = nth(d, *i) {
                let _ = d.rename(id, "Neuer Name");
            }
        }
        Op::Suppress(i) => {
            if let Some(id) = nth(d, *i) {
                let now = d.feature(id).unwrap().suppressed;
                let _ = d.set_suppressed(id, !now);
            }
        }
        Op::Marker(n) => {
            let _ = d.set_marker((*n).min(d.features().len()));
        }
        Op::Delete(i) => {
            if let Some(id) = nth(d, *i) {
                let _ = d.delete_with_dependents(id);
            }
        }
    }
}

#[test]
fn random_edit_sequences_keep_the_document_valid() {
    let checked = Cell::new(0usize);
    let mut runner = TestRunner::new_with_rng(
        Config {
            cases: 40,
            failure_persistence: None,
            rng_seed: RngSeed::Fixed(11),
            ..Config::default()
        },
        TestRng::deterministic_rng(RngAlgorithm::ChaCha),
    );
    runner
        .run(&proptest::collection::vec(op(), 1..12), |ops| {
            let mut d = Document::new();
            for o in &ops {
                apply(&mut d, o);
                prop_assert!(
                    d.check_invariants().is_ok(),
                    "{:?} nach {:?}",
                    d.check_invariants(),
                    o
                );
            }
            let again = d.recompute();
            prop_assert!(
                again.computed.is_empty(),
                "zweiter Lauf rechnet {:?}",
                again.computed
            );
            checked.set(checked.get() + 1);
            Ok(())
        })
        .unwrap();
    assert_eq!(checked.get(), 40);
}

/// Anzahl der Fälle des längeren Laufs.
const LONG_CASES: usize = 120;
/// Mindestanzahl erfolgreich berechneter Boolescher Operationen über alle Fälle; sichert,
/// dass der Lauf nicht nur Fehlerpfade prüft.
const MIN_OK_BOOLEANS: usize = 10;
/// Mindestanzahl erfolgreich berechneter Verschiebungen über alle Fälle.
const MIN_OK_MOVES: usize = 10;

fn ok_count(d: &Document, pick: fn(&FeatureKind) -> bool) -> usize {
    d.features()
        .iter()
        .filter(|f| pick(&f.kind) && f.status == FeatureStatus::Ok)
        .count()
}

#[test]
fn longer_random_edit_sequences_reach_successful_booleans_and_stay_valid() {
    let checked = Cell::new(0usize);
    let ok_booleans = Cell::new(0usize);
    let ok_moves = Cell::new(0usize);
    let mut runner = TestRunner::new_with_rng(
        Config {
            cases: LONG_CASES as u32,
            failure_persistence: None,
            rng_seed: RngSeed::Fixed(23),
            ..Config::default()
        },
        TestRng::deterministic_rng(RngAlgorithm::ChaCha),
    );
    runner
        .run(&proptest::collection::vec(op(), 4..24), |ops| {
            let mut d = Document::new();
            let (mut booleans, mut moves) = (0, 0);
            for o in &ops {
                apply(&mut d, o);
                prop_assert!(
                    d.check_invariants().is_ok(),
                    "{:?} nach {:?}",
                    d.check_invariants(),
                    o
                );
                booleans = booleans.max(ok_count(&d, |k| matches!(k, FeatureKind::Boolean { .. })));
                moves = moves.max(ok_count(&d, |k| matches!(k, FeatureKind::Move { .. })));
            }
            let again = d.recompute();
            prop_assert!(
                again.computed.is_empty(),
                "zweiter Lauf rechnet {:?}",
                again.computed
            );
            ok_booleans.set(ok_booleans.get() + booleans);
            ok_moves.set(ok_moves.get() + moves);
            checked.set(checked.get() + 1);
            Ok(())
        })
        .unwrap();
    assert_eq!(checked.get(), LONG_CASES);
    assert!(
        ok_booleans.get() >= MIN_OK_BOOLEANS,
        "nur {} erfolgreiche Boolesche Operationen",
        ok_booleans.get()
    );
    assert!(
        ok_moves.get() >= MIN_OK_MOVES,
        "nur {} erfolgreiche Verschiebungen",
        ok_moves.get()
    );
}
