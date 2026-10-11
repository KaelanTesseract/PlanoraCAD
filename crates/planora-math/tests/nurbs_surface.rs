use planora_math::DVec3;
use planora_math::nurbs::*;

/// Bikubische Beispielfläche mit ungleichen Gewichten.
fn sample() -> NurbsSurface {
    let kn = |n: usize| {
        let mut k = vec![0.0; 4];
        for i in 1..=(n - 4) {
            k.push(i as f64 / (n - 3) as f64);
        }
        k.extend([1.0; 4]);
        k
    };
    let (nu, nv) = (5, 6);
    let mut c = Vec::new();
    let mut w = Vec::new();
    for i in 0..nu {
        for j in 0..nv {
            c.push(DVec3::new(
                i as f64 * 2.0,
                j as f64 * 1.5,
                ((i * 3 + j * 5) % 7) as f64 * 0.3,
            ));
            w.push(1.0 + ((i + 2 * j) % 3) as f64 * 0.5);
        }
    }
    NurbsSurface::new(3, 3, kn(nu), kn(nv), nu, nv, c, w).unwrap()
}

#[test]
fn derivatives_match_finite_differences() {
    let s = sample();
    let h = 1e-6;
    for &(u, v) in &[(0.2, 0.3), (0.5, 0.5), (0.8, 0.1), (0.05, 0.95)] {
        let d = s.derivs(u, v, 2);
        let fu = (s.point(u + h, v) - s.point(u - h, v)) / (2.0 * h);
        let fv = (s.point(u, v + h) - s.point(u, v - h)) / (2.0 * h);
        let fuu = (s.point(u + h, v) - s.point(u, v) * 2.0 + s.point(u - h, v)) / (h * h);
        let fuv = (s.point(u + h, v + h) - s.point(u + h, v - h) - s.point(u - h, v + h)
            + s.point(u - h, v - h))
            / (4.0 * h * h);
        assert!(d[1][0].distance(fu) < 1e-4, "su");
        assert!(d[0][1].distance(fv) < 1e-4, "sv");
        assert!(d[2][0].distance(fuu) < 2e-1, "suu");
        assert!(d[1][1].distance(fuv) < 2e-1, "suv");
        assert!(d[0][0].distance(s.point(u, v)) < 1e-14);
    }
}

#[test]
fn the_normal_is_a_unit_vector_perpendicular_to_both_tangents() {
    let s = sample();
    for &(u, v) in &[(0.2, 0.3), (0.7, 0.6)] {
        let n = s.normal(u, v);
        let d = s.derivs(u, v, 1);
        assert!((n.length() - 1.0).abs() < 1e-12);
        assert!(n.dot(d[1][0]).abs() < 1e-9 && n.dot(d[0][1]).abs() < 1e-9);
    }
}

#[test]
fn knot_insertion_does_not_change_the_surface() {
    let s = sample();
    let t = s
        .insert_knot_u(0.37, 2)
        .unwrap()
        .insert_knot_v(0.61, 1)
        .unwrap();
    assert_eq!(t.counts(), (s.counts().0 + 2, s.counts().1 + 1));
    for i in 0..=10 {
        for j in 0..=10 {
            let (u, v) = (i as f64 / 10.0, j as f64 / 10.0);
            assert!(s.point(u, v).distance(t.point(u, v)) < 1e-12, "({u},{v})");
        }
    }
    assert!(s.insert_knot_u(0.37, 4).is_err(), "über Grad hinaus");
    assert!(s.insert_knot_u(2.0, 1).is_err(), "außerhalb");
}

#[test]
fn bezier_patches_cover_the_surface_and_their_boxes_contain_it() {
    let s = sample();
    let patches = s.bezier_patches();
    assert_eq!(patches.len(), (5 - 3) * (6 - 3)); // 2 × 3 Abschnitte
    for p in &patches {
        let (lo, hi) = p.bounds();
        for i in 0..=8 {
            for j in 0..=8 {
                let u = p.u.0 + (p.u.1 - p.u.0) * i as f64 / 8.0;
                let v = p.v.0 + (p.v.1 - p.v.0) * j as f64 / 8.0;
                let q = s.point(u, v);
                assert!(
                    q.cmpge(lo - DVec3::splat(1e-9)).all()
                        && q.cmple(hi + DVec3::splat(1e-9)).all()
                );
            }
        }
    }
}

#[test]
fn subdividing_a_patch_gives_four_patches_with_the_same_surface() {
    let s = sample();
    let p = &s.bezier_patches()[0];
    let kids = p.subdivide();
    let mut inside = 0;
    for k in &kids {
        assert!(k.flatness() <= p.flatness() + 1e-12, "wird nicht unebener");
        let (lo, hi) = k.bounds();
        let (pl, ph) = p.bounds();
        assert!(lo.cmpge(pl - DVec3::splat(1e-9)).all() && hi.cmple(ph + DVec3::splat(1e-9)).all());
        let u = 0.5 * (k.u.0 + k.u.1);
        let v = 0.5 * (k.v.0 + k.v.1);
        let q = s.point(u, v);
        assert!(q.cmpge(lo - DVec3::splat(1e-9)).all() && q.cmple(hi + DVec3::splat(1e-9)).all());
        inside += 1;
    }
    assert_eq!(inside, 4);
}

#[test]
fn hostile_data_is_rejected() {
    let k = vec![0.0, 0.0, 1.0, 1.0];
    let c = vec![DVec3::ZERO, DVec3::X, DVec3::Y, DVec3::new(1.0, 1.0, 0.0)];
    let ok = NurbsSurface::new(1, 1, k.clone(), k.clone(), 2, 2, c.clone(), vec![1.0; 4]);
    assert!(ok.is_ok());
    assert!(NurbsSurface::new(1, 1, k.clone(), k.clone(), 2, 2, c.clone(), vec![1.0; 3]).is_err());
    assert!(NurbsSurface::new(1, 1, k.clone(), k.clone(), 2, 3, c.clone(), vec![1.0; 4]).is_err());
    assert!(
        NurbsSurface::new(
            1,
            1,
            k.clone(),
            k.clone(),
            2,
            2,
            c.clone(),
            vec![1.0, 0.0, 1.0, 1.0]
        )
        .is_err()
    );
    let nan = vec![
        DVec3::ZERO,
        DVec3::X,
        DVec3::Y,
        DVec3::new(f64::NAN, 0.0, 0.0),
    ];
    assert!(NurbsSurface::new(1, 1, k.clone(), k.clone(), 2, 2, nan, vec![1.0; 4]).is_err());
    assert!(NurbsSurface::new(0, 1, k.clone(), k.clone(), 2, 2, c.clone(), vec![1.0; 4]).is_err());
    let big = MAX_CONTROL_POINTS + 1;
    assert!(
        NurbsSurface::new(
            1,
            1,
            k.clone(),
            k,
            big,
            2,
            vec![DVec3::ZERO; big * 2],
            vec![1.0; big * 2]
        )
        .is_err()
    );
}

// ---- Zusätzliche Tests (über den Plan hinaus) ----

#[test]
fn second_derivatives_match_differences_of_first_derivatives_tightly() {
    // Strengere Prüfung als oben: zweite Ableitungen gegen zentrale Differenzen der
    // (analytischen) ersten Ableitungen.
    let s = sample();
    let h = 1e-6;
    for &(u, v) in &[
        (0.2, 0.3),
        (0.5, 0.5),
        (0.8, 0.1),
        (0.05, 0.95),
        (0.62, 0.41),
    ] {
        let d = s.derivs(u, v, 2);
        let du = |a: f64, b: f64| s.derivs(a, b, 1);
        let suu = (du(u + h, v)[1][0] - du(u - h, v)[1][0]) / (2.0 * h);
        let svv = (du(u, v + h)[0][1] - du(u, v - h)[0][1]) / (2.0 * h);
        let suv = (du(u, v + h)[1][0] - du(u, v - h)[1][0]) / (2.0 * h);
        let svu = (du(u + h, v)[0][1] - du(u - h, v)[0][1]) / (2.0 * h);
        let scale = 1.0 + d[2][0].length().max(d[0][2].length()).max(d[1][1].length());
        assert!(d[2][0].distance(suu) < 1e-5 * scale, "suu");
        assert!(d[0][2].distance(svv) < 1e-5 * scale, "svv");
        assert!(d[1][1].distance(suv) < 1e-5 * scale, "suv");
        assert!(d[1][1].distance(svu) < 1e-5 * scale, "svu");
        assert_eq!(d[2][1], DVec3::ZERO);
        assert_eq!(d[1][2], DVec3::ZERO);
    }
}

/// Geklemmter Bézier-Knotenvektor über `[a, b]`.
fn bezier_knots(p: usize, a: f64, b: f64) -> Vec<f64> {
    let mut k = vec![a; p + 1];
    k.extend(std::iter::repeat_n(b, p + 1));
    k
}

/// Ein Bézier-Flicken als eigene NURBS-Fläche über seinem Parameterbereich.
fn patch_surface(p: &BezierPatch) -> NurbsSurface {
    NurbsSurface::new(
        p.degree_u,
        p.degree_v,
        bezier_knots(p.degree_u, p.u.0, p.u.1),
        bezier_knots(p.degree_v, p.v.0, p.v.1),
        p.degree_u + 1,
        p.degree_v + 1,
        p.control.clone(),
        p.weights.clone(),
    )
    .unwrap()
}

#[test]
fn bezier_patches_reproduce_the_surface_exactly() {
    let s = sample();
    for p in s.bezier_patches() {
        let ps = patch_surface(&p);
        for i in 0..=6 {
            for j in 0..=6 {
                let u = p.u.0 + (p.u.1 - p.u.0) * i as f64 / 6.0;
                let v = p.v.0 + (p.v.1 - p.v.0) * j as f64 / 6.0;
                assert!(ps.point(u, v).distance(s.point(u, v)) < 1e-12, "({u},{v})");
            }
        }
    }
}

#[test]
fn subdivided_patches_reproduce_the_surface_exactly() {
    let s = sample();
    let p = &s.bezier_patches()[4];
    for k in p.subdivide() {
        let ks = patch_surface(&k);
        for i in 0..=4 {
            for j in 0..=4 {
                let u = k.u.0 + (k.u.1 - k.u.0) * i as f64 / 4.0;
                let v = k.v.0 + (k.v.1 - k.v.0) * j as f64 / 4.0;
                assert!(ks.point(u, v).distance(s.point(u, v)) < 1e-12, "({u},{v})");
            }
        }
    }
}

#[test]
fn flatness_is_zero_for_planar_patches_and_does_not_grow_when_subdividing() {
    // Ebene, schräg im Raum, mit ungleichen Gewichten.
    let (nx, ny) = (DVec3::new(1.0, 0.3, -0.2), DVec3::new(-0.1, 1.0, 0.7));
    let mut c = Vec::new();
    let mut w = Vec::new();
    for i in 0..4 {
        for j in 0..4 {
            c.push(nx * i as f64 + ny * (j as f64 + 0.3 * (i * j) as f64));
            w.push(1.0 + ((i + j) % 2) as f64);
        }
    }
    let plane = BezierPatch {
        u: (0.0, 1.0),
        v: (0.0, 1.0),
        degree_u: 3,
        degree_v: 3,
        control: c,
        weights: w,
    };
    assert!(plane.flatness() < 1e-12, "{}", plane.flatness());
    // Wiederholtes Unterteilen der Beispielflicken: das Maß wächst nie.
    let s = sample();
    for p in s.bezier_patches() {
        let mut level = vec![p];
        for _ in 0..3 {
            let mut next = Vec::new();
            for q in &level {
                for k in q.subdivide() {
                    assert!(k.flatness() <= q.flatness() + 1e-12);
                    next.push(k);
                }
            }
            level = next;
        }
    }
}

#[test]
fn reversing_u_mirrors_the_parameter_and_flips_the_normal() {
    let s = sample();
    let r = s.reversed_u().unwrap(); // mechanisch: reversed_u liefert jetzt Result
    for &(u, v) in &[(0.2, 0.3), (0.7, 0.6)] {
        assert!(r.point(1.0 - u, v).distance(s.point(u, v)) < 1e-12);
        assert!((r.normal(1.0 - u, v) + s.normal(u, v)).length() < 1e-9);
    }
}

#[test]
fn evaluation_outside_the_domain_is_clamped_and_finite() {
    let s = sample();
    assert!(s.point(-1.0, 2.0).distance(s.point(0.0, 1.0)) < 1e-15);
    assert!(s.point(f64::NAN, f64::INFINITY).distance(s.point(0.0, 0.0)) < 1e-15);
    let d = s.derivs(f64::NAN, 0.5, 2);
    assert!(d.iter().flatten().all(|p| p.is_finite()));
    assert_eq!(s.derivs(0.5, 0.5, 7).len(), MAX_SURFACE_DERIV + 1);
    // Knoteneinfügen an einem vorhandenen Knoten und an den Rändern.
    assert!(s.insert_knot_u(0.5, 2).is_ok());
    assert!(
        s.insert_knot_u(0.5, 3).is_err(),
        "0.5 ist schon einfach vorhanden"
    );
    assert!(s.insert_knot_v(0.0, 1).is_err());
    assert!(s.insert_knot_v(f64::NAN, 1).is_err());
}

#[test]
fn a_degenerate_point_has_a_zero_normal() {
    // Bilinearer Flicken, dessen eine Kante auf einen Punkt zusammenfällt (Pol bei u = 0).
    let k = vec![0.0, 0.0, 1.0, 1.0];
    let c = vec![
        DVec3::ZERO,
        DVec3::ZERO,
        DVec3::X,
        DVec3::new(1.0, 1.0, 0.0),
    ];
    let s = NurbsSurface::new(1, 1, k.clone(), k, 2, 2, c, vec![1.0; 4]).unwrap();
    assert_eq!(s.normal(0.0, 0.5), DVec3::ZERO);
    assert!((s.normal(0.5, 0.5).length() - 1.0).abs() < 1e-12);
}

fn sample_curve() -> NurbsCurve {
    let k = vec![0.0, 0.0, 0.0, 0.0, 0.3, 0.6, 1.0, 1.0, 1.0, 1.0];
    let c = vec![
        DVec3::ZERO,
        DVec3::new(1.0, 2.0, 0.0),
        DVec3::new(2.0, -1.0, 1.0),
        DVec3::new(3.0, 0.5, 0.0),
        DVec3::new(4.0, 2.0, -1.0),
        DVec3::new(5.0, 0.0, 0.0),
    ];
    NurbsCurve::new(3, k, c, vec![1.0, 2.0, 0.5, 1.5, 1.0, 3.0]).unwrap()
}

#[test]
fn curve_knot_insertion_and_bezier_segments_keep_the_shape() {
    let c = sample_curve();
    let t = c.insert_knot(0.45, 3).unwrap();
    assert_eq!(t.control_points().len(), 9);
    assert!(c.insert_knot(0.3, 3).is_err());
    assert!(c.insert_knot(1.0, 1).is_err());
    let segs = c.bezier_segments();
    assert_eq!(segs.len(), 3);
    for i in 0..=50 {
        let u = i as f64 / 50.0;
        assert!(c.point(u).distance(t.point(u)) < 1e-12);
    }
    for s in &segs {
        let sc = NurbsCurve::new(
            s.degree,
            bezier_knots(s.degree, s.u.0, s.u.1),
            s.control.clone(),
            s.weights.clone(),
        )
        .unwrap();
        let (lo, hi) = s.bounds();
        for half in s.split_half() {
            let hc = NurbsCurve::new(
                half.degree,
                bezier_knots(half.degree, half.u.0, half.u.1),
                half.control.clone(),
                half.weights.clone(),
            )
            .unwrap();
            for i in 0..=10 {
                let u = half.u.0 + (half.u.1 - half.u.0) * i as f64 / 10.0;
                assert!(hc.point(u).distance(c.point(u)) < 1e-12);
            }
        }
        for i in 0..=10 {
            let u = s.u.0 + (s.u.1 - s.u.0) * i as f64 / 10.0;
            let q = c.point(u);
            assert!(sc.point(u).distance(q) < 1e-12);
            assert!(
                q.cmpge(lo - DVec3::splat(1e-9)).all() && q.cmple(hi + DVec3::splat(1e-9)).all()
            );
        }
    }
}
