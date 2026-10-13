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
use crate::topology::{Edge, Face, Solid, Vertex};
use crate::validate::validate_with;
use analytic::{EdgeGeom, SectionKind, VolumeModel, analytic_geom};
use build::{CAVITY, EndCut, EndPlan, assemble, end_cuts, has_cavity};
use planora_math::nurbs::MAX_COORD;
use planora_math::{DVec3, PI, Tolerance};
use section::ChamferSpec;
use topo::{
    EdgeSides, NEAR_TANGENT_ANGLE, edge_angle, edge_sides, edge_uses, face_name, vertex_edges,
};

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

/// Kleinstes erlaubtes Maß (Radius, Fasenabstand) in mm für Körper ohne Freiformflächen.
pub fn min_blend_size(tol: &Tolerance) -> f64 {
    MIN_BLEND_SIZE_FACTOR * tol.linear
}

/// Kleinstes erlaubtes Maß für diesen Körper: mit Freiformflächen oder -kanten nach der weiteren
/// Knotentoleranz (sonst gleich [`min_blend_size`]).
pub fn min_blend_size_for(solid: &Solid, tol: &Tolerance) -> f64 {
    MIN_BLEND_SIZE_FACTOR * build::blend_tol(solid, &[], tol).linear
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
            "Das Maß ist zu klein: {what} muss größer als 0 sein"
        )));
    }
    if !(x >= min) {
        return Err(BlendError::InvalidParameter(format!(
            "Das Maß ist zu klein: {what} muss mindestens {} mm betragen (war {})",
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

#[allow(clippy::neg_cmp_op_on_partial_ord)]
fn check_request_min(r: &BlendRequest, min: f64) -> Result<(), BlendError> {
    if r.edges.is_empty() {
        return Err(BlendError::NoEdges);
    }
    if r.edges.len() > MAX_BLEND_EDGES {
        return Err(BlendError::InvalidParameter(format!(
            "Zu viele Kanten (höchstens {MAX_BLEND_EDGES})"
        )));
    }
    match &r.kind {
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
                check_size(*radius, "Jeder Radius", min)?;
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

/// Einheitsrichtung, in der die Kante `e` die Ecke `v` verlässt (für beide Enden, wenn die
/// Kante dort anfängt und endet).
fn leaving_directions(solid: &Solid, e: Id<Edge>, v: Id<Vertex>) -> Vec<DVec3> {
    let Some(edge) = solid.edges.get(e) else {
        return Vec::new();
    };
    let sign = (edge.t1 - edge.t0).signum();
    let mut out = Vec::new();
    if edge.start == v {
        out.push((edge.curve.tangent(edge.t0) * sign).normalize_or_zero());
    }
    if edge.end == v {
        out.push((edge.curve.tangent(edge.t1) * -sign).normalize_or_zero());
    }
    out
}

/// Mit „Tangentenkette“: Läuft die Kante an einer Ecke glatt (Tangenten fast entgegengesetzt,
/// Schranke wie `NEAR_TANGENT_ANGLE`) in eine weitere Kante an einer ihrer Flächen weiter, ist
/// sie Teil einer Kette. Stufe 1 rundet keine Ketten; statt still nur diese eine Kante zu runden
/// (und später, mit Task 9, einen anderen Körper zu liefern) wird benannt abgelehnt.
fn check_no_chain(solid: &Solid, e: Id<Edge>) -> Result<(), BlendError> {
    let Some(edge) = solid.edges.get(e) else {
        return Err(BlendError::EdgeNotFound);
    };
    let faces: Vec<Id<Face>> = edge_uses(solid, e).iter().map(|u| u.face).collect();
    let limit = -NEAR_TANGENT_ANGLE.cos();
    for v in [edge.start, edge.end] {
        let own = leaving_directions(solid, e, v);
        for other in vertex_edges(solid, v).into_iter().filter(|o| *o != e) {
            let shares = edge_uses(solid, other)
                .iter()
                .any(|u| faces.contains(&u.face));
            let smooth = leaving_directions(solid, other, v)
                .iter()
                .any(|d| own.iter().any(|o| o.dot(*d) < limit));
            if shares && smooth {
                let names: Vec<String> = faces.iter().map(|f| face_name(solid, *f)).collect();
                return Err(BlendError::Unsupported(format!(
                    "Die Kante zwischen „{}“ läuft glatt in weitere Kanten weiter (Tangentenkette); Ketten werden erst in Stufe 2 unterstützt – Schalter „Tangentenkette“ ausschalten",
                    names.join("“ und „")
                )));
            }
        }
    }
    Ok(())
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
    check_request_min(request, min_blend_size_for(solid, tol))?;
    let kind = section_kind(&request.kind)?;
    let edges = select_edges(solid, request)?;
    if request.tangent_chain {
        for be in &edges {
            check_no_chain(solid, be.edge)?;
        }
    }
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
