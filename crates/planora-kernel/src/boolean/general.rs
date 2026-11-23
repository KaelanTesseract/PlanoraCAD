//! Schnitte in allgemeiner Lage (C15, 2026-11-23): Paare analytischer Flächen, die der exakte Weg
//! nicht kennt (schräge Zylinder, Kegel und Torus in allgemeiner Lage, Zylinder/Kegel neben der
//! Achse einer Kugel …), rechnet der numerische Schnittrechner der Freiformflächen. Dazu wird eine
//! der beiden Flächen **exakt** als rationale NURBS-Drehfläche dargestellt (Strecke bzw. Kreis um
//! die Achse), begrenzt auf den Bereich ihrer Fläche im Körper.

use super::nurbs_ssi::{SsiOutcome, intersect_nurbs};
use super::surfsurf::SurfaceIntersection;
use crate::geometry::Surface;
use planora_math::nurbs::{NurbsCurve, NurbsSurface};
use planora_math::{DVec3, TAU, Tolerance};

/// Zugabe über den Hüllquader der Fläche hinaus, als Anteil seiner Diagonale: Die Schnittkurven
/// enden am Rand der Ersatzfläche; mit Zugabe liegt dieser Rand sicher außerhalb der Fläche, deren
/// Ränder die Kurven danach beschneiden.
const PAD_REL: f64 = 0.05;
/// Kleinste Zugabe (mm).
const PAD_MIN: f64 = 1e-3;
/// Kürzeste Ersatzfläche entlang der Achse (mm); darunter kein Ersatzweg.
const MIN_LENGTH: f64 = 1e-6;

/// Rang beim Umwandeln: Die Fläche mit dem höheren Rang wird NURBS, die andere bleibt exakt.
/// Die Ebene wird nie umgewandelt, die Kugel nur, wenn nichts anderes geht (kommt nicht vor:
/// Kugel–Kugel und Ebene–Kugel rechnet der exakte Weg).
fn rank(s: &Surface) -> u8 {
    match s {
        Surface::Torus { .. } => 4,
        Surface::Cone { .. } => 3,
        Surface::Cylinder { .. } => 2,
        Surface::Sphere { .. } => 1,
        Surface::Plane { .. } | Surface::Nurbs(_) => 0,
    }
}

/// Schnitt zweier analytischer Flächen über den numerischen Schnittrechner; `pts_a`, `pts_b` sind
/// Punkte auf den Rändern ihrer Flächen (sie bestimmen den Bereich entlang der Achse). `None`, wenn sich keine Fläche umwandeln lässt (dann bleibt es
/// bei „nicht unterstützt“).
pub(crate) fn intersect_general(
    a: &Surface,
    pts_a: &[DVec3],
    b: &Surface,
    pts_b: &[DVec3],
    tol: &Tolerance,
) -> Option<SurfaceIntersection> {
    let (conv, other) = if rank(b) > rank(a) {
        (bounded_nurbs(b, pts_b)?, a)
    } else {
        (bounded_nurbs(a, pts_a)?, b)
    };
    Some(match intersect_nurbs(&conv, other, tol) {
        SsiOutcome::Curves(cs) => SurfaceIntersection::Curves(cs),
        SsiOutcome::None => SurfaceIntersection::None,
        SsiOutcome::Touch => SurfaceIntersection::Touch,
        SsiOutcome::Coincident => SurfaceIntersection::Coincident,
        SsiOutcome::Unsupported(msg) => SurfaceIntersection::UnsupportedBecause(msg),
    })
}

/// Exakte NURBS-Darstellung von `s`, begrenzt auf den Bereich der Randpunkte `pts` seiner Fläche
/// (Zylinder und Kegel entlang der Achse, mit Zugabe; der Torus ganz). Auf Zylinder und Kegel ist
/// die Höhe entlang der Achse eine Koordinate der Fläche, ihr Bereich wird also am Rand erreicht.
/// `None` für Ebene, Kugel, Freiformfläche, einen Torus mit Rohrradius ≥ großem Radius oder wenn
/// die Begrenzung keine Fläche übrig lässt.
pub fn bounded_nurbs(s: &Surface, pts: &[DVec3]) -> Option<Surface> {
    match *s {
        Surface::Cylinder { frame, radius } => {
            let (v0, v1) = axial_range(&frame, pts)?;
            let p = |v: f64| frame.origin + frame.x * radius + frame.z * v;
            revolved(&line(p(v0), p(v1))?, &frame, false)
        }
        Surface::Cone {
            frame,
            radius,
            slope,
        } => {
            let (mut v0, mut v1) = axial_range(&frame, pts)?;
            if slope != 0.0 {
                // Nie über die Spitze hinaus: auf der Seite, auf der die Fläche liegt (ihre
                // Randpunkte liegen alle auf dieser Seite).
                let apex = -radius / slope;
                let (lo, hi) = axial_extent(&frame, pts);
                if 0.5 * (lo + hi) >= apex {
                    v0 = v0.max(apex);
                } else {
                    v1 = v1.min(apex);
                }
                if v1 - v0 < MIN_LENGTH {
                    return None;
                }
            }
            let p = |v: f64| frame.origin + frame.x * (radius + slope * v) + frame.z * v;
            revolved(&line(p(v0), p(v1))?, &frame, false)
        }
        Surface::Torus {
            frame,
            major,
            minor,
        } => {
            // Spindel- und Horntorus schnitten sich selbst; die gibt es im Kernel nicht.
            if minor >= major {
                return None;
            }
            let meridian = NurbsCurve::elliptic_arc(
                frame.origin + frame.x * major,
                frame.x * minor,
                frame.z * minor,
                0.0,
                TAU,
            )
            .ok()?;
            revolved(&meridian, &frame, true)
        }
        Surface::Plane { .. } | Surface::Sphere { .. } | Surface::Nurbs(_) => None,
    }
}

/// Bereich entlang der Achse `frame.z`, den die Punkte überdecken (ohne Zugabe).
fn axial_extent(frame: &planora_math::Frame, pts: &[DVec3]) -> (f64, f64) {
    pts.iter()
        .map(|p| (*p - frame.origin).dot(frame.z))
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), v| {
            (lo.min(v), hi.max(v))
        })
}

/// Wie [`axial_extent`], mit Zugabe (Anteil der Diagonale der Punkte); `None` bei nicht endlichem
/// oder leerem Bereich.
fn axial_range(frame: &planora_math::Frame, pts: &[DVec3]) -> Option<(f64, f64)> {
    let (lo, hi) = axial_extent(frame, pts);
    let (bl, bh) = pts.iter().fold(
        (DVec3::splat(f64::INFINITY), DVec3::splat(f64::NEG_INFINITY)),
        |(l, h), p| (l.min(*p), h.max(*p)),
    );
    let pad = PAD_REL * (bh - bl).length() + PAD_MIN;
    let (v0, v1) = (lo - pad, hi + pad);
    (v0.is_finite() && v1.is_finite() && v1 - v0 >= MIN_LENGTH).then_some((v0, v1))
}

fn line(a: DVec3, b: DVec3) -> Option<NurbsCurve> {
    NurbsCurve::new(1, vec![0.0, 0.0, 1.0, 1.0], vec![a, b], vec![1.0; 2]).ok()
}

/// Volle Drehung von `profile` um `frame.z` (u geschlossen), beim Torus auch v geschlossen.
fn revolved(profile: &NurbsCurve, frame: &planora_math::Frame, closed_v: bool) -> Option<Surface> {
    let s = NurbsSurface::revolve(profile, frame.origin, frame.z, 0.0, TAU).ok()?;
    Surface::nurbs(s, true, closed_v).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use planora_math::Frame;

    /// Punkte der Ersatzfläche, gleichmäßig über ihren Bereich.
    fn samples(s: &Surface) -> Vec<DVec3> {
        let Surface::Nurbs(d) = s else {
            panic!("keine Freiformfläche")
        };
        let ((u0, u1), (v0, v1)) = d.surface().domain();
        let mut out = Vec::new();
        for i in 0..=24 {
            for j in 0..=12 {
                let (u, v) = (
                    u0 + (u1 - u0) * i as f64 / 24.0,
                    v0 + (v1 - v0) * j as f64 / 12.0,
                );
                out.push(d.surface().point(u, v));
            }
        }
        out
    }

    /// Ecken eines Quaders als Randpunkte.
    fn corners(lo: DVec3, hi: DVec3) -> Vec<DVec3> {
        (0..8)
            .map(|i| {
                DVec3::new(
                    if i & 1 == 0 { lo.x } else { hi.x },
                    if i & 2 == 0 { lo.y } else { hi.y },
                    if i & 4 == 0 { lo.z } else { hi.z },
                )
            })
            .collect()
    }

    fn tilted() -> Frame {
        Frame::from_origin_normal(DVec3::new(1.0, -2.0, 3.0), DVec3::new(0.3, 0.4, 1.0))
    }

    fn local(f: &Frame, p: DVec3) -> DVec3 {
        let d = p - f.origin;
        DVec3::new(d.dot(f.x), d.dot(f.y), d.dot(f.z))
    }

    #[test]
    fn the_torus_is_exact() {
        let frame = tilted();
        let s = Surface::Torus {
            frame,
            major: 10.0,
            minor: 3.0,
        };
        let n = bounded_nurbs(&s, &corners(DVec3::splat(-20.0), DVec3::splat(20.0))).unwrap();
        for p in samples(&n) {
            let q = local(&frame, p);
            let rho = (q.x * q.x + q.y * q.y).sqrt();
            let d = ((rho - 10.0).powi(2) + q.z * q.z).sqrt() - 3.0;
            assert!(d.abs() < 1e-9, "{p:?}: {d}");
        }
    }

    #[test]
    fn the_cylinder_is_exact_and_covers_the_face() {
        let frame = tilted();
        let s = Surface::Cylinder { frame, radius: 4.0 };
        let pts = corners(DVec3::new(-5.0, -6.0, -2.0), DVec3::new(7.0, 2.0, 9.0));
        let n = bounded_nurbs(&s, &pts).unwrap();
        let (lo, hi) = axial_extent(&frame, &pts);
        let (mut vmin, mut vmax) = (f64::INFINITY, f64::NEG_INFINITY);
        for p in samples(&n) {
            let q = local(&frame, p);
            assert!(((q.x * q.x + q.y * q.y).sqrt() - 4.0).abs() < 1e-9, "{p:?}");
            (vmin, vmax) = (vmin.min(q.z), vmax.max(q.z));
        }
        assert!(
            vmin < lo && vmax > hi,
            "{vmin}..{vmax} deckt {lo}..{hi} nicht"
        );
    }

    #[test]
    fn the_cone_stops_at_its_apex() {
        // Radius 6 bei v = 0, Spitze bei v = 12; die Fläche liegt darunter.
        let frame = Frame::WORLD;
        let s = Surface::Cone {
            frame,
            radius: 6.0,
            slope: -0.5,
        };
        let n = bounded_nurbs(
            &s,
            &corners(DVec3::new(-6.0, -6.0, 0.0), DVec3::new(6.0, 6.0, 12.0)),
        )
        .unwrap();
        for p in samples(&n) {
            assert!(p.z <= 12.0 + 1e-9, "über die Spitze: {p:?}");
            let r = (p.x * p.x + p.y * p.y).sqrt();
            assert!((r - (6.0 - 0.5 * p.z)).abs() < 1e-9, "{p:?}");
        }
    }

    #[test]
    fn plane_and_sphere_are_not_converted() {
        let b = corners(DVec3::splat(-1.0), DVec3::splat(1.0));
        assert!(
            bounded_nurbs(
                &Surface::Plane {
                    frame: Frame::WORLD
                },
                &b
            )
            .is_none()
        );
        let sphere = Surface::Sphere {
            frame: Frame::WORLD,
            radius: 1.0,
        };
        assert!(bounded_nurbs(&sphere, &b).is_none());
    }

    /// Prüferfund (C15): Ein flaches, schräg liegendes Kegelstück dicht an der Spitze. Die Mitte
    /// eines weltachsigen Hüllquaders läge auf der anderen Seite der Spitze; die Randpunkte liegen
    /// alle auf der richtigen.
    #[test]
    fn a_flat_tilted_cone_piece_keeps_its_side_of_the_apex() {
        let frame = Frame::from_origin_normal(DVec3::ZERO, DVec3::new(0.6, 0.2, 0.77));
        // Spitze bei v = 0, Radius 100 bei v = 0,1 (sehr flach).
        let s = Surface::Cone {
            frame,
            radius: 0.0,
            slope: 1000.0,
        };
        // Randpunkte eines Ausschnitts von etwa 60° bei v ∈ [0,02; 0,1].
        let mut pts = Vec::new();
        for i in 0..=12 {
            let a = -0.5 + i as f64 / 12.0;
            for v in [0.02, 0.1] {
                let r = 1000.0 * v;
                pts.push(frame.origin + (frame.x * a.cos() + frame.y * a.sin()) * r + frame.z * v);
            }
        }
        let n = bounded_nurbs(&s, &pts).unwrap();
        let mut above = 0;
        for p in samples(&n) {
            let v = local(&frame, p).z;
            assert!(v >= -1e-9, "auf der anderen Seite der Spitze: {v}");
            above += usize::from(v > 0.05);
        }
        assert!(above > 0, "der Bereich der Fläche fehlt");
    }

    #[test]
    fn a_spindle_torus_is_not_converted() {
        let s = Surface::Torus {
            frame: Frame::WORLD,
            major: 2.0,
            minor: 3.0,
        };
        assert!(bounded_nurbs(&s, &corners(DVec3::splat(-5.0), DVec3::splat(5.0))).is_none());
    }
}
