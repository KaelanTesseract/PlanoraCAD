//! Eigenschafts-, Ketten- und Randfalltests für Boolesche Operationen (Spec §10).
//!
//! Die Eigenschaftstests laufen deterministisch (fester Startwert, keine Regressionsdateien),
//! damit die Testreihe reproduzierbar und schnell bleibt.

use planora_kernel::boolean::*;
use planora_kernel::*;
use planora_math::*;
use planora_mesh::*;
use proptest::prelude::*;
use proptest::test_runner::{RngSeed, TestCaseError, TestRunner};
use std::cell::Cell;

fn k() -> AnalyticKernel {
    AnalyticKernel::default()
}

fn moved(s: &Solid, v: DVec3) -> Solid {
    k().transform(s, &DMat4::from_translation(v)).unwrap()
}

fn vol(s: &Solid) -> f64 {
    validate(s).expect("Ergebnis muss gültig sein");
    let m = tessellate(
        s,
        &TessellationOptions {
            chord_tolerance: 0.001,
        },
    )
    .unwrap();
    assert!(is_watertight(&m, 1e-6));
    mass_properties(&m).volume
}

/// Feste, kleine Konfiguration: 40 Fälle, fester Startwert, keine Regressionsdateien.
fn config() -> ProptestConfig {
    ProptestConfig {
        rng_seed: RngSeed::Fixed(0x5EED_B001),
        failure_persistence: None,
        ..ProptestConfig::with_cases(40)
    }
}

/// Führt eine Eigenschaft mit `config()` aus; anders als das `proptest!`-Makro lässt sich danach
/// noch prüfen, wie viele Fälle tatsächlich etwas getestet haben.
fn run_property<S: Strategy>(strategy: S, test: impl Fn(S::Value) -> Result<(), TestCaseError>)
where
    S::Value: std::fmt::Debug,
{
    let mut runner = TestRunner::new(config());
    if let Err(e) = runner.run(&strategy, test) {
        panic!(
            "{e}
{runner}"
        );
    }
}

/// Überlappungsvolumen zweier achsenparalleler Quader `[lo, hi]`.
fn box_overlap(a: (DVec3, DVec3), b: (DVec3, DVec3)) -> f64 {
    let ext = (a.1.min(b.1) - a.0.max(b.0)).max(DVec3::ZERO);
    ext.x * ext.y * ext.z
}

/// Ein Fehler ist hier nur zulässig, wenn er geometrisch feststeht: `EmptyResult`, wenn das
/// erwartete Volumen 0 ist. Jeder andere Fehler lässt die Eigenschaft scheitern.
fn op_volume(a: &Solid, b: &Solid, op: BoolOp, expected: f64) -> Result<f64, TestCaseError> {
    match k().boolean(a, b, op) {
        Ok(s) => Ok(vol(&s)),
        Err(BooleanError::EmptyResult) if expected < 1e-9 => Ok(0.0),
        Err(e) => Err(TestCaseError::fail(format!(
            "{op:?}: unerwarteter Fehler „{e}“ (erwartetes Volumen {expected})"
        ))),
    }
}

#[test]
fn box_box_volume_balance() {
    let cases = Cell::new(0usize);
    let overlapping = Cell::new(0usize);
    run_property(
        (
            (2.0f64..12.0, 2.0f64..12.0, 2.0f64..12.0),
            (-9.0f64..9.0, -9.0f64..9.0, -9.0f64..9.0),
        ),
        |((sx, sy, sz), (dx, dy, dz))| {
            // allgemeine Lage: Versätze nie genau 0, damit keine bündigen Kanten entstehen
            let d = DVec3::new(dx + 0.0137, dy + 0.0291, dz + 0.0113);
            let size = DVec3::new(sx, sy, sz);
            let a = k().make_box(DVec3::new(10.0, 10.0, 10.0)).unwrap();
            let b = moved(&k().make_box(size).unwrap(), d);
            let (va, vb) = (vol(&a), vol(&b));
            // Quader in allgemeiner Lage: das Schnittvolumen ist exakt bekannt, also auch,
            // welche Operation leer sein muss – Fehler werden nicht übersprungen.
            let vo = box_overlap((DVec3::ZERO, DVec3::splat(10.0)), (d, d + size));
            cases.set(cases.get() + 1);
            if vo > 0.0 {
                overlapping.set(overlapping.get() + 1);
            }
            let vu = op_volume(&a, &b, BoolOp::Union, va + vb - vo)?;
            let vi = op_volume(&a, &b, BoolOp::Intersection, vo)?;
            let vd = op_volume(&a, &b, BoolOp::Difference, va - vo)?;
            prop_assert!((vi - vo).abs() < 1e-6 * (va + vb), "V(A∩B) = {vi} ≠ {vo}");
            prop_assert!(
                (vu + vi - (va + vb)).abs() < 1e-6 * (va + vb),
                "V(A∪B)+V(A∩B) = {} ≠ {}",
                vu + vi,
                va + vb
            );
            prop_assert!(
                (vd - (va - vi)).abs() < 1e-6 * va,
                "V(A−B) = {vd} ≠ {}",
                va - vi
            );
            Ok(())
        },
    );
    // Nicht leer laufen lassen: genug Fälle mit echter Überlappung
    assert_eq!(cases.get(), 40);
    assert!(
        overlapping.get() >= 20,
        "nur {} überlappende Fälle",
        overlapping.get()
    );
}

#[test]
fn box_cylinder_volume_balance() {
    let tried = Cell::new(0usize);
    let checked = Cell::new(0usize);
    run_property(
        (
            0.8f64..3.5,
            3.0f64..14.0,
            (0.5f64..9.5, 0.5f64..9.5, -3.0f64..8.0),
        ),
        |(r, h, (x, y, z))| {
            let a = k().make_box(DVec3::new(10.0, 10.0, 10.0)).unwrap();
            let c = moved(
                &k().make_cylinder(r, h).unwrap(),
                DVec3::new(x + 0.0071, y + 0.0043, z + 0.0087),
            );
            let (va, vb) = (vol(&a), vol(&c));
            tried.set(tried.get() + 1);
            // Die Achse liegt immer im Quader: die Körper überlappen, kein Ergebnis ist leer.
            // Übersprungen werden nur die dokumentierten Grenzfälle (nicht unterstützt,
            // Berührung, unsichere Einordnung); jeder andere Fehler lässt die Eigenschaft scheitern.
            let mut vols = Vec::new();
            for op in [BoolOp::Union, BoolOp::Intersection, BoolOp::Difference] {
                match k().boolean(&a, &c, op) {
                    Ok(s) => vols.push(vol(&s)),
                    Err(
                        BooleanError::UnsupportedIntersection { .. }
                        | BooleanError::AmbiguousContact
                        | BooleanError::AmbiguousClassification,
                    ) => prop_assume!(false),
                    Err(e) => return Err(TestCaseError::fail(format!("{op:?}: {e}"))),
                }
            }
            checked.set(checked.get() + 1);
            let (vu, vi, vd) = (vols[0], vols[1], vols[2]);
            let tol = 0.01 * (va + vb);
            prop_assert!(
                (vu + vi - (va + vb)).abs() < tol,
                "{vu} + {vi} ≠ {va} + {vb}"
            );
            prop_assert!((vd - (va - vi)).abs() < tol, "{vd} ≠ {va} − {vi}");
            for (op, v) in [
                (BoolOp::Union, vu),
                (BoolOp::Difference, vd),
                (BoolOp::Intersection, vi),
            ] {
                prop_assert!(boolean_volume_plausible(op, va, vb, v, 0.02));
            }
            Ok(())
        },
    );
    // Höchstens 10 % der Lagen dürfen als Grenzfall übersprungen werden.
    assert_eq!(checked.get(), 40);
    assert!(
        checked.get() * 10 >= tried.get() * 9,
        "{} von {} Fällen übersprungen",
        tried.get() - checked.get(),
        tried.get()
    );
}

#[test]
fn plate_with_three_holes_chain() {
    let mut s = k().make_box(DVec3::new(30.0, 12.0, 4.0)).unwrap();
    let mut expected = 30.0 * 12.0 * 4.0;
    for cx in [5.0, 15.0, 25.0] {
        let hole = moved(
            &k().make_cylinder(2.0, 8.0).unwrap(),
            DVec3::new(cx, 6.0, -2.0),
        );
        s = k().boolean(&s, &hole, BoolOp::Difference).unwrap();
        expected -= PI * 4.0 * 4.0;
        let v = vol(&s);
        assert!(
            ((v - expected) / expected).abs() < 0.01,
            "{v} vs {expected}"
        );
    }
    assert_eq!(validate(&s).unwrap().genus, 3);
}

#[test]
fn union_then_difference_chain() {
    let base = k().make_box(DVec3::new(20.0, 20.0, 5.0)).unwrap();
    let post = moved(
        &k().make_cylinder(4.0, 15.0).unwrap(),
        DVec3::new(10.0, 10.0, 0.0),
    );
    let s1 = k().boolean(&base, &post, BoolOp::Union).unwrap();
    let bore = moved(
        &k().make_cylinder(2.0, 20.0).unwrap(),
        DVec3::new(10.0, 10.0, -2.0),
    );
    let s2 = k().boolean(&s1, &bore, BoolOp::Difference).unwrap();
    let expected = 20.0 * 20.0 * 5.0 + PI * 16.0 * 10.0 - PI * 4.0 * 15.0;
    let v = vol(&s2);
    assert!(
        ((v - expected) / expected).abs() < 0.01,
        "{v} vs {expected}"
    );
}

#[test]
fn tangent_cylinder_beside_a_box_does_not_panic() {
    let a = k().make_box(DVec3::splat(10.0)).unwrap();
    let c = moved(
        &k().make_cylinder(2.0, 10.0).unwrap(),
        DVec3::new(12.0, 5.0, 0.0),
    ); // berührt die Seitenfläche
    for op in [BoolOp::Union, BoolOp::Difference, BoolOp::Intersection] {
        // Ok oder benannter Fehler, aber keine Panik; ein Ergebnis muss gültig und dicht sein
        if let Ok(s) = k().boolean(&a, &c, op) {
            let v = vol(&s);
            assert!(v > 0.0, "{op:?}: {v}");
        }
    }
}
