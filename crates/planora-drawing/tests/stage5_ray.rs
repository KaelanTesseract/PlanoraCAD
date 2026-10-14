mod common;
use common::model::*;
use planora_drawing::{DrawBody, raycast::*};
use planora_exchange::Progress;
use planora_kernel::boolean::{Containment, face_geom, point_in_face};
use planora_kernel::{Face, Id};
use planora_math::{DVec3, Tolerance};

fn scene(s: &planora_kernel::Solid) -> Scene {
    let b = vec![DrawBody {
        name: "K".into(),
        solid: s,
    }];
    Scene::new(&b, 50.0, &Progress::new()).unwrap()
}

#[test]
fn ray_through_box_hits_two_faces_inside() {
    let s = box_solid(10.0, 20.0, 30.0);
    let sc = scene(&s);
    match sc.cast(DVec3::new(5.0, -5.0, 15.0), DVec3::Y, 0.0) {
        Ray::Hits(h) => {
            assert_eq!(h.len(), 2);
            assert!((h[0].s - 5.0).abs() < 1e-9 && (h[1].s - 25.0).abs() < 1e-9);
            assert!(h.iter().all(|x| x.class == HitClass::Inside));
        }
        Ray::Unsafe(r) => panic!("{r}"),
    }
}

#[test]
fn ray_missing_the_box_has_no_hits() {
    let s = box_solid(10.0, 20.0, 30.0);
    assert!(
        matches!(scene(&s).cast(DVec3::new(50.0, -5.0, 15.0), DVec3::Y, 0.0), Ray::Hits(h) if h.is_empty())
    );
}

#[test]
fn ray_through_an_edge_reports_boundary() {
    let s = box_solid(10.0, 20.0, 30.0);
    match scene(&s).cast(DVec3::new(0.0, -5.0, 15.0), DVec3::Y, 0.0) {
        Ray::Hits(h) => assert!(h.iter().any(|x| x.class == HitClass::Boundary)),
        Ray::Unsafe(_) => {} // beides ist zulässig, nie „Inside“ ohne Rand
    }
}

#[test]
fn ray_in_the_plane_of_a_face_is_unsafe() {
    let s = box_solid(10.0, 20.0, 30.0);
    // Strahl entlang +Y genau in der Ebene x = 0 (Fläche x = 0 reicht von y 0..20, z 0..30)
    assert!(matches!(
        scene(&s).cast(DVec3::new(0.0, -5.0, 15.0), DVec3::Y, 0.0),
        Ray::Unsafe(_)
    ));
    // dieselbe Richtung weit neben der Box ist harmlos
    assert!(
        matches!(scene(&s).cast(DVec3::new(-3.0, -5.0, 15.0), DVec3::Y, 0.0), Ray::Hits(h) if h.is_empty())
    );
}

#[test]
fn min_s_skips_the_own_surface() {
    let s = box_solid(10.0, 20.0, 30.0);
    // Start genau auf der Vorderfläche y=0, nach hinten: mit min_s = 1e-6 nur die hintere Fläche
    match scene(&s).cast(DVec3::new(5.0, 0.0, 15.0), DVec3::Y, 1e-6) {
        Ray::Hits(h) => {
            assert_eq!(h.len(), 1);
            assert!((h[0].s - 20.0).abs() < 1e-9);
        }
        Ray::Unsafe(r) => panic!("{r}"),
    }
}

fn hits(sc: &Scene, o: DVec3, d: DVec3) -> Vec<f64> {
    match sc.cast(o, d, 0.0) {
        Ray::Hits(h) => h.iter().map(|x| x.s).collect(),
        Ray::Unsafe(r) => panic!("{r}"),
    }
}

#[test]
fn quadric_hit_distances_match_formulas() {
    // Hilfskörper (tests/common/model.rs): Achse Z, Mittelpunkt des Bodens bzw. der Kugel im Ursprung.
    // Zylinder r=5, h=12: Strahl (-10,0,6) → +X trifft x=-5 und x=+5 → s = 5 und 15
    let c = cylinder_solid(5.0, 12.0);
    let h = hits(&scene(&c), DVec3::new(-10.0, 0.0, 6.0), DVec3::X);
    assert_eq!(h.len(), 2);
    assert!((h[0] - 5.0).abs() < 1e-9 && (h[1] - 15.0).abs() < 1e-9);
    // Kugel r=7 um den Ursprung: (-10,0,0) → +X: x=-7 und x=+7 → s = 3 und 17
    let k = sphere_solid(7.0);
    let h = hits(&scene(&k), DVec3::new(-10.0, 0.0, 0.0), DVec3::X);
    assert!((h[0] - 3.0).abs() < 1e-9 && (h[1] - 17.0).abs() < 1e-9);
    // Kegelstumpf cone_solid(r0=6, r1=2, h=10): Radius bei z=5 ist 6 - 4*5/10 = 4 → s = 6 und 14
    let kg = cone_solid(6.0, 2.0, 10.0);
    let h = hits(&scene(&kg), DVec3::new(-10.0, 0.0, 5.0), DVec3::X);
    assert!((h[0] - 6.0).abs() < 1e-9 && (h[1] - 14.0).abs() < 1e-9);
    // Torus torus_solid(major=10, minor=3), Achse Z: Strahl (-20,0,0) → +X trifft x=-13,-7,7,13 → s = 7,13,27,33
    let t = torus_solid(10.0, 3.0);
    let h = hits(&scene(&t), DVec3::new(-20.0, 0.0, 0.0), DVec3::X);
    assert_eq!(h.len(), 4);
    for (g, w) in h.iter().zip([7.0, 13.0, 27.0, 33.0]) {
        assert!((g - w).abs() < 1e-7, "{g} vs {w}");
    }
}

#[test]
fn freeform_face_ray_matches_surface_point() {
    // `nurbs_slab_solid()` (tests/common/model.rs): Platte mit einer Freiformwand. Punkt und
    // Normale in der Mitte der Wand aus der Symmetrie des Splines von Hand bestimmt; Strahl von
    // p + 5n in Richtung -n trifft die Fläche bei s = 5.
    let (solid, _face_index) = nurbs_slab_solid();
    let (p, n) = nurbs_slab_center_point_and_normal();
    let h = hits(&scene(&solid), p + n * 5.0, -n);
    assert!(h.iter().any(|s| (s - 5.0).abs() < 1e-8), "{h:?}");
}

#[test]
fn hits_are_sorted_by_s() {
    let c = cylinder_solid(5.0, 12.0);
    let th = 0.3_f64;
    let d = DVec3::new(th.cos(), th.sin(), 0.0);
    let h = hits(&scene(&c), DVec3::new(-10.0, -1.0, 6.0), d);
    assert_eq!(h.len(), 2);
    assert!(h[0] < h[1]);
}

// --- Ergänzungen über die Vorlage hinaus ---

#[test]
fn ray_tangent_to_cylinder_is_unsafe() {
    // Strahl (-10, 5, 6) → +X berührt den Mantel (r = 5) genau in (0, 5, 6): Doppelwurzel.
    let c = cylinder_solid(5.0, 12.0);
    assert!(matches!(
        scene(&c).cast(DVec3::new(-10.0, 5.0, 6.0), DVec3::X, 0.0),
        Ray::Unsafe(_)
    ));
    // Ein Stück daneben (y = 5,5) verfehlt er den Mantel sicher.
    assert!(matches!(
        scene(&c).cast(DVec3::new(-10.0, 5.5, 6.0), DVec3::X, 0.0),
        Ray::Hits(h) if h.is_empty()
    ));
}

#[test]
fn ray_touching_torus_is_unsafe() {
    // Strahl in der Höhe z = minor = 3 entlang +X berührt den Torus oben bei x = ±10.
    let t = torus_solid(10.0, 3.0);
    assert!(matches!(
        scene(&t).cast(DVec3::new(-20.0, 0.0, 3.0), DVec3::X, 0.0),
        Ray::Unsafe(_)
    ));
    // Bei z = 3,5 verfehlt er den Torus.
    assert!(matches!(
        scene(&t).cast(DVec3::new(-20.0, 0.0, 3.5), DVec3::X, 0.0),
        Ray::Hits(h) if h.is_empty()
    ));
    // Bei z = 2,9 schneidet er das Rohr viermal: |x| = 10 ± √(9 − 2,9²).
    let w = (9.0f64 - 2.9 * 2.9).sqrt();
    let h = hits(&scene(&t), DVec3::new(-20.0, 0.0, 2.9), DVec3::X);
    assert_eq!(h.len(), 4, "{h:?}");
    for (g, x) in h.iter().zip([-10.0 - w, -10.0 + w, 10.0 - w, 10.0 + w]) {
        assert!((g - (x + 20.0)).abs() < 1e-7, "{g} vs {}", x + 20.0);
    }
}

#[test]
fn ray_hits_only_the_trimmed_part_of_a_surface() {
    // Halbzylinder r = 5 (y ≥ 0), h = 8: Strahl (3, −10, 4) → +Y liegt in der Hüllbox der
    // Mantelfläche, trifft den vollen Zylinder aber auch bei y = −4 (s = 6), wo die Fläche
    // fehlt. Echte Treffer: Ebene y = 0 bei s = 10, Mantel bei y = √(25 − 9) = 4, s = 14.
    let hc = half_cylinder_solid(5.0, 8.0);
    let h = hits(&scene(&hc), DVec3::new(3.0, -10.0, 4.0), DVec3::Y);
    assert_eq!(h.len(), 2, "{h:?}");
    assert!(
        (h[0] - 10.0).abs() < 1e-9 && (h[1] - 14.0).abs() < 1e-9,
        "{h:?}"
    );
    // Untere Halbkugel r = 7: Strahl bei z = 3 trifft nur die (fehlende) obere Hälfte.
    let lh = lower_hemisphere_solid(7.0);
    let h = hits(&scene(&lh), DVec3::new(-10.0, 0.0, 3.0), DVec3::X);
    assert!(h.is_empty(), "{h:?}");
    // Bei z = −3 trifft er sie zweimal: |x| = √(49 − 9) = √40.
    let w = 40f64.sqrt();
    let h = hits(&scene(&lh), DVec3::new(-10.0, 0.0, -3.0), DVec3::X);
    assert_eq!(h.len(), 2, "{h:?}");
    assert!((h[0] - (10.0 - w)).abs() < 1e-9 && (h[1] - (10.0 + w)).abs() < 1e-9);
}

#[test]
fn dir_is_normalised() {
    // Nicht normierte Richtung: s bleibt die Länge entlang des Strahls.
    let s = box_solid(10.0, 20.0, 30.0);
    let h = hits(&scene(&s), DVec3::new(5.0, -5.0, 15.0), DVec3::Y * 4.0);
    assert_eq!(h.len(), 2);
    assert!((h[0] - 5.0).abs() < 1e-9 && (h[1] - 25.0).abs() < 1e-9);
}

#[test]
fn hit_classes_agree_with_point_in_face() {
    // Unabhängige Probe: Für viele schräge Strahlen stimmt die Klasse jedes Treffers mit
    // `point_in_face` des Kernels am Trefferpunkt überein (Toleranz 16 · geom_tol(50)).
    let tol = Tolerance {
        linear: 16.0 * (64.0 * f64::EPSILON * 50.0f64).max(1e-9),
        ..Tolerance::default()
    };
    let solids = [
        box_solid(10.0, 20.0, 30.0),
        cylinder_solid(5.0, 12.0),
        cone_solid(6.0, 2.0, 10.0),
        sphere_solid(7.0),
        torus_solid(10.0, 3.0),
        half_cylinder_solid(5.0, 8.0),
        nurbs_slab_solid().0,
    ];
    let mut checked = 0;
    for solid in &solids {
        let sc = scene(solid);
        for i in 0..40 {
            let a = 0.37 * i as f64;
            let o = DVec3::new(-30.0 * a.cos(), -30.0 * a.sin(), 3.0 + 0.2 * i as f64);
            let d = (DVec3::new(2.0 * a.cos(), 2.0 * a.sin(), 0.13) + DVec3::new(0.0, 0.1, 0.0))
                .normalize();
            let Ray::Hits(h) = sc.cast(o, d, 0.0) else {
                continue;
            };
            for x in h {
                let fg = face_geom(solid, Id::<Face>::from_index(x.face));
                let want = point_in_face(&fg, o + d * x.s, &tol);
                let got = match x.class {
                    HitClass::Inside => Containment::Inside,
                    HitClass::Boundary => Containment::Boundary,
                };
                assert_eq!(got, want, "Fläche {} bei s = {}", x.face, x.s);
                checked += 1;
            }
        }
    }
    assert!(checked > 100, "zu wenige Treffer geprüft: {checked}");
}
