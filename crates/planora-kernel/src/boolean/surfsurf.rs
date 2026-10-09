use crate::geometry::{Curve, Surface};
use planora_math::{DVec3, Frame, Tolerance};

#[derive(Clone, Debug)]
pub enum SurfaceIntersection {
    /// Kein gemeinsamer Punkt.
    None,
    /// Nur Berührung ohne Durchdringung.
    Touch,
    /// Dieselbe Fläche (innerhalb der Toleranz).
    Coincident,
    Curves(Vec<Curve>),
    /// Paar außerhalb von Stufe A.
    Unsupported,
}

const ANG: f64 = 1e-9;

pub fn surface_name(s: &Surface) -> &'static str {
    match s {
        Surface::Plane { .. } => "Ebene",
        Surface::Cylinder { .. } => "Zylinder",
        Surface::Cone { .. } => "Kegel",
        Surface::Sphere { .. } => "Kugel",
        Surface::Torus { .. } => "Torus",
    }
}

fn line(origin: DVec3, dir: DVec3) -> Curve {
    Curve::Line {
        origin,
        dir: dir.normalize(),
    }
}

fn circle(center: DVec3, normal: DVec3, radius: f64) -> Curve {
    Curve::Circle {
        frame: Frame::from_origin_normal(center, normal),
        radius,
    }
}

/// Schnitt zweier Flächen (symmetrisch in den Argumenten).
pub fn intersect_surfaces(a: &Surface, b: &Surface, tol: &Tolerance) -> SurfaceIntersection {
    use Surface::*;
    match (a, b) {
        (Plane { frame: fa }, Plane { frame: fb }) => plane_plane(fa, fb, tol),
        (Plane { frame: p }, Cylinder { frame, radius })
        | (Cylinder { frame, radius }, Plane { frame: p }) => {
            plane_cylinder(p, frame, *radius, tol)
        }
        (
            Plane { frame: p },
            Cone {
                frame,
                radius,
                slope,
            },
        )
        | (
            Cone {
                frame,
                radius,
                slope,
            },
            Plane { frame: p },
        ) => plane_cone(p, frame, *radius, *slope, tol),
        (Plane { frame: p }, Sphere { frame, radius })
        | (Sphere { frame, radius }, Plane { frame: p }) => {
            plane_sphere(p, frame.origin, *radius, tol)
        }
        (
            Plane { frame: p },
            Torus {
                frame,
                major,
                minor,
            },
        )
        | (
            Torus {
                frame,
                major,
                minor,
            },
            Plane { frame: p },
        ) => plane_torus(p, frame, *major, *minor, tol),
        (
            Sphere {
                frame: fa,
                radius: ra,
            },
            Sphere {
                frame: fb,
                radius: rb,
            },
        ) => sphere_sphere(fa.origin, *ra, fb.origin, *rb, tol),
        (
            Cylinder {
                frame: fa,
                radius: ra,
            },
            Cylinder {
                frame: fb,
                radius: rb,
            },
        ) => {
            if coaxial(fa, fb, tol) {
                revolve_pair(a, b, tol)
            } else if fa.z.cross(fb.z).length() < ANG {
                cylinder_cylinder_parallel(fa, *ra, fb, *rb, tol)
            } else {
                SurfaceIntersection::Unsupported
            }
        }
        _ => revolve_pair(a, b, tol),
    }
}

fn plane_plane(f1: &Frame, f2: &Frame, tol: &Tolerance) -> SurfaceIntersection {
    let d = f1.z.cross(f2.z);
    if d.length() < ANG {
        let dist = (f2.origin - f1.origin).dot(f1.z);
        return if dist.abs() <= tol.linear {
            SurfaceIntersection::Coincident
        } else {
            SurfaceIntersection::None
        };
    }
    let c1 = f1.z.dot(f1.origin);
    let c2 = f2.z.dot(f2.origin);
    let p = (f2.z.cross(d) * c1 + d.cross(f1.z) * c2) / d.dot(d);
    SurfaceIntersection::Curves(vec![line(p, d)])
}

fn plane_cylinder(pf: &Frame, cf: &Frame, r: f64, tol: &Tolerance) -> SurfaceIntersection {
    let n = pf.z;
    let a = cf.z;
    let cos = n.dot(a);
    let dist = n.dot(cf.origin - pf.origin);
    if (cos.abs() - 1.0).abs() < ANG {
        let center = cf.origin + a * (-dist / cos);
        return SurfaceIntersection::Curves(vec![circle(center, n, r)]);
    }
    if cos.abs() < ANG {
        let h = dist.abs();
        if h > r + tol.linear {
            return SurfaceIntersection::None;
        }
        if (h - r).abs() <= tol.linear {
            return SurfaceIntersection::Touch;
        }
        let foot = cf.origin - n * dist;
        let m = a.cross(n).normalize();
        let w = (r * r - h * h).sqrt();
        return SurfaceIntersection::Curves(vec![line(foot + m * w, a), line(foot - m * w, a)]);
    }
    let center = cf.origin + a * (-dist / cos);
    let x = (a - n * cos).normalize();
    let y = n.cross(x);
    let frame = Frame {
        origin: center,
        x,
        y,
        z: n,
    };
    SurfaceIntersection::Curves(vec![Curve::Ellipse {
        frame,
        major: r / cos.abs(),
        minor: r,
    }])
}

fn plane_cone(
    pf: &Frame,
    cf: &Frame,
    radius: f64,
    slope: f64,
    tol: &Tolerance,
) -> SurfaceIntersection {
    let n = pf.z;
    let a = cf.z;
    let cos = n.dot(a);
    let dist = n.dot(cf.origin - pf.origin);
    if (cos.abs() - 1.0).abs() < ANG {
        let h = -dist / cos;
        let rho = radius + slope * h;
        if rho <= tol.linear {
            return SurfaceIntersection::None;
        }
        return SurfaceIntersection::Curves(vec![circle(cf.origin + a * h, n, rho)]);
    }
    if cos.abs() < ANG && dist.abs() <= tol.linear {
        let m = a.cross(n).normalize();
        return SurfaceIntersection::Curves(vec![
            line(cf.origin + m * radius, a + m * slope),
            line(cf.origin - m * radius, a - m * slope),
        ]);
    }
    SurfaceIntersection::Unsupported
}

fn plane_sphere(pf: &Frame, c: DVec3, r: f64, tol: &Tolerance) -> SurfaceIntersection {
    let d = pf.z.dot(c - pf.origin);
    if d.abs() > r + tol.linear {
        return SurfaceIntersection::None;
    }
    if (d.abs() - r).abs() <= tol.linear {
        return SurfaceIntersection::Touch;
    }
    SurfaceIntersection::Curves(vec![circle(c - pf.z * d, pf.z, (r * r - d * d).sqrt())])
}

fn plane_torus(
    pf: &Frame,
    tf: &Frame,
    major: f64,
    minor: f64,
    tol: &Tolerance,
) -> SurfaceIntersection {
    let n = pf.z;
    let a = tf.z;
    let cos = n.dot(a);
    if (cos.abs() - 1.0).abs() < ANG {
        let h = (pf.origin - tf.origin).dot(a);
        if h.abs() > minor + tol.linear {
            return SurfaceIntersection::None;
        }
        if (h.abs() - minor).abs() <= tol.linear {
            return SurfaceIntersection::Touch;
        }
        let w = (minor * minor - h * h).sqrt();
        let center = tf.origin + a * h;
        let mut cs = vec![circle(center, n, major + w)];
        if major - w > tol.linear {
            cs.push(circle(center, n, major - w));
        }
        return SurfaceIntersection::Curves(cs);
    }
    if cos.abs() < ANG && n.dot(tf.origin - pf.origin).abs() <= tol.linear {
        let m = a.cross(n).normalize();
        return SurfaceIntersection::Curves(vec![
            circle(tf.origin + m * major, n, minor),
            circle(tf.origin - m * major, n, minor),
        ]);
    }
    SurfaceIntersection::Unsupported
}

fn sphere_sphere(c1: DVec3, r1: f64, c2: DVec3, r2: f64, tol: &Tolerance) -> SurfaceIntersection {
    let d = (c2 - c1).length();
    if d <= tol.linear {
        return if (r1 - r2).abs() <= tol.linear {
            SurfaceIntersection::Coincident
        } else {
            SurfaceIntersection::None
        };
    }
    if d > r1 + r2 + tol.linear || d < (r1 - r2).abs() - tol.linear {
        return SurfaceIntersection::None;
    }
    if (d - (r1 + r2)).abs() <= tol.linear || (d - (r1 - r2).abs()).abs() <= tol.linear {
        return SurfaceIntersection::Touch;
    }
    let n = (c2 - c1) / d;
    let x = (d * d + r1 * r1 - r2 * r2) / (2.0 * d);
    SurfaceIntersection::Curves(vec![circle(c1 + n * x, n, (r1 * r1 - x * x).sqrt())])
}

fn cylinder_cylinder_parallel(
    f1: &Frame,
    r1: f64,
    f2: &Frame,
    r2: f64,
    tol: &Tolerance,
) -> SurfaceIntersection {
    let a = f1.z;
    let c1 = f1.origin;
    let c2 = f2.origin - a * (f2.origin - f1.origin).dot(a);
    let dv = c2 - c1;
    let d = dv.length();
    if d <= tol.linear {
        return SurfaceIntersection::Unsupported; // koaxial: wird vorher behandelt
    }
    if d > r1 + r2 + tol.linear || d < (r1 - r2).abs() - tol.linear {
        return SurfaceIntersection::None;
    }
    if (d - (r1 + r2)).abs() <= tol.linear || (d - (r1 - r2).abs()).abs() <= tol.linear {
        return SurfaceIntersection::Touch;
    }
    let e = dv / d;
    let x = (d * d + r1 * r1 - r2 * r2) / (2.0 * d);
    let h = (r1 * r1 - x * x).sqrt();
    let w = a.cross(e);
    let q = c1 + e * x;
    SurfaceIntersection::Curves(vec![line(q + w * h, a), line(q - w * h, a)])
}

// ---- Drehflächen mit gemeinsamer Achse: Meridiankurven in der (ρ, h)-Halbebene ----

fn axis_of(s: &Surface) -> Option<(DVec3, DVec3)> {
    match s {
        Surface::Cylinder { frame, .. }
        | Surface::Cone { frame, .. }
        | Surface::Torus { frame, .. } => Some((frame.origin, frame.z)),
        _ => None,
    }
}

fn dist_to_axis(p: DVec3, o: DVec3, z: DVec3) -> f64 {
    let d = p - o;
    (d - z * d.dot(z)).length()
}

fn coaxial(f1: &Frame, f2: &Frame, tol: &Tolerance) -> bool {
    f1.z.cross(f2.z).length() < ANG && dist_to_axis(f2.origin, f1.origin, f1.z) <= tol.linear
}

enum Mer {
    /// ρ = rho0 + slope · h
    Line { rho0: f64, slope: f64 },
    /// Kreis um (rc, hc) mit Radius r
    Circle { rc: f64, hc: f64, r: f64 },
}

fn meridian(s: &Surface, o: DVec3, z: DVec3) -> Mer {
    let f = s.frame();
    let h0 = (f.origin - o).dot(z);
    let sigma = if f.z.dot(z) >= 0.0 { 1.0 } else { -1.0 };
    match s {
        Surface::Cylinder { radius, .. } => Mer::Line {
            rho0: *radius,
            slope: 0.0,
        },
        Surface::Cone { radius, slope, .. } => Mer::Line {
            rho0: radius - slope * sigma * h0,
            slope: slope * sigma,
        },
        Surface::Sphere { radius, .. } => Mer::Circle {
            rc: 0.0,
            hc: h0,
            r: *radius,
        },
        Surface::Torus { major, minor, .. } => Mer::Circle {
            rc: *major,
            hc: h0,
            r: *minor,
        },
        Surface::Plane { .. } => unreachable!("Ebenen werden vorher behandelt"),
    }
}

enum MerResult {
    None,
    Coincident,
    /// Punkte (ρ, h); `true` = alle Punkte sind Berührpunkte
    Points(Vec<(f64, f64)>, bool),
}

fn mer_intersect(a: &Mer, b: &Mer, tol: &Tolerance) -> MerResult {
    match (a, b) {
        (
            Mer::Line {
                rho0: ra,
                slope: sa,
            },
            Mer::Line {
                rho0: rb,
                slope: sb,
            },
        ) => {
            if (sa - sb).abs() < 1e-12 {
                return if (ra - rb).abs() <= tol.linear {
                    MerResult::Coincident
                } else {
                    MerResult::None
                };
            }
            let h = (rb - ra) / (sa - sb);
            MerResult::Points(vec![(ra + sa * h, h)], false)
        }
        (Mer::Line { rho0, slope }, Mer::Circle { rc, hc, r })
        | (Mer::Circle { rc, hc, r }, Mer::Line { rho0, slope }) => {
            let (s, k) = (*slope, rho0 - rc);
            let qa = s * s + 1.0;
            let qb = 2.0 * (k * s - hc);
            let qc = k * k + hc * hc - r * r;
            let disc = qb * qb - 4.0 * qa * qc;
            let scale = qb * qb + 1.0;
            if disc < -1e-12 * scale {
                return MerResult::None;
            }
            let pt = |h: f64| (rho0 + s * h, h);
            if disc.abs() <= 1e-12 * scale {
                return MerResult::Points(vec![pt(-qb / (2.0 * qa))], true);
            }
            let sq = disc.sqrt();
            MerResult::Points(
                vec![pt((-qb + sq) / (2.0 * qa)), pt((-qb - sq) / (2.0 * qa))],
                false,
            )
        }
        (
            Mer::Circle {
                rc: c1,
                hc: h1,
                r: r1,
            },
            Mer::Circle {
                rc: c2,
                hc: h2,
                r: r2,
            },
        ) => {
            let (dx, dy) = (c2 - c1, h2 - h1);
            let d = (dx * dx + dy * dy).sqrt();
            if d <= tol.linear {
                return if (r1 - r2).abs() <= tol.linear {
                    MerResult::Coincident
                } else {
                    MerResult::None
                };
            }
            if d > r1 + r2 + tol.linear || d < (r1 - r2).abs() - tol.linear {
                return MerResult::None;
            }
            let x = (d * d + r1 * r1 - r2 * r2) / (2.0 * d);
            let hh = (r1 * r1 - x * x).max(0.0).sqrt();
            let (ex, ey) = (dx / d, dy / d);
            let base = (c1 + ex * x, h1 + ey * x);
            if hh <= tol.linear {
                return MerResult::Points(vec![base], true);
            }
            MerResult::Points(
                vec![
                    (base.0 - ey * hh, base.1 + ex * hh),
                    (base.0 + ey * hh, base.1 - ex * hh),
                ],
                false,
            )
        }
    }
}

fn revolve_pair(a: &Surface, b: &Surface, tol: &Tolerance) -> SurfaceIntersection {
    // gemeinsame Achse bestimmen
    let (o, z) = match (axis_of(a), axis_of(b)) {
        (Some((oa, za)), Some((ob, zb))) => {
            if za.cross(zb).length() >= ANG || dist_to_axis(ob, oa, za) > tol.linear {
                return SurfaceIntersection::Unsupported;
            }
            (oa, za)
        }
        (Some((o, z)), None) => {
            if dist_to_axis(b.frame().origin, o, z) > tol.linear {
                return SurfaceIntersection::Unsupported;
            }
            (o, z)
        }
        (None, Some((o, z))) => {
            if dist_to_axis(a.frame().origin, o, z) > tol.linear {
                return SurfaceIntersection::Unsupported;
            }
            (o, z)
        }
        (None, None) => return SurfaceIntersection::Unsupported,
    };
    match mer_intersect(&meridian(a, o, z), &meridian(b, o, z), tol) {
        MerResult::None => SurfaceIntersection::None,
        MerResult::Coincident => SurfaceIntersection::Coincident,
        MerResult::Points(points, all_tangent) => {
            let circles: Vec<Curve> = points
                .into_iter()
                .filter(|(rho, _)| *rho > tol.linear)
                .map(|(rho, h)| circle(o + z * h, z, rho))
                .collect();
            if circles.is_empty() {
                SurfaceIntersection::None
            } else if all_tangent {
                SurfaceIntersection::Touch
            } else {
                SurfaceIntersection::Curves(circles)
            }
        }
    }
}
