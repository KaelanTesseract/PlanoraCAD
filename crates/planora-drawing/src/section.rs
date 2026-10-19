//! Schnitt eines Körpers an einer Ebene (Teilprojekt 3d, Schnittansichten), Teil 1: der
//! Kernel-Schnitt. Der Kernel bleibt unverändert; benutzt werden nur `make_box`,
//! `Solid::transformed`, `assign_origin` und `boolean`.
//!
//! Festlegungen:
//! - **S1 Ebene:** `SectionPlane { kind, offset }`. `kind` ist die Normansicht, `d = Basis::of(kind).d`
//!   ist ± eine Weltachse `a`; die Ebene ist `p[a] = offset`. Behalten wird
//!   `d[a]·(p[a] − offset) ≥ 0` (Material hinter der Ebene, vom Betrachter aus).
//! - **S2 Schneidekörper:** ein Quader um die Hüllbox des Körpers, der von der Ebene in Richtung
//!   `−d` reicht. Er bekommt die Herkunft `section_origin(body) = u64::MAX − body`; Schnittflächen
//!   erkennt man an dieser Marke (nicht an ihrer Lage). Trägt ein Eingangskörper schon eine Marke
//!   in diesem Bereich, ist das ein Fehler.
//! - **S3 Ganz vor/hinter, Tangentialfall:** Liegt die Hüllbox ganz auf der behaltenen (oder
//!   entfernten) Seite, einschließlich Berührung, braucht es keinen Kernel. Liegt eine Ecke näher
//!   als 1e-4 mm an der Ebene, wird vor dem Schneiden um `SECTION_NUDGE_MM` zur behaltenen Seite
//!   verschoben. Scheitert `boolean` sonst, folgt ein zweiter Versuch mit Verschiebung.

use crate::basis::Basis;
use crate::model::ViewKind;
use planora_kernel::boolean::boolean;
use planora_kernel::primitives::make_box;
use planora_kernel::{BoolOp, BooleanError, Face, Id, Solid};
use planora_math::{DMat4, DVec3, Tolerance};

/// Kleinste Herkunftsmarke, die für Schnittflächen reserviert ist.
pub const SECTION_ORIGIN_MIN: u64 = u64::MAX - 65_535;
/// Verschiebung der Ebene in Millimetern, wenn sie genau auf einer Fläche oder Kante liegt.
pub const SECTION_NUDGE_MM: f64 = 0.001;
/// Eine Ecke näher als dieser Abstand an der Ebene löst die Verschiebung aus.
const NEAR_MM: f64 = 1e-4;
/// Stützpunkte je Kante für die Hüllbox (wie bei der Ausdehnung der Striche).
const BOUNDS_SAMPLES: usize = 64;

/// Herkunftsmarke der Schnittflächen des Körpers mit dem Index `body`.
pub fn section_origin(body: usize) -> u64 {
    u64::MAX - body as u64
}

/// Schnittebene: Normansicht und Lage der Ebene auf der Weltachse.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SectionPlane {
    pub kind: ViewKind,
    pub offset: f64,
}

impl SectionPlane {
    /// Achse `a` der Ebene und Vorzeichen `d[a]` der Blickrichtung; `None` für die
    /// isometrische Ansicht oder einen nicht endlichen Versatz.
    pub fn axis(&self) -> Option<(usize, f64)> {
        if self.kind == ViewKind::Isometric || !self.offset.is_finite() {
            return None;
        }
        let d = Basis::of(self.kind).d;
        (0..3).find_map(|a| (d[a].abs() > 0.5).then(|| (a, d[a].signum())))
    }
}

/// Ergebnis des Schnitts eines Körpers.
#[derive(Clone, Debug)]
pub enum CutOutcome {
    /// Der Körper liegt ganz auf der behaltenen Seite.
    Kept,
    /// Der Körper liegt ganz auf der entfernten Seite (oder bleibt nichts übrig).
    Removed,
    /// Der geschnittene Körper; `nudged`: die Ebene wurde um `SECTION_NUDGE_MM` verschoben.
    Cut { solid: Solid, nudged: bool },
}

/// Hüllbox des Körpers aus Ecken und Stützpunkten der Kanten; `None` bei leerem Körper oder
/// nicht endlichen Werten.
pub fn solid_bounds(solid: &Solid) -> Option<(DVec3, DVec3)> {
    let mut lo = DVec3::splat(f64::INFINITY);
    let mut hi = DVec3::splat(f64::NEG_INFINITY);
    for (_, v) in solid.vertices.iter() {
        lo = lo.min(v.point);
        hi = hi.max(v.point);
    }
    for (_, e) in solid.edges.iter() {
        for i in 0..BOUNDS_SAMPLES {
            let t = e.t0 + (e.t1 - e.t0) * i as f64 / (BOUNDS_SAMPLES - 1) as f64;
            let p = e.curve.point(t);
            lo = lo.min(p);
            hi = hi.max(p);
        }
    }
    (lo.is_finite() && hi.is_finite()).then_some((lo, hi))
}

/// Flächen mit Herkunft ab `SECTION_ORIGIN_MIN`, aufsteigend nach Id.
pub fn cut_faces(solid: &Solid) -> Vec<Id<Face>> {
    solid
        .faces
        .iter()
        .filter(|(_, f)| {
            f.tag
                .as_ref()
                .is_some_and(|t| t.origin >= SECTION_ORIGIN_MIN)
        })
        .map(|(id, _)| id)
        .collect()
}

/// Schneidet `solid` (der `body`-te exakte Körper) an `plane` und behält die Seite nach S1.
pub fn cut_body(
    solid: &Solid,
    body: usize,
    plane: &SectionPlane,
    tol: &Tolerance,
) -> Result<CutOutcome, String> {
    let (a, da) = plane
        .axis()
        .ok_or_else(|| "keine gültige Schnittebene".to_string())?;
    if body as u64 > u64::MAX - SECTION_ORIGIN_MIN {
        return Err("zu viele Körper für die Schnittmarken".into());
    }
    if !cut_faces(solid).is_empty() {
        return Err("der Körper trägt schon eine Schnittmarke".into());
    }
    let (lo, hi) =
        solid_bounds(solid).ok_or_else(|| "leerer oder ungültiger Körper".to_string())?;

    let side = |off: f64| classify(lo[a], hi[a], da, off);
    match side(plane.offset) {
        Side::Kept => return Ok(CutOutcome::Kept),
        Side::Removed => return Ok(CutOutcome::Removed),
        Side::Cut => {}
    }

    let near = solid
        .vertices
        .iter()
        .any(|(_, v)| (v.point[a] - plane.offset).abs() < NEAR_MM);
    let mut nudged = near;
    let mut offset = plane.offset;
    if nudged {
        offset += da * SECTION_NUDGE_MM;
    }
    loop {
        match side(offset) {
            Side::Kept => return Ok(CutOutcome::Kept),
            Side::Removed => return Ok(CutOutcome::Removed),
            Side::Cut => {}
        }
        let cutter = cutter_box(lo, hi, a, da, offset, body)?;
        match boolean(solid, &cutter, BoolOp::Difference, tol) {
            Ok(out) => return Ok(CutOutcome::Cut { solid: out, nudged }),
            Err(BooleanError::EmptyResult) => return Ok(CutOutcome::Removed),
            Err(BooleanError::InvalidInput(m)) => return Err(m),
            Err(e) => {
                if nudged {
                    return Err(e.to_string());
                }
                nudged = true;
                offset = plane.offset + da * SECTION_NUDGE_MM;
            }
        }
    }
}

enum Side {
    Kept,
    Removed,
    Cut,
}

/// Ordnet die Spanne `[lo, hi]` auf der Achse der Ebene ein (Berührung zählt als ganz auf
/// einer Seite).
fn classify(lo: f64, hi: f64, da: f64, offset: f64) -> Side {
    let (smin, smax) = if da > 0.0 {
        (lo - offset, hi - offset)
    } else {
        (offset - hi, offset - lo)
    };
    if smin >= 0.0 {
        Side::Kept
    } else if smax <= 0.0 {
        Side::Removed
    } else {
        Side::Cut
    }
}

/// Schneidequader S2: quer zur Achse um die Mitte der Hüllbox, längs der Achse von der Ebene in
/// Richtung `−d`.
fn cutter_box(
    lo: DVec3,
    hi: DVec3,
    a: usize,
    da: f64,
    offset: f64,
    body: usize,
) -> Result<Solid, String> {
    let center = (lo + hi) * 0.5;
    let size = hi - lo;
    let mut dims = DVec3::ZERO;
    let mut min = DVec3::ZERO;
    for k in 0..3 {
        if k == a {
            let len = 2.0 * size[k].max(1.0) + (offset - center[k]).abs();
            dims[k] = len;
            min[k] = if da > 0.0 { offset - len } else { offset };
        } else {
            let half = size[k].max(1.0);
            dims[k] = 2.0 * half;
            min[k] = center[k] - half;
        }
    }
    let mut b = make_box(dims, &Tolerance::default())
        .map_err(|e| e.to_string())?
        .transformed(&DMat4::from_translation(min))
        .map_err(|e| e.to_string())?;
    b.assign_origin(section_origin(body));
    Ok(b)
}
