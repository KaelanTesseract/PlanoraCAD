use crate::error::KernelError;
use crate::geometry::{Curve, Surface, UvRect};
use crate::ids::Id;
use crate::profile::{
    Profile, ProfileLoop, Segment, check_hole_layout, check_loop_input, check_self_intersection,
    snap_closed,
};
use crate::topology::{Edge, Face, FaceTag, HalfEdge, Loop, Solid};
use planora_math::nurbs::{NurbsCurve, NurbsError, NurbsSurface};
use planora_math::{DVec3, Frame, Tolerance};

/// Kanten einer Schleife auf der Unter- und Oberseite.
struct LoopEdges {
    bottom: Vec<Id<Edge>>,
    top: Vec<Id<Edge>>,
}

/// Extrudiert ein ebenes Profil (in der xy-Ebene von `frame`) um `height` entlang `frame.z`.
/// Äußere Schleife und Löcher werden bei Bedarf automatisch richtig orientiert.
// Die negierten Vergleiche lehnen absichtlich auch NaN ab.
#[allow(clippy::neg_cmp_op_on_partial_ord)]
pub fn extrude(
    profile: &Profile,
    frame: &Frame,
    height: f64,
    tol: &Tolerance,
) -> Result<Solid, KernelError> {
    if !frame.is_orthonormal(tol.linear) {
        return Err(KernelError::InvalidFrame);
    }
    if !(height.is_finite() && height > tol.linear) {
        return Err(KernelError::InvalidDimension(height));
    }
    // Erst prüfen, dann orientieren: `oriented()` tastet die Bögen ab.
    for lp in std::iter::once(&profile.outer).chain(profile.holes.iter()) {
        check_loop_input(lp, tol)?;
    }
    check_self_intersection(profile, tol)?;
    check_hole_layout(profile, tol)?;
    // Welche Schleifen dreht `oriented()` um? Nötig, um die Wandnummern auf die eingegebene
    // Reihenfolge zurückzurechnen (gleiche Bedingungen wie in `Profile::oriented`).
    let outer_reversed = profile.outer.signed_area() < 0.0;
    let holes_reversed: Vec<bool> = profile
        .holes
        .iter()
        .map(|h| h.signed_area() > 0.0)
        .collect();
    let profile = profile.try_oriented()?;
    for lp in std::iter::once(&profile.outer).chain(profile.holes.iter()) {
        let area = lp.signed_area();
        if !(area.is_finite() && area.abs() > 1e-12) {
            return Err(KernelError::ZeroAreaProfile);
        }
    }

    let mut solid = Solid::new();
    let outer = build_loop(
        &mut solid,
        frame,
        height,
        &profile.outer,
        tol,
        "",
        outer_reversed,
    )?;
    let holes = profile
        .holes
        .iter()
        .zip(&holes_reversed)
        .enumerate()
        .map(|(j, (h, &reversed))| {
            let prefix = format!("Loch {}, ", j + 1);
            build_loop(&mut solid, frame, height, h, tol, &prefix, reversed)
        })
        .collect::<Result<Vec<_>, _>>()?;

    let forward = |edges: &[Id<Edge>]| Loop {
        half_edges: edges
            .iter()
            .map(|&edge| HalfEdge {
                edge,
                forward: true,
            })
            .collect(),
    };
    let backward_reversed = |edges: &[Id<Edge>]| Loop {
        half_edges: edges
            .iter()
            .rev()
            .map(|&edge| HalfEdge {
                edge,
                forward: false,
            })
            .collect(),
    };

    // Deckel: Normale +z, äußere Schleife gegen den Uhrzeigersinn (von oben gesehen).
    let top_frame = Frame {
        origin: frame.origin + frame.z * height,
        ..*frame
    };
    solid.add_face(Face {
        surface: Surface::Plane { frame: top_frame },
        same_sense: true,
        outer: forward(&outer.top),
        inner: holes.iter().map(|h| forward(&h.top)).collect(),
        patch: None,
        tag: Some(FaceTag {
            origin: 0,
            role: "Deckel".into(),
        }),
    });
    // Boden: Normale -z, alle Schleifen in umgekehrter Richtung.
    solid.add_face(Face {
        surface: Surface::Plane { frame: *frame },
        same_sense: false,
        outer: backward_reversed(&outer.bottom),
        inner: holes.iter().map(|h| backward_reversed(&h.bottom)).collect(),
        patch: None,
        tag: Some(FaceTag {
            origin: 0,
            role: "Boden".into(),
        }),
    });

    solid.seal();
    Ok(solid)
}

fn side_loop(
    bottom: Id<Edge>,
    top: Id<Edge>,
    vertical_start: Id<Edge>,
    vertical_end: Id<Edge>,
) -> Loop {
    Loop {
        half_edges: vec![
            HalfEdge {
                edge: bottom,
                forward: true,
            },
            HalfEdge {
                edge: vertical_end,
                forward: true,
            },
            HalfEdge {
                edge: top,
                forward: false,
            },
            HalfEdge {
                edge: vertical_start,
                forward: false,
            },
        ],
    }
}

/// Baut Kanten und Wandflächen einer (bereits orientierten) Schleife.
///
/// `prefix` steht vor der Wandrolle (`""` außen, `"Loch j, "` für Löcher). `reversed` sagt, ob
/// `lp` gegenüber der Eingabe umgedreht wurde; dann entspricht Segment `i` dem Eingabesegment
/// `m − 1 − i`, die 1-basierte Wandnummer ist also `m − i`.
#[allow(clippy::neg_cmp_op_on_partial_ord)]
fn build_loop(
    solid: &mut Solid,
    frame: &Frame,
    height: f64,
    lp: &ProfileLoop,
    tol: &Tolerance,
    prefix: &str,
    reversed: bool,
) -> Result<LoopEdges, KernelError> {
    if !lp.is_closed(tol) {
        return Err(KernelError::ProfileNotClosed);
    }
    let pts = lp.chain_points();
    let m = lp.segments.len();
    let up = frame.z * height;

    let bottom_v: Vec<_> = (0..m)
        .map(|i| solid.add_vertex(frame.point_2d(pts[i])))
        .collect();
    let top_v: Vec<_> = (0..m)
        .map(|i| solid.add_vertex(frame.point_2d(pts[i]) + up))
        .collect();
    let vertical: Vec<_> = (0..m)
        .map(|i| {
            solid.add_edge(
                Curve::Line {
                    origin: frame.point_2d(pts[i]),
                    dir: frame.z,
                },
                0.0,
                height,
                bottom_v[i],
                top_v[i],
            )
        })
        .collect();

    let mut edges = LoopEdges {
        bottom: Vec::with_capacity(m),
        top: Vec::with_capacity(m),
    };

    for i in 0..m {
        let next = (i + 1) % m;
        let wall = if reversed { m - i } else { i + 1 };
        let tag = Some(FaceTag {
            origin: 0,
            role: format!("{prefix}Wand {wall}"),
        });
        match &lp.segments[i] {
            Segment::Line { .. } => {
                let a = frame.point_2d(pts[i]);
                let b = frame.point_2d(pts[next]);
                let len = a.distance(b);
                if !(len > tol.linear) {
                    return Err(KernelError::DegenerateSegment);
                }
                let dir = (b - a) / len;
                let be = solid.add_edge(
                    Curve::Line { origin: a, dir },
                    0.0,
                    len,
                    bottom_v[i],
                    bottom_v[next],
                );
                let te = solid.add_edge(
                    Curve::Line {
                        origin: a + up,
                        dir,
                    },
                    0.0,
                    len,
                    top_v[i],
                    top_v[next],
                );
                // Normale = dir × z zeigt bei gegen den Uhrzeigersinn laufender Schleife nach außen.
                let side_frame = Frame {
                    origin: a,
                    x: dir,
                    y: frame.z,
                    z: dir.cross(frame.z),
                };
                solid.add_face(Face {
                    surface: Surface::Plane { frame: side_frame },
                    same_sense: true,
                    outer: side_loop(be, te, vertical[i], vertical[next]),
                    inner: vec![],
                    patch: None,
                    tag,
                });
                edges.bottom.push(be);
                edges.top.push(te);
            }
            &Segment::Arc { center, sweep } => {
                let radius = pts[i].distance(center);
                if !(radius > tol.linear) || !(sweep.abs() > tol.angular) {
                    return Err(KernelError::DegenerateSegment);
                }
                let rel = pts[i] - center;
                let a0 = rel.y.atan2(rel.x);
                let a1 = a0 + sweep;
                let circle_frame = Frame {
                    origin: frame.point_2d(center),
                    x: frame.x,
                    y: frame.y,
                    z: frame.z,
                };
                let be = solid.add_edge(
                    Curve::Circle {
                        frame: circle_frame,
                        radius,
                    },
                    a0,
                    a1,
                    bottom_v[i],
                    bottom_v[next],
                );
                let top_circle = Frame {
                    origin: circle_frame.origin + up,
                    ..circle_frame
                };
                let te = solid.add_edge(
                    Curve::Circle {
                        frame: top_circle,
                        radius,
                    },
                    a0,
                    a1,
                    top_v[i],
                    top_v[next],
                );
                // Gegen den Uhrzeigersinn laufende Bögen zeigen nach außen (+radial), sonst nach innen.
                solid.add_face(Face {
                    surface: Surface::Cylinder {
                        frame: circle_frame,
                        radius,
                    },
                    same_sense: sweep > 0.0,
                    outer: side_loop(be, te, vertical[i], vertical[next]),
                    inner: vec![],
                    patch: Some(UvRect {
                        u0: a0.min(a1),
                        u1: a0.max(a1),
                        v0: 0.0,
                        v1: height,
                    }),
                    tag,
                });
                edges.bottom.push(be);
                edges.top.push(te);
            }
            Segment::Nurbs { curve } => {
                let to_plane = |p: DVec3| frame.point(p.x, p.y, 0.0);
                // Eine einzelne geschlossene Kurve (Vollellipse): Ende exakt auf den Anfang legen,
                // die geschlossene Wand verlangt gleiche Randreihen.
                let curve = if m == 1 {
                    snap_closed(curve, tol)?
                } else {
                    (**curve).clone()
                };
                let bottom = curve.transformed(to_plane).map_err(invalid_curve)?;
                let top = bottom.transformed(|p| p + up).map_err(invalid_curve)?;
                let surface = NurbsSurface::extrude(&bottom, up).map_err(invalid_curve)?;
                let (t0, t1) = bottom.domain();
                // Eine einzelne geschlossene Kurve (Vollellipse): Naht wie beim Vollkreis, mit
                // einem Eckpunkt und einer geschlossenen Kante unten und oben.
                let closed = m == 1;
                // Die Seitenschleife läuft gegen den Uhrzeigersinn im (u, v)-Rechteck, also um die
                // natürliche Normale; die muss deshalb nach außen zeigen. Wegen `S_v = z · Höhe`
                // gilt das für jede richtig orientierte Schleife; sonst ist die Kurve entartet.
                if !extrude_normal_points_outward(&surface, &bottom, frame) {
                    return Err(KernelError::DegenerateSegment);
                }
                let be = solid.add_edge(Curve::nurbs(bottom), t0, t1, bottom_v[i], bottom_v[next]);
                let te = solid.add_edge(Curve::nurbs(top), t0, t1, top_v[i], top_v[next]);
                solid.add_face(Face {
                    surface: Surface::nurbs(surface, closed, false)?,
                    same_sense: true,
                    outer: side_loop(be, te, vertical[i], vertical[next]),
                    inner: vec![],
                    patch: Some(UvRect {
                        u0: t0,
                        u1: t1,
                        v0: 0.0,
                        v1: 1.0,
                    }),
                    tag,
                });
                edges.bottom.push(be);
                edges.top.push(te);
            }
        }
    }
    Ok(edges)
}

fn invalid_curve(e: NurbsError) -> KernelError {
    KernelError::InvalidCurve(e.to_string())
}

/// Zahl der Probestellen, an denen die Normale einer extrudierten Freiformwand mit der
/// Außenrichtung verglichen wird (über den Parameterbereich verteilt, ohne die Enden).
const SENSE_SAMPLES: usize = 16;

/// Zeigt die natürliche Normale `S_u × S_v` der extrudierten Wand nach außen? Außen ist bei
/// richtig orientierter Schleife `Tangente × z` (wie bei den Geraden). Aus der Geometrie
/// bestimmt (Summe über mehrere Probestellen, damit Stellen mit verschwindender Tangente nicht
/// entscheiden).
fn extrude_normal_points_outward(
    surface: &NurbsSurface,
    bottom: &NurbsCurve,
    frame: &Frame,
) -> bool {
    let ((u0, u1), (v0, v1)) = surface.domain();
    let v = 0.5 * (v0 + v1);
    let sum: f64 = (1..=SENSE_SAMPLES)
        .map(|k| {
            let u = u0 + (u1 - u0) * k as f64 / (SENSE_SAMPLES + 1) as f64;
            let outward = bottom.tangent(u).cross(frame.z);
            surface.normal(u, v).dot(outward)
        })
        .sum();
    sum > 0.0
}
