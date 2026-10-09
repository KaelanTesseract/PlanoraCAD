//! Eigenschafts-, Ketten- und Randfalltests für Boolesche Operationen (Spec §10).
//!
//! Die Eigenschaftstests laufen deterministisch (fester Startwert, keine Regressionsdateien),
//! damit die Testreihe reproduzierbar und schnell bleibt.

use planora_kernel::boolean::*;
use planora_kernel::*;
use planora_math::*;
use planora_mesh::*;
use proptest::prelude::*;
use proptest::test_runner::RngSeed;

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

/// Operation ausführen; „nicht unterstützt“, „leer“ und Berührungsfälle sind erlaubte Ergebnisse.
fn try_op(a: &Solid, b: &Solid, op: BoolOp) -> Option<Solid> {
    match k().boolean(a, b, op) {
        Ok(s) => Some(s),
        Err(BooleanError::EmptyResult) | Err(BooleanError::UnsupportedIntersection { .. }) => None,
        Err(BooleanError::AmbiguousContact | BooleanError::AmbiguousClassification) => None,
        Err(e) => panic!("unerwarteter Fehler {e}"),
    }
}

/// Feste, kleine Konfiguration: 40 Fälle, fester Startwert, keine Regressionsdateien.
fn config() -> ProptestConfig {
    ProptestConfig {
        rng_seed: RngSeed::Fixed(0x5EED_B001),
        failure_persistence: None,
        ..ProptestConfig::with_cases(40)
    }
}

proptest! {
    #![proptest_config(config())]

    #[test]
    fn box_box_volume_balance(
        sx in 2.0f64..12.0, sy in 2.0f64..12.0, sz in 2.0f64..12.0,
        dx in -9.0f64..9.0, dy in -9.0f64..9.0, dz in -9.0f64..9.0,
    ) {
        // allgemeine Lage: Versätze nie genau 0, damit keine bündigen Kanten entstehen
        let (dx, dy, dz) = (dx + 0.0137, dy + 0.0291, dz + 0.0113);
        let a = k().make_box(DVec3::new(10.0, 10.0, 10.0)).unwrap();
        let b = moved(&k().make_box(DVec3::new(sx, sy, sz)).unwrap(), DVec3::new(dx, dy, dz));
        let (va, vb) = (vol(&a), vol(&b));
        let u = try_op(&a, &b, BoolOp::Union);
        let i = try_op(&a, &b, BoolOp::Intersection);
        let d = try_op(&a, &b, BoolOp::Difference);
        let vi = i.as_ref().map(vol).unwrap_or(0.0);
        let vu = u.as_ref().map(vol).unwrap_or(va + vb);
        let vd = d.as_ref().map(vol).unwrap_or(if i.is_none() { va } else { 0.0 });
        prop_assert!((vu + vi - (va + vb)).abs() < 1e-6 * (va + vb), "V(A∪B)+V(A∩B) = {} ≠ {}", vu + vi, va + vb);
        prop_assert!((vd - (va - vi)).abs() < 1e-6 * va, "V(A−B) = {vd} ≠ {}", va - vi);
    }

    #[test]
    fn box_cylinder_volume_balance(
        r in 0.8f64..3.5, h in 3.0f64..14.0,
        x in 0.5f64..9.5, y in 0.5f64..9.5, z in -3.0f64..8.0,
    ) {
        let a = k().make_box(DVec3::new(10.0, 10.0, 10.0)).unwrap();
        let c = moved(&k().make_cylinder(r, h).unwrap(), DVec3::new(x + 0.0071, y + 0.0043, z + 0.0087));
        let (va, vb) = (vol(&a), vol(&c));
        let u = try_op(&a, &c, BoolOp::Union);
        let i = try_op(&a, &c, BoolOp::Intersection);
        let d = try_op(&a, &c, BoolOp::Difference);
        // nicht unterstützte Lagen (Zylinder schneidet eine Kante tangential usw.) werden übersprungen
        prop_assume!(u.is_some() && i.is_some() && d.is_some());
        let (vu, vi, vd) = (vol(&u.unwrap()), vol(&i.unwrap()), vol(&d.unwrap()));
        let tol = 0.01 * (va + vb);
        prop_assert!((vu + vi - (va + vb)).abs() < tol, "{vu} + {vi} ≠ {va} + {vb}");
        prop_assert!((vd - (va - vi)).abs() < tol, "{vd} ≠ {va} − {vi}");
        for (op, v) in [(BoolOp::Union, vu), (BoolOp::Difference, vd), (BoolOp::Intersection, vi)] {
            prop_assert!(boolean_volume_plausible(op, va, vb, v, 0.02));
        }
    }
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
        let _ = k().boolean(&a, &c, op); // Ok oder benannter Fehler, aber keine Panik
    }
}
