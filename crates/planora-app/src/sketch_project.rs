//! Fusion „Projizieren“ (Skizzen 6): Auswahlfilter „Angegebene Elemente“ / „Körper“, Schalter
//! „Projektionsverknüpfung“ und das Anlegen der projizierten Kurven in einem Schritt. Welche
//! Kanten eine Kante, Fläche oder ein Körper liefert, bestimmt [`crate::state::AppState`]; hier
//! werden die Kurvenstücke in die Skizze gelegt, Kanten, die nur ein Punkt wären, übergangen und
//! deckungsgleiche Projektionen nur einmal angelegt (auch gegen schon vorhandene Projektionen).

use crate::sketch_state::{SketchSession, err};
use planora_feature::CopyTarget;
use planora_kernel::Curve;
use planora_math::DVec2;
use planora_sketch::helpers::{Included, ProjectStyle};
use planora_sketch::model::{CurveId, CurveKind, Sketch};
use std::f64::consts::{FRAC_PI_2, TAU};

/// Auswahlfilter des Projizierens (Fusion „Auswahlfilter“).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ProjectFilter {
    /// „Angegebene Elemente“: Kante oder Fläche unter dem Zeiger.
    #[default]
    Entities,
    /// „Körper“: der ganze Körper unter dem Zeiger.
    Bodies,
}

/// Einstellungen des Projizierens im Skizzenfeld; gelten für die Sitzung.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProjectState {
    pub filter: ProjectFilter,
    /// Fusion „Projektionsverknüpfung“: Die Kurven folgen dem Körper (Vorgabe an).
    pub link: bool,
}

impl Default for ProjectState {
    fn default() -> ProjectState {
        ProjectState {
            filter: ProjectFilter::Entities,
            link: true,
        }
    }
}

/// Neue Projektionen sind normale, profilfähige Geometrie (wie Fusion).
const PROJECTED: ProjectStyle = ProjectStyle {
    construction: false,
};

/// Ergebnis von [`SketchSession::project_pieces`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProjectOutcome {
    /// Je neue Kurve der Index ihres Stücks und die Kurve.
    pub added: Vec<(usize, CurveId)>,
    /// Freiformkurven, deren Projektion fehlschlug.
    pub skipped_freeform: usize,
    /// Andere Stücke, die sich nicht projizieren lassen.
    pub skipped_other: usize,
}

impl SketchSession {
    /// Legt die Kurvenstücke `pieces` (Kurve, Anfang, Ende) als feste, normale Kurven in die
    /// Skizze, alles in einem Schritt. Stücke, die nur ein Punkt wären, entfallen; eine
    /// Projektion, die mit einer Kurve aus `known` oder einer schon in diesem Aufruf angelegten
    /// deckungsgleich ist, ebenfalls. Stücke, die nicht gehen, zählen als übersprungen (Freiform
    /// und andere getrennt).
    /// `single`: Es geht um genau eine gewählte Kante; dann ist jeder Fehler ein Fehler (mit
    /// seinem Grund in der Meldung), wie früher beim Kopieren einer Kante. Ohne neue Kurve bleibt
    /// die Skizze unverändert.
    pub fn project_pieces(
        &mut self,
        sk: &mut Sketch,
        pieces: &[(Curve, f64, f64)],
        known: &[CurveId],
        single: bool,
    ) -> Result<ProjectOutcome, String> {
        self.atomic(sk, |se, sk, prior| {
            let tol = sk.tolerance().linear;
            let mut keys: Vec<CurveKey> = known.iter().filter_map(|&c| key_of(sk, c)).collect();
            // Probe in einer leeren Skizze derselben Ebene: So steht die Lage einer Projektion
            // fest, bevor sie in die Skizze kommt.
            let mut probe = Sketch::new(*sk.frame(), "", *sk.tolerance());
            let mut out = ProjectOutcome::default();
            for (i, (curve, t0, t1)) in pieces.iter().enumerate() {
                if !single && sk.projects_to_point(curve, *t0, *t1) {
                    continue;
                }
                let key = match probe.add_projected_curve(curve, *t0, *t1, PROJECTED) {
                    Ok(c) => key_of(&probe, c),
                    Err(e) if single => return Err(err(e)),
                    Err(_) if matches!(curve, Curve::Nurbs(_)) => {
                        out.skipped_freeform += 1;
                        continue;
                    }
                    Err(_) => {
                        out.skipped_other += 1;
                        continue;
                    }
                };
                if key.is_some_and(|k| keys.iter().any(|o| o.same(&k, tol))) {
                    continue;
                }
                let c = sk
                    .add_projected_curve(curve, *t0, *t1, PROJECTED)
                    .map_err(err)?;
                keys.extend(key);
                out.added.push((i, c));
            }
            if !out.added.is_empty() {
                se.solve_checked(sk, prior)?;
            }
            Ok(out)
        })
    }
}

impl SketchSession {
    /// Legt die einbezogenen Stücke `pieces` (Skizzen 10, Spec 5) als feste, verknüpfbare
    /// Geometrie in die Skizze ([`Sketch::add_included`]), alles in einem Schritt; liefert je
    /// Stück sein Ziel (Kurve oder Punkt) in derselben Reihenfolge. Lehnt die Skizze ein Stück ab,
    /// bleibt sie unverändert, und die Meldung nennt den Grund.
    pub fn include_pieces(
        &mut self,
        sk: &mut Sketch,
        pieces: &[Included],
    ) -> Result<Vec<CopyTarget>, String> {
        self.atomic(sk, |se, sk, prior| {
            let mut out = Vec::with_capacity(pieces.len());
            for g in pieces {
                let ids = sk.add_included(g).map_err(err)?;
                match (ids.curves.as_slice(), ids.points.as_slice()) {
                    ([c], []) => out.push(CopyTarget::Curve(*c)),
                    ([], [p]) => out.push(CopyTarget::Point(*p)),
                    _ => return Err(crate::msg!("sketch-include-failed")),
                }
            }
            se.solve_checked(sk, prior)?;
            Ok(out)
        })
    }
}

/// Lage einer Kurve zum Vergleich: Linie über ihre Enden, Kreis über Mitte und Radius, Bögen über
/// Anfang, Mitte des Bogens und Ende, Ellipse über Mitte und die Enden ihrer Halbachsen, feste Splines
/// über Anfang, Mitte des Bereichs und Ende.
#[derive(Clone, Copy, Debug)]
enum CurveKey {
    Line(DVec2, DVec2),
    Circle(DVec2, f64),
    Arc([DVec2; 3]),
    Ellipse([DVec2; 3]),
    EllipticArc([DVec2; 3]),
    Spline([DVec2; 3]),
}

impl CurveKey {
    /// Gleiche Art und Lage innerhalb `tol` (Richtung egal).
    fn same(&self, other: &CurveKey, tol: f64) -> bool {
        let near = |a: DVec2, b: DVec2| a.distance(b) <= tol;
        match (self, other) {
            (CurveKey::Line(a, b), CurveKey::Line(c, d)) => {
                (near(*a, *c) && near(*b, *d)) || (near(*a, *d) && near(*b, *c))
            }
            (CurveKey::Circle(m, r), CurveKey::Circle(n, s)) => {
                near(*m, *n) && (r - s).abs() <= tol
            }
            (CurveKey::Arc(a), CurveKey::Arc(b))
            | (CurveKey::EllipticArc(a), CurveKey::EllipticArc(b))
            | (CurveKey::Spline(a), CurveKey::Spline(b)) => {
                near(a[1], b[1])
                    && ((near(a[0], b[0]) && near(a[2], b[2]))
                        || (near(a[0], b[2]) && near(a[2], b[0])))
            }
            (CurveKey::Ellipse([m, a, b]), CurveKey::Ellipse([n, c, d])) => {
                // Die Halbachsen als Vektoren von der Mitte, Vorzeichen egal.
                let axis = |x: DVec2, y: DVec2| {
                    let (u, v) = (x - *m, y - *n);
                    near(u, v) || near(u, -v)
                };
                near(*m, *n) && axis(*a, *c) && axis(*b, *d)
            }
            _ => false,
        }
    }
}

/// Die Lage der lebenden Kurve `c` zum Vergleich ([`CurveKey`]); freie Splines vergleicht niemand.
fn key_of(sk: &Sketch, c: CurveId) -> Option<CurveKey> {
    let pos = |p| sk.point_xy(p).ok();
    Some(match &sk.curve(c)?.kind {
        CurveKind::Line { a, b } => CurveKey::Line(pos(*a)?, pos(*b)?),
        CurveKind::Circle { center, .. } => CurveKey::Circle(pos(*center)?, sk.radius(c)?),
        CurveKind::Arc { center, start, end } => {
            let (m, s, e) = (pos(*center)?, pos(*start)?, pos(*end)?);
            let a0 = (s - m).y.atan2((s - m).x);
            let a1 = (e - m).y.atan2((e - m).x);
            let half = sweep(a1 - a0, sk) / 2.0;
            let mid = m + s.distance(m) * DVec2::new((a0 + half).cos(), (a0 + half).sin());
            CurveKey::Arc([s, mid, e])
        }
        CurveKind::Ellipse { center, .. } => CurveKey::Ellipse([
            pos(*center)?,
            sk.ellipse_point(c, 0.0)?,
            sk.ellipse_point(c, FRAC_PI_2)?,
        ]),
        CurveKind::EllipticArc { start, end, .. } => {
            let (s, e) = (pos(*start)?, pos(*end)?);
            let t0 = sk.ellipse_param_of(c, s)?;
            let t1 = sk.ellipse_param_of(c, e)?;
            let mid = sk.ellipse_point(c, t0 + sweep(t1 - t0, sk) / 2.0)?;
            CurveKey::EllipticArc([s, mid, e])
        }
        CurveKind::FixedSpline { start, end, .. } => {
            let (lo, hi) = sk.curve_domain(c)?;
            CurveKey::Spline([pos(*start)?, sk.curve_eval(c, 0.5 * (lo + hi))?, pos(*end)?])
        }
        CurveKind::Spline { .. } | CurveKind::ControlSpline { .. } | CurveKind::Conic { .. } => {
            return None;
        }
    })
}

/// Überstrichener Winkel gegen den Uhrzeigersinn; Anfang gleich Ende ist ein voller Umlauf (wie
/// beim Linienzug der Skizze).
fn sweep(delta: f64, sk: &Sketch) -> f64 {
    let s = delta.rem_euclid(TAU);
    if s <= sk.tolerance().angular { TAU } else { s }
}
