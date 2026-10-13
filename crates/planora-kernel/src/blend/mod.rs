//! Rundungen und Fasen (Teilprojekt 8): rollende Kugel, analytische Rundungsflächen, Umbau der
//! Nachbarflächen über `sew` (Spec `2026-10-13-rundungen-und-fasen-design.md`).

pub mod analytic;
pub mod build;
pub mod chain;
pub mod corner;
pub mod error;
pub mod march;
pub mod section;
pub mod topo;

pub use error::BlendError;

use crate::boolean::{Containment, face_geom, pipeline_tol, point_in_face};
use crate::ids::Id;
use crate::topology::{Edge, Face, Solid};
use crate::validate::validate_with;
use analytic::{EdgeGeom, SectionKind, VolumeModel, analytic_geom};
use build::{CAVITY, EndCut, EndPlan, assemble, end_cuts, has_cavity};
use chain::{ChainedEdge, select_edges};
use march::{SectionLaw, march_geom};
use planora_math::nurbs::MAX_COORD;
use planora_math::{PI, RadiusLaw, Tolerance};
use section::ChamferSpec;
use topo::{edge_angle, edge_sides, face_name};

/// Höchstzahl Kanten je Anfrage (nach Ausdehnung der Tangentenkette).
pub const MAX_BLEND_EDGES: usize = 256;
/// Höchstzahl Punkte eines variablen Radius.
pub const MAX_RADIUS_POINTS: usize = 64;
/// Kleinster (und π minus größter) Fasenwinkel: 1°.
pub const MIN_CHAMFER_ANGLE: f64 = PI / 180.0;
/// Kleinstes Maß (Radius, Fasenabstand) als Vielfaches der Längentoleranz des Umbaus
/// (`build::blend_tol`: `tol.linear`, mit Freiformflächen die Knotentoleranz 2e-5 mm). Ohne
/// Freiformflächen sind das 1e-4 mm, mit ihnen 2e-3 mm. Begründung: `assemble` legt Ecken und
/// gekürzte Kanten bis `ON_CURVE_FACTOR · Toleranz` zusammen (`sew` selbst verschweißt Ecken bis
/// `4 · Toleranz`); bei Maßen um diese Weite schloss `sew` den Körper nicht mehr (Task-4-Probe:
/// 43 Fälle „nicht geschlossen“ bei 1e-5 mm, ab 1,5e-5 mm keiner). Der Faktor 100 hält eine
/// Größenordnung Abstand zu dieser Grenze; kleinere Maße sind in der Konstruktion bedeutungslos.
pub const MIN_BLEND_SIZE_FACTOR: f64 = 100.0;
/// Stellen je Berührkurve, an denen geprüft wird, ob sie in ihrer Fläche liegt.
const CONTACT_PROBES: usize = 8;

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
    /// Summe der Beträge der Schätzungen je Kante und Eckstück (mm³, ≥ |`volume_change`|).
    /// Werden konvexe und konkave Kanten zusammen gerundet, heben sich abgetragenes und
    /// hinzugefügtes Material in `volume_change` auf; die Volumenprobe misst ihren Spielraum
    /// deshalb an dieser Größe.
    pub volume_scale: f64,
    pub edges: Vec<Id<Edge>>,
}

/// Kleinstes erlaubtes Maß (Radius, Fasenabstand) in mm für Körper ohne Freiformflächen.
pub fn min_blend_size(tol: &Tolerance) -> f64 {
    MIN_BLEND_SIZE_FACTOR * tol.linear
}

/// Kleinstes erlaubtes Maß für diesen Körper: mit Freiformflächen oder -kanten nach der weiteren
/// Knotentoleranz (sonst gleich [`min_blend_size`]).
pub fn min_blend_size_for(solid: &Solid, tol: &Tolerance) -> f64 {
    MIN_BLEND_SIZE_FACTOR * build::blend_tol(solid, &[], tol).linear
}

/// Kleinstes erlaubtes Maß für diese Anfrage: Ein variabler Radius ergibt Hautflächen
/// (Freiform), der Umbau verschweißt dann mit der Knotentoleranz; deshalb gilt dort das
/// Mindestmaß für Freiformkörper (mindestens [`min_blend_size_for`]). Konstante Maße an Kanten
/// ohne analytische Klasse (ebenfalls Hautflächen) prüft die Geometrie je Kante mit demselben
/// Mindestmaß.
pub fn min_blend_size_request(solid: &Solid, kind: &BlendKind, tol: &Tolerance) -> f64 {
    let body = min_blend_size_for(solid, tol);
    match kind {
        BlendKind::VariableFillet { .. } => {
            body.max(MIN_BLEND_SIZE_FACTOR * pipeline_tol(tol, true).linear)
        }
        _ => body,
    }
}

/// Beträge ab hier (und unter [`SHOW_EXP_BELOW`]) zeigt [`show_num`] in Exponentenschreibweise.
const SHOW_EXP_FROM: f64 = 1e9;
/// Beträge unter dieser Grenze zeigt [`show_num`] in Exponentenschreibweise.
const SHOW_EXP_BELOW: f64 = 1e-5;
/// Gültige Ziffern nach der ersten in [`show_num`] (insgesamt sechs).
const SHOW_DIGITS: i32 = 5;
/// Höchstzahl Nachkommastellen in [`show_num`].
const SHOW_MAX_DECIMALS: i32 = 12;

/// Zahl für Meldungen: höchstens sechs gültige Ziffern ohne Gleitkomma-Rauschen; sehr kleine
/// und sehr große Werte in Exponentenschreibweise, nicht endliche als Wort.
pub(crate) fn show_num(x: f64) -> String {
    if x.is_nan() {
        return "keine Zahl".into();
    }
    if x.is_infinite() {
        return if x > 0.0 {
            "unendlich"
        } else {
            "minus unendlich"
        }
        .into();
    }
    let a = x.abs();
    if a == 0.0 {
        return "0".into();
    }
    let trim = |t: &str| -> String {
        if t.contains('.') {
            t.trim_end_matches('0').trim_end_matches('.').to_string()
        } else {
            t.to_string()
        }
    };
    if !(SHOW_EXP_BELOW..SHOW_EXP_FROM).contains(&a) {
        let t = format!("{x:.prec$e}", prec = SHOW_DIGITS as usize);
        let (m, e) = t.split_once('e').unwrap_or((&t, "0"));
        return format!("{}e{e}", trim(m));
    }
    let decimals = (SHOW_DIGITS - a.log10().floor() as i32).clamp(0, SHOW_MAX_DECIMALS) as usize;
    trim(&format!("{x:.decimals$}"))
}

/// Ein Maß muss endlich sein, mindestens `min` und höchstens `MAX_COORD` betragen.
#[allow(clippy::neg_cmp_op_on_partial_ord)]
fn check_size(x: f64, what: &str, min: f64) -> Result<(), BlendError> {
    if !(x.is_finite() && x <= MAX_COORD) {
        return Err(BlendError::InvalidParameter(format!(
            "{what} muss eine endliche Zahl bis {MAX_COORD:e} mm sein (war {})",
            show_num(x)
        )));
    }
    if x <= 0.0 {
        return Err(BlendError::InvalidParameter(format!(
            "{what} ist zu klein (muss größer als 0 sein)"
        )));
    }
    if !(x >= min) {
        return Err(BlendError::InvalidParameter(format!(
            "{what} ist zu klein (mindestens {} mm, war {})",
            show_num(min),
            show_num(x)
        )));
    }
    Ok(())
}

/// Prüft die Zahlen einer Anfrage (endlich, im Bereich, Punkte streng steigend) mit dem
/// Mindestmaß für Körper ohne Freiformflächen; `blend` prüft mit dem Mindestmaß des Körpers
/// ([`min_blend_size_for`]).
pub fn check_request(r: &BlendRequest, tol: &Tolerance) -> Result<(), BlendError> {
    check_request_min(r, min_blend_size(tol))
}

fn check_request_min(r: &BlendRequest, min: f64) -> Result<(), BlendError> {
    if r.edges.is_empty() {
        return Err(BlendError::NoEdges);
    }
    if r.edges.len() > MAX_BLEND_EDGES {
        return Err(BlendError::InvalidParameter(format!(
            "Zu viele Kanten (höchstens {MAX_BLEND_EDGES})"
        )));
    }
    check_kind_min(&r.kind, min)
}

/// Prüft die Maße einer Art (endlich, mindestens `min`, Punkte streng steigend, Winkel).
#[allow(clippy::neg_cmp_op_on_partial_ord)]
fn check_kind_min(kind: &BlendKind, min: f64) -> Result<(), BlendError> {
    match kind {
        BlendKind::Fillet { radius } => check_size(*radius, "Der Radius", min),
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
                check_size(*radius, "Ein Radius", min)?;
            }
            Ok(())
        }
        BlendKind::ChamferEqual { distance } => check_size(*distance, "Der Abstand", min),
        BlendKind::ChamferTwo {
            distance1,
            distance2,
        } => {
            check_size(*distance1, "Abstand 1", min)?;
            check_size(*distance2, "Abstand 2", min)
        }
        BlendKind::ChamferAngle { distance, angle } => {
            check_size(*distance, "Der Abstand", min)?;
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

/// Querschnittsart einer konstanten Verrundung (`None` beim variablen Radius).
fn section_kind(kind: &BlendKind) -> Option<SectionKind> {
    Some(match kind {
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
        BlendKind::VariableFillet { .. } => return None,
    })
}

/// Analytische Geometrie einer Kante (Einheitstest `build::tests::chain_plan`); Kanten ohne
/// analytische Klasse rechnet [`chained_geom`] über die Bahn.
#[cfg(test)]
fn edge_geom(
    solid: &Solid,
    be: &BlendEdge,
    kind: SectionKind,
    tol: &Tolerance,
) -> Result<EdgeGeom, BlendError> {
    let sides = edge_sides(solid, be.edge, be.first_face)?;
    let angle = edge_angle(solid, be.edge, &sides)?;
    analytic_geom(solid, be.edge, &sides, angle.convex, kind, tol)?.ok_or(BlendError::SpineLost)
}

/// Geometrie einer Kante der Anfrage: mit Radiengesetz über die Bahn mit Hautfläche (Task 14);
/// konstante Rundungen und Fasen analytisch, wenn die Kante einer analytischen Klasse angehört
/// (unverändert wie in Stufe 1 und 2), sonst – Freiformflächen und -kanten, übrige Flächenpaare –
/// ebenfalls über die Bahn (Task 15).
fn chained_geom(
    solid: &Solid,
    ce: &ChainedEdge,
    kind: &BlendKind,
    law: Option<&RadiusLaw>,
    tol: &Tolerance,
) -> Result<EdgeGeom, BlendError> {
    let sides = edge_sides(solid, ce.edge.edge, ce.edge.first_face)?;
    let angle = edge_angle(solid, ce.edge.edge, &sides)?;
    let Some(l) = law else {
        let sk = section_kind(kind).ok_or(BlendError::SpineLost)?;
        if let Some(g) = analytic_geom(solid, ce.edge.edge, &sides, angle.convex, sk, tol)? {
            return Ok(g);
        }
        // Die Bahn ergibt eine Hautfläche (Freiform): Umbau und Verschweißen dann mit der
        // Knotentoleranz, also das Mindestmaß für Freiformkörper (wie beim variablen Radius).
        check_kind_min(kind, MIN_BLEND_SIZE_FACTOR * pipeline_tol(tol, true).linear)?;
        let law = match sk {
            SectionKind::Fillet { radius } => SectionLaw::Fillet(radius),
            SectionKind::Chamfer(spec) => SectionLaw::Chamfer(spec),
        };
        return march_geom(solid, ce, &sides, angle.convex, law, tol);
    };
    march_geom(
        solid,
        ce,
        &sides,
        angle.convex,
        SectionLaw::Variable(l),
        tol,
    )
}

/// Geschlossene Ketten (Ende der letzten Kante = Anfang der ersten) brauchen am Kettenanfang und
/// -ende denselben Radius, sonst passt die Rundung dort nicht zusammen.
fn check_closed_chains(
    solid: &Solid,
    chained: &[ChainedEdge],
    law: &RadiusLaw,
    tol: &Tolerance,
) -> Result<(), BlendError> {
    let chains = chained.iter().map(|c| c.chain).max().map_or(0, |c| c + 1);
    for k in 0..chains {
        let members: Vec<&ChainedEdge> = chained.iter().filter(|c| c.chain == k).collect();
        let first = members.iter().min_by(|x, y| x.offset.total_cmp(&y.offset));
        let last = members.iter().max_by(|x, y| x.offset.total_cmp(&y.offset));
        let (Some(first), Some(last)) = (first, last) else {
            continue;
        };
        let (Some(e0), Some(e1)) = (
            solid.edges.get(first.edge.edge),
            solid.edges.get(last.edge.edge),
        ) else {
            return Err(BlendError::EdgeNotFound);
        };
        let start = if first.reversed { e0.end } else { e0.start };
        let end = if last.reversed { e1.start } else { e1.end };
        if members.len() > 1
            && start == end
            && (law.radius(0.0) - law.radius(1.0)).abs() > tol.linear
        {
            return Err(BlendError::InvalidParameter(
                "Auf einer geschlossenen Kette muss der Radius am Anfang und am Ende gleich sein"
                    .into(),
            ));
        }
    }
    Ok(())
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
/// Schalen), dann die Gültigkeit des Körpers, danach die Auswahl der Kanten (Tangentenkette,
/// Gruppierung in Ketten, Höchstzahl nach Ausdehnung) und erst dann die Geometrie. Neue Flächen
/// heißen nach der Reihenfolge der Kanten nach Ausdehnung der Tangentenkette (P9).
///
/// Arbeitsgrenze: [`march::MAX_BLEND_REQUEST_EVALUATIONS`] NURBS-Auswertungen je Anfrage (siehe
/// [`blend_limited`]).
pub fn blend(
    solid: &Solid,
    request: &BlendRequest,
    tol: &Tolerance,
) -> Result<Blended, BlendError> {
    blend_limited(solid, request, tol, march::MAX_BLEND_REQUEST_EVALUATIONS)
}

/// Wie [`blend`], mit eigener Arbeitsgrenze: Nach mehr als `max_evaluations` NURBS-Auswertungen
/// (`planora_math::nurbs::evaluations`, deterministisch) endet die Anfrage mit
/// [`BlendError::TooExpensive`]. Rein analytische Anfragen werten keine NURBS aus.
pub fn blend_limited(
    solid: &Solid,
    request: &BlendRequest,
    tol: &Tolerance,
    max_evaluations: u64,
) -> Result<Blended, BlendError> {
    let _scope = march::BudgetScope::start(max_evaluations);
    check_request_min(request, min_blend_size_request(solid, &request.kind, tol))?;
    let law = match &request.kind {
        BlendKind::VariableFillet { points } => {
            Some(RadiusLaw::new(points).map_err(|e| BlendError::InvalidParameter(e.to_string()))?)
        }
        _ => None,
    };
    if request
        .edges
        .iter()
        .any(|be| solid.edges.get(be.edge).is_none())
    {
        return Err(BlendError::EdgeNotFound);
    }
    if has_cavity(solid) {
        return Err(BlendError::Unsupported(CAVITY.into()));
    }
    validate_with(solid, tol).map_err(|e| BlendError::InvalidInput(e.to_string()))?;
    let chained = select_edges(solid, request)?;
    if let Some(l) = &law {
        check_closed_chains(solid, &chained, l, tol)?;
    }
    let geoms = chained
        .iter()
        .map(|ce| {
            let g = chained_geom(solid, ce, &request.kind, law.as_ref(), tol)?;
            march::check_budget()?;
            Ok(g)
        })
        .collect::<Result<Vec<_>, BlendError>>()?;
    let plan = end_cuts(solid, &geoms, tol)?;
    march::check_budget()?;
    check_contacts(solid, &geoms, &plan, tol)?;
    march::check_budget()?;
    let per_edge: Vec<f64> = geoms
        .iter()
        .enumerate()
        .map(|(i, g)| edge_volume(g, &plan.starts[i], &plan.ends[i]))
        .collect();
    let volume_change = per_edge.iter().sum::<f64>() + plan.corner_volume;
    let volume_scale = per_edge.iter().map(|v| v.abs()).sum::<f64>() + plan.corner_scale;
    if !(volume_change.is_finite() && volume_scale.is_finite()) {
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
        volume_scale,
        edges: chained.iter().map(|c| c.edge.edge).collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_in_messages_are_readable() {
        let tol = Tolerance::default();
        assert_eq!(show_num(min_blend_size(&tol)), "0.0001");
        assert_eq!(show_num(-1.0), "-1");
        assert_eq!(show_num(2.5), "2.5");
        assert_eq!(show_num(1e-300), "1e-300");
        assert_eq!(show_num(1e300), "1e300");
        assert_eq!(show_num(f64::NAN), "keine Zahl");
        assert_eq!(show_num(f64::NEG_INFINITY), "minus unendlich");
        assert_eq!(show_num(0.1 + 0.2), "0.3");
        assert_eq!(show_num(0.0), "0");
        let m = check_size(1e-300, "Der Radius", min_blend_size(&tol))
            .unwrap_err()
            .to_string();
        assert!(
            m.contains("mindestens 0.0001 mm") && m.contains("war 1e-300") && m.len() < 150,
            "{m}"
        );
    }
}
