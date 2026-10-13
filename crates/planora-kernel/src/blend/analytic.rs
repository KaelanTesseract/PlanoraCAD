//! Analytische Rundungs- und Fasenflächen (Spec R4, R5): Gerade Kanten zwischen Flächen, die sich
//! längs der Kante nicht ändern (Ebene; Zylinder mit Achse parallel zur Kante) ergeben einen
//! Zylinder bzw. eine Ebene; Kreiskanten zwischen Drehflächen um dieselbe Achse (Ebene senkrecht
//! zur Achse, koaxialer Zylinder, Kegel, Torus, Kugel mit Mittelpunkt auf der Achse) einen Torus
//! bzw. einen Kegel, Zylinder oder eine Ebene. Der Querschnitt wird einmal per Newton gerechnet
//! (Task 2) und dann verschoben bzw. gedreht.

use super::error::BlendError;
use super::section::{
    ChamferSpec, Section, Side, chamfer_section, check_curvature, fillet_section,
};
use super::topo::{EdgeSides, face_name, inward_directions};
use crate::boolean::Seg;
use crate::geometry::{Curve, Surface};
use crate::ids::Id;
use crate::topology::{Edge, Solid};
use planora_math::{DVec3, Frame, PI, Tolerance};

/// Winkel (Bogenmaß), bis zu dem Achsen als parallel bzw. Normalen als senkrecht zur Kante
/// gelten (wie `ANG` im analytischen Schnittrechner).
pub const COAXIAL_ANGLE: f64 = 1e-9;
/// Stützpunkte des Kreisbogens im Querschnittspolygon der Volumenschätzung.
const SECTION_ARC_SAMPLES: usize = 64;
/// Spielraum, mit dem die Kugelbahn weiter von der Drehachse entfernt sein muss als der Radius
/// (sonst durchdringt sich der Torus selbst).
const AXIS_MARGIN: f64 = 1e-9;

/// Art des Querschnitts.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SectionKind {
    Fillet { radius: f64 },
    Chamfer(ChamferSpec),
}

/// Wie der Querschnitt entlang der Kante wandert.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Sweep {
    /// Gerade Kante: Verschiebung um `dir` (Einheitsvektor in Kantenrichtung), Länge `length`.
    Translate { dir: DVec3, length: f64 },
    /// Kreiskante: Drehung um die Achse durch `center` mit Richtung `axis` um `sweep`
    /// (Bogenmaß, Vorzeichen wie der Kantenparameter); `x` zeigt von der Achse zum Kantenanfang,
    /// `axis = x × dP/dt` am Kantenanfang (rechtsdrehend in Richtung des Kantenparameters).
    Rotate {
        center: DVec3,
        axis: DVec3,
        x: DVec3,
        sweep: f64,
    },
}

/// Grundlage der Volumenschätzung einer Kante (P2).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum VolumeModel {
    /// Querschnittsfläche mal Länge der Berührkurven (gerade Kanten).
    Prism { area: f64 },
    /// Pappus: Querschnittsfläche mal Weg ihres Schwerpunkts um die Achse.
    Pappus { area: f64, centroid_radius: f64 },
    /// Schon aufsummiertes Volumen (Bahn mit Hautfläche, Task 14).
    Total { volume: f64 },
}

/// Geometrie der Verrundung einer Kante.
#[derive(Clone, Debug)]
pub struct EdgeGeom {
    pub edge: Id<Edge>,
    pub sides: EdgeSides,
    pub convex: bool,
    /// Rundungs- bzw. Fasenfläche und ihre Außenrichtung.
    pub surface: Surface,
    pub same_sense: bool,
    /// Berührkurven auf A und B (Parameter wachsen bzw. fallen wie die Kante).
    pub contact_a: Curve,
    pub contact_b: Curve,
    /// Parameter der Berührkurven am Kantenanfang und am Kantenende.
    pub range: (f64, f64),
    /// Querschnitt von A nach B am Kantenanfang und am Kantenende.
    pub section_start: Seg,
    pub section_end: Seg,
    pub volume: VolumeModel,
    /// Größter Abstand Kantenpunkt–Berührpunkt (Suchfenster der Abschlüsse).
    pub reach: f64,
    /// Geschlossene Kante (Anfang = Ende, voller Umlauf): Abschluss an der Naht.
    pub closed: bool,
    /// Analytische Fläche (`true`) oder Hautfläche (`false`, Task 14).
    pub analytic: bool,
}

/// Abstand eines Punkts zur Achse durch `c` mit Richtung `z`.
fn axis_distance(p: DVec3, c: DVec3, z: DVec3) -> f64 {
    let d = p - c;
    (d - z * d.dot(z)).length()
}

/// Bleibt die Fläche bei Verschiebung längs `dir` gleich?
fn translation_invariant(s: &Surface, dir: DVec3) -> bool {
    match s {
        Surface::Plane { frame } => frame.z.dot(dir).abs() <= COAXIAL_ANGLE,
        Surface::Cylinder { frame, .. } => frame.z.cross(dir).length() <= COAXIAL_ANGLE,
        _ => false,
    }
}

/// Ist die Fläche eine Drehfläche um die Achse (`c`, `z`)?
fn revolves_about(s: &Surface, c: DVec3, z: DVec3, tol: &Tolerance) -> bool {
    match s {
        Surface::Plane { frame } => frame.z.cross(z).length() <= COAXIAL_ANGLE,
        Surface::Cylinder { frame, .. }
        | Surface::Cone { frame, .. }
        | Surface::Torus { frame, .. } => {
            frame.z.cross(z).length() <= COAXIAL_ANGLE
                && axis_distance(frame.origin, c, z) <= tol.linear
        }
        Surface::Sphere { frame, .. } => axis_distance(frame.origin, c, z) <= tol.linear,
        Surface::Nurbs(_) => false,
    }
}

/// Analytische Klasse der Kante, sonst `None` (Freiformflächen, schräge Achsen, Kegel an
/// geraden Kanten …; diese Kanten rechnet ab Task 15 die Bahn).
pub fn classify(
    solid: &Solid,
    edge: Id<Edge>,
    sides: &EdgeSides,
    tol: &Tolerance,
) -> Option<Sweep> {
    let e = solid.edges.get(edge)?;
    let sa = &solid.faces.get(sides.a.face)?.surface;
    let sb = &solid.faces.get(sides.b.face)?.surface;
    let sign = (e.t1 - e.t0).signum();
    match &e.curve {
        Curve::Line { dir, .. } => {
            let d = *dir * sign;
            (translation_invariant(sa, d) && translation_invariant(sb, d)).then_some(
                Sweep::Translate {
                    dir: d,
                    length: (e.t1 - e.t0).abs(),
                },
            )
        }
        Curve::Circle { frame, .. } => {
            // Achse aus der Parametrisierung der Kante selbst (`x × dP/dt`), nicht aus
            // `frame.z`: So dreht ein positiver Winkel wie ein wachsender Kantenparameter, auch
            // wenn der Kreisrahmen linkshändig ist.
            let x = (e.curve.point(e.t0) - frame.origin).normalize_or_zero();
            let y = e.curve.tangent(e.t0).normalize_or_zero();
            let axis = x.cross(y).normalize_or_zero();
            if axis == DVec3::ZERO {
                return None;
            }
            (revolves_about(sa, frame.origin, axis, tol)
                && revolves_about(sb, frame.origin, axis, tol))
            .then_some(Sweep::Rotate {
                center: frame.origin,
                axis,
                x,
                sweep: e.t1 - e.t0,
            })
        }
        _ => None,
    }
}

/// Kreisbogen um `c` von `pa` nach `pb` (Öffnungswinkel unter π): Kreis mit `x` zu `pa`.
fn arc_curve(c: DVec3, pa: DVec3, pb: DVec3) -> (Curve, f64, Frame) {
    let ex = (pa - c).normalize();
    let w = pb - c;
    let ey = (w - ex * w.dot(ex)).normalize();
    let frame = Frame {
        origin: c,
        x: ex,
        y: ey,
        z: ex.cross(ey),
    };
    let angle = w.dot(ey).atan2(w.dot(ex));
    (
        Curve::Circle {
            frame,
            radius: (pa - c).length(),
        },
        angle,
        frame,
    )
}

/// Querschnitt von A nach B als Stück: Kreisbogen (Rundung) oder Strecke (Fase).
pub fn section_seg(kind: SectionKind, sec: &Section) -> Seg {
    match kind {
        SectionKind::Fillet { .. } => {
            let (curve, angle, _) = arc_curve(sec.center, sec.pa, sec.pb);
            Seg::new(curve, 0.0, angle)
        }
        SectionKind::Chamfer(_) => {
            let d = sec.pb - sec.pa;
            Seg::new(
                Curve::Line {
                    origin: sec.pa,
                    dir: d.normalize(),
                },
                0.0,
                d.length(),
            )
        }
    }
}

/// Polygon des Querschnitts zwischen Kantenpunkt `o` und Rundung bzw. Fase.
pub fn section_polygon(kind: SectionKind, o: DVec3, sec: &Section) -> Vec<DVec3> {
    let mut pts = vec![o, sec.pa];
    if let SectionKind::Fillet { .. } = kind {
        let s = section_seg(kind, sec);
        for i in 1..SECTION_ARC_SAMPLES {
            pts.push(s.at(i as f64 / SECTION_ARC_SAMPLES as f64));
        }
    }
    pts.push(sec.pb);
    pts
}

/// Fläche (≥ 0) und Schwerpunkt eines ebenen Polygons im Raum (Fächer um den ersten Punkt).
pub fn polygon_area_centroid(pts: &[DVec3]) -> (f64, DVec3) {
    let Some(&p0) = pts.first() else {
        return (0.0, DVec3::ZERO);
    };
    let mut normal = DVec3::ZERO;
    for w in pts[1..].windows(2) {
        normal += (w[0] - p0).cross(w[1] - p0);
    }
    let n = normal.normalize_or_zero();
    let (mut area, mut moment) = (0.0, DVec3::ZERO);
    for w in pts[1..].windows(2) {
        let a = 0.5 * (w[0] - p0).cross(w[1] - p0).dot(n);
        area += a;
        moment += (p0 + w[0] + w[1]) * (a / 3.0);
    }
    if area.abs() > 0.0 {
        (area.abs(), moment / area)
    } else {
        (0.0, p0)
    }
}

/// `same_sense` einer neuen Fläche aus ihrer natürlichen Normale `natural` an einem Punkt `m`
/// der Fläche: Bei konvexen Kanten zeigt die Außennormale zur alten Kante `o` hin, bei konkaven
/// von ihr weg.
pub fn outward_sense(natural: DVec3, m: DVec3, o: DVec3, convex: bool) -> bool {
    (natural.dot(o - m) > 0.0) == convex
}

/// Analytische Rundungs- bzw. Fasengeometrie; `Ok(None)`, wenn die Kante keiner analytischen
/// Klasse angehört.
pub fn analytic_geom(
    solid: &Solid,
    edge: Id<Edge>,
    sides: &EdgeSides,
    convex: bool,
    kind: SectionKind,
    tol: &Tolerance,
) -> Result<Option<EdgeGeom>, BlendError> {
    let Some(sweep) = classify(solid, edge, sides, tol) else {
        return Ok(None);
    };
    // `classify` hat Kante und Flächen schon nachgeschlagen; fehlende Ids ergeben dort `None`.
    let (Some(e), Some(fa), Some(fb)) = (
        solid.edges.get(edge),
        solid.faces.get(sides.a.face),
        solid.faces.get(sides.b.face),
    ) else {
        return Ok(None);
    };
    let a = Side {
        surface: &fa.surface,
        same_sense: fa.same_sense,
    };
    let b = Side {
        surface: &fb.surface,
        same_sense: fb.same_sense,
    };
    let o = e.curve.point(e.t0);
    let tangent = e.curve.tangent(e.t0) * (e.t1 - e.t0).signum();
    let sec = match kind {
        SectionKind::Fillet { radius } => {
            check_curvature(a, o, convex, radius, &face_name(solid, sides.a.face))?;
            check_curvature(b, o, convex, radius, &face_name(solid, sides.b.face))?;
            fillet_section(a, b, o, tangent, convex, radius, None)?
        }
        SectionKind::Chamfer(spec) => {
            let (wa, wb) = inward_directions(solid, edge, sides, e.t0);
            chamfer_section(a, b, o, tangent, convex, wa, wb, spec)?
        }
    };
    check_section(kind, o, &sec, tol)?;
    let closed = e.start == e.end;
    let geom = match sweep {
        Sweep::Translate { dir, length } => {
            translate_geom(edge, *sides, convex, kind, o, dir, length, sec)
        }
        Sweep::Rotate {
            center,
            axis,
            x,
            sweep,
        } => rotate_geom(
            solid,
            edge,
            *sides,
            convex,
            kind,
            o,
            (center, axis, x),
            sweep,
            sec,
            closed,
            tol,
        )?,
    };
    if !geom_is_finite(&geom) {
        return Err(BlendError::SpineLost);
    }
    Ok(Some(geom))
}

/// Querschnitt brauchbar? Nicht endliche Punkte (feindliche Maße wie NaN oder 1e300) ergeben
/// `SpineLost`; Berührpunkte, die mit dem Kantenpunkt oder miteinander zusammenfallen (Maß
/// unter der Längentoleranz), eine benannte Ablehnung statt eines entarteten Bogens.
#[allow(clippy::neg_cmp_op_on_partial_ord)]
fn check_section(
    kind: SectionKind,
    o: DVec3,
    sec: &Section,
    tol: &Tolerance,
) -> Result<(), BlendError> {
    if !(sec.center.is_finite() && sec.pa.is_finite() && sec.pb.is_finite()) {
        return Err(BlendError::SpineLost);
    }
    let min = tol.linear;
    let radius_ok = match kind {
        SectionKind::Fillet { radius } => radius > min && sec.pa.distance(sec.center) > min,
        SectionKind::Chamfer(_) => true,
    };
    if !(radius_ok
        && sec.pa.distance(o) > min
        && sec.pb.distance(o) > min
        && sec.pa.distance(sec.pb) > min)
    {
        return Err(BlendError::InvalidParameter(format!(
            "Das Maß ist zu klein (die Berührpunkte liegen nicht weiter als {min} mm auseinander)"
        )));
    }
    Ok(())
}

/// Alle Zahlen der Geometrie endlich (Schutz gegen Überläufe bei riesigen Maßen)?
fn geom_is_finite(g: &EdgeGeom) -> bool {
    let frame_ok =
        |f: &Frame| f.origin.is_finite() && f.x.is_finite() && f.y.is_finite() && f.z.is_finite();
    let surface_ok = match &g.surface {
        Surface::Plane { frame } => frame_ok(frame),
        Surface::Cylinder { frame, radius } | Surface::Sphere { frame, radius } => {
            frame_ok(frame) && radius.is_finite()
        }
        Surface::Cone {
            frame,
            radius,
            slope,
        } => frame_ok(frame) && radius.is_finite() && slope.is_finite(),
        Surface::Torus {
            frame,
            major,
            minor,
        } => frame_ok(frame) && major.is_finite() && minor.is_finite(),
        Surface::Nurbs(_) => true,
    };
    let curve_ok = |c: &Curve| match c {
        Curve::Line { origin, dir } => origin.is_finite() && dir.is_finite(),
        Curve::Circle { frame, radius } => frame_ok(frame) && radius.is_finite(),
        Curve::Ellipse {
            frame,
            major,
            minor,
        } => frame_ok(frame) && major.is_finite() && minor.is_finite(),
        Curve::Nurbs(_) => true,
    };
    let seg_ok = |s: &Seg| curve_ok(&s.curve) && s.t0.is_finite() && s.t1.is_finite();
    let volume_ok = match g.volume {
        VolumeModel::Prism { area } => area.is_finite(),
        VolumeModel::Pappus {
            area,
            centroid_radius,
        } => area.is_finite() && centroid_radius.is_finite(),
        VolumeModel::Total { volume } => volume.is_finite(),
    };
    surface_ok
        && curve_ok(&g.contact_a)
        && curve_ok(&g.contact_b)
        && g.range.0.is_finite()
        && g.range.1.is_finite()
        && seg_ok(&g.section_start)
        && seg_ok(&g.section_end)
        && volume_ok
        && g.reach.is_finite()
}

/// Gerade Kante: Zylinder (Rundung) bzw. Ebene (Fase) längs `dir`.
#[allow(clippy::too_many_arguments)]
fn translate_geom(
    edge: Id<Edge>,
    sides: EdgeSides,
    convex: bool,
    kind: SectionKind,
    o: DVec3,
    dir: DVec3,
    length: f64,
    sec: Section,
) -> EdgeGeom {
    let shifted = |s: &Section, d: f64| Section {
        center: s.center + dir * d,
        pa: s.pa + dir * d,
        pb: s.pb + dir * d,
    };
    let (surface, m, natural) = match kind {
        SectionKind::Fillet { radius } => {
            let (_, angle, frame) = arc_curve(sec.center, sec.pa, sec.pb);
            let m = sec.center
                + (frame.x * (angle / 2.0).cos() + frame.y * (angle / 2.0).sin()) * radius;
            (
                Surface::Cylinder { frame, radius },
                m,
                (m - sec.center) / radius,
            )
        }
        SectionKind::Chamfer(_) => {
            let y = (sec.pb - sec.pa).normalize();
            let frame = Frame {
                origin: sec.pa,
                x: dir,
                y,
                z: dir.cross(y),
            };
            (Surface::Plane { frame }, 0.5 * (sec.pa + sec.pb), frame.z)
        }
    };
    let (area, _) = polygon_area_centroid(&section_polygon(kind, o, &sec));
    EdgeGeom {
        edge,
        sides,
        convex,
        same_sense: outward_sense(natural, m, o, convex),
        surface,
        contact_a: Curve::Line {
            origin: sec.pa,
            dir,
        },
        contact_b: Curve::Line {
            origin: sec.pb,
            dir,
        },
        range: (0.0, length),
        section_start: section_seg(kind, &sec),
        section_end: section_seg(kind, &shifted(&sec, length)),
        volume: VolumeModel::Prism { area },
        reach: sec.pa.distance(o).max(sec.pb.distance(o)),
        closed: false,
        analytic: true,
    }
}

/// Kreiskante: Torus (Rundung) bzw. Kegel, Zylinder oder Ebene (Fase) um die Achse.
#[allow(clippy::too_many_arguments, clippy::neg_cmp_op_on_partial_ord)]
fn rotate_geom(
    solid: &Solid,
    edge: Id<Edge>,
    sides: EdgeSides,
    convex: bool,
    kind: SectionKind,
    o: DVec3,
    (center, axis, x): (DVec3, DVec3, DVec3),
    sweep: f64,
    sec: Section,
    closed: bool,
    tol: &Tolerance,
) -> Result<EdgeGeom, BlendError> {
    let y = axis.cross(x);
    let rho = |p: DVec3| (p - center).dot(x);
    let h = |p: DVec3| (p - center).dot(axis);
    // Punkt der Meridianebene (u = 0) um den Winkel `phi` gedreht.
    let rot = |p: DVec3, phi: f64| center + (x * phi.cos() + y * phi.sin()) * rho(p) + axis * h(p);
    for (p, face) in [(sec.pa, sides.a.face), (sec.pb, sides.b.face)] {
        if !(rho(p) > tol.linear) {
            return Err(BlendError::TooLarge {
                face: face_name(solid, face),
            });
        }
    }
    let circle = |p: DVec3| Curve::Circle {
        frame: Frame {
            origin: center + axis * h(p),
            x,
            y,
            z: axis,
        },
        radius: rho(p),
    };
    let at = |phi: f64| Section {
        center: rot(sec.center, phi),
        pa: rot(sec.pa, phi),
        pb: rot(sec.pb, phi),
    };
    let (surface, m, natural) = match kind {
        SectionKind::Fillet { radius } => {
            let rc = rho(sec.center);
            if !(rc > radius * (1.0 + AXIS_MARGIN)) {
                return Err(BlendError::Unsupported(
                    "Die Kugelbahn kommt der Drehachse näher als der Radius; bitte einen kleineren Radius wählen".into(),
                ));
            }
            let frame = Frame {
                origin: center + axis * h(sec.center),
                x,
                y,
                z: axis,
            };
            let va = (h(sec.pa) - h(sec.center)).atan2(rho(sec.pa) - rc);
            let mut vb = (h(sec.pb) - h(sec.center)).atan2(rho(sec.pb) - rc);
            vb = va + (vb - va - PI).rem_euclid(2.0 * PI) - PI;
            let vm = 0.5 * (va + vb);
            let natural = x * vm.cos() + axis * vm.sin();
            (
                Surface::Torus {
                    frame,
                    major: rc,
                    minor: radius,
                },
                sec.center + natural * radius,
                natural,
            )
        }
        SectionKind::Chamfer(_) => {
            let (ra, ha, rb, hb) = (rho(sec.pa), h(sec.pa), rho(sec.pb), h(sec.pb));
            let surface = if (ha - hb).abs() <= tol.linear {
                Surface::Plane {
                    frame: Frame {
                        origin: center + axis * ha,
                        x,
                        y,
                        z: axis,
                    },
                }
            } else if (ra - rb).abs() <= tol.linear {
                Surface::Cylinder {
                    frame: Frame {
                        origin: center,
                        x,
                        y,
                        z: axis,
                    },
                    radius: ra,
                }
            } else {
                Surface::Cone {
                    frame: Frame {
                        origin: center + axis * ha,
                        x,
                        y,
                        z: axis,
                    },
                    radius: ra,
                    slope: (rb - ra) / (hb - ha),
                }
            };
            let m = 0.5 * (sec.pa + sec.pb);
            let (u, v) = surface.project(m);
            let natural = surface.normal(u, v);
            (surface, m, natural)
        }
    };
    let (area, centroid) = polygon_area_centroid(&section_polygon(kind, o, &sec));
    Ok(EdgeGeom {
        edge,
        sides,
        convex,
        same_sense: outward_sense(natural, m, o, convex),
        surface,
        contact_a: circle(sec.pa),
        contact_b: circle(sec.pb),
        range: (0.0, sweep),
        section_start: section_seg(kind, &sec),
        section_end: section_seg(kind, &at(sweep)),
        volume: VolumeModel::Pappus {
            area,
            centroid_radius: rho(centroid),
        },
        reach: sec.pa.distance(o).max(sec.pb.distance(o)),
        closed,
        analytic: true,
    })
}
