use crate::error::KernelError;
use crate::geometry::{Curve, Surface, UvRect};
use crate::ids::Id;
use crate::profile::{ProfileLoop, Segment, check_loop_input, nurbs_steps};
use crate::topology::{Edge, Face, FaceTag, HalfEdge, Loop, Solid};
use planora_math::nurbs::{NurbsCurve, NurbsError, NurbsSurface};
use planora_math::{DVec2, DVec3, Frame, TAU, Tolerance};

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
    /// Bei Freiformsegmenten schon aus der Geometrie bestimmt (siehe [`nurbs_same_sense`]);
    /// sonst `None` und aus `tangent` in der Segmentmitte berechnet.
    same_sense: Option<bool>,
}

/// Zahl der Probestellen, an denen die Normale einer Freiform-Drehfläche mit der Außenrichtung
/// verglichen wird (über den Kurvenbereich verteilt, ohne die Enden, an denen Pole liegen können).
const SENSE_SAMPLES: usize = 16;

fn invalid_curve(e: NurbsError) -> KernelError {
    KernelError::InvalidCurve(e.to_string())
}

/// Zeigt die natürliche Normale der Freiform-Drehfläche nach außen (rechts der Laufrichtung der
/// Profilkurve `rz` in der (r, z)-Halbebene)? Aus der Geometrie bestimmt: an mehreren Stellen der
/// Kurve wird die Flächennormale mit der Außenrichtung verglichen, die Radialrichtung kommt aus dem
/// Flächenpunkt selbst (u ist nur an den Segmentgrenzen der Winkel). Die Normale ist an Polen null
/// und trägt dort nichts bei.
fn nurbs_same_sense(surface: &Surface, rz: &NurbsCurve, frame: &Frame) -> bool {
    let (a, b) = rz.domain();
    let u = std::f64::consts::PI;
    let sum: f64 = (1..=SENSE_SAMPLES)
        .map(|k| {
            let v = a + (b - a) * k as f64 / (SENSE_SAMPLES + 1) as f64;
            let t = rz.tangent(v);
            let t = DVec2::new(t.x, t.y).normalize_or_zero();
            let outward_rz = DVec2::new(t.y, -t.x);
            let rel = surface.point(u, v) - frame.origin;
            let radial = (rel - frame.z * rel.dot(frame.z)).normalize_or_zero();
            let outward = radial * outward_rz.x + frame.z * outward_rz.y;
            surface.normal(u, v).dot(outward)
        })
        .sum();
    sum > 0.0
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
    // Wird die Kette umgedreht, entspricht Segment `i` dem Eingabesegment `m − 1 − i`.
    let reversed = area < 0.0;
    let chain = if reversed {
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
        let seg = &chain.segments[i];
        let tag = Some(FaceTag {
            origin: 0,
            role: format!("Fläche {}", if reversed { m - i } else { i + 1 }),
        });

        let curved = match seg {
            Segment::Line { .. } => {
                let d = q - p;
                let len = d.length();
                if !(len > tol.linear) {
                    return Err(KernelError::DegenerateSegment);
                }
                if d.y.abs() <= tol.linear {
                    add_planar_face(&mut solid, frame, &circles, i, p, q, tag);
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
                    same_sense: None,
                }
            }
            &Segment::Arc { center, sweep } => {
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
                        same_sense: None,
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
                        same_sense: None,
                    }
                } else {
                    return Err(KernelError::UnsupportedSegment(
                        "Kreisbogen mit Mittelpunkt auf der negativen Seite der Achse",
                    ));
                }
            }
            Segment::Nurbs { curve } => nurbs_curved(curve, frame, i == 0, i + 1 == m, tol)?,
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
        let same_sense = curved.same_sense.unwrap_or_else(|| {
            let v_mid = 0.5 * (curved.v_start + curved.v_end);
            let u_mid = std::f64::consts::PI;
            let natural = curved.surface.normal(u_mid, v_mid);
            let outward_rz = DVec2::new(curved.tangent.y, -curved.tangent.x);
            let radial = frame.x * u_mid.cos() + frame.y * u_mid.sin();
            let outward = radial * outward_rz.x + frame.z * outward_rz.y;
            natural.dot(outward) > 0.0
        });
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
            tag,
        });
    }

    solid.seal();
    Ok(solid)
}

/// Freiformsegment der Kette → exakte rationale Drehfläche (volle Drehung, `periodic_u`).
///
/// Die Kurve liegt in der (r, z)-Halbebene (`x = r`, `y = z`) und wird in die xz-Ebene von `frame`
/// gebracht. Beginnt (`first`) bzw. endet (`last`) die Kette mit diesem Segment auf der Achse, wird
/// der zugehörige End-Kontrollpunkt exakt auf die Achse gelegt (Abweichung höchstens `tol.linear`),
/// damit die Fläche dort einen Pol hat (wie die Kugel; die entartete Kante entfällt). Im Inneren
/// muss die Kurve echt neben der Achse liegen (`ProfileCrossesAxis`); eine in sich geschlossene
/// Kurve ist entartet.
fn nurbs_curved(
    curve: &NurbsCurve,
    frame: &Frame,
    first: bool,
    last: bool,
    tol: &Tolerance,
) -> Result<Curved, KernelError> {
    if curve.is_closed(tol.linear) {
        return Err(KernelError::DegenerateSegment);
    }
    let n = nurbs_steps(curve, AXIS_CHECK_STEPS_PER_TURN);
    let (a, b) = curve.domain();
    let inner_off_axis = (1..n).all(|k| {
        let t = a + (b - a) * k as f64 / n as f64;
        curve.point(t).x > tol.linear
    });
    if !inner_off_axis {
        return Err(KernelError::ProfileCrossesAxis);
    }
    let to3 = |x: f64, y: f64| frame.origin + frame.x * x + frame.z * y;
    let cp = curve.control_points();
    let count = cp.len();
    let control: Vec<DVec3> = cp
        .iter()
        .enumerate()
        .map(|(j, p)| {
            let on_axis_end = (j == 0 && first) || (j + 1 == count && last);
            let r = if on_axis_end && p.x.abs() <= tol.linear {
                0.0
            } else {
                p.x
            };
            to3(r, p.y)
        })
        .collect();
    let meridian = NurbsCurve::new(
        curve.degree(),
        curve.knots().knots().to_vec(),
        control,
        curve.weights().to_vec(),
    )
    .map_err(invalid_curve)?;
    let surface = Surface::nurbs(
        NurbsSurface::revolve(&meridian, frame.origin, frame.z, 0.0, TAU).map_err(invalid_curve)?,
        true,
        false,
    )?;
    let same_sense = nurbs_same_sense(&surface, curve, frame);
    Ok(Curved {
        surface,
        v_start: a,
        v_end: b,
        meridian: Curve::nurbs(meridian),
        t0: a,
        t1: b,
        tangent: DVec2::ZERO,
        same_sense: Some(same_sense),
    })
}

/// Abtastschritte je Vollkreis für die Prüfung, dass eine Freiformkurve im Inneren neben der
/// Achse liegt (wie `chain.sample(64)` in [`revolve`]).
const AXIS_CHECK_STEPS_PER_TURN: usize = 64;

/// Segment senkrecht zur Achse → Kreisscheibe bzw. Kreisring als ebene Fläche.
fn add_planar_face(
    solid: &mut Solid,
    frame: &Frame,
    circles: &[Option<Id<Edge>>],
    i: usize,
    p: DVec2,
    q: DVec2,
    tag: Option<FaceTag>,
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
        tag,
    });
}
