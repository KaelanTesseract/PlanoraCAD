mod common;
use common::model::*;
use planora_drawing::limits::geom_tol;
use planora_drawing::{DrawBody, raycast::*};
use planora_exchange::Progress;
use planora_kernel::boolean::{Containment, face_geom, point_in_face, points_to_uv, winding};
use planora_kernel::{Face, Id};
use planora_math::{DVec2, DVec3, Tolerance};

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

fn scene_e(s: &planora_kernel::Solid, extent: f64) -> Scene {
    let b = vec![DrawBody {
        name: "K".into(),
        solid: s,
    }];
    Scene::new(&b, extent, &Progress::new()).unwrap()
}

fn tol_for(extent: f64) -> Tolerance {
    Tolerance {
        linear: 16.0 * geom_tol(extent),
        ..Tolerance::default()
    }
}

// --- Fix-Runde 1: Einordnung nahe gekrümmter Ränder (Polygon tastet Bögen nur alle 5° ab) ---

#[test]
fn rays_just_inside_a_convex_rim_are_not_lost() {
    // Zylinder r = 5, h = 12; senkrechte Strahlen 1e-3 … 1e-4 innerhalb bzw. außerhalb des
    // Randkreises, jeweils genau zwischen zwei Polygonpunkten (2,5° + k·5°), wo das 5°-Polygon
    // um 5·(1 − cos 2,5°) ≈ 4,8e-3 nach innen abweicht.
    let c = cylinder_solid(5.0, 12.0);
    let sc = scene(&c);
    for k in 0..72 {
        let th = (2.5 + 5.0 * k as f64).to_radians();
        let radial = DVec3::new(th.cos(), th.sin(), 0.0);
        for dlt in [1e-3, 3e-4, 1e-4] {
            let o = radial * (5.0 - dlt) - DVec3::Z * 5.0;
            match sc.cast(o, DVec3::Z, 0.0) {
                Ray::Hits(h) => {
                    assert_eq!(h.len(), 2, "innen, {k}, {dlt}: {h:?}");
                    assert!((h[0].s - 5.0).abs() < 1e-9 && (h[1].s - 17.0).abs() < 1e-9);
                    assert!(h.iter().all(|x| x.class == HitClass::Inside), "{h:?}");
                }
                Ray::Unsafe(r) => panic!("innen, {k}, {dlt}: {r}"),
            }
            let o = radial * (5.0 + dlt) - DVec3::Z * 5.0;
            assert!(
                matches!(sc.cast(o, DVec3::Z, 0.0), Ray::Hits(ref h) if h.is_empty()),
                "außen, {k}, {dlt}: {:?}",
                sc.cast(o, DVec3::Z, 0.0)
            );
        }
    }
}

#[test]
fn rays_just_inside_a_hole_rim_hit_no_material() {
    // Platte 200 × 200 × 10 mit Rundloch r = 20 (Ausdehnung 200): 1e-3 innerhalb des Lochrands
    // (zwischen zwei Polygonpunkten) kein Treffer, 1e-3 außerhalb Boden und Deckel.
    let p = plate_with_hole_solid(100.0, 20.0, 10.0);
    let sc = scene_e(&p, 200.0);
    for k in 0..72 {
        let th = (2.5 + 5.0 * k as f64).to_radians();
        let radial = DVec3::new(th.cos(), th.sin(), 0.0);
        let o = radial * (20.0 - 1e-3) - DVec3::Z * 5.0;
        assert!(
            matches!(sc.cast(o, DVec3::Z, 0.0), Ray::Hits(ref h) if h.is_empty()),
            "im Loch, {k}: {:?}",
            sc.cast(o, DVec3::Z, 0.0)
        );
        let o = radial * (20.0 + 1e-3) - DVec3::Z * 5.0;
        match sc.cast(o, DVec3::Z, 0.0) {
            Ray::Hits(h) => {
                assert_eq!(h.len(), 2, "im Material, {k}: {h:?}");
                assert!((h[0].s - 5.0).abs() < 1e-9 && (h[1].s - 15.0).abs() < 1e-9);
                assert!(h.iter().all(|x| x.class == HitClass::Inside), "{h:?}");
            }
            Ray::Unsafe(r) => panic!("im Material, {k}: {r}"),
        }
    }
}

#[test]
fn ray_exactly_through_the_rim_is_boundary() {
    // Schräger Strahl durch den Randpunkt q des Bodenkreises (32,5°): alle Treffer bei s = 5
    // (Boden und Mantel) sind Rand, keiner innen.
    let c = cylinder_solid(5.0, 12.0);
    let th = 32.5f64.to_radians();
    let q = DVec3::new(5.0 * th.cos(), 5.0 * th.sin(), 0.0);
    let d = DVec3::new(-0.3 * th.cos(), -0.3 * th.sin(), 1.0).normalize();
    match scene(&c).cast(q - d * 5.0, d, 0.0) {
        Ray::Hits(h) => {
            let at_q: Vec<_> = h.iter().filter(|x| (x.s - 5.0).abs() < 1e-6).collect();
            assert!(!at_q.is_empty(), "{h:?}");
            assert!(at_q.iter().all(|x| x.class == HitClass::Boundary), "{h:?}");
        }
        Ray::Unsafe(r) => panic!("{r}"),
    }
}

/// Unabhängige Einordnung mit dichten Randpolygonen (zusammen etwa 100 000 Punkte je
/// Schleife) und Kernel-Windungszahl; Randabstand ≤ `tol` → Rand.
fn brute_class(solid: &planora_kernel::Solid, face: usize, p: DVec3, tol: f64) -> Containment {
    let fg = face_geom(solid, Id::<Face>::from_index(face));
    if fg.loops.iter().flatten().any(|s| s.closest(p).1 <= tol) {
        return Containment::Boundary;
    }
    let (u, v) = fg.surface.project(p);
    let shifts = |t: Option<f64>| t.map_or(vec![0.0], |t| vec![-t, 0.0, t]);
    let mut total = 0;
    for lp in &fg.loops {
        let per = (100_000 / lp.len()).max(64);
        let pts: Vec<DVec3> = lp
            .iter()
            .flat_map(|s| (0..per).map(move |k| s.at(k as f64 / per as f64)))
            .collect();
        let poly = points_to_uv(&fg.surface, fg.same_sense, &pts);
        for du in shifts(fg.surface.u_period()) {
            for dv in shifts(fg.surface.v_period()) {
                total += winding(&poly, DVec2::new(u + du, v + dv));
            }
        }
    }
    let sense = if fg.same_sense { 1 } else { -1 };
    if total * sense >= 1 {
        Containment::Inside
    } else {
        Containment::Outside
    }
}

#[test]
fn hit_classes_agree_with_point_in_face() {
    // Unabhängige Probe: Die Klasse jedes Treffers stimmt mit `point_in_face` des Kernels
    // überein – außer nahe gekrümmter Randkurven (Abstand ≤ 0,15 = 3e-3·E), wo dessen
    // 5°-Polygon ungenau ist; dort entscheidet eine dichte Polygon-Einordnung.
    let extent = 50.0;
    let tol = tol_for(extent);
    let zone = 3e-3 * extent;
    let solids = [
        box_solid(10.0, 20.0, 30.0),
        cylinder_solid(5.0, 12.0),
        cone_solid(6.0, 2.0, 10.0),
        sphere_solid(7.0),
        torus_solid(10.0, 3.0),
        half_cylinder_solid(5.0, 8.0),
        lower_hemisphere_solid(7.0),
        nurbs_slab_solid().0,
    ];
    // Senkrechte Strahlen dicht an den Randkreisen (Radien 5, 6, 2, 7).
    let mut near_rim: Vec<(DVec3, DVec3)> = Vec::new();
    for rim in [5.0, 6.0, 2.0, 7.0] {
        for k in 0..12 {
            let th = (2.5 + 17.5 * k as f64).to_radians();
            for rel in [-1.5e-3, -6e-4, -1e-4, 1e-4, 6e-4, 1.5e-3] {
                let rho: f64 = rim * (1.0 + rel);
                near_rim.push((DVec3::new(rho * th.cos(), rho * th.sin(), -20.0), DVec3::Z));
            }
        }
    }
    let (mut checked, mut in_zone) = (0, 0);
    for solid in &solids {
        let sc = scene(solid);
        let mut rays: Vec<(DVec3, DVec3)> = (0..40)
            .map(|i| {
                let a = 0.37 * i as f64;
                let o = DVec3::new(-30.0 * a.cos(), -30.0 * a.sin(), 3.0 + 0.2 * i as f64);
                let d = DVec3::new(2.0 * a.cos(), 2.0 * a.sin() + 0.1, 0.13).normalize();
                (o, d)
            })
            .collect();
        rays.extend(near_rim.iter().copied());
        for (o, d) in rays {
            let Ray::Hits(h) = sc.cast(o, d, 0.0) else {
                continue;
            };
            for x in h {
                let p = o + d * x.s;
                let fg = face_geom(solid, Id::<Face>::from_index(x.face));
                let curved_dist = fg
                    .loops
                    .iter()
                    .flatten()
                    .filter(|s| !matches!(s.curve, planora_kernel::Curve::Line { .. }))
                    .map(|s| s.closest(p).1)
                    .fold(f64::INFINITY, f64::min);
                let rim_dist = fg
                    .loops
                    .iter()
                    .flatten()
                    .map(|s| s.closest(p).1)
                    .fold(f64::INFINITY, f64::min);
                let got = match x.class {
                    HitClass::Inside => Containment::Inside,
                    HitClass::Boundary => Containment::Boundary,
                };
                let want = if curved_dist <= zone {
                    // Zwischen tol und 1e-6 ist auch das dichte Polygon nicht genau genug.
                    if rim_dist > tol.linear && rim_dist < 1e-6 {
                        continue;
                    }
                    in_zone += 1;
                    brute_class(solid, x.face, p, tol.linear)
                } else {
                    point_in_face(&fg, p, &tol)
                };
                assert_eq!(got, want, "Fläche {} bei s = {}, p = {p}", x.face, x.s);
                checked += 1;
            }
        }
    }
    assert!(checked > 300, "zu wenige Treffer geprüft: {checked}");
    assert!(in_zone > 50, "zu wenige Treffer nahe am Rand: {in_zone}");
}

// --- Fix-Runde 1: Doppelwurzel relativ zum Wurzelabstand ---

#[test]
fn nearly_axial_ray_finds_the_mantle_exit() {
    // Zylinder r = 5: Strahl vom Boden dicht am Rand, um 1e-4 rad quer zur Achse gekippt
    // (quer zur Radialrichtung, also Wurzeln symmetrisch um s = 0): a = sin²θ ≈ 1e-8, winzige
    // Diskriminante, aber Wurzeln bei s = ±6. Aufbau so, dass x(6) = 6·sinθ und
    // ρ(6)² = ρ0² + 36·sin²θ = 25.
    let th = 1e-4f64;
    let rho0 = (25.0 - (6.0 * th.sin()).powi(2)).sqrt();
    let o = DVec3::new(0.0, rho0, 0.0);
    let d = DVec3::new(th.sin(), 0.0, th.cos());
    let c = cylinder_solid(5.0, 12.0);
    match scene(&c).cast(o, d, 1e-6) {
        Ray::Hits(h) => {
            assert_eq!(h.len(), 1, "{h:?}");
            assert!((h[0].s - 6.0).abs() < 1e-5, "{h:?}");
            assert_eq!(h[0].class, HitClass::Inside);
        }
        Ray::Unsafe(r) => panic!("{r}"),
    }
}

#[test]
fn grazing_ray_with_a_root_past_min_s_is_unsafe() {
    // Kugel r = 7: Sehne der halben Länge 2e-5 (Doppelwurzel nach Wurzelabstand), Sehnenmitte
    // 1e-5 vor min_s = 1, die hintere Wurzel also 1e-5 hinter min_s: unsicher, nicht leer.
    let rho = (49.0f64 - 4e-10).sqrt();
    let o = DVec3::new(-(1.0 - 1e-5), rho, 0.0);
    let k = sphere_solid(7.0);
    let r = scene(&k).cast(o, DVec3::X, 1.0);
    assert!(matches!(r, Ray::Unsafe(_)), "{r:?}");
}

#[test]
fn ray_nearly_along_a_cone_generator_finds_its_crossings() {
    // Kegelstumpf r0 = 6, r1 = 2, h = 10. Strahl fast entlang der Mantellinie durch R = (0,6,0),
    // um 1e-4 rad tangential gekippt, Ursprung 3e-8 nach innen versetzt: zwei Schnitte weit
    // auseinander bei winziger Diskriminante. Erwartung aus eigener Nullstellensuche von
    // ρ(s) − (6 − 0,4·z(s)) (Abtastung + Bisektion), nur Punkte mit 0 ≤ z ≤ 10.
    let th = 1e-4f64;
    let r = DVec3::new(0.0, 6.0, 0.0);
    let g = DVec3::new(0.0, -4.0, 10.0).normalize();
    let d = g * th.cos() + DVec3::X * th.sin();
    let n = DVec3::new(0.0, 1.0, 0.4).normalize();
    let o = r - n * 3e-8;
    let f = |s: f64| {
        let p = o + d * s;
        p.x.hypot(p.y) - (6.0 - 0.4 * p.z)
    };
    let mut want = Vec::new();
    let (lo, hi, m) = (1e-6, 30.0, 300_000);
    let step = (hi - lo) / m as f64;
    for i in 0..m {
        let (mut a, mut b) = (lo + step * i as f64, lo + step * (i + 1) as f64);
        if f(a) * f(b) >= 0.0 {
            continue;
        }
        for _ in 0..200 {
            let mid = 0.5 * (a + b);
            if f(mid) * f(a) <= 0.0 {
                b = mid;
            } else {
                a = mid;
            }
        }
        let s = 0.5 * (a + b);
        let z = (o + d * s).z;
        if (0.0..=10.0).contains(&z) {
            want.push(s);
        }
    }
    assert!(want.iter().any(|s| *s > 1.0), "Aufbau: {want:?}");
    let kg = cone_solid(6.0, 2.0, 10.0);
    match scene(&kg).cast(o, d, 1e-6) {
        Ray::Hits(h) => {
            let mantle: Vec<f64> = h
                .iter()
                .filter(|x| {
                    let p = o + d * x.s;
                    p.z > 1e-6 && p.z < 10.0 - 1e-6
                })
                .map(|x| x.s)
                .collect();
            assert_eq!(mantle.len(), want.len(), "{h:?} / {want:?}");
            for (g, w) in mantle.iter().zip(&want) {
                assert!((g - w).abs() < 1e-6, "{g} vs {w}");
            }
        }
        Ray::Unsafe(r) => panic!("{r}"),
    }
}
