//! Umwandlung zwischen Snapshot und den Format-Typen. `encode_version` schreibt nur, was sich
//! wieder lesen lässt; `decode_version` prüft alles, bevor etwas entsteht.

use crate::base64;
use crate::error::{ProjectError, Result};
use crate::format::*;
use planora_exchange::{ExchangeMesh, MAX_FILE_BYTES};
use planora_feature::{
    BlendArt, BodyRef, EdgeCopy, EdgeRef, FaceRef, FeatureId, FeatureKind, MAX_COORD_MM, MeshId,
    SketchFeature, SketchPlane, Snapshot, SnapshotFeature, StandardPlane,
};
use planora_kernel::BoolOp;
use planora_math::{DQuat, DVec3, Frame, Tolerance};
use planora_sketch::{
    ConstraintKind, ConstraintRecord, CurveId, CurveKind, CurveRecord, LineOrPoints, PointId,
    PointRecord, Sketch, SketchData,
};
use std::collections::BTreeMap;
use std::sync::Arc;

/// Höchste lesbare Formatnummer der Versionsdateien (P7, P12).
pub const FORMAT_VERSION: u32 = 3;
/// Formatnummer der Rahmendateien `dokument.json` und `.autosave.json`; ihre Struktur ist seit
/// Format 1 unverändert, ältere Programme öffnen das Projekt deshalb weiter (P7).
pub const CONTAINER_FORMAT: u32 = 1;
/// Größte Summe der dekodierten Netzdaten eines Dokuments (wie die Dateigrenze beim Import).
const MAX_MESH_BYTES: u64 = MAX_FILE_BYTES;

/// Kleinste Formatnummer, die den Stand darstellt: 3 mit Netz-Körpern, 2 mit Rundungen oder
/// Fasen, sonst 1 (P7, P12).
pub fn required_format(s: &Snapshot) -> u32 {
    let has = |f: fn(&FeatureKind) -> bool| s.features.iter().any(|x| f(&x.kind));
    if has(|k| matches!(k, FeatureKind::MeshBody { .. })) {
        3
    } else if has(|k| matches!(k, FeatureKind::Verrunden { .. })) {
        2
    } else {
        1
    }
}
/// Längste erlaubte Liste (Features, Punkte, Kurven, Abhängigkeiten, Werte) in einer Datei.
const MAX_LIST: usize = 1_000_000;

fn v3(v: DVec3) -> V3 {
    [v.x, v.y, v.z]
}

/// Prueft eine Zahl aus der Datei: endlich und im Betrag hoechstens `2 * MAX_COORD_MM`. Der
/// Faktor 2 laesst Ursprung plus Groesse (z. B. ein Quader am Rand des Raums) zu, waehrend
/// Werte weit ueber dem Rechenbereich abgelehnt werden.
fn num(origin: &str, what: &str, x: f64) -> Result<f64> {
    if x.is_finite() && x.abs() <= MAX_COORD_MM * 2.0 {
        Ok(x)
    } else {
        Err(ProjectError::corrupt(
            origin,
            format!("{what}: Zahl ungültig oder zu groß"),
        ))
    }
}

fn dv3(origin: &str, what: &str, v: V3) -> Result<DVec3> {
    Ok(DVec3::new(
        num(origin, what, v[0])?,
        num(origin, what, v[1])?,
        num(origin, what, v[2])?,
    ))
}

fn check_len(origin: &str, what: &str, n: usize) -> Result<()> {
    if n > MAX_LIST {
        Err(ProjectError::corrupt(
            origin,
            format!("{what}: zu viele Einträge"),
        ))
    } else {
        Ok(())
    }
}

// ---- Verweise ---------------------------------------------------------------------------

fn face_to(r: &FaceRef) -> FaceRefDto {
    FaceRefDto {
        herkunft: r.origin,
        rolle: r.role.clone(),
        hinweis: v3(r.hint),
    }
}

fn face_from(origin: &str, name: &str, d: &FaceRefDto) -> Result<FaceRef> {
    Ok(FaceRef {
        origin: d.herkunft,
        role: d.rolle.clone(),
        hint: dv3(origin, &format!("„{name}“ Hinweispunkt"), d.hinweis)?,
    })
}

fn edge_to(r: &EdgeRef) -> EdgeRefDto {
    EdgeRefDto {
        a: face_to(&r.a),
        b: face_to(&r.b),
        hinweis: v3(r.hint),
    }
}

fn edge_from(origin: &str, name: &str, d: &EdgeRefDto) -> Result<EdgeRef> {
    Ok(EdgeRef {
        a: face_from(origin, name, &d.a)?,
        b: face_from(origin, name, &d.b)?,
        hint: dv3(origin, &format!("„{name}“ Hinweispunkt"), d.hinweis)?,
    })
}

fn body_to(b: &BodyRef) -> BodyRefDto {
    BodyRefDto {
        feature: b.feature.0,
        index: b.index,
    }
}

fn body_from(d: &BodyRefDto) -> BodyRef {
    BodyRef {
        feature: FeatureId(d.feature),
        index: d.index,
    }
}

// ---- Skizze -----------------------------------------------------------------------------

fn frame_to(f: &Frame) -> FrameDto {
    FrameDto {
        ursprung: v3(f.origin),
        x: v3(f.x),
        y: v3(f.y),
        z: v3(f.z),
    }
}

fn lop_to(l: &LineOrPoints) -> LineOrPointsDto {
    match l {
        LineOrPoints::Line(c) => LineOrPointsDto::Linie { kurve: c.index() },
        LineOrPoints::Points(a, b) => LineOrPointsDto::Punkte {
            a: a.index(),
            b: b.index(),
        },
    }
}

fn lop_from(d: &LineOrPointsDto) -> LineOrPoints {
    match d {
        LineOrPointsDto::Linie { kurve } => LineOrPoints::Line(CurveId::from_index(*kurve)),
        LineOrPointsDto::Punkte { a, b } => {
            LineOrPoints::Points(PointId::from_index(*a), PointId::from_index(*b))
        }
    }
}

fn curve_to(k: &CurveKind) -> CurveKindDto {
    match k {
        CurveKind::Line { a, b } => CurveKindDto::Linie {
            a: a.index(),
            b: b.index(),
        },
        CurveKind::Circle { center, radius } => CurveKindDto::Kreis {
            mitte: center.index(),
            radius: *radius,
        },
        CurveKind::Arc { center, start, end } => CurveKindDto::Bogen {
            mitte: center.index(),
            start: start.index(),
            ende: end.index(),
        },
        CurveKind::Ellipse {
            center,
            ax,
            ay,
            minor,
        } => CurveKindDto::Ellipse {
            mitte: center.index(),
            ax: *ax,
            ay: *ay,
            klein: *minor,
        },
        CurveKind::Spline { points } => CurveKindDto::Spline {
            punkte: points.iter().map(|p| p.index()).collect(),
        },
    }
}

fn curve_from(d: &CurveKindDto) -> CurveKind {
    let p = PointId::from_index;
    match d {
        CurveKindDto::Linie { a, b } => CurveKind::Line { a: p(*a), b: p(*b) },
        CurveKindDto::Kreis { mitte, radius } => CurveKind::Circle {
            center: p(*mitte),
            radius: *radius,
        },
        CurveKindDto::Bogen { mitte, start, ende } => CurveKind::Arc {
            center: p(*mitte),
            start: p(*start),
            end: p(*ende),
        },
        CurveKindDto::Ellipse {
            mitte,
            ax,
            ay,
            klein,
        } => CurveKind::Ellipse {
            center: p(*mitte),
            ax: *ax,
            ay: *ay,
            minor: *klein,
        },
        CurveKindDto::Spline { punkte } => CurveKind::Spline {
            points: punkte.iter().map(|i| p(*i)).collect(),
        },
    }
}

fn constraint_to(k: &ConstraintKind) -> ConstraintKindDto {
    use ConstraintKind as K;
    use ConstraintKindDto as D;
    let (p, c) = (|x: &PointId| x.index(), |x: &CurveId| x.index());
    match k {
        K::Coincident(a, b) => D::Deckungsgleich { a: p(a), b: p(b) },
        K::PointOnCurve(a, k) => D::PunktAufKurve {
            punkt: p(a),
            kurve: c(k),
        },
        K::Horizontal(l) => D::Waagerecht { ziel: lop_to(l) },
        K::Vertical(l) => D::Senkrecht { ziel: lop_to(l) },
        K::Parallel(a, b) => D::Parallel { a: c(a), b: c(b) },
        K::Perpendicular(a, b) => D::Rechtwinklig { a: c(a), b: c(b) },
        K::Tangent(a, b) => D::Tangential { a: c(a), b: c(b) },
        K::Equal(a, b) => D::Gleich { a: c(a), b: c(b) },
        K::Fixed(a, x, y) => D::Fest {
            punkt: p(a),
            x: *x,
            y: *y,
        },
        K::Symmetric(a, b, axis) => D::Symmetrisch {
            a: p(a),
            b: p(b),
            achse: c(axis),
        },
        K::Midpoint(a, k) => D::Mittelpunkt {
            punkt: p(a),
            kurve: c(k),
        },
        K::Distance(a, b, v) => D::Abstand {
            a: p(a),
            b: p(b),
            wert: *v,
        },
        K::DistancePointLine(a, k, v) => D::AbstandPunktLinie {
            punkt: p(a),
            kurve: c(k),
            wert: *v,
        },
        K::Radius(k, v) => D::Radius {
            kurve: c(k),
            wert: *v,
        },
        K::Diameter(k, v) => D::Durchmesser {
            kurve: c(k),
            wert: *v,
        },
        K::Angle(a, b, v) => D::Winkel {
            a: c(a),
            b: c(b),
            wert: *v,
        },
        K::ArcRadius(k) => D::BogenRadius { kurve: c(k) },
        K::EllipseMajor(k, v) => D::EllipseGross {
            kurve: c(k),
            wert: *v,
        },
        K::EllipseMinor(k, v) => D::EllipseKlein {
            kurve: c(k),
            wert: *v,
        },
        K::EllipseAngle(k, v) => D::EllipseWinkel {
            kurve: c(k),
            wert: *v,
        },
    }
}

fn constraint_from(d: &ConstraintKindDto) -> ConstraintKind {
    use ConstraintKind as K;
    use ConstraintKindDto as D;
    let (p, c) = (PointId::from_index, CurveId::from_index);
    match d {
        D::Deckungsgleich { a, b } => K::Coincident(p(*a), p(*b)),
        D::PunktAufKurve { punkt, kurve } => K::PointOnCurve(p(*punkt), c(*kurve)),
        D::Waagerecht { ziel } => K::Horizontal(lop_from(ziel)),
        D::Senkrecht { ziel } => K::Vertical(lop_from(ziel)),
        D::Parallel { a, b } => K::Parallel(c(*a), c(*b)),
        D::Rechtwinklig { a, b } => K::Perpendicular(c(*a), c(*b)),
        D::Tangential { a, b } => K::Tangent(c(*a), c(*b)),
        D::Gleich { a, b } => K::Equal(c(*a), c(*b)),
        D::Fest { punkt, x, y } => K::Fixed(p(*punkt), *x, *y),
        D::Symmetrisch { a, b, achse } => K::Symmetric(p(*a), p(*b), c(*achse)),
        D::Mittelpunkt { punkt, kurve } => K::Midpoint(p(*punkt), c(*kurve)),
        D::Abstand { a, b, wert } => K::Distance(p(*a), p(*b), *wert),
        D::AbstandPunktLinie { punkt, kurve, wert } => {
            K::DistancePointLine(p(*punkt), c(*kurve), *wert)
        }
        D::Radius { kurve, wert } => K::Radius(c(*kurve), *wert),
        D::Durchmesser { kurve, wert } => K::Diameter(c(*kurve), *wert),
        D::Winkel { a, b, wert } => K::Angle(c(*a), c(*b), *wert),
        D::BogenRadius { kurve } => K::ArcRadius(c(*kurve)),
        D::EllipseGross { kurve, wert } => K::EllipseMajor(c(*kurve), *wert),
        D::EllipseKlein { kurve, wert } => K::EllipseMinor(c(*kurve), *wert),
        D::EllipseWinkel { kurve, wert } => K::EllipseAngle(c(*kurve), *wert),
    }
}

fn sketch_to(s: &Sketch) -> SketchDto {
    let d = s.to_data();
    SketchDto {
        name: d.name,
        ebene: frame_to(&d.frame),
        linear: d.tolerance.linear,
        winkel: d.tolerance.angular,
        werte: d.vars,
        punkte: d
            .points
            .iter()
            .map(|p| PointDto {
                x: p.x,
                y: p.y,
                konstruktion: p.construction,
                aktiv: p.alive,
            })
            .collect(),
        kurven: d
            .curves
            .iter()
            .map(|c| CurveDto {
                art: curve_to(&c.kind),
                konstruktion: c.construction,
                aktiv: c.alive,
            })
            .collect(),
        abhaengigkeiten: d
            .constraints
            .iter()
            .map(|c| ConstraintDto {
                art: constraint_to(&c.kind),
                zweig: c.branch,
                gesteuert: c.driven,
                aktiv: c.enabled,
                intern: c.internal,
                extra: c.extra,
                lebt: c.alive,
            })
            .collect(),
    }
}

fn sketch_from(origin: &str, feature: &str, d: &SketchDto) -> Result<Sketch> {
    let ctx = |m: String| ProjectError::corrupt(origin, format!("Skizze „{feature}“: {m}"));
    check_len(origin, "Werte", d.werte.len())?;
    check_len(origin, "Punkte", d.punkte.len())?;
    check_len(origin, "Kurven", d.kurven.len())?;
    check_len(origin, "Abhängigkeiten", d.abhaengigkeiten.len())?;
    let data = SketchData {
        name: d.name.clone(),
        frame: Frame {
            origin: dv3(
                origin,
                &format!("Skizze „{feature}“ Ursprung"),
                d.ebene.ursprung,
            )?,
            x: dv3(origin, &format!("Skizze „{feature}“ x-Achse"), d.ebene.x)?,
            y: dv3(origin, &format!("Skizze „{feature}“ y-Achse"), d.ebene.y)?,
            z: dv3(origin, &format!("Skizze „{feature}“ z-Achse"), d.ebene.z)?,
        },
        tolerance: Tolerance {
            linear: d.linear,
            angular: d.winkel,
        },
        vars: d.werte.clone(),
        points: d
            .punkte
            .iter()
            .map(|p| PointRecord {
                x: p.x,
                y: p.y,
                construction: p.konstruktion,
                alive: p.aktiv,
            })
            .collect(),
        curves: d
            .kurven
            .iter()
            .map(|c| CurveRecord {
                kind: curve_from(&c.art),
                construction: c.konstruktion,
                alive: c.aktiv,
            })
            .collect(),
        constraints: d
            .abhaengigkeiten
            .iter()
            .map(|c| ConstraintRecord {
                kind: constraint_from(&c.art),
                branch: c.zweig,
                driven: c.gesteuert,
                enabled: c.aktiv,
                internal: c.intern,
                extra: c.extra,
                alive: c.lebt,
            })
            .collect(),
    };
    Sketch::from_data(data).map_err(|e| ctx(e.to_string()))
}

// ---- Feature-Arten ----------------------------------------------------------------------

fn plane_name(p: StandardPlane) -> &'static str {
    match p {
        StandardPlane::XY => "xy",
        StandardPlane::XZ => "xz",
        StandardPlane::YZ => "yz",
    }
}

fn kind_to(k: &FeatureKind) -> KindDto {
    match k {
        FeatureKind::Sketch(s) => KindDto::Skizze(Box::new(SketchFeatureDto {
            skizze: sketch_to(&s.sketch),
            lage: match &s.plane {
                SketchPlane::Standard(p) => PlaneDto::Standard {
                    ebene: plane_name(*p).into(),
                },
                SketchPlane::OnFace(r) => PlaneDto::Flaeche {
                    flaeche: face_to(r),
                },
            },
            kopien: s
                .copies
                .iter()
                .map(|c| EdgeCopyDto {
                    kurve: c.curve.index(),
                    kante: edge_to(&c.edge),
                })
                .collect(),
        })),
        FeatureKind::Box { size, origin } => KindDto::Quader {
            groesse: v3(*size),
            ursprung: v3(*origin),
        },
        FeatureKind::Cylinder {
            radius,
            height,
            origin,
        } => KindDto::Zylinder {
            radius: *radius,
            hoehe: *height,
            ursprung: v3(*origin),
        },
        FeatureKind::Sphere { radius, origin } => KindDto::Kugel {
            radius: *radius,
            ursprung: v3(*origin),
        },
        FeatureKind::Cone {
            radius,
            height,
            origin,
        } => KindDto::Kegel {
            radius: *radius,
            hoehe: *height,
            ursprung: v3(*origin),
        },
        FeatureKind::Extrude {
            sketch,
            profile,
            height,
        } => KindDto::Extrusion {
            skizze: sketch.0,
            profil: *profile,
            hoehe: *height,
        },
        FeatureKind::Revolve {
            sketch,
            profile,
            axis,
        } => KindDto::Rotation {
            skizze: sketch.0,
            profil: *profile,
            achse: axis.index(),
        },
        FeatureKind::Boolean { target, tool, op } => KindDto::Boolesch {
            ziel: body_to(target),
            werkzeug: body_to(tool),
            operation: match op {
                BoolOp::Union => "vereinigen",
                BoolOp::Difference => "abziehen",
                BoolOp::Intersection => "schneiden",
            }
            .into(),
        },
        FeatureKind::Move { body, delta } => KindDto::Verschieben {
            koerper: body_to(body),
            versatz: v3(*delta),
        },
        FeatureKind::Verrunden {
            body,
            edges,
            art,
            tangentenkette,
        } => KindDto::Verrunden {
            koerper: body_to(body),
            kanten: edges.iter().map(edge_to).collect(),
            art: art_to(art),
            tangentenkette: *tangentenkette,
        },
        FeatureKind::MeshBody {
            mesh,
            offset,
            rotation,
            scale,
        } => KindDto::NetzKoerper {
            netz: mesh.0,
            versatz: v3(*offset),
            drehung: rotation.to_array(),
            massstab: *scale,
        },
    }
}

fn art_to(a: &BlendArt) -> BlendArtDto {
    match a {
        BlendArt::Rundung { radius } => BlendArtDto::Rundung { radius: *radius },
        BlendArt::RundungVariabel { punkte } => BlendArtDto::RundungVariabel {
            punkte: punkte.iter().map(|(p, r)| [*p, *r]).collect(),
        },
        BlendArt::FaseGleich { abstand } => BlendArtDto::FaseGleich { abstand: *abstand },
        BlendArt::FaseZwei { abstand1, abstand2 } => BlendArtDto::FaseZwei {
            abstand1: *abstand1,
            abstand2: *abstand2,
        },
        BlendArt::FaseWinkel { abstand, winkel } => BlendArtDto::FaseWinkel {
            abstand: *abstand,
            winkel: *winkel,
        },
    }
}

fn art_from(origin: &str, name: &str, d: &BlendArtDto) -> Result<BlendArt> {
    let n = |what: &str, x: f64| num(origin, &format!("„{name}“ {what}"), x);
    Ok(match d {
        BlendArtDto::Rundung { radius } => BlendArt::Rundung {
            radius: n("Radius", *radius)?,
        },
        BlendArtDto::RundungVariabel { punkte } => {
            check_len(origin, "Radienpunkte", punkte.len())?;
            BlendArt::RundungVariabel {
                punkte: punkte
                    .iter()
                    .map(|[p, r]| Ok((n("Position", *p)?, n("Radius", *r)?)))
                    .collect::<Result<Vec<_>>>()?,
            }
        }
        BlendArtDto::FaseGleich { abstand } => BlendArt::FaseGleich {
            abstand: n("Abstand", *abstand)?,
        },
        BlendArtDto::FaseZwei { abstand1, abstand2 } => BlendArt::FaseZwei {
            abstand1: n("Abstand 1", *abstand1)?,
            abstand2: n("Abstand 2", *abstand2)?,
        },
        BlendArtDto::FaseWinkel { abstand, winkel } => BlendArt::FaseWinkel {
            abstand: n("Abstand", *abstand)?,
            winkel: n("Winkel", *winkel)?,
        },
    })
}

fn kind_from(origin: &str, name: &str, d: &KindDto) -> Result<FeatureKind> {
    let n = |what: &str, x: f64| num(origin, &format!("„{name}“ {what}"), x);
    let v = |what: &str, x: V3| dv3(origin, &format!("„{name}“ {what}"), x);
    Ok(match d {
        KindDto::Skizze(s) => {
            check_len(origin, "Kantenkopien", s.kopien.len())?;
            FeatureKind::Sketch(SketchFeature {
                sketch: sketch_from(origin, name, &s.skizze)?,
                plane: match &s.lage {
                    PlaneDto::Standard { ebene } => SketchPlane::Standard(match ebene.as_str() {
                        "xy" => StandardPlane::XY,
                        "xz" => StandardPlane::XZ,
                        "yz" => StandardPlane::YZ,
                        other => {
                            return Err(ProjectError::corrupt(
                                origin,
                                format!("„{name}“: Unbekannte Ebene „{other}“"),
                            ));
                        }
                    }),
                    PlaneDto::Flaeche { flaeche } => {
                        SketchPlane::OnFace(face_from(origin, name, flaeche)?)
                    }
                },
                copies: s
                    .kopien
                    .iter()
                    .map(|c| {
                        Ok(EdgeCopy {
                            curve: CurveId::from_index(c.kurve),
                            edge: edge_from(origin, name, &c.kante)?,
                        })
                    })
                    .collect::<Result<Vec<_>>>()?,
            })
        }
        KindDto::Quader { groesse, ursprung } => FeatureKind::Box {
            size: v("Größe", *groesse)?,
            origin: v("Ursprung", *ursprung)?,
        },
        KindDto::Zylinder {
            radius,
            hoehe,
            ursprung,
        } => FeatureKind::Cylinder {
            radius: n("Radius", *radius)?,
            height: n("Höhe", *hoehe)?,
            origin: v("Ursprung", *ursprung)?,
        },
        KindDto::Kugel { radius, ursprung } => FeatureKind::Sphere {
            radius: n("Radius", *radius)?,
            origin: v("Ursprung", *ursprung)?,
        },
        KindDto::Kegel {
            radius,
            hoehe,
            ursprung,
        } => FeatureKind::Cone {
            radius: n("Radius", *radius)?,
            height: n("Höhe", *hoehe)?,
            origin: v("Ursprung", *ursprung)?,
        },
        KindDto::Extrusion {
            skizze,
            profil,
            hoehe,
        } => FeatureKind::Extrude {
            sketch: FeatureId(*skizze),
            profile: *profil,
            height: n("Höhe", *hoehe)?,
        },
        KindDto::Rotation {
            skizze,
            profil,
            achse,
        } => FeatureKind::Revolve {
            sketch: FeatureId(*skizze),
            profile: *profil,
            axis: CurveId::from_index(*achse),
        },
        KindDto::Boolesch {
            ziel,
            werkzeug,
            operation,
        } => FeatureKind::Boolean {
            target: body_from(ziel),
            tool: body_from(werkzeug),
            op: match operation.as_str() {
                "vereinigen" => BoolOp::Union,
                "abziehen" => BoolOp::Difference,
                "schneiden" => BoolOp::Intersection,
                other => {
                    return Err(ProjectError::corrupt(
                        origin,
                        format!("„{name}“: Unbekannte Operation „{other}“"),
                    ));
                }
            },
        },
        KindDto::Verschieben { koerper, versatz } => FeatureKind::Move {
            body: body_from(koerper),
            delta: v("Versatz", *versatz)?,
        },
        KindDto::Verrunden {
            koerper,
            kanten,
            art,
            tangentenkette,
        } => {
            check_len(origin, "Kanten", kanten.len())?;
            if kanten.is_empty() {
                return Err(ProjectError::corrupt(
                    origin,
                    format!("„{name}“: keine Kante gewählt"),
                ));
            }
            FeatureKind::Verrunden {
                body: body_from(koerper),
                edges: kanten
                    .iter()
                    .map(|k| edge_from(origin, name, k))
                    .collect::<Result<Vec<_>>>()?,
                art: art_from(origin, name, art)?,
                tangentenkette: *tangentenkette,
            }
        }
        KindDto::NetzKoerper {
            netz,
            versatz,
            drehung,
            massstab,
        } => {
            let [x, y, z, w] = *drehung;
            FeatureKind::MeshBody {
                mesh: MeshId(*netz),
                offset: v("Position", *versatz)?,
                rotation: DQuat::from_xyzw(
                    n("Drehung", x)?,
                    n("Drehung", y)?,
                    n("Drehung", z)?,
                    n("Drehung", w)?,
                ),
                scale: n("Maßstab", *massstab)?,
            }
        }
    })
}

// ---- Netze ------------------------------------------------------------------------------

/// Liest die Netze einer Datei. Vor dem Dekodieren werden Länge der Zeichenkette (auch in
/// Summe) und der Kopf gegen die Grenzen und gegen die Gesamtlänge geprüft (Spec §7).
fn meshes_from(origin: &str, list: &[MeshDto]) -> Result<BTreeMap<MeshId, Arc<ExchangeMesh>>> {
    let mut total: u64 = 0;
    let mut out = BTreeMap::new();
    for d in list {
        let bad = |m: &str| ProjectError::corrupt(origin, format!("Netz {}: {m}", d.id));
        if d.id == 0 {
            return Err(bad("Die Kennung ist ungültig"));
        }
        let len = base64::decoded_len(&d.daten).map_err(|e| bad(&e.to_string()))? as u64;
        total = total.saturating_add(len);
        if len > MAX_MESH_BYTES || total > MAX_MESH_BYTES {
            return Err(bad("Die Netzdaten sind zu groß"));
        }
        let head = base64::decode_prefix(&d.daten, 8).map_err(|e| bad(&e.to_string()))?;
        let expected =
            ExchangeMesh::byte_len_from_header(&head).map_err(|e| bad(&e.to_string()))?;
        if expected != len {
            return Err(bad("Die Länge der Netzdaten passt nicht zu den Anzahlen"));
        }
        let bytes = base64::decode(&d.daten).map_err(|e| bad(&e.to_string()))?;
        let mesh = ExchangeMesh::from_bytes(&bytes).map_err(|e| bad(&e.to_string()))?;
        if out.insert(MeshId(d.id), Arc::new(mesh)).is_some() {
            return Err(bad("Die Kennung kommt doppelt vor"));
        }
    }
    Ok(out)
}

// ---- Öffentliche Funktionen -------------------------------------------------------------

/// JSON-Text einer Versionsdatei. Enthält der Snapshot eine nicht endliche Zahl, ist das ein
/// Fehler (das Dokument wird dann nicht geschrieben).
pub fn encode_version(s: &Snapshot) -> Result<String> {
    let file = VersionFile {
        format: required_format(s),
        marker: s.marker,
        naechste_id: s.next_id,
        features: s
            .features
            .iter()
            .map(|f| FeatureDto {
                id: f.id.0,
                name: f.name.clone(),
                unterdrueckt: f.suppressed,
                art: kind_to(&f.kind),
            })
            .collect(),
        netze: s
            .meshes
            .iter()
            .map(|(id, m)| MeshDto {
                id: id.0,
                daten: base64::encode(&m.to_bytes()),
            })
            .collect(),
    };
    let text = serde_json::to_string_pretty(&file).map_err(|e| {
        ProjectError::Invalid(format!("Das Dokument lässt sich nicht schreiben: {e}"))
    })?;
    // Nur schreiben, was sich wieder lesen lässt (nicht endliche Zahlen schreibt serde_json
    // als `null`; das fällt beim Lesen auf).
    decode_version(&text, "Ausgabe").map_err(|e| {
        ProjectError::Invalid(format!(
            "Das Dokument enthält ungültige Werte und wird nicht gespeichert: {e}"
        ))
    })?;
    Ok(text)
}

/// Liest eine Versionsdatei. `origin` ist der Dateiname für Meldungen.
pub fn decode_version(text: &str, origin: &str) -> Result<Snapshot> {
    // Erst nur das Format lesen, damit eine neuere Datei auch bei geänderter Struktur die
    // richtige Meldung bekommt.
    #[derive(serde::Deserialize)]
    struct Head {
        format: u32,
    }
    // Ein vorangestelltes UTF-8-Byte-Order-Mark ist harmlos und wird uebergangen.
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let head: Head =
        serde_json::from_str(text).map_err(|e| ProjectError::corrupt(origin, e.to_string()))?;
    if head.format == 0 {
        return Err(ProjectError::corrupt(origin, "Ungültige Formatnummer 0"));
    }
    if head.format > FORMAT_VERSION {
        return Err(ProjectError::TooNew {
            path: origin.into(),
            found: head.format,
        });
    }
    let file: VersionFile =
        serde_json::from_str(text).map_err(|e| ProjectError::corrupt(origin, e.to_string()))?;
    if file.format < 2
        && file
            .features
            .iter()
            .any(|f| matches!(f.art, KindDto::Verrunden { .. }))
    {
        return Err(ProjectError::corrupt(
            origin,
            "Rundungen und Fasen gibt es erst ab Dateiformat 2",
        ));
    }
    let has_mesh_body = file
        .features
        .iter()
        .any(|f| matches!(f.art, KindDto::NetzKoerper { .. }));
    if file.format < 3 && (has_mesh_body || !file.netze.is_empty()) {
        return Err(ProjectError::corrupt(
            origin,
            "Netz-Körper gibt es erst ab Dateiformat 3",
        ));
    }
    check_len(origin, "Netze", file.netze.len())?;
    let meshes = meshes_from(origin, &file.netze)?;
    for f in &file.features {
        if let KindDto::NetzKoerper { netz, .. } = &f.art
            && !meshes.contains_key(&MeshId(*netz))
        {
            return Err(ProjectError::corrupt(
                origin,
                format!("„{}“: Das Netz {netz} fehlt in der Datei", f.name),
            ));
        }
    }
    check_len(origin, "Features", file.features.len())?;
    let mut features = Vec::with_capacity(file.features.len());
    for f in &file.features {
        features.push(SnapshotFeature {
            id: FeatureId(f.id),
            name: f.name.clone(),
            suppressed: f.unterdrueckt,
            kind: kind_from(origin, &f.name, &f.art)?,
        });
    }
    Ok(Snapshot {
        features,
        marker: file.marker,
        next_id: file.naechste_id,
        meshes,
        drawings: vec![],
        next_drawing: 1,
    })
}
