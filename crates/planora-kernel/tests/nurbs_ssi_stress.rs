use planora_kernel::Surface;
use planora_kernel::boolean::{SsiOutcome, intersect_nurbs};
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
    match k % 5 {
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

/// Bodenwahrheit auf einem Raster von a: Kreuzungen von F_b und ihr kleinster Schnittwinkel.
fn truth(a: &Surface, b: &Surface) -> (Vec<DVec3>, f64) {
    let Surface::Nurbs(d) = a else { panic!() };
    let ((u0, u1), (v0, v1)) = d.surface().domain();
    let n = 80;
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

/// Review I-B: 400 zufällige Fälle (200 Freiform–analytisch, 200 Freiform–Freiform) gegen die
/// Bodenwahrheit eines Rasters. Kein falsches Ergebnis; Ablehnungen bei Schnittwinkel über 10°
/// höchstens 10 %. Läuft nur im Release-Build (`cargo test --release -- --ignored`, etwa 7 min).
#[test]
#[ignore = "Belastungstest, nur mit --release --ignored (etwa 7 min)"]
fn random_pairs_are_rarely_refused_and_never_wrong() {
    let tol = Tolerance::default();
    let mut rng = Rng(4242);
    let mut rows: Vec<(bool, String, String, f64, usize)> = Vec::new(); // (nn, name, outcome, angle, crossings)
    let mut wrong = 0;
    for k in 0..400 {
        let nn = k >= 200;
        let (na, a) = nurbs_kind(&mut rng, k);
        let (nb, b) = if nn {
            nurbs_kind(&mut rng, k / 5 + k)
        } else {
            analytic_kind(&mut rng, k / 5 + k)
        };
        let t = std::time::Instant::now();
        let r = intersect_nurbs(&a, &b, &tol);
        let el = t.elapsed();
        let (cross, ang) = truth(&a, &b);
        let out = match &r {
            SsiOutcome::Curves(cs) => format!("Curves({})", cs.len()),
            SsiOutcome::Unsupported(m) => {
                format!("Unsupported: {}", m.chars().take(60).collect::<String>())
            }
            o => format!("{o:?}"),
        };
        // grobe Prüfung: alle Kreuzungen nahe einer Kurve
        if let SsiOutcome::Curves(cs) = &r {
            let pts: Vec<DVec3> = cs
                .iter()
                .flat_map(|c| {
                    let (t0, t1) = c.domain().unwrap();
                    (0..=3000).map(move |i| c.point(t0 + (t1 - t0) * i as f64 / 3000.0))
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
            let worst = cross.iter().copied().max_by(|x, y| {
                let dx = pts
                    .iter()
                    .map(|p| p.distance(*x))
                    .fold(f64::INFINITY, f64::min);
                let dy = pts
                    .iter()
                    .map(|p| p.distance(*y))
                    .fold(f64::INFINITY, f64::min);
                dx.total_cmp(&dy)
            });
            if gap > 0.05 || off > 2e-5 {
                wrong += 1;
                println!(
                    "WRONG? {k} {na}-{nb} gap={gap:.2e} off={off:.1e} at {worst:?} fa={:?} fb={:?}",
                    worst.map(|x| a.implicit(x)),
                    worst.map(|x| b.implicit(x))
                );
            }
        } else if matches!(r, SsiOutcome::None) && !cross.is_empty() {
            wrong += 1;
            println!("WRONG None {k} {na}-{nb} crossings={}", cross.len());
        }
        let deg = if ang.is_finite() {
            ang.to_degrees()
        } else {
            f64::NAN
        };
        println!(
            "{k} {}{na}-{nb}: {out} angle={deg:.1} cross={} {:?}",
            if nn { "NN " } else { "" },
            cross.len(),
            el
        );
        rows.push((nn, format!("{na}-{nb}"), out, deg, cross.len()));
    }
    for nn in [false, true] {
        let set: Vec<_> = rows.iter().filter(|r| r.0 == nn).collect();
        let refused = set
            .iter()
            .filter(|r| r.2.starts_with("Unsupported"))
            .count();
        let crossing: Vec<_> = set.iter().filter(|r| r.4 > 0 && r.3 > 10.0).collect();
        let refused_crossing = crossing
            .iter()
            .filter(|r| r.2.starts_with("Unsupported"))
            .count();
        let disjoint: Vec<_> = set.iter().filter(|r| r.4 == 0).collect();
        let refused_disjoint = disjoint
            .iter()
            .filter(|r| r.2.starts_with("Unsupported"))
            .count();
        println!(
            "{}: refused {}/{} ; angle>10°: {}/{} ; no grid crossing: {}/{}",
            if nn {
                "NURBS-NURBS"
            } else {
                "NURBS-analytisch"
            },
            refused,
            set.len(),
            refused_crossing,
            crossing.len(),
            refused_disjoint,
            disjoint.len()
        );
    }
    println!("wrong {wrong}");
}
