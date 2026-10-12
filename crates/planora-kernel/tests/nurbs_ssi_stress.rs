use planora_kernel::Surface;
use planora_kernel::boolean::{NURBS_FIT_TOL, SsiOutcome, intersect_nurbs};
use planora_math::nurbs::{NurbsCurve, NurbsSurface};
use planora_math::{DMat3, DVec3, Frame, PI, TAU, Tolerance};

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> f64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (self.0 >> 11) as f64 / (1u64 << 53) as f64
    }
    fn range(&mut self, a: f64, b: f64) -> f64 {
        a + (b - a) * self.next()
    }
    fn unit(&mut self) -> DVec3 {
        loop {
            let v = DVec3::new(
                self.range(-1.0, 1.0),
                self.range(-1.0, 1.0),
                self.range(-1.0, 1.0),
            );
            if v.length() > 0.2 && v.length() < 1.0 {
                return v.normalize();
            }
        }
    }
}

fn place(rng: &mut Rng, s: NurbsSurface, pu: bool, pv: bool) -> Surface {
    let rot = DMat3::from_axis_angle(rng.unit(), rng.range(0.0, TAU));
    let t = DVec3::new(
        rng.range(-1.0, 1.0),
        rng.range(-1.0, 1.0),
        rng.range(-1.0, 1.0),
    );
    Surface::nurbs(s.transformed(|p| rot * p + t).unwrap(), pu, pv).unwrap()
}

fn nurbs_kind(rng: &mut Rng, k: usize) -> (String, Surface) {
    match k % 6 {
        0 => {
            let (r, h) = (rng.range(1.0, 3.0), rng.range(4.0, 10.0));
            let l = NurbsCurve::new(
                1,
                vec![0.0, 0.0, 1.0, 1.0],
                vec![DVec3::new(r, 0.0, -h / 2.0), DVec3::new(r, 0.0, h / 2.0)],
                vec![1.0; 2],
            )
            .unwrap();
            let s =
                NurbsSurface::revolve(&l, DVec3::ZERO, DVec3::Z, rng.range(0.0, TAU), TAU).unwrap();
            ("cyl".into(), place(rng, s, true, false))
        }
        1 => {
            let (r0, r1, h) = (
                rng.range(0.5, 2.0),
                rng.range(1.0, 3.5),
                rng.range(3.0, 8.0),
            );
            let l = NurbsCurve::new(
                1,
                vec![0.0, 0.0, 1.0, 1.0],
                vec![DVec3::new(r0, 0.0, -h / 2.0), DVec3::new(r1, 0.0, h / 2.0)],
                vec![1.0; 2],
            )
            .unwrap();
            let s =
                NurbsSurface::revolve(&l, DVec3::ZERO, DVec3::Z, rng.range(0.0, TAU), TAU).unwrap();
            ("cone".into(), place(rng, s, true, false))
        }
        2 => {
            let r = rng.range(1.0, 3.0);
            let c =
                NurbsCurve::elliptic_arc(DVec3::ZERO, DVec3::X * r, DVec3::Z * r, -PI / 2.0, PI)
                    .unwrap();
            let s =
                NurbsSurface::revolve(&c, DVec3::ZERO, DVec3::Z, rng.range(0.0, TAU), TAU).unwrap();
            ("sph".into(), place(rng, s, true, false))
        }
        3 => {
            let (big, r) = (rng.range(2.0, 4.0), rng.range(0.5, 1.2));
            let c = NurbsCurve::elliptic_arc(
                DVec3::X * big,
                DVec3::X * r,
                DVec3::Z * r,
                rng.range(0.0, TAU),
                TAU,
            )
            .unwrap();
            let s =
                NurbsSurface::revolve(&c, DVec3::ZERO, DVec3::Z, rng.range(0.0, TAU), TAU).unwrap();
            ("tor".into(), place(rng, s, true, true))
        }
        5 => {
            // Welle: 40 × 40 Kontrollpunkte über 6 × 6 mm, Höhe ±0,3
            let n = 40usize;
            let mut kn = vec![0.0; 4];
            for i in 1..=(n - 4) {
                kn.push(i as f64 / (n - 3) as f64);
            }
            kn.extend([1.0; 4]);
            let (fx, fy) = (rng.range(0.3, 1.2), rng.range(0.3, 1.2));
            let mut c = Vec::new();
            for i in 0..n {
                for j in 0..n {
                    let (x, y) = (i as f64 * 6.0 / 39.0 - 3.0, j as f64 * 6.0 / 39.0 - 3.0);
                    c.push(DVec3::new(
                        x,
                        y,
                        0.3 * (fx * x * 3.0).sin() * (fy * y * 3.0).cos(),
                    ));
                }
            }
            let s = NurbsSurface::new(3, 3, kn.clone(), kn, n, n, c, vec![1.0; n * n]).unwrap();
            ("wave40".into(), place(rng, s, false, false))
        }
        _ => {
            let n = 13usize;
            let mut kn = vec![0.0; 4];
            for i in 1..=(n - 4) {
                kn.push(i as f64 / (n - 3) as f64);
            }
            kn.extend([1.0; 4]);
            let mut c = Vec::new();
            for i in 0..n {
                for j in 0..n {
                    c.push(DVec3::new(
                        i as f64 * 6.0 / 12.0 - 3.0,
                        j as f64 * 6.0 / 12.0 - 3.0,
                        rng.range(-0.8, 0.8),
                    ));
                }
            }
            let s = NurbsSurface::new(3, 3, kn.clone(), kn, n, n, c, vec![1.0; n * n]).unwrap();
            ("bump".into(), place(rng, s, false, false))
        }
    }
}

fn analytic_kind(rng: &mut Rng, k: usize) -> (String, Surface) {
    let o = DVec3::new(
        rng.range(-1.5, 1.5),
        rng.range(-1.5, 1.5),
        rng.range(-1.5, 1.5),
    );
    let f = Frame::from_origin_normal(o, rng.unit());
    match k % 5 {
        0 => ("plane".into(), Surface::Plane { frame: f }),
        1 => (
            "sphere".into(),
            Surface::Sphere {
                frame: f,
                radius: rng.range(0.5, 4.0),
            },
        ),
        2 => (
            "cyl".into(),
            Surface::Cylinder {
                frame: f,
                radius: rng.range(0.5, 3.0),
            },
        ),
        3 => (
            "cone".into(),
            Surface::Cone {
                frame: f,
                radius: rng.range(0.5, 2.0),
                slope: rng.range(-0.6, 0.6),
            },
        ),
        _ => (
            "torus".into(),
            Surface::Torus {
                frame: f,
                major: rng.range(1.5, 3.5),
                minor: rng.range(0.3, 1.0),
            },
        ),
    }
}

/// Bodenwahrheit auf einem `n × n`-Raster von a: Kreuzungen von F_b (auf |F_b| < 1e-6
/// nachgehalbiert) und ihr kleinster Schnittwinkel.
fn truth(a: &Surface, b: &Surface, n: usize) -> (Vec<DVec3>, f64) {
    let Surface::Nurbs(d) = a else { panic!() };
    let ((u0, u1), (v0, v1)) = d.surface().domain();
    let at = |i: usize, j: usize| {
        (
            u0 + (u1 - u0) * i as f64 / n as f64,
            v0 + (v1 - v0) * j as f64 / n as f64,
        )
    };
    let f: Vec<Vec<f64>> = (0..=n)
        .map(|i| {
            (0..=n)
                .map(|j| {
                    let (u, v) = at(i, j);
                    b.implicit(d.point(u, v))
                })
                .collect()
        })
        .collect();
    let mut pts = Vec::new();
    let mut min_angle = f64::INFINITY;
    for i in 0..=n {
        for j in 0..=n {
            for (i2, j2) in [(i + 1, j), (i, j + 1)] {
                if i2 > n || j2 > n {
                    continue;
                }
                let (fa, fb) = (f[i][j], f[i2][j2]);
                if fa * fb < 0.0 && (fa - fb).abs() < 1.0 {
                    let (ua, va) = at(i, j);
                    let (ub, vb) = at(i2, j2);
                    let g = |s: f64| {
                        let (u, v) = (ua + (ub - ua) * s, va + (vb - va) * s);
                        b.implicit(d.point(u, v))
                    };
                    let (mut lo, mut hi) = (0.0, 1.0);
                    for _ in 0..50 {
                        let m = 0.5 * (lo + hi);
                        if (g(m) < 0.0) == (fa < 0.0) {
                            lo = m
                        } else {
                            hi = m
                        }
                    }
                    let s = 0.5 * (lo + hi);
                    let (u, v) = (ua + (ub - ua) * s, va + (vb - va) * s);
                    let x = d.point(u, v);
                    if b.implicit(x).abs() > 1e-6 {
                        continue;
                    }
                    let na = d.normal(u, v);
                    let (bu, bv) = b.project(x);
                    let nb = b.normal(bu, bv);
                    let ang = na.dot(nb).abs().clamp(0.0, 1.0).acos();
                    min_angle = min_angle.min(ang);
                    pts.push(x);
                }
            }
        }
    }
    (pts, min_angle)
}

/// Auswertung einer Zufallsmenge.
#[derive(Default, Debug)]
struct Stats {
    /// Falsch: Rasterkreuzung weiter als 0,05 mm von jeder Kurve, Kurvenpunkt (`samples` je Kurve)
    /// mehr als `NURBS_FIT_TOL` neben einer Fläche oder `None` trotz Kreuzung.
    wrong: usize,
    /// je Paarart (Freiform–analytisch, Freiform–Freiform): abgelehnt / gesamt, und bei
    /// Schnittwinkel über 10°: abgelehnt / gesamt
    refused: [usize; 2],
    total: [usize; 2],
    refused_steep: [usize; 2],
    steep: [usize; 2],
}

/// `n_na` Fälle Freiform–analytisch und `n_nn` Fälle Freiform–Freiform aus dem Zufallsgenerator
/// mit `seed`, Bodenwahrheit auf einem `grid × grid`-Raster, `samples` Prüfpunkte je Kurve.
fn run_set(seed: u64, n_na: usize, n_nn: usize, grid: usize, samples: usize) -> Stats {
    let tol = Tolerance::default();
    let mut rng = Rng(seed);
    let mut st = Stats::default();
    for k in 0..n_na + n_nn {
        let nn = k >= n_na;
        let (na, a) = nurbs_kind(&mut rng, k);
        let (nb, b) = if nn {
            nurbs_kind(&mut rng, k / 5 + k)
        } else {
            analytic_kind(&mut rng, k / 5 + k)
        };
        let r = intersect_nurbs(&a, &b, &tol);
        let (cross, ang) = truth(&a, &b, grid);
        if let SsiOutcome::Curves(cs) = &r {
            let pts: Vec<DVec3> = cs
                .iter()
                .flat_map(|c| {
                    let (t0, t1) = c.domain().unwrap();
                    (0..=samples).map(move |i| c.point(t0 + (t1 - t0) * i as f64 / samples as f64))
                })
                .collect();
            let gap = cross
                .iter()
                .map(|x| {
                    pts.iter()
                        .map(|p| p.distance(*x))
                        .fold(f64::INFINITY, f64::min)
                })
                .fold(0.0, f64::max);
            let off = pts
                .iter()
                .map(|p| a.implicit(*p).abs().max(b.implicit(*p).abs()))
                .fold(0.0, f64::max);
            if gap > 0.05 || off > NURBS_FIT_TOL {
                st.wrong += 1;
                println!("FALSCH {k} {na}-{nb}: Lücke {gap:.2e}, Abweichung {off:.1e}");
            }
        } else if matches!(r, SsiOutcome::None) && !cross.is_empty() {
            st.wrong += 1;
            println!(
                "FALSCH {k} {na}-{nb}: None trotz {} Kreuzungen",
                cross.len()
            );
        }
        let refused = matches!(r, SsiOutcome::Unsupported(_));
        let i = usize::from(nn);
        st.total[i] += 1;
        st.refused[i] += usize::from(refused);
        if !cross.is_empty() && ang.to_degrees() > 10.0 {
            st.steep[i] += 1;
            st.refused_steep[i] += usize::from(refused);
        }
        if let SsiOutcome::Unsupported(m) = &r {
            println!(
                "{k} {na}-{nb}: abgelehnt ({m}), Winkel {:.1}°",
                ang.to_degrees()
            );
        }
    }
    println!("{st:?}");
    st
}

fn check(st: &Stats) {
    assert_eq!(st.wrong, 0, "falsche Ergebnisse");
    assert_eq!(st.refused[0], 0, "Freiform–analytisch abgelehnt");
    assert!(
        st.refused_steep[1] * 10 <= st.steep[1],
        "Freiform–Freiform bei über 10° zu oft abgelehnt: {}/{}",
        st.refused_steep[1],
        st.steep[1]
    );
}

/// Review I-B (klein, auch im Debug-Build): 12 Freiform–analytisch- und 4 Freiform–Freiform-Fälle.
#[test]
fn a_small_random_set_is_never_wrong_and_rarely_refused() {
    check(&run_set(7, 12, 4, 16, 300));
}

/// Review I-B (groß): 200 Freiform–analytisch- und 60 Freiform–Freiform-Fälle gegen die
/// Bodenwahrheit eines 80 × 80-Rasters. Nur im Release-Build
/// (`cargo test --release -- --ignored`, etwa 6 min).
#[test]
#[ignore = "Belastungstest, nur mit --release --ignored (etwa 6 min)"]
fn random_pairs_are_never_wrong_and_rarely_refused() {
    check(&run_set(4242, 200, 60, 80, 3000));
}
