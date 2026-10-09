use crate::error::KernelError;
use crate::geometry::{Curve, Surface, UvRect};
use crate::ids::Id;
use crate::profile::{ProfileLoop, Segment, check_loop_input};
use crate::topology::{Edge, Face, HalfEdge, Loop, Solid};
use planora_math::{DVec2, Frame, TAU, Tolerance};

/// Beschreibung eines gekrümmten Flächenstücks, das durch ein Segment entsteht.
struct Curved {
    surface: Surface,
    /// Parameter `v` am Anfang und am Ende des Segments (in Laufrichtung).
    v_start: f64,
    v_end: f64,
    /// Meridiankurve (Bild des Segments bei u = 0) samt Parameterbereich in Laufrichtung.
    meridian: Curve,
    t0: f64,
    t1: f64,
    /// Laufrichtung in der (r, z)-Halbebene in der Segmentmitte.
    tangent: DVec2,
}

/// Rotiert eine Kette in der (r, z)-Halbebene um die z-Achse von `frame` (360°).
///
/// `chain` muss auf der Achse beginnen und enden; innere Punkte müssen echt neben der Achse liegen.
/// Die Kette wird bei Bedarf so gedreht, dass das Material links der Laufrichtung liegt.
// Die negierten Vergleiche lehnen absichtlich auch NaN ab.
#[allow(clippy::neg_cmp_op_on_partial_ord)]
pub fn revolve(chain: &ProfileLoop, frame: &Frame, tol: &Tolerance) -> Result<Solid, KernelError> {
    if !frame.is_orthonormal(tol.linear) {
        return Err(KernelError::InvalidFrame);
    }
    check_loop_input(chain, tol)?;
    let area = chain.signed_area();
    if !(area.is_finite() && area.abs() > 1e-12) {
        return Err(KernelError::ZeroAreaProfile);
    }
    let chain = if area < 0.0 {
        chain.reversed()
    } else {
        chain.clone()
    };

    let pts = chain.chain_points();
    let m = chain.segments.len();
    if pts[0].x.abs() > tol.linear || pts[m].x.abs() > tol.linear {
        return Err(KernelError::ProfileOffAxis);
    }
    if pts[1..m].iter().any(|p| p.x <= tol.linear) {
        return Err(KernelError::ProfileCrossesAxis);
    }
    if chain.sample(64).iter().any(|p| p.x < -tol.linear) {
        return Err(KernelError::ProfileCrossesAxis);
    }

    let to3 = |p: DVec2| frame.origin + frame.x * p.x + frame.z * p.y;
    let mut solid = Solid::new();
    let vertices: Vec<_> = pts.iter().map(|p| solid.add_vertex(to3(*p))).collect();

    // Parallelkreise an den inneren Punkten.
    let mut circles: Vec<Option<Id<Edge>>> = vec![None; m + 1];
    for i in 1..m {
        let p = pts[i];
        let circle_frame = Frame {
            origin: frame.origin + frame.z * p.y,
            ..*frame
        };
        circles[i] = Some(solid.add_edge(
            Curve::Circle {
                frame: circle_frame,
                radius: p.x,
            },
            0.0,
            TAU,
            vertices[i],
            vertices[i],
        ));
    }

    for i in 0..m {
        let (p, q) = (pts[i], pts[i + 1]);
        let seg = chain.segments[i];

        let curved = match seg {
            Segment::Line { .. } => {
                let d = q - p;
                let len = d.length();
                if !(len > tol.linear) {
                    return Err(KernelError::DegenerateSegment);
                }
                if d.y.abs() <= tol.linear {
                    add_planar_face(&mut solid, frame, &circles, i, p, q);
                    continue;
                }
                let meridian = Curve::Line {
                    origin: to3(p),
                    dir: (to3(q) - to3(p)) / len,
                };
                let surface = if d.x.abs() <= tol.linear {
                    Surface::Cylinder {
                        frame: *frame,
                        radius: p.x,
                    }
                } else {
                    let slope = d.x / d.y;
                    Surface::Cone {
                        frame: *frame,
                        radius: p.x - slope * p.y,
                        slope,
                    }
                };
                Curved {
                    surface,
                    v_start: p.y,
                    v_end: q.y,
                    meridian,
                    t0: 0.0,
                    t1: len,
                    tangent: d / len,
                }
            }
            Segment::Arc { center, sweep } => {
                let radius = (p - center).length();
                if !(radius > tol.linear)
                    || !(sweep.abs() > tol.angular)
                    || sweep.abs() > TAU + 1e-9
                {
                    return Err(KernelError::DegenerateSegment);
                }
                let a0 = (p.y - center.y).atan2(p.x - center.x);
                let a1 = a0 + sweep;
                let a_mid = a0 + sweep / 2.0;
                let tangent = DVec2::new(-a_mid.sin(), a_mid.cos()) * sweep.signum();
                let shifted = Frame {
                    origin: frame.origin + frame.z * center.y,
                    ..*frame
                };
                if center.x.abs() <= tol.linear {
                    // Mittelpunkt auf der Achse → Kugel
                    Curved {
                        surface: Surface::Sphere {
                            frame: shifted,
                            radius,
                        },
                        v_start: a0,
                        v_end: a1,
                        meridian: Curve::Circle {
                            frame: Frame {
                                origin: shifted.origin,
                                x: frame.x,
                                y: frame.z,
                                z: frame.x.cross(frame.z),
                            },
                            radius,
                        },
                        t0: a0,
                        t1: a1,
                        tangent,
                    }
                } else if center.x > tol.linear {
                    // Mittelpunkt neben der Achse → Torus
                    Curved {
                        surface: Surface::Torus {
                            frame: shifted,
                            major: center.x,
                            minor: radius,
                        },
                        v_start: a0,
                        v_end: a1,
                        meridian: Curve::Circle {
                            frame: Frame {
                                origin: frame.origin + frame.x * center.x + frame.z * center.y,
                                x: frame.x,
                                y: frame.z,
                                z: frame.x.cross(frame.z),
                            },
                            radius,
                        },
                        t0: a0,
                        t1: a1,
                        tangent,
                    }
                } else {
                    return Err(KernelError::UnsupportedSegment(
                        "Kreisbogen mit Mittelpunkt auf der negativen Seite der Achse",
                    ));
                }
            }
        };

        // Welche Kreise liegen am unteren / oberen Ende des v-Bereichs?
        let along_v = curved.v_end > curved.v_start;
        let (lo, hi) = if along_v { (i, i + 1) } else { (i + 1, i) };
        let meridian = solid.add_edge(
            curved.meridian,
            curved.t0,
            curved.t1,
            vertices[i],
            vertices[i + 1],
        );

        // Kanonische Schleife (gegen den Uhrzeigersinn im (u, v)-Rechteck, also um die natürliche Normale).
        let mut half_edges = Vec::new();
        if let Some(e) = circles[lo] {
            half_edges.push(HalfEdge {
                edge: e,
                forward: true,
            });
        }
        half_edges.push(HalfEdge {
            edge: meridian,
            forward: along_v,
        });
        if let Some(e) = circles[hi] {
            half_edges.push(HalfEdge {
                edge: e,
                forward: false,
            });
        }
        half_edges.push(HalfEdge {
            edge: meridian,
            forward: !along_v,
        });
        let mut outer = Loop { half_edges };

        // Nach außen = rechts der Laufrichtung (das Material liegt links).
        let v_mid = 0.5 * (curved.v_start + curved.v_end);
        let u_mid = std::f64::consts::PI;
        let natural = curved.surface.normal(u_mid, v_mid);
        let outward_rz = DVec2::new(curved.tangent.y, -curved.tangent.x);
        let radial = frame.x * u_mid.cos() + frame.y * u_mid.sin();
        let outward = radial * outward_rz.x + frame.z * outward_rz.y;
        let same_sense = natural.dot(outward) > 0.0;
        if !same_sense {
            outer = outer.reversed();
        }

        solid.add_face(Face {
            surface: curved.surface,
            same_sense,
            outer,
            inner: vec![],
            patch: Some(UvRect {
                u0: 0.0,
                u1: TAU,
                v0: curved.v_start.min(curved.v_end),
                v1: curved.v_start.max(curved.v_end),
            }),
        });
    }

    solid.seal();
    Ok(solid)
}

/// Segment senkrecht zur Achse → Kreisscheibe bzw. Kreisring als ebene Fläche.
fn add_planar_face(
    solid: &mut Solid,
    frame: &Frame,
    circles: &[Option<Id<Edge>>],
    i: usize,
    p: DVec2,
    q: DVec2,
) {
    let plane_frame = Frame {
        origin: frame.origin + frame.z * p.y,
        ..*frame
    };
    // Läuft das Segment nach außen (q.x > p.x), zeigt die Außennormale nach -z.
    let same_sense = q.x < p.x;
    let (big, small) = if p.x >= q.x { (i, i + 1) } else { (i + 1, i) };
    let outer_edge = circles[big].expect("der größere Kreis liegt nie auf der Achse");
    let mut outer = Loop {
        half_edges: vec![HalfEdge {
            edge: outer_edge,
            forward: true,
        }],
    };
    let mut inner = Vec::new();
    if let Some(e) = circles[small] {
        inner.push(Loop {
            half_edges: vec![HalfEdge {
                edge: e,
                forward: false,
            }],
        });
    }
    if !same_sense {
        outer = outer.reversed();
        inner = inner.iter().map(Loop::reversed).collect();
    }
    solid.add_face(Face {
        surface: Surface::Plane { frame: plane_frame },
        same_sense,
        outer,
        inner,
        patch: None,
    });
}
