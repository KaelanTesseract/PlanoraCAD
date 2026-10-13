//! Rundungen und Fasen (Teilprojekt 8): rollende Kugel, analytische Rundungsflächen, Umbau der
//! Nachbarflächen über `sew` (Spec `2026-10-13-rundungen-und-fasen-design.md`).

pub mod analytic;
pub mod build;
pub mod error;
pub mod section;
pub mod topo;

pub use error::BlendError;

use crate::boolean::{Containment, face_geom, point_in_face};
use crate::ids::Id;
use crate::topology::{Edge, Face, Solid};
use crate::validate::validate_with;
use analytic::{EdgeGeom, SectionKind, VolumeModel, analytic_geom};
use build::{CAVITY, EndCut, EndPlan, assemble, end_cuts, has_cavity};
use planora_math::nurbs::MAX_COORD;
use planora_math::{PI, Tolerance};
use section::ChamferSpec;
use topo::{EdgeSides, edge_angle, edge_sides, face_name};

/// Höchstzahl Kanten je Anfrage (nach Ausdehnung der Tangentenkette).
pub const MAX_BLEND_EDGES: usize = 256;
/// Höchstzahl Punkte eines variablen Radius.
pub const MAX_RADIUS_POINTS: usize = 64;
/// Kleinster (und π minus größter) Fasenwinkel: 1°.
pub const MIN_CHAMFER_ANGLE: f64 = PI / 180.0;
/// Kleinstes Maß (Radius, Fasenabstand) als Vielfaches von `tol.linear`. Mit `tol.linear = 1e-6`
/// sind das 1e-4 mm. Begründung: `sew` verschweißt Punkte bis `ON_CURVE_FACTOR · tol.linear`
/// (1e-5 mm); bei Maßen um 1e-5 mm schloss `sew` den Körper nicht mehr (Task-4-Probe: 43 Fälle
/// „nicht geschlossen“, ab 1,5e-5 mm keiner). Der Faktor 100 hält eine Größenordnung Abstand zu
/// dieser Grenze; kleinere Maße sind in der Konstruktion ohnehin bedeutungslos.
pub const MIN_BLEND_SIZE_FACTOR: f64 = 100.0;
/// Stellen je Berührkurve, an denen geprüft wird, ob sie in ihrer Fläche liegt.
const CONTACT_PROBES: usize = 8;
/// Übergangsmeldung bis Task 9.
const SEVERAL_LATER: &str = "Mehrere Kanten in einem Feature werden noch nicht unterstützt";
/// Übergangsmeldung bis Task 14.
const VARIABLE_LATER: &str = "Der variable Radius wird noch nicht unterstützt";

/// Art der Verrundung (Maße in mm, Winkel im Bogenmaß).
#[derive(Clone, Debug, PartialEq)]
pub enum BlendKind {
    Fillet {
        radius: f64,
    },
    /// Punkte (Anteil 0..=1 der Kettenlänge, Radius), Anteile streng steigend.
    VariableFillet {
        points: Vec<(f64, f64)>,
    },
    ChamferEqual {
        distance: f64,
    },
    /// Abstand auf Fläche 1 und auf Fläche 2.
    ChamferTwo {
        distance1: f64,
        distance2: f64,
    },
    /// Abstand auf Fläche 1 und Winkel der Fase zu Fläche 1.
    ChamferAngle {
        distance: f64,
        angle: f64,
    },
}

impl BlendKind {
    pub fn is_chamfer(&self) -> bool {
        matches!(
            self,
            BlendKind::ChamferEqual { .. }
                | BlendKind::ChamferTwo { .. }
                | BlendKind::ChamferAngle { .. }
        )
    }
}

/// Gewählte Kante; `first_face` ist Fläche 1 (Seite der Fasenmaße, P4).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BlendEdge {
    pub edge: Id<Edge>,
    pub first_face: Option<Id<Face>>,
}

/// Anfrage an den Kernel.
#[derive(Clone, Debug, PartialEq)]
pub struct BlendRequest {
    pub edges: Vec<BlendEdge>,
    pub kind: BlendKind,
    /// Glatt anschließende Kanten mitnehmen (R8).
    pub tangent_chain: bool,
}

/// Ergebnis: neuer Körper, geschätzte Volumenänderung (mm³, negativ = Material weg) und die
/// tatsächlich gerundeten Kanten des Eingangskörpers.
#[derive(Clone, Debug)]
pub struct Blended {
    pub solid: Solid,
    pub volume_change: f64,
    pub edges: Vec<Id<Edge>>,
}

/// Kleinstes erlaubtes Maß (Radius, Fasenabstand) in mm.
pub fn min_blend_size(tol: &Tolerance) -> f64 {
    MIN_BLEND_SIZE_FACTOR * tol.linear
}

/// Ein Maß muss endlich sein, mindestens `min_blend_size` und höchstens `MAX_COORD` betragen.
#[allow(clippy::neg_cmp_op_on_partial_ord)]
fn check_size(x: f64, what: &str, tol: &Tolerance) -> Result<(), BlendError> {
    let min = min_blend_size(tol);
    if !(x.is_finite() && x <= MAX_COORD) {
        return Err(BlendError::InvalidParameter(format!(
            "{what} muss eine endliche Zahl bis {MAX_COORD:e} mm sein (war {x})"
        )));
    }
    if !(x >= min) {
        return Err(BlendError::InvalidParameter(format!(
            "Das Maß ist zu klein: {what} muss mindestens {min} mm betragen (war {x})"
        )));
    }
    Ok(())
}

/// Prüft die Zahlen einer Anfrage (endlich, im Bereich, Punkte streng steigend).
#[allow(clippy::neg_cmp_op_on_partial_ord)]
pub fn check_request(r: &BlendRequest, tol: &Tolerance) -> Result<(), BlendError> {
    if r.edges.is_empty() {
        return Err(BlendError::NoEdges);
    }
    if r.edges.len() > MAX_BLEND_EDGES {
        return Err(BlendError::InvalidParameter(format!(
            "Zu viele Kanten (höchstens {MAX_BLEND_EDGES})"
        )));
    }
    match &r.kind {
        BlendKind::Fillet { radius } => check_size(*radius, "Der Radius", tol),
        BlendKind::VariableFillet { points } => {
            if points.len() < 2 || points.len() > MAX_RADIUS_POINTS {
                return Err(BlendError::InvalidParameter(format!(
                    "Ein variabler Radius braucht 2 bis {MAX_RADIUS_POINTS} Punkte"
                )));
            }
            for (i, (s, radius)) in points.iter().enumerate() {
                if !(s.is_finite() && (0.0..=1.0).contains(s)) {
                    return Err(BlendError::InvalidParameter(
                        "Die Positionen müssen zwischen 0 und 100 % liegen".into(),
                    ));
                }
                if i > 0 && !(*s > points[i - 1].0) {
                    return Err(BlendError::InvalidParameter(
                        "Die Positionen müssen streng steigen".into(),
                    ));
                }
                check_size(*radius, "Jeder Radius", tol)?;
            }
            Ok(())
        }
        BlendKind::ChamferEqual { distance } => check_size(*distance, "Der Abstand", tol),
        BlendKind::ChamferTwo {
            distance1,
            distance2,
        } => {
            check_size(*distance1, "Abstand 1", tol)?;
            check_size(*distance2, "Abstand 2", tol)
        }
        BlendKind::ChamferAngle { distance, angle } => {
            check_size(*distance, "Der Abstand", tol)?;
            if angle.is_finite() && *angle >= MIN_CHAMFER_ANGLE && *angle <= PI - MIN_CHAMFER_ANGLE
            {
                Ok(())
            } else {
                Err(BlendError::InvalidParameter(format!(
                    "Der Winkel der Fase muss zwischen 1° und 179° liegen (war {:.3}°)",
                    angle.to_degrees()
                )))
            }
        }
    }
}

/// Querschnittsart einer konstanten Verrundung.
fn section_kind(kind: &BlendKind) -> Result<SectionKind, BlendError> {
    Ok(match kind {
        BlendKind::Fillet { radius } => SectionKind::Fillet { radius: *radius },
        BlendKind::ChamferEqual { distance } => SectionKind::Chamfer(ChamferSpec::Distances {
            a: *distance,
            b: *distance,
        }),
        BlendKind::ChamferTwo {
            distance1,
            distance2,
        } => SectionKind::Chamfer(ChamferSpec::Distances {
            a: *distance1,
            b: *distance2,
        }),
        BlendKind::ChamferAngle { distance, angle } => {
            SectionKind::Chamfer(ChamferSpec::DistanceAngle {
                distance: *distance,
                angle: *angle,
            })
        }
        BlendKind::VariableFillet { .. } => {
            return Err(BlendError::Unsupported(VARIABLE_LATER.into()));
        }
    })
}

/// Kanten der Anfrage ohne Doppelte (die erste Nennung gilt, auch für `first_face`);
/// Stufe 1: genau eine.
fn select_edges(solid: &Solid, r: &BlendRequest) -> Result<Vec<BlendEdge>, BlendError> {
    let mut out: Vec<BlendEdge> = Vec::new();
    for be in &r.edges {
        if solid.edges.get(be.edge).is_none() {
            return Err(BlendError::EdgeNotFound);
        }
        if !out.iter().any(|o| o.edge == be.edge) {
            out.push(*be);
        }
    }
    if out.len() > 1 {
        return Err(BlendError::Unsupported(SEVERAL_LATER.into()));
    }
    Ok(out)
}

fn unsupported_pair(solid: &Solid, sides: &EdgeSides) -> BlendError {
    let label = |f: Id<Face>| {
        solid
            .faces
            .get(f)
            .map_or("unbekannte Fläche", |face| face.surface.kind().label())
    };
    BlendError::Unsupported(format!(
        "Die Verrundung zwischen {} und {} an dieser Kante wird noch nicht unterstützt",
        label(sides.a.face),
        label(sides.b.face)
    ))
}

fn edge_geom(
    solid: &Solid,
    be: &BlendEdge,
    kind: SectionKind,
    tol: &Tolerance,
) -> Result<EdgeGeom, BlendError> {
    let sides = edge_sides(solid, be.edge, be.first_face)?;
    let angle = edge_angle(solid, be.edge, &sides)?;
    analytic_geom(solid, be.edge, &sides, angle.convex, kind, tol)?
        .ok_or_else(|| unsupported_pair(solid, &sides))
}

/// Liegt jede Berührkurve in ihrer Fläche (Stichproben im Inneren)? Sonst ist das Maß zu groß
/// für diese Fläche (Spec §3.4). Kehren sich die gekürzten Berührkurven um, ebenso.
#[allow(clippy::neg_cmp_op_on_partial_ord)]
pub fn check_contacts(
    solid: &Solid,
    geoms: &[EdgeGeom],
    plan: &EndPlan,
    tol: &Tolerance,
) -> Result<(), BlendError> {
    if plan.starts.len() != geoms.len() || plan.ends.len() != geoms.len() {
        return Err(BlendError::ResultInvalid(
            "Kanten und Abschlüsse passen nicht zusammen".into(),
        ));
    }
    for (i, g) in geoms.iter().enumerate() {
        let (s, e) = (&plan.starts[i], &plan.ends[i]);
        let full = g.range.1 - g.range.0;
        for (u, curve, s0, s1) in [
            (g.sides.a, &g.contact_a, s.sa, e.sa),
            (g.sides.b, &g.contact_b, s.sb, e.sb),
        ] {
            let too_large = || BlendError::TooLarge {
                face: face_name(solid, u.face),
            };
            if !((s1 - s0) * full.signum() > 0.0) {
                return Err(too_large());
            }
            if solid.faces.get(u.face).is_none() {
                return Err(BlendError::EdgeNotFound);
            }
            let fg = face_geom(solid, u.face);
            for k in 1..CONTACT_PROBES {
                let t = s0 + (s1 - s0) * k as f64 / CONTACT_PROBES as f64;
                if point_in_face(&fg, curve.point(t), tol) == Containment::Outside {
                    return Err(too_large());
                }
            }
        }
    }
    Ok(())
}

/// Geschätzte Volumenänderung einer Kante (P2): Querschnittsfläche mal Weg der gekürzten
/// Berührkurven (Prisma), bei Kreiskanten mal Weg des Schwerpunkts (Pappus; die Parameter der
/// Berührkreise sind Winkel).
fn edge_volume(g: &EdgeGeom, s: &EndCut, e: &EndCut) -> f64 {
    let sign = if g.convex { -1.0 } else { 1.0 };
    let span = 0.5 * ((e.sa - s.sa).abs() + (e.sb - s.sb).abs());
    sign * match g.volume {
        VolumeModel::Prism { area } => area * span,
        VolumeModel::Pappus {
            area,
            centroid_radius,
        } => area * centroid_radius * span,
        VolumeModel::Total { volume } => volume,
    }
}

/// Rollen der neuen Flächen (P9).
fn roles(kind: &BlendKind, n: usize) -> Vec<String> {
    let label = if kind.is_chamfer() { "Fase" } else { "Rundung" };
    (1..=n).map(|i| format!("{label} {i}")).collect()
}

/// Rundet bzw. fast die Kanten der Anfrage. Der Eingangskörper bleibt unverändert; jedes
/// Scheitern ist eine benannte Ablehnung.
///
/// Reihenfolge: zuerst die billigen Prüfungen (Zahlen der Anfrage, Art, Kanten-Ids, mehrere
/// Schalen), dann die Gültigkeit des Körpers, erst danach die Geometrie.
pub fn blend(
    solid: &Solid,
    request: &BlendRequest,
    tol: &Tolerance,
) -> Result<Blended, BlendError> {
    check_request(request, tol)?;
    let kind = section_kind(&request.kind)?;
    let edges = select_edges(solid, request)?;
    if has_cavity(solid) {
        return Err(BlendError::Unsupported(CAVITY.into()));
    }
    validate_with(solid, tol).map_err(|e| BlendError::InvalidInput(e.to_string()))?;
    let geoms = edges
        .iter()
        .map(|be| edge_geom(solid, be, kind, tol))
        .collect::<Result<Vec<_>, _>>()?;
    let plan = end_cuts(solid, &geoms, tol)?;
    check_contacts(solid, &geoms, &plan, tol)?;
    let volume_change = geoms
        .iter()
        .enumerate()
        .map(|(i, g)| edge_volume(g, &plan.starts[i], &plan.ends[i]))
        .sum::<f64>()
        + plan.corner_volume;
    if !volume_change.is_finite() {
        return Err(BlendError::ResultInvalid(
            "Die Volumenschätzung ist keine endliche Zahl".into(),
        ));
    }
    let result = assemble(
        solid,
        &geoms,
        &plan,
        &roles(&request.kind, geoms.len()),
        tol,
    )?;
    Ok(Blended {
        solid: result,
        volume_change,
        edges: edges.iter().map(|e| e.edge).collect(),
    })
}
