//! Radius entlang einer Kette (Teilprojekt 8, Task 14).

use planora_math::{RadiusLaw, RadiusLawError};

#[test]
fn the_law_passes_its_points_without_overshoot() {
    let pts = [(0.0, 1.0), (0.3, 3.0), (0.5, 2.9), (1.0, 0.5)];
    let law = RadiusLaw::new(&pts).unwrap();
    assert_eq!(law.positions(), &[0.0, 0.3, 0.5, 1.0]);
    for (s, r) in pts {
        assert!((law.radius(s) - r).abs() < 1e-15);
    }
    for w in pts.windows(2) {
        let (lo, hi) = (w[0].1.min(w[1].1), w[0].1.max(w[1].1));
        for i in 0..=1000 {
            let s = w[0].0 + (w[1].0 - w[0].0) * i as f64 / 1000.0;
            let r = law.radius(s);
            assert!(r >= lo - 1e-12 && r <= hi + 1e-12, "s={s}: {r}");
        }
    }
    assert_eq!(law.radius(-1.0), 1.0);
    assert_eq!(law.radius(2.0), 0.5);
    assert_eq!(law.radius(f64::NAN), 1.0);
    let h = 1e-7;
    for s in [0.3, 0.5] {
        let left = (law.radius(s) - law.radius(s - h)) / h;
        let right = (law.radius(s + h) - law.radius(s)) / h;
        assert!((left - right).abs() < 1e-4, "s={s}: {left} / {right}");
    }
}

#[test]
fn hostile_points_are_rejected() {
    assert_eq!(
        RadiusLaw::new(&[(0.0, 1.0)]),
        Err(RadiusLawError::TooFewPoints)
    );
    assert_eq!(
        RadiusLaw::new(&[(0.5, 1.0), (0.5, 2.0)]),
        Err(RadiusLawError::Positions)
    );
    assert_eq!(
        RadiusLaw::new(&[(0.0, 1.0), (f64::NAN, 2.0)]),
        Err(RadiusLawError::Positions)
    );
    assert_eq!(
        RadiusLaw::new(&[(0.0, 1.0), (1.0, 0.0)]),
        Err(RadiusLawError::Radii)
    );
    assert_eq!(
        RadiusLaw::new(&[(0.0, 1.0), (1.0, f64::INFINITY)]),
        Err(RadiusLawError::Radii)
    );
}

// ---- Zusätzliche Tests (über den Plan hinaus) ----

/// Einfacher Zufallsgenerator mit festem Startwert (wie in den Kernel-Tests).
struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> f64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (self.0 >> 11) as f64 / (1u64 << 53) as f64
    }
}

#[test]
fn two_points_give_a_straight_line_and_three_match_hand_computed_slopes() {
    let law = RadiusLaw::new(&[(0.0, 1.0), (1.0, 2.0)]).unwrap();
    for i in 0..=100 {
        let s = i as f64 / 100.0;
        assert!((law.radius(s) - (1.0 + s)).abs() < 1e-15, "{s}");
    }
    // (0, 1), (0,5, 2), (1, 1,5): Steigungen 2 und −1 wechseln das Vorzeichen, also m₁ = 0;
    // an den Enden m₀ = 2, m₂ = −1. Hermite-Formel von Hand.
    let law = RadiusLaw::new(&[(0.0, 1.0), (0.5, 2.0), (1.0, 1.5)]).unwrap();
    let hermite = |t: f64, h: f64, r0: f64, m0: f64, r1: f64, m1: f64| {
        let (t2, t3) = (t * t, t * t * t);
        (2.0 * t3 - 3.0 * t2 + 1.0) * r0
            + (t3 - 2.0 * t2 + t) * h * m0
            + (-2.0 * t3 + 3.0 * t2) * r1
            + (t3 - t2) * h * m1
    };
    for i in 0..=200 {
        let s = i as f64 / 200.0;
        let want = if s <= 0.5 {
            hermite(s / 0.5, 0.5, 1.0, 2.0, 2.0, 0.0)
        } else {
            hermite((s - 0.5) / 0.5, 0.5, 2.0, 0.0, 1.5, -1.0)
        };
        assert!((law.radius(s) - want).abs() < 1e-14, "{s}");
    }
    // Gleiche Radien: konstant, auch zwischen den Punkten.
    let flat = RadiusLaw::new(&[(0.0, 2.0), (0.4, 2.0), (1.0, 2.0)]).unwrap();
    for i in 0..=100 {
        assert_eq!(flat.radius(i as f64 / 100.0), 2.0);
    }
    assert_eq!(flat.radii(), &[2.0, 2.0, 2.0]);
}

#[test]
fn random_laws_stay_between_neighbouring_radii_and_are_smooth() {
    let mut g = Lcg(0x5EED_0014);
    for case in 0..200 {
        // 2 bis 64 Punkte mit Abständen von mindestens etwa 0,2 / (1,2 · n) im Bereich [0, 1].
        let n = 2 + (g.next() * 62.999) as usize;
        let inc: Vec<f64> = (0..n - 1).map(|_| 0.2 + g.next()).collect();
        let total: f64 = inc.iter().sum();
        let mut s = vec![0.0];
        for d in &inc {
            s.push(s.last().unwrap() + d / total);
        }
        let pts: Vec<(f64, f64)> = s.iter().map(|&x| (x, 0.01 + 5.0 * g.next())).collect();
        let law = RadiusLaw::new(&pts).unwrap();
        for w in pts.windows(2) {
            let (lo, hi) = (w[0].1.min(w[1].1), w[0].1.max(w[1].1));
            let mut prev = w[0].1;
            let rising = w[1].1 >= w[0].1;
            for i in 0..=50 {
                let x = w[0].0 + (w[1].0 - w[0].0) * i as f64 / 50.0;
                let r = law.radius(x);
                let slack = 1e-12 * hi;
                assert!(
                    r >= lo - slack && r <= hi + slack,
                    "Fall {case}, x={x}: {r}"
                );
                // Monoton je Stück.
                if rising {
                    assert!(r >= prev - slack, "Fall {case}, x={x}");
                } else {
                    assert!(r <= prev + slack, "Fall {case}, x={x}");
                }
                prev = r;
            }
        }
        // Stetig differenzierbar an den inneren Punkten (einseitige Ableitungen gleich;
        // Richardson-Schritt hebt den Fehler erster Ordnung der Differenzenquotienten auf).
        for w in pts.windows(3) {
            let x = w[1].0;
            let h = 1e-4 * (w[1].0 - w[0].0).min(w[2].0 - w[1].0);
            let one = |sgn: f64, h: f64| (law.radius(x + sgn * h) - law.radius(x)) / (sgn * h);
            let side = |sgn: f64| 2.0 * one(sgn, 0.5 * h) - one(sgn, h);
            let (left, right) = (side(-1.0), side(1.0));
            let scale = 1.0 + left.abs().max(right.abs());
            assert!(
                (left - right).abs() < 1e-3 * scale,
                "Fall {case}, x={x}: {left} / {right}"
            );
        }
    }
}

#[test]
fn degenerate_slopes_are_rejected_not_infinite() {
    // Punktabstand unterläuft: die Steigung wäre unendlich.
    assert_eq!(
        RadiusLaw::new(&[(0.0, 1.0), (1e-320, 2.0)]),
        Err(RadiusLawError::Positions)
    );
    assert_eq!(
        RadiusLaw::new(&[(0.0, 1.0), (-0.5, 2.0)]),
        Err(RadiusLawError::Positions)
    );
    assert_eq!(
        RadiusLaw::new(&[(0.0, 1.0), (1.0, -1.0)]),
        Err(RadiusLawError::Radii)
    );
    assert_eq!(
        RadiusLaw::new(&[(0.0, f64::NAN), (1.0, 1.0)]),
        Err(RadiusLawError::Radii)
    );
    assert_eq!(RadiusLaw::new(&[]), Err(RadiusLawError::TooFewPoints));
}
