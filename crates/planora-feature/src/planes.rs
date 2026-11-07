//! Konstruktionsebenen (Skizzen 6): Versatzebene, Ebene unter Winkel, Mittelebene, Ebene durch
//! drei Punkte und Tangentialebene als eigene Features wie in Autodesk Fusion. Eine Ebene erzeugt
//! keinen Körper, nur ein Koordinatensystem ([`Frame`]); Skizzen können später darauf liegen.
//!
//! Die Rechnung ist rein: Sie liest die sichtbaren Körper und berechneten Skizzen und Ebenen
//! früherer Features, ändert nichts.

use crate::error::FeatureError;
use crate::model::{Document, FeatureId, StandardPlane};
use crate::refs::{EdgeRef, FaceRef, first_face, resolve_edge, resolve_face};
use planora_i18n::tr;
use planora_kernel::{Curve, Solid, Surface};
use planora_math::{DQuat, DVec3, Frame, Tolerance};
use planora_sketch::{CurveId, CurveKind, PointId, plane_of_face};

/// Bezug einer Ebene: Grundebene, ebene Körperfläche oder eine frühere Konstruktionsebene.
#[derive(Clone, Debug, PartialEq)]
pub enum PlaneBase {
    Standard(StandardPlane),
    Face(FaceRef),
    Plane(FeatureId),
}

/// Achse des Ursprungs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StdAxis {
    X,
    Y,
    Z,
}

/// Drehachse einer Ebene unter Winkel: Ursprungsachse, gerade Körperkante oder Skizzenlinie.
#[derive(Clone, Debug, PartialEq)]
pub enum AxisRef {
    Standard(StdAxis),
    Edge(EdgeRef),
    SketchLine { sketch: FeatureId, curve: CurveId },
}

/// Punkt einer Ebene durch drei Punkte: Ursprung, Anfang (`end == false`) oder Ende einer
/// Körperkante, Skizzenpunkt.
#[derive(Clone, Debug, PartialEq)]
pub enum PointRef {
    Origin,
    EdgeEnd { edge: EdgeRef, end: bool },
    SketchPoint { sketch: FeatureId, point: PointId },
}

/// Art und Eingaben einer Konstruktionsebene. Längen in mm, Winkel in Grad (wie die anderen
/// Winkelfelder, etwa der Fasenwinkel).
#[derive(Clone, Debug, PartialEq)]
#[allow(
    clippy::large_enum_variant,
    reason = "few planes per document; the fixed array of three points keeps the interface simple"
)]
pub enum PlaneDef {
    /// Parallel zur Basis im Abstand `distance` entlang ihrer Normalen.
    Offset { base: PlaneBase, distance: f64 },
    /// Durch die Achse, um `angle` gegen die Bezugsebene der Achse gedreht.
    Angle { axis: AxisRef, angle: f64 },
    /// Mittig zwischen zwei Ebenen (parallel: in der Mitte, sonst Winkelhalbierende).
    Mid { a: PlaneBase, b: PlaneBase },
    /// Durch drei Punkte; die x-Achse zeigt vom ersten zum zweiten.
    ThreePoints([PointRef; 3]),
    /// Tangential an einen Zylinder, Kegel oder eine Kugel; der Berührpunkt liegt dort, wo die
    /// Normale der Basis, um `angle` um die Achse der Fläche gedreht, die Fläche trifft.
    Tangent {
        face: FaceRef,
        base: PlaneBase,
        angle: f64,
    },
}

/// Feature „Ebene“ (Konstruktionsebene).
#[derive(Clone, Debug, PartialEq)]
pub struct PlaneFeature {
    pub def: PlaneDef,
}

impl PlaneBase {
    fn dependencies(&self, out: &mut Vec<FeatureId>) {
        match self {
            PlaneBase::Standard(_) => {}
            PlaneBase::Face(r) => out.push(FeatureId(r.origin)),
            PlaneBase::Plane(id) => out.push(*id),
        }
    }

    fn reads_bodies(&self) -> bool {
        matches!(self, PlaneBase::Face(_))
    }
}

fn edge_dependencies(e: &EdgeRef, out: &mut Vec<FeatureId>) {
    out.push(FeatureId(e.a.origin));
    out.push(FeatureId(e.b.origin));
}

impl PointRef {
    fn dependencies(&self, out: &mut Vec<FeatureId>) {
        match self {
            PointRef::Origin => {}
            PointRef::EdgeEnd { edge, .. } => edge_dependencies(edge, out),
            PointRef::SketchPoint { sketch, .. } => out.push(*sketch),
        }
    }
}

impl PlaneFeature {
    /// Features, auf die die Ebene verweist (unsortiert, mit Doppelten).
    pub fn dependencies(&self) -> Vec<FeatureId> {
        let mut d = Vec::new();
        match &self.def {
            PlaneDef::Offset { base, .. } => base.dependencies(&mut d),
            PlaneDef::Angle { axis, .. } => match axis {
                AxisRef::Standard(_) => {}
                AxisRef::Edge(e) => edge_dependencies(e, &mut d),
                AxisRef::SketchLine { sketch, .. } => d.push(*sketch),
            },
            PlaneDef::Mid { a, b } => {
                a.dependencies(&mut d);
                b.dependencies(&mut d);
            }
            PlaneDef::ThreePoints(points) => {
                for p in points {
                    p.dependencies(&mut d);
                }
            }
            PlaneDef::Tangent { face, base, .. } => {
                d.push(FeatureId(face.origin));
                base.dependencies(&mut d);
            }
        }
        d
    }

    /// Liest die Ebene Flächen oder Kanten der sichtbaren Körper? Dann hängt ihr Ergebnis von
    /// allen sichtbaren Körpern ab (eine Fläche kann nach einer Booleschen Operation in einem
    /// anderen Körper liegen).
    pub(crate) fn reads_bodies(&self) -> bool {
        match &self.def {
            PlaneDef::Offset { base, .. } => base.reads_bodies(),
            PlaneDef::Angle { axis, .. } => matches!(axis, AxisRef::Edge(_)),
            PlaneDef::Mid { a, b } => a.reads_bodies() || b.reads_bodies(),
            PlaneDef::ThreePoints(points) => {
                points.iter().any(|p| matches!(p, PointRef::EdgeEnd { .. }))
            }
            PlaneDef::Tangent { .. } => true,
        }
    }

    /// Alle Zahlen (für die Bereichsprüfung).
    pub(crate) fn numbers(&self) -> Vec<f64> {
        match &self.def {
            PlaneDef::Offset { distance, .. } => vec![*distance],
            PlaneDef::Angle { angle, .. } | PlaneDef::Tangent { angle, .. } => vec![*angle],
            PlaneDef::Mid { .. } | PlaneDef::ThreePoints(_) => Vec::new(),
        }
    }
}

impl Document {
    /// Koordinatensystem der Konstruktionsebene `id`; nur bei Status `Ok`, sonst (und für
    /// andere Features) `None`.
    pub fn plane_frame(&self, id: FeatureId) -> Option<Frame> {
        let f = self.feature(id)?;
        if f.status != crate::model::FeatureStatus::Ok {
            return None;
        }
        f.result.as_ref()?.plane
    }
}

/// Löst den Bezug einer Ebene auf: Grundebene, ebene Fläche eines der `solids` (Normale nach
/// außen) oder die berechnete Ebene eines früheren Features.
pub fn resolve_plane_base(
    doc: &Document,
    base: &PlaneBase,
    solids: &[&Solid],
    tol: &Tolerance,
) -> Result<Frame, FeatureError> {
    match base {
        PlaneBase::Standard(p) => Ok(p.frame()),
        PlaneBase::Face(r) => {
            let found = resolve_face(solids, r, tol)?;
            plane_of_face(solids[found.body], found.face)
                .ok_or_else(|| FeatureError::InvalidInput(tr!("feature-plane-face-not-flat")))
        }
        PlaneBase::Plane(id) => doc.plane_frame(*id).ok_or_else(|| {
            let name = doc
                .feature(*id)
                .map_or_else(String::new, |f| f.name.clone());
            FeatureError::InvalidInput(tr!("feature-plane-not-plane", name = name))
        }),
    }
}

/// Berechnet das Koordinatensystem der Ebene aus den sichtbaren Körpern `solids`.
pub(crate) fn compute_plane(
    doc: &Document,
    pf: &PlaneFeature,
    solids: &[&Solid],
    tol: &Tolerance,
) -> Result<Frame, FeatureError> {
    match &pf.def {
        PlaneDef::Offset { base, distance } => {
            let f = resolve_plane_base(doc, base, solids, tol)?;
            Ok(Frame {
                origin: f.origin + f.z * *distance,
                ..f
            })
        }
        PlaneDef::Angle { axis, angle } => {
            let (origin, dir, reference) = resolve_axis(doc, axis, solids, tol)?;
            Ok(angle_frame(origin, dir, reference, *angle))
        }
        PlaneDef::Mid { a, b } => {
            let fa = resolve_plane_base(doc, a, solids, tol)?;
            let fb = resolve_plane_base(doc, b, solids, tol)?;
            mid_frame(&fa, &fb, tol)
        }
        PlaneDef::ThreePoints(points) => {
            let mut p = [DVec3::ZERO; 3];
            for (slot, r) in p.iter_mut().zip(points) {
                *slot = resolve_point(doc, r, solids, tol)?;
            }
            three_point_frame(p, tol)
        }
        PlaneDef::Tangent { face, base, angle } => {
            let reference = resolve_plane_base(doc, base, solids, tol)?;
            let found = resolve_face(solids, face, tol)?;
            let surface = &solids[found.body].faces[found.face].surface;
            tangent_frame(surface, reference.z, *angle, tol)
        }
    }
}

/// Achse als Punkt, Einheitsrichtung und Normale der Bezugsebene durch die Achse (Kante: erste
/// angrenzende ebene Fläche, X/Y: XY, Z: XZ, Skizzenlinie: Skizzenebene).
fn resolve_axis(
    doc: &Document,
    axis: &AxisRef,
    solids: &[&Solid],
    tol: &Tolerance,
) -> Result<(DVec3, DVec3, DVec3), FeatureError> {
    match axis {
        AxisRef::Standard(a) => Ok(match a {
            StdAxis::X => (DVec3::ZERO, DVec3::X, StandardPlane::XY.frame().z),
            StdAxis::Y => (DVec3::ZERO, DVec3::Y, StandardPlane::XY.frame().z),
            StdAxis::Z => (DVec3::ZERO, DVec3::Z, StandardPlane::XZ.frame().z),
        }),
        AxisRef::Edge(e) => {
            let found = resolve_edge(solids, e, tol)?;
            let solid = solids[found.body];
            let edge = &solid.edges[found.edge];
            if !matches!(edge.curve, Curve::Line { .. }) {
                return Err(FeatureError::InvalidInput(tr!("feature-plane-not-line")));
            }
            let (p0, p1) = (
                solid.vertices[edge.start].point,
                solid.vertices[edge.end].point,
            );
            let dir = (p1 - p0).normalize_or_zero();
            if dir == DVec3::ZERO {
                return Err(FeatureError::InvalidInput(tr!("feature-plane-not-line")));
            }
            let reference = [&e.a, &e.b]
                .into_iter()
                .filter_map(|r| first_face(solid, found.edge, r))
                .find_map(|f| plane_of_face(solid, f))
                .ok_or_else(|| FeatureError::InvalidInput(tr!("feature-plane-no-reference")))?;
            Ok((p0, dir, reference.z))
        }
        AxisRef::SketchLine { sketch, curve } => {
            let sk = computed_sketch(doc, *sketch)?;
            let Some(CurveKind::Line { a, b }) = sk.curve(*curve).map(|c| c.kind.clone()) else {
                return Err(FeatureError::InvalidInput(tr!("feature-plane-not-line")));
            };
            let frame = *sk.frame();
            let at = |p: PointId| -> Result<DVec3, FeatureError> {
                let xy = sk
                    .point_xy(p)
                    .map_err(|_| FeatureError::InvalidInput(tr!("feature-plane-not-line")))?;
                Ok(frame.point(xy.x, xy.y, 0.0))
            };
            let (p0, p1) = (at(a)?, at(b)?);
            let dir = (p1 - p0).normalize_or_zero();
            if dir == DVec3::ZERO {
                return Err(FeatureError::InvalidInput(tr!("feature-plane-not-line")));
            }
            Ok((p0, dir, frame.z))
        }
    }
}

/// Berechnete Skizze des Features `id` (Fehler, wenn es keine Skizze ist).
fn computed_sketch(doc: &Document, id: FeatureId) -> Result<&planora_sketch::Sketch, FeatureError> {
    doc.sketch_of(id).ok_or_else(|| {
        let name = doc.feature(id).map_or_else(String::new, |f| f.name.clone());
        FeatureError::InvalidInput(tr!("feature-not-sketch", name = name))
    })
}

/// Lage eines Punktes in Weltkoordinaten.
fn resolve_point(
    doc: &Document,
    r: &PointRef,
    solids: &[&Solid],
    tol: &Tolerance,
) -> Result<DVec3, FeatureError> {
    match r {
        PointRef::Origin => Ok(DVec3::ZERO),
        PointRef::EdgeEnd { edge, end } => {
            let found = resolve_edge(solids, edge, tol)?;
            let solid = solids[found.body];
            let e = &solid.edges[found.edge];
            Ok(solid.vertices[if *end { e.end } else { e.start }].point)
        }
        PointRef::SketchPoint { sketch, point } => {
            let sk = computed_sketch(doc, *sketch)?;
            let xy = sk
                .point_xy(*point)
                .map_err(|_| FeatureError::InvalidInput(tr!("feature-plane-point-missing")))?;
            Ok(sk.frame().point(xy.x, xy.y, 0.0))
        }
    }
}

/// Ebene durch die Achse (`origin`, Einheitsrichtung `dir`): Normale `reference` um `dir` um
/// `angle_deg` gedreht; x-Achse entlang der Achse.
fn angle_frame(origin: DVec3, dir: DVec3, reference: DVec3, angle_deg: f64) -> Frame {
    // Die Bezugsnormale steht senkrecht zur Achse (die Achse liegt in der Bezugsebene); gegen
    // Rundung trotzdem senkrecht gestellt.
    let n0 = (reference - dir * reference.dot(dir)).normalize();
    let z = DQuat::from_axis_angle(dir, angle_deg.to_radians()) * n0;
    Frame {
        origin,
        x: dir,
        y: z.cross(dir),
        z,
    }
}

/// Mittelebene zweier Ebenen: parallel ⇒ in der Mitte mit der Lage von `a`; sonst die
/// Winkelhalbierende durch die Schnittgerade (gleicher vorzeichenbehafteter Abstand zu beiden,
/// Normale zur Seite von `a`); deckungsgleich ⇒ Fehler.
fn mid_frame(a: &Frame, b: &Frame, tol: &Tolerance) -> Result<Frame, FeatureError> {
    let d = a.z.cross(b.z);
    if d.length() < tol.angular.max(1e-9) {
        let dist = (b.origin - a.origin).dot(a.z);
        if dist.abs() <= tol.linear {
            return Err(FeatureError::InvalidInput(tr!("feature-plane-same")));
        }
        return Ok(Frame {
            origin: a.origin + a.z * (dist / 2.0),
            ..*a
        });
    }
    // Punkt der Schnittgeraden (nächster zum Weltursprung).
    let (c1, c2) = (a.z.dot(a.origin), b.z.dot(b.origin));
    let p = (b.z.cross(d) * c1 + d.cross(a.z) * c2) / d.dot(d);
    let x = d.normalize();
    let z = (a.z - b.z).normalize();
    Ok(Frame {
        origin: p,
        x,
        y: z.cross(x),
        z,
    })
}

/// Ebene durch drei Punkte; kollinear (Dreiecksfläche ≤ tol²) ⇒ Fehler.
fn three_point_frame(p: [DVec3; 3], tol: &Tolerance) -> Result<Frame, FeatureError> {
    let (u, v) = (p[1] - p[0], p[2] - p[0]);
    let n = u.cross(v);
    if n.length() / 2.0 <= tol.linear * tol.linear || u.length() <= tol.linear {
        return Err(FeatureError::InvalidInput(tr!("feature-plane-collinear")));
    }
    let x = u.normalize();
    let z = n.normalize();
    Ok(Frame {
        origin: p[0],
        x,
        y: z.cross(x),
        z,
    })
}

/// Tangentialebene an Zylinder, Kegel oder Kugel: Die Richtung `base_normal` (bei Zylinder und
/// Kegel senkrecht zur Achse gestellt) wird um `angle_deg` um die Achse der Fläche gedreht; wo
/// sie die Fläche trifft, berührt die Ebene. Normale = Flächennormale dort; x-Achse entlang der
/// Achse (Kugel: beliebig senkrecht).
fn tangent_frame(
    surface: &Surface,
    base_normal: DVec3,
    angle_deg: f64,
    tol: &Tolerance,
) -> Result<Frame, FeatureError> {
    let rotate =
        |frame: &Frame, v: DVec3| DQuat::from_axis_angle(frame.z, angle_deg.to_radians()) * v;
    let radial_of = |frame: &Frame| -> Result<DVec3, FeatureError> {
        let flat = base_normal - frame.z * base_normal.dot(frame.z);
        if flat.length() <= tol.angular.max(1e-9) {
            return Err(FeatureError::InvalidInput(tr!(
                "feature-plane-tangent-base"
            )));
        }
        Ok(rotate(frame, flat.normalize()))
    };
    let along = |z: DVec3, axis: DVec3| {
        let x = (axis - z * axis.dot(z)).normalize_or_zero();
        if x == DVec3::ZERO {
            z.any_orthonormal_vector()
        } else {
            x
        }
    };
    let (origin, z, x) = match surface {
        Surface::Cylinder { frame, radius } => {
            let r = radial_of(frame)?;
            (frame.origin + r * *radius, r, frame.z)
        }
        Surface::Cone {
            frame,
            radius,
            slope,
        } => {
            let r = radial_of(frame)?;
            let n = (r - frame.z * *slope).normalize();
            (frame.origin + r * *radius, n, along(n, frame.z))
        }
        Surface::Sphere { frame, radius } => {
            let n = rotate(frame, base_normal.normalize());
            (frame.origin + n * *radius, n, along(n, frame.z))
        }
        _ => {
            return Err(FeatureError::InvalidInput(tr!(
                "feature-plane-tangent-kind"
            )));
        }
    };
    Ok(Frame {
        origin,
        x,
        y: z.cross(x),
        z,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mid_of_parallel_planes_keeps_the_side_of_a() {
        let tol = Tolerance::default();
        let a = StandardPlane::XY.frame();
        let b = Frame {
            origin: DVec3::new(0.0, 0.0, 10.0),
            z: -DVec3::Z,
            y: -DVec3::Y,
            ..a
        };
        let m = mid_frame(&a, &b, &tol).unwrap();
        assert!((m.origin.z - 5.0).abs() < 1e-12);
        assert_eq!(m.z, DVec3::Z);
        assert!(mid_frame(&a, &a, &tol).is_err());
    }

    #[test]
    fn frames_are_orthonormal() {
        let tol = Tolerance::default();
        let f = angle_frame(DVec3::ONE, DVec3::Y, DVec3::Z, 30.0);
        assert!(f.is_orthonormal(1e-12));
        let f = three_point_frame(
            [
                DVec3::ZERO,
                DVec3::new(2.0, 0.0, 1.0),
                DVec3::new(0.0, 3.0, 0.0),
            ],
            &tol,
        )
        .unwrap();
        assert!(f.is_orthonormal(1e-12));
        let cone = Surface::Cone {
            frame: Frame::WORLD,
            radius: 5.0,
            slope: -0.5,
        };
        let f = tangent_frame(&cone, DVec3::X, 0.0, &tol).unwrap();
        assert!(f.is_orthonormal(1e-12));
        assert!((f.origin - DVec3::new(5.0, 0.0, 0.0)).length() < 1e-12);
    }
}
