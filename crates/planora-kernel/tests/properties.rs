//! Eigenschaftstests (Spec §8) auf Kernel-Ebene: zufällige Platten mit Löchern.

use planora_kernel::*;
use planora_math::*;
use proptest::prelude::*;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// Rechteckplatte mit `holes` nebeneinanderliegenden Löchern (rund oder eckig):
    /// gültig, Geschlecht = Anzahl der Löcher.
    #[test]
    fn random_plates_have_genus_equal_to_hole_count(
        w in 10.0f64..100.0, h in 10.0f64..100.0, height in 0.5f64..50.0,
        holes in 0usize..4,
        size in 0.1f64..0.45,
        round in proptest::collection::vec(any::<bool>(), 4),
        ox in -50.0f64..50.0, oy in -50.0f64..50.0,
    ) {
        let origin = DVec2::new(ox, oy);
        let pitch = w / (holes as f64 + 1.0);
        // halbe Lochgröße: passt sicher zwischen die Nachbarn und in die Plattenhöhe
        let half = size * pitch.min(h);
        let hole_loops: Vec<ProfileLoop> = (0..holes)
            .map(|i| {
                let c = origin + DVec2::new(pitch * (i as f64 + 1.0), h / 2.0);
                if round[i] {
                    ProfileLoop::circle(c, half)
                } else {
                    ProfileLoop::rectangle(c - DVec2::splat(half), c + DVec2::splat(half))
                }
            })
            .collect();
        let profile = Profile {
            outer: ProfileLoop::rectangle(origin, origin + DVec2::new(w, h)),
            holes: hole_loops,
        };
        let solid = AnalyticKernel::default()
            .extrude(&profile, &Frame::WORLD, height)
            .unwrap();
        let report = validate(&solid).unwrap();
        prop_assert_eq!(report.genus, holes);
        prop_assert_eq!(report.inner_loops, 2 * holes);
    }
}
